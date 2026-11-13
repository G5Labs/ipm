#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$ROOT/infra/self-hosted"

python3 "$ROOT/scripts/test_install_worker_service.py"

fail() {
  echo "self-hosted contract test failed: $*" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  grep -F -- "$needle" "$haystack" >/dev/null || fail "missing '$needle' in $haystack"
}

assert_not_contains() {
  local file="$1"
  local needle="$2"
  if grep -F -- "$needle" "$file" >/dev/null; then
    fail "unexpected '$needle' in $file"
  fi
}

# A whole dry-run command line, anchored: `postgres$` does not match a
# retained `postgres redis`.
assert_line() {
  local file="$1"
  local regex="$2"
  grep -E -- "$regex" "$file" >/dev/null || fail "no line matching '$regex' in $file"
}

# The first line matching `before` precedes the first line matching `after`.
assert_line_before() {
  local file="$1"
  local before="$2"
  local after="$3"
  local b a
  b="$(grep -n -E -- "$before" "$file" | head -n 1 | cut -d: -f1)"
  a="$(grep -n -E -- "$after" "$file" | head -n 1 | cut -d: -f1)"
  [ -n "$b" ] || fail "no line matching '$before' in $file"
  [ -n "$a" ] || fail "no line matching '$after' in $file"
  [ "$b" -lt "$a" ] || fail "'$before' (line $b) must precede '$after' (line $a) in $file"
}

require_file() {
  [ -f "$1" ] || fail "missing file $1"
}

require_file "$BUNDLE/Justfile"
require_file "$BUNDLE/compose.yml"
require_file "$BUNDLE/compose.tls.yml"
require_file "$BUNDLE/Caddyfile.tls"
require_file "$BUNDLE/env.template"
require_file "$BUNDLE/install.sh"
require_file "$BUNDLE/scripts/g5-selfhost.sh"
require_file "$BUNDLE/scripts/g5-workerhost.sh"
require_file "$BUNDLE/scripts/apply-install-env.py"
require_file "$BUNDLE/scripts/render-selfhost-env.py"
require_file "$BUNDLE/scripts/token-response-to-manifest.py"
require_file "$BUNDLE/starter-repos/tinydb/README.md"
require_file "$BUNDLE/starter-repos/tinydb/tinydb/database.py"
require_file "$BUNDLE/starter-repos/tinydb/.semlift/specs/tinydb/database.semlift.md"
require_file "$BUNDLE/starter-repos/tinydb/.semlift/specs/tinydb_aggregate.semlift.md"
require_file "$ROOT/scripts/package-self-hosted-release-assets.sh"
require_file "$ROOT/scripts/test-self-hosted-smoke.sh"
# Every helper build-runtime-disk.sh invokes must be packaged beside it: the
# public installer's runtime staging runs them from the bundle's scripts
# directory (a missing stage-runtime-platform.sh broke fresh installs in
# review of PR #914).
for helper in build-runtime-disk.sh stage-guest-bootstrap.py stage-runtime-platform.sh; do
  assert_contains "$ROOT/scripts/package-self-hosted-release-assets.sh" "\"\$STAGE/scripts/$helper\""
done
for helper in $(grep -oE '"\$repo/scripts/[A-Za-z0-9._-]+"' "$ROOT/scripts/build-runtime-disk.sh" | sed 's#.*/scripts/##; s/"$//'); do
  assert_contains "$ROOT/scripts/package-self-hosted-release-assets.sh" "\"\$STAGE/scripts/$helper\""
done

assert_contains "$BUNDLE/env.template" "G5_GITHUB_CLIENT_ID="
assert_contains "$BUNDLE/env.template" "G5_SELF_HOSTED_DEFAULT_ORG_NAME='Default Organization'"
assert_contains "$BUNDLE/env.template" "G5_SELF_HOSTED_DEFAULT_ORG_SLUG=default"
assert_contains "$BUNDLE/env.template" "SELF_HOSTED=true"
assert_not_contains "$BUNDLE/env.template" "G5D_REGISTRATION_TOKEN="
assert_not_contains "$BUNDLE/env.template" "G5_ACK_PROCESS_ISOLATION="
assert_contains "$BUNDLE/env.template" "G5_RATE_LIMIT_ENABLED=true"
assert_contains "$BUNDLE/env.template" "G5_RATE_LIMIT_RPM=20000"
assert_contains "$BUNDLE/env.template" "G5_TLS_CERT_FILE="
assert_contains "$BUNDLE/env.template" "G5_TLS_KEY_FILE="
assert_contains "$BUNDLE/env.template" "G5_GITHUB_AUTHORIZE_URL="
assert_contains "$BUNDLE/env.template" "G5_GITHUB_TOKEN_URL="
assert_contains "$BUNDLE/env.template" "G5_GITHUB_USERINFO_URL="
assert_contains "$BUNDLE/env.template" "G5_EMAIL_TRANSPORT=disabled"
assert_contains "$BUNDLE/compose.yml" "G5_GITHUB_CLIENT_SECRET:"
assert_contains "$BUNDLE/env.template" "G5_MICROSOFT_CLIENT_ID="
assert_contains "$BUNDLE/env.template" "G5_MICROSOFT_TENANT_ID="
assert_contains "$BUNDLE/compose.yml" "G5_MICROSOFT_CLIENT_SECRET:"
assert_contains "$BUNDLE/compose.yml" "G5_MICROSOFT_TENANT_ID:"
assert_contains "$BUNDLE/scripts/apply-install-env.py" "G5_MICROSOFT_TENANT_ID"
for retired in G5_GOOGLE_ G5_GITLAB_ G5_OAUTH_ENABLED_PROVIDERS G5_MICROSOFT_DEFAULT_ORG_ROLE_MAP G5_AUTH_ALLOWED_EMAILS; do
  assert_not_contains "$BUNDLE/env.template" "$retired"
  assert_not_contains "$BUNDLE/compose.yml" "$retired"
  assert_not_contains "$BUNDLE/scripts/apply-install-env.py" "$retired"
done
assert_contains "$BUNDLE/compose.yml" 'G5_RATE_LIMIT_RPM: ${G5_RATE_LIMIT_RPM:-20000}'
assert_contains "$BUNDLE/compose.yml" "SELF_HOSTED:"
assert_not_contains "$BUNDLE/compose.yml" "G5D_REGISTRATION_TOKEN:"
assert_not_contains "$BUNDLE/compose.yml" "g5d-worker:"
assert_contains "$BUNDLE/compose.yml" "G5_GITHUB_AUTHORIZE_URL:"
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" "G5_EXTRA_COMPOSE_FILE"
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" "validate_tls_configuration"
assert_contains "$BUNDLE/scripts/apply-install-env.py" '"G5_TLS_CERT_FILE",'
assert_contains "$BUNDLE/scripts/apply-install-env.py" '"G5_TLS_KEY_FILE",'
assert_contains "$BUNDLE/compose.tls.yml" '/etc/caddy/tls/fullchain.pem:ro'
assert_contains "$BUNDLE/compose.tls.yml" '/etc/caddy/tls/privkey.pem:ro'
assert_contains "$BUNDLE/Caddyfile.tls" 'tls /etc/caddy/tls/fullchain.pem /etc/caddy/tls/privkey.pem'
assert_contains "$BUNDLE/scripts/apply-install-env.py" '"G5_EXTRA_COMPOSE_FILE"'
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" "G5_SELFHOST_SKIP_PULL"
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" 'DOCKER_CONFIG="${DOCKER_CONFIG:-$G5_HOME/.docker}"'
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" "ensure_docker_compose_available"
assert_not_contains "$BUNDLE/scripts/g5-selfhost.sh" "repair_worker_workspace_permissions"
assert_not_contains "$BUNDLE/scripts/g5-selfhost.sh" "ontology_nodes"
assert_contains "$BUNDLE/scripts/g5-selfhost.sh" 'git -c "safe.directory=$repo_path" -C "$repo_path"'
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "fetch_registry_token_if_needed"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "write_docker_auth_config_from_token"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" 'docker pull "$G5_IMAGE_WORKER"'
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "G5_HOST_JWT is required"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "G5D_HOST_ID is required"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "microvm"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "stage-local-worker-runtime.sh"
assert_contains "$BUNDLE/scripts/g5-workerhost.sh" "install-worker-service.py"
require_file "$BUNDLE/scripts/install-worker-service.py"
assert_not_contains "$BUNDLE/scripts/g5-workerhost.sh" "G5D_WORKER_DISPLAY_NAME"
assert_not_contains "$BUNDLE/scripts/g5-workerhost.sh" "G5_ACK_PROCESS_ISOLATION"
assert_contains "$ROOT/scripts/package-self-hosted-release-assets.sh" \
  'stage-local-worker-runtime.sh'
assert_contains "$BUNDLE/install.sh" "require_cmd git"
assert_contains "$BUNDLE/install.sh" "--worker-host"
assert_contains "$BUNDLE/install.sh" "g5-workerhost.sh"
assert_contains "$BUNDLE/install.sh" "G5_GITHUB_RELEASE_REPO"
assert_contains "$BUNDLE/install.sh" "g5-self-hosted-bundle.tar.gz"

for recipe in setup up logs down backup restore upgrade; do
  if ! grep -Eq "^${recipe}([[:space:]].*)?:" "$BUNDLE/Justfile"; then
    fail "Justfile missing recipe: $recipe"
  fi
done

python3 - "$BUNDLE/compose.yml" <<'PY'
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text()
required_services = [
    "postgres:",
    "db-migrate:",
    "g5-api:",
    "g5-daemon:",
    "edge:",
]
forbidden_services = ["redis:"]
present = [service for service in forbidden_services if f"\n  {service}" in text]
if present:
    sys.exit(f"compose.yml must not define: {', '.join(present)}")
missing = [service for service in required_services if f"\n  {service}" not in text]
if missing:
    raise SystemExit(f"compose.yml missing services: {', '.join(missing)}")

for expected in ["${G5_IMAGE_API", "${G5_IMAGE_EDGE"]:
    if expected not in text:
        raise SystemExit(f"compose.yml missing manifest image reference {expected}")

if "latest" in text:
    raise SystemExit("compose.yml must not default customer runtime images to latest")

if "g5-db-migrate" not in text:
    raise SystemExit("compose.yml must run the repair-aware g5-db-migrate entrypoint")

if "sqlx migrate run" in text:
    raise SystemExit("compose.yml must not bypass migration repairs with raw sqlx migrate run")

if "/dev/kvm:/dev/kvm" in text:
    raise SystemExit("control-plane compose.yml must not run workspace hosts")
PY

TMP="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT

INSTALL="$TMP/opt-g5"
DATA="$TMP/var-lib-g5"
mkdir -p "$INSTALL" "$DATA"
cp -R "$BUNDLE/." "$INSTALL/"
cp "$INSTALL/env.template" "$INSTALL/.env"
cat >> "$INSTALL/.env" <<'EOF'
G5_RELEASE_VERSION=contract-test
G5_IMAGE_API=example.com/g5/api@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
G5_IMAGE_EDGE=example.com/g5/edge@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
G5_IMAGE_WORKER=example.com/g5/worker@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
G5_IMAGE_MIGRATION=example.com/g5/api@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
G5_DB_PASSWORD=contract-db-password
G5_JWT_PRIVATE_KEY=contract-private-key
G5_JWT_PUBLIC_KEY=contract-public-key
G5_SCM_ENCRYPTION_KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
G5_EXTERNAL_URL=https://g5.contract.test
EOF
# A real install's .env names its own install directory; the template's
# /opt/g5 would send upgrade's lock, manifest and records elsewhere.
printf 'G5_HOME=%s\n' "$INSTALL" >> "$INSTALL/.env"
chmod 0600 "$INSTALL/.env"

# Mounted TLS contract: blank rows preserve base behavior; valid paired
# absolute files select the bundled overlay; partial, relative, and invalid
# paths fail before docker compose can mutate containers.
TLS_CERT="$TMP/tls-fullchain.pem"
TLS_KEY="$TMP/tls-privkey.pem"
TLS_CADDYFILE="$TMP/Caddyfile.custom-tls"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
  -subj /CN=g5.contract.test \
  -keyout "$TLS_KEY" -out "$TLS_CERT" >/dev/null 2>&1
cp "$INSTALL/Caddyfile.tls" "$TLS_CADDYFILE"

TLS_ENV="$TMP/tls.env"
cp "$INSTALL/.env" "$TLS_ENV"
cat >> "$TLS_ENV" <<EOF
G5_TLS_CERT_FILE=$TLS_CERT
G5_TLS_KEY_FILE=$TLS_KEY
G5_TLS_CADDYFILE=$TLS_CADDYFILE
EOF
chmod 0600 "$TLS_ENV"

TLS_LOG="$TMP/tls.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$TLS_ENV" \
  bash "$INSTALL/scripts/g5-selfhost.sh" down > "$TLS_LOG"
assert_contains "$TLS_LOG" "$INSTALL/compose.tls.yml"

BLANK_TLS_ENV="$TMP/blank-tls.env"
cp "$INSTALL/.env" "$BLANK_TLS_ENV"
cat >> "$BLANK_TLS_ENV" <<'EOF'
G5_TLS_CERT_FILE='   '
G5_TLS_KEY_FILE='   '
EOF
chmod 0600 "$BLANK_TLS_ENV"
BLANK_TLS_LOG="$TMP/blank-tls.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$BLANK_TLS_ENV" \
  bash "$INSTALL/scripts/g5-selfhost.sh" down > "$BLANK_TLS_LOG"
assert_not_contains "$BLANK_TLS_LOG" "compose.tls.yml"

assert_tls_rejected() {
  local env_file="$1"
  local expected="$2"
  local output="$3"
  chmod 0600 "$env_file"
  if G5_HOME="$INSTALL" \
    G5_DATA_DIR="$DATA" \
    G5_SELFHOST_DRY_RUN=true \
    G5_ENV_FILE="$env_file" \
      bash "$INSTALL/scripts/g5-selfhost.sh" down > "$output" 2>&1; then
    fail "invalid TLS configuration was accepted: $env_file"
  fi
  assert_contains "$output" "$expected"
  assert_not_contains "$output" "+ docker compose"
}

PARTIAL_TLS_ENV="$TMP/partial-tls.env"
cp "$INSTALL/.env" "$PARTIAL_TLS_ENV"
printf 'G5_TLS_CERT_FILE=%s\n' "$TLS_CERT" >> "$PARTIAL_TLS_ENV"
assert_tls_rejected "$PARTIAL_TLS_ENV" \
  "G5_TLS_CERT_FILE and G5_TLS_KEY_FILE must be configured together" \
  "$TMP/partial-tls.log"

RELATIVE_TLS_ENV="$TMP/relative-tls.env"
cp "$INSTALL/.env" "$RELATIVE_TLS_ENV"
cat >> "$RELATIVE_TLS_ENV" <<'EOF'
G5_TLS_CERT_FILE=tls/fullchain.pem
G5_TLS_KEY_FILE=tls/privkey.pem
EOF
assert_tls_rejected "$RELATIVE_TLS_ENV" \
  "G5_TLS_CERT_FILE must be an absolute host path" \
  "$TMP/relative-tls.log"

MISSING_TLS_ENV="$TMP/missing-tls.env"
cp "$INSTALL/.env" "$MISSING_TLS_ENV"
cat >> "$MISSING_TLS_ENV" <<EOF
G5_TLS_CERT_FILE=$TMP/missing-fullchain.pem
G5_TLS_KEY_FILE=$TLS_KEY
EOF
assert_tls_rejected "$MISSING_TLS_ENV" \
  "G5_TLS_CERT_FILE must name a readable regular file" \
  "$TMP/missing-tls.log"

docker compose --env-file "$TLS_ENV" \
  -f "$INSTALL/compose.yml" -f "$INSTALL/compose.tls.yml" \
  config > "$TMP/tls-compose-config.yml"
assert_contains "$TMP/tls-compose-config.yml" "source: $TLS_CERT"
assert_contains "$TMP/tls-compose-config.yml" "source: $TLS_CADDYFILE"
assert_contains "$TMP/tls-compose-config.yml" "target: /etc/caddy/Caddyfile"
assert_contains "$TMP/tls-compose-config.yml" "target: /etc/caddy/tls/fullchain.pem"
assert_contains "$TMP/tls-compose-config.yml" "read_only: true"

docker run --rm \
  -v "$INSTALL/Caddyfile.tls:/etc/caddy/Caddyfile:ro" \
  -v "$TLS_CERT:/etc/caddy/tls/fullchain.pem:ro" \
  -v "$TLS_KEY:/etc/caddy/tls/privkey.pem:ro" \
  caddy:2.10.0-alpine \
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile \
  > "$TMP/caddy-validate.log"

cat > "$TMP/token.json" <<'EOF'
{
  "registry": "example.com",
  "username": "oauth2accesstoken",
  "password": "short-lived-token",
  "release": {
    "version": "contract-upgrade",
    "image_refs": {
      "api": "example.com/g5/api@sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "edge": "example.com/g5/edge@sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "worker": "example.com/g5/worker@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    }
  }
}
EOF

python3 "$INSTALL/scripts/token-response-to-manifest.py" "$TMP/token.json" "$TMP/manifest.json"
python3 "$INSTALL/scripts/render-selfhost-env.py" "$INSTALL/.env" "$TMP/manifest.json" "$TMP/.env.next"
assert_contains "$TMP/.env.next" "G5_RELEASE_VERSION=contract-upgrade"
assert_contains "$TMP/.env.next" "G5_IMAGE_API=example.com/g5/api@sha256:dddd"

RELEASE_ASSETS="$TMP/release-assets"
G5_RELEASE_TAG=v9.9.9 bash "$ROOT/scripts/package-self-hosted-release-assets.sh" "$RELEASE_ASSETS" >/dev/null
require_file "$RELEASE_ASSETS/install.sh"
require_file "$RELEASE_ASSETS/g5-self-hosted-bundle.tar.gz"
require_file "$RELEASE_ASSETS/SHA256SUMS"
for packaged_path in \
  ./scripts/stage-local-worker-runtime.sh \
  ./scripts/apt-install-with-retry.sh \
  ./scripts/check-claude-cli-flags.sh \
  ./scripts/claude-code-cli-version \
  ./scripts/codex-cli-version \
  ./infra/packer/g5-worker/scripts/57-install-test-tools.sh \
  ./infra/packer/g5-worker/scripts/lib/download-with-retry.sh; do
  tar -tzf "$RELEASE_ASSETS/g5-self-hosted-bundle.tar.gz" | \
    grep -Fx -- "$packaged_path" >/dev/null \
    || fail "public bundle missing worker runtime support file: $packaged_path"
done
assert_contains "$RELEASE_ASSETS/install.sh" 'G5_INSTALL_RELEASE="${G5_INSTALL_RELEASE:-v9.9.9}"'
assert_contains "$RELEASE_ASSETS/install.sh" 'G5_GITHUB_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-G5Labs/Releases}"'
assert_contains "$RELEASE_ASSETS/SHA256SUMS" "install.sh"
assert_contains "$RELEASE_ASSETS/SHA256SUMS" "g5-self-hosted-bundle.tar.gz"
if tar -tzf "$RELEASE_ASSETS/g5-self-hosted-bundle.tar.gz" | grep -E '(^|/)g5d(-|$)' >/dev/null; then
  fail "public self-hosted release bundle must not include gated g5d binaries"
fi
if tar -tzf "$RELEASE_ASSETS/g5-self-hosted-bundle.tar.gz" | grep -E '(^|/)(\._|\.DS_Store$|__MACOSX(/|$))' >/dev/null; then
  fail "public self-hosted release bundle must not include host metadata files"
fi

MULTILINE_ENV="$TMP/multiline.env"
cp "$BUNDLE/env.template" "$MULTILINE_ENV"
G5_JWT_PRIVATE_KEY=$'-----BEGIN PRIVATE KEY-----\nprivate\n-----END PRIVATE KEY-----' \
G5_JWT_PUBLIC_KEY=$'-----BEGIN PUBLIC KEY-----\npublic\n-----END PUBLIC KEY-----' \
G5_DB_PASSWORD="password with spaces" \
G5_EDGE_SUBNET=172.30.253.0/29 G5_EDGE_IP=172.30.253.2 G5_API_EDGE_IP=172.30.253.3 \
  python3 "$BUNDLE/scripts/apply-install-env.py" \
    "$MULTILINE_ENV" \
    g5_contract_license \
    https://api.contract.test \
    "$TMP/install-target" \
    "$TMP/install-data" \
    https://g5.contract.test
assert_contains "$MULTILINE_ENV" 'G5_JWT_PRIVATE_KEY=$'\'''
assert_contains "$MULTILINE_ENV" '\nprivate\n'
assert_contains "$MULTILINE_ENV" 'G5_EDGE_SUBNET=172.30.253.0/29'
assert_contains "$MULTILINE_ENV" 'G5_EDGE_IP=172.30.253.2'
assert_contains "$MULTILINE_ENV" 'G5_API_EDGE_IP=172.30.253.3'
set -a
# shellcheck disable=SC1090
. "$MULTILINE_ENV"
set +a
python3 - <<'PY'
import os

private_key = os.environ["G5_JWT_PRIVATE_KEY"]
if "\nprivate\n" not in private_key:
    raise SystemExit("G5_JWT_PRIVATE_KEY did not round-trip multiline PEM text")
if os.environ["G5_DB_PASSWORD"] != "password with spaces":
    raise SystemExit("G5_DB_PASSWORD did not round-trip quoted text")
PY

# Codex is part of every new installation; no opt-in belongs in the bundle.
if grep -q 'G5_CODEX_UI_ENABLED' "$BUNDLE/env.template" "$BUNDLE/compose.yml" "$BUNDLE/scripts/apply-install-env.py"; then
  fail "retired Codex UI deployment gate remains in the bundle"
fi

EC_ENV="$TMP/ec-private.env"
cp "$BUNDLE/env.template" "$EC_ENV"
if G5_JWT_PRIVATE_KEY=$'-----BEGIN EC PRIVATE KEY-----\nprivate\n-----END EC PRIVATE KEY-----' \
  python3 "$BUNDLE/scripts/apply-install-env.py" \
    "$EC_ENV" \
    g5_contract_license \
    https://api.contract.test \
    "$TMP/install-target" \
    "$TMP/install-data" \
    https://g5.contract.test > "$TMP/ec-private.out" 2> "$TMP/ec-private.err"; then
  fail "apply-install-env.py accepted legacy EC private key PEM"
fi
assert_contains "$TMP/ec-private.err" "PKCS#8 P-256 PEM"
assert_contains "$TMP/ec-private.err" "openssl genpkey"

UP_LOG="$TMP/up.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$INSTALL/.env" \
  bash "$INSTALL/scripts/g5-selfhost.sh" up > "$UP_LOG"
assert_contains "$UP_LOG" "docker compose"
assert_line "$UP_LOG" ' up -d --remove-orphans --wait --wait-timeout 180 postgres$'
assert_line_before "$UP_LOG" ' up -d --remove-orphans .* postgres$' 'run --rm db-migrate'
assert_line "$UP_LOG" ' up -d --wait --wait-timeout 180 g5-api g5-daemon edge$'
assert_not_contains "$UP_LOG" "redis"
assert_contains "$UP_LOG" "run --rm db-migrate"
assert_contains "$UP_LOG" "bootstrap self-hosted default org"
assert_not_contains "$UP_LOG" "g5d-worker"
assert_not_contains "$UP_LOG" " pull"

DOWN_LOG="$TMP/down.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$INSTALL/.env" \
  bash "$INSTALL/scripts/g5-selfhost.sh" down > "$DOWN_LOG"
assert_contains "$DOWN_LOG" "down --remove-orphans"
assert_not_contains "$DOWN_LOG" " -v"

# Retire orphaned services before migrations, including the renamed maintenance
# service, so an old writer cannot survive the cutover. This runs before the
# restore block creates a backup directory, and stages no manifest: a fresh dry
# run with a supplied manifest must need neither.
UPGRADE_LOG="$TMP/upgrade.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$INSTALL/.env" \
G5_TARGET_MANIFEST_FILE="$TMP/manifest.json" \
  bash "$INSTALL/scripts/g5-selfhost.sh" upgrade > "$UPGRADE_LOG"
assert_line "$UPGRADE_LOG" ' up -d --remove-orphans --wait --wait-timeout 180 postgres$'
assert_contains "$UPGRADE_LOG" "run --rm db-migrate"
assert_line_before "$UPGRADE_LOG" ' up -d --remove-orphans .* postgres$' 'run --rm db-migrate'
assert_line "$UPGRADE_LOG" ' up -d --wait --wait-timeout 180 g5-api g5-daemon edge$'
assert_not_contains "$UPGRADE_LOG" "redis"
# The preview names the release being installed, not the one still promoted.
assert_contains "$UPGRADE_LOG" "would record the upgrade in $INSTALL/releases/contract-upgrade/upgrade.json"
assert_line "$UPGRADE_LOG" '^g5-selfhost: upgrade complete: contract-upgrade$'

# A failed upgrade from that older bundle rolls back by recreating the
# previous release's images from this Compose file with the previous .env,
# which still names the Redis they used; the setting must reach both images.
# A fresh install's .env names none, and the current release gets none.
# Compose lets the shell's environment override an env file, and this test
# exports the multiline env above into its own shell, so render each file
# with its keys unset, and with the Redis URL unset even when the file has
# none (a fresh install's), so an inherited one cannot leak into the render.
render_compose_json() {
  local env_file="$1"
  local out="$2"
  (
    # shellcheck disable=SC2046
    unset $(sed -n 's/^\([A-Za-z_][A-Za-z0-9_]*\)=.*/\1/p' "$env_file" | sort -u) G5_REDIS_URL G5_EDGE_SUBNET G5_EDGE_IP G5_API_EDGE_IP
    docker compose --env-file "$env_file" -f "$INSTALL/compose.yml" config --format json > "$out"
  )
}
ROLLBACK_ENV="$TMP/rollback.env"
cp "$INSTALL/.env" "$ROLLBACK_ENV"
printf 'G5_REDIS_URL=redis://redis:6379\n' >> "$ROLLBACK_ENV"
render_compose_json "$ROLLBACK_ENV" "$TMP/rollback-config.json"
render_compose_json "$INSTALL/.env" "$TMP/fresh-config.json"
python3 - "$TMP/rollback-config.json" "$TMP/fresh-config.json" <<'PY'
import json, sys
rollback = json.load(open(sys.argv[1]))["services"]
fresh = json.load(open(sys.argv[2]))["services"]
for name in ("g5-api", "g5-daemon"):
    got = rollback[name].get("environment", {}).get("G5_REDIS_URL")
    if got != "redis://redis:6379":
        raise SystemExit(f"{name} must receive the previous release's G5_REDIS_URL on rollback, got {got!r}")
    if fresh[name].get("environment", {}).get("G5_REDIS_URL"):
        raise SystemExit(f"{name} must not point a fresh install at a Redis")
PY

# Defaults also apply when upgrading an old .env that has no edge settings.
CUSTOM_EDGE_ENV="$TMP/custom-edge.env"
cp "$INSTALL/.env" "$CUSTOM_EDGE_ENV"
printf 'G5_EDGE_SUBNET=172.30.252.0/29\nG5_EDGE_IP=172.30.252.2\nG5_API_EDGE_IP=172.30.252.3\n' >> "$CUSTOM_EDGE_ENV"
render_compose_json "$CUSTOM_EDGE_ENV" "$TMP/custom-edge-config.json"
python3 - "$TMP/fresh-config.json" "$TMP/custom-edge-config.json" <<'PYIP'
import ipaddress, json, sys
for path, subnet, edge, api in [
    (sys.argv[1], "172.30.254.0/29", "172.30.254.2", "172.30.254.3"),
    (sys.argv[2], "172.30.252.0/29", "172.30.252.2", "172.30.252.3"),
]:
    model = json.load(open(path))
    services = model["services"]
    assert services["g5-api"]["environment"]["G5_TRUSTED_PROXY_CIDRS"] == edge + "/32"
    assert services["g5-api"]["environment"]["G5_TRUSTED_PROXY_XFF_HOPS"] == "1"
    assert services["edge"]["networks"] == {"edge": {"ipv4_address": edge}}
    assert services["g5-api"]["networks"]["edge"]["ipv4_address"] == api
    assert "g5" in services["g5-api"]["networks"]
    assert all("edge" not in config.get("networks", {}) for name, config in services.items() if name not in {"edge", "g5-api"})
    assert model["networks"]["edge"]["ipam"]["config"][0]["subnet"] == subnet
    assert not model["networks"]["g5"].get("ipam")
    assert edge != api and all(ipaddress.ip_address(ip) in ipaddress.ip_network(subnet) for ip in (edge, api))
PYIP

RESTORE_DIR="$INSTALL/backups/contract-restore"
mkdir -p "$RESTORE_DIR"
printf 'contract dump\n' > "$RESTORE_DIR/postgres.dump"
RESTORE_LOG="$TMP/restore.log"
G5_HOME="$INSTALL" \
G5_DATA_DIR="$DATA" \
G5_SELFHOST_DRY_RUN=true \
G5_ENV_FILE="$INSTALL/.env" \
  bash "$INSTALL/scripts/g5-selfhost.sh" restore "$RESTORE_DIR" > "$RESTORE_LOG"
assert_line "$RESTORE_LOG" ' up -d --remove-orphans --wait --wait-timeout 180 postgres$'
assert_not_contains "$RESTORE_LOG" "redis"
assert_contains "$RESTORE_LOG" "dropdb"
assert_contains "$RESTORE_LOG" "pg_restore"
assert_not_contains "$RESTORE_LOG" "g5d-worker"

INSTALL_LOG="$TMP/install.log"
G5_INSTALL_DRY_RUN=true \
G5_BUNDLE_DIR="$BUNDLE" \
G5_REGISTRY_TOKEN_RESPONSE_FILE="$TMP/token.json" \
  bash "$BUNDLE/install.sh" \
    --token-response-file "$TMP/token.json" \
    --bundle-dir "$BUNDLE" \
    --install-dir "$TMP/install-target" \
    --data-dir "$TMP/install-data" \
    --external-url https://g5.contract.test \
    --dry-run > "$INSTALL_LOG"
assert_contains "$INSTALL_LOG" "write docker auth config"
assert_contains "$INSTALL_LOG" "just setup"
assert_contains "$INSTALL_LOG" "just up"

WORKER_INSTALL_LOG="$TMP/worker-install.log"
G5_INSTALL_DRY_RUN=true \
G5_BUNDLE_DIR="$BUNDLE" \
G5_REGISTRY_TOKEN_RESPONSE_FILE="$TMP/token.json" \
  bash "$BUNDLE/install.sh" \
    --worker-host \
    --token-response-file "$TMP/token.json" \
    --bundle-dir "$BUNDLE" \
    --install-dir "$TMP/worker-install-target" \
    --data-dir "$TMP/worker-install-data" \
    --external-url https://g5.contract.test \
    --host-jwt contract.host.jwt \
    --host-id 00000000-0000-0000-0000-000000000002 \
    --host-org-id 00000000-0000-0000-0000-000000000001 \
    --dry-run > "$WORKER_INSTALL_LOG"
assert_contains "$WORKER_INSTALL_LOG" "render worker release manifest"
assert_contains "$WORKER_INSTALL_LOG" "g5-workerhost.sh"
assert_contains "$WORKER_INSTALL_LOG" "G5_HOST_JWT=[REDACTED]"
assert_not_contains "$WORKER_INSTALL_LOG" "just setup"
assert_not_contains "$WORKER_INSTALL_LOG" "just up"

PACKAGED_WORKER_HOME="$TMP/packaged-worker-home"
mkdir -p "$PACKAGED_WORKER_HOME"
tar -xzf "$RELEASE_ASSETS/g5-self-hosted-bundle.tar.gz" -C "$PACKAGED_WORKER_HOME"
MICROVM_BASE_IMAGE="contract.azurecr.io/g5/task@sha256:$(printf 'a%.0s' {1..64})"
MICROVM_WORKER_LOG="$TMP/microvm-worker.log"
G5_WORKERHOST_DRY_RUN=true \
G5_HOME="$PACKAGED_WORKER_HOME" \
G5_DATA_DIR="$DATA" \
G5_REGISTRY_TOKEN_RESPONSE_FILE="$TMP/token.json" \
G5_REGISTRY_TOKEN_FILE="$TMP/token.json" \
G5_MANIFEST_FILE="$TMP/manifest.json" \
G5D_API_URL=https://g5-internal.contract.test \
G5_HOST_JWT=contract.host.jwt \
G5D_HOST_ID=00000000-0000-0000-0000-000000000002 \
G5D_ORG_ID=00000000-0000-0000-0000-000000000001 \
G5D_ISOLATION_MODE=microvm \
G5D_BASE_IMAGE="$MICROVM_BASE_IMAGE" \
  bash "$PACKAGED_WORKER_HOME/scripts/g5-workerhost.sh" > "$MICROVM_WORKER_LOG"
assert_contains "$MICROVM_WORKER_LOG" "--device=/dev/kvm"
assert_contains "$MICROVM_WORKER_LOG" "Delegate=yes"
assert_contains "$MICROVM_WORKER_LOG" "passt AppArmor"
assert_contains "$MICROVM_WORKER_LOG" "--group-add=\<kvm-gid\>"
assert_contains "$MICROVM_WORKER_LOG" "G5D_ISOLATION_MODE=microvm"
assert_contains "$MICROVM_WORKER_LOG" "G5D_HOST_ID=00000000-0000-0000-0000-000000000002"
assert_contains "$MICROVM_WORKER_LOG" "host"
assert_not_contains "$MICROVM_WORKER_LOG" "G5D_DISPLAY_NAME="
assert_contains "$MICROVM_WORKER_LOG" "G5D_HOST_BASE_IMAGE=$MICROVM_BASE_IMAGE"
assert_contains "$MICROVM_WORKER_LOG" "G5D_ROOTFS_STATE_DIR=/var/lib/g5d/rootfs"
assert_contains "$MICROVM_WORKER_LOG" ":/var/g5d-data"
assert_not_contains "$MICROVM_WORKER_LOG" "--privileged"
assert_contains "$MICROVM_WORKER_LOG" "G5_HOST_JWT=[REDACTED]"
assert_not_contains "$MICROVM_WORKER_LOG" "G5D_HOST_CPUS_PER_VM="
assert_not_contains "$MICROVM_WORKER_LOG" "G5D_HOST_MEM_MIB_PER_VM="
assert_not_contains "$MICROVM_WORKER_LOG" "contract.host.jwt"

MICROVM_AGENT_LOG="$TMP/microvm-packaged-worker.log"
G5_WORKERHOST_DRY_RUN=true \
G5_HOME="$PACKAGED_WORKER_HOME" \
G5_DATA_DIR="$DATA" \
G5_REGISTRY_TOKEN_RESPONSE_FILE="$TMP/token.json" \
G5_REGISTRY_TOKEN_FILE="$TMP/token.json" \
G5_MANIFEST_FILE="$TMP/manifest.json" \
G5D_API_URL=https://g5-internal.contract.test \
G5_HOST_JWT=contract.host.jwt \
G5D_HOST_ID=00000000-0000-0000-0000-000000000003 \
G5D_ORG_ID=00000000-0000-0000-0000-000000000001 \
G5D_ISOLATION_MODE=microvm \
G5D_HOST_BASE_IMAGE="$MICROVM_BASE_IMAGE" \
OPENROUTER_API_KEY=contract-openrouter-key \
  bash "$PACKAGED_WORKER_HOME/scripts/g5-workerhost.sh" > "$MICROVM_AGENT_LOG"
assert_contains "$MICROVM_AGENT_LOG" "stage complete microVM guest runtime"
assert_contains "$MICROVM_AGENT_LOG" "provision and verify XFS reflink store"
assert_contains "$MICROVM_AGENT_LOG" "install g5d-host.service"
assert_contains "$MICROVM_AGENT_LOG" "G5D_ISOLATION_MODE=microvm"
assert_contains "$MICROVM_AGENT_LOG" "G5D_HOST_BASE_IMAGE=$MICROVM_BASE_IMAGE"
assert_contains "$MICROVM_AGENT_LOG" "host"
assert_contains "$MICROVM_AGENT_LOG" "verify microvm service stability"
assert_contains "$MICROVM_AGENT_LOG" "OPENROUTER_API_KEY=[REDACTED]"
assert_not_contains "$MICROVM_AGENT_LOG" "contract-openrouter-key"
assert_not_contains "$MICROVM_AGENT_LOG" "cargo build"
assert_not_contains "$MICROVM_AGENT_LOG" "--privileged"

TAG_ONLY_WORKER_LOG="$TMP/tag-only-worker.log"
if G5_WORKERHOST_DRY_RUN=true \
  G5_HOME="$INSTALL" \
  G5_DATA_DIR="$DATA" \
  G5D_API_URL=https://g5-internal.contract.test \
  G5_HOST_JWT=contract.host.jwt \
  G5D_HOST_ID=00000000-0000-0000-0000-000000000004 \
  G5D_ORG_ID=00000000-0000-0000-0000-000000000001 \
  G5D_ISOLATION_MODE=microvm \
  G5D_BASE_IMAGE=contract.azurecr.io/g5/task:latest \
    bash "$INSTALL/scripts/g5-workerhost.sh" > "$TAG_ONLY_WORKER_LOG" 2>&1; then
  fail "worker installer accepted a tag-only microVM base image"
fi
assert_contains "$TAG_ONLY_WORKER_LOG" "G5D_BASE_IMAGE must be digest-pinned"
assert_not_contains "$TAG_ONLY_WORKER_LOG" "docker pull"

UNACK_PROCESS_LOG="$TMP/process-host.log"
if G5_WORKERHOST_DRY_RUN=true \
  G5_HOME="$INSTALL" \
  G5_DATA_DIR="$DATA" \
  G5D_API_URL=https://g5-internal.contract.test \
  G5_HOST_JWT=contract.host.jwt \
  G5D_HOST_ID=00000000-0000-0000-0000-000000000004 \
  G5D_ORG_ID=00000000-0000-0000-0000-000000000001 \
  G5D_ISOLATION_MODE=process \
    bash "$INSTALL/scripts/g5-workerhost.sh" > "$UNACK_PROCESS_LOG" 2>&1; then
  fail "worker installer accepted process isolation"
fi
assert_contains "$UNACK_PROCESS_LOG" "unsupported G5D_ISOLATION_MODE for external worker host: process"
assert_not_contains "$UNACK_PROCESS_LOG" "docker pull"

DEFAULT_INSTALL_LOG="$TMP/default-install.log"
G5_INSTALL_DRY_RUN=true \
  bash "$RELEASE_ASSETS/install.sh" \
    --license-key g5_contract_license \
    --install-dir "$TMP/default-install-target" \
    --data-dir "$TMP/default-install-data" \
    --external-url https://g5.contract.test \
    --dry-run > "$DEFAULT_INSTALL_LOG"
assert_contains "$DEFAULT_INSTALL_LOG" "https://github.com/G5Labs/Releases/releases/download/v9.9.9/g5-self-hosted-bundle.tar.gz"
assert_contains "$DEFAULT_INSTALL_LOG" '\"version\":\"9.9.9\"'

echo "self-hosted contract test passed"
