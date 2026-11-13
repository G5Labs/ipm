# Database migrations

G5 Lean uses the original G5 deployment mechanism: Helm's
`pre-install,pre-upgrade` migration Job runs `g5-db-migrate`, which invokes
the standard SQLx migrator. SQLx records SHA384 checksums in
`_sqlx_migrations` and refuses to run when an applied migration changes.
Keep that validation enabled. Original G5 has a narrowly scoped repair for a
historical version-87 collision; it is not a general checksum bypass and is
not needed here.

## One-time QA baseline transition

QA initially applied `001_lean_baseline.sql` before the Lean schema work was
complete. PRs #35, #44, #46 and #47 completed their baseline edits during a
coordinated deployment pause and were all merged by September 16, 2026.
That exception is closed. No subsequent PR may edit the baseline.

The QA pause/reset instructions in this section record that historical transition;
they do not direct a new PR to pause deployment triggers or reset a database.

Keep both QA main-branch triggers paused until the combined schema is ready.
The worker-image trigger also patches the API's provisioning configuration,
so pausing only the application deploy is insufficient. PR CI stays enabled.
The existing QA release can serve requests throughout this merge window.

The checksum catalog records the combined baseline from main commit
`6d3936c67d307a69c01de4a0c754d5f8c64afa0f`. Validate a fresh database and
bootstrap seed before merging the guard. Finish the reset using the exact
final main commit that includes the guard; do not reset once per PR.
See [the QA reset runbook](qa-database-reset.md).

## Migration policy

New DDL that takes `ACCESS EXCLUSIVE` locks on live tables starts with `SET LOCAL lock_timeout = '5s'`. The runner applies each transactional migration file in its own transaction, so a timeout leaves no ledger row or partial `ALTER`. The Helm migrate Job retries up to three times (`backoffLimit: 3`); after exhausting retries the release fails and is re-run after the blocking transaction ends.

- Existing SQL files and checksum entries are immutable, including whitespace
  and comments. Do not rename/delete them or repair SQLx's ledger to hide a
  mismatch. This rule overrides older implementation-plan task snippets and
  the historical header in the baseline SQL file.
- Add a new migration using the next unused increasing number, initially
  `002_<description>.sql`. Coordinate numbering and rebase before merging.
- Append only the new file's SHA384 entry to
  `backend/g5-api/src/db/migrations/checksums.sha384`, for example:

  ```bash
  cd backend/g5-api/src/db/migrations
  shasum -a 384 002_example.sql >> checksums.sha384
  ```

- Both PR pipelines check version uniqueness and the checksum catalog before
  SQLx runs. The catalog is reviewed policy, not a secret or a substitute for
  review: changing an old SQL file **and** its recorded checksum would defeat
  this check and must be rejected in review. Do not regenerate the whole file.
- **pgvector stays a bootstrap dependency.** `001_lean_baseline.sql` is
  immutable and still does `CREATE EXTENSION vector` plus a
  `search_documents.embedding vector(1536)` column, so replaying migration
  history on a fresh database still requires a Postgres image with pgvector
  available (`pgvector/pgvector:pg17` in Compose and both CI lanes). P8.9's
  `002_remove_workspace_search.sql` drops every application object that used
  pgvector, so no table, column or index in the running schema uses a vector
  type or operator class, and no runtime indexing or embedding code exists.
  Where the extension itself survives (below) its own types and operator classes
  remain installed in the catalog; nothing references them. Do not swap the database image for
  a plain-Postgres one to remove the string: that breaks fresh-install replay
  until an explicit append-only-compatible bootstrap strategy exists.
- **Dropping an extension needs ownership, which the deployment migrator does
  not have.** `scripts/ci-provision-migrator.sh` pre-creates `vector` and
  `pgcrypto` as the superuser precisely because the migrator is `NOSUPERUSER`
  and cannot `CREATE EXTENSION`; Cloud SQL works the same way. A plain
  `DROP EXTENSION vector` therefore fails with `must be owner of extension
  vector` under the real migrator even though it succeeds for a superuser
  locally. Migration 002 attempts the drop and skips it with a notice when the
  role does not own the extension, so a single-role database ends with no
  extension while CI and Cloud SQL keep it installed but unreferenced, for only
  the platform admin to remove. Validate migrations as `g5_migrator`, not as
  the stock superuser, or this class of failure only appears in CI.
- Validate both a fresh install and an upgrade from the previous schema. Both CI lanes build this checkout's `g5-db-migrate` and use it for database setup; raw SQLx does not exercise the production guard or cluster-role lock. The required Rust integration lane runs every API integration test, including populated historical migration fixtures. A data-bearing migration must add or extend a fixture that seeds affected rows before applying the original SQL through the production runner as a non-superuser without BYPASSRLS, verifies resulting data/security, and repeats the migration as a no-op. Fresh CI databases alone cannot prove populated upgrade behavior. Refresh `.sqlx` when query metadata changes.

Run the static checks from the repository root:

```bash
bash scripts/check-migration-version-uniqueness.sh
python3 scripts/check-migration-checksums.py
python3 -m unittest discover -s scripts -p test_migration_checksums.py
```

If SQLx reports `VersionMismatch`, first compare the deployed migration bytes
and ledger with the candidate. Normally restore the original file and express
the change in a new migration. The disposable-QA reset above is a coordinated
one-time operation, not an automatic deployment recovery strategy.

## Migrator row visibility under FORCE ROW LEVEL SECURITY

The deployment migrator is the Cloud SQL admin user `g5`. It is a member of
`cloudsqlsuperuser`, so it can create roles and grant memberships, but it is
**not** a PostgreSQL superuser and it does **not** have `BYPASSRLS` (verified
on `g5-lean-qa-postgres` on 2026-09-22; Cloud SQL cannot grant that attribute
to a non-superuser). Every tenant table carries `FORCE ROW LEVEL SECURITY`
with policies keyed on `authz.current_org_ids()`, which is NULL in a migration
session. The consequences for a migration file:

- DDL is unaffected. `ADD CONSTRAINT ... CHECK`, `SET NOT NULL` and index
  builds validate every row of the heap regardless of RLS.
- `UPDATE`, `INSERT ... SELECT`, `DELETE`, `MERGE` and any `SELECT` used as a
  guard see **zero rows**, whether written at top level or inside a `DO`
  block (which runs immediately in the migration session). They succeed
  silently. A constraint added afterwards then fails with `check constraint
  ... is violated by some row`, which is how migration 008 failed on QA
  (deploy build `34f334f2`) even though its own preflight `DO` block counted
  zero unresolved rows. `TRUNCATE` is the exception: row security does not
  apply to it, so it empties the whole table.
- Local Compose runs migrations as the container superuser and CI's primary
  lane on an empty database, so neither reproduces this by itself.

Rules for a migration that touches rows in a tenant table:

1. Run its data section as the `BYPASSRLS` definer role. Take the temporary
   SET-capable membership exactly as migration 008 does at its top (snapshot
   `pg_auth_members`, then `GRANT authz_lookup_definer TO SESSION_USER WITH
   SET TRUE` when the snapshot had no SET path), then:

   ```sql
   SET ROLE authz_lookup_definer;
   -- UPDATE / INSERT ... SELECT / DELETE on tenant tables, and any
   -- row-count guards that must see every org's rows
   RESET ROLE;
   ```

   `RESET ROLE` must come before any `CREATE` or `ALTER ... OWNER`: objects
   created while the role is set are owned by `authz_lookup_definer`. Restore
   the membership snapshot at the end of the file as 008 does. The definer
   role needs `SELECT`/`UPDATE`/... on the tables it touches; grant and revoke
   those inside the same migration.
2. `g5-db-migrate` refuses to start when a pending migration reads or
   writes a `public` table that has row-level security, physically holds
   rows, and shows the session none of them. A narrow alternative for table-owner remediation is a transactional, straight-line `ALTER TABLE public.<table> NO FORCE ROW LEVEL SECURITY` / data update / `ALTER TABLE public.<table> FORCE ROW LEVEL SECURITY` sequence. The runner recognizes this only for ordinary tables owned directly by its current role, with no inheritance or partition tree, every read/write protected by the corresponding sequence, and FORCE restored before commit. This form accepts the explicit FORCE toggles, UPDATE/DELETE statements, and `SET CONSTRAINTS ALL IMMEDIATE`. Inside the same table’s open NO FORCE scope, it also accepts `ALTER TABLE public.<table>` with only `DROP COLUMN <identifier>` or `DROP CONSTRAINT <identifier>` actions, including comma-separated lists (as in migration 045). Modifiers such as CASCADE, mixed ALTER actions, role changes, DO blocks, transaction control, other DDL, and nontransactional migrations retain the ordinary visibility check. The ALTER locks keep concurrent sessions out until the transaction commits, while RLS remains enabled. The production runner must be exercised against populated tables when using this form.

   The guard otherwise scans top-level statements
   and `DO` bodies, including the `FROM`/`JOIN`/`USING` sources of an
   `INSERT ... SELECT` or `UPDATE ... FROM`, and exempts only the statements
   between `SET [LOCAL] ROLE authz_lookup_definer` and `RESET ROLE` (or a
   `SET ROLE` to another role). The refusal names the migration and the
   table. It never fires for superuser or `BYPASSRLS` sessions, for tables
   without rows, for `TRUNCATE`, or for `CREATE FUNCTION` bodies (those run
   at call time as the caller). It cannot see into dynamic SQL
   (`EXECUTE format(...)`), so keep data statements static. "Holds rows" is
   judged from the heap size, not from statistics, so a restored backup is
   not mistaken for an empty database; a table whose rows were deleted but
   never vacuumed counts as holding rows until `VACUUM` runs.
3. CI's primary `db-migrate` step invokes `g5-db-migrate` as `g5_migrator`, provisioned by `scripts/ci-provision-migrator.sh` with the Cloud SQL admin's real attributes (`NOSUPERUSER CREATEDB CREATEROLE NOBYPASSRLS`). The populated migration 037 integration test also launches this executable, verifies guard refusal without ownership, successful owner remediation, restored FORCE RLS, and repeat no-op. Each new data-bearing migration needs its own representative populated upgrade regression; the existing fixtures cannot predict every future schema/data shape. For an additional rehearsal against target rows, build this checkout's executable and run:

   ```bash
   # Postgres 17 with pgvector on localhost:55432, superuser g5/g5_test.
   # ROWS_SQL is a data-only dump of the target database at BASE_REF's schema,
   # taken WITHOUT the migration ledger (the rehearsal writes its own):
   #   pg_dump --data-only --exclude-table-data=public._sqlx_migrations ...
   # or a fixture that inserts representative rows.
   PGHOST=127.0.0.1 PGPORT=55432 PGUSER=g5 PGPASSWORD=g5_test \
   BASE_REF=origin/main ROWS_SQL=/path/to/rows.sql \
   G5_DB_MIGRATE_BIN=/absolute/path/to/g5-db-migrate \
     scripts/rehearse-migration-upgrade.sh
   ```

   The script migrates a fresh database to `BASE_REF` as `g5_migrator`, loads
   the rows as the superuser, applies this checkout's migrations as
   `g5_migrator` through `G5_DB_MIGRATE_BIN` and reruns them to prove the no-op path. The executable is mandatory for both the base schema and candidate upgrade; the script refuses a missing executable before changing the database.

The same rule already applies to seeding: the platform seed,
`scripts/seed-prod-org.sh`, switches to `g5_app` for its `orgs` step because the
admin user cannot see that table. That seed provisions only the platform
operator organization (`orgs.kind = 'operator'`, migration 122) and its first
operator's invitation; tenant organizations are created afterwards from the
operator console (`/operator/home`) or `POST /api/v1/platform/orgs`.

## Workspace repository cleanup (migrations 039–040)

This release uses a coordinated database reset. Stop old writers and runtimes,
create the empty database with the normal deployment roles and extensions, then
replay every migration through 040. Earlier migrations and their checksums remain
unchanged. Migration 039 refuses populated workspaces; it does not convert old
branch rows or operation payloads. The former P4.1 live mapping command is retired.

The final model stores membership in `workspace_repos`, remote policy in
`remote_integrations`, and current repository versions in Git. Working refs use
`workspaces.name`; Base refs use each repository's `default_branch`. There are no
stored base branches or fork points. See [the implementation plan](data-model-cleanup-plan.md)
for the full cutover and validation sequence.

Before rollout, prove fresh replay and an unchanged no-op rerun as
`g5_migrator`, then run the retained seed and workflow checks against that schema.
Do not apply 039 to a populated QA database or alter an earlier migration to
bypass its checks. Resetting a deployed database is a separate deployment action;
this source change does not perform it.

The shared runner uses a disposable execution connection while retaining SQLx's database lock and cluster-role coordination. SQLx 0.8.6 can return an SQL/checksum error before its normal session-level advisory unlock; closing the execution session prevents that lock or temporary role state from returning to an application pool and blocking retries. A failed transactional migration leaves its data/schema changes and ledger unapplied.

Migration 087 installs `authz.assert_tenant_tables_force_rls()`. It requires both
ENABLE and FORCE RLS on every application table accessible to `g5_tenant`,
including inherited privileges, column-only grants and partitioned tables.
The shared runner checks this after each subsequent migration and on no-op
replays. For transactional migrations, a failure rolls back the schema and
SQLx ledger entry together; nontransactional migrations cannot roll back their
already committed statements. Historical catalogs before 087 remain replayable.
The assertion uncovered ENABLE-only `api_keys` and `worker_tokens` as well as
the three unprotected notification/usage/quota tables. Migration 087 also forces
RLS on these credential tables and moves their pre-context authentication
helpers to `authz_lookup_definer`, with column privileges limited to those
helpers and execution restricted to `g5_app`. The migrator's temporary SET
capability and schema CREATE privilege are restored before commit.

### Immutable 047/048 recovery contexts

The shared runner recognizes only the original SHA384 checksums of migrations 047 and 048 for a `public.worker_hosts` owner context. These historical files omitted an RLS-aware scope around their legacy-inventory backfills; skipping the visibility guard alone would silently skip populated rows. Their SQL and recorded checksums remain unchanged.

The runner locks and qualifies the ordinary, directly owned, enabled/forced-RLS table on its actual execution connection, then temporarily removes FORCE in an outer transaction. SQLx applies the unchanged migration through a nested savepoint and writes its normal ledger row; the runner restores FORCE before the outer commit. Failure/cancellation rolls back data, schema, ledger and RLS changes and disposes of the session. Tenant policies stay enabled, runtime grants remain unchanged, and nonmatching migrations retain ordinary visibility checks. New migrations must author their own explicit RLS-aware data scope.

For populated upgrades and rehearsals, use `g5-db-migrate` (set `G5_DB_MIGRATE_BIN` for the rehearsal script). Raw `sqlx migrate run` does not apply these shared-runner recovery contexts.
