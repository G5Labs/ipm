//! Private, bounded HTTP mechanics shared by typed provider, login and report clients.
use super::{ProviderError, ProviderErrorKind};
use crate::FailureEffect;
use reqwest::{header::HeaderMap, Method};
use serde::de::DeserializeOwned;
use std::time::Duration;
use url::Url;

const MAX_DIAGNOSTIC: usize = 1024;
pub(super) const MAX_RESPONSE: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct GithubHttp {
    /// Reads follow same-origin redirects (a renamed or moved repository).
    client: reqwest::Client,
    /// Writes never follow a redirect: the destination a write was admitted
    /// for is the only one that may receive it, so a moved repository or a
    /// transferred issue answers with its 3xx, which is a refusal.
    writes: reqwest::Client,
}

/// Only typed adapters may construct requests. A grant-derived URL and selected
/// secret are scoped before entering the transport; neither is logged here.
pub(super) struct Request<'a> {
    pub(super) operation: &'static str,
    pub(super) target: Option<&'a str>,
    pub(super) url: Url,
    pub(super) method: Method,
    pub(super) token: Option<&'a str>,
    pub(super) secrets: &'a [&'a str],
    pub(super) payload: Payload<'a>,
    pub(super) timeout: Duration,
    pub(super) limit: usize,
    pub(super) github: bool,
    /// A GitHub media type replacing the default JSON one (for example the
    /// `full+json` representation that adds rendered Markdown).
    pub(super) media: Option<&'static str>,
}
pub(super) enum Payload<'a> {
    Empty,
    Json(serde_json::Value),
    /// A JSON body that only reads provider state (a GraphQL query).
    Query(serde_json::Value),
    Form(&'a [(&'a str, &'a str)]),
}
pub(super) struct Response {
    pub(super) status: u16,
    pub(super) headers: HeaderMap,
    pub(super) body: Vec<u8>,
    /// The provider answered through at least one same-origin redirect (for
    /// example a renamed or transferred repository).
    pub(super) redirected: bool,
    operation: &'static str,
    target: Option<String>,
    write: bool,
    secrets: Vec<String>,
}

/// Follow a redirect only to the same scheme, host and port as the request
/// (a renamed repository, a moved endpoint), at most five times. A
/// cross-origin redirect is returned to the caller as a 3xx and its
/// credentials are never sent there.
fn same_origin_redirect(attempt: reqwest::redirect::Attempt<'_>) -> reqwest::redirect::Action {
    const MAX_REDIRECTS: usize = 5;
    let same_origin = attempt
        .previous()
        .first()
        .is_some_and(|first| first.origin() == attempt.url().origin());
    if same_origin && attempt.previous().len() <= MAX_REDIRECTS {
        attempt.follow()
    } else {
        attempt.stop()
    }
}

impl GithubHttp {
    pub(super) fn new() -> Result<Self, ProviderError> {
        let build = |redirects: reqwest::redirect::Policy| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .redirect(redirects)
                .retry(reqwest::retry::never())
                .build()
                .map_err(|_| {
                    ProviderError::new(
                        "initialize transport",
                        ProviderErrorKind::Initialization,
                        FailureEffect::Refused,
                        "The provider HTTP client could not be initialized",
                    )
                })
        };
        Ok(Self {
            client: build(reqwest::redirect::Policy::custom(same_origin_redirect))?,
            writes: build(reqwest::redirect::Policy::none())?,
        })
    }
    pub(super) async fn send(&self, request: Request<'_>) -> Result<Response, ProviderError> {
        let write = !matches!(request.method, Method::GET | Method::HEAD)
            && !matches!(request.payload, Payload::Query(_));
        let effect = if write {
            FailureEffect::Unknown
        } else {
            FailureEffect::Refused
        };
        let secrets: Vec<String> = request
            .token
            .into_iter()
            .chain(request.secrets.iter().copied())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let target = request.target.map(|target| redact(target, &secrets));
        let error = |kind, message: &str| {
            let mut error =
                ProviderError::new(request.operation, kind, effect, redact(message, &secrets));
            error.target = target.clone().map(String::into_boxed_str);
            error
        };
        // Reject malformed credentials locally, rather than silently omitting
        // the authorization header and executing with a different authority.
        if request
            .token
            .is_some_and(|token| token.is_empty() || token.bytes().any(|b| b.is_ascii_control()))
        {
            return Err(ProviderError::invalid(
                request.operation,
                "Invalid selected provider credential",
            ));
        }
        let deadline = tokio::time::Instant::now() + request.timeout;
        // Form posts are OAuth token exchanges, which answer JSON only when
        // asked for plain JSON. One Accept value per request.
        let accept = if let Some(media) = request.media.filter(|_| request.github) {
            media
        } else if request.github && !matches!(request.payload, Payload::Form(_)) {
            "application/vnd.github+json"
        } else {
            "application/json"
        };
        // Writes go through the client that follows no redirect; reads may
        // follow a same-origin one.
        let client = if write { &self.writes } else { &self.client };
        let mut builder = client
            .request(request.method.clone(), request.url)
            .header("User-Agent", "G5-API")
            .header("Accept", accept);
        if request.github {
            builder = builder.header("X-GitHub-Api-Version", "2022-11-28");
        }
        if let Some(token) = request.token {
            builder = builder.bearer_auth(token);
        }
        builder = match request.payload {
            Payload::Empty => builder,
            Payload::Json(body) | Payload::Query(body) => builder.json(&body),
            Payload::Form(body) => builder.form(body),
        };
        let built = builder.build().map_err(|_| {
            ProviderError::invalid(
                request.operation,
                "The provider request could not be constructed",
            )
        })?;
        let requested = built.url().clone();
        let mut response = tokio::time::timeout_at(deadline, client.execute(built))
            .await
            .map_err(|_| {
                error(
                    ProviderErrorKind::Deadline,
                    &format!(
                        "The provider request deadline elapsed after {} seconds",
                        request.timeout.as_secs()
                    ),
                )
            })?
            .map_err(|err| {
                error(
                    if err.is_timeout() {
                        ProviderErrorKind::Deadline
                    } else {
                        ProviderErrorKind::Transport
                    },
                    &err.without_url().to_string(),
                )
            })?;
        let mut result = Response {
            status: response.status().as_u16(),
            headers: response.headers().clone(),
            body: Vec::new(),
            redirected: response.url() != &requested,
            operation: request.operation,
            target,
            write,
            secrets,
        };
        loop {
            let chunk = tokio::time::timeout_at(deadline, response.chunk())
                .await
                .map_err(|_| {
                    result.failure(
                        ProviderErrorKind::Deadline,
                        &format!(
                            "The provider response deadline elapsed after {} seconds",
                            request.timeout.as_secs()
                        ),
                    )
                })?
                .map_err(|err| {
                    result.failure(
                        if err.is_timeout() {
                            ProviderErrorKind::Deadline
                        } else {
                            ProviderErrorKind::Transport
                        },
                        &err.without_url().to_string(),
                    )
                })?;
            let Some(chunk) = chunk else { break };
            if result.body.len().saturating_add(chunk.len()) > request.limit {
                return Err(result.failure(
                    ProviderErrorKind::InvalidResponse,
                    "The provider response exceeded the size limit",
                ));
            }
            result.body.extend_from_slice(&chunk);
        }
        Ok(result)
    }
}
impl Response {
    /// GitHub's `Link` header names another page.
    pub(super) fn has_next_page(&self) -> bool {
        self.headers
            .get_all("link")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| value.contains("rel=\"next\""))
    }
    pub(super) fn rate_limited(&self) -> bool {
        self.status == 429
            || rate_limit_reported(
                self.status,
                rate_limit_headers(&self.headers),
                &String::from_utf8_lossy(&self.body),
            )
    }
    pub(super) fn failure(&self, kind: ProviderErrorKind, message: &str) -> ProviderError {
        let effect = if !self.write || (300..500).contains(&self.status) {
            FailureEffect::Refused
        } else if (200..300).contains(&self.status) {
            FailureEffect::Partial
        } else {
            FailureEffect::Unknown
        };
        let mut error =
            ProviderError::new(self.operation, kind, effect, redact(message, &self.secrets));
        error.target = self.target.clone().map(String::into_boxed_str);
        error.status = Some(self.status);
        let header = |name| {
            self.headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|v| redact(v, &self.secrets).into_boxed_str())
        };
        error.request_id = header("x-github-request-id").or_else(|| header("x-request-id"));
        error.retry_after = header("retry-after");
        error.rate_limit_reset = header("x-ratelimit-reset");
        error.rate_limited = self.rate_limited();
        error
    }
    pub(super) fn require_success(&self) -> Result<(), ProviderError> {
        if (200..300).contains(&self.status) {
            return Ok(());
        }
        let message = github_error_message(&String::from_utf8_lossy(&self.body));
        Err(self.failure(
            if self.rate_limited() {
                ProviderErrorKind::RateLimited
            } else {
                ProviderErrorKind::Rejected
            },
            &message,
        ))
    }
    pub(super) fn json<T: DeserializeOwned>(&self) -> Result<T, ProviderError> {
        self.require_success()?;
        serde_json::from_slice(&self.body).map_err(|_| {
            self.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid response for this operation",
            )
        })
    }
}

pub(super) fn redact(text: &str, secrets: &[String]) -> String {
    // Fields are complete and scrubbed before our diagnostic truncation. Native
    // bounded capture has separate handling for actually truncated secrets.
    let mut text = text.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[REDACTED]");
    }
    let scrubbed = g5_contracts::credential_scrub::scrub(&text);
    // Provider-supplied diagnostic URLs can contain arbitrary query secrets.
    scrubbed
        .split_whitespace()
        .map(|word| {
            if word.contains("://") || word.contains('@') {
                "[REDACTED]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_DIAGNOSTIC)
        .collect()
}

/// GitHub's headers report a rate limit: the primary limit is exhausted, or
/// GitHub asked the client to wait (`retry-after`, sent with secondary rate
/// limits while primary requests remain).
pub(super) fn rate_limit_headers(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        == Some("0")
        || headers.contains_key(reqwest::header::RETRY_AFTER)
}

/// GitHub reported a rate limit: in its headers, or, for a refusal, in its
/// error message, as secondary limits do ("You have exceeded a secondary
/// rate limit"). Successful bodies are not scanned.
pub(super) fn rate_limit_reported(status: u16, headers_report: bool, body: &str) -> bool {
    status == 429
        || (matches!(status, 403 | 422)
            && (headers_report || body.to_ascii_lowercase().contains("rate limit")))
}

fn github_error_message(body: &str) -> String {
    // Extract details before redaction: compact JSON can put a validation
    // message and documentation_url in the same whitespace-delimited token,
    // causing the URL redactor to swallow the end of the actual error.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        let mut details = Vec::new();
        if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
            details.push(message.to_string());
        }
        if let Some(errors) = value.get("errors").and_then(serde_json::Value::as_array) {
            for error in errors {
                if let Some(message) = error
                    .as_str()
                    .or_else(|| error.get("message").and_then(serde_json::Value::as_str))
                {
                    details.push(message.to_string());
                } else {
                    let fields: Vec<_> = ["resource", "field", "code"]
                        .iter()
                        .filter_map(|key| error.get(*key).and_then(serde_json::Value::as_str))
                        .collect();
                    if !fields.is_empty() {
                        details.push(fields.join(" "));
                    }
                }
            }
        }
        if !details.is_empty() {
            return details.join("; ");
        }
    }
    "The provider refused the request".to_owned()
}

/// Most redirects one [`GithubHttp::open_hosted`] follows.
const MAX_HOSTED_REDIRECTS: usize = 3;

/// Content a provider hosts, being read: its declared type and length, and
/// its body, which [`HostedBody::chunk`] yields within the byte limit and
/// deadline the request was opened with.
pub(super) struct HostedBody {
    response: reqwest::Response,
    deadline: tokio::time::Instant,
    timeout: Duration,
    limit: usize,
    read: usize,
    operation: &'static str,
    pub(super) content_type: Option<String>,
    pub(super) length: Option<u64>,
}

fn hosted_error(
    operation: &'static str,
    kind: ProviderErrorKind,
    status: Option<u16>,
    message: &str,
) -> ProviderError {
    let mut error = ProviderError::new(operation, kind, FailureEffect::Refused, message);
    error.status = status;
    error
}

fn hosted_transport(operation: &'static str, err: reqwest::Error) -> ProviderError {
    hosted_error(
        operation,
        if err.is_timeout() {
            ProviderErrorKind::Deadline
        } else {
            ProviderErrorKind::Transport
        },
        None,
        &redact(&err.without_url().to_string(), &[]),
    )
}

fn hosted_deadline(operation: &'static str, timeout: Duration) -> ProviderError {
    hosted_error(
        operation,
        ProviderErrorKind::Deadline,
        None,
        &format!(
            "The provider content deadline elapsed after {} seconds",
            timeout.as_secs()
        ),
    )
}

fn hosted_too_large(operation: &'static str, limit: usize) -> ProviderError {
    hosted_error(
        operation,
        ProviderErrorKind::InvalidResponse,
        None,
        &format!("The content is larger than {limit} bytes"),
    )
}

impl HostedBody {
    /// The next part of the body, `None` at its end. Exceeding the limit or
    /// the deadline is an error, never a truncated body.
    pub(super) async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, ProviderError> {
        let chunk = tokio::time::timeout_at(self.deadline, self.response.chunk())
            .await
            .map_err(|_| hosted_deadline(self.operation, self.timeout))?
            .map_err(|err| hosted_transport(self.operation, err))?;
        if let Some(chunk) = &chunk {
            self.read = self.read.saturating_add(chunk.len());
            if self.read > self.limit {
                return Err(hosted_too_large(self.operation, self.limit));
            }
        }
        Ok(chunk)
    }
}

impl GithubHttp {
    /// Open content a provider hosts outside its API (an issue's
    /// attachment), sending no credential: a signed link carries its own
    /// authority. Redirects are followed by hand, each target checked with
    /// `allowed` like the first; a declared length beyond `limit` is refused
    /// before the body is read. Errors never name the URL, which may be
    /// signed.
    pub(super) async fn open_hosted(
        &self,
        operation: &'static str,
        url: Url,
        allowed: &(dyn Fn(&Url) -> bool + Sync),
        timeout: Duration,
        limit: usize,
    ) -> Result<HostedBody, ProviderError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut url = url;
        for hop in 0..=MAX_HOSTED_REDIRECTS {
            if !allowed(&url) {
                return Err(hosted_error(
                    operation,
                    ProviderErrorKind::Rejected,
                    None,
                    if hop == 0 {
                        "The content is not hosted by the provider"
                    } else {
                        "The provider redirected outside its own hosts"
                    },
                ));
            }
            let request = self
                .writes
                .get(url.clone())
                .header("User-Agent", "G5-API")
                // The client's 30 s budget would cut a large video short;
                // this request's own deadline bounds it, body included.
                .timeout(deadline.saturating_duration_since(tokio::time::Instant::now()))
                .build()
                .map_err(|_| {
                    hosted_error(
                        operation,
                        ProviderErrorKind::InvalidInput,
                        None,
                        "The content request could not be constructed",
                    )
                })?;
            let response = tokio::time::timeout_at(deadline, self.writes.execute(request))
                .await
                .map_err(|_| hosted_deadline(operation, timeout))?
                .map_err(|err| hosted_transport(operation, err))?;
            let status = response.status();
            if status.is_redirection() {
                url = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|location| url.join(location).ok())
                    .ok_or_else(|| {
                        hosted_error(
                            operation,
                            ProviderErrorKind::InvalidResponse,
                            Some(status.as_u16()),
                            "The provider redirected without a usable location",
                        )
                    })?;
                continue;
            }
            if !status.is_success() {
                return Err(hosted_error(
                    operation,
                    if status.as_u16() == 429 {
                        ProviderErrorKind::RateLimited
                    } else {
                        ProviderErrorKind::Rejected
                    },
                    Some(status.as_u16()),
                    &format!("The provider answered {} for the content", status.as_u16()),
                ));
            }
            let length = response.content_length();
            if length.is_some_and(|length| length > limit as u64) {
                return Err(hosted_too_large(operation, limit));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            return Ok(HostedBody {
                response,
                deadline,
                timeout,
                limit,
                read: 0,
                operation,
                content_type,
                length,
            });
        }
        Err(hosted_error(
            operation,
            ProviderErrorKind::InvalidResponse,
            None,
            "The provider redirected too many times",
        ))
    }
}
