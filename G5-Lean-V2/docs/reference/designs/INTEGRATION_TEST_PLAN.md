# Integration Test Plan — Workspace Commands, CRDT, & Worker Registration

**Branch:** `fix/require-worker-org-id`  
**Scope:** Fill coverage gaps identified in the auth-token removal audit  
**Date:** 2025-01-20

---

## 1. Coverage Gap Summary

### 1.1 Rust — CLI Commands (g5d/tests/cli_commands.rs)

| Command | Existing Tests | Gap |
|---------|---------------|-----|
| `init` | ✅ init, init-with-force | — |
| `add` | ✅ add-repo, add-repo-with-options | — |
| `refresh` | ✅ refresh | — |
| `commit` | ✅ commit, commit-requires-message | — |
| `push` | ✅ push | — |
| `revert` | ✅ revert | — |
| `clone` | ❌ ZERO | Requires mock HTTP server for g5-api |
| `fork` | ❌ ZERO | Requires workspace + mock HTTP server |
| `merge` | ❌ ZERO | Two workspaces with shared repo |
| `conflicts` | ❌ ZERO | Workspace with `.g5/conflicts/` sidecars |
| `doctor` | ✅ (in e2e) | — |

### 1.2 Rust — CRDT Sync (g5d/tests/crdt_sync.rs)

| Scenario | Existing Tests | Gap |
|----------|---------------|-----|
| Outbound sync | ✅ worker_syncs_file_changes_to_server | — |
| Inbound sync | ✅ worker_receives_server_updates | — |
| Bidirectional sync | ✅ bidirectional_sync | — |
| Reconnection | ✅ sync_reconnect | — |
| State vector exchange | ✅ initial_state_vector_exchange | — |
| Conflict resolution | ❌ | Concurrent conflicting edits to same Y.Text |
| Materialization failure | ❌ | Bad data in CRDT doc → sidecar created |
| Multiple files | ❌ | Two files synced concurrently in one doc |

### 1.3 Rust — Worker Lifecycle (g5d/tests/worker_lifecycle.rs)

| Scenario | Existing Tests | Gap |
|----------|---------------|-----|
| Registration | ✅ worker_registers_on_startup | — |
| Poll | ✅ poll_receives_task | — |
| Heartbeat | ✅ heartbeat_periodic | — |
| Task reporting | ✅ reports_transition, reports_failure | — |
| Shutdown | ✅ graceful_shutdown | — |
| Idle cadence | ✅ poll_idle_cadence | — |
| Reconnect | ✅ reconnect_on_api_failure | — |
| registration_token flow | ❌ | `registration_token: None` in all tests |
| org_id in registration | ❌ | `org_id` set → verify sent in registration body |

### 1.4 Rust — E2E (e2e/src/scenarios/)

| Scenario | Existing Tests | Gap |
|----------|---------------|-----|
| init, add, commit, doctor, help, version | ✅ cli_ops.rs | — |
| CRDT handshake, two-peer sync, convergence | ✅ crdt_collab.rs | — |
| clone | ❌ | Needs live API workspace fixture |
| fork | ❌ | Needs live API workspace to fork |
| merge | ❌ | Two workspaces via API |
| push | ❌ | Commit + push to API |

### 1.5 Frontend — Integration Tests

| Area | Existing Tests | Gap |
|------|---------------|-----|
| WebSocket events | ✅ websocket.test.tsx (16 event types) | — |
| CRDT useDraftCRDT | ✅ crdt.test.tsx | — |
| useWorkspace hooks | ✅ useWorkspace.test.ts | — |
| useDrafts hooks | ✅ useDrafts.test.ts | — |
| CreateWorkspaceDialog | ✅ CreateWorkspaceDialog.test.tsx | — |
| CRDT conflict state in UI | ❌ | CrdtDocumentStore sync error → UI reaction |
| WebSocket reconnect behavior | ❌ | Disconnect → reconnect → cache revalidation |
| Workspace fork/clone flows | ❌ | UI triggers fork → API call → navigation |
| Draft dirty tracking | ❌ | File edit → dirty state → commit flow |

---

## 2. Test Implementation Plan

### Phase 1: Rust CLI Integration Tests (g5d/tests/cli_commands.rs)

#### 2.1.1 `test_cli_clone`
- **Setup:** Start a lightweight mock HTTP server (using `helpers::MockApiServer` or `axum` inline) that serves `GET /api/v1/workspaces/:id` → workspace manifest YAML.
- **Execute:** `g5d clone <workspace-id> <path> --token test-token`
- **Assert:**
  - Exit code 0
  - `.g5/workspace.yaml` created with content from manifest
  - `.g5/workspace-meta/` is a git repo
  - Repos listed in manifest are cloned
- **Error variant:** 401 from API → exit code non-zero + stderr contains "Unauthorized"
- **Error variant:** 404 from API → exit code non-zero + stderr contains "not found"
- **Error variant:** No `--token` and no `G5D_TOKEN` → exit code non-zero

#### 2.1.2 `test_cli_fork`
- **Setup:** Create a workspace with `g5d init` + `g5d add`. Write a `workspace.yaml` with `remote.enabled: true` and `remote.api_url`. Start mock HTTP server for `POST /api/v1/workspaces/:id/fork`.
- **Execute:** `g5d fork <path> --name test-fork` from within the workspace
- **Assert:**
  - Exit code 0
  - New workspace directory created with `.g5/workspace.yaml`
  - `fork_lineage.yaml` exists in workspace-meta
  - Repos cloned from source workspace
- **Error variant:** Workspace without `remote.enabled` → error message about local-only
- **Error variant:** No API URL configured → error message

#### 2.1.3 `test_cli_merge`
- **Setup:** Create two workspaces (source + target), each with the same repo added from a shared bare remote. Make a commit in the source workspace's repo.
- **Execute:** `g5d merge <source-path>` from within the target workspace
- **Assert:**
  - Exit code 0
  - Target repo now contains the source's commit (fast-forward)
  - JSON output with `fast_forwarded: 1`
- **Dry-run variant:** `g5d merge <source> --dry-run` → shows what would merge, no changes on disk
- **Conflict variant:** Both workspaces modify same file → `conflicts: 1` in output

#### 2.1.4 `test_cli_conflicts`
- **Setup:** Create workspace, add `.g5/conflicts/src/main.rs` with agent content, and write `src/main.rs` with CRDT content.
- **Execute:** `g5d conflicts` from within workspace
- **Assert:**
  - Exit code 0
  - Output lists the conflict for `src/main.rs`
  - JSON output with conflict info
- **Clear variant:** `g5d conflicts --clear` → sidecars removed, count reported
- **Accept-local variant:** `g5d conflicts --accept-local src/main.rs` → workspace file overwritten with agent version, sidecar removed
- **Accept-remote variant:** `g5d conflicts --accept-remote src/main.rs` → sidecar removed, workspace file unchanged

### Phase 2: Rust CRDT Integration Tests (g5d/tests/crdt_sync.rs)

#### 2.2.1 `test_concurrent_conflict_resolution`
- **Setup:** MockYjsServer + CrdtPeer. Insert initial text "Hello" in server doc.
- **Action:** Simultaneously: server inserts " World" at position 5, peer inserts " Yjs" at position 5.
- **Assert:** Both docs converge to same text (Yjs guarantees, order may vary). No data loss — both insertions present.

#### 2.2.2 `test_materialization_creates_conflict_sidecar`
- **Setup:** CrdtPeer connected to MockYjsServer. Write a file via the peer's workspace.
- **Action:** Server pushes an update that changes the same file. Meanwhile, simulate an external tool modifying the local file (dirty write).
- **Assert:** CrdtPeer detects the third-party modification and creates a conflict sidecar in `.g5/conflicts/`.

#### 2.2.3 `test_multi_file_concurrent_sync`
- **Setup:** MockYjsServer with doc containing two file entries.
- **Action:** Peer writes file A, server pushes update to file B concurrently.
- **Assert:** Both files materialized correctly. No cross-contamination between file entries.

### Phase 3: Rust Worker Registration Tests (g5d/tests/worker_lifecycle.rs)

#### 2.3.1 `test_worker_registers_with_registration_token`
- **Setup:** MockApiServer expecting `POST /api/v1/workers/register` with `registration_token` field in body.
- **Config:** `WorkerConfig { registration_token: Some("g5w_test_abc123".into()), .. }`
- **Assert:**
  - Registration request body contains `"registrationToken": "g5w_test_abc123"`
  - Worker stores JWT returned from registration response
  - Subsequent requests use `Authorization: Bearer <jwt>`

#### 2.3.2 `test_worker_registers_with_org_id`
- **Setup:** MockApiServer expecting `POST /api/v1/workers/register` with `org_id` field.
- **Config:** `WorkerConfig { org_id: Some("org-uuid-123".into()), .. }`
- **Assert:**
  - Registration request body contains `"orgId": "org-uuid-123"`

#### 2.3.3 `test_worker_registration_rejected`
- **Setup:** MockApiServer returns 401 on registration.
- **Config:** `WorkerConfig { registration_token: Some("g5w_invalid".into()), .. }`
- **Assert:**
  - Worker does not enter poll loop
  - Error logged about rejected registration

### Phase 4: Frontend Integration Tests

#### 2.4.1 CRDT Sync Error UI (src/test/crdt.test.tsx — new tests)
- **Test:** `useDraftCRDT` reports error state when provider connection fails
- **Setup:** Mock provider that emits `connection-error` event
- **Assert:** Hook returns `syncState: 'disconnected'` and error message accessible

#### 2.4.2 WebSocket Reconnect (src/test/websocket.test.tsx — new tests)
- **Test:** After WebSocket disconnect, all subscribed channels' data is revalidated
- **Setup:** Render workspace page, wait for load, simulate ws disconnect + reconnect
- **Assert:** `queryClient.invalidateQueries` called for workspace-scoped query keys

#### 2.4.3 Draft Dirty Tracking (src/test/crdt.test.tsx — new tests)
- **Test:** `useDraftCRDT` tracks dirty state when local edits pending sync
- **Setup:** Mount hook, make local edit, check `isDirty: true`
- **Assert:** After sync confirmation, `isDirty: false`

#### 2.4.4 Workspace Fork Flow (new: src/pages/WorkspaceFork.test.tsx or extend WorkspacePageV2.test.tsx)
- **Test:** Fork button triggers API call and navigates to new workspace
- **Setup:** MSW handler for `POST /api/v1/workspaces/:id/fork`, renderApp with workspace page
- **Assert:** Fork API called with correct body, navigation occurs to new workspace URL

---

## 3. Implementation Notes

### Test Infrastructure Used

| Layer | Framework | Key Helpers |
|-------|-----------|-------------|
| g5d CLI tests | `assert_cmd` + `predicates` + `tempfile` | `g5d_cmd()`, `init_workspace()`, `create_bare_remote()`, `workspace_with_repo()` |
| g5d CRDT tests | `tokio` + `tokio-tungstenite` | `MockYjsServer`, `CrdtPeer`, `CrdtPeerConfig`, `RepoFixture` |
| g5d worker tests | `tokio` | `MockApiServer` (.when/.respond_with/.assert_called), `TestWorkerEnv`, `WorkerConfig` |
| e2e (live stack) | `reqwest` + `assert_cmd` | `TestApiClient::from_env_authed()`, `skip_unless!` |
| Frontend unit | `vitest` + `@testing-library/react` + `msw` | `renderApp()`, `MockWsController`, MSW `server.use()` |
| Frontend e2e | `@playwright/test` | (not yet utilized for these tests) |

### Mock HTTP Server for Clone/Fork CLI Tests

The `clone` and `fork` commands make real HTTP calls (reqwest). For integration tests, we need a lightweight HTTP server:
- Option A: Use the existing `MockApiServer` from `helpers::mock_api` (axum-based, catch-all).
- Option B: Spin up a minimal `axum` server inline in the test.
- **Chosen:** Option A — extend `MockApiServer` since it already handles route matching.

### Registration Token Testing

The `WorkerConfig.registration_token` field is passed in the registration POST body as `registrationToken` (camelCase from serde `rename_all`). Tests should verify the field appears in the serialized request body.

---

## 4. Priority Order

1. **Phase 1: CLI tests** (clone, fork, merge, conflicts) — largest coverage gap
2. **Phase 3: Worker registration** — validates the auth changes on this branch
3. **Phase 2: CRDT tests** — conflict resolution and materialization
4. **Phase 4: Frontend tests** — CRDT error states, reconnect, fork flow

---

## 5. Success Criteria

- All new tests compile with `SQLX_OFFLINE=true cargo check --tests --workspace`
- All new Rust tests pass with `cargo test --workspace` (excluding e2e that need live stack)
- All new frontend tests pass with `npm test` in `generation/client/`  
- No regressions in existing tests
