//! Merge when ready (`g5.git.pr-auto-merge`, `g5.story.merge-when-ready`;
//! `.g5/pr_ontology_diff_design/merge-when-ready`): who turns it on and off,
//! and the background pass that merges as its author once the pull request
//! is ready, against live Postgres and the real routes.

use crate::helpers;
use crate::helpers::review_flow::{body, Flow};

use g5_auth::Scope;
use g5_scm::{GitAuthor, GitRepo};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

/// An owner's child workspace with one commit and a local-only pull request
/// into the owner's parent workspace.
struct Fixture {
    app: TestApp,
    org: TestOrg,
    owner: TestUser,
    child: TestWorkspace,
    parent: Uuid,
    pull_request: Uuid,
}

impl Fixture {
    async fn default_stop(&self, selected: bool) {
        let saved = body(
            self.app
                .authenticated_client(&self.owner)
                .patch_json(
                    "/api/v1/users/me",
                    &json!({"stopVmAfterMergeDefault": selected}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(saved["stopVmAfterMergeDefault"], selected);
    }

    async fn summary(&self) -> Value {
        let summaries = body(
            self.app
                .authenticated_client(&self.owner)
                .get(&format!(
                    "/api/v1/workspaces/{}/pull-requests?role=source&status=all",
                    self.child.id
                ))
                .await,
            StatusCode::OK,
        )
        .await;
        summaries
            .as_array()
            .unwrap()
            .iter()
            .find(|pr| pr["id"] == self.pull_request.to_string())
            .unwrap()
            .clone()
    }

    async fn stop_choice(&self, selected: bool) -> Value {
        body(
            self.app
                .authenticated_client(&self.owner)
                .patch_json(&self.path(), &json!({"stopVmAfterMerge": selected}))
                .await,
            StatusCode::OK,
        )
        .await
    }

    async fn vm_status(&self) -> String {
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id=$1")
            .bind(self.child.id)
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }

    async fn wait_for_vm_stop(&self) {
        tokio::time::timeout(Duration::from_secs(20), async {
            while self.vm_status().await != "stopped" {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("normal Stop must finish after the merge");
    }

    async fn new() -> Self {
        Self::on(TestApp::spawn().await).await
    }

    async fn on(app: TestApp) -> Self {
        Self::on_parent(app, false).await
    }

    /// The parent is a Base workspace when `base`.
    async fn on_parent(app: TestApp, base: bool) -> Self {
        helpers::tracing_capture();
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let child = TestWorkspace::create_named(&app, &org, "feature").await;
        let parent = child.attach_parent_with(&app, base).await;
        commit_on(&app, child.repo_id, &child.name, "fix.txt", "fixed\n").await;
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
        assert_eq!(
            created["pullRequest"]["localMergeStatus"], "ready",
            "{created}"
        );
        let pull_request = created["pullRequest"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        Self {
            app,
            org,
            owner,
            child,
            parent,
            pull_request,
        }
    }

    fn path(&self) -> String {
        format!(
            "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
            self.parent, self.pull_request
        )
    }

    async fn merge(&self) -> reqwest::Response {
        self.app
            .authenticated_client(&self.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/merge",
                    self.parent, self.pull_request
                ),
                &json!({}),
            )
            .await
    }

    async fn enable(&self, user: &TestUser) -> (StatusCode, Value) {
        let response = self
            .app
            .authenticated_client(user)
            .post_json(&self.path(), &json!({}))
            .await;
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn disable(&self, user: &TestUser) -> (StatusCode, Value) {
        let response = self
            .app
            .authenticated_client(user)
            .delete(&self.path())
            .await;
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn row(&self) -> (String, Option<Uuid>, Option<String>) {
        row(&self.app, self.pull_request).await
    }

    async fn events(&self, event_type: &str) -> Vec<(Uuid, Option<String>)> {
        events(&self.app, self.pull_request, event_type).await
    }

    /// A workspace-incarnation token for the child's agent.
    async fn child_agent(&self) -> (TestWorkspaceRuntimePeer, String) {
        let peer = TestWorkspaceRuntimePeer::attach(&self.app, &self.child, self.owner.id).await;
        let token = self
            .app
            .state
            .jwt_minter
            .as_ref()
            .unwrap()
            .mint_workspace_incarnation(
                self.org.id,
                self.child.id,
                self.owner.id,
                peer.vm_id(),
                1,
                3600,
            )
            .unwrap();
        (peer, token)
    }
}

/// The same personal default reaches multiple owned workspaces, while either
/// explicit choice wins, even when it matched the default when saved.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn personal_default_follows_owned_workspaces_but_explicit_choices_win() {
    let f = Fixture::new().await;
    let other = TestWorkspace::create_named(&f.app, &f.org, "second-feature").await;
    let other_parent = other.attach_parent_with(&f.app, false).await;
    let created = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/pull-requests", other.id),
                &json!({"targetWorkspaceId":other_parent}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let other_pr = created["pullRequest"]["id"].as_str().unwrap();
    let other_path =
        format!("/api/v1/workspaces/{other_parent}/pull-requests/{other_pr}/auto-merge");
    let stored: Option<bool> =
        sqlx::query_scalar("SELECT stop_vm_after_merge FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(stored, None);
    for selected in [true, false] {
        f.default_stop(selected).await;
        assert_eq!(f.summary().await["stopVmAfterMerge"], selected);
        let list = body(
            f.app
                .authenticated_client(&f.owner)
                .get(&format!(
                    "/api/v1/workspaces/{}/pull-requests?role=source&status=all",
                    other.id
                ))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(list[0]["stopVmAfterMerge"], selected);
        assert!(list[0]["autoMergeRequest"].is_null());
    }
    // Save false while the inherited value is also false.
    f.stop_choice(false).await;
    f.default_stop(true).await;
    assert_eq!(f.summary().await["stopVmAfterMerge"], false);
    // Save true while the inherited value is true, then switch the default off.
    let chosen = body(
        f.app
            .authenticated_client(&f.owner)
            .patch_json(&other_path, &json!({"stopVmAfterMerge":true}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(chosen["pullRequest"]["stopVmAfterMerge"], true);
    f.default_stop(false).await;
    let list = body(
        f.app
            .authenticated_client(&f.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/pull-requests?role=source&status=all",
                other.id
            ))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(list[0]["stopVmAfterMerge"], true);
    assert!(list[0]["autoMergeRequest"].is_null());
    assert!(f.summary().await["autoMergeRequest"].is_null());
    f.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transfer_uses_new_owner_default_but_never_rearms_a_completed_merge() {
    for merged in [false, true] {
        let f = Fixture::new().await;
        let (peer, _) = f.child_agent().await;
        let member = TestUser::create_with_scopes(
            &f.app,
            f.org.id,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
        )
        .await;
        body(
            f.app
                .authenticated_client(&member)
                .patch_json("/api/v1/users/me", &json!({"stopVmAfterMergeDefault":true}))
                .await,
            StatusCode::OK,
        )
        .await;
        f.stop_choice(false).await;
        if merged {
            body(f.merge().await, StatusCode::OK).await;
        }
        let transferred = body(
            f.app
                .authenticated_client(&f.owner)
                .post_json(
                    &format!("/api/v1/workspaces/{}/transfer", f.child.id),
                    &json!({"toUserId":member.id,"idempotencyKey":"default-transfer"}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(transferred["status"], "completed");
        // The reader and merge initiator are still the old owner; neither
        // their default nor their old explicit choice controls the new owner.
        assert_eq!(f.summary().await["stopVmAfterMerge"], !merged);
        if !merged {
            let (status, enabled) = f.enable(&f.owner).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(enabled["data"]["pullRequest"]["stopVmAfterMerge"], true);
        }
        let stored: Option<bool> = sqlx::query_scalar(
            "SELECT stop_vm_after_merge FROM workspace_pull_requests WHERE id=$1",
        )
        .bind(f.pull_request)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
        assert_eq!(stored, if merged { Some(false) } else { None });
        drop(peer);
        f.app.cleanup().await;
    }
}

/// Both automatic paths start normal Stop after merge, only when selected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn optional_vm_stop_covers_both_automatic_paths_and_both_choices() {
    for (with_agent, inherited) in [(false, false), (true, false), (false, true), (true, true)] {
        for selected in [false, true] {
            let f = Fixture::new().await;
            let (peer, _) = f.child_agent().await;
            let settings = if inherited {
                f.default_stop(selected).await;
                json!({"pullRequest": f.summary().await})
            } else {
                f.stop_choice(selected).await
            };
            assert_eq!(settings["pullRequest"]["stopVmAfterMerge"], selected);
            assert!(settings["pullRequest"]["autoMergeRequest"].is_null());
            let sessions: i64 =
                sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id=$1")
                    .bind(f.child.id)
                    .fetch_one(&f.app.db)
                    .await
                    .unwrap();
            assert_eq!(sessions, 0, "saving a choice must not start an agent");
            let request = if with_agent {
                json!({"agent":{"requestKey":"optional-vm-stop"}})
            } else {
                json!({})
            };
            let started = body(
                f.app
                    .authenticated_client(&f.owner)
                    .post_json(&f.path(), &request)
                    .await,
                StatusCode::OK,
            )
            .await;
            if with_agent {
                let session: Uuid = started["chatSessionId"].as_str().unwrap().parse().unwrap();
                let prompt: String =
                    sqlx::query_scalar("SELECT content FROM session_inputs WHERE session_id=$1")
                        .bind(session)
                        .fetch_one(&f.app.db)
                        .await
                        .unwrap();
                assert_eq!(prompt.contains("Stop VM after merge is selected"), selected);
                assert!(
                    !prompt.contains("g5d workspace stop"),
                    "the API owns shutdown"
                );
            }
            assert_eq!(pass(&f.app).await, 1);
            assert_eq!(f.row().await.0, "merged");
            if inherited {
                f.default_stop(!selected).await;
                assert_eq!(
                    f.summary().await["stopVmAfterMerge"],
                    selected,
                    "a completed merge keeps its choice when the default changes"
                );
            }
            if selected {
                f.wait_for_vm_stop().await;
            }
            assert_eq!(
                f.vm_status().await,
                if selected { "stopped" } else { "running" }
            );
            assert_eq!(
                peer.received_host_command_params("vm.stop").await.len(),
                usize::from(selected)
            );
            assert_eq!(pass(&f.app).await, 0, "merged PRs are not visited again");
            assert_eq!(
                peer.received_host_command_params("vm.stop").await.len(),
                usize::from(selected)
            );
            drop(peer);
            f.app.cleanup().await;
        }
    }
}

/// A workspace's save must not hold a merge response or the shared scan.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pending_workspace_save_does_not_block_the_merge_response_or_scan() {
    for direct in [false, true] {
        let f = Fixture::new().await;
        let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
            &f.app,
            &f.child,
            f.owner.id,
            "workspace.flush",
        )
        .await;
        f.stop_choice(true).await;
        if !direct {
            assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
        }
        tokio::time::timeout(Duration::from_secs(20), async {
            if direct {
                body(f.merge().await, StatusCode::OK).await;
            } else {
                assert_eq!(pass(&f.app).await, 1);
            }
        })
        .await
        .expect("the merge response and scan must finish while the save is held");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !peer
                .received_rpc_methods()
                .await
                .iter()
                .any(|method| method == "workspace.flush")
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("Stop must start saving without a delay");
        assert_eq!(f.row().await.0, "merged");
        assert_eq!(f.vm_status().await, "stopping");
        assert!(peer
            .received_host_command_params("vm.stop")
            .await
            .is_empty());
        assert_eq!(pass(&f.app).await, 0);

        peer.release_rpc_barrier();
        f.wait_for_vm_stop().await;
        assert_eq!(peer.received_host_command_params("vm.stop").await.len(), 1);
        drop(peer);
        f.app.cleanup().await;
    }
}

/// Direct shipping uses the source owner's choice even without auto-merge.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ship_to_parent_honors_stop_choice_with_or_without_merge_when_ready() {
    for (automatic, inherited) in [(false, false), (true, false), (false, true), (true, true)] {
        for selected in [false, true] {
            let f = Fixture::new().await;
            let (peer, _) = f.child_agent().await;
            if inherited {
                f.default_stop(selected).await;
            } else {
                f.stop_choice(selected).await;
            }
            if automatic {
                assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
            }
            let merged = body(f.merge().await, StatusCode::OK).await;
            assert_eq!(merged["pullRequest"]["stopVmAfterMerge"], selected);
            assert_eq!(f.row().await.0, "merged");
            if inherited {
                f.default_stop(!selected).await;
                assert_eq!(f.summary().await["stopVmAfterMerge"], selected);
            }
            if selected {
                f.wait_for_vm_stop().await;
            }
            assert_eq!(
                f.vm_status().await,
                if selected { "stopped" } else { "running" }
            );
            assert_eq!(pass(&f.app).await, 0);
            assert_eq!(
                peer.received_host_command_params("vm.stop").await.len(),
                usize::from(selected),
                "direct shipping must stop exactly once, only when selected"
            );
            drop(peer);
            f.app.cleanup().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_merge_when_ready_does_not_stop_the_vm() {
    let f = Fixture::new().await;
    let (peer, _) = f.child_agent().await;
    f.stop_choice(true).await;
    assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
    assert_eq!(f.disable(&f.owner).await.0, StatusCode::OK);
    assert_eq!(pass(&f.app).await, 0);
    assert_eq!(f.row().await.0, "open");
    assert_eq!(f.vm_status().await, "running");
    assert!(peer
        .received_host_command_params("vm.stop")
        .await
        .is_empty());
    // Disabling automatic shipping preserves the setting for a later click.
    body(f.merge().await, StatusCode::OK).await;
    f.wait_for_vm_stop().await;
    assert_eq!(peer.received_host_command_params("vm.stop").await.len(), 1);
    drop(peer);
    f.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn withdrawing_stop_choice_after_agent_launch_leaves_the_vm_running() {
    let f = Fixture::new().await;
    let (peer, _) = f.child_agent().await;
    body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(
                &f.path(),
                &json!({"stopVmAfterMerge":true,"agent":{"requestKey":"withdraw-stop"}}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    f.stop_choice(false).await;
    assert_eq!(pass(&f.app).await, 1);
    assert_eq!(f.vm_status().await, "running");
    assert!(peer
        .received_host_command_params("vm.stop")
        .await
        .is_empty());
    drop(peer);
    f.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_merge_does_not_stop_the_vm() {
    for direct in [false, true] {
        let f = Fixture::new().await;
        let (peer, _) = f.child_agent().await;
        f.stop_choice(true).await;
        if !direct {
            assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
        }
        sqlx::raw_sql(
            "CREATE FUNCTION reject_test_merge() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'test merge failure'; END $$;
             CREATE TRIGGER reject_test_merge BEFORE UPDATE OF status ON workspace_pull_requests
             FOR EACH ROW WHEN (NEW.status = 'merged') EXECUTE FUNCTION reject_test_merge();",
        )
        .execute(&f.app.db)
        .await
        .unwrap();
        if direct {
            body(f.merge().await, StatusCode::INTERNAL_SERVER_ERROR).await;
        } else {
            assert_eq!(pass(&f.app).await, 0);
        }
        assert_eq!(f.row().await.0, "open");
        assert_eq!(f.vm_status().await, "running");
        assert!(peer
            .received_host_command_params("vm.stop")
            .await
            .is_empty());
        drop(peer);
        f.app.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_choice_is_owner_only_and_frozen_after_merge() {
    let f = Fixture::new().await;
    let (peer, _) = f.child_agent().await;
    let member = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let refused = f
        .app
        .authenticated_client(&member)
        .patch_json(&f.path(), &json!({"stopVmAfterMerge":true}))
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    f.stop_choice(true).await;
    assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
    assert_eq!(pass(&f.app).await, 1);
    let refused = f
        .app
        .authenticated_client(&f.owner)
        .patch_json(&f.path(), &json!({"stopVmAfterMerge":false}))
        .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    f.wait_for_vm_stop().await;
    drop(peer);
    f.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ownership_transfer_clears_the_previous_owners_stop_choice() {
    let f = Fixture::new().await;
    let (peer, _) = f.child_agent().await;
    let member = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    f.stop_choice(true).await;
    let transferred = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/transfer", f.child.id),
                &json!({"toUserId":member.id,"idempotencyKey":"stop-choice-transfer"}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(transferred["status"], "completed");
    // The old owner still owns the target and can enable merge, but their
    // shutdown choice is cleared. Only the new source owner can select it.
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(enabled["data"]["pullRequest"]["stopVmAfterMerge"], false);
    let refused = f
        .app
        .authenticated_client(&f.owner)
        .patch_json(&f.path(), &json!({"stopVmAfterMerge":true}))
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let chosen = body(
        f.app
            .authenticated_client(&member)
            .patch_json(&f.path(), &json!({"stopVmAfterMerge":true}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(chosen["pullRequest"]["stopVmAfterMerge"], true);
    drop(peer);
    f.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_stop_does_not_undo_or_retry_the_successful_merge() {
    for direct in [false, true] {
        let f = Fixture::new().await;
        let (peer, _) = f.child_agent().await;
        f.stop_choice(true).await;
        if !direct {
            assert_eq!(f.enable(&f.owner).await.0, StatusCode::OK);
        }
        sqlx::raw_sql(
            "CREATE SEQUENCE test_stop_attempt;
             GRANT USAGE, SELECT ON SEQUENCE test_stop_attempt TO g5_tenant;
             CREATE FUNCTION reject_test_stop() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN
                 PERFORM nextval('test_stop_attempt');
                 RAISE EXCEPTION 'test transient stop admission failure';
             END $$;
             CREATE TRIGGER reject_test_stop BEFORE UPDATE OF status ON workspace_vms
             FOR EACH ROW WHEN (NEW.status = 'stopping') EXECUTE FUNCTION reject_test_stop();",
        )
        .execute(&f.app.db)
        .await
        .unwrap();
        if direct {
            body(f.merge().await, StatusCode::OK).await;
        } else {
            assert_eq!(
                pass(&f.app).await,
                1,
                "Stop refusal must not change merge success"
            );
        }
        // A sequence advance survives the rejected transaction, proving the
        // async task attempted Stop before the fault is removed.
        tokio::time::timeout(Duration::from_secs(10), async {
            while !sqlx::query_scalar::<_, bool>("SELECT is_called FROM test_stop_attempt")
                .fetch_one(&f.app.db)
                .await
                .unwrap()
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the selected Stop must be attempted");
        assert_eq!(f.row().await.0, "merged");
        assert_eq!(f.vm_status().await, "running");
        sqlx::query("DROP TRIGGER reject_test_stop ON workspace_vms")
            .execute(&f.app.db)
            .await
            .unwrap();
        assert_eq!(
            pass(&f.app).await,
            0,
            "there is no separate post-merge retry job"
        );
        assert_eq!(f.events("merged").await.len(), 1);
        assert!(peer
            .received_host_command_params("vm.stop")
            .await
            .is_empty());
        drop(peer);
        f.app.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_agent_launch_does_not_save_the_stop_choice_or_enable_merge() {
    let f = Fixture::new().await;
    let refused = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &f.path(),
            &json!({
                "stopVmAfterMerge": true, "agent": { "requestKey": "" }
            }),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let selected: Option<bool> =
        sqlx::query_scalar("SELECT stop_vm_after_merge FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(selected, None, "a failed launch must retain inheritance");
    assert_eq!(f.row().await.1, None);
    f.app.cleanup().await;
}

async fn commit_on(
    app: &TestApp,
    repo_id: Uuid,
    branch: &str,
    file: &str,
    contents: &str,
) -> String {
    let path = app.repo_path(repo_id).await;
    let repo = GitRepo::open(&path).unwrap();
    repo.checkout_branch_force(branch).unwrap();
    std::fs::write(path.join(file), contents).unwrap();
    repo.add_all().unwrap();
    repo.commit(
        "Change",
        &GitAuthor::from_profile(Some("Test"), Some("test@g5.test")),
    )
    .unwrap()
}

/// Status, author and waiting reason of a pull request.
async fn row(app: &TestApp, pull_request: Uuid) -> (String, Option<Uuid>, Option<String>) {
    sqlx::query_as(
        "SELECT status, auto_merge_enabled_by, auto_merge_reason
           FROM workspace_pull_requests WHERE id = $1",
    )
    .bind(pull_request)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn events(
    app: &TestApp,
    pull_request: Uuid,
    event_type: &str,
) -> Vec<(Uuid, Option<String>)> {
    sqlx::query_as(
        "SELECT actor_id, message FROM workspace_pull_request_events
          WHERE pull_request_id = $1 AND event_type = $2 ORDER BY created_at, id",
    )
    .bind(pull_request)
    .bind(event_type)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

async fn pass(app: &TestApp) -> usize {
    g5_api::auto_merge::run_once(&app.state).await.unwrap()
}

/// G5 waits for the target's required approval, then merges as the person
/// who turned merge when ready on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_waits_for_approvals_then_merges_as_its_author() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "feature.txt", b"feature\n")
        .await;
    let submitted = f.submit(&app).await;
    let pull_request: Uuid = submitted["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let owner = app.authenticated_client(&f.owner);
    let path = format!(
        "/api/v1/workspaces/{}/pull-requests/{pull_request}/auto-merge",
        f.parent.id
    );
    let enabled = body(owner.post_json(&path, &json!({})).await, StatusCode::OK).await;
    let request = &enabled["pullRequest"]["autoMergeRequest"];
    assert_eq!(request["enabledBy"], f.owner.id.to_string(), "{enabled}");
    assert_eq!(request["mergeMethod"], "REBASE");
    // Turning it on again is a no-op.
    body(owner.post_json(&path, &json!({})).await, StatusCode::OK).await;
    assert_eq!(
        events(&app, pull_request, "auto_merge_enabled").await,
        vec![(f.owner.id, None)]
    );

    assert_eq!(pass(&app).await, 0);
    let (status, enabled_by, reason) = row(&app, pull_request).await;
    assert_eq!(status, "open");
    assert_eq!(enabled_by, Some(f.owner.id));
    assert!(reason.is_some(), "a waiting reason is recorded");
    assert!(
        events(&app, pull_request, "merge_blocked").await.is_empty(),
        "waiting for approvals attempts no merge"
    );

    let request = f.primary_request(&app, "ie").await;
    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pass(&app).await, 1);
    let merged_by: Option<Uuid> =
        sqlx::query_scalar("SELECT merged_by FROM workspace_pull_requests WHERE id = $1")
            .bind(pull_request)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(row(&app, pull_request).await.0, "merged");
    assert_eq!(merged_by, Some(f.owner.id));
    assert_eq!(events(&app, pull_request, "merged").await.len(), 1);
    // A merged pull request is no longer visited.
    assert_eq!(pass(&app).await, 0);
    app.cleanup().await;
}

/// Only a person who may merge turns it on; the source workspace's agent may
/// only turn it off, and the agent's credential never reaches the person route.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_people_who_may_merge_turn_it_on_and_the_agent_only_turns_it_off() {
    let f = Fixture::new().await;
    let member = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let (status, refused) = f.enable(&member).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    let (status, refused) = f.disable(&member).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    let (_peer, token) = f.child_agent().await;
    let agent = format!(
        "{}/api/internal/workspace-agent/{}/pull-requests/{}/auto-merge",
        f.app.base_url, f.child.id, f.pull_request
    );
    for url in [agent.clone(), format!("{}{}", f.app.base_url, f.path())] {
        let response = f
            .app
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert!(
            !response.status().is_success(),
            "the agent turned it on through {url}: {}",
            response.status()
        );
    }
    assert_eq!(f.row().await.1, None);

    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    let response = f
        .app
        .client
        .post(format!("{agent}/disable"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let disabled: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{disabled}");
    assert!(disabled["data"]["pullRequest"]["autoMergeRequest"].is_null());
    assert_eq!(
        f.events("auto_merge_disabled").await,
        vec![(
            f.owner.id,
            Some("Turned off by the source workspace's agent.".to_string())
        )]
    );
    // Already off: a no-op for the owner too.
    let (status, _) = f.disable(&f.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(f.events("auto_merge_disabled").await.len(), 1);
    f.app.cleanup().await;
}

/// A decision whose author can no longer merge into the target is turned
/// off, saying why, and nothing merges.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn losing_merge_authority_turns_it_off() {
    let f = Fixture::new().await;
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    let other = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(f.parent)
        .bind(other.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    assert_eq!(pass(&f.app).await, 0);
    let (status, enabled_by, _) = f.row().await;
    assert_eq!(status, "open");
    assert_eq!(enabled_by, None);
    let disabled = f.events("auto_merge_disabled").await;
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].0, f.owner.id);
    assert!(disabled[0]
        .1
        .as_deref()
        .is_some_and(|message| message.contains("can no longer merge")));
    let notified = notifications(&f.app, f.owner.id).await;
    assert_eq!(notified.len(), 1, "{notified:?}");
    assert_eq!(
        (notified[0].0.as_str(), notified[0].1.as_str()),
        ("pull_request", "warning")
    );
    f.app.cleanup().await;
}

/// A merge attempt that did not land is not repeated until a head moves or
/// the retry interval passes, so Activity is not flooded.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_blocked_attempt_waits_for_a_change_before_retrying() {
    let f = Fixture::new().await;
    let parent_branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(f.parent)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    commit_on(
        &f.app,
        f.child.repo_id,
        &parent_branch,
        "fix.txt",
        "other\n",
    )
    .await;
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");

    assert_eq!(pass(&f.app).await, 0);
    let (status, enabled_by, reason) = f.row().await;
    assert_eq!((status.as_str(), enabled_by), ("open", Some(f.owner.id)));
    assert!(reason.is_some());
    assert_eq!(f.events("merge_blocked").await.len(), 1);

    assert_eq!(pass(&f.app).await, 0);
    assert_eq!(
        f.events("merge_blocked").await.len(),
        1,
        "an unchanged blocked pull request is not attempted again"
    );

    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_attempted_at = now() - interval '11 minutes' WHERE id = $1",
    )
    .bind(f.pull_request)
    .execute(&f.app.db)
    .await
    .unwrap();
    assert_eq!(pass(&f.app).await, 0);
    assert_eq!(f.events("merge_blocked").await.len(), 2);
    f.app.cleanup().await;
}

/// With nothing in the way, the pass merges a local-only pull request into a
/// parent without requirements, and readers see who decided.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ready_pull_request_merges_on_the_next_pass() {
    let f = Fixture::new().await;
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(pass(&f.app).await, 1);
    assert_eq!(f.row().await.0, "merged");
    let listed = body(
        f.app
            .authenticated_client(&f.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/pull-requests?role=source&status=all",
                f.child.id
            ))
            .await,
        StatusCode::OK,
    )
    .await;
    let pull = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|pull| pull["id"] == f.pull_request.to_string())
        .unwrap()
        .clone();
    assert_eq!(pull["status"], "merged");
    assert_eq!(
        pull["autoMergeRequest"]["enabledBy"],
        f.owner.id.to_string(),
        "{pull}"
    );
    f.app.cleanup().await;
}

/// External checks gate the merge even though the organization does not
/// require them, and passing checks wait until the provider heads have
/// stayed unchanged for the settle window; a new head restarts it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn passing_checks_wait_for_settled_provider_heads() {
    use crate::api_pull_request_reads::{
        check_head, checks_binding, github_app, mount_checks_token, revision_rows, CHECK_SHA,
        NEXT_SHA,
    };
    let github = wiremock::MockServer::start().await;
    let f = Fixture::on(github_app(&github).await).await;
    checks_binding(&f.app, &github, &f.owner, &f.org, &f.child).await;
    sqlx::query(
        "INSERT INTO workspace_pull_request_provider_repos
             (org_id, pull_request_id, repo_id, provider, provider_repo_full_name,
              source_remote_branch, target_remote_branch, provider_pr_number,
              provider_status, provider_head_sha)
         VALUES ($1, $2, $3, 'github', 'acme/api', 'feature', 'main', 42, 'open', $4)",
    )
    .bind(f.org.id)
    .bind(f.pull_request)
    .bind(f.child.repo_id)
    .bind(CHECK_SHA)
    .execute(&f.app.db)
    .await
    .unwrap();
    let progress = || async {
        sqlx::query_as::<_, (Option<String>, Option<Value>, Option<String>)>(
            "SELECT auto_merge_reason, auto_merge_observed_heads, auto_merge_attempt_key
               FROM workspace_pull_requests WHERE id = $1",
        )
        .bind(f.pull_request)
        .fetch_one(&f.app.db)
        .await
        .unwrap()
    };
    let checks = |sha: &'static str, conclusion: &'static str| {
        let github = &github;
        async move {
            check_head(github, sha).await;
            revision_rows(
                github,
                sha,
                json!([{"id":7,"name":"tests","head_sha":sha,"status":"completed","conclusion":conclusion}]),
                json!([]),
            )
            .await;
        }
    };
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");

    checks(CHECK_SHA, "failure").await;
    assert_eq!(pass(&f.app).await, 0);
    let (reason, heads, attempt) = progress().await;
    assert!(
        reason.as_deref().is_some_and(|r| r.contains("failing")),
        "{reason:?}"
    );
    assert_eq!((heads, attempt), (None, None));

    github.reset().await;
    mount_checks_token(&github).await;
    checks(CHECK_SHA, "success").await;
    for _ in 0..2 {
        assert_eq!(pass(&f.app).await, 0);
        let (reason, heads, attempt) = progress().await;
        assert!(
            reason.as_deref().is_some_and(|r| r.contains("two minutes")),
            "{reason:?}"
        );
        assert_eq!(heads, Some(json!({"acme/api": CHECK_SHA})));
        assert_eq!(attempt, None, "no merge is attempted while settling");
    }

    // A new provider head restarts the window.
    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_observed_since = now() - interval '3 minutes' WHERE id = $1",
    )
    .bind(f.pull_request)
    .execute(&f.app.db)
    .await
    .unwrap();
    github.reset().await;
    mount_checks_token(&github).await;
    checks(NEXT_SHA, "success").await;
    assert_eq!(pass(&f.app).await, 0);
    let (reason, heads, attempt) = progress().await;
    assert!(
        reason.as_deref().is_some_and(|r| r.contains("two minutes")),
        "{reason:?}"
    );
    assert_eq!(heads, Some(json!({"acme/api": NEXT_SHA})));
    assert_eq!(attempt, None);

    // Once settled, the merge is attempted (this fake refuses the provider
    // merge, so the attempt is recorded rather than landing).
    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_observed_since = now() - interval '3 minutes' WHERE id = $1",
    )
    .bind(f.pull_request)
    .execute(&f.app.db)
    .await
    .unwrap();
    assert_eq!(pass(&f.app).await, 0);
    let (reason, _, attempt) = progress().await;
    assert!(
        attempt.is_some(),
        "settled checks admit an attempt: {reason:?}"
    );
    f.app.cleanup().await;
}

/// g5.git.pr-auto-merge.readiness: failing checks back off the checks read
/// across passes and keep the recorded reason; pending checks are read on
/// every pass (issue #1016).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn idle_checks_back_off_pending_checks_are_read_and_merged_prs_skip_the_gate() {
    use crate::api_pull_request_reads::{
        branch_requirements, check_head, checks_binding, github_app, mount_checks_token,
        revision_rows, CHECK_SHA, NEXT_SHA,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let github = wiremock::MockServer::start().await;
    let f = Fixture::on(github_app(&github).await).await;
    checks_binding(&f.app, &github, &f.owner, &f.org, &f.child).await;
    // `tests` is required, so a running one holds the merge rather than
    // settling; the requirement stays cached for the later scenarios.
    branch_requirements(
        &github,
        "main",
        json!({"enabled":true,"required_status_checks":{"enforcement_level":"everyone","contexts":["tests"],"checks":[{"context":"tests","app_id":null}]}}),
        json!([]),
        1,
    )
    .await;
    sqlx::query(
        "INSERT INTO workspace_pull_request_provider_repos
             (org_id, pull_request_id, repo_id, provider, provider_repo_full_name,
              source_remote_branch, target_remote_branch, provider_pr_number,
              provider_status, provider_head_sha)
         VALUES ($1, $2, $3, 'github', 'acme/api', 'feature', 'main', 42, 'open', $4)",
    )
    .bind(f.org.id)
    .bind(f.pull_request)
    .bind(f.child.repo_id)
    .bind(CHECK_SHA)
    .execute(&f.app.db)
    .await
    .unwrap();
    let pr_reads = || async {
        github
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path().ends_with("/pulls/42"))
            .count()
    };
    let checks = |status: &'static str, conclusion: Value| {
        let github = &github;
        async move {
            check_head(github, CHECK_SHA).await;
            revision_rows(
                github,
                CHECK_SHA,
                json!([{"id":7,"name":"tests","head_sha":CHECK_SHA,"status":status,"conclusion":conclusion}]),
                json!([]),
            )
            .await;
        }
    };
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    // A backoff no run of these passes outlasts, however slow.
    let hour = std::time::Duration::from_secs(3600);
    let mut pacing = g5_api::auto_merge::ChecksPacing::with_backoff(hour, hour);

    checks("completed", json!("failure")).await;
    for _ in 0..3 {
        g5_api::auto_merge::run_once_paced(&f.app.state, &mut pacing)
            .await
            .unwrap();
        let (_, _, reason) = f.row().await;
        assert!(
            reason.as_deref().is_some_and(|r| r.contains("failing")),
            "{reason:?}"
        );
    }
    // One settled observation (two PR reads) serves all three passes.
    assert_eq!(pr_reads().await, 2);

    github.reset().await;
    mount_checks_token(&github).await;
    checks("in_progress", Value::Null).await;
    let mut pacing = g5_api::auto_merge::ChecksPacing::with_backoff(hour, hour);
    for pass in 1..=3 {
        g5_api::auto_merge::run_once_paced(&f.app.state, &mut pacing)
            .await
            .unwrap();
        assert_eq!(pr_reads().await, pass, "pending checks are read each pass");
    }

    // A GitHub PR already merged is outside the checks gate and the settle
    // window: the first pass goes on to the merge, later ones read no checks.
    github.reset().await;
    mount_checks_token(&github).await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"number":42,"state":"closed","merged":true,"mergeable":null,"base":{"ref":"main","sha":NEXT_SHA},"head":{"sha":CHECK_SHA}})))
        .mount(&github)
        .await;
    revision_rows(
        &github,
        CHECK_SHA,
        json!([{"id":7,"name":"tests","head_sha":CHECK_SHA,"status":"completed","conclusion":"success"}]),
        json!([]),
    )
    .await;
    let mut pacing = g5_api::auto_merge::ChecksPacing::with_backoff(hour, hour);
    g5_api::auto_merge::run_once_paced(&f.app.state, &mut pacing)
        .await
        .unwrap();
    let (_, _, reason) = f.row().await;
    assert!(
        !reason.as_deref().is_some_and(|r| r.contains("two minutes")),
        "{reason:?}"
    );
    let reads = github.received_requests().await.unwrap().len();
    for _ in 0..3 {
        g5_api::auto_merge::run_once_paced(&f.app.state, &mut pacing)
            .await
            .unwrap();
    }
    assert_eq!(github.received_requests().await.unwrap().len(), reads);
    f.app.cleanup().await;
}

/// A chat in `workspace` whose agent task has the given status.
async fn chat_with_task(app: &TestApp, f: &Fixture, workspace: Uuid, status: &str) -> Uuid {
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, created_by, title, message_count)
         VALUES ($1, $2, $3, $4, 'Loop', 0)",
    )
    .bind(session)
    .bind(f.org.id)
    .bind(workspace)
    .bind(f.owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tasks(id, org_id, workspace_id, name, kind, status, created_by, chat_session_id)
         VALUES ($1, $2, $3, 'Loop', 'agent', $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(f.org.id)
    .bind(workspace)
    .bind(status)
    .bind(f.owner.id)
    .bind(session)
    .execute(&app.db)
    .await
    .unwrap();
    session
}

/// Category, severity and title of a person's notifications, oldest first.
async fn notifications(app: &TestApp, recipient: Uuid) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT category, severity, title FROM notifications
          WHERE recipient_user_id = $1 ORDER BY created_at, id",
    )
    .bind(recipient)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

/// Content and outcome of a chat's pull-request platform notices.
async fn notices(app: &TestApp, session: Uuid) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT content, artifacts #>> '{platform_notice,outcome}' FROM chat_messages
          WHERE session_id = $1 AND role = 'system'
            AND artifacts #>> '{platform_notice,kind}' = 'pull_request'
            AND artifacts #>> '{platform_notice,delivery}' = 'queued'
          ORDER BY message_index",
    )
    .bind(session)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

/// A merge by merge when ready notifies its author and leaves a notice for
/// the source workspace's working agent, not for an idle chat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_is_announced_to_its_author_and_the_working_agent() {
    let f = Fixture::new().await;
    let working = chat_with_task(&f.app, &f, f.child.id, "waiting").await;
    let idle = chat_with_task(&f.app, &f, f.child.id, "succeeded").await;
    let (status, enabled) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(pass(&f.app).await, 1);

    let notified = notifications(&f.app, f.owner.id).await;
    assert_eq!(notified.len(), 1, "{notified:?}");
    assert_eq!(
        (notified[0].0.as_str(), notified[0].1.as_str()),
        ("pull_request", "info")
    );
    assert!(
        notified[0].2.starts_with("Merged feature into"),
        "{notified:?}"
    );
    let delivered = notices(&f.app, working).await;
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert_eq!(delivered[0].1.as_deref(), Some("merged"));
    assert!(delivered[0].0.contains(&f.pull_request.to_string()));
    assert!(
        notices(&f.app, idle).await.is_empty(),
        "an idle chat is not touched"
    );
    f.app.cleanup().await;
}

/// A person is not notified of what they did themselves; the working agent
/// still hears of a turn-off and of a clicked merge.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_persons_own_turn_off_and_merge_notify_only_the_agent() {
    let f = Fixture::new().await;
    let working = chat_with_task(&f.app, &f, f.child.id, "waiting").await;
    f.enable(&f.owner).await;
    let (status, _) = f.disable(&f.owner).await;
    assert_eq!(status, StatusCode::OK);
    let merged = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{}/merge",
                    f.parent, f.pull_request
                ),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(merged["pullRequest"]["status"], "merged", "{merged}");
    assert!(notifications(&f.app, f.owner.id).await.is_empty());
    let outcomes: Vec<_> = notices(&f.app, working)
        .await
        .into_iter()
        .map(|(_, outcome)| outcome)
        .collect();
    assert_eq!(
        outcomes,
        vec![
            Some("auto_merge_disabled".to_string()),
            Some("merged".to_string())
        ]
    );
    f.app.cleanup().await;
}

/// Merge with agent turns merge when ready on and starts one loop chat in
/// the source workspace; a retry with the same key returns that chat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_with_agent_starts_a_loop_chat_in_the_source() {
    let f = Fixture::new().await;
    let request = json!({"agent": {"requestKey": "merge-with-agent-1"}});
    let started = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(&f.path(), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        started["pullRequest"]["autoMergeRequest"]["enabledBy"],
        f.owner.id.to_string(),
        "{started}"
    );
    let session: Uuid = started["chatSessionId"].as_str().unwrap().parse().unwrap();
    let workspace: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = $1")
            .bind(session)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(workspace, f.child.id);
    let content: String =
        sqlx::query_scalar("SELECT content FROM session_inputs WHERE session_id = $1")
            .bind(session)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert!(content.contains(&f.pull_request.to_string()), "{content}");
    assert!(content.contains("Never merge it yourself"), "{content}");
    // A rebase is published with a lease; the pull request follows pushes
    // (g5.git.pr-cli.push-updates). This PR has no GitHub records, so the
    // prompt names no provider forwarding, republish or external checks
    // (g5.git.pr-no-github); the GitHub variant is unit-tested with the prompt.
    assert!(
        content.contains("git push --force-with-lease="),
        "{content}"
    );
    for absent in [
        "sourceAutoPush",
        "forwards to the provider",
        "g5d pr checks",
        "g5d pr update",
    ] {
        assert!(!content.contains(absent), "{absent}: {content}");
    }

    let again = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(&f.path(), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(again["chatSessionId"], started["chatSessionId"]);
    assert_eq!(f.events("auto_merge_enabled").await.len(), 1);
    f.app.cleanup().await;
}

/// Stop the child's VM through the owner route.
async fn stop_child(f: &Fixture) {
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/stop", f.child.id),
            &json!({ "operationId": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn input_state(app: &TestApp, session: Uuid) -> (Uuid, String, Option<Uuid>) {
    sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
        "SELECT id, delivery, start_consent_id FROM session_inputs WHERE session_id = $1",
    )
    .bind(session)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// Start and ship with agent (`g5.git.pr-auto-merge.with-agent`,
/// `g5.chat.start-and-send`): on a stopped source VM one request records
/// the owner's compound Start, turns merge when ready on and queues the
/// agent's chat bound to that Start; a retry with the same key replays the
/// chat and records no second Start; a Stop that cancels the Start withdraws
/// the chat and keeps the merge-when-ready decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_and_ship_with_agent_records_the_start_and_binds_the_chat() {
    let f = Fixture::new().await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&f.app, &f.child, f.owner.id).await;
    stop_child(&f).await;
    f.stop_choice(true).await;

    let request = json!({"agent": {"requestKey": "start-and-ship-1", "startCompute": true}});
    let started = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(&f.path(), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(started["pullRequest"]["stopVmAfterMerge"], true);
    assert_eq!(
        started["startCompute"]["status"],
        json!("starting"),
        "{started}"
    );
    let intent_id: Uuid = started["startCompute"]["intentId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(f.row().await.1, Some(f.owner.id));
    let kind: String = sqlx::query_scalar(
        "SELECT start_intent ->> 'kind' FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(kind, "compound_start", "a clearly labelled compound Start");
    let session: Uuid = started["chatSessionId"].as_str().unwrap().parse().unwrap();
    let prompt: String =
        sqlx::query_scalar("SELECT content FROM session_inputs WHERE session_id=$1")
            .bind(session)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert!(prompt.contains("Stop VM after merge is selected"));
    assert!(!prompt.contains("g5d workspace stop"));
    let (input_id, delivery, bound) = input_state(&f.app, session).await;
    assert_eq!(delivery, "queued");
    assert_eq!(
        bound,
        Some(intent_id),
        "the chat is bound to its Start consent"
    );

    // A lost response retried: the same chat, one Start.
    let again = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(&f.path(), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(again["chatSessionId"], started["chatSessionId"]);
    assert!(again.get("startCompute").is_none(), "{again}");
    let intent_after_retry: Uuid = sqlx::query_scalar(
        "SELECT (start_intent ->> 'id')::uuid FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(intent_after_retry, intent_id);

    // Stop while starting cancels the consent: the chat is withdrawn, the
    // decision stays.
    stop_child(&f).await;
    let (same_input, delivery, bound) = input_state(&f.app, session).await;
    assert_eq!(same_input, input_id);
    assert_eq!(delivery, "withdrawn");
    assert_eq!(bound, None);
    assert_eq!(f.row().await.1, Some(f.owner.id));

    // A retry of the same key after that replays the claimed chat and records
    // no new Start, and it cannot revive the withdrawn chat
    // (g5.lifecycle.start): refused, with the cancelled consent untouched.
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(&f.path(), &request)
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let (_, delivery, bound) = input_state(&f.app, session).await;
    assert_eq!(delivery, "withdrawn");
    assert_eq!(bound, None);
    let (intent_after_refusal, status_after_refusal): (Uuid, String) = sqlx::query_as(
        "SELECT (start_intent ->> 'id')::uuid, start_intent ->> 'status'
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(intent_after_refusal, intent_id, "no new Start was recorded");
    assert_eq!(status_after_refusal, "cancelled");
    let chats: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(f.child.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(chats, 1);
    f.app.cleanup().await;
}

/// While the source VM runs, `startCompute` records nothing new: the chat is
/// an ordinary queued input and the response carries the running status.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_and_ship_with_agent_on_a_running_vm_just_queues() {
    let f = Fixture::new().await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&f.app, &f.child, f.owner.id).await;
    let started = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(
                &f.path(),
                &json!({"agent": {"requestKey": "start-and-ship-running", "startCompute": true}}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        started["startCompute"]["status"],
        json!("running"),
        "{started}"
    );
    let session: Uuid = started["chatSessionId"].as_str().unwrap().parse().unwrap();
    let (_, delivery, bound) = input_state(&f.app, session).await;
    assert_eq!(delivery, "queued");
    assert_eq!(bound, None);
    f.app.cleanup().await;
}

/// A Start and ship with agent addressed to the wrong workspace (the source
/// instead of the pull request's target) is refused before any VM starts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_and_ship_with_agent_refuses_a_wrong_target_before_starting() {
    let f = Fixture::new().await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&f.app, &f.child, f.owner.id).await;
    stop_child(&f).await;
    let wrong_path = format!(
        "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
        f.child.id, f.pull_request
    );
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(&wrong_path, &json!({"agent": {"startCompute": true}}))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let live_intents: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE workspace_id = $1
           AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(
        live_intents, 0,
        "no Start was recorded for a refused request"
    );
    assert_eq!(f.row().await.1, None);
    f.app.cleanup().await;
}

/// A non-owner's Start and ship with agent is refused before anything is
/// written: no Start consent, no chat, no decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_and_ship_with_agent_needs_the_source_owner() {
    let f = Fixture::new().await;
    let other = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(f.child.id)
        .bind(other.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(&f.path(), &json!({"agent": {"startCompute": true}}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.row().await.1, None);
    let intents: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_vms WHERE workspace_id = $1 AND start_intent IS NOT NULL",
    )
    .bind(f.child.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(intents, 0);
    let chats: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(f.child.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(chats, 0);
    f.app.cleanup().await;
}

/// Only the source workspace's owner can start its agent: otherwise Merge
/// with agent changes nothing, merge when ready included.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_with_agent_needs_the_source_owner() {
    let f = Fixture::new().await;
    let other = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(f.child.id)
        .bind(other.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(&f.path(), &json!({"agent": {}}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.row().await.1, None);
    let chats: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chat_sessions WHERE workspace_id = $1")
            .bind(f.child.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(chats, 0);
    // Without an agent the parent's owner may still turn it on.
    let (status, _) = f.enable(&f.owner).await;
    assert_eq!(status, StatusCode::OK);
    f.app.cleanup().await;
}

/// Starting an agent for a pull request someone else turned merge when
/// ready on for keeps their decision: G5 still merges as them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn starting_an_agent_keeps_another_persons_decision() {
    let f = Fixture::on_parent(TestApp::spawn().await, true).await;
    let admin = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[
            Scope::OrgAdmin,
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(f.org.id)
        .bind(admin.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let (status, enabled) = f.enable(&admin).await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    let started = body(
        f.app
            .authenticated_client(&f.owner)
            .post_json(&f.path(), &json!({"agent": {}}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(started["chatSessionId"].is_string(), "{started}");
    assert_eq!(f.row().await.1, Some(admin.id));
    assert_eq!(f.events("auto_merge_enabled").await.len(), 1);
    f.app.cleanup().await;
}

/// `g5.workspace.delete.pull-requests`: deletion closes a workspace's PRs
/// before it removes the workspace, so once deletion is admitted no new PR
/// opens into it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_pull_request_opens_into_a_workspace_being_deleted() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let child = TestWorkspace::create_named(&app, &org, "feature").await;
    let parent = child.attach_parent_with(&app, false).await;
    commit_on(&app, child.repo_id, &child.name, "fix.txt", "fixed\n").await;
    sqlx::query("UPDATE workspaces SET deletion_operation_id = gen_random_uuid() WHERE id = $1")
        .bind(parent)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/pull-requests", child.id),
            &json!({"targetWorkspaceId": parent}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("deletion is in progress"),
        "{body}"
    );
    let opened: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_pull_requests WHERE source_workspace_id = $1",
    )
    .bind(child.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(opened, 0);
    app.cleanup().await;
}

// --- Members may ship their own work (g5.governance.pr.source-owner-merge, #1104) -------

/// Give the review flow's source workspace to a fresh ordinary member, who
/// reads both workspaces under the fixed organization audience
/// (g5.workspace.audience), so the member owns the source while the org owner
/// keeps the parent and its approval requirement. Returns the member.
async fn hand_source_to_member(app: &TestApp, f: &Flow) -> TestUser {
    let member = TestUser::create_with_scopes(
        app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.source)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    member
}

/// A member who owns the source may turn merge when ready on — without a
/// click-time merge-authority check (decision 3) — and the background merger
/// completes the merge as that member once the target's required approval is
/// met, the approval gate still applying (decisions 1-2).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_member_owning_the_source_merges_when_ready_once_its_requirement_is_met() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "feature.txt", b"feature\n")
        .await;
    let submitted = f.submit(&app).await;
    let pull_request: Uuid = submitted["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // The review request is created while the org owner still owns the source
    // (request creation is owner-only); the member then takes the source over.
    let request = f.primary_request(&app, "ie").await;
    let member = hand_source_to_member(&app, &f).await;
    let path = format!(
        "/api/v1/workspaces/{}/pull-requests/{pull_request}/auto-merge",
        f.parent.id
    );
    let enabled = body(
        app.authenticated_client(&member)
            .post_json(&path, &json!({}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        enabled["pullRequest"]["autoMergeRequest"]["enabledBy"],
        member.id.to_string(),
        "{enabled}"
    );

    // The required approval is unmet: it waits, it is not revoked.
    assert_eq!(pass(&app).await, 0);
    let (status, enabled_by, reason) = row(&app, pull_request).await;
    assert_eq!(status, "open");
    assert_eq!(enabled_by, Some(member.id));
    assert!(reason.is_some(), "a waiting reason is recorded");

    body(
        f.decision(&app, &f.reviewer, &request, "approved").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pass(&app).await, 1);
    assert_eq!(row(&app, pull_request).await.0, "merged");
    let merged_by: Option<Uuid> =
        sqlx::query_scalar("SELECT merged_by FROM workspace_pull_requests WHERE id = $1")
            .bind(pull_request)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(merged_by, Some(member.id));
    app.cleanup().await;
}

/// The background merger rechecks the member's authority each pass and turns
/// merge when ready off, saying why, once it lapses — here the target's
/// approval requirement is retired (g5.git.pr-auto-merge).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_turns_off_when_the_members_authority_lapses() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "feature.txt", b"feature\n")
        .await;
    let submitted = f.submit(&app).await;
    let pull_request: Uuid = submitted["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let member = hand_source_to_member(&app, &f).await;
    let path = format!(
        "/api/v1/workspaces/{}/pull-requests/{pull_request}/auto-merge",
        f.parent.id
    );
    body(
        app.authenticated_client(&member)
            .post_json(&path, &json!({}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pass(&app).await, 0);
    assert_eq!(row(&app, pull_request).await.1, Some(member.id));

    // Retire the only requirement on the parent, as the org administrator.
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
    pass(&app).await;
    let (status, enabled_by, _) = row(&app, pull_request).await;
    assert_eq!(status, "open");
    assert_eq!(enabled_by, None, "the lapsed decision is turned off");
    let disabled = events(&app, pull_request, "auto_merge_disabled").await;
    assert_eq!(disabled.len(), 1, "one turn-off is recorded");
    assert!(
        disabled[0]
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("can no longer merge"),
        "the turn-off says why: {:?}",
        disabled[0].1
    );
    app.cleanup().await;
}

/// Ship to parent (an immediate merge) checks authority up front: a member
/// owning the source may merge only when the target has an attached
/// requirement — the unmet approval then blocks the merge rather than the
/// authority — and is refused outright once the requirement is retired.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_member_ships_to_parent_only_with_a_requirement() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "feature.txt", b"feature\n")
        .await;
    let submitted = f.submit(&app).await;
    let pull_request: Uuid = submitted["pullRequest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let member = hand_source_to_member(&app, &f).await;
    let merge = format!(
        "/api/v1/workspaces/{}/pull-requests/{pull_request}/merge",
        f.parent.id
    );

    // Authority holds (requirement attached); the unmet approval blocks, so
    // the response is not a permission refusal and nothing merges.
    let response = app
        .authenticated_client(&member)
        .post_json(&merge, &json!({}))
        .await;
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a source owner with a requirement holds merge authority"
    );
    assert_eq!(row(&app, pull_request).await.0, "open");

    // Retire the requirement: the member has no merge authority left.
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
    let response = app
        .authenticated_client(&member)
        .post_json(&merge, &json!({}))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(row(&app, pull_request).await.0, "open");
    app.cleanup().await;
}
