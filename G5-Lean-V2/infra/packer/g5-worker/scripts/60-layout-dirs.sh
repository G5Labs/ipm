#!/usr/bin/env bash
# =============================================================================
# 60-layout-dirs.sh — Pre-create the /var/g5d-data/ directory layout per
# rev-7 plan §3 / §5.0.
#
# Final layout on the image (cache/ and lib/workspaces/ are intentionally
# empty — they're populated at runtime by the g5d host supervisor in
# PR-rev7-3):
#
#   /var/g5d-data/
#   ├── runtime/bin/         g5d, claude, codex, gh, gcloud,
#   │                        gke-gcloud-auth-plugin, kubectl, node, npm, pnpm,
#   │                        cargo, rustc, cargo-nextest, just
#   │                        (populated by 30/40/50/55/56/57)
#   │   └── _npm/            vendored npm tree backing claude+codex wrappers
#   │                        (populated by 50; lives under bin/ so it's
#   │                        carried by the rev-7 §5.4 mount-injection)
#   ├── runtime/libs/        platform dependency libraries
#   ├── cache/               per-cache subdirs created on demand
#   │   ├── repos/      warm bare-repo checkouts (per rev-7 §5.4)
#   │   ├── oci-layers/ pulled customer base_image layers (per rev-7 §5.3)
#   │   ├── sccache/    Rust sccache (per rev-7 §5.5)
#   │   ├── pip/        Python wheels (per rev-7 §5.5)
#   │   ├── npm/        node_modules tarballs (per rev-7 §5.5)
#   │   ├── cargo/      crates.io registry (per rev-7 §5.5)
#   │   └── go-mod/     Go modules (per rev-7 §5.5)
#   └── lib/workspaces/  per-workspace-VM reflink clones (per rev-7 §5.4)
#
# Ownership: g5d host runs as root today; the workspace mount is shared
# into customer microVMs that run as their own configured UID.
# =============================================================================
set -euo pipefail

verify_codex_ca_bundle() {
    [ -s /var/g5d-data/runtime/bin/ca-certificates.crt ] || {
        echo "[g5-worker] FATAL: CA bundle missing or empty: /var/g5d-data/runtime/bin/ca-certificates.crt (50-install-codex.sh should stage it)" >&2
        exit 1
    }
    grep -q "BEGIN CERTIFICATE" /var/g5d-data/runtime/bin/ca-certificates.crt || {
        echo "[g5-worker] FATAL: CA bundle does not look like a PEM trust store: /var/g5d-data/runtime/bin/ca-certificates.crt" >&2
        exit 1
    }
}

# Root and cache layout.
install -d -m 0755 /var/g5d-data
# Match the narrow passt policy and the container/self-hosted mount point.
# Both paths are on the host's XFS root; there is no guest filesystem mount.
install -d -m 0755 /var/lib/g5d/rootfs
install -d -m 0755 /var/g5d-data/runtime
install -d -m 0755 /var/g5d-data/runtime/bin
install -d -m 0755 /var/g5d-data/runtime/libs
# Note: /var/g5d-data/runtime/npm is a build-only staging dir created by
# 40-install-claude.sh and removed in 90-cleanup.sh. The mount-injected
# copy lives at /var/g5d-data/runtime/bin/_npm/ — that path is created
# by 50-install-codex.sh as part of vendoring.
install -d -m 0755 /var/g5d-data/cache
install -d -m 0755 /var/g5d-data/cache/repos
install -d -m 0755 /var/g5d-data/cache/oci-layers
install -d -m 0755 /var/g5d-data/cache/sccache
install -d -m 0755 /var/g5d-data/cache/pip
install -d -m 0755 /var/g5d-data/cache/npm
install -d -m 0755 /var/g5d-data/cache/cargo
install -d -m 0755 /var/g5d-data/cache/go-mod
install -d -m 0755 /var/g5d-data/lib
install -d -m 0755 /var/g5d-data/lib/workspaces

# /etc/g5/ — image metadata + supervisor env file destination.
install -d -m 0755 /etc/g5

# Touch a sentinel so first-boot scripts in PR-rev7-3 can verify the layout
# was image-baked (not lazy-mkdir'd at runtime). Empty file; presence is the
# only signal.
touch /var/g5d-data/.g5-layout-v7

# Verify the runtime binaries that earlier phases placed are present.
REQUIRED_BINS=(/var/g5d-data/runtime/bin/g5d /var/g5d-data/runtime/bin/node)
if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" != "1" ]; then
    REQUIRED_BINS+=(/var/g5d-data/runtime/bin/claude /var/g5d-data/runtime/bin/codex /var/g5d-data/runtime/bin/gh /var/g5d-data/runtime/bin/gcloud /var/g5d-data/runtime/bin/gke-gcloud-auth-plugin /var/g5d-data/runtime/bin/kubectl /var/g5d-data/runtime/bin/npm /var/g5d-data/runtime/bin/npx /var/g5d-data/runtime/bin/corepack /var/g5d-data/runtime/bin/pnpm /var/g5d-data/runtime/bin/cargo /var/g5d-data/runtime/bin/rustc /var/g5d-data/runtime/bin/cargo-nextest /var/g5d-data/runtime/bin/just)
fi
for f in "${REQUIRED_BINS[@]}"; do
    [ -x "$f" ] || {
        echo "[g5-worker] FATAL: required binary missing or non-executable: $f" >&2
        exit 1
    }
done

CATALOG_ROOT=/var/g5d-data/runtime/modules
[ -s "$CATALOG_ROOT/catalog.json" ] && [ -s "$CATALOG_ROOT/catalog.sha256" ] || {
    echo "[g5-worker] FATAL: workspace runtime catalog missing (30-build-g5d.sh should stage it)" >&2
    exit 1
}
echo "$(tr -d '[:space:]' < "$CATALOG_ROOT/catalog.sha256")  $CATALOG_ROOT/catalog.json" | sha256sum -c - >/dev/null || {
    echo "[g5-worker] FATAL: workspace runtime catalog digest mismatch" >&2
    exit 1
}

# Verify the in-VM provider runtime (P5.5 — self-contained node). The in-VM
# runner launches the `codex` npm launcher on `node` from the mounted runtime
# bin dir, so `node` must be the OFFICIAL self-contained build
# (40-install-claude.sh) — i.e. it must NOT depend on libnode.so (the distro
# shim that crashes in-guest on externalized builtins). We also require the
# harness every agent turn reads. Mirrors host's verify_runtime_layout so a
# mis-staged image fails at BUILD time, not at thousand-VM boot.
if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" != "1" ]; then
    if ldd /var/g5d-data/runtime/bin/node 2>/dev/null | grep -q 'libnode\.so'; then
        echo "[g5-worker] FATAL: /var/g5d-data/runtime/bin/node depends on libnode.so — not the self-contained build (40-install-claude.sh must install the nodejs.org tarball)" >&2
        exit 1
    fi
    [ -s /var/g5d-data/runtime/assets/harness/AGENTS.md ] || {
        echo "[g5-worker] FATAL: G5 harness missing: /var/g5d-data/runtime/assets/harness/AGENTS.md (30-build-g5d.sh should stage it)" >&2
        exit 1
    }
    # Codex microVM self-containment (Phase 6, host D8). The in-VM runner
    # launches the staged `codex` CLI for a codex task, so it must be present in
    # the mounted runtime bin dir. The self-contained node check above (no
    # libnode.so) serves codex too — its npm launcher runs on the same
    # self-contained node + closure.
    [ -x /var/g5d-data/runtime/bin/codex ] || {
        echo "[g5-worker] FATAL: codex CLI missing or non-executable: /var/g5d-data/runtime/bin/codex (50-install-codex.sh should stage it)" >&2
        exit 1
    }
    [ -x /var/g5d-data/runtime/bin/gh ] || {
        echo "[g5-worker] FATAL: GitHub CLI missing or non-executable: /var/g5d-data/runtime/bin/gh (55-install-gh.sh should stage it)" >&2
        exit 1
    }
    if ! /var/g5d-data/runtime/bin/gh --version >/tmp/gh-version.out 2>&1; then
        echo "[g5-worker] FATAL: gh --version failed under the mounted runtime layout" >&2
        cat /tmp/gh-version.out >&2 || true
        exit 1
    fi
    [ -x /var/g5d-data/runtime/bin/gcloud ] || {
        echo "[g5-worker] FATAL: Google Cloud CLI missing or non-executable: /var/g5d-data/runtime/bin/gcloud (56-install-gcloud-kubectl.sh should stage it)" >&2
        exit 1
    }
    [ -x /var/g5d-data/runtime/bin/google-cloud-sdk/bin/gcloud ] || {
        echo "[g5-worker] FATAL: Google Cloud SDK closure missing: /var/g5d-data/runtime/bin/google-cloud-sdk/bin/gcloud (56-install-gcloud-kubectl.sh should stage it)" >&2
        exit 1
    }
    [ -x /var/g5d-data/runtime/bin/google-cloud-sdk/bin/gke-gcloud-auth-plugin ] || {
        echo "[g5-worker] FATAL: GKE auth plugin closure missing: /var/g5d-data/runtime/bin/google-cloud-sdk/bin/gke-gcloud-auth-plugin (56-install-gcloud-kubectl.sh should stage it)" >&2
        exit 1
    }
    if ! /var/g5d-data/runtime/bin/gcloud --version >/tmp/gcloud-version.out 2>&1; then
        echo "[g5-worker] FATAL: gcloud --version failed under the mounted runtime layout" >&2
        cat /tmp/gcloud-version.out >&2 || true
        exit 1
    fi
    if ! /var/g5d-data/runtime/bin/gke-gcloud-auth-plugin --version >/tmp/gke-gcloud-auth-plugin-version.out 2>&1; then
        echo "[g5-worker] FATAL: gke-gcloud-auth-plugin --version failed under the mounted runtime layout" >&2
        cat /tmp/gke-gcloud-auth-plugin-version.out >&2 || true
        exit 1
    fi
    [ -x /var/g5d-data/runtime/bin/kubectl ] || {
        echo "[g5-worker] FATAL: kubectl missing or non-executable: /var/g5d-data/runtime/bin/kubectl (56-install-gcloud-kubectl.sh should stage it)" >&2
        exit 1
    }
    if ! /var/g5d-data/runtime/bin/kubectl version --client=true >/tmp/kubectl-version.out 2>&1; then
        echo "[g5-worker] FATAL: kubectl version --client=true failed under the mounted runtime layout" >&2
        cat /tmp/kubectl-version.out >&2 || true
        exit 1
    fi
    [ -f /var/g5d-data/runtime/bin/nodejs/lib/node_modules/npm/bin/npm-cli.js ] || {
        echo "[g5-worker] FATAL: npm launcher closure missing: /var/g5d-data/runtime/bin/nodejs/lib/node_modules/npm/bin/npm-cli.js (40-install-claude.sh should stage it)" >&2
        exit 1
    }
    [ -f /var/g5d-data/runtime/bin/nodejs/lib/node_modules/npm/bin/npx-cli.js ] || {
        echo "[g5-worker] FATAL: npx launcher closure missing: /var/g5d-data/runtime/bin/nodejs/lib/node_modules/npm/bin/npx-cli.js (40-install-claude.sh should stage it)" >&2
        exit 1
    }
    [ -f /var/g5d-data/runtime/bin/nodejs/lib/node_modules/corepack/dist/corepack.js ] || {
        echo "[g5-worker] FATAL: Corepack launcher closure missing: /var/g5d-data/runtime/bin/nodejs/lib/node_modules/corepack/dist/corepack.js (40-install-claude.sh should stage it)" >&2
        exit 1
    }
    [ -f /var/g5d-data/runtime/bin/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs ] || {
        echo "[g5-worker] FATAL: vendored pnpm launcher closure missing: /var/g5d-data/runtime/bin/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs (40-install-claude.sh should stage it)" >&2
        exit 1
    }
    if ! npm_out="$(/var/g5d-data/runtime/bin/npm --version 2>&1)"; then
        echo "[g5-worker] FATAL: npm --version failed under the mounted runtime layout" >&2
        echo "$npm_out" >&2
        exit 1
    fi
    if ! npx_out="$(/var/g5d-data/runtime/bin/npx --version 2>&1)"; then
        echo "[g5-worker] FATAL: npx --version failed under the mounted runtime layout" >&2
        echo "$npx_out" >&2
        exit 1
    fi
    if ! corepack_out="$(/var/g5d-data/runtime/bin/corepack --version 2>&1)"; then
        echo "[g5-worker] FATAL: corepack --version failed under the mounted runtime layout" >&2
        echo "$corepack_out" >&2
        exit 1
    fi
    if ! pnpm_out="$(/var/g5d-data/runtime/bin/pnpm --version 2>&1)"; then
        echo "[g5-worker] FATAL: pnpm --version failed under the mounted runtime layout" >&2
        echo "$pnpm_out" >&2
        exit 1
    fi
    [ -d /var/g5d-data/runtime/bin/rust/rustup/toolchains ] || {
        echo "[g5-worker] FATAL: Rust toolchain closure missing: /var/g5d-data/runtime/bin/rust/rustup/toolchains (57-install-test-tools.sh should stage it)" >&2
        exit 1
    }
    if ! /var/g5d-data/runtime/bin/cargo --version >/tmp/cargo-version.out 2>&1; then
        echo "[g5-worker] FATAL: cargo --version failed under the mounted runtime layout" >&2
        cat /tmp/cargo-version.out >&2 || true
        exit 1
    fi
    if ! /var/g5d-data/runtime/bin/rustc --version >/tmp/rustc-version.out 2>&1; then
        echo "[g5-worker] FATAL: rustc --version failed under the mounted runtime layout" >&2
        cat /tmp/rustc-version.out >&2 || true
        exit 1
    fi
    if ! /var/g5d-data/runtime/bin/cargo nextest --version >/tmp/cargo-nextest-version.out 2>&1; then
        echo "[g5-worker] FATAL: cargo nextest --version failed under the mounted runtime layout" >&2
        cat /tmp/cargo-nextest-version.out >&2 || true
        exit 1
    fi
    if ! /var/g5d-data/runtime/bin/just --version >/tmp/just-version.out 2>&1; then
        echo "[g5-worker] FATAL: just --version failed under the mounted runtime layout" >&2
        cat /tmp/just-version.out >&2 || true
        exit 1
    fi
    verify_codex_ca_bundle
    # Reproduce the exact in-VM resolution: run the codex launcher under the
    # mounted layout (LD_LIBRARY_PATH = the runtime bin dir, where node's
    # closure is co-located). An unresolvable node closure or a broken npm
    # launcher fails the BUILD here, not at thousand-VM boot.
    if ! LD_LIBRARY_PATH=/var/g5d-data/runtime/bin \
        /var/g5d-data/runtime/bin/codex --version; then
        echo "[g5-worker] FATAL: codex --version failed under the mounted runtime layout" >&2
        exit 1
    fi
fi

# SEC-C01: the host must never execute the bundle it exports to guests.
# Copy to a distinct immutable image prefix; reflinks share storage, not inodes.
# This runs during image construction, before the service can start.
install -d -m 0755 /opt/g5-host
for directory in bin libs modules assets; do
    cp -a --reflink=auto "/var/g5d-data/runtime/$directory" /opt/g5-host/
done
[ ! /opt/g5-host/bin/g5d -ef /var/g5d-data/runtime/bin/g5d ] || {
    echo "[g5-worker] FATAL: host and guest g5d must have different inodes" >&2
    exit 1
}

echo "[g5-worker] /var/g5d-data/ layout:"
find /var/g5d-data -maxdepth 3 -type d | sort

echo "[g5-worker] 60-layout-dirs.sh OK"

# Freeze the complete independently mounted guest runtime once per image.
bash /tmp/g5d-src/scripts/build-runtime-disk.sh /var/g5d-data/runtime
