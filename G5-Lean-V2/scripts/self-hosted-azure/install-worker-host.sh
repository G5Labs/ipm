#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${G5_INSTALL_DIR:-/opt/g5}"
DATA_DIR="${G5_DATA_DIR:-/var/lib/g5}"
GITHUB_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-G5Labs/Releases}"
INSTALL_RELEASE="${G5_INSTALL_RELEASE:-latest}"
TARGET_VERSION="${G5_TARGET_VERSION:-latest}"
API_URL="${G5_API_URL:?G5_API_URL is required}"
LICENSE_KEY="${G5_LICENSE_KEY:?G5_LICENSE_KEY is required}"
G5D_API_URL="${G5D_API_URL:?G5D_API_URL is required}"
G5_HOST_JWT="${G5_HOST_JWT:?G5_HOST_JWT is required}"
G5D_HOST_ID="${G5D_HOST_ID:?G5D_HOST_ID is required}"
G5D_ORG_ID="${G5D_ORG_ID:?G5D_ORG_ID is required}"
BUNDLE_DIR="${G5_BUNDLE_DIR:?G5_BUNDLE_DIR is required}"
INSTALLER_PATH="${G5_INSTALLER_PATH:?G5_INSTALLER_PATH is required}"
APT_INSTALL_HELPER="${G5_APT_INSTALL_HELPER:-/usr/local/bin/g5-apt-install-with-retry}"

[ -d "$BUNDLE_DIR" ] || { echo "G5_BUNDLE_DIR does not exist: $BUNDLE_DIR" >&2; exit 1; }
[ -r "$INSTALLER_PATH" ] || { echo "G5_INSTALLER_PATH is not readable: $INSTALLER_PATH" >&2; exit 1; }

export DEBIAN_FRONTEND=noninteractive
if [ -x "$APT_INSTALL_HELPER" ]; then
  "$APT_INSTALL_HELPER" util-linux xfsprogs apparmor
else
  apt-get \
    -o Acquire::Retries=3 \
    -o Acquire::http::Timeout=30 \
    -o Acquire::https::Timeout=30 \
    update -qq
  apt-get \
    -o Acquire::Retries=3 \
    -o Acquire::http::Timeout=30 \
    -o Acquire::https::Timeout=30 \
    install -y -qq --no-install-recommends util-linux xfsprogs apparmor
fi

bash "$INSTALLER_PATH" \
  --worker-host \
  --license-key "$LICENSE_KEY" \
  --api-url "$API_URL" \
  --install-dir "$INSTALL_DIR" \
  --data-dir "$DATA_DIR" \
  --external-url "$G5D_API_URL" \
  --github-release-repo "$GITHUB_RELEASE_REPO" \
  --release "$INSTALL_RELEASE" \
  --version "$TARGET_VERSION" \
  --host-jwt "$G5_HOST_JWT" \
  --host-id "$G5D_HOST_ID" \
  --host-org-id "$G5D_ORG_ID" \
  --bundle-dir "$BUNDLE_DIR"
