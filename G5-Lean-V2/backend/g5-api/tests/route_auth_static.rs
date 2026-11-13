use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    file: String,
    handler: String,
    arguments: Vec<String>,
    proof: String,
    guard_tokens: Vec<String>,
    #[serde(default)]
    guard_precedes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownAuthGap {
    file: String,
    handler: String,
    argument: String,
    normalized_identifier: String,
    required_scope: String,
    issue: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteAuthFinding {
    file: String,
    handler: String,
    argument: String,
    normalized_identifier: String,
    required_scope: String,
    issue: String,
}

impl From<&KnownAuthGap> for RouteAuthFinding {
    fn from(gap: &KnownAuthGap) -> Self {
        RouteAuthFinding {
            file: gap.file.clone(),
            handler: gap.handler.clone(),
            argument: gap.argument.clone(),
            normalized_identifier: gap.normalized_identifier.clone(),
            required_scope: gap.required_scope.clone(),
            issue: gap.issue.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct DiscoveredArgument {
    file: String,
    handler: String,
    argument: String,
}

#[derive(Debug, Clone)]
struct RouteHandler {
    file: String,
    handler: String,
    method: String,
    route_kind: RouteRegistrationKind,
    registered_scopes: Vec<String>,
    exemption_reason: Option<String>,
    route_params: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum RouteRegistrationKind {
    Audited,
    Exempt,
    Raw,
}

#[derive(Debug, Clone)]
struct StructField {
    name: String,
    ty: String,
}

#[derive(Debug, Clone, Copy)]
struct AuthContract {
    scope: &'static str,
    guard_tokens: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
struct RouteScopeContract {
    registration: RouteRegistrationRequirement,
    scopes: &'static [&'static str],
    reason: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RouteRegistrationRequirement {
    Audited,
    Exempt,
    Raw,
}

#[test]
fn route_secondary_identifiers_have_auth_manifest_entries() {
    let repo_root = repo_root();
    let routes_dir = repo_root.join("g5-api-routes/src/routes");
    let manifest_path = repo_root
        .parent()
        .expect("backend has repository parent")
        .join("tests/fixtures/security/route_auth_manifest.json");

    let manifest: Vec<ManifestEntry> =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap_or_else(|err| {
            panic!(
                "failed to read route auth manifest at {}: {err}",
                manifest_path.display()
            )
        }))
        .expect("route auth manifest must be valid JSON");
    let known_gaps_path = repo_root
        .parent()
        .expect("backend has repository parent")
        .join("tests/fixtures/security/route_auth_known_gaps.json");
    let known_gaps = load_known_auth_gaps(&known_gaps_path);

    let mut discovered = BTreeSet::new();
    let mut handler_sources = BTreeMap::<(String, String), String>::new();
    let mut registered_handlers = BTreeSet::<(String, String)>::new();

    for path in route_source_files(&routes_dir) {
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let source = strip_line_comments(&raw);
        let rel_file = format!(
            "routes/{}",
            path.file_name()
                .expect("route file has file name")
                .to_string_lossy()
        );
        let structs = parse_structs(&source);
        let route_handlers = discover_route_handlers(&source, &rel_file);

        for route_handler in route_handlers {
            registered_handlers.insert((route_handler.file.clone(), route_handler.handler.clone()));
            let Some((signature, body)) = extract_async_fn(&source, &route_handler.handler) else {
                continue;
            };
            handler_sources.insert(
                (route_handler.file.clone(), route_handler.handler.clone()),
                format!("{signature}\n{body}"),
            );

            for route_param in &route_handler.route_params {
                if is_sensitive_identifier(route_param, "") {
                    discovered.insert(DiscoveredArgument {
                        file: route_handler.file.clone(),
                        handler: route_handler.handler.clone(),
                        argument: format!("route.{route_param}"),
                    });
                }
            }

            for (extractor, type_name) in extractor_types(signature) {
                let Some(fields) = structs.get(&type_name) else {
                    continue;
                };
                for field in fields {
                    if is_sensitive_identifier(&field.name, &field.ty) {
                        discovered.insert(DiscoveredArgument {
                            file: route_handler.file.clone(),
                            handler: route_handler.handler.clone(),
                            argument: format!("{extractor}.{}", field.name),
                        });
                    }
                }
            }
        }
    }
    let discovered = collapse_route_params_covered_by_path_extractors(discovered);

    let mut manifest_args = BTreeSet::new();
    let mut failures = Vec::new();
    let mut findings = BTreeSet::new();

    for entry in &manifest {
        if entry.proof.trim().is_empty() {
            failures.push(format!(
                "{}::{} has an empty proof",
                entry.file, entry.handler
            ));
        }
        if entry.guard_tokens.is_empty() {
            failures.push(format!(
                "{}::{} must name at least one guard token",
                entry.file, entry.handler
            ));
        }
        if !registered_handlers.contains(&(entry.file.clone(), entry.handler.clone())) {
            failures.push(format!(
                "manifest entry references non-route handler {}::{}",
                entry.file, entry.handler
            ));
        }

        let Some(handler_source) =
            handler_sources.get(&(entry.file.clone(), entry.handler.clone()))
        else {
            continue;
        };
        if !entry
            .guard_tokens
            .iter()
            .all(|token| handler_source.contains(token.as_str()))
        {
            failures.push(format!(
                "{}::{} proof guard tokens not found in handler body: {:?}",
                entry.file, entry.handler, entry.guard_tokens
            ));
        }
        for sink in &entry.guard_precedes {
            if !handler_source.contains(sink.as_str()) {
                failures.push(format!(
                    "{}::{} proof sink `{sink}` no longer appears in the handler; point the \
                     manifest at the protected call",
                    entry.file, entry.handler
                ));
                continue;
            }
            if !each_token_precedes_all(
                handler_source,
                &entry.guard_tokens,
                std::slice::from_ref(sink),
            ) {
                failures.push(format!(
                    "{}::{} proof guard tokens {:?} must precede `{sink}`",
                    entry.file, entry.handler, entry.guard_tokens
                ));
            }
        }

        for argument in &entry.arguments {
            manifest_args.insert(DiscoveredArgument {
                file: entry.file.clone(),
                handler: entry.handler.clone(),
                argument: argument.clone(),
            });
        }
    }

    let mut args_by_handler = BTreeMap::<(String, String), Vec<String>>::new();
    for arg in &discovered {
        args_by_handler
            .entry((arg.file.clone(), arg.handler.clone()))
            .or_default()
            .push(arg.argument.clone());
    }
    for ((file, handler), arguments) in args_by_handler {
        let Some(handler_source) = handler_sources.get(&(file.clone(), handler.clone())) else {
            continue;
        };
        for argument in arguments {
            if !argument_is_used_in_handler(handler_source, &file, &handler, &argument) {
                continue;
            }

            let Some(contract) = auth_contract_for_argument(&file, &handler, &argument) else {
                findings.insert(RouteAuthFinding {
                    file: file.clone(),
                    handler: handler.clone(),
                    argument: argument.clone(),
                    normalized_identifier: normalized_argument_name(&file, &handler, &argument),
                    required_scope: "unclassified".to_string(),
                    issue: "unclassified_secondary_identifier".to_string(),
                });
                continue;
            };

            let service_level_boundary =
                service_level_review_projection_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_comment_identity_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_invitation_user_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_member_change_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_change_graph_node_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_project_doc_mutation_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_comment_repo_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_worker_task_transition_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_open_question_metrics_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_pull_request_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_chat_session_file_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_session_input_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_native_reply_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_incarnation_route_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_file_facade_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_lifecycle_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_preview_repo_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_workspace_fork_input_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_approval_review_admission_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_terminal_reattach_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_preview_reauth_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                ) || service_level_background_end_guard_exists(
                    handler_source,
                    &file,
                    &handler,
                    &argument,
                );

            if !contract
                .guard_tokens
                .iter()
                .any(|token| handler_source.contains(token))
                && !service_level_boundary
            {
                findings.insert(RouteAuthFinding {
                    file: file.clone(),
                    handler: handler.clone(),
                    argument: argument.clone(),
                    normalized_identifier: normalized_argument_name(&file, &handler, &argument),
                    required_scope: contract.scope.to_string(),
                    issue: "missing_handler_auth_guard".to_string(),
                });
                continue;
            }

            if !service_level_boundary
                && !covering_auth_check_exists(
                    handler_source,
                    contract.guard_tokens,
                    protected_sink_tokens(),
                    &file,
                    &handler,
                    &argument,
                    AuthCheckPosition::Anywhere,
                )
            {
                findings.insert(RouteAuthFinding {
                    file: file.clone(),
                    handler: handler.clone(),
                    argument: argument.clone(),
                    normalized_identifier: normalized_argument_name(&file, &handler, &argument),
                    required_scope: contract.scope.to_string(),
                    issue: "guard_does_not_cover_identifier".to_string(),
                });
                continue;
            }

            if !service_level_boundary
                && !covering_auth_check_exists(
                    handler_source,
                    contract.guard_tokens,
                    protected_sink_tokens(),
                    &file,
                    &handler,
                    &argument,
                    AuthCheckPosition::BeforeFirstProtectedSink,
                )
            {
                findings.insert(RouteAuthFinding {
                    file: file.clone(),
                    handler: handler.clone(),
                    argument: argument.clone(),
                    normalized_identifier: normalized_argument_name(&file, &handler, &argument),
                    required_scope: contract.scope.to_string(),
                    issue: "auth_guard_after_protected_work".to_string(),
                });
            }
        }
    }

    let stale: Vec<_> = manifest_args.difference(&discovered).cloned().collect();
    if !stale.is_empty() {
        failures.push(format!(
            "stale manifest arguments no longer discovered:\n{}",
            stale
                .iter()
                .map(|arg| format!("  {}::{} {}", arg.file, arg.handler, arg.argument))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let regenerate_known_gaps = std::env::var_os("G5_ROUTE_AUTH_WRITE_KNOWN_GAPS").is_some();
    if regenerate_known_gaps {
        write_known_auth_gaps(&known_gaps_path, &findings);
    }

    if !regenerate_known_gaps {
        let known_finding_keys = known_gaps
            .iter()
            .map(RouteAuthFinding::from)
            .collect::<BTreeSet<_>>();
        let unexpected = findings
            .difference(&known_finding_keys)
            .cloned()
            .collect::<Vec<_>>();
        let stale_known_gaps = known_finding_keys
            .difference(&findings)
            .cloned()
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            failures.push(format!(
                "unexpected route authorization findings; fix the route or update the explicit security backlog:\n{}",
                format_findings(&unexpected)
            ));
        }
        if !stale_known_gaps.is_empty() {
            failures.push(format!(
                "stale route authorization backlog entries; remove them from {}:\n{}",
                known_gaps_path.display(),
                format_findings(&stale_known_gaps)
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "route secondary identifier authorization audit failed:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn shared_member_change_retains_actor_path_and_current_authority() {
    let route = "change_member(&state, &auth, &params, org_service::MemberChange::Remove).await?;";
    let check = |body: &str| {
        service_level_member_change_guard_exists(
            body,
            "routes/orgs.rs",
            "remove_member",
            "path.user_id",
        )
    };
    assert!(check(route));
    assert!(!check(&route.replace("&auth", "other_actor")));
    assert!(!check(&route.replace("&params", "other_membership")));
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-routes/src/routes/orgs.rs"),
        "change_member",
        &[
            "scoped_system_db_for_membership_transition(",
            "ensure_org_user_id(",
        ],
        &[".change_member("],
    );
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-core/src/db/scoped.rs"),
        "scoped_system_db_for_membership_transition",
        &["lock_membership_transition(", "authorize_org_scopes("],
        &["Ok(db)"],
    );
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-core/src/services/identity/org_service.rs"),
        "change_member",
        &["db.tx().begin()"],
        &[".change_member_on_conn("],
    );
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-core/src/services/identity/org_service.rs"),
        "change_member_on_conn",
        &[
            "lock_membership_transition(",
            "require_org_admin_on_conn(",
            "require_admin_continuity(",
        ],
        &[
            ".retire_personal_workspace(",
            ".update_member_role(",
            ".set_member_active(",
            ".remove_member(",
        ],
    );
}

#[test]
fn shared_pr_create_retains_typed_path_and_service_authority() {
    let route = "workspace_pull_request_create::create_or_update(&state, PullRequestCaller::User(&auth), params.id, body).await?;";
    let check = |body: &str| {
        service_level_workspace_pull_request_guard_exists(
            body,
            "routes/workspaces.rs",
            "create_workspace_pull_request",
            "path.id",
        )
    };
    assert!(check(route));
    assert!(!check(&route.replace("params.id", "untrusted_id")));
    assert!(!check(
        &route.replace("PullRequestCaller::User(&auth)", "other_caller")
    ));
    let service = repo_root().join("g5-api-core/src/services/git/workspace_pull_request_create.rs");
    assert_function_contains_all_in_order(
        &service,
        "create_or_update",
        &[".authorized_db(state, source_workspace_id, target_workspace_id)"],
        &[".prepare_pull_request_source("],
    );
    assert_function_contains_all_in_order(
        &service,
        "authorized_db",
        &[
            ".source_db_for_target",
            "ensure_workspace_mutable",
            "ensure_user_workspace_access",
        ],
        &["Ok(db)"],
    );
    assert_function_contains_all_in_order(
        &service,
        "source_db_for_target",
        &[
            "Self::User(auth)",
            "scoped_db_for_workspace(state, auth, source",
            "\"workspaces:write\"",
        ],
        &["Ok(db)"],
    );
}

#[test]
fn high_risk_token_and_websocket_paths_bind_secondary_ids() {
    let repo_root = repo_root();

    assert_function_contains_all_in_order(
        &repo_root.join("g5-api-core/src/services/identity/auth_service.rs"),
        "mint_ws_ticket",
        &["ensure_user_workspace_access"],
        &["mint_ws_jwt("],
    );
    assert_function_contains_all_in_order(
        &repo_root.join("g5-api-core/src/services/identity/auth_service.rs"),
        "approve_cli_device_authorization",
        &["ensure_user_workspace_access"],
        &["approved_at"],
    );
}

#[test]
fn review_edit_audit_requires_nested_parent_and_child_bindings() {
    let root = repo_root();
    let routes = strip_line_comments(
        &fs::read_to_string(root.join("g5-api-routes/src/routes/workspace_reviews.rs")).unwrap(),
    );
    let service = strip_line_comments(
        &fs::read_to_string(
            root.join("g5-api-core/src/services/workspace/workspace_review_service.rs"),
        )
        .unwrap(),
    );
    for (file, handler, argument) in [
        (
            "routes/workspace_reviews.rs",
            "list_replies",
            "query.edit_id",
        ),
        ("routes/workspace_reviews.rs", "add_reply", "body.edit_id"),
        (
            "routes/workspace_reviews.rs",
            "reject_suggestion",
            "body.edit_id",
        ),
    ] {
        let (_, source) = extract_async_fn(&routes, handler).unwrap();
        assert_eq!(
            auth_contract_for_argument(file, handler, argument)
                .unwrap()
                .scope,
            "review edit within workspace-bound parent ledger"
        );
        let proves = |source: &str, routes: &str, service: &str| {
            review_edit_binding_exists(source, file, handler, routes, service)
        };
        assert!(
            proves(source, &routes, &service),
            "{handler} has its actual nested proof"
        );
        let contract = auth_contract_for_argument(file, handler, argument).unwrap();
        assert!(
            !covering_auth_check_exists(
                &source.replace("params.suggestion_id", "unbound_parent"),
                contract.guard_tokens,
                protected_sink_tokens(),
                file,
                handler,
                argument,
                AuthCheckPosition::Anywhere,
            ),
            "{handler} cannot fall back to mere guard-token presence"
        );
        assert!(
            !proves(
                &source.replace("params.suggestion_id", "unbound_parent"),
                &routes,
                &service
            ),
            "{handler} must forward its exact parent"
        );
        assert!(
            !proves(
                &source.replace(argument, "unbound_child"),
                &routes,
                &service
            ),
            "{handler} must forward its exact child"
        );
        assert!(
            !proves(
                &source.replace("workspace_id", "unbound_workspace"),
                &routes,
                &service
            ),
            "{handler} must forward its exact workspace"
        );
        assert!(
            !proves(
                source,
                &routes,
                &service.replace("p.source_workspace_id = $1 AND", "")
            ),
            "{handler} must join its parent to the workspace"
        );
        assert!(
            !proves(
                source,
                &routes,
                &service.replace(".bind(suggestion_id)", ".bind(unbound_parent)")
            ),
            "{handler} must bind its exact parent in SQL"
        );
        let child_binding = if handler == "list_replies" {
            ".find(|e| e.id == id)"
        } else {
            ".position(|e| e.id == edit_id)"
        };
        assert!(
            !proves(source, &routes, &service.replace(child_binding, ".next()")),
            "{handler} must select its child from the loaded parent"
        );
        if handler != "list_replies" {
            assert!(
                !proves(
                    source,
                    &routes.replace(
                        "lock_suggestion(&mut db, workspace_id, suggestion_id)",
                        "lock_suggestion(&mut db, workspace_id, unbound_parent)"
                    ),
                    &service
                ),
                "{handler} must lock the same parent in the shared boundary"
            );
        }
        if handler == "reject_suggestion" {
            assert!(
                !proves(
                    source,
                    &routes.replace("edit_id.is_none_or(|id| id == e.id)", "true"),
                    &service
                ),
                "{handler} must constrain the selected rejection ledger"
            );
        }
    }
    assert!(auth_contract_for_argument(
        "routes/workspace_reviews.rs",
        "unrelated_handler",
        "body.edit_id"
    )
    .is_none());
}

#[test]
fn comment_db_preserves_the_workspace_authorization_boundary() {
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-routes/src/routes/external_issues.rs"),
        "comment_db",
        &[
            "scoped_db_for_workspace",
            "WORKSPACE_READ_SCOPE",
            "workspace_org_id",
            "authorize_org_scopes",
            "WORKSPACE_WRITE_SCOPE",
        ],
        &["Ok(db)"],
    );
}

#[test]
fn merge_override_authority_is_decided_in_one_place() {
    // g5.workspace.merge.admin-override: who may override is one rule that
    // the routes and transactional rechecks share.
    let root = repo_root();
    let routes = root.join("g5-api-routes/src/routes/workspace_merge_override.rs");
    assert_function_contains_all_in_order(
        &routes,
        "authority_for",
        &[
            "require_browser_session(",
            ".target_db(",
            ".pull_request_source_for_target(",
            "current_merge_override_authority(",
        ],
        &["merge_override_authority("],
    );
    assert_function_contains_all_in_order(
        &root.join("g5-api-core/src/services/git/merge_override.rs"),
        "current_merge_override_authority",
        &[
            "require_active_member_on_conn(",
            "current_scope(db, org_id, None, \"org:admin\")",
            "workspace_access_for_role_with(",
            "base_admin_subject_org(",
            "ensure_workspace_owner(",
            "source_workspace_owner_may_merge(",
        ],
        &["merge_override_authority("],
    );
    for (handler, protected) in [
        ("preview_merge_override", "override_preview("),
        ("merge_with_override", "merge_pull_request_as("),
    ] {
        assert_function_contains_all_in_order(&routes, handler, &["authority_for("], &[protected]);
    }
    let mut callers = Vec::new();
    for dir in ["g5-api-routes/src", "g5-api-core/src", "g5-api/src"] {
        for entry in walkdir::WalkDir::new(root.join(dir)) {
            let entry = entry.unwrap();
            if entry.path().extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).unwrap();
            for needle in ["merge_override_authority(", "require_browser_session("] {
                if text.contains(needle) {
                    callers.push(format!(
                        "{} {needle}",
                        entry.path().strip_prefix(&root).unwrap().display()
                    ));
                }
            }
        }
    }
    callers.sort();
    assert_eq!(
        callers,
        vec![
            "g5-api-core/src/middleware/auth_middleware.rs require_browser_session(",
            "g5-api-core/src/services/git/merge_override.rs merge_override_authority(",
            "g5-api-routes/src/routes/workspace_merge_override.rs merge_override_authority(",
            "g5-api-routes/src/routes/workspace_merge_override.rs require_browser_session(",
        ],
        "override authority stays in its shared policy and browser entry points"
    );
}

#[test]
fn owner_or_base_admin_admission_keeps_owner_and_base_admin_checks() {
    let workspaces_rs = repo_root().join("g5-api-routes/src/routes/workspaces.rs");
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "scoped_db_for_owner_or_base_admin",
        &[
            "scoped_db_for_workspace",
            "\"workspaces:write\"",
            "ensure_workspace_owner",
            "scoped_db_for_base_admin",
        ],
        &["Err(owner_refusal)"],
    );
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "scoped_db_for_base_admin",
        &[
            "Scope::OrgAdmin",
            "scoped_db_for_workspace",
            "\"org:admin\"",
        ],
        &["admit_base_admin("],
    );
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "admit_base_admin",
        &["base_admin_subject_org", "require_org_admin_on_conn"],
        &["Ok(db)"],
    );
    // Merge when ready re-admits its author with the same two checks
    // (`g5.git.pr-auto-merge`).
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "target_db",
        &[
            "Self::WhenReady",
            "\"workspaces:write\"",
            "ensure_workspace_owner",
            "\"org:admin\"",
        ],
        &["admit_base_admin("],
    );
    // The subject query lives with the other mutation-policy checks.
    assert_function_contains_all_in_order(
        &repo_root().join("g5-api-core/src/services/identity/workspace_mutation_policy.rs"),
        "base_admin_subject_org",
        &[
            "workspace.is_base",
            "workspace.deleted_at IS NULL",
            "pull_request.target_workspace_id = workspace.id",
        ],
        &["fetch_optional"],
    );
    // The source-owner merge path (g5.governance.pr.source-owner-merge, #1104)
    // tries the owner/Base-admin admission first, then admits the pull
    // request's source owner on an org-scoped write capability (not the
    // target-owner fence), returning the same owner refusal otherwise.
    {
        let source = fs::read_to_string(&workspaces_rs).unwrap();
        let body = extract_named_fn_body(&source, "scoped_db_for_source_owner_merge")
            .expect("scoped_db_for_source_owner_merge exists");
        for token in [
            "scoped_db_for_owner_or_base_admin",
            "scoped_db_for_user_actor_in_org",
            "source_owner_may_merge",
            "is_source_owner_of_pull_request",
            "owner_refusal",
        ] {
            assert!(
                body.contains(token),
                "scoped_db_for_source_owner_merge must reference `{token}`"
            );
        }
    }
    // The Person merge path asks the source-owner admission too.
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "target_db",
        &["Self::Person", "scoped_db_for_source_owner_merge("],
        &["Self::WorkspaceAgent"],
    );
}

#[test]
fn remote_tracking_publication_handlers_require_workspace_write_scope() {
    let repo_root = repo_root();
    let workspaces_rs = repo_root.join("g5-api-routes/src/routes/workspaces.rs");

    let source = fs::read_to_string(&workspaces_rs).unwrap();
    let body =
        extract_named_fn_body(&source, "push_to_remote").expect("publication handler exists");
    assert!(required_git_guards_precede_effect(
        body,
        &[
            "scoped_db_for_workspace",
            "\"workspaces:write\"",
            "ensure_workspace_owner"
        ],
        ".workspace_service.push_to_remote(&mut db, params.id)"
    ));
    let guards = [
        "scoped_db_for_workspace",
        "\"workspaces:write\"",
        "ensure_workspace_owner",
    ];
    let effect = ".workspace_service.push_to_remote(&mut db, params.id)";
    assert!(required_git_guards_precede_effect(
        &body.replace(
            ".push_to_remote(&mut db, params.id)",
            ".push_to_remote(\n &mut db,\n params.id,\n)"
        ),
        &guards,
        effect
    ));
    assert!(!required_git_guards_precede_effect(
        &body.replace(
            ".push_to_remote(&mut db, params.id)",
            "/* .push_to_remote(&mut db, params.id) */"
        ),
        &guards,
        effect
    ));
    assert!(!required_git_guards_precede_effect(
        &body.replace(
            "ensure_workspace_owner",
            "/* ensure_workspace_owner */ removed"
        ),
        &guards,
        effect
    ));
    assert_function_contains_all_in_order(
        &workspaces_rs,
        "set_workspace_push_mode",
        &["scoped_db_for_workspace", "\"workspaces:write\""],
        &["UPDATE workspaces SET push_mode"],
    );
}

fn required_git_guards_precede_effect(source: &str, guards: &[&str], effect: &str) -> bool {
    fn normalized(node: tree_sitter::Node<'_>, source: &str) -> String {
        if matches!(node.kind(), "line_comment" | "block_comment") {
            return String::new();
        }
        if node.child_count() == 0 {
            return source[node.byte_range()].to_owned();
        }
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .map(|child| normalized(child, source))
            .collect()
    }
    fn arguments(node: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
        let arguments = node.child_by_field_name("arguments").unwrap();
        let mut cursor = arguments.walk();
        arguments
            .named_children(&mut cursor)
            .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
            .map(|child| normalized(child, source))
            .collect()
    }
    fn calls<'a>(root: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
        let mut nodes = vec![root];
        let mut calls = Vec::new();
        while let Some(node) = nodes.pop() {
            if node.kind() == "call_expression" {
                calls.push(node);
            }
            let mut cursor = node.walk();
            nodes.extend(node.named_children(&mut cursor));
        }
        calls
    }
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .unwrap();
    let wrapped = format!("fn guarded() {source}");
    let Some(tree) = parser.parse(&wrapped, None) else {
        return false;
    };
    if tree.root_node().has_error() {
        return false;
    }
    let call_nodes = calls(tree.root_node());
    let expected = format!("fn expected() {{ receiver{effect}; }}");
    let Some(expected_tree) = parser.parse(&expected, None) else {
        return false;
    };
    let expected_calls = calls(expected_tree.root_node());
    let Some(expected_call) = expected_calls.first() else {
        return false;
    };
    let expected_function = normalized(
        expected_call.child_by_field_name("function").unwrap(),
        &expected,
    );
    let expected_function = expected_function.strip_prefix("receiver").unwrap();
    let expected_args = arguments(*expected_call, &expected);
    let first_effect = call_nodes
        .iter()
        .filter(|call| {
            let function = normalized(call.child_by_field_name("function").unwrap(), &wrapped);
            let args = arguments(**call, &wrapped);
            function.ends_with(expected_function) && args == expected_args
        })
        .map(|call| call.start_byte())
        .min();
    let Some(first_effect) = first_effect else {
        return false;
    };
    guards.iter().all(|guard| {
        if guard.starts_with('"') {
            // A required scope is a literal argument of the actual scoped DB
            // call, not an unrelated string or a comment elsewhere in the body.
            call_nodes.iter().any(|call| {
                let function = normalized(call.child_by_field_name("function").unwrap(), &wrapped);
                let args = call.child_by_field_name("arguments").unwrap();
                let mut nodes = vec![args];
                let mut found = false;
                while let Some(node) = nodes.pop() {
                    found |=
                        node.kind() == "string_literal" && &wrapped[node.byte_range()] == *guard;
                    let mut cursor = node.walk();
                    nodes.extend(node.named_children(&mut cursor));
                }
                function.rsplit([':', '.']).next() == Some("scoped_db_for_workspace")
                    && found
                    && call.start_byte() < first_effect
            })
        } else {
            call_nodes.iter().any(|call| {
                let function = normalized(call.child_by_field_name("function").unwrap(), &wrapped);
                let name = function.rsplit([':', '.']).next().unwrap_or("");
                name == guard.rsplit("::").next().unwrap_or(guard)
                    && call.start_byte() < first_effect
            })
        }
    })
}

#[test]
fn smart_http_receive_checks_authority_and_admission_before_scm() {
    let source = fs::read_to_string(repo_root().join("g5-api-routes/src/routes/git.rs")).unwrap();
    let body = extract_named_fn_body(&source, "receive_pack_owned").expect("push owner exists");
    let guards = [
        "GitAuth::from_headers",
        "require_read",
        "require_push",
        "require_push_ref_updates",
        "acquire_push_admission",
    ];
    let effect = ".receive_pack(&repo_path, decoded_body, &admitted)";
    assert!(required_git_guards_precede_effect(body, &guards, effect));
    assert!(!required_git_guards_precede_effect(
        &body.replace(effect, "removed_effect"),
        &guards,
        effect
    ));
    for guard in guards {
        assert!(!required_git_guards_precede_effect(
            &body.replace(guard, "removed_guard"),
            &guards,
            effect
        ));
    }
    let reordered = format!("{{ {effect}; guard(); }}").replace("{ .", "{ receiver.");
    assert!(!required_git_guards_precede_effect(
        &reordered,
        &["guard"],
        effect
    ));
    assert!(required_git_guards_precede_effect(
        &body.replace("native_protocol", "renamed_protocol_helper"),
        &guards,
        effect
    ));
    assert!(required_git_guards_precede_effect(
        &body.replace(
            effect,
            ".receive_pack(\n &repo_path,\n decoded_body,\n &admitted,\n)"
        ),
        &guards,
        effect
    ));
    assert!(!required_git_guards_precede_effect(
        &body.replace(effect, &format!("/* {effect} */")),
        &guards,
        effect
    ));
    for guard in [
        "require_read",
        "require_push",
        "require_push_ref_updates",
        "acquire_push_admission",
    ] {
        let changed = body
            .replace(&format!(".{guard}("), &format!("./* {guard}( */ removed("))
            .replace(
                &format!("= {guard}("),
                &format!("= /* {guard}( */ removed("),
            );
        assert!(
            !required_git_guards_precede_effect(&changed, &guards, effect),
            "commented {guard} must not count"
        );
    }
    assert!(!required_git_guards_precede_effect(
        "{ let decoy = \"guard()\"; receiver.effect(); }",
        &["guard"],
        ".effect()"
    ));
    assert!(!required_git_guards_precede_effect(
        "{ /* guard(); receiver.effect(); */ }",
        &["guard"],
        ".effect()"
    ));
}

#[test]
fn read_only_sensitive_mutation_handlers_are_explicitly_classified() {
    let repo_root = repo_root();

    assert_function_contains_all_in_order(
        &repo_root.join("g5-api-routes/src/routes/workspaces.rs"),
        "transition_mutation_policy",
        &[
            "ensure_workspace_allows_mutation",
            "WorkspaceMutationKind::AdminWorkspaceAdministration",
            "\"workspaces.transition_mutation_policy\"",
        ],
        &[".transition_mutation_policy("],
    );
}

#[test]
fn route_handlers_use_expected_api_scopes() {
    let repo_root = repo_root();
    let routes_dir = repo_root.join("g5-api-routes/src/routes");
    let mut failures = Vec::new();

    for path in route_source_files(&routes_dir) {
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let source = strip_line_comments(&raw);
        let rel_file = format!(
            "routes/{}",
            path.file_name()
                .expect("route file has file name")
                .to_string_lossy()
        );

        for route_handler in discover_route_handlers(&source, &rel_file) {
            let Some(contract) = api_scope_contract_for_route(&route_handler) else {
                failures.push(format!(
                    "{}::{} {} has no API scope contract",
                    route_handler.file, route_handler.handler, route_handler.method
                ));
                continue;
            };

            match contract.registration {
                RouteRegistrationRequirement::Audited => {
                    if route_handler.route_kind != RouteRegistrationKind::Audited {
                        failures.push(format!(
                            "{}::{} {} must be registered with audited_route requiring {:?} ({}) but is {}",
                            route_handler.file,
                            route_handler.handler,
                            route_handler.method,
                            contract.scopes,
                            contract.reason,
                            route_kind_label(route_handler.route_kind),
                        ));
                        continue;
                    }

                    if route_handler.registered_scopes.len() != 1
                        || !contract
                            .scopes
                            .iter()
                            .any(|expected| route_handler.registered_scopes[0] == *expected)
                    {
                        failures.push(format!(
                            "{}::{} {} registered scopes {:?}; expected one of {:?} ({})",
                            route_handler.file,
                            route_handler.handler,
                            route_handler.method,
                            route_handler.registered_scopes,
                            contract.scopes,
                            contract.reason,
                        ));
                    }
                }
                RouteRegistrationRequirement::Exempt => {
                    if route_handler.route_kind != RouteRegistrationKind::Exempt {
                        failures.push(format!(
                            "{}::{} {} must be registered with exempt_route ({}) but is {}",
                            route_handler.file,
                            route_handler.handler,
                            route_handler.method,
                            contract.reason,
                            route_kind_label(route_handler.route_kind),
                        ));
                    }
                    if route_handler
                        .exemption_reason
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .is_empty()
                    {
                        failures.push(format!(
                            "{}::{} {} has an empty auth exemption reason",
                            route_handler.file, route_handler.handler, route_handler.method
                        ));
                    }
                }
                RouteRegistrationRequirement::Raw => {
                    if route_handler.route_kind != RouteRegistrationKind::Raw {
                        failures.push(format!(
                            "{}::{} {} must be registered as a pinned raw route ({}) but is {}",
                            route_handler.file,
                            route_handler.handler,
                            route_handler.method,
                            contract.reason,
                            route_kind_label(route_handler.route_kind),
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "route API/OAuth scope audit failed:\n{}",
        failures.join("\n")
    );
}

fn api_scope_contract_for_route(route: &RouteHandler) -> Option<RouteScopeContract> {
    let file = route.file.as_str();
    let handler = route.handler.as_str();
    let method = route.method.as_str();

    let contract = match file {
        "routes/api_keys.rs" => audited(&["Scope::OrgAdmin"], "API key administration"),
        "routes/config.rs" if handler == "update_org_config" => audited(
            &["Scope::OrgAdmin"],
            "organization Runtime Config administration",
        ),
        "routes/auth.rs" => auth_route_contract(handler)?,
        "routes/chat.rs" if handler == "record_session_inspection" => audited(
            &["Scope::SessionsRead"],
            "record one's own inspection of a chat result: any reader of the chat, no content \
             write (g5.chat.inspection.acknowledge)",
        ),
        "routes/chat.rs" => {
            if is_read_method(method) && !matches!(handler, "start_session") {
                audited(&["Scope::SessionsRead"], "chat/session read")
            } else {
                audited(&["Scope::SessionsWrite"], "chat/session mutation")
            }
        }
        "routes/notifications.rs" if matches!(handler, "mark_read" | "mark_all_read") => audited(
            &["Scope::WorkspacesRead"],
            "mark one's own notifications read, one or all of a workspace's: any recipient with \
             read access, whatever their role and whoever owns the workspace; no content write \
             (g5.user.notifications)",
        ),
        "routes/comments.rs" if matches!(handler, "add_reply" | "create_comment") => audited(
            &["Scope::WorkspacesRead"],
            "workspace discussion: readers may comment on pushed content or reply; current-work anchors are owner-only and no content write is granted",
        ),
        "routes/comments.rs" if handler == "update_comment" => audited(
            &["Scope::WorkspacesRead"],
            "edit one's own thread text: any reader who authored it; resolving requires the credential's write scope and the owner inside the handler (g5.documents.comments.authority)",
        ),
        "routes/workspace_review_flow.rs" => audited(
            &["Scope::WorkspacesRead"],
            "named review projections require workspace read; live inspection additionally checks the running owner",
        ),
        "routes/source_control_browser.rs" => match handler {
            "menu" | "checkpoint" | "check" | "apply" | "pushed_scope" | "history_page" => audited(
                &["Scope::WorkspacesRead"],
                "authorized repository projections; live reads also require the running owner",
            ),
            "save" => audited(&["Scope::WorkspacesWrite"], "explicit owner Save checkpoint"),
            _ => return None,
        },
        "routes/questions.rs" if handler == "vote_question" || handler == "withdraw_question_vote" => {
            audited(
                &["Scope::WorkspacesRead"],
                "vote on a referenced question: any reader of the workspace, no content write \
                 (g5.documents.question.votes)",
            )
        }
        "routes/compute.rs" => audited(
            &["Scope::WorkspacesRead"],
            "member-visible running-compute inventory filtered to readable workspaces",
        ),
        "routes/personal.rs" => match handler {
            "get_my_personal_workspace" => audited(
                &["Scope::WorkspacesRead"],
                "personal workspace identity: the caller's own, created for an eligible member \
                 (g5.personal.routes)",
            ),
            "get_personal_workspace_by_handle" => audited(
                &["Scope::WorkspacesRead"],
                "personal workspace identity: one by handle under the workspace read rule \
                 (g5.personal.routes)",
            ),
            _ => return None,
        },
        "routes/comments.rs"
        | "routes/config.rs"
        | "routes/docs.rs"
        | "routes/explainer_templates.rs"
        | "routes/files.rs"
        | "routes/grounding.rs"
        | "routes/home.rs"
        | "routes/intent_log.rs"
        | "routes/notifications.rs"
        | "routes/ontology_changes.rs"
        | "routes/resolve.rs"
        | "routes/preview_profiles.rs"
        | "routes/questions.rs"
        | "routes/source_control_change_graph.rs"
        | "routes/workbench.rs"
        | "routes/workspace_preview.rs"
        | "routes/workspaces.rs" => workspace_route_contract(handler, method),
        "routes/provider_accounts.rs" => {
            if is_read_method(method) {
                audited(&["Scope::WorkspacesRead"], "provider-account list")
            } else {
                audited(&["Scope::WorkspacesWrite"], "provider-account mutation")
            }
        }
        "routes/github_app.rs" => match handler {
            "github_app_setup_callback"
            | "github_app_oauth_callback"
            | "github_app_reconnect_selection" => exempt(
                "GitHub App browser callback authorized by HMAC-signed state matched to its \
                 stored digest, a signed PKCE cookie and GitHub installation access",
            ),
            "get_user_connection" | "start_user_connection" | "disconnect_user_connection" => {
                exempt(
                    "the person's own account-wide GitHub connection; the handler requires a \
                     person session and acts only on that person's user id",
                )
            }
            _ => audited(
                &["Scope::OrgAdmin"],
                "GitHub App binding administration with a path-org admin re-check",
            ),
        },
        "routes/external_issues.rs" => match handler {
            "create_issue_domain" | "update_issue_domain" | "delete_issue_domain" => audited(
                &["Scope::OrgAdmin"],
                "issue domain administration with a path-org admin re-check",
            ),
            "export_issue_domain" => audited(
                &["Scope::OrgAdmin"],
                "export of the org's G5 issues with a path-org admin re-check",
            ),
            "list_issue_domains" | "search_issues" | "resolve_issue" | "get_github_issue"
            | "get_native_issue" | "get_github_write_target" => audited(
                &["Scope::WorkspacesRead"],
                "org-member reads of configured issue domains",
            ),
            "post_native_issue_comment"
            | "edit_native_issue_comment"
            | "change_native_issue_state"
            | "edit_native_issue" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member writes to a G5 issue as the caller's G5 account",
            ),
            "list_native_issue_attachments" | "get_native_issue_attachment" => audited(
                &["Scope::WorkspacesRead"],
                "org-member reads of a G5 issue's attachments",
            ),
            "upload_native_issue_attachment" | "remove_native_issue_attachment" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member attachment upload, or removal by its uploader or an org admin, on a G5 issue",
            ),
            "edit_github_page_issue"
            | "change_github_page_issue_state"
            | "post_github_page_comment"
            | "edit_github_page_comment" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member writes to a GitHub issue as the caller's own GitHub account",
            ),
            "edit_native_workspace_comment" => audited(
                &["Scope::WorkspacesWrite"],
                "edit the caller's own comment on the primary G5 issue",
            ),
            "list_workspace_issue_links" => {
                audited(&["Scope::WorkspacesRead"], "workspace issue link read")
            }
            "get_workspace_activity" => audited(
                &["Scope::WorkspacesRead"],
                "workspace activity read (issue discussion and workspace context)",
            ),
            "get_workspace_activity_current" => audited(
                &["Scope::WorkspacesRead"],
                "workspace activity read (ongoing conversations and tasks)",
            ),
            "create_workspace_issue_link" | "unlink_workspace_issue_link" => {
                audited(&["Scope::WorkspacesWrite"], "workspace issue link mutation")
            }
            "post_workspace_comment" | "edit_workspace_comment" => audited(
                &["Scope::WorkspacesWrite"],
                "comment on the primary issue as the caller's own GitHub account",
            ),
            "change_linked_issue_state" => audited(
                &["Scope::WorkspacesWrite"],
                "close or reopen a linked issue as the caller's own GitHub account",
            ),
            "get_create_target" => audited(
                &["Scope::WorkspacesRead"],
                "org-member read of the issue creation target and the caller's GitHub login",
            ),
            "create_org_issue" => audited(
                &["Scope::WorkspacesWrite"],
                "create an issue in the org's creation target as the caller's own GitHub account",
            ),
            _ => return None,
        },
        "routes/issue_labels.rs" => match handler {
            "list_domain_labels" => audited(
                &["Scope::WorkspacesRead"],
                "org-member read of a configured issue domain's label catalog",
            ),
            "create_domain_label" | "edit_domain_label" | "delete_domain_label" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member write to a domain's label catalog, as the caller's own GitHub \
                 account on GitHub",
            ),
            "change_github_page_labels" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member label change on a GitHub issue as the caller's own GitHub account",
            ),
            "change_native_page_labels" => audited(
                &["Scope::WorkspacesWrite"],
                "org-member label change on a G5 issue as the caller's G5 account",
            ),
            _ => return None,
        },
        "routes/pull_request_conversation.rs" => match handler {
            "post_pull_request_comment" | "resolve_review_thread" => audited(
                &["Scope::WorkspacesWrite"],
                "comment on or resolve a thread of an attached repository's PR as the caller's own GitHub account",
            ),
            _ => return None,
        },
        "routes/git.rs" => match handler {
            "info_refs" | "upload_pack" | "receive_pack" => {
                raw("Git Smart HTTP authenticates inline with GitAuth")
            }
            _ => return None,
        },
        "routes/instance.rs" => match handler {
            "get_instance_info" | "health_check" => exempt("public instance metadata"),
            "get_chat_models" => exempt("authenticated low-risk instance status"),
            _ => return None,
        },
        "routes/internal.rs" => exempt("internal worker route guarded by internal auth middleware"),
        "routes/tasks.rs" => {
            if is_read_method(method) {
                audited(&["Scope::TasksRead"], "task read")
            } else {
                audited(
                    &["Scope::TasksExecute", "Scope::TasksExecute"],
                    "task execution control",
                )
            }
        }
        "routes/mod.rs" => match handler {
            "ws_adapter" => raw("WebSocket endpoints authenticate by ticket"),
            "terminal_ws_adapter" => {
                raw("terminal WebSocket validates a user token and attached owner")
            }
            _ => return None,
        },
        "routes/workspace_flush.rs" => {
            raw("workspace flush endpoints validate an incarnation token inline")
        }
        // G5-Lean#710: the middleware admits the workspace token; each
        // handler checks the live incarnation and the target's agent scope.
        "routes/workspace_agent.rs" => {
            exempt("workspace-agent routes check the incarnation and agent scope in the handler")
        }
        "routes/preview.rs" => match handler {
            // Browser cookie handoff (issue G5Labs/G5#736, routes/preview
            // [I2]): Bearer-authed mint of the workspace-bound preview handoff ticket
            // cookie. Read scope — the ticket only grants what the proxy
            // path itself enforces.
            "preview_session_mint" => audited(&["Scope::TasksRead"], "preview ticket-cookie mint"),
            // The proxy entry handlers front the previewed app with `any()`
            // registration; same read scope as `GET /tasks/:id`.
            "preview_proxy_root" | "preview_proxy_path" => {
                audited(&["Scope::TasksRead"], "browser preview proxy")
            }
            _ => return None,
        },
        "routes/org_approvals.rs" => match handler {
            "list_definitions" => {
                exempt("authenticated org member approval-definition library read")
            }
            _ => audited(
                &["Scope::OrgAdmin"],
                "org approval-definition administration mutation",
            ),
        },
        "routes/workspace_reviews.rs" => match handler {
            "get_review_context" | "list_publications" | "list_suggestions" | "count_suggestions"
            | "list_replies" | "download_review_artifact" => {
                audited(&["Scope::WorkspacesRead"], "review context and suggestion reads")
            }
            "start_review" | "publish_review" | "preview_review" | "discard_review_changes" => {
                audited(
                    &["Scope::WorkspacesWrite"],
                    "review workspace fork/resume, Push review and Discard from my copy, whose \
                     preview saves and writes the review copy (review workspace owner)",
                )
            }
            "accept_suggestion" | "reject_suggestion" | "resolve_suggestion" => audited(
                &["Scope::WorkspacesWrite"],
                "owner decisions on suggestions (source owner; accept writes the running working tree)",
            ),
            "withdraw_suggestion" | "add_reply" => {
                audited(
                &["Scope::WorkspacesRead"],
                "reviewer-side review rows on the source (single-writer: no source content write)",
            )
            }
            "publish_file_suggestions" => audited(
                &["Scope::WorkspacesRead"],
                "a non-owner reader's FILES push: review rows on the source, no content write \
                 (g5.governance.file-suggestions)",
            ),
            _ => return None,
        },
        "routes/workspace_approvals.rs" => match handler {
            "get_approvals_state" => exempt("authenticated workspace member approval-state read"),
            "create_human_requests" => audited(
                &["Scope::WorkspacesWrite"],
                "explicit workspace-scoped human approval-request admission",
            ),
            "cancel_human_request" => exempt(
                "exact-requester authorization: ApprovalConfigService locks the current-org \
                 source-workspace pending request by requested_by before cancellation",
            ),
            "create_review" | "retry_review_cleanup" => audited(
                &["Scope::WorkspacesWrite"],
                "shared workspace-owner review request/cleanup policy",
            ),
            "get_review_report" | "get_review_commits" | "get_review_transcript" => audited(&["Scope::WorkspacesRead"], "retained report under the source workspace audience"),
            "record_decision" => audited(
                &["Scope::WorkspacesRead"],
                "request-addressed human decision: ApprovalConfigService locks the caller's \
                 exact pending request before any write; requestless decisions stay owner-only",
            ),
            _ => audited(
                &["Scope::OrgAdmin"],
                "workspace approval requirement administration",
            ),
        },
        "routes/orgs.rs" => org_route_contract(handler, method),
        "routes/permissions.rs" => permission_route_contract(handler, method),
        "routes/platform.rs" => match handler {
            "list_orgs" | "create_org" => exempt(
                "platform operator route: person session plus the caller's locked current \
                 operator membership (g5.organization.platform-operator.authority)",
            ),
            _ => return None,
        },
        "routes/platform_issues.rs" => match handler {
            "create_platform_issue" => audited(
                &["Scope::UsersRead"],
                "authenticated platform issue reporting",
            ),
            _ => return None,
        },
        "routes/settings.rs" => match handler {
            "get_spend_summary" | "get_spend_history" => {
                audited(&["Scope::BillingAdmin"], "billing spend administration")
            }
            _ => workspace_route_contract(handler, method),
        },
        "routes/self_hosted.rs" => match handler {
            "provision_license" => {
                exempt("self-hosted admin license provisioning uses admin bearer token")
            }
            "issue_registry_token" => {
                exempt("self-hosted registry exchange uses license key bearer token")
            }
            _ => return None,
        },
        "routes/spend.rs" => audited(&["Scope::BillingAdmin"], "billing spend administration"),
        "routes/telemetry.rs" => match handler {
            // UsersRead is the one scope every role carries: UI telemetry
            // must work for read-only viewers, and the route mutates nothing
            // (it re-emits validated events on the g5_audit tracing target;
            // issue #1552, routes/telemetry [D1]). Handler-scoped so a
            // future telemetry handler fails contract scanning instead of
            // silently inheriting this contract.
            "ingest_ui_events" => audited(
                &["Scope::UsersRead"],
                "UI telemetry ingest — telemetry-only g5_audit emission",
            ),
            _ => return None,
        },
        "routes/users.rs" => match handler {
            "get_current_user" | "update_current_user" | "get_user_public" => {
                exempt("authenticated current-user or public-user profile route")
            }
            _ => return None,
        },
        "routes/worker_hosts.rs" => audited(&["Scope::WorkersAdmin"], "worker-host administration"),
        _ => return None,
    };

    Some(contract)
}

fn workspace_route_contract(handler: &str, method: &str) -> RouteScopeContract {
    match handler {
        "record_user_activity" => audited(
            &["Scope::WorkspacesRead"],
            "current owner interaction, including read-only UI use; no content mutation",
        ),
        "resolve_workspace_agent_launch" => audited(
            &["Scope::WorkspacesRead"],
            "non-mutating effective agent-launch resolution",
        ),
        "list_pending_sync_files" => {
            audited(&["Scope::WorkspacesRead"], "non-mutating pending sync list")
        }
        _ if is_read_method(method) && !workspace_handler_mutates_on_read(handler) => {
            audited(&["Scope::WorkspacesRead"], "workspace-scoped read")
        }
        _ => audited(&["Scope::WorkspacesWrite"], "workspace-scoped mutation"),
    }
}

fn auth_route_contract(handler: &str) -> Option<RouteScopeContract> {
    let contract = match handler {
        "login" | "callback" => exempt("OAuth login/callback before local token exists"),
        "refresh_token" | "logout" => exempt("token lifecycle endpoint"),
        "cli_device_start" | "cli_device_token" | "cli_device_approve" => {
            exempt("CLI device authorization flow")
        }
        "preview_reauth" => exempt(
            "preview re-auth navigation — g5_refresh cookie is the credential and \
             scoped_db_for_task re-authorizes per request (routes/preview [D18])",
        ),
        "ws_ticket" => exempt("WebSocket ticket minting with handler-level workspace access guard"),
        "issue_test_token" => exempt("E2E-only test token endpoint gated by environment"),
        "scm_connect" | "scm_connect_pat" | "scm_disconnect" | "list_scm_orgs" => {
            exempt("account SCM credentials with handler-level person-session guard")
        }
        "scm_callback" => exempt("SCM OAuth callback with state and browser attempt cookie"),
        "list_scm_repos" => audited(&["Scope::WorkspacesRead"], "person-session SCM repo read"),
        "import_scm_repos" => audited(
            &["Scope::WorkspacesWrite"],
            "person-session SCM repo import",
        ),
        _ => return None,
    };
    Some(contract)
}

fn org_route_contract(handler: &str, method: &str) -> RouteScopeContract {
    match handler {
        "list_orgs" | "get_org" | "list_members" | "accept_invitation" | "decline_invitation" => {
            exempt("authenticated org self-service route")
        }
        "list_repos" => audited(&["Scope::WorkspacesRead"], "org repo read"),
        // Stop by user: the member themself or an org admin, re-checked by
        // OrgService (`g5.lifecycle.stop.by-user`).
        "member_shutdown" => audited(
            &["Scope::WorkspacesRead"],
            "member's own or administered shutdown progress",
        ),
        "stop_member_workspaces" => audited(
            &["Scope::WorkspacesWrite"],
            "stop the member's own or an administered member's workspaces",
        ),
        _ if is_read_method(method) => audited(&["Scope::OrgAdmin"], "org administration read"),
        _ => audited(&["Scope::OrgAdmin"], "org administration mutation"),
    }
}

fn permission_route_contract(handler: &str, method: &str) -> RouteScopeContract {
    match handler {
        "get_my_scopes" | "check_scopes" => exempt("authenticated self-service scope route"),
        "get_org_default_scopes" | "get_user_scopes" | "list_workspace_overrides" => {
            audited(&["Scope::WorkspacesRead"], "permission read")
        }
        _ if is_read_method(method) => audited(&["Scope::WorkspacesRead"], "permission read"),
        _ => audited(&["Scope::OrgAdmin"], "permission administration mutation"),
    }
}

fn is_read_method(method: &str) -> bool {
    matches!(method, "get" | "head")
}

fn workspace_handler_mutates_on_read(handler: &str) -> bool {
    matches!(handler, "get_draft_for_branch")
}

fn audited(scopes: &'static [&'static str], reason: &'static str) -> RouteScopeContract {
    RouteScopeContract {
        registration: RouteRegistrationRequirement::Audited,
        scopes,
        reason,
    }
}

fn exempt(reason: &'static str) -> RouteScopeContract {
    RouteScopeContract {
        registration: RouteRegistrationRequirement::Exempt,
        scopes: &[],
        reason,
    }
}

fn raw(reason: &'static str) -> RouteScopeContract {
    RouteScopeContract {
        registration: RouteRegistrationRequirement::Raw,
        scopes: &[],
        reason,
    }
}

fn route_kind_label(kind: RouteRegistrationKind) -> &'static str {
    match kind {
        RouteRegistrationKind::Audited => "audited_route",
        RouteRegistrationKind::Exempt => "exempt_route",
        RouteRegistrationKind::Raw => "raw .route",
    }
}

fn auth_contract_for_argument(file: &str, handler: &str, argument: &str) -> Option<AuthContract> {
    if file == "routes/workspaces.rs"
        && handler == "update_workspace_pull_request_auto_merge_settings"
        && matches!(argument, "path.id" | "path.pull_request_id")
    {
        return Some(AuthContract {
            scope: "source owner of the pull request into this target workspace",
            guard_tokens: &["workspace_pull_request_auto_merge::set_stop_vm_after_merge("],
        });
    }
    if file == "routes/source_control_browser.rs"
        && handler == "check"
        && argument == "body.selected"
    {
        return Some(AuthContract {
            scope: "selected revisions of captured authorized repository members",
            guard_tokens: &["capture", "input.selected", "refreshed_menu"],
        });
    }
    if file == "routes/source_control_browser.rs"
        && handler == "pushed_scope"
        && argument == "body.heads"
    {
        return Some(AuthContract {
            scope:
                "fresh authorized repository members; the legacy heads hint cannot select content",
            guard_tokens: &["capture", "input.heads", "current.repos"],
        });
    }
    if file == "routes/source_control_browser.rs"
        && handler == "history_page"
        && argument == "body.repo_id"
    {
        return Some(AuthContract {
            scope: "repository member of the authorized workspace and retained comparison cuts",
            guard_tokens: &["capture", "r.repo_id == input.repo_id"],
        });
    }
    if file == "routes/workspace_review_flow.rs"
        && handler == "changes"
        && argument == "query.pull_request_id"
    {
        return Some(AuthContract {
            scope: "PR belongs to this source workspace; target cuts require current read access",
            guard_tokens: &[
                "scoped_db_for_workspace",
                "history::comparison_cuts",
                "q.pull_request_id",
            ],
        });
    }
    if file == "routes/workspace_review_flow.rs" && matches!(handler, "changes" | "commits") {
        return Some(AuthContract {
            scope: "authorized workspace revision projection",
            guard_tokens: &["scoped_db_for_workspace"],
        });
    }
    // A recipient's chosen set of their own notifications: the scoped
    // read-all changes only unread rows in the authorized workspace that are
    // addressed to the caller, so ids outside that set select nothing
    // (g5.user.notifications).
    if file == "routes/notifications.rs" && handler == "mark_all_read" && argument == "body.ids" {
        return Some(AuthContract {
            scope: "the caller's own notifications in the authorized workspace",
            guard_tokens: &["scoped_db_for_workspace", "mark_all_read"],
        });
    }
    // The named result must be a finished result of an agent task in the
    // path chat, which the caller's read-scoped session view authorizes
    // (chat g5.chat.inspection.acknowledge).
    if file == "routes/chat.rs"
        && handler == "record_session_inspection"
        && argument == "body.task_id"
    {
        return Some(AuthContract {
            scope: "a finished result of the authorized chat",
            guard_tokens: &["scoped_db_for_chat_session_view", "record_inspection"],
        });
    }
    // The edited card must be a reply of the path chat, which the caller's
    // write-scoped session view authorizes (chat g5.chat.ontology-card.edit).
    if file == "routes/chat.rs" && handler == "edit_ontology_card" && argument == "path.mid" {
        return Some(AuthContract {
            scope: "the newest ontology changes card of the authorized chat",
            guard_tokens: &["scoped_db_for_chat_session", "chat_ontology::plan_edit"],
        });
    }
    let identifier = argument.rsplit('.').next().unwrap_or(argument);
    let normalized = normalized_identifier_name(file, handler, identifier);

    // Workspace-agent routes (G5-Lean#710): the path workspace must be the
    // incarnation's own, and every target passes the agent scope check
    // (`db::workspace_agent_scope`) before the reused operation runs.
    if file == "routes/workspace_agent.rs" {
        let contract = match normalized.as_str() {
            "id" => Some(AuthContract {
                scope: "the incarnation's own workspace",
                guard_tokens: &["require_own_workspace"],
            }),
            "target_id" => Some(AuthContract {
                scope: "agent-scoped own or owned, or owner-readable workspace",
                guard_tokens: &[
                    "agent_target",
                    "agent_relation",
                    "agent_read_target",
                    "lifecycle_for",
                    "PullRequestCaller::WorkspaceChild",
                    "admit_child_chat",
                    "admit_approval_target",
                    "admit_approval_read",
                    "admit_self_or_child",
                    "admit_file_read",
                ],
            }),
            "source_workspace_id" => Some(AuthContract {
                scope: "fork source: own, owned or the owner-readable Base",
                guard_tokens: &["agent_fork_source"],
            }),
            "session_id" => Some(AuthContract {
                scope: "chat in an owner-readable workspace or the admitted owned one",
                guard_tokens: &[
                    "admit_session_read",
                    "resolve_chat_session_context",
                    "admit_child_chat",
                ],
            }),
            // The task, turn and input services lock the row and check it
            // belongs to the admitted chat or workspace and its current owner.
            // A chat turn's ontology record is reachable only for a running
            // agent task of the caller's workspace on its own VM and epoch
            // (chat g5.chat.ontology-card).
            "task_id" => Some(AuthContract {
                scope: "task of the admitted workspace or chat",
                guard_tokens: &[
                    "resolve_task_context",
                    "end_session_background_wait",
                    "chat_ontology::running_turn",
                ],
            }),
            "turn_id" => Some(AuthContract {
                scope: "turn of the admitted child chat",
                guard_tokens: &["ensure_session_turn_id"],
            }),
            "input_id" => Some(AuthContract {
                scope: "queued input of the admitted child chat",
                guard_tokens: &[
                    "edit_queued_session_input",
                    "withdraw_queued_session_input",
                    "authorize_unknown_input_redelivery",
                ],
            }),
            // Preview and file operations resolve the repository among the
            // admitted workspace's own repositories before any runtime read
            // or write.
            "repo_id" => Some(AuthContract {
                scope: "repository of the admitted workspace",
                guard_tokens: &[
                    "get_workspace_repository",
                    "start_workspace_preview",
                    "create_preview_profile_with",
                    "read_file_tree_as",
                    "read_workspace_file_as",
                    "write_workspace_file_and_maybe_push_as",
                    "delete_workspace_file_as",
                    "rename_workspace_file_as",
                ],
            }),
            "request_id" => Some(AuthContract {
                scope: "approval request the owner made for the admitted workspace",
                guard_tokens: &["cancel_human_request_with"],
            }),
            // The review service resolves the review together with the
            // admitted source workspace (G5-Lean#1019).
            "review_id" => Some(AuthContract {
                scope: "formal agent review of the admitted workspace",
                guard_tokens: &["approval_review_service"],
            }),
            "pull_request_id" => Some(AuthContract {
                scope: "pull request into or out of the incarnation's own workspace",
                guard_tokens: &[
                    "PullRequestMerger::WorkspaceAgent",
                    // Locks the pull request only with the caller as its source.
                    "workspace_pull_request_auto_merge::disable(",
                ],
            }),
            _ => None,
        };
        if contract.is_some() {
            return contract;
        }
    }

    if file == "routes/mod.rs"
        && handler == "terminal_ws_adapter"
        && argument == "query.terminal_id"
    {
        return Some(AuthContract {
            scope: "current-owner workspace terminal reattachment",
            guard_tokens: &["validate_websocket_ticket", "open_owner_stream"],
        });
    }

    // GitHub identifiers are not G5 tenant resources. Coverage ids are
    // accepted only when the org's active installation can access them (live
    // GitHub list); a setup installation id is only bound into signed OAuth
    // state, and no binding is recorded until GitHub proves the OAuth user
    // can access that installation.
    if file == "routes/github_app.rs"
        && handler == "update_github_app_coverage"
        && normalized == "github_repo_ids"
    {
        return Some(AuthContract {
            scope: "GitHub repositories accessible to the org's active App installation",
            guard_tokens: &["validate_coverage_selection"],
        });
    }
    if file == "routes/github_app.rs"
        && handler == "github_app_setup_callback"
        && normalized == "installation_id"
    {
        return Some(AuthContract {
            scope: "installation id bound into signed OAuth state before access proof",
            guard_tokens: &["oauth_state_for_installation"],
        });
    }

    if file == "routes/config.rs" && handler == "update_org_config" && normalized == "org_id" {
        return Some(AuthContract {
            scope: "path organization the caller currently administers",
            guard_tokens: &["org_admin_db"],
        });
    }

    // Admin page organization settings (g5.security.org-admin-settings): both
    // reads and writes hold the path org's current owner/admin authority.
    if file == "routes/orgs.rs"
        && matches!(handler, "get_admin_settings" | "update_admin_settings")
        && normalized == "org_id"
    {
        return Some(AuthContract {
            scope: "path organization the caller currently administers",
            guard_tokens: &["org_admin_db"],
        });
    }

    // External issue domains are organization configuration. The admin
    // handlers open the path org's RLS scope and re-check its Admin role
    // (org_admin_db) before the service reads or writes a domain, and every
    // domain query is also filtered by the path org.
    if file == "routes/external_issues.rs"
        && matches!(handler, "update_issue_domain" | "delete_issue_domain")
    {
        match normalized.as_str() {
            "org_id" => {
                return Some(AuthContract {
                    scope: "path org the caller administers",
                    guard_tokens: &["org_admin_db"],
                })
            }
            "domain_id" => {
                return Some(AuthContract {
                    scope: "issue domain of the administered path org",
                    guard_tokens: &["update_domain_settings", "delete_domain"],
                })
            }
            _ => {}
        }
    }
    // Exporting reads only a domain found among the path org's own domains,
    // loaded in its admin-checked scope.
    if file == "routes/external_issues.rs" && handler == "export_issue_domain" {
        match normalized.as_str() {
            "org_id" => {
                return Some(AuthContract {
                    scope: "path org the caller administers",
                    guard_tokens: &["org_admin_db"],
                })
            }
            "domain_id" => {
                return Some(AuthContract {
                    scope: "issue domain of the administered path org",
                    guard_tokens: &["org_admin_db", "list_domain_access"],
                })
            }
            _ => {}
        }
    }
    // G5 issue page writes open the path org's member-write scope
    // (native_page_db: scoped_db_for_org with workspaces:write, then the
    // org's native domain); issue numbers and comment ids are looked up only
    // in that domain under its RLS scope, and a comment edit only for the
    // caller's own comment.
    if file == "routes/external_issues.rs"
        && matches!(
            handler,
            "post_native_issue_comment"
                | "edit_native_issue_comment"
                | "change_native_issue_state"
                | "edit_native_issue"
        )
    {
        match normalized.as_str() {
            "org_id" => {
                return Some(AuthContract {
                    scope: "path org with the caller's current write authority",
                    guard_tokens: &["native_page_db"],
                })
            }
            "comment_id" => {
                return Some(AuthContract {
                    scope: "the caller's own comment on a G5 issue of the path org",
                    guard_tokens: &["native_page_db", "edit_comment"],
                })
            }
            _ => {}
        }
    }
    // G5 issue attachments are read in the path org's member-read scope
    // (native_db_with: scoped_db_for_org, then the org's native domain) and
    // written in its member-write scope (native_page_db). An attachment id is
    // looked up only on an issue of that domain under its RLS scope; removal
    // takes the caller's own, or any when `is_org_admin` confirms the caller
    // administers the path org.
    if file == "routes/external_issues.rs"
        && matches!(
            handler,
            "list_native_issue_attachments"
                | "get_native_issue_attachment"
                | "upload_native_issue_attachment"
                | "remove_native_issue_attachment"
        )
    {
        match (handler, normalized.as_str()) {
            ("list_native_issue_attachments" | "get_native_issue_attachment", "org_id") => {
                return Some(AuthContract {
                    scope: "path org with the caller's current read authority",
                    guard_tokens: &["native_db_with"],
                })
            }
            ("upload_native_issue_attachment" | "remove_native_issue_attachment", "org_id") => {
                return Some(AuthContract {
                    scope: "path org with the caller's current write authority",
                    guard_tokens: &["native_page_db"],
                })
            }
            ("get_native_issue_attachment", "attachment_id") => {
                return Some(AuthContract {
                    scope: "an attachment of a G5 issue of the path org",
                    guard_tokens: &["native_db_with", "attachment_content"],
                })
            }
            ("remove_native_issue_attachment", "attachment_id") => {
                return Some(AuthContract {
                    scope: "the caller's own attachment, or any for an org admin, on a G5 issue of the path org",
                    guard_tokens: &["native_page_db", "remove_attachment", "is_org_admin"],
                })
            }
            _ => {}
        }
    }
    // A domain's label catalog: the domain must be one of the path org's,
    // loaded in its member scope (domain_db); a label id is looked up only
    // within that domain by its adapter.
    if file == "routes/issue_labels.rs"
        && matches!(
            handler,
            "list_domain_labels"
                | "create_domain_label"
                | "edit_domain_label"
                | "delete_domain_label"
        )
    {
        match normalized.as_str() {
            "org_id" | "domain_id" => {
                return Some(AuthContract {
                    scope: "configured issue domain of the path org, with the caller's current \
                            read or write authority",
                    guard_tokens: &["domain_db"],
                })
            }
            "label_id" => {
                return Some(AuthContract {
                    scope: "label of that domain",
                    guard_tokens: &["edit_label_as", "delete_label_as"],
                })
            }
            _ => {}
        }
    }
    // Issue label changes open the same member-write scopes as the other
    // issue page writes.
    if file == "routes/issue_labels.rs" && handler == "change_github_page_labels" {
        if let "org_id" | "repository_id" = normalized.as_str() {
            return Some(AuthContract {
                scope: "configured issue domain of the path org with the caller's current write \
                        authority",
                guard_tokens: &["github_page_target"],
            });
        }
    }
    if file == "routes/issue_labels.rs"
        && handler == "change_native_page_labels"
        && normalized == "org_id"
    {
        return Some(AuthContract {
            scope: "path org with the caller's current write authority",
            guard_tokens: &["native_page_db"],
        });
    }
    // The agent names a catalog label by id only within a domain of its
    // owner's organization, loaded in the owner's scope.
    if file == "routes/workspace_agent.rs"
        && matches!(handler, "edit_issue_label" | "delete_issue_label")
        && argument == "body.label_id"
    {
        return Some(AuthContract {
            scope: "label of an issue domain of the owner's organization",
            guard_tokens: &["edit_label_as", "delete_label_as"],
        });
    }
    // GitHub issue page writes open the path org's member-write scope
    // (github_page_target: scoped_db_for_org with workspaces:write, then the
    // repository must be one of the org's domains) before any GitHub write.
    if file == "routes/external_issues.rs"
        && matches!(
            handler,
            "edit_github_page_issue"
                | "change_github_page_issue_state"
                | "post_github_page_comment"
                | "edit_github_page_comment"
        )
    {
        match normalized.as_str() {
            "org_id" => {
                return Some(AuthContract {
                    scope: "path org with the caller's current write authority",
                    guard_tokens: &["github_page_target"],
                })
            }
            "repository_id" => {
                return Some(AuthContract {
                    scope: "configured issue domain of the path org",
                    guard_tokens: &["github_page_target"],
                })
            }
            _ => {}
        }
    }
    // A G5 comment id is edited only on the path workspace's primary issue
    // and only when the caller wrote it.
    if file == "routes/external_issues.rs" && handler == "edit_native_workspace_comment" {
        match normalized.as_str() {
            "workspace_id" => {
                return Some(AuthContract {
                    scope: "workspace audience and current target-org write authority",
                    guard_tokens: &["comment_db"],
                })
            }
            "comment_id" => {
                return Some(AuthContract {
                    scope: "the caller's own comment on the path workspace's primary G5 issue",
                    guard_tokens: &["comment_db", "edit_comment"],
                })
            }
            _ => {}
        }
    }
    // A GitHub repository id is not a G5 tenant resource. It becomes a domain
    // only for a path-org admin, and only when the org's latest active GitHub
    // App binding covers it with Issues read (create_github_domain preflight).
    if file == "routes/external_issues.rs"
        && handler == "create_issue_domain"
        && normalized == "github_repo_id"
    {
        return Some(AuthContract {
            scope: "GitHub repository covered by the org's active App binding",
            guard_tokens: &["org_admin_db", "create_github_domain"],
        });
    }
    // Issue reads are granted by org membership for configured domains only:
    // the repository id must match one of the path org's domains, loaded
    // under its RLS scope, before any GitHub request.
    if file == "routes/external_issues.rs"
        && matches!(handler, "get_github_issue" | "get_github_write_target")
        && normalized == "repository_id"
    {
        return Some(AuthContract {
            scope: "configured issue domain of the path org",
            guard_tokens: &["scoped_db_for_org", "domain_for_repository"],
        });
    }
    // The owner's agent reading a linked issue (routes/internal): the
    // workspace is fenced by `scoped_db_for_workspace_incarnation` first, and
    // the link id is looked up only among that workspace's active links.
    if file == "routes/internal.rs"
        && handler == "get_linked_issue_internal"
        && normalized == "link_id"
    {
        return Some(AuthContract {
            scope: "issue link of the incarnation-fenced workspace",
            guard_tokens: &[
                "scoped_db_for_workspace_incarnation",
                "list_workspace_links",
            ],
        });
    }
    // A link id is subordinate to the path workspace: the handler opens the
    // workspace's owner-checked write scope and the unlink is filtered by
    // that org and workspace.
    if file == "routes/external_issues.rs"
        && handler == "unlink_workspace_issue_link"
        && normalized == "link_id"
    {
        return Some(AuthContract {
            scope: "issue link of the path workspace",
            guard_tokens: &["scoped_db_for_workspace", "unlink_workspace_link"],
        });
    }
    // Activity reports cannot select a VM independently of the authorized
    // workspace. The current owner handle must match both incarnation fields
    // before the runtime's owner/membership/lease-fenced timestamp update.
    if file == "routes/workspaces.rs" && handler == "record_user_activity" && normalized == "vm_id"
    {
        return Some(AuthContract {
            scope: "current owner VM incarnation of the path workspace",
            guard_tokens: &["handle.vm_id == body.vm_id && handle.epoch == body.epoch"],
        });
    }

    // A journey step id is a client catalog slug, not a tenant resource: the
    // mark is stored only on the path workspace, after the owner and mutation
    // policy check, and the service validates the slug shape.
    if file == "routes/workspaces.rs"
        && handler == "set_workspace_journey_step"
        && normalized == "step_id"
    {
        return Some(AuthContract {
            scope: "journey step slug of the path workspace",
            guard_tokens: &["ensure_workspace_mutable", "set_journey_mark"],
        });
    }

    // These collaborative writes use a helper that checks both workspace
    // visibility and current target-org write authority. Its implementation is
    // checked by comment_db_preserves_the_workspace_authorization_boundary.
    if file == "routes/external_issues.rs"
        && matches!(
            handler,
            "post_workspace_comment" | "edit_workspace_comment" | "change_linked_issue_state"
        )
        && normalized == "workspace_id"
    {
        return Some(AuthContract {
            scope: "workspace audience and current target-org write authority",
            guard_tokens: &["comment_db"],
        });
    }

    // A linked issue's state is changed only for a link found among the path
    // workspace's own links (list_workspace_links of that org and workspace);
    // another workspace's link id is not found, and the GitHub write uses the
    // caller's own grant.
    if file == "routes/external_issues.rs"
        && handler == "change_linked_issue_state"
        && normalized == "link_id"
    {
        return Some(AuthContract {
            scope:
                "an issue link of the path workspace, changed as the caller's own GitHub account",
            // The shared transition (`change_linked_issue_state_as`, also the
            // workspace agent's) looks the link up among those links.
            guard_tokens: &[
                "comment_db",
                "list_workspace_links",
                "change_linked_issue_state_as",
            ],
        });
    }

    // The creation target the person saw is only compared with the path
    // org's own default target, read in the org's member-write scope; a
    // mismatch refuses (`destination_changed`) and nothing else is looked up
    // by it.
    if file == "routes/external_issues.rs"
        && handler == "create_org_issue"
        && normalized == "domain_id"
    {
        return Some(AuthContract {
            scope: "creation target of the path org, compared only",
            guard_tokens: &["create_tracker_issue"],
        });
    }

    // A GitHub comment id is not a G5 tenant resource. The handler opens the
    // path workspace's access-checked scope, and GitHub is asked only about a
    // comment on that workspace's primary issue authored by the caller's own
    // GitHub account, using the caller's token (`edit_github_comment`).
    if file == "routes/external_issues.rs"
        && handler == "edit_workspace_comment"
        && normalized == "comment_id"
    {
        return Some(AuthContract {
            scope: "the caller's own GitHub comment on the path workspace's primary issue",
            guard_tokens: &["comment_db", "edit_github_comment"],
        });
    }
    // On a GitHub issue's page the issue is read in one of the path org's
    // domains under its member-write scope, and the comment must be on that
    // issue and the caller's own (`edit_github_comment`).
    if file == "routes/external_issues.rs"
        && handler == "edit_github_page_comment"
        && normalized == "comment_id"
    {
        return Some(AuthContract {
            scope: "the caller's own GitHub comment on an issue of the path org's domains",
            guard_tokens: &["github_page_target", "edit_github_comment"],
        });
    }

    if file == "routes/chat.rs"
        && matches!(
            handler,
            "edit_queued_input"
                | "withdraw_queued_input"
                | "redeliver_unknown_input"
                | "cancel_unknown_input"
        )
        && normalized == "input_id"
    {
        return Some(AuthContract {
            scope: "authorized chat-session input",
            guard_tokens: &["scoped_db_for_chat_session", "task_service"],
        });
    }

    if file == "routes/tasks.rs"
        && handler == "reply_to_provider_request"
        && matches!(normalized.as_str(), "request_id" | "provider_request_id")
    {
        return Some(AuthContract {
            scope: "task-attempt-scoped native provider request",
            guard_tokens: &["scoped_db_for_task", "reply_to_native_request"],
        });
    }

    if file == "routes/workspace_flush.rs" && normalized == "flush_id" {
        return Some(AuthContract {
            scope: "incarnation-scoped workspace flush",
            guard_tokens: &["claims", "WorkspaceFlushService"],
        });
    }

    if file == "routes/workspaces.rs"
        && matches!(
            handler,
            "resume_workspace_vm" | "stop_workspace_vm" | "delete_workspace_action"
        )
        && normalized == "operation_id"
    {
        return Some(AuthContract {
            scope: "owner-authorized workspace lifecycle idempotency key",
            guard_tokens: &["authorize_workspace_action"],
        });
    }

    if file == "routes/worker_hosts.rs"
        && handler == "capacity_stop_workspace"
        && matches!(normalized.as_str(), "workspace_id" | "operation_id")
    {
        return Some(AuthContract {
            scope: "org-operator workspace capacity stop",
            guard_tokens: &["scoped_db_for_org"],
        });
    }

    if file == "routes/workspaces.rs"
        && handler == "transfer_workspace"
        && normalized == "to_user_id"
    {
        return Some(AuthContract {
            scope: "workspace-transfer target organization membership",
            guard_tokens: &["scoped_db_for_workspace", ".transfer("],
        });
    }

    // The challenge is subordinate to the already-authorized task. The route
    // first proves task access, then TaskService compares this exact id with
    // the challenge stored on that same task before accepting any auth JSON.
    if file == "routes/tasks.rs"
        && handler == "submit_task_agent_auth"
        && argument == "body.challenge_id"
    {
        return Some(AuthContract {
            scope: "task-scoped persisted challenge match",
            guard_tokens: &["scoped_db_for_task", "submit_task_agent_auth"],
        });
    }

    // `host_tunnel_attach` (preview reverse-tunnel attach, PR4) authenticates via
    // the `AuthenticatedHost` extractor (host JWT, `Scope::WorkerHost`) and rejects
    // any request whose path `:hostId` differs from the authenticated host's own id
    // (`host.host_id() != host_id` -> Forbidden) before registering the in-memory
    // tunnel. It opens no DB query, so the DB-scoping guards the generic `host_id`
    // contract expects do not apply — the self-identity match IS the auth boundary.
    if file == "routes/internal.rs" && handler == "host_tunnel_attach" {
        return Some(AuthContract {
            scope: "authenticated host self-identity match",
            guard_tokens: &["host.host_id()"],
        });
    }

    // Provider quota read (routes/workspaces D36; provider_account D4): the
    // handler binds `path.id` through `scoped_db_for_workspace` with
    // `workspaces:read` first, then passes `query.account_id` only as the
    // snapshot-row key of `ProviderQuotaService::fetch_snapshot` on that same
    // tenant transaction (org and workspace are resolved server-side), so the
    // account filter selects among rows the caller can already read.
    if file == "routes/workspaces.rs"
        && handler == "get_workspace_provider_quota"
        && normalized == "account_id"
    {
        return Some(AuthContract {
            scope: "workspace-bound provider-quota snapshot row filter",
            guard_tokens: &["scoped_db_for_workspace", "fetch_snapshot"],
        });
    }

    // Approved rebase publication (routes/workspaces D37; workspace_service
    // D36): the handler authorizes the path workspace through
    // `scoped_db_for_workspace(... "workspaces:write")` before forwarding the
    // supplied checkpoint ids. The service then accepts them only when their
    // duplicate-free set exactly equals the active reset-below-baseline merge
    // runs for the authorized workspace's captured repository membership.
    if file == "routes/workspaces.rs"
        && handler == "approved_rebase_publish"
        && matches!(
            normalized.as_str(),
            "expected_merge_run_id" | "expected_merge_run_ids"
        )
    {
        return Some(AuthContract {
            scope: "workspace-scoped exact active reset-checkpoint set",
            guard_tokens: &["scoped_db_for_workspace", "approved_rebase_publish"],
        });
    }

    // Provider accounts (routes/provider_accounts D1/I3;
    // provider_account_service D2/D3): every handler binds `path.org_id`
    // through `scoped_db_for_org` first, then hands `query.workspace_id` /
    // `path.account_id` to ProviderAccountService on that same tenant
    // transaction. The workspace filter applies only when the caller holds
    // `workspaces:read` on that workspace, and the account id is fetched
    // org-scoped under forced RLS and masked as 404 unless the row is visible
    // to the caller, so neither identifier can reach beyond the bound org.
    if file == "routes/provider_accounts.rs"
        && matches!(normalized.as_str(), "account_id" | "workspace_id")
    {
        return Some(AuthContract {
            scope: "org-scoped provider-account service visibility resolution",
            guard_tokens: &["scoped_db_for_org", "provider_account_service"],
        });
    }

    // UI telemetry ingest (issue #1552, routes/telemetry [D2]):
    // `body.ui_session_id` is an opaque per-tab label minted by the browser
    // client for flow ordering — NOT a resource identifier. It opens no DB
    // query and authorizes nothing: the handler's auth boundary is the
    // browser-JWT `AuthContext::User` check (workers/hosts/API keys are
    // rejected with `browser_user_session_required` before the body is
    // touched), and the only thing the label needs "covering" for is input
    // hygiene — `validate_string` bounds its length and rejects control
    // characters before it is re-emitted as a `g5_audit` log field.
    if file == "routes/telemetry.rs" && handler == "ingest_ui_events" {
        return Some(AuthContract {
            scope: "browser-JWT user session; opaque non-resource label (hygiene-validated)",
            guard_tokens: &["validate_string"],
        });
    }

    // Workspace approvals (routes/workspace_approvals D1/D4, B6 PR-2): every
    // handler authorizes the workspace up front via `scoped_db_for_workspace`
    // and delegates to ApprovalConfigService on that scoped connection.
    // `definitionId` (body or path) resolves strictly org-scoped through
    // ApprovalDefinitionService::get_latest / the (org, target, definition)
    // config lookups under forced RLS, so cross-org ids 404/422 with no
    // write; decision writes additionally lock an exact pending request for
    // the current same-organization actor. The manifest entries in
    // tests/fixtures/security/route_auth_manifest.json document the same proofs.
    if file == "routes/workspace_approvals.rs"
        && handler == "get_review_commits"
        && normalized == "repo_id"
    {
        return Some(AuthContract {
            scope: "repository belongs to the source-scoped frozen review assignment",
            guard_tokens: &["scoped_db_for_workspace", "approval_review_service"],
        });
    }
    if file == "routes/workspace_approvals.rs"
        && ((handler == "create_review" && normalized == "retry_review_id")
            || (matches!(
                handler,
                "get_review_report"
                    | "get_review_commits"
                    | "get_review_transcript"
                    | "retry_review_cleanup"
            ) && normalized == "review_id"))
    {
        return Some(AuthContract {
            scope: "source-workspace-scoped formal review identity",
            guard_tokens: &["scoped_db_for_workspace", "approval_review_service"],
        });
    }
    if file == "routes/workspace_approvals.rs" && matches!(normalized.as_str(), "definition_id") {
        let guard_tokens = if handler == "create_review" {
            &["scoped_db_for_workspace", "approval_review_service"][..]
        } else {
            &["scoped_db_for_workspace", "approval_config_service"][..]
        };
        return Some(AuthContract {
            scope: "workspace-scoped approval service resolution",
            guard_tokens,
        });
    }

    if file == "routes/workspace_approvals.rs"
        && handler == "create_human_requests"
        && normalized == "actor_user_ids"
    {
        return Some(AuthContract {
            scope: "workspace-org-scoped explicit approval recipient validation",
            guard_tokens: &["scoped_db_for_workspace", "approval_config_service"],
        });
    }

    // Client operation identity is namespaced by authorized workspace,
    // requester and recipient before any lookup; it grants no resource access.
    if file == "routes/workspace_approvals.rs"
        && handler == "create_human_requests"
        && normalized == "client_request_id"
    {
        return Some(AuthContract {
            scope: "owner-authorized workspace-scoped review request idempotency key",
            guard_tokens: &["scoped_db_for_workspace", "approval_config_service"],
        });
    }

    // Agent operation UUIDs are also owner/source/requester scoped. They
    // select an immutable admission receipt, never an independent resource.
    if file == "routes/workspace_approvals.rs"
        && handler == "create_review"
        && normalized == "client_request_id"
    {
        return Some(AuthContract {
            scope: "owner-authorized workspace-scoped formal review operation identity",
            guard_tokens: &["scoped_db_for_workspace", "approval_review_service"],
        });
    }

    if file == "routes/workspace_approvals.rs"
        && handler == "record_decision"
        && normalized == "request_id"
    {
        return Some(AuthContract {
            scope: "exact pending approval request actor authorization",
            guard_tokens: &["scoped_db_for_workspace", "approval_config_service"],
        });
    }

    if file == "routes/workspace_approvals.rs"
        && handler == "cancel_human_request"
        && normalized == "request_id"
    {
        return Some(AuthContract {
            scope: "exact pending approval requester authorization",
            guard_tokens: &[
                "scoped_db_for_workspace",
                "approval_config_service",
                "cancel_human_request_with",
            ],
        });
    }

    // Org approval-definition writes (routes/org_approvals D1/D4, B6 PR-1):
    // both write handlers authorize the target org up front via
    // `scoped_db_for_org(... "workspaces:write")` behind a `Scope::OrgAdmin`
    // audited route, then delegate to ApprovalDefinitionService on that same
    // scoped connection. `body.human_user_ids` are approver ids that
    // `validate_humans_are_members` requires to be current org_memberships
    // rows of the authorized org before any insert (typed 422 otherwise);
    // `path.definition_id` resolves strictly org-scoped
    // (`WHERE org_id = $1 AND id = $2`) on the tenant ScopedDb under forced
    // RLS, so a cross-org definition id yields 404 with no write. The
    // manifest entries in tests/fixtures/security/route_auth_manifest.json
    // document the same proofs.
    if file == "routes/org_approvals.rs"
        && matches!(handler, "create_definition" | "publish_definition_version")
        && matches!(normalized.as_str(), "human_user_ids" | "definition_id")
    {
        return Some(AuthContract {
            scope: "org-scoped approval-definition service validation",
            guard_tokens: &["scoped_db_for_org", "approval_definition_service"],
        });
    }

    // Fork-chat carry ids are validated by the canonical workspace fork
    // service after the source workspace read scope is established.
    // `clone_workspace` authorizes the SOURCE workspace up front via
    // `scoped_db_for_workspace(... "workspaces:write")` and forwards
    // `body.chat_session_ids` into `clone_service::CloneWorkspaceRequest` on
    // that same scoped connection. The carry copy
    // (`ChatService::copy_sessions_to_workspace`) then filters
    // `workspace_id = <authorized source> AND id = ANY($ids)` org-scoped, so a
    // session id from another workspace or org matches zero rows and is
    // skipped, never carried. The ids therefore cannot reach beyond the
    // workspace the caller already proved write access to; the manifest entry
    // in tests/fixtures/security/route_auth_manifest.json documents the same proof.
    if file == "routes/workspaces.rs"
        && handler == "clone_workspace"
        && normalized == "chat_session_ids"
    {
        return Some(AuthContract {
            scope: "source-workspace-scoped fork-chat carry filter",
            guard_tokens: &["scoped_db_for_workspace", "workspace_fork_service"],
        });
    }

    // Merge-target selection (clone_service D32, CWE-862):
    // `clone_workspace` authorizes the SOURCE workspace up front via
    // `scoped_db_for_workspace(... "workspaces:write")`, then CloneService
    // validates `body.merge_target_workspace_id` with the same audience
    // predicate used by `list_workspaces`: the target must be live, in the same
    // org, and within the fixed identity-based audience. Unauthorized, cross-org, and tombstoned targets are rejected
    // before any mutation; the manifest entry documents the same proof.
    if file == "routes/workspaces.rs"
        && handler == "clone_workspace"
        && normalized == "merge_target_workspace_id"
    {
        return Some(AuthContract {
            scope: "service-validated merge-target audience and membership",
            guard_tokens: &["scoped_db_for_workspace", "workspace_fork_service"],
        });
    }

    if file == "routes/workspaces.rs"
        && handler == "get_open_question_metrics"
        && normalized == "workspace_ids"
    {
        return Some(AuthContract {
            scope: "org-scoped workspace collection access",
            guard_tokens: &["scoped_db_for_org", "open_question_metrics"],
        });
    }

    // These owner actions also admit an administrator of Base's organization
    // on Base (`g5.governance.pr.base-admin`,
    // `g5.workspace.remote.base-admin-pull`); the helper's checks are pinned
    // by `owner_or_base_admin_admission_keeps_owner_and_base_admin_checks`.
    if file == "routes/workspaces.rs"
        && normalized == "workspace_id"
        && matches!(
            handler,
            "merge_workspace_pull_request"
                | "refresh_workspace_pull_request"
                | "close_workspace_pull_request"
                | "disable_workspace_pull_request_auto_merge"
                | "sync_workspace"
                | "set_branch_repo_auto_pull"
                | "promote_branch_repo_remote_tracking"
                | "preview_provider_history_acceptance"
                | "accept_provider_history"
        )
    {
        return Some(AuthContract {
            scope: "workspace owner, or organization administrator on Base",
            guard_tokens: &["scoped_db_for_owner_or_base_admin("],
        });
    }

    // Turning merge when ready on also admits the pull request's source owner —
    // without checking the target requirement, which the background merger
    // rechecks at merge time (`g5.governance.pr.source-owner-merge`, #1104).
    if file == "routes/workspaces.rs"
        && normalized == "workspace_id"
        && handler == "enable_workspace_pull_request_auto_merge"
    {
        return Some(AuthContract {
            scope: "workspace owner, Base administrator, or the pull request's source owner",
            guard_tokens: &["scoped_db_for_source_owner_merge("],
        });
    }

    if file == "routes/workspaces.rs" && normalized == "pull_request_id" {
        let guard_tokens = match handler {
            "merge_workspace_pull_request" => &["merge_workspace_pull_request"][..],
            "refresh_workspace_pull_request" => &["refresh_workspace_pull_request"][..],
            "close_workspace_pull_request" => &["close_workspace_pull_request"][..],
            "get_workspace_pull_request_description" => &["read_pull_request_description"][..],
            // Both lock the pull request only on the admitted workspace's side.
            "enable_workspace_pull_request_auto_merge" => {
                &["workspace_pull_request_auto_merge::enable("][..]
            }
            "disable_workspace_pull_request_auto_merge" => {
                &["workspace_pull_request_auto_merge::disable("][..]
            }
            _ => &[][..],
        };
        if !guard_tokens.is_empty() {
            return Some(AuthContract {
                scope: "workspace pull request path-workspace access",
                guard_tokens,
            });
        }
    }

    // The VM twin of the landing's description read: the service checks that
    // the incarnation's owner can read the pull request (`g5.git.pr-reads`).
    if file == "routes/internal.rs"
        && handler == "get_pull_request_description_internal"
        && normalized == "pull_request_id"
    {
        return Some(AuthContract {
            scope: "workspace pull request path-workspace access",
            guard_tokens: &["read_pull_request_description"],
        });
    }

    if review_edit_argument(file, handler, argument) {
        return Some(AuthContract {
            scope: "review edit within workspace-bound parent ledger",
            guard_tokens: &[
                "edit_thread_id",
                "add_reply_as",
                "reject_suggestion_as",
                "accept_suggestion_as",
            ],
        });
    }

    if file == "routes/workspace_reviews.rs" {
        match normalized.as_str() {
            // A suggestion is reached only through the path workspace: the
            // service join (`get_suggestion`) refuses ids of other workspaces;
            // the owner decisions run that lookup inside the shared `*_as`
            // functions (also used by the owner's agent, routes/internal).
            "suggestion_id" => {
                return Some(AuthContract {
                    scope: "review suggestion path-workspace access",
                    guard_tokens: &[
                        "get_suggestion",
                        "lock_open_suggestion",
                        "accept_suggestion_as",
                        "reject_suggestion_as",
                        "resolve_suggestion_as",
                        "add_reply_as",
                    ],
                })
            }
            // Publication and finding ids are list filters applied inside a
            // query already restricted to the path workspace.
            "publication_id" | "finding_id" => {
                return Some(AuthContract {
                    scope: "review publication filter under path-workspace access",
                    guard_tokens: &["list_suggestions"],
                })
            }
            _ => {}
        }
    }

    match normalized.as_str() {
        "org_id" => Some(AuthContract {
            scope: "org membership",
            guard_tokens: &[
                "scoped_db_for_org",
                "scoped_system_db_for_org",
                "scoped_db_for_user_orgs",
                "scoped_db_for_orgs",
                "scoped_db_for_actor_unscoped",
                "ensure_workspace_org_id",
                "start_cli_device_authorization",
                "check_scopes",
                "refresh_token",
                "import_repos",
                "list_scm_repos",
                "mint_db_capability",
                "mint_worker_db_capability",
                "require_internal",
            ],
        }),
        "workspace_id" => Some(AuthContract {
            scope: "workspace access",
            guard_tokens: &[
                "scoped_db_for_workspace",
                "scoped_system_db_for_workspace",
                "scoped_system_db_for_effective_scopes",
                "scoped_db_for_doc",
                "ensure_user_workspace_access",
                "check_workspace_access",
                "scoped_db_for_selected_workspace",
                "ensure_workspace_doc_id",
                "ensure_workspace_task_id",
                "mint_ws_ticket",
                "start_cli_device_authorization",
                "check_scopes",
                "require_worker_owns_active_workspace",
                "authorize_workspace_action",
                "open_owner_stream",
                "claims",
            ],
        }),
        "repo_id" => Some(AuthContract {
            scope: "repo ownership",
            guard_tokens: &[
                "ensure_workspace_repo_id",
                "scoped_db_for_workspace_repo",
                "ensure_org_repo_id",
                "require_read",
                "require_push",
                "require_push_ref_updates",
            ],
        }),
        "branch_id" | "workspace_branch_id" => Some(AuthContract {
            scope: "workspace branch access",
            guard_tokens: &[
                "scoped_db_for_branch",
                "scoped_db_for_workspace",
                "ensure_committed_draft_head",
                "ensure_workspace_branch_id",
                "resolve_branch_context",
                "resolve_ontology_upsert_branch_id",
                "AND workspace_id = $2",
            ],
        }),
        "merge_run_id" => Some(AuthContract {
            scope: "workspace branch merge-run access",
            guard_tokens: &["ensure_workspace_repo_merge_run_id"],
        }),
        "draft_id" => Some(AuthContract {
            scope: "draft workspace access",
            guard_tokens: &["scoped_db_for_draft", "resolve_draft_context"],
        }),
        "task_id" => Some(AuthContract {
            scope: "task workspace access",
            guard_tokens: &[
                "scoped_db_for_task",
                "resolve_task_context",
                "ensure_workspace_task_id",
                "require_owned_task_context",
                "require_task_owner",
                // Row-locking replacement for require_owned_task_context that
                // also enforces the worker session-generation fence (D46).
                "lock_and_authorize_owned_task_mutation",
                // The preview routes' shared owner/task/incarnation guard: it
                // opens `scoped_db_for_task` for the path task and resolves the
                // preview target before any ticket is minted or relayed.
                "resolve_workspace_preview_target",
            ],
        }),
        "doc_id" | "view_id" | "node_id" => Some(AuthContract {
            scope: "workspace document/node access",
            guard_tokens: &[
                "ensure_workspace_doc_id",
                "ensure_workspace_node_id",
                "resolve_linkable_node_ids",
                "scoped_db_for_doc",
                "resolve_doc_context",
                "scoped_db_for_workspace",
            ],
        }),
        "session_id" => Some(AuthContract {
            scope: "chat session workspace access",
            guard_tokens: &[
                "scoped_db_for_chat_session",
                // Read-only view access: workspace membership without owner gate
                // (chat session visibility contract, routes/chat D0).
                "scoped_db_for_chat_session_view",
                "resolve_chat_session_context",
                "ensure_task_session_id",
                "require_worker_owns_running_task_session",
                // Inbox-surface guard (issue #787, internal D14): running-task
                // lease OR active sticky-runtime owner OR claimed-by-caller.
                "require_worker_inbox_session_access",
                // Wrapper combining the inbox-surface guard with the worker
                // session-generation fence (internal D46).
                "open_authorized_worker_inbox_db",
                // Incarnation-scoped tenant connection for the session-file and
                // attachment routes: live incarnation plus the session's
                // workspace, verified on that connection (internal.rs).
                "scoped_db_for_session_file_incarnation",
                "require_session_in_workspace",
            ],
        }),
        "provider_session_id" => Some(AuthContract {
            scope: "Codex artifact provider session under authorized chat session",
            guard_tokens: &[
                "scoped_db_for_chat_session",
                "require_worker_owns_running_task_session",
                "scoped_db_for_session_file_incarnation",
            ],
        }),
        "generation" => Some(AuthContract {
            scope: "session bundle generation under authorized chat session",
            guard_tokens: &["scoped_db_for_session_file_incarnation"],
        }),
        "message_id" => Some(AuthContract {
            scope: "chat message session access",
            guard_tokens: &["ensure_session_message_id", "ensure_session_turn_id"],
        }),
        "question_id" => Some(AuthContract {
            scope: "question owning workspace access",
            guard_tokens: &["scoped_db_for_question"],
        }),
        "approval_id" => Some(AuthContract {
            scope: "chat approval session access",
            guard_tokens: &["ensure_session_approval_id"],
        }),
        "worker_id" => Some(AuthContract {
            scope: "worker identity or org access",
            guard_tokens: &[
                "require_worker_context",
                "require_worker_match",
                "require_worker_owns_",
                "scoped_db_for_worker",
            ],
        }),
        "host_id" => Some(AuthContract {
            scope: "worker-host org access",
            guard_tokens: &["scoped_db_for_worker_host", "scoped_db_for_org"],
        }),
        "key_id" | "token_id" | "member_id" | "invitation_id" | "notification_id"
        | "comment_id" | "user_id" | "file_id" | "attachment_id" => Some(AuthContract {
            scope: "owning resource access",
            guard_tokens: &[
                "scoped_db_for_org",
                "scoped_db_for_workspace",
                "scoped_db_for_comment",
                "scoped_db_for_notification",
                "ensure_user_workspace_access",
                "ensure_public_user_id",
                "ensure_assignee_belongs_to_org",
                "ensure_org_user_id",
                "ensure_org_user_ids",
                "ensure_workspace_user_ids",
                "ensure_workspace_member_id",
                "ensure_org_invitation_id",
                "ensure_user_invitation_id",
                "ensure_api_key_id",
                "resolve_comment_context",
                "resolve_notification_context",
                "resolve_org_member_context",
                "resolve_invitation_context",
            ],
        }),
        _ => None,
    }
}

fn service_level_terminal_reattach_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/mod.rs"
        || handler != "terminal_ws_adapter"
        || argument != "query.terminal_id"
    {
        return false;
    }
    // The ID is opaque to the route. The workspace-bound ticket and current
    // owner stream are authorized before the ID reaches that workspace's PTY
    // manager, which only reattaches an identity registered in its incarnation.
    let Some(ticket) = source.find("validate_websocket_ticket(&params.token, Some(workspace_id))")
    else {
        return false;
    };
    let Some(owner_stream) = source.find(".open_owner_stream(") else {
        return false;
    };
    let Some(scoped_db) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let Some(carry) = source.find("let terminal_id = params.terminal_id") else {
        return false;
    };
    let Some(send) = source.find("mux_writer.send(&open)") else {
        return false;
    };
    ticket < owner_stream
        && owner_stream < scoped_db
        && scoped_db < carry
        && carry < send
        && statement_window(source, owner_stream).contains("workspace_id")
        && statement_window(source, owner_stream).contains("user_id")
        && source.contains("Channel::Pty")
}

fn service_level_preview_reauth_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/auth.rs"
        || handler != "preview_reauth"
        || normalized_argument_name(file, handler, argument) != "task_id"
    {
        return false;
    }
    // The selected refresh cookie is the credential, so the read-only identity
    // resolver necessarily runs first and keys on the cookie, not the task.
    // The path task then passes the same owner/incarnation boundary as the
    // preview mint (`scoped_db_for_task`, workspaces:read, rolled back)
    // before a handoff ticket for that task is minted (routes/preview [D18]).
    let Some(identity) = source.find(".resolve_refresh_identity(refresh_token, None)") else {
        return false;
    };
    let Some(scoped_db) = source.find("scoped_db_for_task(&state, &auth, task_id") else {
        return false;
    };
    let Some(mint) = source.find(".mint_preview_handoff_ticket(") else {
        return false;
    };
    identity < scoped_db
        && scoped_db < mint
        && statement_window(source, scoped_db).contains("workspaces:read")
        && statement_window(source, mint).contains("task_id")
        && !source[..scoped_db].contains("mint_preview")
}

fn normalized_identifier_name(file: &str, handler: &str, identifier: &str) -> String {
    if identifier == "id" {
        infer_id_name(file, handler).to_string()
    } else {
        normalize_identifier_name(identifier).to_string()
    }
}

fn normalized_argument_name(file: &str, handler: &str, argument: &str) -> String {
    let identifier = argument.rsplit('.').next().unwrap_or(argument);
    normalized_identifier_name(file, handler, identifier)
}

fn collapse_route_params_covered_by_path_extractors(
    discovered: BTreeSet<DiscoveredArgument>,
) -> BTreeSet<DiscoveredArgument> {
    let path_arguments = discovered
        .iter()
        .filter(|arg| arg.argument.starts_with("path."))
        .map(|arg| {
            (
                arg.file.clone(),
                arg.handler.clone(),
                normalized_argument_name(&arg.file, &arg.handler, &arg.argument),
            )
        })
        .collect::<BTreeSet<_>>();

    discovered
        .into_iter()
        .filter(|arg| {
            if !arg.argument.starts_with("route.") {
                return true;
            }
            !path_arguments.contains(&(
                arg.file.clone(),
                arg.handler.clone(),
                normalized_argument_name(&arg.file, &arg.handler, &arg.argument),
            ))
        })
        .collect()
}

fn infer_id_name(file: &str, handler: &str) -> &'static str {
    match file {
        "routes/workspaces.rs" => "workspace_id",
        "routes/mod.rs" if handler == "terminal_ws_adapter" => "workspace_id",
        "routes/tasks.rs" => "task_id",
        "routes/chat.rs" => "session_id",
        "routes/comments.rs" => "comment_id",
        "routes/internal.rs" => infer_internal_id_name(handler),
        "routes/notifications.rs" => "notification_id",
        "routes/orgs.rs" => "org_id",
        "routes/users.rs" => "user_id",
        _ => "id",
    }
}

fn infer_internal_id_name(handler: &str) -> &'static str {
    match handler {
        "get_workspace_manifest_internal"
        | "list_issue_links_internal"
        | "get_linked_issue_internal"
        | "post_issue_comment_internal" => "workspace_id",
        "update_doc_content" => "doc_id",
        "get_chat_session_provider_state"
        | "update_chat_session_provider_state"
        | "complete_pending_chat_message"
        | "update_chat_user_message_content"
        | "chat_session_files_manifest"
        | "published_chat_session_file_bundle"
        | "begin_chat_session_file_bundle"
        | "publish_chat_session_file_bundle"
        | "chat_session_file_content"
        | "put_chat_session_file" => "session_id",
        _ => "task_id",
    }
}

fn normalize_identifier_name(identifier: &str) -> &str {
    match identifier {
        "dependencies" | "target_workspace_id" => "workspace_id",
        "target_workspace_branch_id" => "workspace_branch_id",
        "linked_node_ids"
        | "node_ids"
        | "target_nodes"
        | "parent_id"
        | "target_node_id"
        | "component_id_to_node_id" => "node_id",
        "target_doc_id" => "doc_id",
        // The organization the login landing names for the Connect GitHub offer.
        "after_login_org_id" => "org_id",
        "preferred_repo_id" | "target_repo_id" => "repo_id",
        "preview_target_workspace_id" => "workspace_id",
        "mention_user_ids" | "resolved_by_user_id" => "user_id",
        "mid" => "message_id",
        "created_by_session_id" => "session_id",
        "component_id_to_task_id" => "task_id",
        _ => identifier,
    }
}

fn protected_sink_tokens() -> &'static [&'static str] {
    &[
        ".authz",
        ".auth_service",
        ".permission_service",
        ".api_key_service",
        ".workspace_service",
        ".workspace_merge_service",
        ".task_service",
        ".doc_service",
        ".draft_service",
        ".comment_service",
        ".spend_service",
        ".notification_service",
        "questions::",
        ".home_service",
        "sqlx::query",
        ".fetch_one(",
        ".fetch_optional(",
        ".fetch_all(",
        ".execute(",
        ".handle_ws_upgrade(",
        ".on_upgrade(",
    ]
}

#[derive(Debug, Clone, Copy)]
enum AuthCheckPosition {
    Anywhere,
    BeforeFirstProtectedSink,
}

fn covering_auth_check_exists(
    source: &str,
    guard_tokens: &[impl AsRef<str>],
    sink_tokens: &[impl AsRef<str>],
    file: &str,
    handler: &str,
    argument: &str,
    position: AuthCheckPosition,
) -> bool {
    // Child authorization is the whole nested parent/ledger proof. A call
    // merely mentioning edit_id must not fall back to generic token coverage.
    if review_edit_argument(file, handler, argument) {
        let root = repo_root();
        let routes = strip_line_comments(
            &fs::read_to_string(root.join("g5-api-routes/src/routes/workspace_reviews.rs"))
                .unwrap(),
        );
        let service = strip_line_comments(
            &fs::read_to_string(
                root.join("g5-api-core/src/services/workspace/workspace_review_service.rs"),
            )
            .unwrap(),
        );
        return review_edit_binding_exists(source, file, handler, &routes, &service);
    }
    if service_level_worker_host_guard_exists(source, guard_tokens, file, handler, argument) {
        return true;
    }

    let candidates = argument_auth_value_candidates(file, handler, argument, true);
    let first_sink = earliest_token_index(source, sink_tokens).unwrap_or(source.len());

    guard_tokens.iter().any(|token| {
        token_indices(source, token.as_ref()).any(|token_index| {
            let window = statement_window(source, token_index);
            let window_start = source
                .find(window)
                .expect("statement window is a slice of source");
            let position_matches = match position {
                AuthCheckPosition::Anywhere => true,
                AuthCheckPosition::BeforeFirstProtectedSink => window_start <= first_sink,
            };
            position_matches && contains_any(window, &candidates)
        })
    })
}

fn service_level_worker_host_guard_exists(
    source: &str,
    guard_tokens: &[impl AsRef<str>],
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/worker_hosts.rs" {
        return false;
    }
    if handler == "capacity_stop_workspace" {
        let normalized = normalized_argument_name(file, handler, argument);
        if !matches!(normalized.as_str(), "workspace_id" | "operation_id")
            || !guard_tokens
                .iter()
                .any(|token| token.as_ref() == "scoped_db_for_org")
        {
            return false;
        }
        let Some(scope_index) = source.find("scoped_db_for_org") else {
            return false;
        };
        let Some(membership_index) = source.find("belongs_to_org") else {
            return false;
        };
        let Some(stop_index) = source.rfind(".stop_for_capacity(") else {
            return false;
        };
        return scope_index < membership_index
            && membership_index < stop_index
            && source.contains("body.workspace_id")
            && source.contains("body.operation_id")
            && source.contains("params.org_id");
    }
    // Both `:hostId` worker-host handlers authenticate the same way: they
    // `scoped_db_for_org(&state, &auth, params.org_id, …)` (org membership + RLS)
    // and then pass BOTH `params.host_id` and `params.org_id` into their service
    // sink, so the service call is the covering identifier check. The sink token
    // differs per handler — deallocate hits `deallocate_host`, the PR2 refresh
    // hits `get_host_for_runtime_refresh` (ontology [D43]).
    let service_sink = match handler {
        "deallocate_worker_host" => "remove_host",
        "refresh_worker_host_runtime_labels" => "get_host_for_runtime_refresh",
        "drain_worker_host" => "request_drain",
        _ => return false,
    };
    if normalized_argument_name(file, handler, argument) != "host_id" {
        return false;
    }
    if !guard_tokens
        .iter()
        .any(|token| token.as_ref() == "scoped_db_for_org")
    {
        return false;
    }

    let Some(scope_index) = source.find("scoped_db_for_org") else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    if !scope_window.contains("params.org_id") {
        return false;
    }

    let Some(sink_index) = source.find(service_sink) else {
        return false;
    };
    let sink_window = statement_window(source, sink_index);
    scope_index < sink_index
        && sink_window.contains("params.host_id")
        && sink_window.contains("params.org_id")
}

fn service_level_approval_review_admission_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspace_approvals.rs" {
        return false;
    }
    let normalized = normalized_argument_name(file, handler, argument);
    if matches!(
        handler,
        "get_review_report"
            | "get_review_commits"
            | "get_review_transcript"
            | "retry_review_cleanup"
    ) && (normalized == "review_id"
        || (handler == "get_review_commits" && normalized == "repo_id"))
    {
        let Some(scope_index) = source.find("scoped_db_for_workspace") else {
            return false;
        };
        let scope = statement_window(source, scope_index);
        let method = match handler {
            "get_review_report" => ".report(",
            "get_review_commits" => ".read_commits(",
            "get_review_transcript" => ".transcript(",
            _ => ".retry_cleanup(",
        };
        let Some(sink_index) = source.find(method) else {
            return false;
        };
        let sink = statement_window(source, sink_index);
        return scope_index < sink_index
            && scope.contains("path.workspace_id")
            && scope.contains(if handler != "retry_review_cleanup" {
                "workspaces:read"
            } else {
                "workspaces:write"
            })
            && sink.contains("approval_review_service")
            && sink.contains("&mut db")
            && sink.contains("path.workspace_id")
            && sink.contains("path.review_id")
            && (normalized != "repo_id" || sink.contains("repo_id: query.repo_id"));
    }
    if handler != "create_review"
        || !matches!(
            normalized.as_str(),
            "definition_id" | "retry_review_id" | "client_request_id"
        )
    {
        return false;
    }

    // Admission opens the path workspace's workspaces:write-scoped connection
    // before the body definition id is carried into the service request.
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    if !scope_window.contains("params.workspace_id") || !scope_window.contains("workspaces:write") {
        return false;
    }

    // ApprovalReviewService::admit receives the same scoped connection, path
    // workspace, and typed request. It resolves the definition under forced
    // tenant RLS before creating the stable request/task admission rows.
    let Some(carry_index) = source.find(&format!("{normalized}: body.{normalized}")) else {
        return false;
    };
    let Some(sink_index) = source.find(".admit(") else {
        return false;
    };
    let sink_window = statement_window(source, sink_index);
    scope_index < carry_index
        && carry_index < sink_index
        && compact(sink_window).contains("admit(db,")
        && sink_window.contains("params.workspace_id")
        && sink_window.contains("request")
}

fn service_level_member_change_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    file == "routes/orgs.rs"
        && matches!(handler, "update_member_role" | "remove_member")
        && matches!(
            normalized_argument_name(file, handler, argument).as_str(),
            "org_id" | "user_id"
        )
        && review_call_binding(
            source,
            "change_member",
            &[(0, "&state"), (1, "&auth"), (2, "&params")],
        )
        .is_some()
}

fn service_level_invitation_user_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/orgs.rs" {
        return false;
    }
    if normalized_argument_name(file, handler, argument) != "invitation_id" {
        return false;
    }
    let service_method = match handler {
        "accept_invitation" => "accept_invitation",
        "decline_invitation" => "decline_invitation",
        _ => return false,
    };

    let candidates = argument_auth_value_candidates(file, handler, argument, true);
    token_indices(source, service_method).any(|token_index| {
        let window = statement_window(source, token_index);
        window.contains(".org_service")
            && window.contains("user_id")
            && contains_any(window, &candidates)
    })
}

fn service_level_change_graph_node_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/source_control_change_graph.rs"
        || handler != "get_node_details"
        || normalized_argument_name(file, handler, argument) != "node_id"
    {
        return false;
    }

    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    if !scope_window.contains("params.workspace_id")
        || !scope_window.contains("\"workspaces:read\"")
    {
        return false;
    }

    let Some(details_index) = source.find(".node_details(") else {
        return false;
    };
    let details_window = statement_window(source, details_index);
    scope_index < details_index
        && details_window.contains("params.workspace_id")
        && details_window.contains("params.node_id")
        && details_window.contains("query.revision")
}

fn service_level_project_doc_mutation_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/docs.rs"
        || !matches!(handler, "update_project_doc" | "delete_project_doc")
        || normalized_argument_name(file, handler, argument) != "doc_id"
    {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let Some(parse_index) = source.find("parse_project_doc_handle") else {
        return false;
    };
    let Some(capture_index) = source.find("capture_docs_list") else {
        return false;
    };
    let Some(read_index) = source.find("get_doc_by_path") else {
        return false;
    };
    let Some(validate_index) = source.find("validate_project_doc_owner") else {
        return false;
    };
    let parse_window = statement_window(source, parse_index);
    let capture_window = statement_window(source, capture_index);
    parse_index < scope_index
        && scope_index < capture_index
        && capture_index < read_index
        && read_index < validate_index
        && parse_window.contains("params.doc_id")
        && capture_window.contains("params.workspace_id")
}

fn service_level_comment_identity_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/comments.rs" || handler != "create_comment" || argument != "body.id" {
        return false;
    }
    let service = include_str!("../../g5-api-core/src/services/workspace/comment_service.rs");
    source.contains("id: body.id")
        && source.contains(".create_comment(&mut db, params.workspace_id, user_id, service_req)")
        && source
            .find("scoped_db_for_workspace")
            .zip(source.find("let service_req"))
            .is_some_and(|(scope, request)| scope < request)
        && [
            "comment_index.workspace_id=EXCLUDED.workspace_id",
            "comment_index.author_id=EXCLUDED.author_id",
            "comment_index.scope=EXCLUDED.scope",
            "comment_index.content=EXCLUDED.content",
            "comment_index.commit_sha IS NOT DISTINCT FROM EXCLUDED.commit_sha",
        ]
        .iter()
        .all(|guard| service.contains(guard))
}

#[test]
fn comment_identity_guard_requires_scope_before_the_service_request() {
    let scope = "let db = scoped_db_for_workspace();";
    let request = "let service_req = CreateCommentRequest { id: body.id };";
    let call = ".create_comment(&mut db, params.workspace_id, user_id, service_req)";
    let guarded = |parts: &[&str]| {
        service_level_comment_identity_guard_exists(
            &parts.join("\n"),
            "routes/comments.rs",
            "create_comment",
            "body.id",
        )
    };
    assert!(guarded(&[scope, request, call]));
    assert!(!guarded(&[request, call]));
    assert!(!guarded(&[request, scope, call]));
    assert!(!guarded(&[scope, call]));
}

fn service_level_review_projection_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspace_review_flow.rs" || !matches!(handler, "changes" | "commits") {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let scope = statement_window(source, scope_index);
    if !scope.contains("p.workspace_id") || !scope.contains("\"workspaces:read\"") {
        return false;
    }
    let queries =
        include_str!("../../g5-api-core/src/services/workspace/workspace_review_queries.rs");
    let normalized = normalized_argument_name(file, handler, argument);
    match normalized.as_str() {
        "comment_id" => source.contains("queries::line_anchor(db.conn(), p.workspace_id, id)")
            && source.contains("q.repo_id != Some(repo)") && source.contains("q.path.as_deref() != Some(path.as_str())")
            && queries.contains("WHERE id=$1 AND workspace_id=$2 AND scope='line'"),
        "definition_id" => source.contains("queries::checkpoint(") && source.contains("q.definition_id") && source.contains("auth.require_user_id()?")
            && queries.contains("s.source_workspace_id=$1 AND s.actor_user_id=$2 AND s.approval_definition_id=$3 AND s.actor_type='human'")
            && queries.contains("p.status='open'"),
        "revision_id" => source.contains("queries::revision(") && source.contains("q.revision_id") && source.contains("auth.require_user_id()?")
            && queries.contains("authz.readable_parent_workspace(l.id,$4)") && queries.contains("e.id=$2 AND e.observed_at<=l.cut"),
        "repo_id" if handler == "commits" => source.contains("h.repo_id == repo_id.to_string()")
            && source.contains("queries::revision(") && source.contains("history::published_heads(") && source.contains("list_first_parent_history("),
        "repo_id" => source.contains("let repo_ids = after") && source.contains("q.repo_id.is_some_and(|id| id != repo_id)")
            && source.contains("running_owner") && source.contains(".review_inspect(") && source.contains(".or_else(|| bases.get(&repo_id).copied())"),
        _ => false,
    }
}

fn service_level_comment_repo_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/comments.rs"
        || normalized_argument_name(file, handler, argument) != "repo_id"
    {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    let service_method = match handler {
        "list_comments" => "list_comments",
        "create_comment" => "create_comment",
        _ => return false,
    };
    let Some(service_index) = source.rfind(service_method) else {
        return false;
    };
    scope_index < service_index
}

fn service_level_worker_task_transition_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/internal.rs"
        || handler != "task_status"
        || normalized_argument_name(file, handler, argument) != "task_id"
    {
        return false;
    }
    let Some(worker_index) = source.find("require_worker_context") else {
        return false;
    };
    let Some(transition_index) = source.find("run_worker_task_transition") else {
        return false;
    };
    worker_index < transition_index
        && statement_window(source, transition_index).contains("task_id")
}

fn service_level_workspace_open_question_metrics_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspaces.rs"
        || handler != "get_open_question_metrics"
        || normalized_argument_name(file, handler, argument) != "workspace_ids"
    {
        return false;
    }

    let Some(scope_index) = source.find("scoped_db_for_org") else {
        return false;
    };
    let Some(service_index) = source.find(".capture_open_question_metrics") else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    let service_window = statement_window(source, service_index);
    scope_index < service_index
        && scope_window.contains("params.org_id")
        && service_window.contains("params.org_id")
        && service_window.contains("&workspace_ids")
}

fn service_level_workspace_pull_request_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    // Creation is one typed service operation shared by UI and CLI. The public
    // adapter must forward this path workspace and its authenticated user.
    if file == "routes/workspaces.rs"
        && handler == "create_workspace_pull_request"
        && normalized_argument_name(file, handler, argument) == "workspace_id"
    {
        let Some(index) = source.find("workspace_pull_request_create::create_or_update(") else {
            return false;
        };
        let call = statement_window(source, index);
        return call.contains("PullRequestCaller::User(&auth)") && call.contains("params.id");
    }
    if file != "routes/workspaces.rs"
        || !matches!(
            handler,
            "merge_workspace_pull_request"
                | "refresh_workspace_pull_request"
                | "close_workspace_pull_request"
        )
        || normalized_argument_name(file, handler, argument) != "pull_request_id"
    {
        return false;
    }

    // The shared admission opens the owner's `workspaces:write` scope, or for
    // a pull request into Base an organization administrator's scope bound to
    // that pull request's target (`g5.governance.pr.base-admin`).
    let Some(target_scope_index) = source.find("scoped_db_for_owner_or_base_admin(") else {
        return false;
    };
    let service_call = format!(".{handler}(");
    let Some(service_index) = source.find(&service_call) else {
        return false;
    };
    let scope_window = statement_window(source, target_scope_index);
    let service_window = statement_window(source, service_index);

    target_scope_index < service_index
        && scope_window.contains("params.id")
        && scope_window.contains("BaseAdminAction::PullRequest(params.pull_request_id)")
        && service_window.contains("params.id")
        && service_window.contains("params.pull_request_id")
}

/// Provider session files are keyed by the chat session. The current
/// incarnation must match the live workspace, and the session must belong to
/// that workspace before a provider-session selector reaches storage.
fn service_level_chat_session_file_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file == "routes/internal.rs"
        && handler == "pending_session_file_bundles"
        && normalized_argument_name(file, handler, argument) == "session_id"
    {
        // The optional selector is applied inside a workspace-filtered query
        // after the caller's live workspace incarnation has been checked.
        return source
            .find("scoped_db_for_workspace_incarnation(")
            .is_some_and(|scope| {
                source.find(".pending_bundles(").is_some_and(|lookup| {
                    scope < lookup
                        && statement_window(source, lookup).contains("workspace_id")
                        && statement_window(source, lookup).contains("query.session_id")
                })
            });
    }
    let service_method = match handler {
        "chat_session_files_manifest" => ".manifest(",
        "published_chat_session_file_bundle" => ".published_bundle(",
        "begin_chat_session_file_bundle" => ".begin_bundle(",
        "publish_chat_session_file_bundle" => ".publish_bundle(",
        "chat_session_file_content" => ".load(",
        "put_chat_session_file" => ".upsert(",
        _ => return false,
    };
    if file != "routes/internal.rs"
        || !matches!(
            normalized_argument_name(file, handler, argument).as_str(),
            "session_id" | "provider_session_id" | "generation"
        )
    {
        return false;
    }

    let Some(owner_index) = source.find("scoped_db_for_session_file_incarnation(") else {
        return false;
    };
    let owner_window = statement_window(source, owner_index);
    if !owner_window.contains("chat_session_id") || !owner_window.contains("incarnation") {
        return false;
    }

    let Some(service_index) = source.find(service_method) else {
        return false;
    };
    let service_window = statement_window(source, service_index);
    let service_start = source[..service_index]
        .rfind("state")
        .unwrap_or(service_index);
    owner_index < service_index
        && source[service_start..service_index].contains(".chat_session_file_service")
        && service_window.contains("chat_session_id")
        && (normalized_argument_name(file, handler, argument) == "session_id"
            || service_window.contains("query.provider_session_id")
            || (normalized_argument_name(file, handler, argument) == "generation"
                && service_window.contains("query.generation"))
            || source.contains("provider_session_id: &query.provider_session_id"))
}

fn service_level_session_input_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/chat.rs" || normalized_argument_name(file, handler, argument) != "input_id" {
        return false;
    }
    let service_method = match handler {
        "edit_queued_input" => "edit_queued_session_input",
        "withdraw_queued_input" => "withdraw_queued_session_input",
        "redeliver_unknown_input" => "authorize_unknown_input_redelivery",
        "cancel_unknown_input" => "cancel_unknown_session_input",
        _ => return false,
    };
    let Some(scope_index) = source.find("scoped_db_for_chat_session") else {
        return false;
    };
    let Some(service_index) = source.find(service_method) else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    let service_window = statement_window(source, service_index);
    scope_index < service_index
        && scope_window.contains("params.id")
        && service_window.contains("params.id")
        && service_window.contains("params.input_id")
}

/// The owner's End names the task and attempt of the wait it saw only as an
/// expected identity: `end_session_background_wait` resolves the running
/// attempt of the path session, authorized by `scoped_db_for_chat_session`,
/// and refuses any other task or attempt.
fn service_level_background_end_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/chat.rs"
        || handler != "end_background_wait"
        || normalized_argument_name(file, handler, argument) != "task_id"
    {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_chat_session") else {
        return false;
    };
    let Some(service_index) = source.find("end_session_background_wait") else {
        return false;
    };
    let scope_window = statement_window(source, scope_index);
    let service_window = statement_window(source, service_index);
    scope_index < service_index
        && scope_window.contains("session_id")
        && service_window.contains("session_id")
        && service_window.contains("body.task_id")
}

fn service_level_native_reply_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/tasks.rs"
        || handler != "reply_to_provider_request"
        || !matches!(
            normalized_argument_name(file, handler, argument).as_str(),
            "request_id" | "provider_request_id"
        )
    {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_task") else {
        return false;
    };
    let Some(service_index) = source.find("reply_to_native_request") else {
        return false;
    };
    scope_index < service_index
        && source.contains("provider_request_id: path.provider_request_id")
        && source.contains("request_id: body.request_id")
}

fn service_level_incarnation_route_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    let normalized = normalized_argument_name(file, handler, argument);
    if file == "routes/workspace_flush.rs" {
        let Some(claims_index) = source.find("claims(&state, &headers, workspace_id)") else {
            return false;
        };
        return match normalized.as_str() {
            "workspace_id" => true,
            "flush_id" if handler == "complete" => source
                .find("load_for_verification(&claims, flush_id)")
                .is_some_and(|index| claims_index < index),
            "flush_id" if handler == "fail" => source
                .find(".fail(&claims, flush_id")
                .is_some_and(|index| claims_index < index),
            _ => false,
        };
    }
    if file != "routes/internal.rs" {
        return false;
    }
    match (handler, normalized.as_str()) {
        ("update_doc_content", "doc_id") => {
            let Some(read_index) = source.find("WHERE d.id = $1") else {
                return false;
            };
            let Some(guard_index) = source.find("require_live_workspace_incarnation") else {
                return false;
            };
            let Some(update_index) = source.find("UPDATE docs") else {
                return false;
            };
            read_index < guard_index
                && guard_index < update_index
                && source.contains(".bind(doc_id)")
        }
        _ => false,
    }
}

fn review_edit_argument(file: &str, handler: &str, argument: &str) -> bool {
    matches!(
        (file, handler, argument),
        (
            "routes/workspace_reviews.rs",
            "add_reply" | "reject_suggestion",
            "body.edit_id"
        ) | (
            "routes/workspace_reviews.rs",
            "list_replies",
            "query.edit_id"
        )
    )
}

fn compact(source: &str) -> String {
    source.split_whitespace().collect()
}

// Inspect argument positions with the scanner's existing balanced-call parser;
// identifier presence elsewhere in a statement is not a binding proof.
fn review_call_binding(source: &str, name: &str, bindings: &[(usize, &str)]) -> Option<usize> {
    let token = format!("{name}(");
    let binding = token_indices(source, &token).find(|&index| {
        let Some(call) = extract_balanced(source, index + name.len(), '(', ')') else {
            return false;
        };
        let args = split_top_level_args(&call[1..call.len() - 1]);
        bindings.iter().all(|(slot, expected)| {
            args.get(*slot)
                .is_some_and(|arg| compact(arg) == compact(expected))
        })
    });
    binding
}

fn review_edit_binding_exists(
    source: &str,
    file: &str,
    handler: &str,
    routes: &str,
    service: &str,
) -> bool {
    let body = |name| {
        extract_async_fn(service, name)
            .map(|(_, body)| body)
            .unwrap_or("")
    };
    let get = compact(body("get_suggestion"));
    let lock = body("lock_suggestion");
    // The loaded record belongs to both exact identifiers; failure precedes
    // returning it. Locking retains that same workspace/parent lookup.
    if !get.contains("WHEREp.source_workspace_id=$1ANDs.id=$2")
        || !get.contains(".bind(source_workspace_id).bind(suggestion_id).fetch_optional(db.conn()).await?.ok_or_else(||G5Error::NotFound")
        || review_call_binding(lock, "get_suggestion", &[(0, "db"), (1, "source_workspace_id"), (2, "suggestion_id")]).is_none()
    { return false; }
    let thread = compact(extract_named_fn_body(service, "edit_thread_id").unwrap_or(""));
    let ensure = compact(body("ensure_edit_thread"));
    let read_child = thread.contains("Some(id)=>suggestion.edits.iter().find(|e|e.id==id).map(|e|e.comment_id).ok_or_else(||G5Error::NotFound");
    let write_child = ensure.contains("letindex=suggestion.edits.iter().position(|e|e.id==edit_id).ok_or_else(||G5Error::NotFound")
        && ensure.find(".position(") < ensure.find("create_suggestion_thread(");
    let shared = |name| {
        extract_async_fn(routes, name)
            .map(|(_, body)| body)
            .unwrap_or("")
    };
    let reply = shared("add_reply_as");
    let reply_lock = review_call_binding(
        reply,
        "lock_suggestion",
        &[(0, "&mut db"), (1, "workspace_id"), (2, "suggestion_id")],
    );
    let reply_child = review_call_binding(
        reply,
        "ensure_edit_thread",
        &[
            (0, "&mut db"),
            (1, "workspace_id"),
            (2, "&mut suggestion"),
            (3, "&publication"),
            (4, "id"),
        ],
    );
    let reply_bound = write_child
        && reply_lock
            .zip(reply_child)
            .is_some_and(|(lock, child)| lock < child)
        && compact(reply).contains("ifletSome(id)=edit_id{")
        && reply_child
            .zip(reply.find("add_thread_reply("))
            .is_some_and(|(child, sink)| child < sink);
    let reject = shared("reject_suggestion_as");
    let reject_lock = review_call_binding(
        reject,
        "lock_suggestion",
        &[(0, "&mut db"), (1, "workspace_id"), (2, "suggestion_id")],
    );
    let rejected = compact(reject);
    let selection = "letselected:Vec<_>=suggestion.edits.iter().filter(|e|e.status==\"open\"&&edit_id.is_none_or(|id|id==e.id))";
    let reject_bound = write_child
        && reject_lock.is_some()
        && rejected.contains(selection)
        && rejected.find("lock_suggestion(&mutdb,workspace_id,suggestion_id)")
            < rejected.find(selection)
        && rejected.contains("ifselected.is_empty(){returnErr(")
        && rejected.find(selection) < rejected.find("ifselected.is_empty()")
        && rejected.find("ifselected.is_empty()") < rejected.find("letnow=");
    match (file, handler) {
        ("routes/workspace_reviews.rs", "list_replies") => {
            let scope = review_call_binding(
                source,
                "scoped_db_for_workspace",
                &[(1, "&auth"), (2, "params.workspace_id")],
            );
            let parent = review_call_binding(
                source,
                "get_suggestion",
                &[
                    (0, "&mut db"),
                    (1, "params.workspace_id"),
                    (2, "params.suggestion_id"),
                ],
            );
            let child = review_call_binding(
                source,
                "edit_thread_id",
                &[(0, "&suggestion"), (1, "query.edit_id")],
            );
            read_child
                && scope
                    .zip(parent)
                    .is_some_and(|(scope, parent)| scope < parent)
                && parent
                    .zip(child)
                    .is_some_and(|(parent, child)| parent < child)
                && child
                    .zip(source.find(".list_thread_replies("))
                    .is_some_and(|(child, sink)| child < sink)
        }
        ("routes/workspace_reviews.rs", "add_reply" | "reject_suggestion") => {
            let (name, slots, bound) = if handler == "add_reply" {
                (
                    "add_reply_as",
                    &[
                        (3, "params.workspace_id"),
                        (4, "params.suggestion_id"),
                        (6, "body.edit_id"),
                    ][..],
                    reply_bound,
                )
            } else {
                (
                    "reject_suggestion_as",
                    &[
                        (3, "params.workspace_id"),
                        (4, "params.suggestion_id"),
                        (6, "body.edit_id"),
                    ][..],
                    reject_bound,
                )
            };
            let scope = review_call_binding(
                source,
                "scoped_db_for_workspace",
                &[(1, "&auth"), (2, "params.workspace_id")],
            );
            bound
                && scope
                    .zip(review_call_binding(source, name, slots))
                    .is_some_and(|(scope, call)| scope < call)
        }
        _ => false,
    }
}

fn service_level_workspace_file_facade_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspaces.rs"
        || !matches!(
            normalized_argument_name(file, handler, argument).as_str(),
            "workspace_id" | "repo_id"
        )
    {
        return false;
    }
    let facade = match handler {
        "get_workspace_file" => "read_workspace_file_for_actor",
        "put_workspace_file" => "write_workspace_file_and_maybe_push",
        "delete_workspace_file" => "delete_workspace_file_for_actor",
        "rename_workspace_file" => "rename_workspace_file_for_actor",
        _ => return false,
    };
    source.contains(facade)
}

fn service_level_workspace_lifecycle_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspaces.rs" {
        return false;
    }
    let normalized = normalized_argument_name(file, handler, argument);
    if handler == "delete_workspace_action"
        && matches!(normalized.as_str(), "workspace_id" | "operation_id")
    {
        // Deletion now carries the authorized scope into the service that
        // reserves eligibility and rechecks the current owner before Stop.
        let Some(scope_index) = source.find("scoped_db_for_workspace") else {
            return false;
        };
        return source
            .find(".delete_workspace_with_stop(")
            .is_some_and(|index| scope_index < index)
            && source.contains("\"workspaces:write\"")
            && source.contains("db.reopener()")
            && source
                .contains("params.id,\n            user.user_uuid(),\n            operation_id,")
            && source
                .contains("let operation_id = body.operation_id.unwrap_or_else(Uuid::new_v4)");
    }
    if matches!(handler, "resume_workspace_vm" | "stop_workspace_vm")
        && matches!(normalized.as_str(), "workspace_id" | "operation_id")
    {
        return source.contains("authorize_workspace_action(&state, &auth, params.id)");
    }
    if handler == "transfer_workspace" && normalized == "to_user_id" {
        let Some(scope_index) = source.find("scoped_db_for_workspace") else {
            return false;
        };
        return source
            .find(".transfer(")
            .is_some_and(|index| scope_index < index)
            && source.contains("body.to_user_id");
    }
    false
}

fn service_level_workspace_preview_repo_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspace_preview.rs"
        || handler != "start_preview"
        || normalized_argument_name(file, handler, argument) != "repo_id"
    {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    source
        .find("start_workspace_preview")
        .is_some_and(|index| scope_index < index)
        && source.contains("repo_id: body.repo_id")
}

fn service_level_workspace_fork_input_guard_exists(
    source: &str,
    file: &str,
    handler: &str,
    argument: &str,
) -> bool {
    if file != "routes/workspaces.rs" || handler != "clone_workspace" {
        return false;
    }
    if !matches!(
        normalized_argument_name(file, handler, argument).as_str(),
        "chat_session_ids" | "merge_target_workspace_id"
    ) {
        return false;
    }
    let Some(scope_index) = source.find("scoped_db_for_workspace") else {
        return false;
    };
    source
        .find("workspace_fork_service")
        .is_some_and(|index| scope_index < index)
}

fn argument_is_used_in_handler(source: &str, file: &str, handler: &str, argument: &str) -> bool {
    let candidates = argument_auth_value_candidates(file, handler, argument, false);
    contains_any(source, &candidates)
}

fn argument_auth_value_candidates(
    file: &str,
    handler: &str,
    argument: &str,
    include_request_objects: bool,
) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    let mut parts = argument.split('.');
    let prefix = parts.next().unwrap_or_default();
    let identifier = parts.next().unwrap_or(argument);
    let normalized = normalized_identifier_name(file, handler, identifier);

    if identifier != "id" {
        candidates.insert(identifier.to_string());
        candidates.insert(normalized.clone());
    } else {
        candidates.insert(normalized.clone());
        candidates.insert("params.id".to_string());
    }

    match prefix {
        "body" => {
            for binder in ["body", "req", "request"] {
                candidates.insert(format!("{binder}.{identifier}"));
                candidates.insert(format!("{binder}.{normalized}"));
            }
            candidates.insert(format!("body_{normalized}"));
            candidates.insert(format!("raw_{normalized}"));
            if include_request_objects && body_identifier_can_cross_as_request_object(identifier) {
                candidates.extend(
                    ["body", "req", "request", "service_req", "svc_req"]
                        .into_iter()
                        .map(str::to_string),
                );
            }
        }
        "query" => {
            for binder in ["query", "params"] {
                candidates.insert(format!("{binder}.{identifier}"));
                candidates.insert(format!("{binder}.{normalized}"));
            }
            if include_request_objects {
                candidates.insert("filters".to_string());
            }
        }
        "path" => {
            for binder in ["params", "path"] {
                candidates.insert(format!("{binder}.{identifier}"));
                candidates.insert(format!("{binder}.{normalized}"));
            }
        }
        "route" => {
            if identifier == "id" {
                candidates.insert("params.id".to_string());
            }
            candidates.insert(format!("{normalized}_str"));
            candidates.insert(format!("{identifier}_str"));
            if normalized == "branch_id" {
                candidates.insert("workspace_branch_id".to_string());
                candidates.insert("branch_id_str".to_string());
            }
        }
        _ => {}
    }

    add_identifier_aliases(&normalized, &mut candidates);
    candidates.into_iter().collect()
}

fn body_identifier_can_cross_as_request_object(identifier: &str) -> bool {
    identifier.ends_with("_ids")
        || matches!(
            identifier,
            "dependencies"
                | "linked_node_ids"
                | "node_ids"
                | "node_id"
                | "parent_id"
                | "target_doc_id"
                | "target_node_id"
                | "target_nodes"
                | "preferred_repo_id"
                | "target_repo_id"
                | "workspace_branch_id"
                | "target_workspace_branch_id"
                | "repo_id"
                | "component_id_to_task_id"
                | "component_id_to_node_id"
        )
}

fn add_identifier_aliases(normalized: &str, candidates: &mut BTreeSet<String>) {
    match normalized {
        "workspace_id" => {
            candidates.insert("workspace_id".to_string());
            candidates.insert("workspace_id_str".to_string());
            candidates.insert("params.id".to_string());
            candidates.insert("params.workspace_id".to_string());
        }
        "branch_id" | "workspace_branch_id" => {
            candidates.insert("branch_id".to_string());
            candidates.insert("branch_id_str".to_string());
            candidates.insert("workspace_branch_id".to_string());
            candidates.insert("params.branch_id".to_string());
        }
        "org_id" => {
            candidates.insert("org_id".to_string());
            candidates.insert("body_org_id".to_string());
            candidates.insert("params.org_id".to_string());
        }
        "repo_id" => {
            candidates.insert("repo_id".to_string());
            candidates.insert("body.repo_id".to_string());
            candidates.insert("params.repo_id".to_string());
        }
        "task_id" => {
            candidates.insert("task_id".to_string());
            candidates.insert("params.id".to_string());
            candidates.insert("params.task_id".to_string());
        }
        "doc_id" => {
            candidates.insert("doc_id".to_string());
            candidates.insert("params.doc_id".to_string());
        }
        "node_id" => {
            candidates.insert("node_id".to_string());
            candidates.insert("params.node_id".to_string());
        }
        "session_id" => {
            candidates.insert("session_id".to_string());
            candidates.insert("params.id".to_string());
            candidates.insert("params.session_id".to_string());
        }
        "worker_id" => {
            candidates.insert("worker_id".to_string());
            candidates.insert("params.id".to_string());
            candidates.insert("params.worker_id".to_string());
        }
        _ => {}
    }
}

fn token_indices<'a>(source: &'a str, token: &'a str) -> impl Iterator<Item = usize> + 'a {
    source.match_indices(token).map(|(index, _)| index)
}

fn statement_window(source: &str, token_index: usize) -> &str {
    let start = source[..token_index]
        .rfind(';')
        .or_else(|| source[..token_index].rfind('{'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = source[token_index..]
        .find(';')
        .map(|offset| token_index + offset + 1)
        .unwrap_or_else(|| source.len().min(token_index + 2_000));
    &source[start..end]
}

fn contains_any(source: &str, candidates: &[String]) -> bool {
    candidates
        .iter()
        .any(|candidate| !candidate.is_empty() && source.contains(candidate))
}

/// True when every guard token appears before the first sink token that is
/// present. A body without any sink token passes; callers that protect a
/// required call assert its presence separately.
fn each_token_precedes_all(
    source: &str,
    guard_tokens: &[impl AsRef<str>],
    sink_tokens: &[impl AsRef<str>],
) -> bool {
    let Some(first_sink) = earliest_token_index(source, sink_tokens) else {
        return true;
    };
    guard_tokens.iter().all(|token| {
        source
            .find(token.as_ref())
            .is_some_and(|token_index| token_index <= first_sink)
    })
}

fn earliest_token_index(source: &str, tokens: &[impl AsRef<str>]) -> Option<usize> {
    tokens
        .iter()
        .filter_map(|token| source.find(token.as_ref()))
        .min()
}

fn load_known_auth_gaps(path: &Path) -> Vec<KnownAuthGap> {
    if !path.exists() {
        return Vec::new();
    }
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "failed to read known route auth gaps at {}: {err}",
            path.display()
        )
    }))
    .expect("known route auth gaps must be valid JSON")
}

fn write_known_auth_gaps(path: &Path, findings: &BTreeSet<RouteAuthFinding>) {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct GeneratedKnownGap<'a> {
        #[serde(flatten)]
        finding: &'a RouteAuthFinding,
        note: &'static str,
    }

    let generated = findings
        .iter()
        .map(|finding| GeneratedKnownGap {
            finding,
            note: "Current strict handler-first auth backlog. Do not remove without fixing the route or documenting a narrower auth boundary.",
        })
        .collect::<Vec<_>>();
    let json =
        serde_json::to_string_pretty(&generated).expect("known route auth gaps must serialize");
    fs::write(path, format!("{json}\n")).unwrap_or_else(|err| {
        panic!(
            "failed to write known route auth gaps at {}: {err}",
            path.display()
        )
    });
}

fn format_findings(findings: &[RouteAuthFinding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "  {}::{} {} [{} / {}]",
                finding.file,
                finding.handler,
                finding.argument,
                finding.normalized_identifier,
                finding.issue
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api has backend parent")
        .to_path_buf()
}

fn route_source_files(routes_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(routes_dir)
        .expect("routes directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter(|path| {
            !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("audited.rs" | "guards.rs")
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

fn discover_route_handlers(source: &str, rel_file: &str) -> Vec<RouteHandler> {
    let mut handlers = Vec::new();
    for call_name in ["audited_route", "exempt_route", "route"] {
        let needle = format!("{call_name}(");
        let mut search_from = 0;
        while let Some(offset) = source[search_from..].find(&needle) {
            let call_name_start = search_from + offset;
            if call_name == "route" && !is_raw_route_call(source, call_name_start) {
                search_from = call_name_start + call_name.len();
                continue;
            }

            let call_start = call_name_start + call_name.len();
            let Some(call) = extract_balanced(source, call_start, '(', ')') else {
                break;
            };
            let args = split_top_level_args(&call[1..call.len() - 1]);
            if args.len() >= 2 && args[0].trim_start().starts_with('"') {
                let route_params = route_params(args[0]);
                let route_kind = match call_name {
                    "audited_route" => RouteRegistrationKind::Audited,
                    "exempt_route" => RouteRegistrationKind::Exempt,
                    "route" => RouteRegistrationKind::Raw,
                    _ => unreachable!(),
                };
                let registered_scopes = if route_kind == RouteRegistrationKind::Audited {
                    args.get(2)
                        .map(|arg| parse_scope_tokens(arg))
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let exemption_reason = if route_kind == RouteRegistrationKind::Exempt {
                    args.get(2).map(|arg| string_literal_value(arg))
                } else {
                    None
                };
                for (method, handler) in method_handlers(args[1]) {
                    handlers.push(RouteHandler {
                        file: rel_file.to_string(),
                        handler,
                        method,
                        route_kind,
                        registered_scopes: registered_scopes.clone(),
                        exemption_reason: exemption_reason.clone(),
                        route_params: route_params.clone(),
                    });
                }
            }
            search_from = call_start + call.len();
        }
    }
    handlers.sort_by(|a, b| {
        (
            &a.file,
            &a.handler,
            &a.method,
            a.route_kind,
            &a.registered_scopes,
        )
            .cmp(&(
                &b.file,
                &b.handler,
                &b.method,
                b.route_kind,
                &b.registered_scopes,
            ))
    });
    handlers.dedup_by(|a, b| a.file == b.file && a.handler == b.handler && a.method == b.method);
    handlers
}

fn is_raw_route_call(source: &str, call_name_start: usize) -> bool {
    source[..call_name_start]
        .chars()
        .last()
        .is_some_and(|ch| ch == '.')
}

fn route_params(path_arg: &str) -> Vec<String> {
    let trimmed = path_arg.trim().trim_matches('"');
    trimmed
        .split('/')
        .filter_map(|part| {
            part.strip_prefix(':')
                .or_else(|| part.strip_prefix('*'))
                .map(to_snake_case)
        })
        .collect()
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn method_handlers(method_expr: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"\b(get|post|put|patch|delete|head)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid method handler regex");
    re.captures_iter(method_expr)
        .map(|cap| (cap[1].to_string(), cap[2].to_string()))
        .collect()
}

fn parse_scope_tokens(scope_expr: &str) -> Vec<String> {
    let re = Regex::new(r"Scope::([A-Za-z_][A-Za-z0-9_]*)").expect("valid scope token regex");
    re.captures_iter(scope_expr)
        .map(|cap| format!("Scope::{}", &cap[1]))
        .collect()
}

fn string_literal_value(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_structs(source: &str) -> BTreeMap<String, Vec<StructField>> {
    let struct_re = Regex::new(r"(?s)(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{(.*?)\}")
        .expect("valid struct regex");
    let field_re = Regex::new(r"(?:pub\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^,\n]+)")
        .expect("valid field regex");

    let mut structs = BTreeMap::new();
    for cap in struct_re.captures_iter(source) {
        let name = cap[1].to_string();
        let body = &cap[2];
        let fields = field_re
            .captures_iter(body)
            .map(|field| StructField {
                name: field[1].to_string(),
                ty: field[2].trim().to_string(),
            })
            .collect::<Vec<_>>();
        structs.insert(name, fields);
    }
    structs
}

fn extract_async_fn<'a>(source: &'a str, handler: &str) -> Option<(&'a str, &'a str)> {
    let needle = format!("async fn {handler}(");
    let start = source.find(&needle)?;
    let brace = source[start..].find('{')? + start;
    let signature = &source[start..brace];
    let body = extract_balanced(source, brace, '{', '}')?;
    Some((signature, &source[brace..brace + body.len()]))
}

fn assert_function_contains_all_in_order(
    path: &Path,
    fn_name: &str,
    tokens: &[&str],
    must_precede: &[&str],
) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let body = extract_named_fn_body(&source, fn_name).unwrap_or_else(|| {
        panic!(
            "failed to find function body {fn_name} in {}",
            path.display()
        )
    });
    for token in tokens {
        assert!(
            body.contains(token),
            "{}::{fn_name} is missing required auth guard token `{token}`",
            path.display()
        );
    }
    assert!(
        earliest_token_index(body, must_precede).is_some(),
        "{}::{fn_name} no longer contains any protected call {:?}; it moved or was \
         renamed, so point this check at it",
        path.display(),
        must_precede
    );
    assert!(
        each_token_precedes_all(body, tokens, must_precede),
        "{}::{fn_name} must check {:?} before {:?}",
        path.display(),
        tokens,
        must_precede
    );
}

fn extract_named_fn_body<'a>(source: &'a str, fn_name: &str) -> Option<&'a str> {
    let needle = format!("fn {fn_name}");
    for (start, _) in source.match_indices(&needle) {
        if source[start + needle.len()..].starts_with(|ch: char| ch.is_alphanumeric() || ch == '_')
        {
            continue;
        }
        let rest = &source[start..];
        let brace_offset = rest.find('{')?;
        let semicolon_offset = rest.find(';');
        if semicolon_offset.is_some_and(|offset| offset < brace_offset) {
            continue;
        }
        let brace = start + brace_offset;
        let body = extract_balanced(source, brace, '{', '}')?;
        return Some(&source[brace..brace + body.len()]);
    }
    None
}

fn extractor_types(signature: &str) -> Vec<(&'static str, String)> {
    let parameters = signature
        .find(") ->")
        .map(|end| &signature[..end])
        .unwrap_or(signature);
    let re = Regex::new(r"\b(Path|Query|Json)\s*<\s*([A-Za-z_][A-Za-z0-9_]*)\s*>")
        .expect("valid extractor regex");
    re.captures_iter(parameters)
        .map(|cap| {
            let prefix = match &cap[1] {
                "Path" => "path",
                "Query" => "query",
                "Json" => "body",
                _ => unreachable!(),
            };
            (prefix, cap[2].to_string())
        })
        .collect()
}

fn is_sensitive_identifier(name: &str, ty: &str) -> bool {
    // Semantic block ids and an observed VM version are selectors/concurrency
    // tokens within an already-authorized document or workspace, not separate
    // tenant resources that need route-level secondary-id authorization.
    if matches!(name, "start_block_id" | "expected_vm_id") {
        return false;
    }
    if name == "target_nodes" {
        return true;
    }
    if name == "id" || name.ends_with("_id") || name.ends_with("_ids") {
        return true;
    }
    ty.contains("Uuid") && !matches!(name, "user_code" | "device_code")
}

fn extract_balanced(source: &str, open_index: usize, open: char, close: char) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let slice = &source[open_index..];

    for (idx, ch) in slice.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(&slice[..=idx]);
            }
        }
    }
    None
}

fn split_top_level_args(input: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut paren = 0isize;
    let mut brace = 0isize;
    let mut bracket = 0isize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            ',' if paren == 0 && brace == 0 && bracket == 0 => {
                args.push(input[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < input.len() {
        args.push(input[start..].trim());
    }
    args
}
