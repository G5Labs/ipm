//! Compile-time and source-level audit guards.
//!
//! These tests scan the source code itself to catch structural violations
//! that would be invisible at the type level but dangerous in production.
//!
//! # What these tests catch
//!
//! 1. **Unguarded handlers**: Any `async fn` that takes `State<Arc<AppState>>`
//!    but does NOT extract `AuthContext`, `AuthenticatedUser`, or
//!    `AuthenticatedActor` via an Axum extractor. Handlers that authenticate
//!    through other means (e.g. capability tokens) belong in `EXEMPT_HANDLERS`.
//!
//! 2. **Routes without scope enforcement**: Any route registration
//!    (`get(handler)`, `post(handler)`, etc.) that doesn't have a
//!    `RequireScopes` layer and isn't in a router that uses `route_layer`.
//!
//! 3. **No panics in production code**: `.unwrap()` and `.expect()` in
//!    handler/service code kill the request (or worse, the process). The
//!    guard scans `routes/` and `services/` for these outside test blocks.
//!
//! 4. **No direct DB access in handlers**: Route files should delegate to
//!    services, not call `sqlx::query` directly. This check lives in
//!    `backend/g5-api/tests/route_sql_guard.rs`, which allows only named
//!    route functions.
//!
//! 5. **Typed ID confusion**: Handlers that accept `Path<Uuid>` where a
//!    typed ID (`WorkspaceId`, `TaskId`, etc.) exists risk mixing up IDs.
//!
//! These tests run in CI and will **fail loudly** if someone adds a new
//! endpoint without proper auth.

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    /// Files exempt from Guard 1 (all_handlers_extract_auth).
    ///
    /// All route files now use `AuditedRouter`, so Guard 2 (scope enforcement)
    /// passes structurally — `exempt_route()` forces a documented reason for
    /// every unscoped route. This list is only needed for Guard 1: files whose
    /// handlers legitimately lack auth extractors in their signatures.
    const EXEMPT_FILES: &[&str] = &[
        // Auth routes: login/callback/issue_test_token run before JWT exists.
        "auth.rs",
        // Git smart-HTTP routes authenticate via GitAuth (Basic/Bearer) and
        // authorize inline per repo/ref instead of using AuthContext scopes.
        "git.rs",
        // Instance routes: health_check, get_instance_info are public.
        "instance.rs",
        // Self-hosted delivery routes authenticate with their own bearer
        // tokens: admin token hash for license creation, license key hash for
        // registry exchange.
        "self_hosted.rs",
        // mod.rs: top-level build_router() — assembles sub-routers, not a
        // route file itself. WebSocket adapters use query-param token auth.
        "mod.rs",
        // Guards.rs: test infrastructure, not a route file.
        "guards.rs",
        // browser_origin.rs: helpers module (normalize_browser_origin and
        // related utilities), not a route file. No HTTP handlers; just lives
        // under routes/ for proximity to its callers.
        "browser_origin.rs",
        // start_and_send.rs: the Start-and-send step shared by chat.rs's
        // send and session-start handlers, which enforce their scopes. No
        // HTTP handlers or router of its own.
        "start_and_send.rs",
    ];

    /// Auth extractor types that prove a handler is authenticated.
    const AUTH_EXTRACTORS: &[&str] = &[
        "AuthContext",
        "AuthenticatedUser",
        "AuthenticatedActor",
        // Worker-host JWT extractor (Scope::WorkerHost): verifies the host
        // session JWT and exposes the authenticated host id. Used by host-facing
        // internal endpoints such as the preview reverse-tunnel attach.
        "AuthenticatedHost",
        // Internal workspace routes receive this extension only after the
        // outer auth middleware validates the signed incarnation JWT against
        // an explicitly allowlisted method and path shape.
        "WorkspaceIncarnationContext",
    ];

    /// Handlers that authenticate via mechanisms other than JWT — e.g.
    /// HMAC-signed state verified inside the handler body rather than via
    /// an Axum extractor. These still have auth; the guard test just can't
    /// see it in the function signature.
    /// Format: `("filename.rs", "function_name")`
    const EXEMPT_HANDLERS: &[(&str, &str)] = &[
        // GitHub redirects the browser here without a G5 session. The
        // handlers verify HMAC-signed state against the stored attempt
        // digest, a signed PKCE cookie and GitHub's installation-access
        // proof, and re-check the attempt creator is an org admin.
        ("github_app.rs", "github_app_setup_callback"),
        ("github_app.rs", "github_app_reconnect_selection"),
        ("github_app.rs", "github_app_oauth_callback"),
    ];

    /// Scan all route handler functions and verify they extract an auth type.
    ///
    /// This test reads the actual source files in `src/routes/` and checks
    /// that every `async fn` with `State<Arc<AppState>>` also has one of
    /// the auth extractors in its parameter list.
    ///
    /// If this test fails, you either need to:
    /// 1. Add an auth extractor to your handler, or
    /// 2. Add the file to `EXEMPT_FILES` with a comment explaining why.
    #[test]
    fn all_handlers_extract_auth() {
        let routes_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/routes");
        let mut violations = Vec::new();

        for entry in std::fs::read_dir(routes_dir).expect("routes/ dir must exist") {
            let entry = entry.expect("readable dir entry");
            let path = entry.path();

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip non-Rust files and exempt files
            if !file_name.ends_with(".rs") || EXEMPT_FILES.contains(&file_name) {
                continue;
            }

            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Failed to read {}", path.display()));

            // Find handler functions: `async fn` with `State<Arc<AppState>>`
            // We scan line-by-line, accumulating multi-line signatures.
            let lines: Vec<&str> = source.lines().collect();
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i];

                // Detect handler function start
                if line.contains("async fn") && !line.trim_start().starts_with("//") {
                    // Accumulate the full signature (may span multiple lines)
                    let mut signature = String::new();
                    let start_line = i + 1; // 1-based
                    while i < lines.len() {
                        signature.push_str(lines[i]);
                        signature.push('\n');
                        if lines[i].contains('{') || lines[i].contains("->") {
                            // If we hit the return type or body, check if we
                            // also found the closing paren
                            if signature.contains(')') {
                                break;
                            }
                        }
                        i += 1;
                    }

                    // Only check handlers that receive AppState (route handlers)
                    if signature.contains("State<Arc<AppState>>") {
                        let has_auth = AUTH_EXTRACTORS.iter().any(|ext| signature.contains(ext));

                        if !has_auth {
                            // Extract function name
                            let fn_name = signature
                                .split("async fn ")
                                .nth(1)
                                .and_then(|s| s.split('(').next())
                                .unwrap_or("unknown")
                                .trim();

                            // Check handler-level exemptions
                            let is_exempt = EXEMPT_HANDLERS
                                .iter()
                                .any(|(f, h)| *f == file_name && *h == fn_name);

                            if !is_exempt {
                                violations.push(format!(
                                    "  {}:{} — fn `{}` has State<Arc<AppState>> but no auth extractor",
                                    file_name, start_line, fn_name
                                ));
                            }
                        }
                    }
                }

                i += 1;
            }
        }

        if !violations.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  UNGUARDED HANDLERS DETECTED — AUTH EXTRACTOR MISSING!     ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  Every handler that takes State<Arc<AppState>> MUST also   ║\n\
                 ║  extract AuthContext, AuthenticatedUser, or                ║\n\
                 ║  AuthenticatedActor. Otherwise it runs unauthenticated.    ║\n\
                 ║                                                            ║\n\
                 ║  Fix: add `user: AuthenticatedUser` to handler params.     ║\n\
                 ║  Or: add file to EXEMPT_FILES in guards.rs with reason.    ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Violations:\n{}\n",
                violations.join("\n")
            );
        }
    }

    /// Verify that each non-exempt route file uses RequireScopes or
    /// route_layer somewhere — catching files where someone adds routes
    /// without any scope enforcement.
    #[test]
    fn all_route_files_enforce_scopes() {
        let routes_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/routes");
        let mut violations = Vec::new();

        // Files that legitimately have no RequireScopes
        let mut scope_exempt: HashSet<&str> = EXEMPT_FILES.iter().copied().collect();
        // Flush routes are reachable only through the global middleware's
        // exact method/path allowlist for a signed workspace-incarnation JWT.
        // Each handler extracts WorkspaceIncarnationContext and rechecks the
        // workspace id; an OAuth scope layer would admit the wrong principal.
        scope_exempt.insert("workspace_flush.rs");
        // The override preview and merge handlers are registered with
        // AuditedRouter scopes in workspaces.rs (preview WorkspacesRead,
        // merge WorkspacesWrite); the module has no router of its own. The
        // handler-authentication and typed-id guards still check it.
        scope_exempt.insert("workspace_merge_override.rs");
        // These modules register no HTTP routes. ontology_files.rs is an internal
        // inventory helper: its callers use AuditedRouter, and it checks workspace
        // read scope before delegating to the actor-aware Files reader. Keep the
        // separate handler-authentication guard active for these helpers.
        // change_graph_live.rs captures saved workspace content for the change
        // graph's AuditedRouter handlers and rechecks workspace read scope itself.
        // cookies.rs selects browser credentials; it registers no HTTP handlers.
        let infra_files: HashSet<&str> = [
            "audited.rs",
            "ontology_files.rs",
            "change_graph_live.rs",
            "cookies.rs",
        ]
        .into_iter()
        .collect();

        for entry in std::fs::read_dir(routes_dir).expect("routes/ dir must exist") {
            let entry = entry.expect("readable dir entry");
            let path = entry.path();

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if !file_name.ends_with(".rs")
                || scope_exempt.contains(file_name)
                || infra_files.contains(file_name)
            {
                continue;
            }

            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Failed to read {}", path.display()));

            // Check that the file references RequireScopes or route_layer or AuditedRouter
            let has_scope_enforcement = source.contains("RequireScopes")
                || source.contains("route_layer")
                || source.contains("AuditedRouter")
                || source.contains("audited_route");

            if !has_scope_enforcement {
                violations.push(format!(
                    "  {} — no RequireScopes, route_layer, or AuditedRouter found",
                    file_name
                ));
            }
        }

        if !violations.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  ROUTE FILES WITHOUT SCOPE ENFORCEMENT DETECTED!           ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  Every route file must use RequireScopes, route_layer, or  ║\n\
                 ║  AuditedRouter to enforce authorization.                   ║\n\
                 ║                                                            ║\n\
                 ║  Fix: add RequireScopes to your routes.                    ║\n\
                 ║  Or: add file to EXEMPT_FILES in guards.rs with reason.    ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Violations:\n{}\n",
                violations.join("\n")
            );
        }
    }

    /// Verify that EXEMPT_FILES entries actually correspond to files that
    /// exist — catches stale exemptions after file renames.
    #[test]
    fn exempt_files_all_exist() {
        let routes_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/routes");
        let existing: HashSet<String> = std::fs::read_dir(routes_dir)
            .expect("routes/ dir must exist")
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();

        let missing: Vec<&&str> = EXEMPT_FILES
            .iter()
            .filter(|f| !existing.contains(**f))
            .collect();

        if !missing.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  STALE EXEMPT_FILES ENTRIES — FILES NO LONGER EXIST!       ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  The following EXEMPT_FILES entries in guards.rs refer to  ║\n\
                 ║  files that no longer exist. Remove them.                  ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Stale entries: {:?}\n",
                missing
            );
        }

        // Also verify EXEMPT_HANDLERS reference files that exist
        let missing_handler_files: Vec<&(&str, &str)> = EXEMPT_HANDLERS
            .iter()
            .filter(|(f, _)| !existing.contains(*f))
            .collect();

        if !missing_handler_files.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  STALE EXEMPT_HANDLERS ENTRIES — FILES NO LONGER EXIST!   ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  The following EXEMPT_HANDLERS entries reference files     ║\n\
                 ║  that no longer exist. Remove them.                        ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Stale entries: {:?}\n",
                missing_handler_files
            );
        }
    }

    // =====================================================================
    // Guard 4: No .unwrap() / .expect() in production handler/service code
    // =====================================================================

    /// Known-safe `.unwrap()` / `.expect()` call sites that are either:
    /// - Infallible by construction (static regex, write! to String)
    /// - Startup-only (client builders, config init)
    /// - Date arithmetic on known-valid inputs
    ///
    /// Format: `("dir/file.rs", "unique substring of the line")`.
    ///
    /// Content-based matching — immune to line-number drift from unrelated edits.
    const UNWRAP_EXEMPT: &[(&str, &str)] = &[
        // write!(String, …).unwrap() — infallible (writing to a String never fails)
        (
            "routes/internal.rs",
            r#"write!(hex, "{:02x}", byte).unwrap()"#,
        ),
        // Static LazyLock<Regex> — infallible constant pattern, compiled once at first use
        (
            "services/identity/notification_service.rs",
            r#"Regex::new(r"@(\w[\w.-]{0,38})").unwrap()"#,
        ),
        // reqwest::Client::builder().build().expect() — startup-only static
        // bounded OCI resolver config; fail closed if construction ever fails.
        (
            "services/hosts/runtime_label_resolver.rs",
            r#".expect("bounded OCI resolver HTTP client builds from static config")"#,
        ),
        // reqwest::Client::builder().build().expect() — startup-only static
        // bounded GitHub provider client config; fail closed if construction ever fails.
        (
            "services/git/workspace_pull_request_provider_lifecycle.rs",
            r#".expect("bounded GitHub provider HTTP client builds from static config")"#,
        ),
        // AppState::new() builder — startup-only, no recovery possible
        ("services/mod.rs", r#".expect("repo storage must build"#),
        // Constant RFC3339 string parse — infallible ("total" period)
        (
            "services/identity/spend_service.rs",
            r#".expect("constant RFC3339 string")"#,
        ),
    ];

    /// Scan `routes/` and `services/` for `.unwrap()` and `.expect()` outside
    /// of test code. Panics in production code crash requests and can take
    /// down the whole process.
    #[test]
    fn no_unwrap_in_production_code() {
        let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut violations = Vec::new();

        for subdir in &["routes", "services"] {
            let dir = format!("{}/{}", src_dir, subdir);
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries {
                let entry = entry.expect("readable dir entry");
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if !file_name.ends_with(".rs") {
                    continue;
                }

                let rel_path = format!("{}/{}", subdir, file_name);
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|_| panic!("Failed to read {}", path.display()));

                let mut in_test_block = false;

                for (i, line) in source.lines().enumerate() {
                    let line_num = (i + 1) as u32;
                    let trimmed = line.trim();

                    // Once we hit #[cfg(test)], everything below is test code
                    if trimmed == "#[cfg(test)]" {
                        in_test_block = true;
                    }
                    if in_test_block {
                        continue;
                    }

                    // Skip comments
                    if trimmed.starts_with("//") {
                        continue;
                    }

                    if line.contains(".unwrap()") || line.contains(".expect(") {
                        // Check exemption list (content-based matching)
                        let is_exempt = UNWRAP_EXEMPT
                            .iter()
                            .any(|(f, pattern)| *f == rel_path && line.contains(pattern));

                        if !is_exempt {
                            violations.push(format!(
                                "  {}:{} — {}",
                                rel_path,
                                line_num,
                                trimmed.chars().take(80).collect::<String>()
                            ));
                        }
                    }
                }
            }
        }

        if !violations.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  .unwrap() / .expect() IN PRODUCTION CODE!                ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  These calls will PANIC and crash the request (or worse,   ║\n\
                 ║  the process) in production. Use ? operator, .ok(),        ║\n\
                 ║  .unwrap_or(), or .map_err() instead.                      ║\n\
                 ║                                                            ║\n\
                 ║  If genuinely infallible, add to UNWRAP_EXEMPT with a      ║\n\
                 ║  comment explaining why it can't fail.                     ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Violations:\n{}\n",
                violations.join("\n")
            );
        }
    }

    // =====================================================================
    // Guard 5: No direct sqlx in route handlers — moved to
    // `backend/g5-api/tests/route_sql_guard.rs`, which attributes each call to
    // its enclosing function with a Rust parser and allows only named
    // functions.
    // =====================================================================

    // =====================================================================
    // Guard 6: Typed ID confusion — flag Path<Uuid> for known entity types
    // =====================================================================

    /// Maps variable-name suffixes to the typed ID that should be used instead
    /// of raw `Uuid`. If a handler has `Path(workspace_id): Path<Uuid>`, this
    /// guard will flag it and suggest `WorkspaceId`.
    const TYPED_ID_MAP: &[(&str, &str)] = &[
        ("workspace_id", "WorkspaceId"),
        ("org_id", "OrgId"),
        ("user_id", "UserId"),
        ("task_id", "TaskId"),
        ("session_id", "SessionId"),
        ("branch_id", "BranchId"),
        ("comment_id", "CommentId"),
        ("message_id", "MessageId"),
        ("node_id", "NodeId"),
        ("repo_id", "RepoId"),
    ];

    /// Route files where raw `Path<Uuid>` is tolerated because the typed
    /// IDs aren't wired into the service layer yet.
    const TYPED_ID_EXEMPT_FILES: &[&str] = &[
        // Internal workspace-incarnation endpoints use raw UUIDs throughout;
        // the authenticated incarnation claim supplies their authority fence.
        "internal.rs",
    ];

    /// Scan handler signatures for `Path<Uuid>` where the variable name
    /// indicates a typed ID should be used. Prevents accidentally swapping
    /// a workspace ID for a task ID, etc.
    #[test]
    fn no_raw_uuid_for_typed_ids() {
        let routes_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/routes");
        let mut violations = Vec::new();

        for entry in std::fs::read_dir(routes_dir).expect("routes/ dir must exist") {
            let entry = entry.expect("readable dir entry");
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if !file_name.ends_with(".rs")
                || EXEMPT_FILES.contains(&file_name)
                || TYPED_ID_EXEMPT_FILES.contains(&file_name)
            {
                continue;
            }

            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Failed to read {}", path.display()));

            let mut in_test_block = false;
            for (i, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed == "#[cfg(test)]" {
                    in_test_block = true;
                }
                if in_test_block {
                    continue;
                }
                if trimmed.starts_with("//") {
                    continue;
                }

                // Look for `Path(some_id): Path<Uuid>` patterns
                if line.contains("Path<Uuid>") {
                    // Extract the variable name from Path(var_name)
                    if let Some(start) = line.find("Path(") {
                        let after = &line[start + 5..];
                        if let Some(end) = after.find(')') {
                            let var_name = after[..end].trim();
                            // Check if this var name maps to a typed ID
                            for (suffix, typed_id) in TYPED_ID_MAP {
                                if var_name == *suffix
                                    || var_name.strip_prefix('_') == Some(*suffix)
                                {
                                    violations.push(format!(
                                        "  {}:{} — `{}` should be `{}` not `Uuid`",
                                        file_name,
                                        i + 1,
                                        var_name,
                                        typed_id
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        if !violations.is_empty() {
            panic!(
                "\n\n\
                 ╔══════════════════════════════════════════════════════════════╗\n\
                 ║  RAW Uuid USED WHERE TYPED ID EXISTS!                     ║\n\
                 ╠══════════════════════════════════════════════════════════════╣\n\
                 ║  Handlers should use typed IDs (WorkspaceId, TaskId, etc.) ║\n\
                 ║  instead of raw Uuid in Path<> extractors. This prevents   ║\n\
                 ║  accidentally swapping one entity's ID for another.        ║\n\
                 ║                                                            ║\n\
                 ║  Fix: change `Path<Uuid>` to `Path<WorkspaceId>` (etc.)   ║\n\
                 ║  and update the handler to use the typed ID.              ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\
                 \n\
                 Violations:\n{}\n",
                violations.join("\n")
            );
        }
    }
}
