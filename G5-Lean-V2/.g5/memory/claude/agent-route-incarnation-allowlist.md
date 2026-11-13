---
name: agent-route-incarnation-allowlist
description: A new /api/internal/workspace-agent route answers 401 to the VM token until its method+path is added to the incarnation allowlist in auth_middleware.rs.
metadata:
  type: project
---

Workspace-incarnation (VM) tokens are accepted only on paths matched in `backend/g5-api-core/src/middleware/auth_middleware.rs` (the slice-pattern `match` around the `"workspace-agent"` arms). Registering a route in `routes/workspace_agent.rs` is not enough: the request fails with 401 "Missing or invalid authentication token" before reaching the handler, which looks like a bad token.

**Why:** hit on 2026-10-24 adding `POST .../pull-requests/:id/auto-merge/disable` (merge when ready); the agent test got 401 until the arm was added.

**How to apply:** add a `(Method, [..segments..])` arm next to the similar routes whenever adding an agent route. Leaving a path off the list is also a deliberate guard: merge when ready's turn-on path is absent so a VM token can never reach it. Related: [[route-auth-static-audit-gotchas]], [[merge-when-ready]].
