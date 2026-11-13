# Response to the draft 3 review

Response written 2026-09-13 to [lean-backend-design-v3-review.md](lean-backend-design-v3-review.md).
Each finding was validated independently against the code before deciding.
Accepted revisions are applied in [lean-backend-design-v4.md](lean-backend-design-v4.md);
draft 3 is unchanged. Two accepted revisions change details the user settled in
earlier rounds, and two more choose between alternatives the review left open;
those four are listed under "Decisions needed" at the end.

Summary: all ten findings and the correction are accepted. Six are accepted as
written, four are accepted with a modification explained below. Draft 4 is
larger than draft 3 by the four contracts the review asked for: an incarnation
fence, a dispatch and event contract, a stop barrier, and a merge operation.
None of them reintroduces per-task leases, staging refs, or the lifecycle
kernel.

---

## 1. Restore committed-but-unpushed history — accepted

**Validation.** The review's reproduction matches the draft 3 algorithm: a
no-commit cherry-pick of the checkpoint commit `C` onto branch head `A` applies
only `B → C` and silently drops the local commit `B`. Correct.

**Revision (v4 §5.4).** The checkpoint now records three things: the internal
branch head observed at checkpoint time, the local `HEAD` (which may be ahead
of it), and the working-tree snapshot. The checkpoint commit `C` still has the
local `HEAD` as its parent, so the force-push of the checkpoint ref carries the
whole local commit chain. Restore no longer cherry-picks: it sets the local
branch to `C^` (the local `HEAD` at checkpoint time), materializes `C`'s tree
as unstaged changes, and sets the upstream to the internal branch. If the
internal head moved while the VM was down, the local branch is simply behind or
diverged, exactly as a laptop clone would be; the UI shows ahead/behind and the
agent or user rebases. The cherry-pick path is deleted. Two acceptance tests are
added: clean tree with a committed-but-unpushed file, and local commits plus
dirty edits.

## 2. Fence an old VM before admitting a replacement — accepted

**Validation.** Correct: a renewable token scoped to the workspace does not
revoke an unreachable VM that later reconnects; the unique `workspace_vms` row
only prevents two assignments in the database.

**Revision (v4 §5.2, §6.1, §6.8).** Every VM attach mints a fresh incarnation
`vm_id`. The workspace token carries `vm_id` as a claim; renewal is refused for
any `vm_id` that is not the current one in `workspace_vms`. Git receive-pack,
`chat_session_files` writes, task events, session-file harvests, and the LLM
proxy all reject a stale incarnation. Placement and lease renewal are single
compare-and-swap updates on the row. A host that reconnects reports its running
VMs; the API orders `vm.stop(checkpoint=false)` for any VM whose incarnation is
not current, and the host kills it. Lease expiry on the host side also stops the
VM locally. This is one workspace-level fence, not the old per-task lease
system. Tests: network partition with late reconnect, in addition to host kill.

## 3. Task dispatch and event recovery protocol — accepted

**Validation.** Correct on all three points: a crash between insert and RPC
strands the task; a lost spawn reply can double-launch; a lost exit event is
never repaired by a reconciler that only checks VM state.

**Revision (v4 §7.3, §7.6).** Execution identity is `(task_id, attempt,
vm_id)`. Dispatch is a periodic loop over `queued` tasks whose VM is running,
not a one-shot call at insert time, so an API crash after insert is harmless.
`agent.run_turn` and `proc.spawn` are idempotent on that identity: the
dispatcher returns the existing process if it already started. Every event
carries a per-VM monotonic sequence number; the dispatcher retains terminal
outcomes until the API acknowledges them, and the API persists the task
transition and the corresponding chat message update in one transaction before
acknowledging. On (re)connect the API reconciles its `running` tasks with the
dispatcher's process and outcome inventory: unknown to the dispatcher →
`failed(vm_lost)`; finished but unacknowledged → apply now. The reconciler
loop covers VM-down and question expiry as before. Tests: disconnect before
and after spawn, disconnect at process exit, API restart with the VM alive.

## 4. Stop as a durability barrier, including suspended questions — accepted with modification

**Validation.** Correct: checkpoint-then-stop loses writes that land between
the snapshot and the kill, and a `waiting` task whose provider is blocked
mid-turn cannot be resumed from a transcript harvested only at turn end.

**Revision (v4 §5.7, §7.2).** Stop is now an ordered barrier: close admission
(new Save, run, preview, terminal requests are refused with
`WORKSPACE_STOPPING`), quiesce processes, checkpoint every repository and wait
for the push to succeed, harvest session files, then stop the VM. If any
repository's checkpoint push fails, stop fails: the VM stays running, the
failure is surfaced, and the stop is retried; host drain waits; an operator
force-stop is explicit and acknowledges the loss.

The modification concerns waiting tasks. Rather than suspend a provider
mid-turn (which requires resuming a transcript with a pending tool call the
CLI may not accept), draft 4 treats an open question as activity: a `waiting`
task keeps the VM awake until its question expires (default 60 minutes, the
same bound today's sticky pending-question window uses), so idle stop never
meets a waiting task. Explicit stop and host drain interrupt the turn: the
provider gets SIGINT, the turn ends with the new outcome `interrupted` (partial
output kept), and the question stays answerable; when the answer arrives it
starts a new turn whose prompt carries the answer and a note that the previous
turn was interrupted while waiting. An answer racing with stop is delivered
once because the question row is settled in the same transaction that either
resumes the process or records the interruption. This changes the round-1
detail "waiting does not hold the VM awake" and needs the user's confirmation
(Decision A below).

## 5. Codex transport cannot deliver native approval answers — accepted with modification

**Validation.** Confirmed in code: `sidecar_source.mjs` maps
`exec_approval_request` and its siblings to a hard `CodexApprovalUnsupported`
error because the one-shot stdin is already closed, and the shipped default
preset is `on-failure/workspace-write`. Draft 3 wrongly claimed Codex approvals
map onto `task_questions` like Claude's.

**Revision (v4 §8.3).** Draft 4 restricts Codex chat turns to non-prompting
policies (`approval_policy=never` with `sandbox_mode=workspace-write`, or the
operator's choice among policies that cannot prompt under `exec`), which is
today's effective behavior. `request_permission` applies to Claude only. The
fake Codex CLI never emits approval frames, so it cannot conceal the gap. The
alternative the review points to, the Codex App Server protocol with a process
bounded to one turn, is recorded as a later option and not designed here; it
would change the per-turn invocation from `codex exec resume` to a JSON-RPC
session. Whether interactive Codex approvals are required is Decision B.

## 6. Bind approval, publication, and merge to one source SHA — accepted

**Validation.** Correct. Draft 3 refreshed `head_sha` on every internal push
but published to the provider only on explicit actions, so the provider PR
could sit at an older head than the approved one, and the merge step re-read a
mutable branch.

**Revision (v4 §10.2, §10.3, §10.6).** The merge operation reads the source
and target OIDs from git once, at start, and pins them for the rest of the
operation. Decisions are evaluated for that exact source OID. If the provider
PR head differs from the pinned source OID, the operation republishes the
source branch to the provider first and verifies the provider head. The
provider merge call passes the expected source SHA (`sha` on GitHub, `sha` on
GitLab), so the provider refuses a stale head. The agent review task is given
the SHA it reviews and `g5d pr review` records it. Tests: internal push after
initial publication; source push racing with merge.

## 7. Replace mirror-first merge and rollback — accepted

**Validation.** Correct, and today's code already follows the better order for
base branches ("never mint a local Base commit ahead of GitHub" in
`routes/workspaces.rs`).

**Revision (v4 §10.6).** Merge is a persisted operation with pinned inputs and
a state (`pending`, `provider_merged`, `applied`, `failed`) on the PR row. For
provider-connected targets the provider performs the merge; on success the
mirror imports the provider's resulting target head through the normal inbound
fetch (so the mirror never computes its own merge commit and never diverges by
merge method). An ambiguous provider response (timeout, lost reply) is
reconciled by reading the provider PR state and `merge_commit_sha` and retried
idempotently. Internal target updates from inbound fetch and from local-only
merges are serialized per repository. Local-only repos keep the git
compare-and-swap merge. Synchronous rollback is removed. Tests: crash before
and after the provider call, lost success reply, inbound target update during
merge.

## 8. Receive-pack CAS does not enforce force-with-lease safety — accepted

**Validation.** Correct: a plain `--force` sends the just-advertised remote
head as `old_sha`, so the server's compare-and-swap passes even though the
client never incorporated that commit. The server cannot distinguish `--force`
from `--force-with-lease`.

**Revision (v4 §5.3).** The policy text now says what the server does and does
not provide: workspace branches accept arbitrary rewrites from the workspace's
own identities; the server's check only rejects updates whose `old_sha` no
longer matches (a ref that moved during negotiation) and rejects zero-old-sha
updates of existing refs; lease safety is a client obligation, and the agent
profiles instruct `--force-with-lease`. In the shared-folder model all agents
in a VM share one local clone, so the unseen-remote-commit case arises only
between the VM and a user pushing from a laptop, which is the laptop semantics
the user asked for. Test added for the unseen-remote-commit case to document
the behavior. Whether this documented behavior is acceptable is Decision C.

## 9. Keep provider session directories outside the git snapshot — accepted

**Validation.** Correct. `/work` is the workspace directory and member repos
mount under their `root_path`; nothing guarantees a customer repository ignores
`.g5/`, and `git add -A` would capture transcripts.

**Revision (v4 §6.3, §8.4, §11.4).** Provider runtime state moves outside every
working tree: `CLAUDE_CONFIG_DIR=/home/agent/.claude`,
`CODEX_HOME=/home/agent/.codex`, dispatcher state in `/var/lib/g5d`. Paths are
stable across VM incarnations because the dispatcher recreates them on restore.
In addition, the checkpoint command uses a platform-managed excludes file
(`core.excludesFile`) that excludes `.g5/` runtime subdirectories, and the same
file is set for agent git invocations so a native `git add -A` does not capture
them either. Test: both providers against a customer repo with no `.gitignore`.

## 10. Projections need a repair path independent of event delivery — accepted

**Validation.** Correct: the event bus is a non-blocking WebSocket fan-out
(`services/mod.rs`), and a crash between ref update and publish loses the
event.

**Revision (v4 §5.3, §9.6).** Events are hints. Each projection (docs,
ontology, search, `head_sha_cache`, PR `head_sha`) records the OID it was
computed from per `(repo, ref)`. A convergence loop runs at startup and every
60 seconds: it lists authoritative branch and checkpoint OIDs and re-projects
whatever differs. Fork, merge, inbound fetch, checkpoint, and deletion all go
through the same contract. Test: a successful ref update with its event
dropped converges without another push.

## Correction: `git status` is not conflict detection — accepted

**Validation.** Correct: concurrent overwrites of one file leave one modified
file with the last writer's content; git records no conflict.

**Revision (v4 §6.3).** The text now states the lost-update semantics: last
writer wins, earlier edits to the same file are overwritten, and git shows only
the final state. Conflicts appear only at rebase or merge time. The notice-only
policy is unchanged.

---

## Decisions needed

These are the points where the accepted revision changes an earlier answer or
chooses between alternatives the review left open.

- **A. Waiting tasks and idle stop (finding 4).** Draft 4 keeps the VM awake
  while a question is open (bounded by the question's expiry, default 60 min)
  and interrupts the turn on explicit stop or drain. Round 1 said a waiting
  task does not hold the VM awake. Confirm the new rule, or ask for
  mid-turn suspension (which needs a proven transcript-resume path with a
  pending tool call).
- **B. Codex interactive approvals (finding 5).** Draft 4 restricts Codex chats
  to non-prompting policies, as today. If interactive Codex approvals are
  required, the per-turn invocation must become the App Server JSON-RPC
  protocol bounded to one turn, and that needs a verified prototype before it
  is designed.
- **C. Force-push semantics (finding 8).** Draft 4 documents that workspace
  branches allow owner rewrites and that lease safety is a client obligation.
  Confirm, or ask for a rewrite-authorization protocol (a server check that
  the new head contains the previous head, which forbids rebase on a shared
  branch).
- **D. Explicit stop while tasks run (finding 4).** Draft 4 cancels running
  tasks (SIGINT, then SIGKILL) before the checkpoint; the alternative is to
  refuse the stop until tasks finish. Idle stop is unaffected because it
  requires no running tasks.

## Ambiguities noted while responding

- The review's finding 9 says session directories are "inside the checkout".
  `/work` contains the member repos at their `root_path`, so `/work/.g5` is a
  sibling of the repos in the common layout, not inside a repo working tree.
  The finding still holds for layouts where a repo's `root_path` is the
  workspace root, and moving state out of `/work` removes the question.
- The review asks for "small executable acceptance tests" before phase
  estimates are treated as commitments. Draft 4 lists them per finding in §15;
  they are part of the phase exit criteria, not a separate gate.
