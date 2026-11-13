#!/usr/bin/env bash
# =============================================================================
# boot-and-assert.sh — G5 worker image test gate, layer 1.
#
# Boots the freshly-built raw disk under qemu/KVM (inside a docker child
# container with --device=/dev/kvm), injects an ephemeral cloud-init SSH user
# via a NoCloud seed ISO, then runs the ten in-image asserts from
# infra/packer/g5-worker/README.md "Image-test gate":
#
#   1. systemctl is-enabled g5d-host.service is "enabled"
#   2. g5d-host.service carries LimitNOFILE=1048576
#   3. /etc/g5/worker-image.json parses + runtime_bin_versions.g5d.sha256 non-empty
#   4. /var/g5d-data/runtime/bin/g5d --version runs without dlopen errors
#   5. vm-exec --report-version identifies Cloud Hypervisor and the kernel digest
#   6. cp --reflink=always on XFS root produces SHARED extents (fiemap 0x2000)
#   7. backend/g5d/claude/codex/gh/gcloud/gke-gcloud-auth-plugin/kubectl plus
#      unit-test tool version probes run from the
#      /opt/g5/bin mount contract with no missing runtime deps
#      (A2.5 — minimal dev-grade self-contained proof)
#   8. apt-daily/apt-daily-upgrade timers+services and unattended-upgrades
#      are masked — OS auto-maintenance disabled (G5#1511)
#   9. mkfs.ext4 is on PATH (e2fsprogs retained) — required by the preview
#      scratch disk's host-side format (microvm_runtime D53)
#  10. the host runtime carries the pinned dispatcher module catalog required
#      for supervisor registration and workspace VM startup
#
# When --sbom-output is supplied, the gate also downloads a pinned syft binary
# inside the booted guest, scans `/` (excluding pseudo-filesystems), and writes
# the resulting SPDX JSON to the host. Scanning inside the running guest is
# more accurate than a guestfish tar-out scan because syft sees the kernel's
# view of mounted filesystems instead of a flat tree of decoded inodes.
#
# Authentication: the Packer template's identity scrub (90-cleanup.sh) removes
# the build-time `packer` user, so we can't SSH with build credentials. The
# NoCloud seed creates an ephemeral `g5ci` user with sudo + a freshly-generated
# ed25519 key that exists only for this run.
#
# Usage:
#   boot-and-assert.sh --raw-disk <path-to-disk.raw>
#                      [--sbom-output <path-to-output.spdx.json>]
#                      [--syft-version <X.Y.Z>]
#
# Environment overrides (rarely needed):
#   WORKDIR                  — scratch dir for the seed ISO + overlay (default /workspace/test-gate)
#   BOOT_TIMEOUT_SECONDS     — max wait for sshd inside the booted guest (default 600)
#   VM_MEMORY_MB             — qemu RAM in MB (default 4096)
#   VM_CPUS                  — qemu vCPUs (default 4)
#
# Returns 0 on full pass, non-zero with a serial-log dump on first failure.
# =============================================================================
set -euo pipefail

RAW_DISK=""
SBOM_OUTPUT=""
SYFT_VERSION="1.18.1"

usage() {
    cat <<'USAGE' >&2
Usage: boot-and-assert.sh --raw-disk PATH [--sbom-output PATH --syft-version X.Y.Z]
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --raw-disk)
            RAW_DISK="${2:-}"
            shift 2
            ;;
        --sbom-output)
            SBOM_OUTPUT="${2:-}"
            shift 2
            ;;
        --syft-version)
            SYFT_VERSION="${2:-}"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: unknown argument: $1" >&2
            usage
            exit 2
            ;;
    esac
done

[ -n "$RAW_DISK" ]   || { echo "ERROR: --raw-disk is required" >&2; exit 2; }
[ -f "$RAW_DISK" ]   || { echo "ERROR: raw disk not found: $RAW_DISK" >&2; exit 2; }

WORKDIR="${WORKDIR:-/workspace/test-gate}"
BOOT_TIMEOUT_SECONDS="${BOOT_TIMEOUT_SECONDS:-600}"
VM_MEMORY_MB="${VM_MEMORY_MB:-4096}"
VM_CPUS="${VM_CPUS:-4}"

mkdir -p "$WORKDIR"
SSH_KEY="$WORKDIR/id_g5ci"
OVERLAY="$WORKDIR/overlay.qcow2"
SEED_ISO="$WORKDIR/seed.iso"
SERIAL_LOG="$WORKDIR/serial.log"
QEMU_LOG="$WORKDIR/qemu.log"
VM_CONTAINER="g5-worker-test-vm-$$"

dump_logs_on_failure() {
    ec=$?
    if [ "$ec" -ne 0 ]; then
        echo "==> failure — dumping serial log (last 200 lines):" >&2
        if [ -f "$SERIAL_LOG" ]; then tail -n 200 "$SERIAL_LOG" >&2 || true; fi
        if [ -f "$QEMU_LOG" ];   then echo "==> qemu stderr:" >&2; tail -n 80 "$QEMU_LOG" >&2 || true; fi
        echo "==> docker logs (qemu container):" >&2
        docker logs "$VM_CONTAINER" 2>&1 | tail -n 100 >&2 || true
    fi
    docker rm -f "$VM_CONTAINER" >/dev/null 2>&1 || true
    return "$ec"
}
cleanup() {
    dump_logs_on_failure
    exit $?
}
trap cleanup EXIT INT TERM

# -----------------------------------------------------------------------------
# 1. Generate the SSH key + cloud-init NoCloud seed.
# -----------------------------------------------------------------------------
ssh-keygen -t ed25519 -N '' -f "$SSH_KEY" -C image-test-gate -q
PUBKEY="$(cat "${SSH_KEY}.pub")"

cat > "$WORKDIR/user-data" <<EOF
#cloud-config
hostname: g5-image-test
users:
  - name: g5ci
    groups: [adm, sudo, kvm]
    shell: /bin/bash
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: true
    ssh_authorized_keys:
      - $PUBKEY
ssh_pwauth: false
disable_root: false
package_update: false
package_upgrade: false
runcmd:
  - [ systemctl, daemon-reload ]
EOF

cat > "$WORKDIR/meta-data" <<EOF
instance-id: g5-image-test-$$
local-hostname: g5-image-test
EOF

# Wrap user-data + meta-data into a NoCloud seed ISO. We do this inside an
# Ubuntu container so the host doesn't need cloud-image-utils/genisoimage.
docker run --rm \
    -v "$WORKDIR:/work" \
    ubuntu:24.04 \
    bash -c '
        set -eu
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y --no-install-recommends cloud-image-utils >/dev/null
        cd /work
        cloud-localds seed.iso user-data meta-data
    '
test -s "$SEED_ISO" || { echo "ERROR: NoCloud seed ISO empty or missing" >&2; exit 1; }

# -----------------------------------------------------------------------------
# 2. Create a qcow2 overlay on top of the raw disk so the test boot never
#    mutates the artifact. Much cheaper than copying the 3.5 GB raw.
# -----------------------------------------------------------------------------
docker run --rm \
    -v "$(dirname "$RAW_DISK"):/raw:ro" \
    -v "$WORKDIR:/work" \
    ubuntu:24.04 \
    bash -c '
        set -eu
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y --no-install-recommends qemu-utils >/dev/null
        qemu-img create -f qcow2 -F raw -b "/raw/'"$(basename "$RAW_DISK")"'" /work/overlay.qcow2 >/dev/null
        qemu-img info /work/overlay.qcow2
    '
test -s "$OVERLAY" || { echo "ERROR: qcow2 overlay create failed" >&2; exit 1; }

# -----------------------------------------------------------------------------
# 3. Boot qemu inside a docker child container with /dev/kvm. Detached so the
#    parent can SSH in.
# -----------------------------------------------------------------------------
echo "==> booting raw-disk overlay under qemu/kvm (background, container=$VM_CONTAINER)"
# The qcow2 overlay's backing file is referenced as /raw/<disk> (the path it
# was created with in step 2), so this container MUST mount the raw disk's
# directory at /raw too — otherwise qemu can't open the backing file and the
# boot silently fails. Codex post-push review of PR #116 flagged this.
#
# Network model: Cloud Build runs every step container on the `cloudbuild`
# docker network. `-p 2222:22` would publish to the *build VM's* network
# namespace (not reachable from the step container's loopback). Instead we
# attach to the `cloudbuild` network and reach the qemu container by its
# container name (DNS-resolved by docker on that network). This matches
# the existing `cloudbuild-ci.yaml` pattern for postgres/g5-api.
# Opus 4.7 independent review of PR #116 flagged the prior `-p` approach
# as a guaranteed-timeout in production.
docker run -d \
    --name "$VM_CONTAINER" \
    --network=cloudbuild \
    --device=/dev/kvm \
    -v "$(dirname "$RAW_DISK"):/raw:ro" \
    -v "$WORKDIR:/work" \
    ubuntu:24.04 \
    bash -c '
        set -eu
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        # The Ubuntu package is qemu-system-x86 (it ships the
        # /usr/bin/qemu-system-x86_64 binary); there is no qemu-system-x86_64
        # package, so naming the binary here makes apt fail to locate it.
        apt-get install -y --no-install-recommends \
            qemu-system-x86 qemu-utils >/dev/null
        # Boot via legacy BIOS (SeaBIOS, qemu default — no pflash/OVMF). The
        # g5-worker image is intentionally BIOS-only: grub installs to the
        # bios_grub partition (autoinstall storage config), the GCE image is
        # created WITHOUT UEFI_COMPATIBLE, and Packer itself boots the disk via
        # BIOS during the build. Booting the gate under UEFI/OVMF would test a
        # firmware path the image never ships and hang waiting for an EFI
        # bootloader that the BIOS-only image does not populate in the ESP.
        exec qemu-system-x86_64 \
            -enable-kvm \
            -machine type=q35,accel=kvm \
            -cpu host \
            -m '"$VM_MEMORY_MB"' \
            -smp '"$VM_CPUS"' \
            -drive file=/work/overlay.qcow2,format=qcow2,if=virtio,cache=writeback \
            -drive file=/work/seed.iso,format=raw,if=virtio,media=cdrom,readonly=on \
            -netdev user,id=net0,hostfwd=tcp::22-:22 \
            -device virtio-net-pci,netdev=net0 \
            -nographic \
            -no-reboot \
            -serial file:/work/serial.log \
            -monitor none
    ' 2>"$QEMU_LOG"

# -----------------------------------------------------------------------------
# 4. Wait for SSH. Generous timeout — autoinstall first-boot can take up to
#    300s before cloud-init applies our seed and starts sshd.
# -----------------------------------------------------------------------------
echo "==> waiting for sshd inside booted image (up to ${BOOT_TIMEOUT_SECONDS}s)"
# Reach the qemu container by its DNS name on the cloudbuild network (port 22
# in-container; the qemu hostfwd inside the container maps that to the guest's
# port 22). This is the codified pattern from cloudbuild-ci.yaml.
SSH_OPTS=(
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o ConnectTimeout=5
    -o ServerAliveInterval=10
    -o ServerAliveCountMax=3
    -i "$SSH_KEY"
)
SSH_TARGET="g5ci@${VM_CONTAINER}"
deadline=$(( $(date +%s) + BOOT_TIMEOUT_SECONDS ))
ready=0
while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! docker inspect -f '{{.State.Running}}' "$VM_CONTAINER" 2>/dev/null | grep -q true; then
        echo "ERROR: qemu container exited before SSH became ready" >&2
        exit 1
    fi
    if ssh "${SSH_OPTS[@]}" "$SSH_TARGET" 'echo ssh-ok' 2>/dev/null | grep -q ssh-ok; then
        ready=1
        break
    fi
    sleep 5
done
if [ "$ready" -ne 1 ]; then
    echo "ERROR: timed out waiting for sshd inside booted image" >&2
    exit 1
fi
echo "==> sshd reachable"

# SC2029: we intentionally want the heredoc/quoted command body to expand on
# the remote (booted-guest) side, not on the host running this script.
# shellcheck disable=SC2029
run() { ssh "${SSH_OPTS[@]}" "$SSH_TARGET" "$@"; }

# -----------------------------------------------------------------------------
# 5. Asserts.
# -----------------------------------------------------------------------------
echo "==> assert 1/10 — g5d-host.service is enabled"
state="$(run 'systemctl is-enabled g5d-host.service' || true)"
[ "$state" = "enabled" ] || { echo "FAIL: g5d-host.service is-enabled = '$state'" >&2; exit 1; }

echo "==> assert 2/10 — g5d-host.service has host fd budget"
limit="$(run 'systemctl show g5d-host.service -p LimitNOFILE --value' || true)"
[ "$limit" = "1048576" ] || { echo "FAIL: g5d-host.service LimitNOFILE = '$limit'" >&2; exit 1; }

echo "==> SEC-C01 — independent host bundle and read-only service paths"
# shellcheck disable=SC2016
run "$(cat <<'REMOTE'
set -eu
    test -x /opt/g5-host/bin/g5d
    test ! /opt/g5-host/bin/g5d -ef /var/g5d-data/runtime/bin/g5d
    cmp /opt/g5-host/bin/g5d /var/g5d-data/runtime/bin/g5d
    test "$(systemctl show g5d-host.service -p ProtectSystem --value)" = strict
    systemctl show g5d-host.service -p ReadOnlyPaths --value | grep -F /opt/g5-host
    systemctl show g5d-host.service -p ReadOnlyPaths --value | grep -F /var/g5d-data/runtime
    LD_LIBRARY_PATH=/opt/g5-host/libs /opt/g5-host/bin/g5d --version
    # Test the service mount policy, not just the unit text. The disposable
    # service must retain workspace writes while denying both bundle writes.
    sudo systemd-run --wait --pipe --property=ProtectSystem=strict \
        --property='ReadWritePaths=/var/g5d-data /var/lib/g5d/rootfs' \
        --property="ReadOnlyPaths=/opt/g5-host /var/g5d-data/runtime" \
        /bin/sh -c 'set -eu
            touch /var/g5d-data/.sec-c01-write-probe
            rm /var/g5d-data/.sec-c01-write-probe
            if touch /opt/g5-host/.sec-c01-write-probe; then exit 1; fi
            if touch /var/g5d-data/runtime/.sec-c01-write-probe; then exit 1; fi
        '
REMOTE
)"

echo "==> assert 3/10 — /etc/g5/worker-image.json valid + has runtime_bin_versions.g5d.sha256"
# shellcheck disable=SC2016
run 'set -eu
    test -f /etc/g5/worker-image.json
    python3 - <<PY
import json, sys
data = json.load(open("/etc/g5/worker-image.json"))
sha = data.get("runtime_bin_versions", {}).get("g5d", {}).get("sha256", "")
if not sha:
    sys.exit("FAIL: runtime_bin_versions.g5d.sha256 missing or empty")
print("  g5d sha256:", sha)
PY
'

echo "==> assert 4/10 — g5d --version runs without dlopen errors"
out="$(run '/var/g5d-data/runtime/bin/g5d --version 2>&1')"
echo "$out"
# Codex review N1: fail if ANY line matches the dlopen error pattern (not only
# when EVERY line matches). The prior `grep -v -q` form silently let mixed
# output through.
if echo "$out" | grep -qiE 'dlopen|cannot open shared'; then
    echo "FAIL: dlopen error in g5d --version" >&2
    exit 1
fi

echo "==> assert 5/10 — g5d identifies Cloud Hypervisor and the platform kernel"
out="$(run '/var/g5d-data/runtime/bin/g5d vm-exec --report-version 2>&1')"
echo "$out"
python3 -c '
import json, re, sys
version = json.load(sys.stdin)
assert version["backend"] == "cloud-hypervisor", version
assert version["version"].strip(), version
assert re.fullmatch(r"[0-9a-f]{64}", version["kernel_sha256"]), version
' <<< "$out"

echo "==> assert 6/10 — cp --reflink=always works on XFS root"
# Codex review B3: the previous check `stat -c %b == 0` on the destination
# was wrong — valid XFS reflinks can still report allocated blocks for the
# destination inode. Instead: (a) confirm the filesystem hosting the dest
# was built with reflink=1 (mkfs flag, durable on-disk), and (b) confirm
# the dest extents are SHARED via xfs_io fiemap (real reflink evidence).
# shellcheck disable=SC2016
run 'set -eu
    sudo apt-get install -y --no-install-recommends xfsprogs >/dev/null 2>&1 || true
    dd if=/dev/urandom of=/tmp/reflink-src bs=1M count=4 status=none
    sudo mkdir -p /var/g5d-data/lib/workspaces/reflink-test
    sudo cp --reflink=always /tmp/reflink-src /var/g5d-data/lib/workspaces/reflink-test/dst

    # (a) reflink=1 mkfs flag on the data filesystem
    data_fs="$(sudo df -P /var/g5d-data | awk '"'"'NR==2 {print $1}'"'"')"
    if ! sudo xfs_info "$data_fs" 2>/dev/null | grep -qE '"'"'reflink=1'"'"'; then
        echo "FAIL: data filesystem $data_fs not built with reflink=1" >&2
        sudo xfs_info "$data_fs" >&2 || true
        exit 1
    fi

    # (b) destination extents are SHARED (real reflink evidence). xfs_io
    # `fiemap -v` reports the FIEMAP_EXTENT_SHARED bit (0x2000) in the hex FLAGS
    # column (e.g. 0x2001 = LAST|SHARED), NOT the word "shared" — the previous
    # `grep -qi shared` therefore failed even on a correctly-reflinked file. Test
    # the bit instead. The only 0x tokens in fiemap -v output are extent flags.
    dst=/var/g5d-data/lib/workspaces/reflink-test/dst
    shared_found=0
    for f in $(sudo xfs_io -c "fiemap -v" "$dst" | grep -oE "0x[0-9a-fA-F]+"); do
        [ $(( f & 0x2000 )) -ne 0 ] && shared_found=1
    done
    if [ "$shared_found" -ne 1 ]; then
        echo "FAIL: dst extents are not SHARED — reflink fell back to full copy" >&2
        sudo xfs_io -c "fiemap -v" "$dst" >&2 || true
        exit 1
    fi
    echo "  reflink confirmed: xfs_info reflink=1 AND fiemap SHARED bit (0x2000) set"
'

echo "==> assert 7/10 — runtime CLIs run from the /opt/g5/bin mount contract (A2.5)"
# A2.5 (rev-7 dev micro-VM flip plan): prove backend/g5d/claude/codex/gh/gcloud/
# gke-gcloud-auth-plugin/kubectl and the Rust/client unit-test CLIs
# run from the rev-7 §5.4 mount-injection PATH (/opt/g5/bin), not just their build-time
# /var/g5d-data/runtime/bin location, with no missing shared libs. We bind-mount
# runtime/bin at /opt/g5/bin to emulate the read-only runtime mount the host
# makes into every customer micro-VM, then run each CLI's --version from there.
# This is the minimal DEV-grade static-linking proof (the binaries are
# self-contained against the worker's own glibc and the mount path resolves);
# the foreign-base-image / musl micro-VM compatibility is the deferred PNC
# zero-dependency story (Track C).
# shellcheck disable=SC2016
run 'set -eu
    sudo mkdir -p /opt/g5/bin
    # Emulate the rev-7 §5.4 read-only runtime mount of runtime/bin at /opt/g5/bin.
    if ! mountpoint -q /opt/g5/bin; then
        sudo mount --bind /var/g5d-data/runtime/bin /opt/g5/bin
    fi
    fail=0
    for cli in g5d claude codex gh gcloud gke-gcloud-auth-plugin kubectl npm pnpm corepack cargo rustc cargo-nextest just; do
        echo "--- /opt/g5/bin/$cli --version ---"
        if [ "$cli" = "kubectl" ]; then
            if out="$(/opt/g5/bin/kubectl version --client=true 2>&1)"; then
                echo "$out"
            else
                echo "FAIL: /opt/g5/bin/$cli version probe exited non-zero" >&2
                echo "$out" >&2
                fail=1
                continue
            fi
        else
            if out="$(/opt/g5/bin/$cli --version 2>&1)"; then
                echo "$out"
            else
                echo "FAIL: /opt/g5/bin/$cli version probe exited non-zero" >&2
                echo "$out" >&2
                fail=1
                continue
            fi
        fi
        if [ "$cli" = "cargo" ]; then
            echo "--- /opt/g5/bin/cargo nextest --version ---"
            if nextest_out="$(/opt/g5/bin/cargo nextest --version 2>&1)"; then
                echo "$nextest_out"
            else
                echo "FAIL: /opt/g5/bin/cargo nextest version probe exited non-zero" >&2
                echo "$nextest_out" >&2
                fail=1
            fi
        fi
        # Fail closed on a dynamic-linker / missing-runtime-path error even if the
        # process happened to exit 0.
        if printf "%s" "$out" | grep -qiE "error while loading shared lib|cannot open shared object|symbol lookup error|GLIBC_[0-9].* not found|no such file or directory"; then
            echo "FAIL: /opt/g5/bin/$cli reported a missing runtime dependency" >&2
            fail=1
        fi
    done
    sudo umount /opt/g5/bin 2>/dev/null || true
    [ "$fail" -eq 0 ] || exit 1
    echo "  runtime CLIs self-contained from /opt/g5/bin (g5d + claude + codex + gh + gcloud + gke-gcloud-auth-plugin + kubectl + npm/pnpm + Rust test tools)"
'

echo "==> assert 7b/10 — /opt/g5/bin/claude is the pinned release and carries the runner flag surface"
# The provider runner (design v6 §8.3) passes a fixed flag set to `claude -p`;
# scripts/check-claude-cli-flags.sh is the single checker for that contract
# (docs/lean-provider-cli-contract.md). Copy it into the guest and run it
# against the mounted binary with the repository pin as the expected version.
CLAUDE_PIN_EXPECTED="$(tr -d '[:space:]' < "$(dirname "$0")/../claude-code-cli-version")"
[ -n "$CLAUDE_PIN_EXPECTED" ] || { echo "FAIL: scripts/claude-code-cli-version is empty" >&2; exit 1; }
CODEX_PIN_EXPECTED="$(tr -d '[:space:]' < "$(dirname "$0")/../codex-cli-version")"
scp "${SSH_OPTS[@]}" "$(dirname "$0")/../check-codex-cli-flags.sh" "${SSH_TARGET}:/tmp/g5-check-codex-cli-flags.sh"
scp "${SSH_OPTS[@]}" "$(dirname "$0")/../check-claude-cli-flags.sh" "${SSH_TARGET}:/tmp/g5-check-claude-cli-flags.sh"
# shellcheck disable=SC2016
run "set -eu
    sudo mkdir -p /opt/g5/bin
    if ! mountpoint -q /opt/g5/bin; then
        sudo mount --bind /var/g5d-data/runtime/bin /opt/g5/bin
    fi
    sh /tmp/g5-check-claude-cli-flags.sh /opt/g5/bin/claude '${CLAUDE_PIN_EXPECTED}'
    sh /tmp/g5-check-codex-cli-flags.sh /opt/g5/bin/codex '${CODEX_PIN_EXPECTED}'
    sudo umount /opt/g5/bin 2>/dev/null || true
"

echo "==> assert 8/10 — OS auto-maintenance masked (G5#1511)"
# Stock apt-daily-upgrade restarting g5d-host.service permanently locked
# prod hosts out of the control plane (terminate-on-shutdown revokes the baked
# host_jwt). 80-configure-host.sh masks the periodic apt machinery; assert the
# mask survived into the image so a base-image or script regression fails the
# build instead of shipping a fleet-wide time bomb.
# shellcheck disable=SC2016
run 'set -eu
    fail=0
    for unit in apt-daily.timer apt-daily-upgrade.timer apt-daily.service apt-daily-upgrade.service unattended-upgrades.service; do
        state="$(systemctl is-enabled "$unit" 2>&1 || true)"
        if [ "$state" != "masked" ]; then
            echo "FAIL: $unit is-enabled = $state (expected masked)" >&2
            fail=1
        fi
    done
    grep -q "APT::Periodic::Unattended-Upgrade \"0\"" /etc/apt/apt.conf.d/99-g5-disable-periodic || {
        echo "FAIL: APT::Periodic::Unattended-Upgrade not zeroed in /etc/apt/apt.conf.d/99-g5-disable-periodic" >&2
        fail=1
    }
    [ "$fail" -eq 0 ] || exit 1
    echo "  OS auto-maintenance disabled: apt-daily timers/services + unattended-upgrades masked, APT::Periodic zeroed"
'

echo "==> assert 9/10 — mkfs.ext4 present for the preview scratch disk (microvm_runtime D53)"
# `g5d vm-exec` formats the per-VM scratch backing file host-side and FAILS a
# scratch-requesting boot closed when mkfs.ext4 is missing; assert the image
# retains e2fsprogs so a base-image slimming cannot silently break every
# knob-enabled preview on the fleet.
run 'set -eu
    command -v mkfs.ext4 >/dev/null || { echo "FAIL: mkfs.ext4 not on PATH (e2fsprogs missing)" >&2; exit 1; }
    echo "  mkfs.ext4 present: $(command -v mkfs.ext4)"
'

echo "==> assert 10/10 — host runtime carries the pinned module catalog"
run 'set -eu
    catalog_root=/var/g5d-data/runtime/modules
    test -s "$catalog_root/catalog.json"
    test -s "$catalog_root/catalog.sha256"
    echo "$(tr -d "[:space:]" < "$catalog_root/catalog.sha256")  $catalog_root/catalog.json" | sha256sum -c -
'

echo "==> all ten asserts passed"

# -----------------------------------------------------------------------------
# 6. Optional: in-guest SBOM scan with syft. Scanning inside the running
#    guest gives a more accurate picture than scanning a guestfish tar-out
#    of the raw disk (syft sees the kernel's view of mounted filesystems,
#    including any setuid/setgid bits, ACLs, etc.).
# -----------------------------------------------------------------------------
if [ -n "$SBOM_OUTPUT" ]; then
    echo "==> generating SBOM in-guest with syft v$SYFT_VERSION"
    # Download a pinned syft inside the guest so we don't depend on the
    # guest's package state matching ours. The guest has outbound HTTPS via
    # the user-mode networking the qemu instance gets by default.
    # shellcheck disable=SC2016
    run 'set -eu
        SYFT_URL="https://github.com/anchore/syft/releases/download/v'"$SYFT_VERSION"'/syft_'"$SYFT_VERSION"'_linux_amd64.tar.gz"
        curl -fsSL "$SYFT_URL" -o /tmp/syft.tgz
        tar -C /tmp -xzf /tmp/syft.tgz syft
        chmod +x /tmp/syft
        # syft exclusions are relative to the scan root (dir:/) and MUST start
        # with ./, */, or **/ — absolute /dev,/proc,... are rejected as invalid
        # patterns. ./proc etc. match the pseudo-fs dirs so syft skips them.
        sudo /tmp/syft dir:/ \
            --exclude ./dev \
            --exclude ./proc \
            --exclude ./run \
            --exclude ./sys \
            --exclude ./tmp \
            -o spdx-json=/tmp/g5-worker.sbom.spdx.json
        test -s /tmp/g5-worker.sbom.spdx.json
    '
    mkdir -p "$(dirname "$SBOM_OUTPUT")"
    scp "${SSH_OPTS[@]}" "${SSH_TARGET}:/tmp/g5-worker.sbom.spdx.json" "$SBOM_OUTPUT"
    test -s "$SBOM_OUTPUT" || { echo "ERROR: SBOM scp produced empty file at $SBOM_OUTPUT" >&2; exit 1; }
    echo "==> SBOM written to $SBOM_OUTPUT ($(wc -c <"$SBOM_OUTPUT") bytes)"
fi

# -----------------------------------------------------------------------------
# 7. Graceful shutdown.
# -----------------------------------------------------------------------------
run 'sudo shutdown -h now || true' || true
sleep 5
docker rm -f "$VM_CONTAINER" >/dev/null 2>&1 || true
trap - EXIT INT TERM
echo "image-test gate (layer 1, boot+asserts+SBOM) PASSED"
