//! Incarnation-scoped dispatcher configuration. No workspace file supplies identity.

use std::collections::HashSet;
use std::fmt;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::files::{ObjectFormat, RepositoryRoot};
use g5_internal_api::host_ctl::VmRestoreSpec;

pub use g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION;

/// Reserved VM artifacts, shared by the file API and native Git's excludes file.
/// Product configuration under `.g5` is deliberately outside this list.
pub const RUNTIME_EXCLUDES: &[&str] = &[
    ".g5/cache",
    ".g5/runtime",
    ".g5/state",
    ".g5/logs",
    ".g5/tmp",
    ".g5/sessions",
    ".g5/gitconfig",
    ".g5/gitconfig.host",
    ".g5/layout.json",
];

const DEFAULT_PTY_REATTACH_GRACE: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
#[error("invalid dispatcher configuration: {0}")]
pub struct ConfigError(pub String);

#[derive(Clone, Debug)]
pub struct OwnerGitIdentity {
    pub name: String,
    pub email: String,
}

/// Environment a process-mode host sets so this dispatcher runs outside a
/// guest (`g5.runtime.boot.backend.process-local`). Honoured only when
/// [`PROCESS_MODE_ENV`] is `process`; a guest keeps its hardcoded paths.
pub const PROCESS_MODE_ENV: &str = "G5D_ISOLATION_MODE";
pub const PROCESS_WORKSPACE_ROOT_ENV: &str = "G5D_PROCESS_WORKSPACE_ROOT";
pub const PROCESS_HOME_ENV: &str = "G5D_PROCESS_HOME";
pub const PROCESS_STATE_DIR_ENV: &str = "G5D_PROCESS_STATE_DIR";
pub const PROCESS_RUNTIME_ROOT_ENV: &str = "G5D_PROCESS_RUNTIME_ROOT";
pub use g5d_foundation::runtime_environment::PROCESS_SOCKET_DIR_ENV;

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub workspace_root: PathBuf,
    pub home: PathBuf,
    pub state_dir: PathBuf,
    /// The read-only runtime bundle: `bin/` executables, `modules/catalog.json`
    /// and `assets/harness`. `/opt/g5` in a guest; the host-native runtime
    /// directory on a process-mode host.
    pub runtime_root: PathBuf,
    /// Where the workspace Git socket is bound: the state directory in a
    /// guest; on a process-mode host a short owner-only directory the host
    /// chose, because a socket path is bounded and a checkout path is not.
    pub socket_dir: PathBuf,
}

impl RuntimePaths {
    fn production() -> Self {
        Self {
            workspace_root: PathBuf::from("/work"),
            home: PathBuf::from("/home/agent"),
            state_dir: PathBuf::from("/var/lib/g5d"),
            runtime_root: PathBuf::from("/opt/g5"),
            socket_dir: PathBuf::from("/var/lib/g5d"),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn under_test_root(root: &Path) -> Self {
        Self {
            workspace_root: root.join("work"),
            home: root.join("home/agent"),
            state_dir: root.join("var/lib/g5d"),
            runtime_root: root.join("opt/g5"),
            socket_dir: root.join("var/lib/g5d"),
        }
    }

    /// The paths a process-mode host names explicitly. Every path must be
    /// absolute; the workspace and runtime roots must already exist (the host
    /// materialized the checkout and validated the runtime directory), while
    /// the home and state directories are created by the dispatcher.
    fn from_process_env(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let absolute = |key: &str| -> Result<PathBuf, ConfigError> {
            let value = get(key)
                .ok_or_else(|| ConfigError(format!("{key} is required in process mode")))?;
            validate_value(key, &value)?;
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(ConfigError(format!("{key} must be an absolute path")));
            }
            Ok(path)
        };
        let paths = Self {
            workspace_root: absolute(PROCESS_WORKSPACE_ROOT_ENV)?,
            home: absolute(PROCESS_HOME_ENV)?,
            state_dir: absolute(PROCESS_STATE_DIR_ENV)?,
            runtime_root: absolute(PROCESS_RUNTIME_ROOT_ENV)?,
            socket_dir: absolute(PROCESS_SOCKET_DIR_ENV)?,
        };
        for (key, path) in [
            (PROCESS_WORKSPACE_ROOT_ENV, &paths.workspace_root),
            (PROCESS_RUNTIME_ROOT_ENV, &paths.runtime_root),
            (PROCESS_SOCKET_DIR_ENV, &paths.socket_dir),
        ] {
            if !path.is_dir() {
                return Err(ConfigError(format!(
                    "{key} must name an existing directory: {}",
                    path.display()
                )));
            }
        }
        Ok(paths)
    }

    /// Where the pinned module catalog lives inside the runtime bundle.
    pub fn catalog_path(&self) -> PathBuf {
        self.runtime_root.join("modules/catalog.json")
    }

    /// The runtime bundle's executables (provider CLIs, shell tools).
    pub fn runtime_bin_dir(&self) -> PathBuf {
        self.runtime_root.join("bin")
    }

    /// The provided agent harness inside the runtime bundle.
    pub fn harness_dir(&self) -> PathBuf {
        self.runtime_root.join("assets/harness")
    }

    /// The workspace Git socket every module, terminal and `g5d` CLI child
    /// dials.
    pub fn workspace_git_socket(&self) -> PathBuf {
        self.socket_dir
            .join(g5_contracts::workspace_git::SOCKET_FILE_NAME)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceLayout {
    pub repos: Vec<RepositoryLayout>,
    #[serde(default)]
    pub restore: Option<VmRestoreSpec>,
    /// Reference checkouts beside the members: a personal workspace's
    /// checkouts of Base (`g5.workspace.identity.reference`), or the owner's
    /// personal repository mounted in another workspace (`g5.personal.mount`).
    /// They get an origin remote, but are never captured, flushed or reported
    /// as members; only the mount's personal branch accepts pushes, which the
    /// control plane admits.
    #[serde(default)]
    pub references: Vec<ReferenceLayout>,
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLayout {
    pub repo_id: Uuid,
    pub root_path: String,
    #[serde(default)]
    pub role: Option<String>,
    /// The admitted internal origin as the layout persists it and as the
    /// checkout's `remote.origin.url` names it.
    pub git_url: String,
    pub branch: String,
    pub commit: String,
    /// Where this guest reaches `git_url`, when the two differ; see
    /// [`RepositoryLayout::door_url`].
    #[serde(skip)]
    pub(crate) door_url: Option<String>,
}

impl ReferenceLayout {
    /// The relay's upstream for this reference: the door-mapped origin in a
    /// guest, otherwise `git_url` itself.
    pub fn transport_url(&self) -> &str {
        self.door_url.as_deref().unwrap_or(&self.git_url)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryLayout {
    pub repo_id: Uuid,
    pub root_path: String,
    #[serde(default)]
    pub role: Option<String>,
    /// The admitted internal origin as the layout persists it and as the
    /// checkout's `remote.origin.url` names it. Native Git routing keys on
    /// this URL (`insteadOf`); the relay never contacts it directly in a
    /// guest whose layout names the host loopback.
    pub git_url: String,
    pub branch: String,
    pub branch_oid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_branch: Option<String>,
    #[serde(default = "default_object_format")]
    pub object_format: ObjectFormat,
    /// Tracking refreshes fast-forward this checkout to its moved branch
    /// (`g5.personal.follow`): set only for a personal workspace's private
    /// repository, whose branch the owner's other workspaces also push.
    #[serde(default)]
    pub follow_branch: bool,
    /// Where this guest reaches `git_url` when the layout names the host
    /// loopback: the same host door as its API connection
    /// (`g5.architecture.microvm_backend`). Set by [`DispatchConfig`] loading
    /// in a guest, before the exact-route validation that admits it; a
    /// process-mode host or a non-loopback origin leaves it unset. The
    /// relay's upstream, never a checkout's origin (G5Labs/G5-Lean#1020).
    #[serde(skip)]
    pub(crate) door_url: Option<String>,
}

impl RepositoryLayout {
    /// The relay's upstream for this member: the door-mapped origin in a
    /// guest, otherwise `git_url` itself.
    pub fn transport_url(&self) -> &str {
        self.door_url.as_deref().unwrap_or(&self.git_url)
    }
}

fn default_object_format() -> ObjectFormat {
    ObjectFormat::Sha1
}

/// The owner's personal repository as this layout checks it out, the home
/// of agent memory (`g5.personal.memory`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalRepository {
    pub repo_id: Uuid,
    pub root_path: String,
    /// The checkout's origin ([`RepositoryLayout::git_url`]).
    pub git_url: String,
    /// The relay's upstream for its pushes ([`RepositoryLayout::transport_url`]).
    pub transport_url: String,
    pub branch: String,
}

/// Where the owner's personal repository is checked out (`g5.personal.mount`).
pub const PERSONAL_ROOT_PATH: &str = "personal";

impl WorkspaceLayout {
    /// The personal workspace's own member (the one that follows its branch),
    /// or else the mount: the reference checkout at `personal`, the only
    /// reference an ordinary workspace carries. `None` when Start left it out.
    pub fn personal_repository(&self) -> Option<PersonalRepository> {
        if let Some(repo) = self.repos.iter().find(|repo| repo.follow_branch) {
            return Some(PersonalRepository {
                repo_id: repo.repo_id,
                root_path: repo.root_path.clone(),
                git_url: repo.git_url.clone(),
                transport_url: repo.transport_url().to_owned(),
                branch: repo.branch.clone(),
            });
        }
        self.references
            .iter()
            .find(|reference| reference.root_path == PERSONAL_ROOT_PATH)
            .map(|reference| PersonalRepository {
                repo_id: reference.repo_id,
                root_path: reference.root_path.clone(),
                git_url: reference.git_url.clone(),
                transport_url: reference.transport_url().to_owned(),
                branch: reference.branch.clone(),
            })
    }
}

#[derive(Clone)]
pub struct DispatchConfig {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub owner_user_id: Uuid,
    pub epoch: u64,
    pub workspace_token: String,
    pub listener_token: [u8; 32],
    pub api_url: Url,
    pub dispatch_port: u16,
    pub lease_ttl_secs: u64,
    pub owner_git: OwnerGitIdentity,
    pub paths: RuntimePaths,
    pub layout: WorkspaceLayout,
    /// Host-configured origin of the exact-route bootstrap broker (Agent API
    /// §6.6). Absent means `workspace.bootstrap` cannot submit a graph.
    /// Unprivileged identity for module processes and their descendants when
    /// the host provisioned one (`G5D_MODULE_UID`/`G5D_MODULE_GID`); absent
    /// means the guest runs modules under the dispatcher's own identity.
    pub module_uid: Option<u32>,
    pub module_gid: Option<u32>,
    /// Guest loopback port of the internal Git transport door
    /// (`G5D_GIT_DOOR_PORT`, default [`crate::git_transport::GIT_DOOR_GUEST_PORT`]).
    /// Test harnesses that run several dispatchers on one host pass 0.
    pub git_door_port: u16,
    /// How long a detached browser terminal remains available for reattach.
    pub pty_reattach_grace: Duration,
    /// This dispatcher is a host process on a local process-mode host rather
    /// than a guest (`g5.runtime.boot.backend.process-local`): it listens on
    /// the host loopback only, takes its paths from the host, and stamps no
    /// guest-only loader path or standard-path symlink.
    pub process_mode: bool,
}

impl fmt::Debug for DispatchConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchConfig")
            .field("workspace_id", &self.workspace_id)
            .field("vm_id", &self.vm_id)
            .field("owner_user_id", &self.owner_user_id)
            .field("epoch", &self.epoch)
            .field("workspace_token", &"[REDACTED]")
            .field("listener_token", &"[REDACTED]")
            .field("api_url", &self.api_url)
            .field("dispatch_port", &self.dispatch_port)
            .field("paths", &self.paths)
            .field("process_mode", &self.process_mode)
            .finish_non_exhaustive()
    }
}

impl DispatchConfig {
    /// Dispatcher-owned, atomically replaced workspace credential. Kept outside
    /// the checkout and all snapshots; children receive only this stable path.
    pub fn workspace_token_file(&self) -> PathBuf {
        self.paths.state_dir.join("workspace-token")
    }

    /// Where the listener binds: a guest serves its forwarded port on every
    /// interface; a host process must not expose the dispatcher beyond the
    /// host loopback.
    pub fn listen_addr(&self) -> std::net::IpAddr {
        if self.process_mode {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        } else {
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
        }
    }

    pub fn publish_workspace_token(&self, token: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut staged = tempfile::NamedTempFile::new_in(&self.paths.state_dir)?;
        staged.write_all(token.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::{fs::PermissionsExt, io::AsRawFd};
            // The dispatcher retains write ownership. Agent/terminal identity
            // can only read; unrelated users cannot read this credential.
            if let Some(gid) = self.module_gid {
                if unsafe { libc::fchown(staged.as_raw_fd(), !0, gid) } != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            staged
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o440))?;
        }
        staged
            .persist(self.workspace_token_file())
            .map_err(|error| error.error)?;
        Ok(())
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let required = |key: &str| -> Result<String, ConfigError> {
            let value = get(key).ok_or_else(|| ConfigError(format!("{key} is required")))?;
            validate_value(key, &value)?;
            Ok(value)
        };
        let uuid = |key: &str| -> Result<Uuid, ConfigError> {
            Uuid::parse_str(&required(key)?)
                .map_err(|_| ConfigError(format!("{key} must be a UUID")))
        };
        let epoch = required("G5D_EPOCH")?
            .parse::<u64>()
            .map_err(|_| ConfigError("G5D_EPOCH must be a positive integer".into()))?;
        if epoch == 0 {
            return Err(ConfigError("G5D_EPOCH must be positive".into()));
        }
        let dispatch_port = required("G5D_DISPATCH_PORT")?
            .parse::<u16>()
            .map_err(|_| ConfigError("G5D_DISPATCH_PORT must be a nonzero port".into()))?;
        if dispatch_port == 0 {
            return Err(ConfigError("G5D_DISPATCH_PORT must be nonzero".into()));
        }
        let lease_ttl_secs = get("G5D_LEASE_TTL_SECS")
            .unwrap_or_else(|| "60".into())
            .parse::<u64>()
            .map_err(|_| ConfigError("G5D_LEASE_TTL_SECS must be an integer".into()))?;
        if !(1..=86400).contains(&lease_ttl_secs) {
            return Err(ConfigError(
                "G5D_LEASE_TTL_SECS must be between 1 and 86400".into(),
            ));
        }
        let api_url = Url::parse(&required("G5D_API_URL")?)
            .map_err(|_| ConfigError("G5D_API_URL must be an HTTP(S) URL".into()))?;
        validate_api_url(&api_url, "G5D_API_URL")?;
        // The module identity is the documented unprivileged one: root is
        // refused outright rather than handed to every repository-facing
        // child on a privileged host.
        let identity = |key: &str| -> Result<Option<u32>, ConfigError> {
            match get(key) {
                None => Ok(None),
                Some(value) => {
                    let id = value
                        .parse::<u32>()
                        .map_err(|_| ConfigError(format!("{key} must be an unsigned integer")))?;
                    if id == 0 {
                        return Err(ConfigError(format!("{key} must name an unprivileged id")));
                    }
                    Ok(Some(id))
                }
            }
        };
        let module_uid = identity("G5D_MODULE_UID")?;
        let module_gid = identity("G5D_MODULE_GID")?;
        let git_door_port = match get("G5D_GIT_DOOR_PORT") {
            None => crate::git_transport::GIT_DOOR_GUEST_PORT,
            Some(value) => value
                .parse::<u16>()
                .map_err(|_| ConfigError("G5D_GIT_DOOR_PORT must be a port number".into()))?,
        };
        let pty_reattach_grace = get("G5D_PTY_REATTACH_GRACE_MS")
            .and_then(|value| value.parse::<u64>().ok())
            .map(|millis| Duration::from_millis(millis.clamp(100, 60_000)))
            .unwrap_or(DEFAULT_PTY_REATTACH_GRACE);
        let token_hex = required("G5D_DISPATCH_TOKEN")?;
        if token_hex.len() != 64 || !token_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ConfigError(
                "G5D_DISPATCH_TOKEN must be 32 bytes in hex".into(),
            ));
        }
        let mut listener_token = [0; 32];
        for (index, byte) in listener_token.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&token_hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| ConfigError("invalid listener token".into()))?;
        }

        let process_mode = get(PROCESS_MODE_ENV).as_deref() == Some("process");
        let paths = match get("G5D_DISPATCH_TEST_ROOT") {
            _ if process_mode => RuntimePaths::from_process_env(&get)?,
            None => RuntimePaths::production(),
            Some(root) => {
                #[cfg(any(test, feature = "test-support"))]
                {
                    let root = Path::new(&root);
                    if !root.is_absolute() {
                        return Err(ConfigError("test root must be absolute".into()));
                    }
                    RuntimePaths::under_test_root(root)
                }
                #[cfg(not(any(test, feature = "test-support")))]
                {
                    let _ = root;
                    return Err(ConfigError(
                        "G5D_DISPATCH_TEST_ROOT requires a test-support build".into(),
                    ));
                }
            }
        };
        let layout_path = paths.workspace_root.join(".g5/layout.json");
        let mut bytes = Vec::new();
        std::fs::File::open(&layout_path)
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|e| ConfigError(format!("cannot read workspace layout: {e}")))?;
        if bytes.len() > 1024 * 1024 {
            return Err(ConfigError("workspace layout exceeds 1 MiB".into()));
        }
        let mut layout: WorkspaceLayout = serde_json::from_slice(&bytes)
            .map_err(|e| ConfigError(format!("invalid workspace layout: {e}")))?;
        // Persisted membership names the admitted endpoints, and each
        // checkout's origin names the same URL. Only the guest's transport
        // reaches a loopback origin through the host door its API connection
        // uses: it is carried beside `git_url`, never in its place, so native
        // Git routing still keys on the checkout's real origin
        // (G5Labs/G5-Lean#1020). Keep the exact origin/route validation
        // below on the transport view and never rewrite the file.
        if !process_mode
            && api_url.host_str() == Some(g5d_foundation::runtime_environment::HOST_DOOR_ALIAS)
        {
            for repo in &mut layout.repos {
                repo.door_url = door_url_for(&repo.git_url, "member")?;
            }
            for reference in &mut layout.references {
                reference.door_url = door_url_for(&reference.git_url, "reference")?;
            }
        }
        validate_layout(&layout, &paths.workspace_root, &api_url)?;

        Ok(Self {
            workspace_id: uuid("G5D_WORKSPACE_ID")?,
            vm_id: uuid("G5D_VM_ID")?,
            owner_user_id: uuid("G5D_OWNER_USER_ID")?,
            epoch,
            workspace_token: required("G5D_WORKSPACE_TOKEN")?,
            listener_token,
            api_url,
            dispatch_port,
            lease_ttl_secs,
            owner_git: OwnerGitIdentity {
                name: required("G5D_OWNER_GIT_NAME")?,
                email: required("G5D_OWNER_GIT_EMAIL")?,
            },
            paths,
            layout,
            module_uid,
            module_gid,
            git_door_port,
            pty_reattach_grace,
            process_mode,
        })
    }

    pub fn repo_roots(&self) -> Vec<RepositoryRoot> {
        self.layout
            .repos
            .iter()
            .map(|repo| RepositoryRoot {
                repo_id: repo.repo_id.to_string(),
                root: self.paths.workspace_root.join(&repo.root_path),
                object_format: repo.object_format,
            })
            .collect()
    }

    pub fn runtime_excludes(&self) -> Vec<String> {
        RUNTIME_EXCLUDES.iter().map(|path| (*path).into()).collect()
    }
}

fn validate_value(key: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() || value.len() > 16 * 1024 || value.chars().any(char::is_control) {
        return Err(ConfigError(format!(
            "{key} is empty, oversized or contains control characters"
        )));
    }
    Ok(())
}

fn validate_api_url(url: &Url, variable: &str) -> Result<(), ConfigError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(ConfigError(format!(
            "{variable} must be an HTTP(S) origin without credentials"
        )));
    }
    Ok(())
}

/// The host-door view of a loopback layout origin, `None` when the origin
/// needs no door (see [`RepositoryLayout::door_url`]).
fn door_url_for(git_url: &str, what: &str) -> Result<Option<String>, ConfigError> {
    g5d_foundation::runtime_environment::rewrite_loopback_url_for_door(git_url)
        .map(|url| {
            Url::parse(&url)
                .map(Into::into)
                .map_err(|_| ConfigError(format!("invalid mapped {what} URL")))
        })
        .transpose()
}

fn validate_layout(
    layout: &WorkspaceLayout,
    root: &Path,
    api_url: &Url,
) -> Result<(), ConfigError> {
    if layout.repos.is_empty() || layout.repos.len() > 256 {
        return Err(ConfigError(
            "layout must contain 1 to 256 repositories".into(),
        ));
    }
    let mut ids = HashSet::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    for repo in &layout.repos {
        let path = Path::new(&repo.root_path);
        if repo.root_path.is_empty()
            || repo.root_path.contains('\\')
            || repo
                .root_path
                .split('/')
                .any(|part| part.is_empty() || part == ".")
            || path.components().any(
                |part| !matches!(part, Component::Normal(name) if name != ".git" && name != ".g5"),
            )
            || !ids.insert(repo.repo_id)
            || roots
                .iter()
                .any(|other| path.starts_with(other) || other.starts_with(path))
        {
            return Err(ConfigError(
                "layout requires unique nonoverlapping normalized member roots".into(),
            ));
        }
        validate_value("root_path", &repo.root_path)?;
        validate_value("branch", &repo.branch)?;
        if let Some(upstream) = &repo.upstream_branch {
            validate_value("upstream_branch", upstream)?;
        }
        if repo.branch != layout.repos[0].branch {
            return Err(ConfigError(
                "all member repositories must use the same working branch".into(),
            ));
        }
        let expected = api_url
            .join(&format!("git/{}", repo.repo_id))
            .map_err(|_| ConfigError("cannot derive internal Git URL".into()))?;
        if repo.transport_url() != expected.as_str() {
            return Err(ConfigError(
                "member Git URL must name its exact internal Git route".into(),
            ));
        }
        let mut current = root.to_path_buf();
        for component in path.components() {
            current.push(component);
            let meta = std::fs::symlink_metadata(&current)
                .map_err(|e| ConfigError(format!("cannot inspect member root: {e}")))?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(ConfigError(
                    "member roots must be physical directories".into(),
                ));
            }
        }
        roots.push(path.to_path_buf());
    }
    for reference in &layout.references {
        let path = Path::new(&reference.root_path);
        if reference.root_path.is_empty()
            || reference.root_path.contains('\\')
            || reference
                .root_path
                .split('/')
                .any(|part| part.is_empty() || part == ".")
            || path.components().any(
                |part| !matches!(part, Component::Normal(name) if name != ".git" && name != ".g5"),
            )
            || !ids.insert(reference.repo_id)
            || roots
                .iter()
                .any(|other| path.starts_with(other) || other.starts_with(path))
        {
            return Err(ConfigError(
                "layout requires unique nonoverlapping normalized reference roots".into(),
            ));
        }
        validate_value("root_path", &reference.root_path)?;
        validate_value("branch", &reference.branch)?;
        validate_value("commit", &reference.commit)?;
        if !matches!(reference.commit.len(), 40 | 64)
            || !reference.commit.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ConfigError(
                "reference commit must be an exact object id".into(),
            ));
        }
        let expected = api_url
            .join(&format!("git/{}", reference.repo_id))
            .map_err(|_| ConfigError("cannot derive internal Git URL".into()))?;
        if reference.transport_url() != expected.as_str() {
            return Err(ConfigError(
                "reference Git URL must name its exact internal Git route".into(),
            ));
        }
        // The dispatcher configures each reference's origin in place, so a
        // root the guest replaced with a symlink is refused, as for members.
        let mut current = root.to_path_buf();
        for component in path.components() {
            current.push(component);
            let meta = std::fs::symlink_metadata(&current)
                .map_err(|e| ConfigError(format!("cannot inspect reference root: {e}")))?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(ConfigError(
                    "reference roots must be physical directories".into(),
                ));
            }
        }
        roots.push(path.to_path_buf());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fixture() -> (tempfile::TempDir, HashMap<String, String>) {
        let temp = tempfile::tempdir().unwrap();
        let paths = RuntimePaths::under_test_root(temp.path());
        std::fs::create_dir_all(paths.workspace_root.join(".g5")).unwrap();
        std::fs::create_dir(paths.workspace_root.join("code")).unwrap();
        let repo_id = Uuid::new_v4();
        std::fs::write(paths.workspace_root.join(".g5/layout.json"), serde_json::to_vec(&serde_json::json!({"repos":[{
            "repo_id":repo_id,"root_path":"code","git_url":format!("https://api.test/git/{repo_id}"),"branch":"shared","branch_oid":"a".repeat(40),"upstream_branch":"main"
        }],"restore":null})).unwrap()).unwrap();
        let vars: HashMap<String, String> = [
            ("G5D_WORKSPACE_ID", Uuid::new_v4().to_string()),
            ("G5D_VM_ID", Uuid::new_v4().to_string()),
            ("G5D_OWNER_USER_ID", Uuid::new_v4().to_string()),
            ("G5D_EPOCH", "1".into()),
            ("G5D_WORKSPACE_TOKEN", "secret.workspace.jwt".into()),
            ("G5D_DISPATCH_TOKEN", "ab".repeat(32)),
            ("G5D_API_URL", "https://api.test".into()),
            ("G5D_DISPATCH_PORT", "9100".into()),
            ("G5D_OWNER_GIT_NAME", "Workspace Owner".into()),
            ("G5D_OWNER_GIT_EMAIL", "owner@example.test".into()),
            (
                "G5D_DISPATCH_TEST_ROOT",
                temp.path().to_str().unwrap().into(),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        (temp, vars)
    }

    /// `g5.runtime.boot.backend.process-local`: a process-mode host names
    /// every path explicitly and the dispatcher listens on the loopback only;
    /// without the mode the same variables change nothing.
    #[test]
    fn process_mode_takes_its_paths_from_the_host_and_binds_loopback() {
        let (temp, mut vars) = fixture();
        let guest = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert!(!guest.process_mode);
        assert_eq!(
            guest.listen_addr(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
        );
        assert_eq!(
            guest.paths.catalog_path(),
            temp.path().join("opt/g5/modules/catalog.json")
        );

        let root = temp.path().join("process");
        std::fs::create_dir_all(root.join("checkout/.g5")).unwrap();
        std::fs::create_dir_all(root.join("checkout/code")).unwrap();
        std::fs::copy(
            temp.path().join("work/.g5/layout.json"),
            root.join("checkout/.g5/layout.json"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::create_dir_all(root.join("sock")).unwrap();
        vars.insert(PROCESS_MODE_ENV.into(), "process".into());
        vars.insert(
            PROCESS_SOCKET_DIR_ENV.into(),
            root.join("sock").to_string_lossy().into_owned(),
        );
        vars.insert(
            PROCESS_WORKSPACE_ROOT_ENV.into(),
            root.join("checkout").to_string_lossy().into_owned(),
        );
        vars.insert(
            PROCESS_HOME_ENV.into(),
            root.join("home").to_string_lossy().into_owned(),
        );
        vars.insert(
            PROCESS_STATE_DIR_ENV.into(),
            root.join("state").to_string_lossy().into_owned(),
        );
        vars.insert(
            PROCESS_RUNTIME_ROOT_ENV.into(),
            root.join("runtime").to_string_lossy().into_owned(),
        );
        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert!(config.process_mode);
        assert_eq!(
            config.listen_addr(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );
        assert_eq!(config.paths.workspace_root, root.join("checkout"));
        assert_eq!(config.paths.home, root.join("home"));
        assert_eq!(config.paths.state_dir, root.join("state"));
        assert_eq!(config.paths.runtime_bin_dir(), root.join("runtime/bin"));
        assert_eq!(
            config.paths.harness_dir(),
            root.join("runtime/assets/harness")
        );
        assert_eq!(
            config.paths.workspace_git_socket(),
            root.join("sock/workspace-git.sock"),
            "the socket binds where the host said, not under the long state path"
        );
        assert_eq!(
            guest.paths.workspace_git_socket(),
            temp.path().join("var/lib/g5d/workspace-git.sock")
        );

        // A relative path or a missing root fails closed.
        vars.insert(PROCESS_HOME_ENV.into(), "relative/home".into());
        assert!(DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err());
        vars.insert(
            PROCESS_HOME_ENV.into(),
            root.join("home").to_string_lossy().into_owned(),
        );
        vars.insert(
            PROCESS_RUNTIME_ROOT_ENV.into(),
            root.join("absent").to_string_lossy().into_owned(),
        );
        assert!(DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err());
        vars.remove(PROCESS_RUNTIME_ROOT_ENV);
        assert!(DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err());
        // Another mode value is not process mode.
        vars.insert(PROCESS_MODE_ENV.into(), "microvm".into());
        let guest = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert!(!guest.process_mode);
    }

    #[test]
    fn module_identity_refuses_root_and_garbage() {
        let (_temp, mut vars) = fixture();
        for (uid, gid, ok) in [
            ("65534", "65534", true),
            ("0", "65534", false),
            ("65534", "0", false),
            ("x", "65534", false),
        ] {
            vars.insert("G5D_MODULE_UID".into(), uid.into());
            vars.insert("G5D_MODULE_GID".into(), gid.into());
            let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned());
            assert_eq!(config.is_ok(), ok, "uid {uid} gid {gid}");
        }
    }

    #[test]
    fn config_uses_fixed_identity_and_redacts_credentials() {
        let (_temp, vars) = fixture();
        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert_eq!(config.owner_git.name, "Workspace Owner");
        assert_eq!(config.listener_token, [0xab; 32]);
        assert!(!format!("{config:?}").contains("secret.workspace.jwt"));
        assert_eq!(config.repo_roots().len(), 1);
        for key in [
            "G5D_WORKSPACE_TOKEN",
            "G5D_DISPATCH_TOKEN",
            "G5D_OWNER_GIT_NAME",
            "G5D_OWNER_GIT_EMAIL",
            "G5D_OWNER_USER_ID",
        ] {
            assert!(
                DispatchConfig::from_lookup(|candidate| if candidate == key {
                    None
                } else {
                    vars.get(candidate).cloned()
                })
                .is_err(),
                "missing {key}"
            );
        }
    }

    #[test]
    fn pty_reattach_grace_is_parsed_once_with_existing_bounds() {
        let (_temp, mut vars) = fixture();
        for (raw, expected) in [
            (None, Duration::from_secs(15)),
            (Some("bogus"), Duration::from_secs(15)),
            (Some("1"), Duration::from_millis(100)),
            (Some("250"), Duration::from_millis(250)),
            (Some("999999"), Duration::from_secs(60)),
        ] {
            if let Some(raw) = raw {
                vars.insert("G5D_PTY_REATTACH_GRACE_MS".into(), raw.into());
            } else {
                vars.remove("G5D_PTY_REATTACH_GRACE_MS");
            }
            let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
            assert_eq!(config.pty_reattach_grace, expected);
        }
    }

    #[test]
    fn the_personal_repository_is_the_followed_member_or_the_mount() {
        let member = |root: &str, follow: bool| {
            serde_json::json!({
                "repo_id": Uuid::new_v4(), "root_path": root, "role": "code",
                "git_url": "https://api.test/git/member", "branch": "work",
                "branch_oid": "a".repeat(40), "follow_branch": follow,
            })
        };
        let reference = |root: &str| {
            serde_json::json!({
                "repo_id": Uuid::new_v4(), "root_path": root,
                "git_url": "https://api.test/git/personal", "branch": "users/me",
                "commit": "b".repeat(40),
            })
        };
        let layout = |repos: Vec<serde_json::Value>, references: Vec<serde_json::Value>| {
            serde_json::from_value::<WorkspaceLayout>(
                serde_json::json!({"repos": repos, "references": references}),
            )
            .unwrap()
        };
        // An ordinary workspace: the mount at `personal`.
        let mounted = layout(vec![member("code", false)], vec![reference("personal")]);
        let personal = mounted.personal_repository().unwrap();
        assert_eq!(personal.repo_id, mounted.references[0].repo_id);
        assert_eq!(
            (personal.root_path.as_str(), personal.branch.as_str()),
            ("personal", "users/me")
        );
        // Start left the mount out.
        assert_eq!(
            layout(vec![member("code", false)], vec![]).personal_repository(),
            None
        );
        // A personal workspace: its own member, never a reference checkout.
        let own = layout(
            vec![member("personal", true)],
            vec![reference("base-code"), reference("docs")],
        );
        assert_eq!(
            own.personal_repository().unwrap().repo_id,
            own.repos[0].repo_id
        );
        // A member that merely sits at `personal` is not the owner's repository.
        assert_eq!(
            layout(vec![member("personal", false)], vec![]).personal_repository(),
            None
        );
    }

    #[test]
    fn personal_layout_admits_references_beside_members_only() {
        let (temp, vars) = fixture();
        let paths = RuntimePaths::under_test_root(temp.path());
        let layout_path = paths.workspace_root.join(".g5/layout.json");
        let mut layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&layout_path).unwrap()).unwrap();
        std::fs::create_dir(paths.workspace_root.join("base-code")).unwrap();
        let reference_id = Uuid::new_v4();
        let reference = |root: &str, url: String| {
            serde_json::json!([{
                "repo_id": reference_id, "root_path": root, "role": "code",
                "git_url": url, "branch": "main", "commit": "b".repeat(40),
            }])
        };
        layout["references"] =
            reference("base-code", format!("https://api.test/git/{reference_id}"));
        std::fs::write(&layout_path, serde_json::to_vec(&layout).unwrap()).unwrap();
        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert_eq!(config.layout.references.len(), 1);
        assert_eq!(config.layout.repos.len(), 1);
        assert_eq!(
            config.repo_roots().len(),
            1,
            "references are never captured as members"
        );

        #[cfg(unix)]
        std::os::unix::fs::symlink(
            paths.workspace_root.join("base-code"),
            paths.workspace_root.join("linked-code"),
        )
        .unwrap();
        layout["references"][0]["commit"] = serde_json::json!("b".repeat(12));
        layout["references"][0]["root_path"] = serde_json::json!("base-code");
        std::fs::write(&layout_path, serde_json::to_vec(&layout).unwrap()).unwrap();
        assert!(
            DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err(),
            "an abbreviated reference commit is refused"
        );

        for (root, url) in [
            #[cfg(unix)]
            (
                "linked-code",
                format!("https://api.test/git/{reference_id}"),
            ),
            (
                "missing-code",
                format!("https://api.test/git/{reference_id}"),
            ),
            ("code", format!("https://api.test/git/{reference_id}")),
            (
                "base-code",
                format!("https://elsewhere.test/git/{reference_id}"),
            ),
            (
                "base-code/../code",
                format!("https://api.test/git/{reference_id}"),
            ),
        ] {
            layout["references"] = reference(root, url.clone());
            std::fs::write(&layout_path, serde_json::to_vec(&layout).unwrap()).unwrap();
            assert!(
                DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err(),
                "{root} {url}"
            );
        }
    }

    #[test]
    fn config_accepts_host_layout_with_restore_receipt() {
        let (temp, vars) = fixture();
        let layout_path = RuntimePaths::under_test_root(temp.path())
            .workspace_root
            .join(".g5/layout.json");
        let fixture_layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&layout_path).unwrap()).unwrap();
        let repo_id: Uuid =
            serde_json::from_value(fixture_layout["repos"][0]["repo_id"].clone()).unwrap();
        let repo = g5_internal_api::host_ctl::VmRepoSpec {
            repo_id,
            root_path: "code".into(),
            role: Some("code".into()),
            git_url: format!("https://api.test/git/{repo_id}"),
            branch: "shared".into(),
            branch_oid: "a".repeat(40),
            upstream_branch: Some("main".into()),
            follow_branch: false,
        };
        let restore = g5_internal_api::host_ctl::VmRestoreSpec {
            flush_id: Uuid::new_v4(),
            repos: vec![g5_internal_api::host_ctl::VmRestoreRepoSpec {
                repo_id,
                branch_oid: "a".repeat(40),
                local_branch_oid: "b".repeat(40),
                local_head_oid: "c".repeat(40),
                head_ref: Some("refs/heads/shared".into()),
                sequencer: g5_internal_api::host_ctl::VmSequencer::None,
                snapshot_oid: "d".repeat(40),
                dirty: true,
            }],
        };
        std::fs::write(
            &layout_path,
            serde_json::to_vec(&serde_json::json!({
                "repos": [repo],
                "restore": restore,
            }))
            .unwrap(),
        )
        .unwrap();

        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert_eq!(config.layout.repos[0].branch_oid, "a".repeat(40));
        assert_eq!(config.layout.repos[0].role.as_deref(), Some("code"));
        assert_eq!(
            config.layout.restore.as_ref().unwrap().repos[0].repo_id,
            config.layout.repos[0].repo_id
        );
    }

    #[test]
    fn guest_git_transport_maps_loopback_without_changing_membership_or_route_guards() {
        let (temp, mut vars) = fixture();
        let path = temp.path().join("work/.g5/layout.json");
        let mut layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let repo = layout["repos"][0]["repo_id"].as_str().unwrap().to_owned();
        let admitted = format!("http://127.0.0.1:17500/git/{repo}");
        layout["repos"][0]["git_url"] = admitted.clone().into();
        // The personal mount: a reference checkout whose layout URL is the
        // same loopback host.
        let mount = Uuid::new_v4();
        let mount_admitted = format!("http://127.0.0.1:17500/git/{mount}");
        std::fs::create_dir(temp.path().join("work/personal")).unwrap();
        layout["references"] = serde_json::json!([{
            "repo_id": mount, "root_path": "personal", "git_url": mount_admitted,
            "branch": "users/me", "commit": "b".repeat(40),
        }]);
        let original = serde_json::to_vec(&layout).unwrap();
        std::fs::write(&path, &original).unwrap();
        vars.insert("G5D_API_URL".into(), "http://g5-host:17500".into());
        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        // The persisted URL stays the member's origin, the one its checkout
        // names and native Git routes on; only the relay's upstream takes the
        // host door (G5Labs/G5-Lean#1020).
        let door = format!("http://g5-host:17500/git/{repo}");
        assert_eq!(config.layout.repos[0].git_url, admitted);
        assert_eq!(
            config.layout.repos[0].door_url.as_deref(),
            Some(door.as_str())
        );
        assert_eq!(config.layout.repos[0].transport_url(), door);
        let mount_door = format!("http://g5-host:17500/git/{mount}");
        assert_eq!(config.layout.references[0].git_url, mount_admitted);
        assert_eq!(config.layout.references[0].transport_url(), mount_door);
        assert_eq!(
            config.layout.personal_repository().unwrap().transport_url,
            mount_door
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        for forbidden in [
            format!("http://127.0.0.1:17501/git/{repo}"),
            format!("https://127.0.0.1:17500/git/{repo}"),
            "http://127.0.0.1:17500/git/another-repo".into(),
            format!("http://untrusted.test:17500/git/{repo}"),
            format!("http://credential@127.0.0.1:17500/git/{repo}"),
        ] {
            layout["repos"][0]["git_url"] = forbidden.clone().into();
            std::fs::write(&path, serde_json::to_vec(&layout).unwrap()).unwrap();
            assert!(
                DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err(),
                "{forbidden}"
            );
        }
        // A native dispatcher fixture has no guest door. Its API and Git
        // endpoints must retain the same real loopback origin.
        std::fs::write(&path, &original).unwrap();
        vars.insert("G5D_API_URL".into(), "http://127.0.0.1:17500".into());
        let native = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert_eq!(native.layout.repos[0].git_url, admitted);
        assert_eq!(native.layout.repos[0].door_url, None);
        assert_eq!(native.layout.repos[0].transport_url(), admitted);
        assert_eq!(native.layout.references[0].transport_url(), mount_admitted);
        // Explicit local process mode keeps real host loopback, including Git.
        std::fs::write(&path, original).unwrap();
        vars.insert(PROCESS_MODE_ENV.into(), "process".into());
        vars.insert("G5D_API_URL".into(), "http://127.0.0.1:17500".into());
        for (key, suffix) in [
            (PROCESS_WORKSPACE_ROOT_ENV, "work"),
            (PROCESS_RUNTIME_ROOT_ENV, "runtime"),
            (PROCESS_SOCKET_DIR_ENV, "socket"),
            (PROCESS_HOME_ENV, "home"),
            (PROCESS_STATE_DIR_ENV, "state"),
        ] {
            let directory = temp.path().join(suffix);
            std::fs::create_dir_all(&directory).unwrap();
            vars.insert(key.into(), directory.to_string_lossy().into_owned());
        }
        let process = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        assert_eq!(process.layout.repos[0].git_url, admitted);
        assert_eq!(process.layout.repos[0].door_url, None);
        assert_eq!(process.layout.repos[0].transport_url(), admitted);
        assert_eq!(process.layout.references[0].door_url, None);
    }

    #[test]
    fn config_refuses_unsafe_layout_or_identity() {
        let (_temp, mut vars) = fixture();
        vars.insert("G5D_OWNER_GIT_NAME".into(), "owner\n[credential]".into());
        assert!(DispatchConfig::from_lookup(|key| vars.get(key).cloned()).is_err());
        vars.insert("G5D_OWNER_GIT_NAME".into(), "Owner".into());
        let config = DispatchConfig::from_lookup(|key| vars.get(key).cloned()).unwrap();
        for unsafe_path in [
            "/etc",
            "../code",
            "code/../code",
            ".g5",
            "code//other",
            "code/.",
        ] {
            let mut layout = config.layout.clone();
            layout.repos[0].root_path = unsafe_path.into();
            assert!(
                validate_layout(&layout, &config.paths.workspace_root, &config.api_url).is_err(),
                "{unsafe_path}"
            );
        }
        let mut layout = config.layout.clone();
        layout.repos[0].git_url = "https://evil.test/git/repo".into();
        assert!(validate_layout(&layout, &config.paths.workspace_root, &config.api_url).is_err());
    }
}
