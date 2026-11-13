// g5d test helpers — shared test infrastructure for all g5d integration tests.
// This module contains ZERO #[test] functions — it is purely a library consumed by sibling test files.
//
// Each integration test binary (`agent_loop`, `tools_dispatch`, etc.) compiles
// this module independently but only uses a subset of the helpers. Suppress
// dead-code and unused-import warnings for the unused portion in each binary.
#![allow(dead_code, unused_imports, unused_variables)]

pub mod mock_api;

pub use mock_api::MockApiServer;

use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// Re-export commonly used types from the crate for test convenience.
// These use-paths assume g5d's internal modules are accessible from integration tests
// through the retained crate modules.

/// Worker environment harness that creates a temp directory, initializes a git repo,
/// optionally connects to a real or mock API server.
pub struct TestWorkerEnv {
    pub work_dir: TempDir,
    pub api_client: g5_internal_api::InternalApiClient,
}

impl TestWorkerEnv {
    /// Creates a new `TestWorkerEnv` with a fresh temp directory and initialized git repo.
    ///
    /// # Arguments
    /// * `api_url` — If `Some(url)`, uses that URL for the API client.
    ///   If `None`, uses `"http://127.0.0.1:0"` as a placeholder.
    pub fn new(api_url: Option<&str>) -> Self {
        let work_dir = tempfile::tempdir().expect("Failed to create temp directory");

        // Initialize a git repository in the temp directory.
        let repo = git2::Repository::init(work_dir.path()).expect("Failed to init git repo");

        // Create an initial empty commit so the repo has a valid HEAD.
        let sig =
            git2::Signature::now("test", "test@g5.test").expect("Failed to create git signature");
        let tree_id = repo
            .treebuilder(None)
            .expect("Failed to create treebuilder")
            .write()
            .expect("Failed to write empty tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find empty tree");
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .expect("Failed to create initial commit");

        // Build an InternalApiClient.
        let url = api_url.unwrap_or("http://127.0.0.1:0");
        let api_client = g5_internal_api::InternalApiClient::new(url);

        TestWorkerEnv {
            work_dir,
            api_client,
        }
    }

    /// Opens and returns a `git2::Repository` at the working directory.
    pub fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.work_dir.path()).expect("Failed to open git repo")
    }

    /// Returns the working directory path.
    pub fn work_path(&self) -> &Path {
        self.work_dir.path()
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Initializes a `tracing_subscriber` with `EnvFilter::from_default_env()` for test output.
/// Uses `try_init()` so it's safe to call multiple times (subsequent calls are no-ops).
pub fn setup_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .compact()
        .try_init();
}

/// Polls for the existence of a file at `path` every 50ms until the file exists or the
/// timeout expires.
///
/// Returns `true` if the file was found, `false` if the timeout expired.
pub async fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Reads the file at `path` and asserts it contains the `expected` substring.
/// Panics with a descriptive message if the file doesn't exist or doesn't contain the
/// expected string.
pub fn assert_file_contains(path: &Path, expected: &str) {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    assert!(
        content.contains(expected),
        "File {} does not contain '{}'. Content:\n{}",
        path.display(),
        expected,
        content,
    );
}

/// Opens the git repo at `repo_path` and returns a list of commit messages (most recent first).
/// Walks the commit history from HEAD using `repo.revwalk()`.
pub fn git_log(repo_path: &Path) -> Vec<String> {
    let repo = git2::Repository::open(repo_path).expect("Failed to open git repo");
    let mut revwalk = repo.revwalk().expect("Failed to create revwalk");
    revwalk.push_head().expect("Failed to push HEAD");
    revwalk
        .set_sorting(git2::Sort::TIME)
        .expect("Failed to set sorting");
    revwalk
        .filter_map(|oid| {
            let oid = oid.ok()?;
            let commit = repo.find_commit(oid).ok()?;
            commit.message().ok().map(|m| m.trim().to_string())
        })
        .collect()
}

/// Binds a TCP listener to `127.0.0.1:0`, extracts the assigned port, drops the listener,
/// and returns the port.
///
/// Used when tests need a known port before starting a server.
pub fn random_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind random port")
        .local_addr()
        .expect("Failed to get local addr")
        .port()
}
