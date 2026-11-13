//! Workbench configuration (workbench `g5.workbench.actions`,
//! `g5.workbench.persona`): `GET /api/v1/workspaces/:id/workbench` serves the
//! built-in ribbon actions and personas merged with the committed
//! `.g5/workbench/actions.yaml` and `personas/<phase>.md` of the workspace's
//! docs branch, read from one committed snapshot. A committed file replaces
//! the phases it names; an invalid file returns the built-ins with the error.
//! `.g5/workbench/` is durable repository content: it shows in the Files tree
//! and a fork inherits it with the copied branch.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints. No mocks.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

const IE_OVERRIDE: &str = "ie:\n  - name: Release\n    actions:\n      - id: merge\n        label: Ship it\n      - id: approval\n";

async fn get_workbench(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Value {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/workbench"))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

fn action_ids(groups: &Value) -> Vec<String> {
    groups
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["actions"].as_array().expect("actions").iter())
        .map(|action| action["id"].as_str().expect("id").to_string())
        .collect()
}

fn group_names(groups: &Value) -> Vec<String> {
    groups
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| group["name"].as_str().expect("name").to_string())
        .collect()
}

#[tokio::test]
async fn builtins_are_served_when_nothing_is_committed() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    let config = get_workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("builtin"));
    assert!(config["config"].get("error").is_none());
    assert!(
        config["revision"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{}.", workspace.head_commit)),
        "revision names the committed head: {}",
        config["revision"]
    );
    assert_eq!(
        group_names(&config["actions"]["be"]),
        ["Author", "Analysis", "Mock", "Alignment"]
    );
    assert_eq!(
        group_names(&config["actions"]["se"]),
        ["Author", "Analysis", "Design", "Build", "Land"]
    );
    assert_eq!(
        action_ids(&config["actions"]["ie"]),
        [
            "new-doc",
            "import",
            "review-meeting",
            "extract-decisions",
            "approval",
            "rerun-verification",
            "create-pr",
            "get-approvals",
            "merge"
        ]
    );
    // Menus carry their items; chat actions carry complete prompts.
    let import = &config["actions"]["be"][0]["actions"][1];
    assert_eq!(import["kind"], json!("menu"));
    assert_eq!(import["items"][1]["kind"], json!("chat"));
    assert!(import["items"][1]["prompt"]
        .as_str()
        .unwrap()
        .contains(".g5/ontology/"));
    for phase in ["be", "se", "ie"] {
        assert_eq!(config["personas"][phase]["source"], json!("builtin"));
        assert!(config["personas"][phase]["text"]
            .as_str()
            .unwrap()
            .contains("AGENTS.md"));
    }

    app.cleanup().await;
}

#[tokio::test]
async fn a_committed_file_replaces_the_phases_it_names_and_inherits_by_id() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    let before = get_workbench(&client, workspace.id).await;

    let head = workspace
        .commit_file(&app, ".g5/workbench/actions.yaml", IE_OVERRIDE)
        .await;
    let config = get_workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("committed"), "{config}");
    assert_eq!(action_ids(&config["actions"]["ie"]), ["merge", "approval"]);
    let merge = &config["actions"]["ie"][0]["actions"][0];
    assert_eq!(merge["label"], json!("Ship it"));
    // Omitted behaviour fields come from the built-in action with that id.
    assert_eq!(merge["kind"], json!("open"));
    assert_eq!(merge["target"], json!("merge"));
    assert_eq!(merge["primary"], json!(true));
    // Phases the file does not name keep the built-ins.
    assert_eq!(config["actions"]["be"], before["actions"]["be"]);
    assert_eq!(config["actions"]["se"], before["actions"]["se"]);
    assert_eq!(config["personas"], before["personas"]);
    // The revision follows the committed head.
    assert_ne!(config["revision"], before["revision"]);
    assert!(config["revision"]
        .as_str()
        .unwrap()
        .starts_with(&format!("{head}.")));

    // Re-committing changes the revision again.
    let head = workspace
        .commit_file(
            &app,
            ".g5/workbench/actions.yaml",
            &IE_OVERRIDE.replace("Ship it", "Land it"),
        )
        .await;
    let again = get_workbench(&client, workspace.id).await;
    assert!(again["revision"]
        .as_str()
        .unwrap()
        .starts_with(&format!("{head}.")));
    assert_eq!(
        again["actions"]["ie"][0]["actions"][0]["label"],
        json!("Land it")
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_committed_persona_replaces_that_phase_only() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    workspace
        .commit_file(
            &app,
            ".g5/workbench/personas/se.md",
            "# Our architect\n\nDesign small. Test everything.\n",
        )
        .await;
    let config = get_workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("committed"), "{config}");
    assert_eq!(config["personas"]["se"]["source"], json!("committed"));
    assert_eq!(
        config["personas"]["se"]["text"],
        json!("# Our architect\n\nDesign small. Test everything.")
    );
    assert_eq!(config["personas"]["be"]["source"], json!("builtin"));
    assert_eq!(config["personas"]["ie"]["source"], json!("builtin"));

    app.cleanup().await;
}

#[tokio::test]
async fn an_invalid_file_returns_the_builtins_with_a_precise_error() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    let builtin = get_workbench(&client, workspace.id).await;

    let cases: [(&str, &str); 6] = [
        ("ie: [\n", ".g5/workbench/actions.yaml"),
        (
            "ie:\n  - name: R\n    actions:\n      - id: a\n        label: A\n        kind: chat\n        prompt: p\n      - id: a\n        label: B\n        kind: chat\n        prompt: q\n",
            "duplicate action id `a`",
        ),
        (
            "ie:\n  - name: R\n    actions:\n      - id: a\n        label: A\n        kind: open\n        target: elsewhere\n",
            "unknown variant `elsewhere`",
        ),
        (
            "qa:\n  - name: R\n    actions: []\n",
            "unknown phase `qa`",
        ),
        (
            "ie:\n  - name: R\n    actions:\n      - id: fresh\n        label: A\n        prompt: p\n",
            "missing required field `kind`",
        ),
        (
            "ie:\n  - name: R\n    actions:\n      - id: a\n        label: A\n        kind: chat\n        prompt: p\n        persona: qa\n",
            "unknown variant `qa`",
        ),
    ];
    for (yaml, expected) in cases {
        workspace
            .commit_file(&app, ".g5/workbench/actions.yaml", yaml)
            .await;
        let config = get_workbench(&client, workspace.id).await;
        assert_eq!(config["config"]["status"], json!("invalid"), "{yaml}");
        let error = config["config"]["error"].as_str().unwrap_or_default();
        assert!(error.contains(expected), "{yaml}: {error}");
        // Never a partial merge: the built-ins come back whole.
        assert_eq!(config["actions"], builtin["actions"], "{yaml}");
        assert_eq!(config["personas"], builtin["personas"], "{yaml}");
    }

    // A blank persona file is invalid too.
    workspace
        .commit_file(&app, ".g5/workbench/actions.yaml", IE_OVERRIDE)
        .await;
    workspace
        .commit_file(&app, ".g5/workbench/personas/be.md", "\n\n")
        .await;
    let config = get_workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("invalid"));
    assert!(config["config"]["error"]
        .as_str()
        .unwrap()
        .contains("personas/be.md: persona file is empty"));
    assert_eq!(config["actions"], builtin["actions"]);

    app.cleanup().await;
}

#[tokio::test]
async fn the_configuration_is_read_from_the_docs_repository_and_never_leaks() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);

    // A code-repository file is ignored once a docs repository exists.
    workspace
        .commit_file(
            &app,
            ".g5/workbench/actions.yaml",
            &IE_OVERRIDE.replace("Ship it", "From code repo"),
        )
        .await;
    let docs_repo = workspace.add_docs_repo(&app).await;
    let docs_head =
        workspace.commit_file_in_repo(&app, docs_repo, ".g5/workbench/actions.yaml", IE_OVERRIDE);
    let config = get_workbench(&client, workspace.id).await;
    assert_eq!(config["config"]["status"], json!("committed"), "{config}");
    assert_eq!(
        config["actions"]["ie"][0]["actions"][0]["label"],
        json!("Ship it"),
        "the docs repository wins over the code repository"
    );
    assert!(config["revision"]
        .as_str()
        .unwrap()
        .starts_with(&format!("{docs_head}.")));

    // Another workspace in the same org sees only its own (absent) files.
    let other_config = get_workbench(&client, other.id).await;
    assert_eq!(other_config["config"]["status"], json!("builtin"));
    assert_eq!(
        action_ids(&other_config["actions"]["ie"]),
        [
            "new-doc",
            "import",
            "review-meeting",
            "extract-decisions",
            "approval",
            "rerun-verification",
            "create-pr",
            "get-approvals",
            "merge"
        ]
    );

    // A user outside the organization cannot read it at all.
    let outsider_org = TestOrg::create(&app).await;
    let outsider =
        TestUser::create_with_scopes(&app, outsider_org.id, &[Scope::WorkspacesRead]).await;
    let response = app
        .authenticated_client(&outsider)
        .get(&format!("/api/v1/workspaces/{}/workbench", workspace.id))
        .await;
    assert!(
        matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "outsider got {}",
        response.status()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn committed_workbench_files_show_in_the_files_tree_and_a_fork_inherits_them() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let mut workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    workspace
        .commit_file(&app, ".g5/workbench/actions.yaml", IE_OVERRIDE)
        .await;
    workspace
        .commit_file(&app, ".g5/workbench/personas/ie.md", "# Release captain\n")
        .await;
    // Runtime sidecars beside them stay hidden.
    workspace
        .commit_file(&app, ".g5/workbench/notes.txt", "scratch\n")
        .await;

    // Files tree (durable `.g5` allow-list, `is_files_tree_visible_path`).
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files?depth=-1",
            workspace.id, workspace.repo_id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("tree JSON");
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut paths = Vec::new();
    collect_paths(
        body["data"]["entries"].as_array().expect("entries"),
        &mut paths,
    );
    assert!(
        paths.iter().any(|p| p == ".g5/workbench/actions.yaml"),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == ".g5/workbench/personas/ie.md"),
        "{paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p == ".g5/workbench/notes.txt"),
        "{paths:?}"
    );

    // The committed file is readable through the ordinary file read.
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{}/repos/{}/files/.g5/workbench/actions.yaml",
            workspace.id, workspace.repo_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    // A fork copies the branch, so it serves the same committed configuration.
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/clone", workspace.id),
            &json!({ "name": "workbench-fork", "idempotencyKey": "workbench-fork-key" }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let fork_id = Uuid::parse_str(body["data"]["workspace"]["id"].as_str().expect("fork id"))
        .expect("fork id is a uuid");
    let fork_config = get_workbench(&client, fork_id).await;
    assert_eq!(
        fork_config["config"]["status"],
        json!("committed"),
        "{fork_config}"
    );
    assert_eq!(
        action_ids(&fork_config["actions"]["ie"]),
        ["merge", "approval"]
    );
    assert_eq!(fork_config["personas"]["ie"]["source"], json!("committed"));
    assert_eq!(
        fork_config["personas"]["ie"]["text"],
        json!("# Release captain")
    );

    app.cleanup().await;
}

fn collect_paths(entries: &[Value], out: &mut Vec<String>) {
    for entry in entries {
        out.push(entry["path"].as_str().expect("path").to_string());
        if let Some(children) = entry["children"].as_array() {
            collect_paths(children, out);
        }
    }
}
