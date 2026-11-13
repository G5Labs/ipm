//! Bounded session-file storage and resumable digest invariants in real Postgres.
use crate::helpers;

use g5_api_core::services::chat_session_file_service::{
    ChatSessionFileService, ChatSessionFileUpload, SessionFileIncarnation,
};
use g5_authz::{AuthzService, InProcessAuthz, ScopedDb};
use g5_contracts::types::ids::{OrgId, UserId};
use g5_internal_api::dtos::{PublishedSessionFile, SessionFileBundlePublication};
use g5_internal_api::session_files::{sha256_hex, SessionFileProvider};
use helpers::TestDb;
use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

const SEGMENT: usize = 8 * 1024 * 1024;

#[tokio::test]
async fn tenant_role_can_append_and_publish_a_segmented_file() {
    let db = TestDb::create().await;
    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let org = Uuid::new_v4();
        let user = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let session = Uuid::new_v4();
        let generation = Uuid::new_v4();
        let incarnation = SessionFileIncarnation { vm_id: Uuid::new_v4(), epoch: 1 };
        let mut identity_tx = db.pool.begin().await?;
        sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Segment tenant',$2)")
            .bind(org).bind(format!("segment-tenant-{org}")).execute(&mut *identity_tx).await?;
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Segment tenant')")
            .bind(user).bind(format!("segment-tenant-{user}@example.test")).execute(&mut *identity_tx).await?;
        sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
            .bind(org).bind(user).execute(&mut *identity_tx).await?;
        identity_tx.commit().await?;
        sqlx::query("INSERT INTO workspaces(id,org_id,name,created_by) VALUES($1,$2,'segment-tenant',$3)")
            .bind(workspace).bind(org).bind(user).execute(&db.pool).await?;
        sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
            .bind(session).bind(org).bind(workspace).bind(user).execute(&db.pool).await?;
        sqlx::query("INSERT INTO chat_session_file_bundles(session_id,org_id,generation,state,fenced_vm_id,fenced_epoch) VALUES($1,$2,$3,'pending',$4,1)")
            .bind(session).bind(org).bind(generation).bind(incarnation.vm_id.to_string())
            .execute(&db.pool).await?;

        let system = PgPoolOptions::new().connect(&db.system_url).await?;
        let authz = PgPoolOptions::new().connect(&db.authz_url).await?;
        let tenant = PgPoolOptions::new().connect(&db.tenant_url).await?;
        let authz_service = InProcessAuthz::new(system.clone(), authz.clone());
        let cap = authz_service.mint_db_capability(UserId::from_uuid(user), &[OrgId::from_uuid(org)], &["*".into()]).await?;
        let mut scoped = ScopedDb::from_capability(&tenant, cap).await?;
        let store = ChatSessionFileService::new();
        let provider_session_id = Uuid::new_v4().to_string();
        let path = format!("sessions/2026/10/03/rollout-x-{provider_session_id}.jsonl");
        let first = vec![b'T'; SEGMENT + 2];
        let first_sha = sha256_hex(&first);
        store.upsert(scoped.conn(), session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session_id,
            relative_path: &path, sha256: &first_sha, bytes: &first, base: None,
        }, Some(incarnation), Some(generation)).await.expect("tenant whole upload across segment boundary");
        let mut expected = first;
        expected.extend_from_slice(b"tenant append");
        let final_sha = sha256_hex(&expected);
        store.upsert(scoped.conn(), session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session_id,
            relative_path: &path, sha256: &final_sha, bytes: b"tenant append",
            base: Some(((SEGMENT + 2) as i64, &first_sha)),
        }, Some(incarnation), Some(generation)).await.expect("tenant append to child segment");
        let segment_sizes: Vec<i32> = sqlx::query_scalar(
            "SELECT size FROM chat_session_file_segments WHERE session_id = $1 ORDER BY segment_index",
        ).bind(session).fetch_all(scoped.conn()).await?;
        assert_eq!(segment_sizes, vec![2 + b"tenant append".len() as i32]);
        assert_eq!(store.load(scoped.conn(), session, &provider_session_id, &path)
            .await.expect("tenant loads segmented file").bytes, expected);
        let publication = SessionFileBundlePublication {
            generation, provider: SessionFileProvider::Codex,
            provider_session_id: provider_session_id.clone(),
            files: vec![PublishedSessionFile {
                relative_path: path, sha256: final_sha, size: expected.len() as i64,
            }],
        };
        store.publish_bundle(scoped.conn(), session, &publication, incarnation)
            .await.expect("tenant publishes segmented file");
        scoped.commit().await?;
        tenant.close().await;
        authz.close().await;
        system.close().await;
        Ok(())
    }.await;
    if let Err(error) = &result {
        eprintln!("tenant segmented session file test failed before cleanup: {error:?}");
    }
    db.drop().await;
    result.unwrap();
}

#[tokio::test]
async fn segmented_upload_append_read_publish_and_tenant_boundaries() {
    let db = TestDb::create().await;
    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let store = ChatSessionFileService::new();
        let mut identities = Vec::new();
        for n in 0..2 {
            let org = Uuid::new_v4();
            let user = Uuid::new_v4();
            let workspace = Uuid::new_v4();
            let session = Uuid::new_v4();
            let mut identity_tx = db.pool.begin().await?;
            sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Segment test',$2)")
                .bind(org).bind(format!("segment-{org}")).execute(&mut *identity_tx).await?;
            sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Segment owner')")
                .bind(user).bind(format!("segment-{n}-{user}@example.test")).execute(&mut *identity_tx).await?;
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
                .bind(org).bind(user).execute(&mut *identity_tx).await?;
            identity_tx.commit().await?;
            sqlx::query("INSERT INTO workspaces(id,org_id,name,created_by) VALUES($1,$2,$3,$4)")
                .bind(workspace).bind(org).bind(format!("segment-{n}")).bind(user).execute(&db.pool).await?;
            sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
                .bind(session).bind(org).bind(workspace).bind(user).execute(&db.pool).await?;
            identities.push((org, user, session));
        }
        let (org, user, session) = identities[0];
        let foreign_org = identities[1].0;
        let incarnation = SessionFileIncarnation { vm_id: Uuid::new_v4(), epoch: 1 };
        let generation = Uuid::new_v4();
        sqlx::query("INSERT INTO chat_session_file_bundles(session_id,org_id,generation,state,fenced_vm_id,fenced_epoch) VALUES($1,$2,$3,'pending',$4,1)")
            .bind(session).bind(org).bind(generation).bind(incarnation.vm_id.to_string())
            .execute(&db.pool).await?;
        let provider_session = Uuid::new_v4().to_string();
        let path = format!("sessions/2026/10/03/rollout-x-{provider_session}.jsonl");
        let first = vec![b'A'; SEGMENT - 3];
        let first_sha = sha256_hex(&first);
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &first_sha, bytes: &first, base: None,
        }, Some(incarnation), Some(generation)).await.expect("initial segmented upload");
        let below: (i32, i64) = sqlx::query_as(
            "SELECT octet_length(bytes), (SELECT count(*) FROM chat_session_file_segments
             WHERE session_id = $1) FROM chat_session_files WHERE session_id = $1",
        ).bind(session).fetch_one(&db.pool).await?;
        assert_eq!(below, ((SEGMENT - 3) as i32, 0));

        let suffix = b"1234567890";
        let mut expected = first;
        expected.extend_from_slice(suffix);
        let second_sha = sha256_hex(&expected);
        let appended = store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &second_sha, bytes: suffix,
            base: Some(((SEGMENT - 3) as i64, &first_sha)),
        }, Some(incarnation), Some(generation)).await.expect("cross the first segment boundary");
        assert!(appended.changed);
        let crossing: (i32, Vec<i32>) = (
            sqlx::query_scalar("SELECT octet_length(bytes) FROM chat_session_files WHERE session_id = $1")
                .bind(session).fetch_one(&db.pool).await?,
            sqlx::query_scalar("SELECT size FROM chat_session_file_segments WHERE session_id = $1 ORDER BY segment_index")
                .bind(session).fetch_all(&db.pool).await?,
        );
        assert_eq!(crossing, (SEGMENT as i32, vec![7]));
        let retry = store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &second_sha, bytes: suffix,
            base: Some(((SEGMENT - 3) as i64, &first_sha)),
        }, Some(incarnation), Some(generation)).await.expect("retry an already stored append");
        assert!(!retry.changed);

        let large_suffix = vec![b'B'; SEGMENT + 13];
        let prior_size = expected.len() as i64;
        expected.extend_from_slice(&large_suffix);
        let full_sha = sha256_hex(&expected);
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &full_sha, bytes: &large_suffix,
            base: Some((prior_size, &second_sha)),
        }, Some(incarnation), Some(generation)).await.expect("append across two segment boundaries");
        let segment_rows = sqlx::query(
            "SELECT segment_index, size, octet_length(bytes) AS actual FROM chat_session_file_segments
             WHERE session_id = $1 ORDER BY segment_index",
        ).bind(session).fetch_all(&db.pool).await?;
        assert_eq!(segment_rows.len(), 2);
        for (index, row) in segment_rows.iter().enumerate() {
            assert_eq!(row.get::<i32, _>("segment_index"), (index + 1) as i32);
            assert_eq!(row.get::<i32, _>("size"), row.get::<i32, _>("actual"));
            assert!(row.get::<i32, _>("size") as usize <= SEGMENT);
        }
        let full_segment_ctid: String = sqlx::query_scalar(
            "SELECT ctid::text FROM chat_session_file_segments
             WHERE session_id = $1 AND relative_path = $2 AND segment_index = 1",
        ).bind(session).bind(&path).fetch_one(&db.pool).await?;
        let prior_size = expected.len() as i64;
        expected.push(b'C');
        let extended_sha = sha256_hex(&expected);
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &extended_sha, bytes: b"C",
            base: Some((prior_size, &full_sha)),
        }, Some(incarnation), Some(generation)).await.expect("append after an earlier segment is full");
        assert_eq!(sqlx::query_scalar::<_, String>(
            "SELECT ctid::text FROM chat_session_file_segments
             WHERE session_id = $1 AND relative_path = $2 AND segment_index = 1",
        ).bind(session).bind(&path).fetch_one(&db.pool).await?, full_segment_ctid,
            "a full preceding segment must never be rewritten by a later append");
        let full_sha = extended_sha;
        assert_eq!(store.load(&mut *db.pool.acquire().await?, session, &provider_session, &path)
            .await.expect("read segmented file").bytes, expected);
        let publication = SessionFileBundlePublication {
            generation, provider: SessionFileProvider::Codex,
            provider_session_id: provider_session.clone(),
            files: vec![PublishedSessionFile {
                relative_path: path.clone(), sha256: full_sha.clone(), size: expected.len() as i64,
            }],
        };
        sqlx::query("DELETE FROM chat_session_file_segments WHERE session_id = $1 AND segment_index = 2")
            .bind(session).execute(&db.pool).await?;
        assert!(store.load(&mut *db.pool.acquire().await?, session, &provider_session, &path).await.is_err(),
            "a missing segment cannot be loaded as a complete file");
        assert!(store.publish_bundle(&mut *db.pool.acquire().await?, session, &publication, incarnation)
            .await.is_err(), "a missing segment cannot publish");
        let final_tail = &expected[SEGMENT * 2..];
        sqlx::query("INSERT INTO chat_session_file_segments(session_id,org_id,provider_session_id,relative_path,segment_index,bytes,size) VALUES($1,$2,$3,$4,2,$5,$6)")
            .bind(session).bind(org).bind(&provider_session).bind(&path)
            .bind(final_tail).bind(final_tail.len() as i32).execute(&db.pool).await?;
        store.publish_bundle(&mut *db.pool.acquire().await?, session, &publication, incarnation).await.expect("publish complete segmented file");

        let flags = sqlx::query("SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'public.chat_session_file_segments'::regclass")
            .fetch_one(&db.pool).await?;
        assert!(flags.get::<bool, _>("relrowsecurity") && flags.get::<bool, _>("relforcerowsecurity"));
        let invalid_org = sqlx::query("UPDATE chat_session_file_segments SET org_id = $2 WHERE session_id = $1")
            .bind(session).bind(foreign_org).execute(&db.pool).await.unwrap_err();
        assert_eq!(invalid_org.as_database_error().and_then(|error| error.code()).as_deref(), Some("23503"));

        let foreign_session = identities[1].2;
        let foreign_file = Uuid::new_v4().to_string();
        let foreign_path = format!("sessions/2026/10/03/rollout-x-{foreign_file}.jsonl");
        sqlx::query("INSERT INTO chat_session_files(session_id,org_id,provider,provider_session_id,relative_path,bytes,sha256,size) VALUES($1,$2,'codex',$3,$4,$5,$6,$7)")
            .bind(foreign_session).bind(foreign_org).bind(&foreign_file).bind(&foreign_path)
            .bind(vec![b'X'; SEGMENT]).bind(sha256_hex(&vec![b'X'; SEGMENT]))
            .bind(SEGMENT as i64).execute(&db.pool).await?;
        sqlx::query("INSERT INTO chat_session_file_segments(session_id,org_id,provider_session_id,relative_path,segment_index,bytes,size) VALUES($1,$2,$3,$4,1,$5,1)")
            .bind(foreign_session).bind(foreign_org).bind(&foreign_file).bind(&foreign_path)
            .bind(b"Y".as_slice()).execute(&db.pool).await?;
        let system = PgPoolOptions::new().connect(&db.system_url).await?;
        let authz = PgPoolOptions::new().connect(&db.authz_url).await?;
        let tenant = PgPoolOptions::new().connect(&db.tenant_url).await?;
        let service = InProcessAuthz::new(system.clone(), authz.clone());
        let cap = service.mint_db_capability(UserId::from_uuid(user), &[OrgId::from_uuid(org)], &["*".into()]).await?;
        let mut scoped = ScopedDb::from_capability(&tenant, cap).await?;
        let visible: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT session_id FROM chat_session_file_segments")
            .fetch_all(scoped.conn()).await?;
        assert_eq!(visible, vec![session]);
        scoped.commit().await?;
        tenant.close().await;
        authz.close().await;
        system.close().await;

        let replacement = b"whole rewrite after segments";
        let replacement_sha = sha256_hex(replacement);
        let next_generation = Uuid::new_v4();
        sqlx::query("UPDATE chat_session_file_bundles SET generation = $2, state = 'pending', provider = NULL, provider_session_id = NULL, files = '[]'::jsonb WHERE session_id = $1")
            .bind(session).bind(next_generation).execute(&db.pool).await?;
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &replacement_sha, bytes: replacement, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("whole rewrite clears segments");
        let cleared: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1")
            .bind(session).fetch_one(&db.pool).await?;
        assert_eq!(cleared, 0);
        assert_eq!(store.load(&mut *db.pool.acquire().await?, session, &provider_session, &path)
            .await.expect("read whole rewrite").bytes, replacement);
        sqlx::query("UPDATE chat_session_files SET sha256_state = NULL WHERE session_id = $1")
            .bind(session).execute(&db.pool).await?;
        let after_legacy = b"whole rewrite after segments+append";
        let after_legacy_sha = sha256_hex(after_legacy);
        assert!(store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &after_legacy_sha, bytes: b"+append",
            base: Some((replacement.len() as i64, &replacement_sha)),
        }, Some(incarnation), Some(next_generation)).await.is_err(),
            "a legacy row without state requires the worker's whole-upload fallback");
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
            relative_path: &path, sha256: &after_legacy_sha, bytes: after_legacy, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("legacy fallback whole upload");
        assert!(sqlx::query_scalar::<_, Option<Vec<u8>>>("SELECT sha256_state FROM chat_session_files WHERE session_id = $1")
            .bind(session).fetch_one(&db.pool).await?.is_some());

        let large_whole_id = Uuid::new_v4().to_string();
        let large_whole_path = format!("sessions/2026/10/03/rollout-x-{large_whole_id}.jsonl");
        let large_whole = vec![b'W'; SEGMENT + 1];
        let large_whole_sha = sha256_hex(&large_whole);
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &large_whole_id,
            relative_path: &large_whole_path, sha256: &large_whole_sha,
            bytes: &large_whole, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("large whole upload splits segment zero");
        let whole_lengths: Vec<i32> = sqlx::query_scalar(
            "SELECT octet_length(bytes) FROM chat_session_files WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&large_whole_path).fetch_all(&db.pool).await?;
        assert_eq!(whole_lengths, vec![SEGMENT as i32]);
        let child_lengths: Vec<i32> = sqlx::query_scalar(
            "SELECT size FROM chat_session_file_segments WHERE session_id = $1 AND relative_path = $2 ORDER BY segment_index",
        ).bind(session).bind(&large_whole_path).fetch_all(&db.pool).await?;
        assert_eq!(child_lengths, vec![1]);
        sqlx::query(
            "UPDATE chat_session_files SET bytes = $3, sha256 = $4, size = $5
             WHERE session_id = $1 AND relative_path = $2",
        )
        .bind(session)
        .bind(&large_whole_path)
        .bind(&large_whole)
        .bind(&large_whole_sha)
        .bind(large_whole.len() as i64)
        .execute(&db.pool)
        .await?;
        assert_eq!(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&large_whole_path).fetch_one(&db.pool).await?, 0,
            "a prior API whole-file rewrite must clear stale segments");
        assert_eq!(store.load(&mut *db.pool.acquire().await?, session, &large_whole_id, &large_whole_path)
            .await.expect("read legacy whole-file rewrite").bytes, large_whole);
        let extended_whole_sha = sha256_hex(&[large_whole.as_slice(), b"!"].concat());
        assert!(store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &large_whole_id,
            relative_path: &large_whole_path, sha256: &extended_whole_sha,
            bytes: b"!", base: Some((large_whole.len() as i64, &large_whole_sha)),
        }, Some(incarnation), Some(next_generation)).await.is_err(),
            "a prior API rewrite needs one bounded whole-upload conversion");
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::Codex, provider_session_id: &large_whole_id,
            relative_path: &large_whole_path, sha256: &large_whole_sha,
            bytes: &large_whole, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("convert prior API rewrite to segments");
        assert_eq!(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&large_whole_path).fetch_one(&db.pool).await?, 1);

        let claude_id = Uuid::new_v4().to_string();
        let old_key = format!("projects/-work-old/{claude_id}.jsonl");
        let new_key = format!("projects/-work-new/{claude_id}.jsonl");
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::ClaudeCode, provider_session_id: &claude_id,
            relative_path: &old_key, sha256: &large_whole_sha,
            bytes: &large_whole, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("upload old Claude key");
        assert_eq!(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&old_key).fetch_one(&db.pool).await?, 1);
        let new_key_bytes = b"new Claude project key";
        let new_key_sha = sha256_hex(new_key_bytes);
        store.upsert(&mut *db.pool.acquire().await?, session, ChatSessionFileUpload {
            provider: SessionFileProvider::ClaudeCode, provider_session_id: &claude_id,
            relative_path: &new_key, sha256: &new_key_sha,
            bytes: new_key_bytes, base: None,
        }, Some(incarnation), Some(next_generation)).await.expect("replace old Claude project key");
        assert_eq!(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&old_key).fetch_one(&db.pool).await?, 0);
        assert_eq!(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chat_session_files WHERE session_id = $1 AND relative_path = $2",
        ).bind(session).bind(&old_key).fetch_one(&db.pool).await?, 0);

        let left_suffix = b"left";
        let right_suffix = b"right";
        let left_sha = sha256_hex(&[after_legacy.as_slice(), left_suffix].concat());
        let right_sha = sha256_hex(&[after_legacy.as_slice(), right_suffix].concat());
        let left = async {
            let mut transaction = db.pool.begin().await.unwrap();
            let outcome = store.upsert(&mut transaction, session, ChatSessionFileUpload {
                provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
                relative_path: &path, sha256: &left_sha, bytes: left_suffix,
                base: Some((after_legacy.len() as i64, &after_legacy_sha)),
            }, Some(incarnation), Some(next_generation)).await;
            if outcome.is_ok() {
                transaction.commit().await.unwrap();
            } else {
                transaction.rollback().await.unwrap();
            }
            outcome
        };
        let right = async {
            let mut transaction = db.pool.begin().await.unwrap();
            let outcome = store.upsert(&mut transaction, session, ChatSessionFileUpload {
                provider: SessionFileProvider::Codex, provider_session_id: &provider_session,
                relative_path: &path, sha256: &right_sha, bytes: right_suffix,
                base: Some((after_legacy.len() as i64, &after_legacy_sha)),
            }, Some(incarnation), Some(next_generation)).await;
            if outcome.is_ok() {
                transaction.commit().await.unwrap();
            } else {
                transaction.rollback().await.unwrap();
            }
            outcome
        };
        let (left, right) = tokio::join!(left, right);
        assert_ne!(left.is_ok(), right.is_ok(), "exactly one competing append may advance the file");
        let winning_bytes = store.load(&mut *db.pool.acquire().await?, session, &provider_session, &path)
            .await.expect("load the winning append").bytes;
        assert!(winning_bytes == [after_legacy.as_slice(), left_suffix].concat()
            || winning_bytes == [after_legacy.as_slice(), right_suffix].concat());
        sqlx::query("DELETE FROM chat_sessions WHERE id = $1")
            .bind(session).execute(&db.pool).await?;
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_session_file_segments WHERE session_id = $1")
            .bind(session).fetch_one(&db.pool).await?, 0);
        Ok(())
    }.await;
    if let Err(error) = &result {
        eprintln!("segmented session file test failed before cleanup: {error:?}");
    }
    db.drop().await;
    result.unwrap();
}
