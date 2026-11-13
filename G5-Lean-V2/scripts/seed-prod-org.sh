#!/usr/bin/env bash
# =============================================================================
# seed-prod-org.sh — Platform seed: provision the operator org for cloud
# environments
# =============================================================================
# Creates or repairs the platform operator organization (kind `operator`, slug
# `operator`) directly in the target cloud environment database
# (dev/staging/prod/QA) and invites the deployment's first operator
# (g5.organization.platform-operator.bootstrap). It creates no tenant org:
# tenant orgs, including G5's own development org, are created afterwards by an
# operator from the console (`/operator/home`) or `POST /api/v1/platform/orgs`.
#
# Prerequisites:
#   - gcloud authenticated (only to read g5-db-password when DB_PASSWORD is unset)
#   - cloud-sql-proxy running locally, or use --proxy flag to auto-start
#   - for USE_K8S_PSQL=1: the GKE cluster selected by K8S_CONTEXT
#
# Usage:
#   # Required for every invocation; there is no default cloud target or admin.
#   export GCP_PROJECT=g5-qa-1ccc GCP_REGION=us-central1
#   export DB_INSTANCE_NAME=g5-lean-qa-postgres DB_NAME=g5_lean_qa
#   export DEFAULT_ADMIN_EMAIL=you@g5labs.ai
#
#   # With cloud-sql-proxy already running on localhost:5432
#   DB_HOST=127.0.0.1 ./scripts/seed-prod-org.sh
#
#   # With auto-proxy (requires cloud-sql-proxy installed)
#   ./scripts/seed-prod-org.sh --proxy
#
#   # With an in-cluster psql client pod (recommended for private-only Cloud SQL)
#   K8S_CONTEXT=gke_g5-qa-1ccc_us-central1_g5-lean-qa \
#     USE_K8S_PSQL=1 DB_HOST=10.x.x.x ./scripts/seed-prod-org.sh
#
#   # Custom connection
#   DB_HOST=10.x.x.x DB_PORT=5432 ./scripts/seed-prod-org.sh
#
# What it does (idempotent; a re-run repairs and changes nothing else):
#   1. Connects to the target Postgres and creates/repairs the one operator org
#      (`Platform operator`, slug `operator`, kind `operator`). A tenant org
#      that already holds the slug makes the insert fail on its unique constraint.
#   2. Ensures the non-login worker service user exists. It is the seed's SQL
#      session actor and the inviter of record, and holds a `member`
#      membership in the operator org solely to own its Base workspace (a live
#      workspace needs an owner-eligible member). It cannot sign in, so it
#      never acts as an operator.
#   3. Ensures the operator org's docs repo, Base workspace, its repository
#      membership and settings exist
#   4. Ensures Base has a code repository member, as the tenant creation
#      cascade gives every Base: the org's internal `code` repo is created and
#      attached unless Base already has a code member, which is kept
#   5. Initialises the docs and code repository mirrors on the API's repo
#      storage (USE_K8S_PSQL=1 only) with a first commit, so the base
#      workspace can be cloned and forked and its children can start on the
#      org default image; existing branches and history are never touched
#   6. Sets the org's default workspace image from the deployment's promoted
#      image catalog (DEFAULT_BASE_IMAGE_TRACK, default g5d-g5dev)
#   7. Gives DEFAULT_ADMIN_EMAIL a pending admin invitation to the operator org
#      unless that person is already a member; re-running restores
#      administrator access after the last operator admin was removed
# Tenant orgs are never read or changed. The seed mints no worker registration
# token and writes no Secret Manager secret.
# =============================================================================
set -euo pipefail

# Validate the complete target before starting clients, fetching secrets, or
# installing cleanup traps. Partial overrides must never fall back to prod.
: "${GCP_PROJECT:?Set GCP_PROJECT explicitly; no default cloud target is allowed}"
: "${DB_INSTANCE_NAME:?Set DB_INSTANCE_NAME explicitly}"
: "${DB_NAME:?Set DB_NAME explicitly}"
: "${DEFAULT_ADMIN_EMAIL?Set DEFAULT_ADMIN_EMAIL explicitly (empty skips the invitation)}"
USE_PROXY="${1:-}"
if [[ $# -gt 1 || ( -n "$USE_PROXY" && "$USE_PROXY" != "--proxy" ) ]]; then
  echo "ERROR: Usage: seed-prod-org.sh [--proxy] (target selected through environment variables)." >&2
  exit 1
fi
USE_K8S_PSQL="${USE_K8S_PSQL:-0}"
if [[ "$USE_K8S_PSQL" != "0" && "$USE_K8S_PSQL" != "1" ]]; then
  echo "ERROR: USE_K8S_PSQL must be 0 or 1." >&2
  exit 1
fi
if [[ "$USE_PROXY" == "--proxy" ]]; then
  if [[ "$USE_K8S_PSQL" == "1" || "${DB_HOST:-127.0.0.1}" != "127.0.0.1" ]]; then
    echo "ERROR: --proxy requires local psql and DB_HOST=127.0.0.1." >&2
    exit 1
  fi
  DB_HOST=127.0.0.1
else
  : "${DB_HOST:?Set DB_HOST explicitly, or use --proxy for the selected Cloud SQL instance}"
fi
if [[ "$USE_K8S_PSQL" == "1" ]]; then
  : "${K8S_CONTEXT:?Set K8S_CONTEXT explicitly; the current kubectl context is never used}"
  if [[ "$K8S_CONTEXT" != "gke_${GCP_PROJECT}_"* ]]; then
    echo "ERROR: K8S_CONTEXT must be a GKE context for GCP_PROJECT=$GCP_PROJECT." >&2
    exit 1
  fi
  if [[ "$DB_HOST" == "127.0.0.1" || "$DB_HOST" == "localhost" || "$DB_HOST" == "::1" ]]; then
    echo "ERROR: Set DB_HOST to the Cloud SQL private IP when USE_K8S_PSQL=1." >&2
    exit 1
  fi
fi

# Prefer Homebrew-installed clients when they are not on the default PATH.
if command -v brew >/dev/null 2>&1; then
  for formula in postgresql@16 postgresql@15 postgresql libpq cloud-sql-proxy; do
    prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [[ -n "$prefix" && -d "$prefix/bin" ]]; then
      PATH="$prefix/bin:$PATH"
    fi
  done
  export PATH
fi

# ---------------------------------------------------------------------------
# Configuration (override via env vars)
# ---------------------------------------------------------------------------
GCP_REGION="${GCP_REGION:-us-central1}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:-g5}"
K8S_NAMESPACE="${K8S_NAMESPACE:-g5}"
SQL_PROXY_ARGS="${SQL_PROXY_ARGS:---private-ip}"
K8S_SQL_NAMESPACE="${K8S_SQL_NAMESPACE:-default}"
K8S_SQL_POD_IMAGE="${K8S_SQL_POD_IMAGE:-postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b}"
K8S_SQL_POD_NAME="${K8S_SQL_POD_NAME:-seed-prod-org-psql-$$}"
# The operator org's identity is fixed (g5.organization.platform-operator.identity).
OPERATOR_ORG_NAME="Platform operator"
OPERATOR_ORG_SLUG="operator"
DEFAULT_WORKER_EMAIL="${DEFAULT_WORKER_EMAIL:-worker@g5.internal}"
DEFAULT_WORKER_NAME="${DEFAULT_WORKER_NAME:-G5 Worker Service}"
DEFAULT_REPO_NAME="${DEFAULT_REPO_NAME:-docs}"
DEFAULT_REPO_SLUG="${DEFAULT_REPO_SLUG:-docs}"
DEFAULT_REPO_GIT_URL="${DEFAULT_REPO_GIT_URL:-g5-local://${OPERATOR_ORG_SLUG}/${DEFAULT_REPO_SLUG}}"
DEFAULT_REPO_BRANCH="${DEFAULT_REPO_BRANCH:-main}"
DEFAULT_WORKSPACE_NAME="${DEFAULT_WORKSPACE_NAME:-Base}"
# Org default workspace image: a name from the deployment's promoted catalog
# (G5_PROMOTED_BASE_IMAGES on the API deployment, e.g. g5d-g5dev) resolved to
# its digest, or an explicit digest-pinned DEFAULT_BASE_IMAGE_REF. Empty track
# and empty ref leave the org without a default (the UI then refuses to start
# workspaces with "organization has no default workspace image").
DEFAULT_BASE_IMAGE_TRACK="${DEFAULT_BASE_IMAGE_TRACK-g5d-g5dev}"
DEFAULT_BASE_IMAGE_REF="${DEFAULT_BASE_IMAGE_REF:-}"
K8S_API_DEPLOYMENT="${K8S_API_DEPLOYMENT:-g5-api}"
K8S_API_CONTAINER="${K8S_API_CONTAINER:-g5-api}"
# auto: initialise the docs and code mirrors through the API pod when kubectl
# is in use; skip: leave the branches unborn (fork/clone refuse the workspace
# until each has a first commit). DOCS_MIRROR_INIT is the former name.
REPO_MIRROR_INIT="${REPO_MIRROR_INIT:-${DOCS_MIRROR_INIT:-auto}}"
# An existing org default image is kept on re-runs unless this is 1.
FORCE_DEFAULT_BASE_IMAGE="${FORCE_DEFAULT_BASE_IMAGE:-0}"

# The track name is interpolated into SQL later, so it must satisfy the same
# grammar the orgs table enforces (orgs_default_base_image_track_name) before
# anything runs; the digest ref is checked against its own grammar at use.
if [[ -n "$DEFAULT_BASE_IMAGE_TRACK" && ! "$DEFAULT_BASE_IMAGE_TRACK" =~ ^[a-z0-9][a-z0-9._-]{0,62}$ ]]; then
  echo "ERROR: DEFAULT_BASE_IMAGE_TRACK must match ^[a-z0-9][a-z0-9._-]{0,62}$: '$DEFAULT_BASE_IMAGE_TRACK'" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
PROXY_PID=""
K8S_SQL_POD_CREATED="false"

kube() {
  command kubectl --context="$K8S_CONTEXT" "$@"
}

cleanup() {
  if [[ "$K8S_SQL_POD_CREATED" == "true" ]]; then
    kube delete pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" --ignore-not-found >/dev/null 2>&1 || true
  fi
  if [[ -n "$PROXY_PID" ]]; then
    echo "  Stopping cloud-sql-proxy (PID $PROXY_PID)..." >&2
    kill "$PROXY_PID" 2>/dev/null || true
    wait "$PROXY_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

ensure_k8s_psql_client() {
  if [[ "$USE_K8S_PSQL" != "1" ]]; then
    return 0
  fi

  if kube get pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" >/dev/null 2>&1; then
    return 0
  fi

  echo "=== Starting in-cluster psql client ===" >&2
  kube run "$K8S_SQL_POD_NAME" \
    -n "$K8S_SQL_NAMESPACE" \
    --image="$K8S_SQL_POD_IMAGE" \
    --restart=Never \
    --command -- sh -c "sleep 3600" >/dev/null
  K8S_SQL_POD_CREATED="true"
  kube wait --for=condition=Ready "pod/$K8S_SQL_POD_NAME" \
    -n "$K8S_SQL_NAMESPACE" \
    --timeout=180s >/dev/null
}

psql_exec() {
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    printf '%s\n' "$1" | kube exec -i "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" -- \
      env PGPASSWORD="${DB_PASSWORD:-}" \
      psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA
  else
    PGPASSWORD="${DB_PASSWORD:-}" psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "$1"
  fi
}

psql_batch() {
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    printf '%s\n' "$1" | kube exec -i "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" -- \
      env PGPASSWORD="${DB_PASSWORD:-}" \
      psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA
  else
    PGPASSWORD="${DB_PASSWORD:-}" psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA <<SQL
$1
SQL
  fi
}

# ---------------------------------------------------------------------------
# 0. Optional: start cloud-sql-proxy
# ---------------------------------------------------------------------------
if [[ "$USE_PROXY" == "--proxy" ]]; then
  if ! command -v cloud-sql-proxy >/dev/null 2>&1; then
    echo "ERROR: cloud-sql-proxy is required for --proxy mode." >&2
    exit 1
  fi
  echo "=== Starting cloud-sql-proxy ===" >&2
  INSTANCE_CONN="${GCP_PROJECT}:${GCP_REGION}:${DB_INSTANCE_NAME}"
  # Cloud SQL instances are private-only in these environments, so prefer private IP.
  cloud-sql-proxy "$INSTANCE_CONN" --port="$DB_PORT" $SQL_PROXY_ARGS &
  PROXY_PID=$!
  echo "  PID=$PROXY_PID, waiting for proxy to be ready..." >&2
  sleep 3
fi

# ---------------------------------------------------------------------------
# 0b. Get DB password from Secret Manager if not set
# ---------------------------------------------------------------------------
if [[ -z "${DB_PASSWORD:-}" ]]; then
  echo "=== Fetching DB password from Secret Manager ===" >&2
  DB_PASSWORD=$(gcloud secrets versions access latest \
    --secret=g5-db-password \
    --project="$GCP_PROJECT" 2>/dev/null || true)
  if [[ -z "$DB_PASSWORD" ]]; then
    echo "WARN: Could not fetch g5-db-password from Secret Manager." >&2
    echo "      Set DB_PASSWORD env var if password auth is required." >&2
  fi
  export DB_PASSWORD
fi

# ---------------------------------------------------------------------------
# 0c. Ensure psql is available
# ---------------------------------------------------------------------------
if ! command -v psql >/dev/null 2>&1; then
  if [[ "$USE_K8S_PSQL" != "1" ]]; then
    echo "ERROR: psql is required to seed the operator org." >&2
    echo "       Install PostgreSQL client tools or ensure Homebrew's bin dir is on PATH." >&2
    exit 1
  fi
fi

if [[ "$USE_K8S_PSQL" == "1" ]]; then
  if ! command -v kubectl >/dev/null 2>&1; then
    echo "ERROR: kubectl is required when USE_K8S_PSQL=1." >&2
    exit 1
  fi
fi

echo "=== Seed target: project=$GCP_PROJECT instance=$DB_INSTANCE_NAME database=$DB_NAME host=$DB_HOST ===" >&2
if [[ "$USE_K8S_PSQL" == "1" ]]; then
  echo "  Kubernetes context=$K8S_CONTEXT namespace=$K8S_NAMESPACE" >&2
fi
# Create the client in the parent shell so EXIT cleanup knows whether this
# invocation owns it, including when a later SQL command fails in a subshell.
ensure_k8s_psql_client

# ---------------------------------------------------------------------------
# 1. Resolve or create the operator org
# ---------------------------------------------------------------------------
echo "=== Resolving the operator org ===" >&2

# `orgs` carries FORCE ROW LEVEL SECURITY since migration 003 (P4.1): even the
# table owner is subject to its policies, and only members of g5_app
# (system access) or g5_tenant (scoped by authz.current_org_ids()) may read or
# write rows. The migration/admin user therefore assumes g5_app for this step;
# it must have been granted membership once per database (`GRANT g5_app TO
# <DB_USER>`, see docs/qa-database-reset.md). Set DB_ORG_ROLE="" to skip the
# role switch on a database that predates 003.
DB_ORG_ROLE="${DB_ORG_ROLE-g5_app}"
org_role_prefix=""
if [[ -n "$DB_ORG_ROLE" ]]; then
  org_role_prefix="SET ROLE $DB_ORG_ROLE;"
fi
# The kind, which only this seed (and the dev/E2E test-token fixture) writes,
# identifies the operator org; the orgs_single_operator index keeps it single.
# A tenant org holding the reserved slug makes the insert fail on its unique
# constraint rather than being adopted.
ORG_ID=$(psql_exec "
  $org_role_prefix
  WITH existing AS (
    SELECT id
      FROM orgs
     WHERE kind = 'operator'
     LIMIT 1
  ),
  inserted AS (
    INSERT INTO orgs (name, slug, kind)
    SELECT '$OPERATOR_ORG_NAME', '$OPERATOR_ORG_SLUG', 'operator'
    WHERE NOT EXISTS (SELECT 1 FROM existing)
    RETURNING id
  )
  SELECT id FROM inserted
  UNION ALL
  SELECT id FROM existing
  LIMIT 1;
")
psql_exec "
  $org_role_prefix
  UPDATE orgs
     SET name = '$OPERATOR_ORG_NAME'
   WHERE id = '$ORG_ID'
     AND kind = 'operator'
     AND name IS DISTINCT FROM '$OPERATOR_ORG_NAME';
" >/dev/null
echo "  org_id=$ORG_ID" >&2

# ---------------------------------------------------------------------------
# 2. Ensure the service user (idempotent)
# ---------------------------------------------------------------------------
# The non-login worker service user is the SQL session actor for the rows
# below, the inviter of record, and the owner of the operator org's Base.
USER_ID=$(psql_exec "
  INSERT INTO users (email, display_name, oauth_provider, oauth_id)
  VALUES ('$DEFAULT_WORKER_EMAIL', '$DEFAULT_WORKER_NAME', 'service', 'g5-worker')
  ON CONFLICT (email) DO UPDATE
    SET display_name = EXCLUDED.display_name,
        oauth_provider = EXCLUDED.oauth_provider,
        oauth_id = EXCLUDED.oauth_id
  RETURNING id;
")
echo "  service_user_id=$USER_ID" >&2

# A live workspace needs an owner (workspaces_active_owner_required) holding an
# owner-eligible membership in its org (workspaces_owner_same_org_fk), and the
# operator org has no person at seed time. The service user therefore holds a
# `member` membership solely to own Base. It cannot sign in (oauth_provider
# 'service') and platform routes need a person session, so it never exercises
# operator authority. A re-run restores the role if it was changed.
psql_exec "
  INSERT INTO org_memberships (org_id, user_id, role)
  VALUES ('$ORG_ID', '$USER_ID', 'member')
  ON CONFLICT (org_id, user_id) DO UPDATE
    SET role = EXCLUDED.role
    WHERE org_memberships.role IS DISTINCT FROM EXCLUDED.role;
" >/dev/null
echo "  service user holds the operator org's Base-owner membership (member)" >&2

# ---------------------------------------------------------------------------
# 3. Create docs repo for the org (idempotent)
# ---------------------------------------------------------------------------
REPO_ID=$(psql_batch "
  BEGIN;

  INSERT INTO authz.session_ctx (pid, org_ids, user_id, scopes, set_at)
  VALUES (
    pg_backend_pid(),
    ARRAY['$ORG_ID']::uuid[],
    '$USER_ID',
    ARRAY['*']::text[],
    now()
  )
  ON CONFLICT (pid) DO UPDATE
     SET org_ids = EXCLUDED.org_ids,
         user_id = EXCLUDED.user_id,
         scopes = EXCLUDED.scopes,
         set_at = EXCLUDED.set_at;

  INSERT INTO repos (org_id, name, slug, git_url, default_branch)
  VALUES ('$ORG_ID', '$DEFAULT_REPO_NAME', '$DEFAULT_REPO_SLUG', '$DEFAULT_REPO_GIT_URL', '$DEFAULT_REPO_BRANCH')
  ON CONFLICT (org_id, slug) DO UPDATE
    SET name = EXCLUDED.name,
        git_url = EXCLUDED.git_url,
        default_branch = EXCLUDED.default_branch
  RETURNING id;

  DO \$\$
  BEGIN
    PERFORM authz.clear_ctx();
  END
  \$\$;

  COMMIT;
")
echo "  repo_id=$REPO_ID ($DEFAULT_REPO_SLUG)" >&2

# ---------------------------------------------------------------------------
# 4. Create the docs-only base workspace + settings (idempotent)
# ---------------------------------------------------------------------------
WS_ID=$(psql_batch "
  BEGIN;

  INSERT INTO authz.session_ctx (pid, org_ids, user_id, scopes, set_at)
  VALUES (
    pg_backend_pid(),
    ARRAY['$ORG_ID']::uuid[],
    '$USER_ID',
    ARRAY['*']::text[],
    now()
  )
  ON CONFLICT (pid) DO UPDATE
     SET org_ids = EXCLUDED.org_ids,
         user_id = EXCLUDED.user_id,
         scopes = EXCLUDED.scopes,
         set_at = EXCLUDED.set_at;

  CREATE TEMP TABLE seed_base_workspace (
    id uuid PRIMARY KEY
  ) ON COMMIT DROP;

  INSERT INTO workspaces (
    org_id, name, is_base, docs_repo_id,
    created_by
  )
  SELECT
    '$ORG_ID',
    '$DEFAULT_WORKSPACE_NAME',
    true,
    '$REPO_ID',
    '$USER_ID'
  WHERE NOT EXISTS (
    SELECT 1
      FROM workspaces
     WHERE org_id = '$ORG_ID'
       AND is_base = true
  );

  INSERT INTO seed_base_workspace (id)
  SELECT id
    FROM workspaces
   WHERE org_id = '$ORG_ID'
     AND is_base = true
   ORDER BY created_at ASC
   LIMIT 1;

  UPDATE workspaces
     SET name = '$DEFAULT_WORKSPACE_NAME',
         docs_repo_id = '$REPO_ID',
         created_by = COALESCE(created_by, '$USER_ID'),
         updated_at = now()
   WHERE id = (SELECT id FROM seed_base_workspace);

  INSERT INTO workspace_repos (
    org_id, workspace_id, repo_id, root_path, role
  )
  SELECT
    '$ORG_ID', id, '$REPO_ID', 'docs', 'docs'
  FROM seed_base_workspace
  ON CONFLICT (workspace_id, repo_id) DO NOTHING;

  INSERT INTO workspace_settings (workspace_id, org_id)
  SELECT id, '$ORG_ID'
    FROM seed_base_workspace
  ON CONFLICT (workspace_id) DO NOTHING;

  SELECT id FROM seed_base_workspace;

  DO \$\$
  BEGIN
    PERFORM authz.clear_ctx();
  END
  \$\$;

  COMMIT;
")
echo "  workspace_id=$WS_ID (base)" >&2
echo "  workspace_repos and workspace_settings ensured" >&2

# ---------------------------------------------------------------------------
# 4a. Ensure Base has a code repository member (idempotent)
# ---------------------------------------------------------------------------
# Ordinary workspace Start needs a code member, so a docs-only Base cannot seed
# startable children. Mirror the tenant creation cascade (g5.team.onboarding):
# a providerless `code` repo at g5-local://<org-slug>/code on `main`,
# attached at root `code`. A Base that already has a code member keeps it, and
# an existing `code` repo row is adopted, never rewritten. A provider-backed
# `code` repo is refused: the seed cannot invent its history, and choosing
# another code repository is an administrator decision. Prints the seeded
# repo's `id|default_branch` whenever it is a Base member, so a rerun can
# finish a skipped first commit; prints nothing when Base's code member is
# another repository.
CODE_REPO_ROW=$(psql_batch "
  BEGIN;

  INSERT INTO authz.session_ctx (pid, org_ids, user_id, scopes, set_at)
  VALUES (
    pg_backend_pid(),
    ARRAY['$ORG_ID']::uuid[],
    '$USER_ID',
    ARRAY['*']::text[],
    now()
  )
  ON CONFLICT (pid) DO UPDATE
     SET org_ids = EXCLUDED.org_ids,
         user_id = EXCLUDED.user_id,
         scopes = EXCLUDED.scopes,
         set_at = EXCLUDED.set_at;

  DO \$\$
  BEGIN
    IF NOT EXISTS (
         SELECT 1 FROM workspace_repos
          WHERE workspace_id = '$WS_ID' AND role = 'code'
       )
       AND EXISTS (
         SELECT 1 FROM repos
          WHERE org_id = '$ORG_ID' AND slug = 'code' AND provider IS NOT NULL
       ) THEN
      RAISE EXCEPTION 'The operator org code repository is provider-backed; attach a Base code repository explicitly';
    END IF;
  END
  \$\$;

  INSERT INTO repos (org_id, name, slug, git_url, default_branch)
  SELECT '$ORG_ID', 'code', 'code', 'g5-local://$OPERATOR_ORG_SLUG/code', 'main'
  WHERE NOT EXISTS (
    SELECT 1 FROM workspace_repos
     WHERE workspace_id = '$WS_ID' AND role = 'code'
  )
  ON CONFLICT (org_id, slug) DO NOTHING;

  INSERT INTO workspace_repos (
    org_id, workspace_id, repo_id, root_path, role
  )
  SELECT '$ORG_ID', '$WS_ID', r.id, 'code', 'code'
    FROM repos r
   WHERE r.org_id = '$ORG_ID'
     AND r.slug = 'code'
     AND r.provider IS NULL
     AND NOT EXISTS (
       SELECT 1 FROM workspace_repos
        WHERE workspace_id = '$WS_ID' AND role = 'code'
     )
  ON CONFLICT (workspace_id, repo_id) DO NOTHING;

  SELECT r.id || '|' || r.default_branch
    FROM repos r
    JOIN workspace_repos m
      ON m.repo_id = r.id
     AND m.workspace_id = '$WS_ID'
     AND m.role = 'code'
   WHERE r.org_id = '$ORG_ID'
     AND r.slug = 'code'
     AND r.provider IS NULL;

  DO \$\$
  BEGIN
    PERFORM authz.clear_ctx();
  END
  \$\$;

  COMMIT;
")
CODE_REPO_ID=""
CODE_REPO_BRANCH=""
if [[ -n "$CODE_REPO_ROW" ]]; then
  CODE_REPO_ID="${CODE_REPO_ROW%%|*}"
  CODE_REPO_BRANCH="${CODE_REPO_ROW#*|}"
  if [[ ! "$CODE_REPO_ID" =~ ^[0-9a-f-]{36}$ || ! "$CODE_REPO_BRANCH" =~ ^[A-Za-z0-9._/-]+$ ]]; then
    echo "ERROR: unexpected code repository row: $CODE_REPO_ROW" >&2
    exit 1
  fi
  echo "  code_repo_id=$CODE_REPO_ID (code, Base member at root code)" >&2
else
  echo "  Base already has another code repository member; kept unchanged" >&2
fi

# ---------------------------------------------------------------------------
# 4b. Initialise the seeded repository mirrors and materialise their branches
# ---------------------------------------------------------------------------
# The API keeps every repository as a bare mirror at
# <G5_STORAGE_MOUNT_POINT>/<G5_STORAGE_TENANT_PREFIX>/<org-id>/<repo-id>. The seed
# only wrote rows, so a fresh database had branches with no ref behind them
# and both the fork service ("Source branch refs/heads/main is absent") and
# g5d clone ("absent from the internal mirror") refused the base workspace.
# Base ref materialization is owned by Git. Initialize an empty repository;
# refuse to invent history when a nonempty repository has lost its selected
# ref; never touch an existing branch. Docs gets an empty first commit and
# code the tenant cascade's README, which has no workspace image override, so
# children start on the org default image.
repo_mirror_init() {
  local repo_id="$1" branch="$2" kind="$3" script sha
  read -r -d '' script <<'POD' || true
set -eu
org_id="$1"; repo_id="$2"; branch="$3"; kind="$4"
root="${G5_STORAGE_MOUNT_POINT:-/mnt/g5-repos}/${G5_STORAGE_TENANT_PREFIX:-repos}"
repo="$root/$org_id/$repo_id"
if [ ! -d "$repo" ]; then
  mkdir -p "$root/$org_id"
  git init -q --bare "$repo"
fi
if ! git --git-dir="$repo" rev-parse -q --verify "refs/heads/$branch" >/dev/null 2>&1; then
  if git --git-dir="$repo" show-ref --quiet; then
    echo "MISSING_REF refs/heads/$branch is absent in a nonempty repository" >&2
    exit 2
  fi
  work="$(mktemp -d)"
  git -C "$work" init -q -b "$branch"
  if [ "$kind" = code ]; then
    printf '# code\n\nThis is a G5-managed local project repository.\n' > "$work/README.md"
    git -C "$work" add README.md
  fi
  git -C "$work" -c user.email=seed@g5labs.ai -c user.name="G5 Seed" commit -q --allow-empty -m "Initialise $kind repository"
  git -C "$work" push -q "$repo" "$branch:$branch"
  rm -rf "$work"
fi
printf 'ref: refs/heads/%s\n' "$branch" > "$repo/HEAD"
git --git-dir="$repo" rev-parse "refs/heads/$branch"
POD
  sha="$(printf '%s\n' "$script" | kube exec -i -n "$K8S_NAMESPACE" "deploy/$K8S_API_DEPLOYMENT" -c "$K8S_API_CONTAINER" -- sh -s -- "$ORG_ID" "$repo_id" "$branch" "$kind" 2>&1 | tail -n 1 | tr -d '[:space:]')"
  if [[ ! "$sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "ERROR: $kind mirror initialisation failed: $sha" >&2
    echo "       A branch that records a real commit needs its mirror restored from backup, not re-created." >&2
    return 1
  fi
  printf '%s\n' "$sha"
}

if [[ "$REPO_MIRROR_INIT" == "skip" ]]; then
  echo "  repository mirror initialisation skipped (REPO_MIRROR_INIT=skip); the base workspace cannot be forked or cloned until its docs and code branches have a commit" >&2
elif [[ "$USE_K8S_PSQL" != "1" ]]; then
  echo "WARN: repository mirror initialisation needs kubectl access to the API pod (USE_K8S_PSQL=1); the base workspace cannot be forked or cloned until its docs and code branches have a commit" >&2
else
  echo "=== Initialising repository mirrors ===" >&2
  DOCS_HEAD_SHA="$(repo_mirror_init "$REPO_ID" "$DEFAULT_REPO_BRANCH" docs)"
  echo "  docs mirror ready at $DOCS_HEAD_SHA; base branch materialized" >&2
  if [[ -n "$CODE_REPO_ID" ]]; then
    CODE_HEAD_SHA="$(repo_mirror_init "$CODE_REPO_ID" "$CODE_REPO_BRANCH" code)"
    echo "  code mirror ready at $CODE_HEAD_SHA; base branch materialized" >&2
  fi
fi

# ---------------------------------------------------------------------------
# 4c. Org default workspace image
# ---------------------------------------------------------------------------
# Mirrors PUT /api/v1/orgs/<org>/worker-hosts/image-policy: a track name is
# resolved against the API deployment's promoted catalog and stored together
# with its digest-pinned ref; an explicit ref is stored as a pin. Without a
# default the UI shows "organization has no default workspace image".
resolve_default_base_image() {
  local track="$1" catalog entry name
  if [[ "$USE_K8S_PSQL" != "1" ]]; then
    return 1
  fi
  catalog="$(kube get deploy -n "$K8S_NAMESPACE" "$K8S_API_DEPLOYMENT" \
    -o jsonpath="{.spec.template.spec.containers[?(@.name==\"$K8S_API_CONTAINER\")].env[?(@.name==\"G5_PROMOTED_BASE_IMAGES\")].value}" 2>/dev/null || true)"
  [[ -n "$catalog" ]] || return 1
  # Same grammar as the API's parse_promoted_base_images: comma-separated
  # (newlines are whitespace, not separators), each entry either
  # `track=registry/path@sha256:...` or a bare ref whose track is the last
  # path segment; surrounding whitespace ignored.
  local ref
  catalog="$(printf '%s' "$catalog" | tr '\n\r' '  ')"
  IFS=',' read -r -a entries <<<"$catalog"
  for entry in ${entries[@]+"${entries[@]}"}; do
    entry="${entry#"${entry%%[![:space:]]*}"}"
    entry="${entry%"${entry##*[![:space:]]}"}"
    [[ -n "$entry" ]] || continue
    if [[ "$entry" == *=* ]]; then
      name="${entry%%=*}"
      ref="${entry#*=}"
      # The API trims each side of `=` independently.
      name="${name#"${name%%[![:space:]]*}"}"; name="${name%"${name##*[![:space:]]}"}"
      ref="${ref#"${ref%%[![:space:]]*}"}"; ref="${ref%"${ref##*[![:space:]]}"}"
    else
      ref="$entry"
      name="${ref%%@*}"
      name="${name##*/}"
    fi
    # Like the API parser, an entry whose ref is not digest-pinned is skipped
    # rather than shadowing a later valid entry for the same track.
    if [[ "$name" == "$track" && "$ref" =~ ^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$ ]]; then
      printf '%s\n' "$ref"
      return 0
    fi
  done
  return 1
}

# An org that already has a default image (a pin or a track chosen by an
# administrator, or the API's own tracking) keeps it; the seed only fills an
# unset policy unless FORCE_DEFAULT_BASE_IMAGE=1.
EXISTING_IMAGE_POLICY="$(psql_exec "
  $org_role_prefix
  SELECT COALESCE(default_base_image_track, '') || '|' || COALESCE(default_base_image_ref, '')
    FROM orgs WHERE id = '$ORG_ID';
" | tr -d '[:space:]')"
IMAGE_REF="$DEFAULT_BASE_IMAGE_REF"
IMAGE_TRACK=""
if [[ "$EXISTING_IMAGE_POLICY" != "|" && "${FORCE_DEFAULT_BASE_IMAGE:-0}" != "1" ]]; then
  echo "  org default workspace image already set (track '${EXISTING_IMAGE_POLICY%%|*}', ref '${EXISTING_IMAGE_POLICY#*|}'); keeping it (FORCE_DEFAULT_BASE_IMAGE=1 overrides)" >&2
  IMAGE_REF=""
  DEFAULT_BASE_IMAGE_TRACK=""
fi
if [[ -z "$IMAGE_REF" && -n "$DEFAULT_BASE_IMAGE_TRACK" ]]; then
  if IMAGE_REF="$(resolve_default_base_image "$DEFAULT_BASE_IMAGE_TRACK")"; then
    IMAGE_TRACK="$DEFAULT_BASE_IMAGE_TRACK"
  else
    IMAGE_REF=""
    echo "WARN: promoted image track '$DEFAULT_BASE_IMAGE_TRACK' not found in the API deployment's G5_PROMOTED_BASE_IMAGES; org default workspace image left unchanged" >&2
    echo "      Set DEFAULT_BASE_IMAGE_REF to a digest-pinned ref, or use PUT /api/v1/orgs/$ORG_ID/worker-hosts/image-policy" >&2
  fi
fi
if [[ -n "$IMAGE_REF" ]]; then
  if [[ ! "$IMAGE_REF" =~ ^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$ ]]; then
    echo "ERROR: default workspace image must be digest-pinned (registry/path@sha256:...): $IMAGE_REF" >&2
    exit 1
  fi
  echo "=== Setting org default workspace image ===" >&2
  psql_exec "
    $org_role_prefix
    UPDATE orgs
       SET default_base_image_ref = '$IMAGE_REF',
           default_base_image_track = $( [[ -n "$IMAGE_TRACK" ]] && printf "'%s'" "$IMAGE_TRACK" || printf 'NULL' )
     WHERE id = '$ORG_ID';
  " >/dev/null
  echo "  default workspace image: ${IMAGE_TRACK:+track $IMAGE_TRACK -> }$IMAGE_REF" >&2
fi

# ---------------------------------------------------------------------------
# 5. Invite the first operator (idempotent)
# ---------------------------------------------------------------------------
# A pending admin invitation admits the person at sign-in, where accepting it
# makes them an operator admin. A person who is already a member is skipped;
# later operators are invited by operator admins.
if [[ -n "$DEFAULT_ADMIN_EMAIL" ]]; then
  psql_exec "
    INSERT INTO org_invitations (org_id, email, role, invited_by, status, expires_at)
    SELECT '$ORG_ID', '$DEFAULT_ADMIN_EMAIL', 'admin', '$USER_ID', 'pending',
           now() + interval '3650 days'
    WHERE NOT EXISTS (
      SELECT 1
        FROM org_memberships m
        JOIN users u ON u.id = m.user_id
       WHERE m.org_id = '$ORG_ID'
         AND lower(u.email) = lower('$DEFAULT_ADMIN_EMAIL')
    )
    ON CONFLICT (org_id, email, status) DO UPDATE
      SET role = EXCLUDED.role,
          invited_by = EXCLUDED.invited_by,
          expires_at = GREATEST(org_invitations.expires_at, EXCLUDED.expires_at),
          updated_at = now();
  " >/dev/null
  echo "  bootstrap invitation ensured for ${DEFAULT_ADMIN_EMAIL}" >&2
fi

# ---------------------------------------------------------------------------
# 6. Summary
# ---------------------------------------------------------------------------
echo "" >&2
echo "=== Seed complete ===" >&2
echo "  Operator org:  $ORG_ID ($OPERATOR_ORG_SLUG)" >&2
echo "  Workspace ID:  $WS_ID" >&2
echo "  Docs Repo ID:  $REPO_ID" >&2
echo "  Code Repo ID:  ${CODE_REPO_ID:-(administrator-attached code member kept)}" >&2
if [[ -n "$DEFAULT_ADMIN_EMAIL" ]]; then
  echo "" >&2
  echo "  ${DEFAULT_ADMIN_EMAIL} signs in to accept the operator admin invitation, then creates" >&2
  echo "  tenant orgs from /operator/home or POST /api/v1/platform/orgs." >&2
fi
