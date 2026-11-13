#!/usr/bin/env bash
set -euo pipefail

: "${PGHOST:=localhost}"
: "${PGPORT:=5432}"
: "${PGUSER:=g5}"
: "${PGDATABASE:=g5_test}"
: "${G5_RUNTIME_SYSTEM_DB_USER:=g5_runtime_system}"
: "${G5_RUNTIME_SYSTEM_DB_PASSWORD:=g5_system_test}"
: "${G5_RUNTIME_TENANT_DB_USER:=g5_runtime_tenant}"
: "${G5_RUNTIME_TENANT_DB_PASSWORD:=g5_tenant_test}"
: "${G5_AUTHZ_WRITER_DB_PASSWORD:=g5_authz_test}"

psql \
  -v ON_ERROR_STOP=1 \
  -v runtime_system_user="$G5_RUNTIME_SYSTEM_DB_USER" \
  -v runtime_system_password="$G5_RUNTIME_SYSTEM_DB_PASSWORD" \
  -v runtime_tenant_user="$G5_RUNTIME_TENANT_DB_USER" \
  -v runtime_tenant_password="$G5_RUNTIME_TENANT_DB_PASSWORD" \
  -v authz_writer_password="$G5_AUTHZ_WRITER_DB_PASSWORD" \
  -h "$PGHOST" \
  -p "$PGPORT" \
  -U "$PGUSER" \
  -d "$PGDATABASE" <<'SQL'
BEGIN;
SELECT pg_advisory_xact_lock(640517824501);
SELECT set_config('g5.runtime_system_user', :'runtime_system_user', false);
SELECT set_config('g5.runtime_system_password', :'runtime_system_password', false);
SELECT set_config('g5.runtime_tenant_user', :'runtime_tenant_user', false);
SELECT set_config('g5.runtime_tenant_password', :'runtime_tenant_password', false);
SELECT set_config('g5.authz_writer_password', :'authz_writer_password', false);

DO $bootstrap$
DECLARE
    runtime_system_user text := current_setting('g5.runtime_system_user');
    runtime_system_password text := current_setting('g5.runtime_system_password');
    runtime_tenant_user text := current_setting('g5.runtime_tenant_user');
    runtime_tenant_password text := current_setting('g5.runtime_tenant_password');
    authz_writer_password text := current_setting('g5.authz_writer_password');
    bypass_role text;
BEGIN
    -- A fresh database has not run the baseline migration yet. Create the
    -- NOLOGIN grant roles here so the migrator can receive the SET-capable
    -- memberships that the baseline needs while assigning function owners.
    -- The baseline repeats these guards, so this remains idempotent for
    -- existing environments.
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'g5_app') THEN
        CREATE ROLE g5_app NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'g5_tenant') THEN
        CREATE ROLE g5_tenant NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authz_writer') THEN
        CREATE ROLE authz_writer NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authz_ctx_writer') THEN
        CREATE ROLE authz_ctx_writer NOLOGIN;
    END IF;

    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = runtime_system_user) THEN
        EXECUTE format(
            'CREATE ROLE %I LOGIN PASSWORD %L INHERIT',
            runtime_system_user,
            runtime_system_password
        );
    ELSE
        EXECUTE format(
            'ALTER ROLE %I WITH LOGIN PASSWORD %L INHERIT',
            runtime_system_user,
            runtime_system_password
        );
    END IF;

    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = runtime_tenant_user) THEN
        EXECUTE format(
            'CREATE ROLE %I LOGIN PASSWORD %L INHERIT',
            runtime_tenant_user,
            runtime_tenant_password
        );
    ELSE
        EXECUTE format(
            'ALTER ROLE %I WITH LOGIN PASSWORD %L INHERIT',
            runtime_tenant_user,
            runtime_tenant_password
        );
    END IF;

    EXECUTE format('ALTER ROLE %I NOCREATEDB NOCREATEROLE', runtime_system_user);
    EXECUTE format('ALTER ROLE %I NOCREATEDB NOCREATEROLE', runtime_tenant_user);
    EXECUTE 'ALTER ROLE authz_writer WITH LOGIN PASSWORD '
        || quote_literal(authz_writer_password)
        || ' INHERIT';
    EXECUTE 'ALTER ROLE authz_writer NOCREATEDB NOCREATEROLE';
    EXECUTE 'ALTER ROLE authz_ctx_writer NOLOGIN NOCREATEDB NOCREATEROLE';
    -- The runtime and authz roles must not bypass row-level security.
    -- Clearing BYPASSRLS requires BYPASSRLS on PostgreSQL 16+ (Cloud SQL
    -- executes it for cloudsqlsuperuser members; plain PostgreSQL and the
    -- NOBYPASSRLS CI migrator do not have it), so only alter a role that
    -- actually carries the attribute: a fresh CREATE ROLE never does, and a
    -- role that carries it and cannot be altered fails loudly here.
    FOR bypass_role IN
        SELECT rolname FROM pg_roles
        WHERE rolbypassrls
          AND rolname IN (runtime_system_user, runtime_tenant_user,
                          'authz_writer', 'authz_ctx_writer')
    LOOP
        EXECUTE format('ALTER ROLE %I NOBYPASSRLS', bypass_role);
    END LOOP;
    EXECUTE 'ALTER ROLE g5_app NOLOGIN';
    EXECUTE 'ALTER ROLE g5_tenant NOLOGIN';

    -- Cloud SQL API-created users inherit cloudsqlsuperuser. The runtime
    -- authz roles must not retain that membership after bootstrap.
    IF EXISTS (
        SELECT 1
        FROM pg_auth_members memberships
        JOIN pg_roles granted_role ON granted_role.oid = memberships.roleid
        JOIN pg_roles member_role ON member_role.oid = memberships.member
        WHERE granted_role.rolname = 'cloudsqlsuperuser'
          AND member_role.rolname = 'authz_writer'
    ) THEN
        EXECUTE 'REVOKE cloudsqlsuperuser FROM authz_writer';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM pg_auth_members memberships
        JOIN pg_roles granted_role ON granted_role.oid = memberships.roleid
        JOIN pg_roles member_role ON member_role.oid = memberships.member
        WHERE granted_role.rolname = 'cloudsqlsuperuser'
          AND member_role.rolname = 'authz_ctx_writer'
    ) THEN
        EXECUTE 'REVOKE cloudsqlsuperuser FROM authz_ctx_writer';
    END IF;

    EXECUTE 'GRANT authz_ctx_writer TO current_user';
    EXECUTE 'GRANT authz_writer TO current_user';

    EXECUTE format('REVOKE g5_tenant FROM %I', runtime_system_user);
    EXECUTE format('GRANT g5_app TO %I', runtime_system_user);
    EXECUTE format('REVOKE g5_app FROM %I', runtime_tenant_user);
    EXECUTE format('GRANT g5_tenant TO %I', runtime_tenant_user);
END
$bootstrap$;

SELECT
    rolname,
    rolcanlogin,
    rolsuper,
    rolbypassrls
FROM pg_roles
WHERE rolname IN (
    current_setting('g5.runtime_system_user'),
    current_setting('g5.runtime_tenant_user'),
    'authz_writer',
    'authz_ctx_writer',
    'g5_app',
    'g5_tenant'
)
ORDER BY rolname;
COMMIT;
SQL
