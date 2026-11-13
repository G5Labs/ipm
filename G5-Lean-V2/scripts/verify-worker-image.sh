#!/usr/bin/env bash
# =============================================================================
# verify-worker-image.sh — verify a G5 worker image's signature, SBOM, and
# raw-disk integrity end-to-end.
#
# Operator / PNC-customer verification contract:
#
#   ./verify-worker-image.sh <image-version>
#     # e.g. g5-worker-v20260527-1830-abc1234
#
# What it checks (in order — fails fast on the first issue):
#
#   1. The metadata OCI artifact exists in Google Artifact Registry at the
#      expected ref, and `cosign verify` succeeds against the configured
#      cosign verification key.
#   2. The SBOM attestation (`type=spdxjson`) is present on the same OCI ref
#      and is signed by the same key.
#   3. The raw-disk tarball at the GCS URI matches the sha256 carried by
#      the SIGNED disk-pointer.json pulled from the GAR ref (not the
#      unsigned .tar.gz.sha256 sidecar in GCS — bucket-write would defeat
#      that). The signed value comes from the GAR ref already verified
#      in step 1, so step 3 is end-to-end content-binding.
#   4. summary.json's recorded tarball_sha256 matches what we just computed
#      (defense in depth against an attacker who got past step 3).
#
# Environment:
#   PROJECT_ID            GCP project hosting the registry (required)
#   AR_REGION             Artifact Registry region — default us-central1
#   AR_REPOSITORY         Registry name — default g5-worker-images
#   IMAGE_FAMILY          GCE image family / GAR tag prefix — default g5-worker
#   COSIGN_KEY            cosign --key value. Either a gcpkms:// URI or a
#                         path to an exported PEM. Required.
#   IMAGES_BUCKET         GCS bucket — default ${PROJECT_ID}-g5-worker-images
#   WORK_DIR              Scratch directory — default $(mktemp -d)
#
# Required tooling: cosign 2.x, oras 1.x, gsutil (gcloud SDK), sha256sum, jq.
#
# Air-gapped customers: pre-export the cosign public key as PEM, mirror the
# GCS bucket to an offline location (or use an air-gap GCS gateway), and run
# the script with COSIGN_KEY=/path/to/cosign.pub and IMAGES_BUCKET pointing
# at the offline mirror. The pipeline runs `cosign sign --tlog-upload=false`
# so verification with `--insecure-ignore-tlog` is consistent — no Sigstore
# public-good network access is required.
# =============================================================================
set -euo pipefail

IMAGE_VERSION="${1:?usage: $0 <image-version> (e.g. g5-worker-v20260527-1830-abc1234)}"
PROJECT_ID="${PROJECT_ID:?PROJECT_ID is required}"
ARTIFACT_PROJECT="${ARTIFACT_PROJECT:-${PROJECT_ID}}"
AR_REGION="${AR_REGION:-us-central1}"
AR_REPOSITORY="${AR_REPOSITORY:-g5-worker-images}"
IMAGE_FAMILY="${IMAGE_FAMILY:-g5-worker}"
COSIGN_KEY="${COSIGN_KEY:?COSIGN_KEY is required (gcpkms:// URI or path to PEM)}"
IMAGES_BUCKET="${IMAGES_BUCKET:-${ARTIFACT_PROJECT}-g5-worker-images}"
WORK_DIR="${WORK_DIR:-$(mktemp -d -t g5-verify-XXXX)}"

GAR_REPOSITORY="${AR_REGION}-docker.pkg.dev/${ARTIFACT_PROJECT}/${AR_REPOSITORY}/${IMAGE_FAMILY}"
GCS_PREFIX="gs://${IMAGES_BUCKET}/${IMAGE_VERSION}"

info() { printf '==> %s\n' "$*"; }
ok()   { printf 'OK : %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

command -v cosign    >/dev/null || fail "cosign not on PATH — install from https://github.com/sigstore/cosign/releases"
command -v gsutil    >/dev/null || fail "gsutil not on PATH — install Google Cloud SDK"
command -v sha256sum >/dev/null || fail "sha256sum not on PATH"
command -v jq        >/dev/null || fail "jq not on PATH"
command -v oras      >/dev/null || fail "oras not on PATH — install from https://github.com/oras-project/oras/releases"

# New releases publish OCI metadata by digest. Existing summaries still name
# their original version tags; preserve both formats without rewriting history.
gsutil cp "${GCS_PREFIX}/summary.json" "${WORK_DIR}/summary.json"
GAR_REF="$(jq -er '.gar_ref' "${WORK_DIR}/summary.json")"
if [[ "${GAR_REF}" != "${GAR_REPOSITORY}:${IMAGE_VERSION}" ]]; then
    GAR_DIGEST="${GAR_REF#"${GAR_REPOSITORY}@sha256:"}"
    [[ "${GAR_REF}" == "${GAR_REPOSITORY}@sha256:"* && "${GAR_DIGEST}" =~ ^[0-9a-f]{64}$ ]] \
        || fail "summary metadata reference is outside the selected artifact repository/version"
fi

info "Image version : ${IMAGE_VERSION}"
info "GAR reference : ${GAR_REF}"
info "GCS prefix    : ${GCS_PREFIX}"
info "Cosign --key  : ${COSIGN_KEY}"
info "Workdir       : ${WORK_DIR}"

# -----------------------------------------------------------------------------
# 1. Cosign-verify the GAR OCI metadata artifact.
# -----------------------------------------------------------------------------
info "[1/4] cosign verify (signature)"
# --insecure-ignore-tlog because the pipeline runs --tlog-upload=false
# (air-gap posture).
cosign verify \
    --key "${COSIGN_KEY}" \
    --insecure-ignore-tlog \
    "${GAR_REF}" \
    > "${WORK_DIR}/cosign-verify.json" \
    || fail "cosign verify failed for ${GAR_REF}"
ok "image signature valid"

# -----------------------------------------------------------------------------
# 2. Cosign-verify the SPDX SBOM attestation.
# -----------------------------------------------------------------------------
info "[2/4] cosign verify-attestation (SPDX SBOM)"
cosign verify-attestation \
    --key "${COSIGN_KEY}" \
    --type spdxjson \
    --insecure-ignore-tlog \
    "${GAR_REF}" \
    > "${WORK_DIR}/cosign-attestation.jsonl" \
    || fail "cosign verify-attestation failed for ${GAR_REF}"
ok "SBOM attestation valid"

# -----------------------------------------------------------------------------
# 3. Tarball sha256 matches the SIGNED value in disk-pointer.json.
#
# Codex review B1: previously this step trusted the GCS-side unsigned
# .tar.gz.sha256 sidecar. That meant a writer with bucket-write could swap
# BOTH the tarball and the sidecar and verification would still pass. Now
# we pull the trusted sha256 from disk-pointer.json — which is bound to
# the cosign signature we already verified in step 1 — and compare the
# downloaded tarball bytes against that.
# -----------------------------------------------------------------------------
info "[3/4] tarball sha256 matches signed digest (disk-pointer.json)"
gsutil cp "${GCS_PREFIX}/manifest.json"                     "${WORK_DIR}/manifest.json"
gsutil cp "${GCS_PREFIX}/${IMAGE_VERSION}.tar.gz"           "${WORK_DIR}/${IMAGE_VERSION}.tar.gz"

# Pull disk-pointer.json from the SIGNED GAR artifact, not GCS.
mkdir -p "${WORK_DIR}/oras-pull"
( cd "${WORK_DIR}/oras-pull" && oras pull "${GAR_REF}" >/dev/null ) \
    || fail "oras pull failed for ${GAR_REF}"

signed_sha="$(jq -r '.tarball_sha256' "${WORK_DIR}/oras-pull/disk-pointer.json")"
signed_version="$(jq -r '.image_version' "${WORK_DIR}/oras-pull/disk-pointer.json")"
[ "${signed_version}" = "${IMAGE_VERSION}" ] || fail "signed metadata belongs to another worker version"
actual_sha="$(sha256sum "${WORK_DIR}/${IMAGE_VERSION}.tar.gz" | awk '{print $1}')"

[ -n "${signed_sha}" ] && [ "${signed_sha}" != "null" ] \
    || fail "disk-pointer.json missing tarball_sha256 (old-format signature?)"

if [ "${signed_sha}" != "${actual_sha}" ]; then
    fail "tarball sha256 mismatch — signed=${signed_sha} actual=${actual_sha}"
fi
ok "tarball sha256 matches signed digest from disk-pointer.json"

# -----------------------------------------------------------------------------
# 4. summary.json's tarball_sha256 matches what we computed (defense in depth).
# -----------------------------------------------------------------------------
info "[4/4] summary.json tarball_sha256 matches"
summary_sha="$(jq -r '.tarball_sha256' "${WORK_DIR}/summary.json")"
if [ "${summary_sha}" != "${actual_sha}" ]; then
    fail "summary.json tarball_sha256 mismatch — summary=${summary_sha} actual=${actual_sha}"
fi
ok "summary.json tarball_sha256 matches"

info "verification PASSED for ${IMAGE_VERSION}"
info "  signed-by : ${COSIGN_KEY}"
info "  raw-disk  : ${GCS_PREFIX}/${IMAGE_VERSION}.tar.gz"
info "  sbom      : ${GCS_PREFIX}/sbom.spdx.json"
info "  manifest  : ${GCS_PREFIX}/manifest.json"
info "  gar-ref   : ${GAR_REF}"
