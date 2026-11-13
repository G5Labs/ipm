# Independent review 3: retire the protected-ref mirror / Base remote-tracking parity

Mode: independent discovery review, read-only (no production, test or ontology edits). Reviewer did
not implement the change and did not read the author's explanation. Scope: `origin/main..HEAD`
(7a131d3c design, 50535baf, 9e5eb122) plus the uncommitted working-tree change to
`backend/g5-api/tests/maintenance_integration.rs`.

Inputs read: request (`.g5/chat_intent/retire-protected-ref-mirror/request.md`), pinned design
(`.g5/ontology-plan-retire-protected-ref-mirror.md`, starting at "Ontology changes and governance"),
ontology diff (`workspaces.g5.md`, `durability.g5.md`, `git_governance.g5.md`, `core_user_stories.g5.md`),
`docs/data-model-cleanup-plan.md`, `docs/database-migrations.md`, the full code diff, and the affected
consumers listed below.

The ontology diff in the repository is the same as the diff recorded in the design. `git diff 7a131d3c..HEAD -- .g5 docs` is empty.

## Checks executed (by this reviewer)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` (backend) | **FAIL**. Diffs in 8 files, all of them changed on this branch (see F2). On origin/main, `workspaces.rs` and `workspace_service.rs` are fmt-clean. |
| `prettier --check` on changed frontend files | `WorkspaceStatusBand.test.tsx` was clean on origin/main and is now dirty. The other three files were already dirty on origin/main. |
| `npx vitest run` RemoteTrackingActions, BranchAutoPullControl, WorkspaceStatusBand, BranchActionsMenu tests | 4 files, 103 tests passed |
| `npx tsc --noEmit -p .` (frontend) | passes |
| eslint on changed frontend sources | clean |
| Rust integration tests (worker, migration 056, maintenance) | **Not run by this reviewer**. A full `cargo test -p g5-api --tests` was already running in the shared tree, so I relied on inspection only. |

## Findings

### F1 — Defect (ontology conformance, recoverability): Base has no path through the provider-rewrite checkpoint

- **Governing rule:** `g5.workspace.remote.base-parity` says Base gets "pause, divergence and provider-rewrite approval handling", and that Base's non-fast-forward cases "use the explicit rewrite-approval checkpoint without a VM". The design acceptance says "A provider rewrite of Base's tracked branch pauses for the rewrite-approval checkpoint".
- **Path:**
  - `remote_integrations.rs:286-296` (`classify_history`): the watermark check runs before tip equality, so a rewrite gives `WatermarkViolation`.
  - `branch_integration_service.rs:221-230` maps that to `Diverged{ResetBelowBaseline}`.
  - `worker.rs:1864-1870` then pauses with `watermark_violation` and opens a `reset_below_baseline` run.
  - The only completion for that run is approved rebase publication (`merge.rs:371-373`).
  - `workspace_service.rs:5115-5132` refuses that for Base with `APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE`.
  - `policy.rs::retry` returns `NonRetryable` for `watermark_violation`.
  - `merge.rs:1876` refuses Resolve on Base.
- **Consequence:**
  - Any provider rewrite of Base's tracked branch pauses that Base repository permanently. The UI still shows "Approve rebased publication" (`BranchTrackingPopover.tsx` MergeRunCard), and it fails.
  - This is reachable now: migration 056 turns auto-pull on for every existing Base binding, and background Base cycles are admitted.
  - Upgrade exposure: if the retired mirror had force-imported a provider rewrite into a Base (head equals the provider tip, but the watermark is no longer an ancestor of it), the first post-upgrade cycle hits `WatermarkViolation` and sticks. The design's "equal head, lagging watermark heals after one cycle" outcome holds only when the old watermark is still an ancestor.
  - Before this change the mirror force-imported rewrites. The coordinated pull only refused them and never created an unrecoverable pause.
- **Fix:** this needs a design decision, so treat it as an amendment. Options:
  - (a) A Base-capable rewrite approval. Under owner approval and the workspace fence, Base adopts the captured provider tip: CAS on the branch head, watermark and provider observation through the remote-tracking convergence, with no VM. Gate the UI to that action on Base.
  - (b) Amend `g5.workspace.remote.base-parity` to name the actual Base recovery, and stop offering the non-Base approval on Base.
  - Also decide whether "head == provider tip, watermark off-history" should heal as Equal. That would change the shared `classify_history` policy, so check it against `g5.design.authority` / `g5.architecture.policy`.
- **Regression check:** a worker test with a Base fixture where the provider force-pushes `main`.
  - A background cycle pauses with `watermark_violation`.
  - The chosen approval then succeeds without a VM and leaves head = watermark = provider observation = the new tip.
  - A second case starts with head = provider tip and the watermark off-history. Assert the chosen outcome.

### F2 — Defect (production readiness / CI): rustfmt check fails

- **Path:** the following files fail `cargo fmt --all -- --check` (Justfile:699 and :1931 run it in CI):
  - `g5-api-core/src/services/doc_service.rs:2546` (mis-indented `}],`)
  - `workspace_service.rs:978` (double blank line)
  - `remote_integrations.rs:27`
  - `g5-api-routes/src/routes/workspaces.rs:1870`
  - `g5-scm/src/lib.rs:32`
  - `tests/remote_repo_tracking_integration.rs`
  - `tests/migration_056_base_remote_tracking_parity.rs`
  - `tests/workspace_repository_model.rs`
- Frontend: `WorkspaceStatusBand.test.tsx:81` (`branches: []` mis-indented) is newly prettier-dirty.
- **Consequence:** the CI format gate fails.
- **Fix:** `cargo fmt --all`, plus `prettier --write` on that one test file.
- **Regression check:** `cargo fmt --all -- --check` exits 0.

### F3 — Defect (committed state): the maintenance runner test fails at HEAD

- **Path:** at HEAD, `backend/g5-api/tests/maintenance_integration.rs:798-801` still asserts 14 JoinHandles, but two loops were removed.
- The fix (12 handles plus a comment) exists only as an uncommitted working-tree change.
- **Consequence:** `test_maintenance_runner_spawns_and_cancels` fails on the pushed branch.
- **Fix:** commit the working-tree change. The value 12 is correct: 14 minus `protected_ref_mirror` and `base_auto_pull_scheduler`.
- **Regression check:** that test passes at the committed HEAD.

### F4 — Missing evidence / non-discriminating tests for the changed guards

1. `new_bindings_default_auto_pull_on_only_for_base` exercises only `bind_published`, and the product refuses that path for Base (`PUBLISH_NOT_ON_BASE`, `workspace_service.rs:7219`).
   - Base actually binds through Make Remote: `binding.rs:236` → `policy.rs::bind_policy`, or through the convergence inserts (`convergence.rs:299-310`, `:444-451`).
   - None of these is tested for the default, so the test would still pass if `bind_policy` lacked the Base default.
   - Add a Make Remote on Base (real route) asserting `auto_pull_enabled = true`, plus a non-Base Make Remote asserting false.
2. The `NOT w.is_base` removal in `policy.rs::enqueue_background_inbound_rows` (scheduler) and in `passive_candidates` (pinger), and the `|| current.is_base` removal in `remote_advisory_pinger.rs:234`, have no test.
   - `base_tracks_its_provider_through_the_ordinary_worker` enqueues `background_inbound` directly (`fx.enqueue`). It discriminates only the worker claim filter (`worker.rs:1225`).
   - Add a scheduler sweep (`enqueue_background_inbound_rows`) and an advisory ping for a Base binding.
3. Design acceptance outcomes with no test:
   - Base manual Pull through `POST /workspaces/:id/sync`, per repository, where one repository's failure does not block another.
   - Provider-rewrite pause on Base (see F1).
   - Base fetch credentials after ownership transfer.
   - Docs/ontology projection after a worker import into Base. The removed `repair_noop_coordinated_pull_docs` previously covered the noop case.
4. `maintenance_integration.rs::remote_actor_scope_check_requires_current_membership` was renamed and now asserts "remote integration must not keep using SCM tokens for a removed actor". However, it calls `PermissionService::user_has_workspace_scope`, which remote integration no longer calls.
   - The actual guard is the membership check in `auth/repository_credentials.rs` (~line 174), so the test cannot detect a regression in the claimed property.
   - Retire it with `base_pull_authorized`, or retarget it at `RepositoryCredentials::resolve` for a removed owner.

### F5 — Low (UI vs ontology): Resolve/approval actions are offered on Base where the backend refuses them

- **Rule:** base-parity says "Resolve with agent is therefore unavailable on Base".
- **Path:**
  - `BranchTrackingPopover.tsx` `PausedDivergenceResolution` and the MergeRunCard approval are not gated on `isBase`.
  - The backend refusal at `merge.rs:1876` returns a misleading "This workspace repository has no provider divergence to resolve".
  - The active-run branch of `start_remote_tracking_resolution` (`merge.rs:1862-1870`) does not check `is_base` at all.
- **Fix:**
  - Hide or disable these actions on Base, with copy pointing to Fork.
  - Return an explicit Base refusal.
- **Check:** a component test with a Base workspace paused `diverged` renders no "Resolve with agent".

### F6 — Low (stale text after the change)

- `workspace_service.rs:4321-4330` still says `BASE_REMOTE_BIND_REQUIRES_DEFAULT_BRANCH` is required "for coordinated pull". The rule itself is justified by Base's fixed default branch, but the reason text is wrong.
- `routes/workspaces.rs:4502` doc comment: "rejects local/pinned and Base-managed children" is no longer true.
- `docs/data-model-cleanup-plan.md:326-332` (the governing provider-transport doc) still describes "Base admission checks the actor's current permission and, for scheduled pulls, the current auto-pull policy… Base callers supply captured repositories" — the removed path.
- `tests/api_ontology.rs:403-405` says a Base pull "publishes them with `g5d push`".

### F7 — Low (minimal implementation)

- `worker.rs:1071, 1166`: `CycleState.is_base` is now written but never read. `#[allow(dead_code)]` hides it. Remove it.

### F8 — Design question / ontology accuracy: "Base cannot gain local commits"

- The base-parity text says Base cannot gain local commits. A **pinned** Base repository does gain local merge commits: a PR merge into a target with no bindings is local (`coordinated_repo_publication.rs:486-497`). The design's own rationale mentions "a pinned Base's merged commits".
- Make Remote on such a Base classifies `WorkspaceAhead` (`workspace_service.rs:~4483-4517`). That pauses it as `diverged` with a merge run that needs a VM agent, and Push is blocked while paused, so the Base is stuck.
- This stuck path is pre-existing, but the new D block declares it impossible.
- Proposal: correct the sentence, scoping it to remote-bound Base repositories whose merges are provider-first. Separately decide whether Make Remote on a fast-forward-ahead Base should publish (create-or-fast-forward push) instead of pausing.

### F9 — Design-text inconsistency: auto-pull-off acceptance outcome cannot occur

- The design acceptance says that with auto-pull off "the cycle only fetches and records `fetched`; the pill reads … behind once the advisory observes the tip". `data-model-cleanup-plan.md:82-83` says the same.
- In fact:
  - The scheduler does not enqueue such rows (`policy.rs` `push_mode='auto' OR auto_pull_enabled`).
  - The worker drops them (`worker.rs:1225-1231`).
  - The pinger skips bindings with auto-pull off (`remote_advisory_pinger.rs:234`).
- This holds for every workspace, so parity is preserved. The new test asserts *no* outcomes, which contradicts the design text.
- Proposal: correct the acceptance text, or decide that auto-pull-off Base should still be observed. This is pre-existing behavior, so it is not a defect of this change.

### F10 — Design-text inconsistency: QA reset

- The design's Risks section says "The QA reset should hold auto-pull off until App verification (design step 5)".
- The implementation keeps `scripts/reset-qa.py:480` seeding `auto_pull_enabled=true` (unchanged from main). `scripts/reset-qa.md:100-103, 140-142` documents that the binding stops at "Authentication required" until Retry.
- This is consistent with `g5.workspace.remote.base-auto-pull-default`, and the Retry path exists (`policy.rs::retry` accepts `auth_required`; `worker.rs:2611-2619`). Update the design's Risks bullet to match.

### F11 — Ontology metadata (proposed; the main agent applies)

- `workspaces.g5.md` footers still say "Not implemented" and list deleted paths:
  - `g5.workspace.remote.base-parity`
  - `g5.workspace.remote.base-auto-pull-default`
  - `g5.story.base-follows-provider`
- Proposed entries:
  - **base-parity:** Implemented (partial).
    - Worker/scheduler/pinger exemptions removed.
    - `sync_workspace` uses the per-repository enqueue.
    - The Base coordinated path, scheduler and columns are gone.
    - Gaps: F1 (rewrite approval unavailable on Base), F5 and F8.
    - Evidence: the worker test (import, lagging watermark, auto-pull off). The scheduler, pinger, route and transfer paths are untested (F4).
  - **base-auto-pull-default:** Implemented.
    - `policy.rs::initial_auto_pull_sql` is used by all four insert paths, and migration 056 switches existing Base bindings on.
    - Gap: only `bind_published` is tested (F4.1).
  - `durability.g5.md` **g5.git.mirror:** Implemented. The mirror loop, `fetch_protected_ref`, `repo_mirror_refs` (057) and `ProtectedRefObservation` are removed. No server-side provider fetch writes `refs/heads/*`: the remaining wildcard `GitRepo::fetch` callers are g5d-cli/VM-side.
  - **g5.git.mirror.update:** record the open-assumption result. No reader treating the default-branch ref as an independent mirror was found in projection_convergence, PR provider lifecycle or Make Remote, but this was a limited search.
  - `git_governance.g5.md` **g5.architecture.git_owner:** drop the "fetch_protected_ref remains a protected-ref import path" gap.

## Commitments checked with no finding

| Commitment | Path | Check | Result |
|---|---|---|---|
| `g5.git.mirror` (no poller writes a workspace branch) | `maintenance/mod.rs` (loop removed); `g5-scm/repo.rs` (`fetch_protected_ref` removed); grep for `refs/heads/*` fetch refspecs | Searched for server-side provider→`refs/heads` writers. Only remote integration (`prepare_pull` / convergence) remains; `GitRepo::fetch` callers are g5d-cli | Preserved (inspection) |
| `g5.git.mirror` / `repo_mirror_refs` retirement | `057_drop_repo_mirror_refs.sql`; `workspace_reader.rs`; `dto.rs` | No remaining readers/writers; no SQL functions or views reference the table; frontend, CLI and scripts have no `protectedRefObservation` consumers | Preserved |
| `g5.git.network.ref-scope` (protected branches refuse network writes) | `git_policy.rs:12-20`, `routes/git.rs:283` | `repos.protected_branches` and the receive-pack guard are unchanged | Preserved |
| base-parity: acting user is Base's current owner | `remote_integrations.rs` `POLICY_QUERY` (`w.owner_user_id AS acting_user_id`); `worker.rs:1103`; `repository_credentials.rs` membership check | No separately recorded Base actor remains (columns dropped); a removed owner fails closed | Preserved (inspection; F4.3 transfer untested) |
| base-parity: no Base-only importer/scheduler/policy/actor/exemption | `base_auto_pull_scheduler.rs`, `pull_remote_repository_set`, `prepare_base_pull`, `BasePullAdmission`, `base_pull_authorized`, `set_workspace_base_auto_pull` route, `set_auto_pull` Base refusal, worker/scheduler/pinger `is_base` filters all removed | Grep for remaining `is_base` in remote-tracking paths. Remaining: bind-to-default-branch (justified: fixed branch name), Publish refusal (justified), Resolve refusal (justified by no VM), approved-rebase refusal (see F1) | Preserved except F1/F5 |
| base-parity: per-repository import, no multi-repo cut (H014) | `workspace_service.rs::sync_workspace` → `enqueue_branch_integration_for_workspace` | Per-binding enqueue; paused rows skipped individually | Preserved (route untested for Base, F4.3) |
| base-parity: owner Pull/Push and auto-pull/auto-push controls | `RemoteTrackingActions.tsx`, `BranchAutoPullControl.tsx`, `BranchTrackingPopover.tsx`, `WorkspaceDetailsPanel.tsx:624`; routes `sync`, `push_to_remote` (Base refusal removed), `push_mode`, `repos/:repoId/auto_pull` | Controls render on Base (vitest); routes keep `ensure_workspace_owner` and the GitOperation policy | Preserved |
| base-auto-pull-default | `policy.rs:111-116` used by `bind_published`, `bind_policy`, `convergence.rs` x2; migration 056 | All four `INSERT INTO remote_integrations` production paths use the helper; non-Base COALESCE is false; `reset-qa.py` seeds true | Preserved (inspection; tests partial, F4.1) |
| Migration 056 (RLS owner-scope rule) | `056_base_remote_tracking_parity.sql`; `docs/database-migrations.md` rule 2 | Straight-line NO FORCE → UPDATE…FROM (both tables open) → SET CONSTRAINTS → FORCE; DROP CONSTRAINT/COLUMN list inside the open scope (045 precedent); constraint names match `001_lean_baseline.sql:5497, 8504`; FORCE restored on both; populated non-BYPASSRLS test with replay no-op exists | Preserved (inspection; test not run by me) |
| Migration 057 / fresh install | `057_drop_repo_mirror_refs.sql`; checksums | DDL only; checksums appended; fresh chain unaffected | Preserved (inspection) |
| `g5.workspace.identity.base` (read-only, no VM, no working branch) | Content-write guards untouched; `branch_commit.rs` Base initialization checks unchanged | No content edit or VM start added for Base | Preserved |
| `g5.workspace.base-ui.*` | `WorkspaceDetailsPanel.tsx`, `WorkspacePage.tsx` | Base omissions (Chat, Owner, VM) unchanged; Details does not hide tracking controls (open assumption resolved) | Preserved |
| `g5.workspace.remote-ui.status` | Repository summaries `tracking_mode` CASE (`repositories.rs:61, 141`) | Base unbound repos now `pinned` rather than legacy `tracking`, consistent with parity. Side effect: `WorkspaceTable.tsx:209` no longer badges unbound Base as "tracking" (intended: a pinned Base does not follow the provider). No consumer requires `'tracking'` (convergence only distinguishes `remote_tracking`) | Preserved |
| Removed API/DTO fields | `WorkspaceDetailDTO.baseAutoPullEnabled`, `SyncResultDTO.coordinated`, `PUT /base_auto_pull`, `ProtectedRefObservation`, config `G5_BASE_AUTO_PULL_INTERVAL_SECS` | grep across frontend (src, e2e), g5d / g5d-cli, scripts, helm/deploy: no remaining consumers; push results still use `coordinated` legitimately | Preserved |
| `trigger_kind` history compatibility | `branch_sync_run.rs` (`BASE_AUTO_PULL` removed) | Old rows remain valid text; no enum parse | Preserved |

## Summary of status

- **Implementation:** largely conforms. The mirror and every Base-only scheduler, actor, column, route, worker, scheduler and pinger exemption are removed. The default has one home and covers all insert paths. Migrations follow the RLS owner-scope rule.
- **Blocking defects:**
  - F2: the CI format gate fails.
  - F3: the committed test count is wrong; the fix is uncommitted.
  - F1: the provider-rewrite checkpoint is unusable on Base. This contradicts the new D block and is newly reachable now that auto-pull is on.
- **Verification gaps:**
  - F4: tests do not discriminate the scheduler and pinger changes, or the real Base binding path for the default.
  - No route-level Base Pull, transfer or rewrite tests.
  - I did not execute the Rust integration tests.
- **Design/ontology text corrections:** F8, F9, F10, F11.
