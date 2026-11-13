# Security audit: PR G5Labs/G5-Lean#280 (issue #278)

Scope: `git diff bce87811 HEAD` on `fix/approval-requirement-org-admin`. Read-only audit; no
repository files changed. Paths are relative to the repository root unless absolute.

Changes audited:
- `backend/g5-api-routes/src/routes/workspace_approvals.rs`: `attach_requirement` and `retire_requirement`
  now request only `["org:admin"]` and drop `ensure_workspace_owner` (lines 563-568, 599-601).
  `record_decision` uses `["workspaces:read"]` with no owner check when `requestId` is present, and keeps
  `workspaces:write` plus `ensure_workspace_owner` when it is absent (lines 633-651).
- `infra/docker/oauth-stub/server.js`: the manual-consent callback now honours the typed `email`/`login`
  and derives a new numeric id from `sha256(lower(email))` (lines 240-263).
- `frontend/e2e/oauth.ts` and `frontend/e2e/flows/org-approvals.spec.ts`: a new deterministic org-admin
  identity (id 100004) and a browser flow.
- `backend/g5-api/tests/api_workspace_approvals.rs`: a new org-admin test. The nonowner reviewer now
  settles without `set_current_owner`, and a non-owner settling another actor's request now expects 409
  instead of 403.

## Summary

No critical, high or medium vulnerabilities were found. The relaxed paths still have, in order: tenant
resolution under RLS, workspace read-access enforcement, a locked per-transaction scope check
(`authz.lock_user_scope_authority`, which also takes `FOR SHARE` on the live workspace row and the
membership row), and for decisions, an exact `(org, source workspace, definition, request, actor = caller)`
row lock in the service.

| ID | Severity | Origin | Title |
|----|----------|--------|-------|
| SEC-1 | Low | Introduced (made reachable) by PR | A nonowner settlement creates a successor request that neither the owner nor the reviewer can cancel or replace |
| SEC-2 | Low | Pre-existing; PR widens it | The `/decisions` exempt route does not enforce the token scope ceiling, so a narrowly scoped API key can record a *human* approval |
| SEC-3 | Info | Introduced by design (issue #278) | Any org admin can now unilaterally retire a Base merge requirement, with no notice |
| SEC-4 | Low (dev/CI only) | Introduced by PR | The OAuth stub process crashes on a repeated `email`/`login` query parameter |
| SEC-5 | Info (dev/CI only) | Pre-existing; context changed by PR | The OAuth stub consent page has reflected XSS and an open redirect, and typed emails link to existing accounts |
| SEC-6 | Info | Introduced by PR (test hygiene) | The e2e flow leaves a persistent extra org admin in the shared e2e org |

---

## SEC-1: A nonowner settlement creates an uncancellable, unreplaceable successor request

- **Severity:** Low (a governance-integrity and revocation gap, not privilege escalation)
- **Origin:** The code is pre-existing, but this PR makes it reachable. Before the PR only the workspace
  owner could settle a request, so the successor's `requested_by` was always the owner at that time.
- **Affected code:**
  - `backend/g5-api-core/src/services/approval_config_service.rs:1515-1530`: the successor insert binds
    `requested_by = $2 = actor_user_id`, which is the reviewer.
  - `backend/g5-api-core/src/services/approval_config_service.rs:1200-1215`: cancellation requires
    `requested_by = caller`.
  - `backend/g5-api-routes/src/routes/workspace_approvals.rs:741-751`: the cancel route requires
    `workspaces:write` and `ensure_workspace_owner`.
  - `approval_config_service.rs:1112-1123`: `create_human_requests` uses `ON CONFLICT … WHERE state = 'pending' DO NOTHING`
    and reuses the existing pending row.
- **Exploit scenario:**
  1. The owner O of fork F addresses reviewer R with `POST /workspaces/F/approvals/requests` for definition D.
     Request r1 has `requested_by = O`.
  2. R, who is not the owner, sends `POST /workspaces/F/approvals/decisions {definitionId: D, requestId: r1, outcome: "approved"}`
     and gets 200. r1 is settled, and the service inserts successor r2 with `actor_user_id = R` and `requested_by = R`.
  3. O tries to withdraw the coordination with `POST …/requests/r2/cancel` and gets **404**, because
     `requested_by ≠ O`. R's own cancel attempt gets **403** because R is not the owner. O re-posting the
     request gets r2 back as `reused`.
  4. R keeps a standing ability to record `approved`, `rejected` or `deferred` on F for D for as long as R
     remains an org member with read access. If R is a listed approver of D, each new decision flips the
     gate; a `rejected` blocks the merge. A discretionary addressee who is not listed in `definition.humans`
     does not affect the gate (`approval_config_service.rs:2111`), but can append unlimited entries to F's
     approval history.
- **Evidence:** Code inspection of the lines above. The existing test
  `requester_only_human_request_cancellation_preserves_decision` (`api_workspace_approvals.rs:613-735`)
  covers only the case where the owner is also the actor, so the successor's requester equals the owner.
  The updated `explicit_human_request_admission_and_settlement` now settles as a nonowner (`second`) and
  never checks who can cancel the successor.
- **Recommended fix:** Pick one:
  - Carry the original `requested_by` forward into the successor: `SELECT …, requested_by, expires_at`
    instead of binding `$2`. The person who asked keeps control of the chain.
  - Or let the current workspace owner, as well as the stored requester, cancel a pending request.
  - Record the chosen rule in `g5.governance.approval` or the cancellation contract.
- **Regression test:** O requests R, and R (nonowner) approves. Assert that O can cancel the successor
  (200), that R cannot (403), and that a non-addressed member cannot (403 or 404). Assert the decision
  log is unchanged afterwards.

## SEC-2: `/decisions` does not enforce the token scope ceiling, so narrowly scoped API keys can record human approvals

- **Severity:** Low
- **Origin:** Pre-existing. The route has always been `exempt_route`, so `RequireScopes` never ran. This PR
  extends the gap from owners to every addressed reviewer, and lowers the database floor from
  `workspaces:write` to `workspaces:read`.
- **Affected code:**
  - `backend/g5-api-routes/src/routes/workspace_approvals.rs:486-491`: `/decisions` is `exempt_route`.
    `cancel_human_request` at 497-502 has the same pattern.
  - `backend/g5-api-core/src/db/scoped.rs:564-588`: `authorize_org_scopes` checks only that the API key's
    `claims.org` matches the target org. Scopes are resolved from database role, grants and overrides,
    never from the token's own claims.
  - `backend/g5-api-core/src/middleware/auth_middleware.rs:628-660`: API-key auth builds `AuthContext::User`
    with the key's arbitrary scope subset.
- **Exploit scenario:** Admin A creates `g5k_…` with scopes `["docs:read"]` for a CI bot
  (`routes/api_keys.rs:59-73`; the key acts as A). A is addressed as a reviewer on fork F. The bot, or
  anyone holding the leaked key, sends `POST /workspaces/F/approvals/decisions {definitionId, requestId, outcome: "approved"}`.
  The route layer skips the scope check, and the database check needs only `workspaces:read`, which A's
  role grants. The result is 200, recorded as `actor_type = 'human'` with `actor_user_id = A`. This
  conflicts with `g5.governance.approval` ("Formal approval belongs to an eligible human …") and
  `g5.security.login.service-identities`.
- **Evidence:** Code path above. `route_auth_static` passes with `record_decision` classified as exempt
  (`backend/g5-api/tests/route_auth_static.rs:956-959`). No test sends a narrow API key to `/decisions`.
- **Recommended fix:** Register `/decisions` with `audited_route(…, &[Scope::WorkspacesRead])`, or check
  the claims in the handler. For requestless decisions, also require `WorkspacesWrite` in the token. Decide
  explicitly whether API-key principals (`iss = g5-api-key`) may record a *human* approval at all; the
  ontology suggests rejecting them. Apply the same treatment to `/requests/:id/cancel`.
- **Regression test:** Mint an API key with `["docs:read"]` for an addressed reviewer. `/decisions` with
  `requestId` should return 403 and write no `approval_statuses` row. A key with `["workspaces:read"]`
  should behave as policy decides (403 if API keys are excluded).

## SEC-3: Org admins can unilaterally retire Base merge requirements (by design)

- **Severity:** Info
- **Origin:** Introduced intentionally (issue #278, `g5.governance.approval.requirements`).
- **Affected code:** `backend/g5-api-routes/src/routes/workspace_approvals.rs:593-622`.
- **Scenario:** Before this PR, retiring a requirement on Base needed someone who was both an org admin
  and Base's owner. Now any org admin can do it. An admin who wants to merge without sign-off can retire
  the requirement, merge, and re-attach it. `approval_configs` is append-only and records `created_by`,
  so the action is attributable and visible in the attachment history. Every POST also emits a
  `g5_audit` `user_action.http` event (`backend/g5-api-core/src/middleware/audit_log.rs`), but nobody is
  notified.
- **Recommendation:** None required for this PR. If separation of duties matters for P6, consider
  notifying Base's owner and the listed approvers on retire, or requiring a second admin.

## SEC-4: The OAuth stub crashes on a repeated `email`/`login` parameter

- **Severity:** Low (dev and CI only, denial of service of the stub)
- **Origin:** Introduced by this PR. A similar crash with a missing `redirect_uri` is pre-existing.
- **Affected code:** `infra/docker/oauth-stub/server.js:249-250`. `url.parse(…, true)` returns an array
  for a repeated key, and `(email || '').trim` is then not a function. The `async` request handler's
  rejection is unhandled, and Node exits the process.
- **Scenario:** Anyone who can reach port 9876 (dev compose publishes `"9876:9876"` on all interfaces,
  `infra/docker/docker-compose.dev.yml:102`) and has a valid code, or who gets the browser to submit one,
  sends `GET /login/oauth/authorize/callback?code=<c>&redirect_uri=x&email=a&email=b`. The stub exits.
  `restart: unless-stopped` brings it back, but in-memory codes and tokens are lost.
- **Evidence:** I reproduced this on a scratch instance on private port 19877, not the dev stack. The
  process exited with `TypeError: (email || "").trim is not a function at server.js:249`. The baseline
  stub (`bce87811`) also exits when `redirect_uri` is missing.
- **Recommended fix:** Normalise each parameter
  (`const first = v => Array.isArray(v) ? v[0] : v;`), require `redirect_uri`, and wrap the handler body
  in `try/catch` that returns 400.
- **Regression test:** A small node test (or a Playwright `request` call) that sends a duplicate
  `email`, then asserts a 302 or 400 and that `/healthz` is still 200.

## SEC-5: OAuth stub has reflected XSS, an open redirect, and email-based account linking (dev and CI only)

- **Severity:** Info
- **Origin:** Pre-existing. The PR does not change the consent HTML, but it makes the consent page the
  identity selector for the default dev stack (`OAUTH_AUTO_APPROVE: "false"` in `docker-compose.dev.yml`).
- **Affected code:** `infra/docker/oauth-stub/server.js:226-232` interpolates `scope`, `state`,
  `redirect_uri` and the cookie-derived `user.email`/`user.login` into HTML without escaping.
  `server.js:264-266` redirects to the caller-supplied `redirect_uri` without binding it to
  `codes.get(code).redirect_uri`.
- **Evidence (scratch instance):** `GET /login/oauth/authorize?client_id=local-oauth-client-id&redirect_uri=x"><script>alert(1)</script>&scope=<img src=x onerror=alert(2)>`
  reflected both payloads verbatim. `GET /login/oauth/authorize/callback?code=nope&redirect_uri=https://evil.example/`
  returned 302 to `https://evil.example/?code=nope&state=`.
- **Identity behaviour:** The new callback mints a new GitHub id per typed email, and the stub reports
  every email as `verified: true`. `AuthService` links logins by verified email
  (`backend/g5-api-core/src/services/auth_service.rs:1051-1085`). So typing an *existing* user's email
  signs in as that user and rewrites their `oauth_id` to the hashed id. A later login with the default
  id flips it back. The stub comment "each email a distinct account" is therefore inaccurate for existing
  emails. This adds no power beyond the existing unauthenticated `POST /__e2e/identity`, because the stub
  is already a deliberate authentication bypass. Id collisions: hashed ids fall in [200000, 100199999].
  That range excludes the seeded 100001-100004 but includes the SCM PAT identity 424242, which is only
  used by the separate PAT-validation flow; the chance of a collision is about 1e-8 per email.
- **Deployment check:** The stub is referenced only by `infra/docker/docker-compose.dev.yml`,
  `infra/docker/docker-compose.test.yml`, `infra/ci/cloudbuild-ci-e2e.yaml` and
  `infra/docker/test-dev-build-profile.sh`. No Terraform, Helm or production config references it.
  The backend points GitHub OAuth at the stub only through explicit `G5_GITHUB_*_URL` overrides
  (`backend/g5-api-foundation/src/config.rs:161-173`). Non-dev deployments are not weakened.
- **Recommendation (optional hardening):** HTML-escape the interpolated values, bind the callback
  `redirect_uri` to the code's stored value, and bind the dev port to `127.0.0.1:9876:9876`.

## SEC-6: The e2e flow leaves a persistent extra org admin

- **Severity:** Info (test hygiene, no production impact)
- **Affected code:** `frontend/e2e/flows/org-approvals.spec.ts`. `useNonOwnerOrgAdminIdentity` invites
  `e2e-org-admin@g5.dev` as `admin` into the shared `TEST_ORG`, and the `finally` block only resets the
  stub identity.
- **Impact:** Later specs in the same run see an extra org admin, which could mask role-based assertions.
  The helpers are not bundled into the app: `frontend/e2e` is used only by the Playwright config, and
  nothing under `frontend/src` imports it.
- **Recommendation:** Remove or demote the membership in `finally`, or use a per-test org.

---

## Verified safe, and how

1. **Member or viewer attach/retire is denied.** The route layer `RequireScopes([OrgAdmin])` checks the
   token claims (`workspace_approvals.rs:476-485`, `auth_middleware.rs:851-868`). The database check
   `require_user_scopes_on_conn([org:admin])` resolves role defaults; only `Owner`/`Admin` get `OrgAdmin`
   (`auth_service.rs:3726-3740`). The new test asserts 403 for a member.
2. **Other tenants are denied.** `scoped_db_for_workspace` resolves the workspace under RLS over only the
   caller's orgs (`scoped.rs:856-873`, `resolve_workspace_context` at 239), so the result is 404. API keys
   are also bound to `claims.org` (`scoped.rs:572-576`). For a multi-org JWT with `org:admin` in org A
   targeting a workspace in org B, the token ceiling passes but the locked database check uses B's role,
   giving 403. The test asserts 403 or 404 for cross-tenant attach and retire.
3. **Worker, host and agent bearers are denied.** A host JWT on a public path is rejected with 401
   (`auth_middleware.rs:668-677`). `authorize_org_scopes` requires `AuthContext::User` claims. VM and
   incarnation credentials are admitted only on internal surfaces.
4. **Removed members are denied, and removal is serialized.** `mint_db_capability` rejects non-members.
   `lock_user_scope_authority` takes `FOR SHARE` on the `org_memberships` row, so a concurrent removal
   waits for the decision or attach to commit. `record_human_decision` re-checks current membership
   (`approval_config_service.rs:1306-1319`).
5. **The guards removed from attach/retire and request-addressed decisions are not lost:**
   - Deleted workspaces are filtered by both `resolve_workspace_context` and
     `lock_user_scope_authority` (`deleted_at IS NULL`).
   - Ownership transfer and soft delete still serialize. Migration 042
     (`backend/g5-api/src/db/migrations/042_scope_workspace_liveness_lock.sql:56-66`) takes
     `FOR SHARE OF workspace` whenever a workspace id is passed, which `scoped_db_for_workspace` always
     does. This is the same lock the dropped owner path took.
   - The pending-transfer `WorkspaceStopping` check is no longer applied. Neither operation depends on
     ownership, and transfer does not touch approval tables, so this is acceptable.
   - Read-only and freezing mutation policy was never consulted by the old or the new path. Approval
     administration and decisions are not workspace-content writes
     (`workspace_mutation_policy.rs:23-80`), so this is unchanged.
6. **The request-addressed decision cannot be abused.** The service locks
   `approval_requests r JOIN approval_definitions d … WHERE r.org_id = $1 AND r.source_workspace_id = $2 AND r.approval_definition_id = $3 AND r.id = $4 AND r.actor_type = 'human' AND r.actor_user_id = $5 FOR UPDATE OF r`
   (`approval_config_service.rs:1341-1369`). Therefore:
   - A forged id, a request on another workspace, a mismatched `definitionId`, or another actor's
     request each gets 409 with no write. The updated test asserts 409 for another actor's request.
   - A `cancelled` or `expired` request gets 409.
   - Replaying a `settled` request with an equal fingerprint returns read-only state; an unequal
     fingerprint gets 409.
   - `stage` is validated against the request's exact definition version and the source workspace's
     stages.
   - The settle `UPDATE … AND state = 'pending'` requires exactly one affected row.
   - `deferred` (undo) goes through the same gate.
   - A request in workspace F cannot be settled through path workspace G, because the source workspace
     comes from the path and is bound in the lookup.
7. **Approvals cannot be laundered through discretionary addressees.** The owner can address any org
   member (`create_human_requests` validates only membership). However, the requirement state counts only
   `definition.humans` of the active definition (`approval_config_service.rs:2111-2160`), so a
   non-listed addressee's decision never satisfies or rejects a gate.
8. **Invite-only workspaces are protected.** An addressed user who has lost workspace access fails
   `enforce_workspace_access_for_user` with 403 before any service call.
9. **Requestless decisions are unchanged.** They still require `workspaces:write`, which triggers the
   owner-and-transfer `FOR SHARE` check in `workspace_scoped_db_for_org`, plus `ensure_workspace_owner`.
   The handler's branch and the service's branch both key on the same `body.request_id`, so they cannot
   disagree.
10. **The cross-tab principal-swap exposure is contained.** `security.g5.md` accepts that "approvals …
    are not yet bound to their principal". A request-addressed decision sent under a swapped principal
    gets 409, because the request is bound to the original actor.
11. **Requirement administration grants no owner-only authority.** The new test confirms that a non-owner
    admin is denied 403 on `requests`, `members`, `PATCH workspace` and `stop`.
12. **Definitions are tenant-scoped.** `ApprovalDefinitionService::get_latest` filters
    `WHERE org_id = $1`, on top of the forced RLS `tenant_isolation` policies on `approval_configs`,
    `approval_definitions`, `approval_requests` and `approval_statuses`
    (`001_lean_baseline.sql:8726-8739`). `approval_configs.org_id` is set from the session by trigger.
13. **Audit coverage holds.**
    - The `UserActionAuditLayer` emits one `g5_audit` event for every mutating request by a user,
      whether the route is audited or exempt.
    - Durable records carry the actor: `approval_configs.created_by` and
      `approval_statuses.actor_user_id`.
    - The static route-auth manifest classifies attach and retire as `audited(OrgAdmin)` and decisions as
      `exempt` with a reason. `cargo test -p g5-api --test route_auth_static` passed 7/7.
14. **Test-only code does not leak.** The e2e identities and stub control endpoints exist only under
    `frontend/e2e` and `infra/docker/oauth-stub`. No backend production code was added; the only backend
    change is in route handlers.
15. **Workspace overrides were checked (note only).** `lock_user_scope_authority` counts
    `workspace_permission_overrides` when a workspace id is given, so a workspace-scoped `org:admin`
    override could admit attach and retire on that one workspace. Only org admins can set overrides
    (`routes/permissions.rs:585-600`), and the token must still carry an `org:admin` claim. No
    escalation; P6.4 retires overrides anyway (`g5.security.owner.retirement`).

## Test runs

- `cd backend && cargo test -p g5-api --test route_auth_static`: **7 passed, 0 failed.**
- `cd backend && cargo test -p g5-api --test api_workspace_approvals`: **not executed meaningfully.**
  All 15 tests panicked in setup (`tests/helpers/mod.rs:293`) because no Postgres admin database was
  reachable on 127.0.0.1:5432 or :5433; the dev/test Postgres stack was not running. Per the audit
  constraints I did not start or modify the docker stack. The findings above rest on code tracing, not
  on these tests. Rerun once the test Postgres is up, or set `TEST_DATABASE_URL`, before relying on the
  new `org_admin_administers_requirements_on_base_owned_by_another_identity` and the updated
  settlement/authorization tests.
- I ran scratch instances of the OAuth stub (PR version on port 19877, `bce87811` version on port 19878)
  only to confirm SEC-4 and SEC-5. Both were killed afterwards. The dev stack was not touched.
