#!/usr/bin/env bash
# =============================================================================
# seed-tinydb-workspace.sh — Create/reset a TinyDB source workspace for uplift
# =============================================================================
# Idempotent: cleans up any previous "e2e-tinydb" workspace + repo, then
# recreates everything from scratch. Safe to re-run before each live-LLM batch.
#
# Sets up:
#   1. Workspace named "e2e-tinydb" in the seeded org (via API)
#   2. Dedicated git-backed repo + workspace membership (via SQL)
#   3. TinyDB Python source tree on the Docker volume (source only, no specs)
#   4. semlift.yaml configured for uplift/downlift workflows
#
# Usage:
#   ./scripts/seed-tinydb-workspace.sh [api-url] [tinydb-src-dir]
#   Default API: http://localhost:${G5_PORT_API:-7500}
#   Default source dir: ../external_repos/tinydb/tinydb_src
#
# Outputs:
#   Writes .env.tinydb.test with G5_TINYDB_TEST_WORKSPACE_ID and
#   G5_TINYDB_TEST_REPO_ID.
#
# Requires: curl, jq, docker, python3, running stack with test-token auth
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/seed-workspace-model.sh"
DEFAULT_ENV_FILE="$REPO_ROOT/.env"

if [ -f "$DEFAULT_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  source "$DEFAULT_ENV_FILE"
  set +a
fi

API_URL="${1:-http://localhost:${G5_PORT_API:-7500}}"
TINYDB_SRC_DIR="${2:-$REPO_ROOT/external_repos/tinydb/tinydb_src}"
REPOS_VOLUME="${REPOS_VOLUME:-g5-lean_api_repos}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
DB_NAME="${DB_NAME:-g5_dev}"
AUTH_EMAIL="${G5_E2E_TEST_EMAIL:-e2e-test@g5.dev}"

WORKSPACE_NAME="e2e-tinydb"
REPO_NAME="e2e-tinydb-repo"
ENV_FILE="$REPO_ROOT/.env.tinydb.test"

# ─── Auto-detect API URL ─────────────────────────────────────────────────────
API_PORT="${G5_PORT_API:-7500}"
if [ "$API_URL" = "auto" ] || ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
  if curl -sf "http://localhost:${API_PORT}/healthz" > /dev/null 2>&1; then
    API_URL="http://localhost:${API_PORT}"
  else
    API_IP=$(docker inspect "$API_CONTAINER" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && curl -sf "http://${API_IP}:7500/healthz" > /dev/null 2>&1; then
      API_URL="http://${API_IP}:7500"
    else
      echo "  ✗ Cannot reach g5-api at localhost:${API_PORT} or via container IP"
      echo "  Make sure the target stack is running"
      exit 1
    fi
  fi
fi

if [ ! -d "$TINYDB_SRC_DIR" ]; then
  echo "  ✗ TinyDB source directory not found: $TINYDB_SRC_DIR"
  exit 1
fi

if [ ! -f "$TINYDB_SRC_DIR/__init__.py" ]; then
  echo "  ✗ TinyDB source directory is missing __init__.py: $TINYDB_SRC_DIR"
  exit 1
fi

echo "=== G5 TinyDB Workspace Seed ==="
echo "  API:       $API_URL"
echo "  Source:    $TINYDB_SRC_DIR"
echo "  Workspace: $WORKSPACE_NAME"
echo "  LLM:       ${G5D_LLM_PROVIDER:-<unset>} / ${G5D_LLM_MODEL:-<unset>}"
echo ""

if [ -z "${G5D_LLM_PROVIDER:-}" ] || [ -z "${G5D_LLM_MODEL:-}" ]; then
  echo "  ✗ G5D_LLM_PROVIDER and G5D_LLM_MODEL must be set. Configure .env first."
  exit 1
fi

# ─── Helpers ─────────────────────────────────────────────────────────────────

api_post() {
  local path="$1"; local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "$1"
}

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$1" >/dev/null 2>&1 || true
}


# ─── Wait for API ────────────────────────────────────────────────────────────

echo "Waiting for g5-api..."
for i in $(seq 1 30); do
  if curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
    echo "  ✓ API is healthy"
    break
  fi
  [ "$i" -eq 30 ] && { echo "  ✗ API not healthy after 30 seconds"; exit 1; }
  sleep 1
done

# ─── Step 1: Authenticate ────────────────────────────────────────────────────

echo ""
echo "=== Step 1: Authenticate ==="
AUTH_RESP=$(api_post "/api/v1/auth/test-token" "{\"email\": \"${AUTH_EMAIL}\"}" 2>/dev/null || echo "")
AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty' 2>/dev/null)
if [ -z "$AUTH_TOKEN" ]; then
  echo "  ✗ Failed to get auth token"
  echo "  Response: $AUTH_RESP"
  exit 1
fi
echo "  ✓ Got auth token"

api_authed_post() {
  local path="$1"; local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

api_authed_get() {
  local path="$1"
  curl -sf "${API_URL}${path}" \
    -H "Authorization: Bearer ${AUTH_TOKEN}"
}

api_authed_put() {
  local path="$1"; local data="$2"
  curl -sf -X PUT "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

# ─── Step 2: Get org ID ──────────────────────────────────────────────────────

echo ""
echo "=== Step 2: Get org ==="

HOST_ORG_ID=$(psql_exec "SELECT org_id::text FROM worker_hosts WHERE status = 'ready' ORDER BY last_heartbeat DESC NULLS LAST LIMIT 1;" 2>/dev/null | tr -d '[:space:]')
ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")

if [ -n "$HOST_ORG_ID" ]; then
  ORG_ID="$HOST_ORG_ID"
  echo "  Using workspace host org: $ORG_ID"

  USER_ID=$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]')
  if [ -n "$USER_ID" ]; then
    psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${USER_ID}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
    echo "  ✓ Ensured user membership in workspace host org"
  fi
else
  ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // .data[0].id // empty' 2>/dev/null)
fi

if [ -z "$ORG_ID" ]; then
  echo "  ✗ No orgs found"
  exit 1
fi
echo "  ✓ Org: $ORG_ID"

# ─── Step 3: Cleanup previous TinyDB workspace ──────────────────────────────

echo ""
echo "=== Step 3: Cleanup previous TinyDB workspace ==="

OLD_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}' LIMIT 1;" | tr -d '[:space:]')

if [ -n "$OLD_WS_ID" ]; then
  echo "  Found previous workspace: $OLD_WS_ID"

  OLD_REPO_ID=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}' LIMIT 1;" | tr -d '[:space:]')
  REPO_SLUG=""

  RUNNING_TASKS=$(psql_exec "SELECT id::text FROM tasks WHERE workspace_id = '${OLD_WS_ID}' AND status IN ('queued','running','waiting');" 2>/dev/null || echo "")
  if [ -n "$RUNNING_TASKS" ]; then
    echo "  Cancelling active tasks via API..."
    for TASK_ID in $RUNNING_TASKS; do
      TASK_ID=$(echo "$TASK_ID" | tr -d '[:space:]')
      [ -z "$TASK_ID" ] && continue
      CANCEL_RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
        "${API_URL}/api/v1/tasks/${TASK_ID}/cancel" \
        -H "Authorization: Bearer ${AUTH_TOKEN}" \
        -H "Content-Type: application/json" 2>/dev/null || echo "000")
      echo "    Task ${TASK_ID}: cancel → HTTP ${CANCEL_RESP}"
    done
    sleep 3
  fi

  echo "  Cleaning DB rows..."
  psql_exec_quiet "DELETE FROM session_inputs WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM provider_requests WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM task_dependencies WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM task_attempts WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM timeline_entries WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM tasks WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM chat_sessions WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM docs WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_settings WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_forks WHERE source_workspace_id = '${OLD_WS_ID}' OR target_workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_ownership_transfers WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM worker_vm_events WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_vms WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_flushes WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec "DELETE FROM workspaces WHERE id = '${OLD_WS_ID}';" >/dev/null
  seed_remove_fixture_parent "$OLD_WS_ID"

  if [ -n "$OLD_REPO_ID" ]; then
    REPO_SLUG=$(psql_exec "SELECT slug FROM repos WHERE id = '${OLD_REPO_ID}';" | tr -d '[:space:]')
    if [ "$REPO_SLUG" != "docs" ]; then
      psql_exec_quiet "DELETE FROM repos WHERE id = '${OLD_REPO_ID}';"
      echo "  ✓ Deleted repo row $OLD_REPO_ID"
    fi
  fi

  if [ -n "$OLD_REPO_ID" ] && [ "$REPO_SLUG" != "docs" ]; then
    docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine \
      rm -rf "/data/repos/${ORG_ID}/${OLD_REPO_ID}" 2>/dev/null || true
    echo "  ✓ Removed repo files from volume"
  fi

  echo "  ✓ Previous TinyDB workspace cleaned up"
else
  echo "  No previous TinyDB workspace found"
fi

ORPHAN_REPO_ID=$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = '${REPO_NAME}' LIMIT 1;" | tr -d '[:space:]')
if [ -n "$ORPHAN_REPO_ID" ]; then
  echo "  Cleaning orphaned repo: $ORPHAN_REPO_ID"
  psql_exec_quiet "DELETE FROM workspace_repos WHERE repo_id = '${ORPHAN_REPO_ID}';"
  psql_exec_quiet "DELETE FROM repos WHERE id = '${ORPHAN_REPO_ID}';"
  docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine \
    rm -rf "/data/repos/${ORG_ID}/${ORPHAN_REPO_ID}" 2>/dev/null || true
  echo "  ✓ Orphaned repo cleaned up"
fi

# ─── Step 4: Create workspace via API ───────────────────────────────────────

echo ""
echo "=== Step 4: Create TinyDB workspace ==="
WS_ID=$(seed_create_fixture_workspace "$WORKSPACE_NAME")
if [ -z "$WS_ID" ]; then
  echo "  ✗ Failed to create workspace"
  echo "  Response: $WS_ID"
  exit 1
fi
echo "  ✓ Workspace: $WS_ID"

# ─── Step 5: Create dedicated repo + workspace branch ───────────────────────

echo ""
echo "=== Step 5: Create TinyDB repo ==="

TEST_REPO_ID=$(python3 -c "import uuid; print(uuid.uuid4())")

SQL=$(cat <<EOSQL
BEGIN;

INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
VALUES (
  '${TEST_REPO_ID}',
  '${ORG_ID}',
  '${REPO_NAME}',
  '${REPO_NAME}',
  'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/${REPO_NAME}',
  'main',
  now()
);

INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
VALUES ('${ORG_ID}', '${WS_ID}', '${TEST_REPO_ID}', 'code', 'code');

COMMIT;
EOSQL
)

docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$SQL" >/dev/null
echo "  ✓ Repo + workspace membership created"

# ─── Step 6: Provision TinyDB source files on volume ────────────────────────

echo ""
echo "=== Step 6: Provision TinyDB source files ==="

BASE="/data/repos/${ORG_ID}/${TEST_REPO_ID}"

docker run --rm \
  -v "${REPOS_VOLUME}:/data/repos" \
  -v "${TINYDB_SRC_DIR}:/tinydb_src:ro" \
  alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
rm -rf ${BASE}
mkdir -p ${BASE}/tinydb

cp -R /tinydb_src/. ${BASE}/tinydb/

cat > ${BASE}/semlift.yaml << 'YAML'
version: \"1\"
project_name: tinydb
specs_dir: .semlift/specs
views:
  fan_out_sync: true
  generate_code: true
YAML

cd ${BASE}
git config --global --add safe.directory '*'
git init -b main
git config user.email 'test@g5.dev'
git config user.name 'G5 E2E Test'
git config core.sharedRepository 0777
git add .
git commit -m 'Initial TinyDB source repo'
chmod -R 777 ${BASE}
"

echo "  ✓ TinyDB source repo provisioned at ${BASE}"

TEST_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
git -C ${BASE} rev-parse HEAD
")
TEST_REPO_HEAD=$(printf '%s' "$TEST_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$TEST_REPO_HEAD" ] || [ ${#TEST_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve TinyDB repo HEAD"
  exit 1
fi
seed_capture_fixture_parent "$WS_ID" "$WORKSPACE_NAME"


# This source-only fixture has no docs repo. Reusing the code repo id here makes
# assignments advertise fake docs/reference/guides/views mounts that have no durable source.
psql_exec_quiet "UPDATE workspaces SET docs_repo_id = NULL WHERE id = '${WS_ID}';"


# ─── Step 7: Write .env.tinydb.test ─────────────────────────────────────────

echo ""
echo "=== Step 7: Write .env.tinydb.test ==="
cat > "$ENV_FILE" <<EOF
# Auto-generated by seed-tinydb-workspace.sh — do not edit manually.
# Source this file before running TinyDB live uplift tests:
#   source .env.tinydb.test
G5_TINYDB_TEST_WORKSPACE_ID=${WS_ID}
G5_TINYDB_TEST_REPO_ID=${TEST_REPO_ID}
G5_TINYDB_TEST_ORG_ID=${ORG_ID}
G5_TINYDB_TEST_WORKSPACE_NAME=${WORKSPACE_NAME}
G5_E2E_API_URL=${API_URL}
G5D_ORG_ID=${ORG_ID}
G5D_LLM_PROVIDER=${G5D_LLM_PROVIDER}
G5D_LLM_MODEL=${G5D_LLM_MODEL}
EOF
echo "  ✓ Written to $ENV_FILE"

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo "============================================="
echo "  TinyDB workspace ready!"
echo "============================================="
echo ""
echo "  Workspace ID:  $WS_ID"
echo "  Repo ID:       $TEST_REPO_ID"
echo "  Org ID:        $ORG_ID"
echo "  Repo files:    /data/repos/${ORG_ID}/${TEST_REPO_ID}/"
echo "  Source files:  $(find "$TINYDB_SRC_DIR" -maxdepth 1 -type f | wc -l | tr -d '[:space:]') package files"
echo ""
echo "  Run tests:"
echo "    source .env.tinydb.test"
echo "    cargo test -p g5-e2e --features live-llm -- test_live_llm_tinydb_uplift_from_seeded_workspace --nocapture --test-threads=1"
echo ""
echo "  Reset before next run:"
echo "    just tinydb-seed"
