//! The one check for a browser return path taken from a request (SEC-M36).

/// Whether `candidate` is a path on this origin and nothing else: exactly one
/// leading `/`, no backslash anywhere (browsers read `/\host` as `//host`), no
/// control characters, at most 2048 bytes. Callers add their own narrower
/// rules (the preview re-auth return also refuses fragments).
#[must_use]
pub fn is_local_return_path(candidate: &str) -> bool {
    candidate.starts_with('/')
        && !candidate.starts_with("//")
        && !candidate.contains('\\')
        && candidate.len() <= 2048
        && !candidate.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_same_origin_paths_are_local() {
        for ok in ["/", "/settings", "/a/b?x=1#frag", "/%5Cevil"] {
            assert!(is_local_return_path(ok), "{ok}");
        }
        for bad in [
            "",
            "settings",
            "//evil.example/",
            "/\\evil.example/path",
            "\\\\evil.example/path",
            "/a\\b",
            "https://evil.example/",
            "/a\r\nSet-Cookie: x=y",
            "/\tevil",
            "/a\0b",
            "/a\u{7f}b",
            "/a\u{85}b",
        ] {
            assert!(!is_local_return_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn local_path_limit_is_inclusive_and_counts_utf8_bytes() {
        assert!(is_local_return_path(&format!("/{}", "a".repeat(2047))));
        assert!(!is_local_return_path(&format!("/{}", "a".repeat(2048))));
        assert!(is_local_return_path(&format!("/{}a", "é".repeat(1023))));
        assert!(!is_local_return_path(&format!("/{}", "é".repeat(1024))));
    }
}
