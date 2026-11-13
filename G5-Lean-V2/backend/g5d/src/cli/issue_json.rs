//! `g5d issue view|search --json FIELDS`: gh's field names projected from
//! G5's records. G5's own fields are opt-in and named so they cannot be
//! mistaken for gh's; they never change a gh field's shape
//! (`g5.external-issues.cli`). `--json` alone prints G5's full record.

use g5_contracts::error::G5Error;
use serde_json::{json, Map, Value};

/// `gh issue view` fields G5 has, then G5's own.
pub const VIEW_FIELDS: &[&str] = &[
    "assignees",
    "author",
    "body",
    "closed",
    "closedAt",
    "comments",
    "createdAt",
    "labels",
    "number",
    "state",
    "stateReason",
    "title",
    "updatedAt",
    "url",
    "repository",
    "contentTrusted",
    "workspaceLink",
    "workspaceLinks",
];

/// `gh search issues` fields G5 has, then G5's own.
pub const SEARCH_FIELDS: &[&str] = &[
    "assignees",
    "author",
    "closedAt",
    "commentsCount",
    "createdAt",
    "labels",
    "number",
    "repository",
    "state",
    "title",
    "updatedAt",
    "url",
    "contentTrusted",
    "primaryWorkspace",
];

/// gh fields G5 cannot provide, and why.
fn unavailable(field: &str) -> Option<&'static str> {
    match field {
        "id" => Some("G5 does not hold GitHub's GraphQL id"),
        "body" => Some("search results carry no description; use `g5d issue view`"),
        "milestone"
        | "projectCards"
        | "projectItems"
        | "reactionGroups"
        | "isPinned"
        | "closedByPullRequestsReferences"
        | "authorAssociation"
        | "isLocked"
        | "isPullRequest" => Some("G5 does not read it from GitHub"),
        _ => None,
    }
}

/// The fields to print, or `None` for G5's full record. A field outside
/// `available` is refused, naming the ones there are.
pub fn parse(raw: Option<&str>, available: &[&str]) -> Result<Option<Vec<String>>, G5Error> {
    parse_fields(raw, available, unavailable)
}

/// [`parse`] for any command's fields: `why_unavailable` explains a known
/// field the command cannot provide.
pub fn parse_fields(
    raw: Option<&str>,
    available: &[&str],
    why_unavailable: fn(&str) -> Option<&'static str>,
) -> Result<Option<Vec<String>>, G5Error> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut fields: Vec<String> = Vec::new();
    for field in raw.split(',').map(str::trim).filter(|f| !f.is_empty()) {
        if !available.contains(&field) {
            let why = why_unavailable(field)
                .map(|why| format!(" ({why})"))
                .unwrap_or_default();
            return Err(G5Error::ValidationError {
                details: json!({
                    "field": "json",
                    "error": format!(
                        "JSON field {field:?} is not available{why}. Available fields: {}",
                        available.join(", ")
                    ),
                }),
            });
        }
        if !fields.iter().any(|existing| existing == field) {
            fields.push(field.to_owned());
        }
    }
    if fields.is_empty() {
        return Err(G5Error::ValidationError {
            details: json!({
                "field": "json",
                "error": format!("name at least one field: {}", available.join(", ")),
            }),
        });
    }
    Ok(Some(fields))
}

/// Whether `value` reads as a `--json` field list rather than an issue
/// argument that followed `--json` (`g5d issue view --json 12`).
#[must_use]
pub fn is_field_list(value: &str, available: &[&str]) -> bool {
    value
        .split(',')
        .map(str::trim)
        .any(|field| available.contains(&field) || unavailable(field).is_some())
}

/// `g5d issue view --json FIELDS` from the view response's `data`.
#[must_use]
pub fn project_view(data: &Value, fields: &[String]) -> Value {
    let issue = &data["issue"];
    let field = |name: &str| -> Value {
        match name {
            "assignees" => users(&issue["assignees"]),
            "author" => user(&issue["author"]),
            "body" => json!(issue["body"].as_str().unwrap_or_default()),
            "closed" => json!(issue["state"] == "closed"),
            "closedAt" => issue["closedAt"].clone(),
            "comments" => Value::Array(
                issue["comments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|comment| {
                        json!({
                            "author": user(&comment["author"]),
                            "body": comment["body"],
                            "createdAt": comment["createdAt"],
                            "url": comment["url"],
                        })
                    })
                    .collect(),
            ),
            "createdAt" => issue["createdAt"].clone(),
            "labels" => labels(&issue["labels"]),
            "number" => issue["locator"]["number"].clone(),
            // gh issue view prints GraphQL's upper-case enums.
            "state" => upper(&issue["state"]),
            "stateReason" => upper(&issue["stateReason"]),
            "title" => issue["title"].clone(),
            "updatedAt" => issue["updatedAt"].clone(),
            "url" => issue["canonicalUrl"].clone(),
            "repository" => repository(issue),
            "contentTrusted" => issue["contentTrusted"].clone(),
            "workspaceLink" => match data["workspaceLink"].as_object() {
                Some(link) => json!({"id": link["id"], "role": link["role"]}),
                None => Value::Null,
            },
            "workspaceLinks" => issue["links"].clone(),
            _ => Value::Null,
        }
    };
    select(fields, field)
}

/// `g5d issue search --json FIELDS`: one object per result, as gh prints.
#[must_use]
pub fn project_search(data: &Value, fields: &[String]) -> Value {
    Value::Array(
        data["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|issue| {
                select(fields, |name| match name {
                    "assignees" => users(&issue["assignees"]),
                    "author" => user(&issue["author"]),
                    "closedAt" => issue["closedAt"].clone(),
                    "commentsCount" => issue["commentCount"].clone(),
                    "createdAt" => issue["createdAt"].clone(),
                    "labels" => labels(&issue["labels"]),
                    "number" => issue["locator"]["number"].clone(),
                    // gh search issues prints REST's lower-case state.
                    "state" => issue["state"].clone(),
                    "title" => issue["title"].clone(),
                    "updatedAt" => issue["updatedAt"].clone(),
                    "url" => issue["canonicalUrl"].clone(),
                    "repository" => repository(issue),
                    "contentTrusted" => issue["contentTrusted"].clone(),
                    "primaryWorkspace" => issue["primaryWorkspace"].clone(),
                    _ => Value::Null,
                })
            })
            .collect(),
    )
}

fn select(fields: &[String], value: impl Fn(&str) -> Value) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|name| (name.clone(), value(name)))
            .collect::<Map<_, _>>(),
    )
}

fn user(value: &Value) -> Value {
    match value["login"].as_str() {
        Some(login) => json!({"login": login}),
        None => Value::Null,
    }
}

fn users(values: &Value) -> Value {
    Value::Array(
        values
            .as_array()
            .into_iter()
            .flatten()
            .map(user)
            .filter(|user| !user.is_null())
            .collect(),
    )
}

fn labels(values: &Value) -> Value {
    Value::Array(
        values
            .as_array()
            .into_iter()
            .flatten()
            .map(|label| {
                let mut shown = json!({
                    "name": label["name"],
                    "description": label["description"].as_str().unwrap_or_default(),
                    "color": label["color"],
                });
                if !label["id"].is_null() {
                    shown["id"] = label["id"].clone();
                }
                shown
            })
            .collect(),
    )
}

fn upper(value: &Value) -> Value {
    value
        .as_str()
        .map_or(Value::Null, |text| json!(text.to_ascii_uppercase()))
}

/// gh's `{name, nameWithOwner}` from the issue's `owner/name#N` key; a G5
/// issue (`ACME-12`) names its issue key.
pub(crate) fn repository(issue: &Value) -> Value {
    let key = issue["displayKey"].as_str().unwrap_or_default();
    if issue["provider"] == "g5" {
        let name = key.rsplit_once('-').map_or(key, |(name, _)| name);
        return json!({"name": name, "nameWithOwner": name});
    }
    let full = key
        .rsplit_once('#')
        .map_or(key, |(repository, _)| repository);
    let name = full.rsplit_once('/').map_or(full, |(_, name)| name);
    json!({"name": name, "nameWithOwner": full})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> Value {
        json!({
            "issue": {
                "displayKey": "acme/web#12", "locator": {"repositoryId": "1", "number": 12},
                "canonicalUrl": "https://github.com/acme/web/issues/12",
                "title": "Crash", "state": "closed", "stateReason": "completed",
                "assignees": [{"login": "ada", "avatarUrl": "x"}],
                "labels": [{"name": "bug", "color": "d73a4a", "description": ""}],
                "author": {"login": "reporter"}, "createdAt": "2026-09-01T00:00:00Z",
                "updatedAt": "2026-09-02T00:00:00Z", "closedAt": "2026-09-03T00:00:00Z",
                "commentCount": 1, "contentTrusted": false, "body": "Steps",
                "comments": [{"author": null, "body": "hi", "createdAt": "t", "url": "u"}],
                "links": [{"workspaceId": "w", "workspaceName": "fix", "role": "primary"}]
            },
            "workspaceLink": null
        })
    }

    #[test]
    fn view_fields_use_gh_names_and_shapes_and_g5_fields_only_when_named() {
        let fields = parse(
            Some("title,state,stateReason,closed,number,author,assignees,labels,url"),
            VIEW_FIELDS,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            project_view(&view(), &fields),
            json!({
                "title": "Crash", "state": "CLOSED", "stateReason": "COMPLETED", "closed": true,
                "number": 12, "author": {"login": "reporter"}, "assignees": [{"login": "ada"}],
                "labels": [{"name": "bug", "color": "d73a4a", "description": ""}],
                "url": "https://github.com/acme/web/issues/12",
            })
        );
        let fields = parse(
            Some("comments,repository,workspaceLink,workspaceLinks"),
            VIEW_FIELDS,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            project_view(&view(), &fields),
            json!({
                "comments": [{"author": null, "body": "hi", "createdAt": "t", "url": "u"}],
                "repository": {"name": "web", "nameWithOwner": "acme/web"},
                "workspaceLink": null,
                "workspaceLinks": [{"workspaceId": "w", "workspaceName": "fix", "role": "primary"}],
            })
        );
        assert_eq!(parse(None, VIEW_FIELDS).unwrap(), None);
    }

    #[test]
    fn unknown_and_unavailable_fields_are_refused_with_the_available_ones() {
        for (raw, needle) in [
            ("title,milestone", "G5 does not read it"),
            ("id", "GraphQL id"),
            ("nope", "Available fields: assignees"),
            (",", "name at least one field"),
        ] {
            let error = parse(Some(raw), VIEW_FIELDS).unwrap_err().to_string();
            assert!(error.contains(needle), "{raw}: {error}");
        }
        let error = parse(Some("body"), SEARCH_FIELDS).unwrap_err().to_string();
        assert!(error.contains("g5d issue view"), "{error}");
        assert!(is_field_list("title,url", VIEW_FIELDS));
        assert!(is_field_list("milestone", VIEW_FIELDS));
        assert!(!is_field_list("12", VIEW_FIELDS));
        assert!(!is_field_list("acme/web#12", VIEW_FIELDS));
    }

    #[test]
    fn search_fields_project_each_result() {
        let data = json!({"results": [{
            "displayKey": "acme/web#7", "locator": {"number": 7}, "state": "open",
            "title": "T", "commentCount": 3, "author": {"login": "o"},
            "canonicalUrl": "https://github.com/acme/web/issues/7",
            "primaryWorkspace": {"workspaceId": "w", "workspaceName": "fix", "visible": true}
        }]});
        let fields = parse(
            Some("number,state,commentsCount,repository,primaryWorkspace"),
            SEARCH_FIELDS,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            project_search(&data, &fields),
            json!([{
                "number": 7, "state": "open", "commentsCount": 3,
                "repository": {"name": "web", "nameWithOwner": "acme/web"},
                "primaryWorkspace": {"workspaceId": "w", "workspaceName": "fix", "visible": true},
            }])
        );
    }

    #[test]
    fn a_g5_issue_names_its_key_as_the_repository() {
        assert_eq!(
            repository(&json!({"provider": "g5", "displayKey": "ACME-12"})),
            json!({"name": "ACME", "nameWithOwner": "ACME"})
        );
    }
}
