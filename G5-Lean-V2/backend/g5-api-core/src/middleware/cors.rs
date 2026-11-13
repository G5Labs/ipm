//! CORS middleware configuration for the g5-api HTTP processing pipeline.
//!
//! This is the **second** Tower middleware layer in the stack:
//! Request ID → **CORS** → Compression → Rate Limit → Auth → Handler.
//!
//! The module provides a single public function [`build_cors_layer`] that
//! constructs a [`tower_http::cors::CorsLayer`] from the application's
//! [`CorsConfig`]. It does **not** implement custom middleware — it leverages
//! `tower-http`'s battle-tested CORS implementation and configures it
//! according to the G5 API requirements.
//!
//! # Why CORS Must Be Before Auth
//!
//! Browsers send `OPTIONS` preflight requests **without** an `Authorization`
//! header. If the Auth middleware ran before CORS, all preflight requests
//! would be rejected with `401 UNAUTHORIZED`, making the API completely
//! unusable from browser clients.
//!
//! # Credentials and Specific Origins
//!
//! Because `allow_credentials(true)` is set (required for `httpOnly` refresh
//! token cookies), the `Access-Control-Allow-Origin` header MUST echo the
//! specific requesting origin — it cannot be `*`. The `tower-http`
//! `CorsLayer` handles this correctly when configured with a list of
//! specific origins.

use crate::config::CorsConfig;
use http::{header, HeaderName, HeaderValue, Method};
use std::time::Duration;
use tower_http::cors::CorsLayer;

/// The custom header name for request ID correlation.
///
/// Used in both `allow_headers` (so clients can send it) and
/// `expose_headers` (so client-side JavaScript can read it).
const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Builds the CORS layer for the g5-api middleware stack.
///
/// Reads the allowed origins from `config.allowed_origins` and constructs
/// a [`CorsLayer`] with the following configuration:
///
/// - **Allowed origins**: Parsed from `config.allowed_origins`. Invalid
///   origin strings are logged with [`tracing::warn!`] and skipped.
/// - **Allowed methods**: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`.
/// - **Allowed headers**: `Authorization`, `Content-Type`, `X-Request-Id`.
/// - **Exposed headers**: `X-Request-Id` (so client-side code can read the
///   request ID for error reporting).
/// - **Credentials**: `true` (required for `httpOnly` refresh token cookies).
/// - **Max age**: `86400` seconds (24 hours — browsers cache preflight results).
///
/// # Arguments
///
/// * `config` — Reference to the [`CorsConfig`] from the application's
///   [`AppConfig`](crate::config::AppConfig). Contains the
///   `allowed_origins: Vec<String>` list parsed from the
///   `G5_CORS_ALLOWED_ORIGINS` environment variable.
///
/// # Returns
///
/// A configured [`CorsLayer`] ready to be applied to the axum `Router`
/// via `.layer()` during middleware stack assembly in `mod.rs`.
///
/// # Examples
///
/// ```ignore
/// use crate::config::CorsConfig;
/// use crate::middleware::cors::build_cors_layer;
///
/// let config = CorsConfig {
///     allowed_origins: vec![
///         "http://localhost:5173".to_string(),
///         "https://app.g5.dev".to_string(),
///     ],
/// };
///
/// let cors_layer = build_cors_layer(&config);
/// // Apply: router.layer(cors_layer)
/// ```
pub fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    // Parse each origin string into a HeaderValue. Invalid origins are
    // logged and skipped — the server should not crash due to a
    // misconfigured origin.
    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|origin| {
            origin.parse::<HeaderValue>().ok().or_else(|| {
                tracing::warn!("Invalid CORS origin, skipping: {}", origin);
                None
            })
        })
        .collect();

    CorsLayer::new()
        // Allowed origins: echoes the specific requesting origin from the
        // allowed list. Required because allow_credentials(true) is
        // incompatible with wildcard `*` origins.
        .allow_origin(origins)
        // Allowed methods: all methods used by the G5 REST API (§3.0 of
        // API_AND_STORAGE.md). OPTIONS is included explicitly for preflight
        // requests.
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        // Allowed headers:
        // - Authorization: JWT bearer tokens on /api/v1/ and service tokens
        //   on /api/internal/ endpoints.
        // - Content-Type: JSON request bodies (application/json).
        // - X-Request-Id: Optional client-provided request ID for correlation.
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, X_REQUEST_ID])
        // Exposed headers: X-Request-Id so client-side JavaScript can read
        // the request ID for error reporting and log correlation.
        .expose_headers([X_REQUEST_ID])
        // Credentials: required because the G5 API uses httpOnly cookies
        // for refresh tokens (API_AND_STORAGE.md §4A). Without this,
        // browsers will not send cookies on cross-origin requests.
        .allow_credentials(true)
        // Max age: 24 hours. Tells browsers to cache preflight responses,
        // reducing the number of OPTIONS requests.
        .max_age(Duration::from_secs(86400))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CorsConfig;

    #[test]
    fn test_build_cors_layer_with_valid_origins() {
        let config = CorsConfig {
            allowed_origins: vec![
                "http://localhost:5173".to_string(),
                "https://app.g5.dev".to_string(),
            ],
        };

        // Should not panic — all origins are valid.
        let _layer = build_cors_layer(&config);
    }

    #[test]
    fn test_build_cors_layer_with_empty_origins() {
        let config = CorsConfig {
            allowed_origins: vec![],
        };

        // Should not panic — empty origin list is valid (no origins allowed).
        let _layer = build_cors_layer(&config);
    }

    #[test]
    fn test_build_cors_layer_skips_invalid_origins() {
        // Build a string containing a null byte, which HeaderValue rejects.
        let mut invalid = String::from("invalid");
        invalid.push('\0');
        invalid.push_str("origin");

        let config = CorsConfig {
            allowed_origins: vec![
                "http://localhost:5173".to_string(),
                invalid,
                "https://app.g5.dev".to_string(),
            ],
        };

        // Should not panic — invalid origin is skipped with a warning.
        let _layer = build_cors_layer(&config);
    }

    #[test]
    fn test_build_cors_layer_single_origin() {
        let config = CorsConfig {
            allowed_origins: vec!["http://localhost:5173".to_string()],
        };

        let _layer = build_cors_layer(&config);
    }

    #[test]
    fn test_build_cors_layer_multiple_origins() {
        let config = CorsConfig {
            allowed_origins: vec![
                "http://localhost:5173".to_string(),
                "http://localhost:3000".to_string(),
                "https://app.g5.dev".to_string(),
                "https://staging.g5.dev".to_string(),
            ],
        };

        let _layer = build_cors_layer(&config);
    }

    #[test]
    fn test_build_cors_layer_all_invalid_origins() {
        // Build strings containing null bytes, which HeaderValue rejects.
        let mut invalid1 = String::from("invalid");
        invalid1.push('\0');
        invalid1.push_str("origin1");

        let mut invalid2 = String::from("invalid");
        invalid2.push('\0');
        invalid2.push_str("origin2");

        let config = CorsConfig {
            allowed_origins: vec![invalid1, invalid2],
        };

        // Should not panic — all origins are invalid, layer is created with
        // no allowed origins.
        let _layer = build_cors_layer(&config);
    }
}
