//! `g5d workspace email-owner` — the workspace agent emails the workspace's current
//! owner (`g5.workspace.owner-email`).
//!
//! The command takes no recipient: the server sends to the owner of the live
//! incarnation whose credentials the request carries. Every request carries an
//! idempotency key, so the client's own retries and an agent's rerun with the
//! printed key never send twice. The command fails outside a workspace VM.

use std::io::Read;
use std::path::PathBuf;

use g5_contracts::error::G5Error;
use serde_json::{json, Value};

use super::{stable_context, EmailBodyFormat, EmailOwnerArgs, GlobalFlags};

/// G5 did not confirm the request; a rerun with the same key is safe.
const OUTCOME_UNCLEAR: i32 = 7;

/// Queue the message and print its delivery; the exit status.
pub async fn execute(args: &EmailOwnerArgs, global: &GlobalFlags) -> Result<i32, G5Error> {
    let workspace_id = workspace_id_from(|key| std::env::var(key).ok()).ok_or_else(|| {
        G5Error::Config(
            "`g5d workspace email-owner` runs inside a G5 workspace VM: workspace identity and \
             credentials are not available here."
                .to_string(),
        )
    })?;
    let key = args
        .idempotency_key
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let request = request_body(args, &key)?;
    let client = stable_context::require_worker_client(global).map_err(|_| {
        G5Error::Config(
            "`g5d workspace email-owner` needs workspace credentials; run it from the agent's \
             workspace VM."
                .to_string(),
        )
    })?;
    match client.email_owner(&workspace_id, &request).await {
        Ok(response) => {
            if global.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&response["data"]).unwrap_or_default()
                );
            } else {
                println!("{}", render(&response["data"]));
            }
            Ok(0)
        }
        Err(error) => match outcome_of(&error) {
            Failure::Unclear => {
                eprintln!("G5 did not confirm the email: {error}");
                eprintln!("Rerun with --idempotency-key {key} to retry it; it is not sent twice.");
                Ok(OUTCOME_UNCLEAR)
            }
            Failure::RateLimited(retry_after_secs) => {
                match retry_after_secs {
                    Some(secs) => eprintln!(
                        "G5 refused the email to keep within the owner email limits; retry in {secs} seconds."
                    ),
                    None => eprintln!(
                        "G5 refused the email to keep within the owner email limits; retry later."
                    ),
                }
                Err(error)
            }
            Failure::Refused => Err(error),
        },
    }
}

enum Failure {
    /// No definite answer reached the client (network, timeout, 5xx after
    /// retries, an unreadable success body).
    Unclear,
    RateLimited(Option<u64>),
    /// A definite refusal: validation, authorization, email disabled.
    Refused,
}

fn outcome_of(error: &G5Error) -> Failure {
    match error {
        G5Error::RateLimited { retry_after_secs } => Failure::RateLimited(*retry_after_secs),
        G5Error::Internal { .. }
        | G5Error::ServiceUnavailable { .. }
        | G5Error::DatabaseRetryable { .. } => Failure::Unclear,
        _ => Failure::Refused,
    }
}

fn request_body(args: &EmailOwnerArgs, key: &str) -> Result<Value, G5Error> {
    let body = match (&args.body, &args.body_file) {
        (Some(text), None) => text.clone(),
        (None, Some(path)) => read_text(path)?,
        _ => return validation_error("body", "pass --body or --body-file"),
    };
    if args.subject.trim().is_empty() {
        return validation_error("subject", "must not be empty");
    }
    if body.trim().is_empty() {
        return validation_error("body", "must not be empty");
    }
    let mut request = json!({
        "subject": args.subject,
        "body": body,
        "idempotencyKey": key,
    });
    // Plain text omits the field, so it also works with an API that
    // predates formats.
    if args.format == EmailBodyFormat::Markdown {
        request["format"] = json!("markdown");
    }
    Ok(request)
}

fn render(data: &Value) -> String {
    let id = data["deliveryId"].as_str().unwrap_or("unknown");
    match data["status"].as_str() {
        Some("existing") => format!(
            "An email with this idempotency key was already requested (delivery {id}); nothing new was sent."
        ),
        _ => format!("Queued email to the workspace owner (delivery {id})."),
    }
}

fn read_text(path: &PathBuf) -> Result<String, G5Error> {
    if path == &PathBuf::from("-") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| G5Error::Config(format!("failed to read stdin: {e}")))?;
        return Ok(text);
    }
    std::fs::read_to_string(path)
        .map_err(|e| G5Error::Config(format!("failed to read {}: {e}", path.display())))
}

fn validation_error<T>(field: &str, error: &str) -> Result<T, G5Error> {
    Err(G5Error::ValidationError {
        details: json!({ "field": field, "error": error }),
    })
}

pub(super) fn workspace_id_from(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get("G5D_WORKSPACE_ID")
        .map(|id| id.trim().to_string())
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Result<EmailOwnerArgs, clap::Error> {
        let cli = crate::cli::Cli::try_parse_from(
            std::iter::once("g5d")
                .chain(["workspace", "email-owner"])
                .chain(args.iter().copied()),
        )?;
        match cli.command {
            crate::cli::Command::Workspace(crate::cli::WorkspaceSubArgs {
                command: crate::cli::WorkspaceSubCommand::EmailOwner(args),
            }) => Ok(args),
            other => panic!("parsed {other:?}"),
        }
    }

    #[test]
    fn takes_gh_style_flags_and_needs_a_subject_and_one_body() {
        let args = parse(&[
            "-s",
            "Done",
            "-b",
            "All tests pass",
            "--idempotency-key",
            "k1",
        ])
        .unwrap();
        assert_eq!(args.subject, "Done");
        assert_eq!(args.body.as_deref(), Some("All tests pass"));
        assert_eq!(
            request_body(&args, "k1").unwrap(),
            json!({ "subject": "Done", "body": "All tests pass", "idempotencyKey": "k1" })
        );
        let file = parse(&["--subject", "Done", "-F", "notes.txt"]).unwrap();
        assert_eq!(file.body_file, Some(PathBuf::from("notes.txt")));

        assert_eq!(args.format, EmailBodyFormat::Text);

        let markdown = parse(&["-s", "Digest", "-b", "# Hi", "--format", "markdown"]).unwrap();
        assert_eq!(
            request_body(&markdown, "k2").unwrap(),
            json!({
                "subject": "Digest",
                "body": "# Hi",
                "format": "markdown",
                "idempotencyKey": "k2"
            })
        );
        assert!(parse(&["-s", "x", "-b", "y", "--format", "html"]).is_err());

        assert!(parse(&["-b", "no subject"]).is_err());
        assert!(parse(&["-s", "no body"]).is_err());
        assert!(parse(&["-s", "x", "-b", "y", "-F", "z"]).is_err());
        assert!(parse(&["-s", "x", "-b", "y", "--to", "someone@example.com"]).is_err());
    }

    #[test]
    fn blank_text_is_refused_before_any_request() {
        let args = parse(&["-s", "  ", "-b", "body"]).unwrap();
        assert!(matches!(
            request_body(&args, "k"),
            Err(G5Error::ValidationError { .. })
        ));
        let args = parse(&["-s", "subject", "-b", "\n "]).unwrap();
        assert!(request_body(&args, "k").is_err());
    }

    #[test]
    fn only_answers_without_a_definite_outcome_ask_for_a_rerun() {
        let unclear = G5Error::Internal {
            message: "Network error: connection reset".to_string(),
        };
        assert!(matches!(outcome_of(&unclear), Failure::Unclear));
        assert!(matches!(
            outcome_of(&G5Error::RateLimited {
                retry_after_secs: Some(9)
            }),
            Failure::RateLimited(Some(9))
        ));
        for refused in [
            G5Error::Conflict {
                reason: "Email is not enabled in this deployment".to_string(),
            },
            G5Error::Forbidden {
                missing_scopes: Vec::new(),
            },
            G5Error::ValidationError { details: json!({}) },
        ] {
            assert!(
                matches!(outcome_of(&refused), Failure::Refused),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn output_names_the_delivery_and_never_an_address() {
        assert_eq!(
            render(&json!({ "deliveryId": "d1", "status": "queued" })),
            "Queued email to the workspace owner (delivery d1)."
        );
        assert!(render(&json!({ "deliveryId": "d1", "status": "existing" }))
            .contains("already requested"));
    }

    #[test]
    fn a_workspace_vm_names_its_workspace_in_the_environment() {
        let id = "0192f1a0-0000-7000-8000-000000000001";
        assert_eq!(
            workspace_id_from(|key| (key == "G5D_WORKSPACE_ID").then(|| format!(" {id} "))),
            Some(id.to_string())
        );
        assert_eq!(workspace_id_from(|_| Some("not-a-uuid".to_string())), None);
    }
}
