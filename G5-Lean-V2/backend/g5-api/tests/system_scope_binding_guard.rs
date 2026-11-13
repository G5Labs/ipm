//! System scope is not authority to act on an arbitrary review workspace.
use crate::source_scan::{
    any_excludes_production, item_attributes, parse_rust, production_sources, string_value,
};
use std::path::PathBuf;
use tree_sitter::Node;

fn visit(node: Node<'_>, source: &str, file: &str, findings: &mut Vec<String>) {
    if any_excludes_production(&item_attributes(node, source)) {
        return;
    }
    if node.kind() == "function_item" {
        let name = node
            .child_by_field_name("name")
            .unwrap()
            .utf8_text(source.as_bytes())
            .unwrap();
        let body = node.child_by_field_name("body").unwrap();
        let mut calls = Vec::new();
        let mut literals = Vec::new();
        collect(body, source, &mut calls, &mut literals);
        if calls.iter().any(|(_, call)| {
            call.ends_with("scoped_db_for_system_org")
                || call.ends_with("scoped_db_for_system_org_with")
        }) {
            if literals.iter().any(|literal| literal == "*") {
                findings.push(format!("{file}::{name}: platform wildcard in route scope"));
            }
            let text = body.utf8_text(source.as_bytes()).unwrap();
            let compact: String = text.split_whitespace().collect();
            let justified = match (file, name) {
                // Only provisioned Base ids; these helpers cannot mint review
                // authority. Their ordinary system scope is denied by RLS.
                ("routes/orgs.rs", "bootstrap_base_docs_repo" | "bootstrap_base_code_repo")
                | ("routes/auth.rs", "provision_test_token") => true,
                // Verified callback state selects a connection attempt, not a workspace.
                ("routes/github_app.rs", "pending_attempt") => true,
                // A transport filter only reads hidden-ref names, rolls back,
                // and passes them to the native protocol builder.
                ("routes/git.rs", "read_protocol_for_actor") => {
                    compact.contains("formal_review_runtime_scope(workspace)")
                        && compact.contains("scoped_db_for_system_org(state,org,&[&scope])")
                        && compact.contains(".hidden_git_refs(")
                        && calls.iter().any(|(_, call)| call.ends_with("rollback"))
                        && !calls.iter().any(|(_, call)| matches!(call.as_str(), "execute" | "commit"))
                }
                ("routes/internal.rs", "scoped_db_for_workspace_incarnation") => {
                    compact.contains("formal_review_runtime_scope(claims.workspace_id)")
                        && compact.contains("bound_scopes.push(&binding)")
                        && compact.contains("scoped_db_for_system_org(state,claims.org_id,scopes)")
                        && binding_before_write(&calls)
                }
                ("routes/internal.rs", "update_doc_content") => {
                    compact.contains("formal_review_runtime_scope(incarnation.workspace_id)")
                        && compact.contains("scoped_db_for_system_org(&state,incarnation.org_id,&[\"docs:write\",&binding])")
                        && binding_before_write(&calls)
                }
                ("routes/internal.rs", "save_source_control_internal") => {
                    let position = |name: &str| calls.iter().find(|(_, call)| call.rsplit("::").next() == Some(name)).map(|(pos, _)| *pos);
                    compact.contains("formal_review_runtime_scope(claims.workspace_id)")
                        && compact.contains("scoped_db_for_system_org(&state,claims.org_id,&[\"workspaces:write\",&binding])")
                        && matches!(
                            (position("lock_source_control_checkpoint"), position("require_live_workspace_incarnation"), position("publish_checkpoint")),
                            (Some(lock), Some(proof), Some(publish)) if lock < proof && proof < publish
                        )
                        && binding_before_write(&calls)
                }
                // Email writes system-only delivery data, not workspace children.
                // Its service locks and proves the exact live incarnation before
                // queuing. Keep this exception tied to that owner operation.
                ("routes/internal.rs", "email_owner_internal") => {
                    compact.contains("ifworkspace_id!=incarnation.workspace_id")
                        && compact.contains("&state.system_pool,incarnation.org_id,&[\"workspaces:read\"]")
                        && compact.contains("claims:&incarnation")
                        && !calls.iter().any(|(_, call)| matches!(call.as_str(), "execute" | "upsert" | "create" | "delete" | "update"))
                        && calls.iter().find(|(_, call)| call == "request_owner_email")
                            .is_some_and(|(proof, _)| calls.iter().find(|(_, call)| call == "commit").is_some_and(|(commit, _)| proof < commit))
                }
                // Approval-request reviewer email writes system-only delivery
                // data, not workspace children. The owner was already proven on
                // the preceding tenant transaction (ensure_workspace_owner plus
                // the service's owner check); org_id is resolved from that
                // request. This post-commit step reads with a read-only scope,
                // makes no workspace write, and queues the mail before commit.
                ("routes/workspace_approvals.rs", "notify_requested_reviewers") => {
                    compact.contains("&state.system_pool,org_id,&[\"workspaces:read\"]")
                        && !calls.iter().any(|(_, call)| matches!(call.as_str(), "execute" | "upsert" | "create" | "delete" | "update"))
                        && calls.iter().find(|(_, call)| call.ends_with("request_approval_request_emails"))
                            .is_some_and(|(proof, _)| calls.iter().find(|(_, call)| call == "commit").is_some_and(|(commit, _)| proof < commit))
                }
                // Approval-decision owner email writes system-only delivery
                // data, not workspace children. The decision was already
                // admitted on the preceding tenant transaction, which resolved
                // org_id; this post-commit step reads with a read-only scope,
                // makes no workspace write, and queues the mail before commit.
                ("routes/workspace_approvals.rs", "notify_workspace_owner_of_decision") => {
                    compact.contains("&state.system_pool,org_id,&[\"workspaces:read\"]")
                        && !calls.iter().any(|(_, call)| matches!(call.as_str(), "execute" | "upsert" | "create" | "delete" | "update"))
                        && calls.iter().find(|(_, call)| call.ends_with("request_approval_decision_email"))
                            .is_some_and(|(proof, _)| calls.iter().find(|(_, call)| call == "commit").is_some_and(|(commit, _)| proof < commit))
                }
                _ => false,
            };
            if !justified {
                findings.push(format!("{file}::{name}: unproved system workspace binding"));
            }
        }
        return;
    }
    for child in node.named_children(&mut node.walk()) {
        visit(child, source, file, findings);
    }
}

fn collect(
    node: Node<'_>,
    source: &str,
    calls: &mut Vec<(usize, String)>,
    literals: &mut Vec<String>,
) {
    if matches!(node.kind(), "string_literal" | "raw_string_literal") {
        literals.push(string_value(node.utf8_text(source.as_bytes()).unwrap()));
    }
    if node.kind() == "call_expression" {
        if let Some(function) = node.child_by_field_name("function") {
            // Method names, rather than their receiver's text, identify sinks.
            let function = function.child_by_field_name("field").unwrap_or(function);
            calls.push((
                node.start_byte(),
                function.utf8_text(source.as_bytes()).unwrap().to_string(),
            ));
        }
    }
    for child in node.named_children(&mut node.walk()) {
        collect(child, source, calls, literals);
    }
}

fn binding_before_write(calls: &[(usize, String)]) -> bool {
    let proof = calls
        .iter()
        .filter(|(_, call)| {
            matches!(
                call.as_str(),
                "require_live_workspace_incarnation"
                    | "require_current_workspace_incarnation"
                    | "current_incarnation_repo_target"
                    | "require_incarnation_ref_updates"
                    | "scoped_db_for_workspace_incarnation"
            )
        })
        .map(|(pos, _)| *pos)
        .min();
    let write = calls
        .iter()
        .filter(|(_, call)| {
            matches!(
                call.as_str(),
                "execute" | "commit" | "upsert" | "create" | "delete" | "update"
            )
        })
        .map(|(pos, _)| *pos)
        .min();
    proof.is_some_and(|proof| write.is_none_or(|write| proof < write))
}

#[test]
fn route_system_scopes_keep_their_workspace_bindings() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../g5-api-routes/src");
    let mut findings = Vec::new();
    for (path, source, tree) in production_sources(std::slice::from_ref(&root)) {
        let file = path.strip_prefix(&root).unwrap().to_str().unwrap();
        visit(tree.root_node(), &source, file, &mut findings);
    }
    assert!(findings.is_empty(), "{findings:#?}");
    for source in [
        "async fn unbound() { let db = scoped_db_for_system_org(state, org, scopes).await?; write(db).await?; }",
        "async fn unbound() { let db = scoped_db_for_system_org_with(authz, pool, org, scopes).await?; write(db).await?; }",
        "async fn unbound() { let db = scoped_db_for_system_org(state, org, &[\"*\"]).await?; }",
        "async fn scoped_db_for_workspace_incarnation() { let binding = formal_review_runtime_scope(claims.workspace_id); bound_scopes.push(&binding); let db = scoped_db_for_system_org(state, claims.org_id, scopes).await?; db.execute(write).await?; }",
    ] {
        let mut findings = Vec::new();
        visit(parse_rust(source).root_node(), source, "routes/internal.rs", &mut findings);
        assert!(!findings.is_empty(), "synthetic missing proof must fail: {source}");
    }
    // Removing the new route's path/token proof must fail the inventory too.
    let source = std::fs::read_to_string(root.join("routes/internal.rs")).unwrap();
    let unbound = source.replace("if workspace_id != incarnation.workspace_id", "if false");
    let mut findings = Vec::new();
    visit(
        parse_rust(&unbound).root_node(),
        &unbound,
        "routes/internal.rs",
        &mut findings,
    );
    assert!(findings
        .iter()
        .any(|finding| finding.contains("email_owner_internal")));
    // Checkpoint publication needs its final incarnation fence after the
    // write lock, even though initial Save admission was already authorized.
    for unbound in [
        source.replace(
            "require_live_workspace_incarnation(&mut db, workspace, &claims).await?;",
            "",
        ),
        source.replace(
            ".lock_source_control_checkpoint(&mut db, workspace)",
            ".source_control_generation(&mut db, workspace)",
        ),
    ] {
        let mut findings = Vec::new();
        visit(
            parse_rust(&unbound).root_node(),
            &unbound,
            "routes/internal.rs",
            &mut findings,
        );
        assert!(findings
            .iter()
            .any(|finding| finding.contains("save_source_control_internal")));
    }
}
