#!/usr/bin/env bash
# Prepare a workspace VM for the `full-stack` preview's worker host in microvm
# mode, or fail with the reason. Never a fallback to process mode (ontology
# g5.host.isolation): a VM that cannot run the microvm host says so and names
# the `full-stack-process` profile. Design:
# .g5/pr_ontology_diff_design/nested-virt-followup/ontology-plan.md §3.5.
#
# Checks, in order: Linux x86_64; a usable /dev/kvm (the host's nested
# virtualization switch, g5.architecture.microvm_backend.nested); the platform
# the runtime disk ships under /opt/g5/platform; the inner host's identities
# in the guest image; nft. Then it mounts the reflink root the microvm host
# requires and, once per VM, stages the inner host's Linux guest runtime
# with `just worker-host-setup` (owner decision 2: the canonical staging,
# which builds this branch's own guest g5d). Later opens in the same VM reuse
# the staged runtime under the reflink root.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
platform=${G5_DEV_HOST_PLATFORM_DIR:-/opt/g5/platform}
# The device node the check probes and the checkout dotenv the dev recipes
# source; the regression test points both elsewhere.
kvm=${G5_DEV_HOST_KVM_DEVICE:-/dev/kvm}
dotenv=${G5_DEV_HOST_DOTENV:-$repo/.env}
fail() {
    echo "✗ full-stack preview (microvm host): $*" >&2
    echo "  For the process-mode stack (macOS, ARM, or a host without nested virtualization) open the 'full-stack-process' profile instead." >&2
    exit 1
}
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] \
    || fail "the microvm host needs Linux x86_64; this is $(uname -s) $(uname -m)"
# This profile is the VM-isolated stack: the mode must stay microvm through
# every recipe. The profile exports it, but the dev recipes source the
# checkout's .env with `set -a`, which would override that export, so a
# dotenv selecting another mode is refused here instead of silently starting
# an unisolated host (Codex review).
[ "${G5D_ISOLATION_MODE:-microvm}" = microvm ] \
    || fail "G5D_ISOLATION_MODE=$G5D_ISOLATION_MODE is set; this profile runs the microvm host only"
if [ -f "$dotenv" ]; then
    dotenv_mode=$(sed -n 's/^[[:space:]]*\(export[[:space:]]\{1,\}\)\{0,1\}G5D_ISOLATION_MODE=["'"'"']\{0,1\}\([^"'"'"'[:space:]#]*\).*/\2/p' "$dotenv" | tail -1)
    [ -z "$dotenv_mode" ] || [ "$dotenv_mode" = microvm ] \
        || fail "$dotenv selects G5D_ISOLATION_MODE=$dotenv_mode, which the dev recipes would apply over this profile's microvm mode; remove it or open full-stack-process"
fi
[ -c "$kvm" ] && [ -r "$kvm" ] && [ -w "$kvm" ] \
    || fail "$kvm is not usable in this VM: its host runs with nested virtualization off"
for file in cloud-hypervisor vmlinux initramfs.cpio kernel.config passt; do
    [ -f "$platform/$file" ] || fail "no $file under $platform: this runtime disk ships no platform (image built before the platform directory, or a process-only staging)"
done
grep -q '^CONFIG_SECURITY_LANDLOCK=y$' "$platform/kernel.config" \
    || fail "the platform kernel under $platform has no Landlock; Cloud Hypervisor's --landlock would fail every guest"
getent passwd g5-passt >/dev/null && getent passwd g5-vmm >/dev/null && getent group kvm >/dev/null \
    || fail "this guest image lacks the g5-passt/g5-vmm accounts or the kvm group (an older g5dev image)"
command -v nft >/dev/null || fail "nft is missing from this guest image"
: "${G5_DEV_HOST_DIR:?the profile must export G5_DEV_HOST_DIR on the reflink root}"
G5_DEV_HOST_REFLINK_ROOT="${G5_DEV_HOST_REFLINK_ROOT:-$G5_DEV_HOST_DIR}" "$repo/scripts/dev-host-reflink-root.sh"
[ "$(findmnt -n -o FSTYPE -T "$G5_DEV_HOST_DIR" 2>/dev/null)" = xfs ] \
    || fail "G5_DEV_HOST_DIR=$G5_DEV_HOST_DIR is not on the reflink root"
if [ ! -f "$G5_DEV_HOST_DIR/runtime/runtime.ext4" ]; then
    # Runtime staging precedes dev-up, so it must select the same host cache
    # before its Docker builder dependencies are resolved.
    source "$repo/scripts/lib/dev-cache-profile.sh"
    g5_load_dev_cache_profile
    echo "→ staging the inner host's guest runtime (first preview in this VM; later opens reuse it)"
    (cd "$repo" && just worker-host-setup)
fi
echo "✓ microvm host prerequisites ready (platform $platform, host directory $G5_DEV_HOST_DIR)"
