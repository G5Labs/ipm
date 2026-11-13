//! `g5d issue label` — an issue domain's label catalog, as the workspace
//! owner (`g5.issue.labels.surfaces`), and how label changes print.

use g5_contracts::error::G5Error;
use g5_internal_api::client::InternalApiClient;
use serde_json::{json, Value};

use super::{IssueLabelArgs, IssueLabelCommand};

/// Exit status of a change GitHub did not confirm, as for `g5d issue edit`.
const OUTCOME_UNCLEAR: i32 = 7;

fn validation_error<T>(field: &str, error: impl Into<String>) -> Result<T, G5Error> {
    Err(G5Error::ValidationError {
        details: json!({"field": field, "error": error.into()}),
    })
}

/// The domain a command names. A blank `--domain` (say, an unset variable)
/// is refused rather than falling back to the creation domain.
fn domain(value: Option<&str>) -> Result<Option<String>, G5Error> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => validation_error("domain", "must not be empty"),
        Some(value) => Ok(Some(value.to_owned())),
    }
}

/// The list route's path with its query.
fn list_path(domain: Option<&str>, page: Option<u32>) -> String {
    let mut url = reqwest::Url::parse("http://placeholder/issue-labels").expect("static URL");
    {
        let mut query = url.query_pairs_mut();
        if let Some(domain) = domain {
            query.append_pair("domain", domain);
        }
        if let Some(page) = page {
            query.append_pair("page", &page.to_string());
        }
    }
    match url.query().filter(|query| !query.is_empty()) {
        Some(query) => format!("issue-labels?{query}"),
        None => "issue-labels".to_string(),
    }
}

/// The request a label subcommand sends, and its route; `None` for `list`.
fn request(command: &IssueLabelCommand) -> Result<Option<(&'static str, Value)>, G5Error> {
    let with_domain = |mut body: Value, value: Option<&str>| -> Result<Value, G5Error> {
        if let Some(domain) = domain(value)? {
            body["domain"] = domain.into();
        }
        Ok(body)
    };
    Ok(match command {
        IssueLabelCommand::List(_) => None,
        IssueLabelCommand::Create(args) => {
            let mut body = json!({"name": args.name});
            if let Some(color) = &args.color {
                body["color"] = color.as_str().into();
            }
            if let Some(description) = &args.description {
                body["description"] = description.as_str().into();
            }
            Some(("issue-labels", with_domain(body, args.domain.as_deref())?))
        }
        IssueLabelCommand::Edit(args) => {
            let mut body = json!({"label": args.label});
            if let Some(name) = &args.new_name {
                body["name"] = name.as_str().into();
            }
            if let Some(color) = &args.color {
                body["color"] = color.as_str().into();
            }
            if let Some(description) = &args.description {
                body["description"] = description.as_str().into();
            }
            if args.new_name.is_none() && args.color.is_none() && args.description.is_none() {
                return validation_error("edit", "name a change: --name, --color or --description");
            }
            Some((
                "issue-labels/edit",
                with_domain(body, args.domain.as_deref())?,
            ))
        }
        IssueLabelCommand::Delete(args) => {
            if !args.yes {
                return validation_error(
                    "yes",
                    format!(
                        "deleting {} removes it from every issue; confirm with --yes",
                        args.label
                    ),
                );
            }
            Some((
                "issue-labels/delete",
                with_domain(json!({"label": args.label}), args.domain.as_deref())?,
            ))
        }
    })
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// Run `g5d issue label`, returning the exit status.
pub async fn execute(
    client: &InternalApiClient,
    workspace_id: &str,
    json: bool,
    args: &IssueLabelArgs,
) -> Result<i32, G5Error> {
    let Some((path, body)) = request(&args.command)? else {
        let IssueLabelCommand::List(list) = &args.command else {
            unreachable!("only list sends no body")
        };
        let response = client
            .workspace_agent_get(
                workspace_id,
                &list_path(domain(list.domain.as_deref())?.as_deref(), list.page),
            )
            .await?;
        if json {
            print_json(&response);
        } else {
            print!("{}", render_list(&response["data"]));
        }
        return Ok(0);
    };
    // A create is reconciled by name, so retrying it is safe; an edit or a
    // delete is sent once and an unclear outcome reported.
    let retry = matches!(args.command, IssueLabelCommand::Create(_));
    let response = client
        .workspace_agent_post(workspace_id, path, &body, retry)
        .await?;
    if json {
        print_json(&response);
    } else {
        print!("{}", render_written(&args.command, &response["data"]));
    }
    Ok(if response["data"]["outcome"] == "applied" {
        0
    } else {
        OUTCOME_UNCLEAR
    })
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// One label as `gh label list` prints it: name, description, colour.
fn label_line(label: &Value) -> String {
    let color = label["color"]
        .as_str()
        .map(|color| format!("#{color}"))
        .unwrap_or_default();
    format!(
        "{}\t{}\t{color}",
        text(&label["name"]),
        text(&label["description"])
    )
}

/// `g5d issue label list`.
#[must_use]
pub fn render_list(data: &Value) -> String {
    let labels = data["labels"].as_array().cloned().unwrap_or_default();
    let mut out = if labels.is_empty() {
        format!("{} has no labels.\n", text(&data["displayKey"]))
    } else {
        labels
            .iter()
            .map(|label| format!("{}\n", label_line(label)))
            .collect()
    };
    if let Some(next) = data["nextPage"].as_u64() {
        out.push_str(&format!(
            "More labels: rerun with --page {next}{}.\n",
            match data["displayKey"].as_str() {
                Some(key) => format!(" --domain {key}"),
                None => String::new(),
            }
        ));
    }
    out
}

/// `g5d issue label create|edit|delete`.
fn render_written(command: &IssueLabelCommand, data: &Value) -> String {
    let by = match data["githubLogin"].as_str() {
        Some(login) => format!(" as @{login}"),
        None => String::new(),
    };
    if data["outcome"] != "applied" {
        return format!(
            "GitHub did not confirm the change: {}. List the labels before trying again.\n",
            data["message"].as_str().unwrap_or("unknown outcome")
        );
    }
    let label = &data["label"];
    let verb = match command {
        IssueLabelCommand::Create(_) => "Created",
        IssueLabelCommand::Edit(_) => "Saved",
        IssueLabelCommand::Delete(_) => "Deleted",
        IssueLabelCommand::List(_) => "Listed",
    };
    format!("{verb} label{by}: {}\n", label_line(label))
}

/// The label part of `g5d issue edit`'s report.
#[must_use]
pub fn render_label_change(labels: &Value) -> String {
    let changes = labels["changes"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    let applied: Vec<String> = changes
        .iter()
        .filter(|change| change["outcome"] == "applied")
        .map(|change| {
            format!(
                "{}{}",
                if change["change"] == "add" { "+" } else { "-" },
                text(&change["label"])
            )
        })
        .collect();
    if !applied.is_empty() {
        out.push_str(&format!("Labels changed: {}\n", applied.join(", ")));
    }
    for change in changes
        .iter()
        .filter(|change| change["outcome"] != "applied")
    {
        let what = if change["change"] == "add" {
            "Not added"
        } else {
            "Not removed"
        };
        let why = match text(&change["outcome"]) {
            "unknown" => "GitHub did not confirm it".to_string(),
            "not_attempted" => "not attempted after an earlier refusal".to_string(),
            _ => change["message"]
                .as_str()
                .unwrap_or("GitHub refused it")
                .to_string(),
        };
        out.push_str(&format!("{what}: {} ({why})\n", text(&change["label"])));
    }
    if let Some(issue) = labels["issue"].as_object() {
        let now: Vec<&str> = issue
            .get("labels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|label| text(&label["name"]))
            .collect();
        out.push_str(&format!(
            "Labels now: {}\n",
            if now.is_empty() {
                "none".to_string()
            } else {
                now.join(", ")
            }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command, IssueCommand};
    use clap::Parser;

    fn label_command(args: &[&str]) -> IssueLabelCommand {
        let cli = Cli::try_parse_from([&["g5d", "issue", "label"], args].concat()).unwrap();
        let Command::Issue(issue) = cli.command else {
            panic!("not an issue command")
        };
        let IssueCommand::Label(label) = issue.command else {
            panic!("not a label command")
        };
        label.command
    }

    #[test]
    fn label_commands_send_gh_style_requests_and_delete_needs_yes() {
        let (path, body) = request(&label_command(&[
            "create",
            "priority:p0",
            "-c",
            "d73a4a",
            "-d",
            "Drop everything",
            "--domain",
            "ACME",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(path, "issue-labels");
        assert_eq!(
            body,
            json!({"name": "priority:p0", "color": "d73a4a", "description": "Drop everything", "domain": "ACME"})
        );
        let (path, body) = request(&label_command(&[
            "edit", "wip", "--name", "doing", "-d", "",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(path, "issue-labels/edit");
        assert_eq!(
            body,
            json!({"label": "wip", "name": "doing", "description": ""})
        );
        assert!(request(&label_command(&["edit", "wip"])).is_err());
        assert!(request(&label_command(&["delete", "wip"])).is_err());
        let (path, body) = request(&label_command(&["delete", "wip", "--yes"]))
            .unwrap()
            .unwrap();
        assert_eq!(path, "issue-labels/delete");
        assert_eq!(body, json!({"label": "wip"}));
        assert!(request(&label_command(&["list"])).unwrap().is_none());
        // A blank domain (an unset variable) never means the default one.
        for args in [
            &["delete", "wip", "--yes", "--domain", " "][..],
            &["create", "x", "--domain", ""],
            &["edit", "wip", "-n", "y", "--domain", ""],
        ] {
            assert!(request(&label_command(args)).is_err(), "{args:?}");
        }
        assert!(domain(Some(" ")).is_err());
        assert_eq!(domain(Some(" ACME ")).unwrap().as_deref(), Some("ACME"));
        assert_eq!(list_path(None, None), "issue-labels");
        assert_eq!(
            list_path(Some("acme/web"), Some(2)),
            "issue-labels?domain=acme%2Fweb&page=2"
        );
    }

    #[test]
    fn label_lists_and_changes_print_what_happened() {
        let page = json!({
            "displayKey": "ACME",
            "labels": [
                {"id": "1", "name": "bug", "color": "d73a4a", "description": "Broken"},
                {"id": "2", "name": "wip", "color": null}
            ],
            "nextPage": 2
        });
        assert_eq!(
            render_list(&page),
            "bug\tBroken\t#d73a4a\nwip\t\t\nMore labels: rerun with --page 2 --domain ACME.\n"
        );
        let change = json!({
            "outcome": "partial",
            "changes": [
                {"label": "p1", "change": "add", "outcome": "applied"},
                {"label": "p0", "change": "remove", "outcome": "refused", "message": "no"},
                {"label": "wip", "change": "remove", "outcome": "not_attempted"}
            ],
            "issue": {"labels": [{"name": "p0"}, {"name": "p1"}]}
        });
        assert_eq!(
            render_label_change(&change),
            "Labels changed: +p1\nNot removed: p0 (no)\nNot removed: wip (not attempted after an earlier refusal)\nLabels now: p0, p1\n"
        );
    }
}
