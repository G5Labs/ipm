# G5-Lean governance research: approvals, merge workflow, PRs, branch protection, authz

Repo: `/Users/zliu/Workspace/G5-Lean` (read-only). Paths are relative to that root; migrations live in
`backend/g5-api/src/db/migrations/`. **OBSERVED** = read in code at the cited line. **INFERRED** = my reading, flagged inline.

## 0. Executive summary

1. **Approvals are a fully built ledger with no enforcement.** Eleven governance tables, three services (7.8k lines),
   two route files and ~5k lines of frontend exist for approval definitions / requests / decisions / agent reviews, but
   **no merge, PR, publish or finalizer path reads `approval_statuses` / `approval_requests` / `approval_configs`**.
   OBSERVED: `rg -l "approval_statuses|approval_requests|approval_configs|approval_review_runs" backend --type rust`
   (non-test) returns only `routes/workspace_approvals.rs`, `routes/org_approvals.rs`, `routes/internal.rs`
   (auto-save of the review artifact), the three approval services, `job_service.rs` (creates the review job),
   `task_service.rs:639` (pending-review probe), and two maintenance modules. `job_merge_service.rs` (9,255 lines)
   contains zero occurrences of "approval"; so do `coordinated_repo_publication.rs` and
   `workspace_pull_request_provider_lifecycle.rs`. The frontend states it: `frontend/src/hooks/useWorkspaceApprovals.ts:8`
   "No enforcement — the finalizer gate is a later PR"; `frontend/src/components/workspace/PrepareToShipModal.tsx:398`
   "gated on PR + ready ONLY — never approvals".
2. **The internal git store is structurally protected — by protocol, not by branch name.** The smart-HTTP
   `git-receive-pack` handler accepts only *create-only* pushes to `refs/g5/staged/<operation_id>/<repo_id>` that were
   pre-registered in `workspace_head_operations` (`backend/g5-api-routes/src/routes/git.rs:288-334`,
   `backend/g5-api-core/src/services/workspace_finalizer.rs:1268-1313`). `git push origin main` against
   `{api}/git/{repo_id}` is a 403 for every caller. Branch heads are advanced only by the server-side finalizer.
3. **The provider (GitHub/GitLab) boundary is unprotected by G5.** No branch-protection concept exists (OBSERVED:
   `rg -i "branch_protection|protected_branch|required_status_check|enforce_admins|rulesets?"` over `backend` → 0 hits;
   frontend survey → 0 hits). For **chat-type tasks** the acting user's decrypted OAuth/PAT token is written into
   `<workspace>/.g5/task-<id>.git-credentials` and wired as `credential.helper=store`
   (`backend/g5d-worker/src/task_env.rs:110-135`), `git push` / `git remote` are allowed in agent terminals
   (`backend/g5-scm/src/safety.rs:46-48`), and VM egress has "zero rules at parity"
   (`backend/g5d-isolation/src/isolation/passt.rs:289-291`). Only the provider's own branch protection can stop
   `git push github HEAD:main` from a chat task.
4. **Roles are coarse and API-layer only.** Org roles `owner|admin|member|viewer` map to a fixed scope set; RLS
   isolates *tenants* (org membership), never roles. Workspace-level `admin|editor|viewer` is consulted by exactly two
   checks (mutation-policy transition, member management).
5. **Merge execution is server-side libgit2 plus optional agent "merge jobs"**; provider PRs mirror a local
   `workspace_pull_requests` row and are merged through the GitHub REST merge API with the acting user's token
   (provider-first when the target is a remote-tracking Base).

---

## A. Roles & authz

### A.1 Org roles and scopes (OBSERVED)
- `OrgRole { Owner, Admin, Member, Viewer }` — `backend/g5-contracts/src/types/enums.rs:286-291`; DB CHECK on
  `org_memberships.role` (`001_initial_schema.sql:45-46`). Invite / role update accept only `admin|member|viewer`;
  `owner` is never assignable and cannot be changed (`backend/g5-api-core/src/services/org_service.rs:588-604,
  770-778, 800-803`).
- `WorkspaceRole { Admin, Editor, Viewer }` — `enums.rs:318` (strings at `756-758`); DB CHECK on
  `workspace_members.role` (`001:141-142`).
- Scope vocabulary: 19 variants of `g5_auth::Scope` — `backend/g5-auth/src/scope.rs:35-102`:
  `workspaces:{read,write,admin}`, `jobs:{read,execute,cancel}`, `sessions:{read,write}`, `blobs:read`,
  `billing:admin`, `org:{read,admin}`, `users:read`, `workers:{read,admin}`, `settings:{read,write}`,
  `internal:*` (worker), `worker_host`.
- Role → scope mapping `PermissionService::get_role_scopes` — `backend/g5-api-core/src/services/permission_service.rs:1040-1101`:

  | role | scopes |
  |---|---|
  | viewer | `jobs:read`, `sessions:read`, `workspaces:read` |
  | member | viewer + `sessions:write`, `workspaces:write` |
  | admin | member + `billing:admin`, `jobs:execute`, `org:admin`, `workers:admin`, `workers:read` |
  | owner | same 10 strings as admin |

  `workspaces:admin`, `jobs:cancel`, `settings:*`, `users:read`, `org:read`, `blobs:read` are never granted by role.
- Effective scopes = `org_default_scopes ∪ user_scope_grants (unexpired) ∪ workspace_permission_overrides`
  (`permission_service.rs:14-24, 159-217`); tables created in `001:710-747`.
- Management routes (`backend/g5-api-routes/src/routes/permissions.rs:253-306`):

  | route | scope |
  |---|---|
  | `GET /api/v1/auth/me/scopes`, `POST …/check` | authenticated |
  | `GET /api/v1/orgs/:orgId/scopes` | `workspaces:read` |
  | `PUT /api/v1/orgs/:orgId/scopes` | `org:admin` |
  | `GET/PUT …/scopes/members/:userId/scopes` | `workspaces:read` / `org:admin` |
  | `GET /api/v1/workspaces/:id/permissions` | `workspaces:read` |
  | `PUT/DELETE …/permissions/:userId` | `org:admin` |

  The frontend never calls the org-scope or workspace-override routes (survey: referenced only by
  `frontend/src/lib/__tests__/route-contract.test.ts`).
- Org member routes (`routes/orgs.rs:495-513`): invite, `PATCH …/members/:userId` (role), `DELETE`, invitations —
  all `Scope::OrgAdmin`; `GET /members` is any member.

### A.2 Workspace-level access (OBSERVED)
- `check_user_workspace_access` — `backend/g5-api-core/src/db/scoped.rs:1020-1104`: org `owner|admin` → any live
  workspace in the org; otherwise `workspaces.visibility='org'` → allowed, `'invite_only'` → creator or a
  `workspace_members` row, else `Forbidden`.
- `PermissionService::user_has_workspace_scope` — `permission_service.rs:221-298` repeats that predicate then checks
  the scope union (used by the worker scheduler and `git.rs`).
- `is_workspace_admin` (org owner/admin OR `workspace_members.role='admin'`) —
  `backend/g5-api-core/src/services/workspace_service.rs:4648-4694`; `can_manage_workspace_members` adds
  `workspaces.created_by` — `4704-4720`. These are the only two authorization consumers of `workspace_members.role`
  that I found.

### A.3 Route-layer enforcement (OBSERVED)
- `AuthContext { User(JwtClaims), Worker{worker_id, org_id, task_id, scopes, worker_session_generation},
  Host{host_id, org_id, scopes} }` — `backend/g5-api-core/src/middleware/auth_middleware.rs:72-118`.
- `RequireScopes` layer: 403 if any required scope is missing, for all three variants — `auth_middleware.rs:926-1060`.
- `AuditedRouter` has no `route()`; every handler is `audited_route(path, handler, &[Scope])` or
  `exempt_route(path, handler, reason)` — `backend/g5-api-routes/src/routes/audited.rs:61-136`. A source-scanning test
  fails CI on unguarded handlers (`routes/guards.rs:1-28, 110, 209`).
- `OrgGuard` middleware checks org membership on `/api/v1/orgs/:orgId/...` —
  `backend/g5-api-core/src/middleware/org_guard.rs:55-177`.
- Internal worker routes (`/api/internal/...`) are `exempt_route` with reason "worker session JWT auth
  (Scope::Internal)" — `routes/internal.rs:3150-3240`; handlers call `require_worker_context` and
  `validate_worker_authority_guard` (`internal.rs:7974-7990, 7711-7725`).

### A.4 g5-authz vs permission_service (OBSERVED)
- `g5-authz` = **tenant isolation + capability minting only**. `AuthzService` trait: `is_org_member`,
  `mint_db_capability`, `mint_service_db_capability`, `mint_worker_db_capability`, `mint_blob_capability`,
  `user_org_ids` — `backend/g5-authz/src/lib.rs:66-140`. `DbCapability { org_ids, actor_id, scopes, session_id,
  ttl_secs }` — `capability.rs:12-23`. `ScopedDb::from_capability` opens a transaction and runs
  `SELECT authz.set_ctx($1)` — `scoped_db.rs:114-121`; sessions are rows in `authz.user_sessions` (60 min TTL, `008`).
- RLS: every governance table has `ENABLE + FORCE ROW LEVEL SECURITY` and policy
  `tenant_isolation USING (org_id = ANY(authz.current_org_ids()))` (074/124/129/162/186/253/265); the
  `set_org_id_from_session()` BEFORE INSERT trigger stamps `org_id` (`012_rls_policies.sql:25-54`). No policy
  references a role or scope.
- SQL roles (`017_role_provisioning.sql`): `g5_tenant` (RLS DML pool), `g5_app` (system pool, SELECT-only on
  tenant tables), `authz_writer`, `authz_ctx_writer` (owns the SECURITY DEFINER ctx functions). Approval tables grant
  `g5_tenant` only `SELECT, INSERT` (`186:229-288`); `approval_requests` adds column-level `UPDATE (state,
  settlement_fingerprint, closed_at, updated_at)` (`253:356-381`) — the DB backstops append-only.
- `permission_service` = **the "who may do what" policy** (roles → scopes, grants, overrides). INFERRED: the split is
  clean — authz answers "which org rows", permission answers "which verbs".

### A.5 Git smart-HTTP auth (OBSERVED, `routes/git.rs`)
- `GitAuth::from_headers` accepts `Bearer <jwt>` or Basic `x-worker-token|x-user-token|x-access-token:<jwt>`;
  `is_worker = scopes ∋ internal:*` — `git.rs:132-230`.
- `require_read`: workers need an in-scope task (`RunningOrLiveClaim`) on a workspace using the repo
  (`worker_allowed_branches_for_repo`, join `tasks→jobs→workspaces→workspace_branches`, `git.rs:1099-1180`);
  users need `user_has_workspace_access` — `git.rs:393-437`.
- `require_push`: workers need a *running* task; users need `user_can_write_workspace` (org `owner|admin|member`,
  or `workspaces:write` via org defaults / user grants / workspace overrides) — `git.rs:236-286, 1284-1369`.

---

## B. Approvals model

### B.1 Objects (OBSERVED; schema from migrations, writers from services)
| Object | Key columns / value sets | Written by |
|---|---|---|
| `approval_definitions` — org-scoped, append-only versions, PK `(id, version)` | `name`, `humans_list uuid[]`, `agent_type ∈ {simple}`, `agent_prompt`, `agent_harness ∈ {claude_code, codex}`, `agent_model`, `agent_effort`, `invalidates_previous`, `stage_planning` (default false), `stage_finalizing` (default true), `planning_fallback ∈ {carry_over, skip}` — `186_approvals_governance_core.sql:35-56`, `268:16-20`, `284:7-30`; record `approval_definition_service.rs:32-56` | `ApprovalDefinitionService::create` / `publish_version`; never UPDATE/DELETE (`approval_definition_service.rs:1-7`) |
| `approval_configs` — attach/retire events binding a definition to a **target** workspace | `target_workspace_id`, `definition_version_at_write`, `retired` — `186:61-75` | `ApprovalConfigService::attach` / `retire` — `approval_config_service.rs:933-1045` |
| `approval_requests` — one per `(definition version, actor_slot)` per **source** workspace | `request_key` = sha256(`ApprovalRequestKeyV1{org, source_ws, def, version, actor_slot "human:<uuid>"}`), `actor_type ∈ {human, agent}`, `state ∈ {pending, settled, cancelled, expired, superseded}`, `settlement_fingerprint`, `requested_by` — `253:230-259`; unique active key `256` | `create_human_requests` — `approval_config_service.rs:1050-1225`; agent — `approval_review_service.rs:3517` |
| `approval_statuses` — immutable decisions | `source_workspace_id`, `outcome ∈ {approved, rejected, deferred}`, `actor_type`, `actor_user_id`, `reviewed_git_heads` (head vector), `stage ∈ {planning, finalizing}`, `agent_approval_run_details` (v1–v4) — `186:85-114`, `268:24-26`, `285:88-167` | human — `record_human_decision` `approval_config_service.rs:1333-1517`; agent — `approval_review_service.rs:925, 1978, 2390` |
| `approval_review_runs` — agent review execution ledger, PK `job_id` | `state ∈ {pending_assignment, assigned, generated, merged_to_draft, settled, failed, cancelled}`, `output_doc_path` = `docs/<source_ws>/reviews/agent-approval-<job_id>.md`, `generated_result ∈ {approved, rejected}`, `approval_status_id` — `265:330-579`; ladder trigger `265:589-706`; verdict immutable `285:244-262` | `job_service.rs:5261` on `POST …/approvals/reviews`; advanced by internal task transitions (`internal.rs:2227-2273`) |
| `merge_evaluations` | `merge_run_id`, `outcome ∈ {passed, failed}`, `evaluation jsonb` — `186:142-161` | **no Rust reader or writer outside tests** (`rg merge_evaluations backend --type rust`) |
| `agent_approval_handoffs` — chat tool-permission prompts (different domain) | `approval_id`, `decision ∈ {approved, rejected}`, `status ∈ {resolving, applied, failed, cancelled}`, unique `(session_id, approval_id)` — `162:7-29` | `chat_service.rs:16177-16192` |
| `workspaces.stages ∈ {1,2}`, `workspaces.merge_target_workspace_id` | `178_workspace_stages_and_merge_target.sql:1-26` | fork dialog / workspace update |
| `jobs.approval_context` (immutable via trigger) | `218:231-232, 249-267` | review-job creation |

### B.2 Semantics (OBSERVED)
- A requirement is attached to the **target** (parent / base) workspace; decisions and requests are recorded against
  the **source** (child) and bound to its committed head vector (`reviewed_git_heads`), so a new commit renders prior
  decisions stale → `pending` (`approval_config_service.rs:2423-2427`; test
  `backend/g5-api/tests/api_agent_approval_evaluation.rs:233`).
- Target for approvals = `merge_target_workspace_id` else `cloned_from_id` (`approval_config_service.rs:1697-1718`);
  a decision without a merge target is a validation error (`1493-1500`).
- Stages: `applicable_stages(workspace.stages, stage_planning, stage_finalizing, planning_fallback)` — a 1-stage
  workspace only has `finalizing` (planning-only definitions carry over when `planning_fallback='carry_over'`); a
  2-stage workspace may have both gates — `approval_config_service.rs:2285-2303`. Default decision stage is
  `finalizing` (`1387`).
- Derived requirement state (a projection): `approved` when every listed human approved, `rejected` when any current
  state is rejected, else `pending` — `approval_config_service.rs:178-181`; `caller_can_decide` — `183-187`.
- Agent-as-approver: `POST /api/v1/workspaces/:id/approvals/reviews` (scope `jobs:execute`,
  `routes/workspace_approvals.rs:501`) → `ApprovalReviewService::admit` requires `agent_type='simple'` with a non-empty
  prompt and the definition at its *latest* version (`approval_review_service.rs:3346-3455`), forks a private job
  branch, creates a job with `job_type='approval_review'` (`218:214-229`) executed by a worker; the worker's markdown
  verdict is re-read and settled into `approval_statuses` (`approval_review_service.rs:412-600, 925-1000`).
  "Immediate verdict" (285) lets a run settle without a publication step (`285:172-242`).
- Human inbox = Home attention items `HomeAttentionKind { Approval, OpenQuestion, HumanEscalation }` —
  `home_service.rs:60-64`, fed by `ApprovalConfigService::actionable_for_org` (`home_service.rs:265-290`;
  route `routes/home.rs:96`).
- Routes: `routes/org_approvals.rs:218-231` — `GET /api/v1/orgs/:orgId/approval-definitions` (any member),
  `POST /` and `POST /:definitionId/versions` (`org:admin`). `routes/workspace_approvals.rs:469-503` —
  `GET /api/v1/workspaces/:id/approvals` (member), `POST /requirements` and
  `POST /requirements/:definitionId/retire` (`org:admin`), `POST /decisions` (exempt: "exact request ownership is the
  authorization"), `POST /requests` (`workspaces:write`), `POST /requests/:requestId/cancel` (exempt: requester
  identity), `POST /reviews` (`jobs:execute`).

### B.3 What actually requires approval today (OBSERVED unless marked)
| Action | Gate | Evidence |
|---|---|---|
| Merge child → parent / merge target (`POST /workspaces/:id/merge`, `/rebase-merge*`, `/pull-requests/:pr/merge`, `POST /jobs/:id/merge*`) | **No approval check.** Scope + workspace access + mutation policy + git preflight only | `job_merge_service.rs` 0 "approval" hits; `coordinated_repo_publication.rs` 0; handlers `workspaces.rs:4670-4722, 5298-5640`, `jobs.rs:1050-1059` |
| Publish / push to provider (`/push_to_remote`, `/branches/:b/repos/:r/publish`) | No approval; Base refused; create-only / fast-forward-or-lease | `workspace_service.rs:7906-7990, 12184-12240` |
| Force-publish after reset-below-baseline (`/remote_tracking/approved_rebase_publish`) | User re-confirms an explicit checkpoint set; scope `workspaces:write`; "approved" = the caller's confirmation, not a separate approver | `workspaces.rs:1684-1688`; `workspace_service.rs:8068-8190` |
| Agent tool call requiring permission (chat) | **Real gate**: the worker blocks until `POST /api/v1/sessions/:id/approvals/:approvalId` (scope `sessions:write`) records a handoff | `routes/chat.rs:1315-1318, 3563-3600`; `chat_service.rs:16177-16215` |
| Task / job completion | No approval; worker authority guard + publication scope only | `internal.rs:7711-7830` |
| "Budget approval" | DTO flag `requires_budget_approval: max_tokens > 50_000`, no other reference | `workspaces.rs:1035, 2472` |
| Human escalation (agent asks a human) | Blocks the *task* until resolved; not a merge gate | `human_escalation_service.rs:1-11` |

INFERRED: the "finalizer gate" named by the frontend was meant to consult `approval_statuses` at base-advance time and
write `merge_evaluations`; neither exists in this tree.

### B.4 Approval state machine (OBSERVED from CHECKs, triggers and services)
```
approval_configs (target ws)    attach ─► active ─retire─► retired ─attach─► active        (append-only events)

approval_requests (source ws; one per definition-version × actor_slot)
   create_human_requests | admit(agent)
        │  ON CONFLICT (org, source_ws, request_key) WHERE state='pending' → reuse existing
        ▼
     pending ──record_human_decision | agent settle──► settled (settlement_fingerprint, closed_at)
        │──cancel_human_request (requester only)──────► cancelled
        │──retention / maintenance────────────────────► expired | superseded
   trigger guard_approval_request_update: exactly one pending→terminal transition, identity immutable (253:304-349)

approval_statuses (immutable rows; newest eligible row per actor wins)
   outcome ∈ approved | rejected | deferred   (deferred = durable "undo" back to pending)
   eligible iff reviewed_git_heads == current source head vector, else shown as pending ("stale")
   publish definition version with invalidates_previous=true ⇒ older-version rows flagged superseded

approval_review_runs (agent review job, PK job_id)
   pending_assignment → assigned → generated → merged_to_draft → settled(approval_status_id)
        any ─► failed | cancelled                    (265:589-706; verdict immutable 285:244-262)

derived requirement state: approved | rejected | pending  → UI tab + Home inbox; consumed by nothing else
```

---

## C. Merge workflow

### C.1 Branch / workspace hierarchy and head authority (OBSERVED)
- Task branch `task/<task_id>` (`task_service.rs:30830`) → job branch `job/<job_id>` (`job_service.rs:3211`) →
  workspace branch (`workspace_branches.branch_name`, one row per repo) → parent workspace (`cloned_from_id`) or
  explicit `merge_target_workspace_id` (`workspace_service.rs:10608-10660, 10817-10855`) → **Base** workspace
  (`workspaces.is_base`), which remote-tracks the provider branch and is only advanced by pull (`push_to_remote`
  rejects Base with `COORDINATED_PUSH_REQUIRES_NON_BASE`, `workspace_service.rs:7913-7925`).
- Child publication to the provider uses branch `g5/<repo_branch_name>` (`workspace_service.rs:12263`) and is
  create-only: an existing remote branch at a different head fails with `PUBLISH_TARGET_REF_EXISTS`
  (`7820-7830`).
- Head authority is Postgres, not git: each advance is a `workspace_head_operations` row — register (`preparing`) →
  client pushes to `refs/g5/staged/<op>/<repo>` → `mark_staging_ref_uploaded` flips to `prepared`
  (`workspace_finalizer.rs:1322-1398`) → finalizer verifies the cut and updates `workspace_branches.head_commit`
  + `workspace_branch_repos.head_commit` (`workspace_ops.rs:1962-1995`) → canonical refs are re-projected from the DB
  cut via `promote_operation_ref` ("Canonical refs are disposable projections of the complete current
  database-selected cut", `workspace_finalizer.rs:3337-3405`).

### C.2 Merge/PR/publish route inventory (OBSERVED; all `AuditedRouter`)
| Route | Scope | Handler → service |
|---|---|---|
| `POST /api/v1/jobs/:id/merge/preflight` | `jobs:execute` | `preflight_job_merge` → `JobMergeService::prepare_transaction_free_job_merge_preflight` |
| `POST /api/v1/jobs/:id/merge/direct-clean` (body `preflightFingerprint`) | `jobs:execute` | `direct_clean_job_merge` → `prepare_direct_clean_job_merge` (`job_merge_service.rs:4119`) |
| `POST /api/v1/jobs/:id/merge` | `jobs:execute` | `start_job_merge` → `prepare_job_merge_start` (agent merge job, `5804`) |
| `POST /api/v1/workspaces/:id/merge` | `workspaces:write` | `merge_workspace` → `WorkspaceService::merge_workspace` (fast-forward into parent/target, `10608`) |
| `POST …/:id/rebase-merge`, `…/preflight`, `…/direct-clean`; `GET …/change-graph[…]` | write / write / write / read | `start_workspace_merge` → `prepare_source_workspace_merge` (`4410`) |
| `GET/POST …/:id/pull-requests`, `POST …/preview`, `POST …/:pr/{merge,refresh,close}` | read / write | `coordinated_repo_publication` + provider lifecycle |
| `POST …/:id/push_to_remote`, `PUT …/push_mode`, `PUT …/base_auto_pull`, `POST …/sync` | `workspaces:write` | `WorkspaceService::push_to_remote` (`7906`) |
| `POST …/:id/branches/:b/repos/:r/publish`, `…/remote_tracking`, `PUT …/auto_pull` | `workspaces:write` | `publish_branch_repo` (`12184`), `promote_branch_repo_remote_tracking` |
| `POST …/:id/remote_tracking/approved_rebase_publish` | `workspaces:write` | `approved_rebase_publish` (`8068`) |
| `GET …/:id/branches/:b/merge_runs`, `POST …/:mergeRunId/{start,complete}` | read / write | remote-tracking merge runs (`remote_tracking_merge.rs`) |
| `POST /api/internal/tasks/:id/publish-branch-state/{prepare,finalize}` | worker JWT (`internal:*`) | `prepare_publish_branch_state` (`internal.rs:7974`) → `register_worker_branch_state_publication` |

### C.3 Job merge (job branch → workspace branch) (OBSERVED)
- Eligibility (`job_merge_service.rs:3866-3980`): job `completed` (or a `sync` job blocked with all tasks done),
  `branch_name` starts with `job/` and is materialized, no completed `job_merge_runs` row already consumed it, no
  active run on the workspace (`status IN pending|running|waiting_for_human|rolling_back`), job type not
  merge/preview/explainer/retired-sync. `merge_disabled_reason` strings: "another merge job is running",
  "job branch has already been merged", "job status is …", "job branch is missing or not materialized".
- Preflight: `MergePreflightDecision { FastForwardOrNoOp, CleanMergeNoGitConflicts, GitConflicts, WorkflowRequired }`
  with per-repo `MergePreflightRepoOutcome { Identical, SourceAlreadyMerged, FastForward, CleanMerge, GitConflicts,
  WorkflowRequired }` and a `preflight_fingerprint` that must match at apply time (`636-657`, `4160-4165`).
- Direct-clean: the server materializes a synthetic merge job/run and an authority advance without an agent
  (`4119-4170`; post-commit consumer `merge_publication`).
- Agent merge: `prepare_job_merge_start` forks `job/<merge_job_id>` from the source branches, freezes the agent launch
  (provider / model / permission_mode / effort), and creates merge job + task + run (`5804-5940`; react loop
  `workflow.job_merge.react_loop`, `job_merge_service.rs:47`). Run status `waiting_for_human` exists for agent
  questions (`074:33-36`).
- `job_merge_runs.advances_target` (145) separates source-only rebase runs from target-advancing merges;
  `completion_advance_id` (199) binds a run to its head operation; partial unique indexes enforce one active run per
  source job / source workspace / target (`075:51-65`, `145:22-26`).

### C.4 Workspace merge (child → parent / merge target) (OBSERVED)
- `POST /workspaces/:id/merge`: fast-forward-only per branch into `base_branch`; an explicit merge target is
  re-authorized for user callers with `check_user_workspace_access` (`workspace_service.rs:10608-10660`); non-user
  actors skip the visibility predicate (`workspaces.rs:4680-4686`).
- `POST /workspaces/:id/rebase-merge`: source-owned rebase workflow; Base cannot be a source
  (`reject_base_source_workspace`, `job_merge_service.rs:4432`); clean → direct application, conflicts →
  `WorkflowRequired` → agent merge job (`4456-4480`).
- Mutation policy is consulted (`job_merge_service.rs:40` imports `ensure_workspace_mutable`; PR create
  `workspaces.rs:5116-5121`) but Git operations themselves are exempt (§F).

### C.5 Remote-tracking merges and publication (OBSERVED)
- `branch_integration_worker` classifies `(workspace tip, mirror tip, baseline, dirty_files, trigger)`; `Diverged`
  opens a paused `job_merge_runs` row with `source_kind='remote_tracking'` (`remote_tracking_merge.rs:1-14`).
- `PushMode { Manual, Auto }` (`branch_integration_service.rs:101-107`); `TriggerKind` includes `Push` and
  `ApprovedRebasePublish` (`66-80`, migration 128).
- `push_to_remote` blocks repos paused for `watermark_violation` with "use approved_rebase_publish"
  (`workspace_service.rs:7969-7976`).

### C.6 Who triggers, what executes (OBSERVED + INFERRED)
- Triggers are humans through the routes above. Agents execute merge *jobs* but cannot start them: no agent tool
  creates PRs, merges, or approvals (tool list from `g5d-tooling/src/tools/*.rs`: `git_commit`, `git_push`,
  `git_pull`, `git_fetch`, `git_status`, `git_diff`, `run_in_terminal`, `transition_task_state`,
  `human_escalation`, … — no `create_pull_request` / `merge` / `approve`). INFERRED: goal #4 is not met today.
- CLI: `g5d merge` (local workspace-to-workspace merge, never pushes, no checks —
  `backend/g5d-cli/src/merge.rs`) and `g5d job merge` (merges completed task branches locally then raw
  `git push origin <branch>` — `job.rs:653-777`) are operator commands with no approval checks.
- Executor: server-side libgit2 (`g5_scm::GitRepo`) on the repo store inside `workspace_head_operations`
  finalization, or a worker-run merge job whose result flows through the same staging-ref protocol.

```
  task/<t> ──(task completion publish: staging ref + finalizer)──► job/<j>
  job/<j>  ──(POST /jobs/:id/merge[/direct-clean]: direct or agent merge run)──► <workspace branch>
  child ws ──(POST /workspaces/:id/merge | rebase-merge | pull-requests/:pr/merge)──► parent / merge_target ws
  non-Base ws ──(push_to_remote / publish: ff-or-lease, user token)──► provider g5/<branch>
  child ws ──(PR merge, target = remote-tracking Base)──► GitHub merge API FIRST ──► coordinated pull ──► Base
```

---

## D. Pull requests

### D.1 Local PRs (OBSERVED)
- `workspace_pull_requests` (`124:7-29`): `status ∈ {open, merged, closed}`,
  `local_merge_status ∈ {ready, conflict, stale, blocked, merged, closed}`,
  `provider_status ∈ {local_only, pending, open, action_required, failed, partial, merged}` (`129:26-35`),
  `preflight jsonb`, `last_preflight_fingerprint`, `description_override` (217); one open PR per `(source, target)`.
- `workspace_pull_request_events.event_type ∈ {created, refreshed, source_rebased, merge_blocked, merged, closed,
  provider_created, provider_updated, provider_action_required, provider_failed, provider_merged}` (`129:56-69`).
- `workspace_pull_request_provider_repos`: `provider ∈ {'github'}` only, `provider_pr_number|url|node_id`,
  `provider_status ∈ {pending_create, open, action_required, failed, merged}`, head/base SHAs (`129:71-102`).
- Create (`workspaces.rs:5092-5240`): read access on target, write on source, `ensure_workspace_mutable(source)`,
  insert `status='open', provider_status='local_only'` (`coordinated_repo_publication.rs:428-447`), push/publish
  source branches first (`5160-5166`), commit, then best-effort provider create (`5170-5225`).

### D.2 Provider lifecycle (OBSERVED, `workspace_pull_request_provider_lifecycle.rs`)
- Token: `resolve_github_auth(scm, org_id, actor_id)` → the **acting user's** stored connection
  (`auth/git_auth_resolver.rs:93-110`: `user_scm_connections.access_token_enc` for `(user_id, provider)`,
  decrypted). No GitHub App / installation token / deploy key code path exists.
- Create/update: push `g5/<branch>` with `push_branch_ff_or_lease` (`731-826`), `ls-remote` the target, find-or-create
  the PR via REST (`566-720`); target-branch drift is advisory (`529-541`).
- Merge preflight: GitHub PR `state=='open'`, head SHA == recorded `source_head_sha`, base SHA == recorded
  `target_head_sha` (else `Conflict`); merge via `merge_pull_request(token, full_name, number, head_sha)`;
  `!merge.merged` → `Conflict` (`862-1035`).
- Ordering (`workspaces.rs:5298-5640`): target is a remote-tracking Base
  (`coordinated_repo_publication.rs:576-577` `target_is_remote_tracking_base`) → **GitHub merges first**, then Base
  is imported by a coordinated pull, then the local PR is marked merged ("never mint a local Base commit ahead of
  GitHub", `5450-5456`). Otherwise the local merge commits first and the provider merge is best-effort afterwards
  (`5390-5445`).
- Branch protection: **neither configured nor read by G5.** If GitHub rejects the merge (required reviews / checks),
  G5 records `failed` / `action_required` and returns 409. Provider protection is therefore relied upon *implicitly*
  and only on the PR path; direct pushes never go through a PR.

---

## E. Enforcement locations

### E.1 Rule → layer matrix (OBSERVED)
| Rule | UI | API route | Service | DB | SCM provider |
|---|---|---|---|---|---|
| Tenant isolation | – | `OrgGuard` | `ScopedDb` capability | RLS `tenant_isolation` everywhere | – |
| Scope per endpoint | display only (`useAuth.tsx:42-56` decodes JWT unverified) | `RequireScopes` via `AuditedRouter` | re-checks (`user_has_workspace_scope`) | – | – |
| Workspace visibility (`org` / `invite_only`) | – | – | `check_user_workspace_access` (`scoped.rs:1020`) | – | – |
| Approvals before merge | advisory badges (`PrepareToShipModal.tsx:4-9, 398, 405`) | **none** | **none** | append-only + immutability triggers only; 0 blocking triggers | – |
| Merge preflight (conflicts, stale fingerprint, active-run exclusivity) | server-derived `mergeEligible` / `localMergeStatus` | – | `job_merge_service` (`3866-3980`, `4160`) | partial unique indexes on active runs | – |
| Internal git push destination | – | `git.rs:288-334` staging refs only; `670-674` no force | `authorize_staging_ref` (`workspace_finalizer.rs:1268`) | `workspace_head_operations` lease/status | – |
| Worker publishes only its own branches | – | `internal.rs:7711-7830` (`stable-cwd:branch`, `stable-cwd:isolated-branch`) | `validate_worker_authority_guard` | – | – |
| Protected `main` / base on provider | – | **none** | `push_to_remote` / `publish` ff-only, create-only, refuse Base | – | **only GitHub/GitLab's own rules** |
| Read-only workspace (mutation policy) | badge + admin toggle | routes call `ensure_workspace_mutable` | `workspace_mutation_policy.rs:214-241` | CHECK only (`163:22-29`) | – |
| Agent tool-permission approval | chat inline approve/deny | `POST /sessions/:id/approvals/:approvalId` | `chat_service::resolve_agent_approval` + worker sidecar | unique `(session_id, approval_id)` | – |

Frontend gates are all UX-only by the client's own admission (`WorkspaceDetailsPanel.tsx:170-172` "Client scope check
is a UX gate only; the server route guard remains authoritative"; `lib/agentApprovals.ts:14-16`). The only client
gate with no server counterpart is the approvals tab's "READY / missing N" verdict
(`WorkspaceApprovalsPageContent.tsx:1157-1161`), which is never sent to any merge call
(`useWorkspace.ts:591` posts `{}`).

### E.2 Internal receive-pack sequence (OBSERVED, `routes/git.rs:635-700`)
1. `GitAuth::from_headers` validates the JWT; `is_worker` from `internal:*`.
2. Parse pkt-line ref updates; resolve `repo_id` → local path via `RepoStorage`.
3. With `X-G5-Operation-ID`: every ref must be `refs/g5/staged/<that op>/<repo>` and the user must have write access to
   the proposal's workspace (`require_provisional_push`, workers refused). Without it: `require_read` + `require_push`.
4. `require_push_ref_updates`: each ref must parse as a staging ref for this repo, `old_sha == 0`, `new_sha != 0`, and
   `authorize_staging_ref` must find a live `workspace_head_operations` row (`status IN (preparing, prepared)`,
   unexpired lease) whose `proposed_new_head` equals `new_sha` (`workspace_finalizer.rs:1268-1313`).
5. `X-G5-Force-Push` → 403; stale CAS → 409.
6. Spawn `git receive-pack --stateless-rpc` with `env_clear()`; on success `mark_staging_ref_uploaded` → `prepared`.
   Heads are not moved here ("receive-pack is preparation-only", `git.rs:719-723`).

### E.3 Could an agent with plain git credentials push directly to main/base today? (OBSERVED)
- **Internal store: no.** `origin` in managed workspaces is `{external_url}/git/{repo_id}`
  (`task_service.rs:6608-6612`; manifests are validated to that shape, `g5d-cli/src/connect.rs:844-851`). Any
  `refs/heads/*` destination is rejected by step 4 above. There is no `pre-receive` hook; enforcement is in Rust before
  git is spawned (`git.rs:782-800`).
- **Provider: yes for chat tasks, subject only to provider-side protection.**
  - `resolve_task_scm_credentials` returns the user's SCM credentials only when `job_type == JobType::Chat`
    (`task_service.rs:14649-14658`); other job types get none.
  - The worker writes them to `<workspace>/.g5/task-<id>.git-credentials` (`0600`) as
    `https://x-access-token:<token>@github.com/...` (`oauth2` for GitLab) and exports `G5_REPO_CREDENTIALS` plus
    `credential.helper=store --file=…` to host and VM (`task_env.rs:110-135`,
    `g5d-runtime-core/src/scm_credentials.rs:174-220`).
  - Agent terminal git blocklist = `checkout, switch, reset, stash, clean, merge, rebase, restore` — CRDT-motivated;
    `push` and `remote` allowed (`g5-scm/src/safety.rs:40-48`; wired at `g5d-tooling/src/tools/terminal.rs:464-465`).
  - `git_push` tool has no branch/approval gate (`git_ops.rs:995-1034`); `git_commit` also pushes (`949-951`);
    `resolve_git_auth` uses the worker JWT for mirror URLs and otherwise falls back to `credentials.first()` for any
    non-matching remote (`git_ops.rs:1908-1955`).
  - VM egress: "zero rules at parity" (`passt.rs:289-291`). G5 never verifies stored OAuth scopes
    (TODO at `auth/scm.rs:291-292`).
- **Platform-initiated provider pushes** run server-side with the acting *user's* token, are fast-forward-or-lease,
  target `g5/<branch>`, refuse Base, and refuse to overwrite an existing remote branch at a different head.
  INFERRED: reaching provider `main` through the platform needs the PR merge API or `approved_rebase_publish`, both
  subject to GitHub's rules.

### E.4 Credentials the worker holds (OBSERVED)
- Worker session JWT: default scopes `[Scope::Internal]` (`"internal:*"`), TTL 12 h
  (`backend/g5-auth/src/minter.rs:279-285`); presented to the mirror as Basic `x-access-token:<jwt>` (`git.rs:176-186`).
- Worker registration tokens: `WorkerTokenScope { Org, Workspace, User }` (`worker_token_service.rs:24-32`); `User`
  "inherits the creating user's permissions".
- SCM: `GitAuth::{Token, SshKey, None}` (`g5-scm/src/auth.rs`); tokens are the user's OAuth/PAT from
  `user_scm_connections`.

---

## F. Workspace mutation policy (OBSERVED)
- `workspaces.mutation_policy ∈ {read_write, read_only}`, `mutation_policy_transition ∈ {NULL, 'freezing'}`, audit
  columns (`163:22-29`, `164:30-39`). Migration comment: "Read-only is an app/content mutability policy, NOT a Git-ref
  protection rule … Enforcement lives in the application mutation boundaries" (`163:10-15`).
- `WorkspaceMutationKind` (`workspace_mutation_policy.rs:24-84`): blocked when read-only =
  `DirectWorkspaceWrite | DirectAgentWrite | BackgroundContentWrite | WorkspaceOwnedCommentWrite`; always allowed =
  `GitOperation, GitImportProjection, IssueAssociationWrite, AdminWorkspaceAdministration, PerUserState,
  ReadOnlyPreviewSession, JobCancellation` (`91-99`). `freezing` blocks only the two user-initiated kinds so admitted
  agents drain (`113-119`). Error `WORKSPACE_READ_ONLY` → 403 (`348-358`).
- Transition route `POST /api/v1/workspaces/:id/mutation-policy` (`workspaces.rs:1582`, `workspaces:write`) also
  requires `is_workspace_admin` (`3300-3312`).
- It is an app-layer content freeze, not branch protection: merges into a read-only parent, receive-pack, and remote
  tracking remain allowed by design.

---

## G. Size (OBSERVED, `wc -l`)
- Backend services: `job_merge_service.rs` 9,255; `approval_review_service.rs` 3,921; `approval_config_service.rs`
  3,022; `approval_definition_service.rs` 853; `workspace_pull_request_provider_lifecycle.rs` 2,221;
  `coordinated_repo_publication.rs` 2,931; `remote_tracking_merge.rs` 2,763; `branch_integration_service.rs` 820;
  `workspace_finalizer.rs` 7,569 (head-operation protocol, shared with non-governance paths);
  `workspace_mutation_policy.rs` 359; `permission_service.rs` 1,415; `org_service.rs` 1,572;
  `human_escalation_service.rs` 1,415; `home_service.rs` 595.
- Routes: `org_approvals.rs` 355; `workspace_approvals.rs` 873; `permissions.rs` 704; `git.rs` 1,604; `audited.rs`
  235; `guards.rs` 700; `human_escalations.rs` 576; merge/PR/publish handlers live inside `workspaces.rs` (271 KB) and
  `jobs.rs`.
- Authz/auth: `g5-authz` 1,436; `g5-auth` 2,177; `middleware/auth_middleware.rs` 2,898; `org_guard.rs` 216;
  `db/scoped.rs` 2,495; `auth/scm.rs` 795; `git_auth_resolver.rs` 305.
- Worker side: `g5d-tooling/src/tools/git_ops.rs` 2,706; `scm_credentials.rs` 584; `task_env.rs` 417;
  `g5-scm/src/safety.rs` 294.
- Frontend governance (approvals, PRs, merge modals, remote tracking, members): 13,086 lines across the surveyed
  files (largest: `WorkspaceApprovalsPageContent.tsx` 1,794; `OrgApprovalsPageContent.tsx` 1,276;
  `RebaseChangeGraphPreview.tsx` 1,228; `PrepareToShipModal.tsx` 895).
- Backend tests for approvals / merge / route-auth: 15,223 lines (`api_agent_approval_review.rs` 3,014;
  `route_auth_static.rs` 3,262; `api_workspace_approvals.rs` 1,763; `migration_253_approval_requests.rs` 1,208).
- Schema: 11 governance-only tables (`job_merge_runs`, `job_merge_artifacts`, `workspace_pull_requests`, `_events`,
  `_provider_repos`, `approval_definitions`, `approval_configs`, `approval_statuses`, `merge_evaluations`,
  `approval_requests`, `approval_review_runs`) + `agent_approval_handoffs`; 22 triggers; 17 validator SQL functions;
  0 triggers that block a merge on approval. Core-table columns added: `workspaces.{mutation_policy,
  mutation_policy_transition, mutation_policy_updated_by, mutation_policy_updated_at, stages,
  merge_target_workspace_id}`, `jobs.approval_context`; job_type values `merge`, `approval_review`; job status
  `merged`; task_type `merge`.

### Verification greps run (for the record)
- `rg -il approval backend --type rust` and `rg -n -i approval backend/g5-api-core/src/services/job_merge_service.rs` (0 hits).
- `rg -n "approval_config_service\.|ApprovalConfigService|outgoing_state"` outside the service → only `workspace_approvals.rs`, `home_service.rs`, `home.rs`, `services/mod.rs`.
- `rg -n "merge_evaluations" backend --type rust` (non-test) → 0 hits.
- `rg -i "branch_protection|protected_branch|required_status_check|enforce_admins|rulesets?" backend --type rust` → 0 hits.
- `rg -n "push_force\(" backend --type rust` (excluding the definition) → 0 callers.

---

## Coupling
- **Approvals ↔ workspace topology and head model.** Everything keys on `merge_target_workspace_id ?? cloned_from_id`
  and on the source workspace's committed head vector (`reviewed_git_heads`). Approvals are coupled to the
  DB-authoritative head model, not to git refs; moving heads to plain git branches invalidates the eligibility logic
  as written.
- **Approvals ↔ agent runtime.** Agent reviews are ordinary jobs (`job_type='approval_review'`) with 14 composite FKs
  from `approval_review_runs` into jobs / tasks / workers / head operations / docs and an auto-save into the docs
  draft (`internal.rs:2227-2273`). The ledger cannot be removed without touching the job/task lifecycle.
- **Merge ↔ head-operation protocol.** Every merge (direct or agent) ends as a `workspace_head_operations` advance +
  `job_merge_runs` row + `advance_consumer` receipts; `git.rs` receive-pack is meaningless without the finalizer
  tables. This coupling is what protects the internal store.
- **PRs ↔ user tokens.** Provider create / merge / publish all run under the acting user's personal token; there is no
  service identity at the provider, so GitHub attributes every merge to the clicking human.
- **Frontend ↔ projection contracts.** ~5k lines of approval UI depend on `WorkspaceApprovalsState` (stages, requests,
  review runs, decision log).
- **Mutation policy ↔ every write route.** Dozens of call sites classify a `WorkspaceMutationKind`; orthogonal to git.
- **Chat-task credentials ↔ tool surface.** `git_push` / `git_commit` / `run_in_terminal` inherit whatever credential
  helper the task env installed; the terminal blocklist assumes no provider credentials are present.

## Simplification candidates (minimal governance enforced at the git boundary)
Keep:
1. **The internal receive-pack allow-list + head-operation finalizer** (`git.rs`, `workspace_finalizer.rs`). It already
   makes base/main unreachable by plain git. If workspaces become one VM each with the internal store as checkpoint
   storage, keep "staging-ref-only pushes" and add a *named-branch policy* at the same place: allow
   `refs/heads/<workspace branch>` fast-forward from the workspace's own VM identity; deny `refs/heads/main|<base>`
   unless an approval record exists for `(target branch, source head)`. That satisfies goal #6 without UI checks.
2. **Org roles + `RequireScopes` + RLS tenant isolation.** Cheap, audited by `guards.rs`. The unused
   `workspace_permission_overrides` / `user_scope_grants` UI-less machinery can go if nothing in production uses it.
3. **One provider path to base**: the PR merge API, with GitHub/GitLab branch protection *configured or verified by G5
   at repo connect time* (today it is neither).

Drop / collapse (with risks):
- **Approval ledger v1–v4** (`approval_requests`, `approval_review_runs`, `merge_evaluations`, stages, immediate
  verdicts, 17 validator functions, 22 triggers) → one `approval_decisions(target_branch, source_head, approver,
  outcome, created_at)` table checked in exactly one place (the base-advance finalizer and the provider PR merge call).
  Risk: loses agent-review provenance (artifact hashes, head vectors), the 2-stage planning gate, and ~15k lines of
  tests.
- **Agent-as-approver as a job type** → make the reviewer a normal task that posts a decision through the same API a
  human uses. Risk: weaker evidence binding.
- **Parallel merge machinery** (`job_merge_runs` with three `source_kind`s, direct-clean vs react-loop,
  remote-tracking runs, `advances_target`) → with one VM per workspace, a merge is `git merge` in the VM followed by a
  normal checkpoint push; conflict resolution is just an agent task. Keep only "base advance requires approval +
  preflight fingerprint".
- **Chat-task provider token injection** (`task_env.rs`) → replace with a branch-restricted, short-lived credential
  (GitHub App installation token scoped to non-protected refs, or a G5 proxy remote), or route all provider writes
  through the API. Risk: agents that legitimately `git fetch` private provider repos need a read-only path; the
  `git_push` tool must call the platform instead. Interim mitigations: drop the `credentials.first()` fallback
  (`git_ops.rs:1948`), move the credential store outside the workspace tree, add `remote` to the terminal blocklist.
- **`workspace_mutation_policy`** → keep as-is (359 lines) or fold into a branch-level freeze at the git boundary;
  orthogonal but wired into many routes.
- **Dead / display-only**: `merge_evaluations`, `jobs.approval_context`, `requires_budget_approval`,
  `WorkerTokenScope::User`, `push_force` (no callers) — safe to delete.

## Open questions (for the user)
1. Is the intended enforcement point for "appropriate approval before merging" the **internal base branch**
   (finalizer), the **provider `main`** (GitHub branch protection / PR merge), or both? Today neither checks approvals.
2. With one VM per workspace and native git, does `origin` stay the internal `/git/{repo_id}` store (keeping the
   staging-ref protocol) or become the provider URL? This decides whether `git.rs`'s allow-list survives.
3. Should agents ever hold provider tokens inside the VM? If yes, is a GitHub App installation token with
   branch-restricted permissions acceptable, or must every provider write go through the API?
4. Must the 2-stage (planning / finalizing) gate and agent-authored review artifacts survive, or is a single
   "approved at head X for branch Y by Z" record sufficient?
5. Who may approve: org `admin` / `owner` only, per-definition `humans_list` as now, or provider-side `CODEOWNERS`?
6. Is `workspace_mutation_policy` (content freeze) still needed once branch protection exists at the git boundary?
7. Are `workspace_permission_overrides` / `user_scope_grants` used in production (no frontend caller), or can
   permissions collapse to the four org roles plus workspace visibility?
8. Should Postgres remain the head authority (refs as projections), or should git refs become authoritative with the
   DB as an index? The current protection model depends on the former.
