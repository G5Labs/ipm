//! Database infrastructure module for the G5 API server.
//!
//! Responsible for:
//! 1. Connection pool setup — creating and configuring a `sqlx::PgPool`
//!    from the application's `DatabaseConfig`.
//! 2. Migration management — running embedded SQL migrations at startup.
//! 3. Pool lifecycle — providing the pool to the rest of the application
//!    and supporting graceful shutdown.
//! 4. Database lifecycle support shared by the outer API server and tools.
//!
//! This module is initialized after configuration parsing and before the
//! HTTP server, scheduler, or background tasks start. The pool it produces
//! is the single shared database connection resource used by all services,
//! routes, and maintenance tasks in `g5-api`.

use std::path::Path;

use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tracing::{info, warn};

use g5_api_foundation::config::DatabaseConfig;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Database initialization error type.
///
/// Used for pool creation, connectivity verification, and migration errors.
/// These are fatal startup errors — the caller (`main.rs`) is expected to
/// log the error and exit the process.
///
/// Does NOT implement `axum::response::IntoResponse` — database initialization
/// errors are handled in `main.rs`, not returned as HTTP responses. Runtime
/// query errors use the `AppError` type from the `errors` module instead.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Failed to create database pool: {0}")]
    PoolCreation(#[source] sqlx::Error),

    #[error("Database connectivity check failed: {0}")]
    ConnectivityCheck(#[source] sqlx::Error),

    #[error("Migration failed: {0}")]
    Migration(#[source] sqlx::migrate::MigrateError),

    #[error("Migration cluster-role coordination failed: {0}")]
    MigrationCoordination(#[source] sqlx::Error),

    #[error("Migration cluster-role coordination invariant failed: {0}")]
    MigrationCoordinationInvariant(String),

    #[error("Migration row-visibility guard could not inspect the database: {0}")]
    MigrationRlsGuard(#[source] sqlx::Error),

    #[error("Migration row-visibility guard failed: {0}")]
    MigrationRlsVisibility(String),
}

// ---------------------------------------------------------------------------
// Pool initialization
// ---------------------------------------------------------------------------

/// Create and verify a PostgreSQL connection pool from the given configuration.
///
/// # Initialization sequence
///
/// 1. Build pool options from `DatabaseConfig` (max/min connections, timeouts).
/// 2. Connect to Postgres using the configured URL.
/// 3. Verify connectivity by executing `SELECT 1`.
/// 4. Log pool configuration at `info` level with redacted database URL.
/// 5. Return the pool on success, or a `DbError` on failure.
///
/// If any step fails, the function returns an error with a descriptive message.
/// There is no retry logic at the pool initialization level.
pub async fn init_pool(config: &DatabaseConfig) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        // Defence-in-depth: clear any stale RLS session context left on a
        // recycled connection from a prior request.
        //
        // `before_acquire` runs on EVERY checkout of an idle connection from the
        // pool.  Calling `authz.clear_ctx()` ensures the PID-keyed
        // `authz.session_ctx` row is deleted before the new request begins.
        // Without this, a crashed or panicked handler could leave a stale
        // context that the next handler inherits, bypassing RLS.
        //
        // Returns `Ok(true)` → keep the connection.
        .before_acquire(|conn, _meta| {
            Box::pin(async move {
                // Tolerate missing authz schema (pre-migration state).
                // The function won't exist until migrations have run.
                let _ = sqlx::query!("SELECT authz.clear_ctx()")
                    .execute(&mut *conn)
                    .await;
                Ok(true)
            })
        })
        .connect(&config.url)
        .await
        .map_err(DbError::PoolCreation)?;

    // Verify connectivity with a simple query — catches misconfigurations
    // (wrong credentials, unreachable host, wrong database name) at startup
    // rather than at first request time.
    sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(&pool)
        .await
        .map_err(DbError::ConnectivityCheck)?;

    info!(
        max_connections = config.max_connections,
        min_connections = config.min_connections,
        connect_timeout_secs = config.connect_timeout.as_secs(),
        idle_timeout_secs = config.idle_timeout.as_secs(),
        database_url = %redact_url(&config.url),
        "Database pool initialized successfully"
    );

    Ok(pool)
}

// ---------------------------------------------------------------------------
// Migrations
// ---------------------------------------------------------------------------

// PostgreSQL advisory locks are local to one database. Every migrator that may
// mutate cluster-global roles must therefore acquire this key through the same
// shared database, even when the target databases differ.
const CLUSTER_ROLE_ADVISORY_LOCK_KEY: i64 = 640_517_824_501;
const MIGRATION_COORDINATION_DATABASE: &str = "postgres";

/// Run all pending SQL migrations embedded at compile time.
///
/// Uses `sqlx::migrate!()` to embed migration files from the `migrations/`
/// directory. Migrations are executed in lexicographic order by filename.
/// sqlx automatically manages a `_sqlx_migrations` tracking table.
///
/// On success, logs the number of applied migrations (or "no pending
/// migrations" if already up to date). On failure, returns a `DbError`
/// wrapping the sqlx migration error — the caller should abort startup
/// since a partially-migrated database is unsafe to serve.
pub async fn run_migrations(pool: &PgPool) -> Result<(), DbError> {
    let migrator = sqlx::migrate!("../g5-api/src/db/migrations");

    run_migrator(pool, &migrator).await
}

/// Run pending SQL migrations from an explicit runtime source directory.
///
/// This is used by container entrypoints and Helm hooks so the deployed image
/// migrates from `/app/migrations` while sharing the cluster-role coordination
/// lock with API startup.
pub async fn run_migrations_from_path(
    pool: &PgPool,
    source: impl AsRef<Path>,
) -> Result<(), DbError> {
    let migrator = Migrator::new(source.as_ref())
        .await
        .map_err(DbError::Migration)?;

    run_migrator(pool, &migrator).await
}

async fn run_migrator(pool: &PgPool, migrator: &Migrator) -> Result<(), DbError> {
    let mut coordination_connection = acquire_migration_coordination_lock(pool).await?;

    let migration_result = run_migrator_with_coordination_lock(pool, migrator).await;
    let unlock_result = release_migration_coordination_lock(&mut coordination_connection).await;
    let close_result = coordination_connection
        .close()
        .await
        .map_err(DbError::MigrationCoordination);

    if let Err(migration_error) = migration_result {
        if let Err(unlock_error) = unlock_result {
            warn!(
                error = %unlock_error,
                "Failed to explicitly release migration coordination lock after migration error; closing the dedicated connection releases it"
            );
        }
        if let Err(close_error) = close_result {
            warn!(
                error = %close_error,
                "Failed to cleanly close migration coordination connection after migration error"
            );
        }
        return Err(migration_error);
    }

    unlock_result?;
    close_result?;
    Ok(())
}

async fn acquire_migration_coordination_lock(pool: &PgPool) -> Result<PgConnection, DbError> {
    let target_options = pool.connect_options();
    let coordination_options = target_options
        .as_ref()
        .clone()
        .database(MIGRATION_COORDINATION_DATABASE);
    let mut connection = PgConnection::connect_with(&coordination_options)
        .await
        .map_err(DbError::MigrationCoordination)?;

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(CLUSTER_ROLE_ADVISORY_LOCK_KEY)
        .execute(&mut connection)
        .await
        .map_err(DbError::MigrationCoordination)?;

    Ok(connection)
}

async fn release_migration_coordination_lock(connection: &mut PgConnection) -> Result<(), DbError> {
    let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(CLUSTER_ROLE_ADVISORY_LOCK_KEY)
        .fetch_one(&mut *connection)
        .await
        .map_err(DbError::MigrationCoordination)?;

    if !released {
        return Err(DbError::MigrationCoordinationInvariant(
            "dedicated postgres session did not hold the cluster-role advisory lock at release"
                .to_string(),
        ));
    }

    Ok(())
}

async fn run_migrator_with_coordination_lock(
    pool: &PgPool,
    migrator: &Migrator,
) -> Result<(), DbError> {
    let before = get_applied_migration_count(pool).await;

    rls_guard::assert_pending_migrations_can_see_tenant_rows(pool, migrator).await?;

    // SQLx unlocks its session-level advisory lock only on success. Never
    // return a failed or cancelled execution session to the application pool:
    // a retry on a different connection would wait on our own retained lock.
    let mut execution_connection = pool
        .acquire()
        .await
        .map_err(|error| DbError::Migration(error.into()))?
        .detach();
    // SQLx 0.8.6 run(&mut conn) has an Acquire lifetime issue that makes
    // spawned migration futures non-Send; run_direct preserves that boundary.
    let migration_result = migrator
        .run_direct(&mut GuardedMigrationConnection(&mut execution_connection))
        .await;
    let close_result = execution_connection.close().await;
    if let Err(migration_error) = migration_result {
        if let Err(close_error) = close_result {
            warn!(error = %close_error, "Failed to cleanly close migration execution session after migration error");
        }
        return Err(DbError::Migration(migration_error));
    }
    close_result.map_err(|error| DbError::Migration(error.into()))?;

    let after = get_applied_migration_count(pool).await;
    let applied = after.saturating_sub(before);

    if applied > 0 {
        info!(applied_count = applied, "Database migrations applied");
    } else {
        info!("No pending database migrations");
    }

    Ok(())
}

// Preserve SQLx's validation, advisory locking and original ledger writes.
// Only these immutable historical backfills need an owner execution context.
fn historical_owner_context(migration: &sqlx::migrate::Migration) -> bool {
    let checksum: &[u8] = match migration.version {
        47 => &[
            98, 8, 223, 7, 206, 220, 18, 159, 60, 72, 43, 150, 174, 101, 115, 134, 207, 208, 249,
            253, 20, 212, 44, 204, 166, 203, 118, 5, 198, 224, 217, 237, 131, 193, 45, 180, 179, 8,
            25, 252, 239, 168, 214, 185, 197, 236, 172, 54,
        ],
        48 => &[
            205, 126, 211, 159, 200, 236, 212, 157, 135, 232, 204, 8, 10, 141, 89, 65, 206, 28,
            161, 87, 101, 172, 109, 9, 136, 230, 93, 144, 5, 2, 87, 222, 44, 187, 142, 111, 42,
            219, 52, 203, 29, 163, 103, 204, 198, 137, 131, 54,
        ],
        _ => return false,
    };
    !migration.no_tx && migration.checksum.as_ref() == checksum
}

const HISTORICAL_HOST_QUALIFICATION: &str = "SELECT EXISTS (
    SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
    JOIN pg_roles r ON r.oid=c.relowner
    WHERE n.nspname='public' AND c.relname='worker_hosts' AND c.relkind='r'
      AND r.rolname=current_user AND c.relrowsecurity AND c.relforcerowsecurity
      AND NOT EXISTS (SELECT 1 FROM pg_inherits i
                      WHERE i.inhrelid=c.oid OR i.inhparent=c.oid))";

type MigrationFuture<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<T, sqlx::migrate::MigrateError>> + Send + 'a>,
>;

// Historical catalogs predate the invariant installed by migration 087.
// Once present, check it on every apply and even on no-op replays.
async fn tenant_rls_assertion_installed(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT to_regprocedure('authz.assert_tenant_tables_force_rls()') IS NOT NULL",
    )
    .fetch_one(conn)
    .await
}

async fn assert_tenant_tables_force_rls(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT authz.assert_tenant_tables_force_rls()")
        .execute(conn)
        .await?;
    Ok(())
}

struct GuardedMigrationConnection<'a>(&'a mut PgConnection);

impl sqlx::migrate::Migrate for GuardedMigrationConnection<'_> {
    fn ensure_migrations_table(&mut self) -> MigrationFuture<'_, ()> {
        self.0.ensure_migrations_table()
    }

    fn dirty_version(&mut self) -> MigrationFuture<'_, Option<i64>> {
        self.0.dirty_version()
    }

    fn list_applied_migrations(
        &mut self,
    ) -> MigrationFuture<'_, Vec<sqlx::migrate::AppliedMigration>> {
        self.0.list_applied_migrations()
    }

    fn lock(&mut self) -> MigrationFuture<'_, ()> {
        self.0.lock()
    }

    fn unlock(&mut self) -> MigrationFuture<'_, ()> {
        Box::pin(async move {
            if tenant_rls_assertion_installed(self.0).await? {
                assert_tenant_tables_force_rls(self.0).await?;
            }
            self.0.unlock().await
        })
    }

    fn apply<'e: 'm, 'm>(
        &'e mut self,
        migration: &'m sqlx::migrate::Migration,
    ) -> MigrationFuture<'m, std::time::Duration> {
        Box::pin(async move {
            if !historical_owner_context(migration) {
                if tenant_rls_assertion_installed(self.0).await? {
                    if migration.no_tx {
                        let elapsed = self.0.apply(migration).await?;
                        assert_tenant_tables_force_rls(self.0).await?;
                        return Ok(elapsed);
                    }
                    // Keep both the schema change and SQLx's ledger entry
                    // uncommitted until the tenant-table invariant passes.
                    let mut tx = self.0.begin().await?;
                    let elapsed = tx.apply(migration).await?;
                    assert_tenant_tables_force_rls(&mut tx)
                        .await
                        .map_err(|error| {
                            sqlx::migrate::MigrateError::ExecuteMigration(error, migration.version)
                        })?;
                    tx.commit().await?;
                    return Ok(elapsed);
                }
                return self.0.apply(migration).await;
            }
            let mut tx = self.0.begin().await?;
            // Hold ACCESS EXCLUSIVE through restoration and commit: concurrent
            // tenants cannot observe the temporary owner visibility window.
            sqlx::query("LOCK TABLE public.worker_hosts IN ACCESS EXCLUSIVE MODE")
                .execute(&mut *tx)
                .await?;
            let qualified: bool = sqlx::query_scalar(HISTORICAL_HOST_QUALIFICATION)
                .fetch_one(&mut *tx)
                .await?;
            if !qualified {
                return Err(sqlx::migrate::MigrateError::ExecuteMigration(
                    sqlx::Error::Protocol("historical worker_hosts context requires direct ownership of an ordinary, non-inherited table with enabled and forced RLS".into()),
                    migration.version,
                ));
            }
            sqlx::query("ALTER TABLE public.worker_hosts NO FORCE ROW LEVEL SECURITY")
                .execute(&mut *tx)
                .await?;
            // PgConnection::apply opens a nested savepoint. The unchanged SQL,
            // original-checksum ledger row and timing update remain in this
            // outer transaction until FORCE is restored. Do not rewrite SQL.
            let elapsed = tx.apply(migration).await?;
            sqlx::query("ALTER TABLE public.worker_hosts FORCE ROW LEVEL SECURITY")
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(elapsed)
        })
    }

    fn revert<'e: 'm, 'm>(
        &'e mut self,
        migration: &'m sqlx::migrate::Migration,
    ) -> MigrationFuture<'m, std::time::Duration> {
        self.0.revert(migration)
    }
}

/// Count the number of applied migrations by querying the sqlx migrations table.
/// Returns 0 if the table doesn't exist yet (first run).
async fn get_applied_migration_count(pool: &PgPool) -> u64 {
    sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM _sqlx_migrations WHERE success = true"#)
        .fetch_one(pool)
        .await
        .unwrap_or(0) as u64
}

// ---------------------------------------------------------------------------
// Row-visibility guard for data-bearing migrations
// ---------------------------------------------------------------------------

/// Fail fast when a pending migration would run data statements blind.
///
/// The deployment migrator (the Cloud SQL admin user) is neither a superuser
/// nor `BYPASSRLS`, and the tenant tables carry `FORCE ROW LEVEL SECURITY`
/// with policies keyed on `authz.current_org_ids()`, which is NULL for a
/// migration session. In that combination every `UPDATE`, `INSERT ... SELECT`,
/// `DELETE` and `SELECT` inside a migration sees zero rows, while the
/// `ADD CONSTRAINT` / `SET NOT NULL` that follows validates the real heap.
/// Migration 008 failed exactly this way on QA with the opaque error
/// `check constraint ... is violated by some row`.
///
/// CI cannot catch this on a fresh database, so this guard turns the
/// condition into a clear pre-flight error. For every pending migration it
/// collects the `public` tables that the migration's own session reads or
/// writes: top-level `UPDATE`/`INSERT`/`DELETE`/`MERGE` statements and their
/// `FROM`/`JOIN`/`USING` sources, `SELECT`s, and the statements inside `DO`
/// blocks (which execute immediately, unlike `CREATE FUNCTION` bodies).
/// Statements that run while the file has switched to the `BYPASSRLS` definer
/// (`SET [LOCAL] ROLE authz_lookup_definer` ... `RESET ROLE`) are trusted. If
/// any collected table has row-level security, physically holds rows, and the
/// session can see none of them, the run aborts before any migration executes.
/// Superuser and `BYPASSRLS` sessions are never affected. A transactional,
/// straight-line NO FORCE/data/FORCE sequence is also recognized for ordinary
/// tables owned directly by the migration role; no role or control-flow
/// changes, inherited tables, or unprotected data references are inferred safe.
///
/// Known limits: dynamic SQL (`EXECUTE format(...)`) is not inspected, and
/// `TRUNCATE` is not checked because row security does not apply to it.
mod rls_guard {
    use std::collections::BTreeSet;

    use sqlx::migrate::Migrator;
    use sqlx::PgPool;
    use tracing::info;

    use super::DbError;

    /// Role the convention uses for RLS-aware data sections. Statements that
    /// run while this role is set are exempt from the visibility check.
    const RLS_AWARE_ROLE: &str = "authz_lookup_definer";

    /// Placeholders for characters inside a double-quoted identifier that the
    /// tokenizer would otherwise treat as separators; restored by `unquote`.
    const QUOTED: [(char, char); 5] = [
        (' ', '\u{1}'),
        ('(', '\u{2}'),
        (')', '\u{3}'),
        (',', '\u{4}'),
        (';', '\u{5}'),
    ];

    /// Run the guard for every migration that is not yet in the ledger.
    pub(super) async fn assert_pending_migrations_can_see_tenant_rows(
        pool: &PgPool,
        migrator: &Migrator,
    ) -> Result<(), DbError> {
        let session_bypasses_rls: bool = sqlx::query_scalar(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(pool)
        .await
        .map_err(DbError::MigrationRlsGuard)?;
        if session_bypasses_rls {
            return Ok(());
        }

        // The ledger does not exist before the first migration; treat that as
        // "nothing applied" rather than a guard failure.
        let applied: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
                .fetch_all(pool)
                .await
                .unwrap_or_default();

        let mut findings = Vec::new();
        for migration in migrator.iter() {
            if applied.contains(&migration.version) {
                continue;
            }
            let owner_managed = if migration.no_tx {
                BTreeSet::new()
            } else {
                owner_managed_rls_tables(&migration.sql)
            };
            for table in blind_data_tables(&migration.sql) {
                let Some(state) = table_state(pool, &table).await? else {
                    // Created by a pending migration, a CTE or alias name, or
                    // not a plain table.
                    continue;
                };
                if !state.row_security || !state.may_have_rows {
                    continue;
                }
                if table == "worker_hosts" && super::historical_owner_context(migration) {
                    let qualified: bool = sqlx::query_scalar(super::HISTORICAL_HOST_QUALIFICATION)
                        .fetch_one(pool)
                        .await
                        .map_err(DbError::MigrationRlsGuard)?;
                    if qualified {
                        continue;
                    }
                }
                if owner_managed.contains(&table) && owns_plain_table(pool, &table).await? {
                    continue;
                }
                let visible_rows: i64 = sqlx::query_scalar(&format!(
                    "SELECT count(*) FROM public.{}",
                    quote_ident(&table)
                ))
                .fetch_one(pool)
                .await
                .map_err(DbError::MigrationRlsGuard)?;
                if visible_rows == 0 {
                    findings.push(format!(
                        "migration {} ({}) reads or modifies public.{}, which has row-level security and holds rows, none visible to this session",
                        migration.version, migration.description, table
                    ));
                } else {
                    info!(
                        version = migration.version,
                        table, "Row-visibility guard: rows visible, continuing"
                    );
                }
            }
        }

        if findings.is_empty() {
            return Ok(());
        }
        Err(DbError::MigrationRlsVisibility(format!(
            "the migrator role is not BYPASSRLS and has no org context, so the data statements below would silently see zero rows while later constraint validation sees every row. Run the data section under `SET ROLE {RLS_AWARE_ROLE}` ... `RESET ROLE` (see docs/database-migrations.md) or migrate as a BYPASSRLS role. If the table is truly empty but was never vacuumed since its rows were deleted, VACUUM it. {}",
            findings.join("; ")
        )))
    }

    /// What the guard needs to know about one `public` table.
    struct TableState {
        /// Row-level security is enabled or forced on the table or a partition.
        row_security: bool,
        /// The table, or one of its partitions, physically holds rows.
        may_have_rows: bool,
    }

    /// Inspect a `public` table and its partitions.
    ///
    /// "Holds rows" is decided from the relation's physical size plus the
    /// catalog and cumulative-statistics row counters, never from a single
    /// estimate: `pg_stat_user_tables.n_live_tup` is reset by crash recovery
    /// and point-in-time recovery (a Cloud SQL restore or failover) and
    /// `pg_class.reltuples` is -1 until the first `ANALYZE`, but a heap with
    /// rows is never zero bytes. A table whose rows were all deleted but not
    /// yet vacuumed therefore counts as holding rows; the guard fails closed
    /// and its message says how to clear that.
    async fn table_state(pool: &PgPool, table: &str) -> Result<Option<TableState>, DbError> {
        let row: (Option<bool>, Option<bool>) = sqlx::query_as(
            "WITH RECURSIVE target AS ( \
                 SELECT c.oid FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = 'public' AND c.relname = $1 AND c.relkind IN ('r', 'p') \
             ), rels AS ( \
                 SELECT oid FROM target \
                 UNION \
                 SELECT i.inhrelid FROM pg_inherits i JOIN rels ON rels.oid = i.inhparent \
             ) \
             SELECT bool_or(c.relrowsecurity OR c.relforcerowsecurity), \
                    bool_or(c.relkind = 'r' AND ( \
                        pg_relation_size(c.oid) > 0 \
                        OR c.reltuples > 0 \
                        OR COALESCE(s.n_live_tup, 0) > 0)) \
             FROM rels \
             JOIN pg_class c ON c.oid = rels.oid \
             LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .map_err(DbError::MigrationRlsGuard)?;
        Ok(match row {
            (Some(row_security), Some(may_have_rows)) => Some(TableState {
                row_security,
                may_have_rows,
            }),
            _ => None,
        })
    }

    /// Double-quote an identifier for interpolation into SQL text.
    fn quote_ident(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    /// Names of `public` tables that the migration's own session would read
    /// or write without the `BYPASSRLS` definer role.
    ///
    /// The result may include alias names, PL/pgSQL variables or tables the
    /// migration creates; the caller drops names that are not existing
    /// `public` tables. Quoted identifiers keep their case, unquoted ones are
    /// folded to lower case as PostgreSQL does.
    pub(super) fn blind_data_tables(sql: &str) -> BTreeSet<String> {
        let tokens = without_atomic_function_bodies(tokens(&strip_for_scan(sql)));
        let mut tables = BTreeSet::new();
        let mut elevated = false;
        for statement in tokens.split(|token| token == ";") {
            // A PL/pgSQL block opener shares its chunk with the first
            // statement of the block (`BEGIN RESET ROLE`).
            let statement = match statement.first() {
                Some(first) if first.eq_ignore_ascii_case("BEGIN") => &statement[1..],
                _ => statement,
            };
            if statement.is_empty() {
                continue;
            }
            if let Some(role) = set_role_target(statement) {
                elevated = role.eq_ignore_ascii_case(RLS_AWARE_ROLE);
                continue;
            }
            if is_reset_role(statement) {
                elevated = false;
                continue;
            }
            if elevated || !is_data_statement(statement) {
                continue;
            }
            let ctes = cte_definitions(statement);
            for reference in referenced_tables(statement) {
                // A CTE shadows an unqualified name where the CTE is visible,
                // never `public.name`.
                let shadowed = !reference.qualified
                    && ctes.iter().any(|cte| {
                        cte.name == reference.name && cte.is_visible_at(reference.index)
                    });
                if !shadowed {
                    tables.insert(reference.name);
                }
            }
        }
        tables
    }

    /// Recognize only straight-line, transaction-scoped owner exemptions.
    /// Every data reference must follow that table's NO FORCE and precede its
    /// restoring FORCE. Unknown statements (including DO, role changes and
    /// transaction control) fail closed; their execution order is not inferred.
    fn owner_managed_rls_tables(sql: &str) -> BTreeSet<String> {
        let tokens = tokens(&strip_for_scan(sql));
        let mut open = BTreeSet::new();
        let mut protected = BTreeSet::new();
        let mut unprotected = BTreeSet::new();
        for statement in tokens.split(|token| token == ";").filter(|s| !s.is_empty()) {
            let matches = |actual: &[String], expected: &[&str]| {
                actual.len() == expected.len()
                    && actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
            };
            if statement.len() >= 3 && matches(&statement[..2], &["ALTER", "TABLE"]) {
                let Some(table) = resolve_public_table(&statement[2]).filter(|t| t.qualified)
                else {
                    return BTreeSet::new();
                };
                if matches(
                    &statement[3..],
                    &["NO", "FORCE", "ROW", "LEVEL", "SECURITY"],
                ) {
                    open.insert(table.name);
                } else if matches(&statement[3..], &["FORCE", "ROW", "LEVEL", "SECURITY"]) {
                    open.remove(&table.name);
                } else if open.contains(&table.name) && is_owner_cleanup_drop(&statement[3..]) {
                    // Default DROP actions do not change the role or RLS scope.
                    // In particular, migration 045 removes obsolete columns
                    // after its owner-visible update and before restoring FORCE.
                } else {
                    return BTreeSet::new();
                }
            } else if matches(statement, &["SET", "CONSTRAINTS", "ALL", "IMMEDIATE"]) {
                // Flush deferred checks before restoring FORCE in the same transaction.
            } else if statement[0].eq_ignore_ascii_case("UPDATE")
                || statement[0].eq_ignore_ascii_case("DELETE")
            {
                for table in referenced_tables(statement) {
                    if open.contains(&table.name) {
                        protected.insert(table.name);
                    } else {
                        unprotected.insert(table.name);
                    }
                }
            } else {
                return BTreeSet::new();
            }
        }
        if !open.is_empty() {
            return BTreeSet::new();
        }
        protected.retain(|table| !unprotected.contains(table));
        protected
    }

    /// Accept only DROP COLUMN/CONSTRAINT <identifier>, optionally combined
    /// with commas. Modifiers (including CASCADE), mixed ALTER actions and
    /// incomplete syntax retain the ordinary visibility check.
    fn is_owner_cleanup_drop(actions: &[String]) -> bool {
        !actions.is_empty()
            && actions.split(|token| token == ",").all(|action| {
                action.len() == 3
                    && action[0].eq_ignore_ascii_case("DROP")
                    && (action[1].eq_ignore_ascii_case("COLUMN")
                        || action[1].eq_ignore_ascii_case("CONSTRAINT"))
                    && is_cleanup_identifier(&action[2])
            })
    }

    fn is_cleanup_identifier(token: &str) -> bool {
        if let Some(quoted) = token.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            // Quoted punctuation stays inside one token. Only doubled quotes
            // are permitted inside the identifier, not a second SQL clause.
            return !quoted.is_empty() && !quoted.replace("\"\"", "").contains('"');
        }
        let mut chars = token.chars();
        chars
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
    }

    /// NO FORCE exempts this exact owner, not an arbitrary migration role.
    /// Inheritance/partition trees remain subject to the ordinary visibility
    /// check because altering one relation does not establish every child's state.
    async fn owns_plain_table(pool: &PgPool, table: &str) -> Result<bool, DbError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             JOIN pg_roles r ON r.oid = c.relowner
             WHERE n.nspname = 'public' AND c.relname = $1 AND c.relkind = 'r'
               AND r.rolname = current_user
               AND NOT EXISTS (SELECT 1 FROM pg_inherits i
                               WHERE i.inhrelid = c.oid OR i.inhparent = c.oid))",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .map_err(DbError::MigrationRlsGuard)
    }

    /// `SET [LOCAL | SESSION] ROLE <name>` → the role name (`NONE` included).
    fn set_role_target(statement: &[String]) -> Option<String> {
        let mut index = 0;
        if !statement.first()?.eq_ignore_ascii_case("SET") {
            return None;
        }
        index += 1;
        if statement
            .get(index)
            .is_some_and(|t| t.eq_ignore_ascii_case("LOCAL") || t.eq_ignore_ascii_case("SESSION"))
        {
            index += 1;
        }
        if !statement.get(index)?.eq_ignore_ascii_case("ROLE") {
            return None;
        }
        statement.get(index + 1).map(|name| unquote(name))
    }

    /// `RESET ROLE`.
    fn is_reset_role(statement: &[String]) -> bool {
        statement.len() >= 2
            && statement[0].eq_ignore_ascii_case("RESET")
            && statement[1].eq_ignore_ascii_case("ROLE")
    }

    /// Whether a statement reads or writes rows in the migration session.
    ///
    /// DDL, privilege and session-setting statements are skipped: their
    /// `FROM`/`USING`/`ON` clauses name roles, columns or policy expressions,
    /// not rows. `CREATE TABLE ... AS SELECT` and `CREATE MATERIALIZED VIEW`
    /// read rows at creation and are kept. `EXECUTE` is dynamic SQL the
    /// scanner cannot see into. `TRUNCATE` is exempt from row security.
    fn is_data_statement(statement: &[String]) -> bool {
        const SKIPPED: &[&str] = &[
            "ALTER",
            "DROP",
            "GRANT",
            "REVOKE",
            "COMMENT",
            "SET",
            "RESET",
            "EXECUTE",
            "TRUNCATE",
            "COMMIT",
            "ROLLBACK",
            "SAVEPOINT",
            "RELEASE",
            "VACUUM",
            "ANALYZE",
            "CLUSTER",
            "REINDEX",
            "LOCK",
            "LISTEN",
            "NOTIFY",
            "LANGUAGE",
            "DO",
        ];
        let Some(first) = statement.first() else {
            return false;
        };
        if SKIPPED.iter().any(|kw| first.eq_ignore_ascii_case(kw)) {
            return false;
        }
        if first.eq_ignore_ascii_case("CREATE") {
            let materialized = statement
                .iter()
                .any(|t| t.eq_ignore_ascii_case("MATERIALIZED"));
            let table_as_query = statement.iter().any(|t| t.eq_ignore_ascii_case("TABLE"))
                && statement.windows(2).any(|w| {
                    w[0].eq_ignore_ascii_case("AS")
                        && (w[1].eq_ignore_ascii_case("SELECT")
                            || w[1].eq_ignore_ascii_case("WITH"))
                });
            return materialized || table_as_query;
        }
        true
    }

    /// A CTE defined in the statement and where its name is visible.
    struct CteDefinition {
        /// Lower-cased (or quoted, verbatim) CTE name.
        name: String,
        /// First token index at which the name refers to the CTE.
        visible_from: usize,
        /// Token index at which the enclosing query ends (the `)` closing
        /// the subquery the `WITH` sits in, or the statement length).
        visible_until: usize,
    }

    impl CteDefinition {
        /// Under `WITH RECURSIVE` every CTE of the clause is visible from the
        /// clause onwards (self and forward references included); otherwise
        /// a CTE is visible only after its own definition ends. Either way
        /// the name is scoped to the query that owns the `WITH`.
        fn is_visible_at(&self, index: usize) -> bool {
            index > self.visible_from && index < self.visible_until
        }
    }

    /// A `WITH` clause being scanned at some parenthesis depth.
    #[derive(Clone, Copy)]
    struct WithClause {
        /// Token index of `WITH`.
        with_index: usize,
        /// `WITH RECURSIVE`.
        recursive: bool,
        /// Token index where the owning query ends.
        query_end: usize,
    }

    /// CTEs defined in the statement:
    /// `WITH [RECURSIVE] name [(cols)] AS [[NOT] MATERIALIZED] (body)` and
    /// the comma-separated definitions that follow. A `WITH` clause may sit
    /// anywhere (`CREATE TABLE ... AS WITH ...`, a subquery); recursion and
    /// scope are properties of the clause, tracked per parenthesis depth.
    fn cte_definitions(statement: &[String]) -> Vec<CteDefinition> {
        let mut definitions = Vec::new();
        let mut open_parens: Vec<usize> = Vec::new();
        let mut clauses: Vec<Option<WithClause>> = vec![None];
        for (index, token) in statement.iter().enumerate() {
            if token == "(" {
                open_parens.push(index);
                clauses.push(None);
                continue;
            }
            if token == ")" {
                open_parens.pop();
                if clauses.len() > 1 {
                    clauses.pop();
                }
                continue;
            }
            let depth = open_parens.len();
            if token.eq_ignore_ascii_case("WITH") {
                let recursive = statement
                    .get(index + 1)
                    .is_some_and(|t| t.eq_ignore_ascii_case("RECURSIVE"));
                let query_end = open_parens
                    .last()
                    .and_then(|open| closing_paren(statement, *open))
                    .unwrap_or(statement.len());
                clauses[depth] = Some(WithClause {
                    with_index: index,
                    recursive,
                    query_end,
                });
            }
            let opener = token.eq_ignore_ascii_case("WITH")
                || token.eq_ignore_ascii_case("RECURSIVE")
                || token == ",";
            if !opener {
                continue;
            }
            let Some(clause) = clauses[depth] else {
                continue;
            };
            let Some(name) = statement.get(index + 1) else {
                continue;
            };
            let mut next = index + 2;
            if statement.get(next).is_some_and(|t| t == "(") {
                next = match closing_paren(statement, next) {
                    Some(close) => close + 1,
                    None => continue,
                };
            }
            if !statement
                .get(next)
                .is_some_and(|t| t.eq_ignore_ascii_case("AS"))
            {
                continue;
            }
            next += 1;
            if statement
                .get(next)
                .is_some_and(|t| t.eq_ignore_ascii_case("NOT"))
            {
                next += 1;
            }
            if statement
                .get(next)
                .is_some_and(|t| t.eq_ignore_ascii_case("MATERIALIZED"))
            {
                next += 1;
            }
            if statement.get(next).is_none_or(|t| t != "(") {
                continue;
            }
            let Some(body_end) = closing_paren(statement, next) else {
                continue;
            };
            if let Some(reference) = resolve_public_table(name) {
                definitions.push(CteDefinition {
                    name: reference.name,
                    visible_from: if clause.recursive {
                        clause.with_index
                    } else {
                        body_end
                    },
                    visible_until: clause.query_end,
                });
            }
        }
        definitions
    }

    /// Index of the `)` matching the `(` at `open`.
    fn closing_paren(statement: &[String], open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (offset, token) in statement[open..].iter().enumerate() {
            if token == "(" {
                depth += 1;
            } else if token == ")" {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
        }
        None
    }

    /// A table reference found in a statement.
    struct TableReference {
        /// Lower-cased (or quoted, verbatim) table name.
        name: String,
        /// Written as `public.name`, so it cannot refer to a CTE.
        qualified: bool,
        /// Token index of the reference, for CTE visibility.
        index: usize,
    }

    /// Candidate `public` table references a data statement makes.
    fn referenced_tables(statement: &[String]) -> Vec<TableReference> {
        // `FROM` inside these calls separates arguments, not relations.
        const FROM_IN_CALL: &[&str] = &["EXTRACT", "SUBSTRING", "TRIM", "OVERLAY", "POSITION"];
        let mut references = Vec::new();
        let mut enclosing_calls: Vec<Option<&str>> = Vec::new();
        for (index, token) in statement.iter().enumerate() {
            let previous = index.checked_sub(1).map(|i| statement[i].as_str());
            if token == "(" {
                enclosing_calls.push(previous);
                continue;
            }
            if token == ")" {
                enclosing_calls.pop();
                continue;
            }
            if token.eq_ignore_ascii_case("FROM") || token.eq_ignore_ascii_case("USING") {
                // `IS DISTINCT FROM x` compares values.
                if previous.is_some_and(|p| p.eq_ignore_ascii_case("DISTINCT")) {
                    continue;
                }
                let inside_call = enclosing_calls.last().is_some_and(|call| {
                    call.is_some_and(|name| {
                        FROM_IN_CALL.iter().any(|kw| name.eq_ignore_ascii_case(kw))
                    })
                });
                if inside_call {
                    continue;
                }
                for candidate in relation_list(statement, index + 1) {
                    references.extend(
                        resolve_public_table(&statement[candidate]).map(|r| r.at(candidate)),
                    );
                }
            } else if token.eq_ignore_ascii_case("JOIN") {
                references.extend(candidate_at(statement, index + 1));
            } else if token.eq_ignore_ascii_case("INTO") {
                // `INSERT INTO t` / `MERGE INTO t` name a table; PL/pgSQL's
                // `SELECT ... INTO var` names a variable.
                if previous.is_some_and(|p| {
                    p.eq_ignore_ascii_case("INSERT") || p.eq_ignore_ascii_case("MERGE")
                }) {
                    references.extend(candidate_at(statement, index + 1));
                }
            } else if token.eq_ignore_ascii_case("UPDATE") {
                // `FOR UPDATE`, `ON CONFLICT DO UPDATE`, `ON UPDATE CASCADE`,
                // trigger events and privilege lists are not statements; a
                // real UPDATE is `UPDATE [ONLY] t [[AS] alias] SET`.
                const NOT_A_STATEMENT: &[&str] =
                    &["FOR", "DO", "ON", "OR", "AFTER", "BEFORE", "INSTEAD", ","];
                if previous
                    .is_some_and(|p| NOT_A_STATEMENT.iter().any(|kw| p.eq_ignore_ascii_case(kw)))
                {
                    continue;
                }
                let has_set_clause = statement
                    .iter()
                    .skip(index + 1)
                    .take(6)
                    .any(|t| t.eq_ignore_ascii_case("SET"));
                if has_set_clause {
                    references.extend(candidate_at(statement, index + 1));
                }
            }
        }
        references
    }

    /// Resolve the token at `index` (after an optional `ONLY`/`LATERAL`).
    fn candidate_at(statement: &[String], mut index: usize) -> Option<TableReference> {
        if statement
            .get(index)
            .is_some_and(|t| t.eq_ignore_ascii_case("ONLY") || t.eq_ignore_ascii_case("LATERAL"))
        {
            index += 1;
        }
        statement
            .get(index)
            .and_then(|t| resolve_public_table(t))
            .map(|r| r.at(index))
    }

    impl TableReference {
        /// Record where in the statement the reference was found.
        fn at(mut self, index: usize) -> Self {
            self.index = index;
            self
        }
    }

    /// Token indexes of every relation in a `FROM`/`USING` list starting at
    /// `start`: the first item and each item after a top-level comma or
    /// `JOIN`, until the clause ends. Parenthesised subqueries are skipped.
    fn relation_list(statement: &[String], start: usize) -> Vec<usize> {
        const CLAUSE_END: &[&str] = &[
            "WHERE",
            "GROUP",
            "HAVING",
            "ORDER",
            "LIMIT",
            "OFFSET",
            "SET",
            "RETURNING",
            "WINDOW",
            "UNION",
            "EXCEPT",
            "INTERSECT",
            "FOR",
            "LOOP",
            "WHEN",
            "THEN",
            "ELSE",
            "END",
            "VALUES",
            "USING",
            "INTO",
        ];
        let mut candidates = Vec::new();
        let mut depth = 0usize;
        let mut case_depth = 0usize;
        let mut expect_relation = true;
        let mut index = start;
        while index < statement.len() {
            let token = &statement[index];
            if token == "(" {
                depth += 1;
                expect_relation = false;
            } else if token == ")" {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if depth == 0 {
                // `CASE ... WHEN ... THEN ... ELSE ... END` inside a join
                // condition is an expression, not a clause boundary.
                if token.eq_ignore_ascii_case("CASE") {
                    case_depth += 1;
                    index += 1;
                    continue;
                }
                if case_depth > 0 {
                    if token.eq_ignore_ascii_case("END") {
                        case_depth -= 1;
                    }
                    index += 1;
                    continue;
                }
                if CLAUSE_END.iter().any(|kw| token.eq_ignore_ascii_case(kw)) {
                    break;
                }
                if token == "," || token.eq_ignore_ascii_case("JOIN") {
                    expect_relation = true;
                } else if expect_relation {
                    if token.eq_ignore_ascii_case("ONLY") || token.eq_ignore_ascii_case("LATERAL") {
                        index += 1;
                        continue;
                    }
                    candidates.push(index);
                    expect_relation = false;
                }
            }
            index += 1;
        }
        candidates
    }

    /// Resolve a token to a `public` table reference, or `None` when it is a
    /// keyword, punctuation, a function call, a system catalog, or qualified
    /// with another schema.
    fn resolve_public_table(raw: &str) -> Option<TableReference> {
        const NOT_A_TABLE: &[&str] = &[
            "SET",
            "OF",
            "ON",
            "OR",
            "AND",
            "NOT",
            "NOWAIT",
            "SKIP",
            "WHERE",
            "CASCADE",
            "RESTRICT",
            "SELECT",
            "VALUES",
            "TO",
            "FROM",
            "INTO",
            "WITH",
            "INSERT",
            "UPDATE",
            "DELETE",
            "TRUNCATE",
            "REFERENCES",
            "TRIGGER",
            "USAGE",
            "EXECUTE",
            "ALL",
            "ONLY",
            "LATERAL",
            "STRICT",
            "DEFAULT",
            "NULL",
            "TRUE",
            "FALSE",
            "EXISTS",
            "CASE",
            "IN",
            "ARRAY",
            "ROW",
            "TABLE",
            "LOOP",
            "THEN",
            "RETURNING",
            "DISTINCT",
            "AS",
            "IS",
            "ERRCODE",
            "TEMP",
            "TEMPORARY",
            "UNLOGGED",
            "RECURSIVE",
            "MATERIALIZED",
        ];
        if NOT_A_TABLE.iter().any(|kw| raw.eq_ignore_ascii_case(kw)) {
            return None;
        }
        // Unqualified system catalogs and views (`pg_roles`, `pg_class`) are
        // never tenant tables; `public.pg_*` is reserved by convention.
        if raw.to_ascii_lowercase().starts_with("pg_") {
            return None;
        }
        let (schema, name) = match split_qualified(raw) {
            Some((schema, name)) => (Some(schema), name),
            None => (None, raw),
        };
        if let Some(schema) = schema {
            if !unquote(schema).eq_ignore_ascii_case("public") {
                return None;
            }
        }
        let name = if name.starts_with('"') {
            unquote(name)
        } else {
            name.to_ascii_lowercase()
        };
        let first = name.chars().next()?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        Some(TableReference {
            name,
            qualified: schema.is_some(),
            index: 0,
        })
    }

    /// Split `schema.name`, honouring a quoted schema or name that contains a dot.
    fn split_qualified(raw: &str) -> Option<(&str, &str)> {
        let mut in_quotes = false;
        for (position, ch) in raw.char_indices() {
            match ch {
                '"' => in_quotes = !in_quotes,
                '.' if !in_quotes && position > 0 => {
                    return Some((&raw[..position], &raw[position + 1..]))
                }
                _ => {}
            }
        }
        None
    }

    /// Strip surrounding double quotes, unescape `""`, and restore the
    /// separator characters the tokenizer protected.
    fn unquote(raw: &str) -> String {
        let inner = raw
            .strip_prefix('"')
            .and_then(|n| n.strip_suffix('"'))
            .map_or_else(|| raw.to_string(), |inner| inner.replace("\"\"", "\""));
        QUOTED.iter().fold(inner, |text, (real, placeholder)| {
            text.replace(*placeholder, &real.to_string())
        })
    }

    /// Split scan text into tokens. Parentheses, commas and semicolons are
    /// tokens of their own; `schema . name` written with spaces is rejoined.
    fn tokens(text: &str) -> Vec<String> {
        let mut spaced = String::with_capacity(text.len() + 16);
        for ch in text.chars() {
            match ch {
                '(' | ')' | ',' | ';' => {
                    spaced.push(' ');
                    spaced.push(ch);
                    spaced.push(' ');
                }
                _ => spaced.push(ch),
            }
        }
        let raw: Vec<&str> = spaced.split_whitespace().collect();
        let mut merged: Vec<String> = Vec::with_capacity(raw.len());
        let mut index = 0;
        while index < raw.len() {
            let token = raw[index];
            if token == "." {
                if let (Some(previous), Some(next)) = (merged.pop(), raw.get(index + 1)) {
                    merged.push(format!("{previous}.{next}"));
                    index += 2;
                    continue;
                }
            } else if let Some(stem) = token.strip_suffix('.') {
                if let Some(next) = raw.get(index + 1) {
                    merged.push(format!("{stem}.{next}"));
                    index += 2;
                    continue;
                }
            } else if let Some(rest) = token.strip_prefix('.') {
                if let Some(previous) = merged.pop() {
                    merged.push(format!("{previous}.{rest}"));
                    index += 1;
                    continue;
                }
            }
            merged.push(token.to_string());
            index += 1;
        }
        merged
    }

    /// Drop SQL-standard function bodies (`BEGIN ATOMIC ... END`): they are
    /// parsed at definition time but only run when the function is called.
    /// `CASE ... END` inside such a body is balanced so it does not end it.
    fn without_atomic_function_bodies(tokens: Vec<String>) -> Vec<String> {
        let mut kept = Vec::with_capacity(tokens.len());
        let mut index = 0;
        while index < tokens.len() {
            let is_atomic = tokens[index].eq_ignore_ascii_case("BEGIN")
                && tokens
                    .get(index + 1)
                    .is_some_and(|t| t.eq_ignore_ascii_case("ATOMIC"));
            if !is_atomic {
                kept.push(tokens[index].clone());
                index += 1;
                continue;
            }
            let mut case_depth = 0usize;
            index += 2;
            while index < tokens.len() {
                let token = &tokens[index];
                index += 1;
                if token.eq_ignore_ascii_case("CASE") {
                    case_depth += 1;
                } else if token.eq_ignore_ascii_case("END") {
                    if case_depth == 0 {
                        break;
                    }
                    case_depth -= 1;
                }
            }
        }
        kept
    }

    /// Prepare SQL for scanning: comments (nested block comments included)
    /// and string literals become a space, `CREATE FUNCTION` bodies
    /// (dollar-quoted, not preceded by `DO`) become a space, `DO` bodies
    /// (dollar-quoted or string literals) are kept because they execute
    /// immediately, a string literal after `SET ROLE` is kept as the role
    /// name, and separators inside double-quoted identifiers are protected.
    pub(super) fn strip_for_scan(sql: &str) -> String {
        let bytes = sql.as_bytes();
        let mut out = String::with_capacity(sql.len());
        let mut i = 0;
        while i < bytes.len() {
            let rest = &sql[i..];
            if rest.starts_with("--") {
                out.push(' ');
                i += rest.find('\n').unwrap_or(rest.len());
            } else if rest.starts_with("/*") {
                out.push(' ');
                i += block_comment_span(rest);
            } else if rest.starts_with('\'') {
                let escape_string = out.ends_with(['E', 'e'])
                    && !out[..out.len() - 1].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                if escape_string {
                    // The `E` prefix is part of the literal, not a word.
                    out.pop();
                }
                let span = if escape_string {
                    escape_quoted_span(rest)
                } else {
                    quoted_span(rest, '\'')
                };
                // The body excludes the closing quote when there is one; an
                // unterminated literal runs to the end of the input.
                let terminated = span >= 2 && rest[..span].ends_with('\'');
                let literal = &rest[1..if terminated { span - 1 } else { span }];
                if opens_role_name(&out) {
                    out.push('"');
                    out.push_str(&unescape_literal(literal, escape_string).replace('"', "\"\""));
                    out.push('"');
                } else if opens_do_block(&out) {
                    out.push(';');
                    out.push_str(&strip_for_scan(&unescape_literal(literal, escape_string)));
                    out.push(';');
                } else {
                    out.push(' ');
                }
                i += span;
            } else if rest.starts_with('"') {
                let span = quoted_span(rest, '"');
                for ch in rest[..span].chars() {
                    let protected = QUOTED
                        .iter()
                        .find(|(real, _)| *real == ch || (*real == ' ' && ch.is_whitespace()))
                        .map(|(_, placeholder)| *placeholder);
                    out.push(protected.unwrap_or(ch));
                }
                i += span;
            } else if let Some(delimiter) = dollar_delimiter_at(sql, i) {
                let body_start = delimiter.len();
                let body_end = rest[body_start..]
                    .find(&delimiter)
                    .map_or(rest.len(), |end| body_start + end);
                let skip = (body_end + delimiter.len()).min(rest.len());
                out.push(' ');
                if opens_do_block(&out) {
                    out.push(';');
                    out.push_str(&strip_for_scan(&rest[body_start..body_end]));
                    out.push(';');
                }
                out.push(' ');
                i += skip;
            } else {
                let ch = rest.chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    /// Length of the block comment starting at `text`, honouring nesting.
    fn block_comment_span(text: &str) -> usize {
        let mut depth = 0usize;
        let mut j = 0;
        while j < text.len() {
            if text[j..].starts_with("/*") {
                depth += 1;
                j += 2;
            } else if text[j..].starts_with("*/") {
                depth -= 1;
                j += 2;
                if depth == 0 {
                    return j;
                }
            } else {
                j += text[j..].chars().next().map_or(1, char::len_utf8);
            }
        }
        text.len()
    }

    /// Length of a quoted span starting at `text[0] == quote`, including the
    /// closing quote; a doubled quote inside is an escape.
    fn quoted_span(text: &str, quote: char) -> usize {
        let mut j = quote.len_utf8();
        loop {
            match text[j..].find(quote) {
                Some(offset) => {
                    j += offset + quote.len_utf8();
                    if text[j..].starts_with(quote) {
                        j += quote.len_utf8();
                        continue;
                    }
                    return j;
                }
                None => return text.len(),
            }
        }
    }

    /// Length of an `E'...'` escape-string span (backslash escapes and
    /// doubled quotes do not terminate it).
    fn escape_quoted_span(text: &str) -> usize {
        let mut chars = text.char_indices().skip(1);
        while let Some((position, ch)) = chars.next() {
            match ch {
                '\\' => {
                    chars.next();
                }
                '\'' => {
                    if text[position + 1..].starts_with('\'') {
                        chars.next();
                    } else {
                        return position + 1;
                    }
                }
                _ => {}
            }
        }
        text.len()
    }

    /// Decode a string literal's body (`''` and, for escape strings, `\'`
    /// and `\\`) into the text it denotes.
    fn unescape_literal(body: &str, escape_string: bool) -> String {
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\'' if chars.peek() == Some(&'\'') => {
                    chars.next();
                    out.push('\'');
                }
                '\\' if escape_string => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => {}
                },
                _ => out.push(ch),
            }
        }
        out
    }

    /// Whether the text so far ends in `DO` or `DO LANGUAGE <name>`, i.e. the
    /// body that follows is an anonymous block.
    fn opens_do_block(text: &str) -> bool {
        let words = trailing_words(text, 3);
        match words.as_slice() {
            [last, ..] if last.eq_ignore_ascii_case("DO") => true,
            [_, language, keyword] => {
                keyword.eq_ignore_ascii_case("DO") && language.eq_ignore_ascii_case("LANGUAGE")
            }
            _ => false,
        }
    }

    /// Whether the text so far ends in `SET [LOCAL|SESSION] ROLE`, i.e. the
    /// string literal that follows is a role name.
    fn opens_role_name(text: &str) -> bool {
        let words = trailing_words(text, 3);
        let role_then_set = |a: &str, b: &str| {
            a.eq_ignore_ascii_case("ROLE")
                && (b.eq_ignore_ascii_case("SET")
                    || b.eq_ignore_ascii_case("LOCAL")
                    || b.eq_ignore_ascii_case("SESSION"))
        };
        match words.as_slice() {
            [last, before, ..] => role_then_set(last, before),
            _ => false,
        }
    }

    /// The last `count` words of `text`, most recent first.
    fn trailing_words(text: &str, count: usize) -> Vec<&str> {
        text.rsplit(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | ';'))
            .filter(|w| !w.is_empty())
            .take(count)
            .collect()
    }

    /// A dollar-quote opener at byte `position` of `sql`. A `$` that follows
    /// an identifier character is part of an identifier (`col$x$`), not a
    /// delimiter.
    fn dollar_delimiter_at(sql: &str, position: usize) -> Option<String> {
        let previous = sql[..position].chars().next_back();
        if previous.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            return None;
        }
        dollar_delimiter(&sql[position..])
    }

    /// `$$` or `$tag$` at the start of `text`.
    fn dollar_delimiter(text: &str) -> Option<String> {
        let mut chars = text.chars();
        if chars.next()? != '$' {
            return None;
        }
        let mut tag = String::new();
        for ch in chars {
            if ch == '$' {
                return Some(format!("${tag}$"));
            }
            let valid = if tag.is_empty() {
                ch.is_ascii_alphabetic() || ch == '_'
            } else {
                ch.is_ascii_alphanumeric() || ch == '_'
            };
            if !valid {
                return None;
            }
            tag.push(ch);
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn tables(sql: &str) -> Vec<String> {
            blind_data_tables(sql).into_iter().collect()
        }

        #[test]
        fn owner_exemptions_require_unconditional_balanced_protection() {
            let protected = "ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;
                UPDATE public.workspaces SET push_mode = 'manual';
                ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;";
            assert_eq!(
                owner_managed_rls_tables(protected),
                BTreeSet::from(["workspaces".into()])
            );
            for sql in [
                protected.replace(
                    "ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;",
                    "",
                ),
                format!("UPDATE public.workspaces SET push_mode = 'manual'; {protected}"),
                format!("{protected} DELETE FROM public.workspaces;"),
                format!("DO $$ BEGIN {protected} END $$;"),
                format!("SET ROLE somebody; {protected} RESET ROLE;"),
                format!("BEGIN; {protected} COMMIT;"),
                protected.replace("public.workspaces", "other.workspaces"),
                protected.replace(
                    "UPDATE public.workspaces",
                    "ALTER TABLE public.workspaces OWNER TO somebody; UPDATE public.workspaces",
                ),
            ] {
                assert!(owner_managed_rls_tables(&sql).is_empty(), "{sql}");
            }
            let source = protected.replace(
                "SET push_mode = 'manual'",
                "SET push_mode = 'manual' FROM public.repos",
            );
            assert_eq!(
                owner_managed_rls_tables(&source),
                BTreeSet::from(["workspaces".into()])
            );
            assert!(
                blind_data_tables(&source).contains("repos"),
                "unprotected read sources still require row visibility"
            );
        }

        #[test]
        fn original_045_owner_cleanup_is_recognized() {
            let migration =
                include_str!("../../g5-api/src/db/migrations/045_drop_review_document_targets.sql");
            assert_eq!(
                owner_managed_rls_tables(migration),
                BTreeSet::from(["review_suggestions".into()])
            );
            assert_eq!(tables(migration), vec!["review_suggestions"]);
        }

        #[test]
        fn owner_cleanup_accepts_only_plain_drops_inside_the_same_scope() {
            let bracket = |action: &str| {
                format!(
                    "ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY;
                     UPDATE public.workspaces SET push_mode = 'manual';
                     {action}
                     ALTER TABLE public.workspaces FORCE ROW LEVEL SECURITY;"
                )
            };
            for action in [
                "ALTER TABLE public.workspaces DROP COLUMN legacy;",
                "ALTER TABLE public.workspaces DROP CONSTRAINT legacy;",
                "ALTER TABLE public.workspaces DROP CONSTRAINT legacy, DROP COLUMN old, DROP COLUMN \"old, column\";",
                "ALTER TABLE public.workspaces DROP COLUMN \"quoted\"\"name\";",
            ] {
                assert_eq!(
                    owner_managed_rls_tables(&bracket(action)),
                    BTreeSet::from(["workspaces".into()]),
                    "{action}"
                );
            }
            for action in [
                "ALTER TABLE public.repos DROP COLUMN legacy;",
                "ALTER TABLE other.workspaces DROP COLUMN legacy;",
                "ALTER TABLE workspaces DROP COLUMN legacy;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy CASCADE;",
                "ALTER TABLE public.workspaces DROP CONSTRAINT legacy RESTRICT;",
                "ALTER TABLE public.workspaces DROP COLUMN IF EXISTS legacy;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy, DISABLE ROW LEVEL SECURITY;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy, OWNER TO other;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy, ADD COLUMN extra text;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy,;",
                "ALTER TABLE public.workspaces DROP COLUMN;",
                "ALTER TABLE public.workspaces DROP COLUMN legacy.name;",
                "ALTER TABLE public.workspaces DROP COLUMN \"\";",
                "ALTER TABLE public.workspaces NO FORCE ROW LEVEL SECURITY, DROP COLUMN legacy;",
            ] {
                assert!(
                    owner_managed_rls_tables(&bracket(action)).is_empty(),
                    "{action}"
                );
            }
            for sql in [
                format!(
                    "ALTER TABLE public.workspaces DROP COLUMN legacy; {}",
                    bracket("")
                ),
                format!(
                    "{} ALTER TABLE public.workspaces DROP COLUMN legacy;",
                    bracket("")
                ),
            ] {
                assert!(owner_managed_rls_tables(&sql).is_empty(), "{sql}");
            }
        }

        #[test]
        fn finds_writes_and_their_read_sources() {
            let sql = r#"
                UPDATE public.tasks AS task SET kind = 'agent' FROM public.jobs AS job WHERE task.job_id = job.id;
                INSERT INTO public.task_events(id) SELECT id FROM public.task_questions AS question;
                DELETE FROM public.workspace_branches WHERE scope IN ('job', 'task');
                WITH missing AS (SELECT 1 FROM public.chat_sessions), inserted AS (INSERT INTO public.audit VALUES (1) RETURNING id)
                    UPDATE tasks SET x = 1 FROM missing WHERE EXISTS (SELECT 1 FROM inserted);
                MERGE INTO public.targets t USING public.sources s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = 1;
                SELECT count(*) FROM public.workspaces LEFT JOIN public.repos r ON r.id = w.repo_id;
                TRUNCATE TABLE public.exempt;
            "#;
            assert_eq!(
                tables(sql),
                vec![
                    "audit",
                    "chat_sessions",
                    "jobs",
                    "repos",
                    "sources",
                    "targets",
                    "task_events",
                    "task_questions",
                    "tasks",
                    "workspace_branches",
                    "workspaces",
                ]
            );
        }

        #[test]
        fn walks_comma_separated_relation_lists() {
            let sql = r#"
                SELECT count(*) FROM (VALUES (1)) seed(n), public.orgs;
                SELECT 1 FROM public.a a, public.b AS b, ONLY public.c WHERE a.id = b.id;
                DELETE FROM public.d USING public.e, public.f WHERE d.id = e.id;
                SELECT extract(epoch FROM created_at) FROM public.g;
            "#;
            assert_eq!(tables(sql), vec!["a", "b", "c", "d", "e", "f", "g", "orgs"]);
        }

        #[test]
        fn scans_do_blocks_but_not_function_bodies() {
            let sql = r#"
                CREATE FUNCTION public.f() RETURNS trigger LANGUAGE plpgsql AS $$
                BEGIN
                    UPDATE public.in_function SET chat_session_id = NEW.id WHERE id = NEW.task_id;
                    RETURN NULL;
                END;
                $$;
                DO $body$
                DECLARE
                    unresolved bigint;
                BEGIN
                    SELECT count(*) INTO unresolved FROM public.counted WHERE kind = 'agent';
                    IF unresolved <> 0 THEN
                        RAISE EXCEPTION 'blocked: %', unresolved USING ERRCODE = '23514';
                    END IF;
                    DELETE FROM public.in_do_block;
                    EXECUTE format('UPDATE public.dynamic SET a = %L', 1);
                END
                $body$;
                DO LANGUAGE plpgsql $$ BEGIN UPDATE public.tagged SET a = 1; END $$;
                DO 'BEGIN UPDATE public.string_body SET a = 1; END';
                DO E'BEGIN DELETE FROM public.escape_body WHERE name = \'x\'; END';
                CREATE FUNCTION public.count_orgs() RETURNS bigint LANGUAGE SQL
                BEGIN ATOMIC
                    SELECT CASE WHEN true THEN 1 ELSE 0 END;
                    SELECT count(*) FROM public.atomic_body;
                END;
                SELECT 1 FROM public.after_atomic;
            "#;
            assert_eq!(
                tables(sql),
                vec![
                    "after_atomic",
                    "counted",
                    "escape_body",
                    "in_do_block",
                    "string_body",
                    "tagged"
                ]
            );
        }

        #[test]
        fn ignores_ddl_privileges_comments_strings_and_keyword_neighbours() {
            let sql = r#"
                -- UPDATE public.commented SET a = 1;
                /* DELETE FROM public.block_commented; */
                /* outer /* inner */ UPDATE public.nested_comment SET a = 1; */
                CREATE TRIGGER t BEFORE INSERT OR UPDATE OF status ON public.tasks
                    FOR EACH ROW EXECUTE FUNCTION public.f();
                GRANT SELECT, INSERT, UPDATE, DELETE ON public.tasks TO g5_tenant;
                GRANT UPDATE (name), DELETE ON public.orgs TO g5_app;
                REVOKE ALL ON FUNCTION authz.lookup(uuid) FROM PUBLIC;
                CREATE POLICY p ON public.tasks FOR UPDATE USING (org_id = ANY (authz.current_org_ids()));
                CREATE UNIQUE INDEX u ON public.tasks USING btree (id) WHERE NOT deleted;
                ALTER TABLE public.child ADD FOREIGN KEY (p) REFERENCES public.parent(id)
                    ON DELETE SET NULL ON UPDATE CASCADE;
                INSERT INTO authz.session_ctx(pid) VALUES (1);
                SELECT 'INSERT INTO public.in_string' AS s FROM public.orgs FOR UPDATE;
                SELECT E'it\'s $tag$';
                ALTER TABLE public.tasks ADD CONSTRAINT c CHECK (a <> 'DELETE FROM public.x');
                INSERT INTO public.real(id) VALUES (1) ON CONFLICT (id) DO UPDATE SET id = 1;
                SELECT set_config('k', (SELECT count(*)::text FROM pg_auth_members m JOIN pg_roles r ON r.oid = m.roleid), false);
                UPDATE public.tasks SET a = 1 WHERE name IS DISTINCT FROM branch_name;
                CREATE VIEW public.v AS SELECT * FROM public.viewed;
                CREATE TABLE public.copy AS SELECT * FROM public.copied;
                CREATE MATERIALIZED VIEW public.mv AS SELECT * FROM public.materialized;
            "#;
            assert_eq!(
                tables(sql),
                vec!["copied", "materialized", "orgs", "real", "tasks"]
            );
        }

        #[test]
        fn cte_names_shadow_only_unqualified_references_where_visible() {
            let sql = r#"
                WITH orgs AS (SELECT * FROM public.orgs) SELECT count(*) FROM orgs;
                WITH shadow(id) AS (VALUES (1)) SELECT * FROM shadow;
                WITH m AS MATERIALIZED (SELECT 1 FROM public.mat), n AS NOT MATERIALIZED (SELECT 2) SELECT * FROM m, n;
                WITH RECURSIVE tree AS (SELECT 1 FROM public.nodes UNION ALL SELECT 2 FROM tree) SELECT * FROM tree;
                WITH tasks AS MATERIALIZED (SELECT id FROM tasks) SELECT count(*) FROM tasks;
                WITH later AS (SELECT 1 FROM early), early(id) AS (VALUES (1)) SELECT * FROM later, early;
                CREATE TEMP TABLE task_order AS
                WITH RECURSIVE tasks(id, parent_id, depth) AS (
                    SELECT id, parent_id, 0 FROM task_import WHERE parent_id IS NULL
                    UNION ALL
                    SELECT s.id, s.parent_id, t.depth + 1 FROM task_import s JOIN tasks t ON s.parent_id = t.id
                )
                SELECT * FROM tasks;
                WITH RECURSIVE counted AS (SELECT count(*) FROM jobs), jobs AS (SELECT id FROM job_import)
                SELECT * FROM counted;
                CREATE TEMP TABLE task_stage AS
                SELECT * FROM (
                    WITH workspaces AS (SELECT id FROM workspace_import)
                    SELECT * FROM workspaces
                ) imported;
                SELECT * FROM (WITH repos AS (SELECT 1 FROM repo_import) SELECT * FROM repos) x
                JOIN repos r ON true;
            "#;
            // `tasks` inside its own non-recursive definition and `early`
            // referenced before its definition are the physical tables;
            // under WITH RECURSIVE, self and forward references are CTEs; a
            // CTE nested in a subquery is scoped to that subquery, so the
            // outer `JOIN repos` is the physical table.
            assert_eq!(
                tables(sql),
                vec![
                    "early",
                    "job_import",
                    "mat",
                    "nodes",
                    "orgs",
                    "repo_import",
                    "repos",
                    "task_import",
                    "tasks",
                    "workspace_import",
                ]
            );
        }

        #[test]
        fn case_expressions_do_not_end_relation_lists() {
            let sql = r#"
                SELECT count(*) FROM public.map m
                JOIN public.defaults d ON d.kind = CASE WHEN m.kind IS NULL THEN d.default_kind ELSE m.kind END,
                     public.tasks t
                WHERE t.id = m.task_id;
                MERGE INTO public.target t USING public.source s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = 1;
            "#;
            assert_eq!(
                tables(sql),
                vec!["defaults", "map", "source", "target", "tasks"]
            );
        }

        #[test]
        fn role_switch_exempts_only_the_statements_it_covers() {
            let sql = r#"
                UPDATE public.before SET a = 1;
                SET ROLE authz_lookup_definer;
                UPDATE public.elevated SET a = 1;
                RESET ROLE;
                UPDATE public.after SET a = 1;
                SET LOCAL ROLE "authz_lookup_definer";
                DELETE FROM public.local_elevated;
                SET ROLE g5_app;
                DELETE FROM public.other_role;
                SET SESSION ROLE authz_lookup_definer;
                DELETE FROM public.session_elevated;
                SET ROLE NONE;
                DELETE FROM public.none_role;
                SET ROLE 'authz_lookup_definer';
                DELETE FROM public.literal_elevated;
                DO $$ BEGIN RESET ROLE; END $$;
                UPDATE public.after_do_reset SET a = 1;
            "#;
            assert_eq!(
                tables(sql),
                vec![
                    "after",
                    "after_do_reset",
                    "before",
                    "none_role",
                    "other_role"
                ]
            );
            assert!(
                tables("-- SET ROLE authz_lookup_definer\nUPDATE public.tasks SET a = 1;")
                    .contains(&"tasks".to_string())
            );
        }

        #[test]
        fn tokenizer_handles_spacing_comments_and_quoted_identifiers() {
            let sql = r#"
                UPDATE public . spaced SET a = 1;
                UPDATE/* explanation */public.commented_adjacent SET a = 1;
                UPDATE public.
                    wrapped SET a = 1;
                DELETE FROM "Quoted Name";
                DELETE FROM "public"."Mixed""Quote";
                DELETE FROM ONLY public.only_table;
                UPDATE other_schema.tasks SET a = 1;
                UPDATE public."org;s" SET name = name;
                DELETE FROM "a,b(c)";
            "#;
            assert_eq!(
                tables(sql),
                vec![
                    "Mixed\"Quote",
                    "Quoted Name",
                    "a,b(c)",
                    "commented_adjacent",
                    "only_table",
                    "org;s",
                    "spaced",
                    "wrapped",
                ]
            );
        }

        #[test]
        fn odd_input_does_not_panic() {
            for sql in [
                "",
                ";;;",
                "SELECT 'unterminated",
                "SELECT 'é",
                "SELECT E'é",
                "SELECT 'é''",
                "DO 'é",
                "SELECT \"unterminated",
                "SELECT \"é",
                "DO $$ unterminated",
                "/* unterminated /* nested",
                "SELECT E'\\",
                "UPDATE",
                "INSERT INTO",
                "WITH",
                "WITH x AS (",
                "BEGIN ATOMIC SELECT 1",
                "SET ROLE",
                "$",
                "SELECT '$$' FROM public.t; UPDATE public.u SET a = 1;",
            ] {
                let _ = blind_data_tables(sql);
            }
            assert_eq!(
                tables("SELECT '$$' FROM public.t; UPDATE public.u SET a = 1;"),
                vec!["t", "u"]
            );
        }

        #[test]
        fn migration_008_data_section_is_seen_and_function_bodies_are_not() {
            let sql =
                include_str!("../../g5-api/src/db/migrations/008_lean_mvp_execution_cutover.sql");
            let found = blind_data_tables(sql);
            // Top-level backfills, the CTE insert, the DO-block preflight and
            // the workspace_branches prune.
            for expected in [
                "tasks",
                "jobs",
                "workspaces",
                "chat_sessions",
                "task_events",
                "task_questions",
                "workspace_branches",
            ] {
                assert!(found.contains(expected), "missing {expected} in {found:?}");
            }
            // Referenced only inside CREATE FUNCTION bodies or GRANT lists.
            for absent in [
                "session_inputs",
                "repos",
                "workspace_flushes",
                "workspace_repos",
            ] {
                assert!(!found.contains(absent), "unexpected {absent} in {found:?}");
            }
            // CTE aliases are not reported.
            for alias in ["missing", "inserted"] {
                assert!(
                    !found.contains(alias),
                    "CTE alias {alias} reported in {found:?}"
                );
            }
        }

        #[test]
        fn dollar_quotes_with_tags_and_nested_dollars() {
            let stripped = strip_for_scan("A $x$ inside $$ still $x$ B $$ c $$ D");
            assert_eq!(
                stripped.split_whitespace().collect::<Vec<_>>(),
                vec!["A", "B", "D"]
            );
            assert_eq!(dollar_delimiter("$1$"), None);
            assert_eq!(dollar_delimiter("$tag_1$rest"), Some("$tag_1$".to_string()));
            assert_eq!(dollar_delimiter_at("col$x$ rest", 3), None);
            assert_eq!(
                dollar_delimiter_at("x $x$ rest", 2),
                Some("$x$".to_string())
            );
            assert_eq!(
                tables(
                    "SELECT col$x$ FROM public.dollar_ident; UPDATE public.after_ident SET a = 1;"
                ),
                vec!["after_ident", "dollar_ident"]
            );
            assert!(opens_do_block("stuff; DO "));
            assert!(opens_do_block("DO LANGUAGE plpgsql "));
            assert!(!opens_do_block(
                "CREATE FUNCTION f() RETURNS void LANGUAGE sql AS "
            ));
            assert!(opens_role_name("SET LOCAL ROLE "));
            assert!(!opens_role_name("ALTER ROLE "));
        }
    }
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// URL redaction utility
// ---------------------------------------------------------------------------

/// Redact credentials from a PostgreSQL connection URL for safe logging.
///
/// Replaces the userinfo portion (user:password) and secret-bearing query
/// parameter values with `***` while retaining non-secret connection options.
///
/// # Examples
///
/// ```
/// # use g5_api_data::db::redact_url;
/// assert_eq!(
///     redact_url("postgres://user:password@host:5432/g5"),
///     "postgres://***@host:5432/g5"
/// );
/// assert_eq!(
///     redact_url("postgres://host:5432/g5"),
///     "postgres://host:5432/g5"
/// );
/// ```
#[must_use]
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |offset| authority_start + offset);

    let redacted = if let Some(at_offset) = url[authority_start..authority_end].rfind('@') {
        let at = authority_start + at_offset;
        format!("{}***{}", &url[..authority_start], &url[at..])
    } else {
        url.to_string()
    };

    let Some(query_start) = redacted.find('?') else {
        return redacted;
    };
    let query_end = redacted[query_start + 1..]
        .find('#')
        .map_or(redacted.len(), |offset| query_start + 1 + offset);
    let query = &redacted[query_start + 1..query_end];
    if !query.split('&').any(|part| {
        part.split_once('=')
            .is_some_and(|(key, _)| is_secret_query_key(key))
    }) {
        return redacted;
    }

    let mut output = String::with_capacity(redacted.len());
    output.push_str(&redacted[..query_start + 1]);
    for (index, part) in query.split('&').enumerate() {
        if index > 0 {
            output.push('&');
        }
        match part.split_once('=') {
            Some((key, _)) if is_secret_query_key(key) => {
                output.push_str(key);
                output.push_str("=***");
            }
            _ => output.push_str(part),
        }
    }
    output.push_str(&redacted[query_end..]);
    output
}

fn is_secret_query_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }

    [
        b"password".as_slice(),
        b"passfile",
        b"sslpassword",
        b"sslkey",
    ]
    .iter()
    .any(|secret| decoded.eq_ignore_ascii_case(secret))
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_redact_url_with_credentials() {
        assert_eq!(
            redact_url("postgres://user:password@host:5432/g5"),
            "postgres://***@host:5432/g5"
        );
    }

    #[test]
    fn test_redact_url_user_only() {
        assert_eq!(
            redact_url("postgres://user@host:5432/g5"),
            "postgres://***@host:5432/g5"
        );
    }

    #[test]
    fn test_redact_url_no_credentials() {
        assert_eq!(
            redact_url("postgres://host:5432/g5"),
            "postgres://host:5432/g5"
        );
    }

    #[test]
    fn test_redact_url_no_scheme() {
        assert_eq!(redact_url("host:5432/g5"), "host:5432/g5");
    }

    #[test]
    fn test_redact_url_complex_password() {
        assert_eq!(
            redact_url("postgres://admin:p%40ss%3Aword@db.example.com:5432/mydb?sslmode=require"),
            "postgres://***@db.example.com:5432/mydb?sslmode=require"
        );
    }

    #[test]
    fn test_redact_url_query_password_preserves_safe_options() {
        assert_eq!(
            redact_url(
                "postgres://db.example.com:5432/mydb?user=g5&password=secret&sslmode=require"
            ),
            "postgres://db.example.com:5432/mydb?user=g5&password=***&sslmode=require"
        );
    }

    #[test]
    fn test_redact_url_at_sign_outside_authority_is_not_userinfo() {
        assert_eq!(
            redact_url(
                "postgres://db.example.com/mydb?application_name=ops@example.com&password=secret"
            ),
            "postgres://db.example.com/mydb?application_name=ops@example.com&password=***"
        );
    }

    #[test]
    fn test_redact_url_encoded_secret_key_preserves_fragment() {
        assert_eq!(
            redact_url("postgres://db.example.com/mydb?pass%77ord=secret&sslmode=require#primary"),
            "postgres://db.example.com/mydb?pass%77ord=***&sslmode=require#primary"
        );
    }

    #[test]
    fn test_redact_url_empty() {
        assert_eq!(redact_url(""), "");
    }
}
