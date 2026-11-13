#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]}"
WORKSPACE_DIR="${G5_CI_WORKSPACE_DIR:-/workspace}"
GENERATION_DIR="${G5_CI_GENERATION_DIR:-${WORKSPACE_DIR}}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/opt/g5-rust-target-cache/target}"
LOWER_TARGET="${G5_CI_RUST_XFS_LOWER_TARGET:-${CARGO_TARGET_DIR}}"
XFS_ROOT="${G5_CI_RUST_XFS_ROOT:-${WORKSPACE_DIR}/rust-xfs-orchestrator}"
XFS_MOUNT="${G5_CI_RUST_XFS_MOUNT:-/mnt/g5-rust-xfs}"
XFS_IMAGE="${XFS_ROOT}/rust-target.xfs"
XFS_SIZE_GB="${G5_CI_RUST_XFS_SIZE_GB:-120}"
LOG_DIR="${XFS_ROOT}/logs"
RESULT_DIR="${XFS_ROOT}/results"
METADATA_DIR="${XFS_ROOT}/nextest-metadata"
RELEASE_ARTIFACTS_DIR="${G5_CI_RELEASE_ARTIFACTS_DIR:-${GENERATION_DIR}/.ci-release-artifacts/api}"

export CARGO_HOME="${CARGO_HOME:-/opt/g5-cargo-home}"
export PATH="${CARGO_HOME}/bin:${PATH}"
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

PARALLEL_LANES=(
  rust-test
  rust-integration
  rust-integration-g5d
  isolation-backends
  isolation-unit
)

LANES=("${PARALLEL_LANES[@]}")

HARD_FAIL_LANES=(
  rust-test
  rust-integration
  isolation-backends
  isolation-unit
)

UNIT_PACKAGE_ARGS=(
  -p g5-contracts -p g5-auth -p g5-storage
  -p g5-semantic -p g5-scm -p g5-internal-api
  -p fake-provider-cli
  -p g5-api-foundation -p g5-api-data
  -p g5-api-core -p g5-api-routes -p g5-api
  -p g5d-foundation -p g5d-isolation
  -p g5d-agent -p g5d-host -p g5d-dispatch -p g5d
)

mounted_overlays=()
xfs_mounted=false
LAST_OVERLAY_TARGET=""
LAST_LANE_PID=""

log() {
  printf '[rust-xfs] %s\n' "$*"
}

default_ci_test_jobs() {
  local cores jobs cap
  cores="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"
  jobs=$(( (cores * 3 + 3) / 4 ))
  if (( jobs < 1 )); then
    jobs=1
  fi
  cap="${1:-0}"
  if (( cap > 0 && jobs > cap )); then
    jobs="$cap"
  fi
  printf '%s\n' "$jobs"
}

write_superset_binaries_metadata() {
  local superset_metadata="${METADATA_DIR}/superset-binaries.json"

  log "compiling/listing one superset of Rust test binaries"
  cargo nextest list \
    --list-type binaries-only \
    --message-format json \
    --locked \
    "${UNIT_PACKAGE_ARGS[@]}" \
    --lib --bins --tests --all-features \
    > "${superset_metadata}"

  log "slicing superset binaries metadata into lane metadata"
  python3 - "${METADATA_DIR}/cargo-metadata.json" "${superset_metadata}" "${METADATA_DIR}" <<'PY'
import json
import sys
from pathlib import Path

cargo_metadata_path = Path(sys.argv[1])
superset_metadata_path = Path(sys.argv[2])
metadata_dir = Path(sys.argv[3])

cargo_metadata = json.loads(cargo_metadata_path.read_text())
superset_metadata = json.loads(superset_metadata_path.read_text())

package_names = {package["id"]: package["name"] for package in cargo_metadata["packages"]}
rust_binaries = superset_metadata.get("rust-binaries")
if not isinstance(rust_binaries, dict):
    raise SystemExit("nextest binaries metadata did not contain a rust-binaries object")

def package_name(binary):
    return package_names.get(binary.get("package-id"), "")

def binary_kind(binary):
    return binary.get("kind")

def binary_name(binary):
    return binary.get("binary-name") or binary.get("binary-id") or ""

UNIT_PACKAGES = {
    "g5-contracts",
    "g5-auth",
    "g5-storage",
    "g5-semantic",
    "g5-scm",
    "g5-internal-api",
    "fake-provider-cli",
    "g5-api-foundation",
    "g5-api-data",
    "g5-api-core",
    "g5-api-routes",
    "g5-api",
    "g5d-foundation",
    "g5d-isolation",
    "g5d-agent",
    "g5d-host",
    "g5d-dispatch",
    "g5d",
}

def is_rust_test(binary):
    return (
        package_name(binary) in UNIT_PACKAGES
        and binary_kind(binary) in {"lib", "bin"}
    ) or (
        package_name(binary) == "g5-contracts"
        and binary_kind(binary) == "test"
        and binary_name(binary) == "consumer_boundaries"
    ) or (
        package_name(binary) == "g5d"
        and binary_kind(binary) == "test"
        and binary_name(binary) == "dispatch"
    )

def is_rust_integration(binary):
    return package_name(binary) in {"g5-api", "fake-provider-cli"} and binary_kind(binary) == "test"

def is_g5d_integration(binary):
    return (
        # The real g5d dispatch process boundary belongs to rust-test above.
        package_name(binary) in {"g5d", "g5d-dispatch"}
        and binary_kind(binary) == "test"
        and binary_name(binary) not in {
            "dispatch",
            "isolation_backends",
        }
    )

def is_isolation_backends(binary):
    return (
        package_name(binary) == "g5d"
        and binary_kind(binary) == "test"
        and binary_name(binary) == "isolation_backends"
    )

def is_isolation_unit(binary):
    return package_name(binary) == "g5d-isolation" and binary_kind(binary) == "lib"

lanes = {
    "rust-test": is_rust_test,
    "rust-integration": is_rust_integration,
    "rust-integration-g5d": is_g5d_integration,
    "isolation-backends": is_isolation_backends,
    "isolation-unit": is_isolation_unit,
}

for lane, predicate in lanes.items():
    lane_metadata = dict(superset_metadata)
    lane_metadata["rust-binaries"] = {
        binary_id: binary
        for binary_id, binary in rust_binaries.items()
        if predicate(binary)
    }
    output_path = metadata_dir / f"{lane}-binaries.json"
    output_path.write_text(json.dumps(lane_metadata, separators=(",", ":")) + "\n")
    print(f"{lane}: {len(lane_metadata['rust-binaries'])} binaries")
PY
}

prepare_child_target_mount() {
  local merged_target="${G5_CI_RUST_XFS_MERGED_TARGET:?missing merged target}"

  mount --make-rprivate / 2>/dev/null || true
  mkdir -p "${CARGO_TARGET_DIR}"
  mount --bind "${merged_target}" "${CARGO_TARGET_DIR}"
  cd "${GENERATION_DIR}"

  log "target path: ${CARGO_TARGET_DIR}"
  log "target backing: ${merged_target}"
}

run_prime_compile() {
  prepare_child_target_mount

  log "cleaning baked workspace artifacts"
  bash ./scripts/clean-baked-rust-workspace-artifacts.sh

  mkdir -p "${METADATA_DIR}"
  cargo metadata --locked --all-features --format-version 1 > "${METADATA_DIR}/cargo-metadata.json"

  # Plain (non-instrumented) test compile: coverage instrumentation was
  # dropped from the PR path together with the CI builder image's warmup
  # (G5_CI_BUILDER_COVERAGE=0), so the baked target and this compile share
  # fingerprints. Peak-memory phase on the 2 GB/vCPU CI worker: cap rustc
  # parallelism (orchestration_scripts D22); the caller passes the cap through
  # G5_CI_SUPERSET_COMPILE_JOBS. The debuginfo half of the mitigation lives in
  # the workspace manifest ([profile.dev] debug = 1); do not add per-step
  # CARGO_PROFILE_* overrides here — they fork the baked warmup's fingerprints.
  export CARGO_BUILD_JOBS="${G5_CI_SUPERSET_COMPILE_JOBS:-16}"
  log "superset compile jobs: ${CARGO_BUILD_JOBS}"

  write_superset_binaries_metadata

  sccache --show-stats || true
}

run_release_artifacts() {
  local mode="${1:-all}"

  prepare_child_target_mount
  G5_CI_RELEASE_ARTIFACTS_DIR="${RELEASE_ARTIFACTS_DIR}" \
  G5_CI_RELEASE_ARTIFACTS_MODE="${mode}" \
    bash ./scripts/build-release-artifacts.sh
  sccache --show-stats || true
}

write_g5d_integration_result() {
  local integration_status
  integration_status="$(cat "${RESULT_DIR}/rust-integration-g5d.exit" 2>/dev/null || echo 127)"

  if [[ "${integration_status}" == "0" ]]; then
    echo "RUST_INTEG_G5D=passed" > "${WORKSPACE_DIR}/step-results-rust-integration-g5d.txt"
  else
    echo "RUST_INTEG_G5D=failed" > "${WORKSPACE_DIR}/step-results-rust-integration-g5d.txt"
  fi
}

run_rust_test_lane() {
  local status=0
  cargo nextest run \
    --cargo-metadata "${METADATA_DIR}/cargo-metadata.json" \
    --binaries-metadata "${METADATA_DIR}/rust-test-binaries.json" \
    --target-dir-remap "${CARGO_TARGET_DIR}" \
    --no-fail-fast || status="$?"

  return "${status}"
}

run_rust_integration_lane() {
  local status=0
  local jobs
  jobs="${G5_CI_INTEGRATION_JOBS:-$(default_ci_test_jobs 16)}"
  log "API and fake-provider integration jobs: ${jobs}"

  cargo nextest run \
    --cargo-metadata "${METADATA_DIR}/cargo-metadata.json" \
    --binaries-metadata "${METADATA_DIR}/rust-integration-binaries.json" \
    --target-dir-remap "${CARGO_TARGET_DIR}" \
    --no-fail-fast -j "${jobs}" || status="$?"

  return "${status}"
}

run_rust_integration_g5d_lane() {
  local status=0
  local jobs
  jobs="${G5_CI_G5D_INTEGRATION_JOBS:-$(default_ci_test_jobs 0)}"
  log "g5d integration jobs: ${jobs}"

  cargo nextest run \
    --cargo-metadata "${METADATA_DIR}/cargo-metadata.json" \
    --binaries-metadata "${METADATA_DIR}/rust-integration-g5d-binaries.json" \
    --target-dir-remap "${CARGO_TARGET_DIR}" \
    --no-fail-fast -j "${jobs}" || status="$?"

  return "${status}"
}

prepare_kvm() {
  if [[ -e /dev/kvm ]]; then
    chmod 666 /dev/kvm || true
  fi
  ldconfig || true
  cat "${WORKSPACE_DIR}/kvm_available" 2>/dev/null || true
}

run_isolation_backends_lane() {
  local status=0
  prepare_kvm
  # The g5-ci smoke-cloud-hypervisor step owns strict KVM boot/network coverage.
  # This suite owns process-side isolation and backend-selection checks.
  cargo nextest run \
    --cargo-metadata "${METADATA_DIR}/cargo-metadata.json" \
    --binaries-metadata "${METADATA_DIR}/isolation-backends-binaries.json" \
    --target-dir-remap "${CARGO_TARGET_DIR}" \
    --no-fail-fast --test-threads=1 || status="$?"


  return "${status}"
}

run_isolation_unit_lane() {
  local status=0
  prepare_kvm
  cargo nextest run \
    --cargo-metadata "${METADATA_DIR}/cargo-metadata.json" \
    --binaries-metadata "${METADATA_DIR}/isolation-unit-binaries.json" \
    --target-dir-remap "${CARGO_TARGET_DIR}" \
    --no-fail-fast -j2 \
    -E 'test(isolation)' || status="$?"

  return "${status}"
}

run_lane() {
  local lane="${1:?missing lane}"

  prepare_child_target_mount
  log "starting lane: ${lane}"

  case "${lane}" in
    rust-test)
      run_rust_test_lane
      ;;
    rust-integration)
      run_rust_integration_lane
      ;;
    rust-integration-g5d)
      run_rust_integration_g5d_lane
      ;;
    isolation-backends)
      run_isolation_backends_lane
      ;;
    isolation-unit)
      run_isolation_unit_lane
      ;;
    *)
      log "unknown lane: ${lane}"
      return 2
      ;;
  esac
}

cleanup_parent() {
  local overlay
  for (( idx=${#mounted_overlays[@]} - 1; idx >= 0; idx-- )); do
    overlay="${mounted_overlays[$idx]}"
    umount -l "${overlay}" 2>/dev/null || true
  done

  if [[ "${xfs_mounted}" == "true" ]]; then
    umount -l "${XFS_MOUNT}" 2>/dev/null || true
  fi

  rm -f "${XFS_IMAGE}" 2>/dev/null || true
}

setup_xfs() {
  log "preparing XFS reflink workspace at ${XFS_MOUNT}"
  umount -l "${XFS_MOUNT}" 2>/dev/null || true
  rm -rf "${XFS_ROOT}"
  mkdir -p "${XFS_ROOT}" "${XFS_MOUNT}" "${LOG_DIR}" "${RESULT_DIR}"

  truncate -s "${XFS_SIZE_GB}G" "${XFS_IMAGE}"
  # Pin the on-disk feature set. The image this runs in moved from Debian
  # bookworm (xfsprogs 6.1) to Ubuntu 26.04 (xfsprogs 6.18), which flipped
  # `rmapbt` and `nrext64` on by default. nrext64 needs a 5.19+ kernel, and the
  # Cloud Build host that has to mount this loopback is older, so the defaults
  # produce a filesystem mkfs writes happily and mount rejects with
  # "wrong fs type, bad option, bad superblock". Keep reflink (the point of the
  # lane) and hold the rest at the format the previous base produced.
  mkfs.xfs -f -m reflink=1,rmapbt=0 -i nrext64=0 "${XFS_IMAGE}" >/dev/null
  mount -o loop "${XFS_IMAGE}" "${XFS_MOUNT}"
  xfs_mounted=true
  mkdir -p "${XFS_MOUNT}/test-tmp"
  xfs_info "${XFS_MOUNT}" | tee "${XFS_ROOT}/xfs-info.txt"
}

mount_overlay_target() {
  local name="${1:?missing overlay name}"
  local upper="${XFS_MOUNT}/uppers/${name}"
  local work="${XFS_MOUNT}/work/${name}"
  local merged="${XFS_MOUNT}/merged/${name}"

  mkdir -p "${upper}" "${work}" "${merged}"
  mount -t overlay "g5-rust-${name}" \
    -o "lowerdir=${LOWER_TARGET},upperdir=${upper},workdir=${work}" \
    "${merged}"
  mounted_overlays+=("${merged}")
  LAST_OVERLAY_TARGET="${merged}"
}

run_in_mount_namespace() {
  local merged_target="${1:?missing merged target}"
  shift

  env G5_CI_RUST_XFS_MERGED_TARGET="${merged_target}" \
    G5_TEST_REFLINK_ROOT="${XFS_MOUNT}/test-tmp" \
    unshare --mount --propagation private --fork "${SCRIPT_PATH}" "$@"
}

fanout_lane_upper() {
  local lane="${1:?missing lane}"
  local prime_upper="${XFS_MOUNT}/uppers/prime"
  local lane_upper="${XFS_MOUNT}/uppers/${lane}"

  rm -rf "${lane_upper}"
  mkdir -p "${lane_upper}"
  cp -a --reflink=always "${prime_upper}/." "${lane_upper}/"
}

run_lane_async() {
  local lane="${1:?missing lane}"
  local merged="${2:?missing merged target}"

  (
    set +e
    run_in_mount_namespace "${merged}" --lane "${lane}"
    local status="$?"
    echo "${status}" > "${RESULT_DIR}/${lane}.exit"
    exit "${status}"
  ) > >(tee "${LOG_DIR}/${lane}.log") 2>&1 &

  LAST_LANE_PID="$!"
}

is_hard_fail_lane() {
  local lane="${1:?missing lane}"
  local hard_lane
  for hard_lane in "${HARD_FAIL_LANES[@]}"; do
    if [[ "${lane}" == "${hard_lane}" ]]; then
      return 0
    fi
  done
  return 1
}

summarize_results() {
  local failed=0
  local lane status

  log "lane summary:"
  for lane in "${LANES[@]}"; do
    status="$(cat "${RESULT_DIR}/${lane}.exit" 2>/dev/null || echo 127)"
    log "  ${lane}: ${status}"
    if [[ "${status}" != "0" ]] && is_hard_fail_lane "${lane}"; then
      failed=1
    fi
  done

  write_g5d_integration_result

  if (( failed != 0 )); then
    log "one or more hard-fail Rust lanes failed"
    return 1
  fi

  log "all hard-fail Rust lanes passed"
}

run_parent() {
  if [[ "$(id -u)" != "0" ]]; then
    log "the XFS orchestrator must run as root in a privileged container"
    exit 2
  fi
  if [[ ! -d "${LOWER_TARGET}" ]]; then
    log "missing lower target cache: ${LOWER_TARGET}"
    exit 2
  fi

  trap cleanup_parent EXIT
  setup_xfs

  local prime_merged
  mount_overlay_target prime
  prime_merged="${LAST_OVERLAY_TARGET}"
  log "running one compile prime"
  run_in_mount_namespace "${prime_merged}" --prime | tee "${LOG_DIR}/prime.log"

  du -sh "${XFS_MOUNT}/uppers/prime" | tee "${XFS_ROOT}/prime-upper-size.txt" || true

  local lane merged pid
  local pids=()
  local pid_lanes=()

  for lane in "${PARALLEL_LANES[@]}"; do
    fanout_lane_upper "${lane}"
    mount_overlay_target "${lane}"
    merged="${LAST_OVERLAY_TARGET}"
    run_lane_async "${lane}" "${merged}"
    pid="${LAST_LANE_PID}"
    pids+=("${pid}")
    pid_lanes+=("${lane}")
    log "started ${lane} as pid ${pid}"
  done

  local idx
  for idx in "${!pids[@]}"; do
    if ! wait "${pids[$idx]}"; then
      log "${pid_lanes[$idx]} exited non-zero"
    fi
  done

  sccache --show-stats || true
  summarize_results
}

run_release_artifacts_parent() {
  if [[ "$(id -u)" != "0" ]]; then
    log "the XFS release artifact builder must run as root in a privileged container"
    exit 2
  fi
  if [[ ! -d "${LOWER_TARGET}" ]]; then
    log "missing lower target cache: ${LOWER_TARGET}"
    exit 2
  fi

  trap cleanup_parent EXIT
  setup_xfs

  if [[ "${G5_CI_RELEASE_ARTIFACTS_PARALLEL:-true}" =~ ^(1|true|TRUE|yes|YES)$ ]]; then
    log "building release artifacts with parallel XFS target overlays"

    local name mode merged pid
    local pids=()
    local names=()

    for name in release-api release-worker; do
      case "${name}" in
        release-api)
          mode=api
          ;;
        release-worker)
          mode=worker
          ;;
      esac

      mount_overlay_target "${name}"
      merged="${LAST_OVERLAY_TARGET}"
      (
        set +e
        run_in_mount_namespace "${merged}" --release-artifacts-child "${mode}" \
          2>&1 | tee "${LOG_DIR}/${name}.log"
        local status="${PIPESTATUS[0]}"
        echo "${status}" > "${RESULT_DIR}/${name}.exit"
        exit "${status}"
      ) &
      pid="$!"
      pids+=("${pid}")
      names+=("${name}")
      log "started ${name} release artifact build as pid ${pid}"
    done

    local idx failed=0
    for idx in "${!pids[@]}"; do
      if ! wait "${pids[$idx]}"; then
        log "${names[$idx]} release artifact build exited non-zero"
        failed=1
      fi
    done

    du -sh "${XFS_MOUNT}/uppers/release-api" \
      "${XFS_MOUNT}/uppers/release-worker" \
      | tee "${XFS_ROOT}/release-artifacts-upper-size.txt" || true

    if (( failed != 0 )); then
      return 1
    fi
  else
    mount_overlay_target release-artifacts
    log "building release artifacts with XFS target overlay"
    run_in_mount_namespace "${LAST_OVERLAY_TARGET}" --release-artifacts-child all \
      | tee "${LOG_DIR}/release-artifacts.log"

    du -sh "${XFS_MOUNT}/uppers/release-artifacts" \
      | tee "${XFS_ROOT}/release-artifacts-upper-size.txt" || true
  fi
}

case "${1:-}" in
  --prime)
    run_prime_compile
    ;;
  --release-artifacts)
    run_release_artifacts_parent
    ;;
  --release-artifacts-child)
    shift
    run_release_artifacts "${1:-all}"
    ;;
  --lane)
    shift
    run_lane "${1:?missing lane}"
    ;;
  ""|--run)
    run_parent
    ;;
  *)
    log "usage: ${SCRIPT_PATH} [--run|--prime|--release-artifacts|--release-artifacts-child [all|api|worker]|--lane <name>]"
    exit 2
    ;;
esac
