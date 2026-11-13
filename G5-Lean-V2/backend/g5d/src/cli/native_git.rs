//! CLI composition for the shared native runner. Signal/exit policy stays here;
//! process construction, credentials and cleanup stay in g5-scm.

use g5_contracts::{workspace_git::SOCKET_ENV, G5Error};
use g5_scm::native::{NativeCancellation, NativeGit};
use std::{future::Future, sync::OnceLock, time::Duration};

fn cancellation() -> &'static NativeCancellation {
    static LIFETIME: OnceLock<NativeCancellation> = OnceLock::new();
    LIFETIME.get_or_init(NativeCancellation::default)
}

pub(crate) fn runner() -> Result<NativeGit, G5Error> {
    // Interactive commands are cancellable (Ctrl-C); the bound only stops a
    // wedged transfer, not a large clone over a slow link.
    let timeout = Duration::from_secs(3600);
    let runner = if std::env::var_os(SOCKET_ENV).is_some() {
        let config = std::env::var_os("GIT_CONFIG_GLOBAL").ok_or_else(|| {
            G5Error::Config("Workspace Git requires its runtime-owned configuration".into())
        })?;
        NativeGit::workspace_cli(timeout, config.into())?
    } else {
        NativeGit::cli(timeout)?
    };
    Ok(runner.with_cancellation(cancellation().clone()))
}

/// Credentials authenticate only HTTP(S) transports; local and SSH remotes
/// use their own transport identity (an SSH key still applies to SSH).
pub(crate) fn transport_auth(url: &str, auth: &g5_scm::GitAuth) -> g5_scm::GitAuth {
    let http = url.starts_with("http://") || url.starts_with("https://");
    match auth {
        _ if http => auth.clone(),
        g5_scm::GitAuth::SshKey { .. } => auth.clone(),
        _ => g5_scm::GitAuth::None,
    }
}

/// Clone a checkout through the native runner for every remote scheme. A
/// manifest checkout tracks only its branch; a person's own clone (`full`)
/// keeps every remote branch.
pub(crate) async fn clone_checkout(
    url: &str,
    destination: &std::path::Path,
    branch: &str,
    full: bool,
    auth: &g5_scm::GitAuth,
) -> Result<g5_scm::GitRepo, G5Error> {
    let git = runner()?;
    let (branch, auth) = (
        (!branch.is_empty()).then_some(branch),
        transport_auth(url, auth),
    );
    if full {
        git.clone_full_checkout(url, destination, branch, &auth)
            .await?;
    } else {
        git.clone_checkout(url, destination, branch, &auth).await?;
    }
    g5_scm::GitRepo::open(destination)
}

/// Fetch `origin` after confirming it names the caller's expected URL. The
/// outer error is CLI setup; the inner one is the typed Git outcome.
pub(crate) fn fetch_origin_native(
    repository: &std::path::Path,
    expected_url: &str,
    auth: &g5_scm::GitAuth,
) -> Result<Result<(), g5_scm::native::NativeGitError>, G5Error> {
    Ok(runner()?.fetch_checkout_blocking(
        repository,
        "origin",
        expected_url,
        &transport_auth(expected_url, auth),
    ))
}

/// [`fetch_origin_native`] for callers that need no outcome distinction.
pub(crate) fn fetch_origin_blocking(
    repository: &std::path::Path,
    expected_url: &str,
    auth: &g5_scm::GitAuth,
) -> Result<(), G5Error> {
    Ok(fetch_origin_native(repository, expected_url, auth)??)
}

/// Used only for CLI commands containing these native operations. Host and
/// dispatcher commands retain their own shutdown/drain owners. The listener
/// runs on a runtime worker so synchronous push can be cancelled while joining
/// its private native runtime. Exit follows proof of child cleanup.
pub(crate) async fn with_interrupts(
    work: impl Future<Output = Result<i32, G5Error>>,
) -> Result<i32, G5Error> {
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|error| G5Error::Internal {
        message: format!("Cannot handle CLI interruption: {error}"),
    })?;
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| G5Error::Internal {
        message: format!("Cannot handle CLI termination: {error}"),
    })?;
    let task = tokio::spawn(async move {
        #[cfg(unix)]
        let exit = tokio::select! { _ = interrupt.recv() => 130, _ = terminate.recv() => 143 };
        #[cfg(not(unix))]
        let exit = {
            let _ = tokio::signal::ctrl_c().await;
            130
        };
        cancellation().cancel();
        cancellation().wait_idle().await;
        if cancellation().has_run() {
            eprintln!("Workspace Git operation interrupted. Accepted changes may remain; inspect local and remote repository state before retrying.");
        } else {
            eprintln!("Interrupted.");
        }
        std::process::exit(exit);
    });
    struct Listener(tokio::task::JoinHandle<()>);
    impl Drop for Listener {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let mut listener = Listener(task);
    let result = work.await;
    // A blocking native call can return its cancellation error as soon as its
    // child is reaped. Keep the listener alive until it finishes the signal
    // exit; dropping it here would race and suppress the intended exit status.
    if cancellation().is_cancelled() {
        let _ = (&mut listener.0).await;
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use g5_scm::GitAuth;
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    use std::{process::Command, time::Instant};

    #[test]
    fn interrupt_cleanup_child() {
        let Some(root) = std::env::var_os("G5_NATIVE_SIGNAL_TEST") else {
            return;
        };
        use clap::Parser;
        let family = std::env::var("G5_NATIVE_SIGNAL_FAMILY").unwrap();
        let args = match family.as_str() {
            "doctor" => vec!["g5d", "doctor"],
            "spec" => vec!["g5d", "spec", "list"],
            _ => vec!["g5d", "add", "https://example.invalid/repo"],
        };
        let cli = crate::cli::Cli::try_parse_from(args).unwrap();
        let status = crate::cli::process::build_g5d_runtime().unwrap().block_on(
            crate::cli::process::run_parsed_cli(cli, |_| async move {
                if family == "doctor" || family == "spec" {
                    runner()?.version_blocking()?;
                } else {
                    runner()?.fetch_checkout_blocking(
                        &std::path::PathBuf::from(root),
                        "origin",
                        "https://example.invalid/repo",
                        &GitAuth::None,
                    )?;
                }
                panic!("signal must interrupt the native operation");
            }),
        );
        panic!("signal must exit the CLI, got {status}");
    }

    #[test]
    fn terminal_interrupt_and_termination_wait_for_blocking_native_cleanup() {
        for (family, signal, expected_exit) in ["add", "spec", "doctor"]
            .into_iter()
            .flat_map(|family| [(family, libc::SIGINT, 130), (family, libc::SIGTERM, 143)])
        {
            let root = tempfile::tempdir().unwrap();
            let executable = root.path().join("git");
            std::fs::write(
                &executable,
                r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
    check-ref-format) exit 0;;
    remote) echo https://example.invalid/repo; exit 0;;
    fetch|--version)
      (while [ ! -f "$G5_NATIVE_SIGNAL_TEST/release" ]; do sleep 0.01; done
       echo escaped > "$G5_NATIVE_SIGNAL_TEST/escaped") &
      echo $$ > "$G5_NATIVE_SIGNAL_TEST/pid.tmp"
      mv "$G5_NATIVE_SIGNAL_TEST/pid.tmp" "$G5_NATIVE_SIGNAL_TEST/pid"
      wait
      exit 0;;
  esac
done
exit 99
"#,
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "cli::native_git::tests::interrupt_cleanup_child",
                    "--nocapture",
                ])
                .env("G5_NATIVE_SIGNAL_TEST", root.path())
                .env("G5_NATIVE_SIGNAL_FAMILY", family)
                .env(
                    "PATH",
                    std::env::join_paths(
                        std::iter::once(root.path().to_owned())
                            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                    )
                    .unwrap(),
                )
                .env_remove(SOCKET_ENV)
                .process_group(0)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = command.spawn().unwrap();
            let start_deadline = Instant::now() + Duration::from_secs(5);
            while !root.path().join("pid").exists() {
                if child.try_wait().unwrap().is_some() {
                    let output = child.wait_with_output().unwrap();
                    panic!(
                        "{family} exited before native start: {}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                if Instant::now() >= start_deadline {
                    let _ = child.kill();
                    let output = child.wait_with_output().unwrap();
                    panic!(
                        "{family} native child never started: {}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let git_pid: i32 = std::fs::read_to_string(root.path().join("pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(unsafe { libc::kill(-(child.id() as i32), signal) }, 0);
            let deadline = Instant::now() + Duration::from_secs(5);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    unsafe {
                        libc::kill(-git_pid, libc::SIGKILL);
                    }
                    panic!("CLI did not finish cancellation");
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                status.code(),
                Some(expected_exit),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("Accepted changes may remain"));
            assert_ne!(
                unsafe { libc::kill(git_pid, 0) },
                0,
                "native child must be reaped before CLI exit"
            );
            std::fs::write(root.path().join("release"), "go").unwrap();
            std::thread::sleep(Duration::from_millis(500));
            assert!(!root.path().join("escaped").exists());
        }
    }
}
