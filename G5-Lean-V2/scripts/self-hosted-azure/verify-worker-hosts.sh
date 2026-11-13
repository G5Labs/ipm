#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${G5_INSTALL_DIR:-/opt/g5}"
EXTERNAL_WORKERS="${1:-${G5_EXPECTED_EXTERNAL_WORKERS:-0}}"
HEALTH_TIMEOUT="${G5_AZURE_HEALTH_TIMEOUT:-660}"

die() {
  echo "azure self-hosted worker verification failed: $*" >&2
  exit 1
}

set -a
# shellcheck disable=SC1091
. "$INSTALL_DIR/.env"
set +a
G5_TLS_CADDYFILE="${G5_TLS_CADDYFILE:-$INSTALL_DIR/Caddyfile.tls}"
export G5_TLS_CADDYFILE

compose_sql() {
  docker compose --env-file "$INSTALL_DIR/.env" -f "$INSTALL_DIR/compose.yml" \
    -f "$INSTALL_DIR/compose.tls.yml" \
    exec -T postgres psql \
    -U "${G5_DB_USER:-g5}" \
    -d "${G5_DB_NAME:-g5}" \
    -v ON_ERROR_STOP=1 "$@"
}

[ -n "${G5D_ORG_ID:-}" ] || die "G5D_ORG_ID missing from installed .env"
[ "$EXTERNAL_WORKERS" -eq 1 ] || die "microVM canary expects exactly one self-hosted workspace host"
expected="$EXTERNAL_WORKERS"
deadline=$((SECONDS + HEALTH_TIMEOUT))

while true; do
  count="$(compose_sql -At -v org_id="$G5D_ORG_ID" <<'SQL'
SELECT count(*)
FROM worker_hosts
WHERE org_id = :'org_id'
  AND kind = 'self_hosted'
  AND status = 'ready'
  AND last_heartbeat > now() - interval '30 seconds';
SQL
)"
  case "${count:-}" in
    ''|*[!0-9]*) die "workspace-host count query returned a non-numeric value: ${count:-<empty>}" ;;
  esac
  if [ "$count" -gt "$expected" ]; then
    die "expected exactly $expected ready self-hosted workspace host, saw unexpected excess: $count"
  fi
  if [ "$count" -eq "$expected" ]; then
    compose_sql -At -v org_id="$G5D_ORG_ID" <<'SQL'
SELECT host_id || E'\t' || kind || E'\t' || capabilities::text
FROM worker_hosts
WHERE org_id = :'org_id'
  AND kind = 'self_hosted'
  AND status = 'ready'
ORDER BY created_at DESC;
SQL
    echo "ready self-hosted workspace hosts: $count (expected exactly $expected)"
    exit 0
  fi
  if [ "$SECONDS" -ge "$deadline" ]; then
    die "timed out waiting for $expected ready self-hosted workspace hosts, saw ${count:-0}"
  fi
  sleep 5
done
