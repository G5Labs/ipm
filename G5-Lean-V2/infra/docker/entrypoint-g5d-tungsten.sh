#!/usr/bin/env bash
# =============================================================================
# entrypoint-g5d-tungsten.sh — DinD-aware launcher for the Tungsten host
# =============================================================================
# Starts dockerd as root in the background, waits for the socket, then drops
# to the g5 user (member of the `docker` group) and execs `g5d host`.
#
# The pod must run privileged for dockerd to manage cgroups + iptables.
# =============================================================================

set -euo pipefail

DOCKER_SOCK_PATH="/var/run/docker.sock"
. /usr/local/bin/g5d-provider-env

# /var/run is an emptyDir volume that PERSISTS across container restarts
# within the same pod (only the pod's lifetime resets it). When this
# container exits for any reason (host crash, OOM kill, kubelet
# restart) and the kubelet relaunches it, the previous run's docker.pid,
# managed-containerd sockets, and pidfiles are still on disk. dockerd's
# pidfile check then fails closed ("process with PID N is still running")
# even though PID N in the new container's PID namespace is a different
# process. dockerd-managed containerd also can't bind its already-
# existing socket and times out, surfacing as "failed to start
# containerd: timeout waiting for containerd to start".
#
# Clear the stale DinD state up front so each container restart starts
# from a known-clean slate. No-op on first boot when nothing is present.
rm -f /var/run/docker.pid
rm -rf /var/run/docker

# dockerd writes its log to stderr so kubectl logs interleaves daemon noise
# with host noise; that's acceptable for V1 — the host log is the
# higher-signal stream and dockerd is mostly silent after startup.
dockerd \
    --host="unix://${DOCKER_SOCK_PATH}" \
    --pidfile=/var/run/docker.pid \
    ${DOCKERD_EXTRA_ARGS:-} \
    >&2 &
DOCKERD_PID=$!

# Wait up to 60s for the daemon to accept commands. Testcontainers fails fast
# at test-startup if Docker isn't ready, so we'd rather fail the pod here than
# midway through a `mvn verify`.
for _ in $(seq 1 60); do
    if docker info >/dev/null 2>&1; then
        break
    fi
    if ! kill -0 "${DOCKERD_PID}" 2>/dev/null; then
        echo "entrypoint-g5d-tungsten: dockerd exited during startup" >&2
        exit 1
    fi
    sleep 1
done

if ! docker info >/dev/null 2>&1; then
    echo "entrypoint-g5d-tungsten: dockerd did not become ready within 60s" >&2
    exit 1
fi

# The docker group already grants socket access via `usermod -aG docker g5`
# at image-build time; this re-applies the group on the live socket in case
# /var/run was provided as an emptyDir volume (fresh inode each pod start).
chgrp docker "${DOCKER_SOCK_PATH}"
chmod 0660 "${DOCKER_SOCK_PATH}"

existing_provider_configs="${G5D_PROVIDER_CONFIGS:-}"
g5d_setup_provider_env
if [ -z "$existing_provider_configs" ] && [ -n "${G5D_PROVIDER_CONFIGS:-}" ]; then
    # `g5d_setup_provider_env` runs `mkdir -p` as root for the generated
    # provider-config directory, so the directory inherits root:root with
    # mode 0755 — readable but not writable by g5. g5d host (as g5) needs
    # write access on that directory to create sibling state like
    # `worker-storage/{repos,worktrees}` when `G5_STORAGE_*` are unset (it
    # falls back to `<workspace>/.g5/worker-storage/...` per
    # `WorkerOrchestrator::storage_config_for_worker`). Chown the dir
    # recursively, not just the toml file, otherwise storage init fails
    # with `base_dir cannot be created: Permission denied`.
    chown -R g5:g5 "$(dirname "${G5D_PROVIDER_CONFIGS}")"
fi

# Use `runuser` WITHOUT `-p` (preserve-environment): that flag keeps HOME=/root
# from the calling shell, which makes git stat /root/.gitconfig (not writable
# by g5) and fails workspace preparation. The default `runuser -u g5 --`
# behavior preserves the provider bootstrap environment. Override only HOME so
# unset optional provider variables do not become empty strings and disable g5d's
# own fallback chain.
exec runuser -u g5 -- env \
    HOME=/home/g5 \
    g5d host
