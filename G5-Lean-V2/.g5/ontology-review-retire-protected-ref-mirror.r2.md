# Independent review 2: retire the protected-ref mirror and track Base like any workspace

Mode: independent, read-only discovery. This is a fresh context with no implementation history. No production, test or ontology edits were made.
Scope: `origin/main..HEAD` (7a131d3c, 50535baf, 9e5eb122) plus one uncommitted working-tree edit to `backend/g5-api/tests/maintenance_integration.rs`.
Inputs: request.md, the pinned design (`.g5/ontology-plan-retire-protected-ref-mirror.md`), the ontology and doc diff, the current text of the governing blocks, and the review and design guides.

## Findings (defects, missing evidence and design questions)

### F1. High: Base has no way to approve a provider rewrite, so any provider force-push strands Base
- **Governing rule:** `g5.workspace.remote.base-parity` covers "pause, divergence and provider-rewrite approval handling". It says Base's "non-fast-forward cases are provider rewrites, which use the explicit rewrite-approval checkpoint without a VM". The design acceptance says "A provider rewrite of Base's tracked branch pauses for the rewrite-approval checkpoint".
- **Path:**
  - `remote_integrations.rs:282-295` (`classify_history`): Base's head always equals its watermark, so any provider rewrite below the watermark classifies as `WatermarkViolation`. The repo pauses with `watermark_violation` and opens a `reset_below_baseline` run.
  - `merge.rs:371`: those runs "require approved rebase publication".
  - `workspace_service.rs:5115-5132` (`approved_rebase_publish`): returns `APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE` for Base.
  - `BranchTrackingPopover.tsx:326,822`: still shows the approval action on Base, and the backend refuses it.
- **Consequence:** After any provider force-push, Base's repository stays paused indefinitely. Retry does not clear `watermark_violation` (`policy.rs:63-81`), Resolve is refused on Base, and push is blocked while paused. Before this change the mirror force-imported rewrites (which was wrong), so Base still followed the provider. Now it stops following it permanently.
- **Fix (design question first):** Decide what "approve a provider rewrite" means on Base. The likely answer is: accept the provider tip, CAS-reset Base's branch, and advance the provider ref and watermark with no push.
  - Simply removing the Base guard is probably wrong. `normalize_reviewed_publication_tip` linearizes a candidate onto the remote tip and publishes it, which could replay the commits the provider removed.
  - Implement the chosen Base variant behind the existing checkpoint UI, and record it in the D block and in provider transport.
- **Regression check:** In a worker test, Base is at C2 with watermark C2 and the provider is force-pushed to C1'.
  1. A background cycle pauses with `watermark_violation` and does not move the head.
  2. The owner's approval lands C1' with the watermark and provider ref at C1' and no provider push.
  3. Without approval, nothing force-imports.

### F2. Medium: the ontology claim "Base cannot gain local commits" is false, and Make Remote on such a Base leaves it stuck
- **Governing rule:** `g5.workspace.remote.base-parity` (text quoted in F1). The design's Risks section tells organizations with pinned Bases to use Make Remote. The request's rationale itself mentions "a pinned Base's merged commits".
- **Path:**
  - `workspace_service.rs:6388-6394` (`merge_workspace`): refuses only destinations that have bindings, so a pinned Base can receive child merges.
  - `workspace_service.rs:4483-4519`: Make Remote on a Base that is ahead or diverged binds it paused `diverged` (`merge_required`).
  - `merge.rs:1876`: refuses Resolve for Base, and `approved_rebase_publish` refuses Base.
  - Push is blocked while the repo is paused.
- **Consequence:** A pinned Base that took merges and is then made remote ends up permanently paused, even when it is only ahead and a push would have reconciled it.
- **Fix:** Correct the D block. Then decide the Base behavior:
  - ahead: bind unpaused so the owner's explicit Push publishes;
  - diverged: refuse Make Remote with fork guidance instead of binding into a pause that cannot be recovered.
- **Regression check:** API tests for Make Remote on a Base that is ahead, and on one that has diverged, asserting a recoverable outcome.

### F3. Medium (merge blocker): rustfmt check fails
- **Path:** `rustfmt --check` reports diffs in 8 changed files that were clean on `origin/main`:
  - `doc_service.rs` (`}],` indentation)
  - `remote_integrations.rs` (`pub(crate) use policy::{…}`)
  - `workspace_service.rs`
  - `routes/workspaces.rs`
  - `g5-scm/src/lib.rs`
  - the `remote_repo_tracking_integration.rs`, `workspace_repository_model.rs` and `migration_056_*.rs` tests
- **Consequence:** CI fails at `cargo fmt --all -- --check` (`infra/ci/cloudbuild-ci-rust.yaml:782`).
- **Fix:** Run `cargo fmt --all`.
- **Regression check:** `cargo fmt --all -- --check`.

### F4. Medium (merge blocker): the committed maintenance test fails at HEAD
- **Path:** HEAD's `maintenance_integration.rs:798-801` asserts 14 JoinHandles. `maintenance/mod.rs` now spawns 12 (13 `handles.push` calls, one gated by the outbox config).
- **Consequence:** The committed branch fails `test_maintenance_runner_spawns_and_cancels`. The fix (12) exists only as an uncommitted working-tree edit.
- **Fix:** Commit that edit.
- **Regression check:** The same test.

### F5. Medium (missing evidence): the tests do not cover the removed Base exemptions
`base_tracks_its_provider_through_the_ordinary_worker` enqueues directly (`fx.enqueue`). It catches the removed worker exemption and the removed `set_auto_pull` refusal. It would still pass if any of the following came back, and other outcomes have no test:
- `policy.rs::enqueue_background_inbound_rows` `AND NOT w.is_base` (scheduler).
- `remote_integrations.rs::passive_candidates` `AND NOT w.is_base` and `remote_advisory_pinger.rs:235` `|| current.is_base` (advisory).
- The `initial_auto_pull_sql` default in `bind_policy` (Make Remote) and in both `convergence.rs` inserts. Only `bind_published` is tested.
- The acceptance outcome "after Base ownership transfer, fetches use the new owner's credentials". The fixture uses `provider = NULL` / `GitAuth::None`.
- The QA case reaching the status pill. Only the watermark is asserted, not the repository summary or "Tracking healthy".
- `remote_actor_scope_check_requires_current_membership` was renamed, but it only tests `PermissionService`. Its message ("remote integration must not keep using SCM tokens…") claims behavior the worker does not perform. It should be deleted, or rewritten to exercise the worker's real credential path.

**Fix:**
- A scheduler-sweep test and a pinger test on a Base binding.
- A Make Remote route test on Base asserting `auto_pull_enabled = true`, plus a non-Base control.
- A transfer test using a stub credential resolver that records the acting user.

### F6. Low–Medium (design deviation, needs a decision): the QA reset leaves auto-pull on for Base
- **Governing:** The design's step 5 and Risks say "The QA reset should hold auto-pull off until App verification".
- **Path:** `scripts/reset-qa.py:480` still seeds `true`. `scripts/reset-qa.md:100-103,140-142` now documents an expected `auth_required` pause followed by Retry.
- **Consequence:** This defensible choice matches the user's "on by default". It contradicts the saved design, though, and it is not recorded as an amendment.
- **Fix:** Either amend the design (Risks and step 5) or seed `false` in the reset and enable auto-pull through the ordinary control after App verification.

### F7. Low (design inaccuracy): the acceptance outcome for auto-pull off is not ordinary behavior
- The design says that with auto-pull off "the cycle only fetches and records `fetched`; the pill reads … behind once the advisory observes the tip".
- For every workspace (`worker.rs:1224-1230`, `passive_candidates`), background cycles and the advisory pinger skip bindings with auto-pull off (and push mode manual). Base with auto-pull off therefore never passively reads "behind". The test asserts no cycle at all.
- This matches parity. The design acceptance text should be corrected, and any change to the advisory is a separate global decision.

### F8. Low: the UI offers Resolve with agent on a diverged Base
- **Governing:** `base-parity`: "Resolve with agent … is therefore unavailable on Base".
- **Path:** `BranchTrackingPopover.tsx:504,602` renders `RemoteConflictAgentLauncher` with no Base gate. The backend refuses it (`merge.rs:1876`).
- **Fix:** Hide it on Base and show fork guidance instead.
- **Check:** A popover unit test with `isBase` and a `diverged` run.

### F9. Low: stale text left behind by the retired paths
- `workspace_service.rs:4321-4331`: `BASE_REMOTE_BIND_REQUIRES_DEFAULT_BRANCH` message says "…for coordinated pull".
- `routes/workspaces.rs` `set_branch_repo_auto_pull` doc comment: "rejects local/pinned and Base-managed children".
- `tests/api_ontology.rs:403-405`: says a Base remote pull "publishes them with `g5d push`".
- `RemoteTrackingActions.tsx` push-mode hint: "pushes save to G5", shown on Base.
- `docs/data-model-cleanup-plan.md:327-333`: "Base admission checks the actor's current permission and, for scheduled pulls…", "Base callers supply captured repositories…". This is undated, current architecture text in the doc the ontology names as owner of the tracking mechanics.
- `docs/data-model-cleanup-plan.md:406`: lists "Base pulls … protected-ref refresh" as current credential users.
- `.g5/ontology/software_architecture/workspace.g5.md:148` footer: "failed mirror fetches can appear fresh". That reader no longer exists.

### F10. Low (process / metadata): ontology footers and design assumptions are stale
- The footers still say "Not implemented" and cite removed code:
  - `durability.g5.md:368` (`g5.git.mirror`)
  - `git_governance.g5.md:140` (the `fetch_protected_ref` gap)
  - `workspaces.g5.md:569,804-813` (`g5.story.base-follows-provider`, `base-parity`, `base-auto-pull-default`)
- The saved design's two open assumptions (consumers of the default-branch ref, and Base Details control visibility) were not resolved in the design. The F6 deviation is not recorded there either.
- **Proposed footers:**
  - `g5.git.mirror`: Implemented. The mirror loop, `fetch_protected_ref` and `repo_mirror_refs` are removed (057). Evidence: the schema guard in `workspace_repository_model.rs`.
  - `base-parity`: Partial. Gaps are F1, F2, F5 and F8.
  - `base-auto-pull-default`: Implemented via `policy.rs::initial_auto_pull_sql`, used at all 4 insert sites, plus migration 056. Tests cover only `bind_published`.
  - `git_governance`: remove the `fetch_protected_ref` gap.
- **Assumption resolutions** (from this inspection):
  - (a) Only Base-scoped `CASE WHEN is_base THEN default_branch` readers and `initialize_local_workspace_repo` (symbolic HEAD) name `refs/heads/<default_branch>`. No reader treats it as an independent mirror.
  - (b) WorkspaceDetailsPanel's Base `readOnly` gates only description and mutation-policy controls. BranchTrackingInline, RemoteTrackingActions and BranchAutoPullControl render for Base.

### F11. Low (unlisted behavior change, confirm intent): unbound Base repositories now report `pinned`
- `repositories.rs:61,141` now report unbound Base members as `pinned` instead of `tracking`.
- Effects:
  - the `WorkspaceTable.tsx:205-210` tracking badge disappears for a Base with no bindings;
  - Base's G5-hosted docs repository now reads "Local only — not published" and offers Make Remote (`WorkspaceDetailsPanel.tsx:577-613`).
- This is consistent with parity, but neither the design nor the tests mention it.

### Observations (no defect claimed)
- **Rollout:**
  - 056 and 057 drop columns and a table that pre-upgrade pods still read during the Helm pre-upgrade window:
    - the `get_workspace` SELECT of `base_auto_pull_enabled`;
    - the old mirror and Base scheduler loops.
  - Expect transient 500s or error logs until the rollout finishes. This matches the precedent set by 045.
- **PR-merge import into a Base target:** `workspace_pull_request_provider_lifecycle.rs:~300-400` imports a provider merge into Base through remote-integration convergence, which is consistent with `g5.git.mirror`. It uses the importing actor's credentials rather than Base's owner, and it may initialize a Base binding through `convergence.rs:296` with auto-pull on.
  - Design question: does "acting under Base's current owner" cover this path?
- **Leftover refs:** Old `refs/g5/mirror-fetch/*` refs may remain in existing repositories. They are harmless, but there is no cleanup.

## Checks with no finding (`commitment → path → check → result`)
- **`g5.git.mirror` (only remote tracking writes workspace branches)**
  - Path: `maintenance/mod.rs` (mirror and Base scheduler unregistered), `protected_ref_mirror.rs` and `base_auto_pull_scheduler.rs` deleted, `g5-scm/src/repo.rs::fetch_protected_ref` removed.
  - Check: `git grep` for `repo_mirror_refs|fetch_protected_ref|mirror-fetch|base_auto_pull|prepare_base_pull|BasePullAdmission` in code.
  - Result: the remaining hits are only migrations, tests and history docs. No remaining provider writer outside `remote_integrations` (worker, Make Remote, PR-merge convergence).
- **`g5.git.network.ref-scope`**
  - Path: `repos.protected_branches` and `git_policy::protected_branches_for_repo` are retained.
  - Check: inspection.
  - Result: preserved.
- **`base-parity` worker/scheduler/advisory exemptions removed**
  - Path: `worker.rs:1224-1230`, `policy.rs:257-264`, `remote_integrations.rs:106-110`, `remote_advisory_pinger.rs:228-238`.
  - Check: inspection.
  - Result: removed. Discriminating runtime coverage exists only for the worker (F5).
- **`base-parity` per-repository Pull**
  - Path: `workspace_service.rs::sync_workspace` → `enqueue_branch_integration_for_workspace` (`branch_integration_queue.rs:205-238`). No `is_base` filter.
  - Check: inspection.
  - Result: Base is queued per repository, and one repository's pause does not block another.
- **`base-parity` owner controls**
  - Path: `policy.rs::set_auto_pull` (Base refusal removed), routes `/repos/:repoId/auto_pull`, `/sync`, `/push_to_remote`, `/push_mode` (owner-gated, `GitOperation`).
  - Check: inspection plus the test calling `set_auto_pull` on Base.
  - Result: available to Base's owner.
- **`base-parity` acting user**
  - Path: `remote_integrations.rs:66-77` `POLICY_QUERY` uses `w.owner_user_id AS acting_user_id`.
  - Check: inspection.
  - Result: current owner. Transfer is not runtime-tested (F5).
- **`base-parity` Resolve unavailable**
  - Path: `merge.rs:1876`.
  - Check: inspection.
  - Result: the backend refuses. The UI gap is F8.
- **`base-auto-pull-default`**
  - Path: `policy.rs:111-116`, used at `policy.rs:126,159` and `convergence.rs:301,445`. All 4 production binding inserts were found by a case-insensitive grep. `reset-qa.py` seeds `true`. No fork-time binding copy exists.
  - Check: inspection.
  - Result: on for Base and off otherwise. Only `bind_published` is runtime-tested.
- **Migration 056 (RLS owner form)**
  - Path: straight-line NO FORCE on `workspaces` and `remote_integrations` → UPDATE … FROM → `SET CONSTRAINTS ALL IMMEDIATE` → FORCE `remote_integrations` → ALTER `workspaces` DROP CONSTRAINT/COLUMN list inside the open NO FORCE scope → FORCE.
  - Check: compared with the `docs/database-migrations.md` rule 2 accepted forms. The constraint names match `001_lean_baseline.sql:5497,8504`. No triggers exist on `remote_integrations`. No views or functions reference the dropped columns.
  - Result: conforms. A populated non-BYPASSRLS fixture with a replay exists (`migration_056_base_remote_tracking_parity.rs`). Its runtime result was not observed by me (see the runtime evidence section).
- **Migration 057**
  - Path: `DROP TABLE public.repo_mirror_refs`. Its policies and grants drop with the table, and no functions reference it.
  - Check: inspection.
  - Result: conforms. The populated `repo_mirror_refs` case is not seeded in the fixture (minor gap).
- **Fresh install**
  - 001 creates the objects that 056 and 057 drop.
  - Result: replay is fine by inspection; not executed by me.
- **Checksums**
  - Path: `checksums.sha384`, with two appended entries only.
  - Result: conforms.
- **Removed DTO and API fields**
  - Fields: `baseAutoPullEnabled`, `PUT /base_auto_pull`, `SyncResultDTO.coordinated`, `protected_ref_observation`.
  - Check: `git grep` over frontend, scripts, tests, tools and infra (no `G5_BASE_AUTO_PULL_INTERVAL_SECS` in infra).
  - Result: no remaining consumers. `PushToRemote`'s `coordinated` is unaffected.
- **`g5.workspace.identity.base` (content read-only, no VM)**
  - Path: the write-file guard `routes/workspaces.rs:3343-3350`, the VM guards, Publish refused on Base (`workspace_service.rs:7219`).
  - Check: inspection.
  - Result: preserved.
- **`g5.workspace.base-ui.*`**
  - Path: `WorkspaceDetailsPanel.tsx:181-220` (description and policy only), `WorkspaceVmStatusStrip.tsx`.
  - Check: inspection.
  - Result: unchanged, and the tracking controls are visible.
- **H014 per-repository import**
  - Result: the coordinated all-repository cut is fully removed (`pull_remote_repository_set`, `prepare_base_pull`, `Admission::Base`).
- **Frontend**
  - `npx vitest run` on BranchAutoPullControl, RemoteTrackingActions, WorkspaceStatusBand and BranchActionsMenu: 103/103 passed. `npx tsc --noEmit`: clean.

## Runtime evidence and limits
- **Caller's run:** The caller's `cargo test -p g5-api --tests` log (`scratchpad/g5-api-tests.log`, using the working tree) exited 101. `api_workspace_pull_requests` had 13 failures: create PR returned 500 at `:1106`. Cargo stopped at that binary, so later binaries did not run in that log. These include `remote_repo_tracking_integration`, `migration_056_*`, `maintenance_integration` and `workspace_repository_model`.
- **My rerun:** My targeted rerun of one of those failures (`scratchpad/review2-pr-test.log`) could not reach the code. Postgres refused with "53300 too many clients" because other test runs were going at the same time. The cause of the 500s is therefore unresolved. I found no diff path touching create PR, and the 500s may be connection exhaustion, but this is unverified.
- **Not executed by me:** the Base worker test, the default-binding test, the migration fixture, a fresh-install replay, and the rustfmt fix. Every central Base runtime outcome remains unverified in this review.
