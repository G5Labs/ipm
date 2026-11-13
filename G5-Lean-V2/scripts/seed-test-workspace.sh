#!/usr/bin/env bash
# =============================================================================
# seed-test-workspace.sh — Create/reset an isolated test workspace (mini-calc)
# =============================================================================
# Idempotent: cleans up any previous target workspace + repos, then
# recreates everything from scratch. Safe to re-run before every test batch.
#
# Sets up:
#   1. A checked fork named G5_E2E_TEST_WORKSPACE_NAME (default "e2e-test")
#      from the seeded org's Base workspace
#   2. Mini-calc code repo files on the fork's exact shared branch
#   4. Docs repo view and Project Doc files plus docs-owned local-grounding records
#   5. Lightweight support code repo for multi-repo overlay browser fixtures
#   6. Four current ontology Markdown files plus spec/source Git fixtures
#
# Usage:
#   ./scripts/seed-test-workspace.sh [api-url]
#   Default API: http://localhost:${G5_PORT_API:-7500}
#
# Outputs:
#   Writes .env.test with workspace, code repo, and docs repo identifiers.
#
# Requires: curl, jq, docker, running stack with test-token auth enabled
# =============================================================================
set -Eeuo pipefail
trap 'echo "ERROR: seed-test-workspace.sh failed at line $LINENO (exit $?)" >&2' ERR

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

API_URL="${1:-http://localhost:${G5_PORT_API:-7500}}"
REPOS_VOLUME="${REPOS_VOLUME:-g5-lean_api_repos}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
DB_NAME="${DB_NAME:-g5_dev}"
AUTH_EMAIL="${G5_E2E_TEST_EMAIL:-e2e-test@g5.dev}"
WORKSPACE_NAME="${G5_E2E_TEST_WORKSPACE_NAME:-e2e-test}"
REQUESTED_ORG_ID="${G5_E2E_TEST_ORG_ID:-${G5D_ORG_ID:-${G5_ORG_ID:-}}}"
REPO_SLUG_PREFIX="${G5_E2E_TEST_REPO_SLUG_PREFIX:-$WORKSPACE_NAME}"
CODE_REPO_SLUG="${G5_E2E_TEST_CODE_REPO_SLUG:-${REPO_SLUG_PREFIX}-code-repo}"
DOCS_REPO_SLUG="${G5_E2E_TEST_DOCS_REPO_SLUG:-${REPO_SLUG_PREFIX}-docs-repo}"
CODE_REPO_NAME="${G5_E2E_TEST_CODE_REPO_NAME:-$CODE_REPO_SLUG}"
DOCS_REPO_NAME="${G5_E2E_TEST_DOCS_REPO_NAME:-$DOCS_REPO_SLUG}"
if [[ "$WORKSPACE_NAME" == "e2e-test" ]]; then
  SUPPORT_REPO_SLUG="${G5_E2E_OVERLAY_SUPPORT_REPO_SLUG:-e2e-overlay-support-repo}"
  ID_SCOPE="${G5_E2E_TEST_ID_SCOPE:-}"
else
  SUPPORT_REPO_SLUG="${G5_E2E_OVERLAY_SUPPORT_REPO_SLUG:-${REPO_SLUG_PREFIX}-overlay-support-repo}"
  ID_SCOPE="${G5_E2E_TEST_ID_SCOPE:-$WORKSPACE_NAME}"
fi
SUPPORT_REPO_NAME="${G5_E2E_OVERLAY_SUPPORT_REPO_NAME:-$SUPPORT_REPO_SLUG}"

# ─── Auto-detect API URL ─────────────────────────────────────────────────────
API_PORT="${G5_PORT_API:-7500}"
if ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
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

# Reuse the pinned fleet image with Git installed; fixture operations must not
# depend on repeated, hidden package downloads from an external Alpine mirror.
bash "$SCRIPT_DIR/ensure-base-images.sh" base

echo "=== G5 Test Workspace Seed ==="
echo "  API:  $API_URL"
echo "  Workspace: $WORKSPACE_NAME"
echo ""

# ─── Helper: authenticated API calls ─────────────────────────────────────────

api_request() {
  local method="$1"; local path="$2"; local data="${3:-}"; local auth_token="${4:-}"
  local resp_file err_file status body curl_err

  resp_file="$(mktemp)"
  err_file="$(mktemp)"

  local curl_args=(-sS -o "$resp_file" -w "%{http_code}" -X "$method" "${API_URL}${path}")
  if [ -n "$data" ]; then
    curl_args+=(-H "Content-Type: application/json" -d "$data")
  fi
  if [ -n "$auth_token" ]; then
    curl_args+=(-H "Authorization: Bearer ${auth_token}")
  fi

  if ! status="$(curl "${curl_args[@]}" 2>"$err_file")"; then
    status="000"
  fi

  body="$(cat "$resp_file" 2>/dev/null || true)"
  curl_err="$(tr '\n' ' ' < "$err_file" | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"
  rm -f "$resp_file" "$err_file"

  case "$status" in
    2*) printf '%s' "$body" ;;
    *)
      echo "  ✗ API ${method} ${path} failed" >&2
      echo "  HTTP ${status}${curl_err:+ (${curl_err})}" >&2
      if [ -n "$body" ]; then
        echo "  Response: $body" >&2
      fi
      return 1
      ;;
  esac
}

api_post() {
  local path="$1"; local data="$2"
  api_request "POST" "$path" "$data"
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "$1"
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

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$1" >/dev/null 2>&1 || true
}

remove_fixture_shared_docs_branch() {
  local workspace_id="$1"
  local shared_docs_repo_id="$2"

  [ -z "$shared_docs_repo_id" ] && return 0

  # Workspace creation attaches the org-shared docs branch. This fixture
  # replaces that branch with a dedicated docs repo, so tear down its branch
  # projections before deleting the branch row. Never suppress a failed
  # cleanup here: a retained branch would expose a mixed three-repo workspace
  # to every E2E.
  psql_exec "
    BEGIN;

    DELETE FROM workspace_repos
    WHERE workspace_id = '${workspace_id}'
      AND repo_id = '${shared_docs_repo_id}';

    COMMIT;
  " >/dev/null

  local remaining
  remaining=$(psql_exec "
    SELECT count(*)
    FROM workspace_repos
    WHERE workspace_id = '${workspace_id}'
      AND repo_id = '${shared_docs_repo_id}';
  " | tr -d '[:space:]')
  if [ "$remaining" != "0" ]; then
    echo "  ✗ Failed to remove fixture shared docs branch ${shared_docs_repo_id}" >&2
    exit 1
  fi

  echo "  ✓ Removed fixture shared docs branch (repo=${shared_docs_repo_id})"
}

normalize_repos_root_permissions() {
  docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
mkdir -p /data/repos
chown 10001:10001 /data/repos
chmod 0777 /data/repos
"
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
AUTH_REFRESH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.refreshToken // .data.refresh_token // .refreshToken // .refresh_token // empty' 2>/dev/null)
if [ -z "$AUTH_TOKEN" ]; then
  echo "  ✗ Failed to get auth token"
  echo "  Response: $AUTH_RESP"
  exit 1
fi
echo "  ✓ Got auth token"

api_authed_post() {
  local path="$1"; local data="$2"
  api_request "POST" "$path" "$data" "$AUTH_TOKEN"
}

api_authed_get() {
  local path="$1"
  api_request "GET" "$path" "" "$AUTH_TOKEN"
}

api_authed_put() {
  local path="$1"; local data="$2"
  api_request "PUT" "$path" "$data" "$AUTH_TOKEN"
}

# ─── Step 2: Get org ID ──────────────────────────────────────────────────────

echo ""
echo "=== Step 2: Get org ==="
# Prefer an explicit CI/dev org when provided. Browser CI writes the seeded org
# id before Rust E2E runs; after Rust E2E, worker auto-detection can pick a
# transient worker row from another test org.
#
# Without an explicit org, detect which org the running workers are registered to.
# Workers are scoped to a single org — the test workspace MUST be in the
# same org or workers will never see its tasks (org-level RLS filtering).

if [ -n "$REQUESTED_ORG_ID" ]; then
  ORG_ID="$REQUESTED_ORG_ID"
  echo "  Using explicit org: $ORG_ID"
else
  HOST_ORG_ID=$(psql_exec "SELECT org_id::text FROM worker_hosts WHERE status = 'ready' ORDER BY last_heartbeat DESC NULLS LAST LIMIT 1;" 2>/dev/null | tr -d '[:space:]' || true)
  if [ -n "$HOST_ORG_ID" ]; then
    # Use the org the workspace host serves so dispatch is visible there.
    ORG_ID="$HOST_ORG_ID"
    echo "  Using workspace host org: $ORG_ID"
  else
    # No workspace host is ready — fall back to the first accessible org.
    ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")
    ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)
  fi
fi
if [ -z "$ORG_ID" ]; then
  echo "  ✗ No orgs found"
  exit 1
fi
USER_ID=$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]')
if [ -n "$USER_ID" ]; then
  psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${USER_ID}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
  echo "  ✓ Ensured user membership in selected org"
  # The shared fixture identity is a returning person (ui g5.ui.onboarding-tour):
  # Home opens on Mine and no tour blocks the flows. Fresh-user tour flows
  # create their own identities and keep the migration's never-shown default.
  psql_exec_quiet "UPDATE users SET onboarding_step = 7, onboarding_skipped = false WHERE id = '${USER_ID}';"
  echo "  ✓ Marked the fixture user as onboarded"
fi
echo "  ✓ Org: $ORG_ID"

if [ -n "$AUTH_REFRESH_TOKEN" ]; then
  AUTH_SWITCH_RESP=$(api_post "/api/v1/auth/refresh" "{\"refreshToken\":\"${AUTH_REFRESH_TOKEN}\",\"orgId\":\"${ORG_ID}\"}")
  AUTH_TOKEN=$(echo "$AUTH_SWITCH_RESP" | jq -r '.data.jwt // .jwt // empty' 2>/dev/null)
  AUTH_REFRESH_TOKEN=$(echo "$AUTH_SWITCH_RESP" | jq -r '.data.refreshToken // .data.refresh_token // .refreshToken // .refresh_token // empty' 2>/dev/null)
  if [ -z "$AUTH_TOKEN" ]; then
    echo "  ✗ Failed to switch auth token to selected org"
    echo "  Response: $AUTH_SWITCH_RESP"
    exit 1
  fi
  echo "  ✓ Auth token selected to worker org"
else
  echo "  ⚠ Test token did not include a refresh token; continuing with initial selected org"
fi

# ─── Step 3: Cleanup previous test workspace ─────────────────────────────────

echo ""
echo "=== Step 3: Cleanup previous test workspace ==="

# Find existing target test workspace
OLD_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}' LIMIT 1;" | tr -d '[:space:]')

if [ -n "$OLD_WS_ID" ]; then
  echo "  Found previous workspace: $OLD_WS_ID"

  ACTIVE_VM=$(psql_exec "
    SELECT concat(vm_id, ' (', status, ')')
    FROM workspace_vms
    WHERE workspace_id = '${OLD_WS_ID}'
      AND status IN ('starting', 'running', 'stopping')
    LIMIT 1;
  " | tr -d '[:space:]')
  if [ -n "$ACTIVE_VM" ]; then
    echo "  ✗ Refusing to reset workspace ${OLD_WS_ID} while VM ${ACTIVE_VM} is active" >&2
    echo "  Stop the workspace through the API and wait for it to reach stopped or failed, then rerun the seed." >&2
    exit 1
  fi

  # Retain shared Base repositories. Older fixture versions owned dedicated
  # repositories, so collect only repositories with no other workspace or docs
  # consumer for cleanup after the workspace is removed.
  OLD_REPO_IDS=$(psql_exec "
    SELECT DISTINCT wb.repo_id::text
    FROM workspace_repos wb
    WHERE wb.workspace_id = '${OLD_WS_ID}'
      AND NOT EXISTS (
        SELECT 1 FROM workspace_repos other
        WHERE other.repo_id = wb.repo_id
          AND other.workspace_id <> '${OLD_WS_ID}'
      )
      AND NOT EXISTS (
        SELECT 1 FROM workspaces other
        WHERE other.docs_repo_id = wb.repo_id
          AND other.id <> '${OLD_WS_ID}'
      );
  " 2>/dev/null || echo "")

  # ── Cancel running tasks via API (sends abort signals to workers) ─────
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
    # Brief pause so workers can receive and process the abort signals
    echo "  Waiting 3s for workers to process abort signals..."
    sleep 3
  fi

  # Delete in dependency order (child tables first)
  echo "  Cleaning DB rows..."
  psql_exec_quiet "DELETE FROM session_inputs WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM provider_requests WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM task_dependencies WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM task_attempts WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM timeline_entries WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM tasks WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM chat_sessions WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM docs WHERE workspace_id = '${OLD_WS_ID}';"
  # The lifecycle service owns Git ref deletion. Preserve membership until it
  # has removed this workspace's refs; otherwise the next checked fork sees
  # the old name as taken in the shared Base repositories.
  if [ -z "$(psql_exec "SELECT deleted_at::text FROM workspaces WHERE id = '${OLD_WS_ID}';" | tr -d '[:space:]')" ]; then
    api_authed_post "/api/v1/workspaces/${OLD_WS_ID}/delete" '{"force":true}' >/dev/null
  fi
  # The lifecycle delete itself records a task. Remove that completed task
  # before deleting the fixture workspace row on the next seed.
  psql_exec_quiet "DELETE FROM provider_requests WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM task_attempts WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM timeline_entries WHERE task_id IN (SELECT id FROM tasks WHERE workspace_id = '${OLD_WS_ID}');"
  psql_exec_quiet "DELETE FROM tasks WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_settings WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_forks WHERE source_workspace_id = '${OLD_WS_ID}' OR target_workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_ownership_transfers WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM worker_vm_events WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_vms WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec_quiet "DELETE FROM workspace_flushes WHERE workspace_id = '${OLD_WS_ID}';"
  psql_exec "DELETE FROM workspaces WHERE id = '${OLD_WS_ID}';" >/dev/null

  # Delete the dedicated test repos and their Docker-volume checkouts.
  if [ -n "$OLD_REPO_IDS" ]; then
    for OLD_REPO_ID in $OLD_REPO_IDS; do
      OLD_REPO_ID=$(echo "$OLD_REPO_ID" | tr -d '[:space:]')
      [ -z "$OLD_REPO_ID" ] && continue
      psql_exec_quiet "DELETE FROM repos WHERE id = '${OLD_REPO_ID}';"
      docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 \
        rm -rf "/data/repos/${ORG_ID}/${OLD_REPO_ID}" "/data/repos/${ORG_ID}/.seed-ready-${OLD_REPO_ID}" \
          "/data/repos/${ORG_ID}/.seed-${OLD_REPO_ID}" 2>/dev/null || true
      echo "  ✓ Deleted test repo and volume files $OLD_REPO_ID"
    done
  fi

  echo "  ✓ Previous test workspace cleaned up"
else
  echo "  No previous test workspace found (first run)"
fi

# Always clean up orphaned repos for this target fixture (handles partial
# failures from previous runs where the workspace was deleted but repo rows
# survived). Only the default e2e-test fixture owns the legacy slug cleanup.
if [[ "$WORKSPACE_NAME" == "e2e-test" ]]; then
  ORPHAN_SLUGS=(e2e-test-repo e2e-test-code-repo e2e-test-docs-repo "${SUPPORT_REPO_SLUG}")
else
  ORPHAN_SLUGS=("${CODE_REPO_SLUG}" "${DOCS_REPO_SLUG}" "${SUPPORT_REPO_SLUG}")
fi
for ORPHAN_SLUG in "${ORPHAN_SLUGS[@]}"; do
  ORPHAN_REPO_IDS=$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = '${ORPHAN_SLUG}';" 2>/dev/null || echo "")
  for ORPHAN_REPO_ID in $ORPHAN_REPO_IDS; do
    ORPHAN_REPO_ID=$(echo "$ORPHAN_REPO_ID" | tr -d '[:space:]')
    [ -z "$ORPHAN_REPO_ID" ] && continue
    echo "  Cleaning orphaned repo (${ORPHAN_SLUG}): $ORPHAN_REPO_ID"
    ORPHAN_WS_COUNT=$(psql_exec "
      SELECT count(DISTINCT wb.workspace_id)
      FROM workspace_repos wb
      WHERE wb.repo_id = '${ORPHAN_REPO_ID}';
    " | tr -d '[:space:]')
    ORPHAN_DOCS_REFERENCE_COUNT=$(psql_exec "
      SELECT count(*)
      FROM workspaces
      WHERE docs_repo_id = '${ORPHAN_REPO_ID}';
    " | tr -d '[:space:]')
    if [ "$ORPHAN_WS_COUNT" != "0" ] || [ "$ORPHAN_DOCS_REFERENCE_COUNT" != "0" ]; then
      echo "  ✗ Orphan-slug repo ${ORPHAN_REPO_ID} still has workspace or shared-docs references; refusing destructive cleanup." >&2
      echo "  Use a fresh G5_E2E_TEST_WORKSPACE_NAME and G5_E2E_TEST_REPO_SLUG_PREFIX for an isolated run." >&2
      exit 1
    fi
    psql_exec "DELETE FROM repos WHERE id = '${ORPHAN_REPO_ID}';" >/dev/null
    if [ "$(psql_exec "SELECT EXISTS(SELECT 1 FROM repos WHERE id = '${ORPHAN_REPO_ID}');" | tr -d '[:space:]')" = "t" ]; then
      echo "  ✗ Failed to delete orphaned repo ${ORPHAN_REPO_ID}; dependent rows remain" >&2
      exit 1
    fi
    docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 \
      rm -rf "/data/repos/${ORG_ID}/${ORPHAN_REPO_ID}" "/data/repos/${ORG_ID}/.seed-ready-${ORPHAN_REPO_ID}" \
        "/data/repos/${ORG_ID}/.seed-${ORPHAN_REPO_ID}" 2>/dev/null || true
    echo "  ✓ Orphaned repo cleaned up"
  done
done

# ─── Step 4: Create a checked workspace fork via API ─────────────────────────

echo ""
echo "=== Step 4: Create checked test workspace fork ==="
BASE_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND is_base = true LIMIT 1;" | tr -d '[:space:]')
if [ -z "$BASE_WS_ID" ]; then
  echo "  ✗ Seeded org has no Base workspace"
  exit 1
fi
CLONE_BODY=$(jq -nc \
  --arg name "$WORKSPACE_NAME" \
  --arg key "e2e-seed:${ORG_ID}:${WORKSPACE_NAME}" \
  '{name:$name,idempotencyKey:$key,source:"durable"}')
WS_RESP=$(api_authed_post "/api/v1/workspaces/${BASE_WS_ID}/clone" "$CLONE_BODY")
WS_ID=$(echo "$WS_RESP" | jq -r '.data.workspace.id // empty' 2>/dev/null)
if [ -z "$WS_ID" ]; then
  echo "  ✗ Failed to create checked workspace fork"
  echo "  Response: $WS_RESP"
  exit 1
fi
echo "  ✓ Checked workspace fork: $WS_ID"

# ─── Step 5: Resolve the fork's canonical code/docs members ───────────────────

echo ""
echo "=== Step 5: Resolve checked repository vector ==="
CODE_REPO_ID=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${WS_ID}' AND role = 'code' LIMIT 1;" | tr -d '[:space:]')
DOCS_REPO_ID=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE workspace_id = '${WS_ID}' AND role = 'docs' LIMIT 1;" | tr -d '[:space:]')
if [ -z "$CODE_REPO_ID" ] || [ -z "$DOCS_REPO_ID" ]; then
  echo "  ✗ Checked fork did not produce one code and one docs member"
  exit 1
fi
echo "  Code member: repo=${CODE_REPO_ID}"
echo "  Docs member: repo=${DOCS_REPO_ID}"

# Backwards-compatible aliases used by older e2e helpers/env consumers.
TEST_REPO_ID="$CODE_REPO_ID"

BRANCH_COUNT=$(psql_exec "SELECT count(*) FROM workspace_repos WHERE workspace_id = '${WS_ID}';")
if [ "$BRANCH_COUNT" != "2" ]; then
  echo "  ✗ Checked fork produced ${BRANCH_COUNT} members; expected 2"
  exit 1
fi
echo "  ✓ Checked repository vector has two members"

# ─── Step 6: Provision mini-calc files on volume ─────────────────────────────

echo ""
echo "=== Step 6: Provision mini-calc files ==="

# Keep both mirrors invisible to ref-based projection until all fixture SQL
# identities, links, and grounding files are ready. Publishing the code mirror
# before Step 7 lets convergence create different node IDs ahead of our seed.
BASE="/data/repos/${ORG_ID}/.seed-ready-${CODE_REPO_ID}"
DOCS_BASE="/data/repos/${ORG_ID}/.seed-ready-${DOCS_REPO_ID}"
LOCAL_BASE="/tmp/g5-seed-${CODE_REPO_ID}"
PUBLISH_STAGE="/data/repos/${ORG_ID}/.seed-${CODE_REPO_ID}"

# Git maintenance can remove loose objects while seed containers normalize
# permissions. Ignore only that readdir race; preserve every other permission
# failure so an unusable repos volume still fails the seed loudly.
CHMOD_REPO_TREE_SH='
chmod_repo_tree() {
  target="$1"
  [ -e "$target" ] || return 0
  if find "$target" -ignore_readdir_race -exec chmod a+rwX {} + >/dev/null 2>&1; then
    return 0
  fi

  error_file="/tmp/g5-seed-chmod.$$"
  if ! chmod -R a+rwX "$target" 2>"$error_file"; then
    if grep -v "No such file or directory" "$error_file" >&2; then
      rm -f "$error_file"
      return 1
    fi
  fi
  rm -f "$error_file"
}
'

# The entire mini-calc project is written in a single docker run.
# This is the same content as provision_test_repo_on_volume() in the E2E test
# code, extracted here for the seed script.
provision_mini_calc_repo() {
docker run --rm -v "${REPOS_VOLUME}:/data/repos" \
  -v "$REPO_ROOT/tests/fixtures/browser-ontology:/ontology-fixture:ro" \
  g5/base:26.04 sh -c "
set -e
rm -rf ${LOCAL_BASE} ${PUBLISH_STAGE} ${BASE}
mkdir -p ${LOCAL_BASE}/.semlift/specs/mini-calc/parser
mkdir -p ${LOCAL_BASE}/.semlift/specs/mini-calc/engine
mkdir -p ${LOCAL_BASE}/.g5/ontology
# Current browser discovery reads these files. Retain the legacy fixtures below
# for projection/grounding consumers that still exercise that format.
cp -R /ontology-fixture/. ${LOCAL_BASE}/.g5/ontology/
mkdir -p ${LOCAL_BASE}/src/parser
mkdir -p ${LOCAL_BASE}/src/engine

# A committed, self-contained Preview application used by the workspace-VM
# browser acceptance flow. The server writes one tracked file when it serves a
# request so the same test proves that Preview runs inside the owner checkout
# and that its writes appear on the normal Changes/flush path. Its cleanup
# command writes a second tracked file, so the flow also proves that Close
# runs the profile's cleanup inside the same checkout.
cat > ${LOCAL_BASE}/.g5/workspace.yaml << 'YAML'
preview:
  command: node e2e-preview-server.js
  port: 4173
  description: Workspace Preview E2E fixture
  cleanup_command: printf 'G5_WORKSPACE_PREVIEW_CLEANED\n' > e2e-preview-cleanup.txt
YAML

cat > ${LOCAL_BASE}/e2e-preview-server.js << 'JS'
const fs = require('fs');
const http = require('http');

const marker = 'G5_WORKSPACE_PREVIEW_RENDERED';
http.createServer((request, response) => {
  fs.writeFileSync('e2e-preview-write.txt', marker + '\n', 'utf8');
  if (request.url.split('?')[0] === '/__g5_s11_probe') {
    response.writeHead(200, {
      'content-type': 'text/html; charset=utf-8',
      'content-security-policy': 'sandbox allow-scripts allow-same-origin',
      'set-cookie': '__g5_preview_probe_cookie=escaped; Path=/'
    });
    response.end('<!doctype html><body>G5_PREVIEW_ISOLATION_PROBE<script>document.body.dataset.scriptRan = 1; try { localStorage.setItem(\"__g5_preview_probe\", \"escaped\"); } catch (_) {}</script></body>');
    return;
  }
  response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
  response.end(marker + '\n');
}).listen(Number(process.env.PORT) || 4173, '0.0.0.0');
JS

# A second, named profile whose build is noisy: it prints a few thousand
# lines on both streams over several seconds before serving the same
# application, prints a line on SIGTERM, and runs a slow cleanup. The run-log
# flow (g5.preview.run-log) watches that output live on the hosted terminal
# session, and proves the task journal carries lifecycle events only.
mkdir -p ${LOCAL_BASE}/.g5/preview_profiles
cat > ${LOCAL_BASE}/.g5/preview_profiles/noisy-build.yaml << 'YAML'
command: node e2e-preview-noisy.js
port: 4174
description: Noisy build E2E fixture
cleanup_command: printf 'G5_NOISY_CLEANUP\n'; sleep 4; printf 'G5_WORKSPACE_PREVIEW_CLEANED\n' > e2e-preview-cleanup.txt
YAML

cat > ${LOCAL_BASE}/e2e-preview-noisy.js << 'JS'
const batches = 30;
const perBatch = 100;
let batch = 0;
process.on('SIGTERM', () => {
  process.stdout.write('G5_NOISY_SHUTDOWN\n', () => process.exit(0));
});
const printBatch = () => {
  for (let i = 0; i < perBatch; i++) {
    const n = batch * perBatch + i;
    const line = 'G5_NOISY_LINE ' + String(n).padStart(4, '0') + ' ' + 'x'.repeat(48);
    if (n % 10 === 0) console.error('[stderr] ' + line);
    else console.log(line);
  }
  batch += 1;
  if (batch < batches) {
    setTimeout(printBatch, 500);
    return;
  }
  console.log('G5_NOISY_BUILD_DONE');
  require('./e2e-preview-server.js');
};
printBatch();
JS

# ── semlift.yaml ──
cat > ${LOCAL_BASE}/semlift.yaml << 'YAML'
version: \"1\"
project_name: mini-calc
specs_dir: .semlift/specs
views:
  fan_out_sync: true
  generate_code: true
YAML

# ══════════════════════════════════════════════════════════════════════════
# SPECS — 3-level hierarchy: root → module → leaf
# Aggregation specs (root, parser, engine) describe boundaries & intent.
# Leaf specs describe individual source files with full API docs.
# ══════════════════════════════════════════════════════════════════════════

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc.semlift.md << 'SPEC'
---
component_id: mini-calc
child_spec_ids:
  - mini-calc/lib
  - mini-calc/parser
  - mini-calc/engine
language: rust
complexity: medium
---
# mini-calc — Project Root

A Rust calculator library that parses and evaluates arithmetic expressions
with variable support. This is the top-level aggregation spec; it owns no
source files directly.

[I1. Provide a self-contained embeddable expression evaluator]

Input is a plain-text arithmetic expression (with optional variable
assignments); output is a numeric result or formatted string. The library
should be usable as a dependency in any Rust project with zero runtime
overhead beyond the evaluation itself.

[D1. Module boundary — Parser and Engine separation]

| Child | Responsibility |
|-------|----------------|
| **lib** | Crate entry point — re-exports, shared error/value types |
| **parser** | Front-end: tokenization → AST → static validation |
| **engine** | Back-end: tree-walking evaluation → formatting |

Parser produces an AST; engine consumes it. They share types defined in
\\\`lib.rs\\\` (CalcError, Value, CalcWarning) but otherwise have no
cross-dependency.

[I2. Support variable assignments and arithmetic operations]

Expressions are single-line, evaluated eagerly, left-to-right.
All arithmetic is performed in f64; integer display is a convenience.
No I/O, no async, no allocator customization.

[D2. Error handling — structured errors via CalcError, no panics]

All failure modes must return \\\`Result\\\`; the library must never panic
on valid or invalid input. The \\\`CalcError\\\` enum captures all error
conditions (ParseError, DivisionByZero, Overflow, UndefinedVariable,
UndefinedFunction) as first-class values the caller can inspect or display.

[I3. Support built-in math functions: abs, sqrt, max, min, pow]

A fixed set of single- and two-argument math functions must be available
without requiring any user-defined definitions. These are dispatched by
the evaluator at runtime; no plugin mechanism is required.

[I4. Multiple output format support]

Computed values must be renderable in at least four formats — decimal,
scientific notation, fixed-precision, hexadecimal, and binary — via
the \\\`OutputFormat\\\` enum. Format selection is caller-controlled; the
evaluator is format-agnostic.

[D3. Performance contract — zero runtime dependencies beyond the Rust standard library]

The crate must compile with only \\\`std\\\` (no third-party crates in the
production dependency graph). Evaluation of typical single-line expressions
must complete in sub-millisecond time on modern hardware; no allocation
of heap memory beyond the initial AST construction is permitted.

## Known Gaps

- No user-defined function support (only built-in abs/sqrt/max/min/pow).
- No operator precedence override beyond parentheses.
- No REPL or streaming evaluation mode.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/lib.semlift.md << 'SPEC'
---
component_id: mini-calc/lib
parent_spec_id: mini-calc
source_files:
  - src/lib.rs
language: rust
complexity: low
---
# Crate Root — lib.rs

Entry point for the mini-calc crate. Declares the two top-level modules
(\\\`parser\\\`, \\\`engine\\\`) and defines shared types used across the crate.

[I1. Centralise shared type definitions]

Centralise type definitions that both parser and engine depend on, so
neither module pulls in the other. Every public type and convenience
re-export lives here.

[D1. Public Types]

| Type | Description |
|------|-------------|
| \\\`CalcError\\\` | Enum — ParseError, DivisionByZero, Overflow, UndefinedVariable, UndefinedFunction |
| \\\`Value\\\` | Enum — Integer(i64), Float(f64). Supports \\\`as_f64()\\\`, \\\`is_integer()\\\`, Display |
| \\\`CalcWarning\\\` | Enum — DivisionByLiteralZero, ExcessiveDepth, PossiblyUndefined |

[D2. Re-exports]

- \\\`parser::ast::{Expr, Op, Statement}\\\`
- \\\`engine::evaluator::{evaluate, eval_expr}\\\`
- \\\`engine::context::Context\\\`
- \\\`engine::formatter::{format_value, OutputFormat}\\\`
SPEC

mkdir -p ${LOCAL_BASE}/.semlift/specs/mini-calc/parser
mkdir -p ${LOCAL_BASE}/.semlift/specs/mini-calc/engine

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/parser.semlift.md << 'SPEC'
---
component_id: mini-calc/parser
parent_spec_id: mini-calc
child_spec_ids:
  - mini-calc/parser/mod
  - mini-calc/parser/lexer
  - mini-calc/parser/ast
  - mini-calc/parser/validator
language: rust
complexity: medium
---
# Parser Module — Aggregation

Front-end pipeline: raw input string → tokens → AST → validated AST.

[I1. Transform user text into a validated AST]

Transform user-supplied text into a structured, validated AST that the
engine can evaluate without further parsing or error check.

[D1. Internal module boundary]

| Child | Responsibility |
|-------|----------------|
| **mod** | Module re-exports (\\\`tokenize\\\`, \\\`parse\\\`, \\\`validate\\\`) |
| **lexer** | Splits input into \\\`Token\\\` stream |
| **ast** | Recursive-descent parser producing \\\`Statement\\\` / \\\`Expr\\\` trees |
| **validator** | Static analysis (div-by-zero, depth, undefined refs) |

Data flows strictly lexer → ast → validator. No child should call back
into a sibling (e.g., ast must not call tokenize).

## Assumptions

- Input is a complete, single-line expression (no streaming).
- Whitespace is insignificant (skipped by lexer).
- The validator produces warnings, not hard errors — callers may ignore them.

## Known Gaps

- No position/span tracking on tokens (errors report no column).
- No incremental re-parsing support.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/engine.semlift.md << 'SPEC'
---
component_id: mini-calc/engine
parent_spec_id: mini-calc
child_spec_ids:
  - mini-calc/engine/mod
  - mini-calc/engine/evaluator
  - mini-calc/engine/context
  - mini-calc/engine/formatter
language: rust
complexity: medium
---
# Engine Module — Aggregation

Back-end pipeline: validated AST → evaluation → formatted output.

[I1. Evaluate AST against variable environment]

Evaluate a parsed AST against a variable environment and produce
human-readable output. The engine is stateful (variables persist across
statements within a session) but single-threaded.

[D1. Internal module boundary]

| Child | Responsibility |
|-------|----------------|
| **mod** | Module re-exports |
| **evaluator** | Tree-walking interpreter — produces \\\`Value\\\` results |
| **context** | Scoped variable storage (get/set/push/pop) |
| **formatter** | Renders \\\`Value\\\` and \\\`CalcError\\\` to display strings |

The evaluator reads/writes context; the formatter reads values. No child
should import from the parser module directly (use shared types from lib).

## Assumptions

- Evaluation is eager and left-to-right for binary operators.
- Variable scoping follows simple stack semantics (push/pop scope).
- Built-in functions are hard-coded (no plugin/extension mechanism).

## Known Gaps

- No step-debugging or partial evaluation.
- No memoization or constant folding.
- Built-in function set is closed (abs, sqrt, max, min, pow only).
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/parser/mod.semlift.md << 'SPEC'
---
component_id: mini-calc/parser/mod
parent_spec_id: mini-calc/parser
source_files:
  - src/parser/mod.rs
language: rust
complexity: low
---
# Parser Module Entry — mod.rs

Declares the three parser sub-modules and re-exports their primary
public functions for ergonomic access.

[I1. Provide a single import path for parser sub-modules]

Provide a single import path (\\\`parser::tokenize\\\`, \\\`parser::parse\\\`,
\\\`parser::validate\\\`) so callers don't need to know the internal
sub-module structure.

[D1. Re-exports]

- \\\`pub use lexer::tokenize\\\`
- \\\`pub use ast::parse\\\`
- \\\`pub use validator::validate\\\`
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/parser/lexer.semlift.md << 'SPEC'
---
component_id: mini-calc/parser/lexer
parent_spec_id: mini-calc/parser
source_files:
  - src/parser/lexer.rs
language: rust
complexity: low
---
# Lexer Module

Tokenises an input string into a sequence of \\\`Token\\\` values, skipping
whitespace and rejecting unknown characters.

[I1. Convert raw text into a flat token stream]

Provide the first stage of the parser pipeline: convert raw text into a
flat token stream that the recursive-descent parser can consume without
worrying about character-level details.

[D1. Token type definitions]

\\\`\\\`\\\`rust
pub enum Token {
    Number(f64),
    Ident(String),
    Plus, Minus, Star, Slash,
    LParen, RParen, Equals, Comma,
}
\\\`\\\`\\\`

[D2. Public function signature]

| Signature | Description |
|-----------|-------------|
| \\\`pub fn tokenize(input: &str) -> Result<Vec<Token>, CalcError>\\\` | Scans \\\`input\\\` left-to-right, returns ordered token list or parse error |

## Behaviour Notes

- Consecutive digits (with optional single dot) are collapsed into \\\`Number\\\`.
- Identifiers start with \\\`[a-zA-Z_]\\\` and continue with \\\`[a-zA-Z0-9_]\\\`.
- Unrecognised characters produce \\\`CalcError::ParseError\\\`.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/parser/ast.semlift.md << 'SPEC'
---
component_id: mini-calc/parser/ast
parent_spec_id: mini-calc/parser
source_files:
  - src/parser/ast.rs
language: rust
complexity: medium
---
# AST Module

Defines expression and statement AST nodes and the recursive-descent
parser that converts a token stream into a tree.

[I1. Provide structural representation of parsed expressions]

Provide the structural representation of a parsed expression and the
entry-point parser function. Operator precedence is encoded in the
call hierarchy: additive → multiplicative → unary → primary.

[D1. AST type definitions]

\\\`\\\`\\\`rust
pub enum Op   { Add, Sub, Mul, Div }
pub enum Expr {
    Number(f64),
    Ident(String),
    BinaryOp(Box<Expr>, Op, Box<Expr>),
    FnCall(String, Vec<Expr>),
    Unary(Op, Box<Expr>),
}
pub enum Statement {
    Expression(Expr),
    Assignment(String, Expr),
}
\\\`\\\`\\\`

[D2. Parser entry point]

| Signature | Description |
|-----------|-------------|
| \\\`pub fn parse(tokens: &[Token]) -> Result<Vec<Statement>, CalcError>\\\` | Parses a complete token stream into a list of statements |

## Internal Structure

- \\\`parse_add_sub\\\` — handles \\\`+\\\` / \\\`-\\\` (lowest precedence)
- \\\`parse_mul_div\\\` — handles \\\`*\\\` / \\\`/\\\`
- \\\`parse_unary\\\` — handles unary \\\`-\\\`
- \\\`parse_primary\\\` — handles literals, identifiers, function calls, parens
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/parser/validator.semlift.md << 'SPEC'
---
component_id: mini-calc/parser/validator
parent_spec_id: mini-calc/parser
source_files:
  - src/parser/validator.rs
language: rust
complexity: low
---
# Validator Module

Performs static analysis on parsed AST nodes before evaluation, producing
warnings for suspicious patterns.

[I1. Catch common mistakes at parse time]

Catch common mistakes at parse time so the evaluator doesn't need runtime
guards for detectable issues. Warnings are advisory — evaluation can
proceed even if validation surfaces problems.

[D1. Public validation function]

| Signature | Description |
|-----------|-------------|
| \\\`pub fn validate(stmts: &[Statement]) -> Result<(), Vec<CalcWarning>>\\\` | Walks the AST, returns Ok or a list of warnings |

[D2. Validation checks]

| Check | Warning |
|-------|---------|
| Division by literal zero | \\\`CalcWarning::DivisionByLiteralZero(line)\\\` |
| Expression depth > 64 | \\\`CalcWarning::ExcessiveDepth(depth)\\\` |
| Reference to possibly-undefined variable | \\\`CalcWarning::PossiblyUndefined(name)\\\` |

## Behaviour Notes
- Built-in names (\\\`pi\\\`, \\\`e\\\`, \\\`abs\\\`, \\\`sqrt\\\`, \\\`max\\\`, \\\`min\\\`, \\\`pow\\\`) are
  recognised and do not trigger \\\`PossiblyUndefined\\\`.
- Assignments are tracked left-to-right; forward references warn.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/engine/mod.semlift.md << 'SPEC'
---
component_id: mini-calc/engine/mod
parent_spec_id: mini-calc/engine
source_files:
  - src/engine/mod.rs
language: rust
complexity: low
---
# Engine Module Entry — mod.rs

Declares the three engine sub-modules and re-exports their primary
public functions and types.

[I1. Provide a single import path for engine sub-modules]

Provide a single import path (\\\`engine::evaluate\\\`, \\\`engine::Context\\\`,
\\\`engine::format_value\\\`, etc.) so callers don't need to know the internal
sub-module structure.

[D1. Re-exports]

- \\\`pub use evaluator::{evaluate, eval_expr}\\\`
- \\\`pub use context::Context\\\`
- \\\`pub use formatter::{format_value, format_error, format_table, OutputFormat}\\\`
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/engine/evaluator.semlift.md << 'SPEC'
---
component_id: mini-calc/engine/evaluator
parent_spec_id: mini-calc/engine
source_files:
  - src/engine/evaluator.rs
language: rust
complexity: medium
---
# Evaluator Module

Tree-walking interpreter that evaluates an AST against a \\\`Context\\\`,
producing \\\`Value\\\` results.

[I1. Execute parsed expressions via recursive AST walking]

Execute parsed expressions by recursively walking the AST. Assignments
mutate the context; expressions produce values. Built-in math functions
are dispatched internally.

[D1. Public evaluation API]

| Signature | Description |
|-----------|-------------|
| \\\`pub fn evaluate(stmts: &[Statement], ctx: &mut Context) -> Result<Vec<Value>, CalcError>\\\` | Evaluates a list of statements, returning results for expression statements |
| \\\`pub fn eval_expr(expr: &Expr, ctx: &Context) -> Result<Value, CalcError>\\\` | Evaluates a single expression against an immutable context snapshot |

[D2. Evaluation behaviour rules]

- \\\`evaluate\\\` processes statements sequentially; assignments modify \\\`ctx\\\`
  in place and produce no output value.
- Integer promotion: if \\\`f64\\\` equals its \\\`i64\\\` cast, result is \\\`Value::Integer\\\`.
- Binary ops always compute in f64 (integer result promotion happens per-result).
- Division by zero returns \\\`CalcError::DivisionByZero\\\` at runtime.

[D3. Built-in functions]

\\\`abs(x)\\\`, \\\`sqrt(x)\\\`, \\\`max(a,b)\\\`, \\\`min(a,b)\\\`, \\\`pow(base,exp)\\\` —
any other function name returns \\\`CalcError::UndefinedFunction\\\`.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/engine/context.semlift.md << 'SPEC'
---
component_id: mini-calc/engine/context
parent_spec_id: mini-calc/engine
source_files:
  - src/engine/context.rs
language: rust
complexity: low
---
# Context Module

Scoped variable environment for the evaluator. Variables are stored in
a stack of hash-map scopes supporting nested evaluation contexts.

[I1. Provide mutable variable storage with scoped lookups]

Provide mutable variable storage that the evaluator uses for assignments
and lookups. Scoping allows temporary variable bindings that are
automatically discarded when a scope is popped.

[D1. Context struct and methods]

\\\`\\\`\\\`rust
pub struct Context { /* stack of scopes */ }
\\\`\\\`\\\`

| Signature | Description |
|-----------|-------------|
| \\\`pub fn new() -> Context\\\` | Creates context with built-ins (\\\`pi\\\`, \\\`e\\\`) |
| \\\`pub fn set(&mut self, name: &str, val: Value)\\\` | Binds a variable in the current scope |
| \\\`pub fn get(&self, name: &str) -> Option<&Value>\\\` | Looks up a variable, searching scopes top-down |
| \\\`pub fn has(&self, name: &str) -> bool\\\` | Returns true if the variable is defined |
| \\\`pub fn list(&self) -> Vec<(&str, &Value)>\\\` | Lists all visible variables, sorted by name |
| \\\`pub fn push_scope(&mut self)\\\` | Pushes a new empty scope onto the stack |
| \\\`pub fn pop_scope(&mut self)\\\` | Pops the top scope (panics if only root remains) |

[D2. Scope lookup and default behaviour]

- \\\`get\\\` searches scopes from top (newest) to bottom (root) — inner scopes
  shadow outer ones.
- \\\`new()\\\` pre-seeds \\\`pi\\\` and \\\`e\\\` as \\\`Value::Float\\\` constants.
- Implements \\\`Default\\\` via \\\`new()\\\`.
SPEC

cat > ${LOCAL_BASE}/.semlift/specs/mini-calc/engine/formatter.semlift.md << 'SPEC'
---
component_id: mini-calc/engine/formatter
parent_spec_id: mini-calc/engine
source_files:
  - src/engine/formatter.rs
language: rust
complexity: low
---
# Formatter Module

Formats \\\`Value\\\` results and \\\`CalcError\\\` messages for human-readable display.

[I1. Decouple display logic from evaluation]

Decouple display logic from evaluation so that callers can choose output
formats (decimal, scientific, hex, binary) without modifying the evaluator.

[D1. OutputFormat enum and public functions]

\\\`\\\`\\\`rust
pub enum OutputFormat { Decimal, Scientific, Fixed(u32), Hex, Binary }
\\\`\\\`\\\`

| Signature | Description |
|-----------|-------------|
| \\\`pub fn format_value(val: &Value, fmt: OutputFormat) -> String\\\` | Renders a value in the specified format |
| \\\`pub fn format_error(err: &CalcError) -> String\\\` | Renders an error as a user-friendly string |
| \\\`pub fn format_table(entries: &[(&str, &Value)]) -> String\\\` | Renders a name-value table (for variable listings) |

[D2. Format-specific behaviour]

- \\\`Decimal\\\` format preserves integer display for \\\`Value::Integer\\\`.
- \\\`Hex\\\` and \\\`Binary\\\` cast the f64 to i64 before formatting.
- \\\`format_table\\\` returns the text 'no variables defined' for empty input.
- Column width auto-adjusts to the longest variable name.
SPEC

# ══════════════════════════════════════════════════════════════════════════
# SOURCE FILES
# ══════════════════════════════════════════════════════════════════════════

cat > ${LOCAL_BASE}/src/lib.rs << 'SRC'
//! mini-calc — A Rust calculator library with variable support.
pub mod parser;
pub mod engine;

pub use parser::ast::{Expr, Op, Statement};
pub use engine::evaluator::{evaluate, eval_expr};
pub use engine::context::Context;
pub use engine::formatter::{format_value, OutputFormat};

#[derive(Debug, Clone)]
pub enum CalcError {
    ParseError(String),
    DivisionByZero,
    Overflow,
    UndefinedVariable(String),
    UndefinedFunction(String),
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalcError::ParseError(msg) => write!(f, \"parse error: {}\", msg),
            CalcError::DivisionByZero => write!(f, \"division by zero\"),
            CalcError::Overflow => write!(f, \"numeric overflow\"),
            CalcError::UndefinedVariable(name) => write!(f, \"undefined variable: {}\", name),
            CalcError::UndefinedFunction(name) => write!(f, \"undefined function: {}\", name),
        }
    }
}
impl std::error::Error for CalcError {}

#[derive(Debug, Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match self { Value::Integer(i) => *i as f64, Value::Float(f) => *f }
    }
    pub fn is_integer(&self) -> bool { matches!(self, Value::Integer(_)) }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(i) => write!(f, \"{}\", i),
            Value::Float(v) => write!(f, \"{}\", v),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CalcWarning {
    DivisionByLiteralZero(usize),
    ExcessiveDepth(usize),
    PossiblyUndefined(String),
}
SRC

cat > ${LOCAL_BASE}/src/parser/mod.rs << 'SRC'
pub mod lexer;
pub mod ast;
pub mod validator;
pub use lexer::tokenize;
pub use ast::parse;
pub use validator::validate;
SRC

cat > ${LOCAL_BASE}/src/engine/mod.rs << 'SRC'
pub mod evaluator;
pub mod context;
pub mod formatter;
pub use evaluator::{evaluate, eval_expr};
pub use context::Context;
pub use formatter::{format_value, format_error, format_table, OutputFormat};
SRC

cat > ${LOCAL_BASE}/src/parser/lexer.rs << 'SRC'
use crate::CalcError;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64), Ident(String),
    Plus, Minus, Star, Slash,
    LParen, RParen, Equals, Comma,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => { chars.next(); }
            '0'..='9' | '.' => {
                let mut s = String::new();
                let mut dot = false;
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() { s.push(c); chars.next(); }
                    else if c == '.' && !dot { dot = true; s.push(c); chars.next(); }
                    else { break; }
                }
                tokens.push(Token::Number(s.parse().map_err(|_| CalcError::ParseError(format!(\"bad number: {}\", s)))?));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' { id.push(c); chars.next(); } else { break; }
                }
                tokens.push(Token::Ident(id));
            }
            '+' => { tokens.push(Token::Plus); chars.next(); }
            '-' => { tokens.push(Token::Minus); chars.next(); }
            '*' => { tokens.push(Token::Star); chars.next(); }
            '/' => { tokens.push(Token::Slash); chars.next(); }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            '=' => { tokens.push(Token::Equals); chars.next(); }
            ',' => { tokens.push(Token::Comma); chars.next(); }
            _ => return Err(CalcError::ParseError(format!(\"unexpected: '{}'\", ch))),
        }
    }
    Ok(tokens)
}
SRC

cat > ${LOCAL_BASE}/src/parser/ast.rs << 'SRC'
use crate::CalcError;
use crate::parser::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub enum Op { Add, Sub, Mul, Div }

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64), Ident(String),
    BinaryOp(Box<Expr>, Op, Box<Expr>),
    FnCall(String, Vec<Expr>),
    Unary(Op, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Statement { Expression(Expr), Assignment(String, Expr) }

pub fn parse(tokens: &[Token]) -> Result<Vec<Statement>, CalcError> {
    let mut pos = 0;
    let mut stmts = Vec::new();
    while pos < tokens.len() {
        if pos + 2 <= tokens.len() {
            if let Token::Ident(name) = &tokens[pos] {
                if matches!(tokens.get(pos + 1), Some(Token::Equals)) {
                    let name = name.clone(); pos += 2;
                    stmts.push(Statement::Assignment(name, parse_add_sub(tokens, &mut pos)?));
                    continue;
                }
            }
        }
        stmts.push(Statement::Expression(parse_add_sub(tokens, &mut pos)?));
    }
    Ok(stmts)
}

fn parse_add_sub(tokens: &[Token], pos: &mut usize) -> Result<Expr, CalcError> {
    let mut left = parse_mul_div(tokens, pos)?;
    while *pos < tokens.len() {
        let op = match &tokens[*pos] { Token::Plus => Op::Add, Token::Minus => Op::Sub, _ => break };
        *pos += 1;
        left = Expr::BinaryOp(Box::new(left), op, Box::new(parse_mul_div(tokens, pos)?));
    }
    Ok(left)
}

fn parse_mul_div(tokens: &[Token], pos: &mut usize) -> Result<Expr, CalcError> {
    let mut left = parse_unary(tokens, pos)?;
    while *pos < tokens.len() {
        let op = match &tokens[*pos] { Token::Star => Op::Mul, Token::Slash => Op::Div, _ => break };
        *pos += 1;
        left = Expr::BinaryOp(Box::new(left), op, Box::new(parse_unary(tokens, pos)?));
    }
    Ok(left)
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<Expr, CalcError> {
    if *pos < tokens.len() && matches!(tokens[*pos], Token::Minus) {
        *pos += 1;
        return Ok(Expr::Unary(Op::Sub, Box::new(parse_primary(tokens, pos)?)));
    }
    parse_primary(tokens, pos)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<Expr, CalcError> {
    if *pos >= tokens.len() { return Err(CalcError::ParseError(\"unexpected end\".into())); }
    match &tokens[*pos] {
        Token::Number(n) => { let v = *n; *pos += 1; Ok(Expr::Number(v)) }
        Token::Ident(name) => {
            let name = name.clone(); *pos += 1;
            if *pos < tokens.len() && matches!(tokens[*pos], Token::LParen) {
                *pos += 1;
                let mut args = Vec::new();
                while *pos < tokens.len() && !matches!(tokens[*pos], Token::RParen) {
                    if !args.is_empty() {
                        if !matches!(tokens.get(*pos), Some(Token::Comma)) { return Err(CalcError::ParseError(\"expected ','\".into())); }
                        *pos += 1;
                    }
                    args.push(parse_add_sub(tokens, pos)?);
                }
                if *pos >= tokens.len() { return Err(CalcError::ParseError(\"unclosed '('\".into())); }
                *pos += 1;
                Ok(Expr::FnCall(name, args))
            } else { Ok(Expr::Ident(name)) }
        }
        Token::LParen => {
            *pos += 1;
            let expr = parse_add_sub(tokens, pos)?;
            if *pos >= tokens.len() || !matches!(tokens[*pos], Token::RParen) { return Err(CalcError::ParseError(\"unclosed '('\".into())); }
            *pos += 1;
            Ok(expr)
        }
        other => Err(CalcError::ParseError(format!(\"unexpected token: {:?}\", other))),
    }
}
SRC

cat > ${LOCAL_BASE}/src/parser/validator.rs << 'SRC'
use crate::{CalcWarning, CalcError};
use crate::parser::ast::{Statement, Expr, Op};
use std::collections::HashSet;

pub fn validate(stmts: &[Statement]) -> Result<(), Vec<CalcWarning>> {
    let mut warnings = Vec::new();
    let mut defined: HashSet<String> = HashSet::new();
    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Statement::Assignment(name, expr) => { check(expr, &defined, &mut warnings, i, 0); defined.insert(name.clone()); }
            Statement::Expression(expr) => { check(expr, &defined, &mut warnings, i, 0); }
        }
    }
    if warnings.is_empty() { Ok(()) } else { Err(warnings) }
}

fn check(expr: &Expr, vars: &HashSet<String>, w: &mut Vec<CalcWarning>, line: usize, depth: usize) {
    if depth > 64 { w.push(CalcWarning::ExcessiveDepth(depth)); return; }
    match expr {
        Expr::Number(_) => {}
        Expr::Ident(n) => { if !vars.contains(n) && !matches!(n.as_str(), \"pi\"|\"e\"|\"abs\"|\"sqrt\"|\"max\"|\"min\"|\"pow\") { w.push(CalcWarning::PossiblyUndefined(n.clone())); } }
        Expr::BinaryOp(l, op, r) => {
            if matches!(op, Op::Div) { if let Expr::Number(n) = r.as_ref() { if *n == 0.0 { w.push(CalcWarning::DivisionByLiteralZero(line)); } } }
            check(l, vars, w, line, depth+1); check(r, vars, w, line, depth+1);
        }
        Expr::Unary(_, inner) => { check(inner, vars, w, line, depth+1); }
        Expr::FnCall(_, args) => { for a in args { check(a, vars, w, line, depth+1); } }
    }
}
SRC

cat > ${LOCAL_BASE}/src/engine/evaluator.rs << 'SRC'
use crate::{CalcError, Value};
use crate::parser::ast::{Statement, Expr, Op};
use crate::engine::context::Context;

pub fn evaluate(stmts: &[Statement], ctx: &mut Context) -> Result<Vec<Value>, CalcError> {
    let mut results = Vec::new();
    for stmt in stmts {
        match stmt {
            Statement::Assignment(name, expr) => { let val = eval_expr(expr, ctx)?; ctx.set(name, val); }
            Statement::Expression(expr) => { results.push(eval_expr(expr, ctx)?); }
        }
    }
    Ok(results)
}

pub fn eval_expr(expr: &Expr, ctx: &Context) -> Result<Value, CalcError> {
    match expr {
        Expr::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < i64::MAX as f64 { Ok(Value::Integer(*n as i64)) }
            else { Ok(Value::Float(*n)) }
        }
        Expr::Ident(name) => ctx.get(name).cloned().ok_or_else(|| CalcError::UndefinedVariable(name.clone())),
        Expr::BinaryOp(l, op, r) => {
            let lv = eval_expr(l, ctx)?.as_f64();
            let rv = eval_expr(r, ctx)?.as_f64();
            let res = match op {
                Op::Add => lv + rv, Op::Sub => lv - rv, Op::Mul => lv * rv,
                Op::Div => { if rv == 0.0 { return Err(CalcError::DivisionByZero); } lv / rv }
            };
            Ok(Value::Float(res))
        }
        Expr::Unary(Op::Sub, inner) => {
            match eval_expr(inner, ctx)? {
                Value::Integer(i) => Ok(Value::Integer(-i)),
                Value::Float(f) => Ok(Value::Float(-f)),
            }
        }
        Expr::Unary(_, _) => Err(CalcError::ParseError(\"invalid unary\".into())),
        Expr::FnCall(name, args) => {
            let vals: Result<Vec<Value>, _> = args.iter().map(|a| eval_expr(a, ctx)).collect();
            let vals = vals?;
            match name.as_str() {
                \"abs\" => Ok(Value::Float(vals[0].as_f64().abs())),
                \"sqrt\" => Ok(Value::Float(vals[0].as_f64().sqrt())),
                \"max\" => Ok(Value::Float(vals[0].as_f64().max(vals[1].as_f64()))),
                \"min\" => Ok(Value::Float(vals[0].as_f64().min(vals[1].as_f64()))),
                \"pow\" => Ok(Value::Float(vals[0].as_f64().powf(vals[1].as_f64()))),
                _ => Err(CalcError::UndefinedFunction(name.clone())),
            }
        }
    }
}
SRC

cat > ${LOCAL_BASE}/src/engine/context.rs << 'SRC'
use crate::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Scope { vars: HashMap<String, Value> }
impl Scope { fn new() -> Self { Self { vars: HashMap::new() } } }

#[derive(Debug, Clone)]
pub struct Context { scopes: Vec<Scope> }

impl Context {
    pub fn new() -> Self {
        let mut ctx = Self { scopes: vec![Scope::new()] };
        ctx.set(\"pi\", Value::Float(std::f64::consts::PI));
        ctx.set(\"e\", Value::Float(std::f64::consts::E));
        ctx
    }
    pub fn set(&mut self, name: &str, val: Value) {
        if let Some(scope) = self.scopes.last_mut() { scope.vars.insert(name.to_string(), val); }
    }
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.vars.get(name) { return Some(val); }
        }
        None
    }
    pub fn has(&self, name: &str) -> bool { self.get(name).is_some() }
    pub fn list(&self) -> Vec<(&str, &Value)> {
        let mut seen = HashMap::new();
        for scope in self.scopes.iter().rev() { for (k, v) in &scope.vars { seen.entry(k.as_str()).or_insert(v); } }
        let mut entries: Vec<_> = seen.into_iter().collect();
        entries.sort_by_key(|(k, _)| *k);
        entries
    }
    pub fn push_scope(&mut self) { self.scopes.push(Scope::new()); }
    pub fn pop_scope(&mut self) { assert!(self.scopes.len() > 1); self.scopes.pop(); }
}
impl Default for Context { fn default() -> Self { Self::new() } }
SRC

cat > ${LOCAL_BASE}/src/engine/formatter.rs << 'SRC'
use crate::{CalcError, Value};

#[derive(Debug, Clone)]
pub enum OutputFormat { Decimal, Scientific, Fixed(u32), Hex, Binary }

pub fn format_value(val: &Value, fmt: OutputFormat) -> String {
    match fmt {
        OutputFormat::Decimal => match val {
            Value::Integer(i) => format!(\"{}\", i),
            Value::Float(f) => format!(\"{}\", f),
        },
        OutputFormat::Scientific => format!(\"{:.6e}\", val.as_f64()),
        OutputFormat::Fixed(p) => format!(\"{:.prec\$}\", val.as_f64(), prec = p as usize),
        OutputFormat::Hex => format!(\"0x{:X}\", val.as_f64() as i64),
        OutputFormat::Binary => format!(\"0b{:b}\", val.as_f64() as i64),
    }
}

pub fn format_error(err: &CalcError) -> String {
    match err {
        CalcError::ParseError(msg) => format!(\"Error: parse error — {}\", msg),
        CalcError::DivisionByZero => \"Error: division by zero\".to_string(),
        CalcError::Overflow => \"Error: numeric overflow\".to_string(),
        CalcError::UndefinedVariable(n) => format!(\"Error: undefined variable '{}'\", n),
        CalcError::UndefinedFunction(n) => format!(\"Error: undefined function '{}'\", n),
    }
}

pub fn format_table(entries: &[(&str, &Value)]) -> String {
    if entries.is_empty() { return \"(no variables defined)\".to_string(); }
    let w = entries.iter().map(|(k, _)| k.len()).max().unwrap_or(4);
    let mut lines = vec![
        format!(\"{:<width\$}  | Value\", \"Name\", width = w),
        format!(\"{}--+--------\", \"-\".repeat(w)),
    ];
    for (name, val) in entries {
        lines.push(format!(\"{:<width\$}  | {}\", name, format_value(val, OutputFormat::Decimal), width = w));
    }
    lines.join(\"\n\")
}
SRC

# Physical-path resolution fixtures for grounding navigation. The canonical
# identity below is intentionally present at both its exact and generation/
# projected paths so the API must fail closed instead of guessing.
mkdir -p ${LOCAL_BASE}/generation/src/parser
cat > ${LOCAL_BASE}/src/parser/ambiguous_target.rs << 'SRC'
pub fn ambiguous_target() -> &'static str {
    \"exact\"
}
SRC
cat > ${LOCAL_BASE}/generation/src/parser/ambiguous_target.rs << 'SRC'
pub fn ambiguous_target() -> &'static str {
    \"generation-projected\"
}
SRC

# ── Initialize git repo ──
cd ${LOCAL_BASE}
git config --global --add safe.directory '*'
git init -b main >/dev/null
git config user.email 'test@g5.dev'
git config user.name 'G5 E2E Test'
git config core.sharedRepository 0777
git config core.fileMode false
git config gc.auto 0
git config maintenance.auto false
git add .
git fetch --no-tags file:///data/repos/${ORG_ID}/${CODE_REPO_ID} refs/heads/${WORKSPACE_NAME}
git update-ref refs/heads/main FETCH_HEAD
git commit -m 'Initial mini-calc repo'

# Deterministic Source Control History fixture (issue #1199): keep the main
# first-parent chain longer than one page and include a real merge whose
# first-parent diff adds a known file. Browser coverage clones this repository,
# so it exercises normal, merge, and root commits through the production APIs.
git checkout -b e2e-source-control-history-side >/dev/null
printf 'merge-side fixture\n' > e2e-history-merge.txt
git add e2e-history-merge.txt
git commit -m 'Source control history side commit' >/dev/null
git checkout main >/dev/null
mkdir -p .g5/e2e-history
for i in 1 2 3 4 5 6 7 8 9 10; do
  printf 'history fixture %s\n' \"\$i\" > \".g5/e2e-history/commit-\$i.txt\"
  git add \".g5/e2e-history/commit-\$i.txt\"
  git commit -m \"Source control history fixture \$i\" >/dev/null
done
git merge --no-ff e2e-source-control-history-side -m 'Merge source control history fixture' >/dev/null
git branch -D e2e-source-control-history-side >/dev/null

# Build and permission-normalize away from the registered shared-volume path.
# A complete local object graph is then copied to a hidden shared staging path,
# verified there, and moved to the private seed path. Final publication follows
# the SQL seed below. Retry only the shared-volume copy/verification, not git history
# construction: the local source is immutable after fsck succeeds.
git gc --prune=now
chmod -R 0777 ${LOCAL_BASE}
git fsck --full --strict >/dev/null
publish_ok=0
for attempt in 1 2 3; do
  rm -rf ${PUBLISH_STAGE}
  mkdir -p ${PUBLISH_STAGE}
  if cp -a ${LOCAL_BASE}/. ${PUBLISH_STAGE}/ \
    && git -C ${PUBLISH_STAGE} fsck --full --strict >/dev/null; then
    publish_ok=1
    break
  fi
  echo \"Shared-volume repo verification failed (attempt \$attempt/3); retrying\" >&2
  sleep 1
done
if [ \"\$publish_ok\" -ne 1 ]; then
  echo 'Shared-volume repo verification failed after 3 attempts' >&2
  exit 1
fi
mv ${PUBLISH_STAGE} ${BASE}
git -C ${BASE} fsck --full --strict >/dev/null
echo 'DONE'
"
}
provision_mini_calc_repo

echo "  ✓ Mini-calc project provisioned at ${BASE}"

# ─── Step 6b: Provision support code repo for overlay fixtures ───────────────

echo ""
echo "=== Step 6b: Provision overlay support code repo ==="
SUPPORT_REPO_ID=$(python3 -c "import uuid; print(uuid.uuid4())")
echo "  Support repo UUID: $SUPPORT_REPO_ID"

psql_exec "
INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
VALUES (
  '${SUPPORT_REPO_ID}',
  '${ORG_ID}',
  '${SUPPORT_REPO_NAME}',
  '${SUPPORT_REPO_SLUG}',
  'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/${SUPPORT_REPO_SLUG}',
  'main',
  now()
);
" >/dev/null

SUPPORT_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
BASE=/data/repos/${ORG_ID}/${SUPPORT_REPO_ID}
rm -rf \"\$BASE\"
mkdir -p \"\$BASE/src\"
cat > \"\$BASE/README.md\" << 'SUPPORT_README'
# Overlay Support Code

Small second code repository for workspace-overlay browser fixtures.
SUPPORT_README
cat > \"\$BASE/src/support.rs\" << 'SUPPORT_SRC'
pub fn support_label() -> &'static str {
    \"overlay-support\"
}
SUPPORT_SRC
cd \"\$BASE\"
git config --global --add safe.directory '*'
git init -b main >/dev/null
git config user.email 'test@g5.dev'
git config user.name 'G5 E2E Test'
git config core.sharedRepository 0777
git add .
git commit -m 'Initial overlay support code repo' >/dev/null
${CHMOD_REPO_TREE_SH}
chmod_repo_tree \"\$BASE\"
git rev-parse HEAD
")
SUPPORT_REPO_HEAD=$(printf '%s' "$SUPPORT_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$SUPPORT_REPO_HEAD" ] || [ ${#SUPPORT_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve overlay support repo HEAD"
  exit 1
fi
echo "  ✓ Overlay support repo provisioned at /data/repos/${ORG_ID}/${SUPPORT_REPO_ID} (${SUPPORT_REPO_HEAD})"

# Initialize the docs repo separately. Views and docs-owned intent records
# live here; ontology/code files and target-keyed ontology/code grounding stay
# in the code repo.
docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
rm -rf ${DOCS_BASE}
mkdir -p ${DOCS_BASE}/views
mkdir -p ${DOCS_BASE}/intent_record/views
mkdir -p ${DOCS_BASE}/intent_record/chat
mkdir -p ${DOCS_BASE}/intent_record/external
mkdir -p ${DOCS_BASE}/intent_grounding/views
mkdir -p ${DOCS_BASE}/intent_grounding/chat
mkdir -p ${DOCS_BASE}/intent_grounding/external
touch ${DOCS_BASE}/intent_record/views/.gitkeep
touch ${DOCS_BASE}/intent_record/chat/.gitkeep
touch ${DOCS_BASE}/intent_record/external/.gitkeep
touch ${DOCS_BASE}/intent_grounding/views/.gitkeep
touch ${DOCS_BASE}/intent_grounding/chat/.gitkeep
touch ${DOCS_BASE}/intent_grounding/external/.gitkeep
cd ${DOCS_BASE}
git config --global --add safe.directory '*'
git init -b main
git config user.email 'test@g5.dev'
git config user.name 'G5 E2E Test'
git config core.sharedRepository 0777
git config core.fileMode false
git add .
git fetch --no-tags file:///data/repos/${ORG_ID}/${DOCS_REPO_ID} refs/heads/${WORKSPACE_NAME}
git update-ref refs/heads/main FETCH_HEAD
git commit -m 'Initial e2e docs repo'
${CHMOD_REPO_TREE_SH}
chmod_repo_tree ${DOCS_BASE}
echo 'DOCS_DONE'
"

echo "  ✓ Docs repo provisioned at ${DOCS_BASE}"

# ─── Step 7b: Pre-seed docs ──────────────────────────────────────────────────
# Insert two PM view docs (existing + future) so downlift/uplift tests can
# skip any LLM-powered generation for the seeded view. Also provide one committed
# Project Doc so real-browser editor coverage never relies on another test to
# create mutable shared fixture state. Tests need matching docs rows and files
# on the dedicated docs volume.

echo ""
echo "=== Step 7b: Pre-seed docs ==="

# Generate deterministic UUIDs for the seeded docs. The default e2e-test
# fixture keeps historical IDs; alternate workspaces scope IDs by workspace.
VIEW_EXISTING_ID=$(python3 -c "import uuid; scope='${ID_SCOPE}'; name='preseeded-existing-view'; print(uuid.uuid5(uuid.UUID('12345678-abcd-abcd-abcd-123456789abc'), f'{scope}:{name}' if scope else name))")
VIEW_FUTURE_ID=$(python3 -c "import uuid; scope='${ID_SCOPE}'; name='preseeded-future-view'; print(uuid.uuid5(uuid.UUID('12345678-abcd-abcd-abcd-123456789abc'), f'{scope}:{name}' if scope else name))")
PROJECT_DOC_ID=$(python3 -c "import uuid; scope='${ID_SCOPE}'; name='seeded-editor-contract'; print(uuid.uuid5(uuid.UUID('12345678-abcd-abcd-abcd-123456789abc'), f'{scope}:{name}' if scope else name))")
PROJECT_DOC_FILE_PATH="docs/${WS_ID}/seeded-editor-contract.md"

echo "  Existing view: $VIEW_EXISTING_ID"
echo "  Future view:   $VIEW_FUTURE_ID"
echo "  Project Doc:   $PROJECT_DOC_ID"

# ── View file content: existing PM view ──
EXISTING_VIEW_CONTENT='---
view_id: preseeded-existing-view
persona: pm
@SEEDED_SPEC_SCOPE@
new_nodes: []
linked_docs: []
status: Approved
owner: Core Library Squad
version: 1.0.0
---

# mini-calc — Product Management View

## Target Audience

This view is for product managers and engineering leads who need to understand
the mini-calc library architecture, module responsibilities, and feature surface.
Assumes familiarity with calculator concepts but not Rust internals.

## Executive Summary

mini-calc is a Rust calculator library that parses and evaluates arithmetic
expressions with variable support. It provides a clean modular pipeline:
input string → lexer → AST → validation → evaluation → formatted output.

## Implementation Mapping

[I1. The mini-calc project shall provide a safe expression evaluator]
The calculator library must parse and evaluate arithmetic expressions safely,
supporting variable bindings, built-in functions, and multiple output formats.
The modular pipeline (lexer → AST → validator → evaluator → formatter) ensures
each stage can be tested and evolved independently.

[D1. All arithmetic operations must handle overflow and division by zero]
Every arithmetic operation must detect overflow conditions and division by zero
at evaluation time, returning structured errors (CalcError) rather than
panicking or producing undefined results. The validator also performs static
analysis to warn about division by literal zero before evaluation.

### mini-calc/lib
[I2. Public library API]
Crate root and public API surface. Declares the parser and engine modules,
defines shared error types (CalcError, CalcWarning), and re-exports Value.

### mini-calc/parser/lexer
[I3. Lexer tokenization]
Tokenizes raw input strings into a token stream. Handles arithmetic operators,
parentheses, identifiers, numeric literals, and assignment syntax.

### mini-calc/parser/ast
[I4. AST definition and parsing]
Defines the expression tree (Expr, Statement) and the recursive-descent parser.
Converts token streams into structured ASTs with correct operator precedence.

### mini-calc/parser/validator
[I5. Static analysis validation]
Static analysis pass over the parsed AST. Detects division by literal zero,
excessive nesting depth, and warns about possibly-undefined variables.

### mini-calc/engine/evaluator
[I6. Expression evaluation]
Core evaluation logic that walks the AST and computes numeric results.
Handles runtime division-by-zero, integer-to-float promotion, and built-in functions.

### mini-calc/engine/context
[I7. Variable environment]
Variable environment with scoped lookups. Stores user-defined variables and
built-in constants (pi, e). Supports push/pop scope for nested evaluation.

### mini-calc/engine/formatter
[I8. Output formatting]
Formats computed Values for display. Supports decimal, scientific notation,
fixed-precision, hexadecimal, and binary output modes via OutputFormat enum.'

# ── View file content: future PM view ──
FUTURE_VIEW_CONTENT='---
view_id: preseeded-future-view
persona: pm
@SEEDED_SPEC_SCOPE@
new_nodes:
  - mini-calc/parser/operators
  - mini-calc/engine/comparisons
  - mini-calc/engine/constants
linked_docs: []
status: Draft
owner: Core Library Squad
version: 0.1.0
---

# mini-calc — Future Features View

## Target Audience

This view is for product managers planning the next iteration of mini-calc.
It describes proposed new modules and changes to existing modules needed
to support extended operators, comparison expressions, and constant declarations.

## Executive Summary

The calculator currently supports basic arithmetic with variables and
built-in functions. This future view proposes three enhancements:
extended operators (%, **), comparison expressions, and read-only
constant declarations. These require three new spec modules and
modifications to several existing modules.

## Implementation Mapping

[I1. Extended operator and comparison support]
The calculator must support modulo (%), power (**), and comparison operators
(==, !=, <, >, <=, >=) with correct precedence and associativity rules.
Power is right-associative and binds tighter than multiplication;
comparisons bind looser than addition.

[D1. Read-only constant declarations with immutability enforcement]
Users can declare constants via "const x = 42" syntax. Once defined,
a constant name cannot be reassigned. The ConstantStore wraps Context
with immutability enforcement and pre-loaded math constants.

### mini-calc/lib
[I2. Updated public API]
Crate root needs updated re-exports to expose new operator types,
comparison result values, and the constant declaration API.

### mini-calc/parser/lexer
[I3. Extended lexer tokens]
Must add new token types for modulo (%), power (**), and comparison
operators (==, !=, <, >, <=, >=) to the existing Token enum.

### mini-calc/parser/ast
[I4. Extended AST nodes]
Needs extended Expr variants for comparison and power expressions.
Precedence rules must be updated: ** is right-associative and binds
tighter than * and /; comparisons bind looser than + and -.

### mini-calc/parser/validator
[I5. Enhanced validation rules]
Should add a new warning for chained comparisons (a < b < c) which
are syntactically valid but semantically misleading.

### mini-calc/engine/evaluator
[I6. Extended evaluator operations]
Must implement evaluation for modulo (Rust % for ints, f64::rem for
floats) and power operator (delegating to existing pow built-in).

### mini-calc/engine/context
[I7. Constant support in context]
Needs a new `define_constant()` method that stores values with a
read-only flag, and `set()` must reject writes to constant names.

### mini-calc/engine/formatter
[I8. Formatter compatibility]
No changes required. Comparison results display as integers (0 or 1)
and the existing decimal format handles them correctly.

### mini-calc/parser/operators (NEW)
[D2. Operator precedence module]
New module defining an extended operator precedence table and
associativity rules for the new %, **, and comparison operators.

### mini-calc/engine/comparisons (NEW)
[D3. Comparison evaluation module]
New module implementing comparison evaluation logic. Returns
Value::Integer(1) for true and Value::Integer(0) for false.
Handles mixed int/float comparisons with auto-promotion.

### mini-calc/engine/constants (NEW)
[D4. Constant declarations module]
New module implementing read-only constant declarations via
"const x = 42" syntax. Provides ConstantStore that wraps Context
with immutability enforcement and pre-loaded math constants.'

# One catalog owns both the committed frontmatter and SQL relationship seed.
VIEW_SPEC_SCOPE=$(python3 "$SCRIPT_DIR/seed-view-scope.py")
EXISTING_VIEW_CONTENT="${EXISTING_VIEW_CONTENT//@SEEDED_SPEC_SCOPE@/$VIEW_SPEC_SCOPE}"
FUTURE_VIEW_CONTENT="${FUTURE_VIEW_CONTENT//@SEEDED_SPEC_SCOPE@/$VIEW_SPEC_SCOPE}"

# ── Write view and Project Doc files to Docker volume ──
docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
cd ${DOCS_BASE}
git config --global --add safe.directory '*'
git config core.fileMode false

mkdir -p views docs/${WS_ID}
cat > views/preseeded-existing-view.md << 'VIEWEOF'
${EXISTING_VIEW_CONTENT}
VIEWEOF

cat > views/preseeded-future-view.md << 'VIEWEOF'
${FUTURE_VIEW_CONTENT}
VIEWEOF

cat > ${PROJECT_DOC_FILE_PATH} << 'PROJECTDOCEOF'
---
doc_type: design
title: Seeded Editor Contract
---

# Seeded Editor Contract

This committed Project Doc exists solely to exercise the real Project Doc
editor surface in browser E2E without depending on another test's mutable
workspace setup.
PROJECTDOCEOF

git add views/ docs/
git commit -m 'Add pre-seeded docs and views for E2E testing'
${CHMOD_REPO_TREE_SH}
chmod_repo_tree ${DOCS_BASE}
echo 'DOC_FILES_DONE'
"
echo "  ✓ View and Project Doc files written to volume"

CODE_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
git -C ${BASE} rev-parse HEAD
")
CODE_REPO_HEAD=$(printf '%s' "$CODE_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$CODE_REPO_HEAD" ] || [ ${#CODE_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve seeded code repo HEAD"
  exit 1
fi

DOCS_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
git -C ${DOCS_BASE} rev-parse HEAD
")
DOCS_REPO_HEAD=$(printf '%s' "$DOCS_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$DOCS_REPO_HEAD" ] || [ ${#DOCS_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve seeded docs repo HEAD"
  exit 1
fi
echo "  ✓ Workspace branch heads set: code=${CODE_REPO_HEAD}, docs=${DOCS_REPO_HEAD}"

# ── Insert view metadata; references remain in the committed frontmatter. ──
VIEW_SQL=$(python3 -c "

ws_id = '${WS_ID}'
org_id = '${ORG_ID}'
existing_view_id = '${VIEW_EXISTING_ID}'
future_view_id = '${VIEW_FUTURE_ID}'
project_doc_id = '${PROJECT_DOC_ID}'
project_doc_file_path = '${PROJECT_DOC_FILE_PATH}'
docs_seed_sha = '${DOCS_REPO_HEAD}'
print('BEGIN;')
# The fixture supplies its org identity explicitly.
print('ALTER TABLE docs DISABLE TRIGGER trg_docs_set_org;')

# Existing PM view doc (with pre-seeded SHA for git integration tests)
print(f\"\"\"INSERT INTO docs (id, workspace_id, org_id, name, doc_type, file_path,
  repo_id, last_downlifted_commit_sha,
  content_updated_at, created_at, updated_at)
VALUES (
  '{existing_view_id}', '{ws_id}', '{org_id}', 'preseeded-existing-view', 'pm_view',
  'views/preseeded-existing-view.md',
  '${DOCS_REPO_ID}',
  '{docs_seed_sha}',
  now(), now(), now()
);\"\"\")

# Future PM view doc (with pre-seeded SHA for git integration tests)
print(f\"\"\"INSERT INTO docs (id, workspace_id, org_id, name, doc_type, file_path,
  repo_id, last_downlifted_commit_sha,
  content_updated_at, created_at, updated_at)
VALUES (
  '{future_view_id}', '{ws_id}', '{org_id}', 'preseeded-future-view', 'pm_view',
  'views/preseeded-future-view.md',
  '${DOCS_REPO_ID}',
  '{docs_seed_sha}',
  now(), now(), now()
);\"\"\")

# Committed Project Doc used by the real editor E2E contract.
print(f\"\"\"INSERT INTO docs (id, workspace_id, org_id, name, doc_type, file_path,
  repo_id, last_downlifted_commit_sha,
  content_updated_at, created_at, updated_at)
VALUES (
  '{project_doc_id}', '{ws_id}', '{org_id}', 'Seeded Editor Contract', 'design',
  '{project_doc_file_path}',
  '${DOCS_REPO_ID}',
  '{docs_seed_sha}',
  now(), now(), now()
);\"\"\")

# Re-enable triggers
print('ALTER TABLE docs ENABLE TRIGGER trg_docs_set_org;')
print('COMMIT;')
")

echo "$VIEW_SQL" | docker exec -i "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 >/dev/null

# Set docs_repo_id on the workspace so the API reads view file content from
# the dedicated docs repo. The docs member's branch is the workspace branch
# (the clone route names it after the workspace); the committed-docs
# projection joins docs_branch_name to that member branch and stalled every
# pass ("no rows returned") while this said 'main'.
psql_exec "UPDATE workspaces SET docs_repo_id = '${DOCS_REPO_ID}' WHERE id = '${WS_ID}';" >/dev/null

# Defence: remove any stale workspace_repos that link the org's shared
# auto-provisioned docs repo to this workspace. The fixture must expose exactly
# the dedicated docs repo and dedicated code repo.
SHARED_DOCS_REPO_ID=$(psql_exec "SELECT id::text FROM repos WHERE org_id = '${ORG_ID}' AND slug = 'docs' AND id NOT IN ('${CODE_REPO_ID}', '${DOCS_REPO_ID}');" | tr -d '[:space:]')
remove_fixture_shared_docs_branch "$WS_ID" "$SHARED_DOCS_REPO_ID"

VIEW_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND doc_type = 'pm_view';")
PROJECT_DOC_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND id = '${PROJECT_DOC_ID}' AND doc_type = 'design' AND file_path = '${PROJECT_DOC_FILE_PATH}';")
if [ "$PROJECT_DOC_COUNT" -ne 1 ]; then
  echo "  ✗ Project Doc seed verification failed" >&2
  exit 1
fi
echo "  ✓ View docs seeded: $VIEW_COUNT docs; Project Docs seeded: $PROJECT_DOC_COUNT"

# ─── Step 7d: Write grounding JSONL files to git ─────────────────────────────
# The API reads intent records and grounding edges from JSONL files committed
# in the git repo (target-keyed storage), not from the DB.  We generate the
# same data that was inserted into Postgres above as JSONL files so both the
# legacy DB path and the new JSONL path return consistent results.

echo ""
echo "=== Step 7d: Write grounding JSONL files to git ==="

docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
cd ${BASE}
git config --global --add safe.directory '*'
git config core.fileMode false

# ── Intent record JSONL files ──
# One file per (file_type_subfolder, encoded_file_name).
# Encoding: replace / with __

mkdir -p intent_record/ontology
mkdir -p intent_record/code

# mini-calc root spec: I1, D1, I2, D2, I3, I4, D3.
# These root blocks are marked is_human=true so the read-only Intent Log
# projection has real human rows (the rest of the mini-calc records stay
# is_human=false to keep the machine-generated-exclusion example). I1/D1 are
# grounded (view→root and root→parser/engine), so they surface as traceable
# rows with real parent/child lineage — this is what the intent-log Playwright
# E2E (intent-log.spec.ts) exercises against the CI-seeded compliance workspace.
cat > intent_record/ontology/.semlift__specs__mini-calc.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":true,\"content\":\"Provide a self-contained embeddable expression evaluator\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":true,\"content\":\"Module boundary — Parser and Engine separation\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"I2\",\"is_human\":true,\"content\":\"Support variable assignments and arithmetic operations\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"D2\",\"is_human\":true,\"content\":\"Error handling — structured errors via CalcError, no panics\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"I3\",\"is_human\":true,\"content\":\"Support built-in math functions: abs, sqrt, max, min, pow\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"I4\",\"is_human\":true,\"content\":\"Multiple output format support\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc.semlift.md\",\"intent_record_number\":\"D3\",\"is_human\":true,\"content\":\"Performance contract — zero runtime dependencies beyond std\"}
JSONL

# lib spec: I1, D1, D2
cat > intent_record/ontology/.semlift__specs__mini-calc__lib.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/lib.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":false,\"content\":\"Public library API exposing evaluate function\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/lib.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":false,\"content\":\"evaluate() takes string input and returns Result<f64, CalcError>\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/lib.semlift.md\",\"intent_record_number\":\"D2\",\"is_human\":false,\"content\":\"Re-export parser and engine modules from lib root\"}
JSONL

# parser spec: I1, D1
cat > intent_record/ontology/.semlift__specs__mini-calc__parser.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":false,\"content\":\"Parse arithmetic expressions into AST\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":false,\"content\":\"Recursive descent parser with operator precedence\"}
JSONL

# engine spec: I1, D1
cat > intent_record/ontology/.semlift__specs__mini-calc__engine.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/engine.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":false,\"content\":\"Evaluate AST nodes to produce numeric results\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/engine.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":false,\"content\":\"Tree-walking evaluator with f64 arithmetic\"}
JSONL

# lexer spec: I1, D1, D2
cat > intent_record/ontology/.semlift__specs__mini-calc__parser__lexer.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":false,\"content\":\"Tokenize input string into a sequence of tokens\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":false,\"content\":\"Token types: Number, Plus, Minus, Star, Slash, LParen, RParen\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"intent_record_number\":\"D2\",\"is_human\":false,\"content\":\"Skip whitespace and reject invalid characters with LexError\"}
JSONL

# ast spec: I1, D1, D2
cat > intent_record/ontology/.semlift__specs__mini-calc__parser__ast.semlift.md.jsonl << 'JSONL'
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"intent_record_number\":\"I1\",\"is_human\":false,\"content\":\"Define abstract syntax tree node types for expressions\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"intent_record_number\":\"D1\",\"is_human\":false,\"content\":\"AstNode enum with Number, BinaryOp, and UnaryOp variants\"}
{\"file_type\":\"ontology\",\"file_name\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"intent_record_number\":\"D2\",\"is_human\":false,\"content\":\"Operator enum: Add, Sub, Mul, Div\"}
JSONL

# code records
cat > intent_record/code/src__parser__lexer.rs.jsonl << 'JSONL'
{\"file_type\":\"code\",\"file_name\":\"src/parser/lexer.rs\",\"code_block_name\":\"tokenize\",\"is_human\":false,\"content\":\"pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError>\"}
{\"file_type\":\"code\",\"file_name\":\"src/parser/lexer.rs\",\"code_block_name\":\"Token\",\"is_human\":false,\"content\":\"pub enum Token { Number(f64), Plus, Minus, Star, Slash, LParen, RParen }\"}
JSONL

cat > intent_record/code/src__parser__ast.rs.jsonl << 'JSONL'
{\"file_type\":\"code\",\"file_name\":\"src/parser/ast.rs\",\"code_block_name\":\"AstNode\",\"is_human\":false,\"content\":\"pub enum AstNode { Number(f64), BinaryOp { op: Operator, left: Box<AstNode>, right: Box<AstNode> } }\"}
{\"file_type\":\"code\",\"file_name\":\"src/parser/ast.rs\",\"code_block_name\":\"Operator\",\"is_human\":false,\"content\":\"pub enum Operator { Add, Sub, Mul, Div }\"}
JSONL

cat > intent_record/code/src__engine__evaluator.rs.jsonl << 'JSONL'
{\"file_type\":\"code\",\"file_name\":\"src/engine/evaluator.rs\",\"code_block_name\":\"evaluate_node\",\"is_human\":false,\"content\":\"pub fn evaluate_node(node: &AstNode) -> Result<f64, EvalError>\"}
JSONL

# Code records whose canonical identities cannot resolve to exactly one
# committed physical path. One is absent; the other has exact + generation/
# matches. Both must remain traceable while navigation fails closed.
cat > intent_record/code/src__parser__missing_target.rs.jsonl << 'JSONL'
{\"file_type\":\"code\",\"file_name\":\"src/parser/missing_target.rs\",\"code_block_name\":\"missing_target\",\"is_human\":false,\"content\":\"pub fn missing_target()\"}
JSONL

cat > intent_record/code/src__parser__ambiguous_target.rs.jsonl << 'JSONL'
{\"file_type\":\"code\",\"file_name\":\"src/parser/ambiguous_target.rs\",\"code_block_name\":\"ambiguous_target\",\"is_human\":false,\"content\":\"pub fn ambiguous_target() -> &'static str\"}
JSONL

# ── Grounding edge JSONL files (target-keyed) ──
mkdir -p intent_grounding/ontology
mkdir -p intent_grounding/code

# Target: mini-calc root spec (from views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc.semlift.md\",\"source_id\":\"I1\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc.semlift.md\",\"source_id\":\"D1\",\"target_id\":\"D1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc.semlift.md\",\"source_id\":\"I1\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc.semlift.md\",\"source_id\":\"D1\",\"target_id\":\"D1\",\"relationship\":\"decomposes\"}
JSONL

# Target: parser spec (from root + views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc__parser.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"ontology\",\"source_file\":\".semlift/specs/mini-calc.semlift.md\",\"target_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"source_id\":\"I1\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"ontology\",\"target_file_type\":\"ontology\",\"source_file\":\".semlift/specs/mini-calc.semlift.md\",\"target_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"source_id\":\"D1\",\"target_id\":\"D1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"source_id\":\"I5\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"source_id\":\"I5\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"chat\",\"target_file_type\":\"ontology\",\"source_file\":\"e2e-grounding-chat-session\",\"target_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"source_id\":\"e2e-chat-001\",\"target_id\":\"I1\",\"relationship\":\"requests\",\"relevance\":97}
JSONL

# Target: engine spec (from root + views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc__engine.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"ontology\",\"source_file\":\".semlift/specs/mini-calc.semlift.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I1\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I6\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I7\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I8\",\"target_id\":\"D1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I6\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I7\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"source_id\":\"I8\",\"target_id\":\"D1\",\"relationship\":\"decomposes\"}
JSONL

# Target: lib spec (from views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc__lib.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/lib.semlift.md\",\"source_id\":\"I2\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/lib.semlift.md\",\"source_id\":\"I2\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
JSONL

# Target: lexer spec (from parser + views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc__parser__lexer.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"ontology\",\"source_file\":\".semlift/specs/mini-calc/parser.semlift.md\",\"target_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"source_id\":\"I1\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"source_id\":\"I3\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"source_id\":\"I3\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
JSONL

# Target: ast spec (from views)
cat > intent_grounding/ontology/.semlift__specs__mini-calc__parser__ast.semlift.md.jsonl << 'JSONL'
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-existing-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"source_id\":\"I4\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
{\"source_file_type\":\"view\",\"target_file_type\":\"ontology\",\"source_file\":\"views/preseeded-future-view.md\",\"target_file\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"source_id\":\"I4\",\"target_id\":\"I1\",\"relationship\":\"decomposes\"}
JSONL

# Target: code files (from leaf ontology specs)
cat > intent_grounding/code/src__parser__lexer.rs.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"target_file\":\"src/parser/lexer.rs\",\"source_id\":\"D1\",\"target_id\":\"Token\",\"relationship\":\"implements\"}
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"target_file\":\"src/parser/lexer.rs\",\"source_id\":\"I1\",\"target_id\":\"tokenize\",\"relationship\":\"implements\"}
JSONL

cat > intent_grounding/code/src__parser__ast.rs.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"target_file\":\"src/parser/ast.rs\",\"source_id\":\"D1\",\"target_id\":\"AstNode\",\"relationship\":\"implements\"}
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/ast.semlift.md\",\"target_file\":\"src/parser/ast.rs\",\"source_id\":\"D2\",\"target_id\":\"Operator\",\"relationship\":\"implements\"}
JSONL

cat > intent_grounding/code/src__engine__evaluator.rs.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/engine.semlift.md\",\"target_file\":\"src/engine/evaluator.rs\",\"source_id\":\"D1\",\"target_id\":\"evaluate_node\",\"relationship\":\"implements\"}
JSONL

cat > intent_grounding/code/src__parser__missing_target.rs.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"target_file\":\"src/parser/missing_target.rs\",\"source_id\":\"I1\",\"target_id\":\"missing_target\",\"relationship\":\"implements\"}
JSONL

cat > intent_grounding/code/src__parser__ambiguous_target.rs.jsonl << 'JSONL'
{\"source_file_type\":\"ontology\",\"target_file_type\":\"code\",\"source_file\":\".semlift/specs/mini-calc/parser/lexer.semlift.md\",\"target_file\":\"src/parser/ambiguous_target.rs\",\"source_id\":\"I1\",\"target_id\":\"ambiguous_target\",\"relationship\":\"implements\"}
JSONL

git add intent_record/ intent_grounding/
git commit --amend --no-edit
${CHMOD_REPO_TREE_SH}
chmod_repo_tree ${BASE}
echo 'JSONL_FILES_DONE'
"

docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
cd ${DOCS_BASE}
git config --global --add safe.directory '*'
git config core.fileMode false

mkdir -p intent_record/views
mkdir -p intent_record/chat
mkdir -p intent_record/external
mkdir -p intent_grounding/views
mkdir -p intent_grounding/chat
mkdir -p intent_grounding/external
touch intent_record/chat/.gitkeep
touch intent_record/external/.gitkeep
touch intent_grounding/views/.gitkeep
touch intent_grounding/chat/.gitkeep
touch intent_grounding/external/.gitkeep

cat > intent_record/chat/e2e-grounding-chat-session.jsonl << 'JSONL'
{\"file_type\":\"chat\",\"intent_record_number\":\"e2e-chat-001\",\"chat_session_id\":\"e2e-grounding-chat-session\",\"chat_context\":\"Release review context\\nSecond line preserved\",\"content\":\"Please keep Unicode intact: café 東京 ✅\",\"is_human\":true,\"priority\":2,\"certainty\":94,\"created_by\":\"e2e-reviewer@g5.dev\",\"last_updated_by\":\"e2e-reviewer@g5.dev\",\"created_at\":\"2026-08-03T12:34:56Z\",\"last_updated_at\":\"2026-08-03T12:34:56Z\"}
JSONL

# view: preseeded-existing-view.md (I1, D1, I2..I8)
cat > intent_record/views/views__preseeded-existing-view.md.jsonl << 'JSONL'
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I1\",\"is_human\":true,\"content\":\"The mini-calc project shall provide a safe expression evaluator\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"D1\",\"is_human\":true,\"content\":\"All arithmetic operations must handle overflow and division by zero\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I2\",\"is_human\":true,\"content\":\"Public library API\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I3\",\"is_human\":true,\"content\":\"Lexer tokenization\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I4\",\"is_human\":true,\"content\":\"AST definition and parsing\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I5\",\"is_human\":true,\"content\":\"Static analysis validation\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I6\",\"is_human\":true,\"content\":\"Expression evaluation\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I7\",\"is_human\":true,\"content\":\"Variable environment\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-existing-view.md\",\"intent_record_number\":\"I8\",\"is_human\":true,\"content\":\"Output formatting\",\"priority\":90,\"certainty\":95}
JSONL

# view: preseeded-future-view.md (I1, D1..D4, I2..I8)
cat > intent_record/views/views__preseeded-future-view.md.jsonl << 'JSONL'
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I1\",\"is_human\":true,\"content\":\"Extended operator and comparison support\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"D1\",\"is_human\":true,\"content\":\"Read-only constant declarations with immutability enforcement\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I2\",\"is_human\":true,\"content\":\"Updated public API\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I3\",\"is_human\":true,\"content\":\"Extended lexer tokens\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I4\",\"is_human\":true,\"content\":\"Extended AST nodes\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I5\",\"is_human\":true,\"content\":\"Enhanced validation rules\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I6\",\"is_human\":true,\"content\":\"Extended evaluator operations\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I7\",\"is_human\":true,\"content\":\"Constant support in context\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"I8\",\"is_human\":true,\"content\":\"Formatter compatibility\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"D2\",\"is_human\":true,\"content\":\"Operator precedence module\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"D3\",\"is_human\":true,\"content\":\"Comparison evaluation module\",\"priority\":90,\"certainty\":95}
{\"file_type\":\"view\",\"file_name\":\"views/preseeded-future-view.md\",\"intent_record_number\":\"D4\",\"is_human\":true,\"content\":\"Constant declarations module\",\"priority\":90,\"certainty\":95}
JSONL

git add intent_record/ intent_grounding/
git commit -m 'Add docs intent JSONL files for E2E testing'
${CHMOD_REPO_TREE_SH}
chmod_repo_tree ${DOCS_BASE}
echo 'DOCS_JSONL_FILES_DONE'
"

# Update head_commit to include the new JSONL commits in both repos.
JSONL_CODE_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
git -C ${BASE} rev-parse HEAD
")
JSONL_CODE_REPO_HEAD=$(printf '%s' "$JSONL_CODE_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$JSONL_CODE_REPO_HEAD" ] || [ ${#JSONL_CODE_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve JSONL code repo HEAD"
  exit 1
fi
JSONL_DOCS_REPO_HEAD=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 sh -c "
set -e
git -C ${DOCS_BASE} rev-parse HEAD
")
JSONL_DOCS_REPO_HEAD=$(printf '%s' "$JSONL_DOCS_REPO_HEAD" | tr -d '[:space:]')
if [ -z "$JSONL_DOCS_REPO_HEAD" ] || [ ${#JSONL_DOCS_REPO_HEAD} -ne 40 ]; then
  echo "  ✗ Failed to resolve JSONL docs repo HEAD"
  exit 1
fi
psql_exec_quiet "UPDATE docs SET last_downlifted_commit_sha = '${JSONL_DOCS_REPO_HEAD}' WHERE workspace_id = '${WS_ID}';"
echo "  ✓ JSONL grounding files committed, heads updated: code=${JSONL_CODE_REPO_HEAD}, docs=${JSONL_DOCS_REPO_HEAD}"

# Publish the complete Git files and document metadata together. Advance only
# the exact refs created by the checked fork.
docker run --rm -i -v "${REPOS_VOLUME}:/data/repos" g5/base:26.04 \
  sh -s -- "/data/repos/${ORG_ID}" "$WORKSPACE_NAME" "$CODE_REPO_ID" "$DOCS_REPO_ID" \
  < "$SCRIPT_DIR/publish-seed-workspace-refs.sh"
echo "  ✓ Complete code/docs fixture refs published"

# The workspace remains stopped after a fork. Browser and API fixtures that
# exercise live Save, terminal, preview, or agent paths require an explicit
# owner start and must observe the real host ready event before continuing.
api_authed_post "/api/v1/workspaces/${WS_ID}/resume" '{}' >/dev/null
for attempt in $(seq 1 90); do
  VM_RESP=$(api_authed_get "/api/v1/workspaces/${WS_ID}/vm")
  VM_STATUS=$(echo "$VM_RESP" | jq -r '.data.status // empty')
  if [ "$VM_STATUS" = "running" ]; then
    echo "  ✓ Workspace VM reached running after explicit owner Resume"
    break
  fi
  if [ "$VM_STATUS" = "failed" ]; then
    echo "  ✗ Workspace VM failed during explicit owner Resume"
    echo "  Response: $VM_RESP"
    exit 1
  fi
  if [ "$attempt" -eq 90 ]; then
    echo "  ✗ Timed out waiting for workspace VM readiness (last status: ${VM_STATUS:-unknown})"
    exit 1
  fi
  sleep 2
done

# ─── Step 8: Sanity-check seeded branch shape ────────────────────────────────

echo ""
echo "=== Step 8: Verify seeded workspace shape ==="
DURABLE_BRANCH_COUNT=$(psql_exec "SELECT count(*) FROM workspace_repos WHERE workspace_id = '${WS_ID}';" | tr -d '[:space:]')
FUTURE_VIEW_FILE_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND id = '${VIEW_FUTURE_ID}' AND file_path = 'views/preseeded-future-view.md';" | tr -d '[:space:]')
FUTURE_VIEW_BRANCH_COUNT=$(psql_exec "SELECT count(*) FROM docs WHERE workspace_id = '${WS_ID}' AND id = '${VIEW_FUTURE_ID}' AND repo_id = '${DOCS_REPO_ID}';" | tr -d '[:space:]')

if [ "$DURABLE_BRANCH_COUNT" != "2" ] || [ "$FUTURE_VIEW_FILE_COUNT" != "1" ] || [ "$FUTURE_VIEW_BRANCH_COUNT" != "1" ]; then
  echo "  ✗ Seeded workspace shape check failed"
  echo "    durable branches: ${DURABLE_BRANCH_COUNT} (expected 2)"
  echo "    future view docs: ${FUTURE_VIEW_FILE_COUNT} (expected 1)"
  echo "    future view docs branch: ${FUTURE_VIEW_BRANCH_COUNT} (expected 1)"
  exit 1
fi
echo "  ✓ Repository bindings and view metadata are consistent"

# Prove the same API contract the browser uses, after the real owner VM is ready.
# SQL projection rows alone must never make a current-ontology seed look healthy.
ONTOLOGY_RESPONSE=$(api_authed_get "/api/v1/workspaces/${WS_ID}/repos/${CODE_REPO_ID}/files?path=.g5/ontology&depth=-1")
printf '%s' "$ONTOLOGY_RESPONSE" | jq -e '.data | (.truncated | not) and (.liveFallback == null)' >/dev/null
for ontology_path in root.g5.md parser.g5.md parser/lexer.g5.md engine.g5.md; do
  full_path=".g5/ontology/${ontology_path}"
  printf '%s' "$ONTOLOGY_RESPONSE" | jq -e --arg path "$full_path" '[.data.entries | .. | objects | select(.type? == "file" and .path? == $path)] | length == 1' >/dev/null
  content=$(api_authed_get "/api/v1/workspaces/${WS_ID}/files/${full_path}?repoId=${CODE_REPO_ID}")
  printf '%s' "$content" | jq -e --arg path "$full_path" --rawfile expected "$REPO_ROOT/tests/fixtures/browser-ontology/$ontology_path" '.data | .path == $path and .content == $expected' >/dev/null
done
echo "  ✓ Current ontology files discovered with exact persisted content"

# ─── Step 9: Write fixture env file ───────────────────────────────────────────

echo ""
echo "=== Step 9: Write fixture env file ==="
normalize_repos_root_permissions
echo "  ✓ Restored shared repos root permissions"
ENV_FILE="${G5_E2E_TEST_ENV_FILE:-$REPO_ROOT/.env.test}"
cat > "$ENV_FILE" <<EOF
# Auto-generated by seed-test-workspace.sh — do not edit manually.
# Source this file before running E2E tests:
#   source .env.test && cargo test -p g5-e2e --features live-llm ...
G5_E2E_TEST_WORKSPACE_ID=${WS_ID}
G5_E2E_TEST_REPO_ID=${TEST_REPO_ID}
G5_E2E_OVERLAY_SUPPORT_REPO_ID=${SUPPORT_REPO_ID}
G5_E2E_TEST_CODE_REPO_ID=${CODE_REPO_ID}
G5_E2E_TEST_DOCS_REPO_ID=${DOCS_REPO_ID}
G5_E2E_TEST_ORG_ID=${ORG_ID}
G5D_ORG_ID=${ORG_ID}
G5_E2E_TEST_VIEW_EXISTING_ID=${VIEW_EXISTING_ID}
G5_E2E_TEST_VIEW_FUTURE_ID=${VIEW_FUTURE_ID}
G5_E2E_TEST_PROJECT_DOC_ID=${PROJECT_DOC_ID}
G5_E2E_TEST_SEED_SHA=${JSONL_CODE_REPO_HEAD}
G5_E2E_TEST_DOCS_SEED_SHA=${JSONL_DOCS_REPO_HEAD}
G5D_LLM_PROVIDER=${G5D_LLM_PROVIDER:-openrouter}
G5D_LLM_MODEL=${G5D_LLM_MODEL:-openai/gpt-5.4}
EOF
echo "  ✓ Written to $ENV_FILE"

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo "============================================="
echo "  Test workspace ready!"
echo "============================================="
echo ""
echo "  Workspace ID:  $WS_ID"
echo "  Repo ID:       $TEST_REPO_ID"
echo "  Support repo:  $SUPPORT_REPO_ID"
echo "  Code repo ID:  $CODE_REPO_ID"
echo "  Docs repo ID:  $DOCS_REPO_ID"
echo "  Org ID:        $ORG_ID"
echo "  Existing view: $VIEW_EXISTING_ID"
echo "  Future view:   $VIEW_FUTURE_ID"
echo "  Project Doc:   $PROJECT_DOC_ID"
echo "  Code files:    /data/repos/${ORG_ID}/${CODE_REPO_ID}/"
echo "  Docs files:    /data/repos/${ORG_ID}/${DOCS_REPO_ID}/"
echo ""
echo "  Ontology: 4 current Markdown files; legacy specs remain Git fixtures only"
echo "  Source:   9 Rust source files, 12 spec files"
echo "  Views:    2 pre-seeded PM views (existing + future)"
echo "  Project Docs: 1 committed editor-contract fixture"
echo "  Overlay:  docs repo + mini-calc code repo + support code repo available"
echo ""
echo "  Run tests:"
echo "    source .env.test"
echo "    cargo test -p g5-e2e --features live-llm -- test_live_llm_view_creation_pm --nocapture"
echo ""
echo "  Reset before next run:"
echo "    just test-seed"
