//! Selection of browser credentials must not depend on cookie ordering.

use axum::http::{header, HeaderMap};

/// Accept exactly one nonempty cookie with this name across every header.
/// Unrelated cookie bytes cannot hide a credential or make it ambiguous.
/// Duplicates (even empty or identical) and undecodable credentials are refused.
pub(super) fn credential_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut found = None;
    for header in headers.get_all(header::COOKIE) {
        // Browsers may send non-ASCII app cookies alongside our credential.
        // Scan every pair before decoding so unrelated bytes neither block a
        // valid credential nor conceal a duplicate in the same header.
        for pair in header.as_bytes().split(|byte| *byte == b';') {
            let pair = pair.trim_ascii();
            let Some(separator) = pair.iter().position(|byte| *byte == b'=') else {
                continue;
            };
            let (key, value) = pair.split_at(separator);
            let value = &value[1..];
            if key == name.as_bytes() {
                if found.is_some() || value.is_empty() {
                    return None;
                }
                found = Some(std::str::from_utf8(value).ok()?);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const NAMES: [&str; 8] = [
        "g5_refresh",
        "__Host-g5_refresh",
        "g5_github_app_pkce",
        "__Host-g5_github_app_pkce",
        "__Host-g5_oauth_login",
        "__Host-g5_oauth_scm",
        "preview_refresh",
        "__Host-preview_refresh",
    ];

    fn cookie_headers(values: &[&[u8]], separate_headers: bool) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if separate_headers {
            for value in values {
                headers.append(header::COOKIE, HeaderValue::from_bytes(value).unwrap());
            }
        } else {
            headers.append(
                header::COOKIE,
                HeaderValue::from_bytes(&values.join(b"; ".as_slice())).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn selects_only_the_exact_name_across_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("g5_refresh=legacy; other=x"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-g5_refresh=protected; preview_refresh=custom"),
        );
        assert_eq!(
            credential_cookie(&headers, "__Host-g5_refresh"),
            Some("protected")
        );
        assert_eq!(
            credential_cookie(&headers, "preview_refresh"),
            Some("custom")
        );
        assert_eq!(credential_cookie(&headers, "refresh"), None);
        assert_eq!(credential_cookie(&HeaderMap::new(), "g5_refresh"), None);
    }

    #[test]
    fn unrelated_non_ascii_cookies_do_not_hide_credentials() {
        for name in NAMES {
            for other in [
                b"other=caf\xc3\xa9".as_slice(),
                b"other=\xff",
                b"caf\xc3\xa9=value",
                b"\xff=value",
            ] {
                for unrelated_first in [false, true] {
                    for separate_headers in [false, true] {
                        let credential = format!("{name}=valid");
                        let values = if unrelated_first {
                            [other, credential.as_bytes()]
                        } else {
                            [credential.as_bytes(), other]
                        };
                        let headers = cookie_headers(&values, separate_headers);
                        assert_eq!(credential_cookie(&headers, name), Some("valid"));
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_duplicates_empty_values_and_undecodable_credentials() {
        for name in NAMES {
            for (first, second) in [("a", "b"), ("a", "a"), ("", "a"), ("a", "")] {
                for other in [b"other=x".as_slice(), b"other=caf\xc3\xa9", b"other=\xff"] {
                    for separate_headers in [false, true] {
                        let first =
                            [format!("{name}={first}").as_bytes(), other].join(b"; ".as_slice());
                        let second = format!("{name}={second}");
                        let headers =
                            cookie_headers(&[&first, second.as_bytes()], separate_headers);
                        assert_eq!(credential_cookie(&headers, name), None);
                    }
                }
            }
            let mut headers = HeaderMap::new();
            headers.append(header::COOKIE, format!("{name}=").parse().unwrap());
            assert_eq!(credential_cookie(&headers, name), None);
            let invalid = [format!("{name}=").as_bytes(), b"\xff"].concat();
            assert_eq!(
                credential_cookie(&cookie_headers(&[&invalid], true), name),
                None
            );
            for invalid_first in [false, true] {
                for separate_headers in [false, true] {
                    let valid = format!("{name}=valid");
                    let values = if invalid_first {
                        [invalid.as_slice(), valid.as_bytes()]
                    } else {
                        [valid.as_bytes(), invalid.as_slice()]
                    };
                    assert_eq!(
                        credential_cookie(&cookie_headers(&values, separate_headers), name),
                        None
                    );
                }
            }
        }
    }
}
