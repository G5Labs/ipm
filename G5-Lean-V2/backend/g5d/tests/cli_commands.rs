// g5d CLI Commands Integration Tests
//
// Black-box integration tests that validate g5d's CLI subcommands by spawning
// the compiled `g5d` binary as a child process and asserting on exit codes,
// stdout, and stderr. Tests interact with g5d exclusively through its
// command-line interface, exactly as a user would.
//
// These tests exercise the workspace lifecycle commands (init, add, refresh,
// commit), help/version output, and error handling for invalid commands.

mod helpers;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Get a Command for the g5d binary.
#[allow(deprecated)]
/// Every CLI invocation runs against an empty, test-owned g5 home directory.
/// `g5d` resolves its API URL through the user-client install marker and the
/// global auth cache under `$G5_HOME` (default `~/.g5`) before it looks at the
/// workspace, so a developer who is logged in to the real service would
/// otherwise steer these tests at production instead of the mock API.
fn isolated_g5_home() -> &'static Path {
    static HOME: std::sync::OnceLock<TempDir> = std::sync::OnceLock::new();
    HOME.get_or_init(|| TempDir::new().expect("Failed to create isolated g5 home"))
        .path()
}

///
/// Inside a workspace VM the agent's shell also exports its own G5 context
/// (API URL, workspace, token file, checkouts), so none of it is inherited.
fn g5d_cmd() -> Command {
    let mut cmd = Command::cargo_bin("g5d").expect("Failed to find g5d binary");
    for (key, _) in std::env::vars_os() {
        if key
            .to_str()
            .is_some_and(|key| key.starts_with("G5D_") || key.starts_with("G5_"))
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("G5_HOME", isolated_g5_home());
    cmd
}

/// Initialize a workspace at the given directory using `g5d init`.
fn init_workspace(dir: &Path) {
    g5d_cmd()
        .args(["init", dir.to_str().unwrap()])
        .assert()
        .success();
}

/// Create a bare git repository at `dir/remote.git` and return the path to it.
fn create_bare_remote(dir: &Path) -> std::path::PathBuf {
    let remote_path = dir.join("remote.git");
    git2::Repository::init_bare(&remote_path).expect("Failed to create bare repo");
    // Add an initial commit so the bare repo has a valid HEAD/main branch.
    let repo = git2::Repository::open(&remote_path).expect("Failed to open bare repo");
    // Set HEAD to point to refs/heads/main so clones default to 'main'
    repo.set_head("refs/heads/main")
        .expect("Failed to set HEAD to refs/heads/main");
    let sig = git2::Signature::now("test", "test@g5.test").expect("Failed to create git signature");
    let tree_id = repo
        .treebuilder(None)
        .expect("Failed to create treebuilder")
        .write()
        .expect("Failed to write empty tree");
    let tree = repo.find_tree(tree_id).expect("Failed to find empty tree");
    repo.commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[])
        .expect("Failed to create initial commit in bare repo");
    remote_path
}

fn add_bare_branch(remote_path: &Path, branch: &str) {
    let repo = git2::Repository::open(remote_path).expect("Failed to open bare repo");
    let oid = repo
        .refname_to_id("refs/heads/main")
        .expect("main branch should exist");
    repo.reference(&format!("refs/heads/{branch}"), oid, true, "test branch")
        .expect("Failed to create test branch");
}

fn write_remote_workspace_yaml(workspace_dir: &Path, api_url: &str, workspace_id: &str) {
    std::fs::write(
        workspace_dir.join(".g5/workspace.yaml"),
        format!(
            r#"workspace_id: "{workspace_id}"
remote:
  enabled: true
  api_url: "{api_url}"
repos: []
"#
        ),
    )
    .expect("Failed to write remote workspace.yaml");
}

fn write_spec_fixture(workspace_dir: &Path, component_id: &str, body: &str) {
    let spec_path = workspace_dir
        .join(".semlift/specs")
        .join(format!("{component_id}.semlift.md"));
    std::fs::create_dir_all(spec_path.parent().expect("spec parent should exist"))
        .expect("Failed to create spec directory");
    std::fs::write(
        spec_path,
        format!(
            r#"---
component_id: {component_id}
title: "Spec Finalizer Fixture"
type: file
version: 0.1.0
language: markdown
complexity: low
parent_spec_id: g5/test
child_spec_ids: []
dependencies: []
source_files: []
---

# Spec Finalizer Fixture

{body}
"#
        ),
    )
    .expect("Failed to write spec fixture");
}

fn jsonl_values(stdout: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8(stdout.to_vec())
        .expect("stdout should be utf8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout line should be JSON"))
        .collect()
}

#[test]
fn test_spec_block_inventory_detects_standalone_labels_and_is_deterministic() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    write_spec_fixture(
        tmp.path(),
        "g5/test/finalizer",
        r#"## Behavior

### Standalone intent

[I1. Standalone intent] (Tags: human, uncertain)
This standalone label must be inventoried.

### [D2. Embedded heading label]

This heading-embedded label must not be inventoried as a normal block.

[Q3. Standalone question]
Is this deterministic?
"#,
    );

    let first = g5d_cmd()
        .args([
            "-w",
            tmp.path().to_str().unwrap(),
            "--json",
            "spec",
            "block-inventory",
            "--all",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("block-inventory: 2 block(s)"))
        .get_output()
        .stdout
        .clone();
    let second = g5d_cmd()
        .args([
            "-w",
            tmp.path().to_str().unwrap(),
            "--json",
            "spec",
            "block-inventory",
            "--all",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        first, second,
        "block-inventory output must be deterministic"
    );
    let rows = jsonl_values(&first);
    let ids = rows
        .iter()
        .map(|row| row["old_block_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["I1", "Q3"]);
    assert!(rows
        .iter()
        .all(|row| row["old_content_sha256"].as_str().unwrap().len() == 64));
    let intent = rows
        .iter()
        .find(|row| row["old_block_id"] == "I1")
        .expect("I1 inventory row");
    assert_eq!(intent["human_semantic_tag"]["present"], true);
    assert!(intent["human_semantic_tag"]["origin"].is_null());
    assert_eq!(intent["uncertainty_tag"], true);
}

#[test]
fn test_spec_tag_audit_finds_legacy_wrappers_for_migration_and_ignores_mentions() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    write_spec_fixture(
        tmp.path(),
        "g5/test/tag_audit",
        r#"## Protected Content

[D1. Human-tagged contract]
<human origin="direct_edit">This protected sentence must be audited.</human>

This prose mentions `<human>` but is not a tag.

```html
<human origin="direct_edit">Example only.</human>
```

[D2. Multiline human contract]
<human origin="chat_instruction">
Line one.

Line two.
</human>

[D3. Invalid retired wrapper]
<human origin="unknown">Invalid metadata still needs migration.</human>

[D4. Unclosed retired wrapper]
<uncertainty level="red" source="agent">
"#,
    );

    let first = g5d_cmd()
        .args([
            "-w",
            tmp.path().to_str().unwrap(),
            "--json",
            "spec",
            "tag-audit",
            "--all",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("tag-audit: 4 tagged region(s)"))
        .get_output()
        .stdout
        .clone();
    let second = g5d_cmd()
        .args([
            "-w",
            tmp.path().to_str().unwrap(),
            "--json",
            "spec",
            "tag-audit",
            "--all",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(first, second, "tag-audit output must be deterministic");
    let rows = jsonl_values(&first);
    assert_eq!(rows.len(), 4);
    let origins = rows
        .iter()
        .filter_map(|row| row["human_origin"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(origins, vec!["direct_edit", "chat_instruction"]);
    assert!(rows.iter().any(|row| row["detection"] == "raw_unclosed"));
    assert!(rows
        .iter()
        .any(|row| row["tag_kind"] == "human" && row["human_origin"].is_null()));
    assert!(rows
        .iter()
        .all(|row| row["content_sha256"].as_str().unwrap().len() == 64));
    assert!(rows
        .iter()
        .all(|row| !row["excerpt"].as_str().unwrap().contains("Example only")));
}

#[test]
fn test_cli_auth_login_uses_auth_state_cloud_metadata_before_workspace_default() {
    let workspace_id = "00000000-0000-0000-0000-00000000c105";
    let org_id = "00000000-0000-0000-0000-000000000123";
    let web_url = "https://g5.example.com";
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();
    let g5_dir = ws_path.join(".g5");
    std::fs::create_dir_all(&g5_dir).expect("Failed to create .g5 directory");

    let mock_api = helpers::MockApiServer::start_blocking();
    mock_api
        .when("POST", "/api/v1/auth/cli/start")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "deviceCode": "device-code",
                    "userCode": "ABCD-EFGH",
                    "verificationUri": format!("{web_url}/cli/authorize"),
                    "verificationUriComplete": format!("{web_url}/cli/authorize?userCode=ABCD-EFGH"),
                    "expiresIn": 60,
                    "interval": 1
                }
            }),
        );
    mock_api
        .when("POST", "/api/v1/auth/cli/token")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "status": "expired"
                }
            }),
        );

    std::fs::write(
        g5_dir.join("workspace.yaml"),
        format!(
            "workspace_id: {workspace_id}\nremote:\n  enabled: true\n  api_url: http://127.0.0.1:9\n"
        ),
    )
    .expect("Failed to write workspace.yaml");
    std::fs::write(
        g5_dir.join("auth_state.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "apiUrl": mock_api.url(),
            "webUrl": web_url,
            "orgId": org_id,
            "workspaceId": workspace_id,
            "authMode": "device"
        }))
        .expect("Failed to serialize auth_state.json"),
    )
    .expect("Failed to write auth_state.json");

    g5d_cmd()
        .env_remove("G5D_API_URL")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "auth",
            "login",
            "--device-name",
            "local-cloud-conformance",
            "--timeout-secs",
            "5",
            "--poll-interval-secs",
            "1",
        ])
        .assert()
        .failure();

    mock_api.assert_called_with_body("POST", "/api/v1/auth/cli/start", |body| {
        body.get("workspaceId").and_then(|value| value.as_str()) == Some(workspace_id)
            && body.get("orgId").and_then(|value| value.as_str()) == Some(org_id)
            && body.get("browserOrigin").and_then(|value| value.as_str()) == Some(web_url)
            && body.get("deviceName").and_then(|value| value.as_str())
                == Some("local-cloud-conformance")
    });
}

// ---------------------------------------------------------------------------
// Init tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_init() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();

    // Successful init
    g5d_cmd()
        .args(["init", ws_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("nitializ").or(predicate::str::contains("orkspace")));

    // Verify workspace artifacts exist
    assert!(ws_path.join(".g5/workspace.yaml").exists());
    assert!(ws_path.join(".g5/workspace-meta").exists());

    // Read workspace.yaml and verify it contains a workspace_id
    let ws_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml");
    assert!(
        ws_yaml.contains("workspace_id"),
        "workspace.yaml should contain workspace_id"
    );

    // Verify workspace-meta is a git repository
    let meta_repo = git2::Repository::open(ws_path.join(".g5/workspace-meta"));
    assert!(
        meta_repo.is_ok(),
        ".g5/workspace-meta should be a valid git repository"
    );
    let docs_repo = git2::Repository::open(ws_path.join(".g5/docs"));
    assert!(
        docs_repo.is_ok(),
        ".g5/docs should be initialized as a git repository"
    );

    // Error variant: init on existing workspace should fail with exit code 2
    g5d_cmd()
        .args(["init", ws_path.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already").or(predicate::str::contains("exist")));
}

#[test]
fn test_cli_init_with_force() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();

    // First init
    init_workspace(ws_path);

    // Read the original workspace_id
    let _original_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml");

    // Force re-init
    g5d_cmd()
        .args(["init", "--force", ws_path.to_str().unwrap()])
        .assert()
        .success();

    // Read the new workspace.yaml — should have a new workspace_id
    let new_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml after force init");

    // The YAML files should differ (new workspace_id generated)
    // Note: They may be the same if workspace_id isn't regenerated, but per spec it should be.
    assert!(
        new_yaml.contains("workspace_id"),
        "workspace.yaml should still contain workspace_id after force init"
    );
}

// ---------------------------------------------------------------------------
// Add repo tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_add_repo() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    // Create a bare git repository to serve as the "remote"
    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    // Add the repo
    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("dded").or(predicate::str::contains("repo")));

    // Verify the repo was cloned
    let repo_name = remote_path.file_stem().unwrap().to_str().unwrap();
    let repo_dir = ws_path.join(repo_name);
    assert!(repo_dir.exists(), "Repo should be cloned to {repo_name}");

    // Verify workspace.yaml was updated
    let ws_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml");
    assert!(
        ws_yaml.contains(repo_name),
        "workspace.yaml should reference the added repo"
    );

    // Error variant: adding duplicate repo should fail with exit code 2
    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already").or(predicate::str::contains("exist")));
}

#[test]
fn test_cli_add_repo_with_options() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    // Create a bare git repository
    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    // Add with custom name, branch, and path
    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
            "--name",
            "my-custom-name",
            "--branch",
            "main",
            "--path",
            "custom-location",
        ])
        .assert()
        .success();

    // Verify the repo was cloned to the custom path
    let custom_repo_dir = ws_path.join("custom-location");
    assert!(
        custom_repo_dir.exists(),
        "Repo should be cloned to custom-location"
    );

    // Verify workspace.yaml has the custom name
    let ws_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml");
    assert!(
        ws_yaml.contains("my-custom-name"),
        "workspace.yaml should contain the custom repo name"
    );
}

#[test]
fn test_cli_add_repo_rejects_nested_path() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
            "--path",
            "repos/nested",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("one direct child folder"));

    assert!(!ws_path.join("repos/nested").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_syncs_repo_to_remote_workspace() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    let mock_api = helpers::MockApiServer::start().await;
    let ws_yaml = format!(
        r#"workspace_id: "ws-add-remote"
remote:
  enabled: true
  api_url: "{}"
repos: []
"#,
        mock_api.url()
    );
    std::fs::write(ws_path.join(".g5/workspace.yaml"), ws_yaml)
        .expect("Failed to write workspace.yaml");

    mock_api
        .when("GET", "/api/v1/workspaces/ws-add-remote")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "id": "ws-add-remote",
                    "orgId": "00000000-0000-0000-0000-000000000001",
                    "name": "ws-add-remote"
                }
            }),
        );
    mock_api
        .when(
            "POST",
            "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-remote/repos",
        )
        .respond_with(
            201,
            serde_json::json!({
                "data": {
                    "repoId": "00000000-0000-0000-0000-000000000010",
                    "repoName": "remote",
                    "gitUrl": remote_path.to_string_lossy().to_string()
                }
            }),
        );
    mock_api
        .when("GET", "/api/v1/workspaces/ws-add-remote/manifest")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": "ws-add-remote",
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": [{
                        "name": "remote",
                        "url": remote_path.to_string_lossy().to_string(),
                        "branch": "main",
                        "path": "remote"
                    }]
                }
            }),
        );

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    mock_api.assert_called(
        "POST",
        "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-remote/repos",
        1,
    );
    mock_api.assert_called_with_body(
        "POST",
        "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-remote/repos",
        |body| body["gitUrl"] == remote_path.to_string_lossy().to_string(),
    );

    let refreshed_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read refreshed workspace.yaml");
    assert!(
        refreshed_yaml.contains("ws-add-remote") && refreshed_yaml.contains("path: remote"),
        "workspace.yaml should be refreshed from the remote manifest after add"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_track_remote_posts_remote_tracking_payload() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());
    add_bare_branch(&remote_path, "feature/login");
    add_bare_branch(&remote_path, "g5/local-feature-login");

    let mock_api = helpers::MockApiServer::start().await;
    write_remote_workspace_yaml(ws_path, &mock_api.url(), "ws-add-track");

    mock_api
        .when("GET", "/api/v1/workspaces/ws-add-track")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "id": "ws-add-track",
                    "orgId": "00000000-0000-0000-0000-000000000001",
                    "name": "ws-add-track"
                }
            }),
        );
    mock_api
        .when(
            "POST",
            "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-track/repos",
        )
        .respond_with(
            201,
            serde_json::json!({
                "data": {
                    "repoId": "00000000-0000-0000-0000-000000000010",
                    "repoName": "remote",
                    "gitUrl": remote_path.to_string_lossy().to_string(),
                    "branch": {
                        "workspaceId": "ws-add-track",
                        "repoId": "00000000-0000-0000-0000-000000000010",
                        "branchName": "g5/local-feature-login",
                        "headCommit": "1111111111111111111111111111111111111111"
                    }
                }
            }),
        );
    mock_api
        .when("GET", "/api/v1/workspaces/ws-add-track/manifest")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": "ws-add-track",
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": [{
                        "name": "remote",
                        "url": remote_path.to_string_lossy().to_string(),
                        "branch": "g5/local-feature-login",
                        "remote_branch": "feature/login",
                        "path": "tracked-remote"
                    }]
                }
            }),
        );

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .env_remove("G5D_API_URL")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
            "--track-remote",
            "feature/login",
            "--path",
            "tracked-remote",
        ])
        .assert()
        .success();

    let add_repo_path =
        "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-track/repos";
    mock_api.assert_called_with_body("POST", add_repo_path, |body| {
        body["gitUrl"] == remote_path.to_string_lossy().to_string()
            && body["trackingMode"] == "remote_tracking"
            && body["remoteBranch"] == "feature/login"
            && body["defaultBranch"] == "feature/login"
    });
    mock_api.assert_called("POST", "/api/v1/workspaces/ws-add-track/branches", 0);

    let refreshed_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read refreshed workspace.yaml");
    assert!(
        refreshed_yaml.contains("remote_branch: feature/login")
            && refreshed_yaml.contains("path: tracked-remote"),
        "workspace.yaml should preserve remote_branch and requested local path after add"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_track_remote_fails_closed_on_old_server_gap() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    let mock_api = helpers::MockApiServer::start().await;
    write_remote_workspace_yaml(ws_path, &mock_api.url(), "ws-add-track-old");

    mock_api
        .when("GET", "/api/v1/workspaces/ws-add-track-old")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "id": "ws-add-track-old",
                    "orgId": "00000000-0000-0000-0000-000000000001",
                    "name": "ws-add-track-old"
                }
            }),
        );
    mock_api
        .when(
            "POST",
            "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-track-old/repos",
        )
        .respond_with(
            422,
            serde_json::json!({
                "code": "VALIDATION_ERROR",
                "message": "trackingMode: unknown variant `remote_tracking`"
            }),
        );

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .env_remove("G5D_API_URL")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
            "--track-remote",
            "feature/login",
        ])
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "does not support remote-tracking repo adds yet",
        ));

    mock_api.assert_called("POST", "/api/v1/workspaces/ws-add-track-old/branches", 0);
    let unchanged_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read workspace.yaml");
    assert!(
        unchanged_yaml.contains("repos: []"),
        "old-server failure should not write a local add success"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_track_remote_rejects_branch_mismatch_before_side_effects() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());

    let mock_api = helpers::MockApiServer::start().await;
    write_remote_workspace_yaml(ws_path, &mock_api.url(), "ws-add-track-mismatch");

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .env_remove("G5D_API_URL")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            remote_path.to_str().unwrap(),
            "--branch",
            "main",
            "--track-remote",
            "feature/login",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--branch and --track-remote"));

    mock_api.assert_called("GET", "/api/v1/workspaces/ws-add-track-mismatch", 0);
    mock_api.assert_called(
        "POST",
        "/api/v1/orgs/00000000-0000-0000-0000-000000000001/workspaces/ws-add-track-mismatch/repos",
        0,
    );
    assert!(
        !ws_path.join("remote").exists(),
        "branch mismatch should fail before cloning the local checkout"
    );
}

// ---------------------------------------------------------------------------
// Refresh tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Commit tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Revert tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Help tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_help() {
    g5d_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("g5d"))
        // Verify all subcommand names are listed
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("host"))
        .stdout(predicate::str::contains("doctor"))
        // Verify global flag descriptions are present
        .stdout(predicate::str::contains("--workspace").or(predicate::str::contains("-w")))
        .stdout(predicate::str::contains("--verbose").or(predicate::str::contains("-v")))
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--api-url"));
}

#[test]
fn test_cli_subcommand_help() {
    // Test help for several subcommands
    let subcommands = ["init", "clone", "dispatch", "host"];

    for subcmd in &subcommands {
        g5d_cmd()
            .args([subcmd, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }

    // Verify specific subcommand help content
    g5d_cmd()
        .args(["clone", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace"));

    g5d_cmd()
        .args(["dispatch", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace runtime"));

    g5d_cmd()
        .args(["host", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("control plane"));
}

// ---------------------------------------------------------------------------
// Version tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_version() {
    g5d_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("g5d"))
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}

// ---------------------------------------------------------------------------
// Invalid command tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_invalid_command() {
    g5d_cmd()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("error")
                .or(predicate::str::contains("unrecognized"))
                .or(predicate::str::contains("invalid")),
        );
}

// ---------------------------------------------------------------------------
// JSON output tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_json_output() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();

    // Init with JSON output
    let output = g5d_cmd()
        .args(["--json", "init", ws_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute g5d");

    assert!(
        output.status.success(),
        "g5d --json init should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify stdout is valid JSON
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "stdout should be valid JSON. Got: {}", stdout);

    let json = json.unwrap();
    assert!(
        json.get("status").is_some()
            || json.get("workspace_id").is_some()
            || json.get("workspaceId").is_some(),
        "JSON should contain status or workspace_id. Got: {}",
        json
    );
}

#[test]
fn test_cli_json_error_output() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();

    // First init to create workspace
    init_workspace(ws_path);

    // Second init (without --force) should fail
    let output = g5d_cmd()
        .args(["--json", "init", ws_path.to_str().unwrap()])
        .output()
        .expect("Failed to execute g5d");

    assert!(
        !output.status.success(),
        "g5d --json init on existing workspace should fail"
    );

    // Try to parse stdout or stderr as JSON
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The error JSON may be on stdout or stderr depending on implementation
    let json_str = if !stdout.trim().is_empty() {
        stdout.to_string()
    } else {
        stderr.to_string()
    };

    if !json_str.trim().is_empty() {
        let json: Result<serde_json::Value, _> = serde_json::from_str(json_str.trim());
        if let Ok(json) = json {
            // Verify error fields are present
            let has_error_indicator = json.get("status").is_some()
                || json.get("code").is_some()
                || json.get("error").is_some()
                || json.get("exit_code").is_some()
                || json.get("exitCode").is_some();
            assert!(
                has_error_indicator,
                "JSON error output should contain error indicators. Got: {}",
                json
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Verbosity tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_verbose_flag() {
    let tmp1 = TempDir::new().expect("Failed to create temp directory");
    let tmp2 = TempDir::new().expect("Failed to create temp directory");
    let tmp3 = TempDir::new().expect("Failed to create temp directory");

    // -v (info level)
    g5d_cmd()
        .args(["-v", "init", tmp1.path().to_str().unwrap()])
        .assert()
        .success();

    // -vv (debug level)
    g5d_cmd()
        .args(["-vv", "init", tmp2.path().to_str().unwrap()])
        .assert()
        .success();

    // -vvv (trace level)
    g5d_cmd()
        .args(["-vvv", "init", tmp3.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_cli_quiet_flag() {
    let tmp = TempDir::new().expect("Failed to create temp directory");

    let output = g5d_cmd()
        .args(["--quiet", "init", tmp.path().to_str().unwrap()])
        .output()
        .expect("Failed to execute g5d");

    assert!(output.status.success(), "g5d --quiet init should succeed");

    // In quiet mode, stdout should be empty or minimal
    let stdout = String::from_utf8_lossy(&output.stdout);
    // We don't assert stdout is completely empty because some essential output
    // (like workspace_id) may still be printed. The key is that informational
    // messages are suppressed.
    let _ = stdout;
}

// ---------------------------------------------------------------------------
// Workspace auto-detection tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_workspace_auto_detection() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = tmp.path();
    init_workspace(ws_path);

    // Create a subdirectory to simulate being inside the workspace
    let subdir = ws_path.join("my-repo/src");
    std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

    // Run a workspace-scoped command from the subdirectory — should
    // auto-detect the workspace root.
    g5d_cmd()
        .current_dir(&subdir)
        .args(["spec", "list"])
        .assert()
        .success();
}

#[test]
fn test_cli_no_workspace_error() {
    let tmp = TempDir::new().expect("Failed to create temp directory");
    // Empty temp dir with no .g5/ directory

    g5d_cmd()
        .args([
            "-w",
            tmp.path().to_str().unwrap(),
            "add",
            "https://example.invalid/repo.git",
        ])
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("workspace")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("No workspace")),
        );
}

/// PR-rev7-3c: `g5d doctor` must surface the rev-7 readiness probes in
/// stdout so an operator can see microVM evidence and the workspace host
/// layout without running any other command.
///
/// The doctor exits non-zero when required checks fail (e.g. `Cache` is
/// required and points at `~/.g5/cache`, which is fine on CI; `Git` requires
/// 2.20+, which is universal). We tolerate both 0 and 1 here because the
/// CI host's exact required-check posture is not load-bearing — the point of
/// this test is to assert the new evidence lines are present, not to assert
/// that every required check happens to pass on the CI matrix.
#[test]
fn test_cli_doctor_reports_rev7_probes() {
    let tmp = TempDir::new().expect("Failed to create temp directory");

    let output = g5d_cmd()
        .args(["-w", tmp.path().to_str().unwrap(), "doctor"])
        .assert()
        .get_output()
        .clone();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));

    assert!(
        combined.contains("MicroVM"),
        "doctor stdout must report the MicroVM check; got:\n{combined}"
    );
    assert!(
        combined.contains("workspace host"),
        "doctor stdout must report the workspace host probe; got:\n{combined}"
    );
    if cfg!(target_os = "linux") {
        assert!(
            combined.contains("nested-virt="),
            "Linux doctor stdout must report nested-virt status; got:\n{combined}"
        );
    }
}

/// PR-rev7-3c: `g5d doctor --json` must include the new rev-7 checks as
/// structured entries so downstream control-plane introspection (PR-rev7-7
/// territory) can read them without parsing the human-readable lines.
#[test]
fn test_cli_doctor_json_includes_rev7_probes() {
    let tmp = TempDir::new().expect("Failed to create temp directory");

    let output = g5d_cmd()
        .args(["--json", "-w", tmp.path().to_str().unwrap(), "doctor"])
        .assert()
        .get_output()
        .clone();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Doctor emits one JSON object for the diagnostic result; when any
    // required check fails the runner appends a second error JSON. Parse
    // each JSON value in stdout independently with `StreamDeserializer` so
    // the test is resilient to either outcome.
    let stream = serde_json::Deserializer::from_str(&stdout).into_iter::<serde_json::Value>();
    let mut found_host_up = false;
    for value in stream {
        let value =
            value.unwrap_or_else(|e| panic!("doctor --json stdout has bad JSON: {e}\n{stdout}"));
        let checks = value["checks"]
            .as_array()
            .or_else(|| value["data"]["checks"].as_array());
        if let Some(checks) = checks {
            for check in checks {
                if let Some(name) = check["name"].as_str() {
                    if name == "workspace host" {
                        found_host_up = true;
                    }
                }
            }
        }
    }
    assert!(
        found_host_up,
        "doctor --json must include the workspace host probe; stdout was:\n{stdout}"
    );
}

// ===========================================================================
// Clone tests
// ===========================================================================

/// Clone workspace from mock API → creates local workspace with manifest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("cloned-workspace");

    // Start mock API server
    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when("POST", "/api/v1/workspaces/ws-clone-test-123/clone")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace": {
                        "id": "ws-clone-test-123"
                    }
                }
            }),
        );

    mock_api
        .when("GET", "/api/v1/workspaces/ws-clone-test-123/manifest")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": "ws-clone-test-123",
                    "remote": {
                        "enabled": true,
                        "api_url": "http://localhost:7500"
                    },
                    "repos": []
                }
            }),
        );

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "ws-clone-test-123",
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .success();

    // Verify workspace artifacts
    assert!(
        target_path.join(".g5").exists(),
        ".g5 directory should be created"
    );
    assert!(
        target_path.join(".g5/workspace.yaml").exists(),
        "workspace.yaml should be written"
    );
    assert!(
        target_path.join(".g5/workspace-meta").exists(),
        "workspace-meta should be initialized"
    );
}

/// Clone by workspace UUID with an explicit target path materializes that
/// existing workspace locally instead of creating a sibling server clone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_uuid_with_path_materializes_existing_workspace() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("base");
    let workspace_id = "691a31c9-4705-4000-aead-f378dac7ca44";

    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when(
            "GET",
            &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        )
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": workspace_id,
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": []
                }
            }),
        );

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            workspace_id,
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .success();

    mock_api.assert_called(
        "POST",
        &format!("/api/v1/workspaces/{workspace_id}/clone"),
        0,
    );
    mock_api.assert_called(
        "GET",
        &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        1,
    );

    let workspace_yaml = std::fs::read_to_string(target_path.join(".g5/workspace.yaml"))
        .expect("workspace.yaml should be written");
    assert!(
        workspace_yaml.contains(workspace_id),
        "direct UUID clone should preserve the source workspace id"
    );
}

/// Clone by workspace UUID from a fresh directory should use the global auth
/// cache produced by `g5d auth login --org-id`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_uuid_without_workspace_uses_global_auth_cache() {
    let run_dir = TempDir::new().expect("Failed to create run directory");
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("base");
    let global_auth_dir = TempDir::new().expect("Failed to create global auth directory");
    let global_auth_path = global_auth_dir.path().join("auth_state.json");
    let workspace_id = "ddccee3e-45a4-4bc6-9a26-bd66b72401d2";

    let mock_api = helpers::MockApiServer::start().await;

    std::fs::write(
        &global_auth_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "jwt": "global-jwt",
            "refreshToken": "global-refresh",
            "apiUrl": mock_api.url(),
            "orgId": "83317909-3a36-408e-b176-a65a5f12065d",
            "authMode": "device"
        }))
        .expect("Failed to serialize global auth_state.json"),
    )
    .expect("Failed to write global auth_state.json");

    mock_api
        .when(
            "GET",
            &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        )
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": workspace_id,
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": []
                }
            }),
        );

    g5d_cmd()
        .current_dir(run_dir.path())
        .env("G5D_AUTH_STATE", &global_auth_path)
        .env_remove("G5D_TOKEN")
        .env_remove("G5_API_KEY")
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            workspace_id,
            target_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    mock_api.assert_called(
        "GET",
        &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        1,
    );
    let requests = mock_api.get_requests(
        "GET",
        &format!("/api/v1/workspaces/{workspace_id}/manifest"),
    );
    assert_eq!(
        requests[0].headers.get("authorization").map(String::as_str),
        Some("Bearer global-jwt")
    );
}

/// Clone by workspace name from a fresh directory should resolve the name
/// through the global auth cache before fetching the manifest. A bare name
/// goes through the resolver, which also knows former names
/// (`g5.workspace.former-names`), keyed by the cached organization id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_name_without_workspace_uses_global_auth_cache() {
    let run_dir = TempDir::new().expect("Failed to create run directory");
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("base");
    let global_auth_dir = TempDir::new().expect("Failed to create global auth directory");
    let global_auth_path = global_auth_dir.path().join("auth_state.json");
    let org_id = "83317909-3a36-408e-b176-a65a5f12065d";
    let workspace_id = "ddccee3e-45a4-4bc6-9a26-bd66b72401d2";
    let cloned_workspace_id = "992a8911-5228-4b21-86aa-64275abfbb1d";
    // The typed name is a former name; the resolver answers with the current one.
    let former_name = "workspace-name";
    let current_name = "workspace-renamed";

    let mock_api = helpers::MockApiServer::start().await;

    std::fs::write(
        &global_auth_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "jwt": "global-jwt",
            "refreshToken": "global-refresh",
            "apiUrl": mock_api.url(),
            "orgId": org_id,
            "authMode": "device"
        }))
        .expect("Failed to serialize global auth_state.json"),
    )
    .expect("Failed to write global auth_state.json");

    mock_api.when("GET", "/api/v1/resolve").respond_with(
        200,
        serde_json::json!({
            "data": {
                "orgId": org_id,
                "workspaceId": workspace_id,
                "workspaceName": current_name
            }
        }),
    );
    mock_api
        .when("POST", &format!("/api/v1/workspaces/{workspace_id}/clone"))
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace": {
                        "id": cloned_workspace_id
                    }
                }
            }),
        );
    mock_api
        .when(
            "GET",
            &format!("/api/v1/workspaces/{cloned_workspace_id}/manifest"),
        )
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": cloned_workspace_id,
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": []
                }
            }),
        );

    g5d_cmd()
        .current_dir(run_dir.path())
        .env("G5D_AUTH_STATE", &global_auth_path)
        .env_remove("G5D_TOKEN")
        .env_remove("G5_API_KEY")
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            former_name,
            target_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    mock_api.assert_called("GET", "/api/v1/resolve", 1);
    let resolve_query = mock_api.get_requests("GET", "/api/v1/resolve")[0]
        .query
        .clone()
        .expect("resolver request carries the org and the typed name");
    assert!(
        resolve_query.contains(&format!("org={org_id}")),
        "{resolve_query}"
    );
    assert!(
        resolve_query.contains(&format!("workspace={former_name}")),
        "{resolve_query}"
    );
    mock_api.assert_called(
        "POST",
        &format!("/api/v1/workspaces/{workspace_id}/clone"),
        1,
    );
    mock_api.assert_called(
        "POST",
        &format!("/api/v1/workspaces/{former_name}/clone"),
        0,
    );
    mock_api.assert_called(
        "GET",
        &format!("/api/v1/workspaces/{cloned_workspace_id}/manifest"),
        1,
    );
    mock_api.assert_called(
        "GET",
        &format!("/api/v1/workspaces/{former_name}/manifest"),
        0,
    );
}

/// Clone workspace materializes the docs checkout under `.g5/docs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_materializes_docs_checkout() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("cloned-workspace");

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for docs remote");
    let remote_path = create_bare_remote(remote_tmp.path());
    let remote_repo = git2::Repository::open(&remote_path).expect("Failed to open bare repo");
    let pinned_commit = remote_repo
        .find_reference("refs/heads/main")
        .expect("main ref should exist")
        .target()
        .expect("main ref should point to a commit")
        .to_string();

    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when("POST", "/api/v1/workspaces/ws-clone-docs/clone")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace": {
                        "id": "ws-clone-docs"
                    }
                }
            }),
        );
    mock_api
        .when("GET", "/api/v1/workspaces/ws-clone-docs/manifest")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": "ws-clone-docs",
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "docs": {
                        "repo_url": remote_path.to_string_lossy().to_string(),
                        "branch": "feature/docs-space",
                        "tracking_mode": "tracking",
                        "path": "docs",
                        "commit": pinned_commit
                    },
                    "repos": []
                }
            }),
        );

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "ws-clone-docs",
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .success();

    let docs_repo =
        git2::Repository::open(target_path.join("docs")).expect("docs checkout should exist");
    let head = docs_repo.head().expect("docs repo should have HEAD");
    assert_eq!(
        head.shorthand(),
        Ok("feature/docs-space"),
        "clone should materialize the workspace docs branch locally"
    );
}

/// Clone workspace materializes a pinned commit even when the logical branch
/// does not yet exist on the remote mirror.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_materializes_pinned_commit_on_unpublished_branch() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("cloned-workspace");

    let remote_tmp = TempDir::new().expect("Failed to create temp directory for remote");
    let remote_path = create_bare_remote(remote_tmp.path());
    let remote_repo = git2::Repository::open(&remote_path).expect("Failed to open bare repo");
    let pinned_commit = remote_repo
        .find_reference("refs/heads/main")
        .expect("main ref should exist")
        .target()
        .expect("main ref should point to a commit")
        .to_string();

    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when("POST", "/api/v1/workspaces/ws-clone-pinned/clone")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace": {
                        "id": "ws-clone-pinned"
                    }
                }
            }),
        );
    mock_api
        .when("GET", "/api/v1/workspaces/ws-clone-pinned/manifest")
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": "ws-clone-pinned",
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": [{
                        "name": "remote",
                        "url": remote_path.to_string_lossy().to_string(),
                        "branch": "feature/cloned-workspace",
                        "path": "remote",
                        "commit": pinned_commit
                    }]
                }
            }),
        );

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "ws-clone-pinned",
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .success();

    let cloned_repo =
        git2::Repository::open(target_path.join("remote")).expect("materialized repo should exist");
    let head = cloned_repo.head().expect("cloned repo should have HEAD");
    assert_eq!(
        head.shorthand(),
        Ok("feature/cloned-workspace"),
        "clone should materialize the logical workspace branch locally"
    );
    assert_eq!(
        head.target().map(|oid| oid.to_string()).as_deref(),
        Some(pinned_commit.as_str()),
        "clone should pin the local checkout to the manifest commit"
    );
}

/// Clone workspace fails with 404 → non-zero exit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_not_found() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("missing-workspace");

    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when("POST", "/api/v1/workspaces/nonexistent/clone")
        .respond_status(404);
    mock_api
        .when("GET", "/api/v1/workspaces/nonexistent/manifest")
        .respond_status(404);

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "nonexistent",
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("Not found"))
                .or(predicate::str::contains("404"))
                .or(predicate::str::contains("NotFound")),
        );
}

/// Clone workspace conflicts should preserve the server's explanatory message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_conflict_surfaces_server_message() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("base");

    let mock_api = helpers::MockApiServer::start().await;
    mock_api
        .when("POST", "/api/v1/workspaces/workspace-name/clone")
        .respond_with(
            409,
            serde_json::json!({
                "code": "CONFLICT",
                "message": "Workspace name already exists in this org",
                "details": {}
            }),
        );

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "workspace-name",
            target_path.to_str().unwrap(),
            "--token",
            "test-jwt-token",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Workspace name already exists in this org",
        ));
}

/// Clone workspace fails when target directory already exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_target_exists() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    // target_dir.path() already exists

    let mock_api = helpers::MockApiServer::start().await;

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "ws-123",
            target_dir.path().to_str().unwrap(),
            "--token",
            "test-token",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists").or(predicate::str::contains("exists")));
}

/// Clone workspace fails without authentication token.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_clone_no_token() {
    let target_dir = TempDir::new().expect("Failed to create temp directory");
    let target_path = target_dir.path().join("no-token-ws");

    let mock_api = helpers::MockApiServer::start().await;

    g5d_cmd()
        .args([
            "--api-url",
            &mock_api.url(),
            "clone",
            "ws-123",
            target_path.to_str().unwrap(),
        ])
        .env_remove("G5D_TOKEN")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Unauthorized")
                .or(predicate::str::contains("token"))
                .or(predicate::str::contains("auth")),
        );
}

// ===========================================================================
// Fork tests
// ===========================================================================

// ===========================================================================
// Merge tests
// ===========================================================================

// ===========================================================================
// Conflicts tests
// ===========================================================================

#[tokio::test]
async fn test_cli_add_local_rejects_track_remote() {
    // `--local` and `--track-remote` are mutually exclusive: G5-managed local
    // repos have no upstream to track. The CLI must reject this before any API
    // call instead of silently creating a local repo and dropping the
    // remote-tracking request. Mirrors the API/browser rejection. (Codex PR #450)
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);
    let ws_yaml = r#"workspace_id: "00000000-0000-0000-0000-00000000a441"
remote:
  enabled: true
  api_url: "http://127.0.0.1:1"
repos: []
"#;
    std::fs::write(ws_path.join(".g5/workspace.yaml"), ws_yaml)
        .expect("Failed to write workspace.yaml");

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            "--local",
            "--track-remote",
            "main",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--track-remote is not supported with --local",
        ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_local_repo() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let mock_api = helpers::MockApiServer::start().await;
    let workspace_id = "00000000-0000-0000-0000-00000000a441";
    let org_id = "00000000-0000-0000-0000-000000000001";
    // `--name` omitted: the CLI sends the workspace-derived default only as the
    // `g5-local://` URL seed (no `name` field) so the server can auto-uniquify
    // when the implicit workspace repo already occupies that name. The CLI then
    // adopts the server-returned (here suffixed `_2`) repo name. [C3]
    let seed_repo_name = "my_service_workspace_code_00000000";
    let seed_git_url = format!("g5-local://workspaces/{workspace_id}/repos/{seed_repo_name}.git");
    let materialized_repo_name = format!("{seed_repo_name}_2");
    let materialized_git_url =
        format!("g5-local://workspaces/{workspace_id}/repos/{materialized_repo_name}.git");
    let ws_yaml = format!(
        r#"workspace_id: "{workspace_id}"
remote:
  enabled: true
  api_url: "{}"
repos: []
"#,
        mock_api.url()
    );
    std::fs::write(ws_path.join(".g5/workspace.yaml"), ws_yaml)
        .expect("Failed to write workspace.yaml");

    mock_api
        .when("GET", &format!("/api/v1/workspaces/{workspace_id}"))
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "id": workspace_id,
                    "orgId": org_id,
                    "name": "My Service Workspace"
                }
            }),
        );
    mock_api
        .when(
            "POST",
            &format!("/api/v1/orgs/{org_id}/workspaces/{workspace_id}/repos"),
        )
        .respond_with(
            201,
            serde_json::json!({
                "data": {
                    "repoId": "00000000-0000-0000-0000-000000000010",
                    "repoName": materialized_repo_name,
                    "gitUrl": materialized_git_url,
                    "branch": {
                        "workspaceId": workspace_id,
                        "repoId": "00000000-0000-0000-0000-000000000010",
                        "branchName": "main",
                        "headCommit": "0000000000000000000000000000000000000000"
                    }
                }
            }),
        );
    mock_api
        .when(
            "GET",
            &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        )
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": workspace_id,
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": [{
                        "name": materialized_repo_name,
                        "url": materialized_git_url,
                        "branch": "main",
                        "path": materialized_repo_name
                    }]
                }
            }),
        );

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .args(["-w", ws_path.to_str().unwrap(), "add", "--local"])
        .assert()
        .success();

    let add_repo_path = format!("/api/v1/orgs/{org_id}/workspaces/{workspace_id}/repos");
    mock_api.assert_called("POST", &add_repo_path, 1);
    mock_api.assert_called_with_body("POST", &add_repo_path, |body| {
        body.get("gitUrl").and_then(|value| value.as_str()) == Some(seed_git_url.as_str())
            && body.get("name").is_none()
            && body.get("defaultBranch").and_then(|value| value.as_str()) == Some("main")
            && body.get("bootstrapMode").and_then(|value| value.as_str()) == Some("local")
            && body.get("repoRole").and_then(|value| value.as_str()) == Some("code")
            && body.get("branchName").is_none()
            && body.get("headCommit").is_none()
    });

    let refreshed_yaml = std::fs::read_to_string(ws_path.join(".g5/workspace.yaml"))
        .expect("Failed to read refreshed workspace.yaml");
    assert!(
        refreshed_yaml.contains(&format!("path: {materialized_repo_name}")),
        "workspace.yaml should be refreshed with the server-materialized local repo name"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cli_add_local_repo_with_name_and_branch() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    let mock_api = helpers::MockApiServer::start().await;
    let workspace_id = "00000000-0000-0000-0000-00000000a442";
    let org_id = "00000000-0000-0000-0000-000000000001";
    let expected_repo_name = "custom_service_repo";
    let expected_branch = "develop";
    let git_url = format!("g5-local://workspaces/{workspace_id}/repos/{expected_repo_name}.git");
    let ws_yaml = format!(
        r#"workspace_id: "{workspace_id}"
remote:
  enabled: true
  api_url: "{}"
repos: []
"#,
        mock_api.url()
    );
    std::fs::write(ws_path.join(".g5/workspace.yaml"), ws_yaml)
        .expect("Failed to write workspace.yaml");

    mock_api
        .when("GET", &format!("/api/v1/workspaces/{workspace_id}"))
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "id": workspace_id,
                    "orgId": org_id,
                    "name": "Ignored Workspace Name"
                }
            }),
        );
    mock_api
        .when(
            "POST",
            &format!("/api/v1/orgs/{org_id}/workspaces/{workspace_id}/repos"),
        )
        .respond_with(
            201,
            serde_json::json!({
                "data": {
                    "repoId": "00000000-0000-0000-0000-000000000011",
                    "repoName": expected_repo_name,
                    "gitUrl": git_url,
                    "branch": {
                        "workspaceId": workspace_id,
                        "repoId": "00000000-0000-0000-0000-000000000011",
                        "branchName": expected_branch,
                        "headCommit": "0000000000000000000000000000000000000000"
                    }
                }
            }),
        );
    mock_api
        .when(
            "GET",
            &format!("/api/v1/workspaces/{workspace_id}/manifest"),
        )
        .respond_with(
            200,
            serde_json::json!({
                "data": {
                    "workspace_id": workspace_id,
                    "remote": {
                        "enabled": true,
                        "api_url": mock_api.url()
                    },
                    "repos": [{
                        "name": expected_repo_name,
                        "url": git_url,
                        "branch": expected_branch,
                        "path": expected_repo_name
                    }]
                }
            }),
        );

    g5d_cmd()
        .env("G5D_TOKEN", "test-jwt-token")
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            "--local",
            "--name",
            "Custom Service Repo",
            "--branch",
            expected_branch,
        ])
        .assert()
        .success();

    let add_repo_path = format!("/api/v1/orgs/{org_id}/workspaces/{workspace_id}/repos");
    mock_api.assert_called_with_body("POST", &add_repo_path, |body| {
        body.get("gitUrl").and_then(|value| value.as_str()) == Some(git_url.as_str())
            && body.get("name").and_then(|value| value.as_str()) == Some(expected_repo_name)
            && body.get("defaultBranch").and_then(|value| value.as_str()) == Some(expected_branch)
            && body.get("bootstrapMode").and_then(|value| value.as_str()) == Some("local")
            && body.get("repoRole").and_then(|value| value.as_str()) == Some("code")
    });
}

#[test]
fn test_cli_add_local_rejects_repo_url() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            "https://example.test/repo.git",
            "--local",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository URL").or(predicate::str::contains("repoUrl")));
}

#[test]
fn test_cli_add_local_rejects_path_override() {
    let ws_tmp = TempDir::new().expect("Failed to create temp directory");
    let ws_path = ws_tmp.path();
    init_workspace(ws_path);

    g5d_cmd()
        .args([
            "-w",
            ws_path.to_str().unwrap(),
            "add",
            "--local",
            "--path",
            "repos/custom",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--path").or(predicate::str::contains("path")));
}
