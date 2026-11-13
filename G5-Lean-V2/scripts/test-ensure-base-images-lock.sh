#!/usr/bin/env bash
# Executable contract for the base-image build lock in
# scripts/ensure-base-images.sh (software_architecture/integration-stack/
# docker_images [D21]; test_contract/base_image_consolidation [D13]).
#
# Runs the REAL script with `docker` shadowed on PATH (tier absent, build is a
# no-op success) and a scratch G5_BASE_LOCK_DIR, and stages each lock race
# deterministically with a PATH-shadowed `stat` that mutates the lock on its
# first call before delegating to the real binary — exactly the window a
# waiter hits when a sibling releases between the failed `mkdir` and the age
# read (dev deploy builds 2f51f728 / 74214325).
#
# Run by the ci-rust `image-build-script-contracts` step; also runnable
# locally from  (needs only bash + coreutils; no Docker).
set -euo pipefail
cd "$(dirname "$0")/.."

SCRIPT="$PWD/scripts/ensure-base-images.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

REAL_STAT="$(PATH=/usr/bin:/bin command -v stat)"
[ -n "$REAL_STAT" ] || { echo "FAIL: no real stat binary found" >&2; exit 1; }

mkdir -p "$TEST_ROOT/bin"
cat >"$TEST_ROOT/bin/docker" <<'FAKE'
#!/usr/bin/env bash
# Fake docker: every tier is absent (inspect fails) and every build succeeds.
case "${1:-}:${2:-}" in
  image:inspect) exit 1 ;;
  build:*)       exit 0 ;;
  *)             exit 0 ;;
esac
FAKE
chmod +x "$TEST_ROOT/bin/docker"

# Fake stat: on its FIRST invocation run the staged mutation ($STAT_HOOK),
# then (always) delegate to the real stat so age arithmetic stays real.
cat >"$TEST_ROOT/bin/stat" <<FAKE
#!/usr/bin/env bash
if [ -n "\${STAT_HOOK:-}" ] && [ ! -e "\$STAT_HOOK_FIRED" ]; then
  : > "\$STAT_HOOK_FIRED"
  eval "\$STAT_HOOK"
fi
exec "$REAL_STAT" "\$@"
FAKE
chmod +x "$TEST_ROOT/bin/stat"

run_case() {
  # $1 = case name, $2 = STAT_HOOK (may be empty); stdout/stderr → $OUT
  local name="$1" hook="$2"
  OUT="$TEST_ROOT/$name.out"
  rm -f "$TEST_ROOT/$name.fired"
  set +e
  env PATH="${CASE_PATH:+$CASE_PATH:}$TEST_ROOT/bin:$PATH" \
      STAT_HOOK="$hook" STAT_HOOK_FIRED="$TEST_ROOT/$name.fired" \
      G5_BASE_LOCK_DIR="$LOCK" G5_BASE_LOCK_STALE_AFTER="${STALE:-300}" \
      bash "$SCRIPT" base >"$OUT" 2>&1
  STATUS=$?
  set -e
}

fail() { echo "FAIL: $*" >&2; [ -n "${OUT:-}" ] && sed 's/^/  | /' "$OUT" >&2; exit 1; }

# ---------------------------------------------------------------------------
# (a) The lock directory vanishes between the failed mkdir and the age read.
LOCK="$TEST_ROOT/case-a.lock"
mkdir -p "$LOCK"; : > "$LOCK/heartbeat"; printf 'someone-else' > "$LOCK/owner"
run_case a "rm -rf '$LOCK'"
[ "$STATUS" -eq 0 ] || fail "(a) released-mid-poll lock must be acquired (exit $STATUS)"
grep -q "Base image tiers ensured: base" "$OUT" || fail "(a) script did not reach the tier build"
grep -q "no heartbeat" "$OUT" && fail "(a) released lock was misreported as stale"
[ ! -d "$LOCK" ] || fail "(a) lock directory left behind after release"
echo "PASS (a): lock released between mkdir and age read is retried, not fatal"

# ---------------------------------------------------------------------------
# (b) Release in progress: heartbeat+owner vanish first, the directory a
#     moment later (a background sibling finishes its rmdir 2s in).
LOCK="$TEST_ROOT/case-b.lock"
mkdir -p "$LOCK"; : > "$LOCK/heartbeat"; printf 'someone-else' > "$LOCK/owner"
( sleep 2; rmdir "$LOCK" 2>/dev/null || rm -rf "$LOCK" ) &
run_case b "rm -f '$LOCK/heartbeat' '$LOCK/owner'"
wait
[ "$STATUS" -eq 0 ] || fail "(b) waiter must acquire after an in-progress release completes (exit $STATUS)"
grep -q "no heartbeat" "$OUT" && fail "(b) in-progress release was misreported as stale"
echo "PASS (b): heartbeat-gone-but-directory-present is one observation, not a verdict"

# ---------------------------------------------------------------------------
# (c) A genuinely silent lock (dead holder) still fails loud, and the lock is
#     left for a human: heartbeat older than G5_BASE_LOCK_STALE_AFTER, nobody
#     refreshing it, directory present on every poll.
LOCK="$TEST_ROOT/case-c.lock"
mkdir -p "$LOCK"; : > "$LOCK/heartbeat"; printf 'dead-holder' > "$LOCK/owner"
sleep 3
STALE=1 run_case c ""
[ "$STATUS" -eq 1 ] || fail "(c) silent lock must fail loud with exit 1 (exit $STATUS)"
grep -q "has had no heartbeat for" "$OUT" || fail "(c) missing the stale-lock error"
grep -q "rm -rf '$LOCK'" "$OUT" || fail "(c) missing the exact cleanup command"
grep -q "999999" "$OUT" && fail "(c) stale age must be a measured age, not the unreadable sentinel"
[ -d "$LOCK" ] || fail "(c) a silent lock must be left in place for the human"
echo "PASS (c): silent holder fails loud with the cleanup command, lock untouched"

# ---------------------------------------------------------------------------
# (d) A live holder heartbeats, then releases normally; the waiter waits and
#     then acquires.
LOCK="$TEST_ROOT/case-d.lock"
mkdir -p "$LOCK"; : > "$LOCK/heartbeat"; printf 'live-holder' > "$LOCK/owner"
( for _ in 1 2 3; do sleep 1; : > "$LOCK/heartbeat"; done
  rm -f "$LOCK/heartbeat" "$LOCK/owner"; rmdir "$LOCK" ) &
run_case d ""
wait
[ "$STATUS" -eq 0 ] || fail "(d) waiter must acquire after a normal release (exit $STATUS)"
grep -q "Waiting for a concurrent base-image build" "$OUT" || fail "(d) waiter did not report waiting"
grep -q "Base image tiers ensured: base" "$OUT" || fail "(d) script did not reach the tier build"
echo "PASS (d): normal wait-then-acquire"

# ---------------------------------------------------------------------------
# (e) uutils coreutils' `mkdir` (Ubuntu 26.04) reports success to callers
#     racing to create the same directory. Shadowed here by a `mkdir` that
#     always succeeds on an existing directory, a waiter must still wait for
#     the live holder rather than take its lock (CI e2e build 2d7f8a6b).
LOCK="$TEST_ROOT/case-e.lock"
mkdir -p "$TEST_ROOT/racy-mkdir"
cat >"$TEST_ROOT/racy-mkdir/mkdir" <<'FAKE'
#!/usr/bin/env bash
exec env PATH=/usr/bin:/bin mkdir -p "$@"
FAKE
chmod +x "$TEST_ROOT/racy-mkdir/mkdir"
mkdir -p "$LOCK"; : > "$LOCK/heartbeat"; printf 'live-holder' > "$LOCK/owner"
( for _ in 1 2 3; do sleep 1; : > "$LOCK/heartbeat"
    [ "$(cat "$LOCK/owner")" = live-holder ] || echo stolen > "$TEST_ROOT/case-e.stolen"
  done
  rm -f "$LOCK/heartbeat" "$LOCK/owner"; rmdir "$LOCK" ) &
CASE_PATH="$TEST_ROOT/racy-mkdir" run_case e ""
wait
[ ! -e "$TEST_ROOT/case-e.stolen" ] || fail "(e) a waiter overwrote the live holder's owner token"
[ "$STATUS" -eq 0 ] || fail "(e) waiter must acquire after the holder releases (exit $STATUS)"
grep -q "Waiting for a concurrent base-image build" "$OUT" || fail "(e) waiter took a held lock instead of waiting"
grep -q "Base image tiers ensured: base" "$OUT" || fail "(e) script did not reach the tier build"
echo "PASS (e): a mkdir that succeeds on an existing directory does not take a held lock"

echo "ensure-base-images lock tests passed"
