# Restore Microsoft Entra ID platform login — request and interpretation

## Human instructions

- James Moore, 2026-10-23: "It is a requirement to authorize with Entra. We need to
  support Accenture's OAuth."
- Davide, 2026-10-23 (task brief): restore the Entra platform login that
  [PR #21](https://github.com/G5Labs/G5-Lean/pull/21) (`9d076daaa`) removed, switch the
  Azure self-hosted canary back to Entra login, keep GitHub login supported (the change is
  additive), use the old G5 repository only as a secondary reference, keep the design
  minimal, no TLA+/TLC, and do not merge the PR.

## Agent interpretation

- Enterprise customers (Accenture; PNC, which is Azure-hosted) sign in through their
  Entra tenant, so the self-hosted product must offer Entra login and the Azure canary,
  the only pre-release receipt for Azure customers, must prove a real Entra sign-in.
- Entra is an identity-only provider: it never grants repository access, so its protocol
  lives beside the login service, not in the SCM provider crate.
- Entra app roles are not mapped to organization roles (the pre-removal
  `G5_MICROSOFT_DEFAULT_ORG_ROLE_MAP`); admission follows the shared allowlist,
  invitation and default-admin rules. No provider allowlist
  (`G5_OAUTH_ENABLED_PROVIDERS`) returns: a provider is enabled exactly when its
  credentials are configured.
- Hosted GCP environments gain no Entra secrets: the pre-removal commit only wired
  Google there, and no hosted tenant has asked for Entra.
- Canary: Entra login is required; GitHub login is optional beside it when its secrets
  are present. The redirect URI is stable per DNS label and is ensured through Microsoft
  Graph only when the app registration object id is supplied.
