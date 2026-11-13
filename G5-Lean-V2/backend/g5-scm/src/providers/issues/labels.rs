//! GitHub's issue labels (`g5.issue.labels.github`): a repository's label
//! catalog and the labels on its issues.
//!
//! Reads use the domain's installation token (Issues read); writes use the
//! person's user access token (Issues write) and are classified like every
//! other issue write: a refusal is final, a timeout, network failure or 5xx
//! is unknown, and no redirect is followed. GitHub addresses catalog labels
//! by name in the path, matching names without regard to case.

use serde::Deserialize;

use super::{
    interactive_get, send_issue_write, send_write, GithubIssueLabel, IssueError, WriteFailure,
    READ, READERS,
};
use crate::providers::{Payload, RepositoryClient};

/// Labels per catalog page: GitHub's maximum.
pub const LABEL_PAGE_SIZE: usize = 100;

/// A label name as one path segment. The URL builder drops `.` and `..`
/// segments, which would turn "this label" into "every label".
fn segment(name: &str) -> Result<&str, IssueError> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(IssueError::new(
            g5_contracts::error::ExternalIssueErrorKind::InvalidReference,
            "not a label name",
        ));
    }
    Ok(name)
}

#[derive(Debug, Deserialize)]
pub(super) struct RawCatalogLabel {
    #[serde(default)]
    id: Option<u64>,
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

pub(super) fn catalog_label(raw: RawCatalogLabel) -> GithubIssueLabel {
    GithubIssueLabel {
        id: raw.id,
        name: raw.name,
        color: raw
            .color
            .filter(|color| !color.trim().is_empty())
            .map(|color| color.to_ascii_lowercase()),
        description: raw.description.filter(|text| !text.trim().is_empty()),
    }
}

/// One page (1-based) of the repository's labels, in GitHub's order, and
/// whether GitHub reports another page.
pub async fn fetch_label_page(
    client: &RepositoryClient,
    page: u32,
) -> Result<(Vec<GithubIssueLabel>, bool), IssueError> {
    client.require(READ, READERS)?;
    let repository_id = client.repository_id()?.to_string();
    let response = interactive_get(
        client,
        &["repositories", &repository_id, "labels"],
        &[
            ("per_page", LABEL_PAGE_SIZE.to_string()),
            ("page", page.max(1).to_string()),
        ],
    )
    .await?;
    let more = response.has_next_page();
    let raw: Vec<RawCatalogLabel> = response.json()?;
    Ok((raw.into_iter().map(catalog_label).collect(), more))
}

/// The catalog label named `name` (without regard to case), or `None`.
pub async fn fetch_label(
    client: &RepositoryClient,
    name: &str,
) -> Result<Option<GithubIssueLabel>, IssueError> {
    client.require(READ, READERS)?;
    let name = segment(name)?;
    let repository_id = client.repository_id()?.to_string();
    let response = interactive_get(
        client,
        &["repositories", &repository_id, "labels", name],
        &[],
    )
    .await?;
    if response.status == 404 {
        return Ok(None);
    }
    let raw: RawCatalogLabel = response.json()?;
    Ok(Some(catalog_label(raw)))
}

/// What happened to a catalog write.
#[derive(Debug, Clone)]
pub enum LabelWriteOutcome {
    /// GitHub applied it; the label as it answered.
    Written(GithubIssueLabel),
    /// GitHub answered and refused; nothing changed. A create refused as
    /// `invalid_reference` (422) may name a label that already exists.
    Rejected(IssueError),
    /// No definite answer: read the catalog before trying again.
    Unknown(String),
}

fn written(response: &super::Response) -> LabelWriteOutcome {
    match response.json::<RawCatalogLabel>() {
        Ok(raw) => LabelWriteOutcome::Written(catalog_label(raw)),
        Err(error) => LabelWriteOutcome::Unknown(error.to_string()),
    }
}

fn write_failure(failure: WriteFailure) -> LabelWriteOutcome {
    match failure {
        WriteFailure::Rejected(error) => LabelWriteOutcome::Rejected(error),
        WriteFailure::Unknown(message) => LabelWriteOutcome::Unknown(message),
    }
}

/// `POST /repositories/{id}/labels`.
pub async fn create_label(
    client: &RepositoryClient,
    name: &str,
    color: Option<&str>,
    description: &str,
) -> LabelWriteOutcome {
    let mut payload = serde_json::json!({"name": name, "description": description});
    if let Some(color) = color {
        payload["color"] = color.into();
    }
    match send_issue_write(
        client,
        "create label",
        reqwest::Method::POST,
        &["labels"],
        payload,
    )
    .await
    {
        Ok(response) => written(&response),
        Err(failure) => write_failure(failure),
    }
}

/// A catalog edit; `None` keeps a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelEdit {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// `PATCH /repositories/{id}/labels/{name}`: rename or recolour the label
/// currently named `current`. Unconditional, as GitHub's write is.
pub async fn update_label(
    client: &RepositoryClient,
    current: &str,
    edit: &LabelEdit,
) -> LabelWriteOutcome {
    let current = match segment(current) {
        Ok(current) => current,
        Err(error) => return LabelWriteOutcome::Rejected(error),
    };
    let mut payload = serde_json::Map::new();
    if let Some(name) = &edit.name {
        payload.insert("new_name".into(), name.as_str().into());
    }
    if let Some(color) = &edit.color {
        payload.insert("color".into(), color.as_str().into());
    }
    if let Some(description) = &edit.description {
        payload.insert("description".into(), description.as_str().into());
    }
    match send_issue_write(
        client,
        "edit label",
        reqwest::Method::PATCH,
        &["labels", current],
        serde_json::Value::Object(payload),
    )
    .await
    {
        Ok(response) => written(&response),
        Err(failure) => write_failure(failure),
    }
}

/// What happened to a catalog delete or an issue label removal.
#[derive(Debug, Clone)]
pub enum LabelRemoveOutcome {
    /// GitHub removed it; for an issue, the labels it then has.
    Removed(Vec<GithubIssueLabel>),
    /// GitHub reports no such label (404): nothing to remove.
    Absent,
    Rejected(IssueError),
    Unknown(String),
}

/// `DELETE /repositories/{id}/labels/{name}`: GitHub removes the label from
/// every issue too.
pub async fn delete_label(client: &RepositoryClient, name: &str) -> LabelRemoveOutcome {
    let name = match segment(name) {
        Ok(name) => name,
        Err(error) => return LabelRemoveOutcome::Rejected(error),
    };
    match send_write(
        client,
        "delete label",
        reqwest::Method::DELETE,
        &["labels", name],
        Payload::Empty,
        true,
    )
    .await
    {
        Ok(Some(_)) => LabelRemoveOutcome::Removed(Vec::new()),
        Ok(None) => LabelRemoveOutcome::Absent,
        Err(WriteFailure::Rejected(error)) => LabelRemoveOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => LabelRemoveOutcome::Unknown(message),
    }
}

/// What happened when labels were added to an issue.
#[derive(Debug, Clone)]
pub enum LabelsAddOutcome {
    /// GitHub added them; the labels the issue then has.
    Added(Vec<GithubIssueLabel>),
    Rejected(IssueError),
    Unknown(String),
}

fn issue_labels(response: &super::Response) -> Result<Vec<GithubIssueLabel>, String> {
    response
        .json::<Vec<RawCatalogLabel>>()
        .map(|raw| raw.into_iter().map(catalog_label).collect())
        .map_err(|error| error.to_string())
}

/// `POST /repositories/{id}/issues/{number}/labels`: add `names` to the
/// issue's labels, keeping the others. GitHub creates a label it does not
/// know, so callers add only labels they found in the catalog.
pub async fn add_issue_labels(
    client: &RepositoryClient,
    number: u64,
    names: &[String],
) -> LabelsAddOutcome {
    let number = number.to_string();
    match send_issue_write(
        client,
        "add issue labels",
        reqwest::Method::POST,
        &["issues", &number, "labels"],
        serde_json::json!({"labels": names}),
    )
    .await
    {
        Ok(response) => match issue_labels(&response) {
            Ok(labels) => LabelsAddOutcome::Added(labels),
            Err(message) => LabelsAddOutcome::Unknown(message),
        },
        Err(WriteFailure::Rejected(error)) => LabelsAddOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => LabelsAddOutcome::Unknown(message),
    }
}

/// `DELETE /repositories/{id}/issues/{number}/labels/{name}`: remove one
/// label from the issue. GitHub answers 404 when the issue lacks it.
pub async fn remove_issue_label(
    client: &RepositoryClient,
    number: u64,
    name: &str,
) -> LabelRemoveOutcome {
    let name = match segment(name) {
        Ok(name) => name,
        Err(error) => return LabelRemoveOutcome::Rejected(error),
    };
    let number = number.to_string();
    match send_write(
        client,
        "remove issue label",
        reqwest::Method::DELETE,
        &["issues", &number, "labels", name],
        Payload::Empty,
        true,
    )
    .await
    {
        Ok(Some(response)) => match issue_labels(&response) {
            Ok(labels) => LabelRemoveOutcome::Removed(labels),
            Err(message) => LabelRemoveOutcome::Unknown(message),
        },
        Ok(None) => LabelRemoveOutcome::Absent,
        Err(WriteFailure::Rejected(error)) => LabelRemoveOutcome::Rejected(error),
        Err(WriteFailure::Unknown(message)) => LabelRemoveOutcome::Unknown(message),
    }
}
