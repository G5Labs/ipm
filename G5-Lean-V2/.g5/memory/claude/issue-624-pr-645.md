---
name: issue-624-pr-645
description: "Personal workspaces PR #645 — 2026-10-24 review round (migration 111, stop_view, image fallback) and the findings declined as out of scope."
metadata:
  node_type: memory
  type: project
  originSessionId: 3990055b-6f6a-41a9-84f0-bb200640e996
  modified: 2026-10-24T13:37:11.114Z
---

PR G5Labs/G5-Lean#645 (issue #624, branch `issue-624-personal-workspace-per-user-chat-and-persistent`) ships personal workspaces.

On 2026-10-24 all 25 CodeRabbit threads were closed at head `95df4defc`:

- Fixed: migration `111_personal_retirement_stop_supersession.sql` (owner_removed may supersede a pending nudge in the identity fence), `stop_view` for tombstoned personal rows, org-default image for a personal workspace with no Base code pins, `may_be_unborn` in the expected-unborn cut, login `ensure_after_login` spawned, dispatcher reference-root symlink and OID checks, per-handler route audit for `personal.rs`, panel "Queued" only for a recorded non-running Start, Home chat refusing a send while the selected session loads, purpose block split.
- Declined as not applicable (replied on the thread; CodeRabbit withdrew each): MD041 H1 in memory files (convention is prose after frontmatter), `fail_task_dependents` on start-ended cancels (no production writer of `task_dependencies`), reconciler branch-retry window past 64 (owner's `/me` repairs it), replayed `start_session` with `startCompute` (no client sends `requestKey` with `startCompute`).

**Why:** a later round may re-raise the same findings; the reasons above are what CodeRabbit accepted.

**How to apply:** if a new finding repeats one of the declined cases, answer with the same reason rather than implementing it ([[bot-review-scope-discipline]]). Adding a migration needs a `checksums.sha384` line (`scripts/check-migration-checksums.py`) and a recache of `g5-api-data` ([[sqlx-migrate-embed-incremental-cache]]).
