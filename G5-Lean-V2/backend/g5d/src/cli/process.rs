//! Process-level CLI shell shared by the shipped `g5d` entrypoint.

use std::future::Future;
use std::path::{Path, PathBuf};

use g5_contracts::error::G5Error;

use crate::cli::{exit_codes, format_error, init_logging, Cli};

/// Tracing target of the process shell's exit and panic events. They log at
/// the logical `g5d` root (the mapping table renders this target as `g5d`),
/// outside `g5d::cli`, so a historical `g5d=info,g5d::cli=off` filter keeps
/// them enabled exactly as it did when the shell was its own crate.
pub(crate) const PROCESS_LOG_TARGET: &str = "g5d::process";

#[cfg(test)]
mod target_tests {
    #[test]
    fn the_shell_events_name_the_shared_target_literal() {
        // The macros take the literal (a const ident is ambiguous next to
        // dotted field names); keep both spellings in step.
        let source = include_str!("process.rs");
        assert_eq!(source.matches("target: \"g5d::process\"").count(), 2);
        assert_eq!(super::PROCESS_LOG_TARGET, "g5d::process");
    }
}

/// Run the shipped `g5d` process: build the shared Tokio runtime and dispatch
/// the parsed CLI invocation through [`run_process`].
pub fn run_process_entrypoint<D, Fut>(dispatch: D) -> i32
where
    D: FnOnce(Cli) -> Fut,
    Fut: Future<Output = Result<(), G5Error>>,
{
    #[cfg(target_os = "linux")]
    {
        let mut args = std::env::args_os().skip(1);
        if args.next().is_some_and(|arg| {
            arg == g5d_foundation::process_termination::execution_owner::ENTRYPOINT
        }) {
            return match g5d_foundation::process_termination::execution_owner::run(args) {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("execution owner: {error}");
                    1
                }
            };
        }
    }
    build_g5d_runtime()
        .expect("failed to construct g5d Tokio runtime")
        .block_on(run_process(dispatch))
}

/// Worker-thread stack size for the g5d runtime (spec `g5d/main` D24).
///
/// Tokio's 2 MiB default is not enough for the warm-resume sidecar read loop
/// when g5d is compiled with the `g5-dev` (debug-frame) profile: the polled
/// future stack overflows and aborts the microVM turn (issue #2177). Release
/// frames fit, but the reservation is virtual memory until touched, so sizing
/// for the worst profile costs release builds nothing.
pub const G5D_RUNTIME_THREAD_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Build the shared multi-thread runtime every ordinary `g5d` invocation runs
/// on (host, guest worker, and CLI roles alike), with the explicit D24 stack
/// size applied to worker and blocking threads.
pub fn build_g5d_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(G5D_RUNTIME_THREAD_STACK_BYTES)
        .enable_all()
        .build()
}

/// Parse and execute one `g5d` invocation through the injected command
/// dispatcher, returning the stable process exit code.
pub async fn run_process<D, Fut>(dispatch: D) -> i32
where
    D: FnOnce(Cli) -> Fut,
    Fut: Future<Output = Result<(), G5Error>>,
{
    install_panic_hook();

    run_parsed_cli(Cli::parse_with_source_tracking(), dispatch).await
}

/// The parsed command and dispatch callback enter the same process lifetime.
/// Kept separate from argv parsing so subprocess tests exercise this selection.
pub(crate) async fn run_parsed_cli<D, Fut>(cli: Cli, dispatch: D) -> i32
where
    D: FnOnce(Cli) -> Fut,
    Fut: Future<Output = Result<(), G5Error>>,
{
    init_logging(cli.global.verbose, cli.global.json);
    // gh-compatible PR output reserves stdout for selected data (or field help).
    let json_mode = cli.global.json && !matches!(cli.command, crate::cli::Command::Pr(_));

    let checks = matches!(&cli.command, crate::cli::Command::Pr(args) if matches!(args.command, crate::cli::PrCommand::Checks(_)));
    // `g5d pr` commands report their own success statuses (checks, comments),
    // as do `g5d issue create` and `g5d workspace email-owner` (an unclear
    // outcome).
    let pr = matches!(&cli.command, crate::cli::Command::Pr(_));
    // An agent turn may ask its execution owner to release leftover processes.
    let agent_turn = matches!(&cli.command, crate::cli::Command::AgentTurn);
    let issue = matches!(&cli.command, crate::cli::Command::Issue(_));
    let user = matches!(&cli.command, crate::cli::Command::User(_));
    let email_owner = matches!(
        &cli.command,
        crate::cli::Command::Workspace(args)
            if matches!(args.command, crate::cli::WorkspaceSubCommand::EmailOwner(_))
    );
    // The workspace agent's child and read commands (G5-Lean#710) call
    // g5-api and report their own statuses (an unclear outcome is 7).
    let workspace_agent = matches!(
        &cli.command,
        crate::cli::Command::Workspace(args)
            if !matches!(
                args.command,
                crate::cli::WorkspaceSubCommand::EmailOwner(_)
                    | crate::cli::WorkspaceSubCommand::Refresh(_)
                    | crate::cli::WorkspaceSubCommand::Save
            )
    );
    let native_cli = !email_owner
        && !workspace_agent
        && matches!(
            &cli.command,
            crate::cli::Command::Clone(_)
                | crate::cli::Command::Add(_)
                | crate::cli::Command::Workspace(_)
                | crate::cli::Command::Doctor(_)
                | crate::cli::Command::Spec(_)
        );
    let execution = async {
        if pr {
            let crate::cli::Command::Pr(args) = &cli.command else {
                unreachable!()
            };
            crate::cli::pr::execute_with_status(args, &cli.global).await
        } else if agent_turn {
            // Same stable-context guard as `dispatch` (which admits agent
            // turns), then the turn's own exit status.
            match crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global) {
                Ok(None) => g5d_agent::execute_with_status().await,
                Ok(Some(error)) | Err(error) => Err(error),
            }
        } else if issue {
            if let Some(error) =
                crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global)?
            {
                return Err(error);
            }
            let crate::cli::Command::Issue(args) = &cli.command else {
                unreachable!()
            };
            crate::cli::issue::execute(args, &cli.global).await
        } else if user {
            if let Some(error) =
                crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global)?
            {
                return Err(error);
            }
            let crate::cli::Command::User(args) = &cli.command else {
                unreachable!()
            };
            crate::cli::user::execute(args, &cli.global).await
        } else if email_owner {
            if let Some(error) =
                crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global)?
            {
                return Err(error);
            }
            let crate::cli::Command::Workspace(crate::cli::WorkspaceSubArgs {
                command: crate::cli::WorkspaceSubCommand::EmailOwner(args),
            }) = &cli.command
            else {
                unreachable!()
            };
            crate::cli::email_owner::execute(args, &cli.global).await
        } else if workspace_agent {
            if let Some(error) =
                crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global)?
            {
                return Err(error);
            }
            let crate::cli::Command::Workspace(args) = &cli.command else {
                unreachable!()
            };
            crate::cli::workspace_agent::execute(args, &cli.global).await
        } else {
            dispatch(cli).await.map(|()| exit_codes::SUCCESS)
        }
    };
    let outcome = if native_cli {
        crate::cli::native_git::with_interrupts(execution).await
    } else {
        execution.await
    };
    match outcome {
        Ok(status) => status,
        Err(error) => {
            let (code, mut exit) = error_to_code_and_exit(&error);
            if checks {
                exit = 1;
            }
            if json_mode {
                println!("{}", format_json_error(code, &error, exit));
            } else {
                tracing::error!(target: "g5d::process", error = %error, "g5d exited with error");
                eprintln!("{}", format_error(code, &error.to_string(), exit, false));
            }
            exit
        }
    }
}

fn format_json_error(code: &str, error: &G5Error, exit_code: i32) -> String {
    let mut value = serde_json::json!({
        "status": "error",
        "code": code,
        "message": error.to_string(),
        "exit_code": exit_code,
    });
    if let G5Error::ValidationError { details } = error {
        value["details"] = details.clone();
    }
    value.to_string()
}

/// Map a `G5Error` to the stable numeric CLI exit code.
#[must_use]
pub fn error_to_exit_code(error: &G5Error) -> i32 {
    match error {
        G5Error::ValidationError { details } if is_api_deployment_gap(details) => {
            exit_codes::NETWORK_AUTH_ERROR
        }
        G5Error::ValidationError { .. } => exit_codes::CONFIG_ERROR,
        G5Error::Unauthorized
        | G5Error::Forbidden { .. }
        | G5Error::ScopeMissing { .. }
        | G5Error::WorkspaceReadOnly { .. }
        | G5Error::AgentOtherWorkspacesDisabled
        | G5Error::RateLimited { .. } => exit_codes::NETWORK_AUTH_ERROR,
        G5Error::NotFound { .. } | G5Error::WorkerNotFound => exit_codes::NOT_FOUND,
        G5Error::GitError { .. } => exit_codes::REPO_GIT_ERROR,
        G5Error::Internal { message } => classify_internal_error(message).1,
        _ => exit_codes::GENERIC_ERROR,
    }
}

fn error_to_code_and_exit(error: &G5Error) -> (&'static str, i32) {
    match error {
        G5Error::Unauthorized | G5Error::Forbidden { .. } | G5Error::ScopeMissing { .. } => {
            ("NETWORK_AUTH_ERROR", exit_codes::NETWORK_AUTH_ERROR)
        }
        G5Error::WorkspaceReadOnly { .. } => {
            ("WORKSPACE_READ_ONLY", exit_codes::NETWORK_AUTH_ERROR)
        }
        G5Error::AgentOtherWorkspacesDisabled => (
            "AGENT_OTHER_WORKSPACES_DISABLED",
            exit_codes::NETWORK_AUTH_ERROR,
        ),
        G5Error::NotFound { .. } | G5Error::WorkerNotFound => ("NOT_FOUND", exit_codes::NOT_FOUND),
        G5Error::RateLimited { .. } => ("RATE_LIMITED", exit_codes::NETWORK_AUTH_ERROR),
        G5Error::GitError { .. } => ("REPO_GIT_ERROR", exit_codes::REPO_GIT_ERROR),
        G5Error::ValidationError { details } if is_api_deployment_gap(details) => {
            ("NETWORK_AUTH_ERROR", exit_codes::NETWORK_AUTH_ERROR)
        }
        G5Error::ValidationError { .. } => ("CONFIG_ERROR", exit_codes::CONFIG_ERROR),
        G5Error::AuthorityCutStale { .. } => ("AUTHORITY_CUT_STALE", exit_codes::GENERIC_ERROR),
        G5Error::Internal { message } => classify_internal_error(message),
        _ => ("GENERIC_ERROR", exit_codes::GENERIC_ERROR),
    }
}

fn classify_internal_error(message: &str) -> (&'static str, i32) {
    let message = message.to_lowercase();
    if is_isolation_error_message(&message) {
        ("ISOLATION_ERROR", exit_codes::ISOLATION_ERROR)
    } else if message.contains("no workspace")
        || message.contains("workspace already exists")
        || message.contains("already exists in workspace")
        || message.contains("workspace.yaml")
        || message.contains("no api url")
    {
        ("CONFIG_ERROR", exit_codes::CONFIG_ERROR)
    } else {
        ("GENERIC_ERROR", exit_codes::GENERIC_ERROR)
    }
}

fn is_isolation_error_message(message: &str) -> bool {
    message.contains("cloud hypervisor")
        || message.contains("cloud-hypervisor")
        || message.contains("microvm")
        || message.contains("micro-vm")
        || message.contains("isolation")
}

fn is_api_deployment_gap(details: &serde_json::Value) -> bool {
    details.get("kind").and_then(serde_json::Value::as_str) == Some("server_version_deployment_gap")
}

/// Resolve a command path over the global workspace flag, falling back to cwd.
pub fn resolve_entrypoint_workspace_path(
    global_workspace: Option<&Path>,
    command_path: Option<&Path>,
) -> Result<PathBuf, G5Error> {
    if let Some(path) = command_path {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = global_workspace {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir().map_err(|error| G5Error::Internal {
        message: format!("Failed to determine current directory: {error}"),
    })
}

/// Install the process panic hook before CLI parsing or runtime setup.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info.location().map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_owned())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());

        tracing::event!(
            target: "g5d::process",
            tracing::Level::ERROR,
            { panic.message = %message, panic.location = ?location },
            "g5d panicked — this is a bug, please report it"
        );
        default_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_error_exit_classification() {
        assert_eq!(
            error_to_exit_code(&G5Error::ValidationError {
                details: serde_json::json!({})
            }),
            exit_codes::CONFIG_ERROR
        );
        assert_eq!(
            error_to_exit_code(&G5Error::Unauthorized),
            exit_codes::NETWORK_AUTH_ERROR
        );
        assert_eq!(
            error_to_exit_code(&G5Error::GitError {
                message: "bad ref".to_owned()
            }),
            exit_codes::REPO_GIT_ERROR
        );
        assert_eq!(
            error_to_exit_code(&G5Error::Internal {
                message: "Cloud Hypervisor launch failed".to_owned()
            }),
            exit_codes::ISOLATION_ERROR
        );
    }

    #[test]
    fn authority_cut_stale_keeps_typed_cli_code() {
        let error = G5Error::AuthorityCutStale {
            details: Box::new(g5_contracts::error::AuthorityCutStaleDetails {
                workspace_id: "workspace".to_owned(),
                target_kind: "task".to_owned(),
                target_id: "task".to_owned(),
                current_cut: Vec::new(),
                mismatches: Vec::new(),
            }),
        };

        assert_eq!(error_to_exit_code(&error), exit_codes::GENERIC_ERROR);
        assert_eq!(
            error_to_code_and_exit(&error),
            ("AUTHORITY_CUT_STALE", exit_codes::GENERIC_ERROR)
        );
    }

    #[test]
    fn json_validation_errors_preserve_typed_details() {
        let error = G5Error::ValidationError {
            details: serde_json::json!({
                "diagnostics": [{
                    "code": "TASK_INVALID_INPUT",
                    "line": 1,
                    "column": 1,
                    "message": "invalid task input"
                }]
            }),
        };
        let rendered = format_json_error("CONFIG_ERROR", &error, exit_codes::CONFIG_ERROR);
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(
            value["details"]["diagnostics"][0]["code"],
            "TASK_INVALID_INPUT"
        );
    }

    #[test]
    fn native_git_failures_keep_target_and_effect_in_text_and_json() {
        use g5_scm::native::{FailureEffect, FailureKind, NativeGitError};
        for (effect, expected) in [
            (FailureEffect::Refused, "made no Git changes"),
            (FailureEffect::Partial, "confirmed an effect"),
            (FailureEffect::Unknown, "Git changes may have occurred"),
        ] {
            let detail = if effect == FailureEffect::Partial {
                "Git confirmed the requested remote ref; completion stream failed"
            } else {
                "remote did not report status"
            };
            let error: G5Error = NativeGitError {
                operation: "push",
                target: Some("https://api.example.invalid/git/member refs/heads/main".into()),
                kind: FailureKind::Exit,
                effect,
                detail: detail.into(),
            }
            .into();
            let (code, exit) = error_to_code_and_exit(&error);
            assert_eq!((code, exit), ("REPO_GIT_ERROR", exit_codes::REPO_GIT_ERROR));
            let json: serde_json::Value =
                serde_json::from_str(&format_json_error(code, &error, exit)).unwrap();
            let text = format_error(code, &error.to_string(), exit, false);
            for message in [text.as_str(), json["message"].as_str().unwrap()] {
                assert!(message.contains("https://api.example.invalid/git/member refs/heads/main"));
                assert!(message.contains(detail));
                assert!(message.contains(expected));
                if effect != FailureEffect::Refused {
                    assert!(message.contains("before retrying"));
                }
            }
        }
    }

    #[test]
    fn deployment_gap_uses_network_classification() {
        let error = G5Error::ValidationError {
            details: serde_json::json!({"kind": "server_version_deployment_gap"}),
        };
        assert_eq!(error_to_exit_code(&error), exit_codes::NETWORK_AUTH_ERROR);
        assert_eq!(
            error_to_code_and_exit(&error),
            ("NETWORK_AUTH_ERROR", exit_codes::NETWORK_AUTH_ERROR)
        );
    }

    #[test]
    fn internal_error_classification_is_shared_and_isolation_wins() {
        let mixed = G5Error::Internal {
            message: "workspace.yaml requested a Cloud Hypervisor isolation backend".to_string(),
        };
        assert_eq!(error_to_exit_code(&mixed), exit_codes::ISOLATION_ERROR);
        assert_eq!(
            error_to_code_and_exit(&mixed),
            ("ISOLATION_ERROR", exit_codes::ISOLATION_ERROR)
        );

        for message in ["workspace already exists", "already exists in workspace"] {
            let config = G5Error::Internal {
                message: message.to_string(),
            };
            assert_eq!(error_to_exit_code(&config), exit_codes::CONFIG_ERROR);
            assert_eq!(
                error_to_code_and_exit(&config),
                ("CONFIG_ERROR", exit_codes::CONFIG_ERROR)
            );
        }
    }

    /// Consume roughly `frames * 32KiB` of stack, defeating inlining and
    /// dead-store elimination, so the assertion below actually exercises the
    /// configured thread stack rather than the optimizer.
    #[inline(never)]
    fn burn_stack(frames: usize) -> u64 {
        let mut buf = [0u8; 32 * 1024];
        std::hint::black_box(&mut buf);
        buf[0] = frames as u8;
        if frames == 0 {
            u64::from(std::hint::black_box(buf[4096]))
        } else {
            burn_stack(frames - 1) + u64::from(buf[0])
        }
    }

    /// Regression proof for `g5d/main` D24 / issue #2177: the shared runtime's
    /// worker threads must survive a poll stack far deeper than Tokio's 2 MiB
    /// default (the warm-resume read loop overflows that default in
    /// `g5-dev`-profile guests). ~6.5 MiB of frames must complete without
    /// overflow on the 16 MiB configured stack. Reverting the stack size makes
    /// this abort (visible as a single failed test under nextest's
    /// process-per-test execution).
    #[test]
    fn runtime_worker_stack_survives_deep_poll_frames() {
        let runtime = build_g5d_runtime().expect("runtime builds");
        let total = runtime
            .block_on(async { tokio::spawn(async { burn_stack(200) }).await })
            .expect("deep-stack task completes");
        assert!(total >= 200);
        assert_eq!(
            G5D_RUNTIME_THREAD_STACK_BYTES,
            16 * 1024 * 1024,
            "stack size is a D24 contract value; change ontology first"
        );
    }

    #[test]
    fn entrypoint_workspace_path_preserves_precedence() {
        let global = PathBuf::from("/global");
        let command = PathBuf::from("/command");
        assert_eq!(
            resolve_entrypoint_workspace_path(Some(&global), Some(&command)).unwrap(),
            command
        );
        assert_eq!(
            resolve_entrypoint_workspace_path(Some(&global), None).unwrap(),
            global
        );
    }
}
