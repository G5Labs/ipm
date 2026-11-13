#!/usr/bin/env bash
# =============================================================================
# 58-install-passt.sh — Install the pinned passt binary for microVM networking.
#
# Every microVM's virtio-net eth0 is backed by one unprivileged passt process
# spawned by `g5d vm-exec` (ontology g5.architecture.microvm_backend). The
# worker base is Ubuntu 24.04, whose archive passt (Feb 2024 snapshot)
# predates `--map-host-loopback` (upstream 2024_08_21) — the flag the D50
# guest→host door depends on — so we pin the Ubuntu 26.04 LTS (resolute)
# package instead: its only dependency is libc6 >= 2.38 and noble ships 2.39,
# so the .deb installs cleanly (human decision 2026-07-27, chat 6387dd1b).
#
# Immutable pin: version,
# URL, and sha256 live inline here; a bump is a one-line edit + image roll.
# =============================================================================
set -euo pipefail

# Shared bounded-retry download helper, uploaded by the packer `file`
# provisioner ahead of Phase 1 (G5Labs/G5#1811).
G5_DOWNLOAD_LIB="${G5_DOWNLOAD_LIB:-/tmp/g5-download-with-retry.sh}"
# Fail closed with a diagnosable message. Consumers must each provide this file:
# the packer build uploads it with a `file` provisioner ahead of Phase 1, and any
# container consumer must mount it and point G5_DOWNLOAD_LIB at the mount.
[ -r "$G5_DOWNLOAD_LIB" ] || {
    echo "[g5-worker] FATAL: shared download helper not readable at ${G5_DOWNLOAD_LIB}." >&2
    echo "[g5-worker] Provide it (packer: file provisioner; container: bind-mount + G5_DOWNLOAD_LIB)." >&2
    exit 1
}
# shellcheck source=lib/download-with-retry.sh
. "$G5_DOWNLOAD_LIB"


PASST_DEB_VERSION="${PASST_DEB_VERSION:-0.0~git20260120.386b5f5-1}"
PASST_DEB_SHA256="${PASST_DEB_SHA256:-49eaf773c120432bdf439d26d3de4d9867116580a819e6eba815b600c45b9840}"

case "$(uname -m)" in
    x86_64|amd64) PASST_DEB_ARCH=amd64 ;;
    *)
        echo "[g5-worker] FATAL: no passt pin for architecture $(uname -m)" >&2
        exit 1
        ;;
esac

PASST_DEB_URL="https://archive.ubuntu.com/ubuntu/pool/universe/p/passt/passt_${PASST_DEB_VERSION}_${PASST_DEB_ARCH}.deb"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "[g5-worker] fetching passt ${PASST_DEB_VERSION} (${PASST_DEB_ARCH})"
download_with_retry "passt deb" "$PASST_DEB_URL" "$tmp/passt.deb"

actual="$(sha256sum "$tmp/passt.deb" | awk '{print $1}')"
if [ "$actual" != "$PASST_DEB_SHA256" ]; then
    echo "[g5-worker] FATAL: passt .deb sha256 mismatch. expected=$PASST_DEB_SHA256 actual=$actual" >&2
    exit 1
fi
echo "[g5-worker] passt .deb sha256 OK: $actual"

dpkg -i "$tmp/passt.deb"

# Preserve the distribution profile and passt's namespace/seccomp sandbox.
# G5 places communication files below its owned VM directory instead of /tmp.
# The owner qualifier is defense in depth, not per-VM isolation: the runtime
# sandbox must hide the host filesystem before processing guest traffic.
python3 - <<'PY'
from pathlib import Path

profile = Path('/etc/apparmor.d/usr.bin.passt')
text = profile.read_text()
rule = '  owner /var/lib/g5d/rootfs/*/net/{net.sock,net.sock.repair,pid} w,\n'
if rule not in text:
    if not text.rstrip().endswith('}'):
        raise SystemExit('Unexpected pinned passt AppArmor profile')
    closing = text.rfind('}')
    profile.write_text(text[:closing] + rule + text[closing:])
PY
# A Docker image build can compile the policy but cannot load a policy into
# its future host kernel. Packer runs on that host and loads it immediately.
if [ "${G5_PASST_PROFILE_BUILD_ONLY:-0}" = 1 ]; then
    apparmor_parser -Q -T /etc/apparmor.d/usr.bin.passt
else
    apparmor_parser -r /etc/apparmor.d/usr.bin.passt
fi

# SEC-002: passt sockets must have an attributable identity before guest boot.
# The runtime atomically installs this account's output policy before spawning.
DEBIAN_FRONTEND=noninteractive apt-get install -y nftables
if ! getent passwd g5-passt >/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin g5-passt
fi
[ "$(id -u g5-passt)" != 0 ] && [ "$(id -g g5-passt)" != 0 ] || {
    echo "[g5-worker] FATAL: g5-passt must be an unprivileged account" >&2
    exit 1
}
test -x /usr/sbin/nft

# The runtime contract g5d relies on: `passt` resolvable on PATH and new
# enough to know every flag the D48-D51 spawn plan uses. Probe the exact
# flags now so a bad pin fails the image build, never the fleet.
command -v passt >/dev/null || { echo "[g5-worker] FATAL: passt not on PATH after install" >&2; exit 1; }
passt_help="$(passt --help 2>&1 || true)"
for flag in --map-host-loopback --no-map-gw --dns-forward --vhost-user --one-off --socket --pid -u; do
    if ! grep -q -- "$flag" <<<"$passt_help"; then
        echo "[g5-worker] FATAL: installed passt does not document $flag (pin too old?)" >&2
        exit 1
    fi
done
echo "[g5-worker] passt installed: $(passt --version 2>&1 | head -1)"
