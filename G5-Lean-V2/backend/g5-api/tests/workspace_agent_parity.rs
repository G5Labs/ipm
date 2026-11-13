//! Workspace agents coordinating their owner's workspaces and reading what
//! their owner can read, with the VM's own credential, and stop workspaces by user
//! (G5-Lean#710; `.g5/pr_ontology_diff_design/issue-710-workspace-agent-parity`).
//!
//! Every case drives the real routes against live Postgres: the
//! `/api/internal/workspace-agent` routes with a workspace-incarnation token,
//! `POST /api/v1/orgs/:orgId/members/:userId/stop-workspaces` with person
//! sessions, and the lifecycle reconciler that drains a held membership.

use crate::helpers;

use g5_api::services::{WorkspaceVmService, WorkspaceVmServiceConfig};
use g5_auth::Scope;
use g5_scm::{GitAuthor, GitRepo};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

/// An organization with a member whose workspace runs an agent (`parent`,
/// with a live runtime peer), that workspace's child, and a second member.
struct Fixture {
    app: TestApp,
    org: TestOrg,
    admin: TestUser,
    member: TestUser,
    other: TestUser,
    parent: TestWorkspace,
    child: TestWorkspace,
    peer: TestWorkspaceRuntimePeer,
    token: String,
}

async fn set_owner(app: &TestApp, workspace: Uuid, owner: Uuid) {
    sqlx::query("UPDATE workspaces SET owner_user_id = $2, created_by = $2 WHERE id = $1")
        .bind(workspace)
        .bind(owner)
        .execute(&app.db)
        .await
        .unwrap();
}

/// The owner's setting for whether `workspace`'s agent may change other workspaces
/// (`g5.security.agent-other-workspaces`), written directly.
async fn set_agent_opt_in(app: &TestApp, workspace: Uuid, on: bool) {
    sqlx::query(
        "UPDATE workspace_settings SET agent_may_modify_other_workspaces = $2
         WHERE workspace_id = $1",
    )
    .bind(workspace)
    .bind(on)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn agent_opt_in(app: &TestApp, workspace: Uuid) -> bool {
    sqlx::query_scalar(
        "SELECT agent_may_modify_other_workspaces FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(workspace)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

fn assert_opt_in_refused(status: StatusCode, body: &Value, what: &str) {
    assert_eq!(status, StatusCode::FORBIDDEN, "{what}: {body}");
    assert_eq!(
        body["error"]["code"], "AGENT_OTHER_WORKSPACES_DISABLED",
        "{what}: {body}"
    );
}

async fn set_parent(app: &TestApp, workspace: Uuid, parent: Uuid) {
    sqlx::query("UPDATE workspaces SET cloned_from_id = $2 WHERE id = $1")
        .bind(workspace)
        .bind(parent)
        .execute(&app.db)
        .await
        .unwrap();
}

impl Fixture {
    async fn new() -> Self {
        Self::with_app(TestApp::spawn().await).await
    }

    async fn with_app(app: TestApp) -> Self {
        let mut admin = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut admin).await;
        let member = TestUser::create_with_scopes(
            &app,
            org.id,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
        )
        .await;
        let other = TestUser::create_with_scopes(
            &app,
            org.id,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
        )
        .await;
        let parent = TestWorkspace::create(&app, &org).await;
        set_owner(&app, parent.id, member.id).await;
        let child = TestWorkspace::create(&app, &org).await;
        set_owner(&app, child.id, member.id).await;
        set_parent(&app, child.id, parent.id).await;
        // The default lets this agent change its children; the setting's
        // own cases turn it off.
        let peer = TestWorkspaceRuntimePeer::attach(&app, &parent, member.id).await;
        let token = app
            .state
            .jwt_minter
            .as_ref()
            .unwrap()
            .mint_workspace_incarnation(org.id, parent.id, member.id, peer.vm_id(), 1, 3600)
            .unwrap();
        Self {
            app,
            org,
            admin,
            member,
            other,
            parent,
            child,
            peer,
            token,
        }
    }

    fn agent_path(&self, leaf: &str) -> String {
        format!(
            "{}/api/internal/workspace-agent/{}/{leaf}",
            self.app.base_url, self.parent.id
        )
    }

    async fn agent_get(&self, leaf: &str) -> (StatusCode, Value) {
        let response = self
            .app
            .client
            .get(self.agent_path(leaf))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn agent_post(&self, leaf: &str, body: Value) -> (StatusCode, Value) {
        agent_post_with(&self.app, &self.token, &self.agent_path(leaf), body).await
    }

    /// A workspace owned by `owner`, child of `parent` when given.
    async fn workspace(&self, owner: Uuid, parent: Option<Uuid>) -> TestWorkspace {
        let workspace = TestWorkspace::create(&self.app, &self.org).await;
        set_owner(&self.app, workspace.id, owner).await;
        if let Some(parent) = parent {
            set_parent(&self.app, workspace.id, parent).await;
        }
        workspace
    }

    async fn worker(&self) -> WorkspaceVmService {
        let db = self.app.scoped_db_for_org(self.org.id).await;
        let scope = db.reopener();
        db.rollback().await.unwrap();
        WorkspaceVmService::new(WorkspaceVmServiceConfig {
            system_pool: self.app.system_pool.clone(),
            scope,
            hosts: self.app.state.preview_tunnels.clone(),
            minter: self.app.state.jwt_minter.clone().unwrap(),
            runtime: self.app.state.workspace_runtime.clone(),
            agent_work: self.app.state.agent_work.clone(),
            repo_storage: self.app.repo_storage.clone(),
            git_external_url: self.app.state.git_external_url.clone(),
            ws_hub: self.app.state.event_bus.ws_hub().clone(),
        })
    }

    async fn hold(&self, user: Uuid) -> Option<Uuid> {
        sqlx::query_scalar(
            "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id = $1 AND user_id = $2",
        )
        .bind(self.org.id)
        .bind(user)
        .fetch_one(&self.app.db)
        .await
        .unwrap()
    }

    async fn finish(self) {
        drop(self.peer);
        self.app.cleanup().await;
    }
}

async fn agent_post_with(
    app: &TestApp,
    token: &str,
    url: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// The `workspace_agent.action` audit events of the agent in `caller`, in
/// emission order.
fn agent_audit(
    events: &g5_api::middleware::test_capture::Events,
    caller: Uuid,
) -> Vec<g5_api::middleware::test_capture::CapturedEvent> {
    g5_api::middleware::test_capture::captured_g5_audit_events(events)
        .into_iter()
        .filter(|event| {
            event.field("event") == Some("workspace_agent.action")
                && event.field("caller_workspace_id") == Some(caller.to_string().as_str())
        })
        .collect()
}

async fn person_post(app: &TestApp, user: &TestUser, path: &str) -> (StatusCode, Value) {
    let response = app
        .authenticated_client(user)
        .post_json(path, &json!({}))
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn insert_chat(app: &TestApp, org: Uuid, workspace: Uuid, owner: Uuid) -> Uuid {
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, created_by, title, message_count)
         VALUES ($1, $2, $3, $4, 'Investigation', 2)",
    )
    .bind(session)
    .bind(org)
    .bind(workspace)
    .bind(owner)
    .execute(&app.db)
    .await
    .unwrap();
    for (index, (role, content)) in [
        ("user", "Find the bug"),
        ("assistant", "The bug is in parse()"),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "INSERT INTO chat_messages(id, org_id, session_id, role, content, status, message_index)
             VALUES ($1, $2, $3, $4, $5, 'complete', $6)",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(session)
        .bind(role)
        .bind(content)
        .bind(index as i32)
        .execute(&app.db)
        .await
        .unwrap();
    }
    let task = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(id, org_id, workspace_id, name, kind, status, created_by, chat_session_id)
         VALUES ($1, $2, $3, 'Investigation', 'agent', 'succeeded', $4, $5)",
    )
    .bind(task)
    .bind(org)
    .bind(workspace)
    .bind(owner)
    .bind(session)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO task_events(id, org_id, task_id, entry_type, content, metadata, attempt, seq)
         VALUES ($1, $2, $3, 'output', 'cargo test: 1 failed', '{}', 1, 1)",
    )
    .bind(Uuid::new_v4())
    .bind(org)
    .bind(task)
    .execute(&app.db)
    .await
    .unwrap();
    session
}

// ─── Stop workspaces by user ─────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_stops_its_owners_workspaces_and_the_hold_clears_once_settled() {
    let f = Fixture::new().await;
    // Another owner's fork of one of the member's workspaces stays outside.
    let foreign = f.workspace(f.other.id, Some(f.parent.id)).await;

    let (status, body) = f.agent_post("stop-owner-workspaces", json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let hold: Uuid = body["data"]["holdId"].as_str().unwrap().parse().unwrap();
    assert_eq!(f.hold(f.member.id).await, Some(hold));
    assert_eq!(f.hold(f.other.id).await, None);
    let names: Vec<_> = body["data"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["workspaceId"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&f.parent.id.to_string()), "{body}");
    assert!(!names.contains(&foreign.id.to_string()), "{body}");

    // A repeated request joins the pending hold.
    let (status, body) = f.agent_post("stop-owner-workspaces", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["joined"], true);
    assert_eq!(body["data"]["holdId"], hold.to_string());

    // A fork that finishes during the pass is created stopped: its first
    // message and its launch are refused, so it never attaches compute.
    let (status, body) = f
        .agent_post(
            "fork",
            json!({"name": "during-hold", "prompt": "work", "idempotencyKey": "hold-fork"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(body["data"]["startedChat"]["error"].is_string(), "{body}");
    assert!(body["data"]["startedChat"]["sessionId"].is_null(), "{body}");
    let forked: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let attached: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE workspace_id = $1
           AND (status <> 'stopped' OR start_intent IS NOT NULL)",
    )
    .bind(forked)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(attached, 0, "{body}");

    // While held, no Start is admitted for the member's workspaces, but the
    // other owner's fork is not affected.
    let worker = f.worker().await;
    assert!(worker
        .resume(
            f.child.id,
            f.member.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StartKind::Resume,
        )
        .await
        .is_err());
    let (status, body) = f.agent_get("stop-owner-workspaces").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The reconciler drains the running workspace with the execution-hold
    // reason and clears the hold once everything has settled.
    assert!(worker.reconcile_member_deactivation(16).await.unwrap() >= 1);
    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, stop_reason FROM workspace_vms WHERE workspace_id = $1")
            .bind(f.parent.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(status, "stopping");
    assert_eq!(reason.as_deref(), Some("execution_hold"));
    worker.reconcile_stopping(16).await.unwrap();
    worker.reconcile_member_deactivation(16).await.unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(f.parent.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(status, "stopped");
    assert_eq!(
        f.hold(f.member.id).await,
        None,
        "the hold clears on settlement"
    );
    // Access was never removed.
    let deactivated: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deactivated_at FROM org_memberships WHERE org_id = $1 AND user_id = $2",
    )
    .bind(f.org.id)
    .bind(f.member.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert!(deactivated.is_none());
    f.finish().await;
}

#[tokio::test]
async fn people_stop_their_own_or_as_admin_anyones_workspaces_including_the_owner() {
    let f = Fixture::new().await;
    let path = |user: Uuid| format!("/api/v1/orgs/{}/members/{user}/stop-workspaces", f.org.id);

    // A member cannot stop another member's workspaces.
    let (status, _) = person_post(&f.app, &f.other, &path(f.member.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(f.hold(f.member.id).await, None);

    // A member can stop their own.
    let (status, body) = person_post(&f.app, &f.other, &path(f.other.id)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(f.hold(f.other.id).await.is_some());

    // An administrator can stop another member's, and the organization owner
    // is not exempt (the admin fixture owns the organization).
    let (status, body) = person_post(&f.app, &f.admin, &path(f.member.id)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = person_post(&f.app, &f.admin, &path(f.admin.id)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(f.hold(f.admin.id).await.is_some());

    // The member can read their own progress.
    let response = f
        .app
        .authenticated_client(&f.member)
        .get(&format!(
            "/api/v1/orgs/{}/members/{}/shutdown",
            f.org.id, f.member.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    f.finish().await;
}

// ─── Scope ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn operations_reach_this_workspace_and_its_owners_other_workspaces() {
    let f = Fixture::new().await;
    let unrelated = f.workspace(f.member.id, None).await;
    let foreign_child = f.workspace(f.other.id, Some(f.parent.id)).await;
    let grandchild = f.workspace(f.member.id, Some(f.child.id)).await;
    // Base belongs to the organization even when its creator runs the agent.
    let base = TestWorkspace::create_base(&f.app, &f.org).await;
    set_owner(&f.app, base.id, f.member.id).await;

    // Any workspace of the same owner, related or not (owner scope).
    for target in [unrelated.id, grandchild.id] {
        let (status, body) = f
            .agent_post(&format!("workspaces/{target}/stop"), json!({}))
            .await;
        assert_eq!(status, StatusCode::OK, "stop {target}: {body}");
    }
    for (target, relation) in [
        (f.parent.id, "self"),
        (f.child.id, "child"),
        (unrelated.id, "owned"),
        (grandchild.id, "owned"),
        (base.id, "other"),
    ] {
        let (status, body) = f.agent_get(&format!("workspaces/{target}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["relation"], relation, "{target}: {body}");
    }

    for target in [foreign_child.id, base.id] {
        for leaf in ["start", "stop", "delete"] {
            let (status, body) = f
                .agent_post(&format!("workspaces/{target}/{leaf}"), json!({}))
                .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{leaf} {target}: {body}");
        }
        let (status, _) = f
            .agent_post(
                &format!("workspaces/{target}/sessions"),
                json!({"content": "hello", "requestKey": "k"}),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // The agent never chats in, starts or deletes its own workspace.
    for (leaf, body) in [
        (
            "sessions",
            json!({"content": "hello", "requestKey": "self"}),
        ),
        ("start", json!({})),
        ("delete", json!({})),
    ] {
        let (status, body) = f
            .agent_post(&format!("workspaces/{}/{leaf}", f.parent.id), body)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "self {leaf}: {body}");
    }

    // A workspace transferred to another owner leaves the scope.
    set_owner(&f.app, f.child.id, f.other.id).await;
    let (status, _) = f
        .agent_post(&format!("workspaces/{}/stop", f.child.id), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    set_owner(&f.app, f.child.id, f.member.id).await;
    let (status, body) = f
        .agent_post(&format!("workspaces/{}/stop", f.child.id), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The path workspace must be the credential's own.
    let (status, _) = agent_post_with(
        &f.app,
        &f.token,
        &format!(
            "{}/api/internal/workspace-agent/{}/workspaces/{}/stop",
            f.app.base_url, f.child.id, f.child.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A stale incarnation is refused.
    let stale = f
        .app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.parent.id, f.member.id, Uuid::new_v4(), 1, 3600)
        .unwrap();
    let (status, _) = agent_post_with(
        &f.app,
        &stale,
        &f.agent_path(&format!("workspaces/{}/stop", f.child.id)),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Children lists only same-owner direct children.
    let (status, body) = f.agent_get("children").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec![f.child.id.to_string()]);
    f.finish().await;
}

// ─── Audit ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn every_mutating_agent_request_is_audited_whatever_its_outcome() {
    let events = helpers::tracing_capture();
    let f = Fixture::new().await;
    let unrelated = f.workspace(f.other.id, None).await;

    // Refused by the scope check.
    let (status, _) = f
        .agent_post(&format!("workspaces/{}/stop", unrelated.id), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Rejected before the handler ran: an unknown field.
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/sessions", f.child.id),
            json!({"content": "hi", "requestKey": "k", "unknown": 1}),
        )
        .await;
    assert!(status.is_client_error(), "{status}");
    let rejected_status = status;
    // Succeeded.
    let (status, body) = f
        .agent_post(&format!("workspaces/{}/stop", f.child.id), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Reads are not audited, as for people.
    let (status, _) = f.agent_get("children").await;
    assert_eq!(status, StatusCode::OK);

    let audit = agent_audit(events, f.parent.id);
    assert_eq!(audit.len(), 3, "{audit:#?}");
    for event in &audit {
        assert_eq!(event.level, "INFO");
        assert_eq!(
            event.field("owner_user_id"),
            Some(f.member.id.to_string().as_str())
        );
        assert_eq!(
            event.field("caller_vm_id"),
            Some(f.peer.vm_id().to_string().as_str())
        );
    }
    let stop = "POST /api/internal/workspace-agent/:id/workspaces/:targetId/stop";
    let refused = &audit[0];
    assert_eq!(refused.field("action"), Some(stop));
    assert_eq!(
        refused.field("target_workspace_id"),
        Some(unrelated.id.to_string().as_str())
    );
    assert_eq!(refused.field("status"), Some("403"));
    assert_eq!(refused.field("outcome"), Some("client_error"));
    assert_eq!(refused.field("result"), Some(""));
    let rejected = &audit[1];
    assert_eq!(
        rejected.field("action"),
        Some("POST /api/internal/workspace-agent/:id/workspaces/:targetId/sessions")
    );
    assert_eq!(
        rejected.field("target_workspace_id"),
        Some(f.child.id.to_string().as_str())
    );
    assert_eq!(
        rejected.field("status"),
        Some(rejected_status.as_u16().to_string().as_str())
    );
    assert_eq!(rejected.field("outcome"), Some("client_error"));
    let stopped = &audit[2];
    assert_eq!(stopped.field("action"), Some(stop));
    assert_eq!(stopped.field("operation"), Some("stop"));
    assert_eq!(stopped.field("result"), Some("admitting"));
    assert_eq!(stopped.field("status"), Some("200"));
    assert_eq!(stopped.field("outcome"), Some("success"));
    f.finish().await;
}

// ─── Files ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_edits_files_in_its_running_child_and_reads_what_its_owner_can() {
    let events = helpers::tracing_capture();
    let f = Fixture::new().await;
    let child_peer = TestWorkspaceRuntimePeer::attach(&f.app, &f.child, f.member.id).await;
    let repo = f.child.repo_id;
    let child = f.child.id;

    // A new file needs no hash.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{child}/files/write"),
            json!({"repoId": repo, "path": "notes/plan.md", "content": "step 1\n"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        child_peer
            .file_content(repo, "notes/plan.md")
            .await
            .as_deref(),
        Some("step 1\n")
    );

    // The owner's running child is read live, with its hash and VM.
    let (status, body) = f
        .agent_get(&format!(
            "workspaces/{child}/files/content?repoId={repo}&path=notes/plan.md"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "live");
    assert_eq!(body["data"]["content"], "step 1\n");
    assert_eq!(body["data"]["vmId"], child_peer.vm_id().to_string());
    let first_hash = body["data"]["hash"].as_str().unwrap().to_string();

    // Changing it needs the hash it was read with; a stale one is refused.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{child}/files/write"),
            json!({
                "repoId": repo,
                "path": "notes/plan.md",
                "content": "step 2\n",
                "expectedHash": first_hash,
                "expectedVmId": child_peer.vm_id(),
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{child}/files/write"),
            json!({
                "repoId": repo,
                "path": "notes/plan.md",
                "content": "step 3\n",
                "expectedHash": first_hash,
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        child_peer
            .file_content(repo, "notes/plan.md")
            .await
            .as_deref(),
        Some("step 2\n")
    );

    let (status, body) = f
        .agent_get(&format!(
            "workspaces/{child}/files?repoId={repo}&path=notes"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "live");
    assert!(
        body["data"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "notes/plan.md"),
        "{body}"
    );

    let (_, body) = f
        .agent_get(&format!(
            "workspaces/{child}/files/content?repoId={repo}&path=notes/plan.md"
        ))
        .await;
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{child}/files/delete"),
            json!({"repoId": repo, "path": "notes/plan.md", "expectedHash": body["data"]["hash"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(child_peer.file_content(repo, "notes/plan.md").await, None);

    // Its own workspace is in scope too.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/files/write", f.parent.id),
            json!({"repoId": f.parent.repo_id, "path": "self.txt", "content": "mine\n"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Another member's workspace is readable from its committed cut, never
    // writable; neither is another member's child of a child nor Base.
    let visible = f.workspace(f.other.id, None).await;
    helpers::seed_workspace_authority_head(
        &f.app,
        visible.id,
        visible.repo_id,
        &visible.head_commit,
    )
    .await;
    let (status, body) = f
        .agent_get(&format!(
            "workspaces/{}/files/content?repoId={}&path=README.md",
            visible.id, visible.repo_id
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["source"], "committed");
    assert_eq!(body["data"]["content"], "test workspace\n");
    let foreign_grandchild = f.workspace(f.other.id, Some(child)).await;
    let base = TestWorkspace::create_base(&f.app, &f.org).await;
    set_owner(&f.app, base.id, f.member.id).await;
    for target in [&visible, &foreign_grandchild, &base] {
        for (leaf, body) in [
            (
                "write",
                json!({"repoId": target.repo_id, "path": "x.txt", "content": "x"}),
            ),
            (
                "delete",
                json!({"repoId": target.repo_id, "path": "README.md", "expectedHash": "h"}),
            ),
            (
                "rename",
                json!({"repoId": target.repo_id, "from": "README.md", "to": "R.md"}),
            ),
        ] {
            let (status, body) = f
                .agent_post(&format!("workspaces/{}/files/{leaf}", target.id), body)
                .await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{leaf} {}: {body}",
                target.id
            );
        }
    }
    let private = TestWorkspace::create_personal(&f.app, &f.org, f.other.id).await;
    let (status, _) = f
        .agent_get(&format!(
            "workspaces/{}/files?repoId={}",
            private.id, private.repo_id
        ))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Every change, refused ones included, is audited with its file.
    let audit = agent_audit(events, f.parent.id);
    let files: Vec<_> = audit
        .iter()
        .filter(|event| {
            event
                .field("operation")
                .is_some_and(|operation| operation.ends_with("_file"))
        })
        .collect();
    assert_eq!(files.len(), 5 + 9, "{audit:#?}");
    assert!(
        audit
            .iter()
            .all(|event| !event.field("action").unwrap_or("").starts_with("GET ")),
        "reads are not audited: {audit:#?}"
    );
    let written = files[0];
    assert_eq!(written.field("operation"), Some("write_file"));
    assert_eq!(written.field("result"), Some("written"));
    assert_eq!(written.field("outcome"), Some("success"));
    assert_eq!(
        written.field("target_workspace_id"),
        Some(child.to_string().as_str())
    );
    assert_eq!(written.field("repo_id"), Some(repo.to_string().as_str()));
    assert_eq!(written.field("file_path"), Some("notes/plan.md"));
    let stale = files[2];
    assert_eq!(stale.field("status"), Some("409"));
    assert_eq!(stale.field("outcome"), Some("client_error"));
    assert_eq!(stale.field("file_path"), Some("notes/plan.md"));
    assert_eq!(files[3].field("operation"), Some("delete_file"));
    assert_eq!(files[3].field("result"), Some("deleted"));
    let refused_rename = files[7];
    assert_eq!(refused_rename.field("operation"), Some("rename_file"));
    assert_eq!(refused_rename.field("status"), Some("403"));
    assert_eq!(
        refused_rename.field("target_workspace_id"),
        Some(visible.id.to_string().as_str())
    );
    assert_eq!(refused_rename.field("file_path"), Some("README.md"));
    assert_eq!(refused_rename.field("new_file_path"), Some("R.md"));

    drop(child_peer);
    f.finish().await;
}

// ─── Reads ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reads_reach_every_workspace_the_owner_can_read_and_start_nothing() {
    let f = Fixture::new().await;
    let visible = f.workspace(f.other.id, None).await;
    let private = TestWorkspace::create_personal(&f.app, &f.org, f.other.id).await;
    let visible_chat = insert_chat(&f.app, f.org.id, visible.id, f.other.id).await;
    let private_chat = insert_chat(&f.app, f.org.id, private.id, f.other.id).await;
    let own_chat = insert_chat(&f.app, f.org.id, f.parent.id, f.member.id).await;

    let (status, body) = f.agent_get(&format!("workspaces/{}", visible.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["relation"], "other");
    assert_eq!(body["data"]["vm"]["status"], "stopped");
    assert_eq!(body["data"]["sessions"][0]["id"], visible_chat.to_string());

    let (status, body) = f
        .agent_get(&format!("sessions/{visible_chat}/messages"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["messages"][1]["content"],
        "The bug is in parse()"
    );
    let (status, body) = f
        .agent_get(&format!("sessions/{visible_chat}/timeline"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["data"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["content"] == "cargo test: 1 failed"),
        "{body}"
    );
    let (status, _) = f.agent_get(&format!("sessions/{own_chat}/messages")).await;
    assert_eq!(status, StatusCode::OK);

    // The owner cannot read a private workspace, so neither can the agent.
    let (status, _) = f.agent_get(&format!("workspaces/{}", private.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = f
        .agent_get(&format!("sessions/{private_chat}/messages"))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Reads start nothing.
    let started: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE workspace_id = $1 AND start_intent IS NOT NULL",
    )
    .bind(visible.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(started, 0);

    let (status, body) = f.agent_get("workspaces").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap().to_string())
        .collect();
    assert!(listed.contains(&visible.id.to_string()));
    assert!(!listed.contains(&private.id.to_string()));
    f.finish().await;
}

// ─── Fork with a prompt ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_fork_with_a_prompt_replays_without_reviving_a_stopped_launch() {
    let f = Fixture::new().await;
    let request = json!({
        "name": "task-42-child-01",
        "prompt": "Fix the parser",
        "idempotencyKey": "fork-1",
    });
    let (status, body) = f.agent_post("fork", request.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let session: Uuid = body["data"]["startedChat"]["sessionId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let (owner, parent): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT owner_user_id, cloned_from_id FROM workspaces WHERE id = $1")
            .bind(child)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!((owner, parent), (f.member.id, Some(f.parent.id)));
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE chat_session_id = $1 AND status = 'queued'",
    )
    .bind(session)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    // The launch's Start consent is bound to the fork's chat.
    let intent: Option<String> = sqlx::query_scalar(
        "SELECT start_intent ->> 'id' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(child)
    .fetch_optional(&f.app.db)
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        intent,
        Some(g5_api::services::workspace_vm_service::fork_launch_intent_id(session).to_string()),
        "{body}"
    );

    // Stop cancels the launch; a replayed fork returns the same child and
    // chat and does not start it again.
    let (status, body) = f
        .agent_post(&format!("workspaces/{child}/stop"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Another Start replaces the launch intent, then is cancelled too.
    sqlx::query(
        "UPDATE workspace_vms SET start_intent = jsonb_set(start_intent, '{id}', to_jsonb($2::text))
         WHERE workspace_id = $1",
    )
    .bind(child)
    .bind(Uuid::new_v4().to_string())
    .execute(&f.app.db)
    .await
    .unwrap();
    let (status, body) = f.agent_post("fork", request).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["workspace"]["id"], child.to_string());
    assert_eq!(
        body["data"]["startedChat"]["sessionId"],
        session.to_string()
    );
    assert!(body["data"]["start"]["error"].is_string(), "{body}");
    let status: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(child)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(status, "cancelled");
    f.finish().await;
}

/// A personal workspace has no children, so its agent manages its owner's
/// other workspaces and forks Base under owner scope
/// (`g5.security.agent-workspace-scope`).
#[tokio::test]
async fn a_personal_agent_forks_base_and_manages_its_owners_workspaces() {
    let f = Fixture::new().await;
    let base = TestWorkspace::create_base(&f.app, &f.org).await;
    let response = f
        .app
        .authenticated_client(&f.member)
        .get(&format!("/api/v1/orgs/{}/me", f.org.id))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{body}");
    let personal = TestWorkspace {
        id: body["data"]["workspaceId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        org_id: f.org.id,
        repo_id: Uuid::nil(),
        head_commit: String::new(),
        name: "personal".into(),
    };
    let peer = TestWorkspaceRuntimePeer::attach(&f.app, &personal, f.member.id).await;
    let token = f
        .app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, personal.id, f.member.id, peer.vm_id(), 1, 3600)
        .unwrap();
    let path = |leaf: &str| {
        format!(
            "{}/api/internal/workspace-agent/{}/{leaf}",
            f.app.base_url, personal.id
        )
    };
    let fork = |name: &str, source: Option<Uuid>| {
        let mut body = json!({"name": name, "idempotencyKey": name});
        if let Some(source) = source {
            body["sourceWorkspaceId"] = json!(source);
        }
        let (app, token, url) = (&f.app, &token, path("fork"));
        async move { agent_post_with(app, token, &url, body).await }
    };
    let parent_of = |workspace: Uuid| {
        sqlx::query_as::<_, (Option<Uuid>, Option<Uuid>)>(
            "SELECT owner_user_id, cloned_from_id FROM workspaces WHERE id = $1",
        )
        .bind(workspace)
        .fetch_one(&f.app.db)
    };
    let created = |body: &Value| -> Uuid {
        body["data"]["workspace"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };

    // The personal workspace itself is never a fork source.
    let (status, body) = fork("copy-of-personal", None).await;
    assert_ne!(status, StatusCode::CREATED, "{body}");

    // Base and the owner's other workspaces are.
    let (status, body) = fork("from-base", Some(base.id)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let from_base = created(&body);
    assert_eq!(
        parent_of(from_base).await.unwrap(),
        (Some(f.member.id), Some(base.id))
    );
    let (status, body) = fork("from-parent", Some(f.parent.id)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        parent_of(created(&body)).await.unwrap(),
        (Some(f.member.id), Some(f.parent.id))
    );
    // Not another member's workspace.
    let foreign = f.workspace(f.other.id, None).await;
    let (status, body) = fork("from-foreign", Some(foreign.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // It operates on the owner's workspaces it did not fork, and deletes them.
    let (status, body) = agent_post_with(
        &f.app,
        &token,
        &path(&format!("workspaces/{}/stop", f.child.id)),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = agent_post_with(
        &f.app,
        &token,
        &path(&format!("workspaces/{from_base}/delete")),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Base itself is not a target.
    let (status, body) = agent_post_with(
        &f.app,
        &token,
        &path(&format!("workspaces/{}/stop", base.id)),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Another workspace's agent sees the personal workspace as its owner's.
    let (status, body) = f.agent_get(&format!("workspaces/{}", personal.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["relation"], "owned", "{body}");

    // The owner's setting on the personal workspace still gates all of it.
    set_agent_opt_in(&f.app, personal.id, false).await;
    let (status, body) = fork("refused", Some(base.id)).await;
    assert_opt_in_refused(status, &body, "fork from Base");
    drop(peer);
    f.finish().await;
}

// ─── Owner opt-in to change other workspaces ─────────────────────────────────

#[tokio::test]
async fn other_workspaces_need_the_owners_opt_in() {
    let f = Fixture::new().await;
    let settings = format!("/api/v1/workspaces/{}/settings", f.parent.id);
    let owner = f.app.authenticated_client(&f.member);
    let set = |on: bool| {
        let owner = &owner;
        let settings = &settings;
        async move {
            let response = owner
                .put_json(settings, &json!({"agentMayModifyOtherWorkspaces": on}))
                .await;
            let status = response.status();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"]["agentMayModifyOtherWorkspaces"], on, "{body}");
        }
    };
    set(false).await;

    // Forking is refused before anything is written.
    let (status, body) = f
        .agent_post(
            "fork",
            json!({"name": "refused", "prompt": "Fix it", "idempotencyKey": "refused"}),
        )
        .await;
    assert_opt_in_refused(status, &body, "fork");
    let children: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE cloned_from_id = $1")
            .bind(f.parent.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(children, 1, "only the fixture's child");
    let operations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE source_workspace_id = $1")
            .bind(f.parent.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(operations, 0);

    // Every operation on a child is refused, and an unknown target the same
    // way, so the refusal reveals nothing about it.
    let child = f.child.id;
    let unknown = Uuid::new_v4();
    for target in [child, unknown] {
        for (leaf, body) in [
            ("start", json!({})),
            ("stop", json!({})),
            ("delete", json!({})),
            ("pull-request", json!({})),
            (
                "sessions",
                json!({"content": "Run the tests", "requestKey": "refused"}),
            ),
            (
                "approvals/reviews",
                json!({"definitionId": Uuid::new_v4(), "definitionVersion": 1}),
            ),
        ] {
            let (status, response) = f
                .agent_post(&format!("workspaces/{target}/{leaf}"), body)
                .await;
            assert_opt_in_refused(status, &response, &format!("{leaf} {target}"));
        }
    }
    let vm: Option<String> =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
            .bind(child)
            .fetch_optional(&f.app.db)
            .await
            .unwrap();
    assert_eq!(vm, None, "the child was never started");

    // Reads of the child, and reads and operations on the agent's own
    // workspace, need no opt-in.
    let (status, body) = f.agent_get("children").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = f.agent_get(&format!("workspaces/{child}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["relation"], "child", "{body}");
    for target in [child, f.parent.id] {
        let (status, body) = f.agent_get(&format!("workspaces/{target}/approvals")).await;
        assert_eq!(status, StatusCode::OK, "approvals {target}: {body}");
    }

    // The agent's VM credential cannot turn the setting on: the settings
    // route accepts person sessions only.
    let response = f
        .app
        .client
        .put(format!("{}{settings}", f.app.base_url))
        .bearer_auth(&f.token)
        .json(&json!({"agentMayModifyOtherWorkspaces": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!agent_opt_in(&f.app, f.parent.id).await);
    // Nor can another member, even one who can read the workspace.
    let response = f
        .app
        .authenticated_client(&f.other)
        .put_json(&settings, &json!({"agentMayModifyOtherWorkspaces": true}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!agent_opt_in(&f.app, f.parent.id).await);

    // Once the owner turns it on, the same requests are admitted.
    set(true).await;
    let (status, body) = f
        .agent_post(&format!("workspaces/{child}/stop"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = f
        .agent_post(&format!("workspaces/{unknown}/stop"), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_ne!(body["error"]["code"], "AGENT_OTHER_WORKSPACES_DISABLED");
    f.finish().await;
}

#[tokio::test]
async fn the_opt_in_follows_same_owner_forks_and_transfer_clears_it() {
    let f = Fixture::new().await;
    // A workspace's agent may change other workspaces by default.
    assert!(agent_opt_in(&f.app, f.child.id).await);

    // The agent's fork has the parent's owner, so it copies the setting.
    let (status, body) = f
        .agent_post(
            "fork",
            json!({"name": "inherits", "idempotencyKey": "inherits", "start": false}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let agent_fork: Uuid = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(agent_opt_in(&f.app, agent_fork).await);

    // With the parent turned off, the owner's own fork copies off; another
    // member's fork starts with the default, on.
    set_agent_opt_in(&f.app, f.parent.id, false).await;
    for (user, expected) in [(&f.member, false), (&f.other, true)] {
        let response = f
            .app
            .authenticated_client(user)
            .post_json(
                &format!("/api/v1/workspaces/{}/clone", f.parent.id),
                &json!({"name": format!("fork-{expected}"), "idempotencyKey": format!("fork-{expected}")}),
            )
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let fork: Uuid = body["data"]["id"]
            .as_str()
            .or_else(|| body["data"]["workspace"]["id"].as_str())
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(agent_opt_in(&f.app, fork).await, expected, "{body}");
    }

    // Transfer turns it off in the transfer's commit.
    let response = f
        .app
        .authenticated_client(&f.member)
        .post_json(
            &format!("/api/v1/workspaces/{agent_fork}/transfer"),
            &json!({"toUserId": f.other.id, "idempotencyKey": "give-away"}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    assert_eq!(body["data"]["status"], "completed", "{body}");
    assert!(!agent_opt_in(&f.app, agent_fork).await);
    f.finish().await;
}

// ─── Child chats ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn child_chats_and_follow_ups_queue_once_and_never_start_the_child() {
    let f = Fixture::new().await;
    let new_chat = json!({"content": "Run the tests", "requestKey": "chat-1"});
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions", f.child.id),
            new_chat.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session = body["data"]["sessionId"].as_str().unwrap().to_string();
    let input = body["data"]["inputId"].clone();
    let (status, body) = f
        .agent_post(&format!("workspaces/{}/sessions", f.child.id), new_chat)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["inputId"], input);
    assert_eq!(body["data"]["replayed"], true);

    let follow_up = json!({"content": "Also lint", "requestKey": "follow-1"});
    let path = format!("workspaces/{}/sessions/{session}/messages", f.child.id);
    let (status, body) = f.agent_post(&path, follow_up.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let follow_input = body["data"]["inputId"].clone();
    let (status, body) = f.agent_post(&path, follow_up).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["inputId"], follow_input);
    let (status, _) = f
        .agent_post(
            &path,
            json!({"content": "Different", "requestKey": "follow-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let inputs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM session_inputs WHERE session_id = $1")
            .bind(session.parse::<Uuid>().unwrap())
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(inputs, 2);
    let started: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE workspace_id = $1 AND start_intent IS NOT NULL",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(started, 0, "queued input never starts the child");

    // A session of another workspace cannot be addressed through the child.
    let own_chat = insert_chat(&f.app, f.org.id, f.parent.id, f.member.id).await;
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{own_chat}/messages", f.child.id),
            json!({"content": "x", "requestKey": "follow-2"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    f.finish().await;
}

#[tokio::test]
async fn launches_no_agent_turn_can_run_are_refused_before_anything_is_queued() {
    let f = Fixture::new().await;
    let refused = |status: StatusCode, body: &Value| {
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let text = body.to_string();
        assert!(
            text.contains("unsupported_agent_turn_permission_mode")
                && text.contains("bypassPermissions"),
            "{body}"
        );
    };
    // A mode outside the provider's catalog fails launch validation first.
    let unsupported = |status: StatusCode, body: &Value| {
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(
            body.to_string()
                .contains("unsupported_agent_permission_mode"),
            "{body}"
        );
    };

    // A fork whose first chat could not run creates nothing.
    let (status, body) = f
        .agent_post(
            "fork",
            json!({
                "name": "mode-child",
                "prompt": "Write a limerick",
                "idempotencyKey": "fork-mode",
                "permissionMode": "acceptEdits",
            }),
        )
        .await;
    unsupported(status, &body);
    let forked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE name = 'mode-child' AND deleted_at IS NULL",
    )
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(forked, 0);

    // A new chat in a child: refused, then queued with a runnable mode.
    let sessions = format!("workspaces/{}/sessions", f.child.id);
    let (status, body) = f
        .agent_post(
            &sessions,
            json!({"content": "Plan it", "requestKey": "mode-1", "permissionMode": "plan"}),
        )
        .await;
    unsupported(status, &body);
    let chats: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(f.child.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(chats, 0);
    let (status, body) = f
        .agent_post(
            &sessions,
            json!({
                "content": "Write it",
                "requestKey": "mode-2",
                "permissionMode": "bypassPermissions",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let task: Uuid = body["data"]["taskId"].as_str().unwrap().parse().unwrap();

    // A follow-up in a chat whose fixed mode cannot run names the remedy.
    let stuck = insert_chat(&f.app, f.org.id, f.child.id, f.member.id).await;
    sqlx::query(
        r#"UPDATE chat_sessions
           SET provider_state = '{"claudeCode":{"settings":{"permissionMode":"acceptEdits"}}}'
           WHERE id = $1"#,
    )
    .bind(stuck)
    .execute(&f.app.db)
    .await
    .unwrap();
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{stuck}/messages", f.child.id),
            json!({"content": "Again", "requestKey": "mode-3"}),
        )
        .await;
    refused(status, &body);
    assert!(body.to_string().contains("start a new chat"), "{body}");
    let inputs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM session_inputs WHERE session_id = $1")
            .bind(stuck)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(inputs, 0);

    // A probe says why a task failed.
    sqlx::query(
        r#"UPDATE tasks SET status = 'failed',
               outcome = '{"status":"failed","reason":"dispatch_admission_failed","detail":"Conflict: refused"}'
           WHERE id = $1"#,
    )
    .bind(task)
    .execute(&f.app.db)
    .await
    .unwrap();
    let (status, body) = f.agent_get(&format!("workspaces/{}", f.child.id)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let probed = body["data"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task.to_string())
        .cloned()
        .unwrap();
    assert_eq!(
        probed["failure"],
        json!({"reason": "dispatch_admission_failed", "detail": "Conflict: refused"})
    );
    f.finish().await;
}

// ─── Delete ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_deletes_a_child_but_not_a_child_with_children() {
    let f = Fixture::new().await;
    let with_children = f.workspace(f.member.id, Some(f.parent.id)).await;
    let _grandchild = f.workspace(f.member.id, Some(with_children.id)).await;

    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/delete", with_children.id),
            json!({}),
        )
        .await;
    assert!(status.is_client_error(), "{status}: {body}");
    let (status, body) = f
        .agent_post(&format!("workspaces/{}/delete", f.child.id), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM workspaces WHERE id = $1")
            .bind(f.child.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert!(deleted);
    f.finish().await;
}

// ─── Merge a child ───────────────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_publishes_and_merges_a_childs_pull_request_into_its_workspace() {
    let events = helpers::tracing_capture();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let child = TestWorkspace::create_named(&app, &org, "feature").await;
    let parent_id = child.attach_parent(&app).await;
    let parent_name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(parent_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let parent = TestWorkspace {
        id: parent_id,
        org_id: org.id,
        repo_id: child.repo_id,
        head_commit: child.head_commit.clone(),
        name: parent_name.clone(),
    };
    let peer = TestWorkspaceRuntimePeer::attach(&app, &parent, owner.id).await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, parent_id, owner.id, peer.vm_id(), 1, 3600)
        .unwrap();
    let path = app.repo_path(child.repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    repo.checkout_branch_force(&child.name).unwrap();
    std::fs::write(path.join("fix.txt"), "fixed\n").unwrap();
    repo.add_all().unwrap();
    let commit = repo
        .commit(
            "Fix the parser",
            &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
        )
        .unwrap();

    let base = format!("{}/api/internal/workspace-agent/{parent_id}", app.base_url);
    // Publishing changes the child, so the owner's setting must allow it.
    let publish_url = format!("{base}/workspaces/{}/pull-request", child.id);
    let publish = || {
        agent_post_with(
            &app,
            &token,
            &publish_url,
            json!({"title": "Fix the parser"}),
        )
    };
    set_agent_opt_in(&app, parent_id, false).await;
    let (status, body) = publish().await;
    assert_opt_in_refused(status, &body, "publish with the setting off");
    set_agent_opt_in(&app, parent_id, true).await;
    let (status, body) = publish().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["localMergeStatus"], "ready",
        "{body}"
    );
    let pull_request = body["data"]["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = agent_post_with(
        &app,
        &token,
        &format!("{base}/pull-requests/{pull_request}/refresh"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["localMergeStatus"], "ready", "{body}");
    let response = app
        .client
        .get(format!("{base}/workspaces/{parent_id}/pull-requests"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let listed: Value = response.json().await.unwrap();
    assert_eq!(listed["data"][0]["id"], pull_request, "{listed}");

    // Merging into the agent's own workspace never needs the opt-in.
    set_agent_opt_in(&app, parent_id, false).await;
    let (status, body) = agent_post_with(
        &app,
        &token,
        &format!("{base}/pull-requests/{pull_request}/merge"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let merged: String =
        sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id = $1")
            .bind(pull_request.parse::<Uuid>().unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(merged, "merged");
    let head = git2::Repository::open(&path)
        .unwrap()
        .refname_to_id(&format!("refs/heads/{parent_name}"))
        .unwrap()
        .to_string();
    assert_eq!(
        head, commit,
        "the parent branch advanced to the child's commit"
    );
    // Closing reaches the merged pull request through the agent's merge
    // authority and is refused only because it is merged.
    let (status, body) = agent_post_with(
        &app,
        &token,
        &format!("{base}/pull-requests/{pull_request}/close"),
        json!({}),
    )
    .await;
    assert!(
        status.is_client_error() && status != StatusCode::FORBIDDEN,
        "{status}: {body}"
    );

    // Refresh is audited like the other pull-request operations, and the
    // refused close is audited too.
    let audit = agent_audit(events, parent_id);
    let refresh = audit
        .iter()
        .find(|event| event.field("operation") == Some("refresh_pull_request"))
        .unwrap_or_else(|| panic!("no refresh event in {audit:#?}"));
    assert_eq!(refresh.field("result"), Some("refreshed"));
    assert_eq!(refresh.field("outcome"), Some("success"));
    assert!(refresh
        .field("path")
        .is_some_and(|path| path.contains(&pull_request)));
    let close = audit.last().unwrap();
    assert_eq!(
        close.field("action"),
        Some("POST /api/internal/workspace-agent/:id/pull-requests/:pullRequestId/close")
    );
    assert_eq!(close.field("outcome"), Some("client_error"));
    assert_eq!(
        close.field("status"),
        Some(status.as_u16().to_string().as_str())
    );

    drop(peer);
    app.cleanup().await;
}

// ─── Increment 2: child chat and task controls ───────────────────────────────

#[tokio::test]
async fn an_agent_controls_its_childs_chats_but_never_its_own() {
    let f = Fixture::new().await;
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions", f.child.id),
            json!({"content": "First task", "requestKey": "ctl-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session = body["data"]["sessionId"].as_str().unwrap().to_string();
    let first_input = body["data"]["inputId"].as_str().unwrap().to_string();
    let first_task = body["data"]["taskId"].as_str().unwrap().to_string();
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{session}/messages", f.child.id),
            json!({"content": "Then this", "requestKey": "ctl-2"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let second_input = body["data"]["inputId"].as_str().unwrap().to_string();

    // Read the queue, then edit and withdraw the follow-up under its revision.
    let (status, body) = f.agent_get(&format!("sessions/{session}/inputs")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let follow_up = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["id"] == second_input)
        .unwrap_or_else(|| panic!("follow-up listed: {body}"))
        .clone();
    let revision = follow_up["revision"].as_i64().unwrap();
    let base = format!(
        "workspaces/{}/sessions/{session}/inputs/{second_input}",
        f.child.id
    );
    let (status, body) = f
        .agent_post(
            &format!("{base}/edit"),
            json!({"expectedRevision": revision, "content": "Then this instead"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = f
        .agent_post(
            &format!("{base}/edit"),
            json!({"expectedRevision": revision, "content": "stale"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale revision is refused");
    let (status, body) = f
        .agent_post(
            &format!("{base}/withdraw"),
            json!({"expectedRevision": revision + 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Rename the chat and stop its still-queued first turn.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{session}/rename", f.child.id),
            json!({"title": "Parser fix"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["title"], "Parser fix");
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{session}/stop", f.child.id),
            json!({"turnId": first_input}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/tasks/{first_task}/cancel", f.child.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(first_task.parse::<Uuid>().unwrap())
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert!(
        matches!(task_status.as_str(), "cancelled" | "succeeded" | "failed"),
        "{task_status}"
    );

    // The agent's own chats stay out of reach of these controls.
    let own_chat = insert_chat(&f.app, f.org.id, f.parent.id, f.member.id).await;
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{own_chat}/rename", f.parent.id),
            json!({"title": "mine"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let own_task: Uuid = sqlx::query_scalar("SELECT id FROM tasks WHERE chat_session_id = $1")
        .bind(own_chat)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/tasks/{own_task}/continue", f.parent.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // A child's session cannot be addressed through another workspace path.
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/sessions/{session}/rename", f.parent.id),
            json!({"title": "x"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    f.finish().await;
}

#[tokio::test]
async fn approvals_read_follows_the_owner_read_scope() {
    let f = Fixture::new().await;
    // Own and owned workspaces read, as before.
    for target in [f.parent.id, f.child.id] {
        let (status, body) = f.agent_get(&format!("workspaces/{target}/approvals")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    // Another member's ordinary workspace now reads too, as the owner can in
    // the UI: #1099 widened approval reads to `g5.security.agent-read-scope`.
    let readable = f.workspace(f.other.id, None).await;
    let (status, body) = f
        .agent_get(&format!("workspaces/{}/approvals", readable.id))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // A workspace the owner cannot read (another's personal one) is still refused.
    let hidden = TestWorkspace::create_personal(&f.app, &f.org, f.other.id).await;
    let (status, _) = f
        .agent_get(&format!("workspaces/{}/approvals", hidden.id))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Requesting and cancelling stay agent-scoped: refused on another owner's
    // workspace even when it is readable.
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/approvals/reviews", readable.id),
            json!({"definitionId": Uuid::new_v4(), "definitionVersion": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = f
        .agent_post(
            &format!(
                "workspaces/{}/approvals/requests/{}/cancel",
                readable.id,
                Uuid::new_v4()
            ),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // No agent route records a decision.
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/approvals/decisions", f.parent.id),
            json!({}),
        )
        .await;
    assert!(
        matches!(status, StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND),
        "{status}"
    );
    f.finish().await;
}

#[tokio::test]
async fn issue_writes_without_a_reference_need_a_primary_issue() {
    let f = Fixture::new().await;
    for (leaf, body) in [
        ("issue-state", json!({"state": "closed"})),
        ("issue-edit", json!({"title": "Renamed"})),
    ] {
        let (status, body) = f.agent_post(leaf, body).await;
        assert!(status.is_client_error(), "{leaf}: {status}: {body}");
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{leaf}: {body}");
    }
    f.finish().await;
}

// ─── Managed preview ─────────────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_runs_the_managed_preview_of_its_workspace_and_children() {
    let f = Fixture::new().await;
    f.peer
        .seed_file(
            f.parent.repo_id,
            ".g5/workspace.yaml",
            "preview:\n  command: node server.js\n  port: 4173\n",
        )
        .await;
    let preview = format!("workspaces/{}/preview", f.parent.id);

    let (status, body) = f
        .agent_get(&format!(
            "workspaces/{}/repos/{}/preview-applications",
            f.parent.id, f.parent.repo_id
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["port"], 4173, "{body}");

    let start = |key: &str| json!({"repoId": f.parent.repo_id, "idempotencyKey": key});
    let (status, body) = f.agent_post(&preview, start("p-1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task = body["data"]["taskId"].as_str().unwrap().to_string();
    let (status, body) = f.agent_post(&preview, start("p-1")).await;
    assert_eq!(status, StatusCode::OK, "replay: {body}");
    assert_eq!(body["data"]["taskId"], task.as_str());
    let (status, _) = f.agent_post(&preview, start("p-2")).await;
    assert_eq!(status, StatusCode::CONFLICT, "one preview at a time");
    let (status, body) = f.agent_get(&preview).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["taskId"], task.as_str());

    let (status, body) = f.agent_post(&format!("{preview}/cancel"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1::uuid")
        .bind(&task)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "cancelled");

    // A stopped child's preview needs its VM: refused, but not as out of scope.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/preview", f.child.id),
            json!({"repoId": f.child.repo_id, "idempotencyKey": "c-1"}),
        )
        .await;
    assert!(
        status.is_client_error() && status != StatusCode::FORBIDDEN,
        "{status}: {body}"
    );
    // Another owner's workspace is out of scope.
    let foreign = f.workspace(f.other.id, None).await;
    let (status, _) = f
        .agent_get(&format!("workspaces/{}/preview", foreign.id))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = f
        .agent_post(
            &format!(
                "workspaces/{}/repos/{}/preview-profiles",
                foreign.id, foreign.repo_id
            ),
            json!({"name": "web", "command": "x", "port": 3000}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    f.finish().await;
}

// ─── Preview credential ──────────────────────────────────────────────────────

/// What the child's previewed application answers every request with.
const CHILD_PREVIEW_RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\nconnection: close\r\ncontent-length: 5\r\n\r\nchild";

#[tokio::test]
async fn an_agent_fetches_its_childs_preview_with_a_task_bound_credential() {
    let events = helpers::tracing_capture();
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let f = Fixture::with_app(TestApp::spawn_with_config(config).await).await;
    let child_peer = TestWorkspaceRuntimePeer::attach_with_preview_response(
        &f.app,
        &f.child,
        f.member.id,
        CHILD_PREVIEW_RESPONSE,
    )
    .await;
    let task = crate::api_task_lifecycle::running_preview(
        &f.app,
        &f.child,
        f.member.id,
        child_peer.vm_id(),
    )
    .await;
    let mint = format!("workspaces/{}/preview/credential", f.child.id);

    let (status, body) = f.agent_post(&mint, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let minted = &body["data"];
    assert_eq!(minted["taskId"], task.to_string());
    let host = format!("{task}.preview.g5.example");
    assert_eq!(minted["previewUrl"], format!("https://{host}/"));
    assert_eq!(minted["header"], g5_auth::PREVIEW_AGENT_HEADER);
    let credential = minted["credential"].as_str().unwrap().to_string();
    let audit = agent_audit(events, f.parent.id);
    let minted_event = audit.last().expect("the mint is audited");
    assert_eq!(
        minted_event.field("operation"),
        Some("mint_preview_credential")
    );
    assert_eq!(
        minted_event.field("target_workspace_id"),
        Some(f.child.id.to_string().as_str())
    );
    assert!(!format!("{audit:?}").contains(&credential));

    let fetch = |host: String, extra: &'static [(&'static str, &'static str)]| {
        let mut request = f
            .app
            .client
            .get(format!("{}/api/items?limit=5", f.app.base_url))
            .header("host", host)
            .header(g5_auth::PREVIEW_AGENT_HEADER, &credential);
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        request.send()
    };

    // The child's application answers through the relay, without the credential.
    let response = fetch(host.clone(), &[]).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "child");
    let relayed = child_peer.preview_requests().await;
    assert_eq!(relayed.len(), 1);
    assert!(relayed[0].starts_with("GET /api/items?limit=5 HTTP/1.1\r\n"));
    assert!(!relayed[0]
        .to_ascii_lowercase()
        .contains(g5_auth::PREVIEW_AGENT_HEADER));
    assert!(!relayed[0].contains(&credential));

    // A browser, another task's origin and a WebSocket are refused before relay.
    let response = fetch(host.clone(), &[("sec-fetch-mode", "navigate")])
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "browser");
    let response = fetch(format!("{}.preview.g5.example", Uuid::new_v4()), &[])
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "another task");
    let response = fetch(
        host.clone(),
        &[
            ("connection", "upgrade"),
            ("upgrade", "websocket"),
            ("sec-websocket-version", "13"),
            ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
        ],
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "websocket");
    assert_eq!(child_peer.preview_requests().await.len(), 1);

    // Turning the setting off refuses the next request and new credentials.
    set_agent_opt_in(&f.app, f.parent.id, false).await;
    let response = fetch(host.clone(), &[]).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "setting off");
    let (status, body) = f.agent_post(&mint, json!({})).await;
    assert_opt_in_refused(status, &body, "mint with the setting off");
    set_agent_opt_in(&f.app, f.parent.id, true).await;
    assert_eq!(
        fetch(host.clone(), &[]).await.unwrap().status(),
        StatusCode::OK
    );

    // Once the minting incarnation ends, its credential reaches nothing.
    let bump = |delta: i64| {
        sqlx::query("UPDATE workspace_vms SET epoch = epoch + $2 WHERE workspace_id = $1")
            .bind(f.parent.id)
            .bind(delta)
            .execute(&f.app.db)
    };
    bump(1).await.unwrap();
    let response = fetch(host.clone(), &[]).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "ended incarnation"
    );
    bump(-1).await.unwrap();

    // Self without a preview, and another owner's workspace, mint nothing.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/preview/credential", f.parent.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.to_string().contains("PREVIEW_NOT_READY"), "{body}");
    let foreign = f.workspace(f.other.id, Some(f.parent.id)).await;
    let (status, _) = f
        .agent_post(
            &format!("workspaces/{}/preview/credential", foreign.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Closing the preview retires the credential.
    let (status, body) = f
        .agent_post(
            &format!("workspaces/{}/preview/cancel", f.child.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let response = fetch(host, &[]).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "closed preview"
    );
    assert_eq!(child_peer.preview_requests().await.len(), 2);

    drop(child_peer);
    f.finish().await;
}

// ─── Increment 6: fork with an issue, member lookup ─────────────────────────

/// Issues on for the fixture's organization with G5 issues keyed `ACME`, and
/// the open issues ACME-1 and ACME-2 written by the agent's owner.
async fn native_issues(f: &Fixture) {
    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secret_keys) \
         VALUES ($1, '{\"external_issues_enabled\": true}'::jsonb, '{}') \
         ON CONFLICT (org_id) DO UPDATE \
         SET settings_json = org_configs.settings_json || EXCLUDED.settings_json",
    )
    .bind(f.org.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    let response = f
        .app
        .authenticated_client(&f.admin)
        .post_json(
            &format!("/api/v1/orgs/{}/issue-domains", f.org.id),
            &json!({"provider": "g5", "key": "ACME"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let domain: Value = response.json().await.unwrap();
    for title in ["First", "Second"] {
        let response = f
            .app
            .authenticated_client(&f.member)
            .post_json(
                &format!("/api/v1/orgs/{}/issues", f.org.id),
                &json!({"title": title, "idempotencyKey": title, "domainId": domain["data"]["id"]}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}

/// The child's live links, as `(display key, primary)`.
async fn live_links(f: &Fixture, workspace: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT display_key, is_primary FROM workspace_issue_links
         WHERE workspace_id = $1::uuid AND unlinked_at IS NULL ORDER BY display_key",
    )
    .bind(workspace)
    .fetch_all(&f.app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_fork_links_its_issue_inside_the_fork_as_the_ui_fork_does() {
    let f = Fixture::new().await;
    native_issues(&f).await;

    // `g5d workspace fork --issue 1`: a bare number names the creation
    // domain's issue, and the link is primary unless asked otherwise.
    let request =
        json!({"name": "issue-child", "idempotencyKey": "fork-issue-1", "issue": {"ref": "1"}});
    let (status, body) = f.agent_post("fork", request.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child = body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let link = &body["data"]["issueLink"];
    assert_eq!(link["displayKey"], "ACME-1", "{body}");
    assert_eq!(
        (link["requestedRole"].as_str(), link["role"].as_str()),
        (Some("primary"), Some("primary"))
    );
    assert_eq!(
        live_links(&f, &child).await,
        vec![("ACME-1".to_string(), true)]
    );

    // A retry returns the same child and link; nothing is linked twice.
    let (status, replay) = f.agent_post("fork", request).await;
    assert_eq!(status, StatusCode::CREATED, "{replay}");
    assert_eq!(replay["data"]["workspace"]["id"], child.as_str());
    assert_eq!(replay["data"]["issueLink"]["linkId"], link["linkId"]);
    assert_eq!(live_links(&f, &child).await.len(), 1);

    // ACME-1 has its primary workspace: a second primary fork is refused
    // before any child exists.
    let (status, refused) = f
        .agent_post(
            "fork",
            json!({"name": "second-primary", "idempotencyKey": "fork-issue-2", "issue": {"ref": "ACME-1", "role": "primary"}}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    let created: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE org_id = $1 AND name = 'second-primary'",
    )
    .bind(f.org.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(created, 0);

    // As related it forks, and an issue that does not exist refuses the fork.
    let (status, related) = f
        .agent_post(
            "fork",
            json!({"name": "related-child", "idempotencyKey": "fork-issue-3", "issue": {"ref": "ACME-1", "role": "related"}}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{related}");
    assert_eq!(related["data"]["issueLink"]["role"], "related");
    let related_child = related["data"]["workspace"]["id"].as_str().unwrap();
    assert_eq!(
        live_links(&f, related_child).await,
        vec![("ACME-1".to_string(), false)]
    );
    let (status, missing) = f
        .agent_post(
            "fork",
            json!({"name": "missing-issue", "idempotencyKey": "fork-issue-4", "issue": {"ref": "ACME-99"}}),
        )
        .await;
    assert!(status.is_client_error(), "{status} {missing}");
    let (status, bad_role) = f
        .agent_post(
            "fork",
            json!({"name": "bad-role", "idempotencyKey": "fork-issue-5", "issue": {"ref": "2", "role": "owner"}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad_role}");
    f.finish().await;
}

#[tokio::test]
async fn an_agent_looks_up_its_organizations_members_with_their_connected_accounts() {
    let f = Fixture::new().await;
    // The admin has connected GitHub; `other` is deactivated.
    sqlx::query(
        "INSERT INTO github_app_user_grants (user_id, github_user_id, github_login, access_token_enc)
         VALUES ($1, 4242, 'Ada-Lovelace', '\\x00'::bytea)",
    )
    .bind(f.admin.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = now() WHERE org_id = $1 AND user_id = $2",
    )
    .bind(f.org.id)
    .bind(f.other.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    // Someone in another organization is never found.
    let mut stranger = TestUser::create(&f.app).await;
    let _elsewhere = TestOrg::create_with_admin(&f.app, &mut stranger).await;

    let ids = |body: &Value| -> Vec<String> {
        let mut ids: Vec<String> = body["data"]["users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    };
    let sorted = |users: &[&TestUser]| -> Vec<String> {
        let mut ids: Vec<String> = users.iter().map(|user| user.id.to_string()).collect();
        ids.sort();
        ids
    };

    // Without words: the active members, with accounts and the owner marked.
    let (status, body) = f.agent_get("users").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ids(&body), sorted(&[&f.admin, &f.member]));
    let users = body["data"]["users"].as_array().unwrap();
    let admin = users
        .iter()
        .find(|user| user["id"] == f.admin.id.to_string())
        .unwrap();
    assert_eq!(
        admin["accounts"],
        json!([{"provider": "github", "login": "Ada-Lovelace"}])
    );
    assert_eq!(
        (admin["email"].as_str(), admin["state"].as_str()),
        (Some(f.admin.email.as_str()), Some("active"))
    );
    assert_eq!(admin["me"], false);
    let owner = users
        .iter()
        .find(|user| user["id"] == f.member.id.to_string())
        .unwrap();
    assert_eq!(
        (owner["me"].as_bool(), owner["accounts"].clone()),
        (Some(true), json!([]))
    );
    assert_eq!(body["data"]["hasMore"], false);

    let (_, body) = f.agent_get("users?includeDeactivated=true").await;
    assert_eq!(ids(&body), sorted(&[&f.admin, &f.member, &f.other]));
    let other = body["data"]["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["id"] == f.other.id.to_string())
        .unwrap()
        .clone();
    assert_eq!(other["state"], "deactivated");

    // Words match a login, an email or a name, ignoring case; all must match.
    let (_, body) = f.agent_get("users?q=ada-LOVE").await;
    assert_eq!(ids(&body), sorted(&[&f.admin]));
    let local_part = f.member.email.split('@').next().unwrap().to_uppercase();
    let (_, body) = f.agent_get(&format!("users?q={local_part}")).await;
    assert_eq!(ids(&body), sorted(&[&f.member]));
    let (_, body) = f.agent_get("users?q=ada+nobody-matches-this").await;
    assert_eq!(ids(&body), Vec::<String>::new());
    let (_, body) = f.agent_get(&format!("users?q={}", stranger.email)).await;
    assert_eq!(ids(&body), Vec::<String>::new());

    // One member exactly, deactivated included.
    for (user, expected) in [
        ("%40me".to_string(), &f.member),
        ("github%3Aada-lovelace".to_string(), &f.admin),
        ("Ada-Lovelace".to_string(), &f.admin),
        (f.other.email.to_uppercase(), &f.other),
        (f.member.id.to_string(), &f.member),
    ] {
        let (status, body) = f.agent_get(&format!("users?user={user}")).await;
        assert_eq!(status, StatusCode::OK, "{user}: {body}");
        assert_eq!(ids(&body), sorted(&[expected]), "{user}");
    }
    for user in [
        stranger.email.clone(),
        stranger.id.to_string(),
        "gitlab%3Aada-lovelace".into(),
    ] {
        let (status, body) = f.agent_get(&format!("users?user={user}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{user}: {body}");
    }

    // Limits, and asking two ways at once.
    let (_, body) = f.agent_get("users?limit=1").await;
    assert_eq!(
        (
            body["data"]["users"].as_array().unwrap().len(),
            body["data"]["hasMore"].as_bool()
        ),
        (1, Some(true))
    );
    for query in ["limit=0", "limit=101", "q=ada&user=%40me"] {
        let (status, body) = f.agent_get(&format!("users?{query}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
    }

    // Only the credential's own workspace path, and only a current one.
    let response = f
        .app
        .client
        .get(format!(
            "{}/api/internal/workspace-agent/{}/users",
            f.app.base_url, f.child.id
        ))
        .bearer_auth(&f.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let stale = f
        .app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.parent.id, f.member.id, Uuid::new_v4(), 1, 3600)
        .unwrap();
    let response = f
        .app
        .client
        .get(f.agent_path("users"))
        .bearer_auth(&stale)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    f.finish().await;
}
