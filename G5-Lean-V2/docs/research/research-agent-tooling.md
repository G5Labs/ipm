# G5-Lean research: the agent-facing surface (g5d CLI, tools, MCP, profiles, runtime adapters)

Historical implementation inventory. The ordinary `g5-tools` MCP server and agent
review-comment interfaces have been removed; see the [current provider contract](../lean-provider-cli-contract.md).

Repo: `/Users/zliu/Workspace/G5-Lean` (read-only). All paths below are relative to `backend/` unless they start with `assets/` or `backend/g5d/sidecars/`.
Every claim is tagged **[O]** observed (file:line) or **[I]** inference. Sizes are `wc -l`.

## 0. Method and one framing fact

- **[O]** The internal "NaiveReact" agent engine is retired: `g5d/src/worker/mod.rs:24-35` rejects `ChatAgentProvider::NaiveReact` ("Native/ReAct execution is retired; create a new Claude Code or Codex launch"). Only `ClaudeCodeTaskAgent` (`g5d-worker/src/claude_code.rs:9753`) and `CodexTaskAgent` (`g5d-worker/src/codex.rs:6772`) implement `TaskAgent` in production (plus `FakeTaskAgent`).
- **[I]** Consequently the big in-process `ToolRegistry` (`g5d-tooling/src/tools/mod.rs:2240-2560`: `read_file`, `write_file`, `run_in_terminal`, `git_*`, `transition_task_state`, …) is *not* what provided agents see. Provided agents see (a) their own native tools (Bash/Edit/etc.), (b) the `g5-tools` MCP server (`g5d mcp intent-grounding`), and (c) the stable-cwd `g5d` CLI. Section A.2 still lists every registry tool because the code is present and the `g5d tool`/MCP paths reuse subsets of it.

---

## A. Inventory

### A.1 `g5d` CLI subcommands

Top-level enum: `g5d-cli/src/lib.rs:192-333`; routing: `g5d-cli/src/dispatch.rs:20-67`. Stable-cwd guard (which commands are refused inside a Provided-Agent task workspace, detected via `.g5/provided-agent-chat-workspace.json`): `g5d-cli/src/stable_context.rs:250-359`.

Classification key: **K** keep (platform op), **N** replaceable by native git/shell/file, **R** remove with CRDT/authority, **?** unclear. "CRDT" column = touches CRDT drafts / task-commit bridge / authority head-operations.

| Command | Purpose (one line) | Wraps / replaces | CRDT | g5-api endpoint(s) | Class |
|---|---|---|---|---|---|
| `init` | Create empty classic workspace (`.g5/workspace.yaml`, `.g5/workspace-meta` git repo) — `init.rs:1-12` | `git init`+metadata | no | none (optional remote enable) | N/? |
| `clone <ws>` | Materialize workspace from g5-api manifest, COW-clone repos — `clone.rs:1-6` | `git clone` from `{api}/git/{repo_id}` | no | `/api/v1/workspaces/{}/manifest`, `/clone`, `/branches` (`workspace_manifest.rs`) | N (+K "open workspace") |
| `fork` | Local fork with deferred-publish lineage — `fork.rs:1-12` | `git branch`/worktree | no | none until push | R/N |
| `refresh` | Safe-additive manifest update (add repos, fix remotes) — `refresh.rs:1-5` | `git remote set-url`/clone | no | manifest | N |
| `add <url>` | Add repo to workspace (+ `--track-remote`) — `add.rs:1-4` | `git clone` + membership | no | workspace repos API | K (membership) |
| `commit -m` | Commit **CRDT drafts**: connected ws → server commit; stable task cwd → task-commit request file bridge; offline → local git checkpoint — `commit.rs:1-5,100-116,1276-1292` | `git add -A && git commit` | **yes** | `POST /api/v1/workspaces/{id}/drafts/commit-all` (`commit.rs:1148-1150`); stable: `.g5/task_commit/*` request files consumed by worker (`commit.rs:513-580`, `g5-crdt/src/task_commit.rs:18-21`) | R |
| `push [target]` | Publish heads via **head-operations** + smart-HTTP push to staging refs + finalize; stable cwd → publish-branch-state — `push.rs:1152-1230,260-315,1929-1948` | `git push` | **yes** (authority) | `POST /api/v1/workspaces/{id}/head-operations`, `…/{op}/finalize`, `POST …/docs/sync-committed`; stable: `POST /api/internal/tasks/{id}/publish-branch-state/prepare|finalize` (`task.rs`) | R (client side) / K (server governance) |
| `remote status|sync|push|push-mode|runs|retry|merge` | Server-mediated remote-tracking (upstream GitHub) sync/publish/merge runs; "never shells out to raw git" — `remote.rs:1-6` | `git fetch/push upstream` on server | no | `/api/v1/workspaces/{}/sync`, `/push_to_remote`, `/push_mode`, `/branches/{}/sync_runs[/retry]`, `/branches/{}/merge_runs[/{}/{}]` | K |
| `revert` | `git checkout -- . && git clean -fd` per repo, then reset CRDT draft — `revert.rs:5-12` | `git checkout/clean` | **yes** | none | N |
| `connect` | Long-running local CRDT connector (CrdtPeer FS detection/flush/materialize) — `connect.rs:1-5` | nothing native | **yes** | `/api/v1/workspaces/{id}/branches` + CRDT WS | R |
| `merge <src>` | Merge source workspace into current — `merge.rs:1-3` | `git merge` + Merge API | ? | workspace APIs | ?/N |
| `conflicts` | CRDT inbound-vs-third-party-agent conflict sidecars under `.g5/conflicts/` — `conflicts.rs:1-6` | n/a | **yes** | none | R |
| `intent-store migrate` | One-shot intent-ledger migration — `intent_store.rs:1-5` | file rewrite | no | none | ? (grounding) |
| `worker` | Start worker (poll/direct task); provider adapters — `g5d/src/cli/worker.rs:1-12` | infra | n/a | internal worker APIs | K (infra) |
| `host-up` | Host supervisor (microVM per task) — `lib.rs:249` | infra | n/a | `/api/internal/host/*`, `/api/internal/workers/register` | K (infra) |
| `doctor` | Diagnostics — `doctor.rs:1-6` | infra | no | none | K |
| `job create|status|wait|list|cancel|reset|merge` | Job control; `job merge` locally `git fetch origin` + merges `task/<id>` branches — `job.rs:1-4`, `cmd_merge` | control + `git merge` | no | `/api/v1/workspaces/{}/jobs`, `/api/v1/jobs/{}`, `/cancel`, `/tasks` | K (merge: N) |
| `chat context list|attach|update|remove|read` | Cross-session chat context links; `read` = worker-scoped receipt — `chat.rs:1-16` | n/a | no | `/api/v1/sessions/{}/context-links[/{}]`; read: internal receipt route | K |
| `aql validate|run|inspect|extend` | AQL job graphs — `aql.rs:1-6` | n/a | no | `/api/v1/workspaces/{id}/jobs/aql`; inspect/extend via task-control capability | K (gated) |
| `issue search|file|comment|anchor|update|take|attach-component|transition|report` | Issue tracker + platform bug report — `lib.rs:966-989` | n/a | no | `/api/internal/workspaces/{id}/issues[...]`, `…/platform/issues` | K |
| `auth login|status|logout` | Device-code auth for local CLI — `auth.rs:1-6` | n/a | no | `/api/v1/auth/cli/start`, `/cli/token`, `/logout`, `/refresh` | K |
| `task poll|claim|retry` | Classic external-agent flow: claim task, set up per-task workspace; refused in stable cwd — `stable_context.rs:322-324` | n/a | no | `/api/v1/tasks/{}/retry`, worker APIs | ?/N |
| `task complete [-m] [--result-artifact]` | Stable cwd: **spec-delta validation** → CRDT barrier → git checkpoint per mounted repo → checkpoint/completion publish-branch-state (git push of staging refs) → transition (or deferred file) — `task.rs:4062-4204,3640-3800,4497-4584`. Classic: `ops::checkpoint` + `ops::push origin` + mirror to job branch — `task.rs:4587-4700` | `git commit && git push` + "finish task" | **yes** | `/api/internal/tasks/{id}/self-context`, `/completion-authority/preflight`, `/publish-branch-state/prepare|finalize`, `/transition/preflight`, `/timeline`; deferred: `.g5/deferred_task_transitions/<task>.json` | K (finish-task) but internals R |
| `task rebase` | Replays task branches onto immutable parent cut with CRDT pause file; drives `git rebase`/continue itself — `task.rs:2512-2716`, pause file `.g5/task_commit/rebase.pause.json` (`g5-crdt/src/task_commit.rs:21`) | `git rebase` | **yes** | self-context, source-baseline adopt | R → N |
| `task fail|wait|block` (hidden) | Rejected for agents ("not available to agents… run `g5d task complete`") except legacy `legacy_wait_v0` stamp — `task.rs:3258-3266,3821-3860` | n/a | no | transition/timeline | K (already vestigial) |
| `task list|context` | List job tasks; print TaskAssignment / stable context (paths, dirty summary) — `task.rs:5055,3019` | n/a | no | `/api/v1/jobs/{}/tasks`, self-context | K |
| `tool [--list] <name>` | Local runner over the MCP tool set (same registry as MCP) — `tool.rs:1-5`, `lib.rs:924-951` | n/a | rebase-pause guard | see A.3 | K (dev/debug) |
| `mcp intent-grounding` | stdio MCP server `g5-tools` (protocol `2025-11-25`) over `build_mcp_tool_registry` — `mcp.rs:16-17,66-105` | n/a | rebase-pause guard (`tool.rs:2385-2410`) | none directly | K if grounding stays |
| `view sync <doc> --direction` | Thin wrapper over docs sync endpoint — `view.rs:1-3`; refused in stable cwd | n/a | no | `/api/v1/workspaces/{}/docs/{}/sync` | K |
| `sync doc|view|node` | Trigger sync jobs — `sync.rs:1-4`; refused in stable cwd | n/a | no | `/api/v1/workspaces/{id}/jobs`, `/ontology`, `/docs/{}/sync` | K |
| `spec create|validate|read-ontology|measure-v4|migrate-v4|verify-move|block-inventory|tag-audit|list|sync` | Ontology spec files under `.semlift/specs/<id>.semlift.md` — `spec.rs:1-7`; `sync` registers specs with ontology DB (refused in stable cwd) | file write / lint | no | `POST /api/v1/workspaces/{id}/ontology/sync-specs` (sync only) | create/list: N; validate/read-ontology: ?; v4 migration: ? (one-off); sync: K/? |
| `workspace list|switch|push` | List; switch checkout to another workspace's branch state; publish classic state — `workspace.rs:1-4` | `git checkout` / `git push` | switch/push: authority | workspace APIs | list K; switch N/R; push R |
| `oci materialize` | Pull+extract OCI rootfs — `lib.rs:2316-2321` | infra | no | none | K (infra) |
| `vm-exec`, `preview-serve`, `golden-build`, `preview-attach-listener` (hidden) | In-VM entrypoints (libkrun boot, preview app, docker golden build, chat-attach listener) — `lib.rs:285-333` | infra | no | none | K (infra, not agent-facing) |

**[O]** Stable-cwd refusals (`stable_context.rs:262-357`): `fork, refresh, add, revert, merge, conflicts, worker, connect, host-up, view, sync, workspace switch|push, job create|cancel|reset|merge, aql run, task poll|claim|retry, spec sync, auth login|logout, init, clone`. Allowed: `remote, intent-store, issue, chat, task complete|fail|wait|block|rebase|list|context, spec (others), commit, push, tool, mcp, doctor, oci, job status|wait|list, aql validate|inspect|extend, auth status, workspace list`.

### A.2 In-process tool registry (`g5d-tooling/src/tools/`)

Registry construction: `tools/mod.rs:2240-2560`. Names from each `fn name()`.

| Tool | File | Purpose | Wraps | CRDT | g5-api | Class |
|---|---|---|---|---|---|---|
| `read_file`, `list_directory`, `write_file`, `edit_file_replace`, `delete_file` | `file_ops.rs:1-4` | FS ops against CRDT-materialized tree; mutations "write locally then enqueue a CRDT delta" | native file tools | **yes** | no | R (provided agents use native Edit/Write) |
| `read_spec`, `create_spec`, `edit_spec_replace`, `list_specs`, `validate_spec`, `get_spec_git_diff`, `read_ontology` | `spec_ops.rs:1-4` | Ontology spec CRUD/validation under `.semlift/specs/`; `read_ontology` resolves `@ref` blocks | file tools + lint | create/edit via CrdtPeer | no | `validate_spec`,`read_ontology` on MCP: ?; rest R |
| `git_status`, `git_diff`, `get_path_git_diff`, `git_commit`, `git_push`, `git_pull`, `git_fetch` | `git_ops.rs:1-12` | "Agents cannot use git CLI directly … because the CRDT layer may have pending deltas"; flush CRDT before git | `git status/diff/commit/push/pull/fetch` | **yes** | no (auth: worker JWT for `/git/` mirror, else SCM creds — `resolve_git_auth`) | R → N |
| `run_in_terminal` | `terminal.rs:1-60,436-472` | PTY/VM shell; blocks destructive git (`checkout,switch,reset,stash,clean,merge,rebase,restore`) and, in CRDT workspace-branch contexts, `commit,push,pull,fetch` | `bash` | **yes** (`ensure_materialized_clean_for_reads`) | no | R → N |
| `semantic_search` | `search.rs` | text/regex search index | `rg` | no | no | N |
| `extract_interface` | `interface.rs:1-4` | tree-sitter public-symbol extraction | n/a | no | no | ? (internal only) |
| `transition_task_state`, `g5_commit`, `respond_to_user`, `wait_for_user_input` | `task_lifecycle.rs:1-9` | Model-facing lifecycle (complete → materialize/commit/push/report); chat terminal tools | n/a | **yes** | `/api/internal/tasks/{}/transition`, `/api/internal/messages/{}/complete` | R (internal engine only) |
| `send_message` | `send_message.rs:1-4` | Inject message into another session | n/a | no | `/api/v1/sessions/{}/messages` | K? (internal only) |
| `human_escalation`, `global_escalation` | `hil_escalation.rs:1-7` | HIL escalation (task/job scoped) | n/a | no | `/api/internal/escalations[/global]`; MCP local variant writes `.g5/deferred_human_escalations/<task>.json` (`tool.rs` LocalHumanEscalationTool; `deferred_lifecycle.rs:14`) | K |
| `save_experience`, `list_experience`, `search_experience` | `experience_ops.rs:1-10` | `.g5/experience/` Markdown notes | files + `rg` | no | no | ?/N |
| `trigger_workflow` | `workflow_trigger.rs:1-8` | Create explainer / downlift / uplift sync jobs | n/a | no | `/api/v1/workspaces/{}/docs/explainers`, `/docs/{}/sync` | K (internal only today) |
| `file_issue`, `search_issues`, `add_issue_comment`, `update_issue` | `issue_ops.rs`, `search_issues.rs`, … | Shell to `g5d issue … --json` | n/a | no | via CLI → internal issues API | K (gated `G5_ISSUE_TRACKING_ENABLED`, `issue_tracking.rs:3-10`) |
| `read_chat_session_context` | `read_chat_session_context.rs:1-10` | Shells `g5d chat context read --format json` | n/a | no | internal receipt route | K (gated `G5_CHAT_CONTEXT_RELAY_AVAILABLE`) |
| `track_pass_rate` | `pass_rate_tracker.rs:1-6` | Writes pass-rate YAML into `.semlift.md` frontmatter | file edit | via CrdtPeer | no | ? (internal only) |
| `detect_ontology_cycles` | `ontology_cycles.rs:1-4` | Cycle scan over frontmatter + grounding JSONL | n/a | no | no | ? |
| `create_chat_intent`, `register_all_intent_record`, `update_intent_record`, `build_grounding`, `build_chat_grounding`, `ground_external_doc`, `register_external_doc`, `update_review_metadata`, `extract_intent`, `trace_parent_grounding`, `trace_children_grounding`, `fetch_all_intent`, `fetch_all_grounding`, `verify_grounding`, `jsonl_merge_preflight`, `resolve_jsonl_merge_conflicts` | `grounding_ops.rs:1-6` (22,137 lines) | Intent/grounding ledgers = local JSONL under `intent_record/`, `intent_grounding/` via `g5-semantic` `FsQueryEngine` | files | **yes** via `GroundingToolState::with_crdt_peer`; MCP path gated by rebase pause | no | ? (grounding product decision) |
| `extract_for_view`, `evaluate_relevance`, `apply_sync_to_spec`, `classify_future_action` | `view_ops.rs:1-10` | View-mode (sync-prep) tasks | n/a | yes | no | R (internal only) |
| `browser_navigate … browser_close` (13) | `browser_proxy.rs:1-9` | Proxy to pinned `@playwright/mcp` child | Playwright MCP | no | no | ?/N (agent could run playwright-mcp directly) |
| `invoke_skill`, `spawn_subagent` | `skill_invoke.rs:1-5` | "Legacy delegation schemas… not registered" | n/a | no | no | R |
| `echo_tool`, `mock_tool` | `mod.rs` | tests | — | — | — | — |

MCP-only local implementations (`g5d-cli/src/tool.rs`): `infer_ontology`, `impacted_nodes` (via `CliAnalysisPort` → `WaterfallAnalyzer`, `g5d/src/cli_ports.rs:22-80`), `inspect_job_aql`, `extend_job_aql`, `create_task_graph` (task-control capability env `G5D_TASK_CONTROL_API_URL/TOKEN`, `provided_agent_tools.rs:5-6`), `human_escalation` (deferred file), `validate_spec` (LocalValidateSpecTool).

### A.3 What the MCP server (`g5-tools`) actually exposes

**[O]** Platform projection `g5d-runtime-core/src/provided_agent_tools.rs:176-203` (25 always-on): `create_chat_intent, register_all_intent_record, update_intent_record, build_grounding, build_chat_grounding, register_external_doc, ground_external_doc, update_review_metadata, extract_intent, trace_parent_grounding, trace_children_grounding, fetch_all_intent, fetch_all_grounding, verify_grounding, jsonl_merge_preflight, resolve_jsonl_merge_conflicts, detect_ontology_cycles, validate_spec, read_ontology, human_escalation, save_experience, list_experience, search_experience, infer_ontology, impacted_nodes`.
Gated suffixes (`:239-258`; browser list `:159-173`, AQL `:205-206`): AQL (`ENABLE_AQL`), issue tools (`G5_ISSUE_TRACKING_ENABLED`), `read_chat_session_context` (`G5_CHAT_CONTEXT_RELAY_AVAILABLE`), browser family (`G5_BROWSER_TOOLS_AVAILABLE` or probe of `/opt/ms-playwright/chromium-1237/INSTALLATION_COMPLETE`, `:35-150`). `autonomous_v1` policy removes `human_escalation` (`:285-300`).
The CLI contract table (`g5d-cli/src/tool_surface_contract.rs:361-560`) mirrors this and tags each row with `MutationClass` (`ReadOnly | WorkspaceMutation | HostMediatedMutation | RemoteMutation`); `WorkspaceMutation` rows are blocked by the rebase pause (`tool.rs:2385-2410`). Auxiliary Python tools (local harness only, `:560-568`): `report_grounding_coverage, report_grounding_drift, report_store_coverage, report_ref_edge_gaps, resolve_ontology_dependencies, list_module_leaves, regenerate_experience_index`.
The worker preflights the MCP server before every cold launch and fails the task if the list differs (`g5d-worker/src/mcp_preflight.rs:1-25`; `claude_code.rs:3713-3800`).

Note: no file, shell, or git tools are on the MCP surface. Provided agents get those from their own harness.

### A.4 Shipped profiles (`assets/agents/`)

- **[O]** `claude-code-g5/`: `CLAUDE.md` (807 lines), `.mcp.json` (registers `g5-tools` = `g5d mcp intent-grounding`, cwd `.`), `.claude/settings.json` (hooks only: `SessionStart, UserPromptSubmit, PreToolUse(Agent|Write|Edit|NotebookEdit|Bash|Read), PostToolUse(Write|Edit|mcp__g5-tools__*), SubagentStart, Stop`; `permissions.allow: []`; env `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`), 27 skills (6,919 lines), 12 hook scripts (4,170 lines incl. 1,142 test lines), 4 commands (`commit, pr, review, test`), `G5_SUBAGENT_BOOTSTRAP.md`, `G5_WORKFLOW_BOOTSTRAP.md`, `setup_verification.md`, `profile_manifest.json` (`requiredSkills` list incl. `g5-coordinated-git`).
- **[O]** `codex-g5/`: `AGENTS.md` (765), `G5_SUBAGENT_BOOTSTRAP.md`, `config.toml` (`approval_policy="never"`, `sandbox_mode="workspace-write"`, `[mcp_servers.g5-tools]` + `approval_mode="approve"` per tool), 24 skills (no `g5-coordinated-git`, no `g5-sync-aql-planning`).
- **[O]** What they say about git: `CLAUDE.md:16-80` / `AGENTS.md:22-81` tell the agent to use `g5d task context`, `g5d commit -m`, `g5d push`, `g5d remote push|status|sync`, `g5d task complete|wait|fail`, `g5d task rebase`, and "Do not run raw `git rebase`, `git rebase --continue`, `--abort`, `--skip`" (66-67), "Do not use `g5d job merge`" (78). `g5-coordinated-git/SKILL.md`: "Do not run raw `git` commands independently in mounted repos to simulate product branch, publish, PR, or merge behavior… Do not compensate with raw git pushes"; PR/approval creation is reserved to the platform. `g5-provided-agent-workspace/SKILL.md`: `g5d commit`/`g5d push` only when the prompt asks.
- **[O] Contradiction:** `.claude/commands/commit.md` (Step 6) instructs raw `git add <files>` + `git commit -m …`, and `.claude/commands/pr.md` (Step 5) instructs `git push -u origin HEAD` and `gh pr create --body-file …`. These slash-commands ship in the same profile that forbids raw git/PR creation.
- **[O]** Hooks are advisory: no hook returns exit 2 or a `deny` decision (grep over `assets/agents/claude-code-g5/.claude/hooks/*.py`; `pretool_deny` in `g5_hook_utils.py:155-161` has no callers). `pre-agent-g5-check.py` only injects the `G5_PREAMBLE_v1` block via `pretool_allow` (`:115`). `stop-session-audit.py:573-591` blocks Stop only when `G5_HOOK_STOP_AUDIT_BLOCKING` is truthy. No hook matches git commands.

---

## B. Agent launch inside the guest

### B.1 Claude Code

- **[O] Process shape.** Worker spawns `node <sidecar.mjs>` with `current_dir(workspace_path)` (`g5d-worker/src/claude_code.rs:5552-5556`). The sidecar (`backend/g5d/sidecars/claude/sidecar_source.mjs`, bundled to `sidecar.mjs`) calls the Claude Agent SDK `query()` with (`sidecar_source.mjs:905-999`): `cwd`, `pathToClaudeCodeExecutable: command.claudePath` (resolved from `G5_CLAUDE_PATH`/PATH, `claude_code.rs` `resolve_claude_path`), `includePartialMessages`, `settingSources: ["user","project","local"]` (so `.claude/settings.json` hooks load), `skills: "all"`, `env` (scrubbed provider env + explicit overlay + MCP provenance env; placeholder `ANTHROPIC_API_KEY="sk-no-identity-proxy-placeholder"` when only a base URL survived; `CLAUDE_CONFIG_DIR=/tmp/g5-claude-config-<req>` when SessionStore is active), `permissionMode`, `allowDangerouslySkipPermissions: uid != 0`, `model`, `thinking`, `effort`, `maxTurns`, `resume` (providerSessionId; dropped if the SessionStore has no rows, `:855-899`), `additionalDirectories` (+ `G5_CLAUDE_CODE_ASSETS_DIR`, `:835,984-990`), `allowedTools`, `disallowedTools` (`AskUserQuestion` forced off for non-chat, `disallowed_tools.mjs:9`), `sessionStore` (`G5ApiSessionStore`), `canUseTool = makeG5ApprovalHandler` (approval bridge to chat UI; skipped under `bypassPermissions`, `:477-520`, `permission_modes.mjs:16-18`), `hooks = buildWakeHooks` (sticky-runtime wake hooks, not policy hooks).
- **[O] Permission mode.** `permission_mode_for_task` returns `"bypassPermissions"` for every non-chat task family (`claude_code.rs:8914-8932`); frozen non-chat launches "require bypassPermissions" (`:8831-8837`). Chat uses the server-frozen `ResolvedAgentExecutionV1.permission_mode` (`:896`). Legacy `auto` maps to `bypassPermissions` (`permission_modes.mjs:8-13`).
- **[O] Model.** From the frozen resolution: `config.model` + `ANTHROPIC_MODEL` (`apply_task_root_model`); `CLAUDE_CODE_SUBAGENT_MODEL`, `ANTHROPIC_DEFAULT_OPUS_MODEL` also stamped (32/1 mentions).
- **[O] Env on the child** (`claude_code.rs:5557-5680`, `build_lifecycle_env :4562-4630`): `G5D_SESSION_ID`, `G5D_STABLE_CWD_ROOT`, `GIT_CONFIG_GLOBAL=<ws>/.g5/gitconfig` (guest) or `.g5/gitconfig.host`, `G5D_API_URL`, `G5D_TOKEN` (worker JWT — **only when not host-driven**), `G5D_TASK_ID`, `G5D_DEFER_TASK_TRANSITION=1`, `G5D_DEFERRED_TASK_TRANSITION_DIR=<ws>/.g5/deferred_task_transitions`, `DISABLE_G5_SKILLS`, `DISABLE_G5_ONTOLOGY`, `G5_CLAUDE_EXPLICIT_PROVIDER_ENV_JSON`. Host-driven lifecycle `env_remove`s `G5D_TOKEN, G5_API_KEY, G5D_REGISTRATION_TOKEN` (`:4641-4642`) and substitutes `G5D_TOKEN=loopback-no-auth` + proxy `G5D_API_URL` for the SessionStore. Provider credentials are scrubbed (`:780-792`: `ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, CLAUDE_CODE_OAUTH_TOKEN, …, OPENROUTER_API_KEY`). In-guest microVM adds `HOME=/work`, `XDG_CACHE_HOME=/work/.g5/cache/claude`, `CARGO_HOME`, `PYTHONUSERBASE`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_TELEMETRY/ERROR_REPORTING/AUTOUPDATER=1` and drops to uid/gid 1000 (`:3376-3420,5640-5680`; `guest_runtime.rs:4-5`). The no-identity runner (`g5d/src/agent/agent_runner.rs:770-900`) does `env_clear()` and passes only an allowlist: the above plus `G5D_WORKSPACE_ID`, `G5D_WORKSPACE_BRANCH_ID`, `ANTHROPIC_BASE_URL=<host proxy>`, `PATH=/opt/g5/bin:/usr/local/sbin:…`, TLS bundle paths, `LD_LIBRARY_PATH`, `G5D_SESSION_STORE_UNIX_SOCKET`.
- **[O] Profile materialization.** Assets dir = `G5_CLAUDE_CODE_ASSETS_DIR` or `<exe>/../assets/claude-code-g5[-<profile>]` or repo `assets/agents/claude-code-g5` (`claude_code.rs:995-1040`); copied into the workspace root ("g5 wins"); `.mcp.json` command rewritten to the absolute task-runtime `g5d` (`:3590-3645`); `CLAUDE.md` gets a dynamic `## Current runtime boundary` / `## Current workspace paths` appendix (`:3247-3290`). `DISABLE_G5_SKILLS=true` → methodology-free `CLAUDE.md` and deletion of `.mcp.json`, `.claude/{commands,examples,hooks,scripts,stack_examples,settings*.json}`, bootstrap docs (`:3527-3567`; prompt text in `provided_agent_profile.rs:80-125`). `autonomous_v1` → autonomous prompt (`provided_agent_profile.rs:14-30`). `ENABLE_AQL!=true` removes `g5-sync-aql-planning`.
- **[O] MCP preflight.** Before a cold launch the worker spawns `g5d mcp intent-grounding` with `ENABLE_AQL`, `G5_CHAT_CONTEXT_RELAY_AVAILABLE`, `G5_AUTONOMOUS_ASYNC_POLICY`, optional `G5D_TASK_ID`/`G5D_TASK_CONTROL_API_URL`/`G5D_TASK_CONTROL_TOKEN`, `G5D_TOKEN`/`G5D_API_URL`, and requires the exact tool list (`claude_code.rs:3713-3800`).

### B.2 Codex

- **[O] Process shape.** Worker spawns `node <codex sidecar>` in the workspace (`codex.rs:3864-3870`) with `G5D_SESSION_ID`, `G5D_STABLE_CWD_ROOT`, `GIT_CONFIG_GLOBAL`, `G5D_DEFER_TASK_TRANSITION=1` + dir, `CODEX_HOME=<task-scoped>` (`/opt/g5/codex-home` in guest, `guest_runtime.rs:12`), `G5_CODEX_EXPLICIT_PROVIDER_ENV_JSON`, LLM relay socket/port envs; scrubs `OPENAI_API_KEY, OPENROUTER_API_KEY, CODEX_API_KEY, CODEX_AUTH_JSON, …, G5D_TOKEN, G5_API_KEY, G5D_REGISTRATION_TOKEN` (`:556-570`) — **Codex never receives a worker token** ("Codex deliberately has no worker credential (D4I)", `task.rs:3835`). MicroVM: `HOME=/work`, `SSL_CERT_FILE=/opt/g5/bin/ca-certificates.crt` (+NODE/REQUESTS/CURL variants), uid 1000 (`:3921-3945`).
- **[O] CLI invocation** (`backend/g5d/sidecars/codex/sidecar_source.mjs:484-538,1226-1330`): `codex exec [resume <providerSessionId>] --json --skip-git-repo-check --output-last-message <tmp> [--model M] -c approval_policy="<a>" -c sandbox_mode="<s>" [-c sandbox_workspace_write.network_access=true] [-c model_reasoning_effort="<e>"]`, prompt on stdin, `codexPath = command.codexPath || G5_CODEX_PATH || "codex"`. Permission preset default `"on-failure/workspace-write"` (`codex.rs:94`).
- **[O] CODEX_HOME contents.** `config.toml` rendered per task (`codex.rs:880-953,1020-1060`): provider block (`openrouter` / custom `openai_proxy` with `wire_api="responses"` when `OPENAI_BASE_URL` set / builtin `openai`), `[mcp_servers.g5-tools]` with absolute `g5d` + absolute cwd, and `approval_mode="approve"` for every required MCP tool; `skills/` copied from `assets/agents/codex-g5/skills`; `AGENTS.md` + `G5_SUBAGENT_BOOTSTRAP.md` copied to the workspace root (`:103,1066-1160`); `auth.json` for ChatGPT subscription (`CODEX_AUTH_JSON`).
- **[O] Session resume.** `codex exec resume <id>`; rollout files under `CODEX_HOME/sessions/**` are mirrored to g5-api (`g5-internal-api/src/client.rs:1378-1440`: `POST/GET /api/internal/chat-sessions/{id}/codex-session-artifacts[/content]`; table `codex_session_artifacts`, migration `138`).

### B.3 Restrictions actually imposed on the harness

- **[O]** Tool denylist: only `AskUserQuestion` for non-chat (`disallowed_tools.mjs`); Codex list empty. No `allowedTools` restriction found in code (passed through from the send command if present).
- **[O]** `tool_path_projection.rs:1-8` is **not** a PATH shim; it computes `resolvedPaths` metadata for tool-call timeline rows. No `git` wrapper/shim on the guest PATH was found (`rg` over `g5d-host`, `g5d-isolation`, `g5d-worker`, `g5d`).
- **[O]** Post-turn: both adapters return `requires_post_turn_fs_scan() == true` (`claude_code.rs:9762`, `codex.rs:6781`) so the worker's CRDT peer diffs the checkout after each turn.

---

## C. What stops an agent from running plain `git commit/push/rebase/checkout` today

1. **[O] Nothing at the harness level.** `bypassPermissions` (Claude, all non-chat) and `workspace-write` with network (Codex) let the model run any git command from its own shell.
2. **[O] Prompt policy only** (`CLAUDE.md:16-80`, `g5-coordinated-git`, `g5-rebase-workflow`), contradicted by `/commit` and `/pr` slash-commands (A.4).
3. **[O] `run_in_terminal` git blocklist** (`g5-scm/src/safety.rs:45-48,87-113`; `g5d-tooling/src/tools/terminal.rs:49-60,449-472`; CLI wrapper `g5d-cli/src/git/safety.rs`) applies only to the retired internal engine's terminal tool — the only consumer is `terminal.rs:15,465`.
4. **[O] Credentials make raw `git push` to `origin` fail or be useless:**
   - `origin` for every mounted repo is the g5-api smart-HTTP mirror `{external_url}/git/{repo_id}` (`g5-api-core/src/services/task_service.rs:6609-6613`).
   - The mirror requires a JWT (Bearer or Basic `x-worker-token|x-user-token|x-access-token`, `g5-api-routes/src/routes/git.rs:161-225`). Under host-driven lifecycle the Claude guest has no `G5D_TOKEN` (B.1); Codex never has one (B.2). Legacy agent-driven mode does expose the worker JWT, and then `g5d task complete` uses it.
   - Even with a JWT, `git-receive-pack` accepts **only create-only `refs/g5/staged/<operation_id>/<repo_id>` refs** pre-registered by a workspace head-operation (`git.rs:290-339`), rejects force push (`:670-674`), rejects stale CAS (`:676-692`), requires a *running* task for workers (`:236-250`), and never advances a workspace head — the finalizer does (`:722-748`, "receive-pack is preparation-only").
   - GitHub/GitLab SCM tokens are injected **only for Chat jobs** from the user's `user_scm_connections` (`task_service.rs:14649-14660`), written to `.g5/task-<task_id>.git-credentials` and wired via `GIT_CONFIG_KEY_n=credential.helper` / `store --file=…`, `GIT_TERMINAL_PROMPT=0` (`g5d-worker/src/task_env.rs:114-150,206-235`); the raw JSON is **also exported as `G5_REPO_CREDENTIALS`** in the task env (`task_env.rs:117-121`). Provider username: `x-access-token` (GitLab `oauth2`) (`scm_credentials.rs:212-218`). **[I]** A chat agent could therefore push straight to GitHub if it added a github remote; it cannot do so for non-chat jobs.
5. **[O] `GIT_CONFIG_GLOBAL` pinned** to a task-private file containing exact `safe.directory` entries for each mounted repo (`task_git_config.rs:1-8,16-26`, `task_env.rs:64-80`); this is an enablement for cross-uid access, not a restriction on commit/push.
6. **[O] Docs repo write hardening** for non-Merge jobs: everything outside `docs/<workspace_id>/`, `views/`, `explainers/`, `intent_record/`, `intent_grounding/`, `.git/`, `.g5/` loses write bits (`g5d-foundation/src/docs_write_policy.rs:18,44-53,72-99,111-140`).
7. **[O] CRDT coupling:** the worker's `CrdtPeer` materializes inbound drafts into the checkout; conflicts with third-party edits go to `.g5/conflicts/` (`conflicts.rs:1-6`); `g5d commit` in a stable cwd publishes a request file under `.g5/task_commit/` that the worker executes (`commit.rs:513-580`); a durable pause `.g5/task_commit/rebase.pause.json` blocks MCP `WorkspaceMutation` tools and task checkpoints until `g5d task rebase` succeeds (`tool.rs:2385-2410`, `commit.rs:576-586`, `task_rebase_gate.rs:1-14` flock at `.g5/task_commit/rebase.mutation.lock`).
8. **[O] `g5d task complete` gates completion on spec-delta validation** of `.semlift/specs` (`task.rs:4497-4584`: "completion never validates the full corpus… undeterminable delta fails closed"), and on a succeeded task-commit receipt for every assigned repo (`task.rs:4130-4151`).
9. **[O] Stable-cwd guard** refuses classic lifecycle commands (A.1 list); `task fail|wait|block` are rejected for agents (`task.rs:3258-3266`).

**[I]** Net: today the *real* enforcement of "only the platform advances a branch" lives in the g5-api git route (staging refs + finalizer), not in the guest. Everything else is prompt text, missing credentials, or CRDT plumbing.

---

## D. Spec / semlift / ontology tooling

- **[O] Files.** `.semlift/specs/<component_id>.semlift.md` (YAML frontmatter `component_id, src_files|source_files, dependencies|dependency_spec_ids, parent_spec_id, child_spec_ids` + markdown I/D/Q blocks) — `g5d-cli/src/spec.rs:1-70`; legacy fallback `.g5/specs/<id>.md` (`g5d-foundation/src/ontology_inventory.rs:14-31`). `semlift.yaml` (partial parse `g5d-runtime-core/src/semlift_config.rs:29-40,63-99`): `views.fan_out_sync`, `views.generate_code`, `grounding.chat_intents_enabled` (gates `create_chat_intent`/`build_chat_grounding` registration, `tools/mod.rs:2515-2540`).
- **[O] Agent-facing pieces:** `g5d spec create|validate|read-ontology|list` (+ v4 migration one-offs), MCP `validate_spec`, `read_ontology`, `detect_ontology_cycles`, `infer_ontology`, `impacted_nodes`, `jsonl_merge_preflight`, `resolve_jsonl_merge_conflicts`; skill `g5-ontology-format` (483 lines); `CLAUDE.md` rules 1-13 (ontology is SOT, aggressive grounding).
- **[O] `analysis/`** (26,084 lines: language analyzers, `waterfall.rs`, `ontology_diff.rs`, `cycle_breaker.rs`, `id_service.rs`) is used by: uplift **bootstrap** (deterministic DAG → `submit_task_graph`, no LLM — `g5d/src/worker/bootstrap.rs:1-10`, also `g5d task poll` bootstrap in `task.rs:5168`), `infer_ontology` MCP tool (`cli_ports.rs:22-80`), `spec_ops.rs:27` / `tools/mod.rs:46` (`load_existing_ontology`), `component_id_convention.rs` (repo-scoped IDs `<repo_key>/<path>`).
- **[O] `ontology_inventory.rs`** (spec path + frontmatter list parsing) is used by `spec_ops.rs`, `worker/bootstrap.rs`, `view_task_builder.rs` (Phase B fan-out from view frontmatter `root_spec_ids/new_nodes/linked_nodes`, `:1-17`), `ontology_diff.rs`, `g5d-cli/src/ports.rs`.
- **[O] Product features depending on it:** job types `uplift|downlift|review` (`g5d job create --type`), `g5d sync doc|view|node` / `g5d view sync` (`/docs/{}/sync`, `/ontology`), explainer generation (`trigger_workflow create_explainer` → `/docs/explainers`; `JobType::ExplainerGenerate`), grounding ledgers (`intent_record/`, `intent_grounding/` JSONL via `g5-semantic` 14,186 lines), task completion spec validation (C.8), prompt templates (`uplift_guidance.tera`, `downlift_guidance.tera`, `view_sync_*`, `sync_*`, `merge_*`, `approval_review_*` — 52 templates, 7,265 lines, `g5d/src/prompts/templates/`), skills (`g5-sync-workflow`, `g5-greenfield`, `g5-node-explainer`, …).
- **[I]** The spec/ontology stack is orthogonal to the git/CRDT question except for two couplings: the rebase-pause gate on MCP mutations and the spec-delta gate in `g5d task complete`.

---

## E. LLM proxy (`g5d-worker/src/llm_proxy.rs`, 3,463 lines)

- **[O] Why:** "the per-task sidecar must eventually run with no provider credential and no NIC… this proxy injects the real credential host-side before forwarding to the real Anthropic upstream (or the already-configured gateway)" (`:1-33`). Guest env is repointed (`ANTHROPIC_BASE_URL` → loopback/unix socket; real `ANTHROPIC_API_KEY|ANTHROPIC_AUTH_TOKEN|CLAUDE_CODE_OAUTH_TOKEN` stripped; non-secret placeholders `sk-ant-…-no-identity-proxy-placeholder` stamped so Claude Code ≥2.1.175 still issues calls, `:108-140,387-460`).
- **[O] What it does:** byte-transparent streaming relay of `/v1/messages` (`:847-901,1502-1503`), header stripping/forwarding (`:1533-1620`), OpenRouter extras (session header `x-session-id`, prompt-cache rewrite via `openrouter_cache.rs`, generation-usage polling → `UsageRecordRequest` for cost accounting, `:1142-1400`), provider-quota header capture → `ProviderQuotaReportRequest` (`:978-1140`). Auth modes: `ApiKey` (x-api-key), `BearerToken`, `ClaudeCodeOAuthToken` (`:96-107`).
- **[O] Codex:** same relay used through a node-side TCP→unix bridge (`G5_CODEX_LLM_PROXY_UNIX_SOCKET`, `_TCP_PORT`, `codex.rs:3884-3893`); config.toml renders the proxy base as provider `openai_proxy` (`:900-930`).
- **[O] Depends on it:** no-identity microVM path (`agent_runner.rs`, `agent_drive.rs` preview attach), usage/cost counters ("UI cost counters use real provider accounting"), `task_secrets_for_resolved_agent` route filtering (`g5d-worker/src/lib.rs:60-110`).
- **[I]** In a "one VM per workspace with credentials" model the proxy is optional; it remains the only place that meters provider usage and the only way to keep provider keys out of the VM.

---

## F. Session store proxy and resume

- **[O] Claude:** SDK `sessionStore = G5ApiSessionStore` mirrors the transcript to g5-api (`sidecar_source.mjs:739-800,993-997`); table `claude_session_store_entries(chat_session_id, project_key, session_id, subpath, entry JSONB)` (`g5-api/src/db/migrations/110_claude_session_store_entries.sql:32-49`, tenant isolation `111`, idempotency `112`). Endpoints `/api/internal/chat-sessions/{id}/session-store/{append|load|sessions|entries|subkeys}` (`g5_api_session_store_proxy.rs:70-77`). The host-side proxy is loopback-only, path-pinned to one chat session (constant-time compare), method-pinned, injects the worker JWT, and prunes screenshots from `load` bodies (`:1-45,412`). Resume: worker passes `providerSessionId` → SDK `resume`; the sidecar probes `sessionStore.load()` and drops `--resume` if no rows exist (`:855-899`); `CLAUDE_CONFIG_DIR` is a per-request tmpfs so the local JSONL mirror is ephemeral (`:958-966`). Sticky runtimes additionally keep the sidecar process alive across turns (`RetainedSidecarTransport`, `claude_code.rs:5500-5550`).
- **[O] Codex:** `codex exec resume <thread_id>` + `codex_session_artifacts` rows (bytes of `sessions/**/*.jsonl`, ≤128 MiB, `138_codex_session_artifacts.sql:8-33`) uploaded/loaded through `/api/internal/chat-sessions/{id}/codex-session-artifacts[/content]`.
- **[I]** With a persistent per-workspace VM the resume machinery could shrink to "the provider's own on-disk session dir survives", but transcript mirroring to g5-api is also what the chat UI/history reads (not verified here).

---

## G. Size (lines)

| Group | Lines | Notes |
|---|---|---|
| `g5d-cli/src` total | 60,769 | `task.rs` 8,098; `tool.rs` 6,447; `spec.rs` 6,368; `lib.rs` 3,438; `git/head_reconciliation.rs` 3,328; `push.rs` 3,320; `host_up.rs` 2,680; `commit.rs` 2,183; `doctor.rs` 2,021; `golden_build.rs` 1,730; `workspace_manifest.rs` 1,683; `connect.rs` 1,537; `mcp.rs` 1,535; `api_client.rs` 1,455; `remote.rs` 1,423; `config.rs` 1,370; `tool_surface_contract.rs` 1,142; `stable_context.rs` 1,001; `add.rs` 993; `job.rs` 915; `chat.rs` 711; `preview_serve.rs` 605; `auth.rs` 578; `issue.rs` 554; `workspace.rs` 500; `git/safety.rs` 488; `worker_client.rs` 478; `merge.rs` 471; `task_workspace.rs` 470; `aql.rs` 459; `sync.rs` 402; `clone.rs` 322; `refresh.rs` 307; `init.rs` 247; `conflicts.rs` 207; `fork.rs` 195; `revert.rs` 126; `view.rs` 109; `intent_store.rs` 80 |
| CLI git/CRDT-bound subset (`commit, push, connect, conflicts, revert, fork, merge, task.rs, head_reconciliation, connected_head_port, git/safety`) | ≈19,800 | candidate removal/replacement mass |
| `g5d-tooling/src/tools` | 58,141 | `grounding_ops.rs` 22,137; `spec_ops.rs` 6,791; `task_lifecycle.rs` 5,918; `mod.rs` 4,606; `git_ops.rs` 2,706; `interface.rs` 2,697; `ontology_cycles.rs` 2,401; `file_ops.rs` 2,399; `experience_ops.rs` 1,583; `hil_escalation.rs` 1,578; `view_ops.rs` 817; `terminal.rs` 814; `pass_rate_tracker.rs` 646; `workflow_trigger.rs` 545; `browser_proxy.rs` 542; issue tools ≈1,180; `read_chat_session_context.rs` 308; `skill_invoke.rs` 181; `send_message.rs` 139; `search.rs` 122 |
| Internal-engine-only tools (`file_ops, git_ops, terminal, task_lifecycle, view_ops, search, interface, pass_rate, skill_invoke, send_message`) | ≈16,400 | dead with NaiveReact retired |
| `g5d-runtime-core` | 2,254 | `scm_credentials` 584; `provided_agent_tools` 515; `task_git_config` 459; `deferred_lifecycle` 382 |
| `g5d-foundation` | 1,252 top-level + `crdt/` 20,045 + `workspace/` 9,039 | `docs_write_policy` 527; `ontology_inventory` 547; `task_rebase_gate` 166 |
| `g5-crdt` / `g5-workspace-ops` / `g5-scm` / `g5-semantic` | 3,602 / 4,681 / 10,427 / 14,186 | |
| `g5d-worker/src` | 65,733 | `claude_code.rs` 22,983; `codex.rs` 14,915; `lifecycle.rs` 7,965; `llm_proxy.rs` 3,463; `view_task_builder.rs` 2,128; `runtime_keeper.rs` 1,746; `task_agent.rs` 1,420; `g5_api_session_store_proxy.rs` 1,215; `mcp_preflight.rs` 966; `task_env.rs` 417; `tool_path_projection.rs` 427; `provided_agent_profile.rs` 276 |
| `g5d/src` | 19,182 (+ `worker/` 28,664 + `analysis/` 26,084) | `prompts/mod.rs` 9,907; `worker/mod.rs` 19,397; `worker/bootstrap.rs` 4,859; `cli/worker.rs` 3,199; `agent/agent_runner.rs` 1,984; `agent/ws_inbox.rs` 1,641; `agent/skills` 1,898 |
| Sidecars (source) | 4,242 | `claude/sidecar_source.mjs` 2,281; `codex/sidecar_source.mjs` 1,732; `mcp_preflight.mjs` 229 |
| Prompt templates | 7,265 (52 `.tera`) | |
| `assets/agents` | 26,027 | `CLAUDE.md` 807; `AGENTS.md` 765; Claude skills 6,919; hooks 4,170 |

---

## Coupling

1. **CLI ⇄ CRDT/authority.** `commit`, `push`, `connect`, `conflicts`, `revert`, `task complete/rebase`, `workspace push/switch`, `fork` all encode the CRDT draft model or the head-operation authority protocol (`.g5/task_commit/*`, `refs/g5/staged/*`, `drafts/commit-all`, `head-operations`). Removing CRDT removes the client half; the server half (git route + finalizer) is the governance point to keep.
2. **MCP tools ⇄ CRDT.** Grounding tools are plain JSONL writers, but `GroundingToolState::with_crdt_peer` and the rebase-pause guard tie them to the task-commit bridge (`tool.rs:2385-2410`).
3. **`task complete` ⇄ ontology.** Spec-delta validation is a hard gate on completion (`task.rs:4497-4584`).
4. **Worker ⇄ profile.** The worker rewrites `.mcp.json`, `CLAUDE.md` appendix, deletes skills per env flags, and preflights the MCP tool list; the profile's `requiredSkills` and `g5-tools` name are load-bearing in Rust (`claude_code.rs`, `codex.rs`, `mcp_preflight.rs`).
5. **Lifecycle ⇄ credentials.** Host-driven mode deliberately withholds `G5D_TOKEN`, so `g5d task complete` cannot run in-guest and completion is deferred to files (`.g5/deferred_task_transitions`) the worker replays; Codex is always in this mode.
6. **Prompt text ⇄ CLI names.** `CLAUDE.md`/`AGENTS.md`/skills hard-code `g5d commit|push|remote|task …`; any CLI change needs profile edits (and `merge_user.tera`/`sync_user.tera` render lifecycle instructions from Rust).

## Simplification candidates

Keep (K):
- `g5d issue *`, `g5d chat context *`, `g5d aql *`, `g5d job status|wait|list|cancel|create`, `g5d remote *` (or its successor "publish/PR" op), `g5d auth *`, `g5d doctor`, infra entrypoints (`worker`, `host-up`, `oci`, `preview-*`, `golden-build`), MCP `human_escalation`, issue tools, `read_chat_session_context`, AQL tools; server-side git route governance (`refs/g5/staged` create-only + finalizer) as the git-boundary policy. Risk: low — these are API wrappers.

Replace with native (N):
- `g5d commit` → `git commit`; `g5d push` → `git push origin <branch>` against the g5-api mirror with a **real** per-task credential; `g5d task rebase` → `git rebase`; `g5d revert` → `git checkout/clean`; `job merge`, `workspace switch`, `fork`, `refresh`, `clone`, `init` → `git` + a thin "open/create workspace" API call; registry `file_ops/git_ops/terminal/search` → delete (unused). Risks: (a) the git route today only accepts staging refs — it must learn to accept `refs/heads/<workspace-branch>` pushes and apply policy (branch = workspace, fast-forward-only, actor/task checks) there; (b) `G5_REPO_CREDENTIALS` and the credential store already leak SCM tokens into the task env — decide whether the VM holds upstream (GitHub) creds or only mirror creds; (c) `task complete` currently bundles commit+push+validation+transition — a native flow needs a small "finish task" API that reads the pushed head instead of doing git itself.

Remove with CRDT/authority (R):
- `connect`, `conflicts`, `intent-store`?, `head_reconciliation.rs` (3,328), `connected_head_port.rs`, `g5d-foundation/crdt` (20,045), `g5-crdt` (3,602), task-commit bridge in `commit.rs`, rebase pause/gate (`task_rebase_gate.rs`, `TASK_REBASE_PAUSE_FILE_REL` checks in `tool.rs`), `requires_post_turn_fs_scan` plumbing, `fs_diff.rs`/`attribution.rs` (worker), `workspace push`. Risk: the UI's live "draft" view of uncommitted files and per-line attribution depend on CRDT (frontend not audited here).

Unclear (?):
- Ontology/grounding MCP surface (25 tools, 22k+ lines in `grounding_ops.rs`) and the spec-validation gate — product decision, not a git question; if kept, drop only the rebase-pause coupling.
- `browser_*` proxy (agent could register `@playwright/mcp` itself; g5d adds origin policy + gating).
- Experience tools (`.g5/experience/`) — could be a skill over files.
- `spec sync`, `sync`, `view sync` CLI wrappers — keep if the corresponding jobs stay.
- Profile hooks (advisory only, 4,170 lines) — candidates for deletion or reduction to `SessionStart` + `Stop`.

## Open questions

1. Should the guest hold a credential for the g5-api git mirror (so plain `git push` works) — a per-task JWT (`x-worker-token`) or a per-workspace deploy token? Today host-driven mode intentionally withholds it.
2. Should the guest ever hold upstream (GitHub) credentials, or must upstream publication stay a server-side `remote push`? Today chat jobs get the user's SCM token in-guest (`.g5/task-<id>.git-credentials` and `G5_REPO_CREDENTIALS`).
3. Governance at the git boundary: is "one workspace = one branch, fast-forward only, pushes accepted only from the workspace's VM/task" the intended rule for `git-receive-pack`, replacing staging refs + finalizer? Who resolves the PR-body Project Doc / preview-profile conventions that `g5d commit` prompts today?
4. Do "finish task" semantics survive (result summary, spec validation, checkpoint receipts), or does a task end when the VM's branch head moves and the agent returns?
5. Is the intent/grounding ledger (JSONL under `intent_record/`, `intent_grounding/`) in scope to keep? It is the bulk of the MCP surface and of `g5d-tooling`.
6. Which UI features read CRDT state (live drafts, attribution, conflicts) — do they move to "git status of the VM" polling?
7. Keep the LLM proxy for metering/credential isolation, or let the VM hold provider keys (simpler, no usage accounting)?
8. Is the no-identity carrier path (`agent_runner.rs`, vsock mux, session-store proxy) still needed once each workspace has a long-lived VM with a network identity?
9. The shipped `/commit` and `/pr` slash-commands already teach raw git + `gh pr create`; is that the intended target behaviour (so the contradiction resolves by deleting the CRDT instructions), or should PR creation remain a platform call?
