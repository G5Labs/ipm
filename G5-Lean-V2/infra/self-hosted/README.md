# G5 Self-Hosted Bundle

This directory is the source template for the customer self-hosted control-plane
bundle installed under `/opt/g5`.

It is intentionally separate from:

- `infra/docker/docker-compose.dev.yml`
- `infra/docker/docker-compose.test.yml`
- hosted GCP Cloud Build and Helm deployment

The customer operator surface is the external `Justfile`:

```sh
just setup
just up
just logs
just backup
just upgrade
just down
```

`just upgrade` stops `g5-api`, `g5-daemon` and `edge`
(`stop_app_services`) before it runs `db-migrate`, so no prior-version process
is still querying the old schema while a destructive migration such as
`002_remove_workspace_search.sql` drops the workspace-search tables and the
pgvector extension. Keep that ordering; the database image must stay a
pgvector-capable one because the immutable baseline still creates the extension
during a fresh replay.

For the `g5-daemon` rename, install the release's updated `compose.yml` and
`scripts/g5-selfhost.sh` before running `just upgrade`, preserving the existing
`.env` and data directories. The previous bundle selects the retired
`G5_ROLE=indexer`, which the new API image rejects. The updated bundle removes
the old maintenance container with `compose up --remove-orphans` before database
migrations and starts the replacement with `G5_ROLE=daemon`.

Bundles before 2026-10-09 also ran a `redis` service. The same orphan cleanup
removes its container before migrations. After a successful upgrade, the old
`${G5_DATA_DIR}/redis` directory and unused `.env` entries `G5_REDIS_URL` and
`G5_IMAGE_REDIS` can be deleted.

Runtime prerequisites are Docker with the Compose plugin, Python 3, Git, and
`just`. The first `just up` creates local customer-owned starter Git
repositories; Git is required so the default workspace and starter ontology can
point at real committed revisions. The starter TinyDB ontology is seeded with
`tinydb_aggregate` as its semantic root. The bundle keeps API rate limiting on
and defaults `G5_RATE_LIMIT_RPM` to `20000` for interactive self-hosted
workloads; operators can override it in `/opt/g5/.env`.

Public self-hosted installer assets are published through GitHub Releases in
the public `G5Labs/Releases` repository:

- `install.sh`
- `g5-self-hosted-bundle.tar.gz`
- `SHA256SUMS`

The release bundle is intentionally bootstrap-only. Do not add standalone
`g5d` binaries or private runtime images here; those stay behind the
license-token registry flow until a gated binary-download path exists.
Tagged installer assets default to the matching release bundle and runtime
version; operators can still override with `--bundle-url`, `--bundle-dir`, or
`--version`.

Use `scripts/test-self-hosted-bundle.sh` for local contract checks.

External Linux worker hosts require KVM, cgroup v2, systemd 254 or newer, XFS
reflinks and enforcing AppArmor. The licensed worker OCI supplies a read-only
systemd service root; Docker only downloads and stages artifacts. The supervisor
uses a delegated cgroup and restricted capabilities, and each Cloud Hypervisor
guest gets a private ext4 disk. Customer OCI images never supply the host service
root. Inspect worker logs with `journalctl -u g5d-host.service`; stop/drain the
service before replacing its immutable runtime. Its XFS store has a persistent
mount unit so service restart after host reboot restores the same cache location.

Platform login supports Microsoft Entra ID and GitHub; configure either or
both in `.env`. A provider is shown on the sign-in page exactly when its
credentials are present.

- Microsoft Entra ID: `G5_MICROSOFT_CLIENT_ID`,
  `G5_MICROSOFT_CLIENT_SECRET` and `G5_MICROSOFT_TENANT_ID` (directory UUID;
  domains and multi-tenant aliases are refused) configure the provider.
  Register `<G5_EXTERNAL_URL>/api/v1/auth/callback/microsoft` as a **Web**
  redirect URI with delegated `openid`, `email` and `profile` permissions.
  Login requires an existing stored tenant/object identity and membership.
  Email claims cannot link accounts, satisfy invitations or bootstrap the
  first owner. Legacy Microsoft links are cleared by the security fix for
  [#1101](https://github.com/G5Labs/G5-Lean/issues/1101), so Entra admits nobody
  until separately designed onboarding is available. Use GitHub for setup.
  `G5_MICROSOFT_AUTHORIZE_URL`, `G5_MICROSOFT_TOKEN_URL` and
  `G5_MICROSOFT_JWKS_URL` override protocol endpoints for trusted test stubs;
  issuer validation still requires `https://login.microsoftonline.com/<tenant>/v2.0`.
- GitHub: set `G5_GITHUB_CLIENT_ID` and `G5_GITHUB_CLIENT_SECRET`, and register
  `<G5_EXTERNAL_URL>/api/v1/auth/callback/github` as the OAuth callback.

The self-hosted bundle must not enable the OAuth stub. Organization access
comes from G5 invitations and the default-admin bootstrap; Entra app roles are
not mapped to organization roles. GitLab repository OAuth uses the separate
`G5_SCM_GITLAB_*` settings. Installations that previously shared `G5_GITLAB_*`
login credentials or URL overrides must move those values to the corresponding
SCM settings.

Stored GitLab credentials are bound to the HTTPS host and port of `G5_SCM_GITLAB_USERINFO_URL` (default `https://gitlab.com/api/v4/user`). For a self-managed installation, configure that URL for its API origin together with the authorize and token URLs. The configured clone origin replaces gitlab.com; it does not add a second destination for the same credential. Import and Adopt-Remote reject clone URLs outside that origin or containing userinfo. The origin remains bound if OAuth client credentials are removed. API startup rejects HTTP or malformed GitLab API URLs and URLs containing userinfo, a query, or a fragment, before any service can use stored credentials. Other origins unsupported by the Git transport policy disable GitLab Git credentials and emit a startup warning. GitHub credentials remain bound to `https://github.com:443`.

The Azure canary (`.github/workflows/self-hosted-azure-install.yml`) runs
nightly on `main` and on manual dispatch. It installs the checked-out installer
bundle on a nested-virtualization Azure VM, exercises backup/restore, down/up
and upgrade, starts one external Cloud Hypervisor worker host against an
ephemeral ACR task image, and proves a real OpenRouter-backed chat turn through
Microsoft Entra ID login. It is the only pre-release receipt for Azure-hosted
customers, who sign in with Entra; GitHub login is an optional extra check.

The hosted side is the Lean QA deployment (`G5_CANARY_ADMIN_API_URL`, default
`https://qa.g5labs.ai`): it provisions the canary license and resolves
`latest` to the `api`, `edge` and `g5d` images QA's deploy pushes to
`us-central1-docker.pkg.dev/g5-qa-1ccc/g5`. The `self-hosted-azure-canary`
GitHub environment must supply:

- `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`: OIDC login.
  The app registration needs a federated credential with subject
  `repo:G5Labs/G5-Lean:environment:self-hosted-azure-canary`.
- `G5_LICENSE_ADMIN_TOKEN`: the QA admin token (Secret Manager
  `g5-license-admin-token-qa` in `g5-qa-1ccc`).
- `OPENROUTER_API_KEY`: the real provider credential for the microVM turn.
- `AZURE_ENTRA_CLIENT_ID`, `AZURE_ENTRA_CLIENT_SECRET`: the Entra app
  registration (client secret, not a certificate) the canary install signs in
  through. Its **Web** redirect URI is
  `https://<G5_CANARY_DNS_LABEL>.<azure-region>.cloudapp.azure.com/api/v1/auth/callback/microsoft`.
- `AZURE_ENTRA_TENANT_ID` (optional): the login tenant when it differs from
  `AZURE_TENANT_ID`, which is the fallback.
- `AZURE_ENTRA_APP_OBJECT_ID` (optional): the app registration **object id**.
  When set, the `Ensure Entra redirect URI` step adds the redirect URI through
  Microsoft Graph if it is missing, which needs the OIDC identity to own the
  app registration or hold `Application.ReadWrite.OwnedBy`. When unset, the
  tenant owner registers the URI once by hand.
- `AZURE_ENTRA_TEST_USER_EMAIL`, `AZURE_ENTRA_TEST_USER_PASSWORD` and (when
  MFA is enforced) `AZURE_ENTRA_TEST_USER_TOTP_SECRET`: the Entra login smoke
  account, a tenant member whose UPN equals its email. Its email must be
  `G5_CANARY_DEFAULT_ADMIN_EMAIL`, the installer's owner and the only account
  admitted without an invitation. The
  account must be exempt from Authenticator registration campaigns and
  Conditional Access prompts the browser cannot answer.
- `SELF_HOSTED_GITHUB_CLIENT_ID`, `SELF_HOSTED_GITHUB_CLIENT_SECRET` (optional):
  a GitHub OAuth app whose callback is
  `https://<G5_CANARY_DNS_LABEL>.<azure-region>.cloudapp.azure.com/api/v1/auth/callback/github`.
  When present, the install shows GitHub beside Microsoft.
- `SELF_HOSTED_GITHUB_TEST_USER_EMAIL`, `SELF_HOSTED_GITHUB_TEST_USER_PASSWORD`
  and (when required) `SELF_HOSTED_GITHUB_TEST_USER_TOTP_SECRET` (optional):
  when set with the GitHub OAuth app, the `Verify optional GitHub login beside
  Entra` step signs in through GitHub too. Sign-in admits only members and
  invitees, so its email must also be `G5_CANARY_DEFAULT_ADMIN_EMAIL`.
- `SLACK_WEBHOOK_URL` (optional, **repository** scope, not the environment):
  the notify job runs outside the environment and cannot read its secrets.

Environment-scoped variables, read by the `azure-canary` job: `G5_CANARY_DNS_LABEL` (stable because the Entra and
GitHub callbacks are registered once), `G5_CANARY_ADMIN_API_URL`, `G5_CANARY_AR_LOCATION`,
`G5_CANARY_AR_PROJECT`, `G5_CANARY_AR_REPOSITORY`,
`G5_CANARY_REGISTRY_PULLER_SERVICE_ACCOUNT`,
`G5_CANARY_DEFAULT_ADMIN_EMAIL`, `G5_CANARY_DEFAULT_ADMIN_NAME`. Canary runs
share one concurrency group because the callback hostname is stable.

## Previews need per-preview HTTPS hosts

Previews are served only from `https://<task-id>.<G5_PREVIEW_HOST_SUFFIX>/`,
never from the platform origin. To enable them on a self-hosted installation:

1. Choose a suffix on the same registrable domain as `G5_EXTERNAL_URL`, for
   example `preview.g5.example.com` for `https://g5.example.com`, and set
   `G5_PREVIEW_HOST_SUFFIX` in `.env`. The API refuses to start if the pair is
   not HTTPS and same-site.
2. Point wildcard DNS `*.<suffix>` at the edge.
3. Use the bundled TLS overlay with a certificate that covers both the platform
   host and `*.<suffix>`; the overlay's Caddyfile routes `*.<suffix>` to the API
   for every path. A custom reverse proxy must do the same.

Without the suffix the platform runs normally and opening a preview answers
`503 PREVIEW_ORIGIN_UNCONFIGURED` with that reason in the UI.

## Workspace VM images: workspace-owned updates

The guest image a workspace VM boots is resolved **when the workspace starts**,
from the workspace's code repository and the image configured for its host. The supported customer image-update path
rides the ordinary workspace SDLC:

1. Build your workspace image and push it to a registry the workspace hosts can pull
   from.
2. Add (or edit) the top-level `base_image:` entry in the code repo's
   `.g5/workspace.yaml`, naming the image as a tag reference
   (`<registry>/<repository>:<tag>`) or an immutable digest reference
   (`<registry>/<repository>@sha256:<64 hex>`):

   ```yaml
   base_image: ghcr.io/acme/task-runtime:2026-08
   ```

3. Merge the change. Every subsequent start of that workspace uses the named
   image, including its agents and previews. No host restart, rotation, or root access is needed. This replaces the manual per-host image update
   procedure for task-image changes.
4. Removing the line falls back to the image configured at host
   provisioning time (`G5D_BASE_IMAGE`, which remains digest-only).

A present-but-invalid entry (malformed reference, or an image that cannot be
pulled) fails the task **before** guest launch with an error naming the
workspace-selected reference; it never silently boots the provisioning-time
image. The recovery is fixing or removing the `base_image:` line. Custom
images must satisfy the same guest ABI floor as the provisioned default; the
licensed G5 runtime image itself is unaffected by this entry.

## Client IP and reverse proxies

The bundled edge ignores caller-supplied forwarding headers and supplies the TCP client address. Compose assigns it `G5_EDGE_IP` (default `172.30.254.2`) on the dedicated `G5_EDGE_SUBNET` (`172.30.254.0/29`) shared only with API at `G5_API_EDGE_IP` (default `172.30.254.3`). Both addresses are fixed so container startup order cannot allocate the edge address to API. API trusts that single `/32` and one trailing `X-Forwarded-For` address. The existing database network is unchanged. If the subnet overlaps your networks, set all three variables to a free subnet and two different unused IPv4 addresses inside it before install/upgrade; the installer preserves them. Do not attach untrusted containers to the edge network or reuse its address.

A native API process trusts no proxies by default. Omit `G5_TRUSTED_PROXY_CIDRS` to select this default; an explicitly empty value is invalid. For a custom proxy deployment, set `G5_TRUSTED_PROXY_CIDRS` to the exact proxy addresses/ranges and `G5_TRUSTED_PROXY_XFF_HOPS` to the number of trailing addresses that proxy always overwrites or appends (1–16). Both the peer address and suffix contract must hold on every path; a larger count than a proxy actually supplies lets a caller fill the missing position. Do not trust all private networks, client-supplied prefixes, or `X-Real-IP`. Missing/invalid suffixes use the TCP peer; missing peer identity uses a shared unknown bucket. Configuration changes require an API restart. If another load balancer fronts Caddy, configure Caddy's trust and forwarding policy explicitly; the bundled defaults identify that balancer as the client.

The shipped GCE Helm ingress uses its GFE source ranges and a two-address suffix (`client,frontend`). Registered auth and WebSocket routes are under the existing `/api/*` ingress path, which already goes directly to API, as do Git and preview wildcard traffic. A custom ingress must override both proxy settings to match its actual source addresses and header behavior. This identity is shared by throttling, audit, telemetry and API-key usage; it is not authentication.
