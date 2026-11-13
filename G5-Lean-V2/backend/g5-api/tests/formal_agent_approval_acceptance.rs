//! Risk-based production-boundary cases requested by the owner review.
use super::*;
use g5_contracts::types::{AgentApprovalOutcome, ApprovalReviewIdentity};

async fn assert_no_verdict(f: &Fixture, review: Uuid) {
    let status: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE agent_review_id=$1")
            .bind(review)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(status, 0);
    assert!(f.report(review).await["result"].is_null());
}

#[tokio::test]
async fn deleted_target_is_refused_before_admission_and_lifecycle_loss_fails_capture() {
    for admitted in [false, true] {
        let f = Fixture::with_connections(8).await;
        // A target must remain live during admission and deferred capture.
        let review = if admitted {
            Some(f.request(None).await)
        } else {
            None
        };
        sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
            .bind(f.target)
            .execute(&f.app.db)
            .await
            .unwrap();
        if let Some(review) = review {
            f.reconcile().await;
            assert_eq!(f.report(review).await["failureStage"], "capture");
            assert_eq!(f.report(review).await["stage"], "failed");
            assert_no_verdict(&f, review).await;
        } else {
            let (status, _) = f.request_response(None).await;
            assert!(!status.is_success());
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_approval_reviews")
                .fetch_one(&f.app.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
        let forks: i64 =
            sqlx::query_scalar("SELECT count(*) FROM workspace_forks WHERE source_workspace_id=$1")
                .bind(f.source.id)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(forks, 0);
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn two_repository_coordinator_settles_exact_vectors_and_mismatches_fail_capture() {
    for matching in [true, false] {
        let mut f = Fixture::with_connections(8).await;
        let second = TestWorkspace::create(&f.app, &f.org).await;
        let storage = f.app.repo_path(second.repo_id).await;
        let raw = git2::Repository::open(&storage).unwrap();
        let target_name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
            .bind(f.target)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
        for (workspace, name) in [(f.source.id, &f.source.name), (f.target, &target_name)] {
            if workspace == f.target && !matching {
                continue;
            }
            raw.reference(
                &format!("refs/heads/{name}"),
                git2::Oid::from_str(&second.head_commit).unwrap(),
                false,
                "second member",
            )
            .unwrap();
            sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES($1,$2,$3,'second','docs')")
                .bind(f.org.id).bind(workspace).bind(second.repo_id).execute(&f.app.db).await.unwrap();
        }
        drop(raw);
        f.refork_source().await;
        let review = if matching {
            f.settle(AgentApprovalOutcome::Approved).await
        } else {
            let review = f.request(None).await;
            f.reconcile().await;
            f.reconcile().await; // Target-cut persistence may complete before mismatch is observed.
            review
        };
        let report = f.report(review).await;
        if matching {
            assert_eq!(
                report["assignment"]["resolution"]["sourceHeads"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(
                report["assignment"]["resolution"]["targetHeads"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(report["stage"], "effective");
            assert_eq!(f.gate().await["state"], "approved");
        } else {
            assert_eq!(report["stage"], "failed", "{report}");
            assert_eq!(report["failureStage"], "capture");
            assert!(
                report["failureCause"]
                    .as_str()
                    .unwrap()
                    .contains("source and target repository vectors differ"),
                "{report}"
            );
            assert_no_verdict(&f, review).await;
        }
        f.app.cleanup().await;
    }
}

async fn attach_child(f: &Fixture, review: Uuid) -> helpers::TestWorkspaceRuntimePeer {
    let child = TestWorkspace {
        id: f.child(review).await,
        org_id: f.org.id,
        repo_id: f.source.repo_id,
        head_commit: f.source.head_commit.clone(),
        name: ApprovalReviewIdentity::new(review).workspace_branch(),
    };
    sqlx::query(
        "DELETE FROM workspace_vms WHERE workspace_id=$1 AND status='stopped' AND vm_id IS NULL",
    )
    .bind(child.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    helpers::TestWorkspaceRuntimePeer::attach(&f.app, &child, f.owner.id).await
}

#[tokio::test]
async fn execute_harvest_and_seal_deadlines_stop_the_vm_without_a_verdict() {
    for stage in ["execute", "harvest", "seal"] {
        let f = Fixture::with_connections(8).await;
        let review = f.request(None).await;
        f.reconcile().await;
        f.terminal_receipt(review, AgentApprovalOutcome::Approved)
            .await;
        if stage == "seal" {
            f.reconcile().await;
        }
        sqlx::query("UPDATE agent_approval_reviews SET stage=$2,deadline_at=now()-interval '1 second' WHERE id=$1")
            .bind(review).bind(stage).execute(&f.app.db).await.unwrap();
        let peer = attach_child(&f, review).await;
        f.reconcile().await;
        let report = f.report(review).await;
        assert_eq!(report["stage"], "failed");
        assert_eq!(report["failureStage"], stage);
        assert_no_verdict(&f, review).await;
        f.reconcile().await;
        let report = f.report(review).await;
        assert_eq!(report["cleanupPending"], false, "{report}");
        assert!(peer
            .received_host_command_methods()
            .await
            .iter()
            .any(|method| method == "vm.stop"));
        drop(peer);
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn provider_failures_are_operational_attempt_failures_with_legible_causes() {
    for (code, stage, cause) in [
        (
            "provider_auth_failed",
            "execute",
            "Provider authentication failed",
        ),
        ("process_failed", "execute", "Agent execution failed"),
        ("approval_output_invalid", "harvest", "output_validation"),
    ] {
        let f = Fixture::with_connections(8).await;
        let review = f.request(None).await;
        f.reconcile().await;
        f.terminal_receipt(review, AgentApprovalOutcome::Approved)
            .await;
        sqlx::query("UPDATE tasks SET status='failed' WHERE id=(SELECT task_id FROM agent_approval_reviews WHERE id=$1)")
            .bind(review).execute(&f.app.db).await.unwrap();
        sqlx::query("UPDATE task_attempts SET outcome=$2 WHERE task_id=(SELECT task_id FROM agent_approval_reviews WHERE id=$1)")
            .bind(review).bind(json!({"result":{"code":code}})).execute(&f.app.db).await.unwrap();
        sqlx::query("UPDATE agent_approval_reviews SET stage='execute' WHERE id=$1")
            .bind(review)
            .execute(&f.app.db)
            .await
            .unwrap();
        f.reconcile().await;
        let report = f.report(review).await;
        assert_eq!(report["stage"], "failed");
        assert_eq!(report["failureStage"], stage);
        assert!(
            report["failureCause"].as_str().unwrap().contains(cause),
            "{report}"
        );
        assert_no_verdict(&f, review).await;
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn sealing_refuses_an_actual_active_writer_and_times_out_while_waiting_for_its_workspace() {
    for blocked in [false, true] {
        let f = Fixture::with_connections(8).await;
        let review = f.request(None).await;
        f.reconcile().await;
        f.terminal_receipt(review, AgentApprovalOutcome::Approved)
            .await;
        f.reconcile().await;
        let child = f.child(review).await;
        let mut hold = if blocked {
            sqlx::query("UPDATE agent_approval_reviews SET deadline_at=now()+interval '1 second' WHERE id=$1")
                .bind(review).execute(&f.app.db).await.unwrap();
            let mut tx = f.app.db.begin().await.unwrap();
            sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR NO KEY UPDATE")
                .bind(child)
                .execute(&mut *tx)
                .await
                .unwrap();
            Some(tx)
        } else {
            sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'Active writer','command','running',$4)")
                .bind(Uuid::new_v4()).bind(f.org.id).bind(child).bind(f.owner.id).execute(&f.app.db).await.unwrap();
            None
        };
        f.reconcile().await;
        let report = f.report(review).await;
        assert_eq!(report["stage"], "failed", "{report}");
        assert_eq!(report["failureStage"], "seal");
        assert!(
            report["failureCause"]
                .as_str()
                .unwrap()
                .contains(if blocked {
                    "deadline exceeded"
                } else {
                    "active writers"
                }),
            "{report}"
        );
        assert_no_verdict(&f, review).await;
        if let Some(hold) = hold.take() {
            hold.rollback().await.unwrap();
        }
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn formal_dispatch_carries_only_provider_credentials_and_never_admits_attempt_two() {
    let f = Fixture::with_connections(8).await;
    let secrets = json!({"ANTHROPIC_API_KEY":"sk-formal-test","GH_TOKEN":"unrelated-test-secret","CUSTOM_SECRET":"other-secret","NODE_OPTIONS":"--unsafe-inherited-hook"});
    let encoded = f
        .app
        .state
        .scm_token_manager
        .as_ref()
        .unwrap()
        .encrypt_token(&secrets.to_string())
        .unwrap();
    sqlx::query("UPDATE org_configs SET secrets_enc=$2,secret_keys=$3 WHERE org_id=$1")
        .bind(f.org.id)
        .bind(encoded)
        .bind(vec![
            "ANTHROPIC_API_KEY",
            "GH_TOKEN",
            "CUSTOM_SECRET",
            "NODE_OPTIONS",
        ])
        .execute(&f.app.db)
        .await
        .unwrap();
    let review = f.request(None).await;
    f.reconcile().await;
    let peer = attach_child(&f, review).await;
    f.reconcile().await;
    let mut db = g5_api::db::scoped::scoped_db_for_system_org(&f.app.state, f.org.id, &["*"])
        .await
        .unwrap();
    let dispatch = f
        .app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .expect("formal task is dispatchable");
    assert_eq!(
        dispatch.invoke_params["version"], "1.0.13",
        "schema-6 assignments require their negotiated runtime module"
    );
    let actual = dispatch.invoke_params["secrets"].as_object().unwrap();
    assert_eq!(
        actual.get("ANTHROPIC_API_KEY"),
        Some(&json!("sk-formal-test"))
    );
    assert!(actual.keys().all(|key|g5_contracts::types::FORMAL_APPROVAL_CREDENTIAL_KEYS.contains(&key.as_str())),"{actual:?}");
    db.commit().await.unwrap();
    f.app
        .state
        .task_service
        .invoke_agent_dispatch(&dispatch)
        .await
        .unwrap();
    sqlx::query("UPDATE task_attempts SET start_confirmed=true,ended_at=now(),reason='exit_code',outcome='{}' WHERE task_id=$1")
        .bind(dispatch.task_id).execute(&f.app.db).await.unwrap();
    sqlx::query("UPDATE session_inputs SET delivery='queued',reservation_id=NULL,reservation_revision=NULL,delivered_at=NULL WHERE task_id=$1")
        .bind(dispatch.task_id).execute(&f.app.db).await.unwrap();
    sqlx::query("UPDATE tasks SET status='queued',ended_at=NULL WHERE id=$1")
        .bind(dispatch.task_id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let mut db = g5_api::db::scoped::scoped_db_for_system_org(&f.app.state, f.org.id, &["*"])
        .await
        .unwrap();
    assert!(f
        .app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .unwrap()
        .is_none());
    db.commit().await.unwrap();
    let (status, outcome): (String, Value) =
        sqlx::query_as("SELECT status,outcome FROM tasks WHERE id=$1")
            .bind(dispatch.task_id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert!(
        outcome
            .to_string()
            .contains("formal review never automatically reruns an agent"),
        "{outcome}"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE task_id=$1")
        .bind(dispatch.task_id)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    drop(peer);
    f.app.cleanup().await;
}

#[tokio::test]
async fn target_movement_does_not_change_the_assignment_or_actual_vm_upstream() {
    let f = Fixture::with_connections(12).await;
    g5_api::services::worker_host_service::WorkerHostService::new()
        .reconcile_unconfigured_org_defaults(
            &f.app.db,
            "us-central1",
            &format!(
                "mirror.gcr.io/library/buildpack-deps@sha256:{}",
                "a".repeat(64)
            ),
        )
        .await
        .unwrap();
    let host_workspace = TestWorkspace::create(&f.app, &f.org).await;
    let host = helpers::TestWorkspaceRuntimePeer::attach_with_host_resources(
        &f.app,
        &host_workspace,
        f.owner.id,
        2,
        g5_internal_api::host_ctl::VmSizeSpec {
            cpus: 2,
            mem_mib: 4096,
            scratch_disk_mib: 10240,
        },
    )
    .await;
    let review = f.request(None).await;
    f.reconcile().await;
    let before = f.report(review).await["assignment"].clone();
    let base = before["resolution"]["targetHeads"][0]["headCommit"]
        .as_str()
        .unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(f.target)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    let storage = f.app.repo_path(f.source.repo_id).await;
    let raw = git2::Repository::open(&storage).unwrap();
    let parent = raw.find_commit(git2::Oid::from_str(base).unwrap()).unwrap();
    let signature = git2::Signature::now("Target", "target@test.invalid").unwrap();
    let moved = raw
        .commit(
            None,
            &signature,
            &signature,
            "Target advanced after capture",
            &parent.tree().unwrap(),
            &[&parent],
        )
        .unwrap();
    raw.reference(&format!("refs/heads/{name}"), moved, true, "target moved")
        .unwrap();
    f.reconcile().await;
    assert_eq!(f.report(review).await["assignment"], before);
    let commands = host.received_host_command_params("vm.start").await;
    assert_eq!(commands.len(), 1, "{commands:?}");
    let command = commands[0].get("command").unwrap_or(&commands[0]);
    let repos = command["spec"]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1);
    let upstream = repos[0]["upstream_branch"].as_str().unwrap();
    assert_eq!(upstream, ApprovalReviewIdentity::new(review).base_branch());
    assert_eq!(
        raw.find_reference(&format!("refs/heads/{upstream}"))
            .unwrap()
            .target()
            .unwrap()
            .to_string(),
        base
    );
    assert_ne!(moved.to_string(), base);
    drop(parent);
    drop(raw);
    drop(host);
    f.app.cleanup().await;
}

#[tokio::test]
async fn capture_uses_the_pushed_tip_while_the_source_runtime_has_dirty_and_unpushed_work() {
    let f = Fixture::with_connections(8).await;
    let mirror = f.app.repo_path(f.source.repo_id).await;
    let scratch = tempfile::tempdir().unwrap();
    let checkout = scratch.path().join("source-vm");
    let clone = tokio::process::Command::new("git")
        .args(["clone", "--no-local", "--branch", &f.source.name])
        .arg(&mirror)
        .arg(&checkout)
        .output()
        .await
        .unwrap();
    assert!(
        clone.status.success(),
        "{}",
        String::from_utf8_lossy(&clone.stderr)
    );
    std::fs::write(checkout.join("unpushed.txt"), "unpublished commit\n").unwrap();
    for args in [
        vec!["add", "unpushed.txt"],
        vec![
            "-c",
            "user.name=VM",
            "-c",
            "user.email=vm@test.invalid",
            "commit",
            "-m",
            "Unpushed VM commit",
        ],
    ] {
        let result = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::write(checkout.join("unpushed.txt"), "dirty working content\n").unwrap();
    std::fs::write(checkout.join("untracked.txt"), "not published\n").unwrap();
    let head = tokio::process::Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .await
        .unwrap();
    assert!(head.status.success());
    assert_ne!(
        String::from_utf8(head.stdout).unwrap().trim(),
        f.source.head_commit
    );
    // The API guest peer models the active VM; this independent real checkout
    // supplies its unpublished content. Capture must not ask it to flush/read.
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&f.app, &f.source, f.owner.id).await;
    runtime
        .seed_file(
            f.source.repo_id,
            "unpushed.txt",
            &std::fs::read_to_string(checkout.join("unpushed.txt")).unwrap(),
        )
        .await;
    let review = f.request(None).await;
    f.reconcile().await;
    let report = f.report(review).await;
    assert_eq!(
        report["assignment"]["resolution"]["sourceHeads"][0]["headCommit"],
        f.source.head_commit
    );
    assert_eq!(f.source.git_head(&f.app), f.source.head_commit);
    assert!(
        runtime.received_rpc_methods().await.is_empty(),
        "pushed-only capture must not consult source runtime state"
    );
    drop(runtime);
    f.app.cleanup().await;
}

#[tokio::test]
async fn effective_end_time_follows_the_actual_seal_and_never_moves_with_cleanup() {
    let f = Fixture::with_connections(8).await;
    let review = f.request(None).await;
    f.reconcile().await;
    f.terminal_receipt(review, AgentApprovalOutcome::Approved)
        .await;
    f.reconcile().await;
    let child = f.child(review).await;
    let mut hold = f.app.db.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR NO KEY UPDATE")
        .bind(child)
        .execute(&mut *hold)
        .await
        .unwrap();
    let state = f.app.state.clone();
    let seal = tokio::spawn(async move {
        g5_api::services::approval_review_service::workflow::run_once(&state).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)))")
                .bind(holder).fetch_one(&f.app.db).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let released = chrono::Utc::now();
    hold.rollback().await.unwrap();
    seal.await.unwrap().unwrap();
    let result = f.report(review).await;
    assert_eq!(result["stage"], "effective");
    let ended = chrono::DateTime::parse_from_rfc3339(result["endedAt"].as_str().unwrap()).unwrap();
    assert!(
        ended >= released,
        "ended_at must describe the transition after the seal, not transaction start"
    );
    f.reconcile().await;
    assert_eq!(f.report(review).await["endedAt"], result["endedAt"]);
    f.app.cleanup().await;
}
