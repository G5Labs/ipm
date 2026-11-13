#!/usr/bin/env bash
# Gap-A regression guard for the baseline's ownership scaffolding (#135
# follow-up; originally guarding migrations 080/081).
#
# The baseline migration temporarily GRANTs the migrator (SESSION_USER) a
# SET-capable membership in the BYPASSRLS role `authz_lookup_definer` so it can
# reassign the SECURITY DEFINER functions' ownership, then REVOKEs that
# membership on the way out (the cleanup DO blocks) — but ONLY when its snapshot
# recorded no pre-existing SET path, i.e. the migrator was not already able to
# SET ROLE to it.
#
# That migrator-self revoke is observable ONLY on a FRESH, NON-SUPERUSER apply:
#   * the rust integration tests run as the stock-image SUPERUSER, where
#     pg_has_role(<superuser>, role,'MEMBER') is unconditionally true, so the
#     snapshot records 'true' and the revoke is intentionally skipped; and
#   * the shared test cluster's `authz_lookup_definer` permanently exists (it
#     owns the functions, so it cannot be dropped), so a fresh-create apply can
#     never be reproduced there, and any re-apply needs ADMIN OPTION — itself a
#     membership row → snapshot 'true' → revoke skipped.
#
# The primary `db-migrate` CI step runs migrations as the NON-SUPERUSER
# `g5_migrator` on the ephemeral, freshly-created Postgres — the one place the
# fresh-create migrator-self revoke actually fires. This script photographs it:
# after that apply, the migrator must retain NO SET-capable (`SET ROLE`-granting)
# membership in `authz_lookup_definer`. A non-SET auto-creator admin row
# (admin_option=true, set_option=false) may remain on PG16+; that does not grant
# SET ROLE and is expected. If a future edit of the baseline drops the cleanup
# REVOKE, the temporary SET-capable row survives and this guard fails the build.
#
# Runs as the admin/superuser (PGUSER=g5) so the pg_auth_members read is
# unconditional. Lives in a script file (not inline cloudbuild YAML) to match
# ci-provision-migrator.sh and avoid Cloud Build `$$`->`$` mangling. See
# experience nonsuperuser_migrator_set_role_owner_reassign.md.
set -euo pipefail

: "${PGHOST:=postgres}"
: "${PGPORT:=5432}"
: "${PGUSER:=g5}"
: "${PGDATABASE:=g5_test}"
: "${G5_MIGRATOR_DB_USER:=g5_migrator}"

# Roles whose temporary SET-capable membership in authz_lookup_definer the
# baseline must have revoked: the migrator itself and authz_ctx_writer.
offenders=$(
  psql \
    -v ON_ERROR_STOP=1 \
    -tAX \
    -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" \
    -v migrator="$G5_MIGRATOR_DB_USER" <<'SQL'
SELECT string_agg(
         g.rolname || ' (set_option=true, admin=' || m.admin_option::text || ')',
         '; '
       )
FROM pg_auth_members m
JOIN pg_roles r ON r.oid = m.roleid
JOIN pg_roles g ON g.oid = m.member
WHERE r.rolname = 'authz_lookup_definer'
  AND g.rolname IN (:'migrator', 'authz_ctx_writer')
  AND m.set_option;
SQL
)

if [ -n "$offenders" ]; then
  echo "FAIL: the baseline migration left a temporary SET-capable membership in authz_lookup_definer" >&2
  echo "      that the cleanup REVOKE should have removed: $offenders" >&2
  echo "      (the migrator-self / authz_ctx_writer key was not handed back)" >&2
  exit 1
fi

echo "OK: no SET-capable membership in authz_lookup_definer remains for ${G5_MIGRATOR_DB_USER} or authz_ctx_writer"
echo "    after the non-superuser migrate — the baseline handed back its temporary keys."
