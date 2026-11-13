#!/usr/bin/env bash
# =============================================================================
# seed-timsystem-workspace.sh - Seed TimSystem into the running local G5 stack
# =============================================================================
# Thin TimSystem wrapper around seed-compliance-workspace.sh. The shared seed
# path stages these fixtures as local Git repos, installs them as the
# workspace's managed repos, publishes them natively (fast-forward git push via
# scripts/lib/native-publish.sh), then syncs committed docs/reference/guides/spec
# metadata over the API. It expects a local, gitignored fixture at:
#
#   ../timsystem-workspace/timsystem_doc
#   ../timsystem-workspace/timsystem_code
#
# It also accepts the prepared local g5d workspace shape where the docs repo is
# mounted at ../timsystem-workspace/docs, matching .g5/workspace.yaml.
#
# The fixture is intentionally not committed. Refresh it from the local
# TimSystem source with, for example:
#
#   mkdir -p ../timsystem-workspace
#   rsync -a --delete --exclude .git --exclude node_modules --exclude .venv \
#     --exclude .pytest_cache --exclude .ruff_cache --exclude .mypy_cache \
#     /Users/ziniuw/Desktop/projects/TimSystem/timsystem_doc/ \
#     ../timsystem-workspace/timsystem_doc/
#   rsync -a --delete --exclude .git --exclude node_modules --exclude .venv \
#     --exclude .pytest_cache --exclude .ruff_cache --exclude .mypy_cache \
#     /Users/ziniuw/Desktop/projects/TimSystem/timsystem_code/ \
#     ../timsystem-workspace/timsystem_code/
#
# Usage:
#   ./scripts/seed-timsystem-workspace.sh [docs-repo] [code-repo] [api-url]
#
# Defaults:
#   docs-repo = ../timsystem-workspace/timsystem_doc
#   code-repo = ../timsystem-workspace/timsystem_code
#   api-url   = http://localhost:${G5_PORT_API:-7500}
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$GEN_ROOT"

TIMSYSTEM_ROOT="${G5_TIMSYSTEM_SOURCE_ROOT:-$WORKSPACE_ROOT/timsystem-workspace}"
DEFAULT_DOCS_SOURCE_REPO="$TIMSYSTEM_ROOT/timsystem_doc"
DEFAULT_CODE_SOURCE_REPO="$TIMSYSTEM_ROOT/timsystem_code"
DOCS_SOURCE_REPO="${1:-${G5_TIMSYSTEM_DOCS_SOURCE_REPO:-$DEFAULT_DOCS_SOURCE_REPO}}"
CODE_SOURCE_REPO="${2:-${G5_TIMSYSTEM_CODE_SOURCE_REPO:-$DEFAULT_CODE_SOURCE_REPO}}"
API_URL="${3:-http://localhost:${G5_PORT_API:-7500}}"
PG_CONTAINER="${PG_CONTAINER:-g5-dev-postgres}"
DB_NAME="${DB_NAME:-g5_dev}"
TIMSYSTEM_RUNTIME_ARTIFACT_PATH="${G5_TIMSYSTEM_RUNTIME_ARTIFACT_PATH:-tool-results/toolu_example.txt}"

if [ -z "${1:-}" ] && [ -z "${G5_TIMSYSTEM_DOCS_SOURCE_REPO:-}" ] && \
   [ ! -d "$DOCS_SOURCE_REPO" ] && [ -d "$TIMSYSTEM_ROOT/docs" ]; then
  DOCS_SOURCE_REPO="$TIMSYSTEM_ROOT/docs"
  echo "Using prepared TimSystem docs fixture: $DOCS_SOURCE_REPO"
fi

if [ ! -d "$DOCS_SOURCE_REPO" ] || [ ! -d "$CODE_SOURCE_REPO" ]; then
  cat >&2 <<EOF
ERROR: TimSystem fixture not found.

Expected:
  docs: $DOCS_SOURCE_REPO
  code: $CODE_SOURCE_REPO

Default local docs paths accepted:
  $DEFAULT_DOCS_SOURCE_REPO
  $TIMSYSTEM_ROOT/docs

Create the ignored local fixture first:
  mkdir -p "$WORKSPACE_ROOT/timsystem-workspace"
  rsync -a --delete --exclude .git --exclude node_modules --exclude .venv \\
    --exclude .pytest_cache --exclude .ruff_cache --exclude .mypy_cache \\
    /Users/ziniuw/Desktop/projects/TimSystem/timsystem_doc/ \\
    "$WORKSPACE_ROOT/timsystem-workspace/timsystem_doc/"
  rsync -a --delete --exclude .git --exclude node_modules --exclude .venv \\
    --exclude .pytest_cache --exclude .ruff_cache --exclude .mypy_cache \\
    /Users/ziniuw/Desktop/projects/TimSystem/timsystem_code/ \\
    "$WORKSPACE_ROOT/timsystem-workspace/timsystem_code/"
EOF
  exit 2
fi

ENV_FILE="${G5_TIMSYSTEM_ENV_FILE:-$GEN_ROOT/.env.timsystem-workspace}"

export G5_COMPLIANCE_SEED_LABEL="${G5_TIMSYSTEM_SEED_LABEL:-G5 TimSystem Workspace Seed}"
export G5_COMPLIANCE_SEED_EMAIL="${G5_TIMSYSTEM_SEED_EMAIL:-e2e-test@g5.dev}"
export G5_COMPLIANCE_WORKSPACE_NAME="${G5_TIMSYSTEM_WORKSPACE_NAME:-timsystem}"
export G5_COMPLIANCE_WORKSPACE_DESCRIPTION="${G5_TIMSYSTEM_WORKSPACE_DESCRIPTION:-Imported TimSystem workspace for local regression tests}"
export G5_COMPLIANCE_DOCS_REPO_NAME="${G5_TIMSYSTEM_DOCS_REPO_NAME:-timsystem-doc}"
export G5_COMPLIANCE_DOCS_REPO_SLUG="${G5_TIMSYSTEM_DOCS_REPO_SLUG:-timsystem-doc}"
export G5_COMPLIANCE_CODE_REPO_NAME="${G5_TIMSYSTEM_CODE_REPO_NAME:-timsystem-code}"
export G5_COMPLIANCE_CODE_REPO_SLUG="${G5_TIMSYSTEM_CODE_REPO_SLUG:-timsystem-code}"
export G5_COMPLIANCE_LEGACY_REPO_SLUG="${G5_TIMSYSTEM_LEGACY_REPO_SLUG:-timsystem-repo}"
export G5_COMPLIANCE_BRANCH_NAME="${G5_TIMSYSTEM_BRANCH_NAME:-main}"
export G5_COMPLIANCE_ENV_FILE="$ENV_FILE"
export G5_COMPLIANCE_ALLOW_CHAT_TARGET_GROUNDING="${G5_TIMSYSTEM_ALLOW_CHAT_TARGET_GROUNDING:-true}"
export G5_COMPLIANCE_COPY_IMAGE="${G5_TIMSYSTEM_COPY_IMAGE:-${G5_COMPLIANCE_COPY_IMAGE:-g5/g5d-base:latest}}"
export G5_COMPLIANCE_EXPECTED_ROOT_COMPONENT="${G5_TIMSYSTEM_EXPECTED_ROOT_COMPONENT:-tim_system}"
export G5_COMPLIANCE_EXPECTED_CHILD_COMPONENT="${G5_TIMSYSTEM_EXPECTED_CHILD_COMPONENT:-}"
export G5_COMPLIANCE_EXPECTED_ONTOLOGY_SHAPE_LABEL="${G5_TIMSYSTEM_EXPECTED_ONTOLOGY_SHAPE_LABEL:-timsystem-code_project → tim_system}"
export G5_COMPLIANCE_REQUIRED_DOC_LINK_PATH="${G5_TIMSYSTEM_REQUIRED_DOC_LINK_PATH-}"
export G5_COMPLIANCE_REQUIRED_DOC_LINK_MIN="${G5_TIMSYSTEM_REQUIRED_DOC_LINK_MIN-1}"

"$SCRIPT_DIR/seed-compliance-workspace.sh" "$DOCS_SOURCE_REPO" "$CODE_SOURCE_REPO" "$API_URL"

if [ ! -f "$ENV_FILE" ]; then
  echo "ERROR: TimSystem seed env file was not written: $ENV_FILE" >&2
  exit 1
fi

# shellcheck disable=SC1090
. "$ENV_FILE"

if [ -z "${G5_COMPLIANCE_WORKSPACE_ID:-}" ]; then
  echo "ERROR: TimSystem seed env file did not contain G5_COMPLIANCE_WORKSPACE_ID" >&2
  exit 1
fi

docker exec -i "$PG_CONTAINER" psql -U g5 -d "$DB_NAME" -v ON_ERROR_STOP=1 \
  -v workspace_id="$G5_COMPLIANCE_WORKSPACE_ID" \
  -v artifact_path="$TIMSYSTEM_RUNTIME_ARTIFACT_PATH" <<'EOSQL' >/dev/null
BEGIN;
ALTER TABLE chat_sessions DISABLE ROW LEVEL SECURITY;
ALTER TABLE chat_messages DISABLE ROW LEVEL SECURITY;
ALTER TABLE chat_sessions DISABLE TRIGGER trg_cs_set_org;
ALTER TABLE chat_messages DISABLE TRIGGER trg_cm_set_org;

DELETE FROM chat_messages
WHERE session_id IN (
  SELECT id
  FROM chat_sessions
  WHERE workspace_id = :'workspace_id'::uuid
    AND title = 'TimSystem runtime artifact fixture'
);
DELETE FROM chat_sessions
WHERE workspace_id = :'workspace_id'::uuid
  AND title = 'TimSystem runtime artifact fixture';

WITH target_workspace AS (
  SELECT id AS workspace_id, org_id, owner_user_id
  FROM workspaces
  WHERE id = :'workspace_id'::uuid
),
seed_session AS (
  INSERT INTO chat_sessions (
    workspace_id, scope_type, title, summary,
    tags, org_id, created_by, agent_provider, created_at, updated_at
  )
  SELECT
    workspace_id,
    'workspace',
    'TimSystem runtime artifact fixture',
    'Seeded owned provider artifact used by TimSystem runtime artifact E2E coverage.',
    ARRAY['e2e', 'runtime-artifact']::text[],
    org_id,
    owner_user_id,
    'claude_code',
    now(),
    now()
  FROM target_workspace
  RETURNING id, org_id
),
seed_message AS (
  INSERT INTO chat_messages (
    session_id, role, content, artifacts, citations,
    status, message_index, org_id, created_at
  )
  SELECT
    id,
    'assistant',
    repeat('TimSystem provider tool result fixture for artifact API validation. ', 128),
    jsonb_build_object(
      'artifacts',
      jsonb_build_array(
        jsonb_build_object(
          'kind', 'tool_result',
          'path', :'artifact_path',
          'content', repeat('TimSystem provider tool result fixture for artifact API validation. ', 128)
        )
      )
    ),
    '[]'::jsonb,
    'complete',
    0,
    org_id,
    now()
  FROM seed_session
  RETURNING session_id
)
UPDATE chat_sessions
SET message_count = 1, updated_at = now()
WHERE id IN (SELECT session_id FROM seed_message);

-- Drain deferred foreign keys before changing the trigger state.
SET CONSTRAINTS ALL IMMEDIATE;
ALTER TABLE chat_messages ENABLE TRIGGER trg_cm_set_org;
ALTER TABLE chat_sessions ENABLE TRIGGER trg_cs_set_org;
ALTER TABLE chat_messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE chat_messages FORCE ROW LEVEL SECURITY;
ALTER TABLE chat_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE chat_sessions FORCE ROW LEVEL SECURITY;
COMMIT;
EOSQL
echo "✓ Runtime artifact fixture seeded: $TIMSYSTEM_RUNTIME_ARTIFACT_PATH"

TIMSYSTEM_WORKSPACE_NAME="${G5_COMPLIANCE_WORKSPACE_NAME:-${G5_TIMSYSTEM_WORKSPACE_NAME:-timsystem}}"
cat >> "$ENV_FILE" <<EOF
export G5_TIMSYSTEM_WORKSPACE_ID=${G5_COMPLIANCE_WORKSPACE_ID:-}
export G5_TIMSYSTEM_DOCS_REPO_ID=${G5_COMPLIANCE_DOCS_REPO_ID:-}
export G5_TIMSYSTEM_DOCS_BRANCH_ID=${G5_COMPLIANCE_DOCS_BRANCH_ID:-}
export G5_TIMSYSTEM_CODE_REPO_ID=${G5_COMPLIANCE_CODE_REPO_ID:-}
export G5_TIMSYSTEM_CODE_BRANCH_ID=${G5_COMPLIANCE_CODE_BRANCH_ID:-}
export G5_TIMSYSTEM_WORKSPACE_NAME=${TIMSYSTEM_WORKSPACE_NAME}
export VITE_E2E_TIMSYSTEM_WORKSPACE=${TIMSYSTEM_WORKSPACE_NAME}
EOF
