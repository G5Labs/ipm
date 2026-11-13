#!/usr/bin/env bash
# =============================================================================
# seed-ontology-examples.sh — Create a workspace with only v1/v2 ontology
#                              examples from archive/examples/
# =============================================================================
# Idempotent: cleans up any previous "ontology-examples" workspace, then
# recreates from scratch. Safe to re-run.
#
# Sets up:
#   1. Workspace named "ontology-examples" in the seeded org
#   2. Dedicated repo + workspace membership
#   3. Copies v1 and v2 ontology .g5.md files from archive/examples/
#   4. Seeds ontology nodes (10 total: 5 per version)
#
# Usage:
#   ./scripts/seed-ontology-examples.sh [api-url]
#   Default API: http://localhost:${G5_PORT_API:-7500}
#
# Requires: curl, jq, docker, running g5-dev stack with G5_E2E_AUTH=true
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/seed-workspace-model.sh"
# archive/examples/ is a legacy auxiliary input.
ARCHIVE_DIR="$REPO_ROOT/archive/examples"

API_URL="${1:-http://localhost:${G5_PORT_API:-7500}}"
REPOS_VOLUME="g5-lean_api_repos"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
AUTH_EMAIL="demo@g5labs.ai"
WS_NAME="ontology-examples"

# ─── Verify archive exists ───────────────────────────────────────────────────
if [ ! -d "$ARCHIVE_DIR/ontology_v1" ] || [ ! -d "$ARCHIVE_DIR/ontology_v2" ]; then
  echo "  ✗ Missing archive/examples/ontology_v1 or ontology_v2"
  echo "  Expected at: $ARCHIVE_DIR"
  exit 1
fi

# ─── Auto-detect API URL ─────────────────────────────────────────────────────
API_PORT="${G5_PORT_API:-7500}"
if ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
  if curl -sf "http://localhost:${API_PORT}/healthz" > /dev/null 2>&1; then
    API_URL="http://localhost:${API_PORT}"
  else
    API_IP=$(docker inspect g5-dev-api --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && curl -sf "http://${API_IP}:7500/healthz" > /dev/null 2>&1; then
      API_URL="http://${API_IP}:7500"
    else
      echo "  ✗ Cannot reach g5-api at localhost:${API_PORT} or via container IP"
      echo "  Make sure dev stack is running: just dev-up"
      exit 1
    fi
  fi
fi

echo "=== G5 Ontology Examples Seed ==="
echo "  API:     $API_URL"
echo "  Archive: $ARCHIVE_DIR"
echo ""

# ─── Helpers ──────────────────────────────────────────────────────────────────

api_post() {
  local path="$1"; local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -tAc "$1"
}

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -c "$1" >/dev/null 2>&1 || true
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
AUTH_RESP=$(api_post "/api/v1/auth/test-token" '{"email": "demo@g5labs.ai"}' 2>/dev/null || echo "")
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
  USER_ID=$(psql_exec "SELECT id::text FROM users WHERE email = 'demo@g5labs.ai';" 2>/dev/null | tr -d '[:space:]')
  if [ -n "$USER_ID" ]; then
    psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${USER_ID}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
    echo "  ✓ Ensured user membership in workspace host org"
  fi
else
  ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)
fi
if [ -z "$ORG_ID" ]; then
  echo "  ✗ No orgs found"
  exit 1
fi
echo "  ✓ Org: $ORG_ID"

# ─── Step 3: Cleanup previous workspace ──────────────────────────────────────

echo ""
echo "=== Step 3: Cleanup previous '${WS_NAME}' workspace ==="

OLD_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WS_NAME}' LIMIT 1;" | tr -d '[:space:]')

if [ -n "$OLD_WS_ID" ]; then
  echo "  Found previous workspace: $OLD_WS_ID"
  OLD_REPO_ID=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}' LIMIT 1;" | tr -d '[:space:]')

  # Delete in dependency order
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
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine \
        rm -rf "/data/repos/${ORG_ID}/${OLD_REPO_ID}" 2>/dev/null || true
      echo "  ✓ Deleted old repo $OLD_REPO_ID"
    fi
  fi
  echo "  ✓ Previous workspace cleaned up"
else
  echo "  No previous workspace found (first run)"
fi

# Clean orphaned repos
ORPHAN_REPO_ID=$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = 'ontology-examples-repo' LIMIT 1;" | tr -d '[:space:]')
if [ -n "$ORPHAN_REPO_ID" ]; then
  psql_exec_quiet "DELETE FROM workspace_repos WHERE repo_id = '${ORPHAN_REPO_ID}';"
  psql_exec_quiet "DELETE FROM repos WHERE id = '${ORPHAN_REPO_ID}';"
  docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine \
    rm -rf "/data/repos/${ORG_ID}/${ORPHAN_REPO_ID}" 2>/dev/null || true
  echo "  ✓ Orphaned repo cleaned up"
fi

# ─── Step 4: Create workspace ─────────────────────────────────────────────────

echo ""
echo "=== Step 4: Create workspace ==="
WS_ID=$(seed_create_fixture_workspace "$WS_NAME")
if [ -z "$WS_ID" ]; then
  echo "  ✗ Failed to create workspace"
  echo "  Response: $WS_ID"
  exit 1
fi
echo "  ✓ Workspace: $WS_ID"

# ─── Step 5: Create repo + workspace membership ───────────────────────────────────

echo ""
echo "=== Step 5: Create repo ==="

TEST_REPO_ID=$(python3 -c "import uuid; print(uuid.uuid4())")

echo "  Repo UUID: $TEST_REPO_ID"

SQL=$(cat <<EOSQL
BEGIN;

INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
VALUES (
  '${TEST_REPO_ID}',
  '${ORG_ID}',
  'ontology-examples-repo',
  'ontology-examples-repo',
  'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/ontology-examples-repo',
  'main',
  now()
);

INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
VALUES ('${ORG_ID}', '${WS_ID}', '${TEST_REPO_ID}', 'code', 'code');

COMMIT;
EOSQL
)

docker exec "$PG_CONTAINER" psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -c "$SQL"
echo "  ✓ Repo + workspace membership created"

# ─── Step 6: Copy ontology files to Docker volume ─────────────────────────────

echo ""
echo "=== Step 6: Provision ontology files ==="

BASE="/data/repos/${ORG_ID}/${TEST_REPO_ID}"

# Create a temp tarball of the v1 and v2 examples, then extract into the
# Docker volume via a container. This avoids heredoc quoting issues with
# rich markdown content.

TMPDIR_TAR=$(mktemp -d)
trap "rm -rf $TMPDIR_TAR" EXIT

# Build the directory structure to match the API's path derivation:
#   API reads:  .semlift/specs/{component_id}.semlift.md
#   So component_id "ontology_v1" → .semlift/specs/ontology_v1.semlift.md
#      component_id "ontology_v1/core_data_layer" → .semlift/specs/ontology_v1/core_data_layer.semlift.md
mkdir -p "$TMPDIR_TAR/.semlift/specs/ontology_v1/core_data_layer/models"
mkdir -p "$TMPDIR_TAR/.semlift/specs/ontology_v2/core_data_layer/models"

# Copy v1 files — root.g5.md goes UP one level as ontology_v1.semlift.md
cp "$ARCHIVE_DIR/ontology_v1/root.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v1.semlift.md"
cp "$ARCHIVE_DIR/ontology_v1/core_data_layer.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v1/core_data_layer.semlift.md"
cp "$ARCHIVE_DIR/ontology_v1/core_data_layer/models.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v1/core_data_layer/models.semlift.md"
cp "$ARCHIVE_DIR/ontology_v1/core_data_layer/models/company.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v1/core_data_layer/models/company.semlift.md"
cp "$ARCHIVE_DIR/ontology_v1/core_data_layer/models/dashboard.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v1/core_data_layer/models/dashboard.semlift.md"

# Copy v2 files — same pattern
cp "$ARCHIVE_DIR/ontology_v2/root.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v2.semlift.md"
cp "$ARCHIVE_DIR/ontology_v2/core_data_layer.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v2/core_data_layer.semlift.md"
cp "$ARCHIVE_DIR/ontology_v2/core_data_layer/models.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v2/core_data_layer/models.semlift.md"
cp "$ARCHIVE_DIR/ontology_v2/core_data_layer/models/company.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v2/core_data_layer/models/company.semlift.md"
cp "$ARCHIVE_DIR/ontology_v2/core_data_layer/models/dashboard.g5.md" \
   "$TMPDIR_TAR/.semlift/specs/ontology_v2/core_data_layer/models/dashboard.semlift.md"

# Also create a minimal semlift.yaml
cat > "$TMPDIR_TAR/semlift.yaml" << 'YAML'
version: "1"
project_name: ontology-examples
specs_dir: .semlift/specs
YAML

# Create tarball
TAR_FILE="$TMPDIR_TAR/ontology-seed.tar"
(cd "$TMPDIR_TAR" && tar cf "$TAR_FILE" semlift.yaml .semlift/)

# Extract into Docker volume and init git repo
docker run --rm \
  -v "${REPOS_VOLUME}:/data/repos" \
  -v "$TAR_FILE:/tmp/ontology-seed.tar:ro" \
  alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
rm -rf ${BASE}
mkdir -p ${BASE}
cd ${BASE}
tar xf /tmp/ontology-seed.tar
git config --global --add safe.directory '*'
git init -b main
git config user.email 'test@g5.dev'
git config user.name 'G5 Ontology Seed'
git config core.sharedRepository 0777
git add .
git commit -m 'Seed ontology v1 and v2 examples'
chmod -R 777 ${BASE}
echo 'DONE'
"

echo "  ✓ Ontology files provisioned"
echo "    v1: 5 files (.semlift/specs/ontology_v1.semlift.md + ontology_v1/*.semlift.md)"
echo "    v2: 5 files (.semlift/specs/ontology_v2.semlift.md + ontology_v2/*.semlift.md)"

# ─── Step 8: Configure workspace ─────────────────────────────────────────────

echo ""
echo "=== Step 8: Configure workspace ==="

# This ontology fixture contains one code repository.
seed_capture_fixture_parent "$WS_ID" "$WS_NAME"

# ─── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "============================================="
echo "  Ontology examples workspace ready!"
echo "============================================="
echo ""
echo "  Workspace: ${WS_NAME}"
echo "  Workspace ID: $WS_ID"
echo "  Org ID:       $ORG_ID"
echo "  Repo ID:      $TEST_REPO_ID"
echo ""
echo "  Ontology tree:"
echo "    ├── Ontology v1 (Banking CRM)"
echo "    │   └── Core Data Layer"
echo "    │       └── Models"
echo "    │           ├── Company"
echo "    │           └── Dashboard"
echo "    └── Ontology v2 (Banking CRM)"
echo "        └── Core Data Layer"
echo "            └── Models"
echo "                ├── Company"
echo "                └── Dashboard"
echo ""
echo "  Open in browser:"
echo "    http://localhost:\${G5_PORT_EDGE:-3000}"
