#!/usr/bin/env bash
# Strict CI KVM gate, including the retained real guest network boundaries.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
: "${G5D_BIN:?candidate binary required}"
if [ "${1:-}" != --isolated ]; then
    exec unshare --mount -- bash "$0" --isolated
fi
mount --make-rprivate /
test -c /dev/kvm
root=${G5_CH_TEST_ROOT:-/var/lib/g5d/rootfs}
mkdir -p "$root"
store=$(mktemp /tmp/g5-ch-ci.XXXXXX.xfs)
trap 'umount "$root" 2>/dev/null || true; rm -f "$store"' EXIT
# Sparse: only written blocks cost disk. 16G admits the 3 GiB Docker fixture
# (its template, the alternate-image reflink and three private clones with
# their bootstrap headroom); 8G refused the third boot (#914, build 782a7179).
# The literal #1049 cold-image reproduction grows it to a default one-slot
# host's 200 GiB after the scaled case, so its pressure file stays small.
truncate -s 16G "$store"
mkfs.xfs -f -q -m reflink=1,rmapbt=0 -i nrext64=0 "$store"
mount -o loop "$store" "$root"
# Load the image's narrow passt profile into the isolated CI host kernel.
# Privileged CI containers can hide the securityfs mount even when the host
# kernel enforces AppArmor. Expose its existing policy interface; never disable
# confinement or accept a parser failure.
if ! mountpoint -q /sys/kernel/security; then
    mount -t securityfs securityfs /sys/kernel/security
fi
apparmor_parser -r /etc/apparmor.d/usr.bin.passt
chgrp kvm /dev/kvm
chmod 0660 /dev/kvm
runtime_args=()
if [ -n "${G5_CH_TEST_RUNTIME:-}" ]; then runtime_args=(--runtime "$G5_CH_TEST_RUNTIME"); fi
# The minimal fixture copies the runner's Python with its system library paths.
# A relocated guest-bundle Python can use ORIGIN-relative libraries and is not
# a system fixture input even when it appears first on the caller's PATH.
/usr/bin/python3 "$repo/scripts/prepare-cloud-hypervisor-test.py" --root "$root" --binary "$G5D_BIN" --docker "${runtime_args[@]}"
source "$root/test.env"
size=$G5_TEST_DISK_MIB
# The pinned AppArmor rule allows exactly rootfs/<owned VM>/net. Keep test
# roots in that same shape instead of widening the runtime policy.
export G5_TEST_VM_ROOT="$root"
# Every production boot restores source in the guest. Include seed hits and
# ordinary cold/warm boots, with exact dirty receipts and less than 2 * V free.
/usr/bin/python3 "$repo/scripts/test_cloud_hypervisor_seed.py"
if [ "${G5_TEST_QA_DISK_INCIDENT:-0}" = 1 ]; then
    truncate -s 200G "$store"
    losetup --set-capacity "$(findmnt -n -o SOURCE --target "$root")"
    xfs_growfs "$root"
    /usr/bin/python3 "$repo/scripts/test_cloud_hypervisor_seed.py" --incident
fi
owned=$(mktemp -d "$root/smoke.XXXXXX")
work=$(mktemp -d "$root/work.XXXXXX")
mkdir -p "$owned/rootfs/.g5"
cp "$repo/scripts/test-guest-init-mount-roots.py" "$work/mount-roots.py"
cp "$repo/scripts/test-guest-docker-exec.sh" "$work/docker-exec-check.sh"
# Nested virtualization (g5.architecture.microvm_backend.nested): the first boot
# runs with the switch on. The guest must show VMX, a world-usable /dev/kvm and
# Docker device access (hard). It then acts as an inner host: Landlock, the
# platform passt and egress policy from the runtime disk's /opt/g5/platform and
# a loop-mounted XFS reflink root are hard checks, and a third-level boot goes
# through the in-guest `g5d vm-exec` with a template prepared from the same
# fixture layout (report-only unless G5_NESTED_BOOT_STRICT=1, except that a
# Landlock or passt failure is always hard; see the probe script).
cp "$repo/scripts/test-guest-nested-kvm.py" "$work/nested-kvm.py"
mkdir -p "$work/nested"
cp -a "$G5_TEST_OCI_LAYOUT" "$work/nested/fixture-oci"
# The inner guest's runtime is the minimal fixture runtime, never the guest's
# own (a Packer gate's baked runtime is several GB).
mkdir -p "$work/nested/runtime"
install -m 0644 "$G5_TEST_INNER_RUNTIME/runtime.ext4" "$work/nested/runtime/runtime.ext4"
test -s "$G5_TEST_RUNTIME/platform/cloud-hypervisor" || { echo "runtime disk input ships no platform: $G5_TEST_RUNTIME/platform" >&2; exit 1; }
# Host cache profile (ontology g5.runtime.cache.profile): the fixture runtime
# directory publishes one exactly as host bring-up does, and the guest must
# read it byte for byte at the runtime path. A caller-supplied runtime (the
# Packer gate's extracted baked runtime, mounted read-only) proves instead that
# the shipped disk carries the empty placeholder the init binds over.
if [ -z "${G5_CH_TEST_RUNTIME:-}" ]; then
    printf 'G5_CACHE_DOOR=http://g5-host:7111\nG5_BASE_IMAGE_MIRROR_TIERS=base base-rust base-rust-warm base-node\n' > "$G5_TEST_RUNTIME/cache-profile.env"
    chmod 0644 "$G5_TEST_RUNTIME/cache-profile.env"
fi
if [ -f "$G5_TEST_RUNTIME/cache-profile.env" ]; then
    cp "$G5_TEST_RUNTIME/cache-profile.env" "$work/expected-cache-profile"
else
    : > "$work/expected-cache-profile"
fi
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs" G5D_INTERNAL_WORKLOAD_TEMPLATE="$G5_TEST_TEMPLATE"
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 2 --mem-mib 3072 --scratch-disk-mib "$size" --block-workload \
    --nested-virtualization \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -e G5_SMOKE_ENV=roundtrip \
    -e "G5_NESTED_BOOT_STRICT=${G5_NESTED_BOOT_STRICT:-0}" \
    -e "G5D_WORKSPACE_HOSTNAME=Smoke Workspace/685" -- /bin/sh -ec \
    'grep " / ext4 " /proc/mounts
     test -r /opt/g5/bin/g5d
     test "$G5_SMOKE_ENV" = roundtrip
     test "$(cat /proc/sys/kernel/hostname)" = smoke-workspace-685
     test "$(cat /etc/hostname)" = smoke-workspace-685
     grep -q "^127.0.1.1 smoke-workspace-685$" /etc/hosts
     if echo forbidden > /opt/g5/probe 2>/dev/null; then exit 1; fi
     test -f /opt/g5/cache-profile.env
     test "$(cat /opt/g5/cache-profile.env)" = "$(cat /work/expected-cache-profile)"
     if echo forbidden >> /opt/g5/cache-profile.env 2>/dev/null; then exit 1; fi
     timeout -k 5 60 /usr/bin/python3 /work/mount-roots.py
     timeout -k 5 60 /usr/bin/python3 /work/mount-roots.py --without-profile
     timeout -k 5 60 /usr/bin/python3 /work/mount-roots.py --legacy
     timeout -k 5 30 /usr/bin/python3 /work/nested-kvm.py --expect-present
     G5_TEST_EXPECT_KVM=1 G5_TEST_START_DOCKER=1 timeout -k 5 180 /bin/bash /work/docker-exec-check.sh
     timeout -k 5 420 /usr/bin/python3 /work/nested-kvm.py --vm-exec /work/nested
     echo CH_BOOT_OK'
# A different workspace environment can be admitted on the same host. Exercise
# the cold prepare-and-clone path; the following original-image boot switches back.
original_template=$G5_TEST_TEMPLATE
alternate_template="$(dirname "$original_template")/$(printf '%064d' 1)"
mkdir -p "$alternate_template"
cp -a --reflink=always "$original_template/rootfs" "$alternate_template/rootfs"
printf 'alternate-image' > "$alternate_template/rootfs/image-id"
cp "$original_template/.complete" "$alternate_template/.complete"
rm -rf "$owned"
owned=$(mktemp -d "$root/alternate.XXXXXX")
mkdir -p "$owned/rootfs/.g5"
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs" G5D_INTERNAL_WORKLOAD_TEMPLATE="$alternate_template"
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --scratch-disk-mib "$size" --prepare-private-workload
test -s "$owned/rootfs/.g5/prepared-workload.ext4"
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 1 --mem-mib 512 --scratch-disk-mib "$size" --block-workload \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- \
    /bin/sh -ec 'test "$(cat /image-id)" = alternate-image
     timeout -k 5 30 /usr/bin/python3 /work/nested-kvm.py --expect-absent
     echo CH_ALTERNATE_IMAGE_OK'
export G5D_INTERNAL_WORKLOAD_TEMPLATE="$original_template"
# A completed guest root is disposable, never a second boot input. The runtime
# deliberately refuses to overwrite its previous input disk/ownership files.
rm -rf "$owned"
owned=$(mktemp -d "$root/exit.XXXXXX")
mkdir -p "$owned/rootfs/.g5"
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs"
set +e
"$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 1 --mem-mib 512 --scratch-disk-mib "$size" --block-workload \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- /bin/sh -c 'exit 17'
status=$?
set -e
[ "$status" = 17 ] || { echo "Guest exit status lost: $status" >&2; exit 1; }
rm -rf "$owned"
owned=$(mktemp -d "$root/reboot.XXXXXX")
mkdir -p "$owned/rootfs/.g5"
export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs"
set +e
timeout 90 "$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 1 --mem-mib 512 --scratch-disk-mib "$size" --block-workload \
    -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- /bin/sh -ec \
    'echo G5_FIRST_BOOT; echo unsaved > /work/reboot-probe; /opt/g5/bootstrap/busybox reboot -f; sleep 30' \
    > "$root/reboot.log" 2>&1
status=$?
set -e
cat "$root/reboot.log"
[ "$status" -ne 0 ] && [ "$status" -ne 124 ] || { echo "Reboot was not refused: $status" >&2; exit 1; }
[ "$(grep -c '^G5_FIRST_BOOT' "$root/reboot.log")" = 1 ]
grep -q 'reboot: Restarting system' "$root/reboot.log"
# Either the persistent bootstrap marker refuses replay, or passt --one-off
# exits on reset and the supervisor terminates the VMM first. Both must refuse
# the reboot without rerunning the workload or leaving disk-owning children.
# The marker's refusal before workspace mutation is also covered independently
# by vm_init::tests::reboot_refuses_bootstrap_before_overwriting_guest_changes.
grep -Eq 'guest bootstrap.*File exists|VM network backend exited: exit status: 0' "$root/reboot.log"
/usr/bin/python3 - "$owned" <<'PY'
import os
from pathlib import Path
import sys

owned = os.fsencode(sys.argv[1] + "/")
remaining = []
for process in Path("/proc").glob("[0-9]*"):
    if process.name == str(os.getpid()):
        continue
    try:
        command = (process / "cmdline").read_bytes()
    except (FileNotFoundError, ProcessLookupError):
        continue
    if owned in command:
        remaining.append((process.name, command.replace(b"\0", b" ").decode(errors="replace")))
assert not remaining, f"Reboot left disk/network owners alive: {remaining}"
print("CH_REBOOT_REFUSAL_AND_CHILD_EXIT_OK")
PY
rm -rf "$owned" "$work"
bash "$repo/scripts/test-cloud-hypervisor-sandbox.sh"
bash "$repo/scripts/test-cloud-hypervisor-network.sh"
if [ -n "${G5_CH_TEST_RUNTIME:-}" ]; then
    owned=$(mktemp -d "$root/runtime.XXXXXX")
    work=$(mktemp -d "$root/runtime-work.XXXXXX")
    mkdir -p "$owned/rootfs/.g5"
    export G5D_INTERNAL_OWNED_WORKLOAD_ROOT="$owned/rootfs"
    "$G5D_BIN" vm-exec --rootfs "$owned/rootfs" --cpus 2 --mem-mib 2048 --scratch-disk-mib "$size" --block-workload \
        -V "$work:/work" -V "$G5_TEST_RUNTIME:/opt/g5:ro" -- /bin/sh -ec '
        export PATH=/opt/g5/bin:/bin:/usr/bin HOME=/root LD_LIBRARY_PATH=/opt/g5/bin
        export SSL_CERT_FILE=/opt/g5/bin/ca-certificates.crt CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK=1
        test -s "$SSL_CERT_FILE"
        for tool in g5d node python3 claude codex gh gcloud gke-gcloud-auth-plugin npm pnpm corepack cargo rustc cargo-nextest just; do
            "$tool" --version
        done
        kubectl version --client=true
        cargo nextest --version
        echo A25_VM_SELFCONTAINED_OK'
    rm -rf "$owned" "$work"
fi
echo 'Cloud Hypervisor strict boot/network gate: PASS'
