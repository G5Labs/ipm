#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  rollback-sqlx-migration.sh --version N --description TEXT --rollback-sql FILE [options] [--execute]

Options:
  --context NAME        Kubernetes context. Default: current context.
  --namespace NAME      Kubernetes namespace. Default: g5.
  --secret-name NAME    Secret containing the admin DB URL. Default: g5-api-secrets.
  --secret-key KEY      Secret key for the admin DB URL. Default: G5_ADMIN_DATABASE_URL.
  --version N           Migration version for custom rollback.
  --description TEXT    Migration description for custom rollback.
  --rollback-sql FILE   SQL file containing custom inverse DDL/DML.
  --execute             Execute. Without this flag, print SQL and exit.
  --help                Show this help.

The script runs SQL through an ephemeral, digest-pinned PostgreSQL 17 Alpine pod
and reads the database URL from a Kubernetes secret. It is dry-run by default.
EOF
}

context=""
namespace="g5"
secret_name="g5-api-secrets"
secret_key="G5_ADMIN_DATABASE_URL"
version=""
description=""
rollback_sql_file=""
execute=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --context)
      context="${2:?--context requires a value}"
      shift 2
      ;;
    --namespace)
      namespace="${2:?--namespace requires a value}"
      shift 2
      ;;
    --secret-name)
      secret_name="${2:?--secret-name requires a value}"
      shift 2
      ;;
    --secret-key)
      secret_key="${2:?--secret-key requires a value}"
      shift 2
      ;;
    --version)
      version="${2:?--version requires a value}"
      shift 2
      ;;
    --description)
      description="${2:?--description requires a value}"
      shift 2
      ;;
    --rollback-sql)
      rollback_sql_file="${2:?--rollback-sql requires a value}"
      shift 2
      ;;
    --execute)
      execute=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 2
  fi
}

quote_sql_literal() {
  printf "%s" "$1" | sed "s/'/''/g"
}

emit_custom_sql() {
  local escaped_description
  escaped_description="$(quote_sql_literal "$description")"

  cat <<SQL
BEGIN;
SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '60s';

DO \$\$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM public._sqlx_migrations
        WHERE version = ${version}
          AND description = '${escaped_description}'
          AND success = true
    ) THEN
        RAISE EXCEPTION 'Refusing migration rollback: expected successful migration row not found';
    END IF;
END \$\$;

SQL

  cat "$rollback_sql_file"

  cat <<SQL

DO \$\$
DECLARE
    deleted_count integer;
BEGIN
    DELETE FROM public._sqlx_migrations
    WHERE version = ${version}
      AND description = '${escaped_description}'
      AND success = true;

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    IF deleted_count != 1 THEN
        RAISE EXCEPTION 'Refusing migration rollback: expected to delete exactly one migration row, deleted %', deleted_count;
    END IF;
END \$\$;

COMMIT;

SELECT 'migration_${version}', COALESCE((SELECT version::text || '|' || description || '|' || success::text FROM public._sqlx_migrations WHERE version = ${version}), 'absent');
SELECT 'top_migration', version::text || '|' || description || '|' || success::text FROM public._sqlx_migrations ORDER BY version DESC LIMIT 1;
SQL
}

require_command kubectl

if [[ -z "$version" || -z "$description" || -z "$rollback_sql_file" ]]; then
  echo "Rollback requires --version, --description, and --rollback-sql." >&2
  usage >&2
  exit 2
fi
if [[ ! "$version" =~ ^[0-9]+$ ]]; then
  echo "--version must be an integer." >&2
  exit 2
fi
if [[ ! -f "$rollback_sql_file" ]]; then
  echo "Rollback SQL file not found: $rollback_sql_file" >&2
  exit 2
fi

sql_file="$(mktemp "${TMPDIR:-/tmp}/g5-migration-rollback.XXXXXX.sql")"
trap 'rm -f "$sql_file"' EXIT

emit_custom_sql > "$sql_file"

echo "Prepared rollback for migration ${version} (${description})."

if [[ "$execute" != "1" ]]; then
  echo
  echo "Dry run only. Review the SQL below, then rerun with --execute."
  echo "--------------------------------------------------------------------------------"
  cat "$sql_file"
  exit 0
fi

pod_name="g5-migration-rollback-${version}-$(date +%s)"
overrides="$(printf '{"apiVersion":"v1","spec":{"containers":[{"name":"pg","image":"postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b","stdin":true,"stdinOnce":true,"command":["sh","-lc","psql \\"$DATABASE_URL\\" -v ON_ERROR_STOP=1 -At -f -"],"env":[{"name":"DATABASE_URL","valueFrom":{"secretKeyRef":{"name":"%s","key":"%s"}}}]}]}}' "$secret_name" "$secret_key")"

kubectl_args=(run "$pod_name")
if [[ -n "$context" ]]; then
  kubectl_args+=(--context="$context")
fi
kubectl_args+=(
  -n "$namespace"
  -i
  --rm
  --restart=Never
  --image=postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b
  --override-type=merge
  --overrides="$overrides"
)

echo "Executing rollback through pod ${pod_name} in namespace ${namespace}."
kubectl "${kubectl_args[@]}" < "$sql_file"
