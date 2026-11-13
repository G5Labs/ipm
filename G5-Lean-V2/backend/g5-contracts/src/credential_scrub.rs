//! Credential scrubber — removes SCM token material from free-text
//! strings before persistence.
//!
//! This module is the implementation of
//! [`auth_identity_access.security_view.md` D16](../../../../views/security_views/auth_identity_access.security_view.md):
//! free-text columns owned by background SCM cycles (e.g.,
//! `branch_sync_runs.error_detail`, `branch_integration_queue.last_error`)
//! MUST be passed through this scrubber before they reach the database.
//! Direct interpolation of `git2::Error` or `reqwest::Error` into those
//! columns is forbidden.
//!
//! # Design — denylist regex
//!
//! Per the C3 decision and security view Q5, the scrubber implements a
//! denylist of known token shapes. It is brittle by construction — a new
//! token format ships and the scrubber doesn't catch it. The
//! mitigations:
//!
//! 1. The scrubber covers every shape that exists today (GitHub OAuth,
//!    GitHub App, GitLab, generic HTTPS basic-auth URL forms).
//! 2. Phase B's GitHub App layer will revisit this list when new token
//!    formats appear.
//! 3. The integration worker uses origin-bound `GitAuth::ScmToken` via the git2
//!    credentials callback (per security view D15) — that path keeps
//!    plaintext tokens out of error strings in the first place. The
//!    scrubber is the second line of defense, not the first.
//!
//! # Output
//!
//! Detected token material is replaced with `[REDACTED:<class>]` where
//! `<class>` is one of `github_token`, `gitlab_token`, `https_basic_auth`,
//! `ssh_key`. The classification is best-effort; in ambiguous cases the
//! more specific match wins.

use std::sync::OnceLock;

use regex::Regex;

/// Combined regex matching every known token shape. The first matching
/// alternative wins; we prioritize specific token prefixes over generic
/// URL forms so attribution is accurate.
fn token_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Order matters: more-specific patterns first so they win the
        // `replace_all` race.
        Regex::new(
            r#"(?x)
            (?P<github>          gh[posu]_[A-Za-z0-9]{20,255})                  # gho_, ghp_, ghs_, ghu_
            |
            (?P<gitlab>          glpat-[A-Za-z0-9\-_]{20,64})                   # glpat-
            |
            (?P<https_basic>     https://                                       # protocol
                                 (?:x-access-token|oauth2|[A-Za-z0-9_\-]{1,40}):
                                 [^@/\s]{4,1024}                                # password (token)
                                 @[A-Za-z0-9\.\-_]+                             # host
                                 (?:/[^\s]*)?                                   # path
                                )
            |
            (?P<ssh_key>         -----BEGIN\ (?:RSA\ |DSA\ |EC\ |OPENSSH\ )?PRIVATE\ KEY-----
                                 [\s\S]{0,8192}?
                                 -----END\ (?:RSA\ |DSA\ |EC\ |OPENSSH\ )?PRIVATE\ KEY-----)
            "#,
        )
        .expect("credential_scrub regex must compile at startup")
    })
}

/// Scrub a string of any credential material we can recognize.
///
/// Returns a new `String`. Allocates only when at least one match is
/// found — common case where the input is clean returns an owned clone
/// (cheap for typical short error strings).
///
/// # Output replacement values
///
/// | Match | Replacement |
/// |---|---|
/// | `gho_*` / `ghp_*` / `ghs_*` / `ghu_*` | `[REDACTED:github_token]` |
/// | `glpat-*` | `[REDACTED:gitlab_token]` |
/// | `https://user:token@host/…` | `[REDACTED:https_basic_auth]` |
/// | PEM private key block | `[REDACTED:ssh_key]` |
#[must_use]
pub fn scrub(input: &str) -> String {
    let re = token_pattern();
    let mut last_end = 0usize;
    let mut out = String::with_capacity(input.len());
    for caps in re.captures_iter(input) {
        let m = caps.get(0).expect("group 0 always matches");
        out.push_str(&input[last_end..m.start()]);
        let class = if caps.name("github").is_some() {
            "github_token"
        } else if caps.name("gitlab").is_some() {
            "gitlab_token"
        } else if caps.name("https_basic").is_some() {
            "https_basic_auth"
        } else if caps.name("ssh_key").is_some() {
            "ssh_key"
        } else {
            "credential"
        };
        out.push_str("[REDACTED:");
        out.push_str(class);
        out.push(']');
        last_end = m.end();
    }
    out.push_str(&input[last_end..]);
    out
}

/// Convenience: scrub an `Option<String>`. Returns `None` unchanged;
/// passes `Some` through `scrub` and rewraps.
#[must_use]
pub fn scrub_opt(input: Option<String>) -> Option<String> {
    input.map(|s| scrub(&s))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_string_passes_through() {
        let s = "fetch failed: connection refused on origin";
        let scrubbed = scrub(s);
        assert_eq!(scrubbed, s);
    }

    #[test]
    fn empty_string_passes_through() {
        assert_eq!(scrub(""), "");
    }

    #[test]
    fn redacts_github_oauth_token() {
        let s = "git push failed with token gho_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890 in URL";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:github_token]"), "got: {out}");
        assert!(
            !out.contains("AbCdEfGhIjKlMnOpQrSt"),
            "token substring leaked: {out}"
        );
    }

    #[test]
    fn redacts_github_personal_token() {
        let s = "Auth: ghp_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:github_token]"));
        assert!(!out.contains("ghp_AbCd"));
    }

    #[test]
    fn redacts_github_server_token() {
        let s = "ghs_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890 expired";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:github_token]"));
    }

    #[test]
    fn redacts_github_user_token() {
        let s = "ghu_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890 revoked";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:github_token]"));
    }

    #[test]
    fn redacts_gitlab_token() {
        let s = "GitLab token glpat-AbCdEfGhIjKlMnOpQrSt got 401";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:gitlab_token]"), "got: {out}");
        assert!(!out.contains("glpat-AbCd"));
    }

    #[test]
    fn redacts_https_basic_auth_x_access_token() {
        let s = "fatal: remote at https://x-access-token:gho_topsecrettoken123@github.com/owner/repo.git rejected push";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:"), "got: {out}");
        assert!(
            !out.contains("topsecrettoken"),
            "token substring leaked: {out}"
        );
        // x-access-token URL is a stronger match than the gho_ pattern;
        // the URL form wins because URL match is longer than the token
        // alone — but BOTH classifications are acceptable as long as
        // the literal token does not appear.
    }

    #[test]
    fn redacts_https_basic_auth_oauth2() {
        let s =
            "fetch from https://oauth2:glpat-abcdef123456789012345@gitlab.com/team/proj.git failed";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:"), "got: {out}");
        assert!(!out.contains("glpat-abc"));
    }

    #[test]
    fn redacts_https_basic_auth_generic_user() {
        let s = "https://alice:supersecret123456@bitbucket.org/team/proj.git";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:https_basic_auth]"), "got: {out}");
        assert!(!out.contains("supersecret"));
    }

    #[test]
    fn redacts_pem_private_key() {
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nMIIBzAIBAAJBALy7nWuxKabcdefghijklm\n-----END OPENSSH PRIVATE KEY-----";
        let s = format!("ssh key: {key}");
        let out = scrub(&s);
        assert!(out.contains("[REDACTED:ssh_key]"), "got: {out}");
        assert!(!out.contains("MIIBzAIB"));
    }

    #[test]
    fn redacts_multiple_tokens_in_one_string() {
        let s =
            "primary gho_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890 fallback glpat-AbCdEfGhIjKlMnOpQrSt";
        let out = scrub(s);
        assert!(out.contains("[REDACTED:github_token]"));
        assert!(out.contains("[REDACTED:gitlab_token]"));
        assert!(!out.contains("AbCdEf"));
    }

    #[test]
    fn redacts_in_middle_of_sentence() {
        let s = "before gho_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890 after";
        let out = scrub(s);
        assert_eq!(out, "before [REDACTED:github_token] after");
    }

    #[test]
    fn does_not_match_short_gh_prefix() {
        // gho_ followed by fewer than 20 chars must NOT match — too
        // short to be a real token, likely a literal in docs.
        let s = "describe gho_ in docs";
        assert_eq!(scrub(s), s);
    }

    #[test]
    fn does_not_match_glpat_prefix_only() {
        let s = "the glpat- prefix is for GitLab";
        assert_eq!(scrub(s), s);
    }

    #[test]
    fn scrub_opt_passes_none_through() {
        assert_eq!(scrub_opt(None), None);
    }

    #[test]
    fn scrub_opt_processes_some() {
        let out = scrub_opt(Some("gho_AbCdEfGhIjKlMnOpQrStUvWxYz1234567890".to_string()));
        assert_eq!(out, Some("[REDACTED:github_token]".to_string()));
    }
}
