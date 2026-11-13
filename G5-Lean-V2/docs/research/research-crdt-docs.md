# G5-Lean research: CRDT layer, drafts, "authority", and the document editing model

Repo: `/Users/zliu/Workspace/G5-Lean` (read-only). Paths are relative to that root; `migrations/` = `backend/g5-api/src/db/migrations/`.
Convention: **OBSERVED** = read in code/migrations at the cited `file:line`. **INFERENCE** = my reading of how observed pieces combine; nothing was executed.
`docs/reference/ARCHITECTURE_DISCUSSION.md` is cited only as historical intent; where it disagrees with code, code wins.

## TL;DR

- OBSERVED: One Yjs (`yrs`) document per *draft session*, keyed `(workspace_branch_repo_id, task_id NULL|task)` — not per file, not per workspace. Inside the doc each file is a `Y.Text` keyed by repo-relative path; a `_meta` `Y.Map` holds `dirty_files` and `deleted_files`. Only dirty files carry meaning; clean `Y.Text` entries are ignored by readers.
- OBSERVED: The doc is hosted in `g5-api` (`DocumentStore`/`DocumentHandle`), persisted only in Postgres (`draft_sessions.yjs_state` snapshot + `draft_updates` log; 200 ms debounce, compaction at 50 updates/300 s, eviction after 60 s idle), and synced over a y-websocket binary WebSocket at `/api/v1/yjs/:workspaceId/:branchId?gitSha=…` with three G5-private control commands on opcode `0x7f` (persistence barrier, task commit, head advance).
- OBSERVED: Peers are the browser (`yjs`/`y-websocket`/`y-monaco`), the in-VM `g5d` worker and host supervisor (`CrdtPeer`), and `g5d connect` on a laptop. Provider agents (Claude Code, Codex) are **not** peers: they write plain files and a 2 s scan + final barrier pushes changes into the CRDT. G5-native tools write the filesystem first, then enqueue a CRDT delta.
- OBSERVED: Git commits are built server-side from CRDT content with an in-memory index against server bare repos (`RepoStorage`); no worktree. Branch tips are Postgres columns (`workspace_branch_repos.head_commit`), advanced only via a durable `workspace_head_operations` row ("authority advance"). That machinery is about the git boundary and is separable from the CRDT overlay.
- OBSERVED: UI reads never touch the VM. `RepoReadView::read_files` serves dirty-overlay content from the in-memory Y.Doc (cold-loaded from Postgres) or committed bytes from the server bare repo at the DB-recorded SHA. No worker-RPC file read exists today.
- OBSERVED: `docs` rows are a metadata projection over markdown/HTML files in the workspace docs git repo (`docs/`, `views/`, `designs/`, `explainers/`); `ontology_nodes` projects `.semlift/specs/<component_id>.semlift.md`. Doc reads come straight from the repo view (`is_virtual: true`); doc/ontology writes go to the CRDT overlay and reach git only at draft commit.
- Size: ≈46k backend lines exist only for CRDT/drafts/connected-commit; ≈12k frontend lines incl. tests. Far larger files are threaded with CRDT assumptions (`workspace_finalizer.rs` 7.6k, `sticky_runtime.rs` 8.1k, `task_service.rs` 33k, `supervisor.rs` 29.7k, `doc_service.rs` 11k, `SpecEditor.tsx` 11.6k, `WorkspacePage.tsx` 13.4k).

---

## A. What the CRDT layer is

### A.1 Data structures

OBSERVED
- `backend/g5-crdt/src/doc.rs:12-37` — `CrdtDoc` wraps `yrs::Doc`; file content in `Y.Text` keyed by path (`"src/main.rs"`); `_meta` `Y.Map` with nested `dirty_files` (`:34`) and `deleted_files` (`:37`). Not thread-safe → `tokio::sync::Mutex` in g5-api (`:20-24`; `authority.rs:503-509`). `:148-157` `encode_state()` is the `draft_sessions.yjs_state` blob; `:238-259` `set_text_by_diff` splices minimally so concurrent full-file writes don't duplicate bodies.
- `backend/g5-api-core/src/crdt/mod.rs:70-77` — `DocKey = (workspace_id, workspace_branch_id, git_sha, Option<task_id>)`; `None` = durable canonical branch draft, `Some` = isolated task overlay. `:82-85` `CrdtSessionScope::{Branch, Task(ValidatedTaskDraftScope)}`. `:139-171` `DocumentStore { documents: DashMap<DocKey, Arc<DocumentHandle>>, loading_locks, persistence, awareness, event_bus, tenant_pool, authz, repo_storage, config }`.
- `authority.rs:503-583` — `DocumentHandle { doc: Mutex<yrs::Doc>, awareness: Mutex<yrs::sync::Awareness>, broadcast_tx, awareness_tx, session_id (= draft_sessions.id), org_id, teardown_admission, task_scope, persist_enabled, client_count, last_activity, persist_notify, last_persisted_state_vector, commit_gate, commit_buffering, evicted_tx/rx, retired }`. `:537-541`: worker task documents are isolated overlays that must not write into the shared branch draft log (`persist_enabled=false` unless `G5_TASK_CRDT_PERSISTENCE_ENABLED`).
- `backend/g5d-worker/src/host_support.rs:9-19` — Chat jobs write to the durable *branch* draft (`task_id=None`); every other job type writes to its per-task overlay.
- Uniqueness: `migrations/153` `uq_draft_sessions_branch_repo (workspace_branch_repo_id) WHERE task_id IS NULL`; `154` `uq_draft_sessions_task_repo (workspace_branch_repo_id, task_id)`.
- Lazy per-file seeding: `crdt/overlay.rs:30-46` "Clean retained `Y.Text` entries are ignored unless the path is present in `_meta.dirty_files`; otherwise a clean post-commit draft shell could shadow committed git content." The browser seeds a file's `Y.Text` from REST content on first edit, electing one tab as seed leader (`frontend/src/lib/crdt.ts:328-350`; `CodeEditor.tsx:595-625`).
- Limits: overlay content is `String` (`overlay.rs:10`); the worker skips non-UTF-8 files (`g5d-foundation/src/crdt/mod.rs:1938-1939`); materialize/outbound caps 10 MiB (`:71`, `:847-849`); doc cap 100 MiB (`mod.rs:185`); materializer skips `target, node_modules, dist, .git, __pycache__, .tox, build, .next` (`g5-crdt/src/materializer.rs:34-43`).

INFERENCE: the historical "one CRDT draft per (workspace, branch)" (`ARCHITECTURE_DISCUSSION.md:5182`) is stale; multi-repo workspaces get one peer per repo routed by `CrdtPeer::register_overlay_peer` (`mod.rs:1092`). Binary files cannot live in the overlay at all.

### A.2 Persistence — Postgres only, no blob storage

OBSERVED
- `migrations/001_initial_schema.sql:261-290` `draft_sessions(id, workspace_branch_id, yjs_state BYTEA, compacted_at, compacted_seq, pending_updates, diff_summary JSONB, dirty_files TEXT[], dirty_count, status CHECK IN (active, committing, discarding, stale), last_activity_at, persisted_at, created_at, UNIQUE(workspace_branch_id))`; `:293-298` `draft_updates(seq BIGSERIAL, session_id, update_data BYTEA, created_at)`; `:303-311` `draft_participants(id, session_id, user_id, role editor|reviewer, connected_at, cursor_data)`.
- Evolution: `043` `draft_sessions.head_commit`; `062` `workspace_branch_repo_id`; `151-157` `task_id` + composite FK to `tasks(org_id,id)` + the two partial unique indexes; `195` drops `stale` ("SQL cannot authoritatively inspect Yjs state"); `207` `pending_update_bytes` + compaction index.
- `backend/g5-api-core/src/crdt/persistence.rs:416-423` cold start: `SELECT yjs_state, compacted_seq, org_id, status, head_commit FROM draft_sessions … FOR SHARE`, then replay `draft_updates WHERE seq > compacted_seq` (`:497, :588`). `:835` `INSERT INTO draft_updates (org_id, session_id, update_data)`; `:862-874` `UPDATE draft_sessions SET pending_updates+1, pending_update_bytes+$4, last_activity_at, dirty_files=$2, dirty_count=$3 … WHERE status='active' AND head_commit=$6`. `:1305` `compact()`; `:1559-1571` `ensure_branch_session_with_conn` inserts with `head_commit` copied from `workspace_branch_repos`. `:1930-1958` `reconcile_clean_branch_session_forward` moves a session's `head_commit` forward only when `dirty_count = 0 AND cardinality(dirty_files) = 0`.
- `mod.rs:176-185` (`CrdtConfig::from_env` doc): `G5_CRDT_PERSIST_INTERVAL_MS=200`, `G5_CRDT_GRACE_PERIOD_SEC=60`, `G5_CRDT_COMPACTION_THRESHOLD=50`, `G5_CRDT_COMPACTION_MAX_AGE_SEC=300`, `G5_CRDT_REPLAY_MAX_BYTES=256MiB`, `G5_CRDT_MAX_DOC_SIZE_BYTES=100MiB`, `G5_TASK_CRDT_PERSISTENCE_ENABLED=false`, `G5_CRDT_PERSISTENCE_BARRIER_EXACT_V2_ENABLED=false`. `authority.rs:3764` `persist_task` (debounced), `:3884` `grace_period_scanner` (eviction). `mod.rs:1403-1416` `get_or_load` doc: in-memory hit or cold start under a per-key loading lock.
- Connected-commit side tables: `161_task_commit_requests.sql:15-27` `(org_id, task_id, request_id, workspace_id, worker_id, payload_sha256, status pending|succeeded|failed, terminal_result)`; `279_task_commit_frontier_cuts.sql:5-22` (per-repo `state_vector`, `yjs_state`, `manifest`); `280_task_commit_frontier_receipts.sql:5-23` (10-minute-TTL immutable snapshot receipts).

### A.3 Sync protocol

OBSERVED
- Route: `backend/g5-api-routes/src/routes/mod.rs:455-458` `GET /api/v1/yjs/:workspaceId/:branchId` → `crdt_ws_adapter` (`:544-575`: `gitSha` required; optional `taskId`; optional teardown params `teardownRuntimeId/Generation/OperationId/CrdtSessionId`) → `CrdtAuthority::handle_ws_upgrade` (`:756-772`) with `CrdtWsUpgradeRequest { workspace_id, workspace_branch_id, org_id, git_sha, task_id, claims, workspace_read_only, mode }` (`authority.rs:1800-1812`); `CrdtConnectionMode::{Ordinary{worker_fence}, Teardown{admission}}` (`:1760-1768`); headers `x-g5-crdt-session-id`, `x-g5-crdt-teardown-result` (`:1770-1772`). Worker URL shape: `{api_ws_url}/api/v1/yjs/{ws}/{branch}?gitSha=…[&taskId=…]` (`g5d-foundation/src/crdt/mod.rs:304`, `sync.rs:4057-4067`).
- Wire format (`authority.rs:19-24, 74-85`): standard y-websocket/lib0 — byte0 `MSG_SYNC=0 | MSG_AWARENESS=1 | MSG_QUERY_AWARENESS=3`; sync byte1 `SYNC_STEP1=0 | SYNC_STEP2=1 | SYNC_UPDATE=2`. Keepalive sync-step-1 every 15 s (`:98`); 100 MiB frame caps (`:101-103`).
- G5-private frames on opcode `0x7f` (`g5-crdt/src/lib.rs:31-36`):
  - **command 1** persistence barrier — v1 `[0x7f,1,1]`, v2 exact `[0x7f,2,1,len,state_vector]`; ack failure codes `Unauthorized, RetiredGeneration, PersistenceTimeout, PersistenceFailure, InvalidScope, MalformedFrame, FrontierMismatch, ProtocolNotActivated` (`:42-51`); v2 success carries a `receipt_id` (`:213-230`).
  - **command 2** task commit (`task_commit.rs:14-16`): worker→server `TaskCommitCommandRequest { request_id, task_id, message, repositories: Vec<TaskCommitRepositoryFrontier{ workspace_branch_repo_id, persistence_receipt_id, expected_head_commit, materialized_head_commit, state_vector }> }` (`:112-135`); server→worker `TaskCommitTerminalResult { request_id, status Succeeded|Failed, results[{workspace_branch_repo_id, repo_name, branch_name, commit_sha}], failure_code }` (`:140-197`); failure vocabulary `:149-171` (`StaleHead, RebaseRequired, FrontierNotDurable, CommitConflict, RecoveryPending, …`).
  - **command 3** head advance (`head_advance.rs:8-19`): server→peer `BranchHeadAdvanceNotice { workspace_branch_repo_id, old_head_commit, new_head_commit }`.
- Separate JSON hub at `/api/v1/ws` (`routes/mod.rs:454`), `Channel::Draft{workspace_id, branch}` (`ws/channels.rs:62-66`), events `draft.update{workspaceId, workspaceBranchId, workspaceBranchRepoId, branch, dirtyFiles, dirtyCount}`, `draft.commit{…, commitSha}`, `draft.discard{…, reason, previousHead, currentHead}`, `draft.behind{…, draftHead, currentHead, behindCount}`, `draft.participant{…, action joined|left, user}` (`ws/events.rs:253-343`), plus `workspace.branch_updated` and `workspace.authoritative_cut_advanced` (`:94-121`). The hub code has zero CRDT references (grep on `worker_hub.rs`, `channels.rs`, `events.rs`); it only carries notifications.

### A.4 Peers

OBSERVED
1. **g5-api** hosts the authority (`authority.rs:1-17`; `CrdtAuthority { store, persistence, awareness_mgr, config, task_commit_coordinator }` `:1500-1507`).
2. **Browser**: `frontend/src/lib/crdt.ts:17-19` (`yjs`, `y-websocket`); `:354-370` URL builder; `:441` `POST /api/v1/auth/ws-ticket`; `:542` `new WebsocketProvider`; `:712-730` `useDraftCRDT` contract. Editors bind via `y-monaco` (`CodeEditor.tsx:1028-1033`) or direct `Y.Text` ops (`SpecEditor.tsx:6817-6901`).
3. **In-VM worker / host supervisor**: `g5d-foundation/src/crdt/mod.rs:1-15` "worker-side CRDT peer"; `CrdtPeerConfig.api_ws_url` (`:699-701`); created in `g5d-worker/src/lifecycle.rs:3810, 4491…` and `g5d-host/src/supervisor.rs:10831` (`connect_crdt_peer_with_retry`, `host_support.rs:22-34`).
4. **Laptop**: `g5d-cli/src/connect.rs:1-5` "`g5d connect` keeps clean local repo checkouts synchronized with the G5 durable branch drafts… reuses `CrdtPeer`"; conflicts with third-party agents become sidecars in `.g5/conflicts/<path>` (`conflicts.rs:1-6`) announced over the Unix socket `.g5/crdt.sock` (`mod.rs:900`).
5. **Provider agents (Claude Code / Codex)** are not peers — see B.2.

---

## B. Write paths

### B.1 G5-native tools inside the VM (`backend/g5d-tooling/src/tools/`)

OBSERVED (`file_ops.rs`)
- `write_file`: `tokio::fs::write(absolute_path, content)` (`:159`) **then** `crdt_peer.enqueue_file_write_for_repo(repo_root, crdt_path, content)` (`:165-166`). Filesystem first, CRDT second.
- `edit_file` (replace): `validate_edit_for_repo` (`:730-731`; attribution — "agents never silently overwrite human edits", `g5d-foundation/src/crdt/attribution.rs:1-10`) → `apply_authoritative_edit_replace_for_repo` (`:767-768`; server round-trip `POST /api/internal/tasks/:id/crdt/edit-file-replace` → `DocumentHandle::edit_file_replace_authoritative_already_fenced`, `routes/internal.rs:3259-3261, 9112, 9338`; DTO `g5-internal-api/src/dtos.rs:2168-2177`) → `tokio::fs::write` (`:781`, `:827`) → `enqueue_file_write_for_repo` (`:788-789`).
- `delete_file`: `enqueue_file_delete_for_repo` (`:926-927`). `read_file`/`list_directory` first call `ensure_materialized_clean_for_reads` (`:426-427`, `:1027-1028`) and `record_read_for_repo` (`:469-471`).
- `run_in_terminal`: afterwards `crdt_peer.detect_fs_changes()` then `flush()` (`terminal.rs:116-118`); `g5d-worker/src/fs_diff.rs:1-6` = mtime+hash snapshot diff.
- Git tools: `git_ops.rs:4-11` "Git operations reflect the combined state of the base commit plus the CRDT draft overlay… ensure CRDT state is flushed before any git"; `:380-385, 436`. `g5-workspace-ops/src/commit.rs:734, 812` take `&dyn DraftState` (dirty gate; `g5-workspace-ops/src/draft.rs:17-25`). `g5d-cli/src/git/safety.rs:63, 433` blocks branch changes: "Changing branches would desync the CRDT draft layer".
- Spec/grounding tools: `spec_ops.rs` 5 CRDT enqueue calls / 25 fs writes; `grounding_ops.rs` 2 / 197; `view_ops.rs` 0 / 5 (grep counts).

### B.2 Provider agents — filesystem only, scanned into the CRDT

OBSERVED
- `g5d-worker/src/lifecycle.rs:2309-2313` `spawn_provider_fs_crdt_sync(crdt_peer, task_id, workspace_path, 2 s)` around `task_agent.run()`, then `:2321-2325` `provider_fs_lifecycle_barrier("final")`. Host: `g5d-host/src/supervisor.rs:11699-11705`.
- `g5d-foundation/src/crdt/mod.rs:1768-1783` `detect_fs_changes` ("Compares filesystem state against known state and enqueues CRDT deltas"); `:2716-2725` `provider_fs_sync_once`; `:2734-2755` lifecycle barrier = detect + flush + `flush_and_persist` (command 1); `:2782-2809` `provider_fs_commit_barrier` fails closed if materialization is not clean.

### B.3 Inbound (server → worker filesystem)

OBSERVED
- `g5d-foundation/src/crdt/sync.rs:1-7`: outbound deltas flushed after each tool call; "inbound human edits are applied between tool calls at safe points". `materialize.rs:1-6` atomic temp+fsync+rename; `conflict_detector.rs:1-11` preserves third-party versions as sidecars before overwriting. `CrdtPeer::set_safe_to_materialize` exists (`mod.rs:4559`); grep found no callers in `g5d-worker/src` or `g5d/src` (observed absence only).
- Teardown of a sticky chat runtime drains state via `teardown_flush` under a server-issued capability (`mod.rs:2136-2160`; `authority.rs:1766-1767` `Teardown` mode).

### B.4 Browser / REST writes

OBSERVED
- Live typing: `CodeEditor.tsx:595-625` seeds `Y.Text` then `y-monaco` `MonacoBinding` (`:1028-1033`); `markFileDirty` sets `_meta.dirty_files[path]="true"` (`crdt.ts:60-85`). `SpecEditor.tsx:6817-6901` binds a `Y.Text` keyed by `nodeId` (a file path) with a `Y.UndoManager` tracking `LOCAL_EDIT_ORIGIN` (`:7260-7279`; `crdt.ts:42`). Structured/rich helpers write into the same `Y.Text` in one transaction (`questionAnswerWrite.ts:7-24`, `htmlDocAnswerBridge.tsx:153-212`).
- REST: `PUT /api/v1/branches/:branchId/files/*path` (`routes/files.rs:194`) → `handle.set_file_content` + `document_store.flush_document` (`:599-606`); doc `:571-575`: "does not mutate git; draft commit remains the only path that advances branch history." 409 if no overlay (`:592-596`). Paths under `.git/` or G5 runtime metadata are rejected (`:810-823`). Frontend uses it only to create an empty file (`WorkspacePage.tsx:6171-6180`).
- Ontology: `PUT /api/v1/workspaces/:id/ontology/:nodeId/content` (`routes/ontology.rs:457`) → `get_or_load` (`:714`) → overlay required (`:746`) → `set_file_content` (`:785`) → `flush_document` (`:791`).
- Docs: `PUT /api/v1/workspaces/:id/docs/:docId` (`routes/docs.rs:731`) → `DocService::update_doc` (`doc_service.rs:9265-9372`) → `write_file_to_docs_branch` (`:5763-5837`): if the docs branch is materialized (`:5757-5761`), `write_file_to_docs_draft` (immediately above `:5757`) = `get_or_load` → `set_file_content` → `flush_document`; if the docs branch is unborn, `prepare_initial_files_commit` on the server bare repo + `prepare_initial_api_workspace_operation` (an authority advance, finalized by `routes/docs.rs:88-95, 1567`).

### B.5 CRDT → git commit

OBSERVED — UI path
- Routes (`routes/drafts.rs:262-315`): `GET /api/v1/drafts/:id`, `POST /:id/commit`, `POST /:id/discard`, `GET /:id/participants`, `GET /:id/pre-commit-check`; `GET /api/v1/workspaces/:id/drafts`, `GET …/drafts/diff`, `POST …/drafts/commit-all`; `GET|POST /api/v1/branches/:id/drafts` (GET "ensures" a draft exists; cf. `ARCHITECTURE_DISCUSSION.md:6335-6380`).
- `draft_service.rs:1-13`: "CAS-guarded commit… immutable commits and operation refs are prepared before one short application transaction locks and revalidates the complete expected database cut." `commit_draft` (`:2616`): CAS `branch.head_commit == req.expected_head_commit`; snapshot the live doc; `get_dirty_files`; read base files from git; compute `semantic_dirty_files`; `:2892-2917` build exact tree changes and call `g5_workspace_ops::prepare_tree_commit_from_exact_base(git_repo, {base_commit, changes, message, author})` — `g5-workspace-ops/src/commit.rs:534` doc: "uses an in-memory index and never creates or mutates a branch, worktree, checkout, or on-disk index"; `:2927-2942` `prepare_draft_commit_rollover` (`workspace_ops.rs:1166-1215`: reset the draft snapshot onto `new_head`); `:2945-2960` remote push DISABLED (tech-debt comment).
- Finalization: `workspace_finalizer.rs` (`create_connected_commit_all_operation` `:675`, `PreparedAuthorityAdvance::finalize` `:1654-1682`, `finalize_with_task_crdt_rollovers` `:1780`, `finalize_connected_commit_all_operation` `:4302`, `project_committed_operation` `:3337`; 15-minute operation lease `:58`) then `advance_notifier.rs:1-9` ("reconstructs the complete cut from Postgres and owns the fixed durable projection set").

OBSERVED — connected worker path (`g5d commit` inside a task)
- `g5d-cli/src/commit.rs:1-5`: "Connected workspaces commit the authoritative server-side draft through g5-api. Unconnected/offline workspaces keep the legacy local git checkpoint behavior. Neither mode pushes to remotes." The CLI writes `.g5/task_commit/<request_id>.request.json` and polls for `.result.json` (`g5-crdt/src/task_commit.rs:18-21`; `commit.rs:895-975, 1002`; timeout `G5D_TASK_COMMIT_TIMEOUT_SECS` default 300).
- The live `CrdtPeer` bridge (`task_commit_bridge.rs:1`) runs `provider_fs_authoritative_task_commit` (`mod.rs:2814-2829`): commit barrier → per-repo command-1 barrier → capture `TaskCommitRepositoryFrontier` → send command 2.
- Server `crdt/task_commit.rs:1-5, 59-65, 161-230`: `TaskCommitCoordinator` revalidates the worker lease (`TaskCommitActor`), dedupes via `task_commit_requests` (payload sha; replays a terminal result), stores frontier receipts/cuts, and "delegates mutation to `DraftService`'s existing application transaction" (`commit_all_for_worker_task`, `draft_service.rs:3203`).
- Afterwards the server sends command 3; the peer adopts the new head via `ConnectedHeadAdoptionPort` (`mod.rs:735-738, 1385-1388`). Rebase operations: `TaskCommitFileOperation::{PrepareLocalCheckpoint, PauseTaskRebase, VerifyTaskRebase, AdvanceTaskRebase, AdoptTaskRebase}` (`task_commit.rs:65-73`) with the durable pause marker `.g5/task_commit/rebase.pause.json` (`:21`).

OBSERVED — non-connected path: `g5d push` uploads objects over Git Smart HTTP and calls `POST /api/v1/workspaces/:id/head-operations/:op/finalize` (`g5d-cli/src/push.rs:1285-1290, 1713`); `workspace_head_operations.mode='g5d_push'` requires `crdt_scope_kind='none'` (`196:50-53`). Workers clone from g5-api's `/git/:repoId` with the worker-session JWT (`g5d-worker/src/lifecycle.rs:934-970`; `g5d-foundation/src/workspace/repo_cache.rs:135`).

OBSERVED — discard: `POST /drafts/:id/discard` → `discard_draft` (`draft_service.rs:3009`); the browser resets its Y.Doc on `draft.discard`/`draft.commit`/`workspace.branch_updated` (`ws.ts:850-873`, `CRDT_LIFECYCLE_RESET_EVENT`).

### B.6 Sequence walkthroughs (INFERENCE stitched from the observed pieces above)

1. **Human edits a file in the browser and commits.** Open file → `GET …/files/*path` (committed or overlay) → first keystroke seeds `Y.Text` + `markFileDirty` → y-websocket `SYNC_UPDATE` → `DocumentHandle` applies + broadcasts → `persist_task` appends to `draft_updates` and updates `dirty_files` (200 ms) → `draft.update` event → other tabs/workers materialize. Commit → `POST /drafts/:id/commit{expected_head_commit, message}` → CAS + tree commit in bare repo → `workspace_head_operations` row committed, `workspace_branch_repos.head_commit` advanced, draft snapshot rolled to the new head → `draft.commit` + `workspace.authoritative_cut_advanced` → UI resets the Y.Doc to `gitSha=new`.
2. **Claude Code edits inside the VM; user watches in the UI.** Agent writes `src/x.rs` on disk → every 2 s `detect_fs_changes` enqueues a delta → `SyncEngine` flushes → server applies → `dirty_files` persisted → `GET …/files/src/x.rs` returns overlay bytes with `draft_dirty=true` (VM state never consulted). End of turn: `provider_fs_lifecycle_barrier` waits for a persistence-barrier ack so the durable draft is complete before the turn is reported done. Commit: `g5d commit` → command 2 → server commit-all from the Y.Doc → command 3 → worker checkout adopts the new head.
3. **VM asleep / no worker.** Reads unchanged (bare repo + Postgres-loaded Y.Doc); browser edits persist to Postgres; commit is server-side. On the next task start the worker cold-starts by cloning from `/git/:repoId` and `batch_materialize` of dirty files (`materialize.rs:181`).

---

## C. What "authority" is

OBSERVED
- `migrations/197_authority_branch_scope_binding.sql:1-7`: "normalize workspace, job, and task cuts onto one typed per-repository authority-member relation. `workspace_branches` is the scope member and `workspace_branch_repos` owns the exact repo-local SHA. `draft_sessions` remains a CRDT overlay only." Columns `:9-50`: `workspace_branches.authority_scope_kind IN (workspace, job, task)`, `job_id`, `task_id`, generated `authority_scope_id`; comments `:394-401`.
- `196_workspace_head_operations.sql:6-8` `workspaces.authority_advance_seq`; `:10-71` `workspace_head_operations(target_kind workspace|job|task, target_id, crdt_scope_kind none|canonical|task, crdt_task_id, mode connected_commit_all|g5d_push, idempotency_key, payload_sha256, status preparing|prepared|finalizing|committed|failed, lease_generation, lease_expires_at, terminal_result, workspace_advance_seq, projection_completed_at, cleanup_completed_at)`; `:117-150` `workspace_head_operation_repos(expected_old_head, proposed_new_head, staging_ref='refs/g5/staged/<op>/<repo>', committed_ref='refs/g5/committed/<op>/<repo>', staging_status unchanged|pending|staged|committed|stale|cleaned, draft_session_id)`.
- `198`: handoffs replayable by `advance_id` (`search_reindex_queue.advance_id`, `branch_integration_queue.advance_id`, `workspace_advance_adapter_receipts`). `206`: "authority head application is explicit in the two finalizers… stop every UPDATE trigger from moving either head or materialization state." `208`: validates complete typed cuts. `209`: provenance (`source_workspace_id, source_kind, source_id, producer_kind, producer_id, producer_attempt_id, correlation_id, source_operation_id`), `workspace_head_operation_source_repos`, rename to `workspace_advance_consumer_receipts` + new `workspace_advance_consumer_inputs`.
- Contracts `backend/g5-contracts/src/types/dto.rs:22-26` `AuthorityTarget::{Workspace, Job, Task}`; `:87-91` `CrdtScope::{None, Canonical, Task}` (`:111-130` valid only for matching targets); `:136-160` `RepoBindingVersion { …, head_commit, base_head_commit, canonical_ref, root_path }` — "The branch name is descriptive only and must never be resolved to answer a content read"; `:164-170` `CommittedBindingSnapshot { target, org_id, workspace_id, repos }`; `:192-212` `HistoricalGitVersion { repo_id, head_commit, reason }`.
- `workspace_reader.rs:1-6`: "Callers must select the repository and commit from an authoritative workspace snapshot… never fall back to the mutable repository checkout."

INFERENCE — the invariant maintained: *the current tip of every repo in a workspace/job/task scope is a Postgres value; it changes only through a single-target, CAS-checked (`expected_old_head`), leased, idempotent operation that first stages immutable git objects under operation-scoped refs and then flips all repo members of the cut in one short transaction; every downstream projection (search, docs/ontology projection, external git, task lifecycle) replays from the committed operation id.* The CRDT is explicitly not authority, but the connected-commit finalizer also rolls the draft snapshot forward (`finalize_with_task_crdt_rollovers`; `PreparedTaskCrdtRollover` `task_service.rs:7116`), which is the main place the two are entangled.

---

## D. Read paths

OBSERVED
- `GET /api/v1/branches/:branchId/files/*path` (`routes/files.rs:193, 504-569`) → `resolve_branch_repo_read` (`:226-256`: attaches the connected overlay when the target has a CRDT scope, else committed-only) → `RepoReadView::read_file` → `{path, content, size, encoding utf-8|base64, node_id, draft_dirty}`. Tree: `GET …/files` (`:192`); frontend polls it every 2.5 s (`useFiles.ts:20`) because "Worker / CRDT draft updates are produced out-of-band" (`:33-36`).
- `workspace_reader.rs:676-681` `RepoReadView { repo: RepoBindingVersion, committed: CommittedRepoReadView, overlay: Option<Arc<DocumentHandle>>, dirty_files }`; `:747-818` `read_files`: a path in `dirty_files` is classified via the live handle (`DraftFileOverlay::Content|Deleted|Clean`, `overlay.rs:7-11`); everything else "read through one immutable Git repository handle" at `repo.head_commit`. `RepoFileSource::{DirtyOverlay="dirty_draft", CommittedCut="committed_cut"}`, `RepoFileRead::{Content, Deleted, Missing}` (`:1105-1129`).
- Server-side git: `backend/g5-storage/src/lib.rs:1-9, 75, 115` `RepoStorage::local_path()` = "bare repo root"; `LocalRepoStorage` (`local.rs:1-4`) and `FilestoreRepoStorage` (`filestore.rs:277`, GCS Filestore NFS). Git Smart HTTP served at `/git/:repoId` (`routes/mod.rs:450`). Historical intent `ARCHITECTURE_DISCUSSION.md:3553` "§13.5 Bare Mirror per Repo" matches.
- Docs: `GET /api/v1/workspaces/:id/docs/by-path` → `get_doc_by_path` (`doc_service.rs:8029-8095`) → `read_effective_doc_content` (`:6653-6673`) → DTO `source: "overlay"|"committed"`, `draft_dirty`, `is_virtual: true`. Ontology: `GET …/ontology/:nodeId/content` → `repo_view.read_files` (`routes/ontology.rs:589+`). Intent log and doc references read the same overlay→committed views (`intent_log_service.rs:7-11`; `doc_reference_service.rs:1-6`).
- Worker RPC reads: none. `ws/worker_hub.rs:1-13` routes inbox items only; grep for `read_file|FileContent|ListTree` in it: 0 hits.

INFERENCE
- With no worker running, everything the UI reads still works: committed bytes from the bare repo at the DB SHA, dirty bytes from the Y.Doc cold-loaded from Postgres (`get_or_load`, `mod.rs:1403-1425`). The VM's working tree is never consulted by g5-api — the CRDT exists precisely to make uncommitted VM/browser edits visible server-side without any VM RPC.

---

## E. Documents model

OBSERVED — `docs` table
- `001:231-246` `docs(id, workspace_id, name, doc_type CHECK IN (spec, design, api, guide, adr, note), file_path, last_uplifted_at, last_downlifted_at, content_updated_at, created_at, updated_at)`; `022` adds `'view'`; `025` `content TEXT` ("fallback when git-backed content… is not available"); `029` persona-view types `pm_view, arch_view, security_view, performance_view, test_view, ui_view, customized_view` + columns `persona, instruction, scope_mode, scope_node_id, is_future, review_iterations, custom_persona_label, source_files JSONB` (+ `jobs.mode`, `jobs.target_doc_id`); `034` `linked_doc_paths JSONB`; `036` `mapping_content` ("Dead code… unused"); `064` `workspace_branch_id`, `workspace_branch_repo_id` ("Docs rows are metadata projections for markdown files in the workspace docs repo"); `176` drops the `doc_type` CHECK ("Committed docs are historical Git data"); `296/297` `document_source IN ('user','auxiliary')` (NULL = legacy/direct-git).
- Model `g5-api-data/src/models/doc.rs:6-13`: "A markdown document stored in the `.g5/docs/` directory of a workspace's docs repository. Document reference semantics come from canonical frontmatter at an exact repository cut." `:34-37` `scope_mode/scope_node_id` always NULL, scheduled for drop. `:50-53` `DocNodeLink` deprecated (#1415).
- Surfaces: `doc_service.rs:6177` `DOCS_SURFACE_ROOTS = ["docs","views","designs","explainers"]`; `:8038` accepted extensions `.md .html .htm .jsonl`; `:4852` `VALID_DIRECT_DOC_TYPES`; explainers (`:3671`, `:9377 create_explainer`). In-VM writable roots (`g5d-foundation/src/docs_write_policy.rs:46-55`): `docs/<workspace_id>`, `views`, `explainers`, `intent_record`, `intent_grounding`; everything else in the docs checkout is chmod'ed read-only for non-Merge jobs (`:24-26, 109-142`); `.git`/`.g5` stay writable, the latter for the CRDT socket (`:57-64`).
- Projection refresh: `sync_committed_docs_for_branch` (`:8404-8423`), `capture_committed_docs_for_operation_member` (`:8425+`, advance consumer), `POST …/docs/sync-committed` (`routes/docs.rs:690`). `draft.update` events invalidate `docs`/`views`/intent-record queries in the browser (`ws.ts:1099-1130`).
- Ontology: `models/ontology.rs:5-12` "Content is stored in git, not in Postgres — only the tree structure, metadata, and staleness tracking timestamps are persisted"; `001:177-222` `ontology_nodes(node_type node|document|code, file_path, content_hash, sort_order, sync_status, ahead_of_base, behind_base, …)`, `node_dependencies(dep_type direct|inherited)`; `020/021` `component_id`; `027` `src_files[]`; `060` grounding. Specs at `.semlift/specs/<component_id>.semlift.md` (legacy `.g5/specs/<id>.md`; `ontology_inventory.rs:14-22`; `ontology_service.rs:543, 752`); `POST …/ontology/sync-specs` registers committed specs (`routes/ontology.rs:442`); `maintenance/ontology_reconciler.rs:1-14` re-syncs from `search_reindex_queue` inside the indexer transaction.

INFERENCE — docs and ontology are **git files with Postgres projections** (caches for search, links, staleness), not Postgres content. Editing a doc in the UI = editing the file's `Y.Text` in the docs-branch draft; nothing reaches git until draft commit / commit-all (or the unborn-branch initial commit).

---

## F. Frontend

OBSERVED
- Deps (`frontend/package.json:23, 32-35`): `@monaco-editor/react ^4.7.0`, `y-monaco ^0.1.6`, `y-websocket ^2.1.0`, `yjs ^13.6.0`. No ProseMirror/TipTap/CodeMirror despite the comment "y-prosemirror for rich" (`crdt.ts:14`); rich editing is custom `contentEditable` (`EditableInline.tsx`, 4 uses) plus structured editors writing to `Y.Text`.
- `lib/crdt.ts` (1,486 lines): singleton doc store keyed `(workspaceId, branchId, gitSha)` (`:8-9, 192-197`); `useDraftCRDT` → `{ yDoc, provider, syncState synced|syncing|desynced, lastSyncAt, participants, resetDocument }` (`:176-189`); awareness user state (`:986`); token refresh (`:385`); `resetGlobalCrdtStore` (`:667`); `useCrdtDirtyCount`, `useCrdtFileDirty` (`:93-158`).
- `hooks/useDraft.tsx`: `useDraftSession` GET `/api/v1/branches/:id/drafts` (`:136-146`), `useWorkspaceDrafts`, `useWorkspaceDraftDiff`, `useDraftCRDT` re-export (`:211`), `usePreCommitCheck`, `useCommitDraft` (`:250-262`), `useDiscardDraft` (`:300-307`), `useCommitAll` (`:315-325`).
- `hooks/useFiles.ts` (`useFileTree` poll 2.5 s; `useFileContent` with `draftDirty`; `useWriteFileEntry` PUT) and `hooks/useDocs.ts` (`useDocList` `:79-88`, `useDocContent` by-path `:112-127`, create/delete).
- Components holding the Y.Doc: `editor/CodeEditor.tsx` (`:32-38`), `editor/SpecEditor.tsx` (`:59-60, 6817-6901, 7260-7279`), `editor/EditableInline.tsx`, `editor/StructuredQuestionEditor.tsx`, `editor/questionAnswerWrite.ts`, `editor/SemanticRecordFrame.tsx`, `editor/JobOutputView.tsx`, `shared/htmlDocAnswerBridge.tsx`, `explainer/ExplainerOutputView.tsx`, `draft/{DraftStatusBar,DraftWorkspaceControls,CommitModal}.tsx` (`DraftWorkspaceControls.tsx:149-156` merges server `dirtyCount` with `useCrdtDirtyCount`; `CommitModal.tsx:382-383`), `pages/WorkspacePage.tsx` (`:4076` status bar; `:4631-4634` `isDraftFileDirty` from server `dirtyFiles`; `:4913-4920, 4977-4983, 5080-5109` passes `crdt.yDoc/provider` into editors), `lib/ws.ts` (`:131-168`, `:1099-1209`, `:850-873`).
- A CRDT-free variant already exists: `editor/JobQuestionDocView.tsx:14, 446` renders the question editor with `yjsDoc={null}`.
- Tests: `vitest.config.crdt.ts` runs `e2e/crdt-smoke.test.ts` + `e2e/crdt-ff-conflict.test.ts` (617 lines) against a real API; unit tests `test/crdt.test.tsx`, `test/crdt-commit.test.tsx`, `test/crdt-lifecycle-reset.test.tsx`, `test/editor-crdt-workflow.test.tsx`, `pages/WorkspacePageFileCrdt.test.tsx`; mock `test/__mocks__/y-websocket.ts`.

INFERENCE — what an explicit-Save flow needs
1. Editors keep local state (Monaco model / contentEditable) instead of a shared `Y.Text`; "dirty" becomes a per-tab local flag instead of `_meta.dirty_files`.
2. Save = `PUT /api/v1/branches/:id/files/*path` (route exists) with its backend re-pointed from `DocumentHandle::set_file_content` to (a) an RPC that writes the file in the running VM, or (b) a server-side git commit when the VM is asleep (goal 7). Reads keep the same route; `draft_dirty` becomes `source: live_fs|committed`.
3. Optimistic concurrency per file (base blob hash or `head_commit`+content hash) — today's CAS is at commit granularity only.
4. Presence/cursors (`draft_participants`, awareness) disappear unless a document-only CRDT is reintroduced.
5. `ws.ts` lifecycle reset and 2.5 s tree polling get replaced by VM "file changed" notifications or refetch-on-focus.

---

## G. Size

OBSERVED (`wc -l`)

Backend code that exists only for CRDT/drafts/connected-commit:
| Area | Lines |
|---|---|
| `backend/g5-crdt/src/*` (doc 1041, head_advance 172, lib 383, materializer 898, task_commit 1108) | 3,602 |
| `backend/g5-api-core/src/crdt/*` (authority 4842, awareness 599, mod 2768, overlay 112, persistence 2952, query_observer 36, task_commit 1753) | 13,062 |
| `backend/g5d-foundation/src/crdt/*` (mod 11483, sync 4573, materialize 1839, task_commit_bridge 975, attribution 483, conflict_notifier 363, conflict_detector 329) | 20,045 |
| `services/draft_service.rs` 4,718; `routes/drafts.rs` 659; `g5-workspace-ops/src/draft.rs` 87; `models/draft.rs` 65 | 5,529 |
| `g5d-cli/src/commit.rs` 2,183; `connect.rs` 1,537; `conflicts.rs` 207; `g5d-worker/src/fs_diff.rs` 430 | 4,357 |
| **Total dedicated** | **≈46,600** |

Heavily coupled (CRDT assumptions threaded through, not deletable wholesale): `workspace_finalizer.rs` 7,569; `sticky_runtime.rs` 8,073; `advance_consumer.rs` 2,379; `workspace_ops.rs` 2,349; `workspace_reader.rs` 3,335; `task_service.rs` 33,023; `doc_service.rs` 10,976; `ontology_service.rs` 3,446; `routes/internal.rs` 15,051; `g5d-tooling/tools/file_ops.rs` 2,399, `git_ops.rs` 2,706, `terminal.rs` 814 (+ `spec_ops`, `grounding_ops`); `g5d-worker/lifecycle.rs` 7,965; `g5d-host/supervisor.rs` 29,684. Files mentioning `crdt|Crdt|draft_session|yjs` per crate (over-inclusive): g5-api-core 52 files / 2,014 mentions; g5d-foundation 13 / 911; g5d-tooling 11 / 890; g5d-host 4 / 358; g5d-cli 16 / 225; g5d 6 / 217; g5d-worker 8 / 202; g5-api-routes 14 / 149; g5-contracts 6 / 77; g5-workspace-ops 4 / 43; g5-internal-api 2 / 42; g5-scm 2 / 9.

Frontend: dedicated `lib/crdt.ts` 1,486 + `hooks/useDraft.tsx` 357 + `components/draft/*` 1,847 + CRDT tests/mocks/e2e ≈ **12,133** (incl. tests). Non-test files referencing yjs/CRDT: 24 files, 63,773 lines (dominated by `WorkspacePage.tsx` 13,357 and `SpecEditor.tsx` 11,565).

Tables/columns that exist only for CRDT / drafts / authority:
- CRDT/drafts: `draft_sessions`, `draft_updates`, `draft_participants`; `task_commit_requests` (161), `task_commit_frontier_cuts` (279), `task_commit_frontier_receipts` (280); `workspace_head_operations.crdt_scope_kind/crdt_task_id` and `workspace_head_operation_repos.draft_session_id` (196); `tasks` unique `(org_id,id)` (151/152, FK support only).
- Authority (git boundary; separable from CRDT): `workspaces.authority_advance_seq`, `workspace_head_operations`, `workspace_head_operation_repos`, `workspace_advance_consumer_receipts` (198/209), `workspace_advance_consumer_inputs` and `workspace_head_operation_source_repos` (209), `workspace_head_operation_membership_proposals` (210), `workspace_head_operation_make_remote_applications` (214), eight `pr1884_authority_cutover_*` evidence tables (215), `search_reindex_queue.advance_id` and `branch_integration_queue.advance_id` (198), `workspace_branches.{authority_scope_kind, job_id, task_id, authority_scope_id}` (197).
- Docs projection leftovers: `docs.content` (025 fallback), `docs.mapping_content` (036 dead), `docs.scope_mode/scope_node_id` (TODO drop), `doc_node_links` (deprecated).

---

## Coupling — what depends on CRDT/drafts today

OBSERVED unless marked
1. **Every UI read of uncommitted content** (`files`, `docs/by-path`, ontology content, intent log, doc references, grounding) goes through `RepoReadView` with an optional `DocumentHandle` overlay (`workspace_reader.rs:676-818`; `doc_service.rs:6653`; `intent_log_service.rs:7-11`).
2. **Every UI write** (`PUT files/*path`, `PUT ontology/:id/content`, `PUT docs/:id`, doc create/delete/folders) targets `DocumentHandle::set_file_content`/`delete_file` and needs an overlay (`files.rs:592-596` → 409 without one).
3. **Agent tools** hold `Arc<CrdtPeer>` and enqueue/flush/validate through it; `edit_file` round-trips through the server (`internal.rs:3259`); `git` tools flush and merge CRDT dirty state; `git/safety.rs` forbids branch switches to protect the draft.
4. **Worker/host lifecycle**: CRDT connect-with-retry is part of task startup (`host_support.rs:22-34`); provider runs are wrapped in scan loops and barriers (`lifecycle.rs:2309-2325`); sticky chat runtime teardown drains CRDT state (`Teardown` mode, `sticky_runtime.rs`); workspace cleanup waits on CRDT shutdown confirmations (`mod.rs:4840-4853`).
5. **Commit path**: `commit_draft` and the connected worker commit read content *from the Y.Doc*, not from any filesystem (`draft_service.rs:2892-2917`); the finalizer rolls the draft snapshot in the same operation.
6. **Task authority semantics**: `ensure_task_snapshot_has_no_dirty_overlay` (`workspace_reader.rs:1806`) gates terminal reads on "no unpublished CRDT state"; `CrdtScope` is a column of `workspace_head_operations` (`196:17-19, 41-53`).
7. **Notifications**: `draft.*` events and the browser's lifecycle reset are keyed to draft commits/discards; `useFileTree` polls because of out-of-band CRDT writes.
8. **Docs write policy** carves out `.g5/` for the CRDT socket (`docs_write_policy.rs:57-64`).
9. **Presence** (`draft_participants`, awareness) and **human-authority attribution** (`attribution.rs`) exist only inside the CRDT layer.
10. **Frontend**: `WorkspacePage` threads `yDoc/provider` into `CodeEditor`, `SpecEditor`, structured editors, explainer views; dirty state and commit controls read `_meta.dirty_files`.
11. INFERENCE: `g5d connect` (laptop mirror of drafts) and conflict sidecars have no purpose without the overlay.

---

## Simplification candidates (removing the CRDT layer)

Deleted outright (OBSERVED components; INFERENCE that nothing else needs them once the overlay is gone):
- `g5-crdt` crate; `g5-api-core/src/crdt/*`; `g5d-foundation/src/crdt/*`; `draft_service.rs`; `routes/drafts.rs`; `g5-workspace-ops/src/draft.rs` (`DraftState` gate → always clean); `models/draft.rs`; connected paths in `g5d-cli/{commit,connect,conflicts}.rs`; `fs_diff.rs`; `/api/v1/yjs` route + `crdt_ws_adapter`; `/api/internal/tasks/:id/crdt/edit-file-replace`; `draft.*` WsEvents; `CrdtConfig` env vars; `docs_write_policy` socket carve-out; `git/safety.rs` branch-switch block.
- Tables: `draft_sessions`, `draft_updates`, `draft_participants`, `task_commit_requests`, `task_commit_frontier_cuts`, `task_commit_frontier_receipts`; columns `workspace_head_operations.crdt_scope_kind/crdt_task_id`, `workspace_head_operation_repos.draft_session_id`.
- Frontend: `lib/crdt.ts`, `hooks/useDraft.tsx` (commit/discard hooks become git-commit hooks), `components/draft/*` (rewritten), deps `yjs`/`y-websocket`/`y-monaco`, CRDT test suites and mock.

Must be replaced (INFERENCE, with risks):
1. **Visibility of uncommitted VM edits to the UI.** Today the CRDT is the only channel. Replacement: a worker RPC (`read_file`, `list_tree`, `write_file`, `git status/diff`) over the existing worker WebSocket/vsock, used when the VM runs; bare-repo reads otherwise. Risk: read latency/availability now depend on the VM; "dirty" indicators come from `git status` instead of `_meta.dirty_files`.
2. **Durability of uncommitted work across VM sleep/teardown.** Today `draft_sessions` outlives the VM. Replacement: the VM disk (1 VM per workspace) and/or an auto-checkpoint commit to an internal ref on teardown (the sticky-runtime teardown already has a flush-before-stop shape). Risk: data loss if the disk is not durable; needs an explicit policy.
3. **Commit creation.** `prepare_tree_commit_from_exact_base` reads the Y.Doc. Replacement: native `git commit` in the VM + push to internal git (`/git/:repoId` exists) — the `g5d_push` operation mode already models this (`196:21, 50-53`; `push.rs:1285`). Commands 1-3, frontier receipts and rebase pause files disappear. Risk: today's multi-repo atomicity comes from the single DB cut; with native pushes per repo, decide whether `workspace_head_operations` still gates acceptance (governance-at-git-boundary suggests keeping it as the push acceptance gate).
4. **Browser editing.** Explicit Save via `PUT files/*path` → VM RPC or server git write; per-file optimistic concurrency token; `JobQuestionDocView` shows the non-CRDT editor shape.
5. **Human/agent conflict semantics.** `attribution.rs` and conflict sidecars vanish; replacement is file mtimes/`git diff` plus a "changed on disk" prompt on Save.
6. **Docs/ontology projections** stay (they already project from committed git via advance consumers); only `write_file_to_docs_draft` and ontology `set_file_content` change target.
7. **Task-scoped isolation** (`task_id` overlays, `ensure_task_snapshot_has_no_dirty_overlay`) maps to native task branches/worktrees inside the VM; the DB no longer tracks "dirty overlay" per task.

Keep (separable from CRDT): the authority operation registry and consumer-replay tables (196/198/206/208/209/210/214), `RepoStorage` bare repos + Smart HTTP, `workspace_reader` committed reads, docs/ontology projections and reconcilers.

---

## Open questions for the user

1. Which UI editing capabilities must survive: Monaco code editing with Save; spec/doc rich editing (`EditableInline`, structured question editors, explainer answer bridges) — all currently write to `Y.Text`. Is multi-user presence/cursors required in v1 or deferred to the future document-only CRDT?
2. When the VM is asleep, is the UI read-only (internal git), or may a Save land as a server-side commit on the docs/code branch (the unborn-docs-branch path already does a direct server commit)?
3. Should `workspace_head_operations` remain the mandatory gate for every branch advance, with native `git push` from the VM producing a `g5d_push`-style operation? Or should internal git refs become the authority with the DB merely mirroring them (reversing migration 206's direction)?
4. Multi-repo workspaces: is per-repo push acceptable, or must a cut across repos stay atomic?
5. Task isolation: Chat currently writes to the branch draft while other jobs write per-task overlays. With native git, is it one branch/worktree per task in the same VM, or one working tree with sequential tasks?
6. Durability of uncommitted work across VM teardown/sleep: rely on the VM disk, or auto-checkpoint to an internal ref on teardown?
7. `g5d connect` (laptop sync of drafts) and third-party-agent conflict sidecars: drop entirely, or replace with plain git push/pull?
8. Should the Postgres docs projection keep `docs.content` / `mapping_content` / `scope_mode` at all, given reads are already virtual from git?
