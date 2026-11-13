#!/usr/bin/env bash
# =============================================================================
# seed-demo-multi.sh — Seed 3 demo workspaces showing codex uplift/downlift flow
# =============================================================================
# Sets up three workspaces from the codex_g5 repo at different commits to
# demonstrate the full SemLift lifecycle:
#
#   Workspace 1 ("codex-empty")     — Bare empty workspace, no repos.
#                                     Shows starting from scratch — user adds
#                                     a remote repo and triggers uplift.
#
#   Workspace 2 ("codex-uplifted")  — Fully uplifted repo with specs + two
#                                     architecture/test views (commit 7373aff).
#                                     Used to trigger downlift using the views
#                                     (tool_system_refactor design doc).
#
#   Workspace 3 ("codex-refactored") — Post-downlift state: specs updated +
#                                      code refactored per views (commit 0934045).
#                                      Shows the end result of the downlift.
#
# Flow: 1 (uplift) → 2 (downlift) → 3 (result)
#
# Usage:
#   ./scripts/seed-demo-multi.sh [api-url]
#   Default API: http://localhost:7500
#
# Requires: curl, jq, git, running g5-api with G5_E2E_AUTH=true
#            G5_CODEX_DEMO_SOURCE_REPO (defaults to <repo-root>/codex_g5)
# =============================================================================
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/seed-workspace-model.sh"
AUTH_EMAIL="demo@g5labs.ai"
API_CONTAINER="${API_CONTAINER:-g5-dev-api}"
psql_exec() { docker exec g5-dev-postgres psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -tAc "$1"; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CODEX_REPO="${G5_CODEX_DEMO_SOURCE_REPO:-$REPO_ROOT/codex_g5}"
if ! git -C "$CODEX_REPO" rev-parse --show-toplevel >/dev/null 2>&1; then
  echo "Missing Codex demo source repository: $CODEX_REPO. Set G5_CODEX_DEMO_SOURCE_REPO to a checkout containing commits 7373aff and 0934045." >&2
  exit 2
fi
for commit in 7373aff 0934045; do
  if ! git -C "$CODEX_REPO" cat-file -e "${commit}^{commit}" 2>/dev/null; then
    echo "Codex demo source $CODEX_REPO is missing required commit $commit." >&2
    exit 2
  fi
done
CODEX_REPO="$(cd "$CODEX_REPO" && pwd)"
if [[ "${1:-}" == "--check-source" ]]; then
  echo "Codex demo source is ready: $CODEX_REPO"
  exit 0
fi

API_URL="${1:-http://localhost:7500}"

# ─── Workspace definitions ───────────────────────────────────────────────────
# Each workspace maps to a specific commit in the codex_g5 repo.
#                   Name              Commit    Description
WS_DEFS=(
  "codex-empty:NONE:Bare workspace (add repo manually)"
  "codex-uplifted:7373aff:Uplifted with specs and views"
  "codex-refactored:0934045:Post-downlift refactored code"
)

# ─── Auto-detect API URL ─────────────────────────────────────────────────────
if [ "$API_URL" = "auto" ] || ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
  if curl -sf "http://localhost:7500/healthz" > /dev/null 2>&1; then
    API_URL="http://localhost:7500"
  else
    API_IP=$(docker inspect g5-dev-api --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && curl -sf "http://${API_IP}:7500/healthz" > /dev/null 2>&1; then
      API_URL="http://${API_IP}:7500"
    else
      echo "  ✗ Cannot reach g5-api at localhost:7500 or via container IP"
      echo "  Make sure dev stack is running: just dev-up"
      exit 1
    fi
  fi
fi

echo "════════════════════════════════════════════════════════════"
echo "  G5 Multi-Workspace Demo Setup"
echo "════════════════════════════════════════════════════════════"
echo "  API:   $API_URL"
echo "  Codex: $CODEX_REPO"
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
  [ "$i" -eq 30 ] && { echo "  ✗ API not healthy after 30s"; exit 1; }
  sleep 1
done

# ─── Step 1: Authenticate ────────────────────────────────────────────────────

echo ""
echo "=== Step 1: Authenticate ==="
AUTH_RESP=$(api_post "/api/v1/auth/test-token" '{"email": "demo@g5labs.ai"}' 2>/dev/null || echo "")
AUTH_TOKEN=""
if [ -n "$AUTH_RESP" ]; then
  AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
  [ -n "$AUTH_TOKEN" ] && echo "  ✓ Got auth token" || echo "  ⚠ No auth token"
fi
if [ -z "$AUTH_TOKEN" ]; then
  echo "  ✗ E2E auth is required for multi-workspace seeding"
  exit 1
fi

api_authed_post() {
  local path="$1" data="$2"
  local headers=(-H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf "${API_URL}${path}" "${headers[@]}" -d "$data"
}

api_authed_get() {
  local path="$1"
  local headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf "${API_URL}${path}" "${headers[@]}"
}

api_authed_put() {
  local path="$1" data="$2"
  local headers=(-H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf -X PUT "${API_URL}${path}" "${headers[@]}" -d "$data"
}

# ─── Step 2: Get org ─────────────────────────────────────────────────────────

echo ""
echo "=== Step 2: Resolve org ==="
ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")
ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)
[ -z "$ORG_ID" ] && { echo "  ✗ No orgs found"; exit 1; }
ORG_SLUG=$(echo "$ORGS_RESP" | jq -r '.data[0].orgSlug // empty' 2>/dev/null)
echo "  ✓ Org: $ORG_SLUG ($ORG_ID)"

# Get docs repo (created by auth seeding)
REPOS_RESP=$(api_authed_get "/api/v1/orgs/${ORG_ID}/repos" 2>/dev/null || echo "")
DOCS_REPO_ID=$(echo "$REPOS_RESP" | jq -r '.data[] | select(.slug == "docs") | .id // empty' 2>/dev/null | head -1)
echo "  ✓ Docs repo: ${DOCS_REPO_ID:-none}"

# ─── Step 3: Create workspaces + per-workspace repos ─────────────────────────
# Each workspace needs its own repo so file content is served independently.
# The API resolves files through workspace/repository membership and its Git ref.
# so each workspace's codex snapshot lives at a unique path.

echo ""
echo "=== Step 3: Create workspaces ==="

# Map workspace name → array index (bash 3.x compatible — no associative arrays)
ws_idx() {
  case "$1" in
    codex-empty)      echo 0 ;;
    codex-uplifted)   echo 1 ;;
    codex-refactored) echo 2 ;;
  esac
}
WS_IDS=("" "" "")
WS_COMMITS=("" "" "")
WS_DESCS=("" "" "")
WS_REPO_IDS=("" "" "")
WS_REAL_COMMITS=("" "" "")

for ws_def in "${WS_DEFS[@]}"; do
  IFS=':' read -r ws_name ws_commit ws_desc <<< "$ws_def"

  echo ""
  echo "  ── Creating workspace: $ws_name ──"

  ALL_WS=$(api_authed_get "/api/v1/orgs/${ORG_ID}/workspaces")
  WS_ID=$(echo "$ALL_WS" | jq -r --arg name "$ws_name" '.data[] | select(.name == $name) | .id')
  [ -n "$WS_ID" ] || WS_ID=$(seed_create_fixture_workspace "$ws_name")

  WS_IDS[$(ws_idx "$ws_name")]="$WS_ID"
  WS_COMMITS[$(ws_idx "$ws_name")]="$ws_commit"
  WS_DESCS[$(ws_idx "$ws_name")]="$ws_desc"

  # codex-empty is a bare workspace — no repo needed
  if [ "$ws_commit" = "NONE" ]; then
    echo "    (bare workspace — no repo)"
    WS_REPO_IDS[$(ws_idx "$ws_name")]=""
    continue
  fi

  # Create a per-workspace repo so each workspace has its own file tree.
  # All 3 workspaces have different codex snapshots (different commits),
  # so they can't share a single repo directory.
  REPO_SLUG="codex-${ws_name}"
  REPO_ID=$(psql_exec "SELECT repo_id FROM workspace_repos WHERE org_id = '${ORG_ID}' AND workspace_id = '${WS_ID}' AND role = 'code';")
  [ -n "$REPO_ID" ] || REPO_ID=$(seed_add_fixture_repo "Codex ($ws_name)" "$REPO_SLUG" code)
  echo "    ✓ Fixture repo: $REPO_ID ($REPO_SLUG)"

  WS_REPO_IDS[$(ws_idx "$ws_name")]="$REPO_ID"
done

echo ""
echo "  Summary:"
for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_commit="${WS_COMMITS[$(ws_idx "$ws_name")]}"
  if [ "$ws_commit" = "NONE" ]; then
    echo "    $ws_name → WS=${WS_IDS[$(ws_idx "$ws_name")]:-FAILED} (bare, no repo)"
  else
    echo "    $ws_name → WS=${WS_IDS[$(ws_idx "$ws_name")]:-FAILED} REPO=${WS_REPO_IDS[$(ws_idx "$ws_name")]:-FAILED} @ $ws_commit"
  fi
done

# ─── Step 4: Prepare codex repo snapshots ────────────────────────────────────

echo ""
echo "=== Step 4: Prepare codex snapshots ==="

STAGING_DIR=$(mktemp -d /tmp/g5-demo-multi.XXXXXX)
trap "rm -rf $STAGING_DIR" EXIT

for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_commit="${WS_COMMITS[$(ws_idx "$ws_name")]}"
  ws_id="${WS_IDS[$(ws_idx "$ws_name")]:-}"
  [ -z "$ws_id" ] && continue

  WS_DIR="$STAGING_DIR/$ws_name"
  # Skip bare workspaces — nothing to stage
  if [ "$ws_commit" = "NONE" ]; then
    echo ""
    echo "  ── Staging: $ws_name (bare — skipped) ──"
    continue
  fi

  echo ""
  echo "  ── Staging: $ws_name (commit $ws_commit) ──"

  # Use git archive to snapshot the codex repo at the target commit
  mkdir -p "$WS_DIR"
  (cd "$CODEX_REPO" && git archive "$ws_commit" | tar -x -C "$WS_DIR")

  # Copy .semlift/ to a side directory for ontology/docs seeding.
  # Keep .semlift/ IN the published workspace because the API serves spec and
  # view content by reading .semlift/specs/{component_id}.semlift.md and
  # .semlift/views/*.md directly from the repo directory.
  SEMLIFT_DIR="$STAGING_DIR/${ws_name}-semlift"
  if [ -d "$WS_DIR/.semlift" ]; then
    cp -a "$WS_DIR/.semlift" "$SEMLIFT_DIR"
  fi

  # Create .g5/workspace.yaml
  mkdir -p "$WS_DIR/.g5"
  cat > "$WS_DIR/.g5/workspace.yaml" <<WSEOF
workspace_id: "${ws_id}"
remote:
  enabled: true
  api_url: "${API_URL}"
  project_id: null
clone:
  strategy: cow
  fallback_to_full: true
fork:
  strategy: branch
isolation:
  mode: process
cache:
  l1_enabled: true
  l2_enabled: false
docs: null
repos: []
WSEOF

  # Initialize as a git repo
  cd "$WS_DIR"
  git init -q
  git config user.email "demo@g5labs.ai"
  git config user.name "G5 Demo"
  git add -A
  git commit -q -m "Seed: $ws_name ($ws_commit)

${WS_DESCS[$(ws_idx "$ws_name")]}
Workspace ID: ${ws_id}"

  # Capture the staged commit for the seed report.
  REAL_COMMIT=$(git rev-parse HEAD)
  WS_REAL_COMMITS[$(ws_idx "$ws_name")]="$REAL_COMMIT"

  FILE_COUNT=$(find "$WS_DIR" -type f -not -path '*/.git/*' | wc -l | tr -d ' ')
  echo "    ✓ Staged $FILE_COUNT files"

  # Report key directories present at this commit
  [ -d "$SEMLIFT_DIR/specs" ] && echo "      .semlift/specs/  $(find "$SEMLIFT_DIR/specs" -name '*.semlift.md' | wc -l | tr -d ' ') specs"
  [ -d "$SEMLIFT_DIR/views" ] && echo "      .semlift/views/  $(find "$SEMLIFT_DIR/views" -name '*.md' | wc -l | tr -d ' ') views"
  [ -d "$WS_DIR/g5_aux_files" ]   && echo "      g5_aux_files/    $(find "$WS_DIR/g5_aux_files" -name '*.md' | wc -l | tr -d ' ') design docs"
  [ -d "$WS_DIR/codex_src" ]      && echo "      codex_src/       present"
done

# ─── Step 5: Ensure workspace repository membership ───────────────────────────

echo ""
echo "=== Step 5: Ensure workspace repository membership ==="

for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_id="${WS_IDS[$(ws_idx "$ws_name")]:-}"
  repo_id="${WS_REPO_IDS[$(ws_idx "$ws_name")]:-}"
  [ -z "$ws_id" ] && continue

  # Bare workspace — no repo, no branch needed
  if [ -z "$repo_id" ]; then
    echo "  ✓ $ws_name: bare workspace — no branch needed"
    continue
  fi

  psql_exec "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
    VALUES ('${ORG_ID}', '${ws_id}', '${repo_id}', 'code', 'code')
    ON CONFLICT (workspace_id, repo_id) DO NOTHING;" >/dev/null

done

# ─── Step 6: Publish repos to API container volume ───────────────────────────

echo ""
echo "=== Step 6: Publish to API repos volume ==="

for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_id="${WS_IDS[$(ws_idx "$ws_name")]:-}"
  repo_id="${WS_REPO_IDS[$(ws_idx "$ws_name")]:-}"
  [ -z "$ws_id" ] && continue

  WS_DIR="$STAGING_DIR/$ws_name"
  [ ! -d "$WS_DIR" ] && continue

  # Each workspace has its own repo_id → own directory under /data/repos/<org-id>/
  REPO_DIR_ID="${repo_id:-$ws_id}"

  echo "  ── Publishing: $ws_name → /data/repos/${ORG_ID}/${REPO_DIR_ID} ──"

  # Copy workspace contents into the API container
  docker exec g5-dev-api mkdir -p "/data/repos/${ORG_ID}/${REPO_DIR_ID}" 2>/dev/null || true
  docker cp "$WS_DIR/." "g5-dev-api:/data/repos/${ORG_ID}/${REPO_DIR_ID}/" 2>/dev/null

  # Also create a bare clone for git operations
  BARE_DIR="$STAGING_DIR/${ws_name}.bare"
  (cd "$WS_DIR" && git clone --bare . "$BARE_DIR" 2>/dev/null)
  docker exec g5-dev-api mkdir -p "/data/repos/${ORG_ID}/${REPO_DIR_ID}.bare" 2>/dev/null || true
  docker cp "$BARE_DIR/." "g5-dev-api:/data/repos/${ORG_ID}/${REPO_DIR_ID}.bare/" 2>/dev/null
  rm -rf "$BARE_DIR"

  [ -z "$repo_id" ] || seed_capture_fixture_parent "$ws_id" "$ws_name"
  echo "    ✓ Published"
done

# ─── Step 8: Seed doc rows + views ───────────────────────────────────────────

echo ""
echo "=== Step 8: Seed workspace docs + views ==="

for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_id="${WS_IDS[$(ws_idx "$ws_name")]:-}"
  repo_id="${WS_REPO_IDS[$(ws_idx "$ws_name")]:-}"
  [ -z "$ws_id" ] && continue

  WS_DIR="$STAGING_DIR/$ws_name"
  DOC_SEED_COUNT=0
  VIEW_SEED_COUNT=0

  echo ""
  echo "  ── Seeding docs: $ws_name ──"

  # Clear existing docs for this workspace
  docker exec g5-dev-postgres psql -U g5 -d g5_dev -tAc \
    "DELETE FROM docs WHERE workspace_id = '${ws_id}';" >/dev/null 2>&1 || true

  # ── Seed design docs from g5_aux_files ──
  # Design docs get the #!internal-workspace- prefix so they appear in the
  # "Project Docs" sidebar section, keeping "Views" clean for .semlift views.
  AUX_DIR="$WS_DIR/g5_aux_files"
  if [ -d "$AUX_DIR" ]; then
    for md_file in "$AUX_DIR"/*.md; do
      [ -f "$md_file" ] || continue
      md_name=$(basename "$md_file" .md)
      md_relpath="#!internal-workspace-docs/${md_file#$WS_DIR/}"
      md_content=$(cat "$md_file")

      DOC_BODY=$(jq -n \
        --arg name "$md_name" \
        --arg docType "design" \
        --arg filePath "$md_relpath" \
        --arg content "$md_content" \
        '{"name": $name, "docType": $docType, "filePath": $filePath, "content": $content}')

      DOC_RESP=$(api_authed_post "/api/v1/workspaces/${ws_id}/docs" "$DOC_BODY" 2>/dev/null || echo "")
      [ -n "$DOC_RESP" ] && DOC_SEED_COUNT=$((DOC_SEED_COUNT + 1))
    done
  fi

  # ── Seed view docs from .semlift/views ──
  # Views are docs WITHOUT the #!internal-workspace- prefix, so they appear in
  # the "Views" sidebar section. Using docType "view" (migration 022).
  SEMLIFT_DIR="$STAGING_DIR/${ws_name}-semlift"
  VIEWS_DIR="$SEMLIFT_DIR/views"
  if [ -d "$VIEWS_DIR" ]; then
    for view_file in "$VIEWS_DIR"/*.md; do
      [ -f "$view_file" ] || continue
      view_name=$(basename "$view_file" .md)
      view_relpath=".semlift/views/$(basename "$view_file")"
      view_content=$(cat "$view_file")

      DOC_BODY=$(jq -n \
        --arg name "$view_name" \
        --arg docType "view" \
        --arg filePath "$view_relpath" \
        --arg content "$view_content" \
        '{"name": $name, "docType": $docType, "filePath": $filePath, "content": $content}')

      DOC_RESP=$(api_authed_post "/api/v1/workspaces/${ws_id}/docs" "$DOC_BODY" 2>/dev/null || echo "")
      if [ -n "$DOC_RESP" ]; then
        VIEW_SEED_COUNT=$((VIEW_SEED_COUNT + 1))
      fi
    done
  fi

  echo "    ✓ Seeded $DOC_SEED_COUNT design docs + $VIEW_SEED_COUNT views"

  # ── Set doc sync state per workspace narrative ──

  # Workspace 2 (codex-uplifted): views created, need downlift
  # Views have been uplifted but code hasn't been updated to match them yet.
  # This sets up the "trigger downlift" demo action.
  if [ "$ws_name" = "codex-uplifted" ]; then
    # Design docs are fully synced
    docker exec g5-dev-postgres psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -c \
      "UPDATE docs
          SET last_uplifted_at = content_updated_at,
              last_downlifted_at = content_updated_at,
              updated_at = now()
       WHERE workspace_id = '${ws_id}'
         AND doc_type = 'design';" >/dev/null 2>&1

    # Views are uplifted but NOT downlifted (code doesn't match yet)
    docker exec g5-dev-postgres psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -c \
      "UPDATE docs
          SET last_uplifted_at = content_updated_at,
              last_downlifted_at = NULL,
              updated_at = now()
       WHERE workspace_id = '${ws_id}'
         AND doc_type = 'view';" >/dev/null 2>&1
    echo "    ✓ Views marked as needing downlift (uplifted but not downlifted)"
  fi

  # Workspace 3 (codex-refactored): all fully synced (post-downlift)
  if [ "$ws_name" = "codex-refactored" ]; then
    docker exec g5-dev-postgres psql -U g5 -d g5_dev -v ON_ERROR_STOP=1 -c \
      "UPDATE docs
          SET last_uplifted_at = content_updated_at,
              last_downlifted_at = content_updated_at,
              updated_at = now()
       WHERE workspace_id = '${ws_id}';" >/dev/null 2>&1
    echo "    ✓ All docs marked as fully synced (post-downlift)"
  fi
done

# ─── Step 9: (removed — no pre-seeded job; downlift triggered from UI) ───────

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo ""
echo "════════════════════════════════════════════════════════════"
echo "  Multi-Workspace Demo Ready!"
echo "════════════════════════════════════════════════════════════"
echo ""
echo "  Three workspaces seeded showing the uplift → downlift flow:"
echo ""
echo "  ┌─────────────────────────────────────────────────────────┐"
printf "  │ 1. codex-empty      → Bare workspace, no repos         │\n"
printf "  │    ID: %-46s │\n" "${WS_IDS[0]:-N/A}"
echo "  │    Demo: Add remote repo, trigger uplift               │"
echo "  ├─────────────────────────────────────────────────────────┤"
printf "  │ 2. codex-uplifted   → Uplifted with specs + views      │\n"
printf "  │    ID: %-46s │\n" "${WS_IDS[1]:-N/A}"
printf "  │    Repo: %-44s │\n" "${WS_REPO_IDS[1]:-N/A}"
echo "  │    Commit: 7373aff (specs + arch/test views)           │"
echo "  │    Views:                                              │"
echo "  │      - tool_system_refactor.arch_view.md               │"
echo "  │      - tool_system_refactor.test_view.md               │"
echo "  │    Demo: Trigger downlift from views                   │"
echo "  ├─────────────────────────────────────────────────────────┤"
printf "  │ 3. codex-refactored → Post-downlift refactored code    │\n"
printf "  │    ID: %-46s │\n" "${WS_IDS[2]:-N/A}"
printf "  │    Repo: %-44s │\n" "${WS_REPO_IDS[2]:-N/A}"
echo "  │    Commit: 0934045 (new tools/ module + comparison)   │"
echo "  │    Demo: Show the result of downlift                   │"
echo "  └─────────────────────────────────────────────────────────┘"
echo ""
echo "  Demo Flow:"
echo "    1 → Show bare workspace, add codex repo, watch uplift"
echo "    2 → Show uplifted specs, open views, trigger downlift"
echo "    3 → Show resulting code with new tools/ module"
echo ""
echo "  Org: $ORG_SLUG ($ORG_ID)"
echo "  App: http://localhost:3000"
echo ""
echo "  Environment variables:"
echo "    export G5_DEMO_ORG_ID=${ORG_ID}"
for ws_name in codex-empty codex-uplifted codex-refactored; do
  ws_id="${WS_IDS[$(ws_idx "$ws_name")]:-}"
  var_name=$(echo "$ws_name" | tr '-' '_' | tr '[:lower:]' '[:upper:]')
  echo "    export G5_DEMO_${var_name}_WS_ID=${ws_id}"
done
