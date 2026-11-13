---
name: g5-api-tests-no-scoped-tracing-subscriber
description: "g5-api integration tests that assert on log events must use helpers::tracing_capture (global), not tracing::subscriber::set_default — scoped subscribers drop events when other modules run in parallel."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 6d51ceb7-e70d-4da7-9d88-d278ea06b47a
  modified: 2026-10-18T06:13:37.243Z
---

In `backend/g5-api/tests`, a test that installs its own subscriber with
`tracing::subscriber::set_default` passes alone and within its module, but drops
events when the whole `integration` target runs in parallel. Tracing's shared
callsite-interest cache races with the process-wide subscriber from
`helpers::tracing_capture`. Seen 2026-10-18 on #621: `email_delivery::delivery_logs_carry_no_credential_recipient_or_content`
lost its "Email accepted by provider" line.

**Why:** CI runs every module in one process, so the flake shows up there and not
in a filtered local run.

**How to apply:** call `helpers::tracing_capture()`. Add the target you need to its
`filter_fn`, keeping the buffer bounded, and filter the captured events by target
and your own ids. Reproduce suspected flakes by running the suite together with other
modules (for example `api_orgs::`), not alone. Related: [[g5-api-test-binaries-fill-disk]].
