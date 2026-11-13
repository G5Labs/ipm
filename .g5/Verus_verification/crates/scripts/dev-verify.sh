#!/usr/bin/env bash
# Development-only verification run (no receipt). Usage: dev-verify.sh [timeout_s] [extra verus args...]
set -u
VV=$(cd "$(dirname "$0")/../.." && pwd)
VD=$HOME/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux
T=${1:-300}; shift || true
cd "$VV/crates"; rm -rf target/verus-partial/debug/.fingerprint/ipm-load-history-* target/debug/.fingerprint/ipm-load-history-*
export CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$VV/crates/target" RUSTUP_TOOLCHAIN=1.98.1-x86_64-unknown-linux-gnu
timeout --kill-after=10 "$T" "$VD/cargo-verus" ${CV_CMD:-verify} -p ipm-load-history --offline ${CV_EXTRA:-} -- --num-threads 4 "$@" 2>&1
echo "EXIT=$?"
