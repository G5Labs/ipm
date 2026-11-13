#!/usr/bin/env bash
# Build once in the trusted host-image pipeline. Never runs customer code.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
source "$repo/infra/guest/versions.env"
output=${1:?usage: build-cloud-hypervisor-runtime.sh OUTPUT_DIRECTORY}
[ "$(uname -m)" = x86_64 ] || { echo 'The qualified cloud host architecture is x86_64' >&2; exit 1; }
mkdir -p "$output/bin" "$output/boot"
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT
fetch() {
    curl --fail --location --retry 4 --output "$2" "$1"
    echo "$3  $2" | sha256sum --check --status
}
fetch "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/$CLOUD_HYPERVISOR_VERSION/cloud-hypervisor-static" "$build/cloud-hypervisor" "$CLOUD_HYPERVISOR_X86_64_SHA256"
install -m 0755 "$build/cloud-hypervisor" "$output/bin/cloud-hypervisor"
fetch "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$GUEST_KERNEL_VERSION.tar.xz" "$build/linux.tar.xz" "$GUEST_KERNEL_SHA256"
tar -xf "$build/linux.tar.xz" -C "$build"
kernel="$build/linux-$GUEST_KERNEL_VERSION"
cp "$repo/infra/guest/kernel.config" "$build/kernel.config"
make -C "$kernel" KCONFIG_ALLCONFIG="$build/kernel.config" allnoconfig
# An unmet dependency must fail the image build, not the first Docker workload.
missing=0
install -m 0644 "$kernel/.config" "$output/boot/kernel.config"
while IFS= read -r setting; do
    [[ "$setting" = CONFIG_*=* ]] || continue
    grep -Fxq "$setting" "$kernel/.config" || { echo "Missing guest kernel setting: $setting" >&2; missing=1; }
done < "$build/kernel.config"
[ "$missing" = 0 ]
make -C "$kernel" -j "${G5_BUILD_JOBS:-$(nproc)}" vmlinux
install -m 0644 "$kernel/vmlinux" "$output/boot/vmlinux"
install -m 0644 "$kernel/.config" "$output/boot/kernel.config"
mkdir "$build/initramfs"
cc -static -Os -Wall -Wextra -Werror "$repo/infra/guest/init.c" -o "$build/initramfs/init"
(cd "$build/initramfs" && printf 'init\n' | cpio --quiet -o --format=newc) > "$output/boot/initramfs.cpio"
cp "$repo/infra/guest/versions.env" "$output/boot/versions.env"
(cd "$output" && sha256sum bin/cloud-hypervisor boot/vmlinux boot/initramfs.cpio boot/kernel.config > boot/SHA256SUMS)
"$output/bin/cloud-hypervisor" --version
