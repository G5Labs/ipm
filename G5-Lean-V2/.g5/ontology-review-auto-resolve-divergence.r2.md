# Ontology review: automatic divergence resolution (reviewer 2)

**Mode:** independent, read-only discovery review, reviewer 2 of 3. No production, test or ontology file was edited, and no work was delegated. Inputs: the request ([chat intent](chat_intent/issue-240/auto-resolve-divergence.md)), the pinned [design](ontology-plan-auto-resolve-divergence.md) starting from `Ontology changes and governance`, the working-tree ontology diff, the governing ontology blocks, `docs/data-model-cleanup-plan.md` (sections "Provider transport and publication" and "Reconciliation in the workspace"), and the uncommitted implementation. Disclosure: the pinned design already contains the author's "Implementation reconciliation" (A1–A5 outcomes). I read it as part of the governing input, so findings were not formed before seeing the author's explanation.

**Scope:** `remote_integrations/merge.rs`, `remote_integrations/worker.rs`, `routes/workspaces.rs` (complete route), migration `043_remote_sync_resolved_status.sql` plus checksum, `branch_sync_run.rs`, `g5d-cli/src/remote.rs`, `frontend/src/lib/types.ts`, `BranchTrackingPopover.tsx` and its test, and the new tests in `remote_repo_tracking_integration.rs`. The unrelated issue-240 UI work, the Changes-read coalescer and the session-link fix are out of scope except where they interact.

**Recorded diff check:** the `.g5/ontology/features/workspaces.g5.md` diff contains the three planned blocks, with metadata now rewritten from "Unassessed" to "Implemented". The `core_user_stories` diff also adds the issue-240 `see-remote-tracking-state` link, which is part of the base. No existing commitment changed meaning. This matches the handoff.

## Commitment ledger

Each entry reads: commitment → actual path → counterexample/check → result or gap.

- **`g5.workspace.remote.divergence-auto-complete`: proof runs without a human when an accepted update moves the paused head.**
  - Path: `worker.rs:232` calls `auto_complete_proven_divergence` (`worker.rs:457`) between pickup and claim. The hook selects an open `job_rebase_preflight` run whose integration is paused `diverged` (`worker.rs:480-490`), then calls `prepare_external_remote_tracking_merge_completion` and `apply_…` with `CompletionRequester::Automatic`.
  - Check: native `git push` enqueues `commit` without a pause filter (`routes/git.rs:745` → `policy.rs::enqueue_after_push`), so A1 holds by inspection. Runtime: `landed_resolution_completes_divergence_automatically_and_follows_auto_push_policy` passes.
  - Result: **holds for the externally resolved (no resolution task) variant.**
  - Gaps:
    - The agent variant is unexercised: it uses `source_heads` plus the linear-descendant proof (F3).
    - A transient failure consumes the wake (F2).
    - A racing owner gets a 409 (F1).
- **Same proof as manual confirmation; failing head stays open.**
  - Path: one shared prepare/apply. The `requester` only changes authority, `confirmed_by`, the push upgrade and the audit.
  - Check: `unproven_update_leaves_the_divergence_run_open` passes. It would fail if the ancestry check were skipped.
  - Result: **holds** (`g5.design.reuse` satisfied; no second ancestry rule).
  - Gap: an agent resolution containing a merge commit is untested (F3).
- **Provider-rewrite checkpoints never auto-complete.**
  - Path: three layers: the worker SELECT `phase = 'job_rebase_preflight'`, the prepare `reset_below_baseline` refusal (pre-existing), and the new Automatic phase guard (`merge.rs:356-371`).
  - Check: `provider_rewrite_checkpoint_never_completes_automatically` passes. Its fixture keeps the pause `diverged`, while real checkpoints pause `watermark_violation`, so only the SELECT filter is exercised.
  - Result: **holds by inspection.** The test is non-isolating (F6).
- **`g5.workspace.remote.divergence-auto-publish`: no implicit publication outside push policy.**
  - Path: Automatic enqueues no `push` (`merge.rs:704-760`), and the same wake continues as an ordinary cycle.
  - Check (auto mode): the test asserts no queue row remains, the provider is unchanged, and `push_failed` "requires a GitHub App" is recorded.
  - Check (manual mode): only the provider tip is asserted, and that would be the same without an App in auto mode (F5).
  - Result: **holds by inspection. Auto mode is discriminated; manual mode is weak (F5).** A successful App-backed fast-forward is unverified.
- **Manual confirmation keeps its meaning (explicit publication by the confirming owner).**
  - Path: the `Owner` arm keeps the push-authority fence, `confirmed_by` and the `push` enqueue.
  - Check: existing manual-completion tests still pass (`pause_only_resolution_*`).
  - Result: **holds when the owner wins.**
  - Gap: a late or losing confirmation silently drops the owner's publication request (F1, F7).
- **`g5.governance.publish.actor`: no fabricated confirming human.**
  - Path: `confirmed_by = NULL`; the audit row has status `resolved` and the wake's trigger.
  - Check: the test asserts `("completed", None)` and the resolved row.
  - Result: **holds.** The audit `acting_user_id` is `workspace.owner_user_id` (`merge.rs:737`). The column is NOT NULL, and every other worker audit row also uses the owner (`remote_integrations.rs:71`), so this follows the existing convention rather than fabricating a confirmer (nit N2).
- **`g5.security.owner` / Git write boundary: no added write authority.**
  - Path: the completion only moves DB state. `apply_ref_commits` is an identity CAS (old == new).
  - Result: **holds.**
- **`g5.architecture.recovery`: replay safety, exactly one completion.**
  - Path: locked status guard plus `UPDATE … AND status IN (…)` with `rows_affected() == 1`.
  - Check: a late owner POST returns 200 `completed:false` (tested). A worker crash after commit re-picks the row, the SELECT then finds no paused run, and the wake runs as an ordinary cycle (inspection).
  - Result: **exactly one completion holds.**
  - Gap: "no error visible to the user" fails for a simultaneous owner confirmation (F1).
- **`g5.architecture.recovery.progress`: a proven but transiently failed completion stays progressing.**
  - Path: the hook logs `warn`, the claim sees the pause, and `PreparedCycle::Paused` deletes the queue row (`worker.rs:1377`).
  - Result: **gap (F2).** Only the manual control or a later push to the same repository recovers it.
- **Tenant scoping and RLS.**
  - Path: maintenance scope for the draining org. The SELECT filters `run.org_id = picked.key.org_id` and joins integration on the full composite key. The audit `INSERT … SELECT FROM workspaces` runs in the same scope.
  - Result: **holds by inspection.**
- **Transaction boundaries.**
  - Prepare runs in its own scoped transaction, rolled back by `drop`.
  - Apply runs in one transaction: run and integration `FOR UPDATE`, run update, pause clear and audit insert, then commit.
  - The hook holds no connection across the claim, and lock order matches the owner route.
  - Result: **holds.**
- **Worker lease and claim.**
  - The hook runs under the queue-row lock (`locked_by`, TTL 600 s) and before the remote-pair lease. It writes no provider state, so the pair lease is not needed.
  - A newer push that lands mid-hook changes `requested_at`; finalization then sees `StaleState` and defers, so the newer wake survives.
  - Result: **holds.** The overhead is an extra capability mint plus SELECT on every pickup (N3).
- **`g5.workspace.remote-ui.actions` / A5: the UI reflects completion.**
  - Path: the `resolved` label and success color in `BranchTrackingPopover.tsx`; the pill derivation keys off the pause reason, not the latest run status.
  - Check: the popover unit test renders the label. Apply emits no WS event, so the UI relies on polling.
  - Result: **label holds.** The no-reload refresh is not browser-verified.
- **Migration safety.**
  - The new check set is a superset of 040's, so existing rows validate, and the checksum matches (`shasum -a 384`).
  - Result: **safe on data. Number collision with main (F4); lock pattern differs from main (N4).**
- **CLI and types.**
  - `is_terminal_sync_run_status` includes `resolved`, and the type comment is updated. `policy::retry` treats `resolved` as non-retryable, which is correct.
  - Result: **holds.**

## Findings

### F1: A simultaneous owner "Mark resolved" fails with 409 (defect, low probability)

- **Governing:** design acceptance outcome "A manual confirmation racing an automatic completion yields exactly one completion and no error visible to the user"; `g5.architecture.recovery`.
- **Where:** `merge.rs:605-617`.
- **What happens:** the owner's prepare returns `Ready`, then the worker's apply commits. The owner's apply then locks the run, sees `status = 'completed'` and returns `Conflict("… changed after completion preparation")`, which the route surfaces as 409. Only a prepare that already observes the terminal state returns the idempotent `AlreadyTerminal`.
- **Metadata:** the `divergence-auto-complete` footer says such a race "resolves through the existing row locks". The locks prevent a double completion, but the owner still sees an error.
- **Fix:** under the lock, treat an already `completed` run with a non-null `completion_advance_id` as an idempotent replay (`completed:false`) rather than a Conflict. Keep the Conflict for other changes (different heads or task). Then correct the footer.
- **Regression check:** a service-level test that prepares `Ready` for both `Owner` and `Automatic`, applies Automatic, then applies Owner. Assert `Ok(completed:false)`, a single `completed` row with `confirmed_by IS NULL`, and exactly one `resolved` audit row.

### F2: A proven resolution whose apply fails transiently is not retried (defect, recovery)

- **Governing:** `g5.architecture.recovery.progress`; story `g5.story.resume-after-divergence` ("resumes without a separate confirmation"); design step 2 ("the next accepted update tries again").
- **Where:** `worker.rs:540-551` together with `worker.rs:1359-1378`.
- **What happens:** when prepare is `Ready` but apply fails, the hook only logs. Possible causes:
  - a DB error;
  - the snapshot head moved;
  - more commonly, `apply_ref_commits` (`merge.rs:640-660`) CAS-checks every member ref of the workspace, so a concurrent push to a sibling repository makes it fail with "cut changed".

  The same wake is then claimed as `Paused` and its queue row is deleted. The sibling push only wakes the sibling, so this repository stays paused despite a proven resolution until the owner clicks Mark resolved or pushes again.
- **Fix:** when prepare returned `Ready` and apply failed with a Conflict or DB error, re-arm the queue row (`defer_queue_row_for_pair_lease`-style short `locked_until`, bounded by `attempts`) and skip the claim. That retries the same wake without hot-looping on permanent refusals, which stay `Err` from prepare and keep today's behavior.
- **Regression check:** a test that forces one apply failure (for example a concurrent sibling-member ref move between prepare and apply, or an injected Conflict). Assert that a later drain completes the run without a new enqueue.

### F3: The primary user scenario, an agent-resolved run, is not exercised (missing evidence)

- **Governing:** `divergence-auto-complete` ("linearly without merge commits when an agent resolved it"); design acceptance outcomes 1 and 3 ("agent-style linear rebase … pushed with its lease"; "an agent resolution with a merge commit … leaves the run waiting_for_human").
- **Where:** `remote_repo_tracking_integration.rs:2211-2460`. All four tests use `open_externally_resolvable_divergence`, where agent start fails and `resolution_task_id` is NULL. That variant proves against `pre_start_source_heads` with no linear check.
- **Untested:**
  - the `resolution_task_id` branch (proof against `source_heads`, `is_linear_descendant_of`);
  - the merge-commit refusal;
  - the real native `git push --force-with-lease` acceptance path that enqueues the wake (tests rebase in place and call `fx.enqueue("commit")`).

  The user's request specifically observed the resolution agent's force-push. No existing test covers the linear proof, even for manual completion.
- **Fix:** add tests with a run carrying a `resolution_task_id` and its `source_heads`, covering:
  - a linear rebase that completes automatically;
  - a merge-commit resolution that stays `waiting_for_human` and paused;
  - ideally, a resolution pushed through the git route rather than a direct enqueue.
- **Regression check:** each test must fail if `is_linear_descendant_of` or the `source_heads` selection is removed.

### F4: Migration number collides with main (defect at merge)

- **Where:** `backend/g5-api/src/db/migrations/043_remote_sync_resolved_status.sql`.
- **What happens:** `origin/main` already has `043_agent_attempt_limit_reason.sql`, `044_validate_…` and `045_drop_review_document_targets.sql`. A rebase or merge gives two `043_` files and a conflicting `checksums.sha384` line.
- **Fix:** renumber to the next free number (046 at review time) and regenerate the checksum entry.
- **Regression check:** the migration runner and checksum guard on the rebased branch; the integration suite on a fresh database.

### F5: The manual-push-mode test does not discriminate the policy (missing evidence)

- **Governing:** `divergence-auto-publish`.
- **Where:** `remote_repo_tracking_integration.rs:2366-2392`.
- **What happens:** the test asserts only that the provider tip is unchanged. The fixture has no GitHub App, so auto mode also leaves the provider unchanged. The test would still pass if manual mode were ignored and an implicit push were attempted.
- **Fix:** also assert that manual mode records no push attempt (no `push_failed`/`pushed` row after `resolved`) and that no queue row remains. Ideally, add an App-backed auto-mode case asserting the provider fast-forwards to the resolved tip (acceptance outcome 1, currently unverified).
- **Regression check:** flip the fixture's push mode to `auto` and confirm the manual-mode assertions fail.

### F6: The provider-rewrite test does not exercise the merge-level refusal (missing evidence, low)

- **Where:** `remote_repo_tracking_integration.rs:2424-2460`.
- **What happens:** the test rewrites `phase` to `reset_below_baseline` but leaves the pause `diverged`. The worker SELECT's phase filter returns no run, so neither the pre-existing prepare refusal nor the new Automatic guard (`merge.rs:356-371`) runs. The outcome is correct, but the test does not separate the layers or model a real checkpoint (paused `watermark_violation`).
- **Fix:** add a service-level call of `prepare_…(…, CompletionRequester::Automatic {..})` on a `reset_below_baseline` run that asserts a Conflict. Or build the checkpoint through the real watermark-violation path.
- **Regression check:** the service test fails if the prepare refusals are removed.

### F7: A late manual confirmation drops its publication request (design question)

- **Governing:** `divergence-auto-publish` ("A manual confirmation keeps its current meaning as an explicit publication request by the confirming owner").
- **What happens:** automatic completion usually lands seconds after the push. An owner whose UI still shows Mark resolved clicks it and gets 200 `completed:false`, and nothing is published (`AlreadyTerminal` performs no enqueue). In manual push mode, or in auto mode without an App, the owner's explicit publication intent is silently lost.
- **Related regression:** in auto mode without a GitHub App, Mark resolved previously published through the owner's connection. Now the run auto-completes, the cycle records `push_failed` "requires a GitHub App", and the owner must find and click Push. The plan records this under A3, but the human approved the design before that consequence was known.
- **Decision for the human:** should a confirmation that lands on an automatically completed run still enqueue the owner's explicit `push` (it carries the owner's push authority)? Or is the no-op acceptable?
- **If yes:** in the `AlreadyTerminal` / F1 replay branch for `Owner`, when `confirmed_by IS NULL` (automatically completed) and the head still equals the completion head, enqueue the authorized push.
- **Regression check:** auto-complete in manual mode, then POST complete as the owner, then drain. Assert the provider is at the resolved tip.

## Nits

- **N1. Requester passed twice.** `CompletionRequester` goes to prepare and apply separately, which creates an impossible `(Owner, None)` Internal arm (`merge.rs:756-760`). Carrying the requester inside `Prepared::Ready` would remove the mismatch case.
- **N2. Audit actor.** The `resolved` audit sets `acting_user_id = workspace.owner_user_id` (`merge.rs:737`). This is consistent with other worker rows, but a NULL owner would violate NOT NULL and roll back the whole completion; the hook then only logs `warn`. Also, `rows_affected` is not checked on the `INSERT … SELECT`. Consider asserting one row.
- **N3. Per-pickup overhead.** The hook mints a maintenance capability and runs a SELECT for every non-background pickup, including unpaused members. Gating on the claim's loaded state, or folding the check into the pickup query, would avoid this.
- **N4. Lock pattern.** The migration adds the CHECK in one statement, which takes ACCESS EXCLUSIVE and validates by full scan of the append-only audit table. Main's convention since 043/044 is `ADD CONSTRAINT … NOT VALID` followed by a separate `VALIDATE CONSTRAINT` migration.
- **N5. Pause not re-checked at apply.** Apply does not re-check, under lock, that an Automatic completion still targets a `job_rebase_preflight` run paused `diverged`; it clears any pause reason. In practice no path changes these while a run is open. Adding `locked.phase == "job_rebase_preflight" && integration.sync_paused_reason = 'diverged'` for Automatic would make the precondition atomic.

## Proposed metadata and design edits (not applied)

- **`g5.workspace.remote.divergence-auto-complete` footer:** replace "a simultaneous one resolves through the existing row locks, but that race is not separately tested" with "a simultaneous owner confirmation can receive 409 (F1); a transiently failed apply is not retried (F2); the agent-resolution linear proof is untested (F3)".
- **`g5.workspace.remote.divergence-auto-publish` footer:** note that the manual-mode test does not discriminate the mode in an App-less fixture (F5).
- **Design `Ontology changes and governance`:** add `g5.architecture.recovery.progress` as a governing reference for F2. Record the F7 decision once the human answers.

## Checks run

- `TEST_DATABASE_URL=… cargo test -p g5-api --test remote_repo_tracking_integration -- landed_resolution unproven_update_leaves provider_rewrite_checkpoint_never_completes pause_only_resolution`: **6 passed, 0 failed**.
- Full regression suites: `remote_repo_tracking_integration` 37/37, `remote_tracking_background_loop` 20/20 and `remote_tracking_merge_active` 1/1 passed.
- `cargo clippy -p g5-api-core -p g5-api-routes -p g5-api-data -p g5d-cli --all-targets`: no warnings or errors.
- `shasum -a 384` of the migration matches `checksums.sha384`.
- `git ls-tree origin/main` on the migrations directory shows the 043–045 collision (F4).
- Static inspection: queue precedence (`branch_integration_queue.rs`), native git push enqueue (`routes/git.rs`), pickup, claim and paused finalization (`worker.rs`), and retry policy (`policy.rs`).

## Unverified areas

- Agent-resolved runs: the linear proof, the merge-commit refusal, and the real force-with-lease push path (F3).
- A successful App-backed implicit fast-forward in auto mode.
- The owner/worker simultaneous race (F1) and transient apply failure (F2), neither executed.
- UI refresh without reload after automatic completion (A5): no WS event is emitted and nothing was browser-checked.
- Frontend unit tests were not run by this reviewer.
