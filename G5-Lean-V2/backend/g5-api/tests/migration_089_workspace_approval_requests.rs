//! Populated upgrade collapses the obsolete phase axis under the actual
//! non-superuser, non-BYPASSRLS migration owner; terminal evidence is retained.
use crate::helpers;
use helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;
use uuid::Uuid;

#[tokio::test]
async fn populated_workspace_review_upgrade_preserves_history_and_enforces_one_pending_slot() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "102_"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let org = Uuid::new_v4();
    let person = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    let definition = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Review upgrade',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Reviewer')")
        .bind(person)
        .bind(format!("{person}@g5.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(person)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'upgrade',$3,$3)")
        .bind(workspace).bind(org).bind(person).execute(&db.pool).await.unwrap();
    for version in [1, 2] {
        sqlx::query("INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by) VALUES($1,$2,$3,'Review',ARRAY[$4]::uuid[],$4)")
            .bind(definition).bind(version).bind(org).bind(person).execute(&db.pool).await.unwrap();
    }
    let mut ids = Vec::new();
    for (version, actor, phase) in [
        (1, "human", "ie"),
        (1, "human", "be"),
        (1, "human", "se"),
        (1, "agent", "be"),
        (1, "agent", "ie"),
        (2, "human", "be"),
    ] {
        let id: Uuid = sqlx::query_scalar("INSERT INTO approval_requests(org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,actor_user_id,requested_by,phase) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
            .bind(org).bind(workspace).bind(definition).bind(version).bind(Uuid::new_v4().simple().to_string().repeat(2)).bind(actor)
            .bind((actor=="human").then_some(person)).bind(person).bind(phase).fetch_one(&db.pool).await.unwrap();
        ids.push(id);
        if ids.len() == 1 {
            sqlx::query("UPDATE approval_requests SET state='cancelled',closed_at=transaction_timestamp(),updated_at=transaction_timestamp() WHERE id=$1")
                .bind(id).execute(&db.pool).await.unwrap();
        }
    }
    let before: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id,state,request_key FROM approval_requests ORDER BY seq")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let role = format!("approval_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
        GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
        GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};
        GRANT ALL ON public._sqlx_migrations TO {role};
        ALTER TABLE public.approval_requests OWNER TO {role};
        ALTER FUNCTION public.guard_approval_request_update() OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let migrator = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            db.pool
                .connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password("migration_test"),
        )
        .await
        .unwrap();
    let privileges: (bool, bool) =
        sqlx::query_as("SELECT rolsuper,rolbypassrls FROM pg_roles WHERE rolname=current_user")
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(privileges, (false, false));
    let hidden: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_requests")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(hidden, 0);
    std::fs::copy(
        source.join("102_workspace_approval_requests.sql"),
        catalog.path().join("102_workspace_approval_requests.sql"),
    )
    .unwrap();
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    let can_set: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user,'authz_lookup_definer','SET')")
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert!(!can_set, "temporary migration membership restored");
    let grants: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('authz_lookup_definer','approval_requests','SELECT,UPDATE')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!grants, "temporary data privileges revoked");
    let after: Vec<(Uuid, String, String, Option<String>)> =
        sqlx::query_as("SELECT id,state,request_key,note FROM approval_requests ORDER BY seq")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(after.len(), before.len());
    for (index, row) in after.iter().enumerate() {
        assert_eq!((row.0, &row.2), (before[index].0, &before[index].2));
        assert_eq!(row.3, None);
        assert_eq!(
            row.1,
            match index {
                0 => "cancelled",
                1 | 3 => "superseded",
                _ => "pending",
            }
        );
    }
    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE oid='approval_requests'::regclass",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(forced);
    for actor in ["human", "agent"] {
        let error = sqlx::query("INSERT INTO approval_requests(org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,actor_user_id,requested_by,phase) VALUES($1,$2,$3,1,$4,$5,$6,$7,'be')")
            .bind(org).bind(workspace).bind(definition).bind(Uuid::new_v4().simple().to_string().repeat(2)).bind(actor).bind((actor=="human").then_some(person)).bind(person).execute(&db.pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("approval_requests_workspace_pending_slot")
        );
    }
    let error = sqlx::query("UPDATE approval_requests SET note='replacement',state='cancelled',closed_at=transaction_timestamp(),updated_at=transaction_timestamp() WHERE id=$1")
        .bind(ids[2]).execute(&db.pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().constraint(),
        Some("approval_requests_immutable")
    );
    let oversized = sqlx::query("INSERT INTO approval_requests(org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,actor_user_id,requested_by,phase,note) VALUES($1,$2,$3,1,$4,'human',$5,$5,'ie',$6)")
        .bind(org).bind(workspace).bind(definition).bind(Uuid::new_v4().simple().to_string().repeat(2)).bind(person).bind("é".repeat(4097)).execute(&db.pool).await.unwrap_err();
    assert_eq!(
        oversized.as_database_error().unwrap().constraint(),
        Some("approval_request_note_size")
    );
    migrator.close().await;
    let admin_options = db.pool.connect_options().as_ref().clone();
    db.drop().await;
    // TestDb drops the isolated database, including the migration role's objects.
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin_options.database("postgres"))
        .await
        .unwrap();
    sqlx::raw_sql(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
