#!/usr/bin/env bash
# Regression for scripts/preview-microvm-host.sh (design nested-virt-followup
# §3.5): the full-stack preview's prerequisite check fails with the reason and
# names the process-mode sibling, never falling back by itself
# (g5.host.isolation). The success path needs a platform and root, which the
# strict KVM lane and the QA acceptance exercise; here the refusals are driven
# with a fake platform directory, a uname shim and an injected device path
# (a missing node for "no KVM", /dev/null, a usable character device, for
# "KVM present"), so the outcome does not depend on the machine's /dev/kvm.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fail() { echo "✗ $*" >&2; exit 1; }
SCRIPT="$ROOT/scripts/preview-microvm-host.sh"

run_check() { # <platform dir> <uname -s> <uname -m> <kvm: yes|no> → stderr file; returns the exit status
    local platform="$1" os="$2" arch="$3" kvm="$4" status=0 device
    mkdir -p "$TMP/bin"
    # The script consults uname and a device node; stand in for both.
    printf '#!/bin/sh\ncase "$1" in -s) echo %s ;; -m) echo %s ;; esac\n' "$os" "$arch" > "$TMP/bin/uname"
    chmod 0755 "$TMP/bin/uname"
    if [ "$kvm" = yes ]; then device=/dev/null; else device="$TMP/no-such-kvm"; fi
    G5_DEV_HOST_PLATFORM_DIR="$platform" G5_DEV_HOST_DIR="$TMP/host" G5_DEV_HOST_KVM_DEVICE="$device" \
        G5_DEV_HOST_DOTENV="${RUN_CHECK_DOTENV:-$TMP/no-dotenv}" PATH="$TMP/bin:$PATH" \
        bash "$SCRIPT" >"$TMP/out" 2>"$TMP/err" || status=$?
    return "$status"
}

# 1. Not Linux x86_64: refused with the sibling named.
if run_check "$TMP/none" Darwin arm64 no; then fail "a non-Linux machine must be refused"; fi
grep -q 'needs Linux x86_64' "$TMP/err" || fail "the refusal must name the platform requirement: $(cat "$TMP/err")"
grep -q "full-stack-process" "$TMP/err" || fail "the refusal must point at the process-mode profile"
echo "✓ a non-Linux machine is refused with a pointer to full-stack-process"

# 2. Linux without a usable /dev/kvm (the host's switch is off): refused.
if run_check "$TMP/none" Linux x86_64 no; then fail "a guest without /dev/kvm must be refused"; fi
grep -q 'nested virtualization off' "$TMP/err" || fail "the refusal must explain the missing /dev/kvm: $(cat "$TMP/err")"
grep -q "full-stack-process" "$TMP/err" || fail "the refusal must point at the process-mode profile"
echo "✓ a guest without /dev/kvm is refused with the host switch named"

# 3. The order of checks puts the platform after KVM, and the platform check
#    names the missing file.
mkdir -p "$TMP/partial"
printf '' > "$TMP/partial/cloud-hypervisor"
if run_check "$TMP/partial" Linux x86_64 yes; then fail "a partial platform must be refused"; fi
grep -q 'no vmlinux under' "$TMP/err" || fail "the refusal must name the missing platform file: $(cat "$TMP/err")"
grep -q "full-stack-process" "$TMP/err" || fail "the refusal must point at the process-mode profile"
echo "✓ a runtime disk without the platform is refused"

# 3b. A platform kernel without Landlock is refused before anything is mounted.
mkdir -p "$TMP/nolandlock"
for f in cloud-hypervisor vmlinux initramfs.cpio passt; do printf '' > "$TMP/nolandlock/$f"; done
printf 'CONFIG_KVM=y\n' > "$TMP/nolandlock/kernel.config"
if run_check "$TMP/nolandlock" Linux x86_64 yes; then fail "a platform kernel without Landlock must be refused"; fi
grep -q 'has no Landlock' "$TMP/err" || fail "the refusal must name Landlock: $(cat "$TMP/err")"
echo "✓ a platform kernel without Landlock is refused"

# 3c. The mode is pinned: an ambient or dotenv selection of process mode is
#     refused before anything is mounted, naming the sibling. A dotenv that
#     only mentions the key in a comment, or pins microvm, passes this check.
make_full_platform() { mkdir -p "$1"; for f in cloud-hypervisor vmlinux initramfs.cpio passt; do printf '' > "$1/$f"; done; printf 'CONFIG_SECURITY_LANDLOCK=y\n' > "$1/kernel.config"; }
make_full_platform "$TMP/full"
if G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 run_check "$TMP/full" Linux x86_64 yes; then fail "an ambient process mode must be refused"; fi
grep -q 'runs the microvm host only' "$TMP/err" || fail "the refusal must name the pinned mode: $(cat "$TMP/err")"
printf '# G5D_ISOLATION_MODE=process is only a comment here\nexport G5D_ISOLATION_MODE="process"\n' > "$TMP/dotenv-process"
if RUN_CHECK_DOTENV="$TMP/dotenv-process" run_check "$TMP/full" Linux x86_64 yes; then fail "a dotenv selecting process mode must be refused"; fi
grep -q 'selects G5D_ISOLATION_MODE=process' "$TMP/err" || fail "the refusal must quote the dotenv selection: $(cat "$TMP/err")"
grep -q "full-stack-process" "$TMP/err" || fail "the refusal must point at the process-mode profile"
printf '# G5D_ISOLATION_MODE=process is only a comment here\nG5D_ISOLATION_MODE=microvm\n' > "$TMP/dotenv-microvm"
RUN_CHECK_DOTENV="$TMP/dotenv-microvm" run_check "$TMP/full" Linux x86_64 yes || true
grep -q 'G5D_ISOLATION_MODE' "$TMP/err" && fail "a dotenv pinning microvm must pass the mode check: $(cat "$TMP/err")"
echo "✓ an ambient or dotenv selection of process mode is refused; a microvm pin passes"

# 4. The profile wires the check before dev-up and exports the reflink root.
PROFILE="$ROOT/.g5/preview_profiles/full-stack.yaml"
COMMAND="$(sed -n 's/^command: *//p' "$PROFILE" | head -1)"
case "$COMMAND" in *'scripts/preview-microvm-host.sh && just dev-up'*) ;; *) fail "full-stack must run the prerequisite check right before dev-up: $COMMAND" ;; esac
grep -q '^  G5_DEV_HOST_DIR: /var/lib/g5-dev-host/root$' "$PROFILE" || fail "full-stack must place the host directory on the reflink root outside /work"
grep -q 'G5_DEV_HOST_REFLINK_ROOT="${G5_DEV_HOST_REFLINK_ROOT:-$G5_DEV_HOST_DIR}"' "$SCRIPT" || fail "the check must mount the reflink root at the profile's host directory"
grep -q 'just worker-host-setup' "$SCRIPT" || fail "the check must stage the inner guest runtime with worker-host-setup"
echo "✓ the profile runs the check before dev-up on the reflink root"
echo "preview-microvm-host: OK"
