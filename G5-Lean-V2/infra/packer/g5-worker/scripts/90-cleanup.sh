#!/usr/bin/env bash
# =============================================================================
# 90-cleanup.sh — Strip build tooling, scrub identity, trim FS, finalize.
#
# Removes build-only packages (marked --mark-auto in 10-apt-deps.sh),
# deletes the build-time Rust toolchain under /opt/rust, clears apt + npm
# caches, truncates logs, runs fstrim so the image's sparse representation
# compresses well. The guest-visible Rust test toolchain intentionally remains
# under /var/g5d-data/runtime/bin/rust.
#
# Identity scrubbing — every instance booted from this image must be
# distinguishable from every other one. We remove:
#   - SSH host keys      (cloud-init regenerates per-host on first boot)
#   - /etc/machine-id    (systemd regenerates per-host on first boot)
#   - cloud-init seed    (otherwise the build VM's hostname/identity leaks)
#   - bash history       (the packer user's history would otherwise persist)
#
# Rev-7 §5.0 budget: target ~15 GB on disk for the final image; this
# script is the gatekeeper. Without it the image lands at ~25 GB (Rust
# toolchain + kernel source + cargo registry alone are >5 GB).
# =============================================================================
set -euo pipefail

# 1. Autoremove build-only packages (everything --mark-auto from 10-apt-deps.sh).
echo "[g5-worker] apt-get autoremove build-only packages"
apt-get autoremove -y --purge

# 2. Clean apt caches.
apt-get clean
rm -rf /var/lib/apt/lists/*

# 3. Remove the Rust toolchain (installed under /opt/rust by 10-apt-deps.sh).
rm -rf /opt/rust
rm -f /usr/local/bin/cargo /usr/local/bin/rustc /usr/local/bin/rustup


# 5. Clear npm cache + remove the build-only NPM_BUILD_PREFIX. The runnable
# npm tree is vendored at /var/g5d-data/runtime/bin/_npm/ by
# 40-install-claude.sh + 50-install-codex.sh — that's the copy the
# mount-injected wrappers actually use (rev-7 plan §5.4 line 182).
# Leaving NPM_BUILD_PREFIX in place would double-store the same tree on
# the image.
npm cache clean --force >/dev/null 2>&1 || true
rm -rf /root/.npm /home/packer/.npm
rm -rf /var/g5d-data/runtime/npm

# 6. Truncate cloud-image journals + logs.
journalctl --rotate || true
journalctl --vacuum-time=1s || true
truncate -s 0 /var/log/wtmp /var/log/btmp 2>/dev/null || true
find /var/log -type f -name '*.gz' -delete
find /var/log -type f -name '*.[0-9]' -delete
find /var/log -type f \( -name 'syslog*' -o -name 'auth.log*' -o -name 'kern.log*' -o -name 'dpkg.log*' \) -exec truncate -s 0 {} +

# 7. Identity scrub: every instance booted from this image must be
# distinguishable. Without these wipes, every host shares ssh host keys
# (security smell), the same /etc/machine-id (breaks identity-keyed
# services), and inherits cloud-init state from the build VM.
rm -f /etc/ssh/ssh_host_*
truncate -s 0 /etc/machine-id 2>/dev/null || true
rm -f /var/lib/dbus/machine-id 2>/dev/null || true
ln -sf /etc/machine-id /var/lib/dbus/machine-id
rm -rf /var/lib/cloud/instances/* /var/lib/cloud/seed/* /var/lib/cloud/data/* 2>/dev/null || true

# 8. Lock the ephemeral packer build user — its password was visible in the
# autoinstall seed; nobody should be able to log back in as packer at runtime.
# This is deliberately NOT done here: locking packer / removing its sudoers in
# this final provisioner would revoke the sudo that Packer's own
# `shutdown_command` (sudo as packer) needs moments later, hanging the build
# with no artifact. The scrub is folded into shutdown_command in
# g5-worker.pkr.hcl so it runs as the very last privileged act before poweroff,
# while NOPASSWD sudo is still valid — and is still persisted into the image.

# 9. Clean shell history / temp.
rm -rf /tmp/* /var/tmp/* /root/.bash_history /home/packer/.bash_history 2>/dev/null || true

# 10. Final fstrim so unused blocks compress well in the resulting image.
fstrim -av || true

# 11. Final size report.
echo "[g5-worker] final disk usage:"
df -h /
echo "[g5-worker] /var/g5d-data size:"
du -sh /var/g5d-data
echo "[g5-worker] /var/g5d-data/runtime breakdown:"
du -sh /var/g5d-data/runtime/* 2>/dev/null || true
echo "[g5-worker] top 20 packages by install size:"
# `| head -20` closes the pipe early, so dpkg-query/sort get SIGPIPE (exit 141)
# under `set -o pipefail` — which would abort this otherwise-complete cleanup on
# a purely informational report. `|| true` keeps the diagnostic best-effort.
dpkg-query -W -f='${Package}\t${Installed-Size}\n' | sort -k2 -n -r | head -20 || true

echo "[g5-worker] 90-cleanup.sh OK"
