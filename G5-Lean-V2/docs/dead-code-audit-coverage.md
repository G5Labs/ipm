# Dead-code audit: coverage and evidence

> Historical audit before removal. Batches 1–4 were subsequently authorized; see [implementation and validation](dead-code-retirement.md). Source links below refer to the pre-removal layout.

Prepared 2026-09-12. Read with the [removal proposal](/Users/zliu/Workspace/G5-Lean/docs/dead-code-proposal.md). This is a coverage record, not a declaration that every method in a retained crate or route group is UI-reachable. No application changes were made.

## Root set

| Root | Evidence / treatment |
| --- | --- |
| Browser application | [main.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/main.tsx) → [App](/Users/zliu/Workspace/G5-Lean/frontend/src/App.tsx) → routes, redirects, WorkspacePage tabs and callbacks. 509 modules scanned; 250 reachable in the application graph. |
| Alternate browser/dev entries | main.tsx mock startup, public/debug.html, public/test.html, coming-soon.html and development /ide. Mock/test imports retained separately. |
| HTTP server and indexer | [g5-api main](/Users/zliu/Workspace/G5-Lean/backend/g5-api/src/main.rs) constructs AppState and dispatches API/indexer/all roles. [build_router](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/mod.rs:246) is the actual route root. |
| Maintenance and cloud tasks | [MaintenanceRunner::spawn_all](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/mod.rs:240); cloud reconciliation/rotation/stale-host tasks from API startup. No entire maintenance module is proposed for removal. |
| g5d binary and CLI | [main](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/main.rs) → cli_dispatch → [dispatch](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/dispatch.rs:10). Worker, host-up, workspace/Git, job/task, AQL, tool/MCP, sync/spec/view, OCI/VM, golden builder, preview and installer support are executable roots. |
| Worker execution | WorkerOrchestrator assignment/provider dispatch; deterministic bootstrap; Claude/Codex/Fake vs Native; lifecycle; MCP/tool registries; host supervisor isolation/preview dispatch. |
| Operations/configuration | Root Justfile and its scripts; local Docker/host launcher; Helm/GCP bootstrap/E2E config; Azure/self-hosted installer workflow. Read-only source checks, not running deployments. |

## Cargo coverage

25 backend crates; 629 Rust source files. Cargo metadata read with --offline --locked --no-deps. The tests/e2e workspace member is a verification root, not another backend product crate.

| Crate | Rust files | Boundary reviewed / disposition |
| --- | ---: | --- |
| [g5-api](/Users/zliu/Workspace/G5-Lean/backend/g5-api/Cargo.toml) | 3 | Shipped HTTP/indexer entrypoints, migration/command binaries and integration suites. Retain. |
| [g5-api-contracts](/Users/zliu/Workspace/G5-Lean/backend/g5-api-contracts/Cargo.toml) | 9 | API errors and DTO contracts used across the retained server families. |
| [g5-api-core](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/Cargo.toml) | 154 | Main service/maintenance/CRDT/auth implementation. B4–B6/B9 and method-level candidates; most of the crate remains required. |
| [g5-api-data](/Users/zliu/Workspace/G5-Lean/backend/g5-api-data/Cargo.toml) | 33 | Database/models/repositories, used by current services. Historical schema is outside code-removal scope. |
| [g5-api-foundation](/Users/zliu/Workspace/G5-Lean/backend/g5-api-foundation/Cargo.toml) | 3 | Shared configuration/filesystem/telemetry foundations used by API startup and services. |
| [g5-api-routes](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/Cargo.toml) | 53 | Actual public/internal route boundary. Spend/task duplicates and possible API retirements are scoped separately below. |
| [g5-auth](/Users/zliu/Workspace/G5-Lean/backend/g5-auth/Cargo.toml) | 5 | JWT, scopes and middleware authentication remain in the API/worker path. |
| [g5-authz](/Users/zliu/Workspace/G5-Lean/backend/g5-authz/Cargo.toml) | 4 | Scoped database access and permission enforcement remain in current API and worker operations. |
| [g5-blob](/Users/zliu/Workspace/G5-Lean/backend/g5-blob/Cargo.toml) | 2 | Blob interfaces/adapters consumed by AppState and artifact routes. |
| [g5-common](/Users/zliu/Workspace/G5-Lean/backend/g5-common/Cargo.toml) | 1 | Compatibility facade used by retained consumers/tests; no whole-crate deletion established. |
| [g5-contracts](/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/Cargo.toml) | 38 | Shared wire/domain types, job/provider enums, agent-launch validation and historical payloads. Trim exclusive DTOs only after callers are removed. |
| [g5-crdt](/Users/zliu/Workspace/G5-Lean/backend/g5-crdt/Cargo.toml) | 5 | Shared document/materialization and authority protocol used by editor and worker paths. |
| [g5-internal-api](/Users/zliu/Workspace/G5-Lean/backend/g5-internal-api/Cargo.toml) | 5 | Worker/host/CLI client, assignment DTOs, stream multiplex protocol and service ports. |
| [g5-scm](/Users/zliu/Workspace/G5-Lean/backend/g5-scm/Cargo.toml) | 6 | Actual Git repository, worktree and auth primitives used by current workspace operations. |
| [g5-semantic](/Users/zliu/Workspace/G5-Lean/backend/g5-semantic/Cargo.toml) | 14 | Parsing and filesystem grounding/query implementations used by current CLI/tooling and ontology reads. |
| [g5-storage](/Users/zliu/Workspace/G5-Lean/backend/g5-storage/Cargo.toml) | 4 | Repository storage ports/adapters used by API startup, repository readers and maintenance. |
| [g5-workspace-ops](/Users/zliu/Workspace/G5-Lean/backend/g5-workspace-ops/Cargo.toml) | 8 | Current clone/fork/commit/merge helpers; distinguish these from B3’s old GitManager facade. |
| [g5d](/Users/zliu/Workspace/G5-Lean/backend/g5d/Cargo.toml) | 96 | Shipped binary and composition/adapters. Native engine, B1 old API and B6/B8 candidates are mixed with required worker/prompt/host code. |
| [g5d-cli](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/Cargo.toml) | 48 | All actual command dispatch paths remain roots. B3 old git facade and B10 old daemon config loader are unrooted; live workspace/worker/task/MCP/OCI commands remain. |
| [g5d-foundation](/Users/zliu/Workspace/G5-Lean/backend/g5d-foundation/Cargo.toml) | 44 | Current CRDT, materializer, search, terminal, OCI/golden implementation; B2 is a distinct unrooted connection/terminal bridge. |
| [g5d-host](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/Cargo.toml) | 13 | Host supervisor, assignment, preview attach/tunnels and VM management. Only alternate MicrovmAgent-specific pieces are conditional candidates. |
| [g5d-isolation](/Users/zliu/Workspace/G5-Lean/backend/g5d-isolation/Cargo.toml) | 14 | Normal MicroVm/libkrun and process execution are retained. WSL2 placeholder is a candidate; alternate carrier mode needs coordinated retirement. |
| [g5d-runtime-core](/Users/zliu/Workspace/G5-Lean/backend/g5d-runtime-core/Cargo.toml) | 10 | Shared provider capabilities/tool names, agent receipts, execution context, SCM credentials and runtime ports. |
| [g5d-tooling](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/Cargo.toml) | 26 | Tools also run through g5d tool/MCP and provided-agent paths. B6 supervisor tool and B7 emission tools are scoped cuts; not a Native-only crate. |
| [g5d-worker](/Users/zliu/Workspace/G5-Lean/backend/g5d-worker/Cargo.toml) | 31 | Claude/Codex/Fake drivers, lifecycle, preview/build/task-graph helpers. Retain live providers and lifecycle boundaries. |

## Route/support module inventory

All 52 source modules are listed. “Retain” refers to the family: it does not assert that every registered endpoint has a current UI caller. The actual mount root, generated/exempt route builders, internal clients and operational callers are considered separately. Registered API-only methods remain compatibility choices until individually retired.

| Module | Finding / retained boundary |
| --- | --- |
| [api_keys](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/api_keys.rs) | Organization API credential management and client/worker access; retain registered family. |
| [audited](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/audited.rs) | Shared audited/exempt route construction, keep. |
| [auth](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/auth.rs) | Login/OAuth/SCM and CLI device authorization roots. Method-level dead helpers in services do not retire this router. |
| [blobs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/blobs.rs) | Current issue/agent/artifact attachment and content retrieval. |
| [browser_origin](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/browser_origin.rs) | Shared origin helpers; one unreferenced wrapper remains a secondary lead, not a whole-module proposal. |
| [capacity](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/capacity.rs) | Shared capacity response projections, retain with jobs/workers. |
| [chat](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/chat.rs) | Current session/message/start/resume/context/attachment routes; legacy Native is normalized in this family. |
| [comments](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/comments.rs) | Current editor/node/line comments; unused CommentsSidebar is not feature retirement. |
| [config](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/config.rs) | User/org/workspace runtime and agent-launch settings; current UI and agent launch depend on it. |
| [docs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/docs.rs) | Current documents/explainers/sync; distinguish from retired view_create. |
| [drafts](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/drafts.rs) | Current draft editing/listing/commit/discard/CRDT-related UI operations. |
| [explainer_templates](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/explainer_templates.rs) | Current Create HTML Explainer workflow. |
| [files](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/files.rs) | Current files, commits, history and branch reads used by editors/source control. |
| [git](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/git.rs) | Git smart HTTP is a protocol root used by workspace/worker operations. |
| [golden_builder](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/golden_builder.rs) | Mounted through internal.rs #[path] module and merged into internal router; current worker/builder client uses it. |
| [grounding](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/grounding.rs) | Current grounding/provenance APIs and CLI/tool integration. |
| [guards](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/guards.rs) | Shared authorization/mutation guards, keep. |
| [home](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/home.rs) | Current organization Home view. |
| [human_escalations](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/human_escalations.rs) | Current human question/answer and task wait/resume path; B6 removes only unused automatic supervisor. |
| [instance](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/instance.rs) | Instance info retained. Health is called by Justfile/seed/CI; old chat-model hook has no UI caller. |
| [intent_log](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/intent_log.rs) | Feature-gated current viewer and evidence/provenance; E2E build enables it. |
| [internal](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/internal.rs) | Host/worker/task/CRDT/grounding/usage/preview/agent lifecycle endpoints are indirect UI dependencies. Do not infer death from no browser fetch. |
| [issues](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/issues.rs) | Current Issues UI plus worker/CLI issue tools. |
| [jobs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/jobs.rs) | Job actions and real /jobs/:id/tasks endpoint; view_create creation is rejected; other jobs also have CLI/agent producers. |
| [mod](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/mod.rs) | Actual route composition and WebSocket adapters; keep. |
| [notifications](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/notifications.rs) | Current NotificationPanel/toasts and notification actions. |
| [ontology](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/ontology.rs) | Current tree/content/mutation/sync/diff/dependency routes; uncalled service methods are scoped separately. |
| [ontology_format](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/ontology_format.rs) | Mounted from ontology router; some UI hooks unused, but no whole-module retirement established. |
| [org_approvals](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/org_approvals.rs) | Organization approval definitions; current workspace/org tabs. |
| [orgs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/orgs.rs) | Organizations, memberships, invitations and repository operations; retained UI/API family. |
| [permissions](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/permissions.rs) | Unused UI wrappers; /auth/me/scopes is required by reset-dev-environment.sh. Other management endpoints require separate API retirement. |
| [platform_issues](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/platform_issues.rs) | Current platform issue reporting UI; conditional feature config retained. |
| [preview](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/preview.rs) | Browser reverse proxy/tunnel; current preview UI root. |
| [preview_profiles](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/preview_profiles.rs) | Current preview applications/profiles and preview controls. |
| [provider_accounts](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/provider_accounts.rs) | Current provider accounts/credentials/quota selection and settings. |
| [resolve](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/resolve.rs) | Slug/deep-link resolution for current navigation. |
| [runtime_artifacts](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/runtime_artifacts.rs) | Current runtime artifact retrieval supporting task output. |
| [search](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/search.rs) | Current global/workspace search; scoped vector/FTS backend retained. |
| [self_hosted](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/self_hosted.rs) | Registry/admin installer operations; separate CLI/installer root, not a dead UI feature. |
| [settings](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/settings.rs) | Workspace settings retained; spend report hooks unused. Reporting-only retirement is separate from accounting. |
| [source_control_change_graph](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/source_control_change_graph.rs) | Current Source Control graph/projections. |
| [spend](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/spend.rs) | Confirmed unmounted duplicate: remove with declaration; settings owns live spend endpoints. |
| [tasks](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/tasks.rs) | Current task actions retained; only job_tasks_router and private old listing path are unmounted. |
| [telemetry](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/telemetry.rs) | Current frontend telemetry ingest, feature/config gated. |
| [user_preferences](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/user_preferences.rs) | Current workspace settings reads/writes preferences; retain even though automatic supervisor wiring is absent. |
| [users](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/users.rs) | Current-user/user lookup and API compatibility; retain registered family, no whole-group deletion proposed. |
| [waitlist](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/waitlist.rs) | Registered but only obsolete LandingPage is an application caller; proposed API retirement, external users unverified. |
| [worker_hosts](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/worker_hosts.rs) | Current worker-host management UI and provisioning APIs; conditional self-hosted gates do not make it dead. |
| [worker_tokens](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/worker_tokens.rs) | Worker installation/enrollment and operator credentials; retain. |
| [workers](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/workers.rs) | Worker UI, bootstrap/install and operations routes; individual unused useOrgWorkers hook does not retire the family. |
| [workspace_approvals](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/workspace_approvals.rs) | Current human/agent approval workflows. |
| [workspaces](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/workspaces.rs) | Current workspace creation/cloning/branch/merge/repository/source-control/remote workflows; unused hooks do not retire these. |

## Job/provider/isolation matrix

| Axis | Reachability conclusion |
| --- | --- |
| Claude Code / Codex | Current UI agent choices; keep drivers, sidecars, tools, credential/proxy and protocol support. Codex availability is configuration-gated. |
| Fake | Deterministic dev/test harness and supported dispatch branch; retain verification support. |
| Native / NaiveReact | No new supported UI launch; old fallback remains executable. Retire with compatibility handling, not by deleting the provider enum first. |
| Uplift / downlift | Current WorkspacePage producers and deterministic bootstrap + agent execution. Analysis/parsers remain live. |
| Chat / sync / merge / approval_review / explainer_generate / preview | Current UI, service or tool producers. Their execution machinery remains. |
| Review | Requires a provided-agent launch contract and maps to TaskType::Review. Keep its supported command/agent roots. |
| Compile / test / custom | Accepted through current command/job surfaces, but TaskContext::from_assignment rejects them before agent execution. Compile/test do not establish a Native fallback path. API acceptance alone does not demonstrate a functioning end-to-end workflow; no blanket family deletion proposed. |
| GoldenBuilder | Maintenance-generated, ordinary user submissions rejected; current cache/build support, keep. |
| ViewCreate | Already retired by route/service/claim gates. Remove unregistered emission tools; retain rejection and historical data/display compatibility. |
| MicroVm / libkrun | Current local and GCP mode, keep. |
| Process | Still an operational/test alternative, keep unless explicitly retired. |
| MicrovmAgent | Absent from checked local/GCP choice, but Azure canary selects it; preview-attach shares components. Conditional retirement. |
| WSL2 | Availability is hardcoded false; unused backend/handle branches are a cleanup candidate. |
| AF_VSOCK agent carrier | Guest opt-in remains; shipped host and preview attach use TSI. Transport prototype retirement is separate from the active multiplex/attestation protocols. |

## Important non-UI consumers and false positives

| Apparent dead code | Why it stays / exact distinction |
| --- | --- |
| /api/v1/instance/health | [Justfile](/Users/zliu/Workspace/G5-Lean/Justfile:2546) and seed/CI scripts call it. The unused useInstanceHealth hook is not evidence that the endpoint is unused. |
| /api/v1/auth/me/scopes | [reset workflow](/Users/zliu/Workspace/G5-Lean/scripts/reset-dev-environment.sh:327) uses it. |
| Ordinary terminal execution | B2 removes only the old direct-browser streaming bridge. Tool terminal execution and PTY management remain. |
| g5d connect | Current CLI CRDT synchronization is unrelated to the unused foundation ConnectionManager. |
| Skills / prompts / browser tools | Skill manifests and tool definitions still feed provided agents; browser tools are registered by factory through CLI tool construction. “No direct BrowserProxyTool constructor outside its file” is not evidence of death. |
| debug_tests.rs | It is a live skill module, despite the filename. It was retained in the final Rust inventory/index. |
| WorkerHost/OrgMembers/WorkspaceSettings content components | Current WorkspacePage tabs mount them. Only standalone wrappers are disconnected. |
| User-preferences service | A current settings control reads/writes it; unused supervisor-related logic is narrower. No preference endpoint deletion proposed. |
| Contract schemas, mocks, fake sidecars, parser fixtures | Separate test/dev roots. Preserve behavior coverage; do not delete them because production UI has no direct import. |
| Migrations and historical discriminators | No data deletion or wholesale enum deletion proposed. Old rows/rollback readers may require them. |

## Exact method inventory

These 31 methods are listed in the main proposal; private helpers that become exclusive are not counted. Empty production references are a source-index finding corroborated by name search, not a guarantee about external Rust library users.

| File / line | Method |
| --- | --- |
| [oauth.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/oauth.rs:261) | `exchange_code` |
| [scm.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/scm.rs:297) | `ScmTokenManager::build_clone_url` |
| [scm.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/scm.rs:344) | `ScmTokenManager::refresh_provider_token` |
| [scm.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/scm.rs:447) | `ScmTokenManager::check_token_validity` |
| [linked_doc_indexer.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/linked_doc_indexer.rs:67) | `index_linked_documents` |
| [auth_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/auth_service.rs:3015) | `AuthService::refresh_scm_tokens` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:11321) | `ChatService::update_user_message_content` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:11464) | `ChatService::create_task_session` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:11559) | `ChatService::complete_session` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:11606) | `ChatService::archive_session` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:13465) | `ChatService::reconcile_stranded_running_chat_task` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:13893) | `ChatService::refresh_active_chat_task_context` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:14045) | `ChatService::reactivate_dormant_task` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:14466) | `ChatService::resume_chat_task_on_live_runtime` |
| [chat_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:15925) | `ChatService::handoff_chat_task_to_replacement` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:112) | `OntologyService::resolve_node_repo_view` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1033) | `OntologyService::get_persisted_workspace_node` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1091) | `OntologyService::get_persisted_component_nodes` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1526) | `OntologyService::update_node` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1760) | `OntologyService::get_sync_target_file_path` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1793) | `OntologyService::get_sync_target_repo_id` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:2132) | `OntologyService::add_dependency` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:2246) | `OntologyService::remove_dependency` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:2287) | `OntologyService::compute_staleness` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:2412) | `OntologyService::move_node` |
| [ontology_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:2585) | `OntologyService::get_file_node_map` |
| [search_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/search_service.rs:476) | `SearchService::compute_embedding` |
| [task_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs:25896) | `TaskService::batch_node_shas` |
| [task_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs:25944) | `TaskService::job_view_context` |
| [task_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs:26040) | `TaskService::job_sync_context` |
| [user_service.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/user_service.rs:483) | `UserService::upsert_user_from_oauth` |

## Reproducibility and limits

The structured [evidence file](/Users/zliu/Workspace/G5-Lean/docs/dead-code-audit-evidence.json) records the file inventory, candidate boundaries, frontend importer evidence and virtual-check results. Temporary analysis scripts were run outside the repository; no permanent analyzer dependencies were added.

The review uses syntax/name indexing plus manual execution-path checks. It does not expand every Rust macro or resolve every method to its receiver type. It therefore treats anonymous callbacks, trait dispatch, serde attributes, factories, shared DTOs and test-support APIs conservatively. No observed production-use claims are made.

Broader method-level pruning can continue when the user approves a removal batch; the candidate count from a scanner is deliberately not presented as a deletion list. Current high-impact frontend, protocol, provider, isolation, startup and service boundaries are covered above.
