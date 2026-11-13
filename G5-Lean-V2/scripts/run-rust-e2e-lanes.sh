#!/usr/bin/env bash
# Run retained deterministic Rust E2E scenarios against the workspace-VM stack.
#
# The legacy runner paused task workers and then drove synthetic worker registration,
# claim, acknowledgement, and sticky-runtime scenarios. Those execution paths were
# removed in P4.9. A healthy bound `g5d host` remains available throughout this run
# so tests observe the same dispatcher/VM path used in production.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_DIR"

available_cores() {
  local cores
  cores="$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 2)"
  cores="${cores%%.*}"
  if ! [[ "$cores" =~ ^[1-9][0-9]*$ ]]; then
    cores=2
  fi
  echo "$cores"
}

resolve_threads() {
  local value="${G5_E2E_STACK_THREADS:-2}"
  case "$value" in
    auto|AUTO|auto75|AUTO75|75%)
      local threads=$(( $(available_cores) * 3 / 4 ))
      (( threads > 0 )) || threads=1
      echo "$threads"
      ;;
    *[!0-9]*|'')
      echo "ERROR: G5_E2E_STACK_THREADS must be a positive integer or auto75." >&2
      exit 1
      ;;
    *)
      (( value > 0 )) || {
        echo "ERROR: G5_E2E_STACK_THREADS must be greater than zero." >&2
        exit 1
      }
      echo "$value"
      ;;
  esac
}

cargo_cmd() {
  local subcommand="$1"
  shift
  local -a cmd=(cargo "$subcommand")
  if [[ -n "${G5_E2E_CARGO_JOBS:-}" ]]; then
    cmd+=("-j${G5_E2E_CARGO_JOBS}")
  fi
  printf '+'
  printf ' %q' "${cmd[@]}" "$@"
  printf '\n'
  "${cmd[@]}" "$@"
}

echo "=== Rust E2E workspace-VM runner ==="
echo "  test threads: ${G5_E2E_STACK_THREADS:-2}"
echo "  cargo jobs:   ${G5_E2E_CARGO_JOBS:-cargo default}"

if [[ "${G5_E2E_NO_COMPILE:-false}" != "true" ]]; then
  cargo_cmd build -p g5d
  cargo_cmd test -p g5-e2e --no-run
fi

threads="$(resolve_threads)"

# These scenarios exercise retained control-plane boundaries without creating
# a synthetic worker. Workspace file, agent, preview, and terminal execution is
# covered by the workspace-VM integration suites and explicit VM waits in the
# Docker harness.
#
# Reviewed against Lean design v9 (issue #142): every scenario module still
# names routes or binaries that exist. The groups below are the ones with a
# working gate in this stack:
#   - cli_ops and ci_e2e_lane_isolation are hermetic (g5d CLI on a local bare
#     repository; the CI pipeline's lane-isolation contract);
#   - ontology_lifecycle reads the retained workspace ontology route and skips
#     without the API health probe, like codex_control_frames.
retained_scenarios=(
  browser_interaction
  ci_e2e_lane_isolation
  ci_stale_build_cancellation
  cli_ops
  codex_control_frames
  comment_notifications
  host_boot_timings
  ontology_lifecycle
  r2_guest_lifecycle
  shared_vm_chats
  formal_agent_approvals
  value_loop_matrix
)

# Tests whose one execution owner is the value-loop receipt lane, which runs
# each as a Rust selector (ontology g5.quality.delivery.ci.single-execution-owner).
# Skip only the exact test, never its module, so tests later added to the module
# still run here; scripts/test_rust_e2e_lanes.py rejects an entry the receipt
# catalog does not own. Entries are "<scenario> <test name>".
#   - API-restart continuity (M6) restarts the lane API, which the serialized
#     receipt lane owns exclusively.
#   - Explicit formal reviews (RV7) took ~4.5–7.5 min in each lane; the receipt
#     execution is the one its evidence comes from.
receipt_owned_tests=(
  "shared_vm_chats test_streamed_chat_survives_api_restart_and_continues_same_task"
  "formal_agent_approvals explicit_formal_reviews_keep_verdict_and_evidence_after_isolated_resources_disappear"
)

failed=0
for scenario in "${retained_scenarios[@]}"; do
  echo "--- Rust E2E retained scenario: ${scenario} ---"
  scenario_threads="$threads"
  if [[ "$scenario" == r2_guest_lifecycle || "$scenario" == shared_vm_chats ]]; then
    # These cases boot/stop real VMs and share this lane's single CI host.
    # Serialize cases so the additional steering journey cannot exhaust its
    # VM capacity; each chat journey still exercises concurrent held turns.
    scenario_threads=1
  fi
  test_args=(--test-threads="$scenario_threads")
  for owned in "${receipt_owned_tests[@]}"; do
    if [[ "${owned%% *}" == "$scenario" ]]; then
      test_args+=(--skip "${owned#* }")
    fi
  done
  if ! cargo_cmd test -p g5-e2e "$scenario" -- "${test_args[@]}"; then
    failed=$((failed + 1))
  fi
done

if (( failed > 0 )); then
  echo "ERROR: ${failed} retained Rust E2E scenario group(s) failed" >&2
  exit 1
fi

echo "=== Rust E2E workspace-VM runner complete ==="
