#!/usr/bin/env bash
# The input is the complete trusted runtime, not an OCI/customer filesystem.
set -euo pipefail
runtime=$(realpath "${1:?usage: build-runtime-disk.sh RUNTIME_DIRECTORY}")
repo=$(cd "$(dirname "$0")/.." && pwd)
python3 "$repo/scripts/stage-guest-bootstrap.py" "$runtime"
# Inner hosts (ontology g5.architecture.microvm_backend.nested): ship the host's
# own pinned VMM, kernel, initramfs, kernel config and passt under
# runtime/platform so a nested-enabled guest runs the unmodified vm-exec
# launch. Same pins as the host by construction; absent when this machine has
# no platform, which leaves process-mode development unchanged.
bash "$repo/scripts/stage-runtime-platform.sh" "$runtime"
rm -f "$runtime/runtime.ext4" "$runtime/runtime.ext4.sha256"
disk=$(mktemp "$(dirname "$runtime")/.runtime.ext4.XXXXXX")
native=$(mktemp /tmp/g5-runtime-exec.XXXXXX)
roundtrip=$(mktemp /tmp/g5-runtime-check.XXXXXX)
trap 'rm -f "$disk" "$native" "$roundtrip"' EXIT
# Patch only the guest disk's executable. The host-side bundle remains native
# to its build environment. Direct exec preserves /proc/self/exe for module
# supervision, Git helpers and CLI shims; invoking ld.so as the executable
# would make all those consumers re-exec the loader instead of g5d.
cp "$runtime/bin/g5d" "$native"
chmod 0755 "$native"
patchelf --set-interpreter /opt/g5/bootstrap/loader \
    --force-rpath --set-rpath /opt/g5/bootstrap/libs "$native"
# mkfs needs blocks for file contents plus inode/directory overhead; sparse output.
bytes=$(du --apparent-size --block-size=1 --summarize "$runtime" | cut -f1)
files=$(find "$runtime" -printf . | wc -c)
size=$(( (bytes * 12 / 10 + files * 8192 + 128 * 1024 * 1024 + 1048575) / 1048576 ))
truncate -s "${size}M" "$disk"
mkfs.ext4 -F -q -m 0 -N "$((files + 4096))" -E lazy_itable_init=0,lazy_journal_init=0 -d "$runtime" "$disk"
# debugfs only edits this pristine platform-built disk, never a guest-written
# filesystem. Verify its write explicitly: debugfs can report an error with a
# successful process status.
debugfs -w -R 'rm /bin/g5d' "$disk"
debugfs -w -R "write $native /bin/g5d" "$disk"
debugfs -R "dump /bin/g5d $roundtrip" "$disk"
cmp "$native" "$roundtrip"
# Host cache profile (ontology g5.runtime.cache.profile): the shared disk ships
# only an empty, world-readable placeholder at the guest path. The guest init
# binds the host's own profile from the per-VM bootstrap disk over it; a
# profile the building host may hold in the runtime directory is replaced
# here so it never travels in a shared image. The directory itself is left
# alone: process mode bind-mounts it and reads whatever the host wrote.
placeholder=$(mktemp /tmp/g5-runtime-profile.XXXXXX)
trap 'rm -f "$disk" "$native" "$roundtrip" "$placeholder"' EXIT
chmod 0644 "$placeholder"
debugfs -w -R 'rm /cache-profile.env' "$disk" 2>/dev/null || true
debugfs -w -R "write $placeholder /cache-profile.env" "$disk"
: > "$roundtrip"
debugfs -R "dump /cache-profile.env $roundtrip" "$disk"
[ ! -s "$roundtrip" ] || { echo "runtime disk must ship an empty cache profile placeholder" >&2; exit 1; }
debugfs -R 'stat /cache-profile.env' "$disk" 2>/dev/null | grep -q 'Mode:  0644' \
    || { echo "runtime disk cache profile placeholder must be world-readable" >&2; exit 1; }
e2fsck -f -n "$disk"
chmod 0444 "$disk"
sync -f "$disk"
mv "$disk" "$runtime/runtime.ext4"
(cd "$runtime" && sha256sum runtime.ext4 > runtime.ext4.sha256)
