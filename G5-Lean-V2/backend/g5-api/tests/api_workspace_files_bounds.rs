//! Committed reads keep to the live caps: a committed tree lists at most
//! the Files tree's entry cap and says when it stopped, a committed file
//! past the read limit is refused from its object header, and neither
//! grows with what a repository holds (G5-Lean #450, #455).

use crate::helpers;

use std::path::Path;
use std::process::Command;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};

/// The Files tree's entry cap (`workspace_reader::FILE_TREE_ENTRY_CAP`).
const FILE_TREE_ENTRY_CAP: usize = 10_000;
/// The most bytes one file read materializes
/// (`routes::workspaces::MAX_WORKSPACE_FILE_BYTES`).
const MAX_WORKSPACE_FILE_BYTES: usize = 32 * 1024 * 1024;

#[tokio::test]
async fn owner_file_save_preserves_large_document_request_envelope() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let path = format!("docs/{}/questionnaire.html", workspace.id);
    let hash = runtime.seed_file(workspace.repo_id, &path, "before").await;
    let route = format!(
        "/api/v1/workspaces/{}/files/{path}?repoId={}",
        workspace.id, workspace.repo_id,
    );
    // The real HTTP extractor, owner authority and live write run here. The
    // browser fixture's onSave stub cannot catch the former 2 MiB rejection.
    let content = format!("<html>\r\n{}\r\n</html>", "x".repeat(3 * 1024 * 1024));
    let response = client
        .put_json(
            &route,
            &json!({
                "content": content,
                "encoding": "utf-8",
                "expectedHash": hash,
                "expectedVmId": runtime.vm_id(),
                "commitAndPush": false,
            }),
        )
        .await;
    let status = response.status();
    let result: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{result}");
    let saved_content = runtime.file_content(workspace.repo_id, &path).await;
    assert!(
        saved_content.as_deref() == Some(content.as_str()),
        "runtime must store the exact submitted bytes"
    );

    // The envelope is inclusive and bounded. A different VM deliberately
    // refuses the admitted request before a large runtime write is attempted.
    let mut request = json!({
        "content": "",
        "encoding": "utf-8",
        "expectedHash": result["data"]["hash"],
        "expectedVmId": uuid::Uuid::new_v4(),
        "commitAndPush": true,
    });
    let envelope_bytes = serde_json::to_vec(&request).unwrap().len();
    for (bytes, expected) in [
        (MAX_WORKSPACE_FILE_BYTES, StatusCode::CONFLICT),
        (MAX_WORKSPACE_FILE_BYTES + 1, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        request["content"] = Value::String("x".repeat(bytes - envelope_bytes));
        assert_eq!(serde_json::to_vec(&request).unwrap().len(), bytes);
        assert_eq!(client.put_json(&route, &request).await.status(), expected);
    }
    drop(runtime);
    app.cleanup().await;
}

fn git(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Commit `files` on the workspace's code repository in one commit and
/// advance the workspace to it.
async fn commit_files(app: &TestApp, workspace: &mut TestWorkspace, files: &[(String, Vec<u8>)]) {
    let repo_path = app.repo_path(workspace.repo_id).await;
    for (path, content) in files {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(repo_path.join(parent)).expect("create parent dirs");
        }
        std::fs::write(repo_path.join(path), content).expect("write committed file");
    }
    git(&repo_path, &["add", "-A"]);
    git(&repo_path, &["commit", "-q", "-m", "wide and large"]);
    workspace.head_commit = git(&repo_path, &["rev-parse", "HEAD"]);
}

fn live_members(workspace: &TestWorkspace) -> Value {
    json!([{
        "repo_id": workspace.repo_id,
        "root_path": "",
        "shared_branch": workspace.name,
        "upstream_branch": "main",
        "head": workspace.name,
        "head_oid": workspace.head_commit,
        "on_shared_branch": true,
        "detached_head": false,
        "foreign_head": false,
        "internal_ref_present": true,
        "internal_ref": workspace.head_commit,
        "against_internal": {"ahead": 0, "behind": 0},
        "against_base": {"ahead": 0, "behind": 0},
        "dirty": [],
        "errors": [],
        "sequencer": [],
        "conflicts": [],
    }])
}

async fn read_tree(
    client: &helpers::AuthenticatedClient,
    workspace: &TestWorkspace,
    query: &str,
) -> Value {
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files?{query}",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("tree response JSON");
    assert_eq!(status, StatusCode::OK, "tree read failed: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn committed_file_tree_stops_at_the_entry_cap_and_says_so() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let mut workspace = TestWorkspace::create(&app, &org).await;

    // One directory wider than the cap, and a sibling that fits.
    let mut files = (0..FILE_TREE_ENTRY_CAP + 1)
        .map(|index| (format!("generated/{index:05}.txt"), b"x\n".to_vec()))
        .collect::<Vec<_>>();
    files.push(("src/main.rs".to_string(), b"fn main() {}\n".to_vec()));
    files.push(("VERSION".to_string(), b"1.2.3\n".to_vec()));
    commit_files(&app, &mut workspace, &files).await;

    // The root lists its few children whole.
    let root = read_tree(&client, &workspace, "depth=1").await;
    assert_eq!(root["source"], "committed", "{root}");
    assert_eq!(root["truncated"], false, "{root}");
    assert_eq!(root["repoOid"], workspace.head_commit);
    let names = root["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(names.contains(&"generated".to_string()), "{names:?}");
    assert!(names.contains(&"src".to_string()), "{names:?}");

    // The wide directory lists the cap and reports the rest exists.
    let wide = read_tree(&client, &workspace, "path=generated&depth=1").await;
    assert_eq!(wide["truncated"], true, "{wide}");
    assert_eq!(
        wide["entries"].as_array().unwrap().len(),
        FILE_TREE_ENTRY_CAP
    );
    assert_eq!(wide["entries"][0]["path"], "generated/00000.txt");

    // The whole repository recursively stops at the same cap.
    let whole = read_tree(&client, &workspace, "depth=-1").await;
    assert_eq!(whole["truncated"], true, "{whole}");
    fn count(entries: &[Value]) -> usize {
        entries
            .iter()
            .map(|entry| {
                1 + entry["children"]
                    .as_array()
                    .map_or(0, |children| count(children))
            })
            .sum()
    }
    assert_eq!(
        count(whole["entries"].as_array().unwrap()),
        FILE_TREE_ENTRY_CAP
    );

    // A path that is a file lists that file, sized from its object header.
    let single = read_tree(&client, &workspace, "path=VERSION").await;
    assert_eq!(single["truncated"], false, "{single}");
    assert_eq!(single["entries"].as_array().unwrap().len(), 1, "{single}");
    assert_eq!(single["entries"][0]["path"], "VERSION");
    assert_eq!(single["entries"][0]["type"], "file");
    assert_eq!(single["entries"][0]["size"], 6);

    // The attached owner: the tracked set is past the live cap before any
    // runtime call, so the committed cut is served with its reason, within
    // the same cap.
    let _runtime = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    let owner = read_tree(&client, &workspace, "depth=-1").await;
    assert_eq!(owner["source"], "committed", "{owner}");
    assert_eq!(owner["liveFallback"], "entry_cap", "{owner}");
    assert_eq!(owner["truncated"], true, "{owner}");
    assert_eq!(
        count(owner["entries"].as_array().unwrap()),
        FILE_TREE_ENTRY_CAP
    );
}

#[tokio::test]
async fn oversized_committed_file_is_refused_before_it_is_read() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead, Scope::TasksRead])
            .await;
    let client = app.authenticated_client(&user);
    let mut workspace = TestWorkspace::create(&app, &org).await;

    let oversize = MAX_WORKSPACE_FILE_BYTES + 1;
    commit_files(
        &app,
        &mut workspace,
        &[
            ("assets/huge.bin".to_string(), vec![0_u8; oversize]),
            ("assets/small.bin".to_string(), vec![0_u8, 1, 2, 255]),
        ],
    )
    .await;

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/assets/huge.bin",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("error JSON");
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "FILE_TOO_LARGE", "{body}");
    assert_eq!(body["error"]["details"]["path"], "assets/huge.bin");
    assert_eq!(body["error"]["details"]["size"], oversize);
    assert_eq!(
        body["error"]["details"]["maxBytes"],
        MAX_WORKSPACE_FILE_BYTES
    );
    assert_eq!(
        body["error"]["message"],
        format!(
            "assets/huge.bin is {oversize} bytes; at most {MAX_WORKSPACE_FILE_BYTES} bytes can be read here"
        ),
        "{body}"
    );

    // The tree still sizes the blob from its header without loading it.
    let tree = read_tree(&client, &workspace, "path=assets&depth=1").await;
    let huge = tree["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "huge.bin")
        .expect("huge.bin is listed");
    assert_eq!(huge["size"], oversize);

    // A file within the limit reads as before, binary as base64.
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/assets/small.bin",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("file JSON");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["encoding"], "base64");
    assert_eq!(body["data"]["content"], "AAEC/w==");
    assert_eq!(body["data"]["size"], 4);
    assert_eq!(body["data"]["source"], "committed");
}
