//! Immutable flush capture and receipt ref mechanics. Receipt admission,
//! limits and retry policy remain with the dispatcher.
use super::{failed, validate_relay, MemberGit, MemberOperation};
use crate::native::{FailureEffect, FailureKind, NativeCancellation, NativeGitError};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub struct MemberObjectOwnership {
    pub workspace: PathBuf,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}
#[derive(Clone)]
pub struct MemberFlushCapture {
    pub repository: PathBuf,
    pub branch: String,
    pub transport_url: String,
    pub private_state: PathBuf,
    pub excluded_roots: Vec<String>,
    /// Paths that must not be silently omitted by ignore rules when absent from HEAD.
    pub required_capture_roots: Vec<String>,
    pub ownership: MemberObjectOwnership,
    pub pack_limit: u64,
}
#[derive(Debug)]
pub struct MemberFlushSnapshot {
    pub branch_oid: String,
    pub local_branch_oid: String,
    pub local_head_oid: String,
    pub head_ref: Option<String>,
    pub sequencers: Vec<String>,
    pub snapshot_oid: String,
    pub dirty_paths: Vec<String>,
    pub pack_bytes: u64,
}
#[derive(Debug)]
pub enum MemberFlushOutcome {
    Captured(MemberFlushSnapshot),
    IgnoredPaths(Vec<String>),
}
#[derive(Debug)]
pub struct MemberFileInventory {
    pub deleted: Vec<String>,
    pub paths: Vec<String>,
}
#[derive(Clone)]
pub struct MemberFlushPush {
    pub repository: PathBuf,
    pub transport_url: String,
    pub workspace_id: uuid::Uuid,
    pub flush_id: uuid::Uuid,
    pub snapshot_oid: String,
    pub branch_oid: String,
}
struct ScratchIndex(PathBuf);
impl Drop for ScratchIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(PathBuf::from(format!("{}.lock", self.0.display())));
    }
}
fn paths(text: String) -> Result<Vec<String>, NativeGitError> {
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(failed("Incomplete Git path inventory"));
    }
    Ok(text.split_terminator('\0').map(str::to_owned).collect())
}
impl MemberGit {
    pub async fn file_inventory(&self, root: &Path) -> Result<MemberFileInventory, NativeGitError> {
        Ok(MemberFileInventory {
            deleted: paths(
                self.read(
                    root,
                    "inventory flush files",
                    &["ls-files", "--deleted", "-z"],
                )
                .await?,
            )?,
            paths: paths(
                self.read(
                    root,
                    "inventory flush files",
                    &["ls-files", "-co", "--exclude-standard", "-z"],
                )
                .await?,
            )?,
        })
    }
    pub async fn capture_flush(
        &self,
        request: &MemberFlushCapture,
    ) -> Result<MemberFlushOutcome, NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("capture member flush", async move {
            let result = member.capture_flush_inner(&request).await;
            // This task retains custody through cancellation, including nofollow
            // ownership handoff after any attempted object-store writes.
            let repair = match request.ownership.uid {
                Some(uid) if unsafe { libc::geteuid() } == 0 => hand_over_git_object_directories(&request.ownership.workspace, &request.repository, uid, request.ownership.gid),
                _ => Ok(()),
            };
            match (result, repair) {
                (result, Ok(())) => result,
                (Ok(_), Err(error)) => Err(NativeGitError { operation: "capture member flush", target: Some(request.repository.display().to_string()), kind: FailureKind::Io, effect: FailureEffect::Partial, detail: format!("Snapshot objects were written; object directory ownership handoff failed: {error}") }),
                (Err(mut error), Err(repair)) => { error.detail.push_str(&format!("; object directory ownership handoff failed: {repair}")); Err(error) },
            }
        }).await?
    }
    async fn capture_flush_inner(
        &self,
        request: &MemberFlushCapture,
    ) -> Result<MemberFlushOutcome, NativeGitError> {
        let name = "capture member flush";
        let root = &request.repository;
        let mut git = MemberOperation::new(self, root, name);
        git.transport = Some(&request.transport_url);
        let result = async {
            validate_relay(&request.transport_url, name)?;
            self.check_branch(root, &request.branch, name).await?;
            if request
                .excluded_roots
                .iter()
                .chain(&request.required_capture_roots)
                .any(|path| {
                    path.is_empty()
                        || !Path::new(path)
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_)))
                })
            {
                return Err(failed("Invalid capture repository path"));
            }
            git.check_relay_unchanged(&request.transport_url).await?;
            let branch_ref = format!("refs/heads/{}", request.branch);
            let local_branch_oid = git.read(&["rev-parse", "--verify", &branch_ref]).await?;
            let local_head_oid = self.head_commit(root).await?;
            let head_ref = self
                .current_branch(root)
                .await?
                .map(|branch| format!("refs/heads/{branch}"));
            let sequencers = self.sequencers(root).await?;
            let index = ScratchIndex(
                request
                    .private_state
                    .join(format!("flush-index-{}", uuid::Uuid::new_v4())),
            );
            git.run(&["read-tree", &local_head_oid], Some(&index.0), None)
                .await?;
            if !request.required_capture_roots.is_empty() {
                // Use the HEAD-seeded capture index. Force-staging an ignored
                // addition in the real index must not make it silently disappear.
                let mut args = vec![
                    "--literal-pathspecs",
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "-z",
                    "--",
                ];
                args.extend(request.required_capture_roots.iter().map(String::as_str));
                let ignored = paths(git.run(&args, Some(&index.0), None).await?)?;
                if !ignored.is_empty() {
                    return Ok(MemberFlushOutcome::IgnoredPaths(
                        ignored.into_iter().take(32).collect(),
                    ));
                }
            }
            git.run(&["add", "-A", "--", "."], Some(&index.0), None)
                .await?;
            for excluded in &request.excluded_roots {
                git.run(
                    &[
                        "reset",
                        "-q",
                        &local_head_oid,
                        "--",
                        &format!(":(literal){excluded}"),
                    ],
                    Some(&index.0),
                    None,
                )
                .await?;
            }
            let tree = git.run(&["write-tree"], Some(&index.0), None).await?;
            let head_tree = git
                .read(&["rev-parse", &format!("{local_head_oid}^{{tree}}")])
                .await?;
            let snapshot_oid = if tree == head_tree {
                local_head_oid.clone()
            } else {
                git.read(&[
                    "commit-tree",
                    &tree,
                    "-p",
                    &local_head_oid,
                    "-m",
                    "G5 workspace flush",
                ])
                .await?
            };
            let dirty_paths = paths(
                git.read(&["diff", "--name-only", "-z", &local_head_oid, &snapshot_oid])
                    .await?,
            )?;
            let listing = git
                .read(&[
                    "ls-remote",
                    "--refs",
                    "--",
                    &request.transport_url,
                    &branch_ref,
                ])
                .await?;
            let mut matches = listing
                .lines()
                .filter_map(|line| line.split_once('\t'))
                .filter(|(_, name)| *name == branch_ref);
            let branch_oid = matches
                .next()
                .map(|(oid, _)| oid.to_owned())
                .ok_or_else(|| failed("WORKSPACE_BRANCH_MISSING"))?;
            if matches.next().is_some()
                || [&branch_oid, &local_branch_oid, &snapshot_oid]
                    .iter()
                    .any(|oid| !crate::native::observations::full_oid(oid))
            {
                return Err(failed("Invalid flush object identity"));
            }
            if self
                .read_optional(
                    root,
                    name,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("{branch_oid}^{{commit}}"),
                    ],
                )
                .await?
                .is_none()
            {
                git.read(&[
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--no-write-fetch-head",
                    "--no-auto-maintenance",
                    "--recurse-submodules=no",
                    "--",
                    &request.transport_url,
                    &branch_ref,
                ])
                .await?;
            }
            let pack_bytes = self
                .flush_pack_bytes(
                    root,
                    &snapshot_oid,
                    &local_branch_oid,
                    &branch_oid,
                    request.pack_limit,
                )
                .await?;
            Ok(MemberFlushOutcome::Captured(MemberFlushSnapshot {
                branch_oid,
                local_branch_oid,
                local_head_oid,
                head_ref,
                sequencers,
                snapshot_oid,
                dirty_paths,
                pack_bytes,
            }))
        }
        .await;
        result.map_err(|error| git.failure(error))
    }
    async fn flush_pack_bytes(
        &self,
        root: &Path,
        snapshot: &str,
        branch: &str,
        server: &str,
        limit: u64,
    ) -> Result<u64, NativeGitError> {
        let mut packer = self.clone();
        let cancellation = NativeCancellation::default();
        packer.git.cancellation = cancellation.clone();
        let mut command = packer.command(root, "measure flush pack");
        command
            .args(["pack-objects", "--stdout", "--revs"])
            .input(format!("{snapshot}\n{branch}\n^{server}\n").into_bytes());
        command.read_only();
        let mut stream = command.execute_stream(root).await?;
        let result = tokio::select! {
            _ = self.git.cancellation.cancelled() => Err(NativeGitError { operation: "measure flush pack", target: Some(root.display().to_string()), kind: FailureKind::Cancelled, effect: FailureEffect::Refused, detail: "Flush pack measurement cancelled".into() }),
            result = async {
                let mut buffer = [0u8; 65536]; let mut bytes = 0u64;
                loop {
                    let read = stream.read(&mut buffer).await.map_err(|error| failed(format!("Cannot measure flush pack: {error}")))?;
                    if read == 0 { break; }
                    bytes = bytes.saturating_add(read as u64);
                    if bytes > limit { break; }
                }
                Ok(bytes)
            } => result,
        };
        drop(stream);
        cancellation.cancel();
        cancellation.wait_idle().await;
        result
    }
    /// Each receipt leaf is immutable. The pair may succeed separately; a
    /// failure reports the confirmed leaves and never implies atomicity.
    pub async fn push_flush(&self, request: &MemberFlushPush) -> Result<(), NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("push member flush", async move {
            member.push_flush_inner(&request).await
        })
        .await?
    }
    async fn push_flush_inner(&self, request: &MemberFlushPush) -> Result<(), NativeGitError> {
        let name = "push member flush";
        let root = &request.repository;
        let mut git = MemberOperation::new(self, root, name);
        git.transport = Some(&request.transport_url);
        let result = async {
            validate_relay(&request.transport_url, name)?;
            for oid in [&request.snapshot_oid, &request.branch_oid] {
                if !crate::native::observations::full_oid(oid) {
                    return Err(failed(
                        "Flush publication requires full immutable object IDs",
                    ));
                }
            }
            let transport = self
                .git
                .transport_destination(Some(root), &request.transport_url, name)
                .await?;
            if transport != request.transport_url {
                return Err(failed("Member transport lease was rewritten"));
            }
            self.git
                .check_expansion(Some(root), &request.transport_url, &transport, name)
                .await?;
            self.git
                .check_push_expansion(root, &transport, name)
                .await?;
            let prefix = format!(
                "refs/g5/flush/{}/{}",
                request.workspace_id, request.flush_id
            );
            let targets = [format!("{prefix}/snapshot"), format!("{prefix}/branch")];
            let sources = [&request.snapshot_oid, &request.branch_oid];
            // Observe each leaf so an identical replay is harmless and a
            // different immutable value refuses before any requested update.
            let listing = git
                .read(&[
                    "ls-remote",
                    "--refs",
                    "--",
                    &transport,
                    &targets[0],
                    &targets[1],
                ])
                .await?;
            let mut command = self.command(root, name);
            command.relay(&transport);
            command.args([
                "push",
                "--porcelain",
                "--no-follow-tags",
                "--recurse-submodules=no",
            ]);
            for (target, source) in targets.iter().zip(sources) {
                let existing: Vec<_> = listing
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .filter(|(_, name)| name == target)
                    .collect();
                if existing.len() > 1 || existing.first().is_some_and(|(oid, _)| *oid != source) {
                    return Err(failed(format!(
                        "Immutable flush ref already has a different value: {target}"
                    )));
                }
                command.arg(format!(
                    "--force-with-lease={target}:{}",
                    existing.first().map(|(oid, _)| *oid).unwrap_or("")
                ));
            }
            command.arg("--").arg(&transport);
            for (target, source) in targets.iter().zip(sources) {
                command.arg(format!("{source}:{target}"));
            }
            let output = command.execute_captured().await?;
            let text = output.stdout_text();
            let mut confirmed = Vec::new();
            let mut rejected = Vec::new();
            for line in text
                .split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
            {
                let fields: Vec<_> = line.trim_end_matches('\n').split('\t').collect();
                if fields.len() < 3 {
                    continue;
                }
                let Some((_, target)) = fields[1].rsplit_once(':') else {
                    continue;
                };
                if !targets.iter().any(|name| name == target) {
                    continue;
                }
                if matches!(fields[0], "=" | " " | "+" | "*") {
                    confirmed.push(target.to_owned());
                } else if fields[0] == "!" {
                    rejected.push(format!("{target}: {}", output.redact(fields[2])));
                }
            }
            confirmed.sort();
            confirmed.dedup();
            if output.completed_successfully() && confirmed.len() == 2 {
                return Ok(());
            }
            let mut error = output.failure(
                name,
                &crate::GitAuth::None,
                FailureKind::Exit,
                if !confirmed.is_empty() {
                    FailureEffect::Partial
                } else if rejected.len() == 2 {
                    FailureEffect::Refused
                } else {
                    FailureEffect::Unknown
                },
            );
            error.detail = format!(
                "Confirmed receipt refs: {}; rejected: {}; {}",
                confirmed.join(", "),
                rejected.join(", "),
                error.detail
            );
            Err(error)
        }
        .await;
        result.map_err(|error| git.failure(error))
    }
}
fn hand_over_git_object_directories(
    workspace: &Path,
    root: &Path,
    uid: u32,
    gid: Option<u32>,
) -> std::io::Result<()> {
    use std::ffi::{CString, OsStr};
    use std::fs::{File, OpenOptions};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    fn open(parent: &File, name: &OsStr) -> std::io::Result<File> {
        let name = CString::new(name.as_bytes())?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn own(directory: &File, uid: u32, gid: Option<u32>) -> std::io::Result<()> {
        let metadata = directory.metadata()?;
        if metadata.uid() == uid && gid.is_none_or(|gid| metadata.gid() == gid) {
            return Ok(());
        }
        if unsafe { libc::fchown(directory.as_raw_fd(), uid, gid.unwrap_or(u32::MAX)) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(workspace)?;
    let relative = root
        .strip_prefix(workspace)
        .map_err(std::io::Error::other)?;
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(std::io::Error::other("invalid repository root"));
        };
        directory = open(&directory, name)?;
    }
    let git = open(&directory, OsStr::new(".git"))?;
    let objects = open(&git, OsStr::new("objects"))?;
    own(&objects, uid, gid)?;
    // Git's object-store directories have fixed names. Never walk user paths
    // or follow links: live forks retain terminal sessions, so a path-based
    // recursive chown would race those writers. No object-file scan is needed.
    for name in ["info".to_owned(), "pack".to_owned()]
        .into_iter()
        .chain((0..256).map(|prefix| format!("{prefix:02x}")))
    {
        let child = match open(&objects, OsStr::new(&name)) {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        own(&child, uid, gid)?;
        if name == "info" {
            match open(&child, OsStr::new("commit-graphs")) {
                Ok(graphs) => own(&graphs, uid, gid)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod ownership_tests {
    use super::hand_over_git_object_directories;
    use std::os::unix::fs::{symlink, MetadataExt};

    #[test]
    fn object_directory_handoff_refuses_symlinks_at_every_level() {
        if unsafe { libc::geteuid() } != 0 {
            return;
        }
        for (link, suffix) in [
            ("code", ".git/objects/aa"),
            ("code/.git", "objects/aa"),
            ("code/.git/objects", "aa"),
            ("code/.git/objects/aa", ""),
            ("code/.git/objects/info/commit-graphs", ""),
        ] {
            let fixture = tempfile::tempdir().unwrap();
            let workspace = fixture.path().join("work");
            let outside = fixture.path().join("outside");
            std::fs::create_dir_all(outside.join(suffix)).unwrap();
            let path = workspace.join(link);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(&outside, &path).unwrap();
            assert!(
                hand_over_git_object_directories(
                    &workspace,
                    &workspace.join("code"),
                    1000,
                    Some(1000)
                )
                .is_err(),
                "{link}"
            );
            assert_eq!(std::fs::metadata(&outside).unwrap().uid(), 0, "{link}");
        }
    }
}
