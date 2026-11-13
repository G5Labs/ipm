//! Complete parent update, including replay resolution and bounded abort.
use super::{failed, is_stopped, validate_relay, MemberGit, MemberOperation};
use crate::native::{NativeCancellation, NativeGitError};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq)]
enum RebaseOutcome {
    /// Every commit replayed as it was.
    Clean,
    /// The base's version was kept in these paths, where a commit of the
    /// copy collided with it; everything else of the copy replayed.
    Resolved(Vec<String>),
    /// The rebase could not complete; the sequencer must be aborted by the
    /// caller. `conflicts` names the unmerged paths, if any were seen.
    Failed {
        detail: NativeGitError,
        conflicts: Vec<String>,
    },
    Stopped(NativeGitError),
}

/// Stops the sequencer can be driven through before a series is given up
/// on: far more commits than a review copy ever holds.
const MAX_REBASE_STOPS: usize = 200;

/// Where, under the dispatcher's own state directory, one update keeps the
/// stage files of the paths it resolves: dispatcher-private storage, never
/// a tree a workspace process can write. Under the repository, a process
/// of the workspace could swap the directory for one whose files are
/// symbolic links to something privileged, and a privileged dispatcher
/// creating "its" file would follow them. Here every run gets a fresh,
/// uniquely named directory, every file is created with `O_EXCL` (which
/// never follows a link), and no path is ever reused across runs.
const UPDATE_SCRATCH_ROOT: &str = "update-scratch";
/// The name prefix of one run's directory under [`UPDATE_SCRATCH_ROOT`].
const UPDATE_SCRATCH_PREFIX: &str = "g5-update-";
/// A run's directory older than this was left by an interrupted update
/// (a module's run is bounded to five minutes) and is swept when the next
/// one begins; younger ones may belong to a concurrent update.
const STALE_SCRATCH_AFTER: Duration = Duration::from_secs(10 * 60);

/// One git command line run in a repository, and the same with its
/// standard output written to a file. Contents of files never pass through
/// a command's (bounded, trimmed) output.
trait GitRun {
    fn run(
        &self,
        args: Vec<String>,
    ) -> impl std::future::Future<Output = Result<String, NativeGitError>> + Send;
    fn run_to_file(
        &self,
        args: Vec<String>,
        dest: &Path,
    ) -> impl std::future::Future<Output = Result<(), NativeGitError>> + Send;
}

/// A fresh scratch directory for one update under `private_root` (the
/// dispatcher's [`UPDATE_SCRATCH_ROOT`]), created with `create_dir` so an
/// existing entry of that name fails rather than being reused, and
/// readable (0755) by the identity the git children run under. Whatever
/// an interrupted earlier update left there past [`STALE_SCRATCH_AFTER`]
/// is swept first; a concurrent update's directory is left alone.
fn prepare_scratch(private_root: &Path) -> std::io::Result<PathBuf> {
    make_readable_dir_all(private_root)?;
    sweep_stale_scratch(private_root);
    let scratch = private_root.join(format!("{UPDATE_SCRATCH_PREFIX}{}", uuid::Uuid::new_v4()));
    make_readable_dir(&scratch)?;
    Ok(scratch)
}

/// Remove the run directories of our own prefix that are older than
/// [`STALE_SCRATCH_AFTER`]. Best effort: a sweep that fails costs nothing
/// but disk.
fn sweep_stale_scratch(private_root: &Path) {
    let Ok(entries) = std::fs::read_dir(private_root) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(UPDATE_SCRATCH_PREFIX) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_SCRATCH_AFTER);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `create_dir_all` with each created directory readable and searchable
/// by everyone (0755), whatever the umask.
fn make_readable_dir_all(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        make_readable_dir_all(parent)?;
    }
    match make_readable_dir(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
        Err(error) => Err(error),
    }
}

/// `create_dir` (an existing entry is an error, never reused) with the
/// directory readable and searchable by everyone (0755).
fn make_readable_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Create `path` for writing as a new file (`O_EXCL`: an existing entry,
/// a symbolic link included, is an error and is never followed), readable
/// by everyone (0644) so a git child under another identity can read it.
fn create_scratch_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(file)
}

/// Remove the scratch directory and everything in it; a directory that is
/// not there is fine.
fn discard_scratch(scratch: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(scratch) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Rebase the checked-out branch onto `base`.
///
/// With `favor_base`, where a commit collides with what the base already
/// holds, the base's side of the collided hunks is kept and the replay
/// goes on. A review copy's rounds end up in its source twice: as the
/// copy's own commits and, once the owner accepts them (possibly edited)
/// and commits, in the source's branch. Replaying the copy's commit over a
/// base that already contains its change collides on those lines; the
/// source's decision is the one that stands, and the commit's other hunks
/// of the same file replay as they are. Each stop of the sequencer is
/// resolved path by path from the index's three stages (base, the
/// upstream side, the replayed commit) with `merge-file --ours`, written
/// back through the index, and the series continued (or the commit
/// skipped when nothing of it remains), so every collided path across the
/// whole series is reported. A stop that is not a collision, or a path
/// without three text stages (added on both sides, deleted on one, a
/// binary), fails the update; the caller aborts the sequencer. Without
/// `favor_base` the first collision fails the update the same way. Stage
/// files live in `scratch`, a fresh dispatcher-private directory outside
/// any workspace-writable tree, removed on every way out, a stop
/// included. Paths are read NUL-separated, never from display quoting.
async fn rebase_onto_base<G: GitRun>(
    git: &G,
    base: &str,
    favor_base: bool,
    scratch: &Path,
) -> RebaseOutcome {
    let outcome = rebase_onto_base_inner(git, base, favor_base, scratch).await;
    // A plain filesystem call: it does not depend on a token or a deadline
    // that a stop has spent.
    let _ = discard_scratch(scratch);
    outcome
}

/// Replay a review against the owner's decisions, then retain the previously
/// published tip as an ancestor without changing the replayed tree. Publication
/// remains a fast-forward even when the replay rewrites earlier review rounds.
/// Use the observed published tip rather than the local tip so retrying after
/// an interrupted final merge also repairs the publication ancestry.
async fn update_review_from_parent<G: GitRun>(
    git: &G,
    base: &str,
    published_head: &str,
    scratch: &Path,
) -> RebaseOutcome {
    let outcome = rebase_onto_base(git, base, true, scratch).await;
    if !matches!(outcome, RebaseOutcome::Clean | RebaseOutcome::Resolved(_)) {
        return outcome;
    }
    match git
        .run(
            ["merge", "--no-edit", "-s", "ours", published_head]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
        .await
    {
        Ok(_) => outcome,
        Err(stop) if is_stopped(&stop) => RebaseOutcome::Stopped(stop),
        Err(detail) => RebaseOutcome::Failed {
            detail,
            conflicts: Vec::new(),
        },
    }
}

async fn rebase_onto_base_inner<G: GitRun>(
    git: &G,
    base: &str,
    favor_base: bool,
    scratch: &Path,
) -> RebaseOutcome {
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut detail = match git.run(args(&["rebase", base])).await {
        Ok(_) => return RebaseOutcome::Clean,
        Err(stop) if is_stopped(&stop) => return RebaseOutcome::Stopped(stop),
        Err(detail) => detail,
    };
    let mut resolved: Vec<String> = Vec::new();
    for _ in 0..MAX_REBASE_STOPS {
        // `-z`: literal paths, NUL-terminated; the display form quotes and
        // escapes non-ASCII names and would not name the index entry.
        let conflicts = match git
            .run(args(&["diff", "--name-only", "-z", "--diff-filter=U"]))
            .await
        {
            Ok(text) => nul_separated(&text),
            Err(stop) if is_stopped(&stop) => return RebaseOutcome::Stopped(stop),
            Err(_) => Vec::new(),
        };
        if !favor_base || conflicts.is_empty() {
            // Not a collision (a fetch that left no base, an untracked file
            // in the way, ...), or a workspace whose collisions must stand
            // as they are: nothing a merge strategy would change.
            return RebaseOutcome::Failed { detail, conflicts };
        }
        for path in &conflicts {
            match merge_favoring_ours(git, path, scratch).await {
                Ok(()) => {}
                Err(stop) if is_stopped(&stop) => return RebaseOutcome::Stopped(stop),
                Err(detail) => {
                    // No three text stages to merge (added on both sides,
                    // deleted on one, a binary): nothing to keep by hunk.
                    return RebaseOutcome::Failed { detail, conflicts };
                }
            }
        }
        for path in conflicts {
            if !resolved.contains(&path) {
                resolved.push(path);
            }
        }
        // A commit whose every change the base already had is now empty:
        // it is skipped rather than continued.
        let staged = match git
            .run(args(&["diff", "--cached", "--name-only", "-z"]))
            .await
        {
            Ok(text) => nul_separated(&text),
            Err(stop) if is_stopped(&stop) => return RebaseOutcome::Stopped(stop),
            Err(detail) => {
                return RebaseOutcome::Failed {
                    detail,
                    conflicts: Vec::new(),
                };
            }
        };
        let step = if staged.is_empty() {
            args(&["rebase", "--skip"])
        } else {
            args(&["-c", "core.editor=true", "rebase", "--continue"])
        };
        detail = match git.run(step).await {
            Ok(_) => return RebaseOutcome::Resolved(resolved),
            Err(stop) if is_stopped(&stop) => return RebaseOutcome::Stopped(stop),
            Err(detail) => detail,
        };
    }
    RebaseOutcome::Failed {
        detail: failed(format!(
            "the replay stopped more than {MAX_REBASE_STOPS} times: {detail}"
        )),
        conflicts: Vec::new(),
    }
}

/// The literal entries of a NUL-separated listing (`-z` output), untrimmed:
/// a name may begin or end with a space.
fn nul_separated(text: &str) -> Vec<String> {
    text.split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Resolve one unmerged `path` of a stopped rebase by hunk, the upstream
/// side winning where the two sides collide, and record the result at
/// stage 0 of the index and in the working tree.
///
/// The three stages are written into `scratch` by `cat-file` with its
/// output redirected (nothing of their content passes through a command's
/// output, which is bounded), merged by `merge-file -p --ours` into a
/// fourth file there, hashed into the object store, and set as the path's
/// single index entry under the upstream side's mode; the working tree is
/// then written from the index. The git children only read the scratch
/// files; the files are the process's own, each created new under a name
/// used once. The mode is resolved like a merge from the three stages'
/// modes: the side that changed it wins, the upstream side when both did.
/// A missing stage or a file that cannot be merged as text fails, and the
/// failure names it.
async fn merge_favoring_ours<G: GitRun>(
    git: &G,
    path: &str,
    scratch: &Path,
) -> Result<(), NativeGitError> {
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // One set of names per resolved path, never reused within the run.
    let serial = uuid::Uuid::new_v4();
    let base = scratch.join(format!("{serial}-base"));
    let ours = scratch.join(format!("{serial}-ours"));
    let theirs = scratch.join(format!("{serial}-theirs"));
    let merged = scratch.join(format!("{serial}-merged"));
    // Stage 1 is the merge base, 2 the side being rebased onto (the
    // upstream: "ours" while a rebase replays), 3 the replayed commit.
    for (stage, dest, what) in [
        (1, &base, "common base"),
        (2, &ours, "base side"),
        (3, &theirs, "replayed side"),
    ] {
        let spec = format!(":{stage}:{path}");
        git.run_to_file(args(&["cat-file", "blob", &spec]), dest)
            .await
            .map_err(|failure| match failure {
                detail if !is_stopped(&detail) => {
                    failed(format!("{path}: no {what} to merge: {detail}"))
                }
                stopped => stopped,
            })?;
    }
    let as_arg = |file: &Path| file.to_string_lossy().into_owned();
    // A non-text file fails here.
    git.run_to_file(
        vec![
            "merge-file".to_string(),
            "-p".to_string(),
            "--ours".to_string(),
            as_arg(&ours),
            as_arg(&base),
            as_arg(&theirs),
        ],
        &merged,
    )
    .await
    .map_err(|failure| match failure {
        detail if !is_stopped(&detail) => {
            failed(format!("{path}: cannot be merged by hunk: {detail}"))
        }
        stopped => stopped,
    })?;
    let merged_oid = git
        .run(vec![
            "hash-object".to_string(),
            "-w".to_string(),
            "--".to_string(),
            as_arg(&merged),
        ])
        .await?
        .trim()
        .to_string();
    // The modes of the three stages (`100644 <oid> <stage>\t<path>`; only
    // the mode and stage columns are read, never the displayed path).
    // `ls-files` takes a pathspec, where a leading `:` is magic and `*`
    // globs: the name is passed literally, the way the index commands
    // below read it.
    let listing = git
        .run(args(&["--literal-pathspecs", "ls-files", "-s", "--", path]))
        .await?;
    let stage_mode = |stage: &str| {
        listing
            .lines()
            .find(|line| line.split_whitespace().nth(2) == Some(stage))
            .and_then(|line| line.split_whitespace().next())
            .map(str::to_string)
    };
    let ours_mode = stage_mode("2")
        .ok_or_else(|| failed(format!("{path}: the base side is not in the index")))?;
    let mode = resolve_mode(
        stage_mode("1").as_deref(),
        &ours_mode,
        stage_mode("3").as_deref(),
    );
    let entry = format!("{mode},{merged_oid},{path}");
    git.run(args(&["update-index", "--cacheinfo", &entry]))
        .await?;
    git.run(args(&[
        "--literal-pathspecs",
        "checkout-index",
        "-f",
        "--",
        path,
    ]))
    .await?;
    Ok(())
}

/// The mode of a resolved path, from the modes of its three stages, the
/// way a merge decides it: the side that changed the mode wins, and where
/// both changed it differently the upstream side does (its decision
/// stands, as for the content). A missing base stage leaves the upstream
/// side's mode.
fn resolve_mode(base: Option<&str>, ours: &str, theirs: Option<&str>) -> String {
    match (base, theirs) {
        (Some(base), Some(theirs)) if ours == base && theirs != base => theirs.to_string(),
        _ => ours.to_string(),
    }
}

impl GitRun for MemberOperation<'_> {
    async fn run(&self, args: Vec<String>) -> Result<String, NativeGitError> {
        self.read(&args.iter().map(String::as_str).collect::<Vec<_>>())
            .await
    }
    async fn run_to_file(&self, args: Vec<String>, dest: &Path) -> Result<(), NativeGitError> {
        let file = create_scratch_file(dest)
            .map_err(|error| failed(format!("Cannot create resolution scratch file: {error}")))?;
        MemberOperation::run(
            self,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
            Some(file),
        )
        .await
        .map(|_| ())
    }
}

#[derive(Clone)]
pub struct MemberParentUpdate {
    pub branch: String,
    pub expected_head: String,
    pub repository: PathBuf,
    pub transport_url: String,
    pub parent_branch: String,
    pub published_head: String,
    pub favor_parent: bool,
    pub private_state: PathBuf,
}

#[derive(Debug)]
pub struct MemberUpdateFailure {
    pub error: NativeGitError,
    pub conflicts: Vec<String>,
}

impl MemberGit {
    /// Fetch the selected parent into local tracking, then merge or replay the
    /// admitted checkout. Failed/stopped sequencers are aborted under a fresh,
    /// bounded cleanup context before the operation returns.
    pub async fn update_parent(
        &self,
        request: &MemberParentUpdate,
    ) -> Result<Vec<String>, MemberUpdateFailure> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("update member from parent", async move {
            member.update_parent_inner(&request).await
        })
        .await
        .map_err(|error| MemberUpdateFailure {
            error,
            conflicts: Vec::new(),
        })?
    }

    async fn update_parent_inner(
        &self,
        request: &MemberParentUpdate,
    ) -> Result<Vec<String>, MemberUpdateFailure> {
        let operation = "update member from parent";
        let mut git = MemberOperation::new(self, &request.repository, operation);
        git.transport = Some(&request.transport_url);
        let mut update_started = false;
        let result = async {
            validate_relay(&request.transport_url, operation)?;
            self.check_branch(&request.repository, &request.parent_branch, operation)
                .await?;
            if !crate::native::observations::full_oid(&request.published_head) {
                return Err(failed("Parent update requires the exact published commit"));
            }
            self.check_branch(&request.repository, &request.branch, operation)
                .await?;
            if !crate::native::observations::full_oid(&request.expected_head) {
                return Err(failed(
                    "Parent update requires the exact admitted checkout head",
                ));
            }
            if self.current_branch(&request.repository).await?.as_deref() != Some(&request.branch)
                || self.head_commit(&request.repository).await? != request.expected_head
            {
                return Err(failed("Workspace branch changed before parent update"));
            }
            git.check_relay_unchanged(&request.transport_url).await?;
            let base = format!("refs/remotes/origin/{}", request.parent_branch);
            git.read(&[
                "fetch",
                "--no-tags",
                "--recurse-submodules=no",
                "--",
                &request.transport_url,
                &format!("+refs/heads/{}:{base}", request.parent_branch),
            ])
            .await?;
            if self.current_branch(&request.repository).await?.as_deref() != Some(&request.branch)
                || self.head_commit(&request.repository).await? != request.expected_head
            {
                return Err(failed("Workspace branch changed while fetching parent"));
            }
            update_started = true;
            if !request.favor_parent {
                return Ok(match git.read(&["merge", "--no-edit", &base]).await {
                    Ok(_) => RebaseOutcome::Clean,
                    Err(error) if is_stopped(&error) => RebaseOutcome::Stopped(error),
                    Err(detail) => RebaseOutcome::Failed {
                        detail,
                        conflicts: Vec::new(),
                    },
                });
            }
            let scratch = prepare_scratch(&request.private_state.join(UPDATE_SCRATCH_ROOT))
                .map_err(|error| failed(format!("Cannot prepare update scratch: {error}")))?;
            let outcome =
                update_review_from_parent(&git, &base, &request.published_head, &scratch).await;
            let _ = discard_scratch(&scratch);
            Ok(outcome)
        }
        .await;
        let (mut error, conflicts) = match result {
            Ok(RebaseOutcome::Clean) => return Ok(Vec::new()),
            Ok(RebaseOutcome::Resolved(paths)) => return Ok(paths),
            Ok(RebaseOutcome::Failed { detail, conflicts }) => (detail, conflicts),
            Ok(RebaseOutcome::Stopped(error)) | Err(error) => (error, Vec::new()),
        };
        if update_started {
            let mut cleanup_member = self.clone();
            cleanup_member.git.cancellation = NativeCancellation::default();
            cleanup_member.deadline = Some(Instant::now() + Duration::from_secs(10));
            cleanup_member.git.deadline = cleanup_member.deadline;
            let cleanup =
                MemberOperation::new(&cleanup_member, &request.repository, "abort member update");
            let commands: &[&str] = if request.favor_parent {
                &["merge", "rebase"]
            } else {
                &["merge"]
            };
            for command in commands {
                if let Err(abort) = cleanup.read(&[command, "--abort"]).await {
                    // No sequencer is a normal result after an early failure.
                    if !abort.detail.contains("no rebase in progress")
                        && !abort.detail.contains("There is no merge to abort")
                    {
                        error
                            .detail
                            .push_str(&format!("; cleanup: {}", abort.detail));
                    }
                }
            }
        }
        Err(MemberUpdateFailure {
            error: git.failure(error),
            conflicts,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::checkpoint::{selected_index_entry, SelectedIndexLock};
    use super::*;
    fn git(root: &Path, args: &[&str]) -> Result<String, NativeGitError> {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|e| failed(e.to_string()))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned())
        } else {
            Err(failed(
                String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_owned(),
            ))
        }
    }

    fn ok(root: &Path, args: &[&str]) -> String {
        match git(root, args) {
            Ok(text) => text,
            Err(detail) => panic!("{args:?}: {detail}"),
        }
    }

    fn write(root: &Path, path: &str, text: &str) {
        std::fs::write(root.join(path), text).unwrap();
    }

    #[test]
    fn selected_checkpoint_lock_preserves_native_staging() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        ok(root, &["init", "-q"]);
        write(root, "selected.txt", "base\n");
        write(root, "unrelated.txt", "base\n");
        ok(root, &["add", "-A"]);
        ok(root, &["commit", "-q", "-m", "base"]);
        write(root, "unrelated.txt", "staged\n");
        ok(root, &["add", "unrelated.txt"]);
        let staged_oid = ok(root, &["rev-parse", ":unrelated.txt"]);
        let index = root.join(".git/index");
        let mut lock = SelectedIndexLock::acquire(&index).unwrap();
        let copy = root.join(".git/index.test-copy");
        std::fs::copy(&index, &copy).unwrap();
        write(root, "selected.txt", "new\n");
        assert!(git(root, &["add", "selected.txt"]).is_err());
        lock.install(&copy).unwrap();
        assert_eq!(ok(root, &["rev-parse", ":unrelated.txt"]), staged_oid);
        ok(root, &["add", "selected.txt"]);
    }

    #[test]
    fn selected_checkpoint_rejects_unmerged_index_entries() {
        let oid = "a".repeat(40);
        let unmerged = format!("100644 {oid} 1\tfile\0");
        assert!(selected_index_entry(&unmerged, "file").is_err());
    }

    #[test]
    fn selected_checkpoint_requires_one_exact_stage_zero_path() {
        let oid = "a".repeat(40);
        let selected = format!("100644 {oid} 0\tdir\0");
        assert!(matches!(
            selected_index_entry(&selected, "dir"),
            Ok(Some((mode, hash))) if mode == "100644" && hash == oid
        ));
        assert!(matches!(selected_index_entry("", "dir"), Ok(None)));
        assert!(selected_index_entry(&selected, "dir/child").is_err());
        assert!(selected_index_entry(&format!("100644 {oid} 0\tdir/child\0"), "dir").is_err());
        assert!(selected_index_entry(&format!("{selected}{selected}"), "dir").is_err());
        assert!(selected_index_entry(&format!("100644 {oid} 0\tdir\nchild\0"), "dir").is_err());
    }

    /// A copy checked out on `shared` at B0 (README `a/b/c`, `a.txt` "1",
    /// `b.txt` "2"), with the source's branch fetched as
    /// `refs/remotes/origin/main` at B1: B1 wrote `edits` and added
    /// `w.txt`.
    fn copy_with_moved_base_edits(edits: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        ok(&root, &["init", "-q", "-b", "shared"]);
        write(&root, "README.md", "a\nb\nc\n");
        write(&root, "a.txt", "1\n");
        write(&root, "b.txt", "2\n");
        ok(&root, &["add", "-A"]);
        ok(&root, &["commit", "-q", "-m", "B0"]);
        // B1 on a scratch branch, published as the remote-tracking ref.
        ok(&root, &["checkout", "-q", "-b", "scratch"]);
        for (path, text) in edits {
            write(&root, path, text);
        }
        write(&root, "w.txt", "owner\n");
        ok(&root, &["add", "-A"]);
        ok(&root, &["commit", "-q", "-m", "B1"]);
        let b1 = ok(&root, &["rev-parse", "HEAD"]);
        ok(&root, &["update-ref", "refs/remotes/origin/main", &b1]);
        ok(&root, &["checkout", "-q", "shared"]);
        ok(&root, &["branch", "-q", "-D", "scratch"]);
        (dir, root)
    }

    /// B1 changed README's first line to `base_line`.
    fn copy_with_moved_base(base_line: &str) -> (tempfile::TempDir, PathBuf) {
        let readme = format!("{base_line}\nb\nc\n");
        copy_with_moved_base_edits(&[("README.md", readme.as_str())])
    }

    fn commit(root: &Path, message: &str) -> String {
        ok(root, &["add", "-A"]);
        ok(root, &["commit", "-q", "-m", message]);
        ok(root, &["rev-parse", "HEAD"])
    }

    fn read(root: &Path, path: &str) -> String {
        std::fs::read_to_string(root.join(path)).unwrap()
    }

    struct TestGit(PathBuf);

    impl GitRun for TestGit {
        async fn run(&self, args: Vec<String>) -> Result<String, NativeGitError> {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            git(&self.0, &args)
        }

        async fn run_to_file(&self, args: Vec<String>, dest: &Path) -> Result<(), NativeGitError> {
            let file = create_scratch_file(dest).map_err(|e| failed(e.to_string()))?;
            let output = std::process::Command::new("git")
                .args(["-c", "core.hooksPath=/dev/null"])
                .args(&args)
                .current_dir(&self.0)
                .stdout(file)
                .output()
                .map_err(|e| failed(e.to_string()))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(failed(
                    String::from_utf8_lossy(&output.stderr)
                        .trim_end()
                        .to_owned(),
                ))
            }
        }
    }

    /// The tests' stand-in for the dispatcher's state directory.
    fn private_root() -> PathBuf {
        let root = std::env::temp_dir()
            .join("g5d-builtin-tests")
            .join("update-scratch");
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The update as the module runs it: a fresh scratch directory under
    /// the private root, the rebase, the directory gone.
    async fn rebase(root: &Path, base: &str, favor_base: bool) -> RebaseOutcome {
        let scratch = prepare_scratch(&private_root()).unwrap();
        let outcome =
            rebase_onto_base(&TestGit(root.to_path_buf()), base, favor_base, &scratch).await;
        assert!(
            !scratch.exists(),
            "the scratch directory is gone whatever the outcome"
        );
        assert!(
            !root.join(".git").join("g5-update-scratch").exists(),
            "nothing under the repository"
        );
        outcome
    }

    #[test]
    fn preparing_the_scratch_sweeps_an_interrupted_runs_leftovers_and_keeps_a_concurrent_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("var/lib/g5d/update-scratch");
        std::fs::create_dir_all(&root).unwrap();
        let stale = root.join(format!("{UPDATE_SCRATCH_PREFIX}stale"));
        std::fs::create_dir(&stale).unwrap();
        std::fs::write(stale.join("x-ours"), "half-written\n").unwrap();
        let old = std::time::SystemTime::now() - STALE_SCRATCH_AFTER - Duration::from_secs(60);
        std::fs::File::open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let concurrent = root.join(format!("{UPDATE_SCRATCH_PREFIX}concurrent"));
        std::fs::create_dir(&concurrent).unwrap();
        let foreign = root.join("someone-elses");
        std::fs::create_dir(&foreign).unwrap();

        let scratch = prepare_scratch(&root).unwrap();
        assert!(!stale.exists(), "the interrupted run's directory is swept");
        assert!(concurrent.exists(), "a younger run is left alone");
        assert!(foreign.exists(), "only our own prefix is swept");
        assert!(scratch.is_dir());
        assert!(scratch
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(UPDATE_SCRATCH_PREFIX));
        assert_ne!(
            scratch,
            prepare_scratch(&root).unwrap(),
            "never the same name twice"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&scratch).unwrap().permissions().mode() & 0o777,
                0o755,
                "readable and searchable by the git child's identity"
            );
            let file = scratch.join("probe");
            create_scratch_file(&file).unwrap();
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o644,
                "readable by the git child's identity"
            );
            assert!(
                create_scratch_file(&file).is_err(),
                "an existing entry is never reused"
            );
            std::os::unix::fs::symlink(dir.path().join("victim"), scratch.join("link")).unwrap();
            assert!(
                create_scratch_file(&scratch.join("link")).is_err(),
                "a link is not followed"
            );
            assert!(!dir.path().join("victim").exists());
        }
        discard_scratch(&scratch).unwrap();
        assert!(!scratch.exists());
        discard_scratch(&scratch).unwrap();
    }

    #[test]
    fn the_mode_of_a_resolved_path_follows_the_side_that_changed_it() {
        // Neither changed it.
        assert_eq!(
            resolve_mode(Some("100644"), "100644", Some("100644")),
            "100644"
        );
        // The reviewer made it executable; the base did not touch the mode.
        assert_eq!(
            resolve_mode(Some("100644"), "100644", Some("100755")),
            "100755"
        );
        // The base made it executable; the reviewer did not.
        assert_eq!(
            resolve_mode(Some("100644"), "100755", Some("100644")),
            "100755"
        );
        // Both changed it, differently: the base's decision stands.
        assert_eq!(
            resolve_mode(Some("100755"), "100644", Some("120000")),
            "100644"
        );
        // No base stage known: the base side's mode.
        assert_eq!(resolve_mode(None, "100644", Some("100755")), "100644");
    }

    /// The reviewer changed a line and made the file executable; the base
    /// changed another line: the result carries both edits and is
    /// executable.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_mode_the_reviewer_set_survives_a_content_collision() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, root) = copy_with_moved_base_edits(&[("a.txt", "1 owner\n")]);
        // The copy's commit: the colliding line and the exec bit.
        write(&root, "a.txt", "1 reviewer\n");
        std::fs::set_permissions(root.join("a.txt"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        commit(&root, "C1");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(outcome, RebaseOutcome::Resolved(vec!["a.txt".to_string()]));
        assert_eq!(read(&root, "a.txt"), "1 owner\n", "the base's line stands");
        let listing = ok(&root, &["ls-files", "-s", "--", "a.txt"]);
        assert!(listing.starts_with("100755 "), "{listing}");
        assert_ne!(
            std::fs::metadata(root.join("a.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0,
            "executable in the tree"
        );
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
    }

    /// The base made the file executable (and changed the colliding line);
    /// the reviewer only changed the line: the base's mode.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_mode_the_base_set_carries_over_a_content_collision() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        ok(&root, &["init", "-q", "-b", "shared"]);
        write(&root, "a.txt", "1\n2\n");
        commit(&root, "B0");
        ok(&root, &["checkout", "-q", "-b", "scratch"]);
        write(&root, "a.txt", "1 owner\n2\n");
        std::fs::set_permissions(root.join("a.txt"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let b1 = commit(&root, "B1");
        ok(&root, &["update-ref", "refs/remotes/origin/main", &b1]);
        ok(&root, &["checkout", "-q", "shared"]);
        ok(&root, &["branch", "-q", "-D", "scratch"]);
        write(&root, "a.txt", "1 reviewer\n2\n");
        commit(&root, "C1");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(outcome, RebaseOutcome::Resolved(vec!["a.txt".to_string()]));
        assert_eq!(read(&root, "a.txt"), "1 owner\n2\n");
        let listing = ok(&root, &["ls-files", "-s", "--", "a.txt"]);
        assert!(listing.starts_with("100755 "), "{listing}");
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn a_collision_with_the_moved_base_keeps_the_base_side_and_the_rest_of_the_commit() {
        // The owner accepted the copy's README change with an edit ("A!"),
        // the copy's commit carries its own "A" plus a file of its own.
        let (_dir, root) = copy_with_moved_base("A!");
        write(&root, "README.md", "A\nb\nc\n");
        write(&root, "mine.txt", "reviewer\n");
        ok(&root, &["add", "-A"]);
        ok(&root, &["commit", "-q", "-m", "C1"]);

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(
            outcome,
            RebaseOutcome::Resolved(vec!["README.md".to_string()])
        );
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "A!\nb\nc\n",
            "the base's version stands where the commit collided"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("mine.txt")).unwrap(),
            "reviewer\n",
            "the commit's own file replayed"
        );
        assert!(
            root.join("w.txt").exists(),
            "the base's file is in the tree"
        );
        let b1 = ok(&root, &["rev-parse", "refs/remotes/origin/main"]);
        assert_eq!(
            ok(&root, &["rev-parse", "HEAD~1"]),
            b1,
            "replayed on top of B1"
        );
        assert_eq!(
            ok(&root, &["status", "--porcelain"]),
            "",
            "clean tree, no sequencer"
        );
        assert!(!root.join(".git/rebase-merge").exists());
    }

    #[tokio::test]
    async fn a_clean_replay_reports_no_resolution() {
        let (_dir, root) = copy_with_moved_base("A!");
        write(&root, "mine.txt", "reviewer\n");
        ok(&root, &["add", "-A"]);
        ok(&root, &["commit", "-q", "-m", "C1"]);

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(outcome, RebaseOutcome::Clean);
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "A!\nb\nc\n"
        );
        assert!(root.join("mine.txt").exists());
    }

    #[tokio::test]
    async fn a_failure_that_is_not_a_collision_is_reported_as_it_is() {
        let (_dir, root) = copy_with_moved_base("A!");
        let outcome = rebase(&root, "refs/remotes/origin/missing", true).await;
        match outcome {
            RebaseOutcome::Failed { detail, conflicts } => {
                assert!(conflicts.is_empty(), "{conflicts:?}");
                assert!(!detail.detail.is_empty());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".git/rebase-merge").exists());
    }

    /// Two commits of the copy collide with the base, one file each: the
    /// first also carries a file of its own (continued), the second holds
    /// nothing else once the base's side is kept (skipped). Both paths are
    /// reported.
    #[tokio::test]
    async fn every_collision_across_the_series_keeps_the_base_side_and_is_reported() {
        let (_dir, root) =
            copy_with_moved_base_edits(&[("a.txt", "1 owner\n"), ("b.txt", "2 owner\n")]);
        write(&root, "a.txt", "1 reviewer\n");
        write(&root, "mine.txt", "reviewer\n");
        commit(&root, "C1");
        write(&root, "b.txt", "2 reviewer\n");
        commit(&root, "C2");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(
            outcome,
            RebaseOutcome::Resolved(vec!["a.txt".to_string(), "b.txt".to_string()])
        );
        assert_eq!(read(&root, "a.txt"), "1 owner\n");
        assert_eq!(read(&root, "b.txt"), "2 owner\n");
        assert_eq!(read(&root, "mine.txt"), "reviewer\n");
        assert!(root.join("w.txt").exists());
        let b1 = ok(&root, &["rev-parse", "refs/remotes/origin/main"]);
        assert_eq!(
            ok(&root, &["rev-parse", "HEAD~1"]),
            b1,
            "one commit replayed on B1: C2 had nothing left and was skipped"
        );
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".git/rebase-merge").exists());
    }

    /// One commit changes README in two places: the first line, which the
    /// owner accepted with an edit ("A!"), and the last line, which the
    /// base did not touch. The base wins at the collision; the commit's
    /// other hunk stays.
    #[tokio::test]
    async fn a_collision_is_resolved_by_hunk_and_the_commits_other_edits_in_the_file_stay() {
        let (_dir, root) =
            copy_with_moved_base_edits(&[("README.md", "A!\nb\nc\nd\ne\nf\ng\nh\n")]);
        // B0's README must be long enough for two separate hunks; rewrite
        // B0's content on the copy first, without the base's edit.
        // (copy_with_moved_base_edits wrote "a/b/c"; the base's README has
        // the extra lines, so give the copy the same long file at B0.)
        ok(&root, &["checkout", "-q", "-b", "b0-long"]);
        write(&root, "README.md", "a\nb\nc\nd\ne\nf\ng\nh\n");
        commit(&root, "B0 long");
        let b0 = ok(&root, &["rev-parse", "HEAD"]);
        // The base branch: the same long file with the first line edited.
        ok(&root, &["checkout", "-q", "-b", "base-long"]);
        write(&root, "README.md", "A!\nb\nc\nd\ne\nf\ng\nh\n");
        write(&root, "w.txt", "owner\n");
        let b1 = commit(&root, "B1 long");
        ok(&root, &["update-ref", "refs/remotes/origin/main", &b1]);
        // The copy: from B0 long, first line "A" (collides) and last line
        // "H" (does not).
        ok(&root, &["checkout", "-q", &b0]);
        ok(&root, &["checkout", "-q", "-B", "shared"]);
        write(&root, "README.md", "A\nb\nc\nd\ne\nf\ng\nH\n");
        write(&root, "mine.txt", "reviewer\n");
        commit(&root, "C1");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        assert_eq!(
            outcome,
            RebaseOutcome::Resolved(vec!["README.md".to_string()])
        );
        assert_eq!(
            read(&root, "README.md"),
            "A!\nb\nc\nd\ne\nf\ng\nH\n",
            "the base's first line, the commit's last line"
        );
        assert_eq!(read(&root, "mine.txt"), "reviewer\n");
        assert!(root.join("w.txt").exists());
        assert_eq!(ok(&root, &["rev-parse", "HEAD~1"]), b1);
        assert_eq!(
            ok(&root, &["status", "--porcelain"]),
            "",
            "nothing untracked in the tree"
        );
        assert!(!root.join(".git/rebase-merge").exists());
    }

    /// Names git would display quoted or escaped, and one with a leading
    /// space, collide and are resolved: the paths reach the index
    /// commands literally.
    #[tokio::test]
    async fn collided_paths_are_read_literally_whatever_their_name() {
        // A pathspec would read `:colon.txt` as magic and `a*.txt` as a
        // glob; `a!.txt` sorts before `a*.txt`, collides too, and is
        // executable, so a glob would hand `a*.txt` the wrong mode.
        let names = [
            "caf\u{e9} notes.txt",
            " leading.txt",
            "quote\"d.txt",
            ":colon.txt",
            "a*.txt",
            "a!.txt",
        ];
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        ok(&root, &["init", "-q", "-b", "shared"]);
        for name in names {
            write(&root, name, "1\n2\n3\n");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.join("a!.txt"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        commit(&root, "B0");
        ok(&root, &["checkout", "-q", "-b", "scratch"]);
        for name in names {
            write(&root, name, "owner\n2\n3\n");
        }
        let b1 = commit(&root, "B1");
        ok(&root, &["update-ref", "refs/remotes/origin/main", &b1]);
        ok(&root, &["checkout", "-q", "shared"]);
        ok(&root, &["branch", "-q", "-D", "scratch"]);
        for name in names {
            write(&root, name, "reviewer\n2\nmine\n");
        }
        commit(&root, "C1");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        let mut expected = names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        let resolved = match outcome {
            RebaseOutcome::Resolved(mut resolved) => {
                resolved.sort();
                resolved
            }
            other => panic!("{other:?}"),
        };
        expected.sort();
        assert_eq!(resolved, expected);
        for name in names {
            assert_eq!(read(&root, name), "owner\n2\nmine\n", "{name}");
        }
        // Each resolved file keeps its own mode: the glob's neighbour is
        // executable, the globbed name is not.
        let modes: std::collections::HashMap<String, String> = ok(&root, &["ls-files", "-s"])
            .lines()
            .filter_map(|line| {
                let (meta, path) = line.split_once('\t')?;
                Some((path.to_string(), meta.split(' ').next()?.to_string()))
            })
            .collect();
        assert_eq!(modes.get("a*.txt").map(String::as_str), Some("100644"));
        assert_eq!(modes.get("a!.txt").map(String::as_str), Some("100755"));
        assert_eq!(modes.get(":colon.txt").map(String::as_str), Some("100644"));
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".git/rebase-merge").exists());
    }

    /// A stage that cannot be merged as text fails the update, and no
    /// temporary stage file is left in the tree for the copy's next save.
    #[tokio::test]
    async fn a_failed_resolution_leaves_no_stage_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        ok(&root, &["init", "-q", "-b", "shared"]);
        write(&root, "text.txt", "1\n2\n3\n");
        std::fs::write(root.join("blob.bin"), [0u8, 1, 2, 3, 0, 255]).unwrap();
        commit(&root, "B0");
        ok(&root, &["checkout", "-q", "-b", "scratch"]);
        write(&root, "text.txt", "owner\n2\n3\n");
        std::fs::write(root.join("blob.bin"), [0u8, 9, 9, 9, 0, 255]).unwrap();
        let b1 = commit(&root, "B1");
        ok(&root, &["update-ref", "refs/remotes/origin/main", &b1]);
        ok(&root, &["checkout", "-q", "shared"]);
        ok(&root, &["branch", "-q", "-D", "scratch"]);
        write(&root, "text.txt", "reviewer\n2\n3\n");
        std::fs::write(root.join("blob.bin"), [0u8, 7, 7, 7, 0, 255]).unwrap();
        let before = commit(&root, "C1");

        let outcome = rebase(&root, "refs/remotes/origin/main", true).await;
        match outcome {
            RebaseOutcome::Failed { detail, conflicts } => {
                assert!(detail.detail.contains("blob.bin"), "{detail}");
                assert!(conflicts.contains(&"blob.bin".to_string()), "{conflicts:?}");
            }
            other => panic!("{other:?}"),
        }
        ok(&root, &["rebase", "--abort"]);
        assert_eq!(ok(&root, &["rev-parse", "HEAD"]), before);
        assert_eq!(
            ok(&root, &["status", "--porcelain"]),
            "",
            "nothing untracked in the tree"
        );
    }

    /// Without `favor_base` (an ordinary workspace's update) the first
    /// collision fails the update; after the caller's abort the branch is
    /// where it was.
    #[tokio::test]
    async fn without_favor_base_a_collision_fails_and_the_branch_stays() {
        let (_dir, root) =
            copy_with_moved_base_edits(&[("a.txt", "1 owner\n"), ("b.txt", "2 owner\n")]);
        write(&root, "a.txt", "1 reviewer\n");
        write(&root, "mine.txt", "reviewer\n");
        commit(&root, "C1");
        write(&root, "b.txt", "2 reviewer\n");
        let before = commit(&root, "C2");

        let outcome = rebase(&root, "refs/remotes/origin/main", false).await;
        match outcome {
            RebaseOutcome::Failed { detail, conflicts } => {
                assert_eq!(conflicts, vec!["a.txt".to_string()]);
                assert!(!detail.detail.is_empty());
            }
            other => panic!("{other:?}"),
        }
        // The update op aborts the sequencer on a failure.
        ok(&root, &["rebase", "--abort"]);
        assert_eq!(
            ok(&root, &["rev-parse", "HEAD"]),
            before,
            "the branch is untouched"
        );
        assert_eq!(read(&root, "a.txt"), "1 reviewer\n");
        assert_eq!(ok(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".git/rebase-merge").exists());
    }
}
