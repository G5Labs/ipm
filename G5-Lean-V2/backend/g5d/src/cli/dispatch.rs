//! Parsed-command routing for the shipped `g5d` process.

use g5_contracts::error::G5Error;

use crate::cli::{Cli, Command};

/// Route one parsed command through retained workspace and runtime handlers.
pub async fn dispatch(cli: Cli) -> Result<(), G5Error> {
    // Field discovery and validation must work without a workspace or credentials.
    // PR reads perform their own stable-context discovery before any API request.
    if let Command::Pr(args) = &cli.command {
        return crate::cli::pr::execute(args, &cli.global).await;
    }
    if let Some(error) = crate::cli::stable_context::stable_guard_error(&cli.command, &cli.global)?
    {
        return Err(error);
    }

    match &cli.command {
        Command::Dispatch => {
            g5d_dispatch::run_from_env()
                .await
                .map_err(|error| G5Error::Internal {
                    message: error.to_string(),
                })
        }
        Command::Bootstrap => crate::cli::bootstrap_supervisor::execute().await,
        Command::AgentTurn => g5d_agent::execute().await,
        Command::ApprovalReviewTools { config } => g5d_agent::formal_review::serve(config).await,
        #[cfg(feature = "test-helpers")]
        Command::DispatchFixtureModule => crate::cli::dispatch_fixture_module::execute().await,
        Command::Init(args) => crate::cli::init::execute(args, &cli.global).await,
        Command::Clone(args) => crate::cli::clone::execute(args, &cli.global).await,
        Command::Add(args) => crate::cli::add::execute(args, &cli.global).await,
        Command::Host(args) => crate::cli::host::execute(args, &cli.global).await,
        Command::Doctor(args) => crate::cli::doctor::execute(args, &cli.global).await,
        Command::Pr(args) => crate::cli::pr::execute(args, &cli.global).await,
        Command::Issue(args) => match crate::cli::issue::execute(args, &cli.global).await? {
            0 => Ok(()),
            status => Err(G5Error::Internal {
                message: format!("g5d issue exited with status {status}"),
            }),
        },
        Command::Question(args) => crate::cli::question::execute(args, &cli.global).await,
        Command::Chat(args) => crate::cli::chat::execute(args, &cli.global).await,
        Command::Auth(args) => crate::cli::auth::execute(args, &cli.global).await,
        Command::Spec(args) => crate::cli::spec::execute(args, &cli.global).await,
        Command::Workspace(args) => crate::cli::workspace::execute(args, &cli.global).await,
        Command::User(args) => match crate::cli::user::execute(args, &cli.global).await? {
            0 => Ok(()),
            status => Err(G5Error::Internal {
                message: format!("g5d user exited with status {status}"),
            }),
        },
        Command::Oci(args) => crate::cli::oci::execute(args, &cli.global).await,
        #[cfg(feature = "cloud-hypervisor")]
        Command::VmExec(args) => g5d_isolation::isolation::cloud_hypervisor::vm_exec_main(args),
        #[cfg(feature = "cloud-hypervisor")]
        Command::VmInit => crate::cli::vm_init::execute().await,
        Command::PreviewServe(args) => crate::cli::preview_serve::execute(args, &cli.global).await,
    }
}
