//! Shared repository path policy for G5-owned dot-directories.
//!
//! Root `.g5` contains both durable repository content and local runtime
//! sidecars. Keep this classifier centralized so live workspace file and git paths do not
//! drift into different interpretations of the same files.

use std::path::Path;

/// Fallback slug when a workspace name has no ASCII alphanumerics.
pub const WORKSPACE_PR_BODY_DOC_FALLBACK_SLUG: &str = "workspace";

/// Byte cap on the slug (the same bound as the sync workflow artifact folder
/// slug) so `<slug>.md` stays readable and never approaches the common
/// 255-byte file-name limit: workspace names may be up to 255 characters.
pub const WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN: usize = 64;

/// Slug of the workspace PR-body Project Doc file stem, derived from the
/// workspace name (spec `coordinated_repo_publication` D16): ASCII
/// alphanumerics lowercased, every other character run collapsed to one `-`,
/// leading/trailing `-` trimmed, capped at [`WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN`]
/// bytes (re-trimming a trailing `-` left by the cut), and
/// [`WORKSPACE_PR_BODY_DOC_FALLBACK_SLUG`] when nothing remains. Shared by the server-side reader and the agent-facing
/// route advertisement so the two can never disagree on the file name.
#[must_use]
pub fn workspace_pr_body_doc_slug(workspace_name: &str) -> String {
    let mut slug = String::with_capacity(workspace_name.len());
    let mut prev_dash = false;
    for ch in workspace_name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    // The slug is pure ASCII, so a byte cut never splits a character.
    let slug = slug
        .get(..WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN)
        .unwrap_or(slug)
        .trim_end_matches('-');
    if slug.is_empty() {
        WORKSPACE_PR_BODY_DOC_FALLBACK_SLUG.to_string()
    } else {
        slug.to_string()
    }
}

/// File name (`<slug>.md`) of the workspace PR-body Project Doc.
#[must_use]
pub fn workspace_pr_body_doc_file_name(workspace_name: &str) -> String {
    format!("{}.md", workspace_pr_body_doc_slug(workspace_name))
}

/// Repo-local route of the workspace PR-body Project Doc inside the docs
/// repository: `docs/<workspace_id>/<slug>.md`. Agent surfaces maintain this
/// body-only cumulative PR description across commits and the workspace PR
/// flow reads it at the docs repository's committed snapshot head as the
/// preferred PR body source (spec `coordinated_repo_publication` D16). It
/// replaces the retired per-repo `.g5/pr_body.md`, which is runtime metadata
/// again and is never read.
#[must_use]
pub fn workspace_pr_body_doc_path(workspace_id: &str, workspace_name: &str) -> String {
    format!(
        "docs/{}/{}",
        workspace_id.trim(),
        workspace_pr_body_doc_file_name(workspace_name)
    )
}

/// Canonical `.git/info/exclude` patterns for G5 runtime metadata.
///
/// The broad `.g5/*` ignore keeps unknown future sidecars local. The following
/// unignore rules make the durable repository-owned `.g5` content visible.
/// `.g5/pr_body.md` is intentionally absent: the retired per-repo PR body
/// file is runtime metadata again (the legacy list below still names its
/// unignore line so an older exclude file is recognized and rewritten).
pub const G5_INTERNAL_EXCLUDE_PATTERNS: &[&str] = &[
    ".g5/*",
    "!.g5/workspace.yaml",
    "!.g5/ontology/",
    "!.g5/ontology/**",
    ".g5/ontology/.g5/",
    ".g5/ontology/**/.g5/",
    "!.g5/pr_ontology_diff_design/",
    "!.g5/pr_ontology_diff_design/**",
    ".g5/pr_ontology_diff_design/.git",
    ".g5/pr_ontology_diff_design/**/.git",
    ".g5/pr_ontology_diff_design/.g5",
    ".g5/pr_ontology_diff_design/**/.g5",
    "!.g5/preview_profiles/",
    "!.g5/preview_profiles/*.yaml",
    "!.g5/explainer_templates/",
    "!.g5/explainer_templates/*.yaml",
    "!.g5/workbench/",
    ".g5/workbench/*",
    "!.g5/workbench/actions.yaml",
    "!.g5/workbench/personas/",
    ".g5/workbench/personas/*",
    "!.g5/workbench/personas/*.md",
    "!.g5/experience/",
    "!.g5/experience/**",
    ".g5/experience/.g5/",
    ".g5/experience/**/.g5/",
];

/// Legacy G5 exclude lines that must be rewritten because they hide durable
/// `.g5` content in ways later unignore rules cannot reliably rescue.
pub const LEGACY_G5_INTERNAL_EXCLUDE_PATTERNS: &[&str] = &[
    ".g5",
    ".g5/",
    ".g5/*",
    "!.g5/workspace.yaml",
    "!.g5/ontology/",
    "!.g5/ontology/**",
    ".g5/ontology/.g5/",
    ".g5/ontology/**/.g5/",
    "!.g5/pr_ontology_diff_design/",
    "!.g5/pr_ontology_diff_design/**",
    ".g5/pr_ontology_diff_design/.git",
    ".g5/pr_ontology_diff_design/**/.git",
    ".g5/pr_ontology_diff_design/.g5",
    ".g5/pr_ontology_diff_design/**/.g5",
    // Recognize older directory-only rules when rewriting the exclude file.
    ".g5/pr_ontology_diff_design/.git/",
    ".g5/pr_ontology_diff_design/**/.git/",
    ".g5/pr_ontology_diff_design/.g5/",
    ".g5/pr_ontology_diff_design/**/.g5/",
    "!.g5/pr_body.md",
    "!.g5/preview_profiles/",
    "!.g5/preview_profiles/*.yaml",
    "!.g5/explainer_templates/",
    "!.g5/explainer_templates/*.yaml",
    "!.g5/workbench/",
    ".g5/workbench/*",
    "!.g5/workbench/actions.yaml",
    "!.g5/workbench/personas/",
    ".g5/workbench/personas/*",
    "!.g5/workbench/personas/*.md",
    "!.g5/experience/",
    "!.g5/experience/**",
    ".g5/experience/.g5/",
    ".g5/experience/**/.g5/",
];

/// Normalize a repository-relative path to forward-slash form.
#[must_use]
pub fn normalize_repo_path<P: AsRef<Path>>(path: P) -> String {
    let mut normalized = path.as_ref().to_string_lossy().replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

/// Returns true for durable repository-owned content under root `.g5`.
#[must_use]
pub fn is_durable_g5_repo_content_path<P: AsRef<Path>>(path: P) -> bool {
    let normalized = normalize_repo_path(path);
    is_durable_g5_repo_content_path_str(&normalized)
}

/// Repository-relative directory that holds ontology pages.
pub const ONTOLOGY_ROOT: &str = ".g5/ontology/";

/// Whether a normalized repository-relative path is an ontology file: a
/// `*.g5.md` file at any depth under `.g5/ontology/`. Other files there are
/// ordinary files (`g5.documents.ontology.current-format`).
#[must_use]
pub fn is_ontology_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(ONTOLOGY_ROOT) else {
        return false;
    };
    let mut parts = rest.split('/').peekable();
    let mut valid = !path.contains(['\\', '\0']);
    while let Some(part) = parts.next() {
        valid &= !matches!(part, "" | "." | ".." | ".git" | ".g5");
        if parts.peek().is_none() {
            valid &= part.len() > ".g5.md".len() && part.ends_with(".g5.md");
        }
    }
    valid
}

/// Categories partition changed files by canonical path and workspace member
/// role. Ordinary Markdown and other `.g5` content retain their repository role.
#[must_use]
pub fn change_file_kind(path: &str, repo_role: &str) -> &'static str {
    if is_ontology_path(path) {
        "ontology"
    } else if repo_role == "docs" {
        "docs"
    } else {
        "code"
    }
}

/// Returns true for root `.g5` runtime metadata that should stay local.
#[must_use]
pub fn is_g5_runtime_metadata_path<P: AsRef<Path>>(path: P) -> bool {
    let normalized = normalize_repo_path(path);
    if normalized == ".g5" {
        return true;
    }
    normalized.starts_with(".g5/") && !is_durable_g5_repo_content_path_str(&normalized)
}

/// Shared Files-tree visibility predicate for the g5-api File Tree surface.
///
/// Consumed by both API filter sites (`WorkspaceService::list_file_tree` base
/// git/storage listing and the `routes/files.rs` draft-overlay merge/rebuild)
/// so listing visibility cannot drift between them. A repo path is visible
/// when:
///
/// - it has no hidden component (a dot-prefixed component other than
///   `.semlift` or a dot-basename ending in `.jsonl`), or
/// - it is the `.g5` container directory itself, or
/// - it is durable `.g5` repository content per
///   [`is_durable_g5_repo_content_path`] (`.g5/workspace.yaml`,
///   `.g5/ontology/**`, `.g5/pr_ontology_diff_design/**`,
///   `.g5/preview_profiles/*.yaml`, `.g5/explainer_templates/*.yaml`,
///   `.g5/workbench/actions.yaml`, `.g5/workbench/personas/*.md`,
///   `.g5/experience/**`).
///
/// Runtime `.g5` metadata, `.git/**`, and every other dot-path stay hidden.
/// The visible set is defined by allow-list construction — never enumerate
/// the hidden set, and never reintroduce ad-hoc `starts_with('.')` repo-path
/// filters at consumer sites.
#[must_use]
pub fn is_files_tree_visible_path<P: AsRef<Path>>(path: P) -> bool {
    let normalized = normalize_repo_path(path);
    if normalized.is_empty() {
        return true;
    }
    let has_hidden_component = normalized.split('/').any(|component| {
        component.starts_with('.') && component != ".semlift" && !component.ends_with(".jsonl")
    });
    if !has_hidden_component {
        return true;
    }
    // The `.g5` container row must stay visible so durable children are
    // reachable in the tree; everything else needs the durable allow-list.
    normalized == ".g5" || is_durable_g5_repo_content_path_str(&normalized)
}

fn is_durable_g5_repo_content_path_str(normalized: &str) -> bool {
    for root in [".g5/ontology", ".g5/pr_ontology_diff_design"] {
        if normalized == root {
            return true;
        }
        if let Some(rest) = normalized
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            return !rest.is_empty()
                && rest
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".." | ".git" | ".g5"));
        }
    }
    if normalized == ".g5/workspace.yaml" {
        return true;
    }

    if normalized == ".g5/experience" {
        return true;
    }
    if let Some(rest) = normalized.strip_prefix(".g5/experience/") {
        return !rest.is_empty() && !contains_nested_g5_sidecar(rest);
    }

    if normalized == ".g5/preview_profiles" {
        return true;
    }
    if let Some(rest) = normalized.strip_prefix(".g5/preview_profiles/") {
        return !rest.is_empty() && !rest.contains('/') && rest.ends_with(".yaml");
    }

    if normalized == ".g5/explainer_templates" {
        return true;
    }
    if let Some(rest) = normalized.strip_prefix(".g5/explainer_templates/") {
        return !rest.is_empty() && !rest.contains('/') && rest.ends_with(".yaml");
    }

    // Workbench templates and personas (workbench g5.workbench.actions):
    // exactly `actions.yaml` and `personas/<name>.md`, nothing nested further.
    if normalized == ".g5/workbench" || normalized == ".g5/workbench/actions.yaml" {
        return true;
    }
    if normalized == ".g5/workbench/personas" {
        return true;
    }
    if let Some(rest) = normalized.strip_prefix(".g5/workbench/personas/") {
        return !rest.is_empty() && !rest.contains('/') && rest.ends_with(".md");
    }

    false
}

fn contains_nested_g5_sidecar(rest: &str) -> bool {
    rest == ".g5" || rest.starts_with(".g5/") || rest.ends_with("/.g5") || rest.contains("/.g5/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ontology_files_are_g5_md_pages_under_the_ontology_root() {
        for path in [
            ".g5/ontology/root.g5.md",
            ".g5/ontology/features/documents.g5.md",
            ".g5/ontology/a/b/c/deep.g5.md",
        ] {
            assert!(is_ontology_path(path), "{path}");
        }
        for path in [
            ".g5/ontology/notes.md",
            ".g5/ontology/README.MD",
            ".g5/ontology/.g5.md",
            ".g5/ontology/x.g5.md.bak",
            ".g5/ontology",
            ".g5/ontology/",
            ".g5/chat_intent/x.g5.md",
            "docs/.g5/ontology/x.g5.md",
            ".g5/ontology/../ontology/x.g5.md",
            ".g5/ontology//x.g5.md",
            ".g5/ontology/.git/x.g5.md",
            ".g5/ontology/a/.g5/x.g5.md",
            ".g5/ontology/a\\b.g5.md",
        ] {
            assert!(!is_ontology_path(path), "{path}");
        }
    }

    #[test]
    fn durable_g5_content_is_allowed() {
        for path in [
            ".g5/workspace.yaml",
            ".g5/ontology",
            ".g5/ontology/root.g5.md",
            ".g5/ontology/features/index.md",
            ".g5/pr_ontology_diff_design",
            ".g5/pr_ontology_diff_design/topic",
            ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
            ".g5/pr_ontology_diff_design/topic/mocks/page.html",
            ".g5/pr_ontology_diff_design/topic/screenshots/nested/files.png",
            ".g5/preview_profiles/browser.yaml",
            ".g5/explainer_templates/my-style.yaml",
            ".g5/workbench/actions.yaml",
            ".g5/workbench/personas/be.md",
            ".g5/experience",
            ".g5/experience/_index.md",
            ".g5/experience/agent_llm_tooling/bug_fix/example.md",
        ] {
            assert!(
                is_durable_g5_repo_content_path(path),
                "{path} should be durable .g5 repo content"
            );
            assert!(
                !is_g5_runtime_metadata_path(path),
                "{path} should not be runtime metadata"
            );
        }
    }

    #[test]
    fn g5_runtime_metadata_is_excluded() {
        for path in [
            ".g5",
            ".g5/auth_state.json",
            ".g5/ontology/.g5/state.json",
            ".g5/ontology/nested/.git/config",
            ".g5/ontology/../runtime.json",
            ".g5/pr_ontology_diff_design/.g5/state.json",
            ".g5/pr_ontology_diff_design/topic/.g5/state.json",
            ".g5/pr_ontology_diff_design/topic/.git/config",
            ".g5/pr_ontology_diff_design/../runtime.json",
            ".g5/pr_ontology_diff_design/topic/./plan.md",
            ".g5/pr_ontology_diff_design/topic//plan.md",
            ".g5/pr_ontology_diff_design_old/plan.md",
            ".g5/cli_worker_state.json",
            ".g5/conflicts/src/main.rs",
            ".g5/hook_state/chat_intent/pending.json",
            ".g5/assignment.json",
            ".g5/runtime/rebase.pause.json",
            ".g5/preview_profiles/not-yaml.txt",
            ".g5/preview_profiles/nested/browser.yaml",
            ".g5/explainer_templates/not-yaml.txt",
            ".g5/explainer_templates/nested/my-style.yaml",
            ".g5/workbench/other.yaml",
            ".g5/workbench/actions.yml",
            ".g5/workbench/personas/be.txt",
            ".g5/workbench/personas/nested/be.md",
            ".g5/workbench/nested/actions.yaml",
            // The retired per-repo PR body file is runtime metadata again.
            ".g5/pr_body.md",
            ".g5/pr_body.md.bak",
            ".g5/pr_body.txt",
            ".g5/pr_body/extra.md",
        ] {
            assert!(
                is_g5_runtime_metadata_path(path),
                "{path} should be runtime metadata"
            );
        }
    }

    #[test]
    fn files_tree_visibility_matrix_visible_paths() {
        for path in [
            // Non-dot paths (current behavior preserved).
            "README.md",
            "src/main.rs",
            "docs/spec.md",
            // `.semlift/**` exception preserved.
            ".semlift",
            ".semlift/specs/root.semlift.md",
            // Dot-basename `.jsonl` exception preserved.
            "intent_grounding/ontology/.semlift__specs__root.semlift.md.jsonl",
            // Durable `.g5` content plus its container directory.
            ".g5",
            ".g5/ontology",
            ".g5/ontology/root.g5.md",
            ".g5/ontology/features/index.md",
            ".g5/pr_ontology_diff_design",
            ".g5/pr_ontology_diff_design/topic",
            ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
            ".g5/pr_ontology_diff_design/topic/mocks/page.html",
            ".g5/pr_ontology_diff_design/topic/screenshots/nested/files.png",
            ".g5/workspace.yaml",
            ".g5/preview_profiles",
            ".g5/preview_profiles/browser.yaml",
            ".g5/explainer_templates",
            ".g5/explainer_templates/my-style.yaml",
            ".g5/workbench",
            ".g5/workbench/actions.yaml",
            ".g5/workbench/personas",
            ".g5/workbench/personas/se.md",
            ".g5/experience",
            ".g5/experience/_index.md",
            ".g5/experience/agent_llm_tooling/bug_fix/example.md",
        ] {
            assert!(
                is_files_tree_visible_path(path),
                "{path} should be visible in the files tree"
            );
        }
    }

    #[test]
    fn files_tree_visibility_matrix_hidden_paths() {
        for path in [
            // Runtime `.g5` sidecars stay hidden.
            ".g5/auth_state.json",
            ".g5/hook_state/chat_intent/pending.json",
            ".g5/cache/entry.bin",
            ".g5/conflicts/src/main.rs",
            ".g5/runtime/rebase.pause.json",
            ".g5/pr_ontology_diff_design/.g5/state.json",
            ".g5/pr_ontology_diff_design/topic/.g5/state.json",
            ".g5/pr_ontology_diff_design/topic/.git/config",
            ".g5/pr_ontology_diff_design/../runtime.json",
            ".g5/pr_ontology_diff_design/topic/./plan.md",
            ".g5/pr_ontology_diff_design/topic//plan.md",
            ".g5/pr_ontology_diff_design_old/plan.md",
            ".g5/preview_profiles/not-yaml.txt",
            ".g5/preview_profiles/nested/browser.yaml",
            ".g5/explainer_templates/not-yaml.txt",
            ".g5/explainer_templates/nested/my-style.yaml",
            ".g5/workbench/other.yaml",
            ".g5/workbench/personas/be.txt",
            ".g5/workbench/personas/nested/be.md",
            // The retired per-repo PR body file and its variants stay hidden.
            ".g5/pr_body.md",
            ".g5/pr_body.md.bak",
            ".g5/pr_body/extra.md",
            // Nested tool sidecars under experience stay hidden.
            ".g5/experience/.g5/hook_state/chat.json",
            ".g5/experience/topic/.g5/hook_state/chat.json",
            // Repo control and generic dot-paths stay hidden.
            ".git",
            ".git/config",
            ".github/workflows/ci.yml",
            ".gitignore",
            ".env",
            "src/.hidden/file.rs",
            ".semlift/.cache/tmp",
        ] {
            assert!(
                !is_files_tree_visible_path(path),
                "{path} should stay hidden in the files tree"
            );
        }
    }

    #[test]
    fn workspace_pr_body_doc_slug_normalizes_names() {
        assert_eq!(
            workspace_pr_body_doc_slug("Davide PR/body improvement"),
            "davide-pr-body-improvement"
        );
        assert_eq!(workspace_pr_body_doc_slug("  feat_x  "), "feat-x");
        assert_eq!(
            workspace_pr_body_doc_slug("---"),
            WORKSPACE_PR_BODY_DOC_FALLBACK_SLUG
        );
        assert_eq!(
            workspace_pr_body_doc_slug(""),
            WORKSPACE_PR_BODY_DOC_FALLBACK_SLUG
        );
        assert_eq!(workspace_pr_body_doc_slug("Ünïcode 42"), "n-code-42");
        assert_eq!(workspace_pr_body_doc_file_name("My WS"), "my-ws.md");
    }

    #[test]
    fn workspace_pr_body_doc_slug_fits_file_name_limit() {
        // Longest accepted workspace name (255 chars) of alphanumerics.
        let long = "a".repeat(255);
        let slug = workspace_pr_body_doc_slug(&long);
        assert_eq!(slug, "a".repeat(WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN));
        assert_eq!(
            workspace_pr_body_doc_file_name(&long).len(),
            WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN + ".md".len()
        );
        // A separator landing exactly on the cut boundary is trimmed, and
        // names within the cap are unchanged.
        let boundary = format!("{} {}", "b".repeat(63), "c".repeat(10));
        assert_eq!(workspace_pr_body_doc_slug(&boundary), "b".repeat(63));
        let short = "d".repeat(WORKSPACE_PR_BODY_DOC_SLUG_MAX_LEN);
        assert_eq!(workspace_pr_body_doc_slug(&short), short);
    }

    #[test]
    fn workspace_pr_body_doc_path_is_docs_workspace_scoped() {
        assert_eq!(
            workspace_pr_body_doc_path("ddaa679c-467a-4c09-ba29-b67d323e7117", "Davide PR body"),
            "docs/ddaa679c-467a-4c09-ba29-b67d323e7117/davide-pr-body.md"
        );
        assert_eq!(
            workspace_pr_body_doc_path(" ws-1 ", ""),
            "docs/ws-1/workspace.md"
        );
        // The route lives in the docs repository's Project Docs subtree, never
        // under `.g5`, so the classifier treats it as ordinary visible content.
        let route = workspace_pr_body_doc_path("ws-1", "name");
        assert!(is_files_tree_visible_path(&route));
        assert!(!is_g5_runtime_metadata_path(&route));
    }

    #[test]
    fn canonical_exclude_policy_no_longer_unignores_pr_body() {
        assert!(!G5_INTERNAL_EXCLUDE_PATTERNS.contains(&"!.g5/pr_body.md"));
        // The legacy list keeps the line so older exclude files are rewritten.
        assert!(LEGACY_G5_INTERNAL_EXCLUDE_PATTERNS.contains(&"!.g5/pr_body.md"));
    }

    #[test]
    fn nested_tool_sidecars_under_experience_are_excluded() {
        for path in [
            ".g5/experience/.g5/hook_state/chat.json",
            ".g5/experience/topic/.g5/hook_state/chat.json",
        ] {
            assert!(
                !is_durable_g5_repo_content_path(path),
                "{path} should not be durable experience content"
            );
            assert!(
                is_g5_runtime_metadata_path(path),
                "{path} should remain runtime metadata"
            );
        }
    }

    #[test]
    fn changed_file_categories_use_supported_ontology_then_repository_role() {
        for role in ["code", "docs", "personal"] {
            assert_eq!(change_file_kind(".g5/ontology/a.g5.md", role), "ontology");
            let other = if role == "docs" { "docs" } else { "code" };
            for path in [
                "README.md",
                "assets/logo.png",
                ".g5/ontology/notes.md",
                ".g5/ontology/.g5.md",
                ".g5/ontology/nested/../a.g5.md",
                ".g5/memory/notes.md",
                ".semlift/specs/old.md",
                "docs/outside-managed-root.json",
            ] {
                assert_eq!(change_file_kind(path, role), other, "{role}:{path}");
            }
        }
    }
}
