#!/usr/bin/env bash
# =============================================================================
# 56-install-gcloud-kubectl.sh — Install Google Cloud + Kubernetes CLIs into
# the mounted in-VM runtime bin.
#
# Claude Code and Codex execute bash inside the workspace VM. The host's
# /var/g5d-data/runtime/bin is mounted read-only at /opt/g5/bin and prepended to
# PATH, so tools the agents need must be staged here, not only apt-installed on
# the workspace host.
# =============================================================================
set -euo pipefail

# Shared bounded-retry download helper, uploaded by the packer `file`
# provisioner ahead of Phase 1 (G5Labs/G5#1811).
G5_DOWNLOAD_LIB="${G5_DOWNLOAD_LIB:-/tmp/g5-download-with-retry.sh}"
# Fail closed with a diagnosable message. Consumers must each provide this file:
# the packer build uploads it with a `file` provisioner ahead of Phase 1, and any
# container consumer must mount it and point G5_DOWNLOAD_LIB at the mount.
[ -r "$G5_DOWNLOAD_LIB" ] || {
    echo "[g5-worker] FATAL: shared download helper not readable at ${G5_DOWNLOAD_LIB}." >&2
    echo "[g5-worker] Provide it (packer: file provisioner; container: bind-mount + G5_DOWNLOAD_LIB)." >&2
    exit 1
}
# shellcheck source=lib/download-with-retry.sh
. "$G5_DOWNLOAD_LIB"


if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" = "1" ]; then
    echo "[g5-worker] SKIP_RUNTIME_CLI_INSTALL=1; skipping gcloud/kubectl install (template validation mode)"
    exit 0
fi

RUNTIME_BIN=/var/g5d-data/runtime/bin
install -d -m 0755 "$RUNTIME_BIN"

GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION:-575.0.0}"
GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION#v}"
KUBECTL_VERSION="${KUBECTL_VERSION:-1.36.2}"
KUBECTL_VERSION="v${KUBECTL_VERSION#v}"
case "$(uname -m)" in
    x86_64|amd64)
        GCLOUD_ARCH=x86_64
        KUBECTL_ARCH=amd64
        ;;
    aarch64|arm64)
        GCLOUD_ARCH=arm
        KUBECTL_ARCH=arm64
        ;;
    *)
        echo "[g5-worker] FATAL: unsupported gcloud/kubectl architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

DL_DIR="$(mktemp -d)"
trap 'rm -rf "$DL_DIR"' EXIT

GCLOUD_TARBALL="google-cloud-cli-${GCLOUD_CLI_VERSION}-linux-${GCLOUD_ARCH}.tar.gz"
GCLOUD_URL="https://storage.googleapis.com/cloud-sdk-release/${GCLOUD_TARBALL}"
echo "[g5-worker] downloading Google Cloud CLI ${GCLOUD_CLI_VERSION} from $GCLOUD_URL"
download_with_retry "gcloud ${GCLOUD_CLI_VERSION}" "$GCLOUD_URL" "$DL_DIR/$GCLOUD_TARBALL"

ACTUAL_GCLOUD_SHA="$(sha256sum "$DL_DIR/$GCLOUD_TARBALL" | awk '{print $1}')"
if [ -n "${GCLOUD_CLI_SHA256:-}" ]; then
    if [ "$ACTUAL_GCLOUD_SHA" != "$GCLOUD_CLI_SHA256" ]; then
        echo "[g5-worker] FATAL: gcloud tarball sha256 mismatch. expected=$GCLOUD_CLI_SHA256 actual=$ACTUAL_GCLOUD_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] gcloud sha256 OK (pinned): $ACTUAL_GCLOUD_SHA"
else
    echo "[g5-worker] gcloud sha256: $ACTUAL_GCLOUD_SHA (HINT: pin -var gcloud_cli_sha256=$ACTUAL_GCLOUD_SHA for a reproducible supply-chain lock)"
fi

tar -xzf "$DL_DIR/$GCLOUD_TARBALL" -C "$DL_DIR"
[ -x "$DL_DIR/google-cloud-sdk/bin/gcloud" ] || {
    echo "[g5-worker] FATAL: extracted Google Cloud CLI archive missing bin/gcloud" >&2
    exit 1
}
rm -rf "$RUNTIME_BIN/google-cloud-sdk"
cp -a "$DL_DIR/google-cloud-sdk" "$RUNTIME_BIN/google-cloud-sdk"
chmod -R a+rX "$RUNTIME_BIN/google-cloud-sdk"
cat > "$RUNTIME_BIN/gcloud" <<'EOF'
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK="${CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK:-1}"
export CLOUDSDK_PYTHON="$HERE/python3"
exec "$HERE/google-cloud-sdk/bin/gcloud" "$@"
EOF
chmod 0755 "$RUNTIME_BIN/gcloud"
CLOUDSDK_CORE_DISABLE_PROMPTS=1 CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK=1 \
    "$RUNTIME_BIN/gcloud" components install gke-gcloud-auth-plugin --quiet
chmod -R a+rX "$RUNTIME_BIN/google-cloud-sdk"
[ -x "$RUNTIME_BIN/google-cloud-sdk/bin/gke-gcloud-auth-plugin" ] || {
    echo "[g5-worker] FATAL: gcloud component install did not produce bin/gke-gcloud-auth-plugin" >&2
    exit 1
}
cat > "$RUNTIME_BIN/gke-gcloud-auth-plugin" <<'EOF'
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export CLOUDSDK_PYTHON="$HERE/python3"
exec "$HERE/google-cloud-sdk/bin/gke-gcloud-auth-plugin" "$@"
EOF
chmod 0755 "$RUNTIME_BIN/gke-gcloud-auth-plugin"
printf '%s\n' "$ACTUAL_GCLOUD_SHA" > "$RUNTIME_BIN/gcloud.sha256"
if ! "$RUNTIME_BIN/gcloud" --version >"$DL_DIR/gcloud-version.out" 2>&1; then
    echo "[g5-worker] FATAL: gcloud --version failed; the archive produced an unrunnable CLI" >&2
    cat "$DL_DIR/gcloud-version.out" >&2 || true
    exit 1
fi
awk '/^Google Cloud SDK / {print $4; exit}' "$DL_DIR/gcloud-version.out" > /tmp/gcloud.version
echo "[g5-worker] gcloud --version: $(head -1 "$DL_DIR/gcloud-version.out")"
if ! "$RUNTIME_BIN/gke-gcloud-auth-plugin" --version >"$DL_DIR/gke-gcloud-auth-plugin-version.out" 2>&1; then
    echo "[g5-worker] FATAL: gke-gcloud-auth-plugin --version failed; kubectl GKE auth would fail in-VM" >&2
    cat "$DL_DIR/gke-gcloud-auth-plugin-version.out" >&2 || true
    exit 1
fi
awk 'NR == 1 {print; exit}' "$DL_DIR/gke-gcloud-auth-plugin-version.out" > /tmp/gke-gcloud-auth-plugin.version
echo "[g5-worker] gke-gcloud-auth-plugin --version: $(head -1 "$DL_DIR/gke-gcloud-auth-plugin-version.out")"

KUBECTL_URL="https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${KUBECTL_ARCH}/kubectl"
echo "[g5-worker] downloading kubectl ${KUBECTL_VERSION} from $KUBECTL_URL"
download_with_retry "kubectl ${KUBECTL_VERSION}" "$KUBECTL_URL" "$DL_DIR/kubectl"
ACTUAL_KUBECTL_SHA="$(sha256sum "$DL_DIR/kubectl" | awk '{print $1}')"
if [ -n "${KUBECTL_SHA256:-}" ]; then
    if [ "$ACTUAL_KUBECTL_SHA" != "$KUBECTL_SHA256" ]; then
        echo "[g5-worker] FATAL: kubectl sha256 mismatch. expected=$KUBECTL_SHA256 actual=$ACTUAL_KUBECTL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] kubectl sha256 OK (pinned): $ACTUAL_KUBECTL_SHA"
else
    download_with_retry "kubectl checksum" "${KUBECTL_URL}.sha256" "$DL_DIR/kubectl.sha256"
    EXPECTED_KUBECTL_SHA="$(awk '{print $1}' "$DL_DIR/kubectl.sha256")"
    [ -n "$EXPECTED_KUBECTL_SHA" ] || {
        echo "[g5-worker] FATAL: kubectl checksum file was empty" >&2
        exit 1
    }
    if [ "$ACTUAL_KUBECTL_SHA" != "$EXPECTED_KUBECTL_SHA" ]; then
        echo "[g5-worker] FATAL: kubectl sha256 mismatch vs release checksum. expected=$EXPECTED_KUBECTL_SHA actual=$ACTUAL_KUBECTL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] kubectl sha256 OK: $ACTUAL_KUBECTL_SHA"
    echo "[g5-worker] HINT: pin -var kubectl_sha256=$ACTUAL_KUBECTL_SHA for a reproducible supply-chain lock"
fi
install -m 0755 "$DL_DIR/kubectl" "$RUNTIME_BIN/kubectl"
printf '%s\n' "$ACTUAL_KUBECTL_SHA" > "$RUNTIME_BIN/kubectl.sha256"
if ! "$RUNTIME_BIN/kubectl" version --client=true >"$DL_DIR/kubectl-version.out" 2>&1; then
    echo "[g5-worker] FATAL: kubectl version --client=true failed; the release produced an unrunnable binary" >&2
    cat "$DL_DIR/kubectl-version.out" >&2 || true
    exit 1
fi
awk 'match($0, /v[0-9]+\.[0-9]+\.[0-9]+/) {print substr($0, RSTART + 1, RLENGTH - 1); exit}' "$DL_DIR/kubectl-version.out" > /tmp/kubectl.version
echo "[g5-worker] kubectl version: $(head -1 "$DL_DIR/kubectl-version.out")"
echo "[g5-worker] 56-install-gcloud-kubectl.sh OK (gcloud=$(cat /tmp/gcloud.version), kubectl=$(cat /tmp/kubectl.version))"
