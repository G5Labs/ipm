//! Approval phase cutover (migrations 066–068, g5.workbench.gates) on a
//! populated pre-cutover catalog, run by a table-owner migrator without
//! BYPASSRLS the way Cloud SQL runs it: every stage binding and decision
//! maps onto phases, in-flight requests and runs close, the stage columns
//! and the per-workspace stage count go, and the run is idempotent.
use crate::helpers;

use helpers::TestDb;
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::path::Path;
use uuid::Uuid;

const PASSWORD: &str = "migration_066_disposable_test";
/// The last migration before the phase cutover.
const PRE_CUTOVER: i64 = 65;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn catalog_through(last: i64) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql") {
            let version: i64 = entry
                .file_name()
                .to_string_lossy()
                .split('_')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            if version <= last {
                std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
            }
        }
    }
    catalog
}

struct Fixture {
    db: TestDb,
    role: String,
    migrator: PgPool,
}

impl Fixture {
    /// A database migrated through `PRE_CUTOVER`, owned by a Cloud SQL-like
    /// role (no SUPERUSER, no BYPASSRLS) that runs the cutover.
    async fn new() -> Self {
        let baseline = catalog_through(PRE_CUTOVER);
        let db = TestDb::create_from_migrations(baseline.path()).await;
        let role = format!("migration_066_{}", Uuid::new_v4().simple());
        // The deployed migrator owns authz after creating it in the baseline;
        // replacing a definer function can require granting temporary CREATE.
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{PASSWORD}' NOSUPERUSER NOBYPASSRLS CREATEDB CREATEROLE;
             GRANT CREATE ON DATABASE \"{database}\" TO {role};
             GRANT USAGE, CREATE ON SCHEMA public TO {role};
             ALTER SCHEMA authz OWNER TO {role};
             GRANT authz_ctx_writer, authz_writer TO {role} WITH ADMIN OPTION;
             GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;",
            database = db.db_name
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
             WHERE n.nspname='public' AND c.relkind IN ('r','p')",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        for table in tables {
            sqlx::raw_sql(&format!(
                "ALTER TABLE public.\"{}\" OWNER TO {role}",
                table.replace('"', "\"\"")
            ))
            .execute(&db.pool)
            .await
            .unwrap();
        }
        // Match the baseline's migrator-owned functions when handing the
        // seeded catalog to the non-superuser. Migrations 076 and 083 replace
        // ordinary functions; 087 moves the credential bootstrap helpers to
        // the BYPASSRLS definer before forcing RLS on their tables.
        sqlx::raw_sql(&format!(
            "ALTER FUNCTION public.g5_provider_request_is_immutable() OWNER TO {role}; \
             ALTER FUNCTION public.g5_config_value_is_registered(text, text, jsonb) OWNER TO {role}; \
             ALTER FUNCTION authz.lookup_api_key_by_hash(text) OWNER TO {role}; \
             ALTER FUNCTION authz.lookup_worker_token_by_hash(text) OWNER TO {role}; \
             ALTER FUNCTION authz.touch_api_key_last_used(uuid,text) OWNER TO {role}"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        let migrator = PgPoolOptions::new()
            .max_connections(3)
            .connect_with(
                db.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .username(&role)
                    .password(PASSWORD),
            )
            .await
            .unwrap();
        let privileges: (bool, bool) = sqlx::query_as(
            "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&migrator)
        .await
        .unwrap();
        assert_eq!(privileges, (false, false));
        Self { db, role, migrator }
    }

    async fn cleanup(self) {
        self.migrator.close().await;
        sqlx::raw_sql(&format!(
            "REASSIGN OWNED BY {0} TO CURRENT_USER; DROP OWNED BY {0}; DROP ROLE {0};",
            self.role
        ))
        .execute(&self.db.pool)
        .await
        .unwrap();
        self.db.drop().await;
    }
}

/// Every active agent review state the cutover must cancel.
const ACTIVE_RUN_STATES: [&str; 4] = [
    "pending_assignment",
    "assigned",
    "generated",
    "merged_to_draft",
];

/// Stage-based rows as a tenant left them before the cutover: one
/// workspace per stage count, each with decisions at both stages, a pending
/// and a settled request, an agent review run in every active state and a
/// terminal one.
struct Seed {
    org: Uuid,
    user: Uuid,
    /// (workspace id, stages).
    workspaces: Vec<(Uuid, i32)>,
    /// (definition id, stage_planning, stage_finalizing, planning_fallback).
    definitions: Vec<(Uuid, bool, bool, &'static str)>,
    /// (status id, stage) across both workspaces.
    statuses: Vec<(Uuid, &'static str)>,
    pending_requests: Vec<Uuid>,
    settled_requests: Vec<Uuid>,
    /// (job id, pre-cutover run state) for every active state.
    active_runs: Vec<(Uuid, &'static str)>,
    failed_run_jobs: Vec<Uuid>,
}

/// Advance a freshly inserted run (pending assignment) to `state` through
/// the legal transitions, with the evidence each state's shape requires.
async fn advance_run(
    pool: &PgPool,
    job_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    task_id: Uuid,
    state: &str,
) -> Result<(), sqlx::Error> {
    if state == "pending_assignment" {
        return Ok(());
    }
    let heads = json!([{
        "repoId": repo_id,
        "branchName": "main",
        "workspaceId": workspace_id,
        "headCommit": "a".repeat(40),
    }]);
    sqlx::query(
        "UPDATE approval_review_runs
            SET state = 'assigned', resolution_id = $2, resolved_bootstrap_task_id = $3,
                target_workspace_id = $4, source_heads = $5, target_heads = $5,
                output_repo_id = $6,
                output_doc_path = 'docs/' || $4::text || '/reviews/agent-approval-' || $1::text || '.md',
                resolved_at = now()
          WHERE job_id = $1",
    )
    .bind(job_id)
    .bind(format!("apr_{}{}", job_id.simple(), job_id.simple()))
    .bind(task_id)
    .bind(workspace_id)
    .bind(&heads)
    .bind(repo_id)
    .execute(pool)
    .await?;
    if state == "assigned" {
        return Ok(());
    }
    sqlx::query(
        "UPDATE approval_review_runs
            SET state = 'generated', generated_result = 'approved', generated_task_id = $2,
                generated_attempt_id = $3, generated_job_commit_sha = repeat('b', 40),
                generated_blob_oid = repeat('c', 40), generated_byte_size = 10,
                generated_content_sha256 = repeat('d', 64),
                generated_media_type = 'text/markdown; charset=utf-8', generated_at = now()
          WHERE job_id = $1",
    )
    .bind(job_id)
    .bind(task_id)
    .bind(Uuid::new_v4())
    .execute(pool)
    .await?;
    if state == "generated" {
        return Ok(());
    }
    sqlx::query(
        "UPDATE approval_review_runs SET state = 'merged_to_draft', merged_to_draft_at = now()
          WHERE job_id = $1",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed(pool: &PgPool) -> Result<Seed, sqlx::Error> {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Phase cutover',$2)")
        .bind(org)
        .bind(format!("phases-{org}"))
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Approver')")
        .bind(user)
        .bind(format!("{user}@g5.test"))
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(pool)
        .await?;
    let mut definitions = Vec::new();
    for (planning, finalizing) in [(true, true), (true, false), (false, true), (false, false)] {
        for fallback in ["carry_over", "skip"] {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO approval_definitions
                    (id, version, org_id, name, humans_list, created_by,
                     stage_planning, stage_finalizing, planning_fallback)
                 VALUES ($1, 1, $2, $3, ARRAY[$4]::uuid[], $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(org)
            .bind(format!("p={planning} f={finalizing} {fallback}"))
            .bind(user)
            .bind(planning)
            .bind(finalizing)
            .bind(fallback)
            .execute(pool)
            .await?;
            definitions.push((id, planning, finalizing, fallback));
        }
    }
    let both = definitions[0].0;
    let repo: Uuid = sqlx::query_scalar(
        "INSERT INTO repos(org_id, name, slug, git_url, protected_branches)
         VALUES ($1, 'code', 'code', 'https://example.test/code.git', ARRAY['main'])
         RETURNING id",
    )
    .bind(org)
    .fetch_one(pool)
    .await?;

    let mut workspaces = Vec::new();
    let mut statuses = Vec::new();
    let mut pending_requests = Vec::new();
    let mut settled_requests = Vec::new();
    let mut active_runs = Vec::new();
    let mut failed_run_jobs = Vec::new();
    // One workspace per stage count: the per-workspace stage count is the
    // model the cutover removes, whatever its value.
    for stages in [1, 2] {
        let workspace = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by,stages)
             VALUES($1,$2,$3,false,$4,$4,$5)",
        )
        .bind(workspace)
        .bind(org)
        .bind(format!("stages-{stages}"))
        .bind(user)
        .bind(stages)
        .execute(pool)
        .await?;
        workspaces.push((workspace, stages));

        for stage in ["planning", "finalizing"] {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO approval_statuses
                    (org_id, source_workspace_id, approval_definition_id,
                     approval_definition_version, outcome, actor_type, actor_user_id,
                     reviewed_git_heads, stage)
                 VALUES ($1, $2, $3, 1, 'approved', 'human', $4, '[]'::jsonb, $5)
                 RETURNING id",
            )
            .bind(org)
            .bind(workspace)
            .bind(both)
            .bind(user)
            .bind(stage)
            .fetch_one(pool)
            .await?;
            statuses.push((id, stage));
        }

        let next_key = || format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let pending: Uuid = sqlx::query_scalar(
            "INSERT INTO approval_requests
                (org_id, source_workspace_id, approval_definition_id,
                 approval_definition_version, request_key, actor_type, actor_user_id,
                 requested_by)
             VALUES ($1, $2, $3, 1, $4, 'human', $5, $5)
             RETURNING id",
        )
        .bind(org)
        .bind(workspace)
        .bind(both)
        .bind(next_key())
        .bind(user)
        .fetch_one(pool)
        .await?;
        pending_requests.push(pending);
        let settled: Uuid = sqlx::query_scalar(
            "INSERT INTO approval_requests
                (org_id, source_workspace_id, approval_definition_id,
                 approval_definition_version, request_key, actor_type, actor_user_id,
                 requested_by, state, settlement_fingerprint, closed_at)
             VALUES ($1, $2, $3, 1, $4, 'human', $5, $5, 'settled', $6, now())
             RETURNING id",
        )
        .bind(org)
        .bind(workspace)
        .bind(both)
        .bind(next_key())
        .bind(user)
        .bind("f".repeat(64))
        .fetch_one(pool)
        .await?;
        settled_requests.push(settled);

        // Agent review runs: one in every active state (their jobs still
        // running under a stage-shaped v3 context) and one already terminal.
        let mut runs = ACTIVE_RUN_STATES
            .iter()
            .map(|state| (*state, "pending", "running"))
            .collect::<Vec<_>>();
        runs.push(("failed", "cancelled", "failed"));
        for (state, request_state, job_status) in runs {
            let request_key = next_key();
            let request: Uuid = sqlx::query_scalar(
                "INSERT INTO approval_requests
                    (org_id, source_workspace_id, approval_definition_id,
                     approval_definition_version, request_key, actor_type,
                     requested_by, state, closed_at)
                 VALUES ($1, $2, $3, 1, $4, 'agent', $5, $6, $7)
                 RETURNING id",
            )
            .bind(org)
            .bind(workspace)
            .bind(both)
            .bind(&request_key)
            .bind(user)
            .bind(request_state)
            .bind(if request_state == "pending" {
                None
            } else {
                Some(chrono::Utc::now())
            })
            .fetch_one(pool)
            .await?;
            let job = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO jobs
                    (id, org_id, workspace_id, created_by, name, job_type, status,
                     priority, approval_context)
                 VALUES ($1, $2, $3, $4, 'stage review', 'approval_review', $5, 0, $6)",
            )
            .bind(job)
            .bind(org)
            .bind(workspace)
            .bind(user)
            .bind(job_status)
            .bind(json!({
                "schemaVersion": 3,
                "approvalRequestId": request,
                "requestKey": request_key,
                "definitionId": both,
                "definitionVersion": 1,
                "stage": "planning",
            }))
            .execute(pool)
            .await?;
            let task = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO tasks(id, org_id, workspace_id, job_id, name, kind, status, created_by)
                 VALUES ($1, $2, $3, $4, 'Review', 'command', $5, $6)",
            )
            .bind(task)
            .bind(org)
            .bind(workspace)
            .bind(job)
            .bind(job_status)
            .bind(user)
            .execute(pool)
            .await?;
            sqlx::query(
                "INSERT INTO approval_review_runs
                    (job_id, org_id, source_workspace_id, approval_request_id,
                     definition_id, definition_version, request_key, state)
                 VALUES ($1, $2, $3, $4, $5, 1, $6, 'pending_assignment')",
            )
            .bind(job)
            .bind(org)
            .bind(workspace)
            .bind(request)
            .bind(both)
            .bind(&request_key)
            .execute(pool)
            .await?;
            if state == "failed" {
                sqlx::query("UPDATE approval_review_runs SET state = 'failed' WHERE job_id = $1")
                    .bind(job)
                    .execute(pool)
                    .await?;
                failed_run_jobs.push(job);
            } else {
                advance_run(pool, job, workspace, repo, task, state).await?;
                active_runs.push((job, state));
            }
        }
    }

    Ok(Seed {
        org,
        user,
        workspaces,
        definitions,
        statuses,
        pending_requests,
        settled_requests,
        active_runs,
        failed_run_jobs,
    })
}

async fn column_exists(pool: &PgPool, table: &str, column: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
         WHERE table_schema='public' AND table_name=$1 AND column_name=$2)",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
}

async fn assert_cut_over(pool: &PgPool, seed: &Seed) -> TestResult {
    for (id, planning, finalizing, fallback) in &seed.definitions {
        let phases: Vec<String> =
            sqlx::query_scalar("SELECT phases FROM approval_definitions WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await?;
        let expected: &[&str] = match (planning, finalizing) {
            (true, true) => &["be", "ie"],
            (true, false) => &["be"],
            (false, true) => &["ie"],
            (false, false) => &[],
        };
        assert_eq!(
            phases, expected,
            "planning={planning} finalizing={finalizing} fallback={fallback}"
        );
    }
    for (id, stage) in &seed.statuses {
        let phase: String = sqlx::query_scalar("SELECT phase FROM approval_statuses WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?;
        let expected = if *stage == "planning" { "be" } else { "ie" };
        assert_eq!(phase, expected, "status at stage {stage}");
    }

    for id in &seed.pending_requests {
        let (state, closed, phase): (String, bool, Option<String>) = sqlx::query_as(
            "SELECT state, closed_at IS NOT NULL, phase FROM approval_requests WHERE id=$1",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        assert_eq!((state.as_str(), closed, phase), ("superseded", true, None));
    }
    for id in &seed.settled_requests {
        let state: String = sqlx::query_scalar("SELECT state FROM approval_requests WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?;
        assert_eq!(state, "settled");
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_requests WHERE org_id=$1 AND state='pending'",
    )
    .bind(seed.org)
    .fetch_one(pool)
    .await?;
    assert_eq!(pending, 0);

    let run_state = |job: Uuid| async move {
        sqlx::query_scalar::<_, String>("SELECT state FROM approval_review_runs WHERE job_id=$1")
            .bind(job)
            .fetch_one(pool)
            .await
    };
    for (job, before) in &seed.active_runs {
        assert_eq!(run_state(*job).await?, "cancelled", "run that was {before}");
        // The job itself is left to the API's startup pass.
        let job_status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=$1")
            .bind(job)
            .fetch_one(pool)
            .await?;
        assert_eq!(job_status, "running", "run that was {before}");
    }
    for job in &seed.failed_run_jobs {
        assert_eq!(run_state(*job).await?, "failed");
    }

    for (table, column) in [
        ("approval_definitions", "stage_planning"),
        ("approval_definitions", "stage_finalizing"),
        ("approval_definitions", "planning_fallback"),
        ("approval_statuses", "stage"),
        ("workspaces", "stages"),
    ] {
        assert!(
            !column_exists(pool, table, column).await?,
            "{table}.{column} must be dropped"
        );
    }
    for (table, column) in [
        ("workspace_pull_requests", "merge_admission"),
        ("job_merge_runs", "approval_admission"),
        ("approval_requests", "phase"),
    ] {
        assert!(
            column_exists(pool, table, column).await?,
            "{table}.{column} must exist"
        );
    }
    let forced: Vec<bool> = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE relname = ANY($1) ORDER BY relname",
    )
    .bind(vec![
        "approval_definitions",
        "approval_statuses",
        "approval_requests",
        "approval_review_runs",
    ])
    .fetch_all(pool)
    .await?;
    assert_eq!(forced, vec![true; 4], "FORCE RLS restored");
    Ok(())
}

#[tokio::test]
async fn populated_upgrade_maps_stages_onto_phases_closes_in_flight_work_and_replays() {
    let f = Fixture::new().await;
    let result: TestResult = async {
        let seed = seed(&f.db.pool).await?;
        let catalog = catalog_through(68);
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_cut_over(&f.db.pool, &seed).await?;

        // Replaying this historical cutover leaves its rows unchanged.
        g5_api::db::run_migrations_from_path(&f.migrator, catalog.path()).await?;
        assert_cut_over(&f.db.pool, &seed).await?;
        let (applied, recorded): (i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE version IN (66, 67, 68)), count(*)
             FROM public._sqlx_migrations WHERE success",
        )
        .fetch_one(&f.db.pool)
        .await?;
        assert_eq!(applied, 3);
        assert!(recorded >= 68);

        // New rows follow the phase contracts: a decision names its phase
        // (no default remains), a definition only binds the three phases,
        // and a phase-aware review context is accepted by the jobs CHECK.
        let both = seed.definitions[0].0;
        let without_phase = sqlx::query(
            "INSERT INTO approval_statuses
                (org_id, source_workspace_id, approval_definition_id,
                 approval_definition_version, outcome, actor_type, actor_user_id,
                 reviewed_git_heads)
             VALUES ($1, $2, $3, 1, 'approved', 'human', $4, '[]'::jsonb)",
        )
        .bind(seed.org)
        .bind(seed.workspaces[0].0)
        .bind(both)
        .bind(seed.user)
        .execute(&f.db.pool)
        .await;
        assert!(without_phase.is_err(), "a decision must name its phase");
        let bad_phases = sqlx::query(
            "INSERT INTO approval_definitions
                (version, org_id, name, humans_list, created_by, phases)
             VALUES (1, $1, 'bad', ARRAY[$2]::uuid[], $2, ARRAY['planning']::text[])",
        )
        .bind(seed.org)
        .bind(seed.user)
        .execute(&f.db.pool)
        .await;
        assert!(bad_phases.is_err(), "only be|se|ie bind");
        let request: Uuid = sqlx::query_scalar(
            "INSERT INTO approval_requests
                (org_id, source_workspace_id, approval_definition_id,
                 approval_definition_version, request_key, actor_type,
                 requested_by, phase)
             VALUES ($1, $2, $3, 1, $4, 'agent', $5, 'se')
             RETURNING id",
        )
        .bind(seed.org)
        .bind(seed.workspaces[0].0)
        .bind(both)
        .bind("5".repeat(64))
        .bind(seed.user)
        .fetch_one(&f.db.pool)
        .await?;
        sqlx::query(
            "INSERT INTO jobs
                (id, org_id, workspace_id, created_by, name, job_type, status,
                 priority, approval_context)
             VALUES ($1, $2, $3, $4, 'phase review', 'approval_review', 'pending', 0, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(seed.org)
        .bind(seed.workspaces[0].0)
        .bind(seed.user)
        .bind(json!({
            "schemaVersion": 3,
            "approvalRequestId": request,
            "requestKey": "5".repeat(64),
            "definitionId": both,
            "definitionVersion": 1,
            "phase": "se",
        }))
        .execute(&f.db.pool)
        .await?;
        Ok(())
    }
    .await;
    f.cleanup().await;
    result.unwrap();
}
