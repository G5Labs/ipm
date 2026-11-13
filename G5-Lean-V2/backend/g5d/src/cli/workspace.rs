//! `g5d workspace` — in-VM workspace commands served by the dispatcher.
//!
//! `refresh` forwards to the dispatcher's `workspace.git` module over the local
//! socket; it never retargets the checkout. Publication is a native `git push`
//! of the workspace branch through the dispatcher's Git credential.
//! `email-owner` calls g5-api (see `email_owner`), as do the agent's child
//! and read commands (see `workspace_agent`); the process entry point runs
//! them directly for their own exit status.

use g5_contracts::error::G5Error;

use super::{GlobalFlags, WorkspaceSubArgs, WorkspaceSubCommand};

pub async fn execute(args: &WorkspaceSubArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    match &args.command {
        WorkspaceSubCommand::Save => {
            let id = super::email_owner::workspace_id_from(|key| std::env::var(key).ok())
                .ok_or_else(|| {
                    G5Error::Config("Run g5d workspace save inside the workspace VM".into())
                })?;
            let result = super::stable_context::require_worker_client(global)?
                .save_workspace_changes(&id)
                .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_default()
            );
            Ok(())
        }

        WorkspaceSubCommand::Refresh(args) => {
            crate::cli::workspace_git::execute(
                g5_contracts::workspace_git::LocalGitRequest::Refresh { repo_id: args.repo },
            )
            .await
        }
        WorkspaceSubCommand::EmailOwner(args) => {
            match crate::cli::email_owner::execute(args, global).await? {
                0 => Ok(()),
                status => Err(G5Error::Internal {
                    message: format!("g5d workspace email-owner exited with status {status}"),
                }),
            }
        }
        _ => match crate::cli::workspace_agent::execute(args, global).await? {
            0 => Ok(()),
            status => Err(G5Error::Internal {
                message: format!("g5d workspace exited with status {status}"),
            }),
        },
    }
}
