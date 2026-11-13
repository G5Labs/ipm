# Ontology review: automatic divergence resolution (reviewer 1)

**Mode:** independent, read-only discovery, reviewer 1 of 3. I did not implement this change and had no implementation-session history. I made no production, test or ontology edits; this file is the only one I wrote.

**Inputs:** the request ([chat intent](chat_intent/issue-240/auto-resolve-divergence.md)), the pinned design [ontology-plan-auto-resolve-divergence.md](ontology-plan-auto-resolve-divergence.md), and the changed blocks and their metadata in `ontology/features/workspaces.g5.md`. I also read the governing sections in `docs/data-model-cleanup-plan.md` and the uncommitted working-tree diff.

**Disclosure:** the pinned design already contains an "Implementation reconciliation" section, and the block footers already contain the author's claims. Both were part of the supplied design, so I read them before drawing conclusions. I verified every claim below against source or tests myself.

**Scope:** `merge.rs` (CompletionRequester; prepare/apply), `worker.rs` (`auto_complete_proven_divergence` and its call site), the complete route, migration 043 and its checksum, `branch_sync_run.rs`, `g5d-cli/src/remote.rs`, the frontend `resolved` label, and the new tests. I left the unrelated issue-240 UI work, the Changes-read coalescer and the session-link fix out of scope.

## Ontology diff check

The diff of `workspaces.g5.md` and `core_user_stories.g5.md` matches the recorded handoff diff:
- the new story `g5.story.resume-after-divergence`;
- the decisions `g5.workspace.remote.divergence-auto-complete` and `g5.workspace.remote.divergence-auto-publish`;
- the story index entry.

The footers have since moved from "Unassessed" to "Implemented", with locations and limits. No existing commitment changed meaning.

## Commitment ledger

| Commitment | Actual path | Counterexample / check | Result or gap |
|---|---|---|---|
| `divergence-auto-complete`: an accepted update past the captured tip runs the same proof | `worker.rs:232` calls `auto_complete_proven_divergence` (`worker.rs:457`) before the claim. It selects an open `job_rebase_preflight` run while the member is paused `diverged`, then calls the shared `prepare_/apply_external_remote_tracking_merge_completion` | Rebased head descending from the captured provider tip → run completes, pause clears (test `landed_resolution_completes_…`, runtime pass). Head not descending → run stays open (test `unproven_update_…`, runtime pass) | **Holds** for runs with no resolution task. The agent-linear branch (`merge.rs:455`, used when `resolution_task_id` is set) has no test through the automatic path (F5) |
| Same proof as the manual path, no second ancestry rule (`g5.design.reuse`) | One `prepare_external_remote_tracking_merge_completion`; `Automatic` only skips push-authority capture and adds a phase guard | Inspection | **Holds** |
| A provider-rewrite checkpoint never auto-completes | Worker SQL filter `phase='job_rebase_preflight'`; the existing `reset_below_baseline` refusal in prepare (`merge.rs:~352`) and apply (`merge.rs:605`); the new Automatic phase guard (`merge.rs:362-369`) | Test `provider_rewrite_checkpoint_never_completes_automatically` passes, but it fails only if *all three* guards are removed. The fixture's `diverged` pause is unrealistic, because real checkpoints pause as `watermark_violation` | **Holds** (defence in depth). Nit F8: apply does not recheck the Automatic phase or the `diverged` pause under its lock |
| `divergence-auto-publish`: automatic completion enqueues no push | `merge.rs` Automatic arm writes only an audit row; `Owner` keeps `enqueue_authorized_branch_integration(PUSH)` | Manual mode: test asserts the provider tip does not move. `drain_one_cycle` drains until the queue is empty, so a stray push enqueue would publish and fail the test. Auto mode: test asserts an empty queue and `push_failed` with "requires a GitHub App" | **Holds** in both modes |
| Manual confirmation keeps its meaning as an explicit publication request | Owner path unchanged | A late Mark resolved after an automatic completion returns `AlreadyTerminal` → `completed:false`, and nothing is published (test asserts the no-op) | **Design question F2** |
| `g5.governance.publish.actor`: no fabricated confirming human | `confirmed_by` bound to `None` for Automatic (`merge.rs` UPDATE) | Test asserts `("completed", None)` | **Holds**. Note: the `resolved` audit row's NOT NULL `acting_user_id` uses `workspaces.owner_user_id` (`merge.rs:731`), the same convention as other worker audit rows. It is attribution of authority, not a confirmation (F7) |
| `g5.security.owner` / Git write boundary: no added write authority | Automatic changes lifecycle state only; no refs are written (`apply_ref_commits` is an expected=new CAS no-op) | Inspection | **Holds** |
| `g5.architecture.recovery`: replay-safe, exactly one completion when manual and automatic race | `FOR UPDATE OF run, integration` plus the status guard in apply; `AlreadyTerminal` when prepare runs after completion | Replay after completion → `completed:false` (tested). Simultaneous race: exactly one completion by inspection, but a losing owner whose prepare returned `Ready` gets a **409** | **Gap F1** (user-visible error the design forbids); race untested (F5) |
| Queue/lease safety of the phased drain | Hook runs after pickup (queue row locked by `owner_token`), before the remote-pair lease. Every error is swallowed and logged | Inspection: the claim reloads state after completion; a stale cycle is fenced by `validate_cycle_claim_for_finalization`, whose state includes `sync_paused_reason`; the hook cannot stall the row | **Holds**. Efficiency nit F9 |
| Tenant scoping / RLS | All queries use `scoped_db_for_maintenance_org(org_id)`. The run lookup binds `picked.key.org_id`; the audit insert uses `locked.org_id` | Inspection; runtime tests use the real scoped role | **Holds** |
| Transaction boundaries | Prepare runs in its own scoped tx, dropped (rolled back). Apply runs in a fresh scoped tx and commits | Inspection | **Holds** (same shape as the route) |
| Migration 043 is safe on existing data | DROP/ADD of the status CHECK with a superset value list; checksum appended | All existing statuses remain allowed; same pattern as 040; `remote_tracking_migration` suite passes | **Holds** |
| A1: the wake reaches the worker while paused | `routes/git.rs:745` → `enqueue_after_push` (COMMIT) runs regardless of pause; queue precedence lets commit replace `background_inbound` | Inspection only. Tests enqueue `commit` directly and rebase in the mirror rather than pushing through the git route | **Holds by inspection**; real push path not exercised |
| A4: the resolution task is untouched | Completion never reads or writes the task | Inspection | **Holds**. Stale agent instruction (F6) |
| A5: UI refresh | 15 s polling in `useRemoteTracking.ts`; `resolved` label plus unit test | Vitest passes (22) | **Holds**; not browser-verified |
| `g5.story.resume-after-divergence` | All of the above | — | Holds except F1/F2/F3 |

## Findings

### F1 — defect (low–medium): a racing owner confirmation gets a 409

- **Governing rule:** `g5.architecture.recovery`. The design acceptance requires that a manual confirmation racing an automatic completion yield "exactly one completion and no error visible to the user."
- **Where:** `merge.rs:605-617` (apply's locked recheck) and `routes/workspaces.rs:~6364-6383`.
- **What happens:** if the owner's prepare returns `Ready` and the worker's apply commits first, the owner's apply sees `status='completed'` and returns `Conflict` ("changed after completion preparation"). The UI shows "Failed to mark resolved: …".
- **Consequence:** this race only became reachable with this change, because an actor-less completer now exists. Exactly one completion still happens, so data stays correct, but the user sees a spurious error. The reverse race logs a misleading "automatic divergence completion failed; run stays open" warning (`worker.rs:549`) even though the run did complete.
- **Fix:** in apply's `Ready` arm, when the locked row is already terminal (`completed`), return `completed:false` as an `AlreadyTerminal` replay instead of `Conflict`. Alternatively, have the route retry prepare once on that conflict. Soften the worker log for the same case.
- **Regression check:** prepare as Owner, run the automatic apply, then the Owner apply. Expect HTTP 200 with `completed:false`, one completion, `confirmed_by` NULL. Also test the reverse order.

### F2 — design question (medium): a late manual confirmation no longer publishes

- **Governing rule:** `g5.workspace.remote.divergence-auto-publish` says "A manual confirmation keeps its current meaning as an explicit publication request by the confirming owner."
- **Where:** `merge.rs` `AlreadyTerminal` path; test at `remote_repo_tracking_integration.rs:2345`.
- **What happens:** when the worker completes first, an owner who clicks Mark resolved within the 15 s polling window gets `completed:false`, and no push is enqueued. The confirm text in `BranchTrackingPopover.tsx:520-522` still promises that "the next integration cycle will push the resolved tip". The same applies in auto mode without a GitHub App, where the automatic cycle records `push_failed`.
- **Consequence:** the owner's explicit publication intent is silently dropped. Before this change the click always published, through the owner's personal connection.
- **Fix (pick one):**
  - (a) When an Owner request meets an already-completed run, still enqueue the authorized `push` under the owner's captured authority, which honors the block as written.
  - (b) Amend the block to say a confirmation of an already-completed run is a no-op, and update the confirm copy.

  I recommend (a). It needs a decision because it changes what an explicit click publishes.
- **Regression check:** in manual push mode, complete automatically, then POST complete as the owner. Assert the provider tip equals the resolved tip after a drain (for a) or the documented no-op (for b).

### F3 — defect (low): `g5d remote push/sync --wait` can report the `resolved` row instead of the operation's outcome

- **Where:** `g5d-cli/src/remote.rs:807-829` and `merge.rs:731`.
- **What happens:** the `resolved` audit row carries the trigger of the wake being processed (`trigger_kind: picked.trigger_kind.as_audit_str()`), so a `push` or `manual` wake writes a `resolved` row with trigger `push`/`manual`. `wait_for_terminal_runs` takes the first new terminal run with the expected trigger. A poll that lands between the completion commit and the cycle's own audit commit (which spans provider transport) returns `resolved`, not `pushed`/`push_failed`/`noop`.
- **Note:** the CLI change is redundant either way. Without it, `resolved` would still count as terminal through the `completed_at.is_some()` fallback.
- **Fix:** have the CLI skip `resolved` when waiting for an operation outcome (`is_new_terminal_run` returns false for `resolved`). A more structural option is to give the audit row a trigger that no CLI wait matches.
- **Regression check:** a unit test of `is_new_terminal_run` with a `resolved`/`push` row. Optionally, an integration test where a `push` wake on a proven diverged member yields a `pushed`/`push_failed` outcome for the wait.

### F4 — nit / design note: the pause is cleared without a `diverged` guard

- **Where:** `merge.rs:685-690`.
- **What happens:** apply clears `sync_paused_reason` unconditionally. The worker checked `diverged` only in its unlocked pre-select (`worker.rs:487`). The existing precedent `clear_resumed_divergence_pause` (`worker.rs:2320-2330`) guards with `AND sync_paused_reason = 'diverged'`. In practice the pause cannot change while a `job_rebase_preflight` run is open, so this is defensive.
- **Fix:** for `Automatic`, require `locked.phase == 'job_rebase_preflight'` and the integration's `sync_paused_reason = 'diverged'` under the existing `FOR UPDATE` lock. This also covers F8.

### F5 — missing evidence (medium): acceptance cases the design lists are untested

- **Gaps:**
  - No test of an agent-style run (`resolution_task_id` set) completing automatically. `open_externally_resolvable_divergence` (test line 2213) always produces a run without a task, so the linearity proof (`merge.rs:455-464`) is never exercised through the automatic path.
  - No test that an agent resolution containing a merge commit stays `waiting_for_human`.
  - No manual-vs-automatic race test. The design says "the race needs a test", and the footer admits it is untested.
  - The real git receive-pack wake path (A1) is not exercised; tests enqueue `commit` directly.
- **Fix:** add tests that (1) start a resolution with a task and push a linear rebase, then a merge-commit variant; (2) cover the F1 race; (3) optionally push through `/git` receive-pack so `enqueue_after_push` produces the wake.

### F6 — nit: stale agent instruction

- **Where:** `git_conflict_resolution.rs:103`.
- **What happens:** the provider-resolution prompt still says to report results "so the owner can confirm completion in the remote integration UI". Completion is now automatic once the proven push lands.
- **Fix:** say that pushing the proven rebase completes the run automatically, and that the owner can still confirm manually.

### F7 — nit: attribution of the automatic audit row

- **Where:** `merge.rs:731-747`.
- **What happens:** `acting_user_id = workspaces.owner_user_id` follows the worker convention (`policy.acting_user_id` uses the same source). However, `INSERT … SELECT FROM workspaces` silently inserts nothing if the row is invisible, and fails the whole completion if `owner_user_id` is NULL. In the NULL case the cycle already errors in `load_cycle_state`, so the effect is bounded.
- **Fix (optional):** make the audit insert check its row count, or bind the owner read during prepare/apply. Document that `acting_user_id` on `resolved` rows is the authority principal, not a confirmer.

### F8 — nit / minimal-design: the requester is passed twice

- **What happens:** `CompletionRequester` is passed to both prepare and apply, and the unreachable `(Owner, None)` arm at `merge.rs:756-759` guards a mismatch. A future caller could prepare as one requester and apply as another, for example preparing as Owner and applying as Automatic, which silently drops the publication.
- **Fix:** carry the requester inside `PreparedExternalRemoteTrackingCompletion::Ready` (with `Owner { push_authority }` / `Automatic { trigger_kind }`) and drop apply's parameter. This removes the internal-error arm.

### F9 — nit: efficiency

- **What happens:** every queue pickup in every org now opens one extra scoped maintenance DB and runs one lookup, even though diverged members with open runs are rare.
- **Fix (optional):** return the member's pause reason from `grab_one_queue_row` (or an `EXISTS` flag), and call the hook only when it is `diverged`.

### F10 — nit (pre-existing, cited by the design): stale doc wording

- **Where:** `docs/data-model-cleanup-plan.md` "Reconciliation in the workspace".
- **What happens:** the section says the completion check "verifies both captured tips are ancestors of the published result". The proof checks only the captured provider tip (plus linearity for agent runs). After a rebase, the old local tip is normally not an ancestor, which "Provider transport" itself states. The design cites this sentence as governing.
- **Proposed edit:** reword it to "verifies the captured provider tip is an ancestor of the published result (linearly for agent rebases)".

## Proposed ontology / design updates (not applied)

- **`g5.workspace.remote.divergence-auto-complete` footer:** add the F1 race gap (owner 409) and the F5 gaps (agent-linear and merge-commit cases untested through the automatic path; A1 by inspection only).
- **`g5.workspace.remote.divergence-auto-publish`:** resolve F2 either by code fix (a) or by an amendment that states the late-confirmation semantics, and update the Mark resolved confirm copy to match.
- **Pinned design, "Implementation reconciliation":** record the F2 decision, the F3 CLI wait semantics, and that the `resolved` audit row is the record of automatic completion (since `confirmed_by IS NULL` alone is ambiguous with other completers).

## Checks run

- `cargo clippy -p g5-api-core -p g5-api-routes -p g5-api-data -p g5d-cli`: clean.
- `TEST_DATABASE_URL=… cargo test -p g5-api --test remote_repo_tracking_integration --test remote_tracking_background_loop --test remote_tracking_merge_active --test remote_tracking_merge_direct_clean --test remote_tracking_merge_runs_route --test remote_tracking_migration --test remote_tracking_sync_runs_route`: all pass (37 + 20 + 1 + 5 + 3 + 7 + 4). This includes the four new tests and the existing manual-completion tests.
- `npx vitest run src/components/workspace/BranchTrackingPopover.test.tsx`: 22 pass.
- Test-discrimination analysis by inspection:
  - The manual-mode and auto-mode publication assertions would fail if a `push` were enqueued.
  - The unproven test would fail if the proof were skipped.
  - The provider-rewrite test fails only if all three phase guards are removed.

## Unverified areas

- The simultaneous manual/automatic race at runtime (F1 is from inspection).
- Agent-task runs (linear and merge-commit) through the automatic path.
- The real receive-pack wake path.
- A successful App-backed implicit push after automatic completion.
- Multi-repo divergence, where each member completes independently.
- Browser rendering and refresh of the card and pill.
- CLI wait behaviour under F3.
