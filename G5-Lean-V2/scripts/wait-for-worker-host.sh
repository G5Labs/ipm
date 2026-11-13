#!/usr/bin/env bash
# =============================================================================
# wait-for-worker-host.sh — wait until the selected API lists a ready, live
# worker host for the seeded org.
# =============================================================================
# Usage: ./scripts/wait-for-worker-host.sh <api-url> [timeout-seconds] [host-id]
#
# `dev-up` publishes the UI only after this succeeds (ontology
# g5.runtime.boot.backend.process-local): the host log's "workspace VM host
# started" line precedes its hello, and a host the control plane rejects keeps
# retrying with its process alive, so neither the log nor the pid proves
# registration. With a host id (the one `worker-host-start` provisioned or
# reused, from infra/docker/demo-worker.env) only that host counts: a host
# stopped moments ago still reads as live until its heartbeat ages out, and
# must not stand in for its replacement. The dev API mints the demo persona's
# token (G5_E2E_AUTH); the org is `g5labs` when present, otherwise the
# persona's first org. The host list is paged, newest first, and keeps every
# retired host, so every page is read until the host is found.
set -euo pipefail

API="${1:?api url}"
TIMEOUT="${2:-120}"
HOST_ID="${3:-}"
PAGE=50
deadline=$((SECONDS + TIMEOUT))

live_host_listed() { # <jwt> <org>
  local offset=0 page count
  while :; do
    page="$(curl -fsS --max-time 10 -H "Authorization: Bearer $1" \
      "${API}/api/v1/orgs/$2/worker-hosts?status=ready&limit=${PAGE}&offset=${offset}" 2>/dev/null || true)"
    [ -n "$page" ] || return 1
    if [ "$(jq -r --arg id "$HOST_ID" '[(.data.hosts // [])[] | select(.status == "ready" and .heartbeatState == "live" and ($id == "" or .hostId == $id))] | length' <<<"$page" 2>/dev/null || echo 0)" -gt 0 ] 2>/dev/null; then
      return 0
    fi
    count="$(jq -r '(.data.hosts // []) | length' <<<"$page" 2>/dev/null || echo 0)"
    [ "${count:-0}" -ge "$PAGE" ] 2>/dev/null || return 1
    offset=$((offset + PAGE))
  done
}

while :; do
  jwt="$(curl -fsS --max-time 10 -X POST "${API}/api/v1/auth/test-token" \
    -H 'content-type: application/json' -d '{"email":"demo@g5labs.ai"}' 2>/dev/null \
    | jq -r '.data.jwt // empty' 2>/dev/null || true)"
  if [ -n "$jwt" ]; then
    org="$(curl -fsS --max-time 10 -H "Authorization: Bearer ${jwt}" "${API}/api/v1/orgs" 2>/dev/null \
      | jq -r '(.data // []) as $orgs | (($orgs[] | select(.orgSlug == "g5labs") | (.id // .orgId)) // ($orgs[0] | (.id // .orgId)) // empty)' 2>/dev/null \
      | head -n 1 || true)"
    if [ -n "$org" ] && live_host_listed "$jwt" "$org"; then
      echo "✓ worker host ${HOST_ID:-(any)} registered and live with ${API} (org ${org})"
      exit 0
    fi
  fi
  if [ "$SECONDS" -ge "$deadline" ]; then
    echo "✗ worker host ${HOST_ID:-(any)} not ready and live with ${API} within ${TIMEOUT}s" >&2
    exit 1
  fi
  sleep 3
done
