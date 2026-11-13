//! Filesystem access starts at pinned member-root descriptors. No caller path is
//! passed to an absolute-path filesystem operation, and every opened component
//! refuses symlinks. The mutex serializes RPC preconditions with RPC mutations;
//! native owner processes retain the shared-tree concurrency described by v9.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Mutex;

pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
/// The most entries one `fs.tree` reply carries; a caller's `max_entries`
/// is clamped to it. Past the cap the walk stops and reports `truncated`.
pub const MAX_TREE_ENTRIES: u64 = 50_000;
/// The most bytes of entries one `fs.tree` reply is budgeted at
/// (`file_tree::entry_bytes`); a caller's `max_bytes` is clamped to it.
/// Callers pass their own transport's reply limit, which is smaller.
pub const MAX_TREE_BYTES: u64 = 16 * 1024 * 1024;
/// The most directories one native watch subscription covers, and how deep.
const MAX_WATCH_DIRECTORIES: usize = 10_000;
const MAX_WATCH_DEPTH: usize = 64;
const STAGING_PREFIX: &str = ".g5-dispatch-tmp-";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ObjectFormat {
    #[default]
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepositoryRoot {
    pub repo_id: String,
    pub root: PathBuf,
    #[serde(default)]
    pub object_format: ObjectFormat,
}

#[derive(Debug, Clone)]
pub struct FileError {
    pub code: &'static str,
    pub message: String,
    pub data: Value,
}

impl FileError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: Value::Null,
        }
    }

    pub fn http_status(&self) -> u16 {
        match self.code {
            "FILE_MISSING" => 404,
            "FILE_EXISTS" | "FILE_VERSION_CONFLICT" | "DIRECTORY_NOT_EMPTY" => 409,
            "PAYLOAD_TOO_LARGE" => 413,
            "PATH_FORBIDDEN" => 403,
            "IO_ERROR" => 500,
            _ => 400,
        }
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FileError {}

type Result<T> = std::result::Result<T, FileError>;

struct Member {
    root: File,
    format: ObjectFormat,
}

pub struct FileService {
    members: BTreeMap<String, Member>,
    excludes: Vec<String>,
    mutation: Mutex<()>,
    /// The module identity that owns published files, when the host
    /// provisioned one: the service runs as the dispatcher, so a staged
    /// replacement inode would otherwise hand an editor-saved file back to
    /// root and the next module could not open it for writing.
    owner: Option<(u32, Option<u32>)>,
}

impl FileService {
    pub fn new(repositories: Vec<RepositoryRoot>, runtime_excludes: Vec<String>) -> Result<Self> {
        let mut members = BTreeMap::new();
        for repository in repositories {
            if repository.repo_id.is_empty() || members.contains_key(&repository.repo_id) {
                return Err(invalid("member repository IDs must be nonempty and unique"));
            }
            let path = CString::new(repository.root.as_os_str().as_encoded_bytes())
                .map_err(|_| invalid("invalid configured repository root"))?;
            // Configured roots are trusted, but the final root itself may not be a symlink.
            let root = owned_fd(unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?;
            members.insert(
                repository.repo_id,
                Member {
                    root,
                    format: repository.object_format,
                },
            );
        }
        let mut excludes = Vec::new();
        for exclude in runtime_excludes {
            let exclude = exclude.trim_end_matches('/');
            validate_components(exclude, false)?;
            excludes.push(exclude.to_owned());
        }
        Ok(Self {
            members,
            excludes,
            mutation: Mutex::new(()),
            owner: None,
        })
    }

    /// Publish files as `uid`/`gid` (the module identity) instead of the
    /// dispatcher's own identity. `None` uid keeps the dispatcher's.
    pub fn with_owner(mut self, uid: Option<u32>, gid: Option<u32>) -> Self {
        self.owner = uid.map(|uid| (uid, gid));
        self
    }

    fn own(&self, file: &File) -> Result<()> {
        let Some((uid, gid)) = self.owner else {
            return Ok(());
        };
        if !g5d_foundation::ownership::can_hand_over() {
            return Ok(());
        }
        let metadata = file.metadata().map_err(io_error)?;
        if metadata.uid() == uid && gid.is_none_or(|gid| metadata.gid() == gid) {
            return Ok(());
        }
        let gid = gid.unwrap_or(u32::MAX);
        if unsafe { libc::fchown(file.as_raw_fd(), uid, gid) } != 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    pub fn call(&self, method: &str, params: &Value) -> Result<Value> {
        let repo_id = string(params, "repo_id")?;
        let member = self.member(repo_id)?;
        match method {
            "fs.list" => self.list(member, string(params, "path")?),
            "fs.tree" => {
                let depth = params
                    .get("depth")
                    .map(|depth| {
                        depth
                            .as_i64()
                            .and_then(|depth| i32::try_from(depth).ok())
                            .ok_or_else(|| invalid("fs.tree depth must be an integer"))
                    })
                    .transpose()?
                    .unwrap_or(-1);
                let tracked = match params.get("tracked") {
                    None | Some(Value::Null) => BTreeSet::new(),
                    Some(Value::Array(paths)) => paths
                        .iter()
                        .map(|path| {
                            path.as_str()
                                .map(str::to_owned)
                                .ok_or_else(|| invalid("fs.tree tracked paths must be strings"))
                        })
                        .collect::<Result<BTreeSet<String>>>()?,
                    Some(_) => return Err(invalid("fs.tree tracked must be an array")),
                };
                let cap = params
                    .get("max_entries")
                    .map(|cap| {
                        cap.as_u64()
                            .filter(|cap| *cap > 0)
                            .ok_or_else(|| invalid("fs.tree max_entries must be positive"))
                    })
                    .transpose()?
                    .unwrap_or(MAX_TREE_ENTRIES)
                    .min(MAX_TREE_ENTRIES) as usize;
                let max_bytes = params
                    .get("max_bytes")
                    .map(|cap| {
                        cap.as_u64()
                            .filter(|cap| *cap > 0)
                            .ok_or_else(|| invalid("fs.tree max_bytes must be positive"))
                    })
                    .transpose()?
                    .unwrap_or(MAX_TREE_BYTES)
                    .min(MAX_TREE_BYTES) as usize;
                self.tree(
                    member,
                    string(params, "path")?,
                    depth,
                    &tracked,
                    crate::file_tree::WalkLimits::reply(cap, max_bytes),
                )
            }
            "fs.read" => self.read(member, string(params, "path")?, byte_window(params)?),
            "fs.stat" => self.stat(member, string(params, "path")?),
            "fs.write" => {
                let path = string(params, "path")?;
                self.validate(path, false)?;
                let expected = expected_hash(params)?;
                let bytes =
                    decode_content(string(params, "content")?, string(params, "encoding")?)?;
                let _guard = self
                    .mutation
                    .lock()
                    .map_err(|_| internal("file mutation lock poisoned"))?;
                self.write(member, path, expected, &bytes)
            }
            "fs.delete" => {
                let _guard = self
                    .mutation
                    .lock()
                    .map_err(|_| internal("file mutation lock poisoned"))?;
                self.delete(member, string(params, "path")?, expected_hash(params)?)
            }
            "fs.rename" => {
                let _guard = self
                    .mutation
                    .lock()
                    .map_err(|_| internal("file mutation lock poisoned"))?;
                self.rename(
                    member,
                    string(params, "from")?,
                    string(params, "to")?,
                    expected_hash(params)?,
                )
            }
            _ => Err(FileError::new("METHOD_NOT_FOUND", "unknown file operation")),
        }
    }

    /// Validate and pin the watched object before the caller registers a watcher.
    /// Watch event paths must pass `is_visible_path` before being delivered.
    pub fn validate_watch(&self, repo_id: &str, path: &str) -> Result<()> {
        let member = self.member(repo_id)?;
        self.open(member, path, libc::O_RDONLY).map(|_| ())
    }

    pub fn is_visible_path(&self, repo_id: &str, path: &str) -> bool {
        self.member(repo_id).is_ok() && self.validate(path, true).is_ok()
    }

    /// Pinned, no-symlink directory descriptors for nonrecursive native watches.
    pub fn watch_directories(&self, repo_id: &str, path: &str) -> Result<Vec<(String, File)>> {
        let member = self.member(repo_id)?;
        self.validate(path, true)?;
        let mut result = Vec::new();
        // Keep ancestor watches after deletion so recreating a watched subtree
        // reestablishes its child watches without another client subscription.
        if !path.is_empty() {
            result.push((
                String::new(),
                self.open(member, "", libc::O_RDONLY | libc::O_DIRECTORY)?,
            ));
            let components: Vec<_> = path.split('/').collect();
            for end in 1..components.len() {
                let ancestor = components[..end].join("/");
                match self.open(member, &ancestor, libc::O_RDONLY | libc::O_DIRECTORY) {
                    Ok(file) => result.push((ancestor, file)),
                    Err(error) if matches!(error.code, "FILE_MISSING" | "PATH_FORBIDDEN") => {
                        return Ok(result)
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        let file = match self.open(member, path, libc::O_RDONLY) {
            Ok(file) => file,
            Err(error) if matches!(error.code, "FILE_MISSING" | "PATH_FORBIDDEN") => {
                return Ok(result)
            }
            Err(error) => return Err(error),
        };
        if !file.metadata().map_err(io_error)?.is_dir() {
            return Ok(result);
        }
        // The directories below `path`, pruned by the checkout's `.gitignore`
        // files exactly as the Files tree is: dependency and build output
        // (`node_modules/`, `target/`) is neither shown nor watched, so a
        // repository's watch fits the native limit and a build's writes
        // raise no notifications. Hidden paths stay watched. A subtree that
        // still exceeds the limit is refused as before.
        let walk = crate::file_tree::walk_tree_with(
            &TreeReader {
                service: self,
                member,
            },
            path,
            -1,
            &BTreeSet::new(),
            crate::file_tree::WalkLimits::directories(MAX_WATCH_DIRECTORIES),
            crate::file_tree::Hidden::Keep,
        )?;
        let directories: Vec<&str> = walk
            .entries
            .iter()
            .filter(|entry| entry.is_dir)
            .map(|entry| entry.path.as_str())
            .collect();
        // Levels below the watched root, as the recursive registration
        // counted them: the root is level 0.
        let base = if path.is_empty() {
            0
        } else {
            path.matches('/').count() + 1
        };
        if walk.truncated
            || directories.len() >= MAX_WATCH_DIRECTORIES
            || directories
                .iter()
                .any(|directory| directory.matches('/').count() + 1 - base > MAX_WATCH_DEPTH)
        {
            return Err(FileError::new(
                "WATCH_LIMIT_EXCEEDED",
                "watch exceeds 10000 directories or 64 levels",
            ));
        }
        for directory in directories {
            match self.open(member, directory, libc::O_RDONLY | libc::O_DIRECTORY) {
                Ok(child) => result.push((directory.to_owned(), child)),
                Err(error) if matches!(error.code, "FILE_MISSING" | "PATH_FORBIDDEN") => continue,
                Err(error) => return Err(error),
            }
        }
        result.push((path.to_owned(), file));
        Ok(result)
    }

    /// Native watcher descriptors may outlive a rename. Match the current
    /// confined parent to the held descriptor before disclosing an event name.
    pub fn watch_parent_matches(&self, repo_id: &str, directory_path: &str, held: &File) -> bool {
        let Ok(member) = self.member(repo_id) else {
            return false;
        };
        let Ok(current) = self.open(member, directory_path, libc::O_RDONLY | libc::O_DIRECTORY)
        else {
            return false;
        };
        let (Ok(current), Ok(held)) = (current.metadata(), held.metadata()) else {
            return false;
        };
        current.dev() == held.dev() && current.ino() == held.ino()
    }

    pub fn watch_path_is_directory(&self, repo_id: &str, path: &str) -> bool {
        let Ok(member) = self.member(repo_id) else {
            return false;
        };
        self.open(member, path, libc::O_RDONLY | libc::O_DIRECTORY)
            .is_ok()
    }

    fn member(&self, repo_id: &str) -> Result<&Member> {
        self.members
            .get(repo_id)
            .ok_or_else(|| FileError::new("PATH_FORBIDDEN", "unknown member repository"))
    }

    fn validate(&self, path: &str, root_allowed: bool) -> Result<()> {
        validate_components(path, root_allowed)?;
        if path.split('/').any(|component| {
            component.eq_ignore_ascii_case(".git")
                || component
                    .get(..STAGING_PREFIX.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(STAGING_PREFIX))
        }) || self
            .excludes
            .iter()
            .any(|exclude| same_or_descendant(path, exclude))
        {
            return Err(FileError::new(
                "PATH_FORBIDDEN",
                "repository metadata and platform runtime paths are forbidden",
            ));
        }
        Ok(())
    }

    fn parent(&self, member: &Member, path: &str) -> Result<(File, String)> {
        self.parent_with(member, path, false)
    }

    /// The parent directory of a new file, creating missing intermediate
    /// directories the same confined way the path is walked: one component
    /// at a time from the member root, never through a symlink. A directory
    /// that appears between the failed open and the create is opened on the
    /// retry under the same rules, so a swapped-in symlink is still refused.
    fn parent_creating(&self, member: &Member, path: &str) -> Result<(File, String)> {
        self.parent_with(member, path, true)
    }

    fn parent_with(&self, member: &Member, path: &str, create: bool) -> Result<(File, String)> {
        self.validate(path, false)?;
        let mut components: Vec<_> = path.split('/').collect();
        let name = components
            .pop()
            .ok_or_else(|| invalid("a file path is required"))?;
        let mut directory = openat(&member.root, ".", libc::O_RDONLY | libc::O_DIRECTORY)?;
        for component in components {
            directory = match openat(&directory, component, libc::O_RDONLY | libc::O_DIRECTORY) {
                Ok(next) => next,
                Err(error) if create && error.code == "FILE_MISSING" => {
                    let child = cstring(component)?;
                    if unsafe { libc::mkdirat(directory.as_raw_fd(), child.as_ptr(), 0o755) } < 0 {
                        let error = std::io::Error::last_os_error();
                        // Lost a race to another creator: open whatever is
                        // there now under the same no-follow rules.
                        if error.raw_os_error() != Some(libc::EEXIST) {
                            return Err(io_error(error));
                        }
                    }
                    openat(&directory, component, libc::O_RDONLY | libc::O_DIRECTORY)?
                }
                Err(error) => return Err(error),
            };
            if create {
                // Publish intermediate directories under the same identity
                // as the file. Also repair an existing directory left by a
                // previous failed create/handoff. Use the confined descriptor
                // so a concurrent rename or symlink cannot redirect fchown.
                self.own(&directory)?;
            }
        }
        Ok((directory, name.to_owned()))
    }

    fn open(&self, member: &Member, path: &str, flags: libc::c_int) -> Result<File> {
        self.validate(path, true)?;
        if path.is_empty() {
            return openat(&member.root, ".", flags | libc::O_DIRECTORY);
        }
        let (parent, name) = self.parent(member, path)?;
        if !ordinary_entry(&parent, &name)? {
            return Err(FileError::new(
                "PATH_FORBIDDEN",
                "only regular files and directories are accessible",
            ));
        }
        let file = openat(&parent, &name, flags)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(FileError::new(
                "PATH_FORBIDDEN",
                "only regular files and directories are accessible",
            ));
        }
        Ok(file)
    }

    fn read(&self, member: &Member, path: &str, window: Option<(u64, u64)>) -> Result<Value> {
        let mut file = self.open(member, path, libc::O_RDONLY)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() {
            return Err(invalid("fs.read requires a regular file"));
        }
        if let Some((offset, length)) = window {
            return read_window(&mut file, &metadata, member.format, path, offset, length);
        }
        let bytes = read_bounded(&mut file)?;
        let mut result = file_fields(path, &metadata, member.format, &bytes);
        let content = match std::str::from_utf8(&bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => base64::engine::general_purpose::STANDARD.encode(&bytes),
        };
        result["content"] = Value::String(content);
        Ok(result)
    }

    fn stat(&self, member: &Member, path: &str) -> Result<Value> {
        let mut file = self.open(member, path, libc::O_RDONLY)?;
        let metadata = file.metadata().map_err(io_error)?;
        if metadata.is_dir() {
            return Ok(directory_fields(path, &metadata));
        }
        let (hash, is_utf8) = hash_observation(member.format, &mut file, metadata.len())?;
        Ok(
            json!({"path":path, "type":"file", "hash":hash, "size":metadata.len(), "mode":metadata.mode() & 0o7777, "mtime":mtime(&metadata), "encoding":if is_utf8 {"utf-8"} else {"base64"}}),
        )
    }

    fn list(&self, member: &Member, path: &str) -> Result<Value> {
        let directory = self.open(member, path, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let mut entries = Vec::new();
        for name in directory_names(&directory)? {
            let entry_path = join(path, &name);
            if self.validate(&entry_path, false).is_err() {
                continue;
            }
            match ordinary_entry(&directory, &name) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) if error.code == "FILE_MISSING" => continue,
                Err(error) => return Err(error),
            }
            let file = match openat(&directory, &name, libc::O_RDONLY) {
                Ok(file) => file,
                Err(error) if matches!(error.code, "FILE_MISSING" | "PATH_FORBIDDEN") => continue,
                Err(error) => return Err(error),
            };
            let metadata = file.metadata().map_err(io_error)?;
            if !metadata.is_file() && !metadata.is_dir() {
                continue;
            }
            entries.push(json!({"name": name, "path": entry_path, "type": if metadata.is_dir() { "directory" } else { "file" }, "size": metadata.len(), "mode": metadata.mode() & 0o7777, "mtime": mtime(&metadata)}));
        }
        Ok(json!({"path":path, "entries":entries}))
    }

    /// The whole subtree under `path` in one reply, as the Files tree shows
    /// it: the checkout's `.gitignore` files prune ignored entries unless
    /// `tracked` names them, hidden product paths are skipped, and a walk
    /// past `cap` entries stops and says so (`g5.documents.read.live-tree`).
    fn tree(
        &self,
        member: &Member,
        path: &str,
        depth: i32,
        tracked: &BTreeSet<String>,
        limits: crate::file_tree::WalkLimits,
    ) -> Result<Value> {
        // A missing or non-directory root is the caller's error; a directory
        // that vanishes during the walk is skipped by the walk itself.
        self.open(member, path, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let walk = crate::file_tree::walk_tree(
            &TreeReader {
                service: self,
                member,
            },
            path,
            depth,
            tracked,
            limits,
        )?;
        let entries = walk
            .entries
            .iter()
            .map(|entry| {
                json!({"path": entry.path, "type": if entry.is_dir { "directory" } else { "file" }, "size": entry.size})
            })
            .collect::<Vec<_>>();
        // The checkout's HEAD labels the live tree; read here so the tree
        // never waits on a Changes run in the VM (`g5.documents.read.live-tree`).
        let head_oid = head_oid(member);
        Ok(
            json!({"path": path, "entries": entries, "truncated": walk.truncated, "head_oid": head_oid}),
        )
    }

    fn write(
        &self,
        member: &Member,
        path: &str,
        expected: Option<&str>,
        bytes: &[u8],
    ) -> Result<Value> {
        // A create (no expected hash) may target a folder that does not exist
        // yet, such as a fork's own `docs/<workspace>/` Project Docs folder;
        // an update of an existing file never creates directories.
        let (parent, name) = if expected.is_none() {
            self.parent_creating(member, path)?
        } else {
            self.parent(member, path)?
        };
        let mode = match expected {
            None => {
                if entry_exists(&parent, &name)? {
                    return Err(self.conflict_at(member, &parent, &name, "FILE_EXISTS")?);
                }
                None
            }
            Some(expected) => {
                let mut file = openat(&parent, &name, libc::O_RDONLY)?;
                self.check_hash(member, &mut file, Some(expected))?;
                // Replacement content keeps ordinary access permissions, but
                // never carries setuid, setgid, or sticky bits to the new inode.
                Some(file.metadata().map_err(io_error)?.mode() & 0o0777)
            }
        };
        // Stage beside the destination so rename remains on the same filesystem.
        // Until successful publication, failures leave the destination intact.
        let mut temporary = TemporaryEntry::new(&parent)?;
        let mut file = &temporary.file;
        if let Some(mode) = mode {
            file.set_permissions(std::fs::Permissions::from_mode(mode))
                .map_err(io_error)?;
        }
        file.write_all(bytes).map_err(io_error)?;
        file.sync_data().map_err(io_error)?;
        self.own(file)?;
        let metadata = file.metadata().map_err(io_error)?;
        match expected {
            None => {
                if let Err(error) = rename_noreplace(&parent, &temporary.name, &parent, &name) {
                    return if error.code == "FILE_EXISTS" {
                        Err(self.conflict_at(member, &parent, &name, "FILE_EXISTS")?)
                    } else {
                        Err(error)
                    };
                }
            }
            Some(expected) => {
                // Recheck after slow disk I/O, including rejecting a newly placed
                // symlink. Native owner processes otherwise retain last-writer semantics.
                let mut current = openat(&parent, &name, libc::O_RDONLY)?;
                self.check_hash(member, &mut current, Some(expected))?;
                let source = cstring(&temporary.name)?;
                let target = cstring(&name)?;
                if unsafe {
                    libc::renameat(
                        parent.as_raw_fd(),
                        source.as_ptr(),
                        parent.as_raw_fd(),
                        target.as_ptr(),
                    )
                } != 0
                {
                    return Err(io_error(std::io::Error::last_os_error()));
                }
            }
        }
        temporary.published = true;
        Ok(file_fields(path, &metadata, member.format, bytes))
    }

    fn check_hash(&self, member: &Member, file: &mut File, expected: Option<&str>) -> Result<()> {
        let metadata = file.metadata().map_err(io_error)?;
        if metadata.is_dir() {
            return if expected.is_none() {
                Ok(())
            } else {
                Err(invalid("directory mutations require expected_hash=null"))
            };
        }
        if !metadata.is_file() {
            return Err(FileError::new(
                "PATH_FORBIDDEN",
                "only regular files and directories are accessible",
            ));
        }
        let expected = expected
            .ok_or_else(|| invalid("file delete and rename require a matching expected_hash"))?;
        let actual = hash_reader(member.format, file, metadata.len())?;
        if actual != expected {
            return Err(FileError {
                code: "FILE_VERSION_CONFLICT",
                message: "file changed since it was read".into(),
                data: json!({"hash":actual, "size":metadata.len()}),
            });
        }
        Ok(())
    }

    fn conflict_at(
        &self,
        member: &Member,
        parent: &File,
        name: &str,
        code: &'static str,
    ) -> Result<FileError> {
        let mut error = FileError::new(code, "destination already exists");
        // Do not follow an existing symlink just to populate a conflict.
        if let Ok(mut file) = openat(parent, name, libc::O_RDONLY) {
            let metadata = file.metadata().map_err(io_error)?;
            if metadata.is_file() {
                error.data = json!({"hash":hash_reader(member.format, &mut file, metadata.len())?, "size":metadata.len()});
            }
        }
        Ok(error)
    }

    fn delete(&self, member: &Member, path: &str, expected: Option<&str>) -> Result<Value> {
        let (parent, name) = self.parent(member, path)?;
        let mut file = openat(&parent, &name, libc::O_RDONLY)?;
        self.check_hash(member, &mut file, expected)?;
        let directory = file.metadata().map_err(io_error)?.is_dir();
        let name = cstring(&name)?;
        let rc = unsafe {
            libc::unlinkat(
                parent.as_raw_fd(),
                name.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        };
        if rc != 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok(json!({"path":path, "deleted":true}))
    }

    fn rename(
        &self,
        member: &Member,
        from: &str,
        to: &str,
        expected: Option<&str>,
    ) -> Result<Value> {
        let (source_parent, source_name) = self.parent(member, from)?;
        let (target_parent, target_name) = self.parent(member, to)?;
        let mut source = openat(&source_parent, &source_name, libc::O_RDONLY)?;
        if source.metadata().map_err(io_error)?.is_dir()
            && self
                .excludes
                .iter()
                .any(|exclude| same_or_descendant(exclude, from) || same_or_descendant(exclude, to))
        {
            return Err(FileError::new(
                "PATH_FORBIDDEN",
                "cannot move a directory from or into a platform runtime ancestor",
            ));
        }
        self.check_hash(member, &mut source, expected)?;
        match rename_noreplace(&source_parent, &source_name, &target_parent, &target_name) {
            Ok(()) => Ok(json!({"from":from, "to":to, "renamed":true})),
            Err(error) if error.code == "FILE_EXISTS" => {
                Err(self.conflict_at(member, &target_parent, &target_name, "FILE_EXISTS")?)
            }
            Err(error) => Err(error),
        }
    }
}

/// The confined file service as a tree walk source: directory listings and
/// `.gitignore` reads go through the same pinned-descriptor path checks as
/// every other file operation.
struct TreeReader<'a> {
    service: &'a FileService,
    member: &'a Member,
}

impl crate::file_tree::TreeSource for TreeReader<'_> {
    type Error = FileError;

    fn list(&self, directory: &str) -> Result<crate::file_tree::Listing> {
        let listed = match self.service.list(self.member, directory) {
            Ok(listed) => listed,
            Err(error) if error.code == "FILE_MISSING" => {
                return Ok(crate::file_tree::Listing::Missing)
            }
            // Wider than one listing carries: the tree is past its cap.
            Err(error) if error.code == "DIRECTORY_LIMIT_EXCEEDED" => {
                return Ok(crate::file_tree::Listing::TooWide)
            }
            Err(error) => return Err(error),
        };
        let entries = listed["entries"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        Some(crate::file_tree::SourceEntry {
                            name: entry["name"].as_str()?.to_owned(),
                            is_dir: entry["type"].as_str() == Some("directory"),
                            size: entry["size"].as_u64().unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(crate::file_tree::Listing::Entries(entries))
    }

    fn read_gitignore(&self, directory: &str) -> Option<String> {
        let path = join(directory, crate::file_tree::GITIGNORE);
        let value = self.service.read(self.member, &path, None).ok()?;
        if value["encoding"].as_str() != Some("utf-8") {
            return None;
        }
        value["content"].as_str().map(str::to_owned)
    }
}

/// The most bytes read from one Git metadata file (`HEAD`, a loose ref,
/// `packed-refs`); larger files are treated as unreadable.
const MAX_GIT_METADATA_BYTES: u64 = 8 * 1024 * 1024;
/// How many symbolic refs `HEAD` may chain through before giving up.
const MAX_SYMREF_HOPS: usize = 8;

/// The checkout's HEAD commit, read from `.git` through the same pinned,
/// symlink-refusing descriptor walk as every other file operation, with no
/// Git process. A detached HEAD names the commit; a symbolic one names a
/// ref that may chain through further symbolic refs and is either loose
/// under `.git/` or, when no loose file exists, packed in `packed-refs`.
/// `None` when the layout is not an ordinary clone (a worktree's `.git`
/// file, an unborn branch), when anything is a symlink, a FIFO or too
/// large, or when the chain does not end in a commit: the caller then
/// labels the tree with no OID rather than a wrong one.
fn head_oid(member: &Member) -> Option<String> {
    let git = openat(&member.root, ".git", libc::O_RDONLY | libc::O_DIRECTORY).ok()?;
    let GitMetadata::Content(mut text) = read_git_metadata(&git, "HEAD") else {
        return None;
    };
    for _ in 0..MAX_SYMREF_HOPS {
        let text_trimmed = text.trim();
        let Some(reference) = text_trimmed.strip_prefix("ref: ") else {
            return is_oid(text_trimmed).then(|| text_trimmed.to_owned());
        };
        let reference = reference.trim();
        let components: Vec<&str> = reference.split('/').collect();
        if components
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return None;
        }
        // The loose ref, one confined component at a time.
        let mut directory = openat(&git, ".", libc::O_RDONLY | libc::O_DIRECTORY).ok()?;
        let mut loose = GitMetadata::Missing;
        for (index, part) in components.iter().enumerate() {
            if index + 1 == components.len() {
                loose = read_git_metadata(&directory, part);
            } else {
                match openat(&directory, part, libc::O_RDONLY | libc::O_DIRECTORY) {
                    Ok(next) => directory = next,
                    // A missing directory means no loose ref; anything else
                    // (a symlink, a file in the way) is a rejected layout.
                    Err(error) if error.code == "FILE_MISSING" => break,
                    Err(_) => return None,
                }
            }
        }
        match loose {
            // A loose file: a commit, or another symbolic ref to follow.
            GitMetadata::Content(next) => text = next,
            // No loose file at all: the packed value, if any.
            GitMetadata::Missing => {
                return match read_git_metadata(&git, "packed-refs") {
                    GitMetadata::Content(packed) => packed_ref(&packed, reference),
                    GitMetadata::Missing | GitMetadata::Rejected => None,
                };
            }
            // A loose file that is not readable metadata: never fall back to
            // a packed value that may be stale.
            GitMetadata::Rejected => return None,
        }
    }
    None
}

/// One Git metadata file under `directory`: a regular file, read whole and
/// bounded, as UTF-8. Symlinks, FIFOs, directories and oversized files are
/// not metadata and read as `None`.
fn read_git_metadata(directory: &File, name: &str) -> GitMetadata {
    let file = match openat(directory, name, libc::O_RDONLY) {
        Ok(file) => file,
        Err(error) if error.code == "FILE_MISSING" => return GitMetadata::Missing,
        Err(_) => return GitMetadata::Rejected,
    };
    let Ok(metadata) = file.metadata() else {
        return GitMetadata::Rejected;
    };
    if !metadata.is_file() || metadata.len() > MAX_GIT_METADATA_BYTES {
        return GitMetadata::Rejected;
    }
    let mut bytes = Vec::new();
    if file
        .take(MAX_GIT_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_GIT_METADATA_BYTES
    {
        return GitMetadata::Rejected;
    }
    match String::from_utf8(bytes) {
        Ok(text) => GitMetadata::Content(text),
        Err(_) => GitMetadata::Rejected,
    }
}

/// What a Git metadata read found: the file's text, no file at all (a loose
/// ref may then be packed), or something that is not readable metadata (a
/// symlink, a FIFO, an oversized or non-UTF-8 file), which ends resolution
/// so a stale packed value is never substituted.
enum GitMetadata {
    Content(String),
    Missing,
    Rejected,
}

/// The commit `packed-refs` records for `reference`, skipping the header
/// comments and the `^` peeled lines that follow annotated tags.
fn packed_ref(packed: &str, reference: &str) -> Option<String> {
    packed.lines().find_map(|line| {
        if line.starts_with('#') || line.starts_with('^') {
            return None;
        }
        let (oid, name) = line.split_once(' ')?;
        (name.trim() == reference && is_oid(oid)).then(|| oid.to_owned())
    })
}

fn is_oid(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid(message: impl Into<String>) -> FileError {
    FileError::new("INVALID_PARAMS", message)
}
fn internal(message: impl Into<String>) -> FileError {
    FileError::new("IO_ERROR", message)
}

fn string<'a>(params: &'a Value, field: &str) -> Result<&'a str> {
    params
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{field} must be a string")))
}

fn expected_hash(params: &Value) -> Result<Option<&str>> {
    match params.get("expected_hash") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(hash)) if !hash.is_empty() => Ok(Some(hash)),
        _ => Err(invalid(
            "expected_hash is required and must be a hash string or null",
        )),
    }
}

fn validate_components(path: &str, root_allowed: bool) -> Result<()> {
    if path.is_empty() && root_allowed {
        return Ok(());
    }
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(FileError::new(
            "PATH_FORBIDDEN",
            "a normalized relative member path is required",
        ));
    }
    Ok(())
}

fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| invalid("path contains NUL"))
}

fn same_or_descendant(path: &str, ancestor: &str) -> bool {
    path.eq_ignore_ascii_case(ancestor)
        || (path
            .get(..ancestor.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(ancestor))
            && path.as_bytes().get(ancestor.len()) == Some(&b'/'))
}

fn entry_exists(parent: &File, name: &str) -> Result<bool> {
    let name = cstring(name)?;
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let rc = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc == 0 {
        Ok(true)
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(false)
        } else {
            Err(io_error(error))
        }
    }
}

fn ordinary_entry(parent: &File, name: &str) -> Result<bool> {
    let name = cstring(name)?;
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    let mode = unsafe { metadata.assume_init().st_mode };
    Ok(matches!(mode & libc::S_IFMT, libc::S_IFREG | libc::S_IFDIR))
}

fn owned_fd(fd: RawFd) -> Result<File> {
    if fd < 0 {
        Err(io_error(std::io::Error::last_os_error()))
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn openat(parent: &File, name: &str, flags: libc::c_int) -> Result<File> {
    let name = cstring(name)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(libc::ENXIO | libc::EOPNOTSUPP)) {
            Err(FileError::new(
                "PATH_FORBIDDEN",
                "only regular files and directories are accessible",
            ))
        } else {
            Err(io_error(error))
        }
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn createat(parent: &File, name: &str) -> Result<File> {
    let name = cstring(name)?;
    owned_fd(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o644,
        )
    })
}

struct TemporaryEntry<'a> {
    parent: &'a File,
    name: String,
    file: File,
    published: bool,
}

impl<'a> TemporaryEntry<'a> {
    fn new(parent: &'a File) -> Result<Self> {
        let name = format!("{STAGING_PREFIX}{}", uuid::Uuid::new_v4());
        let file = createat(parent, &name)?;
        Ok(Self {
            parent,
            name,
            file,
            published: false,
        })
    }
}

impl Drop for TemporaryEntry<'_> {
    fn drop(&mut self) {
        // The entry is absent after a successful rename. On any prior failure,
        // remove only the private staging entry, never the requested destination.
        if self.published {
            return;
        }
        if let Ok(name) = cstring(&self.name) {
            unsafe {
                libc::unlinkat(self.parent.as_raw_fd(), name.as_ptr(), 0);
            }
        }
    }
}

fn io_error(error: std::io::Error) -> FileError {
    let (code, message) = match error.raw_os_error() {
        Some(libc::ENOENT) => ("FILE_MISSING", "file or parent directory does not exist"),
        Some(libc::EEXIST) => ("FILE_EXISTS", "destination already exists"),
        Some(libc::ENOTEMPTY) => ("DIRECTORY_NOT_EMPTY", "directory is not empty"),
        Some(libc::ELOOP) | Some(libc::ENOTDIR) => (
            "PATH_FORBIDDEN",
            "symlink traversal and invalid directory components are forbidden",
        ),
        Some(libc::EACCES) | Some(libc::EPERM) => {
            ("PATH_FORBIDDEN", "filesystem permission denied")
        }
        _ => ("IO_ERROR", "filesystem operation failed"),
    };
    // OS paths and runtime details do not belong in protocol errors.
    FileError::new(code, message)
}

fn directory_names(directory: &File) -> Result<Vec<String>> {
    let copy = openat(directory, ".", libc::O_RDONLY | libc::O_DIRECTORY)?;
    use std::os::fd::IntoRawFd;
    let fd = copy.into_raw_fd();
    let dir = unsafe { libc::fdopendir(fd) };
    if dir.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(io_error(error));
    }
    let mut names = Vec::new();
    let mut error = None;
    loop {
        // readdir distinguishes end-of-directory from failure through errno.
        #[cfg(target_os = "linux")]
        unsafe {
            *libc::__errno_location() = 0;
        }
        #[cfg(target_os = "macos")]
        unsafe {
            *libc::__error() = 0;
        }
        let entry = unsafe { libc::readdir(dir) };
        if entry.is_null() {
            let os_error = std::io::Error::last_os_error();
            if os_error.raw_os_error() != Some(0) {
                error = Some(io_error(os_error));
            }
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if let Ok(name) = name.to_str() {
            if name != "." && name != ".." {
                names.push(name.to_owned());
                if names.len() > 10_000 {
                    error = Some(FileError::new(
                        "DIRECTORY_LIMIT_EXCEEDED",
                        "directory listing exceeds 10000 entries",
                    ));
                    break;
                }
            }
        }
    }
    unsafe {
        libc::closedir(dir);
    }
    if let Some(error) = error {
        return Err(error);
    }
    names.sort();
    Ok(names)
}

fn rename_noreplace(
    source_parent: &File,
    source: &str,
    target_parent: &File,
    target: &str,
) -> Result<()> {
    let source = cstring(source)?;
    let target = cstring(target)?;
    #[cfg(target_os = "linux")]
    let rc = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_parent.as_raw_fd(),
            source.as_ptr(),
            target_parent.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let rc = unsafe {
        libc::renameatx_np(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            target_parent.as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    compile_error!("g5d-dispatch requires Linux or macOS descriptor-relative rename support");
    if rc == 0 {
        Ok(())
    } else {
        Err(io_error(std::io::Error::last_os_error()))
    }
}

pub fn decode_content(content: &str, encoding: &str) -> Result<Vec<u8>> {
    match encoding {
        "utf-8" => {
            if content.len() > MAX_FILE_BYTES {
                return Err(payload_too_large());
            }
            Ok(content.as_bytes().to_vec())
        }
        "base64" => {
            // Derive the exact decoded length from the encoded length and padding
            // before allocating the destination. The decoder validates all bytes.
            let len = content.len();
            if !len.is_multiple_of(4) {
                return Err(invalid("invalid base64 content"));
            }
            let padding = content
                .as_bytes()
                .iter()
                .rev()
                .take_while(|&&b| b == b'=')
                .count();
            if padding > 2 {
                return Err(invalid("invalid base64 content"));
            }
            let decoded_len = (len / 4)
                .checked_mul(3)
                .and_then(|n| n.checked_sub(padding))
                .ok_or_else(|| invalid("invalid base64 content"))?;
            if decoded_len > MAX_FILE_BYTES {
                return Err(payload_too_large());
            }
            let mut bytes = vec![0; decoded_len];
            let count = base64::engine::general_purpose::STANDARD
                .decode_slice(content, &mut bytes)
                .map_err(|_| invalid("invalid base64 content"))?;
            bytes.truncate(count);
            Ok(bytes)
        }
        _ => Err(invalid("encoding must be utf-8 or base64")),
    }
}

fn payload_too_large() -> FileError {
    FileError::new(
        "PAYLOAD_TOO_LARGE",
        "file content exceeds the 32 MiB decoded-byte limit",
    )
}

/// One window of a file, for a caller whose reply channel is smaller than
/// the file: the window's bytes (always base64) with the whole file's size
/// and hash. The file is read once and the window cut from those same
/// bytes, so the hash names exactly the content the window came from and
/// a caller whose windows all report one hash has one file. A window past
/// the end is empty, not an error.
fn read_window(
    file: &mut File,
    metadata: &std::fs::Metadata,
    format: ObjectFormat,
    path: &str,
    offset: u64,
    length: u64,
) -> Result<Value> {
    let bytes = read_bounded(file)?;
    let size = bytes.len() as u64;
    let start = offset.min(size) as usize;
    let end = (start as u64).saturating_add(length).min(size) as usize;
    let window = &bytes[start..end];
    Ok(json!({
        "path": path,
        "type": "file",
        "hash": blob_hash(format, &bytes),
        "size": size,
        "mode": metadata.mode() & 0o7777,
        "mtime": mtime(metadata),
        "file_encoding": if std::str::from_utf8(&bytes).is_ok() { "utf-8" } else { "base64" },
        "offset": start,
        "length": window.len(),
        "encoding": "base64",
        "content": base64::engine::general_purpose::STANDARD.encode(window),
    }))
}

/// The optional `offset`/`length` pair of `fs.read`: both or neither, the
/// length between 1 and the file byte limit.
fn byte_window(params: &Value) -> Result<Option<(u64, u64)>> {
    match (params.get("offset"), params.get("length")) {
        (None, None) => Ok(None),
        (Some(offset), Some(length)) => {
            let offset = offset
                .as_u64()
                .ok_or_else(|| invalid("offset must be a non-negative integer"))?;
            let length = length
                .as_u64()
                .filter(|length| *length > 0 && *length <= MAX_FILE_BYTES as u64)
                .ok_or_else(|| invalid("length must be between 1 and the file byte limit"))?;
            Ok(Some((offset, length)))
        }
        _ => Err(invalid("offset and length go together")),
    }
}

fn read_bounded(file: &mut File) -> Result<Vec<u8>> {
    if file.metadata().map_err(io_error)?.len() > MAX_FILE_BYTES as u64 {
        return Err(payload_too_large());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(payload_too_large());
    }
    Ok(bytes)
}

pub fn blob_hash(format: ObjectFormat, bytes: &[u8]) -> String {
    let header = format!("blob {}\0", bytes.len());
    match format {
        ObjectFormat::Sha1 => {
            let mut hash = sha1::Sha1::new();
            hash.update(header);
            hash.update(bytes);
            format!("{:x}", hash.finalize())
        }
        ObjectFormat::Sha256 => {
            let mut hash = sha2::Sha256::new();
            hash.update(header);
            hash.update(bytes);
            format!("{:x}", hash.finalize())
        }
    }
}

fn hash_reader(format: ObjectFormat, file: &mut File, size: u64) -> Result<String> {
    hash_observation(format, file, size).map(|(hash, _)| hash)
}

fn hash_observation(format: ObjectFormat, file: &mut File, size: u64) -> Result<(String, bool)> {
    fn hash<D: Digest>(file: &mut File, size: u64) -> Result<(String, bool)> {
        let mut digest = D::new();
        digest.update(format!("blob {size}\0"));
        let mut buffer = [0u8; 64 * 1024];
        let mut read = 0u64;
        let mut utf8_pending = Vec::new();
        let mut is_utf8 = true;
        loop {
            let n = file.read(&mut buffer).map_err(io_error)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            digest.update(&buffer[..n]);
            if is_utf8 {
                utf8_pending.extend_from_slice(&buffer[..n]);
                match std::str::from_utf8(&utf8_pending) {
                    Ok(_) => utf8_pending.clear(),
                    Err(error) if error.error_len().is_none() => {
                        utf8_pending.drain(..error.valid_up_to());
                    }
                    Err(_) => {
                        is_utf8 = false;
                        utf8_pending.clear();
                    }
                }
            }
        }
        if read != size {
            return Err(FileError::new(
                "FILE_VERSION_CONFLICT",
                "file size changed during observation; retry the read",
            ));
        }
        let hash = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok((hash, is_utf8 && utf8_pending.is_empty()))
    }
    match format {
        ObjectFormat::Sha1 => hash::<sha1::Sha1>(file, size),
        ObjectFormat::Sha256 => hash::<sha2::Sha256>(file, size),
    }
}

fn file_fields(
    path: &str,
    metadata: &std::fs::Metadata,
    format: ObjectFormat,
    bytes: &[u8],
) -> Value {
    json!({"path":path, "type":"file", "hash":blob_hash(format,bytes), "size":bytes.len(), "mode":metadata.mode() & 0o7777, "mtime":mtime(metadata), "encoding":if std::str::from_utf8(bytes).is_ok() { "utf-8" } else { "base64" }})
}

fn directory_fields(path: &str, metadata: &std::fs::Metadata) -> Value {
    json!({"path":path, "type":"directory", "size":metadata.len(), "mode":metadata.mode() & 0o7777, "mtime":mtime(metadata)})
}

fn mtime(metadata: &std::fs::Metadata) -> f64 {
    metadata.mtime() as f64 + metadata.mtime_nsec() as f64 / 1_000_000_000.0
}
fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Barrier};

    fn service(root: &std::path::Path) -> FileService {
        FileService::new(
            vec![RepositoryRoot {
                repo_id: "repo".into(),
                root: root.to_owned(),
                object_format: ObjectFormat::Sha1,
            }],
            vec![".g5/cache".into(), ".g5/runtime".into()],
        )
        .unwrap()
    }

    #[test]
    fn tree_lists_a_subtree_in_one_call_pruned_by_the_checkouts_gitignore() {
        let root = tempfile::tempdir().unwrap();
        for (path, content) in [
            (".gitignore", "target/\n*.log\n!keep.log\n*.md\n"),
            ("README.md", "tracked although *.md is ignored\n"),
            ("notes.md", "untracked and ignored\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("keep.log", "re-included\n"),
            ("debug.log", "ignored\n"),
            ("target/debug/deps/g5.rlib", "build output\n"),
            ("frontend/.gitignore", "dist/\n"),
            ("frontend/dist/app.js", "ignored by the nested dist/\n"),
            ("frontend/src/main.ts", "export {}\n"),
            (".git/HEAD", "repository metadata\n"),
        ] {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, content).unwrap();
        }
        let files = service(root.path());

        let tree = files
            .call(
                "fs.tree",
                &json!({"repo_id":"repo","path":"","depth":-1,"tracked":["README.md"],"max_entries":10000}),
            )
            .unwrap();
        assert_eq!(tree["truncated"], false);
        let listed: Vec<&str> = tree["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            listed,
            [
                "README.md",
                "frontend",
                "keep.log",
                "src",
                "frontend/src",
                "src/main.rs",
                "frontend/src/main.ts",
            ],
            "ignored trees are pruned, tracked and re-included paths stay"
        );
        let directory = tree["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == "src")
            .unwrap();
        assert_eq!(directory["type"], "directory");

        // The cap stops the walk and says so.
        let capped = files
            .call(
                "fs.tree",
                &json!({"repo_id":"repo","path":"","max_entries":2}),
            )
            .unwrap();
        assert_eq!(capped["truncated"], true);
        assert_eq!(capped["entries"].as_array().unwrap().len(), 3);

        // A subtree inherits the rules above it; a missing root is an error.
        let subtree = files
            .call("fs.tree", &json!({"repo_id":"repo","path":"frontend"}))
            .unwrap();
        assert_eq!(
            subtree["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["path"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["frontend/src", "frontend/src/main.ts"]
        );
        assert_eq!(
            files
                .call("fs.tree", &json!({"repo_id":"repo","path":"missing"}))
                .unwrap_err()
                .code,
            "FILE_MISSING"
        );
        assert_eq!(
            files
                .call(
                    "fs.tree",
                    &json!({"repo_id":"repo","path":"","tracked":"README.md"})
                )
                .unwrap_err()
                .code,
            "INVALID_PARAMS"
        );
    }

    #[test]
    fn tree_reports_a_directory_wider_than_one_listing_as_truncated() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "kept\n").unwrap();
        let wide = root.path().join("generated");
        std::fs::create_dir(&wide).unwrap();
        for index in 0..10_001 {
            std::fs::write(wide.join(format!("{index:05}")), "").unwrap();
        }
        let files = service(root.path());
        let tree = files
            .call("fs.tree", &json!({"repo_id":"repo","path":""}))
            .unwrap();
        assert_eq!(
            tree["truncated"], true,
            "a directory past the listing limit is past the tree's cap"
        );
        assert!(
            tree["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["path"] == "README.md"),
            "what was walked before it is still returned"
        );
    }

    #[test]
    fn watch_directories_prune_what_the_checkout_ignores_but_keep_hidden_paths() {
        let root = tempfile::tempdir().unwrap();
        for (path, content) in [
            (".gitignore", "target/\nnode_modules/\n"),
            ("src/main.rs", ""),
            ("frontend/src/app.ts", ""),
            ("frontend/node_modules/pkg/index.js", ""),
            ("target/debug/deps/g5.rlib", ""),
            (".github/workflows/ci.yml", ""),
        ] {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, content).unwrap();
        }
        let files = service(root.path());
        let mut watched: Vec<String> = files
            .watch_directories("repo", "")
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        watched.sort();
        assert_eq!(
            watched,
            [
                "",
                ".github",
                ".github/workflows",
                "frontend",
                "frontend/src",
                "src",
            ],
            "ignored trees are not watched; hidden directories are"
        );

        // A subtree watch keeps the ancestors and inherits the root's rules.
        let mut watched: Vec<String> = files
            .watch_directories("repo", "frontend")
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        watched.sort();
        assert_eq!(watched, ["", "frontend", "frontend/src"]);
    }

    #[test]
    fn watch_counts_directories_against_its_limit_and_not_files() {
        // More hidden files than the watch limit, in two directories: the
        // tree skips `.cache`, the watch covers it with two registrations.
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "").unwrap();
        for bucket in 0..2 {
            let directory = root.path().join(".cache").join(format!("bucket{bucket}"));
            std::fs::create_dir_all(&directory).unwrap();
            for index in 0..5_001 {
                std::fs::write(directory.join(format!("{index:04}")), "").unwrap();
            }
        }
        let files = service(root.path());
        let mut watched: Vec<String> = files
            .watch_directories("repo", "")
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        watched.sort();
        assert_eq!(
            watched,
            ["", ".cache", ".cache/bucket0", ".cache/bucket1"],
            "ten thousand files do not exhaust a directory watch"
        );
    }

    fn tree_head(files: &FileService) -> Value {
        files
            .call("fs.tree", &json!({"repo_id":"repo","path":""}))
            .unwrap()["head_oid"]
            .clone()
    }

    #[test]
    fn tree_reads_the_checkout_head_through_detached_loose_chained_and_packed_refs() {
        let root = tempfile::tempdir().unwrap();
        let git = root.path().join(".git");
        std::fs::create_dir_all(git.join("refs/heads/feature")).unwrap();
        std::fs::write(root.path().join("README.md"), "").unwrap();
        let files = service(root.path());
        let loose = "a".repeat(40);
        let packed = "b".repeat(40);
        let detached = "c".repeat(64);

        // A symbolic HEAD to a loose ref.
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(git.join("refs/heads/main"), format!("{loose}\n")).unwrap();
        assert_eq!(tree_head(&files), json!(loose));

        // A chain of symbolic refs ends in the loose commit, not a stale
        // packed value for the first name.
        std::fs::write(git.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        std::fs::write(git.join("refs/heads/feature/x"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git.join("packed-refs"),
            format!("# pack-refs with: peeled fully-peeled sorted\n{packed} refs/heads/feature/x\n^{packed}\n"),
        )
        .unwrap();
        assert_eq!(tree_head(&files), json!(loose));

        // No loose file: the packed value, past comments and peeled lines.
        std::fs::remove_file(git.join("refs/heads/feature/x")).unwrap();
        assert_eq!(tree_head(&files), json!(packed));

        // A detached HEAD names the commit itself (SHA-256 here).
        std::fs::write(git.join("HEAD"), format!("{detached}\n")).unwrap();
        assert_eq!(tree_head(&files), json!(detached));

        // An unborn branch, a traversal in the ref name, a symbolic cycle.
        std::fs::write(git.join("HEAD"), "ref: refs/heads/unborn\n").unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::write(git.join("HEAD"), "ref: refs/../../etc/passwd\n").unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::write(git.join("HEAD"), "ref: refs/heads/loop\n").unwrap();
        std::fs::write(git.join("refs/heads/loop"), "ref: refs/heads/loop\n").unwrap();
        assert_eq!(tree_head(&files), Value::Null);
    }

    #[test]
    fn tree_head_refuses_symlinks_fifos_worktree_files_and_oversized_metadata() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "").unwrap();
        let files = service(root.path());
        let oid = "d".repeat(40);

        // A worktree's `.git` is a file, not a directory.
        std::fs::write(
            root.path().join(".git"),
            "gitdir: /elsewhere/.git/worktrees/x\n",
        )
        .unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(root.path().join(".git")).unwrap();

        // `.git` itself a symlink out of the checkout.
        std::fs::create_dir(elsewhere.path().join(".git")).unwrap();
        std::fs::write(elsewhere.path().join(".git/HEAD"), format!("{oid}\n")).unwrap();
        symlink(elsewhere.path().join(".git"), root.path().join(".git")).unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(root.path().join(".git")).unwrap();

        // `HEAD` a symlink, then a FIFO, then a ref component a symlink.
        // A stale packed value for the same ref must never stand in for a
        // loose file that was rejected.
        let git = root.path().join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).unwrap();
        std::fs::write(
            git.join("packed-refs"),
            format!("{} refs/heads/main\n", "e".repeat(40)),
        )
        .unwrap();
        symlink(elsewhere.path().join(".git/HEAD"), git.join("HEAD")).unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(git.join("HEAD")).unwrap();
        let fifo = cstring(git.join("HEAD").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(git.join("HEAD")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        symlink(
            elsewhere.path().join(".git/HEAD"),
            git.join("refs/heads/main"),
        )
        .unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(git.join("refs/heads/main")).unwrap();

        // A ref directory component that is a symlink, with a stale packed
        // value waiting behind it.
        std::fs::write(git.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        std::fs::write(
            git.join("packed-refs"),
            format!("{} refs/heads/feature/x\n", "e".repeat(40)),
        )
        .unwrap();
        std::fs::create_dir_all(elsewhere.path().join("feature")).unwrap();
        std::fs::write(elsewhere.path().join("feature/x"), format!("{oid}\n")).unwrap();
        symlink(
            elsewhere.path().join("feature"),
            git.join("refs/heads/feature"),
        )
        .unwrap();
        assert_eq!(tree_head(&files), Value::Null);
        std::fs::remove_file(git.join("refs/heads/feature")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git.join("packed-refs"),
            format!("{} refs/heads/main\n", "e".repeat(40)),
        )
        .unwrap();

        // Oversized metadata is not read.
        let big = std::fs::File::create(git.join("refs/heads/main")).unwrap();
        big.set_len(MAX_GIT_METADATA_BYTES + 1).unwrap();
        drop(big);
        assert_eq!(tree_head(&files), Value::Null);
    }

    #[test]
    fn unix_socket_is_hidden_without_breaking_directory_listing() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("visible.txt"), b"visible").unwrap();
        let _socket = UnixListener::bind(root.path().join("server.sock")).unwrap();
        let files = service(root.path());
        let listing = files
            .call("fs.list", &json!({"repo_id":"repo","path":""}))
            .unwrap();
        let names: Vec<_> = listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["visible.txt"]);
        for method in ["fs.stat", "fs.read"] {
            assert_eq!(
                files
                    .call(method, &json!({"repo_id":"repo","path":"server.sock"}))
                    .unwrap_err()
                    .code,
                "PATH_FORBIDDEN"
            );
        }
    }

    fn write(service: &FileService, path: &str, content: &str, expected: Value) -> Result<Value> {
        service.call("fs.write", &json!({"repo_id":"repo", "path":path, "content":content, "encoding":"utf-8", "expected_hash":expected}))
    }

    fn default_child_gid(root: &std::path::Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        let probe = root.join(".group-probe");
        std::fs::write(&probe, b"").unwrap();
        let gid = std::fs::metadata(&probe).unwrap().gid();
        std::fs::remove_file(probe).unwrap();
        gid
    }

    #[test]
    fn published_files_carry_the_module_identity_on_create_and_replace() {
        use std::os::unix::fs::MetadataExt;
        // The production identity: uid 1000. Root (the CI lanes) performs
        // the real transition; a developer-run dispatcher is not root and
        // must still publish, leaving the file under its own identity.
        let root = tempfile::tempdir().unwrap();
        // SAFETY: geteuid has no failure modes and touches no memory.
        let (uid, gid) = if unsafe { libc::geteuid() } == 0 {
            (1000, 1000)
        } else {
            (unsafe { libc::geteuid() }, default_child_gid(root.path()))
        };
        let files = service(root.path()).with_owner(Some(1000), Some(1000));

        let created = write(&files, "owned.txt", "first", Value::Null).unwrap();
        let metadata = std::fs::metadata(root.path().join("owned.txt")).unwrap();
        assert_eq!((metadata.uid(), metadata.gid()), (uid, gid), "created file");

        let hash = created["hash"].as_str().unwrap().to_owned();
        write(&files, "owned.txt", "second", json!(hash)).unwrap();
        let metadata = std::fs::metadata(root.path().join("owned.txt")).unwrap();
        assert_eq!(
            (metadata.uid(), metadata.gid()),
            (uid, gid),
            "replaced file"
        );
        assert_eq!(
            std::fs::read(root.path().join("owned.txt")).unwrap(),
            b"second"
        );
    }

    #[test]
    fn created_and_partially_published_directories_carry_the_module_identity() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path()).with_owner(Some(1000), Some(1000));
        // A failed previous create may have left a dispatcher-owned parent.
        std::fs::create_dir(root.path().join("docs")).unwrap();
        write(&files, "docs/workspace/test.md", "document", Value::Null).unwrap();
        let expected = if g5d_foundation::ownership::can_hand_over() {
            (1000, 1000)
        } else {
            (unsafe { libc::geteuid() }, default_child_gid(root.path()))
        };
        for path in ["docs", "docs/workspace", "docs/workspace/test.md"] {
            let meta = std::fs::metadata(root.path().join(path)).unwrap();
            assert_eq!((meta.uid(), meta.gid()), expected, "{path}");
        }
    }

    fn read(service: &FileService, path: &str) -> Value {
        service
            .call("fs.read", &json!({"repo_id":"repo", "path":path}))
            .unwrap()
    }

    #[test]
    fn repository_object_format_hashes_match_native_git() {
        for (format, name) in [
            (ObjectFormat::Sha1, "sha1"),
            (ObjectFormat::Sha256, "sha256"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let initialized = std::process::Command::new("git")
                .args(["init", "--quiet", &format!("--object-format={name}")])
                .arg(root.path())
                .output()
                .unwrap();
            assert!(
                initialized.status.success(),
                "{}",
                String::from_utf8_lossy(&initialized.stderr)
            );
            let bytes = b"\0binary\xff\n";
            let mut child = std::process::Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(["hash-object", "--stdin"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(bytes).unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(result.status.success());
            assert_eq!(
                blob_hash(format, bytes),
                String::from_utf8(result.stdout).unwrap().trim()
            );
            let files = FileService::new(
                vec![RepositoryRoot {
                    repo_id: "repo".into(),
                    root: root.path().to_owned(),
                    object_format: format,
                }],
                vec![],
            )
            .unwrap();
            let result = write(&files, "text", "content", Value::Null).unwrap();
            assert_eq!(result["hash"], blob_hash(format, b"content"));
            assert_eq!(read(&files, "text")["hash"], result["hash"]);
        }
    }

    #[test]
    fn read_windows_carry_the_whole_file_identity() {
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let bytes: Vec<u8> = (0..=255u8).collect();
        std::fs::write(root.path().join("blob.bin"), &bytes).unwrap();
        let whole = read(&files, "blob.bin");
        let window = |offset: u64, length: u64| {
            files.call(
                "fs.read",
                &json!({"repo_id":"repo","path":"blob.bin","offset":offset,"length":length}),
            )
        };
        let first = window(0, 100).unwrap();
        assert_eq!(first["hash"], whole["hash"]);
        assert_eq!(first["size"], 256);
        assert_eq!(first["offset"], 0);
        assert_eq!(first["length"], 100);
        assert_eq!(first["encoding"], "base64");
        assert_eq!(first["file_encoding"], "base64");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(first["content"].as_str().unwrap())
                .unwrap(),
            bytes[..100]
        );
        let last = window(200, 100).unwrap();
        assert_eq!(last["length"], 56);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(last["content"].as_str().unwrap())
                .unwrap(),
            bytes[200..]
        );
        let past = window(300, 10).unwrap();
        assert_eq!(past["length"], 0);
        assert_eq!(past["content"], "");
        // A text file's window is still base64: the caller reassembles bytes.
        std::fs::write(root.path().join("text.txt"), "hello world").unwrap();
        let text = files
            .call(
                "fs.read",
                &json!({"repo_id":"repo","path":"text.txt","offset":6,"length":5}),
            )
            .unwrap();
        assert_eq!(text["file_encoding"], "utf-8");
        assert_eq!(text["encoding"], "base64");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(text["content"].as_str().unwrap())
                .unwrap(),
            b"world"
        );
        for params in [
            json!({"repo_id":"repo","path":"blob.bin","offset":0}),
            json!({"repo_id":"repo","path":"blob.bin","length":10}),
            json!({"repo_id":"repo","path":"blob.bin","offset":0,"length":0}),
            json!({"repo_id":"repo","path":"blob.bin","offset":-1,"length":10}),
        ] {
            assert_eq!(
                files.call("fs.read", &params).unwrap_err().code,
                "INVALID_PARAMS",
                "{params}"
            );
        }
    }

    #[test]
    fn required_create_update_and_conflict_preconditions_preserve_bytes() {
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let missing = files
            .call(
                "fs.write",
                &json!({"repo_id":"repo", "path":"a", "content":"x", "encoding":"utf-8"}),
            )
            .unwrap_err();
        assert_eq!(missing.code, "INVALID_PARAMS");
        assert!(!root.path().join("a").exists());
        assert_eq!(
            write(&files, "a", "x", json!("old")).unwrap_err().code,
            "FILE_MISSING"
        );
        let created = write(&files, "a", "initial", Value::Null).unwrap();
        let exists = write(&files, "a", "discard", Value::Null).unwrap_err();
        assert_eq!(exists.code, "FILE_EXISTS");
        assert_eq!(exists.http_status(), 409);
        assert_eq!(exists.data, json!({"hash":created["hash"],"size":7}));
        let stale = write(&files, "a", "discard", json!("stale")).unwrap_err();
        assert_eq!(stale.code, "FILE_VERSION_CONFLICT");
        assert_eq!(stale.data, exists.data);
        assert_eq!(read(&files, "a")["content"], "initial");
        let updated = write(&files, "a", "changed", created["hash"].clone()).unwrap();
        assert_eq!(updated["hash"], blob_hash(ObjectFormat::Sha1, b"changed"));
        assert_eq!(read(&files, "a")["content"], "changed");
        assert_eq!(
            files
                .call("fs.stat", &json!({"repo_id":"repo","path":"a"}))
                .unwrap()["hash"],
            updated["hash"]
        );
    }

    #[test]
    fn update_preserves_access_mode_without_special_bits() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("script");
        std::fs::write(&path, "before").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o7777)).unwrap();
        let files = service(root.path());

        write(
            &files,
            "script",
            "after",
            json!(blob_hash(ObjectFormat::Sha1, b"before")),
        )
        .unwrap();

        assert_eq!(
            std::fs::metadata(path).unwrap().mode() & 0o7777,
            0o0777,
            "replacement content must not inherit special permission bits"
        );
    }

    #[test]
    fn binary_encoding_round_trip_and_preallocation_caps() {
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let bytes = [0, 1, 0xff, 0xfe, 0x80];
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let result = files.call("fs.write", &json!({"repo_id":"repo","path":"binary","content":encoded,"encoding":"base64","expected_hash":null})).unwrap();
        assert_eq!(result["encoding"], "base64");
        let observed = read(&files, "binary");
        assert_eq!(observed["content"], encoded);
        assert_eq!(observed["hash"], blob_hash(ObjectFormat::Sha1, &bytes));
        assert!(decode_content("", "base64").unwrap().is_empty());
        assert_eq!(
            decode_content("%%%?", "base64").unwrap_err().code,
            "INVALID_PARAMS"
        );
        assert_eq!(
            decode_content("A===", "base64").unwrap_err().code,
            "INVALID_PARAMS"
        );
        assert_eq!(
            decode_content("AB==", "base64").unwrap_err().code,
            "INVALID_PARAMS"
        );
        let cap = "x".repeat(MAX_FILE_BYTES);
        assert_eq!(decode_content(&cap, "utf-8").unwrap().len(), MAX_FILE_BYTES);
        let oversized = format!("{cap}x");
        assert_eq!(
            write(&files, "oversized", &oversized, Value::Null)
                .unwrap_err()
                .code,
            "PAYLOAD_TOO_LARGE"
        );
        assert!(!root.path().join("oversized").exists());
        let encoded = base64::engine::general_purpose::STANDARD.encode(cap);
        assert_eq!(
            decode_content(&encoded, "base64").unwrap().len(),
            MAX_FILE_BYTES
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(oversized);
        assert_eq!(files.call("fs.write",&json!({"repo_id":"repo","path":"oversized","content":encoded,"encoding":"base64","expected_hash":null})).unwrap_err().code,"PAYLOAD_TOO_LARGE");
        assert!(!root.path().join("oversized").exists());
        assert_eq!(read(&files, "binary")["content"], observed["content"]);
    }

    #[test]
    fn delete_and_rename_require_hash_and_never_replace_destination() {
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let created = write(&files, "a", "initial", Value::Null).unwrap();
        write(&files, "destination", "keep", Value::Null).unwrap();
        for method in ["fs.delete", "fs.rename"] {
            let params =
                json!({"repo_id":"repo","path":"a","from":"a","to":"new","expected_hash":null});
            assert_eq!(
                files.call(method, &params).unwrap_err().code,
                "INVALID_PARAMS"
            );
            let params =
                json!({"repo_id":"repo","path":"a","from":"a","to":"new","expected_hash":"stale"});
            assert_eq!(
                files.call(method, &params).unwrap_err().code,
                "FILE_VERSION_CONFLICT"
            );
            assert_eq!(read(&files, "a")["content"], "initial");
        }
        let params =
            json!({"repo_id":"repo","from":"a","to":"destination","expected_hash":created["hash"]});
        let error = files.call("fs.rename", &params).unwrap_err();
        assert_eq!(error.code, "FILE_EXISTS");
        assert_eq!(error.data["hash"], blob_hash(ObjectFormat::Sha1, b"keep"));
        assert_eq!(read(&files, "destination")["content"], "keep");
        files.call("fs.rename",&json!({"repo_id":"repo","from":"a","to":"renamed","expected_hash":created["hash"]})).unwrap();
        assert!(!root.path().join("a").exists());
        assert_eq!(read(&files, "renamed")["content"], "initial");
        files
            .call(
                "fs.delete",
                &json!({"repo_id":"repo","path":"renamed","expected_hash":created["hash"]}),
            )
            .unwrap();
        assert!(!root.path().join("renamed").exists());
    }

    #[test]
    fn empty_directory_operations_and_protected_ancestor_rename() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("empty")).unwrap();
        std::fs::create_dir_all(root.path().join("nonempty")).unwrap();
        std::fs::create_dir_all(root.path().join(".g5/cache")).unwrap();
        std::fs::write(root.path().join("nonempty/file"), "keep").unwrap();
        let files = service(root.path());
        assert_eq!(
            files
                .call("fs.stat", &json!({"repo_id":"repo","path":"empty"}))
                .unwrap()["type"],
            "directory"
        );
        files
            .call(
                "fs.rename",
                &json!({"repo_id":"repo","from":"empty","to":"moved","expected_hash":null}),
            )
            .unwrap();
        files
            .call(
                "fs.delete",
                &json!({"repo_id":"repo","path":"moved","expected_hash":null}),
            )
            .unwrap();
        assert_eq!(
            files
                .call(
                    "fs.delete",
                    &json!({"repo_id":"repo","path":"nonempty","expected_hash":null})
                )
                .unwrap_err()
                .code,
            "DIRECTORY_NOT_EMPTY"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("nonempty/file")).unwrap(),
            "keep"
        );
        assert_eq!(
            files
                .call(
                    "fs.rename",
                    &json!({"repo_id":"repo","from":".g5","to":"exposed","expected_hash":null})
                )
                .unwrap_err()
                .code,
            "PATH_FORBIDDEN"
        );
    }

    #[test]
    fn directory_rename_cannot_create_runtime_ancestor_but_product_paths_remain_writable() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("ordinary/cache")).unwrap();
        std::fs::write(root.path().join("ordinary/cache/content"), "original").unwrap();
        let files = service(root.path());
        for destination in [".g5", ".G5"] {
            assert_eq!(files.call("fs.rename",&json!({"repo_id":"repo","from":"ordinary","to":destination,"expected_hash":null})).unwrap_err().code,"PATH_FORBIDDEN");
            assert!(!root.path().join(destination).exists());
            assert_eq!(
                std::fs::read_to_string(root.path().join("ordinary/cache/content")).unwrap(),
                "original"
            );
        }
        std::fs::create_dir(root.path().join(".g5")).unwrap();
        let saved = write(&files, ".g5/workspace.yaml", "name: product", Value::Null).unwrap();
        files.call("fs.rename",&json!({"repo_id":"repo","from":".g5/workspace.yaml","to":".g5/product.yaml","expected_hash":saved["hash"]})).unwrap();
        files.call("fs.rename",&json!({"repo_id":"repo","from":"ordinary","to":".g5/preview_profiles","expected_hash":null})).unwrap();
        assert_eq!(
            read(&files, ".g5/preview_profiles/cache/content")["content"],
            "original"
        );
    }

    #[test]
    fn every_path_surface_refuses_metadata_runtime_absolute_parent_and_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "private").unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        std::fs::create_dir_all(root.path().join(".g5/cache")).unwrap();
        std::fs::write(root.path().join(".git/config"), "secret git").unwrap();
        std::fs::write(root.path().join(".g5/cache/token"), "secret runtime").unwrap();
        std::fs::write(root.path().join(".g5/workspace.yaml"), "product").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        symlink(outside.path().join("secret"), root.path().join("link")).unwrap();
        let files = service(root.path());
        let paths = [
            ".git/config",
            ".GiT/config",
            ".g5/cache/token",
            ".G5/CACHE/token",
            ".g5-dispatch-tmp-secret",
            ".G5-DISPATCH-TMP-secret",
            "/tmp/secret",
            "../secret",
            "a/../secret",
            "escape/secret",
            "link",
            "./link",
            "a//b",
        ];
        for path in paths {
            for method in ["fs.read", "fs.stat", "fs.list", "fs.delete"] {
                let result = files.call(
                    method,
                    &json!({"repo_id":"repo","path":path,"expected_hash":"stale"}),
                );
                assert!(result.is_err(), "{method} {path}");
            }
            assert!(files.validate_watch("repo", path).is_err(), "watch {path}");
            assert!(
                write(&files, path, "bad", Value::Null).is_err(),
                "create {path}"
            );
            assert!(
                write(
                    &files,
                    path,
                    "bad",
                    json!(blob_hash(ObjectFormat::Sha1, b"private"))
                )
                .is_err(),
                "update {path}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(outside.path().join("secret")).unwrap(),
            "private"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(".git/config")).unwrap(),
            "secret git"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(".g5/cache/token")).unwrap(),
            "secret runtime"
        );
        let listing = files
            .call("fs.list", &json!({"repo_id":"repo","path":""}))
            .unwrap();
        let names: Vec<_> = listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec![".g5"]);
        assert_eq!(read(&files, ".g5/workspace.yaml")["content"], "product");
        assert_eq!(
            files
                .call("fs.read", &json!({"repo_id":"another","path":"secret"}))
                .unwrap_err()
                .code,
            "PATH_FORBIDDEN"
        );
    }

    #[test]
    fn concurrent_create_and_same_hash_updates_have_one_winner() {
        let root = tempfile::tempdir().unwrap();
        let files = Arc::new(service(root.path()));
        for (path, expected) in [
            ("create", Value::Null),
            (
                "update",
                write(&files, "update", "before", Value::Null).unwrap()["hash"].clone(),
            ),
        ] {
            let barrier = Arc::new(Barrier::new(3));
            let mut threads = Vec::new();
            for text in ["first", "second"] {
                let files = files.clone();
                let barrier = barrier.clone();
                let expected = expected.clone();
                threads.push(std::thread::spawn(move || {
                    barrier.wait();
                    write(&files, path, text, expected)
                }));
            }
            barrier.wait();
            let results: Vec<_> = threads
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect();
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
            let winner = results
                .iter()
                .find_map(|result| result.as_ref().ok())
                .unwrap();
            assert_eq!(read(&files, path)["hash"], winner["hash"]);
        }
    }

    #[test]
    fn create_makes_missing_parent_directories_but_update_does_not() {
        // A fork's Project Docs live under docs/<workspace-id>/, which does not
        // exist in a fresh checkout; the first create must make it.
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let created = write(
            &files,
            "docs/new-workspace/first.md",
            "# one\n",
            Value::Null,
        )
        .expect("create makes the missing folders");
        assert_eq!(created["hash"], blob_hash(ObjectFormat::Sha1, b"# one\n"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("docs/new-workspace/first.md")).unwrap(),
            "# one\n"
        );
        assert_eq!(
            read(&files, "docs/new-workspace/first.md")["content"],
            "# one\n"
        );
        // Creating beside it reuses the folder; the file itself must not exist.
        write(&files, "docs/new-workspace/second.md", "two", Value::Null).unwrap();
        assert_eq!(
            write(&files, "docs/new-workspace/second.md", "again", Value::Null)
                .unwrap_err()
                .code,
            "FILE_EXISTS"
        );
        // An update with an expected hash never creates directories.
        assert_eq!(
            write(
                &files,
                "docs/other/missing.md",
                "x",
                json!(blob_hash(ObjectFormat::Sha1, b"x"))
            )
            .unwrap_err()
            .code,
            "FILE_MISSING"
        );
        assert!(!root.path().join("docs/other").exists());
        // Protected and excluded components stay forbidden on the create path.
        assert_eq!(
            write(&files, ".git/hooks/pre-commit", "x", Value::Null)
                .unwrap_err()
                .code,
            "PATH_FORBIDDEN"
        );
        // A symlinked intermediate component is never followed to create.
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        assert_eq!(
            write(&files, "escape/deeper/file", "x", Value::Null)
                .unwrap_err()
                .code,
            "PATH_FORBIDDEN"
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn symlink_swap_during_create_cannot_write_outside_member() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("directory")).unwrap();
        let files = Arc::new(service(root.path()));
        let barrier = Arc::new(Barrier::new(2));
        let root_path = root.path().to_owned();
        let outside_path = outside.path().to_owned();
        let swap_barrier = barrier.clone();
        let swapper = std::thread::spawn(move || {
            swap_barrier.wait();
            // A create may legitimately mint an empty `directory` while it is
            // parked (creates make missing parents), so each step is
            // best-effort; the property under test is only that no write ever
            // lands outside the member root.
            for _ in 0..1000 {
                let _ = std::fs::rename(root_path.join("directory"), root_path.join("parked"));
                let _ = std::fs::remove_dir(root_path.join("directory"));
                let _ = symlink(&outside_path, root_path.join("directory"));
                std::thread::yield_now();
                let _ = std::fs::remove_file(root_path.join("directory"));
                let _ = std::fs::remove_dir(root_path.join("directory"));
                let _ = std::fs::rename(root_path.join("parked"), root_path.join("directory"));
            }
        });
        barrier.wait();
        for number in 0..1000 {
            let _ = write(
                &files,
                &format!("directory/file-{number}"),
                "inside",
                Value::Null,
            );
        }
        swapper.join().unwrap();
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn atomic_update_does_not_mutate_other_hardlink_names() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let sources = [
            root.path().join(".git/config"),
            outside.path().join("secret"),
        ];
        let files = service(root.path());
        for (index, source) in sources.iter().enumerate() {
            std::fs::write(source, "protected").unwrap();
            let path = format!("link-{index}");
            std::fs::hard_link(source, root.path().join(&path)).unwrap();
            let result = write(
                &files,
                &path,
                "updated",
                json!(blob_hash(ObjectFormat::Sha1, b"protected")),
            )
            .unwrap();
            assert_eq!(result["hash"], blob_hash(ObjectFormat::Sha1, b"updated"));
            assert_eq!(std::fs::read_to_string(source).unwrap(), "protected");
        }
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".g5-dispatch-tmp-")));
    }

    #[test]
    fn failed_staging_write_preserves_destination_and_cleans_temporary_entry() {
        const ROOT_ENV: &str = "G5D_FILE_STAGE_FAULT_ROOT";
        if let Some(root) = std::env::var_os(ROOT_ENV) {
            // The subprocess confines the actual kernel I/O fault to this test.
            // Both create and update must survive a partially written staging file.
            let root = PathBuf::from(root);
            let files = service(&root);
            let mut previous = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            assert_eq!(
                unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut previous) },
                0
            );
            let limited = libc::rlimit {
                rlim_cur: 64,
                rlim_max: previous.rlim_max,
            };
            unsafe {
                libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            }
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limited) }, 0);
            let update = write(
                &files,
                "existing",
                &"x".repeat(1024),
                json!(blob_hash(ObjectFormat::Sha1, b"original")),
            );
            let create = write(&files, "absent", &"x".repeat(1024), Value::Null);
            let exists = write(&files, "existing", &"x".repeat(1024), Value::Null);
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &previous) }, 0);
            assert_eq!(update.unwrap_err().code, "IO_ERROR");
            assert_eq!(create.unwrap_err().code, "IO_ERROR");
            assert_eq!(exists.unwrap_err().code, "FILE_EXISTS");
            assert_eq!(
                std::fs::read_to_string(root.join("existing")).unwrap(),
                "original"
            );
            assert!(!root.join("absent").exists());
            assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("existing"), "original").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","files::tests::failed_staging_write_preserves_destination_and_cleans_temporary_entry","--nocapture"])
            .env(ROOT_ENV,root.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn stat_streams_native_files_above_upload_cap_and_utf8_crossing_chunks() {
        let root = tempfile::tempdir().unwrap();
        let files = service(root.path());
        let path = root.path().join("large");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
        let stat = files
            .call("fs.stat", &json!({"repo_id":"repo","path":"large"}))
            .unwrap();
        assert_eq!(stat["size"], MAX_FILE_BYTES + 1);
        assert_eq!(stat["encoding"], "utf-8");
        let mut bytes = vec![b'a'; 64 * 1024 - 1];
        bytes.extend_from_slice("🦀".as_bytes());
        std::fs::write(root.path().join("utf8"), &bytes).unwrap();
        let stat = files
            .call("fs.stat", &json!({"repo_id":"repo","path":"utf8"}))
            .unwrap();
        assert_eq!(stat["encoding"], "utf-8");
        assert_eq!(stat["hash"], blob_hash(ObjectFormat::Sha1, &bytes));
        bytes.pop();
        std::fs::write(root.path().join("utf8"), &bytes).unwrap();
        assert_eq!(
            files
                .call("fs.stat", &json!({"repo_id":"repo","path":"utf8"}))
                .unwrap()["encoding"],
            "base64"
        );
    }
}
