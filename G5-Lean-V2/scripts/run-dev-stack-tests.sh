#!/usr/bin/env bash
set -euo pipefail

CARGO_ARGS=()
TEST_ARGS=()

usage() {
  cat <<'EOF'
Usage:
  ./scripts/run-dev-stack-tests.sh rust [cargo-test args...]
  ./scripts/run-dev-stack-tests.sh playwright [playwright args...]

Behavior:
  - Targets the already-running g5-dev stack.
  - Resolves API/edge/OAuth/Postgres URLs from the live containers.
  - Refreshes only the isolated e2e-test workspace fixture unless
    G5_DEV_E2E_SKIP_SEED=true is set.
  - Rust mode keeps the bound workspace host running and uses the retained
    workspace-VM E2E scenario set by default.
EOF
}

bool_true() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

container_running() {
  local container="$1"
  [[ "$(docker inspect -f '{{.State.Running}}' "$container" 2>/dev/null || true)" == "true" ]]
}

require_container() {
  local container="$1"
  if ! container_running "$container"; then
    echo "ERROR: required dev-stack container '$container' is not running." >&2
    echo "Hint: start or restore the shared dev stack with 'just dev-up'." >&2
    exit 1
  fi
}

resolve_host_port() {
  local container="$1"
  local container_port="$2"
  local mapping

  mapping="$(docker port "$container" "$container_port" 2>/dev/null | head -n1 || true)"
  if [[ -z "$mapping" ]]; then
    echo "ERROR: could not resolve published port for $container:$container_port" >&2
    exit 1
  fi

  echo "${mapping##*:}"
}

load_generated_fixture_env() {
  if [[ -f .env.test ]]; then
    set -a
    # shellcheck disable=SC1091
    source .env.test
    set +a
  fi
}

configure_playwright_fixture_aliases() {
  local workspace_name org_slug repo_prefix code_repo_name docs_repo_name support_repo_name

  workspace_name="${G5_E2E_TEST_WORKSPACE_NAME:-${G5_E2E_TEST_WORKSPACE:-e2e-test}}"
  org_slug="${G5_E2E_TEST_ORG_SLUG:-${VITE_E2E_SELECTED_ORG:-g5labs}}"
  repo_prefix="${G5_E2E_TEST_REPO_SLUG_PREFIX:-$workspace_name}"
  code_repo_name="${G5_E2E_TEST_CODE_REPO_NAME:-${G5_E2E_TEST_CODE_REPO_SLUG:-${repo_prefix}-code-repo}}"
  docs_repo_name="${G5_E2E_TEST_DOCS_REPO_NAME:-${G5_E2E_TEST_DOCS_REPO_SLUG:-${repo_prefix}-docs-repo}}"
  if [[ "$workspace_name" == "e2e-test" ]]; then
    support_repo_name="${G5_E2E_OVERLAY_SUPPORT_REPO_NAME:-e2e-overlay-support-repo}"
  else
    support_repo_name="${G5_E2E_OVERLAY_SUPPORT_REPO_NAME:-${repo_prefix}-overlay-support-repo}"
  fi

  export G5_E2E_TEST_WORKSPACE="${G5_E2E_TEST_WORKSPACE:-$workspace_name}"
  export VITE_E2E_SELECTED_ORG="${VITE_E2E_SELECTED_ORG:-$org_slug}"
  export VITE_E2E_ORG="${VITE_E2E_ORG:-$org_slug}"
  export VITE_E2E_WORKSPACE="${VITE_E2E_WORKSPACE:-$workspace_name}"
  export G5_E2E_COMPLIANCE_ORG="${G5_E2E_COMPLIANCE_ORG:-$org_slug}"
  export G5_E2E_COMPLIANCE_WORKSPACE="${G5_E2E_COMPLIANCE_WORKSPACE:-$workspace_name}"
  export G5_E2E_COMPLIANCE_CODE_REPO="${G5_E2E_COMPLIANCE_CODE_REPO:-$code_repo_name}"
  export VITE_E2E_COMPLIANCE_ORG="${VITE_E2E_COMPLIANCE_ORG:-$org_slug}"
  export VITE_E2E_COMPLIANCE_WORKSPACE="${VITE_E2E_COMPLIANCE_WORKSPACE:-$workspace_name}"
  export VITE_E2E_CROSS_ENV_WORKSPACE="${VITE_E2E_CROSS_ENV_WORKSPACE:-$workspace_name}"
  export VITE_E2E_DEMO_ORG="${VITE_E2E_DEMO_ORG:-$org_slug}"
  export VITE_E2E_DEMO_WORKSPACE="${VITE_E2E_DEMO_WORKSPACE:-$workspace_name}"
  export VITE_E2E_DIFF_SEED_ORG="${VITE_E2E_DIFF_SEED_ORG:-$org_slug}"
  export VITE_E2E_DIFF_SEED_WORKSPACE="${VITE_E2E_DIFF_SEED_WORKSPACE:-$workspace_name}"
  export VITE_E2E_MANUAL_COMMIT_ORG="${VITE_E2E_MANUAL_COMMIT_ORG:-$org_slug}"
  export VITE_E2E_MANUAL_COMMIT_WORKSPACE="${VITE_E2E_MANUAL_COMMIT_WORKSPACE:-$workspace_name}"
  export VITE_E2E_CODE_REPO_NAME="${VITE_E2E_CODE_REPO_NAME:-$code_repo_name}"
  export VITE_E2E_DOCS_REPO_NAME="${VITE_E2E_DOCS_REPO_NAME:-$docs_repo_name}"
  export VITE_E2E_OVERLAY_SUPPORT_REPO_NAME="${VITE_E2E_OVERLAY_SUPPORT_REPO_NAME:-$support_repo_name}"
}

seed_dev_fixture() {
  if bool_true "${G5_DEV_E2E_SKIP_SEED:-false}"; then
    echo "Skipping fixture reseed because G5_DEV_E2E_SKIP_SEED=true"
    load_generated_fixture_env
    return
  fi

  echo "Refreshing isolated e2e-test fixture workspace against the running dev stack..."
  G5_E2E_TEST_EMAIL="${G5_E2E_TEST_EMAIL:-e2e-test@g5.dev}" \
    ./scripts/seed-test-workspace.sh "$G5_E2E_API_URL"
  load_generated_fixture_env
}

prepare_rust_args() {
  CARGO_ARGS=()
  TEST_ARGS=()
  RAW_RUST_ARGS="$*"

  local after_double_dash=false
  local arg
  for arg in "$@"; do
    if ! $after_double_dash && [[ "$arg" == "--" ]]; then
      after_double_dash=true
      continue
    fi

    if $after_double_dash; then
      TEST_ARGS+=("$arg")
    else
      CARGO_ARGS+=("$arg")
    fi
  done

}

append_test_threads_if_missing() {
  local arg
  for arg in "${TEST_ARGS[@]+"${TEST_ARGS[@]}"}"; do
    if [[ "$arg" == "--test-threads="* ]]; then
      return
    fi
  done
  TEST_ARGS+=("--test-threads=1")
}

native_workspace_host_running() {
  local pid_file=".dev/g5d-host/host.pid"
  local pid

  [[ -f "$pid_file" ]] || return 1
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  [[ -n "$pid" ]] || return 1
  ps -o command= -p "$pid" 2>/dev/null | grep -q "g5d host"
}

run_dev_rust_e2e() {
  require_container "$DEV_API_CONTAINER"
  require_container "$DEV_POSTGRES_CONTAINER"
  require_container "$DEV_OAUTH_CONTAINER"

  if ! native_workspace_host_running; then
    echo "ERROR: no running bound workspace host found for the shared dev stack." >&2
    echo "Checked the native host PID under '.dev/g5d-host'." >&2
    echo "Hint: ensure 'just dev-up' completed successfully and the host is still running." >&2
    exit 1
  fi

  if ! curl -fsS "${G5_E2E_API_URL}/healthz" >/dev/null; then
    echo "ERROR: g5-dev API is not healthy at ${G5_E2E_API_URL}" >&2
    exit 1
  fi

  seed_dev_fixture
  if [[ -z "${G5_E2E_JWT_PRIVATE_KEY:-}" ]]; then
    source "$SCRIPT_DIR/lib/local-stack-secrets.sh"
    g5_load_local_stack_secrets
  fi
  prepare_rust_args "$@"

  if [[ -z "$RAW_RUST_ARGS" ]]; then
    echo "Running retained Rust E2E scenarios against the shared workspace-VM stack..."
  else
    echo "Running targeted Rust E2E slice against shared dev stack..."
  fi

  echo "Resolved shared-dev E2E URLs:"
  echo "  API:    $G5_E2E_API_URL"
  echo "  DB:     $G5_E2E_DATABASE_URL"
  echo "  OAuth:  $G5_E2E_OAUTH_STUB_URL"
  echo "  Org:    ${G5_E2E_TEST_ORG_ID:-unset}"
  echo "  WS:     ${G5_E2E_TEST_WORKSPACE_ID:-unset}"

  if [[ -z "$RAW_RUST_ARGS" ]]; then
    SQLX_OFFLINE=true ./scripts/run-rust-e2e-lanes.sh
    return
  fi

  append_test_threads_if_missing

  if [[ ${#CARGO_ARGS[@]} -gt 0 ]]; then
    SQLX_OFFLINE=true \
    cargo test -p g5-e2e "${CARGO_ARGS[@]}" -- "${TEST_ARGS[@]+"${TEST_ARGS[@]}"}"
  else
    SQLX_OFFLINE=true \
    cargo test -p g5-e2e -- "${TEST_ARGS[@]+"${TEST_ARGS[@]}"}"
  fi
}

playwright_args_have_project() {
  local arg
  for arg in "$@"; do
    if [[ "$arg" == --project=* || "$arg" == "--project" ]]; then
      return 0
    fi
  done
  return 1
}

reset_oauth_stub_identity() {
  curl -fsS -X POST "${E2E_OAUTH_STUB_URL}/__e2e/identity/reset" >/dev/null 2>&1 || true
}

load_dev_oauth_identity_overrides() {
  local identity_json identity_id identity_login identity_email identity_name

  identity_json="$(curl -fsS "${E2E_OAUTH_STUB_URL}/__e2e/identity" 2>/dev/null || true)"
  if [[ -z "$identity_json" ]]; then
    return
  fi

  identity_id="$(printf '%s' "$identity_json" | jq -r '.user.id // empty' 2>/dev/null)"
  identity_login="$(printf '%s' "$identity_json" | jq -r '.user.login // empty' 2>/dev/null)"
  identity_email="$(printf '%s' "$identity_json" | jq -r '.user.email // empty' 2>/dev/null)"
  identity_name="$(printf '%s' "$identity_json" | jq -r '.user.name // empty' 2>/dev/null)"

  if [[ -n "$identity_id" && -z "${E2E_OAUTH_EXISTING_USER_ID:-}" ]]; then
    export E2E_OAUTH_EXISTING_USER_ID="$identity_id"
  fi
  if [[ -n "$identity_login" && -z "${E2E_OAUTH_EXISTING_USER_LOGIN:-}" ]]; then
    export E2E_OAUTH_EXISTING_USER_LOGIN="$identity_login"
  fi
  if [[ -n "$identity_email" && -z "${E2E_OAUTH_EXISTING_USER_EMAIL:-}" ]]; then
    export E2E_OAUTH_EXISTING_USER_EMAIL="$identity_email"
  fi
  if [[ -n "$identity_name" && -z "${E2E_OAUTH_EXISTING_USER_NAME:-}" ]]; then
    export E2E_OAUTH_EXISTING_USER_NAME="$identity_name"
  fi
}

run_dev_playwright() {
  require_container "$DEV_API_CONTAINER"
  require_container "$DEV_EDGE_CONTAINER"
  require_container "$DEV_OAUTH_CONTAINER"

  if ! curl -fsS "${G5_E2E_API_URL}/healthz" >/dev/null; then
    echo "ERROR: g5-dev API is not healthy at ${G5_E2E_API_URL}" >&2
    exit 1
  fi

  if ! curl -fsS "${E2E_BASE_URL}/healthz" >/dev/null; then
    echo "ERROR: g5-dev edge is not healthy at ${E2E_BASE_URL}" >&2
    exit 1
  fi

  seed_dev_fixture
  configure_playwright_fixture_aliases
  load_dev_oauth_identity_overrides
  local scm_github_api_base_url
  scm_github_api_base_url="$(docker exec "$DEV_API_CONTAINER" printenv G5_SCM_GITHUB_API_BASE_URL 2>/dev/null || true)"
  if [[ "$scm_github_api_base_url" == "http://oauth-stub:9876" ]]; then
    export G5_E2E_SCM_PAT_STUB_ENABLED=true
  fi
  trap reset_oauth_stub_identity EXIT

  local -a pw_args=("$@")
  if ! playwright_args_have_project "${pw_args[@]+"${pw_args[@]}"}"; then
    if [[ ${#pw_args[@]} -gt 0 ]]; then
      pw_args=(--project=chromium "${pw_args[@]}")
    else
      pw_args=(--project=chromium)
    fi
  fi

  echo "Running Playwright against shared dev stack..."
  echo "  Base URL: $E2E_BASE_URL"
  echo "  API URL:  $E2E_API_URL"
  echo "  OAuth:    $E2E_OAUTH_STUB_URL"
  echo "  OAuth user: ${E2E_OAUTH_EXISTING_USER_EMAIL:-unset}"

  (
    cd frontend
    E2E_BASE_URL="$E2E_BASE_URL" \
    E2E_API_URL="$E2E_API_URL" \
    E2E_OAUTH_STUB_URL="$E2E_OAUTH_STUB_URL" \
    E2E_USE_OAUTH="${E2E_USE_OAUTH:-true}" \
    npx playwright test "${pw_args[@]+"${pw_args[@]}"}"
  )
}

MODE="${1:-}"
if [[ -z "$MODE" ]]; then
  usage
  exit 1
fi
shift || true

if [[ "$MODE" != "rust" && "$MODE" != "playwright" ]]; then
  usage
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

DEV_API_CONTAINER="g5-dev-api"
DEV_EDGE_CONTAINER="g5-dev-edge"
DEV_POSTGRES_CONTAINER="g5-dev-postgres"
DEV_OAUTH_CONTAINER="g5-dev-oauth-stub"
require_container "$DEV_API_CONTAINER"
require_container "$DEV_EDGE_CONTAINER"
require_container "$DEV_POSTGRES_CONTAINER"
require_container "$DEV_OAUTH_CONTAINER"

API_PORT="$(resolve_host_port "$DEV_API_CONTAINER" 7500/tcp)"
EDGE_PORT="$(resolve_host_port "$DEV_EDGE_CONTAINER" 80/tcp)"
POSTGRES_PORT="$(resolve_host_port "$DEV_POSTGRES_CONTAINER" 5432/tcp)"
OAUTH_PORT="$(resolve_host_port "$DEV_OAUTH_CONTAINER" 9876/tcp)"

export G5_E2E_API_URL="${G5_E2E_API_URL:-http://localhost:${API_PORT}}"
export E2E_API_URL="${E2E_API_URL:-$G5_E2E_API_URL}"
export E2E_BASE_URL="${E2E_BASE_URL:-http://localhost:${EDGE_PORT}}"
export G5_E2E_DATABASE_URL="${G5_E2E_DATABASE_URL:-postgres://g5:g5_dev@127.0.0.1:${POSTGRES_PORT}/g5_dev}"
export E2E_DATABASE_URL="${E2E_DATABASE_URL:-$G5_E2E_DATABASE_URL}"
export E2E_API_CONTAINER="${E2E_API_CONTAINER:-$DEV_API_CONTAINER}"
export E2E_REPOS_BASE_DIR="${E2E_REPOS_BASE_DIR:-/mnt/g5-repos/repos}"
export G5_E2E_OAUTH_STUB_URL="${G5_E2E_OAUTH_STUB_URL:-http://127.0.0.1:${OAUTH_PORT}}"
export E2E_OAUTH_STUB_URL="${E2E_OAUTH_STUB_URL:-$G5_E2E_OAUTH_STUB_URL}"

if [[ "$MODE" == "rust" ]]; then
  run_dev_rust_e2e "$@"
else
  run_dev_playwright "$@"
fi
