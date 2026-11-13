#!/usr/bin/env bash
# Optional delivery cache preparation. Inputs contain OCI bytes and committed
# Git bundles only; no registry credentials or workspace runtime state.
set -euo pipefail

if [[ -z "${G5_WORKLOAD_IMAGE_REF:-}" ]]; then
    echo "[g5-worker] workload template preparation not configured"
    exit 0
fi
[[ "$G5_WORKLOAD_IMAGE_REF" =~ @sha256:[0-9a-f]{64}$ ]] || {
    echo "[g5-worker] workload environment must be pinned by digest" >&2
    exit 1
}
[[ "${G5_WORKLOAD_DISK_MIB:-}" =~ ^[1-9][0-9]*$ ]] || {
    echo "[g5-worker] workload disk capacity must be positive" >&2
    exit 1
}
input=${G5_WORKLOAD_INPUT_DIR:-/tmp/workload-template-input}
[[ -f "$input/image/oci-layout" && -f "$input/image/index.json" ]]
[[ "$(cat "$input/image-ref")" == "$G5_WORKLOAD_IMAGE_REF" ]]
export G5D_ROOTFS_STATE_DIR=${G5D_ROOTFS_STATE_DIR:-/var/lib/g5d/rootfs}
export G5D_OCI_CACHE_DIR=${G5D_OCI_CACHE_DIR:-/var/g5d-data/cache/oci-layers}
export LD_LIBRARY_PATH=/opt/g5-host/libs
export PATH=/opt/g5-host/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
args=(oci prepare-workload "$input/image" --size-mib "$G5_WORKLOAD_DISK_MIB")
if [[ -f "$input/repositories/seed.json" ]]; then
    args+=(--repo-seed-manifest "$input/repositories/seed.json")
fi
/opt/g5-host/bin/g5d "${args[@]}"
# Record provenance outside the runtime bundle; retain no delivery input bundles.
install -d -m 0755 /etc/g5
python3 - "$G5_WORKLOAD_IMAGE_REF" "$G5_WORKLOAD_DISK_MIB" "$input/repositories/seed.json" <<'PY'
import json, pathlib, sys
seed = pathlib.Path(sys.argv[3])
result = {"image": sys.argv[1], "size_mib": int(sys.argv[2]), "repo_seeded": seed.exists()}
if seed.exists():
    value = json.loads(seed.read_text())
    result["scope"] = value["scope"]
    result["commits"] = [{"repo_id": repo["repo_id"], "commit": repo["commit"]} for repo in value["repositories"]]
pathlib.Path('/etc/g5/workload-template.json').write_text(json.dumps(result, indent=2) + '\n')
PY
rm -rf -- "$input"
echo "[g5-worker] delivery workload templates prepared"
