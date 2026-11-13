#!/usr/bin/env bash
# Executable contract for scripts/ci-run-playwright-selector.sh (deployment
# g5.quality.delivery.ci.provider-agent-parallel). Runs the real script
# against a fake ./node_modules/.bin/playwright that records its argv and
# asserts the invocation shape per selector and the combined exit status.
set -euo pipefail
cd "$(dirname "$0")/.."

SCRIPT="$PWD/scripts/ci-run-playwright-selector.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT
mkdir -p "$TEST_ROOT/node_modules/.bin"
cat >"$TEST_ROOT/node_modules/.bin/playwright" <<'FAKE'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_LOG"
# FAKE_FAIL_GREP: fail the invocation whose argv contains this substring.
if [ -n "${FAKE_FAIL_GREP:-}" ] && [[ "$*" == *"$FAKE_FAIL_GREP"* ]]; then exit 1; fi
exit 0
FAKE
chmod +x "$TEST_ROOT/node_modules/.bin/playwright"

run() {
  # $1 = case name; remaining = env assignments
  local name="$1"; shift
  FAKE_LOG="$TEST_ROOT/$name.log"; : > "$FAKE_LOG"
  set +e
  ( cd "$TEST_ROOT" && env FAKE_LOG="$FAKE_LOG" "$@" bash "$SCRIPT" "$SELECTOR" >"$TEST_ROOT/$name.out" 2>&1 )
  STATUS=$?
  set -e
}
fail() { echo "FAIL: $*" >&2; cat "$FAKE_LOG" >&2; exit 1; }

# 1. Restart-continuity: parallel pass excludes the API-restart test, then a
#    serial one-worker pass runs only that test.
SELECTOR=e2e/flows/chat-restart-continuity.spec.ts
run split G5_PA_PLAYWRIGHT_WORKERS=2
[ "$STATUS" -eq 0 ] || fail "split run exited $STATUS"
[ "$(wc -l < "$FAKE_LOG" | tr -d ' ')" -eq 2 ] || fail "expected exactly two playwright invocations"
sed -n 1p "$FAKE_LOG" | grep -q -- "--workers=2 --grep-invert M6/M14 API restart $SELECTOR" || fail "first pass must be parallel and exclude the API-restart test"
sed -n 2p "$FAKE_LOG" | grep -q -- "--workers=1 --grep M6/M14 API restart $SELECTOR" || fail "second pass must run only the API-restart test on one worker"
grep -q -- "--project=chromium --no-deps" "$FAKE_LOG" || fail "project/no-deps flags must be preserved"

# 2. A failure in either pass fails the selector; the serial pass still runs.
run split-fail-first G5_PA_PLAYWRIGHT_WORKERS=2 FAKE_FAIL_GREP=--grep-invert
[ "$STATUS" -ne 0 ] || fail "parallel-pass failure must fail the selector"
[ "$(wc -l < "$FAKE_LOG" | tr -d ' ')" -eq 2 ] || fail "serial pass must still run after a parallel-pass failure"
run split-fail-second G5_PA_PLAYWRIGHT_WORKERS=2 FAKE_FAIL_GREP="--grep M6"
[ "$STATUS" -ne 0 ] || fail "serial-pass failure must fail the selector"

# 3. Default worker count is one (the historical serial lane).
run split-default
sed -n 1p "$FAKE_LOG" | grep -q -- "--workers=1 --grep-invert" || fail "default must be one worker"

# 4. native-requests: one parallel pass, no grep.
SELECTOR=e2e/flows/native-requests.spec.ts
run native G5_PA_PLAYWRIGHT_WORKERS=2
[ "$STATUS" -eq 0 ] || fail "native run exited $STATUS"
[ "$(wc -l < "$FAKE_LOG" | tr -d ' ')" -eq 1 ] || fail "native-requests must be a single invocation"
grep -q -- "--workers=2 $SELECTOR" "$FAKE_LOG" || fail "native-requests must use the parallel worker count"
grep -q -- "--grep" "$FAKE_LOG" && fail "native-requests must not be filtered"

# 5. Any other selector: serial on one worker regardless of the env.
SELECTOR=e2e/flows/other.spec.ts
run other G5_PA_PLAYWRIGHT_WORKERS=4
grep -q -- "--workers=1 $SELECTOR" "$FAKE_LOG" || fail "unknown selectors must stay serial"

# 6. The grep stays in sync with the spec's restart test title.
grep -q 'test("M6/M14 API restart' frontend/e2e/flows/chat-restart-continuity.spec.ts || fail "the API-restart test title changed; update API_RESTART_GREP"
grep -q 'mode: "parallel"' frontend/e2e/flows/chat-restart-continuity.spec.ts || fail "chat-restart-continuity must opt into parallel mode"
grep -q 'mode: "parallel"' frontend/e2e/flows/native-requests.spec.ts || fail "native-requests must opt into parallel mode"

echo "PASS: ci-run-playwright-selector.sh contract (6 cases)"
