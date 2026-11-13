#!/usr/bin/env bash
# =============================================================================
# 70-systemd.sh — Install + enable the g5d-host systemd unit + wrapper.
#
# The unit launches /usr/local/libexec/g5d-host at boot. That wrapper
# probes `g5d host --help` to decide whether the supervisor module exists
# in the bundled g5d binary:
#
#   - When PR-rev7-3 has shipped, the subcommand exists and the wrapper
#     execs into it (becoming the real supervisor).
#   - Until then, the wrapper prints a short "not yet implemented" line
#     and exits 0 — the unit reports Active=inactive(exited) cleanly with
#     no Restart= loop spamming the journal.
#
# This is intentionally a self-healing transition: shipping PR-rev7-3 is
# strictly a g5d-binary update; no image rebuild + ExecStart-flip is needed.
# =============================================================================
set -euo pipefail

# VMM and network run without root; the host supervisor owns their lifetime.
getent group g5-vmm >/dev/null || groupadd --system g5-vmm
id g5-vmm >/dev/null 2>&1 || useradd --system --gid g5-vmm --no-create-home --shell /usr/sbin/nologin g5-vmm
usermod --append --groups kvm g5-vmm

install -d -m 0755 /etc/g5
install -d -m 0755 /usr/local/libexec

install -m 0644 /tmp/g5d-host.service /etc/systemd/system/g5d-host.service
install -m 0755 /tmp/g5d-host         /usr/local/libexec/g5d-host
install -m 0644 /tmp/g5d-host.env     /etc/g5/host.env

systemctl daemon-reload
systemctl enable g5d-host.service

# Verify the unit is well-formed before sealing the image.
systemd-analyze verify /etc/systemd/system/g5d-host.service || {
    echo "[g5-worker] FATAL: systemd unit failed verification" >&2
    exit 1
}

rm -f /tmp/g5d-host.service /tmp/g5d-host /tmp/g5d-host.env

systemctl is-enabled g5d-host.service

echo "[g5-worker] 70-systemd.sh OK"
