#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="/opt/g5"
DATA_DIR="/var/lib/g5"
API_URL="${G5_API_URL:-https://api.g5labs.ai}"
LICENSE_KEY="${G5_LICENSE_KEY:-}"
BUNDLE_URL="${G5_BUNDLE_URL:-}"
BUNDLE_SHA256="${G5_BUNDLE_SHA256:-}"
BUNDLE_DIR="${G5_BUNDLE_DIR:-}"
TOKEN_RESPONSE_FILE="${G5_REGISTRY_TOKEN_RESPONSE_FILE:-}"
DRY_RUN="${G5_INSTALL_DRY_RUN:-false}"
EXTERNAL_URL="${G5_EXTERNAL_URL:-}"
G5_GITHUB_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-G5Labs/Releases}"
G5_INSTALL_RELEASE="${G5_INSTALL_RELEASE:-latest}"
G5_TARGET_VERSION="${G5_TARGET_VERSION:-}"
WORKER_HOST="${G5_INSTALL_WORKER_HOST:-false}"
HOST_JWT="${G5_HOST_JWT:-}"
HOST_ID="${G5D_HOST_ID:-}"
HOST_ORG_ID="${G5D_ORG_ID:-}"

default_bundle_url() {
  if [ "$G5_INSTALL_RELEASE" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/g5-self-hosted-bundle.tar.gz' "$G5_GITHUB_RELEASE_REPO"
  else
    printf 'https://github.com/%s/releases/download/%s/g5-self-hosted-bundle.tar.gz' "$G5_GITHUB_RELEASE_REPO" "$G5_INSTALL_RELEASE"
  fi
}

requested_runtime_version() {
  if [ -n "$G5_TARGET_VERSION" ]; then
    printf '%s' "$G5_TARGET_VERSION"
  elif [ "$G5_INSTALL_RELEASE" != "latest" ]; then
    printf '%s' "${G5_INSTALL_RELEASE#v}"
  else
    printf 'latest'
  fi
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Options:
  --license-key KEY       G5 license key
  --api-url URL           G5 license API URL
  --install-dir DIR       install directory (default /opt/g5)
  --data-dir DIR          mutable data directory (default /var/lib/g5)
  --external-url URL      canonical customer HTTPS URL
  --release TAG           GitHub Release tag for the default bundle URL
  --version VERSION       entitled G5 runtime version to install
  --worker-host           install this machine as an external worker host
  --host-jwt JWT          bound host credential returned by host provisioning
  --host-id ID            provisioned worker_hosts identity
  --host-org-id ID        organization bound into the host credential
  --github-release-repo R GitHub repo for public installer assets (OWNER/REPO)
  --bundle-url URL        self-hosted bundle tarball URL
  --bundle-sha256 HASH    expected bundle sha256
  --bundle-dir DIR        local bundle directory for tests/offline install
  --token-response-file F registry-token response JSON for tests/offline install
  --dry-run               print commands without executing remote/system changes
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --license-key) LICENSE_KEY="$2"; shift 2 ;;
    --api-url) API_URL="$2"; shift 2 ;;
    --install-dir) INSTALL_DIR="$2"; shift 2 ;;
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --external-url) EXTERNAL_URL="$2"; shift 2 ;;
    --release) G5_INSTALL_RELEASE="$2"; shift 2 ;;
    --version) G5_TARGET_VERSION="$2"; shift 2 ;;
    --worker-host) WORKER_HOST=true; shift ;;
    --host-jwt) HOST_JWT="$2"; shift 2 ;;
    --host-id) HOST_ID="$2"; shift 2 ;;
    --host-org-id) HOST_ORG_ID="$2"; shift 2 ;;
    --github-release-repo) G5_GITHUB_RELEASE_REPO="$2"; shift 2 ;;
    --bundle-url) BUNDLE_URL="$2"; shift 2 ;;
    --bundle-sha256) BUNDLE_SHA256="$2"; shift 2 ;;
    --bundle-dir) BUNDLE_DIR="$2"; shift 2 ;;
    --token-response-file) TOKEN_RESPONSE_FILE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

die() {
  echo "error: $*" >&2
  exit 1
}

is_true() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

run() {
  if is_true "$DRY_RUN"; then
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

require_cmd() {
  if is_true "$DRY_RUN"; then
    return 0
  fi
  command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found"
}

[ -n "$LICENSE_KEY" ] || [ -n "$TOKEN_RESPONSE_FILE" ] || die "missing --license-key"
[ -n "$EXTERNAL_URL" ] || die "missing --external-url"
if is_true "$WORKER_HOST"; then
  [ -n "$HOST_JWT" ] || die "missing --host-jwt"
  [ -n "$HOST_ID" ] || die "missing --host-id"
  [ -n "$HOST_ORG_ID" ] || die "missing --host-org-id"
fi
if [ -z "$BUNDLE_DIR" ] && [ -z "$BUNDLE_URL" ]; then
  BUNDLE_URL="$(default_bundle_url)"
fi
[ -n "$BUNDLE_DIR" ] || [ -n "$BUNDLE_URL" ] || die "missing --bundle-url or --bundle-dir"

require_cmd docker
require_cmd python3
if ! is_true "$WORKER_HOST"; then
  require_cmd just
  require_cmd git
fi
if ! is_true "$DRY_RUN" && ! is_true "$WORKER_HOST"; then
  docker compose version >/dev/null
fi

WORK_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

TOKEN_RESPONSE="$WORK_DIR/registry-token.json"
if [ -n "$TOKEN_RESPONSE_FILE" ]; then
  run cp "$TOKEN_RESPONSE_FILE" "$TOKEN_RESPONSE"
else
  REQUESTED_VERSION="$(requested_runtime_version)"
  BODY="$(G5_REQUESTED_VERSION="$REQUESTED_VERSION" python3 - <<'PY'
import json
import os
print(json.dumps({
    "product": "g5-self-hosted",
    "version": os.environ["G5_REQUESTED_VERSION"],
    "platform": "linux/amd64",
}, separators=(",", ":")))
PY
)"
  if is_true "$DRY_RUN"; then
    printf '+ curl -fsSL -X POST %q -H %q -H %q -d %q -o %q\n' \
      "${API_URL%/}/api/v1/registry/token" \
      "Authorization: Bearer [REDACTED]" \
      "Content-Type: application/json" \
      "$BODY" \
      "$TOKEN_RESPONSE"
  else
    curl -fsSL \
      -X POST "${API_URL%/}/api/v1/registry/token" \
      -H "Authorization: Bearer $LICENSE_KEY" \
      -H "Content-Type: application/json" \
      -d "$BODY" \
      -o "$TOKEN_RESPONSE"
  fi
fi

if ! is_true "$DRY_RUN"; then
  REGISTRY="$(python3 - "$TOKEN_RESPONSE" <<'PY'
import json, sys
print(json.load(open(sys.argv[1])).get("registry", ""))
PY
)"
  PASSWORD="$(python3 - "$TOKEN_RESPONSE" <<'PY'
import json, sys
print(json.load(open(sys.argv[1])).get("password", ""))
PY
)"
  [ -n "$REGISTRY" ] || die "token response missing registry"
  [ -n "$PASSWORD" ] || die "token response missing password"
  if is_true "${G5_INSTALL_SKIP_DOCKER_LOGIN:-false}"; then
    echo "Skipping docker registry auth because G5_INSTALL_SKIP_DOCKER_LOGIN=true"
  else
    echo "Received registry credentials for $REGISTRY."
  fi
else
  echo "+ write docker auth config for [registry-from-token]"
fi

BUNDLE_STAGE="$WORK_DIR/bundle"
if [ -n "$BUNDLE_DIR" ]; then
  run mkdir -p "$BUNDLE_STAGE"
  run cp -R "$BUNDLE_DIR/." "$BUNDLE_STAGE/"
else
  ARCHIVE="$WORK_DIR/self-hosted-bundle.tar.gz"
  run curl -fsSL "$BUNDLE_URL" -o "$ARCHIVE"
  if [ -n "$BUNDLE_SHA256" ] && ! is_true "$DRY_RUN"; then
    echo "$BUNDLE_SHA256  $ARCHIVE" | sha256sum -c -
  fi
  run mkdir -p "$BUNDLE_STAGE"
  run tar -xzf "$ARCHIVE" -C "$BUNDLE_STAGE"
fi

run mkdir -p "$INSTALL_DIR" "$DATA_DIR"
run cp -R "$BUNDLE_STAGE/." "$INSTALL_DIR/"

if is_true "$WORKER_HOST"; then
  if ! is_true "$DRY_RUN"; then
    python3 "$INSTALL_DIR/scripts/token-response-to-manifest.py" "$TOKEN_RESPONSE" "$INSTALL_DIR/install_manifest.json"
    install -m 0600 "$TOKEN_RESPONSE" "$INSTALL_DIR/.registry-token.json"
  else
    echo "+ render worker release manifest from registry token"
  fi
  run chmod 0755 "$INSTALL_DIR/scripts/g5-workerhost.sh" "$INSTALL_DIR/scripts/token-response-to-manifest.py"
  if is_true "$DRY_RUN"; then
    printf '+ G5_HOME=%q G5_DATA_DIR=%q G5_REGISTRY_TOKEN_RESPONSE_FILE=%q G5D_API_URL=%q G5_HOST_JWT=[REDACTED] G5D_HOST_ID=%q G5D_ORG_ID=%q bash %q\n' \
      "$INSTALL_DIR" \
      "$DATA_DIR" \
      "$TOKEN_RESPONSE" \
      "$EXTERNAL_URL" \
      "$HOST_ID" \
      "$HOST_ORG_ID" \
      "$INSTALL_DIR/scripts/g5-workerhost.sh"
  else
    G5_HOME="$INSTALL_DIR" \
    G5_DATA_DIR="$DATA_DIR" \
    G5_REGISTRY_TOKEN_RESPONSE_FILE="$TOKEN_RESPONSE" \
    G5D_API_URL="$EXTERNAL_URL" \
    G5_HOST_JWT="$HOST_JWT" \
    G5D_HOST_ID="$HOST_ID" \
    G5D_ORG_ID="$HOST_ORG_ID" \
      bash "$INSTALL_DIR/scripts/g5-workerhost.sh"
  fi
  echo "G5 external worker install complete: $INSTALL_DIR"
  exit 0
fi

if [ ! -f "$INSTALL_DIR/.env" ]; then
  run cp "$INSTALL_DIR/env.template" "$INSTALL_DIR/.env"
fi

if ! is_true "$DRY_RUN"; then
  python3 "$INSTALL_DIR/scripts/token-response-to-manifest.py" "$TOKEN_RESPONSE" "$INSTALL_DIR/install_manifest.json"
  python3 "$INSTALL_DIR/scripts/render-selfhost-env.py" "$INSTALL_DIR/.env" "$INSTALL_DIR/install_manifest.json" "$INSTALL_DIR/.env.next"
  mv "$INSTALL_DIR/.env.next" "$INSTALL_DIR/.env"
  python3 "$INSTALL_DIR/scripts/apply-install-env.py" \
    "$INSTALL_DIR/.env" \
    "$LICENSE_KEY" \
    "$API_URL" \
    "$INSTALL_DIR" \
    "$DATA_DIR" \
    "$EXTERNAL_URL"
  chmod 0600 "$INSTALL_DIR/.env"
else
  echo "+ render .env with token manifest, license key, API URL, data dir, and external URL"
fi

run chmod 0755 "$INSTALL_DIR/scripts/g5-selfhost.sh" "$INSTALL_DIR/scripts/render-selfhost-env.py" "$INSTALL_DIR/scripts/token-response-to-manifest.py" "$INSTALL_DIR/scripts/apply-install-env.py"
run chmod 0600 "$INSTALL_DIR/.env"

if is_true "$DRY_RUN"; then
  printf '+ cd %q\n' "$INSTALL_DIR"
  run env G5_HOME="$INSTALL_DIR" G5_DATA_DIR="$DATA_DIR" G5_REGISTRY_TOKEN_RESPONSE_FILE="$TOKEN_RESPONSE" G5_SELFHOST_DRY_RUN="$DRY_RUN" just setup
  run env G5_HOME="$INSTALL_DIR" G5_DATA_DIR="$DATA_DIR" G5_SELFHOST_DRY_RUN="$DRY_RUN" just up
else
  (
    cd "$INSTALL_DIR"
    run env G5_HOME="$INSTALL_DIR" G5_DATA_DIR="$DATA_DIR" G5_REGISTRY_TOKEN_RESPONSE_FILE="$TOKEN_RESPONSE" G5_SELFHOST_DRY_RUN="$DRY_RUN" just setup
    run env G5_HOME="$INSTALL_DIR" G5_DATA_DIR="$DATA_DIR" G5_SELFHOST_DRY_RUN="$DRY_RUN" just up
  )
fi

echo "G5 self-hosted install complete: $INSTALL_DIR"
