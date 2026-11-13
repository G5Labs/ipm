//! Typed OCI errors.
//!
//! Variants are designed so the caller can map any failure into a single
//! `G5Error::Internal` envelope at the daemon boundary while preserving the
//! specific failure phase (auth, manifest, layer fetch, digest, cache I/O,
//! tar extract) in the error message for operators.

use g5_contracts::error::G5Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OciError {
    #[error("invalid image reference '{0}': {1}")]
    InvalidImageRef(String, String),

    #[error(
        "unsupported registry host '{0}'; this build supports docker.io, ghcr.io, \
         GAR (*-docker.pkg.dev), ECR (*.dkr.ecr.*.amazonaws.com), and ACR \
         (*.azurecr.io). public.ecr.aws and host:port registries are not supported"
    )]
    UnsupportedRegistry(String),

    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("registry HTTP error {status} for {url}: {body}")]
    RegistryHttp {
        status: u16,
        url: String,
        body: String,
    },

    #[error("authentication failed for {0}: {1}")]
    AuthFailed(String, String),

    #[error(
        "authentication required for {registry} image '{image}' (scope '{scope}'): \
         no credentials available for a private image. Supply credentials for the \
         target registry (precedence: injected config first, then the registry's \
         standard sources) — e.g. GHCR_TOKEN/GITHUB_TOKEN (with `read:packages`) \
         for ghcr.io; GCE workload identity or GOOGLE_APPLICATION_CREDENTIALS for \
         GAR; AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY or an EC2 instance profile \
         for ECR; AZURE_CLIENT_ID/AZURE_CLIENT_SECRET (+AZURE_TENANT_ID) or a \
         managed identity for ACR."
    )]
    AuthRequired {
        registry: String,
        image: String,
        scope: String,
    },

    #[error("manifest decode failed: {0}")]
    ManifestDecode(String),

    #[error("no manifest entry matched platform os=linux arch={arch}")]
    NoMatchingPlatform { arch: String },

    #[error("digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error(
        "invalid digest '{value}': {reason} (codex final-review B1 of PR #122 \
         requires sha256:<64 hex chars> exactly)"
    )]
    InvalidDigest { value: String, reason: String },

    #[error(
        "timed out after {waited_secs}s waiting for cache lock on {path}; \
         the holder may have crashed mid-extract (codex final-review B2 of \
         PR #122)"
    )]
    CacheLockTimeout { path: String, waited_secs: u64 },

    #[error("layer extraction failed for {digest}: {source}")]
    LayerExtract {
        digest: String,
        #[source]
        source: std::io::Error,
    },

    #[error("cache I/O failure at {path}: {source}")]
    CacheIo {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<OciError> for G5Error {
    fn from(value: OciError) -> Self {
        G5Error::Internal {
            message: crate::disk::classify_io(&value, "oci_preparation")
                .map(|pressure| pressure.reason())
                .unwrap_or_else(|| value.to_string()),
        }
    }
}

/// Bounded, sanitized snippet of a registry/token-endpoint response body for
/// inclusion in `OciError` messages. Caps at 200 chars and replaces any
/// whitespace / control char (including newlines and embedded NULs) with a
/// single space, so a misbehaving registry or upstream proxy cannot spill a
/// large HTML/JSON body — possibly containing line-breaking control sequences
/// — into operator-facing logs and structured errors.
pub(crate) fn truncate_body(body: &str) -> String {
    const MAX_CHARS: usize = 200;
    let mut out = String::with_capacity(MAX_CHARS);
    for ch in body.chars().take(MAX_CHARS) {
        if ch.is_whitespace() || ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_caps_at_200_chars() {
        let body = "x".repeat(500);
        let out = truncate_body(&body);
        assert_eq!(out.chars().count(), 200);
    }

    #[test]
    fn truncate_replaces_newlines_and_whitespace() {
        let body = "line1\n\tline2  \r\n  rest";
        let out = truncate_body(body);
        assert!(!out.contains('\n'));
        assert!(!out.contains('\t'));
        assert!(!out.contains('\r'));
        // Adjacent whitespace collapses visually via the per-char map; the
        // exact spacing isn't part of the contract — only that no control or
        // newline char survives.
    }

    #[test]
    fn truncate_short_body_unchanged_minus_outer_trim() {
        assert_eq!(truncate_body("  hello  "), "hello");
    }

    #[test]
    fn truncate_empty_is_empty() {
        assert_eq!(truncate_body(""), "");
    }
}
