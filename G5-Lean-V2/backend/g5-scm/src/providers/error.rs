use crate::FailureEffect;

/// Provider evidence for caller-decided recovery. There is no retry scheduling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderErrorKind {
    InvalidInput,
    UnsupportedCapability,
    Initialization,
    Transport,
    Deadline,
    Rejected,
    InvalidResponse,
    RateLimited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub operation: &'static str,
    pub target: Option<Box<str>>,
    pub kind: ProviderErrorKind,
    pub effect: FailureEffect,
    pub status: Option<u16>,
    pub request_id: Option<Box<str>>,
    pub retry_after: Option<Box<str>>,
    pub rate_limit_reset: Option<Box<str>>,
    pub rate_limited: bool,
    pub message: String,
}

impl ProviderError {
    pub(super) fn new(
        operation: &'static str,
        kind: ProviderErrorKind,
        effect: FailureEffect,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation,
            target: None,
            kind,
            effect,
            status: None,
            request_id: None,
            retry_after: None,
            rate_limit_reset: None,
            rate_limited: false,
            message: message.into(),
        }
    }
    /// When a rate-limited caller may retry, from GitHub's `Retry-After`
    /// seconds or else its `X-RateLimit-Reset` epoch; `None` when neither is
    /// usable or the failure was not a rate limit.
    pub fn retry_at(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        if !self.rate_limited {
            return None;
        }
        if let Some(seconds) = self
            .retry_after
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok())
        {
            return Some(now + chrono::Duration::seconds(i64::from(seconds)));
        }
        let reset = self
            .rate_limit_reset
            .as_deref()?
            .trim()
            .parse::<i64>()
            .ok()?;
        chrono::DateTime::from_timestamp(reset, 0).map(|at| at.max(now))
    }
    pub(super) fn invalid(operation: &'static str, message: &'static str) -> Self {
        Self::new(
            operation,
            ProviderErrorKind::InvalidInput,
            FailureEffect::Refused,
            message,
        )
    }
    pub(super) fn unsupported(operation: &'static str) -> Self {
        Self::new(
            operation,
            ProviderErrorKind::UnsupportedCapability,
            FailureEffect::Refused,
            "The selected provider does not support this operation",
        )
    }
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Provider {} failed", self.operation)?;
        // Outcome guidance comes first so bounded owner status/event fields
        // retain the recovery decision even when provider diagnostics are long.
        match self.effect {
            FailureEffect::Refused => write!(f, "; this invocation made no provider changes")?,
            FailureEffect::Partial => write!(f, "; an effect was confirmed but completion failed; inspect current state before retrying")?,
            FailureEffect::Unknown => write!(f, "; the result is unknown; inspect current state before retrying")?,
        }
        if let Some(status) = self.status {
            write!(f, "; HTTP {status}")?;
        }
        if self.rate_limited {
            write!(f, "; rate limited")?;
        }
        // Full evidence remains in the owned fields; presentation reserves room
        // for the outcome, provider hints and the beginning of the diagnostic.
        if let Some(id) = &self.request_id {
            write!(f, "; request {}", id.chars().take(64).collect::<String>())?;
        }
        if let Some(after) = &self.retry_after {
            write!(
                f,
                "; retry-after {}",
                after.chars().take(32).collect::<String>()
            )?;
        } else if let Some(reset) = &self.rate_limit_reset {
            write!(
                f,
                "; rate limit resets at {}",
                reset.chars().take(32).collect::<String>()
            )?;
        }
        if let Some(target) = &self.target {
            write!(
                f,
                "; target {}",
                target.chars().take(96).collect::<String>()
            )?;
        }
        write!(f, ": {}", self.message)
    }
}
impl std::error::Error for ProviderError {}
impl From<ProviderError> for g5_contracts::G5Error {
    fn from(error: ProviderError) -> Self {
        let message = error.to_string();
        match error.kind {
            ProviderErrorKind::InvalidInput => Self::ValidationError {
                details: serde_json::json!({"message": message}),
            },
            ProviderErrorKind::Rejected
                if error
                    .status
                    .is_some_and(|status| (500..600).contains(&status)) =>
            {
                Self::ServiceUnavailable { message }
            }
            ProviderErrorKind::UnsupportedCapability | ProviderErrorKind::Rejected => {
                Self::Conflict { reason: message }
            }
            ProviderErrorKind::Initialization
            | ProviderErrorKind::Transport
            | ProviderErrorKind::Deadline
            | ProviderErrorKind::InvalidResponse
            | ProviderErrorKind::RateLimited => Self::ServiceUnavailable { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_errors_distinguish_refusal_from_provider_unavailability() {
        use ProviderErrorKind::*;
        for (kind, expected) in [
            (InvalidInput, 422),
            (UnsupportedCapability, 409),
            (Rejected, 409),
            (Initialization, 503),
            (Transport, 503),
            (Deadline, 503),
            (InvalidResponse, 503),
            (RateLimited, 503),
        ] {
            for effect in [
                FailureEffect::Refused,
                FailureEffect::Partial,
                FailureEffect::Unknown,
            ] {
                let mut error =
                    ProviderError::new("read branch", kind.clone(), effect, "diagnostic");
                error.status = Some(404);
                let original = error.to_string();
                let mapped = g5_contracts::G5Error::from(error);
                assert_eq!(mapped.status_code().as_u16(), expected, "{kind:?}");
                assert!(mapped.to_string().contains(&original));
            }
        }
    }

    #[test]
    fn provider_server_failures_preserve_status_and_effect_as_unavailable() {
        for status in [500, 502, 503, 504] {
            for effect in [
                FailureEffect::Refused,
                FailureEffect::Partial,
                FailureEffect::Unknown,
            ] {
                let mut error = ProviderError::new(
                    "provider request",
                    ProviderErrorKind::Rejected,
                    effect,
                    "upstream failed",
                );
                error.status = Some(status);
                let original = error.to_string();
                let mapped = g5_contracts::G5Error::from(error);
                assert_eq!(mapped.status_code().as_u16(), 503);
                assert!(mapped.to_string().contains(&original));
            }
        }
    }
}
