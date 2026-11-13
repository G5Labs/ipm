//! Structural guards for migrated Git callers and the authorized API reader.

use std::{
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

const RAW_READER_OPERATIONS: &[&str] = &[
    "show_file",
    "list_tree_recursive",
    "list_tree_paths",
    "diff_unified_between_commits",
    "list_commits",
    "list_first_parent_commits",
    "commit_diff_summary",
    "commit_file_diff",
    "blame_for_path",
    // Pinned-OID reads that services perform under their own authorization.
    "file_diffs_between_commits",
    "diff_name_only_between_commits",
    "commit_messages_ahead",
    "file_mode",
];

fn generation_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api crate must be beneath generation")
        .to_path_buf()
}

fn backend_crates() -> Vec<String> {
    let mut crates: Vec<_> = fs::read_dir(generation_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    crates.sort();
    assert!(crates.iter().any(|name| name == "g5-contracts"));
    assert!(crates.iter().any(|name| name == "g5-scm"));
    crates
}

fn api_core_source(relative: &str) -> PathBuf {
    generation_dir()
        .join("g5-api-core")
        .join("src")
        .join(relative)
}

const MIGRATED_GIT_OWNERS: &[(&str, &[&str])] = &[
    (
        "services/git/repo_gc.rs",
        &["run_repo_gc", "do_repo_gc_sweep"],
    ),
    ("services/git/git_conflict_resolution.rs", &["pin_inputs"]),
    (
        "services/git/remote_integrations/resolution.rs",
        &["enqueue_resolution"],
    ),
    (
        "services/git/source_control_change_graph_service.rs",
        &["semantic_record_diff_from_text_with_budget"],
    ),
    (
        "services/git/workspace_reader.rs",
        &["committed_at", "expected_unborn_workspace_authority_cuts"],
    ),
    (
        "services/git/workspace_repositories.rs",
        &["read_repository_versions"],
    ),
    (
        "services/workspace/workspace_service.rs",
        &["initialize_local_workspace_repo"],
    ),
    (
        "services/workspace/workspace_fork_service.rs",
        &["check_workspace_name"],
    ),
    (
        "services/workspace/workspace_review_service.rs",
        &["store_review_artifact", "generate_review_changes"],
    ),
    (
        "services/git/remote_integrations/observations.rs",
        &["observed_tip", "observe_tip"],
    ),
];
const SCM_MAINTENANCE_OPERATIONS: &[&str] = &["collect_garbage"];

fn native_process_path(path: &str) -> bool {
    let path = path.trim_start_matches("::");
    ["std::process", "tokio::process"].iter().any(|owner| {
        path == format!("{owner}::*")
            || path == format!("{owner}::Command")
            || path.starts_with(&format!("{owner}::Command::"))
    })
}

fn imported_paths(
    node: tree_sitter::Node<'_>,
    prefix: &str,
    source: &str,
    out: &mut Vec<(String, String)>,
) {
    let text = |node: tree_sitter::Node<'_>| {
        source[node.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let join = |path: String| {
        if prefix.is_empty() {
            path
        } else {
            format!("{prefix}::{path}")
        }
    };
    match node.kind() {
        "use_declaration" => {
            if let Some(argument) = node.child_by_field_name("argument") {
                imported_paths(argument, prefix, source, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                imported_paths(item, prefix, source, out);
            }
        }
        "scoped_use_list" => {
            let prefix = node
                .child_by_field_name("path")
                .map(|node| join(text(node)))
                .unwrap_or_else(|| prefix.to_owned());
            if let Some(list) = node.child_by_field_name("list") {
                imported_paths(list, &prefix, source, out);
            }
        }
        "use_as_clause" => {
            if let (Some(path), Some(alias)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("alias"),
            ) {
                out.push((join(text(path)), text(alias)));
            }
        }
        "use_wildcard" => out.push((join(text(node)), "*".into())),
        "identifier" | "scoped_identifier" | "self" => {
            let path = if text(node) == "self" && !prefix.is_empty() {
                prefix.to_owned()
            } else {
                join(text(node))
            };
            let alias = path.rsplit("::").next().unwrap().to_owned();
            out.push((path, alias));
        }
        _ => {}
    }
}

// Whether a cfg expression can be false/true when `test` is false. Other
// predicates are unknown; only a guaranteed false expression excludes code.
fn production_cfg_values(predicate: &str) -> (bool, bool) {
    if predicate == "test" {
        return (true, false);
    }
    for operator in ["all", "any", "not"] {
        if let Some(inner) = predicate
            .strip_prefix(&format!("{operator}("))
            .and_then(|value| value.strip_suffix(')'))
        {
            let mut depth = 0usize;
            let mut start = 0;
            let mut parts = Vec::new();
            let mut quoted = false;
            let mut escaped = false;
            for (index, character) in inner.char_indices() {
                if quoted {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        quoted = false;
                    }
                    continue;
                }
                if character == '"' {
                    quoted = true;
                    continue;
                }
                match character {
                    '(' => depth += 1,
                    ')' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        parts.push(&inner[start..index]);
                        start = index + 1;
                    }
                    _ => {}
                }
            }
            if start < inner.len() {
                parts.push(&inner[start..]);
            }
            let values: Vec<_> = parts.into_iter().map(production_cfg_values).collect();
            return match operator {
                "all" => (values.iter().any(|v| v.0), values.iter().all(|v| v.1)),
                "any" => (values.iter().all(|v| v.0), values.iter().any(|v| v.1)),
                "not" if values.len() == 1 => (values[0].1, values[0].0),
                _ => (true, true),
            };
        }
    }
    (true, true)
}

fn test_only_attribute(attribute: &str) -> bool {
    attribute
        .strip_prefix("#[cfg(")
        .and_then(|value| value.strip_suffix(")]"))
        .is_some_and(|predicate| !production_cfg_values(predicate).1)
        || attribute == "#[test]"
        || attribute.starts_with("#[tokio::test(")
        || attribute == "#[tokio::test]"
}

fn resolve_alias_path(path: &str, aliases: &std::collections::HashMap<String, String>) -> String {
    let mut resolved = path.trim_start_matches("::").to_owned();
    let mut seen = std::collections::HashSet::new();
    loop {
        let (root, rest) = resolved.split_once("::").unwrap_or((&resolved, ""));
        // Self/module re-exports can grow the suffix on every expansion. Track
        // roots, not whole strings, so syntax guards remain bounded.
        if !seen.insert(root.to_owned()) {
            break;
        }
        let Some(alias) = aliases.get(root) else {
            break;
        };
        resolved = if rest.is_empty() {
            alias.clone()
        } else {
            format!("{alias}::{rest}")
        };
    }
    resolved
}

fn rust_string_literal(expression: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<String>(expression) {
        return Some(value);
    }
    let raw = expression.strip_prefix('r')?;
    let quote = raw.find('"')?;
    let hashes = &raw[..quote];
    if !hashes.chars().all(|character| character == '#') {
        return None;
    }
    raw[quote + 1..]
        .strip_suffix(&format!("\"{hashes}"))
        .map(str::to_owned)
}

// Mixed-tool CLI owners may construct only these actual non-Git tools. A
// configured program is not an exception: Git construction belongs to SCM.
fn permitted_cli_tool(owner: &str, enclosing: &str, expression: &str) -> bool {
    // Existing generic execution supervision is not a Git adapter. Keep the
    // exception to its two actual constructors, never to the surrounding crate.
    if owner == "g5d-foundation/process_termination/execution_owner.rs" {
        let expression: String = expression.chars().filter(|c| !c.is_whitespace()).collect();
        if (enclosing == "command" && expression == "std::env::current_exe()?")
            || (enclosing == "run" && expression == "program")
        {
            return true;
        }
    }
    if (owner == "g5d-dispatch/git.rs"
        && enclosing == "provider_command"
        && expression == "program")
        || (owner == "g5d-dispatch/modules/runner.rs"
            && enclosing == "run_process"
            && expression == "&program")
    {
        return true; // Existing admitted non-Git module/provider execution.
    }
    // Process-mode custody (g5.runtime.boot.backend.process-local): the census
    // lists processes through `ps` where there is no procfs, and the launcher
    // starts the host's own dispatcher. Neither touches a repository.
    if (owner == "g5d-foundation/process_termination.rs"
        && matches!(enclosing, "incarnation_census" | "session_process_groups")
        && expression == "\"ps\"")
        || (owner == "g5d-host/runtime.rs" && enclosing == "command" && expression == "program")
    {
        return true;
    }
    // These existing owners launch selected agent tools, VM/network helpers, or
    // user commands. Limit exceptions to each observed constructor expression.
    if matches!(
        (owner, enclosing, expression),
        ("g5d/dispatch_test_support.rs", "launch", "binary")
            | (
                "g5d-agent/runtime.rs",
                "run" | "generate_session_title",
                "&program"
            )
            | ("g5d-agent/codex_auth.rs", "command", "program")
            | ("g5d-isolation/isolation/passt.rs", "spawn_passt_at", "bin")
            | (
                "g5d-isolation/isolation/cloud_hypervisor.rs",
                "run" | "probe",
                "vmm_binary()"
            )
            | (
                "g5d-isolation/isolation/microvm.rs",
                "build_vm_exec_command_with_env" | "prepare_private_workload",
                "Self::vm_exec_binary()"
            )
            | (
                "g5d-isolation/isolation/mod.rs",
                "platform_command",
                "binary"
            )
            | (
                "g5d-isolation/isolation/microvm.rs",
                "prepare_workload_templates_in_scope" | "prepare_workload_image",
                "std::env::current_exe()?"
            )
            | ("g5d-isolation/isolation/microvm.rs", "new", "\"/bin/true\"")
            | (
                "g5d/cli/vm_init.rs",
                "busybox",
                "\"/opt/g5/bootstrap/busybox\""
            )
            | (
                "g5d/cli/vm_init.rs",
                "run",
                "&config.command[0]"
                    | "\"/opt/g5/bootstrap/loader\""
                    | "\"/usr/local/bin/g5-guest-boot\""
            )
            | (
                "g5d-isolation/isolation/process.rs",
                "start" | "run",
                "&command_path"
            )
    ) {
        return true;
    }
    let Some(program) = rust_string_literal(expression) else {
        return false;
    };
    if program == "git"
        && ((owner == "g5d-host/test_support.rs" && enclosing == "git")
            || (owner == "g5d/dispatch_test_support.rs"
                && matches!(enclosing, "git" | "serve_git_connection")))
    {
        return true; // Existing test/test-helpers-only fixture constructor.
    }
    if owner == "g5d-isolation/isolation/process.rs" && enclosing == "kill" && program == "taskkill"
    {
        return true;
    }
    if owner == "g5d-isolation/isolation/guest_egress.rs"
        && enclosing == "install"
        && program == "/usr/sbin/nft"
    {
        return true; // Fixed host-owned firewall installer; no repository access.
    }
    match owner {
        // Block preparation is pre-guest filesystem work. The constructor
        // enumerates fixed utilities; dynamic programs and Git remain refused.
        "g5d-isolation/isolation/workload_root.rs" if enclosing == "helper" => [
            "cp",
            "tar",
            "mkfs.ext4",
            "/usr/sbin/mkfs.ext4",
            "/sbin/mkfs.ext4",
            "e2fsck",
            "find",
            "rsync",
            "bash",
        ]
        .contains(&program.as_str()),
        "g5d-foundation/workspace/restore.rs" | "g5d-foundation/oci/template.rs" => program == "cp",
        "g5d-foundation/oci/acr.rs" => program == "az",
        "g5d-foundation/oci/gar.rs" => program == "gcloud",
        "g5d-agent/port_scan.rs" => ["lsof", "ss"].contains(&program.as_str()),
        "g5d/cli/doctor.rs" => [
            "stat",
            "sh",
            "systemctl",
            "cp",
            "rustc",
            "node",
            "python3",
            "go",
            "java",
        ]
        .contains(&program.as_str()),
        "g5d/cli/preview_serve.rs"
        | "g5d/cli/bootstrap_supervisor.rs"
        | "g5d/cli/dispatch_fixture_module.rs" => program == "/bin/sh",
        _ => false,
    }
}

/// SCM backend mechanics may name libgit2 and native execution; the
/// `g5_scm::workspace` policy module may not, so it is excluded here.
fn scm_mechanics_scope(relative: &str) -> bool {
    relative.starts_with("g5-scm/") && !relative.starts_with("g5-scm/workspace/")
}

/// Check Rust syntax rather than spelling in comments or whitespace. These
/// migrated owners have no backend exception; other owners remain migration
/// work, not blanket exemptions for new operations in these files.
fn typed_git_violations(relative: &str, source: &str, required: &[&str]) -> Vec<String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).expect("parse Rust source");
    let mut violations = Vec::new();
    if tree.root_node().has_error() {
        let mut nodes = vec![tree.root_node()];
        while let Some(node) = nodes.pop() {
            if (node.is_error() || node.is_missing())
                && !(node
                    .parent()
                    .is_some_and(|parent| parent.kind() == "struct_pattern")
                    && source[node.byte_range()].trim() == r#"#[cfg(feature = "test-support")]"#)
            {
                violations.push(format!(
                    "{relative}: invalid Rust source at {}: {}",
                    node.start_position().row + 1,
                    &source[node.byte_range()]
                ));
            }
            let mut cursor = node.walk();
            nodes.extend(node.named_children(&mut cursor));
        }
    }
    let mut functions = Vec::new();
    let mut imports = std::collections::HashMap::new();
    let mut qualified_paths = Vec::new();
    let mut process_calls = Vec::new();
    let runner = relative == "g5-scm/native/process.rs";
    // `g5_scm::workspace` composes SCM primitives as a policy owner, so it is
    // scanned like any consumer crate rather than as backend mechanics.
    let scm_mechanics = scm_mechanics_scope(relative);
    let mixed_tools = !scm_mechanics || runner;
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        let text = &source[node.byte_range()];
        if node.kind() == "function_item" {
            if let Some(name) = node.child_by_field_name("name") {
                functions.push(&source[name.byte_range()]);
            }
        }
        let native_owner = matches!(
            relative,
            "g5d/cli/workspace_manifest.rs" | "g5d/cli/native_git.rs"
        ) || (scm_mechanics && !runner)
            || matches!(
                relative,
                "services/git/repo_gc.rs"
                    | "g5-api-core/services/git/repo_gc.rs"
                    | "g5-api-routes/routes/workspace_flush.rs"
                    | "g5-api-routes/routes/git.rs"
                    | "g5d-foundation/workspace/repo_cache.rs"
                    | "g5d-host/workspace_vm.rs"
            );
        if !scm_mechanics
            && matches!(node.kind(), "identifier" | "type_identifier")
            && (matches!(text, "git2" | "libgit2_sys")
                || (text == "reqwest"
                    && matches!(
                        relative,
                        "g5-api-core/services/git/github_app.rs"
                            | "g5-api-core/services/git/github_issues.rs"
                            | "g5-api-core/services/identity/auth_service.rs"
                            | "g5-api-core/auth/scm.rs"
                            | "g5-api-core/auth/oauth.rs"
                            | "g5-api-core/services/identity/platform_issue_github.rs"
                            | "g5-api-core/services/git/workspace_pull_request_provider_lifecycle.rs"
                            | "g5-api-core/services/git/workspace_pull_request_read.rs"
                            | "g5-api-core/services/git/workspace_pull_request_checks.rs"
                    )))
            && node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "scoped_identifier"
                        | "scoped_type_identifier"
                        | "use_declaration"
                        | "scoped_use_list"
                        | "use_wildcard"
                        | "use_list"
                        | "use_as_clause"
                        | "extern_crate_declaration"
                )
            })
        {
            violations.push(format!("{relative}: migrated owner accesses a raw backend"));
        }
        if mixed_tools && node.kind() == "call_expression" {
            if let (Some(function), Some(argument)) = (
                node.child_by_field_name("function"),
                node.child_by_field_name("arguments")
                    .and_then(|args| args.named_child(0)),
            ) {
                let function: String = source[function.byte_range()]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let mut parent = node.parent();
                let mut enclosing = "";
                while let Some(ancestor) = parent {
                    if ancestor.kind() == "function_item" {
                        if let Some(name) = ancestor.child_by_field_name("name") {
                            enclosing = &source[name.byte_range()];
                        }
                        break;
                    }
                    parent = ancestor.parent();
                }
                process_calls.push((function, enclosing, &source[argument.byte_range()]));
            }
        }
        if mixed_tools && node.kind() == "type_item" {
            if let (Some(name), Some(value)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("type"),
            ) {
                imports.insert(
                    source[name.byte_range()].to_owned(),
                    source[value.byte_range()]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect(),
                );
            }
        }
        if (native_owner || mixed_tools) && node.kind() == "use_declaration" {
            let mut paths = Vec::new();
            imported_paths(node, "", source, &mut paths);
            for (path, alias) in paths {
                if native_owner && native_process_path(&path) {
                    violations.push(format!(
                        "{relative}: native execution import bypasses the shared runner"
                    ));
                }
                imports.insert(alias, path);
            }
        }
        if native_owner && matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier") {
            let path: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            qualified_paths.push(path);
        }
        if matches!(node.kind(), "field_expression" | "scoped_identifier") {
            let member = node
                .child_by_field_name("field")
                .or_else(|| node.child_by_field_name("name"));
            if member.is_some_and(|member| {
                matches!(
                    &source[member.byte_range()],
                    "head_ref_commits" | "ref_commit_oid"
                )
            }) && !scm_mechanics
            {
                violations.push(format!(
                    "{relative}: retired projection scanner API is forbidden outside SCM mechanics"
                ));
            }
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                let attribute: String = source[child.byte_range()]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                test_only |= test_only_attribute(&attribute);
                continue;
            }
            if matches!(child.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    for (function, enclosing, argument) in process_calls {
        let (root, _rest) = function
            .trim_start_matches("::")
            .split_once("::")
            .unwrap_or((&function, ""));
        let resolved = resolve_alias_path(&function, &imports);
        let wildcard = root == "Command"
            && imports
                .get("*")
                .is_some_and(|path| native_process_path(path));
        if ((resolved.ends_with("::new") && (native_process_path(&resolved) || wildcard))
            || function.ends_with(".provider_command"))
            && !(runner && enclosing == "new")
            && !permitted_cli_tool(relative, enclosing, argument)
        {
            violations.push(format!(
                "{relative}::{enclosing}({argument}): unowned/configured process construction bypasses the Git boundary"
            ));
        }
    }
    for path in qualified_paths {
        let (root, rest) = path
            .trim_start_matches("::")
            .split_once("::")
            .unwrap_or((&path, ""));
        let resolved = imports
            .get(root)
            .map(|prefix| format!("{prefix}::{rest}"))
            .unwrap_or(path);
        if native_process_path(&resolved) {
            violations.push(format!(
                "{relative}: native execution path bypasses the shared runner"
            ));
        }
    }
    for name in required {
        if !functions.contains(name) {
            violations.push(format!("{relative}: missing guarded operation {name}"));
        }
    }
    violations
}

#[test]
fn migrated_git_operations_keep_their_typed_owner_boundaries() {
    let mut violations = Vec::new();
    for (relative, required) in MIGRATED_GIT_OWNERS {
        let source = fs::read_to_string(api_core_source(relative))
            .unwrap_or_else(|error| panic!("missing migrated Git owner {relative}: {error}"));
        violations.extend(typed_git_violations(relative, &source, required));
    }
    for crate_name in backend_crates() {
        let root = generation_dir().join(&crate_name).join("src");
        for entry in WalkDir::new(&root) {
            let entry = entry.expect("walk API source");
            let path = entry.path();
            if entry.file_type().is_file()
                && path.extension().is_some_and(|extension| extension == "rs")
            {
                let relative = path.strip_prefix(&root).unwrap().to_str().unwrap();
                // Only the core maintenance module is the owner, not a same-named route.
                let scope = format!("{crate_name}/{relative}");
                let source = fs::read_to_string(path).unwrap();
                let source = if crate_name == "g5-scm"
                    && matches!(
                        relative,
                        "native/tests.rs" | "scm_transport_tests.rs" | "comparison_fixture.rs"
                    ) {
                    let (parent, module) = if relative == "native/tests.rs" {
                        ("native/mod.rs", "tests")
                    } else if relative == "comparison_fixture.rs" {
                        ("lib.rs", "comparison_fixture")
                    } else {
                        ("lib.rs", "scm_transport_tests")
                    };
                    let parent_source = fs::read_to_string(root.join(parent)).unwrap();
                    let compact: String = parent_source
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    assert!(
                        compact.contains(&format!("#[cfg(test)]mod{module};"))
                            || compact.contains(&format!(
                                "#[cfg(all(test,feature=\"libgit2\"))]mod{module};"
                            ))
                            || (module == "comparison_fixture"
                                && fixture_module_is_gated(&parent_source, module)),
                        "fixture must remain test gated: {relative}"
                    );
                    format!("#[cfg(test)] mod fixture {{ {source} }}")
                } else {
                    source
                };
                violations.extend(typed_git_violations(&scope, &source, &[]));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn migrated_cli_native_operations_use_shared_runner() {
    for (file, required) in [
        ("add.rs", &["infer_remote_default_branch"][..]),
        ("doctor.rs", &["check_git"][..]),
        (
            "spec.rs",
            &[
                "source_path_is_git_ignored",
                "spec_file_is_new_in_worktree",
                "resolve_validation_roots",
            ][..],
        ),
        (
            "workspace_manifest.rs",
            &["materialize_checkout", "materialize_workspace_repo"][..],
        ),
    ] {
        let source = fs::read_to_string(generation_dir().join("g5d/src/cli").join(file))
            .unwrap_or_else(|error| panic!("missing migrated CLI Git owner {file}: {error}"));
        let violations = typed_git_violations(&format!("g5d/cli/{file}"), &source, required);
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }
}

fn backend_dependency_violations(metadata: &serde_json::Value, crate_name: &str) -> Vec<String> {
    let Some(package) = metadata["packages"].as_array().and_then(|packages| {
        packages
            .iter()
            .find(|package| package["name"] == crate_name)
    }) else {
        return vec![format!("missing {crate_name} package")];
    };
    let Some(dependencies) = package["dependencies"].as_array() else {
        return vec![format!("missing {crate_name} dependency inventory")];
    };
    dependencies
        .iter()
        .filter(|dependency| {
            matches!(dependency["name"].as_str(), Some("git2" | "libgit2-sys"))
                && dependency["kind"] != "dev"
        })
        .map(|dependency| {
            format!(
                "{crate_name} directly depends on backend {}",
                dependency["name"]
            )
        })
        .collect()
}

#[test]
fn migrated_crates_only_use_libgit2_in_test_fixtures() {
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(generation_dir().parent().unwrap())
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version=1",
                "--no-deps",
            ])
            .output()
            .expect("read Cargo metadata");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = serde_json::from_slice(&output.stdout).unwrap();
    for crate_name in backend_crates() {
        if crate_name == "g5-scm" {
            continue;
        }
        let violations = backend_dependency_violations(&metadata, &crate_name);
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }
}

#[test]
fn api_core_backend_guard_rejects_aliases_and_missing_packages_but_allows_fixtures() {
    use serde_json::json;
    for kind in [serde_json::Value::Null, json!("build")] {
        let metadata = json!({"packages":[{"name":"g5-api-core", "dependencies":[
            {"name":"git2", "rename":"backend", "kind":kind, "target":"cfg(unix)"}
        ]}]});
        assert!(!backend_dependency_violations(&metadata, "g5-api-core").is_empty());
    }
    assert!(!backend_dependency_violations(&json!({"packages":[]}), "g5-api-core").is_empty());
    assert!(backend_dependency_violations(
        &json!({"packages":[{"name":"g5-api-core", "dependencies":[
            {"name":"git2", "kind":"dev"}, {"name":"g5-scm", "kind":null}
        ]}]}),
        "g5-api-core"
    )
    .is_empty());
    assert!(!typed_git_violations(
        "g5-api-core/services/new.rs",
        "use git2 as backend; fn inspect() { backend::Repository::open(path); }",
        &[]
    )
    .is_empty());
    assert!(typed_git_violations("g5-api-core/services/new.rs", "fn inspect() { let git2_note = \"git2::Repository\"; } #[cfg(test)] mod fixtures { use git2 as backend; }", &[]).is_empty());
}

#[test]
fn workspace_module_backend_guard_covers_new_files_and_renamed_dependencies() {
    use serde_json::json;
    let metadata = json!({"packages":[{"name":"g5d-foundation", "dependencies":[
        {"name":"git2", "rename":"backend", "kind":null}
    ]}]});
    assert!(!backend_dependency_violations(&metadata, "g5d-foundation").is_empty());
    let backend_access = "use git2 as backend; fn inspect() { backend::Repository::open(path); }";
    // The SCM workspace module is a policy owner: its production code may not
    // reach the backend, while SCM mechanics beside it still may.
    assert!(!typed_git_violations("g5-scm/workspace/new.rs", backend_access, &[]).is_empty());
    assert!(typed_git_violations("g5-scm/new.rs", backend_access, &[]).is_empty());
    assert!(!typed_git_violations(
        "g5-scm/workspace/new.rs",
        "fn inspect() { std::process::Command::new(\"git\"); }",
        &[]
    )
    .is_empty());
    assert!(typed_git_violations("g5-scm/workspace/new.rs",
        "pub use crate::prepare_initial_files_commit; #[cfg(test)] mod fixtures { use git2::Repository; }", &[]).is_empty());
}

#[derive(Debug)]
struct SurfaceRule {
    source: String,
    scope: String,
    alias: Option<String>,
    types: Vec<String>,
    exported: bool,
    kind: String,
    implementation_trait: Option<String>,
}

fn public_item(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut cursor = node.walk();
    let public = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "visibility_modifier" && &source[child.byte_range()] == "pub");
    public
}

fn signature_types(node: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    if matches!(
        node.kind(),
        "scoped_type_identifier" | "scoped_identifier" | "type_identifier"
    ) {
        return vec![source[node.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()];
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "block")
        .flat_map(|child| signature_types(child, source))
        .collect()
}

#[derive(Default)]
struct SurfaceContext<'a> {
    scope: &'a str,
    public_trait: bool,
    public_enum: bool,
    implementation_trait: Option<String>,
}

fn collect_surface_rules(
    node: tree_sitter::Node<'_>,
    source: &str,
    file: &str,
    context: SurfaceContext<'_>,
    rules: &mut Vec<SurfaceRule>,
) {
    let SurfaceContext {
        scope,
        public_trait,
        public_enum,
        implementation_trait,
    } = context;
    let exported = public_item(node, source);
    let mut add = |alias: Option<String>, types: Vec<String>, exported| {
        rules.push(SurfaceRule {
            source: format!("{file}:{}", node.start_position().row + 1),
            scope: scope.into(),
            alias,
            types,
            exported,
            kind: node.kind().into(),
            implementation_trait: implementation_trait.clone(),
        });
    };
    if matches!(node.kind(), "struct_item" | "enum_item" | "trait_item") {
        let name = node.child_by_field_name("name").unwrap();
        add(Some(source[name.byte_range()].into()), Vec::new(), exported);
    }
    match node.kind() {
        "use_declaration" => {
            let mut imports = Vec::new();
            imported_paths(node, "", source, &mut imports);
            for (path, alias) in imports {
                add(Some(alias), vec![path], exported);
            }
            return;
        }
        "type_item" => {
            let name = node.child_by_field_name("name").unwrap();
            let ty = node.child_by_field_name("type").unwrap();
            add(
                Some(source[name.byte_range()].into()),
                signature_types(ty, source),
                exported || public_trait || implementation_trait.is_some(),
            );
            return;
        }
        "function_item" | "function_signature_item" => {
            if exported || public_trait {
                add(None, signature_types(node, source), true);
            }
            return; // Local implementation imports and types are not module aliases.
        }
        "field_declaration" => {
            if exported || public_enum {
                add(
                    None,
                    signature_types(node.child_by_field_name("type").unwrap(), source),
                    true,
                );
            }
            return;
        }
        "ordered_field_declaration_list" => {
            let mut cursor = node.walk();
            let mut public_field = public_enum;
            for child in node.named_children(&mut cursor) {
                if child.kind() == "visibility_modifier" {
                    public_field = &source[child.byte_range()] == "pub";
                } else if !matches!(
                    child.kind(),
                    "attribute_item" | "line_comment" | "block_comment"
                ) {
                    if public_field {
                        add(None, signature_types(child, source), true);
                    }
                    public_field = public_enum;
                }
            }
            return;
        }
        _ => {}
    }
    let next_scope = if node.kind() == "mod_item" {
        let name = &source[node.child_by_field_name("name").unwrap().byte_range()];
        if scope.is_empty() {
            name.into()
        } else {
            format!("{scope}::{name}")
        }
    } else {
        scope.into()
    };
    let public_trait = public_trait || (node.kind() == "trait_item" && exported);
    let public_enum = public_enum || (node.kind() == "enum_item" && exported);
    let implementation_trait = if node.kind() == "impl_item" {
        node.child_by_field_name("trait")
            .map(|name| source[name.byte_range()].to_string())
    } else {
        implementation_trait
    };
    let mut cursor = node.walk();
    let mut test_only = false;
    for child in node.named_children(&mut cursor) {
        if child.kind() == "attribute_item" {
            let attr: String = source[child.byte_range()]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            test_only |= attr == "#[cfg(test)]" || attr == "#[test]";
            continue;
        }
        if matches!(child.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if !test_only {
            collect_surface_rules(
                child,
                source,
                file,
                SurfaceContext {
                    scope: &next_scope,
                    public_trait,
                    public_enum,
                    implementation_trait: implementation_trait.clone(),
                },
                rules,
            );
        }
        test_only = false;
    }
}

fn backend_type(
    path: &str,
    scope: &str,
    aliases: &std::collections::HashSet<String>,
    definitions: &std::collections::HashSet<String>,
) -> bool {
    let path = path.trim_start_matches("::");
    if matches!(
        path.split("::").next(),
        Some("git2" | "libgit2_sys" | "reqwest")
    ) || ["std::process", "tokio::process"].iter().any(|root| {
        path == *root
            || path.strip_prefix(&format!("{root}::")).is_some_and(|name| {
                matches!(
                    name.split("::").next(),
                    Some(
                        "Command"
                            | "Child"
                            | "ChildStdin"
                            | "ChildStdout"
                            | "ChildStderr"
                            | "Stdio"
                    )
                )
            })
    }) {
        return true;
    }
    let has_alias = |candidate: &str| {
        aliases.iter().any(|alias| {
            candidate == alias
                || candidate
                    .strip_prefix(alias)
                    .is_some_and(|rest| rest.starts_with("::"))
        })
    };
    if let Some(path) = path.strip_prefix("crate::") {
        return has_alias(path);
    }
    let mut scope = scope;
    let mut path = path.strip_prefix("self::").unwrap_or(path);
    while let Some(rest) = path.strip_prefix("super::") {
        scope = scope.rsplit_once("::").map_or("", |(parent, _)| parent);
        path = rest;
    }
    loop {
        let candidate = if scope.is_empty() {
            path.into()
        } else {
            format!("{scope}::{path}")
        };
        if has_alias(&candidate) {
            return true;
        }
        if definitions.iter().any(|name| {
            candidate == *name
                || candidate
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with("::"))
        }) {
            return false;
        }
        let root = path.split("::").next().unwrap_or(path);
        if aliases.contains(&if scope.is_empty() {
            "*".into()
        } else {
            format!("{scope}::*")
        }) && !matches!(
            root,
            "std"
                | "core"
                | "alloc"
                | "Self"
                | "String"
                | "Vec"
                | "Box"
                | "Option"
                | "Result"
                | "Send"
                | "Sync"
                | "Sized"
                | "Copy"
                | "Clone"
                | "Default"
                | "Iterator"
                | "IntoIterator"
                | "From"
                | "Into"
                | "TryFrom"
                | "TryInto"
                | "AsRef"
                | "AsMut"
                | "PartialEq"
                | "Eq"
                | "PartialOrd"
                | "Ord"
                | "Drop"
        ) {
            return true;
        }
        if scope.is_empty() {
            return false;
        }
        scope = scope.rsplit_once("::").map_or("", |(parent, _)| parent);
    }
}

fn private_trait_in_scope(
    path: &str,
    scope: &str,
    private: &std::collections::HashSet<String>,
) -> bool {
    if let Some(path) = path.strip_prefix("crate::") {
        return private.contains(path);
    }
    let mut scope = scope;
    let mut path = path.strip_prefix("self::").unwrap_or(path);
    while let Some(rest) = path.strip_prefix("super::") {
        scope = scope.rsplit_once("::").map_or("", |(parent, _)| parent);
        path = rest;
    }
    loop {
        if private.contains(&if scope.is_empty() {
            path.into()
        } else {
            format!("{scope}::{path}")
        }) {
            return true;
        }
        if scope.is_empty() {
            return false;
        }
        scope = scope.rsplit_once("::").map_or("", |(parent, _)| parent);
    }
}

/// Inspect explicit signatures, aliases and exports across SCM source modules.
/// Private fields/helpers may use backend types; owned public wrappers are valid.
/// This is a structural ratchet, not a replacement for Rust name/type resolution.
fn scm_public_backend_violations(sources: &[(&str, &str)]) -> Vec<String> {
    let mut rules = Vec::new();
    let mut violations = Vec::new();
    for (file, source) in sources {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        if tree.root_node().has_error() {
            violations.push(format!("{file}: invalid Rust source"));
        }
        let scope = file
            .strip_suffix(".rs")
            .unwrap_or(file)
            .trim_end_matches("/mod")
            .replace('/', "::");
        let scope = if scope == "lib" { "" } else { &scope };
        collect_surface_rules(
            tree.root_node(),
            source,
            file,
            SurfaceContext {
                scope,
                ..SurfaceContext::default()
            },
            &mut rules,
        );
    }
    let module_aliases: Vec<_> = rules
        .iter()
        .filter(|r| r.kind == "use_declaration" && r.types.len() == 1)
        .filter_map(|r| {
            r.alias
                .as_ref()
                .map(|alias| (r.scope.clone(), alias.clone(), r.types[0].clone()))
        })
        .collect();
    for rule in &mut rules {
        let aliases: std::collections::HashMap<_, _> = module_aliases
            .iter()
            .filter(|(scope, _, _)| scope == &rule.scope)
            .map(|(_, alias, path)| (alias.clone(), path.clone()))
            .collect();
        for ty in &mut rule.types {
            *ty = resolve_alias_path(ty, &aliases);
        }
    }
    let definitions: std::collections::HashSet<String> = rules
        .iter()
        .filter_map(|rule| {
            rule.alias.as_ref().map(|name| {
                if rule.scope.is_empty() {
                    name.clone()
                } else {
                    format!("{}::{name}", rule.scope)
                }
            })
        })
        .collect();
    let mut aliases = std::collections::HashSet::new();
    loop {
        let before = aliases.len();
        for rule in &rules {
            if let Some(alias) = &rule.alias {
                if rule
                    .types
                    .iter()
                    .any(|ty| backend_type(ty, &rule.scope, &aliases, &definitions))
                {
                    aliases.insert(if rule.scope.is_empty() {
                        alias.clone()
                    } else {
                        format!("{}::{alias}", rule.scope)
                    });
                }
            }
        }
        if aliases.len() == before {
            break;
        }
    }
    let mut private_traits: std::collections::HashSet<_> = rules
        .iter()
        .filter(|rule| rule.kind == "trait_item" && !rule.exported)
        .filter_map(|rule| {
            rule.alias.as_ref().map(|name| {
                if rule.scope.is_empty() {
                    name.clone()
                } else {
                    format!("{}::{name}", rule.scope)
                }
            })
        })
        .collect();
    loop {
        let before = private_traits.len();
        for rule in &rules {
            if rule.kind == "use_declaration"
                && rule
                    .types
                    .iter()
                    .any(|ty| private_trait_in_scope(ty, &rule.scope, &private_traits))
            {
                if let Some(name) = &rule.alias {
                    private_traits.insert(if rule.scope.is_empty() {
                        name.clone()
                    } else {
                        format!("{}::{name}", rule.scope)
                    });
                }
            }
        }
        if private_traits.len() == before {
            break;
        }
    }
    for rule in rules {
        let private_implementation = rule
            .implementation_trait
            .as_ref()
            .is_some_and(|name| private_trait_in_scope(name, &rule.scope, &private_traits));
        if rule.exported
            && !private_implementation
            && rule
                .types
                .iter()
                .any(|ty| backend_type(ty, &rule.scope, &aliases, &definitions))
        {
            violations.push(format!(
                "{}: public SCM surface exposes a backend type",
                rule.source
            ));
        }
    }
    violations
}

#[test]
fn scm_public_operations_keep_backend_types_private() {
    let root = generation_dir().join("g5-scm/src");
    let mut sources = Vec::new();
    for entry in WalkDir::new(&root) {
        let entry = entry.expect("walk SCM source");
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            sources.push((
                entry
                    .path()
                    .strip_prefix(&root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                fs::read_to_string(entry.path()).unwrap(),
            ));
        }
    }
    for required in ["lib.rs", "repo.rs", "auth.rs", "commit_preparation.rs"] {
        assert!(
            sources.iter().any(|(file, _)| file == required),
            "missing SCM surface {required}"
        );
    }
    let borrowed: Vec<_> = sources
        .iter()
        .map(|(file, source)| (file.as_str(), source.as_str()))
        .collect();
    let violations = scm_public_backend_violations(&borrowed);
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn scm_surface_guard_rejects_new_names_aliases_exports_and_callbacks() {
    for source in [
        "impl Repo { pub fn newly_named(&self) -> &git2::Repository { todo!() } }",
        "use git2::Repository as Backend; type Raw = Backend; pub fn inspect() -> Raw { todo!() }",
        "use git2 as backend; pub fn callbacks(cb: backend::RemoteCallbacks<'_>) {}",
        "pub use git2::Repository as NewPublicName;",
        "pub use git2::*;",
        "use git2::*; pub fn newly_named() -> Repository { todo!() }",
        "pub struct Repo; impl std::ops::Deref for Repo { type Target = git2::Repository; fn deref(&self) -> &Self::Target { todo!() } }",
        "pub type PublicAlias = git2::Index;",
        "pub struct PublicField { pub raw: git2::Repository }",
        "pub struct PublicTuple(pub git2::Index);",
        "pub enum PublicVariant { Raw(git2::Repository) }",
        "pub trait PublicTrait { fn inspect(&self) -> &git2::Repository; }",
    ] {
        assert!(
            !scm_public_backend_violations(&[("repo.rs", source)]).is_empty(),
            "{source}"
        );
    }
    assert!(!scm_public_backend_violations(&[
        ("backend.rs", "pub(crate) type Raw = git2::Repository;"),
        ("lib.rs", "pub use crate::backend::Raw as Exposed;"),
    ])
    .is_empty());
    assert!(scm_public_backend_violations(&[(
        "lib.rs",
        r#"
        use git2::Repository as Backend;
        type Raw = Backend;
        pub struct Repo { inner: Raw }
        pub struct Owned(Raw);
        impl Repo { fn renamed_private(&self) -> &Raw { &self.inner }
                    pub fn operation(&self) -> String { self.inner.path().display().to_string() } }
        mod local { pub struct Raw; pub fn owned() -> Raw { Raw } }
        trait Private { type Item; } struct Hidden;
        impl Private for Hidden { type Item = git2::Repository; }
        use self::Private as RenamedPrivate;
        impl RenamedPrivate for Owned { type Item = git2::Index; }
        mod wildcard { use git2::*; pub struct Owned; fn private() -> Repository { todo!() } pub fn visible() -> Owned { Owned } pub fn value() -> String { String::new() } }
        #[cfg(test)] mod fixtures { pub use git2::Repository; }
    "#
    )])
    .is_empty());
}

/// libgit2 is limited to local object/ref/diff work; every operation that
/// contacts a remote runs through the native runner.
const LIBGIT2_NETWORK_API: &[&str] = &[
    "Remote",
    "RemoteCallbacks",
    "PushOptions",
    "FetchOptions",
    "ProxyOptions",
    "RepoBuilder",
    "Cred",
    "CredentialType",
    "remote_anonymous",
    "connect_auth",
    "create_detached",
];

fn libgit2_network_violations(relative: &str, source: &str) -> Vec<String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut violations = Vec::new();
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "field_identifier"
        ) && LIBGIT2_NETWORK_API.contains(&&source[node.byte_range()])
        {
            violations.push(format!(
                "{relative}: libgit2 network API {} outside the native runner",
                &source[node.byte_range()]
            ));
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                let attribute: String = source[child.byte_range()]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                test_only |= test_only_attribute(&attribute);
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    violations
}

/// Structural backstop for the name-based scan below: SCM's libgit2 is built
/// without HTTPS/SSH transports, so no libgit2 call can reach a provider.
#[test]
fn scm_libgit2_is_built_without_network_transports() {
    let manifest =
        fs::read_to_string(generation_dir().join("g5-scm/Cargo.toml")).expect("SCM manifest");
    let git2 = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("git2 "))
        .expect("SCM declares git2");
    assert!(git2.contains("default-features = false"), "{git2}");
    for transport in ["https", "ssh"] {
        assert!(!git2.contains(&format!("\"{transport}\"")), "{git2}");
    }
}

#[test]
fn libgit2_never_contacts_a_remote() {
    let root = generation_dir().join("g5-scm/src");
    let mut scanned = 0;
    let mut violations = Vec::new();
    for entry in WalkDir::new(&root) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
            let relative = entry
                .path()
                .strip_prefix(&root)
                .unwrap()
                .display()
                .to_string();
            if relative.ends_with("tests.rs") {
                continue;
            }
            scanned += 1;
            violations.extend(libgit2_network_violations(
                &relative,
                &fs::read_to_string(entry.path()).unwrap(),
            ));
        }
    }
    assert!(scanned > 20, "SCM sources were not scanned");
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    for source in [
        "fn fetch() { let mut callbacks = git2::RemoteCallbacks::new(); }",
        "use git2::build::RepoBuilder; fn clone() { RepoBuilder::new(); }",
        "fn probe(repo: &git2::Repository) { repo.remote_anonymous(url); }",
        "fn ls() { remote.connect_auth(direction, None, None); }",
    ] {
        assert!(
            !libgit2_network_violations("new.rs", source).is_empty(),
            "{source}"
        );
    }
    assert!(libgit2_network_violations(
        "repo.rs",
        "fn read(repo: &git2::Repository) { repo.find_remote(name); }\n#[cfg(test)] mod tests { fn t() { git2::RemoteCallbacks::new(); } }"
    )
    .is_empty());
}

#[test]
fn shared_repository_leases_use_the_native_runner() {
    for (file, required) in [
        (
            "repo.rs",
            &[
                "push_commit_native",
                "publish_commit",
                "push_create_ref",
                "push_existing_commit",
            ][..],
        ),
        ("native/mod.rs", &["push_branch", "delete_branch"][..]),
        (
            "native/remote.rs",
            &[
                "remote_branches",
                "remote_branch_tip",
                "fetch_branch_objects",
            ][..],
        ),
        ("native/maintenance.rs", SCM_MAINTENANCE_OPERATIONS),
    ] {
        let source = fs::read_to_string(generation_dir().join("g5-scm/src").join(file)).unwrap();
        let violations = typed_git_violations(&format!("g5-scm/{file}"), &source, required);
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }
    assert!(!typed_git_violations(
        "g5-scm/repo.rs",
        "fn push_commit_native() { std::process::Command::new(binary); }",
        &["push_with_lease"]
    )
    .is_empty());
    assert!(!typed_git_violations(
        "g5-scm/repo.rs",
        "#[cfg(test)] fn push_with_lease() {}",
        &["push_with_lease"]
    )
    .is_empty());
    assert!(typed_git_violations("g5-scm/repo.rs", "use git2::Oid; fn push_with_lease() { let process = Oid::zero(); native.delete_branch(request); }", &["push_with_lease"]).is_empty());
}

#[test]
fn api_gc_guard_rejects_process_construction_and_missing_owner_but_allows_fixtures() {
    let owner = "services/git/repo_gc.rs";
    for source in [
        "use tokio::process::Command as Child; fn collect() { Child::new(binary); }",
        "fn collect() { std::process::Command::new(configured_binary); }",
        "use std::process::*; fn collect() { Command::new(binary); }",
        "use std::process::{self, Stdio}; fn collect() { process::Command::new(binary); }",
        "#[cfg(test)] fn collect() {}",
    ] {
        assert!(
            !typed_git_violations(owner, source, &["collect"]).is_empty(),
            "{source}"
        );
    }
    for helper in ["old_helper", "renamed_helper"] {
        let source = format!("fn collect() {{ {helper}(); }} fn {helper}() {{ runner.collect_garbage(path, threshold); }} #[cfg(test)] mod fixtures {{ use std::process::Command; }}");
        assert!(typed_git_violations(owner, &source, &["collect"]).is_empty());
    }
    for source in [
        "fn collect() { let pid = std::process::id(); runner.collect_garbage(path, threshold); }",
        "use std::process::{self, Stdio}; fn collect() { let pid = process::id(); let io = Stdio::null(); runner.collect_garbage(path, threshold); }",
    ] {
        assert!(typed_git_violations(owner, source, &["collect"]).is_empty());
    }
    assert!(
        !typed_git_violations(owner, "use git2 as backend; fn collect() { }", &["collect"])
            .is_empty()
    );
}

#[test]
fn gc_guard_uses_public_targets_and_accepts_actual_private_helper_renames() {
    let api_owner = "services/git/repo_gc.rs";
    let required = MIGRATED_GIT_OWNERS
        .iter()
        .find(|(file, _)| *file == api_owner)
        .unwrap()
        .1;
    let source = fs::read_to_string(api_core_source(api_owner)).unwrap();
    let renamed = source.replace("gc_repo_if_needed", "collect_one_repository");
    assert!(typed_git_violations(api_owner, &renamed, required).is_empty());
    assert!(!typed_git_violations(
        api_owner,
        &source.replace("do_repo_gc_sweep", "removed_owner"),
        required
    )
    .is_empty());
    let scm_owner = "g5-scm/native/maintenance.rs";
    let source =
        fs::read_to_string(generation_dir().join("g5-scm/src/native/maintenance.rs")).unwrap();
    assert!(typed_git_violations(
        scm_owner,
        &source.replace("loose_object_count", "inspect_loose_objects"),
        SCM_MAINTENANCE_OPERATIONS
    )
    .is_empty());
    assert!(!typed_git_violations(
        scm_owner,
        &source.replace("collect_garbage", "removed_owner"),
        SCM_MAINTENANCE_OPERATIONS
    )
    .is_empty());
}

#[test]
fn cli_native_guard_rejects_aliases_and_headers_but_allows_fixtures_and_refactors() {
    let owner = "g5d/cli/workspace_manifest.rs";
    for source in [
        "use std::process::Command as Child; fn materialize_checkout() { Child::new(binary); }",
        "use tokio::{process as child}; fn materialize_checkout() { child::Command::new(binary); }",
        "fn materialize_checkout() { std::process::Command::new(binary); }",
        "use std as system; fn materialize_checkout() { system::process::Command::new(binary); }",
        "#[cfg(test)] mod tests { fn materialize_checkout() {} }",
    ] {
        assert!(
            !typed_git_violations(owner, source, &["materialize_checkout"]).is_empty(),
            "{source}"
        );
    }
    let source = "use crate::Command; fn materialize_checkout() { let process = 1; let header = \"http.extraHeader\"; let selected = Command::Clone; let runner = NativeGit::cli(timeout); runner.clone_checkout(request); } #[cfg(test)] mod tests { use std::process::Command; }";
    assert!(typed_git_violations(owner, source, &["materialize_checkout"]).is_empty());
}

#[test]
fn typed_git_guard_detects_bypasses_missing_targets_and_allows_internal_refactors() {
    let owner = "services/git/workspace_reader.rs";
    assert!(!typed_git_violations(
        owner,
        "use git2 as backend; fn read() { backend::Repository::open(path); }",
        &["read"]
    )
    .is_empty());
    assert!(!typed_git_violations(
        "services/other.rs",
        "fn read() { repo . head_ref_commits (); }",
        &[]
    )
    .is_empty());
    assert!(!typed_git_violations(
        "services/other.rs",
        "fn read() { GitRepo::ref_commit_oid(&repo, name); }",
        &[]
    )
    .is_empty());
    assert!(!typed_git_violations(owner, "// deleted read operation", &["read"]).is_empty());
    assert!(!typed_git_violations(
        "services/other.rs",
        "fn read() { let reader = GitRepo::head_ref_commits; reader(&repo); }",
        &[]
    )
    .is_empty());
    assert!(typed_git_violations(owner,
        "fn read() {} #[cfg(test)] mod tests { fn fixture() { git2::Repository::init_bare(path); } }", &["read"]).is_empty());
    assert!(typed_git_violations(
        "services/other.rs",
        "#[test] fn fixture() { repo.head_ref_commits(); }",
        &[]
    )
    .is_empty());
    assert!(
        !typed_git_violations(owner, "#[cfg(test)] mod tests { fn read() {} }", &["read"])
            .is_empty()
    );
    assert!(!typed_git_violations(owner, "#[test] fn read() {}", &["read"]).is_empty());
    assert!(!typed_git_violations(
        owner,
        "fn read() {} #[cfg(test)] mod tests {} fn bypass() { git2::Repository::open(path); }",
        &["read"]
    )
    .is_empty());
    for helper in ["old_helper", "renamed_helper"] {
        let source = format!("// git2 is private\nfn read() {{ {helper}(); }} fn {helper}() {{ repo.resolve_revision(target); }}");
        assert!(typed_git_violations(owner, &source, &["read"]).is_empty());
    }
}

/// This checks operation ownership, independent of method/UFCS spelling.
/// Source syntax cannot establish arbitrary receiver types; the small set of
/// same-named authorized-view operations is admitted by owning operation below.
fn content_reader_violations(relative: &str, source: &str) -> Vec<String> {
    git_owner_violations(relative, source, true)
}

/// Ref mutations are confined to named policy owners in every crate; content
/// reader and commit ownership is an API boundary only.
fn ref_mutation_violations(relative: &str, source: &str) -> Vec<String> {
    git_owner_violations(relative, source, false)
}

fn git_owner_violations(relative: &str, source: &str, api: bool) -> Vec<String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut violations = Vec::new();
    let mut nodes = vec![tree.root_node()];
    let mut aliases = std::collections::HashMap::new();
    let mut imports = vec![tree.root_node()];
    while let Some(node) = imports.pop() {
        if node.kind() == "use_declaration" {
            let mut paths = Vec::new();
            imported_paths(node, "", source, &mut paths);
            aliases.extend(paths.into_iter().map(|(path, alias)| (alias, path)));
        }
        let mut cursor = node.walk();
        imports.extend(node.named_children(&mut cursor));
    }
    while let Some(node) = nodes.pop() {
        if node.kind() == "call_expression" {
            if let Some(function) = node.child_by_field_name("function") {
                let expression: String = source[function.byte_range()]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let name = function
                    .child_by_field_name("field")
                    .or_else(|| function.child_by_field_name("name"))
                    .map(|name| &source[name.byte_range()])
                    .unwrap_or(expression.as_str());
                // SQL transaction commit has no arguments; Git commit accepts
                // message/author (and UFCS also the repository receiver).
                let git_commit = api
                    && name == "commit"
                    && node
                        .child_by_field_name("arguments")
                        .is_some_and(|args| args.named_child_count() > 0);
                let reader = api && RAW_READER_OPERATIONS.contains(&name);
                if reader || REF_MUTATOR_OPERATIONS.contains(&name) || git_commit {
                    let mut parent = node.parent();
                    let mut enclosing = "";
                    let mut public = false;
                    while let Some(ancestor) = parent {
                        if ancestor.kind() == "function_item" {
                            enclosing = ancestor
                                .child_by_field_name("name")
                                .map(|n| &source[n.byte_range()])
                                .unwrap_or("");
                            public = public_item(ancestor, source);
                            break;
                        }
                        parent = ancestor.parent();
                    }
                    let permitted = if REF_MUTATOR_OPERATIONS.contains(&name) || git_commit {
                        permitted_ref_mutator(relative, enclosing, name)
                    } else {
                        permitted_content_reader(relative, enclosing, public)
                    };
                    if !permitted {
                        violations.push(format!("{relative}::{enclosing}: Git operation {name} bypasses its policy owner"));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                let attribute: String = source[child.byte_range()]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                test_only |= test_only_attribute(&attribute);
                continue;
            }
            if matches!(child.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    violations
}

const REF_MUTATOR_OPERATIONS: &[&str] = &[
    "create_approval_evidence_ref_if_absent_or_equal",
    "configure_branch_alias",
    "apply_ref_commits",
    "apply_branch_commits",
    "apply_branch_commit",
    "create_branch",
    "checkout_branch_at",
    "create_workspace_fork_refs",
    "create_workspace_fork_copy_refs",
    "write_baseline_ref",
    "publish_commit",
    "push_existing_commit",
    "push_create_ref",
    "delete_workspace_refs",
    "delete_workspace_flush_refs",
    "delete_workspace_fork_refs_if_matches",
    "delete_workspace_fork_copy_refs_if_matches",
    // Native remote ref writes. The async `push_branch` shares its name with a
    // CLI helper; its blocking form and `delete_branch` identify the operation.
    "push_branch_blocking",
    "delete_branch",
    "delete_branch_blocking",
    "push_local_mirror_blocking",
    "create_branch_ref_if_absent",
];
fn permitted_ref_mutator(relative: &str, function: &str, operation: &str) -> bool {
    match relative {
        "g5-api-core/services/git/branch_commit.rs" => {
            operation == "apply_ref_commits"
                && matches!(
                    function,
                    "apply_member_ref_updates" | "restore_applied_branch_refs"
                )
        }
        "g5-api-core/services/workspace/workspace_fork_service.rs" => {
            (function == "pin_approval_evidence"
                && operation == "create_approval_evidence_ref_if_absent_or_equal")
                || (function == "pin_approval_base"
                    && matches!(
                        operation,
                        "create_workspace_fork_refs" | "delete_workspace_fork_refs_if_matches"
                    ))
                || (function == "create_member_refs"
                    && matches!(
                        operation,
                        "create_workspace_fork_refs" | "create_workspace_fork_copy_refs"
                    ))
                || (function == "cleanup_operation"
                    && matches!(
                        operation,
                        "delete_workspace_fork_refs_if_matches"
                            | "delete_workspace_fork_copy_refs_if_matches"
                    ))
        }
        "g5-api-core/services/git/workspace_flush_retention.rs" => {
            function == "delete_refs" && operation == "delete_workspace_flush_refs"
        }
        "g5-api-core/services/git/remote_integrations/merge.rs" => {
            function == "apply_external_remote_tracking_merge_completion"
                && operation == "apply_ref_commits"
        }
        "g5-api-core/services/git/remote_integrations/worker.rs" => {
            (matches!(
                function,
                "apply_cycle_watermarks" | "rollback_cycle_watermarks"
            ) && operation == "apply_ref_commits")
                || (function == "push_workspace_tip" && operation == "push_existing_commit")
        }
        "g5-api-core/services/workspace/workspace_service.rs" => {
            (function == "push_tracked_pr_source" && operation == "publish_commit")
                || (function == "publish_create_ref" && operation == "push_create_ref")
                || (function == "delete_workspace" && operation == "delete_workspace_refs")
        }
        "g5-api-core/services/git/workspace_pull_request_provider_lifecycle.rs" => {
            function == "push_provider_source_branch" && operation == "publish_commit"
        }
        // CLI checkout materialization and local alignment of the user's own
        // checkout; mapped-branch authority stays with the API.
        "g5d/cli/workspace_manifest.rs" => matches!(
            (function, operation),
            (
                "pin_repo_materialization" | "pin_docs_materialization",
                "checkout_branch_at"
            ) | (
                "apply_remote_name_alias",
                "checkout_branch_at" | "configure_branch_alias"
            ) | ("restore_local_unpublished_commits", "apply_branch_commit")
        ),
        _ => false,
    }
}
fn permitted_content_reader(relative: &str, function: &str, public: bool) -> bool {
    match relative {
        "g5-api-core/services/git/workspace_reader.rs" => {
            !public || function == "read_test_file_at_commit"
        }
        // Pinned-OID reads under their owner's authorization (design: second
        // abstraction audit). A new caller must name its owner here.
        "g5-api-core/services/workspace/workspace_review_service.rs" => {
            function == "generate_review_changes"
        }
        "g5-api-core/services/agents/approval_review_service.rs" => matches!(
            function,
            "validate_generated_v2_artifact" | "validate_published_artifact"
        ),
        "g5-api-core/services/workspace/workspace_fork_service.rs" => {
            function == "normalize_fork_receipt"
        }
        "g5-api-core/services/git/coordinated_repo_publication.rs" => {
            function == "derive_repo_pr_text"
        }
        _ => false,
    }
}

#[test]
fn raw_git_content_and_history_algorithms_are_private_to_workspace_reader() {
    let mut violations = Vec::new();
    for crate_name in backend_crates()
        .into_iter()
        .filter(|name| name.starts_with("g5-api"))
    {
        let source_root = generation_dir().join(&crate_name).join("src");
        for entry in WalkDir::new(&source_root) {
            let entry = entry.expect("walk API source");
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
                let relative = format!(
                    "{crate_name}/{}",
                    entry.path().strip_prefix(&source_root).unwrap().display()
                );
                violations.extend(content_reader_violations(
                    &relative,
                    &fs::read_to_string(entry.path()).unwrap(),
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn ref_mutations_outside_scm_use_named_policy_owners() {
    let mut violations = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut scanned = std::collections::BTreeSet::new();
    for crate_name in backend_crates()
        .into_iter()
        .filter(|name| !name.starts_with("g5-api"))
    {
        let source_root = generation_dir().join(&crate_name).join("src");
        // SCM mechanics own the ref primitives; its workspace policy module
        // is a consumer of them like every other crate.
        let walk_root = if crate_name == "g5-scm" {
            source_root.join("workspace")
        } else {
            source_root.clone()
        };
        for entry in WalkDir::new(&walk_root) {
            let entry = entry.expect("walk backend source");
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
                let relative = format!(
                    "{crate_name}/{}",
                    entry.path().strip_prefix(&source_root).unwrap().display()
                );
                let source = fs::read_to_string(entry.path()).unwrap();
                if REF_MUTATOR_OPERATIONS
                    .iter()
                    .any(|operation| source.contains(&format!("{operation}(")))
                {
                    seen.insert(relative.clone());
                }
                violations.extend(ref_mutation_violations(&relative, &source));
                scanned.insert(relative);
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    // Missing-target check: the named CLI owners still exist and are scanned.
    let owner = "g5d/cli/workspace_manifest.rs";
    assert!(
        seen.contains(owner),
        "ref-mutation owner {owner} was not scanned"
    );
    assert!(
        scanned.contains("g5-scm/workspace/commit.rs"),
        "the SCM workspace policy module was not scanned"
    );
}

#[test]
fn ref_mutation_guard_rejects_new_cli_writers_and_allows_named_owners() {
    for source in [
        "fn sneak() { repo.apply_branch_commit(\"main\", old, new); }",
        "fn sneak() { GitRepo::checkout_branch_at(&repo, \"main\", commit); }",
        "fn restore_local_unpublished_commits() { repo.publish_commit(&git, \"origin\", c, \"b\", None, &auth); }",
    ] {
        assert!(
            !ref_mutation_violations("g5d/cli/workspace_manifest.rs", source).is_empty(),
            "{source}"
        );
    }
    assert!(!ref_mutation_violations(
        "g5d-dispatch/new.rs",
        "fn checkpoint() { repo.create_branch(\"x\", head); }"
    )
    .is_empty());
    assert!(ref_mutation_violations(
        "g5d/cli/workspace_manifest.rs",
        "fn restore_local_unpublished_commits() { repo.apply_branch_commit(b, old, new); }"
    )
    .is_empty());
    // Outside the API, ordinary commits and reads are not owner-gated.
    assert!(ref_mutation_violations(
        "g5d/cli/init.rs",
        "fn init() { repo.commit(\"msg\", &author); repo.show_file(oid, path); }"
    )
    .is_empty());
    // Test modules remain exempt.
    assert!(ref_mutation_violations(
        "g5d/cli/workspace_manifest.rs",
        "#[cfg(test)] mod tests { fn t() { repo.apply_branch_commit(b, o, n); } }"
    )
    .is_empty());
}

#[test]
fn audit_reader_and_native_guard_calibration() {
    for source in [
        "fn read() { GitRepo::show_file(repo, oid, path); }",
        "use g5_scm::GitRepo as Repo; fn read() { Repo::show_file(repo, oid, path); }",
        "fn read() { selected.commit_file_diff(base, head, path); }",
        "fn read() { selected.show_file \n (oid, path); }",
        "fn read() { repo.file_diffs_between_commits(base, head); }",
        "fn read() { GitRepo::diff_name_only_between_commits(&repo, base, head); }",
        "fn read() { repo.commit_messages_ahead(target, source, cap); }",
    ] {
        assert!(
            !content_reader_violations("g5-api-new/new.rs", source).is_empty(),
            "{source}"
        );
    }
    for source in [
        "fn duplicate() { repo.apply_branch_commit(branch, old, new); }",
        "fn duplicate() { GitRepo::apply_ref_commits(updates); }",
        "fn duplicate() { repo.create_branch(branch, oid); }",
        "fn duplicate() { repo.commit(message, author); }",
        "fn duplicate() { GitRepo::commit(repo, message, author); }",
        "fn duplicate() { repo.publish_commit(&git, remote, commit, branch, None, &auth); }",
        "fn duplicate() { repo.push_create_ref(&git, remote, commit, branch, &auth); }",
        "fn duplicate() { repo.checkout_branch_at(branch, commit); }",
    ] {
        assert!(!content_reader_violations("g5-api-new/duplicate.rs", source).is_empty());
    }
    assert!(
        content_reader_violations("g5-api-new/transaction.rs", "fn commit() { db.commit(); }")
            .is_empty()
    );
    // A named pinned-read owner admits only its own operation function.
    assert!(content_reader_violations(
        "g5-api-core/services/workspace/workspace_review_service.rs",
        "fn generate_review_changes() { repo.file_diffs_between_commits(a, b); }"
    )
    .is_empty());
    assert!(!content_reader_violations(
        "g5-api-core/services/workspace/workspace_review_service.rs",
        "fn another_reader() { repo.file_diffs_between_commits(a, b); }"
    )
    .is_empty());
    assert!(content_reader_violations(
        "g5-api-core/services/git/workspace_reader.rs",
        "fn renamed_private_helper() { selected.show_file(oid, path); }"
    )
    .is_empty());
    assert!(!content_reader_violations(
        "g5-api-new/services/git/workspace_reader.rs",
        "fn renamed_private_helper() { selected.show_file(oid, path); }"
    )
    .is_empty());
    for scope in [
        "g5-scm/native/new_operation.rs",
        "g5-scm/new_backend.rs",
        "g5-scm/native/process.rs",
    ] {
        for source in [
            "fn duplicate() { std::process::Command::new(known_binary); }",
            "use tokio::process::Command as Child; fn duplicate() { Child::new(known_binary); }",
        ] {
            assert!(
                !typed_git_violations(scope, source, &[]).is_empty(),
                "{scope}: {source}"
            );
        }
    }
    assert!(typed_git_violations(
        "g5-scm/native/new_operation.rs",
        "fn renamed() { runner.head_commit(path); }",
        &[]
    )
    .is_empty());
    assert!(typed_git_violations(
        "g5-scm/native/process.rs",
        "use tokio::process::Command; fn new() { Command::new(binary); }",
        &[]
    )
    .is_empty());
    for source in [
        "pub fn raw() -> std::process::Command { todo!() }",
        "use tokio::process::Command as Builder; pub fn raw() -> Builder { todo!() }",
        "pub use std::process::Child as Running;",
        "use std as system; pub fn raw() -> system::process::Command { todo!() }",
        "use tokio::process as p; pub fn raw() -> p::Command { todo!() }",
    ] {
        assert!(
            !scm_public_backend_violations(&[("native/process.rs", source)]).is_empty(),
            "{source}"
        );
    }
}

#[test]
fn document_reference_correctness_does_not_use_compatibility_projection() {
    let read = |relative: &str| {
        fs::read_to_string(api_core_source(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"))
    };

    for retired in [
        "maintenance/search_indexer.rs",
        "services/workspace/projection_convergence.rs",
        "services/workspace/linked_doc_indexer.rs",
        "services/workspace/ontology_reconciler.rs",
        "services/workspace/ontology_service.rs",
    ] {
        assert!(
            !api_core_source(retired).exists(),
            "retired producer reintroduced: {retired}"
        );
    }
    let doc_service = read("services/workspace/doc_service.rs");
    for forbidden in ["doc_node_links", "ontology_nodes", "projection_state"] {
        assert!(
            !doc_service.contains(forbidden),
            "document UI/sync correctness must not use the compatibility projection: {forbidden}"
        );
    }

    let reference_service = read("services/workspace/doc_reference_service.rs");
    assert!(
        !reference_service.contains("FROM doc_node_links")
            && !reference_service.contains("JOIN doc_node_links")
            && !reference_service.contains("INSERT INTO doc_node_links"),
        "the canonical reference service must consume repository content, not the compatibility table"
    );
    let canonical_reference_service = reference_service
        .split("/// Enrich exact-cut component references for the deprecated UUID-shaped docs")
        .next()
        .expect("canonical reference section");
    assert!(
        !canonical_reference_service.contains("ontology_nodes"),
        "canonical reference resolution must read exact SemLift files, not the asynchronous ontology projection"
    );
}

#[test]
fn native_observation_owners_remain_present_and_free_of_process_bypasses() {
    for (path, scope, required) in [
        (
            "g5d-dispatch/src/modules/builtin.rs",
            "g5d-dispatch/modules/builtin.rs",
            &["changes", "refresh_provider_refs"][..],
        ),
        (
            "g5d-dispatch/src/workspace_flush.rs",
            "g5d-dispatch/workspace_flush.rs",
            &["capture", "capture_repo", "push_all"][..],
        ),
        (
            "g5-scm/src/native/member.rs",
            "g5-scm/native/member.rs",
            &["head_commit", "status", "divergence", "sequencers"][..],
        ),
        (
            "g5-scm/src/native/member/checkpoint.rs",
            "g5-scm/native/member/checkpoint.rs",
            &["checkpoint"][..],
        ),
        (
            "g5-scm/src/native/member/update.rs",
            "g5-scm/native/member/update.rs",
            &["update_parent"][..],
        ),
        (
            "g5-scm/src/native/member/transport.rs",
            "g5-scm/native/member/transport.rs",
            &["push_commit", "refresh_tracking", "switch_back"][..],
        ),
        (
            "g5-scm/src/native/member/flush.rs",
            "g5-scm/native/member/flush.rs",
            &["capture_flush", "push_flush", "file_inventory"][..],
        ),
        (
            "g5-scm/src/native/materialization.rs",
            "g5-scm/native/materialization.rs",
            &[
                "clone_bare_cache",
                "refresh_bare_cache",
                "materialize_exact_checkout",
            ][..],
        ),
        (
            "g5-scm/src/native/restore.rs",
            "g5-scm/native/restore.rs",
            &["restore_checkout"][..],
        ),
        (
            "g5d-foundation/src/workspace/repo_cache.rs",
            "g5d-foundation/workspace/repo_cache.rs",
            &["ensure_warm", "ensure_warm_with_token"][..],
        ),
        (
            "g5d-host/src/workspace_vm.rs",
            "g5d-host/workspace_vm.rs",
            &["restore_workspace"][..],
        ),
        (
            "g5-scm/src/native/configuration.rs",
            "g5-scm/native/configuration.rs",
            &[
                "write_task_safe_directory_config",
                "replace_task_safe_directory_config",
                "ensure_task_safe_directory_config",
                "install_member_configuration",
                "configure_member_remotes",
            ][..],
        ),
        (
            "g5-scm/src/native/checkout_queries.rs",
            "g5-scm/native/checkout_queries.rs",
            &[
                "version_blocking",
                "default_remote_branch",
                "head_commit_blocking",
                "path_is_ignored_blocking",
                "file_is_new_blocking",
            ][..],
        ),
        (
            "g5-api-routes/src/routes/git.rs",
            "g5-api-routes/routes/git.rs",
            &[
                "info_refs",
                "upload_pack",
                "receive_pack",
                "receive_pack_owned",
            ][..],
        ),
        (
            "g5-scm/src/native/protocol.rs",
            "g5-scm/native/protocol.rs",
            &["advertise_refs", "upload_pack", "receive_pack"][..],
        ),
        (
            "g5-api-routes/src/routes/workspace_flush.rs",
            "g5-api-routes/routes/workspace_flush.rs",
            &["complete"][..],
        ),
        (
            "g5-api-core/src/services/git/workspace_flush_service.rs",
            "g5-api-core/services/git/workspace_flush_service.rs",
            &["prove"][..],
        ),
        (
            "g5-scm/src/native/observations.rs",
            "g5-scm/native/observations.rs",
            &[
                "ref_tip",
                "symbolic_ref_target",
                "object_kind",
                "is_ancestor",
            ][..],
        ),
    ] {
        let source = fs::read_to_string(generation_dir().join(path)).unwrap();
        assert!(typed_git_violations(scope, &source, required).is_empty());
        let renamed = source
            .replace("require_commit", "check_commit_object")
            .replace("check_full_ref", "validate_full_reference");
        assert!(typed_git_violations(scope, &renamed, required).is_empty());
        assert!(!typed_git_violations(
            scope,
            &source.replace(required[0], "removed_owner"),
            required
        )
        .is_empty());
        for bypass in [
            "use tokio::process::Command as GitChild; fn bypass() { GitChild::new(binary); }",
            "fn bypass() { std::process::Command::new(binary); }",
            "use std::process as child; fn bypass() { child::Command::new(binary); }",
        ] {
            assert!(
                !typed_git_violations(scope, &format!("{source}\n{bypass}"), required).is_empty()
            );
        }
        assert!(typed_git_violations(
            scope,
            &format!("{source}\n#[cfg(test)] mod fixtures {{ use std::process::Command; }}"),
            required
        )
        .is_empty());
    }
}

#[test]
fn cli_backend_guard_allows_other_tools_and_rejects_git_and_backend_aliases() {
    for source in [
        "fn probe() { let _ = std::process::Command::new(\"git\"); }",
        "use std::process::Command as Process; fn probe() { let _ = Process::new(\"git\"); }",
        "use git2 as backend; fn probe() { let _ = backend::Repository::discover(\".\"); }",
    ] {
        assert!(!typed_git_violations("g5d/cli/spec.rs", source, &[]).is_empty());
    }
}

#[test]
fn cli_guard_covers_configured_programs_and_test_only_cfg() {
    for owner in [
        "g5d/cli/spec.rs",
        "g5d/cli/doctor.rs",
        "g5d/cli/job.rs",
        "g5-semantic/probe.rs",
    ] {
        for source in [
            r#"fn read() { std::process::Command::new(git_path); }"#,
            r#"use tokio::process::Command as Child; fn read() { Child::new(PROGRAM); }"#,
            r#"use std as system; fn read() { system::process::Command::new(program); }"#,
            r#"use tokio::{process as child}; fn read() { child::Command::new(program); }"#,
            r#"fn read() { std::process::Command::new(r"git"); }"#,
            r#"use std::process as p; use p::Command as C; fn read() { C::new("git"); }"#,
            r#"type Process = std::process::Command; fn read() { Process::new("git"); }"#,
            r#"use std::process as p; type Process = p::Command; fn read() { Process::new(program); }"#,
            r#"fn read() { std::process::Command::new("/usr/bin/git"); }"#,
            r#"const PROGRAM: &str = "git"; fn read() { std::process::Command::new(PROGRAM); }"#,
        ] {
            assert!(
                !typed_git_violations(owner, source, &["read"]).is_empty(),
                "{owner}: {source}"
            );
        }
    }
    let bypass = "mod fixture { fn read() { std::process::Command::new(binary); } }";
    for cfg in [
        "test",
        "all(test,unix)",
        "all(unix,any(test,all(test,feature=\"probe\")))",
        "not(not(test))",
    ] {
        assert!(
            typed_git_violations("g5d/cli/spec.rs", &format!("#[cfg({cfg})] {bypass}"), &[])
                .is_empty()
        );
    }
    for cfg in [
        "any(test,unix)",
        "not(test)",
        "all(unix,feature=\"a,test,b\")",
    ] {
        assert!(
            !typed_git_violations("g5d/cli/spec.rs", &format!("#[cfg({cfg})] {bypass}"), &[])
                .is_empty()
        );
    }
    assert!(!typed_git_violations(
        "g5d/cli/spec.rs",
        &format!("#[cfg(all(test,unix))] {bypass}"),
        &["read"]
    )
    .is_empty());
}

#[test]
fn block_filesystem_guard_refuses_git_and_dynamic_executables() {
    let owner = "g5d-isolation/isolation/workload_root.rs";
    assert!(typed_git_violations(
        owner,
        "fn helper() { std::process::Command::new(\"rsync\"); }",
        &[]
    )
    .is_empty());
    for source in [
        "fn helper() { std::process::Command::new(program); }",
        "fn helper() { std::process::Command::new(\"git\"); }",
        "fn other() { std::process::Command::new(\"rsync\"); }",
    ] {
        assert!(!typed_git_violations(owner, source, &[]).is_empty());
    }
    let launcher = "g5d-isolation/isolation/microvm.rs";
    assert!(typed_git_violations(
        launcher,
        "fn prepare_workload_templates_in_scope() { std::process::Command::new(std::env::current_exe()?); }",
        &[],
    )
    .is_empty());
    assert!(!typed_git_violations(
        launcher,
        "fn prepare_workload_templates() { std::process::Command::new(program); }",
        &[],
    )
    .is_empty());
}

#[test]
fn host_checkout_guard_allows_copy_but_refuses_native_git_bypasses() {
    let owner = "g5d-foundation/workspace/restore.rs";
    for source in [
        "fn copy() { tokio::process::Command::new(\"cp\"); }",
        "use tokio::process::Command as Copy; fn copy() { Copy::new(r#\"cp\"#); }",
    ] {
        assert!(typed_git_violations(owner, source, &[]).is_empty());
    }
    for source in [
        "fn run() { tokio::process::Command::new(binary); }",
        "use tokio::process::Command as Git; fn run() { Git::new(\"git\"); }",
        "fn run() { git2::Repository::open(path); }",
    ] {
        assert!(!typed_git_violations(owner, source, &[]).is_empty());
    }
}

#[test]
fn host_guard_covers_new_siblings_and_limits_generic_supervisor_exception() {
    for owner in [
        "g5d-host/new_git.rs",
        "g5d-foundation/workspace/new_git.rs",
        "g5d-foundation/new_git.rs",
        "g5d-agent/new_git.rs",
    ] {
        for source in [
            "fn f() { tokio::process::Command::new(\"git\"); }",
            "fn f() { std::process::Command::new(configured); }",
            "use std::process::Command as Child; type Alias = Child; fn f() { Alias::new(program); }",
        ] {
            assert!(!typed_git_violations(owner, source, &[]).is_empty(), "{owner}: {source}");
        }
    }
    let owner = "g5d-foundation/process_termination/execution_owner.rs";
    assert!(typed_git_violations(owner, "fn command() { tokio::process::Command::new(std::env::current_exe()?); } fn run() { std::process::Command::new(program); }", &[]).is_empty());
    for source in [
        "fn new_git() { std::process::Command::new(program); }",
        "fn run() { std::process::Command::new(\"git\"); }",
    ] {
        assert!(!typed_git_violations(owner, source, &[]).is_empty());
    }
    assert!(typed_git_violations(
        "g5d-foundation/workspace/restore.rs",
        "fn copy() { std::process::Command::new(\"cp\"); }",
        &[]
    )
    .is_empty());
}

#[test]
fn alias_resolution_stops_expanding_recursive_module_prefixes() {
    let aliases =
        std::collections::HashMap::from([("nested".to_owned(), "nested::module".to_owned())]);
    assert_eq!(
        resolve_alias_path("nested::Command::new", &aliases),
        "nested::module::Command::new"
    );
}

fn fixture_module_is_gated(source: &str, module: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut cursor = tree.root_node().walk();
    let mut gated = false;
    let mut found = false;
    for node in tree.root_node().named_children(&mut cursor) {
        let text: String = source[node.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if node.kind() == "attribute_item" {
            gated |= matches!(
                text.as_str(),
                "#[cfg(any(test,feature=\"test-helpers\"))]"
                    | "#[cfg(feature=\"test-helpers\")]"
                    | "#[cfg(test)]"
            );
        } else if !matches!(node.kind(), "line_comment" | "block_comment") {
            if node.kind() == "mod_item"
                && node
                    .child_by_field_name("name")
                    .is_some_and(|n| &source[n.byte_range()] == module)
            {
                if !gated {
                    return false;
                }
                found = true;
            }
            gated = false;
        }
    }
    found
}

#[test]
fn host_git_fixture_stays_test_feature_gated() {
    let source = fs::read_to_string(generation_dir().join("g5d-host/src/lib.rs")).unwrap();
    assert!(fixture_module_is_gated(&source, "test_support"));
    assert!(!typed_git_violations(
        "g5d-host/test_support.rs",
        "fn new_git() { std::process::Command::new(\"git\"); }",
        &[]
    )
    .is_empty());
}

#[test]
fn guest_guard_covers_siblings_and_limits_generic_module_execution() {
    // The grammar's existing field-attribute limitation must not exclude
    // constructors elsewhere in that same function or nested pattern arm.
    let pattern = r#"fn handle(value: &str) {
        let Ok(Message::Run {
            args,
            #[cfg(feature = "test-support")]
            token,
        }) = parse(value) else { return; };
        EFFECT
    }"#;
    let allowed = typed_git_violations(
        "g5d-dispatch/server.rs",
        &pattern.replace("EFFECT", "consume(args);"),
        &[],
    );
    assert!(allowed.is_empty(), "{allowed:?}");
    assert!(!typed_git_violations(
        "g5d-dispatch/server.rs",
        &pattern.replace("EFFECT", "std::process::Command::new(binary);"),
        &[]
    )
    .is_empty());

    for owner in [
        "g5d-dispatch/new_git.rs",
        "g5d-dispatch/modules/builtin.rs",
        "g5d-dispatch/workspace_flush.rs",
    ] {
        for source in [
            "fn run() { std::process::Command::new(program); }",
            "fn run() { ctx.provider_command(\"git\"); }",
            "use tokio::process::Command as Child; fn run() { Child::new(\"git\"); }",
        ] {
            assert!(!typed_git_violations(owner, source, &[]).is_empty());
        }
    }
    assert!(typed_git_violations(
        "g5d-dispatch/git.rs",
        "fn provider_command() { std::process::Command::new(program); }",
        &[]
    )
    .is_empty());
    assert!(!typed_git_violations(
        "g5d-dispatch/git.rs",
        "fn bypass() { std::process::Command::new(program); }",
        &[]
    )
    .is_empty());
    assert!(!typed_git_violations(
        "g5d-dispatch/git.rs",
        "fn provider_command() { std::process::Command::new(\"git\"); }",
        &[]
    )
    .is_empty());
}

#[test]
fn whole_backend_guard_covers_new_crates_and_preserves_local_names() {
    let owner = "g5-new-crate/new_module.rs";
    for source in [
        "fn f(git2: usize, libgit2_sys: usize) -> usize { git2 + libgit2_sys }",
        "fn f() { let git2 = 1; consume(git2); }",
    ] {
        assert!(typed_git_violations(owner, source, &[]).is_empty());
    }
    for source in [
        "fn f() { git2::Repository::open(path); }",
        "use git2 as backend; fn f() { backend::Repository::open(path); }",
        "use libgit2_sys::*; fn f() {}",
        "fn f() { std::process::Command::new(\"git\"); }",
        "use tokio::process::Command as Child; fn f() { Child::new(program); }",
    ] {
        assert!(
            !typed_git_violations(owner, source, &[]).is_empty(),
            "{source}"
        );
    }
    for (owner, function, argument) in [
        ("g5d-agent/runtime.rs", "run", "&program"),
        ("g5d-agent/codex_auth.rs", "command", "program"),
        ("g5d-isolation/isolation/passt.rs", "spawn_passt_at", "bin"),
        (
            "g5d-isolation/isolation/guest_egress.rs",
            "install",
            "\"/usr/sbin/nft\"",
        ),
        (
            "g5d-isolation/isolation/cloud_hypervisor.rs",
            "run",
            "vmm_binary()",
        ),
        ("g5d/cli/vm_init.rs", "run", "&config.command[0]"),
        (
            "g5d/cli/vm_init.rs",
            "busybox",
            "\"/opt/g5/bootstrap/busybox\"",
        ),
        (
            "g5d-isolation/isolation/microvm.rs",
            "prepare_workload_image",
            "std::env::current_exe()?",
        ),
        (
            "g5d-isolation/isolation/microvm.rs",
            "build_vm_exec_command_with_env",
            "Self::vm_exec_binary()",
        ),
        (
            "g5d-isolation/isolation/microvm.rs",
            "prepare_private_workload",
            "Self::vm_exec_binary()",
        ),
        (
            "g5d-isolation/isolation/mod.rs",
            "platform_command",
            "binary",
        ),
        (
            "g5d-isolation/isolation/process.rs",
            "start",
            "&command_path",
        ),
    ] {
        assert!(typed_git_violations(
            owner,
            &format!("fn {function}() {{ std::process::Command::new({argument}); }}"),
            &[]
        )
        .is_empty());
        for source in [
            format!("fn new_git() {{ std::process::Command::new({argument}); }}"),
            format!("fn {function}() {{ std::process::Command::new(\"git\"); }}"),
        ] {
            assert!(
                !typed_git_violations(owner, &source, &[]).is_empty(),
                "{owner}: {source}"
            );
        }
    }
}

#[test]
fn dispatch_git_fixture_stays_test_feature_gated() {
    let source = fs::read_to_string(generation_dir().join("g5d/src/lib.rs")).unwrap();
    assert!(fixture_module_is_gated(&source, "dispatch_test_support"));
    for declaration in [
        "#[cfg(feature=\"test-helpers\")] pub mod dispatch_test_support;",
        "#[cfg(feature=\"test-helpers\")] /* fixture */ pub mod dispatch_test_support;",
    ] {
        assert!(fixture_module_is_gated(
            declaration,
            "dispatch_test_support"
        ));
    }
    for declaration in [
        "// #[cfg(feature=\"test-helpers\")] pub mod dispatch_test_support;\npub mod dispatch_test_support;",
        "// #[cfg(feature=\"test-helpers\")] pub mod dispatch_test_support;",
        "pub mod dispatch_test_support;",
        "#[cfg(any(test,unix))] pub mod dispatch_test_support;",
        "#[cfg(feature=\"test-helpers\")] pub mod other; pub mod dispatch_test_support;",
        "",
    ] {
        assert!(!fixture_module_is_gated(declaration, "dispatch_test_support"), "{declaration}");
    }
    assert!(!typed_git_violations(
        "g5d/dispatch_test_support.rs",
        "fn new_git() { std::process::Command::new(\"git\"); }",
        &[]
    )
    .is_empty());
}

#[test]
fn provider_guards_reject_raw_http_and_allow_owned_adapters() {
    for owner in [
        "g5-api-core/services/git/github_app.rs",
        "g5-api-core/services/git/github_issues.rs",
    ] {
        for source in [
            "use reqwest::Client as Http; fn f(url: &str) { Http::new().get(url); }",
            "fn f() { reqwest::Client::new().post(configured_url); }",
        ] {
            assert!(!typed_git_violations(owner, source, &[]).is_empty());
        }
        assert!(typed_git_violations(owner,
            "fn f(reqwest: usize) { consume(reqwest); } fn operation() { selected.installation_metadata(1); } #[cfg(test)] mod fixtures { use reqwest::Client; }", &[]).is_empty());
    }
    for source in [
        "pub fn raw() -> reqwest::Client { todo!() }",
        "use reqwest::Client as Http; type Alias = Http; pub fn raw() -> Alias { todo!() }",
        "pub use reqwest::RequestBuilder as Adapter;",
        "pub struct Adapter { pub client: reqwest::Client }",
    ] {
        assert!(!scm_public_backend_violations(&[("providers/client.rs", source)]).is_empty());
    }
    assert!(scm_public_backend_violations(&[("providers/client.rs",
        "pub struct Adapter { client: reqwest::Client } impl Adapter { pub fn issue(&self) -> OwnedIssue { todo!() } } struct OwnedIssue;")]).is_empty());
}

#[test]
fn migrated_provider_adapters_remain_present() {
    for (relative, functions) in [
        (
            "services/identity/auth_service.rs",
            &[
                "handle_callback",
                "handle_scm_callback",
                "account_protocol",
                "connection_protocol",
            ][..],
        ),
        (
            "auth/scm.rs",
            &["validate_pat", "access_token", "refresh_gitlab_token"][..],
        ),
        (
            "services/git/workspace_pull_request_provider_lifecycle.rs",
            &[
                "repository_protocol",
                "create_pull_request",
                "merge_pull_request",
                "published_remote_head",
            ][..],
        ),
        (
            "services/git/workspace_pull_request_read.rs",
            &["read_github_pull_request"][..],
        ),
        (
            "services/git/workspace_pull_request_checks.rs",
            &["read_checks"][..],
        ),
        (
            "services/identity/platform_issue_github.rs",
            &[
                "protocol",
                "installation_token",
                "find_issue_by_marker",
                "put_screenshot",
            ][..],
        ),
        (
            "services/git/github_app.rs",
            &[
                "installation_token",
                "exchange_user_grant_code",
                "refresh_user_token",
                "repository_protocol",
            ][..],
        ),
        (
            "services/git/github_issues.rs",
            &[
                "fetch_repository",
                "fetch_issue",
                "post_issue_comment",
                "search_repository_issues",
            ][..],
        ),
    ] {
        let source =
            fs::read_to_string(generation_dir().join("g5-api-core/src").join(relative)).unwrap();
        let owner = format!("g5-api-core/{relative}");
        assert!(typed_git_violations(&owner, &source, functions).is_empty());
        assert!(!typed_git_violations(&owner, "// removed implementation", functions).is_empty());
    }
}

fn enclosing_function<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> &'a str {
    let mut parent = node.parent();
    while let Some(item) = parent {
        if item.kind() == "function_item" {
            if let Some(name) = item.child_by_field_name("name") {
                return &source[name.byte_range()];
            }
            break;
        }
        parent = item.parent();
    }
    "<declarations>"
}
/// A raw HTTP client path: `reqwest` (other than its URL and, inside the
/// provider adapters, method values) or a `hyper`/`hyper_util` client.
fn raw_http_client_path(relative: &str, resolved: &str) -> bool {
    (resolved.starts_with("reqwest::")
        && !resolved.starts_with("reqwest::Url")
        && !(relative.starts_with("g5-scm/providers/") && resolved.starts_with("reqwest::Method")))
        || resolved.starts_with("hyper::client")
        || resolved == "hyper::Client"
        || resolved.starts_with("hyper::Client::")
        || resolved.starts_with("hyper_util::client")
}
/// Request builders whose URL argument a reviewed non-provider owner must not
/// aim at a source-control provider: (method name, URL argument index).
const REQUEST_BUILDERS: &[(&str, usize)] = &[
    ("get", 0),
    ("post", 0),
    ("put", 0),
    ("patch", 0),
    ("delete", 0),
    ("head", 0),
    ("request", 1),
];
fn names_scm_provider(expression: &str) -> bool {
    let lower = expression.to_ascii_lowercase();
    lower.contains("github") || lower.contains("gitlab")
}
fn provider_http_violations(relative: &str, source: &str) -> Vec<String> {
    if matches!(
        relative,
        "g5d-foundation/oci/ecr_tests.rs"
            | "g5d-foundation/oci/end_to_end_tests.rs"
            | "g5d-host/cache_door/tests.rs"
    ) {
        let module = relative.rsplit('/').next().unwrap().trim_end_matches(".rs");
        let (crate_name, rest) = relative.split_once('/').unwrap();
        let parent_dir = rest.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let parent = fs::read_to_string(
            generation_dir()
                .join(crate_name)
                .join("src")
                .join(parent_dir)
                .join("mod.rs"),
        )
        .unwrap();
        if fixture_module_is_gated(&parent, module) {
            return Vec::new();
        }
    }
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut violations = std::collections::BTreeSet::new();
    let mut nodes = vec![tree.root_node()];
    let mut http_references = false;
    let mut deliveries = Vec::new();
    let mut imports = std::collections::HashMap::new();
    let mut qualified = Vec::new();
    // Request URL arguments and local bindings, per enclosing function.
    let mut requests = Vec::new();
    let mut bindings = std::collections::HashMap::new();
    while let Some(node) = nodes.pop() {
        let text = &source[node.byte_range()];
        if node.kind() == "use_declaration" {
            let mut paths = Vec::new();
            imported_paths(node, "", source, &mut paths);
            for (path, alias) in paths {
                imports.insert(alias, path);
            }
        }
        if node.kind() == "extern_crate_declaration" {
            let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            for krate in ["reqwest", "hyper_util", "hyper"] {
                if let Some(alias) = compact
                    .strip_prefix(&format!("externcrate{krate}as"))
                    .and_then(|s| s.strip_suffix(';'))
                {
                    imports.insert(alias.into(), krate.into());
                }
            }
        }
        let scoped = matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier");
        let reference = !node.parent().is_some_and(|p| {
            source[p.byte_range()]
                .trim_start_matches(':')
                .starts_with("reqwest::Url")
                || (relative.starts_with("g5-scm/providers/")
                    && source[p.byte_range()]
                        .trim_start_matches(':')
                        .starts_with("reqwest::Method"))
        }) && matches!(node.kind(), "identifier" | "type_identifier")
            && text == "reqwest"
            && node.parent().is_some_and(|p| {
                matches!(
                    p.kind(),
                    "scoped_identifier"
                        | "scoped_type_identifier"
                        | "use_declaration"
                        | "scoped_use_list"
                        | "use_wildcard"
                        | "use_list"
                        | "use_as_clause"
                        | "extern_crate_declaration"
                )
            });
        let method = (node.kind() == "call_expression")
            .then(|| node.child_by_field_name("function"))
            .flatten()
            .filter(|f| f.kind() == "field_expression")
            .and_then(|f| f.child_by_field_name("field"))
            .map(|field| &source[field.byte_range()]);
        let delivery = method.is_some_and(|m| matches!(m, "send" | "execute" | "send_request"));
        if let Some(index) = method.and_then(|m| {
            REQUEST_BUILDERS
                .iter()
                .find(|(name, _)| *name == m)
                .map(|(_, index)| *index)
        }) {
            if let Some(argument) = node.child_by_field_name("arguments").and_then(|arguments| {
                let mut cursor = arguments.walk();
                let argument = arguments.named_children(&mut cursor).nth(index);
                argument
            }) {
                requests.push((
                    enclosing_function(node, source).to_owned(),
                    source[argument.byte_range()].to_owned(),
                ));
            }
        }
        if node.kind() == "let_declaration" {
            if let (Some(pattern), Some(value)) = (
                node.child_by_field_name("pattern"),
                node.child_by_field_name("value"),
            ) {
                bindings.insert(
                    (
                        enclosing_function(node, source).to_owned(),
                        source[pattern.byte_range()]
                            .trim_start_matches("mut ")
                            .to_owned(),
                    ),
                    source[value.byte_range()].to_owned(),
                );
            }
        }
        if reference || delivery || scoped {
            let enclosing = enclosing_function(node, source);
            if scoped {
                qualified.push((
                    text.chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                    enclosing.to_owned(),
                ));
            }
            if reference {
                http_references = true;
            }
            if delivery {
                deliveries.push(enclosing.to_owned());
            }
            if reference && !permitted_non_git_http(relative, enclosing) {
                violations.insert(format!(
                    "{relative}::{enclosing}: raw HTTP bypasses provider ownership"
                ));
            }
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                test_only |= test_only_attribute(
                    &source[child.byte_range()]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                );
                continue;
            }
            if matches!(child.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    for (path, enclosing) in qualified {
        let resolved = resolve_alias_path(&path, &imports);
        if raw_http_client_path(relative, &resolved) {
            http_references = true;
            if !permitted_non_git_http(relative, &enclosing) {
                violations.insert(format!(
                    "{relative}::{enclosing}: aliased raw HTTP bypasses provider ownership"
                ));
            }
        }
    }
    if http_references {
        for enclosing in deliveries {
            if !permitted_non_git_http(relative, &enclosing) {
                violations.insert(format!(
                    "{relative}::{enclosing}: raw HTTP delivery bypasses provider ownership"
                ));
            }
        }
        // A reviewed non-provider owner is permitted for its own destination,
        // not for a new source-control provider call inside the same function.
        if !relative.starts_with("g5-scm/providers/") {
            for (enclosing, argument) in requests {
                let local = argument.trim_start_matches('&').trim();
                let bound = bindings
                    .get(&(enclosing.clone(), local.to_owned()))
                    .map(String::as_str)
                    .unwrap_or_default();
                if names_scm_provider(&argument) || names_scm_provider(bound) {
                    violations.insert(format!(
                        "{relative}::{enclosing}: provider request outside the provider adapters"
                    ));
                }
            }
        }
    }
    violations.into_iter().collect()
}
fn permitted_non_git_http(relative: &str, function: &str) -> bool {
    // Reviewed non-repository HTTP owners. Exceptions name existing operations,
    // never a whole file: new helpers must establish their own legitimate owner.
    match relative {
        // `hosted_transport`/`open_hosted`: the credential-free fetch of an
        // issue attachment from GitHub's own content hosts, redirects checked
        // (ontology g5.attachments.github).
        "g5-scm/providers/transport.rs" => matches!(
            function,
            "<declarations>"
                | "new"
                | "send"
                | "rate_limit_headers"
                | "same_origin_redirect"
                | "hosted_transport"
                | "open_hosted"
        ),
        // The email module's private Resend client is the one owner of the
        // outbound email provider protocol (ontology g5.email.boundary).
        // The Entra client is the one owner of the Microsoft identity-platform
        // login protocol (token exchange and signing-key retrieval); it never
        // reaches SCM. ID-token verification replaces the former userinfo call.
        "g5-api-core/auth/entra.rs" => matches!(
            function,
            "<declarations>" | "new" | "exchange_code" | "signing_key" | "redact_error"
        ),
        "g5-api-core/services/email/transport.rs" => matches!(
            function,
            "<declarations>" | "new" | "send" | "classify_transport_error"
        ),
        "g5-api-core/cloud/gcp.rs" => matches!(
            function,
            "<declarations>"
                | "ensure_firewalls"
                | "fetch_token"
                | "from_env"
                | "generate_service_account_access_token"
                | "get_optional"
                | "list_instances_by_prefix"
                | "list_up_zones_in_region"
                | "parse_token_response"
                | "poll_operation"
                | "post_create"
                | "request"
                | "terminate_instance"
        ),
        // Preview tunnels relay a workspace's own HTTP service over hyper.
        "g5-api-routes/routes/preview.rs" => matches!(
            function,
            "bridge_mux_stream"
                | "handshake_upstream"
                | "poll_frame"
                | "proxy"
                | "relay_websocket"
                | "send_upstream_request"
        ),
        "g5-api-core/services/hosts/runtime_label_resolver.rs" => matches!(
            function,
            "<declarations>"
                | "build_client"
                | "fetch_anonymous_token"
                | "for_tests"
                | "get_with_anonymous_bearer"
                | "read_body_capped"
                | "send_get"
                | "status_or_error"
                | "validate_bearer_realm"
                | "validate_registry_host"
                | "with_scheme"
        ),
        "g5-internal-api/client.rs" => matches!(
            function,
            "default_headers"
                | "<declarations>"
                | "build_session_files_url"
                | "chat_session_file_content"
                // G5's own PR create/update route, retried on gateway errors only.
                | "create_workspace_pull_request"
                | "is_gateway_status"
                | "get_json_from_absolute_url"
                // An issue or pull-request attachment streamed from G5's own
                // internal route.
                | "issue_attachment_content"
                | "pull_request_attachment_content"
                | "new"
                | "parse_error_response"
                | "parse_error_response_from_status_and_text"
                | "post_json"
                | "post_json_once"
                | "put_chat_session_file"
                | "put_chat_session_file_with_base"
                | "put_no_response"
                | "send_request_with_retry"
                | "send_with_retry"
        ),
        "g5d/cli/api_client.rs" => matches!(
            function,
            "<declarations>"
                | "build_http_client"
                | "do_delete"
                | "do_get"
                | "do_patch"
                | "do_post"
                | "refresh_auth"
                | "status_to_error"
        ),
        "g5d/cli/auth.rs" => matches!(function, "login" | "logout" | "post_json"),
        "g5d/cli/bootstrap_supervisor.rs" => matches!(function, "submit_graph"),
        "g5d/cli/doctor.rs" => matches!(function, "check_workspace"),
        "g5d/cli/issue.rs" => matches!(
            function,
            "auth_headers"
                | "<declarations>"
                | "execute"
                | "report_platform_issue"
                | "send_json_response"
        ),
        // Saves an issue attachment streamed from G5's internal route
        // (ontology g5.attachments.cli); the client never holds GitHub's.
        "g5d/cli/issue_attachments.rs" => matches!(function, "save_one" | "open"),
        "g5d/cli/pr.rs" => matches!(function, "checks_path" | "request_path"),
        // An agent's one request to a G5 preview origin with the credential
        // minted for it, never following a redirect (g5.preview.agent-access).
        "g5d/cli/workspace_agent.rs" => function == "preview_fetch",
        "g5d/cli/workspace_manifest.rs" => matches!(
            function,
            "add_workspace_repo_remote"
                | "authorized_request"
                | "clone_workspace_remote"
                | "fetch_workspace_branch_pins"
                | "fetch_workspace_detail"
                | "fetch_workspace_manifest"
                | "list_org_repos"
                | "list_org_workspaces"
                | "map_add_repo_status_body"
                | "map_api_status"
                | "map_api_status_body"
                | "normalize_api_url"
                | "parse_repo_id_from_git_url"
                | "parse_workspace_url"
                | "resolve_workspace_slug"
                | "rewrite_git_transport_url"
        ),
        "g5d-dispatch/git_transport.rs" => matches!(
            function,
            "<declarations>" | "handle" | "start_with_spool_budget"
        ),
        "g5d-dispatch/workspace_flush.rs" => matches!(function, "<declarations>" | "new" | "post"),
        "g5d-foundation/oci/acr.rs" => matches!(
            function,
            "exchange_access_token"
                | "exchange_refresh_token"
                | "mint"
                | "resolve_aad"
                | "try_imds"
                | "try_service_principal"
        ),
        "g5d-foundation/oci/auth.rs" => matches!(function, "fetch_token" | "mint"),
        "g5d-foundation/oci/ecr.rs" => matches!(
            function,
            "creds_from_imds"
                | "get_authorization_token"
                | "host_header"
                | "mint"
                | "resolve_aws_creds"
        ),
        "g5d-foundation/oci/error.rs" => matches!(function, "<declarations>"),
        // The cache door's fleet reader owns the object-storage protocol
        // (metadata token plus `storage/v1` object reads) for the host; no
        // guest ever holds the credential (ontology g5.runtime.cache.fleet).
        "g5d-host/cache_door/gcs.rs" => matches!(
            function,
            "<declarations>" | "with_endpoints" | "token" | "get"
        ),
        "g5d-foundation/oci/gar.rs" => matches!(
            function,
            "mint" | "resolve_basic_creds" | "try_metadata" | "try_service_account"
        ),
        "g5d-foundation/oci/registry.rs" => matches!(
            function,
            "https" | "<declarations>" | "authed_get" | "finalize_response"
        ),
        _ => false,
    }
}
#[test]
fn all_backend_provider_protocol_uses_its_owner() {
    let mut violations = Vec::new();
    for crate_name in backend_crates() {
        let root = generation_dir().join(&crate_name).join("src");
        for entry in WalkDir::new(&root) {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "rs") {
                let relative = format!(
                    "{crate_name}/{}",
                    path.strip_prefix(&root).unwrap().display()
                );
                violations.extend(provider_http_violations(
                    &relative,
                    &fs::read_to_string(path).unwrap(),
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn provider_guard_covers_new_crates_and_new_helpers_in_http_owners() {
    for owner in [
        "new-crate/reader.rs",
        "g5-scm/new_reader.rs",
        "g5-scm/providers/pulls.rs",
        "g5-scm/providers/transport.rs",
        "g5-api-core/services/identity/auth_service.rs",
        "g5-api-core/auth/entra.rs",
        "g5-api-core/cloud/gcp.rs",
    ] {
        for source in [
            "use reqwest::Client as Http; fn new_helper() { Http::new().get(configured_url).send(); }",
            "use reqwest as http; fn new_helper() { http::Client::new().post(configured_url); }",
            "extern crate reqwest as http; fn new_helper() { http::Client::new(); }",
            "struct Existing { http: reqwest::Client } impl Existing { fn new_helper(&self) { self.http.get(configured_url).send(); } }",
        ] { assert!(!provider_http_violations(owner,source).is_empty(),"{owner}: {source}"); }
        for source in [
            "fn benign(reqwest:usize) { consume(reqwest); }",
            "fn parse() { let url=reqwest::Url::parse(value); sql.execute(db); }",
            "#[cfg(test)] mod fixtures {use reqwest::Client; fn fixture(){Client::new();}}",
            "fn read() { selected.read_pull_request(1,false); }",
        ] {
            assert!(
                provider_http_violations(owner, source).is_empty(),
                "{owner}: {source}"
            );
        }
    }
    assert!(provider_http_violations("g5-api-core/cloud/gcp.rs","struct Cloud { http:reqwest::Client } impl Cloud {fn request(&self) {self.http.get(cloud_url).send();}}").is_empty());
    assert!(!provider_http_violations(
        "g5-api-core/services/new.rs",
        "fn request() {reqwest::Client::new().get(configured_url).send();}"
    )
    .is_empty());
}

#[test]
fn scm_http_guard_permits_shared_transport_and_typed_helpers_only() {
    assert!(provider_http_violations("g5-scm/providers/transport.rs", "struct Transport { http: reqwest::Client } impl Transport { fn new() { reqwest::Client::builder(); } fn send(&self) { self.http.execute(request); } }").is_empty());
    assert!(provider_http_violations("g5-scm/providers/pulls.rs", "impl Client { fn renamed_helper(&self) { self.http.send(Request { method: reqwest::Method::GET }); } }").is_empty());
    for owner in ["g5-scm/new.rs", "g5-scm/providers/pulls.rs"] {
        assert!(!provider_http_violations(
            owner,
            "async fn bypass(url: &str) { reqwest::Client::new().get(url).send().await; }"
        )
        .is_empty());
    }
}

#[test]
fn http_permits_check_request_destinations_and_non_reqwest_clients() {
    let owner = "g5-api-core/cloud/gcp.rs";
    let within = |body: &str| {
        format!("struct Cloud {{ http: reqwest::Client }} impl Cloud {{ fn request(&self) {{ {body} }} }}")
    };
    // A reviewed owner's own destination and harmless refactors stay permitted.
    for body in [
        "self.http.get(cloud_url).send();",
        "let endpoint = format!(\"{}/compute\", self.base); self.http.post(&endpoint).send();",
        "self.http.request(Method::GET, operation_url(&self.project)).send();",
        "let provider = self.labels.get(\"zone\"); self.http.get(zone_url(provider)).send();",
    ] {
        let source = within(body);
        assert!(
            provider_http_violations(owner, &source).is_empty(),
            "{source}"
        );
    }
    // A new source-control provider call inside the same permitted function fails.
    for body in [
        "self.http.get(\"https://api.github.com/repos/o/r\").send();",
        "self.http.post(format!(\"{}/repos/o/r/pulls\", github_api)).send();",
        "let url = format!(\"https://gitlab.com/api/v4/projects/{id}\"); self.http.get(&url).send();",
        "self.http.request(Method::POST, \"https://api.github.com/graphql\").send();",
    ] {
        let source = within(body);
        assert!(!provider_http_violations(owner, &source).is_empty(), "{source}");
    }
    // Non-reqwest HTTP clients are raw HTTP too.
    for source in [
        "use hyper_util::client::legacy::Client; fn new_helper() { Client::builder(executor).build_http().request(req); }",
        "fn new_helper() { hyper_util::client::legacy::Client::builder(executor); }",
        "use hyper::client::conn::http1; async fn new_helper(io: Io) { let (mut sender, conn) = http1::handshake(io).await.unwrap(); sender.send_request(req).await; }",
        "extern crate hyper_util as h; fn new_helper() { h::client::legacy::Client::builder(executor); }",
    ] {
        assert!(
            !provider_http_violations("g5-api-core/services/new.rs", source).is_empty(),
            "{source}"
        );
    }
    // Server and body types are not clients.
    assert!(provider_http_violations(
        "g5-api-core/services/new.rs",
        "fn serve(body: hyper::body::Incoming) -> hyper::Response<()> { todo!() }"
    )
    .is_empty());
}

/// Names that select repository credentials. App installation-token issuance
/// must never reach them: the credential owner calls issuance, never the
/// reverse, so issuance cannot recurse into credential selection.
const CREDENTIAL_SELECTION: &[&str] = &[
    "RepositoryCredentials",
    "repository_credentials",
    "RepositoryOperation",
    "resolve_target",
    "resolve_in_scope",
    "resolve_adoption",
];
const ISSUANCE_OWNERS: &[(&str, &[&str])] = &[
    ("g5-scm/src/providers/app.rs", &["installation_token"]),
    (
        "g5-api-core/src/services/git/github_app.rs",
        &["installation_token", "mint_app_jwt"],
    ),
    (
        "g5-api-core/src/services/git/github_app_binding.rs",
        &["mint_repository_token"],
    ),
    (
        "g5-api-core/src/services/identity/github_app_auth.rs",
        &["sign_app_jwt", "insert"],
    ),
    (
        "g5-api-core/src/services/identity/platform_issue_github.rs",
        &["installation_token"],
    ),
];
fn issuance_recursion_violations(relative: &str, source: &str, issuers: &[&str]) -> Vec<String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut violations = std::collections::BTreeSet::new();
    let mut functions = std::collections::HashSet::new();
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        let text = &source[node.byte_range()];
        if node.kind() == "function_item" {
            if let Some(name) = node.child_by_field_name("name") {
                functions.insert(source[name.byte_range()].to_owned());
            }
        }
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "field_identifier"
        ) && CREDENTIAL_SELECTION.contains(&text)
        {
            violations.insert(format!(
                "{relative}::{}: token issuance reaches repository credential selection ({text})",
                enclosing_function(node, source)
            ));
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                test_only |= test_only_attribute(
                    &source[child.byte_range()]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                );
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    for issuer in issuers {
        if !functions.contains(*issuer) {
            violations.insert(format!("{relative}: issuance owner {issuer} is missing"));
        }
    }
    violations.into_iter().collect()
}
#[test]
fn installation_token_issuance_never_selects_repository_credentials() {
    let mut violations = Vec::new();
    for (relative, issuers) in ISSUANCE_OWNERS {
        let source = fs::read_to_string(generation_dir().join(relative)).unwrap();
        violations.extend(issuance_recursion_violations(relative, &source, issuers));
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
#[test]
fn issuance_guard_rejects_recursion_and_missing_owners_but_allows_refactors() {
    let issuers = &["installation_token"];
    for source in [
        "impl App { async fn installation_token(&self, c: &RepositoryCredentials) { c.resolve().await; } }",
        "fn installation_token() { crate::auth::repository_credentials::select(); }",
        "fn installation_token() { helper(); } fn helper(state: &State) { state.repository_credentials.resolve(); }",
        "fn installation_token(op: RepositoryOperation) { issue(op); }",
        "fn installation_token(c: &Credentials) { c.resolve_target(); }",
        "fn renamed_issuer() { issue(); }",
    ] {
        assert!(
            !issuance_recursion_violations("issuer.rs", source, issuers).is_empty(),
            "{source}"
        );
    }
    for source in [
        "fn installation_token(&self) { if let Some(t) = self.tokens.get(&key) { return t; } self.issue(); }",
        "fn installation_token() { issue(); } fn renamed_private_helper() { sign(); } #[cfg(test)] mod tests { use crate::auth::RepositoryCredentials; }",
    ] {
        assert!(
            issuance_recursion_violations("issuer.rs", source, issuers).is_empty(),
            "{source}"
        );
    }
}
fn provider_dispatch_violations(relative: &str, source: &str) -> Vec<String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut violations = std::collections::BTreeSet::new();
    let mut nodes = vec![tree.root_node()];
    while let Some(node) = nodes.pop() {
        if node.kind() == "string_literal"
            && matches!(&source[node.byte_range()], "\"github\"" | "\"gitlab\"")
        {
            let mut parent = node.parent();
            let mut dispatch = false;
            let mut function = "<declarations>";
            while let Some(item) = parent {
                dispatch |= matches!(item.kind(), "binary_expression" | "match_pattern");
                if item.kind() == "function_item" {
                    function = item
                        .child_by_field_name("name")
                        .map(|n| &source[n.byte_range()])
                        .unwrap_or(function);
                    break;
                }
                parent = item.parent();
            }
            if dispatch && !permitted_provider_selection(relative, function) {
                violations.insert(format!("{relative}::{function}: provider-name dispatch belongs to composition or the scoped protocol adapter"));
            }
        }
        let mut cursor = node.walk();
        let mut test_only = false;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "attribute_item" {
                test_only |= test_only_attribute(
                    &source[child.byte_range()]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                );
                continue;
            }
            if matches!(child.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if !test_only {
                nodes.push(child);
            }
            test_only = false;
        }
    }
    violations.into_iter().collect()
}
fn permitted_provider_selection(relative: &str, function: &str) -> bool {
    // Parsing/composition, credential policy and public response presentation
    // may select a provider. Consumers do not implement parallel wire protocols.
    match relative {
        "g5-scm/scm_provider.rs" => matches!(function, "from_name" | "name"),
        "g5-api-core/auth/oauth.rs" => matches!(function, "from_str"),
        "g5-api-core/services/identity/auth_service.rs" => function == "connect_scm_pat",
        "g5-api-core/services/git/workspace_pull_request_provider_lifecycle.rs" => matches!(
            function,
            "prepare_merged_target_import"
                | "refresh_after_push"
                | "load_provider_candidate"
                | "merge_provider_pull_requests"
                | "capture_provider_merge"
                | "execute_provider_merges"
        ),
        "g5-api-core/services/git/workspace_pull_request_checks.rs" => {
            function == "read_checks_inner"
        }
        // Parsing the tracker an issue-domain request names.
        "g5-api-core/services/git/external_issues.rs" => function == "parse",
        "g5d/cli/pr.rs" => matches!(function, "select_output" | "render"),
        "g5d/cli/pr_json.rs" => function == "field_value",
        // A fixed `github` path segment of the PR conversation routes, not a
        // provider choice.
        "g5-api-core/middleware/auth_middleware.rs" => {
            function == "is_workspace_incarnation_internal_route"
        }
        _ => false,
    }
}
#[test]
fn provider_dispatch_is_confined_to_selected_owners() {
    let mut violations = Vec::new();
    for crate_name in backend_crates() {
        let root = generation_dir().join(&crate_name).join("src");
        for entry in WalkDir::new(&root) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
                violations.extend(provider_dispatch_violations(
                    &format!(
                        "{crate_name}/{}",
                        entry.path().strip_prefix(&root).unwrap().display()
                    ),
                    &fs::read_to_string(entry.path()).unwrap(),
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
#[test]
fn provider_dispatch_guard_accepts_composition_and_rejects_consumer_protocol_choice() {
    for source in [
        "fn read(provider: &str) { if provider == \"github\" { github_read(); } else { gitlab_read(); } }",
        "fn read(provider: &str) { match provider { \"github\" => github_read(), _ => gitlab_read() } }",
        "fn read(provider: Option<&str>) { if provider == Some(\"github\") { github_read(); } }",
    ] { assert!(!provider_dispatch_violations("new-crate/reader.rs", source).is_empty()); }
    for source in [
        "fn read() { selected.read_pull_request(1,false); }",
        "fn presentation() { output(\"github\"); }",
        "#[cfg(test)] mod tests {fn fixture() {if provider == \"github\" {setup();}}}",
    ] {
        assert!(provider_dispatch_violations("new-crate/reader.rs", source).is_empty());
    }
    let composition =
        "fn from_name(name: &str) { match name { \"github\" => Github, _ => Gitlab } }";
    assert!(provider_dispatch_violations("g5-scm/scm_provider.rs", composition).is_empty());
    assert!(!provider_dispatch_violations(
        "g5-scm/scm_provider.rs",
        &composition.replace("from_name", "new_wire_helper")
    )
    .is_empty());
}
