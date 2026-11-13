//! File-tree and Git commit response contracts.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeEntry>>,
}

/// A rendered file tree and whether it is the whole subtree: a listing past
/// the tree's entry or byte cap carries what fit and `truncated`.
#[derive(Debug, Clone)]
pub struct FileTreeListing {
    pub entries: Vec<FileTreeEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummaryDTO {
    pub sha: String,
    pub short_sha: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: String,
}

impl From<g5_scm::CommitSummary> for CommitSummaryDTO {
    fn from(c: g5_scm::CommitSummary) -> Self {
        let committed_at = chrono::DateTime::<chrono::Utc>::from_timestamp(c.committed_at, 0)
            .unwrap_or_default()
            .to_rfc3339();
        let short_sha = c.sha.chars().take(7).collect();
        Self {
            sha: c.sha,
            short_sha,
            summary: c.summary,
            author_name: c.author_name,
            committed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistoryEntryDTO {
    pub sha: String,
    pub short_sha: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: String,
    pub parent_count: usize,
}

impl From<g5_scm::CommitHistoryEntry> for CommitHistoryEntryDTO {
    fn from(commit: g5_scm::CommitHistoryEntry) -> Self {
        let committed_at = chrono::DateTime::<chrono::Utc>::from_timestamp(commit.committed_at, 0)
            .unwrap_or_default()
            .to_rfc3339();
        Self {
            short_sha: commit.sha.chars().take(7).collect(),
            sha: commit.sha,
            summary: commit.summary,
            author_name: commit.author_name,
            committed_at,
            parent_count: commit.parent_count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistoryPageDTO {
    pub commits: Vec<CommitHistoryEntryDTO>,
    pub next_cursor: Option<String>,
}

impl From<g5_scm::CommitHistoryPage> for CommitHistoryPageDTO {
    fn from(page: g5_scm::CommitHistoryPage) -> Self {
        Self {
            commits: page.commits.into_iter().map(Into::into).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiffDTO {
    pub commit: CommitHistoryEntryDTO,
    pub comparison_base: g5_scm::CommitComparisonBase,
    pub first_parent_diff: bool,
    pub files: Vec<g5_scm::CommitDiffFileSummary>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

impl From<g5_scm::CommitDiffSummary> for CommitDiffDTO {
    fn from(diff: g5_scm::CommitDiffSummary) -> Self {
        Self {
            commit: diff.commit.into(),
            comparison_base: diff.comparison_base,
            first_parent_diff: diff.first_parent_diff,
            files: diff.files,
            total_additions: diff.total_additions,
            total_deletions: diff.total_deletions,
        }
    }
}
