//! Git authentication types for remote operations.
//!
//! Provides [`GitAuth`], the credential the native runner injects per
//! invocation, and [`GitAuthor`] for commit author/committer information.

use crate::ScmProvider;
pub use g5_contracts::GitAuthor;

/// Authentication method for git remote operations (fetch, push, clone).
///
/// Only the native runner turns this into an authenticated invocation, per
/// invocation and bound to the destination; see `native::process`.
#[derive(Clone)]
#[non_exhaustive]
pub enum GitAuth {
    /// Platform transport token authentication.
    ///
    /// Per-user provider credentials MUST use `ScmToken`; this variant is for
    /// existing G5-to-G5 transports whose endpoint policy is owned by the caller.
    ///
    /// Sent as basic auth with user `x-access-token`; GitHub, GitLab, and
    /// Bitbucket all accept HTTPS tokens as the password.
    Token(String),

    /// Per-user SCM credential. Its provider origin is carried through every
    /// clone, fetch, push and retry instead of trusting repository metadata.
    ScmToken {
        token: String,
        provider: ScmProvider,
    },

    /// SSH key authentication through a private per-invocation key file.
    SshKey {
        private_key: String,
        passphrase: Option<String>,
    },

    /// No authentication (public repos and local remotes).
    None,
}

// Custom Debug to prevent accidental token/key leakage in logs.
impl std::fmt::Debug for GitAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitAuth::Token(_) => f.debug_tuple("Token").field(&"[REDACTED]").finish(),
            GitAuth::ScmToken { provider, .. } => f
                .debug_struct("ScmToken")
                .field("provider", provider)
                .field("token", &"[REDACTED]")
                .finish(),
            GitAuth::SshKey { passphrase, .. } => f
                .debug_struct("SshKey")
                .field("private_key", &"[REDACTED]")
                .field("passphrase", &passphrase.as_ref().map(|_| "[REDACTED]"))
                .finish(),
            GitAuth::None => write!(f, "None"),
        }
    }
}

impl GitAuth {
    /// Issue a credential only for a supported provider and its exact HTTPS
    /// origin. Persisted provider metadata alone never authorizes a destination.
    #[cfg(test)]
    pub fn scm_token(token: String, provider: &str, git_url: &str) -> Self {
        match ScmProvider::from_name(provider).filter(|p| p.allows(git_url)) {
            Some(provider) => Self::scm_token_for_provider(token, provider, git_url),
            None => Self::None,
        }
    }

    /// Bind a token to an origin selected from trusted operator configuration.
    pub fn scm_token_for_provider(token: String, provider: ScmProvider, git_url: &str) -> Self {
        if provider.allows(git_url) {
            Self::ScmToken { token, provider }
        } else {
            Self::None
        }
    }

    /// GitHub installation and personal tokens use HTTPS even when the
    /// repository identity was entered using GitHub's SSH clone syntax.
    /// This changes only the transport; callers retain the captured identity.
    pub(crate) fn transport_url<'a>(&self, url: &'a str) -> std::borrow::Cow<'a, str> {
        if matches!(self, Self::Token(_)) {
            let path = url
                .strip_prefix("git@github.com:")
                .or_else(|| url.strip_prefix("ssh://git@github.com/"));
            if let Some(path) = path {
                if !path.is_empty() && !path.contains(['?', '#']) {
                    return std::borrow::Cow::Owned(format!("https://github.com/{path}"));
                }
            }
        }
        std::borrow::Cow::Borrowed(url)
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    #[test]
    fn scm_credentials_bind_only_the_exact_provider_origin() {
        assert!(matches!(
            GitAuth::scm_token("secret".into(), "github", "https://github.com/o/r.git"),
            GitAuth::ScmToken { .. }
        ));
        for url in [
            "https://github.com.attacker.example/o/r.git",
            "https://gitlab.com/o/r.git",
            "http://github.com/o/r.git",
            "https://github.com:8443/o/r.git",
            "https://user@github.com/o/r.git",
        ] {
            assert!(
                matches!(
                    GitAuth::scm_token("secret".into(), "github", url),
                    GitAuth::None
                ),
                "{url}"
            );
        }
        let auth = GitAuth::scm_token(
            "sensitive-token-value".into(),
            "github",
            "https://github.com/o/r.git",
        );
        assert!(!format!("{auth:?}").contains("sensitive-token-value"));
    }

    /// A token bound to the provider origin is refused before contacting any
    /// other server, so no challenge can extract it.
    #[test]
    fn foreign_challenge_cannot_extract_scm_token() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            },
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let server_done = done.clone();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !server_done.load(Ordering::SeqCst) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = Vec::new();
                        let mut byte = [0];
                        while socket.read_exact(&mut byte).is_ok() {
                            request.push(byte[0]);
                            if request.ends_with(b"\r\n\r\n") {
                                break;
                            }
                        }
                        requests.push(String::from_utf8_lossy(&request).to_ascii_lowercase());
                        let _ = socket.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=git\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            requests
        });
        let auth = GitAuth::ScmToken {
            token: "stored-member-token".into(),
            provider: ScmProvider::GitHub,
        };
        let git = crate::native::NativeGit::cli(Duration::from_secs(30)).unwrap();
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(git.remote_branches(&format!("http://{address}/github.com/r.git"), &auth));
        done.store(true, Ordering::SeqCst);
        let requests = server.join().unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.effect, crate::FailureEffect::Refused);
        assert!(!error.to_string().contains("stored-member-token"));
        assert!(
            requests.iter().all(|r| !r.contains("authorization:")),
            "untrusted server received credentials"
        );
    }

    #[test]
    fn github_token_transport_preserves_identity() {
        let auth = GitAuth::Token("secret".into());
        for input in [
            "git@github.com:org/repo.git",
            "ssh://git@github.com/org/repo.git",
        ] {
            assert_eq!(auth.transport_url(input), "https://github.com/org/repo.git");
            assert_eq!(GitAuth::None.transport_url(input), input);
        }
        for input in [
            "https://github.com/org/repo.git",
            "git@github.com.evil:org/repo.git",
            "ssh://git@github.com:2222/org/repo.git",
        ] {
            assert_eq!(auth.transport_url(input), input);
        }
    }

    #[test]
    fn test_git_auth_token_debug_redacted() {
        let auth = GitAuth::Token("my-secret-token".to_string());
        let debug = format!("{:?}", auth);
        assert!(debug.contains("Token"));
        assert!(debug.contains("REDACTED"));
        assert!(
            !debug.contains("my-secret-token"),
            "token must not appear in Debug output"
        );
    }

    #[test]
    fn test_git_auth_ssh_key_debug_redacted() {
        let auth = GitAuth::SshKey {
            private_key: "super-secret-key".to_string(),
            passphrase: Some("hunter2".to_string()),
        };
        let debug = format!("{:?}", auth);
        assert!(debug.contains("SshKey"));
        assert!(debug.contains("REDACTED"));
        assert!(
            !debug.contains("super-secret-key"),
            "private key must not appear in Debug output"
        );
        assert!(
            !debug.contains("hunter2"),
            "passphrase must not appear in Debug output"
        );
    }

    #[test]
    fn test_git_auth_none_debug() {
        let auth = GitAuth::None;
        let debug = format!("{:?}", auth);
        assert!(debug.contains("None"));
    }
}
