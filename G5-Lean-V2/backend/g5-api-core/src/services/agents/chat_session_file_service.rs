//! Provider session files in Postgres (design v6 §8.4).
//!
//! `chat_session_files` holds the files a provider CLI needs to resume its
//! own session: the Claude transcript and its subagent transcripts, and the
//! Codex rollout. The provider runner uploads the files whose sha256 changed
//! after every turn and restores them before a resumed turn on a host that
//! lacks them. The API treats the bytes as opaque: it checks the provider
//! path, the digest and the size, and never parses or logs the content.
//!
//! The handler opens a `ScopedDb`, threads it through here, and commits, like
//! every other worker control-plane service.

use chrono::{DateTime, Utc};
use g5_contracts::error::G5Error;
use g5_internal_api::dtos::{
    ChatSessionFileManifestItem, PendingSessionFileBundle, PublishedSessionFile,
    PutChatSessionFileResponse, SessionFileBundleBegin, SessionFileBundlePublication,
};
use g5_internal_api::session_files::{
    sha256_hex, validate_provider_session_id, validate_session_file_path, validate_sha256_hex,
    SessionFileError, SessionFileProvider, CHAT_SESSION_FILE_MAX_BYTES,
};
use serde_json::json;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::agents::session_file_sha256::ResumableSha256;

const SESSION_FILE_SEGMENT_BYTES: usize = 8 * 1024 * 1024;

/// A stored session file with its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredChatSessionFile {
    pub provider: SessionFileProvider,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// One decompressed upload, before validation.
#[derive(Debug, Clone, Copy)]
pub struct ChatSessionFileUpload<'a> {
    pub provider: SessionFileProvider,
    pub provider_session_id: &'a str,
    pub relative_path: &'a str,
    /// The digest the uploader declared; it must match `bytes`.
    pub sha256: &'a str,
    pub bytes: &'a [u8],
    /// Stored prefix for a fenced append. The declared digest names the full
    /// resulting file; `bytes` contains only the suffix in this case.
    pub base: Option<(i64, &'a str)>,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionFileIncarnation {
    pub vm_id: Uuid,
    pub epoch: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ChatSessionFileService;

impl ChatSessionFileService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Restore the preceding complete set only after a normal outcome proves
    /// that the fenced input remains reserved and no provider session was
    /// reported. The caller must not use this for recovered outcomes.
    pub async fn restore_if_undelivered(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        task_id: Uuid,
        attempt: i32,
        incarnation: SessionFileIncarnation,
    ) -> Result<bool, AppError> {
        let changed = sqlx::query(
            "UPDATE chat_session_file_bundles AS bundle
             SET state = 'complete',
                 generation = (bundle.prior_publication ->> 'generation')::uuid,
                 provider = bundle.prior_publication ->> 'provider',
                 provider_session_id = bundle.prior_publication ->> 'provider_session_id',
                 files = bundle.prior_publication -> 'files',
                 cwd_relative_path = bundle.prior_publication ->> 'cwd_relative_path',
                 prior_publication = NULL, updated_at = now()
             WHERE bundle.session_id = $1 AND bundle.state = 'pending'
               AND bundle.prior_publication IS NOT NULL
               AND bundle.provider IS NULL AND bundle.provider_session_id IS NULL
               AND bundle.fenced_task_id = $2 AND bundle.fenced_attempt = $3
               AND bundle.fenced_vm_id = $4 AND bundle.fenced_epoch = $5
               AND EXISTS (
                 SELECT 1 FROM session_inputs AS input
                 WHERE input.task_id = $2 AND input.attempt = $3
                   AND input.reservation_id = bundle.fenced_reservation_id
                   AND input.delivery = 'reserved')",
        )
        .bind(session_id)
        .bind(task_id)
        .bind(attempt)
        .bind(incarnation.vm_id.to_string())
        .bind(
            i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict {
                reason: "session bundle epoch exceeds bigint".into(),
            })?,
        )
        .execute(conn)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Fence the previous publication before this turn can advance provider state.
    pub async fn begin_bundle(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        begin: &SessionFileBundleBegin,
        incarnation: SessionFileIncarnation,
    ) -> Result<(), AppError> {
        if !begin.cwd_relative_path.is_empty()
            && !std::path::Path::new(&begin.cwd_relative_path)
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({"field":"cwd_relative_path","message":"invalid working directory"}),
            }.into());
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tasks AS task
             JOIN session_inputs AS input ON input.task_id = task.id AND input.attempt = task.attempt
             WHERE task.id = $2 AND task.chat_session_id = $1 AND task.attempt = $3
               AND task.vm_id = $5 AND task.epoch = $6
               AND task.status = 'running' AND input.reservation_id = $4
               AND input.delivery = 'reserved' FOR SHARE OF task)",
        )
        .bind(session_id)
        .bind(begin.task_id)
        .bind(begin.attempt)
        .bind(begin.reservation_id)
        .bind(incarnation.vm_id.to_string())
        .bind(i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict { reason: "session bundle epoch exceeds bigint".into() })?)
        .fetch_one(&mut *conn)
        .await?;
        if !active {
            return Err(G5Error::Conflict {
                reason: "session bundle turn is no longer active".into(),
            }
            .into());
        }
        let written = sqlx::query(
            "INSERT INTO chat_session_file_bundles(
                 session_id, org_id, generation, state, fenced_task_id,
                 fenced_attempt, fenced_reservation_id, fenced_vm_id, fenced_epoch, cwd_relative_path)
             SELECT id, org_id, $2, 'pending', $3, $4, $5, $6, $7, $8
             FROM chat_sessions WHERE id = $1
             ON CONFLICT (session_id) DO UPDATE SET
               prior_publication = CASE WHEN chat_session_file_bundles.state = 'complete'
                 THEN jsonb_build_object(
                   'generation', chat_session_file_bundles.generation,
                   'provider', chat_session_file_bundles.provider,
                   'provider_session_id', chat_session_file_bundles.provider_session_id,
                   'files', chat_session_file_bundles.files,
                   'cwd_relative_path', chat_session_file_bundles.cwd_relative_path)
                 ELSE NULL END,
               generation = EXCLUDED.generation, state = 'pending',
               provider = NULL, provider_session_id = NULL, files = '[]'::jsonb,
               fenced_task_id = EXCLUDED.fenced_task_id,
               fenced_attempt = EXCLUDED.fenced_attempt,
               fenced_reservation_id = EXCLUDED.fenced_reservation_id,
               fenced_vm_id = EXCLUDED.fenced_vm_id,
               fenced_epoch = EXCLUDED.fenced_epoch,
               cwd_relative_path = EXCLUDED.cwd_relative_path,
               updated_at = now()
             WHERE chat_session_file_bundles.generation IS DISTINCT FROM EXCLUDED.generation",
        )
        .bind(session_id)
        .bind(begin.generation)
        .bind(begin.task_id)
        .bind(begin.attempt)
        .bind(begin.reservation_id)
        .bind(incarnation.vm_id.to_string())
        .bind(
            i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict {
                reason: "session bundle epoch exceeds bigint".into(),
            })?,
        )
        .bind(&begin.cwd_relative_path)
        .execute(&mut *conn)
        .await?;
        if written.rows_affected() == 0 {
            let same_fence: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM chat_session_file_bundles
                 WHERE session_id = $1 AND generation = $2 AND state = 'pending'
                   AND fenced_task_id = $3 AND fenced_attempt = $4
                   AND fenced_reservation_id = $5 AND fenced_vm_id = $6
                   AND fenced_epoch = $7 AND cwd_relative_path = $8)",
            )
            .bind(session_id)
            .bind(begin.generation)
            .bind(begin.task_id)
            .bind(begin.attempt)
            .bind(begin.reservation_id)
            .bind(incarnation.vm_id.to_string())
            .bind(
                i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict {
                    reason: "session bundle epoch exceeds bigint".into(),
                })?,
            )
            .bind(&begin.cwd_relative_path)
            .fetch_one(conn)
            .await?;
            if same_fence {
                return Ok(());
            }
            return Err(G5Error::NotFound {
                resource: "chat_session".into(),
            }
            .into());
        }
        Ok(())
    }

    /// Pending bytes may be completed only after the dispatcher has recorded
    /// the terminal outcome that follows its process-tree reap.
    pub async fn pending_bundles(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        incarnation: SessionFileIncarnation,
        session_id: Option<Uuid>,
    ) -> Result<Vec<PendingSessionFileBundle>, AppError> {
        let rows = sqlx::query(
            "SELECT bundle.session_id, bundle.generation, bundle.provider,
                    bundle.provider_session_id, bundle.cwd_relative_path
             FROM chat_session_file_bundles AS bundle
             JOIN chat_sessions AS session ON session.id = bundle.session_id
             WHERE session.workspace_id = $1 AND bundle.state = 'pending'
               AND bundle.fenced_vm_id = $2 AND bundle.fenced_epoch = $3
               AND ($4::uuid IS NULL OR bundle.session_id = $4)
               AND bundle.provider IS NOT NULL AND bundle.provider_session_id IS NOT NULL
               AND EXISTS (
                 SELECT 1 FROM task_attempts AS attempt
                 JOIN task_events AS event
                   ON event.task_id = attempt.task_id
                  AND event.attempt = attempt.attempt
                  AND event.vm_id = attempt.vm_id
                 WHERE event.task_id = bundle.fenced_task_id
                   AND event.attempt = bundle.fenced_attempt
                   AND event.vm_id = bundle.fenced_vm_id
                   AND attempt.epoch = bundle.fenced_epoch
                   AND attempt.ended_at IS NOT NULL
                   AND event.metadata ->> 'kind' = 'invocation.outcome'
                   AND event.metadata ->> 'terminal' = 'true'
               )",
        )
        .bind(workspace_id)
        .bind(incarnation.vm_id.to_string())
        .bind(
            i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict {
                reason: "session bundle epoch exceeds bigint".into(),
            })?,
        )
        .bind(session_id)
        .fetch_all(conn)
        .await?;
        rows.into_iter()
            .map(|row| {
                let provider: String = row.try_get("provider")?;
                let provider =
                    SessionFileProvider::parse(&provider).ok_or_else(|| G5Error::Internal {
                        message: "pending session bundle has an invalid provider".into(),
                    })?;
                Ok(PendingSessionFileBundle {
                    session_id: row.try_get("session_id")?,
                    generation: row.try_get("generation")?,
                    provider,
                    provider_session_id: row.try_get("provider_session_id")?,
                    cwd_relative_path: row.try_get("cwd_relative_path")?,
                })
            })
            .collect()
    }

    /// Return only the generation committed as current for this provider session.
    pub async fn published_bundle(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        provider: SessionFileProvider,
        provider_session_id: &str,
    ) -> Result<Option<SessionFileBundlePublication>, AppError> {
        validate_provider_session_id(provider_session_id)
            .map_err(|error| validation_error(&error, "provider_session_id"))?;
        let row = sqlx::query(
            "SELECT generation, files FROM chat_session_file_bundles
             WHERE session_id = $1 AND state = 'complete' AND provider = $2
               AND provider_session_id = $3",
        )
        .bind(session_id)
        .bind(provider.as_str())
        .bind(provider_session_id)
        .fetch_optional(conn)
        .await?;
        row.map(|row| {
            let files: Vec<PublishedSessionFile> = serde_json::from_value(row.get("files"))?;
            Ok(SessionFileBundlePublication {
                generation: row.get("generation"),
                provider,
                provider_session_id: provider_session_id.to_string(),
                files,
            })
        })
        .transpose()
    }

    /// Commit exactly the enumerated set after every named upload is present.
    pub async fn publish_bundle(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        publication: &SessionFileBundlePublication,
        incarnation: SessionFileIncarnation,
    ) -> Result<(), AppError> {
        validate_provider_session_id(&publication.provider_session_id)
            .map_err(|error| validation_error(&error, "provider_session_id"))?;
        if publication.files.is_empty() {
            return Err(G5Error::Conflict {
                reason: "empty session bundle".into(),
            }
            .into());
        }
        let row = sqlx::query(
            "SELECT generation, state, provider, provider_session_id, files,
                    fenced_vm_id, fenced_epoch
             FROM chat_session_file_bundles WHERE session_id = $1 FOR UPDATE",
        )
        .bind(session_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(row) = row else {
            return Err(G5Error::Conflict {
                reason: "session bundle was not begun".into(),
            }
            .into());
        };
        if row.get::<Option<String>, _>("fenced_vm_id").as_deref()
            != Some(incarnation.vm_id.to_string().as_str())
            || row.get::<Option<i64>, _>("fenced_epoch") != i64::try_from(incarnation.epoch).ok()
        {
            return Err(G5Error::Conflict {
                reason: "session bundle belongs to another VM incarnation".into(),
            }
            .into());
        }
        let state: String = row.get("state");
        if row.get::<Uuid, _>("generation") == publication.generation && state == "complete" {
            let files: Vec<PublishedSessionFile> = serde_json::from_value(row.get("files"))?;
            if row.get::<Option<String>, _>("provider").as_deref()
                == Some(publication.provider.as_str())
                && row
                    .get::<Option<String>, _>("provider_session_id")
                    .as_deref()
                    == Some(publication.provider_session_id.as_str())
                && files == publication.files
            {
                // The previous response may have been lost after commit.
                return Ok(());
            }
        }
        if row.get::<Uuid, _>("generation") != publication.generation || state != "pending" {
            return Err(G5Error::Conflict {
                reason: "session bundle generation is no longer pending".into(),
            }
            .into());
        }
        let mut seen = std::collections::HashSet::new();
        for file in &publication.files {
            validate_session_file_path(publication.provider, &file.relative_path)
                .map_err(|error| validation_error(&error, "files"))?;
            validate_sha256_hex(&file.sha256).map_err(|error| validation_error(&error, "files"))?;
            if file.size < 0
                || file.size as u64 > CHAT_SESSION_FILE_MAX_BYTES
                || !seen.insert(&file.relative_path)
            {
                return Err(G5Error::Conflict {
                    reason: "invalid session bundle file set".into(),
                }
                .into());
            }
            let present: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM chat_session_files AS file
                 WHERE file.session_id = $1 AND file.provider = $2
                   AND file.provider_session_id = $3 AND file.relative_path = $4
                   AND file.sha256 = $5 AND file.size = $6
                   AND octet_length(file.bytes) + COALESCE((
                       SELECT sum(segment.size) FROM chat_session_file_segments AS segment
                       WHERE segment.session_id = file.session_id
                         AND segment.provider_session_id = file.provider_session_id
                         AND segment.relative_path = file.relative_path), 0) = file.size
                   AND (SELECT count(*) = COALESCE(max(segment_index), 0)
                        FROM chat_session_file_segments AS segment
                        WHERE segment.session_id = file.session_id
                          AND segment.provider_session_id = file.provider_session_id
                          AND segment.relative_path = file.relative_path))",
            )
            .bind(session_id)
            .bind(publication.provider.as_str())
            .bind(&publication.provider_session_id)
            .bind(&file.relative_path)
            .bind(&file.sha256)
            .bind(file.size)
            .fetch_one(&mut *conn)
            .await?;
            if !present {
                return Err(G5Error::Conflict {
                    reason: "session bundle file is missing or changed".into(),
                }
                .into());
            }
        }
        sqlx::query(
            "UPDATE chat_session_file_bundles SET state = 'complete', provider = $2,
             provider_session_id = $3, files = $4,
             prior_publication = NULL, updated_at = now()
             WHERE session_id = $1",
        )
        .bind(session_id)
        .bind(publication.provider.as_str())
        .bind(&publication.provider_session_id)
        .bind(serde_json::to_value(&publication.files)?)
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Every file stored for one provider session of a chat session,
    /// ordered by path.
    pub async fn manifest(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        provider_session_id: &str,
    ) -> Result<Vec<ChatSessionFileManifestItem>, AppError> {
        validate_provider_session_id(provider_session_id)
            .map_err(|error| validation_error(&error, "provider_session_id"))?;
        let rows = sqlx::query(
            r#"
            SELECT provider, provider_session_id, relative_path, sha256, size, updated_at
            FROM chat_session_files
            WHERE session_id = $1
              AND provider_session_id = $2
            ORDER BY relative_path ASC
            "#,
        )
        .bind(session_id)
        .bind(provider_session_id)
        .fetch_all(conn)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ChatSessionFileManifestItem {
                    provider: parse_stored_provider(row.get("provider"))?,
                    provider_session_id: row.get("provider_session_id"),
                    relative_path: row.get("relative_path"),
                    sha256: row.get("sha256"),
                    size: row.get("size"),
                    updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
                })
            })
            .collect()
    }

    /// One stored file with its bytes; `NotFound` when absent.
    pub async fn load(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        provider_session_id: &str,
        relative_path: &str,
    ) -> Result<StoredChatSessionFile, AppError> {
        validate_provider_session_id(provider_session_id)
            .map_err(|error| validation_error(&error, "provider_session_id"))?;
        let row = sqlx::query(
            r#"
            SELECT file.provider, file.sha256, file.size, file.bytes,
                   COALESCE(parts.indexes, ARRAY[]::integer[]) AS segment_indexes,
                   COALESCE(parts.contents, ARRAY[]::bytea[]) AS segment_bytes
            FROM chat_session_files AS file
            LEFT JOIN LATERAL (
                SELECT array_agg(segment_index ORDER BY segment_index) AS indexes,
                       array_agg(bytes ORDER BY segment_index) AS contents
                FROM chat_session_file_segments
                WHERE session_id = file.session_id
                  AND provider_session_id = file.provider_session_id
                  AND relative_path = file.relative_path
            ) AS parts ON true
            WHERE file.session_id = $1
              AND file.provider_session_id = $2
              AND file.relative_path = $3
            "#,
        )
        .bind(session_id)
        .bind(provider_session_id)
        .bind(relative_path)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| {
            AppError::from(G5Error::NotFound {
                resource: "chat_session_file".to_string(),
            })
        })?;
        let declared_size: i64 = row.get("size");
        let first_bytes: Vec<u8> = row.get("bytes");
        let mut bytes = Vec::with_capacity(declared_size as usize);
        bytes.extend_from_slice(&first_bytes);
        let segment_indexes: Vec<i32> = row.get("segment_indexes");
        let segment_bytes: Vec<Vec<u8>> = row.get("segment_bytes");
        if segment_indexes.len() != segment_bytes.len() {
            return Err(G5Error::SessionFileIntegrity {
                message: "stored session file has an incomplete segment set".into(),
            }
            .into());
        }
        for (index, (segment_index, segment)) in
            segment_indexes.iter().zip(segment_bytes.iter()).enumerate()
        {
            if *segment_index != (index + 1) as i32 {
                return Err(G5Error::SessionFileIntegrity {
                    message: "stored session file has a missing segment".into(),
                }
                .into());
            }
            bytes.extend_from_slice(segment);
        }
        let sha256: String = row.get("sha256");
        if i64::try_from(bytes.len()).ok() != Some(declared_size) || sha256_hex(&bytes) != sha256 {
            return Err(G5Error::SessionFileIntegrity {
                message: "stored session file size or digest does not match".into(),
            }
            .into());
        }
        Ok(StoredChatSessionFile {
            provider: parse_stored_provider(row.get("provider"))?,
            sha256,
            bytes,
        })
    }

    /// Insert or replace one file. Re-sending the stored bytes changes
    /// nothing and reports `changed: false`, so a retried upload is
    /// idempotent. A path already stored for the other provider is a
    /// conflict.
    ///
    /// A Claude path names the CLI's project key, the escaped working
    /// directory. Legacy uploads can still arrive under another key, so an
    /// upload replaces copies of the same session file under older keys.
    /// Verified resume itself requires the saved working directory.
    pub async fn upsert(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        upload: ChatSessionFileUpload<'_>,
        incarnation: Option<SessionFileIncarnation>,
        generation: Option<Uuid>,
    ) -> Result<PutChatSessionFileResponse, AppError> {
        validate_provider_session_id(upload.provider_session_id)
            .map_err(|error| validation_error(&error, "provider_session_id"))?;
        validate_session_file_path(upload.provider, upload.relative_path)
            .map_err(|error| validation_error(&error, "path"))?;
        validate_sha256_hex(upload.sha256).map_err(|error| validation_error(&error, "sha256"))?;
        if upload.bytes.len() as u64 > CHAT_SESSION_FILE_MAX_BYTES {
            return Err(validation_error(&SessionFileError::TooLarge, "body"));
        }
        let whole_hash = upload
            .base
            .is_none()
            .then(|| ResumableSha256::from_bytes(upload.bytes));
        if let Some((base_size, base_sha256)) = upload.base {
            validate_sha256_hex(base_sha256)
                .map_err(|error| validation_error(&error, "base_sha256"))?;
            if generation.is_none()
                || base_size < 0
                || (base_size as u64).saturating_add(upload.bytes.len() as u64)
                    > CHAT_SESSION_FILE_MAX_BYTES
            {
                return Err(G5Error::Conflict {
                    reason: "invalid session file append base or size".into(),
                }
                .into());
            }
        } else if whole_hash
            .as_ref()
            .expect("whole upload has hash state")
            .digest_hex()
            != upload.sha256
        {
            return Err(validation_error(
                &SessionFileError::DigestMismatch,
                "sha256",
            ));
        }

        let session = if let Some(generation) = generation {
            let incarnation = incarnation.ok_or_else(|| G5Error::Conflict {
                reason: "fenced upload requires a VM incarnation".into(),
            })?;
            sqlx::query_as(
                "SELECT session.org_id, session.session_manifest_generation
                 FROM chat_sessions AS session
                 JOIN chat_session_file_bundles AS bundle ON bundle.session_id = session.id
                 WHERE session.id = $1 AND bundle.generation = $2 AND bundle.state = 'pending'
                   AND bundle.fenced_vm_id = $3 AND bundle.fenced_epoch = $4
                 FOR UPDATE OF bundle",
            )
            .bind(session_id)
            .bind(generation)
            .bind(incarnation.vm_id.to_string())
            .bind(
                i64::try_from(incarnation.epoch).map_err(|_| G5Error::Conflict {
                    reason: "session bundle epoch exceeds bigint".into(),
                })?,
            )
            .fetch_optional(&mut *conn)
            .await?
        } else {
            // One-version-back workers still upload without a generation. Keep
            // their files available to the legacy manifest, but never let a
            // legacy upload overwrite the currently published native session.
            let published: Option<bool> = sqlx::query_scalar(
                "SELECT state = 'complete' AND provider = $2 AND provider_session_id = $3
                 FROM chat_session_file_bundles WHERE session_id = $1 FOR UPDATE",
            )
            .bind(session_id)
            .bind(upload.provider.as_str())
            .bind(upload.provider_session_id)
            .fetch_optional(&mut *conn)
            .await?;
            if published == Some(true) {
                return Err(G5Error::Conflict {
                    reason: "legacy upload cannot replace a published session bundle".into(),
                }
                .into());
            }
            sqlx::query_as(
                "SELECT org_id, session_manifest_generation FROM chat_sessions WHERE id = $1",
            )
            .bind(session_id)
            .fetch_optional(&mut *conn)
            .await?
        };
        let (org_id, manifest_generation): (Uuid, i32) = match session {
            Some(session) => session,
            None => {
                // The tenant-scoped session may exist while the requested
                // generation was fenced or completed. Report that as a
                // conflict without revealing another tenant's session.
                let visible: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id = $1)")
                        .bind(session_id)
                        .fetch_one(&mut *conn)
                        .await?;
                if generation.is_some() && visible {
                    return Err(G5Error::Conflict {
                        reason: "session bundle generation is no longer pending".into(),
                    }
                    .into());
                }
                return Err(G5Error::NotFound {
                    resource: format!("chat_session:{session_id}"),
                }
                .into());
            }
        };
        if let Some((base_size, base_sha256)) = upload.base {
            // The bundle row is already locked above. The file lock also
            // serializes appends from concurrent requests under this fence.
            let full_size = base_size + upload.bytes.len() as i64;
            let file = sqlx::query(
                "SELECT org_id, provider, size, sha256, sha256_state,
                        octet_length(bytes) AS first_size, manifest_generation,
                        harvested_from_vm_id, harvested_epoch, updated_at
                 FROM chat_session_files
                 WHERE session_id = $1 AND provider_session_id = $2 AND relative_path = $3
                 FOR UPDATE",
            )
            .bind(session_id)
            .bind(upload.provider_session_id)
            .bind(upload.relative_path)
            .fetch_optional(&mut *conn)
            .await?;
            let Some(file) = file else {
                return Err(G5Error::Conflict {
                    reason: "session file append base is missing".into(),
                }
                .into());
            };
            if file.get::<String, _>("provider") != upload.provider.as_str() {
                return Err(G5Error::Conflict {
                    reason: "session file append provider differs".into(),
                }
                .into());
            }
            let stored_size: i64 = file.get("size");
            let stored_sha256: String = file.get("sha256");
            let harvested_vm_id = incarnation.map(|value| value.vm_id.to_string());
            let harvested_epoch =
                incarnation.map(|value| i64::try_from(value.epoch).unwrap_or(i64::MAX));
            if stored_size == full_size
                && stored_sha256 == upload.sha256
                && file.get::<i32, _>("manifest_generation") == manifest_generation
                && file.get::<Option<String>, _>("harvested_from_vm_id") == harvested_vm_id
                && file.get::<Option<i64>, _>("harvested_epoch") == harvested_epoch
            {
                return Ok(PutChatSessionFileResponse {
                    provider: upload.provider,
                    provider_session_id: upload.provider_session_id.to_string(),
                    relative_path: upload.relative_path.to_string(),
                    sha256: upload.sha256.to_string(),
                    size: full_size,
                    changed: false,
                    updated_at: file.get("updated_at"),
                });
            }
            let first_size: i32 = file.get("first_size");
            let Some(mut hash) = file
                .get::<Option<Vec<u8>>, _>("sha256_state")
                .as_deref()
                .and_then(ResumableSha256::decode)
            else {
                // A pre-migration row takes the worker's existing one-time
                // whole-upload fallback, which also segments large files.
                return Err(G5Error::Conflict {
                    reason: "session file append needs a whole upload".into(),
                }
                .into());
            };
            if stored_size != base_size
                || stored_sha256 != base_sha256
                || hash.len() != base_size as u64
                || hash.digest_hex() != base_sha256
                || first_size < 0
                || first_size as usize > SESSION_FILE_SEGMENT_BYTES
            {
                return Err(G5Error::Conflict {
                    reason: "session file append base or digest did not match".into(),
                }
                .into());
            }
            let segments = sqlx::query(
                "SELECT COALESCE(sum(size), 0)::bigint AS total_size,
                        count(*)::bigint AS segment_count,
                        COALESCE(max(segment_index), 0) AS last_index
                 FROM chat_session_file_segments
                 WHERE session_id = $1 AND provider_session_id = $2 AND relative_path = $3",
            )
            .bind(session_id)
            .bind(upload.provider_session_id)
            .bind(upload.relative_path)
            .fetch_one(&mut *conn)
            .await?;
            let child_size: i64 = segments.get("total_size");
            let child_count: i64 = segments.get("segment_count");
            let mut last_index: i32 = segments.get("last_index");
            if i64::from(first_size) + child_size != base_size
                || child_count != i64::from(last_index)
                || (child_count > 0 && first_size as usize != SESSION_FILE_SEGMENT_BYTES)
            {
                return Err(G5Error::Conflict {
                    reason: "session file segments do not match the append base".into(),
                }
                .into());
            }
            hash.update(upload.bytes).ok_or_else(|| G5Error::Conflict {
                reason: "session file digest length overflow".into(),
            })?;
            if hash.digest_hex() != upload.sha256 {
                return Err(G5Error::Conflict {
                    reason: "session file append resulting digest did not match".into(),
                }
                .into());
            }
            let mut remaining = upload.bytes;
            if last_index == 0 && first_size as usize != SESSION_FILE_SEGMENT_BYTES {
                let take = remaining
                    .len()
                    .min(SESSION_FILE_SEGMENT_BYTES - first_size as usize);
                if take > 0 {
                    sqlx::query(
                        "UPDATE chat_session_files SET bytes = bytes || $4, size = $5
                         WHERE session_id = $1 AND provider_session_id = $2 AND relative_path = $3",
                    )
                    .bind(session_id)
                    .bind(upload.provider_session_id)
                    .bind(upload.relative_path)
                    .bind(&remaining[..take])
                    .bind(full_size)
                    .execute(&mut *conn)
                    .await?;
                    remaining = &remaining[take..];
                }
            }
            if last_index > 0 && !remaining.is_empty() {
                let last_size: i32 = sqlx::query_scalar(
                    "SELECT size FROM chat_session_file_segments
                     WHERE session_id = $1 AND provider_session_id = $2
                       AND relative_path = $3 AND segment_index = $4",
                )
                .bind(session_id)
                .bind(upload.provider_session_id)
                .bind(upload.relative_path)
                .bind(last_index)
                .fetch_one(&mut *conn)
                .await?;
                let take = remaining
                    .len()
                    .min(SESSION_FILE_SEGMENT_BYTES - last_size as usize);
                if take > 0 {
                    sqlx::query(
                        "UPDATE chat_session_file_segments
                         SET bytes = bytes || $5, size = size + $6
                         WHERE session_id = $1 AND provider_session_id = $2
                           AND relative_path = $3 AND segment_index = $4",
                    )
                    .bind(session_id)
                    .bind(upload.provider_session_id)
                    .bind(upload.relative_path)
                    .bind(last_index)
                    .bind(&remaining[..take])
                    .bind(take as i32)
                    .execute(&mut *conn)
                    .await?;
                    remaining = &remaining[take..];
                }
            }
            for chunk in remaining.chunks(SESSION_FILE_SEGMENT_BYTES) {
                last_index += 1;
                sqlx::query(
                    "INSERT INTO chat_session_file_segments
                     (session_id, org_id, provider_session_id, relative_path,
                      segment_index, bytes, size) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(session_id)
                .bind(file.get::<Uuid, _>("org_id"))
                .bind(upload.provider_session_id)
                .bind(upload.relative_path)
                .bind(last_index)
                .bind(chunk)
                .bind(chunk.len() as i32)
                .execute(&mut *conn)
                .await?;
            }
            let updated_at: DateTime<Utc> = sqlx::query_scalar(
                "UPDATE chat_session_files
                 SET sha256 = $4, sha256_state = $5, size = $6,
                     harvested_from_vm_id = $7, harvested_epoch = $8,
                     manifest_generation = $9, updated_at = now()
                 WHERE session_id = $1 AND provider_session_id = $2 AND relative_path = $3
                 RETURNING updated_at",
            )
            .bind(session_id)
            .bind(upload.provider_session_id)
            .bind(upload.relative_path)
            .bind(upload.sha256)
            .bind(hash.encode())
            .bind(full_size)
            .bind(harvested_vm_id)
            .bind(harvested_epoch)
            .bind(manifest_generation)
            .fetch_one(&mut *conn)
            .await?;
            return Ok(PutChatSessionFileResponse {
                provider: upload.provider,
                provider_session_id: upload.provider_session_id.to_string(),
                relative_path: upload.relative_path.to_string(),
                sha256: upload.sha256.to_string(),
                size: full_size,
                changed: true,
                updated_at,
            });
        }
        let size = upload.bytes.len() as i64;
        let first_bytes = &upload.bytes[..upload.bytes.len().min(SESSION_FILE_SEGMENT_BYTES)];
        let hash_state = whole_hash.expect("whole upload has hash state").encode();
        let harvested_vm_id = incarnation.map(|value| value.vm_id.to_string());
        let harvested_epoch =
            incarnation.map(|value| i64::try_from(value.epoch).unwrap_or(i64::MAX));
        let written = sqlx::query_scalar::<_, DateTime<Utc>>(
            r#"
            INSERT INTO chat_session_files
                (session_id, org_id, provider, provider_session_id, relative_path,
                 bytes, sha256, sha256_state, size, harvested_from_vm_id,
                 harvested_epoch, manifest_generation)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (session_id, provider_session_id, relative_path)
            DO UPDATE SET
                bytes = EXCLUDED.bytes,
                sha256 = EXCLUDED.sha256,
                sha256_state = EXCLUDED.sha256_state,
                size = EXCLUDED.size,
                harvested_from_vm_id = EXCLUDED.harvested_from_vm_id,
                harvested_epoch = EXCLUDED.harvested_epoch,
                manifest_generation = EXCLUDED.manifest_generation,
                updated_at = now()
            WHERE chat_session_files.provider = EXCLUDED.provider
              AND (EXCLUDED.harvested_from_vm_id IS NOT NULL
                   OR chat_session_files.harvested_from_vm_id IS NULL)
              AND (chat_session_files.sha256 <> EXCLUDED.sha256
                   OR chat_session_files.sha256_state IS NULL
                   OR octet_length(chat_session_files.bytes) > 8388608
                   OR chat_session_files.harvested_from_vm_id
                        IS DISTINCT FROM EXCLUDED.harvested_from_vm_id
                   OR chat_session_files.harvested_epoch
                        IS DISTINCT FROM EXCLUDED.harvested_epoch
                   OR chat_session_files.manifest_generation
                        IS DISTINCT FROM EXCLUDED.manifest_generation)
            RETURNING updated_at
            "#,
        )
        .bind(session_id)
        .bind(org_id)
        .bind(upload.provider.as_str())
        .bind(upload.provider_session_id)
        .bind(upload.relative_path)
        .bind(first_bytes)
        .bind(upload.sha256)
        .bind(hash_state)
        .bind(size)
        .bind(harvested_vm_id)
        .bind(harvested_epoch)
        .bind(manifest_generation)
        .fetch_optional(&mut *conn)
        .await?;

        if written.is_some() {
            sqlx::query(
                "DELETE FROM chat_session_file_segments
                 WHERE session_id = $1 AND provider_session_id = $2 AND relative_path = $3",
            )
            .bind(session_id)
            .bind(upload.provider_session_id)
            .bind(upload.relative_path)
            .execute(&mut *conn)
            .await?;
            for (index, chunk) in upload.bytes[first_bytes.len()..]
                .chunks(SESSION_FILE_SEGMENT_BYTES)
                .enumerate()
            {
                sqlx::query(
                    "INSERT INTO chat_session_file_segments
                     (session_id, org_id, provider_session_id, relative_path,
                      segment_index, bytes, size) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(session_id)
                .bind(org_id)
                .bind(upload.provider_session_id)
                .bind(upload.relative_path)
                .bind((index + 1) as i32)
                .bind(chunk)
                .bind(chunk.len() as i32)
                .execute(&mut *conn)
                .await?;
            }
        }

        let replaced_other_keys =
            match claude_session_file_suffix(upload.provider, upload.relative_path) {
                Some(suffix) => {
                    sqlx::query(
                        r#"
                    DELETE FROM chat_session_files
                    WHERE session_id = $1
                      AND provider_session_id = $2
                      AND provider = 'claude_code'
                      AND relative_path <> $3
                      AND substring(relative_path FROM '^projects/[^/]+/(.+)$') = $4
                    "#,
                    )
                    .bind(session_id)
                    .bind(upload.provider_session_id)
                    .bind(upload.relative_path)
                    .bind(suffix)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected()
                        > 0
                }
                None => false,
            };

        let response = |changed: bool, updated_at: DateTime<Utc>| PutChatSessionFileResponse {
            provider: upload.provider,
            provider_session_id: upload.provider_session_id.to_string(),
            relative_path: upload.relative_path.to_string(),
            sha256: upload.sha256.to_string(),
            size,
            changed,
            updated_at,
        };
        if let Some(updated_at) = written {
            return Ok(response(true, updated_at));
        }

        // Nothing was written, so the row exists and the conflict clause
        // holds its lock for the rest of the transaction: it already has
        // these bytes, or it belongs to the other provider.
        let existing = sqlx::query(
            r#"
            SELECT provider, sha256, updated_at
            FROM chat_session_files
            WHERE session_id = $1
              AND provider_session_id = $2
              AND relative_path = $3
            "#,
        )
        .bind(session_id)
        .bind(upload.provider_session_id)
        .bind(upload.relative_path)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| {
            AppError::from(G5Error::Internal {
                message: "chat session file upsert wrote no row and found none".to_string(),
            })
        })?;
        let stored_provider = parse_stored_provider(existing.get("provider"))?;
        if stored_provider != upload.provider {
            return Err(G5Error::Conflict {
                reason: format!(
                    "session file path is already stored for provider {stored_provider}"
                ),
            }
            .into());
        }
        let stored_sha256: String = existing.get("sha256");
        if stored_sha256 != upload.sha256 {
            return Err(G5Error::Internal {
                message: "chat session file upsert skipped a changed file".to_string(),
            }
            .into());
        }
        Ok(response(replaced_other_keys, existing.get("updated_at")))
    }
}

/// The part of a Claude session file path after `projects/<project key>/`.
fn claude_session_file_suffix(provider: SessionFileProvider, relative_path: &str) -> Option<&str> {
    if provider != SessionFileProvider::ClaudeCode {
        return None;
    }
    let (_project_key, suffix) = relative_path.strip_prefix("projects/")?.split_once('/')?;
    (!suffix.is_empty()).then_some(suffix)
}

fn parse_stored_provider(value: String) -> Result<SessionFileProvider, AppError> {
    SessionFileProvider::parse(&value).ok_or_else(|| {
        G5Error::Internal {
            message: format!("chat_session_files row has an unknown provider {value:?}"),
        }
        .into()
    })
}

/// A validation error (HTTP 422) whose details name the offending field and
/// the rule it broke.
pub fn validation_error(error: &SessionFileError, field: &str) -> AppError {
    let code = match error {
        SessionFileError::InvalidProviderSessionId => "session_file_provider_session_id_invalid",
        SessionFileError::InvalidPath { .. } => "session_file_path_invalid",
        SessionFileError::InvalidSha256 => "session_file_sha256_invalid",
        SessionFileError::TooLarge => "session_file_too_large",
        SessionFileError::InvalidBody(_) => "session_file_body_invalid",
        SessionFileError::DigestMismatch => "session_file_digest_mismatch",
    };
    G5Error::ValidationError {
        details: json!({
            "code": code,
            "field": field,
            "message": error.to_string(),
        }),
    }
    .into()
}
