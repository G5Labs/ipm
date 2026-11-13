// ---------------------------------------------------------------------------
// g5-api/src/services/workspace/doc_service.rs — Document Service
//
// Manages design docs: CRUD, file references, and content resolution.
// Docs are text documents that capture cross-cutting design intent. Markdown
// remains the default and syncable format; uploaded project docs may also be
// HTML files.
// ---------------------------------------------------------------------------

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::{Arc, Weak};

use base64::Engine;
use chrono::{DateTime, Utc};
use dashmap::{mapref::entry::Entry, DashMap};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use g5_contracts::types::{
    AuthorityTarget, CommittedBindingSnapshot, RepoRole, SyncReferenceRepoVersion,
    SyncReferenceScope,
};
use g5_scm::TreeEntryType;
use g5_semantic::parsing::MarkdownAnchorExtractor;

use crate::errors::AppError;
use crate::services::doc_reference_service::{
    parse_yaml_frontmatter, resolve_document_references_batch_from_content,
    resolve_document_references_from_content, EffectiveDocumentReferences,
};
use crate::services::home_service::{HomeAttentionCursor, HomeAttentionOrder};
pub use crate::services::question_set_service::QuestionReference;
use crate::services::workspace_reader::{
    resolve_committed_snapshot, CommittedRepoReadView, RepoFileOverlay, RepoFileRead,
    RepoFileSource, RepoReadView, ResolvedConnectedBindingView,
};
use crate::services::workspace_runtime::{
    RuntimeRequest, WorkspaceRuntime, WorkspaceRuntimeHandle,
};
use crate::services::EventBus;

// ---------------------------------------------------------------------------
// Service-level DTOs
// ---------------------------------------------------------------------------

/// Metadata prepared for an attached-owner repository file mutation.
pub struct PreparedDocMutation {
    pub doc_id: Uuid,
    pub file_path: Option<String>,
    pub repo_id: Uuid,
}

/// Service-level doc list item (summary without content).
pub struct DocListItemDTO {
    pub id: String,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub document_source: Option<String>,
    pub linked_node_count: i64,
    pub source: String,
    pub scope: String,

    pub repo_id: Option<Uuid>,
    pub draft_dirty: bool,
    pub is_virtual: bool,
    pub is_future: bool,
    pub last_uplifted_at: Option<DateTime<Utc>>,
    pub last_downlifted_at: Option<DateTime<Utc>>,
    pub content_updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Service-level row for a committed doc/view/design file pending downlift sync.
pub struct PendingDocDownliftFile {
    pub id: String,
    pub path: String,
    pub target_doc_ref: String,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub head_commit: String,
    pub last_downlifted_commit_sha: Option<String>,
    pub document_changed: bool,
}

const MAX_QUESTION_PAYLOAD_BYTES: usize = 32_768;

#[derive(Debug, Clone)]
pub struct ProjectDocOpenQuestionSource {
    pub doc_id: String,
    pub display_name: String,
    pub open_question_count: i64,
    pub first_open_question_block_id: Option<String>,
    pub open_questions: Vec<ProjectDocOpenQuestion>,
    /// Stable effective Docs cut used only by organization Home ordering.
    pub actionable_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDocOpenQuestion {
    pub block_id: String,
    pub title: String,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenQuestionFenceSummary {
    count: usize,
    first_block_id: Option<String>,
    questions: Vec<ProjectDocOpenQuestion>,
    /// Every well-formed `g5-question-ref` fence, first occurrence per
    /// reference (`g5.documents.question.metrics`): counted by the set
    /// family, never here.
    references: Vec<QuestionReference>,
}

/// The two Project Doc question families of one corpus read: embedded
/// fences with their sources, and the references the documents hold, each
/// mapped to the stable ids of the documents holding it.
#[derive(Debug, Clone, Default)]
pub struct ProjectDocQuestionFamilies {
    pub project_docs: Vec<ProjectDocOpenQuestionSource>,
    pub references: BTreeMap<QuestionReference, Vec<String>>,
}

/// Parse a `g5-question-ref` fence body (the client `parseQuestionRef`
/// rule): one JSON object with exactly `path` (a question set path) and `id`
/// (a question id). Anything else references nothing.
pub fn parse_question_ref_body(body: &str) -> Option<QuestionReference> {
    let value = serde_json::from_str::<serde_json::Value>(body.trim()).ok()?;
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let path = object.get("path")?.as_str()?;
    let question_id = object.get("id")?.as_str()?;
    if !g5_contracts::types::question_set::is_question_set_path(path)
        || !g5_contracts::types::question_set::is_valid_question_id(question_id)
    {
        return None;
    }
    Some(QuestionReference {
        path: path.to_string(),
        question_id: question_id.to_string(),
    })
}

fn project_doc_question_source_display_name(path: &str, fallback: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

/// Summarize parseable, non-duplicate open `g5-question` fences using the same
/// lenient status/schema rules as the client renderer.
fn summarize_open_question_fences(content: &str) -> OpenQuestionFenceSummary {
    let normalized_content = content.replace("\r\n", "\n").replace('\r', "\n");
    let records = MarkdownAnchorExtractor::extract(&normalized_content).blocks;
    let parser = Parser::new_ext(&normalized_content, Options::all()).into_offset_iter();
    let mut seen_ids = HashSet::new();
    let mut current: Option<(usize, String, bool)> = None;
    let mut reference_body: Option<String> = None;
    let mut references = Vec::new();
    let mut fence_ordinal = 0usize;
    let mut open_count = 0usize;
    let mut first_block_id = None;
    let mut questions = Vec::new();

    for (event, range) in parser {
        if let Some(body) = reference_body.as_mut() {
            match event {
                Event::Text(text) => {
                    if body.len() + text.len() <= MAX_QUESTION_PAYLOAD_BYTES {
                        body.push_str(&text);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(reference) = reference_body
                        .take()
                        .as_deref()
                        .and_then(parse_question_ref_body)
                    {
                        if !references.contains(&reference) {
                            references.push(reference);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if language.trim().eq_ignore_ascii_case("g5-question-ref") =>
            {
                reference_body = Some(String::new());
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if language.trim().eq_ignore_ascii_case("g5-question") =>
            {
                current = Some((range.start, String::new(), false));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, payload, overflowed)) = current.as_mut() {
                    if *overflowed {
                        continue;
                    }
                    let remaining = MAX_QUESTION_PAYLOAD_BYTES - payload.len();
                    if text.len() > remaining {
                        *overflowed = true;
                        continue;
                    }
                    payload.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, payload, overflowed)) = current.as_mut() {
                    if payload.len() == MAX_QUESTION_PAYLOAD_BYTES {
                        *overflowed = true;
                    } else if !*overflowed {
                        payload.push('\n');
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let Some((fence_start, payload, overflowed)) = current.take() else {
                    continue;
                };
                fence_ordinal += 1;
                if overflowed {
                    continue;
                }
                let Ok(serde_yaml::Value::Mapping(mapping)) =
                    serde_yaml::from_str::<serde_yaml::Value>(&payload)
                else {
                    continue;
                };
                let key = |name: &str| serde_yaml::Value::String(name.to_string());
                if mapping.get(key("schema")).and_then(|value| value.as_i64()) != Some(1) {
                    continue;
                }

                let enclosing_record = records
                    .iter()
                    .find(|record| record.byte_range.contains(&fence_start));
                let fallback_id = enclosing_record
                    .map(|record| record.identifier.to_ascii_lowercase())
                    .unwrap_or_else(|| format!("question-{fence_ordinal}"));
                let question_id = mapping
                    .get(key("id"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&fallback_id)
                    .to_string();
                if !seen_ids.insert(question_id) {
                    continue;
                }
                let answered =
                    mapping.get(key("status")).and_then(|value| value.as_str()) == Some("answered");
                let open_record =
                    enclosing_record.filter(|record| record.identifier.starts_with('Q'));
                if !answered {
                    let Some(open_record) = open_record else {
                        continue;
                    };
                    let open_block_id = open_record.identifier.clone();
                    let severity = mapping
                        .get(key("severity"))
                        .and_then(|value| value.as_str())
                        .filter(|value| matches!(*value, "low" | "medium" | "high" | "critical"))
                        .map(str::to_string);
                    open_count += 1;
                    if first_block_id.is_none() {
                        first_block_id = Some(open_block_id.clone());
                    }
                    questions.push(ProjectDocOpenQuestion {
                        block_id: open_block_id,
                        title: open_record.title.clone(),
                        severity,
                    });
                }
            }
            _ => {}
        }
    }

    OpenQuestionFenceSummary {
        count: open_count,
        first_block_id,
        questions,
        references,
    }
}

#[cfg(test)]
fn count_open_question_fences(content: &str) -> usize {
    summarize_open_question_fences(content).count
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_scm::GitAuthor;
    use g5_scm::GitRepo;
    use std::fs;

    #[test]
    fn question_ref_body_parser_mirrors_the_client_rule() {
        let path = "docs/parent-ws/questions/plan.g5q.json";
        assert_eq!(
            parse_question_ref_body(&format!(r#" {{"path": "{path}", "id": "pick-0123abcd"}} "#)),
            Some(QuestionReference {
                path: path.to_string(),
                question_id: "pick-0123abcd".to_string(),
            })
        );
        for body in [
            format!(r#"{{"path": "{path}", "id": "x", "extra": 1}}"#),
            format!(r#"{{"path": "{path}"}}"#),
            r#"{"path": "docs/../questions/plan.g5q.json", "id": "x"}"#.to_string(),
            r#"{"path": "docs/ws/plan.md", "id": "x"}"#.to_string(),
            format!(r#"{{"path": "{path}", "id": "a/b"}}"#),
            r#"{"set": "plan", "id": "x"}"#.to_string(),
            "not json".to_string(),
        ] {
            assert_eq!(parse_question_ref_body(&body), None, "{body}");
        }
    }

    #[test]
    fn references_are_collected_but_never_counted_as_embedded_questions() {
        let markdown = r#"
[Q1. Legacy embedded]
```g5-question
schema: 1
id: legacy-open
status: open
prompt: Keep the legacy fence?
```

[Q2. Referenced]
```g5-question-ref
{"path": "docs/ws/questions/plan.g5q.json", "id": "pick-0123abcd"}
```

```G5-Question-Ref
{"path": "docs/ws/questions/plan.g5q.json", "id": "pick-0123abcd"}
```

```g5-question-ref
{"path": "docs/parent/questions/old.g5q.json", "id": "inherited-1"}
```
"#;
        let summary = summarize_open_question_fences(markdown);
        assert_eq!(summary.count, 1, "only the embedded fence counts");
        assert_eq!(summary.first_block_id.as_deref(), Some("Q1"));
        assert_eq!(
            summary.references,
            vec![
                QuestionReference {
                    path: "docs/ws/questions/plan.g5q.json".to_string(),
                    question_id: "pick-0123abcd".to_string(),
                },
                QuestionReference {
                    path: "docs/parent/questions/old.g5q.json".to_string(),
                    question_id: "inherited-1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn open_question_count_matches_editor_payload_semantics() {
        let markdown = r#"
[Q1. First]
```g5-question
schema: 1
id: first-question
status: open
prompt: Choose one
```

[Q2. Answered]
```G5-QUESTION
schema: 1
id: answered-question
status: answered
prompt: Already done
```

[Q3. Unknown status stays open]
```g5-question
schema: 1
id: future-status
status: waiting
prompt: Future writer
```

```g5-question
schema: 2
id: unsupported
status: open
```

```g5-question
not: [valid
```
"#;

        assert_eq!(count_open_question_fences(markdown), 2);
    }

    #[test]
    fn project_doc_question_source_uses_filename_with_extension() {
        assert_eq!(
            project_doc_question_source_display_name(
                "docs/workspace/html_export_design.md",
                "HTML Export for the Reports Page — Design Doc",
            ),
            "html_export_design.md",
        );
        assert_eq!(
            project_doc_question_source_display_name("", "Fallback name"),
            "Fallback name",
        );
    }

    #[test]
    fn open_question_count_keeps_first_duplicate_effective_id() {
        let markdown = r#"
[Q1. Duplicate]
```g5-question
schema: 1
id: shared-question
status: answered
```

```g5-question
schema: 1
id: shared-question
status: open
```

[Q2. Missing ids share their Q record fallback]
```g5-question
schema: 1
status: open
```

```g5-question
schema: 1
status: open
```
"#;

        assert_eq!(count_open_question_fences(markdown), 1);
    }

    #[test]
    fn open_question_summary_returns_first_open_q_block() {
        let markdown = r#"
## Open Questions

[Q7. First open]
```g5-question
schema: 1
id: first-open
status: open
```

[Q8. Second open]
```g5-question
schema: 1
id: second-open
status: open
```
"#;

        assert_eq!(
            summarize_open_question_fences(markdown),
            OpenQuestionFenceSummary {
                count: 2,
                first_block_id: Some("Q7".to_string()),
                questions: vec![
                    ProjectDocOpenQuestion {
                        block_id: "Q7".to_string(),
                        title: "First open".to_string(),
                        severity: None,
                    },
                    ProjectDocOpenQuestion {
                        block_id: "Q8".to_string(),
                        title: "Second open".to_string(),
                        severity: None,
                    },
                ],
                references: Vec::new(),
            }
        );
    }

    #[test]
    fn open_question_summary_preserves_file_order_and_valid_severity() {
        let markdown = r#"
## Open Questions

[Q9. Medium question first]
```g5-question
schema: 1
id: medium-first
status: open
severity: medium
```

[Q2. Critical question second]
```g5-question
schema: 1
id: critical-second
status: open
severity: critical
```

[Q3. Unknown severity last]
```g5-question
schema: 1
id: unknown-last
status: open
severity: urgent
```
"#;

        let summary = summarize_open_question_fences(markdown);
        assert_eq!(
            summary.questions,
            vec![
                ProjectDocOpenQuestion {
                    block_id: "Q9".to_string(),
                    title: "Medium question first".to_string(),
                    severity: Some("medium".to_string()),
                },
                ProjectDocOpenQuestion {
                    block_id: "Q2".to_string(),
                    title: "Critical question second".to_string(),
                    severity: Some("critical".to_string()),
                },
                ProjectDocOpenQuestion {
                    block_id: "Q3".to_string(),
                    title: "Unknown severity last".to_string(),
                    severity: None,
                },
            ]
        );
    }

    #[test]
    fn open_question_summary_uses_crlf_normalized_anchor_offsets() {
        let markdown = format!(
            "[Q1. First open]\r\n{}```g5-question\r\nschema: 1\r\nstatus: open\r\n```\r\n[Q2. Next]\r\n",
            "\r\n".repeat(32),
        );

        assert_eq!(
            summarize_open_question_fences(&markdown).first_block_id,
            Some("Q1".to_string()),
        );
    }

    #[test]
    fn open_question_summary_caps_oversized_payload_while_streaming() {
        let markdown = format!(
            "[Q1. Oversized]\n```g5-question\nschema: 1\nstatus: open\nprompt: {}\n```\n",
            "x".repeat(MAX_QUESTION_PAYLOAD_BYTES * 2),
        );

        assert_eq!(count_open_question_fences(&markdown), 0);
    }

    #[test]
    fn open_question_summary_ignores_unlabeled_question_fences() {
        let markdown = r#"
## Open Questions

```g5-question
schema: 1
id: unlabeled
status: open
```
"#;

        assert_eq!(
            summarize_open_question_fences(markdown),
            OpenQuestionFenceSummary {
                count: 0,
                first_block_id: None,
                questions: Vec::new(),
                references: Vec::new(),
            }
        );
    }

    #[test]
    fn open_question_count_ignores_duplicate_yaml_keys() {
        let markdown = r#"
```g5-question
schema: 1
id: first
id: second
status: open
```
"#;

        assert_eq!(count_open_question_fences(markdown), 0);
    }

    #[test]
    fn project_doc_source_dedup_keeps_first_effective_path() {
        let first = Uuid::new_v4().to_string();
        let second = Uuid::new_v4().to_string();
        let rows = vec![
            (first.clone(), "docs/ws/plan.md".to_string()),
            (second, "docs/ws/plan.md".to_string()),
        ];
        let mut seen_paths = BTreeSet::new();
        let retained = rows
            .into_iter()
            .filter(|(_, path)| seen_paths.insert(path.clone()))
            .map(|(id, _)| id)
            .collect::<Vec<_>>();

        assert_eq!(retained, vec![first]);
    }

    fn docs_branch(repo_id: Uuid) -> DocsRepositoryVersion {
        DocsRepositoryVersion {
            branch_name: "main".to_string(),
            repo_id,
            workspace_id: Uuid::new_v4(),
            head_commit: "abc123".to_string(),
        }
    }

    #[test]
    fn virtual_doc_id_is_workspace_repository_and_path_scoped() {
        let owner = Uuid::new_v4();
        let branch = docs_branch(owner);

        let id = DocService::virtual_doc_id(&branch, "views/jm-test.md");

        assert_eq!(
            id,
            format!(
                "overlay:{}:{owner}:dmlld3Mvam0tdGVzdC5tZA",
                branch.workspace_id
            )
        );
    }

    #[test]
    fn virtual_metadata_prefers_frontmatter_then_heading_then_file_stem() {
        let branch = docs_branch(Uuid::new_v4());

        let frontmatter = DocService::virtual_metadata_from_path(
            &branch,
            "views/worker-created.md",
            Some("---\ntitle: Worker View\ndoc_type: pm_view\n---\n# Ignored\n".to_string()),
        );
        assert_eq!(frontmatter.name, "Worker View");
        assert_eq!(frontmatter.doc_type, "pm_view");

        let heading = DocService::virtual_metadata_from_path(
            &branch,
            "docs/empty-name.md",
            Some("# Heading Name\n\nBody".to_string()),
        );
        assert_eq!(heading.name, "Heading Name");
        assert_eq!(heading.doc_type, "design");

        let empty = DocService::virtual_metadata_from_path(&branch, "views/new-view.md", None);
        assert_eq!(empty.name, "new view");
        assert_eq!(empty.doc_type, "customized_view");

        for (suffix, expected_type) in [
            ("pm_view", "pm_view"),
            ("arch_view", "arch_view"),
            ("security_view", "security_view"),
            ("performance_view", "performance_view"),
            ("test_view", "test_view"),
            ("ui_view", "ui_view"),
            ("custom_view", "customized_view"),
        ] {
            let path = format!("views/typed.{suffix}.md");
            let metadata = DocService::virtual_metadata_from_path(&branch, &path, None);
            assert_eq!(metadata.doc_type, expected_type, "path {path}");
        }
    }

    #[test]
    fn virtual_metadata_preserves_frontmatter_doc_type_verbatim() {
        let branch = docs_branch(Uuid::new_v4());
        let content = Some("---\ndoc_type: instruction\n---\n# Unsupported Type\n".to_string());

        let project_doc = DocService::virtual_metadata_from_path(
            &branch,
            "docs/workspace/instruction.md",
            content.clone(),
        );
        assert_eq!(project_doc.doc_type, "instruction");

        let view = DocService::virtual_metadata_from_path(&branch, "views/instruction.md", content);
        assert_eq!(view.doc_type, "instruction");
    }

    #[test]
    fn workspace_docs_sync_candidate_path_filter_scopes_to_project_docs_folder() {
        let workspace_id = Uuid::new_v4();
        let other_workspace_id = Uuid::new_v4();
        let ws = workspace_id.to_string();

        assert!(DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}/readme.md"),
            workspace_id
        ));
        assert!(DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}/sub/dir/preview.html"),
            workspace_id
        ));
        assert!(DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}/preview.htm"),
            workspace_id
        ));

        // Other workspaces' Project Docs folders are out of scope.
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{other_workspace_id}/readme.md"),
            workspace_id
        ));
        // Root-level and non-workspace docs/ paths are out of scope.
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "docs/readme.md",
            workspace_id
        ));
        assert!(DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}/session.jsonl"),
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "docs/preview.html",
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "docs/session.jsonl",
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "docs/experiments/notes.md",
            workspace_id
        ));
        // Prefix must match with the trailing slash: `docs/<ws>.md` is a file.
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}.md"),
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            &format!("views/{ws}/preview.htm"),
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "designs/architecture.md",
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "src/preview.html",
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            &format!("docs/{ws}/data.json"),
            workspace_id
        ));
        assert!(!DocService::is_workspace_docs_sync_candidate_path(
            "views/mockup.pdf",
            workspace_id
        ));
    }

    #[test]
    fn workspace_docs_sync_filter_excludes_reserved_workflow_subtree_and_legacy_paths() {
        let workspace_id = Uuid::new_v4();
        let root = format!("docs/{workspace_id}");
        let run_folder = "sync-auth--aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

        for path in [
            format!("{root}/sync/{run_folder}/review/code_review.md"),
            format!("{root}/sync/{run_folder}/merge/7ba13cc6_merge_report.md"),
            format!("{root}/sync/{run_folder}/future-report.jsonl"),
            format!("{root}/code_review.md"),
            format!("{root}/merge/7ba13cc6_merge_report.md"),
        ] {
            assert!(!DocService::is_workspace_docs_sync_candidate_path(
                &path,
                workspace_id,
            ));
        }

        for path in [
            format!("{root}/implementation.md"),
            format!("{root}/sync-notes.md"),
            format!("{root}/archive/run_merge_report.md"),
            format!("{root}/merge/report.md"),
            format!("{root}/merge/nested/run_merge_report.md"),
        ] {
            assert!(DocService::is_workspace_docs_sync_candidate_path(
                &path,
                workspace_id,
            ));
        }
    }

    #[test]
    fn docs_surface_path_filter_accepts_explicit_html_project_docs() {
        assert!(DocService::is_docs_surface_path("docs/readme.md"));
        assert!(DocService::is_docs_surface_path("docs/preview.html"));
        assert!(DocService::is_docs_surface_path("docs/preview.htm"));
        assert!(DocService::is_docs_surface_path("views/preview.html"));
        assert!(DocService::is_docs_surface_path("designs/preview.html"));
        assert!(DocService::is_html_project_doc_path("designs/preview.html"));
        assert!(!DocService::is_docs_surface_path("src/preview.html"));
        assert!(!DocService::is_docs_surface_path("docs/not-supported.txt"));
    }

    /// Managed explainer output must be a first-class docs-surface path.
    ///
    /// Regression guard: `explainers/` was missing from the surface prefixes, so
    /// generated `explainers/<slug>-<stamp>.html` documents were written to the
    /// docs repo and auto-merged but filtered out of `list_docs` — the
    /// Explainers sidebar section stayed empty and the document could not be
    /// opened by path.
    #[test]
    fn docs_surface_path_filter_accepts_managed_explainer_html() {
        assert!(DocService::is_docs_surface_path(
            "explainers/tenant-isolation-2026-08-07-0247.html"
        ));
        assert!(DocService::is_html_project_doc_path(
            "explainers/tenant-isolation-2026-08-07-0247.html"
        ));
        // The managed root is `explainers/` specifically. The singular
        // `explainer/` is a lookalike, not a second supported root, and a
        // nested occurrence anywhere else stays off the docs surface.
        assert!(!DocService::is_docs_surface_path(
            "explainer/tenant-isolation.html"
        ));
        assert!(!DocService::is_docs_surface_path(
            "src/explainers/nope.html"
        ));
        // Extension rules are unchanged for the new root.
        assert!(!DocService::is_docs_surface_path("explainers/notes.txt"));
    }

    /// Every docs-surface root must be admitted by the prefix predicate.
    ///
    /// Pins the invariant that broke explainers: the roots the committed-tree
    /// walks enumerate and the roots `is_docs_surface_path` admits are the
    /// same list. When they drifted, `explainers/x.html` passed path validation
    /// (so `get_doc_by_path` worked) but was absent from `committed_files`, so
    /// `list_docs` dropped every explainer and the sidebar section was empty.
    #[test]
    fn docs_surface_roots_and_prefix_predicate_agree() {
        assert!(
            DocService::DOCS_SURFACE_ROOTS.contains(&"explainers"),
            "explainers/ is the managed Create HTML Explainer root"
        );
        for root in DocService::DOCS_SURFACE_ROOTS {
            assert!(
                DocService::is_docs_surface_path(&format!("{root}/sample.md")),
                "root {root} is walked but not admitted by is_docs_surface_path"
            );
        }
    }

    #[test]
    fn docs_surface_path_filter_accepts_jsonl_project_docs() {
        assert!(DocService::is_docs_surface_path("docs/session.jsonl"));
        assert!(DocService::is_docs_surface_path("views/session.jsonl"));
        assert!(DocService::is_docs_surface_path("designs/session.JSONL"));
        assert!(!DocService::is_docs_surface_path("src/session.jsonl"));
        assert!(!DocService::is_docs_surface_path("docs/records.json"));
    }

    #[test]
    fn create_doc_normalization_preserves_html_upload_extensions() {
        assert_eq!(
            DocService::normalize_doc_name("preview.HTML"),
            "preview.html"
        );
        assert_eq!(
            DocService::normalize_doc_file_path(
                "preview.html",
                Some("docs/workspace/preview.html")
            ),
            "docs/workspace/preview.html"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("preview.htm", Some("preview.HTM")),
            "docs/preview.htm"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("preview.html", None),
            "docs/preview.html"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("report.html", Some("docs/report.md")),
            "docs/report.html"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("report.htm", Some("docs/report.html.md")),
            "docs/report.htm"
        );
    }

    #[test]
    fn create_doc_normalization_preserves_jsonl_upload_extensions() {
        assert_eq!(
            DocService::normalize_doc_name("session.JSONL"),
            "session.jsonl"
        );
        assert_eq!(
            DocService::normalize_doc_file_path(
                "session.jsonl",
                Some("docs/workspace/session.jsonl")
            ),
            "docs/workspace/session.jsonl"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("session.jsonl", None),
            "docs/session.jsonl"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("transcript.jsonl", Some("docs/transcript.md")),
            "docs/transcript.jsonl"
        );
    }

    #[test]
    fn create_doc_normalization_keeps_markdown_defaults() {
        assert_eq!(
            DocService::normalize_doc_name("secondary_index.md.md"),
            "secondary_index.md"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("secondary_index.md", Some("docs/ws/a.md.md")),
            "docs/ws/a.md"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("notes.md", Some("docs/ws/notes.txt")),
            "docs/ws/notes.txt.md"
        );
        assert_eq!(
            DocService::normalize_doc_file_path("My Notes", None),
            "docs/my-notes.md"
        );
    }

    #[tokio::test]
    async fn committed_doc_files_walks_git_docs_and_views_at_head() {
        let temp = tempfile::tempdir().unwrap();
        let storage = TenantRepoStorage::local_for_test(temp.path().join("repos"));
        let org_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();
        let repo_path = storage.test_path(org_id, repo_id);

        fs::create_dir_all(repo_path.join("docs/nested")).unwrap();
        fs::create_dir_all(repo_path.join("views")).unwrap();
        fs::create_dir_all(repo_path.join("designs")).unwrap();
        git2::Repository::init(&repo_path).unwrap();
        fs::write(repo_path.join("docs/plan.md"), "---\ntitle: Plan\n---\n").unwrap();
        fs::write(repo_path.join("docs/nested/deep.md"), "# Deep\n").unwrap();
        fs::write(
            repo_path.join("docs/nested/preview.html"),
            "<h1>Preview</h1>\n",
        )
        .unwrap();
        fs::write(repo_path.join("views/view.md"), "# View\n").unwrap();
        fs::write(repo_path.join("views/view-preview.htm"), "<h1>View</h1>\n").unwrap();
        fs::write(repo_path.join("designs/design.md"), "# Design\n").unwrap();
        fs::write(
            repo_path.join("designs/design-preview.html"),
            "<h1>Design</h1>\n",
        )
        .unwrap();
        fs::write(repo_path.join("README.md"), "# Readme\n").unwrap();

        let repo = GitRepo::open(&repo_path).unwrap();
        repo.add_paths([
            "docs/plan.md",
            "docs/nested/deep.md",
            "docs/nested/preview.html",
            "views/view.md",
            "views/view-preview.htm",
            "designs/design.md",
            "designs/design-preview.html",
            "README.md",
        ])
        .unwrap();
        let head = repo
            .commit(
                "seed docs",
                &GitAuthor {
                    name: "Test User".to_string(),
                    email: "test@example.com".to_string(),
                },
            )
            .unwrap();

        let service = DocService::new(storage);
        let workspace_id = Uuid::new_v4();
        let snapshot = g5_contracts::types::CommittedBindingSnapshot {
            target: AuthorityTarget::Workspace { workspace_id },
            org_id,
            workspace_id,
            snapshot_at: chrono::Utc::now(),
            source: g5_contracts::types::DurableSnapshotSource::Committed,
            repos: vec![g5_contracts::types::RepoBindingVersion {
                workspace_id,
                repo_id,
                repo_name: "docs".to_string(),
                repo_slug: "docs".to_string(),
                repo_role: g5_contracts::types::RepoRole::Docs,

                branch_name: "main".to_string(),
                tracking_mode: "pinned".to_string(),
                remote_branch: None,
                canonical_ref: "refs/heads/main".to_string(),
                head_commit: head,
                root_path: "docs".to_string(),
            }],
        };
        let repo_view = RepoReadView::from_committed_snapshot(
            &crate::services::git::repo_store::TestOrgScope(org_id),
            &snapshot,
            repo_id,
        )
        .unwrap();
        let files = service
            .committed_doc_files(repo_view.committed_view())
            .await
            .expect("read committed docs corpus");

        assert_eq!(
            files
                .get("docs/plan.md")
                .and_then(|content| content.as_deref()),
            Some("---\ntitle: Plan\n---\n")
        );
        assert!(files.contains_key("docs/nested/deep.md"));
        assert!(files.contains_key("docs/nested/preview.html"));
        assert!(files.contains_key("views/view.md"));
        assert!(files.contains_key("views/view-preview.htm"));
        assert!(files.contains_key("designs/design.md"));
        assert!(files.contains_key("designs/design-preview.html"));
        assert!(!files.contains_key("README.md"));
    }
}

/// Service-level doc detail with full content and canonical component references.
pub struct DocDetailDTO {
    pub id: String,
    pub workspace_id: Uuid,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub document_source: Option<String>,
    pub content: Option<String>,
    /// Canonical repo-qualified component identities from the captured cut.
    pub linked_component_ids: Vec<String>,
    /// Immutable exact-cut evidence used when this document starts a sync.
    pub reference_scope: Option<SyncReferenceScope>,
    pub source: String,
    pub scope: String,

    pub repo_id: Option<Uuid>,
    pub draft_dirty: bool,
    pub is_virtual: bool,
    pub last_uplifted_at: Option<DateTime<Utc>>,
    pub last_downlifted_at: Option<DateTime<Utc>>,
    pub content_updated_at: Option<DateTime<Utc>>,
    pub content_hash: Option<String>,
    pub last_uplifted_commit_sha: Option<String>,
    pub last_downlifted_commit_sha: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request DTO for creating a doc (service-level).
pub struct CreateDocRequest {
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub content: Option<String>,
}

/// Service-level summary for view-only listings.
pub struct ViewListItemDTO {
    pub id: String,
    pub name: String,
    pub doc_type: String,
    pub file_path: Option<String>,
    pub is_future: bool,
}

/// Service-level wrapper for a single view detail.
pub struct ViewDetailDTO {
    pub doc: DocDetailDTO,
}

#[derive(Debug, sqlx::FromRow)]
struct DocMetadataRow {
    id: Uuid,
    workspace_id: Uuid,

    name: String,
    doc_type: String,
    file_path: Option<String>,
    document_source: Option<String>,
    last_uplifted_at: Option<DateTime<Utc>>,
    last_downlifted_at: Option<DateTime<Utc>>,
    content_updated_at: Option<DateTime<Utc>>,
    content: Option<String>,
    content_hash: Option<String>,
    last_uplifted_commit_sha: Option<String>,
    last_downlifted_commit_sha: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DocsRepositoryVersion {
    branch_name: String,
    repo_id: Uuid,
    workspace_id: Uuid,
    head_commit: String,
}

#[derive(Debug, sqlx::FromRow)]
struct DocumentSourceRow {
    id: Uuid,
    file_path: String,
    document_source: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct DocsRepoRead {
    branch: DocsRepositoryVersion,
    view: RepoReadView,
    snapshot: CommittedBindingSnapshot,
    repo_views: BTreeMap<Uuid, RepoReadView>,
}

/// Opaque, transaction-detached input for the current Docs list.
#[derive(Clone)]
pub struct CapturedDocsList {
    /// The store narrowed to the capturing transaction's scope
    /// (`g5.repo.storage-tenancy`), for writes that follow the capture.
    repos: ScopedRepoStorage,
    docs_read: Option<DocsRepoRead>,
    docs_cut_at: DateTime<Utc>,
    document_sources_by_path: BTreeMap<String, String>,
}

/// The attached owner's live docs surface (design v9 §9.1): which
/// docs-surface files exist in the running VM's docs checkout right now, and
/// the content of those the committed cut does not carry. Merged over the
/// committed list so an uncommitted, unflushed Project Doc lists as a draft
/// and a live-deleted one disappears.
#[derive(Debug, Default, Clone)]
pub struct LiveDocsOverlay {
    pub present: BTreeSet<String>,
    pub contents: BTreeMap<String, String>,
}

impl LiveDocsOverlay {
    /// Live content of `path` when the overlay read it (uncommitted files and
    /// the workspace's own Project Docs); `None` means the committed cut
    /// stands for that file.
    pub fn content(&self, path: &str) -> Option<&str> {
        self.contents.get(path).map(String::as_str)
    }
}

/// What the attached owner's running checkout holds at one docs-surface path
/// (design v9 §9.1). `Unavailable` means the caller is not the attached owner
/// of a running workspace, or the live read failed, and the committed cut is
/// served instead; readers are never served VM state.
#[derive(Debug, Clone)]
pub enum LiveDocRead {
    Unavailable,
    Missing,
    Content(String),
}

impl CapturedDocsList {
    pub(crate) fn repos(&self) -> &ScopedRepoStorage {
        &self.repos
    }

    pub(crate) fn docs_cut_at(&self) -> DateTime<Utc> {
        self.docs_cut_at
    }

    /// The docs repository this capture reads, when the workspace has one.
    pub fn docs_repo_id(&self) -> Option<Uuid> {
        self.docs_read.as_ref().map(|read| read.branch.repo_id)
    }

    /// Durable source metadata for the exact docs-repository cut represented
    /// by this capture. An individual doc's `source` describes projection
    /// origin, so list callers expose snapshot provenance separately.
    pub fn provenance(&self) -> Option<(String, String, DateTime<Utc>)> {
        self.docs_read.as_ref().map(|read| {
            (
                read.view.snapshot_source().as_str().to_string(),
                read.view.repo.head_commit.clone(),
                read.view.snapshot_at(),
            )
        })
    }
}

fn docs_repo_id_from_snapshot(
    snapshot: &CommittedBindingSnapshot,
) -> Result<Option<Uuid>, AppError> {
    let mut docs_repo_ids = snapshot
        .repos
        .iter()
        .filter(|repo| repo.repo_role == RepoRole::Docs)
        .map(|repo| repo.repo_id);
    let docs_repo_id = docs_repo_ids.next();
    if docs_repo_ids.next().is_some() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Authoritative workspace cut contains more than one docs repository",
                "target": snapshot.target,
            }),
        }));
    }
    Ok(docs_repo_id)
}

fn is_placeholder_head_commit(commit: &str) -> bool {
    let trimmed = commit.trim();
    trimmed.is_empty() || trimmed == "no-changes" || trimmed.chars().all(|c| c == '0')
}

#[derive(Clone)]
struct VirtualDocMetadata {
    id: String,
    name: String,
    doc_type: String,
    file_path: String,
}

// ---------------------------------------------------------------------------
// DocService
// ---------------------------------------------------------------------------

/// Document management service.
///
/// Handles document creation, metadata and captured file-backed reads.
/// Doc content is stored in git via `file_path` — not in Postgres.
pub struct DocService {
    repo_storage: TenantRepoStorage,
    create_request_gates: Arc<DashMap<Uuid, Weak<Mutex<()>>>>,
}

pub struct CreateDocRequestGateGuard {
    registry: Arc<DashMap<Uuid, Weak<Mutex<()>>>>,
    workspace_id: Uuid,
    gate: Arc<Mutex<()>>,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Drop for CreateDocRequestGateGuard {
    fn drop(&mut self) {
        self.guard.take();

        if let Entry::Occupied(entry) = self.registry.entry(self.workspace_id) {
            let registered_gate = entry.get();
            if Weak::ptr_eq(registered_gate, &Arc::downgrade(&self.gate))
                && Arc::strong_count(&self.gate) == 1
            {
                entry.remove();
            }
        }
    }
}

impl DocService {
    const INTERNAL_WORKSPACE_PREFIX: &'static str = "#!internal-workspace-";

    pub fn new(repo_storage: TenantRepoStorage) -> Self {
        Self {
            repo_storage,
            create_request_gates: Arc::new(DashMap::new()),
        }
    }

    pub fn new_with_branch_reconciliation(
        repo_storage: TenantRepoStorage,
        _system_pool: PgPool,
        _event_bus: EventBus,
    ) -> Self {
        Self::new(repo_storage)
    }

    pub async fn acquire_create_doc_request_gate(
        &self,
        workspace_id: Uuid,
    ) -> CreateDocRequestGateGuard {
        let gate = match self.create_request_gates.entry(workspace_id) {
            Entry::Occupied(mut entry) => match entry.get().upgrade() {
                Some(gate) => gate,
                None => {
                    let gate = Arc::new(Mutex::new(()));
                    entry.insert(Arc::downgrade(&gate));
                    gate
                }
            },
            Entry::Vacant(entry) => {
                let gate = Arc::new(Mutex::new(()));
                entry.insert(Arc::downgrade(&gate));
                gate
            }
        };
        let guard = Arc::clone(&gate).lock_owned().await;

        CreateDocRequestGateGuard {
            registry: Arc::clone(&self.create_request_gates),
            workspace_id,
            gate,
            guard: Some(guard),
        }
    }

    const VIEW_DOC_TYPES: &'static [&'static str] = &[
        "view",
        "pm_view",
        "arch_view",
        "security_view",
        "performance_view",
        "test_view",
        "ui_view",
        "customized_view",
    ];

    fn is_view_doc_type(doc_type: &str) -> bool {
        Self::VIEW_DOC_TYPES.contains(&doc_type)
    }

    fn reject_retired_test_view_write(doc_type: &str) -> Result<(), AppError> {
        if doc_type == "test_view" {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({
                        "docType": "test-view authoring is retired; use test_contract ontology files"
                    }),
                },
            ));
        }
        Ok(())
    }

    /// Validates the vocabulary accepted from direct API document authoring.
    ///
    /// Committed Git frontmatter is historical source metadata and deliberately
    /// bypasses this validation in `upsert_committed_doc_projection`.
    /// Persona-view doc types. Retained rows of these types stay readable,
    /// syncable, and deletable, but the generic docs surface can no longer
    /// CREATE one or CONVERT a doc into one (Create HTML Explainer cutover):
    /// leaving those ingresses open would keep Create View functionally alive
    /// behind `POST /docs` / `PUT /docs/:id` after its dedicated route was
    /// retired.
    pub(crate) const PERSONA_VIEW_DOC_TYPES: &'static [&'static str] = &[
        "view",
        "pm_view",
        "arch_view",
        "security_view",
        "performance_view",
        "ui_view",
        "customized_view",
    ];

    fn is_persona_view_doc_type(doc_type: &str) -> bool {
        Self::PERSONA_VIEW_DOC_TYPES.contains(&doc_type)
    }

    /// Doc types the generic direct-create/update surface may WRITE.
    /// Historical persona-view and explainer rows are not writable through
    /// the generic direct-create/update surface.
    const VALID_DIRECT_DOC_TYPES: &'static [&'static str] =
        &["spec", "design", "api", "guide", "adr", "note"];

    fn validate_direct_doc_type(doc_type: &str) -> Result<(), AppError> {
        if Self::VALID_DIRECT_DOC_TYPES.contains(&doc_type) {
            return Ok(());
        }

        if Self::is_persona_view_doc_type(doc_type) {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({
                        "docType": format!(
                            "Persona-view creation is retired; '{doc_type}' docs can no \
                             longer be created or converted to. Use Create Explainer \
                             to start an agent conversation instead."
                        )
                    }),
                },
            ));
        }

        Err(AppError::from_error(
            g5_contracts::error::G5Error::ValidationError {
                details: serde_json::json!({
                    "docType": format!("Invalid doc_type '{doc_type}'")
                }),
            },
        ))
    }

    fn reject_retired_test_view_path(file_path: &str) -> Result<(), AppError> {
        if file_path.ends_with(".test_view.md") || file_path.starts_with("views/test_views/") {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({
                        "filePath": "test-view authoring is retired; use test_contract ontology files"
                    }),
                },
            ));
        }
        Ok(())
    }

    fn accepted_project_doc_extension(value: &str) -> Option<&'static str> {
        let lower = value.trim().to_ascii_lowercase();
        [".html", ".htm", ".md", ".jsonl"]
            .into_iter()
            .find(|extension| lower.ends_with(extension))
    }

    fn project_doc_extension(value: &str) -> &'static str {
        Self::accepted_project_doc_extension(value).unwrap_or(".md")
    }

    fn strip_repeated_suffix_case_insensitive<'a>(mut value: &'a str, suffix: &str) -> &'a str {
        loop {
            if value.len() < suffix.len() {
                return value;
            }
            let suffix_start = value.len() - suffix.len();
            let Some(tail) = value.get(suffix_start..) else {
                return value;
            };
            if !tail.eq_ignore_ascii_case(suffix) {
                return value;
            }
            let Some(stripped) = value.get(..suffix_start) else {
                return value;
            };
            value = stripped;
        }
    }

    fn strip_project_doc_extension<'a>(value: &'a str, extension: &str) -> &'a str {
        if extension == ".md" {
            return Self::strip_repeated_suffix_case_insensitive(value, ".md");
        }

        if value.len() < extension.len() {
            return value;
        }
        let suffix_start = value.len() - extension.len();
        let Some(tail) = value.get(suffix_start..) else {
            return value;
        };
        if !tail.eq_ignore_ascii_case(extension) {
            return value;
        }
        value.get(..suffix_start).unwrap_or(value)
    }

    fn strip_any_project_doc_extension(mut value: &str) -> &str {
        while let Some(extension) = Self::accepted_project_doc_extension(value) {
            let stripped = Self::strip_project_doc_extension(value, extension);
            if stripped == value {
                break;
            }
            value = stripped;
        }
        value
    }

    fn slugify_doc_name(name: &str) -> String {
        let trimmed = name.trim();
        let extension = Self::project_doc_extension(trimmed);
        let stem = Self::strip_project_doc_extension(trimmed, extension);
        let mut slug = String::with_capacity(trimmed.len());
        let mut prev_dash = false;
        for ch in stem.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash {
                slug.push('-');
                prev_dash = true;
            }
        }
        let slug = slug.trim_matches('-');
        if slug.is_empty() {
            "untitled-doc".to_string()
        } else {
            slug.to_string()
        }
    }

    fn normalize_doc_name(name: &str) -> String {
        let trimmed = name.trim();
        let extension = Self::project_doc_extension(trimmed);
        let stem = Self::strip_project_doc_extension(trimmed, extension);
        if stem.is_empty() {
            format!("untitled-doc{extension}")
        } else {
            format!("{stem}{extension}")
        }
    }

    fn normalize_doc_file_path(name: &str, file_path: Option<&str>) -> String {
        let cleaned = file_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| {
                let segments: Vec<&str> = path
                    .split('/')
                    .filter(|segment| {
                        !segment.is_empty() && !segment.starts_with(Self::INTERNAL_WORKSPACE_PREFIX)
                    })
                    .collect();
                segments.join("/")
            })
            .filter(|path| !path.is_empty());

        let extension = Self::project_doc_extension(name);
        let base_path = cleaned.unwrap_or_else(|| format!("docs/{}", Self::slugify_doc_name(name)));
        let base_without_extension = Self::strip_any_project_doc_extension(&base_path);
        let normalized_base =
            if base_without_extension.is_empty() || base_without_extension.ends_with('/') {
                format!("docs/{}", Self::slugify_doc_name(name))
            } else {
                base_without_extension.to_string()
            };
        // A path already rooted at a docs-surface root keeps its root; anything
        // else is placed under `docs/`. Driven by the shared root list so
        // `explainers/` behaves exactly like `views/` here.
        if Self::is_docs_surface_prefix(&normalized_base) {
            format!("{normalized_base}{extension}")
        } else {
            format!("docs/{normalized_base}{extension}")
        }
    }

    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn extract_frontmatter_string_list(content: &str, keys: &[&str]) -> Vec<String> {
        let Some(frontmatter) = parse_yaml_frontmatter(content) else {
            return Vec::new();
        };
        let Some(mapping) = frontmatter.as_mapping() else {
            return Vec::new();
        };

        for key_name in keys {
            let key = serde_yaml::Value::String((*key_name).to_string());
            if let Some(seq) = mapping.get(&key).and_then(|value| value.as_sequence()) {
                return seq
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect();
            }
        }

        Vec::new()
    }

    fn linked_doc_paths_projection(content: &str) -> Option<serde_json::Value> {
        let linked_doc_paths =
            Self::extract_frontmatter_string_list(content, &["linked_doc_paths"]);
        if linked_doc_paths.is_empty() {
            None
        } else {
            serde_json::to_value(linked_doc_paths).ok()
        }
    }

    async fn resolve_docs_branch(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Option<DocsRepositoryVersion>, AppError> {
        let snapshot = resolve_committed_snapshot(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
        )
        .await
        .map_err(AppError::from_error)?;
        let Some(docs_repo_id) = docs_repo_id_from_snapshot(&snapshot)? else {
            return Ok(None);
        };
        let repo = snapshot
            .repo(docs_repo_id)
            .map_err(|message| {
                AppError::from_error(g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({
                        "workspace_id": workspace_id,
                        "repo_id": docs_repo_id,
                        "message": message,
                    }),
                })
            })?
            .clone();

        Ok(Some(DocsRepositoryVersion {
            branch_name: repo.branch_name,
            repo_id: repo.repo_id,
            workspace_id: snapshot.workspace_id,
            head_commit: repo.head_commit,
        }))
    }

    async fn resolve_docs_branch_for_write(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Option<DocsRepositoryVersion>, AppError> {
        let mut members = crate::services::workspace_repositories::load_workspace_repositories(
            db.conn(),
            workspace_id,
            false,
        )
        .await?
        .into_iter()
        .filter(|repo| repo.role == "docs");
        let Some(member) = members.next() else {
            return Ok(None);
        };
        if members.next().is_some() {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "workspace has multiple docs repositories".into(),
            }));
        }
        let head = crate::services::workspace_reader::read_branch_head_oid(
            &self.repo_storage.scoped(&*db),
            member.org_id,
            member.repo_id,
            &member.branch_name,
        )
        .await?;
        let head_commit = match head {
            Some(head) => head,
            None if member.is_base => crate::services::branch_commit::ZERO_GIT_SHA.to_string(),
            None => {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: "working docs repository ref is missing".into(),
                }))
            }
        };
        Ok(Some(DocsRepositoryVersion {
            workspace_id,
            repo_id: member.repo_id,
            branch_name: member.branch_name,
            head_commit,
        }))
    }

    async fn require_docs_branch_for_write(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<DocsRepositoryVersion, AppError> {
        self.resolve_docs_branch_for_write(db, workspace_id)
            .await?
            .ok_or_else(|| {
                AppError::from_error(g5_contracts::error::G5Error::NotFound {
                    resource: format!("workspace/{workspace_id}/docs-branch"),
                })
            })
    }

    /// Resolve the workspace authority's docs member for an update. Readable
    /// materialized members use the complete committed cut; a fresh workspace
    /// may instead expose the same typed member as unborn until the write
    /// materializes it.
    async fn docs_branch_for_update(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<DocsRepositoryVersion, AppError> {
        match self.resolve_docs_branch(db, workspace_id).await? {
            Some(branch) => Ok(branch),
            None => self.require_docs_branch_for_write(db, workspace_id).await,
        }
    }

    fn require_doc_file_path(doc_id: Uuid, file_path: Option<&str>) -> Result<&str, AppError> {
        file_path.ok_or_else(|| {
            AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: format!("Doc {doc_id} is missing the required file_path"),
            })
        })
    }

    /// Isolated Project Doc sync candidates are scoped to the current
    /// workspace's dedicated Project Docs folder `docs/<workspace_id>/`
    /// (`workspace_service` D16 convention: lowercase hyphenated UUID, literal
    /// trailing slash). The reserved `sync/` subtree and legacy workflow-report
    /// paths are readable Project Docs but never Sync targets. Other workspaces'
    /// folders and non-workspace `docs/` paths are excluded.
    pub(crate) fn is_workspace_docs_sync_candidate_path(path: &str, workspace_id: Uuid) -> bool {
        let prefix = format!("docs/{workspace_id}/");
        if g5_contracts::types::is_managed_explainer_path(path) {
            return false;
        }
        if !path.starts_with(&prefix) || !Self::is_docs_surface_path(path) {
            return false;
        }

        let relative_path = &path[prefix.len()..];
        if relative_path.starts_with("sync/") || relative_path == "code_review.md" {
            return false;
        }

        let Some(legacy_merge_report) = relative_path.strip_prefix("merge/") else {
            return true;
        };
        !(legacy_merge_report.ends_with("_merge_report.md")
            && legacy_merge_report.len() > "_merge_report.md".len()
            && !legacy_merge_report.contains('/'))
    }

    /// Roots whose committed files are first-class documents on the docs
    /// surface: listed by `list_docs`, readable by path, and projected from
    /// committed Git.
    ///
    /// `explainers/` is the managed root for modal-driven Create HTML Explainer
    /// output (plan `explainer-create-cutover`, doc_service D17).
    ///
    /// This is the single source of truth for the docs-surface roots. The
    /// committed-tree walks in `committed_doc_files` and
    /// `sync_committed_docs` enumerate exactly these roots, so a root added
    /// here can never be admitted by `is_docs_surface_path` while staying
    /// invisible to the walks — the drift that made generated explainers pass
    /// path validation yet never appear in `list_docs`.
    pub const DOCS_SURFACE_ROOTS: &'static [&'static str] =
        &["docs", "views", "designs", "explainers"];

    fn is_docs_surface_prefix(path: &str) -> bool {
        Self::DOCS_SURFACE_ROOTS
            .iter()
            .any(|root| path.starts_with(&format!("{root}/")))
    }

    fn is_docs_surface_path(path: &str) -> bool {
        let Some(extension) = Self::accepted_project_doc_extension(path) else {
            return false;
        };
        Self::is_docs_surface_prefix(path)
            && (matches!(extension, ".md" | ".jsonl") || Self::is_html_project_doc_path(path))
    }

    fn is_html_project_doc_path(path: &str) -> bool {
        Self::is_docs_surface_prefix(path)
            && matches!(
                Self::accepted_project_doc_extension(path),
                Some(".html" | ".htm")
            )
    }

    fn virtual_doc_id(docs_branch: &DocsRepositoryVersion, file_path: &str) -> String {
        let encoded_path =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(file_path.as_bytes());
        format!(
            "overlay:{}:{}:{encoded_path}",
            docs_branch.workspace_id, docs_branch.repo_id
        )
    }

    fn file_stem_name(file_path: &str) -> String {
        Path::new(file_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.trim().is_empty())
            .unwrap_or(file_path)
            .replace(['-', '_'], " ")
    }

    fn first_h1(content: &str) -> Option<String> {
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("# ")
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(ToString::to_string)
        })
    }

    fn frontmatter_string(content: &str, keys: &[&str]) -> Option<String> {
        let frontmatter = parse_yaml_frontmatter(content)?;
        let mapping = frontmatter.as_mapping()?;
        for key in keys {
            let value = mapping.get(serde_yaml::Value::String((*key).to_string()))?;
            if let Some(text) = value.as_str().filter(|s| !s.trim().is_empty()) {
                return Some(text.trim().to_string());
            }
        }
        None
    }

    fn virtual_metadata_from_path(
        docs_branch: &DocsRepositoryVersion,
        file_path: &str,
        content: Option<String>,
    ) -> VirtualDocMetadata {
        let name = content
            .as_deref()
            .and_then(|body| Self::frontmatter_string(body, &["title", "name"]))
            .or_else(|| content.as_deref().and_then(Self::first_h1))
            .unwrap_or_else(|| Self::file_stem_name(file_path));
        let doc_type = content
            .as_deref()
            .and_then(|body| Self::frontmatter_string(body, &["doc_type", "type"]))
            .unwrap_or_else(|| Self::virtual_doc_type_from_path(file_path).to_string());
        VirtualDocMetadata {
            id: Self::virtual_doc_id(docs_branch, file_path),
            name,
            doc_type,
            file_path: file_path.to_string(),
        }
    }

    fn virtual_doc_type_from_path(file_path: &str) -> &'static str {
        const VIEW_SUFFIX_TYPES: &[(&str, &str)] = &[
            (".pm_view.md", "pm_view"),
            (".arch_view.md", "arch_view"),
            (".security_view.md", "security_view"),
            (".performance_view.md", "performance_view"),
            (".test_view.md", "test_view"),
            (".ui_view.md", "ui_view"),
            (".custom_view.md", "customized_view"),
        ];

        if let Some(doc_type) = VIEW_SUFFIX_TYPES
            .iter()
            .find(|(suffix, _)| file_path.ends_with(suffix))
            .map(|(_, doc_type)| *doc_type)
        {
            doc_type
        } else if file_path.starts_with("views/") {
            "customized_view"
        } else if g5_contracts::types::is_managed_explainer_path(file_path) {
            "explainer"
        } else {
            "design"
        }
    }

    async fn committed_doc_files(
        &self,
        repo_view: &CommittedRepoReadView,
    ) -> Result<BTreeMap<String, Option<String>>, G5Error> {
        if is_placeholder_head_commit(repo_view.head_commit()) {
            return Ok(BTreeMap::new());
        }

        let mut paths = BTreeSet::new();
        let requests = ["docs", "views", "designs", "explainers"]
            .into_iter()
            .map(|root| (root.to_string(), 32))
            .collect::<Vec<_>>();
        let trees = repo_view.list_trees(&self.repo_storage, &requests).await?;
        for entries in trees.into_values().flatten() {
            for entry in entries {
                if !matches!(entry.entry_type, TreeEntryType::File)
                    || !Self::is_docs_surface_path(&entry.path)
                {
                    continue;
                }
                paths.insert(entry.path);
            }
        }

        Ok(repo_view
            .read_files(&self.repo_storage, &paths.into_iter().collect::<Vec<_>>())
            .await?
            .into_iter()
            .map(|(path, content)| {
                let content = content.and_then(|bytes| String::from_utf8(bytes).ok());
                (path, content)
            })
            .collect())
    }

    fn non_view_doc_error() -> AppError {
        AppError::from_error(g5_contracts::error::G5Error::ValidationError {
            details: serde_json::json!({
                "docId": "document is not a view doc"
            }),
        })
    }

    async fn load_docs_repo_read(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        published_only: bool,
    ) -> Result<Option<DocsRepoRead>, AppError> {
        // Public browsing excludes recovery overlays. Internal sync and job
        // preparation still use the exact retained committed/flush cut.
        let target = AuthorityTarget::Workspace { workspace_id };
        let snapshot = if published_only {
            crate::services::workspace_reader::resolve_published_snapshot(
                db,
                &self.repo_storage,
                target,
            )
            .await
        } else {
            resolve_committed_snapshot(db, &self.repo_storage, target).await
        }
        .map_err(AppError::from_error)?;
        let Some(docs_repo_id) = docs_repo_id_from_snapshot(&snapshot)? else {
            return Ok(None);
        };
        let repo_view = RepoReadView::from_committed_snapshot(&*db, &snapshot, docs_repo_id)
            .map_err(AppError::from_error)?;
        let branch = DocsRepositoryVersion {
            branch_name: repo_view.repo.branch_name.clone(),
            repo_id: repo_view.repo.repo_id,
            workspace_id,
            head_commit: repo_view.repo.head_commit.clone(),
        };
        let repo_views = snapshot
            .repos
            .iter()
            .map(|repo| {
                RepoReadView::from_committed_snapshot(&*db, &snapshot, repo.repo_id)
                    .map(|view| (repo.repo_id, view))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(AppError::from_error)?;
        Ok(Some(DocsRepoRead {
            branch,
            view: repo_view,
            snapshot,
            repo_views,
        }))
    }

    fn docs_repo_read_from_connected(
        workspace_id: Uuid,
        connected: &ResolvedConnectedBindingView,
    ) -> Result<Option<DocsRepoRead>, AppError> {
        if connected.view.snapshot.workspace_id != workspace_id
            || connected.view.snapshot.target != (AuthorityTarget::Workspace { workspace_id })
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Docs reader requires the matching connected workspace cut",
                    "workspace_id": workspace_id,
                    "target": connected.view.snapshot.target,
                }),
            }));
        }
        let Some(docs_repo_id) = docs_repo_id_from_snapshot(&connected.view.snapshot)? else {
            return Ok(None);
        };
        let repo_view = connected.repo(docs_repo_id).map_err(AppError::from_error)?;
        let repo_views = connected
            .view
            .snapshot
            .repos
            .iter()
            .map(|repo| {
                connected
                    .repo(repo.repo_id)
                    .map(|view| (repo.repo_id, view))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(AppError::from_error)?;
        let branch = DocsRepositoryVersion {
            branch_name: repo_view.repo.branch_name.clone(),
            repo_id: repo_view.repo.repo_id,
            workspace_id,
            head_commit: repo_view.repo.head_commit.clone(),
        };
        Ok(Some(DocsRepoRead {
            branch,
            view: repo_view,
            snapshot: connected.view.snapshot.clone(),
            repo_views,
        }))
    }

    async fn resolve_doc_references(
        &self,
        docs_read: &DocsRepoRead,
        content: &str,
    ) -> Result<EffectiveDocumentReferences, AppError> {
        resolve_document_references_from_content(
            &self.repo_storage,
            &docs_read.snapshot,
            &docs_read.repo_views,
            content,
        )
        .await
        .map_err(AppError::from_error)
    }

    fn sync_reference_scope(
        docs_read: &DocsRepoRead,
        content: &str,
        source_dirty_overlay: bool,
        references: &EffectiveDocumentReferences,
    ) -> SyncReferenceScope {
        let mut repository_versions = docs_read
            .snapshot
            .repos
            .iter()
            .map(|repo| SyncReferenceRepoVersion {
                repo_id: repo.repo_id.to_string(),
                head_commit: repo.head_commit.clone(),
            })
            .collect::<Vec<_>>();
        repository_versions.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));

        SyncReferenceScope {
            repository_versions,
            source_content_hash: Self::sha256_hex(content.as_bytes()),
            source_dirty_overlay,
            raw_references: references.raw_references.clone(),
            resolved_component_ids: references
                .resolved_references
                .iter()
                .map(|reference| reference.component_id.clone())
                .collect(),
            unresolved_references: references.unresolved_references.clone(),
        }
    }

    async fn require_docs_repo_read(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<DocsRepoRead, AppError> {
        self.load_docs_repo_read(db, workspace_id, false)
            .await?
            .ok_or_else(|| {
                AppError::from_error(g5_contracts::error::G5Error::NotFound {
                    resource: format!("workspace/{workspace_id}/docs-branch"),
                })
            })
    }

    fn decode_doc_content(file_path: &str, bytes: Vec<u8>) -> Result<String, AppError> {
        String::from_utf8(bytes).map_err(|error| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "path": file_path,
                    "message": format!("Document content is not UTF-8: {error}"),
                }),
            })
        })
    }

    async fn read_effective_doc_content(
        &self,
        repo_view: &RepoReadView,
        file_path: &str,
    ) -> Result<(String, bool), AppError> {
        match repo_view
            .read_file(&self.repo_storage, file_path)
            .await
            .map_err(AppError::from_error)?
        {
            RepoFileRead::Content { bytes, source } => Ok((
                Self::decode_doc_content(file_path, bytes)?,
                matches!(source, RepoFileSource::DirtyOverlay),
            )),
            RepoFileRead::Deleted | RepoFileRead::Missing => {
                Err(AppError::from_error(G5Error::NotFound {
                    resource: format!("doc content {file_path}"),
                }))
            }
        }
    }

    async fn captured_deleted_paths(repo_view: &RepoReadView) -> BTreeSet<String> {
        let mut deleted = BTreeSet::new();
        for path in repo_view.dirty_files() {
            if matches!(repo_view.file_overlay(path).await, RepoFileOverlay::Deleted) {
                deleted.insert(path.clone());
            }
        }
        deleted
    }

    async fn update_file_backed_doc_metadata(
        &self,
        db: &mut ScopedDb,
        doc_id: Uuid,
        content: &str,
        committed_doc_type: Option<&str>,
    ) -> Result<(), AppError> {
        let content_hash = Self::sha256_hex(content.as_bytes());
        let linked_doc_paths = Self::linked_doc_paths_projection(content);

        sqlx::query(
            r#"
            UPDATE docs
            SET content = NULL,
                content_hash = $1,
                linked_doc_paths = $2,
                doc_type = COALESCE($3, doc_type),
                content_updated_at = now(),
                updated_at = now()
            WHERE id = $4
            "#,
        )
        .bind(content_hash)
        .bind(linked_doc_paths)
        .bind(committed_doc_type)
        .bind(doc_id)
        .execute(db.conn())
        .await
        .map_err(|e| {
            AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: format!("Failed to normalize doc projection: {e}"),
            })
        })?;

        Ok(())
    }

    fn is_view_starter_content(content: &str) -> bool {
        content.contains("This view is a starter markdown document")
            && content.contains("canonical `root_spec_ids`")
    }

    /// List committed docs under the workspace's dedicated Project Docs
    /// folder `docs/<workspace_id>/` available to Generate. No projection marker
    /// means unknown generation history. Legacy review ledgers are not inputs.
    pub async fn list_pending_downlift_docs(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        connected: &ResolvedConnectedBindingView,
    ) -> Result<Vec<PendingDocDownliftFile>, AppError> {
        let Some(docs_read) = Self::docs_repo_read_from_connected(workspace_id, connected)? else {
            return Ok(Vec::new());
        };
        let DocsRepoRead {
            branch: docs_branch,
            view: repo_view,
            ..
        } = docs_read;
        let branch_repo_id = docs_branch.repo_id;
        let repo_name: String = sqlx::query_scalar(
            r#"
            SELECT name
            FROM repos
            WHERE id = $1
            "#,
        )
        .bind(docs_branch.repo_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| AppError::from_error(g5_contracts::error::G5Error::from(e)))?;

        let deleted_paths = Self::captured_deleted_paths(&repo_view)
            .await
            .into_iter()
            .filter(|path| Self::is_docs_surface_path(path))
            .collect::<BTreeSet<_>>();
        let dirty_paths = repo_view
            .dirty_files()
            .iter()
            .filter(|path| Self::is_docs_surface_path(path))
            .cloned()
            .collect::<BTreeSet<_>>();

        let committed_files = self
            .committed_doc_files(repo_view.committed_view())
            .await
            .map_err(AppError::from_error)?;

        let candidate_paths: BTreeSet<String> = committed_files
            .keys()
            .filter(|path| Self::is_workspace_docs_sync_candidate_path(path, workspace_id))
            .cloned()
            .collect();

        let candidate_paths = candidate_paths
            .into_iter()
            .filter(|path| !dirty_paths.contains(path) && !deleted_paths.contains(path))
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for path in candidate_paths {
            rows.push(PendingDocDownliftFile {
                id: format!("doc:{branch_repo_id}:{path}"),
                target_doc_ref: Self::virtual_doc_id(&docs_branch, &path),
                path,
                repo_id: docs_branch.repo_id,
                repo_name: repo_name.clone(),
                branch_name: docs_branch.branch_name.clone(),
                head_commit: docs_branch.head_commit.clone(),
                last_downlifted_commit_sha: None,
                document_changed: false,
            });
        }

        Ok(rows)
    }

    /// Capture the exact connected Docs repository view while the caller's
    /// authorization transaction is open. The returned value owns no database
    /// connection and is safe to use after the caller closes that transaction.
    pub async fn capture_docs_list(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<CapturedDocsList, AppError> {
        let docs_read = self.load_docs_repo_read(db, workspace_id, true).await?;
        let docs_cut_at = if let Some(read) = &docs_read {
            read.view
                .committed_view()
                .committed_at(&self.repo_storage)
                .await?
        } else {
            Utc::now()
        };
        let document_sources_by_path = if let Some(read) = &docs_read {
            let branch_repo_id = read.branch.repo_id;
            let rows = sqlx::query_as::<_, DocumentSourceRow>(
                r#"
                SELECT id, file_path, document_source, updated_at
                FROM docs
                WHERE workspace_id = $1
                  AND repo_id = $2
                  AND file_path IS NOT NULL
                ORDER BY file_path,
                         (document_source IS NOT NULL) DESC,
                         updated_at DESC,
                         id DESC
                "#,
            )
            .bind(workspace_id)
            .bind(branch_repo_id)
            .fetch_all(db.conn())
            .await?;

            let mut sources = BTreeMap::new();
            let mut warned_paths = BTreeSet::new();
            for row in rows {
                let Some(source) = row.document_source else {
                    continue;
                };
                if let Some(chosen) = sources.get(&row.file_path) {
                    if chosen != &source && warned_paths.insert(row.file_path.clone()) {
                        tracing::warn!(
                            workspace_id = %workspace_id,
                            repo_id = %branch_repo_id,
                            file_path = %row.file_path,
                            chosen_document_source = %chosen,
                            conflicting_document_source = %source,
                            conflicting_row_id = %row.id,
                            conflicting_updated_at = %row.updated_at,
                            "Conflicting Project Doc source projection rows; preserving deterministic winner"
                        );
                    }
                    continue;
                }
                sources.insert(row.file_path, source);
            }
            sources
        } else {
            BTreeMap::new()
        };
        Ok(CapturedDocsList {
            repos: self.repo_storage.scoped(&*db),
            docs_read,
            docs_cut_at,
            document_sources_by_path,
        })
    }

    /// List all docs from one transaction-detached effective repository view.
    ///
    /// Membership, stable identity, presentation metadata, and content state
    /// come only from the exact Git cut plus its validated live workspace file overlay. This
    /// method deliberately has no database parameter and performs no docs or
    /// ontology projection query.
    pub async fn list_docs(
        &self,
        captured: CapturedDocsList,
    ) -> Result<Vec<DocListItemDTO>, AppError> {
        self.list_docs_with_live_overlay(captured, None).await
    }

    /// The running attachment when `actor_user_id` is its owner (design v9
    /// §9.1). Never wakes a stopped workspace and never serves a reader.
    async fn attached_owner_handle(
        runtime: &WorkspaceRuntime,
        actor_user_id: Uuid,
        workspace_id: Uuid,
    ) -> Option<WorkspaceRuntimeHandle> {
        let handle = runtime.get(workspace_id).await.ok()??;
        (handle.status == "running" && handle.owner_user_id == actor_user_id).then_some(handle)
    }

    fn live_docs_request(
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        phase: &str,
    ) -> RuntimeRequest {
        RuntimeRequest::owner(
            handle,
            actor_user_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .saturating_add(30_000) as u64,
            format!("docs-live:{}:{phase}", Uuid::new_v4()),
        )
    }

    /// The attached owner's live docs surface (design v9 §9.1: the owner with
    /// a running attachment reads through the facade; everyone else, and an
    /// unattached owner, reads durable Git). Lists the docs-surface roots of
    /// the docs repository in the running VM and reads the content of every
    /// file the committed cut does not carry plus the workspace's own Project
    /// Docs (`docs/<workspace id>/`), so a doc just written through Save lists
    /// and counts at once. A live listing that fails leaves the committed cut
    /// in place with a warning.
    pub async fn live_docs_overlay(
        &self,
        runtime: &WorkspaceRuntime,
        actor_user_id: Uuid,
        workspace_id: Uuid,
        captured: &CapturedDocsList,
    ) -> Option<LiveDocsOverlay> {
        let repo_id = captured.docs_repo_id()?;
        let handle = Self::attached_owner_handle(runtime, actor_user_id, workspace_id).await?;
        let committed = match self.committed_docs_surface_paths(captured).await {
            Ok(paths) => paths,
            Err(error) => {
                tracing::warn!(%workspace_id, %error, "live docs overlay: committed surface unavailable");
                return None;
            }
        };
        let mut overlay = LiveDocsOverlay::default();
        let mut queue: std::collections::VecDeque<String> = Self::DOCS_SURFACE_ROOTS
            .iter()
            .map(|root| (*root).to_string())
            .collect();
        while let Some(directory) = queue.pop_front() {
            let request = Self::live_docs_request(&handle, actor_user_id, "list");
            let listed = match runtime
                .file_list(&handle, &request, repo_id, &directory)
                .await
            {
                Ok(value) => value,
                // A surface root the checkout does not have is simply empty.
                Err(error) if error.message == "FILE_MISSING" => continue,
                Err(error) => {
                    tracing::warn!(%workspace_id, directory, error = %error.message, "live docs overlay: listing failed; serving the committed cut");
                    return None;
                }
            };
            for entry in listed["entries"].as_array().into_iter().flatten() {
                let Some(path) = entry["path"].as_str() else {
                    continue;
                };
                match entry["type"].as_str() {
                    Some("directory") => queue.push_back(path.to_string()),
                    Some("file") => {
                        overlay.present.insert(path.to_string());
                    }
                    _ => {}
                }
                if overlay.present.len() > 10_000 {
                    tracing::warn!(%workspace_id, "live docs overlay: surface exceeds 10000 files; serving the committed cut");
                    return None;
                }
            }
        }
        let project_docs_prefix = format!("docs/{workspace_id}/");
        let to_read: Vec<String> = overlay
            .present
            .iter()
            .filter(|path| {
                Self::is_docs_surface_path(path)
                    && (!committed.contains(*path) || path.starts_with(&project_docs_prefix))
            })
            .cloned()
            .collect();
        for path in to_read {
            let request = Self::live_docs_request(&handle, actor_user_id, "read");
            match runtime.file_read(&handle, &request, repo_id, &path).await {
                Ok(value) => {
                    if let Some(content) = value["content"].as_str() {
                        if value["encoding"].as_str() != Some("base64") {
                            overlay.contents.insert(path, content.to_string());
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%workspace_id, path, error = %error.message, "live docs overlay: read failed; serving the committed file");
                }
            }
        }
        Some(overlay)
    }

    /// One docs-surface path as the attached owner's running checkout holds
    /// it (design v9 §9.1). Anyone else gets `Unavailable` and reads the
    /// committed cut.
    pub async fn live_doc_read(
        &self,
        runtime: &WorkspaceRuntime,
        actor_user_id: Uuid,
        workspace_id: Uuid,
        captured: &CapturedDocsList,
        path: &str,
    ) -> LiveDocRead {
        let Some(repo_id) = captured.docs_repo_id() else {
            return LiveDocRead::Unavailable;
        };
        let Some(handle) = Self::attached_owner_handle(runtime, actor_user_id, workspace_id).await
        else {
            return LiveDocRead::Unavailable;
        };
        let request = Self::live_docs_request(&handle, actor_user_id, "read");
        match runtime.file_read(&handle, &request, repo_id, path).await {
            Ok(value) => match value["content"].as_str() {
                Some(content) if value["encoding"].as_str() != Some("base64") => {
                    LiveDocRead::Content(content.to_string())
                }
                _ => LiveDocRead::Unavailable,
            },
            Err(error) if error.message == "FILE_MISSING" => LiveDocRead::Missing,
            Err(error) => {
                tracing::warn!(%workspace_id, path, error = %error.message, "live doc read failed; serving the committed file");
                LiveDocRead::Unavailable
            }
        }
    }

    /// Committed bytes of `paths` in the captured docs cut; `None` for a
    /// path the cut does not hold (and for a workspace without docs).
    pub async fn read_committed_docs_files(
        &self,
        captured: &CapturedDocsList,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, AppError> {
        let Some(docs_read) = captured.docs_read.as_ref() else {
            return Ok(paths.iter().map(|path| (path.clone(), None)).collect());
        };
        let reads = docs_read
            .view
            .read_files(&self.repo_storage, paths)
            .await
            .map_err(AppError::from_error)?;
        Ok(paths
            .iter()
            .map(|path| {
                let bytes = match reads.get(path) {
                    Some(RepoFileRead::Content { bytes, .. }) => Some(bytes.clone()),
                    _ => None,
                };
                (path.clone(), bytes)
            })
            .collect())
    }

    /// File paths directly inside `dir` in the captured docs cut.
    pub async fn list_committed_docs_files(
        &self,
        captured: &CapturedDocsList,
        dir: &str,
    ) -> Result<Vec<String>, AppError> {
        let Some(docs_read) = captured.docs_read.as_ref() else {
            return Ok(Vec::new());
        };
        let trees = docs_read
            .view
            .list_trees(&self.repo_storage, &[(dir.to_string(), 1)])
            .await
            .map_err(AppError::from_error)?;
        let prefix = format!("{dir}/");
        Ok(trees
            .into_values()
            .flatten()
            .flatten()
            .filter(|entry| {
                matches!(entry.entry_type, TreeEntryType::File)
                    && entry
                        .path
                        .strip_prefix(&prefix)
                        .is_some_and(|name| !name.contains('/'))
            })
            .map(|entry| entry.path)
            .collect())
    }

    /// Docs-surface file paths present in the captured committed cut.
    pub async fn committed_docs_surface_paths(
        &self,
        captured: &CapturedDocsList,
    ) -> Result<BTreeSet<String>, AppError> {
        let Some(docs_read) = captured.docs_read.as_ref() else {
            return Ok(BTreeSet::new());
        };
        let requests = Self::DOCS_SURFACE_ROOTS
            .iter()
            .map(|root| ((*root).to_string(), u32::MAX))
            .collect::<Vec<_>>();
        let trees = docs_read
            .view
            .list_trees(&self.repo_storage, &requests)
            .await
            .map_err(AppError::from_error)?;
        Ok(trees
            .into_values()
            .flatten()
            .flatten()
            .filter(|entry| {
                matches!(entry.entry_type, TreeEntryType::File)
                    && Self::is_docs_surface_path(&entry.path)
            })
            .map(|entry| entry.path)
            .collect())
    }

    /// List docs from the committed cut, merged with the attached owner's live
    /// docs surface when one is supplied (design v9 §9.1): live-only files
    /// list as dirty drafts with their live content, and files the live
    /// checkout no longer has are dropped.
    pub async fn list_docs_with_live_overlay(
        &self,
        captured: CapturedDocsList,
        live: Option<LiveDocsOverlay>,
    ) -> Result<Vec<DocListItemDTO>, AppError> {
        let committed_paths = self.committed_docs_surface_paths(&captured).await?;
        let document_sources_by_path = captured.document_sources_by_path;
        let Some(docs_read) = captured.docs_read else {
            return Ok(Vec::new());
        };
        let docs_branch = &docs_read.branch;
        let repo_view = &docs_read.view;
        let paths = committed_paths
            .iter()
            .filter(|path| {
                live.as_ref()
                    .is_none_or(|live| live.present.contains(*path))
            })
            .cloned()
            .collect::<Vec<_>>();
        let effective_reads = repo_view
            .read_files(&self.repo_storage, &paths)
            .await
            .map_err(AppError::from_error)?;

        let mut effective_files = effective_reads
            .into_iter()
            .filter_map(|(path, read)| match read {
                RepoFileRead::Content { bytes, source } => {
                    Some((path, (String::from_utf8(bytes).ok(), source)))
                }
                RepoFileRead::Deleted | RepoFileRead::Missing => None,
            })
            .collect::<BTreeMap<_, _>>();
        if let Some(live) = live {
            for (path, (content, source)) in effective_files.iter_mut() {
                if let Some(live_content) = live.content(path) {
                    if content.as_deref() != Some(live_content) {
                        *content = Some(live_content.to_string());
                        *source = RepoFileSource::DirtyOverlay;
                    }
                }
            }
            for path in live
                .present
                .into_iter()
                .filter(|path| Self::is_docs_surface_path(path))
            {
                if effective_files.contains_key(&path) || committed_paths.contains(&path) {
                    continue;
                }
                effective_files.insert(
                    path.clone(),
                    (
                        live.contents.get(&path).cloned(),
                        RepoFileSource::DirtyOverlay,
                    ),
                );
            }
        }
        let effective_contents = effective_files
            .iter()
            .filter_map(|(path, (content, _))| {
                content
                    .as_ref()
                    .map(|content| (path.clone(), content.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let references_by_path = resolve_document_references_batch_from_content(
            &self.repo_storage,
            &docs_read.snapshot,
            &docs_read.repo_views,
            &effective_contents,
        )
        .await
        .map_err(AppError::from_error)?;

        let now = Utc::now();
        let mut result = effective_files
            .into_iter()
            .map(|(path, (content, source))| {
                let draft_dirty = matches!(source, RepoFileSource::DirtyOverlay);
                let meta = Self::virtual_metadata_from_path(docs_branch, &path, content.clone());
                let is_future = meta.doc_type.starts_with("future_");
                DocListItemDTO {
                    id: meta.id,
                    name: meta.name,
                    doc_type: meta.doc_type,
                    file_path: Some(meta.file_path),
                    document_source: document_sources_by_path.get(&path).cloned(),
                    linked_node_count: references_by_path
                        .get(&path)
                        .map_or(0, |references| references.resolved_references.len() as i64),
                    source: if draft_dirty { "overlay" } else { "committed" }.to_string(),
                    scope: "workspace".to_string(),

                    repo_id: Some(docs_branch.repo_id),
                    draft_dirty,
                    is_virtual: true,
                    is_future,
                    last_uplifted_at: None,
                    last_downlifted_at: None,
                    content_updated_at: draft_dirty.then_some(now),
                    created_at: now,
                    updated_at: now,
                }
            })
            .collect::<Vec<_>>();

        result.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(result)
    }

    /// Markdown Project Docs of the workspace: the committed ones (dropped
    /// when the live checkout no longer has them) plus, for the attached
    /// owner, the ones only the live checkout holds.
    fn project_doc_markdown_paths(
        committed: impl Iterator<Item = String>,
        workspace_docs_prefix: &str,
        live: Option<&LiveDocsOverlay>,
    ) -> Vec<String> {
        let is_project_doc_markdown = |path: &str| {
            path.starts_with(workspace_docs_prefix)
                && Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("md")
                            || extension.eq_ignore_ascii_case("markdown")
                    })
        };
        let mut paths = committed
            .filter(|path| {
                is_project_doc_markdown(path) && live.is_none_or(|live| live.present.contains(path))
            })
            .collect::<BTreeSet<_>>();
        if let Some(live) = live {
            paths.extend(
                live.present
                    .iter()
                    .filter(|path| is_project_doc_markdown(path))
                    .cloned(),
            );
        }
        paths.into_iter().collect()
    }

    /// The content a question scan reads for one Project Doc: the live file
    /// when the attached owner's overlay carries it, else the committed read.
    fn effective_project_doc_content(
        path: &str,
        committed: Option<RepoFileRead>,
        live: Option<&LiveDocsOverlay>,
    ) -> Option<String> {
        if let Some(content) = live.and_then(|live| live.content(path)) {
            return Some(content.to_string());
        }
        let Some(RepoFileRead::Content { bytes, .. }) = committed else {
            return None;
        };
        String::from_utf8(bytes).ok()
    }

    /// List source-only summaries for effective non-view Markdown Project Docs
    /// that contain open interactive questions.
    pub async fn open_project_doc_question_sources(
        &self,
        captured: CapturedDocsList,
        live: Option<LiveDocsOverlay>,
    ) -> Result<Vec<ProjectDocOpenQuestionSource>, AppError> {
        Ok(self
            .open_project_doc_question_families(captured, live)
            .await?
            .project_docs)
    }

    /// One corpus read of the effective non-view Markdown Project Docs: the
    /// embedded open-question sources and every question-set reference the
    /// documents hold (`g5.documents.question.metrics`).
    pub async fn open_project_doc_question_families(
        &self,
        captured: CapturedDocsList,
        live: Option<LiveDocsOverlay>,
    ) -> Result<ProjectDocQuestionFamilies, AppError> {
        let docs_cut_at = captured.docs_cut_at;
        let Some(docs_read) = captured.docs_read else {
            return Ok(ProjectDocQuestionFamilies::default());
        };
        let docs_branch = &docs_read.branch;
        let repo_view = &docs_read.view;
        let workspace_id = docs_branch.workspace_id;
        let workspace_docs_prefix = format!("docs/{workspace_id}/");
        let trees = repo_view
            .list_trees(
                &self.repo_storage,
                &[(
                    workspace_docs_prefix.trim_end_matches('/').to_string(),
                    u32::MAX,
                )],
            )
            .await
            .map_err(AppError::from_error)?;
        let paths = Self::project_doc_markdown_paths(
            trees
                .into_values()
                .flatten()
                .flatten()
                .filter(|entry| matches!(entry.entry_type, TreeEntryType::File))
                .map(|entry| entry.path),
            &workspace_docs_prefix,
            live.as_ref(),
        );
        let mut effective_reads = repo_view
            .read_files(&self.repo_storage, &paths)
            .await
            .map_err(AppError::from_error)?;

        let mut sources = Vec::new();
        let mut references: BTreeMap<QuestionReference, Vec<String>> = BTreeMap::new();
        for path in paths {
            let Some(content) = Self::effective_project_doc_content(
                &path,
                effective_reads.remove(&path),
                live.as_ref(),
            ) else {
                continue;
            };
            let metadata =
                Self::virtual_metadata_from_path(docs_branch, &path, Some(content.clone()));
            if Self::is_view_doc_type(&metadata.doc_type) {
                continue;
            }
            let summary = summarize_open_question_fences(&content);
            for reference in &summary.references {
                references
                    .entry(reference.clone())
                    .or_default()
                    .push(metadata.id.clone());
            }
            if summary.count > 0 {
                sources.push(ProjectDocOpenQuestionSource {
                    doc_id: metadata.id,
                    display_name: project_doc_question_source_display_name(&path, &metadata.name),
                    open_question_count: summary.count as i64,
                    first_open_question_block_id: summary.first_block_id,
                    open_questions: summary.questions,
                    actionable_at: docs_cut_at,
                });
            }
        }

        Ok(ProjectDocQuestionFamilies {
            project_docs: sources,
            references,
        })
    }

    /// Home-only page-bounded variant. Stable path-derived identities are
    /// ordered (in the requested page order) before effective content reads;
    /// closed documents are scanned in bounded chunks and do not consume the
    /// positive-source budget.
    pub(crate) async fn open_project_doc_question_sources_bounded(
        &self,
        captured: CapturedDocsList,
        live: Option<LiveDocsOverlay>,
        cursor: Option<&HomeAttentionCursor>,
        order: HomeAttentionOrder,
        candidate_limit: usize,
    ) -> Result<Vec<ProjectDocOpenQuestionSource>, AppError> {
        if candidate_limit == 0 {
            return Ok(Vec::new());
        }
        let docs_cut_at = captured.docs_cut_at;
        let Some(docs_read) = captured.docs_read else {
            return Ok(Vec::new());
        };
        let docs_branch = &docs_read.branch;
        let repo_view = &docs_read.view;
        let workspace_id = docs_branch.workspace_id;
        let workspace_docs_prefix = format!("docs/{workspace_id}/");
        let trees = repo_view
            .list_trees(
                &self.repo_storage,
                &[(
                    workspace_docs_prefix.trim_end_matches('/').to_string(),
                    u32::MAX,
                )],
            )
            .await
            .map_err(AppError::from_error)?;
        let mut identities = Self::project_doc_markdown_paths(
            trees
                .into_values()
                .flatten()
                .flatten()
                .filter(|entry| matches!(entry.entry_type, TreeEntryType::File))
                .map(|entry| entry.path),
            &workspace_docs_prefix,
            live.as_ref(),
        )
        .into_iter()
        .map(|path| {
            let metadata = Self::virtual_metadata_from_path(docs_branch, &path, None);
            let item_id = format!("open_question:{workspace_id}:{}", metadata.id);
            (item_id, metadata.id, path)
        })
        .filter(|(item_id, _, _)| {
            cursor.is_none_or(|cursor| cursor.is_after(docs_cut_at, 1, workspace_id, item_id))
        })
        .collect::<Vec<_>>();
        identities.sort_by(|left, right| order.compare(&left.0, &right.0));

        const READ_CHUNK_SIZE: usize = 32;
        let mut sources = Vec::with_capacity(candidate_limit);
        for chunk in identities.chunks(READ_CHUNK_SIZE) {
            let paths = chunk
                .iter()
                .map(|(_, _, path)| path.clone())
                .collect::<Vec<_>>();
            let mut reads = repo_view
                .read_files(&self.repo_storage, &paths)
                .await
                .map_err(AppError::from_error)?;
            for (_, stable_doc_id, path) in chunk {
                let Some(content) =
                    Self::effective_project_doc_content(path, reads.remove(path), live.as_ref())
                else {
                    continue;
                };
                let metadata =
                    Self::virtual_metadata_from_path(docs_branch, path, Some(content.clone()));
                if Self::is_view_doc_type(&metadata.doc_type) {
                    continue;
                }
                debug_assert_eq!(&metadata.id, stable_doc_id);
                let summary = summarize_open_question_fences(&content);
                if summary.count == 0 {
                    continue;
                }
                sources.push(ProjectDocOpenQuestionSource {
                    doc_id: metadata.id,
                    display_name: project_doc_question_source_display_name(path, &metadata.name),
                    open_question_count: summary.count as i64,
                    first_open_question_block_id: summary.first_block_id,
                    open_questions: summary.questions,
                    actionable_at: docs_cut_at,
                });
                if sources.len() == candidate_limit {
                    return Ok(sources);
                }
            }
        }
        Ok(sources)
    }

    /// Get a single doc by ID within a workspace, including content (from git)
    /// and linked nodes.
    pub async fn get_doc(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        doc_id: Uuid,
    ) -> Result<DocDetailDTO, AppError> {
        let docs_read = self.load_docs_repo_read(db, workspace_id, true).await?;
        let docs_branch = match &docs_read {
            Some(read) => read.branch.clone(),
            None => self.docs_branch_for_update(db, workspace_id).await?,
        };
        self.get_doc_with_branch(db, workspace_id, doc_id, &docs_branch, docs_read.as_ref())
            .await
    }

    /// Readback variant of [`get_doc`] using an already-resolved docs branch.
    /// `update_doc` passes its WRITE-resolved branch here so a metadata-only
    /// update to a still-unborn docs branch (fresh workspace, no content write)
    /// does not surface a false `workspace/<id>/docs-branch` NOT_FOUND on
    /// readback — the read resolver filters unborn heads. (PR #456 review)
    async fn get_doc_with_branch(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        doc_id: Uuid,
        docs_branch: &DocsRepositoryVersion,
        captured_docs_read: Option<&DocsRepoRead>,
    ) -> Result<DocDetailDTO, AppError> {
        let branch_repo_id = docs_branch.repo_id;
        // Fetch the doc metadata
        let row = sqlx::query_as::<_, DocMetadataRow>(
            r#"
            SELECT
                d.id,
                d.workspace_id,
                d.name,
                d.doc_type,
                d.file_path,
                d.document_source,
                d.last_uplifted_at,
                d.last_downlifted_at,
                d.content_updated_at,
                d.content,
                d.content_hash,
                d.last_uplifted_commit_sha,
                d.last_downlifted_commit_sha,
                d.created_at,
                d.updated_at
            FROM docs d
            WHERE d.id = $1
              AND d.workspace_id = $2
              AND d.repo_id = $3
            "#,
        )
        .bind(doc_id)
        .bind(workspace_id)
        .bind(branch_repo_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| {
            tracing::error!(
                doc_id = %doc_id,
                workspace_id = %workspace_id,
                error = %e,
                "Failed to fetch doc"
            );
            AppError::from_error(g5_contracts::error::G5Error::from(e))
        })?;
        let row = row.ok_or_else(|| {
            AppError::from_error(g5_contracts::error::G5Error::NotFound {
                resource: format!("workspace/{workspace_id}/doc/{doc_id}"),
            })
        })?;

        let file_path = row.file_path.clone();
        let file_path_ref = Self::require_doc_file_path(doc_id, file_path.as_deref())?;

        let owned_repo_read;
        let active_docs_read = match captured_docs_read {
            Some(docs_read) => Some(docs_read),
            None if is_placeholder_head_commit(&docs_branch.head_commit) => None,
            None => {
                owned_repo_read = self.require_docs_repo_read(db, workspace_id).await?;
                if owned_repo_read.branch.workspace_id != docs_branch.workspace_id
                    || owned_repo_read.branch.repo_id != docs_branch.repo_id
                    || owned_repo_read.branch.head_commit != docs_branch.head_commit
                {
                    return Err(AppError::from_error(G5Error::Conflict {
                        reason: "docs authority advanced while resolving document content; retry the read"
                            .to_string(),
                    }));
                }
                Some(&owned_repo_read)
            }
        };
        let (content, draft_dirty) = match active_docs_read {
            Some(docs_read) => match self
                .read_effective_doc_content(&docs_read.view, file_path_ref)
                .await
            {
                Ok(content) => content,
                Err(error) if matches!(&error.inner, G5Error::NotFound { .. }) => {
                    (row.content.clone().unwrap_or_default(), false)
                }
                Err(error) => return Err(error),
            },
            None => {
                // An unborn docs branch has no committed object yet. Preserve
                // the existing metadata-only bootstrap behavior.
                (row.content.clone().unwrap_or_default(), false)
            }
        };
        let content =
            if Self::is_view_doc_type(&row.doc_type) && Self::is_view_starter_content(&content) {
                row.content.clone().unwrap_or(content)
            } else {
                content
            };

        let (linked_component_ids, reference_scope) = match active_docs_read {
            Some(docs_read) => {
                let references = self.resolve_doc_references(docs_read, &content).await?;
                let linked_component_ids = references
                    .resolved_references
                    .iter()
                    .map(|reference| reference.component_id.clone())
                    .collect();
                let reference_scope =
                    Self::sync_reference_scope(docs_read, &content, draft_dirty, &references);
                (linked_component_ids, Some(reference_scope))
            }
            None => (Vec::new(), None),
        };

        Ok(DocDetailDTO {
            id: row.id.to_string(),
            workspace_id: row.workspace_id,
            name: row.name,
            doc_type: row.doc_type,
            file_path,
            document_source: row.document_source,
            content: Some(content),
            linked_component_ids,
            reference_scope,
            source: if draft_dirty { "merged" } else { "indexed" }.to_string(),
            scope: "workspace".to_string(),

            repo_id: Some(docs_branch.repo_id),
            draft_dirty,
            is_virtual: false,
            last_uplifted_at: row.last_uplifted_at,
            last_downlifted_at: row.last_downlifted_at,
            content_updated_at: row.content_updated_at,
            content_hash: row.content_hash,
            last_uplifted_commit_sha: row.last_uplifted_commit_sha,
            last_downlifted_commit_sha: row.last_downlifted_commit_sha,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Get a doc by repo-local path on the active workspace docs surface.
    pub async fn get_doc_by_path(
        &self,
        captured: CapturedDocsList,
        file_path: String,
        live: LiveDocRead,
    ) -> Result<DocDetailDTO, AppError> {
        if !Self::is_docs_surface_path(&file_path) {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({
                        "path": "doc path must be under docs/, views/, designs/, or explainers/ and end with .md, .html, .htm, or .jsonl"
                    }),
                },
            ));
        }

        let document_source = captured.document_sources_by_path.get(&file_path).cloned();
        let docs_read = captured.docs_read.ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("docs repository containing '{file_path}'"),
            })
        })?;
        let workspace_id = docs_read.branch.workspace_id;
        let docs_branch = &docs_read.branch;
        let repo_view = &docs_read.view;
        // Design v9 §9.1: the attached owner reads the running checkout; a
        // file it no longer has is gone even if the committed cut carries it.
        let (content, draft_dirty) = match live {
            LiveDocRead::Missing => {
                return Err(AppError::from_error(G5Error::NotFound {
                    resource: format!("doc content {file_path}"),
                }))
            }
            LiveDocRead::Content(live_content) => {
                let committed = self
                    .read_effective_doc_content(repo_view, &file_path)
                    .await
                    .ok();
                let dirty = committed
                    .as_ref()
                    .is_none_or(|(committed, _)| committed != &live_content);
                (live_content, dirty)
            }
            LiveDocRead::Unavailable => {
                self.read_effective_doc_content(repo_view, &file_path)
                    .await?
            }
        };
        let references = self.resolve_doc_references(&docs_read, &content).await?;
        let reference_scope =
            Self::sync_reference_scope(&docs_read, &content, draft_dirty, &references);
        let linked_component_ids = references
            .resolved_references
            .iter()
            .map(|reference| reference.component_id.clone())
            .collect();

        let now = Utc::now();
        let meta = Self::virtual_metadata_from_path(docs_branch, &file_path, Some(content.clone()));
        Ok(DocDetailDTO {
            id: meta.id,
            workspace_id,
            name: meta.name,
            doc_type: meta.doc_type,
            file_path: Some(file_path),
            document_source,
            content: Some(content),
            linked_component_ids,
            reference_scope: Some(reference_scope),
            source: if draft_dirty { "overlay" } else { "committed" }.to_string(),
            scope: "workspace".to_string(),

            repo_id: Some(docs_branch.repo_id),
            draft_dirty,
            is_virtual: true,
            last_uplifted_at: None,
            last_downlifted_at: None,
            content_updated_at: Some(now),
            content_hash: None,
            last_uplifted_commit_sha: None,
            last_downlifted_commit_sha: None,
            created_at: now,
            updated_at: now,
        })
    }

    fn create_doc_lock_key(workspace_id: Uuid, branch_repo_id: Uuid, file_path: &str) -> i64 {
        let mut hasher = Sha256::new();
        hasher.update(b"doc-create:");
        hasher.update(workspace_id.as_bytes());
        hasher.update(branch_repo_id.as_bytes());
        hasher.update(file_path.as_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        i64::from_be_bytes(bytes)
    }

    async fn lock_create_doc_destination(
        db: &mut ScopedDb,
        workspace_id: Uuid,
        branch_repo_id: Uuid,
        file_path: &str,
    ) -> Result<(), AppError> {
        let lock_key = Self::create_doc_lock_key(workspace_id, branch_repo_id, file_path);
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock_key)
            .execute(db.conn())
            .await
            .map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("Failed to lock create-doc destination: {e}"),
                })
            })?;
        Ok(())
    }

    async fn active_create_doc_destination_exists(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_view: Option<&RepoReadView>,
        branch_repo_id: Uuid,
        file_path: &str,
    ) -> Result<bool, AppError> {
        if let Some(repo_view) = repo_view {
            match repo_view.file_overlay(file_path).await {
                RepoFileOverlay::Deleted => return Ok(false),
                RepoFileOverlay::Content(_) => return Ok(true),
                RepoFileOverlay::Clean => {}
            }
        }

        let indexed: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM docs
                WHERE workspace_id = $1
                  AND repo_id = $2
                  AND file_path = $3
            )
            "#,
        )
        .bind(workspace_id)
        .bind(branch_repo_id)
        .bind(file_path)
        .fetch_one(db.conn())
        .await
        .map_err(|e| AppError::from_error(G5Error::from(e)))?;
        if indexed {
            return Ok(true);
        }

        let Some(repo_view) = repo_view else {
            return Ok(false);
        };
        Ok(matches!(
            repo_view
                .read_file(&self.repo_storage, file_path)
                .await
                .map_err(AppError::from_error)?,
            RepoFileRead::Content { .. }
        ))
    }

    /// Create a new doc in a workspace.
    ///
    /// Persona-view doc types are UNCONDITIONALLY rejected here on every
    /// stack — dev/E2E included (doc_service D18): the former process-wide
    /// `G5_E2E_AUTH` exemption authorized any authenticated dev-stack caller.
    pub async fn create_doc(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        user_id: Uuid,
        req: CreateDocRequest,
    ) -> Result<PreparedDocMutation, AppError> {
        Self::reject_retired_test_view_write(&req.doc_type)?;
        Self::validate_direct_doc_type(&req.doc_type)?;
        self.create_doc_impl(db, workspace_id, user_id, req).await
    }

    /// Doc-create pipeline behind [`Self::create_doc`]; doc-type policy is
    /// enforced by the caller before this point.
    async fn create_doc_impl(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        _user_id: Uuid,
        req: CreateDocRequest,
    ) -> Result<PreparedDocMutation, AppError> {
        // Validate name is not empty/whitespace-only
        if req.name.trim().is_empty() {
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({ "name": "Document name must not be blank" }),
                },
            ));
        }
        let normalized_name = Self::normalize_doc_name(&req.name);
        let normalized_file_path =
            Self::normalize_doc_file_path(&normalized_name, req.file_path.as_deref());
        Self::reject_retired_test_view_path(&normalized_file_path)?;
        let initial_content = req.content.unwrap_or_default();
        let docs_branch = self.require_docs_branch_for_write(db, workspace_id).await?;
        let branch_repo_id = docs_branch.repo_id;
        let repo_view = if is_placeholder_head_commit(&docs_branch.head_commit) {
            None
        } else {
            Some(self.require_docs_repo_read(db, workspace_id).await?.view)
        };
        Self::lock_create_doc_destination(db, workspace_id, branch_repo_id, &normalized_file_path)
            .await?;
        let destination_exists = self
            .active_create_doc_destination_exists(
                db,
                workspace_id,
                repo_view.as_ref(),
                branch_repo_id,
                &normalized_file_path,
            )
            .await?;
        if destination_exists {
            tracing::warn!(
                workspace_id = %workspace_id,
                repo_id = %branch_repo_id,
                file_path = %normalized_file_path,
                "Rejected create_doc because the active destination already exists"
            );
            return Err(AppError::from_error(G5Error::Conflict {
                reason: format!("A project document already exists at {normalized_file_path}"),
            }));
        }
        // Insert the doc metadata row. Canonical content lives in the docs repo.
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<String>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, DateTime<Utc>, DateTime<Utc>)>(
            r#"
            INSERT INTO docs (org_id, workspace_id, repo_id, name, doc_type, file_path, document_source, content, content_updated_at)
            VALUES ((SELECT org_id FROM workspaces WHERE id = $1), $1, $2, $3, $4, $5, 'user', $6, now())
            RETURNING id, workspace_id, name, doc_type, file_path,
                      last_uplifted_at, last_downlifted_at, content_updated_at,
                      created_at, updated_at
            "#,
        )
        .bind(workspace_id)

        .bind(branch_repo_id)
        .bind(&normalized_name)
        .bind(&req.doc_type)
        .bind(&normalized_file_path)
        .bind(None::<String>)
        .fetch_one(db.conn())
        .await
        .map_err(|e| {
            tracing::error!(
                workspace_id = %workspace_id,
                name = %req.name,
                error = %e,
                "Failed to create doc"
            );
            AppError::from_error(g5_contracts::error::G5Error::from(e))
        })?;

        let doc_id = row.0;

        self.update_file_backed_doc_metadata(db, doc_id, &initial_content, None)
            .await?;

        Ok(PreparedDocMutation {
            doc_id,
            file_path: Some(normalized_file_path),
            repo_id: docs_branch.repo_id,
        })
    }

    /// List only view-type docs in a workspace.
    pub async fn list_views(
        &self,
        captured: CapturedDocsList,
    ) -> Result<Vec<ViewListItemDTO>, AppError> {
        Ok(self
            .list_docs(captured)
            .await?
            .into_iter()
            .filter(|row| Self::is_view_doc_type(&row.doc_type))
            .map(|row| ViewListItemDTO {
                id: row.id,
                name: row.name,
                doc_type: row.doc_type,
                file_path: row.file_path,
                is_future: row.is_future,
            })
            .collect())
    }

    /// Get a single view doc and reject non-view docs.
    pub async fn get_view(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        doc_id: Uuid,
    ) -> Result<ViewDetailDTO, AppError> {
        let doc = self.get_doc(db, workspace_id, doc_id).await?;
        if !Self::is_view_doc_type(&doc.doc_type) {
            return Err(Self::non_view_doc_error());
        }
        Ok(ViewDetailDTO { doc })
    }

    /// Reject non-view document types before a view deletion reaches the
    /// attached workspace file facade.
    pub fn ensure_deletable_view_type(doc_type: &str) -> Result<(), AppError> {
        if Self::is_view_doc_type(doc_type) {
            Ok(())
        } else {
            Err(Self::non_view_doc_error())
        }
    }

    /// Delete optional view metadata only after the owner file facade succeeds.
    /// Virtual views have no row; repository-qualified paths prevent cross-repo cleanup.
    pub async fn delete_view_metadata(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        file_path: &str,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM docs WHERE workspace_id = $1 AND repo_id = $2 AND file_path = $3 AND doc_type = ANY($4)")
            .bind(workspace_id)
            .bind(repo_id)
            .bind(file_path)
            .bind(Self::VIEW_DOC_TYPES)
            .execute(db.conn())
            .await?;
        Ok(())
    }
}
