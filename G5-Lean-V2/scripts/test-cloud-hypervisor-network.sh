#!/usr/bin/env bash
# Real guest packets in disposable network/mount namespaces. The peer supplies
# synthetic DNS, API and metadata services; no cloud endpoint or credential is used.
set -euo pipefail
: "${G5_TEST_VM_ROOT:?owned VM test directory required}"
: "${G5_TEST_TEMPLATE:?prepared OCI template required}"
: "${G5_TEST_RUNTIME:?complete runtime required}"
: "${G5D_BIN:?candidate g5d required}"
script_dir=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" != --isolated ]; then
    [ "$(id -u)" = 0 ] || { echo 'requires an isolated privileged Linux/KVM test runner' >&2; exit 1; }
    exec unshare --net --mount -- "$0" --isolated
fi
# Changes to mounts, routing and nftables stay in this test namespace.
mount --make-rprivate /
mkdir -p /run/netns
mount -t tmpfs tmpfs /run/netns
ip link set lo up
ip netns add g5-ch-peer
trap 'ip netns del g5-ch-peer' EXIT
ip link add ch-net0 type veth peer name ch-net1
ip link set ch-net1 netns g5-ch-peer
ip addr add 198.18.0.1/24 dev ch-net0
ip link set ch-net0 up
ip netns exec g5-ch-peer ip link set lo up
ip netns exec g5-ch-peer ip addr add 198.18.0.2/24 dev ch-net1
ip netns exec g5-ch-peer ip link set ch-net1 up
ip netns exec g5-ch-peer ip addr add 169.254.169.254/32 dev lo
ip route add default via 198.18.0.2
resolver=$(mktemp)
trap 'rm -f "$resolver"; ip netns del g5-ch-peer' EXIT
printf 'nameserver 169.254.169.254\n' > "$resolver"
chmod 0644 "$resolver" # passt deliberately runs as an unprivileged UID
mount --bind "$resolver" /etc/resolv.conf
python3 "$script_dir/test_cloud_hypervisor_network.py"
