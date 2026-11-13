//! Git refs are the head authority for every workspace content read (P1.1).
//!
//! The read tests move a workspace branch ref with plain git on the internal
//! mirror and never touch the database. Files, trees and document
//! reads must follow the ref without a database head update.
//!
//! The landing tests at the end cover the server-side writer (P1.4): a branch
//! commit compare-and-swaps the ref from the cut's expected head, a replay of
//! a landed cut is idempotent, and a cut whose expected head no longer names
//! the ref is refused without writing the ref or the projection.

use crate::helpers;

use std::path::Path;
use std::process::Command;

use g5_api::errors::G5Error;
use g5_api::services::branch_commit::{
    map_branch_commit_error, prepare_branch_commit, BranchCommitError, FenceMode,
    LandedBranchCommit,
};
use g5_auth::Scope;
use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

fn run_git(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout should be utf-8")
        .trim()
        .to_string()
}

/// Commit files on the mirror's checked-out authoritative branch with plain
/// git. The branch ref moves; nothing in Postgres does.
fn commit_with_plain_git(
    repo_path: &Path,
    branch: &str,
    files: &[(&str, &str)],
    message: &str,
) -> String {
    let current_branch = run_git(repo_path, &["symbolic-ref", "--short", "HEAD"]);
    if current_branch != branch {
        run_git(repo_path, &["checkout", branch]);
    }
    for (path, contents) in files {
        if let Some(parent) = repo_path.join(path).parent() {
            std::fs::create_dir_all(parent).expect("create committed file parent");
        }
        std::fs::write(repo_path.join(path), contents).expect("write committed file");
        run_git(repo_path, &["add", path]);
    }
    run_git(repo_path, &["commit", "-m", message]);
    run_git(repo_path, &["rev-parse", "HEAD"])
}

async fn mirror_path(app: &TestApp, repo_id: Uuid) -> std::path::PathBuf {
    app.repo_path(repo_id).await
}

#[tokio::test]
async fn provider_refresh_preserves_pushed_view_and_publication_keeps_private_recovery() {
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
    let path = mirror_path(&app, workspace.repo_id).await;
    let a = workspace.head_commit.clone();
    let snapshot = commit_with_plain_git(
        &path,
        &workspace.name,
        &[("README.md", "saved dirty work\n")],
        "snapshot",
    );
    let flush = Uuid::new_v4();
    run_git(
        &path,
        &[
            "update-ref",
            &format!("refs/g5/flush/{}/{flush}/snapshot", workspace.id),
            &snapshot,
        ],
    );
    run_git(&path, &["reset", "--hard", &a]);
    let receipts = serde_json::json!([{
        "repo_id": workspace.repo_id, "branch_oid": a, "local_branch_oid": a,
        "local_head_oid": a, "head_ref": format!("refs/heads/{}", workspace.name),
        "sequencer": "none", "snapshot_oid": snapshot, "dirty": true,
        "dirty_paths": ["README.md"]
    }]);
    sqlx::query("INSERT INTO workspace_flushes(flush_id,org_id,workspace_id,origin,vm_id,epoch,reason,required_repo_ids,repos,complete_at,request_key)
                 VALUES ($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'saved-view')")
        .bind(flush).bind(org.id).bind(workspace.id).bind(Uuid::new_v4().to_string())
        .bind(vec![workspace.repo_id]).bind(&receipts).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,status,last_flush_id) VALUES ($1,$2,'stopped',$3)")
        .bind(org.id).bind(workspace.id).bind(flush).execute(&app.db).await.unwrap();
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/files/README.md",
        workspace.id, workspace.repo_id
    );
    let before: Value = client.get(&route).await.json().await.unwrap();
    assert_eq!(before["data"]["source"], "committed", "{before}");
    assert_eq!(before["data"]["content"], "test workspace\n");
    let mut db = app.scoped_db_for_org(org.id).await;
    let operation_cut = g5_api::services::workspace_reader::resolve_published_snapshot(
        &mut db,
        &app.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: workspace.id,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        operation_cut.repo(workspace.repo_id).unwrap().head_commit,
        a,
        "Git operation leases must use the published branch, never a recovery snapshot"
    );
    db.rollback().await.unwrap();

    let b = commit_with_plain_git(
        &path,
        &workspace.name,
        &[("README.md", "published work\n")],
        "provider commit",
    );
    run_git(
        &path,
        &[
            "update-ref",
            &g5_contracts::workspace_git::provider_ref(workspace.id, "main"),
            &b,
        ],
    );
    run_git(
        &path,
        &["update-ref", &format!("refs/heads/{}", workspace.name), &a],
    );
    let refreshed: Value = client.get(&route).await.json().await.unwrap();
    assert_eq!(refreshed["data"]["source"], "committed");
    assert_eq!(refreshed["data"]["content"], "test workspace\n");

    run_git(
        &path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &b,
            &a,
        ],
    );
    let published: Value = client.get(&route).await.json().await.unwrap();
    assert_eq!(published["data"]["source"], "committed", "{published}");
    assert_eq!(published["data"]["content"], "published work\n");
    assert_eq!(published["data"]["repoOid"], b);
    let retained: (Uuid, Value) = sqlx::query_as("SELECT vm.last_flush_id,f.repos FROM workspace_vms vm JOIN workspace_flushes f ON f.flush_id=vm.last_flush_id WHERE vm.workspace_id=$1")
        .bind(workspace.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(retained, (flush, receipts));
    assert_eq!(
        run_git(
            &path,
            &[
                "rev-parse",
                &format!("refs/g5/flush/{}/{flush}/snapshot", workspace.id)
            ]
        ),
        snapshot
    );
}

async fn attach_docs_repo(app: &TestApp, org: &TestOrg, workspace: &TestWorkspace) -> Uuid {
    let docs_repo_id = Uuid::new_v4();
    let slug = format!("docs-{}", docs_repo_id.simple());
    let repo_path = app.init_repo(org.id, docs_repo_id).await;
    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(&repo_path, &["config", "user.name", "G5 Test"]);
    run_git(&repo_path, &["checkout", "-B", "main"]);
    let docs_head = commit_with_plain_git(
        &repo_path,
        "main",
        &[("docs/seed.md", "# Seed\n\nSeed doc.\n")],
        "seed docs repo",
    );
    run_git(&repo_path, &["branch", &workspace.name, &docs_head]);
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) VALUES ($1, $2, $3, $3, $4, 'main')",
    )
    .bind(docs_repo_id)
    .bind(org.id)
    .bind(&slug)
    .bind(repo_path.display().to_string())
    .execute(&app.db)
    .await
    .expect("insert docs repo");
    sqlx::query("UPDATE workspaces SET docs_repo_id = $1 WHERE id = $2")
        .bind(docs_repo_id)
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("mark docs repo on workspace");
    sqlx::query(
        "INSERT INTO workspace_repos \
             (org_id, workspace_id, repo_id, root_path, role) \
         VALUES ($1, $2, $3, 'docs', 'docs')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(docs_repo_id)
    .execute(&app.db)
    .await
    .expect("insert authoritative docs repository mapping");
    docs_repo_id
}

#[tokio::test]
async fn file_and_tree_reads_follow_plain_git_ref_changes() {
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
    let repo_path = mirror_path(&app, workspace.repo_id).await;
    let seeded_head = workspace.head_commit.clone();

    let moved_head = commit_with_plain_git(
        &repo_path,
        &workspace.name,
        &[
            ("README.md", "moved by plain git\n"),
            ("src/new_file.rs", "pub fn new_file() {}\n"),
        ],
        "move the ref with plain git",
    );
    assert_ne!(moved_head, seeded_head);
    // The database never learned about the new commit.

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/README.md",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("file response JSON");
    assert_eq!(
        body["data"]["content"].as_str(),
        Some("moved by plain git\n")
    );
    assert_eq!(body["data"]["source"], "committed");
    assert_eq!(body["data"]["repoOid"], moved_head);
    assert!(
        body["data"]["snapshotAt"].as_str().is_some(),
        "committed reads expose the observation time used for staleness UI"
    );

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files?depth=-1",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("tree response JSON");
    let tree = serde_json::to_string(&body["data"]).expect("tree JSON");
    assert!(
        tree.contains("new_file.rs"),
        "tree must list the file committed with plain git: {tree}"
    );

    // Moving the ref backwards is followed too: the read is the ref, not the
    // newest commit the mirror happens to contain.
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &seeded_head,
        ],
    );
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/README.md",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("file response JSON");
    assert_eq!(body["data"]["content"].as_str(), Some("test workspace\n"));
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files?depth=-1",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("tree response JSON");
    let tree = serde_json::to_string(&body["data"]).expect("tree JSON");
    assert!(
        !tree.contains("new_file.rs"),
        "tree must follow the ref back to the seeded commit: {tree}"
    );

    // Still nothing in Postgres moved.
    app.cleanup().await;
}

#[tokio::test]
async fn docs_reads_follow_the_docs_branch_ref() {
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
    let docs_repo_id = attach_docs_repo(&app, &org, &workspace).await;
    let docs_path = mirror_path(&app, docs_repo_id).await;
    let seeded_head = branch_ref(&docs_path, &workspace.name);

    let moved_head = commit_with_plain_git(
        &docs_path,
        &workspace.name,
        &[(
            "docs/ref-authority.md",
            "# Ref Authority\n\nRead from the ref.\n",
        )],
        "add a doc with plain git",
    );
    assert_ne!(moved_head, seeded_head);

    let response = client
        .get(&format!("/api/v1/workspaces/{}/docs", workspace.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("docs list JSON");
    let docs = body["data"]["docs"].as_array().expect("docs list array");
    assert_eq!(body["data"]["source"], "committed");
    assert_eq!(body["data"]["repoOid"], moved_head);
    assert!(body["data"]["snapshotAt"].as_str().is_some());
    assert!(
        docs.iter()
            .any(|doc| doc["filePath"].as_str() == Some("docs/ref-authority.md")),
        "docs list must include the doc committed with plain git: {docs:?}"
    );

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/docs/by-path?path=docs/ref-authority.md",
            workspace.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("doc by path JSON");
    assert_eq!(
        body["data"]["content"].as_str(),
        Some("# Ref Authority\n\nRead from the ref.\n")
    );

    // The docs head columns are stale and irrelevant.
    app.cleanup().await;
}

#[tokio::test]
async fn internal_document_metadata_refresh_uses_final_schema_and_canonical_content() {
    use helpers::TestWorkspaceRuntimePeer;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo = attach_docs_repo(&app, &org, &workspace).await;
    let path = "views/retained.pm_view.md";
    let content = "---\nlinked_doc_paths: [docs/linked.md]\n---\n# Canonical view\n";
    commit_with_plain_git(
        &mirror_path(&app, repo).await,
        &workspace.name,
        &[(path, content)],
        "committed view",
    );
    let doc_id: Uuid = sqlx::query_scalar(
        "INSERT INTO docs(org_id,workspace_id,repo_id,name,doc_type,file_path,content, \
         last_uplifted_commit_sha,last_downlifted_commit_sha,last_uplifted_at,last_downlifted_at) \
         VALUES($1,$2,$3,'Retained view','pm_view',$4,'Retained fallback',repeat('a',40),repeat('b',40),'2026-10-17T01:02:03Z','2026-10-18T04:05:06Z') RETURNING id"
    )
    .bind(org.id).bind(workspace.id).bind(repo).bind(path)
    .fetch_one(&app.db).await.unwrap();
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    runtime.seed_file(repo, path, content).await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let route = format!("{}/api/internal/docs/{doc_id}/content", app.base_url);
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(d) FROM docs d WHERE id=$1")
        .bind(doc_id)
        .fetch_one(&app.db)
        .await
        .unwrap();

    // A worker payload never supplies content missing from the canonical file.
    let mismatch = app
        .client
        .put(&route)
        .bearer_auth(&token)
        .json(&json!({"content": "Uncommitted worker result"}))
        .send()
        .await
        .unwrap();
    assert_eq!(mismatch.status(), StatusCode::NO_CONTENT);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(d) FROM docs d WHERE id=$1")
        .bind(doc_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(after, before);

    let refresh = app
        .client
        .put(&route)
        .bearer_auth(&token)
        .json(&json!({"content": content}))
        .send()
        .await
        .unwrap();
    let status = refresh.status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        refresh.text().await.unwrap()
    );
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(d) FROM docs d WHERE id=$1")
        .bind(doc_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(after["content"].is_null());
    assert_eq!(
        after["content_hash"],
        format!("{:x}", Sha256::digest(content.as_bytes()))
    );
    assert_eq!(after["linked_doc_paths"], json!(["docs/linked.md"]));
    assert_eq!(
        after["last_uplifted_commit_sha"],
        before["last_uplifted_commit_sha"]
    );
    assert_eq!(
        after["last_downlifted_commit_sha"],
        before["last_downlifted_commit_sha"]
    );

    for suffix in [doc_id.to_string(), format!("views/{doc_id}")] {
        let response = app
            .authenticated_client(&owner)
            .get(&format!(
                "/api/v1/workspaces/{}/docs/{suffix}",
                workspace.id
            ))
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{suffix}: {body}");
        assert_eq!(body["data"]["content"], content, "{suffix}");
        assert_eq!(
            body["data"]["lastUpliftedAt"], "2026-10-17T01:02:03+00:00",
            "{suffix}"
        );
        assert_eq!(
            body["data"]["lastDownliftedAt"], "2026-10-18T04:05:06+00:00",
            "{suffix}"
        );
        assert_eq!(
            body["data"]["lastUpliftedCommitSha"],
            "a".repeat(40),
            "{suffix}"
        );
        assert_eq!(
            body["data"]["lastDownliftedCommitSha"],
            "b".repeat(40),
            "{suffix}"
        );
    }
    drop(runtime);
    app.cleanup().await;
}

#[tokio::test]
async fn markdown_files_follow_refs_without_recreating_retired_nodes() {
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
    let path = mirror_path(&app, workspace.repo_id).await;
    let file = ".semlift/specs/ref-root.semlift.md";
    let content = "---\ncomponent_id: ref-root\n---\n# Ref root\n";
    let head = commit_with_plain_git(&path, &workspace.name, &[(file, content)], "new Markdown");
    let route = format!(
        "/api/v1/workspaces/{}/repos/{}/files/{file}",
        workspace.id, workspace.repo_id
    );
    let response = client.get(&route).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["content"], content);
    assert_eq!(body["data"]["repoOid"], head);
    assert!(body["data"].get("nodeId").is_none());
    run_git(
        &path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &workspace.head_commit,
        ],
    );
    assert_eq!(client.get(&route).await.status(), StatusCode::NOT_FOUND);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','projection_state','doc_node_links')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn a_deleted_branch_ref_fails_the_read_closed() {
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
    let repo_path = mirror_path(&app, workspace.repo_id).await;

    // Detach so the checked-out branch can be deleted, then drop the ref.
    run_git(&repo_path, &["checkout", "--detach"]);
    let workspace_ref = format!("refs/heads/{}", workspace.name);
    run_git(&repo_path, &["update-ref", "-d", &workspace_ref]);
    // The seeded commit still exists, but its presence cannot replace a missing ref.

    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/README.md",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert!(
        response.status().is_client_error(),
        "a read with no branch ref must fail closed, got {}",
        response.status()
    );
    let body: Value = response.json().await.expect("error JSON");
    let text = serde_json::to_string(&body).expect("error text");
    assert!(
        text.contains(&workspace_ref) && text.contains("absent"),
        "error must name the absent ref: {text}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn uploaded_project_docs_keep_empty_and_nonempty_live_content_through_edit_and_reload() {
    use helpers::TestWorkspaceRuntimePeer;
    use serde_json::json;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo = attach_docs_repo(&app, &org, &workspace).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    for (name, initial) in [
        ("empty.md", ""),
        (
            "request.md",
            "[I1. Uploaded intent]\r\nKeep café 中文 😀.\r\n",
        ),
    ] {
        let path = format!("docs/{}/{name}", workspace.id);
        let created = client
            .post_json(
                &format!("/api/v1/workspaces/{}/docs", workspace.id),
                &json!({"name": name, "filePath": path, "content": initial}),
            )
            .await;
        let status = created.status();
        let body: Value = created.json().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["data"]["content"], initial);
        assert_eq!(
            runtime.file_content(repo, &path).await.as_deref(),
            Some(initial)
        );
        let route = format!(
            "/api/v1/workspaces/{}/files/{path}?repoId={repo}",
            workspace.id
        );
        let read: Value = client.get(&route).await.json().await.unwrap();
        assert_eq!(read["data"]["content"], initial, "{read}");
        let edited = format!("{initial}\r\nTyped **content** with  spaces.\r\n");
        let saved = client
            .put_json(
                &route,
                &json!({
                    "content": edited, "expectedHash": read["data"]["hash"],
                    "expectedVmId": runtime.vm_id(), "commitAndPush": false,
                }),
            )
            .await;
        let status = saved.status();
        let saved: Value = saved.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(
            runtime.file_content(repo, &path).await.as_deref(),
            Some(edited.as_str())
        );
        let reloaded: Value = client.get(&route).await.json().await.unwrap();
        assert_eq!(reloaded["data"]["content"], edited, "{reloaded}");
        let doc: Value = client
            .get(&format!(
                "/api/v1/workspaces/{}/docs/by-path?path={path}",
                workspace.id
            ))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(doc["data"]["content"], edited, "{doc}");
    }
    drop(runtime);
    app.cleanup().await;
}

/// Retired feature routes must not resume interpreting old files or writes.
#[tokio::test]
async fn legacy_review_endpoints_are_unavailable() {
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
    attach_docs_repo(&app, &org, &workspace).await;
    assert_eq!(
        client
            .get("/api/v1/grounding/review-metadata")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // With no dashboard route, this follows the ordinary /docs/:docId
    // route and rejects the non-UUID segment during path extraction.
    assert_eq!(
        client
            .get(&format!(
                "/api/v1/workspaces/{}/docs/implementation-overview",
                workspace.id
            ))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .put_json("/api/v1/grounding/review-actions", &serde_json::json!({}))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    app.cleanup().await;
}

#[tokio::test]
async fn generate_and_doc_reads_ignore_obsolete_review_ledgers() {
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
    let repo = attach_docs_repo(&app, &org, &workspace).await;
    let root = mirror_path(&app, repo).await;
    let path = format!("docs/{}/request.md", workspace.id);
    let ledger = format!("intent_record/external/{}.jsonl", path.replace('/', "__"));
    let content = "[I1. Keep content]\r\nExact café 中文 😀 bytes.\r\n";
    commit_with_plain_git(
        &root,
        &workspace.name,
        &[(&path, content)],
        "document without ledger",
    );
    let pending = format!(
        "/api/v1/workspaces/{}/sync/pending?direction=downlift",
        workspace.id
    );
    let read = format!(
        "/api/v1/workspaces/{}/docs/by-path?path={path}",
        workspace.id
    );
    let valid = serde_json::json!({
        "file_type": "external_doc", "file_name": path, "intent_record_number": "I1", "is_human": true,
        "review_metadata": {"schema_version": 1, "implementation_percentage": 60,
            "production_readiness_percentage": 60, "test_coverage_percentage": 60, "confidence_percentage": 80,
            "reviewed_at": "2026-10-26T00:00:00Z", "reviewed_by": "agent", "review_scope": "workspace",
            "evidence_summary": "Retired assessment", "limitations": []}
    }).to_string();
    for old_content in [
        None,
        Some(valid.as_str()),
        Some("not valid JSONL\n"),
        Some("{\"review_metadata\": false, \"review_action_state\": 123}\n"),
    ] {
        if let Some(old) = old_content {
            commit_with_plain_git(
                &root,
                &workspace.name,
                &[(&ledger, old)],
                "historical ledger",
            );
        }
        let response = client.get(&pending).await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        let rows = body["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{body}");
        assert_eq!(rows[0]["path"], path);
        assert_eq!(rows[0]["pendingReasons"], serde_json::json!([]));
        assert!(rows[0].get("reviewActionCount").is_none());
        let body: Value = client.get(&read).await.json().await.unwrap();
        assert_eq!(body["data"]["content"], content, "{body}");
        if let Some(old) = old_content {
            assert_eq!(std::fs::read_to_string(root.join(&ledger)).unwrap(), old);
        }
    }
    app.cleanup().await;
}

/// Create a commit object on top of `parent` without moving any ref, the way
/// a server-side producer builds its result before landing it.
fn commit_object_without_moving_refs(repo_path: &Path, parent: &str, message: &str) -> String {
    let tree = run_git(repo_path, &["rev-parse", &format!("{parent}^{{tree}}")]);
    run_git(
        repo_path,
        &["commit-tree", &tree, "-p", parent, "-m", message],
    )
}

fn branch_ref(repo_path: &Path, branch: &str) -> String {
    run_git(repo_path, &["rev-parse", &format!("refs/heads/{branch}")])
}

/// Prepare a single-repository workspace cut, run `before_land`, then land
/// the prepared cut through the production branch-commit path.
async fn land_workspace_cut(
    app: &TestApp,
    workspace: &TestWorkspace,
    expected_old_head: &str,
    proposed_new_head: &str,
    before_land: impl FnOnce(),
) -> Result<LandedBranchCommit, BranchCommitError> {
    let capture_db = app.scoped_db_for_org(workspace.org_id).await;
    let scope = capture_db.reopener();
    capture_db
        .rollback()
        .await
        .expect("release the capture scope");
    let prepared = prepare_branch_commit(
        &scope,
        &app.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: workspace.id,
        },
        vec![WorkspaceHeadRepoInput {
            repo_id: workspace.repo_id,
            expected_old_head: expected_old_head.to_string(),
            proposed_new_head: proposed_new_head.to_string(),
        }],
        "git_refs_authoritative_test",
    )
    .await?;
    before_land();
    let db = scope.open().await.expect("open the landing scope");
    let workspace_id = workspace.id;
    prepared
        .land_with(
            db,
            &app.state.event_bus,
            &app.repo_storage,
            FenceMode::Blocking,
            move |db, _| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE workspace_settings SET updated_at=now() WHERE workspace_id=$1",
                    )
                    .bind(workspace_id)
                    .execute(db.conn())
                    .await?;
                    Ok(())
                })
            },
        )
        .await
        .map(|(landed, ())| landed)
}

fn assert_stale_cut(error: BranchCommitError, current_head: &str) {
    let BranchCommitError::Database(error) = error else {
        panic!("expected a stale-cut refusal, got {error:?}");
    };
    match map_branch_commit_error(error) {
        G5Error::AuthorityCutStale { details } => assert!(
            details
                .mismatches
                .iter()
                .any(|mismatch| mismatch.current_head == current_head),
            "the refusal must name the ref's current head: {details:?}"
        ),
        other => panic!("expected a typed stale-cut conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn a_branch_commit_lands_by_compare_and_swap_on_the_ref_and_replays_idempotently() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_path = mirror_path(&app, workspace.repo_id).await;
    let seeded_head = workspace.head_commit.clone();
    assert_eq!(branch_ref(&repo_path, &workspace.name), seeded_head);

    let new_head =
        commit_object_without_moving_refs(&repo_path, &seeded_head, "server-side result");
    assert_eq!(
        branch_ref(&repo_path, &workspace.name),
        seeded_head,
        "building the result must not move the ref"
    );

    let landed = land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect("a cut whose expected head names the ref lands");
    assert_eq!(branch_ref(&repo_path, &workspace.name), new_head);
    assert!(
        landed.projection_warnings.is_empty(),
        "a clean landing leaves no pending projection work: {:?}",
        landed.projection_warnings
    );
    assert_eq!(
        landed
            .changed_members()
            .map(|member| member.proposed_new_head.as_str())
            .collect::<Vec<_>>(),
        vec![new_head.as_str()]
    );

    // A retried producer replays the same cut after it landed: the ref already
    // names the result, so the compare-and-swap accepts it without a write.
    let replayed = land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect("replaying a landed cut against its own result is idempotent");
    assert_eq!(branch_ref(&repo_path, &workspace.name), new_head);
    assert!(
        replayed.projection_warnings.is_empty(),
        "a replay leaves no pending projection work: {:?}",
        replayed.projection_warnings
    );
    assert_ne!(replayed.advance_id, landed.advance_id);
    app.cleanup().await;
}

#[tokio::test]
async fn a_branch_commit_whose_expected_head_moved_is_refused_and_writes_nothing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_path = mirror_path(&app, workspace.repo_id).await;
    let seeded_head = workspace.head_commit.clone();
    let new_head =
        commit_object_without_moving_refs(&repo_path, &seeded_head, "server-side result");
    let concurrent_head =
        commit_object_without_moving_refs(&repo_path, &seeded_head, "concurrent writer");

    // Another writer (a native push) moves the ref after the cut was prepared
    // against the seeded head and before it lands.
    let error = land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {
        run_git(
            &repo_path,
            &[
                "update-ref",
                &format!("refs/heads/{}", workspace.name),
                &concurrent_head,
                &seeded_head,
            ],
        );
    })
    .await
    .expect_err("a cut whose expected head no longer names the ref must be refused");
    assert_stale_cut(error, &concurrent_head);
    assert_eq!(
        branch_ref(&repo_path, &workspace.name),
        concurrent_head,
        "the concurrent writer's ref survives the refused cut"
    );

    // Preparing from the stale expectation is refused up front as well.
    let error = land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect_err("a cut prepared from a stale expected head must be refused");
    assert_stale_cut(error, &concurrent_head);
    assert_eq!(branch_ref(&repo_path, &workspace.name), concurrent_head);
    app.cleanup().await;
}

/// Install a deferred constraint trigger that fails the landing's database
/// checks when the domain callback updates settings. Returns the SQL that
/// removes it.
async fn install_deferred_commit_failure(app: &TestApp, workspace_id: Uuid) -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    let function = format!("fail_landing_commit_{suffix}");
    let trigger = format!("fail_landing_commit_trigger_{suffix}");
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN \
             IF NEW.workspace_id = '{workspace_id}'::uuid THEN \
                 RAISE EXCEPTION 'forced landing commit failure'; \
             END IF; \
             RETURN NEW; \
         END $$; \
         CREATE CONSTRAINT TRIGGER {trigger} AFTER UPDATE ON workspace_settings \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION {function}();"
    ))
    .execute(&app.db)
    .await
    .expect("install the deferred commit failure");
    format!("DROP TRIGGER {trigger} ON workspace_settings; DROP FUNCTION {function}();")
}

#[tokio::test]
async fn a_branch_commit_whose_deferred_database_check_fails_never_moves_a_ref() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let repo_path = mirror_path(&app, workspace.repo_id).await;
    let seeded_head = workspace.head_commit.clone();
    let new_head =
        commit_object_without_moving_refs(&repo_path, &seeded_head, "server-side result");

    // Deferred checks run before Git moves a ref (`g5.git.landing.commit-point`):
    // the refused landing leaves the ref untouched, so nothing is half-landed
    // and a retry starts from the same expectations.
    let uninstall = install_deferred_commit_failure(&app, workspace.id).await;
    let error = land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect_err("a refused database COMMIT must fail the landing");
    assert!(
        error.to_string().contains("forced landing commit failure"),
        "the landing must surface the database refusal: {error}"
    );
    assert_eq!(
        branch_ref(&repo_path, &workspace.name),
        seeded_head,
        "a landing refused by a deferred check never moves the ref"
    );

    // A ref that already carried the result before this landing started was
    // not written by it and must survive the same refusal untouched.
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &new_head,
            &seeded_head,
        ],
    );
    land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect_err("a replay whose database COMMIT fails must still fail");
    assert_eq!(
        branch_ref(&repo_path, &workspace.name),
        new_head,
        "a replayed ref the landing did not write must not be rewound"
    );

    // With the failure removed, the retry lands normally.
    sqlx::raw_sql(&uninstall)
        .execute(&app.db)
        .await
        .expect("remove the deferred commit failure");
    run_git(
        &repo_path,
        &[
            "update-ref",
            &format!("refs/heads/{}", workspace.name),
            &seeded_head,
            &new_head,
        ],
    );
    land_workspace_cut(&app, &workspace, &seeded_head, &new_head, || {})
        .await
        .expect("the retry lands once the database accepts the COMMIT");
    assert_eq!(branch_ref(&repo_path, &workspace.name), new_head);
    app.cleanup().await;
}

/// The seeded mirrors become addressable only after all destinations and Git
/// objects pass preflight. Keep this source-integrity check after retiring the
/// projection test that originally owned it.
#[test]
fn fixture_publish_preflights_every_mirror_without_overwriting() {
    let root = tempfile::TempDir::new().unwrap();
    let repos = [Uuid::new_v4(), Uuid::new_v4()];
    let mut heads = Vec::new();
    for repo in repos {
        let staged = root.path().join(format!(".seed-ready-{repo}"));
        std::fs::create_dir(&staged).unwrap();
        run_git(&staged, &["init", "-b", "main"]);
        run_git(&staged, &["config", "user.email", "seed@test.invalid"]);
        run_git(&staged, &["config", "user.name", "Seed"]);
        heads.push(commit_with_plain_git(
            &staged,
            "main",
            &[("README.md", "Retained fixture bytes\n")],
            "seed",
        ));
    }
    let publish = || {
        Command::new("sh")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/publish-seed-repos.sh"))
            .arg(root.path())
            .args(repos.map(|r| r.to_string()))
            .output()
            .unwrap()
    };
    let occupied = root.path().join(repos[1].to_string());
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("preserve"), "Existing destination").unwrap();
    assert!(!publish().status.success());
    assert!(!root.path().join(repos[0].to_string()).exists());
    assert_eq!(
        std::fs::read_to_string(occupied.join("preserve")).unwrap(),
        "Existing destination"
    );
    std::fs::remove_file(occupied.join("preserve")).unwrap();
    std::fs::remove_dir(&occupied).unwrap();
    let result = publish();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for (repo, head) in repos.into_iter().zip(heads) {
        let path = root.path().join(repo.to_string());
        assert!(!root.path().join(format!(".seed-ready-{repo}")).exists());
        assert_eq!(run_git(&path, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            std::fs::read_to_string(path.join("README.md")).unwrap(),
            "Retained fixture bytes\n"
        );
    }
}

#[tokio::test]
async fn project_docs_follow_add_modify_rename_delete_and_missing_mirror_without_caches() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&user);
    let ws = TestWorkspace::create(&app, &org).await;
    let repo = attach_docs_repo(&app, &org, &ws).await;
    let root = mirror_path(&app, repo).await;
    let code = mirror_path(&app, ws.repo_id).await;
    let path = format!("docs/{}/plan.md", ws.id);
    let renamed = format!("docs/{}/renamed.md", ws.id);
    let other = format!("docs/{}/foreign.md", Uuid::new_v4());
    let workflow = format!("docs/{}/sync/run-1/review/code_review.md", ws.id);
    let original = "---\nlinked_specs: [ref-root, missing]\n---\n# Original\n";
    commit_with_plain_git(
        &code,
        &ws.name,
        &[
            (
                ".semlift/specs/ref-root.semlift.md",
                "---\ncomponent_id: ref-root\n---\n# Canonical reference\n",
            ),
            (&path, "Wrong repository content"),
        ],
        "canonical source",
    );
    commit_with_plain_git(
        &root,
        &ws.name,
        &[
            (&path, original),
            (&other, "Other workspace"),
            (&workflow, "Generated review report"),
        ],
        "documents without notifications",
    );
    let listing = format!("/api/v1/workspaces/{}/docs", ws.id);
    let pending = format!(
        "/api/v1/workspaces/{}/sync/pending?direction=downlift",
        ws.id
    );
    let read = |path: &str| format!("/api/v1/workspaces/{}/docs/by-path?path={path}", ws.id);
    let data: Value = client.get(&read(&path)).await.json().await.unwrap();
    assert_eq!(data["data"]["content"], original, "{data}");
    assert_eq!(data["data"]["repoId"], repo.to_string());
    assert!(data["data"].get("linkedNodes").is_none());
    assert_eq!(
        data["data"]["linkedComponentIds"].as_array().unwrap().len(),
        1
    );
    let candidates: Value = client.get(&pending).await.json().await.unwrap();
    let candidates = candidates["data"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0]["path"], path);
    assert_eq!(
        candidates[0]["pendingReasons"],
        serde_json::json!([]),
        "missing markers mean unknown history"
    );
    assert!(candidates[0].get("targetNodeId").is_none());
    let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM docs)+(SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','projection_state','doc_node_links'))").fetch_one(&app.db).await.unwrap();
    assert_eq!(
        count, 0,
        "read-driven discovery must write no compatibility rows"
    );
    commit_with_plain_git(
        &root,
        &ws.name,
        &[(&path, "# Modified\n")],
        "modify without notification",
    );
    let changed: Value = client.get(&read(&path)).await.json().await.unwrap();
    assert_eq!(changed["data"]["content"], "# Modified\n");
    run_git(&root, &["mv", &path, &renamed]);
    run_git(&root, &["commit", "-m", "rename"]);
    assert_eq!(
        client.get(&read(&path)).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(client.get(&read(&renamed)).await.status(), StatusCode::OK);
    let listed: Value = client.get(&listing).await.json().await.unwrap();
    assert!(listed["data"]["docs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["filePath"] == renamed));
    // Missing source cannot be interpreted as deletion of retained metadata.
    let metadata = Uuid::new_v4();
    sqlx::query("INSERT INTO docs(id,org_id,workspace_id,repo_id,name,file_path,content) VALUES($1,$2,$3,$4,'Retained draft',$5,'Preserve metadata')").bind(metadata).bind(org.id).bind(ws.id).bind(repo).bind(&renamed).execute(&app.db).await.unwrap();
    let unavailable = root.with_extension("unavailable");
    std::fs::rename(&root, &unavailable).unwrap();
    let unavailable_response = client.get(&listing).await;
    let unavailable_status = unavailable_response.status();
    let unavailable_body: Value = unavailable_response.json().await.unwrap();
    assert!(
        unavailable_status.is_client_error() || unavailable_status.is_server_error(),
        "missing source must fail instead of listing an empty tree: {unavailable_status} {unavailable_body}"
    );
    assert!(unavailable_body["error"]["code"].is_string());
    let retained: String = sqlx::query_scalar("SELECT content FROM docs WHERE id=$1")
        .bind(metadata)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(retained, "Preserve metadata");
    std::fs::rename(&unavailable, &root).unwrap();
    assert_eq!(client.get(&read(&renamed)).await.status(), StatusCode::OK);
    run_git(&root, &["rm", &renamed]);
    run_git(&root, &["commit", "-m", "delete"]);
    assert_eq!(
        client.get(&read(&renamed)).await.status(),
        StatusCode::NOT_FOUND
    );
    let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM docs WHERE id=$1")
        .bind(metadata)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(retained, 1, "reads never delete metadata");
    app.cleanup().await;
}

#[tokio::test]
async fn view_handle_delete_is_owner_scoped_fenced_and_preserves_other_repository_metadata() {
    use helpers::TestWorkspaceRuntimePeer;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let reader = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let client = app.authenticated_client(&owner);
    let ws = TestWorkspace::create(&app, &org).await;
    let docs = attach_docs_repo(&app, &org, &ws).await;
    let path = "views/preserved.pm_view.md";
    let content = "# Preserve this view until the fenced delete succeeds\n";
    for repo in [docs, ws.repo_id] {
        commit_with_plain_git(
            &mirror_path(&app, repo).await,
            &ws.name,
            &[(path, content)],
            "view",
        );
        sqlx::query("INSERT INTO docs(org_id,workspace_id,repo_id,name,doc_type,file_path,content) VALUES($1,$2,$3,'View','pm_view',$4,'Retained metadata')").bind(org.id).bind(ws.id).bind(repo).bind(path).execute(&app.db).await.unwrap();
    }
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &ws, owner.id).await;
    runtime.seed_file(docs, path, content).await;
    runtime
        .seed_file(ws.repo_id, path, "Other repository bytes")
        .await;
    let listed: Value = client
        .get(&format!("/api/v1/workspaces/{}/docs/views", ws.id))
        .await
        .json()
        .await
        .unwrap();
    let list = listed["data"]
        .as_array()
        .or_else(|| listed["data"]["docs"].as_array())
        .expect("view list");
    let handle = list
        .iter()
        .find(|d| d["filePath"] == path)
        .expect("view lists without projection")["id"]
        .as_str()
        .unwrap();
    assert!(handle.starts_with(&format!("overlay:{}:{docs}:", ws.id)));
    let route = |handle: &str| format!("/api/v1/workspaces/{}/docs/{handle}", ws.id);
    let foreign = handle.replacen(&ws.id.to_string(), &Uuid::new_v4().to_string(), 1);
    assert_eq!(
        client.delete(&route(&foreign)).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let wrong_repo = handle.replacen(&docs.to_string(), &ws.repo_id.to_string(), 1);
    assert_eq!(
        client.delete(&route(&wrong_repo)).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        app.authenticated_client(&reader)
            .delete(&route(handle))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    runtime.refuse_writes(docs, path).await;
    assert_eq!(
        client.delete(&route(handle)).await.status(),
        StatusCode::CONFLICT
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM docs WHERE workspace_id=$1")
        .bind(ws.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2, "failed delete preserves both metadata rows");
    runtime.allow_writes(docs, path).await;
    let removed = client.delete(&route(handle)).await;
    let status = removed.status();
    let body = removed.text().await.unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let survivors: Vec<Uuid> = sqlx::query_scalar("SELECT repo_id FROM docs WHERE workspace_id=$1")
        .bind(ws.id)
        .fetch_all(&app.db)
        .await
        .unwrap();
    assert_eq!(survivors, vec![ws.repo_id]);
    let other: Value = client
        .get(&format!(
            "/api/v1/workspaces/{}/files/{path}?repoId={}",
            ws.id, ws.repo_id
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(other["data"]["content"], "Other repository bytes");
    app.cleanup().await;
}
