#!/usr/bin/env bash
# =============================================================================
# 10-apt-deps.sh — Install all apt packages needed at build time + runtime.
#
# Two classes of packages:
#   1. RUNTIME — must remain on the final image. KVM modules, libcap-ng0,
#      xfsprogs (xfs_info, reflink probe), curl, nodejs (so the
#      mount-injected Claude Code + codex CLIs can run inside the guest
#      microVM that bind-mounts /opt/g5/bin/).
#   2. BUILD — needed to compile platform kernel + g5d but discarded in 90-cleanup.sh.
#      build-essential, pkg-config, libssl-dev, cmake, gcc, git, patchelf,
#      python3-pyelftools, libcap-ng-dev, libclang-dev, clang, lld.
#
# We bake the runtime set into the final image and rely on apt's marking to
# autoremove build-only packages at cleanup time. The package list mirrors
# infra/docker/Dockerfile.g5d-base / Dockerfile.g5d so the image
# semantics stay aligned with the docker-mode worker.
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


# 1.97.1 tracks the workspace toolchain used by the base-rust and g5d-base tiers.
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"
RUSTUP_VERSION="${RUSTUP_VERSION:-1.29.0}"
RUSTUP_VERSION="${RUSTUP_VERSION#v}"

case "$(uname -m)" in
    x86_64|amd64)
        RUST_TARGET=x86_64-unknown-linux-gnu
        ;;
    aarch64|arm64)
        RUST_TARGET=aarch64-unknown-linux-gnu
        ;;
    *)
        echo "[g5-worker] FATAL: unsupported Rust architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

verify_sha256() {
    local file="$1"
    local expected="$2"
    local label="$3"
    local actual
    actual="$(sha256sum "$file" | awk '{print $1}')"
    if [ "$actual" != "$expected" ]; then
        echo "[g5-worker] FATAL: $label sha256 mismatch. expected=$expected actual=$actual" >&2
        exit 1
    fi
    echo "[g5-worker] $label sha256 OK: $actual"
}

echo "[g5-worker] apt-get update + base upgrade"
apt-get update -qq
apt-get -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" \
    dist-upgrade -qq

# Runtime — must stay on the image.
RUNTIME_PKGS=(
    libcap-ng0
    xfsprogs
    # ext4 mkfs for the per-VM scratch disk (microvm_runtime D53):
    # `g5d vm-exec` formats the sparse backing file host-side with mkfs.ext4
    # and FAILS the boot closed when it is missing. Ubuntu ships e2fsprogs by
    # default; listing it makes the retention explicit so a future base-image
    # slimming cannot silently drop it.
    e2fsprogs
    busybox-static
    git
    rsync
    qemu-utils
    curl
    ca-certificates
    jq
    nodejs
    npm
    systemd-resolved
    # GCE guest agent (Ubuntu universe). REQUIRED for the image to work as a
    # real GCE instance: google-guest-agent runs the instance startup-script
    # (how `g5d host` gets its /etc/g5/host.env bootstrapped) and manages
    # metadata-based SSH keys. Without it, on a from-ISO image the startup-script
    # never runs (the A2.5 NoCloud-seed gate never exercised the GCE datasource,
    # so this gap only surfaced on the first standalone-instance boot).
    #
    # NOTE: google-guest-configs is NOT in Ubuntu's default repos (only Google's
    # apt repo). Its job for us is the `metadata.google.internal` -> 169.254.169.254
    # /etc/hosts entry, which 80-configure-host.sh writes directly — so we don't
    # pull it (and the bootstrap hits the metadata server by IP regardless).
    google-guest-agent
)

# Build — used by platform kernel + g5d compile, autoremove-marked.
BUILD_PKGS=(
    build-essential
    bc
    bison
    flex
    libelf-dev
    cpio
    pkg-config
    libssl-dev
    cmake
    gcc
    g++
    make
    git
    patchelf
    python3
    python3-pyelftools
    libcap-ng-dev
    libclang-dev
    clang
    lld
    binutils
    file
    tar
)

echo "[g5-worker] installing runtime packages"
apt-get install -y --no-install-recommends "${RUNTIME_PKGS[@]}"

# Trust-store floor (2026-10-22, eMudhra/emSign intermediate-CA key compromise).
# The bundle baked here is frozen for the image's whole life (periodic apt is
# masked by 80-configure-host.sh) and 50-install-codex.sh copies it into every
# workspace guest, so this build is the only point where the distrusted
# `emSign ECC Root CA - G3` can be removed (USN-8944-1, 2026-10-20). Fail the
# build rather than ship a stale trust store to the fleet.
CA_CERTIFICATES_MIN="${CA_CERTIFICATES_MIN:-20260816ubuntu0.24.04.1}"
ca_have="$(dpkg-query -W -f='${Version}' ca-certificates)"
if ! dpkg --compare-versions "$ca_have" ge "$CA_CERTIFICATES_MIN"; then
    echo "[g5-worker] FATAL: ca-certificates $ca_have is older than $CA_CERTIFICATES_MIN (USN-8944-1)" >&2
    exit 1
fi
if [ -n "$(find /etc/ssl/certs -maxdepth 1 -iname '*emSign_ECC_Root_CA_-_G3*' -print -quit)" ]; then
    echo "[g5-worker] FATAL: distrusted emSign ECC Root CA - G3 is still installed in /etc/ssl/certs" >&2
    exit 1
fi
# USN-8945-1 (XZ Utils, 2026-10-20): liblzma5 is on every host and in every guest.
LIBLZMA5_MIN="${LIBLZMA5_MIN:-5.6.1+really5.4.5-1ubuntu0.4}"
lzma_have="$(dpkg-query -W -f='${Version}' liblzma5)"
if ! dpkg --compare-versions "$lzma_have" ge "$LIBLZMA5_MIN"; then
    echo "[g5-worker] FATAL: liblzma5 $lzma_have is older than $LIBLZMA5_MIN (USN-8945-1)" >&2
    exit 1
fi
echo "[g5-worker] ca-certificates $ca_have OK (>= $CA_CERTIFICATES_MIN; emSign ECC Root CA - G3 absent); liblzma5 $lzma_have OK (>= $LIBLZMA5_MIN)"

echo "[g5-worker] installing build packages (marked auto for later autoremove)"
apt-get install -y --no-install-recommends --mark-auto "${BUILD_PKGS[@]}"

# Rust toolchain — needed to build platform kernel + g5d, and later copied into the
# guest-visible runtime closure by 57-install-test-tools.sh. Installed under
# /opt/rust so 90-cleanup.sh can remove the build-time cargo cache without
# touching $HOME. Pinned (not "stable") so consecutive image rebuilds produce
# bit-identical g5d binaries.
echo "[g5-worker] installing Rust toolchain ${RUST_TOOLCHAIN} via rustup-init ${RUSTUP_VERSION}"
export RUSTUP_HOME=/opt/rust/rustup
export CARGO_HOME=/opt/rust/cargo
mkdir -p "$RUSTUP_HOME" "$CARGO_HOME"

RUST_DL_DIR="$(mktemp -d)"
trap 'rm -rf "$RUST_DL_DIR"' EXIT
RUSTUP_INIT="$RUST_DL_DIR/rustup-init"
RUSTUP_URL="https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUST_TARGET}/rustup-init"
echo "[g5-worker] downloading rustup-init ${RUSTUP_VERSION} from $RUSTUP_URL"
download_with_retry "rustup-init ${RUSTUP_VERSION}" "$RUSTUP_URL" "$RUSTUP_INIT"
if [ -n "${RUSTUP_INIT_SHA256:-}" ]; then
    verify_sha256 "$RUSTUP_INIT" "$RUSTUP_INIT_SHA256" "rustup-init"
else
    download_with_retry "rustup-init checksum" "${RUSTUP_URL}.sha256" "$RUST_DL_DIR/rustup-init.sha256"
    EXPECTED_RUSTUP_SHA="$(awk '{print $1}' "$RUST_DL_DIR/rustup-init.sha256")"
    [ -n "$EXPECTED_RUSTUP_SHA" ] || {
        echo "[g5-worker] FATAL: rustup-init checksum file was empty" >&2
        exit 1
    }
    verify_sha256 "$RUSTUP_INIT" "$EXPECTED_RUSTUP_SHA" "rustup-init"
    echo "[g5-worker] HINT: pin -var rustup_init_sha256=$EXPECTED_RUSTUP_SHA for a reproducible supply-chain lock"
fi
chmod 0755 "$RUSTUP_INIT"
"$RUSTUP_INIT" -y --default-toolchain "${RUST_TOOLCHAIN}" --profile minimal --no-modify-path

ln -sf "$CARGO_HOME/bin/rustc" /usr/local/bin/rustc
ln -sf "$CARGO_HOME/bin/cargo" /usr/local/bin/cargo
ln -sf "$CARGO_HOME/bin/rustup" /usr/local/bin/rustup

rustc --version
cargo --version

# Verify the KVM kernel modules are present in the kernel package. The build
# VM may not load them (the qemu builder runs nested under L1 KVM), but the
# .ko files must be on disk for the deployment-time host to modprobe them.
modinfo kvm >/dev/null || {
    echo "[g5-worker] FATAL: kvm kernel module not present in this kernel" >&2
    exit 1
}
modinfo kvm_intel >/dev/null 2>&1 || modinfo kvm_amd >/dev/null 2>&1 || {
    echo "[g5-worker] FATAL: neither kvm_intel nor kvm_amd modinfo lookup succeeded; image cannot host microVMs" >&2
    exit 1
}

echo "[g5-worker] 10-apt-deps.sh OK"
