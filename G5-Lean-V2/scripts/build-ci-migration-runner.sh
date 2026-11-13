#!/usr/bin/env bash
# Publish the candidate's production migrator for all CI database setup steps.
set -euo pipefail

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
export SQLX_OFFLINE=true
: "${CARGO_TARGET_DIR:?the CI builder must provide its private Cargo target directory}"

bash scripts/clean-baked-rust-workspace-artifacts.sh
cargo build --locked --package g5-api --bin g5-db-migrate

mkdir -p .ci-migration-artifacts
install -m 0755 "$CARGO_TARGET_DIR/debug/g5-db-migrate" .ci-migration-artifacts/g5-db-migrate
