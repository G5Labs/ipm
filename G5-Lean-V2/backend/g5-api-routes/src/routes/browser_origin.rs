//! Browser-origin validation helpers for CLI/device browser handoffs.
//!
//! Downloaded CLI/auth flows need two different origins:
//! - `apiUrl` for g5-api calls
//! - `webUrl` for browser routes such as `/cli/authorize`
//!
//! These helpers keep route handlers from treating those as interchangeable.

use axum::http::{header, HeaderMap, HeaderName};
use g5_contracts::error::G5Error;
use url::Url;

fn validation_error(field: &str, message: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({ field: message.into() }),
    }
}

pub(crate) fn normalize_browser_origin(value: &str) -> Result<String, G5Error> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(validation_error(
            "browserOrigin",
            "Browser origin must not be empty.",
        ));
    }

    let parsed = Url::parse(trimmed).map_err(|_| {
        validation_error(
            "browserOrigin",
            "Browser origin must be an absolute http or https URL origin.",
        )
    })?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(validation_error(
            "browserOrigin",
            "Browser origin must use http or https.",
        ));
    }
    if parsed.host_str().is_none() {
        return Err(validation_error(
            "browserOrigin",
            "Browser origin must include a host.",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(validation_error(
            "browserOrigin",
            "Browser origin must not include credentials.",
        ));
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(validation_error(
            "browserOrigin",
            "Browser origin must not include a path, query, or fragment.",
        ));
    }

    Ok(parsed.origin().ascii_serialization())
}

#[allow(dead_code)]
fn first_header_value(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[allow(dead_code)]
fn fallback_scheme(fallback_external_url: &str) -> &str {
    Url::parse(fallback_external_url)
        .ok()
        .and_then(|url| match url.scheme() {
            "http" => Some("http"),
            "https" => Some("https"),
            _ => None,
        })
        .unwrap_or("https")
}

#[allow(dead_code)]
pub(crate) fn request_browser_origin(
    headers: &HeaderMap,
    fallback_external_url: &str,
) -> Option<String> {
    if let Some(origin) = first_header_value(headers, header::ORIGIN) {
        if let Ok(normalized) = normalize_browser_origin(&origin) {
            return Some(normalized);
        }
    }

    let host = first_header_value(headers, HeaderName::from_static("x-forwarded-host"))
        .or_else(|| first_header_value(headers, header::HOST))?;
    let scheme = first_header_value(headers, HeaderName::from_static("x-forwarded-proto"))
        .unwrap_or_else(|| fallback_scheme(fallback_external_url).to_string());
    normalize_browser_origin(&format!("{scheme}://{host}")).ok()
}

#[allow(dead_code)]
pub(crate) fn resolve_browser_origin_for_request(
    requested_origin: Option<&str>,
    headers: &HeaderMap,
    fallback_external_url: &str,
) -> Result<String, G5Error> {
    let trusted_origin = request_browser_origin(headers, fallback_external_url);

    if let Some(requested_origin) = requested_origin.filter(|value| !value.trim().is_empty()) {
        let normalized = normalize_browser_origin(requested_origin)?;
        if let Some(trusted_origin) = trusted_origin {
            if normalized != trusted_origin {
                return Err(validation_error(
                    "browserOrigin",
                    format!(
                        "Browser origin {normalized} does not match request origin {trusted_origin}."
                    ),
                ));
            }
        }
        return Ok(normalized);
    }

    trusted_origin.ok_or_else(|| {
        validation_error(
            "browserOrigin",
            "Browser origin could not be derived from the request.",
        )
    })
}

pub(crate) fn resolve_browser_origin_from_cli_metadata(
    requested_origin: Option<&str>,
    fallback_external_url: &str,
) -> Result<String, G5Error> {
    if let Some(requested_origin) = requested_origin.filter(|value| !value.trim().is_empty()) {
        return normalize_browser_origin(requested_origin);
    }
    normalize_browser_origin(fallback_external_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn normalize_browser_origin_accepts_only_origins() {
        assert_eq!(
            normalize_browser_origin("https://app.example.com/").unwrap(),
            "https://app.example.com"
        );
        assert!(normalize_browser_origin("https://app.example.com/cli").is_err());
        assert!(normalize_browser_origin("ftp://app.example.com").is_err());
        assert!(normalize_browser_origin("https://user@app.example.com").is_err());
    }

    #[test]
    fn resolve_browser_origin_rejects_request_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://app.example.com"),
        );

        let err = resolve_browser_origin_for_request(
            Some("https://evil.example.com"),
            &headers,
            "https://api.example.com",
        )
        .unwrap_err();

        let G5Error::ValidationError { details } = err else {
            panic!("expected validation error");
        };
        assert!(details["browserOrigin"]
            .as_str()
            .unwrap()
            .contains("does not match request origin"));
    }

    #[test]
    fn resolve_browser_origin_uses_forwarded_host_when_origin_is_absent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-forwarded-proto"),
            HeaderValue::from_static("https"),
        );
        headers.insert(
            HeaderName::from_static("x-forwarded-host"),
            HeaderValue::from_static("g5.example.com"),
        );

        let origin = resolve_browser_origin_for_request(None, &headers, "http://api.internal")
            .expect("forwarded host should produce origin");

        assert_eq!(origin, "https://g5.example.com");
    }

    #[test]
    fn resolve_cli_metadata_uses_web_url_without_request_headers() {
        let origin = resolve_browser_origin_from_cli_metadata(
            Some("https://g5.example.com"),
            "http://api.internal",
        )
        .unwrap();
        assert_eq!(origin, "https://g5.example.com");
    }
}
