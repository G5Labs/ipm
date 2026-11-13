# Restore Microsoft Entra ID platform login

Request: [human intent and interpretation](../../chat_intent/restore-entra-login/request.md).
Designed against main `d35218164` on `feat/restore-entra-login`; reverts the login
parts of [PR #21](https://github.com/G5Labs/G5-Lean/pull/21) where they still fit and
re-homes the protocol where Lean has since moved it.

## Governing contracts

- [Authentication](../../ontology/features/users.g5.md#g5.user.authentication) and
  [login proof](../../ontology/features/users.g5.md#g5.user.login-proof) own the OAuth
  attempt, PKCE and account-linking rules; Entra reuses them unchanged. Added here:
  [login providers](../../ontology/features/users.g5.md#g5.user.login-providers).
- The [admission continuity question](../../ontology/features/users.g5.md#g5.user.admission-continuity-question)
  governs first admission for Entra identities exactly as for GitHub.
- Added here: [Azure canary login](../../ontology/features/operations.g5.md#g5.platform.azure-canary-login),
  the self-hosted lane's required Entra sign-in and optional GitHub check.

## Design

- **Owner of the Entra protocol:** `g5-api-core/src/auth/entra.rs` (code exchange with
  the PKCE verifier, OIDC userinfo, ID-token claim fallback for the email). The SCM
  crate keeps GitHub/GitLab identity because those are repository providers; Entra is
  identity-only, so putting it there would widen the SCM boundary for no reuse. The
  auth service dispatches on the provider name and feeds the existing attempt, allowlist,
  invitation and account-linking flow with the normalized identity.
- **Enablement:** `G5_MICROSOFT_CLIENT_ID`, `G5_MICROSOFT_CLIENT_SECRET` and
  `G5_MICROSOFT_TENANT_ID` enable the provider; the tenant is substituted into the
  default authorize/token endpoints, and URL overrides exist for tests and sovereign
  clouds. `enabled_providers()` and `/api/v1/instance/info` advertise `microsoft` beside
  `github`; the sign-in page renders exactly the advertised set.
- **Rejected:** restoring the Entra app-role → organization-role map and the provider
  allowlist. Neither is required to authorize with Entra; both add configuration and a
  denial path outside the shared admission rules.
- **Canary:** Entra login is the required proof (host provisioning and the microVM chat
  turn both run behind it); GitHub login is an optional second sign-in when
  `SELF_HOSTED_GITHUB_*` secrets exist. The redirect URI is stable per
  `G5_CANARY_DNS_LABEL`; `AZURE_ENTRA_APP_OBJECT_ID` lets the run add it through Graph
  if missing, otherwise the tenant owner registers it once. The three smoke scripts
  share `login-smoke-common.js` so the chat/host evidence has one owner.

## Implementation evidence

- `backend/g5-api-foundation` config tests (tenant required, both providers enabled);
  `g5-api-core` `auth::entra` unit tests against a mock tenant; `g5-api` integration
  tests `test_microsoft_oauth_callback_success_mints_jwt`,
  `test_microsoft_oauth_callback_denied_by_allowlist`,
  `test_microsoft_oauth_callback_refused_code_is_unauthorized` and
  `test_unconfigured_platform_login_providers_are_rejected`.
- `scripts/test-self-hosted-azure-microvm-canary.sh` pins the Entra workflow contract;
  `scripts/test-self-hosted-bundle.sh` pins the bundle settings.
- Gap: no live Entra tenant login has run in Lean yet; the canary needs the secrets
  listed in `infra/self-hosted/README.md` before its first scheduled run.
