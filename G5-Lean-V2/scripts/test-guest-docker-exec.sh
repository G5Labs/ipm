#!/usr/bin/env bash
# Run only inside a disposable guest booted by the candidate initramfs/g5d.
# Exercise the actual Docker/runc exec path and Compose health checks offline.
set -euo pipefail
test -f /.g5/bootstrapped
test -r /opt/g5/bin/g5d

# The minimal CI image has no boot hook. Full-environment acceptance instead
# uses the production Docker daemon already started by g5-dev-dockerd-start.
daemon_pid=
if [ "${G5_TEST_START_DOCKER:-0}" = 1 ]; then
    unset DOCKER_API_VERSION
    export DOCKER_HOST=unix:///run/g5-docker-test.sock
    dockerd --host="$DOCKER_HOST" --pidfile=/run/g5-docker-test.pid \
        --exec-root=/run/g5-docker-test --data-root=/var/lib/docker \
        --storage-driver=overlay2 --feature=containerd-snapshotter=false \
        --bridge=none --iptables=false --ip6tables=false \
        --ip-forward=false --ip-masq=false > /tmp/g5-docker-test.log 2>&1 &
    daemon_pid=$!
fi
root=$(mktemp -d /tmp/g5-docker-exec.XXXXXX)
project="g5-exec-$$"
image="$project:local"
cleanup() {
    result=$?
    trap - EXIT
    if [ "$result" -ne 0 ]; then
        if [ -f "$root/compose.yaml" ]; then
            docker compose -p "$project" -f "$root/compose.yaml" ps -a || true
        fi
        if [ -f /tmp/g5-docker-test.log ]; then tail -n 100 /tmp/g5-docker-test.log || true; fi
    fi
    if [ -f "$root/compose.yaml" ]; then
        docker compose -p "$project" -f "$root/compose.yaml" down --timeout 5 >/dev/null 2>&1 || true
    fi
    docker image rm "$image" >/dev/null 2>&1 || true
    if [ -n "$daemon_pid" ]; then kill "$daemon_pid" 2>/dev/null || true; wait "$daemon_pid" || true; fi
    rm -rf "$root"
    exit "$result"
}
trap cleanup EXIT
for ((attempt=0; attempt<60; attempt++)); do
    if docker info >/dev/null 2>&1; then break; fi
    if [ -n "$daemon_pid" ]; then kill -0 "$daemon_pid"; fi
    sleep 1
done
docker info >/dev/null
test "$(docker info --format '{{.Driver}}')" = overlay2
mkdir -p "$root/image/bin"
cp /opt/g5/bootstrap/busybox "$root/image/bin/busybox"
ln -s busybox "$root/image/bin/sh"
printf 'container-only\n' > "$root/image/.g5-exec-marker"
tar -C "$root/image" -cf - . | docker import - "$image" >/dev/null
# Hosts with nested virtualization on expose /dev/kvm to the guest; a container
# given the device must be able to use it as an unprivileged user, because the
# trusted init made the node 0666 (g5.architecture.microvm_backend.nested).
if [ "${G5_TEST_EXPECT_KVM:-0}" = 1 ]; then
    test "$(docker run --rm --user 65534:65534 --device /dev/kvm "$image" /bin/busybox sh -ec 'test -c /dev/kvm; exec 3<>/dev/kvm; echo DOCKER_KVM_OK')" = DOCKER_KVM_OK
fi
cat > "$root/compose.yaml" <<YAML
services:
  probe:
    image: $image
    network_mode: none
    command: ["/bin/busybox", "sleep", "120"]
    healthcheck:
      test: ["CMD", "/bin/sh", "-ec", "test -f /.g5-exec-marker; test ! -e /opt/g5/bin/g5d"]
      interval: 1s
      timeout: 3s
      retries: 20
YAML
docker compose -p "$project" -f "$root/compose.yaml" up -d --pull never --wait --wait-timeout 45
container=$(docker compose -p "$project" -f "$root/compose.yaml" ps -q probe)
test -n "$container"
test "$(docker inspect --format '{{.State.Health.Status}}' "$container")" = healthy
test "$(docker exec "$container" /bin/sh -ec 'test ! -e /opt/g5/bin/g5d; /bin/busybox cat /.g5-exec-marker')" = container-only
test "$(docker compose -p "$project" -f "$root/compose.yaml" exec -T probe /bin/sh -ec '/bin/busybox cat /.g5-exec-marker')" = container-only
echo G5_DOCKER_EXEC_COMPOSE_HEALTH_OK
