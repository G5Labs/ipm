//! Black-box PR CLI transport, selection and failure behavior.
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cli(home: &TempDir, api: &str, workspace: Uuid) -> Command {
    Command::from_std(cli_process(home, api, workspace))
}
fn cli_process(home: &TempDir, api: &str, workspace: Uuid) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_g5d"));
    // Inside a workspace VM the agent's shell exports its own G5 context: the
    // VM's checkouts (create and update push them) and a token file that wins
    // over the test's G5D_WORKSPACE_TOKEN. Start from none of it.
    for (key, _) in std::env::vars_os() {
        if key
            .to_str()
            .is_some_and(|key| key.starts_with("G5D_") || key.starts_with("G5_"))
        {
            command.env_remove(key);
        }
    }
    command
        .current_dir(home.path())
        .env("G5_HOME", home.path())
        .env("PWD", home.path())
        .env("G5D_API_URL", api)
        .env("G5D_WORKSPACE_ID", workspace.to_string())
        .env("G5D_WORKSPACE_TOKEN", "current-incarnation");
    command
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vm_pr_commands_use_internal_reads_and_produce_json_and_readable_output() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let pr = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests/info");
    Mock::given(method("GET")).and(path(&endpoint)).and(header("authorization","Bearer current-incarnation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"g5","pullRequests":[
            {"id":pr,"sourceWorkspaceId":workspace,"sourceWorkspaceName":"feature","targetWorkspaceName":"main","title":"Fix bug","status":"open","localMergeStatus":"ready","providerStatus":"open","createdAt":"2026-09-30"}
        ]}}))).expect(6).mount(&server).await;
    // The description is the server's canonical read (`g5.git.pr-description`),
    // here the PR-body Project Doc, not the stored `descriptionOverride`.
    Mock::given(method("GET"))
        .and(path(format!("/api/internal/workspaces/{workspace}/pull-requests/{pr}/description")))
        .and(header("authorization", "Bearer current-incarnation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "pullRequestId":pr,"description":"Body from the Project Doc","descriptionSource":"projectDoc",
            "descriptionHtml":null,"repositories":[
                {"repoId":Uuid::new_v4(),"repository":"acme/api","number":42,"comparison":"differs"},
                {"repoId":Uuid::new_v4(),"repository":"acme/web","number":7,"comparison":"unavailable","error":"GitHub could not be read"}
            ]}})))
        .expect(2)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "list", "--json", "id,title,state,source"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        value,
        json!([{"id":pr,"title":"Fix bug","state":"OPEN","source":"g5"}])
    );
    let view = cli(&home, &server.uri(), workspace)
        .args(["pr", "view"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let view = String::from_utf8(view).unwrap();
    assert!(view.contains("feature → main"), "{view}");
    assert!(view.contains("\nBody from the Project Doc\n"), "{view}");
    assert!(
        view.contains("GitHub description differs: acme/api #42"),
        "{view}"
    );
    assert!(
        view.contains("could not be compared: acme/web #7 (GitHub could not be read)"),
        "{view}"
    );
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "--json", "id,body"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value, json!({"id":pr,"body":"Body from the Project Doc"}));
    // Fields without `body` need no description read (the mock expects two).
    cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "--json", "id,state"])
        .assert()
        .success();
    cli(&home, &server.uri(), workspace)
        .args(["pr", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("stored state"));
    // A stored listing reads every shown PR's description in one batch.
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/descriptions"
        )))
        .and(query_param("ids", pr.to_string()))
        .and(header("authorization", "Bearer current-incarnation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[
            {"pullRequestId":pr,"description":"Body from the Project Doc","descriptionSource":"projectDoc"}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "list", "--json", "id,body"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"id":pr,"body":"Body from the Project Doc"}])
    );
    server.verify().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn github_selectors_are_encoded_and_permission_errors_fail_the_command() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests/info");
    Mock::given(method("GET")).and(path(&endpoint)).and(query_param("selector","https://github.com/acme/api/pull/42"))
        .and(query_param("repo","acme/api")).and(query_param("comments","true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"github","pullRequest":{"number":42,"title":"Fix bug","state":"OPEN"}}})))
        .expect(1).mount(&server).await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "view",
            "https://github.com/acme/api/pull/42",
            "--repo",
            "acme/api",
            "--comments",
            "--json",
            "number,title,state,source",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["source"], "github");
    assert_eq!(value["number"], 42);
    assert!(value.get("pullRequest").is_none());
    Mock::given(method("GET"))
        .and(path(&endpoint))
        .and(query_param("selector", "1"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"error":{"code":"FORBIDDEN","message":"Expired VM lease"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "1", "--json", "title"])
        .assert()
        .failure()
        .stdout("");
    cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "--comments"])
        .assert()
        .failure();
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_in_user_can_select_a_workspace_without_a_checkout() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/workspaces/{workspace}/pull-requests/info"
        )))
        .and(header("authorization", "Bearer user-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"source":"g5","pullRequests":[]}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .env_remove("G5D_WORKSPACE_TOKEN")
        .env_remove("G5D_WORKSPACE_ID")
        .env("G5D_TOKEN", "user-token")
        .args([
            "pr",
            "list",
            "--workspace-id",
            &workspace.to_string(),
            "--json=title",
        ])
        .assert()
        .success()
        .stdout("[]\n");
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_checkout_never_silently_selects_the_ambient_workspace() {
    let home = TempDir::new().unwrap();
    let checkout = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let ambient = Uuid::new_v4();
    let explicit = Uuid::new_v4();
    std::fs::create_dir(checkout.path().join(".g5")).unwrap();
    std::fs::write(
        checkout.path().join(".g5/workspace.yaml"),
        format!("workspace_id: {explicit}\n"),
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{explicit}/pull-requests/info"
        )))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            json!({"error":{"code":"FORBIDDEN","message":"Token belongs to another workspace"}}),
        ))
        .expect(2)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), ambient)
        .args([
            "--workspace",
            checkout.path().to_str().unwrap(),
            "pr",
            "list",
        ])
        .assert()
        .failure();
    // Stable metadata must not defeat the explicit classic checkout either.
    std::fs::create_dir(home.path().join(".g5")).unwrap();
    std::fs::write(
        home.path().join(".g5/provided-agent-chat-workspace.json"),
        serde_json::to_vec(&json!({
            "schemaVersion":1,"kind":"provided-agent-stable-chat-cwd","sessionId":Uuid::new_v4(),
            "workspaceId":ambient,"agentProvider":"codex","createdByTaskId":Uuid::new_v4()
        }))
        .unwrap(),
    )
    .unwrap();
    cli(&home, &server.uri(), ambient)
        .args([
            "--workspace",
            checkout.path().to_str().unwrap(),
            "pr",
            "list",
        ])
        .assert()
        .failure();
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cwd_config_and_stable_metadata_discover_the_intended_workspace() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    std::fs::create_dir(home.path().join(".g5")).unwrap();
    std::fs::write(
        home.path().join(".g5/workspace.yaml"),
        format!("workspace_id: {workspace}\n"),
    )
    .unwrap();
    for prefix in ["v1", "internal"] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/{prefix}/workspaces/{workspace}/pull-requests/info"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":{"source":"g5","pullRequests":[]}})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    cli(&home, &server.uri(), Uuid::new_v4())
        .env_remove("G5D_WORKSPACE_ID")
        .env_remove("G5D_WORKSPACE_TOKEN")
        .env("G5D_TOKEN", "user-token")
        .args(["pr", "list"])
        .assert()
        .success();
    std::fs::write(
        home.path().join(".g5/provided-agent-chat-workspace.json"),
        serde_json::to_vec(&json!({
            "schemaVersion":1,"kind":"provided-agent-stable-chat-cwd","sessionId":Uuid::new_v4(),
            "workspaceId":workspace,"agentProvider":"codex","createdByTaskId":Uuid::new_v4()
        }))
        .unwrap(),
    )
    .unwrap();
    cli(&home, &server.uri(), Uuid::new_v4())
        .args(["pr", "list"])
        .assert()
        .success();
    server.verify().await;
}

#[test]
fn json_field_help_and_validation_need_no_workspace_or_credentials() {
    let home = TempDir::new().unwrap();
    // Corrupt metadata would fail discovery if this went through workspace setup.
    std::fs::create_dir(home.path().join(".g5")).unwrap();
    std::fs::write(
        home.path().join(".g5/provided-agent-chat-workspace.json"),
        "invalid",
    )
    .unwrap();
    for args in [
        vec!["pr", "view", "42", "--json"],
        vec!["--json", "pr", "view", "42"],
    ] {
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .env_remove("G5D_WORKSPACE_TOKEN")
            .env_remove("G5D_WORKSPACE_ID")
            .args(args)
            .assert()
            .success()
            .stdout(predicates::str::contains("Available fields:\n"))
            .stdout(predicates::str::contains("mergeable"));
    }
    cli(&home, "http://127.0.0.1:1", Uuid::nil())
        .env_remove("G5D_WORKSPACE_TOKEN")
        .env_remove("G5D_WORKSPACE_ID")
        .args(["pr", "view", "42", "--json="])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicates::str::contains("Unknown JSON field"));
    for fields in ["", "typo", "title,,state", "title,"] {
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .env_remove("G5D_WORKSPACE_TOKEN")
            .env_remove("G5D_WORKSPACE_ID")
            .args(["pr", "list", "--json", fields])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicates::str::contains("Unknown JSON field"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_threads_are_fetched_for_json_and_rendered_with_comments() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests/info");
    let threads = json!([{"path":"src/lib.rs","line":3,"isResolved":false,"isOutdated":true,
        "comments":[{"author":{"login":"coderabbitai"},"createdAt":"T1","body":"Major: check bounds"}]}]);
    let latest = json!([{"id":"PRR_1","author":{"login":"claude"},"state":"CHANGES_REQUESTED","commit":{"oid":"abc"}}]);
    Mock::given(method("GET")).and(path(&endpoint)).and(query_param("selector", "42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"github","pullRequest":{
            "number":42,"title":"Fix","state":"OPEN","comments":[],"commentsHasMore":false,
            "reviews":[{"author":{"login":"claude"},"state":"CHANGES_REQUESTED","body":"See threads"}],"reviewsHasMore":false,
            "reviewThreads":threads,"reviewThreadsHasMore":false,
            "reviewDecision":"CHANGES_REQUESTED","latestReviews":latest,"latestReviewsHasMore":false
        }}}))).expect(3).mount(&server).await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "view",
            "42",
            "--json",
            "reviewThreads,reviewThreadsHasMore",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!({"reviewThreads":threads,"reviewThreadsHasMore":false})
    );
    cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "42", "-c"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Reviews (1)"))
        .stdout(predicates::str::contains(
            "Review threads (1; 1 unresolved)\n\nsrc/lib.rs:3 · unresolved · outdated\ncoderabbitai · T1\nMajor: check bounds",
        ))
        .stdout(predicates::str::contains(
            "Review decision: CHANGES_REQUESTED\nLatest reviews: claude (CHANGES_REQUESTED)",
        ));
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "42", "--json", "reviewDecision,latestReviews"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!({"reviewDecision":"CHANGES_REQUESTED","latestReviews":latest})
    );
    // Thread fields request comments, -c requests both, and the review summary
    // alone requests only reviews.
    let flags: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            let has = |key: &str| r.url.query_pairs().any(|(k, v)| k == key && v == "true");
            (has("comments"), has("reviews"))
        })
        .collect();
    assert_eq!(flags, [(true, false), (true, true), (false, true)]);
    server.verify().await;
}

#[test]
fn gh_output_flags_explain_jq_and_help_lists_exit_codes() {
    let home = TempDir::new().unwrap();
    cli(&home, "http://127.0.0.1:1", Uuid::nil())
        .args(["pr", "view", "42", "--json", "title", "-q", ".title"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains("unexpected argument"))
        .stderr(predicates::str::contains("| jq -r .title"));
    for command in [&["pr", "--help"][..], &["pr", "checks", "--help"]] {
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .args(command)
            .assert()
            .success()
            .stdout(predicates::str::contains("Exit codes"))
            .stdout(predicates::str::contains("5    create/update: G5 PR saved"))
            .stdout(predicates::str::contains("7    comment: outcome unclear"))
            .stdout(predicates::str::contains("8    Checks pending"));
    }
    cli(&home, "http://127.0.0.1:1", Uuid::nil())
        .args(["pr", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("checks --watch --head"))
        .stdout(predicates::str::contains("waits out GitHub rate limits"))
        .stdout(predicates::str::contains("provider/g5/<branch>"));
    cli(&home, "http://127.0.0.1:1", Uuid::nil())
        .args(["pr", "checks", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("only for failed or"))
        .stdout(predicates::str::contains("Seconds between polls"))
        .stdout(predicates::str::contains("--timeout <MINUTES>"))
        .stdout(predicates::str::contains("merged or closed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn json_comments_are_fetched_and_status_is_a_plain_array() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests/info");
    Mock::given(method("GET")).and(path(&endpoint)).and(query_param("selector", "42"))
        .and(query_param("comments", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"github","pullRequest":{
            "number":42,"mergeable":false,"mergeStateStatus":"dirty", "comments":[{"author":{"login":"dev"},"body":"Review"}],"commentsHasMore":true
        }}}))).expect(1).mount(&server).await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "view",
            "--json=number,mergeable,mergeStateStatus,comments,commentsHasMore",
            "42",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!({
            "number":42,"mergeable":"CONFLICTING","mergeStateStatus":"DIRTY",
            "comments":[{"author":{"login":"dev"},"body":"Review"}],"commentsHasMore":true
        })
    );
    Mock::given(method("GET"))
        .and(path(&endpoint))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data":{"source":"g5","pullRequests":[
                {"title":"Open","status":"open"}, {"title":"Closed","status":"closed"}
            ]}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "status", "--json", "title,state"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"title":"Open","state":"OPEN"}])
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_and_update_use_the_ui_request_shape_and_project_the_same_result() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let target = Uuid::new_v4();
    let pr = Uuid::new_v4();
    for (prefix, bearer, count) in [
        ("internal", "current-incarnation", 3),
        ("v1", "user-token", 1),
    ] {
        Mock::given(method("POST")).and(path(format!("/api/{prefix}/workspaces/{workspace}/pull-requests")))
            .and(header("authorization", format!("Bearer {bearer}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
                "pullRequest":{"id":pr,"title":"PR title","status":"open","providerStatus":"pending"},"alreadyExisted":true,"dirtyDrafts":[],
                "publication":{"complete":true,"repositories":[]}
            }}))).expect(count).mount(&server).await;
    }
    // A write's text output shows the description through the same read as view.
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/workspaces/{workspace}/pull-requests/{pr}/description"
        )))
        .and(header("authorization", "Bearer user-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "pullRequestId":pr,"description":"Stdin description","descriptionSource":"saved",
            "descriptionHtml":null,"repositories":[]}})))
        .expect(1)
        .mount(&server)
        .await;
    let body_file = home.path().join("body.md");
    std::fs::write(&body_file, "File description\n").unwrap();
    for (verb, body_args) in [
        ("create", vec!["--body", "Inline description"]),
        ("update", vec!["--body-file", body_file.to_str().unwrap()]),
    ] {
        let out = cli(&home, &server.uri(), workspace)
            .args([
                "pr",
                verb,
                "--target",
                &target.to_string(),
                "--json",
                "id,title,state",
            ])
            .args(body_args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            serde_json::from_slice::<Value>(&out).unwrap(),
            json!({"id":pr,"title":"PR title","state":"OPEN"})
        );
    }
    // The internal description read is not mounted: the saved edit still
    // succeeds, and the failed read only warns.
    cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "update",
            "--target",
            &target.to_string(),
            "--body",
            "Edited",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Edited G5 pull request"))
        .stderr(predicates::str::contains(
            "The PR's description could not be read",
        ));
    cli(&home, &server.uri(), workspace)
        .env_remove("G5D_WORKSPACE_TOKEN")
        .env("G5D_TOKEN", "user-token")
        .args([
            "pr",
            "create",
            "--target-workspace-id",
            &target.to_string(),
            "--body-file",
            "-",
        ])
        .write_stdin("Stdin description\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("Updated G5 pull request"))
        .stdout(predicates::str::contains("\nStdin description\n"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method.as_str() == "POST")
            .map(|r| r.body_json::<Value>().unwrap())
            .collect::<Vec<_>>(),
        vec![
            json!({"targetWorkspaceId":target,"description":"Inline description","mode":"create","replace":false,"save":false}),
            // `update` with new text is `edit` (`g5.git.pr-cli.update`).
            json!({"targetWorkspaceId":target,"description":"File description\n","mode":"edit","replace":false,"save":false}),
            json!({"targetWorkspaceId":target,"description":"Edited","mode":"edit","replace":false,"save":false}),
            json!({"targetWorkspaceId":target,"description":"Stdin description\n","mode":"create","replace":false,"save":false}),
        ]
    );
    server.verify().await;
}

#[test]
fn create_json_help_and_invalid_input_never_attempt_workspace_or_body_loading() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir(home.path().join(".g5")).unwrap();
    std::fs::write(
        home.path().join(".g5/provided-agent-chat-workspace.json"),
        "invalid",
    )
    .unwrap();
    for verb in ["create", "update"] {
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .args(["pr", verb, "--json", "--body-file", "/missing/body.md"])
            .assert()
            .success()
            .stdout(predicates::str::contains("Available fields:"));
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .args(["pr", verb, "--json", "unknown"])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicates::str::contains("Unknown JSON field"));
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .args(["pr", verb, "--title", "   ", "--json", "id"])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicates::str::contains("PR title must not be blank"));
        cli(&home, "http://127.0.0.1:1", Uuid::nil())
            .args(["pr", verb, "--body", "text", "--body-file", "-"])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicates::str::contains("cannot be used with"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_does_not_retry_ambiguous_transport_failures_or_hide_provider_outcomes() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let target = Uuid::new_v4().to_string();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests");
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(
                json!({"error":{"code":"SERVICE_UNAVAILABLE","message":"Reply lost"}}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "create", "--target", &target, "--json", "id"])
        .assert()
        .failure()
        .stdout("");
    server.verify().await;
    server.reset().await;
    let saved_pr = Uuid::new_v4().to_string();
    Mock::given(method("POST")).and(path(&endpoint))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error":{
            "code":"CONFLICT", "message":format!("G5 pull request {saved_pr} was saved, but provider synchronization was not confirmed. Inspect PR state before retrying."),
            "details":{"kind":"pull_request_provider_sync_incomplete","pullRequestId":saved_pr}, "requestId":"partial-pr-test"
        }}))).expect(2).mount(&server).await;
    for json in [false, true] {
        let mut command = cli(&home, &server.uri(), workspace);
        command.args(["pr", "update", "--target", &target]);
        if json {
            command.args(["--json", "id"]);
        }
        // Saved in G5, provider publication unconfirmed: exit 5.
        command
            .assert()
            .code(5)
            .stdout("")
            .stderr(predicates::str::contains(&saved_pr))
            .stderr(predicates::str::contains(
                "Inspect PR state before retrying",
            ));
    }
    server.verify().await;
    server.reset().await;
    Mock::given(method("POST")).and(path(&endpoint))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"alreadyExisted":false,"pullRequest":{
            "id":Uuid::new_v4(),"status":"open","providerStatus":"error","providerRepos":[{"providerRepoFullName":"acme/api","providerStatus":"error","lastError":"Connect GitHub in Account settings"}]
        },"publication":{"complete":false,"repositories":[{"repository":"acme/api","number":null,"action":"action_required",
            "head":null,"title":null,"body":null,"message":"Connect GitHub in Account settings"}]}}}))).expect(1).mount(&server).await;
    // A saved G5 PR whose GitHub publication needs action exits 5 and says why.
    cli(&home, &server.uri(), workspace)
        .args(["pr", "create", "--target", &target])
        .assert()
        .code(5)
        .stdout(predicates::str::contains("provider: error"))
        .stdout(predicates::str::contains(
            "acme/api (no PR) · action required\n    Connect GitHub in Account settings",
        ));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inferred_source_and_target_with_title_use_shared_create_transport() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    for (prefix, user) in [("internal", false), ("v1", true)] {
        Mock::given(method("POST"))
            .and(path(format!(
                "/api/{prefix}/workspaces/{workspace}/pull-requests"
            )))
            .and(wiremock::matchers::body_json(
                json!({"description":null,"title":"My PR","mode":"create","replace":false,"save":false}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
                "pullRequest":{"title":"My PR"},"alreadyExisted":false,
                "publication":{"complete":true,"repositories":[]}
            }})))
            .expect(1)
            .mount(&server)
            .await;
        let mut command = cli(&home, &server.uri(), workspace);
        if user {
            command
                .env_remove("G5D_WORKSPACE_TOKEN")
                .env("G5D_TOKEN", "user-token");
        }
        command
            .args([
                "pr",
                "create",
                if user { "--title" } else { "-t" },
                "My PR",
                "--json",
                "title",
            ])
            .assert()
            .success()
            .stdout("{\n  \"title\": \"My PR\"\n}\n");
    }
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn github_node_id_is_selected_for_numbers_and_urls() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/info"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "source":"github","pullRequest":{"id":"PR_node42","number":42}
        }})))
        .expect(2)
        .mount(&server)
        .await;
    for selector in ["42", "https://github.com/acme/api/pull/42"] {
        let output = cli(&home, &server.uri(), workspace)
            .args(["pr", "view", selector, "--json", "id,number"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap(),
            json!({"id":"PR_node42","number":42})
        );
    }
    server.verify().await;
}

fn check_observation(buckets: &[&str]) -> Value {
    json!({"source":"github","repositories":[{"repository":"acme/api","number":42,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","availability":"ready","checks":buckets.iter().enumerate().map(|(i,b)|json!({"name":format!("check-{i}"),"bucket":b,"state":b.to_ascii_uppercase(),"repository":"acme/api","number":42,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})).collect::<Vec<_>>()}]})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_exit_status_uses_all_results_even_when_json_omits_status() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    for (buckets, exit) in [
        (vec!["pass", "skipping"], 0),
        (vec!["pending"], 8),
        (vec!["fail"], 1),
        (vec!["fail", "pending"], 1),
        (vec!["cancel"], 1),
        (vec![], 8),
    ] {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/internal/workspaces/{workspace}/pull-requests/info"
            )))
            .and(query_param("checks", "true"))
            .and(header("authorization", "Bearer current-incarnation"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":check_observation(&buckets)})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let out = cli(&home, &server.uri(), workspace)
            .args(["pr", "checks", "--json", "name"])
            .assert()
            .code(exit)
            .get_output()
            .stdout
            .clone();
        let rows: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), buckets.len());
        if !buckets.is_empty() {
            assert_eq!(rows[0], json!({"name":"check-0"}));
        }
        server.verify().await;
    }
    server.reset().await;
    let mut data = check_observation(&["pass"]);
    data["repositories"].as_array_mut().unwrap().push(json!({"repository":"acme/other","availability":"unavailable","message":"Grant Checks read access","checks":[]}));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":data})))
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--json", "name"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("Grant Checks read access"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_watch_waits_for_current_head_and_prints_one_final_json_document() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(query_param("checks", "true"))
        .and(query_param(
            "head",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .and(query_param("repo", "acme/api"))
        .and(query_param("details", "true"))
        .and(query_param("selector", "42"))
        .respond_with(move |_: &wiremock::Request| {
            let mut data = check_observation(&["pass"]);
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                data["repositories"][0]["availability"] = json!("pending");
                data["repositories"][0]["checks"] = json!([]);
                data["repositories"][0]["message"] = json!("Waiting for publication");
            }
            ResponseTemplate::new(200).set_body_json(json!({"data":data}))
        })
        .expect(2)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "checks",
            "42",
            "--repo",
            "acme/api",
            "--head",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--watch",
            "--interval",
            "5",
            "--json",
            "name,details",
        ])
        .assert()
        .success()
        .stderr(predicates::str::contains("Waiting for publication"))
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"name":"check-0","details":null}])
    );
    server.verify().await;
}

/// A required check the base branch names but GitHub has not reported yet is an
/// expected pending row: watch keeps waiting for it, and output marks it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_watch_waits_for_expected_required_checks_and_marks_them() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(query_param("checks", "true"))
        .respond_with(move |_: &wiremock::Request| {
            let mut data = check_observation(&["pass"]);
            let repo = &mut data["repositories"][0];
            repo["baseRefName"] = json!("main");
            repo["requiredChecks"] = json!([{"context":"ci","appId":null}]);
            repo["checks"][0]["required"] = json!(false);
            repo["checks"].as_array_mut().unwrap().push(if count.fetch_add(1, Ordering::SeqCst) == 0 {
                json!({"kind":"expected","name":"ci","bucket":"pending","state":"EXPECTED","required":true})
            } else {
                json!({"kind":"check_run","name":"ci","bucket":"pass","state":"SUCCESS","required":true})
            });
            ResponseTemplate::new(200).set_body_json(json!({"data":data}))
        })
        .expect(2)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "checks",
            "--watch",
            "--interval",
            "5",
            "--json",
            "name,required",
        ])
        .assert()
        .success()
        .stderr(predicates::str::contains("ci (required, not reported yet)"))
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"name":"check-0","required":false},{"name":"ci","required":true}])
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_fail_fast_and_read_errors_do_not_loop_or_emit_json_errors_on_stdout() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":check_observation(&["fail","pending"])})),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--fail-fast"])
        .assert()
        .code(1)
        .stdout(predicates::str::contains("fail  check-0"));
    server.verify().await;
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(
                json!({"error":{"code":"FORBIDDEN","message":"Read access revoked"}}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--json", "name"])
        .assert()
        .code(1)
        .stdout("");
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_fields_and_argument_validation_work_without_api_access() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("bucket"))
        .stdout(predicates::str::contains("details"));
    for args in [
        vec!["pr", "checks", "--json", "notAField"],
        vec!["pr", "checks", "--head", "short"],
        vec!["pr", "checks", "--fail-fast"],
        vec!["pr", "checks", "--interval", "0"],
        vec!["pr", "checks", "--watch", "--interval", "4"],
        vec!["pr", "checks", "--timeout", "5"],
        vec!["pr", "checks", "--watch", "--timeout", "0"],
    ] {
        cli(&home, &server.uri(), workspace)
            .args(args)
            .assert()
            .failure();
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A rate limit is waited out until GitHub's reported reset, then read again;
/// `--fail-fast` still stops at a failure, and other unavailability exits 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_watch_waits_out_rate_limits_until_the_reported_reset() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let limited = |buckets: &[&str], retry_at: Value| {
        let mut data = check_observation(buckets);
        let repo = &mut data["repositories"][0];
        repo["availability"] = json!("unavailable");
        repo["rateLimited"] = json!(true);
        repo["retryAt"] = retry_at;
        repo["message"] = json!("GitHub checks returned HTTP 429. Retry after 60 seconds.");
        data
    };
    // --fail-fast keeps a failure found before the limit, with its partial details.
    let mut data = limited(&["fail", "pending"], Value::Null);
    data["repositories"][0]["checks"][0]["details"] = json!({"availability":"partial","summary":"compiler E0308","annotationAvailability":"unavailable"});
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":data})))
        .expect(1)
        .mount(&server)
        .await;
    let output = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "checks",
            "--watch",
            "--fail-fast",
            "--json",
            "name,details",
        ])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("Retry after 60 seconds"))
        .get_output()
        .stdout
        .clone();
    let rows: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(rows[0]["details"]["summary"], "compiler E0308");
    server.verify().await;

    // Otherwise the watch sleeps until the reset and reads again.
    server.reset().await;
    let count = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(std::sync::Mutex::new(None));
    let seen = first.clone();
    Mock::given(method("GET"))
        .respond_with(move |_: &wiremock::Request| {
            let data = if count.fetch_add(1, Ordering::SeqCst) == 0 {
                let reset = chrono::Utc::now() + chrono::Duration::seconds(1);
                *seen.lock().unwrap() = Some(std::time::Instant::now());
                limited(&["pending"], json!(reset.to_rfc3339()))
            } else {
                check_observation(&["pass"])
            };
            ResponseTemplate::new(200).set_body_json(json!({"data":data}))
        })
        .expect(2)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--json", "name"])
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "GitHub rate-limited the checks read; waiting until",
        ));
    let waited = first.lock().unwrap().unwrap().elapsed();
    assert!(waited >= std::time::Duration::from_secs(1), "{waited:?}");
    server.verify().await;

    // A repository unavailable for another reason still ends the watch.
    server.reset().await;
    let mut data = limited(&["pending"], Value::Null);
    data["repositories"].as_array_mut().unwrap().push(json!({"repository":"acme/other","availability":"unavailable","message":"Grant Checks read access","checks":[]}));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":data})))
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--json", "name"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("Grant Checks read access"));
    server.verify().await;
}

/// A merged or closed GitHub PR ends the watch even while checks stay pending.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_watch_stops_once_every_github_pr_is_completed() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let mut data = check_observation(&["pass", "pending"]);
    data["repositories"][0]["prState"] = json!("merged");
    data["repositories"].as_array_mut().unwrap().push(
        json!({"repository":"acme/web","number":7,"availability":"ready","prState":"closed",
            "checks":[{"name":"lint","bucket":"pass"}]}),
    );
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":data})))
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--json", "name"])
        .assert()
        .code(8)
        .stderr(predicates::str::contains(
            "GitHub PR acme/api#42 is merged; stopped watching.",
        ))
        .stderr(predicates::str::contains(
            "GitHub PR acme/web#7 is closed; stopped watching.",
        ));
    server.verify().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_watch_ctrl_c_cancels_reads_and_waits_without_final_json() {
    use std::process::{Command as StdCommand, Stdio};
    use std::time::{Duration, Instant};
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    for delay in [Duration::from_secs(2), Duration::ZERO] {
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":check_observation(&["pending"])}))
                    .set_delay(delay),
            )
            .mount(&server)
            .await;
        let mut command = cli_process(&home, &server.uri(), workspace);
        let mut child = command
            .args([
                "pr",
                "checks",
                "--watch",
                "--interval",
                "30",
                "--json",
                "name",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.received_requests().await.unwrap().is_empty() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(!server.received_requests().await.unwrap().is_empty());
        // In-flight read for the delayed response; read/wait handoff for immediate response.
        assert!(StdCommand::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success());
        while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Ctrl-C did not stop checks watch");
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(130),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comment_and_resolve_send_the_owner_write_and_report_each_outcome() {
    use wiremock::matchers::body_partial_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let comments = format!("/api/internal/workspaces/{workspace}/pull-requests/github/comments");
    let publication = |state: &str, error: Value| {
        json!({"data":{"id":Uuid::new_v4(),"state":state,"destination":"review_thread","threadId":"PRRT_1",
            "displayKey":"acme/api#42","githubLogin":"owner","body":"Fixed","origin":"agent","error":error,
            "externalCommentUrl": if state == "published" { json!("https://github.com/acme/api/pull/42#discussion_r9") } else { Value::Null }}})
    };
    Mock::given(method("POST"))
        .and(path(&comments))
        .and(header("authorization", "Bearer current-incarnation"))
        .and(body_partial_json(
            json!({"selector":null,"replyTo":"PRRT_1","body":"Fixed","idempotencyKey":"k1"}),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(publication("published", Value::Null)),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "comment",
            "--reply-to",
            "PRRT_1",
            "-b",
            "Fixed",
            "--idempotency-key",
            "k1",
        ])
        .assert()
        .code(0)
        .stdout("https://github.com/acme/api/pull/42#discussion_r9\n");
    // An unclear outcome exits 7 and names the key that checks without reposting.
    Mock::given(method("POST"))
        .and(path(&comments))
        .and(body_partial_json(json!({"idempotencyKey":"k2"})))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(publication("outcome_unknown", Value::Null)),
        )
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "comment",
            "--reply-to",
            "PRRT_1",
            "-b",
            "Fixed",
            "--idempotency-key",
            "k2",
        ])
        .assert()
        .code(7)
        .stderr(predicates::str::contains("--idempotency-key k2"));
    // A refusal exits 1; JSON selection still prints the record.
    Mock::given(method("POST"))
        .and(path(&comments))
        .and(body_partial_json(
            json!({"selector":"42","repo":"acme/api","idempotencyKey":"k3"}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(publication(
            "failed",
            json!({"issueError":"access_denied","message":"Resource not accessible"}),
        )))
        .expect(1)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr",
            "comment",
            "42",
            "-R",
            "acme/api",
            "-b",
            "@coderabbitai review",
            "--idempotency-key",
            "k3",
            "--json",
            "state,error,number",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["state"], "failed");
    assert_eq!(value["number"], 42);
    // Resolution sends the requested state.
    Mock::given(method("POST"))
        .and(path(format!("/api/internal/workspaces/{workspace}/pull-requests/github/threads/resolve")))
        .and(body_partial_json(json!({"thread":"PRRT_1","resolved":false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "threadId":"PRRT_1","repository":"acme/api","number":42,"isResolved":false,"changed":true}})))
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "resolve", "PRRT_1", "--unresolve"])
        .assert()
        .code(0)
        .stdout("Marked review thread PRRT_1 on acme/api#42 unresolved\n");
    // Missing input is refused before any request.
    cli(&home, &server.uri(), workspace)
        .args(["pr", "comment", "42"])
        .assert()
        .code(2);
    cli(&home, &server.uri(), workspace)
        .args(["pr", "comment", "-b", "x"])
        .assert()
        .code(2);
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_and_create_new_report_each_repository_and_exit_by_what_was_saved() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/pull-requests");
    // A gateway's HTML 502 is retried; the reconciled retry reports the result.
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .and(wiremock::matchers::body_partial_json(json!({"mode":"create","replace":true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "pullRequest":{"id":Uuid::new_v4(),"title":"Fix","status":"open","providerStatus":"open"},
            "alreadyExisted":true,"dirtyDrafts":[],
            "publication":{"complete":true,"repositories":[{"repository":"acme/api","number":43,
                "url":"https://github.com/acme/api/pull/43","action":"replaced","head":"published",
                "title":null,"body":"applied","message":null}]}}})))
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "create", "--new", "-b", "Why"])
        .assert()
        .code(0)
        .stdout(predicates::str::contains(
            "acme/api #43 · replaced · head published · body applied",
        ));
    server.verify().await;
    server.reset().await;
    // A pending head exits 5 even though the request succeeded.
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .and(wiremock::matchers::body_partial_json(
            json!({"mode":"update","replace":false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "pullRequest":{"id":Uuid::new_v4(),"title":"Fix","status":"open"},"alreadyExisted":true,
            "publication":{"complete":false,"repositories":[{"repository":"acme/api","number":43,
                "action":"reused","head":"pending","title":null,"body":null,"message":null}]}}})))
        .expect(2)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "update"])
        .assert()
        .code(5)
        .stderr(predicates::str::contains("incomplete or pending"));
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "update", "--json", "publication"])
        .assert()
        .code(5)
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["publication"]["repositories"][0]["head"], "pending");
    server.verify().await;
    server.reset().await;
    // Typed failures keep their own status and code: a save/push failure
    // (409 or 422) exits 4; no open PR to update exits 1.
    for (status, error_code, details, code) in [
        (
            409,
            "WORKSPACE_NOT_RUNNING",
            json!({"kind":"pull_request_source_save_failed"}),
            4,
        ),
        (
            422,
            "VALIDATION_ERROR",
            json!({"code":"PR_SOURCE_PROVIDER_ADVANCED","message":"refused: advanced","kind":"pull_request_source_save_failed"}),
            4,
        ),
        (409, "CONFLICT", json!({"kind":"no_open_pull_request"}), 1),
    ] {
        Mock::given(method("POST"))
            .and(path(&endpoint))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({"error":{
                "code":error_code,"message":format!("refused: {error_code}"),"details":details,"requestId":"r"}})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        cli(&home, &server.uri(), workspace)
            .args(["pr", "update"])
            .assert()
            .code(code)
            .stderr(predicates::str::contains("refused"));
    }
    // A provider permission refusal (#612) is a save failure: exit 4 with
    // the server's actionable reason, not a generic internal error.
    let reason = "GitHub refused to publish branch g5/feature to https://github.com/acme/api.git because its new commits change the workflow file .github/workflows/ci.yml and the G5 GitHub App credential lacks the Workflows permission. Nothing was published to that branch.";
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error":{
            "code":"CONFLICT","message":reason,"requestId":"r",
            "details":{"code":"PROVIDER_PERMISSION_REQUIRED","permission":"workflows",
                "credential":"github_app","path":".github/workflows/ci.yml",
                "kind":"pull_request_source_save_failed"}}})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "update"])
        .assert()
        .code(4)
        .stderr(predicates::str::contains(reason));
    // --new belongs to create.
    cli(&home, &server.uri(), workspace)
        .args(["pr", "update", "--new"])
        .assert()
        .code(2);
}

/// `g5.git.pr-no-github`: where GitHub is not expected, `g5d pr` reports no
/// missing GitHub data: checks pass with nothing to wait for, a stored
/// listing needs no notice, and a branch names its G5 PR.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commands_do_not_complain_where_github_is_not_expected() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let pr = Uuid::new_v4();
    let message = "No external checks apply: this PR's repositories are not on GitHub.";
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/info"
        )))
        .and(query_param("checks", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "source":"g5","repositories":[],"noExternalChecks":true,"message":message}})))
        .mount(&server)
        .await;
    // Nothing to wait for, even when watching a head.
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--watch", "--head", &"a".repeat(40)])
        .timeout(std::time::Duration::from_secs(20))
        .assert()
        .code(0)
        .stdout(predicates::str::contains(message))
        .stdout(predicates::str::contains("Observed GitHub checks").not());
    cli(&home, &server.uri(), workspace)
        .args(["pr", "checks", "--json", "name"])
        .assert()
        .code(0)
        .stdout("[]\n")
        .stderr(predicates::str::contains(message));

    // A repository listed from G5's records says so only when GitHub was
    // expected for it.
    for (expected, notice) in [(false, false), (true, true)] {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/internal/workspaces/{workspace}/pull-requests/github"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
                "source":"g5","repository":"docs","githubExpected":expected,
                "fallback": if expected { "GitHub is not connected for this repository" } else { "This repository is not on GitHub" },
                "pullRequests":[{"source":"g5","id":pr,"number":null,"title":"Docs","state":"OPEN",
                    "headRefName":"feature-x","g5PullRequests":[]}],"hasMore":false}})))
            .mount(&server)
            .await;
        let stderr = cli(&home, &server.uri(), workspace)
            .args(["pr", "list", "--repo", "docs"])
            .assert()
            .success()
            .get_output()
            .stderr
            .clone();
        let stderr = String::from_utf8(stderr).unwrap();
        assert_eq!(
            stderr.contains("listing G5's stored PRs instead of GitHub's"),
            notice,
            "{stderr}"
        );
    }

    // A branch whose PR exists only in G5 names that G5 PR.
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/info"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"g5","pullRequests":[
            {"id":pr,"sourceWorkspaceId":workspace,"title":"Docs","status":"open","createdAt":"2026-10-28"}
        ]}})))
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "view", "feature-x", "--json", "id,state"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!({"id":pr,"state":"OPEN"})
    );
}

/// `g5.git.pr-reads`: `g5d pr` reads any PR its owner can read: a G5 ID asks
/// the server for that PR, `--all`/`--workspace` list readable G5 PRs, and
/// every listing fills `body`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reads_reach_every_readable_pr_and_listings_fill_bodies() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let other = Uuid::new_v4();
    let info = format!("/api/internal/workspaces/{workspace}/pull-requests/info");
    let pull = json!({"id":other,"sourceWorkspaceId":Uuid::new_v4(),"sourceWorkspaceName":"dummy",
        "targetWorkspaceName":"Base","title":"foo","status":"open","createdAt":"2026-10-28"});
    Mock::given(method("GET"))
        .and(path(&info))
        .and(query_param("selector", other.to_string()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"source":"g5","pullRequest":pull}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "view", &other.to_string(), "--json", "id,title,state"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!({"id":other,"title":"foo","state":"OPEN"})
    );

    // `--all` and `--workspace` pass their filters to the server, which lists.
    Mock::given(method("GET"))
        .and(path(&info))
        .and(query_param("all", "true"))
        .and(query_param("state", "all"))
        .and(query_param("limit", "5"))
        .and(query_param("author", "@me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "source":"g5","listing":true,"hasMore":true,"pullRequests":[pull]}})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/descriptions"
        )))
        .and(query_param("ids", other.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[
            {"pullRequestId":other,"description":"The body","descriptionSource":"projectDoc"}]})))
        .expect(2)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args([
            "pr", "list", "--all", "-s", "all", "-L", "5", "-A", "@me", "--json", "id,body",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"id":other,"body":"The body"}])
    );
    Mock::given(method("GET"))
        .and(path(&info))
        .and(query_param("workspace", "dummy"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "source":"g5","listing":true,"hasMore":false,"pullRequests":[pull]}})))
        .expect(1)
        .mount(&server)
        .await;
    cli(&home, &server.uri(), workspace)
        .args(["pr", "list", "-W", "dummy"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dummy → Base"));
    cli(&home, &server.uri(), workspace)
        .args(["pr", "list", "--all", "--repo", "acme/api"])
        .assert()
        .failure();

    // A repository listing asks GitHub for bodies and fills stored rows from
    // the canonical description.
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/pull-requests/github"
        )))
        .and(query_param("body", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{
            "source":"github","repository":"acme/api","hasMore":false,"pullRequests":[
                {"source":"github","id":"PR_1","number":1,"title":"On GitHub","body":"GitHub body"},
                {"source":"g5","id":other,"number":null,"title":"foo","body":null}]}})))
        .expect(1)
        .mount(&server)
        .await;
    let out = cli(&home, &server.uri(), workspace)
        .args(["pr", "list", "--repo", "acme/api", "--json", "number,body"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&out).unwrap(),
        json!([{"number":1,"body":"GitHub body"},{"number":null,"body":"The body"}])
    );
    server.verify().await;
}

/// `g5d pr close` on a G5 PR that is no longer open refuses before it
/// comments on any GitHub PR or asks G5 to close it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_an_ended_pr_before_commenting() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let pr = Uuid::new_v4();
    Mock::given(method("GET"))
        .and(path(format!("/api/internal/workspaces/{workspace}/pull-requests/info")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"source":"g5","pullRequests":[
            {"id":pr,"sourceWorkspaceId":workspace,"sourceWorkspaceName":"feature","targetWorkspaceName":"main",
             "title":"Fix bug","status":"merged","localMergeStatus":"merged","providerStatus":"merged","createdAt":"2026-09-30",
             "providerRepos":[{"repoId":Uuid::new_v4(),"providerRepoFullName":"acme/app","providerPrNumber":7,"providerStatus":"merged"}]}
        ]}})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let output = cli(&home, &server.uri(), workspace)
        .args(["pr", "close", &pr.to_string(), "-c", "Superseded"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("is merged, not open"), "{text}");
    server.verify().await;
}
