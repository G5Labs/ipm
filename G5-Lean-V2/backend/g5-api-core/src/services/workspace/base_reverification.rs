//! Base re-verification runs (g5.reverification).
//!
//! A run is a fork of Base whose merge back into Base re-establishes Base's
//! formal verification up to the Base commits the fork froze. One row per run
//! (`base_reverification_runs`) is inserted in the fork's own admission
//! transaction and never updated (g5.reverification.run). Its state is read
//! from the fork, the workspace and that workspace's pull requests into Base:
//! completed at its first merged pull request into Base, abandoned when the
//! fork failed or the workspace was deleted first, otherwise in progress.
//!
//! Base's re-verification status (g5.reverification.status) counts what Base
//! gained since the last completed run forked: the pull requests merged into
//! Base, and the net lines in code repositories outside `.g5/`, from Git and
//! G5's records alone.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{AuthorityTarget, HistoricalGitVersion, HistoricalReadReason, RepoRole};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::git::repo_store::{OrgScope, TenantRepoStorage};
use crate::services::workspace_reader::{resolve_committed_snapshot, CommittedRepoReadView};

/// The two kinds of run (g5.reverification.scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunKind {
    /// Re-verifies what Base gained since the previous completed run forked.
    Incremental,
    /// Makes every model current at the run's own fork commits.
    Full,
}

impl RunKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incremental => "incremental",
            Self::Full => "full",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "incremental" => Some(Self::Incremental),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// A run's derived state; never stored (g5.reverification.run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    InProgress,
    Completed,
    Abandoned,
}

/// One code repository's Base commit, as a run froze it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHead {
    pub repo_id: Uuid,
    pub root_path: String,
    pub commit: String,
}

/// A run with its derived state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: Uuid,
    pub kind: RunKind,
    pub state: RunState,
    pub workspace_id: Uuid,
    /// `None` until the fork has created the workspace.
    pub workspace_name: Option<String>,
    /// The workspace was deleted, as completed runs' workspaces usually are,
    /// so its name no longer opens anything.
    pub workspace_deleted: bool,
    pub owner_user_id: Option<Uuid>,
    pub started_by: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// The label the run's findings carry on their `Run:` line.
    pub label: String,
    /// The Base commits the run forked from: its scope's end.
    pub base_heads: Vec<RunHead>,
    /// The previous completed run's `base_heads`: an incremental run's start.
    pub since_heads: Option<Vec<RunHead>>,
}

/// What a fork admission passes to record its run.
pub struct RunAdmission<'a> {
    pub org_id: Uuid,
    pub base_workspace_id: Uuid,
    /// The fork operation's id, which is also the new workspace's id.
    pub run_id: Uuid,
    pub started_by: Uuid,
    pub kind: RunKind,
    /// Each code member's frozen Base commit.
    pub base_heads: &'a [RunHead],
}

/// Record a run inside its fork's admission transaction, so the run and the
/// fork are admitted together or not at all. Starts are serialized per
/// organization, so at most one run is ever in progress.
pub async fn admit(conn: &mut PgConnection, admission: RunAdmission<'_>) -> Result<(), AppError> {
    if admission.base_heads.is_empty() {
        return Err(validation(
            "reverification",
            "Base has no code repository to re-verify",
        ));
    }
    refuse_while_another_runs(&mut *conn, admission.org_id, admission.base_workspace_id).await?;

    // The run to continue from is the latest completed one, however many
    // abandoned runs followed it.
    let since_run_id = match admission.kind {
        RunKind::Full => None,
        RunKind::Incremental => Some(
            last_completed(&mut *conn, admission.org_id, admission.base_workspace_id)
                .await?
                .map(|run| run.id)
                .ok_or_else(|| {
                    validation(
                        "reverification.kind",
                        "An incremental re-verification needs a completed re-verification \
                         first; start a full one",
                    )
                })?,
        ),
    };
    let base_heads =
        serde_json::to_value(admission.base_heads).map_err(|error| G5Error::Internal {
            message: format!("encode re-verification heads: {error}"),
        })?;
    sqlx::query(
        r#"
        INSERT INTO base_reverification_runs (
            id, org_id, kind, base_workspace_id, workspace_id, started_by, base_heads,
            since_run_id, created_at
        ) VALUES ($1, $2, $3, $4, $1, $5, $6, $7, clock_timestamp())
        "#,
    )
    .bind(admission.run_id)
    .bind(admission.org_id)
    .bind(admission.kind.as_str())
    .bind(admission.base_workspace_id)
    .bind(admission.started_by)
    .bind(base_heads)
    .bind(since_run_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A fork still pending this long has failed: a fork of Base only creates
/// refs, so a live one finishes in seconds. Its run reads as abandoned, and
/// the next start, a retry of that fork included, aborts the fork so it can
/// never finish afterwards.
pub const STALE_FORK_MINUTES: i32 = 30;

/// The run, other than one now being admitted, that can still finish: its
/// workspace's name (or the fork's proposed name while it is being created).
async fn blocking_run(
    conn: &mut PgConnection,
    org_id: Uuid,
    base_workspace_id: Uuid,
) -> Result<Option<Option<String>>, AppError> {
    Ok(sqlx::query_scalar::<_, Option<String>>(&format!(
        "SELECT COALESCE(workspace.name, fork.proposed_name) \
         FROM base_reverification_runs AS run \
         JOIN workspace_forks AS fork ON fork.id = run.id \
         LEFT JOIN workspaces AS workspace ON workspace.id = run.workspace_id \
         WHERE run.org_id = $1 AND run.base_workspace_id = $2 \
           AND fork.status IN ('pending', 'completed') \
           AND workspace.deleted_at IS NULL \
           AND {COMPLETED_AT} IS NULL \
         ORDER BY run.created_at DESC, run.id LIMIT 1"
    ))
    .bind(org_id)
    .bind(base_workspace_id)
    .fetch_optional(conn)
    .await?)
}

/// Runs whose fork is still pending, with no workspace, after
/// [`STALE_FORK_MINUTES`]: forks the next start aborts.
pub async fn stale_run_forks(
    conn: &mut PgConnection,
    org_id: Uuid,
    base_workspace_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT run.id FROM base_reverification_runs AS run \
         JOIN workspace_forks AS fork ON fork.id = run.id \
         WHERE run.org_id = $1 AND run.base_workspace_id = $2 \
           AND fork.status = 'pending' \
           AND NOT EXISTS (SELECT 1 FROM workspaces WHERE id = run.workspace_id) \
           AND fork.created_at < now() - make_interval(mins => $3)",
    )
    .bind(org_id)
    .bind(base_workspace_id)
    .bind(STALE_FORK_MINUTES)
    .fetch_all(conn)
    .await?)
}

/// Take the organization's start lock for this transaction and refuse while
/// any other run can still finish: its fork has not failed, its workspace is
/// not deleted and it has not merged into Base. A fork still pending counts
/// however old it is; a start aborts stale ones before it gets here
/// (`stale_run_forks`). The fork admission calls this before claiming its
/// name, so the losing start of a race hears why.
pub async fn refuse_while_another_runs(
    conn: &mut PgConnection,
    org_id: Uuid,
    base_workspace_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("g5:base-reverification:{org_id}"))
        .execute(&mut *conn)
        .await?;
    match blocking_run(&mut *conn, org_id, base_workspace_id).await? {
        Some(name) => Err(in_progress(name.as_deref())),
        None => Ok(()),
    }
}

/// The organization's runs on `base_workspace_id`, newest first.
pub async fn list_runs(
    conn: &mut PgConnection,
    org_id: Uuid,
    base_workspace_id: Uuid,
    limit: i64,
) -> Result<Vec<Run>, AppError> {
    let rows = sqlx::query_as::<_, RunRow>(&format!(
        "{} ORDER BY run.created_at DESC, run.id LIMIT $3",
        run_select(false)
    ))
    .bind(org_id)
    .bind(base_workspace_id)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    resolve_since_heads(conn, org_id, rows).await
}

/// One run by id (the fork's id), if this organization has it.
pub async fn run_by_id(
    conn: &mut PgConnection,
    org_id: Uuid,
    run_id: Uuid,
) -> Result<Option<Run>, AppError> {
    let rows = sqlx::query_as::<_, RunRow>(&format!("{} AND run.id = $2", run_select(true)))
        .bind(org_id)
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(resolve_since_heads(conn, org_id, rows).await?.pop())
}

/// The latest completed run, whose fork commits are the stale point.
async fn last_completed(
    conn: &mut PgConnection,
    org_id: Uuid,
    base_workspace_id: Uuid,
) -> Result<Option<Run>, AppError> {
    let rows = sqlx::query_as::<_, RunRow>(&format!(
        "{} AND {COMPLETED_AT} IS NOT NULL ORDER BY {COMPLETED_AT} DESC, run.id LIMIT 1",
        run_select(false)
    ))
    .bind(org_id)
    .bind(base_workspace_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(resolve_since_heads(conn, org_id, rows).await?.pop())
}

/// A run completes at its workspace's first merged pull request into Base.
const COMPLETED_AT: &str = "(SELECT min(pr.merged_at) FROM workspace_pull_requests AS pr \
     WHERE pr.org_id = run.org_id AND pr.source_workspace_id = run.workspace_id \
       AND pr.target_workspace_id = run.base_workspace_id AND pr.status = 'merged')";

const RUN_COLUMNS: &str = "run.id, run.kind, run.workspace_id, run.started_by, run.created_at, \
     run.base_heads, run.since_run_id, fork.status AS fork_status, \
     fork.owner_user_id AS fork_owner_user_id, \
     workspace.name AS workspace_name, workspace.owner_user_id, \
     workspace.deleted_at AS workspace_deleted_at";

/// The run query with its derived state's inputs, for one organization
/// (`$1`) and, unless `any_base`, one Base (`$2`).
fn run_select(any_base: bool) -> String {
    let base = if any_base {
        ""
    } else {
        " AND run.base_workspace_id = $2"
    };
    format!(
        "SELECT {RUN_COLUMNS}, {COMPLETED_AT} AS completed_at, \
         (fork.status = 'pending' AND workspace.id IS NULL \
          AND fork.created_at < now() - make_interval(mins => {STALE_FORK_MINUTES})) \
             AS fork_stale \
         FROM base_reverification_runs AS run \
         JOIN workspace_forks AS fork ON fork.id = run.id \
         LEFT JOIN workspaces AS workspace ON workspace.id = run.workspace_id \
         WHERE run.org_id = $1{base}"
    )
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    kind: String,
    workspace_id: Uuid,
    started_by: Uuid,
    created_at: DateTime<Utc>,
    base_heads: serde_json::Value,
    since_run_id: Option<Uuid>,
    fork_status: String,
    fork_owner_user_id: Uuid,
    fork_stale: bool,
    workspace_name: Option<String>,
    owner_user_id: Option<Uuid>,
    workspace_deleted_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

fn run_from_row(row: RunRow, since_heads: Option<Vec<RunHead>>) -> Result<Run, AppError> {
    let kind = RunKind::parse(&row.kind).ok_or_else(|| G5Error::Internal {
        message: format!("unknown re-verification kind {:?}", row.kind),
    })?;
    let state = if row.completed_at.is_some() {
        RunState::Completed
    } else if matches!(row.fork_status.as_str(), "aborting" | "aborted")
        || row.fork_stale
        || row.workspace_deleted_at.is_some()
    {
        RunState::Abandoned
    } else {
        RunState::InProgress
    };
    let base_heads = decode_heads(row.base_heads)?;
    Ok(Run {
        id: row.id,
        kind,
        state,
        workspace_id: row.workspace_id,
        workspace_name: row.workspace_name,
        workspace_deleted: row.workspace_deleted_at.is_some(),
        // Until the fork creates the workspace, its owner is the starter.
        owner_user_id: row.owner_user_id.or(Some(row.fork_owner_user_id)),
        started_by: row.started_by,
        started_at: row.created_at,
        completed_at: row.completed_at,
        label: run_label(row.created_at, kind),
        base_heads,
        since_heads,
    })
}

/// The label a run's findings carry, e.g. `Base re-verification 2026-11-06
/// (incremental)`.
pub fn run_label(started_at: DateTime<Utc>, kind: RunKind) -> String {
    format!(
        "Base re-verification {} ({})",
        started_at.format("%Y-%m-%d"),
        kind.as_str()
    )
}

async fn resolve_since_heads(
    conn: &mut PgConnection,
    org_id: Uuid,
    rows: Vec<RunRow>,
) -> Result<Vec<Run>, AppError> {
    let since_ids = rows
        .iter()
        .filter_map(|row| row.since_run_id)
        .collect::<Vec<_>>();
    let mut since = HashMap::new();
    if !since_ids.is_empty() {
        let found = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
            "SELECT id, base_heads FROM base_reverification_runs \
             WHERE org_id = $1 AND id = ANY($2)",
        )
        .bind(org_id)
        .bind(&since_ids)
        .fetch_all(&mut *conn)
        .await?;
        for (id, heads) in found {
            since.insert(id, decode_heads(heads)?);
        }
    }
    rows.into_iter()
        .map(|row| {
            let since_heads = row.since_run_id.and_then(|id| since.get(&id).cloned());
            run_from_row(row, since_heads)
        })
        .collect()
}

fn decode_heads(value: serde_json::Value) -> Result<Vec<RunHead>, AppError> {
    serde_json::from_value(value).map_err(|error| {
        G5Error::Internal {
            message: format!("decode re-verification heads: {error}"),
        }
        .into()
    })
}

/// The overall status Home shows (g5.ui.home-reverification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    /// No run has completed yet.
    Never,
    /// Nothing changed in Base since the last completed run forked.
    Current,
    Stale,
    InProgress,
}

/// A pull request merged into Base since the last completed run forked.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MergedPullRequest {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub merged_at: DateTime<Utc>,
    pub merged_by: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCounts {
    pub added: u64,
    pub removed: u64,
}

/// Base's re-verification status (g5.reverification.status).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub base_workspace_id: Uuid,
    pub base_workspace_name: String,
    pub state: StatusState,
    pub last_completed: Option<Run>,
    pub in_progress: Option<Run>,
    /// Exact count of pull requests merged since the stale point.
    pub pull_request_count: i64,
    /// Newest first, at most [`PULL_REQUEST_LIMIT`].
    pub pull_requests: Vec<MergedPullRequest>,
    /// `None` before any run completes.
    pub lines: Option<LineCounts>,
    /// Some code repository could not be compared.
    pub lines_unavailable: bool,
    /// Newest first, at most [`RUN_LIMIT`].
    pub runs: Vec<Run>,
    /// Every run on record, of which `runs` lists the newest.
    pub run_count: i64,
}

pub const PULL_REQUEST_LIMIT: i64 = 200;
pub const RUN_LIMIT: i64 = 50;

/// The organization's Base, which re-verification is about.
pub async fn base_workspace(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<Option<(Uuid, String)>, AppError> {
    Ok(sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, name FROM workspaces \
         WHERE org_id = $1 AND is_base AND deleted_at IS NULL \
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(org_id)
    .fetch_optional(conn)
    .await?)
}

/// Whether `workspace_id` is a run's workspace and `target_id` that run's
/// Base. Such a workspace reaches Base only through a pull request, whose
/// merge completes the run; a direct merge would leave no record of it.
pub async fn is_run_into_base(
    conn: &mut PgConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    target_id: Uuid,
) -> Result<bool, G5Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM base_reverification_runs \
         WHERE org_id = $1 AND workspace_id = $2 AND base_workspace_id = $3)",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(target_id)
    .fetch_one(conn)
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("check Base re-verification run: {error}"),
    })
}

/// Whether `user_id` may start a run: anyone who can fork Base, by the fork
/// admission's own rule.
pub async fn can_start(
    system_pool: &sqlx::PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    crate::services::workspace::workspace_fork_service::fork_owner_is_eligible(
        system_pool,
        org_id,
        user_id,
    )
    .await
}

/// Base's re-verification status as read inside the caller's transaction;
/// [`complete_status`] counts its lines after that transaction ends, since
/// Git work never runs under a scoped database transaction.
pub struct StatusRead {
    status: Status,
    lines: Option<LinesPlan>,
}

/// The exact commits whose difference Base's lines count.
struct LinesPlan {
    org_id: Uuid,
    base_workspace_id: Uuid,
    since: Vec<RunHead>,
    /// Base's code members at their current commits, or why they could not
    /// be read.
    current: Result<Vec<g5_contracts::types::RepoBindingVersion>, G5Error>,
}

/// Read the runs, the pull requests since the stale point and Base's
/// current commits, in the caller's organization scope.
pub async fn read_status(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    org_id: Uuid,
    base_workspace_id: Uuid,
    base_workspace_name: String,
) -> Result<StatusRead, AppError> {
    let runs = list_runs(db.conn(), org_id, base_workspace_id, RUN_LIMIT).await?;
    let run_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM base_reverification_runs \
         WHERE org_id = $1 AND base_workspace_id = $2",
    )
    .bind(org_id)
    .bind(base_workspace_id)
    .fetch_one(db.conn())
    .await?;
    let in_progress = runs
        .iter()
        .find(|run| run.state == RunState::InProgress)
        .cloned();
    let last_completed = last_completed(db.conn(), org_id, base_workspace_id).await?;
    let mut status = Status {
        base_workspace_id,
        base_workspace_name,
        state: if in_progress.is_some() {
            StatusState::InProgress
        } else {
            StatusState::Never
        },
        last_completed: last_completed.clone(),
        in_progress,
        pull_request_count: 0,
        pull_requests: Vec::new(),
        lines: None,
        lines_unavailable: false,
        runs,
        run_count,
    };
    let Some(completed) = last_completed else {
        return Ok(StatusRead {
            status,
            lines: None,
        });
    };

    // A run's own pull request into Base, the one that completed it, counts
    // as the run; anything its workspace merges afterwards counts as usual.
    let merged_since = "FROM workspace_pull_requests AS pr \
         WHERE pr.org_id = $1 AND pr.target_workspace_id = $2 AND pr.status = 'merged' \
           AND pr.merged_at > $3 \
           AND NOT EXISTS ( \
               SELECT 1 FROM base_reverification_runs AS run \
               WHERE run.org_id = pr.org_id AND run.workspace_id = pr.source_workspace_id \
                 AND pr.merged_at <= ( \
                     SELECT min(first.merged_at) FROM workspace_pull_requests AS first \
                     WHERE first.org_id = run.org_id \
                       AND first.source_workspace_id = run.workspace_id \
                       AND first.target_workspace_id = run.base_workspace_id \
                       AND first.status = 'merged'))";
    status.pull_request_count =
        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) {merged_since}"))
            .bind(org_id)
            .bind(base_workspace_id)
            .bind(completed.started_at)
            .fetch_one(db.conn())
            .await?;
    status.pull_requests = sqlx::query_as::<_, MergedPullRequest>(&format!(
        "SELECT pr.id, pr.number, pr.title, pr.merged_at, pr.merged_by {merged_since} \
         ORDER BY pr.merged_at DESC, pr.id LIMIT $4"
    ))
    .bind(org_id)
    .bind(base_workspace_id)
    .bind(completed.started_at)
    .bind(PULL_REQUEST_LIMIT)
    .fetch_all(db.conn())
    .await?;

    let current = resolve_committed_snapshot(
        db,
        repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: base_workspace_id,
        },
    )
    .await
    .map(|snapshot| {
        snapshot
            .repos
            .into_iter()
            .filter(|binding| binding.repo_role == RepoRole::Code)
            .collect()
    });
    Ok(StatusRead {
        status,
        lines: Some(LinesPlan {
            org_id,
            base_workspace_id,
            since: completed.base_heads,
            current,
        }),
    })
}

/// Count Base's lines since the stale point and settle the state, after the
/// transaction that read the status has ended.
pub async fn complete_status(
    read: StatusRead,
    scope: &(impl OrgScope + ?Sized),
    repo_storage: &TenantRepoStorage,
) -> Status {
    let StatusRead { mut status, lines } = read;
    let Some(plan) = lines else {
        return status;
    };
    let (lines, unavailable) = lines_since(scope, repo_storage, plan).await;
    status.lines = Some(lines);
    status.lines_unavailable = unavailable;
    status.state = if status.in_progress.is_some() {
        StatusState::InProgress
    } else if status.pull_request_count == 0
        && lines.added == 0
        && lines.removed == 0
        && !unavailable
    {
        StatusState::Current
    } else {
        StatusState::Stale
    };
    status
}

/// Net lines added and removed in Base's code repositories, outside `.g5/`,
/// from the stale point to Base's current commits. A repository that cannot
/// be compared is left out and reported; one with no recorded start counts
/// from the empty tree.
async fn lines_since(
    scope: &(impl OrgScope + ?Sized),
    repo_storage: &TenantRepoStorage,
    plan: LinesPlan,
) -> (LineCounts, bool) {
    let mut total = LineCounts {
        added: 0,
        removed: 0,
    };
    let current = match plan.current {
        Ok(current) => current,
        Err(error) => {
            tracing::warn!(
                base_workspace_id = %plan.base_workspace_id,
                %error,
                "Base re-verification lines unavailable"
            );
            return (total, true);
        }
    };
    let mut unavailable = false;
    for binding in &current {
        let start = plan
            .since
            .iter()
            .find(|head| head.repo_id == binding.repo_id)
            .map(|head| head.commit.clone());
        let key = (binding.repo_id, start.clone(), binding.head_commit.clone());
        let counts = match cached_lines(&key) {
            Some(counts) => Ok(counts),
            None => {
                // One diff per exact commit pair at a time: concurrent Home
                // reads after Base moves wait for the first and reuse it,
                // while other pairs, such as other organizations', proceed.
                let gate = lines_gate(&key);
                let _held = gate.lock().await;
                match cached_lines(&key) {
                    Some(counts) => Ok(counts),
                    None => repo_lines(scope, repo_storage, plan.org_id, binding, start.as_deref())
                        .await
                        .inspect(|counts| remember_lines(key.clone(), *counts)),
                }
            }
        };
        match counts {
            Ok(counts) => {
                total.added += counts.added;
                total.removed += counts.removed;
            }
            Err(error) => {
                tracing::warn!(
                    base_workspace_id = %plan.base_workspace_id,
                    repo_id = %binding.repo_id,
                    %error,
                    "Base re-verification lines unavailable for a repository"
                );
                unavailable = true;
            }
        }
    }
    (total, unavailable)
}

/// The gate for one exact commit pair, shared by the reads waiting on it and
/// forgotten once none holds it.
fn lines_gate(key: &LinesKey) -> Arc<tokio::sync::Mutex<()>> {
    type Gates = Mutex<HashMap<LinesKey, Weak<tokio::sync::Mutex<()>>>>;
    static GATES: OnceLock<Gates> = OnceLock::new();
    let mut gates = GATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(key).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(tokio::sync::Mutex::new(()));
    gates.insert(key.clone(), Arc::downgrade(&gate));
    gate
}

async fn repo_lines(
    scope: &(impl OrgScope + ?Sized),
    repo_storage: &TenantRepoStorage,
    org_id: Uuid,
    binding: &g5_contracts::types::RepoBindingVersion,
    start: Option<&str>,
) -> Result<LineCounts, G5Error> {
    let current = CommittedRepoReadView::from_binding(scope, org_id, binding)?;
    let summary = match start {
        Some(start) => {
            let start = current.historical_sibling(&HistoricalGitVersion {
                repo_id: binding.repo_id,
                head_commit: start.to_string(),
                reason: HistoricalReadReason::Diff,
            })?;
            current
                .source_control_summary_from(repo_storage, &start)
                .await?
        }
        None => {
            current
                .source_control_summary_from_empty(repo_storage)
                .await?
        }
    };
    Ok(count_outside_g5(&summary))
}

fn count_outside_g5(summary: &g5_scm::DiffSummary) -> LineCounts {
    summary
        .files
        .iter()
        .filter(|file| !is_verification_material(&file.path))
        .fold(
            LineCounts {
                added: 0,
                removed: 0,
            },
            |total, file| LineCounts {
                added: total.added + file.additions as u64,
                removed: total.removed + file.deletions as u64,
            },
        )
}

/// Paths under `.g5/` hold ontology, models and findings, never counted.
fn is_verification_material(path: &str) -> bool {
    path == ".g5" || path.starts_with(".g5/")
}

/// A bounded in-process memo of per-repository counts, keyed by the exact
/// commits compared, so Home's polling costs a lookup until Base moves. Lost
/// freely; a miss recomputes.
type LinesKey = (Uuid, Option<String>, String);
const LINES_CACHE_LIMIT: usize = 256;

fn lines_cache() -> &'static Mutex<HashMap<LinesKey, LineCounts>> {
    static CACHE: OnceLock<Mutex<HashMap<LinesKey, LineCounts>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_lines(key: &LinesKey) -> Option<LineCounts> {
    lines_cache().lock().ok()?.get(key).copied()
}

fn remember_lines(key: LinesKey, counts: LineCounts) {
    if let Ok(mut cache) = lines_cache().lock() {
        if cache.len() >= LINES_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, counts);
    }
}

fn in_progress(workspace_name: Option<&str>) -> AppError {
    let workspace = workspace_name
        .map(|name| format!(" in workspace {name}"))
        .unwrap_or_default();
    G5Error::Conflict {
        reason: format!("A Base re-verification is already in progress{workspace}"),
    }
    .into()
}

fn validation(field: &str, message: &str) -> AppError {
    G5Error::ValidationError {
        details: serde_json::json!({ "field": field, "message": message }),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_name_the_start_date_and_kind() {
        let started = DateTime::parse_from_rfc3339("2026-11-06T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            run_label(started, RunKind::Incremental),
            "Base re-verification 2026-11-06 (incremental)"
        );
        assert_eq!(
            run_label(started, RunKind::Full),
            "Base re-verification 2026-11-06 (full)"
        );
    }

    #[test]
    fn line_diffs_wait_only_for_the_same_commit_pair() {
        let repo = Uuid::new_v4();
        let pair = (repo, Some("a".repeat(40)), "b".repeat(40));
        let other = (repo, None, "b".repeat(40));
        let held = lines_gate(&pair);
        assert!(Arc::ptr_eq(&held, &lines_gate(&pair)));
        assert!(!Arc::ptr_eq(&held, &lines_gate(&other)));
        let guard = held.try_lock().expect("a free gate");
        assert!(lines_gate(&pair).try_lock().is_err(), "the same pair waits");
        assert!(
            lines_gate(&other).try_lock().is_ok(),
            "another pair does not"
        );
        drop(guard);
        let weak = Arc::downgrade(&held);
        drop(held);
        assert!(weak.upgrade().is_none(), "an unheld gate is forgotten");
    }

    #[test]
    fn verification_material_is_only_the_g5_root() {
        assert!(is_verification_material(".g5/ontology/root.g5.md"));
        assert!(is_verification_material(
            ".g5/TLA_verification/models/a/A.tla"
        ));
        assert!(is_verification_material(".g5"));
        assert!(!is_verification_material("src/.g5/notes.md"));
        assert!(!is_verification_material(".g5x/file"));
        assert!(!is_verification_material("backend/src/lib.rs"));
    }
}
