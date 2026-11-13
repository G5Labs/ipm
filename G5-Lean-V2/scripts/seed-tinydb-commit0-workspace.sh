#!/usr/bin/env bash
# Create a TinyDB commit0 workspace with separate code and docs repositories.
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
FIXTURE_ROOT="${2:-$REPO_ROOT/tests/fixtures/tinydb-commit0}"
CODE_FIXTURE="$FIXTURE_ROOT/code"
DOCS_FIXTURE="$FIXTURE_ROOT/docs"
REPOS_VOLUME="${REPOS_VOLUME:-g5-lean_api_repos}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
DB_NAME="${DB_NAME:-g5_dev}"
AUTH_EMAIL="${G5_E2E_TEST_EMAIL:-e2e-test@g5.dev}"

WORKSPACE_NAME="e2e-tinydb-commit0"
CODE_REPO_SLUG="e2e-tinydb-code"
DOCS_REPO_SLUG="e2e-tinydb-docs"
ENV_FILE="$REPO_ROOT/.env.tinydb.commit0"

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

psql_exec() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "$1"
}

psql_exec_quiet() {
  docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$1" >/dev/null 2>&1 || true
}


require_commit0_fixture() {
  [ -d "$CODE_FIXTURE/tests" ] || fail "missing acceptance tests: $CODE_FIXTURE/tests"
  [ -f "$CODE_FIXTURE/semlift.yaml" ] || fail "missing semlift.yaml: $CODE_FIXTURE/semlift.yaml"
  [ -f "$DOCS_FIXTURE/designs/tinydb.md" ] || fail "missing docs design: $DOCS_FIXTURE/designs/tinydb.md"

  local spec_count test_count leaked_python
  spec_count=$(find "$CODE_FIXTURE/.semlift/specs" -type f -name '*.semlift.md' | wc -l | tr -d '[:space:]')
  [ "$spec_count" = "1" ] || fail "commit0 fixture must contain exactly one root spec, found $spec_count"

  test_count=$(find "$CODE_FIXTURE/tests" -type f -name 'test_*.py' | wc -l | tr -d '[:space:]')
  [ "$test_count" -gt 0 ] || fail "commit0 fixture contains no acceptance tests"

  [ ! -e "$CODE_FIXTURE/tinydb" ] || fail "commit0 fixture leaked a tinydb implementation directory"
  leaked_python=$(find "$CODE_FIXTURE" -path "$CODE_FIXTURE/tests" -prune -o -type f -name '*.py' -print -quit)
  [ -z "$leaked_python" ] || fail "commit0 fixture leaked Python implementation code: $leaked_python"

  grep -q '^component_id: tinydb$' \
    "$CODE_FIXTURE/.semlift/specs/tinydb.semlift.md" \
    || fail "root spec frontmatter must use the repo-local component_id tinydb"


}

require_commit0_fixture

API_PORT="${G5_PORT_API:-7500}"
if [ "$API_URL" = "auto" ] || ! curl -sf "${API_URL}/healthz" >/dev/null 2>&1; then
  if curl -sf "http://localhost:${API_PORT}/healthz" >/dev/null 2>&1; then
    API_URL="http://localhost:${API_PORT}"
  else
    API_IP=$(docker inspect "$API_CONTAINER" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || true)
    if [ -n "$API_IP" ] && curl -sf "http://${API_IP}:7500/healthz" >/dev/null 2>&1; then
      API_URL="http://${API_IP}:7500"
    else
      fail "cannot reach g5-api on localhost:${API_PORT} or through ${API_CONTAINER}"
    fi
  fi
fi

echo "=== G5 TinyDB Commit0 Seed ==="
echo "API:       $API_URL"
echo "Fixture:   $FIXTURE_ROOT"
echo "Workspace: $WORKSPACE_NAME"

api_post() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" -H 'Content-Type: application/json' -d "$data"
}

echo "Authenticating..."
AUTH_RESP=$(api_post "/api/v1/auth/test-token" "{\"email\":\"${AUTH_EMAIL}\"}" 2>/dev/null || true)
AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty' 2>/dev/null)
[ -n "$AUTH_TOKEN" ] || fail "failed to obtain test auth token: $AUTH_RESP"

api_authed_post() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" \
    -H 'Content-Type: application/json' \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

api_authed_get() {
  local path="$1"
  curl -sf "${API_URL}${path}" -H "Authorization: Bearer ${AUTH_TOKEN}"
}

api_authed_put() {
  local path="$1"
  local data="$2"
  curl -sf -X PUT "${API_URL}${path}" \
    -H 'Content-Type: application/json' \
    -H "Authorization: Bearer ${AUTH_TOKEN}" \
    -d "$data"
}

resolve_commit0_project_doc_id() {
  local workspace_id="$1"
  local docs_repo_id="$2"
  local project_doc_path="$3"

  local encoded_path
  encoded_path=$(jq -rn --arg path "$project_doc_path" '$path | @uri')
  api_authed_get "/api/v1/workspaces/${workspace_id}/docs/by-path?path=${encoded_path}" \
    | jq -er --arg repo "$docs_repo_id" --arg path "$project_doc_path" \
      '.data | select(.repoId == $repo and .filePath == $path) | .id | select(type == "string" and length > 0)'
}

HOST_ORG_ID=$(psql_exec "SELECT org_id::text FROM worker_hosts WHERE status = 'ready' ORDER BY last_heartbeat DESC NULLS LAST LIMIT 1;" 2>/dev/null | tr -d '[:space:]' || true)
if [ -n "$HOST_ORG_ID" ]; then
  ORG_ID="$HOST_ORG_ID"
else
  ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || true)
  ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // .data[0].id // empty' 2>/dev/null)
fi
[ -n "$ORG_ID" ] || fail "no organization available for commit0 seed"

USER_ID=$(psql_exec "SELECT id::text FROM users WHERE email = '${AUTH_EMAIL}';" 2>/dev/null | tr -d '[:space:]' || true)
if [ -n "$USER_ID" ]; then
  psql_exec_quiet "INSERT INTO org_memberships (org_id, user_id, role) VALUES ('${ORG_ID}', '${USER_ID}', 'admin') ON CONFLICT (org_id, user_id) DO NOTHING;"
fi
echo "Org:       $ORG_ID"

OLD_REPO_IDS=$(psql_exec "
  SELECT branch.repo_id::text
  FROM workspace_repos branch
  JOIN repos repo ON repo.id = branch.repo_id
  WHERE branch.workspace_id IN (
    SELECT id FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}'
  )
    AND repo.slug IN ('${CODE_REPO_SLUG}', '${DOCS_REPO_SLUG}')
  UNION
  SELECT id::text
  FROM repos
  WHERE org_id = '${ORG_ID}'
    AND slug IN ('${CODE_REPO_SLUG}', '${DOCS_REPO_SLUG}');
" 2>/dev/null || true)

OLD_WS_ID=$(psql_exec "SELECT id::text FROM workspaces WHERE org_id = '${ORG_ID}' AND name = '${WORKSPACE_NAME}' LIMIT 1;" | tr -d '[:space:]' || true)
if [ -n "$OLD_WS_ID" ]; then
  echo "Cleaning previous commit0 workspace $OLD_WS_ID..."
  RUNNING_TASKS=$(psql_exec "SELECT id::text FROM tasks WHERE workspace_id = '${OLD_WS_ID}' AND status IN ('queued','running','waiting');" 2>/dev/null || true)
  for TASK_ID in $RUNNING_TASKS; do
    curl -sf -X POST "${API_URL}/api/v1/tasks/${TASK_ID}/cancel" \
      -H "Authorization: Bearer ${AUTH_TOKEN}" \
      -H 'Content-Type: application/json' >/dev/null 2>&1 || true
  done
  [ -z "$RUNNING_TASKS" ] || sleep 2

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
fi

psql_exec_quiet "DELETE FROM repos WHERE org_id = '${ORG_ID}' AND slug IN ('${CODE_REPO_SLUG}', '${DOCS_REPO_SLUG}');"
for OLD_REPO_ID in $OLD_REPO_IDS; do
  docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine \
    rm -rf "/data/repos/${ORG_ID}/${OLD_REPO_ID}" >/dev/null 2>&1 || true
done

WS_ID=$(seed_create_fixture_workspace "$WORKSPACE_NAME")
[ -n "$WS_ID" ] || fail "failed to create workspace: $WS_ID"

CODE_REPO_ID=$(python3 -c 'import uuid; print(uuid.uuid4())')
DOCS_REPO_ID=$(python3 -c 'import uuid; print(uuid.uuid4())')

SQL=$(cat <<EOSQL
BEGIN;

INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, created_at)
VALUES
  ('${CODE_REPO_ID}', '${ORG_ID}', 'TinyDB code', '${CODE_REPO_SLUG}', 'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/${CODE_REPO_SLUG}', 'main', now()),
  ('${DOCS_REPO_ID}', '${ORG_ID}', 'TinyDB docs', '${DOCS_REPO_SLUG}', 'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/${DOCS_REPO_SLUG}', 'main', now());

INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
VALUES ('${ORG_ID}', '${WS_ID}', '${CODE_REPO_ID}', 'code', '${CODE_REPO_SLUG}'),
       ('${ORG_ID}', '${WS_ID}', '${DOCS_REPO_ID}', 'docs', '${DOCS_REPO_SLUG}');



UPDATE workspaces
SET docs_repo_id = '${DOCS_REPO_ID}'
WHERE id = '${WS_ID}';

COMMIT;
EOSQL
)
docker exec "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 -c "$SQL" >/dev/null


CODE_BASE="/data/repos/${ORG_ID}/${CODE_REPO_ID}"
DOCS_BASE="/data/repos/${ORG_ID}/${DOCS_REPO_ID}"
PROJECT_DOC_PATH="docs/${WS_ID}/tinydb.md"
docker run --rm \
  -v "${REPOS_VOLUME}:/data/repos" \
  -v "${CODE_FIXTURE}:/seed/code:ro" \
  -v "${DOCS_FIXTURE}:/seed/docs:ro" \
  alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
git config --global --add safe.directory '*'
git config --global user.email 'e2e@g5.dev'
git config --global user.name 'G5 E2E'

rm -rf '${CODE_BASE}' '${DOCS_BASE}'
mkdir -p '${CODE_BASE}' '${DOCS_BASE}/docs/${WS_ID}'
cp -R /seed/code/. '${CODE_BASE}/'
cp /seed/docs/designs/tinydb.md '${DOCS_BASE}/${PROJECT_DOC_PATH}'

git -C '${CODE_BASE}' init -b main >/dev/null
git -C '${CODE_BASE}' add .
git -C '${CODE_BASE}' commit -m 'Seed TinyDB commit0 spec and acceptance tests' >/dev/null

git -C '${DOCS_BASE}' init -b main >/dev/null
git -C '${DOCS_BASE}' add .
git -C '${DOCS_BASE}' commit -m 'Seed TinyDB commit0 design' >/dev/null

chmod -R 777 '${CODE_BASE}' '${DOCS_BASE}'
"

read -r CODE_HEAD DOCS_HEAD <<EOF
$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
printf '%s %s\n' \"\$(git -C '${CODE_BASE}' rev-parse HEAD)\" \"\$(git -C '${DOCS_BASE}' rev-parse HEAD)\"
")
EOF

[ "${#CODE_HEAD}" -eq 40 ] || fail "invalid code repo HEAD: $CODE_HEAD"
[ "${#DOCS_HEAD}" -eq 40 ] || fail "invalid docs repo HEAD: $DOCS_HEAD"

seed_capture_fixture_parent "$WS_ID" "$WORKSPACE_NAME"

LEAKED_TREE=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
git -C '${CODE_BASE}' ls-tree -r --name-only HEAD | grep '^tinydb/' || true
")
[ -z "$LEAKED_TREE" ] || fail "committed code repo leaked TinyDB implementation paths: $LEAKED_TREE"

ROOT_SPEC_COUNT=$(docker run --rm -v "${REPOS_VOLUME}:/data/repos" alpine sh -c "
set -e
apk add --no-cache git >/dev/null 2>&1
git -C '${CODE_BASE}' ls-tree -r --name-only HEAD \
  | grep -E '^\.semlift/specs/[^/]+\.semlift\.md$' \
  | wc -l
" | tr -d '[:space:]')
[ "$ROOT_SPEC_COUNT" = "1" ] || fail "committed code repo has $ROOT_SPEC_COUNT root specs instead of one"

MEMBERS=$(api_authed_get "/api/v1/workspaces/${WS_ID}/branches")
[ "$(echo "$MEMBERS" | jq '[.data[] | select(.materializationStatus == "materialized")] | length')" = "2" ] || fail "expected two materialized members"

DOC_ID=$(resolve_commit0_project_doc_id "$WS_ID" "$DOCS_REPO_ID" "$PROJECT_DOC_PATH")
[ -n "$DOC_ID" ] || fail "expected a readable TinyDB Project Doc at ${PROJECT_DOC_PATH}"

cat > "$ENV_FILE" <<EOF
# Auto-generated by seed-tinydb-commit0-workspace.sh.
G5_TINYDB_COMMIT0_WORKSPACE_ID=${WS_ID}
G5_TINYDB_COMMIT0_CODE_REPO_ID=${CODE_REPO_ID}
G5_TINYDB_COMMIT0_DOCS_REPO_ID=${DOCS_REPO_ID}
G5_TINYDB_COMMIT0_ORG_ID=${ORG_ID}
G5_TINYDB_COMMIT0_WORKSPACE_NAME=${WORKSPACE_NAME}
G5_TINYDB_COMMIT0_CODE_HEAD=${CODE_HEAD}
G5_TINYDB_COMMIT0_DOCS_HEAD=${DOCS_HEAD}
G5_TINYDB_COMMIT0_DOC_ID=${DOC_ID}
G5_TINYDB_COMMIT0_DOC_PATH=${PROJECT_DOC_PATH}
G5_E2E_API_URL=${API_URL}
G5D_ORG_ID=${ORG_ID}
G5D_LLM_PROVIDER=${G5D_LLM_PROVIDER:-}
G5D_LLM_MODEL=${G5D_LLM_MODEL:-}
EOF

echo ""
echo "TinyDB commit0 workspace ready"
echo "  workspace: $WS_ID"
echo "  code repo: $CODE_REPO_ID @ $CODE_HEAD"
echo "  docs repo: $DOCS_REPO_ID @ $DOCS_HEAD"
echo "  project doc: $DOC_ID @ $PROJECT_DOC_PATH"
echo "  env file:  $ENV_FILE"
echo "  code seed: one root spec + acceptance tests; no implementation"
