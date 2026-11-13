//! Browser-bound OAuth attempts for login and SCM connection
//! (SEC-M35; G5Labs/G5-Lean#449 and #456).
//!
//! A stored, single-use `state` proves only that this server started an
//! attempt: whoever holds the authorize or callback URL could finish it. Each
//! attempt is therefore also bound to the browser that started it.
//! Initiation sets a short-lived HttpOnly cookie holding a digest of the
//! attempt's exact state and a PKCE verifier, and the authorize URL carries
//! that verifier's S256 challenge. A callback is refused before the state is
//! consumed or a code exchanged unless this browser presents the cookie issued
//! for that state; the verifier then goes to the provider with the code.
//!
//! The cookie needs no server secret: it only has to be absent from any other
//! browser, which `HttpOnly` and `SameSite=Lax` already guarantee. In secure
//! deployments it carries the `__Host-` prefix, so a sibling subdomain such
//! as a preview origin (SEC-M43) cannot plant an attempt cookie of its own.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Lifetime of an attempt: its stored state and its browser cookie.
pub const OAUTH_ATTEMPT_TTL_SECS: i64 = 600;

const VERSION: &str = "g5oa1";

/// The OAuth flow an attempt belongs to; each keeps its own cookie, so a
/// login and an SCM connection in progress never replace each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthFlow {
    Login,
    ScmConnect,
}

impl OAuthFlow {
    #[must_use]
    pub const fn cookie_name(self, secure: bool) -> &'static str {
        match (self, secure) {
            (Self::Login, true) => "__Host-g5_oauth_login",
            (Self::Login, false) => "g5_oauth_login",
            (Self::ScmConnect, true) => "__Host-g5_oauth_scm",
            (Self::ScmConnect, false) => "g5_oauth_scm",
        }
    }

    /// `Set-Cookie` value carrying an attempt's cookie for this flow.
    #[must_use]
    pub fn set_cookie(self, value: &str, secure: bool) -> String {
        self.cookie(value, OAUTH_ATTEMPT_TTL_SECS, secure)
    }

    /// `Set-Cookie` value expiring this flow's attempt cookie.
    #[must_use]
    pub fn clear_cookie(self, secure: bool) -> String {
        self.cookie("", 0, secure)
    }

    fn cookie(self, value: &str, max_age: i64, secure: bool) -> String {
        // `__Host-` requires `Secure`, `Path=/` and no `Domain`.
        let secure_attr = if secure { "; Secure" } else { "" };
        format!(
            "{}={value}; HttpOnly{secure_attr}; SameSite=Lax; Path=/; Max-Age={max_age}",
            self.cookie_name(secure)
        )
    }
}

/// A new attempt for one state: the PKCE verifier and the cookie that binds
/// it, and the state, to the initiating browser.
pub struct OAuthAttempt {
    verifier: String,
    cookie: String,
}

impl OAuthAttempt {
    #[must_use]
    pub fn new(state: &str) -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let cookie = format!("{VERSION}.{}.{verifier}", state_digest(state));
        Self { verifier, cookie }
    }

    /// The S256 `code_challenge` for the authorize URL.
    #[must_use]
    pub fn code_challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    /// The cookie value to set on the initiating browser.
    #[must_use]
    pub fn cookie_value(&self) -> &str {
        &self.cookie
    }
}

/// The PKCE verifier from `cookie` when it was issued for exactly `state`;
/// `None` for no cookie, another attempt's cookie or a malformed one.
#[must_use]
pub fn attempt_verifier(cookie: Option<&str>, state: &str) -> Option<String> {
    let mut parts = cookie?.split('.');
    let (Some(VERSION), Some(digest), Some(verifier), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if verifier.len() != 43 {
        return None;
    }
    bool::from(digest.as_bytes().ct_eq(state_digest(state).as_bytes()))
        .then(|| verifier.to_string())
}

fn state_digest(state: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(state.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_cookie_continues_only_its_own_state() {
        let attempt = OAuthAttempt::new("state-a");
        let verifier = attempt_verifier(Some(attempt.cookie_value()), "state-a")
            .expect("own state is accepted");
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            attempt.code_challenge()
        );
        assert_eq!(
            attempt_verifier(Some(attempt.cookie_value()), "state-b"),
            None
        );
        assert_eq!(attempt_verifier(None, "state-a"), None);
    }

    #[test]
    fn malformed_attempt_cookies_are_refused() {
        let attempt = OAuthAttempt::new("s");
        let value = attempt.cookie_value();
        let (_, rest) = value.split_once('.').unwrap();
        for bad in [
            String::new(),
            format!("g5oa0.{rest}"),
            format!("{value}.extra"),
            value.rsplit_once('.').unwrap().0.to_string(),
            format!("{}.short", value.rsplit_once('.').unwrap().0),
        ] {
            assert_eq!(attempt_verifier(Some(&bad), "s"), None, "{bad}");
        }
    }

    #[test]
    fn secure_attempt_cookies_are_host_only() {
        let set = OAuthFlow::ScmConnect.set_cookie("v", true);
        assert_eq!(
            set,
            "__Host-g5_oauth_scm=v; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600"
        );
        assert!(!set.contains("Domain"));
        assert_eq!(
            OAuthFlow::Login.clear_cookie(false),
            "g5_oauth_login=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
        );
    }
}
