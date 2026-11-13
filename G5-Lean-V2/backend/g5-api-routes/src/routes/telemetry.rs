//! UI interaction telemetry ingest routes (issue G5Labs/G5#1552).
//!
//! `POST /api/v1/telemetry/ui-events` — the server half of the client UI
//! telemetry stream (`system_invariants/observability_audit` [D6], operations
//! `g5.platform.client-telemetry`): validates batches of UI events posted by
//! the browser client — clicks, submits, navigations and Settings-page
//! preference values — and re-emits each one as a structured INFO event on
//! the `g5_audit` tracing target (`ui.action`, `ui.navigation`,
//! `ui.setting`). Nothing is
//! persisted here — the `g5_audit` stream (and its downstream log sinks) is
//! the storage, the same tracing-only tier as the rest of the audit surface.
//!
//! Identity is server-authoritative: `user_id`/`org_id` come from the
//! authenticated `AuthContext`, never from the payload, and every re-emitted
//! event carries `source = "client"` so client-claimed detail can never be
//! confused with server-observed facts.

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use g5_auth::Scope;
use g5_contracts::error::G5Error;

use crate::errors::AppError;
use crate::middleware::auth_middleware::API_KEY_ISSUER;
use crate::middleware::client_ip::ClientIp;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::AppState;

// ─── Validation bounds (routes/telemetry [D2]) ──────────────────────────────

/// Maximum events per batch (the client chunks above this).
const MAX_EVENTS_PER_BATCH: usize = 200;
/// Maximum length of the client session id.
const MAX_SESSION_ID_LEN: usize = 64;
/// Maximum length of each string field.
const MAX_FIELD_LEN: usize = 200;
/// Maximum length of the `view` dimension field.
const MAX_VIEW_LEN: usize = 40;
/// Request body cap — far above any legitimate batch, far below the axum
/// default (routes/telemetry [D1]).
const MAX_BODY_BYTES: usize = 256 * 1024;

// ─── DTOs ────────────────────────────────────────────────────────────────────

/// Batch of UI events posted by the browser client.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiEventsRequest {
    /// Opaque per-tab telemetry session id minted by the client — NOT a
    /// chat/DB session identifier; it scopes nothing and authorizes nothing
    /// (hence the name: the route-auth scanner's `session_id` contract does
    /// not apply).
    #[serde(rename = "sessionId")]
    pub ui_session_id: String,
    pub events: Vec<UiEventDTO>,
}

/// One client-captured UI event. All fields are untrusted descriptive
/// detail; identity is never taken from here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiEventDTO {
    /// Monotonic per-tab sequence number (strictly increasing in a batch).
    pub seq: u64,
    /// Client wall-clock timestamp (ms since epoch).
    pub ts_ms: u64,
    /// `click` | `navigation` | `submit` | `setting`.
    pub kind: String,
    /// Element identity (data-testid / id / tag); the preference key for
    /// `setting`.
    pub target: Option<String>,
    /// Human-readable label (aria-label / trimmed text / href pathname); the
    /// preference's encoded value for `setting`.
    pub label: Option<String>,
    /// Client route (strictly pathname-only — no query, no fragment).
    pub path: Option<String>,
    /// Sanitized `view` query-param dimension (admin tabs), separated from
    /// `path` so query smuggling into the audit log is impossible.
    pub view: Option<String>,
    /// Context detail (ancestor testid, previous path for navigations,
    /// `snapshot` or `from:<previous value>` for settings).
    pub detail: Option<String>,
}

/// Ingest acknowledgement.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiEventsAcceptedDTO {
    pub accepted: usize,
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Telemetry routes mounted under `/api/v1/telemetry`.
///
/// - `POST /ui-events` — batch-ingest UI events, re-emitted on `g5_audit`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        // UsersRead is the one scope every role carries — telemetry must
        // work for read-only viewers, and the route mutates nothing.
        .audited_route("/ui-events", post(ingest_ui_events), &[Scope::UsersRead])
        .route_layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .into_router()
}

// ─── Handler ─────────────────────────────────────────────────────────────────

/// `POST /api/v1/telemetry/ui-events`
///
/// Validates the whole batch first (no partial emission — a rejected batch
/// emits nothing, so the client can retry without double-logging), then
/// emits one `g5_audit` event per input event. Returns `202 Accepted`.
async fn ingest_ui_events(
    auth: AuthContext,
    client_ip: ClientIp,
    Json(body): Json<UiEventsRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<UiEventsAcceptedDTO>>), AppError> {
    // Browser JWT sessions only: workers/hosts have no UI, and API-key
    // automation posting "UI events" could only be fabrication
    // (routes/telemetry [D1]).
    let claims = match &auth {
        AuthContext::User(claims) if claims.iss.as_deref() != Some(API_KEY_ISSUER) => claims,
        _ => {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec!["browser_user_session_required".into()],
            }))
        }
    };

    // `ui_session_id` is an opaque per-tab label with no resource semantics
    // (routes/telemetry [D2]); its only required guard is input hygiene
    // before re-emission as a `g5_audit` log field.
    if body.ui_session_id.is_empty() {
        return Err(validation_error("sessionId", "must not be empty"));
    }
    validate_string("sessionId", &body.ui_session_id, MAX_SESSION_ID_LEN)?;

    validate_batch(&body)?;

    let accepted = body.events.len();

    for event in &body.events {
        let event_name = match event.kind.as_str() {
            "navigation" => "ui.navigation",
            "setting" => "ui.setting",
            _ => "ui.action",
        };
        tracing::info!(
            target: "g5_audit",
            event = %event_name,
            source = "client",
            user_id = %claims.sub,
            org_id = %claims.org,
            ui_session_id = %body.ui_session_id,
            seq = event.seq,
            client_ts_ms = event.ts_ms,
            kind = %event.kind,
            target = %event.target.as_deref().unwrap_or(""),
            label = %event.label.as_deref().unwrap_or(""),
            path = %event.path.as_deref().unwrap_or(""),
            view = %event.view.as_deref().unwrap_or(""),
            detail = %event.detail.as_deref().unwrap_or(""),
            client_ip = %client_ip,
            "UI event"
        );
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(ResponseEnvelope::ok(UiEventsAcceptedDTO { accepted })),
    ))
}

// ─── Validation (routes/telemetry [D2]) ──────────────────────────────────────

fn validation_error(field: &str, message: &str) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({ field: message }),
    })
}

/// Rejects any string carrying ASCII control characters (log-line injection
/// guard: CR/LF/NUL and friends never reach the tracing emitter).
fn has_control_chars(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_control())
}

fn validate_string(field: &str, value: &str, max_len: usize) -> Result<(), AppError> {
    if value.len() > max_len {
        return Err(validation_error(
            field,
            &format!("must be at most {max_len} bytes"),
        ));
    }
    if has_control_chars(value) {
        return Err(validation_error(
            field,
            "must not contain control characters",
        ));
    }
    Ok(())
}

/// Validates the events array; the `ui_session_id` hygiene checks live
/// inline in the handler (route-auth scanner coverage).
fn validate_batch(body: &UiEventsRequest) -> Result<(), AppError> {
    if body.events.is_empty() {
        return Err(validation_error("events", "must not be empty"));
    }
    if body.events.len() > MAX_EVENTS_PER_BATCH {
        return Err(validation_error(
            "events",
            &format!("at most {MAX_EVENTS_PER_BATCH} events per batch"),
        ));
    }

    let mut prev_seq: Option<u64> = None;
    for event in &body.events {
        if !matches!(
            event.kind.as_str(),
            "click" | "navigation" | "submit" | "setting"
        ) {
            return Err(validation_error(
                "kind",
                "must be one of: click, navigation, submit, setting",
            ));
        }
        for (field, value) in [
            ("target", &event.target),
            ("label", &event.label),
            ("path", &event.path),
            ("view", &event.view),
            ("detail", &event.detail),
        ] {
            if let Some(value) = value {
                validate_string(field, value, MAX_FIELD_LEN)?;
            }
        }
        if let Some(path) = &event.path {
            // Strictly pathname-only: the real client sends
            // `location.pathname`; ANY query or fragment is a hostile or
            // buggy client and would smuggle `?token=...` into the audit
            // log (codex impl review finding 1). The view dimension travels
            // in the separate `view` field.
            if !path.starts_with('/') || path.contains('#') || path.contains('?') {
                return Err(validation_error(
                    "path",
                    "must be a pathname only (leading '/', no '?' or '#')",
                ));
            }
        }
        if let Some(view) = &event.view {
            if view.len() > MAX_VIEW_LEN
                || view.contains('/')
                || view.contains('?')
                || view.contains('#')
            {
                return Err(validation_error(
                    "view",
                    &format!("must be at most {MAX_VIEW_LEN} chars with no '/', '?', or '#'"),
                ));
            }
        }
        // Strictly increasing seq within a batch: cheap ordering/authenticity
        // sanity check (routes/telemetry [D2]).
        if let Some(prev) = prev_seq {
            if event.seq <= prev {
                return Err(validation_error(
                    "seq",
                    "must be strictly increasing within a batch",
                ));
            }
        }
        prev_seq = Some(event.seq);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::test_capture::{captured_g5_audit_events, CapturedEvent};
    use axum::body::Body;
    use axum::http::Request;
    use g5_auth::JwtClaims;
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;

    const USER_ID: &str = "6f0c2f7e-8f5a-4d2b-9c3e-2b1a4d5e6f70";
    const ORG_ID: &str = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

    fn user_claims(iss: &str) -> JwtClaims {
        JwtClaims {
            sub: USER_ID.to_string(),
            org: ORG_ID.to_string(),
            scopes: vec![Scope::UsersRead],
            iat: 0,
            exp: i64::MAX,
            iss: Some(iss.to_string()),
            aud: Some("g5".to_string()),
            jti: None,
            token_use: None,
            client: None,
        }
    }

    fn event(seq: u64, kind: &str) -> serde_json::Value {
        serde_json::json!({
            "seq": seq,
            "tsMs": 1_700_000_000_000u64,
            "kind": kind,
            "target": "save-button",
            "label": "Save",
            "path": "/acme/base",
            "view": "settings",
            "detail": "spec-editor"
        })
    }

    /// Bare handler router (scope enforcement is covered by the audited
    /// route registration + the static route-auth manifest tests; unit
    /// tests exercise the handler contract directly).
    fn test_router() -> Router {
        Router::new().route("/api/v1/telemetry/ui-events", post(ingest_ui_events))
    }

    async fn run(auth: AuthContext, body: serde_json::Value) -> (StatusCode, Vec<CapturedEvent>) {
        let (subscriber, events) = crate::middleware::test_capture::capture_subscriber();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/telemetry/ui-events")
            .header("content-type", "application/json")
            .header("x-forwarded-for", "198.51.100.123")
            .extension(axum::extract::ConnectInfo(
                "203.0.113.9:1234".parse::<std::net::SocketAddr>().unwrap(),
            ))
            .extension(auth)
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = async { test_router().oneshot(req).await.unwrap() }
            .with_subscriber(subscriber)
            .await;
        (response.status(), captured_g5_audit_events(&events))
    }

    fn user() -> AuthContext {
        AuthContext::User(user_claims("g5-api"))
    }

    fn batch(events: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({ "sessionId": "tab-1", "events": events })
    }

    #[tokio::test]
    async fn valid_batch_returns_202_and_emits_per_event() {
        let mut setting = event(4, "setting");
        setting["target"] = serde_json::Value::String("themeName".to_string());
        setting["label"] = serde_json::Value::String("signal-studio".to_string());
        setting["detail"] = serde_json::Value::String("from:editor".to_string());
        let (status, events) = run(
            user(),
            batch(vec![
                event(1, "click"),
                event(2, "navigation"),
                event(3, "submit"),
                setting,
            ]),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(events.len(), 4, "one g5_audit event per input: {events:?}");
        assert_eq!(events[0].field("event"), Some("ui.action"));
        assert_eq!(events[1].field("event"), Some("ui.navigation"));
        assert_eq!(events[2].field("event"), Some("ui.action"));
        // Preference values are queryable by key and value (operations
        // g5.platform.client-telemetry).
        assert_eq!(events[3].field("event"), Some("ui.setting"));
        assert_eq!(events[3].field("target"), Some("themeName"));
        assert_eq!(events[3].field("label"), Some("signal-studio"));
        assert_eq!(events[3].field("detail"), Some("from:editor"));
        assert_eq!(events[0].field("source"), Some("client"));
        assert_eq!(events[0].field("user_id"), Some(USER_ID));
        assert_eq!(events[0].field("org_id"), Some(ORG_ID));
        assert_eq!(events[0].field("ui_session_id"), Some("tab-1"));
        assert_eq!(events[0].field("client_ip"), Some("203.0.113.9"));
        assert_eq!(events[0].field("view"), Some("settings"));
    }

    #[tokio::test]
    async fn invalid_batches_reject_whole_batch_and_emit_nothing() {
        let too_many: Vec<_> = (1..=201).map(|i| event(i, "click")).collect();
        let mut control = event(1, "click");
        control["label"] = serde_json::Value::String("bad\nlabel".to_string());
        let mut long = event(1, "click");
        long["target"] = serde_json::Value::String("x".repeat(201));
        let mut query_path = event(1, "click");
        query_path["path"] =
            serde_json::Value::String("/a?view=settings&token=leak#frag".to_string());
        // codex impl review finding 1: even a SINGLE '?' must be rejected.
        let mut single_query_path = event(1, "click");
        single_query_path["path"] = serde_json::Value::String("/a?token=leak".to_string());
        let mut bad_view = event(1, "click");
        bad_view["view"] = serde_json::Value::String("settings/../../etc".to_string());

        let cases: Vec<(&str, serde_json::Value)> = vec![
            ("empty events", batch(vec![])),
            ("oversized batch", batch(too_many)),
            ("unknown kind", batch(vec![event(1, "hover")])),
            ("control chars", batch(vec![control])),
            ("over-length field", batch(vec![long])),
            ("non-canonical path", batch(vec![query_path])),
            ("single-query path", batch(vec![single_query_path])),
            ("slash-bearing view", batch(vec![bad_view])),
            (
                "non-increasing seq",
                batch(vec![event(2, "click"), event(2, "click")]),
            ),
            (
                "unknown DTO field",
                serde_json::json!({
                    "sessionId": "tab-1",
                    "events": [event(1, "click")],
                    "extra": true
                }),
            ),
        ];

        for (name, body) in cases {
            let (status, events) = run(user(), body).await;
            assert!(
                status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
                "{name}: expected validation rejection, got {status}"
            );
            assert!(
                events.is_empty(),
                "{name}: rejected batch must emit nothing"
            );
        }
    }

    #[tokio::test]
    async fn host_and_api_key_callers_are_forbidden() {
        let host = AuthContext::Host {
            host_id: uuid::Uuid::new_v4(),
            org_id: uuid::Uuid::parse_str(ORG_ID).unwrap(),
            scopes: vec![Scope::WorkerHost],
        };
        let (status, events) = run(host, batch(vec![event(1, "click")])).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(events.is_empty());

        let api_key_user = AuthContext::User(user_claims(API_KEY_ISSUER));
        let (status, events) = run(api_key_user, batch(vec![event(1, "click")])).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(events.is_empty());
    }
}
