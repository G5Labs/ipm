# G5-Lean research: workspace/branch/repo model and Git persistence ("internal Git")

Repo: `/Users/zliu/Workspace/G5-Lean` (read-only). All paths below are relative to that root.
Legend: **OBS** = observed in code/SQL with citation. **INF** = inference from observed facts (not directly verified).

---

## 0. One-paragraph summary

G5 already has an "internal Git": g5-api itself is a smart-HTTP git server (`/git/:repoId/...`) whose object
store is a per-repo mirror under `RepoStorage` (`{base}/{repo_id}`, local disk in dev/self-hosted, Google Cloud
Filestore NFS in prod). Postgres is the authority for branch heads (`workspace_branch_repos.head_commit`); git
`refs/heads/<branch>` in the mirror are a *derived projection* re-written by the server after every accepted
head operation. Every write into the mirror (from the CLI, a worker task, or the UI's CRDT draft) goes through a
two-phase "workspace head operation": prepare a DB row → push objects to a create-only staging ref
`refs/g5/staged/<op>/<repo>` → finalize (DB CAS) → server promotes to `refs/g5/committed/...` and projects
`refs/heads/<branch>`. Pushing to the customer's GitHub/GitLab remote is a separate, queue-driven server-side
step (`branch_integration_queue` → `branch_integration_worker`) that runs in g5-api with the workspace creator's
OAuth token, only for repos in `tracking_mode='remote_tracking'`. Worker VMs hold disposable reflink checkouts
materialized from a warm bare cache cloned from the internal smart-HTTP URL; uncommitted work survives a VM loss
only if it was flushed to the CRDT draft (Postgres) or published as a checkpoint commit.

---

## A. Entities

### A.1 Tables and columns (OBS)

**`orgs`, `users`** — `backend/g5-api/src/db/migrations/001_initial_schema.sql:22-40`. `user_scm_connections` (001:52) holds
provider OAuth connections (used by `ScmTokenManager`).

**`repos`** (001:75-89): `id, org_id, name, slug, git_url, default_branch (default 'main'), provider, provider_repo_id,
provider_full_name, created_at`. `UNIQUE(org_id, slug)`. 055 moved `webhook_secret_enc` to `repo_secrets`, re-scoped
uniqueness to `(org_id, provider, provider_repo_id)` and enabled RLS (055:16-45). Rust: `Repo` in
`backend/g5-api-data/src/models/repo.rs`.

**`workspaces`** (001:92-112): `id, org_id, name, is_base, status IN ('idea','active','blocked','review','integrated'),
description, cloned_from_id, docs_repo_id, docs_branch_name, docs_tracking_mode IN ('tracking','pinned'),
visibility IN ('org','invite_only'), created_by, created_at, updated_at`. Added later:
- `push_mode IN ('manual','auto')` — 119:22-29 (governs outbound publication to the customer remote).
- `base_auto_pull_enabled`, `base_auto_pull_actor_user_id` — 131:5-14 (coordinated Base pulls of docs+code).
- `deleted_at`, `deleted_by` — 139 (soft delete).
- `mutation_policy IN ('read_write','read_only')` — 163; `mutation_policy_transition IN ('freezing')`,
  `mutation_policy_updated_by/at` — 164. 163 header says read-only is an *app content* policy, "NOT a Git-ref
  protection rule" (163:11-15).
- `stages IN (1,2)`, `merge_target_workspace_id` — 178:14-20.
- `authority_advance_seq BIGINT` — 196:5-7 (monotonic per-workspace advance counter).
Rust: `Workspace` in `backend/g5-api-data/src/models/workspace.rs` mirrors exactly these columns.

**`workspace_branches`** (001:115-126): `id, workspace_id, repo_id, branch_name, base_branch, head_commit,
tracking_mode IN ('tracking','pinned')`; originally `UNIQUE(workspace_id, repo_id)` + `UNIQUE(repo_id, branch_name)`.
- 016 relaxed to `UNIQUE(workspace_id, repo_id, branch_name)` + `UNIQUE(repo_id, branch_name)` so `job/<id>` branches
  get their own row (016:3-8).
- 058: `cloned_from_branch_id` (workspace fork lineage, backfilled from `-clone-YYYYMMDD-...` suffix; 058:1-3, 18-24).
- 061: `materialization_status IN ('materialized','unborn','missing_ref','error')` (replaces all-zero SHA sentinel).
- 197: `authority_scope_kind IN ('workspace','job','task')`, `job_id`, `task_id`, generated `authority_scope_id`
  (197 DDL lines 1-40 after comment strip); unique index `(authority_scope_kind, authority_scope_id, repo_id)`.
Rust: `WorkspaceBranch` in `models/workspace.rs` (fields incl. `org_id`, `authority_scope_*`).

**`workspace_branch_repos`** (062:8-24): "repo-local git state for each backing repo" — `id, org_id, workspace_branch_id,
repo_id, repo_branch_name, base_branch, head_commit, tracking_mode, materialization_status, root_path, created_at,
updated_at, UNIQUE(workspace_branch_id, repo_id)`. Added:
- 114:27-33: `remote_branch, baseline_commit, sync_paused_reason, last_sync_at, remote_tip_advisory_sha,
  remote_tip_advisory_observed_at`; `tracking_mode` gains `'remote_tracking'` (114:40-52; forcibly rebuilt in 117).
- 173: `auto_pull_enabled` (per child, opt-in passive inbound pull).
- 189: `diff_base_commit` (base for the UI Diff Graph).
- 200: `base_head_commit` + trigger `maintain_workspace_branch_repo_base_version`.
Rust: `WorkspaceBranchRepo` in `models/workspace.rs`.

**Trigger churn between the two tables (OBS):** 062 installed `trg_wb_sync_repo_entry` mirroring parent→child;
116 and 118 patched it to stop clobbering `tracking_mode`; 206 stopped mirroring `head_commit`/`materialization_status`
entirely and repaired parent from child (206 header + tail). 206's comment: "`workspace_branch_repos` is the exact
repo-local authority member selected by current readers. `workspace_branches` remains its normalized owning projection."

**`repo_manifests`** (013:11-47): per-(worker, repo) checkout tracker (`local_head, upstream_head, storage_path`).
OBS oddity: its FK `fk_repo_manifests_workspace` makes `repo_id` reference `workspaces(id)` (013:43-46).

### A.2 Is a workspace 1-1 with a git branch? (OBS + INF)

- OBS: `create_workspace` mints ONE canonical branch name for the workspace and applies it to every member repo,
  including the org `docs` repo: `mint_canonical_branch_name` → `feat/<name-slug>-<8 hex of workspace id>`
  (`backend/g5-api-core/src/services/workspace_service.rs:15057-15062, 15076-15103`); comment at 15105-15116 calls this
  "the one uniform name per workspace (the CRDT branch-access gate invariant)".
- OBS: besides the canonical row (`authority_scope_kind='workspace'`), each repo also gets `job/<job_id>` rows
  (`authority_scope_kind='job'`) and `task/<task_id>` rows (`'task'`) in the same table (197 backfill; 014 `jobs.branch_name`
  "e.g. job/<job-id>"; 197 excludes names `job/%`, `task/%`, `remote-tracking-base-%`, `archived/%` from the canonical set).
- INF: so a workspace is 1-1 with a *branch name* replicated across N repos, plus a tree of job/task branches per repo
  owned by the same workspace. It is not 1 workspace = 1 git branch in one repo.

### A.3 Multi-repo (OBS)

- Yes. `workspace_branch_repos.root_path` is the mount root of the repo inside the workspace overlay (172 header;
  default derived from slug via `g5_default_repo_mount_root`, BEFORE INSERT trigger 172:60-95).
- `TaskAssignment.repo_paths: Vec<RepoInfo>` with `root_path/root_paths`, `repo_role` (code|docs), `git_url`
  ("Smart HTTP URL for worker clone/fetch"), `branch_name`, `base_commit`, `workspace_branch_id`,
  `workspace_branch_repo_id` (`backend/g5-contracts/src/types/task_context.rs:3469-3497`).
- Docs repo: `workspaces.docs_repo_id` points at the org's `slug='docs'` repo (workspace_service.rs create path ~3447);
  `g5d clone` "initializes the independent docs checkout" (`backend/g5d-cli/src/clone.rs:1-5`).

### A.4 `stages` and `merge_target` (OBS)

178:1-13: `stages` = governance model (1 = approvals only before shipping; 2 = plan/design sign-off gate first).
`merge_target_workspace_id` = workspace this one is intended to merge into; NULL means "merge to fork parent
(`cloned_from_id`)". Both are UI-v3 fork-dialog fields; the code paths reading them are in job_merge/PR services
(not traced further).

---

## B. Where git data lives today

### B.1 The internal repo store (OBS)

- `g5_storage::RepoStorage` trait: `local_path(repo_id) -> {base}/{repo_id}` ("bare repo root"), `worktree_root()`,
  `init_repo`, `remove_repo`, plus file read/list helpers that skip `.git` (`backend/g5-storage/src/lib.rs:9-16, 72-140`).
- `StorageConfig::from_env` (`backend/g5-storage/src/config.rs:96-160`): `G5_STORAGE_PROVIDER=local|filestore`,
  `G5_STORAGE_BASE_DIR` (default `/data/repos`), `G5_STORAGE_WORKTREE_DIR` (default `{base}/.worktrees`),
  `G5_STORAGE_MOUNT_POINT` (default `/mnt/filestore`), `G5_STORAGE_TENANT_PREFIX` (default `repos`).
- Filestore impl: "Bare repos live on NFS mount (shared across all nodes); worktrees on local SSD; lockfile
  `O_CREAT|O_EXCL` mutual exclusion" (`backend/g5-storage/src/filestore.rs:1-33`).
- Infra: self-hosted compose sets `G5_STORAGE_PROVIDER: local`, `/data/repos`, `/data/worktrees`
  (`infra/self-hosted/compose.yml:79-81,148-150`). Prod GKE: Cloud Build substitutions `_FILESTORE_INSTANCE`
  (default `g5-<env>-repos`), PV `g5-repos-pv`, PVC `g5-repos-pvc`, `1Ti` (`infra/ci/cloudbuild-deploy.yaml:34-38,1320-1335`);
  terraform `active_filestore_instance_name = "g5-dev-repos-ssd"`, `2560Gi` (`infra/terraform/terraform.tfvars:41-49`),
  module `infra/terraform/modules/filestore`. ci-docker parity boots g5-api with
  `Filestore { mount_point: "/mnt/g5-repos", tenant_prefix: "repos" }` (`Justfile:2519`). `ci-docker-nfs-init` is a
  macOS host-NFS bootstrap for that CI lane only (`Justfile:947-949`).
- No third-party git server: grep for gitea/soft-serve/git-daemon/http-backend finds nothing; no GCS bucket for repos
  (GCS appears only for sccache).

### B.2 Repo shape on disk (OBS + INF)

- OBS: external repos are cloned with `GitRepo::clone` = `git2::build::RepoBuilder::new().clone(url, path)` with no
  `.bare(true)` (`backend/g5-scm/src/repo.rs:485-505`); local-only ("G5-managed local project") repos use
  `git2::Repository::init_bare` (`workspace_service.rs:6846-6849`, `initialize_local_workspace_repo`).
- OBS: the smart-HTTP handlers tolerate both: `repo_git_dir` picks `{path}/.git` if present else `{path}`
  (`backend/g5-api-routes/src/routes/git.rs:769-776`); non-bare receive-pack sets
  `receive.denyCurrentBranch=updateInstead` (git.rs:42, 779-799). Mirror presence check is
  `.git exists || HEAD exists` (`workspace_service.rs:10136-10137`).
- INF: provider-backed mirrors are therefore non-bare clones *with a working tree* on NFS; local-only repos are bare.

### B.3 Smart-HTTP server inside g5-api (OBS)

- Routes: `GET /git/:repoId/info/refs?service=`, `POST /git/:repoId/git-upload-pack`, `POST /git/:repoId/git-receive-pack`
  (git.rs:1-12); mounted via `.nest("/git/:repoId", git::router())` (`backend/g5-api-routes/src/routes/mod.rs:450`);
  helm routes `/git/*` straight to g5-api for the 3600 s timeout (`infra/helm/g5/values.yaml:377-384`).
- Spawns `git upload-pack --stateless-rpc` / `git receive-pack --stateless-rpc` with `env_clear()`,
  `GIT_DIR`, `safe.directory=*` (git.rs:527-545, 579-585, 779-799); binary from `G5_GIT_BIN` or `which git`
  (`backend/g5-api-core/src/services/mod.rs:834-836`). Body limit 256 MiB (git.rs:41).
- Auth `GitAuth::from_headers`: Bearer JWT, or Basic with username `x-worker-token` / `x-user-token` / `x-access-token`
  and the JWT as password (git.rs:132-230). `require_read`: worker needs a running-or-live-claim task on the repo,
  user needs workspace access (git.rs:390-420); `require_push`: worker needs a *running* task, user needs
  `workspaces:write` (git.rs:233-290).
- **receive-pack only accepts create-only updates to `refs/g5/staged/<operation_id>/<repo_id>`** (git.rs:288-310,
  651-672); `X-G5-Force-Push` header → 403 (git.rs:672-676); a non-zero old SHA is CAS-checked → 409 (git.rs:678-695).
  After the subprocess succeeds it records `mark_staging_ref_uploaded` (git.rs:719-750). The handler "never advances an
  authority cut, canonical ref, lifecycle row, or notification" (git.rs:630-634).
- `repo_gc` maintenance repacks mirrors because libgit2 commits + receive-pack never pack and `gc.auto 0` is set
  (`backend/g5-api-core/src/maintenance/repo_gc.rs:1-23`; spawned from `maintenance/mod.rs:493-503`).

### B.4 Task checkout on the worker host (OBS)

- `g5d-foundation::workspace::repo_cache`: warm **bare** cache keyed by URL — first touch `git clone --bare`, later
  `git fetch --prune`; in-process + cross-process `.lock` dirs (`backend/g5d-foundation/src/workspace/repo_cache.rs:1-24`).
- `materialize.rs`: per task, `cp --reflink=always` the bare cache into `<workspaces_root>/<task_id>/.git`, set
  `core.bare false`, `core.worktree`, `git checkout -f <branch>`; one mount per `repo_paths` entry
  (`materialize.rs:1-27`). `materialize_for_assignment` requires `assignment.repo_paths` (materialize.rs:693-709) and
  deletes any existing dir for the task before re-materializing (711-730). `cleanup_for_task` removes the dir unless a
  "task-workspace recovery obligation" is reserved (materialize.rs:989-1007).
- Supervisor contract `WorkspaceMaterializer` (`backend/g5d-host/src/supervisor.rs:2436-2470`), production impl wraps
  the foundation materializer (2544-2570); `cleanup` is called at 6 teardown sites (supervisor.rs:5790-5972).
- The URL the worker clones is the internal smart-HTTP URL: `RepoInfo.git_url = "{external_url}/git/{repo_id}"`
  (`backend/g5-api-core/src/services/task_service.rs:6608-6612`; manifest: `workspace_service.rs:4390`).
- Note: `clone_service.rs` is *workspace fork* (deep DB copy + git branch creation), not repo cloning
  (`backend/g5-api-core/src/services/clone_service.rs:1-13`); "clone lineage" (058) is that fork lineage.

### B.5 Blob store (OBS)

`g5-blob` reads content-addressed blobs (`ab/cd/<oid>` fanout) from `RepoStorage` gated by Ed25519 capability tokens;
no DB access (`backend/g5-blob/src/lib.rs:1-14`, `capability.rs:31-40`; routes `POST /api/v1/blobs/capabilities`,
`GET /api/v1/blobs/read|tree`, `backend/g5-api-routes/src/routes/blobs.rs:11-14`). Who writes blobs into that layout
was not traced (INF: an indexing/artifact path; not the git object store).

---

## C. Publication / authority / head operations

### C.1 Tables (OBS)

- `workspace_head_operations` (196): `target_kind IN ('workspace','job','task')`, `target_id`, `crdt_scope_kind IN
  ('none','canonical','task')`, `mode IN ('connected_commit_all','g5d_push')`, `idempotency_key`, `payload_sha256`,
  `status IN ('preparing','prepared','finalizing','committed','failed')`, `lease_generation`, `lease_expires_at`,
  `terminal_result`, `workspace_advance_seq` (unique per workspace), `projection_completed_at`, `cleanup_completed_at`.
  Trigger validates target ownership. 204 adds `failure_code`, `superseded_by_advance_id`. 209 adds provenance
  (`source_workspace_id, source_kind, source_id, producer_kind (NOT NULL), producer_id, producer_attempt_id,
  correlation_id, source_operation_id`) + `workspace_head_operation_source_repos`.
- `workspace_head_operation_repos` (196): per repo `expected_old_head`, `proposed_new_head`,
  `staging_ref = 'refs/g5/staged/<op>/<repo>'`, `committed_ref = 'refs/g5/committed/<op>/<repo>'`,
  `staging_status IN ('unchanged','pending','staged','committed','stale','cleaned')`; 205 `transition_kind IN
  ('advance','initialize')`, 261 adds `'reseed','rebase'`; 278 drops the live FKs (rows are immutable provenance).
- `workspace_head_operation_membership_proposals` (210): a `g5d push` may add a *new* repo; staging/storage status
  columns, `repo_role IN ('code','docs')`.
- `workspace_head_operation_make_remote_applications` (214): "Make-Remote" binds a local repo to a provider URL and
  flips the child to `remote_tracking`; carries expected/proposed `git_url/provider/*`, `outcome IN ('bound','published',
  'merge_required')`.
- `workspace_advance_adapter_receipts` (198): `adapter_kind IN ('search_index','external_git')` per operation;
  `branch_integration_queue.advance_id`, `search_reindex_queue.advance_id`.
- `branch_integration_queue` (114:93-110): PK `workspace_branch_repo_id`, `trigger_kind IN ('commit','manual',
  'workspace_open','background_inbound','push')` (119), `attempts, last_error, locked_by, locked_until`.
- `branch_sync_runs` (114:57-88): audit rows, `kind IN ('integration','advisory')`, `trigger_kind` (+ 'push',
  'approved_rebase_publish', 'base_auto_pull' per 119/131), `status IN ('noop','imported','pushed','diverged',
  'dirty_skipped','advisory_updated','auth_required','push_failed','fetch_failed')`, tips/baseline/advisory SHAs,
  `acting_user_id NOT NULL`, `merge_run_id`.
- `branch_integration_remote_pair_leases` (203): generation-fenced lease per `(repo_id, remote_branch)`.
- `job_merge_runs` (074/075/115): `source_kind IN ('job','workspace','remote_tracking')`, `workspace_branch_repo_id`,
  heads JSON, `phase`, `blocking_reason`; 199 `completion_advance_id`.
- `workspace_pull_requests` / `_events` / `_provider_repos` (124/125/129): local merge + provider PR mirror
  (`backend/g5-api-data/src/models/workspace_pull_request.rs`).
- `task_commit_requests` (161) + `task_commit_frontier_cuts` (279, stores `yjs_state` up to 64 MiB) +
  `task_commit_frontier_receipts` (280): durable receipts for connected task commits.

### C.2 Authority model (OBS)

- Postgres is the head authority; git refs are projections. `GitRepo::reconcile_branch_ref_to_authoritative` doc:
  "A missing or differing physical ref is a repairable derived projection, not an alternate source of authority"
  (`backend/g5-scm/src/repo.rs:1403-1409`). `project_committed_operation`: "Canonical refs are disposable projections of
  the complete current database-selected cut" (`workspace_finalizer.rs:3398-3400`).
- `authority_branch_scope_binding` (197) types each `workspace_branches` row as workspace/job/task member;
  readers resolve `(workspace_id, authority_scope_kind, authority_scope_id)` (197 header).
- Finalizers are DB-only: `apply_nonconnected_g5d_push_head` "deliberately DB-only: Git object/ref verification must have
  completed before the transaction opens, and canonical ref projection happens after it commits"
  (`backend/g5-api-core/src/services/workspace_ops.rs:1692-1699`).

### C.3 Flow: agent changed files → workspace head advanced → pushed to remote (OBS)

**Path 1 — `g5d push` (CLI, non-connected, `mode='g5d_push'`)** — `backend/g5d-cli/src/push.rs`:
1. Build candidates `{repo_id, expected_old_head, proposed_new_head}` from local checkouts; `POST
   /api/v1/workspaces/{id}/head-operations` with `AuthorityTarget::Workspace`, `CrdtScope::None`, fresh
   `idempotency_key`, `producer_kind: "g5d_cli"` (push.rs:1178-1212).
2. For each changed repo: `configure_origin(repo, "{api}/git/{repo_id}")` then
   `git -C <repo> push origin <sha>:refs/g5/staged/<op>/<repo>` with JWT auth (push.rs:1108-1130, 1140-1149).
3. `POST .../head-operations/{op}/finalize` (push.rs:1285-1294).
Server: `create_g5d_push_operation` (`workspace_finalizer.rs:570`), `mark_staging_ref_uploaded` (1322),
`verify_workspace_operation_refs` (1401), `finalize_g5d_push_operation` (4988) → CAS on `workspace_branch_repos`
(workspace_ops.rs:1699) → `AdvanceNotifier::notify` (`advance_notifier.rs:192-270`):
  a. `project_internal_git` → `project_committed_operation` (finalizer 3337-3402): `promote_operation_ref`
     staged→committed (`repo.rs:1584-1601`), then `project_branch_ref` CAS on `refs/heads/<branch>` (`repo.rs:1637-1695`).
  b. `write_baseline_ref refs/g5-tracking/<branch>/baseline` for remote-tracking members (`repo.rs:1697-1712`;
     advance_notifier 419-470).
  c. `enqueue_external_git_handoff` → `branch_integration_queue` row with trigger `commit` (or `push` for a
     remote-tracking merge completion) + `external_git` adapter receipt (advance_notifier.rs:603-660).
4. `branch_integration_worker` (maintenance, `backend/g5-api-core/src/maintenance/branch_integration_worker.rs:1-40`)
   drains the queue, fetches the remote branch into the mirror, classifies via `classify_and_act`
   (`branch_integration_service.rs:275`; `TipRelation {Equal, WorkspaceAhead, MirrorAhead, Diverged, WatermarkViolation}`,
   `Action {Noop, Pushed, Imported, Diverged, DivergedPauseOnly, DirtySkipped}`), and pushes only when
   `should_push = trigger==Push || push_mode==Auto` (branch_integration_service.rs:132-135).
   `push_workspace_tip` opens the mirror at `repo_storage.local_path`, resets `origin` to `repos.git_url`, and
   `push_refspec("origin", local_branch_name, remote_branch, auth)` (branch_integration_worker.rs:2302-2340).
   Credentials: `ScmTokenManager::token_for_user(workspace.created_by)` (header 36-42). Diverged → opens a
   `job_merge_runs` row of `source_kind='remote_tracking'` (`remote_tracking_merge.rs:1-15,170`).
   Other server-side pushes to provider remotes: fork creation `push_create_ref` (`clone_service.rs:1715`), PR source
   `push_branch_ff_or_lease` (`workspace_service.rs:12521`, `workspace_pull_request_provider_lifecycle.rs:814`).

**Path 2 — worker task in a microVM** (same staged-ref protocol, internal routes):
`POST /api/internal/tasks/:id/publish-branch-state/prepare` and `/finalize`
(`backend/g5-api-routes/src/routes/internal.rs:7973-8060`). CLI side `stage_stable_repo_branches_with_client`
(`push.rs:1766-1845`) uses the worker JWT as `GitAuth::Token`, uploads with `upload_stable_operation_ref`
(push.rs:1689-1704) honoring the task-private `GIT_CONFIG_GLOBAL`, then `finalize_stable_repo_branches_with_client`
(push.rs:1847-). Publication kinds: `Checkpoint` (target task; `task.rs:3668-3672`), `Completion` (target job;
`task.rs:3737-3741`), `RebaseCheckpoint` (`task.rs:2606-2610`). Terminal completion requires a succeeded durable
task-commit receipt for every assigned repo (internal.rs:8033-8050). Job completion → job branch advanced; merging a job
into the workspace branch is the `job_merge_service` (9,255 lines, not traced here).

**Path 3 — UI/CRDT draft commit** (`mode='connected_commit_all'`, `crdt_scope_kind` canonical|task):
`POST /api/v1/workspaces/{id}/drafts/commit-all` (used 30× in `frontend/src`; `g5d commit` connected mode,
`backend/g5d-cli/src/commit.rs:1-5`). Server `create_connected_commit_all_operation` (finalizer 675) materializes the
Yjs draft into a commit in the mirror (`ops::prepare_files_commit_from_exact_base` family,
`backend/g5-workspace-ops/src/commit.rs:498-534`) and finalizes through the same operation registry.

### C.4 Other named mechanisms (OBS)

- `remote_tracking` / `tracking_mode`: child-only state; only `bootstrap_remote_tracking_branch_repo` and Make-Remote
  (214) promote to `'remote_tracking'` (118 header contract). `remote_branch` + `baseline_commit` required (114 shape
  check). `base_auto_pull` (131): Base workspaces may pull docs+code as one all-or-rollback op, audited as
  `trigger_kind='base_auto_pull'`. `auto_pull_enabled` (173): per child opt-in for scheduler `background_inbound`.
- `approved_rebase_publish` (128/131; `remote_tracking_merge.rs:1125 approve_remote_tracking_rebase_publish`) and the
  261 `rebase` transition: server-selected merge-source publication can publish a raw rebased result while CASing the old cut.
- `workspace_pull_requests`: local merge + provider PR lifecycle (`coordinated_repo_publication.rs`:
  `prepare/apply_workspace_pull_request_create|merge|refresh`, `close_workspace_pull_request`; provider PRs via
  `workspace_pull_request_provider_lifecycle.rs:244,336`).
- Credentials on the worker: `ScmCredential {url, token, provider}` injected at dispatch, written to a 0600 git
  credential store and zero-wiped on drop (`backend/g5d-runtime-core/src/scm_credentials.rs:1-12,25-40,56-70`);
  `task_git_config.rs` writes a task-private global gitconfig `.g5/gitconfig(.host)` with exact `safe.directory`
  entries because `-c safe.directory` is ignored cross-uid (`task_git_config.rs:1-17`). `g5_scm::GitAuth` is
  `Token | SshKey | None` with redacted Debug (`backend/g5-scm/src/auth.rs:16-35`).

---

## D. UI reads without a worker

- OBS: `GET/PUT/DELETE /api/v1/branches/:branchId/files[/*path]` (`backend/g5-api-routes/src/routes/files.rs:1-20`).
  Reads "capture the branch's typed, complete database-authoritative cut and select the requested repository's exact
  SHA … mutable refs and checkouts never select the version" (files.rs:12-15); handlers call
  `resolve_branch_authority_target` / `resolve_committed_snapshot` / `resolve_connected_repo_view`.
- OBS: `workspace_reader.rs` — "Immutable committed-repository reads … never fall back to the mutable repository
  checkout" (1-6). `read_files_at_commit` opens `GitRepo::open(repo_storage.local_path(repo_id))` in `spawn_blocking`
  and calls `show_file(commit, path)` (2120-2145); tree/diff/blame/log/ahead-behind are all `CommittedRepoReadView`
  methods over the same mirror (276-660). So the mirror on NFS is the read source; no worker involved.
- OBS: `RepoReadView` layers a CRDT `DocumentHandle` overlay + `dirty_files` on top of the committed view
  (workspace_reader.rs:676-760) — the "uncommitted" view for connected workspaces comes from Postgres CRDT drafts.
- OBS: `source_control_change_graph_service.rs:1` is a "deterministic, read-only semantic projection of current
  workspace changes" — `ChangeGraphComparison {Workspace, Uncommitted}`, base `DiffBase | AcceptedHead` (41-52), served
  at `/:id/rebase-merge/change-graph[/nodes/:artifactKey/details]`; it uses `diff_base_commit` (189) and CRDT drafts
  (`MAX_DRAFT_DIFF_INPUT_BYTES`, line ~32). 5,784 lines.
- Server-side object cache: none found beyond the mirror itself (INF — I did not find an in-memory git object/LRU
  layer; `repo_gc` exists precisely because reads hit loose objects on NFS).
- `GET /api/v1/workspaces/:id/source-download` archives "all eligible non-doc code repository source snapshots"
  from committed reads (`routes/workspaces.rs:2922-2942`).

---

## E. Restore after host/VM loss

- OBS durable state: Postgres rows (`workspace_branch_repos.head_commit` etc., operations registry, `draft_sessions.yjs_state`
  + `draft_updates` log, `task_commit_frontier_cuts.yjs_state`) and git objects in the mirror on NFS/local disk. Task
  checkpoints are real commits on `task/<id>` branches recorded in `task_checkpoints(task_id, commit_sha, files_changed,
  stats)` (001:495-505) and pushed via the staged-ref protocol.
- OBS disposable state: the per-task directory on the worker host. `materialize_repo_paths` deletes and recreates it
  (materialize.rs:727-730); `cleanup_for_task` removes it on teardown (989-1007); the supervisor calls cleanup at six
  sites (supervisor.rs:5790-5972). Recovery: a new assignment re-materializes from the warm cache cloned from
  `/git/{repo_id}` at `RepoInfo.base_commit`/`task_crdt_head_commits` (task_context.rs:3484-3490, 3520-3525).
- OBS: an explicit "task-workspace recovery obligation" exists for sticky chat runtimes (migration 269
  `teardown_reconciliation_kind IN ('registration_retirement','task_workspace_recovery')`; materialize.rs:997-1003
  preserves the dir when reserved) — i.e., today the platform *does* try to keep a VM's checkout alive for continuity.
- INF: uncommitted edits that were neither flushed to the CRDT draft (connected tasks) nor published as a
  `Checkpoint` are lost with the VM disk. No disk/overlay snapshot mechanism was found (searched materialize/repo_cache/
  supervisor for snapshot/tar of the workspace dir — none).
- INF: because refs are projections, a mirror whose `refs/heads/*` are stale/missing is repaired from the DB cut
  (`reconcile_branch_ref_to_authoritative`, `project_current_cut`); but a mirror whose *objects* are lost is not
  recoverable from the DB (only SHAs are stored). Object durability = NFS durability.

---

## F. g5d-cli git commands (OBS doc headers + call surveys)

| Command | What it does | Server interaction |
|---|---|---|
| `g5d push` | Managed workspace push: head-operation prepare → push to `refs/g5/staged/...` → finalize; may propose new repo membership; deferred child (fork) publish (`push.rs:1-5`) | `/workspaces/{id}/head-operations[/finalize]`, `/git/{repo}` |
| `g5d commit` | Connected: commits the server-side CRDT draft; offline: local git checkpoint. "Neither mode pushes" (`commit.rs:1-5`) | `/workspaces/{id}/drafts/commit-all` |
| `g5d sync` | Wrapper for docs/views/ontology sync jobs (`sync.rs:1-4`) | `/workspaces/{id}/docs/{}/sync`, `/jobs`, `/ontology` |
| `g5d merge` | Merge a source workspace into current: `ops::merge_workspace`, `ops::fetch`, `repo.update_branch_ref`, `checkout_branch` (`merge.rs:1-3`; call survey) | local or API-backed source |
| `g5d fork` | Local fork checkout via `ops::clone`/`ops::init`, writes `.g5/workspace.yaml` + `fork_lineage.yaml` for deferred publish (`fork.rs:1-4, 80, 140-148`) | none until push |
| `g5d refresh` | Fetch manifest, additive updates (adds repos, updates URLs/tracking, never deletes); `rebase_onto`, `is_ancestor_of`, `update_branch_ref`, `checkout_branch` (`refresh.rs:1-5`) | `/workspaces/{id}/manifest` |
| `g5d revert` | `reset_hard_to_head` on modified repos (`revert.rs:1-4`) | none |
| `g5d remote` | `status/sync/push/push-mode/runs/retry/merge` — pure API wrappers, "does not perform local git merge/push" (`remote.rs:1-5, 307-318`) | `/workspaces/{id}/sync`, `/push_to_remote`, `/push_mode`, `/branches/{b}/sync_runs/retry`, `/merge_runs/{}/{}` |
| `g5d clone` | Materialize from manifest, COW where possible, docs checkout, meta repo (`clone.rs:1-5`) | `/workspaces/{id}/manifest` |
| `g5d task complete/rebase` | Two-operation stable completion (checkpoint + completion) after CRDT barrier (`task.rs:3615-3645`) | internal `publish-branch-state/*` |
| `g5d connect` | Local checkout ↔ CRDT draft connector (`connect.rs:1-5`) | websocket CRDT |

Invariants/restrictions vs native git (OBS):
- Agent terminals block `checkout, switch, reset, stash, clean, merge, rebase, restore` because they "mutate branch state
  or the working tree outside the CRDT layer" (`backend/g5-scm/src/safety.rs:33-47`; enforced by
  `backend/g5d-cli/src/git/safety.rs:1-8`).
- The managed remote refuses everything except create-only `refs/g5/staged/<op>/<repo>`; no branch pushes, no force
  (git.rs:288-310, 672-676). `git clone`/`fetch` of the mirror only works with a task/workspace-scoped JWT.
- Local HEAD/branch moves are done by `git/head_reconciliation.rs` ("The server remains authoritative for workspace
  branch rows. This module is the g5d-side authority for moving local refs and checkouts", 1-4).
- `TaskSelfContextAllowedActions {commit, push_task_branch, complete_task, block_task, fail_task, publish_workspace_head}`
  gate what a task may do (`backend/g5d/src/worker/mod.rs:13930-13937`, fixture of the real DTO).
- Task-private `safe.directory` gitconfig (task_git_config.rs) and 0600 credential stores (scm_credentials.rs).

---

## G. Size

Line counts (OBS, `wc -l`):

| Area | Lines |
|---|---|
| `g5-api-core/services/workspace_service.rs` | 16,537 |
| `workspace_finalizer.rs` / `advance_notifier.rs` / `advance_consumer.rs` | 7,569 / 802 / 2,379 |
| `workspace_reader.rs` / `workspace_ops.rs` | 3,335 / 2,349 |
| `coordinated_repo_publication.rs` / `workspace_pull_request_provider_lifecycle.rs` | 2,931 / 2,221 |
| `remote_tracking_merge.rs` / `branch_integration_service.rs` / `branch_sync_runs_service.rs` | 2,763 / 820 / 349 |
| `maintenance/branch_integration_worker.rs` / `repo_gc.rs` | 3,798 / 769 |
| `clone_service.rs` (workspace fork) | 2,159 |
| `source_control_change_graph_service.rs` | 5,784 |
| `job_merge_service.rs` (not read; merge of job/workspace/remote-tracking sources) | 9,255 |
| `g5-api-routes/routes/{workspaces,git,files,blobs}.rs` | 7,300 / 1,604 / 892 / 312 |
| `g5-scm/src/*` (repo.rs 9,357) | 10,427 |
| `g5-workspace-ops/src/*` | 4,681 |
| `g5-storage` / `g5-blob` | 1,671 / 387 |
| `g5d-cli/src/{push,commit,sync,merge,fork,refresh,revert,remote,clone,workspace,workspace_manifest,stable_context}.rs` | 3,320 / 2,183 / 402 / 471 / 195 / 307 / 126 / 1,423 / 322 / 500 / 1,683 / 1,001 |
| `g5d-cli/src/git/{safety,head_reconciliation}.rs` | 488 / 3,328 |
| `g5d-runtime-core/src/{task_git_config,scm_credentials}.rs` | 459 / 584 |
| `g5d-foundation/src/workspace/{materialize,repo_cache}.rs` | ~5,400 / ~2,600 (9,039 dir total) |
| Migrations dedicated to tracking/authority/publication (114-120,128,131,134,145,173,189,196-210,212,214,215,261,262,278-280) | 34 files |

Tables/columns that exist only for publication/authority/remote-tracking (OBS): `workspace_head_operations`,
`workspace_head_operation_repos`, `workspace_head_operation_source_repos`, `workspace_head_operation_membership_proposals`,
`workspace_head_operation_make_remote_applications`, `workspace_advance_adapter_receipts`, `branch_integration_queue`,
`branch_sync_runs`, `branch_integration_remote_pair_leases`, `job_merge_runs.{source_kind='remote_tracking',
workspace_branch_repo_id, completion_advance_id}`, `workspaces.{push_mode, base_auto_pull_*, authority_advance_seq}`,
`workspace_branches.{authority_scope_kind, job_id, task_id, authority_scope_id}`, `workspace_branch_repos.{remote_branch,
baseline_commit, sync_paused_reason, last_sync_at, remote_tip_advisory_sha, remote_tip_advisory_observed_at,
auto_pull_enabled, base_head_commit}`, `search_reindex_queue.advance_id`.

---

## Coupling

1. **CRDT ⇄ Git authority.** `workspace_head_operations.mode='connected_commit_all'` and `crdt_scope_kind`, `draft_sessions.
   workspace_branch_repo_id` (062), `task_commit_frontier_cuts.yjs_state` (279), `RepoReadView` overlays, the "CRDT
   branch-access gate" that forces one branch name per workspace, and the blocked-git-subcommand list all exist because the
   CRDT draft, not the working tree, is the source of uncommitted truth. Removing CRDT collapses most of the finalizer's
   rollover/bookkeeping (`prepare_draft_commit_rollover`, `apply_prepared_draft_dirty_bookkeeping`, task CRDT watermarks).
2. **Postgres-authoritative heads ⇄ git refs as projections.** Every ref move is CAS-in-DB then project-to-git with retry
   and reconciliation (`project_internal_git` loops, `reconcile_branch_ref_to_authoritative`, 206). Choosing "git is the
   authority" would delete the operations registry, adapter receipts, and the projection/cleanup reapers.
3. **Job/task branch tree ⇄ workspace.** `authority_scope_kind` typing (197/208), `job_merge_runs`, task checkpoints and
   two-operation completion (Checkpoint+Completion) tie the job orchestration model to the branch model.
4. **Remote tracking ⇄ internal mirror.** `branch_integration_worker` runs *inside g5-api* against the NFS mirror with the
   creator's OAuth token; `tracking_mode`, `baseline_commit`, `refs/g5-tracking/*`, pair leases and sync-run audits are
   all about reconciling the mirror with the customer remote.
5. **Storage ⇄ deployment.** The whole read/write path assumes a POSIX path to the mirror (`RepoStorage::local_path`),
   which forces NFS (Filestore) for multi-replica g5-api and motivates `repo_gc` and lockfile code.
6. **Worker ⇄ internal mirror.** Workers clone only from `/git/{repo_id}` with task-scoped JWTs; their warm cache and
   reflink materializer are keyed to that URL; the host never talks to GitHub directly except via injected
   `scm_credentials` for non-managed URLs.

## Simplification candidates (for an internal-git-checkpoint model)

| Candidate | What it removes | Risk / what to keep |
|---|---|---|
| Make `refs/heads/<workspace-branch>` in the internal repo the head authority; drop `workspace_branch_repos.head_commit` CAS + operations registry | 196-215, 261, 278 tables; `workspace_finalizer.rs` (7.5k), `advance_notifier/consumer` (3.2k), projection reapers, staged/committed ref namespaces | Need an equivalent atomic multi-repo cut if a workspace still spans repos; git can only CAS one ref per repo (or use a single "workspace" repo / manifest commit). Lose replayable provenance rows. |
| Let agents run native `git commit/push` to `refs/heads/<branch>` on the internal remote (with a pre-receive/update hook or server-side check that the pushed branch equals the workspace's expected branch) | receive-pack staged-ref restriction, `X-G5-Operation-ID`, `require_push_ref_updates`, most of `push.rs` managed flow, `head_reconciliation.rs`, blocked-subcommand list | Governance moves to a hook; must still reject force-push/branch mismatch; audit trail becomes reflog/DB event instead of operation rows. |
| Drop `workspace_branches` ⇄ `workspace_branch_repos` split (keep one row per (workspace, repo)) | trigger chain 062/116/118/206, `authority_scope_*` typing if job/task branches become plain git branches | Job/task branches still need identity for scheduling; could be `job/<id>` naming convention + DB pointer only. |
| Checkpoint = commit on the workspace branch (or a `refs/g5/checkpoints/<ws>/<ts>` ref) pushed before VM shutdown; restore = clone from internal remote | `task_checkpoints` semantics, sticky-runtime "task_workspace_recovery" preservation (269/244/247), `repo_manifests` | Uncommitted-but-unsaved work is defined as lost; need a shutdown hook that always runs (`git add -A && commit && push` to a checkpoint ref) and a rule for squashing/cleaning checkpoint commits before publish. |
| Remove remote-tracking pipeline; publish to GitHub only on explicit user action (or GitHub is just another remote the agent pushes to) | `branch_integration_*`, `branch_sync_runs`, pair leases, `push_mode`, `auto_pull_enabled`, `base_auto_pull`, `remote_tracking` merge runs (~9k lines + 8 migrations) | Lose background import of upstream changes and the "diverged" merge-run UX; need a simple `git fetch upstream && rebase` story and where the creator's token is used. |
| Keep the smart-HTTP server + RepoStorage as-is (it is small: `git.rs` 1.6k, `g5-storage` 1.7k) | nothing | It already gives "internal Git" durability; consider bare mirrors only (drop non-bare `updateInstead` path) and object-store backup of `/data/repos`. |
| Remove non-bare provider mirrors (`GitRepo::clone` → `.bare(true)`) | `NON_BARE_RECEIVE_PACK_POLICY`, `.git`-or-`HEAD` probing, worktree dir config | Server-side merge/commit code (`merge_branch_no_checkout`, `prepare_*_commit_from_exact_base`) already works in-memory on bare repos per its names; verify no path relies on the mirror's working tree. |
| Reads stay on the mirror (`workspace_reader.rs`) but keyed by branch ref instead of DB SHA | `resolve_committed_snapshot` layers, `diff_base_commit`/`base_head_commit` bookkeeping | Need a stable "base" for the diff graph: could be the merge-base with the parent branch computed on demand. |

## Open questions (for the user)

1. **Checkpoint representation.** (a) Commits on the workspace branch itself (simple, but pollutes history; needs
   squash-before-publish); (b) a separate `refs/g5/checkpoints/<workspace>` ref updated force-style (history stays clean,
   restore = `git reset --hard` to it); (c) git bundle/tarball of the working tree + `.git` uploaded to object storage
   (captures untracked/ignored files like build caches, but loses git semantics). Which files must survive a VM loss —
   only tracked source, or also untracked/ignored (node_modules, target/)?
2. **Multi-repo atomicity.** Today one operation updates N repos atomically in Postgres. If the authority becomes git refs,
   is per-repo atomicity acceptable, or should a workspace become a single internal repo (subtrees/submodules) or a
   manifest commit?
3. **Where the branch↔workspace association is enforced.** A `pre-receive`/`update` hook inside g5-api's receive-pack
   (reject any ref other than the workspace's expected branch and its `job/*`/`task/*` children, reject non-FF) vs. a
   pre-flight check only on UI-triggered operations (your goal 5). Do you want force-push ever allowed (e.g. checkpoint refs)?
4. **Job/task branches.** Keep `job/<id>` / `task/<id>` branches as first-class git branches, or fold jobs into
   commits on the workspace branch (one VM per workspace suggests sequential tasks)?
5. **Remote publication.** Should the agent push to GitHub natively with injected credentials (goal 4/6), or should
   g5-api remain the only holder of provider tokens and push the workspace branch on user action?
6. **Storage backend.** Keep NFS Filestore for the mirror, or move to per-workspace bare repos on local SSD + periodic
   object-storage backup (bundles), since single-VM-per-workspace reduces the need for shared POSIX access?
7. **CRDT-era reads.** With no CRDT, "uncommitted" views in the UI would have to come from the VM (live) or from the
   last checkpoint. Is a UI view that is only as fresh as the last checkpoint acceptable?
8. **Historical rows.** Do you need to migrate existing `workspace_head_operations` provenance/receipts, or can the
   re-architecture start from current heads only?
