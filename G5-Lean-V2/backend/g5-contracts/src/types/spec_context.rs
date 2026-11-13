//! `SpecContext` — the ontology binding that scopes an agent task.
//!
//! Contains exactly four fields matching the SemliftSpec YAML frontmatter.
//! Supports merge semantics with left-precedence for scalar fields and
//! union/deduplication for list fields.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::ids::ComponentId;

// ---------------------------------------------------------------------------
// SpecContext
// ---------------------------------------------------------------------------

/// The ontology binding that scopes an agent task. Contains the four core
/// fields matching the SemliftSpec YAML frontmatter, plus two optional
/// commit SHA fields for incremental sync anchoring (staleness_git_diff DD-1).
///
/// This is the core identity structure that tells an agent "what ontology
/// node am I working on?"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecContext {
    /// Hierarchical component ID in the task repo's local ontology namespace.
    ///
    /// Multi-repo tool calls may wrap this in a repo-routed selector at the
    /// boundary (for example `tinydb/tinydb/table`), but task context and spec
    /// frontmatter keep the repo-local value (`tinydb/table`).
    pub component_id: ComponentId,

    /// Repo-local source file paths this spec describes.
    pub src_files: Vec<String>,

    /// Parent spec in this repo's ontology hierarchy.
    pub parent_spec: Option<String>,

    /// Child spec IDs in this repo's ontology hierarchy.
    pub child_specs: Vec<String>,

    /// Git SHA at HEAD when this node was last uplift-synced. None if never uplifted.
    #[serde(default)]
    pub last_uplifted_commit_sha: Option<String>,

    /// Git SHA at HEAD when this node was last downlift-synced. None if never downlifted.
    #[serde(default)]
    pub last_downlifted_commit_sha: Option<String>,
}

impl SpecContext {
    /// Creates a minimal `SpecContext` with empty `src_files`, `None` parent,
    /// and empty `child_specs`.
    #[must_use]
    pub fn new(component_id: ComponentId) -> Self {
        SpecContext {
            component_id,
            src_files: Vec::new(),
            parent_spec: None,
            child_specs: Vec::new(),
            last_uplifted_commit_sha: None,
            last_downlifted_commit_sha: None,
        }
    }

    /// Merges two `SpecContext`s with left-precedence semantics.
    ///
    /// - **Scalar fields** (`component_id`, `parent_spec`): `self` wins
    /// - **List fields** (`src_files`, `child_specs`): union of both,
    ///   deduplicated, sorted; empty strings are filtered out
    ///
    /// The result is a new `SpecContext` — neither input is mutated.
    #[must_use]
    pub fn merge(&self, other: &SpecContext) -> SpecContext {
        // Scalar: self wins (left-precedence)
        let component_id = self.component_id.clone();
        let parent_spec = self.parent_spec.clone();
        let last_uplifted_commit_sha = self
            .last_uplifted_commit_sha
            .clone()
            .or_else(|| other.last_uplifted_commit_sha.clone());
        let last_downlifted_commit_sha = self
            .last_downlifted_commit_sha
            .clone()
            .or_else(|| other.last_downlifted_commit_sha.clone());

        // Lists: union, deduplicate, sort, filter empty strings
        let src_files: Vec<String> = self
            .src_files
            .iter()
            .chain(other.src_files.iter())
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        let child_specs: Vec<String> = self
            .child_specs
            .iter()
            .chain(other.child_specs.iter())
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        SpecContext {
            component_id,
            src_files,
            parent_spec,
            child_specs,
            last_uplifted_commit_sha,
            last_downlifted_commit_sha,
        }
    }

    /// Builder-style setter for `src_files`.
    #[must_use]
    pub fn with_src_files(mut self, files: Vec<String>) -> Self {
        self.src_files = files;
        self
    }

    /// Builder-style setter for `parent_spec`.
    #[must_use]
    pub fn with_parent_spec(mut self, parent: Option<String>) -> Self {
        self.parent_spec = parent;
        self
    }

    /// Builder-style setter for `child_specs`.
    #[must_use]
    pub fn with_child_specs(mut self, children: Vec<String>) -> Self {
        self.child_specs = children;
        self
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl SpecContext {
    #[must_use]
    pub fn test_default() -> Self {
        SpecContext {
            component_id: ComponentId::new("test/component").unwrap(),
            src_files: vec!["src/test.rs".to_string()],
            parent_spec: None,
            child_specs: vec![],
            last_uplifted_commit_sha: None,
            last_downlifted_commit_sha: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_minimal() {
        let id = ComponentId::new("auth/validator").unwrap();
        let ctx = SpecContext::new(id.clone());
        assert_eq!(ctx.component_id, id);
        assert!(ctx.src_files.is_empty());
        assert!(ctx.parent_spec.is_none());
        assert!(ctx.child_specs.is_empty());
    }

    #[test]
    fn test_builder_setters() {
        let id = ComponentId::new("auth/validator").unwrap();
        let ctx = SpecContext::new(id)
            .with_src_files(vec!["src/auth.rs".to_string()])
            .with_parent_spec(Some("auth".to_string()))
            .with_child_specs(vec!["auth/validator/rules".to_string()]);

        assert_eq!(ctx.src_files, vec!["src/auth.rs"]);
        assert_eq!(ctx.parent_spec, Some("auth".to_string()));
        assert_eq!(ctx.child_specs, vec!["auth/validator/rules"]);
    }

    #[test]
    fn test_merge_scalar_left_precedence() {
        let left = SpecContext::new(ComponentId::new("left/component").unwrap())
            .with_parent_spec(Some("left_parent".to_string()));
        let right = SpecContext::new(ComponentId::new("right/component").unwrap())
            .with_parent_spec(Some("right_parent".to_string()));

        let merged = left.merge(&right);
        assert_eq!(merged.component_id.as_str(), "left/component");
        assert_eq!(merged.parent_spec, Some("left_parent".to_string()));
    }

    #[test]
    fn test_merge_lists_union_deduplicated_sorted() {
        let left = SpecContext::new(ComponentId::new("test/a").unwrap())
            .with_src_files(vec!["b.rs".to_string(), "a.rs".to_string()])
            .with_child_specs(vec!["child2".to_string(), "child1".to_string()]);

        let right = SpecContext::new(ComponentId::new("test/b").unwrap())
            .with_src_files(vec!["a.rs".to_string(), "c.rs".to_string()])
            .with_child_specs(vec!["child1".to_string(), "child3".to_string()]);

        let merged = left.merge(&right);

        // Unioned, deduplicated, sorted (BTreeSet guarantees order)
        assert_eq!(merged.src_files, vec!["a.rs", "b.rs", "c.rs"]);
        assert_eq!(merged.child_specs, vec!["child1", "child2", "child3"]);
    }

    #[test]
    fn test_merge_filters_empty_strings() {
        let left = SpecContext::new(ComponentId::new("test/a").unwrap())
            .with_src_files(vec!["a.rs".to_string(), "".to_string()])
            .with_child_specs(vec!["".to_string(), "child1".to_string()]);

        let right = SpecContext::new(ComponentId::new("test/b").unwrap())
            .with_src_files(vec!["".to_string(), "b.rs".to_string()])
            .with_child_specs(vec!["child2".to_string(), "".to_string()]);

        let merged = left.merge(&right);
        assert_eq!(merged.src_files, vec!["a.rs", "b.rs"]);
        assert_eq!(merged.child_specs, vec!["child1", "child2"]);
    }

    #[test]
    fn test_merge_does_not_mutate_inputs() {
        let left = SpecContext::new(ComponentId::new("test/a").unwrap())
            .with_src_files(vec!["a.rs".to_string()]);
        let right = SpecContext::new(ComponentId::new("test/b").unwrap())
            .with_src_files(vec!["b.rs".to_string()]);

        let left_clone = left.clone();
        let right_clone = right.clone();

        let _merged = left.merge(&right);

        assert_eq!(left, left_clone);
        assert_eq!(right, right_clone);
    }

    #[test]
    fn test_serde_camel_case() {
        let ctx = SpecContext::new(ComponentId::new("auth/validator").unwrap())
            .with_src_files(vec!["src/auth.rs".to_string()])
            .with_parent_spec(Some("auth".to_string()))
            .with_child_specs(vec!["auth/rules".to_string()]);

        let json = serde_json::to_value(&ctx).unwrap();

        // Verify camelCase field names
        assert!(json.get("componentId").is_some());
        assert!(json.get("srcFiles").is_some());
        assert!(json.get("parentSpec").is_some());
        assert!(json.get("childSpecs").is_some());

        // Verify snake_case is NOT present
        assert!(json.get("component_id").is_none());
        assert!(json.get("src_files").is_none());
        assert!(json.get("parent_spec").is_none());
        assert!(json.get("child_specs").is_none());
    }

    #[test]
    fn test_serde_roundtrip() {
        let ctx = SpecContext::new(ComponentId::new("auth/validator").unwrap())
            .with_src_files(vec!["src/auth.rs".to_string()])
            .with_parent_spec(Some("auth".to_string()));

        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: SpecContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, parsed);
    }

    #[test]
    fn test_serde_from_camel_case_json() {
        let json = r#"{
            "componentId": "auth/validator",
            "srcFiles": ["src/auth.rs"],
            "parentSpec": "auth",
            "childSpecs": ["auth/rules"]
        }"#;
        let ctx: SpecContext = serde_json::from_str(json).unwrap();
        assert_eq!(ctx.component_id.as_str(), "auth/validator");
        assert_eq!(ctx.src_files, vec!["src/auth.rs"]);
        assert_eq!(ctx.parent_spec, Some("auth".to_string()));
        assert_eq!(ctx.child_specs, vec!["auth/rules"]);
    }

    #[test]
    fn test_test_default() {
        let ctx = SpecContext::test_default();
        assert_eq!(ctx.component_id.as_str(), "test/component");
        assert_eq!(ctx.src_files, vec!["src/test.rs"]);
        assert!(ctx.parent_spec.is_none());
        assert!(ctx.child_specs.is_empty());
    }
}
