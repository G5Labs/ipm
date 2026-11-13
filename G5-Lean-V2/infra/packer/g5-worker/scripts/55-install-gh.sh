#!/usr/bin/env bash
# =============================================================================
# 55-install-gh.sh — Install GitHub CLI into the mounted in-VM runtime bin.
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
    echo "[g5-worker] SKIP_RUNTIME_CLI_INSTALL=1; skipping GitHub CLI install (template validation mode)"
    exit 0
fi

RUNTIME_BIN=/var/g5d-data/runtime/bin
install -d -m 0755 "$RUNTIME_BIN"

GH_CLI_VERSION="${GH_CLI_VERSION:-2.95.0}"
GH_CLI_VERSION="${GH_CLI_VERSION#v}"
case "$(uname -m)" in
    x86_64|amd64) GH_ARCH=amd64 ;;
    aarch64|arm64) GH_ARCH=arm64 ;;
    *)
        echo "[g5-worker] FATAL: unsupported GitHub CLI architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

GH_TARBALL="gh_${GH_CLI_VERSION}_linux_${GH_ARCH}.tar.gz"
GH_URL="https://github.com/cli/cli/releases/download/v${GH_CLI_VERSION}/${GH_TARBALL}"
GH_DL_DIR="$(mktemp -d)"
trap 'rm -rf "$GH_DL_DIR"' EXIT

echo "[g5-worker] downloading GitHub CLI v${GH_CLI_VERSION} from $GH_URL"
download_with_retry "gh ${GH_CLI_VERSION}" "$GH_URL" "$GH_DL_DIR/$GH_TARBALL"

ACTUAL_SHA="$(sha256sum "$GH_DL_DIR/$GH_TARBALL" | awk '{print $1}')"
if [ -n "${GH_CLI_SHA256:-}" ]; then
    if [ "$ACTUAL_SHA" != "$GH_CLI_SHA256" ]; then
        echo "[g5-worker] FATAL: GitHub CLI tarball sha256 mismatch. expected=$GH_CLI_SHA256 actual=$ACTUAL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] GitHub CLI sha256 OK (pinned): $ACTUAL_SHA"
else
    download_with_retry "gh checksums" \
        "https://github.com/cli/cli/releases/download/v${GH_CLI_VERSION}/gh_${GH_CLI_VERSION}_checksums.txt" \
        "$GH_DL_DIR/checksums.txt"
    EXPECTED_SHA="$(awk -v f="$GH_TARBALL" '$2 == f {print $1}' "$GH_DL_DIR/checksums.txt")"
    [ -n "$EXPECTED_SHA" ] || {
        echo "[g5-worker] FATAL: $GH_TARBALL not listed in GitHub CLI checksums" >&2
        exit 1
    }
    if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
        echo "[g5-worker] FATAL: GitHub CLI sha256 mismatch vs release checksums. expected=$EXPECTED_SHA actual=$ACTUAL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] GitHub CLI sha256 OK: $ACTUAL_SHA"
    echo "[g5-worker] HINT: pin -var gh_cli_sha256=$ACTUAL_SHA for a reproducible supply-chain lock"
fi

tar -xzf "$GH_DL_DIR/$GH_TARBALL" -C "$GH_DL_DIR"
GH_EXTRACT="$GH_DL_DIR/gh_${GH_CLI_VERSION}_linux_${GH_ARCH}"
[ -x "$GH_EXTRACT/bin/gh" ] || {
    echo "[g5-worker] FATAL: extracted GitHub CLI tarball missing bin/gh" >&2
    exit 1
}

install -m 0755 "$GH_EXTRACT/bin/gh" "$RUNTIME_BIN/gh"
if ! "$RUNTIME_BIN/gh" --version >/tmp/gh-version.out 2>&1; then
    echo "[g5-worker] FATAL: gh --version failed; the release produced an unrunnable binary" >&2
    cat /tmp/gh-version.out >&2 || true
    exit 1
fi
awk 'NR == 1 {print $3}' /tmp/gh-version.out > /tmp/gh.version
echo "[g5-worker] gh --version: $(head -1 /tmp/gh-version.out)"
echo "[g5-worker] 55-install-gh.sh OK (gh=$(cat /tmp/gh.version))"
