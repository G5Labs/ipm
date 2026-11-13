//! Complete checkpoint operations, retaining selected-path index and ref proof.
use super::{failed, MemberGit, MemberOperation};
use crate::native::{Context, NativeGitError};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) struct CheckpointContext<'a> {
    pub(super) operation: MemberOperation<'a>,
}
impl CheckpointContext<'_> {
    async fn git(&self, root: &Path, args: &[&str]) -> Result<String, NativeGitError> {
        debug_assert_eq!(root, self.operation.root);
        self.operation.read(args).await
    }
    async fn git_with_index(
        &self,
        root: &Path,
        index: &Path,
        args: &[&str],
    ) -> Result<String, NativeGitError> {
        debug_assert_eq!(root, self.operation.root);
        self.operation.run(args, Some(index), None).await
    }
    fn identity(&self) -> (Option<u32>, Option<u32>) {
        match &self.operation.member.git.context {
            Context::Member(environment) => (environment.uid, environment.gid),
            _ => unreachable!("member context"),
        }
    }
}

fn handover_file(file: &std::fs::File, uid: Option<u32>, gid: Option<u32>) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if uid.is_none() && gid.is_none() {
        return Ok(());
    }
    // The open descriptor, not a mutable checkout path, identifies this file.
    if unsafe {
        libc::fchown(
            file.as_raw_fd(),
            uid.unwrap_or(u32::MAX),
            gid.unwrap_or(u32::MAX),
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

struct ScratchIndex(PathBuf);

impl Drop for ScratchIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}.lock", self.0.display()));
    }
}

/// Exclude native Git index writers while installing the selected commit's
/// entries. Git itself uses this lock path for add/switch/commit.
pub(super) struct SelectedIndexLock {
    path: PathBuf,
    index: PathBuf,
    file: std::fs::File,
    installed: bool,
}

impl SelectedIndexLock {
    pub(super) fn acquire(index: &Path) -> std::io::Result<Self> {
        let path = PathBuf::from(format!("{}.lock", index.display()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            index: index.to_path_buf(),
            file,
            installed: false,
        })
    }

    pub(super) fn install(&mut self, source: &Path) -> std::io::Result<()> {
        std::io::copy(&mut std::fs::File::open(source)?, &mut self.file)?;
        self.file
            .set_permissions(std::fs::metadata(source)?.permissions())?;
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(source)?;
        handover_file(&self.file, Some(metadata.uid()), Some(metadata.gid()))?;
        self.file.sync_all()?;
        std::fs::rename(&self.path, &self.index)?;
        self.installed = true;
        Ok(())
    }
}

impl Drop for SelectedIndexLock {
    fn drop(&mut self) {
        if !self.installed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Build a selected-path tree in a private index. The normal index, including
/// unrelated staged work, is preserved; a concurrent file edit after staging
/// cannot change the blob recorded in this commit.
pub(super) async fn commit_selected_paths(
    ctx: &CheckpointContext<'_>,
    root: &Path,
    branch: &str,
    paths: &[String],
    expected_blobs: Option<&BTreeMap<String, String>>,
    message: &str,
) -> Result<(bool, String, usize), NativeGitError> {
    let index_path = ctx.git(root, &["rev-parse", "--git-path", "index"]).await?;
    let index_path = PathBuf::from(index_path);
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        root.join(index_path)
    };
    let index =
        ScratchIndex(index_path.with_file_name(format!("index.g5-{}", uuid::Uuid::new_v4())));
    let head = ctx.git(root, &["rev-parse", "HEAD"]).await?;
    ctx.git_with_index(root, &index.0, &["read-tree", &head])
        .await?;
    let literal_paths: Vec<String> = paths
        .iter()
        .map(|path| format!(":(literal){path}"))
        .collect();
    let mut head_entries = Vec::new();
    for (path, literal) in paths.iter().zip(&literal_paths) {
        let listing = ctx
            .git_with_index(root, &index.0, &["ls-files", "-s", "-z", "--", literal])
            .await?;
        head_entries.push(selected_index_entry(&listing, path)?);
    }
    let mut add_args = vec!["add", "--all", "--force", "--"];
    add_args.extend(literal_paths.iter().map(String::as_str));
    ctx.git_with_index(root, &index.0, &add_args).await?;

    let mut staged_entries = Vec::new();
    for (path, literal) in paths.iter().zip(&literal_paths) {
        let listing = ctx
            .git_with_index(root, &index.0, &["ls-files", "-s", "-z", "--", literal])
            .await?;
        let entry = selected_index_entry(&listing, path)?;
        if let Some(expected) = expected_blobs.and_then(|blobs| blobs.get(path)) {
            // fs.write reports a hash of the bytes it wrote. Git attributes
            // may transform those bytes when `git add` creates the staged
            // blob, so compare the same raw representation instead. The
            // staged entry already has Git's filtered representation;
            // hash-object --path does not read index-only attributes.
            let worktree_hash = ctx
                .git(root, &["hash-object", "--no-filters", "--", path])
                .await?;
            if &worktree_hash != expected {
                return Err(failed(format!("FILE_CHANGED_DURING_SAVE: {path}")));
            }
        }
        staged_entries.push(entry);
    }
    let changed = ctx
        .git_with_index(root, &index.0, &["diff", "--cached", "--name-only"])
        .await?;
    let count = changed.lines().count();
    let committed_head = if count == 0 {
        head.clone()
    } else {
        let mut lock = SelectedIndexLock::acquire(&index_path)
            .map_err(|error| failed(format!("Git index is locked: {error}")))?;
        let active_branch = ctx
            .git(root, &["symbolic-ref", "-q", "--short", "HEAD"])
            .await?;
        if active_branch != branch || ctx.git(root, &["rev-parse", "HEAD"]).await? != head {
            return Err(failed("workspace branch changed before selected commit"));
        }
        let index_copy = ScratchIndex(
            index_path.with_file_name(format!("index.g5-install-{}", uuid::Uuid::new_v4())),
        );
        let mut source = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&index_path)
            .map_err(|error| failed(format!("open Git index: {error}")))?;
        let mut copy = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&index_copy.0)
            .map_err(|error| failed(format!("create Git index copy: {error}")))?;
        std::io::copy(&mut source, &mut copy)
            .map_err(|error| failed(format!("copy Git index: {error}")))?;
        copy.set_permissions(
            source
                .metadata()
                .map_err(|error| failed(error.to_string()))?
                .permissions(),
        )
        .map_err(|error| failed(error.to_string()))?;
        let (uid, gid) = ctx.identity();
        handover_file(&copy, uid, gid)
            .map_err(|error| failed(format!("prepare Git index: {error}")))?;
        drop(copy);
        // Keep staged user content for a selected path when it differs from
        // the old HEAD. Only entries still at the old version are advanced.
        for (((path, literal), entry), head_entry) in paths
            .iter()
            .zip(&literal_paths)
            .zip(&staged_entries)
            .zip(&head_entries)
        {
            let current = selected_index_entry(
                &ctx.git_with_index(
                    root,
                    &index_copy.0,
                    &["ls-files", "-s", "-z", "--", literal],
                )
                .await?,
                path,
            )?;
            if current == *entry || current != *head_entry {
                continue;
            }
            match entry {
                Some((mode, oid)) => {
                    let cacheinfo = format!("{mode},{oid},{path}");
                    ctx.git_with_index(
                        root,
                        &index_copy.0,
                        &["update-index", "--add", "--cacheinfo", &cacheinfo],
                    )
                    .await?;
                }
                None => {
                    ctx.git_with_index(
                        root,
                        &index_copy.0,
                        &["update-index", "--force-remove", "--", path],
                    )
                    .await?;
                }
            }
        }
        let tree = ctx.git_with_index(root, &index.0, &["write-tree"]).await?;
        let commit = ctx
            .git(root, &["commit-tree", &tree, "-p", &head, "-m", message])
            .await?;
        let branch_ref = format!("refs/heads/{branch}");
        ctx.git(root, &["update-ref", &branch_ref, &commit, &head])
            .await?;
        if let Err(error) = lock.install(&index_copy.0) {
            let rollback = ctx
                .git(root, &["update-ref", &branch_ref, &head, &commit])
                .await;
            return Err(failed(format!(
                "install Git index: {error}; ref rollback: {}",
                if rollback.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                }
            )));
        }
        commit
    };
    Ok((count != 0, committed_head, count))
}

pub(super) fn selected_index_entry(
    listing: &str,
    selected_path: &str,
) -> Result<Option<(String, String)>, NativeGitError> {
    if listing.is_empty() {
        return Ok(None);
    }
    let record = listing
        .strip_suffix('\0')
        .ok_or_else(|| failed("invalid selected Git index entry"))?;
    let (header, actual_path) = record
        .split_once('\t')
        .ok_or_else(|| failed("invalid selected Git index entry"))?;
    if actual_path != selected_path || actual_path.contains('\0') {
        return Err(failed("selected Git index entry names a different path"));
    }
    let mut parts = header.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(mode), Some(oid), Some("0"), None) => Ok(Some((mode.into(), oid.into()))),
        (_, _, Some(_), None) => Err(failed("selected Git index entry is unmerged")),
        _ => Err(failed("invalid selected Git index entry")),
    }
}

#[derive(Clone)]
pub struct MemberCheckpoint {
    pub repository: PathBuf,
    pub branch: String,
    pub message: String,
    pub paths: Option<Vec<String>>,
    pub expected_blobs: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemberCheckpointResult {
    pub committed: bool,
    pub head: String,
    pub files: usize,
}

impl MemberGit {
    pub async fn checkpoint(
        &self,
        request: &MemberCheckpoint,
    ) -> Result<MemberCheckpointResult, NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("checkpoint member checkout", async move {
            member.checkpoint_inner(&request).await
        })
        .await?
    }

    async fn checkpoint_inner(
        &self,
        request: &MemberCheckpoint,
    ) -> Result<MemberCheckpointResult, NativeGitError> {
        let name = "checkpoint member checkout";
        let ctx = CheckpointContext {
            operation: MemberOperation::new(self, &request.repository, name),
        };
        let root = &request.repository;
        let result = async {
            self.check_branch(root, &request.branch, name).await?;
            if self.current_branch(root).await?.as_deref() != Some(&request.branch) {
                return Err(failed("Workspace branch changed before checkpoint"));
            }
            if let Some(paths) = &request.paths {
                if paths.is_empty()
                    || paths.iter().any(|path| {
                        path.is_empty()
                            || !Path::new(path)
                                .components()
                                .all(|part| matches!(part, std::path::Component::Normal(_)))
                    })
                {
                    return Err(failed(
                        "Selected checkpoint requires nonempty repository-relative paths",
                    ));
                }
            }
            let conflicts = self.conflicts(root, request.paths.as_deref()).await?;
            if !conflicts.is_empty() {
                return Err(failed(format!("UNMERGED_PATHS: {}", conflicts.join(", "))));
            }
            if let Some(paths) = &request.paths {
                let (committed, head, files) = commit_selected_paths(
                    &ctx,
                    root,
                    &request.branch,
                    paths,
                    request.expected_blobs.as_ref(),
                    &request.message,
                )
                .await?;
                return Ok(MemberCheckpointResult {
                    committed,
                    head,
                    files,
                });
            }
            let dirty = self.status(root).await?;
            if dirty.is_empty() {
                return Ok(MemberCheckpointResult {
                    committed: false,
                    head: self.head_commit(root).await?,
                    files: 0,
                });
            }
            let index_path = PathBuf::from(
                ctx.operation
                    .read(&["rev-parse", "--git-path", "index"])
                    .await?,
            );
            let index_path = if index_path.is_absolute() {
                index_path
            } else {
                root.join(index_path)
            };
            let mut lock = SelectedIndexLock::acquire(&index_path)
                .map_err(|error| failed(format!("Git index is locked: {error}")))?;
            let head = self.head_commit(root).await?;
            if self.current_branch(root).await?.as_deref() != Some(&request.branch) {
                return Err(failed("Workspace branch changed before checkpoint"));
            }
            let index = ScratchIndex(
                index_path.with_file_name(format!("index.g5-{}", uuid::Uuid::new_v4())),
            );
            let mut source = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&index_path)
                .map_err(|error| failed(format!("open Git index: {error}")))?;
            let mut copy = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&index.0)
                .map_err(|error| failed(format!("create Git index copy: {error}")))?;
            std::io::copy(&mut source, &mut copy)
                .map_err(|error| failed(format!("copy Git index: {error}")))?;
            copy.set_permissions(
                source
                    .metadata()
                    .map_err(|error| failed(error.to_string()))?
                    .permissions(),
            )
            .map_err(|error| failed(error.to_string()))?;
            let (uid, gid) = ctx.identity();
            handover_file(&copy, uid, gid)
                .map_err(|error| failed(format!("prepare Git index: {error}")))?;
            drop(copy);
            ctx.operation
                .run(&["add", "--all"], Some(&index.0), None)
                .await?;
            let staged = ctx
                .operation
                .run(
                    &["diff", "--cached", "--name-only", "-z", &head],
                    Some(&index.0),
                    None,
                )
                .await?;
            let committed = !staged.is_empty();
            let commit = if committed {
                let tree = ctx
                    .operation
                    .run(&["write-tree"], Some(&index.0), None)
                    .await?;
                let commit = ctx
                    .operation
                    .read(&["commit-tree", &tree, "-p", &head, "-m", &request.message])
                    .await?;
                if self.current_branch(root).await?.as_deref() != Some(&request.branch) {
                    return Err(failed("Workspace branch changed before checkpoint"));
                }
                ctx.operation
                    .read(&[
                        "update-ref",
                        &format!("refs/heads/{}", request.branch),
                        &commit,
                        &head,
                    ])
                    .await?;
                commit
            } else {
                head.clone()
            };
            if let Err(error) = lock.install(&index.0) {
                let rollback = if committed {
                    ctx.operation
                        .read(&[
                            "update-ref",
                            &format!("refs/heads/{}", request.branch),
                            &head,
                            &commit,
                        ])
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                };
                return Err(failed(format!(
                    "install Git index: {error}; ref rollback: {}",
                    if rollback.is_ok() {
                        "succeeded"
                    } else {
                        "failed"
                    }
                )));
            }
            Ok(MemberCheckpointResult {
                committed,
                head: commit,
                files: if committed { dirty.len() } else { 0 },
            })
        }
        .await;
        result.map_err(|error| ctx.operation.failure(error))
    }
}
