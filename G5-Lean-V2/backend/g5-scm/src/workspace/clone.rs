//! Repository initialization. Clones contact a remote and use the native runner.

use std::path::Path;

use crate::GitRepo;
use g5_contracts::G5Error;

/// Initializes a new git repository at the given path.
///
/// If `bare` is `true`, creates a bare repository (no working tree).
pub fn init(path: &Path, bare: bool) -> Result<GitRepo, G5Error> {
    GitRepo::init(path, bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_creates_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init(dir.path(), false).unwrap();
        assert!(repo.workdir().is_some());
    }

    #[test]
    fn test_init_bare_creates_bare_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init(dir.path(), true).unwrap();
        assert!(repo.workdir().is_none());
    }
}
