//! Branch management operations: create, checkout, resolve, setup.

use crate::GitRepo;
use g5_contracts::G5Error;

/// Checks out an existing branch.
///
/// Delegates to [`GitRepo::checkout_branch`].
pub fn checkout_branch(repo: &GitRepo, branch: &str) -> Result<(), G5Error> {
    repo.checkout_branch(branch)
}

/// Returns the checked-out branch, refusing detached or unborn HEAD.
pub fn current_branch(repo: &GitRepo) -> Result<String, G5Error> {
    repo.current_branch()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GitAuthor, GitRepo};

    fn seed_repo() -> (GitRepo, tempfile::TempDir) {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        repo.add_all().unwrap();
        repo.commit("Initial commit", &author).unwrap();
        (repo, dir)
    }

    #[test]
    fn test_checkout_branch_switches_head() {
        let (repo, _dir) = seed_repo();
        let sha = repo.current_head().unwrap();

        repo.create_branch("other", &sha).unwrap();
        checkout_branch(&repo, "other").unwrap();

        let current = current_branch(&repo).unwrap();
        assert_eq!(current, "other");
    }

    #[test]
    fn test_checkout_branch_nonexistent_errors() {
        let (repo, _dir) = seed_repo();
        let result = checkout_branch(&repo, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_current_branch_returns_name() {
        let (repo, _dir) = seed_repo();
        let name = current_branch(&repo).unwrap();
        assert!(!name.is_empty());
    }
    #[test]
    fn current_branch_refuses_detached_and_unborn_head() {
        let dir = tempfile::tempdir().unwrap();
        let empty = crate::workspace::init(dir.path(), false).unwrap();
        assert!(current_branch(&empty).is_err());
        let (repo, dir) = seed_repo();
        let raw = git2::Repository::open(dir.path()).unwrap();
        raw.set_head_detached(raw.head().unwrap().target().unwrap())
            .unwrap();
        assert!(current_branch(&repo).is_err());
    }
}
