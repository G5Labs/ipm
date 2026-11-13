//! `ScopedDb` — Transaction wrapper with RLS session context.
//!
//! Every tenant-data query goes through a `ScopedDb` instance. On creation,
//! it begins a transaction and calls `authz.set_ctx(session_id)` to activate
//! RLS scoping. On commit, it calls `authz.clear_ctx()` then `COMMIT`.
//! On drop without commit, sqlx's `Transaction::Drop` issues `ROLLBACK`,
//! which automatically undoes the `set_ctx` INSERT.
//!
//! # Lifecycle
//!
//! ```ignore
//! let cap: DbCapability = authz.mint_db_capability(user_id).await?;
//! let mut db = ScopedDb::from_capability(&pool, cap).await?;
//! let rows = sqlx::query("SELECT * FROM workspaces")
//!     .fetch_all(db.conn())
//!     .await?;
//! db.commit().await?;  // clear_ctx() + COMMIT
//! // or just drop `db` — ROLLBACK undoes set_ctx INSERT
//! ```
//!
//! # Safety
//!
//! - `set_ctx()` MUST execute inside a transaction (never autocommit).
//! - `current_org_ids()` returns NULL when no session → fail-closed.
//! - `clear_ctx()` failure on commit is **fatal** — the transaction is aborted.

use g5_contracts::types::ids::{OrgId, SessionId, UserId};
use sqlx::{postgres::PgConnection, PgPool, Postgres, Transaction};

use crate::capability::DbCapability;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors specific to scoped database operations.
#[derive(Debug, thiserror::Error)]
pub enum ScopedDbError {
    /// Failed to begin transaction or set session context.
    #[error("Failed to initialize scoped session: {0}")]
    InitFailed(#[source] sqlx::Error),

    /// Failed to clear context on commit.
    #[error("Failed to clear session context: {0}")]
    ClearFailed(#[source] sqlx::Error),

    /// Failed to commit transaction.
    #[error("Transaction commit failed: {0}")]
    CommitFailed(#[source] sqlx::Error),

    /// The capability's session was expired or invalid.
    #[error("Invalid or expired session: {session_id}")]
    InvalidSession { session_id: SessionId },

    /// The inner transaction has already been consumed by `commit()` or
    /// `rollback()`.  This is a programming error — callers should not
    /// attempt to use the connection after consuming it.
    #[error("ScopedDb transaction already consumed (committed or rolled back)")]
    TransactionConsumed,
}

impl ScopedDbError {
    /// Whether an error returned by `ScopedDb::commit` proves COMMIT was not
    /// sent or was refused. Transport loss and unknown server completion do
    /// not authorize compensation of external effects.
    #[must_use]
    pub fn commit_was_refused(&self) -> bool {
        match self {
            Self::ClearFailed(_) => true,
            Self::CommitFailed(error) => error
                .as_database_error()
                .and_then(|error| error.try_downcast_ref::<sqlx::postgres::PgDatabaseError>())
                .is_some_and(|error| {
                    error.severity() == sqlx::postgres::PgSeverity::Error
                        && !error.code().starts_with("08")
                        && error.code() != "40003"
                }),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// ScopedDb
// ---------------------------------------------------------------------------

/// A database transaction scoped to a set of orgs via RLS session context.
///
/// ## Invariants
///
/// 1. A `ScopedDb` always has an active transaction.
/// 2. `authz.set_ctx(session_id)` has been called on the transaction's connection.
/// 3. All queries through `tx()` are subject to `current_org_ids()` RLS policies.
/// 4. `commit()` clears context then commits. Drop without commit rolls back.
pub struct ScopedDb {
    tx: Option<Transaction<'static, Postgres>>,
    pool: PgPool,
    org_ids: Vec<OrgId>,
    actor_id: UserId,
    scopes: Vec<String>,
    session_id: SessionId,
    accessed: bool,
    rollback_actions: Vec<Box<dyn FnOnce() + Send + Sync>>,
    commit_actions: Vec<Box<dyn FnOnce() + Send + Sync>>,
}

/// Transaction-free handle that can reopen the same authorized RLS scope.
///
/// Long-running Git, live workspace file, and network operations must not retain an idle
/// [`ScopedDb`] transaction merely so they can open a later application
/// transaction. Capture this handle, consume the current transaction, perform
/// the external work, then call [`ScopedDbReopener::open`] for the next short
/// database phase.
#[derive(Clone)]
pub struct ScopedDbReopener {
    pool: PgPool,
    capability: DbCapability,
}

impl ScopedDbReopener {
    /// Open a fresh transaction with the captured reusable RLS session.
    pub async fn open(&self) -> Result<ScopedDb, ScopedDbError> {
        ScopedDb::from_capability(&self.pool, self.capability.clone()).await
    }

    /// The org IDs the captured session is scoped to.
    #[must_use]
    pub fn org_ids(&self) -> &[OrgId] {
        &self.capability.org_ids
    }
}

impl ScopedDb {
    /// Create a `ScopedDb` from a `DbCapability`.
    ///
    /// 1. Acquires a connection from the pool and begins a transaction.
    /// 2. Calls `authz.set_ctx(session_id)` to activate RLS scoping.
    /// 3. Returns the scoped transaction wrapper.
    ///
    /// If `set_ctx()` raises an exception (expired/invalid session), the
    /// transaction is rolled back and [`ScopedDbError::InvalidSession`] is
    /// returned.
    pub async fn from_capability(
        pool: &PgPool,
        capability: DbCapability,
    ) -> Result<Self, ScopedDbError> {
        let mut tx = pool.begin().await.map_err(ScopedDbError::InitFailed)?;

        // Activate RLS session context
        sqlx::query("SELECT authz.set_ctx($1)")
            .bind(capability.session_id.into_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                // Check if this is a "no_data_found" error from set_ctx()
                if let sqlx::Error::Database(ref db_err) = e {
                    if db_err.code().as_deref() == Some("P0002") {
                        return ScopedDbError::InvalidSession {
                            session_id: capability.session_id,
                        };
                    }
                }
                ScopedDbError::InitFailed(e)
            })?;

        let active: bool = sqlx::query_scalar("SELECT authz.session_memberships_active()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ScopedDbError::InitFailed)?;
        if !active {
            return Err(ScopedDbError::InvalidSession {
                session_id: capability.session_id,
            });
        }

        Ok(Self {
            tx: Some(tx),
            pool: pool.clone(),
            org_ids: capability.org_ids,
            actor_id: capability.actor_id,
            scopes: capability.scopes,
            session_id: capability.session_id,
            accessed: false,
            rollback_actions: Vec::new(),
            commit_actions: Vec::new(),
        })
    }

    /// Create a `ScopedDb` without calling `set_ctx()`.
    ///
    /// Used in development/test mode where RLS is not active.
    /// Still wraps a transaction for consistency.
    pub async fn unscoped(pool: &PgPool, capability: DbCapability) -> Result<Self, ScopedDbError> {
        let tx = pool.begin().await.map_err(ScopedDbError::InitFailed)?;

        Ok(Self {
            tx: Some(tx),
            pool: pool.clone(),
            org_ids: capability.org_ids,
            actor_id: capability.actor_id,
            scopes: capability.scopes,
            session_id: capability.session_id,
            accessed: false,
            rollback_actions: Vec::new(),
            commit_actions: Vec::new(),
        })
    }

    /// Get a mutable reference to the inner transaction.
    ///
    /// All queries MUST go through this reference to be subject to RLS.
    ///
    /// # Panics
    ///
    /// Panics if called after `commit()` or `rollback()`.
    pub fn tx(&mut self) -> &mut Transaction<'static, Postgres> {
        self.accessed = true;
        self.tx
            .as_mut()
            .expect("ScopedDb transaction already consumed (committed or rolled back)")
    }

    /// Non-panicking variant of [`tx()`](Self::tx).
    ///
    /// Returns `Err(ScopedDbError::TransactionConsumed)` instead of
    /// panicking when the transaction has already been consumed.
    pub fn try_tx(&mut self) -> Result<&mut Transaction<'static, Postgres>, ScopedDbError> {
        self.accessed = true;
        self.tx.as_mut().ok_or(ScopedDbError::TransactionConsumed)
    }

    /// Get a mutable reference to the underlying database connection.
    ///
    /// This is the concrete type (`PgConnection`) that implements sqlx's
    /// `Executor` trait. Use this when passing to `sqlx::query!()` methods:
    ///
    /// ```ignore
    /// sqlx::query!("SELECT ...").fetch_all(db.conn()).await?;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called after `commit()` or `rollback()`.
    pub fn conn(&mut self) -> &mut PgConnection {
        self.accessed = true;
        self.tx
            .as_deref_mut()
            .expect("ScopedDb transaction already consumed (committed or rolled back)")
    }

    /// Non-panicking variant of [`conn()`](Self::conn).
    ///
    /// Returns `Err(ScopedDbError::TransactionConsumed)` instead of
    /// panicking when the transaction has already been consumed.
    pub fn try_conn(&mut self) -> Result<&mut PgConnection, ScopedDbError> {
        self.accessed = true;
        self.tx
            .as_deref_mut()
            .ok_or(ScopedDbError::TransactionConsumed)
    }

    /// Whether application code has borrowed this transaction for a query.
    ///
    /// A freshly reopened transaction remains unaccessed after the internal
    /// RLS context setup. Finalizers use this bit to reject caller-owned row
    /// and advisory locks before entering their canonical lock order.
    #[must_use]
    pub fn is_unaccessed(&self) -> bool {
        !self.accessed
    }

    /// The org IDs this session is scoped to.
    #[must_use]
    pub fn org_ids(&self) -> &[OrgId] {
        &self.org_ids
    }

    /// The primary (first) org ID. Returns error if no orgs.
    pub fn primary_org_id(&self) -> Result<OrgId, ScopedDbError> {
        self.org_ids
            .first()
            .copied()
            .ok_or_else(|| ScopedDbError::InvalidSession {
                session_id: self.session_id,
            })
    }

    /// The actor (user or worker) ID.
    #[must_use]
    pub fn actor_id(&self) -> UserId {
        self.actor_id
    }

    /// The effective scopes for this session.
    #[must_use]
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// The session ID (references `authz.user_sessions`).
    #[must_use]
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Capture this transaction's reusable authorization context without
    /// opening another transaction.
    #[must_use]
    pub fn reopener(&self) -> ScopedDbReopener {
        ScopedDbReopener {
            pool: self.pool.clone(),
            capability: DbCapability {
                org_ids: self.org_ids.clone(),
                actor_id: self.actor_id,
                scopes: self.scopes.clone(),
                session_id: self.session_id,
                // `from_capability` validates the reusable session through
                // `authz.set_ctx`; it does not consume this informational TTL.
                ttl_secs: 0,
            },
        }
    }

    /// Create an independent `ScopedDb` that shares the same RLS session.
    ///
    /// Opens a **new** transaction from the pool, calls `set_ctx()` with the
    /// same `session_id`, and returns a second `ScopedDb`.  The forked
    /// instance has its own transaction and must be committed or dropped
    /// independently.
    ///
    /// Use this when you need concurrent DB access within a single request
    /// (e.g. parallel search legs), since a single PostgreSQL connection
    /// cannot run queries concurrently.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut db2 = db.fork().await?;
    /// let (a, b) = tokio::join!(
    ///     sqlx::query("SELECT …").fetch_all(db.conn()),
    ///     sqlx::query("SELECT …").fetch_all(db2.conn()),
    /// );
    /// db2.commit().await?;
    /// db.commit().await?;
    /// ```
    pub async fn fork(&self) -> Result<Self, ScopedDbError> {
        self.reopener().open().await
    }

    /// Keep external provisional resources tied to this transaction. Actions
    /// run when rollback is queued/confirmed (including drop or a rejected
    /// commit), and are discarded after success. Forks do not inherit them.
    /// An uncertain/cancelled commit preserves resources for reconciliation.
    pub fn on_rollback(&mut self, action: impl FnOnce() + Send + Sync + 'static) {
        self.rollback_actions.push(Box::new(action));
    }

    /// Run `action` once this transaction has committed, so consumers are
    /// notified only of stored facts. Discarded on rollback, on drop without
    /// commit, and when the commit fails or its outcome is uncertain: a
    /// reconnecting reader recovers the stored state instead. Forks do not
    /// inherit it.
    pub fn on_commit(&mut self, action: impl FnOnce() + Send + Sync + 'static) {
        self.commit_actions.push(Box::new(action));
    }

    /// Position in the after-commit queue, for work that may be undone by
    /// `ROLLBACK TO SAVEPOINT`: pass it to
    /// [`discard_commit_actions_since`](Self::discard_commit_actions_since)
    /// when that savepoint is rolled back.
    #[must_use]
    pub fn commit_actions_mark(&self) -> usize {
        self.commit_actions.len()
    }

    /// Drop the after-commit actions registered after `mark`, because the
    /// statements they announce were rolled back to a savepoint.
    pub fn discard_commit_actions_since(&mut self, mark: usize) {
        self.commit_actions.truncate(mark);
    }

    /// Commit the transaction.
    ///
    /// 1. Calls `authz.clear_ctx()` to remove the session context for this PID.
    /// 2. Commits the transaction.
    ///
    /// If `clear_ctx()` fails, the transaction is aborted (rolled back).
    /// This is a fatal error — stale context must never persist.
    pub async fn commit(mut self) -> Result<(), ScopedDbError> {
        let mut tx = self
            .tx
            .take()
            .expect("ScopedDb transaction already consumed");

        sqlx::query("SELECT authz.clear_ctx()")
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(
                    session_id = %self.session_id,
                    error = %e,
                    "FATAL: clear_ctx() failed — aborting transaction"
                );
                ScopedDbError::ClearFailed(e)
            })?;

        // Once COMMIT is in flight, cancellation or a transport error cannot
        // prove rollback. Drop the callbacks without running them in those
        // cases; deleting a resource of a committed row would be unsafe.
        let actions = std::mem::take(&mut self.rollback_actions);
        let committed_actions = std::mem::take(&mut self.commit_actions);
        match tx.commit().await {
            Ok(()) => {
                for action in committed_actions {
                    action();
                }
                Ok(())
            }
            Err(error) => {
                // A server ErrorResponse is not always rollback evidence:
                // FATAL/PANIC disconnects and unknown-completion SQLSTATEs
                // can leave the client unable to determine the outcome.
                let error = ScopedDbError::CommitFailed(error);
                if error.commit_was_refused() {
                    for action in actions {
                        action();
                    }
                }
                Err(error)
            }
        }
    }

    /// Rollback the transaction explicitly.
    ///
    /// Usually not needed — dropping `ScopedDb` without calling `commit()`
    /// automatically rolls back the transaction.
    pub async fn rollback(mut self) -> Result<(), ScopedDbError> {
        let tx = self
            .tx
            .take()
            .expect("ScopedDb transaction already consumed");
        tx.rollback().await.map_err(ScopedDbError::CommitFailed)
    }
}

impl Drop for ScopedDb {
    fn drop(&mut self) {
        if self.tx.is_some() {
            tracing::trace!(
                session_id = %self.session_id,
                "ScopedDb dropped without commit — transaction will ROLLBACK"
            );
            // Queue SQL rollback before compensating external resources.
            drop(self.tx.take());
        }
        for action in self.rollback_actions.drain(..) {
            action();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_db_error_display() {
        let err = ScopedDbError::InvalidSession {
            session_id: SessionId::nil(),
        };
        assert!(err.to_string().contains("Invalid or expired session"));
    }

    #[test]
    fn scoped_db_error_init_display() {
        let err = ScopedDbError::InitFailed(sqlx::Error::PoolTimedOut);
        assert!(err.to_string().contains("Failed to initialize"));
    }

    #[test]
    fn scoped_db_error_clear_display() {
        let err = ScopedDbError::ClearFailed(sqlx::Error::PoolTimedOut);
        assert!(err.to_string().contains("clear session context"));
        assert!(err.commit_was_refused());
    }

    #[test]
    fn scoped_db_error_commit_display() {
        let err = ScopedDbError::CommitFailed(sqlx::Error::PoolTimedOut);
        assert!(err.to_string().contains("commit failed"));
        assert!(!err.commit_was_refused());
    }
}
