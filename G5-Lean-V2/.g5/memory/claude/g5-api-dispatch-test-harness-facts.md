---
name: g5-api-dispatch-test-harness-facts
description: Three g5-api integration-harness facts that cost reruns on #695 — workspace config routes need ?orgId=, unconfirmed attempts retry only after a 2 s backoff, and chat_messages bind (task_id, attempt) to task_attempts.
metadata:
  type: project
---

Learned writing `api_runtime_secret_admission.rs` (2026-10-22, #695):

- `PUT /api/v1/workspaces/{id}/config` requires `?orgId=<org>`; without it the route
  answers 400 "missing field orgId" before any handler logic.
- `load_unconfirmed_agent_dispatch` only picks attempts with
  `tasks.updated_at <= now() - interval '2 seconds'`; a test that reserves right after
  `agent_tasks::run_once` must backdate `updated_at` first.
- `chat_messages` has `chat_messages_attempt_fk (org_id, task_id, attempt) → task_attempts`
  and a pair check `(task_id IS NULL) = (attempt IS NULL)`: a notice for a task that never
  reserved an attempt must carry NULL task/attempt, or the whole reservation transaction
  rolls back and the task silently stays queued.
- `TestOrg::create_with_owner` seeds an org-level `ANTHROPIC_API_KEY`, so effective
  secret sets in tests contain it.

**Why:** each of these failed a test with an opaque symptom (400, `None`, "queued").
**How to apply:** when a dispatch test leaves a task queued with no error, suspect a
rolled-back transaction from a constraint, not the loop. See [[rust-and-integration-test-setup]].
