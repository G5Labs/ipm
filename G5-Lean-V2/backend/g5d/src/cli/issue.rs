//! `g5d issue` — the workspace's external issues (read, create, link and
//! comment, as the workspace owner) and platform bug reporting, for the
//! agent inside a G5 workspace.

use std::io::Read;
use std::path::PathBuf;

use g5_contracts::error::G5Error;
use g5_internal_api::client::InternalApiClient;
use g5_internal_api::dtos::{InternalPlatformIssueContext, InternalPlatformIssueReportRequest};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::Value;

use super::{
    issue_attach, issue_attachments, issue_json, issue_label, issue_links, stable_context,
    GlobalFlags, IssueArgs, IssueCommand, IssueCreateArgs, IssueEditArgs, IssueLinkArgs,
    IssueReportArgs, IssueSearchArgs, IssueShowArgs, IssueStateArg,
};

/// Exit status of a creation whose outcome GitHub did not make clear, as for
/// `g5d pr comment`.
pub const OUTCOME_UNCLEAR: i32 = 7;

/// Run `g5d issue`, returning the process exit status.
pub async fn execute(args: &IssueArgs, global: &GlobalFlags) -> Result<i32, G5Error> {
    // `report` files a *platform* bug (`platform_issue_reporting` capability,
    // enforced server-side and failing closed with 503).
    let workspace_id = match stable_context::discover(global)? {
        Some(stable) => stable.metadata.workspace_id,
        None => incarnation_workspace_id(|key| std::env::var(key).ok()).ok_or_else(|| {
            G5Error::Config(
                "`g5d issue` runs inside a G5 workspace: run it from the agent's workspace so \
                 worker auth and workspace self-context can be validated."
                    .to_string(),
            )
        })?,
    };
    let client = stable_context::require_worker_client(global)?;
    let http = reqwest::Client::new();
    let base_url = client.base_url().trim_end_matches('/').to_string();

    let fields = args.json_fields.as_ref().map(|fields| fields.as_deref());
    let json = global.json || fields.is_some();
    // Only `view` and `search` select gh fields; elsewhere --json alone is
    // the full record. `g5d issue view --json 12` keeps 12 as the issue.
    let selected = match (&args.command, fields) {
        (IssueCommand::Show(show), Some(Some(value)))
            if show.issue.is_none() && !issue_json::is_field_list(value, issue_json::VIEW_FIELDS) =>
        {
            return view_issue(&client, &workspace_id, json, None, Some(value), show).await;
        }
        (IssueCommand::Show(_) | IssueCommand::Search(_), Some(Some(value))) => Some(value),
        (_, Some(Some(value))) => {
            return validation_error(
                "json",
                format!(
                    "only `g5d issue view` and `search` select JSON fields ({value}); use --json alone for the full record"
                ),
            )
        }
        _ => None,
    };

    match &args.command {
        IssueCommand::Report(report_args) => {
            let response =
                report_platform_issue(&http, &base_url, &workspace_id, &client, report_args)
                    .await?;
            print_issue_output(json, &response);
        }
        IssueCommand::List => {
            let url = format!("{base_url}/api/internal/workspaces/{workspace_id}/issue-links");
            let response = send_json_response(&client, http.get(url)).await?;
            print_rendered(json, &response, issue_links::render_list);
        }
        IssueCommand::Show(show_args) => {
            let fields = issue_json::parse(selected, issue_json::VIEW_FIELDS)?;
            return view_issue(
                &client,
                &workspace_id,
                json,
                fields,
                show_args.issue.as_deref(),
                show_args,
            )
            .await;
        }
        IssueCommand::Search(search_args) => {
            let fields = issue_json::parse(selected, issue_json::SEARCH_FIELDS)?;
            let response = client
                .search_issues(&workspace_id, &search_query(search_args)?)
                .await
                .inspect_err(report_search_retry)?;
            match fields {
                Some(fields) => print_json(&issue_json::project_search(&response["data"], &fields)),
                None if json => print_json(&response),
                None => {
                    print!("{}", issue_links::render_search(&response["data"]));
                    eprint!("{}", issue_links::render_search_notes(&response["data"]));
                }
            }
        }
        IssueCommand::Attachments(attachments_args) => {
            let reference = view_reference(
                attachments_args.issue.as_deref(),
                attachments_args.repo.as_deref(),
            )?;
            return issue_attachments::save_issue_attachments(
                &client,
                &workspace_id,
                json,
                reference,
                attachments_args,
            )
            .await;
        }
        IssueCommand::Unlink(unlink_args) => {
            let response = client
                .unlink_issue(
                    &workspace_id,
                    &serde_json::json!({"ref": unlink_args.issue.trim()}),
                )
                .await?;
            print_rendered(json, &response, issue_links::render_unlinked);
        }
        IssueCommand::Close(state_args) | IssueCommand::Reopen(state_args) => {
            let requested = if matches!(args.command, IssueCommand::Close(_)) {
                "closed"
            } else {
                "open"
            };
            let mut body = serde_json::json!({"state": requested});
            if let Some(issue) = state_args.issue.as_deref().map(str::trim) {
                if issue.is_empty() {
                    return validation_error("issue", "must not be empty");
                }
            }
            if let Some(reference) =
                view_reference(state_args.issue.as_deref(), state_args.repo.as_deref())?
            {
                body["ref"] = serde_json::json!(reference);
            }
            // The transition is idempotent (`g5.external-issues.transition`),
            // so the client's retries are safe.
            let response = client
                .workspace_agent_post(&workspace_id, "issue-state", &body, true)
                .await?;
            let applied = response["data"]["outcome"].as_str() == Some("applied");
            if json {
                print_json(&response);
            } else {
                let data = &response["data"];
                let key = data["issue"]["displayKey"]
                    .as_str()
                    .or_else(|| data["issue"]["key"].as_str())
                    .unwrap_or("the issue");
                match data["outcome"].as_str() {
                    Some("applied") => println!(
                        "{} {key} as {}.",
                        if requested == "closed" {
                            "Closed"
                        } else {
                            "Reopened"
                        },
                        data["githubLogin"].as_str().unwrap_or("the owner"),
                    ),
                    _ => println!(
                        "GitHub did not confirm the change: {}",
                        data["message"].as_str().unwrap_or("unknown outcome")
                    ),
                }
            }
            // An unconfirmed transition is safe to send again.
            if !applied {
                return Ok(OUTCOME_UNCLEAR);
            }
        }
        IssueCommand::Edit(edit_args) => {
            return edit_issue(&client, &workspace_id, json, edit_args).await;
        }
        IssueCommand::Label(label_args) => {
            return issue_label::execute(&client, &workspace_id, json, label_args).await;
        }
        IssueCommand::Comment(comment_args) => {
            let mut body = optional_text(
                comment_args.body.as_deref(),
                comment_args.body_file.as_ref(),
            )?
            .unwrap_or_default();
            let images = issue_attach::read_images(&comment_args.attach)?;
            if body.trim().is_empty() && images.is_empty() {
                return validation_error("body", "must not be empty");
            }
            let mut reference =
                view_reference(comment_args.issue.as_deref(), comment_args.repo.as_deref())?;
            // The images first, so the comment never shows one that is not
            // there; the comment then names the issue they went to.
            if !images.is_empty() {
                let uploaded =
                    issue_attach::upload(&client, &workspace_id, reference.as_deref(), &images)
                        .await?;
                body = issue_attach::with_images(&body, &uploaded.markdown);
                reference = Some(uploaded.issue_url);
            }
            let idempotency_key = comment_args
                .idempotency_key
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let url = format!("{base_url}/api/internal/workspaces/{workspace_id}/issue-comments");
            let mut request = serde_json::json!({"body": body, "idempotencyKey": idempotency_key});
            if let Some(reference) = reference {
                request["ref"] = serde_json::json!(reference);
            }
            let response = match send_json_response(&client, http.post(url).json(&request)).await {
                Ok(response) => response,
                Err(error) => {
                    // The comment may still have been posted: a retry with
                    // this key cannot post it twice.
                    eprintln!(
                        "To retry this comment safely, pass --idempotency-key {idempotency_key}"
                    );
                    return Err(error);
                }
            };
            print_rendered(json, &response, issue_links::render_comment);
            if response["data"]["state"] == "outcome_unknown" {
                // Posting again with this key looks for the comment first.
                eprintln!(
                    "To check for this comment, rerun with --idempotency-key {idempotency_key}"
                );
                return Ok(OUTCOME_UNCLEAR);
            }
        }
        IssueCommand::Create(create_args) => {
            return create_issue(&client, &workspace_id, json, create_args).await;
        }
        IssueCommand::Link(link_args) => {
            let response = client
                .link_issue(&workspace_id, &link_request(link_args))
                .await?;
            print_rendered(json, &response, issue_links::render_linked);
        }
    }
    Ok(0)
}

/// The issue `g5d issue view` and `comment` ask for: `-R owner/name` turns a
/// number into `owner/name#N`, `-R ACME` (a G5 issue key) into `ACME-N`.
fn view_reference(issue: Option<&str>, repo: Option<&str>) -> Result<Option<String>, G5Error> {
    let issue = issue.map(str::trim).filter(|issue| !issue.is_empty());
    let Some(repo) = repo.map(str::trim) else {
        return Ok(issue.map(str::to_owned));
    };
    let number = issue
        .map(|issue| issue.strip_prefix('#').unwrap_or(issue))
        .filter(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    match number {
        Some(number) if repo.contains('/') => Ok(Some(format!("{repo}#{number}"))),
        Some(number) if is_issue_key(repo) => Ok(Some(format!("{repo}-{number}"))),
        Some(_) => validation_error(
            "repo",
            "name the repository as owner/name, or a G5 issue key such as ACME",
        ),
        None => validation_error(
            "repo",
            "-R names the repository of an issue number; give the number too, or a full reference without -R",
        ),
    }
}

/// A G5 issue key (`ACME`): uppercase letters and digits, starting with a
/// letter. A lowercase name is more likely a repository missing its owner.
fn is_issue_key(value: &str) -> bool {
    (2..=10).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_uppercase())
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// `g5d issue view`: the issue as text with its context note, G5's full
/// record with `--json`, or gh's fields with `--json FIELDS`.
async fn view_issue(
    client: &InternalApiClient,
    workspace_id: &str,
    json: bool,
    fields: Option<Vec<String>>,
    issue: Option<&str>,
    args: &IssueShowArgs,
) -> Result<i32, G5Error> {
    let reference = view_reference(issue, args.repo.as_deref())?;
    let comments = args.comments
        || fields
            .as_ref()
            .is_some_and(|fields| fields.iter().any(|field| field == "comments"));
    let response = client
        .view_issue(workspace_id, reference.as_deref(), comments)
        .await?;
    match fields {
        Some(fields) => print_json(&issue_json::project_view(&response["data"], &fields)),
        None if json => print_json(&response),
        None => print!(
            "{}",
            issue_links::render_issue(&response["data"], args.comments)
        ),
    }
    Ok(0)
}

/// Qualifiers G5 search sets itself, with what to use instead. The server
/// refuses them too; refusing here says which option replaces each.
const RESERVED_QUALIFIERS: &[(&str, &str)] = &[
    ("repo:", "use -R owner/name"),
    (
        "org:",
        "G5 searches the organization's issue repositories already",
    ),
    (
        "user:",
        "G5 searches the organization's issue repositories already",
    ),
    (
        "owner:",
        "G5 searches the organization's issue repositories already",
    ),
    (
        "is:",
        "use --state for open or closed; G5 searches issues only",
    ),
    ("type:", "G5 searches issues only"),
    ("state:", "use --state"),
];

/// The search route's query, or the refusal of a filter G5 cannot apply.
fn search_query(args: &IssueSearchArgs) -> Result<Vec<(&'static str, String)>, G5Error> {
    let text = args.query.join(" ");
    for term in text.split_whitespace() {
        let bare = term
            .trim_start_matches(['-', '"', '('])
            .to_ascii_lowercase();
        if let Some((qualifier, instead)) = RESERVED_QUALIFIERS
            .iter()
            .find(|(qualifier, _)| bare.starts_with(qualifier))
        {
            return validation_error(
                "query",
                format!("G5 search sets {qualifier} itself and cannot take {term}: {instead}"),
            );
        }
    }
    let limit = args.limit.unwrap_or(SEARCH_MAX_LIMIT);
    if !(1..=SEARCH_MAX_LIMIT).contains(&limit) {
        return validation_error(
            "limit",
            format!("G5 search returns at most {SEARCH_MAX_LIMIT} issues: use -L 1-{SEARCH_MAX_LIMIT} and narrow the query"),
        );
    }
    let mut query = vec![("limit", limit.to_string())];
    if !text.trim().is_empty() {
        query.push(("q", text));
    }
    query.push((
        "state",
        match args.state {
            Some(IssueStateArg::Open) => "open",
            Some(IssueStateArg::Closed) => "closed",
            None => "all",
        }
        .to_string(),
    ));
    match args.assignee.as_deref().map(str::trim) {
        None => {}
        Some("@me") => query.push(("assignee", "me".to_string())),
        Some(other) => {
            return validation_error(
                "assignee",
                format!("G5 search filters only issues assigned to @me (the workspace owner) or unassigned (--no-assignee), not {other}; try the query `assignee:{}`", other.trim_start_matches('@')),
            )
        }
    }
    if args.no_assignee {
        query.push(("assignee", "none".to_string()));
    }
    // gh's --label and --no-label are query qualifiers; quoting keeps a
    // label's exact name, spaces and commas included.
    let mut qualifiers: Vec<String> = Vec::new();
    for label in names(&args.labels) {
        if label.contains('"') {
            return validation_error(
                "label",
                format!("label names cannot contain a quote: {label}"),
            );
        }
        qualifiers.push(format!("label:\"{label}\""));
    }
    if args.no_label {
        qualifiers.push("no:label".to_string());
    }
    if !qualifiers.is_empty() {
        let joined = qualifiers.join(" ");
        match query.iter_mut().find(|(key, _)| *key == "q") {
            Some((_, text)) => {
                text.push(' ');
                text.push_str(&joined);
            }
            None => query.push(("q", joined)),
        }
    }
    if let Some(repo) = args.repo.as_deref().map(str::trim) {
        if !repo.contains('/') && !is_issue_key(repo) {
            return validation_error(
                "repo",
                "name the repository as owner/name, or a G5 issue key such as ACME",
            );
        }
        query.push(("repo", repo.to_string()));
    }
    Ok(query)
}

/// Say on stderr when a refused search may be retried: the exit status (3)
/// and G5's error line do not carry the wait.
fn report_search_retry(error: &G5Error) {
    if let G5Error::RateLimited { retry_after_secs } = error {
        match retry_after_secs {
            Some(secs) => eprintln!(
                "G5 refused the search to keep within the agents' search budget; retry in {secs} seconds."
            ),
            None => eprintln!(
                "G5 refused the search to keep within the agents' search budget; retry in a minute."
            ),
        }
    }
}

/// Most issues one search returns: G5 merges one page per repository.
const SEARCH_MAX_LIMIT: usize = 30;

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// The create request, or the option `gh` would have prompted for.
fn create_request(args: &IssueCreateArgs, key: &str) -> Result<Value, G5Error> {
    let title = args.title.as_deref().map(str::trim).unwrap_or_default();
    let body = optional_text(args.body.as_deref(), args.body_file.as_ref())?;
    let (title, body) = match (title.is_empty(), body) {
        (false, Some(body)) => (title, body),
        _ => {
            return validation_error(
                if title.is_empty() { "title" } else { "body" },
                "g5d issue create never prompts: supply --title and --body (or --body-file)",
            )
        }
    };
    let mut request = serde_json::json!({
        "title": title,
        "body": body,
        "assignees": args.assignees.iter().map(|a| a.trim()).filter(|a| !a.is_empty()).collect::<Vec<_>>(),
        "idempotencyKey": key,
    });
    let labels = names(&args.labels);
    if !labels.is_empty() {
        request["labels"] = labels.into();
    }
    // A blank `--parent` (say, an unset variable) must not quietly create a
    // top-level issue.
    if let Some(parent) = args.parent.as_deref().map(str::trim) {
        if parent.is_empty() {
            return validation_error("parent", "must not be empty");
        }
        request["parent"] = parent.into();
    }
    Ok(request)
}

/// Names as given, trimmed, blanks dropped.
fn names(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The edit request, or the refusal of an edit that names no change.
fn edit_request(args: &IssueEditArgs) -> Result<Value, G5Error> {
    let mut request = serde_json::json!({
        "addAssignees": names(&args.add_assignees),
        "removeAssignees": names(&args.remove_assignees),
    });
    let (add_labels, remove_labels) = (names(&args.add_labels), names(&args.remove_labels));
    if !add_labels.is_empty() {
        request["addLabels"] = add_labels.into();
    }
    if !remove_labels.is_empty() {
        request["removeLabels"] = remove_labels.into();
    }
    if let Some(title) = args.title.as_deref() {
        if title.trim().is_empty() {
            return validation_error("title", "must not be empty");
        }
        request["title"] = title.trim().into();
    }
    if let Some(body) = optional_text(args.body.as_deref(), args.body_file.as_ref())? {
        request["body"] = body.into();
    }
    if let Some(issue) = args.issue.as_deref().map(str::trim) {
        if issue.is_empty() {
            return validation_error("issue", "must not be empty");
        }
    }
    if let Some(reference) = view_reference(args.issue.as_deref(), args.repo.as_deref())? {
        request["ref"] = reference.into();
    }
    let unchanged = ["title", "body", "addLabels", "removeLabels"]
        .iter()
        .all(|field| request.get(*field).is_none())
        && request["addAssignees"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && request["removeAssignees"]
            .as_array()
            .is_some_and(Vec::is_empty);
    if unchanged {
        return validation_error(
            "edit",
            "name a change: --title, --body/--body-file, --add-assignee, --remove-assignee, --add-label or --remove-label",
        );
    }
    Ok(request)
}

/// `g5d issue edit`: one write as the owner. An unconfirmed edit exits 7
/// (rerunning is harmless); one GitHub made without every assignee change
/// exits 1.
async fn edit_issue(
    client: &InternalApiClient,
    workspace_id: &str,
    json: bool,
    args: &IssueEditArgs,
) -> Result<i32, G5Error> {
    let mut request = edit_request(args)?;
    let images = issue_attach::read_images(&args.attach)?;
    if !images.is_empty() {
        let Some(body) = request["body"].as_str().map(str::to_owned) else {
            return validation_error(
                "attach",
                "--attach adds images to the new description: pass it with -b or -F",
            );
        };
        let reference = request["ref"].as_str().map(str::to_owned);
        let uploaded =
            issue_attach::upload(client, workspace_id, reference.as_deref(), &images).await?;
        request["body"] = issue_attach::with_images(&body, &uploaded.markdown).into();
        request["ref"] = uploaded.issue_url.into();
    }
    // Setting the same values again is harmless, so the client's retries
    // are safe (`g5.external-issues.edit`).
    let response = client
        .workspace_agent_post(workspace_id, "issue-edit", &request, true)
        .await?;
    if json {
        print_json(&response);
    } else {
        print!("{}", issue_links::render_edited(&response["data"]));
    }
    Ok(match response["data"]["outcome"].as_str() {
        Some("applied") => 0,
        Some("partial") => 1,
        _ => OUTCOME_UNCLEAR,
    })
}

fn link_request(args: &IssueLinkArgs) -> Value {
    serde_json::json!({
        "ref": args.issue.trim(),
        "role": if args.primary { "primary" } else { "related" },
    })
}

/// `g5d issue create`: print the issue URL as `gh` does, or the whole result
/// with `--json`. An unclear outcome exits [`OUTCOME_UNCLEAR`] with the key
/// whose rerun looks for the issue before creating it. An issue created but
/// not under its requested parent still prints its URL; the exit status is
/// 7 when GitHub did not confirm the parent and 1 when it refused.
async fn create_issue(
    client: &InternalApiClient,
    workspace_id: &str,
    json: bool,
    args: &IssueCreateArgs,
) -> Result<i32, G5Error> {
    let key = args
        .idempotency_key
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut request = create_request(args, &key)?;
    let images = issue_attach::read_images(&args.attach)?;
    if !images.is_empty() {
        // Refused before anything is created unless new issues are G5's.
        request["attachments"] = true.into();
    }
    let response = match client.create_issue(workspace_id, &request).await {
        Ok(response) => response,
        Err(error) => {
            // A lost response may hide a created issue: a rerun with this key
            // looks for it first.
            eprintln!("To retry this issue safely, pass --idempotency-key {key}");
            return Err(error);
        }
    };
    print_rendered(json, &response, issue_links::render_created);
    if response["data"]["state"] == "created" {
        let mut status = parent_status(&response["data"]["parent"], &key);
        let dropped = names(
            &response["data"]["labelsNotApplied"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|label| label.as_str().map(str::to_owned))
                .collect::<Vec<_>>(),
        );
        if !dropped.is_empty() {
            // GitHub silently drops labels the author may not set; the issue
            // stays, so add them with an explicit change.
            eprintln!(
                "GitHub created the issue without the labels {}; add them with `g5d issue edit {} --add-label ...` if the owner may set them.",
                dropped.join(", "),
                response["data"]["issue"]["displayKey"].as_str().unwrap_or("the issue"),
            );
            status = status.max(1);
        }
        if images.is_empty() {
            return Ok(status);
        }
        let attached =
            attach_to_created(client, workspace_id, &response["data"], &images, &key).await?;
        return Ok(status.max(attached));
    }
    eprintln!(
        "GitHub did not confirm the issue; it may exist. Rerun with --idempotency-key {key} to check for it before creating it again."
    );
    Ok(OUTCOME_UNCLEAR)
}

/// Add `images` to a just-created G5 issue and show them at the end of its
/// description, only if the description is still the one created; 1 when
/// someone changed it first, leaving the images listed at the issue.
///
/// A rerun with `key` is safe only while the description is still the one
/// created (the server refuses a replay whose description differs), so it is
/// suggested only then.
async fn attach_to_created(
    client: &InternalApiClient,
    workspace_id: &str,
    created: &Value,
    images: &[issue_attach::LocalImage],
    key: &str,
) -> Result<i32, G5Error> {
    let url = created["issue"]["url"].as_str().unwrap_or_default();
    let uploaded = issue_attach::upload(client, workspace_id, Some(url), images)
        .await
        .inspect_err(|_| {
            eprintln!(
                "{url} was created but not every image was added; rerun with \
                 --idempotency-key {key} to finish without creating it again."
            );
        })?;
    let original = created["payload"].as_str().unwrap_or_default();
    let shown = issue_attach::with_images(original, &uploaded.markdown);
    let request = serde_json::json!({
        "ref": uploaded.issue_url, "body": shown, "expectedBody": original,
        "addAssignees": [], "removeAssignees": [],
    });
    let error = match client
        .workspace_agent_post(workspace_id, "issue-edit", &request, true)
        .await
    {
        Ok(_) => return Ok(0),
        Err(error) => error,
    };
    // A refusal, or a retry after a lost answer, may hide an edit that
    // landed: the description decides.
    let current = client
        .view_issue(workspace_id, Some(&uploaded.issue_url), false)
        .await
        .ok();
    let body = current
        .as_ref()
        .and_then(|current| current["data"]["issue"]["body"].as_str())
        .map(str::trim_end);
    if body == Some(shown.as_str()) {
        return Ok(0);
    }
    if body.is_some() && body != Some(original.trim_end()) {
        eprintln!(
            "The description of {url} changed before the images could be shown in it; \
             they are attached to the issue (g5d issue attachments lists them). Add them \
             to the text with g5d issue edit if they belong there."
        );
        return Ok(1);
    }
    if body.is_some() {
        eprintln!(
            "The images are attached to {url} but not yet shown in its description; \
             rerun with --idempotency-key {key} to finish."
        );
    } else {
        eprintln!(
            "The images are attached to {url}, but whether its description shows them \
             is unknown; check with g5d issue view {url} before rerunning."
        );
    }
    Err(error)
}

/// Report a requested parent the created issue is not under; the exit
/// status.
fn parent_status(parent: &Value, key: &str) -> i32 {
    let state = parent["state"].as_str().unwrap_or("attached");
    if state == "attached" {
        return 0;
    }
    let display_key = parent["displayKey"].as_str().unwrap_or_default();
    let message = parent["message"]
        .as_str()
        .map(|message| format!(": {message}"))
        .unwrap_or_default();
    eprintln!("The issue was created but not added under {display_key}{message}.");
    match state {
        "outcome_unknown" => {
            eprintln!("Rerun with --idempotency-key {key} to check it and add it there; the issue is not created again.");
            OUTCOME_UNCLEAR
        }
        "not_attached" => {
            eprintln!("Rerun with --idempotency-key {key} to try adding it there again; the issue is not created again.");
            1
        }
        _ => 1,
    }
}

fn print_rendered(json: bool, response: &Value, render: fn(&Value) -> String) {
    if json {
        print_json(response);
    } else {
        print!("{}", render(&response["data"]));
    }
}

/// The workspace a workspace VM runs, as its dispatcher exports it
/// (`G5D_WORKSPACE_ID`). A VM has no Provided-Agent stable cwd metadata.
fn incarnation_workspace_id(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get("G5D_WORKSPACE_ID")
        .map(|id| id.trim().to_string())
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
}

fn auth_headers(token: &str) -> Result<HeaderMap, G5Error> {
    let token = token.trim();
    if token.is_empty() {
        return Err(G5Error::Unauthorized);
    }
    let mut headers = HeaderMap::new();
    let value =
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| G5Error::Internal {
            message: format!("invalid worker auth token header: {e}"),
        })?;
    headers.insert(AUTHORIZATION, value);
    Ok(headers)
}

/// File a platform bug about G5 itself via the worker-attributed internal intake
/// (`POST /api/internal/workspaces/:id/platform/issues`). The reporting user is
/// attributed **server-side** from the task/session — this command sends no user
/// id. A 503 (feature disabled / unconfigured) surfaces as a typed error so the
/// caller reports "unavailable" rather than retrying.
async fn report_platform_issue(
    http: &reqwest::Client,
    base_url: &str,
    workspace_id: &str,
    client: &InternalApiClient,
    args: &IssueReportArgs,
) -> Result<Value, G5Error> {
    let title = args.title.trim();
    if title.is_empty() {
        return validation_error("title", "must not be empty");
    }
    // Severity is a validated Clap enum (low/medium/high/critical); no empty or
    // out-of-vocabulary value can reach here.
    let severity = args.severity.as_str();
    let body_text =
        optional_text(args.body.as_deref(), args.body_file.as_ref())?.unwrap_or_default();

    // Provenance breadcrumbs aid human triage of a chat-filed report; the
    // reporter identity itself is resolved server-side, never sent here.
    let kind = args
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let report = InternalPlatformIssueReportRequest {
        title: title.to_string(),
        body: body_text,
        kind,
        severity: severity.to_string(),
        context: InternalPlatformIssueContext {
            breadcrumbs: vec![
                "source: g5d issue report (chat)".to_string(),
                format!("workspaceId: {workspace_id}"),
            ],
            ..Default::default()
        },
    };

    let url = format!("{base_url}/api/internal/workspaces/{workspace_id}/platform/issues");
    send_json_response(client, http.post(url).json(&report)).await
}

async fn send_json_response(
    client: &InternalApiClient,
    builder: reqwest::RequestBuilder,
) -> Result<Value, G5Error> {
    let builder = builder.headers(auth_headers(&client.current_token()?)?);
    let response = builder.send().await.map_err(|e| G5Error::Internal {
        message: format!("issue API request failed: {e}"),
    })?;
    let status = response.status();
    let text = response.text().await.map_err(|e| G5Error::Internal {
        message: format!("issue API response read failed: {e}"),
    })?;
    let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
        serde_json::json!({
            "error": {
                "message": text,
            }
        })
    });
    if !status.is_success() {
        // A 503 means the platform-issue capability is disabled / unconfigured
        // (the server fail-closes). Surface it as a typed "unavailable" error so
        // the caller reports it as such rather than a validation failure.
        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Err(G5Error::ServiceUnavailable {
                message: format!("issue API unavailable (503): {parsed}"),
            });
        }
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "status": status.as_u16(),
                "response": parsed,
            }),
        });
    }
    Ok(parsed)
}

fn optional_text(inline: Option<&str>, file: Option<&PathBuf>) -> Result<Option<String>, G5Error> {
    match (inline, file) {
        (Some(text), None) => Ok(Some(text.to_string())),
        (None, Some(path)) => Ok(Some(read_text(path)?)),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => validation_error("body", "use either inline text or file, not both"),
    }
}

fn read_text(path: &PathBuf) -> Result<String, G5Error> {
    if path == &PathBuf::from("-") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| G5Error::Internal {
                message: format!("failed to read stdin: {e}"),
            })?;
        return Ok(text);
    }
    std::fs::read_to_string(path).map_err(|e| G5Error::Internal {
        message: format!("failed to read {}: {e}", path.display()),
    })
}

fn validation_error<T>(field: &str, error: impl Into<String>) -> Result<T, G5Error> {
    Err(G5Error::ValidationError {
        details: serde_json::json!({
            "field": field,
            "error": error.into(),
        }),
    })
}

fn print_issue_output(json: bool, response: &Value) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(response).unwrap_or_default()
        );
        return;
    }
    if let Some(submission_id) = response["data"]["submissionId"].as_str() {
        println!("Filed platform issue. submissionId={submission_id}");
        return;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(response).unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::{
        create_request, edit_request, incarnation_workspace_id, link_request, search_query,
        view_reference,
    };
    use crate::cli::{Cli, Command, IssueCommand};
    use clap::Parser;
    use g5_contracts::error::G5Error;
    use serde_json::Value;
    use std::path::PathBuf;

    fn create_args(args: &[&str]) -> crate::cli::IssueCreateArgs {
        let cli = Cli::try_parse_from(
            ["g5d", "issue", "create"]
                .iter()
                .chain(args)
                .copied()
                .collect::<Vec<_>>(),
        )
        .unwrap();
        match cli.command {
            Command::Issue(issue) => match issue.command {
                IssueCommand::Create(create) => create,
                other => panic!("parsed {other:?}"),
            },
            other => panic!("parsed {other:?}"),
        }
    }

    #[test]
    fn create_takes_gh_flags_and_never_prompts() {
        let args = create_args(&[
            "-t",
            "Crash on save",
            "-b",
            "Steps",
            "-a",
            "@me,octo",
            "--assignee",
            "hub",
        ]);
        let request = create_request(&args, "k-1").unwrap();
        assert_eq!(
            request,
            serde_json::json!({
                "title": "Crash on save", "body": "Steps",
                "assignees": ["@me", "octo", "hub"], "idempotencyKey": "k-1",
            })
        );
        // Like `gh` without a terminal: title and body are both required.
        for missing in [vec!["-b", "Steps"], vec!["-t", "Crash"], vec![]] {
            let error = create_request(&create_args(&missing), "k").unwrap_err();
            assert!(error.to_string().contains("never prompts"), "{error}");
        }
        // A parent is sent only when given, and a blank one is refused.
        let request = create_request(
            &create_args(&["-t", "T", "-b", "", "--parent", " #12 "]),
            "k",
        )
        .unwrap();
        assert_eq!(request["parent"], "#12");
        assert!(create_request(&create_args(&["-t", "T", "-b", ""]), "k")
            .unwrap()
            .get("parent")
            .is_none());
        let error =
            create_request(&create_args(&["-t", "T", "-b", "", "--parent", " "]), "k").unwrap_err();
        assert!(
            matches!(&error, G5Error::ValidationError { details } if details["field"] == "parent"),
            "{error}"
        );
        // Labels are sent only when given, as gh's -l/--label names them.
        let request = create_request(
            &create_args(&[
                "-t",
                "T",
                "-b",
                "",
                "-l",
                "bug,priority:p2",
                "--label",
                "ui",
            ]),
            "k",
        )
        .unwrap();
        assert_eq!(
            request["labels"],
            serde_json::json!(["bug", "priority:p2", "ui"])
        );
        assert!(create_request(&create_args(&["-t", "T", "-b", ""]), "k")
            .unwrap()
            .get("labels")
            .is_none());
        // An empty body is a body, as `gh issue create -b ''` accepts.
        assert!(create_request(&create_args(&["-t", "Crash", "-b", ""]), "k").is_ok());
        assert!(Cli::try_parse_from(["g5d", "issue", "create", "-b", "x", "-F", "f"]).is_err());
    }

    #[test]
    fn view_is_show_and_link_defaults_to_related() {
        let cli = Cli::try_parse_from(["g5d", "issue", "view", "#12"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Issue(issue) if matches!(&issue.command, IssueCommand::Show(show) if show.issue.as_deref() == Some("#12"))
        ));
        for (args, role) in [
            (vec!["12"], "related"),
            (vec!["acme/web#12", "--primary"], "primary"),
        ] {
            let cli = Cli::try_parse_from(
                ["g5d", "issue", "link"]
                    .iter()
                    .chain(&args)
                    .copied()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let Command::Issue(issue) = cli.command else {
                panic!()
            };
            let IssueCommand::Link(link) = issue.command else {
                panic!()
            };
            assert_eq!(link_request(&link)["role"], role);
            assert_eq!(link_request(&link)["ref"], args[0]);
        }
        // Comments take gh's short body flags too.
        assert!(Cli::try_parse_from(["g5d", "issue", "comment", "-b", "hi"]).is_ok());
    }

    fn issue_args(args: &[&str]) -> crate::cli::IssueArgs {
        let cli = Cli::try_parse_from(
            ["g5d", "issue"]
                .iter()
                .chain(args)
                .copied()
                .collect::<Vec<_>>(),
        )
        .unwrap();
        match cli.command {
            Command::Issue(issue) => issue,
            other => panic!("parsed {other:?}"),
        }
    }

    #[test]
    fn json_takes_gh_fields_in_every_position_and_alone_means_the_full_record() {
        for args in [
            vec!["view", "12", "--json", "title,url"],
            vec!["view", "--json=title,url", "12"],
            vec!["--json", "title,url", "view", "12"],
        ] {
            let issue = issue_args(&args);
            assert_eq!(
                issue.json_fields,
                Some(Some("title,url".to_string())),
                "{args:?}"
            );
            assert!(
                matches!(&issue.command, IssueCommand::Show(show) if show.issue.as_deref() == Some("12")),
                "{args:?}"
            );
        }
        for args in [
            vec!["--json", "list"],
            vec!["list", "--json"],
            vec!["view", "12", "--json"],
        ] {
            assert_eq!(issue_args(&args).json_fields, Some(None), "{args:?}");
        }
        // `view --json 12` parses 12 as fields; execute() keeps it as the
        // issue because it names no field.
        let issue = issue_args(&["view", "--json", "12"]);
        assert_eq!(issue.json_fields, Some(Some("12".to_string())));
        assert!(!crate::cli::issue_json::is_field_list(
            "12",
            crate::cli::issue_json::VIEW_FIELDS
        ));
    }

    #[test]
    fn attachments_names_the_issue_as_view_does() {
        let issue = issue_args(&[
            "attachments",
            "12",
            "-c",
            "-R",
            "acme/web",
            "--dir",
            "/tmp/mocks",
        ]);
        let IssueCommand::Attachments(attachments) = issue.command else {
            panic!()
        };
        assert!(attachments.comments && !attachments.list);
        assert_eq!(
            attachments.dir.as_deref(),
            Some(std::path::Path::new("/tmp/mocks"))
        );
        assert_eq!(
            view_reference(attachments.issue.as_deref(), attachments.repo.as_deref()).unwrap(),
            Some("acme/web#12".to_string())
        );
        let primary = issue_args(&["attachments", "--list"]);
        let IssueCommand::Attachments(attachments) = primary.command else {
            panic!()
        };
        assert!(attachments.issue.is_none() && attachments.list && attachments.dir.is_none());
        // Listing saves nothing, so it takes no directory.
        assert!(
            Cli::try_parse_from(["g5d", "issue", "attachments", "--list", "--dir", "x"]).is_err()
        );
    }

    #[test]
    fn attach_is_repeatable_on_create_comment_and_edit() {
        let issue = issue_args(&[
            "comment", "ACME-3", "-b", "Shot:", "--attach", "a.png", "--attach", "b.gif",
        ]);
        let IssueCommand::Comment(comment) = issue.command else {
            panic!()
        };
        assert_eq!(
            comment.attach,
            [PathBuf::from("a.png"), PathBuf::from("b.gif")]
        );
        let issue = issue_args(&["create", "-t", "T", "-b", "B", "--attach", "a.png"]);
        let IssueCommand::Create(create) = issue.command else {
            panic!()
        };
        assert_eq!(create.attach, [PathBuf::from("a.png")]);
        let issue = issue_args(&["edit", "ACME-3", "-b", "New", "--attach", "a.png"]);
        let IssueCommand::Edit(edit) = issue.command else {
            panic!()
        };
        assert_eq!(edit.attach, [PathBuf::from("a.png")]);
    }

    #[test]
    fn pr_attachments_names_a_github_pull_request_as_view_does() {
        let cli = Cli::try_parse_from([
            "g5d",
            "pr",
            "attachments",
            "42",
            "-R",
            "acme/api",
            "-c",
            "--list",
        ])
        .unwrap();
        let Command::Pr(pr) = cli.command else {
            panic!()
        };
        let crate::cli::PrCommand::Attachments(attachments) = pr.command else {
            panic!()
        };
        assert_eq!(attachments.pull_request, "42");
        assert_eq!(attachments.repo.as_deref(), Some("acme/api"));
        assert!(attachments.comments && attachments.list);
        assert!(Cli::try_parse_from(["g5d", "pr", "attachments"]).is_err());
        assert!(
            Cli::try_parse_from(["g5d", "pr", "attachments", "42", "--list", "--dir", "x"])
                .is_err()
        );
    }

    #[test]
    fn view_takes_comments_and_a_repository_for_a_number() {
        let issue = issue_args(&["view", "12", "-c", "-R", "acme/web"]);
        let IssueCommand::Show(show) = issue.command else {
            panic!()
        };
        assert!(show.comments);
        assert_eq!(
            view_reference(show.issue.as_deref(), show.repo.as_deref()).unwrap(),
            Some("acme/web#12".to_string())
        );
        assert_eq!(
            view_reference(Some("#7"), Some("acme/api")).unwrap(),
            Some("acme/api#7".to_string())
        );
        assert_eq!(view_reference(None, None).unwrap(), None);
        // A G5 issue key names its issues KEY-N.
        assert_eq!(
            view_reference(Some("#7"), Some("ACME")).unwrap(),
            Some("ACME-7".to_string())
        );
        for (issue, repo) in [
            (None, "acme/web"),
            (Some("acme/web#3"), "acme/web"),
            (Some("3"), "web"),
        ] {
            let error = view_reference(issue, Some(repo)).unwrap_err();
            assert!(
                matches!(&error, G5Error::ValidationError { details } if details["field"] == "repo"),
                "{issue:?} {repo}: {error}"
            );
        }
    }

    #[test]
    fn comment_names_any_issue_as_view_does() {
        let issue = issue_args(&["comment", "12", "-R", "ACME", "-b", "Started"]);
        let IssueCommand::Comment(comment) = issue.command else {
            panic!()
        };
        assert_eq!(
            view_reference(comment.issue.as_deref(), comment.repo.as_deref()).unwrap(),
            Some("ACME-12".to_string())
        );
        let issue = issue_args(&["comment", "acme/web#3", "--body", "x"]);
        let IssueCommand::Comment(comment) = issue.command else {
            panic!()
        };
        assert_eq!(comment.issue.as_deref(), Some("acme/web#3"));
        let issue = issue_args(&["comment", "-b", "x"]);
        let IssueCommand::Comment(comment) = issue.command else {
            panic!()
        };
        assert_eq!(comment.issue, None, "the primary issue by default");
    }

    fn edit(args: &[&str]) -> Result<Value, G5Error> {
        let issue = issue_args(&[&["edit"], args].concat());
        let IssueCommand::Edit(edit) = issue.command else {
            panic!()
        };
        edit_request(&edit)
    }

    #[test]
    fn edit_sends_gh_style_changes_for_the_issue_view_names() {
        let request = edit(&[
            "12",
            "-R",
            "ACME",
            "-t",
            " New title ",
            "--add-assignee",
            "a@acme.dev,@me",
            "--add-assignee",
            "b@acme.dev",
            "--remove-assignee",
            "c@acme.dev",
        ])
        .unwrap();
        assert_eq!(request["ref"], "ACME-12");
        assert_eq!(request["title"], "New title");
        assert_eq!(
            request["addAssignees"],
            serde_json::json!(["a@acme.dev", "@me", "b@acme.dev"])
        );
        assert_eq!(
            request["removeAssignees"],
            serde_json::json!(["c@acme.dev"])
        );
        assert!(request.get("body").is_none(), "an unnamed field is kept");

        // The primary issue by default; a description alone is a change.
        let request = edit(&["-b", "New description"]).unwrap();
        assert!(request.get("ref").is_none());
        assert_eq!(request["body"], "New description");

        // Nothing to change, a blank title or a blank issue is refused locally.
        for args in [&[][..], &["12"], &["-t", " "], &["", "-t", "x"]] {
            assert!(edit(args).is_err(), "{args:?}");
        }

        // Labels change by deltas, as gh names them; a label change alone
        // is an edit.
        let request = edit(&[
            "ACME-3",
            "--add-label",
            "priority:p1,triaged",
            "--remove-label",
            "priority:p0",
        ])
        .unwrap();
        assert_eq!(
            request["addLabels"],
            serde_json::json!(["priority:p1", "triaged"])
        );
        assert_eq!(request["removeLabels"], serde_json::json!(["priority:p0"]));
        assert!(edit(&["--add-label", " "]).is_err());
    }

    #[test]
    fn edit_refuses_gh_milestones_and_projects_with_guidance() {
        for flag in ["--milestone", "--remove-project"] {
            let args = ["g5d", "issue", "edit", "12", flag, "x"];
            let error = Cli::try_parse_from(args).unwrap_err();
            let args: Vec<_> = args.iter().map(std::ffi::OsString::from).collect();
            let hint = crate::cli::gh_habit_hint(&args, error.kind()).unwrap();
            assert!(
                hint.contains("title, description, assignees and labels"),
                "{flag}: {hint}"
            );
        }
    }

    #[test]
    fn close_and_reopen_name_any_issue_as_view_does() {
        let issue = issue_args(&["close", "12", "-R", "acme/web"]);
        let IssueCommand::Close(close) = issue.command else {
            panic!()
        };
        assert_eq!(
            view_reference(close.issue.as_deref(), close.repo.as_deref()).unwrap(),
            Some("acme/web#12".to_string())
        );
    }

    fn search(args: &[&str]) -> Result<Vec<(&'static str, String)>, G5Error> {
        let issue = issue_args(&[&["search"], args].concat());
        let IssueCommand::Search(search) = issue.command else {
            panic!()
        };
        search_query(&search)
    }

    #[test]
    fn search_sends_gh_filters_and_defaults_to_both_states_and_thirty() {
        let query = search(&[
            "crash",
            "label:bug",
            "--state",
            "open",
            "--assignee",
            "@me",
            "-R",
            "acme/web",
            "-L",
            "5",
        ])
        .unwrap();
        let get = |key: &str| {
            query
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("q"), Some("crash label:bug"));
        assert_eq!(get("state"), Some("open"));
        assert_eq!(get("assignee"), Some("me"));
        assert_eq!(get("repo"), Some("acme/web"));
        assert_eq!(get("limit"), Some("5"));
        // A G5 issue key names the native domain.
        let query = search(&["-R", "ACME"]).unwrap();
        assert!(query.contains(&("repo", "ACME".to_string())));
        let query = search(&["--no-assignee"]).unwrap();
        assert!(query.contains(&("assignee", "none".to_string())));
        assert!(query.contains(&("state", "all".to_string())));
        assert!(query.contains(&("limit", "30".to_string())));
        assert!(!query.iter().any(|(k, _)| *k == "q"));
        // gh's --label and --no-label become qualifiers that keep the exact
        // name.
        let query = search(&["crash", "--label", "bug,good first issue"]).unwrap();
        assert!(query.contains(&(
            "q",
            r#"crash label:"bug" label:"good first issue""#.to_string()
        )));
        let query = search(&["--no-label"]).unwrap();
        assert!(query.contains(&("q", "no:label".to_string())));
        assert!(search(&["--label", "a\"b"]).is_err());
        assert!(
            Cli::try_parse_from(["g5d", "issue", "search", "--label", "x", "--no-label"]).is_err()
        );
    }

    #[test]
    fn search_refuses_what_g5_cannot_apply_and_says_what_to_use() {
        for (args, field, needle) in [
            (vec!["--assignee", "octo"], "assignee", "assignee:octo"),
            (vec!["-L", "31"], "limit", "at most 30"),
            (vec!["-L", "0"], "limit", "at most 30"),
            (vec!["crash", "repo:acme/web"], "query", "-R owner/name"),
            (vec!["is:open"], "query", "--state"),
            (vec!["--", "-type:pr"], "query", "issues only"),
            (vec!["-R", "web"], "repo", "owner/name"),
        ] {
            let error = search(&args).unwrap_err();
            assert!(
                matches!(&error, G5Error::ValidationError { details } if details["field"] == field
                    && details["error"].as_str().unwrap().contains(needle)),
                "{args:?}: {error:?}"
            );
        }
        assert!(Cli::try_parse_from([
            "g5d",
            "issue",
            "search",
            "--assignee",
            "@me",
            "--no-assignee"
        ])
        .is_err());
        assert!(Cli::try_parse_from(["g5d", "issue", "search", "--state", "all"]).is_err());
        assert!(Cli::try_parse_from(["g5d", "issue", "unlink"]).is_err());
    }

    #[test]
    fn a_workspace_vm_names_its_workspace_in_the_environment() {
        let id = "9a9f7c64-839b-4b07-bf9c-95b080a4c316";
        assert_eq!(
            incarnation_workspace_id(|key| (key == "G5D_WORKSPACE_ID").then(|| id.to_string())),
            Some(id.to_string())
        );
        assert_eq!(incarnation_workspace_id(|_| None), None);
        assert_eq!(
            incarnation_workspace_id(|_| Some("not-a-workspace".to_string())),
            None
        );
    }
}
