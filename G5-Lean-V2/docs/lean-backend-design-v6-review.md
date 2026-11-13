# Follow-up review of backend design v6

Reviewed 2026-09-13 against [v6](./lean-backend-design-v6.md) and the [response to the v5 review](./lean-backend-design-v5-review-response.md). Recommendation: revise the five remaining recovery gaps below before accepting the design. Four v5 findings are closed at the design level; four are partially addressed. The remaining changes fit the chosen architecture and require no new product decisions.

| V5 finding | Assessment against v6 |
|---|---|
| F1: checkpoint drops tracked, ignored files | Closed for the reported failure. The seeded index captures and restores the dirty file, including a same-second, same-size edit, without changing the real index. See the small command-status note below. |
| F2: background fetch weakens the agent's lease | Closed. Independently verified that `ls-remote` leaves the tracking ref unchanged and the stale implicit-lease push still fails. |
| F3: stopped workspaces never reach dispatch | Closed at the contract level. Queued-work selection now precedes VM-state filtering and requests resume. |
| F4: a recorded but undelivered start becomes lost | Partial. Same-incarnation replay fixes the original crash window. Incarnation loss and cancellation still need G1 and G2. |
| F5: stop revokes its own authority | Closed at the contract level. Admission drain, continued authority/lease renewal, and release ordering now agree with reconciliation. |
| F6: answer delivery crosses a nonexistent transaction | Partial. Commit-first polling and explicit at-least-once delivery resolve the original concern; successor eligibility needs G3. |
| F7: merge side effects are lost on retry | Partial. Provider import recovery and the original local-CAS crash case are addressed. A subsequent target update still breaks local recovery; see G5. |
| F8: timeline replay loses events | Partial. Journaling and replay address retention and ordering, but the database key omits part of the execution identity; see G4. |

Decisions A–D remain accepted, including interruption on explicit stop, deferred Codex native approvals, owner branch rewrites, and the checkpoint policy. This review also accepts the documented possibility of repeating an answer after an ambiguous consumption acknowledgement.

1. **G1 — [P1] Resolve unconfirmed starts when their VM incarnation is lost.** [§7.6, lines 885–891](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:885)

   The loss reconciler now handles only **confirmed** running/waiting tasks, while every unconfirmed start is re-offered with its original `exec_id`. Consider a start whose reply is lost, followed by host loss before confirmation. The task remains `running, start_confirmed=false` and bound to the dead `vm_id`. It is neither failed/requeued by reconciliation nor selected as queued demand to wake the workspace. A replacement VM cannot execute that stale identity. The same gap exists if the original start never reached the VM at all. An eventual wall-clock timeout does not provide the selected VM-loss retry behavior.

   **Change:** distinguish an unknown start on a still-current dispatcher from loss of the incarnation itself. Replay the former with the same identity; settle every outstanding intent bound to a lost incarnation, including unconfirmed ones, and apply the existing failure/retry policy. Missing confirmation does not prove that no process ran.

   **Validation to add:** lose the incarnation before delivery and after spawn with its reply lost. With retry disabled, both settle; with retry enabled, both create the permitted fresh attempt and wake a replacement. An executable model of the written predicates confirmed that neither loss handling nor wake selection currently selects these tasks.

2. **G2 — [P1] Persist cancellation at the dispatcher before rejecting delayed starts.** [§7.6, lines 853–857](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:853)

   Stopping future re-sends does not retract a start already sent. The API can record cancellation while that request is delayed; signalling an absent process does nothing, and the delayed request can subsequently launch it. No cancelled execution disposition is specified. Reconnect cleanup may eventually kill the process, but cannot prevent intervening side effects and need not run immediately. The response's claim that not re-sending prevents resurrection is therefore unsupported.

   **Change:** retain and retry a cancellation command until acknowledged, and have the dispatcher record a cancellation disposition keyed by `exec_id`, even when no process exists yet. A later start for that identity must return that disposition without spawning. Define the ordering guarantee at the dispatcher: cancellation that wins the launch decision prevents launch; a start that already won must be terminated. The blanket §15 promise that any racing cancellation “never launches” needs this qualification.

   **Validation to add:** deliver cancellation before a delayed first start, replay both commands, and verify no spawn; separately verify termination when start wins. A small model of the documented dispositions reproduced a first launch after API cancellation.

3. **G3 — [P1] Do not create an answer successor after explicit cancellation.** [§7.2, lines 763–779](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:763)

   The successor condition includes an execution that is “otherwise terminal,” while cancellation only cancels **open** questions. If an answer commits, the user cancels before `consumed_at` is recorded, and reconciliation runs, the question is still `answered`, the task is terminal, and the rule creates a new `chat_turn`. That new task can wake the workspace and continue work the user just cancelled. The same catch-all can also restart a normally completed or timed-out turn after a missing consumption record.

   **Change:** make successor eligibility explicit for interruption/VM loss, and suppress continuation on user cancellation rather than treating every terminal outcome alike. Serialize cancellation with successor creation so cancellation also covers a successor already created by that question. Preserve the accepted at-least-once behavior within the eligible interruption cases.

   **Validation to add:** answer commit → cancel → reconciliation, including a concurrent successor creation. No new runnable turn should survive cancellation. The literal terminal/answer predicates currently select this sequence for continuation.

4. **G4 — [P1] Include the attempt in event deduplication and acknowledgement state.** [§7.6, lines 858–870](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:858), [§7.1, lines 704–716](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:704)

   Journal sequences are per `exec_id = (task_id, attempt, vm_id)`, but the event uniqueness constraint is only `(task_id, vm_id, seq)`. Two attempts of one task on the same VM therefore collide. This is reachable through the specified stop path: interrupt and requeue a retry-enabled task, fail the final checkpoint push, reopen admission on the same incarnation, and dispatch the next attempt. Its sequence numbers restart, so its events are discarded as duplicates of the previous attempt. A watermark stored only on the mutable task row also needs an explicit execution association to avoid acknowledging the wrong attempt.

   **Change:** carry the full execution identity into event keys, durable watermarks and acknowledgement checks. Keep old-attempt replay separate from transitions of the current attempt.

   **Observed:** an isolated SQL uniqueness model inserted two events from each of two attempts on the same VM; the specified key retained only the first attempt's two events. Add the failed-stop/retry scenario to §15, including replay and delayed acknowledgements from the old attempt.

5. **G5 — [P2] Recognize a local merge that was applied before the target advanced again.** [§10.6, lines 1288–1298](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:1288)

   Exact equality with `merge_result_sha` recovers only while the target remains unchanged. Let the operation CAS the target from B to result R, then crash before recording `applied`. Before recovery, the target workspace owner pushes a normal fast-forward R → N; workspace-branch targets explicitly use this local path. Recovery sees N rather than R, its original CAS fails, and the specified “any other value” rule reports `failed`, even though the merge succeeded and remains in the target's history.

   **Change:** retain application evidence that survives subsequent target writes, for example a durable receipt committed atomically with the ref update, or fence subsequent writers until the operation is resolved. Recognizing a descendant covers the ordinary fast-forward case, but permitted owner rewrites require stronger evidence if recovery must determine whether application occurred.

   **Observed:** a temporary Git repository reproduced successful B → R, subsequent R → N, and rejection of the original CAS while R remained an ancestor of N. Extend the local-CAS recovery test with the intervening target push; recovery must retain the successful merge outcome.

Small recipe clarification: `git update-index --really-refresh` at [line 374](/Users/zliu/Workspace/G5-Lean/docs/lean-backend-design-v6.md:374) returned exit status 1 with `kept.lock: needs update` for the normal dirty-file case. The snapshot succeeded when execution continued to `git add -A`. Specify handling of this expected status so an error-propagating command runner does not abort every dirty checkpoint.

Validation: read the response and revised contracts; independently exercised the checkpoint recipe, restore, lease observation, and local merge recovery in temporary repositories using Git 2.54.0; checked the event uniqueness constraint with an isolated SQL model; traced the start/loss/cancel/answer predicates with small executable models. These models validate consequences of the written contract, not backend implementation or production behavior. No backend, design, or response file was modified; only this review was added.
