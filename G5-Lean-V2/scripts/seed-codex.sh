#!/usr/bin/env bash
# =============================================================================
# seed-codex.sh — Seed a G5 workspace with the codex-rs codebase
# =============================================================================
# Sets up a codex-rs workspace in the running G5 dev stack so that
# explainer generation can operate on codex-rs code.
#
# Steps:
#   1. Authenticate via test-token endpoint
#   2. Re-use the existing org, create a "codex" workspace with code and docs repos
#   3. Create local workspace directory with codex-rs source + ontology
#   4. Copy tool_system_refactor.md as the instruction doc
#   5. Publish repo to API container volume
#   6. Build ontology from .semlift/specs YAML frontmatter
#   7. Verify the optional instruction documents from the docs repository
#   8. (Optional) Create an architecture explainer for the tool system refactor
#
# Usage:
#   ./scripts/seed-codex.sh [api-url] [codex-dir]
#   Default API: http://localhost:${G5_PORT_API:-7500}
#   Default codex source: ../../codex/codex-rs (relative to generation/)
#
# Requires: curl, jq, git, python3, running g5-api with G5_E2E_AUTH=true
# =============================================================================
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/seed-workspace-model.sh"
AUTH_EMAIL="demo@g5labs.ai"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
psql_exec() { docker exec g5-dev-postgres psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -tAc "$1"; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
G5_ROOT="$REPO_ROOT"

API_URL="${1:-http://localhost:${G5_PORT_API:-7500}}"
CODEX_SRC="${2:-$G5_ROOT/codex/codex-rs}"
WORKSPACE_DIR="$REPO_ROOT/codex-workspace"

# ─── Validate codex source exists ────────────────────────────────────────────

if [ ! -d "$CODEX_SRC" ]; then
  echo "  ✗ Codex source not found at: $CODEX_SRC"
  echo "  Usage: $0 [api-url] [codex-rs-dir]"
  exit 1
fi

if [ ! -d "$CODEX_SRC/.semlift/specs" ]; then
  echo "  ✗ No .semlift/specs directory found in $CODEX_SRC"
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

echo "=== G5 Codex Workspace Setup ==="
echo "  API:        $API_URL"
echo "  Codex src:  $CODEX_SRC"
echo "  Workspace:  $WORKSPACE_DIR"
echo ""

# ─── Helpers ─────────────────────────────────────────────────────────────────

api_post() {
  local path="$1" data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

api_get() {
  local path="$1"
  curl -sf "${API_URL}${path}"
}

# ─── Wait for API ────────────────────────────────────────────────────────────

echo "Waiting for g5-api..."
for i in $(seq 1 30); do
  if curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
    echo "  ✓ API is healthy"
    break
  fi
  if [ "$i" -eq 30 ]; then
    echo "  ✗ API not healthy after 30 seconds"
    exit 1
  fi
  sleep 1
done

# ─── Step 1: Authenticate ────────────────────────────────────────────────────

echo ""
echo "=== Step 1: Authenticate ==="
AUTH_RESP=$(api_post "/api/v1/auth/test-token" '{"email": "demo@g5labs.ai"}' 2>/dev/null || echo "")
AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
if [ -n "$AUTH_TOKEN" ]; then
  echo "  ✓ Got auth token"
else
  echo "  ✗ Failed to get auth token"
  echo "  Response: $AUTH_RESP"
  exit 1
fi

api_authed_post() {
  local path="$1" data="$2"
  local headers=(-H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -s "${API_URL}${path}" "${headers[@]}" -d "$data"
}

api_authed_get() {
  local path="$1"
  local headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -s "${API_URL}${path}" "${headers[@]}"
}

# ─── Step 2: Create codex workspace ──────────────────────────────────────────

echo ""
echo "=== Step 2: Create codex workspace + repo ==="

# Get the existing org (created by seed-demo or test-token)
ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")
ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)

if [ -z "$ORG_ID" ]; then
  echo "  ✗ No orgs found — run 'just demo-up' first or ensure test-token auto-creates org"
  exit 1
fi
ORG_SLUG=$(echo "$ORGS_RESP" | jq -r '.data[0].orgSlug // empty' 2>/dev/null)
echo "  ✓ Org: $ORG_SLUG ($ORG_ID)"

# Check if a "codex" workspace already exists
WS_RESP=$(api_authed_get "/api/v1/orgs/${ORG_ID}/workspaces" 2>/dev/null || echo "")
WS_ID=$(echo "$WS_RESP" | jq -r '.data[] | select(.name == "codex" or .name == "Codex") | .id // empty' 2>/dev/null | head -1)

if [ -n "$WS_ID" ]; then
  echo "  ✓ Codex workspace already exists: $WS_ID"
else
  WS_ID=$(seed_create_fixture_workspace codex)
  echo "  ✓ Created codex workspace: $WS_ID"
fi

# Check if a "codex" repo already exists in this workspace
BRANCHES_RESP=$(api_authed_get "/api/v1/workspaces/${WS_ID}/branches" 2>/dev/null || echo "")
CODEX_REPO_ID=$(echo "$BRANCHES_RESP" | jq -r '.data[] | select(.repoName == "codex") | .repoId // empty' 2>/dev/null | head -1)

if [ -n "$CODEX_REPO_ID" ]; then
  echo "  ✓ Codex repo already linked to workspace: $CODEX_REPO_ID"
else
  CODEX_REPO_ID=$(seed_add_fixture_repo codex codex code)
  echo "  ✓ Created + linked codex repo: $CODEX_REPO_ID"
fi

# Explainers write to a dedicated docs member, not the code repository.
WORKSPACE_NAME=$(psql_exec "SELECT name FROM workspaces WHERE id = '${WS_ID}' AND org_id = '${ORG_ID}';")
if [ -z "$WORKSPACE_NAME" ]; then
  echo "  ✗ Cannot resolve Codex workspace name" >&2
  exit 1
fi
DOCS_REPO_ID=$(psql_exec "SELECT repo_id FROM workspace_repos WHERE workspace_id = '${WS_ID}' AND org_id = '${ORG_ID}' AND role = 'docs';")
if [ -z "$DOCS_REPO_ID" ]; then
  DOCS_REPO_ID=$(seed_add_fixture_repo codex-docs codex-docs docs)
fi
DOCS_ROOT_PATH=$(psql_exec "SELECT root_path FROM workspace_repos WHERE workspace_id = '${WS_ID}' AND repo_id = '${DOCS_REPO_ID}';")
if [ -z "$DOCS_ROOT_PATH" ]; then
  echo "  ✗ Cannot resolve Codex docs root path" >&2
  exit 1
fi
echo "  ✓ Docs repo linked to workspace: $DOCS_REPO_ID"

# ─── Step 3: Create local workspace directory ────────────────────────────────

echo ""
echo "=== Step 3: Create local workspace directory ==="

if [ -d "$WORKSPACE_DIR" ]; then
  echo "  Removing existing codex workspace..."
  chmod -R u+w "$WORKSPACE_DIR" 2>/dev/null || true
  rm -rf "$WORKSPACE_DIR" 2>/dev/null || true
  if [ -d "$WORKSPACE_DIR" ]; then
    sudo rm -rf "$WORKSPACE_DIR" 2>/dev/null || true
  fi
fi

mkdir -p "$WORKSPACE_DIR/.g5"

cat > "$WORKSPACE_DIR/.g5/workspace.yaml" <<EOF
workspace_id: "${WS_ID}"
remote:
  enabled: true
  api_url: "${API_URL}"
  project_id: null
docs:
  repo_url: "${API_URL}/git/${DOCS_REPO_ID}"
  repo_id: "${DOCS_REPO_ID}"
  branch: "${WORKSPACE_NAME}"
  path: "repos/${DOCS_ROOT_PATH}"
repos:
  - name: codex
    url: "${API_URL}/git/${CODEX_REPO_ID}"
    repo_id: "${CODEX_REPO_ID}"
    branch: "${WORKSPACE_NAME}"
    path: "."
EOF
echo "  ✓ .g5/workspace.yaml (workspace_id: ${WS_ID})"

# ─── Step 4: Copy codex-rs ontology (specs) ──────────────────────────────────

echo ""
echo "=== Step 4: Copy codex-rs ontology ==="

SPECS_SRC="$CODEX_SRC/.semlift/specs"
SPECS_DEST="$WORKSPACE_DIR/.semlift/specs"

mkdir -p "$SPECS_DEST"
cp -r "$SPECS_SRC/"* "$SPECS_DEST/"
SPEC_COUNT=$(find "$SPECS_DEST" -name '*.semlift.md' -type f | wc -l | tr -d ' ')
echo "  ✓ Copied $SPEC_COUNT spec files to .semlift/specs/"

# Create a semlift.yaml config for the codex workspace
cat > "$WORKSPACE_DIR/.semlift/semlift.yaml" <<'EOF'
workspace_config:
  source_paths:
    - .

analysis:
  supported_languages: ['rust']
  exclude_patterns:
    - '.semlift/*'
    - 'target/*'
    - '*.pyc'
  max_file_size_mb: 5
  ignore_binary_files: true

inference:
  provider: openrouter
  model: anthropic/claude-sonnet-4.6
  api_key: ${OPENROUTER_API_KEY}
  temperature: 0.1
  max_tokens: 64000
  timeout_seconds: 600

summarization:
  enable_summarization: true
  max_tokens_per_level: 700000
  max_levels: 2
  preserve_prefix_count: 3
  target_summary_tokens: 70000
  token_estimate_divisor: 3

tasks:
  max_concurrent: 1
  timeout_seconds: 3600
  max_retries: 2
  monitor_timeout_minutes: 1200
  agent_execution_model: "react"
  react_max_iterations: 150
  react_iteration_timeout: 600.0
  react_convergence_threshold: 0.8
  react_allow_direct_mcp_execution: true

workspace:
  isolation_enabled: false
  isolation_for_task_types: ["downlift"]
  force_isolation_for_all_tasks: false
  git_integration: true
  auto_cleanup: true
  preserve_on_failure: true
  max_concurrent_workspaces: 3
  workspace_timeout_seconds: 600

logging:
  level: INFO
  save_trajectory: true

agents:
  execution_model: react

use_d4_format: false
use_suffix_component_id: true
prompt_compression_level: full

views:
  fan_out_sync: true

downlift:
  root_spec: codex-rs_project_aggregate
  generate_child_specs: true
  output_dir: generated/
  incremental: true
  overwrite: false
EOF
echo "  ✓ Created .semlift/semlift.yaml"

# ─── Step 5: Copy codex-rs source code ───────────────────────────────────────

echo ""
echo "=== Step 5: Copy codex-rs source code ==="

# Copy Cargo workspace files
cp "$CODEX_SRC/Cargo.toml" "$WORKSPACE_DIR/" 2>/dev/null || true
cp "$CODEX_SRC/Cargo.lock" "$WORKSPACE_DIR/" 2>/dev/null || true

# Copy all crate source directories (src/ and tests/)
CRATE_COUNT=0
for crate_dir in "$CODEX_SRC"/*/; do
  crate_name=$(basename "$crate_dir")

  # Skip non-crate directories
  case "$crate_name" in
    target|scripts|docs|.semlift|.git|.github|node_modules) continue ;;
  esac

  # Copy src/ directory
  if [ -d "$crate_dir/src" ]; then
    mkdir -p "$WORKSPACE_DIR/$crate_name/src"
    cp -r "$crate_dir/src/"* "$WORKSPACE_DIR/$crate_name/src/" 2>/dev/null || true
    CRATE_COUNT=$((CRATE_COUNT + 1))
  fi

  # Copy tests/ directory if present
  if [ -d "$crate_dir/tests" ]; then
    mkdir -p "$WORKSPACE_DIR/$crate_name/tests"
    cp -r "$crate_dir/tests/"* "$WORKSPACE_DIR/$crate_name/tests/" 2>/dev/null || true
  fi

  # Copy Cargo.toml per crate
  [ -f "$crate_dir/Cargo.toml" ] && cp "$crate_dir/Cargo.toml" "$WORKSPACE_DIR/$crate_name/" 2>/dev/null || true

  # Copy build.rs if exists
  [ -f "$crate_dir/build.rs" ] && cp "$crate_dir/build.rs" "$WORKSPACE_DIR/$crate_name/" 2>/dev/null || true
done

TOTAL_FILES=$(find "$WORKSPACE_DIR" -type f \( -name '*.rs' -o -name '*.toml' \) | wc -l | tr -d ' ')
echo "  ✓ Copied $CRATE_COUNT crates ($TOTAL_FILES Rust + TOML files)"

# ─── Step 5.5: Copy instruction document ────────────────────────────────────

echo ""
echo "=== Step 5.5: Copy instruction documents ==="

INSTRUCTION_SRC="$G5_ROOT/archived_aux_files_readonly/demo_plan/tool_system_refactor.md"
INSTRUCTION_DEST="$WORKSPACE_DIR/docs"
mkdir -p "$INSTRUCTION_DEST"

if [ -f "$INSTRUCTION_SRC" ]; then
  cp "$INSTRUCTION_SRC" "$INSTRUCTION_DEST/"
  echo "  ✓ Copied tool_system_refactor.md to docs/"
else
  echo "  ⚠ tool_system_refactor.md not found at $INSTRUCTION_SRC"
fi

# Also copy the remediation doc if present
REMEDIATION_SRC="$G5_ROOT/archived_aux_files_readonly/demo_plan/tool_system_refactor_remediation.md"
if [ -f "$REMEDIATION_SRC" ]; then
  cp "$REMEDIATION_SRC" "$INSTRUCTION_DEST/"
  echo "  ✓ Copied tool_system_refactor_remediation.md to docs/"
fi

# ─── Step 6: Initialize git repo ────────────────────────────────────────────
#
# CODEX_SRC is an already-git-tracked upstream tree (seed_scripts D31). A
# plain `cp -r` (Steps 4-5) followed by an unforced, non-force-flagged git
# staging command silently drops any upstream file that is tracked in
# codex-rs's own history but also matches a `.gitignore` pattern carried
# over with a copied subtree, so this staging step force-adds everything
# the filesystem copy produced and then
# asserts each copied subtree's staged tree hash exactly matches the
# upstream tracked tree hash for that subtree, failing loudly on drift
# instead of adding `!`-negation rules to any seeded ignore file.

echo ""
echo "=== Step 6: Initialize git repo ==="

if [ ! -d "$CODEX_SRC/.git" ]; then
  echo "  ✗ $CODEX_SRC is not a git checkout — cannot verify tracked-file fidelity"
  echo "  seed_scripts D31 requires copied-tree seeds to stage from a verified"
  echo "  clean clone of the upstream source; point codex-dir at a real git"
  echo "  checkout of codex-rs and re-run."
  exit 1
fi

if [ -n "$(git -C "$CODEX_SRC" status --porcelain 2>/dev/null)" ]; then
  echo "  ✗ $CODEX_SRC has uncommitted changes — refusing to seed from a dirty tree"
  echo "  Commit or discard local changes in $CODEX_SRC, then re-run."
  exit 1
fi
CODEX_SRC_HEAD="$(git -C "$CODEX_SRC" rev-parse HEAD)"

cd "$WORKSPACE_DIR"
git init -q
git add -f -A
git commit -q -m "Initial codex-rs workspace for G5 explainer generation"
echo "  ✓ Git repo initialized with initial commit (force-added tracked-but-ignored paths)"

# Tree-hash fidelity gate: every subtree copied from codex-rs must stage to
# the exact same tree hash the upstream commit records for that subtree,
# proving the force-add above reproduced the upstream tracked set exactly
# rather than merely widening what got staged.
verify_codex_subtree_hash() {
  local rel_path="$1"
  local staged_hash upstream_hash
  staged_hash="$(git rev-parse "HEAD:${rel_path}" 2>/dev/null || echo "missing")"
  upstream_hash="$(git -C "$CODEX_SRC" rev-parse "${CODEX_SRC_HEAD}:${rel_path}" 2>/dev/null || echo "missing")"
  if [ "$staged_hash" != "$upstream_hash" ]; then
    echo "  ✗ Tree-hash mismatch for '${rel_path}': staged=${staged_hash} upstream=${upstream_hash}"
    echo "  The seeded workspace tree diverges from upstream codex-rs commit ${CODEX_SRC_HEAD}."
    exit 1
  fi
  echo "  ✓ ${rel_path} tree hash matches upstream ($staged_hash)"
}

verify_codex_subtree_hash ".semlift/specs"
for crate_dir in "$CODEX_SRC"/*/; do
  crate_name=$(basename "$crate_dir")
  case "$crate_name" in
    target|scripts|docs|.semlift|.git|.github|node_modules) continue ;;
  esac
  [ -d "$crate_dir/src" ] && verify_codex_subtree_hash "${crate_name}/src"
  [ -d "$crate_dir/tests" ] && verify_codex_subtree_hash "${crate_name}/tests"
done

cd "$REPO_ROOT"

# ─── Step 7: Publish repo to API container volume ───────────────────────────

echo ""
echo "=== Step 7: Publish repo to API container volume ==="

if [ -n "$CODEX_REPO_ID" ]; then
  REPO_DEST="/data/repos/${ORG_ID}/${CODEX_REPO_ID}"

  docker exec g5-dev-api mkdir -p "$REPO_DEST" 2>/dev/null || true

  if tar -C "$WORKSPACE_DIR" -cf - . | docker exec -i g5-dev-api tar --no-same-owner -C "$REPO_DEST" -xf -; then
    echo "  ✓ Copied workspace to API volume: $REPO_DEST"
  else
    echo "  ⚠ tar pipe failed — retrying with docker cp..."
    docker cp "$WORKSPACE_DIR/." "g5-dev-api:${REPO_DEST}/"
  fi

  docker exec g5-dev-api chmod -R a+rwX "$REPO_DEST" 2>/dev/null || true

  # Verify: check that spec files have content
  EMPTY_COUNT=$(docker exec g5-dev-api find "$REPO_DEST/.semlift/specs" -name '*.semlift.md' -size 0 2>/dev/null | wc -l | tr -d ' ')
  if [ "${EMPTY_COUNT:-0}" -gt 0 ]; then
    echo "  ⚠ Found $EMPTY_COUNT empty spec files — retrying copy..."
    tar -C "$WORKSPACE_DIR" -cf - . | docker exec -i g5-dev-api tar --no-same-owner -C "$REPO_DEST" -xf -
    docker exec g5-dev-api chmod -R a+rwX "$REPO_DEST" 2>/dev/null || true
  fi

  # Verify source code accessible
  RS_COUNT=$(docker exec g5-dev-api find "$REPO_DEST" -name '*.rs' -type f 2>/dev/null | wc -l | tr -d ' ')
  echo "  ✓ Verified: $RS_COUNT .rs files accessible in container"
else
  echo "  ✗ No repo ID — cannot publish"
  exit 1
fi

# Keep an existing docs repository intact on reruns. A fresh fixture needs
# a real commit before seed_capture_fixture_parent selects its workspace ref.
docker exec "$API_CONTAINER" sh -eu -c '
  repo="/data/repos/$1/$2"
  mkdir -p "$repo"
  if ! git -C "$repo" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$repo" init -q -b main
  fi
  if ! git -C "$repo" rev-parse --verify "HEAD^{commit}" >/dev/null 2>&1; then
    git -C "$repo" -c user.name="G5 Seed" -c user.email="seed@g5.local" \
      commit -q --allow-empty -m "Initialize Codex docs"
  fi
  git -C "$repo" rev-parse --verify "HEAD^{commit}" >/dev/null
' sh "$ORG_ID" "$DOCS_REPO_ID"

# Seed optional instruction files into the canonical docs Git repository.
# API document writes require a running workspace VM; fixture construction
# happens before one exists. Keep any existing document content on reruns.
for DOC_NAME in tool_system_refactor tool_system_refactor_remediation; do
  DOC_PATH="docs/${DOC_NAME}.md"
  [ -f "$WORKSPACE_DIR/$DOC_PATH" ] || continue
  docker exec -i "$API_CONTAINER" sh -eu -c '
    repo="/data/repos/$1/$2"
    path="$3"
    if git -C "$repo" cat-file -e "HEAD:$path" 2>/dev/null; then
      cat >/dev/null
    else
      mkdir -p "$repo/docs"
      cat >"$repo/$path"
      git -C "$repo" add -- "$path"
      git -C "$repo" -c user.name="G5 Seed" -c user.email="seed@g5.local" \
        commit -q --only -m "Seed Codex instruction document" -- "$path"
    fi
  ' sh "$ORG_ID" "$DOCS_REPO_ID" "$DOC_PATH" <"$WORKSPACE_DIR/$DOC_PATH"
done

seed_capture_fixture_parent "$WS_ID" codex

# ─── Step 9: Verify the canonical instruction documents ─────────────────────

echo ""
echo "=== Step 9: Verify instruction documents ==="
DOCS_RESP=$(api_authed_get "/api/v1/workspaces/${WS_ID}/docs")
echo "$DOCS_RESP" | jq -e '.data.docs | arrays' >/dev/null || {
  echo "  ✗ Cannot read workspace docs: $DOCS_RESP" >&2
  exit 1
}
for DOC_NAME in tool_system_refactor tool_system_refactor_remediation; do
  DOC_PATH="docs/${DOC_NAME}.md"
  [ -f "$WORKSPACE_DIR/$DOC_PATH" ] || continue
  echo "$DOCS_RESP" | jq -e --arg path "$DOC_PATH" '.data.docs | any(.filePath == $path)' >/dev/null || {
    echo "  ✗ Seeded instruction is not readable: $DOC_PATH" >&2
    exit 1
  }
  echo "  ✓ Instruction doc available: $DOC_PATH"
done

# ─── Step 10: Optionally trigger an architecture explainer ──────────────────────────

echo ""
echo "=== Step 10: Explainer creation (manual trigger) ==="
echo ""
echo "  The workspace is ready. To create an architecture explainer for the tool system,"
echo "  configure an agent provider credential and permission mode in org settings,"
echo "  then run the following curl command (or use the G5 frontend):"
echo ""

# Build the first message of an ordinary workspace conversation
EXPLAINER_TOPIC="Codex tool system architecture"
EXPLAINER_PROMPT="Analyze the codex-rs tool system and explain tool dispatch across codex.rs, openai_tools.rs, and function_tool.rs, including the module boundaries, tests, and error handling."
if [ -f "$WORKSPACE_DIR/docs/tool_system_refactor.md" ]; then
  EXPLAINER_TOPIC="Tool system refactor architecture"
  EXPLAINER_PROMPT="${EXPLAINER_PROMPT} Use the instruction document docs/tool_system_refactor.md in the docs repository to explain the proposed refactor and migration plan."
fi

EXPLAINER_PROMPT="Create an explainer: ${EXPLAINER_TOPIC}. ${EXPLAINER_PROMPT} Create one self-contained HTML page. Save it in the docs repository under docs/${WS_ID}/explainers/ with a descriptive filename ending in the current date and time (YYYY-MM-DD-HHMM).html; append a numeric suffix if needed to avoid overwriting an existing file. Write the file and report its workspace path in this chat."

cat <<EXPLAINERCMD
  curl -X POST "${API_URL}/api/v1/workspaces/${WS_ID}/sessions/start" \\
    -H "Content-Type: application/json" \\
    -H "Authorization: Bearer \$TOKEN" \\
    -d '{
      "title": "${EXPLAINER_TOPIC}",
      "content": "${EXPLAINER_PROMPT}"
    }'
EXPLAINERCMD

echo ""
echo "  To get a token first:"
echo "    TOKEN=\$(curl -s ${API_URL}/api/v1/auth/test-token -H 'Content-Type: application/json' -d '{\"email\":\"demo@g5labs.ai\"}' | jq -r '.data.jwt')"
echo ""

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo "============================================="
echo "  Codex workspace ready!"
echo "============================================="
echo ""
echo "  Directory:     $WORKSPACE_DIR"
echo "  Workspace ID:  $WS_ID"
echo "  Org ID:        $ORG_ID"
echo "  Code repo ID:  $CODEX_REPO_ID"
echo "  Docs repo ID:  $DOCS_REPO_ID"
echo "  API URL:       $API_URL"
echo ""
echo "  Contents:"
du -sh "$WORKSPACE_DIR/.semlift/specs" 2>/dev/null | sed 's/^/    /'
for crate in core cli tui common exec; do
  du -sh "$WORKSPACE_DIR/$crate/src" 2>/dev/null | sed 's/^/    /'
done
echo ""
echo ""
echo "  Environment variables:"
echo "    export G5_CODEX_WORKSPACE_ID=${WS_ID}"
echo "    export G5_CODEX_ORG_ID=${ORG_ID}"
echo "    export G5_CODEX_REPO_ID=${CODEX_REPO_ID}"
