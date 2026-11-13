//! Effective document/view reference resolution.
//!
//! Reference semantics come from repository frontmatter, never from the
//! deprecated `doc_node_links` projection. Callers capture one complete
//! workspace and task cut through `workspace_reader`, attach any live workspace file
//! overlays once, and pass those views here.

use std::collections::{BTreeMap, BTreeSet};

use crate::services::git::repo_store::TenantRepoStorage;
use g5_contracts::error::G5Error;
use g5_contracts::types::CommittedBindingSnapshot;
use uuid::Uuid;

use crate::services::workspace_reader::{RepoFileRead, RepoReadView};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDocumentReference {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub repo_slug: String,
    /// Repo-routed selector used at workspace/tool boundaries.
    pub component_id: String,
    /// Repository-local identity stored in SemLift frontmatter and projections.
    pub repo_local_component_id: String,
    pub file_path: String,
    pub node_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectiveDocumentReferences {
    pub has_frontmatter: bool,
    pub raw_references: Vec<String>,
    pub resolved_references: Vec<ResolvedDocumentReference>,
    pub unresolved_references: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OntologyDocumentCandidate {
    repo_id: Uuid,
    workspace_id: Uuid,
    repo_name: String,
    repo_slug: String,
    component_id: String,
    file_path: String,
    node_name: String,
}

/// Resolve references from one effective document in an already-captured cut.
pub async fn resolve_effective_document_references(
    repo_storage: &TenantRepoStorage,
    snapshot: &CommittedBindingSnapshot,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    docs_repo_id: Uuid,
    file_path: &str,
) -> Result<EffectiveDocumentReferences, G5Error> {
    validate_captured_views(snapshot, repo_views)?;
    let docs_view = repo_views
        .get(&docs_repo_id)
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Document repository is not present in the captured authority cut",
                "repo_id": docs_repo_id,
                "target": snapshot.target,
            }),
        })?;
    let content = match docs_view.read_file(repo_storage, file_path).await? {
        RepoFileRead::Content { bytes, .. } => {
            String::from_utf8(bytes).map_err(|error| G5Error::ValidationError {
                details: serde_json::json!({
                    "message": format!("Document content is not UTF-8: {error}"),
                    "path": file_path,
                    "repo_id": docs_repo_id,
                }),
            })?
        }
        RepoFileRead::Deleted | RepoFileRead::Missing => {
            return Err(G5Error::NotFound {
                resource: format!(
                    "Document file '{file_path}' in captured {:?} cut",
                    snapshot.target
                ),
            });
        }
    };

    resolve_document_references_from_content(repo_storage, snapshot, repo_views, &content).await
}

/// Resolve frontmatter already captured by a caller from the same repo view.
pub async fn resolve_document_references_from_content(
    repo_storage: &TenantRepoStorage,
    snapshot: &CommittedBindingSnapshot,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    content: &str,
) -> Result<EffectiveDocumentReferences, G5Error> {
    validate_captured_views(snapshot, repo_views)?;

    let Some(frontmatter) = parse_yaml_frontmatter(content) else {
        return Ok(EffectiveDocumentReferences::default());
    };
    let raw_references = dedup_preserving_order(extract_linked_nodes(&frontmatter));
    let catalog = load_ontology_catalog(repo_storage, snapshot, repo_views).await?;
    let (resolved_references, unresolved_references) =
        resolve_references_from_catalog(&catalog, &raw_references);

    Ok(EffectiveDocumentReferences {
        has_frontmatter: true,
        raw_references,
        resolved_references,
        unresolved_references,
    })
}

/// Resolve several effective documents from one captured authority cut.
///
/// Authority drift is checked once and each distinct selector is resolved once
/// for the batch. Callers retain their own path/id keys.
pub async fn resolve_document_references_batch_from_content(
    repo_storage: &TenantRepoStorage,
    snapshot: &CommittedBindingSnapshot,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
    contents: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, EffectiveDocumentReferences>, G5Error> {
    validate_captured_views(snapshot, repo_views)?;

    let parsed = contents
        .iter()
        .map(|(key, content)| {
            let frontmatter = parse_yaml_frontmatter(content);
            let references = frontmatter
                .as_ref()
                .map(extract_linked_nodes)
                .map(dedup_preserving_order)
                .unwrap_or_default();
            (key.clone(), (frontmatter.is_some(), references))
        })
        .collect::<BTreeMap<_, _>>();
    let all_references = parsed
        .values()
        .flat_map(|(_, references)| references.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let catalog = if all_references.is_empty() {
        Vec::new()
    } else {
        load_ontology_catalog(repo_storage, snapshot, repo_views).await?
    };
    let mut resolution_by_reference = BTreeMap::new();
    for reference in all_references {
        let resolved = resolve_reference_from_catalog(&catalog, &reference);
        resolution_by_reference.insert(reference, resolved);
    }

    let mut result = BTreeMap::new();
    for (key, (has_frontmatter, raw_references)) in parsed {
        let mut resolved_references = Vec::new();
        let mut unresolved_references = Vec::new();
        for reference in &raw_references {
            match resolution_by_reference.get(reference).cloned().flatten() {
                Some(node) => resolved_references.push(node),
                None => unresolved_references.push(reference.clone()),
            }
        }
        resolved_references.sort_by(|left, right| {
            left.node_name
                .cmp(&right.node_name)
                .then_with(|| left.component_id.cmp(&right.component_id))
        });
        resolved_references.dedup_by(|left, right| {
            left.repo_id == right.repo_id && left.component_id == right.component_id
        });
        result.insert(
            key,
            EffectiveDocumentReferences {
                has_frontmatter,
                raw_references,
                resolved_references,
                unresolved_references,
            },
        );
    }
    Ok(result)
}

async fn load_ontology_catalog(
    repo_storage: &TenantRepoStorage,
    snapshot: &CommittedBindingSnapshot,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
) -> Result<Vec<OntologyDocumentCandidate>, G5Error> {
    let mut catalog = Vec::new();
    for binding in &snapshot.repos {
        let view = repo_views
            .get(&binding.repo_id)
            .ok_or_else(|| G5Error::Internal {
                message: format!(
                    "Captured committed view for repository {} disappeared during ontology enumeration",
                    binding.repo_id
                ),
            })?;
        let mut paths = view
            .list_committed_trees(repo_storage, &[(".semlift/specs".to_string(), 64)])
            .await?
            .remove(".semlift/specs")
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .map(|entry| entry.path)
            .filter(|path| is_semlift_spec_path(path))
            .collect::<BTreeSet<_>>();
        paths.extend(
            view.dirty_files()
                .iter()
                .filter(|path| is_semlift_spec_path(path))
                .cloned(),
        );
        let requested = paths.into_iter().collect::<Vec<_>>();
        for (file_path, read) in view.read_files(repo_storage, &requested).await? {
            let RepoFileRead::Content { bytes, .. } = read else {
                continue;
            };
            let content = String::from_utf8(bytes).map_err(|error| G5Error::ValidationError {
                details: serde_json::json!({
                    "message": format!("SemLift spec content is not UTF-8: {error}"),
                    "path": file_path,
                    "repo_id": binding.repo_id,
                    "target": snapshot.target,
                }),
            })?;
            let frontmatter = parse_yaml_frontmatter(&content);
            let component_id = frontmatter
                .as_ref()
                .and_then(extract_component_id)
                .or_else(|| component_id_from_semlift_path(&file_path))
                .ok_or_else(|| G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": "SemLift spec in the captured authority cut has no component_id",
                        "path": file_path,
                        "repo_id": binding.repo_id,
                        "target": snapshot.target,
                    }),
                })?;
            let node_name = ontology_node_name(frontmatter.as_ref(), &content, &component_id);
            catalog.push(OntologyDocumentCandidate {
                repo_id: binding.repo_id,
                workspace_id: binding.workspace_id,
                repo_name: binding.repo_name.clone(),
                repo_slug: binding.repo_slug.clone(),
                component_id,
                file_path,
                node_name,
            });
        }
    }
    catalog.sort_by(|left, right| {
        left.repo_name
            .cmp(&right.repo_name)
            .then_with(|| left.component_id.cmp(&right.component_id))
            .then_with(|| left.file_path.cmp(&right.file_path))
    });
    Ok(catalog)
}

fn resolve_references_from_catalog(
    catalog: &[OntologyDocumentCandidate],
    references: &[String],
) -> (Vec<ResolvedDocumentReference>, Vec<String>) {
    let mut resolved = Vec::with_capacity(references.len());
    let mut unresolved = Vec::new();
    for reference in references {
        match resolve_reference_from_catalog(catalog, reference) {
            Some(node) => resolved.push(node),
            None => unresolved.push(reference.clone()),
        }
    }
    resolved.sort_by(|left, right| {
        left.node_name
            .cmp(&right.node_name)
            .then_with(|| left.component_id.cmp(&right.component_id))
    });
    resolved.dedup_by(|left, right| {
        left.repo_id == right.repo_id && left.component_id == right.component_id
    });
    (resolved, unresolved)
}

fn resolve_reference_from_catalog(
    catalog: &[OntologyDocumentCandidate],
    reference: &str,
) -> Option<ResolvedDocumentReference> {
    let candidate = resolve_ontology_document_reference(catalog, reference)?;
    let repo_key = if candidate.repo_slug.trim().is_empty() {
        candidate.repo_name.as_str()
    } else {
        candidate.repo_slug.as_str()
    };
    Some(ResolvedDocumentReference {
        workspace_id: candidate.workspace_id,
        repo_id: candidate.repo_id,
        repo_name: candidate.repo_name.clone(),
        repo_slug: candidate.repo_slug.clone(),
        component_id: format!("{repo_key}/{}", candidate.component_id),
        repo_local_component_id: candidate.component_id.clone(),
        file_path: candidate.file_path.clone(),
        node_name: candidate.node_name.clone(),
    })
}

fn validate_captured_views(
    snapshot: &CommittedBindingSnapshot,
    repo_views: &BTreeMap<Uuid, RepoReadView>,
) -> Result<(), G5Error> {
    if snapshot.repos.len() != repo_views.len() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Document reference resolution requires the complete captured repository vector",
                "target": snapshot.target,
                "expected_repo_count": snapshot.repos.len(),
                "actual_repo_count": repo_views.len(),
            }),
        });
    }
    for binding in &snapshot.repos {
        let Some(view) = repo_views.get(&binding.repo_id) else {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Captured repository vector is missing a member",
                    "target": snapshot.target,
                    "repo_id": binding.repo_id,
                }),
            });
        };
        if view.repo.workspace_id != binding.workspace_id
            || view.repo.repo_id != binding.repo_id
            || view.repo.head_commit != binding.head_commit
        {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Captured repository view does not match its authoritative binding",
                    "target": snapshot.target,
                    "repo_id": binding.repo_id,
                }),
            });
        }
    }
    Ok(())
}

fn resolve_ontology_document_reference<'a>(
    catalog: &'a [OntologyDocumentCandidate],
    reference: &str,
) -> Option<&'a OntologyDocumentCandidate> {
    let exact = matching_ontology_documents(catalog, reference, None);
    if let Some(candidate) = select_unambiguous_ontology_document(&exact) {
        return Some(candidate);
    }

    let (repo_key, repo_local_ref) = split_repo_scoped_reference(reference)?;
    let scoped_exact = exact
        .into_iter()
        .filter(|(_, candidate)| ontology_repo_matches(candidate, repo_key))
        .collect::<Vec<_>>();
    if let Some(candidate) = select_unambiguous_ontology_document(&scoped_exact) {
        return Some(candidate);
    }

    let scoped = matching_ontology_documents(catalog, repo_local_ref, Some(repo_key));
    select_unambiguous_ontology_document(&scoped)
}

fn matching_ontology_documents<'a>(
    catalog: &'a [OntologyDocumentCandidate],
    reference: &str,
    repo_key: Option<&str>,
) -> Vec<(i32, &'a OntologyDocumentCandidate)> {
    let lookup_keys = node_reference_lookup_keys(reference);
    catalog
        .iter()
        .filter(|candidate| {
            repo_key.is_none_or(|repo_key| ontology_repo_matches(candidate, repo_key))
        })
        .filter_map(|candidate| {
            let rank = if lookup_keys.contains(&candidate.component_id) {
                0
            } else if lookup_keys.contains(&candidate.file_path) {
                1
            } else if lookup_keys.contains(&candidate.node_name) {
                2
            } else {
                return None;
            };
            Some((rank, candidate))
        })
        .collect()
}

fn select_unambiguous_ontology_document<'a>(
    candidates: &[(i32, &'a OntologyDocumentCandidate)],
) -> Option<&'a OntologyDocumentCandidate> {
    let best_rank = candidates.iter().map(|(rank, _)| *rank).min()?;
    let best = candidates
        .iter()
        .filter(|(rank, _)| *rank == best_rank)
        .map(|(_, candidate)| *candidate)
        .collect::<Vec<_>>();
    let unique = best
        .iter()
        .map(|candidate| {
            (
                candidate.workspace_id,
                candidate.repo_id,
                candidate.component_id.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    (unique.len() == 1).then(|| best[0])
}

pub fn parse_yaml_frontmatter(content: &str) -> Option<serde_yaml::Value> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let rest = after_first
        .find('\n')
        .map(|index| &after_first[index + 1..])?;
    let end = rest.find("\n---")?;
    serde_yaml::from_str(&rest[..end]).ok()
}

pub fn extract_linked_nodes(frontmatter: &serde_yaml::Value) -> Vec<String> {
    let Some(mapping) = frontmatter.as_mapping() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for key_name in ["root_spec_ids", "linked_nodes", "linked_specs", "new_nodes"] {
        let key = serde_yaml::Value::String(key_name.to_string());
        if let Some(nodes) = mapping.get(&key).and_then(serde_yaml::Value::as_sequence) {
            result.extend(
                nodes
                    .iter()
                    .filter_map(serde_yaml::Value::as_str)
                    .map(ToString::to_string),
            );
        }
    }
    result
}

fn extract_component_id(frontmatter: &serde_yaml::Value) -> Option<String> {
    frontmatter
        .as_mapping()?
        .get(serde_yaml::Value::String("component_id".to_string()))?
        .as_str()
        .map(str::trim)
        .filter(|component_id| !component_id.is_empty())
        .map(ToString::to_string)
}

fn component_id_from_semlift_path(path: &str) -> Option<String> {
    path.strip_prefix(".semlift/specs/")
        .and_then(|path| path.strip_suffix(".semlift.md"))
        .filter(|component_id| !component_id.is_empty())
        .map(ToString::to_string)
}

fn ontology_node_name(
    frontmatter: Option<&serde_yaml::Value>,
    content: &str,
    component_id: &str,
) -> String {
    if let Some(name) = frontmatter
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|mapping| {
            ["name", "title"].iter().find_map(|key| {
                mapping
                    .get(serde_yaml::Value::String((*key).to_string()))
                    .and_then(serde_yaml::Value::as_str)
            })
        })
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    if let Some(heading) = content
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|heading| !heading.is_empty())
    {
        return heading.to_string();
    }
    component_id
        .rsplit('/')
        .next()
        .unwrap_or(component_id)
        .to_string()
}

fn is_semlift_spec_path(path: &str) -> bool {
    path.starts_with(".semlift/specs/") && path.ends_with(".semlift.md")
}

fn dedup_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn node_reference_lookup_keys(reference: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return keys;
    }
    push_lookup_key(&mut keys, trimmed);

    let mut component = trimmed;
    for prefix in [
        ".semlift/specs/",
        ".g5/specs/",
        "G5_ONTOLOGY/core_ontology/",
        "core_ontology/",
        "G5_ONTOLOGY/",
    ] {
        if let Some(stripped) = component.strip_prefix(prefix) {
            component = stripped;
            break;
        }
    }
    if let Some(stripped) = component.strip_suffix(".semlift.md") {
        component = stripped;
    } else if let Some(stripped) = component.strip_suffix(".g5.md") {
        component = stripped;
    }

    let component = legacy_linked_node_alias(component);
    push_lookup_key(&mut keys, component);
    push_lookup_key(&mut keys, &format!(".semlift/specs/{component}.semlift.md"));
    keys
}

fn legacy_linked_node_alias(component: &str) -> &str {
    match component {
        "core_validation/core_validation" => "core_validation",
        "infrastructure/infrastructure" => "infrastructure",
        "core_validation/validation_metrics" => "infrastructure/validation_metrics",
        "infrastructure/audit" => "infrastructure/audit_log",
        "infrastructure/multi_tenancy" => "infrastructure/tenant_context",
        _ => component,
    }
}

fn push_lookup_key(keys: &mut Vec<String>, key: &str) {
    if !key.is_empty() && !keys.iter().any(|existing| existing == key) {
        keys.push(key.to_string());
    }
}

fn split_repo_scoped_reference(reference: &str) -> Option<(&str, &str)> {
    let (repo_key, repo_local_ref) = reference.split_once('/')?;
    (!repo_key.is_empty() && !repo_local_ref.is_empty()).then_some((repo_key, repo_local_ref))
}

fn ontology_repo_matches(candidate: &OntologyDocumentCandidate, repo_key: &str) -> bool {
    candidate.repo_name == repo_key || candidate.repo_slug == repo_key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_yaml_frontmatter_valid() {
        let content =
            "---\nlinked_nodes:\n  - auth/validator\n  - auth/claims\n---\n# Title\nBody text";
        let fm = parse_yaml_frontmatter(content).expect("should parse");
        let nodes = extract_linked_nodes(&fm);
        assert_eq!(nodes, vec!["auth/validator", "auth/claims"]);
    }

    #[test]
    fn test_parse_yaml_frontmatter_no_frontmatter() {
        let content = "# Just a title\nNo frontmatter here.";
        assert!(parse_yaml_frontmatter(content).is_none());
    }

    #[test]
    fn test_parse_yaml_frontmatter_empty_linked_nodes() {
        let content = "---\ntitle: My Doc\n---\n# Title";
        let fm = parse_yaml_frontmatter(content).expect("should parse");
        let nodes = extract_linked_nodes(&fm);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_yaml_frontmatter_unclosed() {
        let content = "---\nlinked_nodes:\n  - foo\n# Missing closing delimiter";
        assert!(parse_yaml_frontmatter(content).is_none());
    }

    #[test]
    fn test_extract_linked_nodes_not_sequence() {
        let content = "---\nlinked_nodes: not_a_list\n---\n# Title";
        let fm = parse_yaml_frontmatter(content).expect("should parse");
        let nodes = extract_linked_nodes(&fm);
        assert!(nodes.is_empty());
    }

    #[test]
    fn frontmatter_reference_order_is_canonical() {
        let content = concat!(
            "---\n",
            "root_spec_ids:\n  - g5/root\n",
            "linked_nodes:\n  - g5/leaf\n",
            "linked_specs:\n  - g5/peer\n",
            "new_nodes:\n  - g5/future\n",
            "---\n"
        );
        let frontmatter = parse_yaml_frontmatter(content).expect("frontmatter");
        assert_eq!(
            extract_linked_nodes(&frontmatter),
            ["g5/root", "g5/leaf", "g5/peer", "g5/future"]
        );
    }

    #[test]
    fn lookup_keys_normalize_semlift_paths() {
        assert_eq!(
            node_reference_lookup_keys(
                ".semlift/specs/reference_data/receiver_directory.semlift.md"
            ),
            [
                ".semlift/specs/reference_data/receiver_directory.semlift.md",
                "reference_data/receiver_directory",
            ]
        );
    }

    #[test]
    fn lookup_keys_apply_legacy_aliases() {
        assert_eq!(
            node_reference_lookup_keys("infrastructure/audit.g5.md"),
            [
                "infrastructure/audit.g5.md",
                "infrastructure/audit_log",
                ".semlift/specs/infrastructure/audit_log.semlift.md",
            ]
        );
    }

    #[test]
    fn repo_scoped_reference_requires_both_segments() {
        assert_eq!(
            split_repo_scoped_reference("tinydb/tinydb/table"),
            Some(("tinydb", "tinydb/table"))
        );
        assert_eq!(split_repo_scoped_reference("tinydb"), None);
        assert_eq!(split_repo_scoped_reference("/tinydb/table"), None);
        assert_eq!(split_repo_scoped_reference("tinydb/"), None);
    }

    #[test]
    fn canonical_resolution_uses_repo_qualification_to_remove_ambiguity() {
        let branch_a = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let branch_b = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let repo_a = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
        let repo_b = Uuid::parse_str("10000000-0000-0000-0000-000000000002").unwrap();
        let catalog = vec![
            OntologyDocumentCandidate {
                repo_id: repo_a,
                workspace_id: branch_a,
                repo_name: "tinydb".to_string(),
                repo_slug: "tinydb".to_string(),
                component_id: "table".to_string(),
                file_path: ".semlift/specs/table.semlift.md".to_string(),
                node_name: "Table".to_string(),
            },
            OntologyDocumentCandidate {
                repo_id: repo_b,
                workspace_id: branch_b,
                repo_name: "other".to_string(),
                repo_slug: "other".to_string(),
                component_id: "table".to_string(),
                file_path: ".semlift/specs/table.semlift.md".to_string(),
                node_name: "Table".to_string(),
            },
        ];

        assert!(resolve_reference_from_catalog(&catalog, "table").is_none());
        let resolved =
            resolve_reference_from_catalog(&catalog, "tinydb/table").expect("qualified reference");
        assert_eq!(resolved.repo_id, repo_a);
        assert_eq!(resolved.component_id, "tinydb/table");
        assert_eq!(resolved.repo_local_component_id, "table");
    }
}
