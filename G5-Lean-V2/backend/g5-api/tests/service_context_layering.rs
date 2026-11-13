//! Bounded-context layering of `g5-api-core` services.
//!
//! `g5-api-core/src/services` is the `email`, `identity`, `git`, `hosts`,
//! `workspace` and `agents` contexts, each depending only on earlier ones
//! (`.g5/pr_ontology_diff_design/p8.5-crate-consolidation/ontology-plan.md`,
//! "Tranche 2, bounded contexts"), with the maintenance jobs inside the
//! context they serve. Every file under a context directory belongs to that
//! context (plus the credential, runtime, cloud and grounding files assigned
//! below), and no file names a module of a later context, whether through
//! its context path or through the old `crate::services::<name>` and
//! `crate::maintenance::<job>` paths that the composition roots keep as
//! re-exports. A later context is reached only through a trait the earlier
//! one owns, installed in `services/mod.rs`, which is the composition root
//! and exempt.
//!
//! A file's module identity comes from the declarations that reach it from
//! `lib.rs` (`mod x;`, `#[path]`, inline modules), not from its file name.
//! Scanned references: `use` trees (through the names they bind in their
//! lexical scope, so `use crate::services as svc; svc::task_service::X` and
//! `self::svc::X` are seen, and two functions may bind the same name
//! differently), qualified paths in code including the types inside generic
//! arguments and qualified-trait forms, and `a::b` chains inside macro
//! arguments, resolved against that identity, the modules the file declares
//! and the names the composition roots re-export. A glob import of a
//! composition root, or a glob re-export from one, hides what it brings in
//! and is refused. Field access through `AppState` is not a path and is not
//! seen; the move itself will make those structural.
//!
//! A new file outside the context directories fails until it is assigned.

use crate::source_scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Context {
    Email,
    Identity,
    Git,
    Hosts,
    Workspace,
    Agents,
    /// Composition roots: wire every context, scanned by nobody.
    Composition,
}

use Context::*;

/// Relative to `g5-api-core/src`. A key without `.rs` names a directory and
/// everything beneath it: each context directory, and the files outside
/// `services/` that belong to a context.
const CONTEXTS: &[(&str, Context)] = &[
    // email: outbound transactional email every context may request; it
    // names no other context.
    ("services/email", Email),
    // identity: who may do what in an organization, and the leaves every
    // context reads.
    ("services/identity", Identity),
    // git: refs, landings, remotes, pull requests and provider issues.
    ("services/git", Git),
    ("auth/repository_credentials.rs", Git),
    ("git_runtime.rs", Git),
    // hosts: worker hosts, VM custody and the cloud that supplies them.
    ("services/hosts", Hosts),
    ("cloud", Hosts),
    // workspace: the workspace, its VM lifecycle, documents, ontology,
    // reviews and approvals.
    ("services/workspace", Workspace),
    ("grounding_projection.rs", Workspace),
    // agents: tasks, chats, jobs and the agent configuration they run with.
    ("services/agents", Agents),
    // composition roots
    ("services/mod.rs", Composition),
    ("maintenance/mod.rs", Composition),
];

/// Every file (or directory) under the context directories, with the context it
/// was reviewed into. A file sitting in another context directory, or a new file
/// without an entry, fails: directory placement alone is not ownership review.
const HOMES: &[(&str, Context)] = &[
    // email
    ("markdown.rs", Email),
    ("render.rs", Email),
    ("store.rs", Email),
    ("transport.rs", Email),
    ("worker.rs", Email),
    // identity
    ("api_key_service.rs", Identity),
    ("auth_service.rs", Identity),
    ("github_app_auth.rs", Identity),
    ("invitation_gc.rs", Identity),
    ("membership_activity.rs", Identity),
    ("notification_service.rs", Identity),
    ("org_admin_settings.rs", Identity),
    ("org_service.rs", Identity),
    ("permission_service.rs", Identity),
    ("platform_issue_github.rs", Identity),
    ("platform_issue_intake.rs", Identity),
    ("platform_issue_outbox.rs", Identity),
    ("platform_operator.rs", Identity),
    ("provider_quota_service.rs", Identity),
    ("self_hosted_license_service.rs", Identity),
    ("session_pruning.rs", Identity),
    ("spend_service.rs", Identity),
    ("usage_aggregation.rs", Identity),
    ("user_service.rs", Identity),
    ("workspace_incarnation.rs", Identity),
    ("workspace_mutation_policy.rs", Identity),
    ("workspace_names.rs", Identity),
    // git
    ("advance_consumer.rs", Git),
    ("branch_commit.rs", Git),
    ("workspace_revision_history.rs", Git),
    ("branch_integration_scheduler.rs", Git),
    ("branch_integration_service.rs", Git),
    ("branch_integration_worker.rs", Git),
    ("branch_sync_runs_service.rs", Git),
    ("coordinated_repo_publication.rs", Git),
    ("pull_request_adapters.rs", Git),
    ("external_issues", Git),
    ("external_issues.rs", Git),
    ("git_conflict_resolution.rs", Git),
    ("git_policy.rs", Git),
    ("git_test_barrier.rs", Git),
    ("github_app.rs", Git),
    ("github_app_binding.rs", Git),
    ("github_issues.rs", Git),
    ("github_user_grants.rs", Git),
    ("issue_comments.rs", Git),
    ("issue_creations.rs", Git),
    ("issue_labels.rs", Git),
    ("merge_override.rs", Git),
    ("native_issue_query.rs", Git),
    ("native_issues", Git),
    ("native_issues.rs", Git),
    ("native_issues", Git),
    ("workspace_merge_service.rs", Git),
    ("remote_advisory_pinger.rs", Git),
    ("remote_integrations", Git),
    ("remote_integrations.rs", Git),
    ("remote_sync_run_retention.rs", Git),
    ("remote_tracking_merge.rs", Git),
    ("repo_gc.rs", Git),
    ("repo_store.rs", Git),
    ("source_control_change_graph_service.rs", Git),
    ("unified_diff.rs", Git),
    ("workspace_flush_retention.rs", Git),
    ("workspace_flush_service.rs", Git),
    ("workspace_ops.rs", Git),
    ("workspace_pull_request_auto_merge.rs", Git),
    ("workspace_pull_request_checks.rs", Git),
    ("workspace_pull_request_attachments.rs", Git),
    ("workspace_pull_request_conversation.rs", Git),
    ("workspace_pull_request_list.rs", Git),
    ("workspace_pull_request_description.rs", Git),
    ("workspace_pull_request_create.rs", Git),
    ("workspace_pull_request_freshness.rs", Git),
    ("workspace_pull_request_provider_lifecycle.rs", Git),
    ("workspace_pull_request_provider_merge.rs", Git),
    ("workspace_pull_request_read.rs", Git),
    ("workspace_audience.rs", Git),
    ("workspace_pull_requests.rs", Git),
    ("workspace_reader.rs", Git),
    ("workspace_repositories.rs", Git),
    // hosts
    ("host_capacity.rs", Hosts),
    ("host_control_state.rs", Hosts),
    ("live_hints.rs", Hosts),
    ("host_provisioning.rs", Hosts),
    ("host_registry.rs", Hosts),
    ("runtime_label_resolver.rs", Hosts),
    ("source_preparation.rs", Hosts),
    ("worker_host_service.rs", Hosts),
    ("workspace_vm_release.rs", Hosts),
    // workspace
    ("approval_activity.rs", Workspace),
    ("approval_config_service.rs", Workspace),
    ("approval_config_service", Workspace), // The pure requirement evaluator the approvals projection and merge admission share.
    ("approval_definition_service.rs", Workspace),
    ("approval_request_email.rs", Workspace),
    ("approval_request_retention.rs", Workspace),
    ("base_reverification.rs", Workspace),
    ("comment_service.rs", Workspace),
    ("doc_reference_service.rs", Workspace),
    ("doc_service.rs", Workspace),
    ("home_service.rs", Workspace),
    ("intent_log_service.rs", Workspace),
    ("preview_config.rs", Workspace),
    ("preview_run.rs", Workspace),
    ("question_set_service.rs", Workspace),
    ("question_vote_service.rs", Workspace),
    ("review_edits.rs", Workspace),
    ("workspace_review_queries.rs", Workspace),
    ("review_source_notifications.rs", Workspace),
    ("workspace_activity.rs", Workspace),
    ("personal_workspace.rs", Workspace),
    ("workspace_file_save.rs", Workspace),
    ("workspace_fork_service.rs", Workspace),
    ("workspace_kind.rs", Workspace),
    ("workspace_lifecycle.rs", Workspace),
    ("workspace_owner_email.rs", Workspace),
    ("workspace_owner_limit.rs", Workspace),
    ("workspace_review_service.rs", Workspace),
    ("workspace_runtime.rs", Workspace),
    ("workspace_service.rs", Workspace),
    ("workspace_vm_service.rs", Workspace),
    // agents
    ("agent_attachment_retention.rs", Agents),
    ("agent_dispatch.rs", Agents),
    ("agent_steering.rs", Agents),
    ("agent_tasks.rs", Agents),
    ("attempt_follower.rs", Agents),
    ("segment_map.rs", Agents),
    ("approval_review_service.rs", Agents),
    ("approval_review_service", Agents), // Durable formal-review coordination; effects stay with fork/task/VM owners.
    ("attempt_snapshot.rs", Agents),
    ("chat_activity.rs", Agents),
    ("chat_ontology.rs", Agents),
    ("chat_service.rs", Agents),
    ("chat_session_file_service.rs", Agents),
    ("codex_auth.rs", Agents),
    ("config_service.rs", Agents),
    ("provider_account_service.rs", Agents),
    ("session_file_sha256.rs", Agents),
    ("subscription_usage.rs", Agents),
    ("task_lifecycle.rs", Agents),
    ("task_service.rs", Agents),
    ("timeline_cleanup.rs", Agents),
    ("workbench_config.rs", Agents),
    ("workspace_messages.rs", Agents),
];

/// Backward references tolerated for now: `(file, referenced file, why)`.
/// Keep it empty.
const ALLOWED_BACKWARD: &[(&str, &str, &str)] = &[];

/// A reference from one file to a module of a later context.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BackwardEdge {
    file: String,
    line: usize,
    from: Context,
    target: String,
    to: Context,
}

/// A module path from the crate root.
type ModulePath = Vec<String>;

fn core_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api lives beneath backend")
        .join("g5-api-core")
        .join("src")
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .expect("file under core src")
        .to_string_lossy()
        .replace('\\', "/")
}

/// The context a file or module path belongs to, by its longest assigned
/// prefix.
fn context_of(relative_path: &str) -> Option<(&'static str, Context)> {
    CONTEXTS
        .iter()
        .filter(|(key, _)| {
            relative_path == *key
                || (!key.ends_with(".rs") && relative_path.starts_with(&format!("{key}/")))
        })
        .max_by_key(|(key, _)| key.len())
        .map(|(key, context)| (*key, *context))
}

/// The module a file is, and whether it compiles only in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleIdentity {
    path: ModulePath,
    test_only: bool,
}

/// Module identity of every file the crate root reaches through `mod x;`
/// (with or without `#[path]`) and inline modules, keyed by path relative
/// to `src`. `services/workspace_pull_request_checks.rs` is
/// `services::workspace_pull_request_provider_lifecycle::read::checks`.
fn module_map(root: &Path) -> BTreeMap<String, ModuleIdentity> {
    fn visit(
        root: &Path,
        file: &Path,
        identity: ModuleIdentity,
        map: &mut BTreeMap<String, ModuleIdentity>,
    ) {
        let key = relative(file, root);
        if map.contains_key(&key) {
            return;
        }
        let source = std::fs::read_to_string(file).expect("read core source");
        let tree = source_scan::parse_rust(&source);
        let declarations = source_scan::module_declarations(file, &source, &tree);
        map.insert(key, identity.clone());
        for declaration in declarations {
            let Some(target) = declaration
                .candidates
                .iter()
                .find(|candidate| candidate.is_file())
            else {
                continue;
            };
            let mut path = identity.path.clone();
            path.extend(declaration.inline.iter().cloned());
            path.push(declaration.name.clone());
            let child = ModuleIdentity {
                path,
                test_only: identity.test_only || declaration.test_only,
            };
            visit(root, target, child, map);
        }
    }
    let mut map = BTreeMap::new();
    let crate_root = ModuleIdentity {
        path: Vec::new(),
        test_only: false,
    };
    visit(root, &root.join("lib.rs"), crate_root, &mut map);
    map
}

fn compact(node: Node<'_>, source: &str) -> String {
    unraw(node.utf8_text(source.as_bytes()).unwrap_or_default())
}

/// Whitespace removed and raw identifiers normalized: `r#task_service`
/// names the module `task_service`.
fn unraw(text: &str) -> String {
    text.split_whitespace()
        .collect::<String>()
        .replace("r#", "")
}

fn join(prefix: &str, rest: &str) -> String {
    if rest == "self" || rest.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        rest.to_string()
    } else {
        format!("{prefix}::{rest}")
    }
}

/// Every full path a `use` tree names, with the name it binds (`*` for a
/// glob).
fn expand_use(node: Node<'_>, source: &str, prefix: &str, out: &mut Vec<(String, String)>) {
    match node.kind() {
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if !matches!(child.kind(), "line_comment" | "block_comment") {
                    expand_use(child, source, prefix, out);
                }
            }
        }
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .map(|path| join(prefix, &compact(path, source)))
                .unwrap_or_else(|| prefix.to_string());
            if let Some(list) = node.child_by_field_name("list") {
                expand_use(list, source, &path, out);
            }
        }
        "use_as_clause" => {
            let path = node
                .child_by_field_name("path")
                .map(|path| join(prefix, &compact(path, source)))
                .unwrap_or_else(|| prefix.to_string());
            let alias = node
                .child_by_field_name("alias")
                .map(|alias| compact(alias, source))
                .unwrap_or_default();
            out.push((path, alias));
        }
        "use_wildcard" => {
            let path = node
                .named_child(0)
                .map(|path| join(prefix, &compact(path, source)))
                .unwrap_or_else(|| prefix.to_string());
            out.push((path, "*".to_string()));
        }
        _ => {
            let path = join(prefix, &compact(node, source));
            let name = path.rsplit("::").next().unwrap_or_default().to_string();
            out.push((path, name));
        }
    }
}

/// What one scanned path does: binds a name through `use`, glob-imports a
/// module, or names a path in code or a macro.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Binding {
    Name(String),
    Glob,
    Path,
}

/// Where a reference or binding sits: the inline modules enclosing it and
/// the blocks (function bodies and nested blocks) enclosing it inside that
/// module. A `use` in a block binds its name only inside that block.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    scope: Vec<String>,
    blocks: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Reference {
    /// As written.
    path: String,
    binding: Binding,
    /// The `use` is `pub` (a re-export when it sits in a composition root).
    public: bool,
    place: Place,
    line: usize,
}

/// Production code of one file: every path it names, the names its `use`
/// items bind (every alternative, since conditional items may bind one name
/// differently) and the modules it declares, each with its place.
#[derive(Debug, Default)]
struct Scan {
    references: Vec<Reference>,
    /// `(place, name)` of a `use` item, with the paths as written.
    bindings: BTreeMap<(Place, String), Vec<String>>,
    /// `(scope, name)` of every declared `mod`.
    declared: BTreeSet<(Vec<String>, String)>,
}

/// The segments of a scoped path (`a::b::c`), skipping generic arguments
/// and qualified-trait forms, which come back as nodes to scan on their own.
/// `Vec::<T>::new` is `Vec::new` plus `T`; `<T as Trait>::f` is only `T`.
fn scoped_segments<'tree>(node: Node<'tree>, source: &str) -> (Option<String>, Vec<Node<'tree>>) {
    let mut reversed = Vec::new();
    let mut extra = Vec::new();
    let mut current = node;
    let mut qualified = false;
    loop {
        if let Some(name) = current.child_by_field_name("name") {
            reversed.push(compact(name, source));
        }
        let Some(path) = current.child_by_field_name("path") else {
            break;
        };
        match path.kind() {
            "scoped_identifier" | "scoped_type_identifier" => current = path,
            "identifier" | "type_identifier" | "crate" | "self" | "super" | "metavariable" => {
                reversed.push(compact(path, source));
                break;
            }
            "generic_type" | "generic_type_with_turbofish" => {
                if let Some(arguments) = path.child_by_field_name("type_arguments") {
                    extra.push(arguments);
                }
                match path.child_by_field_name("type") {
                    Some(inner)
                        if matches!(
                            inner.kind(),
                            "scoped_identifier" | "scoped_type_identifier"
                        ) =>
                    {
                        current = inner;
                    }
                    Some(inner) => {
                        reversed.push(compact(inner, source));
                        break;
                    }
                    None => break,
                }
            }
            _ => {
                extra.push(path);
                qualified = true;
                break;
            }
        }
    }
    if qualified || reversed.is_empty() {
        return (None, extra);
    }
    reversed.reverse();
    (Some(reversed.join("::")), extra)
}

fn scan(source: &str, tree: &tree_sitter::Tree) -> Scan {
    fn visit(node: Node<'_>, source: &str, place: &mut Place, out: &mut Scan) {
        if source_scan::any_excludes_production(&source_scan::item_attributes(node, source)) {
            return;
        }
        let line = node.start_position().row + 1;
        match node.kind() {
            "mod_item" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|name| compact(name, source))
                    .unwrap_or_default();
                out.declared.insert((place.scope.clone(), name.clone()));
                if let Some(body) = node.child_by_field_name("body") {
                    let blocks = std::mem::take(&mut place.blocks);
                    place.scope.push(name);
                    visit(body, source, place, out);
                    place.scope.pop();
                    place.blocks = blocks;
                }
                return;
            }
            "block" => {
                place.blocks.push(node.id());
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    visit(child, source, place, out);
                }
                place.blocks.pop();
                return;
            }
            "use_declaration" => {
                let mut children = node.walk();
                let public = node
                    .children(&mut children)
                    .any(|child| child.kind() == "visibility_modifier");
                if let Some(argument) = node.child_by_field_name("argument") {
                    let mut paths = Vec::new();
                    expand_use(argument, source, "", &mut paths);
                    for (path, name) in paths {
                        let binding = if name == "*" {
                            Binding::Glob
                        } else {
                            out.bindings
                                .entry((place.clone(), name.clone()))
                                .or_default()
                                .push(path.clone());
                            Binding::Name(name)
                        };
                        out.references.push(Reference {
                            path,
                            binding,
                            public,
                            place: place.clone(),
                            line,
                        });
                    }
                }
                return;
            }
            "scoped_identifier" | "scoped_type_identifier" => {
                let (path, extra) = scoped_segments(node, source);
                if let Some(path) = path {
                    out.references.push(Reference {
                        path,
                        binding: Binding::Path,
                        public: false,
                        place: place.clone(),
                        line,
                    });
                }
                for child in extra {
                    visit(child, source, place, out);
                }
                return;
            }
            "token_tree" => {
                let mut tokens = Vec::new();
                leaf_tokens(node, source, &mut tokens);
                let mut index = 0;
                while index < tokens.len() {
                    let (token, token_line) = tokens[index];
                    if let Some(head) = identifier(token)
                        .filter(|_| tokens.get(index + 1).map(|t| t.0) == Some("::"))
                    {
                        let mut path = head.to_string();
                        let mut cursor = index + 1;
                        while tokens.get(cursor).map(|t| t.0) == Some("::") {
                            match tokens.get(cursor + 1).and_then(|t| identifier(t.0)) {
                                Some(segment) => {
                                    path.push_str("::");
                                    path.push_str(segment);
                                    cursor += 2;
                                }
                                None => break,
                            }
                        }
                        out.references.push(Reference {
                            path,
                            binding: Binding::Path,
                            public: false,
                            place: place.clone(),
                            line: token_line,
                        });
                        index = cursor;
                    } else {
                        index += 1;
                    }
                }
                return;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, place, out);
        }
    }

    let mut out = Scan::default();
    visit(tree.root_node(), source, &mut Place::default(), &mut out);
    out
}

/// The identifier a macro token spells, raw prefix removed; `$crate` in a
/// macro definition is the crate root.
fn identifier(token: &str) -> Option<&str> {
    if token == "$crate" {
        return Some("crate");
    }
    let token = token.strip_prefix("r#").unwrap_or(token);
    let plain = token
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    plain.then_some(token)
}

fn leaf_tokens<'a>(node: Node<'_>, source: &'a str, tokens: &mut Vec<(&'a str, usize)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string_literal" | "raw_string_literal" | "char_literal" | "line_comment"
            | "block_comment" => {}
            _ if child.child_count() == 0 => tokens.push((
                child.utf8_text(source.as_bytes()).unwrap_or_default(),
                child.start_position().row + 1,
            )),
            _ => leaf_tokens(child, source, tokens),
        }
    }
}

/// The `use` bindings of `name` visible at `place`, with the place that
/// binds them (where their own paths resolve): the innermost enclosing block
/// first, then the module itself, then (an over-approximation that can only
/// add edges) the enclosing inline modules.
fn bound<'a>(scan: &'a Scan, place: &Place, name: &str) -> Option<(Place, &'a Vec<String>)> {
    (0..=place.blocks.len())
        .rev()
        .map(|length| Place {
            scope: place.scope.clone(),
            blocks: place.blocks[..length].to_vec(),
        })
        .chain((0..place.scope.len()).rev().map(|length| Place {
            scope: place.scope[..length].to_vec(),
            blocks: Vec::new(),
        }))
        .find_map(|candidate| {
            scan.bindings
                .get(&(candidate.clone(), name.to_string()))
                .map(|paths| (candidate, paths))
        })
}

/// A path written in `module` at `place`, as every module path from the
/// crate root it can mean: `crate::` is absolute; `self::` and `super::`
/// chains reach a module of this file, where the next segment may again be
/// a name `use` binds there; a bare first segment is a bound name, a module
/// declared in that scope, or an external crate (returned as written, which
/// matches no context).
fn resolve(
    module: &[String],
    place: &Place,
    scan: &Scan,
    path: &str,
    depth: usize,
) -> Vec<ModulePath> {
    let mut segments: Vec<String> = path.split("::").map(str::to_owned).collect();
    let here: ModulePath = module.iter().chain(&place.scope).cloned().collect();
    let first = segments.first().cloned().unwrap_or_default();
    let through_bindings = |reached_place: &Place,
                            segments: Vec<String>|
     -> Option<Vec<ModulePath>> {
        let next = segments.first()?;
        let (defining, alternatives) = bound(scan, reached_place, next)?;
        if depth >= 8 {
            return None;
        }
        let rest = &segments[1..];
        Some(
            alternatives
                .iter()
                .flat_map(|alternative| resolve(module, &defining, scan, alternative, depth + 1))
                .map(|mut resolved| {
                    resolved.extend(rest.iter().cloned());
                    resolved
                })
                .collect(),
        )
    };
    let mut base = match first.as_str() {
        "crate" => {
            segments.remove(0);
            Vec::new()
        }
        "self" | "super" => {
            if first == "self" {
                segments.remove(0);
            }
            let mut base = here;
            while segments.first().map(String::as_str) == Some("super") {
                segments.remove(0);
                base.pop();
            }
            base
        }
        _ => {
            if let Some(resolved) = through_bindings(place, segments.clone()) {
                return resolved;
            }
            if scan
                .declared
                .contains(&(place.scope.clone(), first.clone()))
            {
                here
            } else {
                return vec![segments];
            }
        }
    };
    // While the path is still inside this file's own modules, each further
    // segment may be a name `use` binds in that (inline) module, which
    // expands there (`self::svc::X`, `imports::svc::X`), or a declared inline
    // module to walk into. Module-level bindings only: a block-local `use`
    // never shadows what `self::`/`super::` reach.
    while let Some(scope) = base.strip_prefix(module).map(<[String]>::to_vec) {
        let reached = Place {
            scope: scope.clone(),
            blocks: Vec::new(),
        };
        if let Some(resolved) = through_bindings(&reached, segments.clone()) {
            return resolved;
        }
        match segments.first() {
            Some(next) if scan.declared.contains(&(scope, next.clone())) => {
                base.push(segments.remove(0));
            }
            _ => break,
        }
    }
    base.append(&mut segments);
    vec![base]
}

/// What a composition root re-exports, at the root or inside its inline
/// modules and whatever the `use` item's visibility (a private `use` in
/// `services/mod.rs` still makes `crate::services::<name>` resolve for every
/// service): names keyed by their path from the root (`[TaskService]` for
/// `pub use task_service::TaskService`, `[exports, TaskService]` for the
/// same inside `pub mod exports`), with the module paths each can come from,
/// and the glob re-exports, which hide what they forward and are refused.
#[derive(Debug, Default, PartialEq, Eq)]
struct Reexports {
    names: BTreeMap<Vec<String>, Vec<ModulePath>>,
    globs: Vec<(usize, String)>,
}

fn reexports(module: &[String], scan: &Scan) -> Reexports {
    let mut out = Reexports::default();
    for reference in scan
        .references
        .iter()
        .filter(|reference| reference.place.blocks.is_empty())
    {
        let place = Place {
            scope: reference.place.scope.clone(),
            blocks: Vec::new(),
        };
        match &reference.binding {
            Binding::Name(name) => {
                let mut key = place.scope.clone();
                key.push(name.clone());
                out.names.entry(key).or_default().extend(resolve(
                    module,
                    &place,
                    scan,
                    &reference.path,
                    0,
                ));
            }
            Binding::Glob => out.globs.push((reference.line, reference.path.clone())),
            Binding::Path => {}
        }
    }
    out
}

/// Files by module path, for the modules that compile in production.
type ModuleFiles = BTreeMap<ModulePath, String>;

/// The assigned files a full module path can name, with their contexts,
/// following the composition roots' re-exports (keeping the segments after
/// the re-exported name).
fn targets_of(
    segments: &[String],
    files: &ModuleFiles,
    reexports: &BTreeMap<String, Reexports>,
    depth: usize,
) -> Vec<(String, Context)> {
    for length in (1..=segments.len()).rev() {
        let Some(file) = files.get(&segments[..length]) else {
            continue;
        };
        let Some((_, context)) = context_of(file) else {
            return Vec::new();
        };
        if context == Composition && depth < 4 {
            // The longest re-exported name path after the root wins
            // (`exports::TaskService` before `exports`).
            for taken in (length + 1..=segments.len()).rev() {
                let Some(forwarded) = reexports
                    .get(file)
                    .and_then(|found| found.names.get(&segments[length..taken]))
                else {
                    continue;
                };
                let rest = &segments[taken..];
                let mut found = Vec::new();
                for resolved in forwarded {
                    let mut full = resolved.clone();
                    full.extend(rest.iter().cloned());
                    if full[..] != segments[..] {
                        found.extend(targets_of(&full, files, reexports, depth + 1));
                    }
                }
                if !found.is_empty() {
                    return found;
                }
            }
        }
        return vec![(file.clone(), context)];
    }
    Vec::new()
}

/// What one file's production code names outside its own context.
#[derive(Debug, Default, PartialEq, Eq)]
struct Classification {
    edges: Vec<BackwardEdge>,
    /// Glob imports of a composition root: `(line, path as written)`.
    composition_globs: Vec<(usize, String)>,
}

fn classify(
    file: &str,
    from: Context,
    module: &[String],
    scan: &Scan,
    files: &ModuleFiles,
    reexports: &BTreeMap<String, Reexports>,
) -> Classification {
    let mut out = Classification::default();
    for reference in &scan.references {
        for segments in resolve(module, &reference.place, scan, &reference.path, 0) {
            for (target, to) in targets_of(&segments, files, reexports, 0) {
                if to == Composition {
                    if reference.binding == Binding::Glob {
                        out.composition_globs
                            .push((reference.line, reference.path.clone()));
                    }
                    continue;
                }
                if to <= from {
                    continue;
                }
                let edge = BackwardEdge {
                    file: file.to_string(),
                    line: reference.line,
                    from,
                    target,
                    to,
                };
                if !out.edges.contains(&edge) {
                    out.edges.push(edge);
                }
            }
        }
    }
    out
}

#[test]
fn every_service_and_maintenance_file_is_assigned_and_declared() {
    let root = core_src();
    let mut files = Vec::new();
    source_scan::collect_rust_files(&root.join("services"), &mut files);
    source_scan::collect_rust_files(&root.join("maintenance"), &mut files);
    let unassigned: Vec<String> = files
        .iter()
        .map(|file| relative(file, &root))
        .filter(|file| context_of(file).is_none())
        .collect();
    assert!(
        unassigned.is_empty(),
        "put each new service file in a context directory (or assign it in CONTEXTS):\n{}",
        unassigned.join("\n")
    );
    let missing: Vec<&str> = CONTEXTS
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !root.join(key).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "CONTEXTS names files that no longer exist:\n{}",
        missing.join("\n")
    );
    let modules = module_map(&root);
    let undeclared: Vec<String> = files
        .iter()
        .map(|file| relative(file, &root))
        .filter(|file| !modules.contains_key(file))
        .collect();
    assert!(
        undeclared.is_empty(),
        "no module declaration reaches these files from lib.rs; declare or delete them:\n{}",
        undeclared.join("\n")
    );
}

#[test]
fn every_context_file_has_its_reviewed_home() {
    let root = core_src();
    let contexts: Vec<(&str, Context)> = CONTEXTS
        .iter()
        .filter(|(key, context)| key.starts_with("services/") && *context != Composition)
        .map(|(key, context)| (*key, *context))
        .collect();
    let mut misplaced = Vec::new();
    let mut seen = BTreeSet::new();
    for (directory, context) in &contexts {
        let mut files = Vec::new();
        source_scan::collect_rust_files(&root.join(directory), &mut files);
        for file in files {
            let inside = relative(&file, &root.join(directory));
            if inside == "mod.rs" {
                continue;
            }
            // A nested directory is reviewed as one unit.
            let key = inside.split('/').next().unwrap_or(&inside).to_string();
            seen.insert(key.clone());
            match HOMES.iter().find(|(home, _)| *home == key) {
                Some((_, home)) if home == context => {}
                Some((_, home)) => misplaced.push(format!(
                    "{directory}/{inside} sits in {context:?} but is reviewed into {home:?}"
                )),
                None => misplaced.push(format!(
                    "{directory}/{inside} has no entry in HOMES; review its context and add one"
                )),
            }
        }
    }
    assert!(
        misplaced.is_empty(),
        "context placement must match the reviewed inventory:\n{}",
        misplaced.join("\n")
    );
    let stale: Vec<&str> = HOMES
        .iter()
        .map(|(home, _)| *home)
        .filter(|home| !seen.contains(*home))
        .collect();
    assert!(
        stale.is_empty(),
        "HOMES names files that no longer exist; remove them:\n{}",
        stale.join("\n")
    );
}

#[test]
fn no_context_names_a_later_context() {
    let root = core_src();
    let roots = vec![
        root.join("services"),
        root.join("maintenance"),
        root.join("auth"),
        root.join("cloud"),
        root.join("git_runtime.rs"),
        root.join("grounding_projection.rs"),
    ];
    let mut files = Vec::new();
    for entry in &roots {
        if entry.is_dir() {
            source_scan::collect_rust_files(entry, &mut files);
        } else {
            files.push(entry.clone());
        }
    }
    files.sort();
    let modules = module_map(&root);
    let module_files: ModuleFiles = modules
        .iter()
        .filter(|(_, identity)| !identity.test_only)
        .map(|(file, identity)| (identity.path.clone(), file.clone()))
        .collect();
    let sources: Vec<(String, ModuleIdentity, Scan)> = files
        .iter()
        .map(|file| relative(file, &root))
        .filter(|file| context_of(file).is_some())
        .map(|file| {
            let identity = modules
                .get(&file)
                .unwrap_or_else(|| panic!("{file} is declared by no module reachable from lib.rs"))
                .clone();
            let source = std::fs::read_to_string(root.join(&file)).expect("read core source");
            let tree = source_scan::parse_rust(&source);
            let scan = scan(&source, &tree);
            (file, identity, scan)
        })
        .filter(|(_, identity, _)| !identity.test_only)
        .collect();
    let reexport_map: BTreeMap<String, Reexports> = sources
        .iter()
        .filter(|(file, _, _)| context_of(file).is_some_and(|(_, c)| c == Composition))
        .map(|(file, identity, scan)| (file.clone(), reexports(&identity.path, scan)))
        .collect();
    let glob_reexports: Vec<String> = reexport_map
        .iter()
        .flat_map(|(file, found)| {
            found
                .globs
                .iter()
                .map(move |(line, path)| format!("{file}:{line} re-exports {path}::*"))
        })
        .collect();
    assert!(
        glob_reexports.is_empty(),
        "a composition root re-exports a glob, which hides which context each \
         forwarded name reaches; re-export the names:\n{}",
        glob_reexports.join("\n")
    );

    let mut edges = BTreeSet::new();
    let mut composition_globs = Vec::new();
    for (file, identity, scan) in &sources {
        let Some((_, from)) = context_of(file) else {
            continue;
        };
        if from == Composition {
            continue;
        }
        let classified = classify(
            file,
            from,
            &identity.path,
            scan,
            &module_files,
            &reexport_map,
        );
        edges.extend(classified.edges);
        composition_globs.extend(
            classified
                .composition_globs
                .into_iter()
                .map(|(line, path)| format!("{file}:{line} glob-imports {path}")),
        );
    }
    assert!(
        composition_globs.is_empty(),
        "a glob import of a composition root hides which context it reaches; \
         import the names:\n{}",
        composition_globs.join("\n")
    );

    let allowed: BTreeSet<(&str, &str)> = ALLOWED_BACKWARD
        .iter()
        .map(|(file, target, _)| (*file, *target))
        .collect();
    let unexpected: Vec<String> = edges
        .iter()
        .filter(|edge| !allowed.contains(&(edge.file.as_str(), edge.target.as_str())))
        .map(|edge| {
            format!(
                "{}:{} [{:?}] names {} [{:?}]",
                edge.file, edge.line, edge.from, edge.target, edge.to
            )
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "a context names a later one; reach it through a trait the earlier \
         context owns, installed in services/mod.rs:\n{}",
        unexpected.join("\n")
    );
    let stale: Vec<String> = ALLOWED_BACKWARD
        .iter()
        .filter(|(file, target, _)| {
            !edges
                .iter()
                .any(|edge| edge.file == *file && edge.target == *target)
        })
        .map(|(file, target, _)| format!("{file} -> {target}"))
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWED_BACKWARD entries no longer occur; remove them:\n{}",
        stale.join("\n")
    );
}

fn segments(path: &[&str]) -> ModulePath {
    path.iter().map(|s| (*s).to_owned()).collect()
}

/// A synthetic crate layout for the resolution tests, in the moved shape.
fn fixture_files() -> ModuleFiles {
    [
        (vec!["services"], "services/mod.rs"),
        (vec!["services", "agents"], "services/agents/mod.rs"),
        (vec!["services", "git"], "services/git/mod.rs"),
        (vec!["services", "identity"], "services/identity/mod.rs"),
        (
            vec!["services", "agents", "task_service"],
            "services/agents/task_service.rs",
        ),
        (
            vec!["services", "agents", "chat_service"],
            "services/agents/chat_service.rs",
        ),
        (
            vec!["services", "agents", "agent_tasks"],
            "services/agents/agent_tasks.rs",
        ),
        (
            vec!["services", "agents", "session_file_sha256"],
            "services/agents/session_file_sha256.rs",
        ),
        (
            vec!["services", "git", "branch_commit"],
            "services/git/branch_commit.rs",
        ),
        (
            vec!["services", "identity", "org_service"],
            "services/identity/org_service.rs",
        ),
        (
            vec!["services", "git", "remote_integrations"],
            "services/git/remote_integrations.rs",
        ),
        (
            vec!["services", "git", "remote_integrations", "worker"],
            "services/git/remote_integrations/worker.rs",
        ),
        (
            vec!["services", "git", "remote_integrations", "worker", "merge"],
            "services/git/remote_integrations/merge.rs",
        ),
        (vec!["maintenance"], "maintenance/mod.rs"),
        (vec!["cloud"], "cloud/mod.rs"),
        (vec!["cloud", "gcp"], "cloud/gcp.rs"),
    ]
    .into_iter()
    .map(|(path, file)| (segments(&path), file.to_string()))
    .collect()
}

/// The composition roots' re-exports in the moved shape: every service keeps
/// its `crate::services::<name>` path and every job its
/// `crate::maintenance::<job>` path.
fn fixture_reexports() -> BTreeMap<String, Reexports> {
    let forward = |names: &[(&str, &[&str])]| Reexports {
        names: names
            .iter()
            .map(|(name, path)| (segments(&[name]), vec![segments(path)]))
            .collect(),
        globs: Vec::new(),
    };
    BTreeMap::from([
        (
            "services/mod.rs".to_string(),
            forward(&[
                (
                    "TaskService",
                    &["services", "agents", "task_service", "TaskService"],
                ),
                ("jobs", &["maintenance"]),
                ("task_service", &["services", "agents", "task_service"]),
                ("chat_service", &["services", "agents", "chat_service"]),
                (
                    "session_file_sha256",
                    &["services", "agents", "session_file_sha256"],
                ),
                ("org_service", &["services", "identity", "org_service"]),
                ("branch_commit", &["services", "git", "branch_commit"]),
                (
                    "remote_integrations",
                    &["services", "git", "remote_integrations"],
                ),
            ]),
        ),
        (
            "maintenance/mod.rs".to_string(),
            forward(&[("agent_tasks", &["services", "agents", "agent_tasks"])]),
        ),
    ])
}

/// Classify `source` as `services/git/branch_commit.rs` (Git) in the fixture.
fn classify_git(source: &str) -> Classification {
    let tree = source_scan::parse_rust(source);
    let scan = scan(source, &tree);
    classify(
        "services/git/branch_commit.rs",
        Git,
        &segments(&["services", "git", "branch_commit"]),
        &scan,
        &fixture_files(),
        &fixture_reexports(),
    )
}

fn owned(found: &[(&str, Context)]) -> Vec<(String, Context)> {
    found
        .iter()
        .map(|(file, context)| ((*file).to_string(), *context))
        .collect()
}

fn edge_lines(classified: &Classification) -> Vec<(usize, &str)> {
    classified
        .edges
        .iter()
        .map(|edge| (edge.line, edge.target.as_str()))
        .collect()
}

#[test]
fn use_trees_bindings_and_relative_paths_resolve() {
    let source = "mod merge;\n\
                  use crate::services::foo::{Bar, baz::Qux as Q, self};\n\
                  use super::{a, b::*};\n\
                  use crate::services as svc;\n\
                  use svc::task_service::TaskService;\n\
                  use merge::Thing;\n\
                  fn f() { super::super::branch_commit::x(); Q::y(); }\n";
    let tree = source_scan::parse_rust(source);
    let scan = scan(source, &tree);
    let found: Vec<&str> = scan
        .references
        .iter()
        .map(|reference| reference.path.as_str())
        .collect();
    assert_eq!(
        found,
        vec![
            "crate::services::foo::Bar",
            "crate::services::foo::baz::Qux",
            "crate::services::foo",
            "super::a",
            "super::b",
            "crate::services",
            "svc::task_service::TaskService",
            "merge::Thing",
            "super::super::branch_commit::x",
            "Q::y",
        ]
    );
    assert_eq!(scan.references[4].binding, Binding::Glob);
    assert!(scan.declared.contains(&(Vec::new(), "merge".to_string())));
    let module = segments(&["services", "git", "remote_integrations", "worker"]);
    let resolve_here = |path: &str| resolve(&module, &Place::default(), &scan, path, 0);
    assert_eq!(
        resolve_here("svc::task_service::TaskService"),
        vec![segments(&["services", "task_service", "TaskService"])]
    );
    assert_eq!(
        resolve_here("self::svc::chat_service::go"),
        vec![segments(&["services", "chat_service", "go"])]
    );
    assert_eq!(
        resolve_here("merge::Thing"),
        vec![segments(&[
            "services",
            "git",
            "remote_integrations",
            "worker",
            "merge",
            "Thing"
        ])]
    );
    assert_eq!(
        resolve_here("super::super::branch_commit::x"),
        vec![segments(&["services", "git", "branch_commit", "x"])]
    );
    assert_eq!(
        resolve_here("Q::y"),
        vec![segments(&["services", "foo", "baz", "Qux", "y"])]
    );
    assert_eq!(
        resolve_here("foo::Other"),
        vec![segments(&["services", "foo", "Other"])]
    );
    assert_eq!(
        resolve_here("tokio::spawn"),
        vec![segments(&["tokio", "spawn"])]
    );
    assert_eq!(
        resolve(
            &segments(&["maintenance"]),
            &Place::default(),
            &scan,
            "super::x",
            0
        ),
        vec![segments(&["x"])]
    );
    assert_eq!(
        context_of("services/git/remote_integrations/merge.rs"),
        Some(("services/git", Git))
    );
    assert_eq!(context_of("cloud/gcp.rs"), Some(("cloud", Hosts)));
    assert_eq!(context_of("services/new_thing.rs"), None);
}

#[test]
fn composition_root_reexports_resolve_relative_to_the_root() {
    let source = "mod agents;\n\
                  pub use agents::task_service::TaskService;\n\
                  pub use self::agents::chat_service::{ChatService as Chat};\n\
                  pub use crate::maintenance as jobs;\n\
                  pub use agents::chat_service::*;\n\
                  use agents::chat_service::ChatService as PrivateChat;\n\
                  use crate::db::maintenance::Thing;\n\
                  pub mod exports { pub use super::agents::r#task_service::TaskService as Task; pub use super::agents::chat_service::*; }\n\
                  pub struct AppState;\n";
    let tree = source_scan::parse_rust(source);
    let scan = scan(source, &tree);
    let found = reexports(&segments(&["services"]), &scan);
    assert_eq!(
        found.names,
        BTreeMap::from([
            (
                segments(&["TaskService"]),
                vec![segments(&[
                    "services",
                    "agents",
                    "task_service",
                    "TaskService"
                ])]
            ),
            (
                segments(&["Chat"]),
                vec![segments(&[
                    "services",
                    "agents",
                    "chat_service",
                    "ChatService"
                ])]
            ),
            (segments(&["jobs"]), vec![segments(&["maintenance"])]),
            (
                segments(&["exports", "Task"]),
                vec![segments(&[
                    "services",
                    "agents",
                    "task_service",
                    "TaskService"
                ])]
            ),
            (
                segments(&["PrivateChat"]),
                vec![segments(&[
                    "services",
                    "agents",
                    "chat_service",
                    "ChatService"
                ])]
            ),
            (
                segments(&["Thing"]),
                vec![segments(&["db", "maintenance", "Thing"])]
            ),
        ])
    );
    assert_eq!(
        found.globs,
        vec![
            (5, "agents::chat_service".to_string()),
            (8, "super::agents::chat_service".to_string())
        ]
    );
    let files = fixture_files();
    let mut reexport_map = fixture_reexports();
    reexport_map.insert("services/mod.rs".to_string(), found);
    let targets = |path: &[&str]| targets_of(&segments(path), &files, &reexport_map, 0);
    assert_eq!(
        targets(&["services", "TaskService"]),
        owned(&[("services/agents/task_service.rs", Agents)])
    );
    // A private `use` at the root is a compatibility path for everything
    // under `services`.
    assert_eq!(
        targets(&["services", "PrivateChat", "new"]),
        owned(&[("services/agents/chat_service.rs", Agents)])
    );
    // A re-export inside an inline module of the root forwards too.
    assert_eq!(
        targets(&["services", "exports", "Task", "new"]),
        owned(&[("services/agents/task_service.rs", Agents)])
    );
    assert_eq!(
        targets(&["services", "Chat", "new"]),
        owned(&[("services/agents/chat_service.rs", Agents)])
    );
    // The segments after a forwarded name are kept: a root alias to another
    // composition root still reaches the eventual module through that
    // root's own re-exports.
    assert_eq!(
        targets(&["services", "jobs", "agent_tasks", "run"]),
        owned(&[("services/agents/agent_tasks.rs", Agents)])
    );
    assert_eq!(
        targets(&["services", "jobs"]),
        owned(&[("maintenance/mod.rs", Composition)])
    );
    assert_eq!(
        targets(&["services", "AppState"]),
        owned(&[("services/mod.rs", Composition)])
    );
    assert_eq!(targets(&["db", "maintenance"]), Vec::new());
}

#[test]
fn aliases_globs_and_macro_paths_are_classified() {
    // A module alias, a `{self as ...}` alias, a `self::`/`super::` path
    // through an alias and a composition-root re-export each reach Agents.
    let aliased = classify_git(
        "use crate::services as svc;\n\
         use crate::services::{self as services};\n\
         use svc::task_service::TaskService;\n\
         fn f() { services::chat_service::go(); }\n\
         fn g() { let _ = crate::services::TaskService::new(); }\n\
         fn h() { self::svc::chat_service::go(); }\n\
         mod inner { pub fn i() { super::svc::task_service::run(); } }\n",
    );
    assert_eq!(
        edge_lines(&aliased),
        vec![
            (3, "services/agents/task_service.rs"),
            (4, "services/agents/chat_service.rs"),
            (5, "services/agents/task_service.rs"),
            (6, "services/agents/chat_service.rs"),
            (7, "services/agents/task_service.rs"),
        ]
    );
    assert!(aliased.composition_globs.is_empty());

    // Function-local imports bind their name only in their block, so one
    // function's `svc` cannot hide another's, in either order; conditional
    // items may bind one name to several paths and each alternative counts.
    for source in [
        "fn a() { use crate::services as svc; svc::task_service::run(); }\n\
         fn b() { use std as svc; svc::mem::drop(1); }\n",
        "fn b() { use std as svc; svc::mem::drop(1); }\n\
         fn a() { use crate::services as svc; svc::task_service::run(); }\n",
    ] {
        let local = classify_git(source);
        assert_eq!(local.edges.len(), 1, "{source}");
        assert_eq!(local.edges[0].target, "services/agents/task_service.rs");
    }
    let conditional = classify_git(
        "#[cfg(feature = \"x\")]\n\
         use crate::services as svc;\n\
         #[cfg(not(feature = \"x\"))]\n\
         use std as svc;\n\
         fn f() { svc::task_service::run(); }\n",
    );
    assert_eq!(
        edge_lines(&conditional),
        vec![(5, "services/agents/task_service.rs")]
    );
    // A chained alias resolves where it was bound, so a block-local name
    // that shadows one link of the chain does not redirect it.
    let chained = classify_git(
        "use crate::services as svc;\n\
         use svc as api;\n\
         fn f() { use std as svc; let _: Option<api::TaskService> = None; svc::mem::drop(1); }\n",
    );
    assert_eq!(
        edge_lines(&chained),
        vec![(3, "services/agents/task_service.rs")]
    );
    // The private compatibility path of a crate-private module, direct and
    // through an alias, reaches the module it forwards to.
    let private_path = classify_git(
        "use crate::services::session_file_sha256 as sha;\n\
         fn f() { crate::services::session_file_sha256::ResumableSha256::new(); sha::ResumableSha256::new(); }\n",
    );
    assert_eq!(
        edge_lines(&private_path),
        vec![
            (1, "services/agents/session_file_sha256.rs"),
            (2, "services/agents/session_file_sha256.rs")
        ]
    );
    // `self::` names the module's own import, not a block-local shadow.
    let shadowed = classify_git(
        "use crate::services as svc;\n\
         fn f() { use std as svc; let _: Option<self::svc::task_service::TaskService> = None; }\n",
    );
    assert_eq!(
        edge_lines(&shadowed),
        vec![(2, "services/agents/task_service.rs")]
    );
    // A name bound inside an inline module expands when a path walks
    // through that module.
    let through_inline = classify_git(
        "mod imports { pub use crate::services as svc; }\n\
         fn f(_: imports::svc::TaskService) {}\n",
    );
    assert_eq!(
        edge_lines(&through_inline),
        vec![(2, "services/agents/task_service.rs")]
    );
    // `$crate` in a macro definition is the crate root.
    let dollar_crate = classify_git(
        "macro_rules! later_type { () => { $crate::services::TaskService }; }\n\
         fn f(_: later_type!()) {}\n",
    );
    assert_eq!(
        edge_lines(&dollar_crate),
        vec![(1, "services/agents/task_service.rs")]
    );
    // Raw identifiers name the same modules.
    let raw = classify_git(
        "use crate::services::r#task_service::TaskService;\n\
         use crate::r#services as svc;\n\
         fn f() { svc::r#chat_service::go(); tracing::info!(\"{}\", crate::services::r#chat_service::NAME); }\n",
    );
    assert_eq!(
        edge_lines(&raw),
        vec![
            (1, "services/agents/task_service.rs"),
            (3, "services/agents/chat_service.rs")
        ]
    );

    // Paths inside generic arguments and qualified-trait forms are scanned
    // on their own; the outer path keeps only its real segments.
    let nested = classify_git(
        "fn f() { Vec::<crate::services::task_service::TaskService>::new(); }\n\
         fn g(x: &crate::services::TaskService) { <crate::services::TaskService as Clone>::clone(x); }\n\
         fn h() -> Option<crate::services::agents::chat_service::ChatService> { None }\n",
    );
    assert_eq!(
        edge_lines(&nested),
        vec![
            (1, "services/agents/task_service.rs"),
            (2, "services/agents/task_service.rs"),
            (3, "services/agents/chat_service.rs"),
        ]
    );

    // Paths inside macro arguments resolve through the same bindings.
    let in_macro = classify_git(
        "use crate::services as svc;\n\
         fn f() { tracing::info!(target: \"x\", \"{}\", svc::chat_service::NAME); }\n",
    );
    assert_eq!(
        edge_lines(&in_macro),
        vec![(2, "services/agents/chat_service.rs")]
    );

    // An inline module resolves `super::` against its enclosing module and
    // sees the bindings around it.
    let inline = classify_git(
        "use crate::services as svc;\n\
         mod inner { pub fn f() { super::super::super::agents::chat_service::go(); } }\n\
         mod other { pub fn g() { svc::task_service::run(); } }\n",
    );
    assert_eq!(
        edge_lines(&inline),
        vec![
            (2, "services/agents/chat_service.rs"),
            (3, "services/agents/task_service.rs")
        ]
    );

    // A glob of a composition root is refused; a glob of a same-context or
    // earlier module is fine, and a glob of a later module is an edge.
    let globbed = classify_git(
        "use crate::services::*;\n\
         use super::remote_integrations::*;\n\
         use crate::cloud::gcp::*;\n\
         use crate::services::chat_service::*;\n",
    );
    assert_eq!(
        globbed.composition_globs,
        vec![(1, "crate::services".to_string())]
    );
    assert_eq!(
        edge_lines(&globbed),
        vec![(3, "cloud/gcp.rs"), (4, "services/agents/chat_service.rs")]
    );

    // Same-context and earlier-context references, test-only code, external
    // crates and plain generic calls produce nothing.
    let clean = classify_git(
        "use super::remote_integrations::worker::Thing;\n\
         use crate::services::org_service::OrgService;\n\
         use tokio::sync::Mutex;\n\
         fn f() { Vec::<u8>::new(); <String as Clone>::clone(&String::new()); }\n\
         #[cfg(test)]\n\
         mod tests { use crate::services::chat_service::ChatService; }\n",
    );
    assert_eq!(clean, Classification::default());
}

#[test]
fn module_identity_follows_declarations_not_file_names() {
    let root = core_src();
    let modules = module_map(&root);
    let identity = |file: &str| {
        modules
            .get(file)
            .unwrap_or_else(|| panic!("{file} is declared"))
            .clone()
    };
    assert_eq!(
        identity("services/git/workspace_pull_request_checks.rs").path,
        segments(&[
            "services",
            "git",
            "workspace_pull_request_provider_lifecycle",
            "read",
            "checks"
        ])
    );
    assert_eq!(
        identity("maintenance/mod.rs").path,
        segments(&["maintenance"])
    );
    assert_eq!(
        identity("services/git/remote_integrations/worker.rs").path,
        segments(&["services", "git", "remote_integrations", "worker"])
    );
    assert!(!identity("services/agents/task_service.rs").test_only);
    assert_eq!(
        identity("services/agents/agent_tasks.rs").path,
        segments(&["services", "agents", "agent_tasks"])
    );

    // A `super` chain from the path-attributed file resolves in Rust's
    // module tree, so a reach into Agents from there is classified.
    let source =
        "fn f() { super::super::super::super::agents::chat_service::ChatService::new(); }\n";
    let tree = source_scan::parse_rust(source);
    let scan = scan(source, &tree);
    let checks = identity("services/git/workspace_pull_request_checks.rs").path;
    assert_eq!(
        resolve(
            &checks,
            &Place::default(),
            &scan,
            "super::super::super::super::agents::chat_service::ChatService::new",
            0
        ),
        vec![segments(&[
            "services",
            "agents",
            "chat_service",
            "ChatService",
            "new"
        ])]
    );
    let module_files: ModuleFiles = modules
        .iter()
        .filter(|(_, identity)| !identity.test_only)
        .map(|(file, identity)| (identity.path.clone(), file.clone()))
        .collect();
    let classified = classify(
        "services/git/workspace_pull_request_checks.rs",
        Git,
        &checks,
        &scan,
        &module_files,
        &BTreeMap::new(),
    );
    assert_eq!(
        edge_lines(&classified),
        vec![(1, "services/agents/chat_service.rs")]
    );
}
