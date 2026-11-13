#!/usr/bin/env bash
# Install Docker Engine + Compose into a G5 guest image (docker_images [I32]).
# Shared by Dockerfile.g5d-g5dev and Dockerfile.g5d-pairsoft-preview so the
# keyring, suite and package set never drift between the images that run
# dockerd in the microVM. Build-time only; runs as root inside `docker build`.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu resolute stable" \
    > /etc/apt/sources.list.d/docker.list
apt-get update
apt-get install -y --no-install-recommends \
    containerd.io \
    docker-buildx-plugin \
    docker-ce \
    docker-ce-cli \
    docker-compose-plugin \
    iptables
rm -rf /var/lib/apt/lists/*
