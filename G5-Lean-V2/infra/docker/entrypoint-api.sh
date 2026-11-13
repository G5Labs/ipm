#!/bin/bash
set -e

# Apply pending migrations from files when an explicit migration URL is present.
if [ -d /app/migrations ] && [ -n "${DATABASE_URL:-}" ]; then
  echo "[entrypoint] Running repair-aware migrations from /app/migrations/ ..."
  G5_MIGRATION_SOURCE=/app/migrations /usr/local/bin/g5-db-migrate
  echo "[entrypoint] Migrations complete."
fi

exec /usr/local/bin/g5-api "$@"
