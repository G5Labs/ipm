#!/usr/bin/env bash
# =============================================================================
# run-integration.sh — Lightweight integration tests against the dev stack
# =============================================================================
# Runs integration tests without a full E2E rebuild. Assumes the dev stack
# (docker compose dev) is already running.
# Usage: ./scripts/run-integration.sh
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Keep shell-side port/url defaults aligned with the same `.env` file that
# docker compose dev reads, when present.
if [ -f "$ROOT_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$ROOT_DIR/.env"
  set +a
fi

# Requires dev stack running: just dev-stack-up
API_URL="${G5_API_URL:-http://localhost:${G5_PORT_API:-7500}}"

echo "Running integration tests against $API_URL..."

# Rust integration tests (require DATABASE_URL). The fallback
# port is the compose file's own default (5432), which also matches the
# g5d-g5dev guest's native test-DB helper; a laptop that shifts ports does so
# through the `.env` sourced above (see .env.example).
export DATABASE_URL="${DATABASE_URL:-postgres://g5:g5_dev@localhost:${G5_PORT_POSTGRES:-5432}/g5_dev}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

cargo test -p g5-api --test '*' -- --test-threads=1
cargo test -p g5d-dispatch -p g5d --test '*' -- --test-threads=1

echo "Integration tests complete."
