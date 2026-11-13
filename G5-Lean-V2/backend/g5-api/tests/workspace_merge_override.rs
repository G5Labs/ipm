//! Merge with override (`g5.workspace.merge.admin-override` and its formal
//! child, `g5.security.browser-session-actions`;
//! `.g5/pr_ontology_diff_design/issue-873`): who may override, the shown set
//! the merge re-checks, the record, the bypassed requests and the
//! announcement, against live Postgres and the real routes.

use crate::helpers;
use crate::helpers::review_flow::{body, Flow};

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

/// A pull request into the flow's parent with the parent's required
/// approval requested from two reviewers and still pending.
struct Blocked {
    flow: Flow,
    pull_request: Uuid,
}

impl Blocked {
    async fn new(app: &TestApp) -> Self {
        Self::with_flow(app, Flow::new(app).await).await
    }

    async fn with_flow(app: &TestApp, flow: Flow) -> Self {
        flow.publish_file(app, flow.parent.repo_id, "feature.txt", b"feature\n")
            .await;
        let submitted = flow.submit(app).await;
        let pull_request = submitted["pullRequest"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        flow.request(app, "ie").await;
        Self { flow, pull_request }
    }

    fn path(&self) -> String {
        format!(
            "/api/v1/workspaces/{}/pull-requests/{}/merge-override",
            self.flow.parent.id, self.pull_request
        )
    }

    async fn status(&self, app: &TestApp) -> String {
        sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id = $1")
            .bind(self.pull_request)
            .fetch_one(&app.db)
            .await
            .unwrap()
    }

    async fn request_states(&self, app: &TestApp) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT state FROM approval_requests WHERE pull_request_id = $1 ORDER BY seq",
        )
        .bind(self.pull_request)
        .fetch_all(&app.db)
        .await
        .unwrap()
    }

    async fn override_events(&self, app: &TestApp) -> Vec<(Uuid, Option<String>)> {
        sqlx::query_as(
            "SELECT actor_id, message FROM workspace_pull_request_events
              WHERE pull_request_id = $1 AND event_type = 'merge_overridden'",
        )
        .bind(self.pull_request)
        .fetch_all(&app.db)
        .await
        .unwrap()
    }

    /// Durable state an authorization refusal must leave untouched, including
    /// actual Git refs rather than just the PR's cached merge status.
    async fn merge_state(&self, app: &TestApp) -> Value {
        let mut db = app.scoped_db_for_org(self.flow.org.id).await;
        let cut = g5_api::services::workspace_reader::resolve_committed_snapshot(
            &mut db,
            &app.repo_storage,
            g5_contracts::types::AuthorityTarget::Workspace {
                workspace_id: self.flow.parent.id,
            },
        )
        .await
        .unwrap();
        let heads: Vec<_> = cut
            .repos
            .iter()
            .map(|repo| json!([repo.repo_id, repo.branch_name, repo.head_commit]))
            .collect();
        db.rollback().await.unwrap();
        let state: Value = sqlx::query_scalar(
            "SELECT jsonb_build_object(
                'pr', to_jsonb(pr),
                'requests', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.seq), '[]')
                    FROM approval_requests r WHERE r.pull_request_id = pr.id),
                'decisions', (SELECT coalesce(jsonb_agg(to_jsonb(s) ORDER BY s.seq), '[]')
                    FROM approval_statuses s WHERE s.source_workspace_id = pr.source_workspace_id),
                'overrides', (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.created_at), '[]')
                    FROM workspace_pull_request_events e
                    WHERE e.pull_request_id = pr.id AND e.event_type = 'merge_overridden'))
             FROM workspace_pull_requests pr WHERE pr.id = $1",
        )
        .bind(self.pull_request)
        .fetch_one(&app.db)
        .await
        .unwrap();
        json!({"heads": heads, "state": state})
    }

    async fn transfer_target(&self, app: &TestApp) {
        self.transfer_workspace(app, self.flow.parent.id).await;
    }

    async fn transfer_workspace(&self, app: &TestApp, workspace_id: Uuid) {
        let transferred = body(
            app.authenticated_client(&self.flow.owner)
                .post_json(
                    &format!("/api/v1/workspaces/{workspace_id}/transfer"),
                    &json!({
                        "toUserId": self.flow.second.id,
                        "idempotencyKey": Uuid::new_v4().to_string(),
                    }),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(transferred["status"], "completed", "{transferred}");
    }

    async fn prepare_override(
        &self,
        app: &TestApp,
    ) -> g5_api::services::coordinated_repo_publication::PreparedLocalWorkspacePullRequestMerge
    {
        use g5_api::services::coordinated_repo_publication::MergeWorkspacePullRequestOutcome;
        use g5_api::services::git::merge_override::{
            current_merge_override_authority, MergeOverrideIntent,
        };

        let f = &self.flow;
        let token = browser_session(app, &f.owner, f.org.id).await;
        let (status, shown) = preview(app, &self.path(), &token, true).await;
        assert_eq!(status, StatusCode::OK, "{shown}");
        let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
        let authority = current_merge_override_authority(&mut db, f.source, f.parent.id, true)
            .await
            .unwrap();
        assert!(authority.g5.is_allowed());
        let intent = MergeOverrideIntent {
            actor_id: f.owner.id,
            actor_name: Some(f.owner.display_name.clone()),
            reason: "Approver is away this week".into(),
            self_override: true,
            expected_digest: shown["data"]["digest"].as_str().unwrap().into(),
            authority,
            checks: vec![],
            github: vec![],
        };
        let prepared = app
            .state
            .coordinated_repo_publication_service
            .merge_workspace_pull_request(
                db,
                &app.state.workspace_merge_service,
                f.parent.id,
                self.pull_request,
                f.owner.id,
                Some(&intent),
            )
            .await
            .unwrap();
        let MergeWorkspacePullRequestOutcome::PreparedLocal(prepared) = prepared else {
            panic!("fixture must prepare a local merge");
        };
        *prepared
    }
}

/// A person session the way the web sign-in issues it.
async fn browser_session(app: &TestApp, user: &TestUser, org: Uuid) -> String {
    app.state
        .auth_service
        .issue_user_tokens(user.id, Some(org), g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .unwrap()
        .jwt
}

/// A person session the way CLI device login issues it.
async fn cli_session(app: &TestApp, user: &TestUser, org: Uuid) -> String {
    app.state
        .auth_service
        .issue_user_tokens(user.id, Some(org), g5_auth::SESSION_CLIENT_CLI)
        .await
        .unwrap()
        .jwt
}

/// A request as the web app's same-origin fetch sends it: fetch metadata,
/// and no `Origin` (browsers omit it on same-origin reads).
fn from_browser(
    _app: &TestApp,
    request: reqwest::RequestBuilder,
    token: &str,
) -> reqwest::RequestBuilder {
    request
        .bearer_auth(token)
        .header("Sec-Fetch-Site", "same-origin")
}

async fn preview(app: &TestApp, path: &str, token: &str, browser: bool) -> (StatusCode, Value) {
    let request = app.client.get(format!("{}{path}", app.base_url));
    let request = if browser {
        from_browser(app, request, token)
    } else {
        request.bearer_auth(token)
    };
    let response = request.send().await.unwrap();
    (
        response.status(),
        response.json().await.unwrap_or(Value::Null),
    )
}

async fn confirm(
    app: &TestApp,
    path: &str,
    token: &str,
    browser: bool,
    digest: &str,
) -> (StatusCode, Value) {
    let request = app.client.post(format!("{}{path}", app.base_url)).json(
        &json!({"reason": "Approver is away this week", "confirmed": true, "digest": digest}),
    );
    let request = if browser {
        from_browser(app, request, token)
    } else {
        request.bearer_auth(token)
    };
    let response = request.send().await.unwrap();
    (
        response.status(),
        response.json().await.unwrap_or(Value::Null),
    )
}

async fn notifications(app: &TestApp, recipient: Uuid) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT title, message FROM notifications
          WHERE recipient_user_id = $1 AND category = 'pull_request' ORDER BY created_at",
    )
    .bind(recipient)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_non_admin_source_owner_cannot_waive_g5_but_can_ship_approved_work() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    let request = f.primary_request(&app, "ie").await;
    blocked.transfer_target(&app).await;
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let token = browser_session(&app, &f.owner, f.org.id).await;
    let before = blocked.merge_state(&app).await;

    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(
        shown["data"]["authority"]["g5"]["allowed"], false,
        "{shown}"
    );
    assert_eq!(shown["data"]["authority"]["github"]["allowed"], true);
    assert_eq!(
        shown["data"]["blockers"]["approvals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, refused) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_override_not_authorized(status, &refused);
    assert_eq!(blocked.merge_state(&app).await, before);

    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let merged = body(
        app.authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/merge",
                    f.parent.id, blocked.pull_request
                ),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(merged["pullRequest"]["status"], "merged");
    assert_eq!(merged["pullRequest"]["mergedBy"], f.owner.id.to_string());
    assert!(merged["pullRequest"]["mergeOverride"].is_null());
    assert!(blocked.override_events(&app).await.is_empty());
    app.cleanup().await;
}

fn assert_override_not_authorized(status: StatusCode, refused: &Value) {
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("merge_override_not_authorized"),
        "{refused}"
    );
}

async fn wait_for_blocked_by(app: &TestApp, blocker: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("merge reached the held transaction lock");
}

/// The route authorized the old owner, then released that transaction. A
/// transfer completes while the service's reopened admission waits: the
/// captured browser authority must not authorize the eventual waiver.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_transfer_during_override_admission_refuses_without_merge_effects() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    blocked.transfer_target(&app).await;
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let path = blocked.path();
    let (status, shown) = preview(&app, &path, &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(shown["data"]["authority"]["g5"]["allowed"], true);
    {
        let mut gate = app.db.begin().await.unwrap();
        let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended(
            'g5:approval-config:' || $1::text || ':' || $2::text, 0))",
        )
        .bind(blocked.flow.org.id)
        .bind(blocked.flow.parent.id)
        .execute(&mut *gate)
        .await
        .unwrap();
        let merging = confirm(
            &app,
            &path,
            &token,
            true,
            shown["data"]["digest"].as_str().unwrap(),
        );
        tokio::pin!(merging);
        tokio::select! {
            result = &mut merging => panic!("merge finished before admission was released: {result:?}"),
            () = wait_for_blocked_by(&app, blocker) => {}
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            blocked.transfer_workspace(&app, blocked.flow.source),
        )
        .await
        .expect("transfer completes before the waiting merge resumes");
        let before = blocked.merge_state(&app).await;
        gate.commit().await.unwrap();
        let (status, refused) = merging.await;
        assert_override_not_authorized(status, &refused);
        assert_eq!(blocked.merge_state(&app).await, before);
    }
    app.cleanup().await;
}

/// A concurrent transfer waits for held authority. If it wins between merge
/// transactions, the merge refuses; otherwise landing and settlement precede
/// the transfer. Neither ordering may permit a stale-owner effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn override_and_target_transfer_serialize_or_refuse() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let path = blocked.path();
    let (status, shown) = preview(&app, &path, &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    let before = blocked.merge_state(&app).await;
    {
        let mut gate = app.db.begin().await.unwrap();
        let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        g5_api::services::branch_commit::acquire_workspace_commit_fence(
            &mut gate,
            blocked.flow.parent.id,
        )
        .await
        .unwrap();
        let merging = confirm(
            &app,
            &path,
            &token,
            true,
            shown["data"]["digest"].as_str().unwrap(),
        );
        tokio::pin!(merging);
        tokio::select! {
            result = &mut merging => panic!("merge finished before application was released: {result:?}"),
            () = wait_for_blocked_by(&app, blocker) => {}
        }
        let merge_pid: i32 = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) LIMIT 1",
        )
        .bind(blocker)
        .fetch_one(&app.db)
        .await
        .unwrap();
        let transferring = blocked.transfer_target(&app);
        tokio::pin!(transferring);
        tokio::select! {
            () = &mut transferring => panic!("transfer must wait for the admitted mutation"),
            () = wait_for_blocked_by(&app, merge_pid) => {}
        }
        gate.commit().await.unwrap();
        let (merged, ()) = tokio::join!(&mut merging, &mut transferring);
        if merged.0 == StatusCode::OK {
            assert_eq!(blocked.status(&app).await, "merged");
            assert_eq!(blocked.override_events(&app).await.len(), 1);
        } else {
            assert_eq!(merged.0, StatusCode::CONFLICT, "{}", merged.1);
            assert!(
                merged.1["error"]["code"] == "WORKSPACE_STOPPING"
                    || merged.1["error"]["details"]["code"] == "merge_override_not_authorized",
                "{}",
                merged.1,
            );
            assert_eq!(blocked.merge_state(&app).await, before);
        }
    }
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_override_preview_does_not_survive_membership_revocation() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    // Retain another active Admin when the workspace owner loses activity.
    sqlx::query(
        "UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id IN ($2, $3)",
    )
    .bind(blocked.flow.org.id)
    .bind(blocked.flow.owner.id)
    .bind(blocked.flow.second.id)
    .execute(&app.db)
    .await
    .unwrap();
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    let before = blocked.merge_state(&app).await;
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = now() WHERE org_id = $1 AND user_id = $2",
    )
    .bind(blocked.flow.org.id)
    .bind(blocked.flow.owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, refused) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(blocked.merge_state(&app).await, before);
    app.cleanup().await;
}

/// Prepared admission does not preserve revoked authority. Each mutation is
/// performed after preparation released its locks, before local application.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_prepared_g5_override_rechecks_each_revocable_authority() {
    for change in ["source_owner", "last_requirement", "admin_role"] {
        let app = TestApp::spawn().await;
        let blocked = Blocked::new(&app).await;
        let f = &blocked.flow;
        blocked.transfer_target(&app).await;
        let prepared = blocked.prepare_override(&app).await;
        match change {
            "source_owner" => blocked.transfer_workspace(&app, f.source).await,
            "last_requirement" => {
                body(
                    app.authenticated_client(&f.owner)
                        .post_json(
                            &format!(
                                "/api/v1/workspaces/{}/approvals/requirements/{}/retire",
                                f.parent.id, f.definition
                            ),
                            &json!({}),
                        )
                        .await,
                    StatusCode::OK,
                )
                .await;
            }
            "admin_role" => {
                sqlx::query(
                    "UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2",
                )
                .bind(f.org.id)
                .bind(f.owner.id)
                .execute(&app.db)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = blocked.merge_state(&app).await;
        let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
        let result = app
            .state
            .coordinated_repo_publication_service
            .apply_prepared_workspace_pull_request_merge(
                &mut db,
                &app.state.workspace_merge_service,
                prepared,
            )
            .await;
        assert!(
            matches!(result,
                Err(g5_contracts::G5Error::ConflictDetails { ref details, .. })
                    if details["code"] == "merge_override_not_authorized"
            ),
            "authority change: {change}; {:?}",
            result.err()
        );
        db.rollback().await.unwrap();
        assert_eq!(blocked.merge_state(&app).await, before, "{change}");
        app.cleanup().await;
    }
}

/// A competing merge owns the commit fence before locking workspace rows.
/// Waiting override application must not hold a row that writer still needs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_base_override_waiting_for_a_commit_fence_does_not_block_its_writer() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::with_flow(&app, Flow::with_base_parent(&app).await).await;
    let f = &blocked.flow;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(f.parent.id)
        .bind(f.second.id)
        .execute(&app.db)
        .await
        .unwrap();
    let prepared = blocked.prepare_override(&app).await;
    let before = blocked.merge_state(&app).await;
    let writer_lock = {
        let mut gate = app.db.begin().await.unwrap();
        let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *gate)
            .await
            .unwrap();
        let mut workspaces = [f.source, f.parent.id];
        workspaces.sort();
        for workspace_id in workspaces {
            g5_api::services::branch_commit::acquire_workspace_commit_fence(
                &mut gate,
                workspace_id,
            )
            .await
            .unwrap();
        }
        let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
        let writer_lock = {
            let applying = app
                .state
                .coordinated_repo_publication_service
                .apply_prepared_workspace_pull_request_merge(
                    &mut db,
                    &app.state.workspace_merge_service,
                    prepared,
                );
            tokio::pin!(applying);
            tokio::select! {
                result = &mut applying => panic!("application finished before the writer: {:?}", result.err()),
                () = wait_for_blocked_by(&app, blocker) => {}
            }
            let writer_lock =
                sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE NOWAIT")
                    .bind(f.parent.id)
                    .execute(&mut *gate)
                    .await;
            gate.rollback().await.unwrap();
            applying.await.unwrap();
            writer_lock
        };
        db.rollback().await.unwrap();
        writer_lock
    };
    assert_eq!(blocked.merge_state(&app).await, before);
    app.cleanup().await;
    assert!(
        writer_lock.is_ok(),
        "fenced writer was blocked: {writer_lock:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn target_ownership_does_not_replace_current_organization_administration() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    let before = blocked.merge_state(&app).await;
    sqlx::query("UPDATE org_memberships SET role = 'member' WHERE org_id = $1 AND user_id = $2")
        .bind(blocked.flow.org.id)
        .bind(blocked.flow.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, current) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(
        current["data"]["authority"]["g5"]["allowed"], false,
        "{current}"
    );
    let (status, refused) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_override_not_authorized(status, &refused);
    assert_eq!(blocked.merge_state(&app).await, before);
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_owning_admin_can_override_after_target_transfer() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(shown["data"]["authority"]["g5"]["allowed"], true);

    blocked.transfer_target(&app).await;
    let (status, merged) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(blocked.status(&app).await, "merged");
    assert_eq!(blocked.override_events(&app).await.len(), 1);
    assert!(blocked
        .request_states(&app)
        .await
        .iter()
        .all(|state| state == "bypassed"));
    assert_eq!(
        merged["data"]["pullRequest"]["mergeOverride"]["selfOverride"],
        true
    );
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_admin_can_override_into_base_owned_by_someone_else() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::with_flow(&app, Flow::with_base_parent(&app).await).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(blocked.flow.parent.id)
        .bind(blocked.flow.second.id)
        .execute(&app.db)
        .await
        .unwrap();
    let token = browser_session(&app, &blocked.flow.owner, blocked.flow.org.id).await;
    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(shown["data"]["authority"]["g5"]["allowed"], true);
    assert_eq!(
        shown["data"]["blockers"]["approvals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, merged) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["data"]["pullRequest"]["status"], "merged");
    assert_eq!(
        merged["data"]["pullRequest"]["mergeOverride"]["actorId"],
        blocked.flow.owner.id.to_string()
    );
    assert_eq!(blocked.override_events(&app).await.len(), 1);
    assert!(blocked
        .request_states(&app)
        .await
        .iter()
        .all(|state| state == "bypassed"));
    app.cleanup().await;
}

/// An administrator who may merge, in the web app, sees exactly the unmet
/// requirement, gives a reason and confirms: the pull request merges, the
/// admission records the override, pending requests close as bypassed and
/// never as decided, and the reviewers learn of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_administrator_merges_with_override_and_it_stays_flagged() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    let token = browser_session(&app, &f.owner, f.org.id).await;

    let (status, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    let shown = &shown["data"];
    let approvals = shown["blockers"]["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 1, "{shown}");
    assert_eq!(approvals[0]["definitionName"], "Security review");
    assert_eq!(approvals[0]["state"], "pending");
    assert_eq!(shown["blockers"]["checks"], json!([]));
    assert_eq!(shown["blockers"]["github"], json!([]));
    assert_eq!(shown["notOverridable"], json!([]));
    assert_eq!(shown["authority"]["g5"]["allowed"], true);
    // The flow's owner owns the source workspace too.
    assert_eq!(shown["selfOverride"], true);

    let (status, merged) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["digest"].as_str().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    let pull_request = &merged["data"]["pullRequest"];
    assert_eq!(pull_request["status"], "merged");
    assert_eq!(pull_request["mergedBy"], f.owner.id.to_string());
    let record = &pull_request["mergeOverride"];
    assert_eq!(record["actorId"], f.owner.id.to_string(), "{pull_request}");
    assert_eq!(record["reason"], "Approver is away this week");
    assert_eq!(record["selfOverride"], true);
    assert_eq!(record["waivedApprovals"].as_array().unwrap().len(), 1);
    assert_eq!(pull_request["mergeAdmission"]["admitted"], true);

    assert_eq!(
        blocked.override_events(&app).await,
        vec![(f.owner.id, Some("Approver is away this week".to_string()))]
    );
    let states = blocked.request_states(&app).await;
    assert!(!states.is_empty());
    assert!(states.iter().all(|state| state == "bypassed"), "{states:?}");
    let decisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE source_workspace_id = $1")
            .bind(f.source)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(decisions, 0, "the override decided nothing");

    for reviewer in [&f.reviewer, &f.second] {
        let told = notifications(&app, reviewer.id).await;
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].0.ends_with("with override"), "{told:?}");
        assert!(
            told[0].1.contains("Merged with override by")
                && told[0].1.contains("(a self-override)")
                && told[0].1.contains("Approver is away this week")
                && told[0]
                    .1
                    .contains("required approval Security review (pending)"),
            "{told:?}"
        );
    }
    assert!(
        notifications(&app, f.owner.id).await.is_empty(),
        "whoever overrode is not told of their own merge"
    );

    // Both workspaces' Activity show the override: the source among its
    // pull request's events, the target among the merges that landed in it.
    for (workspace, incoming) in [(f.source, false), (f.parent.id, true)] {
        let activity = body(
            app.authenticated_client(&f.owner)
                .get(&format!("/api/v1/workspaces/{workspace}/activity"))
                .await,
            StatusCode::OK,
        )
        .await;
        let rows: Vec<&Value> = activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| {
                entry["kind"] == "pull_request"
                    && entry["pullRequestId"] == blocked.pull_request.to_string()
            })
            .collect();
        let overridden = rows
            .iter()
            .find(|entry| entry["eventType"] == "merge_overridden")
            .unwrap_or_else(|| panic!("{workspace}: {activity}"));
        assert_eq!(overridden["message"], "Approver is away this week");
        assert_eq!(overridden["actor"]["userId"], f.owner.id.to_string());
        assert_eq!(overridden["incoming"], incoming);
        assert!(rows.iter().any(|entry| entry["eventType"] == "merged"));
        if incoming {
            // The target shows how incoming work landed, not its lifecycle.
            assert!(
                rows.iter().all(|entry| matches!(
                    entry["eventType"].as_str(),
                    Some("merged" | "merge_overridden")
                )),
                "{activity}"
            );
        }
    }

    // The pull request read keeps the flag.
    let info = body(
        app.authenticated_client(&f.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/pull-requests/info",
                f.source
            ))
            .await,
        StatusCode::OK,
    )
    .await;
    let read = info["pullRequests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pull| pull["id"] == blocked.pull_request.to_string())
        .unwrap();
    assert_eq!(
        read["mergeOverride"]["reason"], "Approver is away this week",
        "{info}"
    );
    app.cleanup().await;
}

/// The same request changes nothing from anyone but an administrator who may
/// merge, in a browser session from the web app: not a member, another
/// organization's administrator, the administrator's CLI session, API key or
/// a session from before sessions recorded their kind, a request without the
/// browser's headers, or a workspace agent's credential.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn override_is_refused_without_effect_for_every_other_caller() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    let path = blocked.path();
    let token = browser_session(&app, &f.owner, f.org.id).await;
    let (_, shown) = preview(&app, &path, &token, true).await;
    let digest = shown["data"]["digest"].as_str().unwrap().to_string();

    // A member who may read but not merge, in a browser session.
    let member = browser_session(&app, &f.reviewer, f.org.id).await;
    assert_eq!(
        preview(&app, &path, &member, true).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        confirm(&app, &path, &member, true, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    // A writing member who owns nothing here.
    let writer = TestUser::create_with_scopes(
        &app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let writer = browser_session(&app, &writer, f.org.id).await;
    assert_eq!(
        confirm(&app, &path, &writer, true, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    let admin = browser_session(&app, &f.reviewer, f.org.id).await;
    assert_eq!(
        preview(&app, &path, &admin, true).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        confirm(&app, &path, &admin, true, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    // Another organization's administrator.
    let mut outsider = TestUser::create(&app).await;
    let other = TestOrg::create_with_admin(&app, &mut outsider).await;
    let outsider = browser_session(&app, &outsider, other.id).await;
    let (status, _) = confirm(&app, &path, &outsider, true, &digest).await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "{status}"
    );
    // The administrator's CLI session, even with browser headers.
    let cli = cli_session(&app, &f.owner, f.org.id).await;
    let (status, refused) = confirm(&app, &path, &cli, true, &digest).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert!(
        refused.to_string().contains("browser_session_required"),
        "{refused}"
    );
    // A session without a recorded kind (the test fixtures' tokens).
    assert_eq!(
        confirm(&app, &path, &f.owner.jwt, true, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    // The browser session without the browser's headers.
    assert_eq!(
        confirm(&app, &path, &token, false, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    // The administrator's all-scopes organization API key.
    let created = body(
        app.authenticated_client(&f.owner)
            .post_json(
                &format!("/api/v1/orgs/{}/api-keys", f.org.id),
                &json!({"name": "admin key", "scopes": ["org:admin", "workspaces:read", "workspaces:write"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let key = created["key"].as_str().unwrap().to_string();
    assert_eq!(
        confirm(&app, &path, &key, true, &digest).await.0,
        StatusCode::FORBIDDEN
    );
    // A workspace agent's credential is not accepted on the person route.
    let vm = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.source, f.owner.id, Uuid::new_v4(), 1, 3600)
        .unwrap();
    assert_eq!(
        confirm(&app, &path, &vm, true, &digest).await.0,
        StatusCode::UNAUTHORIZED
    );

    assert_eq!(blocked.status(&app).await, "open");
    assert!(blocked.override_events(&app).await.is_empty());
    assert!(blocked
        .request_states(&app)
        .await
        .iter()
        .all(|state| state == "pending"));

    // Still possible for the administrator in the browser.
    let (status, merged) = confirm(&app, &path, &token, true, &digest).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    app.cleanup().await;
}

/// The merge re-checks the shown set: a new rejection or a push after the
/// dialog opened refuses with the current set and records nothing; with
/// nothing left to waive the ordinary merge applies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_changed_set_refuses_and_returns_what_would_be_waived_now() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    let path = blocked.path();
    let token = browser_session(&app, &f.owner, f.org.id).await;
    let (_, shown) = preview(&app, &path, &token, true).await;
    let digest = shown["data"]["digest"].as_str().unwrap().to_string();

    // A reviewer rejects after the dialog opened.
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "rejected").await,
        StatusCode::OK,
    )
    .await;
    let (status, refused) = confirm(&app, &path, &token, true, &digest).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    let details = &refused["error"]["details"];
    assert_eq!(details["code"], "merge_override_changed", "{refused}");
    assert_eq!(details["blockers"]["approvals"][0]["state"], "rejected");
    assert_eq!(blocked.status(&app).await, "open");
    assert!(blocked.override_events(&app).await.is_empty());

    // The current set is overridable once shown.
    let (_, shown) = preview(&app, &path, &token, true).await;
    assert_eq!(
        shown["data"]["blockers"]["approvals"][0]["state"],
        "rejected"
    );
    let current = shown["data"]["digest"].as_str().unwrap().to_string();
    assert_ne!(current, digest);

    // A push after it was shown changes the source version.
    f.publish_file(&app, f.parent.repo_id, "later.txt", b"later\n")
        .await;
    let (status, refused) = confirm(&app, &path, &token, true, &current).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(blocked.override_events(&app).await.is_empty());
    app.cleanup().await;
}

/// Nothing to waive: the override refuses and the ordinary merge applies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_override_with_nothing_to_waive_refuses() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    let token = browser_session(&app, &f.owner, f.org.id).await;
    let (_, shown) = preview(&app, &blocked.path(), &token, true).await;
    assert_eq!(shown["data"]["blockers"]["approvals"], json!([]), "{shown}");
    let (status, refused) = confirm(
        &app,
        &blocked.path(),
        &token,
        true,
        shown["data"]["digest"].as_str().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["error"]["details"]["code"],
        "merge_override_not_needed"
    );
    assert_eq!(blocked.status(&app).await, "open");
    app.cleanup().await;
}

/// Merge when ready never becomes an override, even when its author could
/// make one: it keeps waiting on the unmet approval.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_never_overrides() {
    let app = TestApp::spawn().await;
    let blocked = Blocked::new(&app).await;
    let f = &blocked.flow;
    body(
        app.authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
                    f.parent.id, blocked.pull_request
                ),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(g5_api::auto_merge::run_once(&app.state).await.unwrap(), 0);
    assert_eq!(blocked.status(&app).await, "open");
    assert!(blocked.override_events(&app).await.is_empty());
    app.cleanup().await;
}
