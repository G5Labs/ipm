//! Passive quota observations. Never send these through the durable event journal.
use chrono::{DateTime, Utc};
use g5_contracts::types::AgentAuthModeV1;
use g5_internal_api::{
    subscription_usage::{SubscriptionUsage, SubscriptionUsageReport},
    InternalApiClient,
};
use serde_json::Value;
use tokio::{sync::watch, task::JoinHandle};

use crate::types::{AgentTurnInputV1, ProviderId};

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    utilization: f64,
    reset_at: Option<DateTime<Utc>>,
    captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

fn window(value: &Value, percent: bool, now: DateTime<Utc>) -> Option<Window> {
    let utilization = value[if percent {
        "usedPercent"
    } else {
        "utilization"
    }]
    .as_f64()?
        / if percent { 100.0 } else { 1.0 };
    if !utilization.is_finite() || !(0.0..=1.0).contains(&utilization) {
        return None;
    }
    let reset_at = value["resetsAt"]
        .as_i64()
        .and_then(|v| DateTime::from_timestamp(v, 0));
    if reset_at.is_some_and(|reset| reset <= now) {
        return None;
    }
    Some(Window {
        utilization,
        reset_at,
        captured_at: now,
    })
}

impl Observation {
    pub fn claude(frame: &Value) -> Option<Self> {
        let now = Utc::now();
        let info = &frame["rate_limit_info"];
        let mut observation = Self {
            five_hour: window(&info["unifiedWindows"]["five_hour"], false, now),
            seven_day: window(&info["unifiedWindows"]["seven_day"], false, now),
        };
        // Older frames can report only the representative window. Never turn
        // a status/threshold or a model-specific allowance into a percentage.
        match info["rateLimitType"].as_str() {
            Some("five_hour") if observation.five_hour.is_none() => {
                observation.five_hour = window(info, false, now)
            }
            Some("seven_day") if observation.seven_day.is_none() => {
                observation.seven_day = window(info, false, now)
            }
            _ => {}
        }
        observation.any().then_some(observation)
    }

    pub fn codex(frame: &Value) -> Option<Self> {
        let limits = &frame["params"]["rateLimits"];
        if !limits["limitId"].is_null() && limits["limitId"] != "codex" {
            return None;
        }
        let now = Utc::now();
        let mut observation = Self::default();
        for name in ["primary", "secondary"] {
            let value = &limits[name];
            match value["windowDurationMins"].as_i64() {
                Some(300) => observation.five_hour = window(value, true, now),
                Some(10080) => observation.seven_day = window(value, true, now),
                _ => {}
            }
        }
        observation.any().then_some(observation)
    }

    fn any(&self) -> bool {
        self.five_hour.is_some() || self.seven_day.is_some()
    }

    fn merge(&mut self, newer: Self) -> Option<SubscriptionUsage> {
        let now = Utc::now();
        for (old, new) in [
            (&mut self.five_hour, newer.five_hour),
            (&mut self.seven_day, newer.seven_day),
        ] {
            if new.is_some() {
                *old = new;
            }
            if old
                .as_ref()
                .is_some_and(|v| v.reset_at.is_some_and(|reset| reset <= now))
            {
                *old = None;
            }
        }
        let captured_at = [&self.five_hour, &self.seven_day]
            .into_iter()
            .flatten()
            .map(|v| v.captured_at)
            .min()?;
        Some(SubscriptionUsage {
            five_hour_utilization: self.five_hour.as_ref().map(|v| v.utilization),
            five_hour_reset_at: self.five_hour.as_ref().and_then(|v| v.reset_at),
            seven_day_utilization: self.seven_day.as_ref().map(|v| v.utilization),
            seven_day_reset_at: self.seven_day.as_ref().and_then(|v| v.reset_at),
            captured_at,
        })
    }
}

/// At most one in-flight report and one coalesced snapshot. Dropping the turn
/// cancels delivery; neither a slow API nor a failed report changes execution.
pub(crate) struct Reporter {
    observations: Observation,
    send: watch::Sender<Option<SubscriptionUsage>>,
    task: JoinHandle<()>,
}

impl Reporter {
    pub fn start(input: &AgentTurnInputV1, client: Option<InternalApiClient>) -> Option<Self> {
        if !matches!(
            (input.provider.provider_id, input.provider.auth_mode),
            (
                ProviderId::ClaudeCode,
                Some(AgentAuthModeV1::AnthropicOauth)
            ) | (
                ProviderId::Codex,
                Some(AgentAuthModeV1::ChatGptSubscription)
            )
        ) {
            return None;
        }
        let client = client?;
        let task_id = input.task.task_id.parse().ok()?;
        let attempt = input.task.attempt;
        let session_id = input.task.session_id.clone();
        let (send, mut receive) = watch::channel(None::<SubscriptionUsage>);
        let task = tokio::spawn(async move {
            while receive.changed().await.is_ok() {
                let Some(usage) = receive.borrow_and_update().clone() else {
                    continue;
                };
                let report = SubscriptionUsageReport {
                    task_id,
                    attempt,
                    usage,
                };
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    client.report_subscription_usage(&session_id, &report),
                )
                .await;
            }
        });
        Some(Self {
            observations: Observation::default(),
            send,
            task,
        })
    }

    pub fn observe(&mut self, observation: Observation) {
        if let Some(usage) = self.observations.merge(observation) {
            self.send.send_replace(Some(usage));
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_native_windows_and_legacy_representative_have_known_units() {
        let frame = json!({"rate_limit_info": {"status":"allowed", "unifiedWindows": {
            "five_hour":{"utilization":0.23,"resetsAt":4102444800_i64},
            "seven_day":{"utilization":0.45,"resetsAt":4103049600_i64}
        }}});
        let value = Observation::default()
            .merge(Observation::claude(&frame).unwrap())
            .unwrap();
        assert_eq!(value.five_hour_utilization, Some(0.23));
        assert_eq!(value.seven_day_utilization, Some(0.45));
        assert!(Observation::claude(&json!({"rate_limit_info":{"status":"allowed"}})).is_none());
        assert!(Observation::claude(
            &json!({"rate_limit_info":{"rateLimitType":"seven_day_opus","utilization":0.4}})
        )
        .is_none());
        let legacy = Observation::claude(
            &json!({"rate_limit_info":{"rateLimitType":"five_hour","utilization":0.0}}),
        )
        .unwrap();
        assert_eq!(legacy.five_hour.unwrap().utilization, 0.0);
    }

    #[test]
    fn codex_duration_and_bucket_determine_meaning_not_primary_position() {
        let frame = json!({"params":{"rateLimits":{"limitId":"codex",
            "primary":{"usedPercent":45,"windowDurationMins":10080},
            "secondary":{"usedPercent":23,"windowDurationMins":300}}}});
        let observation = Observation::codex(&frame).unwrap();
        assert_eq!(observation.five_hour.unwrap().utilization, 0.23);
        assert_eq!(observation.seven_day.unwrap().utilization, 0.45);
        for bucket in [json!("codex_other"), json!(42)] {
            let mut invalid = frame.clone();
            invalid["params"]["rateLimits"]["limitId"] = bucket;
            assert!(Observation::codex(&invalid).is_none());
        }
        let mut other_window = frame;
        other_window["params"]["rateLimits"]["primary"]["windowDurationMins"] = json!(60);
        other_window["params"]["rateLimits"]["secondary"] = Value::Null;
        assert!(Observation::codex(&other_window).is_none());
    }

    #[test]
    fn invalid_or_expired_values_never_become_zero_and_sparse_updates_keep_age() {
        for value in [json!(null), json!("0.5"), json!(-0.1), json!(1.1)] {
            assert!(window(&json!({"utilization":value}), false, Utc::now()).is_none());
        }
        assert!(window(&json!({"utilization":0.5,"resetsAt":1}), false, Utc::now()).is_none());
        let old_time = Utc::now() - chrono::Duration::minutes(20);
        let mut state = Observation {
            five_hour: Some(Window {
                utilization: 0.2,
                reset_at: None,
                captured_at: old_time,
            }),
            seven_day: None,
        };
        let new = Observation::claude(
            &json!({"rate_limit_info":{"rateLimitType":"seven_day","utilization":0.5}}),
        )
        .unwrap();
        let value = state.merge(new).unwrap();
        assert_eq!(value.five_hour_utilization, Some(0.2));
        assert_eq!(value.seven_day_utilization, Some(0.5));
        assert_eq!(value.captured_at, old_time);
        state.five_hour.as_mut().unwrap().reset_at =
            Some(Utc::now() - chrono::Duration::seconds(1));
        assert!(state
            .merge(Observation::default())
            .unwrap()
            .five_hour_utilization
            .is_none());
    }
}
