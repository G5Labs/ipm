//! Platform-shipped in-process modules: `workspace.changes` and
//! `workspace.diagnostics`. Both are thin, typed projections; neither accepts
//! caller argv, and every git invocation is a fixed command line over the
//! member checkout with the runtime's excludes file. Network Git goes through
//! the token-isolated [`crate::git_transport`], never the checkout's mutable
//! `origin` and never with a credential in reach of repository Git.
//!
//! Built-ins run under the same wall clock and cancellation as process
//! modules: every git child is supervised, stopped with its process group on
//! cancel or deadline, and reaped before the outcome settles, so the admission
//! permit is never released while a member checkout is still being mutated.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use g5_scm::native::{
    FailureKind, MemberAdopt, MemberAdoptFailure, MemberCheckpoint, MemberGit, MemberParentUpdate,
    MemberPathSync, MemberPathSyncOutcome, MemberPush, MemberRebase, MemberRebaseFailure,
    MemberTrackingRefresh, NativeCancellation, NativeGitError,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::runner::outcome;
use super::{ExecId, ModuleEntry, ModuleRuntime};
use crate::config::RepositoryLayout;
use crate::protocol::RpcError;

pub const CHANGES: &str = "workspace.changes";
pub const DIAGNOSTICS: &str = "workspace.diagnostics";
pub const BOOTSTRAP: &str = "workspace.bootstrap";
pub const PREVIEW: &str = "workspace.preview";

pub const PROBES: &[&str] = &[
    "workspace-integrity",
    "runtime-health",
    "port-connectivity",
    "module-catalog-integrity",
];

const CAPTURE_PROFILES: &[&str] = &["runtime", "workspace", "full"];
/// Output bound for the git children of a direct diagnostics probe.
const PROBE_OUTPUT_LIMIT: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// Typed inputs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ChangesInput {
    review_mode: Option<String>,
    review_head: Option<String>,
    review_path: Option<String>,
    #[serde(default)]
    repo_id: Option<uuid::Uuid>,
    /// Commit message for the commit operation.
    #[serde(default)]
    message: Option<String>,
    /// Literal repository-relative paths for a scoped commit.
    #[serde(default)]
    paths: Option<Vec<String>>,
    /// Blob IDs returned by fs.write for files that this commit must contain.
    #[serde(default)]
    expected_blobs: Option<BTreeMap<String, String>>,
    /// `update`: where a replayed commit collides with what the base already
    /// holds, keep the base's side of those paths and go on, instead of
    /// stopping. Set by the control plane for a review copy only: its
    /// rounds reach the source as the owner's decisions, which stand. An
    /// ordinary workspace's update refuses a collision as before.
    #[serde(default)]
    favor_base: bool,
    /// `push`: the internal head the push must replace. `rebase` and
    /// `adopt`: the captured G5 head, which is the result's only lease.
    #[serde(default)]
    expected_head: Option<String>,
    /// The exact local commit to publish, even if another operation moves HEAD.
    #[serde(default)]
    push_head: Option<String>,
    /// `rebase`: the target workspace branch and its captured head.
    #[serde(default)]
    onto_branch: Option<String>,
    #[serde(default)]
    onto_head: Option<String>,
    /// `adopt`: the divergence run whose pinned remote input is
    /// `provider_tip`, the reviewed provider commit the branch is reset to.
    #[serde(default)]
    run_id: Option<uuid::Uuid>,
    #[serde(default)]
    provider_tip: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeInput {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureInput {
    #[serde(default)]
    profile: Option<String>,
}

/// Validated here; the fields themselves are consumed by `g5d bootstrap`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapInput {
    #[serde(default)]
    #[allow(dead_code)]
    repo_id: Option<uuid::Uuid>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    step_timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    repo_id: uuid::Uuid,
    #[serde(default)]
    profile: Option<String>,
    /// The isolated origin the browser reaches the preview from, sent only
    /// to catalogs from `2026.10.26-workspace-preview-origin` on
    /// (`g5.workspace.preview-origin-security`).
    #[serde(default)]
    origin: Option<String>,
}

/// A bare `https://host` origin, as the API derives for a preview task:
/// no userinfo, port, path, query or fragment, in canonical ASCII form.
pub(crate) fn is_preview_origin(origin: &str) -> bool {
    url::Url::parse(origin).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.origin().ascii_serialization() == origin
    })
}

fn invalid(reason: String) -> RpcError {
    RpcError {
        code: -32602,
        name: "INVALID_INPUT",
        data: json!({"reason":reason}),
    }
}

/// Strict typed validation of a built-in operation's input, before any slot
/// is reserved or process spawned. Unknown fields, malformed identifiers and
/// undeclared values are refused; omitted optional fields are fine.
pub fn validate_input(module_id: &str, operation: &str, input: &Value) -> Result<(), RpcError> {
    match (module_id, operation) {
        (
            CHANGES,
            "inspect" | "get" | "push" | "refresh" | "update" | "switch_back" | "commit" | "rebase"
            | "adopt",
        ) => {
            let input = serde_json::from_value::<ChangesInput>(input.clone())
                .map_err(|e| invalid(e.to_string()))?;
            if operation == "inspect" {
                if input.repo_id.is_none()
                    || !input.review_mode.as_deref().is_some_and(|m| {
                        matches!(
                            m,
                            "all" | "uncommitted" | "unpushed" | "source_state" | "source_diff"
                        )
                    })
                    || !input.review_head.as_deref().is_some_and(valid_oid)
                {
                    return Err(invalid(
                        "inspect requires a member, named mode and pushed OID".into(),
                    ));
                }
                if input.review_path.as_deref().is_some_and(|p| {
                    p.is_empty()
                        || p.starts_with('/')
                        || p.contains('\0')
                        || p.split('/').any(|s| s == ".." || s == ".")
                }) {
                    return Err(invalid("inspect requires a safe file path".into()));
                }
            } else if input.review_mode.is_some()
                || input.review_head.is_some()
                || input.review_path.is_some()
            {
                return Err(invalid("review inputs require inspect".into()));
            }
            if operation == "rebase" {
                let exact = |oid: &Option<String>| oid.as_deref().is_some_and(valid_oid);
                if input.repo_id.is_none()
                    || !exact(&input.expected_head)
                    || !exact(&input.onto_head)
                    || !input.onto_branch.as_deref().is_some_and(valid_branch_name)
                {
                    return Err(invalid(
                        "rebase requires one repository, a target branch and exact source and target OIDs".into(),
                    ));
                }
            } else if input.onto_branch.is_some() || input.onto_head.is_some() {
                return Err(invalid("onto_branch and onto_head require rebase".into()));
            }
            if operation == "adopt" {
                if input.repo_id.is_none()
                    || input.run_id.is_none()
                    || !input.expected_head.as_deref().is_some_and(valid_oid)
                    || !input.provider_tip.as_deref().is_some_and(valid_oid)
                {
                    return Err(invalid(
                        "adopt requires one repository, a run and exact G5 and provider OIDs"
                            .into(),
                    ));
                }
            } else if input.run_id.is_some() || input.provider_tip.is_some() {
                return Err(invalid("run_id and provider_tip require adopt".into()));
            }
            if let Some(head) = input.expected_head {
                if !matches!(operation, "push" | "rebase" | "adopt")
                    || input.repo_id.is_none()
                    || !matches!(head.len(), 40 | 64)
                    || !head.bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err(invalid(
                        "expected_head requires push, rebase or adopt, one repository and an exact Git OID".into(),
                    ));
                }
            }
            if let Some(paths) = input.paths {
                if operation != "commit"
                    || input.repo_id.is_none()
                    || paths.is_empty()
                    || paths.iter().any(|path| {
                        path.is_empty()
                            || path.starts_with('/')
                            || path.split('/').any(|part| part == "." || part == "..")
                            || path.contains('\0')
                    })
                {
                    return Err(invalid(
                        "paths requires commit, one repository and safe relative paths".into(),
                    ));
                }
                if let Some(blobs) = &input.expected_blobs {
                    if blobs.keys().any(|path| !paths.contains(path))
                        || blobs.values().any(|oid| !valid_oid(oid))
                    {
                        return Err(invalid(
                            "expected_blobs must name selected paths and exact Git OIDs".into(),
                        ));
                    }
                }
            } else if input.expected_blobs.is_some() {
                return Err(invalid(
                    "expected_blobs requires scoped commit paths".into(),
                ));
            }
            if let Some(head) = input.push_head {
                if operation != "push" || input.repo_id.is_none() || !valid_oid(&head) {
                    return Err(invalid(
                        "push_head requires push, one repository and an exact Git OID".into(),
                    ));
                }
            }
            Ok(())
        }
        (DIAGNOSTICS, "probe") => {
            let probe: ProbeInput =
                serde_json::from_value(input.clone()).map_err(|e| invalid(e.to_string()))?;
            if !PROBES.contains(&probe.name.as_str()) {
                return Err(RpcError {
                    code: -32602,
                    name: "UNKNOWN_PROBE",
                    data: json!({"name":probe.name,"known":PROBES}),
                });
            }
            Ok(())
        }
        (DIAGNOSTICS, "capture") => {
            let capture: CaptureInput =
                serde_json::from_value(input.clone()).map_err(|e| invalid(e.to_string()))?;
            if let Some(profile) = capture.profile.as_deref() {
                if !CAPTURE_PROFILES.contains(&profile) {
                    return Err(RpcError {
                        code: -32602,
                        name: "UNKNOWN_PROFILE",
                        data: json!({"profile":profile,"known":CAPTURE_PROFILES}),
                    });
                }
            }
            Ok(())
        }
        (BOOTSTRAP, "run") => {
            let bootstrap: BootstrapInput =
                serde_json::from_value(input.clone()).map_err(|e| invalid(e.to_string()))?;
            if bootstrap.profile.as_deref().is_some_and(|p| p != "setup") {
                return Err(RpcError {
                    code: -32602,
                    name: "UNKNOWN_PROFILE",
                    data: json!({"profile":bootstrap.profile,"known":["setup"]}),
                });
            }
            if bootstrap.step_timeout_ms == Some(0) {
                return Err(invalid("step_timeout_ms must be positive".into()));
            }
            Ok(())
        }
        (PREVIEW, "start") => {
            let preview: PreviewInput = serde_json::from_value(input.clone())
                .map_err(|error| invalid(error.to_string()))?;
            if preview.repo_id.is_nil() {
                return Err(invalid("repo_id must be non-nil".to_string()));
            }
            if let Some(profile) = preview.profile.as_deref() {
                // The API applies the same rule before it starts a preview.
                g5_contracts::types::validate_preview_profile_name(profile).map_err(invalid)?;
            }
            if preview
                .origin
                .as_deref()
                .is_some_and(|origin| !is_preview_origin(origin))
            {
                return Err(invalid("origin must be a bare https origin".to_string()));
            }
            Ok(())
        }
        (module, operation) => Err(RpcError {
            code: 404,
            name: "OPERATION_NOT_FOUND",
            data: json!({"module_id":module,"operation":operation}),
        }),
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|c| c.is_ascii_hexdigit())
}

/// Why a supervised operation stopped before producing its result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    TimedOut,
    Native(NativeGitError),
}

enum GitFailure {
    Stopped(Stop),
    Failed(String),
}

/// Supervision shared by every git child of one built-in execution.
#[derive(Clone)]
struct Ctx<'a> {
    rt: &'a ModuleRuntime,
    cancel: &'a CancellationToken,
    deadline: Option<Instant>,
    /// Most bytes one git child may produce on either pipe.
    output_limit: u64,
    native_cancel: NativeCancellation,
    git_guard: Option<Arc<tokio::sync::OwnedMutexGuard<()>>>,
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        None => std::future::pending().await,
    }
}

pub(super) fn stopped(stop: Stop) -> Value {
    match stop {
        Stop::Cancelled => outcome("cancelled", json!({})),
        Stop::TimedOut => outcome("timed_out", json!({})),
        Stop::Native(error) => outcome(
            if error.kind == FailureKind::Deadline {
                "timed_out"
            } else {
                "cancelled"
            },
            json!({"git":error.to_string()}),
        ),
    }
}

fn git_failure(error: NativeGitError) -> GitFailure {
    if matches!(error.kind, FailureKind::Cancelled | FailureKind::Deadline) {
        GitFailure::Stopped(Stop::Native(error))
    } else {
        GitFailure::Failed(error.to_string())
    }
}

impl Ctx<'_> {
    fn git_ok<T>(&self, result: Result<T, GitFailure>) -> Result<Option<T>, Stop> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(GitFailure::Failed(_)) => Ok(None),
            Err(GitFailure::Stopped(stop)) => Err(stop),
        }
    }
    fn failure(&self, mut error: NativeGitError) -> GitFailure {
        let secrets = self.rt.secrets();
        error.detail = secrets.redact_diagnostic(&error.detail);
        error.target = error.target.map(|target| secrets.redact_str(&target));
        if error.kind == FailureKind::Cancelled
            && self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            error.kind = FailureKind::Deadline;
        }
        git_failure(error)
    }
    async fn operate<T, F, Fut>(&self, work: F) -> Result<T, GitFailure>
    where
        T: Send + 'static,
        F: FnOnce(MemberGit) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
    {
        let native = self.native_cancel.clone();
        let git = self
            .rt
            .git()
            .member_git(
                self.deadline,
                self.output_limit.try_into().unwrap_or(usize::MAX),
                native.clone(),
                self.rt.config.module_uid,
                self.rt.config.module_gid,
            )
            .map_err(git_failure)?;
        let cancel = self.cancel.clone();
        let deadline = self.deadline;
        let (keep_alive, mut caller_dropped) = tokio::sync::oneshot::channel::<()>();
        // Own the supplied typed operation and any captured transport lease
        // until SCM proves cleanup, even if this awaiting adapter disappears.
        let admission = self.git_guard.clone();
        let task = tokio::spawn(async move {
            let _admission = admission;
            let future = work(git);
            tokio::pin!(future);
            let result = tokio::select! {
                result = &mut future => result,
                _ = cancel.cancelled() => { native.cancel(); future.await },
                _ = sleep_until(deadline) => { native.cancel(); future.await },
                _ = &mut caller_dropped => { native.cancel(); future.await },
            };
            native.wait_idle().await;
            result
        });
        let result = task.await.map_err(|_| {
            GitFailure::Failed(
                "Git operation owner stopped unexpectedly; inspect current state before retrying"
                    .into(),
            )
        });
        drop(keep_alive);
        result
    }

    async fn native<T, F, Fut>(&self, work: F) -> Result<T, GitFailure>
    where
        T: Send + 'static,
        F: FnOnce(MemberGit) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, NativeGitError>> + Send + 'static,
    {
        self.operate(work)
            .await?
            .map_err(|error| self.failure(error))
    }

    async fn head(&self, root: &Path) -> Result<String, GitFailure> {
        let root = root.to_path_buf();
        self.native(move |git| async move { git.head_commit(&root).await })
            .await
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

pub async fn run(
    rt: Arc<ModuleRuntime>,
    exec_id: &ExecId,
    module: &ModuleEntry,
    operation: &str,
    input: Value,
    cancel: CancellationToken,
) -> Value {
    let deadline = module
        .limits
        .wall_clock_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    // The catalog requires an output limit of every module but `agent.turn`,
    // which is never a built-in.
    let output_limit = module.limits.max_output_bytes.unwrap_or(u64::MAX);
    let ctx = Ctx {
        rt: &rt,
        cancel: &cancel,
        deadline,
        output_limit,
        native_cancel: NativeCancellation::default(),
        git_guard: None,
    };
    let result = match module.module_id.as_str() {
        CHANGES => changes(&ctx, exec_id, operation, &input).await,
        DIAGNOSTICS => diagnostics(&ctx, operation, &input).await,
        other => Ok(outcome(
            "failed",
            json!({"reason":"no_builtin_handler","module_id":other}),
        )),
    };
    let mut value = match result {
        Ok(value) => value,
        Err(stop) => {
            let timed_out = stop == Stop::TimedOut
                || matches!(&stop, Stop::Native(error) if error.kind == FailureKind::Deadline);
            let mut value = stopped(stop);
            if timed_out {
                value["detail"]["wall_clock_ms"] = json!(module.limits.wall_clock_ms);
            }
            value
        }
    };
    // The declared output bound applies to in-process results too.
    let size = serde_json::to_vec(&value)
        .map(|b| b.len() as u64)
        .unwrap_or(0);
    if size > output_limit {
        value = outcome(
            "limit_exceeded",
            json!({"limit":"max_output_bytes","value":output_limit}),
        );
    }
    value
}

// ---------------------------------------------------------------------------
// workspace.changes
// ---------------------------------------------------------------------------

/// Git's own branch-name rules (`git check-ref-format --branch`): no empty
/// name or component, no component starting with `.` or ending in `.lock`,
/// no `..`, `@{`, `//`, no ASCII control character, space, `~`, `^`, `:`,
/// `?`, `*`, `[` or `\`, no leading or trailing `/`, no trailing `.`, not
/// `@`; plus the argument-safety rule that a name never starts with `-`.
/// Every name Git accepts for a branch is accepted here — `feature+test`
/// included — because Changes operates on the authoritative shared branch
/// exactly as named.
fn valid_branch_name(name: &str) -> bool {
    if name.is_empty()
        || name == "@"
        || name.starts_with('-')
        || name.starts_with('/')
        || name.ends_with('/')
        || name.ends_with('.')
        || name.contains("..")
        || name.contains("//")
        || name.contains("@{")
    {
        return false;
    }
    if name.chars().any(|c| {
        c.is_ascii_control() || c == ' ' || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return false;
    }
    name.split('/').all(|component| {
        !component.is_empty() && !component.starts_with('.') && !component.ends_with(".lock")
    })
}

fn members(rt: &ModuleRuntime, selected: Option<uuid::Uuid>) -> Vec<(RepositoryLayout, PathBuf)> {
    rt.config
        .layout
        .repos
        .iter()
        .filter(|repo| selected.is_none_or(|id| repo.repo_id == id))
        .map(|repo| {
            (
                repo.clone(),
                rt.config.paths.workspace_root.join(&repo.root_path),
            )
        })
        .collect()
}

/// The association: the checkout must sit on exactly the shared workspace
/// branch, and that branch must already exist as an internal ref. Nothing
/// here may mint one. Detached and foreign heads and interrupted sequencer
/// state are reported, never silently continued.
async fn association(ctx: &Ctx<'_>, repo: &RepositoryLayout, root: &Path) -> Result<Value, Stop> {
    let shared = &repo.branch;
    let owned_root = root.to_path_buf();
    let head = ctx
        .git_ok(
            ctx.native(move |git| async move { git.current_branch(&owned_root).await })
                .await,
        )?
        .flatten();
    let head_oid = ctx.git_ok(ctx.head(root).await)?;
    let owned_root = root.to_path_buf();
    let branch = shared.clone();
    let tracking = ctx
        .git_ok(
            ctx.native(move |git| async move { git.tracking_tip(&owned_root, &branch).await })
                .await,
        )?
        .flatten();
    let on_shared = head.as_deref() == Some(shared.as_str());
    let owned_root = root.to_path_buf();
    let sequencer = ctx.git_ok(
        ctx.native(move |git| async move { git.sequencers(&owned_root).await })
            .await,
    )?;
    let sequencer_available = sequencer.is_some();
    let sequencer = sequencer.unwrap_or_default();
    let mut errors = Vec::new();
    if head_oid.is_none() {
        errors.push("HEAD_UNAVAILABLE");
    }
    if !sequencer_available {
        errors.push("SEQUENCER_UNAVAILABLE");
    }
    if !valid_branch_name(shared) {
        errors.push("INVALID_BRANCH_NAME");
    }
    // The member's own upstream target reaches git argv in refspecs too.
    if repo
        .upstream_branch
        .as_deref()
        .is_some_and(|upstream| !valid_branch_name(upstream))
    {
        errors.push("INVALID_UPSTREAM_BRANCH");
    }
    if head.is_none() {
        errors.push("DETACHED_HEAD");
    } else if !on_shared {
        errors.push("BRANCH_ASSOCIATION_MISMATCH");
    }
    if tracking.is_none() {
        errors.push("MISSING_INTERNAL_BRANCH");
    }
    if !sequencer.is_empty() {
        errors.push("INTERRUPTED_SEQUENCER");
    }
    Ok(json!({
        "repo_id": repo.repo_id,
        "root_path": repo.root_path,
        "shared_branch": shared,
        "upstream_branch": repo.upstream_branch,
        "head": head,
        "head_oid": head_oid,
        "detached_head": head.is_none(),
        "foreign_head": head.is_some() && !on_shared,
        "sequencer": sequencer,
        "on_shared_branch": on_shared,
        "internal_ref_present": tracking.is_some(),
        "internal_ref": tracking,
        "errors": errors,
    }))
}

/// The dirty set, or `None` when `git status` could not be collected (for
/// example its output exceeded the module's bound): an unknown tree is never
/// reported as clean.
async fn dirty_set(ctx: &Ctx<'_>, root: &Path) -> Result<Option<Vec<Value>>, Stop> {
    let root = root.to_path_buf();
    match ctx
        .native(move |git| async move { git.status(&root).await })
        .await
    {
        Ok(entries) => Ok(Some(
            entries
                .into_iter()
                .map(|entry| serde_json::to_value(entry).expect("status value"))
                .collect(),
        )),
        Err(GitFailure::Failed(_)) => Ok(None),
        Err(GitFailure::Stopped(stop)) => Err(stop),
    }
}

/// `git status --porcelain=v1 -z`: entries end in NUL and paths are literal
/// (never C-quoted, whatever characters they hold), so a consumer can use
/// them as file paths. A rename or copy names the new path in its entry and
/// the original in the following NUL-terminated field, reported as `from`.
async fn ahead_behind(ctx: &Ctx<'_>, root: &Path, upstream: &str) -> Result<Value, Stop> {
    let root = root.to_path_buf();
    let branch = upstream
        .strip_prefix("refs/remotes/origin/")
        .expect("tracking ref")
        .to_owned();
    Ok(
        match ctx.git_ok(
            ctx.native(move |git| async move { git.divergence(&root, &branch).await })
                .await,
        )? {
            Some(value) => json!(value),
            None => json!({"ahead":null,"behind":null}),
        },
    )
}

fn errors_of(report: &Value) -> Vec<Value> {
    report["errors"].as_array().cloned().unwrap_or_default()
}

async fn changes(
    ctx: &Ctx<'_>,
    exec_id: &ExecId,
    operation: &str,
    input: &Value,
) -> Result<Value, Stop> {
    let rt = ctx.rt;
    let _git_guard = if operation == "refresh" {
        None
    } else {
        Some(tokio::select! {
            guard = rt.git_operations.clone().lock_owned() => Arc::new(guard),
            _ = ctx.cancel.cancelled() => return Err(Stop::Cancelled),
            _ = sleep_until(ctx.deadline) => return Err(Stop::TimedOut),
        })
    };
    let owned_ctx = Ctx {
        git_guard: _git_guard,
        ..ctx.clone()
    };
    let ctx = &owned_ctx;
    // Already validated before admission; a second strict parse keeps the
    // typed shape authoritative here too.
    let parsed: ChangesInput = serde_json::from_value(input.clone()).expect("validated input");
    let input = &parsed;
    let selected = parsed.repo_id;
    let favor_base = parsed.favor_base;
    let members = members(rt, selected);
    if selected.is_some() && members.is_empty() {
        return Ok(outcome(
            "failed",
            json!({"reason":"UNKNOWN_MEMBER","repo_id":selected}),
        ));
    }
    match operation {
        "inspect" => {
            let (repo, root) = members.first().expect("validated repository");
            let report = association(ctx, repo, root).await?;
            if !errors_of(&report).is_empty() {
                return Ok(outcome("failed", report));
            }
            let root = root.to_path_buf();
            let mode = input.review_mode.clone().expect("validated mode");
            let head = input.review_head.clone().expect("validated head");
            let path = input.review_path.clone();
            let result = ctx.git_ok(
                ctx.native(move |git| async move {
                    git.review_inspect(&root, &mode, &head, path.as_deref())
                        .await
                })
                .await,
            )?;
            match result {
                Some(result) => Ok(json!({"status":"succeeded","result":result})),
                None => Ok(outcome("failed", json!({"reason":"CONTENT_UNAVAILABLE"}))),
            }
        }
        "refresh" => refresh_provider_refs(rt, selected, ctx.cancel, ctx.deadline).await,
        "get" => {
            let mut reports = Vec::new();
            for (repo, root) in &members {
                let mut report = association(ctx, repo, root).await?;
                match dirty_set(ctx, root).await? {
                    Some(dirty) => report["dirty"] = json!(dirty),
                    None => {
                        report["dirty"] = Value::Null;
                        if let Some(errors) = report["errors"].as_array_mut() {
                            errors.push(json!("STATUS_UNAVAILABLE"));
                        }
                    }
                }
                report["against_internal"] =
                    ahead_behind(ctx, root, &format!("refs/remotes/origin/{}", repo.branch))
                        .await?;
                report["against_upstream"] = match repo.upstream_branch.as_deref() {
                    Some(upstream) => {
                        ahead_behind(ctx, root, &format!("refs/remotes/origin/{upstream}")).await?
                    }
                    None => json!({"ahead":null,"behind":null}),
                };
                let root = root.to_path_buf();
                report["conflicts"] = json!(ctx
                    .git_ok(
                        ctx.native(move |git| async move { git.conflicts(&root, None).await })
                            .await
                    )?
                    .unwrap_or_default());
                reports.push(report);
            }
            Ok(json!({"status":"succeeded","result":{"members":reports}}))
        }
        "push" => {
            let mut results = Vec::new();
            for (repo, root) in &members {
                let report = association(ctx, repo, root).await?;
                if !errors_of(&report).is_empty() {
                    results.push(
                        json!({"repo_id":repo.repo_id,"pushed":false,"errors":report["errors"]}),
                    );
                    continue;
                }
                let expected = report["internal_ref"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if input
                    .expected_head
                    .as_ref()
                    .is_some_and(|head| head != &expected)
                {
                    results.push(json!({"repo_id":repo.repo_id,"pushed":false,"errors":["INTERNAL_BRANCH_MOVED"]}));
                    continue;
                }
                // Capture a commit once. A concurrent local commit must not
                // change what this operation publishes or reports as saved.
                let Some(head) = ctx.git_ok(ctx.head(root).await)? else {
                    results.push(json!({"repo_id":repo.repo_id,"pushed":false,"errors":["HEAD_UNAVAILABLE"]}));
                    continue;
                };
                let head = input.push_head.as_deref().unwrap_or(&head);
                let Some(transport) = rt.transport() else {
                    results.push(json!({"repo_id":repo.repo_id,"pushed":false,"errors":["TRANSPORT_UNAVAILABLE"]}));
                    continue;
                };
                let lease =
                    transport.observed_lease(&repo.repo_id.to_string(), repo.transport_url());
                let _ = rt.emit(
                    exec_id,
                    "module.progress",
                    json!({"repo_id":repo.repo_id,"step":"push"}),
                    false,
                );
                let request = MemberPush {
                    repository: root.clone(),
                    transport_url: lease.url().into(),
                    branch: repo.branch.clone(),
                    commit: head.to_owned(),
                    expected,
                };
                match ctx
                    .native(move |git| async move {
                        let _lease = lease;
                        git.push_commit(&request).await
                    })
                    .await
                {
                    Ok(()) => {
                        results.push(json!({"repo_id":repo.repo_id,"pushed":true,"head":head,"saved_to_g5":true,"provider_sync":"managed_by_g5"}));
                    }
                    Err(GitFailure::Failed(detail)) => {
                        let error = if detail.contains("RECONCILIATION_REQUIRED") {
                            "RECONCILIATION_REQUIRED"
                        } else if detail.contains("stale info")
                            || detail.contains("non-fast-forward")
                        {
                            "INTERNAL_BRANCH_MOVED"
                        } else {
                            "PUSH_FAILED"
                        };
                        results.push(json!({"repo_id":repo.repo_id,"pushed":false,"errors":[error],"detail":detail}));
                    }
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                }
            }
            let all = results.iter().all(|r| r["pushed"] == true);
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        "update" => {
            let mut results = Vec::new();
            for (repo, root) in &members {
                let report = association(ctx, repo, root).await?;
                if !errors_of(&report).is_empty() {
                    results.push(
                        json!({"repo_id":repo.repo_id,"updated":false,"errors":report["errors"]}),
                    );
                    continue;
                }
                match dirty_set(ctx, root).await? {
                    Some(dirty) if dirty.is_empty() => {}
                    Some(_) => {
                        results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["DIRTY_WORKING_TREE"]}));
                        continue;
                    }
                    None => {
                        results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["STATUS_UNAVAILABLE"]}));
                        continue;
                    }
                }
                let Some(transport) = rt.transport() else {
                    results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["TRANSPORT_UNAVAILABLE"]}));
                    continue;
                };
                let Some(upstream) = repo.upstream_branch.as_deref() else {
                    results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["PARENT_UNAVAILABLE"]}));
                    continue;
                };
                let lease =
                    transport.observed_lease(&repo.repo_id.to_string(), repo.transport_url());
                let _ = rt.emit(
                    exec_id,
                    "module.progress",
                    json!({"repo_id":repo.repo_id,"step":"fetch"}),
                    false,
                );
                let (Some(head), Some(published)) =
                    (report["head_oid"].as_str(), report["internal_ref"].as_str())
                else {
                    results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["ASSOCIATION_UNAVAILABLE"]}));
                    continue;
                };
                let request = MemberParentUpdate {
                    branch: repo.branch.clone(),
                    expected_head: head.into(),
                    repository: root.clone(),
                    transport_url: lease.url().into(),
                    parent_branch: upstream.into(),
                    published_head: published.into(),
                    favor_parent: favor_base,
                    private_state: rt.config.paths.state_dir.clone(),
                };
                match ctx.operate(move |git| async move { let _lease = lease; git.update_parent(&request).await }).await {
                    Ok(Ok(resolved)) => {
                        let mut result = json!({"repo_id":repo.repo_id,"updated":true,"upstream_branch":repo.upstream_branch});
                        if !resolved.is_empty() {
                            result["resolved"] = json!(resolved);
                            result["resolved_note"] = json!(format!("{upstream} version kept where your commit collided"));
                        }
                        results.push(result);
                    }
                    Ok(Err(failure)) => match ctx.failure(failure.error) {
                        GitFailure::Stopped(stop) => return Err(stop),
                        GitFailure::Failed(detail) => results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["UPDATE_FAILED"],"detail":detail,"conflicts":failure.conflicts})),
                    },
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                    Err(GitFailure::Failed(detail)) => results.push(json!({"repo_id":repo.repo_id,"updated":false,"errors":["UPDATE_FAILED"],"detail":detail})),
                }
            }
            let all = results.iter().all(|r| r["updated"] == true);
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        "rebase" => {
            // `g5.architecture.workspace_rebase`: one member, exact inputs.
            // The checkout must be on its branch, clean and free of a
            // sequencer; every refusal leaves G5 and local work unchanged.
            let mut results = Vec::new();
            for (repo, root) in &members {
                let report = association(ctx, repo, root).await?;
                if !errors_of(&report).is_empty() {
                    results.push(
                        json!({"repo_id":repo.repo_id,"rebased":false,"errors":report["errors"]}),
                    );
                    continue;
                }
                match dirty_set(ctx, root).await? {
                    Some(dirty) if dirty.is_empty() => {}
                    Some(dirty) => {
                        results.push(json!({"repo_id":repo.repo_id,"rebased":false,"errors":["DIRTY_WORKING_TREE"],"dirty":dirty}));
                        continue;
                    }
                    None => {
                        results.push(json!({"repo_id":repo.repo_id,"rebased":false,"errors":["STATUS_UNAVAILABLE"]}));
                        continue;
                    }
                }
                let Some(transport) = rt.transport() else {
                    results.push(json!({"repo_id":repo.repo_id,"rebased":false,"errors":["TRANSPORT_UNAVAILABLE"]}));
                    continue;
                };
                let lease =
                    transport.observed_lease(&repo.repo_id.to_string(), repo.transport_url());
                let _ = rt.emit(
                    exec_id,
                    "module.progress",
                    json!({"repo_id":repo.repo_id,"step":"rebase"}),
                    false,
                );
                let request = MemberRebase {
                    repository: root.clone(),
                    transport_url: lease.url().into(),
                    branch: repo.branch.clone(),
                    expected_head: parsed.expected_head.clone().expect("validated"),
                    onto_branch: parsed.onto_branch.clone().expect("validated"),
                    onto_head: parsed.onto_head.clone().expect("validated"),
                };
                let expected = request.expected_head.clone();
                match ctx
                    .operate(move |git| async move {
                        let _lease = lease;
                        git.rebase_onto(&request).await
                    })
                    .await
                {
                    Ok(Ok(done)) => results.push(json!({"repo_id":repo.repo_id,"rebased":true,"head":done.head,"previous_head":expected,"published":done.published,"saved_to_g5":true})),
                    Ok(Err(failure)) => {
                        let (code, extra) = match failure {
                            MemberRebaseFailure::InputMoved(code) => (code, json!({})),
                            MemberRebaseFailure::Diverged { local, conflicts } => (
                                "LOCAL_HISTORY_DIVERGED",
                                json!({"local_head":local,"conflicts":conflicts}),
                            ),
                            MemberRebaseFailure::Conflicts(paths) => {
                                ("REBASE_CONFLICT", json!({"conflicts":paths}))
                            }
                            MemberRebaseFailure::Unpublished { head, error } => {
                                match ctx.failure(error) {
                                    GitFailure::Stopped(stop) => return Err(stop),
                                    GitFailure::Failed(detail) => {
                                        let code = if detail.contains("stale info")
                                            || detail.contains("Force-with-lease rejected")
                                        {
                                            "INTERNAL_BRANCH_MOVED"
                                        } else {
                                            "PUSH_FAILED"
                                        };
                                        (code, json!({"local_head":head,"detail":detail}))
                                    }
                                }
                            }
                            MemberRebaseFailure::Failed(error) => match ctx.failure(error) {
                                GitFailure::Stopped(stop) => return Err(stop),
                                GitFailure::Failed(detail) => {
                                    ("REBASE_FAILED", json!({"detail":detail}))
                                }
                            },
                        };
                        let mut result =
                            json!({"repo_id":repo.repo_id,"rebased":false,"errors":[code]});
                        if let (Some(result), Some(extra)) =
                            (result.as_object_mut(), extra.as_object())
                        {
                            result.extend(extra.clone());
                        }
                        results.push(result);
                    }
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                    Err(GitFailure::Failed(detail)) => results.push(json!({"repo_id":repo.repo_id,"rebased":false,"errors":["REBASE_FAILED"],"detail":detail})),
                }
            }
            let all = results.iter().all(|r| r["rebased"] == true);
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        "adopt" => {
            // `g5.workspace.remote.adopt-provider-history`: one member, exact
            // inputs. The checkout must be on its branch, clean, free of a
            // sequencer and hold nothing G5 lacks; every refusal leaves G5
            // and local work unchanged.
            let mut results = Vec::new();
            for (repo, root) in &members {
                let report = association(ctx, repo, root).await?;
                if !errors_of(&report).is_empty() {
                    results.push(
                        json!({"repo_id":repo.repo_id,"adopted":false,"errors":report["errors"]}),
                    );
                    continue;
                }
                match dirty_set(ctx, root).await? {
                    Some(dirty) if dirty.is_empty() => {}
                    Some(dirty) => {
                        results.push(json!({"repo_id":repo.repo_id,"adopted":false,"errors":["DIRTY_WORKING_TREE"],"dirty":dirty}));
                        continue;
                    }
                    None => {
                        results.push(json!({"repo_id":repo.repo_id,"adopted":false,"errors":["STATUS_UNAVAILABLE"]}));
                        continue;
                    }
                }
                let Some(transport) = rt.transport() else {
                    results.push(json!({"repo_id":repo.repo_id,"adopted":false,"errors":["TRANSPORT_UNAVAILABLE"]}));
                    continue;
                };
                let lease =
                    transport.observed_lease(&repo.repo_id.to_string(), repo.transport_url());
                let _ = rt.emit(
                    exec_id,
                    "module.progress",
                    json!({"repo_id":repo.repo_id,"step":"adopt"}),
                    false,
                );
                let request = MemberAdopt {
                    repository: root.clone(),
                    transport_url: lease.url().into(),
                    workspace_id: rt.config.workspace_id,
                    branch: repo.branch.clone(),
                    run_id: parsed.run_id.expect("validated"),
                    expected_head: parsed.expected_head.clone().expect("validated"),
                    provider_tip: parsed.provider_tip.clone().expect("validated"),
                };
                let expected = request.expected_head.clone();
                match ctx
                    .operate(move |git| async move {
                        let _lease = lease;
                        git.adopt_provider_tip(&request).await
                    })
                    .await
                {
                    Ok(Ok(done)) => results.push(json!({"repo_id":repo.repo_id,"adopted":true,"head":done.head,"previous_head":expected,"published":done.published,"saved_to_g5":true})),
                    Ok(Err(failure)) => {
                        let (code, extra) = match failure {
                            MemberAdoptFailure::InputMoved(code) => (code, json!({})),
                            MemberAdoptFailure::UnpushedCommits { local } => {
                                ("UNPUSHED_COMMITS", json!({"local_head":local}))
                            }
                            MemberAdoptFailure::Unpublished(error) => match ctx.failure(error) {
                                GitFailure::Stopped(stop) => return Err(stop),
                                GitFailure::Failed(detail) => {
                                    let code = if detail.contains("stale info")
                                        || detail.contains("Force-with-lease rejected")
                                    {
                                        "INTERNAL_BRANCH_MOVED"
                                    } else {
                                        "PUSH_FAILED"
                                    };
                                    (code, json!({"detail":detail}))
                                }
                            },
                            MemberAdoptFailure::Failed(error) => match ctx.failure(error) {
                                GitFailure::Stopped(stop) => return Err(stop),
                                GitFailure::Failed(detail) => {
                                    ("ADOPT_FAILED", json!({"detail":detail}))
                                }
                            },
                        };
                        let mut result =
                            json!({"repo_id":repo.repo_id,"adopted":false,"errors":[code]});
                        if let (Some(result), Some(extra)) =
                            (result.as_object_mut(), extra.as_object())
                        {
                            result.extend(extra.clone());
                        }
                        results.push(result);
                    }
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                    Err(GitFailure::Failed(detail)) => results.push(json!({"repo_id":repo.repo_id,"adopted":false,"errors":["ADOPT_FAILED"],"detail":detail})),
                }
            }
            let all = results.iter().all(|r| r["adopted"] == true);
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        "commit" => {
            // Everything in the working tree becomes one commit on the shared
            // branch (untracked files included, hooks skipped): Push review
            // records the reviewer's current state as a round without any
            // git step on their part. A clean member commits nothing.
            let message = parsed
                .message
                .as_deref()
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or("Review round");
            let mut results = Vec::new();
            for (repo, root) in &members {
                let report = association(ctx, repo, root).await?;
                if !errors_of(&report).is_empty() {
                    results.push(
                        json!({"repo_id":repo.repo_id,"committed":false,"errors":report["errors"]}),
                    );
                    continue;
                }
                let owned_root = root.clone();
                let paths = parsed.paths.clone();
                match ctx.native(move |git| async move { git.conflicts(&owned_root, paths.as_deref()).await }).await {
                    Ok(paths) if !paths.is_empty() => {
                        results.push(json!({"repo_id":repo.repo_id,"committed":false,"errors":["UNMERGED_PATHS"],"paths":paths,"detail":"resolve the conflicted files first"}));
                        continue;
                    }
                    Ok(_) => {},
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                    Err(GitFailure::Failed(detail)) => {
                        results.push(json!({"repo_id":repo.repo_id,"committed":false,"errors":["STATUS_UNAVAILABLE"],"detail":detail}));
                        continue;
                    }
                }
                let _ = rt.emit(
                    exec_id,
                    "module.progress",
                    json!({"repo_id":repo.repo_id,"step":"commit"}),
                    false,
                );
                let request = MemberCheckpoint {
                    repository: root.clone(),
                    branch: repo.branch.clone(),
                    message: message.into(),
                    paths: parsed.paths.clone(),
                    expected_blobs: parsed.expected_blobs.clone(),
                };
                match ctx.native(move |git| async move { git.checkpoint(&request).await }).await {
                    Ok(value) => results.push(json!({"repo_id":repo.repo_id,"committed":value.committed,"head":value.head,"files":value.files})),
                    Err(GitFailure::Failed(detail)) => {
                        let code = if detail.contains("FILE_CHANGED_DURING_SAVE:") { "FILE_CHANGED_DURING_SAVE" } else if detail.contains("UNMERGED_PATHS:") { "UNMERGED_PATHS" } else { "COMMIT_FAILED" };
                        results.push(json!({"repo_id":repo.repo_id,"committed":false,"errors":[code],"detail":detail}));
                    }
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                }
            }
            let all = results.iter().all(|r| r["errors"].is_null());
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        "switch_back" => {
            let mut results = Vec::new();
            for (repo, root) in &members {
                // The shared name reaches `git checkout` argv: only a Git-valid,
                // non-option name does.
                if !valid_branch_name(&repo.branch) {
                    results.push(json!({"repo_id":repo.repo_id,"switched":false,"errors":["INVALID_BRANCH_NAME"]}));
                    continue;
                }
                let owned_root = root.clone();
                let branch = repo.branch.clone();
                match ctx.native(move |git| async move { git.switch_back(&owned_root, &branch).await }).await {
                    Ok(materialized) => results.push(json!({"repo_id":repo.repo_id,"switched":true,"materialized_local":materialized})),
                    Err(GitFailure::Failed(detail)) => {
                        let code = if detail.contains("MISSING_INTERNAL_BRANCH") { "MISSING_INTERNAL_BRANCH" } else { "SWITCH_FAILED" };
                        let mut failure = json!({"repo_id":repo.repo_id,"switched":false,"errors":[code]});
                        if code != "MISSING_INTERNAL_BRANCH" { failure["detail"] = json!(detail); }
                        results.push(failure)
                    }
                    Err(GitFailure::Stopped(stop)) => return Err(stop),
                }
            }
            let all = results.iter().all(|r| r["switched"] == true);
            Ok(json!({"status": if all {"succeeded"} else {"failed"},"result":{"members":results}}))
        }
        other => Ok(outcome(
            "failed",
            json!({"reason":"UNKNOWN_OPERATION","operation":other}),
        )),
    }
}

/// Deliver Git objects and tracking refs without touching a working branch,
/// index or dirty files. The platform invokes this periodically and before
/// launching an agent, so repository code never needs a network credential.
pub(crate) async fn refresh_provider_refs(
    rt: &ModuleRuntime,
    selected: Option<uuid::Uuid>,
    cancel: &CancellationToken,
    deadline: Option<Instant>,
) -> Result<Value, Stop> {
    let _guard = tokio::select! {
        guard = rt.git_operations.clone().lock_owned() => Arc::new(guard),
        _ = cancel.cancelled() => return Err(Stop::Cancelled),
        _ = sleep_until(deadline) => return Err(Stop::TimedOut),
    };
    let ctx = Ctx {
        rt,
        cancel,
        deadline,
        output_limit: PROBE_OUTPUT_LIMIT,
        native_cancel: NativeCancellation::default(),
        git_guard: Some(_guard),
    };
    let Some(transport) = rt.transport() else {
        return Ok(outcome("failed", json!({"reason":"TRANSPORT_UNAVAILABLE"})));
    };
    let selected_members = members(rt, selected);
    if selected.is_some() && selected_members.is_empty() {
        return Ok(outcome("failed", json!({"reason":"UNKNOWN_MEMBER"})));
    }
    let mut results = Vec::new();
    for (repo, root) in selected_members {
        let lease = transport.lease(&repo.repo_id.to_string(), repo.transport_url());
        let request = MemberTrackingRefresh {
            repository: root,
            transport_url: lease.url().into(),
            workspace_id: rt.config.workspace_id,
            branch: repo.branch,
            parent_branch: repo.upstream_branch,
            follow_branch: repo.follow_branch,
        };
        match ctx.native(move |git| async move { let _lease = lease; git.refresh_tracking(&request).await }).await {
            Ok(value) => results.push(json!({"repo_id":repo.repo_id,"refreshed":true,"parent_refreshed":value.parent_refreshed,"parent_error":value.parent_failure.map(|error| error.to_string()),"fast_forwarded":value.fast_forwarded})),
            Err(GitFailure::Failed(detail)) => results.push(json!({"repo_id":repo.repo_id,"refreshed":false,"detail":detail})),
            Err(GitFailure::Stopped(stop)) => return Err(stop),
        }
    }
    Ok(
        json!({"status":if results.iter().all(|r| r["refreshed"] == true) {"succeeded"} else {"failed"},
        "result":{"members":results}}),
    )
}

/// Agent memory folders in the owner's personal repository, one per provider
/// (`g5.personal.memory`). Each provider writes its own native memory tree.
pub(crate) const PERSONAL_MEMORY_DIRECTORIES: [&str; 2] = ["memory/claude", "memory/codex"];

/// Concurrent index additions keep both lines when a sync replays its memory
/// commits onto a moved personal branch.
const PERSONAL_MEMORY_ATTRIBUTES: &str =
    "memory/claude/MEMORY.md merge=union\nmemory/codex/MEMORY.md merge=union\n";

/// Commit and publish agent memory after a code push
/// (`g5.personal.memory-sync`). Best effort: the outcome is reported for the
/// log, and anything left behind waits for the next code push.
pub(crate) async fn sync_personal_memory(
    rt: &ModuleRuntime,
    cancel: &CancellationToken,
    deadline: Option<Instant>,
) -> Result<Value, Stop> {
    let Some(personal) = rt.config.layout.personal_repository() else {
        return Ok(json!({"status":"skipped","reason":"NO_PERSONAL_REPOSITORY"}));
    };
    let _guard = tokio::select! {
        guard = rt.git_operations.clone().lock_owned() => Arc::new(guard),
        _ = cancel.cancelled() => return Err(Stop::Cancelled),
        _ = sleep_until(deadline) => return Err(Stop::TimedOut),
    };
    let ctx = Ctx {
        rt,
        cancel,
        deadline,
        output_limit: PROBE_OUTPUT_LIMIT,
        native_cancel: NativeCancellation::default(),
        git_guard: Some(_guard),
    };
    let Some(transport) = rt.transport() else {
        return Ok(json!({"status":"failed","reason":"TRANSPORT_UNAVAILABLE"}));
    };
    let attributes = rt
        .config
        .paths
        .state_dir
        .join("personal-memory.gitattributes");
    let replay_attributes = match write_shared_file(&attributes, PERSONAL_MEMORY_ATTRIBUTES) {
        Ok(()) => Some(attributes),
        Err(error) => {
            tracing::debug!(%error, "personal memory replay attributes unavailable");
            None
        }
    };
    // The sync's own push is not a code push, so it never wakes itself.
    let lease = transport.lease(&personal.repo_id.to_string(), &personal.transport_url);
    let request = MemberPathSync {
        repository: rt.config.paths.workspace_root.join(&personal.root_path),
        transport_url: lease.url().into(),
        branch: personal.branch,
        directories: PERSONAL_MEMORY_DIRECTORIES.map(String::from).to_vec(),
        message: "Save agent memory".into(),
        replay_attributes,
    };
    Ok(
        match ctx
            .native(move |git| async move {
                let _lease = lease;
                git.sync_paths(&request).await
            })
            .await
        {
            Ok(MemberPathSyncOutcome::Current) => json!({"status":"current"}),
            Ok(MemberPathSyncOutcome::Published { head, files }) => {
                json!({"status":"published","head":head,"files":files})
            }
            Ok(MemberPathSyncOutcome::Skipped { reason, detail }) => {
                json!({"status":"skipped","reason":reason,"detail":detail})
            }
            Err(GitFailure::Failed(detail)) => json!({"status":"failed","detail":detail}),
            Err(GitFailure::Stopped(stop)) => return Err(stop),
        },
    )
}

/// Replace a small runtime-owned file that module Git processes read.
fn write_shared_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("runtime file has no parent"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".g5-memory-")
        .tempfile_in(parent)?;
    std::io::Write::write_all(&mut temporary, contents.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// workspace.diagnostics
// ---------------------------------------------------------------------------

/// Run one of the fixed probes. Cancellation and the deadline apply to the
/// git children of `workspace-integrity`; the others are pure observation.
pub async fn probe(
    rt: &ModuleRuntime,
    name: &str,
    cancel: &CancellationToken,
    deadline: Option<Instant>,
) -> Result<Value, Stop> {
    let ctx = Ctx {
        rt,
        cancel,
        deadline,
        output_limit: PROBE_OUTPUT_LIMIT,
        native_cancel: NativeCancellation::default(),
        git_guard: None,
    };
    Ok(match name {
        "workspace-integrity" => {
            let mut members = Vec::new();
            for repo in &rt.config.layout.repos {
                let root = rt.config.paths.workspace_root.join(&repo.root_path);
                let mut report = association(&ctx, repo, &root).await?;
                report["git_dir_present"] = json!(root.join(".git").exists());
                members.push(report);
            }
            let ok = members
                .iter()
                .all(|m| errors_of(m).is_empty() && m["git_dir_present"] == true);
            json!({"probe":name,"ok":ok,"members":members})
        }
        "runtime-health" => {
            let observation = rt.observe();
            json!({"probe":name,"ok":observation["health"]["lease_valid"] == true,"observation":observation})
        }
        "port-connectivity" => {
            let ports: Vec<u16> = rt.observe()["ports"]
                .as_array()
                .map(|p| {
                    p.iter()
                        .filter_map(|v| v.as_u64())
                        .map(|v| v as u16)
                        .collect()
                })
                .unwrap_or_default();
            let mut results = Vec::new();
            for port in ports {
                let reachable = tokio::time::timeout(
                    Duration::from_millis(250),
                    tokio::net::TcpStream::connect(("127.0.0.1", port)),
                )
                .await
                .is_ok_and(|r| r.is_ok());
                results.push(json!({"port":port,"reachable":reachable}));
            }
            json!({"probe":name,"ok":results.iter().all(|r| r["reachable"] == true),"ports":results})
        }
        "module-catalog-integrity" => {
            let path = rt.catalog_path();
            let on_disk = std::fs::read(path)
                .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
                .ok();
            let ok = on_disk.as_deref() == Some(rt.catalog.content_hash.as_str());
            json!({"probe":name,"ok":ok,"loaded_hash":rt.catalog.content_hash,"on_disk_hash":on_disk,"catalog_version":rt.catalog.catalog_version,"path":path})
        }
        other => json!({"probe":other,"ok":false,"error":"UNKNOWN_PROBE","known":PROBES}),
    })
}

/// Bounded, redacted diagnostics bundle (`g5.diagnostics.bundle/v1`). Shared
/// by the `diagnostics.capture` RPC and the `workspace.diagnostics capture`
/// module operation so both surfaces produce identical evidence.
pub async fn capture(
    rt: &ModuleRuntime,
    profile: &str,
    cancel: &CancellationToken,
    deadline: Option<Instant>,
) -> Result<Result<Value, RpcError>, Stop> {
    let probes: Vec<&str> = match profile {
        "runtime" => vec!["runtime-health", "module-catalog-integrity"],
        "workspace" => vec!["workspace-integrity", "runtime-health"],
        "full" => PROBES.to_vec(),
        other => {
            return Ok(Err(RpcError {
                code: -32602,
                name: "UNKNOWN_PROFILE",
                data: json!({"profile":other,"known":CAPTURE_PROFILES}),
            }))
        }
    };
    let mut results = Vec::new();
    for name in probes {
        results.push(probe(rt, name, cancel, deadline).await?);
    }
    let bundle = json!({
        "bundle_version": "g5.diagnostics.bundle/v1",
        "profile": profile,
        "captured_at": crate::protocol::now_ms(),
        "workspace_id": rt.config.workspace_id,
        "vm_id": rt.config.vm_id,
        "epoch": rt.config.epoch,
        "catalog_version": rt.catalog.catalog_version,
        "catalog_hash": rt.catalog.content_hash,
        "probes": results,
        "executions": rt.history(&json!({})).unwrap_or(Value::Null),
    });
    Ok(Ok(rt.secrets().redact_json(&bundle)))
}

async fn diagnostics(ctx: &Ctx<'_>, operation: &str, input: &Value) -> Result<Value, Stop> {
    match operation {
        "probe" => {
            let name = input["name"].as_str().unwrap_or("");
            if !PROBES.contains(&name) {
                return Ok(outcome(
                    "failed",
                    json!({"reason":"UNKNOWN_PROBE","name":name,"known":PROBES}),
                ));
            }
            let result = probe(ctx.rt, name, ctx.cancel, ctx.deadline).await?;
            Ok(json!({"status":"succeeded","result":result}))
        }
        "capture" => {
            let profile = input["profile"].as_str().unwrap_or("runtime");
            Ok(
                match capture(ctx.rt, profile, ctx.cancel, ctx.deadline).await? {
                    Ok(bundle) => json!({"status":"succeeded","result":bundle}),
                    Err(error) => {
                        outcome("failed", json!({"reason":error.name,"detail":error.data}))
                    }
                },
            )
        }
        other => Ok(outcome(
            "failed",
            json!({"reason":"UNKNOWN_OPERATION","operation":other}),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_branch_name, validate_input, CHANGES, PREVIEW};
    use serde_json::json;

    #[test]
    fn scoped_commit_rejects_unselected_or_unsafe_paths() {
        let repo_id = uuid::Uuid::new_v4();
        assert!(validate_input(
            CHANGES,
            "commit",
            &json!({"repo_id":repo_id,"paths":["new file.txt","src/main.rs"],"expected_blobs":{"new file.txt":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}})
        )
        .is_ok());
        for input in [
            json!({"paths":["src/main.rs"]}),
            json!({"repo_id":repo_id,"paths":[]}),
            json!({"repo_id":repo_id,"paths":["../outside"]}),
            json!({"repo_id":repo_id,"paths":["/outside"]}),
            json!({"repo_id":repo_id,"paths":["a"],"expected_blobs":{"b":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}),
            json!({"repo_id":repo_id,"paths":["a"],"expected_blobs":{"a":"bad"}}),
        ] {
            assert!(
                validate_input(CHANGES, "commit", &input).is_err(),
                "{input}"
            );
        }
        assert!(validate_input(
            CHANGES,
            "push",
            &json!({"repo_id":repo_id,"paths":["src/main.rs"]})
        )
        .is_err());
        assert!(validate_input(
            CHANGES,
            "push",
            &json!({"repo_id":repo_id,"push_head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
        )
        .is_ok());
        assert!(validate_input(
            CHANGES,
            "commit",
            &json!({"repo_id":repo_id,"paths":["a"],"push_head":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
        ).is_err());
    }

    /// The preview origin the API derives (`g5.workspace.preview-origin-security`).
    #[test]
    fn preview_start_accepts_only_a_bare_https_origin() {
        let repo_id = uuid::Uuid::new_v4();
        let start = |origin: serde_json::Value| {
            validate_input(
                PREVIEW,
                "start",
                &json!({"repo_id": repo_id, "profile": "full-stack", "origin": origin}),
            )
        };
        assert!(start(json!(
            "https://0b9c2f0e-4ea9-401d-a054-8f782edffd5b.preview.g5.example"
        ))
        .is_ok());
        assert!(start(json!(null)).is_ok());
        for bad in [
            "http://p.preview.g5.example",
            "https://p.preview.g5.example/",
            "https://p.preview.g5.example:8443",
            "https://user@p.preview.g5.example",
            "https://P.preview.g5.example",
            "",
        ] {
            assert!(start(json!(bad)).is_err(), "{bad:?}");
        }
    }

    /// The guest refuses the profile names the API refuses: one kebab-case
    /// rule, `g5_contracts::types::validate_preview_profile_name`.
    #[test]
    fn preview_start_accepts_only_kebab_case_profile_names() {
        let repo_id = uuid::Uuid::new_v4();
        for profile in ["web", "web-client-2"] {
            assert!(
                validate_input(
                    PREVIEW,
                    "start",
                    &json!({"repo_id":repo_id,"profile":profile})
                )
                .is_ok(),
                "{profile}"
            );
        }
        for profile in ["web_client", "Web", "", "a.b", "../etc", &"x".repeat(65)] {
            assert!(
                validate_input(
                    PREVIEW,
                    "start",
                    &json!({"repo_id":repo_id,"profile":profile})
                )
                .is_err(),
                "{profile:?}"
            );
            assert!(
                g5_contracts::types::validate_preview_profile_name(profile).is_err(),
                "the API refuses {profile:?} too"
            );
        }
    }

    #[test]
    fn branch_names_follow_git_rules_not_a_character_whitelist() {
        for name in [
            "feature+test",
            "shared",
            "feature/abc-1.2_x",
            "release/2026.09",
            "über",
            "a#b",
            "a=b",
            "a,b",
            "a@b",
            "a(b)",
        ] {
            assert!(valid_branch_name(name), "{name}");
        }
        for name in [
            "",
            "@",
            "-x",
            "/x",
            "x/",
            "x.",
            "a..b",
            "a//b",
            "a@{b",
            "a b",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "a\tb",
            ".hidden",
            "x/.y",
            "x.lock",
            "x/y.lock/z",
        ] {
            assert!(!valid_branch_name(name), "{name}");
        }
    }
}
