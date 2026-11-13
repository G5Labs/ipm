# Review 1 — retire-protected-ref-mirror (independent discovery, read-only)

Scope: `origin/main..HEAD` (7a131d3c, 50535baf, 9e5eb122) plus the uncommitted edit to
`backend/g5-api/tests/maintenance_integration.rs` (handle count 14 → 12). Inputs: request,
pinned design (`.g5/ontology-plan-retire-protected-ref-mirror.md`), ontology diff, governing
blocks. No production, test or ontology file was edited.

## Checks run

| Check | Result |
|---|---|
| `vitest` RemoteTrackingActions, BranchAutoPullControl, BranchActionsMenu, WorkspaceStatusBand | 4 files, 103 tests pass |
| `tsc --noEmit -p tsconfig.json` (frontend) | clean |
| `cargo test --test migration_056_base_remote_tracking_parity` | pass (populated, non-BYPASSRLS owner, replay no-op) |
| `cargo test --test remote_repo_tracking_integration base_tracks_its_provider_through_the_ordinary_worker` | pass on rerun. The first attempt panicked creating a scoped DB while the shared Postgres was saturated. |
| `cargo test --test remote_repo_tracking_integration new_bindings_default_auto_pull_on_only_for_base` | pass |
| `cargo test --test remote_tracking_background_loop scheduler_skips_base_workspace_remote_tracking_rows` | **FAIL** (left 1, right 0); see F1 |
| Shared full run (`scratchpad/g5-api-tests.log`, not started by me) | stopped at `api_workspace_pull_requests` with 13 × HTTP 500 in fixture PR creation. My single rerun of one of those tests failed on Postgres `53300 too many clients`, and peer processes were running the same binary concurrently. The cause looks environmental but is **not established**. Later binaries, including `remote_tracking_background_loop`, never ran in that run. |
| `rustfmt --check` on changed Rust files | formatting drift (F12) |

## Findings

### F1 — Defect (high; fails CI): a test still asserts the retired Base scheduler exemption
- Governing: `g5.workspace.remote.base-parity` ("no Base-only … scheduler … or exemption").
- Path: `backend/g5-api/tests/remote_tracking_background_loop.rs:507-533` (`scheduler_skips_base_workspace_remote_tracking_rows`). The diff did not touch it.
- Consequence: it fails deterministically now that `policy.rs:262` admits Base (`left: 1, right: 0`). The integration lane goes red, and the scheduler half of parity has no positive test.
- Fix: invert it, e.g. `scheduler_enqueues_base_like_any_workspace`, asserting `base.background_queue_count == 1` (and auto-pull-off + manual push mode → 0).
- Regression check: the inverted test fails if `AND NOT w.is_base` returns to `enqueue_background_inbound_rows`.

### F2 — Defect / design gap (high): no resolution path after a provider rewrite or divergence on Base
- Governing: `g5.workspace.remote.base-parity` covers "pause, divergence and provider-rewrite approval handling", says Base's non-fast-forward cases "use the explicit rewrite-approval checkpoint without a VM", and says these paths contain no Base-only exemption. Plan acceptance: "pauses for the rewrite-approval checkpoint; the branch is not force-imported."
- Path:
  - A force-push of the tracked provider branch classifies as `WatermarkViolation` (`remote_integrations.rs::classify_history`). That produces `Action::Diverged{ResetBelowBaseline}` (`branch_integration_service.rs:222`), which pauses `watermark_violation` and opens a `reset_below_baseline` run (`worker.rs:1866`).
  - The only approval action is `POST …/remote_tracking/approved_rebase_publish`, and it still refuses Base: `workspace_service.rs:5115-5131` `APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE`.
  - Even without that refusal, the approval's semantics publish G5's reviewed tip *to the provider* (`worker.rs::prepare_approved_rebase_publish`, ~2162+). On Base that would push Base's pre-rewrite head over the provider's rewrite.
  - With a lagging watermark and Base's default `push_mode='auto'`, a rewrite can instead classify `Diverged`. That gives `DivergedPauseOnly`, whose only action, Resolve with agent, is refused on Base (`merge.rs:1876`).
- Consequence: after any force-push of the tracked branch, Base stops following the provider indefinitely. The UI offers an Approve/Resolve action that always fails, and no owner-level recovery exists. The retired mirror force-imported this case, so this is a newly reachable dead end.
- Fix (needs a human design decision, then an ontology amendment): define Base's rewrite approval as "accept the provider tip into Base." It would be a CAS import of the provider tip together with the watermark and provider ref, under current-owner authority, with no provider push, and could be built as a Base variant of the existing checkpoint. If that is deferred, record the gap explicitly in the base-parity footer and in the design's Risks.
- Regression check: a worker test with a Base fixture. Force-push the remote to an unrelated commit and drain one cycle: the repo pauses `watermark_violation` and Base's head is unchanged. Approve through the public route: Base's head and watermark equal the provider tip, the pause clears, and the provider ref is unchanged.

### F3 — Defect (medium): the UI offers VM-only actions on Base
- Governing: base-parity says "A step that needs a running owner VM, such as Resolve with agent, is therefore unavailable on Base". `g5.workspace.base-ui.*` also applies.
- Path:
  - `frontend/src/components/workspace/BranchTrackingPopover.tsx`: `PausedDivergenceResolution` (~566-615) and `MergeRunCard`'s `RemoteConflictAgentLauncher` / approval (~470-520) have no `isBase` gating.
  - The "behind" guidance (~764-777) says "Ask an agent in the running workspace…", which is wrong for Base.
- Consequence: since this change, Base reaches these states, and its owner sees actions the backend refuses.
- Fix: read `isBase` (as `RemoteTrackingActions` does) and hide the agent launcher. Show Base-appropriate guidance: Fetch from Remote or enable auto-pull, plus fork to edit.
- Regression check: a popover vitest with `isBase: true` and a diverged/reset run asserts there is no launcher or approve button.

### F4 — Missing evidence (medium): the auto-pull default test does not exercise Base's real binding path
- Governing: `g5.workspace.remote.base-auto-pull-default`.
- Path: `remote_repo_tracking_integration.rs::new_bindings_default_auto_pull_on_only_for_base` calls `bind_published`. That is the Publish path, which Base refuses (`workspace_service.rs:7219` `PUBLISH_NOT_ON_BASE`). Base binds through Make Remote, i.e. `binding.rs:236` → `policy.rs::bind_policy`.
- Consequence: dropping the default from `bind_policy` (or from the two `convergence.rs` inserts) would not fail any test.
- Inspection: all four insert paths use `initial_auto_pull_sql` with the correct workspace placeholder (`$2` in `bind_policy`/`bind_published`, `$1` in `convergence.rs:301,445`). The code is correct.
- Fix: add a Make Remote-on-Base test (route or `bind_policy`) asserting `auto_pull_enabled = true`, alongside a non-Base Make Remote asserting `false`.
- Also missing: a route-level test of `PUT /workspaces/:id/repos/:repoId/auto_pull` on a read-only Base as its owner. The test calls `set_auto_pull` directly.

### F5 — Missing evidence (medium/low): acceptance outcomes with no executed check
- Advisory pinger includes Base: `remote_advisory_pinger.rs:234` and `remote_integrations.rs:109`. Verified by inspection only.
- Base manual Pull through `POST /workspaces/:id/sync`: per-repository enqueue, then worker import, with one repository's failure not blocking another. Only the mocked frontend test covers it, and the old coordinated-pull API test was deleted.
- A provider rewrite on Base pauses rather than force-importing (see F2).
- After a Base ownership transfer, fetches use the new owner. By inspection, `POLICY_QUERY` uses `w.owner_user_id` (`remote_integrations.rs:71`) and explicit triggers check transfer (`worker.rs:1189`). Not executed.
- The "behind" pill when auto-pull is off: by inspection, the pinger skips rows with auto-pull off (`remote_advisory_pinger.rs:234`). With `push_mode='manual'`, nothing observes the tip, so Base cannot read "behind". This is ordinary behaviour, but the plan's acceptance wording assumes otherwise. Clarify or test it.

### F6 — Undocumented design deviation (low/medium): the QA reset does not hold auto-pull off
- Governing: pinned design step 5 and Risks ("The QA reset should hold auto-pull off until App verification").
- Path: `scripts/reset-qa.py:480` already seeds `auto_pull_enabled=true` and was unchanged. `scripts/reset-qa.md:100-103,140-142` now documents an `auth_required` pause plus **Retry** instead.
- Consequence: the behaviour is acceptable (the pause is set at `worker.rs:2619`), but the implementation diverges from the reviewed design without an amendment.
- Fix: update the design's step 5 and Risks to record the chosen behaviour, or seed `false` and enable after App verification.

### F7 — Stale governing document (low): `docs/data-model-cleanup-plan.md`
- `:327-333` ("Review follow-up", an undated design section) still says "Base admission checks the actor's current permission and, for scheduled pulls, the current auto-pull policy… Base callers supply captured repositories and fetched tips."
- `:350-353` claims regression coverage for "Base all-repository publication and refusal, disable/permission changes", but those tests were deleted.
- `:406` lists "Base pulls … protected-ref refresh" as credential consumers.
- Fix: rewrite these lines to describe the per-repository integration, or mark them historical.

### F8 — Stale ontology metadata (low; expected review obligation)
- These footers still read "Not implemented" and name deleted code:
  - `g5.git.mirror` (`durability.g5.md:368`)
  - `g5.workspace.remote.base-parity` (`workspaces.g5.md:803-807`)
  - `g5.workspace.remote.base-auto-pull-default` (`:813`)
  - `g5.story.base-follows-provider` (`:570`)
- `g5.architecture.git_owner` (`git_governance.g5.md:140`) keeps the `fetch_protected_ref` gap, but the function is gone.
- Unchanged footers that still cite mirror freshness: `g5.architecture.content_reader` (`workspace.g5.md:148`, "failed mirror fetches can appear fresh") and `g5.documents.read.labels` (`documents.g5.md:584`). `protected_ref_observation` had no consumer, so no client lost a label.
- Proposed status lines:
  - `g5.git.mirror`: Implemented. The loop, `fetch_protected_ref`, `repo_mirror_refs` (057) and the DTO field are removed. The remaining provider→branch writers are the worker import, Make Remote and approved publication. The bare `+refs/heads/*:refs/heads/*` fetch is called only by g5d-cli.
  - base-parity: Partially implemented. Exemptions are removed from the worker, scheduler, pinger and `set_auto_pull`, and Base uses `/sync` enqueue. Gaps: F2 and F3.
  - auto-pull default: Implemented through `initial_auto_pull_sql` on all four insert paths and migration 056. Gap: F4 test coverage.
  - `g5.git.mirror.update`: the open assumption is resolved by inspection. Every default-branch read goes through Base membership (`CASE WHEN is_base THEN default_branch`): `workspace_fork_service.rs:648,2748`, `workspace_repositories.rs:35`, `projection_convergence.rs:246`. No independent mirror reader exists.

### F9 — Stale strings and dead field (low)
- `workspace_service.rs:4321-4331`: the `BASE_REMOTE_BIND_REQUIRES_DEFAULT_BRANCH` message says "for coordinated pull". The restriction is justified by Base's fixed branch name, which is out of scope; reword the reason.
- `workspace_mutation_policy.rs:35`: comment mentions "base pull/auto-pull".
- `worker.rs:1071,1166`: `CycleState.is_base` no longer has a reader (hidden by `#[allow(dead_code)]`). Remove it for minimality.

### F10 — Design question (low): Base `push_mode` default
- `org_service.rs` creates Base without `push_mode`, so it gets the column default `'auto'` (migration 040). The diff also removed `COORDINATED_PUSH_REQUIRES_NON_BASE` (`workspace_service.rs` ~4961).
- Effect under parity:
  - (a) Base background cycles run even with auto-pull off, because `push_mode='auto'` admits them. They fetch only.
  - (b) If Base is ahead of the provider (possible only with a lagging watermark, e.g. QA's pre-fix state, after a provider resets the branch backwards), an automatic cycle fast-forward pushes Base's head to the provider default branch and undoes the provider's reset.
- The ontology grants Base the auto-push controls, so this is parity. The user specified only the auto-pull default, so confirm whether Base should start `push_mode='manual'`.

### F11 — Production readiness question (low): contract migration before rollout
- 056/057 run as the Helm pre-upgrade hook. Old pods still select `base_auto_pull_enabled` (GET workspace runtime query, `WorkspaceDetail` selects, org bootstrap `RETURNING`), and the old mirror and scheduler write and read the dropped objects. They return 500s or log errors until rollout completes.
- Precedent exists (045 dropped columns in the same release), so this is probably accepted. Confirm that the deployment tolerates it.

### F12 — Nit: formatting
`rustfmt --check` reports drift in:
- `doc_service.rs:2546` (misindented `}],`)
- `workspace_service.rs:978` (double blank line)
- `routes/workspaces.rs:1870`
- `g5-scm/src/lib.rs:32`
- `remote_repo_tracking_integration.rs:3523`
- `migration_056_…rs:207`

Also `frontend/.../WorkspaceStatusBand.test.tsx:81` indentation. No CI format gate was found.

## Commitments checked with no finding
- `g5.git.mirror` → `maintenance/mod.rs`, `g5-scm/src/repo.rs`, migration 057 → searched for every provider→`refs/heads` writer (`fetch*` refspecs and callers) → **preserved**. API-side writers are only remote integration. `g5d-cli`'s bare forced fetch is VM-side against G5 origin.
- `g5.git.network.ref-scope` → `repos.protected_branches` and `git_policy` are unchanged → **preserved**.
- base-parity, worker exemption → `worker.rs:1224` → `base_tracks_…` import and lagging-watermark scenarios would fail with `|| state.is_base` → **preserved (runtime)**.
- base-parity, scheduler/pinger/`passive_candidates`/`set_auto_pull` exemptions → `policy.rs:262`, `remote_integrations.rs:109`, `remote_advisory_pinger.rs:234`, `policy.rs:16` → **preserved**. F1 is a runtime proof that Base is enqueued.
- Base-only importer/scheduler/columns/route removed → grep across backend, frontend, g5d/g5d-cli, scripts, tests/e2e → no remaining consumers of `base_auto_pull*`, `/base_auto_pull`, `coordinated` sync result, `protectedRefObservation`, `repo_mirror_refs` → **preserved**.
- Migration 056 → constraint names exist in `001_lean_baseline.sql:5497,8504`. Owner NO FORCE scope covers the read of `workspaces` and the write of `remote_integrations`. DROP-only ALTER inside the scope, `SET CONSTRAINTS ALL IMMEDIATE`, FORCE restored. The populated non-BYPASSRLS test passes, the replay is a no-op, and the test discriminates (rows are invisible without the scope) → **preserved**. A fresh install via 001→057 is consistent.
- `g5.workspace.identity.base` → Publish still refused (`:7219`). `/sync` and `auto_pull` are owner-gated GitOperations. Worker landing has no content guard, so imports land on read-only Base. Resolve with agent is refused in the backend (`merge.rs:1876`) → **preserved in the backend** (UI: F3).
- `g5.security.credentials`/`admission` → the acting user is the current owner. Manual sync is owner-gated and not an explicit-authority trigger. Push stays explicit with owner/generation → **preserved (inspection)**.
- `g5.workspace.remote-ui.status` → no Base special case remains in `remoteTrackingStatus.ts` or the pill → **preserved**.
- `tracking_mode` for unbound Base repositories changed `'tracking'` → `'pinned'` (`repositories.rs:61,141`). Consumers are `WorkspaceTable.isTrackingMode` and mocks. No backend reader requires `'tracking'`, and the new value is truthful now that nothing follows the provider → **acceptable**.
- Credential operation variants for the mirror or Base pull → none remain → **preserved**.
