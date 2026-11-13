#[cfg(unix)]
mod flush;
#[cfg(unix)]
pub use flush::{
    MemberFileInventory, MemberFlushCapture, MemberFlushOutcome, MemberFlushPush,
    MemberFlushSnapshot, MemberObjectOwnership,
};
// Git mechanics for an admitted member checkout. The dispatcher supplies the
// runtime context and retains membership, admission and transport leases.
#[cfg(unix)]
mod checkpoint;
#[cfg(unix)]
pub use checkpoint::{MemberCheckpoint, MemberCheckpointResult};
#[cfg(unix)]
mod path_sync;
#[cfg(unix)]
pub use path_sync::{MemberPathSync, MemberPathSyncOutcome};
mod source_control;
mod transport;
pub use transport::{MemberPush, MemberTrackingRefresh, MemberTrackingResult};
mod update;
pub use update::{MemberParentUpdate, MemberUpdateFailure};
mod rebase;
pub use rebase::{MemberRebase, MemberRebaseFailure, MemberRebaseResult};
mod adopt;
pub use adopt::{MemberAdopt, MemberAdoptFailure, MemberAdoptResult};

use super::{
    invalid, Context, FailureEffect, FailureKind, Invocation, NativeCancellation, NativeGit,
    NativeGitError, Output,
};
use crate::GitAuthor;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct MemberGitEnvironment {
    pub search_path: OsString,
    pub home: PathBuf,
    pub global_config: PathBuf,
    pub workspace_socket: PathBuf,
    pub author: GitAuthor,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub library_path: Option<OsString>,
    pub dyld_library_path: Option<OsString>,
}

#[derive(Clone, Debug)]
pub struct MemberGit {
    git: NativeGit,
    deadline: Option<Instant>,
    output_limit: usize,
}

impl MemberGit {
    pub fn new(
        environment: MemberGitEnvironment,
        deadline: Option<Instant>,
        output_limit: usize,
        cancellation: NativeCancellation,
    ) -> Result<Self, NativeGitError> {
        if output_limit == 0 {
            return Err(invalid(
                "setup member Git",
                "Git output limit must be positive",
            ));
        }
        let binary =
            super::process::resolve_host_binary(&environment.search_path).map_err(|detail| {
                NativeGitError {
                    operation: "setup member Git",
                    target: None,
                    kind: FailureKind::Spawn,
                    effect: FailureEffect::Refused,
                    detail,
                }
            })?;
        Ok(Self {
            git: NativeGit {
                binary,
                // Per invocation when the request sets no deadline; member
                // fetches and pushes can move large histories.
                timeout: Duration::from_secs(1800),
                deadline,
                cancellation,
                context: Context::Member(environment),
                process_scope: None,
                hidden_protocol_refs: Vec::new(),
                #[cfg(test)]
                bootstrap_test_root: None,
            },
            deadline,
            output_limit,
        })
    }

    fn command(&self, root: &Path, operation: &'static str) -> Invocation {
        let mut command = self.git.command(Some(root), operation);
        command.capture_limit(self.output_limit);
        command
    }

    async fn output(
        &self,
        root: &Path,
        operation: &'static str,
        args: &[&str],
        read_only: bool,
        index: Option<&Path>,
        file: Option<std::fs::File>,
    ) -> Result<Output, NativeGitError> {
        let mut command = self.command(root, operation);
        for arg in args {
            command.arg(arg);
        }
        if read_only {
            command.read_only();
        }
        if let Some(index) = index {
            command.temporary_index(index);
        }
        if let Some(file) = file {
            command.stdout_file(file);
        }
        command
            .execute()
            .await
            .map_err(|error| error.at_target(&root.display().to_string(), &crate::GitAuth::None))
    }

    fn text_result(
        &self,
        output: Output,
        operation: &'static str,
        read_only: bool,
    ) -> Result<String, NativeGitError> {
        let effect = if read_only {
            FailureEffect::Refused
        } else if output.completed_successfully() {
            FailureEffect::Partial
        } else {
            FailureEffect::Unknown
        };
        if !output.completed_successfully() || output.truncated {
            return Err(output.failure(
                operation,
                &crate::GitAuth::None,
                if output.truncated {
                    FailureKind::InvalidOutput
                } else {
                    FailureKind::Exit
                },
                effect,
            ));
        }
        String::from_utf8(output.into_protocol_bytes())
            .map(|text| text.trim_end().to_owned())
            .map_err(|_| NativeGitError {
                operation,
                target: None,
                kind: FailureKind::InvalidOutput,
                effect,
                detail: "Git output is not valid UTF-8".into(),
            })
    }

    async fn read(
        &self,
        root: &Path,
        operation: &'static str,
        args: &[&str],
    ) -> Result<String, NativeGitError> {
        self.text_result(
            self.output(root, operation, args, true, None, None).await?,
            operation,
            true,
        )
    }

    async fn read_optional(
        &self,
        root: &Path,
        operation: &'static str,
        args: &[&str],
    ) -> Result<Option<String>, NativeGitError> {
        let output = self.output(root, operation, args, true, None, None).await?;
        if output.status.as_ref().and_then(|status| status.code()) == Some(1)
            && !output.truncated
            && output.stderr_text().is_empty()
        {
            return Ok(None);
        }
        self.text_result(output, operation, true).map(Some)
    }

    /// Observe the exact current HEAD. Corruption/unavailable storage is an
    /// error, never an absent or clean checkout.
    pub async fn head_commit(&self, root: &Path) -> Result<String, NativeGitError> {
        let oid = self
            .read(root, "read member HEAD", &["rev-parse", "--verify", "HEAD"])
            .await?;
        if !super::observations::full_oid(&oid) {
            return Err(invalid(
                "read member HEAD",
                "Git returned an invalid HEAD object ID",
            ));
        }
        Ok(oid)
    }

    pub async fn current_branch(&self, root: &Path) -> Result<Option<String>, NativeGitError> {
        self.read_optional(
            root,
            "read member branch",
            &["symbolic-ref", "-q", "--short", "HEAD"],
        )
        .await
    }

    pub async fn tracking_tip(
        &self,
        root: &Path,
        branch: &str,
    ) -> Result<Option<String>, NativeGitError> {
        self.check_branch(root, branch, "read member tracking ref")
            .await?;
        self.read_optional(
            root,
            "read member tracking ref",
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{branch}"),
            ],
        )
        .await
    }

    pub async fn sequencers(&self, root: &Path) -> Result<Vec<String>, NativeGitError> {
        let git_dir = self
            .read(
                root,
                "read member Git directory",
                &["rev-parse", "--absolute-git-dir"],
            )
            .await?;
        Ok([
            ("rebase-merge", "rebase"),
            ("rebase-apply", "rebase"),
            ("MERGE_HEAD", "merge"),
            ("CHERRY_PICK_HEAD", "cherry-pick"),
            ("REVERT_HEAD", "revert"),
            ("BISECT_LOG", "bisect"),
        ]
        .into_iter()
        .filter(|(marker, _)| Path::new(&git_dir).join(marker).exists())
        .map(|(_, label)| label.to_owned())
        .collect())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemberStatusEntry {
    pub status: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct MemberDivergence {
    pub ahead: u64,
    pub behind: u64,
}

impl MemberGit {
    /// Status does not acquire the optional index refresh lock. Unknown or
    /// truncated output is an error, never an empty/clean status.
    pub async fn status(&self, root: &Path) -> Result<Vec<MemberStatusEntry>, NativeGitError> {
        let text = self
            .read(
                root,
                "read member status",
                &[
                    "--no-optional-locks",
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                ],
            )
            .await?;
        parse_status(&text)
    }

    pub async fn divergence(
        &self,
        root: &Path,
        tracking_branch: &str,
    ) -> Result<MemberDivergence, NativeGitError> {
        let operation = "read member divergence";
        self.check_branch(root, tracking_branch, operation).await?;
        let text = self
            .read(
                root,
                operation,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("HEAD...refs/remotes/origin/{tracking_branch}"),
                ],
            )
            .await?;
        let counts: Vec<_> = text.split_whitespace().collect();
        if counts.len() != 2 {
            return Err(invalid(operation, "Git returned invalid divergence counts"));
        }
        Ok(MemberDivergence {
            ahead: counts[0]
                .parse()
                .map_err(|_| invalid(operation, "Git returned invalid ahead count"))?,
            behind: counts[1]
                .parse()
                .map_err(|_| invalid(operation, "Git returned invalid behind count"))?,
        })
    }

    pub async fn conflicts(
        &self,
        root: &Path,
        paths: Option<&[String]>,
    ) -> Result<Vec<String>, NativeGitError> {
        let literal: Vec<String> = paths
            .unwrap_or_default()
            .iter()
            .map(|path| format!(":(literal){path}"))
            .collect();
        let mut args = vec!["diff", "--name-only", "-z", "--diff-filter=U", "--"];
        args.extend(literal.iter().map(String::as_str));
        let text = self.read(root, "read member conflicts", &args).await?;
        Ok(text
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect())
    }

    async fn check_branch(
        &self,
        root: &Path,
        branch: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        // Full refs permit names that checkout treats as options or HEAD
        // shorthand. The public operation accepts literal short branch names.
        if branch.starts_with('-') || branch == "HEAD" {
            return Err(failed("Invalid member branch name"));
        }
        self.read(
            root,
            operation,
            &["check-ref-format", &format!("refs/heads/{branch}")],
        )
        .await
        .map(|_| ())
    }
}

pub(super) fn parse_status(text: &str) -> Result<Vec<MemberStatusEntry>, NativeGitError> {
    let operation = "read member status";
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(invalid(
            operation,
            "Git status did not contain complete path records",
        ));
    }
    let mut fields = text.split_terminator('\0');
    let mut entries = Vec::new();
    while let Some(entry) = fields.next() {
        let (Some(status), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            return Err(invalid(
                operation,
                "Git status returned an invalid path record",
            ));
        };
        if path.is_empty() || entry.as_bytes()[2] != b' ' {
            return Err(invalid(
                operation,
                "Git status returned an invalid path record",
            ));
        }
        let from = if status.contains(['R', 'C']) {
            Some(
                fields
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| invalid(operation, "Git status omitted a rename source"))?
                    .to_owned(),
            )
        } else {
            None
        };
        entries.push(MemberStatusEntry {
            status: status.trim().to_owned(),
            path: path.to_owned(),
            from,
        });
    }
    Ok(entries)
}

/// Per-operation, in-memory evidence. This is neither retry state nor a queue.
struct MemberOperation<'a> {
    member: &'a MemberGit,
    root: &'a Path,
    name: &'static str,
    transport: Option<&'a str>,
    completed: std::sync::Mutex<Vec<String>>,
}

impl<'a> MemberOperation<'a> {
    fn new(member: &'a MemberGit, root: &'a Path, name: &'static str) -> Self {
        Self {
            member,
            root,
            name,
            transport: None,
            completed: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn failure(&self, mut error: NativeGitError) -> NativeGitError {
        let completed = self.completed.lock().unwrap();
        if !completed.is_empty() {
            error.effect = FailureEffect::Partial;
            error.detail = format!(
                "Completed Git steps: {}; {}",
                completed.join(", "),
                error.detail
            );
        }
        error.operation = self.name;
        error.at_target(&self.root.display().to_string(), &crate::GitAuth::None)
    }

    async fn run(
        &self,
        args: &[&str],
        index: Option<&Path>,
        file: Option<std::fs::File>,
    ) -> Result<String, NativeGitError> {
        // This classifier sees only private, fixed algorithms, never caller argv.
        let mut verb = 0;
        while args.get(verb).is_some_and(|arg| arg.starts_with('-')) {
            verb += if args[verb] == "-c" { 2 } else { 1 };
        }
        let verb = args.get(verb).copied().unwrap_or("");
        let read_only = matches!(
            verb,
            "rev-parse"
                | "symbolic-ref"
                | "ls-files"
                | "diff"
                | "status"
                | "merge-base"
                | "rev-list"
                | "cat-file"
                | "ls-remote"
                | "check-ref-format"
        ) || (verb == "hash-object" && !args.contains(&"-w"));
        let mut command = self.member.command(self.root, self.name);
        if let Some(url) = self.transport {
            command.relay(url);
        }
        for arg in args {
            command.arg(arg);
        }
        if read_only {
            command.read_only();
        }
        if let Some(index) = index {
            command.temporary_index(index);
        }
        if let Some(file) = file {
            command.stdout_file(file);
        }
        let output = command.execute().await?;
        if output.completed_successfully() && !read_only {
            self.completed.lock().unwrap().push(verb.to_owned());
        }
        self.member.text_result(output, self.name, read_only)
    }

    async fn read(&self, args: &[&str]) -> Result<String, NativeGitError> {
        self.run(args, None, None).await
    }

    /// Refuse a relay URL that repository configuration rewrites (insteadOf),
    /// so member transport can only reach its runtime-owned lease.
    async fn check_relay_unchanged(&self, transport_url: &str) -> Result<(), NativeGitError> {
        if self
            .read(&["ls-remote", "--get-url", "--", transport_url])
            .await?
            != transport_url
        {
            return Err(failed("Member transport lease was rewritten"));
        }
        Ok(())
    }
}

fn failed(detail: impl Into<String>) -> NativeGitError {
    NativeGitError {
        operation: "change member checkout",
        target: None,
        kind: FailureKind::Rejected,
        effect: FailureEffect::Refused,
        detail: detail.into(),
    }
}
fn is_stopped(error: &NativeGitError) -> bool {
    matches!(error.kind, FailureKind::Cancelled | FailureKind::Deadline)
}

fn validate_relay(url: &str, operation: &'static str) -> Result<(), NativeGitError> {
    let parsed =
        url::Url::parse(url).map_err(|_| invalid(operation, "Invalid member transport lease"))?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some("127.0.0.1")
        || parsed.port().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid(
            operation,
            "Member transport must be the runtime's loopback lease",
        ));
    }
    Ok(())
}

impl MemberGit {
    /// Retain complete-operation cleanup when a caller drops its awaiter.
    /// The same cancellation counter covers this owner and all child commands,
    /// so admission owners can await wait_idle before releasing their custody.
    async fn supervise_operation<T: Send + 'static>(
        &self,
        operation: &'static str,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<T, NativeGitError> {
        let (keep_alive, mut caller_dropped) = tokio::sync::oneshot::channel::<()>();
        let cancellation = self.git.cancellation.clone();
        let active = cancellation.started();
        let task = tokio::spawn(async move {
            let _active = active;
            tokio::pin!(work);
            tokio::select! {
                result = &mut work => result,
                _ = &mut caller_dropped => {
                    cancellation.cancel();
                    work.await
                }
            }
        });
        let result = task.await.map_err(|_| NativeGitError {
            operation, target: None, kind: FailureKind::Io, effect: FailureEffect::Unknown,
            detail: "Member Git operation owner stopped unexpectedly; inspect current state before retrying".into(),
        });
        drop(keep_alive);
        result
    }
}

#[cfg(all(test, unix))]
mod tests;

impl MemberGit {
    /// Named owner-only review comparisons. Fixed Git commands, no ref picker,
    /// no hooks or external diff driver. File bytes keep trailing newlines.
    pub async fn review_inspect(
        &self,
        root: &Path,
        mode: &str,
        pushed: &str,
        path: Option<&str>,
    ) -> Result<serde_json::Value, NativeGitError> {
        let op = "inspect workspace review content";
        if !matches!(
            mode,
            "all" | "uncommitted" | "unpushed" | "source_state" | "source_diff"
        ) || !matches!(pushed.len(), 40 | 64)
            || !pushed.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid(op, "Invalid named review comparison"));
        }
        if mode.starts_with("source_") {
            return self.source_control_inspect(root, mode, pushed).await;
        }
        let head = self
            .read(root, op, &["rev-parse", "--verify", "HEAD"])
            .await?;
        let base = if mode == "unpushed" { pushed } else { &head };
        let listed = if mode == "all" {
            self.read(
                root,
                op,
                &[
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ],
            )
            .await?
        } else {
            self.read(
                root,
                op,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--name-only",
                    "-z",
                    base,
                    "--",
                ],
            )
            .await?
        };
        let mut paths = listed
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        let statuses = self.status(root).await?;
        for s in statuses {
            if s.status == "??" {
                paths.insert(s.path);
            } else if mode == "all" && s.status.contains('D') {
                paths.remove(&s.path);
            }
        }
        let mut before = None;
        let mut binary = false;
        if let Some(path) = path {
            if path.is_empty()
                || Path::new(path).is_absolute()
                || path.split('/').any(|s| s == ".." || s == ".")
                || path.contains('\0')
            {
                return Err(invalid(op, "Invalid file path"));
            }
            let present = self
                .read(
                    root,
                    op,
                    &[
                        "ls-tree",
                        "--name-only",
                        "-z",
                        base,
                        "--",
                        &format!(":(literal){path}"),
                    ],
                )
                .await?;
            if !present.is_empty() {
                let output = self
                    .output(
                        root,
                        op,
                        &[
                            "show",
                            "--no-ext-diff",
                            "--no-textconv",
                            &format!("{base}:{path}"),
                        ],
                        true,
                        None,
                        None,
                    )
                    .await?;
                if !output.completed_successfully() || output.truncated {
                    return Err(output.failure(
                        op,
                        &crate::GitAuth::None,
                        FailureKind::InvalidOutput,
                        FailureEffect::Refused,
                    ));
                }
                let bytes = output.into_protocol_bytes();
                binary = bytes.contains(&0) || std::str::from_utf8(&bytes).is_err();
                before = String::from_utf8(bytes).ok();
            }
        }
        Ok(
            serde_json::json!({"headCommit":head,"beforeCommit":base,"paths":paths,"before":before,"binary":binary}),
        )
    }
}
