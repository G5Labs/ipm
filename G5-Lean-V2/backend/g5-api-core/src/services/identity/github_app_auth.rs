//! The one GitHub App JWT signer, installation-token cache policy and
//! diagnostic redactor. The repository integration App and the platform report
//! App are different Apps: each owner keeps its own configuration and its own
//! cache instance, and maps failures to its own error meaning.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use g5_scm::providers::InstallationToken;
use serde::Serialize;
use tokio::sync::Mutex;

/// Refresh cached installation tokens this long before GitHub's expiry.
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);
/// Upper bound when GitHub omits or garbles `expires_at` (tokens last ~1h).
const TOKEN_MAX_CACHE: Duration = Duration::from_secs(50 * 60);

#[derive(Serialize)]
struct AppJwtClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

/// Sign an RS256 App JWT now. The error is a diagnostic for the owner's own
/// error type; it never contains the key.
pub(crate) fn sign_app_jwt(app_id: &str, private_key_pem: &str) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock error: {err}"))?
        .as_secs();
    sign_app_jwt_at(app_id, private_key_pem, now)
}

/// `iat` is backdated 60s for clock skew and `exp` is 9 minutes ahead
/// (GitHub allows at most 10).
pub(crate) fn sign_app_jwt_at(
    app_id: &str,
    private_key_pem: &str,
    now_secs: u64,
) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let claims = AppJwtClaims {
        iat: now_secs.saturating_sub(60),
        exp: now_secs + 9 * 60,
        iss: app_id.to_string(),
    };
    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|err| format!("invalid GitHub App private key: {err}"))?;
    encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|err| format!("App JWT signing failed: {err}"))
}

struct CachedToken {
    token: String,
    refresh_after: SystemTime,
}

/// Installation tokens cached per owner-chosen key until shortly before the
/// expiry GitHub reported. No lock is held while a token is issued.
pub(crate) struct InstallationTokenCache<K> {
    tokens: Mutex<HashMap<K, CachedToken>>,
}

impl<K: Eq + Hash> InstallationTokenCache<K> {
    pub(crate) fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn get(&self, key: &K) -> Option<String> {
        let now = SystemTime::now();
        self.tokens
            .lock()
            .await
            .get(key)
            .filter(|cached| cached.refresh_after > now)
            .map(|cached| cached.token.clone())
    }

    pub(crate) async fn insert(&self, key: K, issued: &InstallationToken) {
        let now = SystemTime::now();
        let refresh_after = refresh_after(now, issued.expires_at.as_deref());
        let mut tokens = self.tokens.lock().await;
        tokens.retain(|_, cached| cached.refresh_after > now);
        tokens.insert(
            key,
            CachedToken {
                token: issued.token.clone(),
                refresh_after,
            },
        );
    }
}

/// Reuse until the safety margin before GitHub's `expires_at`, never longer
/// than the conservative default.
fn refresh_after(now: SystemTime, expires_at: Option<&str>) -> SystemTime {
    let mut refresh_after = now + TOKEN_MAX_CACHE;
    if let Some(expires_at) =
        expires_at.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
    {
        let expires_at =
            UNIX_EPOCH + Duration::from_secs(u64::try_from(expires_at.timestamp()).unwrap_or(0));
        let early = expires_at
            .checked_sub(TOKEN_REFRESH_MARGIN)
            .unwrap_or(UNIX_EPOCH);
        refresh_after = refresh_after.min(early);
    }
    refresh_after
}

/// Scrub recognizable credentials, then drop URL-, `@`- and GitHub-token-shaped
/// words and bound the length, so neither credentials nor credentialed URLs
/// reach logs or persisted error text.
pub(crate) fn redact_message(message: &str) -> String {
    const MAX: usize = 500;
    let scrubbed = g5_contracts::credential_scrub::scrub(message);
    let sanitized = scrubbed
        .split_whitespace()
        .map(|token| {
            if token.contains("://")
                || token.contains('@')
                || ["ghs_", "ghu_", "gho_", "ghp_"]
                    .iter()
                    .any(|prefix| token.starts_with(prefix))
            {
                "[redacted]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut truncated = sanitized.chars().take(MAX).collect::<String>();
    if sanitized.chars().count() > MAX {
        truncated.push_str("...");
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_honors_reported_expiry_with_a_margin_and_bounds_unknown_expiry() {
        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        assert_eq!(refresh_after(now, None), now + TOKEN_MAX_CACHE);
        assert_eq!(refresh_after(now, Some("garbled")), now + TOKEN_MAX_CACHE);
        // A token expiring in ten minutes is reused for five, not fifty.
        let soon = chrono::DateTime::<chrono::Utc>::from(now + Duration::from_secs(600));
        assert_eq!(
            refresh_after(now, Some(&soon.to_rfc3339())),
            now + Duration::from_secs(300)
        );
        // An already-expiring token is never reused.
        let expired = chrono::DateTime::<chrono::Utc>::from(now + Duration::from_secs(60));
        assert!(refresh_after(now, Some(&expired.to_rfc3339())) < now);
    }

    #[tokio::test]
    async fn cache_entries_are_per_key_and_expire() {
        let cache = InstallationTokenCache::new();
        let issued = |token: &str, expires_at: Option<String>| InstallationToken {
            token: token.into(),
            expires_at,
        };
        cache
            .insert(1, &issued("ghs_one", Some("2999-01-01T00:00:00Z".into())))
            .await;
        cache
            .insert(2, &issued("ghs_stale", Some("2000-01-01T00:00:00Z".into())))
            .await;
        assert_eq!(cache.get(&1).await.as_deref(), Some("ghs_one"));
        assert_eq!(cache.get(&2).await, None);
        assert_eq!(cache.get(&3).await, None);
    }

    #[test]
    fn redact_message_strips_credentials_urls_and_bounds_length() {
        let out = redact_message(
            "failed https://x-access-token:ghs_secret@api.github.com/x ghs_raw plain glpat-abcdefghijklmnopqrstuvwxyz",
        );
        assert!(!out.contains("ghs_secret"));
        assert!(!out.contains("ghs_raw"));
        assert!(!out.contains("glpat-"));
        assert!(out.contains("plain"));
        let long = redact_message(&"word ".repeat(400));
        assert!(long.ends_with("...") && long.chars().count() == 503);
    }

    #[test]
    fn invalid_keys_fail_without_echoing_the_key() {
        let error = sign_app_jwt_at(
            "1",
            "-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----",
            1,
        )
        .unwrap_err();
        assert!(!error.contains("nope"));
    }
}
