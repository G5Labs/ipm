//! Personal workspace runtime (`g5.workspace.identity.reference`,
//! `g5.personal.project-docs`, `g5.personal.mount`, `g5.personal.mount-push`,
//! G5-Lean#624): Start pins Base's members as reference checkouts in the boot
//! specification, a personal incarnation reads them through Git but never
//! writes them, every other workspace mounts its owner's private repository
//! and fast-forwards its branch, and agent turns launch in the private
//! repository.

use crate::helpers;

use helpers::{
    TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn body_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// An org with a Base whose code member sits at `code`, and the owner's
/// personal workspace as a `TestWorkspace` the runtime peer can attach to.
async fn personal_with_base(
    app: &TestApp,
    owner: &TestUser,
    org: &TestOrg,
) -> (TestWorkspace, TestWorkspace) {
    let base = TestWorkspace::create_base(app, org).await;
    sqlx::query("UPDATE workspace_repos SET root_path = 'code' WHERE workspace_id = $1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .expect("base root");
    let client = app.authenticated_client(owner);
    let (status, body) = body_of(client.get(&format!("/api/v1/orgs/{}/me", org.id)).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = Uuid::parse_str(body["data"]["workspaceId"].as_str().unwrap()).unwrap();
    let repo_id = Uuid::parse_str(body["data"]["repoId"].as_str().unwrap()).unwrap();
    let name = body["data"]["name"].as_str().unwrap().to_string();
    let path = app.repo_path(repo_id).await;
    let head = g5_scm::GitRepo::open(&path)
        .unwrap()
        .read_branch_oid(&name)
        .unwrap()
        .unwrap();
    (
        base,
        TestWorkspace {
            id,
            org_id: org.id,
            repo_id,
            head_commit: head,
            name,
        },
    )
}

#[tokio::test]
async fn start_pins_base_members_as_reference_checkouts_outside_the_member_vector() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (base, personal) = personal_with_base(&app, &owner, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &personal, owner.id).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/stop", personal.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/resume", personal.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let starts = runtime.received_host_command_params("vm.start").await;
    let spec = &starts.last().expect("a vm.start was sent")["spec"];
    let members = spec["repos"].as_array().expect("members");
    assert_eq!(
        members.len(),
        1,
        "only the private repository is a member: {spec}"
    );
    assert_eq!(members[0]["root_path"], json!("personal"));
    assert_eq!(members[0]["branch"], json!(personal.name));
    assert_eq!(
        members[0]["follow_branch"],
        json!(true),
        "the personal VM follows a branch the owner's other workspaces push"
    );
    let references = spec["reference_repos"].as_array().expect("references");
    assert_eq!(references.len(), 1, "{spec}");
    assert_eq!(references[0]["repo_id"], json!(base.repo_id));
    assert_eq!(references[0]["root_path"], json!("code"));
    assert_eq!(references[0]["branch"], json!("main"));
    assert_eq!(
        references[0]["commit"],
        json!(base.head_commit),
        "pinned at Base's durable head when Start was admitted"
    );

    app.cleanup().await;
}

/// A working workspace whose member sits at `code`, as forks of Base do; a
/// member at the workspace root would overlap the mount.
async fn working_workspace(app: &TestApp, org: &TestOrg) -> TestWorkspace {
    let workspace = TestWorkspace::create(app, org).await;
    sqlx::query("UPDATE workspace_repos SET root_path = 'code' WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("member root");
    workspace
}

/// Stop and resume `workspace` through the API; the `vm.start` spec sent.
async fn restart(
    app: &TestApp,
    owner: &TestUser,
    runtime: &TestWorkspaceRuntimePeer,
    workspace_id: Uuid,
) -> Value {
    let client = app.authenticated_client(owner);
    for action in ["stop", "resume"] {
        let (status, body) = body_of(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{workspace_id}/{action}"),
                    &json!({ "operationId": Uuid::new_v4() }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{action}: {body}");
    }
    let starts = runtime.received_host_command_params("vm.start").await;
    starts.last().expect("a vm.start was sent")["spec"].clone()
}

#[tokio::test]
async fn an_ordinary_workspace_mounts_its_owners_personal_repository_at_start() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (_base, personal) = personal_with_base(&app, &owner, &org).await;
    let workspace = working_workspace(&app, &org).await;
    workspace.attach_parent(&app).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;

    let spec = restart(&app, &owner, &runtime, workspace.id).await;
    let members = spec["repos"].as_array().expect("members");
    assert_eq!(members.len(), 1, "the mount is not a member: {spec}");
    assert_eq!(members[0]["repo_id"], json!(workspace.repo_id));
    assert!(
        members[0].get("follow_branch").is_none(),
        "only the personal VM follows its branch: {spec}"
    );
    let references = spec["reference_repos"].as_array().expect("the mount");
    assert_eq!(references.len(), 1, "{spec}");
    let mount = &references[0];
    assert_eq!(mount["repo_id"], json!(personal.repo_id));
    assert_eq!(mount["root_path"], json!("personal"));
    assert_eq!(mount["branch"], json!(personal.name));
    assert_eq!(
        mount["commit"],
        json!(personal.head_commit),
        "pinned at the personal branch's durable head"
    );
    assert!(
        mount["git_url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/git/{}", personal.repo_id)),
        "{mount}"
    );
    assert!(
        mount.get("role").is_none(),
        "no role: neither image selection nor the docs alias sees it"
    );

    // A member at the mount's root keeps the Start and drops the mount.
    sqlx::query("UPDATE workspace_repos SET root_path = 'personal' WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("member root");
    let spec = restart(&app, &owner, &runtime, workspace.id).await;
    assert!(
        spec.get("reference_repos").is_none(),
        "a colliding member leaves the mount out: {spec}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_personal_incarnation_reads_base_repositories_but_never_pushes_them() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (base, personal) = personal_with_base(&app, &owner, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &personal, owner.id).await;
    let token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .unwrap()
        .mint_workspace_incarnation(org.id, personal.id, owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let advertise = |repo_id: Uuid, service: &'static str| {
        let token = token.clone();
        let url = format!("{}/git/{repo_id}/info/refs?service={service}", app.base_url);
        let client = app.client.clone();
        async move {
            client
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .expect("advertisement")
                .status()
        }
    };

    assert!(advertise(personal.repo_id, "git-upload-pack")
        .await
        .is_success());
    assert!(advertise(personal.repo_id, "git-receive-pack")
        .await
        .is_success());
    assert!(
        advertise(base.repo_id, "git-upload-pack")
            .await
            .is_success(),
        "a reference checkout fetches Base"
    );
    assert_eq!(
        advertise(base.repo_id, "git-receive-pack").await,
        StatusCode::FORBIDDEN,
        "reference checkouts are never pushed"
    );
    assert_eq!(
        advertise(other.repo_id, "git-upload-pack").await,
        StatusCode::FORBIDDEN,
        "a repository outside Base is not a reference"
    );

    // A working workspace's incarnation gains nothing from this rule.
    let working_runtime = TestWorkspaceRuntimePeer::attach(&app, &other, owner.id).await;
    let working_token = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .unwrap()
        .mint_workspace_incarnation(org.id, other.id, owner.id, working_runtime.vm_id(), 1, 3600)
        .unwrap();
    let status = app
        .client
        .get(format!(
            "{}/git/{}/info/refs?service=git-upload-pack",
            app.base_url, base.repo_id
        ))
        .bearer_auth(working_token)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test]
async fn personal_agent_turns_launch_in_the_private_repository() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (_base, personal) = personal_with_base(&app, &owner, &org).await;
    let working = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    for (workspace_id, expected) in [
        (
            personal.id,
            json!({ "relativePath": "personal", "personal": true }),
        ),
        (working.id, json!({})),
    ] {
        let (status, body) = body_of(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{workspace_id}/sessions/start"),
                    &json!({ "content": "hello" }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let task_id = Uuid::parse_str(body["data"]["taskId"].as_str().unwrap()).unwrap();
        let target: Value = sqlx::query_scalar(
            "SELECT spec -> 'agent' -> 'workspaceTarget' FROM tasks WHERE id = $1",
        )
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(target, expected);
    }

    app.cleanup().await;
}

/// The organization's VM lifecycle service, as the lifecycle loop runs it.
async fn lifecycle_for_org(app: &TestApp, org_id: Uuid) -> g5_api::services::WorkspaceVmService {
    let db = app.scoped_db_for_org(org_id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
        system_pool: app.state.system_pool.clone(),
        scope,
        hosts: app.state.preview_tunnels.clone(),
        minter: app.state.jwt_minter.clone().unwrap(),
        runtime: app.state.workspace_runtime.clone(),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.state.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}

/// Drive the retirement reconcilers until the retired personal VM stops.
async fn wait_for_retired_stop(
    app: &TestApp,
    lifecycle: &g5_api::services::WorkspaceVmService,
    workspace_id: Uuid,
) {
    for _ in 0..100 {
        let _ = lifecycle.reconcile_retired_personal(8).await;
        let _ = lifecycle.reconcile_stopping(8).await;
        let status: String =
            sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
                .bind(workspace_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        if status == "stopped" {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the retired personal VM stops");
}

/// Removing a member whose personal VM is running commits at once, then the
/// VM stops with `owner_removed` and no save window (g5.personal.retirement).
#[tokio::test]
async fn removing_a_member_stops_their_running_personal_vm() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let (_base, personal) = personal_with_base(&app, &member, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &personal, member.id).await;

    let status = app
        .authenticated_client(&owner)
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, member.id))
        .await
        .status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "removal never waits on the VM"
    );

    // The route's follow-up stop runs in the background; the reconciler is
    // its retry and is idempotent with it.
    let lifecycle = lifecycle_for_org(&app, org.id).await;
    wait_for_retired_stop(&app, &lifecycle, personal.id).await;
    // No save window: the owner is gone, so the host kills the VM at once.
    let stops = runtime.received_host_command_params("vm.stop").await;
    let last = stops.last().expect("the host was told to stop the VM");
    assert_eq!(last["mode"], json!("kill"), "{stops:?}");
    assert_eq!(last["grace_secs"], json!(0), "{stops:?}");
    let deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM workspaces WHERE id = $1")
            .bind(personal.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(deleted);

    app.cleanup().await;
}

/// Retirement's `owner_removed` stop supersedes a pending deadline stop
/// instead of waiting for the nudge deadline, and reports its outcome although
/// the workspace is already tombstoned (g5.personal.retirement).
#[tokio::test]
async fn retirement_supersedes_a_pending_deadline_stop_and_reports_its_outcome() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let (_base, personal) = personal_with_base(&app, &member, &org).await;
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &personal, member.id).await;
    let lifecycle = lifecycle_for_org(&app, org.id).await;

    let nudged = lifecycle
        .stop_as_system_for_tests(
            personal.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StopReason::Drain,
            Some(chrono::Utc::now() + chrono::TimeDelta::minutes(30)),
        )
        .await
        .expect("schedule the drain stop");
    assert_eq!(nudged.status, "running", "the drain stop only nudges");

    let status = app
        .authenticated_client(&owner)
        .delete(&format!("/api/v1/orgs/{}/members/{}", org.id, member.id))
        .await
        .status();
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Before the nudge deadline, and with retirement's kill rather than a
    // drain's save window.
    wait_for_retired_stop(&app, &lifecycle, personal.id).await;
    let stops = runtime.received_host_command_params("vm.stop").await;
    let last = stops.last().expect("the host was told to stop the VM");
    assert_eq!(last["mode"], json!("kill"), "{stops:?}");

    // A repeated retirement stop reads the tombstoned row's outcome.
    let operation_id: Uuid =
        sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id = $1")
            .bind(personal.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let outcome = lifecycle
        .stop_retired_personal(personal.id, operation_id)
        .await
        .expect("a retired personal workspace's stop reports its outcome");
    assert_eq!(outcome.status, "stopped");

    app.cleanup().await;
}

/// A clone of a repository through smart HTTP, authenticated as one VM
/// incarnation, the way the guest's own Git reaches the mount.
struct IncarnationCheckout {
    dir: std::path::PathBuf,
    auth: String,
    _root: tempfile::TempDir,
}

impl IncarnationCheckout {
    async fn clone(app: &TestApp, repo_id: Uuid, branch: &str, token: &str) -> Self {
        use base64::Engine;
        let basic =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        let auth = format!("http.extraHeader=Authorization: Basic {basic}");
        let root = tempfile::tempdir().expect("checkout root");
        let dir = root.path().join("personal");
        let checkout = Self {
            dir: dir.clone(),
            auth,
            _root: root,
        };
        let (ok, output) = checkout
            .run_in(
                checkout._root.path(),
                &[
                    "clone",
                    "--branch",
                    branch,
                    &format!("{}/git/{repo_id}", app.base_url),
                    dir.to_str().unwrap(),
                ],
            )
            .await;
        assert!(ok, "clone through the mount's read rule: {output}");
        checkout
    }

    async fn run_in(&self, cwd: &std::path::Path, args: &[&str]) -> (bool, String) {
        let output = tokio::process::Command::new("git")
            .args(["-c", &self.auth])
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "Mount")
            .env("GIT_AUTHOR_EMAIL", "mount@g5.test")
            .env("GIT_COMMITTER_NAME", "Mount")
            .env("GIT_COMMITTER_EMAIL", "mount@g5.test")
            .output()
            .await
            .expect("run git");
        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }

    async fn git(&self, args: &[&str]) -> (bool, String) {
        self.run_in(&self.dir, args).await
    }

    async fn commit(&self, file: &str) -> String {
        std::fs::write(self.dir.join(file), format!("{file}\n")).expect("write note");
        for args in [vec!["add", file], vec!["commit", "-q", "-m", file]] {
            let (ok, output) = self.git(&args).await;
            assert!(ok, "{output}");
        }
        self.git(&["rev-parse", "HEAD"]).await.1.trim().to_string()
    }

    async fn push(&self, refspecs: &[&str]) -> (bool, String) {
        let mut args = vec!["push", "--porcelain", "origin"];
        args.extend(refspecs);
        self.git(&args).await
    }
}

async fn branch_head(app: &TestApp, repo_id: Uuid, branch: &str) -> String {
    let path = app.repo_path(repo_id).await;
    g5_scm::GitRepo::open(&path)
        .unwrap()
        .read_branch_oid(branch)
        .unwrap()
        .unwrap()
}

fn incarnation_token(org_id: Uuid, workspace_id: Uuid, owner: Uuid, vm_id: Uuid) -> String {
    g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .unwrap()
        .mint_workspace_incarnation(org_id, workspace_id, owner, vm_id, 1, 3600)
        .unwrap()
}

#[tokio::test]
async fn the_owners_other_workspaces_fast_forward_the_personal_branch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (_base, personal) = personal_with_base(&app, &owner, &org).await;
    let branch = personal.name.clone();
    let branch_ref = format!("refs/heads/{branch}");
    let working = working_workspace(&app, &org).await;
    working.attach_parent(&app).await;
    let working_runtime = TestWorkspaceRuntimePeer::attach(&app, &working, owner.id).await;
    let token = incarnation_token(org.id, working.id, owner.id, working_runtime.vm_id());
    let personal_runtime = TestWorkspaceRuntimePeer::attach(&app, &personal, owner.id).await;
    let mount = IncarnationCheckout::clone(&app, personal.repo_id, &branch, &token).await;

    // Running personal VM: the push lands, credited to the personal branch.
    let first = mount.commit("from-working.md").await;
    let (ok, output) = mount.push(&[&format!("HEAD:{branch_ref}")]).await;
    assert!(ok, "fast-forward from the mount: {output}");
    assert_eq!(branch_head(&app, personal.repo_id, &branch).await, first);

    // Rewrites are refused even with an exact lease; so are other refs.
    let (ok, _) = mount
        .git(&["commit", "-q", "--amend", "-m", "rewrite"])
        .await;
    assert!(ok);
    let (ok, output) = mount
        .push(&[
            &format!("--force-with-lease={branch_ref}:{first}"),
            &format!("HEAD:{branch_ref}"),
        ])
        .await;
    assert!(!ok, "a non-fast-forward never lands: {output}");
    assert_eq!(branch_head(&app, personal.repo_id, &branch).await, first);
    let (ok, _) = mount.git(&["reset", "-q", "--hard", &first]).await;
    assert!(ok);
    for refspec in ["HEAD:refs/heads/other", "HEAD:refs/tags/v1"] {
        let (ok, output) = mount.push(&[refspec]).await;
        assert!(!ok, "{refspec}: {output}");
    }

    // Stopped with a clean, fully published receipt: the push lands and the
    // next personal Start restores from it.
    let client = app.authenticated_client(&owner);
    let (status, body) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/stop", personal.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let second = mount.commit("while-stopped.md").await;
    let (ok, output) = mount.push(&[&format!("HEAD:{branch_ref}")]).await;
    assert!(
        ok,
        "a stopped clean personal workspace takes the push: {output}"
    );
    assert_eq!(branch_head(&app, personal.repo_id, &branch).await, second);

    // Stopped holding unpublished work, or starting: refused, not queued.
    let third = mount.commit("refused.md").await;
    sqlx::query(
        "UPDATE workspace_flushes SET repos = jsonb_set(repos, '{0,dirty}', 'true')
         WHERE flush_id = (SELECT last_flush_id FROM workspace_vms WHERE workspace_id = $1)",
    )
    .bind(personal.id)
    .execute(&app.db)
    .await
    .expect("dirty receipt");
    let (ok, output) = mount.push(&[&format!("HEAD:{branch_ref}")]).await;
    assert!(!ok, "a dirty saved state refuses the push: {output}");
    sqlx::query(
        "UPDATE workspace_flushes SET repos = jsonb_set(repos, '{0,dirty}', 'false')
         WHERE flush_id = (SELECT last_flush_id FROM workspace_vms WHERE workspace_id = $1)",
    )
    .bind(personal.id)
    .execute(&app.db)
    .await
    .expect("clean receipt");
    sqlx::query(
        "UPDATE workspace_vms SET start_intent = jsonb_build_object(
             'id', $2::text, 'kind', 'resume', 'status', 'pending',
             'owner_user_id', $3::text, 'lifecycle_generation', (SELECT lifecycle_generation FROM workspaces WHERE id = $1),
             'attempts', 0,
             'created_at', now(), 'expires_at', now() + interval '1 hour')
         WHERE workspace_id = $1",
    )
    .bind(personal.id)
    .bind(Uuid::new_v4().to_string())
    .bind(owner.id.to_string())
    .execute(&app.db)
    .await
    .expect("pending start");
    let (ok, output) = mount.push(&[&format!("HEAD:{branch_ref}")]).await;
    assert!(
        !ok,
        "a starting personal workspace refuses the push: {output}"
    );
    assert_eq!(branch_head(&app, personal.repo_id, &branch).await, second);
    assert_ne!(branch_head(&app, personal.repo_id, &branch).await, third);
    sqlx::query("UPDATE workspace_vms SET start_intent = NULL WHERE workspace_id = $1")
        .bind(personal.id)
        .execute(&app.db)
        .await
        .expect("clear start");

    let (status, body) = body_of(
        client
            .post_json(
                &format!("/api/v1/workspaces/{}/resume", personal.id),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let starts = personal_runtime
        .received_host_command_params("vm.start")
        .await;
    let spec = &starts.last().expect("a vm.start was sent")["spec"];
    assert_eq!(
        spec["repos"][0]["branch_oid"],
        json!(second),
        "the next personal Start checks out the pushed head: {spec}"
    );
    assert!(
        spec.get("restore").is_none_or(Value::is_null),
        "the clean receipt is superseded by the moved head: {spec}"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn only_the_owners_workspaces_reach_their_personal_repository() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (_base, personal) = personal_with_base(&app, &owner, &org).await;
    let other = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    let (status, body) = body_of(
        app.authenticated_client(&other)
            .get(&format!("/api/v1/orgs/{}/me", org.id))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let others_personal_repo = Uuid::parse_str(body["data"]["repoId"].as_str().unwrap()).unwrap();
    let theirs = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(theirs.id)
        .bind(other.id)
        .execute(&app.db)
        .await
        .expect("their workspace");
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &theirs, other.id).await;
    let token = incarnation_token(org.id, theirs.id, other.id, runtime.vm_id());
    let advertise = |repo_id: Uuid, service: &'static str| {
        let token = token.clone();
        let url = format!("{}/git/{repo_id}/info/refs?service={service}", app.base_url);
        let client = app.client.clone();
        async move {
            client
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .expect("advertisement")
                .status()
        }
    };

    for service in ["git-upload-pack", "git-receive-pack"] {
        assert_eq!(
            advertise(personal.repo_id, service).await,
            StatusCode::FORBIDDEN,
            "another member's workspace never reaches the owner's repository ({service})"
        );
        assert!(
            advertise(others_personal_repo, service).await.is_success(),
            "it reaches its own owner's ({service})"
        );
    }

    app.cleanup().await;
}
