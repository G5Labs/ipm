# G5-Lean dead-code proposal

> Historical audit before removal. Batches 1–4 were subsequently authorized; see [implementation and validation](dead-code-retirement.md). Source links below refer to the pre-removal layout.

Re-audited 2026-09-12. **Proposal only. No application code, commands, configuration, migrations or tests were changed or removed.**

The broader audit found substantial backend candidates beyond Native/ReAct and MicrovmAgent: the old local instance server, direct browser/worker connection stack, GitManager facade, scheduler family, manifest service, automatic HIL supervisor, unregistered view tools, cache operations, duplicate WebSocket adapters and old daemon configuration loader. There are **26 additional whole-file candidates (9,940 physical lines)** plus method-level cuts, alongside the original 13 frontend files and 21 hooks. Counts include comments, blank lines and inline tests; partial files and conditional retirement paths are not included in that total.

**Recommended order:** review backend groups B1–B10 and disconnected frontend implementations first; then retire Native with an explicit old-assignment policy. Treat MicrovmAgent as conditional retirement: the Azure canary still selects it, and active preview attachment shares some of its code. Keep the ordinary MicroVm/libkrun execution path.

## Scope, method and confidence

“Reachable” means reachable from the current shipped UI through an import, route, callback, supported feature flag, API, background process, worker, agent tool, or protocol. Direct URLs and administrator/developer screens count. CLI/Justfile/installer roots were checked separately so removal proposals do not silently break development or operations. Registered APIs without a UI caller are listed as API-retirement choices, not globally unreachable functions.

The audit inventories **629 Rust source files across 25 backend crates**, the frontend's **509 TypeScript source modules**, **52 route/support modules**, CLI dispatch, both binary entrypoints, AppState/service construction, task/provider/isolation dispatch, maintenance startup, and checked local/GCP/Azure configurations. A Rust syntax index covers 623 non-test-only source files; six explicitly test-only source files are excluded. Inline test modules, comments, imports/re-exports and serde callback attributes are distinguished. One grammar limitation inside a test-only async closure was handled by parsing that file's production prefix; no source file was changed. The index generated 403 no-reference function leads, **not that many safe deletions**. Findings below require additional wiring/caller checks; matching a name is not Rust type resolution.

The TypeScript compiler resolves imports, re-exports, aliases and exported values. The application reaches 250 modules, or 253 including mock startup. Of 15 disconnected non-test modules, 13 are product removal candidates; the others are an alternate mock bootstrap and a deliberately test-only API-contract schema. Public HTML and the only nonliteral dynamic import were checked separately.

See the [coverage and retained-path inventory](/Users/zliu/Workspace/G5-Lean/docs/dead-code-audit-coverage.md) and [structured evidence](/Users/zliu/Workspace/G5-Lean/docs/dead-code-audit-evidence.json). This is a broad source/configuration audit, not a type-resolved whole-program Rust proof or a production traffic audit. Existing database contents, external consumers and deployed fleet selections were not inspected. Historical formats and operational paths are explicitly protected below.


## Backend candidates with no production entry path found

These are stronger findings than “the UI has no button.” The source contains no production construction/registration path to the implementation, or its only callers are themselves in an unrooted subsystem. Module declarations and public exports do not make a subsystem executable. Remove the described wiring together with each candidate; deleting isolated files would leave compilation errors.

### B1. Old g5d instance HTTP server — 9 files, 2,378 lines

[ApiServer](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/api/mod.rs:209) is never constructed by the shipped binary or CLI dispatch. Its `build_router` registers only four internal worker endpoints; even that router has no production server startup. The older chat, search, tree, connect, Git and worker-management handlers in this directory are additionally absent from that router.

**Propose removing** all nine files under [g5d/src/api/](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/api/mod.rs), the `pub mod api` declaration, and the unused [WorkerOrchestrator::runtime_handles](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/mod.rs:5380) adapter that returns this module’s `WorkerRuntimeHandles`. The status tests construct the state directly; that is not evidence of a server running in the product.

**Keep:** the separate [g5-api HTTP server](/Users/zliu/Workspace/G5-Lean/backend/g5-api/src/main.rs:516), the actual [CLI dispatch](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/dispatch.rs:10), `g5d worker`, `host-up`, current worker/internal API clients, and preview listeners. No Justfile recipe was found to start this old `ApiServer`.

### B2. Old direct browser-to-worker connection stack — 4 files, 2,125 lines

The [foundation ConnectionManager](/Users/zliu/Workspace/G5-Lean/backend/g5d-foundation/src/connection/mod.rs:66) has no production constructor caller. Its external owner is the unused B1 API state. `register_stream_routes` returns the input router unchanged; the broker route returns `NOT_IMPLEMENTED`. The [TerminalStreamSource](/Users/zliu/Workspace/G5-Lean/backend/g5d-foundation/src/terminal/stream.rs:111) bridge is created only by `TerminalManager::create_stream_source`, which also has no production caller.

**Propose removing together:** `g5d-foundation/src/connection/{mod,streams,token}.rs`, `terminal/stream.rs`, their exports, the unused `create_stream_source` method, and the g5d facade’s connection re-export. This removes the disconnected data-channel/token/multiplex implementation, not the current terminal implementation.

**Keep:** [TerminalManager/PTY execution](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/src/tools/terminal.rs:83), current worker inbox and preview tunnels, and the unrelated Redis `ConnectionManager` used by rate limiting. The current UI talks to g5-api; absence of the old direct-stream routes does not imply that all WebSockets are dead.

### B3. Old GitManager and task-worktree facade — 2 whole files, 959 lines, plus part of mod.rs

[GitManager](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/git/mod.rs:70) has no production construction. Its only non-test owner is B1’s `GitOps`. `clone_with_cow`, `ensure_bare_clone` and [TaskWorktree](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/git/worktree.rs:31) are consumed only by that facade and their own tests; `cleanup_stale_worktrees` has no production caller.

**Propose removing:** `GitManager`, `GitConfig`, their methods/tests, [git/clone.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/git/clone.rs), [git/worktree.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/git/worktree.rs), and their module declarations/re-exports.

**Keep:** the rest of `git/mod.rs`, `head_reconciliation`, credentials, safety and task configuration; the current [task_workspace::setup_task_workspace](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/task_workspace.rs:52) calls `g5_workspace_ops::fork_task_workspace`. The host’s `Materializer`, `g5-scm`, `g5-workspace-ops::cow_clone`, and the user-facing `g5d clone/fork/commit/push` commands remain live. Similar names refer to different implementations.

### B4. Old scheduler family — 4 files, 1,554 lines

[LocalityScorer](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/scheduler/locality.rs:41), [WorkerScaler](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/scheduler/scaling.rs:66), and [SpendChecker](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/scheduler/spend_check.rs:62) have no production instantiation or startup registration. In particular, the comment saying SpendChecker runs on each scheduler tick is stale. The old WorkerScaler’s provisioning method only inserts worker reservations; current cloud bring-up has its own implementation.

**Propose removing:** `scheduler/{mod,locality,scaling,spend_check}.rs` and the scheduler module/re-export declarations. There is an integration test that directly constructs SpendChecker; preserve any still-relevant budget behavior coverage by targeting the actual retained boundary.

**Keep:** current [TaskService](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs), host task claims, [cloud bring-up/reconciliation](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/cloud/mod.rs:24), [SpendService::record_usage](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/spend_service.rs:601), and [golden-builder budget checks](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/golden_builder_triggers.rs:821). This finding does not establish that current budget enforcement is complete; it establishes that these older classes do not provide it.

### B5. Worker checkout ManifestService — 1 file, 261 lines

[ManifestService](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/manifest_service.rs:26) is declared but never instantiated or added to AppState. None of its six database operations has a production caller. The comment saying `refresh_upstream_head` is called by `repo_fetcher` is not supported by the actual source.

**Propose removing:** this file, its service module declaration, and the `g5-contracts::manifest::ManifestEntry` DTO/module if the final reference check still finds only this deleted service and its own tests. Do not confuse it with the live workspace manifest, skill manifest, or task-commit manifest.

**Keep:** historical migrations and `repo_manifests` data for now. Removing unused Rust code is not authorization to drop a table. Current repository materialization, fetch and task-commit logic remain.

### B6. Automatic HIL supervisor — 4 files, 1,926 lines, plus wiring

This is a complete unwired feature, not merely a side effect of removing Native. Neither [HilSupervisorRouter](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/hil_supervisor_router.rs:59) nor [HilSupervisorService](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/services/hil_supervisor_service.rs:56) is constructed. The claimed Redis dispatch/consumer connection in the comments is absent. The current [internal escalation producer](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/internal.rs:13803) explicitly passes `"to_human"`.

**Propose removing:** those two files, [SupervisorAssistTool](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/src/tools/supervisor_assist.rs:34) (its sole production-shaped constructor is inside the unused service), [hil_supervisor_guidance.tera](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/prompts/templates/hil_supervisor_guidance.tera), its registration/render helper, module exports, and tests exclusive to this supervisor. Review `SupervisorOutcome` as an exclusive DTO; preserve any historical wire values still needed to read stored records.

**Keep:** HumanEscalationService, the human question/answer UI, escalation tools, notifications, and current task wait/resume behavior. Also keep the user-preferences endpoint: [WorkspaceSettingsContent](/Users/zliu/Workspace/G5-Lean/frontend/src/components/settings/WorkspaceSettingsContent.tsx:692) still uses it. Unused supervisor-specific preference fields can be considered separately; they do not justify deleting the preferences feature.

### B7. Unregistered legacy view-emission tools

[EmitTempExtractTool](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/src/tools/view_ops.rs:239) and [EmitViewArtifactTool](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/src/tools/view_ops.rs:350) are exported but never constructed/registered in the product. The current registry at [tools/mod.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d-tooling/src/tools/mod.rs:2572) registers the other four view tools, not these two. CLI/MCP tool construction was checked as a separate root.

**Propose removing:** those two structs/impls, their re-exports, exclusive request/helpers, dedicated tests, and Native prepopulation references when Native is retired. Keep the rest of view_ops.rs and current sync tools.

The larger legacy `view_create` / `view_future_create` path is already rejected by [JobService](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/job_service.rs:3630); the public job route returns Gone. Remaining Create-only prompt branches can be trimmed with a historical-compatibility review. **Keep** `JobType::ViewCreate` deserialization, old-job cost/display support, rejection guards and migrations 182/216. Current `explainer_generate`, document sync, view sync and `view_task_builder` are different, live workflows.

### B8. Unused prewarming and remote L2 cache — 2 whole files, 737 lines, plus methods

The current worker constructs CacheClient for the [WorkspaceCacheHydrator boundary](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/cache/mod.rs:629); [worker lifecycle](/Users/zliu/Workspace/G5-Lean/backend/g5d-worker/src/lifecycle.rs:2196) calls hydration. However, `upload_new`, `trigger_prewarm`, `prewarm_sync` and `watch_lockfiles` have no production callers. The PreWarmEngine object is allocated, but its work is never started by a product caller.

Both production CacheClient constructors, in the [provided-agent branch](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/mod.rs:6911) and [Native branch](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/mod.rs:7479), explicitly supply `CacheMode::Local` and `remote: None`. The only [L2 constructor gate](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/cache/mod.rs:363) requires `CacheMode::Gcs`; there is no product caller supplying it. Consequently the remote cache is also unreachable from the shipped worker construction. The old configuration loader that parses remote-cache options is itself uncalled (B10).

**Propose removing:** [cache/prewarm.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/cache/prewarm.rs), [cache/l2.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/cache/l2.rs), their fields/allocation/cancel/export wiring, L2-only branches in CacheClient/CacheHydrator, `CacheClient::trigger_prewarm`, and [CacheClient::upload_new](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/cache/mod.rs:511) with its exclusive UploadResult support. Retire exclusive remote-cache configuration only after tracing shared config types. Other generic cache get/put/statistics methods need receiver-specific review before inclusion; similarly named methods exist throughout the repository.

**Keep:** CacheClient’s active L1 hydration and the independent OCI/golden-cache pipeline. This is not a proposal to delete all cache code or GCS storage infrastructure.

### B9. Duplicate WebSocket adapters and event bus

The mounted [ws_adapter](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/mod.rs:464) calls `handle_ws_connection` directly; [ws::ws_handler](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/ws/mod.rs:498) has no caller. The older [ws::EventBus](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/ws/mod.rs:190) is used only by the unused B4 SpendChecker; AppState uses the different `services::EventBus`. The old `ws::start_redis_listener` is never started.

**Propose removing:** that unused handler/its exclusive query DTO, the older `ws::EventBus` after B4, and `ws::start_redis_listener`. **Keep:** WsHub, handle_ws_connection, services::EventBus, worker_hub and its [actually started Redis inbox listener](/Users/zliu/Workspace/G5-Lean/backend/g5-api/src/main.rs:340), preview tunnels and channel authorization. No claim is made that the active worker inbox listener replaces browser event broadcasting; these are distinct protocols.

### B10. Old daemon configuration loader

[G5dConfig](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/config.rs:369), its [CliArgs](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/config.rs:417), and [G5dConfig::load](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/config.rs:1010) have no production caller. Their apparent wiring exists in comments and direct construction tests. The shipped CLI parses the different [Cli/GlobalFlags](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/lib.rs:109) and dispatches current commands.

**Propose removing:** G5dConfig, this old CliArgs, their load/test_default implementations, exclusive subordinate types/helpers/imports and tests. This is a partial-file cleanup. **Keep** WorkspaceConfig, shared cache/isolation/clone types that retain live consumers, the actual CLI flags and dispatch, and the unrelated g5-api-foundation::CliArgs used by the API binary. Do not remove config.rs wholesale or change working Justfile commands based on the obsolete flag definitions.

The seven whole-file groups B1–B6 and B8 total **26 additional files / 9,940 physical lines**, including inline tests, comments and blank lines. All 26 files are byte-identical to their original-reference counterparts: these leftovers predate the folder relocation. The count excludes all partial-file cuts, Native/ReAct, MicrovmAgent, WSL2, frontend candidates and the previously found spend duplicate. It is a review scope, not a guaranteed final diff size.

## Backend methods inside otherwise live services

The following methods have no non-test call/reference in the backend source index and were checked by exact-name search. They are proposed **method-level** cuts; their containing services remain live. Public Rust methods often avoid ordinary dead-code warnings, which is why a successful build did not expose them.

| Area | Proposed method cuts | Retained path / boundary |
| --- | --- | --- |
| [OntologyService](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/ontology_service.rs:1526) | `resolve_node_repo_view`, `get_persisted_workspace_node`, `get_persisted_component_nodes`, `update_node`, `get_sync_target_file_path`, `get_sync_target_repo_id`, `add_dependency`, `remove_dependency`, `compute_staleness`, `move_node`, `get_file_node_map` | Current ontology routes, projected reads, `create_node`, `delete_node`, content updates, dependency reads and reconciliation remain. “update_node” is not the mounted “update_node_content” route. |
| [ChatService convenience wrappers](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:13465) | `reconcile_stranded_running_chat_task`, `refresh_active_chat_task_context`, `reactivate_dormant_task`, `resume_chat_task_on_live_runtime`, `handoff_chat_task_to_replacement` | Current send paths call their **`_prelocked` variants** directly, e.g. chat.rs:3292 and :3371. Keep those variants and their lock/identity/recovery behavior. Retarget wrapper-only tests to the retained production boundary. |
| [Older ChatService helpers](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs:11464) | `create_task_session`, `complete_session`, `archive_session`, `update_user_message_content` | Keep current session creation, session deletion, message processing and the shared completion/cancellation/lifecycle implementation. The old task-session helper still stamps `naive_react`. |
| [TaskService metadata helpers](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs:25896) | `batch_node_shas`, `job_view_context`, `job_sync_context` | Keep assignment construction and active view/sync context resolution; remove only these uncalled helpers. |
| [Unscoped embedding helper](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/search_service.rs:476) | `compute_embedding`; its private `fetch_embedding_settings` becomes uncalled with it | Active indexing and search use `compute_embedding_scoped` (calls at :843 and :1072). Keep providers, scoped settings, vector/FTS search and usage accounting. |
| [Older SCM helpers](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/scm.rs:297) | `build_clone_url`, `refresh_provider_token`, `check_token_validity`; [oauth::exchange_code](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/auth/oauth.rs:261); [AuthService::refresh_scm_tokens](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/auth_service.rs:3015); [UserService::upsert_user_from_oauth](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/user_service.rs:483) | Keep current login/SCM OAuth, credential resolution, token handling, auth callbacks, user services and provider connections. These method names do not justify deleting auth modules. |
| [Linked-document index wrapper](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/linked_doc_indexer.rs:67) | `index_linked_documents` | `index_linked_documents_scoped` is retained; background reindexing and linked-document behavior remain. |

There are **31 methods in this table**, plus the private embedding-settings helper. The evidence inventory records exact locations. Small accessors, serializers, builders, test-support APIs and other mechanically flagged symbols are not automatically approved removal candidates. In particular, CRDT/lifecycle convenience APIs used only by tests need a deliberate coverage decision; deleting those tests would not demonstrate that their production replacement is correct.

## Frontend candidates and the previously identified duplicate routes

### Frontend: 13 whole files

Each candidate has no production or mock-mode import path from main.tsx. “Only tests” does not mean those tests should all be discarded; see the test plan below.

| Candidate | Lines | Evidence and exact boundary |
| --- | ---: | --- |
| [components/chat/SessionPicker.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/chat/SessionPicker.tsx) | 304 | No importers. Chat remains live through ChatPanel; remove only this unused picker. |
| [components/chat/ToolCallBubble.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/chat/ToolCallBubble.tsx) | 202 | No importers. Remove this unused renderer, not tool-call support in the current chat timeline. |
| [components/comments/CommentsSidebar.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/comments/CommentsSidebar.tsx) | 293 | No importers. WorkspacePage still loads node/line comments and passes them to current editors. |
| [components/draft/DirtyFilesList.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/draft/DirtyFilesList.tsx) | 368 | No importers. Current dirty-file/source-control handling lives in WorkspacePage and its active components. |
| [components/draft/DraftEditor.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/draft/DraftEditor.tsx) | 400 | Only a test imports it. Current workspace renders CodeEditor. Retarget the shared CRDT workflow test before removing. |
| [components/execution/IntervenePanel.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/execution/IntervenePanel.tsx) | 327 | Only its own test imports it. WorkspacePage explicitly says Intervene is no longer a visible drawer tab; preserve current takeover/task controls. |
| [components/notifications/NotificationBanner.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/notifications/NotificationBanner.tsx) | 275 | No importers, including its ClientInstallBanner export. Current NotificationPanel and NotificationToastStack remain live. |
| [components/org/OrgMembersPage.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/org/OrgMembersPage.tsx) | 27 | Unused standalone wrapper. Preserve OrgMembersPageContent and the redirect into the workspace tab. |
| [components/org/WorkerHostsPage.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/components/org/WorkerHostsPage.tsx) | 49 | Standalone wrapper used only by tests. Preserve WorkerHostsPageContent and move the behavioral tests to that live component. |
| [hooks/usePermissions.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/usePermissions.ts) | 208 | No importers. Remove these frontend wrappers only; permission enforcement and the operator-facing scope endpoint remain required. |
| [hooks/useSpend.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useSpend.ts) | 31 | No importers; contains only useSpendHistory. Remove the unused frontend hook, preserving accounting and spend-limit enforcement. |
| [pages/LandingPage.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/LandingPage.tsx) | 511 | Only its own test imports it. Both / and /landing redirect to public/coming-soon.html; login uses App.tsx. |
| [pages/WorkspaceSettingsPage.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspaceSettingsPage.tsx) | 85 | No importers. Settings now render through WorkspaceSettingsContent inside WorkspacePage. |

Positive checks for the replacement paths:

- [LandingRedirect](/Users/zliu/Workspace/G5-Lean/frontend/src/App.tsx:740) sends visitors to coming-soon.html. The current page’s [Join waitlist link](/Users/zliu/Workspace/G5-Lean/frontend/public/coming-soon.html:243) uses mailto, not the old form.
- [Workspace tab rendering](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:5508) mounts WorkspaceSettingsContent, WorkerHostsPageContent and OrgMembersPageContent; keep these content components and App.tsx redirects.
- [Current editor rendering](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:4913) mounts CodeEditor. The file continues to load comments and render active notifications/chat/source-control controls.
- [Takeover handling](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:7848) explicitly records that Intervene is no longer a visible drawer tab. Removing IntervenePanel does not justify removing takeover services.

### Backend: two duplicate paths

| Candidate | Evidence | Proposed boundary |
| --- | --- | --- |
| [Unused spend router; whole file is 200 lines](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/spend.rs:154) | The module is declared, but spend::router() has no caller. The real endpoint is registered by [settings::router()](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/settings.rs:284). | Remove spend.rs and its module declaration. Keep settings.rs, SpendService, DTOs used by settings, and the live spend API tests. Update stale source-audit references. |
| [tasks::job_tasks_router](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/tasks.rs:80) and its private list_job_tasks handler at line 236 | The registration shown in its comment does not exist. [jobs::job_detail_router](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/jobs.rs:1065) registers the real /jobs/:id/tasks handler, implemented at line 3952 in jobs.rs. | Remove only this unused router, handler, dedicated PaginationQuery/default helpers and imports that become unused. Keep tasks.rs and all task actions. |

The obsolete task handler’s sole service call is [TaskService::list_tasks_for_job](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs:23658). No other caller was found. Its body can go with that path after a final reference check. The similarly named JobService::list_tasks_for_job is live through job progress; do not remove it. Shared task DTO conversion/capacity/background/attempt helpers also remain live, including attach_capacity_block through get_task.

## Unused symbols inside live frontend files

The symbol-reference scan found these 21 exported hooks have no callers in the reachable UI graph; an independent name search found only declarations and, for some, tests. Remove individual hooks and their exclusive imports/types, not their containing modules or backend APIs. Some live screens invoke equivalent APIs through other hooks.

| Live module | Unused hooks | Test-only references found |
| --- | --- | --- |
| [useAuth.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useAuth.tsx:715) | `useMyScopes` | [useUsers.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useUsers.test.ts) |
| [useSettings.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useSettings.ts:52) | `useWorkspaceSpend`, `useInstanceHealth`, `useChatModels` | [useSettings.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useSettings.test.ts) |
| [useWorkspace.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useWorkspace.ts:229) | `useWorkspaceDependencies`, `useCreateWorkspace`, `useStartWorkspaceSyncChat`, `useDirectCleanWorkspaceMerge` | [useWorkspace.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useWorkspace.test.ts) |
| [useDraft.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useDraft.tsx:203) | `useDraftParticipants` | [useDrafts.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useDrafts.test.ts) |
| [useFiles.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useFiles.ts:136) | `useFileWatch` | None |
| [useOntology.tsx](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useOntology.tsx:66) | `useOntologyChildren`, `useNodeDeps`, `useSpecFormatValidation`, `useHumanBlocks`, `useUncertaintySummary` | [useOntology.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useOntology.test.ts) |
| [useIssues.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useIssues.ts:415) | `useAttachComponent` | None |
| [useViews.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useViews.ts:18) | `useViews`, `useViewDetail`, `useDeleteView` | None |
| [useWorkers.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useWorkers.ts:67) | `useOrgWorkers` | None |
| [useWorkerHosts.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useWorkerHosts.ts:155) | `useUpdateWorkerImagePolicy` | [useWorkerHosts.test.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useWorkerHosts.test.ts) |

Additional small candidates verified to have no callers:

- [WorkspaceGrid](/Users/zliu/Workspace/G5-Lean/frontend/src/components/layout/Layout.tsx:134) and Pane at line 232: unused layout implementations. Keep AppShell, TopBar and banner context.
- [SearchFilter](/Users/zliu/Workspace/G5-Lean/frontend/src/components/tree/TreeNode.tsx:617): unused filter component. Keep all active tree/document components.
- [mocks/browser.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/mocks/browser.ts:12): unused MSW service-worker bootstrap, 14 lines. [Mock startup](/Users/zliu/Workspace/G5-Lean/frontend/src/main.tsx:140) imports mocks/fetch-interceptor instead. Keep MSW dependencies, handlers, fixtures and the fetch interceptor so dev:mock and tests still work. Do not sweep the whole mocks/test directory.

Not included merely because an export has no runtime caller: test-reset utilities, fixture data, API-contract schemas, and the PlatformIssueReportButton compatibility host used by isolated behavioral tests. These have a current verification purpose. A later cleanup can relocate them into test support without losing coverage.

## Provider retirement: Native / NaiveReact

**The current UI offers Claude Code and, when enabled, Codex; Native/ReAct is not an available launch choice.** Its internal provider name is `NaiveReact` / `naive_react`. The evidence supports retiring the legacy engine from the supported UI execution path, but does not establish that it has never run or that no historical task can still reach its fallback.

Execution-path evidence:

1. [supportedCloudAgentProviders](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:2317) returns only `claude_code` and optionally `codex`; the actual chat selector consumes it at line 10559. The [harness picker](/Users/zliu/Workspace/G5-Lean/frontend/src/lib/harnessPicker.ts:52) likewise resolves to those two harnesses.
2. [WorkspacePage](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:10769) maps an old session’s `naive_react` value to Claude Code for the effective chat provider. The [send-message API](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/chat.rs:2568) applies the same compatibility normalization before resolving the next turn.
3. [New chat launches](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/chat.rs:1823) resolve an immutable agent launch. The shared [launch validation](/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/src/types/agent_launch.rs:1090) permits Claude Code, Codex and the deterministic Fake harness, rejecting Native. [Agent job families](/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/src/types/agent_launch.rs:1140), including uplift/downlift/review/custom, require this launch contract. Fake remains a development/test dependency and should not be removed with Native.
4. The worker’s provided-agent path returns before native LLM setup. However, the [legacy fallback](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/mod.rs:7307) remains implemented and constructs [NaiveReactTaskAgent / AgentLoop](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/mod.rs:7455). [The provider enum](/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/src/types/task_context.rs:394) still defaults to NaiveReact, and worker provider selection uses a default when the assignment lacks provider metadata. Therefore it is not honest to describe the engine as having no executable references.
5. The no-launch job families were checked separately. Compile/test jobs do not require an agent launch, but [TaskContext::from_assignment](/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/src/types/task_context.rs:2925) rejects them before the worker calls build_and_execute_task. They therefore do **not** establish a current Native fallback path. Preview and GoldenBuilder have separate host execution. Custom also fails this worker conversion despite being accepted by job creation; API acceptance alone does not establish end-to-end support. This audit does not claim those command-triggered workflows have been exercised successfully.

Proposed removal boundary, subject to explicit approval:

- Remove the legacy [NaiveReactTaskAgent adapter](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/task_agent.rs:11), native dispatch/setup branch, and [AgentLoop / AgentLoopConfig / react_loop implementation](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/agent/mod.rs:129).
- Follow its exclusive dependencies: native dialog compaction, prepopulation, skill routing, nested ReAct execution, and native LLM adapters under `backend/g5d/src/llm/`. Delete only code whose remaining consumers are in the retired branch or retired tests. This needs a symbol-level cleanup, not a directory deletion.
- Separate the skill registry/manifest from its native execution engine. Both [Claude’s host execution](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/host/agent_drive.rs:302) and [Codex’s host execution](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/host/agent_drive.rs:485) still call `register_default_skills` and consume its manifest for prompts. Keep those definitions and their required assets until the prompt contract is deliberately changed.
- Preserve the live guest [agent_runner](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/agent/agent_runner.rs:380), preview-attach listener, WebSocket inbox, shared worker protocol/result types and provider LLM proxy. They support Claude/Codex despite living under, or being re-exported through, `agent/`.
- Preserve historical provider deserialization or introduce an explicit compatibility policy before removing the enum value/default. Decide how old queued/retryable native assignments are handled, while keeping existing chat normalization. Do not rewrite historical database migrations or assume every stored `naive_react` label proves native execution.

The unwired HIL supervisor and the unused `ChatService::create_task_session` helper are detailed separately above. Their comments are not evidence of an active runtime integration.

Names containing `react_loop` in [merge workflow metadata](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/job_merge_service.rs:49) are not sufficient removal evidence: current Claude/Codex merge workflows still use those subtype labels and prompts. Keep them unless their actual workflow is retired.

Validation after approval must cover both Claude and Codex through host/guest and worker dispatch, chat start/resume, sync/merge/explainer/approval jobs, Fake-based tests, old-session compatibility, and shared prompt/skill-manifest construction. This section is based on source reachability; no production logs or persisted deployment inventory were examined, and no application code was removed.

## Isolation modes: MicrovmAgent, WSL2 and AF_VSOCK

**MicrovmAgent is a retirement candidate for the normal task-execution path, but it is not globally unreachable.** The current local/GCP configuration uses `MicroVm` (`microvm`), which boots a full worker inside the VM. `MicrovmAgent` (`microvm-agent`) is the alternative where the host drives the agent and the guest runs the sidecar. Both use the same libkrun VM backend; neither means the native ReAct provider.

| Area | Finding | Recommendation |
| --- | --- | --- |
| `IsolationMode::MicroVm` and the libkrun runtime | Default mode, explicitly selected by local development and GCP configuration. | Keep. These execute current UI-triggered jobs and previews. |
| `IsolationMode::MicrovmAgent` and its separate task drive | Retired by the local launcher, absent from the checked GCP configuration, still selectable by CLI/environment and explicitly selected by the Azure canary. | Retire the alternate execution mode only together with its remaining configuration/tooling consumers. It is not a safe unqualified dead-code deletion today. |
| `Wsl2Backend` and unreachable WSL2 runtime branches | Constructor hardcodes unavailable; availability check returns false on every platform; run/start only return errors. | Strong cleanup candidate. Remove the nonfunctional backend and its unreachable handle/selection branches, while retaining clear unsupported-platform diagnostics. |
| `AgentCarrier::Vsock` transport prototype | Guest has an opt-in AF_VSOCK branch, but the shipped host stamps TSI; preview attach also constructs TSI. | Candidate to retire as an unsupported alternate transport, after checking for direct experimental CLI users. Keep the live multiplex protocol and TSI transport. |

### Where current configuration points

- [Local worker-host launcher](/Users/zliu/Workspace/G5-Lean/scripts/worker-host-env.sh:75) explicitly says the no-identity local path is retired and sets `G5D_ISOLATION_MODE=microvm`. The local `.dev/g5d-host/host-up.mode` marker also contains `microvm`.
- [GCP worker-host bootstrap](/Users/zliu/Workspace/G5-Lean/infra/terraform/modules/worker-host/templates/host-bootstrap.sh.tpl:104), [Helm worker values](/Users/zliu/Workspace/G5-Lean/infra/helm/g5/values.yaml:251), and the current Rust E2E cloud-build configuration select `microvm`.
- In contrast, the [Azure installation canary](/Users/zliu/Workspace/G5-Lean/.github/workflows/self-hosted-azure-install.yml:468) explicitly sets `microvm-agent` and `G5D_AGENT_CARRIER_LIVE=1`. The [self-hosted worker installer](/Users/zliu/Workspace/G5-Lean/infra/self-hosted/scripts/g5-workerhost.sh:512) has a real branch that stages and starts this mode. Associated installer/canary tests assert it. These are in-repository consumers; no claim is made here that the workflow recently ran or that a deployed fleet still uses the mode.
- The [central mode gate](/Users/zliu/Workspace/G5-Lean/backend/g5d-isolation/src/isolation/mod.rs:1229) rejects `MicrovmAgent` only when its opt-in flag is off. It is not an unconditional rejection. Nearby older comments calling the implementation “inert” or “always rejected” no longer describe that condition accurately.

### Proposed MicrovmAgent-specific retirement boundary

After the remaining Azure/self-hosted use is migrated or explicitly retired, review these pieces together:

1. The `MicrovmAgent` enum/config/CLI option and its opt-in flag, plus the corresponding assignment command/environment branch in [assignment.rs](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/assignment.rs:479). Keep the `MicroVm` worker command/environment builders.
2. [spawn_microvm_agent_drive](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs:6482) and [drive_microvm_agent_once](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs:11519), plus helpers proven exclusive to their one-shot VM boot/drive, host-side bootstrap, and parked-runtime paths. Keep shared supervisor bookkeeping and the separate preview-attach drive.
3. [HostAgentCarrier](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/agent_host.rs:331): its only non-test construction found is inside `drive_microvm_agent_once`. Review the struct and dedicated port/spec wiring as candidates once that caller is removed. The rest of agent_host.rs contains shared routing and is not a whole-file deletion candidate.
4. The standalone `g5d agent-runner` CLI/boot entry path and its dedicated configuration parsing/connection setup. Preserve the shared guest sidecar orchestration functions consumed by preview attach. The host-side Codex-over-carrier branch may then become unused too; recheck it separately from the live Codex worker implementation.
5. Installer runtime/stability branches and tests that specifically target `microvm-agent`; migrate the Azure canary to the retained mode with a real smoke test before deleting that supported lane. Keep the shared runtime staging script: the current `microvm` guests still need its binaries and provider assets.

Do not estimate removable lines by counting whole files with “microvm” or “agent” in the name. Several large files mix exclusive and shared code, so an exact deletion count requires separating their symbols first.

### Confirmed live UI path that reuses the older names

The [preview browser’s Attach chat button](/Users/zliu/Workspace/G5-Lean/frontend/src/components/preview/PreviewBrowserView.tsx:399) calls [WorkspacePage.handleAttachChat](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:7475). The subsequent chat start sends [attachPreviewJobId](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/WorkspacePage.tsx:11035), and the API records an attached-preview assignment. The supervisor [dispatches that assignment to preview attach before checking isolation mode](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs:5720).

That path uses:

- [AgentCarrierDeps on ordinary MicroVm hosts](/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/host_up.rs:1428), including the helpers named `build_microvm_agent_claude_code_config` / `build_microvm_agent_codex_config` in the existing construction code. Their names alone do not prove they are unused.
- [MicrovmAgentTeardown inside drive_preview_attach_once](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs:13067), with no owned VM handle so detaching does not kill the preview VM.
- [The same NoIdentityAgentDriver port](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs:13423); the current preview-attach drive requests Claude Code.
- [agent_runner::run_with_mux and the shared handshake/validation helpers](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/agent/preview_attach.rs:29), called by the preview listener. Therefore `agent/agent_runner.rs` must be split or trimmed rather than deleted wholesale.

Keep libkrun boot/reaping, networking, OCI/rootfs handling, guest runtime staging, preview tunnels, stream multiplexing, LLM/session-store proxies and the current in-VM worker lifecycle. They support active UI work. `RunHandleInner::Krunvm` is also still the handle constructed by LibkrunBackend; the old-looking name does not make it a removed krunvm-CLI implementation.

### WSL2: narrower, stronger dead-code finding

[Wsl2Backend::new](/Users/zliu/Workspace/G5-Lean/backend/g5d-isolation/src/isolation/microvm.rs:1350) hardcodes `available: false`, [check_wsl2](/Users/zliu/Workspace/G5-Lean/backend/g5d-isolation/src/isolation/detect.rs:236) returns false on every platform, and run/start always error. Its private [RunHandleInner::Wsl2 variant](/Users/zliu/Workspace/G5-Lean/backend/g5d-isolation/src/isolation/mod.rs:758) has no runtime producer; only a test constructs it. Proposed cleanup: remove the placeholder implementation, dead handle/selection arms, unused name generator, WSL2-only tests and misleading backend matrix entries. If a public diagnostic schema needs its Windows field, retain an explicit unsupported result instead of the fake backend. Keep the functioning LibkrunBackend in the same file.

### AF_VSOCK carrier: prototype, not the live mux

[HostAgentCarrier::wire_spec](/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/agent_host.rs:430) always writes `G5D_AGENT_CARRIER=tsi`; [preview attach configuration](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/agent/preview_attach.rs:312) also chooses TSI. The alternate [AgentCarrier::Vsock branch](/Users/zliu/Workspace/G5-Lean/backend/g5d/src/agent/agent_runner.rs:259) remains selectable directly and calls connect_vsock_and_run, but no matching producer was found in the shipped host path. The code describes it as a future carrier. This is a scoped prototype-retirement candidate, not a reason to delete files named `vsock_mux`/`vsock_transport`, AF_VSOCK-based guest attestation, or other active VM networking.

No microVM implementation, configuration, workflow or test was changed during this investigation. Validation of an approved removal would need ordinary Claude/Codex task execution, preview startup and Attach chat/Stop/re-attach, VM teardown/reaping, local dev startup, and the migrated self-hosted canary. Source evidence here does not establish current production traffic or Azure run history.

## APIs without current UI callers: separate retirement decisions

| Surface | Finding | Proposed treatment |
| --- | --- | --- |
| Old waitlist signup API | [The obsolete page](/Users/zliu/Workspace/G5-Lean/frontend/src/pages/LandingPage.tsx:243) is the only application HTTP caller found. The shipped coming-soon page uses email. The [POST /api/v1/waitlist route](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/waitlist.rs:30) is still registered and calls a dedicated WaitlistService/model. | Best end-to-end feature retirement candidate: remove route, service, model, AppState/module wiring, auth exemption, stale mock handler/route constant and feature-specific tests together if this API is no longer supported. Check for external website/API callers first. Preserve the existing migration and signup data; do not drop the table as a code cleanup. |
| Spend summary/history HTTP APIs | UI hooks useWorkspaceSpend and useSpendHistory are unused, but [settings.rs](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/settings.rs:284) still exposes both GET endpoints. | Optional HTTP reporting retirement. Keep accounting/usage ingestion and budget checks. Do not remove SpendService or spend schema wholesale. This is separate from safely removing the unmounted duplicate spend.rs above. |
| Old instance chat-model catalogue | [useChatModels](/Users/zliu/Workspace/G5-Lean/frontend/src/hooks/useSettings.ts:186) is unused; no live frontend or checked script caller was found for its URL or route constant. The [handler](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/instance.rs:413) remains registered and invokes ChatService::get_available_models. | Optional endpoint/helper/DTO retirement after checking external API consumers and helper sharing. Keep current provider-account selection, agent-launch model settings, instance info and operational health endpoints. |
| Granular permission-management HTTP APIs | No live UI caller was found for the scope/override management hooks and [router groups](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/permissions.rs:253). However, /auth/me/scopes has a real script caller. | Retain the scope endpoint needed by the Justfile reset workflow. Any retirement of the other management endpoints is an API-surface decision; retain authorization services, persisted grants and enforcement. Do not remove permissions.rs wholesale. |

The native-engine and alternate microVM-mode candidates above are focused backend retirement proposals. No entire backend crate, database migration, or worker subsystem is proposed for deletion. The pending retained-feature list and backend design can justify larger removals later.

## Code that must not be classified as dead from missing UI calls

- **Authorization and operator commands:** [reset-dev-environment.sh](/Users/zliu/Workspace/G5-Lean/scripts/reset-dev-environment.sh:327) calls GET /api/v1/auth/me/scopes; [Justfile](/Users/zliu/Workspace/G5-Lean/Justfile:3192) runs this script. PermissionService also authorizes [job creation](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/job_service.rs:2514), provider access, task execution and merges.
- **Accounting:** [internal worker usage ingestion](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/internal.rs:9736) calls SpendService::record_usage; [builder maintenance](/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/golden_builder_triggers.rs:821) checks spend limits. These are indirect dependencies of UI-triggered work.
- **Workers and cached builds:** [golden_builder.rs is mounted through a path attribute](/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/internal.rs:134), then merged into the internal router at line 3192. [The worker client](/Users/zliu/Workspace/G5-Lean/backend/g5-internal-api/src/client.rs:947) calls its dispatch/transition endpoints. Absence from the top-level mod.rs declarations is not proof of dead code.
- **Feature-gated screens:** [capability resolution](/Users/zliu/Workspace/G5-Lean/frontend/src/lib/capabilities.ts:41) and [local Compose build flags](/Users/zliu/Workspace/G5-Lean/infra/docker/docker-compose.dev.yml:381) make cloud workspace workflows available; the intent-log viewer is explicitly enabled in the E2E build. [The /ide route](/Users/zliu/Workspace/G5-Lean/frontend/src/App.tsx:1008) exists in dev mode. These are product/feature-retirement choices, not disconnected code.
- **CRDT, Git, preview tunnels, task lifecycle, maintenance, webhooks and shipped agent assets:** browser actions depend on work performed through protocols and background processing. Do not remove these based on direct browser-request counts.
- **Contract and mock support:** [api-schemas.ts](/Users/zliu/Workspace/G5-Lean/frontend/src/lib/api-schemas.ts:1) is imported by [the API contract suite](/Users/zliu/Workspace/G5-Lean/frontend/src/lib/__tests__/contract.test.ts:24). Mock startup imports test/msw/handlers and fixtures. frontend/public/debug.html also imports mock support directly. These are deliberate non-production consumers.


## Validation and proposed execution batches

1. **No-caller backend cleanup:** B1–B10, duplicate spend/task adapters, and the scoped method cuts. Remove module/re-export/AppState wiring together. Retarget tests that exercise retained behavior; remove only tests dedicated to retired implementations. Run affected Rust family checks and relevant API/worker tests. Preserve all Justfile commands unless a separately approved feature retirement requires changing one.
2. **Disconnected frontend cleanup:** the 13 files and 21 hooks. Retarget WorkerHostsPage tests to WorkerHostsPageContent and DraftEditor workflow tests to CodeEditor. Preserve CRDT commit/discard, worker provisioning, login, notifications, settings, comments and takeover coverage.
3. **Native retirement:** remove the native execution engine and exclusive dependencies while preserving shared prompt/skill manifests, provided-agent tools, Fake test harness, preview attachment and protocol types. Define handling of queued/retryable historical Native assignments before changing defaults or deserialization. Exercise Claude and Codex chat/start/resume, sync/merge/approval/explainer and deterministic bootstrap paths.
4. **Alternate-mode retirement:** migrate or explicitly retire the Azure/self-hosted MicrovmAgent lane first; preserve preview Attach chat and ordinary MicroVm/libkrun. Remove WSL2's nonfunctional backend separately. Treat AF_VSOCK carrier selection as an opt-in transport retirement, not deletion of the multiplex protocol.
5. **Public API retirement:** separately approve waitlist, reporting, unused model catalogue or permission-management surface changes. Preserve migrations/data and required command callers.

Validation performed for this proposal: refreshed frontend import/symbol analysis; **13-file virtual removal typecheck: baseline 0 diagnostics, simulated 0 diagnostics**, with files hidden only in a temporary compiler host; read-only Cargo metadata; Rust syntax/caller indexing with no remaining production parse errors; exact-name and registration checks for the proposed groups; source review of replacement paths and local/GCP/Azure configurations. The virtual check does not test runtime behavior, and no Rust deletion experiment or deployment was performed. Build/runtime tests for an implemented removal remain work for the approved change.

No application code has been removed. The next decision is which proposed batches you want implemented.
