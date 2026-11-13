//! Reviewers are told when the workspace they review moves on.
//!
//! A review copy is forked from its source workspace's branch tip, and every
//! round a reviewer pushes is measured against the source head it reviewed.
//! When that head moves (the owner committed from the Reviews page, pushed
//! natively, pulled a remote-tracking member, merged a pull request, or the
//! branch-integration worker landed a remote import), the rounds pushed
//! before it are outdated on the file pages and the copy wants an Update.
//!
//! The notice is filed in the reviewer's own copy, where a reviewer who was
//! away reads it on reopening; it names the source's activity, so a reviewer
//! who can no longer open the source is told nothing more about it.
//!
//! Every place that moves a source workspace's branch tip goes through one
//! of the two entry points here, whether or not it runs inside a request:
//! the routes wrap them with [`AppState`], the maintenance worker builds a
//! notifier from its own dependencies.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::db::scoped::{scoped_db_for_system_org_with, user_can_access_workspace};
use crate::errors::AppError;
use crate::services::branch_commit::{LandedBranchCommit, LandedCommitNotifier};
use crate::services::notification_service::{CreateNotificationRequest, NotificationService};
use crate::services::workspace_review_service::{self as reviews, WorkspaceReviewRecord};
use crate::services::{AppState, EventBus};
use crate::ws::events::NotificationAction;

/// One branch head of a review source that just moved.
#[derive(Debug, Clone)]
pub struct SourceHeadMove {
    pub branch: String,
    pub new_head: String,
}

/// Tells the reviewers of a workspace that its branch moved. Cheap to build;
/// it opens its own system-scoped transaction per call.
pub struct ReviewSourceNotifier {
    system_pool: PgPool,
    tenant_pool: PgPool,
    authz: Arc<dyn g5_authz::AuthzService>,
    notifications: NotificationService,
}

impl ReviewSourceNotifier {
    #[must_use]
    pub fn new(
        system_pool: PgPool,
        tenant_pool: PgPool,
        authz: Arc<dyn g5_authz::AuthzService>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            notifications: NotificationService::new(event_bus, system_pool.clone()),
            system_pool,
            tenant_pool,
            authz,
        }
    }

    #[must_use]
    pub fn from_state(state: &AppState) -> Self {
        Self::new(
            state.system_pool.clone(),
            state.tenant_pool.clone(),
            state.authz.clone(),
            state.event_bus.clone(),
        )
    }

    /// Tell every reviewer with a live review copy of `source_workspace_id`
    /// that its branch moved: one notice per reviewer per call, filed in
    /// their copy; the actor gets none. A notification problem never fails
    /// the caller (the push or landing already happened): it is logged.
    pub async fn notify_reviewers_of_source_head(
        &self,
        org_id: Uuid,
        source_workspace_id: Uuid,
        moves: &[SourceHeadMove],
        actor_user_id: Uuid,
    ) {
        if moves.is_empty() {
            return;
        }
        if let Err(error) = self
            .notify_inner(org_id, source_workspace_id, moves, actor_user_id)
            .await
        {
            tracing::warn!(
                workspace_id = %source_workspace_id,
                error = %error,
                "reviewers were not told that the workspace has a new commit"
            );
        }
    }

    /// [`Self::notify_reviewers_of_source_head`] for a cut the server landed
    /// (a pull, a merge, a pull request, a remote import): every member
    /// whose branch ref moved counts. The integration worker reaches it as a
    /// [`LandedCommitNotifier`].
    pub async fn notify_reviewers_after_landing(
        &self,
        landed: &LandedBranchCommit,
        actor_user_id: Uuid,
    ) {
        let moves: Vec<SourceHeadMove> = landed
            .changed_members()
            .map(|member| SourceHeadMove {
                branch: member.branch_name.clone(),
                new_head: member.proposed_new_head.clone(),
            })
            .collect();
        self.notify_reviewers_of_source_head(
            landed.org_id,
            landed.workspace_id,
            &moves,
            actor_user_id,
        )
        .await;
    }

    async fn notify_inner(
        &self,
        org_id: Uuid,
        source_workspace_id: Uuid,
        moves: &[SourceHeadMove],
        actor_user_id: Uuid,
    ) -> Result<(), AppError> {
        let mut db = scoped_db_for_system_org_with(
            self.authz.as_ref(),
            &self.tenant_pool,
            org_id,
            &["workspaces:read", "workspaces:write"],
        )
        .await?;
        // A review copy's own branch moving is the reviewer saving a round,
        // not a base that anyone reviews against.
        if reviews::find_review_by_review_workspace(&mut db, org_id, source_workspace_id)
            .await?
            .is_some()
        {
            db.rollback().await.map_err(AppError::from)?;
            return Ok(());
        }
        let copies = reviews::list_reviews_of_source(&mut db, org_id, source_workspace_id).await?;
        let mut recipients: Vec<&WorkspaceReviewRecord> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for copy in &copies {
            if copy.reviewer_user_id == actor_user_id || !seen.insert(copy.reviewer_user_id) {
                continue;
            }
            // The notice is filed in the reviewer's own copy, so the access
            // check on the filed workspace would only see the copy. What it
            // carries is the source's activity (its name, branch, commit,
            // actor): a reviewer who lost access to the source keeps their
            // copy but is told nothing more about the source.
            if !user_can_access_workspace(
                &self.system_pool,
                &mut db,
                copy.reviewer_user_id,
                org_id,
                source_workspace_id,
            )
            .await?
            {
                tracing::debug!(
                    source_workspace_id = %source_workspace_id,
                    reviewer_user_id = %copy.reviewer_user_id,
                    "source-head notice skipped: reviewer no longer has access to the source"
                );
                continue;
            }
            recipients.push(copy);
        }
        if recipients.is_empty() {
            db.rollback().await.map_err(AppError::from)?;
            return Ok(());
        }
        let mut actor_label =
            reviews::reviewer_display_name(&self.system_pool, actor_user_id).await?;
        if actor_label.trim().is_empty() {
            actor_label = "The owner".to_string();
        }
        let source_name = recipients[0].source_workspace_name.clone();
        let what = moves
            .iter()
            .map(|change| {
                let short: String = change.new_head.chars().take(7).collect();
                format!("{short} on {}", change.branch)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let title = format!("{source_name} has a new commit");
        let message = format!(
            "{actor_label} committed {what}. Update your review copy to review it; \
             rounds pushed before it are now outdated."
        );
        // Persisted against the reviewer's copy, not the source: a reviewer
        // who was away reopens their copy, and its inbox is what they load;
        // the source's inbox belongs to the owner's page.
        for copy in recipients {
            if !user_can_access_workspace(
                &self.system_pool,
                &mut db,
                copy.reviewer_user_id,
                org_id,
                copy.review_workspace_id,
            )
            .await?
            {
                tracing::debug!(
                    workspace_id = %copy.review_workspace_id,
                    recipient_user_id = %copy.reviewer_user_id,
                    "source-head notice skipped: recipient no longer has access to their copy"
                );
                continue;
            }
            self.notifications
                .create_notification(
                    &mut db,
                    CreateNotificationRequest {
                        workspace_id: copy.review_workspace_id,
                        recipient_user_id: copy.reviewer_user_id,
                        actor_user_id: Some(actor_user_id),
                        category: "review".to_string(),
                        severity: "info".to_string(),
                        title: title.clone(),
                        message: message.clone(),
                        task_id: None,
                        comment_id: None,
                        reply_id: None,
                        actions: vec![NotificationAction {
                            label: "Open my review copy".to_string(),
                            action: "navigate".to_string(),
                            target: format!("workspace:{}?view=review", copy.review_workspace_id),
                        }],
                        expires_at: None,
                    },
                )
                .await?;
        }
        db.commit().await.map_err(AppError::from)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl LandedCommitNotifier for ReviewSourceNotifier {
    async fn notify_after_landing(&self, landed: &LandedBranchCommit, actor_user_id: Uuid) {
        self.notify_reviewers_after_landing(landed, actor_user_id)
            .await;
    }
}
