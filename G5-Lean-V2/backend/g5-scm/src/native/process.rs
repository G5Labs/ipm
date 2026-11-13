use super::{FailureEffect, FailureKind, NativeCancellation, NativeGitError};
use crate::GitAuth;
use base64::Engine;
use std::{
    ffi::{OsStr, OsString},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::ExitStatus,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, DuplexStream, ReadBuf},
    process::Command,
    sync::{oneshot, watch},
    time::Instant,
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const API_PATH: &str = "/usr/bin:/usr/local/bin";

pub(super) fn resolve_api_binary(configured: &Path) -> Result<PathBuf, String> {
    let candidate = if configured.components().count() == 1 {
        executable_on_path(configured.as_os_str(), OsStr::new(API_PATH))
            .ok_or("Git executable was not found on the API subprocess path")?
    } else {
        configured.to_owned()
    };
    candidate
        .canonicalize()
        .map_err(|_| "Cannot resolve the configured API Git executable".into())
}

pub(super) fn resolve_binary() -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH")
        .ok_or("PATH is unavailable; install Git or fix the CLI runtime path")?;
    let name = OsStr::new(if cfg!(windows) { "git.exe" } else { "git" });
    let candidate = executable_on_path(name, &path)
        .ok_or("Git executable was not found on the CLI runtime path")?;
    // A relative PATH entry must remain stable if a command changes cwd.
    candidate
        .canonicalize()
        .map_err(|_| "Cannot resolve the Git executable".into())
}

pub(super) fn resolve_host_binary(path: &OsStr) -> Result<PathBuf, String> {
    executable_on_path(
        OsStr::new(if cfg!(windows) { "git.exe" } else { "git" }),
        path,
    )
    .ok_or("Git executable was not found on the selected host runtime path")?
    .canonicalize()
    .map_err(|_| "Cannot resolve the host Git executable".into())
}

fn executable_on_path(name: &OsStr, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directory| directory.join(name))
        .find(|candidate| {
            let Ok(metadata) = candidate.metadata() else {
                return false;
            };
            if !metadata.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                let Ok(path) = std::ffi::CString::new(candidate.as_os_str().as_bytes()) else {
                    return false;
                };
                // Mode bits alone ignore which identity class applies and ACLs.
                // SAFETY: a valid NUL-terminated path; this only checks access.
                if unsafe {
                    libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS)
                } != 0
                {
                    return false;
                }
            }
            true
        })
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn executable_lookup_skips_non_executable_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        for (directory, mode) in [(&first, 0o600), (&second, 0o700)] {
            let program = directory.join("git");
            std::fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let search = std::env::join_paths([&first, &second]).unwrap();
        assert_eq!(
            crate::native::locate_executable(&search).unwrap(),
            second.join("git").canonicalize().unwrap()
        );
        assert!(crate::native::locate_executable(first.as_os_str()).is_err());
        // A root process can execute any file with an execute bit; a regular
        // owner must not inherit group/other execute bits that do not apply.
        for mode in [0o610, 0o641] {
            std::fs::set_permissions(first.join("git"), std::fs::Permissions::from_mode(mode))
                .unwrap();
            let expected = if unsafe { libc::geteuid() } == 0 {
                &first
            } else {
                &second
            };
            let selected = crate::native::locate_executable(&search).unwrap();
            assert_eq!(selected, expected.join("git").canonicalize().unwrap());
            assert!(std::process::Command::new(selected)
                .status()
                .unwrap()
                .success());
        }
    }

    #[tokio::test]
    async fn host_bundle_file_limit_stops_a_real_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bounded");
        let mut command = super::Invocation::new(
            std::path::Path::new("/bin/sh"),
            None,
            "test bounded writer",
            std::time::Duration::from_secs(5),
        );
        command.args([
            "-c",
            "dd if=/dev/zero of=\"$1\" bs=4096 count=16 2>/dev/null",
            "writer",
        ]);
        command.arg(&path);
        command.limit_file_size(1024);
        let result = command.execute().await.unwrap();
        assert!(!result.status.is_some_and(|status| status.success()));
        assert!(std::fs::metadata(path).unwrap().len() <= 1024);
    }

    #[test]
    fn repository_trust_names_the_resolved_directory() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let invocation = super::Invocation::new(
            std::path::Path::new("/bin/true"),
            Some(&link),
            "test",
            std::time::Duration::from_secs(1),
        );
        let args: Vec<_> = invocation
            .command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let trusted = format!("safe.directory={}", real.canonicalize().unwrap().display());
        assert_eq!(
            args,
            ["-C", &link.to_string_lossy(), "-c", &trusted],
            "-C keeps the caller's path; trust names its real directory"
        );
    }
}

pub(super) struct Invocation {
    command: Command,
    operation: &'static str,
    timeout: Duration,
    read_only: bool,
    secrets: Vec<String>,
    config_count: usize,
    cancellation: NativeCancellation,
    credential_files: Vec<tempfile::TempPath>,
    input: Option<Vec<u8>>,
    stdout_limit: usize,
    stderr_limit: usize,
    stdout_file: Option<std::fs::File>,
    stdout_stream: Option<DuplexStream>,
    finish_descendants: bool,
}

impl Invocation {
    pub(super) fn set_process_scope(&mut self, scope: Option<std::sync::Arc<std::fs::File>>) {
        #[cfg(target_os = "linux")]
        if let Some(file) = scope {
            use std::os::fd::AsRawFd;
            unsafe {
                self.command.pre_exec(move || {
                    if libc::write(file.as_raw_fd(), b"0".as_ptr().cast(), 1) != 1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = scope;
    }

    /// Bound a host seed bundle at the writer, including Git's pack children.
    /// The supervisor still accounts for the aggregate across repositories.
    pub(super) fn limit_file_size(&mut self, bytes: u64) {
        #[cfg(unix)]
        unsafe {
            self.command.pre_exec(move || {
                let limit = libc::rlimit {
                    rlim_cur: bytes as libc::rlim_t,
                    rlim_max: bytes as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        #[cfg(not(unix))]
        let _ = bytes;
    }

    pub(super) fn new(
        binary: &Path,
        repository: Option<&Path>,
        operation: &'static str,
        timeout: Duration,
    ) -> Self {
        let mut command = Command::new(binary);
        command
            .kill_on_drop(true)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        // Context selectors belong to the runtime; repository and invocation
        // controls belong to this operation. In particular, -C does not beat
        // GIT_DIR, and GIT_CONFIG_PARAMETERS beats GIT_CONFIG_COUNT.
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy();
            if key.starts_with("GIT_TRACE")
                || key.starts_with("GIT_CONFIG_KEY_")
                || key.starts_with("GIT_CONFIG_VALUE_")
                || matches!(
                    key.as_ref(),
                    "GIT_CURL_VERBOSE"
                        | "GIT_CONFIG"
                        | "GIT_CONFIG_PARAMETERS"
                        | "GIT_CONFIG_COUNT"
                        | "GIT_DIR"
                        | "GIT_COMMON_DIR"
                        | "GIT_WORK_TREE"
                        | "GIT_IMPLICIT_WORK_TREE"
                        | "GIT_INDEX_FILE"
                        | "GIT_OBJECT_DIRECTORY"
                        | "GIT_ALTERNATE_OBJECT_DIRECTORIES"
                        | "GIT_GRAFT_FILE"
                        | "GIT_SHALLOW_FILE"
                        | "GIT_NO_REPLACE_OBJECTS"
                        | "GIT_REPLACE_REF_BASE"
                        | "GIT_PREFIX"
                        | "GIT_NAMESPACE"
                        | "G5D_TOKEN"
                        | "G5_API_KEY"
                        | "G5D_WORKSPACE_TOKEN"
                        | "G5D_DISPATCH_GIT_TOKEN"
                        | "G5_GIT_TOKEN_FILE"
                        | "G5_SSH_KEY_PASSPHRASE_FILE"
                )
            {
                command.env_remove(name);
            }
        }
        command.env("LC_ALL", "C");
        if let Some(repository) = repository {
            command.arg("-C").arg(repository);
            // Git matches ownership exceptions against the resolved directory
            // (its cwd after -C), so a symlinked storage path must be trusted
            // by its real path. A path that does not exist yet stays as given.
            let trusted = repository
                .canonicalize()
                .unwrap_or_else(|_| repository.to_path_buf());
            let mut safe = OsString::from("safe.directory=");
            safe.push(trusted.as_os_str());
            command.arg("-c").arg(safe);
        }
        Self {
            command,
            operation,
            timeout,
            read_only: false,
            secrets: Vec::new(),
            config_count: 0,
            cancellation: NativeCancellation::default(),
            credential_files: Vec::new(),
            input: None,
            stdout_limit: OUTPUT_LIMIT,
            stderr_limit: OUTPUT_LIMIT,
            stdout_file: None,
            stdout_stream: None,
            finish_descendants: false,
        }
    }

    pub(super) fn set_cancellation(&mut self, cancellation: NativeCancellation) {
        self.cancellation = cancellation;
    }
    pub(super) fn runtime_config(&mut self, path: &Path) {
        self.command
            .env("GIT_CONFIG_GLOBAL", path)
            .env("GIT_CONFIG_NOSYSTEM", "1");
    }
    pub(super) fn api_context(&mut self) {
        self.command
            .env_clear()
            .env("PATH", API_PATH)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
    }
    pub(super) fn host_context(&mut self, environment: &super::HostGitEnvironment) {
        self.command
            .env_clear()
            .env("PATH", &environment.search_path)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        if !environment.system_config {
            self.command.env("GIT_CONFIG_NOSYSTEM", "1");
        }
        for (name, value) in [
            ("HOME", environment.home.as_deref()),
            ("XDG_CONFIG_HOME", environment.xdg_config_home.as_deref()),
            ("GIT_CONFIG_GLOBAL", environment.global_config.as_deref()),
            (
                "GIT_CONFIG_SYSTEM",
                environment.system_config_path.as_deref(),
            ),
            ("GIT_SSH", environment.transport.ssh_program.as_deref()),
            ("SSH_AUTH_SOCK", environment.ssh_agent_socket.as_deref()),
            ("GIT_SSL_CAINFO", environment.ca_bundle.as_deref()),
            ("SSL_CERT_DIR", environment.ca_directory.as_deref()),
        ] {
            if let Some(value) = value {
                self.command.env(name, value);
            }
        }
        for (name, value) in [
            ("LD_LIBRARY_PATH", &environment.library_path),
            ("DYLD_LIBRARY_PATH", &environment.dyld_library_path),
        ] {
            if let Some(value) = value {
                self.command.env(name, value);
            }
        }
        for (name, value) in [
            ("http_proxy", &environment.transport.http_proxy),
            ("https_proxy", &environment.transport.https_proxy),
            ("all_proxy", &environment.transport.all_proxy),
            ("no_proxy", &environment.transport.no_proxy),
            ("GIT_SSH_COMMAND", &environment.transport.ssh_command),
        ] {
            if let Some(value) = value {
                self.command.env(name, value);
            }
        }
        // Host-controlled caches/checkouts never execute guest-installed hooks.
        self.config(
            "core.hooksPath",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    }

    pub(super) fn member_context(&mut self, environment: &super::MemberGitEnvironment) {
        self.finish_descendants = true;
        self.command
            .env_clear()
            .env("PATH", &environment.search_path)
            .env("HOME", &environment.home)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_CONFIG_GLOBAL", &environment.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", &environment.author.name)
            .env("GIT_COMMITTER_NAME", &environment.author.name)
            .env("GIT_AUTHOR_EMAIL", &environment.author.email)
            .env("GIT_COMMITTER_EMAIL", &environment.author.email)
            .env(
                g5_contracts::workspace_git::SOCKET_ENV,
                &environment.workspace_socket,
            );
        for (name, value) in [
            ("LD_LIBRARY_PATH", &environment.library_path),
            ("DYLD_LIBRARY_PATH", &environment.dyld_library_path),
        ] {
            if let Some(value) = value {
                self.command.env(name, value);
            }
        }
        #[cfg(unix)]
        {
            if let Some(uid) = environment.uid {
                self.command.uid(uid);
            }
            if let Some(gid) = environment.gid {
                self.command.gid(gid);
            }
        }
        self.config(
            "core.hooksPath",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    }

    pub(super) fn temporary_index(&mut self, index: &Path) {
        self.command.env("GIT_INDEX_FILE", index);
    }
    pub(super) fn stdout_file(&mut self, file: std::fs::File) {
        self.stdout_file = Some(file);
    }
    pub(super) fn capture_limit(&mut self, limit: usize) {
        self.stdout_limit = limit;
        // Protocol inventories may be large; error diagnostics stay bounded.
        self.stderr_limit = limit.min(OUTPUT_LIMIT);
    }

    pub(super) fn server_repository(&mut self, repository: &Path, git_dir: &Path) {
        self.command.env("GIT_DIR", git_dir);
        if repository != git_dir {
            self.command.env("GIT_WORK_TREE", repository);
        }
        for name in ["GIT_SSL_CAINFO", "SSL_CERT_DIR"] {
            if let Some(value) = std::env::var_os(name) {
                self.command.env(name, value);
            }
        }
    }
    pub(super) fn protocol_output(&mut self) {
        // Protocol payload is data, not diagnostics. Advertisement/receive
        // responses retain the existing complete-response buffering behavior.
        self.stdout_limit = usize::MAX;
    }
    pub(super) fn relay(&mut self, url: &str) {
        self.secrets.push(url.to_owned());
        self.config("credential.helper", "");
        self.config(&format!("credential.{url}.helper"), "");
        self.config("http.extraHeader", "");
        self.config(&format!("http.{url}.extraHeader"), "");
        self.config("http.followRedirects", "false");
        self.config(&format!("http.{url}.followRedirects"), "false");
        self.command.env("GIT_ASKPASS", "false");
    }

    pub(super) fn member_rewrite(&mut self, canonical: &str, relay: &str) {
        self.secrets.push(relay.to_owned());
        self.config(&format!("url.{relay}.insteadOf"), canonical);
    }

    pub(super) fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(arg);
        self
    }
    pub(super) fn args<const N: usize>(&mut self, args: [&str; N]) -> &mut Self {
        self.command.args(args);
        self
    }
    pub(super) fn read_only(&mut self) -> &mut Self {
        self.read_only = true;
        self
    }

    pub(super) fn input(&mut self, input: Vec<u8>) -> &mut Self {
        self.input = Some(input);
        self
    }

    pub(super) fn authenticate(&mut self, url: &str, auth: &GitAuth) -> Result<(), NativeGitError> {
        let token = match auth {
            GitAuth::Token(token) => token,
            GitAuth::ScmToken { token, provider } => {
                if !provider.allows(url) {
                    return Err(super::invalid(
                        self.operation,
                        "SCM push destination is not authorized",
                    ));
                }
                token
            }
            GitAuth::SshKey {
                private_key,
                passphrase,
            } => {
                self.secrets.push(private_key.clone());
                let key = self.secret_file("ssh-key", private_key, false)?;
                let batch = if let Some(passphrase) = passphrase {
                    self.secrets.push(passphrase.clone());
                    let phrase = self.secret_file("ssh-passphrase", passphrase, false)?;
                    let askpass = self.secret_file(
                        "ssh-askpass",
                        "#!/bin/sh\ncat \"$G5_SSH_KEY_PASSPHRASE_FILE\"\n",
                        true,
                    )?;
                    self.command
                        .env("G5_SSH_KEY_PASSPHRASE_FILE", phrase)
                        .env("SSH_ASKPASS", askpass)
                        .env("SSH_ASKPASS_REQUIRE", "force")
                        .env("DISPLAY", "g5");
                    "no"
                } else {
                    "yes"
                };
                let key = key.to_str().ok_or_else(|| {
                    super::invalid(self.operation, "SSH credential directory must be UTF-8")
                })?;
                self.command.env(
                    "GIT_SSH_COMMAND",
                    format!(
                        "ssh -i '{}' -o IdentitiesOnly=yes -o BatchMode={batch}",
                        key.replace('\'', "'\\''")
                    ),
                );
                return Ok(());
            }
            GitAuth::None => return Ok(()),
        };
        if token.is_empty() || token.chars().any(char::is_control) {
            return Err(super::invalid(
                self.operation,
                "Platform Git token is empty or contains control characters",
            ));
        }
        // Higher-priority inherited parameter channels were cleared above.
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        self.secrets.extend([token.clone(), encoded.clone()]);
        self.config("credential.helper", "");
        self.config(&format!("credential.{url}.helper"), "");
        self.config("http.followRedirects", "false");
        self.config(&format!("http.{url}.followRedirects"), "false");
        self.config("http.extraHeader", "");
        self.config(&format!("http.{url}.extraHeader"), "");
        self.config(
            &format!("http.{url}.extraHeader"),
            &format!("Authorization: Basic {encoded}"),
        );
        self.command.env("GIT_ASKPASS", "false");
        Ok(())
    }

    fn secret_file(
        &mut self,
        kind: &str,
        contents: &str,
        executable: bool,
    ) -> Result<PathBuf, NativeGitError> {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .prefix(&format!("g5-{kind}-"))
            .tempfile()
            .map_err(|_| {
                super::invalid(self.operation, "Cannot create private Git credential file")
            })?;
        file.write_all(contents.as_bytes()).map_err(|_| {
            super::invalid(self.operation, "Cannot write private Git credential file")
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(std::fs::Permissions::from_mode(if executable {
                    0o700
                } else {
                    0o600
                }))
                .map_err(|_| {
                    super::invalid(self.operation, "Cannot secure private Git credential file")
                })?;
        }
        #[cfg(not(unix))]
        let _ = executable;
        // Linux cannot execute an askpass script while its writable descriptor
        // remains open. Keep unlink ownership without retaining that descriptor.
        let file = file.into_temp_path();
        let path = file.to_path_buf();
        self.credential_files.push(file);
        Ok(path)
    }

    pub(super) fn config(&mut self, key: &str, value: &str) {
        self.command
            .env(format!("GIT_CONFIG_KEY_{}", self.config_count), key)
            .env(format!("GIT_CONFIG_VALUE_{}", self.config_count), value);
        self.config_count += 1;
        self.command
            .env("GIT_CONFIG_COUNT", self.config_count.to_string());
    }

    pub(super) async fn execute(self) -> Result<Output, NativeGitError> {
        let mut output = self.execute_captured().await?;
        if let Some(error) = output.interruption.take() {
            return Err(error);
        }
        Ok(output)
    }

    /// Preserve collected protocol evidence even if completion is interrupted.
    /// Only a protocol-aware operation may interpret this partial output.
    pub(super) async fn execute_captured(self) -> Result<Output, NativeGitError> {
        let operation = self.operation;
        let effect = if self.read_only {
            FailureEffect::Refused
        } else {
            FailureEffect::Unknown
        };
        let (cancellation, cancelled) = oneshot::channel::<()>();
        // This task owns the process until cleanup, even when the awaiting
        // caller disappears. Dropping the sender wakes its cancellation arm.
        let active = self.cancellation.started();
        let supervisor = tokio::spawn(async move {
            let _active = active;
            self.supervise(cancelled, None).await
        });
        let result = supervisor.await.map_err(|_| NativeGitError {
            target: None,
            operation,
            kind: FailureKind::Io,
            effect,
            detail: "Git execution supervisor stopped unexpectedly".into(),
        });
        drop(cancellation);
        result?
    }

    pub(super) async fn execute_stream(
        mut self,
        target: &Path,
    ) -> Result<NativeGitStream, NativeGitError> {
        let operation = self.operation;
        let effect = if self.read_only {
            FailureEffect::Refused
        } else {
            FailureEffect::Unknown
        };
        let target = target.to_string_lossy().into_owned();
        let (reader, writer) = tokio::io::duplex(OUTPUT_LIMIT);
        self.stdout_stream = Some(writer);
        let (cancellation, cancelled) = oneshot::channel();
        let (started, start) = oneshot::channel();
        let (finished, completion) = oneshot::channel();
        let active = self.cancellation.started();
        let error_target = target.clone();
        tokio::spawn(async move {
            let _active = active;
            let result = self
                .supervise(cancelled, Some(started))
                .await
                .and_then(|output| {
                    if output.completed_successfully() {
                        Ok(())
                    } else {
                        Err(output.failure(operation, &GitAuth::None, FailureKind::Exit, effect))
                    }
                })
                .map_err(|error| error.at_target(&error_target, &GitAuth::None));
            let _ = finished.send(result);
        });
        start
            .await
            .map_err(|_| NativeGitError {
                operation,
                target: Some(redact(&target, &[])),
                kind: FailureKind::Io,
                effect,
                detail: "Git stream supervisor stopped during startup".into(),
            })?
            .map_err(|error| error.at_target(&target, &GitAuth::None))?;
        Ok(NativeGitStream {
            reader,
            completion,
            cancellation: Some(cancellation),
            stdout_done: false,
            ended: false,
            operation,
            effect,
        })
    }

    async fn supervise(
        mut self,
        mut cancelled: oneshot::Receiver<()>,
        mut started: Option<oneshot::Sender<Result<(), NativeGitError>>>,
    ) -> Result<Output, NativeGitError> {
        let effect = if self.read_only {
            FailureEffect::Refused
        } else {
            FailureEffect::Unknown
        };
        let failure = |kind, detail: &str| NativeGitError {
            target: None,
            operation: self.operation,
            kind,
            effect,
            detail: redact(detail, &self.secrets),
        };
        if self.timeout.is_zero() {
            let error = NativeGitError {
                effect: FailureEffect::Refused,
                ..failure(
                    FailureKind::Deadline,
                    "Git operation deadline elapsed before execution",
                )
            };
            if let Some(started) = started.take() {
                let _ = started.send(Err(error.clone()));
            }
            return Err(error);
        }
        if self.cancellation.is_cancelled()
            || cancelled
                .try_recv()
                .is_err_and(|error| matches!(error, oneshot::error::TryRecvError::Closed))
        {
            let error = NativeGitError {
                effect: FailureEffect::Refused,
                ..failure(
                    FailureKind::Cancelled,
                    "Caller cancelled before Git started",
                )
            };
            if let Some(started) = started.take() {
                let _ = started.send(Err(error.clone()));
            }
            return Err(error);
        }
        self.command
            .stdin(if self.input.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(match self.stdout_file.take() {
                Some(file) => std::process::Stdio::from(file),
                None => std::process::Stdio::piped(),
            })
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        self.command.process_group(0);
        let mut child = match self.command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let error = NativeGitError {
                    effect: FailureEffect::Refused,
                    ..failure(FailureKind::Spawn, &format!("Cannot start Git: {error}"))
                };
                if let Some(started) = started.take() {
                    let _ = started.send(Err(error.clone()));
                }
                return Err(error);
            }
        };
        let mut group = ProcessGroup(child.id());
        if let Some(started) = started.take() {
            let _ = started.send(Ok(()));
        }
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take().expect("piped stderr");
        let mut captured_stdout = Capture::default();
        let mut captured_stderr = Capture::default();
        let execution_started = Instant::now();
        let (backpressure, downstream) = watch::channel((Duration::ZERO, None));
        let completion = async {
            // Do not reap the leader until pipes close. Its PID remains reserved
            // while descendant-held pipes may need process-group cancellation.
            let (input_result, stdout_result, stderr_result, descendants_result) = tokio::join!(
                async {
                    if let (Some(mut stdin), Some(input)) = (stdin, self.input.take()) {
                        stdin.write_all(&input).await?;
                        stdin.shutdown().await?;
                    }
                    Ok::<_, std::io::Error>(())
                },
                async {
                    if let Some(mut stream) = self.stdout_stream.take() {
                        let mut stdout = stdout.expect("streamed stdout");
                        let mut buffer = [0; 16 * 1024];
                        loop {
                            // Waiting for Git to produce bytes uses its budget;
                            // waiting for the HTTP reader to accept them does not.
                            let count = stdout.read(&mut buffer).await?;
                            if count == 0 {
                                break;
                            }
                            let blocked_at = Instant::now();
                            backpressure.send_modify(|state| state.1 = Some(blocked_at));
                            let result = stream.write_all(&buffer[..count]).await;
                            backpressure.send_modify(|state| {
                                state.0 += blocked_at.elapsed();
                                state.1 = None;
                            });
                            result?;
                        }
                        stream.shutdown().await
                    } else if let Some(stdout) = stdout {
                        drain(stdout, &mut captured_stdout, self.stdout_limit).await
                    } else {
                        Ok(())
                    }
                },
                drain(stderr, &mut captured_stderr, self.stderr_limit),
                async {
                    if self.finish_descendants {
                        group.finish_descendants().await
                    } else {
                        Ok(())
                    }
                }
            );
            // A child can close stdin after committing refs. Keep draining both
            // outputs before reporting that write failure, so its confirmation
            // is not lost merely because the request pipe broke first.
            input_result?;
            stdout_result?;
            stderr_result?;
            descendants_result?;
            child.wait().await
        };
        let result = tokio::select! {
            _ = self.cancellation.cancelled() => Err(failure(FailureKind::Cancelled, "Caller cancelled Git execution")),
            _ = &mut cancelled => Err(failure(FailureKind::Cancelled, "Caller cancelled Git execution")),
            reason = execution_deadline(execution_started, self.timeout, downstream) =>
                Err(failure(FailureKind::Deadline, reason)),
            result = completion => match result {
                Ok(status) => Ok(status),
                Err(error) => Err(failure(FailureKind::Io, &format!("Cannot collect Git result: {error}"))),
            },
        };
        let (status, interruption) = match result {
            Ok(status) => (Some(status), None),
            Err(error) => {
                group.kill();
                let _ = child.start_kill();
                (child.wait().await.ok(), Some(error))
            }
        };
        group.0 = None;
        Ok(Output {
            status,
            interruption,
            stdout_is_diagnostic: self.stdout_limit != usize::MAX,
            stdout: captured_stdout.bytes,
            stderr: captured_stderr.bytes,
            truncated: captured_stdout.truncated || captured_stderr.truncated,
            secrets: self.secrets,
        })
    }
}

/// Track cumulative execution time, preserving elapsed backpressure even when
/// several writes complete before the deadline task observes a notification.
/// Time blocked delivering bytes downstream is excluded from the budget, but a
/// single delivery may not block for longer than the whole budget: a reader
/// that stays connected while accepting nothing would otherwise hold Git
/// indefinitely. A slow reader that keeps accepting bytes is unaffected.
async fn execution_deadline(
    started: Instant,
    budget: Duration,
    mut downstream: watch::Receiver<(Duration, Option<Instant>)>,
) -> &'static str {
    const ELAPSED: &str = "Git execution deadline elapsed";
    loop {
        let (blocked, paused_at) = *downstream.borrow_and_update();
        let elapsed = paused_at
            .unwrap_or_else(Instant::now)
            .duration_since(started)
            .saturating_sub(blocked);
        let remaining = budget.saturating_sub(elapsed);
        if remaining.is_zero() {
            return ELAPSED;
        }
        if let Some(paused_at) = paused_at {
            tokio::select! {
                changed = downstream.changed() => {
                    if changed.is_err() {
                        // The producer is gone; no downstream write can remain active.
                        tokio::time::sleep(remaining).await;
                        return ELAPSED;
                    }
                }
                _ = tokio::time::sleep_until(paused_at + budget) => {
                    return "Git execution deadline elapsed: the response reader accepted no data for the execution budget";
                }
            }
        } else {
            tokio::select! {
                changed = downstream.changed() => {
                    if changed.is_err() {
                        tokio::time::sleep(remaining).await;
                        return ELAPSED;
                    }
                }
                _ = tokio::time::sleep(remaining) => {}
            }
        }
    }
}

/// Binary protocol output with bounded buffering and owned process cleanup.
/// Dropping this reader cancels its Git process. Successful EOF includes the
/// process exit result; errors after bytes were emitted remain stream errors.
pub struct NativeGitStream {
    reader: DuplexStream,
    completion: oneshot::Receiver<Result<(), NativeGitError>>,
    cancellation: Option<oneshot::Sender<()>>,
    stdout_done: bool,
    ended: bool,
    operation: &'static str,
    effect: FailureEffect,
}
impl AsyncRead for NativeGitStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.ended || buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if !self.stdout_done {
            let before = buf.filled().len();
            match Pin::new(&mut self.reader).poll_read(cx, buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    self.ended = true;
                    self.cancellation.take();
                    return Poll::Ready(Err(error));
                }
                Poll::Ready(Ok(())) if buf.filled().len() != before => return Poll::Ready(Ok(())),
                Poll::Ready(Ok(())) => self.stdout_done = true,
            }
        }
        let result = match Pin::new(&mut self.completion).poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Ok(result)) => result,
            Poll::Ready(Err(_)) => Err(NativeGitError {
                operation: self.operation,
                target: None,
                kind: FailureKind::Io,
                effect: self.effect,
                detail: "Git stream supervisor stopped before completion".into(),
            }),
        };
        self.ended = true;
        self.cancellation.take();
        Poll::Ready(result.map_err(std::io::Error::other))
    }
}

struct ProcessGroup(Option<u32>);
impl ProcessGroup {
    /// Observe exit without reaping the leader, whose reserved PID protects
    /// the group identifier from reuse while we stop surviving descendants.
    async fn finish_descendants(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        if let Some(pid) = self.0 {
            loop {
                let exited = {
                    let mut status = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
                    let result = unsafe {
                        libc::waitid(
                            libc::P_PID,
                            pid,
                            status.as_mut_ptr(),
                            libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
                        )
                    };
                    if result < 0 {
                        let error = std::io::Error::last_os_error();
                        if error.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(error);
                    }
                    unsafe { status.assume_init().si_pid() != 0 }
                };
                if exited {
                    self.kill();
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        Ok(())
    }

    fn kill(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.0 {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.kill();
    }
}

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn drain(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    capture: &mut Capture,
    limit: usize,
) -> std::io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok(());
        }
        let retained = count.min(limit.saturating_sub(capture.bytes.len()));
        capture.bytes.extend_from_slice(&buffer[..retained]);
        capture.truncated |= count > retained;
    }
}

pub(super) struct Output {
    pub(super) status: Option<ExitStatus>,
    interruption: Option<NativeGitError>,
    stdout: Vec<u8>,
    stdout_is_diagnostic: bool,
    stderr: Vec<u8>,
    pub(super) truncated: bool,
    secrets: Vec<String>,
}
impl Output {
    pub(super) fn into_protocol_bytes(self) -> Vec<u8> {
        self.stdout
    }
    pub(super) fn protocol_bytes(&self) -> &[u8] {
        &self.stdout
    }
    pub(super) fn completed_successfully(&self) -> bool {
        self.interruption.is_none() && self.status.is_some_and(|status| status.success())
    }
    pub(super) fn redact(&self, text: &str) -> String {
        redact(text, &self.secrets)
    }
    /// The remote refused or required credentials. Git reports a rejected
    /// injected header as a failed prompt, since the header is not a Git
    /// credential; all of these mean the selected grant was not accepted.
    pub(super) fn credential_refused(&self) -> bool {
        let stderr = self.stderr_text().to_ascii_lowercase();
        [
            "could not read username",
            "could not read password",
            "terminal prompts disabled",
            "authentication failed",
            "invalid username or",
            "requested url returned error: 401",
            "requested url returned error: 403",
            "http 401",
            "http 403",
            "permission denied (publickey",
        ]
        .iter()
        .any(|signal| stderr.contains(signal))
    }

    pub(super) fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    pub(super) fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub(super) fn detail(&self, auth: &GitAuth) -> String {
        let stderr = String::from_utf8_lossy(&self.stderr);
        let stdout = String::from_utf8_lossy(&self.stdout);
        let mut detail = redact(
            if stderr.trim().is_empty() {
                if self.stdout_is_diagnostic {
                    stdout.trim()
                } else {
                    "Git protocol process did not complete successfully"
                }
            } else {
                stderr.trim()
            },
            &self.secrets,
        );
        if let GitAuth::Token(token) = auth {
            if !token.is_empty() {
                detail = detail.replace(token, "[REDACTED]");
            }
        }
        if self.truncated {
            detail.push_str(" [output truncated]");
        }
        if let Some(error) = &self.interruption {
            detail = format!("{}; {detail}", error.detail);
        }
        detail
    }
    pub(super) fn failure(
        &self,
        operation: &'static str,
        auth: &GitAuth,
        kind: FailureKind,
        effect: FailureEffect,
    ) -> NativeGitError {
        NativeGitError {
            target: None,
            operation,
            kind: self.interruption.as_ref().map_or(kind, |error| error.kind),
            effect,
            detail: self.detail(auth),
        }
    }
    pub(super) fn success(
        self,
        operation: &'static str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        if self.completed_successfully() {
            Ok(())
        } else {
            Err(self.failure(operation, auth, FailureKind::Exit, FailureEffect::Unknown))
        }
    }
}
pub(super) fn redact(text: &str, secrets: &[String]) -> String {
    let mut text = text.to_owned();
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret, "[REDACTED]");
            // Bounded capture can end partway through a credential. Scrub
            // meaningful prefixes without damaging coincidental short suffixes
            // (e.g. "branch" followed by a secret beginning with "http").
            for (end, _) in secret.char_indices().rev().filter(|(end, _)| *end >= 8) {
                if text.ends_with(&secret[..end]) {
                    text.truncate(text.len() - end);
                    text.push_str("[REDACTED]");
                    break;
                }
            }
        }
    }
    // Rejected URLs can contain arbitrary/encoded userinfo or query secrets,
    // including token shapes not recognized by the general credential scrubber.
    static URLS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let urls =
        URLS.get_or_init(|| regex::Regex::new(r#"[A-Za-z][A-Za-z0-9+.-]*://[^\s'"<>]+"#).unwrap());
    let text = urls.replace_all(&text, |captures: &regex::Captures<'_>| {
        let value = &captures[0];
        let authority = value
            .split_once("://")
            .unwrap()
            .1
            .split('/')
            .next()
            .unwrap();
        if authority.contains('@') || value.contains(['?', '#']) {
            "[REDACTED:repository_url]".to_owned()
        } else {
            value.to_owned()
        }
    });
    g5_contracts::credential_scrub::scrub(&text)
}
