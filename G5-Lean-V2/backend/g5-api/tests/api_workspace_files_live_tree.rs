//! The attached owner's live Files tree honors the checkout's `.gitignore`
//! files and never fails on an oversized checkout.
//!
//! Regression guard for the QA symptom of 2026-10-05: a chat turn ran
//! `cargo test` and `npm ci` in the workspace VM, after which the live walk
//! listed `target/` and `node_modules/` for 89 s and answered 422 "file tree
//! exceeds 10000 entries", so the Files panel showed "Could not load files"
//! while the repository itself is under 3000 entries.

use crate::helpers;

use g5_auth::Scope;
use helpers::runtime_peer::LIVE_HEAD_OID;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};

fn collect_paths(entries: &[Value], out: &mut Vec<String>) {
    for entry in entries {
        out.push(
            entry["path"]
                .as_str()
                .expect("tree entry has a path")
                .to_string(),
        );
        if let Some(children) = entry["children"].as_array() {
            collect_paths(children, out);
        }
    }
}

/// The live Changes report the tree route reads the repository HEAD from.
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

async fn read_full_tree(
    client: &helpers::AuthenticatedClient,
    workspace: &TestWorkspace,
) -> (Value, Vec<String>) {
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files?depth=-1",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("tree response JSON");
    assert_eq!(status, StatusCode::OK, "tree read failed: {body}");
    let mut paths = Vec::new();
    collect_paths(
        body["data"]["entries"]
            .as_array()
            .expect("tree entries are an array"),
        &mut paths,
    );
    (body["data"].clone(), paths)
}

#[tokio::test]
async fn live_file_tree_skips_paths_the_checkout_ignores() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    let repo_id = workspace.repo_id;

    for (path, content) in [
        // `*.md` matches the committed README.md, which Git keeps tracked.
        (
            ".gitignore",
            "target/\nnode_modules/\n*.log\n!keep.log\n/dist\n*.md\n",
        ),
        ("README.md", "live\n"),
        ("notes.md", "untracked and ignored by *.md\n"),
        ("src/main.rs", "fn main() {}\n"),
        ("keep.log", "re-included at the root\n"),
        ("debug.log", "ignored by *.log\n"),
        ("dist/bundle.js", "ignored by the anchored /dist\n"),
        ("target/debug/deps/g5.rlib", "build output\n"),
        ("frontend/.gitignore", "dist/\n!*.log\n"),
        ("frontend/build.log", "re-included by the nested file\n"),
        ("frontend/dist/app.js", "ignored by the nested dist/\n"),
        ("frontend/node_modules/pkg/index.js", "dependency\n"),
        ("frontend/src/main.ts", "export {}\n"),
        (
            "backend/dist/keep.rs",
            "the nested dist/ rule stays under frontend/\n",
        ),
    ] {
        runtime.seed_file(repo_id, path, content).await;
    }

    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert_eq!(data["source"], "live");
    assert!(data["liveFallback"].is_null(), "{data}");
    assert_eq!(
        data["repoOid"], LIVE_HEAD_OID,
        "the live tree is labeled with the checkout HEAD the VM read, not the durable head: {data}"
    );
    assert_ne!(data["repoOid"], workspace.head_commit);

    for expected in [
        "README.md",
        "src/main.rs",
        "keep.log",
        "frontend/build.log",
        "frontend/src/main.ts",
        "backend/dist/keep.rs",
    ] {
        assert!(
            paths.iter().any(|path| path == expected),
            "{expected} must be listed: {paths:?}"
        );
    }
    for hidden in [
        "notes.md",
        "debug.log",
        "dist",
        "target",
        "frontend/dist",
        "frontend/node_modules",
    ] {
        assert!(
            !paths
                .iter()
                .any(|path| path == hidden || path.starts_with(&format!("{hidden}/"))),
            "{hidden} must not be listed: {paths:?}"
        );
    }

    // The VM listed the subtree in one call behind one watch on its root;
    // the API no longer walks directory by directory
    // (`g5.documents.read.live-tree`).
    let methods = runtime.received_rpc_methods().await;
    let count = |name: &str| methods.iter().filter(|method| *method == name).count();
    assert_eq!(
        count("fs.tree"),
        1,
        "one tree call lists the checkout: {methods:?}"
    );
    assert_eq!(
        count("fs.watch"),
        1,
        "one watch covers the subtree: {methods:?}"
    );
    assert_eq!(count("fs.list"), 0, "no per-directory walk: {methods:?}");
    assert_eq!(
        count("fs.read"),
        0,
        "the VM reads its own .gitignore files: {methods:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_live_tree_too_large_in_bytes_serves_the_committed_cut() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    let repo_id = workspace.repo_id;

    // Far fewer entries than the entry cap, but their paths alone exceed the
    // 3 MiB byte budget that keeps one reply under the runtime's 4 MiB
    // reply limit: 2,000 files with 2 KiB names.
    runtime.seed_file(repo_id, "README.md", "live\n").await;
    let long = "x".repeat(2040);
    for index in 0..2_000 {
        runtime
            .seed_file(repo_id, &format!("generated/{index:04}{long}"), "x\n")
            .await;
    }

    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert_eq!(
        data["source"], "committed",
        "an oversized reply falls back to the committed cut: {data}"
    );
    assert_eq!(data["liveFallback"], "entry_cap", "{data}");
    assert!(paths.iter().any(|path| path == "README.md"), "{paths:?}");
    assert!(
        !paths.iter().any(|path| path.starts_with("generated")),
        "the uncommitted generated tree is not served: {paths:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn the_live_tree_does_not_wait_for_the_changes_report() {
    // On QA an agent's modules starved the VM's `workspace.changes` run and
    // the Files tab hung on it for 20 s before the browser gave up. The tree
    // labels itself with the HEAD the VM read beside the listing.
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime =
        TestWorkspaceRuntimePeer::attach_with_unresponsive_changes(&app, &workspace, user.id).await;
    runtime
        .seed_file(workspace.repo_id, "src/main.rs", "fn main() {}\n")
        .await;

    let started = std::time::Instant::now();
    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the tree must not wait on the Changes budget: {:?}",
        started.elapsed()
    );
    assert_eq!(data["source"], "live", "{data}");
    assert_eq!(data["repoOid"], LIVE_HEAD_OID, "{data}");
    assert!(paths.iter().any(|path| path == "src/main.rs"), "{paths:?}");
    let methods = runtime.received_rpc_methods().await;
    assert!(
        !methods.iter().any(|method| method == "modules.invoke"),
        "the tree read must not start a Changes run: {methods:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_live_tree_without_a_readable_head_carries_no_oid() {
    // The durable head may not describe the checkout, so a live listing
    // whose HEAD the VM could not read is labeled with no OID rather than
    // that one (`g5.documents.read.labels`).
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_without_tree_head(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    runtime
        .seed_file(workspace.repo_id, "src/main.rs", "fn main() {}\n")
        .await;

    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert_eq!(data["source"], "live", "{data}");
    assert!(
        data["repoOid"].is_null(),
        "no OID rather than a wrong one: {data}"
    );
    assert_ne!(data["repoOid"], workspace.head_commit);
    assert!(paths.iter().any(|path| path == "src/main.rs"), "{paths:?}");

    app.cleanup().await;
}

#[tokio::test]
async fn an_outdated_runtime_serves_the_committed_cut_with_its_reason() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_without_tree_rpc(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    runtime
        .seed_file(workspace.repo_id, "untracked.rs", "live only\n")
        .await;

    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert_eq!(
        data["source"], "committed",
        "a VM without fs.tree falls back to the committed cut: {data}"
    );
    assert_eq!(data["repoOid"], workspace.head_commit);
    assert_eq!(
        data["liveFallback"], "runtime_outdated",
        "the panel is told why the live tree was replaced: {data}"
    );
    assert!(
        paths.iter().any(|path| path == "README.md"),
        "the committed cut is served: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path == "untracked.rs"),
        "live-only work is not served from the committed cut: {paths:?}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn oversized_live_file_tree_serves_the_committed_cut() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &workspace,
        user.id,
        live_members(&workspace),
    )
    .await;
    let repo_id = workspace.repo_id;

    // More entries than the live tree carries, in no ignored directory.
    runtime.seed_file(repo_id, "README.md", "live\n").await;
    for directory in 0..101 {
        for file in 0..100 {
            runtime
                .seed_file(
                    repo_id,
                    &format!("generated/{directory:03}/file{file:03}.txt"),
                    "x\n",
                )
                .await;
        }
    }

    let (data, paths) = read_full_tree(&client, &workspace).await;
    assert_eq!(
        data["source"], "committed",
        "an oversized checkout falls back to the committed cut: {data}"
    );
    assert_eq!(data["repoOid"], workspace.head_commit);
    assert_eq!(
        data["liveFallback"], "entry_cap",
        "the panel is told why the live tree was replaced: {data}"
    );
    assert!(
        paths.iter().any(|path| path == "README.md"),
        "the committed cut lists the seeded repository: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.starts_with("generated")),
        "the uncommitted generated tree is not served: {paths:?}"
    );

    app.cleanup().await;
}
