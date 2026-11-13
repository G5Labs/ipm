---
name: issue-626-pr-634
description: "PR #634 (issue 626) VM custody fix — scope, design, deferred findings, and ready-for-ship watch."
metadata:
  node_type: memory
  type: project
  originSessionId: 1a339771-9cef-46b0-8410-d67363745eac
  modified: 2026-10-18T08:43:41.478Z
---

# Issue 626 / PR 634: VM custody fix

PR [#634](https://github.com/G5Labs/G5-Lean/pull/634) on branch
`issue-626-preserve-vm-custody-through-ambiguous-starts` implements G5-Lean#626
(preserve VM custody through ambiguous Starts, concurrent Stop, rotation).

**Design:** `.g5/pr_ontology_diff_design/issue-626/ontology-plan.md`. New ontology
blocks `g5.runtime.start.settlement` (hosts) and `g5.lifecycle.transfer.admission`
(workspace_vm); `g5.runtime.host.rotation` metadata updated. Migration 097 (renumbered after main's 093-095 and a sibling PR's pending 096) adds
`workspace_vms.start_command_sent_at`.

**Core mechanism:** host absence frees a placed incarnation only when its Start is
settled/undeliverable. Control plane stamps `start_command_sent_at` under the row
lock and enqueues `vm.start` in one txn (`HostCommandClient::dispatch`); tunnel
publishes the command client only after `accept_hello`; host keeps a
`pending_starts` registry (register at request dequeue in `ctl.rs::execute` via
`admit_request`, reports booting in hello `inventory()` and `vm.status`, NOT in the
partial heartbeat, cancels on Stop). `idle_quiesce` locks the host row then re-reads
occupancy. `complete_boot` joins an owning stop's `Busy` via `await_reservation_release`.

**Review round 2 (2026-10-19, independent Codex review posted as zcl-g5):** the phantom
`reported_live_vm_ids` occupancy is fixed on the control-plane side — every settled
non-admission release (`release_refused_placement`, `release_failed_start`,
`release_stopped`) retracts the id after its fenced update matched, host row locked
first (placement/hello order); no synthetic exit, heartbeats never retract. The exact
concurrent-Stop `Busy` race is forced by a test-only journal barrier
(`EventJournal::hold_writes`). S32 (process waiter) unchanged, out of scope.

**Validated locally (Postgres 17, pinned 1.97.1):** host lib pending-start + journal-fail
tests; API tests hello-fence, rotation race, transfer admission; 121-test regression over
api_workspace_vms/api_host_capacity/rotation; clippy clean. Reflink materializer host
tests fail locally (no loop device) — environmental, CI runs them under XFS
([[xfs-reflink-loop-mount-for-host-tests]]).

**Ready-for-ship watch:** green g5-ci-rust + g5-ci-e2e (+ terraform only if applicable);
address Codex P0/P1 and CodeRabbit Critical/Major inline; no merge conflict. Reviews
(@chatgpt-codex-connector, @coderabbitai, @zcl-g5) post up to 30 min after each push.
Follow [[bot-review-scope-discipline]]: realism-check each finding, don't grow the diff
for unreachable cases, don't re-trigger Codex per push.

**Watcher lesson (2026-10-18):** `git fetch origin main` only updates `FETCH_HEAD`; it does
not create or move `refs/remotes/origin/main` in this workspace clone, so a poller reading
`HEAD..origin/main` reported "behind 0" while main advanced 6 commits. Always fetch with
`git fetch origin main:refs/remotes/origin/main` (or compare against `FETCH_HEAD`) before
judging merge drift. Also: do not end a requested monitoring loop unilaterally; keep one
long watcher alive and report, or ask.
