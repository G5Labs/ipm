#!/usr/bin/env bash
# =============================================================================
# sweep-dev-cloud-resources.sh — Interactive sweep of orphaned/stray dev cloud
# resources, asking 1-by-1 before deleting.
# =============================================================================
# Companion to reset-dev-environment.sh. That reset wipes Postgres metadata only;
# it deliberately leaves cloud-side artifacts behind (see its README "known
# limitations") and KEEPS backups. This script sweeps those leftovers:
#
#   1. ALL managed instance groups   — gcloud compute instance-groups managed list
#   2. ALL VM instances              — gcloud compute instances list
#   3. ALL VPC networks              — gcloud compute networks list (+ cascade for
#                                      per-org ones: instances, firewalls, NAT, subnets)
#   4. Cloud SQL backups             — gcloud sql backups list   (⚠ restore points)
#   5. Filestore backups             — gcloud filestore backups list
#   6. GCS reset exports             — gs://$BACKUP_BUCKET/dev-reset-*.sql.gz (optional)
#
# "List everything" model: it enumerates ALL resources (NO name filter) so a leaked
# resource can never be hidden by a naming convention, then CLASSIFIES each:
#   - CONTROL PLANE  (GKE infra, shared/default networks) → PROTECTED, always.
#   - STANDING HOST  (a tenant's worker host)             → PROTECTED unless SWEEP_PROTECTED=1.
#   - PER-ORG        (g5_org_id label / g5-org-<uuid> net)→ orphan-checked: live=PROTECTED,
#                                                           deleted=offered.
#   - UNCLASSIFIED   (none of the above)                  → OFFERED with a loud label.
# Each offered item shows an estimated monthly cost and prompts y/N — NOTHING is
# deleted without an explicit "y". There is no blanket --yes. Use --dry-run to list
# everything (with classification + cost) without prompting or deleting.
#
# Orphan/registration annotation (opt-in): with CHECK_DB=1 it reads the live `orgs`
# and `worker_hosts` tables once (same connection model as reset-dev-environment.sh)
# to resolve org slugs, ORPHAN/IN-USE status, and REGISTERED/NOT-REGISTERED. Without
# CHECK_DB, per-org items are "unverified" — protected by name/label where possible.
#
# Like the reset, it refuses a project whose id looks like prod (ALLOW_PROD=1 to
# override) and targets DEV by default.
#
# Usage:
#   ./scripts/sweep-dev-cloud-resources.sh --dry-run        # list + cost, no prompts
#   ./scripts/sweep-dev-cloud-resources.sh                  # interactive sweep
#   ./scripts/sweep-dev-cloud-resources.sh --only backups   # one category
#   CHECK_DB=1 USE_K8S_PSQL=1 DB_HOST=10.x.x.x ./scripts/sweep-dev-cloud-resources.sh
#
# Categories for --only: workers | networks | backups   (repeatable / comma-sep)
# =============================================================================
set -euo pipefail

# Prefer Homebrew-installed clients when not on the default PATH (psql for CHECK_DB).
if command -v brew >/dev/null 2>&1; then
  for formula in postgresql@16 postgresql@15 postgresql libpq; do
    prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
    [[ -n "$prefix" && -d "$prefix/bin" ]] && PATH="$prefix/bin:$PATH"
  done
  export PATH
fi

# ---------------------------------------------------------------------------
# Configuration (override via env) — defaults target DEV.
# ---------------------------------------------------------------------------
GCP_PROJECT="${GCP_PROJECT:-g5-dev-1ccc}"
GCP_REGION="${GCP_REGION:-us-central1}"

# Resource discovery filters.
# "List everything" model: the sweep enumerates ALL compute instances, MIGs and
# networks in the project (NO name filter) so a leaked resource can never be
# hidden by a naming convention. Each item is then CLASSIFIED:
#   - CONTROL PLANE  — GKE infra (label goog-gke/goog-k8s or name ~CONTROL_PLANE_REGEX),
#                      and non-per-org networks (default / shared VPC). Always protected.
#   - STANDING HOST  — a tenant's worker host (label component=*worker-host*, or name
#                      g5-<tenant>-host-<ws>). Protected unless SWEEP_PROTECTED=1.
#   - PER-ORG        — has a g5_org_id label or lives in a g5-org-<uuid> network;
#                      orphan-checked against the live orgs table (live→protected,
#                      deleted→offered).
#   - UNCLASSIFIED   — none of the above: OFFERED with a loud label so a genuine
#                      leak is never silently skipped.
ORG_NET_PREFIX="g5-org-"
CONTROL_PLANE_REGEX="${CONTROL_PLANE_REGEX:-^gke-}"   # extra always-protected name prefixes
SWEEP_PROTECTED="${SWEEP_PROTECTED:-0}"               # also offer standing worker hosts
# Per-org resources whose org can't be confirmed (CHECK_DB off / DB unreachable)
# are PROTECTED by default — an [unverified] g5-org-* could be a LIVE org, and its
# cascade is destructive. Set SWEEP_UNVERIFIED=1 to offer them without a DB check.
SWEEP_UNVERIFIED="${SWEEP_UNVERIFIED:-0}"

# Backups. By default the sweep only offers RESET-CREATED backups (the unbounded
# leftovers) — not unrelated manual/managed restore points. SWEEP_ALL_BACKUPS=1
# offers every backup (still per-item confirmed).
DB_INSTANCE_NAME="${DB_INSTANCE_NAME:-g5-dev-postgres}"
FILESTORE_BACKUP_REGION="${FILESTORE_BACKUP_REGION:-$GCP_REGION}"
BACKUP_BUCKET="${BACKUP_BUCKET:-}"                     # for GCS reset exports
SWEEP_ALL_BACKUPS="${SWEEP_ALL_BACKUPS:-0}"
# reset-created markers: Cloud SQL description substring + Filestore name prefix.
RESET_SQL_BACKUP_MATCH="${RESET_SQL_BACKUP_MATCH:-reset-dev-environment.sh}"
RESET_FILESTORE_BACKUP_REGEX="${RESET_FILESTORE_BACKUP_REGEX:-^dev-repos-pre-reset-}"

# Optional DB orphan check (annotate g5-org-* networks against the live orgs table).
CHECK_DB="${CHECK_DB:-0}"
DB_HOST="${DB_HOST:-127.0.0.1}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:-g5}"
DB_NAME="${DB_NAME:-g5_dev}"
USE_K8S_PSQL="${USE_K8S_PSQL:-0}"
K8S_SQL_NAMESPACE="${K8S_SQL_NAMESPACE:-default}"
K8S_SQL_POD_IMAGE="${K8S_SQL_POD_IMAGE:-postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b}"
K8S_SQL_POD_NAME="${K8S_SQL_POD_NAME:-sweep-dev-psql-$$}"

# Cost-estimate rates (USD, us-central1, order-of-magnitude only). Used solely to
# inform the per-item prompt; never authoritative.
HOURS_PER_MONTH=730
RATE_FILESTORE_GIB_MONTH="0.08"   # Filestore backup storage
RATE_SQL_BACKUP_GIB_MONTH="0.08"  # Cloud SQL backup storage
RATE_GCS_GIB_MONTH="0.02"         # GCS standard storage
RATE_NAT_GW_HOUR="0.044"          # Cloud NAT gateway

# Flags
DRY_RUN=0
ONLY_GIVEN=0
declare -a ONLY=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run|-n) DRY_RUN=1 ;;
    --only)       shift; ONLY_GIVEN=1; IFS=',' read -r -a _o <<< "${1:-}"; ONLY+=(${_o[@]+"${_o[@]}"}) ;;
    --only=*)     ONLY_GIVEN=1; IFS=',' read -r -a _o <<< "${1#*=}"; ONLY+=(${_o[@]+"${_o[@]}"}) ;;
    -h|--help)    sed -n '2,46p' "$0"; exit 0 ;;
    *) echo "Unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done
# Reject a present-but-empty --only (e.g. `--only=` or `--only ""`): it would
# leave ONLY empty and silently broaden a scoped run to ALL categories.
if [[ "$ONLY_GIVEN" == "1" && ${#ONLY[@]} -eq 0 ]]; then
  echo "--only was given with no category — specify workers, networks, and/or backups (comma-separated)" >&2; exit 2
fi
# Fail fast on a typo'd --only value (else the run is a silent no-op).
for _c in ${ONLY[@]+"${ONLY[@]}"}; do
  case "$_c" in
    workers|networks|backups) ;;
    *) echo "Invalid --only category '${_c}' (valid: workers, networks, backups)" >&2; exit 2 ;;
  esac
done

log()  { echo "$*" >&2; }
die()  { echo "ERROR: $*" >&2; exit 1; }

# Should a category run? (empty ONLY ⇒ all.)
want() {
  [[ ${#ONLY[@]} -eq 0 ]] && return 0
  local c
  for c in "${ONLY[@]}"; do [[ "$c" == "$1" ]] && return 0; done
  return 1
}

# control_plane_reason <name> <labels> → reason string if this is must-protect
# control-plane infra (always protected, no SWEEP_PROTECTED override), else "".
control_plane_reason() {
  local name="$1" labels="${2:-}"
  case "$labels" in *goog-gke*|*goog-k8s*) echo "GKE-managed"; return ;; esac
  [[ "$name" =~ $CONTROL_PLANE_REGEX ]] && { echo "control plane (CONTROL_PLANE_REGEX)"; return; }
  echo ""
}

# is_standing_host <name> <labels> → true for a tenant's Terraform worker host
# (label component=*worker-host*, or name g5-<tenant>-host-<ws>). Protected
# unless SWEEP_PROTECTED=1; never matches API-managed g5-host-<uuid> instances.
is_standing_host() { [[ "${2:-}" == *worker-host* || "$1" =~ ^g5-[a-z0-9]+-host- ]]; }

# org_uuid_from <labels> <network> → owning org id (prefers the authoritative
# g5_org_id instance label; falls back to the g5-org-<uuid> network name). Empty
# if neither is present.
org_uuid_from() {
  local labels="${1:-}" net="${2:-}" u=""
  case "$labels" in *g5_org_id=*) u="${labels##*g5_org_id=}"; u="${u%%;*}" ;; esac
  [[ -z "$u" && "$net" == ${ORG_NET_PREFIX}* ]] && u="${net#"$ORG_NET_PREFIX"}"
  printf '%s' "$u"
}

# bytes → "<g> GiB" and a "$<cost>/mo" string at the given $/GiB-month rate.
bytes_to_gib()  { awk -v b="${1:-0}" 'BEGIN{printf "%.2f", (b+0)/1073741824}'; }
gib_month_cost() { awk -v g="${1:-0}" -v r="$2" 'BEGIN{printf "%.2f", (g+0)*r}'; }

# Tally.
DELETED=0
SKIPPED=0
FAILED=0
DISCOVERY_FAILED=0
_migs=""; _vms=""; _nets=""; _sqlb=""; _fsb=""; _gcs=""   # populated by glist_into (printf -v); declared for shellcheck

# glist_into <varname> <gcloud-list-cmd...> — capture a discovery list into the
# named variable IN THE PARENT SHELL. On failure (missing IAM, disabled API, bad
# filter) WARN loudly and set DISCOVERY_FAILED so a failed list can't masquerade
# as "nothing to clean". MUST be called outside a process substitution (so the
# global persists) and feed its loop via a here-string (`<<< "$var"`).
#
# The `if var=$(...)` form is deliberate: set -e is ignored in an if-condition,
# so a gcloud failure takes the else branch instead of aborting the script.
glist_into() {
  local __var="$1"; shift
  local __out
  if __out="$("$@" 2>/dev/null)"; then
    printf -v "$__var" '%s' "$__out"
  else
    DISCOVERY_FAILED=1
    log "  WARN: resource discovery FAILED — results may be INCOMPLETE (a leak could be hidden):"
    log "        $* (check IAM / API enablement / filter)"
    printf -v "$__var" '%s' ""
  fi
}

# ---------------------------------------------------------------------------
# Guards + tooling
# ---------------------------------------------------------------------------
if [[ "$GCP_PROJECT" == *prod* && "${ALLOW_PROD:-0}" != "1" ]]; then
  die "Refusing to run against project '$GCP_PROJECT' (looks like prod). Set ALLOW_PROD=1 to override."
fi
command -v gcloud >/dev/null 2>&1 || die "gcloud is required."
if [[ -n "$BACKUP_BUCKET" ]]; then
  command -v gcloud >/dev/null 2>&1 || die "gcloud (storage) is required for the GCS sweep."
fi

# ---------------------------------------------------------------------------
# Optional DB connection (CHECK_DB=1) — minimal, mirrors reset-dev-environment.sh.
# Returns existing org ids on stdout (one per line). Best-effort: on any failure
# it logs a warning and the caller falls back to "unverified" annotations.
# ---------------------------------------------------------------------------
K8S_SQL_POD_CREATED="false"
cleanup() {
  if [[ "$K8S_SQL_POD_CREATED" == "true" ]]; then
    kubectl delete pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" --ignore-not-found >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

# Create the in-cluster psql pod in the PARENT shell. It MUST NOT be created
# inside a $(...) command substitution: the K8S_SQL_POD_CREATED=true assignment
# would happen in a subshell and be lost, so the EXIT trap wouldn't clean up and
# the pod would linger for an hour. Idempotent.
ensure_k8s_psql_pod() {
  [[ "$USE_K8S_PSQL" == "1" ]] || return 0
  kubectl get pod "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" >/dev/null 2>&1 && return 0
  kubectl run "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" --image="$K8S_SQL_POD_IMAGE" \
    --restart=Never --command -- sh -c "sleep 3600" >/dev/null
  K8S_SQL_POD_CREATED="true"
  kubectl wait --for=condition=Ready "pod/$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" --timeout=180s >/dev/null
}

_psql() {
  local sql="$1"
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    printf '%s\n' "$sql" | kubectl exec -i "$K8S_SQL_POD_NAME" -n "$K8S_SQL_NAMESPACE" -- \
      env PGPASSWORD="${DB_PASSWORD:-}" psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA
  else
    PGPASSWORD="${DB_PASSWORD:-}" psql -q -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tA <<SQL
$sql
SQL
  fi
}

ORG_SLUG_MAP=""       # lines: <org-uuid>|<slug>
WORKER_HOST_MAP=""    # lines: <cloud_instance_id>|<status>|<last_heartbeat>
DB_CHECK_OK=0
init_db_check() {
  [[ "$CHECK_DB" == "1" ]] || return 0
  # With USE_K8S_PSQL=1, psql runs INSIDE the in-cluster pod (only kubectl is
  # needed locally); otherwise a local psql client is required.
  if [[ "$USE_K8S_PSQL" == "1" ]]; then
    command -v kubectl >/dev/null 2>&1 || { log "  NOTE: CHECK_DB=1 with USE_K8S_PSQL=1 but kubectl not found — orphan annotation disabled."; return 0; }
  else
    command -v psql >/dev/null 2>&1 || { log "  NOTE: CHECK_DB=1 but psql not found locally (use USE_K8S_PSQL=1 for the in-cluster path) — orphan annotation disabled."; return 0; }
    # CRITICAL: annotations drive destructive prompts. A loopback host with no
    # --proxy/USE_K8S_PSQL almost certainly hits a LOCAL g5_dev, whose org ids
    # would mislabel LIVE cloud networks/hosts as [ORPHAN] and make a destructive
    # prompt look safe. Refuse to annotate from any loopback alias (127.x /
    # localhost / ::1) — fall back to "unverified" (protected) — unless allowed.
    case "$DB_HOST" in 127.*|localhost|::1|"[::1]"|"") _db_local=1 ;; *) _db_local=0 ;; esac
    if [[ "$_db_local" == "1" && "${ALLOW_LOCAL_DB:-0}" != "1" ]]; then
      log "  NOTE: CHECK_DB=1 but DB_HOST='${DB_HOST}' is loopback with no proxy/USE_K8S_PSQL — refusing"
      log "        to annotate from a possibly-LOCAL DB (could mislabel live cloud resources as ORPHAN)."
      log "        Use USE_K8S_PSQL=1 + the Cloud SQL private IP, or set ALLOW_LOCAL_DB=1."
      return 0
    fi
  fi
  if [[ -z "${DB_PASSWORD:-}" ]]; then
    DB_PASSWORD="$(gcloud secrets versions access latest --secret=g5-db-password --project="$GCP_PROJECT" 2>/dev/null || true)"
    export DB_PASSWORD
  fi
  # Best-effort: a pod RBAC/image/timeout failure must NOT abort the sweep (DB
  # annotation is optional and falls back to gcloud-only). The `if !` also keeps
  # set -e from aborting on the kubectl run/wait inside.
  if ! ensure_k8s_psql_pod; then
    log "  NOTE: could not set up the in-cluster psql pod (RBAC/image/timeout) — orphan annotation disabled (gcloud-only)."
    return 0
  fi
  # Org id→slug (orphan status + human name). worker_hosts.org_id has ON DELETE
  # CASCADE, so a row exists only for a still-live org's host = "registered".
  # normalized-id (hyphens stripped, to match the g5_org_id label form) | full id | slug
  if ORG_SLUG_MAP="$(_psql "SELECT replace(id::text,'-','') || '|' || id || '|' || slug FROM orgs;" 2>/dev/null)"; then
    DB_CHECK_OK=1
    WORKER_HOST_MAP="$(_psql "SELECT coalesce(cloud_instance_id,'') || '|' || status || '|' || coalesce(to_char(last_heartbeat,'YYYY-MM-DD HH24:MI'),'never') FROM worker_hosts WHERE cloud_instance_id IS NOT NULL;" 2>/dev/null || true)"
    log "  DB check enabled ($(printf '%s' "$ORG_SLUG_MAP" | grep -c . || true) live org(s), $(printf '%s' "$WORKER_HOST_MAP" | grep -c . || true) registered worker host(s))."
  else
    log "  NOTE: CHECK_DB=1 but the orgs query failed — orphan annotation disabled (proceeding gcloud-only)."
  fi
}

# org_status_for <org-uuid> → "ORPHAN" | "IN USE" | "unverified"
# Accepts the uuid in either hyphenated (network) or stripped (label) form.
org_status_for() {
  local u; u="$(printf '%s' "${1:-}" | tr -d '-')"
  [[ "$DB_CHECK_OK" == "1" && -n "$u" ]] || { echo "unverified"; return; }
  if printf '%s\n' "$ORG_SLUG_MAP" | grep -qiF -- "$u|"; then echo "IN USE"; else echo "ORPHAN"; fi
}

# org_slug_for <org-uuid> → slug, or "" if not found / no DB check.
org_slug_for() {
  local u; u="$(printf '%s' "${1:-}" | tr -d '-')"
  [[ "$DB_CHECK_OK" == "1" && -n "$u" ]] || { echo ""; return; }
  printf '%s\n' "$ORG_SLUG_MAP" | awk -F'|' -v u="$u" 'tolower($1)==tolower(u){print $3; exit}'
}

# worker_registration_for <zone/name> → registration label for the prompt.
worker_registration_for() {
  [[ "$DB_CHECK_OK" == "1" ]] || { echo "registration unknown (set CHECK_DB=1)"; return; }
  local row; row="$(printf '%s\n' "$WORKER_HOST_MAP" | awk -F'|' -v k="$1" '$1==k{print; exit}')"
  if [[ -n "$row" ]]; then
    echo "REGISTERED (status=$(printf '%s' "$row" | cut -d'|' -f2), heartbeat $(printf '%s' "$row" | cut -d'|' -f3))"
  else
    echo "NOT REGISTERED (no worker_hosts row)"
  fi
}

# ---------------------------------------------------------------------------
# Prompt helper. Prints the item + cost, asks y/N (unless --dry-run). Returns 0
# to proceed with deletion, 1 to skip. Honors a non-interactive stdin gracefully.
# ---------------------------------------------------------------------------
confirm_delete() {
  local desc="$1" cost="${2:-}" reply
  log ""
  log "  • ${desc}"
  [[ -n "$cost" ]] && log "    est. cost: ${cost}"
  if [[ "$DRY_RUN" == "1" ]]; then
    log "    [dry-run] would prompt to delete (skipped)."
    SKIPPED=$((SKIPPED + 1)); return 1
  fi
  # Read the answer from the controlling terminal, NOT stdin: the enclosing loops
  # redirect stdin from `gcloud ... | while read`, so a bare `read` here would
  # swallow the gcloud output instead of the operator's keypress.
  if [[ ! -r /dev/tty ]]; then
    die "No controlling terminal (/dev/tty) for confirmation — run interactively, or use --dry-run."
  fi
  printf '    Delete this? [y/N] ' >&2
  if ! IFS= read -r reply </dev/tty; then reply=""; fi
  case "$reply" in
    y|Y|yes|YES) return 0 ;;
    *) log "    skipped."; SKIPPED=$((SKIPPED + 1)); return 1 ;;
  esac
}

# run_delete <human-label> <cmd...> — execute a delete, update tallies + log.
# ALWAYS returns 0: a failed delete must NOT abort the sweep under `set -e` (the
# cascade keeps going and the final non-zero exit on FAILED>0 reports it).
run_delete() {
  local label="$1"; shift
  if "$@" >/dev/null 2>&1; then
    log "    ✓ deleted ${label}"; DELETED=$((DELETED + 1))
  else
    log "    ✗ FAILED to delete ${label} (try manually): $*"; FAILED=$((FAILED + 1))
  fi
  return 0
}

# Round a float to 2dp for display (awk; avoids bc dependency).
money() { awk -v v="$1" 'BEGIN{printf "%.2f", v}'; }

# Estimated monthly $ for a VM machine type (order-of-magnitude; on-demand).
vm_monthly_cost() {
  local mt="$1" hr
  case "$mt" in
    e2-micro)       hr="0.0084" ;; e2-small)   hr="0.0168" ;; e2-medium) hr="0.0335" ;;
    e2-standard-2)  hr="0.067"  ;; e2-standard-4) hr="0.134" ;; e2-standard-8) hr="0.268" ;;
    e2-standard-16) hr="0.536"  ;;
    n2-standard-2)  hr="0.097"  ;; n2-standard-4) hr="0.194" ;; n2-standard-8) hr="0.388" ;;
    n1-standard-1)  hr="0.0475" ;; n1-standard-2) hr="0.095" ;; n1-standard-4) hr="0.19" ;;
    c2-standard-4)  hr="0.2088" ;; c2-standard-8) hr="0.4176" ;; c2-standard-16) hr="0.8352" ;;
    c3-standard-4)  hr="0.2010" ;; c3-standard-8) hr="0.4020" ;; c3-standard-22) hr="1.1055" ;;
    *) echo "running compute charges apply (machine type ${mt:-?})"; return ;;
  esac
  echo "≈ \$$(money "$(awk -v h="$hr" -v m="$HOURS_PER_MONTH" 'BEGIN{print h*m}')")/mo while RUNNING (${mt})"
}

# =============================================================================
# Banner
# =============================================================================
log "================ SWEEP DEV CLOUD RESOURCES ================"
log "  Project:   $GCP_PROJECT   Region: $GCP_REGION"
log "  Mode:      $([[ "$DRY_RUN" == 1 ]] && echo 'DRY-RUN (list + cost, no deletes)' || echo 'INTERACTIVE (confirm each)')"
log "  Categories:$([[ ${#ONLY[@]} -eq 0 ]] && echo ' workers networks backups (all)' || printf ' %s' "${ONLY[@]}")"
log "=========================================================="
init_db_check

# =============================================================================
# 1 + 2. Workers — MIGs first (deleting a MIG removes its managed instances),
# then any remaining standalone instances.
# =============================================================================
if want workers; then
  log ""
  log "=== ALL managed instance groups ==="
  # name|region|zone|targetSize ; exactly one of region/zone is set (regional vs
  # zonal MIG), which also selects the delete flag.
  glist_into _migs gcloud compute instance-groups managed list --project="$GCP_PROJECT" \
    --format="value[separator='|'](name, region.basename(), zone.basename(), targetSize)"
  while IFS='|' read -r mname mregion mzone msize; do
    [[ -n "$mname" ]] || continue
    reason="$(control_plane_reason "$mname" "")"
    if [[ -n "$reason" ]]; then
      log ""; log "  • MIG ${mname}  [PROTECTED — ${reason}]"; log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    if is_standing_host "$mname" "" && [[ "$SWEEP_PROTECTED" != "1" ]]; then
      log ""; log "  • MIG ${mname}  [PROTECTED — standing worker host; deleting stops a tenant's worker plane]"
      log "    not offered (set SWEEP_PROTECTED=1 to override)."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    if [[ -n "$mregion" ]]; then loc_flag="--region=$mregion"; loc_desc="region $mregion"
    else                         loc_flag="--zone=$mzone";     loc_desc="zone $mzone"; fi
    tag=""; is_standing_host "$mname" "" && tag="  [standing worker host — SWEEP_PROTECTED override]"
    if confirm_delete "MIG ${mname} (${loc_desc}, target size ${msize:-?})${tag}" \
                      "billed per running managed instance"; then
      # shellcheck disable=SC2086
      run_delete "MIG ${mname}" gcloud compute instance-groups managed delete "$mname" \
        --project="$GCP_PROJECT" $loc_flag --quiet
    fi
  done <<< "$_migs"

  log ""
  log "=== ALL VM instances ==="
  log "    (a VM still managed by a MIG you kept above will be recreated — delete the MIG instead.)"
  # name|zone|status|machineType|creationTimestamp|network|labels
  glist_into _vms gcloud compute instances list --project="$GCP_PROJECT" \
    --format="value[separator='|'](name, zone.basename(), status, machineType.basename(), creationTimestamp, networkInterfaces[0].network.basename(), labels)"
  while IFS='|' read -r vname vzone vstatus vmtype vcreated vnet vlabels; do
    [[ -n "$vname" ]] || continue
    # 1. Control plane (GKE etc.) — always protected.
    reason="$(control_plane_reason "$vname" "$vlabels")"
    if [[ -n "$reason" ]]; then
      log ""; log "  • VM ${vname}  [PROTECTED — ${reason}]"; log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    # 2. Standing tenant worker host — protected unless SWEEP_PROTECTED=1.
    if is_standing_host "$vname" "$vlabels" && [[ "$SWEEP_PROTECTED" != "1" ]]; then
      log ""; log "  • VM ${vname}  [PROTECTED — standing worker host; SWEEP_PROTECTED=1 to override, or delete its MIG]"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    # 3. Per-org host (g5_org_id label preferred, else g5-org-<uuid> network).
    vuuid="$(org_uuid_from "$vlabels" "$vnet")"
    vstat="$(org_status_for "$vuuid")"
    vslug="$(org_slug_for "$vuuid")"
    vreg="$(worker_registration_for "$vzone/$vname")"
    if [[ "$vstat" == "IN USE" ]]; then
      log ""; log "  • VM ${vname}  [PROTECTED — belongs to LIVE org '${vslug:-$vuuid}'; ${vreg}]"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    # A per-org host whose org can't be confirmed (unverified) could belong to a
    # LIVE org — protect by default; only confirmed ORPHANs are offered.
    if [[ -n "$vuuid" && "$vstat" != "ORPHAN" && "$SWEEP_UNVERIFIED" != "1" ]]; then
      log ""; log "  • VM ${vname}  [PROTECTED — org ${vuuid} UNVERIFIED (could be live); run CHECK_DB=1, or SWEEP_UNVERIFIED=1 to offer]"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    if [[ -n "$vuuid" ]]; then
      case "$vstat" in
        ORPHAN) owner="org ${vslug:-$vuuid} [ORPHAN — org deleted]" ;;
        *)      owner="org ${vuuid} [unverified (SWEEP_UNVERIFIED=1) — set CHECK_DB=1 to confirm]" ;;
      esac
    else
      owner="UNCLASSIFIED — no org label/network and not recognized as control-plane; VERIFY before deleting"
    fi
    if confirm_delete "VM ${vname} (${vzone}, ${vstatus}, ${vmtype}; created ${vcreated})
       owner: ${owner}
       ${vreg}" \
                      "$(vm_monthly_cost "$vmtype")"; then
      run_delete "VM ${vname}" gcloud compute instances delete "$vname" \
        --project="$GCP_PROJECT" --zone="$vzone" --quiet
    fi
  done <<< "$_vms"
fi

# =============================================================================
# 3. Per-org VPC networks (g5-org-*) + cascade of their dependents.
# =============================================================================
delete_network_cascade() {
  local net="$1" name region
  # Forwarding rules → instances → firewalls → routers (NAT) → subnets → network.
  while IFS='|' read -r name region; do
    [[ -n "$name" ]] || continue
    run_delete "forwarding-rule ${name}" gcloud compute forwarding-rules delete "$name" \
      --project="$GCP_PROJECT" --region="$region" --quiet
  done < <(gcloud compute forwarding-rules list --project="$GCP_PROJECT" \
            --filter="network~/${net}\$" --format="value[separator='|'](name, region.basename())" 2>/dev/null)

  while IFS='|' read -r name zone; do
    [[ -n "$name" ]] || continue
    run_delete "instance ${name}" gcloud compute instances delete "$name" \
      --project="$GCP_PROJECT" --zone="$zone" --quiet
  done < <(gcloud compute instances list --project="$GCP_PROJECT" \
            --filter="networkInterfaces.network~/${net}\$" --format="value[separator='|'](name, zone.basename())" 2>/dev/null)

  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    run_delete "firewall ${name}" gcloud compute firewall-rules delete "$name" \
      --project="$GCP_PROJECT" --quiet
  done < <(gcloud compute firewall-rules list --project="$GCP_PROJECT" \
            --filter="network~/${net}\$" --format="value(name)" 2>/dev/null)

  while IFS='|' read -r name region; do
    [[ -n "$name" ]] || continue
    run_delete "router ${name}" gcloud compute routers delete "$name" \
      --project="$GCP_PROJECT" --region="$region" --quiet
  done < <(gcloud compute routers list --project="$GCP_PROJECT" \
            --filter="network~/${net}\$" --format="value[separator='|'](name, region.basename())" 2>/dev/null)

  while IFS='|' read -r name region; do
    [[ -n "$name" ]] || continue
    run_delete "subnet ${name}" gcloud compute networks subnets delete "$name" \
      --project="$GCP_PROJECT" --region="$region" --quiet
  done < <(gcloud compute networks subnets list --project="$GCP_PROJECT" \
            --filter="network~/${net}\$" --format="value[separator='|'](name, region.basename())" 2>/dev/null)

  run_delete "network ${net}" gcloud compute networks delete "$net" --project="$GCP_PROJECT" --quiet
}

if want networks; then
  log ""
  log "=== ALL VPC networks ==="
  glist_into _nets gcloud compute networks list --project="$GCP_PROJECT" --format="value(name)"
  while IFS= read -r net; do
    [[ -n "$net" ]] || continue
    # Only the app-created per-org networks (g5-org-<uuid>) are sweepable. Every
    # other network (default, the cluster/worker VPC, anything shared) is control
    # plane — listed but never offered, so the cascade can't touch shared infra.
    if [[ "$net" != ${ORG_NET_PREFIX}* ]]; then
      log ""; log "  • network ${net}  [PROTECTED — control plane / shared (not a per-org VPC)]"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    uuid="${net#"$ORG_NET_PREFIX"}"
    status="$(org_status_for "$uuid")"
    slug="$(org_slug_for "$uuid")"
    # A network whose org is still live must NOT be torn down (its cascade would
    # kill the live org's worker host + subnets).
    if [[ "$status" == "IN USE" ]]; then
      log ""
      log "  • network ${net}  [PROTECTED — IN USE by live org '${slug:-$uuid}']"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    # Unverified (no DB confirmation) could be a LIVE org — protect by default.
    if [[ "$status" != "ORPHAN" && "$SWEEP_UNVERIFIED" != "1" ]]; then
      log ""
      log "  • network ${net}  [PROTECTED — org status UNVERIFIED (could be live); run CHECK_DB=1, or SWEEP_UNVERIFIED=1 to offer]"
      log "    not offered."
      SKIPPED=$((SKIPPED + 1)); continue
    fi
    case "$status" in
      ORPHAN) owner="org ${slug:-$uuid} ORPHAN — org deleted" ;;
      *)      owner="unverified (SWEEP_UNVERIFIED=1) — set CHECK_DB=1 to confirm it isn't a live org" ;;
    esac
    nat="$(gcloud compute routers list --project="$GCP_PROJECT" --filter="network~/${net}\$" --format='value(name)' 2>/dev/null | grep -c . || true)"
    cost="network is free"; [[ "${nat:-0}" -gt 0 ]] && cost="≈ \$$(money "$(awk -v r="$RATE_NAT_GW_HOUR" -v m="$HOURS_PER_MONTH" 'BEGIN{print r*m}')")/mo per Cloud NAT gateway (+ data)"
    if confirm_delete "network ${net}  [${owner}]  (cascades firewalls/routers/subnets/instances)" "$cost"; then
      delete_network_cascade "$net"
    fi
  done <<< "$_nets"
fi

# =============================================================================
# 4 + 5 + 6. Backups.
# =============================================================================
if want backups; then
  log ""
  # By default only sweep ON_DEMAND backups — those are the unbounded leftovers
  # the reset script creates. AUTOMATED backups are Cloud SQL's managed daily DR
  # schedule (auto-pruned by the instance retention policy); deleting them by hand
  # just reduces DR coverage. INCLUDE_AUTOMATED_BACKUPS=1 to include them too.
  # Build the Cloud SQL backup filter:
  #   default               → reset-created on-demand only
  #   INCLUDE_AUTOMATED=1    → reset-created on-demand + ALL automated (the managed
  #                            schedule) — NOT unrelated manual on-demand restores
  #   SWEEP_ALL_BACKUPS=1    → everything (no filter)
  if [[ "$SWEEP_ALL_BACKUPS" == "1" ]]; then
    SQL_BACKUP_FILTER=""
  elif [[ -n "$RESET_SQL_BACKUP_MATCH" ]]; then
    SQL_BACKUP_FILTER="type=ON_DEMAND AND description~${RESET_SQL_BACKUP_MATCH}"
    [[ "${INCLUDE_AUTOMATED_BACKUPS:-0}" == "1" ]] && SQL_BACKUP_FILTER="( ${SQL_BACKUP_FILTER} ) OR type=AUTOMATED"
  else
    # No reset marker configured — fall back to type-based scoping.
    SQL_BACKUP_FILTER="type=ON_DEMAND"
    [[ "${INCLUDE_AUTOMATED_BACKUPS:-0}" == "1" ]] && SQL_BACKUP_FILTER=""
  fi
  sql_filter_args=()
  [[ -n "$SQL_BACKUP_FILTER" ]] && sql_filter_args=(--filter="$SQL_BACKUP_FILTER")
  log "=== Cloud SQL backups (${DB_INSTANCE_NAME}${SQL_BACKUP_FILTER:+, $SQL_BACKUP_FILTER}) — ⚠ THESE ARE RESTORE POINTS ==="
  [[ "$SWEEP_ALL_BACKUPS" == "1" ]] || log "    (reset-created only; SWEEP_ALL_BACKUPS=1 for all, INCLUDE_AUTOMATED_BACKUPS=1 for the managed schedule.)"
  glist_into _sqlb gcloud sql backups list --instance="$DB_INSTANCE_NAME" --project="$GCP_PROJECT" \
    ${sql_filter_args[@]+"${sql_filter_args[@]}"} \
    --format="value[separator='|'](id, windowStartTime, type, status, description)"
  while IFS='|' read -r bid bwin btype bstatus bdesc; do
    [[ -n "$bid" ]] || continue
    # Per-backup chargeable size isn't in `list`; fetch it with a describe.
    bbytes="$(gcloud sql backups describe "$bid" --instance="$DB_INSTANCE_NAME" \
              --project="$GCP_PROJECT" --format='value(maxChargeableBytes)' 2>/dev/null || true)"
    if [[ "$bbytes" =~ ^[0-9]+$ && "$bbytes" -gt 0 ]]; then
      bgib="$(bytes_to_gib "$bbytes")"
      bcost="≈ \$$(gib_month_cost "$bgib" "$RATE_SQL_BACKUP_GIB_MONTH")/mo (${bgib} GiB chargeable)"
    else
      bcost="backup storage ≈ \$${RATE_SQL_BACKUP_GIB_MONTH}/GiB-mo (size unavailable)"
    fi
    if confirm_delete "Cloud SQL backup ${bid}  (${btype} ${bstatus}, ${bwin})  ${bdesc:+— $bdesc}
       ⚠ deleting this removes a RESTORE POINT for ${DB_INSTANCE_NAME}" "$bcost"; then
      run_delete "Cloud SQL backup ${bid}" gcloud sql backups delete "$bid" \
        --instance="$DB_INSTANCE_NAME" --project="$GCP_PROJECT" --quiet
    fi
  done <<< "$_sqlb"

  log ""
  log "=== Filestore backups (region ${FILESTORE_BACKUP_REGION}) ==="
  [[ "$SWEEP_ALL_BACKUPS" == "1" ]] || log "    (reset-created '${RESET_FILESTORE_BACKUP_REGEX}' only; SWEEP_ALL_BACKUPS=1 for all.)"
  # storageBytes is the ACTUAL backed-up size (billed), not capacityGb (the source
  # share's 1 TiB capacity) — the latter overstates cost by ~1000×.
  glist_into _fsb gcloud filestore backups list --project="$GCP_PROJECT" --region="$FILESTORE_BACKUP_REGION" \
    --format="value[separator='|'](name, createTime, storageBytes, state)"
  while IFS='|' read -r fname fcreate fbytes fstate; do
    [[ -n "$fname" ]] || continue
    short="${fname##*/}"
    # Skip non-reset Filestore backups (manual restore points) by default.
    if [[ "$SWEEP_ALL_BACKUPS" != "1" && ! "$short" =~ $RESET_FILESTORE_BACKUP_REGEX ]]; then
      continue
    fi
    fgib="$(bytes_to_gib "$fbytes")"
    cost="≈ \$$(gib_month_cost "$fgib" "$RATE_FILESTORE_GIB_MONTH")/mo (${fgib} GiB used)"
    if confirm_delete "Filestore backup ${short}  (${fstate}, created ${fcreate})" "$cost"; then
      run_delete "Filestore backup ${short}" gcloud filestore backups delete "$short" \
        --region="$FILESTORE_BACKUP_REGION" --project="$GCP_PROJECT" --quiet
    fi
  done <<< "$_fsb"

  if [[ -n "$BACKUP_BUCKET" ]]; then
    log ""
    log "=== GCS reset exports (gs://${BACKUP_BUCKET}/dev-reset-*.sql.gz) ==="
    # List the bucket root (succeeds even when empty) and filter in-loop, so a
    # real access failure (missing bucket / no Storage IAM) surfaces via
    # glist_into instead of a no-match exit being mistaken for "no exports".
    glist_into _gcs gcloud storage ls "gs://${BACKUP_BUCKET}/"
    while IFS= read -r obj; do
      [[ "$obj" == gs://*dev-reset-*.sql.gz ]] || continue
      bytes="$(gcloud storage objects describe "$obj" --format='value(size)' 2>/dev/null || true)"
      gib="$(bytes_to_gib "$bytes")"
      cost="≈ \$$(gib_month_cost "$gib" "$RATE_GCS_GIB_MONTH")/mo (${gib} GiB)"
      if confirm_delete "GCS export ${obj}" "$cost"; then
        run_delete "GCS export ${obj}" gcloud storage rm "$obj"
      fi
    done <<< "$_gcs"
  fi
fi

# =============================================================================
# Summary
# =============================================================================
log ""
log "=== Sweep complete$([[ "$DRY_RUN" == 1 ]] && echo ' (DRY-RUN — nothing deleted)') ==="
log "  Deleted: $DELETED   Skipped: $SKIPPED   Failed: $FAILED"
if [[ "$DISCOVERY_FAILED" == "1" ]]; then
  log "  ⚠ Some resource discovery FAILED — the list above may be INCOMPLETE. Re-run after"
  log "    fixing the gcloud error (IAM / API / filter) before trusting a clean result."
fi
{ [[ "$FAILED" -gt 0 || "$DISCOVERY_FAILED" == "1" ]]; } && exit 1
exit 0
