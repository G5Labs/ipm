# Local and CI stack credentials

`just dev-up`, `just dev-stack-up`, backend restart, and the Docker validation runner load one retained local credential bundle. The default file is `$XDG_CONFIG_HOME/g5/local-stack-secrets.json`, or `~/.config/g5/local-stack-secrets.json` when XDG_CONFIG_HOME is unset. It is outside the checkout, private to the OS user, and shared by worktrees because the dev Postgres volume is also shared. `G5_LOCAL_STACK_SECRETS_FILE` selects another file outside the checkout.

First startup generates matching ES256 JWT keys and a random 32-byte SCM encryption key only when no earlier G5 dev data or retained G5 local test stack exists. The container probe uses G5 service/project names, validation labels or G5 network membership, so unrelated projects with generic Compose filenames do not block setup. Publication flushes the file and its containing directory chain through the filesystem root before exporting credentials. A failed directory or ancestor flush stops the launch; a retry reuses and flushes the same retained file and chain, even when another initializer created those directories. Later starts reuse the file. Keep it with backups of the associated data; losing the encryption key makes retained encrypted credentials unreadable. Ordinary stack cleanup retains this file.

To manage credentials explicitly, supply `G5_JWT_PRIVATE_KEY`, its matching `G5_JWT_PUBLIC_KEY`, and a random 64-hex-character `G5_SCM_ENCRYPTION_KEY` through `.env` or the environment. The loader sets `G5_JWT_SECRET` to the same private key for user tokens; if supplied separately, it must match. A complete explicit configuration takes precedence without creating a managed bundle. Partial, malformed, mismatched, or known public fixture credentials fail; they are never filled from another source.

For direct Compose use, load the same configuration before starting services:

```bash
set -a
if [[ -f .env ]]; then source .env; fi
set +a
source scripts/lib/local-stack-secrets.sh
g5_load_local_stack_secrets
docker compose -p g5-dev -f infra/docker/docker-compose.dev.yml up -d
```

The helper's Python command emits secret-bearing shell input. Use the shell helper; do not run that command into a terminal or log. Compose can still parse build/down configuration without keys, but API/daemon startup rejects empty configured keys before connecting to the database.

## Existing local data

An existing volume without its bundle is a recovery case, not a fresh installation. Restore the original bundle or provide the complete original credentials. If the data was encrypted with the old public fixture key, the new binary intentionally refuses it even when `G5_E2E_AUTH=true`. Preserve the data and use a controlled migration of stored credentials before switching keys. No automatic ciphertext migration is supplied.

For a deliberately disposable dev environment, `just dev-down` stops its workers and deletes its Compose volumes, including the dev database and repositories; use it only when that data can be discarded. For a disposable validation stack, `just ci-docker-down` removes the current worktree's validation services and volumes (use the same `G5_CI_COMPOSE_PROJECT` override as startup). Then remove obsolete fixture settings from `.env` and start again. These are explicit operator recovery choices; the bootstrap never performs teardown or key rotation itself, and ordinary cleanup keeps the retained credential file.

If migration 153 blocks `just dev-up` on the old membership-less `g5labs-unassigned` tenant, recreate the disposable dev volume with `just dev-down` followed by `just dev-up` (this deletes the dev database and repositories).

## CI and recovery tests

Cloud Build generates one fresh bundle in its private `stack-secrets` volume, outside `/workspace`. Its API replicas and restarts share that bundle. The Rust E2E runner receives the signing key through the runtime-only `G5_E2E_JWT_PRIVATE_KEY` variable for the host-loss stale-token test; it is not compiled into the test binary. Local Docker validation and `just e2e-live` use the same loader alias. `just e2e-run`, `just test-live-e2e`, and the Rust `just dev-e2e` wrapper load the local signer after their `.env`/`.env.test` inputs unless `G5_E2E_JWT_PRIVATE_KEY` is already supplied for an external stack. A separately launched host-loss test also needs the signing key of its actual stack in that variable.

Generated keys stay out of Docker build contexts and artifact directories. Committed PEM files remain public fixtures for deterministic unit and startup-rejection tests. Deployment startup rejects that key's cryptographic identity, including an equivalent PEM representation, independently of the E2E-login flag. Starting without a configured verifier preserves the binary's optional-key policy, but the router and middleware share one validator that rejects every token, including Git and WebSocket credentials, instead of trusting an implicit fixture. Preview forwarding still strips bearer credentials when no verification key is configured. Local validation resolves credentials before recording configuration identity and refuses a change between that snapshot and service startup.
