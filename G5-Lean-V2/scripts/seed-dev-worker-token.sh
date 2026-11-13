#!/usr/bin/env bash
# =============================================================================
# seed-dev-worker-token.sh — Provision local workspace-host credentials
# =============================================================================
# Resolves the demo org via the test-token flow, then creates an org-scoped
# `worker_hosts` identity used by `g5d host` through the public operator API.
#
# Outputs shell exports on stdout for callers:
#   export G5D_ORG_ID=<uuid>
#   export G5D_HOST_ID=<uuid>
#   export G5_HOST_JWT=<jwt>
#
# Human-readable status goes to stderr.
# =============================================================================
set -euo pipefail

API_URL="${1:-http://localhost:7500}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEN_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
# G5_WORKER_ORG (internal plumbing, set by the worker-host* recipes from their
# positional org argument — not a user knob) selects which of the demo user's
# orgs the host targets (default: first org) and gives that org its own env file.
if [ -n "${G5_WORKER_ORG:-}" ]; then
  # Fail closed on unsafe slugs before using the value in a filesystem path
  # (ENV_FILE) or request payloads (PR #1530 review): lowercase [a-z0-9-],
  # not dash-leading.
  case "$G5_WORKER_ORG" in
    *[!a-z0-9-]*|-*)
      echo "  ✗ invalid org slug '${G5_WORKER_ORG}' — expected lowercase [a-z0-9-], not starting with '-'" >&2
      exit 1
      ;;
  esac
  # Prefixed so no slug (the valid slug `demo` included) can name the
  # default instance's file, which dev-up reads for the host it started.
  ENV_FILE="${GEN_ROOT}/infra/docker/org-${G5_WORKER_ORG}-worker.env"
else
  ENV_FILE="${GEN_ROOT}/infra/docker/demo-worker.env"
fi

curl_health() {
  local url="$1"
  curl -fsS \
    --retry 5 \
    --retry-all-errors \
    --retry-delay 1 \
    --connect-timeout 3 \
    --max-time 10 \
    "$url" >/dev/null 2>&1
}

curl_json() {
  local label="$1"
  shift

  local body err status
  body="$(mktemp)"
  err="$(mktemp)"
  if curl -fsS \
    --retry 5 \
    --retry-all-errors \
    --retry-delay 1 \
    --connect-timeout 5 \
    --max-time 30 \
    "$@" >"$body" 2>"$err"; then
    cat "$body"
    rm -f "$body" "$err"
    return 0
  fi

  status=$?
  echo "  ✗ ${label} failed (curl exit ${status})" >&2
  if [ -s "$err" ]; then
    sed -n '1,5p' "$err" | sed 's/^/    /' >&2
  fi
  rm -f "$body" "$err"
  return "$status"
}

if [ "$API_URL" = "auto" ] || ! curl_health "${API_URL}/healthz"; then
  if curl_health "http://localhost:7500/healthz"; then
    API_URL="http://localhost:7500"
  else
    API_IP=$(docker inspect g5-dev-api --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' 2>/dev/null || echo "")
    if [ -n "$API_IP" ] && curl_health "http://${API_IP}:7500/healthz"; then
      API_URL="http://${API_IP}:7500"
    else
      echo "  ✗ Cannot reach g5-api — cannot provision a workspace host" >&2
      exit 1
    fi
  fi
fi

AUTH_RESP=$(curl_json "auth test-token" "${API_URL}/api/v1/auth/test-token" \
  -H "Content-Type: application/json" \
  -d '{"email":"demo@g5labs.ai"}')
AUTH_TOKEN=$(printf '%s' "$AUTH_RESP" | jq -r '.data.jwt // empty' 2>/dev/null || true)
if [ -z "$AUTH_TOKEN" ]; then
  echo "  ✗ Failed to mint auth token for demo@g5labs.ai" >&2
  exit 1
fi

ORGS_RESP=$(curl_json "org lookup" "${API_URL}/api/v1/orgs" \
  -H "Authorization: Bearer ${AUTH_TOKEN}")
if [ -n "${G5_WORKER_ORG:-}" ]; then
  ORG_ID=$(printf '%s' "$ORGS_RESP" | jq -r --arg slug "$G5_WORKER_ORG" \
    '.data[] | select(.orgSlug == $slug) | .orgId // empty' 2>/dev/null | head -1)
  ORG_SLUG="$G5_WORKER_ORG"
  if [ -z "$ORG_ID" ]; then
    AVAILABLE=$(printf '%s' "$ORGS_RESP" | jq -r '[.data[].orgSlug] | join(", ")' 2>/dev/null || true)
    echo "  ✗ No org with slug '${G5_WORKER_ORG}' for demo@g5.dev (available: ${AVAILABLE:-none})" >&2
    exit 1
  fi
else
  # Slug-less default: prefer the org seed-dev-org.sh seeds (slug g5labs) so
  # the default host's org is deterministic. First-org ordering is unstable on
  # multi-org DBs (e.g. alphabetical re-sort after adding an org) and silently
  # re-homed the default host; fall back to the first org only when the seeded
  # org is absent (custom dev DBs).
  ORG_ID=$(printf '%s' "$ORGS_RESP" | jq -r \
    '.data[] | select(.orgSlug == "g5labs") | .orgId // empty' 2>/dev/null | head -1)
  if [ -n "$ORG_ID" ]; then
    ORG_SLUG="g5labs"
  else
    ORG_ID=$(printf '%s' "$ORGS_RESP" | jq -r '.data[0].orgId // empty' 2>/dev/null || true)
    ORG_SLUG=$(printf '%s' "$ORGS_RESP" | jq -r '.data[0].orgSlug // empty' 2>/dev/null || true)
  fi
fi
if [ -z "$ORG_ID" ]; then
  echo "  ✗ Failed to resolve demo org" >&2
  exit 1
fi

# Local bootstrap explicitly configures its fixture organization. The admin
# setting is authoritative; the returned snapshot drives the launched host.
VM_DISK_MIB="${G5D_SCRATCH_DISK_MIB:-10240}"
HOST_SLOTS="${G5D_HOST_MAX_MICROVMS:-2}"
if ! [[ "$VM_DISK_MIB" =~ ^[1-9][0-9]*$ && "$HOST_SLOTS" =~ ^[1-9][0-9]*$ ]] \
  || (( VM_DISK_MIB % 1024 != 0 )); then
  echo '  ✗ Local host disk allowance must be whole GiB and slots must be positive' >&2
  exit 1
fi
DISK_SETTINGS=$(curl_json "local organization disk allowance" \
  "${API_URL}/api/v1/orgs/${ORG_ID}/admin-settings" -X PUT \
  -H "Content-Type: application/json" -H "Authorization: Bearer ${AUTH_TOKEN}" \
  -d "$(jq -nc --argjson v "$((VM_DISK_MIB / 1024))" '{vmDiskAllowanceGib: $v}')")
printf '%s' "$DISK_SETTINGS" | jq -e --argjson v "$((VM_DISK_MIB / 1024))" \
  '.data.vmDiskAllowanceGib == $v' >/dev/null

HOST_RESP=$(curl_json "workspace host provision" "${API_URL}/api/v1/orgs/${ORG_ID}/worker-hosts" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${AUTH_TOKEN}" \
  -d "$(jq -nc --argjson slots "$HOST_SLOTS" '{region:"us-central1",instanceType:"c3-standard-4",workspaceVmLimit:$slots,baseImageRef:"ghcr.io/g5labs/g5d-worker-dev@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}')")
HOST_ID=$(printf '%s' "$HOST_RESP" | jq -r '.data.hostId // empty' 2>/dev/null || true)
HOST_JWT=$(printf '%s' "$HOST_RESP" | jq -r '.data.hostJwt // empty' 2>/dev/null || true)
if [ -z "$HOST_ID" ] || [ -z "$HOST_JWT" ]; then
  echo "  ✗ Failed to provision local workspace host" >&2
  exit 1
fi
VM_DISK_MIB=$(printf '%s' "$HOST_RESP" | jq -er '.data.diskAllocation.vmDiskMib | select(. > 0)')
HOST_SLOTS=$(printf '%s' "$HOST_RESP" | jq -er '.data.diskAllocation.slots | select(. > 0)')
echo "  ✓ Workspace host provisioned for ${ORG_SLUG:-demo org} (${HOST_ID})" >&2

cat >"${ENV_FILE}" <<EOF
G5D_ORG_ID=${ORG_ID}
G5D_HOST_ID=${HOST_ID}
G5_HOST_JWT=${HOST_JWT}
G5D_SCRATCH_DISK_MIB=${VM_DISK_MIB}
G5D_HOST_MAX_MICROVMS=${HOST_SLOTS}
EOF
chmod 600 "${ENV_FILE}" 2>/dev/null || true
echo "  ✓ Wrote demo workspace-host env: ${ENV_FILE}" >&2

echo "export G5D_ORG_ID=${ORG_ID}"
echo "export G5D_HOST_ID=${HOST_ID}"
echo "export G5_HOST_JWT=${HOST_JWT}"
echo "export G5D_SCRATCH_DISK_MIB=${VM_DISK_MIB}"
echo "export G5D_HOST_MAX_MICROVMS=${HOST_SLOTS}"
