#!/usr/bin/env bash
# Regression for scripts/stage-runtime-platform.sh (ontology
# g5.architecture.microvm_backend.nested, design nested-virt-followup §3.2):
# a runtime disk carries the host's own pinned platform under platform/ when
# the building machine has one, ships none when it has none, and refuses a
# partial or digest-mismatched platform.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fail() { echo "✗ $*" >&2; exit 1; }
SCRIPT="$ROOT/scripts/stage-runtime-platform.sh"

make_platform() { # <dir>
    mkdir -p "$1/bin" "$1/boot"
    printf '#!/bin/sh\necho cloud-hypervisor v54.0\n' > "$1/bin/cloud-hypervisor"
    printf 'vmlinux-bytes' > "$1/boot/vmlinux"
    printf 'initramfs-bytes' > "$1/boot/initramfs.cpio"
    printf 'CONFIG_KVM=y\nCONFIG_SECURITY_LANDLOCK=y\n' > "$1/boot/kernel.config"
    printf 'CLOUD_HYPERVISOR_VERSION=v54.0\n' > "$1/boot/versions.env"
    chmod 0755 "$1/bin/cloud-hypervisor"
    (cd "$1" && sha256sum bin/cloud-hypervisor boot/vmlinux boot/initramfs.cpio boot/kernel.config > boot/SHA256SUMS)
    printf '#!/bin/sh\necho passt 2026\n' > "$1/passt"
    chmod 0755 "$1/passt"
}

env_for() { # <platform dir>
    printf 'G5D_CLOUD_HYPERVISOR_BIN=%s G5D_GUEST_KERNEL=%s G5D_GUEST_INITRAMFS=%s G5D_PASST_BIN=%s' \
        "$1/bin/cloud-hypervisor" "$1/boot/vmlinux" "$1/boot/initramfs.cpio" "$1/passt"
}

# 1. A complete platform is staged with the kernel config beside the kernel and digests.
make_platform "$TMP/host"
mkdir -p "$TMP/runtime1/bin"
env $(env_for "$TMP/host") bash "$SCRIPT" "$TMP/runtime1" > "$TMP/out1"
for f in cloud-hypervisor vmlinux initramfs.cpio kernel.config passt SHA256SUMS versions.env; do
    [ -f "$TMP/runtime1/platform/$f" ] || fail "staged platform lacks $f"
done
[ -x "$TMP/runtime1/platform/cloud-hypervisor" ] && [ -x "$TMP/runtime1/platform/passt" ] || fail "VMM and passt must be executable"
[ ! -x "$TMP/runtime1/platform/vmlinux" ] || fail "the kernel is data, not executable"
(cd "$TMP/runtime1/platform" && sha256sum --check --status SHA256SUMS) || fail "staged SHA256SUMS must match the staged files"
cmp -s "$TMP/host/boot/kernel.config" "$TMP/runtime1/platform/kernel.config" || fail "kernel.config must be the host's"
grep -q 'staged' "$TMP/out1" || fail "staging must report what it did"
echo "✓ a complete platform is staged with its digests"

# 2. The platform is replaced, never merged, on a restage.
printf 'stale' > "$TMP/runtime1/platform/leftover"
env $(env_for "$TMP/host") bash "$SCRIPT" "$TMP/runtime1" >/dev/null
[ ! -e "$TMP/runtime1/platform/leftover" ] || fail "a restage must replace the directory"
echo "✓ a restage replaces the platform directory"

# 3. No platform anywhere: no directory, success, one line saying so. The
#    defaults point at /opt/g5-host, which this test machine must not have for
#    the case to be meaningful; otherwise point them into the empty fixture.
mkdir -p "$TMP/runtime2/bin" "$TMP/nowhere"
env $(env_for "$TMP/nowhere") bash "$SCRIPT" "$TMP/runtime2" > "$TMP/out2"
[ ! -e "$TMP/runtime2/platform" ] || fail "a machine without a platform must ship none"
grep -q 'ships none' "$TMP/out2" || fail "the absence must be reported: $(cat "$TMP/out2")"
echo "✓ a machine without a platform ships none and says so"

# 4. A partial platform fails the build instead of shipping half a launch.
make_platform "$TMP/partial"
rm "$TMP/partial/boot/initramfs.cpio"
mkdir -p "$TMP/runtime3/bin"
if env $(env_for "$TMP/partial") bash "$SCRIPT" "$TMP/runtime3" 2>"$TMP/err3"; then
    fail "a partial platform must fail"
fi
grep -q 'incomplete platform' "$TMP/err3" || fail "the failure must name the missing input: $(cat "$TMP/err3")"
[ ! -e "$TMP/runtime3/platform" ] || fail "a failed staging must leave no platform directory"
echo "✓ a partial platform fails the build"

# 5. A host digest list that disagrees with the files fails the build.
make_platform "$TMP/tampered"
printf 'other-kernel' > "$TMP/tampered/boot/vmlinux"
mkdir -p "$TMP/runtime4/bin"
if env $(env_for "$TMP/tampered") bash "$SCRIPT" "$TMP/runtime4" 2>"$TMP/err4"; then
    fail "a digest mismatch must fail"
fi
grep -q 'does not match' "$TMP/err4" || fail "the failure must name the digest list: $(cat "$TMP/err4")"
echo "✓ a digest mismatch against the host's SHA256SUMS fails the build"

# 6. build-runtime-disk.sh calls the stager once, after the bootstrap staging,
#    so every producer of a runtime disk (Packer, local staging, the CI
#    fixture) carries the platform through one owner.
BUILD="$ROOT/scripts/build-runtime-disk.sh"
[ "$(grep -c 'stage-runtime-platform.sh' "$BUILD")" = 1 ] || fail "build-runtime-disk.sh must stage the platform exactly once"
L_BOOT="$(grep -n 'stage-guest-bootstrap.py' "$BUILD" | head -1 | cut -d: -f1)"
L_PLAT="$(grep -n 'stage-runtime-platform.sh' "$BUILD" | cut -d: -f1)"
L_SIZE="$(grep -n 'du --apparent-size' "$BUILD" | cut -d: -f1)"
[ "$L_BOOT" -lt "$L_PLAT" ] && [ "$L_PLAT" -lt "$L_SIZE" ] || fail "the platform must be staged after the bootstrap and before the disk is sized"
# The local staging container receives a COPY of the platform under the
# staging output (platform-input/, beside runtime/), never a bind of the
# machine's own paths: a Docker-out-of-Docker caller's bind sources name the
# daemon host, not the builder. Run the input assembly alone against a fake
# platform and check the layout, the environment and the all-or-none rule.
STAGE="$ROOT/scripts/stage-local-worker-runtime.sh"
grep -q '${PLATFORM_MOUNTS\[@\]+"${PLATFORM_MOUNTS\[@\]}"}' "$STAGE" || fail "the runtime-disk container must receive the platform mounts"
grep -q 'rm -rf "$PLATFORM_INPUT"' "$STAGE" || fail "the platform input must be removed after the disk build"
block="$(sed -n '/^# --- platform input (begin) ---$/,/^# --- platform input (end) ---$/p' "$STAGE")"
[ -n "$block" ] || fail "the platform input block was not found in $STAGE"
run_input() { # <platform dir or empty> <passt path or empty> → prints the mounts, one per line; the caller sets INPUT_OUT
    local plat="$1" passt="$2"
    ( set -euo pipefail; OUT_DIR="$INPUT_OUT"; info() { :; }; fail() { echo "FAIL: $*"; exit 1; }
      uname() { echo Linux; }
      export G5D_CLOUD_HYPERVISOR_BIN="${plat:+$plat/bin/cloud-hypervisor}" G5D_GUEST_KERNEL="${plat:+$plat/boot/vmlinux}" \
             G5D_GUEST_INITRAMFS="${plat:+$plat/boot/initramfs.cpio}" G5D_PASST_BIN="$passt"
      [ -n "$plat" ] || unset G5D_CLOUD_HYPERVISOR_BIN G5D_GUEST_KERNEL G5D_GUEST_INITRAMFS
      [ -n "$passt" ] || unset G5D_PASST_BIN
      eval "$block"
      printf '%s\n' ${PLATFORM_MOUNTS[@]+"${PLATFORM_MOUNTS[@]}"} )
}
make_platform "$TMP/stage-host"
INPUT_OUT="$(mktemp -d)"
out="$(run_input "$TMP/stage-host" "$TMP/stage-host/passt")" || fail "the host-layout platform must stage: $out"
printf '%s\n' "$out" | grep -qx -- "$INPUT_OUT/platform-input:/platform-input:ro" || fail "the copied input directory must be the only bind: $out"
printf '%s\n' "$out" | grep -q -- "^$TMP" && fail "no machine path may be bound into the container: $out"
printf '%s\n' "$out" | grep -qx 'G5D_PASST_BIN=/platform-input/passt' || fail "the container must be told the copied passt: $out"
printf '%s\n' "$out" | grep -qx 'G5D_GUEST_KERNEL=/platform-input/boot/vmlinux' || fail "the container must be told the copied kernel: $out"
for f in bin/cloud-hypervisor boot/vmlinux boot/initramfs.cpio boot/kernel.config boot/SHA256SUMS boot/versions.env passt; do
    [ -f "$INPUT_OUT/platform-input/$f" ] || fail "platform input lacks $f"
done
(cd "$INPUT_OUT/platform-input" && sha256sum --check --status boot/SHA256SUMS) || fail "the copied input must still verify against the host's digest list"
rm -rf "$INPUT_OUT"
# A workspace VM restages from the runtime disk's flat /opt/g5/platform, whose
# digest list names flat files: the copy must carry a regenerated host-layout
# list that the stager then verifies, and a tampered flat list must fail.
FLAT="$TMP/flat-platform"
mkdir -p "$FLAT"
cp "$TMP/stage-host/bin/cloud-hypervisor" "$FLAT/cloud-hypervisor"; cp "$TMP/stage-host/boot/vmlinux" "$FLAT/vmlinux"
cp "$TMP/stage-host/boot/initramfs.cpio" "$FLAT/initramfs.cpio"; cp "$TMP/stage-host/boot/kernel.config" "$FLAT/kernel.config"; cp "$TMP/stage-host/passt" "$FLAT/passt"
(cd "$FLAT" && sha256sum cloud-hypervisor vmlinux initramfs.cpio kernel.config passt > SHA256SUMS)
INPUT_OUT="$(mktemp -d)"
out="$( set -euo pipefail; OUT_DIR="$INPUT_OUT"; info() { :; }; fail() { echo "FAIL: $*"; exit 1; }; uname() { echo Linux; }
    export G5D_CLOUD_HYPERVISOR_BIN="$FLAT/cloud-hypervisor" G5D_GUEST_KERNEL="$FLAT/vmlinux" G5D_GUEST_INITRAMFS="$FLAT/initramfs.cpio" G5D_PASST_BIN="$FLAT/passt"
    eval "$block"; printf '%s\n' ${PLATFORM_MOUNTS[@]+"${PLATFORM_MOUNTS[@]}"} )" || true
printf '%s\n' "$out" | grep -q '^FAIL' && fail "the flat runtime-disk layout must stage: $out"
(cd "$INPUT_OUT/platform-input" && sha256sum --check --status boot/SHA256SUMS) || fail "the copied flat platform must carry a host-layout digest list"
mkdir -p "$INPUT_OUT/runtime2/bin"
env G5D_CLOUD_HYPERVISOR_BIN="$INPUT_OUT/platform-input/bin/cloud-hypervisor" G5D_GUEST_KERNEL="$INPUT_OUT/platform-input/boot/vmlinux" \
    G5D_GUEST_INITRAMFS="$INPUT_OUT/platform-input/boot/initramfs.cpio" G5D_PASST_BIN="$INPUT_OUT/platform-input/passt" \
    bash "$SCRIPT" "$INPUT_OUT/runtime2" >/dev/null || fail "the stager must accept the copied flat platform"
cmp -s "$FLAT/vmlinux" "$INPUT_OUT/runtime2/platform/vmlinux" || fail "the restaged kernel must be the runtime disk's"
rm -rf "$INPUT_OUT"
printf 'tampered' > "$FLAT/vmlinux"
INPUT_OUT="$(mktemp -d)"
out="$( set -euo pipefail; OUT_DIR="$INPUT_OUT"; info() { :; }; fail() { echo "FAIL: $*"; exit 1; }; uname() { echo Linux; }
    export G5D_CLOUD_HYPERVISOR_BIN="$FLAT/cloud-hypervisor" G5D_GUEST_KERNEL="$FLAT/vmlinux" G5D_GUEST_INITRAMFS="$FLAT/initramfs.cpio" G5D_PASST_BIN="$FLAT/passt"
    eval "$block" )" || true
printf '%s\n' "$out" | grep -q 'does not match its files' || fail "a flat digest list that disagrees with its files must fail the staging: $out"
rm -rf "$INPUT_OUT"
echo "✓ the runtime disk's flat platform restages with a regenerated host-layout digest list"

# Nothing on the machine: no bind, no input directory. The overrides point at
# absent fixture paths so the case does not depend on the machine lacking
# /opt/g5-host (the CI builder image and Linux development hosts have one).
INPUT_OUT="$(mktemp -d)"
out="$(run_input "$TMP/absent-platform" "$TMP/absent-platform/passt")" || fail "a machine without a platform must not fail the staging: $out"
[ -z "$out" ] && [ ! -e "$INPUT_OUT/platform-input" ] || fail "a machine without a platform must add no mount: $out"
rm -rf "$INPUT_OUT"
# A partial platform fails before the disk build.
make_platform "$TMP/stage-partial"; rm "$TMP/stage-partial/boot/vmlinux"
INPUT_OUT="$(mktemp -d)"
out="$(run_input "$TMP/stage-partial" "$TMP/stage-partial/passt")" || true
printf '%s\n' "$out" | grep -q 'FAIL: incomplete Cloud Hypervisor platform' || fail "a partial platform must fail the staging: $out"
rm -rf "$INPUT_OUT"
echo "✓ the staging container receives a copy of the platform under the output directory, all or none"
echo "✓ every runtime-disk producer stages the platform through build-runtime-disk.sh"
echo "stage-runtime-platform: OK"
