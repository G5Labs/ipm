#!/usr/bin/env bash
# Executable contract for the in-guest dockerd starter
# infra/docker/g5-dev-dockerd-start.sh (software_architecture/infra/docker_images
# [I32] single-flight bullet; test_contract/base_image_consolidation [D15]).
#
# Runs the REAL script (the exact file COPY'd into Dockerfile.g5d-g5dev as
# /usr/local/bin/g5-dev-dockerd-start) against PATH-shadowed
# id/docker/dockerd/findmnt/sudo/pgrep/getent doubles and the
# G5_DEV_DOCKERD_* env seams — the [D13] lock-oracle pattern. No VM, no
# root, no Docker.
#
# Run by the ci-rust `image-build-script-contracts` step; also runnable
# locally from  (bash + coreutils + util-linux flock).
set -euo pipefail
cd "$(dirname "$0")/../.."

SCRIPT="$PWD/infra/docker/g5-dev-dockerd-start.sh"
# Mode-independent on purpose: workspace materialization can strip
# working-tree exec bits, so the oracle requires only that the file EXISTS and
# runs it through bash. The exec bit of the INSTALLED script is asserted where
# it is authoritative — the Dockerfile `chmod 0755` + `test -x` build gate.
[ -f "$SCRIPT" ] || { echo "FAIL: $SCRIPT is missing" >&2; exit 1; }
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

mkdir -p "$TEST_ROOT/bin"
# The script re-execs through sudo unless `id -u` is 0: pretend to be root so
# the unprivileged oracle exercises the root code path.
cat >"$TEST_ROOT/bin/id" <<'FAKE'
#!/usr/bin/env bash
echo 0
FAKE
# `docker info` succeeds only once the staged readiness marker exists; while
# unready it prints the real client's connection error shape on stderr so the
# script's captured `docker info error` diagnostic carries content.
cat >"$TEST_ROOT/bin/docker" <<'FAKE'
#!/usr/bin/env bash
if [ "${1:-}" = "info" ]; then
  if [ -e "$DOCKER_READY_MARKER" ]; then exit 0; fi
  echo "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?" >&2
  exit 1
fi
exit 0
FAKE
# `dockerd` records every launch (plus whether the single-flight lock fd 9
# leaked into the daemon), optionally becomes ready after a short delay, and
# optionally stays alive without ever becoming ready (readiness-timeout
# scenarios need a LIVE unready daemon or the early-exit check fires).
cat >"$TEST_ROOT/bin/dockerd" <<'FAKE'
#!/usr/bin/env bash
if [ -e /proc/self/fd/9 ]; then fd9=open; else fd9=closed; fi
echo "dockerd launched fd9=$fd9" >> "$DOCKERD_LAUNCHES"
if [ -n "${DOCKERD_READY_AFTER:-}" ]; then
  sleep "$DOCKERD_READY_AFTER"
  touch "$DOCKER_READY_MARKER"
  sleep 5
elif [ -n "${DOCKERD_STAY_ALIVE:-}" ]; then
  sleep "$DOCKERD_STAY_ALIVE"
fi
FAKE
# Non-ext4 data root: the storage flip must pin vfs (and never ext4-only
# machinery); the flip itself is owned by microvm_runtime [D53].
cat >"$TEST_ROOT/bin/findmnt" <<'FAKE'
#!/usr/bin/env bash
echo tmpfs
FAKE
cat >"$TEST_ROOT/bin/sudo" <<'FAKE'
#!/usr/bin/env bash
exec "$@"
FAKE
# Deterministic pgrep: never sees the build host's real daemons. Reports a
# live fake dockerd when the scenario stages one, OR when this run itself
# launched the stay-alive/ready-after stub (tracked via the launches file so
# the launcher branch and the readiness loop see a consistent liveness view).
cat >"$TEST_ROOT/bin/pgrep" <<'FAKE'
#!/usr/bin/env bash
if [ "${2:-}" = "dockerd" ] && [ -n "${PGREP_DOCKERD_PID:-}" ]; then
  echo "$PGREP_DOCKERD_PID"
  exit 0
fi
exit 1
FAKE
# DNS probe double: resolves registry-1.docker.io unless GETENT_FAIL stages a
# resolution failure (the DNS/registry-failure scenario).
cat >"$TEST_ROOT/bin/getent" <<'FAKE'
#!/usr/bin/env bash
if [ -n "${GETENT_FAIL:-}" ]; then
  echo "getent: could not resolve ${2:-}" >&2
  exit 2
fi
echo "203.0.113.7 registry-1.docker.io"
FAKE
chmod +x "$TEST_ROOT/bin/"*

new_sandbox() {
  # $1 = case name; sets SB, LOCK, LOG, DAEMON_JSON, DATA_ROOT, MARKER, LAUNCHES
  SB="$TEST_ROOT/$1"
  mkdir -p "$SB/data-root"
  LOCK="$SB/start.lock"
  LOG="$SB/dockerd.log"
  DAEMON_JSON="$SB/daemon.json"
  DATA_ROOT="$SB/data-root"
  MARKER="$SB/ready"
  LAUNCHES="$SB/launches"
  RESOLV="$SB/resolv.conf"
  printf '{\n  "storage-driver": "vfs"\n}\n' > "$DAEMON_JSON"
  printf 'nameserver 8.8.8.8\nnameserver 1.1.1.1\noptions use-vc\n' > "$RESOLV"
}

run_script() {
  # Extra env as NAME=VALUE args; captures STATUS, writes stdout+stderr to $OUT
  OUT="$SB/out.$((++RUN_SEQ))"
  set +e
  env PATH="$TEST_ROOT/bin:$PATH" \
      DOCKER_READY_MARKER="$MARKER" DOCKERD_LAUNCHES="$LAUNCHES" \
      G5_DEV_DOCKERD_LOG="$LOG" G5_DEV_DOCKERD_LOCK="$LOCK" \
      G5_DEV_DOCKERD_DAEMON_JSON="$DAEMON_JSON" \
      G5_DEV_DOCKERD_DATA_ROOT="$DATA_ROOT" \
      G5_DEV_DOCKERD_RESOLV_CONF="$RESOLV" \
      "$@" bash "$SCRIPT" >"$OUT" 2>&1
  STATUS=$?
  set -e
}
RUN_SEQ=0

fail() { echo "FAIL: $*" >&2; [ -n "${OUT:-}" ] && [ -f "$OUT" ] && sed 's/^/  | /' "$OUT" >&2; exit 1; }

# ---------------------------------------------------------------------------
# (a) Two cold invocations race: exactly ONE dockerd launch, both exit 0 once
#     the stub daemon becomes ready, and the daemon never inherits the lock fd.
new_sandbox case-a
# Each racer runs in a background SUBSHELL: `run_script` deliberately keeps
# the caller alive (it stores the result in STATUS under set +e), so the
# subshell must explicitly exit with $STATUS or `wait` would always report 0
# and mask a failing starter (review round 2 P2).
( RUN_SEQ=100; run_script DOCKERD_READY_AFTER=1; exit "$STATUS" ) &
PID_A=$!
( RUN_SEQ=200; run_script DOCKERD_READY_AFTER=1; exit "$STATUS" ) &
PID_B=$!
set +e
wait "$PID_A"; STATUS_A=$?
wait "$PID_B"; STATUS_B=$?
set -e
OUT=""
[ "$STATUS_A" -eq 0 ] || fail "(a) first racer must exit 0 (exit $STATUS_A)"
[ "$STATUS_B" -eq 0 ] || fail "(a) second racer must exit 0 (exit $STATUS_B)"
[ -f "$LAUNCHES" ] || fail "(a) no dockerd launch was recorded at all"
LAUNCH_COUNT="$(wc -l < "$LAUNCHES")"
[ "$LAUNCH_COUNT" -eq 1 ] || fail "(a) expected exactly 1 dockerd launch, got $LAUNCH_COUNT: $(cat "$LAUNCHES")"
grep -q "fd9=closed" "$LAUNCHES" || fail "(a) dockerd inherited the single-flight lock fd: $(cat "$LAUNCHES")"
# Regression (review round 2 P2): a FAILING starter in a background subshell
# must surface its real exit code through `wait` — a masked 0 here would let
# every background assertion above pass vacuously.
new_sandbox case-a2
( RUN_SEQ=100; run_script G5_DEV_DOCKERD_READY_TIMEOUT=1 DOCKERD_STAY_ALIVE=5; exit "$STATUS" ) &
PID_FAIL=$!
set +e
wait "$PID_FAIL"; STATUS_FAIL=$?
set -e
OUT=""
[ "$STATUS_FAIL" -eq 65 ] || fail "(a2) background failure must propagate exit 65 through wait (got $STATUS_FAIL)"
echo "PASS (a): concurrent cold start launches one daemon, shares one readiness result (background failures propagate)"

# ---------------------------------------------------------------------------
# (b) A dockerd process already exists but readiness lags: the caller waits
#     for that daemon instead of launching another, and exits 0 when ready.
new_sandbox case-b
( sleep 2; touch "$MARKER" ) &
LATE_READY=$!
run_script PGREP_DOCKERD_PID=12345
wait "$LATE_READY"
[ "$STATUS" -eq 0 ] || fail "(b) waiter must exit 0 once the existing daemon is ready (exit $STATUS)"
[ ! -e "$LAUNCHES" ] || fail "(b) a second dockerd was launched beside the existing one: $(cat "$LAUNCHES")"
echo "PASS (b): existing not-yet-ready daemon is awaited, never doubled"

# ---------------------------------------------------------------------------
# (c) Readiness never arrives while the launched daemon stays ALIVE: exit 65
#     with phase+elapsed, the captured `docker info` error, the bounded
#     dockerd.log tail (100-line AND 8KiB caps held against an oversized
#     log), the failure-class context probes, and the DNS probes on stderr.
new_sandbox case-c
{
  for i in $(seq 1 299); do
    printf 'padline-%03d: %086d\n' "$i" 0
  done
  echo 'time=now level=error msg="failed to start daemon: iptables: nf_tables CHAIN_ADD failed"'
} > "$LOG"
run_script G5_DEV_DOCKERD_READY_TIMEOUT=2 DOCKERD_STAY_ALIVE=10
[ "$STATUS" -eq 65 ] || fail "(c) readiness timeout with a live daemon must exit 65 (exit $STATUS)"
grep -q "dockerd failed to become ready within 2s" "$OUT" || fail "(c) missing the timeout reason"
grep -q "phase=readiness-wait elapsed=" "$OUT" || fail "(c) missing the phase/elapsed stamp"
grep -q "docker info error: Cannot connect to the Docker daemon" "$OUT" || fail "(c) missing the captured docker info error"
grep -q -- "--- dockerd.log tail (last 100 lines, 8KiB cap) ---" "$OUT" || fail "(c) missing the log-tail header"
grep -q "padline-295" "$OUT" || fail "(c) tail is missing recent log lines"
grep -q "padline-150" "$OUT" && fail "(c) 100-line cap not honored (line 150 of 300 leaked)"
grep -q "padline-205" "$OUT" && fail "(c) 8KiB cap not honored (line 205 leaked past the byte cap)"
grep -q "dockerd pids:" "$OUT" || fail "(c) missing dockerd PID probe"
grep -q "containerd pids:" "$OUT" || fail "(c) missing containerd PID probe"
grep -q "docker.pid:" "$OUT" || fail "(c) missing pidfile probe"
grep -q "docker.sock:" "$OUT" || fail "(c) missing socket probe"
grep -q 'storage driver: "storage-driver": "vfs"; data-root fstype: tmpfs' "$OUT" || fail "(c) missing storage-driver/fstype probe"
grep -q "iptables errors in log tail: 1" "$OUT" || fail "(c) missing iptables error count from the bounded tail"
grep -q "CHAIN_ADD failed" "$OUT" || fail "(c) missing the iptables error line itself"
grep -q "data-root disk:" "$OUT" || fail "(c) missing data-root disk probe"
grep -q "mem: MemAvailable:" "$OUT" || fail "(c) missing MemAvailable probe"
grep -q "resolv.conf nameservers: nameserver 8.8.8.8 nameserver 1.1.1.1" "$OUT" || fail "(c) missing resolv.conf nameserver probe"
grep -q "registry-1.docker.io resolution: 203.0.113.7 registry-1.docker.io" "$OUT" || fail "(c) missing the registry resolution probe"
[ "$(wc -c < "$OUT")" -lt 14336 ] || fail "(c) diagnostics not bounded ($(wc -c < "$OUT") bytes)"
echo "PASS (c): timeout exits 65 with phase, docker-info error, bounded tail, probes, and DNS diagnostics"

# ---------------------------------------------------------------------------
# (d) Fast path: daemon already ready — exit 0 with no lock and no launch.
new_sandbox case-d
touch "$MARKER"
run_script
[ "$STATUS" -eq 0 ] || fail "(d) fast path must exit 0 (exit $STATUS)"
[ ! -e "$LOCK" ] || fail "(d) fast path must not take (or even create) the lock"
[ ! -e "$LAUNCHES" ] || fail "(d) fast path must not launch dockerd"
echo "PASS (d): ready daemon short-circuits with no lock taken"

# ---------------------------------------------------------------------------
# (e) Lock held beyond the bounded wait: loud failure, exit 64, no launch —
#     a lock-wait timeout is never a second launcher.
new_sandbox case-e
flock "$LOCK" sleep 15 &
HOLDER=$!
# Give the holder a moment to actually acquire the lock.
for _ in $(seq 1 50); do
  flock -n "$LOCK" true 2>/dev/null || break
  sleep 0.1
done
run_script G5_DEV_DOCKERD_LOCK_TIMEOUT=1
kill "$HOLDER" 2>/dev/null || true
set +e; wait "$HOLDER" 2>/dev/null; set -e
[ "$STATUS" -eq 64 ] || fail "(e) lock-wait timeout must exit 64 (exit $STATUS)"
grep -q "could not acquire" "$OUT" || fail "(e) missing the loud lock-timeout message"
grep -q "phase=lock-wait elapsed=" "$OUT" || fail "(e) missing the lock-wait phase stamp"
[ ! -e "$LAUNCHES" ] || fail "(e) lock-wait timeout must never launch a second dockerd"
echo "PASS (e): bounded lock wait fails loud with exit 64, never launches"

# ---------------------------------------------------------------------------
# (f) The launched dockerd EXITS before readiness: fail fast with exit 66 and
#     the diagnostics — never burn the full readiness timeout.
new_sandbox case-f
FAIL_FAST_START="$(date +%s)"
run_script G5_DEV_DOCKERD_READY_TIMEOUT=30
FAIL_FAST_ELAPSED=$(( $(date +%s) - FAIL_FAST_START ))
[ "$STATUS" -eq 66 ] || fail "(f) daemon death before readiness must exit 66 (exit $STATUS)"
grep -q "dockerd exited before readiness" "$OUT" || fail "(f) missing the daemon-exited reason"
grep -q "phase=readiness-wait elapsed=" "$OUT" || fail "(f) missing the phase/elapsed stamp"
[ "$FAIL_FAST_ELAPSED" -lt 15 ] || fail "(f) did not fail fast (took ${FAIL_FAST_ELAPSED}s of a 30s timeout)"
LAUNCH_COUNT="$(wc -l < "$LAUNCHES")"
[ "$LAUNCH_COUNT" -eq 1 ] || fail "(f) expected exactly 1 launch, got $LAUNCH_COUNT"
echo "PASS (f): daemon death before readiness fails fast with exit 66"

# ---------------------------------------------------------------------------
# (g) DNS/registry failure is distinguishable: with resolution failing, the
#     timeout diagnostics carry the FAILED resolution probe (daemon-local vs
#     DNS/registry-egress separation).
new_sandbox case-g
run_script G5_DEV_DOCKERD_READY_TIMEOUT=2 DOCKERD_STAY_ALIVE=10 GETENT_FAIL=1
[ "$STATUS" -eq 65 ] || fail "(g) readiness timeout must still exit 65 (exit $STATUS)"
grep -qE "registry-1.docker.io resolution: (getent: could not resolve|FAILED)" "$OUT" || fail "(g) missing the failed-resolution probe"
echo "PASS (g): DNS/registry failure is named in the diagnostics"

# ---------------------------------------------------------------------------
# (h) Repeated invocation after Docker is ready: exits 0 fast, no second
#     launch, no restart of the healthy daemon (fresh lock path untouched).
new_sandbox case-h
run_script DOCKERD_READY_AFTER=0
[ "$STATUS" -eq 0 ] || fail "(h) cold start must exit 0 (exit $STATUS)"
LAUNCH_COUNT="$(wc -l < "$LAUNCHES")"
[ "$LAUNCH_COUNT" -eq 1 ] || fail "(h) expected exactly 1 launch from the cold start, got $LAUNCH_COUNT"
run_script
[ "$STATUS" -eq 0 ] || fail "(h) repeated invocation must exit 0 (exit $STATUS)"
LAUNCH_COUNT="$(wc -l < "$LAUNCHES")"
[ "$LAUNCH_COUNT" -eq 1 ] || fail "(h) repeated invocation restarted a healthy daemon (launches=$LAUNCH_COUNT)"
echo "PASS (h): repeated invocation reuses the healthy daemon, never restarts it"

# ---------------------------------------------------------------------------
# (i) Cache door profile (ontology g5.runtime.cache.door): a cold start on a
#     host whose bring-up published a profile rewrites daemon.json so dockerd
#     pulls through the door over plain HTTP and mirrors Docker Hub through
#     mirror.gcr.io; without a profile the daemon's registry settings are
#     untouched. The storage-driver flip still happens in both cases.
if command -v jq >/dev/null 2>&1; then
  new_sandbox case-i
  printf 'G5_CACHE_DOOR=http://g5-host:7111\nG5_BASE_IMAGE_REGISTRY=g5-host:7111/us-central1-docker.pkg.dev/p/g5-ci\nG5_BASE_IMAGE_MIRROR_TIERS=base base-rust base-rust-warm base-node\n' > "$SB/cache-profile.env"
  run_script DOCKERD_READY_AFTER=1 G5_DEV_CACHE_PROFILE="$SB/cache-profile.env"
  [ "$STATUS" -eq 0 ] || fail "(i) cold start with a profile must exit 0 (exit $STATUS)"
  [ "$(jq -r '."insecure-registries"[0]' "$DAEMON_JSON")" = "g5-host:7111" ] || fail "(i) daemon.json lacks the door as an insecure registry: $(cat "$DAEMON_JSON")"
  [ "$(jq -r '."registry-mirrors"[0]' "$DAEMON_JSON")" = "https://mirror.gcr.io" ] || fail "(i) daemon.json lacks the Docker Hub mirror: $(cat "$DAEMON_JSON")"
  [ "$(jq -r '."storage-driver"' "$DAEMON_JSON")" = "vfs" ] || fail "(i) storage driver flip lost: $(cat "$DAEMON_JSON")"
  grep -q "base-rust: command not found" "$OUT" && fail "(i) the profile's tier list ran as a command: the profile must be read as data"
  new_sandbox case-i2
  run_script DOCKERD_READY_AFTER=1 G5_DEV_CACHE_PROFILE="$SB/absent-profile.env"
  [ "$STATUS" -eq 0 ] || fail "(i) cold start without a profile must exit 0 (exit $STATUS)"
  grep -q "insecure-registries" "$DAEMON_JSON" && fail "(i) daemon.json gained registry settings without a profile: $(cat "$DAEMON_JSON")"
  # The runtime disk ships an empty placeholder on hosts without a door
  # (ontology g5.runtime.cache.profile): it must read exactly as no profile.
  new_sandbox case-i3
  : > "$SB/empty-profile.env"
  run_script DOCKERD_READY_AFTER=1 G5_DEV_CACHE_PROFILE="$SB/empty-profile.env"
  [ "$STATUS" -eq 0 ] || fail "(i) cold start with the empty placeholder must exit 0 (exit $STATUS)"
  grep -q "insecure-registries" "$DAEMON_JSON" && fail "(i) daemon.json gained registry settings from the empty placeholder: $(cat "$DAEMON_JSON")"
  echo "PASS (i): a cache profile points dockerd at the door; no profile or the empty placeholder leaves daemon.json alone"
else
  echo "SKIP (i): jq unavailable"
fi

echo "g5-dev-dockerd-start contract tests passed"
