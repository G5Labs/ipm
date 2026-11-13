//! Owner identity and native Git configuration for this incarnation.
//!
//! The global configuration is safe to inherit in repository processes. The
//! relay receives an exact member credential in memory; Git hooks, filters,
//! and provider descendants do not receive the token.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use g5_scm::native::configuration::{
    configure_member_remotes, install_member_configuration, MemberConfiguration,
    MemberRemoteConfiguration,
};
#[cfg(any(test, target_os = "linux"))]
use uuid::Uuid;

use crate::config::{
    ConfigError, DispatchConfig, OwnerGitIdentity, RepositoryLayout, RuntimePaths, RUNTIME_EXCLUDES,
};

#[derive(Debug)]
pub struct GitRuntime {
    paths: RuntimePaths,
    owner: OwnerGitIdentity,
    global_config: PathBuf,
    repos: Vec<RepositoryLayout>,
    /// Reference checkouts get an origin remote too, so ordinary Git can
    /// fetch other workspaces' pushed history in them.
    references: Vec<crate::config::ReferenceLayout>,
    /// Set when this g5d is the mounted runtime bundle's own binary, or a
    /// process-mode host named its runtime directory, so shells can also
    /// reach the bundle's other tools. Local process fixtures leave it unset
    /// and keep an image-only command path.
    runtime_bin: Option<PathBuf>,
    /// Host process rather than guest: no bundle loader path is stamped.
    process_mode: bool,
    /// What a process-mode host asked every process of the incarnation to
    /// carry: the incarnation marker that lets a stop find work that left the
    /// session, and the developer's Docker connection selection. Empty in a
    /// guest.
    process_passthrough: Vec<(OsString, OsString)>,
    /// The customer image's declared environment, as the host named it in
    /// `G5D_IMAGE_ENV_KEYS` and this process inherited it at boot, already
    /// filtered by the reserved-key policy (ontology `g5.runtime.image-env`).
    /// Forwarded to every module, terminal and preview before the
    /// runtime-owned entries, which therefore win on a collision.
    image_env: Vec<(OsString, OsString)>,
}

/// Resolve the image keys the host named against this process environment,
/// keeping only keys the reserved-key policy lets a task export and that are
/// not native loader paths (those stay scoped to G5 processes). `lookup` is
/// the process environment; a named key that is absent is dropped. A name is
/// used exactly as the host wrote it: one that would change under trimming
/// is rejected rather than resolved to a different variable, so the
/// forwarded value is always the image's own.
fn image_environment(
    named_keys: Option<&str>,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Vec<(OsString, OsString)> {
    let Some(named_keys) = named_keys else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    named_keys
        .split(g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_SEPARATOR)
        .filter(|key| {
            *key == key.trim()
                && g5_contracts::types::task_context::is_exportable_task_env_key(key)
                && !g5d_foundation::runtime_environment::NATIVE_LOADER_PATH_KEYS.contains(key)
                && seen.insert(key.to_string())
        })
        .filter_map(|key| lookup(key).map(|value| (OsString::from(key), value)))
        .collect()
}

pub struct InternalGitCredential {
    pub username: &'static str,
    pub password: String,
}

impl std::fmt::Debug for InternalGitCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InternalGitCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// The process-mode keys the host set on this dispatcher for every process of
/// the incarnation (`g5.runtime.boot.backend.process-local`): the incarnation
/// marker, the outer incarnation's marker when this host is nested in one,
/// and the host Docker selection, an allowlist and nothing else.
fn process_passthrough(lookup: impl Fn(&str) -> Option<OsString>) -> Vec<(OsString, OsString)> {
    [
        g5d_foundation::runtime_environment::PROCESS_INCARNATION_ENV,
        g5d_foundation::runtime_environment::PROCESS_INCARNATION_OUTER_ENV,
    ]
    .into_iter()
    .chain(g5d_foundation::runtime_environment::HOST_DOCKER_ENV)
    .filter_map(|key| {
        lookup(key)
            .filter(|value| !value.is_empty())
            .map(|value| (OsString::from(key), value))
    })
    .collect()
}

/// The command path every owner terminal and module receives.
///
/// The `g5d` launcher directory is prepended, because a login shell must reach
/// the shim rather than the bundle binary that needs the G5 loader path. The
/// bundle's own directory is appended instead, so the tools it supplies for
/// shell use are reachable by name while an image-supplied `node`, `cargo` or
/// `just` of the same name keeps winning lookup. This is command lookup only:
/// no loader path follows it, and `runtime_bin` is unset away from a real
/// guest, leaving a local fixture's path image-only.
fn command_path(runtime_bin: Option<&Path>, launcher: &Path, inherited: &str) -> String {
    let path = g5d_foundation::runtime_environment::prepend_path(launcher, inherited);
    match runtime_bin {
        Some(bin) => {
            g5d_foundation::runtime_environment::append_path(&bin.to_string_lossy(), &path)
        }
        None => path,
    }
}

impl GitRuntime {
    pub fn install(config: &DispatchConfig) -> Result<Self, ConfigError> {
        ensure_directory(&config.paths.home)?;
        own_provider_home(config)?;
        ensure_directory(&config.paths.state_dir)?;
        let executable = std::env::current_exe().map_err(io_error)?;
        // One owner for "this process is the mounted bundle's g5d": the guest
        // standard-path symlink below keys off it. The appended command path
        // keys off it too, or off the runtime directory a process-mode host
        // named; a local fixture triggers neither.
        let guest_bundle = executable
            == Path::new(g5d_foundation::runtime_environment::GUEST_RUNTIME_BIN_DIR).join("g5d");
        let runtime_bin = if config.process_mode {
            Some(config.paths.runtime_bin_dir())
        } else {
            guest_bundle
                .then(|| PathBuf::from(g5d_foundation::runtime_environment::GUEST_RUNTIME_BIN_DIR))
        };
        let loader = if config.process_mode {
            // The launcher runs the host's own binary; the shim inherits
            // whatever loader setup the host process carries.
            Vec::new()
        } else {
            g5d_foundation::runtime_environment::native_loader_environment()
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let _bin = g5d_foundation::runtime_environment::install_g5d_launcher(
            &config.paths.state_dir.join("bin"),
            &executable,
            &loader,
        )
        .map_err(io_error)?;
        // Login shells can reset PATH. The installed guest runtime reserves
        // this one command in the image's standard bin; all other tools remain
        // image-owned. Local process fixtures never modify the host's bin.
        #[cfg(target_os = "linux")]
        if guest_bundle {
            use std::os::unix::fs::PermissionsExt;
            let system_bin = Path::new("/usr/local/bin");
            for directory in [Path::new("/usr"), Path::new("/usr/local"), system_bin] {
                match std::fs::create_dir(directory) {
                    Ok(()) => {
                        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o755))
                            .map_err(io_error)?
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AlreadyExists
                            && directory.is_dir() => {}
                    Err(error) => return Err(io_error(error)),
                }
            }
            let staged = system_bin.join(format!(".g5d-{}", Uuid::new_v4()));
            std::os::unix::fs::symlink(_bin.join("g5d"), &staged).map_err(io_error)?;
            std::fs::rename(staged, system_bin.join("g5d")).map_err(io_error)?;
        }
        let global_config = config.paths.state_dir.join("gitconfig");
        install_member_configuration(&MemberConfiguration {
            global_config: global_config.clone(),
            transport_config: config.paths.state_dir.join("native-gitconfig"),
            excludes_file: config.paths.state_dir.join("excludes"),
            safe_directories: config
                .layout
                .repos
                .iter()
                .map(|repo| config.paths.workspace_root.join(&repo.root_path))
                .collect(),
            author: g5_scm::GitAuthor {
                name: config.owner_git.name.clone(),
                email: config.owner_git.email.clone(),
            },
            excluded_roots: RUNTIME_EXCLUDES
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            // Atomic FileService staging is excluded at every depth.
            excluded_file_patterns: vec![".g5-dispatch-tmp-*".into()],
        })
        .map_err(|error| ConfigError(error.to_string()))?;

        let named_image_keys =
            std::env::var(g5d_foundation::runtime_environment::IMAGE_ENV_KEYS_ENV).ok();
        let image_env = image_environment(named_image_keys.as_deref(), |key| std::env::var_os(key));
        let process_passthrough = if config.process_mode {
            process_passthrough(|key| std::env::var_os(key))
        } else {
            Vec::new()
        };

        Ok(Self {
            paths: config.paths.clone(),
            owner: config.owner_git.clone(),
            global_config,
            repos: config.layout.repos.clone(),
            references: config.layout.references.clone(),
            runtime_bin,
            process_mode: config.process_mode,
            process_passthrough,
            image_env,
        })
    }

    /// Install ordinary Git's origin transport and upstream without putting a
    /// platform credential in the checkout or in a repository process. The
    /// caller owns the relay leases for the lifetime of this incarnation.
    /// The `insteadOf` key is each checkout's real origin, the persisted
    /// `git_url`; a guest's door-mapped view is only the lease's upstream
    /// (G5Labs/G5-Lean#1020).
    pub fn configure_native_remotes(
        &self,
        transport: &crate::git_transport::GitTransport,
    ) -> Result<Vec<crate::git_transport::TransportLease>, ConfigError> {
        let mut leases = Vec::new();
        let mut members = Vec::new();
        for repo in &self.repos {
            // A member push is a code push; it wakes memory sync.
            let lease = transport.observed_lease(&repo.repo_id.to_string(), repo.transport_url());
            members.push(MemberRemoteConfiguration {
                checkout: self.paths.workspace_root.join(&repo.root_path),
                config_path: self
                    .paths
                    .state_dir
                    .join(format!("native-git-{}", repo.repo_id)),
                canonical_url: repo.git_url.clone(),
                transport_url: lease.url().to_owned(),
                branch: repo.branch.clone(),
            });
            leases.push(lease);
        }
        for reference in &self.references {
            let lease = transport.lease(&reference.repo_id.to_string(), reference.transport_url());
            members.push(MemberRemoteConfiguration {
                checkout: self.paths.workspace_root.join(&reference.root_path),
                config_path: self
                    .paths
                    .state_dir
                    .join(format!("native-git-{}", reference.repo_id)),
                canonical_url: reference.git_url.clone(),
                transport_url: lease.url().to_owned(),
                branch: reference.branch.clone(),
            });
            leases.push(lease);
        }
        configure_member_remotes(&self.paths.state_dir.join("native-gitconfig"), &members)
            .map_err(|error| ConfigError(error.to_string()))?;
        Ok(leases)
    }

    /// Explicit environment for native owner processes. Callers add only
    /// declared build secrets and module inputs after clearing inherited env.
    pub fn process_environment(&self) -> Vec<(OsString, OsString)> {
        // Image-declared keys first, runtime-owned keys after them: every
        // consumer applies this vector in order, so the runtime wins on a
        // collision (ontology `g5.runtime.image-env`).
        let mut env = self.image_env.clone();
        env.extend([
            (
                "PATH".into(),
                command_path(
                    self.runtime_bin.as_deref(),
                    &self.paths.state_dir.join("bin"),
                    &std::env::var("PATH")
                        .unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
                )
                .into(),
            ),
            ("HOME".into(), self.paths.home.clone().into()),
            (
                "CLAUDE_CONFIG_DIR".into(),
                self.paths.home.join(".claude").into(),
            ),
            ("CODEX_HOME".into(), self.paths.home.join(".codex").into()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                self.global_config.clone().into(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_AUTHOR_NAME".into(), self.owner.name.clone().into()),
            ("GIT_AUTHOR_EMAIL".into(), self.owner.email.clone().into()),
            ("GIT_COMMITTER_NAME".into(), self.owner.name.clone().into()),
            (
                "GIT_COMMITTER_EMAIL".into(),
                self.owner.email.clone().into(),
            ),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (
                g5_contracts::workspace_git::SOCKET_ENV.into(),
                self.paths.workspace_git_socket().into(),
            ),
        ]);
        if !self.process_mode {
            env.extend(g5d_foundation::runtime_environment::native_loader_environment());
        }
        env.extend(self.process_passthrough.iter().cloned());
        env
    }

    /// Select member execution inputs; SCM alone assembles native Git's env.
    pub fn member_git(
        &self,
        deadline: Option<std::time::Instant>,
        output_limit: usize,
        cancellation: g5_scm::native::NativeCancellation,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<g5_scm::native::MemberGit, g5_scm::native::NativeGitError> {
        // A process-mode dispatcher runs the host's own Git against the host's
        // own loader setup; the guest runtime's library path is a guest fact.
        let loaders = if self.process_mode {
            Vec::new()
        } else {
            g5d_foundation::runtime_environment::native_loader_environment()
        };
        let loader = |name: &str| {
            loaders
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        g5_scm::native::MemberGit::new(
            g5_scm::native::MemberGitEnvironment {
                search_path: g5d_foundation::runtime_environment::prepend_path(
                    &self.paths.state_dir.join("bin"),
                    &std::env::var("PATH")
                        .unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
                )
                .into(),
                home: self.paths.home.clone(),
                global_config: self.global_config.clone(),
                workspace_socket: self.paths.workspace_git_socket(),
                author: g5_scm::GitAuthor {
                    name: self.owner.name.clone(),
                    email: self.owner.email.clone(),
                },
                uid,
                gid,
                library_path: loader("LD_LIBRARY_PATH"),
                dyld_library_path: loader("DYLD_LIBRARY_PATH"),
            },
            deadline,
            output_limit,
            cancellation,
        )
    }

    pub fn provider_command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear().envs(self.process_environment());
        command
    }

    /// Select the current incarnation credential for one admitted member or
    /// reference upstream, as the relay's leases name it. The relay injects
    /// it into HTTP; no repository process or file receives it. A reference's
    /// origin needs it too, so `git pull` and the personal mount's push work;
    /// the control plane decides what the incarnation may read or push there.
    pub fn credential_for_url(
        &self,
        current_token: &str,
        git_url: &str,
    ) -> Result<Option<InternalGitCredential>, ConfigError> {
        if current_token.is_empty()
            || current_token.len() > 64 * 1024
            || current_token.chars().any(char::is_control)
        {
            return Err(ConfigError("invalid internal Git token".into()));
        }
        if !self
            .repos
            .iter()
            .any(|repo| repo.transport_url() == git_url)
            && !self
                .references
                .iter()
                .any(|reference| reference.transport_url() == git_url)
        {
            return Ok(None);
        }
        Ok(Some(InternalGitCredential {
            username: "x-access-token",
            password: current_token.to_owned(),
        }))
    }
}

/// Hand the provider home (`/home/agent`: session files, `.claude`, the
/// proxy socket directory) to the host-provisioned module identity. The
/// rootfs is extracted without image ownership and this directory is created
/// by the dispatcher as root, so without this every non-builtin module fails
/// its first write with `EACCES` (QA, 2026-09-23: `agent.turn` exited 1 on
/// `create_dir_all($HOME/.g5/runtime)`). Owner-only, like the image's home.
fn own_provider_home(config: &DispatchConfig) -> Result<(), ConfigError> {
    let Some(uid) = config.module_uid else {
        return Ok(());
    };
    if !g5d_foundation::ownership::can_hand_over() {
        tracing::warn!(
            uid,
            "not root: the provider home stays under the dispatcher's identity"
        );
        return Ok(());
    }
    g5d_foundation::ownership::chown_tree(&config.paths.home, Some(uid), config.module_gid)
        .map_err(|error| {
            ConfigError(format!(
                "cannot hand the provider home to the module identity: {error}"
            ))
        })?;
    std::fs::set_permissions(
        &config.paths.home,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .map_err(|error| ConfigError(format!("cannot restrict the provider home: {error}")))
}

fn ensure_directory(path: &Path) -> Result<(), ConfigError> {
    std::fs::create_dir_all(path).map_err(io_error)?;
    let metadata = std::fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ConfigError(
            "runtime configuration directory must be physical".into(),
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> ConfigError {
    ConfigError(format!("cannot install runtime Git configuration: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RepositoryLayout, WorkspaceLayout};
    use crate::files::ObjectFormat;

    /// [`g5.runtime.bundle.tools`]: the bundle's shell tools become reachable
    /// by name without displacing the image, and the `g5d` shim still wins.
    #[test]
    fn the_command_path_appends_the_bundle_and_keeps_the_launcher_first() {
        let bundle = g5d_foundation::runtime_environment::GUEST_RUNTIME_BIN_DIR;
        let launcher = Path::new("/var/lib/g5d/bin");
        let image = "/opt/g5-git/bin:/usr/local/bin:/usr/bin:/bin";

        let guest = command_path(Some(Path::new(bundle)), launcher, image);
        assert_eq!(
            guest,
            format!("/var/lib/g5d/bin:{image}:{bundle}"),
            "the launcher leads and the bundle trails the image"
        );
        let order = |dir: &str| guest.split(':').position(|entry| entry == dir);
        assert!(
            order("/var/lib/g5d/bin") < order(bundle),
            "the g5d shim must win over the bundle's own binary: {guest}"
        );
        for image_dir in ["/opt/g5-git/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            assert!(
                order(image_dir) < order(bundle),
                "an image tool must win over the bundle's copy: {image_dir} in {guest}"
            );
        }

        // Re-entry through a module that inherited this path adds no duplicate.
        assert_eq!(
            command_path(Some(Path::new(bundle)), launcher, &guest),
            guest
        );

        // Away from a real guest the path stays image-only, so local and macOS
        // process fixtures keep the environment they are asserted against.
        assert_eq!(
            command_path(None, launcher, image),
            format!("/var/lib/g5d/bin:{image}")
        );
        assert!(!command_path(None, launcher, image).contains(bundle));
    }

    /// A process-mode dispatcher forwards exactly the incarnation marker and
    /// the host Docker selection it was started with to every module,
    /// terminal and preview; a guest forwards none of it.
    #[test]
    fn process_mode_forwards_the_incarnation_marker_and_docker_selection_only() {
        let host: std::collections::HashMap<&str, &str> = [
            (
                "G5D_PROCESS_INCARNATION",
                "6f4c0e9a-0000-4000-8000-000000000001",
            ),
            ("DOCKER_HOST", "unix:///tmp/review-host-docker.sock"),
            ("DOCKER_CONFIG", "/Users/dev/.docker"),
            ("DOCKER_TLS_VERIFY", ""),
            ("G5D_DISPATCH_TOKEN", "never"),
            ("AWS_SECRET_ACCESS_KEY", "never"),
        ]
        .into_iter()
        .collect();
        let forwarded = process_passthrough(|key| host.get(key).map(OsString::from));
        assert_eq!(
            forwarded,
            vec![
                (
                    OsString::from("G5D_PROCESS_INCARNATION"),
                    OsString::from("6f4c0e9a-0000-4000-8000-000000000001")
                ),
                (
                    OsString::from("DOCKER_HOST"),
                    OsString::from("unix:///tmp/review-host-docker.sock")
                ),
                (
                    OsString::from("DOCKER_CONFIG"),
                    OsString::from("/Users/dev/.docker")
                ),
            ]
        );
        let (_temp, _config, runtime) = fixture();
        assert!(
            runtime.process_passthrough.is_empty(),
            "a guest forwards nothing"
        );
        assert!(!runtime
            .process_environment()
            .iter()
            .any(|(key, _)| key == "G5D_PROCESS_INCARNATION"));
    }

    /// The installed runtime exposes the bundle only when this process really
    /// is the mounted bundle's g5d; a test binary never is. Asserted as "the
    /// runtime contributes no bundle entry" rather than "the result contains
    /// none", because a test runner launched through the bundle's own `cargo`
    /// wrapper already carries that directory in its ambient PATH.
    #[test]
    fn a_local_fixture_adds_no_bundle_entry_to_the_command_path() {
        let (_temp, _config, runtime) = fixture();
        assert_eq!(runtime.runtime_bin, None);
        let inherited =
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
        let path = runtime
            .process_environment()
            .into_iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.to_string_lossy().into_owned())
            .expect("PATH");
        assert_eq!(
            path,
            g5d_foundation::runtime_environment::prepend_path(
                &runtime.paths.state_dir.join("bin"),
                &inherited
            ),
            "a local fixture's path must be the launcher plus the inherited path, unchanged"
        );
    }

    /// [`g5.runtime.image-env`]: only the keys the host named, only those the
    /// reserved-key policy exports, never a loader path, never a missing key.
    #[test]
    fn image_environment_forwards_named_exportable_keys_only() {
        let process = std::collections::HashMap::from([
            ("RUSTUP_HOME", "/usr/local/rustup"),
            ("CARGO_HOME", "/usr/local/cargo"),
            ("PATH", "/image/bin"),
            ("HOME", "/home/ubuntu"),
            ("G5D_DISPATCH_TOKEN", "secret"),
            ("LD_LIBRARY_PATH", "/opt/g5/bin"),
            ("SPEC_ONLY", "not named"),
            // A padded name is a different variable from the unpadded one;
            // the dispatcher must not resolve the former to the latter.
            (" FOO ", "image padded"),
            ("FOO", "spec only"),
        ]);
        let lookup = |key: &str| process.get(key).map(|value| OsString::from(*value));
        let forwarded = image_environment(
            Some("RUSTUP_HOME:PATH:HOME:G5D_DISPATCH_TOKEN:LD_LIBRARY_PATH:MISSING:CARGO_HOME:RUSTUP_HOME: FOO "),
            lookup,
        );
        assert_eq!(
            forwarded,
            vec![
                ("RUSTUP_HOME".into(), "/usr/local/rustup".into()),
                ("CARGO_HOME".into(), "/usr/local/cargo".into()),
            ],
            "reserved, loader, missing, duplicate, unnamed and padded keys must not be forwarded"
        );
        assert!(image_environment(None, lookup).is_empty());
        assert!(image_environment(Some(""), lookup).is_empty());
    }

    /// The runtime-owned entries come after the image's, so a consumer that
    /// applies the vector in order keeps the runtime's PATH, HOME and
    /// provider-home keys even when the image names them.
    #[test]
    fn runtime_owned_keys_win_over_forwarded_image_keys() {
        let (_temp, _config, mut runtime) = fixture();
        runtime.image_env = vec![
            ("RUSTUP_HOME".into(), "/usr/local/rustup".into()),
            ("CLAUDE_CONFIG_DIR".into(), "/image/claude".into()),
            ("HOME".into(), "/home/ubuntu".into()),
        ];
        let env = runtime.process_environment();
        let position = |key: &str| env.iter().position(|(k, _)| k == key).unwrap();
        let last_value = |key: &str| {
            env.iter()
                .rev()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(
            last_value("RUSTUP_HOME"),
            OsString::from("/usr/local/rustup")
        );
        assert_eq!(
            last_value("HOME"),
            OsString::from(runtime.paths.home.clone()),
            "the runtime's home must be the effective one"
        );
        assert_eq!(
            last_value("CLAUDE_CONFIG_DIR"),
            OsString::from(runtime.paths.home.join(".claude")),
        );
        assert!(
            position("RUSTUP_HOME") < position("PATH"),
            "image keys precede runtime-owned keys: {env:?}"
        );
        let effective: std::collections::HashMap<_, _> = env.iter().cloned().collect();
        assert_eq!(
            effective[std::ffi::OsStr::new("HOME")],
            OsString::from(runtime.paths.home.clone())
        );
    }

    fn fixture() -> (tempfile::TempDir, DispatchConfig, GitRuntime) {
        let temp = tempfile::tempdir().unwrap();
        let paths = RuntimePaths::under_test_root(temp.path());
        std::fs::create_dir_all(paths.workspace_root.join("code")).unwrap();
        let repo_id = Uuid::new_v4();
        let config = DispatchConfig {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            owner_user_id: Uuid::new_v4(),
            epoch: 1,
            workspace_token: "secret-token".into(),
            listener_token: [1; 32],
            api_url: "https://api.example.test".parse().unwrap(),
            dispatch_port: 9000,
            lease_ttl_secs: 60,
            owner_git: OwnerGitIdentity {
                name: "Owner Name".into(),
                email: "owner@example.test".into(),
            },
            paths,
            layout: WorkspaceLayout {
                repos: vec![RepositoryLayout {
                    repo_id,
                    root_path: "code".into(),
                    role: None,
                    git_url: format!("https://api.example.test/git/{repo_id}"),
                    branch: "shared".into(),
                    branch_oid: "a".repeat(40),
                    upstream_branch: Some("main".into()),
                    object_format: ObjectFormat::Sha1,
                    follow_branch: false,
                    door_url: None,
                }],
                restore: None,
                references: Vec::new(),
            },
            module_uid: None,
            module_gid: None,
            git_door_port: 0,
            pty_reattach_grace: std::time::Duration::from_secs(15),
            process_mode: false,
        };
        let runtime = GitRuntime::install(&config).unwrap();
        (temp, config, runtime)
    }

    #[test]
    fn install_hands_the_provider_home_to_the_module_identity() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // The production spec: uid 1000. Root (the CI lanes) performs the
        // real transition; a developer-run dispatcher is not root and must
        // still install, leaving the home under its own identity.
        let (temp, mut config, _) = fixture();
        // SAFETY: geteuid has no failure modes and touches no memory.
        let (uid, gid) = if unsafe { libc::geteuid() } == 0 {
            (1000, 1000)
        } else {
            (
                unsafe { libc::geteuid() },
                std::fs::metadata(&config.paths.home).unwrap().gid(),
            )
        };
        // A stale root-owned entry from an earlier boot is re-owned too.
        std::fs::create_dir_all(config.paths.home.join(".claude")).unwrap();
        config.module_uid = Some(1000);
        config.module_gid = Some(1000);
        let mode_before = std::fs::metadata(&config.paths.home)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        GitRuntime::install(&config).unwrap();

        let home = std::fs::metadata(&config.paths.home).unwrap();
        assert_eq!((home.uid(), home.gid()), (uid, gid));
        // Only the root handoff restricts the home; a skipped handoff (a
        // developer whose own uid may even be 1000, under any umask) leaves
        // the mode exactly as it was.
        if g5d_foundation::ownership::can_hand_over() {
            assert_eq!(home.permissions().mode() & 0o777, 0o700);
        } else {
            assert_eq!(home.permissions().mode() & 0o777, mode_before);
        }
        let nested = std::fs::metadata(config.paths.home.join(".claude")).unwrap();
        assert_eq!((nested.uid(), nested.gid()), (uid, gid));
        drop(temp);
    }

    fn git(runtime: &GitRuntime, repo: &Path, args: &[&str]) -> String {
        let output = runtime
            .provider_command("git")
            .current_dir(repo)
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn native_git_add_uses_installed_excludes_and_commits_as_workspace_owner() {
        let (_temp, config, runtime) = fixture();
        let repo = config.paths.workspace_root.join("code");
        git(&runtime, &repo, &["init", "--quiet"]);
        std::fs::create_dir_all(repo.join(".g5/preview_profiles")).unwrap();
        std::fs::write(repo.join(".g5/workspace.yaml"), "name: product\n").unwrap();
        std::fs::write(
            repo.join(".g5/preview_profiles/dev.yaml"),
            "command: npm start\n",
        )
        .unwrap();
        git(&runtime, &repo, &["add", "-A"]);
        git(&runtime, &repo, &["commit", "--quiet", "-m", "seed"]);

        for path in RUNTIME_EXCLUDES {
            let target = repo.join(path);
            if path.ends_with("gitconfig")
                || path.ends_with("gitconfig.host")
                || path.ends_with("layout.json")
            {
                std::fs::write(target, "runtime\n").unwrap();
            } else {
                std::fs::create_dir_all(&target).unwrap();
                std::fs::write(target.join("private-state"), "runtime\n").unwrap();
            }
        }
        std::fs::write(repo.join(".g5/workspace.yaml"), "name: changed product\n").unwrap();
        std::fs::write(repo.join(".g5/new-product.yaml"), "new: product\n").unwrap();
        std::fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("src/.g5-dispatch-tmp-native-add"),
            "in-flight save",
        )
        .unwrap();
        git(
            &runtime,
            &repo,
            &["config", "user.name", "Historic launcher"],
        );
        git(
            &runtime,
            &repo,
            &["config", "user.email", "old@example.test"],
        );
        git(&runtime, &repo, &["add", "-A"]);
        let staged = git(&runtime, &repo, &["diff", "--cached", "--name-only"]);
        assert_eq!(
            staged,
            ".g5/new-product.yaml\n.g5/workspace.yaml\nmain.rs\n"
        );
        git(
            &runtime,
            &repo,
            &["commit", "--quiet", "-m", "owner changes"],
        );
        let author = git(&runtime, &repo, &["show", "-s", "--format=%an|%ae|%cn|%ce"]);
        assert!(
            author.starts_with("Owner Name|owner@example.test|Owner Name|owner@example.test"),
            "{author}"
        );
    }

    #[test]
    fn credential_is_exactly_scoped_rotates_in_memory_and_never_enters_provider_env() {
        let (_temp, config, runtime) = fixture();
        let provider = runtime.provider_command("env").output().unwrap();
        let env = String::from_utf8(provider.stdout).unwrap();
        assert!(!env.contains("TOKEN"));
        assert!(!env.contains("credential.helper"));
        assert!(!std::fs::read_to_string(&runtime.global_config)
            .unwrap()
            .contains("credential"));
        assert!(!config
            .paths
            .state_dir
            .join("git-credential-dispatch")
            .exists());

        for token in ["initial-token", "rotated-token"] {
            let credential = runtime
                .credential_for_url(token, &config.layout.repos[0].git_url)
                .unwrap()
                .unwrap();
            assert_eq!(credential.username, "x-access-token");
            assert_eq!(credential.password, token);
            assert!(!format!("{credential:?}").contains(token));
        }
        for url in [
            "https://evil.test/git/repo".into(),
            format!("{}/extra", config.layout.repos[0].git_url),
            "https://api.example.test/git/sibling".into(),
            config.layout.repos[0].git_url.replace("https:", "http:"),
        ] {
            assert!(
                runtime
                    .credential_for_url("must-not-leak", &url)
                    .unwrap()
                    .is_none(),
                "{url}"
            );
        }
        for token in [
            String::new(),
            "injected\nvalue".into(),
            "x".repeat(64 * 1024 + 1),
        ] {
            assert!(runtime
                .credential_for_url(&token, &config.layout.repos[0].git_url)
                .is_err());
        }
    }

    /// A microVM guest whose layout names the host loopback
    /// (G5Labs/G5-Lean#1020): the checkout's origin is the persisted URL, so
    /// the `insteadOf` key must be that URL for `git` to reach the relay at
    /// all, while the relay forwards to the host door.
    #[tokio::test]
    async fn native_remotes_key_on_the_persisted_origin_and_relay_through_the_door() {
        let (_temp, mut config, _) = fixture();
        config.api_url = "http://g5-host:17500".parse().unwrap();
        let repo_id = config.layout.repos[0].repo_id;
        let persisted = format!("http://localhost:17500/git/{repo_id}");
        let door = format!("http://g5-host:17500/git/{repo_id}");
        config.layout.repos[0].git_url = persisted.clone();
        config.layout.repos[0].door_url = Some(door.clone());
        let mount_id = Uuid::new_v4();
        let mount_persisted = format!("http://127.0.0.1:17500/git/{mount_id}");
        let mount_door = format!("http://g5-host:17500/git/{mount_id}");
        std::fs::create_dir_all(config.paths.workspace_root.join("personal")).unwrap();
        config
            .layout
            .references
            .push(crate::config::ReferenceLayout {
                repo_id: mount_id,
                root_path: "personal".into(),
                role: None,
                git_url: mount_persisted.clone(),
                branch: "users/me".into(),
                commit: "b".repeat(40),
                door_url: Some(mount_door.clone()),
            });
        let runtime = std::sync::Arc::new(GitRuntime::install(&config).unwrap());
        let transport = crate::git_transport::GitTransport::start(
            runtime.clone(),
            std::sync::Arc::new(|| "secret-token".to_owned()),
            0,
        )
        .await
        .unwrap();
        let leases = runtime.configure_native_remotes(&transport).unwrap();
        assert_eq!(leases.len(), 2);
        for (lease, root, persisted, door) in [
            (&leases[0], "code", &persisted, &door),
            (&leases[1], "personal", &mount_persisted, &mount_door),
        ] {
            assert_eq!(lease.upstream(), *door);
            let repo = config.paths.workspace_root.join(root);
            git(&runtime, &repo, &["init", "--quiet"]);
            git(&runtime, &repo, &["remote", "add", "origin", persisted]);
            // Ordinary git in the checkout dials the relay, not its own loopback.
            assert_eq!(
                git(&runtime, &repo, &["ls-remote", "--get-url", "origin"]).trim(),
                lease.url()
            );
            // The relay authenticates its upstream; the checkout's origin is
            // never an upstream in a guest.
            assert!(runtime
                .credential_for_url("secret-token", door)
                .unwrap()
                .is_some());
            assert!(runtime
                .credential_for_url("secret-token", persisted)
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn a_reference_origin_gets_the_credential_and_nothing_else_does() {
        let (_temp, mut config, _) = fixture();
        let reference_id = Uuid::new_v4();
        let reference_url = format!("https://api.example.test/git/{reference_id}");
        std::fs::create_dir_all(config.paths.workspace_root.join("personal")).unwrap();
        config
            .layout
            .references
            .push(crate::config::ReferenceLayout {
                repo_id: reference_id,
                root_path: "personal".into(),
                role: None,
                git_url: reference_url.clone(),
                branch: "personal".into(),
                commit: "b".repeat(40),
                door_url: None,
            });
        let runtime = GitRuntime::install(&config).unwrap();
        let credential = runtime
            .credential_for_url("current-token", &reference_url)
            .unwrap()
            .expect("a reference's origin is relayed with the VM credential");
        assert_eq!(credential.password, "current-token");
        for url in [
            format!("{reference_url}/extra"),
            reference_url.replace("https:", "http:"),
            format!("https://api.example.test/git/{}", Uuid::new_v4()),
        ] {
            assert!(
                runtime
                    .credential_for_url("must-not-leak", &url)
                    .unwrap()
                    .is_none(),
                "{url}"
            );
        }
    }

    #[test]
    fn repository_hooks_and_filters_never_receive_surrounding_platform_tokens() {
        const CHILD: &str = "G5D_TOKEN_INHERITANCE_TEST_CHILD";
        const SYNTHETIC: &str = "synthetic-inheritance-test-token";
        if std::env::var_os(CHILD).is_none() {
            // Keep the synthetic surrounding environment isolated from parallel tests.
            let output=Command::new(std::env::current_exe().unwrap())
                .args(["--exact","git::tests::repository_hooks_and_filters_never_receive_surrounding_platform_tokens","--nocapture"])
                .env(CHILD,"1").env("G5D_DISPATCH_GIT_TOKEN",SYNTHETIC).env("G5D_WORKSPACE_TOKEN",SYNTHETIC)
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            return;
        }
        assert_eq!(std::env::var("G5D_DISPATCH_GIT_TOKEN").unwrap(), SYNTHETIC);
        let (_temp, config, runtime) = fixture();
        let repo = config.paths.workspace_root.join("code");
        git(&runtime, &repo, &["init", "--quiet"]);
        let hook = repo.join(".git/hooks/pre-commit");
        std::fs::write(&hook,"#!/bin/sh\nprintf '%s|%s' \"${G5D_DISPATCH_GIT_TOKEN-unset}\" \"${G5D_WORKSPACE_TOKEN-unset}\" > hook-observation\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            repo.join(".gitattributes"),
            "filtered.txt filter=untrusted\n",
        )
        .unwrap();
        std::fs::write(repo.join("filtered.txt"), "content\n").unwrap();
        git(&runtime,&repo,&["config","filter.untrusted.clean","printf '%s|%s' \"${G5D_DISPATCH_GIT_TOKEN-unset}\" \"${G5D_WORKSPACE_TOKEN-unset}\" > filter-observation; cat"]);
        // Credential resolution itself invokes no repository Git command,
        // regardless of repository hooks, filters, or the surrounding token.
        let credential = runtime
            .credential_for_url("latest-rotated-token", &config.layout.repos[0].git_url)
            .unwrap()
            .unwrap();
        assert_eq!(credential.password, "latest-rotated-token");
        assert!(!repo.join("hook-observation").exists());
        assert!(!repo.join("filter-observation").exists());
        git(&runtime, &repo, &["add", "filtered.txt"]);
        git(
            &runtime,
            &repo,
            &["commit", "--quiet", "-m", "native owner commit"],
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("hook-observation")).unwrap(),
            "unset|unset"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("filter-observation")).unwrap(),
            "unset|unset"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reinstall_replaces_config_symlink_without_touching_target() {
        let (_temp, config, runtime) = fixture();
        let victim = config.paths.home.join("victim");
        std::fs::write(&victim, "untouched").unwrap();
        std::fs::remove_file(&runtime.global_config).unwrap();
        std::os::unix::fs::symlink(&victim, &runtime.global_config).unwrap();
        GitRuntime::install(&config).unwrap();
        assert_eq!(std::fs::read_to_string(victim).unwrap(), "untouched");
        assert!(!std::fs::symlink_metadata(runtime.global_config)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
