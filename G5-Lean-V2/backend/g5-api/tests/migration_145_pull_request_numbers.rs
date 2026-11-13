//! Migration 145 (`g5.git.pr-local-number`, `g5.workspace.delete.pull-requests`):
//! existing and new G5 PRs take random numbers from 1 to 0xffffff, unique in
//! their organization, and PRs left open by deleted workspaces close once
//! with an event that names the deletion. Applied as a non-superuser migrator
//! that may set the definer role, like the deployed runner.

use crate::helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const MIGRATION: &str = "145_pull_request_numbers.sql";

struct Seed {
    owner: Uuid,
    first: Uuid,
    from_deleted: Uuid,
    into_deleted: Uuid,
    merged_from_deleted: Uuid,
    live: Uuid,
    other_org: Uuid,
}

async fn org(db: &TestDb, owner: Uuid) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $2)")
        .bind(org)
        .bind(format!("org-{}", org.simple()))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(owner)
        .execute(&db.pool)
        .await
        .unwrap();
    org
}

async fn workspace(db: &TestDb, org: Uuid, owner: Uuid, deleted: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
         VALUES ($1, $2, $3, false, $4, $4)",
    )
    .bind(id)
    .bind(org)
    .bind(format!("ws-{}", id.simple()))
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    if deleted {
        sqlx::query("UPDATE workspaces SET deleted_at = now() - interval '1 day', deleted_by = $2 WHERE id = $1")
            .bind(id)
            .bind(owner)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    id
}

async fn pull_request(
    db: &TestDb,
    org: Uuid,
    source: Uuid,
    target: Uuid,
    status: &str,
    minutes_ago: i32,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_pull_requests
             (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status,
              title, last_preflight_fingerprint, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'PR', 'f', now() - make_interval(mins => $7))",
    )
    .bind(id)
    .bind(org)
    .bind(source)
    .bind(target)
    .bind(status)
    .bind(if status == "open" { "ready" } else { status })
    .bind(minutes_ago)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

async fn seed(db: &TestDb) -> Seed {
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Owner')")
        .bind(owner)
        .bind(format!("owner-{owner}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    let a = org(db, owner).await;
    let parent = workspace(db, a, owner, false).await;
    let child = workspace(db, a, owner, false).await;
    let gone = workspace(db, a, owner, true).await;
    let gone_target = workspace(db, a, owner, true).await;
    let elsewhere = workspace(db, a, owner, false).await;
    // Creation order, oldest first, is the numbering order.
    let first = pull_request(db, a, child, parent, "closed", 50).await;
    let from_deleted = pull_request(db, a, gone, parent, "open", 40).await;
    let into_deleted = pull_request(db, a, elsewhere, gone_target, "open", 30).await;
    let merged_from_deleted = pull_request(db, a, gone, elsewhere, "merged", 20).await;
    let live = pull_request(db, a, child, elsewhere, "open", 10).await;
    let b = org(db, owner).await;
    let b_source = workspace(db, b, owner, false).await;
    let b_target = workspace(db, b, owner, false).await;
    let other_org = pull_request(db, b, b_source, b_target, "open", 60).await;
    Seed {
        owner,
        first,
        from_deleted,
        into_deleted,
        merged_from_deleted,
        live,
        other_org,
    }
}

async fn row(db: &TestDb, id: Uuid) -> (i64, String, Option<String>, Option<Uuid>) {
    sqlx::query_as(
        "SELECT pr.number, pr.status, e.message, e.actor_id FROM workspace_pull_requests pr
         LEFT JOIN workspace_pull_request_events e
           ON e.pull_request_id = pr.id AND e.event_type = 'closed'
         WHERE pr.id = $1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn numbers_pull_requests_and_closes_those_of_deleted_workspaces() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "145"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let seed = seed(&db).await;

    let role = format!("pr_numbers_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT USAGE, CREATE ON SCHEMA public TO {role}; ALTER SCHEMA authz OWNER TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.workspace_pull_requests OWNER TO {role};
         ALTER TABLE public.workspace_pull_request_events OWNER TO {role};"
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
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    let can_set: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user,'authz_lookup_definer','SET')")
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert!(!can_set, "temporary migration membership restored");
    let definer_keeps: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('authz_lookup_definer','public.workspace_pull_request_events','INSERT')
             OR has_column_privilege('authz_lookup_definer','public.workspace_pull_requests','status','UPDATE')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!definer_keeps, "the backfill grants are revoked");
    // Each close is dated by its deletion, not by the migration.
    let dated: bool = sqlx::query_scalar(
        "SELECT bool_and(created_at < now() - interval '12 hours') \
         FROM workspace_pull_request_events \
         WHERE event_type = 'closed' AND pull_request_id = ANY($1)",
    )
    .bind(vec![seed.from_deleted, seed.into_deleted])
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(dated, "closes are dated by the deletion");
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .expect("a repeated run is a no-op");

    // Every PR has a random number in range, unique in its organization.
    let numbers: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT org_id, number FROM workspace_pull_requests ORDER BY org_id, number",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(numbers.len(), 6);
    assert!(numbers.iter().all(|(_, n)| (1..=0xff_ffff).contains(n)));
    let mut distinct = numbers.clone();
    distinct.dedup();
    assert_eq!(distinct.len(), numbers.len(), "{numbers:?}");
    let deleted_source = Some("Closed because its source workspace was deleted".to_string());
    let deleted_target = Some("Closed because its target workspace was deleted".to_string());
    let state = |row: (i64, String, Option<String>, Option<Uuid>)| (row.1, row.2, row.3);
    assert_eq!(
        state(row(&db, seed.other_org).await),
        ("open".into(), None, None)
    );
    assert_eq!(
        state(row(&db, seed.first).await),
        ("closed".into(), None, None)
    );
    assert_eq!(
        state(row(&db, seed.from_deleted).await),
        ("closed".into(), deleted_source, Some(seed.owner))
    );
    assert_eq!(
        state(row(&db, seed.into_deleted).await),
        ("closed".into(), deleted_target, Some(seed.owner))
    );
    // A merged PR of a deleted workspace is not closed again.
    assert_eq!(
        state(row(&db, seed.merged_from_deleted).await),
        ("merged".into(), None, None)
    );
    assert_eq!(
        state(row(&db, seed.live).await),
        ("open".into(), None, None)
    );

    // A new PR takes a random number its organization has not used, with no
    // lock and no sequence: replaying the trigger's random draws, the first
    // candidate is taken, so it takes the next.
    let (org, source, target): (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT org_id, source_workspace_id, target_workspace_id FROM workspace_pull_requests WHERE id = $1",
    )
    .bind(seed.first)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let sequence: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_serial_sequence('public.workspace_pull_requests', 'number')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(sequence, None, "no sequence");
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query("SELECT setseed(0.25)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let candidates: (i64, i64) = sqlx::query_as(
        "SELECT 1 + floor(random() * 16777215)::bigint, 1 + floor(random() * 16777215)::bigint",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_pull_requests
             (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status,
              title, last_preflight_fingerprint, number)
         VALUES (gen_random_uuid(), $1, $2, $3, 'closed', 'closed', 'PR', 'f', $4)",
    )
    .bind(org)
    .bind(source)
    .bind(target)
    .bind(candidates.0)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SELECT setseed(0.25)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let skipped: i64 = sqlx::query_scalar(
        "INSERT INTO workspace_pull_requests
             (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status,
              title, last_preflight_fingerprint)
         VALUES (gen_random_uuid(), $1, $2, $3, 'open', 'ready', 'PR', 'f')
         RETURNING number",
    )
    .bind(org)
    .bind(source)
    .bind(target)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(skipped, candidates.1, "the taken first draw is skipped");
    tx.rollback().await.unwrap();
    let next = pull_request(&db, org, source, target, "open", 0).await;
    let number = row(&db, next).await.0;
    assert!((1..=0xff_ffff).contains(&number));
    // Numbers stay unique within an organization and in range.
    let duplicate = sqlx::query(
        "INSERT INTO workspace_pull_requests
             (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status,
              title, last_preflight_fingerprint, number)
         VALUES (gen_random_uuid(), $1, $2, $3, 'closed', 'closed', 'PR', 'f', $4)",
    )
    .bind(org)
    .bind(source)
    .bind(target)
    .bind(number)
    .execute(&db.pool)
    .await
    .expect_err("a duplicate number");
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505")
    );
    let out_of_range = sqlx::query(
        "INSERT INTO workspace_pull_requests
             (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status,
              title, last_preflight_fingerprint, number)
         VALUES (gen_random_uuid(), $1, $2, $3, 'closed', 'closed', 'PR', 'f', 16777216)",
    )
    .bind(org)
    .bind(source)
    .bind(target)
    .execute(&db.pool)
    .await
    .expect_err("a number past 0xffffff");
    assert_eq!(
        out_of_range
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    db.drop().await;
}
