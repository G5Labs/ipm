---
name: g5-api-tests-sign-in-and-org-creation
description: Since #776, g5-api integration tests must invite an identity before an OAuth login and create cascade orgs through helpers::platform (no POST /api/v1/orgs).
metadata:
  type: reference
---

Since G5-Lean #776 sign-in admits only pending invitations or memberships (no allowlist),
and self-serve `POST /api/v1/orgs` is gone (405).

**How to apply:** in `backend/g5-api/tests`, call
`helpers::platform::invite_for_sign_in(&app, email)` before an OAuth callback login of a
fresh identity (it creates an inviter user — use a direct membership instead if the test
counts users). Create cascade orgs with `helpers::platform::create_org_via_operator(&app,
&owner, json!({...}))`, which returns a response-like value in the old cascade shape
(`org`, `baseWorkspace`, `docsRepo`, `codeRepo`). Validation errors are 422, not 400.
Related: [[g5-api-test-binaries-fill-disk]], [[issue-776-platform-operator]].
