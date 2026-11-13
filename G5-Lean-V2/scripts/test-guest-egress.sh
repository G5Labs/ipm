#!/usr/bin/env bash
# Real passt packets and nftables in a disposable Docker network namespace.
# No host network, cloud credentials or real metadata endpoints are used.
set -euo pipefail
cd "$(dirname "$0")/.."
docker build -t g5-sec002-network-test -f - . <<'DOCKERFILE'
FROM ubuntu:26.04
RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables iproute2 passt python3 curl tcpdump cargo rustc pkg-config libssl-dev build-essential libclang-dev libcap-dev >/dev/null && useradd --system --uid 499 --user-group --no-create-home --shell /usr/sbin/nologin g5-passt
DOCKERFILE
# Build with network access, then run the compiled test with no external route.
docker run --rm -v "$PWD:/src:ro" -v g5-sec002-target:/target -v g5-sec002-cargo:/cargo \
    -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target -w /src g5-sec002-network-test \
    bash -c 'cargo test -p g5d-isolation --lib --no-default-features --no-run --message-format=json > /target/test-build.json && python3 -c '\''import json; from pathlib import Path; print(next(v["executable"] for s in Path("/target/test-build.json").read_text().splitlines() if (v:=json.loads(s)).get("executable") and v["profile"].get("test")))'\'' > /target/egress-test-executable'
docker run --rm --privileged --network none \
    -v "$PWD:/src:ro" -v g5-sec002-target:/target -w /src \
    -e G5_EGRESS_ISOLATED_TEST=1 -e G5_EGRESS_TEST_SCRIPT=/src/scripts/test_guest_egress_packets.py \
    g5-sec002-network-test bash -eu -c '
        sysctl -qw net.ipv6.conf.all.disable_ipv6=0 net.ipv6.conf.default.disable_ipv6=0
        ip link set lo up
        ip addr add 169.254.169.254/32 dev lo
        ip addr add 169.254.2.253/32 dev lo
        ip netns add sec002-peer
        ip link add egress0 type veth peer name egress1
        ip link set egress1 netns sec002-peer
        ip addr add 198.18.0.1/24 dev egress0
        ip link set egress0 up
        ip netns exec sec002-peer ip addr add 198.18.0.2/24 dev egress1
        ip netns exec sec002-peer ip link set egress1 up
        ip netns exec sec002-peer ip link set lo up
        ip netns exec sec002-peer sysctl -qw net.ipv6.conf.all.disable_ipv6=0 net.ipv6.conf.default.disable_ipv6=0
        ip -6 addr add fe80::1/64 dev egress0 nodad
        ip netns exec sec002-peer ip -6 addr add fe80::2/64 dev egress1 nodad
        ip netns exec sec002-peer python3 /src/scripts/test_guest_egress_packets.py --probe-host-neighbor
        ip route add default via 198.18.0.2
        printf "nameserver 169.254.169.254\n" > /etc/resolv.conf
        "$(cat /target/egress-test-executable)" passt_vhost_user_policy_gate --ignored --nocapture
    '
