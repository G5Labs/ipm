# Automatic divergence resolution: review

**Mode:** three independent read-only discovery reviews ran in parallel, each in a fresh subagent with the same request, pinned design, ontology, implementation snapshot and skill. The implementing agent then consolidated the findings and made one repair. The raw reports are preserved unchanged in [r1](ontology-review-auto-resolve-divergence.r1.md), [r2](ontology-review-auto-resolve-divergence.r2.md) and [r3](ontology-review-auto-resolve-divergence.r3.md). Reviewer 3 disclosed that it read the plan's implementation-reconciliation section before writing findings.

**Inputs:** [request](chat_intent/issue-240/auto-resolve-divergence.md), [design](ontology-plan-auto-resolve-divergence.md), the working tree on `ben/issue-240-remote-branch-ui`.

## Initial result (before repair)

All three reviewers found that the core commitments hold:

- Automatic completion reuses the owner's proof.
- `confirmed_by` stays empty and a `resolved` audit row is written.
- No `push` is enqueued, so publication follows the push policy, with no fallback to personal credentials.
- Provider-rewrite runs are excluded.
- A replayed wake is a no-op.
- Queries are org-scoped.

Reviewers 2 and 3 confirmed A1 by inspection: a native git push queues a `commit` wake even while paused.

Checks the reviewers ran: clippy clean; `remote_repo_tracking_integration` 37/37 (including the 4 new tests); background loop 20/20; merge-active 1/1; direct-clean 5/5; runs route 3/3; migration 7/7; multi-workspace 8/8; sync-runs route 4/4; popover vitest 22/22; migration checksum script passing.

## Consolidated findings

| # | Finding | Kind | Raised by | Planned disposition |
|---|---|---|---|---|
| C1 | `043_remote_sync_resolved_status.sql` collides with `origin/main`'s 043–045, which landed after this branch's base | Defect (merge blocker) | r2 F4, r3 F1 | Renumber after main's last migration, using main's `NOT VALID` then `VALIDATE` pattern |
| C2 | An owner "Mark resolved" racing the worker can get 409 "changed after completion preparation" (`merge.rs` locked recheck). The worker then logs "run stays open" | Defect | r1 F1, r2 F1, r3 F2 | Treat an already-completed run under the lock as an idempotent replay for either requester |
| C3 | An owner confirmation after an automatic completion returns `completed:false` and publishes nothing, although `divergence-auto-publish` says a manual confirmation stays an explicit publication request | Defect vs the governing block (reviewers asked for a decision) | r1 F2, r2 F7, r3 F3 | The block already decides it: queue the owner's authorized push on a late confirmation |
| C4 | If the proof passes but apply fails transiently, the paused claim consumes the wake and the repo stays paused until another update | Defect (recovery) | r2 F2, r3 F6 | On apply failure after a passing proof, defer the queue row so a later sweep retries |
| C5 | After automatic completion in manual mode, the watermark may stay at the old baseline while the recorded provider tip moved, so the pill may read "behind" | Probable UI defect (not runtime-verified) | r3 F9 | Verify in the manual-mode test; fix if confirmed |
| C6 | `g5d remote … --wait` can return the `resolved` row instead of the real outcome | Defect (CLI) | r1 F3 | Have the wait skip `resolved` rows |
| C7 | The agent-resolved path (linear-descendant proof against `source_heads`, merge-commit refusal) is untested | Missing evidence | r1 F5, r2 F3, r3 F4 | Add worker tests for an agent run |
| C8 | No race test | Missing evidence | r1 F5, r3 F5 | Add a deterministic owner-after-automatic and lock-ordering test |
| C9 | The provider-rewrite test exercises only the worker's SELECT filter; `merge.rs` refusals are unexercised | Missing evidence | r2 F6 | Exercise the automatic refusal directly |
| C10 | Apply does not recheck the `diverged` pause and the `job_rebase_preflight` phase under lock for automatic completion | Nit (defensive) | r1 F4, r2, r3 F8 | Recheck under lock |
| C11 | The audit `INSERT … SELECT` does not check its row count | Nit | r1 F7, r2, r3 F10 | Require exactly one row |
| C12 | The requester is passed to both prepare and apply, leaving an unreachable `(Owner, None)` branch | Nit | r1 F8, r2 | Carry the requester and owner authority inside the prepared result |
| C13 | Operational errors during preparation are logged at debug as "not proven" | Nit | r3 F7 | Log only proof refusals at debug, everything else at warn |
| C14 | The `explicit_push_requested == 0` assertion always passes | Nit | r3 F11 | Remove it |
| C15 | The agent prompt (`git_conflict_resolution.rs`) still tells the owner to confirm completion | Nit | r1 F6 | Say G5 completes the run once the push lands |
| C16 | Every non-background pickup now opens an extra DB scope | Nit (efficiency) | r1 F9, r2 | Accept: one indexed lookup per wake |
| C17 | The cleanup-plan doc says the proof checks "both captured tips"; only the provider tip is checked | Nit (pre-existing doc) | r1 F10 | Out of scope; recorded |
| C18 | The real git push path, an App-backed implicit push, multi-repo divergence and UI refresh are unverified | Verification gap | all | Recorded as unverified |

## Repair and dispositions

| # | Disposition |
|---|---|
| C1 | **Fixed.** Renumbered to `049_remote_sync_resolved_status.sql` (`NOT VALID`) and `050_validate_remote_sync_resolved_status.sql` (`VALIDATE`), matching main's pattern, with checksum entries. The checksum and version scripts pass. Rebasing onto main will still conflict textually in `checksums.sha384` (a union resolution). The dev database had been reset, so there was no stale version-43 row to clean up. |
| C2 | **Fixed.** A run already completed under the lock replays as `completed:false` for either requester, so the worker no longer logs "stays open" and the owner sees no 409. Covered by `owner_confirmation_racing_automatic_completion_replays_and_requests_publication`, which prepares as the owner, lets the worker complete, then applies. |
| C3 | **Fixed within the existing contract.** `divergence-auto-publish` already says a manual confirmation stays an explicit publication request. A late or raced owner confirmation of a completed run now re-checks the owner's push authority and queues that owner's `push`. The same test asserts the queued `push` by the owner and the published tip. No ontology wording changed. |
| C4 | **Fixed (source-inspected).** When the proof passed but apply failed, or the proof could not be evaluated for an operational reason, the worker defers the queue row with the existing short lease-defer window instead of letting the paused claim consume it. There is no fault-injection test. A persistent apply failure keeps deferring that wake, which the footer records. |
| C5 | **Confirmed and fixed.** In manual mode the post-completion cycle classified `WorkspaceAhead` as a Noop and left the watermark at the pre-divergence commit. The Noop path now advances the watermark to the provider tip when the relation is `WorkspaceAhead` and the watermark lags. That tip descends from the old watermark and is an ancestor of the workspace tip, so it is a valid shared point. The manual-mode test asserts the watermark, and fails without the fix. |
| C6 | **Fixed.** The CLI wait skips `resolved` rows, and the redundant entry in the terminal-status list was removed. Unit test: `wait_skips_an_automatic_divergence_completion_row`. |
| C7 | **Fixed.** `agent_resolution_pushed_through_git_completes_automatically` runs the existing agent flow: a real conflicting rebase pushed with a lease through the production Git HTTP receiver. It asserts that the accepted push queued the wake, that the run completed automatically with no confirming human, the pause cleared, and a `resolved` audit row exists, and that an explicit push then publishes. The same test drives merge-commit heads through the worker and asserts the run stays open. |
| C8 | **Fixed.** See C2. |
| C9 | **Fixed.** The provider-rewrite test also calls the completion service with an automatic requester and asserts its `Conflict`. |
| C10 | **Fixed.** For automatic requesters, apply rechecks `job_rebase_preflight` and the `diverged` pause under the row lock; if either changed it returns `completed:false` without effects. |
| C11 | **Fixed.** The audit insert requires a workspace owner and exactly one inserted row. |
| C12 | **Fixed.** `PreparedRequester` carries the owner's captured push authority inside both prepared variants; apply lost its requester parameter and the unreachable branch. |
| C13 | **Fixed.** Only `Conflict`/`ValidationError` refusals log at debug as "not proven". Other preparation errors warn and defer, as in C4. |
| C14 | **Fixed.** Removed. |
| C15 | **Fixed.** The resolution prompt now says G5 completes the run once the push lands and passes its check, and that the owner can also confirm. |
| C16 | **Accepted.** One indexed lookup per non-background wake. |
| C17 | **Recorded.** Pre-existing documentation wording, out of scope. |
| C18 | **Still unverified:** a successful App-backed implicit push, multi-repo divergence, and the UI refreshing in a browser. The real git push path is now covered by C7. |

## Rechecks

Recorded in the final report once the regression run completes.

## PR review follow-up (CodeRabbit on G5Labs/G5-Lean#267)

- **Unbounded retry gate (worker).** The C4 repair deferred the wake indefinitely after an unapplied completion, so a persistent failure could hold the member's queue row. That included any explicit push merged into it. **Fixed.**
  - Failures are now classified: retryable (`Conflict`, `ServiceUnavailable`, `DatabaseRetryable`) or terminal.
  - Retryable failures back off on the queue row's existing `attempts`, `last_attempt_at` and `last_error`: 15 s doubling, up to five attempts.
  - Terminal failures, and the last attempt, set the run's `blocking_reason`, which the merge-run card now shows. The wake then proceeds exactly as it would while paused; that path already drops all wakes while a divergence run is open, so no explicit push is newly discarded.
  - "Mark resolved" recovers using the same proof and queues the owner's push.
  - A missing workspace owner for the audit row is now terminal.
  - Test: `terminal_automatic_completion_failure_surfaces_on_the_run_without_holding_the_wake`. The backoff SQL was prepared against the schema but not executed by a test.
- **Unbounded coalescer wait (Changes reads).** A failed read is not shared, so each queued caller could serialize roughly one 30 s budget behind earlier failures. **Fixed.** `FreshRunCoalescer::run` now takes the caller's arrival deadline, which bounds both its wait for the lock and its own read; timeouts return the existing "exceeded its deadline" error. Test: `a_reader_queued_behind_a_stalled_run_gives_up_at_its_deadline`.
- **Rebase.** `origin/main` gained migrations 046–048, so ours were renumbered to 049/050.
- **Retry counter shared with pickups.** The retry limit and backoff reused the queue's `attempts` column, which every pickup also increments. **Fixed.** Migration `051_queue_auto_completion_attempts.sql` adds a dedicated `auto_completion_attempts` counter. Queue upserts leave it untouched, so it survives merged wakes, and it ends with the row. Test: `retryable_automatic_completion_failure_backs_off_then_surfaces_on_the_run`. It injects a serialization failure on the `resolved` insert and asserts attempts 1–4 with backoffs of about 15/30/60/120 s, followed by the surfaced blocking reason. The backoff path is now executed.
- **Contract detail in metadata.** The `divergence-auto-complete` and `divergence-auto-publish` footers had accumulated behavioral rules, against the format guide. **Fixed.** Those rules now sit in the block bodies: the audit attribution, the race replay, the bounded retry and its limits, the terminal-failure path with manual recovery, the manual-mode tracking state and the late-confirmation publication. The footers keep only locations, tests and gaps. No commitment's meaning changed.
- **Abandoned Changes executions kept their slot.** This was raised outside the diff. When a read's deadline expired after `workspace.changes` started, the local future ended without `modules.cancel`, so the VM execution kept one of the module's two slots. **Fixed.**
  - The read now enforces its caller's deadline itself.
  - On a deadline, a status-poll failure or an invoke reply lost to the deadline, it sends a best-effort `modules.cancel` with its own 2 s budget.
  - The coalescer now bounds only the wait behind other readers, so a read is never dropped mid-flight.
  - Test: `an_abandoned_live_changes_read_cancels_its_vm_execution`, using a new unresponsive-Changes mode in the test runtime peer. It fails without the cancel.  - Follow-up: `a_live_changes_read_cancels_its_vm_execution_at_the_deadline` covers the deadline path itself. The peer reports `running` until cancelled; the test asserts the read fails at the production 30 s deadline and that `modules.cancel` follows the final poll. It is slow by design.

