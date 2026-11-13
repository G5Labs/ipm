#!/usr/bin/env bash
# Stage the host's pinned Cloud Hypervisor platform into RUNTIME/platform so
# an inner host inside a nested-enabled workspace VM can run the unmodified
# `g5d vm-exec` launch (ontology g5.architecture.microvm_backend.nested; design
# .g5/pr_ontology_diff_design/nested-virt-followup). Called by
# build-runtime-disk.sh, so every runtime disk (Packer image, local staging,
# CI fixture) carries the same files under the same pins as the host that
# built it: /opt/g5-host (or the G5D_* overrides) is the only source. A
# machine without the platform ships none, which leaves process-mode
# development unchanged; a partial platform fails the build.
set -euo pipefail
runtime=$(realpath "${1:?usage: stage-runtime-platform.sh RUNTIME_DIRECTORY}")
vmm=${G5D_CLOUD_HYPERVISOR_BIN:-/opt/g5-host/bin/cloud-hypervisor}
kernel=${G5D_GUEST_KERNEL:-/opt/g5-host/boot/vmlinux}
initramfs=${G5D_GUEST_INITRAMFS:-/opt/g5-host/boot/initramfs.cpio}
passt=${G5D_PASST_BIN:-$(command -v passt 2>/dev/null || echo /usr/bin/passt)}
# platform_kernel_config() reads kernel.config beside the kernel; keep it there.
config=$(dirname "$kernel")/kernel.config
destination="$runtime/platform"
rm -rf "$destination"
present=0
for file in "$vmm" "$kernel" "$initramfs" "$config" "$passt"; do
    [ -f "$file" ] && present=$((present + 1))
done
if [ "$present" = 0 ]; then
    echo "stage-runtime-platform: no Cloud Hypervisor platform on this machine; the runtime ships none (inner hosts need process mode)"
    exit 0
fi
if [ "$present" != 5 ]; then
    echo "stage-runtime-platform: incomplete platform: need $vmm $kernel $initramfs $config $passt" >&2
    exit 1
fi
# The host pipeline writes boot/SHA256SUMS relative to the platform root
# (build-cloud-hypervisor-runtime.sh). Verify against it when the layout is
# the host's; overrides pointing elsewhere are trusted inputs of the caller.
sums="$(dirname "$kernel")/SHA256SUMS"
if [ -f "$sums" ] && [ "$(basename "$(dirname "$kernel")")" = boot ]; then
    (cd "$(dirname "$kernel")/.." && sha256sum --check --status boot/SHA256SUMS) \
        || { echo "stage-runtime-platform: $sums does not match the platform files" >&2; exit 1; }
fi
install -d -m 0755 "$destination"
install -m 0755 "$vmm" "$destination/cloud-hypervisor"
install -m 0644 "$kernel" "$destination/vmlinux"
install -m 0644 "$initramfs" "$destination/initramfs.cpio"
install -m 0644 "$config" "$destination/kernel.config"
install -m 0755 "$passt" "$destination/passt"
grep -q '^CONFIG_SECURITY_LANDLOCK=y$' "$destination/kernel.config" \
    || echo "stage-runtime-platform: warning: this kernel has no Landlock; an inner host cannot start guests with it" >&2
versions="$(dirname "$kernel")/versions.env"
[ -f "$versions" ] && install -m 0644 "$versions" "$destination/versions.env"
(cd "$destination" && sha256sum cloud-hypervisor vmlinux initramfs.cpio kernel.config passt > SHA256SUMS)
echo "stage-runtime-platform: staged $(du -sh "$destination" | cut -f1) under $destination"
