---
name: migration-definer-data-section
description: A migration that updates or reads rows of an RLS table must run that section as authz_lookup_definer with the 099/102 membership snapshot; replacing authz.* functions also needs a temporary GRANT CREATE ON SCHEMA authz; the migration test's migrator role must own schema authz.
metadata:
  type: project
---

`g5-db-migrate` refuses (MigrationRlsVisibility) any top-level or `DO`-block
statement that reads or writes a populated RLS table outside
`SET ROLE authz_lookup_definer` … `RESET ROLE`. Found 2026-10-23 on migration
108 (workspace rename): the backfill `UPDATE workspaces` and a duplicate-check
`DO` block both tripped it.

**How to apply:**
- Copy the membership snapshot / `GRANT authz_lookup_definer TO SESSION_USER WITH
  SET TRUE` / restore blocks from migration 102 (rename the `g5.*` GUC keys), grant
  the definer the column privileges it needs, and put the data statements between
  `SET ROLE` and `RESET ROLE`. A `CREATE UNIQUE INDEX` is a valid DDL guard that
  needs no `DO` block.
- The definer lost CREATE on schema `authz` after 001/008; to `CREATE OR REPLACE`
  an `authz.*` function under `SET ROLE`, snapshot
  `has_schema_privilege('authz_lookup_definer','authz','CREATE')`, `GRANT USAGE,
  CREATE ON SCHEMA authz TO authz_lookup_definer`, and revoke after `RESET ROLE`
  (migration 099 pattern). Otherwise: `permission denied for schema authz`.
- In a `migration_NNN_*.rs` test, create the migrator role as in test 089
  (`NOSUPERUSER NOBYPASSRLS`, `GRANT authz_lookup_definer … WITH ADMIN TRUE, INHERIT
  FALSE, SET FALSE`) and also `ALTER SCHEMA authz OWNER TO {role}` (test 045) so the
  schema grant works; assert `pg_has_role(current_user,'authz_lookup_definer','SET')`
  is false afterwards. Replay quickly with `docker exec … psql -a -v ON_ERROR_STOP=1`
  on a scratch DB migrated to N-1 by `target/debug/g5-db-migrate` with
  `G5_MIGRATION_SOURCE=<dir of 001..N-1>`.

The 081-style owner exemption (NO FORCE / UPDATE / FORCE) is recognized only when
the **whole file** is that form (`owner_managed_rls_tables` in `g5-api-data/src/db.rs`
returns nothing on any other statement), so a column `ADD COLUMN` plus its backfill
must be two migrations (2026-10-26: 119 DDL, 121 personal backfill).

Related: [[sqlx-migrate-embed-incremental-cache]], [[authz-definer-column-grants]].
