//! Current ontology uses the same HTTP discovery/read/write boundary as Files.
use crate::helpers::{
    runtime_peer::TestWorkspaceRuntimePeer, TestApp, TestOrg, TestUser, TestWorkspace,
};
use g5_auth::Scope;
use serde_json::{json, Value};
use uuid::Uuid;

const PATH: &str = ".g5/ontology/root.g5.md";
fn listing(ws: Uuid, repo: Uuid) -> String {
    format!("/api/v1/workspaces/{ws}/repos/{repo}/files?path=.g5/ontology&depth=-1")
}
fn file(ws: Uuid, repo: Uuid, path: &str) -> String {
    format!("/api/v1/workspaces/{ws}/files/{path}?repoId={repo}")
}
fn paths(value: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(entries) = value.as_array() {
        for entry in entries {
            if entry["type"] == "file" {
                result.push(entry["path"].as_str().unwrap().to_owned());
            }
            result.extend(paths(&entry["children"]));
        }
    }
    result
}

#[tokio::test]
async fn committed_ontology_files_are_repo_qualified_without_projection_storage() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut ws = TestWorkspace::create(&app, &org).await;
    let other = ws.add_docs_repo(&app).await;
    ws.commit_file(&app, PATH, "# Code ontology").await;
    ws.commit_file_in_repo(&app, other, PATH, "# Docs ontology");
    ws.commit_file(&app, ".g5/ontology/features/malformed.md", "[I · broken]")
        .await;
    ws.commit_file(
        &app,
        ".g5/ontology/features/.g5/runtime/hidden.md",
        "secret",
    )
    .await;
    ws.commit_file(&app, ".g5/plans/hidden.md", "plan").await;
    let client = app.authenticated_client(&reader);
    let data: Value = client
        .get(&listing(ws.id, ws.repo_id))
        .await
        .json()
        .await
        .unwrap();
    let found = paths(&data["data"]["entries"]);
    assert!(found.contains(&PATH.to_owned()));
    assert!(found.contains(&".g5/ontology/features/malformed.md".to_owned()));
    assert!(!found.iter().any(|p| p.contains("hidden")));
    assert_eq!(data["data"]["truncated"], false);
    for (repo, expected) in [(ws.repo_id, "# Code ontology"), (other, "# Docs ontology")] {
        let response = client.get(&file(ws.id, repo, PATH)).await;
        assert_eq!(response.status(), 200);
        let data: Value = response.json().await.unwrap();
        assert_eq!(data["data"]["content"], expected);
        assert_eq!(data["data"]["source"], "committed");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_tables WHERE schemaname='public' AND tablename IN ('ontology_nodes','projection_state','doc_node_links')")
        .fetch_one(&app.db).await.unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        client
            .get(&format!("/api/v1/workspaces/{}/ontology", ws.id))
            .await
            .status(),
        404
    );
    app.cleanup().await;
}

#[tokio::test]
async fn live_ontology_discovery_reads_no_bodies_and_file_save_enforces_hash_and_vm() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let ws = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &ws, owner.id).await;
    let client = app.authenticated_client(&owner);
    let empty: Value = client
        .get(&listing(ws.id, ws.repo_id))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(empty["data"]["entries"], json!([]));
    let hash = runtime.seed_file(ws.repo_id, PATH, "# Live ontology").await;
    let data: Value = client
        .get(&listing(ws.id, ws.repo_id))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(paths(&data["data"]["entries"]), vec![PATH]);
    assert!(!runtime
        .received_rpc_methods()
        .await
        .iter()
        .any(|m| m == "fs.read"));
    let route = file(ws.id, ws.repo_id, PATH);
    let data: Value = client.get(&route).await.json().await.unwrap();
    assert_eq!(data["data"]["source"], "live");
    assert_eq!(data["data"]["content"], "# Live ontology");
    let vm = &data["data"]["vmId"];
    let wrong_vm = client.put_json(&route, &json!({"content":"wrong", "encoding":"utf-8", "expectedHash":hash,"expectedVmId":Uuid::new_v4()})).await;
    assert_eq!(wrong_vm.status(), 409);
    let save = client.put_json(&route, &json!({"content":"# Edited ontology", "encoding":"utf-8", "expectedHash":hash, "expectedVmId":vm})).await;
    assert_eq!(save.status(), 200);
    let stale = client
        .put_json(
            &route,
            &json!({"content":"stale", "encoding":"utf-8", "expectedHash":hash, "expectedVmId":vm}),
        )
        .await;
    assert_eq!(stale.status(), 409);
    let saved: Value = client.get(&route).await.json().await.unwrap();
    assert_eq!(saved["data"]["content"], "# Edited ontology");
    drop(runtime);
    app.cleanup().await;
}
