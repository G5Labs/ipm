#!/usr/bin/env bash
set -euo pipefail

WORKSPACE_DIR="${G5_CI_WORKSPACE_DIR:-/workspace}"
GENERATION_DIR="${G5_CI_GENERATION_DIR:-${WORKSPACE_DIR}}"
ARTIFACT_ROOT="${G5_CI_RELEASE_ARTIFACTS_ROOT:-${GENERATION_DIR}/.ci-release-artifacts}"
API_ARTIFACT_DIR="${G5_CI_API_RELEASE_ARTIFACTS_DIR:-${G5_CI_RELEASE_ARTIFACTS_DIR:-${ARTIFACT_ROOT}/api}}"
WORKER_ARTIFACT_DIR="${G5_CI_WORKER_RELEASE_ARTIFACTS_DIR:-${ARTIFACT_ROOT}/worker}"

export CARGO_HOME="${CARGO_HOME:-/opt/g5-cargo-home}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
export PATH="${CARGO_HOME}/bin:${PATH}"
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

cd "${GENERATION_DIR}"

clean_baked_artifacts() {
  case "${G5_CI_SKIP_BAKED_ARTIFACT_CLEAN:-false}" in
    1|true|TRUE|yes|YES)
      echo "Skipping baked local workspace artifact cleanup for release artifact build"
      ;;
    *)
      bash ./scripts/clean-baked-rust-workspace-artifacts.sh
      ;;
  esac
}

verify_self_contained_g5d() {
  local binary="$1"
  local report="$2"

  bash ./scripts/assert-g5d-self-contained.sh "${binary}" "${report}"
}

build_api_artifacts() {
  echo "=== Rust release artifacts: cargo build --release -p g5-api -p g5d ==="
  cargo build --release --locked -p g5-api -p g5d

  local api_tmp_dir="${API_ARTIFACT_DIR}.tmp"
  rm -rf "${api_tmp_dir}" "${API_ARTIFACT_DIR}"
  mkdir -p "${api_tmp_dir}"

  install -m 0755 "${CARGO_TARGET_DIR}/release/g5-api" "${api_tmp_dir}/g5-api"
  install -m 0755 "${CARGO_TARGET_DIR}/release/g5d" "${api_tmp_dir}/g5d"
  verify_self_contained_g5d "${api_tmp_dir}/g5d" "${api_tmp_dir}/g5d.ldd"

  local sqlx_bin
  sqlx_bin="$(command -v sqlx || true)"
  if [[ -z "${sqlx_bin}" ]]; then
    echo "ERROR: sqlx binary not found in PATH" >&2
    exit 2
  fi
  install -m 0755 "${sqlx_bin}" "${api_tmp_dir}/sqlx"

  (
    cd "${api_tmp_dir}"
    sha256sum g5-api g5d sqlx g5d.ldd > SHA256SUMS
  )

  mv "${api_tmp_dir}" "${API_ARTIFACT_DIR}"

  echo "Rust API release artifacts written to ${API_ARTIFACT_DIR}"
  ls -lh "${API_ARTIFACT_DIR}"
}

build_worker_artifacts() {
  # Cloud Hypervisor support is a default feature; the real KVM gate qualifies it.
  echo "=== Rust worker release artifact: cargo build --release -p g5d -p fake-provider-cli ==="
  cargo build --release --locked -p g5d
  cargo build --release --locked -p fake-provider-cli --bins

  local worker_tmp_dir="${WORKER_ARTIFACT_DIR}.tmp"
  rm -rf "${worker_tmp_dir}" "${WORKER_ARTIFACT_DIR}"
  mkdir -p "${worker_tmp_dir}"

  install -m 0755 "${CARGO_TARGET_DIR}/release/g5d" "${worker_tmp_dir}/g5d"
  verify_self_contained_g5d "${worker_tmp_dir}/g5d" "${worker_tmp_dir}/g5d.ldd"
  # Fake provider CLIs ride along for the deterministic CI worker image
  # (Dockerfile.g5d-prebuilt copies them beside g5d).
  for fake in fake-claude fake-codex fake-g5-tools; do
    install -m 0755 "${CARGO_TARGET_DIR}/release/${fake}" "${worker_tmp_dir}/${fake}"
  done

  (
    cd "${worker_tmp_dir}"
    sha256sum g5d g5d.ldd fake-claude fake-codex fake-g5-tools > SHA256SUMS
  )

  mv "${worker_tmp_dir}" "${WORKER_ARTIFACT_DIR}"

  echo "Rust worker release artifacts written to ${WORKER_ARTIFACT_DIR}"
  ls -lh "${WORKER_ARTIFACT_DIR}"
}

clean_baked_artifacts

case "${G5_CI_RELEASE_ARTIFACTS_MODE:-all}" in
  all)
    build_api_artifacts
    build_worker_artifacts
    ;;
  api)
    build_api_artifacts
    ;;
  worker)
    build_worker_artifacts
    ;;
  *)
    echo "ERROR: unknown G5_CI_RELEASE_ARTIFACTS_MODE=${G5_CI_RELEASE_ARTIFACTS_MODE}" >&2
    exit 2
    ;;
esac
