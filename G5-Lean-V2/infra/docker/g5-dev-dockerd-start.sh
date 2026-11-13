#!/usr/bin/env bash
# Start the in-guest Docker daemon for `just dev-up`.
#
# Single-flight contract: software_architecture/infra/docker_images I32
# ("dockerd up by default — and single-flight", issue G5Labs/G5#2400).
# Two cold invocations legitimately overlap shortly after boot (the
# microvm_runtime D57 `g5-guest-boot` hook and the preview `full-stack` profile's
# first command) — a POSSIBLE race and observability weakness, not a
# confirmed universal failure cause (a double-invocation preview,
# task 0876c5b7, succeeded with Docker ready in ~5.3s). All cold-start work
# runs under an exclusive flock, exactly one caller ever launches dockerd,
# everyone else shares that daemon's readiness result, and every failure is
# phase-stamped with a distinct exit code and bounded diagnostics. Shell
# oracle: infra/docker/test-g5-dev-dockerd-start.sh
# (test_contract/base_image_consolidation D15).
#
# Exit codes: 0 ready; 64 lock-wait timeout; 65 readiness timeout with the
# daemon still alive; 66 dockerd exited before readiness.
set -euo pipefail
# Fast path: an already-ready daemon needs no lock and no work — a repeated
# invocation after Docker is up NEVER restarts a healthy daemon (I32).
if docker info >/dev/null 2>&1; then
  exit 0
fi

if [[ "$(id -u)" -ne 0 ]]; then
  exec sudo /usr/local/bin/g5-dev-dockerd-start "$@"
fi

# Env seams with production defaults. Overridable SOLELY so the
# test_contract/base_image_consolidation D15 shell oracle can exercise this
# exact installed script without a VM or root (the D13 lock-oracle pattern).
G5_DEV_DOCKERD_LOG="${G5_DEV_DOCKERD_LOG:-/var/log/dockerd.log}"
G5_DEV_DOCKERD_LOCK="${G5_DEV_DOCKERD_LOCK:-/run/g5-dev-dockerd-start.lock}"
G5_DEV_DOCKERD_DAEMON_JSON="${G5_DEV_DOCKERD_DAEMON_JSON:-/etc/docker/daemon.json}"
G5_DEV_DOCKERD_DATA_ROOT="${G5_DEV_DOCKERD_DATA_ROOT:-/var/lib/docker}"
G5_DEV_DOCKERD_RESOLV_CONF="${G5_DEV_DOCKERD_RESOLV_CONF:-/etc/resolv.conf}"
G5_DEV_DOCKERD_READY_TIMEOUT="${G5_DEV_DOCKERD_READY_TIMEOUT:-60}"
G5_DEV_DOCKERD_LOCK_TIMEOUT="${G5_DEV_DOCKERD_LOCK_TIMEOUT:-90}"

STARTED_AT="$(date +%s 2>/dev/null || echo 0)"
PHASE="init"
LAST_DOCKER_INFO_ERROR=""
LAUNCHED_DOCKERD_PID=""

elapsed_s() {
  local now
  now="$(date +%s 2>/dev/null || echo 0)"
  echo $((now - STARTED_AT))
}

# Readiness probe that keeps the FINAL `docker info` error for diagnostics.
docker_ready() {
  local err
  if err="$(docker info 2>&1 >/dev/null)"; then
    return 0
  fi
  LAST_DOCKER_INFO_ERROR="$err"
  return 1
}

# On any failure, stderr is the only surface that survives the per-VM rootfs
# teardown (it reaches the host's redacted guest-console relay), so print the
# startup phase + elapsed time, the last `docker info` error, a BOUNDED
# dockerd.log tail, one-line probes that distinguish the daemon-local failure
# classes (PID/socket contention, containerd, storage-driver, iptables,
# resource exhaustion), and DNS/connectivity probes that separate daemon
# failure from DNS/registry-egress failure. Bounded and credential-free.
print_failure_diagnostics() {
  local reason="$1"
  {
    echo "g5-dev-dockerd-start: FAILED: ${reason} (phase=${PHASE} elapsed=$(elapsed_s)s)"
    echo "docker info error: $(printf '%s' "${LAST_DOCKER_INFO_ERROR:-(none captured)}" | head -c 2048)"
    echo "--- dockerd.log tail (last 100 lines, 8KiB cap) ---"
    local log_tail=""
    if [[ -f "$G5_DEV_DOCKERD_LOG" ]]; then
      log_tail="$(tail -n 100 "$G5_DEV_DOCKERD_LOG" | tail -c 8192 || true)"
      printf '%s\n' "$log_tail"
    else
      echo "(no dockerd log at ${G5_DEV_DOCKERD_LOG})"
    fi
    echo "--- context probes ---"
    echo "dockerd pids: $(pgrep -x dockerd 2>/dev/null | tr '\n' ' ')"
    echo "containerd pids: $(pgrep -x containerd 2>/dev/null | tr '\n' ' ')"
    if [[ -f /var/run/docker.pid ]]; then
      echo "docker.pid: present ($(tr -cd '0-9' < /var/run/docker.pid))"
    else
      echo "docker.pid: absent"
    fi
    if [[ -S /var/run/docker.sock ]]; then
      echo "docker.sock: present"
    else
      echo "docker.sock: absent"
    fi
    echo "storage driver: $(grep -o '"storage-driver": "[a-z0-9]*"' "$G5_DEV_DOCKERD_DAEMON_JSON" 2>/dev/null || echo unknown); data-root fstype: $(findmnt -no FSTYPE "$G5_DEV_DOCKERD_DATA_ROOT" 2>/dev/null || echo unknown)"
    local iptables_count
    iptables_count="$(printf '%s\n' "$log_tail" | grep -ci iptables || true)"
    echo "iptables errors in log tail: ${iptables_count}"
    if [[ "${iptables_count:-0}" -gt 0 ]]; then
      printf '%s\n' "$log_tail" | grep -i iptables | head -n 5 || true
    fi
    echo "data-root disk: $(df -Ph "$G5_DEV_DOCKERD_DATA_ROOT" 2>/dev/null | tail -1 || echo unknown)"
    echo "mem: $(grep MemAvailable /proc/meminfo 2>/dev/null || echo unknown)"
    echo "--- dns/connectivity probes ---"
    echo "resolv.conf nameservers: $(grep -E '^nameserver' "$G5_DEV_DOCKERD_RESOLV_CONF" 2>/dev/null | tr '\n' ' ')"
    echo "registry-1.docker.io resolution: $( (timeout 3 getent hosts registry-1.docker.io 2>&1 || echo FAILED) | head -1 )"
  } >&2
}

# Single-flight: every cold-start step below runs under an exclusive flock on
# a dedicated fd. A lock-wait timeout is a LOUD, distinct failure — never a
# second launcher (I32 contract).
PHASE="lock-wait"
exec 9>"$G5_DEV_DOCKERD_LOCK"
if ! flock -w "$G5_DEV_DOCKERD_LOCK_TIMEOUT" 9; then
  echo "g5-dev-dockerd-start: could not acquire ${G5_DEV_DOCKERD_LOCK} within ${G5_DEV_DOCKERD_LOCK_TIMEOUT}s; another invocation holds the single-flight lock and has not finished — refusing to launch a second dockerd" >&2
  print_failure_diagnostics "lock-wait timeout after ${G5_DEV_DOCKERD_LOCK_TIMEOUT}s"
  exit 64
fi

# Re-probe under the lock: the lock winner may have already brought the
# daemon up while we waited; share its readiness result.
if docker_ready; then
  exit 0
fi

PHASE="daemon-startup"
if ! pgrep -x dockerd >/dev/null 2>&1; then
  # No live daemon: this caller is the single launcher.
  # Scratch-disk boots (microvm_runtime D53) mount a real ext4 block
  # device at /var/lib/docker; overlay2 is safe and fast there, while
  # vfs stays for tmpfs/virtio-fs data-roots. Set the driver BOTH ways
  # on every start: a one-way flip would let a reused rootfs carry a
  # stale overlay2 pin onto a later non-ext4 boot.
  if [[ "$(findmnt -no FSTYPE "$G5_DEV_DOCKERD_DATA_ROOT" 2>/dev/null)" == "ext4" ]]; then
    sed -i 's/"storage-driver": "vfs"/"storage-driver": "overlay2"/' "$G5_DEV_DOCKERD_DAEMON_JSON"
  else
    sed -i 's/"storage-driver": "overlay2"/"storage-driver": "vfs"/' "$G5_DEV_DOCKERD_DAEMON_JSON"
  fi
  # Cache door (ontology g5.runtime.cache.door): when the host's bring-up
  # published a profile, let dockerd pull through the door over plain HTTP
  # (the alias maps to the host's loopback only) and mirror Docker Hub
  # through Google's public mirror. Without a profile (the runtime disk's empty
  # placeholder reads as none; ontology g5.runtime.cache.profile) the daemon's
  # registry settings are left exactly as the image shipped them. The profile is read
  # as data, never sourced: a value with spaces (the tier list) would run as
  # a command.
  cache_profile="${G5_DEV_CACHE_PROFILE:-/opt/g5/cache-profile.env}"
  door=""
  if [[ -r "$cache_profile" ]]; then
    door="$(grep -E '^G5_CACHE_DOOR=' "$cache_profile" 2>/dev/null | head -n 1 | cut -d= -f2- || true)"
  fi
  if [[ -n "$door" ]] && command -v jq >/dev/null 2>&1; then
    door_host="${door#http://}"
    door_host="${door_host#https://}"
    door_host="${door_host%%/*}"
    rewritten="$(mktemp)"
    if jq --arg door "$door_host" \
         '."insecure-registries" = [$door] | ."registry-mirrors" = ["https://mirror.gcr.io"]' \
         "$G5_DEV_DOCKERD_DAEMON_JSON" > "$rewritten"; then
      cat "$rewritten" > "$G5_DEV_DOCKERD_DAEMON_JSON"
    fi
    rm -f "$rewritten"
  fi
  mkdir -p "$(dirname "$G5_DEV_DOCKERD_LOG")"
  # 9>&- closes the lock fd for the daemon: dockerd must never inherit the
  # single-flight lock, or it would hold it for its whole life (I32).
  nohup dockerd >>"$G5_DEV_DOCKERD_LOG" 2>&1 9>&- &
  LAUNCHED_DOCKERD_PID=$!
fi
# else: a dockerd process already exists but is not ready yet — do NOT launch
# another; wait for that daemon's readiness below (I32 contract).

PHASE="readiness-wait"
for _ in $(seq 1 "$G5_DEV_DOCKERD_READY_TIMEOUT"); do
  if docker_ready; then
    exit 0
  fi
  # Fail fast when the daemon this run is waiting on has already died —
  # burning the remaining timeout would only delay the diagnostics. Re-check
  # readiness once after observing death so a became-ready-then-exited stub
  # or crash-after-ready daemon cannot race a false 66.
  daemon_gone=""
  if [[ -n "$LAUNCHED_DOCKERD_PID" ]]; then
    kill -0 "$LAUNCHED_DOCKERD_PID" 2>/dev/null || daemon_gone=1
  else
    pgrep -x dockerd >/dev/null 2>&1 || daemon_gone=1
  fi
  if [[ -n "$daemon_gone" ]]; then
    if docker_ready; then
      exit 0
    fi
    print_failure_diagnostics "dockerd exited before readiness"
    exit 66
  fi
  sleep 1
done

print_failure_diagnostics "dockerd failed to become ready within ${G5_DEV_DOCKERD_READY_TIMEOUT}s"
exit 65
