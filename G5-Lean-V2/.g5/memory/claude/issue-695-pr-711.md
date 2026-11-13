---
name: issue-695-pr-711
description: Issue #695 (runtime secret admission + bounded terminal retention) shipped as PR #711 on branch FVB-i660-F6-runtime-secret-admission-and-terminal; scope, decisions, and the watch state as of 2026-10-22.
metadata:
  type: project
---

PR #711 (branch `FVB-i660-F6-runtime-secret-admission-and-terminal`, workspace
ce9ab075) fixes #695. Design: `.g5/pr_ontology_diff_design/issue-695/ontology-plan.md`;
ontology blocks `g5.chat.secret-admission`, `g5.terminal.secret-admission`,
`g5.terminal.output-retention`.

Decisions a reviewer may question, and why they stand:
- Shared policy limits (64 KiB value, 128 entries, 256 KiB serialized) are bounded by the
  terminal `open` frame; the module path keeps its looser guest parser (direct RPC tests
  inject `HOME`/`G5_TEST_*`).
- Unconfirmed attempts with a now-invalid set re-invoke with a `secret_preflight_error`
  (attachment-preflight pattern), never a cancelled disposition: a lost invoke response may
  mean the guest is running the turn; reservation makes no network calls, so no status probe.
- A candidate settled at admission continues the pass (`QueuedCandidate` loop, bound 8); the
  admission notice carries NULL task/attempt; enqueue rebinds kept inputs when a fresh turn
  is created.
- Loader variables stay unreserved (#535/#576).

State 2026-10-22: PR open, merged main (#692) once (only MEMORY.md conflicted). First CI run
failed two static guards (core-table writers: the chat notice insert had to live in
chat_service.rs; route-auth audit: terminal_ws_adapter must keep ticket < open_owner_stream <
scoped_db_for_workspace < `let terminal_id` order) plus a timing flake in
`agent_execution_tree_is_reaped_before_settlement` (passes locally, 61 s). CodeRabbit's first
review: 5 findings, all addressed (omit `secret_preflight_error` when None for older guests
with deny_unknown_fields; settle the preview intent on admission failure; ontology block
splits and metadata order).
Local Postgres container `g5-test-pg` (unlabeled `docker run`, port 5432) still running for
reruns; remove it before handoff.
