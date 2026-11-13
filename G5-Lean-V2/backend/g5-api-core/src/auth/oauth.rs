//! OAuth authorization URL and state helpers. Live token/identity protocol
//! belongs to SCM for GitHub and to [`super::entra`] for Microsoft Entra ID.

use std::fmt;
use std::str::FromStr;

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::errors::G5Error;

// ---------------------------------------------------------------------------
// OAuthProvider enum
// ---------------------------------------------------------------------------

/// Supported identity providers for OAuth login.
///
/// Parsed from URL path segments (`:provider`) in route handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    GitHub,
    Microsoft,
}

impl fmt::Display for OAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OAuthProvider::GitHub => write!(f, "github"),
            OAuthProvider::Microsoft => write!(f, "microsoft"),
        }
    }
}

impl FromStr for OAuthProvider {
    type Err = AppError;

    /// Parses a provider name string (case-insensitive).
    ///
    /// # Errors
    /// Returns `AppError` wrapping `G5Error::ValidationError` for unknown
    /// provider names.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "github" => Ok(OAuthProvider::GitHub),
            "microsoft" => Ok(OAuthProvider::Microsoft),
            _ => Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": format!("Unknown OAuth provider: {}", s)
                }),
            })),
        }
    }
}

// ---------------------------------------------------------------------------
// OAuthProviderConfig
// ---------------------------------------------------------------------------

/// Configuration for a single OAuth provider.
///
/// Loaded from application configuration at startup. Contains all the
/// provider-specific URLs, credentials, and scopes needed for the OAuth
/// Authorization Code flow.
#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

// ---------------------------------------------------------------------------
// OAuthTokenResponse
// ---------------------------------------------------------------------------

/// Response from the provider's token endpoint after exchanging an
/// authorization code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub scope: Option<String>,
}

// ---------------------------------------------------------------------------
// OAuthUserInfo
// ---------------------------------------------------------------------------

/// Normalized user information extracted from the provider's user info
/// endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    pub oauth_id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub provider: String,
}

// ---------------------------------------------------------------------------
// OAuthRedirect
// ---------------------------------------------------------------------------

/// Returned when initiating an OAuth flow. Contains the URL to redirect
/// the user to and the state parameter for CSRF verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthRedirect {
    pub url: String,
    pub state: String,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Constructs the OAuth authorization URL that the user's browser should
/// be redirected to.
///
/// # URL construction
///
/// ```text
/// {config.authorize_url}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code
/// ```
///
/// All query parameters are properly URL-encoded.
#[must_use]
pub fn build_authorize_url(config: &OAuthProviderConfig, state: &str) -> String {
    let scopes_joined = config.scopes.join(" ");

    // Use url::form_urlencoded for proper percent-encoding of query parameters.
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &scopes_joined)
        .append_pair("state", state)
        .append_pair("response_type", "code")
        .finish();

    format!("{}?{}", config.authorize_url, query)
}

/// Generates a cryptographically random 256-bit (32-byte) state parameter
/// encoded as a 64-character lowercase hexadecimal string.
///
/// Used for CSRF prevention in OAuth flows.
///
/// # Requirements
///
/// - Uses `OsRng` for cryptographically secure randomness.
/// - Output is exactly 64 hex characters (lowercase).
#[must_use]
pub fn generate_oauth_state() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- OAuthProvider Display tests --

    #[test]
    fn test_display_github() {
        assert_eq!(OAuthProvider::GitHub.to_string(), "github");
    }

    #[test]
    fn test_display_microsoft() {
        assert_eq!(OAuthProvider::Microsoft.to_string(), "microsoft");
    }

    // -- OAuthProvider FromStr tests --

    #[test]
    fn test_from_str_lowercase() {
        assert_eq!(
            OAuthProvider::from_str("github").unwrap(),
            OAuthProvider::GitHub
        );
    }

    #[test]
    fn test_from_str_case_insensitive() {
        assert_eq!(
            OAuthProvider::from_str("GitHub").unwrap(),
            OAuthProvider::GitHub
        );
        assert_eq!(
            OAuthProvider::from_str("GITHUB").unwrap(),
            OAuthProvider::GitHub
        );
        assert_eq!(
            OAuthProvider::from_str("Microsoft").unwrap(),
            OAuthProvider::Microsoft
        );
        assert_eq!(
            OAuthProvider::from_str("MICROSOFT").unwrap(),
            OAuthProvider::Microsoft
        );
    }

    #[test]
    fn test_from_str_unknown_provider() {
        let result = OAuthProvider::from_str("bitbucket");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err.inner {
            G5Error::ValidationError { details } => {
                let msg = details["message"].as_str().unwrap();
                assert!(
                    msg.contains("Unknown OAuth provider: bitbucket"),
                    "Expected validation error message, got: {}",
                    msg
                );
            }
            other => panic!("Expected G5Error::ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn test_from_str_empty_string() {
        let result = OAuthProvider::from_str("");
        assert!(result.is_err());
    }

    // -- OAuthProvider Serde tests --

    #[test]
    fn test_serde_serialize() {
        let json = serde_json::to_string(&OAuthProvider::GitHub).unwrap();
        assert_eq!(json, "\"github\"");
    }

    #[test]
    fn test_serde_deserialize() {
        let provider: OAuthProvider = serde_json::from_str("\"github\"").unwrap();
        assert_eq!(provider, OAuthProvider::GitHub);

        let provider: OAuthProvider = serde_json::from_str("\"microsoft\"").unwrap();
        assert_eq!(provider, OAuthProvider::Microsoft);
    }

    // -- build_authorize_url tests --

    #[test]
    fn test_build_authorize_url_basic() {
        let config = OAuthProviderConfig {
            client_id: "test_client_id".into(),
            client_secret: "test_secret".into(),
            authorize_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            userinfo_url: "https://api.github.com/user".into(),
            scopes: vec!["user:email".into(), "read:user".into()],
            redirect_uri: "https://example.com/callback".into(),
        };
        let state = "abc123";

        let url = build_authorize_url(&config, state);

        assert!(url.starts_with("https://github.com/login/oauth/authorize?"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=abc123"));
        // Scopes are space-separated and URL-encoded
        assert!(url.contains("scope=user"));
        // redirect_uri should be URL-encoded
        assert!(url.contains("redirect_uri="));
    }

    #[test]
    fn test_build_authorize_url_encodes_special_chars() {
        let config = OAuthProviderConfig {
            client_id: "id with spaces".into(),
            client_secret: "secret".into(),
            authorize_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            userinfo_url: "https://example.com/userinfo".into(),
            scopes: vec!["scope one".into(), "scope two".into()],
            redirect_uri: "https://example.com/callback?extra=param".into(),
        };
        let state = "state&value";

        let url = build_authorize_url(&config, state);

        // URL should not contain unencoded ampersands in parameter values
        // (the & separating query params is fine)
        assert!(url.starts_with("https://example.com/auth?"));
        // Verify it's a parseable URL
        let parsed = reqwest::Url::parse(&url).unwrap();
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(pairs["client_id"], "id with spaces");
        assert_eq!(pairs["state"], "state&value");
        assert_eq!(pairs["response_type"], "code");
    }

    // -- generate_oauth_state tests --

    #[test]
    fn test_generate_oauth_state_length() {
        let state = generate_oauth_state();
        // 32 bytes * 2 hex chars per byte = 64 characters
        assert_eq!(state.len(), 64);
    }

    #[test]
    fn test_generate_oauth_state_is_hex() {
        let state = generate_oauth_state();
        assert!(
            state.chars().all(|c| c.is_ascii_hexdigit()),
            "State should only contain hex characters"
        );
    }

    #[test]
    fn test_generate_oauth_state_is_lowercase() {
        let state = generate_oauth_state();
        assert_eq!(state, state.to_lowercase(), "State should be lowercase hex");
    }

    #[test]
    fn test_generate_oauth_state_uniqueness() {
        let state1 = generate_oauth_state();
        let state2 = generate_oauth_state();
        assert_ne!(state1, state2, "Two generated states should be different");
    }

    // -- OAuthTokenResponse serde tests --

    #[test]
    fn test_token_response_full_deserialization() {
        let json = r#"{
            "access_token": "gho_abc123",
            "token_type": "bearer",
            "refresh_token": "ghr_xyz789",
            "expires_in": 7200,
            "scope": "user:email read:user"
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "gho_abc123");
        assert_eq!(resp.token_type, "bearer");
        assert_eq!(resp.refresh_token.as_deref(), Some("ghr_xyz789"));
        assert_eq!(resp.expires_in, Some(7200));
        assert_eq!(resp.scope.as_deref(), Some("user:email read:user"));
    }

    #[test]
    fn test_token_response_minimal_deserialization() {
        // GitHub doesn't return refresh_token or expires_in for OAuth App tokens
        let json = r#"{
            "access_token": "gho_abc123",
            "token_type": "bearer"
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "gho_abc123");
        assert_eq!(resp.token_type, "bearer");
        assert!(resp.refresh_token.is_none());
        assert!(resp.expires_in.is_none());
        assert!(resp.scope.is_none());
    }

    // -- OAuthUserInfo tests --

    #[test]
    fn test_user_info_serialization_roundtrip() {
        let info = OAuthUserInfo {
            oauth_id: "12345".into(),
            email: "user@example.com".into(),
            display_name: "Test User".into(),
            avatar_url: Some("https://example.com/avatar.png".into()),
            provider: "github".into(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: OAuthUserInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.oauth_id, "12345");
        assert_eq!(deserialized.email, "user@example.com");
        assert_eq!(deserialized.display_name, "Test User");
        assert_eq!(
            deserialized.avatar_url.as_deref(),
            Some("https://example.com/avatar.png")
        );
        assert_eq!(deserialized.provider, "github");
    }

    #[test]
    fn test_user_info_with_no_avatar() {
        let info = OAuthUserInfo {
            oauth_id: "67890".into(),
            email: "user@example.com".into(),
            display_name: "Test User".into(),
            avatar_url: None,
            provider: "github".into(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: OAuthUserInfo = serde_json::from_str(&json).unwrap();
        assert!(deserialized.avatar_url.is_none());
    }

    // -- OAuthRedirect tests --

    #[test]
    fn test_oauth_redirect_serialization() {
        let redirect = OAuthRedirect {
            url: "https://github.com/login/oauth/authorize?...".into(),
            state: "abc123def456".into(),
        };

        let json = serde_json::to_value(&redirect).unwrap();
        assert_eq!(json["url"], "https://github.com/login/oauth/authorize?...");
        assert_eq!(json["state"], "abc123def456");
    }

    // -- OAuthProvider equality and hash tests --

    #[test]
    fn test_provider_equality() {
        assert_eq!(OAuthProvider::GitHub, OAuthProvider::GitHub);
    }

    #[test]
    fn test_provider_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(OAuthProvider::GitHub);
        set.insert(OAuthProvider::Microsoft);
        assert_eq!(set.len(), 2);

        // Inserting duplicate should not increase size
        set.insert(OAuthProvider::GitHub);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_provider_copy_clone() {
        let provider = OAuthProvider::GitHub;
        let copied = provider; // Copy
        let cloned = provider; // Clone (Copy type)
        assert_eq!(provider, copied);
        assert_eq!(provider, cloned);
    }

    #[test]
    fn test_provider_debug() {
        let debug_str = format!("{:?}", OAuthProvider::GitHub);
        assert_eq!(debug_str, "GitHub");
    }
}
