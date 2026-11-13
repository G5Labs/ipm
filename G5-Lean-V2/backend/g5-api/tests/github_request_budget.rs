//! GitHub request budget (`g5.git.provider-read-budget`, issue #1016): every
//! request a representative caller sends to the GitHub stub, grouped by method
//! and path, must match its scenario's table exactly. A new caller, endpoint
//! or repeated read shows up as a mismatch even when no test filters for it; a
//! drop shows up too, so a reduction is locked in by editing the table.
//!
//! Loop scenarios run `PASSES` passes back to back, all inside the first
//! checks backoff (an hour here, so a slow run cannot outlast it), so a table
//! counts those passes, not an hour of them; at the production cadence merge
//! when ready makes a pass every 30 seconds.

use crate::api_pull_request_reads::{
    check_head, checks_binding, github_app, revision_rows, CHECK_SHA, NEXT_SHA,
};
use crate::helpers;
use crate::helpers::review_flow::body;
use g5_scm::{GitAuthor, GitRepo};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Passes per merge-when-ready scenario.
const PASSES: usize = 10;

/// Merge-when-ready pacing whose idle backoff no scenario's passes outlast,
/// so the tables never depend on how fast the passes ran.
fn held_pacing() -> g5_api::auto_merge::ChecksPacing {
    let hour = std::time::Duration::from_secs(3600);
    g5_api::auto_merge::ChecksPacing::with_backoff(hour, hour)
}

/// Requests the stub has received so far; a scenario's window starts here.
async fn mark(github: &MockServer) -> usize {
    github.received_requests().await.unwrap().len()
}

/// Requests since `start` by `METHOD path`, with commit SHAs as `{sha}`.
async fn tally(github: &MockServer, start: usize) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for request in &github.received_requests().await.unwrap()[start..] {
        let path = request
            .url
            .path()
            .split('/')
            .map(|segment| {
                if segment.len() == 40 && segment.bytes().all(|b| b.is_ascii_hexdigit()) {
                    "{sha}"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        *counts
            .entry(format!("{} {path}", request.method))
            .or_default() += 1;
    }
    counts
}

/// Fails with the whole observed table, ready to paste, when it differs.
fn assert_budget(scenario: &str, actual: &BTreeMap<String, usize>, budget: &[(&str, usize)]) {
    let expected: BTreeMap<String, usize> = budget
        .iter()
        .map(|(endpoint, count)| (endpoint.to_string(), *count))
        .collect();
    if *actual != expected {
        let table = actual
            .iter()
            .map(|(endpoint, count)| format!("        (\"{endpoint}\", {count}),"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{scenario}: GitHub requests differ from the budget \
             (g5.git.provider-read-budget).\nObserved:\n{table}\nBudget: {expected:#?}"
        );
    }
}

async fn fixture(app: &TestApp) -> (TestUser, TestOrg, TestWorkspace) {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    (owner, org, workspace)
}

/// One checks read, as each poll of Prepare to Ship or `g5d pr checks
/// --watch` makes it.
async fn checks_read(app: &TestApp, owner: &TestUser, w: &TestWorkspace) -> Value {
    body(
        app.authenticated_client(owner)
            .get(&format!(
                "/api/v1/workspaces/{}/pull-requests/info?checks=true&selector=42",
                w.id
            ))
            .await,
        StatusCode::OK,
    )
    .await["repositories"][0]
        .clone()
}

async fn mount_completed_pull(github: &MockServer, merged: bool) {
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"closed","merged":merged,"mergeable":null,"base":{"ref":"main","sha":NEXT_SHA},"head":{"sha":CHECK_SHA}})))
        .mount(github)
        .await;
}

/// Base-branch protection requiring the `tests` check, so a pending run holds
/// merge when ready; it outranks `revision_rows`' nothing-required default.
async fn require_tests(github: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/branches/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name":"main","protection":{"enabled":true,"required_status_checks":{"enforcement_level":"everyone","contexts":["tests"],"checks":[{"context":"tests","app_id":null}]}}})))
        .with_priority(1)
        .mount(github)
        .await;
}

fn run(status: &str, conclusion: Value) -> Value {
    json!([{"id":7,"name":"tests","head_sha":CHECK_SHA,"status":status,"conclusion":conclusion}])
}

/// A read with a required check pending observes once; a settled read,
/// including one only advisory checks keep pending, observes twice to confirm.
#[tokio::test]
async fn an_open_pull_requests_checks_read_stays_within_budget() {
    for (scenario, required, status, conclusion, bucket, budget) in [
        (
            "pending required check read",
            true,
            "in_progress",
            Value::Null,
            "pending",
            &[
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 1),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 1),
                ("GET /repos/acme/api/commits/{sha}/statuses", 1),
                ("GET /repos/acme/api/pulls/42", 1),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ][..],
        ),
        (
            "pending advisory check read",
            false,
            "in_progress",
            Value::Null,
            "pending",
            &[
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 2),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 2),
                ("GET /repos/acme/api/commits/{sha}/statuses", 2),
                ("GET /repos/acme/api/pulls/42", 2),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ][..],
        ),
        (
            "passing checks read",
            false,
            "completed",
            json!("success"),
            "pass",
            &[
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 2),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 2),
                ("GET /repos/acme/api/commits/{sha}/statuses", 2),
                ("GET /repos/acme/api/pulls/42", 2),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ][..],
        ),
    ] {
        let github = MockServer::start().await;
        let app = github_app(&github).await;
        let (owner, org, w) = fixture(&app).await;
        checks_binding(&app, &github, &owner, &org, &w).await;
        if required {
            require_tests(&github).await;
        }
        check_head(&github, CHECK_SHA).await;
        revision_rows(&github, CHECK_SHA, run(status, conclusion), json!([])).await;
        let start = mark(&github).await;
        let repo = checks_read(&app, &owner, &w).await;
        assert_eq!(repo["availability"], "ready", "{scenario}: {repo}");
        assert_eq!(repo["checks"][0]["bucket"], bucket, "{scenario}: {repo}");
        assert_budget(scenario, &tally(&github, start).await, budget);
        app.cleanup().await;
    }
}

/// A merged or closed pull request's read reports its state with one settled
/// observation; the caller then stops, so there is no second poll to budget.
#[tokio::test]
async fn a_completed_pull_requests_checks_read_stays_within_budget() {
    for (scenario, merged, state) in [
        ("merged checks read", true, "merged"),
        ("closed checks read", false, "closed"),
    ] {
        let github = MockServer::start().await;
        let app = github_app(&github).await;
        let (owner, org, w) = fixture(&app).await;
        checks_binding(&app, &github, &owner, &org, &w).await;
        mount_completed_pull(&github, merged).await;
        revision_rows(
            &github,
            CHECK_SHA,
            run("completed", json!("success")),
            json!([]),
        )
        .await;
        let start = mark(&github).await;
        let repo = checks_read(&app, &owner, &w).await;
        assert_eq!(repo["prState"], state, "{scenario}: {repo}");
        assert_budget(
            scenario,
            &tally(&github, start).await,
            &[
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 2),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 2),
                ("GET /repos/acme/api/commits/{sha}/statuses", 2),
                ("GET /repos/acme/api/pulls/42", 2),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ],
        );
        app.cleanup().await;
    }
}

/// An open pull request from a child workspace (bound to GitHub `acme/api`)
/// into its parent, with no provider PR yet.
struct Open {
    owner: TestUser,
    org: TestOrg,
    child: TestWorkspace,
    parent: Uuid,
    pull_request: Uuid,
}

async fn open_pull_request(app: &TestApp, github: &MockServer) -> Open {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let child = TestWorkspace::create_named(app, &org, "feature").await;
    let parent = child.attach_parent_with(app, false).await;
    let mirror = app.repo_path(child.repo_id).await;
    let repo = GitRepo::open(&mirror).unwrap();
    repo.checkout_branch_force(&child.name).unwrap();
    std::fs::write(mirror.join("fix.txt"), "fixed\n").unwrap();
    repo.add_all().unwrap();
    repo.commit(
        "Change",
        &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
    )
    .unwrap();
    let created = body(
        app.authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/pull-requests", child.id),
                &json!({"targetWorkspaceId": parent}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let pull_request = created["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    checks_binding(app, github, &owner, &org, &child).await;
    Open {
        owner,
        org,
        child,
        parent,
        pull_request,
    }
}

/// An open pull request with merge when ready on, published as GitHub PR 42
/// at `CHECK_SHA`.
struct Armed {
    app: TestApp,
    github: MockServer,
    pull_request: Uuid,
}

impl Armed {
    async fn new() -> Self {
        let github = MockServer::start().await;
        let app = github_app(&github).await;
        let open = open_pull_request(&app, &github).await;
        sqlx::query(
            "INSERT INTO workspace_pull_request_provider_repos
                 (org_id, pull_request_id, repo_id, provider, provider_repo_full_name,
                  source_remote_branch, target_remote_branch, provider_pr_number,
                  provider_status, provider_head_sha)
             VALUES ($1, $2, $3, 'github', 'acme/api', 'feature', 'main', 42, 'open', $4)",
        )
        .bind(open.org.id)
        .bind(open.pull_request)
        .bind(open.child.repo_id)
        .bind(CHECK_SHA)
        .execute(&app.db)
        .await
        .unwrap();
        body(
            app.authenticated_client(&open.owner)
                .post_json(
                    &format!(
                        "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
                        open.parent, open.pull_request
                    ),
                    &json!({}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        Self {
            app,
            github,
            pull_request: open.pull_request,
        }
    }

    /// Runs `passes` consecutive passes continuing `pacing` and returns their
    /// requests and the pull request's status and waiting reason.
    async fn passes(
        &self,
        pacing: &mut g5_api::auto_merge::ChecksPacing,
        passes: usize,
    ) -> (BTreeMap<String, usize>, String, String) {
        let start = mark(&self.github).await;
        for _ in 0..passes {
            g5_api::auto_merge::run_once_paced(&self.app.state, pacing)
                .await
                .unwrap();
        }
        let (status, reason): (String, Option<String>) = sqlx::query_as(
            "SELECT status, auto_merge_reason FROM workspace_pull_requests WHERE id = $1",
        )
        .bind(self.pull_request)
        .fetch_one(&self.app.db)
        .await
        .unwrap();
        (
            tally(&self.github, start).await,
            status,
            reason.unwrap_or_default(),
        )
    }
}

/// Failing checks are read once and then back off; pending checks and
/// settling heads are read on every pass. The base branch's requirements are
/// read once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_passes_stay_within_budget() {
    let n = PASSES;
    for (scenario, status, conclusion, waiting, budget) in [
        (
            "failing checks, merge when ready",
            "completed",
            json!("failure"),
            "failing",
            vec![
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 2),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 2),
                ("GET /repos/acme/api/commits/{sha}/statuses", 2),
                ("GET /repos/acme/api/pulls/42", 2),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ],
        ),
        (
            "pending checks, merge when ready",
            "in_progress",
            Value::Null,
            "finish",
            vec![
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", n),
                ("GET /repos/acme/api/commits/{sha}/check-suites", n),
                ("GET /repos/acme/api/commits/{sha}/statuses", n),
                ("GET /repos/acme/api/pulls/42", n),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ],
        ),
        (
            "passing checks settling, merge when ready",
            "completed",
            json!("success"),
            "two minutes",
            vec![
                ("GET /repos/acme/api/branches/main", 1),
                ("GET /repos/acme/api/commits/{sha}/check-runs", 2 * n),
                ("GET /repos/acme/api/commits/{sha}/check-suites", 2 * n),
                ("GET /repos/acme/api/commits/{sha}/statuses", 2 * n),
                ("GET /repos/acme/api/pulls/42", 2 * n),
                ("GET /repos/acme/api/rules/branches/main", 1),
                ("POST /app/installations/555/access_tokens", 1),
            ],
        ),
    ] {
        let armed = Armed::new().await;
        require_tests(&armed.github).await;
        check_head(&armed.github, CHECK_SHA).await;
        revision_rows(&armed.github, CHECK_SHA, run(status, conclusion), json!([])).await;
        let mut pacing = held_pacing();
        let (actual, status, reason) = armed.passes(&mut pacing, PASSES).await;
        assert_eq!(status, "open", "{scenario}");
        assert!(reason.contains(waiting), "{scenario}: {reason}");
        assert_budget(scenario, &actual, &budget);
        armed.app.cleanup().await;
    }
}

/// A provider PR found merged ends the checks reads after the first pass:
/// that pass's settled read and merge attempt, whose reads the merge path
/// owns, are its only GitHub requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_stops_reading_a_merged_provider_pr() {
    let armed = Armed::new().await;
    mount_completed_pull(&armed.github, true).await;
    revision_rows(
        &armed.github,
        CHECK_SHA,
        run("completed", json!("success")),
        json!([]),
    )
    .await;
    let mut pacing = held_pacing();
    let (first, _, _) = armed.passes(&mut pacing, 1).await;
    // At least the settled checks read; the merge's own fresh reads vary.
    assert!(
        first
            .get("GET /repos/acme/api/pulls/42")
            .is_some_and(|reads| *reads >= 2),
        "{first:#?}"
    );
    let (rest, status, reason) = armed.passes(&mut pacing, PASSES - 1).await;
    // This fixture's provider row records no repository identity, so the merge
    // is refused and the pull request waits, still with nothing to read.
    assert_eq!(status, "open");
    assert!(reason.contains("identity"), "{reason}");
    assert_budget(
        "merged provider PR, merge when ready, after pass 1",
        &rest,
        &[],
    );
    armed.app.cleanup().await;
}

/// Background loops send nothing for an open pull request without a provider
/// PR: a GitHub-bound repository alone is no reason to read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_loops_send_nothing_without_a_provider_pr() {
    let github = MockServer::start().await;
    let app = github_app(&github).await;
    open_pull_request(&app, &github).await;
    let start = mark(&github).await;
    let mut pacing = held_pacing();
    for _ in 0..PASSES {
        g5_api::auto_merge::run_once_paced(&app.state, &mut pacing)
            .await
            .unwrap();
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap();
    }
    assert_budget("open local pull request", &tally(&github, start).await, &[]);
    app.cleanup().await;
}
