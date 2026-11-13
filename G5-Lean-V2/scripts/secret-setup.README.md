# Environment secret setup

These are operator-run helpers, not deployment or credential-registration tasks.
Prepare the target's Secret Manager containers first. Use a separate GitHub OAuth
App/GitHub App registration and the intended provider credentials for each environment.

```bash
# Login OAuth, signing/SCM keys and model credentials; prompts or existing
# documented environment variables supply values. No cluster actions.
./scripts/bootstrap-secrets.sh \
  --project g5lean-staging-1ccc \
  --external-url https://lean.g5labs.ai

# Optional GitHub App integration. Each command retains the target explicitly.
./scripts/github-app-secrets.sh \
  --project g5lean-staging-1ccc \
  --external-url https://lean.g5labs.ai seed --dry-run /path/to/app.pem
```

For production use `--project g5lean-prod-1ccc --external-url https://app.g5labs.ai`.
Both release projects require explicit project and URL arguments. Cluster and region
derive from the project (`g5-lean-staging` / `g5-lean-prod`, `us-central1`); optional
`--cluster` / `--region` on the GitHub App helper must match. Namespace stays `g5`.
Explicit replacement HTTPS hostnames are supported. Paths, query strings, credentials,
ports, the marketing apex and known other-environment origins are rejected.

Login callback: `<origin>/api/v1/auth/callback/github`.
GitHub App callback: `<origin>/api/v1/github-app/oauth/callback`.
Register these with GitHub; the helpers do not create or change provider registrations.
The license secret is derived as `g5-license-admin-token-staging` or
`g5-license-admin-token-prod`; a conflicting override is refused.

`bootstrap-secrets.sh` appends versions when none exist and skips existing versions.
It never reads secret payloads. If only one JWT key has versions, it stops before
all writes to avoid combining an old key with a newly generated counterpart.
`--force` appends fresh versions to **all** listed secrets, including regenerated
JWT and SCM encryption keys. Treat that as an explicit rotation/recovery operation:
it can invalidate sessions and make existing encrypted SCM tokens unreadable.
Recover a missing key separately when retaining existing authority is required.
Skipped provider inputs use the existing placeholder behavior; that is not working
provider setup. There are no database-password changes, restarts or version deletions.

The GitHub App helper's `seed` prompts and confirms seven appended versions;
`sync` annotates the target ExternalSecret and verifies its synced value lengths;
`restart` confirms and restarts API/daemon, then checks the target instance endpoint.
`rollback` appends blank versions to disable the integration after sync/restart.
Only `seed` and `rollback` support `--dry-run`. They still read container metadata.
No values are printed or put in subprocess arguments; use the target-preserving
next commands printed by the helper. Each cluster operation uses a temporary kubeconfig.

Compatibility: the GitHub App helper still defaults to QA. Bootstrap still resolves
the active project when omitted, retaining the legacy dev origin for `g5-dev-1ccc`;
QA now defaults to its own `https://qa.g5labs.ai` origin. Unknown legacy projects
require an explicit HTTPS origin. A release project cannot be inherited from the
active gcloud configuration, and the artifact-only admin project is refused.
QA's documented `--license-admin-secret-id g5-license-admin-token-qa` remains supported.

Offline tests run the actual shell scripts against local executable stubs:

```bash
python3 -m unittest scripts.test_bootstrap_secrets_script scripts.test_github_app_secrets_script
```
