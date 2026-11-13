//! Issue labels (`g5.issue.labels`): the shapes and validation every tracker
//! shares. The issue service validates a request here before any adapter is
//! asked; the native adapter ([`super::native_issue_labels`]) and the GitHub
//! adapter (`g5_scm` label transport) then match names and apply it
//! (`g5.issue.labels.contract`).

use g5_contracts::error::G5Error;
use serde::Serialize;
use serde_json::json;

use crate::services::git::external_issues::{ExternalIssueLabelDTO, ExternalIssueSummaryDTO};

/// Longest label name, as on GitHub.
pub const MAX_LABEL_NAME_CHARS: usize = 50;
/// Longest label description, as on GitHub.
pub const MAX_LABEL_DESCRIPTION_CHARS: usize = 100;
/// Most labels one change names (added and removed together), or one
/// creation selects.
pub const MAX_LABELS_PER_CHANGE: usize = 20;
/// Labels per catalog page: GitHub's page maximum.
pub const LABEL_PAGE_SIZE: usize = 100;
/// Most catalog pages a GitHub label is looked for by id (1,000 labels).
pub const MAX_LABEL_SCAN_PAGES: u32 = 10;

fn invalid(field: &str, code: &str, error: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: json!({"field": field, "code": code, "error": error.into()}),
    }
}

/// A label name: one trimmed, non-empty line within the limit.
pub fn validate_name(field: &str, name: &str) -> Result<String, G5Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid(field, "invalid_label", "a label name is empty"));
    }
    if name.contains(['\r', '\n']) {
        return Err(invalid(
            field,
            "invalid_label",
            "a label name must be one line",
        ));
    }
    // GitHub addresses labels by name in the path, where `.` and `..` would
    // name another resource.
    if name == "." || name == ".." {
        return Err(invalid(
            field,
            "invalid_label",
            format!("{name} is not a label name"),
        ));
    }
    if name.chars().count() > MAX_LABEL_NAME_CHARS {
        return Err(invalid(
            field,
            "invalid_label",
            format!("label names are at most {MAX_LABEL_NAME_CHARS} characters: {name}"),
        ));
    }
    Ok(name.to_string())
}

/// A colour as six hex digits, with or without `#`, normalized to lowercase
/// without `#`.
pub fn normalize_color(color: &str) -> Result<String, G5Error> {
    let bare = color.trim();
    let bare = bare.strip_prefix('#').unwrap_or(bare);
    if bare.len() != 6 || !bare.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "color",
            "invalid_color",
            format!("a colour is six hex digits, such as d73a4a: {color}"),
        ));
    }
    Ok(bare.to_ascii_lowercase())
}

/// A description: one line within the limit; empty clears it.
pub fn validate_description(description: &str) -> Result<String, G5Error> {
    let description = description.trim();
    if description.contains(['\r', '\n']) {
        return Err(invalid(
            "description",
            "invalid_label",
            "a label description must be one line",
        ));
    }
    if description.chars().count() > MAX_LABEL_DESCRIPTION_CHARS {
        return Err(invalid(
            "description",
            "invalid_label",
            format!("label descriptions are at most {MAX_LABEL_DESCRIPTION_CHARS} characters"),
        ));
    }
    Ok(description.to_string())
}

/// A new catalog label, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLabel {
    pub name: String,
    pub color: Option<String>,
    pub description: String,
}

/// Validate a new catalog label.
pub fn validate_new_label(
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
) -> Result<NewLabel, G5Error> {
    Ok(NewLabel {
        name: validate_name("name", name)?,
        color: color.map(normalize_color).transpose()?,
        description: description
            .map(validate_description)
            .transpose()?
            .unwrap_or_default(),
    })
}

/// A catalog edit; `None` keeps a field, an empty description clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelPatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// Validate a catalog edit, which must change something.
pub fn validate_patch(
    name: Option<&str>,
    color: Option<&str>,
    description: Option<&str>,
) -> Result<LabelPatch, G5Error> {
    let patch = LabelPatch {
        name: name.map(|name| validate_name("name", name)).transpose()?,
        color: color.map(normalize_color).transpose()?,
        description: description.map(validate_description).transpose()?,
    };
    if patch == LabelPatch::default() {
        return Err(invalid(
            "label",
            "no_change",
            "name a change: a new name, colour or description",
        ));
    }
    Ok(patch)
}

/// How a catalog label is named: by its opaque id, or by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelSelector {
    Id(String),
    Name(String),
}

impl LabelSelector {
    /// The label a request names: an id when given, else a name.
    pub fn from_request(id: Option<&str>, name: Option<&str>) -> Result<Self, G5Error> {
        match (
            id.map(str::trim).filter(|id| !id.is_empty()),
            name.map(str::trim).filter(|name| !name.is_empty()),
        ) {
            (Some(id), None) => Ok(Self::Id(id.to_string())),
            (None, Some(name)) => Ok(Self::Name(validate_name("label", name)?)),
            _ => Err(invalid(
                "label",
                "invalid_label",
                "name the label by its id or its name, not both",
            )),
        }
    }

    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Id(id) => format!("label {id}"),
            Self::Name(name) => format!("label {name}"),
        }
    }
}

/// Names a label change or creation selects: each validated, none twice
/// (without regard to case).
fn names(field: &str, raw: &[String]) -> Result<Vec<String>, G5Error> {
    let mut out: Vec<String> = Vec::new();
    for name in raw {
        let name = validate_name(field, name)?;
        if out
            .iter()
            .any(|seen| seen.to_lowercase() == name.to_lowercase())
        {
            return Err(invalid(
                field,
                "duplicate_label",
                format!("{name} is named twice"),
            ));
        }
        out.push(name);
    }
    Ok(out)
}

/// A validated label change: names to add and to remove.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelChange {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

impl LabelChange {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

/// Validate a change (`g5.issue.labels.contract`): at least one label, at
/// most [`MAX_LABELS_PER_CHANGE`], none named twice or both added and
/// removed. `allow_empty` lets an edit that changes other fields name none.
pub fn validate_change(
    add: &[String],
    remove: &[String],
    allow_empty: bool,
) -> Result<LabelChange, G5Error> {
    let change = LabelChange {
        add: names("addLabels", add)?,
        remove: names("removeLabels", remove)?,
    };
    if let Some(both) = change.add.iter().find(|name| {
        change
            .remove
            .iter()
            .any(|other| other.to_lowercase() == name.to_lowercase())
    }) {
        return Err(invalid(
            "labels",
            "label_conflict",
            format!("{both} is both added and removed"),
        ));
    }
    if change.add.len() + change.remove.len() > MAX_LABELS_PER_CHANGE {
        return Err(invalid(
            "labels",
            "too_many_labels",
            format!("one change names at most {MAX_LABELS_PER_CHANGE} labels"),
        ));
    }
    if change.is_empty() && !allow_empty {
        return Err(invalid(
            "labels",
            "no_change",
            "name a label to add or remove",
        ));
    }
    Ok(change)
}

/// Validate the labels a new issue selects.
pub fn validate_selection(labels: &[String]) -> Result<Vec<String>, G5Error> {
    let labels = names("labels", labels)?;
    if labels.len() > MAX_LABELS_PER_CHANGE {
        return Err(invalid(
            "labels",
            "too_many_labels",
            format!("a new issue selects at most {MAX_LABELS_PER_CHANGE} labels"),
        ));
    }
    Ok(labels)
}

/// The refusal of names the domain's catalog lacks: labels are never created
/// by naming them (`g5.issue.labels`).
#[must_use]
pub fn unknown_labels(field: &str, names: &[String], domain: &str) -> G5Error {
    invalid(
        field,
        "unknown_label",
        format!(
            "{} {} not in {domain}'s labels; add {} to the label catalog first",
            names.join(", "),
            if names.len() == 1 { "is" } else { "are" },
            if names.len() == 1 { "it" } else { "them" },
        ),
    )
}

/// Whether `labels` carries `name`, without regard to case.
#[must_use]
pub fn carries(labels: &[ExternalIssueLabelDTO], name: &str) -> bool {
    labels
        .iter()
        .any(|label| label.name.to_lowercase() == name.to_lowercase())
}

/// The refusal of a catalog name another label already has, carrying it.
#[must_use]
pub fn label_exists(existing: &ExternalIssueLabelDTO) -> G5Error {
    G5Error::ConflictDetails {
        reason: format!("a label named {} already exists", existing.name),
        details: json!({"code": "label_exists", "label": existing}),
    }
}

/// Whether an existing label is what a create asked for: a retried create
/// returns it rather than refusing.
#[must_use]
pub fn matches_new(existing: &ExternalIssueLabelDTO, new: &NewLabel) -> bool {
    existing.name == new.name
        && new
            .color
            .as_ref()
            .is_none_or(|color| existing.color.as_deref() == Some(color.as_str()))
        && existing.description.as_deref().unwrap_or_default() == new.description
}

/// One page of a domain's catalog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelPageDTO {
    pub domain_id: uuid::Uuid,
    pub display_key: String,
    pub labels: Vec<ExternalIssueLabelDTO>,
    /// The next page's number, when more labels exist.
    pub next_page: Option<u32>,
}

/// A catalog write's result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelWriteDTO {
    /// `applied`, or `unknown` (no definite answer: list the catalog before
    /// trying again).
    pub outcome: &'static str,
    /// The label as the tracker answered; for a delete, as it was.
    pub label: Option<ExternalIssueLabelDTO>,
    /// The GitHub account the write was made as; `None` for G5 issues.
    pub github_login: Option<String>,
    pub message: Option<String>,
}

/// One requested change's outcome.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelChangeOutcomeDTO {
    pub label: String,
    /// `add` or `remove`.
    pub change: &'static str,
    /// `applied`, `refused`, `unknown` or `not_attempted`.
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The result of an issue label change (`g5.issue.labels.contract`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueLabelChangeDTO {
    /// `applied` (every change), `partial` (some were refused or not
    /// attempted) or `unknown` (some outcome is unclear: read the issue
    /// before trying again).
    pub outcome: &'static str,
    pub changes: Vec<LabelChangeOutcomeDTO>,
    /// The issue as last read, with the labels then observed; `None` when
    /// that read failed.
    pub issue: Option<ExternalIssueSummaryDTO>,
    pub github_login: Option<String>,
    pub message: Option<String>,
}

impl IssueLabelChangeDTO {
    /// Every change applied, as a native change in one transaction.
    #[must_use]
    pub fn applied(change: &LabelChange, issue: ExternalIssueSummaryDTO) -> Self {
        let changes = change
            .add
            .iter()
            .map(|name| (name, "add"))
            .chain(change.remove.iter().map(|name| (name, "remove")))
            .map(|(name, kind)| LabelChangeOutcomeDTO {
                label: name.clone(),
                change: kind,
                outcome: "applied",
                message: None,
            })
            .collect();
        Self {
            outcome: "applied",
            changes,
            issue: Some(issue),
            github_login: None,
            message: None,
        }
    }

    /// The overall outcome of per-change outcomes.
    #[must_use]
    pub fn overall(changes: &[LabelChangeOutcomeDTO]) -> &'static str {
        if changes.iter().any(|change| change.outcome == "unknown") {
            "unknown"
        } else if changes.iter().all(|change| change.outcome == "applied") {
            "applied"
        } else {
            "partial"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn code(error: &G5Error) -> String {
        match error {
            G5Error::ValidationError { details } => details["code"].as_str().unwrap().to_string(),
            other => panic!("not a validation error: {other:?}"),
        }
    }

    #[test]
    fn names_colours_and_descriptions_are_validated_and_normalized() {
        assert_eq!(
            validate_name("name", "  priority:p0 ").unwrap(),
            "priority:p0"
        );
        assert_eq!(
            code(&validate_name("name", "  ").unwrap_err()),
            "invalid_label"
        );
        assert_eq!(
            code(&validate_name("name", "a\nb").unwrap_err()),
            "invalid_label"
        );
        assert!(validate_name("name", &"x".repeat(50)).is_ok());
        assert!(validate_name("name", &"x".repeat(51)).is_err());
        assert_eq!(normalize_color("#D73A4A").unwrap(), "d73a4a");
        assert_eq!(normalize_color("0e8a16").unwrap(), "0e8a16");
        for bad in ["red", "#12345", "1234567", "#gggggg", ""] {
            assert_eq!(
                code(&normalize_color(bad).unwrap_err()),
                "invalid_color",
                "{bad}"
            );
        }
        assert!(validate_description(&"d".repeat(100)).is_ok());
        assert!(validate_description(&"d".repeat(101)).is_err());
        assert_eq!(
            code(&validate_patch(None, None, None).unwrap_err()),
            "no_change"
        );
        assert_eq!(
            validate_patch(None, None, Some("")).unwrap().description,
            Some(String::new())
        );
    }

    #[test]
    fn a_change_names_each_label_once_within_the_limit() {
        let change =
            validate_change(&strings(&["bug", " wip "]), &strings(&["p1"]), false).unwrap();
        assert_eq!(change.add, strings(&["bug", "wip"]));
        assert_eq!(change.remove, strings(&["p1"]));
        assert_eq!(
            code(&validate_change(&strings(&["Bug", "bug"]), &[], false).unwrap_err()),
            "duplicate_label"
        );
        assert_eq!(
            code(&validate_change(&strings(&["bug"]), &strings(&["BUG"]), false).unwrap_err()),
            "label_conflict"
        );
        assert_eq!(
            code(&validate_change(&[], &[], false).unwrap_err()),
            "no_change"
        );
        assert!(validate_change(&[], &[], true).unwrap().is_empty());
        let many: Vec<String> = (0..21).map(|n| format!("l{n}")).collect();
        assert_eq!(
            code(&validate_change(&many, &[], false).unwrap_err()),
            "too_many_labels"
        );
        assert_eq!(
            code(&validate_selection(&many).unwrap_err()),
            "too_many_labels"
        );
    }

    #[test]
    fn a_retried_create_matches_only_the_same_label() {
        let existing = ExternalIssueLabelDTO {
            id: Some("7".to_string()),
            name: "bug".to_string(),
            color: Some("d73a4a".to_string()),
            description: Some("Broken".to_string()),
        };
        let same = validate_new_label("bug", Some("#D73A4A"), Some("Broken")).unwrap();
        assert!(matches_new(&existing, &same));
        let no_color = validate_new_label("bug", None, Some("Broken")).unwrap();
        assert!(matches_new(&existing, &no_color));
        let other = validate_new_label("bug", Some("000000"), Some("Broken")).unwrap();
        assert!(!matches_new(&existing, &other));
        let renamed_case = validate_new_label("Bug", Some("d73a4a"), Some("Broken")).unwrap();
        assert!(!matches_new(&existing, &renamed_case));
    }

    #[test]
    fn the_overall_outcome_never_hides_an_unclear_or_unapplied_change() {
        let outcome = |outcome| LabelChangeOutcomeDTO {
            label: "x".to_string(),
            change: "add",
            outcome,
            message: None,
        };
        assert_eq!(
            IssueLabelChangeDTO::overall(&[outcome("applied")]),
            "applied"
        );
        assert_eq!(
            IssueLabelChangeDTO::overall(&[outcome("applied"), outcome("refused")]),
            "partial"
        );
        assert_eq!(
            IssueLabelChangeDTO::overall(&[outcome("applied"), outcome("not_attempted")]),
            "partial"
        );
        assert_eq!(
            IssueLabelChangeDTO::overall(&[outcome("unknown"), outcome("not_attempted")]),
            "unknown"
        );
    }
}
