//! Canonical ref-name validation for user-supplied branch names.
//!
//! Provides [`is_valid_ref_name()`], which wraps
//! `git2::Reference::is_valid_name` for callers that don't take git2
//! directly. Used by the remote-repo-tracking workspace-create path to
//! reject malformed `remote_branch` / `local_branch_name` values (see
//! `workspace_repo_isolation.security_view.md` D14 and arch view D47).
//!
//! The destructive-git-command blocklist that used to live here was retired
//! with the live workspace file layer: agents run plain git natively (design v6 §11.2).

use g5_contracts::G5Error;

/// Parse Git's object-ID representation and test its null sentinel. This checks
/// value syntax only, not whether an object exists or has a particular type.
pub fn object_id_is_zero(value: &str) -> Result<bool, G5Error> {
    Ok(git2::Oid::from_str(value)
        .map_err(crate::backend_error)?
        .is_zero())
}

/// Hash bytes using Git blob framing without opening or writing a repository.
pub fn hash_blob(bytes: &[u8]) -> Result<String, G5Error> {
    Ok(git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .map_err(crate::backend_error)?
        .to_string())
}

/// Validate a user-supplied branch name as a git ref.
///
/// Internally formats `refs/heads/<name>` and calls
/// `git2::Reference::is_valid_name`, which enforces the canonical git
/// ref-format rules: no embedded `..`, no leading `-`, no trailing
/// `.lock`, no control characters, no consecutive slashes, etc.
///
/// Returns `true` only when the resulting full ref name passes
/// validation.
#[must_use]
pub fn is_valid_ref_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let full_ref = format!("refs/heads/{}", name);
    git2::Reference::is_valid_name(&full_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_values_preserve_git_framing_and_null_sentinel_errors() {
        assert_eq!(
            hash_blob(b"").unwrap(),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            hash_blob(b"test content\n").unwrap(),
            "d670460b4b4aece5915caf5c68d12f560a9fe3e4"
        );
        assert!(object_id_is_zero(&"0".repeat(40)).unwrap());
        assert!(!object_id_is_zero(&"A".repeat(40)).unwrap());
        for invalid in ["", "not-an-oid", &"f".repeat(41)] {
            assert!(matches!(
                object_id_is_zero(invalid),
                Err(G5Error::GitError { .. })
            ));
        }
    }

    #[test]
    fn valid_ref_names_are_accepted() {
        // Keep one legacy customer-repository path: this validator accepts Git
        // syntax and must not reinterpret an existing customer's branch name.
        for name in ["main", "feature/x", "workspace-a", "job/abc123"] {
            assert!(is_valid_ref_name(name), "{name} should be valid");
        }
    }

    #[test]
    fn invalid_ref_names_are_rejected() {
        for name in [
            "",
            "~tilde",
            "a..b",
            "trailing.lock",
            "double//slash",
            "space name",
        ] {
            assert!(!is_valid_ref_name(name), "{name:?} should be invalid");
        }
    }
}
