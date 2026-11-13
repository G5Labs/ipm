#!/usr/bin/env bash
# g5-dev-testdb-start — throwaway native Postgres 16 (+pgvector) for
# DB-backed test runs inside a g5d-g5dev guest, no sudo and no Docker needed
# (software_architecture/infra/docker_images I32).
#
# Usage:
#   g5-dev-testdb-start          start (idempotent) and print URL exports
#   g5-dev-testdb-start --stop   stop the helper-owned Postgres
#
# Ownership discipline (I32): this helper manages ONLY the Postgres it started
# (its own PGDATA). An occupied port it does not own (e.g. the `just dev-up`
# compose stack) is reported, never adopted, modified, or shut down.
#
# Role/db match the docker-compose.dev.yml defaults (g5 / g5_dev / db g5_dev)
# so DATABASE_URL-driven integration tests run unchanged.
set -euo pipefail

PG_BIN="/usr/lib/postgresql/16/bin"
DIR="${G5_TESTDB_DIR:-/tmp/g5-testdb}"
PGDATA="${DIR}/pg"
PG_PORT="${G5_TESTDB_PG_PORT:-5432}"

# initdb refuses to run as root; the agent identity is uid 1000 (ubuntu).
if [[ "$(id -u)" -eq 0 ]]; then
  exec runuser -u ubuntu -- "$0" "$@"
fi

own_pg_running() {
  [[ -d "${PGDATA}" ]] && "${PG_BIN}/pg_ctl" -D "${PGDATA}" status >/dev/null 2>&1
}

if [[ "${1:-}" == "--stop" ]]; then
  if own_pg_running; then
    "${PG_BIN}/pg_ctl" -D "${PGDATA}" -m fast stop >/dev/null
  fi
  echo "g5-dev-testdb: stopped the helper-owned Postgres (data kept under ${DIR})" >&2
  exit 0
fi

mkdir -p "${DIR}"

if ! own_pg_running; then
  if "${PG_BIN}/pg_isready" -q -h 127.0.0.1 -p "${PG_PORT}"; then
    echo "g5-dev-testdb: port ${PG_PORT} already serves a Postgres this helper does not own (dev-up stack?) — use that stack, or set G5_TESTDB_PG_PORT to a free port" >&2
    exit 1
  fi
  if [[ ! -f "${PGDATA}/PG_VERSION" ]]; then
    # trust auth on loopback: single-user throwaway VM, no secret to protect.
    "${PG_BIN}/initdb" -D "${PGDATA}" -U g5 --auth=trust >/dev/null
  fi
  # -k: the default unix_socket_directories (/var/run/postgresql) is
  # root-owned; keep the socket in the data dir's parent instead.
  "${PG_BIN}/pg_ctl" -D "${PGDATA}" -w -l "${DIR}/pg.log" \
    -o "-p ${PG_PORT} -c listen_addresses=127.0.0.1 -k ${DIR}" start >/dev/null
fi

# Setup runs only against the helper-owned server (guaranteed above).
psql() { command psql -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "${PG_PORT}" -U g5 "$@"; }
if ! psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'g5_dev'" | grep -q 1; then
  psql -d postgres -c "CREATE DATABASE g5_dev" >/dev/null
fi
# Password is irrelevant under trust auth but set so the compose-shaped URL
# (with password) also works if auth is ever tightened.
psql -d postgres -c "ALTER USER g5 PASSWORD 'g5_dev'" >/dev/null
psql -d g5_dev -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null

# TEST_DATABASE_URL is what the g5-api integration harness reads; the G5_*
# name is what the API itself reads. Export all of them so `eval "$(...)"`
# followed by a bare `cargo test` needs no hand-written variables.
cat <<EOF
export DATABASE_URL=postgres://g5:g5_dev@127.0.0.1:${PG_PORT}/g5_dev
export TEST_DATABASE_URL=postgres://g5:g5_dev@127.0.0.1:${PG_PORT}/g5_dev
export G5_DATABASE_URL=postgres://g5:g5_dev@127.0.0.1:${PG_PORT}/g5_dev
EOF
echo "g5-dev-testdb: postgres :${PG_PORT} ready (logs/data under ${DIR})" >&2
