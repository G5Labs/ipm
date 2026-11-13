#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPER="${SCRIPT_DIR}/apt-install-with-retry.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

mkdir -p "$TEST_ROOT/bin"
FAKE_APT="$TEST_ROOT/bin/apt-get"
cat >"$FAKE_APT" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

operation=""
for argument in "$@"; do
    case "$argument" in
        update|install) operation="$argument"; break ;;
    esac
done

if [ "$operation" = "update" ]; then
    exit 0
fi

if [ "$operation" != "install" ]; then
    echo "fake apt-get received no supported operation: $*" >&2
    exit 64
fi

has_no_recommends=0
has_curl=0
for argument in "$@"; do
    case "$argument" in
        --no-install-recommends) has_no_recommends=1 ;;
        curl) has_curl=1 ;;
    esac
done
if [ "$has_no_recommends" -ne 1 ] || [ "$has_curl" -ne 1 ]; then
    echo "install invocation omitted required flags or package: $*" >&2
    exit 64
fi

attempt=0
if [ -f "$FAKE_APT_STATE" ]; then
    attempt="$(<"$FAKE_APT_STATE")"
fi
attempt=$((attempt + 1))
printf '%s\n' "$attempt" >"$FAKE_APT_STATE"

if [ "${FAKE_APT_INSTALL_SLEEP_SECONDS:-0}" -gt 0 ]; then
    sleep "$FAKE_APT_INSTALL_SLEEP_SECONDS"
fi

if [ "$attempt" -le "$FAKE_APT_INSTALL_FAILURES" ]; then
    exit 100
fi
EOF
chmod +x "$FAKE_APT"

FAKE_DPKG_QUERY="$TEST_ROOT/bin/dpkg-query"
cat >"$FAKE_DPKG_QUERY" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
package="${*: -1}"
if [ "$package" = "${FAKE_DPKG_QUERY_MISSING:-}" ]; then
    exit 1
fi
printf '%s\n' "$*" >>"$FAKE_DPKG_QUERY_STATE"
printf 'ii '
EOF
chmod +x "$FAKE_DPKG_QUERY"

preinstalled_state="$TEST_ROOT/preinstalled-state"
PATH="$TEST_ROOT/bin:$PATH" \
FAKE_DPKG_QUERY_STATE="$preinstalled_state" \
APT_PACKAGES_PREINSTALLED=1 \
    "$HELPER" curl ca-certificates

if [ -e "$TEST_ROOT/retry-state" ]; then
    echo "preinstalled package verification unexpectedly invoked apt-get" >&2
    exit 1
fi
if [ "$(wc -l <"$preinstalled_state" | tr -d ' ')" -ne 2 ]; then
    echo "expected every preinstalled package to be verified" >&2
    exit 1
fi

missing_state="$TEST_ROOT/missing-state"
if PATH="$TEST_ROOT/bin:$PATH" \
    FAKE_DPKG_QUERY_STATE="$missing_state" \
    FAKE_DPKG_QUERY_MISSING=ca-certificates \
    APT_PACKAGES_PREINSTALLED=1 \
        "$HELPER" curl ca-certificates; then
    echo "expected preinstalled package verification to fail closed" >&2
    exit 1
fi

retry_state="$TEST_ROOT/retry-state"
PATH="$TEST_ROOT/bin:$PATH" \
FAKE_APT_STATE="$retry_state" \
FAKE_APT_INSTALL_FAILURES=2 \
APT_RETRY_ATTEMPTS=4 \
APT_RETRY_INITIAL_DELAY_SECONDS=0 \
    "$HELPER" curl

if [ "$(<"$retry_state")" -ne 3 ]; then
    echo "expected success on the third install attempt" >&2
    exit 1
fi

failure_state="$TEST_ROOT/failure-state"
if PATH="$TEST_ROOT/bin:$PATH" \
    FAKE_APT_STATE="$failure_state" \
    FAKE_APT_INSTALL_FAILURES=4 \
    APT_RETRY_ATTEMPTS=3 \
    APT_RETRY_INITIAL_DELAY_SECONDS=0 \
        "$HELPER" curl; then
    echo "expected helper to fail after exhausting its retry budget" >&2
    exit 1
fi

if [ "$(<"$failure_state")" -ne 3 ]; then
    echo "expected exactly three bounded install attempts" >&2
    exit 1
fi

timeout_state="$TEST_ROOT/timeout-state"
if PATH="$TEST_ROOT/bin:$PATH" \
    FAKE_APT_STATE="$timeout_state" \
    FAKE_APT_INSTALL_FAILURES=0 \
    FAKE_APT_INSTALL_SLEEP_SECONDS=5 \
    APT_RETRY_ATTEMPTS=1 \
    APT_RETRY_ATTEMPT_TIMEOUT_SECONDS=1 \
        "$HELPER" curl; then
    echo "expected a hung package bootstrap attempt to time out" >&2
    exit 1
fi

if [ "$(<"$timeout_state")" -ne 1 ]; then
    echo "expected the timed-out install to start exactly once" >&2
    exit 1
fi

echo "apt install retry tests passed"
