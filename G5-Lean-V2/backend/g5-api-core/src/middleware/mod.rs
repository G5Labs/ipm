//! Middleware module for g5-api.
//!
//! Assembles the Tower middleware stack applied to every HTTP request.
//! Stack execution order (outermost to innermost):
//!
//! Request ID → Client IP → Preview Security → CORS → Compression → Rate Limit → Auth → User-Action Audit → Handler
//!
//! In axum, `.layer()` calls are applied in reverse order — the **last**
//! `.layer()` call wraps the **outermost** layer. Therefore, `RequestIdLayer`
//! must be the last `.layer()` call to ensure it executes first on every
//! request.

use std::sync::Arc;

use axum::Router;
use g5_auth::JwtValidator;
use sqlx::PgPool;

use crate::auth::HostAuthValidator;
use crate::config::AppConfig;

// ---------------------------------------------------------------------------
// Submodule declarations
// ---------------------------------------------------------------------------

pub mod audit_log;
pub mod auth_middleware;
pub mod authenticated;
pub mod client_ip;
pub mod compression;
pub mod cors;
pub mod preview_host;
mod preview_security;
pub mod rate_limit;
pub mod request_id;

#[doc(hidden)]
pub mod test_capture;

// ---------------------------------------------------------------------------
// Re-exports for convenient access from other modules
// ---------------------------------------------------------------------------

// From request_id.rs
pub use request_id::RequestId;
pub use request_id::RequestIdLayer;

// From audit_log.rs — per-user-action g5_audit stream (issue #1552)
pub use audit_log::UserActionAuditLayer;

// From auth_middleware.rs
pub use auth_middleware::extract_session_user_id;
pub use auth_middleware::require_browser_session;
pub use auth_middleware::AuthContext;
pub use auth_middleware::AuthLayer;
pub use auth_middleware::RequireScopes;
pub use auth_middleware::PREVIEW_REAUTH_MARKER_PARAM;
pub use auth_middleware::PREVIEW_TICKET_COOKIE_HOST;
pub use auth_middleware::{PreviewAgentContext, WorkspaceIncarnationContext};

// From preview_host.rs — per-preview subdomain origin (issue #935)
pub use preview_host::preview_host_middleware;
pub use preview_host::PreviewHostOrigin;

// From authenticated.rs — type-state auth extractors
pub use authenticated::AuthenticatedActor;
pub use authenticated::AuthenticatedHost;
pub use authenticated::AuthenticatedUser;

// From rate_limit.rs
pub use rate_limit::InMemoryRateLimitStore;
pub use rate_limit::RateLimitConfig;
pub use rate_limit::RateLimitError;
pub use rate_limit::RateLimitLayer;
pub use rate_limit::RateLimitStore;
pub use rate_limit::{run_rate_limit_window_trim, PostgresRateLimitStore};

// From cors.rs
pub use cors::build_cors_layer;

// From compression.rs
pub use compression::build_compression_layer;

// ---------------------------------------------------------------------------
// Middleware stack assembly
// ---------------------------------------------------------------------------

/// Assembles the complete Tower middleware stack and applies it to the given router.
///
/// # Arguments
///
/// * `router` — Axum Router with all routes defined but no middleware applied.
/// * `config` — Application configuration (used for CORS origins, rate limit settings).
/// * `jwt_validator` — Pre-constructed JWT validator (ES256 public key).
/// * `rate_limit_store` — In-memory rate limit store (caller retains Arc for cleanup scheduling).
///
/// # Middleware Ordering (SECURITY-CRITICAL)
///
/// Layers are applied in reverse order (last `.layer()` = outermost):
///
/// 1. Request ID (outermost) — generates `req_<nanoid>`, sets `x-request-id` header
/// 2. Client IP — derives the shared identity from transport and configured proxy authority
/// 3. Preview Security — fences and sandboxes platform-origin preview requests
/// 4. CORS — handles preflight OPTIONS, must be before auth
/// 5. Compression — compresses response bodies (gzip, brotli, deflate)
/// 6. Rate Limit — per-IP token bucket, protects auth from brute-force
/// 7. Auth — JWT/service token validation, stores AuthContext
/// 8. User-Action Audit (innermost) — one `g5_audit` event per in-scope
///    authenticated user action (issue #1552); must be inside auth so it
///    observes the inserted AuthContext and the handler's response status
///
/// # Returns
///
/// The input router with all middleware layers applied.
pub fn build_middleware_stack(
    router: Router,
    config: &AppConfig,
    jwt_validator: Arc<JwtValidator>,
    rate_limit_store: Arc<InMemoryRateLimitStore>,
    api_key_pool: Option<PgPool>,
    host_validator: Option<Arc<dyn HostAuthValidator>>,
) -> Router {
    let client_ip_policy = client_ip::ClientIpPolicy::new(
        config.server.trusted_proxy_cidrs.clone(),
        config.server.trusted_proxy_xff_hops,
    );
    // Build individual layers from configuration
    let cors_layer = cors::build_cors_layer(&config.cors);
    let compression_layer = compression::build_compression_layer();
    let rate_limit_config = RateLimitConfig {
        requests_per_minute: config.server.rate_limit_rpm,
        burst_size: config.server.rate_limit_rpm,
        enabled: config.server.rate_limit_enabled,
    };
    let rate_limit_layer = RateLimitLayer::new(
        rate_limit_store as Arc<dyn RateLimitStore>,
        rate_limit_config,
    );
    // Trusted main origin for the preview re-auth redirect (routes/preview
    // [D18]): validated at startup so a misconfigured G5_EXTERNAL_URL fails
    // loudly here instead of silently redirecting browsers to an origin
    // without the g5_refresh cookie. Never derived from request headers.
    let main_origin = canonical_main_origin(&config.server.external_url).unwrap_or_else(|e| {
        panic!(
            "Invalid G5_EXTERNAL_URL {:?} (required for the preview re-auth redirect): {e}",
            config.server.external_url
        )
    });
    // [D15] embedded-preview deployment invariant: with a preview suffix
    // configured, workspace and previews must be schemeful same-site or the
    // embed's SameSite=Lax cookie silently never attaches in the iframe.
    // Fail loudly at startup instead (PR #1283 review, chat intent 00fd38ee).
    if let Some(suffix) = config.server.preview_host_suffix.as_deref() {
        if let Err(e) = validate_preview_suffix_same_site(&main_origin, suffix) {
            panic!(
                "Invalid G5_EXTERNAL_URL / G5_PREVIEW_HOST_SUFFIX pair (the embedded \
                 Preview Browser requires a schemeful same-site deployment): {e}"
            );
        }
    }
    let auth_layer = AuthLayer::new(jwt_validator).with_main_origin(main_origin);
    let auth_layer = match api_key_pool {
        Some(pool) => auth_layer.with_api_key_pool(pool),
        None => auth_layer,
    };
    let auth_layer = match host_validator {
        Some(validator) => auth_layer.with_host_validator(validator),
        None => auth_layer,
    };

    tracing::info!(
        "Middleware stack assembled: RequestId → ClientIp → PreviewSecurity → CORS → Compression → RateLimit → Auth → UserActionAudit"
    );

    // Apply layers in reverse order — last .layer() = outermost middleware
    // Execution order: RequestId → ClientIp → PreviewSecurity → CORS → Compression → RateLimit → Auth →
    // UserActionAudit → Handler
    router
        .layer(audit_log::UserActionAuditLayer) // innermost (runs last before handler)
        .layer(auth_layer)
        .layer(rate_limit_layer)
        .layer(compression_layer)
        .layer(cors_layer)
        // Wrap auth, errors and CORS preflights as well as redirects.
        .layer(axum::middleware::from_fn(
            preview_security::platform_preview_path_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            client_ip_policy,
            client_ip::resolve_client_ip,
        ))
        .layer(RequestIdLayer) // outermost — position 1 (runs first on every request)
}

/// Canonicalizes and validates the browser-facing main origin
/// (`config.server.external_url`) for the preview re-auth redirect
/// (routes/preview [D18]) and the subdomain-mode `frame-ancestors`
/// workspace allowance (routes/preview [D16]): http(s) scheme, a host, no
/// userinfo, no query/fragment; trailing slashes are trimmed. Anything
/// else is a deployment error and must fail startup.
pub fn canonical_main_origin(external_url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(external_url).map_err(|e| format!("not a URL: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("scheme must be http(s), got {:?}", parsed.scheme()));
    }
    if parsed.host_str().is_none() {
        return Err("missing host".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("userinfo not allowed".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("query/fragment not allowed".to_string());
    }
    // Origin-only: a base path (https://host/app) would put the re-auth
    // redirect under /app/api/v1/auth/... where no route exists.
    if parsed.path() != "/" {
        return Err(format!("path must be '/', got {:?}", parsed.path()));
    }
    // The ASCII serialization (punycode host, lowercased, default port
    // elided), NOT the raw input: a valid Unicode/IDN G5_EXTERNAL_URL parses
    // fine, but a raw Unicode origin fails HTTP header-value construction
    // when [D16] rebuilds `frame-ancestors` AFTER X-Frame-Options was
    // already removed — silently leaving the response with no anti-framing
    // policy (routes/preview [D16], PR #1283 review P0).
    Ok(parsed.origin().ascii_serialization())
}

/// Startup validation for the routes/preview [D15] embedded-preview
/// deployment invariant: with `G5_PREVIEW_HOST_SUFFIX` configured, the
/// workspace (`server.external_url`) and the preview origins
/// (`<uuid>.<suffix>`) must be schemeful same-site — https scheme
/// (subdomain previews are served https-only, routes/preview [D13]/[D15];
/// a cross-SCHEME pair is cross-site even on one registrable domain) and a
/// shared Public-Suffix-List registrable domain. An unrelated pair would
/// make the embed's `SameSite=Lax` handoff cookie silently unavailable
/// inside the iframe: every embedded login would break after handoff while
/// top-level flows kept working, so it is a deployment error and must fail
/// startup (PR #1283 review, chat intent 00fd38ee).
/// Canonicalize and validate `G5_PREVIEW_HOST_SUFFIX` as a bare IDNA-ASCII
/// domain before it is interpolated into `https://<uuid>.<suffix>/` preview
/// URLs and the PSL same-site check (routes/preview [D13]/[D15]).
///
/// `psl::domain_str` is a registrable-domain EXTRACTOR, not a hostname
/// validator: it returns `Some("g5labs.ai")` for `preview..g5labs.ai` and
/// `https://preview.g5labs.ai` alike, so those malformed values would pass
/// the same-site check and then be interpolated verbatim into the mint/reauth
/// handoff URLs, minting broken previews despite the startup guard (PR #1283
/// review P1). `url::Host::parse` runs IDNA (Unicode → punycode) and rejects
/// a scheme, userinfo, port, or path, but it still accepts empty DNS labels
/// (`preview..g5labs.ai`, `.g5labs.ai`, `g5labs.ai.`), so those are rejected
/// explicitly here. Returns the lowercase-punycode canonical form.
pub(crate) use g5_api_foundation::config::canonical_preview_host_suffix;

pub(crate) fn validate_preview_suffix_same_site(
    main_origin: &str,
    preview_host_suffix: &str,
) -> Result<(), String> {
    // Canonicalize + validate the suffix first: a malformed suffix must fail
    // startup here rather than silently reach `https://<uuid>.<suffix>/`
    // ([D15], PR #1283 review P1). The canonical form is what the PSL check
    // below (and the mint/reauth handoff URLs) use.
    let canonical_suffix = canonical_preview_host_suffix(preview_host_suffix)?;
    let parsed =
        url::Url::parse(main_origin).map_err(|e| format!("main origin is not a URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err(format!(
            "G5_EXTERNAL_URL scheme must be https when G5_PREVIEW_HOST_SUFFIX is \
             configured (subdomain previews are served https-only, and a \
             cross-scheme pair is cross-site: the embedded Preview Browser's \
             SameSite=Lax session cookie would be withheld in the iframe), got \
             {:?}",
            parsed.scheme()
        ));
    }
    let Some(host) = parsed.host_str() else {
        return Err("main origin has no host".to_string());
    };
    // A preview host is `<uuid>.<suffix>`; the extra leading label never
    // changes the registrable domain, so the suffix itself is validated.
    let workspace_site = psl::domain_str(host);
    let preview_site = psl::domain_str(canonical_suffix.as_str());
    match (workspace_site, preview_site) {
        (Some(w), Some(p)) if w.eq_ignore_ascii_case(p) => Ok(()),
        (w, p) => Err(format!(
            "G5_EXTERNAL_URL host {host:?} (registrable domain {w:?}) and \
             G5_PREVIEW_HOST_SUFFIX {canonical_suffix:?} (registrable domain \
             {p:?}) are not same-site: the embedded Preview Browser's \
             SameSite=Lax session cookie would be withheld in the iframe and \
             every embedded login would break after the handoff"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_main_origin, canonical_preview_host_suffix, validate_preview_suffix_same_site,
    };

    #[test]
    fn canonical_main_origin_accepts_and_trims() {
        assert_eq!(
            canonical_main_origin("https://dev.g5labs.ai/").unwrap(),
            "https://dev.g5labs.ai"
        );
        assert_eq!(
            canonical_main_origin("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
    }

    #[test]
    fn canonical_main_origin_ascii_serializes_idn_hosts() {
        // [D16] P0 regression (PR #1283 review): a Unicode/IDN
        // G5_EXTERNAL_URL must canonicalize to the punycode ASCII origin so
        // the subdomain-mode `frame-ancestors` HeaderValue reconstruction
        // can never fail after X-Frame-Options was already stripped.
        let origin = canonical_main_origin("https://münchen.example/").unwrap();
        assert_eq!(origin, "https://xn--mnchen-3ya.example");
        assert!(origin.is_ascii());
        assert!(http::HeaderValue::from_str(&format!("frame-ancestors 'self' {origin}")).is_ok());
    }

    #[test]
    fn canonical_main_origin_rejects_bad_origins() {
        assert!(canonical_main_origin("ftp://x").is_err());
        assert!(canonical_main_origin("https://u:p@host").is_err());
        assert!(canonical_main_origin("https://host/?q=1").is_err());
        assert!(canonical_main_origin("https://host/#f").is_err());
        assert!(canonical_main_origin("https://host/app").is_err());
        assert!(canonical_main_origin("not a url").is_err());
    }

    #[test]
    fn preview_suffix_same_site_accepts_shared_registrable_domain() {
        // The production shape: workspace dev.g5labs.ai, previews
        // <uuid>.preview.dev.g5labs.ai — registrable domain g5labs.ai.
        assert!(validate_preview_suffix_same_site(
            "https://dev.g5labs.ai",
            "preview.dev.g5labs.ai"
        )
        .is_ok());
        // A sibling (not nested) suffix on the same registrable domain is
        // still same-site.
        assert!(
            validate_preview_suffix_same_site("https://dev.g5labs.ai", "preview.g5labs.ai").is_ok()
        );
    }

    #[test]
    fn preview_suffix_same_site_rejects_unrelated_domains_and_http() {
        // Unrelated registrable domains → the Lax cookie would be withheld
        // in the iframe ([D15] deployment invariant).
        assert!(validate_preview_suffix_same_site(
            "https://dev.g5labs.ai",
            "preview.othersite.com"
        )
        .is_err());
        // Cross-scheme is cross-site even on one registrable domain.
        assert!(
            validate_preview_suffix_same_site("http://dev.g5labs.ai", "preview.dev.g5labs.ai")
                .is_err()
        );
        // IP-address and single-label hosts have no registrable domain.
        assert!(
            validate_preview_suffix_same_site("https://127.0.0.1", "preview.dev.g5labs.ai")
                .is_err()
        );
        assert!(
            validate_preview_suffix_same_site("https://localhost", "preview.localhost").is_err()
        );
    }

    #[test]
    fn canonical_preview_host_suffix_accepts_and_canonicalizes() {
        // A bare ASCII domain passes through lowercased.
        assert_eq!(
            canonical_preview_host_suffix("Preview.Dev.G5Labs.AI").unwrap(),
            "preview.dev.g5labs.ai"
        );
        // Surrounding whitespace is trimmed.
        assert_eq!(
            canonical_preview_host_suffix("  preview.g5labs.ai  ").unwrap(),
            "preview.g5labs.ai"
        );
        // A Unicode/IDN suffix is canonicalized to its punycode ASCII form so
        // the interpolated `https://<uuid>.<suffix>/` URL is well-formed
        // (PR #1283 review P1).
        let idn = canonical_preview_host_suffix("preview.münchen.example").unwrap();
        assert_eq!(idn, "preview.xn--mnchen-3ya.example");
        assert!(idn.is_ascii());
    }

    #[test]
    fn canonical_preview_host_suffix_rejects_malformed() {
        // `psl::domain_str` accepts all of these (registrable-domain
        // extraction, not hostname validation); the canonicalizer must not.
        for bad in [
            "",
            "   ",
            "https://preview.g5labs.ai", // scheme
            "preview.g5labs.ai:8443",    // port
            "user@preview.g5labs.ai",    // userinfo
            "preview.g5labs.ai/app",     // path
            "preview..g5labs.ai",        // empty inner label
            ".g5labs.ai",                // empty leading label
            "g5labs.ai.",                // empty trailing label
            "192.0.2.1",                 // IP literal, not a domain
        ] {
            assert!(
                canonical_preview_host_suffix(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn preview_suffix_same_site_rejects_malformed_suffix() {
        // A malformed suffix that `psl::domain_str` would still resolve to
        // g5labs.ai must fail startup rather than reach URL interpolation.
        assert!(validate_preview_suffix_same_site(
            "https://dev.g5labs.ai",
            "https://preview.g5labs.ai"
        )
        .is_err());
        assert!(
            validate_preview_suffix_same_site("https://dev.g5labs.ai", "preview..g5labs.ai")
                .is_err()
        );
    }
}
