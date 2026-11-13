//! Issue commands consume the current credential at each HTTP boundary.
use assert_cmd::Command;
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn issue_commands_read_the_current_credential_and_fail_closed_if_removed() {
    for remove_credential in [false, true] {
        let home = TempDir::new().unwrap();
        let token_path = home.path().join("workspace-token");
        std::fs::write(&token_path, "initial-token").unwrap();
        let server = MockServer::start().await;
        let workspace = Uuid::new_v4();
        let publish_path = token_path.clone();
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/internal/workspaces/{workspace}/issue-links"
            )))
            .and(header("authorization", "Bearer initial-token"))
            .respond_with(move |_: &wiremock::Request| {
                if remove_credential {
                    std::fs::remove_file(&publish_path).unwrap();
                } else {
                    let next = publish_path.with_extension("next");
                    std::fs::write(&next, "renewed-token").unwrap();
                    std::fs::rename(next, &publish_path).unwrap();
                }
                ResponseTemplate::new(200).set_body_json(json!({"data": {"links": []}}))
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/internal/workspaces/{workspace}/issues/view"
            )))
            .and(header("authorization", "Bearer renewed-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"issue": {"title": "Linked issue"}, "workspaceLink": null}
            })))
            .expect(if remove_credential { 0 } else { 1 })
            .mount(&server)
            .await;
        let command = |args: &[&str]| {
            let mut command =
                Command::from_std(std::process::Command::new(env!("CARGO_BIN_EXE_g5d")));
            command
                .current_dir(home.path())
                .env("G5_HOME", home.path())
                .env("PWD", home.path())
                .env_remove("G5D_STABLE_CWD_ROOT")
                .env_remove("G5D_TOKEN")
                .env_remove("G5_API_KEY")
                .env("G5D_API_URL", server.uri())
                .env("G5D_WORKSPACE_ID", workspace.to_string())
                .env("G5D_WORKSPACE_TOKEN", "must-not-fall-back")
                .env("G5D_WORKSPACE_TOKEN_FILE", &token_path)
                .args(args);
            command
        };
        command(&["issue", "list"]).assert().success();
        if remove_credential {
            command(&["issue", "view", "--json"]).assert().failure();
        } else {
            command(&["issue", "view", "--json"])
                .assert()
                .success()
                .stdout(predicates::str::contains("Linked issue"));
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), if remove_credential { 1 } else { 2 });
        server.verify().await;
    }
}

fn issue_command(home: &TempDir, server: &MockServer, workspace: Uuid) -> Command {
    let token_path = home.path().join("workspace-token");
    std::fs::write(&token_path, "vm-token").unwrap();
    let mut command = Command::from_std(std::process::Command::new(env!("CARGO_BIN_EXE_g5d")));
    command
        .current_dir(home.path())
        .env("G5_HOME", home.path())
        .env("PWD", home.path())
        .env_remove("G5D_STABLE_CWD_ROOT")
        .env_remove("G5D_TOKEN")
        .env_remove("G5_API_KEY")
        .env("G5D_API_URL", server.uri())
        .env("G5D_WORKSPACE_ID", workspace.to_string())
        .env("G5D_WORKSPACE_TOKEN_FILE", &token_path);
    command
}

/// `g5d issue create` prints the URL like `gh`, exits 7 with the retry key
/// when GitHub did not confirm, and is sent once (never retried by itself).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_prints_the_url_and_reports_an_unclear_outcome_with_its_key() {
    use wiremock::matchers::body_partial_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/issues");
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .and(header("authorization", "Bearer vm-token"))
        .and(body_partial_json(json!({
            "title": "Crash on save", "body": "Steps", "assignees": ["@me", "octo"],
            "idempotencyKey": "k-1",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {
            "state": "created", "sent": true,
            "issue": {"url": "https://github.com/acme/web/issues/40", "number": 40},
            "destination": {"displayKey": "acme/web"},
        }})))
        .expect(1)
        .mount(&server)
        .await;
    issue_command(&home, &server, workspace)
        .args([
            "issue",
            "create",
            "-t",
            "Crash on save",
            "-b",
            "Steps",
            "-a",
            "@me,octo",
            "--idempotency-key",
            "k-1",
        ])
        .assert()
        .success()
        .stdout("https://github.com/acme/web/issues/40\n");
    server.verify().await;

    server.reset().await;
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "state": "outcome_unknown", "sent": true, "issue": null,
            "destination": {"displayKey": "acme/web"}, "idempotencyKey": "k-2",
        }})))
        .expect(1)
        .mount(&server)
        .await;
    issue_command(&home, &server, workspace)
        .args([
            "--json",
            "issue",
            "create",
            "-t",
            "T",
            "-b",
            "",
            "--idempotency-key",
            "k-2",
        ])
        .assert()
        .code(7)
        .stdout(predicates::str::contains("\"outcome_unknown\""))
        .stderr(predicates::str::contains("--idempotency-key k-2"));
    server.verify().await;

    // A gateway error is not retried: the rerun is the agent's choice.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path(&endpoint))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&server)
        .await;
    issue_command(&home, &server, workspace)
        .args([
            "issue",
            "create",
            "-t",
            "T",
            "-b",
            "B",
            "--idempotency-key",
            "k-3",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--idempotency-key k-3"));
    server.verify().await;

    // Without a title and body nothing is sent and nothing prompts.
    server.reset().await;
    issue_command(&home, &server, workspace)
        .args(["issue", "create", "-t", "T"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("never prompts"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// With `--parent`, an issue that is not under its parent still prints its
/// URL; an unconfirmed parent exits 7 with the key and a refused one exits 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_with_a_parent_reports_an_issue_left_outside_it() {
    use wiremock::matchers::body_partial_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let endpoint = format!("/api/internal/workspaces/{workspace}/issues");
    for (parent_state, message, code, hint) in [
        ("attached", None, 0, None),
        (
            "outcome_unknown",
            Some("GitHub timed out"),
            7,
            Some("--idempotency-key k-p"),
        ),
        (
            "not_attached",
            Some("Parent cannot have more than 100 sub-issues"),
            1,
            Some("--idempotency-key k-p"),
        ),
        (
            "other_parent",
            Some("the issue is already a sub-issue of acme/web#3"),
            1,
            None,
        ),
    ] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path(&endpoint))
            .and(body_partial_json(json!({"parent": "#12", "idempotencyKey": "k-p"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {
                "state": "created", "sent": true,
                "issue": {"url": "https://github.com/acme/web/issues/41", "number": 41},
                "destination": {"displayKey": "acme/web"},
                "parent": {"displayKey": "acme/web#12", "number": 12, "state": parent_state, "message": message},
            }})))
            .expect(1)
            .mount(&server)
            .await;
        let assert = issue_command(&home, &server, workspace)
            .args([
                "issue",
                "create",
                "-t",
                "Sub",
                "-b",
                "B",
                "--parent",
                "#12",
                "--idempotency-key",
                "k-p",
            ])
            .assert()
            .code(code)
            .stdout("https://github.com/acme/web/issues/41\n");
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        if let Some(message) = message {
            assert!(
                stderr.contains("not added under acme/web#12"),
                "{parent_state}: {stderr}"
            );
            assert!(stderr.contains(message), "{parent_state}: {stderr}");
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        assert_eq!(
            hint.is_some_and(|hint| stderr.contains(hint)),
            hint.is_some(),
            "{parent_state}: {stderr}"
        );
        server.verify().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn link_links_an_issue_as_related_unless_primary() {
    use wiremock::matchers::body_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/internal/workspaces/{workspace}/issue-links"
        )))
        .and(body_json(json!({"ref": "#12", "role": "related"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {
            "displayKey": "acme/web#12", "role": "related",
        }})))
        .expect(1)
        .mount(&server)
        .await;
    issue_command(&home, &server, workspace)
        .args(["issue", "link", "#12"])
        .assert()
        .success()
        .stdout("Linked acme/web#12 to this workspace as related.\n");
    server.verify().await;
}

fn viewed_issue() -> serde_json::Value {
    json!({"data": {
        "workspaceLink": null,
        "issue": {
            "displayKey": "acme/web#40", "locator": {"repositoryId": "1", "number": 40},
            "canonicalUrl": "https://github.com/acme/web/issues/40",
            "title": "Other team's bug", "state": "open", "stateReason": null,
            "author": {"login": "reporter"}, "createdAt": "2026-09-01T00:00:00Z",
            "updatedAt": "2026-09-02T00:00:00Z", "contentTrusted": false,
            "assignees": [], "labels": [{"name": "bug", "color": "d73a4a"}],
            "body": "Steps", "commentCount": 1, "commentsTruncated": false,
            "comments": [{"author": {"login": "ada"}, "body": "Seen it too",
                          "createdAt": "2026-09-01T01:00:00Z", "url": "u"}],
            "links": [{"workspaceId": "w", "workspaceName": "fix-login", "role": "primary"}]
        }
    }})
}

/// `g5d issue view` reads any issue: `-R` names the repository of a number,
/// `-c` asks for comments, and `--json FIELDS` prints gh's field names with
/// G5's own only when named.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_reads_any_issue_with_gh_flags_and_fields() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let view_path = format!("/api/internal/workspaces/{workspace}/issues/view");
    Mock::given(method("GET"))
        .and(path(&view_path))
        .and(query_param("ref", "acme/web#40"))
        .and(query_param("comments", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(viewed_issue()))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(&view_path))
        .and(query_param("ref", "40"))
        .and(query_param("comments", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(viewed_issue()))
        .expect(1)
        .mount(&server)
        .await;

    let output = issue_command(&home, &server, workspace)
        .args(["issue", "view", "40", "-R", "acme/web", "-c"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.starts_with("acme/web#40 (not linked) [open]\n"),
        "{text}"
    );
    assert!(
        text.contains("Linked from: fix-login (primary)\n"),
        "{text}"
    );
    assert!(
        text.contains("not marked this issue's repository as trusted"),
        "{text}"
    );
    assert!(text.contains("### @ada"), "{text}");

    // Without -c the comments stay on the issue.
    issue_command(&home, &server, workspace)
        .args(["issue", "view", "40"])
        .assert()
        .success()
        .stdout(predicates::str::ends_with(
            "Comments: 1 (use -c to show them)\n",
        ));

    // `comments` among the fields asks for them; nothing else is printed.
    let output = issue_command(&home, &server, workspace)
        .args([
            "issue",
            "view",
            "acme/web#40",
            "--json",
            "title,state,comments,workspaceLinks",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value,
        json!({
            "title": "Other team's bug", "state": "OPEN",
            "comments": [{"author": {"login": "ada"}, "body": "Seen it too",
                          "createdAt": "2026-09-01T01:00:00Z", "url": "u"}],
            "workspaceLinks": [{"workspaceId": "w", "workspaceName": "fix-login", "role": "primary"}],
        })
    );

    // Fields G5 lacks, and gh flags it does not take, send nothing.
    issue_command(&home, &server, workspace)
        .args(["issue", "view", "40", "--json", "milestone"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("Available fields"));
    issue_command(&home, &server, workspace)
        .args(["issue", "view", "40", "--web"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("does not open a browser"));
    server.verify().await;
}

/// `g5d issue search` prints gh's tab-separated lines, keeps its notes on
/// stderr, refuses what G5 cannot apply without asking the API, and exits 3
/// when the agents' budget is spent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn search_prints_gh_lines_and_refuses_what_g5_cannot_apply() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let search_path = format!("/api/internal/workspaces/{workspace}/issues/search");
    Mock::given(method("GET"))
        .and(path(&search_path))
        .and(query_param("q", "crash label:bug"))
        .and(query_param("state", "all"))
        .and(query_param("assignee", "me"))
        .and(query_param("limit", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "results": [{
                "displayKey": "acme/web#7", "locator": {"number": 7}, "state": "open",
                "title": "Crash on save", "labels": [{"name": "bug"}],
                "updatedAt": "2026-09-02T00:00:00Z", "contentTrusted": true,
                "primaryWorkspace": {"visible": true, "workspaceId": "w", "name": "fix"}
            }],
            "domainErrors": [], "domainCount": 1, "truncated": true
        }})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(&search_path))
        .and(query_param("q", "flood"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({"error": {
            "code": "RATE_LIMITED", "message": "Rate limited",
            "details": {"retry_after_secs": 42}, "requestId": "req-1"
        }})))
        .expect(1)
        .mount(&server)
        .await;

    issue_command(&home, &server, workspace)
        .args([
            "issue",
            "search",
            "crash",
            "label:bug",
            "--assignee",
            "@me",
            "-L",
            "5",
        ])
        .assert()
        .success()
        .stdout("acme/web\t7\topen\tCrash on save\tbug\t2026-09-02T00:00:00Z\n")
        .stderr(predicates::str::contains("More issues match"));
    let output = issue_command(&home, &server, workspace)
        .args([
            "issue",
            "search",
            "crash label:bug",
            "--assignee",
            "@me",
            "-L",
            "5",
            "--json",
            "number,repository,primaryWorkspace",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value,
        json!([{"number": 7, "repository": {"name": "web", "nameWithOwner": "acme/web"},
                "primaryWorkspace": {"visible": true, "workspaceId": "w", "name": "fix"}}])
    );

    for (args, needle) in [
        (vec!["--assignee", "octo"], "assignee:octo"),
        (vec!["-L", "100"], "at most 30"),
        (vec!["crash", "repo:other/public"], "-R owner/name"),
        (vec!["--mentions", "octo"], "qualifiers in the query"),
        (vec!["--sort", "created"], "no --sort"),
    ] {
        issue_command(&home, &server, workspace)
            .args([&["issue", "search"], args.as_slice()].concat())
            .assert()
            .code(2)
            .stderr(predicates::str::contains(needle));
    }
    issue_command(&home, &server, workspace)
        .args(["issue", "search", "flood"])
        .assert()
        .code(3)
        .stderr(predicates::str::contains("retry in 42 seconds"));
    server.verify().await;
}

/// `g5d issue unlink` names the link that ended and that the issue is
/// unchanged; an issue the workspace does not link exits 6.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unlink_ends_a_link_and_reports_one_that_is_not_there() {
    use wiremock::matchers::body_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let unlink_path = format!("/api/internal/workspaces/{workspace}/issue-links/unlink");
    Mock::given(method("POST"))
        .and(path(&unlink_path))
        .and(body_json(json!({"ref": "#12"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "link": {"displayKey": "acme/web#12", "role": "primary"}
        }})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(&unlink_path))
        .and(body_json(json!({"ref": "acme/web#99"})))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": {
            "code": "NOT_FOUND",
            "message": "acme/web#99 among this workspace's linked issues (acme/web#12) not found"
        }})))
        .expect(1)
        .mount(&server)
        .await;
    issue_command(&home, &server, workspace)
        .args(["issue", "unlink", "#12"])
        .assert()
        .success()
        .stdout("Unlinked acme/web#12 (primary) from this workspace; the issue is unchanged.\n");
    issue_command(&home, &server, workspace)
        .args(["issue", "unlink", "acme/web#99"])
        .assert()
        .code(6);
    server.verify().await;
}

/// `g5d issue label` keeps a domain's catalog through the agent routes, and
/// `edit --add-label/--remove-label` reports each label's outcome: a partial
/// change exits 1, an unconfirmed one 7 (`g5.issue.labels.surfaces`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn labels_are_listed_created_deleted_and_changed_with_their_outcomes() {
    use wiremock::matchers::body_json;
    let home = TempDir::new().unwrap();
    let server = MockServer::start().await;
    let workspace = Uuid::new_v4();
    let agent = format!("/api/internal/workspace-agent/{workspace}");
    Mock::given(method("GET"))
        .and(path(format!("{agent}/issue-labels")))
        .and(query_param("domain", "acme/web"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "displayKey": "acme/web", "nextPage": null,
            "labels": [{"id": "1", "name": "bug", "color": "d73a4a", "description": "Broken"}]
        }})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{agent}/issue-labels")))
        .and(body_json(json!({"name": "priority:p1", "color": "fbca04"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "outcome": "applied", "githubLogin": "ada",
            "label": {"id": "3", "name": "priority:p1", "color": "fbca04"}
        }})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{agent}/issue-labels/delete")))
        .and(body_json(json!({"label": "wip"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "outcome": "unknown", "githubLogin": "ada", "label": null, "message": "timed out"
        }})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{agent}/issue-edit")))
        .and(body_json(json!({
            "ref": "acme/web#12", "addAssignees": [], "removeAssignees": [],
            "addLabels": ["priority:p1"], "removeLabels": ["priority:p0"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "outcome": "partial", "githubLogin": "ada", "message": "GitHub refused part of the change",
            "issue": {"displayKey": "acme/web#12", "assignees": [], "labels": [{"name": "priority:p0"}, {"name": "priority:p1"}]},
            "labels": {
                "outcome": "partial",
                "changes": [
                    {"label": "priority:p1", "change": "add", "outcome": "applied"},
                    {"label": "priority:p0", "change": "remove", "outcome": "refused", "message": "Must have triage access"}
                ],
                "issue": {"labels": [{"name": "priority:p0"}, {"name": "priority:p1"}]}
            }
        }})))
        .expect(1)
        .mount(&server)
        .await;

    issue_command(&home, &server, workspace)
        .args(["issue", "label", "list", "--domain", "acme/web"])
        .assert()
        .success()
        .stdout("bug\tBroken\t#d73a4a\n");
    issue_command(&home, &server, workspace)
        .args(["issue", "label", "create", "priority:p1", "-c", "fbca04"])
        .assert()
        .success()
        .stdout("Created label as @ada: priority:p1\t\t#fbca04\n");
    // Deleting needs --yes and sends nothing without it.
    issue_command(&home, &server, workspace)
        .args(["issue", "label", "delete", "wip"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--yes"));
    issue_command(&home, &server, workspace)
        .args(["issue", "label", "delete", "wip", "--yes"])
        .assert()
        .code(7)
        .stdout(predicates::str::contains("did not confirm"));
    issue_command(&home, &server, workspace)
        .args([
            "issue",
            "edit",
            "12",
            "-R",
            "acme/web",
            "--add-label",
            "priority:p1",
            "--remove-label",
            "priority:p0",
        ])
        .assert()
        .code(1)
        .stdout(predicates::str::contains(
            "Labels changed: +priority:p1\nNot removed: priority:p0 (Must have triage access)\nLabels now: priority:p0, priority:p1\n",
        ));
    server.verify().await;
}
