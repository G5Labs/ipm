#!/usr/bin/env bash
# Delivery-only: prepare a generic environment without executing its image.
# Registry credentials are acquired by the existing G5 OCI client in memory;
# the exported payload contains only pristine filesystem bytes and identity.
set -euo pipefail
image=${1:?usage: publish-workload-template.sh PINNED_IMAGE G5D_BUILDER_IMAGE SIZE_MIB}
builder=${2:?builder image required}
size=${3:?capacity required}
[[ "$image" =~ @sha256:[0-9a-f]{64}$ ]] || { echo 'immutable source image required' >&2; exit 1; }
[[ "$size" =~ ^[1-9][0-9]*$ ]] || { echo 'positive MiB capacity required' >&2; exit 1; }
# Cloud Build shares /workspace with the Docker daemon; /tmp is not shared.
stage=$(mktemp -d "${G5_WORKLOAD_BUILD_ROOT:-/workspace}/g5-workload-delivery.XXXXXX")
trap 'rm -rf "$stage"' EXIT
# The build network supplies the BUILD service account to the existing GAR
# provider. No customer executable or hook runs in this container.
docker run --rm --privileged --user 0 --entrypoint bash \
    --network="${G5_TEMPLATE_BUILD_NETWORK:-cloudbuild}" \
    -v "$stage:/delivery" -e SOURCE_IMAGE="$image" -e DISK_MIB="$size" "$builder" -ec '
    set -euo pipefail
    truncate -s "$((DISK_MIB * 4 + 8192))M" /delivery/cache.xfs
    mkfs.xfs -f -q -m reflink=1,rmapbt=0 -i nrext64=0 /delivery/cache.xfs
    mkdir /state
    mount -o loop /delivery/cache.xfs /state
    trap "umount /state" EXIT
    export G5D_ROOTFS_STATE_DIR=/state/rootfs G5D_OCI_CACHE_DIR=/state/oci
    g5d oci prepare-workload "$SOURCE_IMAGE" --size-mib "$DISK_MIB" --export-artifact /delivery/artifact
    '
reference=$(cat "$stage/artifact/reference.txt")
docker build --tag "$reference" "$stage/artifact"
docker push "$reference"
printf 'Published generic workload companion: %s\n' "$reference"
