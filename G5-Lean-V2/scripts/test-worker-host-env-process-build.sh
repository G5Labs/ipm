#!/usr/bin/env bash
# Regression for the local worker-host build helper's mode-specific features
# (ontology g5.runtime.boot.backend.process-local, review R1): process mode
# builds the host g5d without the Cloud Hypervisor feature so a machine without Cloud Hypervisor
# can run it; the default microvm build keeps Cloud Hypervisor linked; and process mode
# without the acknowledgement is refused when the helper is sourced.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { echo "✗ $*" >&2; exit 1; }

# Source the real helper in a subshell with a recording `cargo` function, so
# the build arguments are observed without compiling anything.
record_build() { # <mode> <ack-or-empty> <out-file> [fake-provider-flag-value] [jobs-override]
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-$1"
        export G5D_ISOLATION_MODE="$1"
        if [ -n "$2" ]; then export G5D_ALLOW_PROCESS_ISOLATION="$2"; else unset G5D_ALLOW_PROCESS_ISOLATION; fi
        if [ -n "${4:-}" ]; then export G5_CLAUDE_CODE_FAKE="$4"; else unset G5_CLAUDE_CODE_FAKE; fi
        if [ -n "${5:-}" ]; then export G5_DEV_HOST_CARGO_JOBS="$5"; else unset G5_DEV_HOST_CARGO_JOBS; fi
        unset G5_CODEX_FAKE PKG_CONFIG_PATH
        # A known core count, so the job count is asserted exactly.
        nproc() { echo 7; }
        uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
        cargo() { printf '%s\n' "$*" > "$3"; }
        out="$3"
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh"
        cargo() { printf '%s env_jobs=%s\n' "$*" "${CARGO_BUILD_JOBS:-unset}" > "$out"; }
        g5_worker_host_build >/dev/null 2>&1
    )
}

record_build microvm "" "$TMP/microvm.args"
grep -q -- '-p g5d' "$TMP/microvm.args" || fail "microvm build must build the g5d package: $(cat "$TMP/microvm.args")"
grep -q -- '--no-default-features' "$TMP/microvm.args" && fail "microvm build must keep the default Cloud Hypervisor feature: $(cat "$TMP/microvm.args")"
grep -q -- '-p fake-provider-cli' "$TMP/microvm.args" && fail "microvm build takes the fake CLIs from the guest bundle, not a host build: $(cat "$TMP/microvm.args")"
echo "✓ microvm build keeps Cloud Hypervisor linked"

# Cargo and every launcher must address the same private output directory,
# including paths containing spaces and callers using Cargo's standard knob.
for selection in dedicated cargo relative; do
    (
        unset G5_DEV_HOST_CARGO_TARGET_DIR CARGO_TARGET_DIR
        case "$selection" in
            dedicated) export G5_DEV_HOST_CARGO_TARGET_DIR="$TMP/preview build"; expected="$G5_DEV_HOST_CARGO_TARGET_DIR" ;;
            cargo) export CARGO_TARGET_DIR="$TMP/cargo build"; expected="$CARGO_TARGET_DIR" ;;
            relative) export G5_DEV_HOST_CARGO_TARGET_DIR=.dev/preview-build; expected="$ROOT/.dev/preview-build" ;;
        esac
        export G5_DEV_HOST_DIR="$TMP/private-host" G5D_ISOLATION_MODE=microvm
        # The microvm build requires Linux; stub it so the case runs on macOS too.
        uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
        source "$ROOT/scripts/worker-host-env.sh"
        [ "$G5_WORKER_HOST_G5D" = "$expected/$G5_DEV_HOST_CARGO_PROFILE/g5d" ] || fail "host launcher does not use the selected Cargo output"
        cargo() { printf '%s\n' "$@" > "$TMP/private.args"; }
        g5_worker_host_build >/dev/null 2>&1
        grep -Fxq -- --target-dir "$TMP/private.args" || fail "build did not select the host output directory"
        grep -Fxq -- "$expected" "$TMP/private.args" || fail "build split or changed the selected output path"
        first=$(g5_worker_host_launch_fingerprint)
        G5_WORKER_HOST_G5D="$TMP/another-build/g5d"
        [ "$first" != "$(g5_worker_host_launch_fingerprint)" ] || fail "the launch fingerprint must track the executable path"
    )
done
echo "✓ host builds and launches share an isolated Cargo output, which affects launch freshness"

record_build process 1 "$TMP/process.args"
grep -q -- '-p g5d' "$TMP/process.args" || fail "process build must build the g5d package: $(cat "$TMP/process.args")"
grep -q -- '--no-default-features' "$TMP/process.args" || fail "process build must not require Cloud Hypervisor: $(cat "$TMP/process.args")"
grep -q -- '-p fake-provider-cli' "$TMP/process.args" && fail "a developer's process build must not build the fake provider CLIs: $(cat "$TMP/process.args")"
# The host build links one binary, so it uses every core instead of the
# workspace's jobs=2 cap (a QA preview VM held two of four vCPUs for ten minutes).
# The count travels as --jobs, never as CARGO_BUILD_JOBS: an exported CARGO_*
# key would enter the guest's sccache hashes and miss the image's warm cache.
grep -q -- '--jobs 7 ' "$TMP/process.args" || fail "the host build must pass the core count as --jobs: $(cat "$TMP/process.args")"
grep -q -- '--jobs 7 ' "$TMP/microvm.args" || fail "the microvm host build must pass the core count as --jobs too: $(cat "$TMP/microvm.args")"
grep -q 'env_jobs=unset$' "$TMP/process.args" || fail "the host build must not export CARGO_BUILD_JOBS: $(cat "$TMP/process.args")"
record_build process 1 "$TMP/process-jobs.args" "" 3
grep -q -- '--jobs 3 ' "$TMP/process-jobs.args" || fail "G5_DEV_HOST_CARGO_JOBS must override the core count: $(cat "$TMP/process-jobs.args")"
for bad in 0 abc -2 ""; do
    record_build process 1 "$TMP/process-badjobs.args" "" "$bad"
    grep -q -- '--jobs 7 ' "$TMP/process-badjobs.args" || fail "an invalid G5_DEV_HOST_CARGO_JOBS ('$bad') must fall back to the core count: $(cat "$TMP/process-badjobs.args")"
done
echo "✓ process build drops the Cloud Hypervisor feature, leaves the fake providers out and uses every core"

# A deterministic provider lane resolves fake-claude beside the runtime's g5d
# (as in the guest bundle), so only then are the fake CLIs built and staged.
record_build process 1 "$TMP/process-fake.args" 1
grep -q -- '-p fake-provider-cli' "$TMP/process-fake.args" || fail "a deterministic lane's process build must build the fake provider CLIs: $(cat "$TMP/process-fake.args")"
echo "✓ a deterministic provider lane builds the fake provider CLIs"

if record_build process "" "$TMP/unacked.args" 2>/dev/null; then
    fail "process mode without G5D_ALLOW_PROCESS_ISOLATION must be refused when the helper is sourced"
fi
[ ! -e "$TMP/unacked.args" ] || fail "an unacknowledged process mode must not reach the build"
echo "✓ process mode without the acknowledgement is refused"

if record_build porcess 1 "$TMP/typo.args" 2>/dev/null; then
    fail "an unrecognized mode must be refused, not ignored"
fi
echo "✓ an unrecognized mode is refused"

# Review R29: the API address the host registers with is part of its launch
# fingerprint, so a running host serving another control plane is stale, and
# the address follows the selected stack's API port.
fingerprint_for() { # <api-url-override-or-empty> <api-port>
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-fp"
        export G5D_ISOLATION_MODE=microvm
        export G5_PORT_API="$2"
        if [ -n "$1" ]; then export G5_WORKER_HOST_API_URL="$1"; else unset G5_WORKER_HOST_API_URL; fi
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh"
        printf '%s %s\n' "$G5D_API_URL" "$(g5_worker_host_launch_fingerprint)"
    )
}
read -r url_a fp_a < <(fingerprint_for "" 7500)
read -r url_b fp_b < <(fingerprint_for "" 17500)
read -r url_c fp_c < <(fingerprint_for "http://localhost:27500" 17500)
[ "$url_a" = "http://localhost:7500" ] && [ "$url_b" = "http://localhost:17500" ] || fail "the API address must follow the selected API port: $url_a / $url_b"
[ "$url_c" = "http://localhost:27500" ] || fail "the dedicated override must still win: $url_c"
[ "$fp_a" != "$fp_b" ] || fail "fingerprints must differ when only the API address changes"
[ "$fp_b" != "$fp_c" ] || fail "fingerprints must differ for the override address"
[ "$fp_a" = "$(fingerprint_for "" 7500 | cut -d' ' -f2)" ] || fail "the fingerprint must be stable for an unchanged address"
echo "✓ the launch fingerprint follows the selected API address"

# A preview can itself run inside a G5 VM, whose Codex entrypoint is a shell
# wrapper with a sibling npm closure. Relocating it with a symlink breaks $0
# lookup before `login status` can run. Exercise the staged entrypoints with
# that layout, including shell metacharacters in the installed path.
(
    export G5_DEV_HOST_DIR="$TMP/process launchers"
    export G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1
    unset G5_WORKER_ORG G5_CLAUDE_CODE_FAKE G5_CODEX_FAKE
    source "$ROOT/scripts/worker-host-env.sh"
    tools="$TMP/tools with 'quotes' and \$dollars"
    mkdir -p "$tools/_npm/bin" "$G5_RUNTIME_BIN_DIR"
    cat > "$tools/codex" <<'SH'
#!/bin/sh
HERE="$(cd "$(dirname "$0")" && pwd)"
exec "$HERE/_npm/bin/codex" "$@"
SH
    cat > "$tools/_npm/bin/codex" <<'SH'
#!/bin/sh
if [ "$1" = --version ]; then
    printf 'codex-cli %s\n' "$PROCESS_TEST_CODEX_VERSION"
    exit 0
fi
printf '%s\n' "$HOME" "$PWD" "$#" "$@"
read -r input
printf '%s\n' "$input" >&2
exit 23
SH
    for tool in claude node git docker; do
        printf '#!/bin/sh\nif [ "$1" = --version ]; then printf "%%s (Claude Code)\\n" "$PROCESS_TEST_CLAUDE_VERSION"; exit 0; fi\nprintf "host tool: %%s\\n" "$0"\n' > "$tools/$tool"
    done
    export PROCESS_TEST_CODEX_VERSION="$(cat "$ROOT/scripts/codex-cli-version")"
    export PROCESS_TEST_CLAUDE_VERSION="$(cat "$ROOT/scripts/claude-code-cli-version")"
    chmod +x "$tools/"* "$tools/_npm/bin/codex"
    export PATH="$tools:$PATH"
    G5_WORKER_HOST_G5D="$tools/node"
    # Simulate the previous staging layout; upgrading must never truncate the
    # installed tool through this symlink. Re-staging must also be idempotent.
    ln -s "$tools/codex" "$G5_RUNTIME_BIN_DIR/codex"
    cp "$tools/codex" "$TMP/codex-before"
    for tool in codex claude; do
        if [ "$tool" = codex ]; then
            export PROCESS_TEST_CODEX_VERSION=0.159.3
        else
            export PROCESS_TEST_CLAUDE_VERSION=2.1.285
        fi
        if g5_worker_host_stage_process_runtime > "$TMP/pin.out" 2> "$TMP/pin.err"; then
            fail "process staging accepted an unpinned $tool"
        fi
        grep -q "process mode needs pinned $tool" "$TMP/pin.err" || fail "pin mismatch must identify $tool"
        [ ! -e "$G5_DEV_HOST_DIR/runtime-process/modules/catalog.json" ] || fail "unpinned staging published the catalog"
        export PROCESS_TEST_CODEX_VERSION="$(cat "$ROOT/scripts/codex-cli-version")"
        export PROCESS_TEST_CLAUDE_VERSION="$(cat "$ROOT/scripts/claude-code-cli-version")"
    done
    for pass in 1 2; do
        g5_worker_host_stage_process_runtime
        for document in ontology.md ontology-format.md ontology-examples.md uplift.md downlift.md \
            TLC/uplift.md TLC/downlift.md TLC/pr_incremental_TLA_reverification.md \
            Verus/uplift.md Verus/downlift.md Verus/new-feature-pr.md Verus/incremental-reverification.md; do
            cmp "$ROOT/.agents/guidance/$document" "$G5_DEV_HOST_DIR/runtime-process/assets/harness/.agents/guidance/$document" || fail "process staging lost guidance: $document"
        done
        status=0
        HOME="$TMP/owner home" "$G5_RUNTIME_BIN_DIR/codex" login 'argument with spaces' '' \
            > "$TMP/launcher.out" 2> "$TMP/launcher.err" <<< 'stdin intact' || status=$?
        [ "$status" = 23 ] || fail "staged Codex did not preserve its closure or exit status (pass $pass): $status"
        printf '%s\n' "$TMP/owner home" "$PWD" 3 login 'argument with spaces' '' > "$TMP/launcher.expected"
        cmp "$TMP/launcher.expected" "$TMP/launcher.out" || fail "staged Codex changed environment, cwd or argv"
        [ "$(cat "$TMP/launcher.err")" = 'stdin intact' ] || fail "staged Codex changed stdin/stderr"
        cmp "$TMP/codex-before" "$tools/codex" || fail "staging overwrote the installed Codex wrapper"
        for tool in claude node git docker; do
            [ "$("$G5_RUNTIME_BIN_DIR/$tool")" = "host tool: $tools/$tool" ] || fail "staged $tool did not invoke its installed path"
        done
    done
)
echo "✓ process launchers preserve installed tool paths, argv, environment, stdio and exit status"
echo "✓ process staging refuses provider pin mismatches before publishing the catalog"

# Inner host inside a workspace VM (ontology g5.architecture.microvm_backend.nested,
# design nested-virt-followup §3.3): with the runtime disk's platform directory
# present, a Linux microvm host resolves the four launch artifacts there when
# the caller set none; explicit values, process mode and macOS are untouched.
source_env() { # <mode> <platform dir> <os> [extra exports…] → prints the four variables
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-discovery" G5D_ISOLATION_MODE="$1" G5_DEV_HOST_PLATFORM_DIR="$2"
        [ "$1" = process ] && export G5D_ALLOW_PROCESS_ISOLATION=1
        os="$3"; shift 3
        for assignment in "$@"; do export "$assignment"; done
        uname() { if [ "$1" = -s ]; then echo "$os"; else command uname "$@"; fi; }
        unset G5D_CLOUD_HYPERVISOR_BIN_SET
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
        printf '%s|%s|%s|%s\n' "${G5D_CLOUD_HYPERVISOR_BIN:-unset}" "${G5D_GUEST_KERNEL:-unset}" "${G5D_GUEST_INITRAMFS:-unset}" "${G5D_PASST_BIN:-unset}"
    )
}
PLATFORM="$TMP/platform"
mkdir -p "$PLATFORM"
for f in cloud-hypervisor vmlinux initramfs.cpio kernel.config passt; do printf 'x' > "$PLATFORM/$f"; done
chmod 0755 "$PLATFORM/cloud-hypervisor" "$PLATFORM/passt"
( unset G5D_CLOUD_HYPERVISOR_BIN G5D_GUEST_KERNEL G5D_GUEST_INITRAMFS G5D_PASST_BIN
  [ "$(source_env microvm "$PLATFORM" Linux)" = "$PLATFORM/cloud-hypervisor|$PLATFORM/vmlinux|$PLATFORM/initramfs.cpio|$PLATFORM/passt" ] \
      || fail "a Linux microvm host must discover the runtime disk's platform: $(source_env microvm "$PLATFORM" Linux)"
  [ "$(source_env microvm "$PLATFORM" Linux G5D_GUEST_KERNEL=/elsewhere/vmlinux)" = "$PLATFORM/cloud-hypervisor|/elsewhere/vmlinux|$PLATFORM/initramfs.cpio|$PLATFORM/passt" ] \
      || fail "an explicit artifact path must win over discovery"
  [ "$(source_env microvm "$TMP/no-platform" Linux)" = "unset|unset|unset|unset" ] \
      || fail "without a platform directory the Rust defaults must stay in force"
  [ "$(source_env process "$PLATFORM" Linux)" = "unset|unset|unset|unset" ] \
      || fail "process mode must not resolve VMM artifacts"
  [ "$(source_env microvm "$PLATFORM" Darwin)" = "unset|unset|unset|unset" ] \
      || fail "macOS must not resolve VMM artifacts"
)
echo "✓ a Linux microvm host discovers the runtime disk's platform; explicit values, process mode and macOS are untouched"

# Root boundary (design §3.5, owner decision 3): only a Linux microvm host run
# by a non-root caller is wrapped in sudo, with PATH, HOME and the G5 keys
# preserved by name; process mode, macOS and root never are. Liveness never
# signals. Observed through a recording `sudo` on PATH.
root_probe() { # <mode> <os> <uid> → the recorded sudo argv or "direct"
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-root" G5D_ISOLATION_MODE="$1" G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
        [ "$1" = process ] && export G5D_ALLOW_PROCESS_ISOLATION=1
        os="$2"; uid="$3"
        uname() { if [ "$1" = -s ]; then echo "$os"; else command uname "$@"; fi; }
        id() { if [ "$1" = -u ]; then echo "$uid"; else command id "$@"; fi; }
        sudo() { printf 'sudo %s\n' "$*"; }
        export G5D_PROBE_KEY=1 G5_PROBE_KEY=1 RUST_LOG=g5d=info
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
        out="$(g5_worker_host_as_root echo direct)"
        printf '%s' "$out"
    )
}
out="$(root_probe microvm Linux 1000)"
case "$out" in "sudo -n --preserve-env="*" echo direct") ;; *) fail "a Linux microvm host run by uid 1000 must go through sudo -n: $out" ;; esac
case "$out" in *"--preserve-env=PATH,HOME,"*) ;; *) fail "sudo must preserve PATH and HOME by name: $out" ;; esac
case "$out" in *G5D_PROBE_KEY*G5_PROBE_KEY*|*G5_PROBE_KEY*G5D_PROBE_KEY*) ;; *) fail "sudo must preserve the exported G5D_*/G5_* keys: $out" ;; esac
case "$out" in *RUST_LOG*) ;; *) fail "sudo must preserve RUST_LOG for the readiness gate: $out" ;; esac
[ "$(root_probe microvm Linux 0)" = direct ] || fail "root must run the host directly"
# The host launch lifts the launcher's inherited file-size and data caps (an
# agent harness's envelope) through prlimit, in place, under the same sudo.
exec_probe() { # <uid> → what the exec'd launch argv looks like
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-exec" G5D_ISOLATION_MODE=microvm G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
        uid="$1"
        uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
        id() { if [ "$1" = -u ]; then echo "$uid"; else command id "$@"; fi; }
        mkdir -p "$TMP/exec-bin"
        printf '#!/bin/sh\nprintf "sudo %%s\\n" "$*"\n' > "$TMP/exec-bin/sudo"
        printf '#!/bin/sh\necho prlimit-stub-must-not-run >&2; exit 97\n' > "$TMP/exec-bin/prlimit"
        chmod 0755 "$TMP/exec-bin/sudo" "$TMP/exec-bin/prlimit"
        export PATH="$TMP/exec-bin:$PATH"
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
        g5_worker_host_exec_as_root echo host-argv
    )
}
out="$(exec_probe 1000)"
case "$out" in "sudo -n --preserve-env="*" prlimit --fsize=unlimited --data=unlimited -- echo host-argv") ;; *) fail "a sudo launch must lift the file-size and data caps through prlimit in place: $out" ;; esac
[ "$(exec_probe 0)" = host-argv ] || fail "root must exec the host directly, without prlimit"
[ "$(root_probe process Linux 1000)" = direct ] || fail "process mode must never use sudo"
[ "$(root_probe microvm Darwin 1000)" = direct ] || fail "macOS must never use sudo"
( set -euo pipefail; export G5_DEV_HOST_DIR="$TMP/host-alive" G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
  source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
  sleep 5 & pid=$!
  g5_worker_host_alive "$pid" || fail "a live pid must read as alive"
  kill "$pid"; wait "$pid" 2>/dev/null || true
  g5_worker_host_alive "$pid" && fail "a dead pid must not read as alive"
  true )
# Signals and lsof follow the target's owner, not the requested mode: a root
# host left by a microvm run is still stoppable after flipping to process mode,
# and the caller's own processes are never signalled through sudo.
kill_probe() { # <mode> <owner uid of the target> → the recorded argv
    (
        set -euo pipefail
        export G5_DEV_HOST_DIR="$TMP/host-kill" G5D_ISOLATION_MODE="$1" G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
        [ "$1" = process ] && export G5D_ALLOW_PROCESS_ISOLATION=1
        # A name the helper's own locals cannot shadow.
        probe_target_owner="$2"
        uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
        id() { if [ "$1" = -u ]; then echo 1000; else command id "$@"; fi; }
        ps() { if [ "$1" = "-o" ] && [ "$2" = "uid=" ]; then echo " $probe_target_owner"; else command ps "$@"; fi; }
        sudo() { printf 'sudo %s\n' "$*"; }
        kill() { printf 'kill %s\n' "$*"; }
        lsof() { printf 'lsof %s\n' "$*"; }
        # shellcheck source=scripts/worker-host-env.sh
        source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
        g5_worker_host_kill -TERM 4242; g5_worker_host_kill -KILL -4242; g5_worker_host_as_owner_of 4242 lsof -p 4242
    )
}
[ "$(kill_probe process 0)" = $'sudo -n kill -TERM -- 4242\nsudo -n kill -KILL -- -4242\nsudo -n lsof -p 4242' ] \
    || fail "a root-owned host must be signalled through sudo even when process mode is now selected: $(kill_probe process 0)"
[ "$(kill_probe microvm 1000)" = $'kill -TERM -- 4242\nkill -KILL -- -4242\nlsof -p 4242' ] \
    || fail "the caller's own processes must never be signalled through sudo: $(kill_probe microvm 1000)"
echo "✓ only a Linux microvm host run by a non-root caller crosses the root boundary; signals follow the target's owner; liveness never signals"

# Reflink root check (design §3.5, owner decision 1): a microvm host directory
# without reflink support is refused before the host is built, with the mount
# script and the process-mode profile named; process mode and macOS skip it.
( set -euo pipefail; export G5_DEV_HOST_DIR="$TMP/host-reflink" G5D_ISOLATION_MODE=microvm G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
  uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
  source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
  probe="$(mktemp "$TMP/.probe.XXXXXX")"
  if cp --reflink=always "$probe" "$probe.clone" 2>/dev/null; then
      g5_worker_host_check_reflink_root || fail "a reflink-capable directory must pass"
      echo "· this filesystem reflinks; the refusal path is covered where it does not"
  else
      if g5_worker_host_check_reflink_root 2> "$TMP/reflink.err"; then fail "a directory without reflinks must be refused"; fi
      grep -q 'dev-host-reflink-root.sh' "$TMP/reflink.err" || fail "the refusal must name the mount script: $(cat "$TMP/reflink.err")"
      grep -q 'full-stack-process' "$TMP/reflink.err" || fail "the refusal must name the process-mode profile: $(cat "$TMP/reflink.err")"
  fi
  rm -f "$probe" "$probe.clone"
  export G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1
  g5_worker_host_check_reflink_root || fail "process mode must not require reflinks" )
echo "✓ a microvm host directory without reflinks is refused with the mount script named"

# Root-owned state from a microvm host blocks a later unprivileged process host
# before the launch, with the repair named; the staged runtimes are not
# inspected, and microvm mode and macOS skip the check. Ownership is compared
# by uid, so a shimmed `id -u` makes the caller's own files look foreign.
( set -euo pipefail; export G5_DEV_HOST_DIR="$TMP/host-owned" G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
  mkdir -p "$G5_DEV_HOST_DIR/state/host-control" "$G5_DEV_HOST_DIR/runtime/bin"
  printf 'journal' > "$G5_DEV_HOST_DIR/state/host-control/journal.json"
  uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
  source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
  g5_worker_host_check_state_ownership || fail "state the caller owns must pass"
  id() { case "$1" in -un) echo someone-else ;; -u) echo 4242 ;; *) command id "$@" ;; esac; }
  if g5_worker_host_check_state_ownership 2> "$TMP/owned.err"; then fail "state owned by another user must be refused"; fi
  grep -q "holds state not owned by someone-else (first: $G5_DEV_HOST_DIR" "$TMP/owned.err" || fail "the refusal must name the first foreign path: $(cat "$TMP/owned.err")"
  grep -q 'sudo chown -R someone-else' "$TMP/owned.err" || fail "the refusal must name the repair: $(cat "$TMP/owned.err")"
  unset -f id
  rm -rf "$G5_DEV_HOST_DIR/state"
  g5_worker_host_check_state_ownership || fail "a host directory the caller owns must pass after the repair"
  export G5D_ISOLATION_MODE=microvm
  mkdir -p "$G5_DEV_HOST_DIR/state"
  g5_worker_host_check_state_ownership || fail "a microvm host (root) must not be blocked by ownership" )
echo "✓ root-owned microvm state blocks an unprivileged process host before launch, with the repair named"

# Freshness: every helper the runtime-disk builder invokes is a staged-runtime
# input, so a helper-only change restages the guest runtime on the next
# dev-up instead of reusing the previous staging (Codex review).
for helper in $(grep -oE '"\$repo/scripts/[A-Za-z0-9._-]+"' "$ROOT/scripts/build-runtime-disk.sh" | sed 's#.*/scripts/##; s/"$//'); do
    grep -Fq "\"scripts/$helper\"" "$ROOT/scripts/worker-host-env.sh" || fail "scripts/$helper is invoked by build-runtime-disk.sh but missing from g5_worker_host_runtime_source_hash"
done
echo "✓ every runtime-disk helper is a guest-runtime freshness input"

# The recorded pid is the host, not sudo's supervisor: with the root boundary
# active, the launch pid's child named g5d is resolved (a script named g5d
# stands in: a script's process name is its file name; it runs under a forking
# parent, as sudo forks the host); without the boundary the launch pid is
# returned as is.
( set -euo pipefail; export G5_DEV_HOST_DIR="$TMP/host-pid" G5D_ISOLATION_MODE=microvm G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
  uname() { if [ "$1" = -s ]; then echo Linux; else command uname "$@"; fi; }
  # The suite also runs as root (Docker CI); the boundary is a non-root caller's.
  id() { if [ "$1" = -u ]; then echo 1000; else command id "$@"; fi; }
  source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
  printf '#!/bin/bash\nsleep 30 & wait\n' > "$TMP/g5d"; chmod 0755 "$TMP/g5d"
  bash -c '"$0" & wait' "$TMP/g5d" & launch=$!
  resolved="$(g5_worker_host_resolve_launched_pid "$launch")"
  child="$(pgrep -P "$launch" -x g5d | head -1)"
  kill "$child" 2>/dev/null || true; kill "$launch" 2>/dev/null || true; wait "$launch" 2>/dev/null || true
  [ -n "$child" ] && [ "$resolved" = "$child" ] || fail "the launched host pid must be the g5d child of the launch (launch=$launch child=$child resolved=$resolved)"
  id() { if [ "$1" = -u ]; then echo 0; else command id "$@"; fi; }
  [ "$(g5_worker_host_resolve_launched_pid 4242)" = 4242 ] || fail "without the root boundary the launch pid is the host" )
echo "✓ the recorded host pid is the g5d child of a sudo launch"

# Without lsof (the g5dev image), a process is matched to the host directory
# through /proc: its working directory or an open file under it.
if [ "$(uname -s)" = Linux ]; then
( set -euo pipefail; export G5_DEV_HOST_DIR="$TMP/host-holds" G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 G5_DEV_HOST_PLATFORM_DIR="$TMP/no-platform"
  mkdir -p "$G5_DEV_HOST_DIR/state" "$TMP/elsewhere"
  source "$ROOT/scripts/worker-host-env.sh" >/dev/null 2>&1
  lsof() { echo "lsof must not be consulted in this case" >&2; return 1; }
  command() { if [ "$1" = -v ] && [ "$2" = lsof ]; then return 1; fi; builtin command "$@"; }
  ( cd "$G5_DEV_HOST_DIR/state" && exec sleep 30 ) & inside=$!
  ( cd "$TMP/elsewhere" && exec sleep 30 3>"$G5_DEV_HOST_DIR/state/open-file" ) & holder=$!
  ( cd "$TMP/elsewhere" && exec sleep 30 ) & outside=$!
  sleep 0.3
  g5_worker_host_pid_holds_dir "$inside" "$G5_DEV_HOST_DIR" || fail "a process working under the host dir must match"
  g5_worker_host_pid_holds_dir "$holder" "$G5_DEV_HOST_DIR" || fail "a process holding a file under the host dir must match"
  if g5_worker_host_pid_holds_dir "$outside" "$G5_DEV_HOST_DIR"; then fail "a process outside the host dir must not match"; fi
  if g5_worker_host_pid_holds_dir "$inside" "$TMP/host-hold"; then fail "a prefix of the host dir must not match"; fi
  # A mount every process can see (the preview's XFS host root is one) is not
  # a reference held by the process: an unrelated process must not match a
  # host directory that is a mount point in its namespace.
  if g5_worker_host_pid_holds_dir "$outside" /sys; then fail "a mount visible to every process must not make an unrelated process a stray"; fi
  kill "$inside" "$holder" "$outside" 2>/dev/null || true )
echo "✓ without lsof a process is matched to the host directory through /proc"
fi
echo "worker-host-env process build: OK"
