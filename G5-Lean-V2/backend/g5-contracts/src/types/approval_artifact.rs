//! Restored artifact contracts from d048743f; execution and verdict settlement remain separate.
pub use super::approval_artifact_limits::*;
use super::{MergeSourceHead, APPROVAL_REVIEW_CONTEXT_MAX_JSON_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;
pub const APPROVAL_REVIEW_OUTPUT_SCHEMA_VERSION: u8 = 1;
pub const APPROVAL_REVIEW_OUTPUT_SCHEMA_VERSION_V2: u8 = 2;
pub const APPROVAL_REVIEW_RESOLUTION_SCHEMA_VERSION_V2: u8 = 2;
/// The assignment schema new formal reviews use. Schema 5 added an
/// agent-authored frontmatter `summary`; schema 6 keeps schema 5's exact
/// frontmatter and validation and runs under the contract with the ordered
/// report body (`g5.formal-agent-approval/v5`).
pub const APPROVAL_REVIEW_RESOLUTION_SCHEMA_VERSION_CURRENT: u8 = 6;
/// Maximum Unicode scalar values in a schema 5/6 agent-authored summary.
pub const AGENT_APPROVAL_SUMMARY_MAX_CHARS: usize = 250;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalMergeBaseRepository {
    pub repo_id: String,
    pub repo_name: String,
    pub git_sha: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalMergeBaseCut {
    pub repositories: Vec<ApprovalMergeBaseRepository>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalReviewResolutionV2 {
    pub schema_version: u8,
    pub resolution_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<ApprovalMergeBaseCut>,
    pub source_workspace_id: Uuid,
    pub source_heads: Vec<MergeSourceHead>,
    pub target_workspace_id: Uuid,
    pub target_heads: Vec<MergeSourceHead>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_base_heads: Option<Vec<MergeSourceHead>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repository_names: BTreeMap<String, String>,
    pub definition_id: Uuid,
    pub definition_version: i32,
    pub output_doc_id: Uuid,
    pub output_repo_id: Uuid,
    pub output_doc_path: String,
}

impl ApprovalReviewResolutionV2 {
    /// Schemas 4 to 6 freeze assignedAt and merge bases and submit through
    /// the canonical writer; earlier schemas carry neither.
    pub fn has_assignment_metadata(&self) -> bool {
        matches!(self.schema_version, 4..=6)
    }

    /// Schema 5 and 6 reports carry an agent-authored frontmatter summary;
    /// schema 6 differs from 5 only in its contract's body guidance.
    pub fn has_authored_summary(&self) -> bool {
        matches!(self.schema_version, 5 | 6)
    }

    /// Historical assignments used target HEAD; new assignments freeze the workspace base.
    pub fn comparison_heads(&self) -> &[MergeSourceHead] {
        self.workspace_base_heads
            .as_deref()
            .unwrap_or(&self.target_heads)
    }

    pub fn comparison_sha(&self, repo: &str) -> Option<&str> {
        if self.has_assignment_metadata() {
            self.merge_base
                .as_ref()?
                .repositories
                .iter()
                .find(|h| h.repo_id == repo)
                .map(|h| h.git_sha.as_str())
        } else {
            self.comparison_heads()
                .iter()
                .find(|h| h.repo_id == repo)
                .map(|h| h.head_commit.as_str())
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.has_assignment_metadata() {
            validate_utc(
                self.assigned_at
                    .as_deref()
                    .ok_or("assignedAt is required")?,
            )?;
            let bases = &self
                .merge_base
                .as_ref()
                .ok_or("mergeBase is required")?
                .repositories;
            let ids = bases
                .iter()
                .map(|h| h.repo_id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            if ids.len() != bases.len()
                || ids
                    != self
                        .source_heads
                        .iter()
                        .map(|h| h.repo_id.as_str())
                        .collect()
            {
                return Err("mergeBase membership differs from source".into());
            }
            for base in bases {
                validate_lower_hex(&base.git_sha, &[40, 64], "mergeBase.gitSha")?;
                if self.repository_names.get(&base.repo_id) != Some(&base.repo_name) {
                    return Err("mergeBase repository name differs".into());
                }
            }
        } else if self.assigned_at.is_some() || self.merge_base.is_some() {
            return Err("historical schema does not support assignment metadata".into());
        }
        match self.schema_version {
            2 if self.workspace_base_heads.is_none() && self.repository_names.is_empty() => {}
            3..=6 => {
                let bases = self
                    .workspace_base_heads
                    .as_ref()
                    .ok_or("workspaceBaseHeads is required for schemaVersion 3")?;
                validate_v2_heads(bases, "workspaceBaseHeads")?;
                let ids = |heads: &[MergeSourceHead]| {
                    heads
                        .iter()
                        .map(|h| h.repo_id.clone())
                        .collect::<std::collections::BTreeSet<_>>()
                };
                let source_ids = ids(&self.source_heads);
                if source_ids != ids(bases)
                    || source_ids != ids(&self.target_heads)
                    || source_ids != self.repository_names.keys().cloned().collect()
                    || self
                        .repository_names
                        .values()
                        .any(|name| name.trim().is_empty())
                {
                    return Err("repository names and source/base/parent vectors must cover the same repositories".into());
                }
            }
            _ => return Err("unsupported resolution schemaVersion or fields".into()),
        }
        validate_prefixed_hash(&self.resolution_id, "apr_", 64, "resolutionId")?;
        if self.definition_version < 1 {
            return Err("definitionVersion must be positive".to_string());
        }
        validate_v2_heads(&self.source_heads, "sourceHeads")?;
        validate_v2_heads(&self.target_heads, "targetHeads")?;
        if !self
            .source_heads
            .iter()
            .any(|head| Uuid::parse_str(&head.repo_id).ok() == Some(self.output_repo_id))
        {
            return Err("outputRepoId must identify a source repository".to_string());
        }
        validate_output_path_v2(&self.output_doc_path, self.source_workspace_id, None)?;
        validate_json_size(
            self,
            APPROVAL_REVIEW_CONTEXT_MAX_JSON_BYTES,
            "approval review resolution",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentApprovalOutcome {
    Approved,
    Rejected,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAgentApprovalV1 {
    pub outcome: AgentApprovalOutcome,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentApprovalFrontmatterV1 {
    schema_version: u8,
    result: AgentApprovalOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAgentApprovalV2 {
    pub outcome: AgentApprovalOutcome,
    /// The decision text: schema 5/6's exact frontmatter `summary`; for older
    /// schemas the trimmed body, which is not a summary and is never shown
    /// as one.
    pub summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentApprovalFrontmatterV2 {
    schema_version: u8,
    resolution_id: String,
    result: AgentApprovalOutcome,
    /// Schemas 5 and 6 only: the agent-authored summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    source: AgentApprovalCutV2,
    target: AgentApprovalCutV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_base: Option<AgentApprovalCutV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assigned_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    submitted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merge_base: Option<ApprovalMergeBaseCut>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentApprovalCutV2 {
    workspace_id: Uuid,
    repositories: Vec<AgentApprovalRepositoryV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentApprovalRepositoryV2 {
    repo_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    workspace_id: Uuid,
    branch_name: String,
    git_sha: String,
}

pub fn validate_agent_approval_markdown(bytes: &[u8]) -> Result<ParsedAgentApprovalV1, String> {
    let (frontmatter, body) = split_agent_approval_markdown(bytes, false)?;
    let parsed: AgentApprovalFrontmatterV1 = serde_yaml::from_str(frontmatter)
        .map_err(|error| format!("approval artifact frontmatter is invalid: {error}"))?;
    if parsed.schema_version != APPROVAL_REVIEW_OUTPUT_SCHEMA_VERSION {
        return Err("approval artifact schemaVersion must be 1".to_string());
    }
    if parsed.result == AgentApprovalOutcome::Deferred {
        return Err("agent-authored result must be approved or rejected".to_string());
    }
    let summary = body.trim();
    validate_agent_approval_summary(summary)?;
    Ok(ParsedAgentApprovalV1 {
        outcome: parsed.result,
        summary: summary.to_string(),
    })
}

pub fn validate_agent_approval_markdown_v2(
    bytes: &[u8],
    resolution: &ApprovalReviewResolutionV2,
) -> Result<ParsedAgentApprovalV2, String> {
    resolution.validate()?;
    let (frontmatter, body) =
        split_agent_approval_markdown(bytes, resolution.has_authored_summary())?;
    let parsed: AgentApprovalFrontmatterV2 = serde_yaml::from_str(frontmatter)
        .map_err(|error| format!("approval artifact frontmatter is invalid: {error}"))?;
    if parsed.schema_version != resolution.schema_version {
        return Err("approval artifact schemaVersion does not match the assignment".to_string());
    }
    if resolution.schema_version == 2 {
        // Even explicit null additions must not widen the historical closed schema.
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(frontmatter).map_err(|e| e.to_string())?;
        if yaml.get("workspaceBase").is_some()
            || ["source", "target"].iter().any(|cut| {
                yaml.get(*cut)
                    .and_then(|v| v.get("repositories"))
                    .and_then(|v| v.as_sequence())
                    .is_some_and(|repos| repos.iter().any(|repo| repo.get("repoName").is_some()))
            })
        {
            return Err("schemaVersion 2 does not support workspaceBase or repoName".into());
        }
    }
    if !resolution.has_authored_summary() {
        // Older closed schemas never had `summary`, not even as an explicit null.
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(frontmatter).map_err(|e| e.to_string())?;
        if yaml.get("summary").is_some() {
            return Err(format!(
                "schemaVersion {} does not support summary",
                resolution.schema_version
            ));
        }
    }
    if parsed.resolution_id != resolution.resolution_id {
        return Err("approval artifact resolutionId does not match the assignment".to_string());
    }
    if parsed.result == AgentApprovalOutcome::Deferred {
        return Err("agent-authored result must be approved or rejected".to_string());
    }
    validate_frontmatter_cut(
        &parsed.source,
        resolution.source_workspace_id,
        &resolution.source_heads,
        "source",
        &resolution.repository_names,
    )?;
    validate_frontmatter_cut(
        &parsed.target,
        resolution.target_workspace_id,
        &resolution.target_heads,
        "target",
        &resolution.repository_names,
    )?;
    match (&parsed.workspace_base, &resolution.workspace_base_heads) {
        (Some(cut), Some(heads)) => validate_frontmatter_cut(
            cut,
            resolution.source_workspace_id,
            heads,
            "workspaceBase",
            &resolution.repository_names,
        )?,
        (None, None) => {}
        _ => return Err("approval artifact workspaceBase does not match the assignment".into()),
    }
    if resolution.has_assignment_metadata() {
        if parsed.assigned_at != resolution.assigned_at
            || parsed.merge_base != resolution.merge_base
        {
            return Err("report assignment metadata differs from frozen assignment".into());
        }
        validate_utc(
            parsed
                .submitted_at
                .as_deref()
                .ok_or("submittedAt is required")?,
        )?;
    } else if parsed.assigned_at.is_some()
        || parsed.merge_base.is_some()
        || parsed.submitted_at.is_some()
    {
        return Err("historical report does not support schema 4 fields".into());
    }
    let body = body.trim();
    validate_agent_approval_summary(body)?;
    let summary = if resolution.has_authored_summary() {
        let summary = parsed.summary.ok_or_else(|| {
            format!(
                "summary is required for schemaVersion {}",
                resolution.schema_version
            )
        })?;
        validate_agent_approval_authored_summary(&summary)?;
        summary
    } else {
        body.to_string()
    };
    Ok(ParsedAgentApprovalV2 {
        outcome: parsed.result,
        summary,
    })
}

pub fn render_agent_approval_frontmatter_v2(
    resolution: &ApprovalReviewResolutionV2,
    result: AgentApprovalOutcome,
) -> Result<String, String> {
    resolution.validate()?;
    if result == AgentApprovalOutcome::Deferred {
        return Err("agent-authored result must be approved or rejected".to_string());
    }

    fn render_cut(
        workspace_id: Uuid,
        heads: &[MergeSourceHead],
        field: &str,
        names: &BTreeMap<String, String>,
    ) -> Result<AgentApprovalCutV2, String> {
        let repositories = heads
            .iter()
            .map(|head| {
                Ok(AgentApprovalRepositoryV2 {
                    repo_name: names.get(&head.repo_id).cloned(),
                    repo_id: Uuid::parse_str(&head.repo_id)
                        .map_err(|_| format!("{field}.repoId must be a UUID"))?,
                    workspace_id: Uuid::parse_str(&head.workspace_id)
                        .map_err(|_| format!("{field}.workspaceId must be a UUID"))?,
                    branch_name: head.branch_name.clone(),
                    git_sha: head.head_commit.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(AgentApprovalCutV2 {
            workspace_id,
            repositories,
        })
    }

    let frontmatter = AgentApprovalFrontmatterV2 {
        summary: None,
        assigned_at: resolution.assigned_at.clone(),
        submitted_at: None,
        merge_base: resolution.merge_base.clone(),
        schema_version: resolution.schema_version,
        resolution_id: resolution.resolution_id.clone(),
        result,
        source: render_cut(
            resolution.source_workspace_id,
            &resolution.source_heads,
            "source",
            &resolution.repository_names,
        )?,
        target: render_cut(
            resolution.target_workspace_id,
            &resolution.target_heads,
            "target",
            &resolution.repository_names,
        )?,
        workspace_base: resolution
            .workspace_base_heads
            .as_ref()
            .map(|heads| {
                render_cut(
                    resolution.source_workspace_id,
                    heads,
                    "workspaceBase",
                    &resolution.repository_names,
                )
            })
            .transpose()?,
    };
    serde_yaml::to_string(&frontmatter)
        .map(|yaml| yaml.trim_end().to_string())
        .map_err(|error| format!("failed to render approval artifact frontmatter: {error}"))
}

/// A schema 5/6 summary is one nonblank plain-text line of at most
/// [`AGENT_APPROVAL_SUMMARY_MAX_CHARS`] characters, stored exactly as written:
/// no surrounding whitespace or control characters.
pub fn validate_agent_approval_authored_summary(summary: &str) -> Result<(), String> {
    if summary.trim().is_empty() {
        return Err("summary must not be blank".into());
    }
    if summary.chars().count() > AGENT_APPROVAL_SUMMARY_MAX_CHARS {
        return Err(format!(
            "summary must be at most {AGENT_APPROVAL_SUMMARY_MAX_CHARS} characters"
        ));
    }
    if summary.trim() != summary {
        return Err("summary must not have leading or trailing whitespace".into());
    }
    if summary.chars().any(char::is_control) {
        return Err("summary must be one line of plain text without control characters".into());
    }
    Ok(())
}

/// Validates a report body (historically named for the clipped decision text).
pub fn validate_agent_approval_summary(summary: &str) -> Result<(), String> {
    if summary.trim().is_empty() {
        return Err("approval summary must not be blank".to_string());
    }
    if summary.len() > AGENT_APPROVAL_OUTPUT_MAX_BYTES {
        return Err(format!(
            "approval summary must be at most {AGENT_APPROVAL_OUTPUT_MAX_BYTES} bytes"
        ));
    }
    Ok(())
}

/// `quoted_summary` lets schema 5/6's top-level quoted `summary` scalar contain
/// text such as `: *` that the line guard below would otherwise read as YAML
/// alias, anchor or tag syntax; a quoted scalar can be none of those.
fn split_agent_approval_markdown(
    bytes: &[u8],
    quoted_summary: bool,
) -> Result<(&str, &str), String> {
    if bytes.len() > AGENT_APPROVAL_OUTPUT_MAX_BYTES {
        return Err(format!(
            "approval artifact must be at most {AGENT_APPROVAL_OUTPUT_MAX_BYTES} bytes"
        ));
    }
    if bytes.contains(&0) {
        return Err("approval artifact must not contain NUL".to_string());
    }
    let document = std::str::from_utf8(bytes)
        .map_err(|_| "approval artifact must be valid UTF-8".to_string())?;
    let frontmatter_and_body = document
        .strip_prefix("---\n")
        .ok_or_else(|| "approval artifact must start with YAML frontmatter".to_string())?;
    let (frontmatter, body) = frontmatter_and_body.split_once("\n---\n").ok_or_else(|| {
        "approval artifact frontmatter must have one closing delimiter".to_string()
    })?;
    if frontmatter.len() > AGENT_APPROVAL_FRONTMATTER_MAX_BYTES {
        return Err(format!(
            "approval frontmatter must be at most {AGENT_APPROVAL_FRONTMATTER_MAX_BYTES} bytes"
        ));
    }
    if frontmatter.lines().any(|line| {
        if quoted_summary && (line.starts_with("summary: '") || line.starts_with("summary: \"")) {
            return false;
        }
        let trimmed = line.trim();
        trimmed.starts_with('&')
            || trimmed.starts_with('*')
            || trimmed.starts_with('!')
            || trimmed.starts_with("<<:")
            || trimmed.contains(": &")
            || trimmed.contains(": *")
            || trimmed.contains(": !")
    }) {
        return Err(
            "approval artifact YAML aliases, anchors, tags, and merge keys are forbidden"
                .to_string(),
        );
    }
    if body.lines().any(|line| line.trim() == "---") {
        return Err("approval artifact must contain only one YAML document".to_string());
    }
    Ok((frontmatter, body))
}

fn validate_v2_heads(heads: &[MergeSourceHead], field: &str) -> Result<(), String> {
    validate_heads(heads, field, true)?;
    let mut repo_ids = HashSet::with_capacity(heads.len());
    for head in heads {
        let repo_id = Uuid::parse_str(&head.repo_id)
            .map_err(|_| format!("{field} contains invalid repoId"))?;
        if !repo_ids.insert(repo_id) {
            return Err(format!("{field} contains duplicate repoId"));
        }
    }
    Ok(())
}

fn validate_frontmatter_cut(
    cut: &AgentApprovalCutV2,
    expected_workspace_id: Uuid,
    expected_heads: &[MergeSourceHead],
    field: &str,
    names: &BTreeMap<String, String>,
) -> Result<(), String> {
    if cut.workspace_id != expected_workspace_id {
        return Err(format!(
            "approval artifact {field}.workspaceId does not match the assignment"
        ));
    }
    if cut.repositories.is_empty() {
        return Err(format!(
            "approval artifact {field}.repositories must not be empty"
        ));
    }
    let mut actual = cut.repositories.clone();
    actual.sort_by_key(|repo| repo.repo_id);
    if actual
        .windows(2)
        .any(|pair| pair[0].repo_id == pair[1].repo_id)
    {
        return Err(format!(
            "approval artifact {field}.repositories contains duplicate repoId"
        ));
    }
    for repo in &actual {
        if repo.branch_name.trim().is_empty() {
            return Err(format!(
                "approval artifact {field}.repositories contains blank branchName"
            ));
        }
        validate_lower_hex(
            &repo.git_sha,
            &[40, 64],
            &format!("approval artifact {field}.repositories.gitSha"),
        )?;
    }
    validate_v2_heads(expected_heads, field)?;
    let mut expected = expected_heads
        .iter()
        .map(|head| {
            Ok(AgentApprovalRepositoryV2 {
                repo_name: names.get(&head.repo_id).cloned(),
                repo_id: Uuid::parse_str(&head.repo_id)
                    .map_err(|_| format!("{field} contains invalid repoId"))?,
                workspace_id: Uuid::parse_str(&head.workspace_id)
                    .map_err(|_| format!("{field} contains invalid workspaceId"))?,
                branch_name: head.branch_name.clone(),
                git_sha: head.head_commit.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    expected.sort_by_key(|repo| repo.repo_id);
    if actual != expected {
        return Err(format!(
            "approval artifact {field}.repositories does not match the assignment"
        ));
    }
    Ok(())
}

fn validate_heads(
    heads: &[MergeSourceHead],
    field: &str,
    require_nonempty: bool,
) -> Result<(), String> {
    if require_nonempty && heads.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    let mut repo_ids = HashSet::with_capacity(heads.len());
    for head in heads {
        if head.repo_id.trim().is_empty()
            || head.branch_name.trim().is_empty()
            || head.workspace_id.trim().is_empty()
        {
            return Err(format!("{field} contains a blank identity field"));
        }
        if !repo_ids.insert(head.repo_id.as_str()) {
            return Err(format!("{field} contains duplicate repoId"));
        }
        Uuid::parse_str(&head.workspace_id)
            .map_err(|_| format!("{field} contains invalid workspaceId"))?;
        validate_lower_hex(&head.head_commit, &[40, 64], &format!("{field}.headCommit"))?;
    }
    Ok(())
}

pub fn validate_output_path_v2(
    path: &str,
    source_workspace_id: Uuid,
    job_id: Option<Uuid>,
) -> Result<(), String> {
    const SHAPE: &str =
        "outputDocPath must be docs/<sourceWorkspaceId>/reviews/agent-approval-<jobId>.md";
    if path.contains('\0') || path.starts_with('/') || path.contains('\\') {
        return Err("outputDocPath must be a safe relative path".to_string());
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() != 4
        || components[0] != "docs"
        || components[1] != source_workspace_id.to_string()
        || components[2] != "reviews"
        || components
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(SHAPE.to_string());
    }
    let parsed_job_id = components[3]
        .strip_prefix("agent-approval-")
        .and_then(|value| value.strip_suffix(".md"))
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| SHAPE.to_string())?;
    if job_id.is_some_and(|expected| parsed_job_id != expected) {
        return Err(SHAPE.to_string());
    }
    Ok(())
}

fn validate_json_size<T: Serialize>(value: &T, maximum: usize, field: &str) -> Result<(), String> {
    let encoded =
        serde_json::to_vec(value).map_err(|error| format!("failed to encode {field}: {error}"))?;
    if encoded.len() > maximum {
        return Err(format!("{field} must be at most {maximum} bytes"));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, lengths: &[usize], field: &str) -> Result<(), String> {
    if !lengths.contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{field} must be lowercase hexadecimal"));
    }
    Ok(())
}

pub(super) fn validate_prefixed_hash(
    value: &str,
    prefix: &str,
    hex_len: usize,
    field: &str,
) -> Result<(), String> {
    let suffix = value
        .strip_prefix(prefix)
        .ok_or_else(|| format!("{field} has an invalid prefix"))?;
    validate_lower_hex(suffix, &[hex_len], field)
}

fn validate_utc(value: &str) -> Result<(), String> {
    let time = chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| "timestamp must be RFC 3339 UTC")?;
    if time.offset().local_minus_utc() != 0 {
        return Err("timestamp must be UTC".into());
    }
    Ok(())
}
/// Schema 4 has one canonical writer. The model supplies no metadata or path.
pub fn render_agent_approval_report_v4(
    resolution: &ApprovalReviewResolutionV2,
    result: AgentApprovalOutcome,
    body: &str,
    submitted_at: &str,
) -> Result<String, String> {
    if resolution.schema_version != 4 {
        return Err("schema 4 assignment required".into());
    }
    render_canonical_report(resolution, result, None, body, submitted_at)
}

/// The canonical writer for schemas 5 and 6: schema 4 plus the agent-authored
/// `summary`, which is written exactly (after trimming) and never derived
/// from the body. Schema 6 writes the same fields; only `schemaVersion` differs.
pub fn render_agent_approval_report_v5(
    resolution: &ApprovalReviewResolutionV2,
    result: AgentApprovalOutcome,
    summary: &str,
    body: &str,
    submitted_at: &str,
) -> Result<String, String> {
    if !resolution.has_authored_summary() {
        return Err("schema 5 or 6 assignment required".into());
    }
    let summary = summary.trim();
    validate_agent_approval_authored_summary(summary)?;
    render_canonical_report(resolution, result, Some(summary), body, submitted_at)
}

fn render_canonical_report(
    resolution: &ApprovalReviewResolutionV2,
    result: AgentApprovalOutcome,
    summary: Option<&str>,
    body: &str,
    submitted_at: &str,
) -> Result<String, String> {
    validate_agent_approval_summary(body.trim())?;
    validate_utc(submitted_at)?;
    let mut frontmatter: AgentApprovalFrontmatterV2 =
        serde_yaml::from_str(&render_agent_approval_frontmatter_v2(resolution, result)?)
            .map_err(|e| e.to_string())?;
    frontmatter.summary = summary.map(str::to_string);
    frontmatter.submitted_at = Some(submitted_at.into());
    let yaml = serde_yaml::to_string(&frontmatter).map_err(|e| e.to_string())?;
    let document = format!("---\n{}---\n\n{}\n", yaml, body.trim());
    validate_agent_approval_markdown_v2(document.as_bytes(), resolution)?;
    Ok(document)
}
