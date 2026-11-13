#!/usr/bin/env bash
# =============================================================================
# rehearse-migration-upgrade.sh — upgrade rehearsal with rows, as the real
# deployment migrator
# =============================================================================
# CI migrates an EMPTY database, and local Compose migrates as the container
# superuser. Neither can show that a data-bearing migration works against
# existing rows for the Cloud SQL admin, which is NOSUPERUSER and NOBYPASSRLS
# and therefore sees zero rows of every FORCE ROW LEVEL SECURITY table
# (docs/database-migrations.md, "Migrator row visibility"). This script
# rehearses exactly that path:
#
#   1. drop/create REHEARSAL_DB on the given Postgres (superuser connection)
#   2. provision g5_migrator with the Cloud SQL admin's attributes
#      (scripts/ci-provision-migrator.sh) and bootstrap the runtime roles
#      (scripts/bootstrap-runtime-db-users.sh) as that migrator
#   3. apply BASE_REF's migrations as g5_migrator (the schema the target
#      database is on today)
#   4. load ROWS_SQL as the superuser: a data-only dump of a database on
#      BASE_REF's schema taken with
#        pg_dump --data-only --exclude-table-data=public._sqlx_migrations
#      (step 3 writes the ledger; a dump that carries it fails on the
#      primary key and is refused before loading), or a fixture inserting
#      representative rows
#   5. apply this checkout's migrations as g5_migrator — through
#      G5_DB_MIGRATE_BIN so the production row-visibility guard runs — and
#      rerun them to prove the no-op path
#
# Required: psql, git, and G5_DB_MIGRATE_BIN pointing to this checkout's
# built g5-db-migrate. Connection: PGHOST/PGPORT/PGUSER/PGPASSWORD for the SUPERUSER.
#
# Usage:
#   PGHOST=127.0.0.1 PGPORT=55432 PGUSER=g5 PGPASSWORD=g5_test \
#   BASE_REF=origin/main ROWS_SQL=rows.sql G5_DB_MIGRATE_BIN=/path/to/g5-db-migrate \
#     scripts/rehearse-migration-upgrade.sh
#
# Optional:
#   REHEARSAL_DB       database name (default g5_rehearsal)
#   HEAD_MIGRATIONS    migrations dir to apply in step 5
#                      (default backend/g5-api/src/db/migrations)
#   G5_MIGRATOR_DB_PASSWORD  (default g5_test)
# =============================================================================
set -euo pipefail

: "${PGHOST:=127.0.0.1}"
: "${PGPORT:=5432}"
: "${PGUSER:=g5}"
: "${PGPASSWORD:?set PGPASSWORD for the superuser connection}"
: "${BASE_REF:=origin/main}"
: "${ROWS_SQL:?set ROWS_SQL to a data-only dump or fixture SQL file}"
: "${G5_DB_MIGRATE_BIN:?set G5_DB_MIGRATE_BIN to g5-db-migrate built from this checkout}"
: "${REHEARSAL_DB:=g5_rehearsal}"
: "${HEAD_MIGRATIONS:=backend/g5-api/src/db/migrations}"
: "${G5_MIGRATOR_DB_USER:=g5_migrator}"
: "${G5_MIGRATOR_DB_PASSWORD:=g5_test}"
: "${G5_RUNTIME_SYSTEM_DB_PASSWORD:=g5_system_test}"
: "${G5_RUNTIME_TENANT_DB_PASSWORD:=g5_tenant_test}"
: "${G5_AUTHZ_WRITER_DB_PASSWORD:=g5_authz_test}"
export PGHOST PGPORT PGPASSWORD

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in psql git; do
  command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: $tool is required" >&2; exit 1; }
done
[[ -x "$G5_DB_MIGRATE_BIN" ]] || { echo "ERROR: G5_DB_MIGRATE_BIN is not executable: $G5_DB_MIGRATE_BIN" >&2; exit 1; }
[[ -f "$ROWS_SQL" ]] || { echo "ERROR: ROWS_SQL file not found: $ROWS_SQL" >&2; exit 1; }
[[ -d "$HEAD_MIGRATIONS" ]] || { echo "ERROR: HEAD_MIGRATIONS is not a directory: $HEAD_MIGRATIONS" >&2; exit 1; }

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

migrator_url="postgres://${G5_MIGRATOR_DB_USER}:${G5_MIGRATOR_DB_PASSWORD}@${PGHOST}:${PGPORT}/${REHEARSAL_DB}"

echo "=== 1. Fresh database ${REHEARSAL_DB} on ${PGHOST}:${PGPORT} ===" >&2
psql -U "$PGUSER" -d postgres -v ON_ERROR_STOP=1 -q \
  -c "DROP DATABASE IF EXISTS \"${REHEARSAL_DB}\"" \
  -c "CREATE DATABASE \"${REHEARSAL_DB}\""

echo "=== 2. Provision ${G5_MIGRATOR_DB_USER} (NOSUPERUSER NOBYPASSRLS) and bootstrap runtime roles ===" >&2
PGUSER="$PGUSER" PGDATABASE="$REHEARSAL_DB" \
  G5_MIGRATOR_DB_USER="$G5_MIGRATOR_DB_USER" G5_MIGRATOR_DB_PASSWORD="$G5_MIGRATOR_DB_PASSWORD" \
  bash scripts/ci-provision-migrator.sh >/dev/null
PGUSER="$G5_MIGRATOR_DB_USER" PGPASSWORD="$G5_MIGRATOR_DB_PASSWORD" PGDATABASE="$REHEARSAL_DB" \
  G5_RUNTIME_SYSTEM_DB_PASSWORD="$G5_RUNTIME_SYSTEM_DB_PASSWORD" \
  G5_RUNTIME_TENANT_DB_PASSWORD="$G5_RUNTIME_TENANT_DB_PASSWORD" \
  G5_AUTHZ_WRITER_DB_PASSWORD="$G5_AUTHZ_WRITER_DB_PASSWORD" \
  bash scripts/bootstrap-runtime-db-users.sh >/dev/null

echo "=== 3. Apply ${BASE_REF} migrations as ${G5_MIGRATOR_DB_USER} ===" >&2
base_dir="${work_dir}/base"
mkdir -p "$base_dir"
git archive "$BASE_REF" backend/g5-api/src/db/migrations | tar -x -C "$base_dir"
DATABASE_URL="$migrator_url" G5_MIGRATION_SOURCE="${base_dir}/backend/g5-api/src/db/migrations" "$G5_DB_MIGRATE_BIN"

echo "=== 4. Load rows from ${ROWS_SQL} as ${PGUSER} ===" >&2
if grep -qiE '^(COPY|INSERT INTO) +("?public"?\.)?"?_sqlx_migrations"?[ (]' "$ROWS_SQL"; then
  echo "ERROR: ${ROWS_SQL} carries _sqlx_migrations rows; step 3 already wrote the ledger for ${BASE_REF}." >&2
  echo "       Re-dump with: pg_dump --data-only --exclude-table-data=public._sqlx_migrations ..." >&2
  exit 1
fi
psql -U "$PGUSER" -d "$REHEARSAL_DB" -v ON_ERROR_STOP=1 -q -1 -f "$ROWS_SQL"

apply_head() {
  DATABASE_URL="$migrator_url" G5_MIGRATION_SOURCE="$HEAD_MIGRATIONS" "$G5_DB_MIGRATE_BIN"
}

echo "=== 5. Apply ${HEAD_MIGRATIONS} as ${G5_MIGRATOR_DB_USER} ===" >&2
apply_head
echo "=== 5b. Rerun to prove the no-op path ===" >&2
apply_head

echo "OK: ${BASE_REF} -> ${HEAD_MIGRATIONS} rehearsed with rows as ${G5_MIGRATOR_DB_USER} on ${REHEARSAL_DB}" >&2
