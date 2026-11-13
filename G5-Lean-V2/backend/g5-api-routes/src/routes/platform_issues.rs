//! Public REST route for platform bug reporting.
//!
//! `POST /api/v1/platform/issues` lets any authenticated G5 user report a bug
//! about the G5 platform itself. The handler is thin: fail-closed gating,
//! validate + size-cap the payload, throttle per user, and hand the report to
//! the durable [`PlatformIssueIntakeService`] outbox — which delivers it to a
//! G5-owned GitHub repository server-side. It never talks to GitHub directly and
//! never exposes the GitHub credential. It is deliberately NOT org-scoped: a
//! platform report does not land in any tenant.

use std::io::Cursor;
use std::sync::{Arc, LazyLock};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::{AuthContext, WorkspaceIncarnationContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::{
    AppState, PlatformIssueIntakeService, PlatformIssueReport, PlatformIssueScreenshot,
};
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use g5_contracts::types::WorkspaceId;
use g5_internal_api::dtos::InternalPlatformIssueReportRequest;

/// Hard size caps enforced before enqueue (design §5.7 abuse controls).
const MAX_TITLE_LEN: usize = 500;
const MAX_BODY_LEN: usize = 20_000;
const MAX_SCREENSHOT_BYTES: usize = 5 * 1024 * 1024;
const MULTIPART_OVERHEAD_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = MULTIPART_OVERHEAD_BYTES;
const MAX_MULTIPART_BYTES: usize = MAX_SCREENSHOT_BYTES
    + MAX_TITLE_LEN
    + MAX_BODY_LEN
    + MAX_CONTEXT_BYTES
    + MULTIPART_OVERHEAD_BYTES;
const MAX_SCREENSHOT_DECODED_BYTES: u64 = 40 * 1024 * 1024;
/// The only severities the intake accepts. Kept in lockstep with the client
/// dropdown and the `severity:<severity>` GitHub label vocabulary
/// (@ref software_architecture/g5-api/services/platform_issue_intake#D4): any
/// other value is rejected before it can reach the durable outbox or a label.
const PLATFORM_ISSUE_SEVERITIES: [&str; 4] = ["low", "medium", "high", "critical"];
const MAX_CONCURRENT_PNG_DECODES: usize = 2;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static PNG_DECODE_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_PNG_DECODES)));

// ─── Wire DTOs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformIssueReportRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub severity: String,
    #[serde(default)]
    pub context: PlatformIssueContext,
}

impl From<InternalPlatformIssueReportRequest> for PlatformIssueReportRequest {
    fn from(request: InternalPlatformIssueReportRequest) -> Self {
        let context = request.context;
        Self {
            title: request.title,
            body: request.body,
            kind: request.kind,
            severity: request.severity,
            context: PlatformIssueContext {
                route: context.route,
                app_version: context.app_version,
                user_agent: context.user_agent,
                viewport: context.viewport,
                workspace_slug: context.workspace_slug,
                org_slug: context.org_slug,
                breadcrumbs: context.breadcrumbs,
            },
        }
    }
}

/// Structured client context rendered into the issue body's
/// "Environment / context" section. Serializes camelCase so the stored JSONB
/// keys match what the intake service reads.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformIssueContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breadcrumbs: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformIssueReportAck {
    /// Durable submission id (the outbox row id). Write-only v1 returns this
    /// acknowledgement rather than the created GitHub issue URL.
    pub submission_id: Uuid,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    // UsersRead is carried by every user role; there is no per-org scope because
    // platform reports do not land in a tenant. The handler rejects worker/host.
    AuditedRouter::new()
        .audited_route("/", post(create_platform_issue), &[Scope::UsersRead])
        .into_router()
        // Reject an over-limit stream before multipart parsing so callers
        // consistently receive 413 instead of a parser validation error.
        .layer(RequestBodyLimitLayer::new(MAX_MULTIPART_BYTES))
        .layer(DefaultBodyLimit::max(MAX_MULTIPART_BYTES))
}

// ─── Handler ─────────────────────────────────────────────────────────────────

async fn create_platform_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    multipart: Multipart,
) -> Result<(StatusCode, Json<ResponseEnvelope<PlatformIssueReportAck>>), AppError> {
    // Fail closed: 503 when the feature is off or the GitHub delivery config is
    // absent/invalid (the intake service is only `Some` when fully configured).
    let intake = platform_intake(&state)?;
    // Any authenticated user; reject worker/host actors.
    let reporter = auth.require_user_id()?;
    // Per-user throttle protects the shared GitHub credential quota.
    intake.check_rate(reporter).await?;

    let (request, screenshot) = parse_report_multipart(multipart).await?;
    let report = finalize_report(&state, reporter, request, screenshot).await?;
    let submission_id = intake.enqueue(report).await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(ResponseEnvelope::ok(PlatformIssueReportAck {
            submission_id,
        })),
    ))
}

pub(crate) async fn create_platform_issue_internal(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(WorkspaceIncarnationContext(incarnation)): axum::extract::Extension<
        WorkspaceIncarnationContext,
    >,
    Path(workspace_id): Path<WorkspaceId>,
    body: Bytes,
) -> Result<(StatusCode, Json<ResponseEnvelope<PlatformIssueReportAck>>), AppError> {
    // Fail closed when platform reporting is off / unconfigured.
    let intake = platform_intake(&state)?;
    // Resolve the acting human server-side; rejects a non-worker actor and a
    // caller that does not own the active workspace. Runs before any body parse.
    let (guard_db, _org_id, reporter) = crate::routes::internal::resolve_internal_issue_actor(
        &state,
        &incarnation,
        workspace_id.into_uuid(),
        "create_platform_issue_internal",
    )
    .await?;
    // Throttle keyed on the resolved user BEFORE deserializing the (body-limit
    // bounded) request so an authenticated worker cannot force full JSON parsing
    // past its quota. Protects the shared GitHub credential quota.
    intake.check_rate(reporter).await?;

    // Reject unknown fields (e.g. a `screenshot`, unsupported on the agent path)
    // with a 4xx before enqueue.
    let request: PlatformIssueReportRequest =
        serde_json::from_slice::<InternalPlatformIssueReportRequest>(&body)
            .map_err(|err| validation(&format!("invalid report JSON: {err}")))?
            .into();

    let report = finalize_report(&state, reporter, request, None).await?;
    let submission_id = intake.enqueue(report).await?;
    guard_db.commit().await.map_err(AppError::from)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(ResponseEnvelope::ok(PlatformIssueReportAck {
            submission_id,
        })),
    ))
}

/// Validate, size-cap, and sanitize a parsed report, snapshot the reporter's
/// display name + deployment base URL, and build the durable [`PlatformIssueReport`].
/// Shared by the public user handler and the internal attributed handler so both
/// entry points enforce identical validation and produce identical outbox rows.
async fn finalize_report(
    state: &AppState,
    reporter: Uuid,
    mut request: PlatformIssueReportRequest,
    screenshot: Option<PlatformIssueScreenshot>,
) -> Result<PlatformIssueReport, AppError> {
    let title = request.title.trim().to_string();
    if title.is_empty() || title.chars().count() > MAX_TITLE_LEN {
        return Err(validation(
            "title is required and must be at most 500 characters",
        ));
    }
    if request.body.len() > MAX_BODY_LEN {
        return Err(validation("body exceeds the maximum size"));
    }
    let severity = normalize_severity(&request.severity)?;
    let kind = request
        .kind
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "bug".to_string());

    sanitize_context(&mut request.context);
    // Serialize once: this is both the exact admission measurement and the
    // payload persisted by the outbox service.
    let context_bytes = serde_json::to_vec(&request.context)
        .map_err(|_| validation("context cannot be serialized"))?;
    if context_bytes.len() > MAX_CONTEXT_BYTES {
        return Err(validation("context exceeds the maximum size"));
    }
    let context = serde_json::from_slice(&context_bytes)
        .map_err(|_| validation("context cannot be serialized"))?;

    // Snapshot the reporter's human-readable name so the delivered GitHub issue
    // shows a name, not just a UUID, for human triage. Delegated to
    // `UserService` so the route keeps clear of the DB (service boundary):
    // a missing user row degrades to the id-only reporter line.
    let reporter_display_name = state
        .user_service
        .resolve_display_name(&state.system_pool, reporter)
        .await?;
    // Snapshot the deployment browser base URL so the worker can render the
    // captured route as a clickable link straight to the customer view.
    let report_base_url = Some(state.external_url.trim().to_string()).filter(|u| !u.is_empty());

    Ok(PlatformIssueReport {
        reporter_user_id: reporter,
        reporter_display_name,
        report_base_url,
        title,
        body: request.body,
        kind,
        severity,
        context,
        screenshot,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// The intake service, or a fail-closed 503 when platform reporting is disabled
/// / not fully configured.
fn platform_intake(state: &AppState) -> Result<&PlatformIssueIntakeService, AppError> {
    if !state.platform_issue_reporting_available() {
        return Err(platform_issue_unavailable());
    }
    state
        .platform_issue_intake
        .as_ref()
        .ok_or_else(platform_issue_unavailable)
}

fn platform_issue_unavailable() -> AppError {
    AppError::from_error(G5Error::ServiceUnavailable {
        message: "Platform bug reporting is disabled or not configured for this deployment"
            .to_string(),
    })
}

fn validation(message: &str) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({ "message": message }),
    })
}

/// Defensively constrain the reporter-supplied severity to the fixed vocabulary
/// before it can reach `platform_issue_outbox.severity` or a `severity:<severity>`
/// GitHub label. Matching is case-insensitive and the canonical lowercase form
/// is stored. Rejects empty and any out-of-vocabulary value with a 4xx.
fn normalize_severity(raw: &str) -> Result<String, AppError> {
    let severity = raw.trim().to_ascii_lowercase();
    if PLATFORM_ISSUE_SEVERITIES.contains(&severity.as_str()) {
        Ok(severity)
    } else {
        Err(validation(
            "severity must be one of: low, medium, high, critical",
        ))
    }
}

/// Preserve only the pathname in the user-controlled context. The client
/// never supplies query or fragment data, but the API must enforce that privacy
/// boundary for raw callers as well.
fn sanitize_context(context: &mut PlatformIssueContext) {
    context.route = context.route.take().map(|route| {
        route
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_string()
    });
}

/// Parse the `multipart/form-data` body: a required `report` JSON part plus an
/// optional `screenshot` image part.
async fn parse_report_multipart(
    mut multipart: Multipart,
) -> Result<(PlatformIssueReportRequest, Option<PlatformIssueScreenshot>), AppError> {
    let mut report: Option<PlatformIssueReportRequest> = None;
    let mut screenshot: Option<PlatformIssueScreenshot> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| validation(&format!("invalid multipart body: {err}")))?
    {
        match field.name() {
            Some("report") => {
                let text = field
                    .text()
                    .await
                    .map_err(|err| validation(&format!("invalid report part: {err}")))?;
                let parsed = serde_json::from_str::<PlatformIssueReportRequest>(&text)
                    .map_err(|err| validation(&format!("invalid report JSON: {err}")))?;
                report = Some(parsed);
            }
            Some("screenshot") => {
                let is_png_content_type =
                    field.content_type().is_some_and(|mime| mime == "image/png");
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|err| validation(&format!("invalid screenshot part: {err}")))?;
                if bytes.len() > MAX_SCREENSHOT_BYTES {
                    return Err(validation("screenshot exceeds the maximum size"));
                }
                if !bytes.is_empty() {
                    validate_png(is_png_content_type, &bytes).await?;
                    screenshot = Some(PlatformIssueScreenshot {
                        bytes: bytes.to_vec(),
                        mime: "image/png".to_string(),
                    });
                }
            }
            _ => {
                // Ignore unknown parts.
            }
        }
    }

    let report = report.ok_or_else(|| validation("missing required 'report' part"))?;
    Ok((report, screenshot))
}

/// Verify that a kept screenshot is an actual bounded PNG, not merely a file
/// with an image-like MIME header or extension. `image` validates the PNG
/// structure and decoded pixels while its allocation limit prevents a compact
/// decompression bomb from exceeding the admission budget.
async fn validate_png(is_png_content_type: bool, bytes: &[u8]) -> Result<(), AppError> {
    validate_png_with_semaphore(is_png_content_type, bytes, PNG_DECODE_SEMAPHORE.clone()).await
}

async fn validate_png_with_semaphore(
    is_png_content_type: bool,
    bytes: &[u8],
    decode_semaphore: Arc<Semaphore>,
) -> Result<(), AppError> {
    if !is_png_content_type {
        return Err(validation("screenshot must use Content-Type image/png"));
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err(validation("screenshot must have a PNG signature"));
    }

    let permit = decode_semaphore.try_acquire_owned().map_err(|_| {
        AppError::from_error(G5Error::RateLimited {
            retry_after_secs: Some(1),
        })
    })?;
    let bytes = bytes.to_vec();
    let decoded = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_SCREENSHOT_DECODED_BYTES);
        reader.limits(limits);
        reader.decode().is_ok()
    })
    .await
    .map_err(|_| validation("screenshot validation task failed"))?;
    decoded
        .then_some(())
        .ok_or_else(|| validation("screenshot must decode as a PNG within the size limit"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn report_request_parses_camel_case_context() {
        let json = r#"{
            "title": "Tab bar loses state",
            "body": "resets on reload",
            "severity": "high",
            "context": {
                "route": "/workspaces/abc",
                "appVersion": "1.2.3",
                "userAgent": "Mozilla/5.0",
                "viewport": "1280x720",
                "workspaceSlug": "ws-1",
                "orgSlug": "org-1",
                "breadcrumbs": ["opened modal"]
            }
        }"#;
        let parsed: PlatformIssueReportRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.title, "Tab bar loses state");
        assert_eq!(parsed.severity, "high");
        assert_eq!(parsed.context.app_version.as_deref(), Some("1.2.3"));
        assert_eq!(parsed.context.workspace_slug.as_deref(), Some("ws-1"));
        assert_eq!(parsed.context.breadcrumbs, vec!["opened modal".to_string()]);
    }

    #[test]
    fn context_serializes_camel_case_keys() {
        let ctx = PlatformIssueContext {
            app_version: Some("9.9.9".to_string()),
            user_agent: Some("UA".to_string()),
            ..Default::default()
        };
        let value = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            value.get("appVersion").and_then(|v| v.as_str()),
            Some("9.9.9")
        );
        assert_eq!(value.get("userAgent").and_then(|v| v.as_str()), Some("UA"));
        // Absent optionals are omitted.
        assert!(value.get("route").is_none());
    }

    #[test]
    fn normalize_severity_accepts_vocabulary_case_insensitively() {
        for (input, expected) in [
            ("low", "low"),
            ("Medium", "medium"),
            ("  HIGH ", "high"),
            ("Critical", "critical"),
        ] {
            assert_eq!(normalize_severity(input).unwrap(), expected);
        }
    }

    #[test]
    fn normalize_severity_rejects_out_of_vocabulary_and_empty() {
        for input in ["", "  ", "s1", "urgent", "blocker"] {
            let error = normalize_severity(input).expect_err("must reject");
            // ValidationError → 4xx before the value can reach a GitHub label.
            assert_eq!(
                error.into_response().status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
    }

    #[test]
    fn internal_request_rejects_unknown_screenshot_field() {
        // The agent path carries no screenshot; a stray field must be refused,
        // not silently dropped (deny_unknown_fields).
        let with_screenshot = r#"{
            "title": "t",
            "severity": "low",
            "screenshot": "Zm9v"
        }"#;
        assert!(
            serde_json::from_str::<InternalPlatformIssueReportRequest>(with_screenshot).is_err()
        );

        // A clean payload still parses and maps into the shared request.
        let clean = r#"{ "title": "t", "severity": "low" }"#;
        let parsed: PlatformIssueReportRequest =
            serde_json::from_str::<InternalPlatformIssueReportRequest>(clean)
                .unwrap()
                .into();
        assert_eq!(parsed.title, "t");
        assert_eq!(parsed.severity, "low");
    }

    #[test]
    fn ack_serializes_submission_id_camel_case() {
        let ack = PlatformIssueReportAck {
            submission_id: Uuid::nil(),
        };
        let value = serde_json::to_value(&ack).unwrap();
        assert!(value.get("submissionId").is_some());
    }

    #[tokio::test]
    async fn png_admission_rejects_wrong_mime_or_signature() {
        assert!(validate_png(false, PNG_SIGNATURE).await.is_err());
        assert!(validate_png(true, b"not a png").await.is_err());
    }

    #[tokio::test]
    async fn png_admission_rejects_decode_saturation_with_retry_hint() {
        let error = validate_png_with_semaphore(true, PNG_SIGNATURE, Arc::new(Semaphore::new(0)))
            .await
            .expect_err("a saturated decode semaphore must reject immediately");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get("retry-after").unwrap(), "1");
    }
}
