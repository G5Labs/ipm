//! Read PRs or invoke the UI's create/update flow without exposing provider credentials.
use crate::cli::{
    api_client::ApiClient, config::WorkspaceConfig, stable_context, GlobalFlags, PrArgs,
    PrChecksArgs, PrCommand, PrViewArgs,
};
use g5_contracts::G5Error;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

#[path = "pr_write.rs"]
mod write;

/// The selector and repository a branch resolves to: a GitHub number in its
/// repository, or a stored G5 PR's ID.
fn branch_selector(pull: write::BranchPullRequest) -> (String, Option<String>) {
    match pull {
        write::BranchPullRequest::GitHub(number, repository) => {
            (number, Some(repository).filter(|r| !r.is_empty()))
        }
        write::BranchPullRequest::G5(id) => (id.to_string(), None),
    }
}

/// Whether a PR argument names a G5 PR itself, by its ID or its `PR######` name,
/// rather than a GitHub PR or a branch (`g5.git.pr-cli.selectors`).
pub(crate) fn names_g5_pull_request(selector: &str) -> bool {
    write::names_g5_pull_request(selector)
}

/// A G5 PR's name (`PR3fa9c1`) as the server writes it; a server that
/// predates names sends the bare number.
pub(crate) fn g5_name(number: &Value) -> Option<String> {
    match number {
        Value::String(name) => Some(name.clone()),
        Value::Number(n) => n.as_u64().map(write::g5_pull_request_name),
        _ => None,
    }
}

/// G5 PRs have no GitHub conversation; matches the server's refusal.
const G5_CONVERSATION_REFUSAL: &str = "G5 PRs have no comments or review threads; discuss the \
    work in the workspace's comments and reviews. `g5d pr comment` and `g5d pr resolve` act on \
    GitHub PRs (a number or URL)";

/// `g5.attachments.cli`: a G5 PR has no attachments of its own.
const G5_ATTACHMENTS_REFUSAL: &str = "G5 PRs have no attachments of their own: their \
    description is G5's, so read its files in the workspace. `g5d pr view` names their GitHub \
    PRs, and `g5d pr attachments NUMBER` reads one";

/// Where a stored PR's output carries its description read.
pub(crate) const DESCRIPTION_READ: &str = "descriptionRead";

/// Attaches the canonical description to a single stored PR's output when it
/// is shown, so `g5d` never resolves the description itself.
async fn attach_description(
    api: &Api<'_>,
    output: &mut Value,
    selection: &crate::cli::pr_json::Selection,
) -> Result<(), G5Error> {
    let human = matches!(selection, crate::cli::pr_json::Selection::Human);
    if output["source"] != "g5"
        || !output["pullRequest"].is_object()
        || !selection.shows_description()
        || (human && api.global.quiet)
    {
        return Ok(());
    }
    let read = write::description(api, text(&output["pullRequest"], "id")).await?;
    output["pullRequest"][DESCRIPTION_READ] = read;
    Ok(())
}

/// Attaches the canonical descriptions to a stored-PR listing whose `body` is
/// selected, in one batch read for the PRs it shows.
async fn attach_descriptions(
    api: &Api<'_>,
    output: &mut Value,
    selection: &crate::cli::pr_json::Selection,
) -> Result<(), G5Error> {
    if !selection.selects_body() {
        return Ok(());
    }
    // A repository listing's rows already carry GitHub's body; its stored G5
    // rows take the canonical description (`g5.git.pr-reads`).
    let repository = output["mode"] == "repository";
    if !repository && output["source"] != "g5" {
        return Ok(());
    }
    let Some(pulls) = output["pullRequests"].as_array_mut() else {
        return Ok(());
    };
    let stored = |p: &Value| !repository || p["source"] == "g5";
    let ids: Vec<String> = pulls
        .iter()
        .filter(|p| stored(p))
        .map(|p| text(p, "id").to_owned())
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let mut read = write::descriptions(api, &ids).await?;
    for pull in pulls.iter_mut().filter(|p| stored(p)) {
        if let Some(entry) = read.remove(text(pull, "id")) {
            if repository {
                pull["body"] = entry["description"].clone();
            } else {
                pull[DESCRIPTION_READ] = entry;
            }
        }
    }
    Ok(())
}

/// After a write that already succeeded, a failed description read only warns.
async fn attach_description_after_write(
    api: &Api<'_>,
    output: &mut Value,
    selection: &crate::cli::pr_json::Selection,
) {
    if let Err(error) = attach_description(api, output, selection).await {
        eprintln!("The PR's description could not be read: {error}");
    }
}

fn invalid(message: &str) -> G5Error {
    G5Error::ValidationError {
        details: json!({"message": message}),
    }
}

pub async fn execute(args: &PrArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    match execute_with_status(args, global).await? {
        0 => Ok(()),
        status => Err(invalid(&format!("PR checks exited with status {status}"))),
    }
}

pub(crate) async fn execute_with_status(
    args: &PrArgs,
    global: &GlobalFlags,
) -> Result<i32, G5Error> {
    let fields = args
        .json_fields
        .as_ref()
        .map(|fields| fields.as_deref())
        .or(global.json.then_some(None));
    // `attachments` prints its whole report with --json, as `g5d issue
    // attachments` does, and selects no fields.
    let selection = match (&args.command, fields) {
        (PrCommand::Attachments(_), Some(Some(value))) => {
            return Err(invalid(&format!(
                "g5d pr attachments selects no JSON fields ({value}); use --json alone for the full report"
            )));
        }
        (PrCommand::Attachments(_), _) => crate::cli::pr_json::Selection::Human,
        _ => crate::cli::pr_json::Selection::parse(fields, &args.command)?,
    };
    if selection.print_available_fields() {
        return Ok(0);
    }
    // `PR######` names a G5 PR, which has no GitHub conversation: refuse before
    // any request (`g5.git.pr-repository`).
    let (conversation_target, refusal) = match &args.command {
        PrCommand::Comment(comment) => (comment.pull_request.as_deref(), G5_CONVERSATION_REFUSAL),
        PrCommand::Attachments(attachments) => (
            Some(attachments.pull_request.as_str()),
            G5_ATTACHMENTS_REFUSAL,
        ),
        _ => (None, G5_CONVERSATION_REFUSAL),
    };
    if let Some(name) = conversation_target.filter(|s| names_g5_pull_request(s)) {
        if Uuid::parse_str(name).is_err() || refusal == G5_ATTACHMENTS_REFUSAL {
            return Err(invalid(&format!("{} is a G5 PR. {refusal}.", name.trim())));
        }
    }
    if let PrCommand::Checks(checks) = &args.command {
        if checks
            .head
            .as_deref()
            .is_some_and(|h| h.len() != 40 || !h.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(invalid("--head requires a full 40-character commit SHA"));
        }
    }
    // `update` with new text is `edit` (`g5.git.pr-cli.update`).
    let edits_text = |write: &crate::cli::PrWriteArgs| {
        write.title.is_some() || write.body.is_some() || write.body_file.is_some()
    };
    let write = match &args.command {
        PrCommand::Create(create) => Some((&create.write, "create", create.replace)),
        PrCommand::Update(update) if edits_text(update) => Some((update, "edit", false)),
        PrCommand::Update(update) => Some((update, "update", false)),
        _ => None,
    };
    let mut create_body = match write {
        Some((args, mode, replace)) => {
            if args
                .title
                .as_deref()
                .is_some_and(|title| title.trim().is_empty())
            {
                return Err(invalid("PR title must not be blank"));
            }
            let description = read_body(args.body.as_deref(), args.body_file.as_ref())?;
            // g5d publishes committed work only (`g5.git.pr-cli.create-save`).
            let mut body = json!({
                "description": description, "mode": mode, "replace": replace, "save": false,
            });
            if let Some(target) = args.target_workspace_id {
                body["targetWorkspaceId"] = json!(target);
            }
            if let Some(title) = &args.title {
                body["title"] = json!(title);
            }
            Some(body)
        }
        _ => None,
    };
    let stable = stable_context::discover(global)?;
    let cwd = std::env::current_dir().map_err(|e| G5Error::Config(e.to_string()))?;
    let root = crate::cli::resolve_workspace_path(global.workspace.as_ref(), &cwd);
    let config = root
        .as_ref()
        .map(|root| WorkspaceConfig::load(root))
        .transpose()
        .map_err(|e| G5Error::Config(e.to_string()))?
        .flatten();
    let discovered = if let Some(explicit) = &global.workspace {
        // An explicit checkout never falls back to an ambient VM/workspace ID.
        // A stable workspace may have metadata instead of workspace.yaml.
        config
            .as_ref()
            .and_then(|c| c.workspace_id.clone())
            .or_else(|| {
                stable
                    .as_ref()
                    .filter(|context| {
                        explicit
                            .canonicalize()
                            .ok()
                            .zip(context.root.canonicalize().ok())
                            .is_some_and(|(a, b)| a == b)
                    })
                    .map(|context| context.metadata.workspace_id.clone())
            })
    } else {
        stable
            .as_ref()
            .map(|s| s.metadata.workspace_id.clone())
            .or_else(|| std::env::var("G5D_WORKSPACE_ID").ok())
            .or_else(|| config.as_ref().and_then(|c| c.workspace_id.clone()))
    };
    let workspace = args
        .workspace_id
        .or_else(|| discovered.as_deref().and_then(|s| s.parse().ok()))
        .ok_or_else(|| invalid("Run inside a G5 workspace or pass --workspace-id UUID"))?;
    let internal = stable.is_some()
        || (std::env::var_os("G5D_WORKSPACE_TOKEN_FILE").is_some()
            || std::env::var_os("G5D_WORKSPACE_TOKEN").is_some());
    let api = Api {
        global,
        internal,
        root: root.as_deref(),
        workspace,
    };
    if let PrCommand::Merge(merge) = &args.command {
        return merge_command(&api, merge, &selection).await;
    }
    if let PrCommand::Attachments(attachments) = &args.command {
        if !internal {
            return Err(invalid(
                "g5d pr attachments runs inside a workspace, as its agent",
            ));
        }
        let client = stable_context::require_worker_client(global)?;
        return crate::cli::issue_attachments::save_pull_request_attachments(
            &client,
            &workspace.to_string(),
            global.json || args.json_fields.is_some(),
            attachments,
        )
        .await;
    }
    if let PrCommand::Close(close) = &args.command {
        let pull = write::close(&api, close).await?;
        let mut output = json!({"source": "g5", "pullRequest": pull});
        attach_description_after_write(&api, &mut output, &selection).await;
        if let Some(projected) = selection.project(&output) {
            println!(
                "{}",
                serde_json::to_string_pretty(&projected).map_err(|e| invalid(&e.to_string()))?
            );
        } else if !global.quiet {
            let on_github = crate::cli::pr_json::github_publications(&output["pullRequest"])
                .iter()
                .any(|r| r["number"].is_number());
            println!(
                "Closed G5 pull request{}",
                if on_github {
                    "; its GitHub PRs stay open on GitHub."
                } else {
                    "."
                }
            );
            print!("{}", render(&output));
        }
        return Ok(0);
    }
    if let PrCommand::Edit(edit) = &args.command {
        if edit.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Err(invalid("PR title must not be blank"));
        }
        let description = read_body(edit.body.as_deref(), edit.body_file.as_ref())?;
        if edit.title.is_none() && description.is_none() {
            return Err(invalid(
                "Nothing to edit; pass --title, --body or --body-file",
            ));
        }
        let pull = write::g5_pull_request(&api, edit.pull_request.as_deref(), None).await?;
        if text(&pull, "sourceWorkspaceId") != workspace.to_string() {
            return Err(invalid(
                "g5d pr edit changes PRs out of this workspace; run it in the source workspace",
            ));
        }
        create_body = Some(json!({
            "mode": "edit", "save": false, "targetWorkspaceId": pull["targetWorkspaceId"],
            "title": edit.title, "description": description,
        }));
    }
    if let PrCommand::Create(create) = &args.command {
        let target = match &create.base {
            Some(name) => Some(write::target_by_name(&api, name).await?),
            None => create.write.target_workspace_id,
        };
        if create.dry_run {
            let preview = write::dry_run(&api, target).await?;
            if selection.project(&json!({})).is_some() || global.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&preview).map_err(|e| invalid(&e.to_string()))?
                );
            } else {
                println!(
                    "Would publish into workspace {} (nothing was published)\n\n{}\n\n{}",
                    text(&preview, "targetWorkspaceId"),
                    text(&preview, "title"),
                    text(&preview, "body")
                );
            }
            return Ok(0);
        }
        if let (Some(body), Some(target)) = (create_body.as_mut(), target) {
            body["targetWorkspaceId"] = json!(target);
        }
    }
    let publishes = matches!(&args.command, PrCommand::Create(_))
        || matches!(&args.command, PrCommand::Update(update) if !edits_text(update));
    if publishes {
        if matches!(&args.command, PrCommand::Update(_)) && !global.quiet {
            eprintln!(
                "A push already updates the PR while Auto-push is on; republishing the pushed head now."
            );
        }
        if let Err(refusal) = write::publish_committed_work().await? {
            eprintln!("{refusal}");
            return Ok(write::REFUSED);
        }
    }
    // A branch names its GitHub PR (`g5.git.pr-cli.selectors`).
    let resolved = match &args.command {
        PrCommand::View(view) => match write::classify(view.pull_request.as_deref()) {
            write::Selector::Branch(branch) => {
                let (number, repository) = branch_selector(
                    write::branch_pull_request(&api, branch, view.repo.as_deref()).await?,
                );
                Some(PrCommand::View(PrViewArgs {
                    pull_request: Some(number),
                    repo: repository,
                    comments: view.comments,
                }))
            }
            _ => None,
        },
        PrCommand::Checks(checks) => match write::classify(checks.pull_request.as_deref()) {
            write::Selector::Branch(branch) => {
                let (number, repository) = branch_selector(
                    write::branch_pull_request(&api, branch, checks.repo.as_deref()).await?,
                );
                Some(PrCommand::Checks(crate::cli::PrChecksArgs {
                    pull_request: Some(number),
                    repo: repository,
                    ..checks.clone()
                }))
            }
            _ => None,
        },
        _ => None,
    };
    let command = resolved.as_ref().unwrap_or(&args.command);
    if matches!(args.command, PrCommand::Comment(_) | PrCommand::Resolve(_)) {
        return conversation(&api, &args.command, &selection).await;
    }
    if let PrCommand::Checks(checks) = command {
        let path = checks_path(workspace, internal, checks, selection.needs_details())?;
        let (path, root) = (&path, &root);
        let read = move || async move {
            if internal {
                let client = stable_context::require_worker_client(global)?;
                let response: Value = client.get_json(path).await?;
                response
                    .get("data")
                    .cloned()
                    .ok_or_else(|| invalid("PR checks response is missing data"))
            } else {
                let client = match root {
                    Some(root) => ApiClient::new(&crate::cli::resolve_api_url(global), root),
                    None => ApiClient::new_global(&crate::cli::resolve_api_url(global)),
                };
                client.ensure_auth().await?;
                client.get::<Value>(path).await
            }
        };
        return watch_checks(checks, global, &selection, read).await;
    }
    let route = request_path(workspace, internal, command, &selection)?;
    let sent = async {
        if internal {
            let client = stable_context::require_worker_client(global)?;
            let response: Value = match &create_body {
                Some(body) => {
                    client
                        .create_workspace_pull_request(&workspace.to_string(), body)
                        .await?
                }
                None => client.get_json(&route).await?,
            };
            response
                .get("data")
                .cloned()
                .ok_or_else(|| invalid("PR API response is missing data"))
        } else {
            let client = match &root {
                Some(root) => ApiClient::new(&crate::cli::resolve_api_url(global), root),
                None => ApiClient::new_global(&crate::cli::resolve_api_url(global)),
            };
            client.ensure_auth().await?;
            match &create_body {
                Some(body) => client.post_reconciling::<_, Value>(&route, body).await,
                None => client.get::<Value>(&route).await,
            }
        }
    };
    let data = match sent.await {
        Ok(data) => data,
        Err(error) if create_body.is_some() => return write_failure(error),
        Err(error) => return Err(error),
    };
    let output = if create_body.is_some() {
        if !data["pullRequest"].is_object() {
            return Err(invalid("PR API response is missing pullRequest"));
        }
        let mut pull = data["pullRequest"].clone();
        pull["publication"] = data["publication"].clone();
        let mut output =
            json!({"source": "g5", "pullRequest": pull, "alreadyExisted": data["alreadyExisted"]});
        attach_description_after_write(&api, &mut output, &selection).await;
        output
    } else {
        let mut output = select_output(data, workspace, command)?;
        attach_description(&api, &mut output, &selection).await?;
        attach_descriptions(&api, &mut output, &selection).await?;
        output
    };
    // A repository without GitHub integration was listed from G5's records
    // (`g5.git.pr-list.local`); say so even when JSON is selected, unless the
    // repository was never expected on GitHub (`g5.git.pr-no-github`).
    if let Some(reason) = output["fallback"].as_str() {
        if !global.quiet && output["githubExpected"] != false {
            eprintln!("{reason}; listing G5's stored PRs instead of GitHub's.");
        }
    }
    if let Some(output) = selection.project(&output) {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| G5Error::Internal {
                message: e.to_string()
            })?
        );
    } else if !global.quiet {
        if let Some(body) = &create_body {
            println!(
                "{} G5 pull request",
                match body["mode"].as_str() {
                    Some("edit") => "Edited",
                    Some("update") => "Republished",
                    _ if output["alreadyExisted"] == true => "Updated",
                    _ => "Created",
                }
            );
        }
        print!("{}", render(&output));
        if create_body.is_some() {
            print!(
                "{}",
                render_publication(&output["pullRequest"]["publication"])
            );
        }
    }
    if create_body.is_some() && output["pullRequest"]["publication"]["complete"] != true {
        eprintln!("The G5 pull request is saved, but GitHub publication is incomplete or pending; see the reasons above.");
        return Ok(PUBLICATION_INCOMPLETE);
    }
    Ok(0)
}

/// The G5 PR is saved, but GitHub publication is incomplete or pending.
const PUBLICATION_INCOMPLETE: i32 = 5;

/// Exit status of a failed create/update: what was saved decides it.
fn write_failure(error: G5Error) -> Result<i32, G5Error> {
    // A save failure keeps its own status: a 409 arrives as a typed conflict,
    // a 422 as validation details (nested by g5d's own client).
    let kind = match &error {
        G5Error::ConflictDetails { details, .. } => details["kind"].as_str(),
        G5Error::ValidationError { details } => details["kind"]
            .as_str()
            .or_else(|| details["details"]["kind"].as_str()),
        _ => None,
    };
    let status = match kind {
        Some("pull_request_provider_sync_incomplete") => PUBLICATION_INCOMPLETE,
        Some("pull_request_source_save_failed") => crate::cli::exit_codes::REPO_GIT_ERROR,
        _ => return Err(error),
    };
    eprintln!("{error}");
    Ok(status)
}

/// One line per GitHub repository this create/update touched.
fn render_publication(publication: &Value) -> String {
    let Some(repos) = publication["repositories"].as_array() else {
        return String::new();
    };
    let mut out = String::new();
    if !repos.is_empty() {
        out.push_str("\nGitHub:\n");
    }
    for repo in repos {
        let number = repo["number"]
            .as_i64()
            .map_or_else(|| "(no PR)".to_owned(), |n| format!("#{n}"));
        let mut parts = vec![text(repo, "action").replace('_', " ")];
        if let Some(head) = repo["head"].as_str() {
            parts.push(format!("head {head}"));
        }
        for field in ["title", "body"] {
            if let Some(state) = repo[field].as_str() {
                parts.push(format!("{field} {}", state.replace('_', " ")));
            }
        }
        out.push_str(&format!(
            "  {} {number} · {}\n",
            text(repo, "repository"),
            parts.join(" · ")
        ));
        if let Some(message) = repo["message"].as_str() {
            out.push_str(&format!("    {message}\n"));
        }
    }
    out
}

/// `-b TEXT`, or `-F PATH` (`-` for stdin).
fn read_body(body: Option<&str>, file: Option<&PathBuf>) -> Result<Option<String>, G5Error> {
    let read = |result: std::io::Result<String>| {
        result
            .map(Some)
            .map_err(|e| invalid(&format!("Read body: {e}")))
    };
    match file {
        Some(path) if path.as_os_str() == "-" => read(std::io::read_to_string(std::io::stdin())),
        Some(path) => read(std::fs::read_to_string(path)),
        None => Ok(body.map(str::to_owned)),
    }
}

/// Where PR API calls go: the workspace's internal API with its token, or
/// the public API with the user's login.
struct Api<'a> {
    global: &'a GlobalFlags,
    internal: bool,
    root: Option<&'a std::path::Path>,
    workspace: Uuid,
}

impl Api<'_> {
    fn public_client(&self) -> ApiClient {
        let url = crate::cli::resolve_api_url(self.global);
        match self.root {
            Some(root) => ApiClient::new(&url, root),
            None => ApiClient::new_global(&url),
        }
    }
}

/// Exit status of a comment whose outcome GitHub did not make clear.
const OUTCOME_UNCLEAR: i32 = 7;

/// `g5d pr merge`: merge now, or turn merge when ready on or off
/// (`g5.git.pr-auto-merge`).
async fn merge_command(
    api: &Api<'_>,
    args: &crate::cli::PrMergeArgs,
    selection: &crate::cli::pr_json::Selection,
) -> Result<i32, G5Error> {
    if api.internal && args.auto {
        eprintln!(
            "Only a person can turn on merge when ready. Ask the workspace owner to turn it on \
             in Prepare to Ship in the G5 web app."
        );
        return Ok(crate::cli::exit_codes::NETWORK_AUTH_ERROR);
    }
    // A G5 ID, GitHub number or URL, or branch (`g5.git.pr-cli.selectors`).
    let pull = write::g5_pull_request(api, args.pull_request.as_deref(), None).await?;
    let (id, target) = (text(&pull, "id"), text(&pull, "targetWorkspaceId"));
    let workspace = api.workspace;
    let data: Value = if api.internal {
        let client = stable_context::require_worker_client(api.global)?;
        let (suffix, idempotent) = if args.disable_auto {
            ("auto-merge/disable", true)
        } else {
            // Merging is sent once: an unclear outcome is reported, not repeated.
            ("merge", false)
        };
        let response = client
            .workspace_agent_post(
                &workspace.to_string(),
                &format!("pull-requests/{id}/{suffix}"),
                &json!({}),
                idempotent,
            )
            .await?;
        response
            .get("data")
            .cloned()
            .ok_or_else(|| invalid("PR API response is missing data"))?
    } else {
        let client = api.public_client();
        client.ensure_auth().await?;
        let base = format!("/api/v1/workspaces/{target}/pull-requests/{id}");
        if args.auto {
            client
                .post::<_, Value>(&format!("{base}/auto-merge"), &json!({}))
                .await?
        } else if args.disable_auto {
            client
                .delete::<Value>(&format!("{base}/auto-merge"))
                .await?
        } else {
            client
                .post::<_, Value>(&format!("{base}/merge"), &json!({}))
                .await?
        }
    };
    let pull = data
        .get("pullRequest")
        .cloned()
        .ok_or_else(|| invalid("PR API response is missing pullRequest"))?;
    let output = json!({"source": "g5", "pullRequest": pull});
    if let Some(projected) = selection.project(&output) {
        println!(
            "{}",
            serde_json::to_string_pretty(&projected).map_err(|e| invalid(&e.to_string()))?
        );
    } else if !api.global.quiet {
        let pull = &output["pullRequest"];
        let name = format!(
            "{} → {}",
            text(pull, "sourceWorkspaceName"),
            text(pull, "targetWorkspaceName")
        );
        println!(
            "{}",
            if args.auto {
                format!("Merge when ready is on for {name}; G5 merges it once it is ready.")
            } else if args.disable_auto {
                format!("Merge when ready is off for {name}.")
            } else {
                format!("Merged {name}.")
            }
        );
        print!("{}", render(&output));
    }
    Ok(0)
}

/// `g5d pr comment` and `g5d pr resolve`.
async fn conversation(
    api: &Api<'_>,
    command: &PrCommand,
    selection: &crate::cli::pr_json::Selection,
) -> Result<i32, G5Error> {
    let workspace = api.workspace;
    let (suffix, body, key) = match command {
        PrCommand::Comment(args) => {
            let text = read_body(args.body.as_deref(), args.body_file.as_ref())?
                .ok_or_else(|| invalid("Supply the comment with -b TEXT or -F PATH"))?;
            if text.trim().is_empty() {
                return Err(invalid("The comment must not be empty"));
            }
            if args.pull_request.is_none() && args.reply_to.is_none() {
                return Err(invalid(
                    "Supply a GitHub PR number or URL, or --reply-to THREAD_ID",
                ));
            }
            let key = args
                .idempotency_key
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let body = json!({
                "selector": args.pull_request, "repo": args.repo, "body": text,
                "replyTo": args.reply_to, "idempotencyKey": key,
            });
            ("comments", body, Some(key))
        }
        PrCommand::Resolve(args) => (
            "threads/resolve",
            json!({"thread": args.thread, "repo": args.repo, "resolved": !args.unresolve}),
            None,
        ),
        _ => unreachable!("only conversation writes"),
    };
    let sent = async {
        if api.internal {
            let client = stable_context::require_worker_client(api.global)?;
            let workspace = workspace.to_string();
            let response = if key.is_some() {
                client.post_pull_request_comment(&workspace, &body).await?
            } else {
                client.resolve_review_thread(&workspace, &body).await?
            };
            response
                .get("data")
                .cloned()
                .ok_or_else(|| invalid("PR API response is missing data"))
        } else {
            let client = api.public_client();
            client.ensure_auth().await?;
            client
                .post::<_, Value>(
                    &format!("/api/v1/workspaces/{workspace}/pull-requests/github/{suffix}"),
                    &body,
                )
                .await
        }
    };
    let data = match sent.await {
        Ok(data) => data,
        Err(error) => {
            if let Some(key) = &key {
                // The comment may still have been posted: a rerun with this
                // key checks for it instead of posting it again.
                eprintln!("To retry this comment safely, pass --idempotency-key {key}");
            }
            return Err(error);
        }
    };
    let result = match &key {
        Some(key) => comment_result(&data, key),
        None => data,
    };
    if let Some(output) = selection.project(&json!({"result": result})) {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| invalid(&e.to_string()))?
        );
    } else if !api.global.quiet {
        print!("{}", render_conversation(&result));
    }
    Ok(match result["state"].as_str() {
        Some("failed") => 1,
        Some("pending" | "outcome_unknown") => {
            if let Some(key) = &key {
                eprintln!(
                    "GitHub did not confirm the comment. Rerun with --idempotency-key {key} to check for it without posting again."
                );
            }
            OUTCOME_UNCLEAR
        }
        _ => 0,
    })
}

/// The recorded comment submission in CLI form.
fn comment_result(data: &Value, key: &str) -> Value {
    let display = text(data, "displayKey");
    let (repository, number) = display
        .rsplit_once('#')
        .map_or((display, None), |(repo, n)| (repo, n.parse::<i64>().ok()));
    json!({
        "id": data["id"], "state": data["state"], "url": data["externalCommentUrl"],
        "destination": data["destination"], "threadId": data["threadId"],
        "repository": repository, "number": number,
        "author": {"login": data["githubLogin"]}, "body": data["body"], "origin": data["origin"],
        "idempotencyKey": key, "createdAt": data["createdAt"], "publishedAt": data["publishedAt"],
        "error": data["error"],
    })
}

fn render_conversation(result: &Value) -> String {
    let at = format!("{}#{}", text(result, "repository"), result["number"]);
    if let Some(thread) = result["threadId"]
        .as_str()
        .filter(|_| result.get("isResolved").is_some())
    {
        let state = if result["isResolved"] == true {
            "resolved"
        } else {
            "unresolved"
        };
        return if result["changed"] == true {
            format!("Marked review thread {thread} on {at} {state}\n")
        } else {
            format!("Review thread {thread} on {at} was already {state}\n")
        };
    }
    let place = match result["threadId"].as_str() {
        Some(thread) => format!("review thread {thread} on {at}"),
        None => at,
    };
    match text(result, "state") {
        "published" => format!("{}\n", text(result, "url")),
        "failed" => format!(
            "GitHub refused the comment on {place}: {}\n",
            text(&result["error"], "message")
        ),
        _ => format!("Comment on {place} not confirmed by GitHub\n"),
    }
}

fn request_path(
    workspace: Uuid,
    internal: bool,
    command: &PrCommand,
    selection: &crate::cli::pr_json::Selection,
) -> Result<String, G5Error> {
    let prefix = if internal { "internal" } else { "v1" };
    if matches!(command, PrCommand::Create(_) | PrCommand::Update(_)) {
        return Ok(format!(
            "/api/{prefix}/workspaces/{workspace}/pull-requests"
        ));
    }
    if let PrCommand::List(args) = command {
        if args.repository_mode() {
            return repository_list_path(prefix, workspace, args, selection.selects_body());
        }
    }
    let path = format!("/api/{prefix}/workspaces/{workspace}/pull-requests/info");
    let mut url = reqwest::Url::parse(&format!("http://placeholder{path}")).unwrap();
    // Readable G5 PRs beyond this workspace (`g5.git.pr-reads`).
    if let PrCommand::List(args) = command {
        if args.readable_mode() {
            let mut query = url.query_pairs_mut();
            match &args.in_workspace {
                Some(workspace) if !args.all => query.append_pair("workspace", workspace),
                _ => query.append_pair("all", "true"),
            };
            query.append_pair("state", &args.state);
            query.append_pair("limit", &args.limit.to_string());
            for (key, value) in [("author", &args.author), ("search", &args.search)] {
                if let Some(value) = value {
                    query.append_pair(key, value);
                }
            }
        }
    }
    if let PrCommand::View(PrViewArgs {
        pull_request,
        repo,
        comments,
    }) = command
    {
        let github = pull_request
            .as_deref()
            .filter(|s| !names_g5_pull_request(s));
        if github.is_none() && (repo.is_some() || *comments) {
            return Err(invalid(
                "--repo and --comments require a GitHub PR number or URL",
            ));
        }
        // A G5 PR ID or `PR######` name reads any G5 PR you can read
        // (`g5.git.pr-reads`).
        if let Some(name) = pull_request.as_deref().filter(|s| names_g5_pull_request(s)) {
            url.query_pairs_mut()
                .append_pair("selector", &write::server_selector(name));
        }
        if let Some(selector) = github {
            url.query_pairs_mut().append_pair("selector", selector);
            if let Some(repo) = repo {
                url.query_pairs_mut().append_pair("repo", repo);
            }
            if *comments || selection.needs_comments() {
                url.query_pairs_mut().append_pair("comments", "true");
            }
            if *comments || selection.needs_reviews() {
                url.query_pairs_mut().append_pair("reviews", "true");
            }
        }
    }
    Ok(match url.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    })
}

/// `g5d pr list` in repository mode (`g5.git.pr-list`).
fn repository_list_path(
    prefix: &str,
    workspace: Uuid,
    args: &crate::cli::PrListArgs,
    body: bool,
) -> Result<String, G5Error> {
    if args.limit > REPOSITORY_LIST_MAX {
        return Err(invalid(
            "A repository listing returns at most 100 PRs; use --limit 1-100 and narrow the filters",
        ));
    }
    let path = format!("/api/{prefix}/workspaces/{workspace}/pull-requests/github");
    let mut url = reqwest::Url::parse(&format!("http://placeholder{path}")).unwrap();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("state", &args.state);
        query.append_pair("limit", &args.limit.to_string());
        if body {
            query.append_pair("body", "true");
        }
        for (key, value) in [
            ("repo", &args.repo),
            ("head", &args.head),
            ("base", &args.base),
            ("author", &args.author),
            ("search", &args.search),
        ] {
            if let Some(value) = value {
                query.append_pair(key, value);
            }
        }
        if !args.label.is_empty() {
            query.append_pair("label", &args.label.join(","));
        }
    }
    Ok(format!("{path}?{}", url.query().unwrap_or_default()))
}

/// Most PRs a repository listing returns (one GitHub page).
const REPOSITORY_LIST_MAX: u32 = 100;

fn select_output(mut data: Value, workspace: Uuid, command: &PrCommand) -> Result<Value, G5Error> {
    if matches!(command, PrCommand::List(args) if args.repository_mode()) {
        if !data["pullRequests"].is_array() {
            return Err(invalid("PR API response is missing pullRequests"));
        }
        data["mode"] = json!("repository");
        return Ok(data);
    }
    if data["source"] == "github" {
        return Ok(data);
    }
    // The server answered for one readable PR, or listed and limited the
    // readable PRs itself (`g5.git.pr-reads`).
    if data["pullRequest"].is_object() {
        return Ok(json!({"source": "g5", "pullRequest": data["pullRequest"]}));
    }
    if data["listing"] == true {
        return Ok(data);
    }
    let mut pulls = data["pullRequests"]
        .as_array()
        .cloned()
        .ok_or_else(|| invalid("PR API response is missing pullRequests"))?;
    pulls.sort_by(|a, b| {
        text(b, "createdAt")
            .cmp(text(a, "createdAt"))
            .then_with(|| text(b, "id").cmp(text(a, "id")))
    });
    match command {
        PrCommand::Checks(_) => unreachable!("checks uses live provider observations"),
        PrCommand::Merge(_) => unreachable!("merge resolves its pull request through view"),
        PrCommand::Create(_) | PrCommand::Update(_) | PrCommand::Edit(_) => {
            unreachable!("create/update/edit use their response directly")
        }
        PrCommand::Close(_) => unreachable!("close uses its response directly"),
        PrCommand::Comment(_) | PrCommand::Resolve(_) => {
            unreachable!("conversation writes use their response directly")
        }
        PrCommand::Attachments(_) => unreachable!("attachments are saved by their own command"),
        PrCommand::List(args) => {
            pulls.retain(|p| args.state == "all" || p["status"] == args.state);
            let has_more = pulls.len() > args.limit as usize;
            pulls.truncate(args.limit as usize);
            data["pullRequests"] = json!(pulls);
            data["hasMore"] = json!(has_more);
        }
        PrCommand::Status => {
            pulls.retain(|p| p["status"] == "open");
            data["pullRequests"] = json!(pulls);
        }
        PrCommand::View(args) => {
            let selected = match args.pull_request.as_deref() {
                Some(id) => pulls
                    .iter()
                    .find(|p| text(p, "id").parse::<Uuid>().ok() == id.parse().ok()),
                None => {
                    let outgoing =
                        |p: &&Value| text(p, "sourceWorkspaceId") == workspace.to_string();
                    pulls
                        .iter()
                        .filter(outgoing)
                        .find(|p| p["status"] == "open")
                        .or_else(|| pulls.iter().find(outgoing))
                }
            }
            .ok_or_else(|| G5Error::NotFound {
                resource:
                    "pull request; use `g5d pr list --state all` or supply a GitHub number/URL"
                        .into(),
            })?;
            return Ok(json!({"source": "g5", "pullRequest": selected}));
        }
    }
    Ok(data)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn render(data: &Value) -> String {
    let mut out = String::new();
    if data["mode"] == "repository" {
        render_repository_list(&mut out, data);
    } else if data["source"] == "github" {
        let p = &data["pullRequest"];
        out.push_str(&format!(
            "{} #{}: {}\n{}{} · {}\n{} → {}\n{}\n\n{}\n",
            text(p, "repository"),
            p["number"],
            text(p, "title"),
            text(p, "state"),
            if p["isDraft"] == true { " (draft)" } else { "" },
            text(&p["author"], "login"),
            text(p, "headRefName"),
            text(p, "baseRefName"),
            text(p, "url"),
            text(p, "body")
        ));
        out.push_str(&match p["state"].as_str() {
            Some("MERGED" | "CLOSED") => "Mergeable: not applicable\n".to_string(),
            _ => format!(
                "Mergeable: {}{} · merge state: {}\n",
                crate::cli::pr_json::mergeable(&p["mergeable"]),
                if p["mergeable"].is_null() {
                    " (GitHub is still computing it; read again shortly)"
                } else {
                    ""
                },
                crate::cli::pr_json::merge_state(&p["mergeStateStatus"]),
            ),
        });
        render_g5_links(&mut out, &p["g5PullRequests"]);
        if let Some(latest) = p["latestReviews"].as_array() {
            let reviewers: Vec<_> = latest
                .iter()
                .map(|r| format!("{} ({})", text(&r["author"], "login"), text(r, "state")))
                .collect();
            out.push_str(&format!(
                "\nReview decision: {}\nLatest reviews: {}{}\n",
                p["reviewDecision"].as_str().unwrap_or("none"),
                if reviewers.is_empty() {
                    "none".into()
                } else {
                    reviewers.join(", ")
                },
                if p["latestReviewsHasMore"] == true {
                    ", and more on GitHub"
                } else {
                    ""
                }
            ));
        }
        if let Some(comments) = p["comments"].as_array() {
            out.push_str(&match p["commentCount"].as_u64() {
                Some(total) if total > comments.len() as u64 => {
                    format!("\nDiscussion comments ({} of {total})\n", comments.len())
                }
                _ => format!("\nDiscussion comments ({})\n", comments.len()),
            });
            for c in comments {
                out.push_str(&format!(
                    "\n{} · {}\n{}\n",
                    text(&c["author"], "login"),
                    text(c, "createdAt"),
                    text(c, "body")
                ));
            }
            if p["commentsHasMore"] == true {
                out.push_str("\nMore comments are available on GitHub (showing the first 100).\n");
            }
        }
        if let Some(reviews) = p["reviews"].as_array() {
            out.push_str(&format!("\nReviews ({})\n", reviews.len()));
            for r in reviews {
                out.push_str(&format!(
                    "\n{} · {} · {}\n",
                    text(&r["author"], "login"),
                    text(r, "state"),
                    text(r, "submittedAt")
                ));
                if !text(r, "body").is_empty() {
                    out.push_str(&format!("{}\n", text(r, "body")));
                }
            }
            if p["reviewsHasMore"] == true {
                out.push_str("\nMore reviews are available on GitHub.\n");
            }
        }
        if let Some(threads) = p["reviewThreads"].as_array() {
            render_review_threads(&mut out, threads);
            if p["reviewThreadsHasMore"] == true {
                out.push_str("\nMore review threads are available on GitHub.\n");
            }
        }
    } else {
        out.push_str("G5 workspace PRs (stored state)\n");
        if let Some(p) = data.get("pullRequest") {
            render_workspace_pr(&mut out, p);
            render_description(&mut out, &p[DESCRIPTION_READ]);
        } else if let Some(pulls) = data["pullRequests"].as_array() {
            if pulls.is_empty() {
                out.push_str("No matching pull requests.\n");
            }
            for p in pulls {
                render_workspace_pr(&mut out, p);
            }
            if data["hasMore"] == true {
                out.push_str("More results available; increase --limit.\n");
            }
        }
    }
    // Provider/user text is untrusted terminal content, including OSC escape
    // sequences. Preserve ordinary Markdown and line breaks, not controls.
    out.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

/// The canonical description, then the landing's notice when a publication
/// differs from it or could not be compared (`g5.git.pr-description`).
fn render_description(out: &mut String, read: &Value) {
    if !read.is_object() {
        return;
    }
    match read["description"].as_str() {
        Some(description) => out.push_str(&format!("\n{description}\n")),
        None => out.push_str("\nNo description saved for this PR.\n"),
    }
    let repos = read["repositories"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let name = |r: &Value| format!("{} #{}", text(r, "repository"), r["number"]);
    let differs: Vec<_> = repos
        .iter()
        .filter(|r| r["comparison"] == "differs")
        .map(name)
        .collect();
    let unchecked: Vec<_> = repos
        .iter()
        .filter(|r| r["comparison"] == "unavailable")
        .map(|r| match r["error"].as_str() {
            Some(error) => format!("{} ({error})", name(r)),
            None => name(r),
        })
        .collect();
    if !differs.is_empty() {
        out.push_str(&format!(
            "\nGitHub description differs: {}\n",
            differs.join(", ")
        ));
    }
    if !unchecked.is_empty() {
        out.push_str(&format!(
            "\nThe GitHub description could not be compared: {}\n",
            unchecked.join(", ")
        ));
    }
}

/// A repository's PRs, one per line: GitHub's number, a local repository
/// pull request's G5 PR name (`PR3fa9c1`), or `-` for a stored G5 PR not yet on
/// GitHub; state, title, head branch and the linked G5 PRs.
fn render_repository_list(out: &mut String, data: &Value) {
    let pulls = data["pullRequests"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    out.push_str(&if data["source"] == "g5" {
        format!("{}: G5 stored PRs\n", text(data, "repository"))
    } else {
        format!("{}: GitHub PRs, newest first\n", text(data, "repository"))
    });
    if pulls.is_empty() {
        out.push_str("No matching pull requests.\n");
    }
    for p in pulls {
        let number = match p["number"].as_i64() {
            Some(n) => format!("#{n}"),
            None => g5_name(&p["g5Number"]).unwrap_or_else(|| "-".to_owned()),
        };
        let draft = if p["isDraft"] == true { " (draft)" } else { "" };
        out.push_str(&format!(
            "{number}\t{}{draft}\t{}\t{}\n",
            text(p, "state"),
            text(p, "title"),
            text(p, "headRefName"),
        ));
        render_g5_links(out, &p["g5PullRequests"]);
    }
    if data["hasMore"] == true {
        out.push_str("More results available; narrow the filters or increase --limit.\n");
    }
}

/// The G5 PRs linking a GitHub PR (`g5.git.pr-g5-identity`).
fn render_g5_links(out: &mut String, links: &Value) {
    for link in links.as_array().into_iter().flatten() {
        let workspace = |name: &str, deleted: &str| {
            if link[deleted] == true {
                format!("{} (deleted)", text(link, name))
            } else {
                text(link, name).to_string()
            }
        };
        let name = g5_name(&link["number"]).unwrap_or_else(|| "G5 PR".to_owned());
        out.push_str(&format!(
            "  {name} {} ({}): {} → {}\n",
            text(link, "id"),
            text(link, "status"),
            workspace("sourceWorkspaceName", "sourceWorkspaceDeleted"),
            workspace("targetWorkspaceName", "targetWorkspaceDeleted"),
        ));
    }
}

/// Inline review threads, unresolved first, each headed by its file location
/// and whether it is resolved or outdated against the current head.
fn render_review_threads(out: &mut String, threads: &[Value]) {
    let unresolved = threads.iter().filter(|t| t["isResolved"] != true).count();
    out.push_str(&format!(
        "\nReview threads ({}; {unresolved} unresolved)\n",
        threads.len()
    ));
    let mut ordered: Vec<_> = threads.iter().collect();
    ordered.sort_by_key(|t| t["isResolved"] == true);
    for t in ordered {
        let (line, start, original) = match t["line"].as_u64() {
            Some(line) => (Some(line), t["startLine"].as_u64(), ""),
            None => (
                t["originalLine"].as_u64(),
                t["originalStartLine"].as_u64(),
                " (original)",
            ),
        };
        let location = match (start, line) {
            (Some(start), Some(line)) if start != line => format!(":{start}-{line}{original}"),
            (_, Some(line)) => format!(":{line}{original}"),
            _ => String::new(),
        };
        let mut state = if t["isResolved"] == true {
            match t["resolvedBy"]["login"].as_str() {
                Some(login) => format!("resolved by {login}"),
                None => "resolved".into(),
            }
        } else {
            "unresolved".into()
        };
        if t["isOutdated"] == true {
            state.push_str(" · outdated");
        }
        out.push_str(&format!("\n{}{location} · {state}\n", text(t, "path")));
        if let Some(id) = t["id"].as_str() {
            out.push_str(&format!("thread {id}\n"));
        }
        for c in t["comments"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "{} · {}\n{}\n",
                text(&c["author"], "login"),
                text(c, "createdAt"),
                text(c, "body")
            ));
        }
        if t["commentsHasMore"] == true {
            out.push_str("More replies are available on GitHub.\n");
        }
    }
}

fn render_workspace_pr(out: &mut String, p: &Value) {
    // A deleted side keeps its PRs readable and says so (`g5.git.pr-reads`).
    let workspace = |name: &str, deleted: &str| {
        if p[deleted] == true {
            format!("{} (deleted)", text(p, name))
        } else {
            text(p, name).to_string()
        }
    };
    let name = g5_name(&p["number"]);
    let number = name
        .as_deref()
        .map_or_else(String::new, |name| format!("{name} · "));
    out.push_str(&format!(
        "\n{}  {}  {}\n  {number}{} → {} · local: {} · provider: {}\n",
        text(p, "id"),
        text(p, "status"),
        text(p, "title"),
        workspace("sourceWorkspaceName", "sourceWorkspaceDeleted"),
        workspace("targetWorkspaceName", "targetWorkspaceDeleted"),
        text(p, "localMergeStatus"),
        text(p, "providerStatus")
    ));
    // Forwarding only matters for a PR with GitHub records (`g5.git.pr-no-github`).
    let github = crate::cli::pr_json::github_publications(p);
    let published = !github.is_empty();
    if p["sourceAutoPush"] == false && p["status"] == "open" && published {
        out.push_str(
            "  Auto-push is off for the source workspace: pushes stay in G5 until g5d pr update (Update PR) publishes them.\n",
        );
    }
    if !text(p, "blockingMessage").is_empty() {
        out.push_str(&format!("  {}\n", text(p, "blockingMessage")));
    }
    // g5.workspace.merge.admin-override.formal: an override stays flagged.
    let merge_override = &p["mergeOverride"];
    if merge_override.is_object() {
        let by = merge_override["actorName"]
            .as_str()
            .unwrap_or("an administrator");
        let self_override = if merge_override["selfOverride"] == true {
            " (self-override)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  Merged with override by {by}{self_override}: {}\n",
            text(merge_override, "reason")
        ));
        for approval in merge_override["waivedApprovals"]
            .as_array()
            .into_iter()
            .flatten()
        {
            out.push_str(&format!(
                "    waived approval: {} ({})\n",
                text(approval, "definitionName"),
                text(approval, "state")
            ));
        }
        for repository in merge_override["waivedChecks"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let checks: Vec<String> = repository["checks"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|check| format!("{} {}", text(check, "name"), text(check, "bucket")))
                .collect();
            out.push_str(&format!(
                "    waived checks in {}: {}\n",
                text(repository, "repository"),
                if checks.is_empty() {
                    text(repository, "availability").to_string()
                } else {
                    checks.join(", ")
                }
            ));
        }
        for repository in merge_override["githubRepositories"]
            .as_array()
            .into_iter()
            .flatten()
        {
            out.push_str(&format!(
                "    GitHub rules bypassed in {} ({})\n",
                text(repository, "repository"),
                text(repository, "mergeState")
            ));
        }
    }
    let auto = &p["autoMergeRequest"];
    if auto.is_object() {
        out.push_str(&format!(
            "  Merge when ready: on since {}",
            text(auto, "enabledAt")
        ));
        match auto["reason"].as_str() {
            Some(reason) => out.push_str(&format!(" · {reason}\n")),
            None => out.push('\n'),
        }
    }
    let evidence = &p["mergeEvidence"];
    match evidence["stale"].as_bool() {
        Some(true) => {
            out.push_str("  Merge evidence is stale: the target has moved since it was computed.\n")
        }
        None if evidence.is_object() => out.push_str(
            "  Merge evidence freshness is unknown: a target branch could not be read.\n",
        ),
        _ => {}
    }
    for r in evidence["repos"].as_array().into_iter().flatten() {
        let paths: Vec<_> = r["conflictPaths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if paths.is_empty() {
            continue;
        }
        // Every changed member has a repository pull request to name it by.
        let name = p["repositoryPullRequests"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(github.iter())
            .find(|repo| repo["repoId"] == r["repoId"])
            .map_or_else(|| text(r, "repoId"), |repo| text(repo, "repository"));
        out.push_str(&format!(
            "  Conflicts in {name} → {}:\n",
            text(r, "targetBranch")
        ));
        for path in paths {
            out.push_str(&format!("    {path}\n"));
        }
    }
    for r in &github {
        out.push_str(&format!(
            "  {} {} {}\n",
            text(r, "repository"),
            text(r, "providerStatus"),
            text(r, "url")
        ));
        if !text(r, "lastError").is_empty() {
            out.push_str(&format!("    {}\n", text(r, "lastError")));
        }
    }
    // Local repository pull requests are named by their G5 PR and share its
    // lifecycle (`g5.git.pr-repository`); GitHub's are listed above.
    for r in p["repositoryPullRequests"].as_array().into_iter().flatten() {
        if r["adapter"] == "g5" {
            out.push_str(&format!(
                "  {} {} {} (not on GitHub)\n",
                text(r, "repository"),
                name.as_deref().unwrap_or("G5 PR"),
                text(r, "state")
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_repository_pull_requests_pass_checks_and_render_as_g5s() {
        // g5.git.pr-no-github: a member not on GitHub has nothing to wait for.
        let data = json!({"source":"github","repositories":[
            {"repository":"acme/api","number":42,"availability":"ready","headRefOid":"a",
             "checks":[{"bucket":"pass","name":"ci"}]},
            {"repository":"docs","tracker":"g5","number":null,"g5Number":"PR000007","availability":"ready","noExternalChecks":true,
             "message":"No external checks apply: this repository is not on GitHub.","checks":[]},
        ]});
        let outcome = check_outcome(&data).unwrap();
        assert!(!outcome.pending && !outcome.failed && !outcome.unavailable);
        let text = render_checks(&data);
        assert!(
            text.contains("docs (PR000007)\n  No external checks apply"),
            "{text}"
        );
        assert!(
            !text.contains("No checks reported for this revision yet"),
            "{text}"
        );
    }

    #[test]
    fn a_g5_pr_shows_its_number_local_rows_and_deleted_workspaces() {
        let mut out = String::new();
        render_workspace_pr(
            &mut out,
            &json!({"id":"pr-1","number":"PR000012","status":"closed","title":"Docs",
                "sourceWorkspaceName":"child","sourceWorkspaceDeleted":true,
                "targetWorkspaceName":"parent","targetWorkspaceDeleted":false,
                "localMergeStatus":"closed","providerStatus":"local_only","providerRepos":[],
                "repositoryPullRequests":[{"repository":"docs","adapter":"g5","number":null,"state":"CLOSED"}]}),
        );
        // A G5 PR is named `PR000012`, never with a GitHub-style `#12`.
        assert!(out.contains("PR000012 · child (deleted) → parent"), "{out}");
        assert!(
            out.contains("docs PR000012 CLOSED (not on GitHub)"),
            "{out}"
        );
        assert!(!out.contains('#'), "{out}");
    }
    use crate::cli::pr_json::Selection;
    use clap::Parser;
    #[test]
    fn a_merge_with_override_stays_flagged_in_the_view() {
        let mut out = String::new();
        render_workspace_pr(
            &mut out,
            &serde_json::json!({
                "id": "pr", "status": "merged", "title": "T",
                "sourceWorkspaceName": "s", "targetWorkspaceName": "Base",
                "localMergeStatus": "merged", "providerStatus": "local_only",
                "mergeOverride": {"actorName": "Ada", "reason": "Approver away", "selfOverride": true,
                    "waivedApprovals": [{"definitionName": "Security review", "state": "pending"}],
                    "waivedChecks": [
                        {"repository": "acme/api", "availability": "ready",
                         "checks": [{"name": "ci", "bucket": "fail", "required": true}]},
                        {"repository": "acme/web", "availability": "unavailable", "checks": []}
                    ],
                    "githubRepositories": [{"repository": "acme/api", "mergeState": "blocked"}]}
            }),
        );
        assert!(
            out.contains("Merged with override by Ada (self-override): Approver away"),
            "{out}"
        );
        assert!(
            out.contains("waived approval: Security review (pending)"),
            "{out}"
        );
        assert!(out.contains("waived checks in acme/api: ci fail"), "{out}");
        assert!(
            out.contains("waived checks in acme/web: unavailable"),
            "{out}"
        );
        assert!(
            out.contains("GitHub rules bypassed in acme/api (blocked)"),
            "{out}"
        );
        let mut plain = String::new();
        render_workspace_pr(
            &mut plain,
            &serde_json::json!({"id": "pr", "status": "merged", "mergeOverride": null}),
        );
        assert!(!plain.contains("override"));
    }
    #[test]
    fn pr_arguments_and_query_encoding() {
        let cli = crate::cli::Cli::try_parse_from([
            "g5d",
            "pr",
            "view",
            "https://github.com/acme/r/pull/42",
            "-R",
            "acme/r",
            "-c",
            "--json",
        ])
        .unwrap();
        assert!(!cli.global.json);
        let crate::cli::Command::Pr(args) = cli.command else {
            panic!()
        };
        assert_eq!(args.json_fields, Some(None));
        let route = request_path(
            Uuid::nil(),
            true,
            &args.command,
            &crate::cli::pr_json::Selection::Human,
        )
        .unwrap();
        assert!(route.starts_with("/api/internal/workspaces/"));
        let url = reqwest::Url::parse(&format!("http://test{route}")).unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "selector").unwrap().1,
            "https://github.com/acme/r/pull/42"
        );
        assert!(crate::cli::Cli::try_parse_from(["g5d", "pr", "list", "--limit", "0"]).is_err());
        // `PR######` names a G5 PR, sent in its canonical form; GitHub-only
        // options refuse it as they refuse a G5 ID.
        let view = |selector: &str, repo: Option<&str>, comments: bool| {
            request_path(
                Uuid::nil(),
                false,
                &PrCommand::View(PrViewArgs {
                    pull_request: Some(selector.into()),
                    repo: repo.map(str::to_owned),
                    comments,
                }),
                &Selection::Human,
            )
        };
        assert!(view("pr3FA9C1", None, false)
            .unwrap()
            .ends_with("/pull-requests/info?selector=PR3fa9c1"));
        assert!(view("PR3fa9c1", Some("acme/api"), false).is_err());
        assert!(view("PR3fa9c1", None, true).is_err());
        assert!(view("7", None, true).unwrap().contains("selector=7&"));
    }
    #[test]
    fn repository_listing_uses_gh_flags_and_its_own_route() {
        let parse = |argv: &[&str]| {
            let cli = crate::cli::Cli::try_parse_from(argv).unwrap();
            let crate::cli::Command::Pr(args) = cli.command else {
                panic!()
            };
            args
        };
        let args = parse(&[
            "g5d",
            "pr",
            "list",
            "-R",
            "acme/api",
            "-H",
            "issue-816-landing",
            "-B",
            "main",
            "-A",
            "octo",
            "-l",
            "bug",
            "--label",
            "needs review,ui",
            "-S",
            "landing page",
            "-s",
            "merged",
            "-L",
            "50",
        ]);
        let PrCommand::List(list) = &args.command else {
            panic!()
        };
        assert!(list.repository_mode());
        assert_eq!(list.label, ["bug", "needs review", "ui"]);
        let route = request_path(Uuid::nil(), true, &args.command, &Selection::Human).unwrap();
        assert!(
            route.starts_with("/api/internal/workspaces/00000000-0000-0000-0000-000000000000/pull-requests/github?"),
            "{route}"
        );
        let url = reqwest::Url::parse(&format!("http://test{route}")).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        for (key, value) in [
            ("repo", "acme/api"),
            ("head", "issue-816-landing"),
            ("base", "main"),
            ("author", "octo"),
            ("label", "bug,needs review,ui"),
            ("search", "landing page"),
            ("state", "merged"),
            ("limit", "50"),
        ] {
            assert_eq!(query.get(key).map(String::as_str), Some(value), "{key}");
        }
        // Any one repository option switches mode; plain list stays on the
        // workspace's stored G5 PRs.
        let head_only = parse(&["g5d", "pr", "list", "--head", "b"]);
        let route =
            request_path(Uuid::nil(), false, &head_only.command, &Selection::Human).unwrap();
        assert!(
            route.starts_with("/api/v1/workspaces/") && route.contains("/pull-requests/github?")
        );
        let plain = parse(&["g5d", "pr", "list", "-s", "all", "-L", "500"]);
        let PrCommand::List(list) = &plain.command else {
            panic!()
        };
        assert!(!list.repository_mode());
        let route = request_path(Uuid::nil(), false, &plain.command, &Selection::Human).unwrap();
        assert!(route.ends_with("/pull-requests/info"), "{route}");
        // A repository listing is one GitHub page.
        let over = parse(&["g5d", "pr", "list", "-R", "acme/api", "-L", "101"]);
        assert!(request_path(Uuid::nil(), false, &over.command, &Selection::Human).is_err());
    }
    #[test]
    fn repository_listing_projects_rows_as_returned_and_renders_g5_links() {
        let args = crate::cli::PrListArgs {
            state: "all".into(),
            limit: 30,
            head: Some("issue-816-landing".into()),
            ..Default::default()
        };
        let command = PrCommand::List(args);
        let data = json!({"source":"github","repository":"acme/api","hasMore":true,"pullRequests":[
            {"source":"github","id":"PR_822","number":822,"title":"Landing","state":"MERGED","isDraft":false,
             "headRefName":"issue-816-landing","g5PullRequests":[{"id":"6e59","number":"PR000003","status":"merged",
             "sourceWorkspaceName":"issue-816","targetWorkspaceName":"Base"}]}]});
        let output = select_output(data, Uuid::nil(), &command).unwrap();
        assert_eq!(output["mode"], "repository");
        let selection = Selection::parse(
            Some(Some("number,state,headRefName,g5PullRequests")),
            &command,
        )
        .unwrap();
        let projected = selection.project(&output).unwrap();
        assert_eq!(projected[0]["state"], "MERGED");
        assert_eq!(projected[0]["g5PullRequests"][0]["id"], "6e59");
        let rendered = render(&output);
        assert!(
            rendered.starts_with("acme/api: GitHub PRs, newest first\n"),
            "{rendered}"
        );
        assert!(rendered.contains("#822\tMERGED\tLanding\tissue-816-landing\n"));
        assert!(rendered.contains("  PR000003 6e59 (merged): issue-816 → Base\n"));
        assert!(rendered.contains("More results available"));
        // Stored fallback rows keep their own shape; a local repository pull
        // request shows its G5 PR's name, one not yet on GitHub `-`.
        let stored = json!({"source":"g5","repository":"docs","fallback":"This repository is not on GitHub",
            "hasMore":false,"pullRequests":[{"source":"g5","id":"6e59","number":null,"title":"Docs",
            "state":"OPEN","headRefName":"issue-830","g5PullRequests":[]},
            {"source":"g5","tracker":"g5","id":"7f60","number":null,"g5Number":"PR000007","title":"Local",
            "state":"MERGED","headRefName":"issue-831","g5PullRequests":[]}]});
        let output = select_output(stored, Uuid::nil(), &command).unwrap();
        assert_eq!(
            Selection::parse(Some(Some("state,source")), &command)
                .unwrap()
                .project(&output)
                .unwrap(),
            json!([{"state":"OPEN","source":"g5"},{"state":"MERGED","source":"g5"}])
        );
        assert_eq!(
            Selection::parse(Some(Some("number,g5Number")), &command)
                .unwrap()
                .project(&output)
                .unwrap()[1],
            json!({"number":null,"g5Number":"PR000007"})
        );
        let rendered = render(&output);
        assert!(
            rendered.contains("docs: G5 stored PRs\n-\tOPEN\tDocs\tissue-830\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\nPR000007\tMERGED\tLocal\tissue-831\n"),
            "{rendered}"
        );
        // Fields that need one PR's full read point to `view`; `body` is
        // listed (`g5.git.pr-reads`).
        assert!(Selection::parse(Some(Some("id,body")), &command).is_ok());
        for field in ["comments", "mergeable", "baseRefOid"] {
            let error = Selection::parse(Some(Some(field)), &command).err().unwrap();
            assert!(
                error.to_string().contains("g5d pr view"),
                "{field}: {error}"
            );
        }
        assert!(matches!(
            Selection::parse(Some(None), &command).unwrap(),
            Selection::Available(fields) if fields.contains(&"g5PullRequests") && fields.contains(&"labels")
                && !fields.contains(&"localMergeStatus")
        ));
    }
    #[test]
    fn workspace_selection_prefers_open_outgoing_and_filters_lists() {
        let workspace = Uuid::new_v4();
        let old = Uuid::new_v4();
        let new = Uuid::new_v4();
        let data = json!({"source":"g5","pullRequests":[
            {"id":new,"sourceWorkspaceId":workspace,"status":"closed","createdAt":"2026-09-30"},
            {"id":old,"sourceWorkspaceId":workspace,"status":"open","createdAt":"2026-09-29"}
        ]});
        let view = PrCommand::View(PrViewArgs {
            pull_request: None,
            repo: None,
            comments: false,
        });
        assert_eq!(
            select_output(data.clone(), workspace, &view).unwrap()["pullRequest"]["id"],
            old.to_string()
        );
        let list = PrCommand::List(crate::cli::PrListArgs {
            state: "all".into(),
            limit: 1,
            ..Default::default()
        });
        let output = select_output(data.clone(), workspace, &list).unwrap();
        assert_eq!(output["hasMore"], true);
        assert_eq!(output["pullRequests"][0]["id"], new.to_string());
        assert_eq!(
            select_output(data, workspace, &PrCommand::Status).unwrap()["pullRequests"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(select_output(json!({"pullRequests":[]}), workspace, &view).is_err());
    }
    #[test]
    fn render_preserves_unknown_states_and_drops_terminal_controls() {
        let output = render(
            &json!({"source":"github","pullRequest":{"number":1,"title":"hello\u{1b}[2J","state":"OPEN","body":"a\nb","comments":[],"commentsHasMore":true}}),
        );
        assert!(!output.contains('\u{1b}'));
        assert!(output.contains("a\nb"));
        assert!(output.contains("first 100"));
        let output = render(
            &json!({"source":"github","pullRequest":{"number":1,"title":"t","state":"OPEN","commentCount":140,"comments":[{"body":"x"}]}}),
        );
        assert!(
            output.contains("Discussion comments (1 of 140)"),
            "{output}"
        );
        assert!(render(&json!({"source":"g5","pullRequests":[]})).contains("No matching"));
    }
    #[test]
    fn auto_push_off_is_reported_only_for_a_pr_with_github_records() {
        let view = |repos: Value| {
            render(&json!({"source":"g5","pullRequest":{
                "id":"p","status":"open","title":"t","sourceAutoPush":false,"providerRepos":repos}}))
        };
        assert!(view(json!([{"providerRepoFullName":"acme/api"}])).contains("Auto-push is off"));
        // `g5.git.pr-no-github`: nothing is forwarded where nothing is on GitHub.
        assert!(!view(json!([])).contains("Auto-push is off"));
    }
    #[test]
    fn render_reports_merge_freshness_and_conflicting_paths() {
        let live = |state: &str, mergeable: Value| {
            render(&json!({"source":"github","pullRequest":{
                "number":1,"title":"t","state":state,"mergeable":mergeable,"mergeStateStatus":"dirty"}}))
        };
        assert!(live("OPEN", json!(false)).contains("Mergeable: CONFLICTING · merge state: DIRTY"));
        assert!(live("OPEN", Value::Null).contains("UNKNOWN (GitHub is still computing it"));
        assert!(live("MERGED", Value::Null).contains("Mergeable: not applicable"));
        let repo = Uuid::new_v4();
        let stored = |stale: Value| {
            render(&json!({"source":"g5","pullRequests":[{
                "id":"p","status":"open","title":"t","localMergeStatus":"conflict",
                "providerRepos":[{"repoId":repo,"providerRepoFullName":"acme/api"}],
                "mergeEvidence":{"stale":stale,"repos":[
                    {"repoId":repo,"targetBranch":"main","targetMoved":stale,"conflictPaths":["src/a.rs","b.md"]}]}}]}))
        };
        let output = stored(json!(true));
        assert!(output.contains("Merge evidence is stale"), "{output}");
        assert!(output.contains("Conflicts in acme/api → main:\n    src/a.rs\n    b.md\n"));
        assert!(!stored(json!(false)).contains("stale"));
        assert!(stored(Value::Null).contains("freshness is unknown"));
    }
    #[test]
    fn render_groups_review_threads_by_location_and_resolution() {
        let output = render(&json!({"source":"github","pullRequest":{
            "number":1,"title":"t","state":"OPEN","comments":[],
            "reviewDecision":"CHANGES_REQUESTED","latestReviews":[{"author":{"login":"claude"},"state":"CHANGES_REQUESTED"}],
            "reviews":[{"author":{"login":"claude"},"state":"COMMENTED","submittedAt":"T0","body":"Walkthrough"}],
            "reviewThreads":[
                {"path":"src/a.rs","line":null,"originalLine":7,"originalStartLine":5,"isResolved":true,"isOutdated":true,
                 "resolvedBy":{"login":"dev"},"comments":[{"author":{"login":"bot"},"createdAt":"T1","body":"Old\u{1b}]8;;"}]},
                {"id":"PRRT_b","path":"src/b.rs","line":12,"startLine":null,"isResolved":false,"isOutdated":false,"commentsHasMore":true,
                 "comments":[{"author":{"login":"coderabbitai"},"createdAt":"T2","body":"Major: fix"}]},
                {"path":"docs/c.md","line":null,"originalLine":null,"subjectType":"FILE","isResolved":false,"isOutdated":false,"comments":[]}
            ],
            "reviewThreadsHasMore":true
        }}));
        assert!(output.contains("Reviews (1)\n\nclaude · COMMENTED · T0\nWalkthrough\n"));
        assert!(output.contains(
            "\nReview decision: CHANGES_REQUESTED\nLatest reviews: claude (CHANGES_REQUESTED)\n"
        ));
        assert!(output.contains("Review threads (3; 2 unresolved)"));
        assert!(output
            .contains("\nsrc/b.rs:12 · unresolved\nthread PRRT_b\ncoderabbitai · T2\nMajor: fix\nMore replies"));
        assert!(output.contains("\ndocs/c.md · unresolved\n"));
        assert!(output.contains("\nsrc/a.rs:5-7 (original) · resolved by dev · outdated\n"));
        assert!(output.find("src/b.rs").unwrap() < output.find("src/a.rs").unwrap());
        assert!(output.contains("More review threads are available"));
        assert!(!output.contains('\u{1b}'));
    }

    #[test]
    fn conversation_results_name_the_destination_and_outcome() {
        let dto = json!({"id":"p1","state":"published","destination":"review_thread","threadId":"PRRT_1",
            "displayKey":"acme/api#42","githubLogin":"owner","body":"Fixed","origin":"agent",
            "externalCommentUrl":"https://github.com/acme/api/pull/42#discussion_r9"});
        let result = comment_result(&dto, "k1");
        assert_eq!(
            (
                &result["repository"],
                &result["number"],
                &result["author"]["login"],
                &result["idempotencyKey"]
            ),
            (
                &json!("acme/api"),
                &json!(42),
                &json!("owner"),
                &json!("k1")
            )
        );
        assert_eq!(
            render_conversation(&result),
            "https://github.com/acme/api/pull/42#discussion_r9\n"
        );
        let failed = comment_result(
            &json!({"state":"failed","displayKey":"acme/api#42","error":{"message":"Resource not accessible"}}),
            "k2",
        );
        assert_eq!(
            render_conversation(&failed),
            "GitHub refused the comment on acme/api#42: Resource not accessible\n"
        );
        let unclear = comment_result(
            &json!({"state":"outcome_unknown","displayKey":"acme/api#42"}),
            "k3",
        );
        assert!(render_conversation(&unclear).contains("not confirmed"));
        let resolved = json!({"threadId":"PRRT_1","repository":"acme/api","number":42,"isResolved":true,"changed":false});
        assert_eq!(
            render_conversation(&resolved),
            "Review thread PRRT_1 on acme/api#42 was already resolved\n"
        );
    }

    #[test]
    fn watch_backs_off_while_unchanged_and_resets_on_change() {
        let interval = Duration::from_secs(15);
        let same = json!([{"repository":"acme/api","checks":[{"bucket":"pending"}]}]);
        let other = json!([{"repository":"acme/api","checks":[{"bucket":"pass"}]}]);
        assert_eq!(backoff(None, &same, interval), interval);
        let mut delay = interval;
        let mut seen = Vec::new();
        for _ in 0..6 {
            delay = backoff(Some(&(same.clone(), delay)), &same, interval);
            seen.push(delay.as_secs());
        }
        assert_eq!(seen, [30, 60, 120, 120, 120, 120]);
        assert_eq!(
            backoff(Some(&(same.clone(), delay)), &other, interval),
            interval
        );
        // An interval above the cap is never shortened.
        let long = Duration::from_secs(600);
        assert_eq!(backoff(Some(&(same.clone(), long)), &same, long), long);
    }

    #[test]
    fn watch_never_sleeps_past_its_deadline() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(10);
        let wait = Duration::from_secs(30);
        assert_eq!(
            until_deadline(Duration::from_secs(5), now, deadline),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            until_deadline(wait, now, deadline),
            Some(Duration::from_secs(10))
        );
        assert_eq!(until_deadline(wait, deadline, deadline), None);
        assert_eq!(
            until_deadline(wait, deadline + Duration::from_secs(1), deadline),
            None
        );
    }

    #[test]
    fn watch_waits_out_rate_limits_only_when_every_unavailable_repo_is_limited() {
        let now: Reset = "2026-10-31T12:00:00Z".parse().unwrap();
        let limited = |retry: Value| json!({"repository":"acme/api","availability":"unavailable","rateLimited":true,"retryAt":retry,"checks":[]});
        let ready = json!({"repository":"acme/web","availability":"ready","checks":[]});
        let observation = |repos: Vec<Value>| json!({ "repositories": repos });
        let at = |s: &str| s.parse::<Reset>().unwrap();
        assert_eq!(
            rate_limit_wait(
                &observation(vec![
                    limited(json!("2026-10-31T12:00:30Z")),
                    limited(json!("2026-10-31T12:05:00Z")),
                    ready.clone()
                ]),
                now
            ),
            Some((Duration::from_secs(302), Some(at("2026-10-31T12:05:00Z"))))
        );
        for retry in [
            Value::Null,
            json!("2026-10-31T11:59:00Z"),
            json!("not a time"),
        ] {
            assert_eq!(
                rate_limit_wait(&observation(vec![limited(retry)]), now),
                Some((RATE_LIMIT_FALLBACK, None))
            );
        }
        let refused = json!({"repository":"acme/web","availability":"unavailable","message":"Grant Checks read access","checks":[]});
        assert_eq!(
            rate_limit_wait(
                &observation(vec![limited(json!("2026-10-31T12:01:00Z")), refused]),
                now
            ),
            None
        );
        assert_eq!(rate_limit_wait(&observation(vec![ready]), now), None);
    }

    #[test]
    fn watch_stops_once_every_read_pr_is_completed() {
        let repo = |name: &str, state: Option<&str>| {
            let mut repo = json!({"repository":name,"number":42});
            if let Some(state) = state {
                repo["prState"] = json!(state);
            }
            repo
        };
        let observation = |repos: Vec<Value>| json!({ "repositories": repos });
        assert_eq!(
            completed_prs(&observation(vec![
                repo("acme/api", Some("merged")),
                repo("acme/web", Some("closed")),
                repo("acme/docs", None)
            ])),
            Some(vec![
                "GitHub PR acme/api#42 is merged; stopped watching.".to_owned(),
                "GitHub PR acme/web#42 is closed; stopped watching.".to_owned()
            ])
        );
        assert_eq!(
            completed_prs(&observation(vec![
                repo("acme/api", Some("merged")),
                repo("acme/web", Some("open"))
            ])),
            None
        );
        assert_eq!(
            completed_prs(&observation(vec![repo("acme/api", None)])),
            None
        );
    }

    #[test]
    fn watch_timeout_requires_watch_and_interval_has_a_floor() {
        let parse = |args: &[&str]| {
            let mut all = vec!["g5d", "pr", "checks"];
            all.extend_from_slice(args);
            crate::cli::Cli::try_parse_from(all)
        };
        let crate::cli::Command::Pr(args) = parse(&["--watch"]).unwrap().command else {
            panic!()
        };
        let PrCommand::Checks(checks) = args.command else {
            panic!()
        };
        assert_eq!((checks.interval, checks.timeout), (15, 30));
        assert!(parse(&[]).is_ok());
        assert!(parse(&["--timeout", "5"]).is_err());
        assert!(parse(&["--watch", "--timeout", "0"]).is_err());
        assert!(parse(&["--watch", "--timeout", "1441"]).is_err());
        assert!(parse(&["--watch", "--interval", "4"]).is_err());
        assert!(parse(&["--watch", "--interval", "5", "--timeout", "1"]).is_ok());
    }

    /// g5.git.provider-read-budget: when each read of a default `--watch`
    /// happens, in seconds, over its whole 30-minute timeout. Each is one
    /// checks read, whose GitHub requests g5-api's `github_request_budget`
    /// test pins; together they bound one watcher's GitHub requests.
    #[tokio::test(start_paused = true)]
    async fn watch_read_budget_over_a_full_timeout() {
        async fn watch(observe: impl Fn(usize) -> Value) -> (i32, Vec<u64>) {
            let cli =
                crate::cli::Cli::try_parse_from(["g5d", "-q", "pr", "checks", "--watch"]).unwrap();
            let crate::cli::Command::Pr(pr) = &cli.command else {
                panic!()
            };
            let PrCommand::Checks(checks) = &pr.command else {
                panic!()
            };
            let selection = crate::cli::pr_json::Selection::parse(None, &pr.command).unwrap();
            let start = Instant::now();
            let mut reads = Vec::new();
            let status = watch_checks(checks, &cli.global, &selection, || {
                reads.push(start.elapsed().as_secs());
                let data = observe(reads.len() - 1);
                async move { Ok(data) }
            })
            .await
            .unwrap();
            (status, reads)
        }
        let observation = |bucket: &str| {
            json!({"repositories":[{"repository":"acme/api","number":42,"availability":"ready",
                "checks":[{"name":"tests","bucket":bucket}]}]})
        };
        let every = |step: u64| (0..=1800).step_by(step as usize).collect::<Vec<u64>>();

        // Unchanged pending checks: 15 s doubling to two minutes, then a final
        // read at the deadline; 19 reads in 30 minutes.
        let (status, reads) = watch(|_| observation("pending")).await;
        let mut expected = vec![0, 15, 45, 105];
        expected.extend((225..1800).step_by(120));
        expected.push(1800);
        assert_eq!((status, reads), (8, expected));

        // Checks that change on every read keep the 15 s interval: the most a
        // watch reads.
        let (status, reads) = watch(|i| {
            let mut data = observation("pending");
            data["repositories"][0]["checks"][0]["name"] = json!(format!("step-{i}"));
            data
        })
        .await;
        assert_eq!((status, reads), (8, every(15)));

        // A rate limit with no future reset waits a minute between reads.
        let (status, reads) = watch(|_| {
            let mut data = observation("pending");
            data["repositories"][0]["availability"] = json!("unavailable");
            data["repositories"][0]["rateLimited"] = json!(true);
            data
        })
        .await;
        assert_eq!((status, reads), (8, every(60)));

        // A merged PR ends the watch on its first read; a settled result on
        // the read after it changes.
        let (status, reads) = watch(|_| {
            let mut data = observation("pending");
            data["repositories"][0]["prState"] = json!("merged");
            data
        })
        .await;
        assert_eq!((status, reads), (8, vec![0]));
        let (status, reads) = watch(|i| observation(if i == 0 { "pending" } else { "pass" })).await;
        assert_eq!((status, reads), (0, vec![0, 15]));
    }
}

fn checks_path(
    workspace: Uuid,
    internal: bool,
    args: &crate::cli::PrChecksArgs,
    json_details: bool,
) -> Result<String, G5Error> {
    let prefix = if internal { "internal" } else { "v1" };
    let path = format!("/api/{prefix}/workspaces/{workspace}/pull-requests/info");
    let mut url = reqwest::Url::parse(&format!("http://placeholder{path}")).unwrap();
    let mut query = url.query_pairs_mut();
    query.append_pair("checks", "true");
    if let Some(selector) = &args.pull_request {
        query.append_pair("selector", &write::server_selector(selector));
    }
    if let Some(repo) = &args.repo {
        query.append_pair("repo", repo);
    }
    if let Some(head) = &args.head {
        query.append_pair("head", head);
    }
    if args.details || json_details {
        query.append_pair("details", "true");
    }
    drop(query);
    Ok(format!("{path}?{}", url.query().unwrap()))
}

#[derive(Default)]
struct CheckOutcome {
    pending: bool,
    failed: bool,
    unavailable: bool,
}

fn check_outcome(data: &Value) -> Result<CheckOutcome, G5Error> {
    // A PR that expects no GitHub has nothing to wait for (`g5.git.pr-no-github`).
    if data["noExternalChecks"] == true {
        return Ok(CheckOutcome::default());
    }
    let repos = data["repositories"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| invalid("PR checks API returned no repository observations"))?;
    let mut result = CheckOutcome::default();
    for repo in repos {
        let checks = repo["checks"]
            .as_array()
            .ok_or_else(|| invalid("PR checks API omitted check enumeration"))?;
        // A local repository pull request has nothing to wait for.
        if repo["noExternalChecks"] == true {
            continue;
        }
        match repo["availability"].as_str() {
            Some("unavailable") => result.unavailable = true,
            Some("pending") => result.pending = true,
            Some("ready") => {}
            _ => return Err(invalid("PR checks API returned invalid availability")),
        }
        if checks.is_empty() {
            result.pending = true;
        }
        for check in checks {
            match check["bucket"].as_str() {
                Some("fail" | "cancel") => result.failed = true,
                Some("pending") => result.pending = true,
                Some("pass" | "skipping") => {}
                _ => return Err(invalid("PR checks API returned an unknown check bucket")),
            }
        }
    }
    Ok(result)
}

/// One checks read, or with `--watch` reads paced by `g5.git.check-waits`
/// until a settled result, completed PRs or the timeout; returns the exit
/// status. `read` makes one checks read.
async fn watch_checks<F, Fut>(
    checks: &PrChecksArgs,
    global: &GlobalFlags,
    selection: &crate::cli::pr_json::Selection,
    mut read: F,
) -> Result<i32, G5Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Value, G5Error>>,
{
    let cancel = tokio::signal::ctrl_c();
    tokio::pin!(cancel);
    let interval = Duration::from_secs(checks.interval);
    let deadline = Instant::now() + Duration::from_secs(checks.timeout * 60);
    let mut pace: Option<(Value, Duration)> = None;
    loop {
        let mut data = tokio::select! {
            result = read() => result?,
            _ = &mut cancel => return Ok(130),
        };
        if checks.required {
            // Only what the base branch requires decides the outcome.
            for repo in data["repositories"].as_array_mut().into_iter().flatten() {
                if let Some(rows) = repo["checks"].as_array_mut() {
                    rows.retain(|row| row["required"] == true);
                }
            }
        }
        let observed = check_outcome(&data)?;
        let mut timed_out = false;
        if checks.watch && !(checks.fail_fast && observed.failed) {
            let wait = if observed.unavailable {
                rate_limit_wait(&data, chrono::Utc::now())
            } else if observed.pending {
                let delay = backoff(pace.as_ref(), &data["repositories"], interval);
                pace = Some((data["repositories"].clone(), delay));
                Some((delay, None))
            } else {
                None
            };
            if let Some((wait, reset)) = wait {
                if let Some(completed) = completed_prs(&data) {
                    for line in completed {
                        eprintln!("{line}");
                    }
                } else if let Some(sleep) = until_deadline(wait, Instant::now(), deadline) {
                    if !global.quiet {
                        eprintln!("{}", render_checks(&data));
                    }
                    if observed.unavailable {
                        pace = None;
                        eprintln!(
                            "GitHub rate-limited the checks read; waiting until {} to read again.",
                            reset.map_or_else(
                                || "about a minute from now".to_owned(),
                                |at| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                            )
                        );
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(sleep) => {},
                        _ = &mut cancel => return Ok(130),
                    }
                    continue;
                } else {
                    eprintln!(
                        "Stopped watching after {} minute{} with checks still pending.",
                        checks.timeout,
                        if checks.timeout == 1 { "" } else { "s" }
                    );
                    timed_out = true;
                }
            }
        }
        let rows: Vec<_> = data["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["checks"].as_array().unwrap().iter().cloned())
            .collect();
        if let Some(output) = selection.project(&json!({"checks": rows})) {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).map_err(|e| invalid(&e.to_string()))?
            );
            // Selection can omit availability entirely; keep recovery messages on stderr.
            if data["noExternalChecks"] == true && !global.quiet {
                eprintln!("{}", text(&data, "message"));
            }
            for repo in data["repositories"].as_array().unwrap() {
                if let Some(message) = repo["message"].as_str() {
                    eprintln!("{message}");
                }
            }
        } else if !global.quiet {
            print!("{}", render_checks(&data));
        }
        // A timeout reports pending, even while waiting out a rate limit,
        // unless a check has already failed.
        return Ok(if observed.failed || (observed.unavailable && !timed_out) {
            1
        } else if observed.pending || timed_out {
            8
        } else {
            0
        });
    }
}

/// The longest a watch backs off between polls, unless `--interval` is longer.
const WATCH_BACKOFF_CAP: Duration = Duration::from_secs(120);
/// How long a watch waits on a rate limit GitHub gave no future reset for.
const RATE_LIMIT_FALLBACK: Duration = Duration::from_secs(60);
/// Margin after GitHub's reported reset, so the next read is not early.
const RATE_LIMIT_MARGIN: Duration = Duration::from_secs(2);

/// The next poll delay: `interval` after a changed (or first) observation,
/// doubling while it repeats, capped at two minutes or `interval` if larger.
fn backoff(
    previous: Option<&(Value, Duration)>,
    repositories: &Value,
    interval: Duration,
) -> Duration {
    match previous {
        Some((seen, delay)) if seen == repositories => {
            delay.saturating_mul(2).min(WATCH_BACKOFF_CAP.max(interval))
        }
        _ => interval,
    }
}

/// How long to sleep before the next poll, never past the deadline; `None`
/// once the deadline has passed.
fn until_deadline(wait: Duration, now: Instant, deadline: Instant) -> Option<Duration> {
    let left = deadline
        .checked_duration_since(now)
        .filter(|d| !d.is_zero())?;
    Some(wait.min(left))
}

/// A rate-limit reset GitHub reported for a check read.
type Reset = chrono::DateTime<chrono::Utc>;

/// The wait for an unavailable observation whose every unavailable repository
/// was rate-limited: until the latest reported reset (plus a margin), or a
/// minute when none is in the future. `None` for any other unavailability.
fn rate_limit_wait(data: &Value, now: Reset) -> Option<(Duration, Option<Reset>)> {
    let unavailable: Vec<_> = data["repositories"]
        .as_array()?
        .iter()
        .filter(|repo| repo["availability"] == "unavailable")
        .collect();
    if unavailable.is_empty() || unavailable.iter().any(|repo| repo["rateLimited"] != true) {
        return None;
    }
    let reset = unavailable
        .iter()
        .filter_map(|repo| repo["retryAt"].as_str())
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&chrono::Utc))
        .max()
        .filter(|at| *at > now);
    Some(match reset {
        Some(at) => (
            (at - now).to_std().unwrap_or_default() + RATE_LIMIT_MARGIN,
            Some(at),
        ),
        None => (RATE_LIMIT_FALLBACK, None),
    })
}

/// One stop line per GitHub PR when every repository that read its PR found
/// it merged or closed (at least one did); `None` while any is open.
fn completed_prs(data: &Value) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    for repo in data["repositories"].as_array()? {
        match repo["prState"].as_str() {
            None => {}
            Some(state @ ("merged" | "closed")) => lines.push(format!(
                "GitHub PR {}#{} is {state}; stopped watching.",
                repo["repository"].as_str().unwrap_or("unknown"),
                repo["number"]
            )),
            Some(_) => return None,
        }
    }
    (!lines.is_empty()).then_some(lines)
}

fn render_checks(data: &Value) -> String {
    if data["noExternalChecks"] == true {
        return format!("{}\n", text(data, "message"));
    }
    let mut text = String::from("Observed GitHub checks (not merge approval):\n");
    for repo in data["repositories"].as_array().into_iter().flatten() {
        if repo["noExternalChecks"] == true {
            let name = g5_name(&repo["g5Number"])
                .or_else(|| g5_name(&repo["number"]))
                .unwrap_or_else(|| "G5 PR".to_owned());
            text.push_str(&format!(
                "{} ({name})\n  {}\n",
                repo["repository"].as_str().unwrap_or("repository"),
                repo["message"]
                    .as_str()
                    .unwrap_or("No external checks apply.")
            ));
            continue;
        }
        text.push_str(&format!(
            "{} #{} @ {}\n",
            repo["repository"].as_str().unwrap_or("unpublished member"),
            repo["number"],
            repo["headRefOid"].as_str().unwrap_or("unavailable")
        ));
        if let Some(message) = repo["message"].as_str() {
            text.push_str(&format!("  {message}\n"));
        }
        if let (Some(branch), Some([])) = (
            repo["baseRefName"].as_str(),
            repo["requiredChecks"].as_array().map(Vec::as_slice),
        ) {
            text.push_str(&format!("  {branch} requires no checks.\n"));
        }
        let rows = repo["checks"].as_array().unwrap();
        if rows.is_empty() {
            text.push_str("  No checks reported for this revision yet.\n");
        }
        for check in rows {
            let required = if check["kind"] == "expected" {
                " (required, not reported yet)"
            } else if check["required"] == true {
                " (required)"
            } else {
                ""
            };
            text.push_str(&format!(
                "  {}  {}{required}  {}  {} @ {}\n",
                check["bucket"].as_str().unwrap_or("unknown"),
                check["name"].as_str().unwrap_or("unknown"),
                check["link"].as_str().unwrap_or(""),
                check["revision"].as_str().unwrap_or("head"),
                check["commitOid"].as_str().unwrap_or("unknown")
            ));
            if let Some(details) = check.get("details") {
                text.push_str(&format!(
                    "{}\n",
                    serde_json::to_string_pretty(details).unwrap()
                ));
            }
        }
    }
    // Provider-controlled content is data, never terminal escape sequences.
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
