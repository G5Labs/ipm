#!/usr/bin/env bash
# Local build-and-probe check for infra/docker/Dockerfile.g5d-pairsoft-preview.
#
# Builds the image exactly as cloudbuild-deploy.yaml's build-g5d-pairsoft-preview
# step does (base tier first, then the one docker build), then boots it as
# UID 1000 and exercises the tool surface PairSoft's gates and the preview
# profiles call: pwsh, powershell, sqlcmd, func, azurite, serve, dotnet, node,
# the two offline npm seeds, the baked Playwright Chromium, and — with
# --privileged for the daemon test only — the single-flight dockerd starter
# through its sudo rule.
#
# Needs a Docker daemon that can build linux/amd64 images (the Node and
# Chrome tarballs are x64-only). Not part of the daemon-free CI contract
# lane; scripts/test_pairsoft_preview_image.py is the static twin that runs
# there.
#
# Usage: infra/docker/test-pairsoft-preview-image.sh [--no-build]
#   G5_PAIRSOFT_IMAGE   image tag to build/probe (default g5d-pairsoft-preview:local)
#   G5_BASE_IMAGE_TAG   base tier tag (default 26.04, as ensure-base-images.sh)
set -euo pipefail
cd "$(dirname "$0")/../.."

IMAGE="${G5_PAIRSOFT_IMAGE:-g5d-pairsoft-preview:local}"
BASE_TAG="${G5_BASE_IMAGE_TAG:-26.04}"
BUILD=1
[[ "${1:-}" == "--no-build" ]] && BUILD=0

if [[ "$BUILD" -eq 1 ]]; then
  bash scripts/ensure-base-images.sh base
  docker build \
    --tag="$IMAGE" \
    --build-arg="BASE_IMAGE=g5/base:${BASE_TAG}" \
    --file=infra/docker/Dockerfile.g5d-pairsoft-preview \
    .
fi

run_as_agent() {
  docker run --rm --user 1000:1000 --entrypoint bash "$IMAGE" -o pipefail -c "$1"
}

echo "== tool surface as uid 1000"
run_as_agent '
  set -e
  test "$(id -u)" = 1000
  dotnet --list-sdks | grep -E "^8[.]0[.]403 "
  node --version && npm --version
  pwsh -NoProfile -NonInteractive -Command "\$PSVersionTable.PSVersion.ToString()"
  powershell -NoProfile -NonInteractive -Command "Get-ChildItem /work | Out-Null; exit 0"
  sqlcmd --version
  func --version && azurite --version && serve --version
  chromium --version && chromedriver --version
  docker --version && docker compose version
  test -L /home/agent/.cache/ms-playwright && ls -d /opt/ms-playwright/chromium-* >/dev/null
  test -w /work/.azurite && test -w /work/.azure-functions && test -w /opt/pairsoft/npm-cache
'

echo "== offline npm installs from both seeds"
for seed in /opt/pairsoft/npm-seed /opt/pairsoft/npm-seed-e2e; do
  run_as_agent "
    set -e
    mkdir -p /tmp/offline && cp ${seed}/package.json /tmp/offline/ && cd /tmp/offline
    npm install --offline --no-audit --no-fund >/tmp/npm.log 2>&1 || { tail -50 /tmp/npm.log; exit 1; }
    ! grep -qi 'http fetch' /tmp/npm.log
    echo \"offline install ok: ${seed}\"
  "
done

echo "== in-guest dockerd through the sudo rule (privileged)"
docker run --rm --privileged --user 1000:1000 --entrypoint bash "$IMAGE" -o pipefail -c '
  set -e
  sudo -n /usr/local/bin/g5-dev-dockerd-start
  docker info --format "{{.ServerVersion}} {{.Driver}}"
  docker run --rm hello-world >/dev/null && echo "container run ok"
'

echo "== layer ancestry"
G5_BASE_IMAGE_REF="g5/base:${BASE_TAG}" bash scripts/verify-base-digest-identity.sh "$IMAGE"
echo "PASS: $IMAGE"
