#!/usr/bin/env bash
# =============================================================================
# seed-demo.sh — Create an isolated demo workspace for E2E demos
# =============================================================================
# Sets up:
#   1. Database entities (org, user, workspace) via g5-api
#   2. Local workspace directory with .g5/workspace.yaml
#   3. Design documents in g5 docs folder
#   4. Generation ontology (specs) + source code
#   5. Git repo initialized with initial commit
#
# Usage:
#   ./scripts/seed-demo.sh [api-url] [demo-dir]
#   Default API: http://localhost:${G5_PORT_API:-7500}
#   Default dir: ./demo-workspace
#
# Repo publish backend (G5_SEED_WORKSPACE_BACKEND):
#   native    (default; `g5d-push` is accepted as an alias) — for every
#               workspace branch of the docs repo, clone that branch from the
#               API git mirror, replace its tree with the demo content and
#               commit. The mirror admits no push to any of these branches
#               (Base's default branch is protected, and the demo and
#               e2e-test workspaces the test-token cascade forks are stopped
#               while a branch push needs a running incarnation), so every
#               cut is installed into the API repo volume the way fixtures
#               are: objects fetched in place, ref compare-and-swapped
#               against the head the mirror advertised. Every workspace's
#               committed files are read directly through the API
#               (scripts/lib/native-publish.sh).
#   legacy-db — tar-pipe the working tree into the api_repos volume and
#               materialize workspace member refs in Git. Diagnostics only.
#
# Requires: curl, jq, git, rsync, docker (API repo volume access), running
# g5-api with G5_E2E_AUTH=true
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/lib/native-publish.sh"

API_URL="${1:-http://localhost:${G5_PORT_API:-7500}}"
DEMO_DIR="${2:-$REPO_ROOT/demo-workspace}"

G5_SEED_WORKSPACE_BACKEND="${G5_SEED_WORKSPACE_BACKEND:-native}"
case "$G5_SEED_WORKSPACE_BACKEND" in
  native|g5d-push) G5_SEED_WORKSPACE_BACKEND=native ;;
  legacy-db) ;;
  *)
    echo "✗ Unsupported G5_SEED_WORKSPACE_BACKEND='${G5_SEED_WORKSPACE_BACKEND}' (expected native or legacy-db)" >&2
    exit 1
    ;;
esac

# ─── Auto-detect API URL ─────────────────────────────────────────────────────
# In Docker-in-Docker (e.g. devcontainers), the configured host port may not be
# reachable even though the container port is published. Fall back to container IP.
API_PORT="${G5_PORT_API:-7500}"
if [ "$API_URL" = "auto" ] || ! curl -sf "${API_URL}/healthz" > /dev/null 2>&1; then
  # Try localhost first
  if curl -sf "http://localhost:${API_PORT}/healthz" > /dev/null 2>&1; then
    API_URL="http://localhost:${API_PORT}"
  else
    # Fall back to container IP
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

echo "=== G5 Demo Workspace Setup ==="
echo "  API:  $API_URL"
echo "  Dir:  $DEMO_DIR"
echo ""

# ─── Helper: authenticated API call ──────────────────────────────────────────

api_post() {
  local path="$1"
  local data="$2"
  curl -sf "${API_URL}${path}" \
    -H "Content-Type: application/json" \
    -d "$data"
}

api_get() {
  local path="$1"
  curl -sf "${API_URL}${path}"
}

# ─── Wait for API to be ready ────────────────────────────────────────────────

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

# ─── Step 1: Get E2E auth token ──────────────────────────────────────────────

echo ""
echo "=== Step 1: Authenticate ==="
AUTH_RESP=$(api_post "/api/v1/auth/test-token" '{"email": "demo@g5labs.ai"}' 2>/dev/null || echo "")
if [ -n "$AUTH_RESP" ]; then
  # AuthTokensDTO uses camelCase: {"data": {"jwt": "...", "refreshToken": "...", ...}}
  AUTH_TOKEN=$(echo "$AUTH_RESP" | jq -r '.data.jwt // .jwt // empty')
  if [ -n "$AUTH_TOKEN" ]; then
    echo "  ✓ Got auth token"
  else
    echo "  ✗ No auth token in response"
    echo "  Response was: $AUTH_RESP"
    exit 1
  fi
else
  echo "  ✗ E2E auth is required for demo seeding"
  exit 1
fi

# Helper with auth token so route handlers see the real user_id.
api_authed_post() {
  local path="$1"
  local data="$2"
  local headers=(-H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf "${API_URL}${path}" "${headers[@]}" -d "$data"
}

api_authed_get() {
  local path="$1"
  local headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf "${API_URL}${path}" "${headers[@]}"
}

api_authed_put() {
  local path="$1"
  local data="$2"
  local headers=(-H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}")
  curl -sf -X PUT "${API_URL}${path}" "${headers[@]}" -d "$data"
}

# ─── Helpers: native publish backend ─────────────────────────────────────────

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "✗ Required command not found: $1" >&2
    exit 1
  fi
}

psql_exec() {
  docker exec g5-dev-postgres psql -U g5 -d g5_dev -tAc "$1"
}

# The g5-api container that mounts the repo volume at /data/repos.
API_CONTAINER="${G5_API_CONTAINER:-g5-dev-api}"

wait_for_materialized_branch_head() {
  local workspace_id="$1"
  local label="$2"
  local expected_head="$3"
  local head=""

  for _attempt in $(seq 1 20); do
    head="$(api_authed_get "/api/v1/workspaces/${workspace_id}/branches" | jq -r --arg repo "$DOCS_REPO_ID" '.data[] | select(.repoId == $repo) | .headCommit')"
    if [ "$head" = "$expected_head" ]; then
      return 0
    fi
    sleep 1
  done

  echo "  ✗ ${label} branch did not materialize at ${expected_head} after publish (head=${head:-missing})" >&2
  exit 1
}

# ─── Step 2: Seed database entities ──────────────────────────────────────────

echo ""
echo "=== Step 2: Seed database ==="

# The test-token endpoint now creates the full cascade:
#   user → org ("G5 Labs" / "g5labs") → workspaces ("base" + "demo") →
#   docs repo → workspace membership linking repo to Base.
# We just need to fetch the IDs.

# Fetch the user's orgs
ORGS_RESP=$(api_authed_get "/api/v1/orgs" 2>/dev/null || echo "")
ORG_ID=$(echo "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null)

if [ -z "$ORG_ID" ]; then
  echo "  ✗ No orgs found for demo user"
  echo "  Response: $ORGS_RESP"
  exit 1
fi

ORG_SLUG=$(echo "$ORGS_RESP" | jq -r '.data[0].orgSlug // empty' 2>/dev/null)
echo "  ✓ Org: $ORG_SLUG ($ORG_ID)"

# Get the base workspace
WS_RESP=$(api_authed_get "/api/v1/orgs/${ORG_ID}/workspaces" 2>/dev/null || echo "")
WS_ID=$(echo "$WS_RESP" | jq -r '.data[] | select(.name == "base" or .name == "Base") | .id // empty' 2>/dev/null | head -1)
[ -z "$WS_ID" ] && WS_ID=$(echo "$WS_RESP" | jq -r '.data[0].id // empty' 2>/dev/null)

if [ -z "$WS_ID" ]; then
  echo "  ✗ No workspaces found"
  exit 1
fi
echo "  ✓ Workspace: $WS_ID"

# Get the docs repo
REPOS_RESP=$(api_authed_get "/api/v1/orgs/${ORG_ID}/repos" 2>/dev/null || echo "")
DOCS_REPO_ID=$(echo "$REPOS_RESP" | jq -r '.data[] | select(.slug == "docs") | .id // empty' 2>/dev/null | head -1)
if [ -n "$DOCS_REPO_ID" ]; then
  echo "  ✓ Docs Repo: $DOCS_REPO_ID"
else
  echo "  ⚠ No docs repo — file tree / doc reads will be empty"
fi

# Check workspace branches
BRANCHES_RESP=$(api_authed_get "/api/v1/workspaces/${WS_ID}/branches" 2>/dev/null || echo "")
BRANCH_COUNT=$(echo "$BRANCHES_RESP" | jq -r '.data | length' 2>/dev/null || echo "0")
echo "  ✓ Branches: $BRANCH_COUNT"

# ─── Step 3: Create local workspace directory ────────────────────────────────

echo ""
echo "=== Step 3: Create local workspace ==="

# Clean up if exists
if [ -d "$DEMO_DIR" ]; then
  echo "  Removing existing demo workspace..."
  # Fix ownership if docker cp created root-owned files
  chmod -R u+w "$DEMO_DIR" 2>/dev/null || true
  rm -rf "$DEMO_DIR" 2>/dev/null || true
  # If still present (e.g. root-owned from docker), use sudo
  if [ -d "$DEMO_DIR" ]; then
    sudo rm -rf "$DEMO_DIR" 2>/dev/null || true
  fi
fi

mkdir -p "$DEMO_DIR/.g5"

# Write workspace.yaml with the API-assigned UUID
cat > "$DEMO_DIR/.g5/workspace.yaml" <<EOF
workspace_id: "${WS_ID}"
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
EOF
echo "  ✓ .g5/workspace.yaml (workspace_id: ${WS_ID})"

# ─── Step 4: Copy design documents ───────────────────────────────────────────

echo ""
echo "=== Step 4: Copy design documents ==="

DOCS_DIR="$DEMO_DIR/docs/reference/designs"
mkdir -p "$DOCS_DIR"
cp -r "$REPO_ROOT/docs/reference/designs/"*.md "$DOCS_DIR/" 2>/dev/null || true
DOC_COUNT=$(ls -1 "$DOCS_DIR"/*.md 2>/dev/null | wc -l)
echo "  ✓ Copied $DOC_COUNT design documents to docs/reference/designs/"

# Create internal workspace project docs under the #!internal-workspace- prefix.
# These appear in the "Project Docs" sidebar section (not "Views").
WS_SLUG="demo"
INTERNAL_DIR="$DEMO_DIR/#!internal-workspace-${WS_SLUG}"
mkdir -p "$INTERNAL_DIR"

cat > "$INTERNAL_DIR/00-user-stories.md" <<'IWDOC'
---
doc_type: project
section: user-stories
projects:
  - id: frontmatter-dependents
    name: Frontmatter Dependents
linked_nodes:
  - frontend/src/components/editor/SpecEditor.tsx
owner: product
updated_at: "2026-02-18"
---

# User Stories

## Frontmatter Dependents

### Business summary
- We want every project document to show what depends on it.
- A business user should immediately see "if this changes, what else is affected?"
- This reduces guesswork in planning and release decisions.

### Primary users
- Product leaders reviewing delivery risk
- Engineering managers planning scope and sequencing
- Engineers validating blast radius before implementation

### User stories
1. As a product leader, I want to open one doc and see dependent files/components so I can explain impact in one sentence.
2. As an engineering manager, I want dependency visibility in the same workspace flow so planning meetings are faster.
3. As an engineer, I want dependency context near the doc so I can make safer changes with less rework.

### Example: expected frontmatter render
```yaml
---
title: Payment Service Rollout
dependents:
  - Checkout UI
  - Billing Worker
  - Revenue Dashboard
---
```

Expected UI render:
- **Dependents**
- Checkout UI
- Billing Worker
- Revenue Dashboard

### Sample chart (dependency map)
```mermaid
graph TD
  A["Payment Service Rollout"] --> B["Checkout UI"]
  A --> C["Billing Worker"]
  A --> D["Revenue Dashboard"]
```

### Acceptance criteria
- Dependents render directly from doc frontmatter.
- Dependents are readable by non-technical users (plain labels, not internal IDs).
- A user can quickly answer: "what breaks if this doc changes?"
IWDOC

cat > "$INTERNAL_DIR/01-design.md" <<'IWDOC'
---
doc_type: project
section: design
projects:
  - id: frontmatter-dependents
    name: Frontmatter Dependents
linked_nodes:
  - frontend/src/components/editor/SpecEditor.tsx
owner: design-engineering
updated_at: "2026-02-18"
---

# Design

## Frontmatter Dependents

### User-facing behavior
- Each project doc includes a "Dependents" block in frontmatter.
- The UI renders those dependents in a simple list directly under project context.
- Labels prioritize business readability (feature names, capabilities, or service names).

### Data contract
- Source of truth: doc frontmatter.
- Render rule: if `dependents` exists, show ordered list; if empty, show "No dependents declared."
- Avoid technical-only output such as raw IDs where a name can be shown.

### Success criteria
- A first-time user understands dependency impact in under 10 seconds.
- Demo presenter can explain change impact without opening code.
IWDOC

cat > "$INTERNAL_DIR/03-status.md" <<'IWDOC'
---
doc_type: project
section: status
projects:
  - id: frontmatter-dependents
    name: Frontmatter Dependents
linked_nodes:
  - frontend/src/components/editor/SpecEditor.tsx
owner: engineering
updated_at: "2026-02-18"
---

# Status

## Frontmatter Dependents

### Milestones
- [ ] Finalize frontmatter schema for `dependents`
- [ ] Render dependents in Project Docs UI
- [ ] Validate copy for business readability
- [ ] Demo rehearsal with leadership narrative

### Risks
- Inconsistent frontmatter quality across docs can reduce trust.
- Overly technical labels can make the feature harder to understand.
IWDOC

cat > "$INTERNAL_DIR/04-security.md" <<'IWDOC'
---
doc_type: project
section: security
projects:
  - id: frontmatter-dependents
    name: Frontmatter Dependents
linked_nodes:
  - frontend/src/components/editor/SpecEditor.tsx
owner: security
updated_at: "2026-02-18"
---

# Security

## Frontmatter Dependents

### Security considerations
- Dependents shown in UI must respect workspace access rules.
- The feature must not leak hidden/internal paths to unauthorized users.
- Dependents rendering should be display-only unless edit permissions are granted.

### Controls
- Enforce existing workspace auth checks before serving doc data.
- Sanitize displayed labels to avoid exposing internal-only naming.
- Keep an audit trail for frontmatter edits that change dependency visibility.
IWDOC

cat > "$INTERNAL_DIR/05-operations.md" <<'IWDOC'
---
doc_type: project
section: operations
projects:
  - id: frontmatter-dependents
    name: Frontmatter Dependents
linked_nodes:
  - frontend/src/components/editor/SpecEditor.tsx
owner: operations
updated_at: "2026-02-18"
---

# Operations

## Frontmatter Dependents

### Rollout plan
1. Seed project docs with clear business-facing dependent examples.
2. Verify dependents render in Project Docs after `just demo-start`.
3. Run demo script: open doc -> show dependents -> explain impact.

### Runbook checks
- Project Docs open without hidden/internal workspace marker text.
- Dependents text is understandable by a non-technical stakeholder.
- Presenter can explain impact narrative in under one minute.
IWDOC

INTERNAL_DOC_COUNT=$(find "$INTERNAL_DIR" -name '*.md' | wc -l | tr -d ' ')
echo "  ✓ Created $INTERNAL_DOC_COUNT internal workspace docs in #!internal-workspace-${WS_SLUG}/"

# ─── Step 5: Copy ontology (specs) ───────────────────────────────────────────

echo ""
echo "=== Step 5: Copy ontology ==="

if [ -d "$REPO_ROOT/.semlift/specs" ]; then
  mkdir -p "$DEMO_DIR/.semlift"
  cp -r "$REPO_ROOT/.semlift/specs" "$DEMO_DIR/.semlift/specs"
  SPEC_COUNT=$(find "$DEMO_DIR/.semlift/specs" -name '*.md' | wc -l)
  echo "  ✓ Copied $SPEC_COUNT spec files to .semlift/specs/"
else
  echo "  ⚠ No .semlift/specs found — skipping"
fi

# Copy semlift.yaml config
if [ -f "$REPO_ROOT/semlift.yaml" ]; then
  cp "$REPO_ROOT/semlift.yaml" "$DEMO_DIR/semlift.yaml"
  echo "  ✓ Copied semlift.yaml"
fi

# ─── Step 6: Copy generation source code ─────────────────────────────────────

echo ""
echo "=== Step 6: Copy source code ==="

# Copy Rust crates (source only, not target/). Keep this list aligned with the
# workspace's split implementation owners so demo navigation and grounding see
# the same physical sources as production.
RUST_CRATES=(
  g5-contracts g5-auth g5-storage
  g5-semantic g5-scm g5-internal-api
  g5-api-foundation g5-api-data g5-api-core g5-api-routes g5-api
  g5d-foundation g5d-isolation g5d-agent g5d-host g5d-dispatch g5d
)
for crate in "${RUST_CRATES[@]}"; do
  if [ -d "$REPO_ROOT/backend/$crate/src" ]; then
    mkdir -p "$DEMO_DIR/backend/$crate/src"
    cp -r "$REPO_ROOT/backend/$crate/src/"* "$DEMO_DIR/backend/$crate/src/"
    # Copy Cargo.toml
    [ -f "$REPO_ROOT/backend/$crate/Cargo.toml" ] && cp "$REPO_ROOT/backend/$crate/Cargo.toml" "$DEMO_DIR/backend/$crate/"
  fi
done
echo "  ✓ Copied Rust crates: ${RUST_CRATES[*]}"

# Copy root Cargo files
cp "$REPO_ROOT/Cargo.toml" "$DEMO_DIR/" 2>/dev/null || true
cp "$REPO_ROOT/Cargo.lock" "$DEMO_DIR/" 2>/dev/null || true

# Copy client source
if [ -d "$REPO_ROOT/frontend/src" ]; then
  mkdir -p "$DEMO_DIR/frontend/src"
  cp -r "$REPO_ROOT/frontend/src/"* "$DEMO_DIR/frontend/src/"
  cp "$REPO_ROOT/frontend/package.json" "$DEMO_DIR/frontend/" 2>/dev/null || true
  cp "$REPO_ROOT/frontend/tsconfig.json" "$DEMO_DIR/frontend/" 2>/dev/null || true
  cp "$REPO_ROOT/frontend/vite.config.ts" "$DEMO_DIR/frontend/" 2>/dev/null || true
  echo "  ✓ Copied client source"
fi

# Copy .sqlx offline cache
if [ -d "$REPO_ROOT/.sqlx" ]; then
  cp -r "$REPO_ROOT/.sqlx" "$DEMO_DIR/.sqlx"
  echo "  ✓ Copied .sqlx offline query cache"
fi

# ─── Step 7: Initialize git repo ─────────────────────────────────────────────

echo ""
echo "=== Step 7: Initialize git repo ==="

cd "$DEMO_DIR"
git init -q
git config user.email "demo@g5labs.ai"
git config user.name "G5 Demo"
git add -A
git commit -q -m "Initial commit: G5 generation system demo workspace

Workspace ID: ${WS_ID}
Org ID: ${ORG_ID}
Source: self-hosted (copied from the repository root)"

# Captured for the legacy-db backend's branch materialization update.
HEAD_COMMIT=$(git rev-parse HEAD)
echo "  ✓ Git repo initialized with initial commit"

# Initialize workspace-meta
mkdir -p ".g5/workspace-meta"
cd ".g5/workspace-meta"
git init -q
cp ../workspace.yaml .
git add -A
git config user.email "demo@g5labs.ai"
git config user.name "G5 Demo"
git commit -q -m "g5d init: workspace ${WS_ID}"
cd "$DEMO_DIR"
echo "  ✓ Workspace-meta initialized"

# ─── Step 7.5: Publish repo content ─────────────────────────────────────────

echo ""
echo "=== Step 7.5: Publish repo (backend: ${G5_SEED_WORKSPACE_BACKEND}) ==="

# Publish the seeded docs repo so every non-job workspace branch row on it
# (base "main" plus the "demo" and "e2e-test" forks the test-token cascade
# creates with real refs) carries the demo tree. Without that, task
# assignment and worker checkout see the bootstrap stub instead of the demo.

# The mirror admits none of these pushes: Base's default branch is protected
# (git_policy.rs) and base workspaces are read-only (git.rs admission), and
# the demo and e2e-test workspaces the test-token cascade forks are stopped
# while a branch push needs a running incarnation with a live lease. So every
# docs cut is installed the way fixtures are: the new objects are fetched
# into the API repo volume and the ref is captured in place, compare-and-
# swapped against the head the mirror advertised when the branch was cloned.
# Base's default branch is also the mirror's HEAD; a child branch never is.
seed_demo_install_docs_branch() {
  local label="$1" clone_dir="$2" branch="$3" old_head="$4" new_head="$5" bundle set_head=0
  if [ "$label" = "Base" ]; then
    set_head=1
  fi
  if [ "$old_head" = "$new_head" ]; then
    echo "  ✓ docs/${branch} (${label}) already at ${new_head:0:12}; nothing to install"
    return 0
  fi
  if ! docker exec "$API_CONTAINER" test -d "/data/repos/${ORG_ID}/${DOCS_REPO_ID}" 2>/dev/null; then
    echo "  ✗ The demo docs can only be installed through the API repo volume, and container '${API_CONTAINER}' with /data/repos/${ORG_ID}/${DOCS_REPO_ID} is not reachable from here." >&2
    echo "    The mirror refuses these pushes (protected Base branch, stopped seed workspaces), so this seed cannot publish to a remote stack." >&2
    exit 1
  fi
  bundle="$(mktemp "${TMPDIR:-/tmp}/g5-demo-base.XXXXXX")"
  # A failed fetch or a stale compare-and-swap exits the seed; neither
  # bundle may outlive it, on the host or in the container.
  trap 'rm -f "$bundle"' EXIT
  git -C "$clone_dir" bundle create --quiet "$bundle" "${old_head}..refs/heads/${branch}"
  docker exec -i "$API_CONTAINER" sh -eu -c '
    repo="$1"
    bundle="$(mktemp /tmp/g5-demo-base.XXXXXX)"
    trap '"'"'rm -f "$bundle"'"'"' EXIT
    cat > "$bundle"
    git -c safe.directory="*" -C "$repo" fetch --quiet "$bundle" "refs/heads/$2"
    git -c safe.directory="*" -C "$repo" update-ref "refs/heads/$2" "$3" "$4"
    if [ "$5" = "1" ]; then
      git -c safe.directory="*" -C "$repo" symbolic-ref HEAD "refs/heads/$2"
    fi
  ' sh "/data/repos/${ORG_ID}/${DOCS_REPO_ID}" "$branch" "$new_head" "$old_head" "$set_head" <"$bundle"
  rm -f "$bundle"
  trap - EXIT
  echo "  ✓ docs/${branch} (${label}): installed ${new_head:0:12} into the API repo volume (was ${old_head:0:12})"
}

# native backend: every workspace branch row on the docs repo receives the
# same demo tree. Each branch is cloned from the API mirror (the fork service
# already created it), its tree replaced with the demo content and committed,
# and the cut installed through the repo volume (seed_demo_install_docs_branch
# says why no branch is pushed). The API owns materialization and every
# workspace's published cut is checked afterwards through the API.
seed_publish_native() {
  require_command rsync

  local seed_ws_dir="$REPO_ROOT/.tmp/seed-workspaces/demo-docs"
  case "$seed_ws_dir" in
    "/"|"$REPO_ROOT"|"$DEMO_DIR")
      echo "  ✗ Refusing to replace unsafe seed workspace path: $seed_ws_dir" >&2
      exit 1
      ;;
  esac
  rm -rf "$seed_ws_dir"
  mkdir -p "$seed_ws_dir"
  native_publish_configure "$API_URL" "$AUTH_TOKEN"

  # Base workspace first so the repo's default branch lands before variants.
  local branch_rows
  branch_rows="$(psql_exec "
    SELECT m.workspace_id::text || '|' || CASE WHEN w.is_base THEN r.default_branch ELSE w.name END || '|' || w.is_base::text
      FROM workspace_repos m JOIN workspaces w ON w.id = m.workspace_id JOIN repos r ON r.id = m.repo_id
     WHERE m.repo_id = '${DOCS_REPO_ID}' AND w.deleted_at IS NULL
     ORDER BY w.is_base DESC, w.name;")"
  if [ -z "$branch_rows" ]; then
    echo "  ✗ No workspace branch rows found for docs repo ${DOCS_REPO_ID}" >&2
    exit 1
  fi

  # `published_rows` keeps ws|branch|head for the materialization wait and
  # branch-head verification after the installs.
  # Rows arrive on fd 3 so git/curl inside the loop cannot consume them.
  local ws_id branch_name is_base clone_dir old_head new_head
  local published_rows=""
  while IFS='|' read -r -u 3 ws_id branch_name is_base; do
    [ -z "$ws_id" ] && continue
    clone_dir="$seed_ws_dir/${branch_name//[^A-Za-z0-9._-]/-}-${ws_id}"

    echo "  … cloning docs branch ${branch_name} for workspace ${ws_id}"
    if ! native_publish_git clone --quiet --branch "$branch_name" \
      "${API_URL}/git/${DOCS_REPO_ID}" "$clone_dir"; then
      echo "  ✗ Could not clone docs branch ${branch_name} from the mirror (it must already exist; workspace branch creation is reserved for the fork service)" >&2
      exit 1
    fi
    old_head="$(git -C "$clone_dir" rev-parse HEAD)"
    find "$clone_dir" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
    rsync -a --exclude '.git/' "$DEMO_DIR"/ "$clone_dir"/
    git -C "$clone_dir" config user.email "demo@g5labs.ai"
    git -C "$clone_dir" config user.name "G5 Demo"
    git -C "$clone_dir" config gc.auto 0
    git -C "$clone_dir" add -A
    if ! git -C "$clone_dir" diff --cached --quiet; then
      git -C "$clone_dir" commit -q -m "seed: demo workspace content (${branch_name})"
    fi
    new_head="$(git -C "$clone_dir" rev-parse HEAD)"

    if [ "$is_base" = "true" ] || [ "$is_base" = "t" ]; then
      seed_demo_install_docs_branch "Base" "$clone_dir" "$branch_name" "$old_head" "$new_head"
    else
      seed_demo_install_docs_branch "workspace ${ws_id}" "$clone_dir" "$branch_name" "$old_head" "$new_head"
    fi
    published_rows="${published_rows}${ws_id}|${branch_name}|${new_head}"$'\n'
  done 3<<< "$branch_rows"

  local pushed_head
  while IFS='|' read -r -u 3 ws_id branch_name pushed_head; do
    [ -z "$ws_id" ] && continue
    wait_for_materialized_branch_head "$ws_id" "$branch_name" "$pushed_head"
    echo "  ✓ Branch ${branch_name} materialized at ${pushed_head}"
  done 3<<< "$published_rows"
}

# legacy-db backend: tar-pipe the working tree into the api_repos volume and
# materialize workspace member refs in Git. The g5-api container reads
# file content from {G5_STORAGE_BASE_DIR}/{org_id}/{repo_id}/ on a Docker-managed named
# volume mounted at /data/repos/. tar-pipe instead of `docker cp` because
# `docker cp` on macOS Docker Desktop can silently produce 0-byte files when
# copying large directory trees into named volumes (a known VirtioFS /
# gRPC-FUSE reliability issue).
seed_publish_legacy_db() {
  local REPO_DEST="/data/repos/${ORG_ID}/${DOCS_REPO_ID}"

  # Create a git branch for every product workspace branch row on the docs
  # repo (e.g. "demo", "e2e-test"). Worker materialization runs
  # `git checkout <branch_name>` and fails with "pathspec '<name>' did not
  # match any file(s) known to git" when the ref is missing. Created before
  # the bare clone so the origin inherits the refs. Job branches are
  # excluded — the API forks those at job time.
  local ws_branch_names ws_branch created_refs=0
  ws_branch_names="$(psql_exec "
    SELECT DISTINCT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END
    FROM workspace_repos m JOIN workspaces w ON w.id = m.workspace_id JOIN repos r ON r.id = m.repo_id
    WHERE m.repo_id = '${DOCS_REPO_ID}' AND w.deleted_at IS NULL;")"
  while IFS= read -r ws_branch; do
    [ -z "$ws_branch" ] && continue
    if ! git -C "$DEMO_DIR" show-ref --verify --quiet "refs/heads/${ws_branch}"; then
      git -C "$DEMO_DIR" branch "$ws_branch" "$HEAD_COMMIT"
      created_refs=$((created_refs + 1))
    fi
  done <<< "$ws_branch_names"
  echo "  ✓ Created ${created_refs} git ref(s) matching workspace branch rows"

  # Create a bare clone so the API can access the materialized origin.
  # The remote URL uses the container-internal path; it resolves once both
  # the working tree and the bare repo are copied into the API volume.
  local BARE_DIR="/tmp/g5-demo-origin-${DOCS_REPO_ID}.bare"
  rm -rf "$BARE_DIR"
  git -C "$DEMO_DIR" clone --bare . "$BARE_DIR" 2>/dev/null
  git -C "$DEMO_DIR" remote add origin "/data/repos/${ORG_ID}/${DOCS_REPO_ID}.bare"

  # Create the target directory inside the API container
  docker exec g5-dev-api mkdir -p "$REPO_DEST" 2>/dev/null || true

  # Copy demo workspace content via tar pipe (reliable across all Docker backends).
  # Use --no-same-owner so extracted files are owned by the running user (root)
  # instead of preserving the macOS host UID (501:staff). Without this, the
  # worker (uid 999) cannot write to the repo directory.
  if tar -C "$DEMO_DIR" -cf - . | docker exec -i g5-dev-api tar --no-same-owner -C "$REPO_DEST" -xf -; then
    echo "  ✓ Copied demo workspace to API volume: $REPO_DEST"
  else
    echo "  ⚠ tar pipe copy failed — retrying with docker cp..."
    docker cp "$DEMO_DIR/." "g5-dev-api:${REPO_DEST}/"
  fi

  # Fix ownership: the api_repos volume is shared with g5d-worker containers
  # which run as g5 (uid 999). Ensure the worker can write to the repo
  # (e.g. create views/ directories via emit_view_artifact).
  docker exec g5-dev-api chmod -R a+rwX "$REPO_DEST" 2>/dev/null || true

  # Verify: check that design docs have content (not 0-byte)
  local EMPTY_COUNT EMPTY_RETRY
  EMPTY_COUNT=$(docker exec g5-dev-api find "$REPO_DEST/designs" -name '*.md' -size 0 2>/dev/null | wc -l | tr -d ' ')
  if [ "${EMPTY_COUNT:-0}" -gt 0 ]; then
    echo "  ⚠ Found $EMPTY_COUNT empty .md files — retrying copy..."
    tar -C "$DEMO_DIR" -cf - . | docker exec -i g5-dev-api tar --no-same-owner -C "$REPO_DEST" -xf -
    docker exec g5-dev-api chmod -R a+rwX "$REPO_DEST" 2>/dev/null || true
    EMPTY_RETRY=$(docker exec g5-dev-api find "$REPO_DEST/designs" -name '*.md' -size 0 2>/dev/null | wc -l | tr -d ' ')
    if [ "${EMPTY_RETRY:-0}" -gt 0 ]; then
      echo "  ✗ Still $EMPTY_RETRY empty files after retry — doc content may be missing"
    else
      echo "  ✓ Retry succeeded — all files have content"
    fi
  fi

  # Copy the bare clone so `git push origin` works inside the container
  if [ -d "$BARE_DIR" ]; then
    docker exec g5-dev-api mkdir -p "/data/repos/${ORG_ID}/${DOCS_REPO_ID}.bare" 2>/dev/null || true
    if docker cp "$BARE_DIR/." "g5-dev-api:/data/repos/${ORG_ID}/${DOCS_REPO_ID}.bare/" 2>/dev/null; then
      echo "  ✓ Copied bare origin to API volume: /data/repos/${ORG_ID}/${DOCS_REPO_ID}.bare"
    else
      echo "  ⚠ bare repo docker cp failed — materialized origin may be unavailable"
    fi
    rm -rf "$BARE_DIR"
  fi


}

if [ -n "$DOCS_REPO_ID" ]; then
  case "$G5_SEED_WORKSPACE_BACKEND" in
    native) seed_publish_native ;;
    legacy-db) seed_publish_legacy_db ;;
  esac
else
  echo "  ⚠ No docs_repo_id — skipping repo publish"
fi

# ─── Step 8.5: Seed doc rows for workspace documents ─────────────────────────

echo ""
echo "=== Step 8.5: Seed workspace docs ==="

# The docs table needs rows so the sidebar shows documents. Each doc
# has a file_path relative to the repo root (e.g. "docs/reference/designs/DESIGN.md" or
# "#!internal-workspace-demo/00-user-stories.md").
# Content is read from git working tree at {repos_base_dir}/{docs_repo_id}/{file_path}.

DOC_SEED_COUNT=0
# Scan markdown docs under both docs/reference/designs/ and top-level internal workspace docs.
DOC_SCAN_DIRS=("$DEMO_DIR/designs")
shopt -s nullglob
for internal_dir in "$DEMO_DIR"/'#!internal-workspace-'*; do
  [ -d "$internal_dir" ] && DOC_SCAN_DIRS+=("$internal_dir")
done
shopt -u nullglob

# Keep doc seeding idempotent across repeated `demo-seed` runs.
DOC_CLEARED=$(docker exec g5-dev-postgres psql -U g5 -d g5_dev -tAc \
  "WITH deleted AS (
     DELETE FROM docs
      WHERE workspace_id = '${WS_ID}'
     RETURNING 1
   )
   SELECT count(*) FROM deleted;" | tr -d '[:space:]')
echo "  ✓ Cleared ${DOC_CLEARED:-0} existing docs rows for workspace"

INTERNAL_PREFIX="#!internal-workspace-${WS_SLUG}/"

while IFS= read -r md_file; do
  [ -f "$md_file" ] || continue
  md_name=$(basename "$md_file" .md)
  md_relpath="${md_file#$DEMO_DIR/}"

  # Determine doc_type from filename
  doc_type="design"
  case "$md_name" in
    *API*|*api*)         doc_type="api" ;;
    *SPEC*|*spec*)       doc_type="spec" ;;
    *GUIDE*|*guide*)     doc_type="guide" ;;
    *ADR*|*adr*)         doc_type="adr" ;;
    *DEVELOPMENT*|*dev*) doc_type="guide" ;;
    *PROMPTS*|*prompt*)  doc_type="note" ;;
  esac

  DOC_BODY=$(jq -n \
    --arg name "$md_name" \
    --arg docType "$doc_type" \
    --arg filePath "$md_relpath" \
    '{"name": $name, "docType": $docType, "filePath": $filePath}')

  DOC_RESP=$(api_authed_post "/api/v1/workspaces/${WS_ID}/docs" "$DOC_BODY" 2>/dev/null || echo "")
  if [ -n "$DOC_RESP" ]; then
    DOC_SEED_COUNT=$((DOC_SEED_COUNT + 1))
  fi
done < <(find "${DOC_SCAN_DIRS[@]}" -name '*.md' -type f 2>/dev/null)

# Keep one "freshly changed" project doc for demo push flows; mark the rest
# as fully synced so the UI and sync actions are deterministic.
FRESH_DOC_PATH="${INTERNAL_PREFIX}00-user-stories.md"
SYNCED_DOCS_COUNT=$(docker exec g5-dev-postgres psql -U g5 -d g5_dev -tAc \
  "WITH updated AS (
     UPDATE docs
        SET last_uplifted_at = content_updated_at,
            last_downlifted_at = content_updated_at,
            updated_at = now()
      WHERE workspace_id = '${WS_ID}'
        AND COALESCE(file_path, '') <> '${FRESH_DOC_PATH}'
     RETURNING 1
   )
   SELECT count(*) FROM updated;" | tr -d '[:space:]')

FRESH_DOC_COUNT=$(docker exec g5-dev-postgres psql -U g5 -d g5_dev -tAc \
  "WITH updated AS (
     UPDATE docs
        SET last_downlifted_at = NULL,
            updated_at = now()
      WHERE workspace_id = '${WS_ID}'
        AND file_path = '${FRESH_DOC_PATH}'
     RETURNING 1
   )
   SELECT count(*) FROM updated;" | tr -d '[:space:]')

echo "  ✓ Seeded $DOC_SEED_COUNT docs"
echo "  ✓ Marked ${SYNCED_DOCS_COUNT:-0} docs as in-sync"
echo "  ✓ Left ${FRESH_DOC_COUNT:-0} fresh doc as changed (${FRESH_DOC_PATH})"

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo "============================================="
echo "  Demo workspace ready!"
echo "============================================="
echo ""
echo "  Directory:    $DEMO_DIR"
echo "  Workspace ID: $WS_ID"
echo "  Org ID:       $ORG_ID"
echo "  API URL:      $API_URL"
echo ""
echo "  Contents:"
du -sh "$DEMO_DIR/designs" "$DEMO_DIR/.semlift/specs" \
  "$DEMO_DIR/backend/g5-api-foundation/src" \
  "$DEMO_DIR/backend/g5-api-data/src" "$DEMO_DIR/backend/g5-api-core/src" \
  "$DEMO_DIR/backend/g5-api-routes/src" "$DEMO_DIR/backend/g5-api/src" \
  "$DEMO_DIR/backend/g5d-foundation/src" \
  "$DEMO_DIR/backend/g5d-isolation/src" \
  "$DEMO_DIR/backend/g5d-agent/src" "$DEMO_DIR/backend/g5d-host/src" \
  "$DEMO_DIR/backend/g5d-dispatch/src" "$DEMO_DIR/backend/g5d/src" \
  "$DEMO_DIR/frontend/src" 2>/dev/null | sed 's/^/    /'
echo ""
echo "  To start the workspace host and this workspace:"
echo "    just worker-host-start ${ORG_SLUG}"
echo "    curl -X POST ${API_URL}/api/v1/workspaces/${WS_ID}/resume"
echo ""
echo "  Environment variables for scripts:"
echo "    export G5_DEMO_WORKSPACE_ID=${WS_ID}"
echo "    export G5_DEMO_ORG_ID=${ORG_ID}"
echo "    export G5_DEMO_DIR=${DEMO_DIR}"
