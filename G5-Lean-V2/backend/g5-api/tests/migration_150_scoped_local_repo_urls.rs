//! Migration 150 (`g5.repo.hosted-url`): every G5-hosted repository's
//! recorded URL becomes `g5-local://<org-slug>/<repo-slug>`, docs, code,
//! personal and seed-style rows alike, while provider repositories and
//! provider-less repositories with a real remote keep theirs. Applied by the
//! production `g5-db-migrate` executable as a non-superuser table owner
//! without BYPASSRLS, like the deployed runner.

use crate::helpers::TestDb;
use std::path::Path;
use std::process::Output;
use uuid::Uuid;

async fn run_migration_command(database_url: &str, source: &Path) -> Output {
    // Nextest remaps executable paths when running a build archive.
    let binary = std::env::var_os("NEXTEST_BIN_EXE_g5_db_migrate")
        .or_else(|| std::env::var_os("NEXTEST_BIN_EXE_g5-db-migrate"))
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_g5-db-migrate").into());
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new(binary)
            .env("DATABASE_URL", database_url)
            .env("G5_MIGRATION_SOURCE", source)
            .env("G5_MIGRATION_DATABASE_MAX_CONNECTIONS", "2")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("g5-db-migrate exceeded the populated upgrade timeout")
    .expect("run g5-db-migrate")
}

const MIGRATION: &str = "150_scoped_local_repo_urls.sql";

async fn org(db: &TestDb, slug: &str) -> Uuid {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $2)")
        .bind(org)
        .bind(slug)
        .execute(&db.pool)
        .await
        .unwrap();
    org
}

async fn repo(db: &TestDb, org: Uuid, slug: &str, git_url: &str, provider: Option<&str>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, provider) VALUES ($1, $2, $3, $3, $4, $5)",
    )
    .bind(id)
    .bind(org)
    .bind(slug)
    .bind(git_url)
    .bind(provider)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

async fn url(db: &TestDb, id: Uuid) -> String {
    sqlx::query_scalar("SELECT git_url FROM repos WHERE id = $1")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn rewrites_g5_hosted_repository_urls_and_keeps_remote_ones() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "150"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;

    let suffix = Uuid::new_v4().simple().to_string();
    let acme_slug = format!("acme-{suffix}");
    let acme = org(&db, &acme_slug).await;
    let other_slug = format!("other-{suffix}");
    let other = org(&db, &other_slug).await;
    let workspace = Uuid::new_v4();
    let docs = repo(
        &db,
        acme,
        "docs",
        &format!("file:///var/lib/g5/repos/{acme_slug}/docs.git"),
        None,
    )
    .await;
    let code = repo(
        &db,
        acme,
        "code",
        &format!("g5-local://workspaces/{workspace}.git"),
        None,
    )
    .await;
    let personal_slug = format!("personal-{workspace}");
    let personal = repo(
        &db,
        acme,
        &personal_slug,
        &format!("g5-local://personal/{workspace}.git"),
        None,
    )
    .await;
    let seeded = repo(&db, other, "tinydb", "file:///data/repos/pending", None).await;
    let path = repo(&db, other, "path", "/data/repos/fixture", None).await;
    let provider = repo(
        &db,
        acme,
        "provider",
        "https://github.com/example/provider.git",
        Some("github"),
    )
    .await;
    let remote = repo(&db, acme, "remote", "https://example.test/remote.git", None).await;
    // A provider-less repository a workspace tracks as a remote, through a
    // local `file://` fixture remote, keeps its URL.
    let tracked = repo(
        &db,
        acme,
        "tracked",
        "file:///srv/fixtures/tracked.git",
        None,
    )
    .await;
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Owner')")
        .bind(owner)
        .bind(format!("owner-{owner}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(acme)
        .bind(owner)
        .execute(&db.pool)
        .await
        .unwrap();
    let tracking_workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, org_id, name, is_base, owner_user_id, created_by)
         VALUES ($1, $2, $3, false, $4, $4)",
    )
    .bind(tracking_workspace)
    .bind(acme)
    .bind(format!("ws-{}", tracking_workspace.simple()))
    .bind(owner)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
         VALUES ($1, $2, $3, 'tracked', 'code')",
    )
    .bind(acme)
    .bind(tracking_workspace)
    .bind(tracked)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch)
         VALUES ($1, $2, $3, 'main')",
    )
    .bind(acme)
    .bind(tracking_workspace)
    .bind(tracked)
    .execute(&db.pool)
    .await
    .unwrap();

    let role = format!("scoped_urls_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.repos OWNER TO {role};
         ALTER TABLE public.orgs OWNER TO {role};
         ALTER TABLE public.remote_integrations OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    let mut migration_url = url::Url::parse(&db.system_url).unwrap();
    migration_url.set_username(&role).unwrap();
    migration_url.set_password(Some("migration_test")).unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    let applied = run_migration_command(migration_url.as_str(), catalog.path()).await;
    assert!(
        applied.status.success(),
        "migration 150 failed:\n{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    assert_eq!(url(&db, docs).await, format!("g5-local://{acme_slug}/docs"));
    assert_eq!(url(&db, code).await, format!("g5-local://{acme_slug}/code"));
    assert_eq!(
        url(&db, personal).await,
        format!("g5-local://{acme_slug}/{personal_slug}")
    );
    assert_eq!(
        url(&db, seeded).await,
        format!("g5-local://{other_slug}/tinydb")
    );
    assert_eq!(
        url(&db, path).await,
        format!("g5-local://{other_slug}/path")
    );
    assert_eq!(
        url(&db, provider).await,
        "https://github.com/example/provider.git"
    );
    assert_eq!(url(&db, remote).await, "https://example.test/remote.git");
    assert_eq!(url(&db, tracked).await, "file:///srv/fixtures/tracked.git");

    let forced: Vec<bool> = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class
          WHERE oid IN ('public.repos'::regclass, 'public.orgs'::regclass,
                        'public.remote_integrations'::regclass)",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(forced, vec![true; 3], "FORCE row-level security restored");

    let replay = run_migration_command(migration_url.as_str(), catalog.path()).await;
    assert!(
        replay.status.success(),
        "a repeated run is a no-op:\n{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(url(&db, code).await, format!("g5-local://{acme_slug}/code"));
}
