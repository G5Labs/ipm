//! Test fixtures for E2E tests — temporary directories, git repos, etc.

use crate::helpers::api_client::{OrgResponse, TestApiClient, WorkspaceResponse};

use std::process::Command;
use tempfile::TempDir;
use uuid::Uuid;

/// API-backed org fixture that avoids the shared seeded CI org.
pub struct IsolatedOrgFixture {
    pub api: TestApiClient,
    pub org: OrgResponse,
}

impl IsolatedOrgFixture {
    /// Create a fresh org and switch the client to that org context.
    pub async fn create(api: TestApiClient, prefix: &str) -> anyhow::Result<Self> {
        let org = api.create_org(&format!("{prefix}-org")).await?;
        Ok(Self { api, org })
    }
}

/// API-backed org + workspace fixture for live-stack tests.
pub struct IsolatedApiFixture {
    pub api: TestApiClient,
    pub org: OrgResponse,
    pub workspace: WorkspaceResponse,
}

impl IsolatedApiFixture {
    /// Create a fresh org and workspace with the org docs repo attached.
    pub async fn create(api: TestApiClient, prefix: &str) -> anyhow::Result<Self> {
        let org = api.create_org(&format!("{prefix}-org")).await?;
        let workspace = api
            .create_workspace(org.id, &format!("{prefix}-ws"))
            .await?;
        Ok(Self {
            api,
            org,
            workspace,
        })
    }
}

/// A temporary workspace directory with an optional bare git repo for testing.
pub struct WorkspaceFixture {
    pub dir: TempDir,
    pub workspace_id: uuid::Uuid,
}

impl WorkspaceFixture {
    /// Create a temp directory with a bare git repo and a working clone
    /// containing some initial content. Useful for testing COW clone,
    /// workspace init, and repo-add workflows.
    pub fn with_git_repo() -> anyhow::Result<Self> {
        let dir = TempDir::new()?;

        // Create a bare repo
        Command::new("git")
            .args(["init", "--bare", "test-repo.git"])
            .current_dir(dir.path())
            .output()?;

        // Create a non-bare clone with some content
        Command::new("git")
            .args(["clone", "test-repo.git", "test-repo"])
            .current_dir(dir.path())
            .output()?;

        let repo_path = dir.path().join("test-repo");
        std::fs::create_dir_all(repo_path.join("src"))?;
        std::fs::write(repo_path.join("README.md"), "# Test Repo\n")?;
        std::fs::write(repo_path.join("src/main.rs"), "fn main() {}\n")?;

        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&repo_path)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@g5.dev")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@g5.dev")
            .output()?;
        Command::new("git")
            .args(["push"])
            .current_dir(&repo_path)
            .output()?;

        Ok(Self {
            dir,
            workspace_id: uuid::Uuid::new_v4(),
        })
    }

    /// Create a simple temp directory (no git repo).
    pub fn empty() -> anyhow::Result<Self> {
        Ok(Self {
            dir: TempDir::new()?,
            workspace_id: uuid::Uuid::new_v4(),
        })
    }
}
