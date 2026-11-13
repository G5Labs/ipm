#!/usr/bin/env bash
# =============================================================================
# seed-ci-org.sh — Seed the CI org, workspaces, and workspace host
# =============================================================================
# Creates the OAuth-backed E2E org/workspaces plus the worker service user
# directly in Postgres.
# Must run after Postgres is healthy and before the workspace host starts.
#
# Outputs:
#   G5D_ORG_ID=<uuid>
#   G5D_REPO_ID=<uuid>
#   G5D_HOST_ID=<uuid>                    (when G5_E2E_PROVISION_WORKER_HOST=1)
#   G5_HOST_JWT=<jwt>                     (when G5_E2E_PROVISION_WORKER_HOST=1)
# to stdout (for eval/export by the caller).
#
# Usage:
#   seed_env="$(./scripts/seed-ci-org.sh)"
#   eval "$seed_env"
#   docker compose ... up -d g5d-host
# =============================================================================
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-g5-test-postgres}"
DB_USER="${DB_USER:-g5}"
DB_NAME="${DB_NAME:-g5_test}"

psql_exec() {
  docker exec "$POSTGRES_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -tAc "$1"
}

first_result_line() {
  local first=""
  local line=""
  while IFS= read -r line; do
    if [[ -z "$first" ]]; then
      first="$line"
    fi
  done
  printf '%s\n' "$first"
}

sha256_hex() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s' "$1" | sha256sum | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    printf '%s' "$1" | shasum -a 256 | awk '{print $1}'
  else
    python3 - <<'PY' "$1"
import hashlib
import sys

print(hashlib.sha256(sys.argv[1].encode()).hexdigest())
PY
  fi
}

random_hex_32() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    python3 - <<'PY'
import secrets

print(secrets.token_hex(32))
PY
  fi
}

echo "--- Seeding CI org ---" >&2

E2E_ORG_NAME="${G5_E2E_TEST_ORG_NAME:-G5 Labs}"
E2E_ORG_SLUG="${G5_E2E_TEST_ORG_SLUG:-g5labs}"

# 1. Create users first. Test-token provisioning creates its default fixture
#    orgs through the production create-org cascade. This script may target a
#    lane-specific org slug, so it creates that org below and then creates its
#    shared demo workspace through the public checked-fork endpoint.
API_URL="${G5_E2E_API_URL:-http://localhost:7501}"

# Service user for workers (idempotent). Test-token provisioning resolves users
# by unique email before OAuth identity, so the seeder must do the same.
CI_WORKER_EMAIL="ci-worker@g5.internal"
USER_ID=$(psql_exec "
  SELECT id FROM users WHERE email = '${CI_WORKER_EMAIL}';
" | first_result_line)
if [[ -z "$USER_ID" ]]; then
  USER_ID=$(psql_exec "
    INSERT INTO users (email, display_name, oauth_provider, oauth_id)
    VALUES ('${CI_WORKER_EMAIL}', 'CI Worker', 'service', 'ci-worker')
    ON CONFLICT (oauth_provider, oauth_id)
    DO UPDATE SET
      email = EXCLUDED.email,
      display_name = EXCLUDED.display_name
    RETURNING id;
  " | first_result_line)
fi
echo "  user_id=$USER_ID (service)" >&2

# Member user — owns the org the operator persona creates for it (idempotent).
E2E_MEMBER_EMAIL="${G5_E2E_TEST_EMAIL:-e2e-test@g5.dev}"
E2E_MEMBER_USER_ID=$(psql_exec "
  SELECT id FROM users WHERE email = '${E2E_MEMBER_EMAIL}';
" | first_result_line)
if [[ -z "$E2E_MEMBER_USER_ID" ]]; then
  E2E_MEMBER_USER_ID=$(psql_exec "
    INSERT INTO users (email, display_name, oauth_provider, oauth_id)
    VALUES ('${E2E_MEMBER_EMAIL}', 'E2E Test User', 'test', 'e2e-test')
    ON CONFLICT (oauth_provider, oauth_id)
    DO UPDATE SET
      email = EXCLUDED.email,
      display_name = EXCLUDED.display_name
    RETURNING id;
  " | first_result_line)
fi
echo "  e2e_member_user_id=$E2E_MEMBER_USER_ID" >&2

# The shared member represents a returning user. Fresh-user tour tests create
# their own identities and retain the migration's never-shown defaults.
psql_exec "
  UPDATE users SET onboarding_step = 7, onboarding_skipped = false
  WHERE id = '${E2E_MEMBER_USER_ID}';
" >/dev/null

# Reauth user — minted at test time by issue_test_token with a fresh email
# (tc_vw_002 view_creation_wiring). issue_test_token grants fresh users
# membership only in the hardcoded g5labs/g5labs-member orgs, so dedicated
# CI-lane orgs (cicd D30/D31) must pre-provision this fixed identity or the
# test's use_seeded_org worker-token bootstrap 403s against the lane org
# (canary cb626bf5). Identity tuple matches issue_test_token's insert
# ('e2e' provider, email as oauth_id); its by-email lookup reuses this row.
# Deliberately NOT env-overridable: view_creation_wiring.rs calls
# obtain_test_token("e2e-vw-reauth@g5.dev") with a literal, so an override
# would silently desynchronize the seeded membership from the test identity.
E2E_REAUTH_EMAIL="e2e-vw-reauth@g5.dev"
# Mirror issue_test_token: look up by email FIRST — the row may already exist
# under another oauth provider, and users.email is unique, so a blind insert
# keyed only on (oauth_provider, oauth_id) would fail on that constraint.
E2E_REAUTH_USER_ID=$(psql_exec "
  SELECT id FROM users WHERE email = '${E2E_REAUTH_EMAIL}';
" | first_result_line)
if [[ -z "$E2E_REAUTH_USER_ID" ]]; then
  E2E_REAUTH_USER_ID=$(psql_exec "
    INSERT INTO users (email, display_name, oauth_provider, oauth_id)
    VALUES ('$E2E_REAUTH_EMAIL', 'E2E Test ($E2E_REAUTH_EMAIL)', 'e2e', '$E2E_REAUTH_EMAIL')
    ON CONFLICT (oauth_provider, oauth_id)
    DO UPDATE SET
      email = EXCLUDED.email,
      display_name = EXCLUDED.display_name
    RETURNING id;
  " | first_result_line)
fi
echo "  e2e_reauth_user_id=$E2E_REAUTH_USER_ID" >&2

# New user — intentionally left WITHOUT an org membership so onboarding
# first-login flows can run through the real path.
E2E_NEW_USER_EMAIL="${G5_E2E_NEW_USER_EMAIL:-e2e-new@g5.dev}"
E2E_NEW_USER_ID=$(psql_exec "
  SELECT id FROM users WHERE email = '${E2E_NEW_USER_EMAIL}';
" | first_result_line)
if [[ -z "$E2E_NEW_USER_ID" ]]; then
  E2E_NEW_USER_ID=$(psql_exec "
    INSERT INTO users (email, display_name, oauth_provider, oauth_id)
    VALUES ('${E2E_NEW_USER_EMAIL}', 'E2E New User', 'test', 'e2e-new')
    ON CONFLICT (oauth_provider, oauth_id)
    DO UPDATE SET
      email = EXCLUDED.email,
      display_name = EXCLUDED.display_name
    RETURNING id;
  " | first_result_line)
fi
echo "  e2e_new_user_id=$E2E_NEW_USER_ID" >&2

# Mint an auth token for the member user; it creates the shared demo fork below.
MEMBER_AUTH_RESP=$(curl -s "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d "{\"email\": \"${E2E_MEMBER_EMAIL}\"}")
MEMBER_TOKEN=$(printf '%s' "$MEMBER_AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
if [[ -z "$MEMBER_TOKEN" ]]; then
  echo "  ✗ Failed to mint auth token for ${E2E_MEMBER_EMAIL}" >&2
  echo "  Response: ${MEMBER_AUTH_RESP}" >&2
  exit 1
fi

# Create the org through the operator API so the creation cascade bootstraps
# its docs and code repositories (g5.organization.platform-operator.create-org).
# The test-token path makes the fixture operator persona an operator; the org
# is created for the member user as owner, as the retired self-serve call did.
# Idempotent: reuse the org if its slug already exists (non-fresh DB re-run).
E2E_OPERATOR_EMAIL="e2e-operator@g5labs.ai"
ORG_ID=$(psql_exec "SELECT id FROM orgs WHERE slug = '$E2E_ORG_SLUG';" | first_result_line)
if [[ -n "$ORG_ID" ]]; then
  echo "  org exists, reusing org_id=$ORG_ID" >&2
else
  OPERATOR_AUTH_RESP=$(curl -s "${API_URL}/api/v1/auth/test-token" \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"${E2E_OPERATOR_EMAIL}\"}")
  OPERATOR_TOKEN=$(printf '%s' "$OPERATOR_AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
  if [[ -z "$OPERATOR_TOKEN" ]]; then
    echo "  ✗ Failed to mint auth token for ${E2E_OPERATOR_EMAIL}" >&2
    echo "  Response: ${OPERATOR_AUTH_RESP}" >&2
    exit 1
  fi
  ORG_CREATE_RESP=$(curl -s "${API_URL}/api/v1/platform/orgs" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
    -d "$(jq -nc --arg name "$E2E_ORG_NAME" --arg slug "$E2E_ORG_SLUG" --arg owner "$E2E_MEMBER_EMAIL" \
      '{name:$name, slug:$slug, adminEmail:$owner}')")
  ORG_ID=$(printf '%s' "$ORG_CREATE_RESP" | jq -r '.data.org.id // empty')
  if [[ -z "$ORG_ID" ]]; then
    echo "  ✗ Failed to create org via API (POST /api/v1/platform/orgs)" >&2
    echo "  Response: ${ORG_CREATE_RESP}" >&2
    exit 1
  fi
  echo "  org_id=$ORG_ID (via operator API; repositories bootstrapped)" >&2
fi
REPO_ID=$(psql_exec "SELECT id FROM repos WHERE org_id = '$ORG_ID' AND slug = 'docs';" | first_result_line)
echo "  repo_id=$REPO_ID (docs)" >&2

# Memberships: the member is the initial Admin (added by the creation cascade).
# Ensure the service user is admin and the member membership exists (idempotent).
# The new user is intentionally left out.
psql_exec "
  INSERT INTO org_memberships (org_id, user_id, role)
  VALUES ('$ORG_ID', '$USER_ID', 'admin')
  ON CONFLICT (org_id, user_id) DO NOTHING;
" >/dev/null
psql_exec "
  INSERT INTO org_memberships (org_id, user_id, role)
  VALUES ('$ORG_ID', '$E2E_MEMBER_USER_ID', 'admin')
  ON CONFLICT (org_id, user_id) DO NOTHING;
" >/dev/null
psql_exec "
  INSERT INTO org_memberships (org_id, user_id, role)
  VALUES ('$ORG_ID', '$E2E_REAUTH_USER_ID', 'admin')
  ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'admin';
" >/dev/null
echo "  org_memberships ensured (service + member + reauth admin)" >&2

# Every E2E scenario owns what it creates through these few users, so the
# per-owner limits (g5.organization.owner-limits; defaults 5 running VMs and 50
# workspaces) would refuse the suite long before the organization VM quota it
# already runs under. Lift them for this fixture org only; a value an earlier
# run or an administrator set stays.
psql_exec "
  INSERT INTO configs (org_id, scope, scope_id, key, value)
  VALUES ('$ORG_ID', 'org', '$ORG_ID', 'max_running_vms_per_owner', '100'::jsonb),
         ('$ORG_ID', 'org', '$ORG_ID', 'max_workspaces_per_owner', '1000'::jsonb)
  ON CONFLICT (org_id, scope, scope_id, key) DO NOTHING;
" >/dev/null
echo "  per-owner limits lifted for the fixture org" >&2

# The create-org cascade creates Base. Lane-specific orgs do not exist when the
# first test token is minted, so test-token provisioning cannot create their
# demo fork. Create it through the same public clone route used by production;
# never fall back to retired org-level creation or raw refs. The downstream
# seed-test-workspace.sh stage owns the disposable e2e-test fork and must be
# able to create it from a clean exact-name reservation.
BASE_WS_ID=$(psql_exec "
  SELECT id FROM workspaces
  WHERE org_id = '$ORG_ID' AND is_base = true
  LIMIT 1;
" | first_result_line)
echo "  workspace_id=$BASE_WS_ID (base, via API)" >&2

if [[ -z "$BASE_WS_ID" || -z "$REPO_ID" ]]; then
  echo "  ✗ Missing checked Base fixture: base_workspace_id=${BASE_WS_ID:-<empty>} repo_id=${REPO_ID:-<empty>}" >&2
  exit 1
fi

FIXTURE_NAME="demo"
DEMO_WS_ID=$(psql_exec "
  SELECT id FROM workspaces
  WHERE org_id = '$ORG_ID' AND name = '${FIXTURE_NAME}' AND deleted_at IS NULL
  LIMIT 1;
" | first_result_line)
if [[ -z "$DEMO_WS_ID" ]]; then
  FORK_RESP_FILE="$(mktemp)"
  if ! FORK_HTTP_CODE=$(curl -sS -o "$FORK_RESP_FILE" -w '%{http_code}' \
    -X POST "${API_URL}/api/v1/workspaces/${BASE_WS_ID}/clone" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${MEMBER_TOKEN}" \
    -d "$(jq -nc \
      --arg name "$FIXTURE_NAME" \
      --arg key "ci-seed:${BASE_WS_ID}:${FIXTURE_NAME}" \
      '{name:$name,idempotencyKey:$key}')"); then
    FORK_HTTP_CODE="000"
  fi
  DEMO_WS_ID=$(jq -r '.data.workspace.id // empty' "$FORK_RESP_FILE")
  if [[ "$FORK_HTTP_CODE" != "201" || -z "$DEMO_WS_ID" ]]; then
    echo "  ✗ Failed to create checked ${FIXTURE_NAME} fork: HTTP ${FORK_HTTP_CODE}" >&2
    cat "$FORK_RESP_FILE" >&2 || true
    echo "" >&2
    rm -f "$FORK_RESP_FILE"
    exit 1
  fi
  rm -f "$FORK_RESP_FILE"
fi
echo "  workspace_id=$DEMO_WS_ID (${FIXTURE_NAME}, checked fork)" >&2

for CHECK_WS_ID in "$BASE_WS_ID" "$DEMO_WS_ID"; do
  STATE=$(curl -fsS "${API_URL}/api/v1/workspaces/${CHECK_WS_ID}/branches" -H "Authorization: Bearer ${MEMBER_TOKEN}")
  printf '%s' "$STATE" | jq -e '.data | length > 0 and all(.[]; .materializationStatus == "materialized" and (.headCommit | test("^[0-9a-f]{40}$")))' >/dev/null
 done
echo "  Base and checked demo fork have materialized Git refs" >&2

# 7. Mint an authenticated member token for Runtime Config and host setup.
API_URL="${G5_E2E_API_URL:-http://localhost:7501}"
AUTH_RESP=$(curl -sf "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d "{\"email\": \"${E2E_MEMBER_EMAIL}\"}")
AUTH_TOKEN=$(printf '%s' "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
if [[ -z "$AUTH_TOKEN" ]]; then
  echo "  ✗ Failed to mint auth token for ${E2E_MEMBER_EMAIL}" >&2
  echo "  Response: ${AUTH_RESP}" >&2
  exit 1
fi

# 8. Seed a non-live direct-Anthropic API key so agentic fixture tasks pass the
# canonical pre-dispatch resolver. ANTHROPIC_AUTH_TOKEN is reserved for the
# OpenRouter bearer-token projection; direct Claude OAuth is represented by
# CLAUDE_CODE_OAUTH_TOKEN. Real LLM call paths remain stubbed at the worker.
CONFIG_RESP_FILE="$(mktemp)"
CONFIG_HTTP_CODE=$(curl -s -o "$CONFIG_RESP_FILE" -w '%{http_code}' \
  -X PUT "${API_URL}/api/v1/orgs/${ORG_ID}/config" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${AUTH_TOKEN}" \
  -d '{"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}' || echo "000")
if [[ "$CONFIG_HTTP_CODE" != "200" ]]; then
  echo "  ✗ Failed to seed org Runtime Config ANTHROPIC_API_KEY: HTTP ${CONFIG_HTTP_CODE}" >&2
  echo "  Response body:" >&2
  cat "$CONFIG_RESP_FILE" >&2 || true
  echo "" >&2
  rm -f "$CONFIG_RESP_FILE"
  exit 1
fi
rm -f "$CONFIG_RESP_FILE"
echo "  org_config seeded ANTHROPIC_API_KEY (Runtime Config)" >&2

# 9. Pin the workspace guest image used by explicit Resume. The lifecycle
# resolver fails closed when the org has no default, and the host must receive
# the same immutable digest that was inspected here rather than independently
# resolving a mutable tag later.
WORKSPACE_BASE_IMAGE="${G5_E2E_WORKSPACE_BASE_IMAGE:-${G5D_HOST_BASE_IMAGE:-mirror.gcr.io/library/buildpack-deps:resolute}}"
if [[ "$WORKSPACE_BASE_IMAGE" == *@sha256:* ]]; then
  WORKSPACE_BASE_IMAGE_REF="$WORKSPACE_BASE_IMAGE"
else
  docker pull "$WORKSPACE_BASE_IMAGE" >/dev/null
  WORKSPACE_BASE_IMAGE_REF=$(docker image inspect \
    --format '{{range .RepoDigests}}{{println .}}{{end}}' "$WORKSPACE_BASE_IMAGE" \
    | first_result_line)
fi
if ! [[ "$WORKSPACE_BASE_IMAGE_REF" =~ ^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$ ]]; then
  echo "  ✗ Could not resolve immutable workspace guest image: ${WORKSPACE_BASE_IMAGE}" >&2
  exit 1
fi
IMAGE_POLICY_RESP=$(curl -sS -X PUT \
  "${API_URL}/api/v1/orgs/${ORG_ID}/worker-hosts/image-policy" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${AUTH_TOKEN}" \
  -d "{\"baseImageRef\":\"${WORKSPACE_BASE_IMAGE_REF}\"}")
IMAGE_POLICY_REF=$(printf '%s' "$IMAGE_POLICY_RESP" | jq -r '.data.resolvedImageRef // empty')
if [[ "$IMAGE_POLICY_REF" != "$WORKSPACE_BASE_IMAGE_REF" ]]; then
  echo "  ✗ Failed to seed org workspace image policy" >&2
  echo "  Response: ${IMAGE_POLICY_RESP}" >&2
  exit 1
fi
echo "  workspace image policy=$WORKSPACE_BASE_IMAGE_REF" >&2

# 10. The candidate microVM lane must register through the production host-JWT
# surface so the durable host identity proves which `g5d host` runtime owns the VM.
# Cloud features are disabled in the hermetic E2E stack, so this creates only
# the durable worker_hosts row and credential; the Cloud Build step starts the
# already-built host container itself.
HOST_ID=""
HOST_JWT=""
if [[ "${G5_E2E_PROVISION_WORKER_HOST:-0}" == "1" ]]; then
  # Advertised capacity must agree with the host container's runtime
  # G5D_HOST_MAX_MICROVMS. Callers that omit the override retain the
  # historical three-slot default.
  HOST_WORKSPACE_VM_LIMIT="${G5_E2E_HOST_WORKSPACE_VM_LIMIT:-3}"
  if ! [[ "$HOST_WORKSPACE_VM_LIMIT" =~ ^[1-9][0-9]*$ ]]; then
    echo "  ✗ G5_E2E_HOST_WORKSPACE_VM_LIMIT must be a positive integer" >&2
    exit 1
  fi
  # The API caps workspaceVmLimit at the declared instance type's vCPU count
  # (routes/worker_hosts.rs, no CPU oversubscription). Cloud features are off
  # in the hermetic stack, so the type is metadata here: callers advertising
  # more than four slots declare an allowlisted larger shape (c3-standard-8).
  HOST_INSTANCE_TYPE="${G5_E2E_HOST_INSTANCE_TYPE:-c3-standard-4}"
  # This API call creates only the durable host row and credential. Cloud Build
  # launches the separately built candidate container, so use an explicit
  # immutable CI sentinel instead of relying on a production image fallback.
  HOST_BASE_IMAGE_REF="${G5_E2E_HOST_BASE_IMAGE_REF:-ghcr.io/g5labs/g5d-worker-ci@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
  # This hermetic fixture uses smaller private disks. Configure V through the
  # organization-admin API before creating the immutable host allocation.
  DISK_SETTINGS=$(curl -fsS -X PUT "${API_URL}/api/v1/orgs/${ORG_ID}/admin-settings" \
    -H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d '{"vmDiskAllowanceGib":10}')
  printf '%s' "$DISK_SETTINGS" | jq -e '.data.vmDiskAllowanceGib == 10' >/dev/null
  HOST_RESP=$(curl -sS -X POST "${API_URL}/api/v1/orgs/${ORG_ID}/worker-hosts" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "{\"region\":\"us-central1\",\"instanceType\":\"${HOST_INSTANCE_TYPE}\",\"workspaceVmLimit\":${HOST_WORKSPACE_VM_LIMIT},\"baseImageRef\":\"${HOST_BASE_IMAGE_REF}\"}")
  HOST_ID=$(printf '%s' "$HOST_RESP" | jq -r '.data.hostId // empty')
  HOST_JWT=$(printf '%s' "$HOST_RESP" | jq -r '.data.hostJwt // empty')
  if [[ -z "$HOST_ID" || -z "$HOST_JWT" ]]; then
    echo "  ✗ Failed to provision DB-only CI worker host" >&2
    echo "  Response: ${HOST_RESP}" >&2
    exit 1
  fi
  HOST_DISK_MIB=$(printf '%s' "$HOST_RESP" | jq -er '.data.diskAllocation.vmDiskMib | select(. > 0)')
  HOST_DISK_GIB=$(printf '%s' "$HOST_RESP" | jq -er '.data.diskAllocation.diskGib | select(. > 0)')
  printf '%s' "$HOST_RESP" | jq -e --argjson slots "$HOST_WORKSPACE_VM_LIMIT" \
    '.data.diskAllocation.slots == $slots' >/dev/null
  echo "  worker_host_id=$HOST_ID (DB-only host-JWT bootstrap via API)" >&2
fi

echo "--- Seed complete ---" >&2

# Output for eval
echo "export G5D_ORG_ID=$ORG_ID"
echo "export G5D_REPO_ID=$REPO_ID"
if [[ -n "$HOST_ID" ]]; then
  echo "export G5D_HOST_ID=$HOST_ID"
  echo "export G5_HOST_JWT=$HOST_JWT"
  echo "export G5D_SCRATCH_DISK_MIB=$HOST_DISK_MIB"
  echo "export G5D_HOST_XFS_SIZE_GB=$HOST_DISK_GIB"
fi
