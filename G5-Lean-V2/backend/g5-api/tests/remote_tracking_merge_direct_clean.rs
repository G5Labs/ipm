//! Remote reconciliation requires an agent in an already-running workspace,
//! including divergences with disjoint edits. Completion records the published
//! Git advance and queues provider publication; provider rewrites need approval.

use crate::helpers;

use g5_contracts::types::WorkspaceRepoKey;
use g5_scm::GitRepo;
use helpers::{finalize_authority_head, TestApp, TestOrg, TestUser, TestWorkspace};
use serde_json::{json, Value};
use std::process::Command;
use uuid::Uuid;

fn git_head(repo_path: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .expect("git rev-parse runs");
    assert!(
        out.status.success(),
        "git rev-parse HEAD failed in {repo_path:?}"
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Set the member's remote policy and seed the Git sync watermark.
async fn make_paused_remote_tracking_branch(
    app: &TestApp,
    workspace: &TestWorkspace,
    remote_branch: &str,
    baseline: &str,
) -> WorkspaceRepoKey {
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch,sync_paused_reason) VALUES ($1,$2,$3,$4,'diverged')")
        .bind(workspace.org_id).bind(workspace.id).bind(workspace.repo_id).bind(remote_branch)
        .execute(&app.db).await.unwrap();
    let path = app.repo_path(workspace.repo_id).await;
    GitRepo::open(&path)
        .unwrap()
        .write_baseline_ref(&workspace.name, baseline)
        .unwrap();
    WorkspaceRepoKey {
        org_id: workspace.org_id,
        workspace_id: workspace.id,
        repo_id: workspace.repo_id,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_waiting_merge_run(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    remote_branch: &str,
    workspace_tip: &str,
    mirror_tip: &str,
    baseline: &str,
) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO workspace_merge_runs
            (org_id, workspace_id, source_kind,
             source_workspace_id, target_workspace_id, repo_id,
             source_branch_name, status, phase, source_heads, target_heads,
             pre_start_source_heads, started_at)
        VALUES ($1, $2, 'remote_tracking', NULL, NULL, $3, $4,
                'waiting_for_human', 'workspace_rebase_preflight', $5, $6, $5, now())
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(repo_id)
    .bind(remote_branch)
    .bind(json!({ "workspace_tip": workspace_tip, "mirror_tip": mirror_tip }))
    .bind(json!({ "baseline": baseline }))
    .fetch_one(&app.db)
    .await
    .expect("insert waiting merge run")
}

// ---------------------------------------------------------------------------
// A clean divergence still requires the owner to start the workspace.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_clean_divergence_requires_running_workspace_and_preserves_git() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let member =
        make_paused_remote_tracking_branch(&app, &workspace, "main", &workspace.head_commit).await;

    // No VM is registered: Resolve must refuse without rewriting G5's Git
    // or implicitly starting a workspace, even for disjoint edits.

    // Identity for the tracked repo + its local branch name.
    let tracked_repo_id = workspace.repo_id;
    let local_branch_name = workspace.name.clone();
    let repo_path = app.repo_path(tracked_repo_id).await;

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo_path)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    // `git merge-base --is-ancestor <a> <b>` exits 0 iff a is an ancestor of b.
    let is_ancestor = |a: &str, b: &str| -> bool {
        Command::new("git")
            .args(["merge-base", "--is-ancestor", a, b])
            .current_dir(&repo_path)
            .status()
            .expect("git merge-base runs")
            .success()
    };

    git(&["config", "user.email", "g5@test.local"]);
    git(&["config", "user.name", "G5 Test"]);

    // Baseline `B` = the fixture HEAD; build a clean (disjoint-file) divergence.
    let baseline = git_head(&repo_path);
    // LOCAL tip `L`: the workspace's divergent commit.
    git(&["checkout", "-B", &local_branch_name, &baseline]);
    std::fs::write(repo_path.join("local_only.txt"), "local work\n").expect("write local");
    git(&["add", "local_only.txt"]);
    git(&["commit", "-m", "local divergent commit"]);
    let local_tip = git_head(&repo_path);
    // REMOTE tip `R`: a sibling commit on a different file (clean to rebase).
    git(&["checkout", "-B", "remote-sim", &baseline]);
    std::fs::write(repo_path.join("remote_only.txt"), "remote work\n").expect("write remote");
    git(&["add", "remote_only.txt"]);
    git(&["commit", "-m", "remote divergent commit"]);
    let remote_tip = git_head(&repo_path);
    git(&["update-ref", "refs/heads/main", &remote_tip]);
    // Leave the worktree on the local branch (the tracked ref points at L).
    git(&["checkout", &local_branch_name]);

    assert!(
        !is_ancestor(&remote_tip, &local_tip) && !is_ancestor(&local_tip, &remote_tip),
        "L and R must be siblings (a genuine divergence)"
    );

    // Anchor the tracked branch state at `L` (both parent + child heads).

    let merge_run_id = insert_waiting_merge_run(
        &app,
        org.id,
        workspace.id,
        member.repo_id,
        "main",
        &local_tip,  // workspace_tip = L
        &remote_tip, // mirror_tip   = R
        &baseline,
    )
    .await;

    // Resolve must keep this checkpoint pending until the workspace is running.
    let client = app.authenticated_client(&user);
    let resp = client
        .post_json::<Value>(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{}/start",
                workspace.id, workspace.repo_id, merge_run_id
            ),
            &json!({}),
        )
        .await;
    let status = resp.status();
    let response_text = resp.text().await.expect("start response body");
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{response_text}");
    let body: Value = serde_json::from_str(&response_text).expect("json");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING");

    let run: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resolution_task_id, completion_advance_id
         FROM workspace_merge_runs WHERE id = $1",
    )
    .bind(merge_run_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(run, ("waiting_for_human".into(), None, None));
    assert_eq!(workspace.git_head(&app), local_tip);
    assert_eq!(git(&["rev-parse", "refs/heads/main"]), remote_tip);

    let paused: Option<String> = sqlx::query_scalar(
        "SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(paused.as_deref(), Some("diverged"));
    for table in ["tasks", "workspace_vms", "remote_integration_queue"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE workspace_id=$1"
        ))
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 0, "Resolve must not create {table} without a VM");
    }

    app.cleanup().await;
}

#[tokio::test]
async fn external_completion_records_its_advance_and_upgrades_existing_handoff() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let member =
        make_paused_remote_tracking_branch(&app, &workspace, "main", &workspace.head_commit).await;

    let repo_path = app.repo_path(workspace.repo_id).await;
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo_path)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["config", "user.email", "g5@test.local"]);
    git(&["config", "user.name", "G5 Test"]);

    let opening_tip = workspace.head_commit.clone();
    let opening_tree = git(&["rev-parse", &format!("{opening_tip}^{{tree}}")]);
    let reviewed_remote_tip = git(&[
        "commit-tree",
        &opening_tree,
        "-p",
        &opening_tip,
        "-m",
        "reviewed remote tip",
    ]);
    let resolved_tip = git(&[
        "commit-tree",
        &opening_tree,
        "-p",
        &reviewed_remote_tip,
        "-m",
        "externally resolved tip",
    ]);
    let merge_run_id = insert_waiting_merge_run(
        &app,
        org.id,
        workspace.id,
        member.repo_id,
        "main",
        &opening_tip,
        &reviewed_remote_tip,
        &opening_tip,
    )
    .await;

    let finalized = finalize_authority_head(
        &app,
        org.id,
        member.workspace_id,
        member.repo_id,
        &resolved_tip,
    )
    .await
    .expect("commit exact externally resolved authority tip");
    let advance_id = finalized.advance_id;
    let handoff_before: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT trigger_kind, advance_id FROM remote_integration_queue \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .fetch_optional(&app.db)
    .await
    .expect("load ordinary authority handoff");
    assert_eq!(
        handoff_before,
        Some(("commit".to_string(), Some(advance_id))),
        "ordinary authority notification must precede external completion"
    );

    let client = app.authenticated_client(&user);
    let endpoint = format!(
        "/api/v1/workspaces/{}/repos/{}/merge_runs/{}/complete",
        workspace.id, workspace.repo_id, merge_run_id
    );
    let response = client.post_json::<Value>(&endpoint, &json!({})).await;
    let status = response.status();
    let body: Value = response.json().await.expect("completion response");
    assert_eq!(status, 200, "external completion failed: {body}");
    assert_eq!(body["data"]["completed"], true);

    let (run_status, linked_advance): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, completion_advance_id FROM workspace_merge_runs WHERE id = $1",
    )
    .bind(merge_run_id)
    .fetch_one(&app.db)
    .await
    .expect("load completed merge run");
    assert_eq!(run_status, "completed");
    // Advance ids carry no registry row since P1.4, so the completion cannot
    // name the landing that moved the head; it records its own advance id.
    let linked_advance =
        linked_advance.expect("the completed run must record its completion advance");
    assert_eq!(workspace.git_head(&app), resolved_tip);
    let paused_reason: Option<String> = sqlx::query_scalar(
        "SELECT sync_paused_reason FROM remote_integrations WHERE workspace_id=$1 AND repo_id=$2",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(paused_reason.is_none());
    let handoff_after: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT trigger_kind, advance_id FROM remote_integration_queue \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .fetch_optional(&app.db)
    .await
    .expect("load upgraded authority handoff");
    assert_eq!(
        handoff_after,
        Some(("push".to_string(), Some(advance_id))),
        "completion must upgrade the existing handoff to an explicit push keyed by the landing that moved the head"
    );

    let replay = client.post_json::<Value>(&endpoint, &json!({})).await;
    let replay_status = replay.status();
    let replay_body: Value = replay.json().await.expect("completion replay response");
    assert_eq!(
        replay_status, 200,
        "completion replay failed: {replay_body}"
    );
    assert_eq!(replay_body["data"]["completed"], false);
    let replayed_advance: Option<Uuid> =
        sqlx::query_scalar("SELECT completion_advance_id FROM workspace_merge_runs WHERE id = $1")
            .bind(merge_run_id)
            .fetch_one(&app.db)
            .await
            .expect("load replayed merge run");
    assert_eq!(
        replayed_advance,
        Some(linked_advance),
        "a completion replay must keep the recorded completion advance"
    );
    let handoff_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_integration_queue \
         WHERE workspace_id = $1 AND repo_id = $2 AND advance_id = $3 AND trigger_kind = 'push'",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .bind(advance_id)
    .fetch_one(&app.db)
    .await
    .expect("count replay-safe push handoff");
    assert_eq!(handoff_count, 1);

    app.cleanup().await;
}

async fn load_integration_handoff(
    app: &TestApp,
    member: WorkspaceRepoKey,
) -> Option<(String, Option<Uuid>)> {
    sqlx::query_as(
        "SELECT trigger_kind, advance_id FROM remote_integration_queue \
         WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(member.workspace_id)
    .bind(member.repo_id)
    .fetch_optional(&app.db)
    .await
    .expect("load external Git handoff")
}

// ---------------------------------------------------------------------------
// The external-Git handoff names the newest landing that woke the member.
// Advance ids carry no sequence since P1.4: a later landing's enqueue replaces
// the pending advance while the trigger precedence still applies.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn successive_landings_hand_off_the_newest_advance() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let member =
        make_paused_remote_tracking_branch(&app, &workspace, "main", &workspace.head_commit).await;
    let repo_path = app.repo_path(workspace.repo_id).await;
    let commit_on = |parent: &str, message: &str| {
        let out = Command::new("git")
            .args([
                "commit-tree",
                &format!("{parent}^{{tree}}"),
                "-p",
                parent,
                "-m",
                message,
            ])
            .current_dir(&repo_path)
            .env("GIT_AUTHOR_NAME", "G5 Test")
            .env("GIT_AUTHOR_EMAIL", "g5@test.local")
            .env("GIT_COMMITTER_NAME", "G5 Test")
            .env("GIT_COMMITTER_EMAIL", "g5@test.local")
            .output()
            .expect("git commit-tree runs");
        assert!(
            out.status.success(),
            "git commit-tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let first_head = commit_on(&workspace.head_commit, "first landing");
    let first = finalize_authority_head(
        &app,
        org.id,
        member.workspace_id,
        member.repo_id,
        &first_head,
    )
    .await
    .expect("land the first commit");
    assert_eq!(
        load_integration_handoff(&app, member).await,
        Some(("commit".to_string(), Some(first.advance_id))),
        "the first landing must hand off its own advance"
    );

    let second_head = commit_on(&first_head, "second landing");
    let second = finalize_authority_head(
        &app,
        org.id,
        member.workspace_id,
        member.repo_id,
        &second_head,
    )
    .await
    .expect("land the second commit");
    assert_ne!(second.advance_id, first.advance_id);
    assert_eq!(
        load_integration_handoff(&app, member).await,
        Some(("commit".to_string(), Some(second.advance_id))),
        "the newest landing must replace the pending advance"
    );
    let branch_name = &workspace.name;
    assert_eq!(
        git_rev_parse(&repo_path, &format!("refs/heads/{branch_name}")),
        second_head,
        "both landings moved the branch ref"
    );

    app.cleanup().await;
}

fn git_rev_parse(repo_path: &std::path::Path, reference: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", reference])
        .current_dir(repo_path)
        .output()
        .expect("git rev-parse runs");
    assert!(out.status.success(), "git rev-parse {reference} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ---------------------------------------------------------------------------
// Conflicting edits have the same running-workspace requirement.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_conflicting_divergence_requires_running_workspace() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let member =
        make_paused_remote_tracking_branch(&app, &workspace, "main", &workspace.head_commit).await;
    // No VM registered: Resolve must require a running workspace.

    let tracked_repo_id = workspace.repo_id;
    let local_branch_name = workspace.name.clone();
    let repo_path = app.repo_path(tracked_repo_id).await;

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo_path)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    git(&["config", "user.email", "g5@test.local"]);
    git(&["config", "user.name", "G5 Test"]);

    // Both sides edit the same file's same region differently.
    std::fs::write(repo_path.join("conflict.txt"), "base line\n").expect("write base");
    git(&["add", "conflict.txt"]);
    git(&["commit", "-m", "seed conflict file"]);
    let conflict_base = git_head(&repo_path);

    git(&["checkout", "-B", &local_branch_name, &conflict_base]);
    std::fs::write(repo_path.join("conflict.txt"), "local edit\n").expect("write local");
    git(&["add", "conflict.txt"]);
    git(&["commit", "-m", "local conflicting edit"]);
    let local_tip = git_head(&repo_path);

    git(&["checkout", "-B", "remote-sim", &conflict_base]);
    std::fs::write(repo_path.join("conflict.txt"), "remote edit\n").expect("write remote");
    git(&["add", "conflict.txt"]);
    git(&["commit", "-m", "remote conflicting edit"]);
    let remote_tip = git_head(&repo_path);
    git(&["update-ref", "refs/heads/main", &remote_tip]);
    git(&["checkout", &local_branch_name]);

    let merge_run_id = insert_waiting_merge_run(
        &app,
        org.id,
        workspace.id,
        member.repo_id,
        "main",
        &local_tip,
        &remote_tip,
        &conflict_base,
    )
    .await;

    let client = app.authenticated_client(&user);
    let resp = client
        .post_json::<Value>(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{}/start",
                workspace.id, workspace.repo_id, merge_run_id
            ),
            &json!({}),
        )
        .await;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING");

    let run_status: String =
        sqlx::query_scalar("SELECT status FROM workspace_merge_runs WHERE id = $1")
            .bind(merge_run_id)
            .fetch_one(&app.db)
            .await
            .expect("merge run status");
    assert_ne!(
        run_status, "completed",
        "a conflicting divergence must remain unresolved without a running workspace"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn provider_rewrite_while_waiting_opens_explicit_approval_without_launching() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let path = app.repo_path(workspace.repo_id).await;
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["config", "user.name", "G5 Test"]);
    git(&["config", "user.email", "g5@test.local"]);
    git(&[
        "commit",
        "--allow-empty",
        "-m",
        "acknowledged remote commit",
    ]);
    let watermark = git(&["rev-parse", "HEAD"]);
    make_paused_remote_tracking_branch(&app, &workspace, "main", &watermark).await;
    git(&["commit", "--allow-empty", "-m", "local work"]);
    let local = git(&["rev-parse", "HEAD"]);
    let old = insert_waiting_merge_run(
        &app,
        org.id,
        workspace.id,
        workspace.repo_id,
        "main",
        &local,
        &watermark,
        &watermark,
    )
    .await;
    // Provider main now points behind the last acknowledged synchronization cut.
    git(&["update-ref", "refs/heads/main", &workspace.head_commit]);
    let response = app
        .authenticated_client(&user)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/repos/{}/merge_runs/{old}/start",
                workspace.id, workspace.repo_id
            ),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["data"]["run"]["phase"], "reset_below_baseline",
        "{body}"
    );
    assert_eq!(body["data"]["run"]["status"], "waiting_for_human");
    assert!(body["data"]["run"]["resolutionTaskId"].is_null());
    let active: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, source_heads FROM workspace_merge_runs WHERE workspace_id=$1 AND status='waiting_for_human'"
    ).bind(workspace.id).fetch_all(&app.db).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_ne!(active[0].0, old);
    assert_eq!(active[0].1["mirror_tip"], workspace.head_commit);
    let old_evidence: (String, Value) =
        sqlx::query_as("SELECT status, source_heads FROM workspace_merge_runs WHERE id=$1")
            .bind(old)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(old_evidence.0, "cancelled");
    assert_eq!(old_evidence.1["mirror_tip"], watermark);
    assert_eq!(
        git(&["rev-parse", &format!("refs/heads/{}", workspace.name)]),
        local
    );
    assert_eq!(
        GitRepo::open(&path)
            .unwrap()
            .read_sync_watermark(&workspace.name)
            .unwrap(),
        Some(watermark)
    );
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    app.cleanup().await;
}
