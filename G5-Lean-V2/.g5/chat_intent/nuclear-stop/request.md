# Stop always completes

## Human request

On 2026-10-07 (UTC) the user wrote:

> when i try to stop the VM at https://qa.g5labs.ai/g5/add-gcloud-to-image, it stuck at `Stopping` for quite a while and still not stopped, this is very bad, the stop VM button should act as the nuclear weapon to kill all lifecycle issues and offer a fresh new restart when user hit the Start button after VM is stopped

The user then asked: "what was the design for that 10-minutes deadline? which PR implements it".

Answers to the agent's questions:

- "When the Stop button can't save uncommitted work (the save fails or the VM doesn't answer), what should happen?" → **Stop anyway, show loss**: a short grace period (about 1–2 minutes), then kill the VM; changes since the last good save are lost and a notice says exactly what.
- "Should automatic stops (idle after 30 minutes, transfer) also force through when saving fails?" → **Every stop**: idle and transfer stops also kill the VM and record the loss when saving fails.
- "What should I do with the stuck add-gcloud-to-image workspace on QA right now?" → **Leave it**.

After reading the proposed fix list, the user wrote:

> regarding "6. Lost work is invisible." please do NOT improve it, keep as-it, we do not need to show. i agree with all others except 2 as i comment before, please implement and then ask codex (via cli) with gpt-6-astra and extra high effort to review, once codex approves, file a github pr

Asked what to use instead of item 2's two-minute save window (options: keep 10 minutes, one try with no retries, 2 minutes), the user chose **2 minutes after all**.

## Observed incident

Evidence from QA Cloud Logging, namespace `g5`, workspace `f9e2fec7-736f-415f-8942-4fcec23acf00`, task `1a5e74d4-88ee-44a6-a101-765bfe500feb`:

- A Bash tool result held NUL bytes. Projecting that journal event fails in Postgres (`invalid byte sequence for encoding "UTF8": 0x00`) on every agent reconciler tick, so the chat turn never settles. Fourteen chat Stops between 00:27:48 and 00:27:54 returned 200 and changed nothing.
- Owner Stop at 00:30:58 (operation `74b763d2-0bf3-477e-9a5f-1cd0bbf38902`) replayed the same event while quiescing agent tasks, failed with `AGENT_STOP_REPLAY_COMMIT_FAILED`, and `defer_stop_transport` retried every 5 seconds.
- At 00:40:58, the 10-minute `DEFAULT_STOP_DEADLINE_SECS` (added by PR #117, commit `09f11e34`) passed. Every guest RPC carries the stop deadline as its own deadline, so it failed at once with `DEADLINE_EXCEEDED`, and the stop retries ended. By the code, owner Stop then aborts and reopens admission. The database row was not read.
- v9 (`docs/workspace-vm-api-v9.md` §5) gave owner, transfer and idle stops 120 seconds of transient retry and then an abort. The 10 minutes is the deadline-stop flush window; owner Stop inherits it only through the shared default.

## Agent interpretation

- **Every stop completes.** Owner, idle, transfer, Delete and deadline stops all end with the VM stopped and released. Stop never aborts back to a running VM. This reverses "the stop contract permits no force Stop", the stop half of `g5.git.flush-failure`, and the "must not force loss" clause of `g5.lifecycle.transfer.failure` (human decision H072 accepted a transfer abort on flush failure).
- **Save first, inside a bounded window.** Stop keeps today's order (fence, settle attempts, harvest, flush). When a step cannot finish inside the window, Stop stops calling the guest, records what was not saved, and kills the VM through its host.
- **The window is two minutes** for owner, idle, transfer and ordinary Delete stops, which is v9's 120-second figure and inside the user's "about 1–2 minutes". Deadline stops keep their existing flush deadline after the 30-minute nudge. Force Delete skips the window.
- **No new loss display.** The loss stays recorded as today (`unflushed_loss_at` and a short reason); no notice, no UI and no richer record.
- **"Fresh restart"** is the existing restore: Start creates a new incarnation from the last complete receipt. Nothing a forced stop leaves behind may block that Start.
- **Not in this request:** the NUL-byte projection fix (done separately in #337), per-turn chat Stop on a poisoned event, a host that cannot be reached at all, and the QA workspace itself.
