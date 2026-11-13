#!/usr/bin/env bash
# Run against an already provisioned, exclusively owned Linux KVM E2E stack.
# This script does not start services, seed data, install tools, or change hosts.
# Start its bound host(s) with G5D_WORKLOAD_ROOT=block and enough total scratch
# capacity for the guest image plus two simultaneous workspaces. The API + PTY
# layout test, not the caller's environment, proves which backend actually ran.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/test-block-workload-e2e.sh --layout-only|--docker-only|--full

All modes require an owned isolated stack, its seeded two-member workspace,
and at least two VM slots for --layout-only/--full:
  G5_E2E_BLOCK_ISOLATED_STACK=true
  G5_E2E_API_URL, G5D_ORG_ID
  G5_E2E_AUTH_MODE=oauth and G5_E2E_OAUTH_STUB_URL (or enabled test-token auth)

--layout-only: real block mounts, read-only executable runtime, Files/terminal,
               two live VMs on one host, Stop/Start receipt restore.
--docker-only: a Docker-capable guest image on the bound block host, plus
               G5_E2E_BLOCK_DOCKER_IMAGE=image@sha256:<64 hex digits> (/bin/sh).
--full: both above and existing selected Save, dirty Fork, Files/Changes,
        terminal, preview, API-restart and host-loss journeys. Also requires:
  G5_E2E_API_RESTART_CONTAINER, G5_E2E_LIVE_HOST_CONTAINER,
  G5_E2E_DATABASE_URL, G5_E2E_JWT_PRIVATE_KEY (existing stack signer),
  E2E_BASE_URL, E2E_API_URL, E2E_OAUTH_STUB_URL,
  VITE_E2E_ORG, VITE_E2E_SELECTED_ORG (owned fixture org slugs),
  Docker access to ONLY this stack, installed frontend dependencies + Chromium.

Use --layout-only for the ordinary buildpack-deps guest; it does not qualify
Docker. --full requires a Docker-capable guest and fails on skipped browser
tests. Existing recovery helpers independently check their container allowlists.
No mode should target shared QA or a developer's active workspace stack.
USAGE
}

mode="${1:-}"
case "$mode" in
  --layout-only|--docker-only|--full) [[ $# == 1 ]] || { usage; exit 2; } ;;
  --help|-h) usage; exit 0 ;;
  *) usage >&2; exit 2 ;;
esac

require_env() {
  [[ -n "${!1:-}" ]] || { echo "ERROR: required environment variable $1 is absent" >&2; exit 2; }
}
[[ "${G5_E2E_BLOCK_ISOLATED_STACK:-}" == true ]] || {
  echo 'ERROR: set G5_E2E_BLOCK_ISOLATED_STACK=true only for an exclusively owned test stack' >&2
  exit 2
}
require_env G5_E2E_API_URL
require_env G5D_ORG_ID
if [[ "${G5_E2E_AUTH_MODE:-}" == oauth ]]; then
  require_env G5_E2E_OAUTH_STUB_URL
fi
if [[ "$mode" != --layout-only ]]; then
  require_env G5_E2E_BLOCK_DOCKER_IMAGE
  [[ "$G5_E2E_BLOCK_DOCKER_IMAGE" =~ ^[^[:space:]]+@sha256:[[:xdigit:]]{64}$ ]] || {
    echo 'ERROR: G5_E2E_BLOCK_DOCKER_IMAGE must be a pinned trusted image with /bin/sh' >&2
    exit 2
  }
fi
if [[ "$mode" == --full ]]; then
  for variable in G5_E2E_API_RESTART_CONTAINER G5_E2E_LIVE_HOST_CONTAINER \
    G5_E2E_DATABASE_URL G5_E2E_JWT_PRIVATE_KEY \
    E2E_BASE_URL E2E_API_URL E2E_OAUTH_STUB_URL \
    VITE_E2E_ORG VITE_E2E_SELECTED_ORG; do
    require_env "$variable"
  done
  [[ "${E2E_API_URL%/}" == "${G5_E2E_API_URL%/}" ]] || {
    echo 'ERROR: Rust and browser qualification must use the same isolated API URL' >&2
    exit 2
  }
  [[ "$VITE_E2E_ORG" == "$VITE_E2E_SELECTED_ORG" ]] || {
    echo 'ERROR: browser auth and journeys must select the same owned fixture org' >&2
    exit 2
  }
  docker info >/dev/null
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_DIR"
export G5_E2E_BLOCK_WORKLOAD=true

run_test() {
  local selector="$1"
  shift
  # Exact test names plus enumeration prevent an accidentally empty selector
  # from looking like a successful qualification after a rename.
  local listed
  listed="$(cargo test -p g5-e2e "$selector" -- --list --exact)"
  [[ "$listed" == *"$selector: test"* ]] || {
    echo "ERROR: required test selector does not exist: $selector" >&2
    exit 1
  }
  cargo test -p g5-e2e "$selector" -- --exact --test-threads=1 --nocapture "$@"
}

if [[ "$mode" != --docker-only ]]; then
  run_test scenarios::block_workload::test_block_workload_layout_files_isolation_and_restore
fi
if [[ "$mode" != --layout-only ]]; then
  run_test scenarios::block_workload::test_block_workload_docker_overlay_on_block --ignored
fi
if [[ "$mode" != --full ]]; then
  echo "Completed $mode only; the other block acceptance journeys were not run."
  exit 0
fi

run_test scenarios::fork_recovery::test_failed_live_flush_then_api_restart_forks_one_complete_child
run_test scenarios::fork_recovery::test_oversized_stop_kills_the_vm_and_start_restores_the_last_receipt
run_test scenarios::r2_guest_lifecycle::test_guest_terminal_reconnects_and_stop_fences_the_session
run_test scenarios::r2_guest_lifecycle::test_guest_terminal_transfer_fences_old_owner_and_resets_identity

report_dir="${G5_E2E_BLOCK_REPORT_DIR:-$REPO_DIR/target/block-workload-e2e}"
mkdir -p "$report_dir"
report_dir="$(cd "$report_dir" && pwd)"
(
  cd frontend
  # Use the installed candidate dependencies; npx must not fetch a different version.
  test -x node_modules/.bin/playwright
  E2E_USE_OAUTH=true PLAYWRIGHT_JSON_OUTPUT_FILE="$report_dir/browser.json" \
    node_modules/.bin/playwright test --project=chromium --workers=1 --reporter=list,json \
    e2e/flows/files-source-editor.spec.ts \
    e2e/flows/changes-and-push.spec.ts \
    e2e/flows/explicit-save.spec.ts \
    e2e/flows/workspace-fork-outcomes.spec.ts \
    e2e/flows/workspace-fork-isolation.spec.ts \
    e2e/flows/workspace-vm.spec.ts \
    e2e/flows/workspace-preview.spec.ts \
    e2e/flows/terminal.spec.ts
)
node - "$report_dir/browser.json" <<'JS'
const fs = require('fs');
const report = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (!(report.stats.expected > 0) || report.stats.skipped || report.stats.unexpected || report.stats.flaky) {
  throw new Error('Full block qualification requires browser passes without skips/flakes: ' + JSON.stringify(report.stats));
}
JS
# The destructive host-loss journey runs last and is confined by the existing
# helper's exact bound-container checks. It must never target a shared host.
run_test scenarios::host_loss_recovery::test_verified_receipt_restores_after_host_loss_and_fences_preview
echo "Completed full block workload qualification; browser report: $report_dir/browser.json"
