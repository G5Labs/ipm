#!/usr/bin/env bash
# Build the platform host, guest runtime and pinned VMM/kernel artifacts.
set -euo pipefail

SRC_DIR="/tmp/g5d-src"
[ -d "$SRC_DIR" ] || {
    echo "[g5-worker] FATAL: g5d source tree not found at $SRC_DIR; check Packer file provisioner ordering" >&2
    exit 1
}

# Strip any cached build artefacts that may have come along (target/, node_modules/).
# Defensive — the pipeline's source tarball excludes them, but a developer running
# `packer build` from a dirty worktree should not silently bake stale bytes.
rm -rf "$SRC_DIR/target" "$SRC_DIR/frontend/node_modules" || true

export CARGO_HOME=/opt/rust/cargo
export RUSTUP_HOME=/opt/rust/rustup
export PATH="$CARGO_HOME/bin:$PATH"
export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/usr/local/lib64/pkgconfig"
export LIBRARY_PATH="/usr/local/lib:/usr/local/lib64"
export LD_LIBRARY_PATH="/usr/local/lib:/usr/local/lib64"

# SQLX_OFFLINE matches the canonical CLAUDE.md backend command — required
# because g5d transitively depends on a sqlx-using crate and the build VM
# has no DATABASE_URL.
export SQLX_OFFLINE=true

cd "$SRC_DIR"
bash scripts/build-cloud-hypervisor-runtime.sh /opt/g5-host
echo "[g5-worker] cargo build --locked --release -p g5d  (Cloud Hypervisor backend)"
cargo build --locked --release -p g5d

BIN="$SRC_DIR/target/release/g5d"
[ -x "$BIN" ] || {
    echo "[g5-worker] FATAL: g5d binary not produced at $BIN" >&2
    exit 1
}

# Install the binary into the rev-7 §3 path.
install -d -m 0755 /var/g5d-data/runtime/bin
install -m 0755 "$BIN" /var/g5d-data/runtime/bin/g5d

# The host supervisor reads this catalog before registering with the API, and
# each workspace VM mounts it from the same runtime root. Keep the reviewed
# digest beside the bytes, as the Docker and local runtime staging paths do.
CATALOG_SRC="$SRC_DIR/backend/g5d-dispatch/modules"
CATALOG_DST=/var/g5d-data/runtime/modules
install -d -m 0755 "$CATALOG_DST"
install -m 0644 "$CATALOG_SRC/catalog.json" "$CATALOG_DST/catalog.json"
install -m 0644 "$CATALOG_SRC/catalog.sha256" "$CATALOG_DST/catalog.sha256"
echo "$(tr -d '[:space:]' < "$CATALOG_DST/catalog.sha256")  $CATALOG_DST/catalog.json" | sha256sum -c -

/var/g5d-data/runtime/bin/g5d --version

# Record the SHA + size for the worker-image.json (75-write-image-info.sh).
sha256sum /var/g5d-data/runtime/bin/g5d > /tmp/g5d.sha256
stat -c '%s' /var/g5d-data/runtime/bin/g5d > /tmp/g5d.size

install -d -m 0755 /var/g5d-data/runtime/bin

# Stage the harness before deleting the source tree. Workspace VMs mount this
# runtime at /opt/g5, where agent.turn reads it for every provider launch.
HARNESS=/var/g5d-data/runtime/assets/harness
rm -rf "$HARNESS"
install -d -m 0755 "$HARNESS/.agents"
cp -a "$SRC_DIR/.agents/AGENTS.md" "$HARNESS/AGENTS.md"
cp -a "$SRC_DIR/.agents/skills" "$HARNESS/.agents/skills"
cp -a "$SRC_DIR/.agents/guidance" "$HARNESS/.agents/guidance"
# Fail the build rather than ship an image whose every agent turn fails.
if [ ! -s "$HARNESS/AGENTS.md" ]; then
    echo "[g5-worker] FATAL: harness not staged under $HARNESS" >&2
    exit 1
fi
for skill in "$HARNESS"/.agents/skills/*/; do
    [ -s "${skill}SKILL.md" ] || {
        echo "[g5-worker] FATAL: harness skill incomplete: $skill" >&2
        exit 1
    }
done
echo "[g5-worker] staged harness -> $HARNESS"

# Retain build scripts until runtime disk assembly in phase 60.

echo "[g5-worker] 30-build-g5d.sh OK ($(cat /tmp/g5d.size) bytes)"
