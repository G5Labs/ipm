//! Migration 110 through the Helm/CI migration executable (ontology
//! `g5.quality.migrations.verification`, `g5.workspace.kind`,
//! `g5.personal.identity`, `g5.personal.capacity`,
//! `g5.personal.retirement`, `g5.personal.start-and-send`).
//!
//! 110 adds the personal registry kind (`workspaces.personal_user_id` with its
//! owner check, one-live index and kind immutability), the personal idle
//! default config key, the `owner_removed` stop reason, the Start-and-send
//! binding column and the trigger that settles it, and the system pool's
//! retirement column grant. It is schema only: the real deployment migrator
//! (NOSUPERUSER, NOBYPASSRLS, owner of what it alters) applies it over a
//! populated tenant, rewrites no row, and replays as a no-op.

use crate::helpers;

use helpers::TestDb;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use uuid::Uuid;

async fn run_migration_command(database_url: &str, source: &Path) -> Result<Output, sqlx::Error> {
    let binary = std::env::var_os("NEXTEST_BIN_EXE_g5_db_migrate")
        .or_else(|| std::env::var_os("NEXTEST_BIN_EXE_g5-db-migrate"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_g5-db-migrate").into());
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        Command::new(binary)
            .env("DATABASE_URL", database_url)
            .env("G5_MIGRATION_SOURCE", source)
            .env("G5_MIGRATION_DATABASE_MAX_CONNECTIONS", "2")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "g5-db-migrate exceeded the populated upgrade timeout",
        ))
    })?
    .map_err(sqlx::Error::Io)
}

#[tokio::test]
async fn upgrade_adds_the_personal_kind_and_start_binding_without_rewriting_rows() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    let version = |name: &std::ffi::OsStr| -> u64 {
        name.to_string_lossy()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && version(&entry.file_name()) < 110
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let app = TestDb::create_from_migrations(catalog.path()).await;
    let name = "110_personal_workspaces.sql";
    std::fs::copy(source.join(name), catalog.path().join(name)).unwrap();

    // A populated tenant: an owner, a working workspace with a waiting Start.
    let org = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Personal migration',$2)")
        .bind(org)
        .bind(format!("personal-migration-{}", org.simple()))
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Owner')")
        .bind(owner)
        .bind(format!("owner-{owner}@example.test"))
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(owner)
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by)
         VALUES($1,$2,'users/legacy',false,$3,$3)",
    )
    .bind(workspace)
    .bind(org)
    .bind(owner)
    .execute(&app.pool)
    .await
    .unwrap();

    let role = format!("personal_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'personal_migration_test' NOSUPERUSER NOBYPASSRLS; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role}; GRANT ALL ON public._sqlx_migrations TO {role};"))
        .execute(&mut *tx).await.unwrap();
    // The deployment migrator owns what 110 alters; FORCE ROW LEVEL SECURITY
    // still hides tenant rows.
    sqlx::raw_sql(&format!(
        "ALTER FUNCTION public.g5_config_value_is_registered(text, text, jsonb) OWNER TO {role}; \
         ALTER TABLE public.workspaces OWNER TO {role}; \
         ALTER TABLE public.workspace_vms OWNER TO {role}; \
         ALTER TABLE public.session_inputs OWNER TO {role}; \
         ALTER TABLE public.users OWNER TO {role}"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut migration_url = url::Url::parse(&app.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url
        .set_password(Some("personal_migration_test"))
        .unwrap();
    let migration = run_migration_command(migration_url.as_str(), catalog.path()).await;
    let replay = match &migration {
        Ok(output) if output.status.success() => {
            Some(run_migration_command(migration_url.as_str(), catalog.path()).await)
        }
        _ => None,
    };
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&app.pool)
    .await
    .unwrap();
    let migration = migration.expect("migration runs");
    assert!(
        migration.status.success(),
        "the production executable applies 110 as a non-BYPASSRLS owner: {migration:?}"
    );
    let replay = replay.unwrap().expect("replay runs");
    assert!(replay.status.success(), "replay is a no-op: {replay:?}");

    // No row was rewritten: the legacy name stays an ordinary workspace.
    let (name, personal): (String, Option<Uuid>) =
        sqlx::query_as("SELECT name, personal_user_id FROM workspaces WHERE id = $1")
            .bind(workspace)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), personal), ("users/legacy", None));

    // The kind is immutable in both directions.
    assert!(
        sqlx::query("UPDATE workspaces SET personal_user_id = owner_user_id WHERE id = $1")
            .bind(workspace)
            .execute(&app.pool)
            .await
            .is_err()
    );
    // The bound user is the owner, never Base, never forked.
    let personal_ws = Uuid::new_v4();
    let insert = |id: Uuid, owner_id: Uuid, bound: Uuid, is_base: bool| {
        sqlx::query(
            "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by,personal_user_id)
             VALUES($1,$2,$3,$4,$5,$5,$6)",
        )
        .bind(id)
        .bind(org)
        .bind(format!("users/p-{}", id.simple()))
        .bind(is_base)
        .bind(owner_id)
        .bind(bound)
    };
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Other')")
        .bind(other)
        .bind(format!("other-{other}@example.test"))
        .execute(&app.pool)
        .await
        .unwrap();
    assert!(insert(Uuid::new_v4(), owner, other, false)
        .execute(&app.pool)
        .await
        .is_err());
    assert!(insert(Uuid::new_v4(), owner, owner, true)
        .execute(&app.pool)
        .await
        .is_err());
    insert(personal_ws, owner, owner, false)
        .execute(&app.pool)
        .await
        .unwrap();
    assert!(
        insert(Uuid::new_v4(), owner, owner, false)
            .execute(&app.pool)
            .await
            .is_err(),
        "at most one live personal workspace per (organization, user)"
    );
    assert!(
        sqlx::query("UPDATE workspaces SET personal_user_id = NULL WHERE id = $1")
            .bind(personal_ws)
            .execute(&app.pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(personal_ws)
        .execute(&app.pool)
        .await
        .unwrap();
    insert(Uuid::new_v4(), owner, owner, false)
        .execute(&app.pool)
        .await
        .expect("a tombstoned personal workspace frees the slot");

    // The personal idle default is registered with the ordinary bounds.
    for (value, expected) in [
        (json!(59), false),
        (json!(60), true),
        (json!(1800), true),
        (json!(86_401), false),
    ] {
        let accepted = sqlx::query(
            "INSERT INTO configs(org_id,scope,scope_id,key,value)
             VALUES($1,'org',$1,'default_personal_idle_timeout_secs',$2)
             ON CONFLICT (org_id,scope,scope_id,key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(org)
        .bind(&value)
        .execute(&app.pool)
        .await
        .is_ok();
        assert_eq!(accepted, expected, "{value}");
    }

    // owner_removed is a stop reason beside 099's member_deactivated; the
    // binding column exists.
    let now = chrono::Utc::now();
    let intent: Value = json!({
        "id": Uuid::new_v4(), "kind": "compound_start", "owner_user_id": owner,
        "lifecycle_generation": 0, "status": "pending", "created_at": now,
        "expires_at": now + chrono::Duration::minutes(15), "attempts": 0,
    });
    sqlx::query(
        "INSERT INTO workspace_vms(org_id,workspace_id,status,start_intent)
         VALUES($1,$2,'stopped',$3)",
    )
    .bind(org)
    .bind(workspace)
    .bind(&intent)
    .execute(&app.pool)
    .await
    .expect("a compound Start intent is stored");
    for constraint in [
        "workspace_vms_stop_reason_check",
        "workspace_vms_pending_stop_reason_check",
    ] {
        let definition: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conname = $1",
        )
        .bind(constraint)
        .fetch_one(&app.pool)
        .await
        .unwrap();
        assert!(
            definition.contains("owner_removed") && definition.contains("member_deactivated"),
            "{constraint}: {definition}"
        );
    }
    let column: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_name = 'session_inputs' AND column_name = 'start_consent_id'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(column, 1);
    let grant: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('g5_app', 'public.workspaces', 'deleted_at', 'UPDATE')",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        grant,
        "the system pool can retire a personal workspace in a membership change"
    );

    app.drop().await;
}
