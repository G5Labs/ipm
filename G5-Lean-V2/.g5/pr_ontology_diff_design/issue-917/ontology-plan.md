# Protect platform cookies from same-site previews — issue #917

Source: [G5Labs/G5-Lean#917](https://github.com/G5Labs/G5-Lean/issues/917), read with
`g5d issue show` on 2026-10-28. Source inspection at `9e72c262b` confirms that
refresh and GitHub App attempt cookies lack host prefixes and their readers select
the first matching cookie. A preview can supply another account's valid credential
through a parent-domain cookie; signatures and the refresh origin gate do not bind
that credential to the browser that originally received it.

## Decision and ownership

Use the existing browser-enforced `__Host-` boundary already used by login and SCM
attempt cookies. AuthService owns the effective refresh-cookie name, including
`G5_REFRESH_COOKIE_NAME`; the GitHub App cookie helper owns its attempt-cookie name.
The route adapters issue, read and expire those names consistently. A small shared
credential-cookie reader owns ambiguity rejection for refresh, preview
re-authentication, logout, login/SCM attempts and GitHub App attempts. It replaces
the existing cross-route parser reuse; no new session or attempt lifecycle is needed.

| Deployment | Refresh cookie | GitHub App attempt cookie | Attributes |
| --- | --- | --- | --- |
| Secure | `__Host-g5_refresh` by default; custom names also protected | `__Host-g5_github_app_pkce` | `Secure; HttpOnly; SameSite=Lax; Path=/`, no `Domain` |
| Plain HTTP development | Configured unprefixed name, default `g5_refresh` | `g5_github_app_pkce` | Existing HttpOnly/Lax cookies and paths, without `Secure`; previews remain unavailable |

Resolve a custom secure name once, adding `__Host-` if it is not already present;
`preview_refresh` therefore becomes `__Host-preview_refresh`. AppState composition
refuses an effective secure name equal to a login, SCM or GitHub App attempt cookie,
using those owners' actual names. This cross-owner check belongs in composition:
Identity must not depend on the later Git context ([service layering](../p8.5-crate-consolidation/ontology-plan.md)).
Moving refresh to `Path=/` must not make previously
separate cookies overwrite each other. This rejects both the base and already
prefixed forms of those three names, preserving other custom names. Reject a reserved
secure-prefix name in insecure configuration rather than emit a cookie the browser
cannot accept. In secure mode, retain any browser-valid legacy name/path only for
expiry. This retains
full-stack G5 previews: the relay permits `__Host-preview_refresh` while continuing
to strip the reserved platform `g5_` names and their prefixed variants.

Read all Cookie headers and accept exactly one nonempty occurrence of the selected
name. Duplicate occurrences fail even when values are equal or one is empty;
undecodable selected credential values also fail. Unrelated cookie bytes, including
UTF-8 or undecodable app data and legacy names in secure mode, cannot supply
authority, make a valid protected cookie ambiguous, or hide another credential
occurrence. Every header is examined; only the selected credential needs decoding.
Refusal precedes refresh renewal or provider code exchange. Logout still clears cookies
when the cookie credential is absent or ambiguous, without choosing a token to revoke.
Explicit body-token refresh/logout retain their existing semantics.

The existing refresh origin/Fetch Metadata gate, token lifetime and revocation
rules, signed GitHub states, PKCE, single-use attempts and binding transaction's
authority checks remain in force. Browser host isolation supplies the missing
anti-transplant guarantee; the [HTTP cookie draft's host-prefix rules](https://httpwg.org/http-extensions/draft-ietf-httpbis-rfc6265bis.html#section-4.1.3.2)
require the attributes above. The design assumes browsers enforce those rules.

## Cutover and recovery

Secure readers accept only the protected name, with no legacy fallback. Existing
cookie sessions require sign-in at their next renewal; already issued access tokens
retain their ordinary lifetime. In-flight GitHub App attempts restart. Database
refresh credentials and programmatic body-token clients remain valid.

Credential replacement and clearing responses expire the old host-only name at
its original path (`/api/v1/auth` or `/api/v1/github-app`) as well as setting or
clearing the effective cookie. Logout revokes only an unambiguous accepted cookie
and any explicit body credential, never a legacy cookie read for compatibility.
GitHub intermediate setup/selection responses preserve the attempt cookie while
terminal callbacks clear it. Parent-domain cookies at arbitrary attacker-chosen
paths can remain in the jar; ignoring their legacy names makes them ineffective.

All API replicas must cut over before the protection is effective for the
deployment; a mixed-version rollout still has vulnerable readers. A fallback to
old names would preserve the attack, so session continuity is deliberately traded
for the repaired browser boundary. No refresh-token rotation or database migration
is needed; refresh credentials are already stored as digests in this checkout.

The issue's optional refresh-session check on GitHub callbacks is deferred. The
protected attempt cookie restores the initiating-browser guarantee and keeps the
existing callback protocol. A second check would introduce separate decisions
about session expiry, logout or account switching during an otherwise valid attempt.
Separate-site preview hosting remains the existing deferred work.

## Governing changes and affected consumers

The actual ontology diff adds shared [host protection](../../ontology/concepts/security.g5.md#g5.security.browser-cookie-origin)
and [unambiguous credential-cookie reads](../../ontology/concepts/security.g5.md#g5.security.browser-cookie-selection),
and links them from [refresh exchange](../../ontology/features/users.g5.md#g5.security.login.refresh-exchange)
and [preview isolation](../../ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security).
[Session authority](../../ontology/features/users.g5.md#g5.user.session-authority)
and [credential classes](../../ontology/concepts/security.g5.md#g5.security.credential-classes)
continue to govern token use and initiation of connections.

Update the config help, preview-isolation documentation and e2e identity helpers
for secure versus HTTP names/paths and custom refresh names. Retain one effective
refresh cookie per e2e identity, cleaning old fixtures as well. The Rust E2E OAuth
client extracts the freshly issued credential, ignoring legacy-expiry response
headers even when it reaches a secure browser deployment over an internal HTTP URL.
The shared attempt
reader also serves login and SCM callbacks, so their successful and refused flows
are affected consumers, alongside all GitHub setup/OAuth/selection callbacks.

## Required verification

- Unit and startup-configuration tests: default/custom names in both modes; refusal of
  secure names colliding with login/SCM/GitHub App attempts; secure
  attributes and matching expiry; one/multiple Cookie headers; duplicate valid,
  equal and empty values; undecodable credentials; unrelated UTF-8 and undecodable
  app data cannot block a valid credential or hide duplicates; no legacy fallback.
- Real API/database tests: two users' valid refresh tokens never select the first
  duplicate or mint a JWT; rejected cookies have no renewal/revocation effects;
  legacy-only secure refresh fails; a protected cookie with planted legacy cookies
  still renews the intended user. Exercise logout revocation and both expiry headers,
  explicit body clients, custom names, login and preview re-authentication.
- GitHub provider-double/API tests: valid attempts with duplicate or legacy-only
  PKCE cookies fail before exchange, do not store a user grant or bind an
  installation, and do not consume the attempt. Cover setup and reconnect selection
  as well as OAuth completion; then complete a valid attempt and retain single-use
  and current-admin checks. The double establishes platform effects, not live GitHub.
- HTTPS browser coverage: from a same-site preview, execute `document.cookie`
  writes with a parent Domain and longer path. Show legacy cookies can be planted,
  protected platform cookies cannot, and a later same-origin refresh preserves the
  intended identity, even with a planted non-ASCII app cookie. Cover the legacy-only
  sign-in recovery path and protected GitHub attempt name; assert the preview relay
  strips both protected platform names.
  Run the affected login, identity continuity, sign-out and preview flows, plus the
  ordinary build/lint and targeted backend regressions.

Self-review traced every reader/writer, the custom preview-cookie configuration,
shared login/SCM parser and GitHub intermediate selection flow. The change adds no
durable state or cross-owner transaction.

An isolated loopback HTTPS probe ran on 2026-10-28 with Chromium
`145.0.7632.6`, a disposable profile and synthetic credentials. From a sibling
preview host, JavaScript planted both legacy names on the shared parent domain at
longer paths. The browser sent each attacker's value before the victim's value.
Equivalent `__Host-` Domain-cookie writes were rejected; valid host-only cookies
set on the preview stayed there, and only the victim's protected values reached
the platform host. This establishes the browser mechanism, not G5 endpoint behavior.

## Implementation and verification

Implemented the effective names, matching issuance/expiry, hard cutover and shared
strict reader. The API tests exercise real PostgreSQL effects and provider doubles;
the HTTPS preview flow now performs the JavaScript planting and same-origin refresh
sequence, and the sign-out suite covers legacy-session sign-in recovery. E2E identity
helpers preserve custom names and both transport modes. The native approvals browser
fixture explicitly selects its plain-HTTP development mode.

Local verification on 2026-10-28–29:

- 14 cookie/configuration unit tests, 126 API/database regressions across auth,
  GitHub grants/bindings and credential authority, two preview relay/re-authentication
  tests and all 12 static authorization checks passed. The static audit now recognizes
  the shared reader's borrowed token without changing its required authorization order.
- `cargo fmt --all -- --check`, Clippy for the affected API crates with all targets
  and warnings denied, frontend production build and lint passed (lint warnings,
  no errors).
- The optional native approvals browser journey passed with the existing onboarding
  overlay temporarily dismissed. Its unchanged Home inventory step otherwise has
  its link click swallowed by that overlay; the temporary dismissal is not part of
  this PR. The fixture's HTTP cookie name/path now match the API.
- Playwright discovers the changed flows. The HTTPS preview journey runs in PR
  CI; the isolated probe above establishes browser enforcement only. The exact
  legacy-session recovery browser test also passed locally through the real
  SPA/API/PostgreSQL with an HTTPS edge and the existing OAuth provider stub.

[PR #938](https://github.com/G5Labs/G5-Lean/pull/938) also ran all 1,922 API
integration tests successfully. Its separate HTTPS lane passed the browser planting,
identity and sign-out flows. CI identified two source-guard integration
gaps, the Rust E2E client's legacy-cookie extraction and a recovery-test login
landing-page assumption. The source guards are now satisfied (all 214 route unit
tests pass), and three GitHub API regressions pass after the header adjustment.
The Rust client's regression test passes for secure, HTTP and custom names, and
Clippy passes for the affected route/E2E targets. The next CI run passed Rust CI,
the Rust E2E profiles and 308 browser tests; its sole browser failure was the
recovery test's assumption that `/projects` exposes the account control. The
corrected test isolates a valid legacy cookie in a fresh context without an
access token, then checks rejection, the sign-in redirect, recovery and preserved
identity. Its local HTTPS run passed without product changes or interception of
platform requests; the temporary local harness was removed. The PR's latest
checks remain the authority for readiness.

The [owner's follow-up review](https://github.com/G5Labs/G5-Lean/pull/938#pullrequestreview-5692595766)
identified lockout from unrelated non-ASCII cookies. On 2026-10-29, Chromium
`145.0.7632.6` accepted a preview's parent-domain `x=café` cookie and sent its raw
UTF-8 bytes beside the protected platform cookie. The new parser test failed on
the prior implementation, and the real API regression returned 401 for a valid
session. Selection now tolerates unrelated cookie bytes while preserving refusal
of duplicate, empty or undecodable selected credentials across every header.
All 215 route unit tests, 61 auth API tests and five targeted GitHub/preview API
regressions pass, including identity preservation and logout revocation. Formatting,
Clippy with warnings denied, browser-test lint and discovery pass. The HTTPS preview
regression also plants the non-ASCII cookie; that journey and legacy-session
recovery passed in PR CI on `d4359e764` alongside all 1,923 API integration tests.

The [custom-name review](https://github.com/G5Labs/G5-Lean/pull/938#pullrequestreview-5692831474)
identified a cutover regression: a refresh name matching an attempt cookie loses
its former name/path separation when both become host-prefixed root-path cookies.
AppState composition refuses those configurations using the login, SCM and GitHub
App owners' effective names. The collision test failed before the guard. The initial
guard passed functional tests but CI's service-layering check rejected its direct
Identity-to-Git dependency; the check now lives in composition, preserving the
same startup refusal and the owners' dependency rules. A startup regression covers
all six base/protected collision names and HTTP compatibility without database
access or an HTTP listener. The existing API custom-name and unrelated-cookie
refresh/logout regressions remain the permitted-use checks. All three tests and
all seven service-layering checks pass after the composition correction, along
with formatting and Clippy for core/API with all targets and warnings denied.

Historical formal evidence does not establish cookie-prefix enforcement; no
TLA+/TLC work was requested.
