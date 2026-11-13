//! Post-transfer credential rebinding (design v9 §6.5).
//!
//! "The platform token is the incarnation's. Transfer releases the old VM
//! before the new owner's first dispatch, so an old owner's key or token is
//! never used after the owner commit." Concretely, once
//! `POST /workspaces/:id/transfer` completes:
//!
//! - the incarnation token minted for the old owner's VM is refused at the
//!   flush routes and at the git boundary, for the workspace branch and for
//!   the flush namespace alike;
//! - the old owner's own user token no longer writes the workspace branch;
//! - the new owner's first start hands the host a fresh incarnation token
//!   bound to the new owner and a new VM identity, and that token is the one
//!   the flush routes accept.

use crate::helpers;

use base64::Engine;
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::process::Command;
use uuid::Uuid;

use g5_auth::Scope;
use helpers::{
    TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
    TEST_PUBLIC_KEY_PEM,
};

fn mint_incarnation(org_id: Uuid, workspace_id: Uuid, owner_id: Uuid, vm_id: Uuid) -> String {
    g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(org_id, workspace_id, owner_id, vm_id, 1, 3600)
        .expect("mint incarnation token")
}

async fn flush_start(
    app: &TestApp,
    workspace_id: Uuid,
    token: &str,
    key: &str,
) -> (StatusCode, Value) {
    let response = app
        .client
        .post(format!(
            "{}/api/internal/workspace-incarnations/{workspace_id}/flushes/start",
            app.base_url
        ))
        .bearer_auth(token)
        .json(&json!({"reason": "shutdown", "idempotency_key": key}))
        .send()
        .await
        .expect("flush start request");
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// A clone that pushes with one fixed `Authorization` header.
struct Checkout {
    dir: std::path::PathBuf,
    auth_header: String,
    _root: tempfile::TempDir,
}

impl Checkout {
    async fn clone(app: &TestApp, workspace: &TestWorkspace, authorization: &str) -> Self {
        let root = tempfile::tempdir().expect("create checkout root");
        let dir = root.path().join("repo");
        let auth_header = format!("http.extraHeader=Authorization: {authorization}");
        let remote = format!("{}/git/{}", app.base_url, workspace.repo_id);
        let cloned = Command::new("git")
            .args(["-c", &auth_header, "clone", "--branch", &workspace.name])
            .arg(&remote)
            .arg(&dir)
            .output()
            .await
            .expect("run authenticated clone");
        assert!(
            cloned.status.success(),
            "clone failed: {}",
            String::from_utf8_lossy(&cloned.stderr)
        );
        Self {
            dir,
            auth_header,
            _root: root,
        }
    }

    fn git(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .args(["-c", &self.auth_header])
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_AUTHOR_NAME", "Transfer")
            .env("GIT_AUTHOR_EMAIL", "transfer@g5.test")
            .env("GIT_COMMITTER_NAME", "Transfer")
            .env("GIT_COMMITTER_EMAIL", "transfer@g5.test");
        command
    }

    async fn commit(&self, file: &str) -> String {
        std::fs::write(self.dir.join(file), format!("{file}\n")).expect("write file");
        for args in [vec!["add", file], vec!["commit", "-q", "-m", file]] {
            let done = self.git(&args).output().await.expect("commit");
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        }
        let out = self
            .git(&["rev-parse", "HEAD"])
            .output()
            .await
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    async fn push(&self, refspecs: &[&str]) -> (bool, String) {
        let mut args = vec!["push", "--porcelain", "origin"];
        args.extend(refspecs);
        let out = self.git(&args).output().await.expect("push");
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    async fn fetch(&self) -> bool {
        self.git(&["fetch", "origin"])
            .output()
            .await
            .expect("fetch")
            .status
            .success()
    }
}

fn basic(token: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"))
    )
}

fn expect_refused(case: &str, (ok, output): (bool, String)) {
    assert!(!ok, "{case}: push must be refused, got success:\n{output}");
    assert!(
        output.contains("403"),
        "{case}: expected HTTP 403 in git's output:\n{output}"
    );
}

fn data(body: &Value) -> &Value {
    body.get("data").expect("response should contain data")
}

#[tokio::test]
async fn transfer_rebinds_the_incarnation_credentials_to_the_new_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::SessionsRead,
            Scope::SessionsWrite,
            Scope::TasksExecute,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let old_vm_id = runtime.vm_id();
    let old_token = mint_incarnation(org.id, workspace.id, owner.id, old_vm_id);

    // Before the transfer the old incarnation is current: it pushes, and
    // the owner's own token pushes. (No flush is left open here: the
    // transfer's own stop flush needs the workspace's single active flush
    // slot, workspace_flushes_one_active_origin_idx.)
    let incarnation = Checkout::clone(&app, &workspace, &basic(&old_token)).await;
    let before = incarnation.commit("before-transfer.txt").await;
    let (ok, output) = incarnation
        .push(&[&format!("{before}:refs/heads/{}", workspace.name)])
        .await;
    assert!(ok, "incarnation push before transfer:\n{output}");
    // Start from the accepted incarnation commit: the Git boundary permits
    // fast-forward pushes, so credential checks must not depend on force-push.
    let owner_checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    let owner_before = owner_checkout.commit("owner-before-transfer.txt").await;
    let (ok, output) = owner_checkout
        .push(&[&format!("{owner_before}:refs/heads/{}", workspace.name)])
        .await;
    assert!(ok, "owner push before transfer:\n{output}");

    let transferred = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = transferred.status();
    let transferred: Value = transferred.json().await.expect("parse transfer outcome");
    assert_eq!(status, StatusCode::OK, "transfer failed: {transferred}");
    assert_eq!(data(&transferred)["status"], "completed");
    assert!(runtime
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));

    // The old incarnation token is dead at every boundary it used to pass.
    let (status, body) = flush_start(&app, workspace.id, &old_token, "after").await;
    assert_eq!(status, StatusCode::CONFLICT, "old token new flush: {body}");
    assert_eq!(body["error"]["code"], "STALE_INCARNATION");
    let after = incarnation.commit("after-transfer.txt").await;
    expect_refused(
        "old incarnation pushes the workspace branch",
        incarnation
            .push(&[&format!("{after}:refs/heads/{}", workspace.name)])
            .await,
    );
    let prefix = format!("refs/g5/flush/{}/{}", workspace.id, Uuid::new_v4());
    expect_refused(
        "old incarnation pushes its flush leaves",
        incarnation
            .push(&[
                &format!("{after}:{prefix}/snapshot"),
                &format!("{before}:{prefix}/branch"),
            ])
            .await,
    );
    assert!(
        !incarnation.fetch().await,
        "the old incarnation must not read the repository either"
    );

    // The old owner keeps org membership but no longer owns the branch.
    let owner_after = owner_checkout.commit("owner-after-transfer.txt").await;
    expect_refused(
        "former owner pushes the workspace branch",
        owner_checkout
            .push(&[&format!("{owner_after}:refs/heads/{}", workspace.name)])
            .await,
    );

    // The new owner's first start mints a token for the new owner and a new
    // VM identity; that token is the one the lifecycle routes accept.
    let resumed = app
        .authenticated_client(&next_owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = resumed.status();
    let resumed: Value = resumed.json().await.expect("parse resume");
    assert_eq!(status, StatusCode::OK, "new owner resume: {resumed}");
    assert_eq!(data(&resumed)["outcome"], "starting");
    let starts = runtime.received_host_command_params("vm.start").await;
    assert_eq!(starts.len(), 1, "one start after the transfer: {starts:?}");
    let command = starts[0].get("command").unwrap_or(&starts[0]);
    for repo in command["spec"]["repos"]
        .as_array()
        .expect("start repository vector")
    {
        assert!(
            repo.get("upstream_branch").is_none_or(Value::is_null),
            "new owner must not receive an inaccessible parent ref: {repo}"
        );
    }
    let new_token = starts[0]["command"]["workspace_token"]
        .as_str()
        .or_else(|| starts[0]["workspace_token"].as_str())
        .unwrap_or_else(|| panic!("start command carries the workspace token: {}", starts[0]))
        .to_string();
    let claims = g5_auth::JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None)
        .expect("construct validator")
        .validate_workspace_incarnation(&new_token)
        .expect("the start token is a workspace incarnation token");
    assert_eq!(claims.org_id, org.id);
    assert_eq!(claims.workspace_id, workspace.id);
    assert_eq!(
        claims.owner_user_id, next_owner.id,
        "the new incarnation is bound to the new owner"
    );
    assert_ne!(
        claims.vm_id, old_vm_id,
        "the new incarnation is a new VM identity"
    );
    let persisted: (Uuid, Option<String>, i64) = sqlx::query_as(
        "SELECT workspace.owner_user_id, vm.vm_id, vm.epoch
         FROM workspaces workspace JOIN workspace_vms vm ON vm.workspace_id = workspace.id
         WHERE workspace.id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("load the workspace after the new owner's start");
    assert_eq!(persisted.0, next_owner.id);
    assert_eq!(
        persisted.1.as_deref(),
        Some(claims.vm_id.to_string().as_str())
    );
    assert_eq!(persisted.2, claims.epoch as i64);

    let (status, body) = flush_start(&app, workspace.id, &new_token, "after").await;
    assert_eq!(status, StatusCode::OK, "new token flush: {body}");
    assert_eq!(body["vm_id"], claims.vm_id.to_string());
    let (status, body) = flush_start(&app, workspace.id, &old_token, "after").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "old token after new start: {body}"
    );

    app.cleanup().await;
}
