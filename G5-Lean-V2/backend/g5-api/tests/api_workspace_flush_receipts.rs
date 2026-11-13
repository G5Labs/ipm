//! Flush receipt state machine (design v9 §5.5) at the internal
//! `workspace-incarnations/:id/flushes/*` routes.
//!
//! `WorkspaceFlushService` owns the durable receipt: one flush per
//! `(workspace_id, vm_id, request_key)`, a member vector fixed at start, and
//! a completion or a block that is recorded exactly once. These tests state
//! each rule from the caller's side, through the routes the guest
//! dispatcher uses, so the HTTP codes the dispatcher branches on are the
//! ones asserted:
//!
//! - start is idempotent by request key and refuses a replay whose reason
//!   changed, a malformed key, or a workspace with no members;
//! - completion requires exactly the member vector recorded at start, an
//!   unknown flush id is not found, a replay with the same receipt is
//!   accepted, and a different receipt for the same flush conflicts;
//! - a recorded block refuses later completion and later start replays,
//!   keeps its first `blocked_since` for the same fingerprint, notifies the
//!   owner once per fingerprint and clears on the next completed flush;
//! - a block or a completion is final: the other transition conflicts;
//! - the service proves a receipt itself: a direct service call whose flush
//!   leaf does not hold the reported commit is refused and records nothing.
//! - a token whose incarnation was replaced can neither complete its open
//!   flush nor start another (`STALE_INCARNATION`, the lifecycle's reader).

use crate::helpers;

use base64::Engine;
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::process::Command;
use uuid::Uuid;

use g5_api::services::workspace_flush_service::{WorkspaceFlushError, WorkspaceFlushService};
use g5_internal_api::workspace_flush::FlushRepoReceipt;
use helpers::{
    TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
};

struct Incarnation {
    token: String,
    workspace_id: Uuid,
}

impl Incarnation {
    fn mint(org: &TestOrg, workspace: &TestWorkspace, owner: &TestUser, vm_id: Uuid) -> Self {
        let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
            .expect("construct incarnation minter")
            .mint_workspace_incarnation(org.id, workspace.id, owner.id, vm_id, 1, 3600)
            .expect("mint incarnation token");
        Self {
            token,
            workspace_id: workspace.id,
        }
    }

    async fn post(&self, app: &TestApp, leaf: &str, body: &Value) -> (StatusCode, Value) {
        let response = app
            .client
            .post(format!(
                "{}/api/internal/workspace-incarnations/{}/flushes/{leaf}",
                app.base_url, self.workspace_id
            ))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .expect("flush route request");
        let status = response.status();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn start(&self, app: &TestApp, reason: &str, key: &str) -> (StatusCode, Value) {
        self.post(
            app,
            "start",
            &json!({"reason": reason, "idempotency_key": key}),
        )
        .await
    }

    async fn complete(&self, app: &TestApp, flush_id: &str, repos: Value) -> (StatusCode, Value) {
        self.post(
            app,
            &format!("{flush_id}/complete"),
            &json!({"repos": repos}),
        )
        .await
    }

    async fn fail(&self, app: &TestApp, flush_id: &str, failure: &Value) -> (StatusCode, Value) {
        self.post(app, &format!("{flush_id}/fail"), failure).await
    }
}

fn error_code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or("")
}

fn flush_id(body: &Value) -> String {
    body["flush_id"]
        .as_str()
        .expect("flush start carries the flush id")
        .to_string()
}

fn failure(fingerprint: &str) -> Value {
    json!({
        "code": "FLUSH_TOO_LARGE",
        "fingerprint": fingerprint,
        "limits": {"per_file_bytes": 1, "tree_bytes": 2, "pack_bytes": 3},
        "offenders": [{
            "repo_id": Uuid::nil(),
            "path": "big.bin",
            "bytes": 4,
            "category": "file"
        }],
        "detail": "one file exceeds the per-file limit"
    })
}

/// A clone of the workspace branch that can push the two flush leaves the
/// completion proof requires, so `complete` reaches the service instead of
/// stopping at the route's ref verification.
struct FlushProof {
    branch_oid: String,
    snapshot_oid: String,
    dir: std::path::PathBuf,
    auth_header: String,
    _root: tempfile::TempDir,
}

impl FlushProof {
    async fn prepare(app: &TestApp, workspace: &TestWorkspace, token: &str) -> Self {
        let root = tempfile::tempdir().expect("create checkout root");
        let dir = root.path().join("repo");
        let auth =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        let auth_header = format!("http.extraHeader=Authorization: Basic {auth}");
        let remote = format!("{}/git/{}", app.base_url, workspace.repo_id);
        let cloned = Command::new("git")
            .args(["-c", &auth_header, "clone", "--branch", &workspace.name])
            .arg(&remote)
            .arg(&dir)
            .output()
            .await
            .expect("run incarnation clone");
        assert!(
            cloned.status.success(),
            "clone failed: {}",
            String::from_utf8_lossy(&cloned.stderr)
        );
        let mut proof = Self {
            branch_oid: String::new(),
            snapshot_oid: String::new(),
            dir,
            auth_header,
            _root: root,
        };
        proof.branch_oid = proof.rev_parse("HEAD").await;
        std::fs::write(proof.dir.join("dirty.txt"), "unflushed edit\n").expect("write file");
        for args in [
            vec!["add", "dirty.txt"],
            vec!["commit", "-q", "-m", "flush snapshot"],
        ] {
            let done = proof.git(&args).output().await.expect("snapshot commit");
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        }
        proof.snapshot_oid = proof.rev_parse("HEAD").await;
        proof
    }

    fn git(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .args(["-c", &self.auth_header])
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_AUTHOR_NAME", "Flush Receipts")
            .env("GIT_AUTHOR_EMAIL", "flush@g5.test")
            .env("GIT_COMMITTER_NAME", "Flush Receipts")
            .env("GIT_COMMITTER_EMAIL", "flush@g5.test");
        command
    }

    async fn rev_parse(&self, rev: &str) -> String {
        let out = self
            .git(&["rev-parse", rev])
            .output()
            .await
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    async fn push_leaves(&self, workspace_id: Uuid, flush_id: &str) {
        let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
        let snapshot = format!("{}:{prefix}/snapshot", self.snapshot_oid);
        let branch = format!("{}:{prefix}/branch", self.branch_oid);
        let pushed = self
            .git(&["push", "--porcelain", "origin", &snapshot, &branch])
            .output()
            .await
            .expect("push flush leaves");
        assert!(
            pushed.status.success(),
            "flush leaves must push: {}{}",
            String::from_utf8_lossy(&pushed.stdout),
            String::from_utf8_lossy(&pushed.stderr)
        );
    }

    fn receipt(&self, repo_id: Uuid, sequencer: &str) -> Value {
        json!({
            "repo_id": repo_id,
            "branch_oid": self.branch_oid,
            "local_branch_oid": self.branch_oid,
            "local_head_oid": self.branch_oid,
            "head_ref": null,
            "sequencer": sequencer,
            "snapshot_oid": self.snapshot_oid,
            "dirty": true,
            "dirty_paths": ["dirty.txt"]
        })
    }
}

async fn flush_block(app: &TestApp, workspace_id: Uuid) -> Option<Value> {
    sqlx::query_scalar::<_, Option<Value>>(
        "SELECT last_error -> 'flush_block' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("load flush block")
}

async fn flush_error(app: &TestApp, flush_id: &str) -> Option<Value> {
    sqlx::query_scalar::<_, Option<Value>>(
        "SELECT error FROM workspace_flushes WHERE flush_id = $1::uuid",
    )
    .bind(flush_id)
    .fetch_one(&app.db)
    .await
    .expect("load flush error")
}

async fn block_notifications(app: &TestApp, workspace_id: Uuid, owner_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE workspace_id = $1 AND recipient_user_id = $2
           AND category = 'workspace.flush_blocked'",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .fetch_one(&app.db)
    .await
    .expect("count flush block notifications")
}

#[tokio::test]
async fn flush_start_is_idempotent_by_request_key() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());

    let (status, first) = incarnation.start(&app, "shutdown", "stop-1").await;
    assert_eq!(status, StatusCode::OK, "first start: {first}");
    assert_eq!(first["vm_id"], runtime.vm_id().to_string());
    assert_eq!(first["epoch"], 1);
    assert_eq!(first["reason"], "shutdown");
    assert_eq!(
        first["required_repo_ids"],
        json!([workspace.repo_id]),
        "the member vector is fixed at start"
    );

    let (status, replay) = incarnation.start(&app, "shutdown", "stop-1").await;
    assert_eq!(status, StatusCode::OK, "replay: {replay}");
    assert_eq!(replay, first, "a replayed start returns the same receipt");

    let (status, changed) = incarnation.start(&app, "fork", "stop-1").await;
    assert_eq!(status, StatusCode::CONFLICT, "changed reason: {changed}");
    assert_eq!(error_code(&changed), "FLUSH_COMPLETION_CONFLICT");

    // One flush is open at a time per workspace
    // (workspace_flushes_one_active_origin_idx): a second key while the
    // first is neither complete nor blocked is refused and records nothing.
    let (status, second) = incarnation.start(&app, "fork", "fork-1").await;
    assert_eq!(status, StatusCode::CONFLICT, "second open flush: {second}");
    assert_eq!(error_code(&second), "FLUSH_IN_PROGRESS");
    let count_flushes = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_flushes WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("count flush rows")
    };
    assert_eq!(count_flushes().await, 1);

    // Once the first is closed, a new key opens a new flush.
    let (status, blocked) = incarnation
        .fail(&app, &flush_id(&first), &failure(&"a".repeat(64)))
        .await;
    assert_eq!(status, StatusCode::OK, "close the first flush: {blocked}");
    let (status, other) = incarnation.start(&app, "fork", "fork-1").await;
    assert_eq!(status, StatusCode::OK, "other key: {other}");
    assert_ne!(flush_id(&other), flush_id(&first));
    assert_eq!(other["reason"], "fork");
    assert_eq!(count_flushes().await, 2);

    for (case, key) in [
        ("empty", String::new()),
        ("padded", " stop-2".to_string()),
        ("control character", "stop\u{7}2".to_string()),
        ("too long", "k".repeat(257)),
    ] {
        let (status, body) = incarnation.start(&app, "shutdown", &key).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case} key: {body}");
        assert_eq!(error_code(&body), "INVALID_FLUSH_REQUEST", "{case} key");
    }

    // A workspace whose member vector is empty cannot start a flush at all:
    // there is nothing to preserve and nothing a completion could prove.
    sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("empty the member vector");
    let (status, body) = incarnation.start(&app, "shutdown", "stop-3").await;
    assert_eq!(status, StatusCode::CONFLICT, "empty vector: {body}");
    assert_eq!(error_code(&body), "FLUSH_INCOMPLETE_VECTOR");

    app.cleanup().await;
}

#[tokio::test]
async fn flush_completion_requires_the_recorded_member_vector() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;

    let (status, started) = incarnation.start(&app, "shutdown", "vector").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;

    let member = proof.receipt(workspace.repo_id, "none");
    let stranger = proof.receipt(Uuid::new_v4(), "none");
    for (case, repos) in [
        ("no members", json!([])),
        ("an unknown member", json!([stranger])),
        ("an extra member", json!([member, stranger])),
        ("a duplicated member", json!([member, member])),
    ] {
        let (status, body) = incarnation.complete(&app, &id, repos).await;
        assert_eq!(status, StatusCode::CONFLICT, "{case}: {body}");
        assert_eq!(error_code(&body), "FLUSH_INCOMPLETE_VECTOR", "{case}");
    }
    assert_eq!(
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT complete_at FROM workspace_flushes WHERE flush_id = $1::uuid",
        )
        .bind(&id)
        .fetch_one(&app.db)
        .await
        .expect("load flush completion"),
        None,
        "a refused vector records no completion"
    );

    let (status, body) = incarnation
        .complete(&app, &Uuid::new_v4().to_string(), json!([member]))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown flush: {body}");
    assert_eq!(error_code(&body), "FLUSH_REF_UNKNOWN");

    let (status, body) = incarnation.complete(&app, &id, json!([member])).await;
    assert_eq!(status, StatusCode::OK, "exact vector: {body}");
    assert_eq!(body["flush_id"], id);
    assert_eq!(body["repos"], json!([member]));

    app.cleanup().await;
}

#[tokio::test]
async fn flush_completion_replays_the_same_receipt_and_conflicts_on_another() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;

    let (status, started) = incarnation.start(&app, "shutdown", "complete").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;
    let receipt = proof.receipt(workspace.repo_id, "none");

    let (status, first) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::OK, "first completion: {first}");
    let (status, replay) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::OK, "replayed completion: {replay}");
    assert_eq!(
        replay, first,
        "a replayed completion returns the recorded receipt and time"
    );

    // Same refs, same proof, but the receipt itself differs: the recorded
    // one wins and the caller learns it is out of step.
    let other = proof.receipt(workspace.repo_id, "rebase");
    let (status, body) = incarnation.complete(&app, &id, json!([other])).await;
    assert_eq!(status, StatusCode::CONFLICT, "other receipt: {body}");
    assert_eq!(error_code(&body), "FLUSH_COMPLETION_CONFLICT");

    let (status, body) = incarnation.fail(&app, &id, &failure(&"a".repeat(64))).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "block after completion: {body}"
    );
    assert_eq!(error_code(&body), "FLUSH_COMPLETION_CONFLICT");
    assert_eq!(flush_block(&app, workspace.id).await, None);

    let (status, again) = incarnation.start(&app, "shutdown", "complete").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "start replay after completion: {again}"
    );
    assert_eq!(flush_id(&again), id);

    let (last_flush, complete_at): (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT vm.last_flush_id, flush.complete_at
             FROM workspace_vms vm
             JOIN workspace_flushes flush ON flush.flush_id = vm.last_flush_id
             WHERE vm.workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .expect("load the incarnation's last flush");
    assert_eq!(last_flush.map(|id| id.to_string()), Some(id));
    assert!(complete_at.is_some());

    app.cleanup().await;
}

/// `WorkspaceFlushService::complete` proves the receipt without a route in
/// front (`g5.git.receipt`): a receipt naming a commit its snapshot leaf does
/// not hold is refused and records nothing; the proved receipt then completes.
#[tokio::test]
async fn service_completion_refuses_a_receipt_its_flush_refs_do_not_prove() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;
    let (status, started) = incarnation.start(&app, "shutdown", "service-proof").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;
    let flush = Uuid::parse_str(&id).expect("flush id is a UUID");

    let claims = g5_auth::WorkspaceIncarnationClaims {
        org_id: org.id,
        workspace_id: workspace.id,
        owner_user_id: owner.id,
        vm_id: runtime.vm_id(),
        epoch: 1,
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 3600,
        iss: Some("g5-api-test".to_string()),
        aud: g5_auth::WORKSPACE_INCARNATION_AUDIENCE.to_string(),
        jti: Uuid::new_v4().to_string(),
        token_use: g5_auth::WORKSPACE_INCARNATION_TOKEN_USE.to_string(),
    };
    let service = WorkspaceFlushService::new(
        app.state.system_pool.clone(),
        app.state.repo_storage.clone(),
        app.state.git_bin.clone(),
    );
    let receipt = |value: Value| -> FlushRepoReceipt {
        serde_json::from_value(value).expect("decode the test receipt")
    };
    // A clean receipt at the branch commit; the snapshot leaf holds the
    // snapshot commit instead.
    let mut forged = receipt(proof.receipt(workspace.repo_id, "none"));
    forged.snapshot_oid = proof.branch_oid.clone();
    forged.dirty = false;
    forged.dirty_paths = Vec::new();
    let refused = service.complete(&claims, flush, vec![forged]).await;
    assert!(
        matches!(
            refused,
            Err(WorkspaceFlushError::Proof("FLUSH_REF_MISMATCH"))
        ),
        "a receipt the flush refs do not prove: {refused:?}"
    );
    let complete_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT complete_at FROM workspace_flushes WHERE flush_id = $1")
            .bind(flush)
            .fetch_one(&app.db)
            .await
            .expect("load the flush");
    assert_eq!(complete_at, None, "a refused receipt records nothing");

    service
        .complete(
            &claims,
            flush,
            vec![receipt(proof.receipt(workspace.repo_id, "none"))],
        )
        .await
        .expect("the proved receipt completes");
    app.cleanup().await;
}

#[tokio::test]
async fn flush_refuses_a_token_for_a_replaced_incarnation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;
    let (status, started) = incarnation.start(&app, "shutdown", "replaced").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;

    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("replace the incarnation");
    let receipt = proof.receipt(workspace.repo_id, "none");
    let (status, body) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::CONFLICT, "complete: {body}");
    assert_eq!(error_code(&body), "STALE_INCARNATION", "complete: {body}");
    let (status, body) = incarnation
        .start(&app, "shutdown", "after-replacement")
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "start: {body}");
    assert_eq!(error_code(&body), "STALE_INCARNATION", "start: {body}");
    let complete_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT complete_at FROM workspace_flushes WHERE flush_id = $1::uuid")
            .bind(&id)
            .fetch_one(&app.db)
            .await
            .expect("load the flush");
    assert_eq!(
        complete_at, None,
        "the replaced incarnation completes nothing"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn flush_completion_accepts_a_branch_advance_after_capture_without_replacing_it() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;
    let (status, started) = incarnation.start(&app, "shutdown", "racing-import").await;
    assert_eq!(status, StatusCode::OK);
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;
    let receipt = proof.receipt(workspace.repo_id, "none");

    // A provider import advances G5 after the guest's branch observation and
    // leaf upload but before it completes the recovery receipt.
    let mirror = app.repo_path(workspace.repo_id).await;
    let repo = git2::Repository::open(&mirror).unwrap();
    let parent = repo
        .find_commit(git2::Oid::from_str(&proof.branch_oid).unwrap())
        .unwrap();
    let signature = git2::Signature::now("Provider", "provider@test.local").unwrap();
    let incoming = repo
        .commit(
            Some(&format!("refs/heads/{}", workspace.name)),
            &signature,
            &signature,
            "provider fast-forward",
            &parent.tree().unwrap(),
            &[&parent],
        )
        .unwrap();
    let (status, completed) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["repos"], json!([receipt]));
    assert_eq!(
        repo.refname_to_id(&format!("refs/heads/{}", workspace.name))
            .unwrap(),
        incoming
    );
    let (status, replay) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, completed);
    app.cleanup().await;
}

/// A too-large flush is final for that flush, and the failure stays on the
/// flush row. The stop that asked for it gives up saving and kills the VM
/// (`g5.lifecycle.stop.force`), so the workspace keeps no block that would
/// later keep idle stop away, and no repair notice is sent.
#[tokio::test]
async fn too_large_flush_records_its_failure_on_the_flush_only() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;

    let (status, started) = incarnation.start(&app, "shutdown", "blocked").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);

    for (case, body) in [
        ("another code", {
            let mut body = failure(&"b".repeat(64));
            body["code"] = json!("FLUSH_LOST");
            body
        }),
        ("a short fingerprint", failure("abc")),
        ("a non-hex fingerprint", failure(&"z".repeat(64))),
    ] {
        let (status, response) = incarnation.fail(&app, &id, &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {response}");
        assert_eq!(error_code(&response), "INVALID_FLUSH_REQUEST", "{case}");
    }
    assert_eq!(flush_error(&app, &id).await, None);

    let fingerprint = "b".repeat(64);
    let (status, first) = incarnation.fail(&app, &id, &failure(&fingerprint)).await;
    assert_eq!(status, StatusCode::OK, "first failure: {first}");
    assert_eq!(first["flush_id"], id);
    let blocked_since = first["blocked_since"].clone();
    let recorded = flush_error(&app, &id)
        .await
        .expect("the failure is recorded on the flush");
    assert_eq!(recorded["flush_block"]["fingerprint"], fingerprint);
    assert_eq!(recorded["flush_block"]["offenders"][0]["path"], "big.bin");
    assert_eq!(
        flush_block(&app, workspace.id).await,
        None,
        "the workspace keeps no block"
    );
    let latest: Value = sqlx::query_scalar(
        "SELECT last_error -> 'latest' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the latest workspace error");
    assert_eq!(latest["code"], "FLUSH_TOO_LARGE");
    assert_eq!(
        block_notifications(&app, workspace.id, owner.id).await,
        0,
        "no repair notice is sent"
    );

    let (status, replay) = incarnation.fail(&app, &id, &failure(&fingerprint)).await;
    assert_eq!(status, StatusCode::OK, "replayed failure: {replay}");
    assert_eq!(
        replay["blocked_since"], blocked_since,
        "a replay keeps the first failure time"
    );

    // A failed flush is final: its leaves are no longer accepted at the git
    // boundary, it cannot complete, and a start replay of its key reports the
    // conflict instead of handing the flush back.
    let prefix = format!("refs/g5/flush/{}/{id}", workspace.id);
    let leaves = [
        format!("{}:{prefix}/snapshot", proof.snapshot_oid),
        format!("{}:{prefix}/branch", proof.branch_oid),
    ];
    let pushed = proof
        .git(&["push", "--porcelain", "origin", &leaves[0], &leaves[1]])
        .output()
        .await
        .expect("push leaves of a failed flush");
    assert!(
        !pushed.status.success(),
        "leaves of a failed flush must be refused: {}",
        String::from_utf8_lossy(&pushed.stdout)
    );
    let receipt = proof.receipt(workspace.repo_id, "none");
    let (status, body) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "complete after failure: {body}"
    );
    assert_eq!(error_code(&body), "FLUSH_COMPLETION_CONFLICT");
    let (status, body) = incarnation.start(&app, "shutdown", "blocked").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "start replay after failure: {body}"
    );
    assert_eq!(error_code(&body), "FLUSH_COMPLETION_CONFLICT");

    // A later flush with a new key is unaffected and completes.
    let (status, next) = incarnation.start(&app, "shutdown", "repaired").await;
    assert_eq!(status, StatusCode::OK, "next start: {next}");
    let next_id = flush_id(&next);
    proof.push_leaves(workspace.id, &next_id).await;
    let (status, body) = incarnation
        .complete(
            &app,
            &next_id,
            json!([proof.receipt(workspace.repo_id, "none")]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "next completion: {body}");
    assert_eq!(flush_error(&app, &next_id).await, None);
    assert_eq!(block_notifications(&app, workspace.id, owner.id).await, 0);

    app.cleanup().await;
}

/// Source control's History (and every other committed-snapshot reader)
/// follows the branch tip while the VM is up. A flush also runs when a
/// review copy forks a running workspace and `last_flush_id` is never
/// cleared, so before this rule a commit pushed after that fork stayed
/// invisible until the next stop. A durably stopped VM's receipt supplies the
/// working-tree cut only while its recorded branch vector still matches Git.
#[tokio::test]
async fn committed_reads_follow_the_branch_tip_while_the_vm_runs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;

    // The fork flush of a running workspace: a complete receipt whose
    // snapshot carries a dirty edit on top of the branch.
    let (status, started) = incarnation.start(&app, "fork", "fork-history").await;
    assert_eq!(status, StatusCode::OK, "start: {started}");
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;
    let receipt = proof.receipt(workspace.repo_id, "none");
    let (status, completed) = incarnation.complete(&app, &id, json!([receipt])).await;
    assert_eq!(status, StatusCode::OK, "complete: {completed}");

    // The owner commits on the branch and pushes, VM still up.
    let branch = workspace.name.clone();
    for args in [
        vec![
            "checkout",
            "-q",
            "-B",
            branch.as_str(),
            proof.branch_oid.as_str(),
        ],
        vec![
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "after the fork flush",
        ],
    ] {
        let done = proof.git(&args).output().await.expect("run");
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    }
    let pushed_head = proof.rev_parse("HEAD").await;
    let refspec = format!("HEAD:refs/heads/{branch}");
    let pushed = proof
        .git(&["push", "--porcelain", "origin", &refspec])
        .output()
        .await
        .expect("push the branch");
    assert!(
        pushed.status.success(),
        "branch push: {}{}",
        String::from_utf8_lossy(&pushed.stdout),
        String::from_utf8_lossy(&pushed.stderr)
    );

    let history_url = format!(
        "/api/v1/workspaces/{}/repos/{}/commits/history?limit=5",
        workspace.id, workspace.repo_id
    );
    let client = app.authenticated_client(&owner);
    for vm_status in ["running", "starting", "stopping"] {
        sqlx::query("UPDATE workspace_vms SET status = $2 WHERE workspace_id = $1")
            .bind(workspace.id)
            .bind(vm_status)
            .execute(&app.db)
            .await
            .expect("set VM status");
        let response = client.get(&history_url).await;
        assert_eq!(response.status(), StatusCode::OK, "{vm_status}");
        let page: Value = response.json().await.expect("history json");
        assert_eq!(
            page["data"]["commits"][0]["sha"], pushed_head,
            "{vm_status}: History follows the branch tip, not the fork flush: {page}"
        );
    }

    // Stopping cannot hide a newer publication behind an older receipt.
    // A stopped row carries no host or VM identity (incarnation shape).
    for vm_status in ["stopped", "failed"] {
        sqlx::query(
            "UPDATE workspace_vms SET status = $2, host_id = NULL, vm_id = NULL
             WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .bind(vm_status)
        .execute(&app.db)
        .await
        .expect("set VM status");
        let response = client.get(&history_url).await;
        assert_eq!(response.status(), StatusCode::OK, "{vm_status}");
        let page: Value = response.json().await.expect("history json");
        assert_eq!(
            page["data"]["commits"][0]["sha"], pushed_head,
            "{vm_status}: a newer publication supersedes the browsing cut: {page}"
        );
    }

    // History always follows pushed state, including after a branch rewind.
    // The retained dirty snapshot remains recovery evidence, not public history.
    helpers::seed_branch_ref(&app, workspace.repo_id, &branch, &proof.branch_oid).await;
    let response = client.get(&history_url).await;
    assert_eq!(response.status(), StatusCode::OK);
    let page: Value = response.json().await.expect("history json");
    assert_eq!(
        page["data"]["commits"][0]["sha"], proof.branch_oid,
        "{page}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn flush_git_observations_refuse_invalid_proofs_without_advancing_receipts() {
    let storage = tempfile::tempdir().unwrap();
    let mut config = helpers::test_config();
    config.storage = g5_storage::StorageConfig::Local {
        base_dir: storage.path().to_owned(),
        worktree_dir: storage.path().join("worktrees"),
    };
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let incarnation = Incarnation::mint(&org, &workspace, &owner, runtime.vm_id());
    let proof = FlushProof::prepare(&app, &workspace, &incarnation.token).await;
    let (status, started) = incarnation
        .start(&app, "shutdown", "git-proof-observations")
        .await;
    assert_eq!(status, StatusCode::OK);
    let id = flush_id(&started);
    proof.push_leaves(workspace.id, &id).await;
    let valid = proof.receipt(workspace.repo_id, "none");
    let mut missing = valid.clone();
    missing["branch_oid"] = json!("a".repeat(40));
    let mut wrong_width = valid.clone();
    wrong_width["branch_oid"] = json!("a".repeat(64));
    let mut tree = valid.clone();
    tree["branch_oid"] = json!(proof.rev_parse("HEAD^{tree}").await);
    let mut mismatched = valid.clone();
    mismatched["snapshot_oid"] = json!(proof.branch_oid);
    mismatched["dirty"] = json!(false);
    let mut reversed = valid.clone();
    reversed["snapshot_oid"] = json!(proof.branch_oid);
    reversed["local_head_oid"] = json!(proof.snapshot_oid);
    // Use a branch leaf with the valid descendant while the snapshot is its ancestor.
    let repo_path = app.repo_path(workspace.repo_id).await;
    let repo = git2::Repository::open(&repo_path).unwrap();
    let snapshot_ref = format!("refs/g5/flush/{}/{id}/snapshot", workspace.id);
    for (receipt, code) in [
        (missing, "FLUSH_NOT_COMMIT"),
        (wrong_width, "FLUSH_NOT_COMMIT"),
        (tree, "FLUSH_NOT_COMMIT"),
        (mismatched, "FLUSH_REF_MISMATCH"),
    ] {
        let (status, body) = incarnation.complete(&app, &id, json!([receipt])).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(error_code(&body), code);
    }
    repo.reference(
        &snapshot_ref,
        git2::Oid::from_str(&proof.branch_oid).unwrap(),
        true,
        "fixture reversed ancestry",
    )
    .unwrap();
    let (status, body) = incarnation.complete(&app, &id, json!([reversed])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(error_code(&body), "FLUSH_HEAD_UNREACHABLE");
    repo.reference(
        &snapshot_ref,
        git2::Oid::from_str(&proof.snapshot_oid).unwrap(),
        true,
        "restore valid snapshot",
    )
    .unwrap();
    // A same-named shadow branch is not an immutable receipt leaf.
    let branch_ref = format!("refs/g5/flush/{}/{id}/branch", workspace.id);
    for (reference, oid) in [
        (&snapshot_ref, &proof.snapshot_oid),
        (&branch_ref, &proof.branch_oid),
    ] {
        repo.find_reference(reference).unwrap().delete().unwrap();
        repo.reference(
            &format!("refs/heads/{reference}"),
            git2::Oid::from_str(oid).unwrap(),
            true,
            "fixture shadow",
        )
        .unwrap();
    }
    let (status, body) = incarnation.complete(&app, &id, json!([valid])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(error_code(&body), "FLUSH_REF_MISMATCH");
    for (reference, oid) in [
        (&snapshot_ref, &proof.snapshot_oid),
        (&branch_ref, &proof.branch_oid),
    ] {
        repo.reference(
            reference,
            git2::Oid::from_str(oid).unwrap(),
            true,
            "restore exact leaf",
        )
        .unwrap();
    }
    let ref_path = repo.path().join(&snapshot_ref);
    let saved_ref = std::fs::read(&ref_path).unwrap();
    std::fs::write(&ref_path, "broken ref\n").unwrap();
    let (status, body) = incarnation.complete(&app, &id, json!([valid])).await;
    std::fs::write(&ref_path, saved_ref).unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_code(&body), "FLUSH_UNAVAILABLE");
    let blob = repo.blob(b"corrupt object fixture").unwrap().to_string();
    let object_path = repo
        .path()
        .join("objects")
        .join(&blob[..2])
        .join(&blob[2..]);
    std::fs::remove_file(&object_path).unwrap();
    std::fs::write(&object_path, b"broken compressed object").unwrap();
    let mut corrupt = valid.clone();
    corrupt["branch_oid"] = json!(blob);
    let (status, body) = incarnation.complete(&app, &id, json!([corrupt])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_code(&body), "FLUSH_UNAVAILABLE");
    // Put the matching proof in an ancestor, leaving the selected store invalid.
    let git_dir = repo.path().to_owned();
    let parent_git = repo_path.parent().unwrap().join(".git");
    std::fs::rename(&git_dir, &parent_git).unwrap();
    if repo.is_bare() {
        std::fs::create_dir(&repo_path).unwrap();
    }
    let (status, body) = incarnation.complete(&app, &id, json!([valid])).await;
    if repo.is_bare() {
        std::fs::remove_dir(&repo_path).unwrap();
    }
    std::fs::rename(&parent_git, &git_dir).unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_code(&body), "FLUSH_UNAVAILABLE");
    // Loss of repository metadata is an execution error, not proof of a missing branch.
    let head = repo.path().join("HEAD");
    let retained_head = std::fs::read(&head).unwrap();
    std::fs::remove_file(&head).unwrap();
    let (status, body) = incarnation.complete(&app, &id, json!([valid])).await;
    std::fs::write(&head, retained_head).unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_code(&body), "FLUSH_UNAVAILABLE");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("read ref tip"));
    let (completed, selected): (Option<chrono::DateTime<chrono::Utc>>, Option<Uuid>) = sqlx::query_as("SELECT f.complete_at, vm.last_flush_id FROM workspace_flushes f JOIN workspace_vms vm USING (workspace_id) WHERE f.flush_id=$1::uuid").bind(&id).fetch_one(&app.db).await.unwrap();
    assert!(completed.is_none());
    assert_ne!(selected.map(|value| value.to_string()), Some(id.clone()));
    let (status, body) = incarnation.complete(&app, &id, json!([valid])).await;
    assert_eq!(status, StatusCode::OK, "restored valid proof: {body}");
    let selected: Option<Uuid> =
        sqlx::query_scalar("SELECT last_flush_id FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(selected.map(|value| value.to_string()), Some(id));
    app.cleanup().await;
}
