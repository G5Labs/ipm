#!/usr/bin/env bash
# =============================================================================
# 80-configure-host.sh — Apply host-side sysctl + module configuration and
# hard-verify XFS root with reflink.
#
# Sysctl tunings (rev-7 §3 — host must be ready for many microVMs):
#   - fs.inotify.max_user_watches  — workspace materialization watches lots of files
#   - fs.inotify.max_user_instances — enough instances for concurrent workspace VMs
#   - vm.max_map_count             — concurrent VMM address mappings
#
# KVM modules: the autoinstall seed (http/user-data) already wrote
# /etc/modules-load.d/g5-worker-kvm.conf with BOTH kvm_intel and kvm_amd.
# We don't redo it here; modprobe silently skips the variant that doesn't
# match the deployment-time host's CPU. This file double-checks the config
# is present so a regression in autoinstall surfaces at build time.
#
# XFS hard verification: this is the rev-7 §5.0 keystone. The autoinstall
# seed declares `fstype: xfs` with `extra_options: ["-m", "reflink=1"]`, so
# the root MUST be XFS with reflink at this point. If it isn't, the image
# is non-compliant and we fail the build (per Rule 5: no graceful
# degradation). PR-rev7-3's `g5d doctor` will look for this same status.
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


# --- 1. Sysctl tunings ------------------------------------------------------

install -d -m 0755 /etc/sysctl.d
cat > /etc/sysctl.d/99-g5-worker.conf <<'EOF'
# Provisioned by g5-worker image (rev 7). See infra/packer/g5-worker/
# scripts/80-configure-host.sh for rationale.
fs.inotify.max_user_watches=1048576
fs.inotify.max_user_instances=1024
vm.max_map_count=262144
EOF
chmod 0644 /etc/sysctl.d/99-g5-worker.conf

# --- 2. KVM modules config from autoinstall seed verified persisted ---------

KVM_CONF=/etc/modules-load.d/g5-worker-kvm.conf
if [ ! -f "$KVM_CONF" ]; then
    # The autoinstall seed should have created this; if it's missing the seed
    # was edited badly. Re-create defensively + log loudly.
    echo "[g5-worker] WARN: $KVM_CONF missing; re-creating from in-image fallback" >&2
    cat > "$KVM_CONF" <<'EOF'
kvm
kvm_intel
kvm_amd
EOF
fi

# Confirm both module names are configured (defensive against partial edits).
for mod in kvm kvm_intel kvm_amd; do
    grep -qxF "$mod" "$KVM_CONF" || {
        echo "[g5-worker] FATAL: $mod not configured in $KVM_CONF; image cannot host microVMs cross-CPU-vendor" >&2
        cat "$KVM_CONF" >&2
        exit 1
    }
done

# --- 3. XFS root + reflink hard verification --------------------------------

ROOT_FSTYPE="$(findmnt -no FSTYPE /)"
if [ "$ROOT_FSTYPE" != "xfs" ]; then
    echo "[g5-worker] FATAL: root filesystem is $ROOT_FSTYPE, expected xfs (rev-7 §5.0)" >&2
    findmnt /
    exit 1
fi
echo "[g5-worker] root filesystem is XFS"

xfs_info / | tee /tmp/xfs_info.txt
grep -q "reflink=1" /tmp/xfs_info.txt || {
    echo "[g5-worker] FATAL: XFS reflink not enabled on root; check autoinstall mkfs.xfs flags" >&2
    exit 1
}
echo "[g5-worker] XFS reflink=1 confirmed"

# End-to-end reflink check: surface a regression where the FS reports
# reflink-capable but the kernel/lib path is actually broken. cp must
# succeed AND produce identical bytes.
echo "reflink-test-data-$(date +%s)" > /tmp/reflink_src.txt
cp --reflink=always /tmp/reflink_src.txt /tmp/reflink_dst.txt
cmp /tmp/reflink_src.txt /tmp/reflink_dst.txt
rm -f /tmp/reflink_src.txt /tmp/reflink_dst.txt /tmp/xfs_info.txt
echo "[g5-worker] cp --reflink=always round-trip verified"

# --- 4. Optional summary marker for PR-rev7-3 doctor -----------------------

install -d -m 0755 /etc/g5
echo "xfs-reflink-verified" > /etc/g5/xfs-root-status

# --- 5. First-boot bootstrap: re-enable cloud-init -------------------------
#
# Subiquity finishes the autoinstall by locking the installed system's
# cloud-init to `datasource_list: [ None ]` (via /etc/cloud/cloud.cfg.d/
# 99-installer.cfg, and sometimes /etc/cloud/cloud-init.disabled). ds-identify
# then disables cloud-init on every subsequent boot — which breaks the rev-7
# design assumption (90-cleanup.sh §7) that cloud-init regenerates the wiped
# ssh host keys + applies a datasource on first boot, and breaks the
# image-test gate, which injects a NoCloud seed ISO to create its ssh user.
#
# Re-enable cloud-init with a datasource_list covering BOTH the gate (NoCloud
# seed) and GCE (instance metadata), falling back to None. The 99-zzz- prefix
# sorts AFTER subiquity's 99-installer.cfg so our datasource_list wins the
# cloud-init config merge. 90-cleanup.sh then wipes /var/lib/cloud state so the
# next boot is treated as a fresh instance and cloud-init runs its modules
# (including cc_ssh, which regenerates the absent host keys).
rm -f /etc/cloud/cloud-init.disabled
install -d -m 0755 /etc/cloud/cloud.cfg.d
cat > /etc/cloud/cloud.cfg.d/99-zzz-g5-worker-datasource.cfg <<'EOF'
# Provisioned by g5-worker image (rev 7) — overrides subiquity's installer
# datasource_list: [None] lock so cloud-init runs on first boot. See
# scripts/80-configure-host.sh for rationale.
datasource_list: [ NoCloud, GCE, None ]
EOF
chmod 0644 /etc/cloud/cloud.cfg.d/99-zzz-g5-worker-datasource.cfg

# --- 6. Belt + suspenders: regenerate ssh host keys before sshd ------------
#
# 90-cleanup.sh wipes /etc/ssh/ssh_host_* so each instance gets unique keys.
# cloud-init's cc_ssh regenerates them, but if cloud-init is slow/absent on a
# given boot, sshd would fail to start with no host keys (the exact failure the
# image-test gate hit). A drop-in ExecStartPre runs `ssh-keygen -A` (creates
# only MISSING keys — a no-op after first boot) so sshd always has host keys,
# independent of cloud-init timing. Applies on GCE and in the test gate alike.
install -d -m 0755 /etc/systemd/system/ssh.service.d
cat > /etc/systemd/system/ssh.service.d/10-g5-hostkeys.conf <<'EOF'
[Service]
ExecStartPre=-/usr/bin/ssh-keygen -A
EOF
chmod 0644 /etc/systemd/system/ssh.service.d/10-g5-hostkeys.conf

# --- 7. GCE guest environment: enable agent + metadata hosts entry ---------
#
# google-guest-agent was installed in 10-apt-deps.sh. Enable it (and the
# startup/shutdown-script units it ships) so that, on a real GCE instance, the
# agent runs the instance startup-script which writes /etc/g5/host.env for
# `g5d host`. apt's postinst normally enables them, but this image wipes
# cloud/machine state (90-cleanup.sh) and is otherwise hardened, so enable
# explicitly. `|| true`: a missing unit would already have aborted 10-apt-deps.sh.
systemctl enable google-guest-agent.service 2>/dev/null || true
systemctl enable google-startup-scripts.service 2>/dev/null || true
systemctl enable google-shutdown-scripts.service 2>/dev/null || true

# google-guest-configs (which writes the metadata /etc/hosts entry) is not in
# Ubuntu's default repos, so add the entry directly: the GCE metadata server is
# always reachable at the link-local 169.254.169.254. This makes
# `metadata.google.internal` resolvable for cloud-init's DataSourceGCE and any
# tooling that uses the hostname. (The host bootstrap already hits the IP.)
if ! grep -q "metadata.google.internal" /etc/hosts; then
  printf '169.254.169.254 metadata.google.internal metadata\n' >> /etc/hosts
fi

# --- 7b. GCE network: deterministic netplan (DHCP the NIC + metadata route) -
#
# THE actual root cause of "A3 worker boots but never registers": on this
# from-ISO image the GCE primary NIC (kernel renames eth0 -> ens4) never gets a
# DHCP address — the subiquity/installer netplan pins the build-time NIC name,
# and cloud-init can't generate a matching GCE config because that needs the
# metadata server, which needs the network (chicken-and-egg). Result observed on
# a real instance: `ens4 ... False` (no IP) -> 169.254.169.254 unreachable ->
# guest agent can't fetch the startup-script -> host bootstrap never runs.
#
# google-guest-configs solves this with a deterministic netplan; it's not
# installable (not in Ubuntu repos; Google's guest-env apt repo 404s), so ship
# our own: DHCPv4 on ANY ethernet NIC (`e*` matches ens4 on GCE and eth0/ens3 in
# the NoCloud test gates) plus the explicit on-link route to the link-local
# metadata server (DHCP does not provide it on GCE). cloud-init's network
# management is disabled so it can't race/override this.
#
# Test-gate safety: `e*` is broad enough to match the gate NICs too, the route
# is harmless where there's no metadata server, and the A2.5 version checks need
# no network anyway.
cat > /etc/cloud/cloud.cfg.d/99-g5-disable-network-config.cfg <<'EOF'
# g5-worker ships a deterministic netplan (90-g5-gce.yaml); cloud-init must not
# race it (it can't reach metadata to build a GCE config anyway).
network: {config: disabled}
EOF
chmod 0644 /etc/cloud/cloud.cfg.d/99-g5-disable-network-config.cfg

cat > /etc/netplan/90-g5-gce.yaml <<'EOF'
network:
  version: 2
  renderer: networkd
  ethernets:
    g5-eth:
      match:
        name: "e*"
      dhcp4: true
      dhcp6: false
      dhcp4-overrides:
        use-routes: true
      # On-link route to the GCE metadata server. The link-local 169.254.169.254
      # is NOT routable via the default gateway and DHCP does not push it, so add
      # it explicitly or the startup-script never reaches metadata.
      routes:
        - to: 169.254.169.254/32
          scope: link
EOF
chmod 0600 /etc/netplan/90-g5-gce.yaml
# Drop the installer/cloud-init netplans that pin the build-time NIC name.
rm -f /etc/netplan/00-installer-config.yaml /etc/netplan/50-cloud-init.yaml

# --- 8. Cloud Ops Agent: ship journald to Cloud Logging --------------------
#
# A no-external-IP, hardened worker host is otherwise unobservable: SSH is not
# guaranteed, the serial buffer rolls off, and `g5d host` (Type=simple) logs
# only to journald. The Ops Agent forwards systemd-journald — including the
# supervisor's registration, heartbeat and workspace-VM output — to Cloud Logging,
# which is the right observability channel for these hosts. The instance SA has
# logging.logWriter + monitoring.metricWriter.
#
# Installed from Google's apt repo directly. (The dl.google.com
# add-google-cloud-ops-agent.sh convenience script now 404s, and `curl -sSO`
# without -f silently saved the 404 HTML, which `bash` then choked on.) The
# repo distribution is codename-scoped: `google-cloud-ops-agent-<codename>-all`.
echo "[g5-worker] installing google-cloud-ops-agent from apt"
. /etc/os-release
OPS_AGENT_KEY="$(mktemp)"
download_with_retry "google-cloud ops-agent apt key" \
  https://packages.cloud.google.com/apt/doc/apt-key.gpg "$OPS_AGENT_KEY"
gpg --dearmor -o /usr/share/keyrings/google-cloud-ops-agent.gpg < "$OPS_AGENT_KEY"
rm -f "$OPS_AGENT_KEY"
cat > /etc/apt/sources.list.d/google-cloud-ops-agent.list <<EOF
deb [signed-by=/usr/share/keyrings/google-cloud-ops-agent.gpg] https://packages.cloud.google.com/apt google-cloud-ops-agent-${VERSION_CODENAME}-all main
EOF
apt-get update -qq
apt-get install -y --no-install-recommends google-cloud-ops-agent

# Collect systemd-journald on top of the agent's built-in defaults so the
# supervisor's logs land in Cloud Logging (log name `journald`).
install -d -m 0755 /etc/google-cloud-ops-agent
cat > /etc/google-cloud-ops-agent/config.yaml <<'EOF'
logging:
  receivers:
    journald:
      type: systemd_journald
  service:
    pipelines:
      journald_pipeline:
        receivers: [journald]
EOF
chmod 0644 /etc/google-cloud-ops-agent/config.yaml
systemctl enable google-cloud-ops-agent.service 2>/dev/null || true

# --- 9. Disable OS auto-maintenance (G5#1511) -------------------------------
#
# Stock Ubuntu's apt-daily-upgrade restarted g5d-host.service on live prod
# hosts (libsystemd upgrade -> systemd re-exec -> unit stop/start). The
# supervisor's graceful-shutdown terminate call then moved the worker_hosts
# row to `terminated`, permanently revoking the baked-in host_jwt — a healthy
# VM left crash-looping forever with zero workers for its org. Worker hosts
# are immutable and force-rotated at the 24h credential cliff; OS patching
# happens at image build time (10-apt-deps.sh dist-upgrade) and reaches the
# fleet through the signed image-family rollout. Mask the periodic apt units
# entirely — needrestart / unattended-upgrades exclusion lists are strictly
# weaker (they do not survive a systemd re-exec). See
# deployment_model/worker_image_and_microvm_rollout [D8].
systemctl mask apt-daily.timer apt-daily-upgrade.timer \
    apt-daily.service apt-daily-upgrade.service \
    unattended-upgrades.service

# Belt + suspenders: zero the APT::Periodic knobs so even a manually invoked
# /usr/lib/apt/apt.systemd.daily exits without touching packages.
cat > /etc/apt/apt.conf.d/99-g5-disable-periodic <<'EOF'
// Provisioned by g5-worker image — see scripts/80-configure-host.sh §9 and
// deployment_model/worker_image_and_microvm_rollout [D8] (G5#1511).
APT::Periodic::Update-Package-Lists "0";
APT::Periodic::Download-Upgradeable-Packages "0";
APT::Periodic::AutocleanInterval "0";
APT::Periodic::Unattended-Upgrade "0";
EOF
chmod 0644 /etc/apt/apt.conf.d/99-g5-disable-periodic

echo "[g5-worker] 80-configure-host.sh OK"
