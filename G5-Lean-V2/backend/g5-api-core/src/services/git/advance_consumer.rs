//! Domain bookkeeping keyed by a landed cut's advance id.
//!
//! P1.4 deleted the head-operation registry: commits land by a direct
//! compare-and-swap on the branch ref (`services::branch_commit`) and nothing
//! is registered, leased or replayed. A few domain consumers that a later PR
//! retires still record inputs and receipts keyed by the advance id the
//! landing helper mints. They apply their effects inside the landing
//! transaction of that advance: the registered landing consumers below, and
//! the workspace-merge and workspace pull-request settlements a merge producer lands
//! with. A landed cut is therefore never left unsettled, and a failed
//! settlement refuses the landing with every ref where it was.
//!
//! Remote integration inputs hold exact operation evidence; Git owns all heads
//! and synchronization watermarks. Policy and audit receipts remain in SQL.
//! - workspace pull-request merge receipts: LEAN-TODO(P6.4)

use std::path::{Component, Path};

use crate::services::git::repo_store::ScopedRepoStorage;
use g5_contracts::error::G5Error;
use g5_contracts::types::{HistoricalGitVersion, HistoricalReadReason};
use g5_internal_api::dtos::TaskResultArtifactRef;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use g5_contracts::types::AuthorityTarget;

use crate::services::branch_commit::BranchCommitMember;
use crate::services::workspace_reader::CommittedRepoReadView;

/// The exact members of a cut used to settle its domain effects while landing.
// LEAN-TODO(P6.4): retires with workspace pull requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandedCutRecord {
    pub advance_id: Uuid,
    pub workspace_id: Uuid,
    pub target: AuthorityTarget,
    pub members: Vec<BranchCommitMember>,
}

/// Validate a completion artifact against the exact repository head the
/// worker published.
pub async fn validate_task_result_artifact(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    published_head: &str,
    artifact: &TaskResultArtifactRef,
) -> Result<(), G5Error> {
    let repo_id = Uuid::parse_str(&artifact.repository).map_err(|_| G5Error::ValidationError {
        details: serde_json::json!({
            "field": "resultArtifact.repository",
            "error": "result artifact repository must be a canonical UUID",
        }),
    })?;
    if repo_id.to_string() != artifact.repository {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.repository",
                "error": "result artifact repository must use canonical lowercase UUID form",
            }),
        });
    }
    let path = Path::new(&artifact.repo_relative_path);
    if artifact.repo_relative_path.is_empty()
        || artifact.repo_relative_path.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.repoRelativePath",
                "error": "result artifact path must be normalized and repository-relative",
            }),
        });
    }
    if artifact.sha256.len() != 64
        || !artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.sha256",
                "error": "result artifact digest must be lowercase SHA-256",
            }),
        });
    }
    let published_view = CommittedRepoReadView::from_historical(
        repo_storage,
        org_id,
        &HistoricalGitVersion {
            repo_id,
            head_commit: published_head.to_string(),
            reason: HistoricalReadReason::OperationValidation,
        },
    )
    .map_err(|error| G5Error::ValidationError {
        details: serde_json::json!({
            "field": "resultArtifact.repository",
            "error": format!("result artifact published head is invalid: {error}"),
            "repository": artifact.repository,
        }),
    })?;
    let bytes = published_view
        .read_file(repo_storage, &artifact.repo_relative_path)
        .await
        .map_err(|error| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.repoRelativePath",
                "error": format!("result artifact is unavailable at the published head: {error}"),
            }),
        })?;
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    if actual_sha256 != artifact.sha256 {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.sha256",
                "error": "result artifact digest does not match the published head",
                "expected": actual_sha256,
                "provided": artifact.sha256,
            }),
        });
    }
    if bytes.len() as u64 != artifact.byte_length {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "resultArtifact.byteLength",
                "error": "result artifact byte length does not match the published head",
                "expected": bytes.len(),
                "provided": artifact.byte_length,
            }),
        });
    }
    Ok(())
}

/// Record a consumer input keyed by a landing's advance id.
pub async fn register_consumer_input<T: Serialize>(
    conn: &mut PgConnection,
    advance_id: Uuid,
    consumer_kind: &str,
    payload: &T,
) -> Result<(), sqlx::Error> {
    if consumer_kind.trim().is_empty() {
        return Err(protocol_error("Advance consumer kind must be non-empty"));
    }
    let payload =
        serde_json::to_value(payload).map_err(|error| protocol_error(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO workspace_advance_consumer_inputs (operation_id, consumer_kind, payload)
        VALUES ($1, $2, $3)
        ON CONFLICT (operation_id, consumer_kind) DO NOTHING
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .bind(&payload)
    .execute(&mut *conn)
    .await?;
    let registered: Option<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT payload
        FROM workspace_advance_consumer_inputs
        WHERE operation_id = $1 AND consumer_kind = $2
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .fetch_optional(&mut *conn)
    .await?;
    match registered {
        Some(registered) if registered == payload => Ok(()),
        Some(_) => Err(protocol_error(format!(
            "Advance {advance_id} consumer {consumer_kind} was registered with a different payload"
        ))),
        None => Err(protocol_error(format!(
            "Advance {advance_id} consumer {consumer_kind} input was not visible after registration"
        ))),
    }
}

pub(crate) async fn load_consumer_input<T: DeserializeOwned>(
    conn: &mut PgConnection,
    advance_id: Uuid,
    consumer_kind: &str,
) -> Result<Option<T>, sqlx::Error> {
    let payload: Option<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT payload
        FROM workspace_advance_consumer_inputs
        WHERE operation_id = $1
          AND consumer_kind = $2
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .fetch_optional(&mut *conn)
    .await?;
    payload
        .map(|payload| {
            serde_json::from_value(payload).map_err(|error| protocol_error(error.to_string()))
        })
        .transpose()
}

pub async fn consumer_receipt_result<T: DeserializeOwned>(
    conn: &mut PgConnection,
    advance_id: Uuid,
    consumer_kind: &str,
) -> Result<Option<T>, sqlx::Error> {
    let result: Option<Option<serde_json::Value>> = sqlx::query_scalar(
        r#"
        SELECT result
        FROM workspace_advance_consumer_receipts
        WHERE operation_id = $1 AND consumer_kind = $2
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .fetch_optional(&mut *conn)
    .await?;
    result
        .flatten()
        .map(|result| {
            serde_json::from_value(result).map_err(|error| protocol_error(error.to_string()))
        })
        .transpose()
}

pub(crate) async fn consumer_receipt_exists(
    conn: &mut PgConnection,
    advance_id: Uuid,
    consumer_kind: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM workspace_advance_consumer_receipts
            WHERE operation_id = $1 AND consumer_kind = $2
        )
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .fetch_one(&mut *conn)
    .await
}

pub(crate) async fn record_consumer_receipt<T: Serialize>(
    conn: &mut PgConnection,
    advance_id: Uuid,
    consumer_kind: &str,
    result: Option<&T>,
) -> Result<(), sqlx::Error> {
    let result = result
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| protocol_error(error.to_string()))?;
    sqlx::query(
        r#"
        INSERT INTO workspace_advance_consumer_receipts (
            operation_id, consumer_kind, result
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (operation_id, consumer_kind) DO NOTHING
        "#,
    )
    .bind(advance_id)
    .bind(consumer_kind)
    .bind(result)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn protocol_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}

// Compatibility entry points for generic landing producers. Integration policy
// and its stored consumer payloads are owned by the remote integration module.
pub(crate) use crate::services::git::remote_integrations::convergence::{
    apply_registered_landing_consumers, register_remote_tracking_convergence_consumer,
};
pub use crate::services::git::remote_integrations::convergence::{
    register_initial_remote_tracking_consumer, validate_initial_remote_tracking_inputs,
    InitialRemoteTrackingInput, PreparedRemoteTrackingConvergence, RemoteTrackingConvergenceStatus,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_convergence_input_without_pause_reason_decodes() {
        let historical = serde_json::json!({
            "workspaceId": Uuid::nil(),
            "repoId": Uuid::nil(),
            "remoteBranch": "main",
            "expectedTrackingMode": "remote_tracking",
            "expectedRemoteBranch": "main",
            "expectedOldHead": "1111111111111111111111111111111111111111",
            "proposedNewHead": "2222222222222222222222222222222222222222",
            "expectedBaselineCommit": "1111111111111111111111111111111111111111",
            "expectedGitUrl": "https://example.test/repo.git",
            "actingUserId": Uuid::nil(),
            "triggerKind": "manual",
            "status": "noop",
        });

        let decoded: PreparedRemoteTrackingConvergence =
            serde_json::from_value(historical).expect("decode historical convergence input");
        assert_eq!(decoded.expected_sync_paused_reason, None);

        let encoded =
            serde_json::to_value(decoded).expect("re-encode historical convergence input");
        assert!(
            encoded.get("expectedSyncPausedReason").is_none(),
            "recovery must preserve the historical payload shape"
        );
    }

    #[tokio::test]
    async fn result_artifact_validation_is_bound_to_the_published_head() {
        use crate::services::git::repo_store::{TenantRepoStorage, TestOrgScope};
        use g5_scm::{GitAuthor, GitRepo};

        let root = tempfile::tempdir().expect("repository storage");
        let org_id = Uuid::new_v4();
        let storage = TenantRepoStorage::local_for_test(root.path().to_path_buf())
            .scoped(&TestOrgScope(org_id));
        let repo_id = Uuid::new_v4();
        let repo_path = storage.repo_path(org_id, repo_id).expect("repository path");
        std::fs::create_dir_all(&repo_path).expect("create repository path");
        git2::Repository::init(&repo_path).expect("initialize repository");
        let bytes = b"authoritative full review\n";
        std::fs::write(repo_path.join("review.md"), bytes).expect("write review artifact");
        let repo = GitRepo::open(&repo_path).expect("open repository");
        repo.add_paths(["review.md"])
            .expect("stage review artifact");
        let head = repo
            .commit(
                "record review artifact",
                &GitAuthor {
                    name: "Artifact Test".to_string(),
                    email: "artifact@example.com".to_string(),
                },
            )
            .expect("commit review artifact");
        let valid = TaskResultArtifactRef {
            repository: repo_id.to_string(),
            repo_relative_path: "review.md".to_string(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            byte_length: bytes.len() as u64,
        };
        validate_task_result_artifact(&storage, org_id, &head, &valid)
            .await
            .expect("exact artifact evidence must validate");

        for invalid in [
            TaskResultArtifactRef {
                repository: Uuid::new_v4().to_string(),
                ..valid.clone()
            },
            TaskResultArtifactRef {
                repo_relative_path: "./review.md".to_string(),
                ..valid.clone()
            },
            TaskResultArtifactRef {
                repo_relative_path: "missing.md".to_string(),
                ..valid.clone()
            },
            TaskResultArtifactRef {
                sha256: "0".repeat(64),
                ..valid.clone()
            },
            TaskResultArtifactRef {
                byte_length: valid.byte_length + 1,
                ..valid.clone()
            },
        ] {
            assert!(
                validate_task_result_artifact(&storage, org_id, &head, &invalid)
                    .await
                    .is_err(),
                "mismatched artifact evidence must fail closed: {invalid:?}"
            );
        }
    }
}
