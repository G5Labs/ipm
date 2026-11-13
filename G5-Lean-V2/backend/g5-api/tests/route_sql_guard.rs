//! Raw SQL in route handlers.
//!
//! Route handlers should call domain operations, not the database. This guard
//! finds every `sqlx::query*` and `sqlx::raw_sql` path in production code under
//! `g5-api-routes/src` and requires its enclosing function to be one of the
//! named exemptions below. An exemption whose function no longer runs SQL
//! fails too, so the list only shrinks. Moving these into services is tracked
//! in `.g5/pr_ontology_diff_design/abstraction-leakiness/ontology-plan.md`.
//!
//! A call is attributed to its innermost enclosing function, qualified by
//! enclosing functions, impl types and inline modules (`outer::helper`,
//! `Handler::method`), so a nested helper or a same-name method does not
//! inherit an exemption.

use crate::source_scan;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Tree};

/// Route functions (file relative to `g5-api-routes/src`, qualified function)
/// that still run SQL directly.
const ROUTE_SQL_EXEMPT: &[(&str, &str)] = &[
    // Smart-HTTP auth, ref policy and receive-pack admission interleave git
    // subprocess execution with repository and workspace lookups at the
    // protocol edge.
    ("routes/git.rs", "acquire_push_admission"),
    ("routes/git.rs", "repo_workspace_targets"),
    ("routes/git.rs", "require_incarnation_ref_updates"),
    ("routes/git.rs", "GitAuth::require_push_ref_updates"),
    ("routes/git.rs", "user_has_workspace_access"),
    // Health check performs a trivial SELECT 1.
    ("routes/instance.rs", "health_check"),
    // Workspace-incarnation endpoints combine service calls with direct SQL for
    // task, session, attachment, document operations.
    ("routes/internal.rs", "download_chat_attachment_content"),
    ("routes/internal.rs", "require_session_in_workspace"),
    ("routes/internal.rs", "update_doc_content"),
    // Capacity stop checks that the workspace exists in the organization.
    ("routes/worker_hosts.rs", "capacity_stop_workspace"),
    // Workspace reads behind route decisions (settings, Base check, clone
    // organization), and the push-mode settings write.
    ("routes/workspaces.rs", "clone_workspace"),
    (
        "routes/workspaces.rs",
        "ensure_workspace_file_mutation_allowed",
    ),
    ("routes/workspaces.rs", "get_workspace"),
    ("routes/workspaces.rs", "set_workspace_push_mode"),
];

/// One direct SQL path in production code.
#[derive(Debug, PartialEq, Eq)]
struct SqlCall {
    function: String,
    line: usize,
}

fn is_sql_path(text: &str) -> bool {
    let compact: String = text.split_whitespace().collect();
    let path = compact.trim_start_matches("::");
    path.starts_with("sqlx::query") || path.starts_with("sqlx::raw_sql")
}

/// Leaf tokens of a macro's token tree in order, skipping string literals and
/// comments, with their lines.
fn token_tree_tokens<'a>(node: Node<'_>, source: &'a str, tokens: &mut Vec<(&'a str, usize)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string_literal" | "raw_string_literal" | "char_literal" | "line_comment"
            | "block_comment" => {}
            _ if child.child_count() == 0 => tokens.push((
                child.utf8_text(source.as_bytes()).unwrap_or_default(),
                child.start_position().row + 1,
            )),
            _ => token_tree_tokens(child, source, tokens),
        }
    }
}

/// Every `sqlx::query*` or `sqlx::raw_sql` path in production code of one
/// file, with its qualified enclosing function, including paths inside macro
/// arguments such as `tokio::try_join!`. A `use` of those functions, or a
/// glob import of `sqlx`, is reported as `(use)`: a short name would hide
/// later calls.
fn sql_calls(source: &str, tree: &Tree) -> Vec<SqlCall> {
    fn visit(node: Node<'_>, source: &str, scope: &mut Vec<String>, calls: &mut Vec<SqlCall>) {
        if source_scan::any_excludes_production(&source_scan::item_attributes(node, source)) {
            return;
        }
        let text = || node.utf8_text(source.as_bytes()).unwrap_or_default();
        let function = |scope: &[String]| {
            if scope.is_empty() {
                "(module)".to_string()
            } else {
                scope.join("::")
            }
        };
        match node.kind() {
            "scoped_identifier" if is_sql_path(text()) => {
                calls.push(SqlCall {
                    function: function(scope),
                    line: node.start_position().row + 1,
                });
                return;
            }
            "token_tree" => {
                let mut tokens = Vec::new();
                token_tree_tokens(node, source, &mut tokens);
                for window in tokens.windows(3) {
                    if window[0].0 == "sqlx"
                        && window[1].0 == "::"
                        && (window[2].0.starts_with("query") || window[2].0 == "raw_sql")
                    {
                        calls.push(SqlCall {
                            function: function(scope),
                            line: window[0].1,
                        });
                    }
                }
                return;
            }
            "use_declaration" => {
                let compact: String = text().split_whitespace().collect();
                if compact.contains("sqlx::query")
                    || compact.contains("sqlx::raw_sql")
                    || compact.contains("sqlx::*")
                    || (compact.contains("sqlx::{")
                        && (compact.contains("query") || compact.contains("raw_sql")))
                {
                    calls.push(SqlCall {
                        function: "(use)".to_string(),
                        line: node.start_position().row + 1,
                    });
                }
                return;
            }
            _ => {}
        }
        let name = match node.kind() {
            "function_item" | "mod_item" => node
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source.as_bytes()).ok()),
            "impl_item" => node
                .child_by_field_name("type")
                .and_then(|name| name.utf8_text(source.as_bytes()).ok()),
            _ => None,
        };
        if let Some(name) = name {
            scope.push(name.to_string());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, scope, calls);
        }
        if name.is_some() {
            scope.pop();
        }
    }
    let mut calls = Vec::new();
    visit(tree.root_node(), source, &mut Vec::new(), &mut calls);
    calls
}

#[test]
fn route_sql_is_limited_to_named_functions() {
    let routes_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api lives directly under backend")
        .join("g5-api-routes/src");
    let exempt: BTreeSet<(String, String)> = ROUTE_SQL_EXEMPT
        .iter()
        .map(|(file, function)| (file.to_string(), function.to_string()))
        .collect();
    assert_eq!(exempt.len(), ROUTE_SQL_EXEMPT.len(), "duplicate exemption");

    let mut violations = Vec::new();
    let mut used = BTreeSet::new();
    let mut per_function = BTreeMap::<(String, String), usize>::new();
    for (path, source, tree) in source_scan::production_sources(std::slice::from_ref(&routes_src)) {
        let relative: PathBuf = path.strip_prefix(&routes_src).expect("route source").into();
        let file = relative.to_string_lossy().replace('\\', "/");
        for call in sql_calls(&source, &tree) {
            let key = (file.clone(), call.function.clone());
            if exempt.contains(&key) {
                *per_function.entry(key.clone()).or_default() += 1;
                used.insert(key);
            } else {
                violations.push(format!("  {file}:{} ({})", call.line, call.function));
            }
        }
    }
    let stale: Vec<String> = exempt
        .difference(&used)
        .map(|(file, function)| format!("  {file}::{function}"))
        .collect();

    assert!(
        violations.is_empty() && stale.is_empty(),
        "Route handlers must call domain operations instead of running SQL.\n\
         Move the query into the owning service. If a route genuinely needs it, \
         add (file, function) to ROUTE_SQL_EXEMPT in {} with a comment explaining why.\n\n\
         Violations:\n{}\n\n\
         Stale exemptions (the function no longer runs SQL; remove them):\n{}\n",
        file!(),
        if violations.is_empty() {
            "  none".to_string()
        } else {
            violations.join("\n")
        },
        if stale.is_empty() {
            "  none".to_string()
        } else {
            stale.join("\n")
        },
    );
    eprintln!(
        "route SQL: {} calls in {} exempt functions",
        per_function.values().sum::<usize>(),
        per_function.len()
    );
}

#[test]
fn sql_calls_are_attributed_to_their_enclosing_function() {
    let source = r##"
        use sqlx::Row;
        use sqlx::{query_scalar, Postgres};
        async fn exempt(db: &mut Db) {
            let _ = sqlx::query("SELECT 1");
            fn helper() {
                let _ = sqlx::query_as::<_, (i64,)>("SELECT 2");
            }
            let _ = sqlx::query!("SELECT 3");
        }
        async fn other() {
            fn exempt() {}
            let _ = sqlx::query_scalar::<_, i64>("SELECT 4");
            let text = "sqlx::query(\"not code\")";
            // sqlx::query("comment")
        }
        impl Handler {
            fn exempt(&self) {
                let _ = sqlx::raw_sql("SELECT 5");
            }
        }
        async fn joined(db: &Db) {
            let _ = tokio::try_join!(sqlx::query("SELECT 7").fetch_one(db), ::sqlx::query_scalar::<_, i64>("SELECT 8").fetch_one(db));
            assert!(sqlx::query!("SELECT 9").fetch_one(db).await.is_ok(), "sqlx::query(\"text\")");
            let _ = ::sqlx::query("SELECT 10");
        }
        use sqlx::*;
        #[cfg(test)]
        mod tests {
            fn fixture() {
                let _ = sqlx::query("SELECT 6");
                let _ = tokio::join!(sqlx::query("SELECT 11"));
            }
        }
    "##;
    let tree = source_scan::parse_rust(source);
    let calls: Vec<(String, usize)> = sql_calls(source, &tree)
        .into_iter()
        .map(|call| (call.function, call.line))
        .collect();
    assert_eq!(
        calls,
        [
            ("(use)".to_string(), 3),
            ("exempt".to_string(), 5),
            ("exempt::helper".to_string(), 7),
            ("exempt".to_string(), 9),
            ("other".to_string(), 13),
            ("Handler::exempt".to_string(), 19),
            ("joined".to_string(), 23),
            ("joined".to_string(), 23),
            ("joined".to_string(), 24),
            ("joined".to_string(), 25),
            ("(use)".to_string(), 27),
        ]
    );
}
