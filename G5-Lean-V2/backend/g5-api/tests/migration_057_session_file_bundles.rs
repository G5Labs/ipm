//! The complete-bundle marker is tenant-scoped durable state, not a VM cache.
use crate::helpers;

use g5_api_core::services::chat_session_file_service::{
    ChatSessionFileService, ChatSessionFileUpload, SessionFileIncarnation,
};
use g5_authz::{AuthzService, InProcessAuthz, ScopedDb};
use g5_contracts::types::ids::{OrgId, UserId};
use g5_internal_api::dtos::{
    PublishedSessionFile, SessionFileBundleBegin, SessionFileBundlePublication,
};
use g5_internal_api::session_files::{sha256_hex, SessionFileProvider};
use helpers::TestDb;
use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

fn append_upload<'a>(
    provider_session_id: &'a str,
    relative_path: &'a str,
    suffix: &'a [u8],
    base_size: i64,
    base_sha256: &'a str,
    target_sha256: &'a str,
) -> ChatSessionFileUpload<'a> {
    ChatSessionFileUpload {
        provider: SessionFileProvider::Codex,
        provider_session_id,
        relative_path,
        sha256: target_sha256,
        bytes: suffix,
        base: Some((base_size, base_sha256)),
    }
}

#[tokio::test]
async fn bundle_publication_table_is_forced_rls_and_tenant_scoped() {
    let db = TestDb::create().await;
    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let flags = sqlx::query(
            "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'public.chat_session_file_bundles'::regclass",
        ).fetch_one(&db.pool).await?;
        assert!(flags.get::<bool, _>("relrowsecurity"));
        assert!(flags.get::<bool, _>("relforcerowsecurity"));

        let mut sessions = Vec::new();
        for n in 0..2 {
            let org = Uuid::new_v4();
            let user = Uuid::new_v4();
            let workspace = Uuid::new_v4();
            let session = Uuid::new_v4();
            let vm_id = Uuid::new_v4();
            let mut identity_tx = db.pool.begin().await?;
            sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Bundle test',$2)")
                .bind(org).bind(format!("bundle-test-{org}")).execute(&mut *identity_tx).await?;
            sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Bundle owner')")
                .bind(user).bind(format!("bundle-{n}-{user}@example.test")).execute(&mut *identity_tx).await?;
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
                .bind(org).bind(user).execute(&mut *identity_tx).await?;
            identity_tx.commit().await?;
            sqlx::query("INSERT INTO workspaces(id,org_id,name,created_by) VALUES($1,$2,$3,$4)")
                .bind(workspace).bind(org).bind(format!("bundle-{n}")).bind(user).execute(&db.pool).await?;
            sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
                .bind(session).bind(org).bind(workspace).bind(user).execute(&db.pool).await?;
            sqlx::query("INSERT INTO chat_session_file_bundles(session_id,org_id,generation,state,fenced_vm_id,fenced_epoch) VALUES($1,$2,$3,'pending',$4,1)")
                .bind(session).bind(org).bind(Uuid::new_v4()).bind(vm_id.to_string()).execute(&db.pool).await?;
            sessions.push((org, user, session, vm_id));
        }

        let system = PgPoolOptions::new().connect(&db.system_url).await?;
        let authz = PgPoolOptions::new().connect(&db.authz_url).await?;
        let tenant = PgPoolOptions::new().connect(&db.tenant_url).await?;
        let service = InProcessAuthz::new(system.clone(), authz.clone());
        let (org, user, own_session, vm_id) = sessions[0];
        let incarnation = SessionFileIncarnation { vm_id, epoch: 1 };
        let cap = service.mint_db_capability(UserId::from_uuid(user), &[OrgId::from_uuid(org)], &["*".into()]).await?;
        let mut scoped = ScopedDb::from_capability(&tenant, cap).await?;
        let visible: Vec<Uuid> = sqlx::query_scalar("SELECT session_id FROM chat_session_file_bundles")
            .fetch_all(scoped.conn()).await?;
        assert_eq!(visible, vec![own_session]);
        let generation: Uuid = sqlx::query_scalar("SELECT generation FROM chat_session_file_bundles WHERE session_id = $1")
            .bind(own_session).fetch_one(scoped.conn()).await?;
        let store = ChatSessionFileService::new();
        let provider_session_id = Uuid::new_v4().to_string();
        let relative_path = format!("sessions/2026/10/02/rollout-x-{provider_session_id}.jsonl");
        let bytes = b"tool result: ORCHID-73";
        let sha256 = sha256_hex(bytes);
        let publication = SessionFileBundlePublication {
            generation,
            provider: SessionFileProvider::Codex,
            provider_session_id: provider_session_id.clone(),
            files: vec![PublishedSessionFile { relative_path: relative_path.clone(), sha256: sha256.clone(), size: bytes.len() as i64 }],
        };
        assert!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read pending bundle").is_none());
        assert!(store.publish_bundle(scoped.conn(), own_session, &publication, incarnation).await.is_err(), "missing file cannot publish");
        store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &sha256,
            bytes,
            base: None,
        }, Some(incarnation), Some(generation)).await.expect("upload bundle file");
        store.publish_bundle(scoped.conn(), own_session, &publication, incarnation).await.expect("publish complete bundle");
        let legacy_id = Uuid::new_v4().to_string();
        let legacy_path = format!("sessions/2026/10/02/rollout-x-{legacy_id}.jsonl");
        store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &legacy_id,
            relative_path: &legacy_path,
            sha256: &sha256,
            bytes,
            base: None,
        }, None, None).await.expect("previous worker upload remains accepted");
        assert!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &legacy_id).await.expect("legacy upload has no publication").is_none());
        assert!(store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &sha256,
            bytes,
            base: None,
        }, None, None).await.is_err(), "legacy upload cannot alter the published native session");
        assert!(store.begin_bundle(scoped.conn(), own_session, &SessionFileBundleBegin {
            generation: Uuid::new_v4(), task_id: Uuid::new_v4(), attempt: 1,
            reservation_id: Uuid::new_v4(), cwd_relative_path: String::new(),
        }, SessionFileIncarnation { vm_id: Uuid::new_v4(), epoch: 1 }).await.is_err(), "an unrelated task cannot invalidate the current bundle");
        assert_eq!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read complete bundle"), Some(publication.clone()));
        let workspace: Uuid = sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = $1")
            .bind(own_session).fetch_one(&db.pool).await?;
        let task_id = Uuid::new_v4();
        let reservation_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,spec,attempt,chat_session_id,created_by,vm_id,epoch)
             VALUES($1,$2,$3,'Bundle turn','agent','running',$4,1,$5,$6,$7,1)",
        )
        .bind(task_id).bind(org).bind(workspace)
        .bind(serde_json::json!({"agent":{"schemaVersion":"g5.agent-task-spec/v1","agentLaunch":{"schemaVersion":1}}}))
        .bind(own_session).bind(user).bind(vm_id.to_string()).execute(&db.pool).await?;
        sqlx::query(
            "INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at)
             VALUES($1,$2,1,$3,1,false,now())",
        )
        .bind(org).bind(task_id).bind(vm_id.to_string())
        .execute(&db.pool).await?;
        sqlx::query(
            "INSERT INTO session_inputs(id,org_id,session_id,input_index,content,input_origin,delivery,
             reservation_id,reservation_revision,task_id,attempt)
             VALUES($1,$2,$3,0,'Continue','human','reserved',$4,1,$5,1)",
        )
        .bind(Uuid::new_v4()).bind(org).bind(own_session).bind(reservation_id).bind(task_id)
        .execute(&db.pool).await?;
        let next_generation = Uuid::new_v4();
        store.begin_bundle(scoped.conn(), own_session, &SessionFileBundleBegin {
            generation: next_generation, task_id, attempt: 1, reservation_id,
            cwd_relative_path: String::new(),
        }, incarnation).await.expect("active reserved turn fences the published bundle");
        let wrong_incarnation = SessionFileIncarnation { vm_id: Uuid::new_v4(), epoch: 1 };
        assert!(store.pending_bundles(scoped.conn(), workspace, incarnation, Some(own_session))
            .await.expect("pending lookup").is_empty(), "a running attempt cannot complete its pending set");
        sqlx::query("UPDATE chat_session_file_bundles SET provider = 'codex', provider_session_id = $2 WHERE session_id = $1")
            .bind(own_session).bind(&provider_session_id).execute(scoped.conn()).await?;
        sqlx::query(
            "INSERT INTO task_events(id,org_id,task_id,entry_type,content,metadata,attempt,vm_id,seq)
             VALUES($1,$2,$3,'output','', $4,1,$5,1)",
        )
        .bind(Uuid::new_v4()).bind(org).bind(task_id)
        .bind(serde_json::json!({"kind":"invocation.outcome","terminal":true}))
        .bind(vm_id.to_string()).execute(scoped.conn()).await?;
        assert!(store.pending_bundles(scoped.conn(), workspace, incarnation, Some(own_session))
            .await.expect("pending lookup").is_empty(), "an outcome without attempt settlement cannot authorize late harvest");
        sqlx::query("UPDATE task_attempts SET ended_at = started_at + interval '1 second', reason = 'interrupted', outcome = '{}'::jsonb WHERE task_id = $1 AND attempt = 1")
            .bind(task_id).execute(scoped.conn()).await?;
        assert_eq!(store.pending_bundles(scoped.conn(), workspace, incarnation, Some(own_session)).await.expect("pending lookup").len(), 1);
        assert!(store.pending_bundles(scoped.conn(), workspace, wrong_incarnation, Some(own_session)).await.expect("pending lookup").is_empty(),
            "a replacement VM cannot complete the old generation");
        assert!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read newly pending bundle").is_none());
        let stale_upload = store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &sha256,
            bytes,
            base: None,
        }, Some(incarnation), Some(generation)).await;
        assert!(matches!(stale_upload, Err(ref error) if matches!(error.inner, g5_contracts::error::G5Error::Conflict { .. })),
            "old generation must report conflict after begin: {stale_upload:?}");
        assert!(store.publish_bundle(scoped.conn(), own_session, &publication, incarnation).await.is_err(),
            "old generation cannot publish after begin");
        assert!(store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &sha256,
            bytes,
            base: None,
        }, Some(wrong_incarnation), Some(next_generation)).await.is_err(),
            "a replacement VM cannot upload into the pending generation");
        assert!(store.publish_bundle(scoped.conn(), own_session, &SessionFileBundlePublication {
            generation: next_generation, ..publication.clone()
        }, wrong_incarnation).await.is_err(),
            "a replacement VM cannot publish the pending generation");
        assert!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read after stale attempts").is_none());
        let suffix = b"\nsecond turn";
        let full_bytes = [bytes.as_slice(), suffix.as_slice()].concat();
        let full_sha256 = sha256_hex(&full_bytes);
        assert!(store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, suffix, bytes.len() as i64, &sha256_hex(b"stale"), &full_sha256),
            Some(incarnation), Some(next_generation)).await.is_err(), "stale append base must fail");
        assert!(store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, suffix, bytes.len() as i64, &sha256, &sha256_hex(b"wrong result")),
            Some(incarnation), Some(next_generation)).await.is_err(), "wrong full digest must fail");
        assert!(store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, suffix, g5_internal_api::session_files::CHAT_SESSION_FILE_MAX_BYTES as i64, &sha256, &full_sha256),
            Some(incarnation), Some(next_generation)).await.is_err(), "append beyond cap must fail");
        let appended = store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, suffix, bytes.len() as i64, &sha256, &full_sha256),
            Some(incarnation), Some(next_generation)).await.expect("append the new bytes");
        assert!(appended.changed);
        let retried_append = store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, suffix, bytes.len() as i64, &sha256, &full_sha256),
            Some(incarnation), Some(next_generation)).await.expect("lost append response can be retried");
        assert!(!retried_append.changed);
        let stored = store.load(scoped.conn(), own_session, &provider_session_id, &relative_path)
            .await.expect("read appended content");
        assert_eq!(stored.bytes, full_bytes);
        assert_eq!(stored.sha256, full_sha256);
        let next_suffix = b"\nthird turn";
        let extended_bytes = [full_bytes.as_slice(), next_suffix.as_slice()].concat();
        let extended_sha256 = sha256_hex(&extended_bytes);
        let second_append = store.upsert(scoped.conn(), own_session,
            append_upload(&provider_session_id, &relative_path, next_suffix, full_bytes.len() as i64, &full_sha256, &extended_sha256),
            Some(incarnation), Some(next_generation)).await.expect("a later turn appends to the full prior file");
        assert!(second_append.changed);
        assert_eq!(store.load(scoped.conn(), own_session, &provider_session_id, &relative_path)
            .await.expect("read second append").bytes, extended_bytes);
        let replacement = store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &full_sha256,
            bytes: &full_bytes,
            base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("whole-file rewrite after appends");
        assert!(replacement.changed);
        let overwritten = store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex,
            provider_session_id: &provider_session_id,
            relative_path: &relative_path,
            sha256: &full_sha256,
            bytes: &full_bytes,
            base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("whole upload remains idempotent");
        assert!(!overwritten.changed);
        let next_publication = SessionFileBundlePublication {
            generation: next_generation,
            files: vec![PublishedSessionFile { relative_path: relative_path.clone(), sha256: full_sha256, size: full_bytes.len() as i64 }],
            ..publication
        };
        store.publish_bundle(scoped.conn(), own_session, &next_publication, incarnation).await.expect("current generation publishes");
        assert_eq!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read next complete bundle"), Some(next_publication.clone()));
        store.publish_bundle(scoped.conn(), own_session, &next_publication, incarnation).await.expect("same publication retries after a lost response");
        assert_eq!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id).await.expect("read after retry"), Some(next_publication.clone()));
        let mut altered_publication = next_publication.clone();
        altered_publication.files[0].size += 1;
        assert!(store.publish_bundle(scoped.conn(), own_session, &altered_publication, incarnation).await.is_err(),
            "a completed generation cannot change its file set");
        sqlx::query("UPDATE tasks SET status = 'succeeded' WHERE id = $1")
            .bind(task_id).execute(scoped.conn()).await?;
        let undelivered_task = Uuid::new_v4();
        let undelivered_reservation = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,spec,attempt,chat_session_id,created_by,vm_id,epoch)
             VALUES($1,$2,$3,'Undelivered bundle turn','agent','running',$4,1,$5,$6,$7,1)",
        )
        .bind(undelivered_task).bind(org).bind(workspace)
        .bind(serde_json::json!({"agent":{"schemaVersion":"g5.agent-task-spec/v1","agentLaunch":{"schemaVersion":1}}}))
        .bind(own_session).bind(user).bind(vm_id.to_string()).execute(scoped.conn()).await?;
        sqlx::query(
            "INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at)
             VALUES($1,$2,1,$3,1,false,now())",
        )
        .bind(org).bind(undelivered_task).bind(vm_id.to_string()).execute(scoped.conn()).await?;
        sqlx::query(
            "INSERT INTO session_inputs(id,org_id,session_id,input_index,content,input_origin,delivery,
             reservation_id,reservation_revision,task_id,attempt)
             VALUES($1,$2,$3,1,'Retry','human','reserved',$4,1,$5,1)",
        )
        .bind(Uuid::new_v4()).bind(org).bind(own_session).bind(undelivered_reservation)
        .bind(undelivered_task).execute(scoped.conn()).await?;
        let fence = |generation| SessionFileBundleBegin {
            generation, task_id: undelivered_task, attempt: 1,
            reservation_id: undelivered_reservation, cwd_relative_path: String::new(),
        };
        let undelivered_generation = Uuid::new_v4();
        store.begin_bundle(scoped.conn(), own_session, &fence(undelivered_generation), incarnation)
            .await.expect("fence before failed spawn");
        store.begin_bundle(scoped.conn(), own_session, &fence(undelivered_generation), incarnation)
            .await.expect("lost begin response can be retried");
        let prior_generation: Option<Uuid> = sqlx::query_scalar(
            "SELECT (prior_publication ->> 'generation')::uuid FROM chat_session_file_bundles WHERE session_id = $1",
        )
        .bind(own_session).fetch_one(scoped.conn()).await?;
        assert_eq!(prior_generation, Some(next_generation), "retry preserves the undo publication");
        let mismatched_retry = SessionFileBundleBegin {
            cwd_relative_path: "different-directory".into(),
            ..fence(undelivered_generation)
        };
        assert!(store.begin_bundle(scoped.conn(), own_session, &mismatched_retry, incarnation)
            .await.is_err(), "a reused generation with a different fence is refused");
        assert!(store.restore_if_undelivered(scoped.conn(), own_session, undelivered_task, 1, incarnation)
            .await.expect("undo positively undelivered fence"));
        assert_eq!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::Codex, &provider_session_id)
            .await.expect("restored prior publication"), Some(next_publication.clone()));
        store.begin_bundle(scoped.conn(), own_session, &fence(Uuid::new_v4()), incarnation)
            .await.expect("fence before provider reports a session");
        sqlx::query("UPDATE chat_session_file_bundles SET provider = 'codex', provider_session_id = $2 WHERE session_id = $1")
            .bind(own_session).bind(&provider_session_id).execute(scoped.conn()).await?;
        assert!(!store.restore_if_undelivered(scoped.conn(), own_session, undelivered_task, 1, incarnation)
            .await.expect("reported session keeps the fence"));
        let claude_generation = Uuid::new_v4();
        let claude_session = Uuid::new_v4().to_string();
        let claude_path = format!("projects/-work-repo/{claude_session}/tool-results/fact.txt");
        let claude_resume_path = format!("projects/-work-repo/{claude_session}.jsonl");
        sqlx::query("UPDATE chat_session_file_bundles SET generation = $2, provider = NULL, provider_session_id = NULL, files = '[]'::jsonb WHERE session_id = $1")
            .bind(own_session).bind(claude_generation).execute(scoped.conn()).await?;
        let mut claude_files = Vec::new();
        for (path, bytes) in [(&claude_resume_path, b"native transcript".as_slice()),
                              (&claude_path, b"tool-only fact".as_slice())] {
            let sha = sha256_hex(bytes);
            store.upsert(scoped.conn(), own_session, ChatSessionFileUpload {
                provider: SessionFileProvider::ClaudeCode,
                provider_session_id: &claude_session,
                relative_path: path,
                sha256: &sha,
                bytes,
                base: None,
            }, Some(incarnation), Some(claude_generation)).await.expect("upload Claude file");
            claude_files.push(PublishedSessionFile {
                relative_path: path.clone(), sha256: sha, size: bytes.len() as i64,
            });
        }
        let claude_publication = SessionFileBundlePublication {
            generation: claude_generation, provider: SessionFileProvider::ClaudeCode,
            provider_session_id: claude_session.clone(), files: claude_files,
        };
        store.publish_bundle(scoped.conn(), own_session, &claude_publication, incarnation)
            .await.expect("publish Claude transcript and referenced tool result");
        assert_eq!(store.published_bundle(scoped.conn(), own_session, SessionFileProvider::ClaudeCode, &claude_session)
            .await.expect("read Claude publication"), Some(claude_publication));
        scoped.commit().await?;
        // The fixture made one pending row per org above; free the foreign
        // session key so this assertion exercises the same-org FK, not PK.
        sqlx::query("DELETE FROM chat_session_file_bundles WHERE session_id = $1")
            .bind(sessions[1].2)
            .execute(&db.pool)
            .await?;
        let foreign_cap = service.mint_db_capability(UserId::from_uuid(user), &[OrgId::from_uuid(org)], &["*".into()]).await?;
        let mut foreign_scoped = ScopedDb::from_capability(&tenant, foreign_cap).await?;
        let foreign_insert = sqlx::query(
            "INSERT INTO chat_session_file_bundles(session_id,org_id,generation,state)
             VALUES($1,$2,$3,'pending')",
        )
        .bind(sessions[1].2)
        .bind(org)
        .bind(Uuid::new_v4())
        .execute(foreign_scoped.conn())
        .await;
        assert_eq!(
            foreign_insert.unwrap_err().as_database_error().and_then(|error| error.code()).as_deref(),
            Some("23503"),
            "the same-org foreign key must reject cross-org bundle attachment"
        );
        foreign_scoped.rollback().await?;
        let foreign_insert = sqlx::query(
            "INSERT INTO chat_session_file_bundles(session_id,org_id,generation,state) VALUES($1,$2,$3,'pending')"
        ).bind(sessions[1].2).bind(org).bind(Uuid::new_v4()).execute(&db.pool).await;
        assert_eq!(foreign_insert.unwrap_err().as_database_error().and_then(|error| error.code()).as_deref(),
            Some("23503"), "the operator still cannot bypass the same-org foreign key");
        tenant.close().await;
        authz.close().await;
        system.close().await;
        Ok(())
    }.await;
    db.drop().await;
    result.unwrap();
}
