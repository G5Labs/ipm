# Browser authentication continuity

Source: [issue #698](https://github.com/G5Labs/G5-Lean/issues/698). The user requested
design followed by implementation and a verified PR. Authorized workspace branch:
`FVB-i698-F9-browser-authentication-continuity` (initial checkout, `015310db`).

## Governing behavior

[Authentication continuity](../../ontology/features/ui.g5.md#g5.ui.authentication-continuity)
owns unresolved authentication and stale-answer rejection.
[Token adoption](../../ontology/features/ui.g5.md#g5.ui.authentication-continuity.adoption)
owns same-person page continuity; the [draft boundary](../../ontology/features/ui.g5.md#g5.ui.authentication-continuity.draft-owner)
derives retention independently of profile availability.
[Session authority](../../ontology/features/users.g5.md#g5.user.session-authority)
owns actual refusal and revocation. [File drafts](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts)
and [comment drafts](../../ontology/features/documents.g5.md#g5.documents.comments.drafts)
retain their respective person/workspace and organization boundaries.
Cookie origin checks, including the separate #646 problem, are unchanged.

## Decision and interactions

Reuse AuthProvider's loading state, token-change subscription and transition fencing.
An absent profile is not evidence of sign-out. Initial authentication and replacement
profiles stay unresolved on network, 408, 429 and temporary server failures, retrying
with exponential delay capped at 30 seconds while mounted. A profile 401 first renews
credentials through the existing refresh exchange; an authoritative refusal ends only
the current session. If that newly issued credential also receives 401, end the session
instead of exchanging indefinitely. Remember the renewed credential across temporary
profile failures; if it expires while waiting, permit another renewal so a long outage
does not strand a valid cookie session. A refusal answered for a token replaced meanwhile
cannot sign out its replacement, including a same-context renewal. A successful refresh
with an unavailable profile retries that profile without repeatedly exchanging a
still-usable token.

Token adoption publishes person, organization and scopes together before rendering.
All adoption producers (initial, silent API retry, periodic, explicit switch and access
recovery) apply the same identity rule: retain the same person's profile with the new
active organization while reloading it; remove a different person's profile immediately.
RequireAuth keeps the same person's children mounted during recovery. File draft
retention uses token identity independently of profile availability; organization-scoped
composers and request guards retain their existing isolation. No draft schema or new
durable store is needed. Scoped-config requests are still aborted before token changes
and caches evicted under the replacement credential.

Each recovery belongs to the current transition. Another context, logout or unmount
cancels its scheduled retry and fences outstanding results, including an A–B–A return.
Same-context token renewal can complete recovery with the renewed credential. Explicit
logout invalidates pending work immediately, even if the network logout fails. Existing
provider lifecycle ownership is sufficient; a second session manager or persistence
protocol would duplicate it without adding a required guarantee.

## Verification

Exercise AuthProvider, RequireAuth, the real API client's 401 path, file drafts and
scoped composer/cache consumers. Cover both implicit adoption paths; pending, unavailable
and recovered profiles; initial refresh failures and partial recovery; rejected credentials;
different people; sign-out/unmount during requests or backoff; and superseded completions.
Run real browser regressions with two tabs sharing the simulated server session, alongside
the existing auth/API/draft regressions, frontend lint/build and applicable CI. The browser
server double models cookie-selected tokens and failure schedules; it does not establish
server refresh-cookie cryptography or origin enforcement, which this change does not alter.

Self-review: retaining only a loading spinner would still unmount unsent text; retaining
the old organization would expose stale context. Retaining person identity while publishing
the new organization and scopes satisfies both continuity and isolation. Independent
request binding remains necessary: preserving a page never authorizes replaying an old
organization's write with a new organization's token.

## Implementation evidence

`useAuthContinuity.test.tsx` reproduced eleven failures before the fix. The final
auth regressions include initial refresh/profile recovery and refusal, partial recovery,
same-person implicit adoption, cancellation and stale profile/switch completions.
`e2e/browser/auth-continuity.spec.ts` passes in Chromium with two tabs sharing an
HTTP-only refresh cookie and the real ChatPanel, AuthProvider, RequireAuth, file-draft
store and organization-scoped comment owner. The five browser tests cover both implicit
paths, unavailable replacement profiles, initial partial recovery and refusal after
fresh credential renewal, including an intervening outage.

The broader consumer run exposed that draft eviction must remain with RequireAuth
(plus actual sign-out), not unconditional token publication: standalone editor consumers
also exercise captured-principal refusal without a guarded page. It also exposed the
shared test JWT's obsolete `orgId` claim; the fixture now uses the server's `org` claim.
The affected app, clone, file and auth suites pass together. Frontend lint has no errors.
Production build passes with a 6 GiB Node heap after lifting this VM command's inherited
data-segment limit; no repository build settings changed. Full-suite and PR CI results
are recorded on the PR; browser doubles do not claim server authentication coverage.

Review refinement: CodeRabbit's repeated-401 case is a current authentication refusal,
not one of the specified temporary failures. Auth middleware returns 401 when it cannot
admit the bearer; retain one renewal for an expired access token, then honor refusal of
the newly issued credential. Temporary profile failures preserve that renewal state;
expiration during backoff permits renewal again. Regression cases cover these sequences
and a newer token arriving before the old profile's 401/403. The ontology's independently changeable
recovery, adoption and draft-boundary commitments now have separate stable blocks.
