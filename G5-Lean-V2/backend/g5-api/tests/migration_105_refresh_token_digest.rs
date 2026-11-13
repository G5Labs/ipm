//! Migration 105 (`refresh_tokens.token_hash`, G5-Lean#660,
//! `g5.user.session-authority`) must apply on a database migrated through
//! main's 104 with plaintext refresh credentials, rewrite every existing row
//! to the digest the service looks up, so valid sessions survive the upgrade,
//! and replay as a no-op.

use crate::helpers::{refresh_token_hash, TestDb};
use std::path::Path;
use uuid::Uuid;

const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/db/migrations");

async fn columns(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'refresh_tokens'
         ORDER BY ordinal_position",
    )
    .fetch_all(pool)
    .await
    .expect("read column catalog")
}

async fn applied_versions(pool: &sqlx::PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("read applied migrations")
}

/// Historical catalog through 104, without 105 or any later migrations.
fn stage_main_catalog() -> tempfile::TempDir {
    let staged = tempfile::tempdir().expect("scratch catalog");
    for entry in std::fs::read_dir(Path::new(MIGRATIONS)).expect("list migrations") {
        let path = entry.expect("migration entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let version = name
            .split('_')
            .next()
            .and_then(|prefix| prefix.parse::<i64>().ok());
        if name.ends_with(".sql") && version.is_some_and(|version| version <= 104) {
            std::fs::copy(&path, staged.path().join(name)).expect("stage migration");
        }
    }
    staged
}

#[tokio::test]
async fn migration_105_rewrites_plaintext_refresh_credentials_to_their_digests() {
    let staged = stage_main_catalog();
    let db = TestDb::create_from_migrations(staged.path()).await;
    let before = applied_versions(&db.pool).await;
    assert_eq!(before.last(), Some(&104), "historical catalog: {before:?}");
    assert!(!before.contains(&105));
    assert!(columns(&db.pool).await.contains(&"token".to_string()));

    // Sessions as main stored them: the bearer itself, with its own idle
    // expiry and issue time.
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, display_name, oauth_provider, oauth_id)
         VALUES ($1, $2, 'Existing', 'github', $3)",
    )
    .bind(user_id)
    .bind(format!("{}@example.com", user_id.simple()))
    .bind(user_id.to_string())
    .execute(&db.pool)
    .await
    .expect("an existing user");
    let plaintexts: Vec<String> = (0..3)
        .map(|_| Uuid::new_v4().simple().to_string())
        .collect();
    for (index, token) in plaintexts.iter().enumerate() {
        sqlx::query(
            "INSERT INTO refresh_tokens (token, user_id, expires_at, created_at)
             VALUES ($1, $2, now() + interval '20 days', now() - make_interval(days => $3))",
        )
        .bind(token)
        .bind(user_id)
        .bind(index as i32)
        .execute(&db.pool)
        .await
        .expect("a plaintext session row");
    }

    g5_api::db::run_migrations(&db.pool)
        .await
        .expect("apply 105 on top of main's catalog");
    let after = applied_versions(&db.pool).await;
    assert_eq!(after.iter().filter(|v| **v == 105).count(), 1, "{after:?}");
    let columns_after = columns(&db.pool).await;
    assert!(columns_after.contains(&"token_hash".to_string()));
    assert!(!columns_after.contains(&"token".to_string()));

    // Every row now holds exactly the digest the service looks up, with its
    // lifetimes untouched, so the credential a client still holds keeps
    // working and the stored value is no longer a bearer.
    let rows: Vec<(String, f64)> = sqlx::query_as(
        "SELECT token_hash, EXTRACT(EPOCH FROM (created_at - now()))::float8
         FROM refresh_tokens WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .expect("read migrated rows");
    assert_eq!(rows.len(), plaintexts.len());
    for ((stored, age_secs), plaintext) in rows.iter().zip(&plaintexts) {
        assert_eq!(stored, &refresh_token_hash(plaintext));
        assert_ne!(stored, plaintext);
        assert!(*age_secs <= 1.0, "issue time preserved: {age_secs}");
    }
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM refresh_tokens WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(refresh_token_hash(&plaintexts[0]))
    .fetch_optional(&db.pool)
    .await
    .expect("look up by digest");
    assert_eq!(found, Some(user_id));

    // Replaying the migrator changes nothing.
    g5_api::db::run_migrations(&db.pool)
        .await
        .expect("replay as a no-op");
    let replayed: Vec<String> = sqlx::query_scalar(
        "SELECT token_hash FROM refresh_tokens WHERE user_id = $1 ORDER BY token_hash",
    )
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .expect("read rows after replay");
    let mut expected: Vec<String> = plaintexts.iter().map(|t| refresh_token_hash(t)).collect();
    expected.sort();
    assert_eq!(replayed, expected);

    db.drop().await;
}
