#!/usr/bin/env bash
# Executable contract for the fail-closed G5_WORKER_IMAGE chart guard (helm
# ontology D27, cloudbuild_pipelines D30/D31; PR #1885 review): renders the
# chart across the accept/reject value matrix so a future Helm/Sprig semantic
# change, template relocation, or values-merge change cannot silently bypass
# or over-trigger the guard. Run by the ci-rust `worker-image-guard-contract`
# step; also runnable locally (uses an installed helm when present, else
# downloads the same pinned, checksum-verified build the deploy pipeline
# installs — bump version/checksum together with infra/ci/cloudbuild-deploy.yaml).
set -euo pipefail
cd "$(dirname "$0")/.."
CHART=infra/helm/g5

HELM_VERSION="v3.21.4"
HELM_SHA256="61f88ab166748cb19604d7884cb100ae9ccb13804ddeb98e08af167eacbb6a14"
HELM_BIN="helm"
if ! command -v helm >/dev/null 2>&1; then
  source infra/packer/g5-worker/scripts/lib/download-with-retry.sh
  download_with_retry \
    "helm ${HELM_VERSION}" \
    "https://get.helm.sh/helm-${HELM_VERSION}-linux-amd64.tar.gz" \
    /tmp/helm.tar.gz
  echo "${HELM_SHA256}  /tmp/helm.tar.gz" | sha256sum -c - >/dev/null
  tar -xzf /tmp/helm.tar.gz -C /tmp linux-amd64/helm
  HELM_BIN=/tmp/linux-amd64/helm
fi

BASE=(--set namespace=g5 --set global.projectId=p --set global.clusterName=c
      --set global.registry=r --set global.imageTag=t)

render() {
  "${HELM_BIN}" template g5 "${CHART}" --values "${CHART}/values.yaml" \
    --values "${CHART}/values-control-plane.yaml" "$@" "${BASE[@]}" >/dev/null 2>&1
}

must_fail() {
  if render "$@"; then
    echo "FAIL: rendered, but must be rejected: $*" >&2
    exit 1
  fi
}

must_pass() {
  if ! render "$@"; then
    echo "FAIL: rejected, but must render: $*" >&2
    exit 1
  fi
}

CONCRETE="projects/g5-dev-1ccc/global/images/g5-worker-v20260814-1703-5f7fd69"

# Base values omit the key entirely (infra/self-hosted/local posture): renders.
must_pass

for overlay in values-qa.yaml; do
  ov=(--values "${CHART}/${overlay}")
  # Checked-in sentinel without pipeline injection: rejected.
  must_fail "${ov[@]}"
  # Present non-concrete values: rejected.
  must_fail "${ov[@]}" --set-string api.env.G5_WORKER_IMAGE=""
  must_fail "${ov[@]}" --set-string api.env.G5_WORKER_IMAGE="projects/g5-dev-1ccc/global/images/family/g5-worker"
  must_fail "${ov[@]}" --set-string 'api.env.G5_WORKER_IMAGE=projects/p/global/images/name?x=1'
  # Pipeline-injected concrete ref: renders.
  must_pass "${ov[@]}" --set-string api.env.G5_WORKER_IMAGE="${CONCRETE}"
done

HELM_BIN="${HELM_BIN}" python3 scripts/test_worker_host_identity.py

echo "worker-image guard matrix: all assertions passed"

# Companion contracts for the same deploy surface (cicd D27): the resolver's
# unit suite and the Cloud Build waitFor ordering check — no other CI step
# runs the scripts test files, and a forward waitFor reference is
# latent-until-first-run (Cloud Build rejects it only at build parse).
(cd scripts && python3 -m unittest -q test_resolve_worker_image)
(cd scripts && python3 -m unittest -q test_await_worker_image)
(cd scripts && python3 -m unittest -q test_cloudbuild_waitfor_order)
(cd scripts && python3 -m unittest -q test_deploy_queue_blockers)
bash scripts/test-check-main-freshness.sh
echo "resolver + worker-image gate unit suites, freshness orchestration, waitFor ordering: passed"
# Image-owned host paths must survive bootstrap metadata from an older API.
(cd scripts && python3 -m unittest -q test_worker_runtime_isolation)
