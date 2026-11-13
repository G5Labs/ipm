# Users

<a id="g5.user"></a>
[I · g5.user]

A user is one person with a G5 account, who signs in through an external provider and belongs to one or more organizations.

<a id="g5.user.identity"></a>
[I · g5.user.identity]

A user is identified by an email address and has basic profile information and notification settings.

<a id="g5.user.memberships"></a>
[I · g5.user.memberships]

A user belongs to one or more organizations.

<a id="g5.user.authentication"></a>
[I · g5.user.authentication]

Users authenticate to the platform through an external OAuth service such as GitHub.

<a id="g5.user.settings"></a>
[I · g5.user.settings]

Each user has organization-specific settings for model selection, keys and interface preferences.

<a id="g5.user.onboarding"></a>
[D · g5.user.onboarding]

The user record keeps the person's [onboarding tour](ui.g5.md#g5.ui.onboarding-tour) progress: the highest step reached and whether they skipped it. These are facts about the person, not the browser, so drop-off per step and skip counts can be read across users.

- The step is null until the welcome card shows, 0 to 6 during the tour, and 7 when finished.
- The profile read returns both and the profile update accepts them. The stored step only ever rises, so a replay cannot erase it, and the skip flag is set or cleared as sent.


## Authentication and notifications

<a id="g5.user.login-proof"></a>
[D · g5.user.login-proof]

An OAuth callback requires the initiating browser’s unexpired, single-use login attempt, which it consumes before exchanging credentials with the requested provider.

- Provider identity and provider-confirmed email may link one existing user; an ambiguous match to several users fails.
- Entra email and username claims are never provider-confirmed proof for linking, admission or invitation acceptance.
- No provider may attach a sign-in to a service account.

<a id="g5.user.login-providers"></a>
[D · g5.user.login-providers]

Platform login providers are GitHub OAuth and Microsoft Entra ID (OIDC). A provider is enabled exactly when its credentials are configured, and Entra also requires one directory's tenant UUID, not an alias or domain; the sign-in page offers exactly the enabled set. Entra is identity-only: it never grants repository access, and its app roles are not mapped to organization roles.

<a id="g5.user.login-providers.entra"></a>
[D · g5.user.login-providers.entra]

An Entra login is the `(tid, oid)` pair from an ID token G5 validates fully: its signature against the tenant's published, rotation-aware keys, its matching issuer, tenant, audience and the nonce of the consumed attempt, and valid expiry and not-before within a small clock tolerance. Email and username claims are display hints only and cannot change the account's email.

- Only an existing stored Microsoft identity with an organization membership may sign in; an unmatched identity is refused without creating an account or accepting an invitation.
- Legacy Microsoft links are cleared on upgrade. Entra onboarding and new-account linking need a separate design.

<a id="g5.user.admission"></a>
[D · g5.user.admission]

Sign-in admits a verified identity only when it has a current pending [invitation](organizations.g5.md#g5.organization.invitation-authority) or an existing organization membership; no deployment email or domain allowlist exists. A membership admits the account even when inactive, but [active membership](../concepts/security.g5.md#g5.security.active-membership) still governs organization access. A refused identity sees which account was refused, and a lookup failure is reported as an error, not a denial.

<a id="g5.user.session-authority"></a>
[D · g5.user.session-authority]

Session renewal requires a valid refresh token within its idle and absolute lifetimes, and renewal cannot extend the original absolute lifetime. G5 stores a refresh credential only as a one-way digest and returns the plaintext once to the client that logged in, so reading the database cannot replay it. Logout revokes the refresh credential.

- An explicit organization switch resolves current [active membership](../concepts/security.g5.md#g5.security.active-membership) and authority for that organization.
- Sign-in takes the person's newest active membership, breaking ties among memberships created together (as when one sign-in accepts several invitations) by organization id.
- An implicit renewal keeps its stored organization while it stays accessible, otherwise applies the sign-in rule, and gives an account-only session when no membership remains.
- One inactive membership does not revoke access to others.

<a id="g5.user.notifications"></a>
[D · g5.user.notifications]

Notifications are stored privately for their recipient, and delivery follows committed storage. Marking them read — one, a chosen set of a workspace's, or all of a workspace's at once — changes that recipient’s inbox only, so any recipient may do it with read access to the workspace alone, whatever their role or who owns it; it is not a write under the [viewer rule](organizations.g5.md#g5.organization.viewers).

- The bell count and panel show one unread set, and Clear all marks exactly the notifications it showed read, each in the workspace that stores it, so a cleared inbox stays cleared after a reload and an unseen arrival stays unread.
- Mention targets come from the workspace’s permitted audience; a mention failure does not roll back an otherwise valid comment.

<a id="g5.user.notifications.email-preferences"></a>
[D · g5.user.notifications.email-preferences]

A recipient owns account-wide delivery preferences, kept on their user record rather than the browser or one organization, that decide whether a kind of mail may be sent to them. The first, shown as "Approval requests" in Settings and default on, governs both the [request email](reviews.g5.md#g5.governance.approval.request-email) to a reviewer and the [decision email](reviews.g5.md#g5.governance.approval.decision-email) to a workspace owner; each honors it before queuing and rechecks it at delivery.

This settles the approval case of the email module's [preference question](../software_architecture/email.g5.md#g5.email.preference-question); a general preference framework (channels, categories, digests) is still open.

## Programmatic access

<a id="g5.user.api-key-authority"></a>
[D · g5.user.api-key-authority]

Organization administrators create and revoke API keys through their own person session; a delegated key cannot mint, extend or revoke keys. A key is attributed to a user and organization, returned once and stored only as a hash.

- Authentication rejects expired or revoked keys, and each admitted action stays within the key’s scopes, its organization and the user’s current [active membership](../concepts/security.g5.md#g5.security.active-membership) and permissions.
- Deactivation denies use without deleting the key; explicit reactivation permits otherwise-valid keys again but never revives separately revoked or expired keys.
- Removing the person from the organization revokes their keys there in the removal's own commit, so no route admits them afterwards (membership-free reads included) and a re-invitation does not revive them.

<a id="g5.user.cli-authorization"></a>
[D · g5.user.cli-authorization]

G5 CLI login uses a short-lived request approved through the person's own authenticated session, for an organization and, if named, a workspace they may access; approval and device exchange reject a request expired at admission. Only the matching device credential may exchange an approved request, at most once, and a denied or consumed request cannot authorize another exchange. [Credential classes](../concepts/security.g5.md#g5.security.credential-classes) stay distinct throughout.

## Session exchange

<a id="g5.security.login"></a>
[D · g5.security.login]

Login exchanges a verified identity for a G5 session over the browser's cookies; this section covers how that exchange and later refreshes are admitted and protected. Who may sign in is governed by [login proof](#g5.user.login-proof), [providers](#g5.user.login-providers) and [admission](#g5.user.admission).

<a id="g5.security.login.refresh-exchange"></a>
[D · g5.security.login.refresh-exchange]

Cookie-based session refresh admits only platform or configured browser origins and rejects cross-site Fetch Metadata, including from sibling preview origins. A cookie exchange returns the access token in JSON and the refresh credential only in its cookie; an explicit refresh token in the request body keeps its credential response and is exempt from this cookie-origin gate. Non-browser clients may omit browser headers.

- Refresh issuance, renewal, preview re-authentication and logout share [host protection](../concepts/security.g5.md#g5.security.browser-cookie-origin) and [unambiguous cookie selection](../concepts/security.g5.md#g5.security.browser-cookie-selection); the origin gate alone cannot stop a sibling preview from planting a cookie for a later platform-origin request.
- In production the web client keeps the access token in memory only, never in `localStorage` or other script-readable storage, and a new tab or reload gets one through this exchange. Only development and E2E test logins may persist a token.

## Metadata

### g5.user

- agent
- Defines the `g5.user` ID level: a person with a G5 account. Composed from [Ben's ontology](../../../docs/ben_new_ontology.md#L36) lines on identity, memberships and authentication.

### g5.user.identity

- human-provided
- Source: [Ben, line 36](../../../docs/ben_new_ontology.md#L36).
- Gap [CI-G4](../../../docs/code_gaps.md): inbox delivery exists, but delivery-preference fields were retired; see [chat/identity evidence](../../uplift/ben-ontology/investigations/chat-identity.md).
- [Notification](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/chat-identity/Notification.tla) checked storage before delivery, recipient-only access and private read state. It did not establish delivery preferences or eventual delivery.

### g5.user.memberships

- human-provided
- Source: [Ben, line 37](../../../docs/ben_new_ontology.md#L37).

### g5.user.authentication

- human-provided
- Source: [Ben, line 38](../../../docs/ben_new_ontology.md#L38).

### g5.user.settings

- human-provided
- Source: [Ben, line 39](../../../docs/ben_new_ontology.md#L39).
- [UiPreferenceScope](../../uplift/ben-ontology/investigations/chat-identity.md) showed one scope observing another's preferences with browser-wide storage, while user/organization-keyed storage preserved isolation. [CI-G2](../../../docs/code_gaps.md) remains open; this was a bounded storage model, not a browser test or a claim about API keys in local storage.

### g5.user.onboarding

- human-provided
- [Chat intent](../../chat_intent/onboarding-tour/request.md) and [design](../../pr_ontology_diff_design/onboarding-tour/ontology-plan.md), 2026-10-26. User request for one field "along the lines of userHasCompletedTutorial" recording the max step completed or skipped; the separate skip flag exists because the user chose to resume an unfinished tour until it is completed or skipped, which needs a skip told apart from leaving.
- Migration `121_user_onboarding.sql`; `g5-api-data` `User`, `user_service.rs` (`CurrentUserDTO`, `UpdateUserRequest`, the `GREATEST` merge); `routes/users.rs`. Tests: `api_users.rs` (fields on GET, monotonic step, skip flag, range validation).

### g5.user.login-proof

- agent
- Source and evidence: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md). `OAuthAttempt` checked initiating browser, requested provider, expiry, single use and unique account-linking evidence.
- TLS, token security and remote-provider proof were assumptions of that archived model. [Issue #1101](https://github.com/G5Labs/G5-Lean/issues/1101) requires validated Entra identity without email proof and service-account refusal; [design](../../pr_ontology_diff_design/issue-1101/ontology-plan.md).

### g5.user.login-providers

- human-verified
- Source: [James Moore's Entra requirement and the restore request](../../chat_intent/restore-entra-login/request.md); [design](../../pr_ontology_diff_design/restore-entra-login/ontology-plan.md). Supersedes the GitHub-only restriction of [PR #21](https://github.com/G5Labs/G5-Lean/pull/21), which retired Google and GitLab login; those stay retired. The app-role mapping and provider allowlist were rejected, not deferred.
- Security correction: [issue #1101](https://github.com/G5Labs/G5-Lean/issues/1101), [design](../../pr_ontology_diff_design/issue-1101/ontology-plan.md), and [Microsoft's claim semantics](https://learn.microsoft.com/en-us/entra/identity-platform/id-token-claims-reference).
- Dated assumption, 2026-11-05: no Entra sign-in has run in any deployment and there are no self-hosted installs, as recorded in issue #1101. Therefore clearing legacy links needs no session or credential revocation. With no established `(tid, oid)` links, Entra admits nobody until separately authorized onboarding exists.

### g5.user.login-providers.entra

- human-verified
- Expands: [login providers](#g5.user.login-providers).
- Reason the validated `(tid, oid)` identity and refusal of unmatched identities: [issue #1101](https://github.com/G5Labs/G5-Lean/issues/1101) closes account takeover through tenant-controlled email claims while retaining [login proof](#g5.user.login-proof) and [admission](#g5.user.admission); [design](../../pr_ontology_diff_design/issue-1101/ontology-plan.md) and [Microsoft's claim semantics](https://learn.microsoft.com/en-us/entra/identity-platform/id-token-claims-reference).

### g5.user.admission

- human-verified
- Source: owner, 2026-10-24: "The allowlist exists only for bootstrap. If we have a better bootstrap mechanism then the allowlist could be retired?" ([chat intent](../../chat_intent/issue-776/request.md#self-serve-creation-and-the-allowlist)). This resolves the former admission-continuity question by retiring the gate. That question's provisional invitation and membership exceptions are now the whole rule.
- Reason an identity with neither an invitation nor a membership is refused: organizations exist only through [operators](organizations.g5.md#g5.organization.creation), so such an identity has nothing to use.
- The provisional rule's [AdmissionContinuity](../../uplift/ben-ontology/quantitative-refinement/identity.md) check (identity-bound admission, distinct lookup errors, member re-entry after an invitation is consumed) still applies. Its allowlist branch no longer exists.
- Retires `G5_AUTH_ALLOWED_EMAILS`, which was introduced for G5Labs/G5#1517; the API logs a warning when it is still set. `AuthService::handle_callback` returns `AdmissionDenied`. `api_auth` covers refusal without an invitation, invitation and membership admission, linking an operator-created account and lookup failure. [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#sign-in-admission-without-an-allowlist).

### g5.user.session-authority

- agent
- Source and evidence: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md). `RefreshAuthority` checked valid refresh credentials, idle/absolute expiry, target-organization membership and the original lifetime cap.
- The bounded check permits a renewal admitted before logout to finish; it does not promise instantaneous revocation of issued access tokens.
- [Organization-scoped deactivation](../../chat_intent/member-removal/request.md#human-instructions) adds active-organization selection without global logout; [design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#authorization-through-every-path). Refresh, explicit switch and CLI approval/exchange now recheck activity. PostgreSQL integration tests exercise fallback to another organization and account-only access; the prior bounded model does not cover these changes.
- Digest storage: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660), [design](../../pr_ontology_diff_design/issue-660/ontology-plan.md#5-refresh-credentials-are-stored-as-digests). Migration 105 renames `refresh_tokens.token` to `token_hash` and rewrites existing rows to their SHA-256 digest, keeping valid sessions; no rotation or reuse detection is introduced.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 34 (CC-Q1).

### g5.user.notifications

- agent
- Source and evidence: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md). `Notification` checked storage before delivery, recipient-only access and private read state.
- Audience selection and mention rollback were inspected separately; delivery channels and eventual delivery were not established by the bounded check.
- Clear all: `frontend/src/hooks/useNotifications.ts` (`active`, `clearAll`) feeds both the bell and `NotificationPanel`; `useNotifications.test.tsx` covers the shared unread set and persistence per owning workspace.
- Read access alone, and the chosen set: `routes/notifications.rs` admits mark-read and read-all under the read scope, since the write scope demands current workspace ownership, and read-all takes optional `ids`; `api_comments.rs::test_mention_recipient_marks_notifications_read_with_read_access_only` covers a non-owner recipient marking a set, one and all.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 35 (US-Q1).

### g5.user.notifications.email-preferences

- human-provided
- Expands: [notifications](#g5.user.notifications).
- Account-wide delivery preferences, from [issue #991](https://github.com/G5Labs/G5-Lean/issues/991) ([design](../../pr_ontology_diff_design/issue-991/ontology-plan.md)): the first is `users.email_on_approval_request` (default on), and the owner chose account-wide over the per-organization lean of CI-G4 ([chat intent](../../chat_intent/issue-991/request.md)). On 2026-11-02 the owner chose to have the same preference govern the decision email rather than add a second toggle ([chat intent](../../chat_intent/issue-991/request.md#owner-follow-up-2026-11-02-decision-email)). Closes the approval slice of gap [CI-G4](../../../docs/code_gaps.md).

### g5.user.api-key-authority

- agent
- [ApiKeyAdmission](../../uplift/ben-ontology/investigations/workspace.md) checked authentication rejection for revoked/expired keys and operation admission within organization, token scope, current permission and membership. It permits a request authenticated before revocation to continue; hashing, entropy, every route and the full RLS schema were not proved.
- Governing account boundary: [credential classes](../concepts/security.g5.md#g5.security.credential-classes); an all-scopes organization key is not a person session.
- [Deactivation design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#authorization-through-every-path) preserves stored keys while denying their use through current membership activity. Implemented in API-key validation and admission; integration tests retain and reject an existing key, then accept it after explicit reactivation. The prior model does not establish this behavior.
- Person-session management: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660); see [credential classes](../concepts/security.g5.md#g5.security.credential-classes) for the refused delegated creation and the disposition of keys issued by keys before this change.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 36 (SC-Q2).

### g5.user.cli-authorization

- agent
- Source: [coverage C03/F2](../../uplift/ben-ontology/reviews/coverage-identity.md#f2), merged credential-boundary repair `4c9aa418`; G5 CLI login differs from native Codex authorization.
- [CliAuthorization](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked person-session approval, target access, expiry at admission and single consumption under competing polls; failure after consumption may require a new request. It did not make approval, consumption and token issue atomic or prove cryptographic secrecy; expiry and access changes between these steps remain limits.

### g5.security.login

- agent
- Defines the `g5.security.login` ID level: the browser cookie exchange for a G5 session, a child of the `g5.security` concept in [security.g5.md](../concepts/security.g5.md). Children: [refresh exchange](#g5.security.login.refresh-exchange).

### g5.security.login.refresh-exchange

- agent
- Expands: [login](#g5.security.login).
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md).
- `routes/auth.rs` rejects malformed/duplicate origin and Fetch Metadata headers. This supplements [session lifetime](#g5.user.session-authority); it does not promise immediate revocation of already issued access tokens.
- [Issue #917 design and verification](../../pr_ontology_diff_design/issue-917/ontology-plan.md) implement host-bound issuance and selection through the shared cookie contracts, including refusal without renewal/revocation and legacy-session sign-in recovery.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 37 (SC-Q3).
