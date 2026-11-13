# =============================================================================
# worker-host-env.sh — Shared environment for the native local worker host
# (`g5d host` in Linux microVM or explicitly selected local process mode — the
# local microVM dev flip, g5/g5d/host D16).
#
# SOURCED (not executed) by the Justfile worker-host* recipes so the
# foreground, background, and setup paths can never drift. Everything lives
# under the gitignored .dev/g5d-host/ (or .dev/g5d-host-<slug>/ for
# a G5_WORKER_ORG per-org instance) — delete that directory for a full
# reset (the repo cache under it is a warm cache worth keeping otherwise).
#
# =============================================================================

# shellcheck disable=SC2034  # consumed by the sourcing recipe
GEN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The local host is a development loop, not a release artifact build. Keep its
# profile and binary path coupled so build, smoke, signing, and launch can never
# select different binaries. Release/CI callers do not source this script.
G5_DEV_HOST_CARGO_PROFILE="${G5_DEV_HOST_CARGO_PROFILE:-g5-dev}"
case "$G5_DEV_HOST_CARGO_PROFILE" in
    ""|*[!a-zA-Z0-9_-]*)
        echo "✗ invalid G5_DEV_HOST_CARGO_PROFILE '$G5_DEV_HOST_CARGO_PROFILE'" >&2
        return 1 2>/dev/null || exit 1
        ;;
esac
export G5_DEV_HOST_CARGO_PROFILE
# A preview can build beside feature-reduced tests in the same checkout.
# Keep its executable out of their output directory: the host re-executes
# itself for VM preparation and must keep the binary it was built to run.
G5_DEV_HOST_CARGO_TARGET_DIR="${G5_DEV_HOST_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-${GEN_ROOT}/target}}"
case "$G5_DEV_HOST_CARGO_TARGET_DIR" in
    /*) ;;
    *) G5_DEV_HOST_CARGO_TARGET_DIR="${GEN_ROOT}/$G5_DEV_HOST_CARGO_TARGET_DIR" ;;
esac
export G5_DEV_HOST_CARGO_TARGET_DIR
G5_WORKER_HOST_G5D="${G5_DEV_HOST_CARGO_TARGET_DIR}/${G5_DEV_HOST_CARGO_PROFILE}/g5d"
export G5_WORKER_HOST_G5D
# Per-org host instances: G5_WORKER_ORG is INTERNAL plumbing — the worker-host*
# recipes always export it from their positional `org` argument (dev-up sets it
# from the G5_WORKER_ORGS roster), so it is never a user-facing knob. When
# non-empty it moves every host-scoped path (pid/log/mode/fingerprint files,
# warm repo cache, staged guest runtime) under a per-org dir so one
# one `g5d host` per org can run concurrently. The stray-reaper below scopes by
# $G5_DEV_HOST_DIR, so per-org instances are mutually invisible to each
# other's reap — the same mechanism that protects sibling clones. Empty keeps
# the historical singleton dir. Exported because child processes
# (stage-local-worker-runtime.sh) derive their default output dir from it.
if [ -n "${G5_WORKER_ORG:-}" ]; then
    # Reject unsafe slugs BEFORE any path or downstream use: the slug is
    # interpolated into filesystem paths here and into an env-file name in
    # seed-dev-worker-token.sh, so path separators / traversal / metacharacters
    # must fail closed (PR #1530 review). Org slugs are lowercase
    # alphanumerics and dashes, not dash-leading.
    case "$G5_WORKER_ORG" in
        *[!a-z0-9-]*|-*)
            echo "✗ invalid org slug '${G5_WORKER_ORG}' — expected lowercase [a-z0-9-], not starting with '-'" >&2
            return 1 2>/dev/null || exit 1
            ;;
    esac
    export G5_WORKER_ORG
    # Deterministic per-org dir: deliberately IGNORE any inherited
    # G5_DEV_HOST_DIR — a nested `just` invocation (the worker-host-stop
    # sweep, dev-up's roster loop) runs after its parent already sourced this
    # file and exported the PARENT instance's dir, which would otherwise leak
    # in via the :- default and scope this org's instance to the wrong dir.
    G5_DEV_HOST_DIR="${GEN_ROOT}/.dev/g5d-host-${G5_WORKER_ORG}"
else
    G5_DEV_HOST_DIR="${G5_DEV_HOST_DIR:-${GEN_ROOT}/.dev/g5d-host}"
fi
export G5_DEV_HOST_DIR

# The local dev flip ALWAYS targets the LOCAL g5-api. G5D_API_URL is ALSO the
# variable the `g5d` CLI uses to point at a REMOTE env (e.g. dev.g5labs.ai), so
# devs commonly have it exported in their shell. Honoring that ambient value
# here would aim local worker registration + token seeding at the cloud — whose
# dev-only POST /api/v1/auth/test-token returns 404, failing `just dev-up` with
# "mint worker registration token via https://dev.g5labs.ai". Ignore the ambient
# G5D_API_URL; default to localhost and accept ONLY a dedicated, purpose-named
# override (G5_WORKER_HOST_API_URL) for the rare remote-API case.
export G5D_API_URL="${G5_WORKER_HOST_API_URL:-http://localhost:${G5_PORT_API:-7500}}"
# Workspace isolation (ontology g5.runtime.boot.backend.process-local).
# `microvm` (default) boots one Cloud Hypervisor guest per attached workspace: the host
# owns lifecycle and dispatch while the guest runs the shipped runtime and
# native tools against the workspace's fixed repository set. `process` runs
# every workspace as unisolated host processes — localhost development only,
# e.g. to run the `full-stack` preview profile on this Mac's own Docker — and
# needs G5D_ALLOW_PROCESS_ISOLATION=1 in the same environment; `g5d host`
# refuses the mode without it. Anything else is refused rather than ignored.
export G5D_ISOLATION_MODE="${G5D_ISOLATION_MODE:-microvm}"
case "$G5D_ISOLATION_MODE" in
    microvm) ;;
    process)
        case "${G5D_ALLOW_PROCESS_ISOLATION:-}" in
            1|true) export G5D_ALLOW_PROCESS_ISOLATION ;;
            *)
                echo "✗ G5D_ISOLATION_MODE=process runs workspaces as unisolated host processes (localhost development only)." >&2
                echo "  Set G5D_ALLOW_PROCESS_ISOLATION=1 to acknowledge that, or use G5D_ISOLATION_MODE=microvm." >&2
                return 1 2>/dev/null || exit 1
                ;;
        esac
        ;;
    *)
        echo "✗ unrecognized G5D_ISOLATION_MODE '$G5D_ISOLATION_MODE' — expected microvm or process" >&2
        return 1 2>/dev/null || exit 1
        ;;
esac

# Platform discovery for an inner host (ontology
# g5.architecture.microvm_backend.nested): inside a workspace VM the runtime
# disk ships the host's pinned VMM, kernel, initramfs, kernel config and passt
# under /opt/g5/platform (scripts/stage-runtime-platform.sh). Resolve the
# launch artifacts there when the caller set none; a real Linux host keeps the
# Rust defaults under /opt/g5-host, and explicit values always win. The
# kernel config stays beside the kernel, where platform_kernel_config() and
# the nested-virtualization guard read it.
G5_DEV_HOST_PLATFORM_DIR="${G5_DEV_HOST_PLATFORM_DIR:-/opt/g5/platform}"
export G5_DEV_HOST_PLATFORM_DIR
if [ "$G5D_ISOLATION_MODE" = microvm ] && [ "$(uname -s)" = Linux ] \
   && [ -x "$G5_DEV_HOST_PLATFORM_DIR/cloud-hypervisor" ]; then
    export G5D_CLOUD_HYPERVISOR_BIN="${G5D_CLOUD_HYPERVISOR_BIN:-$G5_DEV_HOST_PLATFORM_DIR/cloud-hypervisor}"
    export G5D_GUEST_KERNEL="${G5D_GUEST_KERNEL:-$G5_DEV_HOST_PLATFORM_DIR/vmlinux}"
    export G5D_GUEST_INITRAMFS="${G5D_GUEST_INITRAMFS:-$G5_DEV_HOST_PLATFORM_DIR/initramfs.cpio}"
    export G5D_PASST_BIN="${G5D_PASST_BIN:-$G5_DEV_HOST_PLATFORM_DIR/passt}"
fi

# Provider CLI version pins for the local guest runtime. Single canonical values
# for the local dev flip — `stage-local-worker-runtime.sh` inherits them via the
# environment, and g5_worker_host_ensure_runtime (below) compares it against the
# staged stamps to redeploy a bump on `just dev-up`.
# CLAUDE_CODE_VERSION and CODEX_VERSION default from the single-source pin
# files scripts/{claude-code,codex}-cli-version (#1342 D-13), shared with
# stage-local-worker-runtime.sh and the Packer image build; the env overrides
# remain for deliberate testing. The remaining pins below are still duplicated
# defaults — KEEP them EQUAL to infra/ci/cloudbuild-packer-worker.yaml and
# stage-local-worker-runtime.sh until consolidated the same way.
export CLAUDE_CODE_VERSION="${CLAUDE_CODE_VERSION:-$(cat "${GEN_ROOT}/scripts/claude-code-cli-version")}"
export CODEX_VERSION="${CODEX_VERSION:-$(cat "${GEN_ROOT}/scripts/codex-cli-version")}"
export GH_CLI_VERSION="${GH_CLI_VERSION:-2.95.0}"
export GH_CLI_VERSION="${GH_CLI_VERSION#v}"
export GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION:-575.0.0}"
export GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION#v}"
export KUBECTL_VERSION="${KUBECTL_VERSION:-1.36.2}"
export KUBECTL_VERSION="${KUBECTL_VERSION#v}"
export NODE_VERSION="${NODE_VERSION:-22.23.4}"
export NODE_VERSION="${NODE_VERSION#v}"
export PNPM_VERSION="${PNPM_VERSION:-11.9.0}"
export PNPM_VERSION="${PNPM_VERSION#v}"
# 1.97.1 tracks the workspace toolchain (safe __cpuid needs rustc >= 1.97).
export RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"
export RUSTUP_VERSION="${RUSTUP_VERSION:-1.29.0}"
export RUSTUP_VERSION="${RUSTUP_VERSION#v}"
export JUST_VERSION="${JUST_VERSION:-1.55.1}"
export JUST_VERSION="${JUST_VERSION#v}"
export CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION:-0.9.138}"
export CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION#cargo-nextest-}"
export CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION#v}"

# The provisioning helper replaces these values before `g5d host` starts.
export G5D_HOST_ID="${G5D_HOST_ID:-}"

# The worker-host-start readiness gate greps the log for the supervisor's
# info-level "Host registered with g5-api" line. A repo/dev .env commonly sets
# a global RUST_LOG=warn, so preserve caller filters but always make g5d's own
# readiness logs visible unless the caller already requested info-or-more.
case ",${RUST_LOG:-}," in
    *,g5d=info,*|*,g5d=debug,*|*,g5d=trace,*)
        ;;
    *)
        export RUST_LOG="${RUST_LOG:+${RUST_LOG},}g5d=info"
        ;;
esac

# Repo-local roots: the /var/g5d-data defaults of the signed worker image are
# not writable on macOS. G5D_ROOTFS_STATE_DIR in particular MUST be overridden
# (its /var/lib/g5d/rootfs default fails on macOS — isolation/microvm.rs).
# The runtime directory `g5d host` validates before registering: the
# Docker-staged Linux guest bundle for microvm, or the host-native directory
# g5_worker_host_stage_process_runtime assembles for process mode (a symlink to
# the host build of g5d and launchers for this machine's claude/codex/node/git/docker,
# plus this checkout's catalog and harness).
if [ "$G5D_ISOLATION_MODE" = "process" ]; then
    export G5_RUNTIME_BIN_DIR="${G5_DEV_HOST_DIR}/runtime-process/bin"
else
    export G5_RUNTIME_BIN_DIR="${G5_DEV_HOST_DIR}/runtime/bin"
fi
export G5D_HOST_RUNTIME_BIN_DIR="$G5_RUNTIME_BIN_DIR"
export G5D_HOST_STATE_DIR="${G5_DEV_HOST_DIR}/state"

# Worker images install `g5d` on PATH; the host flip keeps that parity, since
# the host build otherwise lives only in its Cargo profile.
export PATH="$(dirname "$G5_WORKER_HOST_G5D"):${PATH}"
export G5D_HOST_CACHE_ROOT="${G5_DEV_HOST_DIR}/cache/repos"
export G5D_HOST_WORKSPACES_ROOT="${G5_DEV_HOST_DIR}/workspaces"
export G5D_OCI_CACHE_DIR="${G5_DEV_HOST_DIR}/oci-cache"
export G5D_ROOTFS_STATE_DIR="${G5_DEV_HOST_DIR}/rootfs"

# Default workspace VM rootfs. Any DockerHub OCI ref works; the in-process
# extractor prepares its template in G5D_ROOTFS_STATE_DIR before registration. The
# with-identity in-VM worker runs `git` INSIDE the guest (the no-identity carrier
# did git host-side), and agent-run Rust/native package tests need the guest to
# provide `cc`, `ld`, `make`, `pkg-config`, and development headers. Bare
# a bare distro image ships neither git nor the linker stack, and a `*-scm`
# variant has git but no `cc`. `buildpack-deps:resolute` is the ubuntu-26.04
# image carrying both git and the native build baseline (verified: glibc 2.43,
# git 2.53.0, cc/gcc/make/pkg-config/ld).
#
# It must be resolute (26.04), not noble (24.04): the mounted `g5d` is now built
# on the 26.04 host plane and links glibc 2.43, so a noble guest (2.39) could
# not load it. That is the forbidden direction of
# system_invariants/microvm_execution_boundary [D12], which allows build-old /
# run-new only. Mirrors DEFAULT_BASE_IMAGE in
# backend/g5-api-core/src/cloud/startup_script.rs.
#
# Keep an OCI ref (extracted per-VM), not a directory (shared across VMs ->
# races). Override only with another image that preserves those guest-tool
# guarantees AND sits at or above the glibc floor. (See experience
# microvm_agent_in_vm_worker_needs_git_in_guest_base.)
export G5D_HOST_BASE_IMAGE="${G5D_HOST_BASE_IMAGE:-buildpack-deps:resolute}"

# Resolve CPU/RAM from this host unless the operator explicitly overrides them.
# A process host defaults to one slot: two process-mode workspaces would share
# this machine's loopback ports (g5.runtime.boot.backend.process-local).
if [ "$G5D_ISOLATION_MODE" = "process" ]; then
    export G5D_HOST_MAX_MICROVMS="${G5D_HOST_MAX_MICROVMS:-1}"
else
    export G5D_HOST_MAX_MICROVMS="${G5D_HOST_MAX_MICROVMS:-2}"
fi
export G5D_SCRATCH_DISK_MIB="${G5D_SCRATCH_DISK_MIB:-10240}"
# Nested virtualization (g5.architecture.microvm_backend.nested): off unless the
# operator sets `on`; `g5d host` then requires a platform kernel with KVM built in.
export G5D_NESTED_VIRTUALIZATION="${G5D_NESTED_VIRTUALIZATION:-off}"

# PID + log locations for the background lifecycle.
export G5_WORKER_HOST_PID_FILE="${G5_DEV_HOST_DIR}/host.pid"
export G5_WORKER_HOST_LOG_FILE="${G5_DEV_HOST_DIR}/host.log"
# Records the G5D_ISOLATION_MODE the live host was started with so a later
# `worker-host-start` after a config change restarts the host. Written next to
# the PID and checked by the start guard.
export G5_WORKER_HOST_MODE_FILE="${G5_DEV_HOST_DIR}/host.mode"
# Records the launch freshness hash for the live host so source/runtime changes
# restart the host instead of reusing a stale binary or guest runtime.
export G5_WORKER_HOST_LAUNCH_FINGERPRINT_FILE="${G5_DEV_HOST_DIR}/host.launch.sha256"

mkdir -p "$G5_DEV_HOST_DIR" "$G5D_HOST_STATE_DIR" "$G5D_HOST_CACHE_ROOT" \
    "$G5D_HOST_WORKSPACES_ROOT" "$G5D_OCI_CACHE_DIR" "$G5D_ROOTFS_STATE_DIR"

# Match the deployed supervisor's descriptor budget for concurrent dispatch,
# Git and image preparation. Respect macOS's system ceiling in process mode.
_g5_fd_target=1048576
_g5_fd_maxproc="$(sysctl -n kern.maxfilesperproc 2>/dev/null || true)"
if [ -n "$_g5_fd_maxproc" ] && [ "$_g5_fd_maxproc" -lt "$_g5_fd_target" ] 2>/dev/null; then
    _g5_fd_target="$_g5_fd_maxproc"
fi
# Raise the SOFT limit only (`-Sn`): a non-root process can lift soft up to its
# hard limit without touching hard, and the macOS default hard limit is
# `unlimited`. `|| true` keeps a constrained shell (hard < target) non-fatal.
if [ "$(ulimit -Sn)" != "unlimited" ] && [ "$(ulimit -Sn)" -lt "$_g5_fd_target" ] 2>/dev/null; then
    ulimit -Sn "$_g5_fd_target" 2>/dev/null || true
fi
unset _g5_fd_target _g5_fd_maxproc

# --- helpers (used by the recipes) -------------------------------------------

# Fail unless the one-time setup artifacts exist. No silent fallback (Rule 5):
# the caller decides to run worker-host-setup or switch to the process path.
g5_worker_host_check_artifacts() {
    local missing=0
    for asset in \
        g5d node claude codex gh gcloud \
        gke-gcloud-auth-plugin kubectl python3 npm npx corepack pnpm \
        cargo rustc cargo-nextest just ca-certificates.crt; do
        if [ ! -e "$G5_RUNTIME_BIN_DIR/$asset" ]; then
            echo "✗ missing $G5_RUNTIME_BIN_DIR/$asset" >&2
            missing=1
        fi
    done
    if [ ! -x "$G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gcloud" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gcloud" >&2
        missing=1
    fi
    if [ ! -x "$G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gke-gcloud-auth-plugin" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gke-gcloud-auth-plugin" >&2
        missing=1
    fi
    if [ ! -e "$G5_RUNTIME_BIN_DIR/_npm/bin/codex" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/_npm/bin/codex" >&2
        missing=1
    fi
    if [ ! -f "$G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/npm/bin/npm-cli.js" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/npm/bin/npm-cli.js" >&2
        missing=1
    fi
    if [ ! -f "$G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" >&2
        missing=1
    fi
    if [ ! -d "$G5_RUNTIME_BIN_DIR/rust/rustup/toolchains" ]; then
        echo "✗ missing $G5_RUNTIME_BIN_DIR/rust/rustup/toolchains" >&2
        missing=1
    fi
    if [ ! -x "$G5_WORKER_HOST_G5D" ]; then
        echo "✗ missing host binary $G5_WORKER_HOST_G5D" >&2
        missing=1
    fi
    if [ "$missing" = 1 ]; then
        echo "" >&2
        # Preserve the org in the guidance: a bare `worker-host-setup` stages
        # the DEFAULT .dev/g5d-host dir, so retrying a per-org start would
        # keep failing on its own still-missing dir (PR #1530 review P2).
        echo "Native worker host is not set up. Run:  just worker-host-setup${G5_WORKER_ORG:+ ${G5_WORKER_ORG}}" >&2
        echo "Run just worker-host-setup, then retry this command." >&2
        return 1
    fi
}

# A deterministic provider lane (`G5_CLAUDE_CODE_FAKE=1`, `G5_CODEX_FAKE=1`)
# resolves `fake-claude` / `fake-codex` beside the runtime's g5d instead of the
# real providers, exactly as in the guest bundle. A developer's host never
# needs them, so they are built and staged only when such a flag is set.
g5_worker_host_deterministic_providers() {
    case "${G5_CLAUDE_CODE_FAKE:-}${G5_CODEX_FAKE:-}" in
        "") return 1 ;;
    esac
    for value in "${G5_CLAUDE_CODE_FAKE:-}" "${G5_CODEX_FAKE:-}"; do
        case "$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')" in
            1|true|yes) return 0 ;;
        esac
    done
    return 1
}

# Process mode (g5.runtime.boot.backend.process-local): assemble the
# host-native runtime directory `g5d host` validates before registering. The
# dispatcher and the catalog's `g5d` entrypoints run the host build itself, so
# bin/g5d must resolve to it; claude, codex, node, git and docker are this
# machine's own installs; the catalog and harness are this checkout's; the
# fake provider CLIs (host build of `tools/fake-provider-cli`) are linked only
# for a deterministic provider lane. Fail closed on a missing tool: a host
# registered without it would fail every chat turn or preview later instead
# of now. Idempotent and cheap, so every start re-stages it (unlike the
# Docker-built guest bundle).
g5_worker_host_stage_process_runtime() {
    local root="${G5_DEV_HOST_DIR}/runtime-process" tool path launcher missing=0 skill fake expected actual fake_flag
    mkdir -p "$root/bin" "$root/modules" "$root/assets"
    [ -x "$G5_WORKER_HOST_G5D" ] || { echo "✗ missing host binary $G5_WORKER_HOST_G5D" >&2; return 1; }
    ln -sfn "$G5_WORKER_HOST_G5D" "$root/bin/g5d"
    for fake in fake-claude fake-codex fake-g5-tools; do
        path="$(dirname "$G5_WORKER_HOST_G5D")/$fake"
        if g5_worker_host_deterministic_providers; then
            [ -x "$path" ] || { echo "✗ missing host build of $fake at $path (a deterministic provider flag is set; g5_worker_host_build builds it beside g5d)" >&2; return 1; }
            ln -sfn "$path" "$root/bin/$fake"
        else
            rm -f "$root/bin/$fake"
        fi
    done
    for tool in claude codex node git docker; do
        if path="$(command -v "$tool" 2>/dev/null)"; then
            case "$tool" in
                claude) expected="$(cat "${GEN_ROOT}/scripts/claude-code-cli-version") (Claude Code)"; fake_flag="${G5_CLAUDE_CODE_FAKE:-}" ;;
                codex) expected="codex-cli $(cat "${GEN_ROOT}/scripts/codex-cli-version")"; fake_flag="${G5_CODEX_FAKE:-}" ;;
                *) expected=""; fake_flag="" ;;
            esac
            case "$(printf '%s' "$fake_flag" | tr '[:upper:]' '[:lower:]')" in
                1|true|yes) expected="" ;;
            esac
            if [ -n "$expected" ]; then
                actual="$("$path" --version 2>/dev/null)" || actual="version probe failed"
                if [ "$actual" != "$expected" ]; then
                    echo "✗ process mode needs pinned $tool at $path: expected '$expected', got '$actual'" >&2
                    echo "  Put the pinned CLI from scripts/*-cli-version first on PATH, then restart the worker host." >&2
                    return 1
                fi
            fi
            # Invoke the installed path itself: the worker-image Codex wrapper
            # finds its Node/npm closure relative to $0, which a new symlink
            # relocates into runtime-process/bin. exec preserves stdio, signals
            # and exit status. Replace old symlinks without writing through them.
            launcher="$(mktemp "$root/bin/.$tool.XXXXXX")" || return 1
            printf '#!/usr/bin/env bash\nexec %q "$@"\n' "$path" > "$launcher"
            chmod 0755 "$launcher"
            mv -f "$launcher" "$root/bin/$tool"
        else
            echo "✗ process mode needs '$tool' on PATH (host-native runtime)" >&2
            missing=1
        fi
    done
    if [ "$missing" = 1 ]; then
        echo "  Install the missing tools on this machine (Claude Code and Codex CLIs, Node.js, git, Docker), then retry." >&2
        return 1
    fi
    install -m 0644 "${GEN_ROOT}/backend/g5d-dispatch/modules/catalog.json" "$root/modules/catalog.json"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$root/modules/catalog.json" | awk '{print $1}' > "$root/modules/catalog.sha256"
    else
        sha256sum "$root/modules/catalog.json" | awk '{print $1}' > "$root/modules/catalog.sha256"
    fi
    rm -rf "$root/assets/harness"
    mkdir -p "$root/assets/harness/.agents"
    install -m 0644 "${GEN_ROOT}/.agents/AGENTS.md" "$root/assets/harness/AGENTS.md"
    cp -R "${GEN_ROOT}/.agents/skills" "$root/assets/harness/.agents/skills"
    cp -R "${GEN_ROOT}/.agents/guidance" "$root/assets/harness/.agents/guidance"
    for skill in "$root"/assets/harness/.agents/skills/*/; do
        [ -s "${skill}SKILL.md" ] || { echo "✗ harness skill incomplete: $skill" >&2; return 1; }
    done
    echo "✓ host-native process runtime staged at $root (g5d → $G5_WORKER_HOST_G5D)" >&2
}

# Mode-aware launch preparation shared by the worker-host recipes, after the
# host binary itself is built: microvm proves vm-exec, keeps the Docker-staged
# guest bundle current; process stages the host-native
# runtime directory and needs none of that.
g5_worker_host_prepare_runtime() {
    if [ "$G5D_ISOLATION_MODE" = "process" ]; then
        g5_worker_host_check_state_ownership || return 1
        g5_worker_host_stage_process_runtime
        return
    fi
    g5_worker_host_check_reflink_root || return 1
    g5_worker_host_check_host_vm_exec || return 1
    g5_worker_host_ensure_runtime || return 1
    g5_worker_host_check_artifacts || return 1
}

# Force-reap every microVM process that belongs to THIS repo's dev host but is
# NOT the live PID-file host — the cleanup a single graceful `kill -TERM` of the
# recorded PID cannot do. Two classes leak past `worker-host-stop` today:
#   * in-VM processes orphaned when their host
#     is SIGKILLed or crashes — a wedged in-VM task makes the host's graceful
#     drain hang, so the stop path SIGKILLs the host and its VMs reparent to
#     init and keep running (and keep polling the control plane);
#   * an extra `g5d host` from an earlier `dev-up` whose PID the file
#     overwrote — the start guard only ever checks the one recorded PID.
# Both then contend on the same localhost:7500 control plane and route chat turns
# nondeterministically to a stale/pre-fix host.
#
# Scope is THIS repo ONLY, so a sibling clone's stack is never touched: VMs are
# matched by the absolute "$G5_DEV_HOST_DIR/" (trailing slash — the default
# dir is a PREFIX of every per-org dir `.dev/g5d-host-<org>`, so a bare
# substring match would reap sibling per-org instances) carried in their
# `--rootfs`/`-V` args, and host processes by an open file under
# "$G5_DEV_HOST_DIR/" (via lsof, or /proc where lsof is absent; the user's
# own processes need no privilege, a root host is read through sudo). Reap a
# sibling clone (e.g. .../G5_archive) from its own checkout, or use
# aux_files/bug_fix/microVM/microvm_clean_repro.sh for a cross-repo sweep.
#
# $1 (optional): a PID to keep (the host this stop already handled). NEVER call
# this while a HEALTHY pid-file host is running — its own live VMs match the dir
# and would be killed; only call it on the stop path or before a fresh start.
g5_worker_host_reap_strays() {
    local keep_pid="${1:-}" reaped=0 pid cmd
    # Enumerate ONLY processes whose executable is `g5d` (matched by name via
    # `pgrep -x`, so a shell / editor / grep that merely MENTIONS these paths in
    # its argv is never a candidate — a substring match over `ps` output would
    # wrongly reap them). Then classify each by subcommand and scope to THIS
    # repo.
    for pid in $(pgrep -x g5d 2>/dev/null || true); do
        if [ "$pid" = "$keep_pid" ]; then continue; fi
        cmd="$(ps -o command= -p "$pid" 2>/dev/null || true)"
        case "$cmd" in
            *" vm-exec "*|*" agent-runner "*)
                # Stray in-VM child: its --rootfs/-V mounts live under the
                # dev-host dir, so scope by it — a sibling clone (different dir)
                # is left untouched.
                case "$cmd" in
                    *"$G5_DEV_HOST_DIR/"*)
                        if g5_worker_host_kill -KILL "$pid" 2>/dev/null; then
                            echo "  reaped stray microVM pid $pid" >&2
                            reaped=$((reaped + 1))
                        fi
                        ;;
                esac
                ;;
            *" dispatch"*|*" preview-serve"*|*" agent-turn"*|*" bootstrap"*|*" mcp"*)
                # Stray process-mode dispatcher or module process
                # (g5.runtime.boot.backend.process-local): its log, home and
                # checkout live under the dev-host dir, which lsof or /proc
                # shows; a sibling clone's processes do not. Each runs in its own group.
                if g5_worker_host_pid_holds_dir "$pid" "$G5_DEV_HOST_DIR"; then
                    g5_worker_host_kill -KILL "-$pid" 2>/dev/null || true
                    if g5_worker_host_kill -KILL "$pid" 2>/dev/null; then
                        echo "  reaped stray process-mode dispatcher pid $pid (this repo)" >&2
                        reaped=$((reaped + 1))
                    fi
                fi
                ;;
            *" host"*)
                # Orphan host for THIS repo: confirm it holds a file or its
                # cwd under the dev-host dir (lsof, or /proc where lsof is
                # absent; a root microvm host is read through sudo, chosen by
                # its owner). A sibling clone's host has a different dir and is
                # skipped. Drain briefly, then ensure it is gone.
                if g5_worker_host_pid_holds_dir "$pid" "$G5_DEV_HOST_DIR"; then
                    g5_worker_host_kill -TERM "$pid" 2>/dev/null || true
                    for _ in 1 2 3; do g5_worker_host_alive "$pid" || break; sleep 1; done
                    g5_worker_host_kill -KILL "$pid" 2>/dev/null || true
                    echo "  reaped orphan host pid $pid (this repo)" >&2
                    reaped=$((reaped + 1))
                fi
                ;;
        esac
    done
    # Always report — a silent success reads as "did nothing". When zero strays
    # exist the line confirms the reap actually ran and the repo is clean.
    if [ "$reaped" -gt 0 ]; then
        echo "✓ reaped $reaped stray g5d host/runtime process(es) under $G5_DEV_HOST_DIR" >&2
    else
        echo "✓ no stray g5d host/runtime processes for this repo ($G5_DEV_HOST_DIR)" >&2
    fi
    return 0
}

# Cargo verifies the current source before every launch; process mode omits VM support.
g5_worker_host_build() {
    if [ "$G5D_ISOLATION_MODE" = microvm ] && [ "$(uname -s)" != Linux ]; then
        echo "Cloud Hypervisor requires Linux/KVM. For local process mode explicitly set G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1." >&2
        return 1
    fi
    local -a features=() packages=(-p g5d)
    if [ "$G5D_ISOLATION_MODE" = "process" ]; then
        features=(--no-default-features)
        if g5_worker_host_deterministic_providers; then
            packages+=(-p fake-provider-cli)
        fi
    fi
    # One binary links at the end, so the workspace's jobs=2 cap (there to
    # keep several test links from exhausting memory) does not apply: use
    # every core unless the caller says otherwise. Measured 2026-10-13 in a QA
    # preview VM: the host build held two of four vCPUs for ten minutes. The
    # count travels as `--jobs`, never as CARGO_BUILD_JOBS: the guest image
    # primes its sccache without that variable and an exported CARGO_* key
    # would change every object's hash and miss the warm cache.
    local jobs
    case "${G5_DEV_HOST_CARGO_JOBS:-}" in
        ''|*[!0-9]*|0*)
            if [ -n "${G5_DEV_HOST_CARGO_JOBS:-}" ]; then
                echo "⚠  G5_DEV_HOST_CARGO_JOBS='${G5_DEV_HOST_CARGO_JOBS}' is not a positive integer — using the core count" >&2
            fi
            jobs="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
            ;;
        *) jobs="$G5_DEV_HOST_CARGO_JOBS" ;;
    esac
    echo "=== Building host g5d ($G5_DEV_HOST_CARGO_PROFILE, ${G5D_ISOLATION_MODE}${features[0]:+ ${features[*]}}, ${jobs} jobs) — ensures the launched host is not stale ===" >&2
    if ! ( cd "$GEN_ROOT" && SQLX_OFFLINE=true cargo build --target-dir "$G5_DEV_HOST_CARGO_TARGET_DIR" --profile "$G5_DEV_HOST_CARGO_PROFILE" --jobs "$jobs" "${packages[@]}" ${features[@]+"${features[@]}"} ); then
        echo "✗ host g5d build failed — refusing to launch a stale binary" >&2
        return 1
    fi
}

# Root boundary for a microvm host on Linux (design nested-virt-followup
# §3.5, owner decision 3): passt and the VMM drop to installer-owned
# identities and the egress policy runs nft, so `g5d host` needs root. Only
# the host process, the signals that stop or reap it (which follow the target
# process's owner, see g5_worker_host_kill), and the reflink-root mount cross
# this boundary; cargo, Docker, the staged runtime and every file under the
# checkout stay the caller's (the root-owned CARGO_HOME preview failure of
# 2026-10-14 is the trap). Process mode and macOS never see sudo.
# The host reads its configuration from the environment, so the caller's
# G5D_*/G5_* keys, PATH and HOME are preserved by name: sudoers resets PATH
# through secure_path even under -E, and -n refuses to prompt.
g5_worker_host_needs_root() {
    [ "$G5D_ISOLATION_MODE" = microvm ] && [ "$(uname -s)" = Linux ] && [ "$(id -u)" != 0 ]
}
g5_worker_host_root_env_list() {
    local names
    names="$(compgen -A export | grep -E '^(G5D_|G5_|RUST_LOG$|RUST_BACKTRACE$|TMPDIR$)' | tr '\n' ',' || true)"
    printf 'PATH,HOME,%s' "${names%,}"
}
# Run a command as root when the host needs it, else as is (kill, lsof, …).
g5_worker_host_as_root() {
    if g5_worker_host_needs_root; then
        sudo -n --preserve-env="$(g5_worker_host_root_env_list)" "$@"
    else
        "$@"
    fi
}
# Replace the current shell with the command (a backgrounded call leaves the
# host, not a wrapper subshell, at $!). The launcher may itself run inside an
# agent harness that bounds the file and data sizes of its process tree
# (g5d-dispatch's module envelope: 4 GiB each for a workspace preview's
# processes), and sudo keeps those bounds; the host's block templates (scratch
# disk × slots, 18 GiB in the full-stack preview) and its guests' memory belong
# to the previewed instance, not to that envelope (SIGXFSZ at template
# preparation, 2026-10-28). Root may lift them: prlimit execs the host in
# place, so the pid the launch leaves behind is still the host's.
g5_worker_host_exec_as_root() {
    if g5_worker_host_needs_root; then
        local -a lift=()
        command -v prlimit >/dev/null 2>&1 && lift=(prlimit --fsize=unlimited --data=unlimited --)
        exec sudo -n --preserve-env="$(g5_worker_host_root_env_list)" ${lift[@]+"${lift[@]}"} "$@"
    else
        exec "$@"
    fi
}
# Liveness without a signal: a root host cannot be probed with kill -0 by its
# unprivileged launcher, and ps answers for any owner on Linux and macOS.
g5_worker_host_alive() {
    ps -o pid= -p "$1" >/dev/null 2>&1
}
# Signals and inspection follow the target's owner, not the requested mode: a
# developer who flips a running root microvm host to process mode must still
# be able to stop and reap it (Codex review). A process the caller owns, or a
# caller who is root, needs no sudo; a pid nobody owns (already gone) is
# signalled directly so the usual "no such process" failure stays visible.
g5_worker_host_pid_owned_by_other() {
    local owner
    owner="$(ps -o uid= -p "${1#-}" 2>/dev/null | tr -d ' ')"
    [ -n "$owner" ] && [ "$owner" != "$(id -u)" ]
}
g5_worker_host_kill() { # <-SIGNAL> <pid | -pgid>
    if [ "$(uname -s)" = Linux ] && [ "$(id -u)" != 0 ] && g5_worker_host_pid_owned_by_other "$2"; then
        sudo -n kill "$1" -- "$2"
    else
        kill "$1" -- "$2"
    fi
}
# Run a command with the privilege the target process's owner requires.
g5_worker_host_as_owner_of() { # <pid> <command…>
    local pid="$1"; shift
    if [ "$(uname -s)" = Linux ] && [ "$(id -u)" != 0 ] && g5_worker_host_pid_owned_by_other "$pid"; then
        sudo -n "$@"
    else
        "$@"
    fi
}
# Does the process itself hold a file or its working directory under DIR?
# lsof where installed (macOS, developer Linux); otherwise /proc on Linux (the
# g5dev image ships no lsof), through the target owner's privilege. Only
# process-specific references count: a mount under DIR (the preview's XFS
# host root) is visible to every process in the namespace, so mountinfo would
# make the enclosing preview and dispatcher look like strays (Codex review).
# The listing is captured before matching: the recipes run under pipefail,
# where an early-exiting `grep -q` would turn a match into a SIGPIPE failure.
g5_worker_host_pid_holds_dir() { # <pid> <dir>
    local pid="$1" dir="$2" listing="" line
    if command -v lsof >/dev/null 2>&1; then
        listing="$(g5_worker_host_as_owner_of "$pid" lsof -Fn -p "$pid" 2>/dev/null || true)"
        case "$listing" in *"$dir/"*) return 0 ;; esac
        return 1
    elif [ -d "/proc/$pid" ]; then
        listing="$(g5_worker_host_as_owner_of "$pid" sh -c 'readlink "/proc/$1/cwd" "/proc/$1/fd"/* 2>/dev/null' _ "$pid" || true)"
        while IFS= read -r line; do
            case "$line" in "$dir"|"$dir"/*) return 0 ;; esac
        done <<< "$listing"
        return 1
    else
        return 1
    fi
}
# The pid a launch leaves behind: under sudo the recorded `$!` is sudo's
# supervisor, which forks the host (PAM sessions), so SIGKILL to it would
# orphan the host. Resolve the child named g5d, waiting briefly for sudo to
# fork it; without sudo the launch pid is the host.
g5_worker_host_resolve_launched_pid() { # <launch pid>
    local launch="$1" child="" i
    if ! g5_worker_host_needs_root; then
        printf '%s' "$launch"
        return 0
    fi
    for i in $(seq 1 100); do
        child="$(pgrep -P "$launch" -x g5d 2>/dev/null | head -1 || true)"
        [ -n "$child" ] && break
        g5_worker_host_alive "$launch" || break
        sleep 0.1
    done
    printf '%s' "${child:-$launch}"
}

# A microvm host clones workload templates and workspace caches with reflinks
# and refuses admission without them (workload_template::prepare). Inside a
# workspace VM the checkout's ext4 disk has none: the full-stack preview
# mounts an XFS image with scripts/dev-host-reflink-root.sh and points
# G5_DEV_HOST_DIR at it. Prove the property here, before a host is built or
# launched, with the same clone the host will make.
g5_worker_host_check_reflink_root() {
    [ "$G5D_ISOLATION_MODE" = microvm ] && [ "$(uname -s)" = Linux ] || return 0
    local probe
    mkdir -p "$G5_DEV_HOST_DIR" || { echo "✗ cannot create $G5_DEV_HOST_DIR" >&2; return 1; }
    probe="$(mktemp "$G5_DEV_HOST_DIR/.reflink-probe.XXXXXX")" || return 1
    printf probe > "$probe"
    if cp --reflink=always "$probe" "$probe.clone" 2>/dev/null; then
        rm -f "$probe" "$probe.clone"
        return 0
    fi
    rm -f "$probe" "$probe.clone"
    echo "✗ $G5_DEV_HOST_DIR has no reflink support; a microvm host refuses admission there." >&2
    echo "  Mount a reflink root with scripts/dev-host-reflink-root.sh and export G5_DEV_HOST_DIR to it," >&2
    echo "  or use G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 (full-stack-process profile)." >&2
    return 1
}

# A Linux microvm host ran as root and left root-owned state (journal, repo
# cache, workspace roots, OCI cache) under the shared host directory. A later
# process-mode host runs unprivileged in the same directory and would fail
# deep inside the host with permission errors (Codex review). Refuse before
# the launch and say what to do; process mode itself never uses sudo. The
# staged runtimes are skipped: the Docker-staged guest runtime is the
# caller's and the process runtime is rebuilt every start. Ownership is
# compared numerically, so the check needs no name lookup.
g5_worker_host_check_state_ownership() {
    [ "$G5D_ISOLATION_MODE" = process ] && [ "$(uname -s)" = Linux ] && [ "$(id -u)" != 0 ] || return 0
    [ -d "$G5_DEV_HOST_DIR" ] || return 0
    local foreign
    foreign="$(find "$G5_DEV_HOST_DIR" \( -path "$G5_DEV_HOST_DIR/runtime" -o -path "$G5_DEV_HOST_DIR/runtime-process" \) -prune \
        -o ! -uid "$(id -u)" -print -quit 2>/dev/null || true)"
    [ -z "$foreign" ] && return 0
    echo "✗ $G5_DEV_HOST_DIR holds state not owned by $(id -un) (first: $foreign), left by a root microvm host." >&2
    echo "  A process host runs unprivileged there and would fail on it. Run: sudo chown -R $(id -un) $G5_DEV_HOST_DIR" >&2
    echo "  (or remove that directory, or point G5_DEV_HOST_DIR elsewhere), then retry." >&2
    return 1
}

g5_worker_host_check_host_vm_exec() {
    local output=""
    if output="$("$G5_WORKER_HOST_G5D" vm-exec --report-version 2>&1)"; then
        return 0
    fi
    echo "✗ host g5d does not expose a working vm-exec subcommand" >&2
    echo "  $output" >&2
    echo "  (platform: ${G5D_CLOUD_HYPERVISOR_BIN:-/opt/g5-host/bin/cloud-hypervisor}; inside a workspace VM the runtime disk supplies it under ${G5_DEV_HOST_PLATFORM_DIR})" >&2
    echo "  Re-run: cd . && just worker-host-setup${G5_WORKER_ORG:+ ${G5_WORKER_ORG}}" >&2
    return 1
}

# Content hash for the Linux guest runtime inputs that `worker-host` cannot
# refresh through the native host build. This intentionally covers the guest g5d
# Rust inputs, the bundled Claude sidecar source/bundle/package metadata, the
# harness sources, staging script, and runtime pins. It is a launch-time freshness guard,
# not a reproducible build attestation.
g5_worker_host_runtime_source_hash() {
    local -a inputs=(
        "Cargo.lock"
        "Cargo.toml"
        "infra/docker/Dockerfile.g5d"
        "infra/docker/Dockerfile.base"
        "infra/docker/Dockerfile.base-rust"
        "infra/docker/Dockerfile.base-rust-warm"
        "scripts/build-cloud-hypervisor-runtime.sh"
        "scripts/build-runtime-disk.sh"
        "scripts/stage-guest-bootstrap.py"
        "scripts/stage-runtime-platform.sh"
        "infra/guest"
        "scripts/ensure-base-images.sh"
        "scripts/stage-local-worker-runtime.sh"
        "scripts/claude-code-cli-version"
        "scripts/codex-cli-version"
        "infra/packer/g5-worker/scripts/57-install-test-tools.sh"
        "backend/g5d/Cargo.toml"
        "backend/g5d/src"
        "backend/g5d-foundation/Cargo.toml"
        "backend/g5d-foundation/src"
        "backend/g5d-isolation/Cargo.toml"
        "backend/g5d-isolation/src"
        "backend/g5d-agent/Cargo.toml"
        "backend/g5d-agent/src"
        "backend/g5d-host/Cargo.toml"
        "backend/g5d-host/src"
        "backend/g5d-dispatch/Cargo.toml"
        "backend/g5d-dispatch/src"
        "backend/g5-contracts/Cargo.toml"
        "backend/g5-contracts/src"
        "backend/g5-auth/Cargo.toml"
        "backend/g5-auth/src"
        "backend/g5-storage/Cargo.toml"
        "backend/g5-storage/src"
        "backend/g5-semantic/Cargo.toml"
        "backend/g5-semantic/src"
        "backend/g5-scm/Cargo.toml"
        "backend/g5-scm/src"
        "backend/g5-internal-api/Cargo.toml"
        "backend/g5-internal-api/src"
        "backend/g5-authz/Cargo.toml"
        "backend/g5-authz/src"
        ".agents/AGENTS.md"
        ".agents/skills"
        ".agents/guidance"
    )
    local -a existing=()
    local path
    for path in "${inputs[@]}"; do
        if [ -e "${GEN_ROOT}/${path}" ]; then
            existing+=("$path")
        fi
    done
    (
        cd "$GEN_ROOT"
        printf 'CLAUDE_CODE_VERSION=%s\n' "${CLAUDE_CODE_VERSION:-}"
        printf 'CODEX_VERSION=%s\n' "${CODEX_VERSION:-}"
        printf 'GH_CLI_VERSION=%s\n' "${GH_CLI_VERSION:-}"
        printf 'GH_CLI_SHA256=%s\n' "${GH_CLI_SHA256:-}"
        printf 'GCLOUD_CLI_VERSION=%s\n' "${GCLOUD_CLI_VERSION:-}"
        printf 'GCLOUD_CLI_SHA256=%s\n' "${GCLOUD_CLI_SHA256:-}"
        printf 'KUBECTL_VERSION=%s\n' "${KUBECTL_VERSION:-}"
        printf 'KUBECTL_SHA256=%s\n' "${KUBECTL_SHA256:-}"
        printf 'NODE_VERSION=%s\n' "${NODE_VERSION:-22.23.4}"
        printf 'NODE_SHA256=%s\n' "${NODE_SHA256:-}"
        printf 'PNPM_VERSION=%s\n' "${PNPM_VERSION:-11.9.0}"
        printf 'RUST_TOOLCHAIN=%s\n' "${RUST_TOOLCHAIN:-1.97.1}"
        printf 'RUSTUP_VERSION=%s\n' "${RUSTUP_VERSION:-1.29.0}"
        printf 'RUSTUP_INIT_SHA256=%s\n' "${RUSTUP_INIT_SHA256:-}"
        printf 'JUST_VERSION=%s\n' "${JUST_VERSION:-1.55.1}"
        printf 'JUST_SHA256=%s\n' "${JUST_SHA256:-}"
        printf 'CARGO_NEXTEST_VERSION=%s\n' "${CARGO_NEXTEST_VERSION:-0.9.138}"
        printf 'CARGO_NEXTEST_SHA256=%s\n' "${CARGO_NEXTEST_SHA256:-}"
        # The runtime disk carries this machine's platform for inner hosts
        # (stage-runtime-platform.sh); a platform rebuild restages it.
        for platform_file in \
            "${G5D_CLOUD_HYPERVISOR_BIN:-/opt/g5-host/bin/cloud-hypervisor}" \
            "${G5D_GUEST_KERNEL:-/opt/g5-host/boot/vmlinux}" \
            "${G5D_GUEST_INITRAMFS:-/opt/g5-host/boot/initramfs.cpio}" \
            "${G5D_PASST_BIN:-$(command -v passt 2>/dev/null || echo /usr/bin/passt)}"; do
            if [ -f "$platform_file" ]; then
                printf 'platform %s ' "$platform_file"; shasum -a 256 "$platform_file" | awk '{print $1}'
            fi
        done
        if [ "${#existing[@]}" -gt 0 ]; then
            find "${existing[@]}" -type f -print0 \
                | LC_ALL=C sort -z \
                | while IFS= read -r -d '' file; do
                    shasum -a 256 "$file"
                done
        fi
    ) | shasum -a 256 | awk '{print $1}'
}

g5_worker_host_write_runtime_source_hash() {
    local source_stamp="${G5_DEV_HOST_DIR}/runtime/guest-runtime-source.sha256"
    mkdir -p "$(dirname "$source_stamp")"
    g5_worker_host_runtime_source_hash > "$source_stamp"
}

g5_worker_host_launch_fingerprint() {
    (
        cd "$GEN_ROOT"
        # Do not hash the selected-profile g5d itself: on macOS, cargo's no-op build
        # and ad-hoc codesign can toggle Mach-O signature bytes even when source
        # is unchanged. Freshness is source/runtime based, and vm-exec support is
        # proven separately by g5_worker_host_check_host_vm_exec.
        printf 'guest-runtime-source=%s\n' "$(g5_worker_host_runtime_source_hash)"
        printf 'G5D_ISOLATION_MODE=%s\n' "$G5D_ISOLATION_MODE"
        # The control plane the host registers with is part of what it was
        # launched for: a running host serving another API address is stale
        # (review R29).
        printf 'G5D_API_URL=%s\n' "$G5D_API_URL"
        printf 'G5_DEV_HOST_CARGO_PROFILE=%s\n' "$G5_DEV_HOST_CARGO_PROFILE"
        printf 'G5_WORKER_HOST_G5D=%s\n' "$G5_WORKER_HOST_G5D"
        printf 'G5D_HOST_BASE_IMAGE=%s\n' "${G5D_HOST_BASE_IMAGE:-}"
        printf 'CLAUDE_CODE_VERSION=%s\n' "${CLAUDE_CODE_VERSION:-}"
        printf 'CODEX_VERSION=%s\n' "${CODEX_VERSION:-}"
        printf 'GH_CLI_VERSION=%s\n' "${GH_CLI_VERSION:-}"
        printf 'GH_CLI_SHA256=%s\n' "${GH_CLI_SHA256:-}"
        printf 'GCLOUD_CLI_VERSION=%s\n' "${GCLOUD_CLI_VERSION:-}"
        printf 'GCLOUD_CLI_SHA256=%s\n' "${GCLOUD_CLI_SHA256:-}"
        printf 'KUBECTL_VERSION=%s\n' "${KUBECTL_VERSION:-}"
        printf 'KUBECTL_SHA256=%s\n' "${KUBECTL_SHA256:-}"
        printf 'NODE_VERSION=%s\n' "${NODE_VERSION:-22.23.4}"
        printf 'NODE_SHA256=%s\n' "${NODE_SHA256:-}"
        printf 'PNPM_VERSION=%s\n' "${PNPM_VERSION:-11.9.0}"
        printf 'RUSTUP_INIT_SHA256=%s\n' "${RUSTUP_INIT_SHA256:-}"
        printf 'JUST_SHA256=%s\n' "${JUST_SHA256:-}"
        printf 'CARGO_NEXTEST_SHA256=%s\n' "${CARGO_NEXTEST_SHA256:-}"
    ) | shasum -a 256 | awk '{print $1}'
}

g5_worker_host_write_launch_fingerprint() {
    mkdir -p "$(dirname "$G5_WORKER_HOST_LAUNCH_FINGERPRINT_FILE")"
    printf '%s\n' "$1" > "$G5_WORKER_HOST_LAUNCH_FINGERPRINT_FILE"
}

# Re-stage the Linux guest runtime when either the staged Claude Code version has
# drifted from the CLAUDE_CODE_VERSION pin or the staged guest source hash has
# drifted from the checkout — so `just dev-up` redeploys both a version bump and
# guest-side source fixes. The guest `claude` binary, guest `g5d`, and sidecar
# bundle are NOT produced by the native host cargo build; they are staged by
# Docker. This is the guest-runtime half of g5_worker_host_build's host-binary
# guard.
#
# stage-local-worker-runtime.sh is a full Docker build (minutes), so this is
# GATED on recorded stamps and is a no-op when the staged runtime already matches
# the pin + source hash — the overwhelmingly common case, so a normal `dev-up`
# pays nothing. First-time bootstrap still belongs to `worker-host-setup`; a host with no staged
# runtime is left to g5_worker_host_check_artifacts, which points the developer at
# worker-host-setup. This only keeps an already set-up host current.
g5_worker_host_ensure_runtime() {
    # Only act once a runtime is staged; missing assets → check_artifacts.
    [ -e "$G5_RUNTIME_BIN_DIR/claude" ] || return 0
    local stamp="${G5_DEV_HOST_DIR}/runtime/claude-code.version" staged=""
    local source_stamp="${G5_DEV_HOST_DIR}/runtime/guest-runtime-source.sha256"
    local current_source_hash="" staged_source_hash="" staged_node_version="" staged_pnpm_version="" staged_codex_version="" staged_gh_version="" staged_gcloud_version="" staged_gcloud_sha="" staged_kubectl_version="" staged_kubectl_sha="" staged_rustup_version="" staged_cargo_version="" staged_rustc_version="" staged_cargo_nextest_version="" staged_just_version="" codex_npm_bad_dir=""
    [ -f "$stamp" ] && staged="$(cat "$stamp" 2>/dev/null || true)"
    if [ -d "$G5_RUNTIME_BIN_DIR/_npm" ]; then
        codex_npm_bad_dir="$(find "$G5_RUNTIME_BIN_DIR/_npm" -type d ! -perm -005 -print -quit 2>/dev/null || true)"
    fi
    if [ ! -f "${G5_DEV_HOST_DIR}/runtime/runtime.ext4" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/codex" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/gh" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/gcloud" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gcloud" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/gke-gcloud-auth-plugin" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/google-cloud-sdk/bin/gke-gcloud-auth-plugin" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/kubectl" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/npm" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/npx" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/corepack" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/pnpm" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/cargo" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/rustc" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/cargo-nextest" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/just" ] \
        || [ ! -e "$G5_RUNTIME_BIN_DIR/_npm/bin/codex" ] \
        || [ ! -f "$G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/npm/bin/npm-cli.js" ] \
        || [ ! -f "$G5_RUNTIME_BIN_DIR/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" ] \
        || [ ! -d "$G5_RUNTIME_BIN_DIR/rust/rustup/toolchains" ] \
        || [ ! -s "$G5_RUNTIME_BIN_DIR/ca-certificates.crt" ] \
        || [ -n "$codex_npm_bad_dir" ]; then
        echo "→ Guest runtime missing provider, package-manager, test, or agent shell CLI artifacts — restaging full provider runtime…" >&2
        if [ -n "$codex_npm_bad_dir" ]; then
            echo "  Codex npm closure is not non-root traversable: $codex_npm_bad_dir" >&2
        fi
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale/incomplete provider worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    if [ -n "${CODEX_VERSION:-}" ]; then
        [ -f "$G5_RUNTIME_BIN_DIR/codex.version" ] \
            && staged_codex_version="$(awk '{print $NF}' "$G5_RUNTIME_BIN_DIR/codex.version" 2>/dev/null || true)"
        if [ "$staged_codex_version" != "$CODEX_VERSION" ]; then
            echo "→ Codex guest runtime drift (staged='${staged_codex_version:-unknown}', pinned='${CODEX_VERSION}') — restaging…" >&2
            if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
                echo "✗ guest runtime restage failed — refusing to launch a stale-Codex worker" >&2
                return 1
            fi
            g5_worker_host_write_runtime_source_hash
            return 0
        fi
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/gh.version" ] \
        && staged_gh_version="$(awk 'NR == 1 {print $NF}' "$G5_RUNTIME_BIN_DIR/gh.version" 2>/dev/null || true)"
    if [ "$staged_gh_version" != "$GH_CLI_VERSION" ]; then
        echo "→ GitHub CLI guest runtime drift (staged='${staged_gh_version:-unknown}', pinned='${GH_CLI_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-gh worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/gcloud.version" ] \
        && staged_gcloud_version="$(awk 'NR == 1 {print $NF}' "$G5_RUNTIME_BIN_DIR/gcloud.version" 2>/dev/null || true)"
    [ -f "$G5_RUNTIME_BIN_DIR/gcloud.sha256" ] \
        && staged_gcloud_sha="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/gcloud.sha256" 2>/dev/null || true)"
    if [ "$staged_gcloud_version" != "$GCLOUD_CLI_VERSION" ] \
        || { [ -n "${GCLOUD_CLI_SHA256:-}" ] && [ "$staged_gcloud_sha" != "$GCLOUD_CLI_SHA256" ]; }; then
        echo "→ Google Cloud CLI guest runtime drift (staged_version='${staged_gcloud_version:-unknown}', pinned_version='${GCLOUD_CLI_VERSION}', staged_sha='${staged_gcloud_sha:-unknown}', pinned_sha='${GCLOUD_CLI_SHA256:-untracked}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-gcloud worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/kubectl.version" ] \
        && staged_kubectl_version="$(awk 'NR == 1 {print $NF}' "$G5_RUNTIME_BIN_DIR/kubectl.version" 2>/dev/null || true)"
    [ -f "$G5_RUNTIME_BIN_DIR/kubectl.sha256" ] \
        && staged_kubectl_sha="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/kubectl.sha256" 2>/dev/null || true)"
    if [ "$staged_kubectl_version" != "$KUBECTL_VERSION" ] \
        || { [ -n "${KUBECTL_SHA256:-}" ] && [ "$staged_kubectl_sha" != "$KUBECTL_SHA256" ]; }; then
        echo "→ kubectl guest runtime drift (staged_version='${staged_kubectl_version:-unknown}', pinned_version='${KUBECTL_VERSION}', staged_sha='${staged_kubectl_sha:-unknown}', pinned_sha='${KUBECTL_SHA256:-untracked}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-kubectl worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/node.version" ] \
        && staged_node_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/node.version" 2>/dev/null || true)"
    if [ "$staged_node_version" != "$NODE_VERSION" ]; then
        echo "→ Node guest runtime drift (staged='${staged_node_version:-unknown}', pinned='${NODE_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-node worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/pnpm.version" ] \
        && staged_pnpm_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/pnpm.version" 2>/dev/null || true)"
    if [ "$staged_pnpm_version" != "$PNPM_VERSION" ]; then
        echo "→ pnpm guest runtime drift (staged='${staged_pnpm_version:-unknown}', pinned='${PNPM_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-pnpm worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/rustup.version" ] \
        && staged_rustup_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/rustup.version" 2>/dev/null || true)"
    if [ "$staged_rustup_version" != "$RUSTUP_VERSION" ]; then
        echo "→ rustup guest runtime drift (staged='${staged_rustup_version:-unknown}', pinned='${RUSTUP_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-rustup worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    [ -f "$G5_RUNTIME_BIN_DIR/cargo.version" ] \
        && staged_cargo_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/cargo.version" 2>/dev/null || true)"
    [ -f "$G5_RUNTIME_BIN_DIR/rustc.version" ] \
        && staged_rustc_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/rustc.version" 2>/dev/null || true)"
    [ -f "$G5_RUNTIME_BIN_DIR/cargo-nextest.version" ] \
        && staged_cargo_nextest_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/cargo-nextest.version" 2>/dev/null || true)"
    [ -f "$G5_RUNTIME_BIN_DIR/just.version" ] \
        && staged_just_version="$(awk 'NR == 1 {print $1}' "$G5_RUNTIME_BIN_DIR/just.version" 2>/dev/null || true)"
    if [ "$staged_cargo_version" != "$RUST_TOOLCHAIN" ] \
        || [ "$staged_rustc_version" != "$RUST_TOOLCHAIN" ] \
        || [ "$staged_cargo_nextest_version" != "$CARGO_NEXTEST_VERSION" ] \
        || [ "$staged_just_version" != "$JUST_VERSION" ]; then
        echo "→ Rust test-tool guest runtime drift (cargo='${staged_cargo_version:-unknown}', rustc='${staged_rustc_version:-unknown}', nextest='${staged_cargo_nextest_version:-unknown}', just='${staged_just_version:-unknown}'; pinned Rust='${RUST_TOOLCHAIN}', nextest='${CARGO_NEXTEST_VERSION}', just='${JUST_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-test-tool worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi
    if [ "$staged" != "$CLAUDE_CODE_VERSION" ]; then
        echo "→ Claude Code guest runtime drift (staged='${staged:-unknown}', pinned='${CLAUDE_CODE_VERSION}') — restaging…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch a stale-Claude worker" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
        return 0
    fi

    current_source_hash="$(g5_worker_host_runtime_source_hash)"
    [ -f "$source_stamp" ] && staged_source_hash="$(cat "$source_stamp" 2>/dev/null || true)"
    if [ "$staged_source_hash" != "$current_source_hash" ]; then
        echo "→ Guest runtime source drift (staged='${staged_source_hash:-unknown}', current='${current_source_hash}') — restaging full guest runtime…" >&2
        if ! "${GEN_ROOT}/scripts/stage-local-worker-runtime.sh"; then
            echo "✗ guest runtime restage failed — refusing to launch stale guest code" >&2
            return 1
        fi
        g5_worker_host_write_runtime_source_hash
    fi
}

# Defensive parse of seed-dev-worker-token.sh output: accept only the expected
# expected `export` lines (never eval arbitrary script output).
g5_worker_host_seed_token() {
    local api_url="${1:-auto}" seed_out
    if ! seed_out="$(umask 077 && "$GEN_ROOT/scripts/seed-dev-worker-token.sh" "$api_url")"; then
        echo "✗ failed to provision local workspace-host credentials via ${api_url}" >&2
        return 1
    fi
    G5D_ORG_ID="$(printf '%s\n' "$seed_out" | sed -n 's/^export G5D_ORG_ID=//p' | head -1)"
    G5D_HOST_ID="$(printf '%s\n' "$seed_out" | sed -n 's/^export G5D_HOST_ID=//p' | head -1)"
    G5_HOST_JWT="$(printf '%s\n' "$seed_out" | sed -n 's/^export G5_HOST_JWT=//p' | head -1)"
    G5D_SCRATCH_DISK_MIB="$(printf '%s\n' "$seed_out" | sed -n 's/^export G5D_SCRATCH_DISK_MIB=//p' | head -1)"
    G5D_HOST_MAX_MICROVMS="$(printf '%s\n' "$seed_out" | sed -n 's/^export G5D_HOST_MAX_MICROVMS=//p' | head -1)"
    if [ -z "$G5D_ORG_ID" ] || [ -z "$G5D_HOST_ID" ] || [ -z "$G5_HOST_JWT" ] \
        || ! [[ "$G5D_SCRATCH_DISK_MIB" =~ ^[1-9][0-9]*$ && "$G5D_HOST_MAX_MICROVMS" =~ ^[1-9][0-9]*$ ]]; then
        echo "✗ seed-dev-worker-token.sh did not yield bound host credentials" >&2
        return 1
    fi
    export G5D_ORG_ID G5D_HOST_ID G5_HOST_JWT G5D_SCRATCH_DISK_MIB G5D_HOST_MAX_MICROVMS
}
