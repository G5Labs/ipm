#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$ROOT/infra/self-hosted"

API_IMAGE="${G5_SELFHOST_SMOKE_API_IMAGE:-}"
EDGE_IMAGE="${G5_SELFHOST_SMOKE_EDGE_IMAGE:-}"
WORKER_IMAGE="${G5_SELFHOST_SMOKE_WORKER_IMAGE:-g5-selfhost-smoke-worker:local}"
BUILD_NOOP_WORKER=true
if [ -n "${G5_SELFHOST_SMOKE_WORKER_IMAGE:-}" ]; then
  BUILD_NOOP_WORKER=false
fi
OAUTH_IMAGE="${G5_SELFHOST_SMOKE_OAUTH_IMAGE:-g5-selfhost-oauth-stub:local}"
PLAYWRIGHT_MODULE="${G5_SELFHOST_SMOKE_PLAYWRIGHT_MODULE:-$ROOT/frontend/node_modules/playwright}"
PLAYWRIGHT_CONTAINER_IMAGE="${G5_SELFHOST_SMOKE_PLAYWRIGHT_CONTAINER_IMAGE:-mcr.microsoft.com/playwright:v1.52.0-noble}"
PLAYWRIGHT_NPM_PACKAGE="${G5_SELFHOST_SMOKE_PLAYWRIGHT_NPM_PACKAGE:-playwright@1.52.0}"
SKIP_PLAYWRIGHT="${G5_SELFHOST_SMOKE_SKIP_PLAYWRIGHT:-false}"
SKIP_DOCKER_LOGIN="${G5_SELFHOST_SMOKE_SKIP_DOCKER_LOGIN:-true}"
SKIP_PULL="${G5_SELFHOST_SMOKE_SKIP_PULL:-true}"
SMOKE_DOCKER_NETWORK="${G5_SELFHOST_SMOKE_DOCKER_NETWORK:-}"
SMOKE_REGISTRY="${G5_SELFHOST_SMOKE_REGISTRY:-}"
SMOKE_REGISTRY_USERNAME="${G5_SELFHOST_SMOKE_REGISTRY_USERNAME:-oauth2accesstoken}"
SMOKE_REGISTRY_PASSWORD="${G5_SELFHOST_SMOKE_REGISTRY_PASSWORD:-offline-smoke-token}"
SMOKE_TMP_ROOT="${G5_SELFHOST_SMOKE_TMP_ROOT:-}"

die() {
  echo "self-hosted smoke failed: $*" >&2
  exit 1
}

is_true() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found"
}

free_port() {
  python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

wait_http() {
  local url="$1"
  local label="$2"
  for _ in $(seq 1 90); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  die "$label did not become ready at $url"
}

write_release_manifest() {
  local version="$1"
  local api_image="$2"
  local edge_image="$3"
  local worker_image="$4"
  local migration_image="$5"
  local output="$6"
  python3 - "$version" "$api_image" "$edge_image" "$worker_image" "$migration_image" "$output" <<'PY'
import json
import sys
from pathlib import Path

version, api, edge, worker, migration, output = sys.argv[1:7]
manifest = {
    "schema_version": 1,
    "product": "g5-self-hosted",
    "version": version,
    "images": {
        "api": api,
        "edge": edge,
        "worker": worker,
    },
    "migration": {
        "image": migration,
    },
}
Path(output).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
PY
}

write_playwright_login_script() {
  local output="$1"
  cat > "$output" <<'JS'
const modulePath = process.env.PLAYWRIGHT_MODULE_PATH;
const { chromium } = modulePath ? require(modulePath) : require("playwright");

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.goto(`${process.env.BASE_URL}/login`, { waitUntil: "domcontentloaded" });
    const github = page.getByRole("link", { name: /continue with github/i });
    await github.waitFor({ timeout: 30_000 });
    if (await page.getByRole("link", { name: /continue with google/i }).count()) {
      throw new Error("Google login was visible in self-hosted smoke");
    }
    const userResponsePromise = page.waitForResponse(
      (response) => response.url().includes("/api/v1/users/me") && response.status() === 200,
      { timeout: 45_000 },
    );
    await github.click();
    const userResponse = await userResponsePromise;
    const payload = await userResponse.json();
    const email = payload?.data?.email ?? payload?.email;
    if (email !== "selfhost-smoke@g5.dev") {
      throw new Error(`OAuth login returned unexpected user email: ${email}`);
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
JS
}

[ -n "$API_IMAGE" ] || die "G5_SELFHOST_SMOKE_API_IMAGE is required"
[ -n "$EDGE_IMAGE" ] || die "G5_SELFHOST_SMOKE_EDGE_IMAGE is required"
if [ -z "$SMOKE_REGISTRY" ]; then
  case "$API_IMAGE" in
    */*) SMOKE_REGISTRY="${API_IMAGE%%/*}" ;;
    *) SMOKE_REGISTRY="selfhost-smoke.local" ;;
  esac
fi

require_cmd docker
require_cmd python3
require_cmd just
require_cmd curl
require_cmd openssl

# One key pair for this smoke run, retained through its upgrade/restart steps.
set +x
source "$ROOT/scripts/lib/jwt-keys.sh"
PRIVATE_KEY_PEM="$(generate_es256_private_key)"
PUBLIC_KEY_PEM="$(printf '%s\n' "$PRIVATE_KEY_PEM" | derive_es256_public_key)"

if [ -z "$SMOKE_TMP_ROOT" ] && [ -n "$SMOKE_DOCKER_NETWORK" ] && [ -d /workspace ] && [ -w /workspace ]; then
  SMOKE_TMP_ROOT=/workspace/.g5-selfhost-smoke
fi
if [ -n "$SMOKE_TMP_ROOT" ]; then
  mkdir -p "$SMOKE_TMP_ROOT"
  TMP="$(mktemp -d "$SMOKE_TMP_ROOT/g5-selfhost-smoke.XXXXXX")"
else
  TMP="$(mktemp -d)"
fi
INSTALL_DIR="$TMP/opt-g5"
DATA_DIR="$TMP/var-lib-g5"
TOKEN_RESPONSE="$TMP/token-response.json"
COMPOSE_OVERRIDE="$TMP/compose.smoke.yml"
OAUTH_PORT="$(free_port)"
HTTP_PORT="$(free_port)"
HTTPS_PORT="$(free_port)"
COMPOSE_PROJECT="g5-selfhost-smoke-$$"
OAUTH_CONTAINER="$COMPOSE_PROJECT-oauth"

BASE_URL="http://127.0.0.1:$HTTP_PORT"
OAUTH_BROWSER_BASE_URL="http://127.0.0.1:$OAUTH_PORT"
OAUTH_API_BASE_URL="http://host.docker.internal:$OAUTH_PORT"
OAUTH_RUN_NETWORK_ARGS=(-p "0.0.0.0:$OAUTH_PORT:9876")
PLAYWRIGHT_NETWORK_ARGS=(--network host)
G5_API_CONNECTIVITY_BLOCK='    extra_hosts:
      - "host.docker.internal:host-gateway"'
EDGE_CONNECTIVITY_BLOCK=""

if [ -n "$SMOKE_DOCKER_NETWORK" ]; then
  BASE_URL="${G5_SELFHOST_SMOKE_BASE_URL:-http://edge:80}"
  OAUTH_BROWSER_BASE_URL="http://$OAUTH_CONTAINER:9876"
  OAUTH_API_BASE_URL="$OAUTH_BROWSER_BASE_URL"
  OAUTH_RUN_NETWORK_ARGS=(--network "$SMOKE_DOCKER_NETWORK" --network-alias "$OAUTH_CONTAINER")
  PLAYWRIGHT_NETWORK_ARGS=(--network "$SMOKE_DOCKER_NETWORK")
  G5_API_CONNECTIVITY_BLOCK='    networks:
      - g5
      - smoke'
  EDGE_CONNECTIVITY_BLOCK="
  edge:
    networks:
      - g5
      - smoke

networks:
  smoke:
    name: $SMOKE_DOCKER_NETWORK
    external: true"
fi

selfhost() {
  G5_HOME="$INSTALL_DIR" \
  G5_DATA_DIR="$DATA_DIR" \
  G5_EXTRA_COMPOSE_FILE="$COMPOSE_OVERRIDE" \
    bash "$INSTALL_DIR/scripts/g5-selfhost.sh" "$@"
}

selfhost_with_manifest() {
  local manifest="$1"
  shift
  G5_HOME="$INSTALL_DIR" \
  G5_DATA_DIR="$DATA_DIR" \
  G5_EXTRA_COMPOSE_FILE="$COMPOSE_OVERRIDE" \
  G5_TARGET_MANIFEST_FILE="$manifest" \
    bash "$INSTALL_DIR/scripts/g5-selfhost.sh" "$@"
}

selfhost_with_manifest_and_wait_timeout() {
  local manifest="$1"
  local timeout="$2"
  shift 2
  G5_HOME="$INSTALL_DIR" \
  G5_DATA_DIR="$DATA_DIR" \
  G5_EXTRA_COMPOSE_FILE="$COMPOSE_OVERRIDE" \
  G5_TARGET_MANIFEST_FILE="$manifest" \
  G5_SELFHOST_WAIT_TIMEOUT="$timeout" \
    bash "$INSTALL_DIR/scripts/g5-selfhost.sh" "$@"
}

compose_sql() {
  G5_DATA_DIR="$DATA_DIR" \
    docker compose \
      --env-file "$INSTALL_DIR/.env" \
      -f "$INSTALL_DIR/compose.yml" \
      -f "$COMPOSE_OVERRIDE" \
      exec -T postgres \
      psql -U "${G5_DB_USER:-g5}" -d "${G5_DB_NAME:-g5}" -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  local status=$?
  set +e
  if [ "$status" -ne 0 ] && [ -f "$INSTALL_DIR/compose.yml" ] && [ -f "$INSTALL_DIR/.env" ] && [ -f "$COMPOSE_OVERRIDE" ]; then
    echo "--- self-hosted smoke compose ps ---" >&2
    G5_DATA_DIR="$DATA_DIR" docker compose \
      --env-file "$INSTALL_DIR/.env" \
      -f "$INSTALL_DIR/compose.yml" \
      -f "$COMPOSE_OVERRIDE" \
      ps >&2
    echo "--- self-hosted smoke compose logs ---" >&2
    G5_DATA_DIR="$DATA_DIR" docker compose \
      --env-file "$INSTALL_DIR/.env" \
      -f "$INSTALL_DIR/compose.yml" \
      -f "$COMPOSE_OVERRIDE" \
      logs --no-color --tail=200 postgres db-migrate g5-api g5-daemon edge >&2
  fi
  if [ -d "$INSTALL_DIR" ]; then
    G5_HOME="$INSTALL_DIR" \
    G5_DATA_DIR="$DATA_DIR" \
    G5_EXTRA_COMPOSE_FILE="$COMPOSE_OVERRIDE" \
      bash "$INSTALL_DIR/scripts/g5-selfhost.sh" down >/dev/null 2>&1
  fi
  docker rm -f "$OAUTH_CONTAINER" >/dev/null 2>&1
  rm -rf "$TMP" 2>/dev/null || {
    chmod -R u+rwX "$TMP" 2>/dev/null || true
    rm -rf "$TMP" 2>/dev/null || {
      if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
        sudo rm -rf "$TMP"
      else
        echo "warning: could not remove self-hosted smoke temp dir $TMP; root-owned Docker files may remain" >&2
      fi
    }
  }
  return "$status"
}
trap cleanup EXIT

docker build -t "$OAUTH_IMAGE" -f "$ROOT/infra/docker/oauth-stub/Dockerfile" "$ROOT/infra/docker/oauth-stub"

if is_true "$BUILD_NOOP_WORKER" && ! docker image inspect "$WORKER_IMAGE" >/dev/null 2>&1; then
  worker_build_dir="$TMP/worker-image"
  mkdir -p "$worker_build_dir"
  cat > "$worker_build_dir/Dockerfile" <<'EOF'
FROM alpine:3.22
RUN addgroup -S g5 && adduser -S -G g5 g5
USER g5
CMD ["sh", "-c", "while true; do mkdir -p /workspace/.g5 && sleep 5; done"]
EOF
  docker build -t "$WORKER_IMAGE" "$worker_build_dir"
fi

docker rm -f "$OAUTH_CONTAINER" >/dev/null 2>&1 || true
# Browser authorization enters through 127.0.0.1, while the API container
# exchanges the code through host.docker.internal. Bind the CI-only stub on the
# host gateway path too so Ubuntu runners exercise the real container callback.
docker run -d \
  --name "$OAUTH_CONTAINER" \
  "${OAUTH_RUN_NETWORK_ARGS[@]}" \
  -e OAUTH_PORT=9876 \
  -e OAUTH_AUTO_APPROVE=true \
  -e OAUTH_CLIENT_ID=local-oauth-client-id \
  -e OAUTH_CLIENT_SECRET=local-oauth-client-secret \
  -e OAUTH_USER_EMAIL=selfhost-smoke@g5.dev \
  -e OAUTH_USER_LOGIN=selfhost-smoke \
  -e "OAUTH_USER_NAME=Self Hosted Smoke" \
  "$OAUTH_IMAGE"
wait_http "$OAUTH_BROWSER_BASE_URL/healthz" "OAuth stub"

cat > "$TOKEN_RESPONSE" <<EOF
{
  "registry": "$SMOKE_REGISTRY",
  "username": "$SMOKE_REGISTRY_USERNAME",
  "password": "$SMOKE_REGISTRY_PASSWORD",
  "release": {
    "version": "selfhost-smoke",
    "image_refs": {
      "api": "$API_IMAGE",
      "edge": "$EDGE_IMAGE",
      "worker": "$WORKER_IMAGE"
    }
  }
}
EOF

cat > "$COMPOSE_OVERRIDE" <<EOF
services:
  g5-api:
$G5_API_CONNECTIVITY_BLOCK
    environment:
      G5_JWT_SECRET: |
$(printf '%s\n' "$PRIVATE_KEY_PEM" | sed 's/^/        /')
      G5_JWT_PRIVATE_KEY: |
$(printf '%s\n' "$PRIVATE_KEY_PEM" | sed 's/^/        /')
      G5_JWT_PUBLIC_KEY: |
$(printf '%s\n' "$PUBLIC_KEY_PEM" | sed 's/^/        /')

  g5-daemon:
    environment:
      G5_JWT_SECRET: |
$(printf '%s\n' "$PRIVATE_KEY_PEM" | sed 's/^/        /')
      G5_JWT_PRIVATE_KEY: |
$(printf '%s\n' "$PRIVATE_KEY_PEM" | sed 's/^/        /')
      G5_JWT_PUBLIC_KEY: |
$(printf '%s\n' "$PUBLIC_KEY_PEM" | sed 's/^/        /')
$EDGE_CONNECTIVITY_BLOCK
EOF

export G5_COMPOSE_PROJECT="$COMPOSE_PROJECT"
export G5_INSTALL_SKIP_DOCKER_LOGIN="$SKIP_DOCKER_LOGIN"
export G5_SELFHOST_SKIP_PULL="$SKIP_PULL"
export G5_SELFHOST_ALLOW_CHMOD_PERMISSION_FALLBACK=true
export G5_EXTRA_COMPOSE_FILE="$COMPOSE_OVERRIDE"
export G5_HTTP_PORT="$HTTP_PORT"
export G5_HTTPS_PORT="$HTTPS_PORT"
export G5_DB_PASSWORD="selfhost-smoke-db-password"
export G5_JWT_PRIVATE_KEY="placeholder-overridden-by-smoke-compose"
export G5_JWT_PUBLIC_KEY="placeholder-overridden-by-smoke-compose"
export G5_SCM_ENCRYPTION_KEY="$(openssl rand -hex 32)"
export G5_COOKIE_SECURE=false
export G5_GITHUB_CLIENT_ID=local-oauth-client-id
export G5_GITHUB_CLIENT_SECRET=local-oauth-client-secret
export G5_GITHUB_AUTHORIZE_URL="$OAUTH_BROWSER_BASE_URL/login/oauth/authorize"
export G5_GITHUB_TOKEN_URL="$OAUTH_API_BASE_URL/login/oauth/access_token"
export G5_GITHUB_USERINFO_URL="$OAUTH_API_BASE_URL/user"
export SELF_HOSTED=true
export ENABLE_CLOUD_FEATURES=false
export RUST_LOG="${RUST_LOG:-g5_api=info}"

bash "$BUNDLE/install.sh" \
  --license-key g5_smoke_offline \
  --api-url http://127.0.0.1:9999 \
  --token-response-file "$TOKEN_RESPONSE" \
  --bundle-dir "$BUNDLE" \
  --install-dir "$INSTALL_DIR" \
  --data-dir "$DATA_DIR" \
  --external-url "$BASE_URL"

wait_http "$BASE_URL/healthz" "G5 edge"
curl -fsS "$BASE_URL/api/v1/instance/info" > "$TMP/instance-info.json"
python3 - "$TMP/instance-info.json" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1]))
data = payload.get("data") or payload
providers = data.get("authProviders")
if providers != ["github"]:
    raise SystemExit(f"expected only github provider, got {providers!r}")
if data.get("devLoginEnabled") is not False:
    raise SystemExit(f"expected devLoginEnabled=false, got {data.get('devLoginEnabled')!r}")
if data.get("selfHosted") is not True:
    raise SystemExit(f"expected selfHosted=true, got {data.get('selfHosted')!r}")
if "cloud_workers" in data.get("features", []):
    raise SystemExit(f"self-hosted instance must not advertise cloud_workers, got {data.get('features')!r}")
if "codex_ui" not in data.get("features", []):
    raise SystemExit(f"self-hosted default must advertise the standard codex_ui capability, got {data.get('features')!r}")
PY

DEFAULT_SEED_ROW="$(
  compose_sql -At -F $'\t' <<'SQL'
WITH default_org AS (
  SELECT id FROM orgs WHERE slug = 'default'
),
base_ws AS (
  SELECT id
    FROM workspaces
   WHERE org_id = (SELECT id FROM default_org)
     AND name = 'Base'
     AND is_base
),
tinydb_repo AS (
  SELECT id, org_id, git_url
    FROM repos
   WHERE org_id = (SELECT id FROM default_org)
     AND slug = 'tinydb'
)
SELECT
  COALESCE((SELECT id::text FROM tinydb_repo), ''),
  COALESCE((SELECT org_id::text FROM tinydb_repo), ''),
  COALESCE((SELECT git_url FROM tinydb_repo), ''),
  (SELECT count(*) FROM base_ws),
  (SELECT count(*) FROM tinydb_repo),
  (SELECT count(*) FROM workspace_repos member
     JOIN base_ws w ON w.id = member.workspace_id
     JOIN tinydb_repo r ON r.id = member.repo_id);
SQL
)"
IFS=$'\t' read -r TINYDB_REPO_ID TINYDB_ORG_ID TINYDB_GIT_URL BASE_WORKSPACE_COUNT TINYDB_REPO_COUNT TINYDB_MEMBER_COUNT <<< "$DEFAULT_SEED_ROW"

[ -n "$TINYDB_REPO_ID" ] || die "self-hosted default TinyDB repo was not created"
[ "$TINYDB_GIT_URL" = "g5-local://default/tinydb" ] || die "expected TinyDB git_url g5-local://default/tinydb, got ${TINYDB_GIT_URL:-<empty>}"
[ "${BASE_WORKSPACE_COUNT:-0}" -eq 1 ] || die "expected one default Base workspace, got ${BASE_WORKSPACE_COUNT:-0}"
[ "${TINYDB_REPO_COUNT:-0}" -eq 1 ] || die "expected one TinyDB starter repo, got ${TINYDB_REPO_COUNT:-0}"
[ "${TINYDB_MEMBER_COUNT:-0}" -eq 1 ] || die "expected TinyDB to be bound to Base exactly once"

# Validate the selected committed files directly; no projection rows are produced.
TINYDB_DEFAULT_BRANCH=$(compose_sql -At -v repo_id="$TINYDB_REPO_ID" <<'SQL'
SELECT default_branch FROM repos WHERE id = :'repo_id'::uuid;
SQL
)
TINYDB_REPO_PATH="$DATA_DIR/storage/repos/$TINYDB_ORG_ID/$TINYDB_REPO_ID"
TINYDB_GIT_HEAD=$(git -c safe.directory='*' -C "$TINYDB_REPO_PATH" rev-parse --verify "refs/heads/${TINYDB_DEFAULT_BRANCH}^{commit}")
TINYDB_SPEC_PATHS=$(git -c safe.directory='*' -C "$TINYDB_REPO_PATH" ls-tree -r --name-only "$TINYDB_GIT_HEAD" -- .semlift/specs/)
TINYDB_SPEC_COUNT=$(printf '%s\n' "$TINYDB_SPEC_PATHS" | awk '/\.semlift\.md$/ { count++ } END { print count+0 }')
TINYDB_ROOT_COUNT=$(printf '%s\n' "$TINYDB_SPEC_PATHS" | awk '/^\.semlift\/specs\/[^/]+\.semlift\.md$/ { count++ } END { print count+0 }')
[ "$TINYDB_ROOT_COUNT" -eq 1 ] || die "expected one committed TinyDB root spec, got $TINYDB_ROOT_COUNT"
[ "$TINYDB_SPEC_COUNT" -ge 10 ] || die "expected committed TinyDB spec files, got $TINYDB_SPEC_COUNT"
for path in .semlift/specs/tinydb/database.semlift.md tinydb/database.py; do
  [ "$(git -c safe.directory='*' -C "$TINYDB_REPO_PATH" cat-file -t "${TINYDB_GIT_HEAD}:${path}")" = "blob" ] \
    || die "TinyDB selected commit is missing ${path}"
done

for endpoint in /api/v1/admin/licenses /api/v1/registry/token; do
  code="$(
    curl -sS -o /tmp/g5-selfhost-smoke-endpoint.out -w '%{http_code}' \
      -X POST "$BASE_URL$endpoint" \
      -H 'Content-Type: application/json' \
      -d '{}' || true
  )"
  [ "$code" = "404" ] || die "self-hosted endpoint $endpoint should be unavailable, got HTTP $code"
done
code="$(
  curl -sS -o /tmp/g5-selfhost-smoke-worker-host.out -w '%{http_code}' \
    -X POST "$BASE_URL/api/v1/orgs/selfhost-smoke/worker-hosts" \
    -H 'Content-Type: application/json' \
    -d '{}' || true
)"
case "$code" in
  401|403|404) ;;
  *) die "self-hosted worker-host provisioning should reject unauthenticated probes, got HTTP $code" ;;
esac

if ! is_true "$SKIP_PLAYWRIGHT"; then
  PLAYWRIGHT_SCRIPT="$TMP/playwright-login.js"
  write_playwright_login_script "$PLAYWRIGHT_SCRIPT"
  if command -v node >/dev/null 2>&1 && [ -d "$PLAYWRIGHT_MODULE" ]; then
    BASE_URL="$BASE_URL" PLAYWRIGHT_MODULE_PATH="$PLAYWRIGHT_MODULE" node "$PLAYWRIGHT_SCRIPT"
  else
    docker run --rm \
      "${PLAYWRIGHT_NETWORK_ARGS[@]}" \
      -e BASE_URL="$BASE_URL" \
      -e PLAYWRIGHT_NPM_PACKAGE="$PLAYWRIGHT_NPM_PACKAGE" \
      -v "$PLAYWRIGHT_SCRIPT:/tmp/playwright-login.js:ro" \
      "$PLAYWRIGHT_CONTAINER_IMAGE" \
      sh -ec '
        work=/tmp/g5-playwright
        mkdir -p "$work"
        cd "$work"
        npm init -y >/dev/null
        PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install --no-audit --no-fund "$PLAYWRIGHT_NPM_PACKAGE" >/dev/null
        PLAYWRIGHT_MODULE_PATH="$work/node_modules/playwright" node /tmp/playwright-login.js
      '
  fi
fi

selfhost logs g5-api >/tmp/g5-selfhost-smoke-logs.txt 2>&1 &
logs_pid=$!
sleep 5
kill "$logs_pid" >/dev/null 2>&1 || true
wait "$logs_pid" >/dev/null 2>&1 || true

compose_sql -c "CREATE TABLE IF NOT EXISTS selfhost_smoke_restore_marker (id text PRIMARY KEY, value text NOT NULL);"
compose_sql -c "INSERT INTO selfhost_smoke_restore_marker (id, value) VALUES ('marker', 'before-backup') ON CONFLICT (id) DO UPDATE SET value = EXCLUDED.value;"
selfhost backup
find "$INSTALL_DIR/backups" -mindepth 2 -maxdepth 2 -name postgres.dump -size +0c | grep -q . \
  || die "backup did not produce a non-empty postgres.dump"
BACKUP_PATH="$(find "$INSTALL_DIR/backups" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
[ -n "$BACKUP_PATH" ] || die "backup path was not created"

compose_sql -c "UPDATE selfhost_smoke_restore_marker SET value = 'after-backup' WHERE id = 'marker';"
selfhost restore "$BACKUP_PATH"
RESTORED_VALUE="$(compose_sql -At -c "SELECT value FROM selfhost_smoke_restore_marker WHERE id = 'marker';")"
[ "$RESTORED_VALUE" = "before-backup" ] \
  || die "restore did not recover backed-up database state, got $RESTORED_VALUE"
wait_http "$BASE_URL/healthz" "G5 edge after restore"

UPGRADE_MANIFEST="$TMP/manifest-upgrade-success.json"
write_release_manifest "selfhost-smoke-upgrade" "$API_IMAGE" "$EDGE_IMAGE" "$WORKER_IMAGE" "$API_IMAGE" "$UPGRADE_MANIFEST"
selfhost_with_manifest "$UPGRADE_MANIFEST" upgrade
grep -q '^G5_RELEASE_VERSION=selfhost-smoke-upgrade$' "$INSTALL_DIR/.env" \
  || die "successful upgrade did not promote release version"
[ -f "$INSTALL_DIR/releases/selfhost-smoke-upgrade/upgrade.json" ] \
  || die "successful upgrade did not write release record"
wait_http "$BASE_URL/healthz" "G5 edge after upgrade"

FAIL_MIGRATION_IMAGE="g5-selfhost-smoke-failing-migration:local"
FAIL_MIGRATION_DIR="$TMP/failing-migration-image"
mkdir -p "$FAIL_MIGRATION_DIR"
cat > "$FAIL_MIGRATION_DIR/Dockerfile" <<'EOF'
FROM alpine:3.22
RUN printf '#!/bin/sh\nexit 42\n' > /usr/local/bin/sqlx \
  && printf '#!/bin/sh\nexit 0\n' > /usr/local/bin/sleep \
  && chmod +x /usr/local/bin/sqlx /usr/local/bin/sleep
EOF
docker build -t "$FAIL_MIGRATION_IMAGE" "$FAIL_MIGRATION_DIR"
FAIL_MIGRATION_MANIFEST="$TMP/manifest-failing-migration.json"
write_release_manifest "selfhost-smoke-bad-migration" "$API_IMAGE" "$EDGE_IMAGE" "$WORKER_IMAGE" "$FAIL_MIGRATION_IMAGE" "$FAIL_MIGRATION_MANIFEST"
if selfhost_with_manifest "$FAIL_MIGRATION_MANIFEST" upgrade; then
  die "upgrade unexpectedly succeeded with a failing migration image"
fi
grep -q '^G5_RELEASE_VERSION=selfhost-smoke-upgrade$' "$INSTALL_DIR/.env" \
  || die "failed migration upgrade changed the active release"
wait_http "$BASE_URL/healthz" "G5 edge after migration rollback"

BAD_API_IMAGE="g5-selfhost-smoke-bad-api:local"
BAD_API_DIR="$TMP/bad-api-image"
mkdir -p "$BAD_API_DIR"
cat > "$BAD_API_DIR/Dockerfile" <<'EOF'
FROM alpine:3.22
CMD ["sh", "-c", "while true; do sleep 3600; done"]
EOF
docker build -t "$BAD_API_IMAGE" "$BAD_API_DIR"
BAD_HEALTH_MANIFEST="$TMP/manifest-bad-health.json"
write_release_manifest "selfhost-smoke-bad-health" "$BAD_API_IMAGE" "$EDGE_IMAGE" "$WORKER_IMAGE" "$API_IMAGE" "$BAD_HEALTH_MANIFEST"
if selfhost_with_manifest_and_wait_timeout "$BAD_HEALTH_MANIFEST" 45 upgrade; then
  die "upgrade unexpectedly succeeded with an unhealthy API image"
fi
grep -q '^G5_RELEASE_VERSION=selfhost-smoke-upgrade$' "$INSTALL_DIR/.env" \
  || die "failed health upgrade changed the active release"
wait_http "$BASE_URL/healthz" "G5 edge after health rollback"

# Codex must be available in a default installation.
codex_ui_seen=""
for _ in $(seq 1 30); do
  instance_info="$(curl -fsS "$BASE_URL/api/v1/instance/info" 2>/dev/null || true)"
  if printf '%s' "$instance_info" | grep -q '"codex_ui"'; then
    codex_ui_seen=1
    break
  fi
  sleep 2
done
[ -n "$codex_ui_seen" ] \
  || die "codex_ui missing from instance info in the default installation"

selfhost down

echo "self-hosted smoke test passed"
