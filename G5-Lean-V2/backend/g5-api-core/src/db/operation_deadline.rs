//! Server-side query bounds for an explicitly bounded owner operation.
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::G5Error;

pub(crate) async fn apply(
    db: &mut ScopedDb,
    deadline: Option<DateTime<Utc>>,
) -> Result<(), G5Error> {
    if let Some(deadline) = deadline {
        if deadline <= Utc::now() {
            return Err(G5Error::ServiceUnavailable {
                message: "Operation deadline exceeded".into(),
            });
        }
        let remaining = deadline
            .signed_duration_since(Utc::now())
            .num_milliseconds()
            .saturating_add(1)
            .clamp(1, i64::from(i32::MAX));
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{remaining}ms"))
            .execute(db.conn())
            .await?;
    }
    Ok(())
}
