#!/usr/bin/env bash
# =============================================================================
# reset-dev-environment.sh — Wipe & regenerate the CLOUD dev environment
# (dev.g5labs.ai) to a clean slate.
# =============================================================================
# Standup action item (2026-06-15): "Reset Dev Environment — wipe and regenerate
# the current development environment to apply new invariants and start from a
# clean slate." dev has accumulated a bucket of broken workspaces/orgs that
# violate new enforcement invariants; this script clears all tenant data and
# leaves a single empty `internal-g5-devs` org with the people who are members
# (or invited) today.
#
# It runs in TWO steps, because the wipe also removes the operator org and the
# calling operator's membership (g5.organization.platform-operator.bootstrap):
#
# STEP 1 — WIPE (default; fail-closed on the backup):
#   1. Resolve + confirm the target Cloud SQL instance (dev by default).
#   2. BACKUP FIRST — `gcloud sql backups create` (+ optional GCS export), a
#      `gcloud filestore backups create` of the git repos (Filestore, not in
#      Postgres), and a local snapshot of every org member / pending invitation.
#      Aborts the wipe if any backup does not succeed.
#   3. Capture "people as of today" = active members ∪ pending invitations.
#   4. WIPE — `TRUNCATE orgs ... CASCADE` as the admin `g5` role (a plain
#      DELETE FROM orgs fails: ~35 child tables FK orgs(id) WITHOUT cascade).
#      Global tables (`users`, `oauth_scopes`) are preserved by design so the
#      same people can be re-invited; auth sessions are cleared.
#   5. PLATFORM SEED — re-run scripts/seed-prod-org.sh against the same
#      database: it provisions only the operator org and gives
#      DEFAULT_ADMIN_EMAIL (default: the G5D_TOKEN user) a pending operator
#      admin invitation. Then restart g5-api + g5-daemon to clear process-local
#      caches, and STOP: the operator signs in again, which accepts that
#      invitation.
#
# STEP 2 — `--regenerate --members-file=<snapshot>` (after signing in again):
#   6. Create the empty `internal-g5-devs` org through the operator API
#      (POST /api/v1/platform/orgs), owned by the G5D_TOKEN user, exactly as an
#      operator does from /operator/home. The creation cascade also initializes
#      the docs repo working tree server-side.
#   7. Re-invite every captured person via the invite API with their captured role.
#
# This is DESTRUCTIVE and runs against a SHARED environment. It requires an
# explicit confirmation (type the instance name) unless --yes is passed, and it
# refuses to touch a project whose id looks like prod unless ALLOW_PROD=1.
#
# Mechanism: the WIPE and the platform seed are direct SQL (the API has no
# org-delete endpoint, and only the seed writes the operator org), but tenant
# org/workspace/docs-repo CREATION goes through the operator API.
#
# Prerequisites:
#   - gcloud authenticated with IAM for Secret Manager + Cloud SQL on the project
#   - cloud-sql-proxy installed (for --proxy), OR an in-cluster psql pod
#     (USE_K8S_PSQL=1 with DB_HOST=<Cloud SQL private IP>; also lets the seed
#     initialise the operator docs and code mirrors and image policy)
#   - psql client (or kubectl for the in-cluster path); curl + jq
#   - step 1: DEFAULT_ADMIN_EMAIL, or G5D_TOKEN to resolve it
#   - step 2: G5D_TOKEN = a fresh operator session bearer for $API_URL
#     (G5_E2E_AUTH is off on dev). Get one via `g5d --api-url <url> auth login`
#     or a copied web session after signing in.
#
# Usage:
#   # Dry run — print every step + SQL, touch nothing:
#   ./scripts/reset-dev-environment.sh --dry-run
#
#   # Step 1, real run, via an in-cluster psql pod (recommended):
#   G5D_TOKEN=... USE_K8S_PSQL=1 DB_HOST=10.x.x.x ./scripts/reset-dev-environment.sh
#
#   # Step 1, auto-starting cloud-sql-proxy against the private IP:
#   DEFAULT_ADMIN_EMAIL=you@g5labs.ai ./scripts/reset-dev-environment.sh --proxy
#
#   # Non-interactive (CI / scripted) — skip the typed confirmation:
#   ./scripts/reset-dev-environment.sh --proxy --yes
#
#   # Also export a full SQL dump to GCS in addition to the on-demand backup:
#   BACKUP_BUCKET=g5-dev-db-backups ./scripts/reset-dev-environment.sh --proxy
#
#   # Step 2, after signing in at $API_URL again:
#   G5D_TOKEN=<fresh token> ./scripts/reset-dev-environment.sh --regenerate \
#     --members-file=/tmp/g5-dev-reset-backups/members-g5-dev-1ccc-<ts>.tsv
# =============================================================================
set -euo pipefail

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
# Configuration (override via env vars) — defaults target the DEV environment.
# ---------------------------------------------------------------------------
GCP_PROJECT="${GCP_PROJECT:-g5-dev-1ccc}"
GCP_REGION="${GCP_REGION:-us-central1}"
DB_INSTANCE_NAME="${DB_INSTANCE_NAME:-g5-dev-postgres}"
DB_HOST="${DB_HOST:-127.0.0.1}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:-g5}"
DB_NAME="${DB_NAME:-g5_dev}"
SQL_PROXY_ARGS="${SQL_PROXY_ARGS:---private-ip}"
USE_K8S_PSQL="${USE_K8S_PSQL:-0}"
K8S_SQL_NAMESPACE="${K8S_SQL_NAMESPACE:-default}"
K8S_SQL_POD_IMAGE="${K8S_SQL_POD_IMAGE:-postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b}"
K8S_SQL_POD_NAME="${K8S_SQL_POD_NAME:-reset-dev-psql-$$}"

# New empty org to leave behind. Created in step 2 through the operator API
# (POST /api/v1/platform/orgs, owned by g5-api-routes/src/routes/platform.rs →
# the shared creation cascade), which also
# initializes the docs repo working tree server-side — mirroring "the first
# thing we do is create a new org, always on our side".
NEW_ORG_NAME="${NEW_ORG_NAME:-internal-g5-devs}"
NEW_ORG_SLUG="${NEW_ORG_SLUG:-internal-g5-devs}"

# Platform API endpoint + operator auth. G5_E2E_AUTH is OFF on dev, so there
# is no test-token shortcut: supply a real operator session as a Bearer token
# (G5D_TOKEN) — e.g. from `g5d auth login` or a copied web session. Step 2
# creates the org for THAT user as Admin (adminEmail = GET /api/v1/users/me),
# just as an operator creates an org for a named initial Admin. Step 1 uses it only to
# resolve DEFAULT_ADMIN_EMAIL when that is unset.
API_URL="${API_URL:-https://dev.g5labs.ai}"
G5D_TOKEN="${G5D_TOKEN:-}"

# Platform seed re-run after the wipe (step 1). DEFAULT_ADMIN_EMAIL gets the
# pending operator admin invitation; it defaults to the G5D_TOKEN user's email.
SEED_SCRIPT="${SEED_SCRIPT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/seed-prod-org.sh}"
DEFAULT_ADMIN_EMAIL="${DEFAULT_ADMIN_EMAIL:-}"
# The seed pins every kubectl call to an explicit context (USE_K8S_PSQL=1);
# default to the current one, which must be the GKE context of GCP_PROJECT.
K8S_CONTEXT="${K8S_CONTEXT:-}"

# Restart API processes after the wipe so process-local tenant caches reload
# from the fresh database. Needs kubectl pointed at the g5-dev cluster. Set
# RESTART_API=0 to skip.
RESTART_API="${RESTART_API:-1}"
API_K8S_NAMESPACE="${API_K8S_NAMESPACE:-g5}"
API_DEPLOYMENTS="${API_DEPLOYMENTS:-g5-api g5-daemon}"

# Backup destination. The on-demand Cloud SQL backup is always taken; a SQL
# export to GCS is additionally taken when BACKUP_BUCKET is set. The local
# member/invitation snapshot is written under BACKUP_DIR (outside the repo).
BACKUP_BUCKET="${BACKUP_BUCKET:-}"
BACKUP_DIR="${BACKUP_DIR:-${TMPDIR:-/tmp}/g5-dev-reset-backups}"

# Git repos live on the Filestore (NFS) share, NOT in Postgres, so the Cloud SQL
# backup does not cover them. The wipe leaves them orphaned on disk; back them up
# first too. dev's BASIC_HDD tier supports BACKUPS (not snapshots); billed on
# used bytes (~$0.08/GiB-mo). The backup is kept (not auto-deleted). Set
# BACKUP_REPOS=0 to skip.
BACKUP_REPOS="${BACKUP_REPOS:-1}"
FILESTORE_INSTANCE="${FILESTORE_INSTANCE:-g5-dev-repos}"
FILESTORE_ZONE="${FILESTORE_ZONE:-us-central1-a}"
FILESTORE_SHARE="${FILESTORE_SHARE:-g5_repos}"
FILESTORE_BACKUP_REGION="${FILESTORE_BACKUP_REGION:-$GCP_REGION}"

# Flags
DRY_RUN=0
ASSUME_YES=0
USE_PROXY=0
REGENERATE=0
MEMBERS_FILE_ARG=""
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=1 ;;
    --yes|-y)     ASSUME_YES=1 ;;
    --proxy)      USE_PROXY=1 ;;
    --regenerate) REGENERATE=1 ;;
    --members-file=*) MEMBERS_FILE_ARG="${arg#--members-file=}" ;;
    -h|--help)    sed -n '2,90p' "$0"; exit 0 ;;
    *) echo "Unknown argument: $arg (try --help)" >&2; exit 2 ;;
  esac
done

TS="$(date -u +%Y%m%d-%H%M%SZ)"
# Non-zero on partial failure (e.g. some re-invites failed) so callers/CI don't
# read a half-finished reset as success. Set to 1 by the offending step.
RESET_EXIT=0

log()  { echo "$*" >&2; }
die()  { echo "ERROR: $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Helpers (connection handling mirrors scripts/seed-prod-org.sh)
# ---------------------------------------------------------------------------
PROXY_PID=""
K8S_SQL_POD_CREATED="false"

cleanup() {
  if [[ "$K8S_SQL_POD_CREATED" == "true" ]]; then
    kubectl delete pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" --ignore-not-found >/dev/null 2>&1 || true
  fi
  if [[ -n "$PROXY_PID" ]]; then
    log "  Stopping cloud-sql-proxy (PID $PROXY_PID)..."
    kill "$PROXY_PID" 2>/dev/null || true
    wait "$PROXY_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

ensure_k8s_psql_client() {
  [[ "$USE_K8S_PSQL" == "1" ]] || return 0
  if kubectl get pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" >/dev/null 2>&1; then
    return 0
  fi
  log "=== Starting in-cluster psql client ==="
  kubectl run "$K8S_SQL_POD_NAME" \
    -n "$K8S_SQL_NAMESPACE" \
    --image="$K8S_SQL_POD_IMAGE" \
    --restart=Never \
    --command -- sh -c "sleep 3600" >/dev/null
  K8S_SQL_POD_CREATED="true"
  kubectl wait --for=condition=Ready "pod/$K8S_SQL_POD_NAME" \
    -n "$K8S_SQL_NAMESPACE" --timeout=180s >/dev/null
}

# psql_exec <sql>     → single value / row stream (-tA), connection per call.
# psql_batch <sql>    → multi-statement batch (transactions), connection per call.
# In --dry-run these print the SQL to stderr and return empty WITHOUT executing.
_psql() {
  local sql="$1"
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    ensure_k8s_psql_client
    printf '%s\n' "$sql" | kubectl exec -i "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" -- \
      env PGPASSWORD="${DB_PASSWORD:-}" \
      psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA
  else
    PGPASSWORD="${DB_PASSWORD:-}" psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA <<SQL
$sql
SQL
  fi
}

psql_exec()  { _psql "$1"; }
psql_batch() { _psql "$1"; }

# psql_write <label> <sql> — a mutating statement. Honors --dry-run.
psql_write() {
  local label="$1" sql="$2"
  if [[ "$DRY_RUN" == "1" ]]; then
    log "  [dry-run] would run: $label"
    printf '%s\n' "$sql" | sed 's/^/    | /' >&2
    return 0
  fi
  _psql "$sql"
}

# Single-quote escaping for values interpolated into SQL literals.
sql_lit() { printf "%s" "${1//\'/\'\'}"; }

# api_call <METHOD> <path> [json-body] — authenticated platform API request.
# Prints the response body on stdout; fails closed on a non-2xx status.
api_call() {
  local method="$1" path="$2" body="${3:-}" resp code
  local args=(-sS -X "$method" "${API_URL}${path}"
              -H "Authorization: Bearer ${G5D_TOKEN}"
              -H "Content-Type: application/json"
              -w $'\n%{http_code}' --max-time 60)
  [[ -n "$body" ]] && args+=(-d "$body")
  for _ in 1 2 3 4 5; do
    if ! resp="$(curl "${args[@]}")"; then sleep 5; continue; fi
    code="${resp##*$'\n'}"; resp="${resp%$'\n'*}"
    # Retry transient gateway errors — e.g. the ingress/NEG still re-registering
    # backends just after a g5-api restart returns 502/503/504 with no body.
    if [[ "$code" == "502" || "$code" == "503" || "$code" == "504" ]]; then
      sleep 5; continue
    fi
    if [[ "$code" -lt 200 || "$code" -ge 300 ]]; then
      die "API $method $path returned $code: $resp"
    fi
    printf '%s' "$resp"; return 0
  done
  die "API $method $path still failing after retries (last status: ${code:-none})"
}

# token_preflight — fail before anything destructive or org-creating when the
# operator token is rejected or $API_URL is not the dev API. curl already
# writes '000' to stdout on a connection failure, so capture its output
# verbatim — do NOT append our own '000' (that yields '000000', which slips
# past the check). Require a 2xx: a 404/500 means $API_URL is the wrong
# service or the API is unhealthy.
token_preflight() {
  log "=== Preflight: validating operator token against $API_URL ==="
  PF_CODE="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 \
    -H "Authorization: Bearer ${G5D_TOKEN}" "${API_URL}/api/v1/auth/me/scopes" 2>/dev/null || true)"
  PF_CODE="${PF_CODE:-000}"
  case "$PF_CODE" in
    2??) log "  ✓ $API_URL reachable and token accepted (HTTP ${PF_CODE})." ;;
    000) die "Operator preflight failed: ${API_URL} unreachable (curl could not connect).
       Check API_URL / network connectivity." ;;
    401) die "Operator preflight failed: ${API_URL} rejected G5D_TOKEN (401 — expired or wrong
       environment). Mint a fresh operator token and re-run." ;;
    *)   die "Operator preflight failed: ${API_URL}/api/v1/auth/me/scopes returned ${PF_CODE}
       (expected 2xx). Check API_URL points at the dev API and the token is valid." ;;
  esac
}

# token_email — the G5D_TOKEN user's email (GET /api/v1/users/me).
token_email() {
  api_call GET /api/v1/users/me | jq -r '.data.email // empty'
}

# ---------------------------------------------------------------------------
# 0. Guards: prod protection + tooling
# ---------------------------------------------------------------------------
if [[ "$GCP_PROJECT" == *prod* && "${ALLOW_PROD:-0}" != "1" ]]; then
  die "Refusing to run against project '$GCP_PROJECT' (looks like prod). Set ALLOW_PROD=1 to override."
fi

# ---------------------------------------------------------------------------
# STEP 2 (--regenerate): create the empty org through the operator API and
# re-invite today's people. Needs no database, gcloud or kubectl access.
# ---------------------------------------------------------------------------
if [[ "$REGENERATE" == "1" ]]; then
  [[ -n "$MEMBERS_FILE_ARG" ]] \
    || die "--regenerate needs --members-file=<members-*.tsv snapshot written by the wipe step>."
  MEMBERS_FILE="$MEMBERS_FILE_ARG"
  log ""
  log "================ REGENERATE DEV ENVIRONMENT ================"
  log "  API:            $API_URL"
  log "  New empty org:  $NEW_ORG_NAME ($NEW_ORG_SLUG)"
  log "  Re-invite list: $MEMBERS_FILE"
  log "  Mode:           $([[ "$DRY_RUN" == 1 ]] && echo DRY-RUN || echo LIVE)"
  log "============================================================"
  if [[ "$DRY_RUN" == "1" ]]; then
    log "  [dry-run] would GET  ${API_URL}/api/v1/users/me (Admin email = the G5D_TOKEN user)"
    log "  [dry-run] would POST ${API_URL}/api/v1/platform/orgs  body={\"name\":\"$NEW_ORG_NAME\",\"slug\":\"$NEW_ORG_SLUG\",\"adminEmail\":\"<admin>\"}"
    log "            (adopting an existing '$NEW_ORG_SLUG' from GET /api/v1/platform/orgs instead)"
    log "  [dry-run] would POST ${API_URL}/api/v1/orgs/<org>/members once per person in $MEMBERS_FILE"
    log "            (body {email, role}; skipping the initial Admin)."
    exit 0
  fi
  [[ -r "$MEMBERS_FILE" ]] || die "Cannot read members file '$MEMBERS_FILE'."
  command -v curl >/dev/null 2>&1 || die "curl is required (platform API calls)."
  command -v jq >/dev/null 2>&1 || die "jq is required (platform API responses)."
  command -v openssl >/dev/null 2>&1 || die "openssl is required (operator JWT preflight)."
  [[ -n "$G5D_TOKEN" ]] || die "G5D_TOKEN is required: sign in at $API_URL (accepting the operator
       invitation), then export a fresh session token as G5D_TOKEN."
  token_preflight
  # POST /orgs/:id/members is gated on Scope::OrgAdmin by scope middleware
  # (checked against the JWT's own claims) before the handler, and there is no
  # wildcard. An operator admin session carries it. Decode the JWT payload
  # (base64url) and check locally before creating the org.
  JWT_PAYLOAD="$(printf '%s' "$G5D_TOKEN" | cut -d. -f2 | tr '_-' '/+')"
  case $(( ${#JWT_PAYLOAD} % 4 )) in 2) JWT_PAYLOAD="${JWT_PAYLOAD}==";; 3) JWT_PAYLOAD="${JWT_PAYLOAD}=";; esac
  JWT_SCOPES="$(printf '%s' "$JWT_PAYLOAD" | openssl base64 -d -A 2>/dev/null | jq -r '.scopes // [] | join(" ")' 2>/dev/null || true)"
  if [[ -n "$JWT_SCOPES" ]]; then
    case " $JWT_SCOPES " in
      *" org:admin "*) log "  ✓ Operator token carries org:admin (re-invites authorized)." ;;
      *) die "G5D_TOKEN lacks 'org:admin', required to re-invite members (POST /orgs/:id/members).
       Use an operator admin session. Token scopes: [${JWT_SCOPES}]." ;;
    esac
  else
    log "  WARN: could not decode operator JWT scopes locally — proceeding on the live 2xx probe."
  fi
  ADMIN_EMAIL="$(token_email)"
  [[ -n "$ADMIN_EMAIL" ]] || die "GET /api/v1/users/me returned no email for G5D_TOKEN."

  log ""
  log "=== [1/2] Create empty org '$NEW_ORG_NAME' via the operator API ==="
  # Adopt an org this step already created (e.g. a lost response) rather than
  # creating a second one; the wipe left no other tenant org.
  find_new_org() {
    api_call GET /api/v1/platform/orgs \
      | jq -r --arg slug "$NEW_ORG_SLUG" '[.data[] | select(.slug == $slug) | .id][0] // empty'
  }
  NEW_ORG_ID="$(find_new_org)"
  if [[ -n "$NEW_ORG_ID" ]]; then
    log "  Adopted existing org '$NEW_ORG_SLUG' (org_id=$NEW_ORG_ID)."
  else
    CREATE_ORG_BODY="$(jq -nc --arg name "$NEW_ORG_NAME" --arg slug "$NEW_ORG_SLUG" --arg admin "$ADMIN_EMAIL" \
      '{name:$name, slug:$slug, adminEmail:$admin}')"
    # One attempt: the cascade commits before its repository bootstrap, so a
    # failed response may still have created the org. Reconcile by slug.
    raw="$(curl -sS -X POST "${API_URL}/api/v1/platform/orgs" \
            -H "Authorization: Bearer ${G5D_TOKEN}" -H "Content-Type: application/json" \
            -d "$CREATE_ORG_BODY" -w $'\n%{http_code}' --max-time 120 2>/dev/null || printf '\n000')"
    CREATE_CODE="${raw##*$'\n'}"; ORG_RESP="${raw%$'\n'*}"
    if [[ "$CREATE_CODE" == "403" ]]; then
      die "POST /api/v1/platform/orgs returned 403: the G5D_TOKEN user is not an operator member.
       Sign in at $API_URL as DEFAULT_ADMIN_EMAIL from step 1 (accepting the operator invitation)."
    fi
    NEW_ORG_ID="$(printf '%s' "$ORG_RESP" | jq -r '.data.org.id // empty' 2>/dev/null || true)"
    if [[ -z "$NEW_ORG_ID" ]]; then
      log "  create HTTP ${CREATE_CODE:-?} returned no org id — checking whether the org committed anyway…"
      NEW_ORG_ID="$(find_new_org)"
      [[ -n "$NEW_ORG_ID" ]] || die "Org creation failed (HTTP ${CREATE_CODE:-?}) and no '$NEW_ORG_SLUG' org exists.
       Re-run --regenerate. Response: $ORG_RESP"
      log "  ⚠ Adopted already-committed org '$NEW_ORG_SLUG' (org_id=$NEW_ORG_ID). Its repository"
      log "    bootstrap may be incomplete; VERIFY Base before relying on it."
      RESET_EXIT=1
    fi
  fi
  log "  ✓ Org '$NEW_ORG_NAME' (org_id=$NEW_ORG_ID, admin=${ADMIN_EMAIL})"

  # Re-invite "people as of today" (the snapshot has Admin/Member/Viewer roles);
  # the initial Admin is already a member and is skipped. Best-effort per person.
  log ""
  log "=== [2/2] Re-invite today's people into '$NEW_ORG_NAME' ==="
  INVITED=0; INVITE_FAILED=0
  # psql -tA emits '|'-separated columns; the snapshot rows are `email|role`.
  while IFS='|' read -r EMAIL ROLE; do
    [[ -n "$EMAIL" ]] || continue
    [[ "$(printf '%s' "$EMAIL" | tr '[:upper:]' '[:lower:]')" == "$(printf '%s' "$ADMIN_EMAIL" | tr '[:upper:]' '[:lower:]')" ]] && continue
    BODY="$(jq -nc --arg email "$EMAIL" --arg role "$ROLE" '{email:$email, role:$role}')"
    code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 30 \
      -X POST "${API_URL}/api/v1/orgs/${NEW_ORG_ID}/members" \
      -H "Authorization: Bearer ${G5D_TOKEN}" -H "Content-Type: application/json" \
      -d "$BODY" || echo 000)"
    if [[ "$code" -ge 200 && "$code" -lt 300 ]]; then
      INVITED=$((INVITED + 1))
    else
      INVITE_FAILED=$((INVITE_FAILED + 1))
      # Do not log the email (PII) — only the role + HTTP code. The full
      # member/invite list is in the pre-wipe snapshot ($MEMBERS_FILE).
      log "  WARN: an invite (${ROLE}) failed → HTTP ${code}"
    fi
  done < "$MEMBERS_FILE"
  if [[ "$INVITE_FAILED" -gt 0 ]]; then
    log "  ⚠ Invited ${INVITED} person(s) into '$NEW_ORG_NAME' (${INVITE_FAILED} FAILED — see WARNs)."
    # The wipe removed the old memberships, so a failed invite is real data
    # loss; re-invite the failures from the snapshot ($MEMBERS_FILE).
    RESET_EXIT=1
  else
    log "  ✓ Invited ${INVITED} person(s) into '$NEW_ORG_NAME'."
  fi
  log ""
  log "=== Regenerate complete ==="
  log "  New org: $NEW_ORG_NAME (id=$NEW_ORG_ID), admin ${ADMIN_EMAIL}"
  log "  Other platform operators are invited to the operator org from its Members page."
  exit "$RESET_EXIT"
fi

# ---------------------------------------------------------------------------
# STEP 1 (default): backup, wipe, platform seed.
# ---------------------------------------------------------------------------
[[ -z "$MEMBERS_FILE_ARG" ]] || die "--members-file is only used with --regenerate."
# --dry-run is a pure offline preview: it never touches gcloud/psql/kubectl, so
# do not require them to be installed.
if [[ "$DRY_RUN" != "1" ]]; then
  command -v gcloud >/dev/null 2>&1 || die "gcloud is required."
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    command -v kubectl >/dev/null 2>&1 || die "kubectl is required when USE_K8S_PSQL=1."
    [[ "$DB_HOST" != "127.0.0.1" ]] || die "Set DB_HOST to the Cloud SQL private IP when USE_K8S_PSQL=1."
    K8S_CONTEXT="${K8S_CONTEXT:-$(kubectl config current-context 2>/dev/null || true)}"
    [[ "$K8S_CONTEXT" == "gke_${GCP_PROJECT}_"* ]] \
      || die "The platform seed needs K8S_CONTEXT (or the current kubectl context) to be a GKE context of $GCP_PROJECT; got '${K8S_CONTEXT}'."
  else
    command -v psql >/dev/null 2>&1 || die "psql is required (or set USE_K8S_PSQL=1)."
    # Guard the destructive default: a bare 127.0.0.1 with neither --proxy nor an
    # in-cluster pod almost certainly points at a LOCAL Postgres, not Cloud SQL.
    # Wiping there would "succeed" against the wrong DB and leave real dev intact
    # (a silent no-op that reads as success). Demand an explicit Cloud SQL path.
    if [[ "$USE_PROXY" != "1" && "$DB_HOST" == "127.0.0.1" && "${ALLOW_LOCAL_DB:-0}" != "1" ]]; then
      die "DB_HOST is 127.0.0.1 with neither --proxy nor USE_K8S_PSQL=1 — this targets a LOCAL
       database, not Cloud SQL '$DB_INSTANCE_NAME'. Use --proxy, or USE_K8S_PSQL=1 with
       DB_HOST=<Cloud SQL private IP>, or set ALLOW_LOCAL_DB=1 if you truly mean localhost."
    fi
  fi
  [[ -r "$SEED_SCRIPT" ]] || die "Platform seed not found at '$SEED_SCRIPT' (set SEED_SCRIPT)."
  # The reset restarts g5-api + the daemon after the wipe so process-local
  # caches do not retain deleted tenant state. Verify connectivity up front.
  if [[ "$RESTART_API" == "1" ]]; then
    command -v kubectl >/dev/null 2>&1 || die "kubectl is required to restart API processes (RESTART_API=1). Point it at g5-dev or set RESTART_API=0."
    kubectl get namespace "$API_K8S_NAMESPACE" >/dev/null 2>&1 \
      || die "kubectl cannot reach namespace '$API_K8S_NAMESPACE' — point it at the g5-dev cluster BEFORE the wipe, or set RESTART_API=0."
  fi
  # The seed invites the first operator. Resolve that email BEFORE anything
  # destructive: an unusable token must fail here, not after the TRUNCATE.
  if [[ -z "$DEFAULT_ADMIN_EMAIL" ]]; then
    [[ -n "$G5D_TOKEN" ]] || die "Set DEFAULT_ADMIN_EMAIL (the operator to invite after the wipe), or
       G5D_TOKEN to use that session's email. Get a token with 'g5d --api-url $API_URL auth login'."
    command -v curl >/dev/null 2>&1 || die "curl is required (platform API calls)."
    command -v jq >/dev/null 2>&1 || die "jq is required (platform API responses)."
    token_preflight
    DEFAULT_ADMIN_EMAIL="$(token_email)"
    [[ -n "$DEFAULT_ADMIN_EMAIL" ]] || die "GET /api/v1/users/me returned no email for G5D_TOKEN."
  fi
  [[ "$DEFAULT_ADMIN_EMAIL" =~ ^[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+$ ]] \
    || die "DEFAULT_ADMIN_EMAIL is not a plain email address: '$DEFAULT_ADMIN_EMAIL'."
fi

# ---------------------------------------------------------------------------
# 1. Start cloud-sql-proxy if requested
# ---------------------------------------------------------------------------
if [[ "$USE_PROXY" == "1" && "$DRY_RUN" != "1" ]]; then
  command -v cloud-sql-proxy >/dev/null 2>&1 || die "cloud-sql-proxy is required for --proxy."
  # Refuse to start if something is ALREADY listening on the port: a stale proxy
  # or a local Postgres that happens to be named '$DB_NAME' could otherwise be
  # wiped instead of Cloud SQL. We must own the listener on this port.
  if lsof -nP -iTCP:"$DB_PORT" -sTCP:LISTEN >/dev/null 2>&1 \
     || (exec 3<>"/dev/tcp/127.0.0.1/$DB_PORT") 2>/dev/null; then
    die "Port ${DB_PORT} on 127.0.0.1 is already in use — refusing to start cloud-sql-proxy
       (a stale proxy or local DB could be wiped instead of Cloud SQL). Stop that
       listener, or set DB_PORT to a free port."
  fi
  log "=== Starting cloud-sql-proxy ==="
  cloud-sql-proxy "${GCP_PROJECT}:${GCP_REGION}:${DB_INSTANCE_NAME}" --port="$DB_PORT" $SQL_PROXY_ARGS &
  PROXY_PID=$!
  log "  PID=$PROXY_PID, waiting for proxy to be ready..."
  sleep 3
  # If the proxy died (port bind / auth failure), abort BEFORE anything
  # destructive — never fall through to a different listener on the same port.
  kill -0 "$PROXY_PID" 2>/dev/null \
    || die "cloud-sql-proxy exited immediately (port bind or auth failure). Aborting before backup/wipe."
fi

# ---------------------------------------------------------------------------
# 2. DB password from Secret Manager (skipped in dry-run if absent)
# ---------------------------------------------------------------------------
if [[ -z "${DB_PASSWORD:-}" && "$DRY_RUN" != "1" ]]; then
  log "=== Fetching DB password from Secret Manager (g5-db-password) ==="
  DB_PASSWORD=$(gcloud secrets versions access latest \
    --secret=g5-db-password --project="$GCP_PROJECT" 2>/dev/null || true)
  if [[ -z "$DB_PASSWORD" ]]; then
    die "Could not fetch g5-db-password from Secret Manager. Set DB_PASSWORD to override."
  fi
  export DB_PASSWORD
fi

# ---------------------------------------------------------------------------
# 3. Confirm target — sanity-check the live DB and require an explicit ack
# ---------------------------------------------------------------------------
log ""
log "================ RESET DEV ENVIRONMENT ================"
log "  Project:        $GCP_PROJECT"
log "  Cloud SQL:      $DB_INSTANCE_NAME ($GCP_REGION)"
log "  Database:       $DB_NAME   (user: $DB_USER)"
log "  Connection:     $([[ "$USE_K8S_PSQL" == 1 ]] && echo "in-cluster psql pod → $DB_HOST" || echo "${DB_HOST}:${DB_PORT}")"
log "  Operator invite: ${DEFAULT_ADMIN_EMAIL:-<resolved from G5D_TOKEN>} (platform seed)"
log "  Backups dir:    $BACKUP_DIR"
log "  GCS export:     ${BACKUP_BUCKET:-<none — on-demand Cloud SQL backup only>}"
log "  Mode:           $([[ "$DRY_RUN" == 1 ]] && echo DRY-RUN || echo LIVE / DESTRUCTIVE)"
log "======================================================"

if [[ "$DRY_RUN" != "1" ]]; then
  CURRENT_DB="$(psql_exec "SELECT current_database();")"
  [[ "$CURRENT_DB" == "$DB_NAME" ]] || die "Connected DB is '$CURRENT_DB', expected '$DB_NAME'. Aborting."
  ORG_COUNT="$(psql_exec "SELECT count(*) FROM orgs;")"
  WS_COUNT="$(psql_exec "SELECT count(*) FROM workspaces;")"
  log "  Live state: ${ORG_COUNT} org(s), ${WS_COUNT} workspace(s) will be DESTROYED."

  if [[ "$ASSUME_YES" != "1" ]]; then
    log ""
    log "  This permanently wipes ALL tenant data in '$DB_NAME' on '$DB_INSTANCE_NAME'."
    printf '  Type the instance name (%s) to proceed: ' "$DB_INSTANCE_NAME" >&2
    read -r CONFIRM
    [[ "$CONFIRM" == "$DB_INSTANCE_NAME" ]] || die "Confirmation did not match. Aborting (nothing changed)."
  fi
fi

mkdir -p "$BACKUP_DIR"
# The local snapshots hold member/invitation email addresses (PII) and BACKUP_DIR
# defaults under /tmp. Lock the dir down before writing so a normal umask (022)
# can't leave them world-readable on a shared host.
chmod 700 "$BACKUP_DIR" 2>/dev/null || true
MEMBERS_FILE="$BACKUP_DIR/members-${GCP_PROJECT}-${TS}.tsv"
ORGS_FILE="$BACKUP_DIR/orgs-${GCP_PROJECT}-${TS}.tsv"

# ---------------------------------------------------------------------------
# 4. BACKUP FIRST (fail-closed). Nothing destructive runs before this succeeds.
# ---------------------------------------------------------------------------
log ""
log "=== [1/3] Backup ==="

# 4a. Local snapshot of orgs + members/invitations (for the audit trail and the
#     re-invite list). Written before the wipe; safe to keep off-repo.
PEOPLE_SQL="
  -- Emit the email with its STORED casing (not lowercased): invite_member looks
  -- existing users up with an exact 'WHERE email = \$1', so a lowercased address
  -- for a mixed-case user would miss the immediate-membership path and recreate
  -- them as a pending invitation. Dedupe case-insensitively, and when the same
  -- person appears as both a member and an invitation prefer the users-table
  -- casing (src=0) so the exact lookup hits. A snapshot from a pre-conversion
  -- database maps historical Owners to Admin; regeneration never assigns Owner.
  SELECT (array_agg(email ORDER BY src))[1] AS email,
         CASE WHEN bool_or(role IN ('admin', 'owner')) THEN 'admin'
              WHEN bool_or(role = 'member')           THEN 'member'
              ELSE 'viewer' END AS role
    FROM (
      SELECT u.email, m.role, 0 AS src
        FROM org_memberships m JOIN users u ON u.id = m.user_id
       -- The platform seed's service user (Base owner of the operator org)
       -- is not a person to re-invite.
       WHERE u.oauth_provider IS DISTINCT FROM 'service'
      UNION ALL
      SELECT email, role, 1 AS src
        FROM org_invitations
       WHERE status = 'pending'
         -- Skip already-expired invitations: the invite flows reject them
         -- (accept_invitation / auto-accept gate on expires_at), so re-issuing
         -- them would only recreate dead rows.
         AND (expires_at IS NULL OR expires_at > now())
    ) people
   WHERE email IS NOT NULL AND email <> ''
   GROUP BY lower(email)
   ORDER BY 1;"

if [[ "$DRY_RUN" == "1" ]]; then
  log "  [dry-run] would snapshot orgs → $ORGS_FILE"
  log "  [dry-run] would snapshot members ∪ pending invitations → $MEMBERS_FILE"
  log "  [dry-run] member/invite query:"
  printf '%s\n' "$PEOPLE_SQL" | sed 's/^/    | /' >&2
else
  psql_exec "SELECT id, slug, name FROM orgs ORDER BY created_at;" > "$ORGS_FILE" || true
  psql_exec "$PEOPLE_SQL" > "$MEMBERS_FILE"
  chmod 600 "$ORGS_FILE" "$MEMBERS_FILE" 2>/dev/null || true
  PEOPLE_COUNT="$(grep -c . "$MEMBERS_FILE" || true)"
  log "  Snapshotted $(grep -c . "$ORGS_FILE" || echo 0) org(s) → $ORGS_FILE"
  log "  Snapshotted ${PEOPLE_COUNT} distinct person(s) to re-invite → $MEMBERS_FILE"
fi

# 4b. On-demand Cloud SQL backup (server-side, restorable via gcloud). MANDATORY.
BACKUP_DESC="pre-reset-dev ${TS} (reset-dev-environment.sh)"
if [[ "$DRY_RUN" == "1" ]]; then
  log "  [dry-run] would run: gcloud sql backups create --instance=$DB_INSTANCE_NAME --project=$GCP_PROJECT --description=\"$BACKUP_DESC\""
else
  log "  Creating on-demand Cloud SQL backup (this may take a few minutes)..."
  gcloud sql backups create \
    --instance="$DB_INSTANCE_NAME" \
    --project="$GCP_PROJECT" \
    --description="$BACKUP_DESC" \
    || die "Backup failed — refusing to wipe. Nothing was changed."
  log "  ✓ On-demand backup created. List with: gcloud sql backups list --instance=$DB_INSTANCE_NAME --project=$GCP_PROJECT"
fi

# 4c. Optional full SQL export to GCS (extra safety net / portable dump).
if [[ -n "$BACKUP_BUCKET" ]]; then
  EXPORT_URI="gs://${BACKUP_BUCKET}/dev-reset-${TS}.sql.gz"
  if [[ "$DRY_RUN" == "1" ]]; then
    log "  [dry-run] would run: gcloud sql export sql $DB_INSTANCE_NAME $EXPORT_URI --database=$DB_NAME --project=$GCP_PROJECT"
  else
    log "  Exporting SQL dump → $EXPORT_URI ..."
    gcloud sql export sql "$DB_INSTANCE_NAME" "$EXPORT_URI" \
      --database="$DB_NAME" --project="$GCP_PROJECT" \
      || die "GCS SQL export failed — refusing to wipe. Nothing was changed."
    log "  ✓ SQL dump exported → $EXPORT_URI"
  fi
fi

# 4d. Filestore repos backup (git repo content; NOT covered by the SQL backup).
# Fail-closed like the SQL backup. dev's BASIC_HDD tier supports backups (not
# snapshots). The backup is kept; delete later with `gcloud filestore backups
# delete`. Billed on used bytes (~$0.08/GiB-month).
if [[ "$BACKUP_REPOS" == "1" ]]; then
  # Filestore backup IDs must match ^[a-z][-a-z0-9]{0,61}[a-z0-9]$ (all lowercase,
  # no trailing hyphen). TS ends in an uppercase 'Z', so lowercase the whole id.
  REPOS_BACKUP_ID="$(printf 'dev-repos-pre-reset-%s' "$TS" | tr '[:upper:]' '[:lower:]')"
  if [[ "$DRY_RUN" == "1" ]]; then
    log "  [dry-run] would run: gcloud filestore backups create $REPOS_BACKUP_ID --instance=$FILESTORE_INSTANCE --instance-zone=$FILESTORE_ZONE --file-share=$FILESTORE_SHARE --region=$FILESTORE_BACKUP_REGION --project=$GCP_PROJECT"
  else
    log "  Creating Filestore repos backup '$REPOS_BACKUP_ID' (this may take a few minutes)..."
    gcloud filestore backups create "$REPOS_BACKUP_ID" \
      --instance="$FILESTORE_INSTANCE" \
      --instance-zone="$FILESTORE_ZONE" \
      --file-share="$FILESTORE_SHARE" \
      --region="$FILESTORE_BACKUP_REGION" \
      --project="$GCP_PROJECT" \
      --description="pre-reset-dev ${TS} (reset-dev-environment.sh)" \
      || die "Filestore repos backup failed — refusing to wipe. Nothing was changed."
    log "  ✓ Filestore backup '$REPOS_BACKUP_ID' created. List with: gcloud filestore backups list --project=$GCP_PROJECT --region=$FILESTORE_BACKUP_REGION"
  fi
fi

# ---------------------------------------------------------------------------
# 4d. Deallocate API-managed worker host VMs BEFORE the wipe. TRUNCATE removes
# their worker_hosts rows via the org FK without the app's terminate path, so
# any API-provisioned GCE instances would keep running with no DB record for the
# reconciler/UI/DELETE route to reap — a cloud leak. Capture + best-effort
# terminate them first (manual command printed if a teardown can't be issued).
# ---------------------------------------------------------------------------
WORKER_HOSTS_FILE="$BACKUP_DIR/worker-hosts-${GCP_PROJECT}-${TS}.tsv"
if [[ "$DRY_RUN" == "1" ]]; then
  log ""
  log "  [dry-run] would enumerate non-terminated worker_hosts with a cloud_instance_id"
  log "            and best-effort 'gcloud compute instances delete' each before the wipe."
else
  psql_exec "SELECT cloud_instance_id, region, status FROM worker_hosts
             WHERE cloud_instance_id IS NOT NULL AND status <> 'terminated';" > "$WORKER_HOSTS_FILE" || true
  chmod 600 "$WORKER_HOSTS_FILE" 2>/dev/null || true
  if [[ -s "$WORKER_HOSTS_FILE" ]]; then
    log ""
    log "  Deallocating $(grep -c . "$WORKER_HOSTS_FILE") API-managed worker host VM(s) (snapshot: $WORKER_HOSTS_FILE)"
    while IFS='|' read -r INSTID _; do
      [[ -n "$INSTID" ]] || continue
      # The GCP provider persists cloud_instance_id as "zone/name" (gcp.rs encodes
      # it that way and decodes before terminate); older/other rows may hold a bare
      # name. Honor the zone-qualified form directly; fall back to a name lookup.
      if [[ "$INSTID" == */* ]]; then
        ZONE="${INSTID%%/*}"; NAME="${INSTID##*/}"
      else
        NAME="$INSTID"
        ZONE="$(gcloud compute instances list --project="$GCP_PROJECT" \
                --filter="name=$NAME" --format='value(zone)' 2>/dev/null | head -1)"
      fi
      if [[ -n "$ZONE" ]]; then
        if gcloud compute instances delete "$NAME" --project="$GCP_PROJECT" \
             --zone="$ZONE" --quiet >/dev/null 2>&1; then
          log "  ✓ Terminated worker host VM '$NAME' ($ZONE)"
        else
          log "  WARN: could not terminate '$NAME' ($ZONE). Delete it manually:"
          log "        gcloud compute instances delete $NAME --zone=$ZONE --project=$GCP_PROJECT"
        fi
      else
        log "  NOTE: worker host '$NAME' not found in compute (already gone) — skipping."
      fi
    done < "$WORKER_HOSTS_FILE"
  fi
fi

# ---------------------------------------------------------------------------
# 5. WIPE — TRUNCATE orgs CASCADE (admin g5 bypasses RLS). Clear auth sessions.
# ---------------------------------------------------------------------------
# A plain DELETE FROM orgs fails: ~35 child tables FK orgs(id) WITHOUT cascade
# (see the FOREIGN KEY (org_id) constraints in 001_lean_baseline.sql). TRUNCATE
# CASCADE truncates every referencing table transitively, regardless of the
# ON DELETE clause. `users` and `oauth_scopes` carry no FK to orgs and are
# preserved on purpose so the same people can be re-invited.
log ""
log "=== [2/3] Wipe all tenant data ==="
psql_write "TRUNCATE orgs CASCADE (+ clear auth sessions)" "
  BEGIN;
  SET LOCAL statement_timeout = '120s';
  TRUNCATE TABLE orgs RESTART IDENTITY CASCADE;
  -- Auth sessions/contexts have no orgs FK; clear them so no stale session
  -- references a wiped org. (users + oauth_scopes are intentionally kept.)
  TRUNCATE TABLE authz.user_sessions RESTART IDENTITY CASCADE;
  TRUNCATE TABLE authz.session_ctx;
  COMMIT;
"
log "  ✓ Tenant data truncated (orgs + all org-referencing tables; users kept)."

# ---------------------------------------------------------------------------
# 5a. PLATFORM SEED — the wipe removed the operator org too. Re-run the
# platform seed (g5.organization.platform-operator.bootstrap) on the same
# connection: it creates only the operator org and invites DEFAULT_ADMIN_EMAIL
# as its admin. Tenant orgs are created afterwards by that operator (step 2).
# ---------------------------------------------------------------------------
log ""
log "=== [3/3] Platform seed: operator org + operator invitation ==="
# DB_PASSWORD reaches the seed through the exported environment, never argv.
SEED_ENV=(
  GCP_PROJECT="$GCP_PROJECT" GCP_REGION="$GCP_REGION" DB_INSTANCE_NAME="$DB_INSTANCE_NAME"
  DB_NAME="$DB_NAME" DB_USER="$DB_USER" DB_HOST="$DB_HOST" DB_PORT="$DB_PORT"
  DEFAULT_ADMIN_EMAIL="$DEFAULT_ADMIN_EMAIL"
  USE_K8S_PSQL="$USE_K8S_PSQL" K8S_NAMESPACE="$API_K8S_NAMESPACE"
  K8S_SQL_NAMESPACE="$K8S_SQL_NAMESPACE" K8S_SQL_POD_IMAGE="$K8S_SQL_POD_IMAGE"
)
[[ "$USE_K8S_PSQL" == "1" ]] && SEED_ENV+=(K8S_CONTEXT="$K8S_CONTEXT")
if [[ "$DRY_RUN" == "1" ]]; then
  log "  [dry-run] would run: GRANT g5_app TO $DB_USER (the seed assumes g5_app for orgs rows)"
  log "  [dry-run] would run: $SEED_SCRIPT with GCP_PROJECT=$GCP_PROJECT DB_NAME=$DB_NAME USE_K8S_PSQL=$USE_K8S_PSQL"
  log "            DEFAULT_ADMIN_EMAIL=<operator> (operator org, docs repo, Base, admin invitation)"
else
  psql_write "GRANT g5_app TO $DB_USER" "GRANT g5_app TO \"$DB_USER\";" >/dev/null
  # The seed connects separately; a failure leaves dev wiped with no operator
  # org, which re-running this same seed command repairs.
  env "${SEED_ENV[@]}" bash "$SEED_SCRIPT" \
    || die "Platform seed failed after the wipe. Re-run it (idempotent):
       env $(printf '%q ' "${SEED_ENV[@]}")bash $(printf '%q' "$SEED_SCRIPT")"
  log "  ✓ Operator org seeded; ${DEFAULT_ADMIN_EMAIL} has a pending operator admin invitation."
  if [[ "$USE_K8S_PSQL" != "1" ]]; then
    log "  NOTE: without USE_K8S_PSQL=1 the seed could not initialise the operator repository mirrors or"
    log "        image policy (see its WARN lines). Re-run it with USE_K8S_PSQL=1 to finish them."
  fi
fi

# ---------------------------------------------------------------------------
# 5b. Restart API processes after the tenant reset so their in-memory caches
# cannot retain rows from the deleted organizations. This also gives the
# operator's sign-in (step 2) a clean, healthy g5-api. Needs kubectl on the
# g5-dev cluster. Run the reset during a quiet window.
# ---------------------------------------------------------------------------
if [[ "$RESTART_API" == "1" ]]; then
  log ""
  log "=== Reset API process caches (restart g5-api + daemon) ==="
  if [[ "$DRY_RUN" == "1" ]]; then
    log "  [dry-run] would: kubectl rollout restart deployment/<dep> -n $API_K8S_NAMESPACE for [$API_DEPLOYMENTS]"
    log "            then wait for each rollout + the public endpoint before stopping for sign-in."
  else
    # kubectl connectivity was preflighted BEFORE the wipe. Failures here are
    # FATAL: the DB is already truncated, and an un-restarted API or daemon
    # can retain tenant state from before the reset.
    for dep in $API_DEPLOYMENTS; do
      kubectl rollout restart "deployment/$dep" -n "$API_K8S_NAMESPACE" >/dev/null 2>&1 \
        || die "Could not restart deployment/$dep (ns $API_K8S_NAMESPACE) after the wipe. Restart it manually, then continue with step 2 (--regenerate)."
    done
    log "  ✓ Restart requested for [$API_DEPLOYMENTS] (ns $API_K8S_NAMESPACE)"
    # Every deployment (g5-api AND the daemon) must actually roll out. The public
    # healthz below only reflects g5-api, so a stale daemon would otherwise slip
    # past — treat a rollout-status timeout as fatal too.
    for dep in $API_DEPLOYMENTS; do
      kubectl rollout status "deployment/$dep" -n "$API_K8S_NAMESPACE" --timeout=180s >/dev/null 2>&1 \
        && log "  ✓ $dep rollout ready" \
        || die "deployment/$dep did not roll out within 180s after the wipe. Verify it (kubectl rollout status deployment/$dep -n $API_K8S_NAMESPACE),
       then continue with step 2 (--regenerate), or restore from the pre-reset backup."
    done
    # Pods ready != the public ingress/NEG routes to them yet. Sign-in and
    # step 2 go through $API_URL, so wait for the PUBLIC endpoint to serve 200
    # (else it returns a bare 502 mid-reroute).
    log "  waiting for $API_URL to serve healthy through the ingress..."
    API_HEALTHY=0
    for _ in $(seq 1 60); do
      hcode="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 "$API_URL/healthz" 2>/dev/null || true)"
      if [[ "${hcode:-000}" == "200" ]]; then log "  ✓ $API_URL healthy"; API_HEALTHY=1; break; fi
      sleep 5
    done
    # Do NOT report the wipe as ready against an unhealthy API.
    if [[ "$API_HEALTHY" != "1" ]]; then
      die "g5-api did not become healthy at $API_URL/healthz after the restart (last HTTP ${hcode:-000}).
       The DB is already wiped; do NOT assume dev is reset. Verify the rollout, then
       continue with step 2 (--regenerate), or restore from the pre-reset backup."
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 7. Summary
# ---------------------------------------------------------------------------
log ""
log "=== Wipe complete$([[ "$DRY_RUN" == 1 ]] && echo " (DRY-RUN — nothing changed)") ==="
log "  Project / DB:   $GCP_PROJECT / $DB_NAME on $DB_INSTANCE_NAME"
log "  Operator org:   seeded; operator admin invitation for ${DEFAULT_ADMIN_EMAIL:-<operator>}"
log "  Backups:        on-demand Cloud SQL backup${BACKUP_BUCKET:+ + gs://$BACKUP_BUCKET/dev-reset-${TS}.sql.gz}$([[ "$BACKUP_REPOS" == 1 ]] && echo " + Filestore backup dev-repos-pre-reset-${TS}")"
log "  Local snapshot: $MEMBERS_FILE"
if [[ "$DRY_RUN" != "1" ]]; then
  log ""
  log "  Restore (if needed):"
  log "    gcloud sql backups list   --instance=$DB_INSTANCE_NAME --project=$GCP_PROJECT"
  log "    gcloud sql backups restore <BACKUP_ID> --restore-instance=$DB_INSTANCE_NAME --project=$GCP_PROJECT"
fi

# ---------------------------------------------------------------------------
# 8. Orphaned-resource reminder. The wipe deletes Postgres metadata only — it
# leaves orphaned per-org VPC networks (+ Cloud NAT) and API-managed worker VMs
# running and billing. Point the operator at the interactive sweep that cleans
# them up (see sweep-dev-cloud-resources.README.md).
# ---------------------------------------------------------------------------
log ""
log "--------------------------------------------------------------------"
log "--------------------------------------------------------------------"
log "  !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
log "  The wipe leaves orphaned cloud resources (per-org VPCs + Cloud NAT,"
log "  API-managed worker VMs) RUNNING and billing. Please run the cleanup"
log "  script for orphaned resources:"
log ""
log "      ./scripts/sweep-dev-cloud-resources.sh --dry-run     # preview"
log "      CHECK_DB=1 USE_K8S_PSQL=1 DB_HOST=<cloud-sql-ip> \\"
log "        ./scripts/sweep-dev-cloud-resources.sh             # interactive"
log ""
log "  Read ./scripts/sweep-dev-cloud-resources.README.md first."
log "  !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
log "--------------------------------------------------------------------"
log "--------------------------------------------------------------------"

# ---------------------------------------------------------------------------
# 9. Stop: step 2 runs as the operator after signing in again.
# ---------------------------------------------------------------------------
log ""
log "  NEXT (step 2): sign in at $API_URL as ${DEFAULT_ADMIN_EMAIL:-<operator>}, which accepts the operator"
log "  invitation, export that session's token as G5D_TOKEN, then create '$NEW_ORG_NAME' and re-invite:"
log ""
log "      G5D_TOKEN=<fresh token> ./scripts/reset-dev-environment.sh --regenerate \\"
log "        --members-file=$MEMBERS_FILE"

exit "$RESET_EXIT"
