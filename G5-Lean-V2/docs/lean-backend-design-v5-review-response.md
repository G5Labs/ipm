# Response to the draft 5 review

Response written 2026-09-13 to [lean-backend-design-v5-review.md](lean-backend-design-v5-review.md).
Each finding was validated independently; the git-mechanics findings (F1, F2,
F7 local path) were reproduced in temporary repositories, and the proposed
fixes were tested the same way before being written into
[lean-backend-design-v6.md](lean-backend-design-v6.md). Draft 5 is unchanged.

Summary: all eight findings are accepted. None changes a settled decision
(A to D and the round-1 and round-2 answers stand). Each is a contract
correction inside the design draft 5 already had: the checkpoint recipe, the
checkpoint's remote observation, the dispatch loop's ordering, start intents,
the stop barrier's authorization window, answer delivery, merge recovery, and
event retention. No decision is needed from the user; two behaviors worth
knowing about are listed at the end.

---

## F1 — Seed the checkpoint index — accepted

**Validation.** Reproduced: with an empty temporary index, `git add -A`
treats a tracked file that matches `.gitignore` as ignored, the checkpoint
omits it, and restore deletes it. Seeding the temporary index from the real
index keeps the file tracked and captures its dirty edit; the real index and
working tree are untouched (`git status` unchanged). Checked on git 2.54.

**Revision (v6 §5.4).** The recipe is now: copy the real index preserving its
timestamp (`cp -p .git/index <tmp>`), `git update-index --really-refresh`
under the temporary index so content is re-hashed rather than trusted from the
stat cache, then `git add -A`, `write-tree`, `commit-tree`. Ignore rules
therefore suppress only untracked files, never tracked ones. §15 adds the
tracked-but-ignored case and the same-second, same-size edit case.

## F2 — Checkpoint observation must not move the agent's lease — accepted

**Validation.** Reproduced with two clones: a `--force-with-lease` push that
was correctly rejected became accepted after a background `git fetch origin`,
and the laptop's commit was displaced. Observing the remote head with
`git ls-remote` instead left the tracking ref and the lease untouched, and the
same push stayed rejected.

**Revision (v6 §5.4, §11.2).** The checkpoint task never runs `git fetch`.
It records the observed internal head with `git ls-remote origin
refs/heads/<branch>` and stores it in the checkpoint message; no local ref
changes. Only user-initiated operations (`git.fetch_rebase`, "Update from
origin") and the agent's own `git fetch` update `origin/<branch>`, which is
the laptop behavior. The profile keeps `--force-with-lease` and additionally
recommends the explicit form `--force-with-lease=<branch>:<expected>` when an
agent rewrites after fetching.

## F3 — Wake stopped workspaces before filtering dispatches — accepted

**Validation.** Correct: draft 5's loop filtered on "VM running" before
calling `ensure_vm_running`, so nothing ever woke a stopped workspace.

**Revision (v6 §7.3).** The loop selects dependency-ready `queued` tasks
independently of VM state, groups them by workspace, requests resume for any
workspace whose VM is `stopped` or `failed`, and issues start commands only
for workspaces whose VM is `running` with a free concurrency slot. Tests: a
new task and a new chat turn with every VM initially stopped.

## F4 — Replay a start recorded but never delivered — accepted

**Validation.** Correct: recording `running` before the RPC and classifying
"dispatcher does not know this `exec_id`" as `vm_lost` fails a task that
never ran.

**Revision (v6 §7.3, §7.6).** Starts are two-phase. The API records
`status=running, start_confirmed=false` with the `exec_id`, then sends the
idempotent start. The dispatcher's reply confirms the disposition (started
now, already running, or already finished) and the API records
`start_confirmed=true`. Unconfirmed starts are re-sent by the dispatch loop
and on reconnect with the same `exec_id`; the "unknown to the dispatcher →
`vm_lost`" rule applies only to confirmed starts. Cancel before confirmation
marks the task `cancelled` so the loop stops re-sending, and reconnect
reconciliation cancels any process the dispatcher reports for a task the API
holds as `cancelled`, so a delayed start cannot resurrect it. Tests: crash
between record and send, cancel racing an unconfirmed start.

## F5 — Keep the draining incarnation authorized during stop — accepted

**Validation.** Correct: draft 5 revoked the incarnation at step 1 and let
the reconciler treat `stopping` as loss, which denies the barrier's own writes
and can race it with `vm_lost`.

**Revision (v6 §5.7, §6.8, §7.6).** Closing admission and revoking the
incarnation are separate. During `stopping` the incarnation stays current and
its lease keeps renewing; new user work is refused, but the incarnation's
checkpoint pushes, session-file harvest, interruption outcomes, event replay,
and acknowledgements are accepted. Admission close waits for already admitted
file mutations to finish before the quiesce step and the final snapshot.
Revocation happens at release, when the row becomes `stopped` and `vm_id` is
cleared. The reconciler treats `starting`, `running`, and `stopping` with a
matching `vm_id` and an unexpired lease as live; `vm_lost` applies only to
`failed`, to lease expiry, or to a `vm_id` that is no longer current. Tests: a
reconciler tick right after admission closes, a long checkpoint and harvest,
a Save admitted just before stop.

## F6 — Persist answer intent; deliver by polling committed state — accepted

**Validation.** Correct: a Postgres transaction cannot include an RPC to a
blocked process; either order leaves a gap.

**Revision (v6 §7.2, §8.3).** The `agent.answer` RPC is removed. The answer is
committed first (`task_questions.answer`, `answered_at`, `status=answered`),
bound to the question's `exec_id`. Delivery is a read of committed state: the
`g5-tools` MCP server that is blocking on the question long-polls the API for
that row and returns the answer as the tool result when it appears, then
records `consumed_at` (idempotent). If the process is gone (turn interrupted
or lost) before consumption, the reconciler creates exactly one successor
`chat_turn` task for the answered question, enforced by a unique
`task_questions.successor_task_id`, and that turn's prompt carries the answer
and the interruption note. Delivery is therefore at-least-once with the
duplicate bounded to one visible note in the successor turn when a process
consumed the answer and was interrupted before its consumption record landed.
Tests: API loss before and after consumption, answer racing stop, retries.

## F7 — Preserve merge side effects after partial success — accepted

**Validation.** Correct for both paths. The local-path reproduction is
standard git behavior: a compare-and-swap ref update with the old value fails
once the ref already holds the new value.

**Revision (v6 §10.6).** The merge operation keeps one identity and its
pinned inputs until resolved; a retry never re-pins while an operation is
unresolved. Provider path: after `provider_merged` is recorded with the
provider's `merge_commit_sha`, a failed import is retried without re-pinning,
and recovery attributes a remotely merged PR to this operation only if the
provider reports the pinned source SHA as the merged head. Local path: the
merge commit is written to the object store and its OID persisted as
`merge_result_sha` before the compare-and-swap; on recovery, a target already
equal to `merge_result_sha` is recorded as applied. Terminal `failed` is
reserved for a known rejection or conflict before anything was applied.
Tests: provider success followed by import failure; local CAS success
followed by a crash before the metadata commit.

## F8 — Retain nonterminal events, not only terminal outcomes — accepted

**Validation.** Correct: sequence numbers deduplicate but do not recover, and
gating on a highest-seen sequence discards a late earlier event.

**Revision (v6 §6.3, §7.6).** The dispatcher journals every event per
execution (`exec_id`, per-execution `seq`) on the VM disk until acknowledged;
the API acknowledges a cumulative watermark per `exec_id`; on reconnect the
dispatcher replays from the watermark in order, live events queue behind the
replay. The API inserts events with a unique `(task_id, vm_id, seq)` index and
ignores duplicates by that key; there is no highest-seen gate. Sequence spaces
are per execution, so concurrent tasks do not interleave. Retention is
bounded by disk (oldest acknowledged journals are pruned first; unacknowledged
journals are never pruned while the VM runs). Test: API restart in the middle
of a tool call with the VM alive; the complete timeline appears after
reconnect.

---

## Behaviors worth knowing (no decision needed)

- **Answer delivery is at-least-once** (F6): in the narrow window where a
  process consumed an answer and was interrupted before recording
  consumption, the successor turn repeats the answer with a note. The
  alternative, exactly-once, would require the provider process to
  acknowledge inside the same transaction, which is not possible.
- **The final checkpoint at stop uses a content re-hash**
  (`--really-refresh`) rather than the stat cache. On very large repositories
  this adds seconds to the stop barrier; the periodic checkpoint uses the same
  recipe, and the cost is bounded by the size of tracked files.
