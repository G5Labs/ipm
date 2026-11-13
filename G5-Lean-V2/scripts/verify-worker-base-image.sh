#!/usr/bin/env bash
# Verify the default workspace VM base image has the guest OS tools that
# mount-injected /opt/g5/bin CLIs do not provide.
set -euo pipefail

# Default mirrors DEFAULT_BASE_IMAGE (g5-api-core cloud/startup_script.rs):
# the guest must sit at or above the glibc 2.43 floor (microvm [D12]).
IMAGE="${1:-${G5D_HOST_BASE_IMAGE:-buildpack-deps:resolute}}"
PLATFORM="${G5D_HOST_BASE_IMAGE_PLATFORM:-linux/amd64}"

main() {
    echo "[g5-worker-base] verifying ${IMAGE} (${PLATFORM})"

    docker run --rm --platform "${PLATFORM}" "${IMAGE}" sh -lc '
set -eu

for tool in git cc gcc g++ ld make pkg-config; do
    command -v "$tool" >/dev/null || {
        echo "missing required guest tool: $tool" >&2
        exit 1
    }
done

for pkg in \
    build-essential \
    gcc \
    g++ \
    binutils \
    make \
    libssl-dev \
    zlib1g-dev \
    libffi-dev \
    libsqlite3-dev \
    libzstd-dev; do
    dpkg-query -W "$pkg" >/dev/null 2>&1 || {
        echo "missing required guest package: $pkg" >&2
        exit 1
    }
done

tmp="$(mktemp -d)"
trap "rm -rf \"$tmp\"" EXIT
cat > "$tmp/link-smoke.c" <<EOF
int main(void) { return 0; }
EOF
cc "$tmp/link-smoke.c" -o "$tmp/link-smoke"
"$tmp/link-smoke"
'

    echo "[g5-worker-base] OK: ${IMAGE} has git plus native linker/build deps"
}

main "$@"
