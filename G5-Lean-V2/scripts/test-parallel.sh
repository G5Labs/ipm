#!/usr/bin/env bash
# Parallel phased test runner for the G5 workspace.
# Runs independent crates/test-types concurrently within each phase,
# but phases execute sequentially (compile → unit → integration → doc).
#
# Usage:
#   ./scripts/test-parallel.sh              # all phases
#   ./scripts/test-parallel.sh unit         # unit tests only
#   ./scripts/test-parallel.sh integration  # integration tests only
#   ./scripts/test-parallel.sh doc          # doc tests only
#   ./scripts/test-parallel.sh compile      # compile check only

set -euo pipefail

if [[ -f .env ]]; then
    # Local tests need port overrides from .env, but auth/provider variables
    # such as G5D_ORG_ID change CLI semantics and make tests non-hermetic.
    for name in G5_PORT_POSTGRES; do
        if [[ -z "${!name:-}" ]]; then
            # shellcheck disable=SC1091
            value="$(source .env; printf '%s' "${!name:-}")"
            if [[ -n "$value" ]]; then
                export "$name=$value"
            fi
        fi
    done
fi

export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
G5D_TEST_FEATURE_ARGS=(--no-default-features --features test-helpers)
API_IMPLEMENTATION_UNIT_PACKAGE_ARGS=(
    -p g5-api-foundation -p g5-api-data
    -p g5-api-core -p g5-api-routes
)
API_UNIT_PACKAGE_ARGS=("${API_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}" -p g5-api)
G5D_IMPLEMENTATION_UNIT_PACKAGE_ARGS=(
    -p g5d-foundation -p g5d-isolation
    -p g5d-agent -p g5d-host
    -p g5d-dispatch
)

PHASE="${1:-all}"
FAILED=0
PASS_COUNT=0
FAIL_COUNT=0
RESULT_LABELS=()
RESULT_STATUSES=()

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_phase() { echo -e "\n${CYAN}═══ $1 ═══${NC}"; }
log_ok()    { echo -e "  ${GREEN}✓${NC} $1"; }
log_fail()  { echo -e "  ${RED}✗${NC} $1"; }
log_skip()  { echo -e "  ${YELLOW}⊘${NC} $1 (skipped)"; }

# Run a command and preserve its exit status. Background callers use this
# primitive so the parent shell can account for the result after `wait`.
run_logged() {
    local label="$1"; shift
    local logfile="/tmp/g5-test-${label// /_}.log"
    if "$@" > "$logfile" 2>&1; then
        log_ok "$label"
        return 0
    else
        log_fail "$label (see $logfile)"
        return 1
    fi
}

record_result() {
    local label="$1"
    local status="$2"
    RESULT_LABELS+=("$label")
    RESULT_STATUSES+=("$status")
    if [[ "$status" == "pass" ]]; then
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        FAIL_COUNT=$((FAIL_COUNT + 1))
        FAILED=1
    fi
}

# Run a foreground command and account for it without aborting later phases.
run_tracked() {
    local label="$1"; shift
    if run_logged "$label" "$@"; then
        record_result "$label" "pass"
    else
        record_result "$label" "FAIL"
    fi
}

# Account for a background command in the parent shell. Shell variables written
# by a background child are not visible here, so every PID must cross this gate.
wait_tracked() {
    local pid="$1"
    local label="$2"
    if wait "$pid"; then
        record_result "$label" "pass"
    else
        record_result "$label" "FAIL"
    fi
}

# ──────────────────────────────────────────────
# Phase 0: Compile check (all crates in parallel)
# ──────────────────────────────────────────────
phase_compile() {
    log_phase "Phase 0: Compile Check"

    # Build all test binaries without running them — catches compile errors fast.
    # The ordinary local test lane intentionally avoids VM-only features so
    # contributor hosts can run CI without a microVM toolchain. Dedicated
    # isolation gates exercise g5d with all VM features enabled.
    run_tracked "compile-workspace" cargo test --workspace --exclude g5d --exclude g5-e2e --no-run
    run_tracked "compile g5d process mode" cargo test -p g5d "${G5D_TEST_FEATURE_ARGS[@]}" --no-run
}

# ──────────────────────────────────────────────
# Phase 1: Unit tests (per-crate, parallel)
# ──────────────────────────────────────────────
phase_unit() {
    log_phase "Phase 1: Unit Tests (parallel per crate)"

    # Launch each crate family's unit tests concurrently. Extracted packages
    # own the implementation tests; the outer packages retain shell tests.
    run_logged "unit g5 shared crates" cargo test -p g5-contracts -p g5-auth -p g5-storage -p g5-semantic -p g5-scm -p g5-internal-api --lib &
    local pid_common=$!

    run_logged "unit g5-api family" cargo test "${API_UNIT_PACKAGE_ARGS[@]}" --lib --bins &
    local pid_api=$!

    run_logged "unit g5d implementation" cargo test "${G5D_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}" --lib &
    local pid_g5d_implementation=$!

    run_logged "unit g5d" cargo test -p g5d "${G5D_TEST_FEATURE_ARGS[@]}" --lib &
    local pid_g5d=$!

    # Wait for all unit test jobs
    wait_tracked "$pid_common" "unit g5 shared crates"
    wait_tracked "$pid_api" "unit g5-api family"
    wait_tracked "$pid_g5d_implementation" "unit g5d implementation"
    wait_tracked "$pid_g5d" "unit g5d"
}

# ──────────────────────────────────────────────
# Phase 2: Integration tests (serial — shared resources)
# ──────────────────────────────────────────────
phase_integration() {
    log_phase "Phase 2: Integration Tests (serial)"

    # g5d integration tests must run serially (shared PTY, temp dirs)
    if ls backend/g5d/tests/*.rs &>/dev/null; then
        run_tracked "integration g5d" cargo test -p g5d-dispatch -p g5d "${G5D_TEST_FEATURE_ARGS[@]}" --test '*' -- --test-threads=1
    else
        log_skip "integration g5d (no test files)"
    fi

    # g5-api integration tests (if any). Inside a g5d-g5dev guest nothing
    # listens on the test-DB port until the baked helper is started, and
    # agents kept skipping these suites as "needs a live Postgres"; start it
    # here when it exists and the port is silent. It is idempotent and never
    # adopts a stack it did not start (a compose stack on the same port is
    # simply used), and it exports the URLs the harness reads.
    if ls backend/g5-api/tests/*.rs &>/dev/null 2>&1; then
        local testdb_ready=1
        local pg_port="${G5_PORT_POSTGRES:-5432}"
        # An explicit database URL names the caller's target; never start a
        # local helper over it or redirect the suite to the helper's database.
        if [[ -z "${TEST_DATABASE_URL:-}${DATABASE_URL:-}" ]] \
            && command -v g5-dev-testdb-start >/dev/null && command -v pg_isready >/dev/null \
            && ! pg_isready -q -h 127.0.0.1 -p "$pg_port" 2>/dev/null; then
            # The helper and the probe must agree on the port, and a helper
            # failure (for example a foreign process on the port it refuses
            # to adopt) must not be masked by eval'ing an empty stdout.
            local testdb_exports
            if testdb_exports="$(G5_TESTDB_PG_PORT="$pg_port" g5-dev-testdb-start)"; then
                eval "$testdb_exports"
                log_ok "test DB started on :$pg_port"
            else
                testdb_ready=0
                record_result "test DB start" "FAIL"
                log_fail "test DB start (g5-dev-testdb-start failed; see above)"
            fi
        fi
        # The harness reads TEST_DATABASE_URL. Point it at the configured port
        # whether this run started Postgres or found it already listening,
        # unless the caller set the harness name explicitly.
        export TEST_DATABASE_URL="${TEST_DATABASE_URL:-${DATABASE_URL:-postgres://g5:g5_dev@127.0.0.1:${pg_port}/g5_dev}}"
        if [[ $testdb_ready -eq 1 ]]; then
            run_tracked "integration g5-api" cargo test -p g5-api --test '*' -- --test-threads=1
        else
            log_skip "integration g5-api (no test DB)"
        fi
    else
        log_skip "integration g5-api (no test files)"
    fi
}

# ──────────────────────────────────────────────
# Phase 3: Doc tests (parallel per crate)
# ──────────────────────────────────────────────
phase_doc() {
    log_phase "Phase 3: Doc Tests (parallel per crate)"

    run_logged "doc g5 shared crates" cargo test -p g5-contracts -p g5-auth -p g5-storage -p g5-semantic -p g5-scm -p g5-internal-api --doc &
    local pid_common=$!

    run_logged "doc g5-api implementation" cargo test "${API_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}" --doc &
    local pid_api=$!

    run_logged "doc g5d implementation" cargo test "${G5D_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}" --doc &
    local pid_g5d_implementation=$!

    run_logged "doc g5d" cargo test -p g5d "${G5D_TEST_FEATURE_ARGS[@]}" --doc &
    local pid_g5d=$!

    wait_tracked "$pid_common" "doc g5 shared crates"
    wait_tracked "$pid_api" "doc g5-api implementation"
    wait_tracked "$pid_g5d_implementation" "doc g5d implementation"
    wait_tracked "$pid_g5d" "doc g5d"
}

# ──────────────────────────────────────────────
# Orchestration
# ──────────────────────────────────────────────
START_TIME=$(date +%s)

case "$PHASE" in
    compile)
        phase_compile
        ;;
    unit)
        phase_compile
        phase_unit
        ;;
    integration)
        phase_compile
        phase_integration
        ;;
    doc)
        phase_compile
        phase_doc
        ;;
    all)
        phase_compile
        phase_unit
        phase_integration
        phase_doc
        ;;
    *)
        echo "Usage: $0 {all|compile|unit|integration|doc}"
        exit 1
        ;;
esac

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

# ──────────────────────────────────────────────
# Summary
# ──────────────────────────────────────────────
echo ""
log_phase "Summary (${ELAPSED}s)"
echo -e "  ${GREEN}Passed: $PASS_COUNT${NC}"
if (( FAIL_COUNT > 0 )); then
    echo -e "  ${RED}Failed: $FAIL_COUNT${NC}"
    echo ""
    echo -e "${RED}Failed targets:${NC}"
    for ((index = 0; index < ${#RESULT_LABELS[@]}; index++)); do
        if [[ "${RESULT_STATUSES[$index]}" == "FAIL" ]]; then
            label="${RESULT_LABELS[$index]}"
            echo -e "  ${RED}✗${NC} $label → /tmp/g5-test-${label// /_}.log"
        fi
    done
fi
echo ""

exit $FAILED
