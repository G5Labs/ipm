//! Structural guards for the Files/Docs/Ontology current-reader boundary and
//! workspace open-question capture/materialize seam.
//!
//! Committed snapshot selection is covered by workspace-reader unit tests.
//! These assertions make the transaction cut and projection-table exclusion
//! explicit so a later refactor cannot silently move repository I/O back
//! under a scoped DB transaction or bypass the attached-owner file facade.

fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let tree = super::source_scan::parse_rust(source);
    assert!(!tree.root_node().has_error(), "invalid Rust source");
    let mut cursor = tree.root_node().walk();
    for item in tree.root_node().named_children(&mut cursor) {
        if item.kind() == "function_item"
            && item
                .child_by_field_name("name")
                .is_some_and(|node| &source[node.byte_range()] == name)
        {
            let body = item.child_by_field_name("body").expect("function body");
            return &source[body.byte_range()];
        }
    }
    panic!("missing function {name}");
}

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source
        .find(start)
        .unwrap_or_else(|| panic!("missing {start}"));
    let tail = &source[start_index..];
    let end_index = tail.find(end).unwrap_or_else(|| panic!("missing {end}"));
    &tail[..end_index]
}

fn assert_before(source: &str, first: &str, second: &str) {
    let first_index = source
        .find(first)
        .unwrap_or_else(|| panic!("missing first marker {first}"));
    let second_index = source
        .find(second)
        .unwrap_or_else(|| panic!("missing second marker {second}"));
    assert!(
        first_index < second_index,
        "expected {first:?} before {second:?}"
    );
}

fn without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn current_reader_routes_close_capture_transactions_before_repository_io() {
    let comparison = include_str!("../../g5-api-routes/src/routes/source_control_change_graph.rs");
    let baseline = function_body(comparison, "get_ontology_baseline");
    assert_before(
        baseline,
        ".ontology_comparison_repositories(",
        "db.commit().await",
    );
    assert_before(baseline, "db.commit().await", ".ontology_baseline(");
    let files = include_str!("../../g5-api-routes/src/routes/files.rs");
    // The person's route and the workspace agent share one listing body.
    let file_list = function_body(files, "read_file_tree_as");
    assert!(function_body(files, "list_file_tree").contains("read_file_tree_for_actor("));
    assert!(function_body(files, "read_file_tree_for_actor").contains("read_file_tree_as("));
    assert_before(file_list, "db.commit().await", ".committed_tracked_paths(");
    assert_before(file_list, "db.commit().await", "list_live_file_tree(");
    assert_before(file_list, "db.commit().await", "committed_file_tree(");
    let committed_list = function_body(files, "committed_file_tree");
    assert!(committed_list.contains(".list_effective_file_tree(&read"));
    assert!(!committed_list.contains("scoped_db"));
    assert!(!committed_list.contains("db.conn()"));

    let docs = include_str!("../../g5-api-routes/src/routes/docs.rs");
    let doc_list = section(docs, "async fn list_docs(", "async fn get_doc(");
    assert_before(
        doc_list,
        "db.commit().await",
        ".list_docs_with_live_overlay(captured, live)",
    );
    let doc_content = section(docs, "async fn get_doc_by_path(", "async fn create_doc(");
    assert_before(
        doc_content,
        "db.commit().await",
        ".get_doc_by_path(captured, query.path, live)",
    );
    let doc_delete = section(
        docs,
        "async fn delete_project_doc(",
        "async fn list_views_handler(",
    );
    assert_before(
        doc_delete,
        "db.commit().await",
        "delete_workspace_file_for_actor(",
    );
    let doc_update = section(
        docs,
        "async fn update_project_doc(",
        "async fn delete_project_doc(",
    );
    assert_before(
        doc_update,
        "db.commit().await",
        "write_workspace_file_and_maybe_push(",
    );

    let workspaces = include_str!("../../g5-api-routes/src/routes/workspaces.rs");
    let open_question_metrics = section(
        workspaces,
        "async fn get_open_question_metrics(",
        "async fn get_workspace(",
    );
    assert_before(
        open_question_metrics,
        ".capture_open_question_metrics(",
        "db.commit().await",
    );
    assert_before(
        open_question_metrics,
        "db.commit().await",
        ".open_question_metrics(captured, &state.doc_service, &state.workspace_runtime)",
    );
}

#[test]
fn docs_current_lists_have_no_projection_read_path() {
    let docs = include_str!("../../g5-api-core/src/services/workspace/doc_service.rs");
    let doc_list = section(
        docs,
        "pub async fn list_docs(",
        "/// Get a single doc by ID",
    );
    assert!(!doc_list.contains("FROM docs"));
    assert!(!doc_list.contains("db.conn()"));
    let compact_doc_list = without_whitespace(doc_list);
    assert!(compact_doc_list.contains("repo_view.list_trees"));
    assert!(compact_doc_list.contains("repo_view.read_files"));
    let doc_content = section(
        docs,
        "pub async fn get_doc_by_path(",
        "fn create_doc_lock_key(",
    );
    assert!(!doc_content.contains("FROM docs"));
    assert!(!doc_content.contains("db.conn()"));
    assert!(doc_content.contains("read_effective_doc_content"));
    assert!(!docs.contains("capture_project_doc_mutation"));
    assert!(!docs.contains("update_project_doc_from_capture"));
    assert!(!docs.contains("delete_project_doc_from_capture"));

    let routes = include_str!("../../g5-api-routes/src/routes/docs.rs");
    let doc_update = section(
        routes,
        "async fn update_project_doc(",
        "async fn delete_project_doc(",
    );
    assert!(doc_update.contains("write_workspace_file_and_maybe_push("));
    let workspaces = include_str!("../../g5-api-routes/src/routes/workspaces.rs");
    let save_helper = section(
        workspaces,
        "pub(crate) async fn write_workspace_file_and_maybe_push(",
        "/// Commit only these files",
    );
    assert!(save_helper.contains("write_workspace_file_and_maybe_push_as("));
    assert!(save_helper.contains("write_workspace_file_as("));
    // The explicit Save is the Workspace owner's one operation, shared with
    // the question service (`g5.architecture.question_owner`).
    assert!(save_helper.contains("workspace_file_save::save_live_file("));
    let doc_delete = section(
        routes,
        "async fn delete_project_doc(",
        "async fn list_views_handler(",
    );
    assert!(doc_delete.contains("delete_workspace_file_for_actor("));
    let open_question_sources = section(
        docs,
        "pub async fn open_project_doc_question_sources(",
        "/// Get a single doc by ID",
    );
    assert!(!open_question_sources.contains("db.conn()"));
    assert!(!open_question_sources.contains("sqlx::"));
    assert!(!open_question_sources.contains("active_doc_surface"));
    let compact_open_question_sources = without_whitespace(open_question_sources);
    assert!(compact_open_question_sources.contains("repo_view.list_trees"));
    assert!(compact_open_question_sources.contains("repo_view.read_files"));
}

#[test]
fn open_question_metrics_materialization_holds_no_tenant_db_scope() {
    let service = include_str!("../../g5-api-core/src/services/workspace/workspace_service.rs");
    let entrypoint = section(
        service,
        "pub async fn open_question_metrics(",
        "/// List all workspaces in an org",
    );
    assert!(!entrypoint.contains("db.conn()"));
    assert!(!entrypoint.contains("sqlx::"));
    assert!(!entrypoint.contains("ScopedDb"));
    assert!(entrypoint.contains("open_project_doc_question_families("));
    assert!(entrypoint.contains("open_question_set_sources("));

    // The question-set family reads Git and the live file only.
    let question_sets =
        include_str!("../../g5-api-core/src/services/workspace/question_set_service.rs");
    let set_scan = section(
        question_sets,
        "pub async fn open_question_set_sources(",
        "// -- Answer and clear",
    );
    assert!(!set_scan.contains("db.conn()"));
    assert!(!set_scan.contains("sqlx::"));
    assert!(!set_scan.contains("ScopedDb"));

    let committed_read = section(
        service,
        "pub(crate) async fn get_branch_file_content(",
        "pub async fn get_workspace_settings(",
    );
    assert!(!committed_read.contains("db.conn()"));
    assert!(!committed_read.contains("sqlx::"));
    assert!(!committed_read.contains("ScopedDb"));
}

#[test]
fn ontology_comparison_inventory_uses_file_authority_after_capture_commit() {
    let source = include_str!("../../g5-api-routes/src/routes/ontology_files.rs");
    let discovery = function_body(source, "current_ontology_files");
    assert_before(
        discovery,
        "resolve_committed_snapshot(",
        "db.commit().await",
    );
    assert_before(discovery, "db.commit().await", "read_file_tree_for_actor(");
    assert!(!discovery.contains("read_workspace_file"));
    assert!(!discovery.contains("ontology_nodes"));
}
