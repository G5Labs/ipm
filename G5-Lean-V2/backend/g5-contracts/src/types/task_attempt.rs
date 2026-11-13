//! Durable execution-attempt contracts for stable logical tasks.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        #[non_exhaustive]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_enum!(TaskAttemptDisposition {
    Completed => "completed",
    Interrupted => "interrupted",
    Blocked => "blocked",
    Failed => "failed",
    Cancelled => "cancelled",
});

string_enum!(TaskAttemptReason {
    Completed => "completed",
    BudgetExhausted => "budget_exhausted",
    ProviderTransient => "provider_transient",
    ProviderRateLimited => "provider_rate_limited",
    ProviderAuth => "provider_auth",
    ProviderTimeout => "provider_timeout",
    WorkerLost => "worker_lost",
    LeaseExpired => "lease_expired",
    HumanInputRequired => "human_input_required",
    DependencyBlocked => "dependency_blocked",
    ManualPause => "manual_pause",
    TaskDefect => "task_defect",
    StateCorruption => "state_corruption",
    Cancelled => "cancelled",
    LegacyUnclassified => "legacy_unclassified",
});

impl TaskAttemptReason {
    pub const fn is_terminal_defect(self) -> bool {
        matches!(self, Self::TaskDefect | Self::StateCorruption)
    }
}

string_enum!(TaskAttemptRetryPolicy {
    Automatic => "automatic",
    Manual => "manual",
    None => "none",
});

string_enum!(TaskLifecyclePosture {
    Active => "active",
    Resumable => "resumable",
    ActionRequired => "action_required",
    Terminal => "terminal",
});

/// Typed evidence supplied when one execution attempt ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionEndV1 {
    pub schema_version: u8,
    pub attempt_id: Uuid,
    pub disposition: TaskAttemptDisposition,
    pub reason: TaskAttemptReason,
    pub retry_policy: TaskAttemptRetryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_summary: Option<String>,
}

impl TaskExecutionEndV1 {
    pub const SCHEMA_VERSION: u8 = 1;

    pub fn new(
        attempt_id: Uuid,
        disposition: TaskAttemptDisposition,
        reason: TaskAttemptReason,
        retry_policy: TaskAttemptRetryPolicy,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            attempt_id,
            disposition,
            reason,
            retry_policy,
            provider: None,
            provider_session_id: None,
            continuation_summary: None,
        }
    }

    /// Validate the closed vocabulary as one coherent end-of-attempt value.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != Self::SCHEMA_VERSION {
            return Err("unsupported execution-end schema version");
        }
        if self
            .provider
            .as_ref()
            .is_some_and(|value| value.parse::<super::ChatAgentProvider>().is_err())
        {
            return Err("provider must be claude_code or codex");
        }
        if self
            .provider_session_id
            .as_ref()
            .is_some_and(|value| value.chars().count() > 512)
        {
            return Err("provider session id exceeds 512 characters");
        }
        if self
            .continuation_summary
            .as_ref()
            .is_some_and(|value| value.chars().count() > 8_192)
        {
            return Err("continuation summary exceeds 8192 characters");
        }

        match self.disposition {
            TaskAttemptDisposition::Completed
                if self.reason == TaskAttemptReason::Completed
                    && self.retry_policy == TaskAttemptRetryPolicy::None => {}
            TaskAttemptDisposition::Failed
                if self.reason.is_terminal_defect()
                    && self.retry_policy == TaskAttemptRetryPolicy::None => {}
            TaskAttemptDisposition::Cancelled
                if self.reason == TaskAttemptReason::Cancelled
                    && self.retry_policy == TaskAttemptRetryPolicy::Manual => {}
            TaskAttemptDisposition::Interrupted | TaskAttemptDisposition::Blocked
                if !self.reason.is_terminal_defect()
                    && self.reason != TaskAttemptReason::Completed
                    && self.reason != TaskAttemptReason::Cancelled
                    && self.retry_policy != TaskAttemptRetryPolicy::None => {}
            _ => return Err("incoherent attempt disposition, reason, and retry policy"),
        }
        Ok(())
    }

    /// Public lifecycle posture derived solely from typed attempt evidence.
    pub const fn lifecycle_posture(&self) -> TaskLifecyclePosture {
        if self.reason.is_terminal_defect()
            || matches!(self.disposition, TaskAttemptDisposition::Completed)
        {
            TaskLifecyclePosture::Terminal
        } else if matches!(self.disposition, TaskAttemptDisposition::Cancelled) {
            TaskLifecyclePosture::ActionRequired
        } else {
            match self.retry_policy {
                TaskAttemptRetryPolicy::Automatic => TaskLifecyclePosture::Resumable,
                TaskAttemptRetryPolicy::Manual => TaskLifecyclePosture::ActionRequired,
                TaskAttemptRetryPolicy::None => TaskLifecyclePosture::Terminal,
            }
        }
    }

    /// Canonical logical-task status selected after closing this attempt.
    pub const fn logical_task_status(&self) -> &'static str {
        match self.disposition {
            TaskAttemptDisposition::Completed => "succeeded",
            TaskAttemptDisposition::Cancelled => "cancelled",
            _ if self.reason.is_terminal_defect() => "failed",
            _ => match self.retry_policy {
                TaskAttemptRetryPolicy::Automatic => "queued",
                TaskAttemptRetryPolicy::Manual => "waiting",
                TaskAttemptRetryPolicy::None => "failed",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_values_serialize_to_the_sql_vocabulary() {
        assert_eq!(
            serde_json::to_string(&TaskAttemptDisposition::Interrupted).unwrap(),
            "\"interrupted\""
        );
        assert_eq!(
            serde_json::to_string(&TaskAttemptReason::ProviderTimeout).unwrap(),
            "\"provider_timeout\""
        );
        assert_eq!(
            serde_json::to_string(&TaskAttemptRetryPolicy::Automatic).unwrap(),
            "\"automatic\""
        );
        assert_eq!(
            serde_json::to_string(&TaskLifecyclePosture::ActionRequired).unwrap(),
            "\"action_required\""
        );
    }

    #[test]
    fn typed_attempt_end_derives_canonical_logical_task_status() {
        let budget = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Interrupted,
            TaskAttemptReason::BudgetExhausted,
            TaskAttemptRetryPolicy::Manual,
        );
        assert_eq!(budget.validate(), Ok(()));
        assert_eq!(
            budget.lifecycle_posture(),
            TaskLifecyclePosture::ActionRequired
        );
        assert_eq!(budget.logical_task_status(), "waiting");

        let provider = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Interrupted,
            TaskAttemptReason::ProviderTransient,
            TaskAttemptRetryPolicy::Automatic,
        );
        assert_eq!(provider.validate(), Ok(()));
        assert_eq!(
            provider.lifecycle_posture(),
            TaskLifecyclePosture::Resumable
        );
        assert_eq!(provider.logical_task_status(), "queued");

        let completed = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Completed,
            TaskAttemptReason::Completed,
            TaskAttemptRetryPolicy::None,
        );
        assert_eq!(completed.validate(), Ok(()));
        assert_eq!(completed.logical_task_status(), "succeeded");

        let cancelled = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Cancelled,
            TaskAttemptReason::Cancelled,
            TaskAttemptRetryPolicy::Manual,
        );
        assert_eq!(cancelled.validate(), Ok(()));
        assert_eq!(cancelled.logical_task_status(), "cancelled");

        let corruption = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Failed,
            TaskAttemptReason::StateCorruption,
            TaskAttemptRetryPolicy::None,
        );
        assert_eq!(corruption.validate(), Ok(()));
        assert_eq!(
            corruption.lifecycle_posture(),
            TaskLifecyclePosture::Terminal
        );
        assert_eq!(corruption.logical_task_status(), "failed");
    }

    #[test]
    fn execution_end_accepts_only_supported_provider_ids() {
        let mut end = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Completed,
            TaskAttemptReason::Completed,
            TaskAttemptRetryPolicy::None,
        );
        assert_eq!(end.validate(), Ok(()));
        for provider in ["claude_code", "codex"] {
            end.provider = Some(provider.to_string());
            assert_eq!(end.validate(), Ok(()));
        }
        for provider in ["unsupported", "", "claude_code "] {
            end.provider = Some(provider.to_string());
            assert!(end.validate().is_err());
        }
    }

    #[test]
    fn typed_attempt_end_rejects_terminalizing_transient_failure() {
        let incoherent = TaskExecutionEndV1::new(
            Uuid::new_v4(),
            TaskAttemptDisposition::Failed,
            TaskAttemptReason::ProviderTimeout,
            TaskAttemptRetryPolicy::None,
        );
        assert!(incoherent.validate().is_err());
    }
}
