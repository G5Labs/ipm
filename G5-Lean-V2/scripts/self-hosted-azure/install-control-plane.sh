#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${G5_INSTALL_DIR:-/opt/g5}"
DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
GITHUB_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-G5Labs/Releases}"
INSTALL_RELEASE="${G5_INSTALL_RELEASE:-latest}"
TARGET_VERSION="${G5_TARGET_VERSION:-latest}"
UPGRADE_VERSION="${G5_UPGRADE_VERSION:-$TARGET_VERSION}"
EXTERNAL_URL="${G5_EXTERNAL_URL:?G5_EXTERNAL_URL is required}"
API_URL="${G5_API_URL:?G5_API_URL is required}"
LICENSE_KEY="${G5_LICENSE_KEY:?G5_LICENSE_KEY is required}"
HEALTH_TIMEOUT="${G5_AZURE_HEALTH_TIMEOUT:-600}"
BUNDLE_DIR="${G5_BUNDLE_DIR:?G5_BUNDLE_DIR is required}"
INSTALLER_PATH="${G5_INSTALLER_PATH:?G5_INSTALLER_PATH is required}"
TLS_CERT_FILE="${G5_TLS_CERT_FILE:?G5_TLS_CERT_FILE is required}"
TLS_KEY_FILE="${G5_TLS_KEY_FILE:?G5_TLS_KEY_FILE is required}"
G5_TLS_CADDYFILE="${G5_TLS_CADDYFILE:-$INSTALL_DIR/Caddyfile.tls}"
QUESTION_V1_COMPOSE_FILE_SOURCE="${G5_QUESTION_V1_COMPOSE_FILE_SOURCE:?G5_QUESTION_V1_COMPOSE_FILE_SOURCE is required}"

die() {
  echo "azure self-hosted control-plane install failed: $*" >&2
  exit 1
}

random_hex() {
  openssl rand -hex "$1"
}

wait_http() {
  local url="$1"
  local label="$2"
  local deadline=$((SECONDS + HEALTH_TIMEOUT))
  until curl -fsS "$url" >/dev/null 2>&1; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      die "timed out waiting for $label at $url"
    fi
    sleep 5
  done
}

compose() {
  local args=(--env-file "$INSTALL_DIR/.env" -f "$INSTALL_DIR/compose.yml")
  if [ -n "${G5_EXTRA_COMPOSE_FILE:-}" ]; then
    args+=(-f "$G5_EXTRA_COMPOSE_FILE")
  fi
  args+=(-f "$INSTALL_DIR/compose.tls.yml")
  docker compose "${args[@]}" "$@"
}

compose_sql() {
  compose exec -T postgres psql \
    -U "${G5_DB_USER:-g5}" \
    -d "${G5_DB_NAME:-g5}" \
    -v ON_ERROR_STOP=1 "$@"
}

set_env_key() {
  local key="$1"
  local value="$2"
  python3 - "$INSTALL_DIR/.env" "$key" "$value" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
key = sys.argv[2]
value = sys.argv[3]
pattern = re.compile(rf"^{re.escape(key)}=.*$")
replacement = f"{key}={value}"
lines = path.read_text().splitlines()
for index, line in enumerate(lines):
    if pattern.match(line):
        lines[index] = replacement
        break
else:
    lines.append(replacement)
path.write_text("\n".join(lines) + "\n")
PY
}

mkdir -p "$INSTALL_DIR" "$DATA_DIR"
[ -d "$BUNDLE_DIR" ] || die "G5_BUNDLE_DIR does not exist: $BUNDLE_DIR"
[ -r "$INSTALLER_PATH" ] || die "G5_INSTALLER_PATH is not readable: $INSTALLER_PATH"
[ -r "$TLS_CERT_FILE" ] || die "G5_TLS_CERT_FILE is not readable: $TLS_CERT_FILE"
[ -r "$TLS_KEY_FILE" ] || die "G5_TLS_KEY_FILE is not readable: $TLS_KEY_FILE"
[ -r "$QUESTION_V1_COMPOSE_FILE_SOURCE" ] || die "question-v1 compose override is not readable: $QUESTION_V1_COMPOSE_FILE_SOURCE"

if [ -z "${G5_JWT_PRIVATE_KEY:-}" ]; then
  G5_JWT_PRIVATE_KEY="$(openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256)"
  export G5_JWT_PRIVATE_KEY
fi
if [ -z "${G5_JWT_PUBLIC_KEY:-}" ]; then
  G5_JWT_PUBLIC_KEY="$(printf '%s\n' "$G5_JWT_PRIVATE_KEY" | openssl pkey -pubout)"
  export G5_JWT_PUBLIC_KEY
fi
export G5_DB_PASSWORD="${G5_DB_PASSWORD:-$(random_hex 24)}"
export G5_SCM_ENCRYPTION_KEY="${G5_SCM_ENCRYPTION_KEY:-$(random_hex 32)}"
export G5_COOKIE_SECURE="${G5_COOKIE_SECURE:-true}"
export SELF_HOSTED=true
export ENABLE_CLOUD_FEATURES="${ENABLE_CLOUD_FEATURES:-false}"
export G5_CORS_ALLOWED_ORIGINS="${G5_CORS_ALLOWED_ORIGINS:-$EXTERNAL_URL}"
export G5_TLS_CERT_FILE G5_TLS_KEY_FILE
export G5_TLS_CADDYFILE

bash "$INSTALLER_PATH" \
  --license-key "$LICENSE_KEY" \
  --api-url "$API_URL" \
  --install-dir "$INSTALL_DIR" \
  --data-dir "$DATA_DIR" \
  --external-url "$EXTERNAL_URL" \
  --github-release-repo "$GITHUB_RELEASE_REPO" \
  --release "$INSTALL_RELEASE" \
  --version "$TARGET_VERSION" \
  --bundle-dir "$BUNDLE_DIR"

install -m 0644 "$QUESTION_V1_COMPOSE_FILE_SOURCE" "$INSTALL_DIR/compose.question-v1.yml"
export G5_EXTRA_COMPOSE_FILE="$INSTALL_DIR/compose.question-v1.yml"
export G5_SYNC_QUESTION_V1_STAMPING="${G5_SYNC_QUESTION_V1_STAMPING:-true}"
set_env_key G5_EXTRA_COMPOSE_FILE "$G5_EXTRA_COMPOSE_FILE"
set_env_key G5_SYNC_QUESTION_V1_STAMPING "$G5_SYNC_QUESTION_V1_STAMPING"
compose up -d --force-recreate g5-api
[ "$(compose exec -T g5-api printenv G5_SYNC_QUESTION_V1_STAMPING)" = "true" ] \
  || die "g5-api did not receive G5_SYNC_QUESTION_V1_STAMPING=true"

chmod 0600 "$INSTALL_DIR/.env"
wait_http "$EXTERNAL_URL/healthz" "G5 edge"

curl -fsS "$EXTERNAL_URL/api/v1/instance/info" -o /tmp/g5-instance-info.json
python3 - /tmp/g5-instance-info.json <<'PY'
import json
import os
import sys

payload = json.load(open(sys.argv[1]))
data = payload.get("data") or payload
if data.get("selfHosted") is not True:
    raise SystemExit(f"expected selfHosted=true, got {data.get('selfHosted')!r}")
if "cloud_workers" in data.get("features", []):
    raise SystemExit(f"self-hosted instance advertised cloud_workers: {data.get('features')!r}")
# The canary signs in through Microsoft Entra ID; GitHub login is present
# only when the optional SELF_HOSTED_GITHUB_* credentials were supplied.
expected = ["microsoft"]
if os.environ.get("G5_GITHUB_CLIENT_ID"):
    expected = ["github", "microsoft"]
if data.get("authProviders") != expected:
    raise SystemExit(f"expected auth providers {expected!r}, got {data.get('authProviders')!r}")
PY

for endpoint in /api/v1/admin/licenses /api/v1/registry/token; do
  code="$(curl -sS -o /tmp/g5-selfhost-route.out -w '%{http_code}' -X POST "$EXTERNAL_URL$endpoint" -H 'Content-Type: application/json' -d '{}' || true)"
  [ "$code" = "404" ] || die "self-hosted route $endpoint should be unavailable, got HTTP $code"
done
code="$(curl -sS -o /tmp/g5-selfhost-worker-host-route.out -w '%{http_code}' -X POST "$EXTERNAL_URL/api/v1/orgs/selfhost/worker-hosts" -H 'Content-Type: application/json' -d '{}' || true)"
case "$code" in
  401|403|404) ;;
  *) die "self-hosted worker-host provisioning should reject unauthenticated probes, got HTTP $code" ;;
esac

(cd "$INSTALL_DIR" && just logs g5-api >/tmp/g5-just-logs.txt 2>&1 & echo $! >/tmp/g5-logs.pid)
sleep 5
kill "$(cat /tmp/g5-logs.pid)" >/dev/null 2>&1 || true

compose_sql -c "CREATE TABLE IF NOT EXISTS selfhost_azure_restore_marker (id text PRIMARY KEY, value text NOT NULL);"
compose_sql -c "INSERT INTO selfhost_azure_restore_marker (id, value) VALUES ('marker', 'before-backup') ON CONFLICT (id) DO UPDATE SET value = EXCLUDED.value;"
(cd "$INSTALL_DIR" && just backup)
backup_path="$(find "$INSTALL_DIR/backups" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
[ -n "$backup_path" ] || die "backup path was not created"
[ -s "$backup_path/postgres.dump" ] || die "backup did not create a non-empty postgres.dump"

compose_sql -c "UPDATE selfhost_azure_restore_marker SET value = 'after-backup' WHERE id = 'marker';"
(cd "$INSTALL_DIR" && just restore "$backup_path")
restored="$(compose_sql -At -c "SELECT value FROM selfhost_azure_restore_marker WHERE id = 'marker';")"
[ "$restored" = "before-backup" ] || die "restore marker mismatch: $restored"

(cd "$INSTALL_DIR" && just down)
if curl -fsS "$EXTERNAL_URL/healthz" >/dev/null 2>&1; then
  die "health endpoint still responded after just down"
fi
(cd "$INSTALL_DIR" && just up)
wait_http "$EXTERNAL_URL/healthz" "G5 edge after just up"

set_env_key G5_TARGET_VERSION "$UPGRADE_VERSION"
(cd "$INSTALL_DIR" && just upgrade)
wait_http "$EXTERNAL_URL/healthz" "G5 edge after just upgrade"

# The release canary validates the exact immutable API image delivered by the
# licensed installer. It must never compile or substitute a local derivative,
# otherwise a green result would not be a receipt for the published release.
api_image="$(sed -n 's/^G5_IMAGE_API=//p' "$INSTALL_DIR/.env" | tail -n 1)"
[ -n "$api_image" ] || die "installed .env does not define G5_IMAGE_API"
docker image inspect "$api_image" >/dev/null 2>&1 || die "installed API image is unavailable: $api_image"
api_container_id="$(compose ps -q g5-api)"
[ -n "$api_container_id" ] || die "release g5-api container is not running"
release_image_id="$(docker image inspect "$api_image" --format '{{.Id}}')"
running_image_id="$(docker inspect "$api_container_id" --format '{{.Image}}')"
[ "$running_image_id" = "$release_image_id" ] || \
  die "running g5-api image mismatch: expected $release_image_id got $running_image_id"
[ "$(compose_sql -At -c "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'workers' AND column_name = 'external_host_id');")" = "t" ] || \
  die "workers.external_host_id migration is absent"
[ "$(compose_sql -At -c "SELECT position('external_host_id' in pg_get_functiondef('authz.lookup_worker_host_for_auth(uuid)'::regprocedure)) > 0;")" = "t" ] || \
  die "host-auth lookup function does not include external_host_id"

cat >"$INSTALL_DIR/azure-canary-summary.json" <<EOF
{
  "externalUrl": "$EXTERNAL_URL",
  "installRelease": "$INSTALL_RELEASE",
  "targetVersion": "$TARGET_VERSION",
  "upgradeVersion": "$UPGRADE_VERSION",
  "releaseApiImage": "$api_image",
  "releaseApiImageId": "$release_image_id"
}
EOF

echo "G5 Azure self-hosted canary complete: $EXTERNAL_URL"
