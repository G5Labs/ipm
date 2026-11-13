#!/usr/bin/env bash
# Provision a NON-SUPERUSER CI migrator that mirrors the Cloud SQL admin role.
#
# CD runs migrations as the Cloud SQL admin (G5_ADMIN_DATABASE_URL). That user
# is a member of `cloudsqlsuperuser`: it has CREATEROLE/CREATEDB, it is NOT a
# PostgreSQL superuser, and it does NOT have BYPASSRLS (verified on the QA
# instance on 2026-09-22: pg_roles.rolbypassrls = false for `g5`). A superuser
# can `SET ROLE` to any role, reassign ownership implicitly and read every row
# of a FORCE ROW LEVEL SECURITY table, so running CI migrations as the
# stock-image superuser `g5` silently masks two classes of migration bug:
#   1. a migration that needs an explicit role membership to
#      `ALTER ... OWNER TO` / `SET ROLE` (e.g. the `authz_lookup_definer`-owned
#      SECURITY DEFINER functions in the baseline migration) — passes in CI,
#      fails the live deploy with `must be able to SET ROLE "<role>"`;
#   2. a migration whose UPDATE/INSERT/DELETE targets a FORCE RLS tenant table:
#      without BYPASSRLS and without an org context the statement sees ZERO
#      rows, while the ADD CONSTRAINT / SET NOT NULL that follows validates the
#      real heap — passes in CI, fails the live deploy with "constraint ... is
#      violated by some row" (migration 008 on QA, deploy build 34f334f2).
#
# The primary db-migrate CI step runs as `g5_migrator` (provisioned here) so
# both classes fail in CI rather than only on CD. We match the Cloud SQL admin:
#   - attributes: NOSUPERUSER CREATEDB CREATEROLE NOBYPASSRLS
#   - extensions pre-created (a non-superuser cannot CREATE EXTENSION for
#     superuser-only extensions; `IF NOT EXISTS` then makes the baseline's
#     CREATE EXTENSION a no-op)
#   - ADMIN-capable membership in authz_ctx_writer / authz_writer so this
#     disposable CI role can model the Cloud SQL admin's ability to restrict
#     API-created roles before the baseline runs.
#   - `authz_lookup_definer` (the NOLOGIN BYPASSRLS owner of the SECURITY
#     DEFINER lookups) pre-created by the superuser, with an ADMIN-only
#     membership row for the migrator. Plain PostgreSQL 16+ refuses
#     `CREATE ROLE ... BYPASSRLS` from a role without BYPASSRLS and refuses a
#     membership GRANT from a role without ADMIN OPTION; Cloud SQL executes
#     both on behalf of cloudsqlsuperuser members (its grants show grantor
#     `cloudsqladmin`), and the role is cluster-level there, so it already
#     exists for every database reset. The ADMIN-only row (inherit=false,
#     set=false) has the shape of the PG16 automatic creator membership that
#     the baseline's snapshot/restore logic and ci-assert-migrator-revoke.sh
#     already tolerate; the migrations still take and hand back their
#     temporary SET-capable membership exactly as on Cloud SQL.
#
# This lives in a script file (not inline cloudbuild YAML) so the PL/pgSQL
# dollar-quoting is not mangled by Cloud Build `$$` -> `$` substitution. See
# experience nonsuperuser_migrator_set_role_owner_reassign.md and
# cloudbuild_unescaped_dollar_substitution.md.
set -euo pipefail

: "${PGHOST:=postgres}"
: "${PGPORT:=5432}"
: "${PGUSER:=g5}"
: "${PGDATABASE:=g5_test}"
: "${G5_MIGRATOR_DB_USER:=g5_migrator}"
: "${G5_MIGRATOR_DB_PASSWORD:=g5_test}"

psql \
  -v ON_ERROR_STOP=1 \
  -v migrator_user="$G5_MIGRATOR_DB_USER" \
  -v migrator_password="$G5_MIGRATOR_DB_PASSWORD" \
  -h "$PGHOST" \
  -p "$PGPORT" \
  -U "$PGUSER" \
  -d "$PGDATABASE" <<'SQL'
SELECT set_config('g5.migrator_user', :'migrator_user', false);
SELECT set_config('g5.migrator_password', :'migrator_password', false);

DO $prov$
DECLARE
    migrator_user text := current_setting('g5.migrator_user');
    migrator_password text := current_setting('g5.migrator_password');
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = migrator_user) THEN
        EXECUTE format(
            'CREATE ROLE %I LOGIN PASSWORD %L NOSUPERUSER CREATEDB CREATEROLE NOBYPASSRLS',
            migrator_user, migrator_password
        );
    ELSE
        EXECUTE format(
            'ALTER ROLE %I WITH LOGIN PASSWORD %L NOSUPERUSER CREATEDB CREATEROLE NOBYPASSRLS',
            migrator_user, migrator_password
        );
    END IF;

    -- The authz helper roles the baseline SET ROLEs to. The baseline's role
    -- provisioning is idempotent against these; pre-creating them lets us grant
    -- the migrator the membership CD provides out-of-band before the migrate
    -- step runs.
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'authz_ctx_writer') THEN
        CREATE ROLE authz_ctx_writer NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'authz_writer') THEN
        CREATE ROLE authz_writer NOLOGIN;
    END IF;
    -- The BYPASSRLS definer role: created here by the superuser because a
    -- NOBYPASSRLS migrator cannot create it (see header). Cluster-level, like
    -- on Cloud SQL, so it survives database drops in the same container.
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'authz_lookup_definer') THEN
        CREATE ROLE authz_lookup_definer WITH BYPASSRLS NOLOGIN;
    END IF;
END
$prov$;

-- Superuser-sensitive extensions, pre-created so the baseline's
-- CREATE EXTENSION IF NOT EXISTS becomes a no-op for the non-superuser migrator.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Object-creation privileges in the primary CI database + the out-of-band
-- authz role memberships (re-granting is idempotent).
DO $grants$
DECLARE
    migrator_user text := current_setting('g5.migrator_user');
BEGIN
    EXECUTE format('GRANT ALL ON DATABASE %I TO %I', current_database(), migrator_user);
    EXECUTE format('GRANT ALL ON SCHEMA public TO %I', migrator_user);
    EXECUTE format('GRANT authz_ctx_writer TO %I WITH ADMIN OPTION', migrator_user);
    EXECUTE format('GRANT authz_writer TO %I WITH ADMIN OPTION', migrator_user);
    -- ADMIN-only (no INHERIT, no SET): lets the migrator grant itself the
    -- temporary SET-capable membership the baseline and migration 008 take,
    -- without giving it a standing SET ROLE path to the BYPASSRLS owner.
    EXECUTE format(
        'GRANT authz_lookup_definer TO %I WITH ADMIN TRUE, INHERIT FALSE, SET FALSE',
        migrator_user
    );
END
$grants$;

SELECT rolname, rolsuper, rolcreaterole, rolcreatedb, rolbypassrls
FROM pg_roles
WHERE rolname = current_setting('g5.migrator_user');
SQL

echo "Provisioned non-superuser CI migrator: ${G5_MIGRATOR_DB_USER}"
