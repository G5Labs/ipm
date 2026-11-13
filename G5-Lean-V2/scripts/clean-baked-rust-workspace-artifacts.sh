#!/usr/bin/env bash
set -euo pipefail

target_base="${CARGO_TARGET_DIR:-}"
if [[ -z "$target_base" || ! -d "$target_base" ]]; then
  exit 0
fi

case "${G5_CI_SKIP_BAKED_ARTIFACT_CLEAN:-false}" in
  1|true|TRUE|yes|YES)
    echo "Skipping baked local workspace artifact cleanup for target-prime image"
    exit 0
    ;;
esac

packages=(
  g5-contracts
  g5-auth
  g5-storage
  g5-semantic
  g5-scm
  g5-internal-api
  fake-provider-cli
  g5-authz
  g5-api-foundation
  g5-api-data
  g5-api-core
  g5-api-routes
  g5-api
  g5d-foundation
  g5d-isolation
  g5d-agent
  g5d-host
  g5d-dispatch
  g5d
  g5-e2e
)

clean_target_dir() {
  local target_dir="$1"
  [[ -d "$target_dir" ]] || return 0

  local args=()
  local package
  for package in "${packages[@]}"; do
    args+=(-p "$package")
  done

  echo "Cleaning baked local workspace artifacts from ${target_dir}"
  CARGO_TARGET_DIR="$target_dir" cargo clean "${args[@]}" >/dev/null 2>&1 || true
}

clean_target_dir "$target_base"
clean_target_dir "$target_base/llvm-cov-target"
