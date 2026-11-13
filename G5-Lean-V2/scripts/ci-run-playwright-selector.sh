#!/usr/bin/env bash
# Run one value-loop Playwright selector on the provider-agent lane.
#
# Usage: ci-run-playwright-selector.sh <selector> [playwright args...]
#   Run from the lane's Playwright client directory (./node_modules/.bin/playwright).
#
# Environment:
#   G5_PA_PLAYWRIGHT_WORKERS  Workers for the parallel-safe part of a selector
#                             (default 1, i.e. the historical serial lane).
#
# deployment g5.quality.delivery.ci.provider-agent-parallel: each test in
# chat-restart-continuity.spec.ts and native-requests.spec.ts clones and boots
# its own workspace VM, so they run on several workers — except the one test
# that `docker restart`s the lane API, which would interrupt every other test
# in flight. That test runs alone, after the parallel pass, on one worker.
# Both passes must pass for the selector to pass; the receipt records the
# selector as one unit.
set -uo pipefail

selector="${1:?selector}"
shift
workers="${G5_PA_PLAYWRIGHT_WORKERS:-1}"
pw=(./node_modules/.bin/playwright test --project=chromium --no-deps "$@")

# Title prefix of the only test that restarts the lane API; keep in sync with
# frontend/e2e/flows/chat-restart-continuity.spec.ts (restartAssignedApi).
API_RESTART_GREP='M6/M14 API restart'

case "$selector" in
  e2e/flows/chat-restart-continuity.spec.ts)
    echo "[selector] $selector: parallel pass with $workers worker(s), excluding '$API_RESTART_GREP'"
    "${pw[@]}" --workers="$workers" --grep-invert "$API_RESTART_GREP" "$selector"
    rc=$?
    echo "[selector] $selector: serial pass for '$API_RESTART_GREP' on one worker"
    "${pw[@]}" --workers=1 --grep "$API_RESTART_GREP" "$selector"
    rc2=$?
    [[ $rc -eq 0 && $rc2 -eq 0 ]]
    ;;
  e2e/flows/native-requests.spec.ts)
    echo "[selector] $selector: parallel pass with $workers worker(s)"
    "${pw[@]}" --workers="$workers" "$selector"
    ;;
  *)
    echo "[selector] $selector: serial pass on one worker"
    "${pw[@]}" --workers=1 "$selector"
    ;;
esac
