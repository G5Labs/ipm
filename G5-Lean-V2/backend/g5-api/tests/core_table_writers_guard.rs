//! Declared writers of the core platform tables.
//!
//! Any module holding a database connection can run SQL, so Rust visibility
//! cannot keep a table's writes with its owner. This guard reads every SQL
//! string literal in production code under `backend/*/src`, finds `INSERT
//! INTO`, `UPDATE … SET`, `DELETE FROM`, `MERGE INTO` and `TRUNCATE`
//! statements against the core tables, and compares the files that write each
//! table with [`CORE_TABLE_WRITERS`]. A write into a table that another module
//! owns is an [`Role::Exception`] that names its reason; ownership work shows
//! its progress by deleting exceptions.
//!
//! Ontology: `g5.architecture.commands.writers` in
//! `.g5/ontology/software_architecture/boundaries.g5.md`. The exceptions follow
//! `.g5/pr_ontology_diff_design/abstraction-leakiness/ontology-plan.md`.
//!
//! The scan decodes each literal, blanks SQL comments and quoted values, and
//! skips code that only compiles in tests. Limits: SQL assembled from several
//! literals or with `format!` arguments, and SQL functions defined in
//! migrations, are not attributed to a writer.

use crate::source_scan;

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tree_sitter::Node;

/// Why a file writes a core table.
#[derive(Clone, Copy, Debug)]
enum Role {
    /// The file belongs to the table's owner, or writes columns the ontology
    /// assigns to the file's owner.
    Owner(&'static str),
    /// A write into a table that another module owns.
    Exception(&'static str),
}

use Role::{Exception, Owner};

/// The files (relative to `backend/`) allowed to write each core table.
const CORE_TABLE_WRITERS: &[(&str, &[(&str, Role)])] = &[
    (
        "workspace_vms",
        &[
            (
                "g5-api-core/src/services/workspace/workspace_vm_service.rs",
                Owner("Workspace lifecycle"),
            ),
            (
                "g5-api-core/src/services/hosts/workspace_vm_release.rs",
                Owner("Workspace lifecycle: every VM release"),
            ),
            (
                "g5-api-core/src/services/hosts/worker_host_service.rs",
                Owner("placement, boot binding and placement release"),
            ),
            (
                "g5-api-core/src/services/hosts/host_capacity.rs",
                Owner("managed-capacity wait diagnostics; does not change Start consent"),
            ),
            (
                "g5-api-core/src/services/hosts/host_control_state.rs",
                Owner("host event and lease facts"),
            ),
            (
                "g5-api-core/src/services/agents/task_lifecycle.rs",
                Owner("preview intent columns (g5.preview.service)"),
            ),
            (
                "g5-api-core/src/services/git/workspace_flush_service.rs",
                Owner("flush receipt and flush block columns"),
            ),
            (
                "g5-api-core/src/services/workspace/workspace_runtime.rs",
                Owner("owner activity time"),
            ),
            (
                "g5-api-core/src/services/workspace/workspace_service.rs",
                Owner("registry delete"),
            ),
            (
                "g5-api-core/src/services/workspace/workspace_fork_service.rs",
                Owner("checked fork inserts the child row"),
            ),
        ],
    ),
    (
        "workspaces",
        &[
            (
                "g5-api-core/src/services/workspace/workspace_service.rs",
                Owner("workspace registry"),
            ),
            (
                "g5-api-core/src/services/workspace/workspace_fork_service.rs",
                Owner("checked fork creates the child workspace"),
            ),
            (
                "g5-api-core/src/services/workspace/workspace_vm_service.rs",
                Owner("lifecycle generation and ownership transfer"),
            ),
            (
                "g5-api-core/src/services/identity/org_service.rs",
                Owner("organization creation writes its Base workspace (g5.team.onboarding)"),
            ),
            (
                "g5-api-core/src/services/workspace/personal_workspace.rs",
                Owner("personal workspace creation and retirement (g5.personal.lifecycle)"),
            ),
            (
                "g5-api-routes/src/routes/workspaces.rs",
                Exception("push-mode and base auto-pull settings routes write the row directly"),
            ),
        ],
    ),
    (
        "worker_hosts",
        &[
            (
                "g5-api-core/src/services/hosts/worker_host_service.rs",
                Owner("host registry and placement"),
            ),
            (
                "g5-api-core/src/services/hosts/host_capacity.rs",
                Owner("managed host claims, idle evidence, retirement and cloud cleanup"),
            ),
            (
                "g5-api-core/src/services/hosts/host_control_state.rs",
                Owner("host control facts"),
            ),
            (
                "g5-api-core/src/services/hosts/workspace_vm_release.rs",
                Owner("retracts a settled, never-admitted incarnation from reported inventory"),
            ),
            (
                "g5-api-core/src/cloud/rotation.rs",
                Owner("cloud host rotation"),
            ),
            (
                "g5-api-core/src/cloud/stale_host.rs",
                Owner("stale cloud host cleanup"),
            ),
        ],
    ),
    (
        "tasks",
        &[
            (
                "g5-api-core/src/services/agents/task_lifecycle.rs",
                Owner("TaskService"),
            ),
            (
                "g5-api-core/src/services/agents/agent_dispatch.rs",
                Owner("TaskService dispatch and inbox"),
            ),
            (
                "g5-api-core/src/services/identity/spend_service.rs",
                Owner("SpendService usage counters"),
            ),
            (
                "g5-api-core/src/services/identity/usage_aggregation.rs",
                Owner("SpendService usage counter repair"),
            ),
        ],
    ),
    (
        "task_attempts",
        &[
            (
                "g5-api-core/src/services/agents/task_lifecycle.rs",
                Owner("TaskService"),
            ),
            (
                "g5-api-core/src/services/agents/agent_dispatch.rs",
                Owner("TaskService dispatch"),
            ),
            (
                "g5-api-core/src/services/agents/codex_auth.rs",
                Owner("TaskService Codex authorization"),
            ),
            (
                "g5-api-core/src/services/agents/agent_tasks.rs",
                Owner("TaskService coordinator"),
            ),
        ],
    ),
    ("task_dependencies", &[]),
    (
        "session_inputs",
        &[
            (
                "g5-api-core/src/services/agents/agent_dispatch.rs",
                Owner("TaskService inbox"),
            ),
            (
                "g5-api-core/src/services/agents/agent_steering.rs",
                Owner("TaskService immutable steering delivery"),
            ),
            (
                "g5-api-core/src/services/agents/task_lifecycle.rs",
                Owner("TaskService inbox delivery"),
            ),
        ],
    ),
    (
        "chat_sessions",
        &[
            (
                "g5-api-core/src/services/agents/chat_service.rs",
                Owner("ChatService"),
            ),
            (
                "g5-api-core/src/services/identity/spend_service.rs",
                Owner("SpendService usage counters"),
            ),
            (
                "g5-api-core/src/services/identity/usage_aggregation.rs",
                Owner("SpendService usage counter repair"),
            ),
        ],
    ),
    (
        "chat_messages",
        &[(
            "g5-api-core/src/services/agents/chat_service.rs",
            Owner("ChatService"),
        )],
    ),
    (
        "chat_attachments",
        &[
            (
                "g5-api-core/src/services/agents/chat_service.rs",
                Owner("ChatService"),
            ),
            (
                "g5-api-core/src/services/agents/agent_attachment_retention.rs",
                Owner("ChatService attachment retention"),
            ),
        ],
    ),
    (
        "usage_records",
        &[(
            "g5-api-core/src/services/identity/spend_service.rs",
            Owner("SpendService"),
        )],
    ),
];

/// One write statement found in a production string literal.
#[derive(Debug, PartialEq, Eq)]
struct Write {
    table: String,
    line: usize,
}

/// Matches the statement keyword and target of a write. TRUNCATE targets are
/// split separately because they form a list.
fn write_statement() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?ix)
            \b(?:insert\s+into|delete\s+from|merge\s+into)\s+(?:only\s+)?
              (?:"?public"?\s*\.\s*)?"?([a-z_][a-z0-9_]*)"?
            | \bupdate\s+(?:only\s+)?(?:"?public"?\s*\.\s*)?"?([a-z_][a-z0-9_]*)"?
              (?:\s+(?:as\s+)?"?[a-z_][a-z0-9_]*"?)?\s+set\b
            | \btruncate\s+(?:table\s+)?([^;]*)"#,
        )
        .expect("write statement pattern")
    })
}

fn schema_dot() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\s*\.\s*").expect("qualified name pattern"))
}

/// SQL with comments and single-quoted values blanked, so only statement
/// tokens remain. Block comments nest, as in PostgreSQL.
fn sql_tokens(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut tokens = String::with_capacity(sql.len());
    let mut index = 0;
    while index < chars.len() {
        match (chars[index], chars.get(index + 1)) {
            ('-', Some('-')) => {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
                tokens.push(' ');
            }
            ('/', Some('*')) => {
                let mut depth = 0;
                while index < chars.len() {
                    if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                        depth += 1;
                        index += 2;
                    } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                        depth -= 1;
                        index += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
                tokens.push(' ');
            }
            ('\'', _) => {
                // An E'...' value takes backslash escapes, including \'.
                let escape_string = index > 0
                    && matches!(chars[index - 1], 'E' | 'e')
                    && (index < 2
                        || !(chars[index - 2].is_alphanumeric() || chars[index - 2] == '_'));
                index += 1;
                while index < chars.len() {
                    if escape_string && chars[index] == '\\' {
                        index += 2;
                        continue;
                    }
                    if chars[index] == '\'' {
                        if chars.get(index + 1) == Some(&'\'') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                tokens.push(' ');
            }
            (c, _) => {
                tokens.push(c);
                index += 1;
            }
        }
    }
    tokens
}

/// Tables written by the SQL in one string value.
fn written_tables(sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    for captures in write_statement().captures_iter(&sql_tokens(sql)) {
        if let Some(table) = captures.get(1).or_else(|| captures.get(2)) {
            tables.push(table.as_str().to_ascii_lowercase());
            continue;
        }
        let Some(targets) = captures.get(3) else {
            continue;
        };
        let targets = schema_dot().replace_all(targets.as_str(), ".");
        for target in targets.split(',') {
            let name = target
                .split_whitespace()
                .find(|word| !word.eq_ignore_ascii_case("only"))
                .unwrap_or_default()
                .trim_end_matches('*')
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .trim_matches('"')
                .to_ascii_lowercase();
            if !name.is_empty() {
                tables.push(name);
            }
        }
    }
    tables
}

/// Write statements in the production string literals of one file.
fn writes_in(source: &str, tree: &tree_sitter::Tree) -> Vec<Write> {
    fn visit(node: Node<'_>, source: &str, writes: &mut Vec<Write>) {
        if source_scan::any_excludes_production(&source_scan::item_attributes(node, source)) {
            return;
        }
        if matches!(node.kind(), "string_literal" | "raw_string_literal") {
            let literal = node.utf8_text(source.as_bytes()).expect("UTF-8 literal");
            for table in written_tables(&source_scan::string_value(literal)) {
                writes.push(Write {
                    table,
                    line: node.start_position().row + 1,
                });
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, writes);
        }
    }
    let mut writes = Vec::new();
    visit(tree.root_node(), source, &mut writes);
    writes
}

/// True when a crate manifest lists `sqlx` among its normal dependencies,
/// including target-specific ones: only those crates can write the platform
/// database.
fn depends_on_sqlx(manifest: &str) -> bool {
    let mut in_dependencies = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            let header: String = line.split_whitespace().collect();
            let table = header.trim_start_matches('[').trim_end_matches(']');
            if table == "dependencies.sqlx" || table.ends_with(".dependencies.sqlx") {
                return true;
            }
            in_dependencies = table == "dependencies" || table.ends_with(".dependencies");
            continue;
        }
        if !in_dependencies {
            continue;
        }
        let key = line
            .split(['=', '.'])
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches('"');
        if key == "sqlx" {
            return true;
        }
    }
    false
}

fn backend_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api lives directly under backend")
        .to_path_buf()
}

/// Every core-table write in production code, keyed by table and file.
fn production_writes(backend: &Path) -> BTreeMap<(String, String), Vec<usize>> {
    let mut roots = Vec::new();
    for entry in fs::read_dir(backend).expect("read backend directory") {
        let crate_root = entry.expect("read backend entry").path();
        let Ok(manifest) = fs::read_to_string(crate_root.join("Cargo.toml")) else {
            continue;
        };
        if depends_on_sqlx(&manifest) && crate_root.join("src").is_dir() {
            roots.push(crate_root.join("src"));
        }
    }
    assert!(
        roots.iter().any(|root| root.ends_with("g5-api-core/src")),
        "the scan must include the API service crate"
    );

    let core_tables: BTreeSet<&str> = CORE_TABLE_WRITERS.iter().map(|(table, _)| *table).collect();
    let mut writes = BTreeMap::<(String, String), Vec<usize>>::new();
    for (path, source, tree) in source_scan::production_sources(&roots) {
        let relative = path
            .strip_prefix(backend)
            .expect("source under backend")
            .to_string_lossy()
            .replace('\\', "/");
        for write in writes_in(&source, &tree) {
            if core_tables.contains(write.table.as_str()) {
                writes
                    .entry((write.table, relative.clone()))
                    .or_default()
                    .push(write.line);
            }
        }
    }
    writes
}

#[test]
fn core_tables_have_only_their_declared_writers() {
    let backend = backend_root();
    let found = production_writes(&backend);

    let mut declared = BTreeMap::new();
    for (table, writers) in CORE_TABLE_WRITERS {
        for (file, role) in *writers {
            let previous = declared.insert((table.to_string(), file.to_string()), *role);
            assert!(previous.is_none(), "{table} lists {file} twice");
            let (Owner(reason) | Exception(reason)) = role;
            assert!(
                !reason.trim().is_empty(),
                "{table} entry for {file} needs a reason"
            );
        }
    }

    let unlisted: Vec<String> = found
        .iter()
        .filter(|(key, _)| !declared.contains_key(*key))
        .map(|((table, file), lines)| format!("  {table}: {file} (lines {lines:?})"))
        .collect();
    let stale: Vec<String> = declared
        .iter()
        .filter(|(key, _)| !found.contains_key(*key))
        .map(|((table, file), role)| format!("  {table}: {file} ({role:?})"))
        .collect();

    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "Core table writers differ from CORE_TABLE_WRITERS in {}.\n\n\
         Unlisted writers. Call the owner's operation instead; if the write is \
         genuinely owned by this file, add it with the owner it belongs to:\n{}\n\n\
         Stale entries (the file no longer writes the table; remove them):\n{}\n",
        file!(),
        if unlisted.is_empty() {
            "  none".to_string()
        } else {
            unlisted.join("\n")
        },
        if stale.is_empty() {
            "  none".to_string()
        } else {
            stale.join("\n")
        },
    );

    let exceptions = CORE_TABLE_WRITERS
        .iter()
        .flat_map(|(_, writers)| writers.iter())
        .filter(|(_, role)| matches!(role, Exception(_)))
        .count();
    eprintln!(
        "core table writers: {} declared, {exceptions} cross-owner exceptions",
        declared.len()
    );
}

#[test]
fn write_scanner_reads_statement_shapes_and_skips_test_code() {
    let source = r###"
        fn production() {
            let update = "UPDATE tasks SET name = $1 WHERE id = $2";
            let escaped = "UPDATE \"jobs\" SET status = $1";
            let commented = "UPDATE /* audit */ chat_sessions SET title = $1";
            let quoted_alias = r#"UPDATE chat_attachments AS "a" SET message_id = $1"#;
            let composed = r#"WITH moved AS (UPDATE chat_messages AS message SET content = $1)
                INSERT INTO usage_records(id) VALUES ($1)"#;
            let delete = "DELETE FROM public.task_attempts WHERE id = $1";
            let continued = "UPDATE \
                workspace_vms SET status = 'stopped'";
            let truncate = "TRUNCATE TABLE ONLY session_inputs, public.worker_hosts RESTART IDENTITY";
            let lock = "SELECT id FROM tasks FOR UPDATE OF tasks";
            let prose = "failed to update tasks";
            let value = "SELECT 'UPDATE tasks SET name = 1' -- UPDATE jobs SET x = 1";
            let upsert = "INSERT INTO workspaces(id) VALUES ($1)
                ON CONFLICT (id) DO UPDATE SET name = excluded.name";
        }
        #[cfg(test)]
        mod tests {
            fn fixture() {
                let ignored = "UPDATE workspaces SET name = $1";
            }
        }
        #[cfg(any(test, feature = "test-helpers"))]
        fn helper() {
            let ignored = "DELETE FROM chat_sessions";
        }
        #[cfg(test)]
        /// Documented test helper.
        fn documented_helper() {
            let ignored = "INSERT INTO jobs(id) VALUES ($1)";
        }
        #[cfg(any(test, unix))]
        fn unix_production() {
            let kept = "DELETE FROM task_dependencies";
        }
    "###;
    let tree = source_scan::parse_rust(source);
    let tables: Vec<String> = writes_in(source, &tree)
        .into_iter()
        .map(|write| write.table)
        .collect();
    assert_eq!(
        tables,
        [
            "tasks",
            "jobs",
            "chat_sessions",
            "chat_attachments",
            "chat_messages",
            "usage_records",
            "task_attempts",
            "workspace_vms",
            "session_inputs",
            "worker_hosts",
            "workspaces",
            "task_dependencies"
        ]
    );
}

#[test]
fn escape_string_values_and_spaced_qualified_names_are_read() {
    assert_eq!(
        written_tables(
            r"UPDATE chat_sessions SET title = E'it\'s' WHERE id = $1; UPDATE tasks SET name = $2"
        ),
        ["chat_sessions", "tasks"]
    );
    assert_eq!(
        written_tables(r#"TRUNCATE public . tasks, ONLY "jobs""#),
        ["tasks", "jobs"]
    );
}

#[test]
fn database_crates_are_found_from_their_manifests() {
    for (manifest, expected) in [
        ("[dependencies]\nsqlx = { workspace = true }\n", true),
        ("[dependencies]\nsqlx\t= { workspace = true }\n", true),
        ("[dependencies] # runtime\nsqlx.workspace = true\n", true),
        ("[dependencies.sqlx]\nworkspace = true\n", true),
        ("[target.'cfg(unix)'.dependencies]\nsqlx = \"0.8\"\n", true),
        (
            "[dependencies]\nserde = \"1\"\n\n[dev-dependencies]\nsqlx = { workspace = true }\n",
            false,
        ),
        ("[dependencies]\nsqlx-core = \"0.8\"\n", false),
        ("[dependencies]\n# sqlx = \"0.8\"\n", false),
    ] {
        assert_eq!(depends_on_sqlx(manifest), expected, "{manifest:?}");
    }
}
