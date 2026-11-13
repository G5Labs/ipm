---
name: native-stack-browser-e2e
description: "Run Playwright flows in a KVM-less VM against a native g5-api + Vite; key setup traps (secrets refusal, migration source, port 5173, VITE flags)."
metadata:
  node_type: memory
  type: reference
  originSessionId: 01445522-0581-432a-911e-0f37acc1fc40
  modified: 2026-10-25T07:12:27.672Z
---

Without KVM or Docker images, `alignment-shell.spec.ts` Home flows still run against a
native stack (verified 2026-10-20, issue #664); workspace clones work without a VM host.

- Postgres: the dev compose container on 5432; `CREATE DATABASE g5_native` so integration
  tests' DB stays separate.
- Secrets: `scripts/stack_secrets.py local` refuses once the `g5-lean_pgdata_dev` volume
  exists. Generate a throwaway bundle with `scripts/lib/jwt-keys.sh`
  (`generate_es256_private_key` / `derive_es256_public_key`) into a 0600 /tmp env file:
  `G5_JWT_PRIVATE_KEY`, `G5_JWT_PUBLIC_KEY`, `G5_JWT_SECRET`=`G5_E2E_JWT_PRIVATE_KEY`=private,
  `G5_SCM_ENCRYPTION_KEY`=`openssl rand -hex 32`.
- Migrate: `G5_MIGRATION_SOURCE=backend/g5-api/src/db/migrations target/debug/g5-db-migrate`
  (without it: "No such file or directory"; the API then dies on a missing `authz` schema).
- API: `target/debug/g5-api` with `G5_ROLE=api G5_DATABASE_URL=… G5_PORT=7500 G5_E2E_AUTH=true
  G5_REPOS_BASE_DIR=/tmp/native-repos G5_ALIGNMENT_UI_ENABLED=true G5_COOKIE_SECURE=false`.
- Vite: something holds 5173 silently, so Vite lands on 5174 and Playwright's own webServer
  times out. Start `npm run dev` yourself with `VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS=1
  VITE_G5_ENABLE_INTENT_LOG_VIEWER=1` (without them Fork is hidden and Workspaces-tile flows
  fail), then run with `E2E_BASE_URL=http://localhost:5174 G5_ALIGNMENT_UI_ENABLED=true`.
- Test-token user `e2e-test@g5.dev` gets org `g5labs` with Base and its children
  `e2e-test` and `demo`; approvals attached to Base give real attention items.
- Never `pkill -f <pattern>` from the Bash tool: it matches the tool's own shell and kills it.
- Only Chromium is installed: pass `--project=chromium` (CI runs Chromium only too). A
  viewport below desktop width (900px) switches the shell to the mobile layout — no tab strip,
  no `left-sidebar` — so keep the default viewport. Back-to-back runs hit the API's rate limit
  (429 on `/users/me`, org-switch refresh); avoid extra API calls in a spec (2026-10-25, #820).
- An unlabeled `docker run --name g5-native-pg ... pgvector/pgvector:pg17@<compose digest>` on
  5432 avoids the compose secrets check; the API binaries build in ~5 min with sccache warm.
  A running workspace preview uses other ports ([[in-vm-preview-runs-dev-up]]).

Related: [[rust-and-integration-test-setup]], [[g5-api-test-binaries-fill-disk]].
