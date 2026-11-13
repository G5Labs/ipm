#!/usr/bin/env bash
# Give a local microvm host a reflink-capable directory on a machine whose
# filesystem has none (a G5 workspace VM: one ext4 workload disk). A microvm
# host refuses admission without reflinks (workload_template::prepare), so the
# full-stack preview mounts a sparse XFS image through loop and points
# G5_DEV_HOST_DIR at it (ontology g5.runtime.boot.backend.process-local;
# design .g5/pr_ontology_diff_design/nested-virt-followup §3.5). The image
# lives outside /work on purpose: /work is the archived workspace tree. It is
# ephemeral with the VM and reused across preview opens in the same VM, so
# the staged guest runtime under it survives a Close.
#
# Usage: dev-host-reflink-root.sh [--status]
#   G5_DEV_HOST_REFLINK_IMAGE  backing file  (default /var/lib/g5-dev-host/root.xfs)
#   G5_DEV_HOST_REFLINK_ROOT   mount point   (default /var/lib/g5-dev-host/root)
#   G5_DEV_HOST_REFLINK_GIB    image size cap (default 48; also capped at 80% of free space
#                              and at a file-size limit this process cannot lift)
#   G5_DEV_HOST_REFLINK_OWNER  uid:gid owning the mount point (default the invoking user)
# Re-executes itself through sudo when not root; needs mkfs.xfs (xfsprogs),
# loop devices and XFS in the running kernel.
set -euo pipefail
image=${G5_DEV_HOST_REFLINK_IMAGE:-/var/lib/g5-dev-host/root.xfs}
root=${G5_DEV_HOST_REFLINK_ROOT:-/var/lib/g5-dev-host/root}
cap_gib=${G5_DEV_HOST_REFLINK_GIB:-48}
owner=${G5_DEV_HOST_REFLINK_OWNER:-${SUDO_UID:-$(id -u)}:${SUDO_GID:-$(id -g)}}
fail() { echo "✗ dev-host-reflink-root: $*" >&2; exit 1; }
case "${1:-}" in
    --status)
        if mountpoint -q "$root" && [ "$(findmnt -n -o FSTYPE "$root")" = xfs ]; then
            echo "mounted $root ($(findmnt -n -o SOURCE "$root"))"; exit 0
        fi
        echo "not mounted: $root"; exit 1 ;;
    "") ;;
    *) fail "unknown argument $1" ;;
esac
[ "$(uname -s)" = Linux ] || fail "Linux only"
if [ "$(id -u)" != 0 ]; then
    exec sudo -n G5_DEV_HOST_REFLINK_IMAGE="$image" G5_DEV_HOST_REFLINK_ROOT="$root" \
        G5_DEV_HOST_REFLINK_GIB="$cap_gib" G5_DEV_HOST_REFLINK_OWNER="$owner" "$0" "$@"
fi
# The agent harness bounds the file size of its whole process tree
# (g5d-dispatch's module envelope: 4 GiB for a preview's processes, 16 GiB for
# an agent turn's, kept by sudo), so growing a larger sparse image dies with
# SIGXFSZ (exit 153). Root
# lifts the bound for this process; anyone else gets the hard limit and the
# image is sized within it below.
ulimit -f unlimited 2>/dev/null || ulimit -S -f "$(ulimit -H -f)" 2>/dev/null || true
command -v mkfs.xfs >/dev/null || fail "mkfs.xfs missing (install xfsprogs)"
grep -qw xfs /proc/filesystems || modprobe xfs 2>/dev/null || fail "the running kernel has no XFS"
mkdir -p "$root" "$(dirname "$image")"
if mountpoint -q "$root"; then
    # A foreign mount is refused untouched: its ownership is not ours to change.
    [ "$(findmnt -n -o FSTYPE "$root")" = xfs ] || fail "$root is mounted but not XFS"
else
    # The owner gets the empty mount point before anything that can fail: the
    # dev recipes create the host's directories under it as that user, and a
    # stop after a failed preparation must not trip on a root-owned directory.
    chown "$owner" "$root"
    if [ ! -f "$image" ]; then
        # Sparse: blocks are allocated as the host writes. Cap by the backing
        # filesystem's free space so the host's own capacity checks see a
        # bound the disk can honour.
        free_kib=$(df -Pk "$(dirname "$image")" | awk 'NR == 2 {print $4}')
        max_gib=$(( free_kib * 8 / 10 / 1024 / 1024 ))
        [ "$max_gib" -ge 8 ] || fail "only ${max_gib} GiB free under $(dirname "$image"); need at least 8"
        fsize_kib=$(ulimit -f)
        if [ "$fsize_kib" != unlimited ]; then
            limit_gib=$(( fsize_kib / 1024 / 1024 ))
            [ "$limit_gib" -ge 8 ] || fail "this process may not write files above ${limit_gib} GiB (ulimit -f) and cannot lift the limit; need at least 8"
            [ "$limit_gib" -ge "$max_gib" ] || max_gib=$limit_gib
        fi
        size_gib=$cap_gib
        [ "$size_gib" -le "$max_gib" ] || size_gib=$max_gib
        # A failed creation leaves no half-made image behind for the next run.
        trap 'rm -f "$image.tmp"' EXIT
        truncate -s "${size_gib}G" "$image.tmp"
        mkfs.xfs -q -m reflink=1 "$image.tmp"
        mv "$image.tmp" "$image"
        trap - EXIT
        echo "→ created ${size_gib} GiB sparse XFS image $image"
    fi
    # Close the writable setup FD before mounting. Older libmount's combined
    # `mount -o loop` conflicts with kernels that block raw writes to mounted
    # devices, including our minimal guest kernel.
    loop=$(losetup --find --show "$image") || fail "cannot allocate loop device for $image"
    if mount -t xfs -o noatime "$loop" "$root"; then
        # Linux keeps a mounted loop alive and clears it when unmounted.
        losetup --detach "$loop" || fail "cannot mark $loop for cleanup on unmount"
    else
        mount_status=$?
        losetup --detach "$loop" || echo "Loop cleanup also failed: $loop" >&2
        fail "cannot mount $image on $root (mount exit $mount_status)"
    fi
fi
chown "$owner" "$root"
chmod 0755 "$root"
# Prove the property the host will require, as the host does at admission.
probe=$(mktemp "$root/.reflink-probe.XXXXXX")
printf probe > "$probe"
cp --reflink=always "$probe" "$probe.clone" || { rm -f "$probe"; fail "$root does not support reflinks"; }
rm -f "$probe" "$probe.clone"
echo "✓ reflink-capable host root at $root ($(df -Ph "$root" | awk 'NR == 2 {print $4}') free)"
