#!/usr/bin/env bash
# =============================================================================
# 50-install-codex.sh — Install OpenAI's codex CLI into runtime/bin/.
#
# Distribution: the codex CLI ships as @openai/codex on npm. UNLIKE the current
# claude-code package (a self-contained native binary), codex's npm bin entry is
# a real ESM launcher script `bin/codex.js` that, at runtime, resolves and execs
# the platform-native binary from its optionalDependency
# (@openai/codex-<platform>; the Linux build is the musl-static
# x86_64-unknown-linux-musl variant). Because codex.js needs node, we keep the
# node-wrapper pattern here: npm-install globally into /var/g5d-data/runtime/npm,
# vendor the resulting tree under runtime/bin/_npm/, and drop a wrapper at
# /var/g5d-data/runtime/bin/codex that re-execs codex.js through the bundled
# Node.js (staged at runtime/bin/node by 40-install-claude.sh).
#
# Same static-linkage caveat and strict-fail policy applies. See
# 40-install-claude.sh header for the full rationale.
# =============================================================================
set -euo pipefail

stage_codex_ca_bundle() {
    # Codex runs in a cleared sidecar environment inside the customer microVM.
    # Stage a deterministic CA bundle into the mounted runtime closure and pass
    # its path explicitly from the in-guest agent runner.
    install -m 0644 /etc/ssl/certs/ca-certificates.crt /var/g5d-data/runtime/bin/ca-certificates.crt
    if [ ! -s /var/g5d-data/runtime/bin/ca-certificates.crt ]; then
        echo "[g5-worker] FATAL: staged CA bundle is empty: /var/g5d-data/runtime/bin/ca-certificates.crt" >&2
        exit 1
    fi
}

if [ "${SKIP_RUNTIME_CLI_INSTALL:-0}" = "1" ]; then
    echo "[g5-worker] SKIP_RUNTIME_CLI_INSTALL=1; skipping codex install (template validation mode)"
    exit 0
fi

NPM_BUILD_PREFIX=/var/g5d-data/runtime/npm
# Node + npm prefix were already configured in 40-install-claude.sh; we
# re-set npm prefix defensively in case the scripts are re-run individually.
npm config set prefix "$NPM_BUILD_PREFIX" --global

# Fail closed on a missing pin (#1342 D-13): the single source is
# scripts/codex-cli-version, resolved by g5-worker.pkr.hcl; an
# empty value would float to npm latest and can change the platform
# optionalDependency closure between otherwise-identical image builds.
if [ -z "${CODEX_VERSION:-}" ]; then
    echo "FATAL: CODEX_VERSION is empty. Single source of truth:" >&2
    echo "scripts/codex-cli-version (resolved by g5-worker.pkr.hcl)." >&2
    exit 2
fi
PIN="@${CODEX_VERSION}"

echo "[g5-worker] npm install -g @openai/codex${PIN}"
npm install --global --no-audit --no-fund "@openai/codex${PIN}"

# Vendor the npm tree into runtime/bin/_npm/ so codex.js + its platform-native
# optionalDependency are carried by the rev-7 §5.4 read-only runtime disk into the
# customer microVM. 40-install-claude.sh installs claude as a standalone native
# binary and prunes its package from the prefix, so this `cp -a` vendors only
# the codex tree (no ~200MB claude SEA duplicated into the mount).
NPM_VENDORED=/var/g5d-data/runtime/bin/_npm
rm -rf "$NPM_VENDORED"
install -d -m 0755 "$(dirname "$NPM_VENDORED")"
cp -a "$NPM_BUILD_PREFIX" "$NPM_VENDORED"
chmod -R a+rX "$NPM_VENDORED"

CODEX_PKG="$NPM_VENDORED/lib/node_modules/@openai/codex"
[ -f "$CODEX_PKG/package.json" ] || {
    echo "[g5-worker] FATAL: codex package.json missing after vendoring: $CODEX_PKG/package.json" >&2
    exit 1
}
CODEX_PLATFORM_PKG="$(/var/g5d-data/runtime/bin/node -e "
const path = require(\"path\");
const pkgPath = process.argv[1];
const key = \"@openai/codex-\" + process.platform + \"-\" + process.arch;
const pkg = require(path.join(pkgPath, \"package.json\"));
if (!pkg.optionalDependencies || !pkg.optionalDependencies[key]) {
  console.error(\"FATAL: @openai/codex does not declare optionalDependency \" + key);
  process.exit(1);
}
try {
  const resolved = require.resolve(key + \"/package.json\", { paths: [pkgPath] });
  console.log(key + \" \" + resolved);
} catch (err) {
  console.error(\"FATAL: @openai/codex optionalDependency \" + key + \" missing from vendored npm closure under \" + pkgPath + \". npm install may have omitted optional dependencies; do not advertise Codex without this native binary.\");
  process.exit(1);
}
" "$CODEX_PKG")"
echo "[g5-worker] verified codex optional dependency ${CODEX_PLATFORM_PKG}"

if [ ! -x "$NPM_VENDORED/bin/codex" ]; then
    CANDIDATE="$(find "$NPM_VENDORED" -name codex -type f -executable | head -1)"
    [ -n "$CANDIDATE" ] || {
        echo "[g5-worker] FATAL: codex bin not found under $NPM_VENDORED after vendoring" >&2
        ls -la "$NPM_VENDORED/bin" || true
        exit 1
    }
    REL="$(realpath --relative-to="$NPM_VENDORED/bin" "$CANDIDATE")"
    ln -s "$REL" "$NPM_VENDORED/bin/codex"
fi
[ -e "$NPM_VENDORED/bin/codex" ] || {
    echo "[g5-worker] FATAL: codex npm launcher closure missing at $NPM_VENDORED/bin/codex" >&2
    exit 1
}

# Wrapper resolves everything relative to $HERE (= /opt/g5/bin inside a
# customer microVM via the rev-7 §5.4 read-only runtime disk).
cat > /var/g5d-data/runtime/bin/codex <<'EOF'
#!/usr/bin/env bash
# G5 worker-image wrapper for codex CLI. See claude wrapper for rationale.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
exec "$NODE" "$HERE/_npm/bin/codex" "$@"
EOF
chmod +x /var/g5d-data/runtime/bin/codex

stage_codex_ca_bundle
sh /tmp/g5-check-codex-cli-flags.sh /var/g5d-data/runtime/bin/codex "$CODEX_VERSION"

if ! /var/g5d-data/runtime/bin/codex --version >/tmp/codex-version.out 2>&1; then
    echo "[g5-worker] FATAL: codex --version failed; the npm install produced an unrunnable binary" >&2
    cat /tmp/codex-version.out >&2 || true
    exit 1
fi
echo "[g5-worker] codex --version: $(cat /tmp/codex-version.out)"

if [ -f "$NPM_BUILD_PREFIX/lib/node_modules/@openai/codex/package.json" ]; then
    RESOLVED_VER="$(jq -r .version "$NPM_BUILD_PREFIX/lib/node_modules/@openai/codex/package.json")"
    echo "$RESOLVED_VER" > /tmp/codex.version
else
    echo "unknown" > /tmp/codex.version
fi

echo "[g5-worker] 50-install-codex.sh OK (codex=$(cat /tmp/codex.version))"
