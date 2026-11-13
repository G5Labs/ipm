//! Git receive-pack policy matrix (design v9 §5.3).
//!
//! Every push reaches `routes::git::receive_pack` through real `git push`
//! against the smart-HTTP route, so the matrix exercises the same parsing,
//! authentication and preflight the guest and a laptop use. One case per
//! rule the policy functions state:
//!
//! - a user writes only the workspace branch of a workspace they own
//!   (`require_push_ref_updates`): protected branches, deletions, the
//!   flush namespace and any non-branch namespace are refused, as is any
//!   push by a non-owner member and the obsolete X-G5-Force-Push header;
//! - owners and current incarnations can rewrite their existing branch with
//!   an exact lease; stale leases and stale receive commands cannot overwrite
//!   a newer tip, and a missing branch cannot be recreated;
//! - a workspace incarnation updates only its own workspace branch and
//!   creates only its own flush leaves
//!   (`require_incarnation_ref_updates`): another branch, a branch
//!   create/delete, an unknown flush id, a flush ref update and any other
//!   namespace are refused;
//! - a reader without workspace access cannot even advertise refs
//!   (`require_read`);
//! - every non-lifecycle push, laptop pushes included, is admitted only while
//!   the workspace has a running, leased attachment, because the push shares
//!   the workspace mutation barrier with the VM's own writes
//!   (`acquire_push_admission`);
//! - an incarnation's token is checked by the workspace lifecycle's reader
//!   (`workspace_incarnation`), again under the admission lock, so a newer
//!   epoch refuses both its pushes and its reads.

use crate::helpers;

use base64::Engine;
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::process::Command;
use uuid::Uuid;

use helpers::{
    TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
};

/// A cloned checkout that pushes with one fixed `Authorization` header.
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
            .env("GIT_AUTHOR_NAME", "Policy Matrix")
            .env("GIT_AUTHOR_EMAIL", "policy@g5.test")
            .env("GIT_COMMITTER_NAME", "Policy Matrix")
            .env("GIT_COMMITTER_EMAIL", "policy@g5.test");
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
        self.rev_parse("HEAD").await
    }

    async fn rev_parse(&self, rev: &str) -> String {
        let out = self
            .git(&["rev-parse", rev])
            .output()
            .await
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Push the refspecs with optional extra `-c` config; returns
    /// `(success, stdout + stderr)`.
    async fn push(&self, extra_config: &[&str], refspecs: &[&str]) -> (bool, String) {
        let mut args: Vec<&str> = Vec::new();
        for config in extra_config {
            args.push("-c");
            args.push(config);
        }
        args.extend(["push", "--porcelain", "origin"]);
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

    async fn assert_leased_rewrite(&self, app: &TestApp, workspace: &TestWorkspace) -> String {
        let previous = self.rev_parse("HEAD").await;
        let base = self.rev_parse("HEAD^").await;
        let amended = self
            .git(&["commit", "--amend", "-m", "rewritten workspace commit"])
            .output()
            .await
            .expect("amend workspace commit");
        assert!(
            amended.status.success(),
            "{}",
            String::from_utf8_lossy(&amended.stderr)
        );
        let rewritten = self.rev_parse("HEAD").await;
        assert_ne!(rewritten, previous);
        let ancestry = self
            .git(&["merge-base", "--is-ancestor", &previous, &rewritten])
            .status()
            .await
            .unwrap();
        assert_eq!(ancestry.code(), Some(1), "fixture must require a rewrite");
        let branch_ref = format!("refs/heads/{}", workspace.name);
        let lease = format!("--force-with-lease={branch_ref}:{previous}");
        expect_accepted(
            "exact lease → rewritten workspace branch",
            self.push(&[], &[&lease, &format!("HEAD:{branch_ref}")])
                .await,
        );

        // The client must refuse a lease captured before another publication.
        let (ok, output) = self
            .push(&[], &[&lease, &format!("{base}:{branch_ref}")])
            .await;
        assert!(
            !ok && output.contains("stale info"),
            "stale lease must fail: {output}"
        );

        // Also exercise the server-side CAS, rather than only Git's client-side
        // lease check: this is a command negotiated before the ref moved.
        let command = format!("{previous} {base} {branch_ref}\0report-status\n");
        let packet = format!("{:04x}{command}0000", command.len() + 4);
        let refused = app
            .client
            .post(format!(
                "{}/git/{}/git-receive-pack",
                app.base_url, workspace.repo_id
            ))
            .header(
                "Authorization",
                self.auth_header
                    .strip_prefix("http.extraHeader=Authorization: ")
                    .unwrap(),
            )
            .header("Content-Type", "application/x-git-receive-pack-request")
            .body(packet)
            .send()
            .await
            .unwrap();
        assert_eq!(
            refused.status(),
            StatusCode::CONFLICT,
            "stale receive command must fail"
        );
        let remote = self
            .git(&["ls-remote", "origin", &branch_ref])
            .output()
            .await
            .unwrap();
        assert!(remote.status.success());
        assert_eq!(
            String::from_utf8_lossy(&remote.stdout)
                .split_whitespace()
                .next(),
            Some(rewritten.as_str())
        );
        rewritten
    }
}

fn expect_refused(case: &str, (ok, output): (bool, String), status: u16) {
    assert!(!ok, "{case}: push must be refused, got success:\n{output}");
    assert!(
        output.contains(&format!("error: {status}")) || output.contains(&format!(" {status}")),
        "{case}: expected HTTP {status} in git's output:\n{output}"
    );
}

fn expect_accepted(case: &str, (ok, output): (bool, String)) {
    assert!(ok, "{case}: push must succeed:\n{output}");
}

async fn add_member(app: &TestApp, org: &TestOrg, user: &TestUser) {
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member') \
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(org.id)
    .bind(user.id)
    .execute(&app.db)
    .await
    .expect("add org member");
}

#[tokio::test]
async fn user_pushes_are_confined_to_the_owned_workspace_branch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let branch = workspace.name.clone();

    let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    checkout.commit("owner-change.txt").await;

    // No attachment yet: even the owner's own branch is not writable, the
    // push shares the mutation barrier with a running VM.
    expect_refused(
        "owner → own branch without a running attachment",
        checkout
            .push(&[], &[&format!("HEAD:refs/heads/{branch}")])
            .await,
        403,
    );
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;

    // Protected: the repository's default branch.
    expect_refused(
        "owner → protected default branch",
        checkout.push(&[], &["+HEAD:refs/heads/main"]).await,
        403,
    );
    // Deletion is never permitted over the network.
    expect_refused(
        "owner → branch deletion",
        checkout
            .push(&[], &[&format!(":refs/heads/{branch}")])
            .await,
        403,
    );
    // Only refs/heads/<branch> is a user push namespace.
    expect_refused(
        "owner → tag namespace",
        checkout.push(&[], &["HEAD:refs/tags/v1"]).await,
        403,
    );
    expect_refused(
        "owner → flush namespace",
        checkout
            .push(
                &[],
                &[&format!(
                    "HEAD:refs/g5/flush/{}/{}/snapshot",
                    workspace.id,
                    Uuid::new_v4()
                )],
            )
            .await,
        403,
    );
    // The obsolete HTTP escape hatch does not bypass native Git admission.
    expect_refused(
        "owner → X-G5-Force-Push",
        checkout
            .push(
                &["http.extraHeader=X-G5-Force-Push: 1"],
                &[&format!("HEAD:refs/heads/{branch}")],
            )
            .await,
        403,
    );
    // Both fast-forwards and leased rewrites of the owned branch are permitted.
    expect_accepted(
        "owner → own workspace branch",
        checkout
            .push(&[], &[&format!("HEAD:refs/heads/{branch}")])
            .await,
    );
    let rewritten = checkout.assert_leased_rewrite(&app, &workspace).await;

    // An org member who is not the owner reads the repository but cannot
    // write the branch (§4.3 one human writer).
    let member = TestUser::create(&app).await;
    add_member(&app, &org, &member).await;
    let member_checkout =
        Checkout::clone(&app, &workspace, &format!("Bearer {}", member.jwt)).await;
    member_checkout.commit("member-change.txt").await;
    expect_refused(
        "member → owned workspace branch",
        member_checkout
            .push(&[], &[&format!("+HEAD:refs/heads/{branch}")])
            .await,
        403,
    );

    sqlx::query("UPDATE workspace_vms SET status='stopping' WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    expect_refused(
        "owner → rewrite while workspace stopping",
        checkout
            .push(
                &[],
                &[
                    &format!("--force-with-lease=refs/heads/{branch}:{rewritten}"),
                    &format!("HEAD^:refs/heads/{branch}"),
                ],
            )
            .await,
        403,
    );

    app.cleanup().await;
}

#[tokio::test]
async fn strangers_cannot_read_the_repository() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let stranger = TestUser::create(&app).await;

    let response = app
        .client
        .get(format!(
            "{}/git/{}/info/refs?service=git-upload-pack",
            app.base_url, workspace.repo_id
        ))
        .bearer_auth(&stranger.jwt)
        .send()
        .await
        .expect("advertisement request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "a user outside the org must not read the repository: {status} {body}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn incarnation_pushes_are_confined_to_its_branch_and_flush_leaves() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES ($1,$2,$3,'main')")
        .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint incarnation token");
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    let branch = workspace.name.clone();

    let checkout = Checkout::clone(&app, &workspace, &format!("Basic {basic}")).await;
    let base = checkout.rev_parse("HEAD").await;
    let head = checkout.commit("vm-change.txt").await;

    // Any branch other than the incarnation's own workspace branch.
    expect_refused(
        "incarnation → other branch",
        checkout
            .push(&[], &["+HEAD:refs/heads/some-other-branch"])
            .await,
        403,
    );
    expect_refused(
        "incarnation → protected default branch",
        checkout.push(&[], &["+HEAD:refs/heads/main"]).await,
        403,
    );
    // Workspace branches are update-only for an incarnation.
    expect_refused(
        "incarnation → branch deletion",
        checkout
            .push(&[], &[&format!(":refs/heads/{branch}")])
            .await,
        403,
    );
    // Non-branch, non-flush namespaces.
    expect_refused(
        "incarnation → tag namespace",
        checkout.push(&[], &["HEAD:refs/tags/v1"]).await,
        403,
    );
    // A flush id the API never issued for this incarnation.
    expect_refused(
        "incarnation → unknown flush id",
        checkout
            .push(
                &[],
                &[&format!(
                    "HEAD:refs/g5/flush/{}/{}/snapshot",
                    workspace.id,
                    Uuid::new_v4()
                )],
            )
            .await,
        403,
    );
    // Flush leaves other than snapshot/branch.
    let started = app
        .client
        .post(format!(
            "{}/api/internal/workspace-incarnations/{}/flushes/start",
            app.base_url, workspace.id
        ))
        .bearer_auth(&token)
        .json(&json!({"reason": "shutdown", "idempotency_key": "policy-matrix"}))
        .send()
        .await
        .expect("flush start");
    let started_body: Value = started.json().await.unwrap_or(Value::Null);
    let flush_id = started_body["flush_id"]
        .as_str()
        .expect("flush start carries the flush id")
        .to_string();
    let prefix = format!("refs/g5/flush/{}/{flush_id}", workspace.id);
    expect_refused(
        "incarnation → unknown flush leaf",
        checkout.push(&[], &[&format!("HEAD:{prefix}/other")]).await,
        403,
    );
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(queued, 0, "rejected pushes must not enqueue provider work");
    // The permitted writes: the own branch (fast-forward) and the two
    // create-only flush leaves.
    expect_accepted(
        "incarnation → own workspace branch",
        checkout
            .push(&[], &[&format!("HEAD:refs/heads/{branch}")])
            .await,
    );
    let queued: String = sqlx::query_scalar(
        "SELECT trigger_kind FROM remote_integration_queue WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(workspace.id)
    .bind(workspace.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(queued, "commit");
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let rewritten = checkout.assert_leased_rewrite(&app, &workspace).await;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(queued, 1, "accepted rewrites enqueue provider work too");
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    expect_accepted(
        "incarnation → flush leaves",
        checkout
            .push(
                &[],
                &[
                    &format!("{head}:{prefix}/snapshot"),
                    &format!("{base}:{prefix}/branch"),
                ],
            )
            .await,
    );
    // A flush leaf is immutable: moving it to a different OID is refused
    // (a same-OID replay is accepted, covered in api_workspace_vms).
    expect_refused(
        "incarnation → flush leaf update",
        checkout
            .push(&[], &[&format!("+{base}:{prefix}/snapshot")])
            .await,
        403,
    );

    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        queued, 0,
        "flush refs and rejected updates must not enqueue provider work"
    );
    sqlx::query("UPDATE workspace_vms SET epoch=epoch+1 WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    expect_refused(
        "old incarnation → rewrite",
        checkout
            .push(
                &[],
                &[
                    &format!("--force-with-lease=refs/heads/{branch}:{rewritten}"),
                    &format!("{base}:refs/heads/{branch}"),
                ],
            )
            .await,
        403,
    );
    let read = checkout
        .git(&["ls-remote", "origin"])
        .output()
        .await
        .expect("ls-remote");
    expect_refused(
        "old incarnation → read",
        (
            read.status.success(),
            String::from_utf8_lossy(&read.stderr).into_owned(),
        ),
        403,
    );
    app.cleanup().await;
}

#[tokio::test]
async fn network_rewrites_cannot_recreate_a_missing_workspace_branch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let owner_checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    let vm_checkout = Checkout::clone(&app, &workspace, &format!("Bearer {token}")).await;
    let path = app.repo_path(workspace.repo_id).await;
    let branch_ref = format!("refs/heads/{}", workspace.name);
    let removed = Command::new("git")
        .current_dir(&path)
        .args(["update-ref", "-d", &branch_ref])
        .status()
        .await
        .unwrap();
    assert!(removed.success());
    for (name, checkout) in [("owner", owner_checkout), ("incarnation", vm_checkout)] {
        expect_refused(
            name,
            checkout.push(&[], &[&format!("+HEAD:{branch_ref}")]).await,
            403,
        );
    }
    let absent = Command::new("git")
        .current_dir(&path)
        .args(["show-ref", "--verify", "--quiet", &branch_ref])
        .status()
        .await
        .unwrap();
    assert_eq!(absent.code(), Some(1));
    app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_push_automatically_updates_the_existing_pr_without_saving_again() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let target = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by) VALUES($1,$2,'main',true,$3,$3)")
        .bind(target).bind(org.id).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
        .bind(org.id)
        .bind(target)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1")
        .bind(workspace.id).bind(target).execute(&app.db).await.unwrap();
    // Merge admission evaluates only the configured merge target.
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(target)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    checkout.commit("first.txt").await;
    let refspec = format!("HEAD:refs/heads/{}", workspace.name);
    expect_accepted("initial push", checkout.push(&[], &[&refspec]).await);
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/pull-requests", workspace.id),
            &json!({"targetWorkspaceId":target}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let id: Uuid = body["data"]["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let methods = peer.received_rpc_methods().await;
    sqlx::query("UPDATE workspaces SET mutation_policy='read_only' WHERE id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        g5_api::maintenance::workspace_pull_requests::run_once(&app.state, None)
            .await
            .unwrap(),
        0
    );
    let cancel = tokio_util::sync::CancellationToken::new();
    let worker =
        g5_api::maintenance::workspace_pull_requests::spawn(app.state.clone(), cancel.clone());
    let new_head = checkout.commit("second.txt").await;
    expect_accepted(
        "push after PR creation",
        checkout.push(&[], &[&refspec]).await,
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let heads: Value =
                sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
                    .bind(id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            if heads[0]["headCommit"] == new_head {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("background PR refresh follows receive-pack");
    assert_eq!(
        peer.received_rpc_methods().await,
        methods,
        "automatic refresh must not save or push through the guest"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id=$1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    sqlx::query("UPDATE workspaces SET mutation_policy='read_write',mutation_policy_transition='freezing' WHERE id=$1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    let freezing_head = checkout.commit("third.txt").await;
    expect_accepted("push while freezing", checkout.push(&[], &[&refspec]).await);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let heads: Value =
                sqlx::query_scalar("SELECT source_heads FROM workspace_pull_requests WHERE id=$1")
                    .bind(id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            if heads[0]["headCommit"] == freezing_head {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("push completion while freezing");
    assert_eq!(peer.received_rpc_methods().await, methods);
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{target}/pull-requests/{id}/merge"),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "merge without Update PR: {body}");
    cancel.cancel();
    worker.await.unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn symbolic_workspace_ref_is_refused_before_native_receive() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES ($1,$2,$3,'main')")
        .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    let original = checkout.rev_parse("HEAD").await;
    let proposed = checkout.commit("ref-observation.txt").await;
    let path = app.repo_path(workspace.repo_id).await;
    let repo = git2::Repository::open(&path).unwrap();
    let target = "refs/heads/symbolic-target";
    repo.reference(
        target,
        git2::Oid::from_str(&original).unwrap(),
        false,
        "fixture target",
    )
    .unwrap();
    let workspace_ref = format!("refs/heads/{}", workspace.name);
    repo.reference_symbolic(&workspace_ref, target, true, "fixture alias")
        .unwrap();
    let command = format!("{original} {proposed} {workspace_ref}\0report-status\n");
    let response = app
        .client
        .post(format!(
            "{}/git/{}/git-receive-pack",
            app.base_url, workspace.repo_id
        ))
        .bearer_auth(&owner.jwt)
        .header("Content-Type", "application/x-git-receive-pack-request")
        .body(format!("{:04x}{command}0000", command.len() + 4))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response
        .text()
        .await
        .unwrap()
        .contains("symbolic workspace refs"));
    assert_eq!(repo.refname_to_id(target).unwrap().to_string(), original);
    assert_eq!(
        repo.find_reference(&workspace_ref)
            .unwrap()
            .symbolic_target()
            .unwrap(),
        Some(target)
    );
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(pending, 0, "refusal must not schedule publication");
    app.cleanup().await;
}

#[cfg(unix)]
#[test]
fn late_observation_failures_preserve_confirmed_push_effects() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let git = std::process::Command::new("which")
        .arg("git")
        .output()
        .unwrap();
    assert!(git.status.success());
    let git = String::from_utf8(git.stdout).unwrap().trim().to_owned();
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let marker = quote(&dir.path().join("after-receive").to_string_lossy());
    let arm = quote(&dir.path().join("armed").to_string_lossy());
    let attempts = quote(&dir.path().join("failed-reads").to_string_lossy());
    let wrapper = dir.path().join("git-wrapper");
    std::fs::write(&wrapper, format!(r#"#!/bin/sh
receive=no
advertise=no
for arg in "$@"; do
  case "$arg" in
    receive-pack) receive=yes;;
    --advertise-refs) advertise=yes;;
    for-each-ref) if [ -f {marker} ]; then echo attempt >> {attempts}; echo 'fatal: injected post-receive observation failure' >&2; exit 128; fi;;
  esac
done
if [ "$receive" = yes ] && [ "$advertise" = no ]; then
  mode=$(/bin/cat {arm} 2>/dev/null)
  printf 'attempt\n' >> {arm}.receives
  if [ "$mode" = disconnect ]; then
    printf started > {arm}.started
    while [ ! -f {arm}.release ]; do /bin/sleep 0.01; done
  fi
  {git} "$@"
  result=$?
  if [ "$mode" = exit ]; then echo 'fatal: injected failure after acceptance' >&2; exit 7; fi
  if [ -f {arm} ] && [ "$mode" != disconnect ]; then /usr/bin/touch {marker}; fi
  exit "$result"
fi
exec {git} "$@"
"#, git=quote(&git))).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Keep the configured executable override out of parallel test processes.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(helpers::exact_test_name(
            module_path!(),
            "late_observation_effects_child",
        ))
        .arg("--nocapture")
        .env("G5_GIT_BIN", &wrapper)
        .env("G5_OBSERVATION_TEST_DIR", dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A filter that matches nothing also exits 0: require the child to have run.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{}{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn late_observation_effects_child() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncWriteExt;
    let Ok(root) = std::env::var("G5_OBSERVATION_TEST_DIR") else {
        return;
    };
    let root = std::path::Path::new(&root);
    for mode in [
        "confirmed",
        "partial",
        "unknown",
        "exit",
        "disconnect",
        "publication",
    ] {
        for name in [
            "armed",
            "armed.started",
            "armed.release",
            "armed.receives",
            "after-receive",
            "failed-reads",
        ] {
            let _ = std::fs::remove_file(root.join(name));
        }
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES ($1,$2,$3,'main')")
        .bind(org.id).bind(workspace.id).bind(workspace.repo_id).execute(&app.db).await.unwrap();
        let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
            .unwrap()
            .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
            .unwrap();
        let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {token}")).await;
        let old = checkout.rev_parse("HEAD").await;
        let new = checkout.commit("late-read.txt").await;
        let branch = format!("refs/heads/{}", workspace.name);
        let path = app.repo_path(workspace.repo_id).await;
        let repo = git2::Repository::open(&path).unwrap();
        if mode == "publication" {
            sqlx::raw_sql("CREATE FUNCTION fail_push_publication() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected publication failure'; END $$; CREATE TRIGGER fail_push_publication BEFORE INSERT OR UPDATE ON remote_integration_queue FOR EACH ROW EXECUTE FUNCTION fail_push_publication();")
                .execute(&app.db).await.unwrap();
        }
        let mut updates = vec![(old.clone(), new.clone(), branch.clone())];
        if mode == "partial" {
            let started: Value = app
                .client
                .post(format!(
                    "{}/api/internal/workspace-incarnations/{}/flushes/start",
                    app.base_url, workspace.id
                ))
                .bearer_auth(&token)
                .json(&json!({"reason":"shutdown", "idempotency_key":"late-read"}))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let flush_id = started["flush_id"].as_str().unwrap();
            let leaf = format!("refs/g5/flush/{}/{flush_id}/snapshot", workspace.id);
            updates.push(("0".repeat(40), new.clone(), leaf.clone()));
            let hook = repo.path().join("hooks/update");
            std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
            std::fs::write(&hook, format!("#!/bin/sh\n[ \"$1\" != '{leaf}' ]\n")).unwrap();
            std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut pack = checkout
            .git(&["pack-objects", "--stdout", "--revs"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        pack.stdin
            .take()
            .unwrap()
            .write_all(format!("{new}\n^{old}\n").as_bytes())
            .await
            .unwrap();
        let packed = pack.wait_with_output().await.unwrap();
        assert!(packed.status.success());
        let mut body = Vec::new();
        for (index, (old, new, reference)) in updates.iter().enumerate() {
            let capability = if index == 0 && mode != "unknown" {
                "\0report-status"
            } else {
                ""
            };
            let line = format!("{old} {new} {reference}{capability}\n");
            body.extend_from_slice(format!("{:04x}{line}", line.len() + 4).as_bytes());
        }
        body.extend_from_slice(b"0000");
        body.extend(packed.stdout);
        let mut events = app.state.event_bus.subscribe_branch_updates();
        std::fs::write(root.join("armed"), mode).unwrap();
        let (status, message) = if mode == "disconnect" {
            use tower::ServiceExt;
            let router = axum::Router::new()
                .nest("/git/:repoId", g5_api::routes::git::router())
                .with_state(app.state.clone());
            let request = axum::http::Request::builder()
                .method("POST")
                .uri(format!("/git/{}/git-receive-pack", workspace.repo_id))
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/x-git-receive-pack-request")
                .body(axum::body::Body::from(body))
                .unwrap();
            let handler = tokio::spawn(router.oneshot(request));
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !root.join("armed.started").exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            handler.abort();
            assert!(handler.await.unwrap_err().is_cancelled());
            let pool = app.db.clone();
            let workspace_id = workspace.id;
            let contender = tokio::spawn(async move {
                sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
                    .bind(workspace_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            });
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            assert!(
                !contender.is_finished(),
                "cancelled HTTP awaiter must retain admission while Git is alive"
            );
            std::fs::write(root.join("armed.release"), "release").unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), contender)
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let queued: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1",
                    )
                    .bind(workspace.id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
                    if queued == 1 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            // The request has no response after cancellation. Remaining checks
            // inspect actual refs, queue and events from the continuing owner.
            (StatusCode::OK, String::new())
        } else {
            let response = app
                .client
                .post(format!(
                    "{}/git/{}/git-receive-pack",
                    app.base_url, workspace.repo_id
                ))
                .bearer_auth(&token)
                .header("Content-Type", "application/x-git-receive-pack-request")
                .body(body)
                .send()
                .await
                .unwrap();
            (response.status(), response.text().await.unwrap())
        };
        assert_eq!(
            repo.refname_to_id(&branch).unwrap().to_string(),
            new,
            "real receive-pack must have moved the branch"
        );
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM remote_integration_queue WHERE workspace_id=$1",
        )
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if mode == "unknown" {
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{message}");
            assert!(
                message.contains("Git changes may have occurred")
                    && message.contains("inspect current refs"),
                "{message}"
            );
            assert_eq!(queued, 0, "unconfirmed refs must not schedule publication");
            assert!(events.try_recv().is_err());
        } else {
            assert_eq!(
                queued,
                if mode == "publication" { 0 } else { 1 },
                "publication outcome must reflect the actual queue"
            );
            let event = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
                .await
                .expect("confirmed push must publish branch hint")
                .unwrap();
            let json = serde_json::to_value(event).unwrap();
            assert!(json.to_string().contains(&new));
            if mode == "confirmed" || mode == "disconnect" {
                assert_eq!(status, StatusCode::OK, "{message}");
                assert!(
                    !root.join("failed-reads").exists(),
                    "confirmed acceptance needs no later read"
                );
            } else {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{message}");
                assert!(
                    message.contains("Git confirmed an effect")
                        && message.contains(&branch)
                        && message.contains("Update PR"),
                    "{message}"
                );
                if mode == "partial" {
                    assert!(
                        repo.find_reference(&updates[1].2).is_err(),
                        "rejected sibling must remain absent"
                    );
                }
            }
        }
        assert_eq!(
            std::fs::read_to_string(root.join("armed.receives")).unwrap(),
            "attempt\n",
            "no automatic receive replay"
        );
        if mode == "partial" || mode == "unknown" {
            assert_eq!(
                std::fs::read_to_string(root.join("failed-reads")).unwrap(),
                "attempt\n",
                "no automatic read replay"
            );
        }
        app.cleanup().await;
    }
}

/// An owner's push checks the VM lease once it holds the VM row: a lease that
/// runs out while the push waits for a row held without changes is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_push_checks_the_lease_once_it_holds_the_vm_row() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    let head = checkout.commit("owner-change.txt").await;

    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = clock_timestamp() + interval '3 seconds'
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .expect("shorten the lease");
    let mut holder = app.db.begin().await.expect("begin the holder");
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *holder)
        .await
        .expect("hold the VM row");
    let refspec = format!("{head}:refs/heads/{}", workspace.name);
    let refspecs = [refspec.as_str()];
    let push = checkout.push(&[], &refspecs);
    let release = async {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'
                       AND query LIKE 'SELECT status FROM workspace_vms%')",
                )
                .fetch_one(&app.db)
                .await
                .expect("read lock waits");
                let expired: bool = sqlx::query_scalar(
                    "SELECT lease_expires_at <= clock_timestamp() FROM workspace_vms
                     WHERE workspace_id = $1",
                )
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .expect("read the lease");
                if waiting && expired {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the push waits for the VM row until its lease ends");
        holder.rollback().await.expect("release the VM row");
    };
    let (pushed, ()) = tokio::join!(push, release);
    expect_refused("owner → own branch after its lease ran out", pushed, 403);
    app.cleanup().await;
}

/// Receive-pack checks an incarnation's ref updates without locks, then
/// admits the push under the VM row's lock through the lifecycle's reader. A
/// newer incarnation that commits while the push waits for that lock refuses
/// the push.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn incarnation_push_is_rechecked_under_the_admission_lock() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .expect("mint incarnation token");
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    let checkout = Checkout::clone(&app, &workspace, &format!("Basic {basic}")).await;
    let base = checkout.rev_parse("HEAD").await;
    let head = checkout.commit("raced.txt").await;

    // The newer incarnation stays uncommitted until the push waits for it.
    let mut replacement = app.db.begin().await.expect("begin the replacement");
    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&mut *replacement)
        .await
        .expect("replace the incarnation");
    let refspec = format!("{head}:refs/heads/{}", workspace.name);
    let refspecs = [refspec.as_str()];
    let push = checkout.push(&[], &refspecs);
    let commit_replacement = async {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'
                       AND query LIKE '%lookup_workspace_runtime_authority%'
                       AND query LIKE '%FOR SHARE OF vm%')",
                )
                .fetch_one(&app.db)
                .await
                .expect("read lock waits");
                if waiting {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the push waits for the VM row at admission");
        replacement.commit().await.expect("commit the replacement");
    };
    let (pushed, ()) = tokio::join!(push, commit_replacement);
    expect_refused("raced incarnation → own branch", pushed, 403);
    let owner_view = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    assert_eq!(
        owner_view.rev_parse("HEAD").await,
        base,
        "the refused push must not move the branch"
    );
    app.cleanup().await;
}

/// A user's push needs `workspaces:write` from PermissionService's scope
/// union, the rule every other write uses. Only a workspace's owner pushes,
/// and owners are always members with that scope by role, so the grant sources
/// are checked on the union itself. Each source (organization default, user
/// grant, workspace override) contributes within the role ceiling: a member
/// gains a scope their role lacks, while a viewer never gains write
/// (`g5.organization.viewer-ceiling`).
#[tokio::test]
async fn push_write_permission_is_the_permission_service_union() {
    use g5_api::services::PermissionService;

    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let member = TestUser::create(&app).await;
    add_member(&app, &org, &member).await;
    let viewer = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .expect("add viewer");
    let holds = |user_id: Uuid, scope: &'static str| {
        let pool = app.state.system_pool.clone();
        let (org_id, workspace_id) = (org.id, workspace.id);
        async move {
            PermissionService::user_holds_workspace_scope(
                &pool,
                user_id,
                org_id,
                workspace_id,
                scope,
            )
            .await
            .expect("resolve the scope union")
        }
    };
    let execute = |sql: &'static str, user_id: Uuid, scope: &'static str| {
        let db = app.db.clone();
        let (org_id, workspace_id) = (org.id, workspace.id);
        async move {
            sqlx::query(sql)
                .bind(org_id)
                .bind(user_id)
                .bind(workspace_id)
                .bind(scope)
                .execute(&db)
                .await
                .expect(sql);
        }
    };

    for scope in ["workspaces:write", "tasks:execute"] {
        sqlx::query(
            "INSERT INTO oauth_scopes (scope, description, category)
             VALUES ($1, 'Test scope', 'test') ON CONFLICT (scope) DO NOTHING",
        )
        .bind(scope)
        .execute(&app.db)
        .await
        .expect("seed the scope");
    }
    assert!(holds(owner.id, "workspaces:write").await, "admin role");
    assert!(holds(member.id, "workspaces:write").await, "member role");
    assert!(
        !holds(viewer.id, "workspaces:write").await,
        "a viewer holds no write scope"
    );
    assert!(
        !holds(member.id, "tasks:execute").await,
        "a member's role lacks execute"
    );

    // Each source contributes to the union for a member; none raises a
    // viewer to write.
    for (user_id, scope, gained) in [
        (member.id, "tasks:execute", true),
        (viewer.id, "workspaces:write", false),
    ] {
        execute(
            "INSERT INTO org_default_scopes (org_id, scope)
             SELECT $1, $4 WHERE $2::uuid IS NOT NULL AND $3::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;
        assert_eq!(holds(user_id, scope).await, gained, "org default scope");
        execute(
            "DELETE FROM org_default_scopes
             WHERE org_id = $1 AND scope = $4
               AND $2::uuid IS NOT NULL AND $3::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;

        execute(
            "INSERT INTO user_scope_grants (org_id, user_id, scope, expires_at)
             SELECT $1, $2, $4, now() - interval '1 minute'
             WHERE $3::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;
        assert!(!holds(user_id, scope).await, "an expired user grant");
        execute(
            "UPDATE user_scope_grants SET expires_at = NULL
             WHERE org_id = $1 AND user_id = $2 AND scope = $4 AND $3::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;
        assert_eq!(holds(user_id, scope).await, gained, "user grant");
        execute(
            "DELETE FROM user_scope_grants
             WHERE org_id = $1 AND user_id = $2 AND scope = $4 AND $3::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;

        execute(
            "INSERT INTO workspace_permission_overrides (workspace_id, user_id, scope)
             SELECT $3, $2, $4 WHERE $1::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;
        assert_eq!(holds(user_id, scope).await, gained, "workspace override");
        execute(
            "DELETE FROM workspace_permission_overrides
             WHERE workspace_id = $3 AND user_id = $2 AND scope = $4 AND $1::uuid IS NOT NULL",
            user_id,
            scope,
        )
        .await;
    }

    app.cleanup().await;
}

/// A client that advertises a large body but never sends it must still receive
/// the repository/authentication denial. This detects buffering as well as decode
/// ordering at the real HTTP boundary.
async fn git_status_without_sending_body(
    app: &TestApp,
    repo_id: Uuid,
    service: &str,
    token: Option<&str>,
) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let url = reqwest::Url::parse(&app.base_url).unwrap();
    let mut socket = tokio::net::TcpStream::connect((
        url.host_str().unwrap(),
        url.port_or_known_default().unwrap(),
    ))
    .await
    .unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    socket
        .write_all(format!(
            "POST /git/{repo_id}/{service} HTTP/1.1\r\nHost: localhost\r\n{authorization}Content-Encoding: gzip\r\nContent-Length: 268435456\r\nConnection: close\r\n\r\n"
        ).as_bytes())
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            response.push(socket.read_u8().await.unwrap());
            if response.ends_with(b"\r\n") {
                break;
            }
        }
    })
    .await
    .expect("authorization must return without waiting for any body bytes");
    std::str::from_utf8(&response)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn git_posts_authorize_before_reading_or_decoding_the_body() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let stranger = TestUser::create(&app).await;
    let viewer = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(org.id)
        .bind(viewer.id)
        .execute(&app.db)
        .await
        .unwrap();

    for service in ["git-upload-pack", "git-receive-pack"] {
        assert_eq!(
            git_status_without_sending_body(&app, workspace.repo_id, service, None).await,
            401
        );
        assert_eq!(
            git_status_without_sending_body(&app, Uuid::new_v4(), service, Some(&owner.jwt)).await,
            404
        );
        assert!(matches!(
            git_status_without_sending_body(&app, workspace.repo_id, service, Some(&stranger.jwt))
                .await,
            403 | 404
        ));
        // Also send malformed compressed bytes, so a future eager decoder is
        // detected independently of the unread-body case.
        let response = app
            .client
            .post(format!(
                "{}/git/{}/{service}",
                app.base_url, workspace.repo_id
            ))
            .bearer_auth(&stranger.jwt)
            .header("Content-Encoding", "gzip")
            .body(vec![0xff; 32])
            .send()
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ));
    }
    // Prove this principal passes read checks before checking push denial.
    let response = app
        .client
        .get(format!(
            "{}/git/{}/info/refs?service=git-upload-pack",
            app.base_url, workspace.repo_id
        ))
        .bearer_auth(&viewer.jwt)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        git_status_without_sending_body(
            &app,
            workspace.repo_id,
            "git-receive-pack",
            Some(&viewer.jwt)
        )
        .await,
        403
    );
    let response = app
        .client
        .post(format!(
            "{}/git/{}/git-receive-pack",
            app.base_url, workspace.repo_id
        ))
        .bearer_auth(&viewer.jwt)
        .header("Content-Encoding", "gzip")
        .body(vec![0xff; 32])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let stale_token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .unwrap()
        .mint_workspace_incarnation(org.id, workspace.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    sqlx::query("UPDATE workspace_vms SET epoch=epoch+1 WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    for service in ["git-upload-pack", "git-receive-pack"] {
        assert_eq!(
            git_status_without_sending_body(&app, workspace.repo_id, service, Some(&stale_token))
                .await,
            403
        );
    }
    app.cleanup().await;
}

fn compress_git_request(encoding: &str, reader: impl std::io::Read) -> Vec<u8> {
    use flate2::{
        write::{DeflateEncoder, GzEncoder},
        Compression,
    };
    let mut reader = reader;
    if encoding == "deflate" {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        std::io::copy(&mut reader, &mut encoder).unwrap();
        encoder.finish().unwrap()
    } else {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        std::io::copy(&mut reader, &mut encoder).unwrap();
        encoder.finish().unwrap()
    }
}

#[tokio::test]
async fn git_posts_bound_wire_and_expanded_bodies_and_preserve_compressed_protocols() {
    use std::io::Read;
    const LIMIT: usize = 256 * 1024 * 1024;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let checkout = Checkout::clone(&app, &workspace, &format!("Bearer {}", owner.jwt)).await;
    let head = checkout.rev_parse("HEAD").await;
    let post = |service: &str| {
        app.client
            .post(format!(
                "{}/git/{}/{service}",
                app.base_url, workspace.repo_id
            ))
            .bearer_auth(&owner.jwt)
            .header("Content-Type", format!("application/x-{service}-request"))
    };

    for encoding in ["gzip", "x-gzip", "deflate"] {
        // Exercise native Git with valid compressed fetch and push commands.
        let want = format!("want {head}\n");
        let fetch = format!("{:04x}{want}00000009done\n", want.len() + 4);
        let response = post("git-upload-pack")
            .header("Content-Encoding", encoding)
            .body(compress_git_request(encoding, fetch.as_bytes()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{encoding} fetch");
        assert!(response
            .bytes()
            .await
            .unwrap()
            .windows(4)
            .any(|bytes| bytes == b"PACK"));

        let command = format!(
            "{head} {head} refs/heads/{}\0report-status\n",
            workspace.name
        );
        let mut push = format!("{:04x}{command}0000", command.len() + 4).into_bytes();
        let empty_pack = checkout
            .git(&["pack-objects", "--stdout"])
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .unwrap();
        assert!(empty_pack.status.success());
        push.extend(empty_pack.stdout);
        let response = post("git-receive-pack")
            .header("Content-Encoding", encoding)
            .body(compress_git_request(encoding, push.as_slice()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{encoding} push");
        let report = response.text().await.unwrap();
        assert!(
            report.contains(&format!("ok refs/heads/{}", workspace.name)),
            "{report}"
        );

        // Highly compressible bodies cross the actual production ceiling, not
        // only a small test-only cap. Construct them without an expanded buffer.
        let bomb = compress_git_request(encoding, std::io::repeat(0).take((LIMIT + 1) as u64));
        assert!(bomb.len() < LIMIT / 10);
        for service in ["git-upload-pack", "git-receive-pack"] {
            let response = post(service)
                .header("Content-Encoding", encoding)
                .body(bomb.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::PAYLOAD_TOO_LARGE,
                "{encoding} {service}"
            );
            assert!(response.text().await.unwrap().contains("size limit"));
        }
        let at_limit = compress_git_request(encoding, std::io::repeat(0).take(LIMIT as u64));
        let response = post("git-receive-pack")
            .header("Content-Encoding", encoding)
            .body(at_limit)
            .send()
            .await
            .unwrap();
        // The size is allowed; parsing rejects binary zeros as a pkt-line length.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(response.text().await.unwrap().contains("pkt-line"));
    }
    for chunks in [256, 257] {
        let chunk = bytes::Bytes::from(vec![0; 1024 * 1024]);
        let stream = futures_util::stream::iter(
            (0..chunks).map(move |_| Ok::<_, std::io::Error>(chunk.clone())),
        );
        let response = post("git-receive-pack")
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if chunks == 256 {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::PAYLOAD_TOO_LARGE
            }
        );
    }
    // Fetch uses the same explicit wire limit, including for an encoded body.
    let chunk = bytes::Bytes::from(vec![0; 1024 * 1024]);
    let stream =
        futures_util::stream::iter((0..257).map(move |_| Ok::<_, std::io::Error>(chunk.clone())));
    let response = post("git-upload-pack")
        .header("Content-Encoding", "gzip")
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    app.cleanup().await;
}
