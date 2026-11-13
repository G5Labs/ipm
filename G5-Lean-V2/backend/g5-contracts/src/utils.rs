//! General-purpose string and byte utilities.
//!
//! These helpers handle edge cases (such as multi-byte UTF-8 boundaries or
//! U+0000) that are easy to get wrong when truncating or storing user-supplied
//! or LLM-generated text.

/// Truncate a UTF-8 string to at most `max_bytes` bytes, returning a valid
/// `&str` that ends on a character boundary.
///
/// If `max_bytes >= s.len()` the entire string is returned unchanged.
///
/// # Examples
///
/// ```
/// use g5_contracts::utils::safe_truncate;
///
/// // ASCII — trivial case
/// assert_eq!(safe_truncate("hello world", 5), "hello");
///
/// // Multi-byte: '€' is 3 bytes (E2 82 AC).  Truncating at byte 4 must
/// // NOT split the second '€', so the result is only the first character.
/// assert_eq!(safe_truncate("€€", 4), "€");
///
/// // Zero budget returns an empty slice.
/// assert_eq!(safe_truncate("abc", 0), "");
///
/// // Budget larger than the string returns the whole string.
/// assert_eq!(safe_truncate("abc", 100), "abc");
/// ```
#[must_use]
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    // `floor_char_boundary` is nightly-only, so we walk backwards manually.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Replace every U+0000 in `text` with U+FFFD, the Unicode replacement
/// character.
///
/// PostgreSQL `text` and `jsonb` cannot hold U+0000, and provider or tool
/// output can carry it (a command that prints a binary file). Replacing keeps
/// the text's shape and marks where the character was; it matches how lossy
/// UTF-8 decoding already renders bytes that are not text.
pub fn replace_nul(text: &mut String) {
    if text.contains('\0') {
        *text = text.replace('\0', "\u{FFFD}");
    }
}

/// [`replace_nul`] applied to every string and object key in `value`.
///
/// A replaced key never displaces another entry: one that collides with a key
/// already present is suffixed `#2`, `#3`, … (as `Secrets::redact_json` does
/// for redacted keys), and keys without U+0000 keep their names.
pub fn replace_nul_in_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => replace_nul(text),
        serde_json::Value::Array(items) => items.iter_mut().for_each(replace_nul_in_json),
        serde_json::Value::Object(map) => {
            if map.keys().any(|key| key.contains('\0')) {
                let (replaced, kept): (Vec<_>, Vec<_>) = std::mem::take(map)
                    .into_iter()
                    .partition(|(key, _)| key.contains('\0'));
                map.extend(kept);
                for (mut key, item) in replaced {
                    replace_nul(&mut key);
                    let base = key.clone();
                    let mut n = 2;
                    while map.contains_key(&key) {
                        key = format!("{base}#{n}");
                        n += 1;
                    }
                    map.insert(key, item);
                }
            }
            map.values_mut().for_each(replace_nul_in_json);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nul_is_replaced_in_every_string_and_key() {
        let mut value = serde_json::json!({
            "content": "\u{7f}ELF\u{2}\0\0",
            "nested": [{"a\0b": "x\0"}, 7, null, "clean"],
        });
        replace_nul_in_json(&mut value);
        assert_eq!(
            value,
            serde_json::json!({
                "content": "\u{7f}ELF\u{2}\u{FFFD}\u{FFFD}",
                "nested": [{"a\u{FFFD}b": "x\u{FFFD}"}, 7, null, "clean"],
            })
        );
        assert!(!serde_json::to_string(&value).unwrap().contains("\\u0000"));
    }

    #[test]
    fn a_replaced_key_never_displaces_another_entry() {
        let mut value = serde_json::json!({
            "a\0": 1,
            "a\u{FFFD}": 2,
            "a\u{FFFD}#2": 3,
            "b\0": 4,
            "b\u{FFFD}": 5,
        });
        replace_nul_in_json(&mut value);
        assert_eq!(
            value,
            serde_json::json!({
                "a\u{FFFD}": 2,
                "a\u{FFFD}#2": 3,
                "a\u{FFFD}#3": 1,
                "b\u{FFFD}": 5,
                "b\u{FFFD}#2": 4,
            })
        );
    }

    #[test]
    fn ascii_truncation() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn exact_length_returns_full_string() {
        assert_eq!(safe_truncate("abc", 3), "abc");
    }

    #[test]
    fn larger_budget_returns_full_string() {
        assert_eq!(safe_truncate("abc", 100), "abc");
    }

    #[test]
    fn zero_budget_returns_empty() {
        assert_eq!(safe_truncate("abc", 0), "");
    }

    #[test]
    fn multibyte_no_split() {
        // '€' = 3 bytes (E2 82 AC)
        let s = "€€"; // 6 bytes total
        assert_eq!(safe_truncate(s, 4), "€"); // can't fit second €
        assert_eq!(safe_truncate(s, 3), "€"); // exactly one €
        assert_eq!(safe_truncate(s, 2), ""); // can't fit any €
        assert_eq!(safe_truncate(s, 6), "€€"); // full string
    }

    #[test]
    fn mixed_ascii_and_multibyte() {
        // 'a' = 1 byte, '日' = 3 bytes
        let s = "a日b";
        assert_eq!(safe_truncate(s, 1), "a");
        assert_eq!(safe_truncate(s, 2), "a"); // can't fit '日'
        assert_eq!(safe_truncate(s, 4), "a日");
        assert_eq!(safe_truncate(s, 5), "a日b");
    }

    #[test]
    fn empty_string() {
        assert_eq!(safe_truncate("", 10), "");
        assert_eq!(safe_truncate("", 0), "");
    }

    #[test]
    fn four_byte_emoji() {
        // '😀' = 4 bytes (F0 9F 98 80)
        let s = "😀x";
        assert_eq!(safe_truncate(s, 3), ""); // can't split emoji
        assert_eq!(safe_truncate(s, 4), "😀");
        assert_eq!(safe_truncate(s, 5), "😀x");
    }
}
