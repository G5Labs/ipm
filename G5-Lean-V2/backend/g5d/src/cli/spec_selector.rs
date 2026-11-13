//! Repo-routed spec selector resolution for `g5d spec`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use g5_contracts::error::G5Error;

fn clean_repo_route_key(value: &str) -> Option<String> {
    let key = value.trim().trim_matches('/');
    if key.is_empty() || key == "." {
        None
    } else {
        Some(key.to_string())
    }
}

fn explicit_spec_route_error(component_id: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "field": "component_id",
            "component_id": component_id,
            "message": "multi-repo spec mutations require component_id to include an explicit repo route. Use the exact component_id returned by `g5d spec list`: <repo-key>/<repo-local-component-id>.",
        }),
    }
}

fn non_owning_spec_route_error(component_id: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "field": "component_id",
            "component_id": component_id,
            "message": "the selected repository cannot own ontology specs",
        }),
    }
}

/// D53 (`g5d/cli/sync_spec_remote#D53`, #1013): repo-routed spec selector
/// resolution over an explicit `(repo_root, RepoInfo)` inventory, which the
/// CLI builds from stable-context mounts or `.g5/workspace.yaml`.
#[derive(Debug, Clone)]
pub(crate) struct SpecSelectorResolution {
    pub(crate) repo_root: PathBuf,
    pub(crate) repo_local_component_id: String,
}

fn spec_route_examples_for_roots(
    roots: &[(PathBuf, g5_contracts::types::RepoInfo)],
    component_id: &str,
) -> Vec<String> {
    let mut examples = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, repo) in roots {
        if !repo_can_own_spec(repo) {
            continue;
        }
        let Some(route_key) = preferred_repo_route_key(repo) else {
            continue;
        };
        if seen.insert(route_key.clone()) {
            examples.push(format!("{route_key}/{}", component_id.trim_matches('/')));
        }
    }
    examples
}

/// Resolve a spec selector against an explicit repo inventory
/// (`multi_repo_workspace_ownership#D8`): an explicit `<repo-key>/…` route
/// wins (longest route key; distinct repos at equal length fail closed); a
/// bare selector resolves only with unique ownership — the component exists
/// in exactly one visible code repo, or exactly one code repo is visible —
/// and otherwise fails closed listing the qualified candidates. A repo-local
/// id whose first segment merely equals the repo key by namespace resolves
/// through the route prefix like any other selector (route stripped once,
/// never twice).
pub(crate) fn resolve_spec_selector_in_roots(
    roots: &[(PathBuf, g5_contracts::types::RepoInfo)],
    selector: &str,
) -> Result<SpecSelectorResolution, G5Error> {
    let normalized = selector.trim().trim_matches('/');
    if normalized.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "component_id",
                "message": "spec selector must not be empty",
            }),
        });
    }

    if let Some((repo_root, repo_local_component_id, _)) =
        resolve_explicit_spec_route_in_roots(roots, selector)?
    {
        return Ok(SpecSelectorResolution {
            repo_root,
            repo_local_component_id,
        });
    }

    // Bare selector: unique ownership or fail closed (sync_spec_remote D45
    // extended across repo roots).
    let mut code_repos = Vec::new();
    let mut seen_repo_ids = BTreeSet::new();
    for (repo_root, repo) in roots {
        if !repo_can_own_spec(repo) || !seen_repo_ids.insert(repo.repo_id.clone()) {
            continue;
        }
        code_repos.push((repo_root.clone(), repo.clone()));
    }
    if code_repos.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "component_id",
                "component_id": selector,
                "message": "no visible code repo can own ontology components",
            }),
        });
    }
    let existing: Vec<_> = code_repos
        .iter()
        .filter(|(repo_root, _)| spec_file_exists_in_repo(repo_root, normalized))
        .cloned()
        .collect();
    match existing.len() {
        1 => {
            let (repo_root, _) = existing.into_iter().next().expect("one match");
            return Ok(SpecSelectorResolution {
                repo_root,
                repo_local_component_id: normalized.to_string(),
            });
        }
        count if count > 1 => {
            return Err(G5Error::Conflict {
                reason: format!(
                    "bare ontology component_id '{normalized}' exists in multiple visible code repos; use an explicit repo route such as {}",
                    spec_route_examples_for_roots(&existing, normalized).join(", ")
                ),
            });
        }
        _ => {}
    }
    match code_repos.len() {
        1 => {
            let (repo_root, _) = code_repos.into_iter().next().expect("one code repo");
            Ok(SpecSelectorResolution {
                repo_root,
                repo_local_component_id: normalized.to_string(),
            })
        }
        _ => Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "component_id",
                "component_id": selector,
                "message": "bare ontology component_id is ambiguous across multiple visible code repos; use an explicit repo route",
                "examples": spec_route_examples_for_roots(roots, normalized),
            }),
        }),
    }
}

fn repo_selector_keys(repo: &g5_contracts::types::RepoInfo) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let component_key = repo_component_key(repo);
    if !component_key.is_empty() {
        keys.insert(component_key);
    }
    if let Some(repo_name) = clean_repo_route_key(&repo.name) {
        keys.insert(repo_name);
    }
    keys
}

fn selector_has_route_prefix(selector: &str, route_key: &str) -> bool {
    selector
        .strip_prefix(route_key)
        .is_some_and(|rest| rest.starts_with('/'))
}

fn strip_selector_route<'a>(selector: &'a str, route_key: &str) -> &'a str {
    selector
        .strip_prefix(route_key)
        .unwrap_or(selector)
        .trim_start_matches('/')
}

/// Resolve only the explicit repo-route portion of a spec selector: longest
/// prefix wins, a non-owning repo is rejected, and distinct repos at equal
/// length fail closed.
fn resolve_explicit_spec_route_in_roots(
    roots: &[(PathBuf, g5_contracts::types::RepoInfo)],
    selector_component_id: &str,
) -> Result<Option<(PathBuf, String, g5_contracts::types::RepoInfo)>, G5Error> {
    let normalized_selector = selector_component_id.trim().trim_matches('/');
    let mut matches = Vec::new();
    let mut matched_non_owner = false;

    for (repo_root, repo) in roots {
        for route_key in repo_selector_keys(repo) {
            if selector_has_route_prefix(normalized_selector, &route_key) {
                if !repo_can_own_spec(repo) {
                    matched_non_owner = true;
                    continue;
                }
                let repo_local_component_id =
                    strip_selector_route(normalized_selector, &route_key).to_string();
                if repo_local_component_id.is_empty() {
                    return Err(explicit_spec_route_error(selector_component_id));
                }
                matches.push((
                    route_key,
                    repo_root.clone(),
                    repo.clone(),
                    repo_local_component_id,
                ));
            }
        }
    }

    if matches.is_empty() {
        if matched_non_owner {
            return Err(non_owning_spec_route_error(selector_component_id));
        }
        return Ok(None);
    }

    matches.sort_by(|(left_key, _, _, _), (right_key, _, _, _)| {
        right_key
            .split('/')
            .count()
            .cmp(&left_key.split('/').count())
            .then_with(|| right_key.len().cmp(&left_key.len()))
    });
    let longest_segments = matches[0].0.split('/').count();
    let longest_len = matches[0].0.len();
    let longest: Vec<_> = matches
        .into_iter()
        .filter(|(key, _, _, _)| {
            key.split('/').count() == longest_segments && key.len() == longest_len
        })
        .collect();
    let repo_ids: BTreeSet<String> = longest
        .iter()
        .map(|(_, _, repo, _)| repo.repo_id.clone())
        .collect();
    if repo_ids.len() > 1 {
        return Err(G5Error::Conflict {
            reason: format!(
                "Spec selector '{}' matches multiple visible repo route prefixes; use a unique repo prefix",
                selector_component_id
            ),
        });
    }

    let (_, repo_root, repo, repo_local_component_id) =
        longest.into_iter().next().expect("non-empty route matches");
    Ok(Some((repo_root, repo_local_component_id, repo)))
}

fn repo_can_own_spec(repo: &g5_contracts::types::RepoInfo) -> bool {
    repo.repo_role != g5_contracts::types::RepoRole::Docs
}

fn preferred_repo_route_key(repo: &g5_contracts::types::RepoInfo) -> Option<String> {
    repo_selector_keys(repo).into_iter().find(|key| key != ".")
}

fn spec_file_exists_in_repo(repo_root: &Path, component_id: &str) -> bool {
    let canonical = format!(".semlift/specs/{}.semlift.md", component_id);
    let legacy = format!(".g5/specs/{}.md", component_id);
    repo_root.join(canonical).exists() || repo_root.join(legacy).exists()
}

fn repo_component_key(repo: &g5_contracts::types::RepoInfo) -> String {
    repo.root_path
        .as_deref()
        .and_then(clean_repo_route_key)
        .or_else(|| {
            repo.root_paths
                .iter()
                .find_map(|root| clean_repo_route_key(root))
        })
        .or_else(|| clean_repo_route_key(&repo.name))
        .unwrap_or_else(|| repo.repo_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::types::RepoInfo;
    use tempfile::TempDir;

    fn inventory_repo(
        repo_id: &str,
        name: &str,
        mount: &str,
        role: g5_contracts::types::RepoRole,
    ) -> RepoInfo {
        RepoInfo {
            repo_id: repo_id.to_string(),
            name: name.to_string(),
            root_path: Some(mount.to_string()),
            root_paths: vec![mount.to_string()],
            repo_role: role,
            git_url: None,
            branch_name: None,
            base_commit: None,
        }
    }

    fn seed_spec(root: &Path, component_id: &str) {
        let path = root.join(format!(".semlift/specs/{component_id}.semlift.md"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("---\ncomponent_id: {component_id}\n---\n# Spec\n"),
        )
        .unwrap();
    }

    /// D53 (#1013): routed selectors resolve to the owning repo with the
    /// route stripped once, including the namespace case where the
    /// repo-local id begins with the repo key text.
    #[test]
    fn resolve_spec_selector_in_roots_routes_and_namespaces() {
        let dir = TempDir::new().unwrap();
        let code_root = dir.path().join("g5");
        let docs_root = dir.path().join("docs");
        std::fs::create_dir_all(&code_root).unwrap();
        std::fs::create_dir_all(&docs_root).unwrap();
        seed_spec(&code_root, "g5/g5-api/routes/tasks");
        let roots = vec![
            (
                code_root.clone(),
                inventory_repo("repo-code", "g5", "g5", g5_contracts::types::RepoRole::Code),
            ),
            (
                docs_root,
                inventory_repo(
                    "repo-docs",
                    "g5-docs",
                    "docs",
                    g5_contracts::types::RepoRole::Docs,
                ),
            ),
        ];

        // Routed selector: route stripped exactly once; the remaining
        // namespace segment equal to the repo key survives (E2E-12 / R14).
        let resolved = resolve_spec_selector_in_roots(&roots, "g5/g5/g5-api/routes/tasks").unwrap();
        assert_eq!(resolved.repo_local_component_id, "g5/g5-api/routes/tasks");
        assert_eq!(resolved.repo_root, code_root);
    }

    /// D53: a bare selector resolves through unique on-disk ownership; when
    /// the same repo-local id exists in two visible code repos it fails
    /// closed listing qualified candidates (D45 extended across roots).
    #[test]
    fn resolve_spec_selector_in_roots_bare_unique_and_duplicate() {
        let dir = TempDir::new().unwrap();
        let a_root = dir.path().join("alpha");
        let b_root = dir.path().join("beta");
        std::fs::create_dir_all(&a_root).unwrap();
        std::fs::create_dir_all(&b_root).unwrap();
        seed_spec(&a_root, "shared/thing");
        let c_root = dir.path().join("gamma");
        std::fs::create_dir_all(&c_root).unwrap();
        let roots = vec![
            (
                a_root.clone(),
                inventory_repo(
                    "repo-a",
                    "alpha",
                    "alpha",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                b_root.clone(),
                inventory_repo(
                    "repo-b",
                    "beta",
                    "beta",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                c_root,
                inventory_repo(
                    "repo-c",
                    "gamma",
                    "gamma",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
        ];

        // Unique on-disk ownership resolves the bare selector.
        let resolved = resolve_spec_selector_in_roots(&roots, "shared/thing").unwrap();
        assert_eq!(resolved.repo_root, a_root);

        // Duplicate logical ids across repos fail closed with candidates.
        seed_spec(&b_root, "shared/thing");
        let err = resolve_spec_selector_in_roots(&roots, "shared/thing").unwrap_err();
        let message = format!("{err:?}");
        assert!(message.contains("alpha/shared/thing"), "{message}");
        assert!(message.contains("beta/shared/thing"), "{message}");
        // A code repo without the spec is not offered as a candidate.
        assert!(!message.contains("gamma/"), "{message}");
    }

    /// D53: a bare selector that exists nowhere is ambiguous across multiple
    /// visible code repos and fails BEFORE any write, listing candidates;
    /// with exactly one code repo it auto-resolves (single-repo
    /// compatibility).
    #[test]
    fn resolve_spec_selector_in_roots_new_component_ownership() {
        let dir = TempDir::new().unwrap();
        let a_root = dir.path().join("alpha");
        let b_root = dir.path().join("beta");
        let docs_root = dir.path().join("docs");
        std::fs::create_dir_all(&a_root).unwrap();
        std::fs::create_dir_all(&b_root).unwrap();
        std::fs::create_dir_all(&docs_root).unwrap();
        let multi = vec![
            (
                a_root.clone(),
                inventory_repo(
                    "repo-a",
                    "alpha",
                    "alpha",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                b_root,
                inventory_repo(
                    "repo-b",
                    "beta",
                    "beta",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
        ];
        let err = resolve_spec_selector_in_roots(&multi, "brand/new").unwrap_err();
        let message = format!("{err:?}");
        assert!(message.contains("alpha/brand/new"), "{message}");
        assert!(message.contains("beta/brand/new"), "{message}");

        // One code repo + one docs repo: the docs repo cannot own specs, so
        // the single code repo auto-resolves.
        let single = vec![
            (
                a_root.clone(),
                inventory_repo(
                    "repo-a",
                    "alpha",
                    "alpha",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                docs_root,
                inventory_repo(
                    "repo-docs",
                    "docs",
                    "docs",
                    g5_contracts::types::RepoRole::Docs,
                ),
            ),
        ];
        let resolved = resolve_spec_selector_in_roots(&single, "brand/new").unwrap();
        assert_eq!(resolved.repo_local_component_id, "brand/new");
        assert_eq!(resolved.repo_root, a_root);
    }

    #[test]
    fn resolve_spec_selector_in_roots_rejects_non_owning_routes() {
        let dir = TempDir::new().unwrap();
        let code_root = dir.path().join("alpha");
        let docs_root = dir.path().join("docs");
        std::fs::create_dir_all(&code_root).unwrap();
        std::fs::create_dir_all(&docs_root).unwrap();
        let roots = vec![
            (
                code_root,
                inventory_repo(
                    "repo-a",
                    "alpha",
                    "alpha",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                docs_root,
                inventory_repo(
                    "repo-docs",
                    "docs",
                    "docs",
                    g5_contracts::types::RepoRole::Docs,
                ),
            ),
        ];

        let err = resolve_spec_selector_in_roots(&roots, "docs/brand/new").unwrap_err();
        assert!(
            format!("{err:?}").contains("cannot own ontology specs"),
            "{err:?}"
        );
    }

    #[test]
    fn resolve_spec_selector_in_roots_fails_closed_for_shared_route_keys() {
        let dir = TempDir::new().unwrap();
        let first_root = dir.path().join("first");
        let second_root = dir.path().join("second");
        std::fs::create_dir_all(&first_root).unwrap();
        std::fs::create_dir_all(&second_root).unwrap();
        let roots = vec![
            (
                first_root,
                inventory_repo(
                    "repo-first",
                    "shared",
                    "shared",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
            (
                second_root,
                inventory_repo(
                    "repo-second",
                    "shared",
                    "shared",
                    g5_contracts::types::RepoRole::Code,
                ),
            ),
        ];

        let err = resolve_spec_selector_in_roots(&roots, "shared/brand/new").unwrap_err();
        assert!(
            format!("{err:?}").contains("matches multiple visible repo route prefixes"),
            "{err:?}"
        );
    }

    #[test]
    fn resolve_spec_selector_in_roots_rejects_docs_only_inventory() {
        let dir = TempDir::new().unwrap();
        let docs_root = dir.path().join("docs");
        std::fs::create_dir_all(&docs_root).unwrap();
        let roots = vec![(
            docs_root,
            inventory_repo(
                "repo-docs",
                "docs",
                "docs",
                g5_contracts::types::RepoRole::Docs,
            ),
        )];

        let err = resolve_spec_selector_in_roots(&roots, "brand/new").unwrap_err();
        assert!(
            format!("{err:?}").contains("no visible code repo can own ontology components"),
            "{err:?}"
        );
    }
}
