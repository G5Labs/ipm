# Ontology review: automatic divergence resolution (reviewer 3)

**Mode:** independent, read-only discovery review (reviewer 3 of 3). No production, test or ontology file was edited and nothing was delegated. The reviewer did not take part in the implementation.

**Inputs:** the request ([chat intent](chat_intent/issue-240/auto-resolve-divergence.md)); the pinned design [ontology-plan-auto-resolve-divergence.md](ontology-plan-auto-resolve-divergence.md), starting from `Ontology changes and governance`; the changed blocks in `features/workspaces.g5.md` and `concepts/core_user_stories.g5.md`; the governing blocks and `docs/data-model-cleanup-plan.md` ("Provider transport and publication", "Reconciliation in the workspace"); and the uncommitted working-tree diff.

**Disclosure:** the pinned design file already contains the author's "Implementation reconciliation" section (A1–A5 outcomes), and the ontology footers already carry implementation claims. I read both before writing these findings because they are part of the pinned design. Every finding below comes from my own source inspection or check runs.

**Ontology diff check:** the four added blocks and the story index entry match the handoff diff in meaning. The footers have since moved from "Unassessed" to "Implemented" claims. Findings 2, 4 and 9 show where those claims go further than the evidence.

## Commitment ledger

`commitment → actual path → counterexample / check → result or gap`

| Commitment | Actual path | Counterexample / check | Result |
|---|---|---|---|
| `g5.workspace.remote.divergence-auto-complete`: an accepted update that moves a member paused `diverged` past the captured tip runs the same proof | `git.rs:745` `enqueue_after_push` (commit wake, no pause filter) → `worker.rs:232` `auto_complete_proven_divergence` (before claim) → `merge.rs` `prepare_…` / `apply_…` with `CompletionRequester::Automatic` | Inspection: the push route enqueues regardless of pause and allows non-fast-forward (`receive.denyNonFastForwards=false`). The worker SQL limits to `job_rebase_preflight`, open status and pause `diverged`. Runtime: `landed_resolution_completes_divergence_automatically_and_follows_auto_push_policy` and the manual-mode test pass. | Holds for runs without an agent task. **Gap:** the agent path (linear, no merge commits) is untested (F4). A proven head whose apply fails transiently gets no retry (F6). |
| Same, failing head leaves the run open and the member paused | `prepare_…` Err → debug log, `Ok(false)` | `unproven_update_leaves_the_divergence_run_open` passes. It would fail without the ancestry check. | Holds (non-agent). Agent merge-commit counterexample untested (F4). |
| Same, provider-rewrite checkpoints never auto-complete | worker SQL `phase='job_rebase_preflight'`; `prepare` reset guard plus the new Automatic phase guard; `apply` locked `phase` check | `provider_rewrite_checkpoint_never_completes_automatically` passes. It discriminates only against removing all three guards together. Only two phases exist, so the new Automatic guard is redundant but harmless. | Holds |
| Same, manual confirmation stays available and applies the same proof | route `complete_branch_merge_run` → `CompletionRequester::Owner` | The existing `pause_only_resolution_*` tests pass. A late owner confirmation returns 200 `completed:false` (tested). | Holds. **Gap:** a racing confirmation returns 409 (F2). |
| `g5.workspace.remote.divergence-auto-publish`: automatic completion is not a publication request | `merge.rs` match on `(requester, push_authority)`: Automatic writes a `resolved` audit row and no `enqueue_authorized_branch_integration` | Auto test: the remote tip is unchanged after the drain. `drain_one_cycle` drains until the queue is empty, so an enqueued owner push would have published and failed the assertion. Manual-mode test: the remote is unchanged. | Holds |
| Same, auto-push publishes as the next fast-forward under push policy | the same wake's ordinary cycle (`classify_and_act` WorkspaceAhead + auto) | Runtime: the cycle attempts the implicit push and records `push_failed` "requires a GitHub App" (no App in the fixture). There is no fallback to a personal connection. | Partially verified. A successful App-backed push was not exercised (disclosed in the footer). |
| Same, manual confirmation keeps meaning as explicit publication by the confirming owner | Owner branch enqueues `push` under captured `PushAuthority` | Inspection: unchanged for the Ready path. The AlreadyTerminal path (late or racing click after auto-completion) enqueues nothing. | **Design question** (F3) |
| `g5.governance.publish.actor`: no fabricated confirming human | `confirmed_by` bound to NULL for Automatic; audit `acting_user_id = workspaces.owner_user_id` | Tests assert `confirmed_by IS NULL`. The audit actor matches other automatic worker audit rows (policy acting user), and the UI does not render the actor. | Holds. Nit F10 on how the audit insert is written. |
| `g5.security.owner` / Git write boundary: no added write authority | automatic completion writes only SQL lifecycle rows and a no-op ref CAS | Inspection: no Git ref changes; the triggering update was already admitted. | Holds |
| `g5.architecture.recovery`: replay safe, exactly one completion | `apply` `FOR UPDATE OF run, integration` + status guard + `rows_affected` checks; worker SQL selects only open runs | Inspection: one completion either way. Replay after completion is a no-op (the SQL finds no open run). | Holds for the effect. **Gap:** the user-visible 409 in one ordering (F2); no race test (F5). |
| `g5.architecture.recovery.progress`: unfinished obligations stay discoverable and progressing | `auto_complete_proven_divergence` error arm | Inspection: an apply error is logged and the same wake then runs a paused cycle, which consumes the row. | **Gap** (F6). The obligation stays discoverable (Mark resolved) but does not progress automatically. |
| `g5.design.reuse`: one proof | the prepare/apply pair is shared through `CompletionRequester` | Inspection: no second ancestry rule. | Holds |
| `g5.workspace.remote-ui.actions` / A5: UI reflects completion without reload | `useActiveBranchMergeRuns` and the sync-runs feed poll every 15s; `resolved` label and success tone in `BranchTrackingPopover.tsx` | Popover unit tests pass (22). Inspection of `remoteTrackingStatus.ts`: in manual push mode the pill likely shows "behind" after completion (F9). | Card and label hold. Pill state is **unverified or likely wrong** in manual mode (F9). |
| Cleanup plan: the pause suspends fetches until explicit reconciliation | the proven push lands as the reconciliation; the pause clears in the completion transaction | Inspection. | Consistent with the design's interpretation that the proven agent or owner push is that reconciliation |
| Migration safety | `043_remote_sync_resolved_status.sql` + checksum | The check set is a strict superset of 040's, so existing rows validate. The checksum script passes. `origin/main` already has `043_agent_attempt_limit_reason.sql`, `044_…` and `045_…`. | **Defect at merge** (F1) |
| Tenant scoping | all worker queries use `scoped_db_for_maintenance_org(org_id)` and filter by `picked.key` | Inspection: RLS scope and explicit org filter; the audit insert's `workspaces` read stays under the same scope. | Holds |
| Lease/claim interaction | auto-completion runs after pickup (queue row `locked_by`) and before the remote-pair lease | Inspection: the queue row lock serializes the key. A stale expired-lease cycle fails `validate_cycle_claim_for_finalization` (StaleState) because `sync_paused_reason` changed. Pair-lease deferral can repeat the attempt, which is idempotent. | Holds |

## Findings

### F1. Defect (merge blocker): migration version 043 collides with `main`
- **Governing rule:** migration safety on existing data; reviewed SQLx catalog.
- **Location:** `backend/g5-api/src/db/migrations/043_remote_sync_resolved_status.sql`, `checksums.sha384:43`.
- **Evidence:** `git ls-tree origin/main` lists `043_agent_attempt_limit_reason.sql`, `044_validate_agent_attempt_limit_reason.sql` and `045_drop_review_document_targets.sql`. The branch's merge base stops at 042. SQLx 0.8 `Migrator` keys on the numeric version.
- **Consequence:** after merge or rebase, two migrations share version 43. A database that already applied main's 043 fails with `VersionMismatch(43)`, or skips this migration and keeps rejecting `status='resolved'`. Every automatic completion would then fail its audit insert and roll back.
- **Fix:** rebase on main and renumber to the next free version (currently `046_remote_sync_resolved_status.sql`), then regenerate its checksum line.
- **Regression check:** `python3 scripts/check-migration-checksums.py`; run migrations on a database at main's head; rerun the four new tests.

### F2. Defect (minor): an owner confirmation that races automatic completion returns 409
- **Governing rule:** design acceptance "A manual confirmation racing an automatic completion yields exactly one completion and no error visible to the user". Also `g5.architecture.recovery`.
- **Location:** `backend/g5-api-core/src/services/remote_integrations/merge.rs:605-617` (locked revalidation) and `backend/g5-api-routes/src/routes/workspaces.rs` `complete_branch_merge_run`.
- **Sequence:** the owner's `prepare` returns `Ready`. The worker's automatic apply commits. The owner's `apply` locks the run, sees `status='completed'`, and returns `Conflict` "changed after completion preparation". The popover shows "Failed to mark resolved: …". In the reverse order, the worker logs a misleading warn: "automatic divergence completion failed; run stays open", although the run is completed.
- **Consequence:** exactly one completion still happens. The owner sees a spurious error, and the footer claim that the race "resolves through the existing row locks" is inaccurate.
- **Fix:** in the Ready branch of `apply`, if the locked run is already terminal, roll back and return `completed: false`, matching the AlreadyTerminal replay. Alternatively, have the route re-prepare once on this conflict. Make the worker treat that outcome as `Ok(false)`.
- **Regression check:** an integration test that calls `prepare_…(Owner)`, then runs `drain_one_cycle` (automatic completion), then calls `apply_…(Owner)`. Expect `Ok(completed=false)` and exactly one `resolved` audit row. Also test the reverse order.

### F3. Design question: a late or racing owner confirmation no longer publishes in manual push mode
- **Governing rule:** `g5.workspace.remote.divergence-auto-publish`: "A manual confirmation keeps its current meaning as an explicit publication request by the confirming owner."
- **Location:** `merge.rs` AlreadyTerminal branch of `apply` (no push enqueue); `BranchTrackingPopover.tsx:519-522`, whose copy says "The next integration cycle will push the resolved tip to the remote."
- **Consequence:** in manual push mode, an owner who clicks Confirm resolved within the ≤15s polling window after automatic completion gets 200 `completed:false`. Nothing is published, which contradicts the button copy and arguably the block. Under auto-push this does not matter.
- **Options:** (a) on an Owner request that finds the run completed automatically (`confirmed_by IS NULL` plus a `resolved` audit row), still enqueue the authorized `push`. (b) Amend the block and copy so a confirmation is a publication request only when it completes the run. This needs a human decision.
- **Regression check:** for (a), a manual-mode test where automatic completion runs, then the owner completes, then a drain: the remote equals the resolved tip.

### F4. Missing evidence: the agent-resolution proof (linear, no merge commits) is never exercised
- **Governing rule:** `g5.workspace.remote.divergence-auto-complete` ("linearly without merge commits when an agent resolved it"); design acceptance items 1 and 3.
- **Location:** `backend/g5-api/tests/remote_repo_tracking_integration.rs` `open_externally_resolvable_divergence` (the agent start fails, so `resolution_task_id` is NULL and the proof uses `pre_start_source_heads` without the linearity check).
- **Consequence:** under auto-push, a divergence run exists only when the owner chose "Resolve with agent" (`classify_and_act` pauses auto-push divergence without a run). The agent path is therefore the main production case, and both its pass case and its merge-commit counterexample are unverified. No other test in `backend/g5-api/tests` reaches `is_linear_descendant_of`.
- **Fix:** add a fixture run with `resolution_task_id` and `source_heads` set, either through a successful start or by fixture setup of the start's persisted inputs. Test (a) a linear rebase onto `source_heads.mirror_tip`, which completes automatically, and (b) a merge of the provider tip, which leaves the run `waiting_for_human` and the member paused.
- **Regression check:** both new tests fail if the `resolution_task_id.is_some() && !is_linear_descendant_of` branch is removed.

### F5. Missing evidence: no race test
- **Governing rule:** `g5.architecture.recovery`; the design notes that "the race needs a test"; the footer admits it is untested.
- **Fix and check:** the interleaving test proposed in F2.

### F6. Defect (minor): a proven resolution whose automatic apply fails has no retry owner
- **Governing rule:** `g5.workspace.remote.divergence-auto-complete` ("A passing head completes the run") and `g5.architecture.recovery.progress`.
- **Location:** `backend/g5-api-core/src/services/remote_integrations/worker.rs:543-551` (error arm) together with `drain_one_org` continuing to the claim.
- **Consequence:** if `apply` fails after a passing proof (pool or DB error, `apply_ref_commits` CAS window, NOT NULL on a missing owner, the F1 constraint), the same wake proceeds to a paused cycle, which consumes the queue row. Nothing retries until another accepted update to that repository or a manual click. The obligation stays discoverable, but tracking does not resume on its own.
- **Fix:** have `auto_complete_proven_divergence` return a tri-state result. On an apply error other than "already completed or changed by another actor", defer the queue row briefly (as `defer_queue_row_for_pair_lease` does) and skip the claim. The existing `attempts` counter bounds the retries.
- **Regression check:** a unit or integration test with a forced apply failure (for example, a temporary trigger rejecting `remote_sync_runs` inserts). After the failure clears, the row is retried and the run completes without a new update.

### F7. Nit: operational prepare failures are logged at debug level as "not proven"
- **Location:** `worker.rs:510-521`.
- **Consequence:** DB errors, a missing local repo path, missing `workspace_tip`/`mirror_tip` in run heads, or Git open errors are indistinguishable from an ordinary "not yet proven" and invisible at the default log level.
- **Fix:** log `G5Error::Conflict` (proof refusal) at debug and everything else at warn.

### F8. Nit: the automatic eligibility predicate is not enforced at the commit point
- **Location:** worker SQL (`sync_paused_reason = 'diverged'`, read transaction) versus `merge.rs:684-695` (unconditional `SET sync_paused_reason = NULL`).
- **Consequence:** today every writer that changes the pause also changes the run's status in the same transaction, and `apply` locks both rows, so I found no reachable counterexample. The commitment ("paused for divergence") still rests on that coupling rather than on the commit-time predicate.
- **Fix:** for `Automatic`, add `AND sync_paused_reason = 'diverged'` to the clear. The existing `rows_affected != 1` check then turns a changed pause into a conflict.

### F9. Missing evidence, likely UI defect: in manual push mode the pill shows "behind" after automatic completion
- **Governing rule:** A5 / `g5.workspace.remote-ui.status`; story `g5.story.resume-after-divergence` ("remote tracking resumes").
- **Location:** `worker.rs` `execute_decision` Noop arm (a WorkspaceAhead noop does not advance the watermark unless the tips are equal); `frontend/src/components/workspace/remoteTrackingStatus.ts` `isBehind` (`advisory != baseline`) outranks `isAhead`.
- **Consequence:** after automatic completion in manual mode, the watermark stays at the pre-divergence baseline B. The provider observation recorded by the diverged cycle is R, which descends from B. The pill likely reads "Remote moved — your branch is behind" although the workspace already contains R. Before this change, completion always published immediately and advanced the watermark, so this state is new as the normal post-resolution outcome. Not runtime-verified.
- **Fix options (design):** have the client or summary treat the remote as not behind when the head descends from the advisory (the server has Git), or have the cycle advance the watermark when the workspace descends from the provider tip (this changes cycle semantics and needs design).
- **Regression check:** extend the manual-mode test to read the tracking summary (`baseline_commit`, `remote_tip_advisory_sha`, `head_commit`) and feed `deriveRemoteTrackingStatus`. Expect `ahead`, not `behind`.

### F10. Nit: the audit insert's actor source
- **Location:** `merge.rs:726-756`.
- **Consequence:** `INSERT … SELECT … FROM workspaces` silently inserts nothing if the row is not visible (no `rows_affected` check). It fails the whole completion if `owner_user_id` is NULL, because `remote_sync_runs.acting_user_id` is NOT NULL. Using the owner as the audit actor is consistent with other automatic worker rows and is not a fabricated confirmation, since the UI does not render it.
- **Fix:** check `rows_affected() == 1`. Optionally record `reviewed_remote_tip` as `new_mirror_tip` for audit completeness.

### F11. Nit: one test assertion is vacuous
- **Location:** `remote_repo_tracking_integration.rs:2330-2338` (`explicit_push_requested == 0`).
- **Consequence:** `drain_one_cycle` returns only when the queue is empty, so this assertion always passes. The real discrimination comes from `remote_tip == remote` and the status sequence. Remove it or replace it with a check on queue state captured before the drain finishes, for example by asserting that no `push` trigger audit row exists.

## Proposed ontology and design edits (not applied)

- `g5.workspace.remote.divergence-auto-complete` footer: replace "a simultaneous one resolves through the existing row locks" with "one ordering returns 409 to the owner (F2)". Record that the agent linear/merge-commit path and the transient-apply retry are untested or unimplemented.
- `g5.story.resume-after-divergence` footer: the test claim covers non-agent runs only. Add the manual-mode pill gap (F9).
- `g5.workspace.remote.divergence-auto-publish`: needs a human decision on F3. Either amend the block to say a confirmation that finds the run already completed does not publish, or implement the push.
- Plan `Implementation reconciliation`: A5 "covered" should note the manual-mode behind-pill risk. A1 should note the missing retry owner after a failed apply (F6). Add the migration renumber once rebased (F1).

## Checks run

- `cargo clippy -p g5-api-core -p g5-api-routes --all-targets`: clean.
- `cargo test -p g5-api-core --lib remote_integrations`: 23 passed.
- `TEST_DATABASE_URL=… cargo test -p g5-api --test remote_repo_tracking_integration`: 37 passed, including the 4 new tests.
- `… --test remote_tracking_background_loop --test remote_tracking_merge_active --test remote_tracking_multi_workspace --test remote_tracking_sync_runs_route`: 20, 1, 8 and 4 passed. The first combined run failed 3 background-loop tests inside the test-database bootstrap (`helpers/mod.rs:241`, "Failed to run migrations"). They passed singly and on a full rerun, so I attribute the failure to concurrent test-database setup (other reviewers sharing the server), not to this change.
- `python3 scripts/check-migration-checksums.py`: catalog matches. `shasum -a 384` of 043 matches its entry.
- `npx vitest run src/components/workspace/BranchTrackingPopover.test.tsx`: 22 passed.
- `git ls-tree origin/main backend/g5-api/src/db/migrations/`: confirms the version collision (F1).

## Unverified areas

- The real accepted-update path: the tests rewrite the authority repository directly and enqueue `commit`. The `git.rs` wake was confirmed by inspection only.
- A successful GitHub App-backed implicit push after automatic completion.
- The agent-resolution path (F4), the owner/automatic race (F2/F5), and retry after a transient apply failure (F6).
- UI behavior in a browser: card removal, pill state (F9), resolution link refresh.
- Multi-repo coordinated divergence, where each repository completes independently.
