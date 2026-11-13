//! Real HTTP/Git authority and All changes baseline parity.
use crate::helpers;
use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use serde_json::Value;
use uuid::Uuid;

const PATH: &str = ".g5/ontology/root.g5.md";
fn doc(body: &str) -> String {
    format!("# Root\n\n<a id=\"a\"></a>\n[I · a]\n\n{body}\n\n## Metadata\n\n### a\n\n- human-provided\n")
}
fn url(ws: Uuid, repo: Uuid, path: &str) -> String {
    format!("/api/v1/workspaces/{ws}/source-control/ontology-baseline?repoId={repo}&path={path}")
}
#[tokio::test]
async fn baseline_reads_exact_merge_base_not_child_head_and_scopes_repository() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let docs_repo = child.add_docs_repo(&app).await;
    let original = doc("Original body");
    let base = child.commit_file(&app, PATH, &original).await;
    child.commit_file_in_repo(&app, docs_repo, PATH, &doc("Other repository"));
    let parent = child.attach_parent(&app).await;
    let head = child.commit_file(&app, PATH, &doc("Changed body")).await;
    // Advance the parent independently. Both heads differ from their merge base.
    let repository = git2::Repository::open(app.repo_path(child.repo_id).await).unwrap();
    let base_commit = repository
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    let signature = git2::Signature::now("Test", "test@example.com").unwrap();
    let parent_head = repository
        .commit(
            Some(&format!("refs/heads/parent-{parent}")),
            &signature,
            &signature,
            "Independent parent change",
            &base_commit.tree().unwrap(),
            &[&base_commit],
        )
        .unwrap()
        .to_string();
    let client = app.authenticated_client(&user);
    let response = client.get(&url(child.id, child.repo_id, PATH)).await;
    assert_eq!(response.status(), 200);
    let data: Value = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["baselineCommit"], base);
    assert_eq!(data["sourceCommit"], head);
    assert_eq!(data["parentCommit"], parent_head);
    assert_eq!(data["content"], original);
    let graph = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/change-graph?comparison=workspace",
            child.id
        ))
        .await;
    assert_eq!(graph.status(), 200);
    let graph: Value = graph.json().await.unwrap();
    assert_eq!(graph["data"]["range"]["baseKind"], "merge_base");
    assert_eq!(
        graph["data"]["range"]["parentWorkspaceId"],
        parent.to_string()
    );
    let other = client
        .get(&url(child.id, docs_repo, PATH))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(other["data"]["content"], doc("Other repository"));
    let absent = client
        .get(&url(child.id, child.repo_id, ".g5/ontology/added.g5.md"))
        .await;
    assert_eq!(absent.status(), 200);
    assert!(absent.json::<Value>().await.unwrap()["data"]["content"].is_null());
    assert_eq!(
        client
            .get(&url(child.id, Uuid::new_v4(), PATH))
            .await
            .status(),
        404
    );
    assert_eq!(
        client
            .get(&url(child.id, child.repo_id, ".g5/ontology/../secret.md"))
            .await
            .status(),
        422
    );
    // Parent advances to the child's head: the selected base and token must change.
    helpers::seed_branch_ref(&app, child.repo_id, &format!("parent-{parent}"), &head).await;
    let advanced = client
        .get(&url(child.id, child.repo_id, PATH))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(advanced["data"]["baselineCommit"], head);
    assert_ne!(advanced["data"]["revision"], data["revision"]);
    assert_eq!(advanced["data"]["content"], doc("Changed body"));
}

#[tokio::test]
async fn baseline_unavailable_is_not_an_empty_file_and_requires_read_authority() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let child = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    let standalone = client.get(&url(child.id, child.repo_id, PATH)).await;
    assert!(!standalone.status().is_success());
    assert_eq!(
        standalone.json::<Value>().await.unwrap()["error"]["code"],
        "DIFF_GRAPH_UNAVAILABLE"
    );
    let base = TestWorkspace::create_base(&app, &org).await;
    assert!(!client
        .get(&url(base.id, base.repo_id, PATH))
        .await
        .status()
        .is_success());
    let stranger_org = TestOrg::create(&app).await;
    let stranger =
        TestUser::create_with_scopes(&app, stranger_org.id, &[Scope::WorkspacesRead]).await;
    assert!(!app
        .authenticated_client(&stranger)
        .get(&url(child.id, child.repo_id, PATH))
        .await
        .status()
        .is_success());
    let response = app
        .client
        .get(format!(
            "{}{}",
            app.base_url,
            url(child.id, child.repo_id, PATH)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let parent = child.attach_parent(&app).await;
    let repository = git2::Repository::open(app.repo_path(child.repo_id).await).unwrap();
    let parent_ref = format!("refs/heads/parent-{parent}");
    repository
        .find_reference(&parent_ref)
        .unwrap()
        .delete()
        .unwrap();
    assert!(!client
        .get(&url(child.id, child.repo_id, PATH))
        .await
        .status()
        .is_success());
    helpers::seed_branch_ref(
        &app,
        child.repo_id,
        &format!("parent-{parent}"),
        &child.head_commit,
    )
    .await;
    let private_owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(parent)
        .bind(private_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(client
        .get(&url(child.id, child.repo_id, PATH))
        .await
        .status()
        .is_success());
    // A membership mismatch must not fall back to this repository's head.
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2")
        .bind(parent)
        .bind(child.repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(!client
        .get(&url(child.id, child.repo_id, PATH))
        .await
        .status()
        .is_success());
}

#[tokio::test]
async fn baseline_refuses_oversized_blobs_and_unrelated_history() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(&app, PATH, &"x".repeat(10 * 1024 * 1024 + 1))
        .await;
    let parent = child.attach_parent(&app).await;
    let client = app.authenticated_client(&user);
    let oversized = client.get(&url(child.id, child.repo_id, PATH)).await;
    assert!(!oversized.status().is_success());
    let repository = git2::Repository::open(app.repo_path(child.repo_id).await).unwrap();
    let current = repository
        .find_commit(git2::Oid::from_str(&child.head_commit).unwrap())
        .unwrap();
    let signature = git2::Signature::now("Test", "test@example.com").unwrap();
    let unrelated = repository
        .commit(
            None,
            &signature,
            &signature,
            "Unrelated root",
            &current.tree().unwrap(),
            &[],
        )
        .unwrap();
    repository
        .reference(
            &format!("refs/heads/parent-{parent}"),
            unrelated,
            true,
            "test unrelated history",
        )
        .unwrap();
    // A missing path must still fail when there is no merge base; not Added.
    let unavailable = client
        .get(&url(child.id, child.repo_id, ".g5/ontology/absent.g5.md"))
        .await;
    assert!(!unavailable.status().is_success());
}

#[tokio::test]
async fn overview_includes_committed_and_live_files_and_preserves_deleted_file_baselines() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let docs_repo = child.add_docs_repo(&app).await;
    child.commit_file(&app, PATH, &doc("Original")).await;
    let removed_path = ".g5/ontology/removed.g5.md";
    child
        .commit_file(&app, removed_path, &doc("Deleted file"))
        .await;
    child.commit_file_in_repo(&app, docs_repo, PATH, &doc("Docs original"));
    child.attach_parent(&app).await;
    child.commit_file(&app, PATH, &doc("Committed edit")).await;
    child.commit_file_in_repo(&app, docs_repo, PATH, &doc("Docs changed"));
    let client = app.authenticated_client(&user);
    let endpoint = format!(
        "/api/v1/workspaces/{}/source-control/ontology-changes",
        child.id
    );
    let response = client.get(&endpoint).await;
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    let committed: Value = client.get(&endpoint).await.json().await.unwrap();
    let files = committed["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let file = files
        .iter()
        .find(|f| f["repoId"] == child.repo_id.to_string())
        .unwrap();
    assert_eq!(file["baseline"], doc("Original"));
    assert_eq!(file["current"], doc("Committed edit"));
    assert!(file.get("nodeId").is_none());

    // The runtime is a transport double; the HTTP router, database authority,
    // Git baselines, Files source selection and overview assembly are real.
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &child, user.id).await;
    runtime
        .seed_file(child.repo_id, PATH, &doc("Uncommitted edit"))
        .await;
    runtime
        .seed_file(docs_repo, PATH, &doc("Docs changed"))
        .await;
    let added_path = ".g5/ontology/added.g5.md";
    runtime
        .seed_file(child.repo_id, added_path, &doc("Uncommitted addition"))
        .await;
    let response = client.get(&endpoint).await;
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    let live: Value = client.get(&endpoint).await.json().await.unwrap();
    let files = live["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 4, "{live}");
    let file = files
        .iter()
        .find(|f| f["repoId"] == child.repo_id.to_string() && f["path"] == PATH)
        .unwrap();
    assert_eq!(file["current"], doc("Uncommitted edit"));
    assert_eq!(file["source"], "live");
    let removed = files.iter().find(|f| f["path"] == removed_path).unwrap();
    assert!(removed["current"].is_null());
    assert!(removed.get("nodeId").is_none());
    assert_eq!(removed["baseline"], doc("Deleted file"));
    let added = files.iter().find(|f| f["path"] == added_path).unwrap();
    assert!(added["baseline"].is_null());
    assert_eq!(added["current"], doc("Uncommitted addition"));
    assert_ne!(live["data"]["revision"], committed["data"]["revision"]);
    assert!(!runtime
        .received_rpc_methods()
        .await
        .iter()
        .any(|m| m == "fs.write"));
    runtime.refuse_reads(child.repo_id, PATH).await;
    assert!(!client.get(&endpoint).await.status().is_success());
    app.cleanup().await;
}

#[tokio::test]
async fn overview_accepts_live_repositories_without_an_ontology_directory() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child.add_docs_repo(&app).await;
    child.commit_file(&app, PATH, &doc("Original")).await;
    child.attach_parent(&app).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&app, &child, user.id).await;
    runtime
        .seed_file(child.repo_id, PATH, &doc("Live edit"))
        .await;
    let response = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/ontology-changes",
            child.id
        ))
        .await;
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    let data: Value = response.json().await.unwrap();
    assert_eq!(data["data"]["repositories"].as_array().unwrap().len(), 1);
    assert_eq!(data["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(data["data"]["files"][0]["current"], doc("Live edit"));
    app.cleanup().await;
}

#[tokio::test]
async fn overview_refuses_missing_parent_denied_authority_and_oversize_content() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    let endpoint = format!(
        "/api/v1/workspaces/{}/source-control/ontology-changes",
        child.id
    );
    assert!(!client.get(&endpoint).await.status().is_success());
    child.attach_parent(&app).await;
    let empty: Value = client.get(&endpoint).await.json().await.unwrap();
    assert_eq!(empty["data"]["files"], serde_json::json!([]));
    let without_read_scope =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    assert_eq!(
        app.authenticated_client(&without_read_scope)
            .get(&endpoint)
            .await
            .status(),
        reqwest::StatusCode::FORBIDDEN,
        "ontology overview requires the read scope even for a workspace writer"
    );
    let stranger_org = TestOrg::create(&app).await;
    let stranger =
        TestUser::create_with_scopes(&app, stranger_org.id, &[Scope::WorkspacesRead]).await;
    assert!(!app
        .authenticated_client(&stranger)
        .get(&endpoint)
        .await
        .status()
        .is_success());
    child
        .commit_file(&app, PATH, &"x".repeat(10 * 1024 * 1024 + 1))
        .await;
    let response = client.get(&endpoint).await;
    assert!(!response.status().is_success());
    app.cleanup().await;
}

#[tokio::test]
async fn overview_refuses_a_parent_revision_change_during_live_reads() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child.commit_file(&app, PATH, &doc("Original")).await;
    let parent = child.attach_parent(&app).await;
    let head = child.commit_file(&app, PATH, &doc("Committed edit")).await;
    let runtime =
        helpers::TestWorkspaceRuntimePeer::attach_with_read_barrier(&app, &child, user.id, PATH, 2)
            .await;
    runtime
        .seed_file(child.repo_id, PATH, &doc("Live edit"))
        .await;
    let client = app.authenticated_client(&user);
    let endpoint = format!(
        "/api/v1/workspaces/{}/source-control/ontology-changes",
        child.id
    );
    let request = client.get(&endpoint);
    let advance = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !runtime
                .received_rpc_methods()
                .await
                .iter()
                .any(|method| method == "fs.read")
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("overview reached captured live read");
        helpers::seed_branch_ref(&app, child.repo_id, &format!("parent-{parent}"), &head).await;
        // The second read releases the transport barrier after moving the parent.
        assert!(client
            .get(&format!(
                "/api/v1/workspaces/{}/files/{PATH}?repoId={}",
                child.id, child.repo_id
            ))
            .await
            .status()
            .is_success());
    };
    let (response, ()) = tokio::join!(request, advance);
    assert!(!response.status().is_success());
    let body: Value = response.json().await.unwrap();
    assert!(
        body["error"]["details"]["message"]
            .as_str()
            .unwrap()
            .contains("revisions changed"),
        "{body}"
    );
    app.cleanup().await;
}
