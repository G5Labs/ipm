//! API-layer error handling infrastructure.
//!
//! Bridges the shared [`G5Error`] enum (from `g5-common::error`) to axum HTTP
//! responses. Defines [`AppError`], the application-level error wrapper used
//! throughout all route handlers and service layers in `g5-api`, and implements
//! axum's [`IntoResponse`] trait so that any `AppError` is automatically
//! serialized as a JSON error envelope with the correct HTTP status code.
//!
//! # Architecture
//!
//! `G5Error` (in `g5-common`) is a shared, binary-agnostic error type. The
//! `g5-api` server needs an axum-specific wrapper that:
//! 1. Carries a `request_id` for error correlation (injected by middleware).
//! 2. Implements [`IntoResponse`] to produce the standard JSON error envelope.
//! 3. Provides ergonomic `From` conversions so route handlers can use `?`.

pub mod catalog;

use axum::Json;
use axum::{
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};
use serde::Serialize;

// Re-exports from g5-common for convenience — other modules in g5-api only
// need to import from `crate::errors`.
pub use g5_contracts::error::{ErrorResponse, G5Error};

pub use catalog::ErrorCatalog;

// ---------------------------------------------------------------------------
// ApiResult type alias
// ---------------------------------------------------------------------------

/// Convenience result type for route handlers.
///
/// ```ignore
/// async fn get_workspace(/* ... */) -> ApiResult<Json<WorkspaceDTO>> {
///     // ...
/// }
/// ```
pub type ApiResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------
// ErrorEnvelope — JSON wrapper
// ---------------------------------------------------------------------------

/// JSON error envelope matching the API convention from §3.0 of
/// `API_AND_STORAGE.md`.
///
/// Wraps an [`ErrorResponse`] in a top-level `"error"` key:
///
/// ```json
/// {
///   "error": {
///     "code": "NOT_FOUND",
///     "message": "Resource not found: workspace ws-123",
///     "details": {},
///     "requestId": "req_abc123"
///   }
/// }
/// ```
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorResponse,
}

// ---------------------------------------------------------------------------
// AppError — axum-integrated error wrapper
// ---------------------------------------------------------------------------

/// Application-level error wrapper for axum integration.
///
/// Carries a [`G5Error`] plus a `request_id` for error correlation. The
/// `request_id` is typically injected by the request_id middleware, or can
/// be set directly by handlers that have access to the request ID.
#[derive(Debug)]
pub struct AppError {
    /// The underlying G5 error.
    pub inner: G5Error,
    /// Request ID for error correlation. Empty string if not yet injected.
    /// Boxed so `AppError` stays within the large-`Err` size budget.
    pub request_id: Box<str>,
    /// Which stage of an operation failed, added to the response as
    /// `details.kind` without changing the error's status or code.
    pub kind: Option<FailureKind>,
}

/// A stable classification of where an operation failed. One byte, so
/// `AppError` stays within the large-`Err` size budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Saving or pushing the source failed before a PR was created or
    /// updated; nothing was published.
    PullRequestSourceSaveFailed,
    /// The Source Control checklist no longer represents the requested read.
    SourceControlStaleList,
}

impl FailureKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PullRequestSourceSaveFailed => "pull_request_source_save_failed",
            Self::SourceControlStaleList => "source_control_stale_list",
        }
    }
}

impl AppError {
    /// Creates a new `AppError` with the given error and request ID.
    #[must_use]
    pub fn new(error: G5Error, request_id: String) -> Self {
        Self {
            inner: error,
            request_id: request_id.into_boxed_str(),
            kind: None,
        }
    }

    /// Creates a new `AppError` with an empty request ID.
    ///
    /// Intended for contexts where the request_id is injected later by
    /// middleware, or for `From` conversions where the request ID is not
    /// available.
    #[must_use]
    pub fn from_error(error: G5Error) -> Self {
        Self {
            inner: error,
            request_id: Box::default(),
            kind: None,
        }
    }

    /// Classify the failure in `details.kind`, keeping its status, code and
    /// other details.
    #[must_use]
    pub fn with_kind(mut self, kind: FailureKind) -> Self {
        self.kind = Some(kind);
        self
    }
}

// ---------------------------------------------------------------------------
// IntoResponse for AppError
// ---------------------------------------------------------------------------

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.inner.status_code();
        let mut error_response = self.inner.to_error_response(self.request_id.to_string());
        if let Some(kind) = self.kind {
            if !error_response.details.is_object() {
                error_response.details = serde_json::json!({});
            }
            error_response.details["kind"] = serde_json::json!(kind.as_str());
        }

        let exposes_details_message = match &self.inner {
            G5Error::ValidationError { details } => matches!(
                details.get("code").and_then(|value| value.as_str()),
                Some(
                    "BASE_WORKSPACE_REMOVAL"
                        | "FILE_SUGGESTION_REFUSED"
                        // Runtime Config secret refusals name the key and the
                        // limit so the editor can show what to correct
                        // (`g5.chat.secret-admission`).
                        | "secret_name_invalid"
                        | "secret_name_reserved"
                        | "secret_value_too_large"
                        | "secret_value_nul"
                        | "secrets_too_many"
                        | "secrets_too_large"
                )
            ),
            _ => false,
        };
        if exposes_details_message {
            if let Some(message) = error_response
                .details
                .get("message")
                .and_then(|value| value.as_str())
            {
                error_response.message = message.to_string();
            }
        }

        // Log based on error severity.
        if status.is_server_error() {
            // 5xx errors: log at error level with full message for debugging.
            // The response body uses the sanitized message (handled by
            // G5Error::to_error_response).
            tracing::error!(
                request_id = %self.request_id,
                error_code = %error_response.code,
                error_message = %self.inner,
                "Internal server error"
            );
        } else {
            // 4xx errors: log at warn level with error code and request_id.
            tracing::warn!(
                request_id = %self.request_id,
                error_code = %error_response.code,
                "Client error"
            );
        }

        let envelope = ErrorEnvelope {
            error: error_response,
        };

        let mut response = (status, Json(envelope)).into_response();
        if let G5Error::RateLimited {
            retry_after_secs: Some(retry_after_secs),
        } = &self.inner
        {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after_secs.to_string())
                    .expect("a u64 retry duration is a valid HTTP header value"),
            );
        }
        response
    }
}

// ---------------------------------------------------------------------------
// Display for AppError
// ---------------------------------------------------------------------------

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// ---------------------------------------------------------------------------
// From<G5Error> for AppError
// ---------------------------------------------------------------------------

impl From<G5Error> for AppError {
    /// Converts a `G5Error` into an `AppError` with an empty request_id.
    ///
    /// This enables `?` propagation in handlers that return
    /// `Result<T, AppError>`.
    fn from(error: G5Error) -> Self {
        AppError::from_error(error)
    }
}

// ---------------------------------------------------------------------------
// From<T> conversions for common external error types
// ---------------------------------------------------------------------------

// All of these delegate to G5Error::from(T) first, then wrap in AppError.
// This pattern ensures the error mapping logic lives in g5-common and is
// shared across all binaries.

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::from_error(G5Error::from(e))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::from_error(G5Error::from(e))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::from_error(G5Error::from(e))
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        AppError::from_error(G5Error::from(e))
    }
}

impl From<g5_authz::AuthzError> for AppError {
    fn from(e: g5_authz::AuthzError) -> Self {
        use g5_authz::AuthzError;
        match e {
            AuthzError::NotMember(_) => AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec!["org_membership".into()],
            }),
            AuthzError::InsufficientScopes(scopes) => AppError::from_error(G5Error::Forbidden {
                missing_scopes: scopes,
            }),
            AuthzError::Internal(msg) => AppError::from_error(G5Error::Internal { message: msg }),
        }
    }
}

impl From<g5_authz::ScopedDbError> for AppError {
    fn from(e: g5_authz::ScopedDbError) -> Self {
        use g5_authz::ScopedDbError;
        match e {
            ScopedDbError::InvalidSession { .. } => AppError::from_error(G5Error::Unauthorized),
            ScopedDbError::InitFailed(msg)
            | ScopedDbError::ClearFailed(msg)
            | ScopedDbError::CommitFailed(msg) => {
                // Preserve SQLSTATE before stringification, including failures
                // during COMMIT. Only the owning operation may decide to replay.
                if g5_contracts::error::sqlx_error_is_retryable(&msg) {
                    AppError::from_error(G5Error::from(msg))
                } else {
                    AppError::from_error(G5Error::Internal {
                        message: msg.to_string(),
                    })
                }
            }
            ScopedDbError::TransactionConsumed => AppError::from_error(G5Error::Internal {
                message: "Database transaction already consumed".to_string(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    // -- AppError construction tests --

    #[test]
    fn test_app_error_new() {
        let err = AppError::new(
            G5Error::NotFound {
                resource: "workspace ws-123".into(),
            },
            "req_abc".into(),
        );
        assert_eq!(&*err.request_id, "req_abc");
        assert_eq!(err.inner.error_code(), "NOT_FOUND");
    }

    #[test]
    fn test_app_error_from_error_empty_request_id() {
        let err = AppError::from_error(G5Error::Unauthorized);
        assert!(err.request_id.is_empty());
        assert_eq!(err.inner.error_code(), "UNAUTHORIZED");
    }

    #[test]
    fn test_from_g5_error() {
        let g5_err = G5Error::Forbidden {
            missing_scopes: vec!["admin".into()],
        };
        let app_err: AppError = g5_err.into();
        assert!(app_err.request_id.is_empty());
        assert_eq!(app_err.inner.error_code(), "FORBIDDEN");
    }

    // -- From<T> conversion tests --

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<String>("invalid json").unwrap_err();
        let app_err = AppError::from(json_err);
        assert_eq!(app_err.inner.error_code(), "INTERNAL_ERROR");
        assert!(app_err.request_id.is_empty());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err = AppError::from(io_err);
        assert_eq!(app_err.inner.error_code(), "INTERNAL_ERROR");
    }

    #[test]
    fn test_from_jsonwebtoken_error() {
        // Create a JWT error by trying to decode an invalid token
        let jwt_err = jsonwebtoken::decode::<serde_json::Value>(
            "invalid.token.here",
            &jsonwebtoken::DecodingKey::from_secret(b"secret"),
            &jsonwebtoken::Validation::default(),
        )
        .unwrap_err();
        let app_err = AppError::from(jwt_err);
        // JWT errors should map to Unauthorized (not Internal)
        assert_eq!(app_err.inner.error_code(), "UNAUTHORIZED");
    }

    // -- IntoResponse tests --

    #[tokio::test]
    async fn test_into_response_not_found() {
        let err = AppError::new(
            G5Error::NotFound {
                resource: "workspace ws-123".into(),
            },
            "req_42".into(),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify envelope structure
        let error_obj = json.get("error").expect("missing 'error' key");
        assert_eq!(error_obj["code"], "NOT_FOUND");
        assert_eq!(error_obj["message"], "Resource not found: workspace ws-123");
        assert_eq!(error_obj["requestId"], "req_42");
    }

    #[tokio::test]
    async fn test_into_response_internal_sanitized() {
        let err = AppError::new(
            G5Error::Internal {
                message: "secret database password".into(),
            },
            "req_99".into(),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let error_obj = json.get("error").expect("missing 'error' key");
        assert_eq!(error_obj["code"], "INTERNAL_ERROR");
        // Message must be sanitized — no internal details exposed
        assert_eq!(error_obj["message"], "An internal error occurred");
        assert_eq!(error_obj["requestId"], "req_99");
    }

    #[tokio::test]
    async fn test_into_response_forbidden_with_details() {
        let err = AppError::new(
            G5Error::Forbidden {
                missing_scopes: vec!["admin".into(), "write".into()],
            },
            "req_77".into(),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let error_obj = json.get("error").expect("missing 'error' key");
        assert_eq!(error_obj["code"], "FORBIDDEN");
        let details = &error_obj["details"];
        let scopes = details["missing_scopes"].as_array().unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[tokio::test]
    async fn test_into_response_envelope_structure() {
        // Verify the exact JSON envelope structure for any error
        let err = AppError::new(
            G5Error::RateLimited {
                retry_after_secs: Some(60),
            },
            "req_rate".into(),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "60");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Top-level must have exactly one key: "error"
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("error"), "envelope must have 'error' key");

        // The error object must have code, message, details, requestId
        let error_obj = &obj["error"];
        assert!(error_obj.get("code").is_some());
        assert!(error_obj.get("message").is_some());
        assert!(error_obj.get("details").is_some());
        assert!(error_obj.get("requestId").is_some());
    }

    // -- ApiResult type alias test --

    #[test]
    fn test_api_result_type_alias() {
        // Verify ApiResult<T> works as expected
        fn returns_ok() -> ApiResult<String> {
            Ok("hello".into())
        }
        fn returns_err() -> ApiResult<String> {
            Err(AppError::from_error(G5Error::Unauthorized))
        }
        assert!(returns_ok().is_ok());
        assert!(returns_err().is_err());
    }

    // -- ErrorEnvelope serialization test --

    #[test]
    fn test_error_envelope_serialization() {
        let envelope = ErrorEnvelope {
            error: ErrorResponse {
                code: "NOT_FOUND".into(),
                message: "Resource not found".into(),
                details: serde_json::json!({}),
                request_id: "req_1".into(),
            },
        };
        let json = serde_json::to_value(&envelope).unwrap();
        assert!(json.get("error").is_some());
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        // Verify camelCase from ErrorResponse's serde rename
        assert!(json["error"].get("requestId").is_some());
    }

    // -- Re-export tests --

    #[test]
    fn test_reexports_available() {
        // Verify that G5Error, ErrorResponse, and ErrorCatalog are
        // accessible through this module.
        let _err = G5Error::Unauthorized;
        let _resp = ErrorResponse {
            code: String::new(),
            message: String::new(),
            details: serde_json::json!(null),
            request_id: String::new(),
        };
        let _catalog_count = ErrorCatalog::all_codes().len();
    }
}
