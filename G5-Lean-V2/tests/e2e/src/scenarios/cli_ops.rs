//! g5d CLI Operations E2E Tests
//!
//! Tests every `g5d` CLI command against the live stack.

use crate::helpers::fixtures::WorkspaceFixture;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// g5d init: creates .g5/ directory structure
#[tokio::test]
async fn test_g5d_init() -> anyhow::Result<()> {
    let dir = TempDir::new()?;

    Command::cargo_bin("g5d")?
        .args(["init", dir.path().to_str().unwrap()])
        .assert()
        .success();

    assert!(dir.path().join(".g5/workspace.yaml").exists());
    assert!(dir.path().join(".g5/workspace-meta").exists());

    Ok(())
}

/// g5d init --json: outputs JSON with workspace_id
#[tokio::test]
async fn test_g5d_init_json_output() -> anyhow::Result<()> {
    let dir = TempDir::new()?;

    let output = Command::cargo_bin("g5d")?
        .args(["init", dir.path().to_str().unwrap(), "--json"])
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let json_slice = extract_last_json_object(&stdout)?;
    let json: serde_json::Value = serde_json::from_str(json_slice)?;
    assert!(json["workspace_id"].is_string());

    Ok(())
}

fn extract_last_json_object(stdout: &str) -> anyhow::Result<&str> {
    let end = stdout
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("g5d init --json produced no JSON object end"))?;
    let bytes = stdout.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for idx in (0..=end).rev() {
        let ch = bytes[idx] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '}' => depth += 1,
            '{' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow::anyhow!("g5d init --json produced unmatched braces"))?;
                if depth == 0 {
                    return Ok(stdout[idx..=end].trim());
                }
            }
            _ => {}
        }
    }

    Err(anyhow::anyhow!(
        "g5d init --json produced no matching JSON object start"
    ))
}

/// g5d add: adds a repo to an initialized workspace
#[tokio::test]
async fn test_g5d_add_repo() -> anyhow::Result<()> {
    let repo_fixture = WorkspaceFixture::with_git_repo()?;
    let dir = TempDir::new()?;
    let repo_url = repo_fixture
        .dir
        .path()
        .join("test-repo.git")
        .to_string_lossy()
        .to_string();

    // Init workspace first
    Command::cargo_bin("g5d")?
        .args(["init", dir.path().to_str().unwrap()])
        .assert()
        .success();

    // Add a repo from a local bare fixture so the E2E suite stays hermetic.
    Command::cargo_bin("g5d")?
        .args(["add", &repo_url])
        .current_dir(dir.path())
        .assert()
        .success();

    // Verify workspace.yaml updated
    let yaml = std::fs::read_to_string(dir.path().join(".g5/workspace.yaml"))?;
    assert!(
        yaml.contains("test-repo"),
        "repo not added to workspace.yaml"
    );

    Ok(())
}

/// g5d doctor: reports system capabilities
#[tokio::test]
async fn test_g5d_doctor() -> anyhow::Result<()> {
    let dir = TempDir::new()?;

    Command::cargo_bin("g5d")?
        .args(["init", dir.path().to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("g5d")?
        .args(["doctor"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("git"));

    Ok(())
}

/// g5d --help: shows help text
#[tokio::test]
async fn test_g5d_help() -> anyhow::Result<()> {
    Command::cargo_bin("g5d")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("g5d"));

    Ok(())
}

/// g5d --version: prints version
#[tokio::test]
async fn test_g5d_version() -> anyhow::Result<()> {
    Command::cargo_bin("g5d")?
        .args(["--version"])
        .assert()
        .success();

    Ok(())
}

/// Invalid CLI command: exits with non-zero code
#[tokio::test]
async fn test_g5d_invalid_command() -> anyhow::Result<()> {
    Command::cargo_bin("g5d")?
        .args(["not-a-command"])
        .assert()
        .failure();

    Ok(())
}
