#!/usr/bin/env bash
# CI-only entrypoint for the `g5d host` workspace host container.
#
# Production worker hosts boot with an XFS-reflink ROOT filesystem (the
# autoinstall seed declares `fstype: xfs` + `-m reflink=1`, hard-verified by
# infra/packer/g5-worker/scripts/80-configure-host.sh), so `/var/g5d-data`
# inherits reflink there. In CI the host runs as a docker container whose
# `/var/g5d-data` would otherwise sit on the non-reflink overlay filesystem,
# and the workspace materializer is reflink-or-fail-loud (workspace D6,
# backend/g5d-foundation/src/workspace/materialize.rs::reflink_copy) — so every
# workspace-VM `cp --reflink=always` clone of the warm bare repo fails with
# "Operation not supported" and no microVM ever runs a real workload.
#
# This wrapper provisions a self-contained XFS-reflink loopback at
# `/var/g5d-data` (cache + workspaces roots share one device, as reflinks
# cannot cross filesystems), fails closed if reflink does not round-trip
# (mirroring the packer host-config check), then execs `g5d` with the args
# passed by the Cloud Build step (`host`). Requires the container to run
# `--privileged` (loop device + mount) with `xfsprogs` present in the image.
set -euo pipefail

# RAM admission must see all ancestor limits, but this CI worker may write
# only its own Docker cgroup. The launcher supplies the complete mount read-only.
[ -f /.dockerenv ] && [ "$(stat -fc %T /sys/fs/cgroup)" = cgroup2fs ] || {
    echo 'CI worker requires Docker and the complete cgroup-v2 hierarchy' >&2
    exit 1
}
CGROUP_RELATIVE="$(sed -n 's/^0:://p' /proc/self/cgroup)"
case "$CGROUP_RELATIVE" in
    /system.slice/docker-*.scope) CGROUP_ID="${CGROUP_RELATIVE#/system.slice/docker-}"; CGROUP_ID="${CGROUP_ID%.scope}" ;;
    /docker/*) CGROUP_ID="${CGROUP_RELATIVE#/docker/}" ;;
    *) echo 'Refusing a non-Docker cgroup or the host root' >&2; exit 1 ;;
esac
[[ "$CGROUP_ID" =~ ^[a-f0-9]{64}$ && "$CGROUP_ID" == "$HOSTNAME"* ]] || {
    echo 'Cgroup does not belong to this container' >&2; exit 1
}
[ ! -e /sys/fs/cgroup/cgroup.type ] || {
    echo 'CI memory accounting requires visible ancestors' >&2; exit 1
}
CGROUP_ROOT="/sys/fs/cgroup$CGROUP_RELATIVE"
# Make a separate mount before remounting: never make the host-wide bind writable.
mount --bind "$CGROUP_ROOT" "$CGROUP_ROOT"
mount -o remount,bind,rw "$CGROUP_ROOT"
mkdir "$CGROUP_ROOT/supervisor"
echo $$ > "$CGROUP_ROOT/supervisor/cgroup.procs"
echo '+cpu +memory +io +pids' > "$CGROUP_ROOT/cgroup.subtree_control"
# Privileged CI containers can hide the securityfs mount even when the host
# kernel enforces AppArmor. Expose its existing policy interface; never disable
# confinement or accept a parser failure.
if ! mountpoint -q /sys/kernel/security; then
    mount -t securityfs securityfs /sys/kernel/security
fi
apparmor_parser -r /etc/apparmor.d/usr.bin.passt
chgrp kvm /dev/kvm
chmod 0660 /dev/kvm

XFS_IMAGE="${G5D_HOST_XFS_IMAGE:-/var/g5d-data.xfs.img}"
XFS_MOUNT="${G5D_HOST_DATA_ROOT:-/var/g5d-data}"
XFS_SIZE_GB="${G5D_HOST_XFS_SIZE_GB:-10}"

echo "[ci-host-xfs] provisioning XFS reflink store: ${XFS_SIZE_GB}G at ${XFS_MOUNT} (image ${XFS_IMAGE})"
mkdir -p "${XFS_MOUNT}"

# Sparse backing file: truncate does not allocate blocks up front, so a
# generous logical size costs nothing until workspaces are materialized.
# `-f` forces mkfs onto the regular (non-block-device) image file — xfsprogs
# otherwise refuses a plain file (codex review).
truncate -s "${XFS_SIZE_GB}G" "${XFS_IMAGE}"
# Pin the on-disk feature set for the same reason as
# scripts/run-rust-xfs-orchestrator.sh: Ubuntu 26.04 ships xfsprogs 6.18, which
# defaults `rmapbt` and `nrext64` on. nrext64 needs a 5.19+ kernel, so on an
# older CI host mkfs succeeds and the subsequent mount fails with "wrong fs
# type, bad option, bad superblock". reflink stays on — it is the property this
# loopback exists to provide and is hard-verified below.
mkfs.xfs -f -q -m reflink=1,rmapbt=0 -i nrext64=0 "${XFS_IMAGE}"
mount -o loop "${XFS_IMAGE}" "${XFS_MOUNT}"

# Hard-verify reflink end to end (mirrors packer 80-configure-host.sh): the
# materializer is fail-loud, so prove `cp --reflink=always` round-trips before
# host startup rather than failing after a workspace is admitted.
probe_src="${XFS_MOUNT}/.reflink-probe.src"
probe_dst="${XFS_MOUNT}/.reflink-probe.dst"
echo "reflink-probe-$$" > "${probe_src}"
cp --reflink=always "${probe_src}" "${probe_dst}"
cmp "${probe_src}" "${probe_dst}"
rm -f "${probe_src}" "${probe_dst}"
echo "[ci-host-xfs] cp --reflink=always round-trip verified on ${XFS_MOUNT}"

# Keep the same canonical socket path as the image's narrow AppArmor policy.
# A bind mount retains XFS without introducing a second policy allowance.
mkdir -p "${XFS_MOUNT}/rootfs" /var/lib/g5d/rootfs
mount --bind "${XFS_MOUNT}/rootfs" /var/lib/g5d/rootfs
export G5D_ROOTFS_STATE_DIR=/var/lib/g5d/rootfs

exec /usr/local/bin/g5d "$@"
