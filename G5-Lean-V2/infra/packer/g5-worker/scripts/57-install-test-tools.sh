#!/usr/bin/env bash
# =============================================================================
# 57-install-test-tools.sh — Install unit-test CLIs into the mounted runtime bin.
#
# Claude Code and Codex run shell commands inside the workspace VM. The
# host's runtime bin dir is mounted read-only at /opt/g5/bin, so Rust/client test
# tools must live in that runtime closure rather than only on the workspace host.
# =============================================================================
set -euo pipefail

if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" = "1" ]; then
    echo "[g5-worker] SKIP_RUNTIME_CLI_INSTALL=1; skipping test-tool install (template validation mode)"
    exit 0
fi

RUNTIME_BIN="${RUNTIME_BIN:-/var/g5d-data/runtime/bin}"
install -d -m 0755 "$RUNTIME_BIN"

# 1.97.1 tracks the workspace toolchain used by the base-rust and g5d-base tiers.
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"
RUSTUP_VERSION="${RUSTUP_VERSION:-1.29.0}"
RUSTUP_VERSION="${RUSTUP_VERSION#v}"
JUST_VERSION="${JUST_VERSION:-1.55.1}"
JUST_VERSION="${JUST_VERSION#v}"
CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION:-0.9.138}"
CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION#cargo-nextest-}"
CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION#v}"

case "$(uname -m)" in
    x86_64|amd64)
        RUST_TARGET=x86_64-unknown-linux-gnu
        JUST_TARGET=x86_64-unknown-linux-musl
        NEXTEST_TARGET=x86_64-unknown-linux-gnu
        ;;
    aarch64|arm64)
        RUST_TARGET=aarch64-unknown-linux-gnu
        JUST_TARGET=aarch64-unknown-linux-musl
        NEXTEST_TARGET=aarch64-unknown-linux-gnu
        ;;
    *)
        echo "[g5-worker] FATAL: unsupported test-tool architecture: $(uname -m)" >&2
        exit 1
        ;;
esac

DL_DIR="$(mktemp -d)"
trap 'rm -rf "$DL_DIR"' EXIT

# Pinned-artifact cache baked into the staging image (Dockerfile.g5d-base
# "Layer 8.5"); when a pinned tarball/checksum is present there, use it instead
# of downloading so GitHub release-asset outages stay off the CI critical path
# (G5Labs/G5#1811). Absent cache (Packer VM, plain ubuntu:24.04 local staging,
# version drift) falls back to download_with_retry below.
DL_CACHE="${G5_DL_CACHE:-/opt/g5/dl-cache}"

fetch_cached() {
    # fetch_cached <cache-file-name> <label> <url> <dest-path>
    if [ -f "$DL_CACHE/$1" ]; then
        echo "[g5-worker] using cached $1 from $DL_CACHE"
        cp "$DL_CACHE/$1" "$4"
    else
        download_with_retry "$2" "$3" "$4"
    fi
}

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

# Shared bounded-retry download helper, uploaded by the packer `file`
# provisioner ahead of Phase 1 (G5Labs/G5#1811). Sourced rather than
# redefined here so the whole provisioner chain shares one implementation.
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

write_rust_tool_wrapper() {
    local tool="$1"
    cat > "$RUNTIME_BIN/$tool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
TOOL="$(basename "$0")"
TOOLCHAIN_BIN="$(find "$HERE/rust/rustup/toolchains" -mindepth 2 -maxdepth 2 -type d -name bin | sort | head -n 1)"
if [ -z "$TOOLCHAIN_BIN" ] || [ ! -x "$TOOLCHAIN_BIN/$TOOL" ]; then
    echo "FATAL: mounted Rust toolchain is missing $TOOL under $HERE/rust/rustup/toolchains" >&2
    exit 1
fi
export RUSTUP_HOME="${RUSTUP_HOME:-$HERE/rust/rustup}"
if [ -z "${CARGO_HOME:-}" ]; then
    if [ -n "${G5_CARGO_HOME:-}" ]; then
        export CARGO_HOME="$G5_CARGO_HOME"
    else
        export CARGO_HOME="${HOME:-/tmp}/.cargo"
    fi
fi
mkdir -p "$CARGO_HOME" 2>/dev/null || true
export PATH="$HERE:$TOOLCHAIN_BIN:$PATH"
exec "$TOOLCHAIN_BIN/$TOOL" "$@"
EOF
    chmod 0755 "$RUNTIME_BIN/$tool"
}

echo "[g5-worker] staging Rust toolchain ${RUST_TOOLCHAIN} into $RUNTIME_BIN/rust"
RUST_RUNTIME="$RUNTIME_BIN/rust"
rm -rf "$RUST_RUNTIME"
install -d -m 0755 "$RUST_RUNTIME"

if [ -d /opt/rust/rustup/toolchains ]; then
    # Packer already installed the pinned build toolchain in 10-apt-deps.sh.
    # Copy only rustup's installed toolchains, not the build-time cargo cache.
    install -d -m 0755 "$RUST_RUNTIME/rustup"
    cp -a /opt/rust/rustup/toolchains "$RUST_RUNTIME/rustup/toolchains"
else
    RUSTUP_INIT="$DL_DIR/rustup-init"
    RUSTUP_URL="https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${RUST_TARGET}/rustup-init"
    download_with_retry "rustup-init ${RUSTUP_VERSION}" "$RUSTUP_URL" "$RUSTUP_INIT"
    if [ -n "${RUSTUP_INIT_SHA256:-}" ]; then
        verify_sha256 "$RUSTUP_INIT" "$RUSTUP_INIT_SHA256" "rustup-init"
    else
        download_with_retry "rustup-init checksum" "${RUSTUP_URL}.sha256" "$DL_DIR/rustup-init.sha256"
        EXPECTED_RUSTUP_SHA="$(awk '{print $1}' "$DL_DIR/rustup-init.sha256")"
        [ -n "$EXPECTED_RUSTUP_SHA" ] || {
            echo "[g5-worker] FATAL: rustup-init checksum file was empty" >&2
            exit 1
        }
        verify_sha256 "$RUSTUP_INIT" "$EXPECTED_RUSTUP_SHA" "rustup-init"
        echo "[g5-worker] HINT: pin -var rustup_init_sha256=$EXPECTED_RUSTUP_SHA for a reproducible supply-chain lock"
    fi
    chmod 0755 "$RUSTUP_INIT"
    TMP_RUSTUP_HOME="$DL_DIR/rustup-home"
    TMP_CARGO_HOME="$DL_DIR/cargo-home"
    RUSTUP_HOME="$TMP_RUSTUP_HOME" CARGO_HOME="$TMP_CARGO_HOME" \
        "$RUSTUP_INIT" -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
    cp -a "$TMP_RUSTUP_HOME" "$RUST_RUNTIME/rustup"
fi
chmod -R a+rX "$RUST_RUNTIME"

TOOLCHAIN_BIN="$(find "$RUST_RUNTIME/rustup/toolchains" -mindepth 2 -maxdepth 2 -type d -name bin | sort | head -n 1)"
[ -n "$TOOLCHAIN_BIN" ] || {
    echo "[g5-worker] FATAL: no Rust toolchain bin dir staged under $RUST_RUNTIME/rustup/toolchains" >&2
    exit 1
}
for tool in cargo rustc rustdoc; do
    [ -x "$TOOLCHAIN_BIN/$tool" ] || {
        echo "[g5-worker] FATAL: staged Rust toolchain missing $tool at $TOOLCHAIN_BIN/$tool" >&2
        exit 1
    }
    write_rust_tool_wrapper "$tool"
done
for optional_tool in rustfmt clippy-driver; do
    if [ -x "$TOOLCHAIN_BIN/$optional_tool" ]; then
        write_rust_tool_wrapper "$optional_tool"
    fi
done

JUST_TARBALL="just-${JUST_VERSION}-${JUST_TARGET}.tar.gz"
JUST_URL="https://github.com/casey/just/releases/download/${JUST_VERSION}/${JUST_TARBALL}"
fetch_cached "$JUST_TARBALL" "just ${JUST_VERSION}" "$JUST_URL" "$DL_DIR/$JUST_TARBALL"
if [ -n "${JUST_SHA256:-}" ]; then
    verify_sha256 "$DL_DIR/$JUST_TARBALL" "$JUST_SHA256" "just"
else
    fetch_cached "just-${JUST_VERSION}-SHA256SUMS" "just SHA256SUMS" "https://github.com/casey/just/releases/download/${JUST_VERSION}/SHA256SUMS" "$DL_DIR/just-SHA256SUMS"
    EXPECTED_JUST_SHA="$(awk -v f="$JUST_TARBALL" '$2 == f {print $1}' "$DL_DIR/just-SHA256SUMS")"
    [ -n "$EXPECTED_JUST_SHA" ] || {
        echo "[g5-worker] FATAL: $JUST_TARBALL not listed in just SHA256SUMS" >&2
        exit 1
    }
    verify_sha256 "$DL_DIR/$JUST_TARBALL" "$EXPECTED_JUST_SHA" "just"
    echo "[g5-worker] HINT: pin -var just_sha256=$EXPECTED_JUST_SHA for a reproducible supply-chain lock"
fi
tar -xzf "$DL_DIR/$JUST_TARBALL" -C "$DL_DIR"
[ -x "$DL_DIR/just" ] || {
    echo "[g5-worker] FATAL: just archive missing executable 'just'" >&2
    exit 1
}
install -m 0755 "$DL_DIR/just" "$RUNTIME_BIN/just"

NEXTEST_TAG="cargo-nextest-${CARGO_NEXTEST_VERSION}"
NEXTEST_TARBALL="${NEXTEST_TAG}-${NEXTEST_TARGET}.tar.gz"
NEXTEST_URL="https://github.com/nextest-rs/nextest/releases/download/${NEXTEST_TAG}/${NEXTEST_TARBALL}"
fetch_cached "$NEXTEST_TARBALL" "cargo-nextest ${CARGO_NEXTEST_VERSION}" "$NEXTEST_URL" "$DL_DIR/$NEXTEST_TARBALL"
if [ -n "${CARGO_NEXTEST_SHA256:-}" ]; then
    verify_sha256 "$DL_DIR/$NEXTEST_TARBALL" "$CARGO_NEXTEST_SHA256" "cargo-nextest"
else
    fetch_cached "${NEXTEST_TARBALL%.tar.gz}.sha256" "cargo-nextest checksum" "${NEXTEST_URL%.tar.gz}.sha256" "$DL_DIR/cargo-nextest.sha256"
    EXPECTED_NEXTEST_SHA="$(awk '{print $1}' "$DL_DIR/cargo-nextest.sha256")"
    [ -n "$EXPECTED_NEXTEST_SHA" ] || {
        echo "[g5-worker] FATAL: cargo-nextest checksum file was empty" >&2
        exit 1
    }
    verify_sha256 "$DL_DIR/$NEXTEST_TARBALL" "$EXPECTED_NEXTEST_SHA" "cargo-nextest"
    echo "[g5-worker] HINT: pin -var cargo_nextest_sha256=$EXPECTED_NEXTEST_SHA for a reproducible supply-chain lock"
fi
tar -xzf "$DL_DIR/$NEXTEST_TARBALL" -C "$DL_DIR"
[ -x "$DL_DIR/cargo-nextest" ] || {
    echo "[g5-worker] FATAL: cargo-nextest archive missing executable 'cargo-nextest'" >&2
    exit 1
}
install -m 0755 "$DL_DIR/cargo-nextest" "$RUNTIME_BIN/cargo-nextest"

CARGO_VERSION_OUT="$DL_DIR/cargo-version.out"
RUSTC_VERSION_OUT="$DL_DIR/rustc-version.out"
CARGO_NEXTEST_VERSION_OUT="$DL_DIR/cargo-nextest-version.out"
JUST_VERSION_OUT="$DL_DIR/just-version.out"

if ! "$RUNTIME_BIN/cargo" --version >"$CARGO_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: cargo --version failed under the mounted runtime layout" >&2
    cat "$CARGO_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/rustc" --version >"$RUSTC_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: rustc --version failed under the mounted runtime layout" >&2
    cat "$RUSTC_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/cargo" nextest --version >"$CARGO_NEXTEST_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: cargo nextest --version failed under the mounted runtime layout" >&2
    cat "$CARGO_NEXTEST_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/just" --version >"$JUST_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: just --version failed under the mounted runtime layout" >&2
    cat "$JUST_VERSION_OUT" >&2 || true
    exit 1
fi

awk 'NR == 1 {print $2; exit}' "$CARGO_VERSION_OUT" > "$RUNTIME_BIN/cargo.version"
awk 'NR == 1 {print $2; exit}' "$RUSTC_VERSION_OUT" > "$RUNTIME_BIN/rustc.version"
awk 'NR == 1 {print $2; exit}' "$CARGO_NEXTEST_VERSION_OUT" > "$RUNTIME_BIN/cargo-nextest.version"
awk 'NR == 1 {print $2; exit}' "$JUST_VERSION_OUT" > "$RUNTIME_BIN/just.version"
printf "%s\n" "$RUSTUP_VERSION" > "$RUNTIME_BIN/rustup.version"

echo "[g5-worker] cargo --version: $(cat "$CARGO_VERSION_OUT")"
echo "[g5-worker] rustc --version: $(cat "$RUSTC_VERSION_OUT")"
echo "[g5-worker] cargo nextest --version: $(cat "$CARGO_NEXTEST_VERSION_OUT")"
echo "[g5-worker] just --version: $(cat "$JUST_VERSION_OUT")"
echo "[g5-worker] 57-install-test-tools.sh OK (rustup=$(cat "$RUNTIME_BIN/rustup.version"), cargo=$(cat "$RUNTIME_BIN/cargo.version"), rustc=$(cat "$RUNTIME_BIN/rustc.version"), cargo-nextest=$(cat "$RUNTIME_BIN/cargo-nextest.version"), just=$(cat "$RUNTIME_BIN/just.version"))"
