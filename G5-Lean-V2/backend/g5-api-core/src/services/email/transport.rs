//! Delivery transports: the Resend HTTP API and the in-memory capture
//! transport. Provider responses are classified here and never leave the
//! module (`g5.email.retry`, `g5.email.ambiguity`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use uuid::Uuid;

/// One message as handed to a transport.
pub(super) struct Outgoing<'a> {
    pub(super) delivery_id: Uuid,
    pub(super) sender: &'a str,
    pub(super) recipient: &'a str,
    pub(super) subject: &'a str,
    pub(super) text: &'a str,
    pub(super) html: &'a str,
}

impl Outgoing<'_> {
    /// The provider idempotency key, fixed by the delivery identity.
    pub(super) fn idempotency_key(&self) -> String {
        format!("g5-email-{}", self.delivery_id)
    }
}

/// A transport's classified answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SendOutcome {
    /// The provider accepted the message.
    Accepted { provider_message_id: Option<String> },
    /// The provider permanently refused this message; retrying cannot help.
    Rejected { code: String },
    /// Retry later. `uncertain` means the message may have been sent;
    /// `configuration` marks credential or sender errors a person must fix.
    Retry {
        code: String,
        uncertain: bool,
        configuration: bool,
        retry_after: Option<Duration>,
    },
}

#[async_trait::async_trait]
pub(super) trait Transport: Send + Sync {
    async fn send(&self, message: &Outgoing<'_>) -> SendOutcome;
}

// ---------------------------------------------------------------------------
// Resend
// ---------------------------------------------------------------------------

/// Total time for one provider call; below the worker's lease.
const RESEND_TIMEOUT: Duration = Duration::from_secs(30);
const RESEND_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct ResendTransport {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl ResendTransport {
    pub(super) fn new(api_url: &str, api_key: &str) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(RESEND_TIMEOUT)
            .connect_timeout(RESEND_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("email HTTP client initialization failed: {error}"))?;
        Ok(Self {
            client,
            endpoint: format!("{}/emails", api_url.trim_end_matches('/')),
            api_key: api_key.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl Transport for ResendTransport {
    async fn send(&self, message: &Outgoing<'_>) -> SendOutcome {
        let body = serde_json::json!({
            "from": message.sender,
            "to": [message.recipient],
            "subject": message.subject,
            "text": message.text,
            "html": message.html,
        });
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", message.idempotency_key())
            .json(&body)
            .send()
            .await;
        match response {
            Ok(response) => {
                let status = response.status().as_u16();
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(Duration::from_secs);
                let body = response.text().await.unwrap_or_default();
                classify_response(status, &body, retry_after)
            }
            Err(error) => classify_transport_error(&error),
        }
    }
}

/// Map a Resend HTTP answer onto a delivery outcome (design table
/// "Resend classification").
fn classify_response(status: u16, body: &str, retry_after: Option<Duration>) -> SendOutcome {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if (200..300).contains(&status) {
        return SendOutcome::Accepted {
            provider_message_id: parsed
                .get("id")
                .and_then(|id| id.as_str())
                .map(sanitize_code),
        };
    }
    let name = parsed
        .get("name")
        .and_then(|name| name.as_str())
        .map(sanitize_code)
        .unwrap_or_else(|| "unknown".to_string());
    let code = format!("http_{status}:{name}");
    let retry = |uncertain: bool, configuration: bool| SendOutcome::Retry {
        code: code.clone(),
        uncertain,
        configuration,
        retry_after,
    };
    match (status, name.as_str()) {
        (401 | 403, _) => retry(false, true),
        (409, "concurrent_idempotent_requests" | "resource_locked") => retry(false, false),
        (429, _) => retry(false, false),
        (400 | 409 | 422, _) => SendOutcome::Rejected { code },
        (500..=599, _) => retry(true, false),
        // Anything else (404, 405, other 4xx) is a request the provider
        // refuses as sent; treat it as a configuration problem worth retrying.
        _ => retry(false, true),
    }
}

/// A failure to connect never reached the provider; any other failure after
/// the request was sent leaves the outcome unknown.
fn classify_transport_error(error: &reqwest::Error) -> SendOutcome {
    let not_sent = error.is_connect() || error.is_builder();
    SendOutcome::Retry {
        code: if error.is_timeout() {
            "timeout".to_string()
        } else if error.is_connect() {
            "connect".to_string()
        } else {
            "transport".to_string()
        },
        uncertain: !not_sent,
        configuration: false,
        retry_after: None,
    }
}

/// Keep provider codes and ids short and log-safe.
fn sanitize_code(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(80)
        .collect()
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// A message kept by the capture transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedEmail {
    pub delivery_id: Uuid,
    pub idempotency_key: String,
    pub sender: String,
    pub recipient: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

const CAPTURE_LIMIT: usize = 200;

/// Keeps the most recent messages in memory and logs them instead of
/// sending, for tests and local development (`g5.email.provider`).
#[derive(Clone, Default)]
pub struct CaptureTransport {
    messages: Arc<Mutex<VecDeque<CapturedEmail>>>,
}

impl CaptureTransport {
    /// Captured messages, oldest first.
    #[must_use]
    pub fn messages(&self) -> Vec<CapturedEmail> {
        self.messages
            .lock()
            .map(|messages| messages.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl Transport for CaptureTransport {
    async fn send(&self, message: &Outgoing<'_>) -> SendOutcome {
        let captured = CapturedEmail {
            delivery_id: message.delivery_id,
            idempotency_key: message.idempotency_key(),
            sender: message.sender.to_string(),
            recipient: message.recipient.to_string(),
            subject: message.subject.to_string(),
            text: message.text.to_string(),
            html: message.html.to_string(),
        };
        tracing::info!(
            target: "g5_api::email::capture",
            delivery_id = %captured.delivery_id,
            to = %captured.recipient,
            subject = %captured.subject,
            "Captured email (not sent):\n{}",
            captured.text
        );
        if let Ok(mut messages) = self.messages.lock() {
            // A repeated identity replaces nothing: like the provider's
            // idempotency key, the same delivery is kept once.
            if !messages
                .iter()
                .any(|kept| kept.delivery_id == captured.delivery_id)
            {
                if messages.len() == CAPTURE_LIMIT {
                    messages.pop_front();
                }
                messages.push_back(captured);
            }
        }
        SendOutcome::Accepted {
            provider_message_id: Some(format!("capture-{}", message.delivery_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retry(code: &str, uncertain: bool, configuration: bool) -> SendOutcome {
        SendOutcome::Retry {
            code: code.to_string(),
            uncertain,
            configuration,
            retry_after: None,
        }
    }

    #[test]
    fn resend_answers_are_classified() {
        assert_eq!(
            classify_response(200, r#"{"id":"49a3999c-0ce1"}"#, None),
            SendOutcome::Accepted {
                provider_message_id: Some("49a3999c-0ce1".to_string())
            }
        );
        assert_eq!(
            classify_response(422, r#"{"name":"missing_required_field"}"#, None),
            SendOutcome::Rejected {
                code: "http_422:missing_required_field".to_string()
            }
        );
        assert_eq!(
            classify_response(400, r#"{"name":"validation_error"}"#, None),
            SendOutcome::Rejected {
                code: "http_400:validation_error".to_string()
            }
        );
        assert_eq!(
            classify_response(409, r#"{"name":"invalid_idempotent_request"}"#, None),
            SendOutcome::Rejected {
                code: "http_409:invalid_idempotent_request".to_string()
            }
        );
        assert_eq!(
            classify_response(409, r#"{"name":"concurrent_idempotent_requests"}"#, None),
            retry("http_409:concurrent_idempotent_requests", false, false)
        );
        assert_eq!(
            classify_response(401, r#"{"name":"missing_api_key"}"#, None),
            retry("http_401:missing_api_key", false, true)
        );
        assert_eq!(
            classify_response(403, r#"{"name":"validation_error"}"#, None),
            retry("http_403:validation_error", false, true)
        );
        assert_eq!(
            classify_response(503, "gateway", None),
            retry("http_503:unknown", true, false)
        );
        assert_eq!(
            classify_response(
                429,
                r#"{"name":"rate_limit_exceeded"}"#,
                Some(Duration::from_secs(7))
            ),
            SendOutcome::Retry {
                code: "http_429:rate_limit_exceeded".to_string(),
                uncertain: false,
                configuration: false,
                retry_after: Some(Duration::from_secs(7)),
            }
        );
    }

    #[test]
    fn provider_codes_are_log_safe() {
        assert_eq!(sanitize_code("re_\nsecret key=1"), "re_secretkey1");
        assert_eq!(sanitize_code(&"a".repeat(500)).len(), 80);
    }
}
