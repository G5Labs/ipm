//! Bind provider identity, policy, synchronization refs and audit in one publication.
use crate::errors::AppError;
use crate::services::branch_commit::{
    map_branch_commit_error, BranchCommitError, BranchCommitMember, FenceMode, LandedBranchCommit,
    PreparedBranchCommit, ZERO_GIT_SHA,
};
use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use crate::services::EventBus;
use g5_authz::ScopedDb;
use g5_contracts::{types::WorkspaceRepoKey, G5Error};
use uuid::Uuid;

#[must_use]
pub(crate) struct PreparedBindingPublication {
    advance: PreparedBranchCommit,
    proposal: PreparedMakeRemoteApplication,
}

pub(crate) fn prepare_binding_publication(
    advance: PreparedBranchCommit,
    storage: &ScopedRepoStorage,
    proposal: PreparedMakeRemoteApplication,
) -> Result<PreparedBindingPublication, G5Error> {
    validate_make_remote_application(advance.members(), &proposal)
        .map_err(map_branch_commit_error)?;
    let refs = super::watermark_effects(
        storage,
        advance.org_id(),
        advance.members(),
        &[super::SyncWatermarkUpdate {
            workspace_id: proposal.workspace_id,
            repo_id: proposal.repo_id,
            expected: proposal.expected_baseline_commit.clone(),
            new: (proposal.proposed_baseline_commit != ZERO_GIT_SHA)
                .then(|| proposal.proposed_baseline_commit.clone()),
        }],
    )?;
    Ok(PreparedBindingPublication {
        advance: advance.with_additional_refs(refs),
        proposal,
    })
}

impl PreparedBindingPublication {
    pub(crate) async fn land_interactive_bounded(
        self,
        db: ScopedDb,
        events: &EventBus,
        storage: &TenantRepoStorage,
    ) -> Result<(LandedBranchCommit, ()), BranchCommitError> {
        let Self { advance, proposal } = self;
        advance
            .land_with(
                db,
                events,
                storage,
                FenceMode::InteractiveBounded,
                move |db, members| {
                    Box::pin(async move {
                        apply_make_remote_application(db.conn(), members, &proposal)
                            .await
                            .map_err(|error| AppError::from(map_branch_commit_error(error)))
                    })
                },
            )
            .await
    }
}

/// Make-Remote binding evidence applied in the landing transaction of the
/// cut that publishes the bound branch.
// LEAN-TODO(P1.5): Make-Remote reduces to "bind repo to provider, then publish".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedMakeRemoteApplication {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub expected_git_url: String,
    pub expected_provider: Option<String>,
    pub expected_provider_repo_id: Option<String>,
    pub expected_provider_full_name: Option<String>,
    pub proposed_git_url: String,
    pub proposed_provider: Option<String>,
    pub proposed_provider_repo_id: Option<String>,
    pub proposed_provider_full_name: Option<String>,
    pub expected_tracking_mode: String,
    pub expected_remote_branch: Option<String>,
    pub expected_baseline_commit: Option<String>,
    pub expected_remote_tip_advisory_sha: Option<String>,
    pub expected_sync_paused_reason: Option<String>,
    pub expected_head: String,
    pub proposed_head: String,
    pub proposed_tracking_mode: String,
    pub proposed_remote_branch: String,
    pub proposed_baseline_commit: String,
    pub proposed_remote_tip_advisory_sha: String,
    pub proposed_sync_paused_reason: Option<String>,
    pub acting_user_id: Uuid,
    pub trigger_kind: String,
    pub sync_status: String,
    pub outcome: String,
    pub merge_run_id: Option<Uuid>,
}

fn make_remote_protocol_error(message: &str) -> sqlx::Error {
    sqlx::Error::Protocol(message.to_string())
}

fn make_remote_is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value.as_bytes().iter().any(|byte| *byte != b'0')
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

// LEAN-TODO(P1.5): Make-Remote reduces to "bind repo to provider, then publish".
fn validate_make_remote_application(
    members: &[BranchCommitMember],
    proposal: &PreparedMakeRemoteApplication,
) -> Result<(), sqlx::Error> {
    let Some(member) = members.iter().find(|member| {
        member.repo_id == proposal.repo_id && member.workspace_id == proposal.workspace_id
    }) else {
        return Err(make_remote_protocol_error(
            "Make-Remote application does not name an exact cut member",
        ));
    };
    if member.expected_old_head != proposal.expected_head
        || member.proposed_new_head != proposal.proposed_head
    {
        return Err(make_remote_protocol_error(
            "Make-Remote application head tuple differs from its cut member",
        ));
    }
    let lower_hex_sha = |value: &str| {
        value.len() == 40
            && value
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    };
    if !make_remote_is_git_sha(&proposal.expected_head)
        || !make_remote_is_git_sha(&proposal.proposed_head)
        || !lower_hex_sha(&proposal.proposed_baseline_commit)
        || !make_remote_is_git_sha(&proposal.proposed_remote_tip_advisory_sha)
        || proposal.proposed_tracking_mode != "remote_tracking"
        || proposal.proposed_remote_branch.trim().is_empty()
        || proposal.trigger_kind != crate::models::branch_sync_run::trigger_kind::MANUAL
    {
        return Err(make_remote_protocol_error(
            "Make-Remote application contains malformed identity or tracking evidence",
        ));
    }
    let merge_required = proposal.outcome == "merge_required";
    if merge_required
        != (proposal.merge_run_id.is_some()
            && proposal.proposed_sync_paused_reason.as_deref() == Some("diverged")
            && proposal.sync_status == crate::models::branch_sync_run::status::DIVERGED)
        || (!merge_required
            && (proposal.merge_run_id.is_some()
                || proposal.proposed_sync_paused_reason.is_some()
                || !matches!(proposal.sync_status.as_str(), "noop" | "imported")))
        || !matches!(
            proposal.outcome.as_str(),
            "bound" | "published" | "merge_required"
        )
    {
        return Err(make_remote_protocol_error(
            "Make-Remote application outcome metadata is inconsistent",
        ));
    }
    Ok(())
}

/// Bind the repository to its provider identity and flip the member to
/// remote tracking, in the landing transaction, after the member heads were
/// refreshed.
// LEAN-TODO(P1.5): Make-Remote reduces to "bind repo to provider, then publish".
async fn apply_make_remote_application(
    conn: &mut sqlx::PgConnection,
    members: &[BranchCommitMember],
    proposal: &PreparedMakeRemoteApplication,
) -> Result<(), sqlx::Error> {
    validate_make_remote_application(members, proposal)?;

    let identity_update = sqlx::query(
        r#"
        UPDATE repos
           SET git_url = $1,
               provider = $2,
               provider_repo_id = $3,
               provider_full_name = $4
         WHERE id = $5
           AND org_id = $6
           AND git_url = $7
           AND provider IS NOT DISTINCT FROM $8
           AND provider_repo_id IS NOT DISTINCT FROM $9
           AND provider_full_name IS NOT DISTINCT FROM $10
        "#,
    )
    .bind(&proposal.proposed_git_url)
    .bind(&proposal.proposed_provider)
    .bind(&proposal.proposed_provider_repo_id)
    .bind(&proposal.proposed_provider_full_name)
    .bind(proposal.repo_id)
    .bind(proposal.org_id)
    .bind(&proposal.expected_git_url)
    .bind(&proposal.expected_provider)
    .bind(&proposal.expected_provider_repo_id)
    .bind(&proposal.expected_provider_full_name)
    .execute(&mut *conn)
    .await;
    let identity_update = match identity_update {
        Ok(result) => result,
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
            return Err(make_remote_protocol_error(
                "Make-Remote repository identity was claimed by a concurrent operation",
            ));
        }
        Err(error) => return Err(error),
    };
    if identity_update.rows_affected() != 1 {
        return Err(make_remote_protocol_error(
            "Make-Remote repository identity changed after preparation",
        ));
    }

    if proposal.expected_remote_branch.is_some()
        || proposal.expected_tracking_mode == "remote_tracking"
    {
        return Err(make_remote_protocol_error(
            "Make-Remote requires an unbound repository",
        ));
    }
    if !super::bind_policy(
        conn,
        WorkspaceRepoKey {
            org_id: proposal.org_id,
            workspace_id: proposal.workspace_id,
            repo_id: proposal.repo_id,
        },
        &proposal.proposed_remote_branch,
        proposal.proposed_sync_paused_reason.as_deref(),
        true,
    )
    .await?
    {
        return Err(make_remote_protocol_error(
            "Make-Remote binding changed after preparation",
        ));
    }

    crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
        conn,
        proposal.workspace_id,
    )
    .await
    .map_err(|error| make_remote_protocol_error(&error.to_string()))?;

    if let Some(merge_run_id) = proposal.merge_run_id {
        let active_run: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
              FROM workspace_merge_runs
             WHERE repo_id = $1 AND workspace_id = $2
               AND source_kind = 'remote_tracking'
               AND status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
             LIMIT 1
             FOR UPDATE
            "#,
        )
        .bind(proposal.repo_id)
        .bind(proposal.workspace_id)
        .fetch_optional(&mut *conn)
        .await?;
        match active_run {
            Some(existing) if existing != merge_run_id => {
                return Err(make_remote_protocol_error(
                    "Make-Remote branch already has a different active merge run",
                ));
            }
            Some(_) => {}
            None => {
                let source_heads = serde_json::json!({
                    "workspace_tip": &proposal.proposed_head,
                    "mirror_tip": &proposal.proposed_remote_tip_advisory_sha,
                });
                let target_heads = serde_json::json!({
                    "baseline": &proposal.proposed_baseline_commit,
                });
                sqlx::query(
                    r#"
                    INSERT INTO workspace_merge_runs (
                        id, workspace_id, repo_id, source_kind,
                        source_workspace_id, target_workspace_id,
                        source_branch_name, status, phase, source_heads,
                        target_heads, pre_start_source_heads
                    ) VALUES (
                        $1, $2, $3, 'remote_tracking', NULL, NULL,
                        $4, 'waiting_for_human', 'workspace_rebase_preflight',
                        $5, $6, $5
                    )
                    "#,
                )
                .bind(merge_run_id)
                .bind(proposal.workspace_id)
                .bind(proposal.repo_id)
                .bind(&proposal.proposed_remote_branch)
                .bind(&source_heads)
                .bind(&target_heads)
                .execute(&mut *conn)
                .await?;
            }
        }
    }

    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, repo_id, workspace_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip,
            old_mirror_tip, new_mirror_tip,
            new_baseline, merge_run_id,
            started_at, completed_at
        ) VALUES (
            $1, $2, $12, $3, 'integration', $4, $5,
            $6, $7, $8, $9, $10, $11, now(), now()
        )
        "#,
    )
    .bind(proposal.org_id)
    .bind(proposal.repo_id)
    .bind(proposal.acting_user_id)
    .bind(&proposal.trigger_kind)
    .bind(&proposal.sync_status)
    .bind(&proposal.expected_head)
    .bind(&proposal.proposed_head)
    .bind(&proposal.expected_remote_tip_advisory_sha)
    .bind(&proposal.proposed_remote_tip_advisory_sha)
    .bind(&proposal.proposed_baseline_commit)
    .bind(proposal.merge_run_id)
    .bind(proposal.workspace_id)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn make_remote_application_requires_exact_member_and_consistent_outcome() {
        let workspace_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();
        let expected_head = "a".repeat(40);
        let proposed_head = "b".repeat(40);
        let members = vec![BranchCommitMember {
            workspace_id,
            repo_id,
            branch_name: "main".to_string(),
            expected_old_head: expected_head.clone(),
            proposed_new_head: proposed_head.clone(),
            transition: g5_contracts::types::WorkspaceHeadTransitionKind::Advance,
        }];
        let proposal = PreparedMakeRemoteApplication {
            org_id: Uuid::new_v4(),
            workspace_id,
            repo_id,
            expected_git_url: "g5-local://repo".to_string(),
            expected_provider: None,
            expected_provider_repo_id: None,
            expected_provider_full_name: None,
            proposed_git_url: "https://github.com/acme/repo.git".to_string(),
            proposed_provider: Some("github".to_string()),
            proposed_provider_repo_id: Some("acme/repo".to_string()),
            proposed_provider_full_name: Some("acme/repo".to_string()),
            expected_tracking_mode: "tracking".to_string(),
            expected_remote_branch: None,
            expected_baseline_commit: None,
            expected_remote_tip_advisory_sha: None,
            expected_sync_paused_reason: None,
            expected_head: expected_head.clone(),
            proposed_head: proposed_head.clone(),
            proposed_tracking_mode: "remote_tracking".to_string(),
            proposed_remote_branch: "main".to_string(),
            proposed_baseline_commit: proposed_head.clone(),
            proposed_remote_tip_advisory_sha: proposed_head,
            proposed_sync_paused_reason: None,
            acting_user_id: Uuid::new_v4(),
            trigger_kind: crate::models::branch_sync_run::trigger_kind::MANUAL.to_string(),
            sync_status: crate::models::branch_sync_run::status::IMPORTED.to_string(),
            outcome: "bound".to_string(),
            merge_run_id: None,
        };
        assert!(validate_make_remote_application(&members, &proposal).is_ok());

        let mut stale = proposal.clone();
        stale.expected_head = "c".repeat(40);
        assert!(validate_make_remote_application(&members, &stale).is_err());

        let mut malformed_merge = proposal;
        malformed_merge.outcome = "merge_required".to_string();
        assert!(validate_make_remote_application(&members, &malformed_merge).is_err());
    }
}
