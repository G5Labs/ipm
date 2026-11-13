# Issue #403: QA API restart recovery

## Human request

Work on [G5Labs/G5-Lean#403](https://github.com/G5Labs/G5-Lean/issues/403), reading it with `g5d issue show` first. The issue asks to remove the redundant worker-pin restart after confirming the deploy gate, provide readable non-JSON HTTP errors, recover loaded PR destinations after an outage, and acquire WebSocket tickets through the refreshing API client. It requires Vitest regressions, the existing WorkspacePageBasePull suite, and ontology updates for D35 and workspace recovery.

## Agent interpretation

Implement the issue in this branch. Let the gated deploy alone install the API worker pin. Retry transiently failed, already-loaded destinations on subsequent successful workspace-detail reads, preserving their warning until their own read succeeds. Reuse API authentication and WebSocket backoff. The larger deployment-availability design in #402 is outside this change.

The initially checked-out authorized branch is `remove-redundant-api-restarts`, at `a094de28efa1a18734398a3a49c66469ee5d7e40`; append and push only this branch. No deployment is requested.
