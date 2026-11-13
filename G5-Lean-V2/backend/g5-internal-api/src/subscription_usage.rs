//! Best-effort display observations, never execution or billing records.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionUsage {
    pub five_hour_utilization: Option<f64>,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    pub seven_day_utilization: Option<f64>,
    pub seven_day_reset_at: Option<DateTime<Utc>>,
    /// Oldest observation included, so a sparse update cannot freshen old data.
    pub captured_at: DateTime<Utc>,
}

impl SubscriptionUsage {
    pub fn valid(&self) -> bool {
        let values = [self.five_hour_utilization, self.seven_day_utilization];
        self.captured_at <= Utc::now() + chrono::Duration::seconds(60)
            && values.iter().any(Option::is_some)
            && values
                .into_iter()
                .flatten()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionUsageReport {
    pub task_id: Uuid,
    pub attempt: u32,
    pub usage: SubscriptionUsage,
}
