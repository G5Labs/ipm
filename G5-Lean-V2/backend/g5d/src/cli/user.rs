//! `g5d user search|view` — the workspace agent looking up its organization's
//! members, for naming assignees (`g5.organization.agent-member-directory`).
//!
//! Calls `/api/internal/workspace-agent/<this workspace>/users` with the VM's
//! own credential; the server reads the roster as the owner's Members page
//! does and adds each member's connected provider accounts.

use g5_contracts::error::G5Error;
use serde_json::{json, Map, Value};

use super::{issue_json, stable_context, GlobalFlags, UserArgs, UserCommand};

/// The fields `--json a,b` can select from a member record.
pub const USER_FIELDS: &[&str] = &["accounts", "email", "id", "me", "name", "role", "state"];

/// gh user fields G5 members do not have, and where to look instead.
fn unavailable(field: &str) -> Option<&'static str> {
    match field {
        "login" => Some("a member's provider logins are under accounts"),
        _ => None,
    }
}

/// Whether a value that followed `--json` names fields rather than the
/// member or search words (`g5d user view --json @me`).
fn is_field_list(value: &str) -> bool {
    value
        .split(',')
        .map(str::trim)
        .any(|field| USER_FIELDS.contains(&field) || unavailable(field).is_some())
}

/// What one run asks for once a value clap read as `--json FIELDS` is put
/// back where it belongs.
#[derive(Debug, PartialEq, Eq)]
struct Lookup {
    path: String,
    /// `None`: text; `Some(None)`: the full record; `Some(Some(fields))`.
    json: Option<Option<Vec<String>>>,
}

fn lookup(args: &UserArgs, global_json: bool) -> Result<Lookup, G5Error> {
    let mut fields = args.json_fields.clone();
    let mut stray: Option<String> = None;
    if let Some(Some(value)) = &fields {
        if !is_field_list(value) {
            stray = Some(value.clone());
            fields = Some(None);
        }
    }
    let json = match fields {
        Some(fields) => Some(issue_json::parse_fields(
            fields.as_deref(),
            USER_FIELDS,
            unavailable,
        )?),
        None if global_json => Some(None),
        None => None,
    };
    Ok(Lookup {
        path: path(&args.command, stray)?,
        json,
    })
}

/// Run a `g5d user` subcommand; its exit status.
pub async fn execute(args: &UserArgs, global: &GlobalFlags) -> Result<i32, G5Error> {
    let lookup = lookup(args, global.json)?;
    let workspace = super::workspace_agent::workspace_id_from(|key| std::env::var(key).ok())
        .ok_or_else(|| {
            G5Error::Config(
                "`g5d user` runs inside a G5 workspace VM: workspace identity and credentials \
                 are not available here."
                    .to_string(),
            )
        })?;
    let client = stable_context::require_worker_client(global).map_err(|_| {
        G5Error::Config(
            "`g5d user` needs workspace credentials; run it from the agent's workspace VM."
                .to_string(),
        )
    })?;
    let response = client.workspace_agent_get(&workspace, &lookup.path).await?;
    let data = &response["data"];
    match &lookup.json {
        Some(fields) => {
            if matches!(args.command, UserCommand::Search(_))
                && data["hasMore"].as_bool() == Some(true)
            {
                eprintln!("More members match; add words or raise -L to see them.");
            }
            let printed = as_json(&args.command, data, fields.as_deref());
            println!(
                "{}",
                serde_json::to_string_pretty(&printed).unwrap_or_default()
            );
        }
        None => println!("{}", render(&args.command, data)),
    }
    Ok(0)
}

/// `view`: the member as an object; `search`: an array of members, as gh
/// prints `view` and `search`. `fields` keeps only those keys.
fn as_json(command: &UserCommand, data: &Value, fields: Option<&[String]>) -> Value {
    let project = |user: &Value| match fields {
        None => user.clone(),
        Some(fields) => Value::Object(
            fields
                .iter()
                .map(|field| (field.clone(), user[field.as_str()].clone()))
                .collect::<Map<_, _>>(),
        ),
    };
    let users = data["users"].as_array().cloned().unwrap_or_default();
    match command {
        UserCommand::View(_) => users.first().map(project).unwrap_or(Value::Null),
        UserCommand::Search(_) => Value::Array(users.iter().map(project).collect()),
    }
}

/// The lookup path with its query. `stray` is a word clap took as the value
/// of `--json`: a search word, or the member `view` names.
fn path(command: &UserCommand, stray: Option<String>) -> Result<String, G5Error> {
    let mut params: Vec<(&str, String)> = Vec::new();
    match command {
        UserCommand::Search(search) => {
            let words = stray
                .into_iter()
                .chain(search.query.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            if !words.trim().is_empty() {
                params.push(("q", words));
            }
            if search.include_deactivated {
                params.push(("includeDeactivated", "true".into()));
            }
            if let Some(limit) = search.limit {
                params.push(("limit", limit.to_string()));
            }
        }
        UserCommand::View(view) => {
            let user = match (&view.user, stray) {
                (Some(user), None) => user.clone(),
                (None, Some(user)) => user,
                (Some(_), Some(value)) => {
                    return Err(validation(&format!(
                        "{value:?} is not a JSON field. Available fields: {}",
                        USER_FIELDS.join(", ")
                    )))
                }
                (None, None) => {
                    return Err(validation(
                        "name a member: @me, a user id, an email or a login",
                    ))
                }
            };
            params.push(("user", user));
        }
    }
    let url = reqwest::Url::parse_with_params("http://users/", &params).map_err(|error| {
        G5Error::Internal {
            message: format!("cannot encode the lookup: {error}"),
        }
    })?;
    Ok(match url.query() {
        Some(query) if !query.is_empty() => format!("users?{query}"),
        _ => "users".into(),
    })
}

fn validation(error: &str) -> G5Error {
    G5Error::ValidationError {
        details: json!({"field": "json", "error": error}),
    }
}

/// One line per member: name, email, role, state, accounts.
fn render(command: &UserCommand, data: &Value) -> String {
    let users = data["users"].as_array().cloned().unwrap_or_default();
    if users.is_empty() {
        return "No members match.".into();
    }
    let mut lines: Vec<String> = users.iter().map(line).collect();
    if matches!(command, UserCommand::Search(_)) && data["hasMore"].as_bool() == Some(true) {
        lines.push(format!(
            "Showing the first {}; add words or raise -L to see more.",
            users.len()
        ));
    }
    lines.join("\n")
}

fn line(user: &Value) -> String {
    let text = |key: &str| user[key].as_str().unwrap_or("-").to_string();
    let mut fields = vec![
        format!("{} <{}>", text("name"), text("email")),
        text("role"),
    ];
    if user["state"].as_str() != Some("active") {
        fields.push(text("state"));
    }
    let accounts: Vec<String> = user["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|account| {
            format!(
                "{}:{}",
                account["provider"].as_str().unwrap_or("-"),
                account["login"].as_str().unwrap_or("-")
            )
        })
        .collect();
    if !accounts.is_empty() {
        fields.push(accounts.join(" "));
    }
    if user["me"].as_bool() == Some(true) {
        fields.push("(@me, the workspace owner)".into());
    }
    fields.push(text("id"));
    fields.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;
    use serde_json::json;

    fn user_args(args: &[&str]) -> UserArgs {
        let mut argv = vec!["g5d", "user"];
        argv.extend_from_slice(args);
        match Cli::try_parse_from(argv).expect("parses").command {
            crate::cli::Command::User(args) => args,
            other => panic!("unexpected command {other:?}"),
        }
    }

    fn command(args: &[&str]) -> UserCommand {
        user_args(args).command
    }

    fn path(command: &UserCommand) -> Result<String, G5Error> {
        super::path(command, None)
    }

    #[test]
    fn search_words_and_flags_become_the_query() {
        assert_eq!(path(&command(&["search"])).unwrap(), "users");
        assert_eq!(
            path(&command(&[
                "search",
                "ada",
                "love",
                "-L",
                "5",
                "--include-deactivated"
            ]))
            .unwrap(),
            "users?q=ada+love&includeDeactivated=true&limit=5"
        );
        assert_eq!(path(&command(&["list"])).unwrap(), "users");
    }

    #[test]
    fn view_names_one_member_exactly() {
        assert_eq!(
            path(&command(&["view", "@me"])).unwrap(),
            "users?user=%40me"
        );
        assert_eq!(
            path(&command(&["show", "github:octo"])).unwrap(),
            "users?user=github%3Aocto"
        );
    }

    #[test]
    fn json_takes_fields_in_any_position_and_alone_means_the_full_record() {
        let fields = |list: &[&str]| Some(Some(list.iter().map(|f| f.to_string()).collect()));
        for argv in [
            vec!["view", "@me", "--json", "email,accounts"],
            vec!["--json", "email,accounts", "view", "@me"],
            vec!["view", "--json=email,accounts", "@me"],
        ] {
            let lookup = lookup(&user_args(&argv), false).unwrap();
            assert_eq!(lookup.path, "users?user=%40me", "{argv:?}");
            assert_eq!(lookup.json, fields(&["email", "accounts"]), "{argv:?}");
        }
        for argv in [vec!["view", "@me", "--json"], vec!["--json", "search"]] {
            assert_eq!(
                lookup(&user_args(&argv), false).unwrap().json,
                Some(None),
                "{argv:?}"
            );
        }
        // The root `--json` asks for the full record too; no flag is text.
        let plain = user_args(&["search"]);
        assert_eq!(lookup(&plain, true).unwrap().json, Some(None));
        assert_eq!(lookup(&plain, false).unwrap().json, None);
        // A word clap read as fields is put back: the member, or a search word.
        let view = lookup(&user_args(&["view", "--json", "ada@x.io"]), false).unwrap();
        assert_eq!(
            (view.path.as_str(), view.json),
            ("users?user=ada%40x.io", Some(None))
        );
        let search = lookup(&user_args(&["search", "--json", "ada", "love"]), false).unwrap();
        assert_eq!(search.path, "users?q=ada+love");
        // Unknown fields are refused with the available ones.
        let error = lookup(&user_args(&["search", "--json", "login"]), false).unwrap_err();
        assert!(error.to_string().contains("accounts"), "{error}");
        assert!(lookup(&user_args(&["view"]), false).is_err());
    }

    #[test]
    fn json_prints_view_as_an_object_and_search_as_an_array() {
        let data = json!({
            "users": [
                {"id": "u1", "name": "Ada", "email": "ada@x.io", "role": "admin",
                 "state": "active", "me": true,
                 "accounts": [{"provider": "github", "login": "ada-l"}]}
            ],
            "hasMore": false
        });
        let fields = vec!["email".to_string(), "accounts".to_string()];
        assert_eq!(
            as_json(&command(&["view", "@me"]), &data, Some(&fields)),
            json!({"email": "ada@x.io", "accounts": [{"provider": "github", "login": "ada-l"}]})
        );
        assert_eq!(
            as_json(&command(&["search"]), &data, None),
            json!([data["users"][0].clone()])
        );
    }

    #[test]
    fn a_line_names_the_assignee_values() {
        let data = json!({
            "users": [
                {"id": "u1", "name": "Ada", "email": "ada@x.io", "role": "admin",
                 "state": "active", "me": true,
                 "accounts": [{"provider": "github", "login": "ada-l"}]},
                {"id": "u2", "name": "Bob", "email": "bob@x.io", "role": "member",
                 "state": "deactivated", "me": false, "accounts": []}
            ],
            "hasMore": true
        });
        let text = render(&command(&["search"]), &data);
        assert_eq!(
            text,
            "Ada <ada@x.io>  admin  github:ada-l  (@me, the workspace owner)  u1\n\
             Bob <bob@x.io>  member  deactivated  u2\n\
             Showing the first 2; add words or raise -L to see more."
        );
        assert_eq!(
            render(
                &command(&["search"]),
                &json!({"users": [], "hasMore": false})
            ),
            "No members match."
        );
    }
}
