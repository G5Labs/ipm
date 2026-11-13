//! Constant-time comparison utilities for token validation.
//!
//! Prevents timing side-channel attacks when comparing secrets.

/// Compares two byte slices in constant time using XOR accumulation.
///
/// Returns `true` if the slices are equal, `false` otherwise.
/// The comparison time depends only on the length of the slices,
/// not on the content — preventing timing side-channel attacks.
///
/// # Panics
///
/// Does not panic. Returns `false` if lengths differ.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq_equal() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(
            b"abcdef0123456789abcdef0123456789",
            b"abcdef0123456789abcdef0123456789"
        ));
    }

    #[test]
    fn test_constant_time_eq_not_equal() {
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hellp"));
        assert!(!constant_time_eq(b"aaa", b"aab"));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"a", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
    }
}
