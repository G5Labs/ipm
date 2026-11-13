use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("read source directory");
    for entry in entries {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|ext| ext == std::ffi::OsStr::new("rs"))
        {
            files.push(path);
        }
    }
}

// This guard checks individual literals, not SQL assembled across literals or
// format! arguments. It deliberately avoids combining unrelated adjacent queries;
// dynamically assembled query semantics still need review and runtime coverage.
fn branch_query_literals(source: &str) -> Vec<(usize, &str)> {
    matching_literals(source, &["workspace_branches", "workspace_branch_repos"])
}

fn matching_literals<'a>(source: &'a str, patterns: &[&str]) -> Vec<(usize, &'a str)> {
    fn visit<'a>(
        node: Node<'_>,
        source: &'a str,
        patterns: &[&str],
        found: &mut Vec<(usize, &'a str)>,
    ) {
        let mut previous = node.prev_named_sibling();
        while let Some(attribute) = previous {
            if attribute.kind() != "attribute_item" {
                break;
            }
            if attribute
                .utf8_text(source.as_bytes())
                .unwrap()
                .contains("cfg(test)")
            {
                return;
            }
            previous = attribute.prev_named_sibling();
        }
        if matches!(node.kind(), "string_literal" | "raw_string_literal") {
            let literal = node.utf8_text(source.as_bytes()).expect("UTF-8 literal");
            // A module path or tracing target (`g5_api::services::remote_integrations`)
            // names code, never storage: the legacy tracing-target table spells
            // the owner module without touching its tables or refs.
            let is_code_path = literal.contains("::");
            if !is_code_path && patterns.iter().any(|table| literal.contains(table)) {
                found.push((node.start_position().row + 1, literal));
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, patterns, found);
        }
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Rust grammar");
    let tree = parser.parse(source, None).expect("parse Rust source");
    let mut found = Vec::new();
    visit(tree.root_node(), source, patterns, &mut found);
    found
}

#[test]
fn production_queries_do_not_reference_retired_branch_tables() {
    let mut files = Vec::new();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api must live directly under the generation workspace");
    for crate_name in [
        "g5-api-foundation",
        "g5-api-data",
        "g5-api-core",
        "g5-api-routes",
        "g5-api",
    ] {
        collect_rust_files(&workspace_root.join(crate_name).join("src"), &mut files);
    }

    let mut offenders = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).expect("read Rust source file");
        for (line, literal) in branch_query_literals(&content) {
            offenders.push(format!("{}:{line}\n{literal}", path.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "Production SQL must use canonical workspace/repository membership; retired branch tables cannot be queried.\n\n{}",
        offenders.join("\n\n---\n\n")
    );
}

#[test]
fn branch_query_guard_keeps_adjacent_sql_separate_and_checks_raw_literals() {
    let source = r##"
        fn queries() {
            let update = "UPDATE workspace_branches SET head_commit=$1 WHERE id=$2";
            let queue = "SELECT applied_oid FROM projection_state ORDER BY applied_at LIMIT 1";
            let forbidden = r#"SELECT id AS "id!" FROM workspace_branches LIMIT 1"#;
        }
    "##;
    let queries = branch_query_literals(source);
    assert_eq!(queries.len(), 2);
    assert!(queries[0].1.contains("UPDATE workspace_branches"));
    assert!(queries[1].1.contains("SELECT id"));
}

#[test]
fn remote_integration_schema_and_git_names_do_not_escape_the_owner() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let owner = backend.join("g5-api-core/src/services/git/remote_integrations");
    let owner_entry = owner.with_extension("rs");
    let mut files = Vec::new();
    for name in ["g5-api-core", "g5-api-routes", "g5-api"] {
        collect_rust_files(&backend.join(name).join("src"), &mut files);
    }
    let mut violations = Vec::new();
    for path in files {
        if path == owner_entry || path.starts_with(&owner) {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        for (line, literal) in matching_literals(
            &source,
            &[
                "remote_integrations",
                "remote_integration_queue",
                "remote_ref_leases",
                "refs/g5-tracking/",
            ],
        ) {
            violations.push(format!("{}:{line}: {literal}", path.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "Remote callers must use integration operations and views, not private storage: {}",
        violations.join("\n")
    );
}
