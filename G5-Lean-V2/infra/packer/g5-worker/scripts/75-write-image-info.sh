#!/usr/bin/env bash
# =============================================================================
# 75-write-image-info.sh — Bake build metadata into /etc/g5/worker-image.json.
#
# Per rev-7 plan §5.2 (worker capability reporting), the supervisor reads
# this file at startup and reports image_sha + image_version +
# runtime_bin_versions to the control plane. The PR-rev7-3 implementation
# will hard-depend on this file's schema; treat it as a stable contract.
#
# Schema (JSON):
#   {
#     "image_version":   "g5-worker-vYYYYMMDD-HHMM-<sha>",
#     "image_family":    "g5-worker",
#     "build_timestamp": "YYYYMMDD-HHMM",
#     "git_short_sha":   "<7-hex-sha or 'devbuild'>",
#     "cloud_hypervisor_version": "<git tag>",
#     "guest_kernel_version": "<release tag>",
#     "rust_toolchain":  "<rust version pinned at build time>",
#     "kernel_version":  "<uname -r>",
#     "root_filesystem": "xfs",
#     "xfs_reflink":     "1",
#     "runtime_platform_sha256": { "cloud-hypervisor": "...", "vmlinux": "...",
#                                  "initramfs.cpio": "...", "kernel.config": "...",
#                                  "passt": "..." }   (runtime/platform, inner hosts)
#     "runtime_bin_versions": {
#       "g5d":    { "version": "...", "sha256": "...", "size": <bytes> },
#       "claude": { "version": "...", "sha256": "...", "size": <bytes> },
#       "codex":  { "version": "...", "sha256": "...", "size": <bytes> },
#       "gh":     { "version": "...", "sha256": "...", "size": <bytes> },
#       "gcloud": { "version": "...", "sha256": "...", "size": <bytes> },
#       "gke-gcloud-auth-plugin": { "version": "...", "sha256": "...", "size": <bytes> },
#       "kubectl": { "version": "...", "sha256": "...", "size": <bytes> },
#       "node":   { "version": "...", "sha256": "...", "size": <bytes> },
#       "npm":    { "version": "...", "sha256": "...", "size": <bytes> },
#       "pnpm":   { "version": "...", "sha256": "...", "size": <bytes> },
#       "cargo":  { "version": "...", "sha256": "...", "size": <bytes> },
#       "rustc":  { "version": "...", "sha256": "...", "size": <bytes> },
#       "cargo-nextest": { "version": "...", "sha256": "...", "size": <bytes> },
#       "just":   { "version": "...", "sha256": "...", "size": <bytes> }
#     }
#   }
# =============================================================================
set -euo pipefail
source /opt/g5-host/boot/versions.env

bin_info() {
    local path="$1"
    if [ ! -x "$path" ]; then
        jq -n '{version: "missing", sha256: "missing", size: 0}'
        return
    fi
    local sha size ver
    sha="$(sha256sum "$path" | awk '{print $1}')"
    size="$(stat -c '%s' "$path")"
    ver="unknown"
    case "$path" in
        */g5d)
            ver="$(LD_LIBRARY_PATH="/var/g5d-data/runtime/libs" "$path" --version 2>/dev/null | awk '{print $2}' || echo unknown)"
            ;;
        */node)
            ver="$("$path" --version 2>/dev/null | sed 's/^v//' || echo unknown)"
            ;;
    esac
    jq -n --arg v "$ver" --arg s "$sha" --argjson sz "$size" \
        '{version: $v, sha256: $s, size: $sz}'
}

read_version() {
    local runtime_path="$1"
    local tmp_path="$2"
    if [ -f "$runtime_path" ]; then
        cat "$runtime_path"
    elif [ -f "$tmp_path" ]; then
        cat "$tmp_path"
    else
        echo unknown
    fi
}

CLAUDE_VER="$(read_version /var/g5d-data/runtime/claude-code.version /tmp/claude-code.version)"
CODEX_VER="$(read_version /var/g5d-data/runtime/bin/codex.version /tmp/codex.version)"
GH_VER="$(read_version /var/g5d-data/runtime/bin/gh.version /tmp/gh.version)"
GCLOUD_VER="$(read_version /var/g5d-data/runtime/bin/gcloud.version /tmp/gcloud.version)"
GKE_AUTH_PLUGIN_VER="$(read_version /var/g5d-data/runtime/bin/gke-gcloud-auth-plugin.version /tmp/gke-gcloud-auth-plugin.version)"
KUBECTL_VER="$(read_version /var/g5d-data/runtime/bin/kubectl.version /tmp/kubectl.version)"
NPM_VER="$(read_version /var/g5d-data/runtime/bin/npm.version /tmp/npm.version)"
NPX_VER="$(read_version /var/g5d-data/runtime/bin/npx.version /tmp/npx.version)"
COREPACK_VER="$(read_version /var/g5d-data/runtime/bin/corepack.version /tmp/corepack.version)"
PNPM_VER="$(read_version /var/g5d-data/runtime/bin/pnpm.version /tmp/pnpm.version)"
CARGO_VER="$(read_version /var/g5d-data/runtime/bin/cargo.version /tmp/cargo.version)"
RUSTC_VER="$(read_version /var/g5d-data/runtime/bin/rustc.version /tmp/rustc.version)"
CARGO_NEXTEST_VER="$(read_version /var/g5d-data/runtime/bin/cargo-nextest.version /tmp/cargo-nextest.version)"
JUST_VER="$(read_version /var/g5d-data/runtime/bin/just.version /tmp/just.version)"

# G5D / node info objects.
G5D_JSON="$(bin_info /var/g5d-data/runtime/bin/g5d)"
NODE_JSON="$(bin_info /var/g5d-data/runtime/bin/node)"

# Claude + codex are wrapper shims; record the wrapper hash plus the npm
# resolved version explicitly (the wrapper hash doesn't change when the
# underlying npm package is bumped — we want both fields).
bin_info_with_version() {
    local path="$1"
    local version="$2"
    if [ ! -x "$path" ]; then
        jq -n --arg v "$version" '{version: $v, sha256: "missing", size: 0}'
        return
    fi
    local sha size
    sha="$(sha256sum "$path" | awk '{print $1}')"
    size="$(stat -c '%s' "$path")"
    jq -n --arg v "$version" --arg s "$sha" --argjson sz "$size" \
        '{version: $v, sha256: $s, size: $sz}'
}

CLAUDE_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/claude "$CLAUDE_VER")"
CODEX_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/codex "$CODEX_VER")"
GH_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/gh "$GH_VER")"
GCLOUD_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/gcloud "$GCLOUD_VER")"
GKE_AUTH_PLUGIN_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/gke-gcloud-auth-plugin "$GKE_AUTH_PLUGIN_VER")"
KUBECTL_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/kubectl "$KUBECTL_VER")"
NPM_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/npm "$NPM_VER")"
NPX_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/npx "$NPX_VER")"
COREPACK_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/corepack "$COREPACK_VER")"
PNPM_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/pnpm "$PNPM_VER")"
CARGO_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/cargo "$CARGO_VER")"
RUSTC_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/rustc "$RUSTC_VER")"
CARGO_NEXTEST_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/cargo-nextest "$CARGO_NEXTEST_VER")"
JUST_JSON="$(bin_info_with_version /var/g5d-data/runtime/bin/just "$JUST_VER")"

# The platform the runtime disk carries for inner hosts (stage-runtime-platform.sh):
# the same pins as /opt/g5-host, recorded so a deployed image shows what a
# nested-enabled workspace VM will run.
PLATFORM_SUMS=/var/g5d-data/runtime/platform/SHA256SUMS
if [ -s "$PLATFORM_SUMS" ]; then
    PLATFORM_JSON="$(awk '{printf "%s%s", (NR > 1 ? "," : ""), "\"" $2 "\":\"" $1 "\""}' "$PLATFORM_SUMS" | sed 's/^/{/; s/$/}/')"
    echo "$PLATFORM_JSON" | jq -e . >/dev/null || { echo "[g5-worker] FATAL: unreadable $PLATFORM_SUMS" >&2; exit 1; }
else
    echo "[g5-worker] FATAL: runtime platform digests missing at $PLATFORM_SUMS (build-runtime-disk.sh should stage them)" >&2
    exit 1
fi

install -d -m 0755 /etc/g5

jq -n \
    --arg image_version    "${G5_IMAGE_VERSION:-unknown}" \
    --arg image_family     "${G5_IMAGE_FAMILY:-g5-worker}" \
    --arg build_timestamp  "${G5_BUILD_TIMESTAMP:-unknown}" \
    --arg git_short_sha    "${G5_GIT_SHORT_SHA:-unknown}" \
    --arg cloud_hypervisor_version  "${CLOUD_HYPERVISOR_VERSION:-unknown}" \
    --arg guest_kernel_version "${GUEST_KERNEL_VERSION:-unknown}" \
    --arg rust_toolchain   "${RUST_TOOLCHAIN:-unknown}" \
    --arg kernel_version   "$(uname -r)" \
    --argjson g5d    "$G5D_JSON" \
    --argjson claude "$CLAUDE_JSON" \
    --argjson codex  "$CODEX_JSON" \
    --argjson gh     "$GH_JSON" \
    --argjson gcloud "$GCLOUD_JSON" \
    --argjson gke_auth_plugin "$GKE_AUTH_PLUGIN_JSON" \
    --argjson kubectl "$KUBECTL_JSON" \
    --argjson node   "$NODE_JSON" \
    --argjson npm    "$NPM_JSON" \
    --argjson npx    "$NPX_JSON" \
    --argjson corepack "$COREPACK_JSON" \
    --argjson pnpm   "$PNPM_JSON" \
    --argjson cargo  "$CARGO_JSON" \
    --argjson rustc  "$RUSTC_JSON" \
    --argjson cargo_nextest "$CARGO_NEXTEST_JSON" \
    --argjson just   "$JUST_JSON" \
    --argjson platform "$PLATFORM_JSON" \
    '{
        image_version: $image_version,
        image_family: $image_family,
        build_timestamp: $build_timestamp,
        git_short_sha: $git_short_sha,
        cloud_hypervisor_version: $cloud_hypervisor_version,
        guest_kernel_version: $guest_kernel_version,
        rust_toolchain: $rust_toolchain,
        kernel_version: $kernel_version,
        root_filesystem: "xfs",
        xfs_reflink: "1",
        runtime_platform_sha256: $platform,
        runtime_bin_versions: {
            g5d: $g5d,
            claude: $claude,
            codex: $codex,
            gh: $gh,
            gcloud: $gcloud,
            "gke-gcloud-auth-plugin": $gke_auth_plugin,
            kubectl: $kubectl,
            node: $node,
            npm: $npm,
            npx: $npx,
            corepack: $corepack,
            pnpm: $pnpm,
            cargo: $cargo,
            rustc: $rustc,
            "cargo-nextest": $cargo_nextest,
            just: $just
        }
    }' > /etc/g5/worker-image.json

chmod 0644 /etc/g5/worker-image.json

echo "[g5-worker] /etc/g5/worker-image.json:"
cat /etc/g5/worker-image.json

echo "[g5-worker] 75-write-image-info.sh OK"
