use super::{validate_publication_claim, PublicationClaim};
use crate::services::git::repo_store::TenantRepoStorage;
use crate::{
    errors::AppError,
    services::{
        branch_commit::{
            BranchCommitError, BranchCommitMember, FenceMode, LandedBranchCommit,
            PreparedBranchCommit,
        },
        EventBus,
    },
};
use g5_authz::ScopedDb;
use std::{future::Future, pin::Pin};
use uuid::Uuid;

/// A remote operation whose synchronization effects have already been
/// registered. Callers cannot unwrap the generic commit or forget its admission
/// check. Worker leases are captured and applied inside the integration module.
#[must_use]
pub struct PreparedRemotePublication {
    advance: PreparedBranchCommit,
    admission: Admission,
}

enum Admission {
    BaseAcceptance(BaseAcceptanceAdmission),
    PendingWorker,
    Claimed(Box<PublicationClaim>),
}

/// Landing-time proof for an owner's or organization administrator's
/// acceptance of provider history on Base
/// (`g5.workspace.remote.base-provider-rewrite`).
pub(super) struct BaseAcceptanceAdmission {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub actor_id: Uuid,
}

impl PreparedRemotePublication {
    pub(super) fn new(advance: PreparedBranchCommit) -> Self {
        Self {
            advance,
            admission: Admission::PendingWorker,
        }
    }

    pub(super) fn with_claim(mut self, claim: PublicationClaim) -> Self {
        self.admission = Admission::Claimed(Box::new(claim));
        self
    }

    /// Unwrap a cut prepared without admission so a producer can attach its
    /// own ref effects before choosing its admission.
    pub(super) fn into_unadmitted_advance(self) -> PreparedBranchCommit {
        debug_assert!(matches!(self.admission, Admission::PendingWorker));
        self.advance
    }

    pub(super) fn for_base_acceptance(mut self, admission: BaseAcceptanceAdmission) -> Self {
        self.admission = Admission::BaseAcceptance(admission);
        self
    }

    pub fn advance_id(&self) -> Uuid {
        self.advance.advance_id()
    }
    pub fn target(&self) -> &g5_contracts::types::AuthorityTarget {
        self.advance.target()
    }

    pub fn member(&self, repo_id: Uuid) -> Option<&BranchCommitMember> {
        self.advance.member(repo_id)
    }

    pub async fn land(
        self,
        db: ScopedDb,
        events: &EventBus,
        storage: &TenantRepoStorage,
    ) -> Result<LandedBranchCommit, BranchCommitError> {
        self.land_with(db, events, storage, FenceMode::Blocking, |_, _| {
            Box::pin(async { Ok(()) })
        })
        .await
        .map(|(landed, ())| landed)
    }

    pub async fn land_interactive_bounded(
        self,
        db: ScopedDb,
        events: &EventBus,
        storage: &TenantRepoStorage,
    ) -> Result<LandedBranchCommit, BranchCommitError> {
        self.land_with(
            db,
            events,
            storage,
            FenceMode::InteractiveBounded,
            |_, _| Box::pin(async { Ok(()) }),
        )
        .await
        .map(|(landed, ())| landed)
    }

    pub async fn land_with<T, F>(
        self,
        db: ScopedDb,
        events: &EventBus,
        storage: &TenantRepoStorage,
        fence: FenceMode,
        in_tx: F,
    ) -> Result<(LandedBranchCommit, T), BranchCommitError>
    where
        T: Send,
        F: for<'a> FnOnce(
            &'a mut ScopedDb,
            &'a [BranchCommitMember],
        )
            -> Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>,
    {
        let Self { advance, admission } = self;
        advance
            .land_with_admission(
                db,
                events,
                storage,
                fence,
                move |db| {
                    Box::pin(async move {
                        match admission {
                            Admission::BaseAcceptance(base) => {
                                // The workspace must still be Base, and the
                                // acting user its current owner or an
                                // administrator of its organization
                                // (g5.workspace.remote.base-admin-pull). Then
                                // close the runs this acceptance supersedes,
                                // in the same transaction as the reset. The
                                // landing already holds the fence and, from
                                // inventory resolution, the workspace row
                                // lock, so this read takes no lock of its own.
                                let current: Option<(bool, bool)> = sqlx::query_as(
                                    "SELECT is_base, owner_user_id IS NOT DISTINCT FROM $3
                                       FROM workspaces
                                      WHERE org_id = $1 AND id = $2 AND deleted_at IS NULL",
                                )
                                .bind(base.org_id)
                                .bind(base.workspace_id)
                                .bind(base.actor_id)
                                .fetch_optional(db.conn())
                                .await
                                .map_err(AppError::from)?;
                                let admitted = match current {
                                    Some((true, true)) => true,
                                    Some((true, false)) => {
                                        match crate::services::PermissionService::require_org_admin_on_conn(
                                            db.conn(),
                                            base.actor_id,
                                            base.org_id,
                                        )
                                        .await
                                        {
                                            Ok(()) => true,
                                            Err(g5_contracts::G5Error::Forbidden { .. }) => false,
                                            Err(error) => return Err(error.into()),
                                        }
                                    }
                                    _ => false,
                                };
                                if !admitted {
                                    return Err(g5_contracts::G5Error::Conflict {
                                        reason: "You are no longer Base's owner or an administrator of its organization".into(),
                                    }
                                    .into());
                                }
                                sqlx::query(
                                    "UPDATE workspace_merge_runs
                                        SET status = 'completed', blocking_reason = 'accepted_provider_history',
                                            completed_at = now(), updated_at = now()
                                      WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3
                                        AND source_kind = 'remote_tracking'
                                        AND status IN ('pending', 'running', 'waiting_for_human')",
                                )
                                .bind(base.org_id)
                                .bind(base.workspace_id)
                                .bind(base.repo_id)
                                .execute(db.conn())
                                .await
                                .map_err(AppError::from)?;
                            }
                            Admission::Claimed(claim) => {
                                validate_publication_claim(db.conn(), &claim).await?
                            }
                            Admission::PendingWorker => {
                                return Err(g5_contracts::G5Error::Conflict {
                                    reason: "Remote worker publication has no live admission claim"
                                        .into(),
                                }
                                .into())
                            }
                        }
                        Ok(())
                    })
                },
                in_tx,
            )
            .await
    }
}
