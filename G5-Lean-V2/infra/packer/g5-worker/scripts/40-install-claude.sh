#!/usr/bin/env bash
# =============================================================================
# 40-install-claude.sh — Install the Claude Code CLI runtime into runtime/bin/.
#
# Distribution (current upstream packaging): @anthropic-ai/claude-code ships a
# SELF-CONTAINED NATIVE BINARY. The npm package's bin entry is `bin/claude.exe`
# — despite the `.exe` name it is the host platform's native executable (a Node
# Single-Executable-Application that bundles the JS payload + a node runtime
# into one file; ELF on Linux, Mach-O on macOS). The package selects the right
# binary per platform via optionalDependencies
# (@anthropic-ai/claude-code-<platform>) and a postinstall copy that lands the
# matching binary at bin/claude.exe. We install via npm so the version is
# reproducibly pinnable in CI, then install that native binary DIRECTLY as
# /var/g5d-data/runtime/bin/claude — no node wrapper.
#
# Why NOT a node wrapper anymore (the bug this replaced):
#   The previous revision did `exec node _npm/bin/claude`. Against the current
#   package that fails with `ERR_UNKNOWN_FILE_EXTENSION: Unknown file extension
#   ".exe"` because node tries to ESM-load the native binary. The binary must
#   be executed directly. Running it directly is also the upstream-intended
#   invocation (npm's own bin symlink points straight at bin/claude.exe).
#
# Why npm and not the curl|bash installer:
#   - The curl|bash installer at https://claude.ai/install.sh resolves a
#     user-scoped install path (~/.claude/local/) which is fragile inside a
#     mount-injected /opt/g5/bin tree.
#   - The npm package is reproducibly versionable via --version pinning in CI.
#
# Static-linkage caveat (rev-7 §5.0 + risk register):
#   The Linux binary is a Node SEA dynamically linked against glibc — it bundles
#   node but still needs a compatible libc inside whatever runtime executes it
#   (worker host or, via the rev-7 §5.4 read-only runtime disk at /opt/g5/bin, the
#   customer microVM base_image). This is the same libc constraint the bundled
#   node imposed before; the single self-contained binary does not add a new
#   dependency. Full static/zero-dependency hardening is tracked separately.
#
# Node bundling (P5.5 — self-contained node): this script installs the OFFICIAL
#   self-contained Node.js build (nodejs.org node-v22.x-linux-x64) at
#   /var/g5d-data/runtime/bin/node, NOT the distro `node`. codex's `bin/codex.js`
#   ESM launcher needs node at runtime, and — the reason the build matters most —
#   the in-VM `g5d dispatch` launches that `codex` inside the workspace VM,
#   where only the runtime bin dir is mounted from the read-only runtime disk (at
#   /opt/g5/bin).
#
#   Why NOT the distro node (the bug this replaced): the Debian/Ubuntu `node` is
#   a ~26KB shim (`NEEDED libnode.so.109`) that ALSO externalizes JS builtins to
#   /usr/share/nodejs/... — a path the guest does not mount. Even after staging
#   node's full libnode.so closure (the prior P5.5 fix), the in-VM node still
#   crashed loading those externalized builtins. The official self-contained
#   build embeds V8 + all builtins in ONE binary: no libnode.so, no externalized
#   files, so it runs from the mounted bin dir alone.
#
# Shared-lib closure (still co-located): the self-contained node is dynamically
#   linked only against glibc (provided by the customer base_image) + libstdc++/
#   libgcc_s. We still co-locate its `ldd` closure (libstdc++ etc., EXCLUDING the
#   glibc core + loader) next to node so a base_image with an older libstdc++ can
#   still run it, and the in-VM provider launch sets LD_LIBRARY_PATH=/opt/g5/bin
#   (a non-secret library path; see agent_runner::guest_allowlist_env /
#   host::assignment AGENT_LD_LIBRARY_PATH_ENV). The build-time smoke gate
#   reproduces the exact in-VM resolution (co-located closure + LD_LIBRARY_PATH)
#   by running `node --version` and a builtin-module load.
#
# Strict-fail policy: per Rule 5 (no graceful stubs), a missing/unrunnable
# claude binary fails the build.
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


if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" = "1" ]; then
    echo "[g5-worker] SKIP_RUNTIME_CLI_INSTALL=1; skipping Claude Code install (template validation mode)"
    exit 0
fi

# --- Install the OFFICIAL self-contained Node.js (P5.5) -----------------------
#
# The distro `node` is a libnode shim that ALSO externalizes JS builtins to
# /usr/share/nodejs (unmounted in-guest), so it crashes in the microVM even with
# its .so closure staged. The nodejs.org build is one self-contained binary
# (embedded V8 + builtins): no libnode.so, no externalized files.
RUNTIME_BIN=/var/g5d-data/runtime/bin
install -d -m 0755 "$RUNTIME_BIN"

NODE_VERSION="${NODE_VERSION:-22.23.4}"
PNPM_VERSION="${PNPM_VERSION:-11.9.0}"
PNPM_VERSION="${PNPM_VERSION#v}"
# Fail closed on a missing pin (#1342 D-13): the single source is
# scripts/claude-code-cli-version, resolved by g5-worker.pkr.hcl;
# an empty value would float to npm latest and break local/prod parity.
if [ -z "${CLAUDE_CODE_VERSION:-}" ]; then
    echo "FATAL: CLAUDE_CODE_VERSION is empty. Single source of truth:" >&2
    echo "scripts/claude-code-cli-version (resolved by g5-worker.pkr.hcl)." >&2
    exit 2
fi
NODE_DIRNAME="node-v${NODE_VERSION}-linux-x64"
NODE_TARBALL="${NODE_DIRNAME}.tar.xz"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TARBALL}"
NODE_DL_DIR="$(mktemp -d)"
trap 'rm -rf "$NODE_DL_DIR"' EXIT

echo "[g5-worker] downloading official self-contained node v${NODE_VERSION} from $NODE_URL"
download_with_retry "node ${NODE_VERSION}" "$NODE_URL" "$NODE_DL_DIR/$NODE_TARBALL"

# Supply-chain pin: a non-empty NODE_SHA256 (var.node_sha256) is the HARD pin;
# otherwise verify against the official SHASUMS256.txt fetched over TLS. Either
# way a mismatch fails the build (Rule 5 — never install an unverified runtime).
ACTUAL_SHA="$(sha256sum "$NODE_DL_DIR/$NODE_TARBALL" | awk '{print $1}')"
if [ -n "${NODE_SHA256:-}" ]; then
    if [ "$ACTUAL_SHA" != "$NODE_SHA256" ]; then
        echo "[g5-worker] FATAL: node tarball sha256 mismatch (pinned via node_sha256). expected=$NODE_SHA256 actual=$ACTUAL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] node tarball sha256 OK (pinned): $ACTUAL_SHA"
else
    download_with_retry "node SHASUMS256.txt" "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" "$NODE_DL_DIR/SHASUMS256.txt"
    EXPECTED_SHA="$(awk -v f="$NODE_TARBALL" '$2 == f {print $1}' "$NODE_DL_DIR/SHASUMS256.txt")"
    [ -n "$EXPECTED_SHA" ] || {
        echo "[g5-worker] FATAL: $NODE_TARBALL not listed in the official SHASUMS256.txt" >&2
        exit 1
    }
    if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
        echo "[g5-worker] FATAL: node tarball sha256 mismatch vs official manifest. expected=$EXPECTED_SHA actual=$ACTUAL_SHA" >&2
        exit 1
    fi
    echo "[g5-worker] node tarball sha256 OK (official manifest): $ACTUAL_SHA"
    echo "[g5-worker] HINT: pin -var node_sha256=$ACTUAL_SHA for a reproducible supply-chain lock"
fi

tar -xJf "$NODE_DL_DIR/$NODE_TARBALL" -C "$NODE_DL_DIR"
NODE_EXTRACT="$NODE_DL_DIR/$NODE_DIRNAME"
[ -x "$NODE_EXTRACT/bin/node" ] || {
    echo "[g5-worker] FATAL: extracted tarball missing bin/node at $NODE_EXTRACT/bin/node" >&2
    exit 1
}
install -m 0755 "$NODE_EXTRACT/bin/node" "$RUNTIME_BIN/node"
NODE_BIN="$RUNTIME_BIN/node"

# Expose package managers from the same official Node tarball. Copy npm/corepack
# from the tarball and vendor a pinned pnpm release into the same closure, then
# wrap each command so it uses the mounted node binary and writes caches outside
# the read-only /opt/g5/bin runtime mount.
NODEJS_RUNTIME="$RUNTIME_BIN/nodejs"
rm -rf "$NODEJS_RUNTIME"
install -d -m 0755 "$NODEJS_RUNTIME/lib"
cp -a "$NODE_EXTRACT/lib/node_modules" "$NODEJS_RUNTIME/lib/"
"$NODE_EXTRACT/bin/node" "$NODE_EXTRACT/lib/node_modules/npm/bin/npm-cli.js" \
    install --global --prefix "$NODEJS_RUNTIME" --no-audit --no-fund "pnpm@${PNPM_VERSION}"
chmod -R a+rX "$NODEJS_RUNTIME"
cat > "$RUNTIME_BIN/npm" <<'EOF'
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${npm_config_cache:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export npm_config_cache="$G5_NPM_CACHE"
    else
        export npm_config_cache="${HOME:-/tmp}/.npm"
    fi
fi
mkdir -p "$npm_config_cache" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/npm/bin/npm-cli.js" "$@"
EOF
cat > "$RUNTIME_BIN/npx" <<'EOF'
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${npm_config_cache:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export npm_config_cache="$G5_NPM_CACHE"
    else
        export npm_config_cache="${HOME:-/tmp}/.npm"
    fi
fi
mkdir -p "$npm_config_cache" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/npm/bin/npx-cli.js" "$@"
EOF
cat > "$RUNTIME_BIN/corepack" <<'EOF'
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${COREPACK_HOME:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export COREPACK_HOME="$G5_NPM_CACHE/corepack"
    else
        export COREPACK_HOME="${HOME:-/tmp}/.cache/corepack"
    fi
fi
mkdir -p "$COREPACK_HOME" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/corepack/dist/corepack.js" "$@"
EOF
cat > "$RUNTIME_BIN/pnpm" <<'EOF'
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
exec "$NODE" "$HERE/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" "$@"
EOF
chmod 0755 "$RUNTIME_BIN/npm" "$RUNTIME_BIN/npx" "$RUNTIME_BIN/corepack" "$RUNTIME_BIN/pnpm"

# Regression guard: the whole point of switching to the self-contained build is
# that node no longer NEEDs libnode.so (the distro shim we replaced). If it does,
# we accidentally installed a shim — fail rather than reintroduce the in-guest
# crash.
if ldd "$NODE_BIN" 2>/dev/null | grep -q 'libnode\.so'; then
    echo "[g5-worker] FATAL: staged node still depends on libnode.so — not the self-contained build" >&2
    ldd "$NODE_BIN" >&2 || true
    exit 1
fi

# --- Co-locate node's (small) shared-lib closure into the runtime bin dir ------
#
# The self-contained node links only against glibc + libstdc++/libgcc_s. We
# co-locate the application-level closure (libstdc++ etc.) next to node, EXCLUDING
# the glibc core + dynamic loader (provided by the customer base_image; co-packaging
# them risks ABI/loader conflicts in-guest). The in-VM sidecar spawn adds
# LD_LIBRARY_PATH=/opt/g5/bin so these resolve even on a base_image with an older
# libstdc++.
echo "[g5-worker] staging node shared-lib closure into $RUNTIME_BIN"
node_closure_staged=0
while read -r lib; do
    [ -n "$lib" ] && [ -f "$lib" ] || continue
    case "$(basename "$lib")" in
        ld-linux*|ld.so*|libc.so.*|libc-*.so|libm.so.*|libm-*.so|libdl.so.*|libpthread.so.*|librt.so.*|libresolv.so.*|libgcc_s.so.*)
            # glibc core / loader provided by the guest base_image. (libgcc_s is
            # part of the base toolchain runtime; leave it to the base image.)
            continue ;;
    esac
    install -m 0644 "$lib" "$RUNTIME_BIN/"
    node_closure_staged=$((node_closure_staged + 1))
done < <(ldd "$NODE_BIN" | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^\//) print $i }')
echo "[g5-worker] staged $node_closure_staged closure lib(s) alongside node"

# Strict smoke gate (P5.5): reproduce the EXACT in-VM resolution — co-located
# closure in $ORIGIN + LD_LIBRARY_PATH at the same dir — and prove the staged
# node loads its embedded builtins (the distro shim externalized them and
# crashed in-guest), not merely that `node --version` prints. Per Rule 5, a
# node that cannot run under the mounted layout fails the build.
if ! LD_LIBRARY_PATH="$RUNTIME_BIN" "$NODE_BIN" -e 'require("node:fs"); require("node:child_process"); console.log("node self-check OK " + process.version)' >/tmp/node-selfcheck.out 2>&1; then
    echo "[g5-worker] FATAL: node builtin self-check failed with LD_LIBRARY_PATH=$RUNTIME_BIN (self-contained node broken under mounted layout):" >&2
    cat /tmp/node-selfcheck.out >&2 || true
    exit 1
fi
echo "[g5-worker] node self-check (LD_LIBRARY_PATH=$RUNTIME_BIN): $(cat /tmp/node-selfcheck.out)"
echo "[g5-worker] staged node --version: $(LD_LIBRARY_PATH="$RUNTIME_BIN" "$NODE_BIN" --version)"
NODE_VERSION_OUT="$NODE_DL_DIR/node-version.out"
NPM_VERSION_OUT="$NODE_DL_DIR/npm-version.out"
NPX_VERSION_OUT="$NODE_DL_DIR/npx-version.out"
PNPM_VERSION_OUT="$NODE_DL_DIR/pnpm-version.out"
COREPACK_VERSION_OUT="$NODE_DL_DIR/corepack-version.out"
LD_LIBRARY_PATH="$RUNTIME_BIN" "$NODE_BIN" --version | sed 's/^v//' > "$NODE_VERSION_OUT"
if ! "$RUNTIME_BIN/npm" --version >"$NPM_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: npm --version failed under the mounted runtime layout" >&2
    cat "$NPM_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/npx" --version >"$NPX_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: npx --version failed under the mounted runtime layout" >&2
    cat "$NPX_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/pnpm" --version >"$PNPM_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: pnpm --version failed under the mounted runtime layout" >&2
    cat "$PNPM_VERSION_OUT" >&2 || true
    exit 1
fi
if ! "$RUNTIME_BIN/corepack" --version >"$COREPACK_VERSION_OUT" 2>&1; then
    echo "[g5-worker] FATAL: corepack --version failed under the mounted runtime layout" >&2
    cat "$COREPACK_VERSION_OUT" >&2 || true
    exit 1
fi
install -m 0644 "$NODE_VERSION_OUT" "$RUNTIME_BIN/node.version"
install -m 0644 "$NPM_VERSION_OUT" "$RUNTIME_BIN/npm.version"
install -m 0644 "$NPX_VERSION_OUT" "$RUNTIME_BIN/npx.version"
install -m 0644 "$PNPM_VERSION_OUT" "$RUNTIME_BIN/pnpm.version"
install -m 0644 "$COREPACK_VERSION_OUT" "$RUNTIME_BIN/corepack.version"
echo "[g5-worker] staged npm $(cat "$RUNTIME_BIN/npm.version"), npx $(cat "$RUNTIME_BIN/npx.version"), pnpm $(cat "$RUNTIME_BIN/pnpm.version"), corepack $(cat "$RUNTIME_BIN/corepack.version")"

# --- Stage a portable Python runtime for Claude Code profile hooks -----------
#
# The provided Claude Code profiles invoke Python hooks from `.claude/settings.json`
# during SessionStart/UserPromptSubmit/Tool events. The guest sees only the
# runtime bin mount at /opt/g5/bin, so depending on customer base_image python3
# makes startup fail before the first LLM request. Stage Python + stdlib +
# application-level shared libs into the same mounted runtime and expose a
# /opt/g5/bin/python3 wrapper that sets PYTHONHOME/LD_LIBRARY_PATH.
PYVER="$(python3 - <<'PY'
import sys
print(f"{sys.version_info.major}.{sys.version_info.minor}")
PY
)"
PY_BIN_REAL="$(readlink -f "$(command -v python3)")"
PY_RUNTIME="$RUNTIME_BIN/python3-runtime"
rm -rf "$PY_RUNTIME"
install -d -m 0755 "$PY_RUNTIME/bin" "$PY_RUNTIME/lib" "$PY_RUNTIME/lib/native"
install -m 0755 "$PY_BIN_REAL" "$PY_RUNTIME/bin/python${PYVER}"
cp -a "/usr/lib/python${PYVER}" "$PY_RUNTIME/lib/"
py_closure_staged=0
stage_python_libs() {
    subject="$1"
    while read -r lib; do
        [ -n "$lib" ] && [ -f "$lib" ] || continue
        case "$(basename "$lib")" in
            ld-linux*|ld.so*|libc.so.*|libc-*.so|libm.so.*|libm-*.so|libdl.so.*|libpthread.so.*|librt.so.*)
                continue ;;
        esac
        if [ ! -e "$PY_RUNTIME/lib/native/$(basename "$lib")" ]; then
            install -m 0644 "$lib" "$PY_RUNTIME/lib/native/"
            py_closure_staged=$((py_closure_staged + 1))
        fi
    done < <(ldd "$subject" | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^\//) print $i }')
}
stage_python_libs "$PY_BIN_REAL"
SQLITE_EXT="$(find "$PY_RUNTIME/lib/python${PYVER}/lib-dynload" -maxdepth 1 -name '_sqlite3*.so' -print -quit)"
if [ -z "$SQLITE_EXT" ]; then
    echo "[g5-worker] FATAL: staged python runtime missing _sqlite3 extension; gcloud requires sqlite3" >&2
    exit 1
fi
stage_python_libs "$SQLITE_EXT"
cat > "$RUNTIME_BIN/python3" <<EOF
#!/bin/sh
set -eu
HERE="\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)"
export PYTHONHOME="\$HERE/python3-runtime"
if [ -n "\${LD_LIBRARY_PATH:-}" ]; then
    export LD_LIBRARY_PATH="\$HERE/python3-runtime/lib/native:\$LD_LIBRARY_PATH"
else
    export LD_LIBRARY_PATH="\$HERE/python3-runtime/lib/native"
fi
exec "\$HERE/python3-runtime/bin/python${PYVER}" "\$@"
EOF
chmod 0755 "$RUNTIME_BIN/python3"
if ! "$RUNTIME_BIN/python3" -c 'import json, pathlib, sqlite3, subprocess, sys; print(sys.version.split()[0])' >/tmp/python3-runtime.out 2>&1; then
    echo "[g5-worker] FATAL: staged python3 runtime failed; Claude Code hooks would fail in-guest:" >&2
    cat /tmp/python3-runtime.out >&2 || true
    exit 1
fi
echo "[g5-worker] staged python3 $(cat /tmp/python3-runtime.out) for Claude Code hooks (${py_closure_staged} closure lib(s))"

# Install claude-code into a build-time npm prefix. The package postinstall
# copies the platform-matched native binary into bin/claude.exe.
NPM_BUILD_PREFIX=/var/g5d-data/runtime/npm
install -d -m 0755 "$NPM_BUILD_PREFIX"
npm config set prefix "$NPM_BUILD_PREFIX" --global

PIN="@${CLAUDE_CODE_VERSION}"

echo "[g5-worker] npm install -g @anthropic-ai/claude-code${PIN}"
# --no-audit --no-fund cuts ~20s and is fine for an air-gapped runtime image.
npm install --global --no-audit --no-fund "@anthropic-ai/claude-code${PIN}"

# Locate the native binary that npm staged. `bin/claude.exe` is the canonical
# entrypoint; fall back to the package.json `bin.claude` field if upstream
# renames it.
CC_PKG="$NPM_BUILD_PREFIX/lib/node_modules/@anthropic-ai/claude-code"
CLAUDE_NATIVE="$CC_PKG/bin/claude.exe"
if [ ! -f "$CLAUDE_NATIVE" ]; then
    BINREL="$(jq -r '.bin.claude // empty' "$CC_PKG/package.json" 2>/dev/null || true)"
    [ -n "$BINREL" ] && CLAUDE_NATIVE="$CC_PKG/$BINREL"
fi
if [ ! -f "$CLAUDE_NATIVE" ]; then
    echo "[g5-worker] FATAL: claude native binary not found under $CC_PKG/bin" >&2
    ls -la "$CC_PKG/bin" 2>/dev/null || true
    exit 1
fi

# Guard against a regression to a node-script entrypoint: confirm the file is a
# native executable. If upstream ever reverts to a JS bin we must rework the
# wrapper rather than silently install an unrunnable shim.
if ! file -b "$CLAUDE_NATIVE" | grep -qE "ELF .*executable|Mach-O .*executable"; then
    echo "[g5-worker] FATAL: $CLAUDE_NATIVE is not a native executable:" >&2
    file "$CLAUDE_NATIVE" >&2 || true
    exit 1
fi

# Capture the resolved version BEFORE pruning the npm tree.
if [ -f "$CC_PKG/package.json" ]; then
    RESOLVED_VER="$(jq -r .version "$CC_PKG/package.json")"
    echo "$RESOLVED_VER" > /tmp/claude-code.version
else
    echo "unknown" > /tmp/claude-code.version
fi

# Install the native binary DIRECTLY as runtime/bin/claude (a real file, so the
# rev-7 §5.4 read-only runtime disk carries the actual self-contained executable into
# the customer microVM at /opt/g5/bin/claude — no node, no _npm tree needed).
install -m 0755 "$CLAUDE_NATIVE" /var/g5d-data/runtime/bin/claude

# Prune the claude-code package from the npm prefix now that its binary is
# installed standalone. This keeps 50-install-codex.sh's _npm vendoring (a full
# `cp -a` of the prefix) lean — otherwise the ~200MB claude SEA would be
# duplicated into the mount-injected _npm tree on every customer microVM.
rm -rf "$CC_PKG"
rm -f "$NPM_BUILD_PREFIX/bin/claude"

# Smoke test: the binary MUST print a version string. Failure here means the
# image is not production-ready and we fail the build per Rule 5.
if ! /var/g5d-data/runtime/bin/claude --version >/tmp/claude-version.out 2>&1; then
    echo "[g5-worker] FATAL: claude --version failed; the installed binary is unrunnable" >&2
    cat /tmp/claude-version.out >&2 || true
    exit 1
fi
echo "[g5-worker] claude --version: $(cat /tmp/claude-version.out)"

# Flag contract (docs/lean-provider-cli-contract.md): the provider runner passes
# a fixed flag set to `claude -p`; prove the pinned binary carries it. The
# checker is uploaded by the packer `file` provisioner (g5-worker.pkr.hcl) from
# scripts/check-claude-cli-flags.sh; a missing checker fails closed like the
# download helper above.
G5_CLAUDE_FLAG_CHECK="${G5_CLAUDE_FLAG_CHECK:-/tmp/g5-check-claude-cli-flags.sh}"
[ -r "$G5_CLAUDE_FLAG_CHECK" ] || {
    echo "[g5-worker] FATAL: Claude CLI flag checker not readable at ${G5_CLAUDE_FLAG_CHECK}." >&2
    echo "[g5-worker] Provide it (packer: file provisioner from scripts/check-claude-cli-flags.sh; container: bind-mount + G5_CLAUDE_FLAG_CHECK)." >&2
    exit 1
}
sh "$G5_CLAUDE_FLAG_CHECK" /var/g5d-data/runtime/bin/claude "$CLAUDE_CODE_VERSION"

echo "[g5-worker] 40-install-claude.sh OK (claude=$(cat /tmp/claude-code.version))"
