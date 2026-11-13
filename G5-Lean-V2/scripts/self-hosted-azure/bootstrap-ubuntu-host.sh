#!/usr/bin/env bash
# Root bootstrap for the Azure self-hosted canary's control and worker VMs.
# .github/workflows/self-hosted-azure-install.yml copies this single file to
# each VM and runs it with sudo before any secret lands, so every third-party
# input here is either a signed apt repository or a version-pinned release
# checked against a checksum recorded in this file before it runs. just is the
# same 1.55.1 release and sha256 as .github/actions/setup-just, replacing the
# mutable upstream installer script that was piped into a root shell, floated
# to the latest release and executed whatever the endpoint returned
# (G5Labs/G5-Lean#527; operations g5.quality.delivery.pinned-third-party-tools).
#
# scripts/test-pinned-download-verification.sh sources this file and exercises
# install_pinned_just against substituted bytes; the main flow runs only when
# the file is executed, never when it is sourced.
set -euo pipefail

JUST_VERSION=1.55.1
JUST_SHA256_X86_64=b0ef600f0df20d5ae91ae931627c499fc52b477ffe5f5ea7b7b3ec616b16c778
JUST_SHA256_AARCH64=b0ee814c9656427408e339893541e30d9027828686839499b2a2a34dd61ad173

# install_pinned_just DEST_DIR: download the pinned just release for this
# machine, verify its sha256, then extract and install the binary into
# DEST_DIR. Nothing is extracted or executed before the checksum matches, and
# an architecture without a recorded pin fails before any download.
install_pinned_just() {
  local dest_dir="$1" machine target expected tarball url work actual_version
  machine="$(uname -m)"
  case "$machine" in
    x86_64)  target=x86_64-unknown-linux-musl;  expected="$JUST_SHA256_X86_64" ;;
    aarch64) target=aarch64-unknown-linux-musl; expected="$JUST_SHA256_AARCH64" ;;
    *)
      echo "FATAL: no pinned just ${JUST_VERSION} release for architecture '${machine}'" >&2
      return 1
      ;;
  esac
  tarball="just-${JUST_VERSION}-${target}.tar.gz"
  url="https://github.com/casey/just/releases/download/${JUST_VERSION}/${tarball}"
  work="$(mktemp -d)"
  curl --proto '=https' --tlsv1.2 -fsSL --retry 4 --retry-delay 5 \
    --connect-timeout 20 --max-time 300 -o "${work}/${tarball}" "$url"
  if ! printf '%s  %s\n' "$expected" "${work}/${tarball}" | sha256sum --check --status; then
    echo "FATAL: ${tarball} sha256 mismatch; expected ${expected}, got $(sha256sum "${work}/${tarball}" | cut -d' ' -f1)" >&2
    rm -rf "$work"
    return 1
  fi
  tar -xzf "${work}/${tarball}" -C "$work" just
  install -m 0755 "${work}/just" "${dest_dir}/just"
  rm -rf "$work"
  actual_version="$("${dest_dir}/just" --version)"
  if [[ "$actual_version" != "just ${JUST_VERSION}" ]]; then
    echo "FATAL: installed just version mismatch; expected 'just ${JUST_VERSION}', got '${actual_version}'" >&2
    rm -f "${dest_dir}/just"
    return 1
  fi
  echo "Installed ${actual_version} from ${tarball} (sha256 ${expected})"
}

main() {
  export DEBIAN_FRONTEND=noninteractive

  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gnupg \
    jq \
    openssl \
    python3 \
    unzip

  install -m 0755 -d /etc/apt/keyrings
  if [ ! -f /etc/apt/keyrings/docker.asc ]; then
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
    chmod a+r /etc/apt/keyrings/docker.asc
  fi

  . /etc/os-release
  cat >/etc/apt/sources.list.d/docker.list <<EOF
deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${VERSION_CODENAME} stable
EOF

  apt-get update
  apt-get install -y --no-install-recommends \
    containerd.io \
    docker-buildx-plugin \
    docker-ce \
    docker-ce-cli \
    docker-compose-plugin

  systemctl enable --now docker

  if ! command -v just >/dev/null 2>&1; then
    install_pinned_just /usr/local/bin
  fi

  mkdir -p /opt/g5 /var/lib/g5
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
