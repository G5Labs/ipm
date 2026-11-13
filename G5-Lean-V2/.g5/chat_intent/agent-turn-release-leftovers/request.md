# Agent turns stop hanging on processes the agent leaves running

Source: chat with the owner on 2026-10-12/13 about two QA Codex chats
(`issue-452-security`, `issue-453-git-smart-http-decompression-unbounded`) whose
follow-up messages stayed queued for 1h40m.

## Human instructions

- Investigation, confirmed by the owner's guest `ps`: each finished turn's execution
  owner (`g5d __g5-execution-owner`, uid 1000) still held a PostgreSQL server the
  agent had started with `sudo` (uid `postgres`). The owner cannot signal it, so the
  turn's result was never recorded. Stopping that PostgreSQL released both turns at once.
- The owner accepted: keep the cleanup process at the dispatcher's identity and run
  only `agent-turn` as `ubuntu` ("looks good to me").
- A stuck-cleanup log is not needed once cleanup runs as root.
- Killing the agent's PostgreSQL at the end of every turn is not acceptable: "if agent
  put some data in one turn, then it cannot query it in the next turn"; "why do we need
  to kill the postgres, can we leave it to agent to decide when to start and kill?"
- An idle PostgreSQL in every dev VM is "too costly".
- No chat notice about processes still running after a turn: "no need, very confusing to user".
- Approved and invoked `/build-to-ship` ("proposal looks good to me") for: a normal
  finish kills nothing; stop, cancel, limits, or a failed turn kill everything the turn
  started; the cleanup process runs as root; one ontology amendment.

## Agent interpretation

- "Normal finish" means the provider itself ended the turn: a successful result, or a
  deferred native question with a clean provider exit. Outcomes the supervisor forces
  (interrupt, setup deadline, provider output limits or read failure, ended background
  wait, undeliverable or expired native request) count as limits or stops, and a
  provider error or supervisor failure counts as a failed turn.
- Processes left running after a normal finish leave the execution: they no longer hold
  execution capacity, the VM's init adopts them, and the agent decides when to stop them.
- The release decision belongs to the supervisor, which alone knows how the turn ended;
  the execution owner only applies it. Stop requests from the dispatcher always win.
