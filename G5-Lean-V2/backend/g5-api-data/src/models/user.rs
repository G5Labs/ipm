use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a row in the `users` table (API_AND_STORAGE.md §2.1).
///
/// A G5 platform user authenticated via GitHub OAuth.
/// This is a passive data container — no business logic, no database queries.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub oauth_provider: Option<String>,
    pub oauth_id: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Highest onboarding-tour step reached (users g5.user.onboarding):
    /// `None` until the welcome card is shown, 7 when finished.
    pub onboarding_step: Option<i16>,
    /// Whether the person skipped the tour at `onboarding_step`.
    pub onboarding_skipped: bool,
}
