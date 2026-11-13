//! `g5d spec` — Spec management CLI subcommands for external agents.
//!
//! Provides `create`, `validate`, and `list` subcommands for managing
//! SemliftSpec files without requiring the full internal agent loop.
//!
//! Specs are stored in `.semlift/specs/<component_id>.semlift.md` with
//! YAML frontmatter + markdown body.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use g5_contracts::error::G5Error;
use g5_contracts::types::ontology_markup::{
    duplicate_id_blocks, parse_ontology_document, validate_ontology_corpus, IdBlockFact,
    IdBlockKind, OntologyLintDiagnostic, OntologyLintInput, OntologyLintOptions,
    OntologyLintReport, OntologyLintSeverity, OntologyMarkupDocument, OntologyMarkupSummary,
    ReferencePlacement, ReferenceSpecificity, SemanticRecordMetadata,
};
use g5_contracts::types::ontology_resolution::{
    estimate_ontology_tokens, resolve_ontology_bundle, OntologyReadMode, OntologyReadOptions,
    OntologySourceDocument,
};

use super::workspace_manifest::parse_repo_id_from_git_url;
use super::{stable_context, GlobalFlags, OntologyLintMode, SpecArgs, SpecCommand};
use crate::cli::config::{WorkspaceConfig, WorkspaceRepoEntry};

// ─── Frontmatter parsing (standalone, no TaskContext) ────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct SpecFrontmatter {
    component_id: String,
    #[serde(default)]
    src_files: Vec<String>,
    #[serde(default)]
    source_files: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    dependency_spec_ids: Vec<String>,
    #[serde(default)]
    parent_spec_id: Option<String>,
    #[serde(default)]
    child_spec_ids: Vec<String>,
    #[serde(default, rename = "language")]
    _language: Option<String>,
    #[serde(flatten)]
    _extra: HashMap<String, serde_yaml::Value>,
}

impl SpecFrontmatter {
    fn source_paths(&self) -> Vec<&str> {
        if self.src_files.is_empty() {
            self.source_files.iter().map(String::as_str).collect()
        } else {
            self.src_files.iter().map(String::as_str).collect()
        }
    }

    fn dependency_refs(&self) -> Vec<&str> {
        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        for dep in self
            .dependencies
            .iter()
            .chain(self.dependency_spec_ids.iter())
        {
            if seen.insert(dep.as_str()) {
                refs.push(dep.as_str());
            }
        }
        refs
    }
}

fn extract_frontmatter(content: &str) -> Result<&str, String> {
    if !content.starts_with("---") {
        return Err("Missing YAML frontmatter (must start with '---')".into());
    }
    let rest = &content[3..];
    let closing = rest
        .find("---")
        .ok_or_else(|| "Malformed YAML frontmatter (missing closing '---')".to_string())?;
    Ok(&rest[..closing])
}

fn parse_frontmatter(content: &str) -> Result<SpecFrontmatter, String> {
    let fm = extract_frontmatter(content)?;
    serde_yaml::from_str::<SpecFrontmatter>(fm)
        .map_err(|e| format!("Invalid YAML frontmatter: {}", e))
}

fn parse_dependencies(content: &str) -> Vec<String> {
    parse_frontmatter(content)
        .map(|fm| {
            fm.dependency_refs()
                .into_iter()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ─── Spec path resolution ────────────────────────────────────────────────────

fn spec_path(workspace: &Path, component_id: &str) -> PathBuf {
    let semlift = workspace
        .join(".semlift")
        .join("specs")
        .join(format!("{}.semlift.md", component_id));
    let g5 = workspace
        .join(".g5")
        .join("specs")
        .join(format!("{}.md", component_id));

    if semlift.exists() {
        semlift
    } else if g5.exists() {
        g5
    } else {
        semlift // default for new specs
    }
}

fn find_specs_dir(workspace: &Path) -> Option<PathBuf> {
    let semlift = workspace.join(".semlift").join("specs");
    let g5 = workspace.join(".g5").join("specs");
    if semlift.is_dir() {
        Some(semlift)
    } else if g5.is_dir() {
        Some(g5)
    } else {
        None
    }
}

fn specs_dir_for_spec_path(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) != Some("specs") {
            continue;
        }
        let Some(metadata_dir) = ancestor.parent().and_then(|parent| parent.file_name()) else {
            continue;
        };
        if metadata_dir == ".semlift" || metadata_dir == ".g5" {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

// ─── Cycle detection ─────────────────────────────────────────────────────────

fn detect_dependency_cycle(
    workspace: &Path,
    component_id: &str,
    dependencies: &[String],
) -> Result<(), String> {
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(component_id.to_string());

    let mut parent_map: HashMap<String, String> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for dep in dependencies {
        queue.push_back(dep.clone());
        parent_map.insert(dep.clone(), component_id.to_string());
    }

    while let Some(dep_id) = queue.pop_front() {
        if dep_id == component_id {
            let mut chain = vec![component_id.to_string()];
            let mut walker = parent_map.get(&dep_id).cloned().unwrap_or_default();
            let mut seen = HashSet::new();
            while walker != component_id && !walker.is_empty() {
                if seen.contains(&walker) {
                    break;
                }
                seen.insert(walker.clone());
                chain.push(walker.clone());
                walker = parent_map.get(&walker).cloned().unwrap_or_default();
            }
            chain.push(component_id.to_string());
            chain.reverse();
            return Err(format!("Dependency cycle: {}", chain.join(" → ")));
        }
        if visited.contains(&dep_id) {
            continue;
        }
        visited.insert(dep_id.clone());

        let dep_path = spec_path(workspace, &dep_id);
        if let Ok(dep_content) = std::fs::read_to_string(&dep_path) {
            for trans_dep in parse_dependencies(&dep_content) {
                if !visited.contains(&trans_dep) || trans_dep == component_id {
                    parent_map
                        .entry(trans_dep.clone())
                        .or_insert(dep_id.clone());
                    queue.push_back(trans_dep);
                }
            }
        }
    }
    Ok(())
}

// ─── Workspace resolution ────────────────────────────────────────────────────

fn resolve_workspace(global: &GlobalFlags) -> Result<PathBuf, G5Error> {
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("Failed to get cwd: {}", e),
    })?;
    super::resolve_workspace_path(global.workspace.as_ref(), &cwd).ok_or_else(|| {
        G5Error::Internal {
            message: "No workspace found. Run from a workspace directory or use --workspace."
                .into(),
        }
    })
}

fn resolve_spec_workspace(global: &GlobalFlags) -> Result<PathBuf, G5Error> {
    if let Some(stable) = stable_context::discover(global)? {
        if !explicit_classic_workspace_outside_stable_root(global, &stable)? {
            return Ok(stable.root);
        }
    }
    resolve_workspace(global)
}

fn find_managed_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if current.join(".g5").join("workspace.yaml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// CLI-visible repo inventory for repo-routed spec selector resolution
/// (sync_spec_remote D53, #1013). Sources, in order: stable-context mounts
/// (task workspaces), then `.g5/workspace.yaml` managed repos (Local Setup)
/// — the same sources the CLI tool surface already uses. Single-root
/// contexts (bare git checkout, explicit `--workspace`) carry no repo
/// metadata so bare selectors keep byte- and exit-code-identical legacy
/// behavior.
enum SpecCliInventory {
    SingleRoot,
    Repos(Vec<(PathBuf, g5_contracts::types::RepoInfo)>),
}

fn repo_info_for_inventory(
    repo_id: String,
    name: String,
    mount_path: &str,
    repo_role: g5_contracts::types::RepoRole,
) -> g5_contracts::types::RepoInfo {
    let mount = mount_path.trim().trim_matches('/').to_string();
    g5_contracts::types::RepoInfo {
        repo_id,
        name,
        root_path: (!mount.is_empty()).then(|| mount.clone()),
        root_paths: if mount.is_empty() {
            Vec::new()
        } else {
            vec![mount]
        },
        repo_role,
        git_url: None,
        branch_name: None,
        base_commit: None,
    }
}

fn spec_cli_inventory(global: &GlobalFlags) -> Result<SpecCliInventory, G5Error> {
    if global.workspace.is_none() {
        // Task workspaces: stable-context mount metadata is the ownership
        // inventory (multi_repo_workspace_ownership D8 — metadata, not
        // string shape).
        if let Some(stable) = stable_context::discover(global)? {
            if !stable.metadata.mounts.is_empty() {
                let mut repos = Vec::new();
                for mount in &stable.metadata.mounts {
                    let Some(path) = stable_context::mount_path_for_repo(&stable, &mount.repo_id)?
                    else {
                        continue;
                    };
                    repos.push((
                        path,
                        repo_info_for_inventory(
                            mount.repo_id.clone(),
                            mount
                                .repo_name
                                .clone()
                                .unwrap_or_else(|| mount.mount_path.clone()),
                            &mount.mount_path,
                            mount.repo_role,
                        ),
                    ));
                }
                if !repos.is_empty() {
                    return Ok(SpecCliInventory::Repos(repos));
                }
            }
            return Ok(SpecCliInventory::SingleRoot);
        }

        // Local Setup: the enclosing `.g5/workspace.yaml` names every managed
        // repo with its mount path; a repo-qualified selector must not become
        // a literal deeper path (#1013).
        let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
            message: format!("Failed to get cwd: {}", e),
        })?;
        if let Some(root) = find_managed_workspace_root(&cwd) {
            let ws_config = WorkspaceConfig::load(&root).map_err(|e| G5Error::Internal {
                message: format!(
                    "Failed to load managed workspace config at '{}': {e}",
                    root.display()
                ),
            })?;
            if let Some(ws_config) = ws_config {
                if !ws_config.repos.is_empty() {
                    let docs_path = ws_config
                        .docs
                        .as_ref()
                        .map(|docs| docs.path.trim_matches('/').to_string());
                    let mut repos = Vec::new();
                    for entry in &ws_config.repos {
                        let repo_id = parse_repo_id_from_git_url(&entry.url)
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| entry.name.clone());
                        let role = if docs_path.as_deref() == Some(entry.path.trim_matches('/')) {
                            g5_contracts::types::RepoRole::Docs
                        } else {
                            g5_contracts::types::RepoRole::Code
                        };
                        repos.push((
                            root.join(&entry.path),
                            repo_info_for_inventory(repo_id, entry.name.clone(), &entry.path, role),
                        ));
                    }
                    return Ok(SpecCliInventory::Repos(repos));
                }
            }
        }
    }

    resolve_workspace(global).map(|_| SpecCliInventory::SingleRoot)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn explicit_classic_workspace_outside_stable_root(
    global: &GlobalFlags,
    stable: &stable_context::StableContext,
) -> Result<bool, G5Error> {
    let Some(explicit) = global.workspace.as_ref() else {
        return Ok(false);
    };
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("Failed to get cwd: {}", e),
    })?;
    let explicit = if explicit.is_absolute() {
        explicit.clone()
    } else {
        cwd.join(explicit)
    };
    if !explicit.join(".g5").join("workspace.yaml").exists() {
        return Ok(false);
    }
    Ok(canonical_or_self(&explicit) != canonical_or_self(&stable.root))
}

fn selector_has_route_prefix(selector: &str, route_key: &str) -> bool {
    let selector = selector.trim().trim_matches('/');
    let route_key = route_key.trim().trim_matches('/');
    !route_key.is_empty()
        && selector
            .strip_prefix(route_key)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn repo_entry_route_keys(entry: &WorkspaceRepoEntry) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let name = entry.name.trim().trim_matches('/');
    if !name.is_empty() {
        keys.insert(name.to_string());
    }
    let path = entry.path.trim().trim_matches('/');
    if !path.is_empty() && path != "." {
        keys.insert(path.to_string());
    }
    keys
}

fn managed_workspace_requires_repo_route(ws_config: &WorkspaceConfig) -> bool {
    ws_config.docs.is_some() || ws_config.repos.len() > 1
}

fn strip_selector_route(selector: &str, route_key: &str) -> Option<String> {
    if !selector_has_route_prefix(selector, route_key) {
        return None;
    }
    Some(
        selector
            .trim()
            .trim_matches('/')
            .strip_prefix(route_key.trim().trim_matches('/'))
            .unwrap_or(selector)
            .trim_start_matches('/')
            .to_string(),
    )
    .filter(|local| !local.is_empty())
}

#[derive(Debug, Clone)]
struct ResolvedSpecCreateTarget {
    workspace: PathBuf,
    repo_local_component_id: String,
    selector_component_id: String,
    route_key: Option<String>,
}

#[derive(Debug, Clone)]
struct SpecListRoot {
    workspace: PathBuf,
    route_key: Option<String>,
    repo_id: Option<String>,
    repo_name: Option<String>,
}

fn stable_mount_route_keys(
    mount: &stable_context::StableWorkspaceMountMetadata,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    if let Some(name) = mount.repo_name.as_deref() {
        let name = name.trim().trim_matches('/');
        if !name.is_empty() {
            keys.insert(name.to_string());
        }
    }
    let path = mount.mount_path.trim().trim_matches('/');
    if !path.is_empty() && path != "." {
        keys.insert(path.to_string());
    }
    keys
}

pub(crate) fn primary_stable_mount_route_key(
    mount: &stable_context::StableWorkspaceMountMetadata,
) -> Option<String> {
    mount
        .repo_name
        .as_deref()
        .map(str::trim)
        .map(|name| name.trim_matches('/'))
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            let path = mount.mount_path.trim().trim_matches('/');
            (!path.is_empty() && path != ".").then(|| path.to_string())
        })
}

fn resolve_stable_spec_create_target(
    stable: &stable_context::StableContext,
    component_id: &str,
) -> Result<ResolvedSpecCreateTarget, G5Error> {
    let selector = component_id.trim().trim_matches('/');
    if selector.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "component_id",
                "component_id": component_id,
                "message": "spec component id must not be empty",
            }),
        });
    }

    if stable.metadata.mounts.is_empty() {
        return Ok(ResolvedSpecCreateTarget {
            workspace: stable_context::fallback_single_repo_path(stable),
            repo_local_component_id: selector.to_string(),
            selector_component_id: selector.to_string(),
            route_key: None,
        });
    }

    let mut matches = Vec::new();
    for mount in &stable.metadata.mounts {
        for route_key in stable_mount_route_keys(mount) {
            if let Some(local_component_id) = strip_selector_route(selector, &route_key) {
                matches.push((route_key, mount, local_component_id));
            }
        }
    }

    if matches.is_empty() {
        let repo_routes = stable
            .metadata
            .mounts
            .iter()
            .flat_map(stable_mount_route_keys)
            .collect::<Vec<_>>();
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "component_id",
                "component_id": component_id,
                "message": "provided-agent multi-repo spec mutation requires a repo-qualified component id: <repo>/<component>",
                "repoRoutes": repo_routes,
            }),
        });
    }

    matches.sort_by(|(left, _, _), (right, _, _)| {
        right
            .split('/')
            .count()
            .cmp(&left.split('/').count())
            .then_with(|| right.len().cmp(&left.len()))
    });
    let best_segments = matches[0].0.split('/').count();
    let best_len = matches[0].0.len();
    let best = matches
        .into_iter()
        .filter(|(key, _, _)| key.split('/').count() == best_segments && key.len() == best_len)
        .collect::<Vec<_>>();
    let repo_ids = best
        .iter()
        .map(|(_, mount, _)| mount.repo_id.as_str())
        .collect::<BTreeSet<_>>();
    if repo_ids.len() > 1 {
        return Err(G5Error::Conflict {
            reason: format!(
                "component_id '{}' matches multiple visible repo routes; use a unique repo prefix",
                component_id
            ),
        });
    }

    let (route_key, mount, repo_local_component_id) =
        best.into_iter().next().expect("non-empty route matches");
    let workspace =
        stable_context::mount_path_for_repo(stable, &mount.repo_id)?.ok_or_else(|| {
            G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "component_id": component_id,
                    "repoId": mount.repo_id,
                    "message": "repo-qualified spec selector matched a repo that is not mounted in this stable task workspace",
                }),
            }
        })?;

    Ok(ResolvedSpecCreateTarget {
        workspace,
        repo_local_component_id,
        selector_component_id: selector.to_string(),
        route_key: Some(route_key),
    })
}

#[derive(Debug, Clone)]
struct ManagedSpecRoute {
    route_key: String,
    workspace: PathBuf,
    identity: String,
}

fn managed_spec_routes(
    managed_workspace_root: &Path,
    ws_config: &WorkspaceConfig,
) -> Vec<ManagedSpecRoute> {
    let mut routes = Vec::new();
    for entry in &ws_config.repos {
        for route_key in repo_entry_route_keys(entry) {
            routes.push(ManagedSpecRoute {
                route_key,
                workspace: managed_workspace_root.join(&entry.path),
                identity: entry.name.clone(),
            });
        }
    }
    if let Some(docs) = ws_config.docs.as_ref() {
        let docs_path = docs.path.trim().trim_matches('/');
        if !docs_path.is_empty() && docs_path != "." {
            routes.push(ManagedSpecRoute {
                route_key: docs_path.to_string(),
                workspace: managed_workspace_root.join(&docs.path),
                identity: format!("docs:{}", docs.path),
            });
        }
    }
    routes
}

fn resolve_managed_spec_create_target(
    resolved_workspace: &Path,
    component_id: &str,
) -> Result<Option<ResolvedSpecCreateTarget>, G5Error> {
    let Some(managed_workspace_root) = find_managed_workspace_root(resolved_workspace) else {
        return Ok(None);
    };
    let Some(ws_config) = WorkspaceConfig::load(&managed_workspace_root)
        .map_err(|err| G5Error::Config(err.to_string()))?
    else {
        return Ok(None);
    };
    if ws_config.repos.is_empty() && ws_config.docs.is_none() {
        return Ok(None);
    }

    let selector = component_id.trim().trim_matches('/');
    let mut matches = Vec::new();
    let routes = managed_spec_routes(&managed_workspace_root, &ws_config);
    for route in routes {
        if let Some(local_component_id) = strip_selector_route(selector, &route.route_key) {
            matches.push((
                route.route_key,
                route.workspace,
                route.identity,
                local_component_id,
            ));
        }
    }

    if matches.is_empty() {
        if managed_workspace_requires_repo_route(&ws_config) {
            let repo_routes = managed_spec_routes(&managed_workspace_root, &ws_config)
                .into_iter()
                .map(|route| route.route_key)
                .collect::<Vec<_>>();
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "component_id": component_id,
                    "message": "managed multi-repo spec creation requires a repo-qualified component id: <repo>/<component>",
                    "repoRoutes": repo_routes,
                }),
            });
        }
        return Ok(None);
    }

    matches.sort_by(|(left, _, _, _), (right, _, _, _)| {
        right
            .split('/')
            .count()
            .cmp(&left.split('/').count())
            .then_with(|| right.len().cmp(&left.len()))
    });
    let best_segments = matches[0].0.split('/').count();
    let best_len = matches[0].0.len();
    let best = matches
        .into_iter()
        .filter(|(key, _, _, _)| key.split('/').count() == best_segments && key.len() == best_len)
        .collect::<Vec<_>>();
    let identities = best
        .iter()
        .map(|(_, _, identity, _)| identity.as_str())
        .collect::<BTreeSet<_>>();
    if identities.len() > 1 {
        return Err(G5Error::Conflict {
            reason: format!(
                "component_id '{}' matches multiple managed repo routes; use a unique repo prefix",
                component_id
            ),
        });
    }

    let (route_key, workspace, _, repo_local_component_id) =
        best.into_iter().next().expect("non-empty route matches");
    Ok(Some(ResolvedSpecCreateTarget {
        workspace,
        repo_local_component_id,
        selector_component_id: selector.to_string(),
        route_key: Some(route_key),
    }))
}

fn resolve_spec_create_target(
    resolved_workspace: &Path,
    component_id: &str,
    global: &GlobalFlags,
) -> Result<ResolvedSpecCreateTarget, G5Error> {
    if let Some(stable) = stable_context::discover(global)? {
        if !explicit_classic_workspace_outside_stable_root(global, &stable)? {
            return resolve_stable_spec_create_target(&stable, component_id);
        }
    }

    if let Some(target) = resolve_managed_spec_create_target(resolved_workspace, component_id)? {
        return Ok(target);
    }

    let selector = component_id.trim().trim_matches('/');
    Ok(ResolvedSpecCreateTarget {
        workspace: resolved_workspace.to_path_buf(),
        repo_local_component_id: selector.to_string(),
        selector_component_id: selector.to_string(),
        route_key: None,
    })
}

// ─── Subcommand dispatch ─────────────────────────────────────────────────────

pub async fn execute(args: &SpecArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    match &args.command {
        SpecCommand::Create(create_args) => cmd_create(create_args, global).await,
        SpecCommand::Validate(validate_args) => cmd_validate(validate_args, global).await,
        SpecCommand::ReadOntology(read_args) => cmd_read_ontology(read_args, global).await,
        SpecCommand::MeasureV4(measure_args) => cmd_measure_v4(measure_args, global).await,
        SpecCommand::MigrateV4(migrate_args) => cmd_migrate_v4(migrate_args, global).await,
        SpecCommand::VerifyMove(verify_args) => cmd_verify_move(verify_args, global).await,
        SpecCommand::BlockInventory(inventory_args) => {
            cmd_block_inventory(inventory_args, global).await
        }
        SpecCommand::TagAudit(tag_args) => cmd_tag_audit(tag_args, global).await,
        SpecCommand::List(list_args) => cmd_list(list_args, global).await,
    }
}

// ─── create ──────────────────────────────────────────────────────────────────

/// Replace only the frontmatter `component_id:` line value (route stripped
/// before write, sync_spec_remote D53). Body content is untouched.
fn replace_frontmatter_component_id(
    content: &str,
    from: &str,
    to: &str,
) -> Result<String, G5Error> {
    let mut replaced = false;
    let mut delimiters = 0;
    let mut out: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.trim_end() == "---" {
            delimiters += 1;
            out.push(line.to_string());
            continue;
        }
        if delimiters == 1 && !replaced {
            let trimmed = line.trim_start();
            if let Some(raw_value) = trimmed.strip_prefix("component_id:") {
                let value = raw_value.trim().trim_matches('"').trim_matches('\'');
                if value == from {
                    let indent = &line[..line.len() - trimmed.len()];
                    out.push(format!("{indent}component_id: {to}"));
                    replaced = true;
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    if !replaced {
        return Err(G5Error::Internal {
            message: format!("Failed to strip repo route from frontmatter component_id '{from}'"),
        });
    }
    let mut result = out.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

async fn cmd_create(args: &super::SpecCreateArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    // Repo-routed selector resolution (sync_spec_remote D53, #1013): an
    // unqualified selector that is ambiguous across visible repos fails HERE
    // — before any content is read or written; a routed selector writes
    // under the owning repo with the route stripped from the identity.
    let resolved_workspace = resolve_spec_workspace(global)?;
    let target = resolve_spec_create_target(&resolved_workspace, &args.component_id, global)?;
    let workspace = target.workspace.clone();
    let repo_local_id = target.repo_local_component_id.clone();

    // Read content from --file or stdin
    let content = if let Some(ref file_path) = args.file {
        tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to read file {}: {}", file_path.display(), e),
            })?
    } else {
        // Read from stdin (supports heredoc, pipe, etc.)
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to read from stdin: {}", e),
            })?;
        buf
    };

    if content.trim().is_empty() {
        return Err(G5Error::Internal {
            message: "Empty spec content. Provide content via --file or stdin (heredoc/pipe)."
                .into(),
        });
    }

    // Parse and validate frontmatter
    let frontmatter = parse_frontmatter(&content).map_err(|msg| G5Error::Internal {
        message: format!("Invalid spec: {}", msg),
    })?;

    // Frontmatter identity is always repo-local (sync_spec_remote D53): a
    // repo-local frontmatter id is accepted as-is; a frontmatter that echoes
    // the routed CLI selector has the route stripped before write; anything
    // else is a mismatch.
    let content = if frontmatter.component_id == repo_local_id {
        content
    } else if frontmatter.component_id == args.component_id.trim().trim_matches('/')
        && frontmatter.component_id != repo_local_id
    {
        replace_frontmatter_component_id(&content, &frontmatter.component_id, &repo_local_id)?
    } else {
        return Err(G5Error::Internal {
            message: format!(
                "component_id mismatch: CLI arg resolves to repo-local '{}' but frontmatter says '{}'",
                repo_local_id, frontmatter.component_id
            ),
        });
    };

    // Check for dependency cycles
    let dependency_refs = frontmatter
        .dependency_refs()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !dependency_refs.is_empty() {
        if let Err(cycle_msg) =
            detect_dependency_cycle(&workspace, &repo_local_id, &dependency_refs)
        {
            return Err(G5Error::Internal {
                message: format!("FORBIDDEN: {}", cycle_msg),
            });
        }
    }

    // Resolve output path and create directories
    let path = spec_path(&workspace, &repo_local_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to create spec directory: {}", e),
            })?;
    }

    // Check for overwrites
    let is_new = !path.exists();
    if path.exists() && !args.force {
        return Err(G5Error::Internal {
            message: format!(
                "Spec already exists: {}. Use --force to overwrite.",
                path.display()
            ),
        });
    }

    // Write spec
    tokio::fs::write(&path, &content)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to write spec: {}", e),
        })?;

    info!(
        component_id = %repo_local_id,
        selector_component_id = %target.selector_component_id,
        path = %path.display(),
        "Spec created"
    );

    if global.json {
        let result = serde_json::json!({
            "status": if is_new { "created" } else { "updated" },
            "componentId": repo_local_id,
            "selectorComponentId": target.selector_component_id,
            "repoRoute": target.route_key,
            "path": path.display().to_string(),
            "srcFiles": frontmatter.src_files,
            "dependencies": dependency_refs,
            "parentSpecId": frontmatter.parent_spec_id,
            "childSpecIds": frontmatter.child_spec_ids,
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        let action = if is_new { "Created" } else { "Updated" };
        println!("✓ {} spec: {}", action, repo_local_id);
        if target.selector_component_id != repo_local_id {
            println!("  Selector:     {}", target.selector_component_id);
        }
        println!("  Path:         {}", path.display());
        if !frontmatter.src_files.is_empty() {
            println!("  Source files:  {}", frontmatter.src_files.join(", "));
        }
        if let Some(ref parent) = frontmatter.parent_spec_id {
            println!("  Parent:        {}", parent);
        }
        if !dependency_refs.is_empty() {
            println!("  Dependencies:  {}", dependency_refs.join(", "));
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub enum SpecValidationTarget {
    Component(String),
    All,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecValidationIssue {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecValidationReport {
    pub valid: bool,
    pub checked: usize,
    /// Repo-route-qualified identifiers (@ref
    /// software_architecture/g5d/cli/sync_spec_remote#D58) for every spec
    /// actually checked, so a validation failure names which components
    /// were inspected, not just how many. Sorted and deduplicated; empty
    /// only when `checked` is `0`.
    pub checked_components: Vec<String>,
    pub issues: Vec<SpecValidationIssue>,
    /// Typed warnings that do not affect `valid` (cli spec D52): missing
    /// `source_files` on specs that are new in the working tree, where the
    /// mandated spec-before-code ordering makes not-yet-written sources
    /// expected rather than stale.
    pub pre_implementation_warnings: Vec<SpecValidationIssue>,
    pub ontology_markup: Vec<SpecOntologyMarkupReport>,
    pub ontology_lint_mode: OntologyLintMode,
    pub ontology_lint: OntologyLintReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecOntologyMarkupReport {
    pub path: String,
    pub component_id: String,
    #[serde(flatten)]
    pub summary: OntologyMarkupSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyV4MetricDocument {
    pub path: String,
    pub component_id: String,
    #[serde(flatten)]
    pub summary: OntologyMarkupSummary,
    pub raw_token_estimate: usize,
    pub resolved_token_estimate: usize,
    pub token_delta: i64,
    pub inline_ref_targets: Vec<String>,
    pub derived_ref_targets: Vec<String>,
    pub weak_whole_node_ref_targets: Vec<String>,
    pub ref_set_before: Vec<String>,
    pub ref_set_after: Vec<String>,
    pub ref_set_preserved: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyV4MetricTotals {
    pub document_count: usize,
    pub raw_token_estimate: usize,
    pub resolved_token_estimate: usize,
    pub token_delta: i64,
    pub inline_ref_count: usize,
    pub derived_ref_count: usize,
    pub weak_whole_node_ref_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyV4MeasurementReport {
    pub valid: bool,
    pub checked: usize,
    pub target: String,
    pub totals: OntologyV4MetricTotals,
    pub documents: Vec<OntologyV4MetricDocument>,
    pub ontology_lint: OntologyLintReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyV4WriteCounters {
    pub source_files: usize,
    pub git_commits: usize,
    pub intent_records: usize,
    pub grounding_records: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyV4MigrationDryRunReport {
    pub dry_run: bool,
    pub valid: bool,
    pub checked: usize,
    pub target: String,
    pub writes: OntologyV4WriteCounters,
    pub measurement: OntologyV4MeasurementReport,
}

#[derive(Debug, Clone)]
struct IndexedSpec {
    root: PathBuf,
    path: PathBuf,
    path_component_id: String,
    frontmatter: Option<SpecFrontmatter>,
    parse_error: Option<String>,
}

impl IndexedSpec {
    fn component_id(&self) -> String {
        self.frontmatter
            .as_ref()
            .map(|fm| fm.component_id.clone())
            .unwrap_or_else(|| self.path_component_id.clone())
    }
}

fn issue(
    path: &Path,
    component_id: Option<String>,
    message: impl Into<String>,
) -> SpecValidationIssue {
    SpecValidationIssue {
        path: path.display().to_string(),
        component_id,
        message: message.into(),
    }
}

fn ontology_lint_rule_id(diag: &OntologyLintDiagnostic) -> String {
    serde_json::to_value(diag.rule_id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{:?}", diag.rule_id))
}

fn ontology_lint_issue_message(diag: &OntologyLintDiagnostic) -> String {
    let rule = ontology_lint_rule_id(diag);
    match (diag.location.block_id.as_deref(), diag.location.line) {
        (Some(block), Some(line)) => {
            format!("{rule}: {} ({block}, line {line})", diag.message)
        }
        (Some(block), None) => format!("{rule}: {} ({block})", diag.message),
        (None, Some(line)) => format!("{rule}: {} (line {line})", diag.message),
        (None, None) => format!("{rule}: {}", diag.message),
    }
}

fn component_id_from_spec_path(specs_dir: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(specs_dir).unwrap_or(path);
    let mut text = rel.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = text.strip_suffix(".semlift.md") {
        text = stripped.to_string();
    } else if let Some(stripped) = text.strip_suffix(".md") {
        text = stripped.to_string();
    }
    text
}

fn is_spec_file(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.ends_with(".semlift.md") || text.ends_with(".md")
}

fn collect_indexed_specs(roots: &[PathBuf]) -> Vec<IndexedSpec> {
    let mut specs = Vec::new();
    let mut seen_paths = HashSet::new();

    for root in roots {
        for specs_dir in [
            root.join(".semlift").join("specs"),
            root.join(".g5").join("specs"),
        ] {
            if !specs_dir.is_dir() {
                continue;
            }
            for entry in walkdir::WalkDir::new(&specs_dir)
                .into_iter()
                .filter_map(Result::ok)
            {
                if !entry.file_type().is_file() || !is_spec_file(entry.path()) {
                    continue;
                }
                let path = entry.path().to_path_buf();
                let key = canonical_or_self(&path);
                if !seen_paths.insert(key) {
                    continue;
                }
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let (frontmatter, parse_error) = match parse_frontmatter(&content) {
                    Ok(fm) => (Some(fm), None),
                    Err(err) => (None, Some(err)),
                };
                specs.push(IndexedSpec {
                    root: root.clone(),
                    path: path.clone(),
                    path_component_id: component_id_from_spec_path(&specs_dir, &path),
                    frontmatter,
                    parse_error,
                });
            }
        }
    }

    specs.sort_by(|a, b| a.path.cmp(&b.path));
    specs
}

fn validate_source_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Some("source file path is empty".to_string());
    }
    let p = Path::new(trimmed);
    if p.is_absolute() {
        return Some(format!(
            "source file path must be repository-relative: {trimmed}"
        ));
    }
    if p.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Some(format!(
            "source file path must not escape the repository: {trimmed}"
        ));
    }
    None
}

fn source_path_is_git_ignored(repo_root: &Path, path: &str) -> bool {
    crate::cli::native_git::runner()
        .and_then(|runner| {
            runner
                .path_is_ignored_blocking(repo_root, Path::new(path))
                .map_err(Into::into)
        })
        .unwrap_or(false)
}

fn body_after_frontmatter(content: &str) -> Option<&str> {
    let fm = extract_frontmatter(content).ok()?;
    let offset = 3 + fm.len();
    let rest = content.get(offset..)?;
    let closing = rest.find("---")?;
    content.get(offset + closing + 3..)
}

fn validate_heading_style(
    content: &str,
    spec: &IndexedSpec,
    issues: &mut Vec<SpecValidationIssue>,
) {
    let Some(body) = body_after_frontmatter(content) else {
        return;
    };
    for (idx, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if idx > 0 && trimmed.len() >= 3 && trimmed.chars().all(|c| c == '=' || c == '-') {
            issues.push(issue(
                &spec.path,
                Some(spec.component_id()),
                format!(
                    "Line {}: use ATX headings (# Title), not setext underline headings",
                    idx + 1
                ),
            ));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RepoScopedComponentId {
    pub(crate) root: PathBuf,
    pub(crate) component_id: String,
}

impl RepoScopedComponentId {
    fn new(root: &Path, component_id: impl Into<String>) -> Self {
        Self {
            root: root.to_path_buf(),
            component_id: component_id.into(),
        }
    }
}

fn validation_repo_key(root: &Path, route_keys: &HashMap<PathBuf, String>) -> String {
    route_keys
        .get(root)
        .cloned()
        .or_else(|| {
            std::fs::canonicalize(root)
                .ok()
                .and_then(|canonical| route_keys.get(&canonical).cloned())
        })
        .unwrap_or_else(|| {
            std::fs::canonicalize(root)
                .unwrap_or_else(|_| root.to_path_buf())
                .display()
                .to_string()
        })
}

/// Repo-route-qualified label for one component (@ref
/// software_architecture/g5d/cli/sync_spec_remote#D58). `pub(crate)` so the
/// `g5d task complete` scope report (@ref
/// software_architecture/g5d/cli/external_agent_tools#D20) can reuse the same
/// label shape for the selected components as `checked_components`
/// uses for the validated closure.
pub(crate) fn validation_component_label(
    component: &RepoScopedComponentId,
    multi_root: bool,
    route_keys: &HashMap<PathBuf, String>,
) -> String {
    if multi_root {
        format!(
            "{}/{}",
            validation_repo_key(&component.root, route_keys),
            component.component_id
        )
    } else {
        component.component_id.clone()
    }
}

fn find_dependency_cycles(
    graph: &HashMap<RepoScopedComponentId, Vec<RepoScopedComponentId>>,
) -> Vec<Vec<RepoScopedComponentId>> {
    fn visit(
        node: &RepoScopedComponentId,
        graph: &HashMap<RepoScopedComponentId, Vec<RepoScopedComponentId>>,
        visiting: &mut Vec<RepoScopedComponentId>,
        visited: &mut HashSet<RepoScopedComponentId>,
        cycles: &mut Vec<Vec<RepoScopedComponentId>>,
    ) {
        if let Some(pos) = visiting.iter().position(|item| item == node) {
            let mut cycle = visiting[pos..].to_vec();
            cycle.push(node.clone());
            if !cycles.iter().any(|existing| existing == &cycle) {
                cycles.push(cycle);
            }
            return;
        }
        if visited.contains(node) {
            return;
        }
        visiting.push(node.clone());
        for dep in graph.get(node).into_iter().flatten() {
            if graph.contains_key(dep) {
                visit(dep, graph, visiting, visited, cycles);
            }
        }
        visiting.pop();
        visited.insert(node.clone());
    }

    let mut visited = HashSet::new();
    let mut cycles = Vec::new();
    let mut nodes = graph.keys().cloned().collect::<Vec<_>>();
    nodes.sort();
    for node in nodes {
        visit(&node, graph, &mut Vec::new(), &mut visited, &mut cycles);
    }
    cycles
}

pub fn validate_specs_for_roots(
    roots: &[PathBuf],
    target: SpecValidationTarget,
) -> SpecValidationReport {
    validate_specs_for_roots_with_lint_mode(roots, target, OntologyLintMode::Off)
}

pub fn validate_specs_for_roots_with_lint_mode(
    roots: &[PathBuf],
    target: SpecValidationTarget,
    ontology_lint_mode: OntologyLintMode,
) -> SpecValidationReport {
    validate_specs_for_roots_with_lint_mode_and_routes(
        roots,
        target,
        ontology_lint_mode,
        &HashMap::new(),
    )
}

/// Validate with repo route keys so multi-repo `checked_components` labels are
/// repo-route-qualified `<repo-key>/<component-id>` selectors instead of
/// host-absolute paths (@ref
/// software_architecture/g5d/cli/sync_spec_remote#D58, @ref
/// system_invariants/multi_repo_workspace_ownership#D8). Lint stays `Off`, as
/// the completion gate never runs ontology lint (@ref
/// software_architecture/g5d/cli/sync_spec_remote#D51).
pub fn validate_specs_for_roots_with_routes(
    roots: &[PathBuf],
    target: SpecValidationTarget,
    route_keys: &HashMap<PathBuf, String>,
) -> SpecValidationReport {
    validate_specs_for_roots_with_lint_mode_and_routes(
        roots,
        target,
        OntologyLintMode::Off,
        route_keys,
    )
}

pub(crate) fn validate_specs_for_roots_with_lint_mode_and_routes(
    roots: &[PathBuf],
    target: SpecValidationTarget,
    ontology_lint_mode: OntologyLintMode,
    route_keys: &HashMap<PathBuf, String>,
) -> SpecValidationReport {
    let specs = collect_indexed_specs(roots);
    let mut issues = Vec::new();
    let mut pre_implementation_warnings = Vec::new();
    let mut ontology_markup = Vec::new();

    if specs.is_empty() {
        return SpecValidationReport {
            valid: false,
            checked: 0,
            checked_components: Vec::new(),
            issues: vec![SpecValidationIssue {
                path: roots
                    .first()
                    .map(|root| root.display().to_string())
                    .unwrap_or_else(|| ".".to_string()),
                component_id: None,
                message: "No Semlift specs found under .semlift/specs".to_string(),
            }],
            pre_implementation_warnings,
            ontology_markup,
            ontology_lint_mode,
            ontology_lint: OntologyLintReport {
                valid: true,
                diagnostics: Vec::new(),
            },
        };
    }

    let mut by_component: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_path_component: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, spec) in specs.iter().enumerate() {
        by_path_component
            .entry(spec.path_component_id.clone())
            .or_default()
            .push(idx);
        if let Some(fm) = &spec.frontmatter {
            by_component
                .entry(fm.component_id.clone())
                .or_default()
                .push(idx);
        }
    }

    // Duplicate detection is keyed by (repo root, component_id): the same
    // repo-local component id in two different visible repo roots is a valid
    // pair of distinct canonical identities, never a duplicate
    // (sync_spec_remote D56, multi_repo_workspace_ownership D8). Intra-root
    // duplicates remain hard failures; in a multi-root corpus each finding
    // names its repo route so it stays addressable, while single-root output
    // stays byte-compatible. BTreeMap keeps finding order deterministic.
    let mut by_root_component: std::collections::BTreeMap<RepoScopedComponentId, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (idx, spec) in specs.iter().enumerate() {
        if let Some(fm) = &spec.frontmatter {
            by_root_component
                .entry(RepoScopedComponentId::new(
                    &spec.root,
                    fm.component_id.clone(),
                ))
                .or_default()
                .push(idx);
        }
    }
    let multi_root = specs.iter().any(|spec| spec.root != specs[0].root);

    let selected_indices: Vec<usize> = match &target {
        SpecValidationTarget::All => (0..specs.len()).collect(),
        SpecValidationTarget::Component(component_id) => {
            let mut selected = by_component
                .get(component_id)
                .cloned()
                .or_else(|| by_path_component.get(component_id).cloned())
                .unwrap_or_default();
            selected.sort_unstable();
            selected.dedup();
            if selected.is_empty() {
                issues.push(SpecValidationIssue {
                    path: roots
                        .first()
                        .map(|root| root.display().to_string())
                        .unwrap_or_else(|| ".".to_string()),
                    component_id: Some(component_id.clone()),
                    message: format!("Spec not found: {component_id}"),
                });
            }
            selected
        }
    };

    // (@ref software_architecture/g5d/cli/sync_spec_remote#D58) Name every
    // checked component, not just how many — the same repo-route-qualified
    // label the duplicate-ID and dependency-cycle findings already use, so a
    // validation failure can report which components were actually
    // inspected directly from the report.
    let mut checked_components: Vec<String> = selected_indices
        .iter()
        .map(|idx| {
            validation_component_label(
                &RepoScopedComponentId::new(&specs[*idx].root, specs[*idx].component_id()),
                multi_root,
                route_keys,
            )
        })
        .collect();
    checked_components.sort();
    checked_components.dedup();

    // Duplicate detection is keyed by (repo root, component_id): the same
    // repo-local component id in two different visible repo roots is a valid
    // pair of distinct canonical identities, never a duplicate
    // (sync_spec_remote D56, multi_repo_workspace_ownership D8). Intra-root
    // duplicates remain hard failures; in a multi-root corpus each finding
    // names its repo route so it stays addressable, while single-root output
    // stays byte-compatible.
    for (component, indices) in &by_root_component {
        if indices.len() > 1 {
            let message = if multi_root {
                let repo_key = validation_repo_key(&component.root, route_keys);
                let mut conflicting_paths = indices
                    .iter()
                    .map(|idx| specs[*idx].path.display().to_string())
                    .collect::<Vec<_>>();
                conflicting_paths.sort();
                format!(
                    "duplicate component_id '{}' within repo '{repo_key}' \
                     (routed selector '{repo_key}/{}'); conflicting paths: {}",
                    component.component_id,
                    component.component_id,
                    conflicting_paths.join(", ")
                )
            } else {
                format!("duplicate component_id '{}'", component.component_id)
            };
            for idx in indices {
                issues.push(issue(
                    &specs[*idx].path,
                    Some(component.component_id.clone()),
                    message.clone(),
                ));
            }
        }
    }

    let selected_set = selected_indices.iter().copied().collect::<HashSet<_>>();
    let selected_components = selected_indices
        .iter()
        .map(|idx| RepoScopedComponentId::new(&specs[*idx].root, specs[*idx].component_id()))
        .collect::<HashSet<_>>();
    let component_ids = by_root_component.keys().cloned().collect::<HashSet<_>>();
    let mut graph: HashMap<RepoScopedComponentId, Vec<RepoScopedComponentId>> = HashMap::new();

    for spec in &specs {
        if let Some(fm) = &spec.frontmatter {
            graph.insert(
                RepoScopedComponentId::new(&spec.root, fm.component_id.clone()),
                fm.dependency_refs()
                    .into_iter()
                    .map(|dep| RepoScopedComponentId::new(&spec.root, dep))
                    .collect(),
            );
        }
    }

    let (ontology_lint, scoped_lint_diagnostics) = if ontology_lint_mode.computes_lints() {
        // Phase-1 lint is repo-local. Running one corpus per root prevents an
        // identical component id in another repo from overwriting ownership or
        // satisfying references (sync_spec_remote D56).
        let mut lint_corpora = std::collections::BTreeMap::<PathBuf, Vec<(String, String)>>::new();
        for spec in &specs {
            if let Ok(content) = std::fs::read_to_string(&spec.path) {
                lint_corpora
                    .entry(spec.root.clone())
                    .or_default()
                    .push((spec.component_id(), content));
            }
        }
        let mut scoped_diagnostics = Vec::new();
        for (root, lint_corpus) in lint_corpora {
            let lint_inputs = lint_corpus
                .iter()
                .map(|(component_id, content)| OntologyLintInput {
                    component_id,
                    content,
                })
                .collect::<Vec<_>>();
            let lint_options = match &target {
                SpecValidationTarget::All => OntologyLintOptions::default(),
                SpecValidationTarget::Component(_) => {
                    let selected_for_root = selected_components
                        .iter()
                        .filter(|component| component.root == root)
                        .map(|component| component.component_id.clone());
                    OntologyLintOptions::default().with_target_components(selected_for_root)
                }
            };
            let full_lint_report = validate_ontology_corpus(&lint_inputs, lint_options);
            scoped_diagnostics.extend(
                full_lint_report
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| (root.clone(), diagnostic)),
            );
        }
        let lint_diagnostics = scoped_diagnostics
            .iter()
            .map(|(root, diagnostic)| {
                let mut diagnostic = diagnostic.clone();
                if multi_root {
                    let qualify = |component_id: &str| {
                        validation_component_label(
                            &RepoScopedComponentId::new(root, component_id),
                            true,
                            route_keys,
                        )
                    };
                    diagnostic.location.component_id = qualify(&diagnostic.location.component_id);
                    for location in &mut diagnostic.related_locations {
                        location.component_id = qualify(&location.component_id);
                    }
                }
                diagnostic
            })
            .collect::<Vec<_>>();
        (
            OntologyLintReport {
                valid: lint_diagnostics
                    .iter()
                    .all(|diag| diag.severity != OntologyLintSeverity::Blocking),
                diagnostics: lint_diagnostics,
            },
            scoped_diagnostics,
        )
    } else {
        (OntologyLintReport::default(), Vec::new())
    };
    if ontology_lint_mode.promotes_blocking() {
        for (root, diag) in &scoped_lint_diagnostics {
            if diag.severity != OntologyLintSeverity::Blocking {
                continue;
            }
            let scoped_component =
                RepoScopedComponentId::new(root, diag.location.component_id.clone());
            let issue_path = by_root_component
                .get(&scoped_component)
                .and_then(|indices| indices.first())
                .map(|idx| specs[*idx].path.as_path())
                .unwrap_or_else(|| Path::new("."));
            let component_label =
                validation_component_label(&scoped_component, multi_root, route_keys);
            let rendered_diagnostic = if multi_root {
                let mut rendered = diag.clone();
                rendered.location.component_id = component_label.clone();
                for location in &mut rendered.related_locations {
                    location.component_id = validation_component_label(
                        &RepoScopedComponentId::new(root, &location.component_id),
                        true,
                        route_keys,
                    );
                }
                rendered
            } else {
                diag.clone()
            };
            issues.push(issue(
                issue_path,
                Some(component_label),
                ontology_lint_issue_message(&rendered_diagnostic),
            ));
        }
    }

    for idx in &selected_indices {
        let spec = &specs[*idx];

        let content = match std::fs::read_to_string(&spec.path) {
            Ok(content) => content,
            Err(err) => {
                issues.push(issue(
                    &spec.path,
                    Some(spec.component_id()),
                    format!("failed to read spec: {err}"),
                ));
                continue;
            }
        };
        let summary_component_id = spec.component_id();
        let markup_document = parse_ontology_document(&summary_component_id, &content);
        for duplicate in duplicate_id_blocks(&markup_document) {
            issues.push(issue(
                &spec.path,
                Some(summary_component_id.clone()),
                format!(
                    "duplicate block id '{}': first defined at line {}, redefined at line {} — a line-start '[{}]' bracket defines a block; keep prose bracket references mid-line",
                    duplicate.block_id,
                    duplicate.first_line,
                    duplicate.duplicate_line,
                    duplicate.block_id
                ),
            ));
        }
        ontology_markup.push(SpecOntologyMarkupReport {
            path: spec.path.display().to_string(),
            component_id: summary_component_id.clone(),
            summary: markup_document.summary(),
        });

        if let Some(err) = &spec.parse_error {
            issues.push(issue(&spec.path, None, err.clone()));
            continue;
        }

        let Some(fm) = &spec.frontmatter else {
            continue;
        };

        if fm.component_id != spec.path_component_id {
            issues.push(issue(
                &spec.path,
                Some(fm.component_id.clone()),
                format!(
                    "component_id mismatch: frontmatter has '{}', but path implies '{}'",
                    fm.component_id, spec.path_component_id
                ),
            ));
        }

        if let Some(parent) = fm
            .parent_spec_id
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            if !component_ids.contains(&RepoScopedComponentId::new(&spec.root, parent)) {
                issues.push(issue(
                    &spec.path,
                    Some(fm.component_id.clone()),
                    format!("parent_spec_id references missing spec '{parent}'"),
                ));
            }
        }

        for child in &fm.child_spec_ids {
            let child = child.trim();
            if child.is_empty() {
                issues.push(issue(
                    &spec.path,
                    Some(fm.component_id.clone()),
                    "child_spec_ids contains an empty component id",
                ));
            } else if !component_ids.contains(&RepoScopedComponentId::new(&spec.root, child)) {
                issues.push(issue(
                    &spec.path,
                    Some(fm.component_id.clone()),
                    format!("child_spec_ids references missing spec '{child}'"),
                ));
            }
        }

        for dep in fm.dependency_refs() {
            let dep = dep.trim();
            if dep.is_empty() {
                issues.push(issue(
                    &spec.path,
                    Some(fm.component_id.clone()),
                    "dependencies contains an empty component id",
                ));
            } else if !component_ids.contains(&RepoScopedComponentId::new(&spec.root, dep)) {
                issues.push(issue(
                    &spec.path,
                    Some(fm.component_id.clone()),
                    format!("dependency references missing spec '{dep}'"),
                ));
            }
        }

        // The source_files reachability check runs for every selected spec.
        if selected_set.contains(idx) {
            let mut spec_is_new_in_worktree: Option<bool> = None;
            for src in fm.source_paths() {
                if let Some(message) = validate_source_path(src) {
                    issues.push(issue(&spec.path, Some(fm.component_id.clone()), message));
                } else if source_path_is_git_ignored(&spec.root, src) {
                    issues.push(issue(
                        &spec.path,
                        Some(fm.component_id.clone()),
                        format!(
                            "source file is ignored by git and will be absent from clean CI checkouts: {src}"
                        ),
                    ));
                } else if !spec.root.join(src).is_file() {
                    // Spec-before-code ordering means a freshly authored leaf
                    // legitimately lists sources that do not exist yet (cli spec
                    // D52); only specs with committed history keep the hard
                    // failure so stale source_files on existing leaves still fail.
                    let is_new = *spec_is_new_in_worktree
                        .get_or_insert_with(|| spec_file_is_new_in_worktree(&spec.path));
                    if is_new {
                        pre_implementation_warnings.push(issue(
                            &spec.path,
                            Some(fm.component_id.clone()),
                            format!(
                                "pre-implementation: source file does not exist yet: {src} \
                                 (spec is new in the working tree; this becomes a hard \
                                 failure once the spec is committed)"
                            ),
                        ));
                    } else {
                        issues.push(issue(
                            &spec.path,
                            Some(fm.component_id.clone()),
                            format!("source file does not exist: {src}"),
                        ));
                    }
                }
            }
        }

        validate_heading_style(&content, spec, &mut issues);
    }

    for cycle in find_dependency_cycles(&graph) {
        let include = match &target {
            SpecValidationTarget::All => true,
            SpecValidationTarget::Component(_) => {
                cycle.iter().any(|item| selected_components.contains(item))
            }
        };
        if !include {
            continue;
        }
        if let Some(first) = cycle.first() {
            let path = by_root_component
                .get(first)
                .and_then(|indices| indices.first())
                .map(|idx| specs[*idx].path.as_path())
                .unwrap_or_else(|| Path::new("."));
            let cycle_labels = cycle
                .iter()
                .map(|item| validation_component_label(item, multi_root, route_keys))
                .collect::<Vec<_>>();
            issues.push(issue(
                path,
                Some(first.component_id.clone()),
                format!("dependency cycle: {}", cycle_labels.join(" -> ")),
            ));
        }
    }

    issues.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.component_id.cmp(&b.component_id))
            .then(a.message.cmp(&b.message))
    });
    issues.dedup_by(|a, b| {
        a.path == b.path && a.component_id == b.component_id && a.message == b.message
    });

    SpecValidationReport {
        valid: issues.is_empty(),
        checked: selected_set.len(),
        checked_components,
        issues,
        pre_implementation_warnings,
        ontology_markup,
        ontology_lint_mode,
        ontology_lint,
    }
}

/// A spec absent from git history is "new in the working tree": its
/// `source_files` may legitimately not exist yet under the mandated
/// spec-before-code ordering (cli spec D52). Conservative on ambiguity —
/// outside a git repository, or when git is unavailable, every spec counts as
/// existing so missing sources stay hard failures.
fn spec_file_is_new_in_worktree(spec_path: &Path) -> bool {
    crate::cli::native_git::runner()
        .and_then(|runner| runner.file_is_new_blocking(spec_path).map_err(Into::into))
        .unwrap_or(false)
}

fn roots_from_stable_context(
    stable: &stable_context::StableContext,
) -> Result<Vec<PathBuf>, G5Error> {
    if stable.metadata.mounts.is_empty() {
        return Ok(vec![stable_context::fallback_single_repo_path(stable)]);
    }

    let mut roots = Vec::new();
    for mount in &stable.metadata.mounts {
        if let Some(path) = stable_context::mount_path_for_repo(stable, &mount.repo_id)? {
            roots.push(path);
        }
    }
    if roots.is_empty() {
        roots.push(stable.root.clone());
    }
    Ok(roots)
}

fn resolve_validation_roots(global: &GlobalFlags) -> Result<Vec<PathBuf>, G5Error> {
    if let Some(stable) = stable_context::discover(global)? {
        if !explicit_classic_workspace_outside_stable_root(global, &stable)? {
            return roots_from_stable_context(&stable);
        }
    }

    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("Failed to get cwd: {}", e),
    })?;

    if let Some(explicit) = global.workspace.as_ref() {
        let path = if explicit.is_absolute() {
            explicit.clone()
        } else {
            cwd.join(explicit)
        };
        return Ok(vec![canonical_or_self(&path)]);
    }

    if let Ok(Some(workdir)) = g5_scm::GitRepo::discover_workdir(&cwd) {
        return Ok(vec![canonical_or_self(&workdir)]);
    }

    resolve_workspace(global).map(|workspace| vec![workspace])
}

fn list_roots_from_stable_context(
    stable: &stable_context::StableContext,
) -> Result<Vec<SpecListRoot>, G5Error> {
    if stable.metadata.mounts.is_empty() {
        return Ok(vec![SpecListRoot {
            workspace: stable_context::fallback_single_repo_path(stable),
            route_key: None,
            repo_id: stable.metadata.repo_ids.first().cloned(),
            repo_name: None,
        }]);
    }

    let mut roots = Vec::new();
    for mount in &stable.metadata.mounts {
        if let Some(path) = stable_context::mount_path_for_repo(stable, &mount.repo_id)? {
            roots.push(SpecListRoot {
                workspace: path,
                route_key: primary_stable_mount_route_key(mount),
                repo_id: Some(mount.repo_id.clone()),
                repo_name: mount.repo_name.clone(),
            });
        }
    }
    if roots.is_empty() {
        roots.push(SpecListRoot {
            workspace: stable.root.clone(),
            route_key: None,
            repo_id: None,
            repo_name: None,
        });
    }
    Ok(roots)
}

fn managed_spec_list_roots(
    resolved_workspace: &Path,
) -> Result<Option<Vec<SpecListRoot>>, G5Error> {
    let Some(managed_workspace_root) = find_managed_workspace_root(resolved_workspace) else {
        return Ok(None);
    };
    let Some(ws_config) = WorkspaceConfig::load(&managed_workspace_root)
        .map_err(|err| G5Error::Config(err.to_string()))?
    else {
        return Ok(None);
    };
    if ws_config.repos.is_empty() && ws_config.docs.is_none() {
        return Ok(None);
    }

    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in &ws_config.repos {
        let workspace = managed_workspace_root.join(&entry.path);
        if !seen.insert(canonical_or_self(&workspace)) {
            continue;
        }
        let route_key = repo_entry_route_keys(entry).into_iter().next();
        roots.push(SpecListRoot {
            workspace,
            route_key,
            repo_id: parse_repo_id_from_git_url(&entry.url).map(|id| id.to_string()),
            repo_name: Some(entry.name.clone()),
        });
    }
    if let Some(docs) = ws_config.docs.as_ref() {
        let workspace = managed_workspace_root.join(&docs.path);
        if seen.insert(canonical_or_self(&workspace)) {
            let docs_path = docs.path.trim().trim_matches('/');
            roots.push(SpecListRoot {
                workspace,
                route_key: (!docs_path.is_empty() && docs_path != ".")
                    .then(|| docs_path.to_string()),
                repo_id: None,
                repo_name: (!docs_path.is_empty()).then(|| docs_path.to_string()),
            });
        }
    }

    Ok((!roots.is_empty()).then_some(roots))
}

fn resolve_spec_list_roots(
    resolved_workspace: &Path,
    global: &GlobalFlags,
) -> Result<Vec<SpecListRoot>, G5Error> {
    if let Some(stable) = stable_context::discover(global)? {
        if !explicit_classic_workspace_outside_stable_root(global, &stable)? {
            return list_roots_from_stable_context(&stable);
        }
    }

    if let Some(roots) = managed_spec_list_roots(resolved_workspace)? {
        return Ok(roots);
    }

    Ok(vec![SpecListRoot {
        workspace: resolved_workspace.to_path_buf(),
        route_key: None,
        repo_id: None,
        repo_name: None,
    }])
}

fn print_validation_report(
    report: &SpecValidationReport,
    target: &SpecValidationTarget,
    global: &GlobalFlags,
) {
    if global.json {
        println!("{}", serde_json::to_string_pretty(report).unwrap());
        return;
    }

    let target_label = match target {
        SpecValidationTarget::All => "all specs".to_string(),
        SpecValidationTarget::Component(component_id) => format!("spec '{component_id}'"),
    };

    if report.valid {
        println!("✓ Validated {target_label} ({} checked)", report.checked);
    } else {
        println!(
            "✗ Validation failed for {target_label}: {} issue(s), {} checked",
            report.issues.len(),
            report.checked
        );
        for issue in &report.issues {
            match &issue.component_id {
                Some(component_id) => {
                    println!("  - {} [{}]: {}", issue.path, component_id, issue.message);
                }
                None => {
                    println!("  - {}: {}", issue.path, issue.message);
                }
            }
        }
    }
}

fn cli_read_mode(mode: super::OntologyReadModeArg) -> OntologyReadMode {
    match mode {
        super::OntologyReadModeArg::Raw => OntologyReadMode::Raw,
        super::OntologyReadModeArg::Resolved => OntologyReadMode::Resolved,
        super::OntologyReadModeArg::Both => OntologyReadMode::Both,
    }
}

fn target_from_v4_args(
    component_id: &Option<String>,
    all: bool,
) -> Result<SpecValidationTarget, G5Error> {
    if all {
        Ok(SpecValidationTarget::All)
    } else {
        component_id
            .clone()
            .map(SpecValidationTarget::Component)
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "error": "component_id_required",
                    "message": "supply a component id or --all",
                }),
            })
    }
}

fn spec_target_label(target: &SpecValidationTarget) -> String {
    match target {
        SpecValidationTarget::All => "all".to_string(),
        SpecValidationTarget::Component(component_id) => component_id.clone(),
    }
}

fn reference_target_key(component: &str, block: Option<&str>) -> String {
    match block {
        Some(block) => format!("{component}#{block}"),
        None => component.to_string(),
    }
}

fn sorted_reference_targets(
    document: &OntologyMarkupDocument,
    placement: ReferencePlacement,
) -> Vec<String> {
    let targets = document
        .refs
        .iter()
        .filter(|reference| reference.placement == placement)
        .map(|reference| {
            reference_target_key(
                &reference.target_component,
                reference.target_block.as_deref(),
            )
        })
        .collect::<BTreeSet<_>>();
    targets.into_iter().collect()
}

fn sorted_ref_set(document: &OntologyMarkupDocument) -> Vec<String> {
    let targets = document
        .refs
        .iter()
        .map(|reference| {
            reference_target_key(
                &reference.target_component,
                reference.target_block.as_deref(),
            )
        })
        .collect::<BTreeSet<_>>();
    targets.into_iter().collect()
}

fn sorted_weak_whole_node_targets(document: &OntologyMarkupDocument) -> Vec<String> {
    let targets = document
        .refs
        .iter()
        .filter(|reference| {
            reference.placement == ReferencePlacement::Inline
                && reference.specificity == ReferenceSpecificity::WholeNode
        })
        .map(|reference| reference.target_component.clone())
        .collect::<BTreeSet<_>>();
    targets.into_iter().collect()
}

fn estimate_cli_tokens(content: &str) -> usize {
    estimate_ontology_tokens(content)
}

fn load_v4_corpus(
    roots: &[PathBuf],
    target: &SpecValidationTarget,
) -> Result<(Vec<OntologySourceDocument>, Vec<usize>), G5Error> {
    let specs = collect_indexed_specs(roots);
    if specs.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "error": "no_specs_found",
                "roots": roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>(),
            }),
        });
    }

    let mut component_paths = HashMap::<String, String>::new();
    let mut documents = Vec::new();
    let mut selected_indices = Vec::new();

    for spec in specs {
        let content = std::fs::read_to_string(&spec.path).map_err(|e| G5Error::Internal {
            message: format!("Failed to read spec {}: {}", spec.path.display(), e),
        })?;
        let component_id = spec.component_id();
        let path = spec.path.display().to_string();
        if let Some(existing_path) = component_paths.get(&component_id) {
            if existing_path != &path {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "error": "duplicate_component_id",
                        "componentId": component_id,
                        "existingPath": existing_path,
                        "duplicatePath": path,
                    }),
                });
            }
        } else {
            component_paths.insert(component_id.clone(), path.clone());
        }

        let is_selected = match target {
            SpecValidationTarget::All => true,
            SpecValidationTarget::Component(target_component) => {
                &component_id == target_component || spec.path_component_id == *target_component
            }
        };
        let document = OntologySourceDocument::from_content(component_id, path, content);
        if is_selected {
            selected_indices.push(documents.len());
        }
        documents.push(document);
    }

    if selected_indices.is_empty() {
        return Err(G5Error::NotFound {
            resource: format!("Spec not found: {}", spec_target_label(target)),
        });
    }

    Ok((documents, selected_indices))
}

fn filter_lint_report_for_selection(
    lint_report: OntologyLintReport,
    selected_component_ids: &HashSet<String>,
) -> OntologyLintReport {
    let diagnostics = lint_report
        .diagnostics
        .into_iter()
        .filter(|diag| {
            selected_component_ids.contains(&diag.location.component_id)
                || diag
                    .related_locations
                    .iter()
                    .any(|location| selected_component_ids.contains(&location.component_id))
        })
        .collect::<Vec<_>>();
    OntologyLintReport {
        valid: diagnostics
            .iter()
            .all(|diag| diag.severity != OntologyLintSeverity::Blocking),
        diagnostics,
    }
}

fn build_v4_measurement_report(
    roots: &[PathBuf],
    target: SpecValidationTarget,
) -> Result<OntologyV4MeasurementReport, G5Error> {
    let (corpus, selected_indices) = load_v4_corpus(roots, &target)?;
    let selected_component_ids = selected_indices
        .iter()
        .map(|idx| corpus[*idx].component_id.clone())
        .collect::<HashSet<_>>();
    let lint_inputs = corpus
        .iter()
        .map(|document| OntologyLintInput {
            component_id: &document.component_id,
            content: &document.content,
        })
        .collect::<Vec<_>>();
    let ontology_lint = filter_lint_report_for_selection(
        validate_ontology_corpus(&lint_inputs, OntologyLintOptions::default()),
        &selected_component_ids,
    );

    let mut documents = selected_indices
        .iter()
        .map(|idx| {
            let document = &corpus[*idx];
            let raw_token_estimate = estimate_cli_tokens(&document.content);
            let resolved = resolve_ontology_bundle(
                &corpus,
                &OntologyReadOptions {
                    component_id: document.component_id.clone(),
                    mode: OntologyReadMode::Resolved,
                    token_budget: None,
                    include_transitive: true,
                    include_diagnostics: true,
                    start_block_id: None,
                    targeted_ref: None,
                },
            );
            let resolved_token_estimate = resolved.total_token_estimate;
            let ref_set_before = sorted_ref_set(&document.markup);
            let ref_set_after = ref_set_before.clone();
            OntologyV4MetricDocument {
                path: document.path.clone(),
                component_id: document.component_id.clone(),
                summary: document.markup.summary(),
                raw_token_estimate,
                resolved_token_estimate,
                token_delta: resolved_token_estimate as i64 - raw_token_estimate as i64,
                inline_ref_targets: sorted_reference_targets(
                    &document.markup,
                    ReferencePlacement::Inline,
                ),
                derived_ref_targets: sorted_reference_targets(
                    &document.markup,
                    ReferencePlacement::DerivedIndex,
                ),
                weak_whole_node_ref_targets: sorted_weak_whole_node_targets(&document.markup),
                ref_set_preserved: ref_set_before == ref_set_after,
                ref_set_before,
                ref_set_after,
            }
        })
        .collect::<Vec<_>>();

    documents.sort_by(|left, right| {
        left.component_id
            .cmp(&right.component_id)
            .then(left.path.cmp(&right.path))
    });

    let totals = OntologyV4MetricTotals {
        document_count: documents.len(),
        raw_token_estimate: documents.iter().map(|doc| doc.raw_token_estimate).sum(),
        resolved_token_estimate: documents
            .iter()
            .map(|doc| doc.resolved_token_estimate)
            .sum(),
        token_delta: documents.iter().map(|doc| doc.token_delta).sum(),
        inline_ref_count: documents
            .iter()
            .map(|doc| doc.summary.inline_ref_count)
            .sum(),
        derived_ref_count: documents
            .iter()
            .map(|doc| doc.summary.derived_ref_count)
            .sum(),
        weak_whole_node_ref_count: documents
            .iter()
            .map(|doc| doc.weak_whole_node_ref_targets.len())
            .sum(),
    };
    let valid = ontology_lint.valid && documents.iter().all(|doc| doc.ref_set_preserved);
    Ok(OntologyV4MeasurementReport {
        valid,
        checked: documents.len(),
        target: spec_target_label(&target),
        totals,
        documents,
        ontology_lint,
    })
}

fn print_v4_measurement_report(report: &OntologyV4MeasurementReport, global: &GlobalFlags) {
    if global.json {
        println!("{}", serde_json::to_string_pretty(report).unwrap());
        return;
    }
    println!(
        "Ontology v4 measurement: {} document(s), raw_tokens={}, resolved_tokens={}, delta={}",
        report.checked,
        report.totals.raw_token_estimate,
        report.totals.resolved_token_estimate,
        report.totals.token_delta
    );
    if !report.valid {
        println!(
            "Blocking diagnostics: {}",
            report.ontology_lint.diagnostics.len()
        );
    }
}

fn print_v4_migration_report(report: &OntologyV4MigrationDryRunReport, global: &GlobalFlags) {
    if global.json {
        println!("{}", serde_json::to_string_pretty(report).unwrap());
        return;
    }
    println!(
        "Ontology v4 migration dry-run: {} document(s), writes=0, valid={}",
        report.checked, report.valid
    );
}

fn v4_gate_result(valid: bool, error: &'static str) -> Result<(), G5Error> {
    if valid {
        Ok(())
    } else {
        Err(G5Error::ValidationError {
            details: serde_json::json!({ "error": error }),
        })
    }
}

fn resolve_read_ontology_root(
    roots: &[PathBuf],
    component_id: &str,
) -> Result<(PathBuf, String, String), G5Error> {
    for root in roots {
        let path = spec_path(root, component_id);
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|e| G5Error::Internal {
            message: format!("Failed to read spec {}: {}", path.display(), e),
        })?;
        let root_component_id = parse_frontmatter(&content)
            .map(|frontmatter| frontmatter.component_id)
            .unwrap_or_else(|_| component_id.to_string());
        return Ok((path, root_component_id, content));
    }

    Err(G5Error::NotFound {
        resource: format!("Spec not found: {}", component_id),
    })
}

fn load_read_ontology_corpus(
    root_path: &Path,
    root_component_id: &str,
    root_content: &str,
) -> Result<Vec<OntologySourceDocument>, G5Error> {
    let mut documents = HashMap::<String, OntologySourceDocument>::new();
    let mut source_paths = HashMap::<String, String>::new();
    let specs_dir = specs_dir_for_spec_path(root_path).ok_or_else(|| G5Error::Internal {
        message: format!(
            "Failed to derive specs directory for read-ontology root {}",
            root_path.display()
        ),
    })?;

    for entry in walkdir::WalkDir::new(&specs_dir) {
        let entry = entry.map_err(|e| G5Error::Internal {
            message: format!(
                "Failed to walk specs directory {}: {}",
                specs_dir.display(),
                e
            ),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_spec_file(path) {
            continue;
        }
        let content = std::fs::read_to_string(path).map_err(|e| G5Error::Internal {
            message: format!("Failed to read spec {}: {}", path.display(), e),
        })?;
        let component_id = parse_frontmatter(&content)
            .map(|frontmatter| frontmatter.component_id)
            .unwrap_or_else(|_| component_id_from_spec_path(&specs_dir, path));
        let source_path = path.to_string_lossy().to_string();
        if let Some(existing_path) = source_paths.get(&component_id) {
            if existing_path != &source_path {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "error": "duplicate_component_id",
                        "componentId": component_id,
                        "existingPath": existing_path,
                        "duplicatePath": source_path,
                    }),
                });
            }
            continue;
        }
        source_paths.insert(component_id.clone(), source_path.clone());
        documents.insert(
            component_id.clone(),
            OntologySourceDocument::from_content(component_id, source_path, content),
        );
    }

    let root_source_path = root_path.to_string_lossy().to_string();
    if let Some(existing_path) = source_paths.get(root_component_id) {
        if existing_path != &root_source_path {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "error": "duplicate_component_id",
                    "componentId": root_component_id,
                    "existingPath": existing_path,
                    "duplicatePath": root_source_path,
                }),
            });
        }
    }
    documents.insert(
        root_component_id.to_string(),
        OntologySourceDocument::from_content(
            root_component_id.to_string(),
            root_source_path,
            root_content.to_string(),
        ),
    );

    let mut values = documents.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.component_id.cmp(&right.component_id));
    Ok(values)
}

// ─── read-ontology ──────────────────────────────────────────────────────────

async fn cmd_read_ontology(
    args: &super::SpecReadOntologyArgs,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    // Repo-routed selector resolution (sync_spec_remote D53): cross-root
    // first-match-wins is removed — a multi-repo inventory selects exactly
    // one owning repo root through ownership metadata, and a bare selector
    // present in several repos fails closed with qualified candidates (D45
    // extended across roots) instead of resolving by walk order.
    let (roots, component_id) = match spec_cli_inventory(global)? {
        SpecCliInventory::SingleRoot => {
            (resolve_validation_roots(global)?, args.component_id.clone())
        }
        SpecCliInventory::Repos(repos) => {
            let resolution = crate::cli::spec_selector::resolve_spec_selector_in_roots(
                &repos,
                &args.component_id,
            )?;
            (
                vec![resolution.repo_root],
                resolution.repo_local_component_id,
            )
        }
    };
    let (root_path, root_component_id, root_content) =
        resolve_read_ontology_root(&roots, &component_id)?;
    let corpus = load_read_ontology_corpus(&root_path, &root_component_id, &root_content)?;
    let mode = cli_read_mode(args.mode);
    let options = OntologyReadOptions {
        component_id: root_component_id,
        mode,
        token_budget: args.token_budget,
        include_transitive: args.include_transitive,
        include_diagnostics: args.include_diagnostics,
        start_block_id: args.start_block_id.clone(),
        targeted_ref: None,
    };
    let bundle = resolve_ontology_bundle(&corpus, &options);
    if let Some(message) = bundle.start_block_abort.as_deref() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "error": "start_block_resolution_aborted",
                "message": message,
            }),
        });
    }

    if global.json {
        let output = serde_json::to_string_pretty(&bundle).map_err(|e| G5Error::Internal {
            message: format!("Failed to serialize read-ontology output: {}", e),
        })?;
        println!("{output}");
        return Ok(());
    }

    match mode {
        OntologyReadMode::Raw => {
            print!("{}", bundle.raw_source.unwrap_or_default());
        }
        OntologyReadMode::Resolved => {
            print!("{}", bundle.resolved_markdown.unwrap_or_default());
        }
        OntologyReadMode::Both => {
            let raw_source = bundle.raw_source.unwrap_or_default();
            let resolved_markdown = bundle.resolved_markdown.unwrap_or_default();
            print!("{raw_source}");
            if !resolved_markdown.is_empty() {
                if !raw_source.ends_with('\n') {
                    println!();
                }
                print!("{resolved_markdown}");
            }
        }
    }

    Ok(())
}

// ─── ontology v4 measurement / dry-run migration ────────────────────────────

async fn cmd_measure_v4(
    args: &super::SpecMeasureV4Args,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    let roots = resolve_validation_roots(global)?;
    let target = target_from_v4_args(&args.component_id, args.all)?;
    let report = build_v4_measurement_report(&roots, target)?;
    let valid = report.valid;
    print_v4_measurement_report(&report, global);
    v4_gate_result(valid, "ontology_v4_measurement_gate_failed")
}

async fn cmd_migrate_v4(
    args: &super::SpecMigrateV4Args,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    let roots = resolve_validation_roots(global)?;
    let target = target_from_v4_args(&args.component_id, args.all)?;
    let measurement = build_v4_measurement_report(&roots, target)?;
    let valid = args.dry_run && measurement.valid;
    let report = OntologyV4MigrationDryRunReport {
        dry_run: true,
        valid,
        checked: measurement.checked,
        target: measurement.target.clone(),
        writes: OntologyV4WriteCounters {
            source_files: 0,
            git_commits: 0,
            intent_records: 0,
            grounding_records: 0,
        },
        measurement,
    };
    print_v4_migration_report(&report, global);
    v4_gate_result(valid, "ontology_v4_migration_dry_run_failed")
}

// ─── verify-move ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct VerifyMoveRow {
    from_component: String,
    from_block: String,
    to_component: String,
    to_block: String,
    moved_text: String,
    #[serde(default)]
    verbatim: bool,
    #[serde(default)]
    rename: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifyMoveViolation {
    from_component: String,
    from_block: String,
    to_component: String,
    to_block: String,
    present_in_concept: bool,
    absent_from_leaf: bool,
    verbatim_required: bool,
    reason: String,
}

#[derive(Debug, Serialize)]
struct VerifyMoveReport {
    valid: bool,
    checked: usize,
    violations: Vec<VerifyMoveViolation>,
}

#[derive(Debug, Clone, Copy)]
struct VerifyMoveRename<'a> {
    from: &'a str,
    to: &'a str,
}

fn is_verify_move_token_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn parse_verify_move_rename(rename: Option<&str>) -> Result<Option<VerifyMoveRename<'_>>, G5Error> {
    let Some(rename) = rename else {
        return Ok(None);
    };
    let Some((from, to)) = rename
        .split_once('\u{2192}')
        .or_else(|| rename.split_once("->"))
    else {
        return Err(G5Error::Internal {
            message: "verify-move: rename must use Old->New or Old\u{2192}New".to_string(),
        });
    };
    let from = from.trim();
    let to = to.trim();
    if from.is_empty()
        || to.is_empty()
        || !from.chars().all(is_verify_move_token_char)
        || !to.chars().all(is_verify_move_token_char)
    {
        return Err(G5Error::Internal {
            message: format!(
                "verify-move: rename must contain exactly one identifier token on each side; got '{rename}'"
            ),
        });
    }
    Ok(Some(VerifyMoveRename { from, to }))
}

fn replace_verify_move_whole_token(moved_text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(moved_text.len());
    let mut cursor = 0usize;

    while let Some(relative_start) = moved_text[cursor..].find(from) {
        let start = cursor + relative_start;
        let end = start + from.len();
        let before_ok = moved_text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_verify_move_token_char(ch));
        let after_ok = moved_text[end..]
            .chars()
            .next()
            .is_none_or(|ch| !is_verify_move_token_char(ch));

        if before_ok && after_ok {
            out.push_str(&moved_text[cursor..start]);
            out.push_str(to);
            cursor = end;
        } else {
            let next = start
                + moved_text[start..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
            out.push_str(&moved_text[cursor..next]);
            cursor = next;
        }
    }

    out.push_str(&moved_text[cursor..]);
    out
}

fn apply_verify_move_rename(moved_text: &str, rename: Option<&str>) -> Result<String, G5Error> {
    match parse_verify_move_rename(rename)? {
        Some(rename) => Ok(replace_verify_move_whole_token(
            moved_text,
            rename.from,
            rename.to,
        )),
        None => Ok(moved_text.to_string()),
    }
}

fn resolve_verify_move_spec_path(
    roots: &[PathBuf],
    component_id: &str,
) -> Result<PathBuf, G5Error> {
    let mut matches = BTreeSet::new();
    for root in roots {
        let path = spec_path(root, component_id);
        if path.is_file() {
            matches.insert(std::fs::canonicalize(&path).unwrap_or(path));
        }
    }

    match matches.len() {
        0 => Err(G5Error::Internal {
            message: format!("verify-move: spec not found for '{component_id}' in resolved roots"),
        }),
        1 => Ok(matches.into_iter().next().expect("one match")),
        _ => Err(G5Error::Internal {
            message: format!(
                "verify-move: component '{component_id}' is ambiguous across roots: {}",
                matches
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

fn read_verify_move_spec(
    roots: &[PathBuf],
    cache: &mut HashMap<PathBuf, String>,
    component_id: &str,
) -> Result<String, G5Error> {
    let path = resolve_verify_move_spec_path(roots, component_id)?;
    if let Some(existing) = cache.get(&path) {
        return Ok(existing.clone());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| G5Error::Internal {
        message: format!(
            "verify-move: cannot read spec for '{component_id}' at {}: {e}",
            path.display()
        ),
    })?;
    cache.insert(path, content.clone());
    Ok(content)
}

fn verify_move_block_text(
    component_id: &str,
    content: &str,
    block_id: &str,
) -> Result<String, G5Error> {
    let document = parse_ontology_document(component_id, content);
    let blocks = document
        .id_blocks
        .iter()
        .filter(|block| block.raw_id == block_id)
        .collect::<Vec<_>>();
    match blocks.len() {
        0 => Err(G5Error::Internal {
            message: format!("verify-move: block '{block_id}' not found in '{component_id}'"),
        }),
        1 => Ok(blocks[0].raw_text.clone()),
        _ => Err(G5Error::Internal {
            message: format!("verify-move: block '{block_id}' is ambiguous in '{component_id}'"),
        }),
    }
}

#[cfg(test)]
fn build_verify_move_report(
    workspace_root: &Path,
    manifest: &str,
) -> Result<VerifyMoveReport, G5Error> {
    build_verify_move_report_for_roots(&[workspace_root.to_path_buf()], manifest)
}

fn build_verify_move_report_for_roots(
    roots: &[PathBuf],
    manifest: &str,
) -> Result<VerifyMoveReport, G5Error> {
    let mut file_cache: HashMap<PathBuf, String> = HashMap::new();

    let mut checked = 0usize;
    let mut violations = Vec::new();

    for (idx, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: VerifyMoveRow = serde_json::from_str(line).map_err(|e| G5Error::Internal {
            message: format!(
                "verify-move: manifest line {} is not valid JSON: {e}",
                idx + 1
            ),
        })?;
        checked += 1;

        let moved_text = row.moved_text.trim();
        if moved_text.is_empty() {
            return Err(G5Error::Internal {
                message: format!(
                    "verify-move: manifest line {} has empty moved_text",
                    idx + 1
                ),
            });
        }

        let concept_content = read_verify_move_spec(roots, &mut file_cache, &row.to_component)?;
        let leaf_content = read_verify_move_spec(roots, &mut file_cache, &row.from_component)?;
        let concept_block =
            verify_move_block_text(&row.to_component, &concept_content, row.to_block.trim())?;
        let leaf_block =
            verify_move_block_text(&row.from_component, &leaf_content, row.from_block.trim())?;
        let normalized = apply_verify_move_rename(moved_text, row.rename.as_deref())?;
        let present_in_concept = !row.verbatim || concept_block.contains(normalized.trim());
        let absent_from_leaf = !leaf_block.contains(moved_text);

        if !present_in_concept || !absent_from_leaf {
            let mut reasons = Vec::new();
            if !present_in_concept {
                reasons.push("moved bytes not found in destination block".to_string());
            }
            if !absent_from_leaf {
                reasons.push("moved bytes still present in source block".to_string());
            }
            violations.push(VerifyMoveViolation {
                from_component: row.from_component,
                from_block: row.from_block,
                to_component: row.to_component,
                to_block: row.to_block,
                present_in_concept,
                absent_from_leaf,
                verbatim_required: row.verbatim,
                reason: reasons.join("; "),
            });
        }
    }

    Ok(VerifyMoveReport {
        valid: violations.is_empty(),
        checked,
        violations,
    })
}

async fn cmd_verify_move(
    args: &super::SpecVerifyMoveArgs,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    let roots = resolve_validation_roots(global)?;
    if roots.is_empty() {
        return Err(G5Error::Internal {
            message: "verify-move: no workspace root resolved".to_string(),
        });
    }
    let manifest = std::fs::read_to_string(&args.moves).map_err(|e| G5Error::Internal {
        message: format!(
            "verify-move: cannot read manifest {}: {e}",
            args.moves.display()
        ),
    })?;
    let report = build_verify_move_report_for_roots(&roots, &manifest)?;

    if global.json {
        let json = serde_json::to_string_pretty(&report).map_err(|e| G5Error::Internal {
            message: format!("verify-move: cannot serialize report: {e}"),
        })?;
        println!("{json}");
    } else if report.valid {
        println!("verify-move: OK - {} move(s) verified", report.checked);
    } else {
        println!(
            "verify-move: FAIL - {} of {} move(s) violated the gate:",
            report.violations.len(),
            report.checked
        );
        for violation in &report.violations {
            println!(
                "  {}#{} -> {}#{}: {}",
                violation.from_component,
                violation.from_block,
                violation.to_component,
                violation.to_block,
                violation.reason
            );
        }
    }

    if report.valid {
        Ok(())
    } else {
        Err(G5Error::Internal {
            message: format!(
                "verify-move gate failed: {} violation(s)",
                report.violations.len()
            ),
        })
    }
}

// ─── block-inventory / tag-audit ────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct BlockInventoryHumanTag {
    present: bool,
    /// Deprecated output field retained so migration tooling can consume old
    /// inventory rows. Canonical record metadata carries no origin value.
    origin: Option<String>,
}

#[derive(Debug, Serialize)]
struct BlockInventoryRow {
    old_file: String,
    component_id: String,
    old_block_id: String,
    old_block_kind: String,
    old_heading: String,
    old_content_sha256: String,
    old_line_start: usize,
    old_line_end: usize,
    source_path: String,
    human_region: bool,
    human_semantic_tag: BlockInventoryHumanTag,
    uncertainty_tag: bool,
    binding_count: usize,
    binding_warning_count: usize,
}

#[derive(Debug, Clone)]
struct TagSpan {
    detection: &'static str,
    block_index: Option<usize>,
    tag_kind: String,
    human_origin: Option<String>,
    uncertainty_level: Option<String>,
    uncertainty_source: Option<String>,
    start_offset: usize,
    end_offset: usize,
    start_line: usize,
    end_line: usize,
    raw_text: String,
}

type ParsedTagMetadata = (String, Option<String>, Option<String>, Option<String>);

#[derive(Debug, Serialize)]
struct TagAuditRow {
    old_file: String,
    component_id: String,
    detection: &'static str,
    block_index: Option<usize>,
    tag_kind: String,
    human_origin: Option<String>,
    uncertainty_level: Option<String>,
    uncertainty_source: Option<String>,
    start_line: usize,
    end_line: usize,
    content_sha256: String,
    enclosing_block_id: Option<String>,
    excerpt: String,
}

fn sha256_hex(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    format!("{digest:x}")
}

fn id_block_kind_label(kind: IdBlockKind) -> &'static str {
    match kind {
        IdBlockKind::Intent => "intent",
        IdBlockKind::Design => "design",
        IdBlockKind::Question => "question",
    }
}

fn line_number_for_offset(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn inventory_specs_dir(root: &Path, archived: bool) -> PathBuf {
    if archived {
        root.join(".semlift").join("archived_specs")
    } else {
        root.join(".semlift").join("specs")
    }
}

fn collect_inventory_spec_paths(
    root: &Path,
    component_id: Option<&str>,
    all: bool,
    archived: bool,
) -> Result<Vec<PathBuf>, G5Error> {
    let specs_dir = inventory_specs_dir(root, archived);
    if !specs_dir.is_dir() {
        return Err(G5Error::NotFound {
            resource: format!("specs root not found: {}", specs_dir.display()),
        });
    }

    if all {
        let mut paths = walkdir::WalkDir::new(&specs_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && is_spec_file(entry.path()))
            .map(|entry| entry.into_path())
            .collect::<Vec<_>>();
        paths.sort();
        return Ok(paths);
    }

    let component_id = component_id.ok_or_else(|| G5Error::ValidationError {
        details: serde_json::json!({
            "error": "component_id_required",
            "message": "supply a component id or --all",
        }),
    })?;
    let path = specs_dir.join(format!("{component_id}.semlift.md"));
    if !path.is_file() {
        return Err(G5Error::NotFound {
            resource: format!("Spec not found: {component_id}"),
        });
    }
    Ok(vec![path])
}

fn component_id_for_inventory_file(specs_dir: &Path, content: &str, path: &Path) -> String {
    parse_frontmatter(content)
        .map(|frontmatter| frontmatter.component_id)
        .unwrap_or_else(|_| component_id_from_spec_path(specs_dir, path))
}

fn semantic_metadata_from_block_indexes(
    document: &OntologyMarkupDocument,
    indexes: &[usize],
) -> Option<SemanticRecordMetadata> {
    indexes
        .iter()
        .filter_map(|idx| document.blocks.get(*idx))
        .find_map(|block| block.semantic_metadata.clone())
}

fn id_block_has_human_overlap(
    id_block_start: usize,
    id_block_end: usize,
    spans: &[TagSpan],
) -> bool {
    spans.iter().any(|span| {
        span.tag_kind == "human"
            && span.start_offset < id_block_end
            && span.end_offset > id_block_start
    })
}

fn id_block_starts_with_standalone_label(content: &str, block: &IdBlockFact) -> bool {
    content
        .get(block.start_offset..block.label_start_offset)
        .is_some_and(|prefix| prefix.trim().is_empty())
}

fn block_inventory_rows_for_file(
    root: &Path,
    specs_dir: &Path,
    path: &Path,
) -> Result<Vec<BlockInventoryRow>, G5Error> {
    let content = std::fs::read_to_string(path).map_err(|e| G5Error::Internal {
        message: format!("block-inventory: cannot read {}: {e}", path.display()),
    })?;
    let component_id = component_id_for_inventory_file(specs_dir, &content, path);
    let document = parse_ontology_document(&component_id, &content);
    let raw_tag_spans = scan_raw_tag_spans(&content);
    let source_path = rel_path(root, path);
    let mut rows = Vec::new();

    for block in &document.id_blocks {
        if !id_block_starts_with_standalone_label(&content, block) {
            continue;
        }

        let metadata = semantic_metadata_from_block_indexes(&document, &block.block_indexes);
        let human_tag = metadata.as_ref().is_some_and(|metadata| metadata.human);
        let uncertainty_tag = metadata.as_ref().is_some_and(|metadata| metadata.uncertain);
        let line_end = line_number_for_offset(&content, block.end_offset);
        let binding_count = document
            .bindings
            .iter()
            .filter(|binding| binding.owner_id.as_deref() == Some(block.raw_id.as_str()))
            .count();
        let binding_warning_count = document
            .warnings
            .iter()
            .filter(|warning| {
                warning
                    .line
                    .is_some_and(|line| line >= block.start_line && line <= line_end)
            })
            .count();
        let human_region = human_tag
            || id_block_has_human_overlap(block.start_offset, block.end_offset, &raw_tag_spans);

        rows.push(BlockInventoryRow {
            old_file: source_path.clone(),
            component_id: component_id.clone(),
            old_block_id: block.raw_id.clone(),
            old_block_kind: id_block_kind_label(block.kind).to_string(),
            old_heading: block.title.clone(),
            old_content_sha256: sha256_hex(&block.raw_text),
            old_line_start: block.start_line,
            old_line_end: line_end,
            source_path: source_path.clone(),
            human_region,
            human_semantic_tag: BlockInventoryHumanTag {
                present: human_tag,
                origin: None,
            },
            uncertainty_tag,
            binding_count,
            binding_warning_count,
        });
    }

    Ok(rows)
}

fn parse_tag_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{attr_name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let value = &tag[start..];
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn tag_metadata(opening_tag: &str, tag_name: &str) -> Option<ParsedTagMetadata> {
    match tag_name {
        "human" => {
            let origin = parse_tag_attribute(opening_tag, "origin")?;
            if !matches!(
                origin.as_str(),
                "direct_edit" | "chat_instruction" | "escalation_feedback"
            ) {
                return None;
            }
            Some(("human".to_string(), Some(origin), None, None))
        }
        "uncertainty" => {
            let level = parse_tag_attribute(opening_tag, "level")?;
            if !matches!(level.as_str(), "red" | "orange" | "normal") {
                return None;
            }
            let source = parse_tag_attribute(opening_tag, "source");
            Some(("uncertainty".to_string(), None, Some(level), source))
        }
        _ => None,
    }
}

fn scan_raw_tag_spans(content: &str) -> Vec<TagSpan> {
    let mut spans = Vec::new();
    let mut line_offsets = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        line_offsets.push(offset);
        offset += line.len();
    }
    if !content.ends_with('\n') && (line_offsets.is_empty() || offset < content.len()) {
        line_offsets.push(offset);
    }

    let lines = content.lines().collect::<Vec<_>>();
    let mut in_code_fence = false;
    let mut line_idx = 0usize;
    while line_idx < lines.len() {
        let line = lines[line_idx];
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            line_idx += 1;
            continue;
        }
        if in_code_fence {
            line_idx += 1;
            continue;
        }

        let leading_ws = line.len() - trimmed.len();
        let mut found: Option<(&str, usize)> = None;
        for tag_name in ["uncertainty", "uncertain", "human"] {
            let prefix = format!("<{tag_name}");
            if trimmed.starts_with(&prefix)
                && trimmed[prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch == '>' || ch.is_whitespace())
            {
                found = Some((tag_name, leading_ws));
                break;
            }
        }
        let Some((tag_name, open_col)) = found else {
            line_idx += 1;
            continue;
        };
        let Some(open_end_col) = line[open_col..].find('>').map(|pos| open_col + pos + 1) else {
            let start_offset = line_offsets[line_idx] + open_col;
            let end_offset = line_offsets[line_idx] + line.len();
            spans.push(TagSpan {
                detection: "raw_malformed",
                block_index: None,
                tag_kind: if tag_name == "human" {
                    "human"
                } else {
                    "uncertainty"
                }
                .to_string(),
                human_origin: None,
                uncertainty_level: None,
                uncertainty_source: None,
                start_offset,
                end_offset,
                start_line: line_idx + 1,
                end_line: line_idx + 1,
                raw_text: content[start_offset..end_offset.min(content.len())].to_string(),
            });
            line_idx += 1;
            continue;
        };
        let opening_tag = &line[open_col..open_end_col];
        let (kind, human_origin, uncertainty_level, uncertainty_source) =
            tag_metadata(opening_tag, tag_name).unwrap_or_else(|| {
                (
                    if tag_name == "human" {
                        "human"
                    } else {
                        "uncertainty"
                    }
                    .to_string(),
                    None,
                    None,
                    None,
                )
            });
        let close_token = format!("</{tag_name}>");
        let mut close_line_idx = line_idx;
        let mut close_col = line[open_end_col..]
            .find(&close_token)
            .map(|pos| open_end_col + pos);
        while close_col.is_none() && close_line_idx + 1 < lines.len() {
            close_line_idx += 1;
            let close_line = lines[close_line_idx];
            if close_line.trim_start().starts_with("```") {
                break;
            }
            close_col = close_line.find(&close_token);
        }
        let Some(close_col) = close_col else {
            let start_offset = line_offsets[line_idx] + open_col;
            let end_offset = line_offsets[line_idx] + open_end_col;
            spans.push(TagSpan {
                detection: "raw_unclosed",
                block_index: None,
                tag_kind: kind,
                human_origin: None,
                uncertainty_level: None,
                uncertainty_source: None,
                start_offset,
                end_offset,
                start_line: line_idx + 1,
                end_line: line_idx + 1,
                raw_text: content[start_offset..end_offset.min(content.len())].to_string(),
            });
            line_idx += 1;
            continue;
        };
        let start_offset = line_offsets[line_idx] + open_col;
        let end_offset = line_offsets[close_line_idx] + close_col + close_token.len();
        let raw_text = content[start_offset..end_offset.min(content.len())].to_string();
        spans.push(TagSpan {
            detection: if close_line_idx > line_idx {
                "raw_multiline"
            } else {
                "raw_tag"
            },
            block_index: None,
            tag_kind: kind,
            human_origin,
            uncertainty_level,
            uncertainty_source,
            start_offset,
            end_offset,
            start_line: line_idx + 1,
            end_line: close_line_idx + 1,
            raw_text,
        });
        line_idx = close_line_idx + 1;
    }

    spans
}

fn enclosing_block_id(document: &OntologyMarkupDocument, span: &TagSpan) -> Option<String> {
    document
        .id_blocks
        .iter()
        .find(|block| {
            block.start_offset <= span.start_offset && block.end_offset >= span.end_offset
        })
        .or_else(|| {
            document
                .id_blocks
                .iter()
                .filter(|block| block.start_offset <= span.start_offset)
                .max_by_key(|block| block.start_offset)
        })
        .map(|block| block.raw_id.clone())
}

fn excerpt(text: &str) -> String {
    const LIMIT: usize = 160;
    let mut out = String::new();
    for ch in text.chars().take(LIMIT) {
        out.push(ch);
    }
    out
}

fn tag_audit_rows_for_file(
    root: &Path,
    specs_dir: &Path,
    path: &Path,
) -> Result<Vec<TagAuditRow>, G5Error> {
    let content = std::fs::read_to_string(path).map_err(|e| G5Error::Internal {
        message: format!("tag-audit: cannot read {}: {e}", path.display()),
    })?;
    let component_id = component_id_for_inventory_file(specs_dir, &content, path);
    let document = parse_ontology_document(&component_id, &content);
    // This command is deliberately migration-only. Canonical `(Tags: ...)`
    // metadata is inventoried by `block-inventory`; `tag-audit` reports only
    // retired wrapper bytes that still need conversion.
    let mut spans = scan_raw_tag_spans(&content);
    spans.sort_by(|left, right| {
        left.start_offset
            .cmp(&right.start_offset)
            .then(left.detection.cmp(right.detection))
            .then(left.end_offset.cmp(&right.end_offset))
    });

    let source_path = rel_path(root, path);
    let rows = spans
        .into_iter()
        .map(|span| {
            let enclosing_block_id = enclosing_block_id(&document, &span);
            TagAuditRow {
                old_file: source_path.clone(),
                component_id: component_id.clone(),
                detection: span.detection,
                block_index: span.block_index,
                tag_kind: span.tag_kind,
                human_origin: span.human_origin,
                uncertainty_level: span.uncertainty_level,
                uncertainty_source: span.uncertainty_source,
                start_line: span.start_line,
                end_line: span.end_line,
                content_sha256: sha256_hex(&span.raw_text),
                enclosing_block_id,
                excerpt: excerpt(&span.raw_text),
            }
        })
        .collect::<Vec<_>>();
    Ok(rows)
}

async fn cmd_block_inventory(
    args: &super::SpecBlockInventoryArgs,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    let roots = resolve_validation_roots(global)?;
    let root = roots.first().cloned().ok_or_else(|| G5Error::Internal {
        message: "block-inventory: no workspace root resolved".to_string(),
    })?;
    let specs_dir = inventory_specs_dir(&root, args.archived);
    let paths =
        collect_inventory_spec_paths(&root, args.component_id.as_deref(), args.all, args.archived)?;
    let mut rows = Vec::new();
    for path in &paths {
        rows.extend(block_inventory_rows_for_file(&root, &specs_dir, path)?);
    }
    rows.sort_by(|left, right| {
        left.old_file
            .cmp(&right.old_file)
            .then(left.old_line_start.cmp(&right.old_line_start))
            .then(left.old_block_id.cmp(&right.old_block_id))
    });
    for row in &rows {
        println!(
            "{}",
            serde_json::to_string(row).map_err(|e| G5Error::Internal {
                message: format!("block-inventory: cannot serialize row: {e}"),
            })?
        );
    }
    if !global.quiet {
        eprintln!(
            "block-inventory: {} block(s) across {} file(s) under {}",
            rows.len(),
            paths.len(),
            specs_dir.display()
        );
    }
    Ok(())
}

async fn cmd_tag_audit(
    args: &super::SpecTagAuditArgs,
    global: &GlobalFlags,
) -> Result<(), G5Error> {
    let roots = resolve_validation_roots(global)?;
    let root = roots.first().cloned().ok_or_else(|| G5Error::Internal {
        message: "tag-audit: no workspace root resolved".to_string(),
    })?;
    let specs_dir = inventory_specs_dir(&root, args.archived);
    let paths =
        collect_inventory_spec_paths(&root, args.component_id.as_deref(), args.all, args.archived)?;
    let mut rows = Vec::new();
    let mut file_count = 0usize;
    for path in &paths {
        let file_rows = tag_audit_rows_for_file(&root, &specs_dir, path)?;
        if !file_rows.is_empty() {
            file_count += 1;
        }
        rows.extend(file_rows);
    }
    rows.sort_by(|left, right| {
        left.old_file
            .cmp(&right.old_file)
            .then(left.start_line.cmp(&right.start_line))
            .then(left.detection.cmp(right.detection))
    });
    for row in &rows {
        println!(
            "{}",
            serde_json::to_string(row).map_err(|e| G5Error::Internal {
                message: format!("tag-audit: cannot serialize row: {e}"),
            })?
        );
    }
    if !global.quiet {
        eprintln!(
            "tag-audit: {} tagged region(s) across {} file(s) under {}",
            rows.len(),
            file_count,
            specs_dir.display()
        );
    }
    Ok(())
}

// ─── validate ────────────────────────────────────────────────────────────────

async fn cmd_validate(args: &super::SpecValidateArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let (roots, target, route_keys) = if args.all {
        let (roots, route_keys) = match spec_cli_inventory(global)? {
            SpecCliInventory::SingleRoot => (resolve_validation_roots(global)?, HashMap::new()),
            SpecCliInventory::Repos(repos) => {
                let route_keys = repos
                    .iter()
                    .map(|(repo_root, repo)| (repo_root.clone(), repo.name.clone()))
                    .collect();
                (
                    repos.into_iter().map(|(repo_root, _)| repo_root).collect(),
                    route_keys,
                )
            }
        };
        (roots, SpecValidationTarget::All, route_keys)
    } else {
        let selector = args
            .component_id
            .clone()
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "componentId": "required unless --all is supplied"
                }),
            })?;
        // Repo-routed selector resolution (sync_spec_remote D53): a routed
        // selector validates its repo-local id within the owning repo only;
        // a bare selector resolves through unique ownership or fails closed
        // listing qualified candidates (D45 extended across roots).
        match spec_cli_inventory(global)? {
            SpecCliInventory::SingleRoot => (
                resolve_validation_roots(global)?,
                SpecValidationTarget::Component(selector),
                HashMap::new(),
            ),
            SpecCliInventory::Repos(repos) => {
                let resolution =
                    crate::cli::spec_selector::resolve_spec_selector_in_roots(&repos, &selector)?;
                (
                    vec![resolution.repo_root],
                    SpecValidationTarget::Component(resolution.repo_local_component_id),
                    HashMap::new(),
                )
            }
        }
    };
    let report = validate_specs_for_roots_with_lint_mode_and_routes(
        &roots,
        target.clone(),
        args.ontology_lint,
        &route_keys,
    );
    print_validation_report(&report, &target, global);

    if report.valid {
        Ok(())
    } else {
        Err(G5Error::ValidationError {
            details: serde_json::json!({
                "error": "spec validation failed",
                "issues": report.issues,
            }),
        })
    }
}

fn routed_spec_file_path(root: &Path, path: &Path, route_key: &str) -> String {
    let repo_relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    format!(
        "{}/{}",
        route_key.trim_end_matches('/'),
        repo_relative.trim_start_matches('/')
    )
}

// ─── list ────────────────────────────────────────────────────────────────────

async fn cmd_list(args: &super::SpecListArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    // Multi-repo inventories emit repo-routed selectors that round-trip into
    // every other spec command (sync_spec_remote D53, tool-surface
    // `list_specs` shape). Single-root contexts keep the legacy output
    // byte-identical.
    let resolved_workspace = resolve_spec_workspace(global)?;
    let list_roots = resolve_spec_list_roots(&resolved_workspace, global)?;

    let pattern = args.pattern.as_deref().unwrap_or("*");
    let glob_pat = glob::Pattern::new(pattern).unwrap_or_else(|_| glob::Pattern::new("*").unwrap());

    let mut specs: Vec<serde_json::Value> = Vec::new();
    let mut any_specs_dir = false;

    for root in &list_roots {
        let Some(specs_dir) = find_specs_dir(&root.workspace) else {
            continue;
        };
        any_specs_dir = true;
        let route_key = root.route_key.as_ref();

        for entry in walkdir::WalkDir::new(&specs_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if ext != Some("md") {
                continue;
            }

            let rel = path.strip_prefix(&specs_dir).unwrap_or(path);
            let component_id = rel
                .to_string_lossy()
                .trim_end_matches(".semlift.md")
                .trim_end_matches(".md")
                .to_string();

            // The routed selector prefixes the repo route key even when the
            // repo-local id already starts with the same text (namespace, not
            // route — bootstrap_view_fanout D1).
            let routed_selector = route_key
                .as_ref()
                .map(|key| format!("{key}/{component_id}"));

            if pattern != "*"
                && !glob_pat.matches(&component_id)
                && !routed_selector
                    .as_ref()
                    .is_some_and(|selector| glob_pat.matches(selector))
            {
                continue;
            }

            // Read frontmatter for summary info
            let mut src_files = Vec::new();
            let mut parent = None;
            let mut deps = Vec::new();
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(fm) = parse_frontmatter(&content) {
                    src_files = fm.src_files;
                    parent = fm.parent_spec_id;
                    deps = fm.dependencies;
                }
            }

            let mut record = serde_json::json!({
                "componentId": routed_selector
                    .clone()
                    .unwrap_or_else(|| component_id.clone()),
                "srcFiles": src_files,
                "parentSpecId": parent,
                "dependencies": deps,
            });
            if let Some(route_key) = route_key {
                record["specComponentId"] = serde_json::json!(component_id);
                if let Some(repo_id) = root.repo_id.as_ref() {
                    record["repoId"] = serde_json::json!(repo_id);
                }
                record["repoName"] =
                    serde_json::json!(root.repo_name.as_deref().unwrap_or(route_key));
                record["repoPrefix"] = serde_json::json!(route_key);
                record["filePath"] =
                    serde_json::json!(routed_spec_file_path(&root.workspace, path, route_key));
            }
            specs.push(record);
        }
    }

    if !any_specs_dir {
        if global.json {
            println!("[]");
        } else {
            println!("No specs found.");
        }
        return Ok(());
    }

    // Sort by component_id
    specs.sort_by(|a, b| {
        a["componentId"]
            .as_str()
            .unwrap_or("")
            .cmp(b["componentId"].as_str().unwrap_or(""))
    });

    if global.json {
        println!("{}", serde_json::to_string_pretty(&specs).unwrap());
    } else if specs.is_empty() {
        println!("No specs found.");
    } else {
        println!("{:<40} {:<30} {:<20}", "COMPONENT", "SRC FILES", "PARENT");
        println!("{}", "-".repeat(90));
        for spec in &specs {
            let cid = spec["componentId"].as_str().unwrap_or("-");
            let files: Vec<&str> = spec["srcFiles"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let parent = spec["parentSpecId"].as_str().unwrap_or("-");
            println!(
                "{:<40} {:<30} {:<20}",
                cid,
                if files.is_empty() {
                    "-".to_string()
                } else {
                    files.join(", ")
                },
                parent
            );
        }
        println!("\n{} spec(s) total", specs.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn plant_spec(root: &Path, component_id: &str, frontmatter_extra: &str) {
        let path = root
            .join(".semlift")
            .join("specs")
            .join(format!("{component_id}.semlift.md"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "---\ncomponent_id: {component_id}\n{frontmatter_extra}---\n# {component_id}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn routed_spec_file_path_preserves_canonical_and_legacy_layouts() {
        let root = Path::new("/workspace/g5");
        assert_eq!(
            routed_spec_file_path(
                root,
                Path::new("/workspace/g5/.semlift/specs/software_architecture/api.semlift.md"),
                "g5",
            ),
            "g5/.semlift/specs/software_architecture/api.semlift.md"
        );
        assert_eq!(
            routed_spec_file_path(
                root,
                Path::new("/workspace/g5/.g5/specs/software_architecture/api.md"),
                "g5",
            ),
            "g5/.g5/specs/software_architecture/api.md"
        );
    }

    struct EnvCwdRestore {
        old_pwd: Option<std::ffi::OsString>,
        old_cwd: PathBuf,
    }

    impl EnvCwdRestore {
        fn set(path: &Path) -> Self {
            let guard = Self {
                old_pwd: std::env::var_os("PWD"),
                old_cwd: std::env::current_dir().unwrap(),
            };
            std::env::set_var("PWD", path);
            std::env::set_current_dir(path).unwrap();
            guard
        }
    }

    impl Drop for EnvCwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.old_cwd);
            if let Some(value) = &self.old_pwd {
                std::env::set_var("PWD", value);
            } else {
                std::env::remove_var("PWD");
            }
        }
    }

    fn write_spec_content(root: &Path, component_id: &str, content: &str) {
        let path = spec_path(root, component_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn verify_move_accepts_verbatim_move_and_repoint() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns a stable error envelope.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/workspaces",
            "---\ncomponent_id: g5/g5-api/routes/workspaces\n---\n# Workspaces Route\n\n[D3. Error shape]\n\n@ref operation_policy/error_envelope#D1 because workspaces use the shared envelope.\n",
        );

        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "from_component": "g5/g5-api/routes/workspaces",
                "from_block": "D3",
                "to_component": "operation_policy/error_envelope",
                "to_block": "D1",
                "moved_text": "Every handler returns a stable error envelope.",
                "verbatim": false
            })
            .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(report.valid, "{report:#?}");
        assert_eq!(report.checked, 2);
    }

    #[test]
    fn verify_move_applies_single_token_rename_for_destination_check() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns AppError.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns G5Error.",
            "verbatim": true,
            "rename": "G5Error->AppError"
        })
        .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(report.valid, "{report:#?}");
    }

    #[test]
    fn verify_move_reports_missing_destination_and_failed_delete() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nDifferent concept body.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\nEvery handler returns a stable error envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(!report.valid);
        assert_eq!(report.checked, 1);
        assert_eq!(report.violations.len(), 1);
        assert!(!report.violations[0].present_in_concept);
        assert!(!report.violations[0].absent_from_leaf);
    }

    #[test]
    fn verify_move_uses_declared_blocks_for_presence_and_absence() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nDifferent concept body.\n\n[D2. Other policy]\n\nEvery handler returns a stable error envelope.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n\n[D5. Historical copy]\n\nEvery handler returns a stable error envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(!report.valid);
        assert_eq!(report.checked, 1);
        assert_eq!(report.violations.len(), 1);
        assert!(!report.violations[0].present_in_concept);
        assert!(report.violations[0].absent_from_leaf);
    }

    #[test]
    fn verify_move_accepts_duplicate_text_outside_source_block() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns a stable error envelope.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n\n[D5. Historical copy]\n\nEvery handler returns a stable error envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(report.valid, "{report:#?}");
        assert_eq!(report.checked, 1);
    }

    #[test]
    fn verify_move_resolves_specs_from_all_roots() {
        let root_one = TempDir::new().unwrap();
        let root_two = TempDir::new().unwrap();
        write_spec_content(
            root_two.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns a stable error envelope.\n",
        );
        write_spec_content(
            root_two.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let roots = vec![root_one.path().to_path_buf(), root_two.path().to_path_buf()];
        let report = build_verify_move_report_for_roots(&roots, &manifest).unwrap();
        assert!(report.valid, "{report:#?}");
        assert_eq!(report.checked, 1);
    }

    #[test]
    fn verify_move_rename_does_not_replace_subtokens() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns AppErrorCode.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns G5ErrorCode.",
            "verbatim": true,
            "rename": "G5Error->AppError"
        })
        .to_string();

        let report = build_verify_move_report(dir.path(), &manifest).unwrap();
        assert!(!report.valid);
        assert_eq!(report.checked, 1);
        assert_eq!(report.violations.len(), 1);
        assert!(!report.violations[0].present_in_concept);
        assert!(report.violations[0].absent_from_leaf);
    }

    #[test]
    fn verify_move_rejects_malformed_rename() {
        let err =
            apply_verify_move_rename("Every handler returns G5Error.", Some("G5Error AppError"))
                .unwrap_err();
        assert!(err.to_string().contains("rename must use"));
    }

    #[test]
    fn verify_move_fails_closed_when_declared_block_is_missing() {
        let dir = TempDir::new().unwrap();
        write_spec_content(
            dir.path(),
            "operation_policy/error_envelope",
            "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns a stable error envelope.\n",
        );
        write_spec_content(
            dir.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D9",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let err = build_verify_move_report(dir.path(), &manifest).unwrap_err();
        assert!(err.to_string().contains("block 'D9' not found"));
    }

    #[test]
    fn verify_move_fails_closed_when_component_is_ambiguous_across_roots() {
        let root_one = TempDir::new().unwrap();
        let root_two = TempDir::new().unwrap();
        for root in [root_one.path(), root_two.path()] {
            write_spec_content(
                root,
                "operation_policy/error_envelope",
                "---\ncomponent_id: operation_policy/error_envelope\n---\n# Error Envelope\n\n[D1. Error shape]\n\nEvery handler returns a stable error envelope.\n",
            );
        }
        write_spec_content(
            root_one.path(),
            "g5/g5-api/routes/tasks",
            "---\ncomponent_id: g5/g5-api/routes/tasks\n---\n# Tasks Route\n\n[D4. Error shape]\n\n@ref operation_policy/error_envelope#D1 because routes use the shared envelope.\n",
        );
        let manifest = serde_json::json!({
            "from_component": "g5/g5-api/routes/tasks",
            "from_block": "D4",
            "to_component": "operation_policy/error_envelope",
            "to_block": "D1",
            "moved_text": "Every handler returns a stable error envelope.",
            "verbatim": true
        })
        .to_string();

        let roots = vec![root_one.path().to_path_buf(), root_two.path().to_path_buf()];
        let err = build_verify_move_report_for_roots(&roots, &manifest).unwrap_err();
        assert!(err.to_string().contains("ambiguous across roots"));
    }

    #[test]
    fn validate_specs_missing_source_warns_for_uncommitted_spec_then_fails_after_commit() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .expect("git runs");
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "--quiet"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);

        write_spec_content(
            root,
            "g5/newleaf",
            "---\ncomponent_id: g5/newleaf\ntitle: New Leaf\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files:\n- src/newleaf.rs\n---\n# New Leaf\n\n[D1. Behavior]\n\nBody.\n",
        );

        // Uncommitted spec: missing source is a typed pre-implementation
        // warning, not a hard failure (cli spec D52).
        let report = validate_specs_for_roots(
            &[root.to_path_buf()],
            SpecValidationTarget::Component("g5/newleaf".to_string()),
        );
        assert!(
            report.valid,
            "pre-implementation leaf must validate green: {:?}",
            report.issues
        );
        assert_eq!(
            report.pre_implementation_warnings.len(),
            1,
            "{:?}",
            report.pre_implementation_warnings
        );
        assert!(report.pre_implementation_warnings[0]
            .message
            .contains("does not exist yet"));

        // Committed spec: the same missing source is a hard failure again.
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "add spec"]);
        let committed = validate_specs_for_roots(
            &[root.to_path_buf()],
            SpecValidationTarget::Component("g5/newleaf".to_string()),
        );
        assert!(!committed.valid);
        assert!(
            committed.issues.iter().any(|i| i
                .message
                .contains("source file does not exist: src/newleaf.rs")),
            "{:?}",
            committed.issues
        );
        assert!(committed.pre_implementation_warnings.is_empty());
    }

    #[test]
    fn validate_specs_all_allows_identical_component_ids_across_repo_roots() {
        // sync_spec_remote D56: identical repo-local component ids in two
        // different visible repo roots are two distinct canonical identities
        // (multi_repo_workspace_ownership D8), not duplicates.
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("alpha");
        let root_b = dir.path().join("beta");
        for root in [&root_a, &root_b] {
            write_spec_content(
                root,
                "shared/dup",
                "---\ncomponent_id: shared/dup\ntitle: Shared Dup\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Shared Dup\n\n[D1. Behavior]\n\nBody.\n",
            );
        }

        let report = validate_specs_for_roots(&[root_a, root_b], SpecValidationTarget::All);
        assert!(
            !report
                .issues
                .iter()
                .any(|i| i.message.contains("duplicate component_id")),
            "cross-repo identical repo-local ids must not report duplicates: {:?}",
            report.issues
        );
        assert!(report.valid, "{:?}", report.issues);
    }

    #[test]
    fn validate_specs_all_does_not_satisfy_a_missing_reference_from_another_repo() {
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("alpha");
        let root_b = dir.path().join("beta");
        plant_spec(&root_a, "consumer", "dependencies:\n- shared/provider\n");
        plant_spec(&root_b, "shared/provider", "dependencies: []\n");

        let report = validate_specs_for_roots(&[root_a.clone(), root_b], SpecValidationTarget::All);
        assert!(!report.valid, "{:?}", report.issues);
        assert!(
            report.issues.iter().any(|issue| {
                issue.path.starts_with(&root_a.display().to_string())
                    && issue
                        .message
                        .contains("dependency references missing spec 'shared/provider'")
            }),
            "another repo's bare id must not satisfy alpha's reference: {:?}",
            report.issues
        );
    }

    #[test]
    fn validate_specs_all_detects_cycles_independently_in_each_repo_graph() {
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("alpha");
        let root_b = dir.path().join("beta");
        plant_spec(&root_a, "a", "dependencies:\n- b\n");
        plant_spec(&root_a, "b", "dependencies:\n- a\n");
        plant_spec(&root_b, "a", "dependencies: []\n");
        plant_spec(&root_b, "b", "dependencies: []\n");

        let report = validate_specs_for_roots(&[root_a.clone(), root_b], SpecValidationTarget::All);
        assert!(!report.valid, "{:?}", report.issues);
        assert!(
            report.issues.iter().any(|issue| {
                issue.path.starts_with(&root_a.display().to_string())
                    && issue.message.contains("dependency cycle")
                    && issue.message.contains("alpha/")
            }),
            "beta's duplicate ids must not overwrite alpha's cycle: {:?}",
            report.issues
        );
    }

    #[test]
    fn validate_specs_all_still_fails_intra_repo_duplicates_with_repo_attribution() {
        // D56 over-fix guard: two files inside ONE repo root claiming the same
        // component_id remain a hard duplicate failure, and in a multi-root
        // corpus each finding names its owning repo root.
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("alpha");
        let root_b = dir.path().join("beta");
        write_spec_content(
            &root_a,
            "shared/dup",
            "---\ncomponent_id: shared/dup\ntitle: Shared Dup\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Shared Dup\n\n[D1. Behavior]\n\nBody.\n",
        );
        // Second file in the SAME root claiming the same component_id.
        write_spec_content(
            &root_a,
            "shared/copy",
            "---\ncomponent_id: shared/dup\ntitle: Shared Copy\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Shared Copy\n\n[D1. Behavior]\n\nBody.\n",
        );
        write_spec_content(
            &root_b,
            "other/leaf",
            "---\ncomponent_id: other/leaf\ntitle: Other Leaf\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Other Leaf\n\n[D1. Behavior]\n\nBody.\n",
        );

        let report = validate_specs_for_roots(&[root_a.clone(), root_b], SpecValidationTarget::All);
        assert!(!report.valid);
        let dup_issues: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.message.contains("duplicate component_id 'shared/dup'"))
            .collect();
        assert_eq!(dup_issues.len(), 2, "{:?}", report.issues);
        assert!(
            dup_issues.iter().all(|i| i.message.contains("alpha")),
            "multi-root duplicate findings must carry repo attribution: {:?}",
            dup_issues
        );
        let canonical_root_a = std::fs::canonicalize(&root_a).unwrap();
        let routed_selector = format!("{}/shared/dup", canonical_root_a.display());
        let conflicting_paths = dup_issues
            .iter()
            .map(|issue| issue.path.clone())
            .collect::<Vec<_>>();
        assert!(dup_issues.iter().all(|issue| {
            issue.message.contains(&routed_selector)
                && conflicting_paths
                    .iter()
                    .all(|path| issue.message.contains(path))
        }), "multi-root duplicate findings must name their routed selector and every conflicting path: {dup_issues:?}");
    }

    #[test]
    fn validation_repo_key_without_inventory_uses_canonical_path_not_basename() {
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("one").join("shared");
        let root_b = dir.path().join("two").join("shared");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();

        let route_keys = HashMap::new();
        let key_a = validation_repo_key(&root_a, &route_keys);
        let key_b = validation_repo_key(&root_b, &route_keys);

        assert_eq!(
            key_a,
            std::fs::canonicalize(&root_a)
                .unwrap()
                .display()
                .to_string()
        );
        assert_eq!(
            key_b,
            std::fs::canonicalize(&root_b)
                .unwrap()
                .display()
                .to_string()
        );
        assert_ne!(key_a, key_b, "same-basename roots must remain distinct");
        assert_ne!(key_a, "shared");
        assert_ne!(key_b, "shared");
    }

    #[test]
    fn validate_specs_all_lint_diagnostics_attribute_to_owning_repo() {
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("alpha");
        let root_b = dir.path().join("beta");
        write_spec_content(
            &root_a,
            "shared/dup",
            "---\ncomponent_id: shared/dup\ntype: file\nsource_files: []\n---\n# Alpha\n\n[D1. Alpha behavior]\nAlpha owns this block.\n",
        );
        write_spec_content(
            &root_b,
            "shared/dup",
            "---\ncomponent_id: shared/dup\ntype: file\nsource_files: []\n---\n# Beta\n\n[D1. Beta behavior]\nBeta references @ref shared/dup#D404.\n",
        );
        let route_keys = HashMap::from([
            (root_a.clone(), "alpha".to_string()),
            (root_b.clone(), "beta".to_string()),
        ]);

        for roots in [
            vec![root_a.clone(), root_b.clone()],
            vec![root_b.clone(), root_a.clone()],
        ] {
            let report = validate_specs_for_roots_with_lint_mode_and_routes(
                &roots,
                SpecValidationTarget::All,
                OntologyLintMode::Strict,
                &route_keys,
            );
            let issue = report
                .issues
                .iter()
                .find(|issue| issue.message.contains("validate_references"))
                .expect("beta lint issue");
            assert_eq!(issue.component_id.as_deref(), Some("beta/shared/dup"));
            assert!(
                issue.path.starts_with(&root_b.display().to_string()),
                "lint issue must use beta's path under either root order: {issue:?}"
            );
        }
    }

    #[test]
    fn validate_specs_single_root_duplicate_message_stays_byte_compatible() {
        // D56: single-root workspaces keep byte-compatible duplicate output.
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        write_spec_content(
            &root,
            "shared/dup",
            "---\ncomponent_id: shared/dup\ntitle: Shared Dup\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Shared Dup\n\n[D1. Behavior]\n\nBody.\n",
        );
        write_spec_content(
            &root,
            "shared/copy",
            "---\ncomponent_id: shared/dup\ntitle: Shared Copy\ntype: file\nversion: 0.1.0\nlanguage: rust\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files: []\n---\n# Shared Copy\n\n[D1. Behavior]\n\nBody.\n",
        );

        let report = validate_specs_for_roots(&[root], SpecValidationTarget::All);
        assert!(!report.valid);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.message == "duplicate component_id 'shared/dup'"),
            "single-root duplicate message must stay byte-identical: {:?}",
            report.issues
        );
    }

    #[test]
    fn spec_create_resolves_repo_qualified_component_to_managed_repo_root() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(workspace.join(".g5")).unwrap();
        std::fs::create_dir_all(workspace.join("docs")).unwrap();
        std::fs::create_dir_all(workspace.join("tinydb")).unwrap();
        std::fs::write(
            workspace.join(".g5/workspace.yaml"),
            r#"
workspace_id: test-workspace
docs:
  repo_url: git@example.com/docs.git
  branch: main
  path: docs
repos:
  - name: tinydb
    url: git@example.com/tinydb.git
    branch: main
    path: tinydb
"#,
        )
        .unwrap();

        let rejected = resolve_managed_spec_create_target(&workspace, "aggregate").unwrap_err();
        assert!(
            format!("{rejected:?}").contains("repo-qualified component id"),
            "{rejected:?}"
        );

        let resolved = resolve_managed_spec_create_target(&workspace, "tinydb/aggregate")
            .unwrap()
            .unwrap();
        assert_eq!(resolved.workspace, workspace.join("tinydb"));
        assert_eq!(resolved.repo_local_component_id, "aggregate");
        assert_eq!(resolved.selector_component_id, "tinydb/aggregate");
        assert_eq!(resolved.route_key.as_deref(), Some("tinydb"));
    }

    #[test]
    #[serial_test::serial]
    fn stable_spec_create_requires_routed_selector_and_strips_route() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        std::fs::create_dir_all(root.join("tlslite-ng/.git")).unwrap();
        std::fs::create_dir_all(root.join("docs/.git")).unwrap();
        std::fs::write(
            stable_context::metadata_path(root),
            serde_json::json!({
                "schemaVersion": 1,
                "kind": "provided-agent-stable-chat-cwd",
                "sessionId": "session-1",
                "workspaceId": "workspace-1",
                "agentProvider": "claude_code",
                "createdByTaskId": "task-1",
                "repoIds": ["repo-code", "repo-docs"],
                "mounts": [
                    {
                        "repoId": "repo-code",
                        "mountPath": "tlslite-ng",
                        "repoName": "tlslite-ng",
                        "repoRole": "code"
                    },
                    {
                        "repoId": "repo-docs",
                        "mountPath": "docs",
                        "repoName": "docs",
                        "repoRole": "docs"
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let _restore = EnvCwdRestore::set(root);
        let global = GlobalFlags {
            workspace: None,
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let stable = stable_context::discover(&global).unwrap().unwrap();
        let rejected = resolve_stable_spec_create_target(&stable, "tlslite_aggregate").unwrap_err();
        assert!(
            format!("{rejected:?}").contains("repo-qualified component id"),
            "{rejected:?}"
        );

        let resolved =
            resolve_stable_spec_create_target(&stable, "tlslite-ng/tlslite_aggregate").unwrap();
        assert_eq!(resolved.workspace, root.join("tlslite-ng"));
        assert_eq!(resolved.repo_local_component_id, "tlslite_aggregate");
        assert_eq!(
            resolved.selector_component_id,
            "tlslite-ng/tlslite_aggregate"
        );
        assert_eq!(resolved.route_key.as_deref(), Some("tlslite-ng"));
    }

    #[test]
    #[serial_test::serial]
    fn spec_list_roots_from_stable_context_use_mounts_not_ambient_root() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        std::fs::create_dir_all(root.join(".semlift/specs")).unwrap();
        std::fs::create_dir_all(root.join("tlslite-ng/.git")).unwrap();
        std::fs::create_dir_all(root.join("docs/.git")).unwrap();
        std::fs::write(
            stable_context::metadata_path(root),
            serde_json::json!({
                "schemaVersion": 1,
                "kind": "provided-agent-stable-chat-cwd",
                "sessionId": "session-1",
                "workspaceId": "workspace-1",
                "agentProvider": "claude_code",
                "createdByTaskId": "task-1",
                "repoIds": ["repo-code", "repo-docs"],
                "mounts": [
                    {
                        "repoId": "repo-code",
                        "mountPath": "tlslite-ng",
                        "repoName": "tlslite-ng",
                        "repoRole": "code"
                    },
                    {
                        "repoId": "repo-docs",
                        "mountPath": "docs",
                        "repoName": "docs",
                        "repoRole": "docs"
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let _restore = EnvCwdRestore::set(root);
        let global = GlobalFlags {
            workspace: None,
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let roots = resolve_spec_list_roots(root, &global).unwrap();
        let workspaces = roots
            .iter()
            .map(|root| root.workspace.clone())
            .collect::<BTreeSet<_>>();
        assert!(workspaces.contains(&root.join("tlslite-ng")));
        assert!(workspaces.contains(&root.join("docs")));
        assert!(!workspaces.contains(&root.to_path_buf()));
    }

    #[test]
    #[serial_test::serial]
    fn resolve_spec_workspace_uses_stable_root_without_workspace_yaml() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".g5")).unwrap();
        std::fs::write(
            stable_context::metadata_path(dir.path()),
            serde_json::json!({
                "schemaVersion": 1,
                "kind": "provided-agent-stable-chat-cwd",
                "sessionId": "session-1",
                "workspaceId": "workspace-1",
                "agentProvider": "claude_code",
                "createdByTaskId": "task-1",
                "repoIds": ["repo-1"],
                "mounts": []
            })
            .to_string(),
        )
        .unwrap();

        let _restore = EnvCwdRestore::set(dir.path());
        let global = GlobalFlags {
            workspace: None,
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let workspace = resolve_spec_workspace(&global).unwrap();
        assert_eq!(workspace, dir.path().to_path_buf());
    }

    #[test]
    fn validate_specs_rejects_gitignored_source_file_even_when_present() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .expect("git runs");
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "--quiet"]);
        std::fs::write(root.join(".gitignore"), ".agents/\n").unwrap();
        let ignored_source = root
            .join(".agents")
            .join("skills")
            .join("g5-rebase-workflow")
            .join("SKILL.md");
        std::fs::create_dir_all(ignored_source.parent().unwrap()).unwrap();
        std::fs::write(&ignored_source, "# Local mirror\n").unwrap();

        write_spec_content(
            root,
            "g5/local-agent-assets",
            "---\ncomponent_id: g5/local-agent-assets\ntitle: Local Agent Assets\ntype: file\nversion: 0.1.0\nlanguage: markdown\ncomplexity: low\nparent_spec_id: null\nchild_spec_ids: []\ndependencies: []\nsource_files:\n- .agents/skills/g5-rebase-workflow/SKILL.md\n---\n# Local Agent Assets\n\n[D1. Behavior]\n\nBody.\n",
        );

        let report = validate_specs_for_roots(
            &[root.to_path_buf()],
            SpecValidationTarget::Component("g5/local-agent-assets".to_string()),
        );
        assert!(!report.valid);
        assert!(
            report.issues.iter().any(|issue| issue.message.contains(
                "source file is ignored by git and will be absent from clean CI checkouts"
            )),
            "{:?}",
            report.issues
        );
        assert!(report.pre_implementation_warnings.is_empty());
    }

    #[test]
    fn validate_specs_all_recurses_and_reports_valid_graph() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub mod child;\n").unwrap();
        std::fs::write(dir.path().join("src/child.rs"), "pub fn run() {}\n").unwrap();

        plant_spec(
            dir.path(),
            "root",
            "child_spec_ids:\n- root/child\nsource_files:\n- src/lib.rs\n",
        );
        plant_spec(
            dir.path(),
            "root/child",
            "parent_spec_id: root\ndependency_spec_ids:\n- root\nsource_files:\n- src/child.rs\n",
        );

        let report =
            validate_specs_for_roots(&[dir.path().to_path_buf()], SpecValidationTarget::All);
        assert!(report.valid, "{:?}", report.issues);
        assert_eq!(report.checked, 2);
        assert_eq!(report.ontology_markup.len(), 2);
    }

    #[test]
    fn validate_specs_flags_duplicate_block_ids() {
        let dir = TempDir::new().unwrap();
        let spec_dir = dir.path().join(".semlift").join("specs").join("auth");
        std::fs::create_dir_all(&spec_dir).unwrap();
        std::fs::write(
            spec_dir.join("login.semlift.md"),
            "---\ncomponent_id: auth/login\n---\n# Auth Login\n\n[D1. Scope]\nFirst definition.\n\n[D1]\nWrapped prose reference at a line start.\n",
        )
        .unwrap();

        let report =
            validate_specs_for_roots(&[dir.path().to_path_buf()], SpecValidationTarget::All);
        assert!(!report.valid);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.message.contains("duplicate block id 'D1'")),
            "issues: {:?}",
            report.issues
        );
    }

    #[test]
    fn validate_specs_reports_additive_ontology_markup_summary() {
        let dir = TempDir::new().unwrap();
        let concept_path = dir
            .path()
            .join(".semlift")
            .join("specs")
            .join("data_model")
            .join("company.semlift.md");
        let spec_path = dir
            .path()
            .join(".semlift")
            .join("specs")
            .join("software_architecture")
            .join("company_model.semlift.md");
        std::fs::create_dir_all(concept_path.parent().unwrap()).unwrap();
        std::fs::write(
            &concept_path,
            "---\ncomponent_id: data_model/company\ntype: file\n---\n# Company\n\n[D5. Physical shape]\n\nCompany table shape.\n",
        )
        .unwrap();
        std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
        std::fs::write(
            &spec_path,
            "---\ncomponent_id: software_architecture/company_model\n---\n# Company Model\n\n[D1. Code-level surface]\n\nThis architecture leaf realizes @ref data_model/company#D5.\n**Binding:** maps the logical entity fields into concrete columns.\n",
        )
        .unwrap();

        let report =
            validate_specs_for_roots(&[dir.path().to_path_buf()], SpecValidationTarget::All);
        assert!(report.valid, "{:?}", report.issues);
        assert_eq!(report.checked, 2);

        let json = serde_json::to_value(&report).unwrap();
        let architecture = json["ontologyMarkup"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["componentId"] == "software_architecture/company_model")
            .unwrap();
        assert_eq!(architecture["era"], "v4_candidate");
        assert_eq!(architecture["nodeClass"], "softwareArchitecture");
        assert_eq!(architecture["inlineRefCount"], 1);
        assert_eq!(architecture["bindingCount"], 1);
        assert_eq!(json["ontologyLint"]["valid"], true);
    }

    #[test]
    fn validate_specs_gates_phase1_lint_diagnostics() {
        let dir = TempDir::new().unwrap();
        let concept_path = dir
            .path()
            .join(".semlift")
            .join("specs")
            .join("data_model")
            .join("company.semlift.md");
        let spec_path = dir
            .path()
            .join(".semlift")
            .join("specs")
            .join("software_architecture")
            .join("company_model.semlift.md");
        std::fs::create_dir_all(concept_path.parent().unwrap()).unwrap();
        std::fs::write(
            &concept_path,
            "---\ncomponent_id: data_model/company\ntype: file\n---\n# Company\n\n[I1. Company concept]\n\nCompany shape overview.\n",
        )
        .unwrap();
        std::fs::create_dir_all(spec_path.parent().unwrap()).unwrap();
        std::fs::write(
            &spec_path,
            "---\ncomponent_id: software_architecture/company_model\ntype: file\n---\n# Company Model\n\n## References\n\n- `@ref data_model/company#D404` - missing block\n\n## Behavior\n\n[D1. Weak model]\n\nThe model depends on @ref data_model/company for shape.\nIt also claims @ref data_model/company#D404\n",
        )
        .unwrap();

        let report =
            validate_specs_for_roots(&[dir.path().to_path_buf()], SpecValidationTarget::All);
        assert!(report.valid, "{:?}", report.issues);

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["ontologyLintMode"], "off");
        assert_eq!(json["ontologyLint"]["valid"], true);
        assert!(json["ontologyLint"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(report.issues.is_empty());

        let strict_report = validate_specs_for_roots_with_lint_mode(
            &[dir.path().to_path_buf()],
            SpecValidationTarget::All,
            OntologyLintMode::Strict,
        );
        assert!(!strict_report.valid);

        let json = serde_json::to_value(&strict_report).unwrap();
        assert_eq!(json["ontologyLintMode"], "strict");
        assert_eq!(json["ontologyLint"]["valid"], false);
        let diagnostics = json["ontologyLint"]["diagnostics"].as_array().unwrap();
        let rules = diagnostics
            .iter()
            .map(|diagnostic| diagnostic["ruleId"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rules.contains("validate_references"), "{rules}");
        assert!(rules.contains("ref_quality_lint"), "{rules}");

        let issues = strict_report
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(issues.contains("validate_references"), "{issues}");
        assert!(issues.contains("ref_quality_lint"), "{issues}");
    }

    #[test]
    fn validate_specs_all_reports_missing_parent_dependency_and_cycle() {
        let dir = TempDir::new().unwrap();
        plant_spec(
            dir.path(),
            "a",
            "parent_spec_id: missing\ndependencies:\n- b\nsrc_files:\n- ../escape.rs\n",
        );
        plant_spec(dir.path(), "b", "dependencies:\n- a\n");

        let report =
            validate_specs_for_roots(&[dir.path().to_path_buf()], SpecValidationTarget::All);
        assert!(!report.valid);
        let messages = report
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(messages.contains("parent_spec_id references missing spec"));
        assert!(messages.contains("source file path must not escape"));
        assert!(messages.contains("dependency cycle"));
    }

    #[test]
    #[serial_test::serial]
    fn resolve_validation_roots_uses_stable_single_root_without_workspace_yaml() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".g5")).unwrap();
        std::fs::write(
            stable_context::metadata_path(dir.path()),
            serde_json::json!({
                "schemaVersion": 1,
                "kind": "provided-agent-stable-chat-cwd",
                "sessionId": "session-1",
                "workspaceId": "workspace-1",
                "agentProvider": "claude_code",
                "createdByTaskId": "task-1",
                "repoIds": ["repo-1"],
                "mounts": []
            })
            .to_string(),
        )
        .unwrap();

        let _restore = EnvCwdRestore::set(dir.path());

        let global = GlobalFlags {
            workspace: None,
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let roots = resolve_validation_roots(&global).unwrap();
        assert_eq!(roots, vec![dir.path().to_path_buf()]);
    }
}
