#!/usr/bin/env bash
# Container build counterpart of the Packer platform stages. The VMM is Linux
# x86_64 only; native ARM development images retain explicit process mode.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
if [ "$(uname -m)" != x86_64 ]; then
    echo 'Cloud Hypervisor platform is unavailable on this architecture; explicit process development only'
    exit 0
fi
apt-get update -qq
apt-get install -y --no-install-recommends \
    gcc make bc bison flex libelf-dev libssl-dev cpio xz-utils curl python3 \
    e2fsprogs xfsprogs busybox-static iproute2 nftables apparmor
bash "$repo/scripts/build-cloud-hypervisor-runtime.sh" /opt/g5-host
G5_DOWNLOAD_LIB="$repo/infra/packer/g5-worker/scripts/lib/download-with-retry.sh" \
    G5_PASST_PROFILE_BUILD_ONLY=1 bash "$repo/infra/packer/g5-worker/scripts/58-install-passt.sh"
getent group kvm >/dev/null || groupadd --system kvm
getent group g5-vmm >/dev/null || groupadd --system g5-vmm
id g5-vmm >/dev/null 2>&1 || useradd --system --gid g5-vmm --no-create-home --shell /usr/sbin/nologin g5-vmm
usermod --append --groups kvm g5-vmm
rm -rf /var/lib/apt/lists/*
