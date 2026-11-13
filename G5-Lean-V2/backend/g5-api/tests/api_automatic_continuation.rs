//! Automatic continuation of chat turns a platform interruption left behind
//! (`g5.lifecycle.start.automatic-continuation`). The lifecycle reconciler
//! starts the workspace again for its owner after a lost VM or a drain stop,
//! decides each ended incarnation once, caps loss-caused continuations
//! between owner Starts and leaves holds on the owner's decision alone.

use crate::helpers;
use g5_api::services::workspace_vm_service::StartKind;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TEST_WORKER_IMAGE_REF};
use serde_json::{json, Value};
use uuid::Uuid;

async fn workspace_vm_service(
    app: &TestApp,
    actor_user_id: Uuid,
    org_id: Uuid,
) -> g5_api::services::WorkspaceVmService {
    sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
        .bind(org_id)
        .bind(TEST_WORKER_IMAGE_REF)
        .execute(&app.db)
        .await
        .expect("set the organization image");
    let db = app.scoped_db_for_user_org(actor_user_id, org_id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
        system_pool: app.state.system_pool.clone(),
        scope,
        hosts: app.state.preview_tunnels.clone(),
        minter: app
            .state
            .jwt_minter
            .clone()
            .expect("test app configures a lifecycle JWT minter"),
        runtime: app.state.workspace_runtime.clone(),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.state.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}

/// The workspace's VM row after its incarnation `epoch` ended: `failed` with
/// a loss record for a lost VM, `stopped` for a completed stop. Custody is
/// already cleared, as every release leaves it.
async fn ended_vm(
    app: &TestApp,
    workspace: &TestWorkspace,
    status: &str,
    epoch: i64,
    record: Option<Value>,
) {
    let lossy = status == "failed";
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, epoch, status, started_at, unflushed_loss_at, last_error,
             automatic_continuation
         ) VALUES (
             $1, $2, $3, $4, now() - interval '1 hour',
             CASE WHEN $5 THEN now() END,
             CASE WHEN $5 THEN '{\"reason\":\"lease_expired\"}'::jsonb END,
             $6
         )
         ON CONFLICT (workspace_id) DO UPDATE
         SET epoch = EXCLUDED.epoch, status = EXCLUDED.status,
             host_id = NULL, vm_id = NULL, start_intent = NULL,
             started_at = EXCLUDED.started_at,
             unflushed_loss_at = EXCLUDED.unflushed_loss_at,
             last_error = EXCLUDED.last_error,
             automatic_continuation = EXCLUDED.automatic_continuation",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(epoch)
    .bind(status)
    .bind(lossy)
    .bind(record)
    .execute(&app.db)
    .await
    .expect("record the ended incarnation");
}

/// One chat turn of the workspace in the state its interruption left it.
async fn agent_turn(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner_id: Uuid,
    status: &str,
    continuation_reason: Option<&str>,
    hold_reason: Option<&str>,
    interrupt_reason: Option<&str>,
) -> Uuid {
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions (id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1, $2, $3, 'workspace', 'Automatic continuation test', $4)",
    )
    .bind(session_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("insert chat session");
    let mut spec = json!({
        "agent": {
            "schemaVersion": "g5.agent-task-spec/v1",
            "agentLaunch": {"schemaVersion": 1}
        }
    });
    if let Some(reason) = interrupt_reason {
        spec["lifecycle"] = json!({
            "interrupt_operation_id": Uuid::new_v4(),
            "interrupt_reason": reason,
            "interrupt_requested_at": chrono::Utc::now(),
        });
    }
    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks (
             id, org_id, workspace_id, name, kind, status, spec, attempt,
             chat_session_id, continuation_reason, hold_reason, created_by
         ) VALUES ($1, $2, $3, 'Automatic continuation test', 'agent', $4, $5, 1, $6, $7, $8, $9)",
    )
    .bind(task_id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(status)
    .bind(spec)
    .bind(session_id)
    .bind(continuation_reason)
    .bind(hold_reason)
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("insert agent turn");
    task_id
}

/// The settled attempt a lost incarnation `epoch` left behind for `task`.
async fn lost_attempt(app: &TestApp, org_id: Uuid, task: Uuid, epoch: i64) {
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed,
             started_at, ended_at, reason, outcome)
         VALUES ($1,$2,1,$3,$4,true,now() - interval '1 minute',now(),'vm_lost',
                 '{\"status\":\"vm_lost\"}'::jsonb)",
    )
    .bind(org_id)
    .bind(task)
    .bind(Uuid::new_v4().to_string())
    .bind(epoch)
    .execute(&app.db)
    .await
    .expect("insert the lost attempt");
}

async fn vm_row(app: &TestApp, workspace_id: Uuid) -> (Option<Value>, Option<Value>) {
    sqlx::query_as::<_, (Option<Value>, Option<Value>)>(
        "SELECT start_intent, automatic_continuation FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("read the VM row")
}

async fn intent_wait_minutes(app: &TestApp, workspace_id: Uuid) -> f64 {
    sqlx::query_scalar::<_, f64>(
        "SELECT (EXTRACT(EPOCH FROM (start_intent ->> 'expires_at')::timestamptz
                             - (start_intent ->> 'created_at')::timestamptz) / 60)::double precision
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("read the consent lifetime")
}

/// A lost VM whose turn settled `vm_lost` gets one automatic start for its
/// owner, with the longer capacity wait, and the incarnation is decided once.
#[tokio::test]
async fn lost_vm_turn_starts_the_workspace_again_once() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    let intent = intent.expect("an automatic start intent");
    assert_eq!(intent["kind"], "automatic_continuation");
    assert_eq!(intent["owner_user_id"], json!(owner.id));
    assert_eq!(
        intent["status"], "queued_for_capacity",
        "no host is reachable, so the consent waits for capacity"
    );
    let wait = intent_wait_minutes(&app, workspace.id).await;
    assert!(
        (59.9..=60.1).contains(&wait),
        "consent waits an hour, got {wait}"
    );
    let record = record.expect("the decision record");
    assert_eq!(record["outcome"], "recorded");
    assert_eq!(record["cause"], "vm_lost");
    assert_eq!(record["epoch"], 1);
    assert_eq!(record["count"], 1);
    assert_eq!(record["intent_id"], intent["id"]);

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0,
        "the ended incarnation is decided once"
    );
    let (again, _) = vm_row(&app, workspace.id).await;
    assert_eq!(again.expect("intent kept")["id"], intent["id"]);

    // The owner learns of it in the interrupted chat and as a live notification.
    let notice: String = sqlx::query_scalar(
        "SELECT message.content FROM chat_messages AS message
         JOIN tasks AS task ON task.chat_session_id = message.session_id
         WHERE task.id = $1 AND message.role = 'system'
         ORDER BY message.message_index DESC LIMIT 1",
    )
    .bind(lost)
    .fetch_one(&app.db)
    .await
    .expect("the interrupted chat carries the notice");
    assert_eq!(
        notice,
        "Interrupted: the workspace VM was lost. G5 started the workspace again and the interrupted chat continues."
    );
    let (title, severity): (String, String) = sqlx::query_as(
        "SELECT title, severity FROM notifications
         WHERE workspace_id = $1 AND recipient_user_id = $2 AND category = 'workspace_lifecycle'",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .expect("the owner's notification");
    assert_eq!(
        (title.as_str(), severity.as_str()),
        ("Workspace started again", "info")
    );
    app.cleanup().await;
}

/// The third loss since the owner's last Start is refused: the turn stays
/// held for their Start, and the refusal is recorded for the incarnation.
#[tokio::test]
async fn loss_cap_holds_the_turn_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(
        &app,
        &workspace,
        "failed",
        3,
        Some(json!({"epoch": 2, "outcome": "recorded", "cause": "vm_lost", "count": 2})),
    )
    .await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 3).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none(), "no consent is recorded past the cap");
    let record = record.expect("the decision record");
    assert_eq!(record["outcome"], "cap_reached");
    assert_eq!(record["epoch"], 3);
    assert_eq!(record["count"], 2);
    let (title, message): (String, String) = sqlx::query_as(
        "SELECT title, message FROM notifications
         WHERE workspace_id = $1 AND recipient_user_id = $2 AND category = 'workspace_lifecycle'",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .expect("the owner's notification");
    assert_eq!(title, "Workspace stays stopped");
    assert!(message.ends_with("Press Start to continue."), "{message}");
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    app.cleanup().await;
}

/// A drain leaves the turn queued with its interrupt reason; the workspace
/// starts again and the drain does not count toward the loss cap.
#[tokio::test]
async fn drain_interrupted_turn_starts_again_without_counting_toward_the_cap() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(
        &app,
        &workspace,
        "stopped",
        4,
        Some(json!({"epoch": 3, "outcome": "recorded", "cause": "vm_lost", "count": 2})),
    )
    .await;
    agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    // An attempt an earlier incarnation lost is history, not this drain's cause.
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed,
             started_at, ended_at, reason, outcome)
         SELECT $1, id, 1, $2, 2, true, now() - interval '1 day', now() - interval '1 day',
                'vm_lost', '{\"status\":\"vm_lost\"}'::jsonb
         FROM tasks WHERE workspace_id = $3",
    )
    .bind(org.id)
    .bind(Uuid::new_v4().to_string())
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .unwrap();
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("an automatic start intent")["kind"],
        "automatic_continuation"
    );
    let record = record.expect("the decision record");
    assert_eq!(record["outcome"], "recorded");
    assert_eq!(record["cause"], "host_stop");
    assert_eq!(record["epoch"], 4);
    assert_eq!(record["count"], 2, "a drain leaves the loss count alone");
    app.cleanup().await;
}

/// An automatic start whose consent ended without the VM running (the boot
/// failed, or it expired or was cancelled while waiting for capacity) ends
/// the continuation: the turn waits for the owner, and a later epoch from a
/// refused placement does not renew the hour.
#[tokio::test]
async fn consent_that_ended_without_running_ends_the_continuation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    for (status, intent_status, epoch, outcome) in [
        ("failed", "failed", 5, "recorded"),
        ("stopped", "expired", 2, "recorded"),
    ] {
        let workspace = TestWorkspace::create(&app, &org).await;
        let intent_id = Uuid::new_v4();
        ended_vm(
            &app,
            &workspace,
            status,
            epoch,
            Some(json!({
                "epoch": epoch - 1, "outcome": outcome, "cause": "host_stop",
                "count": 0, "intent_id": intent_id,
                "at": chrono::Utc::now() - chrono::Duration::hours(2),
            })),
        )
        .await;
        sqlx::query(
            "UPDATE workspace_vms SET started_at = NULL, start_intent = $2 WHERE workspace_id = $1",
        )
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": intent_status,
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now() - chrono::Duration::hours(2),
            "expires_at": chrono::Utc::now() - chrono::Duration::hours(1),
        }))
        .execute(&app.db)
        .await
        .unwrap();
        agent_turn(
            &app,
            &workspace,
            owner.id,
            "queued",
            Some("interrupted"),
            None,
            Some("drain"),
        )
        .await;

        assert_eq!(
            service.reconcile_automatic_continuations(16).await.unwrap(),
            0,
            "{intent_status} consent is not renewed"
        );
        let (intent, record) = vm_row(&app, workspace.id).await;
        assert_eq!(
            intent.expect("the ended intent stays")["id"],
            json!(intent_id)
        );
        let record = record.expect("the decision record");
        assert_eq!(record["outcome"], "ended");
        assert_eq!(record["epoch"], epoch);
        assert_eq!(
            service.reconcile_automatic_continuations(16).await.unwrap(),
            0
        );
    }
    app.cleanup().await;
}

/// A consent Ready consumed ran its VM. When a later drain stops that VM and
/// cancels the record, the next drain-interrupted turn still continues: two
/// drains in a row are two continuations.
#[tokio::test]
async fn a_consent_that_ran_does_not_end_the_lineage_after_a_later_drain() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "stopped",
        3,
        Some(json!({
            "epoch": 2, "outcome": "recorded", "cause": "host_stop",
            "count": 0, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "cancelled",
            "consumed_vm_id": Uuid::new_v4(),
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now() - chrono::Duration::hours(2),
            "expires_at": chrono::Utc::now() - chrono::Duration::hours(1),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    let intent = intent.expect("a new automatic start");
    assert_ne!(intent["id"], json!(intent_id));
    assert_eq!(intent["kind"], "automatic_continuation");
    assert_eq!(record.expect("the decision record")["outcome"], "recorded");
    app.cleanup().await;
}

/// An owner's Start that fails before Ready ends its own consent; the turns
/// queued behind it are not an interruption and are not started again.
#[tokio::test]
async fn a_failed_initial_boot_is_not_continued() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, started_at,
             start_command_sent_at, lease_expires_at
         ) VALUES ($1,$2,$3,$4,1,'starting',NULL,now() - interval '2 minutes',now() - interval '1 second')
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 1, status = 'starting',
             started_at = NULL, start_command_sent_at = EXCLUDED.start_command_sent_at,
             lease_expires_at = EXCLUDED.lease_expires_at",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let task = agent_turn(&app, &workspace, owner.id, "queued", None, None, None).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(service.reconcile_expired_leases().await.unwrap(), 1);
    let marker: Option<String> = sqlx::query_scalar(
        "SELECT spec #>> '{lifecycle,interrupt_reason}' FROM tasks WHERE id = $1",
    )
    .bind(task)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        marker.is_none(),
        "a Start that never reached Ready interrupts nothing"
    );
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert_eq!(record.expect("the decision record")["outcome"], "none");
    app.cleanup().await;
}

/// A turn the drain policy re-queued whose attempt nevertheless settled
/// `vm_lost` (the VM went before quiescence finished) is a loss for the cap.
#[tokio::test]
async fn a_drain_queued_turn_whose_attempt_was_lost_counts_as_a_loss() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let task = agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed,
             started_at, ended_at, reason, outcome)
         VALUES ($1,$2,1,$3,1,true,now() - interval '1 minute',now(),'vm_lost',
                 '{\"status\":\"vm_lost\"}'::jsonb)",
    )
    .bind(org.id)
    .bind(task)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (_, record) = vm_row(&app, workspace.id).await;
    let record = record.expect("the decision record");
    assert_eq!(record["cause"], "vm_lost");
    assert_eq!(record["count"], 1);
    app.cleanup().await;
}

/// An owner's Start after a loss that fails before Ready leaves the old
/// `vm_lost` hold in place. That hold belongs to the earlier incarnation and
/// is not read as a fresh interruption: the failed Start is not revived.
#[tokio::test]
async fn a_hold_from_an_earlier_incarnation_is_not_a_fresh_interruption() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 2, None).await;
    sqlx::query("UPDATE workspace_vms SET started_at = NULL WHERE workspace_id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert_eq!(record.expect("the decision record")["outcome"], "none");
    app.cleanup().await;
}

/// A lease that expires while a drain stop is still saving is a loss: the
/// VM row ends `stopped` with `lease_expired_during_stop`, the drained turns
/// are stamped, and the continuation counts as a loss.
#[tokio::test]
async fn lease_loss_during_an_unfinished_drain_counts_as_a_loss() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, started_at,
             stop_reason, stop_operation_id, stop_generation, stop_deadline_at,
             lease_expires_at
         ) VALUES ($1,$2,$3,$4,2,'stopping',now() - interval '1 hour',
                   'drain',$5,1,now() + interval '5 minutes',now() - interval '1 second')
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 2, status = 'stopping',
             started_at = EXCLUDED.started_at, stop_reason = 'drain',
             stop_operation_id = EXCLUDED.stop_operation_id, stop_generation = 1,
             stop_deadline_at = EXCLUDED.stop_deadline_at,
             lease_expires_at = EXCLUDED.lease_expires_at",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .unwrap();
    let task = agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(service.reconcile_expired_leases().await.unwrap(), 1);
    let (status, reason, marker): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT vm.status, vm.last_error ->> 'reason',
                (SELECT spec #>> '{lifecycle,interrupt_reason}' FROM tasks WHERE id = $2)
         FROM workspace_vms AS vm WHERE vm.workspace_id = $1",
    )
    .bind(workspace.id)
    .bind(task)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "stopped");
    assert_eq!(reason.as_deref(), Some("lease_expired_during_stop"));
    assert_eq!(marker.as_deref(), Some("vm_lost"));

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (_, record) = vm_row(&app, workspace.id).await;
    let record = record.expect("the decision record");
    assert_eq!(record["cause"], "vm_lost");
    assert_eq!(record["count"], 1);
    app.cleanup().await;
}

/// A capacity stop announced to the owner decides the incarnation at once:
/// a VM lost during the warning window is not started again against it.
#[tokio::test]
async fn an_announced_capacity_stop_decides_the_incarnation_before_a_loss() {
    use g5_api::services::workspace_vm_service::StopReason;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, started_at, lease_expires_at
         ) VALUES ($1,$2,$3,$4,2,'running',now() - interval '1 hour',now() + interval '1 hour')
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 2, status = 'running',
             started_at = EXCLUDED.started_at, lease_expires_at = EXCLUDED.lease_expires_at",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let task = agent_turn(&app, &workspace, owner.id, "running", None, None, None).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    service
        .stop_as_system_for_tests(workspace.id, Uuid::new_v4(), StopReason::Capacity, None)
        .await
        .expect("the capacity stop is announced");
    let (_, record) = vm_row(&app, workspace.id).await;
    let record = record.expect("the announced stop decided the incarnation");
    assert_eq!(record["outcome"], "stopped");
    assert_eq!(record["cause"], "capacity");
    assert_eq!(record["epoch"], 2);

    // The VM is lost in the warning window and its attempt settles vm_lost.
    sqlx::query(
        "UPDATE workspace_vms
         SET status = 'failed', host_id = NULL, vm_id = NULL, stop_reason = NULL,
             stop_operation_id = NULL, stop_generation = NULL, stop_deadline_at = NULL,
             stop_retry_at = NULL, nudged_at = NULL,
             pending_stop_reason = NULL, pending_stop_deadline_at = NULL,
             last_error = '{\"reason\":\"lease_expired\"}'::jsonb, unflushed_loss_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tasks SET status = 'waiting', hold_reason = 'vm_lost',
                          continuation_reason = 'vm_lost' WHERE id = $1",
    )
    .bind(task)
    .execute(&app.db)
    .await
    .unwrap();
    lost_attempt(&app, org.id, task, 2).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none(), "the scheduled stop stands");
    app.cleanup().await;
}

/// An automatic consent waiting for capacity is cancelled, not replayed,
/// once the owner has cancelled the last turn it was recorded for.
#[tokio::test]
async fn a_replayed_consent_with_no_turn_left_is_cancelled() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "failed",
        1,
        Some(json!({
            "epoch": 1, "outcome": "recorded", "cause": "vm_lost",
            "count": 1, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(50),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    // The only interrupted turn was cancelled by the owner meanwhile; another
    // turn is held on an input of unknown delivery, which the owner decides.
    agent_turn(&app, &workspace, owner.id, "cancelled", None, None, None).await;
    let blocked = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("delivery_unknown"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, blocked, 1).await;
    sqlx::query(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin, delivery,
             reservation_id, reservation_revision, task_id, attempt
         ) SELECT gen_random_uuid(), $1, chat_session_id, 0, 'unknown', 'human',
                  'delivery_unknown', gen_random_uuid(), 1, id, 1
           FROM tasks WHERE id = $2",
    )
    .bind(org.id)
    .bind(blocked)
    .execute(&app.db)
    .await
    .unwrap();
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    let replay = service
        .resume_queued(
            workspace.id,
            owner.id,
            intent_id,
            StartKind::AutomaticContinuation,
        )
        .await;
    assert!(replay.is_err(), "an empty consent is refused");
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the intent stays recorded")["status"],
        "cancelled"
    );
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0,
        "a cancelled consent with no work is not decided again"
    );
    app.cleanup().await;
}

/// A drain-queued turn whose clarification is still unanswered waits for the
/// answer, like a held one: it does not qualify for an automatic start.
#[tokio::test]
async fn a_queued_turn_with_an_unanswered_clarification_waits_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "stopped", 1, None).await;
    let task = agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    // The drain settled the attempt that asked the question.
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed,
             started_at, ended_at, reason, outcome)
         VALUES ($1,$2,1,$3,1,true,now() - interval '1 minute',now(),'interrupted',
                 '{\"status\":\"interrupted\"}'::jsonb)",
    )
    .bind(org.id)
    .bind(task)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .expect("insert the settled attempt");
    sqlx::query(
        "INSERT INTO provider_requests(
             org_id, task_id, attempt, provider_request_id, kind, payload, display,
             status, resume_session_id, response_deadline_at)
         VALUES ($1,$2,1,'question-1','question','{}','{}','pending',$3,NULL)",
    )
    .bind(org.id)
    .bind(task)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .expect("insert the pending clarification");
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert_eq!(record.expect("the decision record")["outcome"], "none");
    app.cleanup().await;
}

/// A replay of an owner Start that a Stop has already cancelled records
/// nothing and leaves the Stop's decision in place.
#[tokio::test]
async fn a_replayed_cancelled_owner_start_keeps_the_stop_decision() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "failed",
        1,
        Some(json!({"epoch": 1, "outcome": "stopped", "cause": "user", "count": 0})),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "resume", "status": "cancelled",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 0,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(10),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    let replay = service
        .resume_queued(workspace.id, owner.id, intent_id, StartKind::Resume)
        .await;
    assert!(replay.is_err(), "a cancelled consent is not revived");
    let (_, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        record.expect("the Stop's decision stays")["outcome"],
        "stopped"
    );
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    app.cleanup().await;
}

/// A drain re-admitted after the VM already ended neither decides the
/// incarnation nor cancels the automatic continuation it led to.
#[tokio::test]
async fn a_drain_readmitted_after_the_stop_leaves_the_continuation_alone() {
    use g5_api::services::workspace_vm_service::StopReason;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "stopped", 1, None).await;
    agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("interrupted"),
        None,
        Some("drain"),
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    let intent = intent.expect("the automatic start");
    assert_eq!(intent["status"], "queued_for_capacity");

    service
        .stop_as_system_for_tests(workspace.id, Uuid::new_v4(), StopReason::Drain, None)
        .await
        .expect("the late drain is admitted as already stopped");
    let (after, record_after) = vm_row(&app, workspace.id).await;
    assert_eq!(
        after.expect("the consent stays")["status"],
        "queued_for_capacity"
    );
    assert_eq!(record_after, record, "the drain decides nothing");
    app.cleanup().await;
}

/// After a loss the owner pressed Start themselves and that consent lapsed
/// waiting for capacity. Their decision stands: the platform does not renew
/// it with an hour of its own.
#[tokio::test]
async fn a_lapsed_owner_start_after_a_loss_is_not_renewed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": Uuid::new_v4(), "kind": "resume", "status": "expired",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 0,
            "created_at": chrono::Utc::now() - chrono::Duration::minutes(30),
            "expires_at": chrono::Utc::now() - chrono::Duration::minutes(15),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the owner's intent stays")["status"],
        "expired"
    );
    let record = record.expect("the decision record");
    assert_eq!(record["outcome"], "ended");
    assert_eq!(record["cause"], "consent_lapsed");
    app.cleanup().await;
}

/// A Resume that only attaches to compute already running records no consent
/// and leaves a scheduled stop's decision in place.
#[tokio::test]
async fn attaching_to_running_compute_keeps_a_scheduled_stop_decision() {
    use g5_api::services::workspace_vm_service::StopReason;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let consumed = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, started_at, lease_expires_at,
             start_intent
         ) VALUES ($1,$2,$3,$4,2,'running',now() - interval '1 hour',now() + interval '1 hour',$5)
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 2, status = 'running',
             started_at = EXCLUDED.started_at, lease_expires_at = EXCLUDED.lease_expires_at,
             start_intent = EXCLUDED.start_intent",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .bind(json!({
        "id": consumed, "kind": "resume", "status": "consumed",
        "consumed_vm_id": Uuid::new_v4(),
        "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
        "created_at": chrono::Utc::now() - chrono::Duration::hours(1),
        "expires_at": chrono::Utc::now() - chrono::Duration::minutes(45),
    }))
    .execute(&app.db)
    .await
    .unwrap();
    agent_turn(&app, &workspace, owner.id, "running", None, None, None).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;
    service
        .stop_as_system_for_tests(workspace.id, Uuid::new_v4(), StopReason::Capacity, None)
        .await
        .expect("the capacity stop is announced");
    let (_, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        record.as_ref().expect("the announced stop decided")["outcome"],
        "stopped"
    );

    service
        .resume(workspace.id, owner.id, Uuid::new_v4(), StartKind::Resume)
        .await
        .expect("Resume attaches to the running VM");
    let (intent, after) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consumed intent stays")["id"],
        json!(consumed)
    );
    assert_eq!(after, record, "attaching decides nothing");
    app.cleanup().await;
}

/// The owner's Continue re-queues a held turn and strips its marker; the
/// lost attempt still counts, so the waiting consent is replayed, not
/// cancelled.
#[tokio::test]
async fn a_replayed_consent_keeps_a_turn_continue_requeued() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "failed",
        1,
        Some(json!({
            "epoch": 1, "outcome": "recorded", "cause": "vm_lost",
            "count": 1, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(50),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let requeued = agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        Some("vm_lost"),
        None,
        None,
    )
    .await;
    lost_attempt(&app, org.id, requeued, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    service
        .resume_queued(
            workspace.id,
            owner.id,
            intent_id,
            StartKind::AutomaticContinuation,
        )
        .await
        .expect("the consent is replayed");
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consent stays")["status"],
        "queued_for_capacity"
    );
    app.cleanup().await;
}

/// Cancelling the last interrupted turn cancels the automatic consent still
/// waiting for capacity, in the same transaction.
#[tokio::test]
async fn cancelling_the_last_interrupted_turn_cancels_the_waiting_consent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "failed",
        1,
        Some(json!({
            "epoch": 1, "outcome": "recorded", "cause": "vm_lost",
            "count": 1, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(50),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;

    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let cancelled = app
        .state
        .task_service
        .request_workspace_task_cancel(&mut db, owner.id, lost)
        .await
        .expect("the owner cancels the held turn");
    db.commit().await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consent stays recorded")["status"],
        "cancelled"
    );
    app.cleanup().await;
}

/// Withdrawing the only input of a turn a drain left queued before its first
/// dispatch ends that turn and, with it, the automatic consent waiting for
/// capacity.
#[tokio::test]
async fn withdrawing_the_last_queued_input_cancels_the_waiting_consent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "stopped",
        1,
        Some(json!({
            "epoch": 1, "outcome": "recorded", "cause": "host_stop",
            "count": 0, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(50),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let task = agent_turn(
        &app,
        &workspace,
        owner.id,
        "queued",
        None,
        None,
        Some("drain"),
    )
    .await;
    let (session_id, input_id): (Uuid, Uuid) = sqlx::query_as(
        "INSERT INTO session_inputs(
             id, org_id, session_id, input_index, content, input_origin, delivery, task_id
         ) SELECT gen_random_uuid(), $1, chat_session_id, 0, 'queued before the drain', 'human',
                  'queued', id
           FROM tasks WHERE id = $2
         RETURNING session_id, id",
    )
    .bind(org.id)
    .bind(task)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let withdraw = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/sessions/{session_id}/inputs/{input_id}/withdraw"),
            &json!({"expectedRevision": 1}),
        )
        .await;
    assert_eq!(withdraw.status(), reqwest::StatusCode::OK);
    let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(task)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "cancelled", "the turn left without input ends");
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consent stays recorded")["status"],
        "cancelled"
    );
    app.cleanup().await;
}

/// The chat's Stop on a held turn ends it and, with it, the automatic
/// consent waiting for capacity.
#[tokio::test]
async fn chat_stop_on_the_last_held_turn_cancels_the_waiting_consent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let intent_id = Uuid::new_v4();
    ended_vm(
        &app,
        &workspace,
        "failed",
        1,
        Some(json!({
            "epoch": 1, "outcome": "recorded", "cause": "vm_lost",
            "count": 1, "intent_id": intent_id,
        })),
    )
    .await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(workspace.id)
        .bind(json!({
            "id": intent_id, "kind": "automatic_continuation", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
            "created_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::minutes(50),
        }))
        .execute(&app.db)
        .await
        .unwrap();
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    // The turn's own delivered message, which Stop names.
    let (session_id, message_id): (Uuid, Uuid) = sqlx::query_as(
        "WITH input AS (
             INSERT INTO session_inputs(
                 id, org_id, session_id, input_index, content, input_origin, delivery,
                 reservation_id, reservation_revision, delivery_generation, task_id, attempt,
                 delivered_at, delivery_audit
             ) SELECT gen_random_uuid(), $1, chat_session_id, 0, 'finish the report', 'human',
                      'delivered', gen_random_uuid(), 1, 1, id, 1, now(),
                      '[{\"event\":\"input.accepted\"}]'::jsonb
               FROM tasks WHERE id = $2
             RETURNING id, session_id
         )
         INSERT INTO chat_messages(
             id, org_id, session_id, role, content, status, message_index, input_id, task_id, attempt
         ) SELECT id, $1, session_id, 'user', 'finish the report', 'complete', 0, id, $2, 1
           FROM input
         RETURNING session_id, id",
    )
    .bind(org.id)
    .bind(lost)
    .fetch_one(&app.db)
    .await
    .expect("insert the turn's message");

    let stopped = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/sessions/{session_id}/messages/{message_id}/stop"),
            &json!({}),
        )
        .await;
    let status = stopped.status();
    let body: Value = stopped.json().await.expect("stop response");
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
        .bind(lost)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(task_status, "cancelled", "{body}");
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consent stays recorded")["status"],
        "cancelled"
    );
    app.cleanup().await;
}

/// A turn the owner's Stop was interrupting when the VM was lost keeps their
/// decision: its marker names the Stop, so it is not continued.
#[tokio::test]
async fn a_turn_the_owner_was_stopping_when_the_vm_was_lost_waits_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        Some("user"),
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert_eq!(record.expect("the decision record")["outcome"], "none");
    app.cleanup().await;
}

/// Replaying the owner's original Start while the VM is still starting
/// records nothing new, so a stop announced since keeps its decision.
#[tokio::test]
async fn replaying_a_starting_owner_start_keeps_an_announced_stop_decision() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    let intent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at,
             start_deadline_at, start_intent, automatic_continuation
         ) VALUES ($1,$2,$3,$4,2,'starting',now() + interval '1 hour',now() + interval '2 minutes',
                   $5,$6)
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 2, status = 'starting',
             lease_expires_at = EXCLUDED.lease_expires_at,
             start_deadline_at = EXCLUDED.start_deadline_at,
             start_intent = EXCLUDED.start_intent,
             automatic_continuation = EXCLUDED.automatic_continuation",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .bind(json!({
        "id": intent_id, "kind": "resume", "status": "starting",
        "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 1,
        "created_at": chrono::Utc::now() - chrono::Duration::minutes(1),
        "expires_at": chrono::Utc::now() + chrono::Duration::minutes(14),
    }))
    .bind(json!({"epoch": 2, "outcome": "stopped", "cause": "capacity", "count": 0}))
    .execute(&app.db)
    .await
    .unwrap();
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    service
        .resume_queued(workspace.id, owner.id, intent_id, StartKind::Resume)
        .await
        .expect("the replay observes the start under way");
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(intent.expect("the intent stays")["id"], json!(intent_id));
    assert_eq!(
        record.expect("the announced stop's decision stays")["outcome"],
        "stopped"
    );
    app.cleanup().await;
}

/// An organization that turned automatic continuation off leaves an
/// interrupted workspace to its owner's Start; nothing is decided, so turning
/// it on again still continues the turn.
#[tokio::test]
async fn an_organization_can_turn_automatic_continuation_off() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let client = app.authenticated_client(&owner);
    let off = client
        .put_json(
            &format!("/api/v1/orgs/{}/admin-settings", org.id),
            &json!({"automaticContinuationEnabled": false}),
        )
        .await;
    assert_eq!(off.status(), reqwest::StatusCode::OK);
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert!(
        record.is_none(),
        "nothing is decided while the setting is off"
    );

    let on = client
        .put_json(
            &format!("/api/v1/orgs/{}/admin-settings", org.id),
            &json!({"automaticContinuationEnabled": true}),
        )
        .await;
    assert_eq!(on.status(), reqwest::StatusCode::OK);
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    app.cleanup().await;
}

/// Placement's owner limit refuses the start after its consent was recorded,
/// ending that consent in place: the decision becomes a refusal and the owner
/// is told the workspace stays stopped, not that it started.
#[tokio::test]
async fn an_owner_limit_refusal_after_consent_is_recorded_as_refused() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let limited = app
        .authenticated_client(&owner)
        .put_json(
            &format!("/api/v1/orgs/{}/admin-settings", org.id),
            &json!({"maxRunningVmsPerOwner": 0}),
        )
        .await;
    assert_eq!(limited.status(), reqwest::StatusCode::OK);
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("the consent placement ended")["status"],
        "failed"
    );
    let record = record.expect("the decision record");
    assert_eq!(record["outcome"], "refused:owner_limit");
    assert_eq!(record["epoch"], 1);
    let (title, message): (String, String) = sqlx::query_as(
        "SELECT title, message FROM notifications
         WHERE workspace_id = $1 AND recipient_user_id = $2 AND category = 'workspace_lifecycle'",
    )
    .bind(workspace.id)
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .expect("the owner's notification");
    assert_eq!(title, "Workspace stays stopped");
    assert!(
        message.contains("as many workspace VMs as the organization allows"),
        "{message}"
    );
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0,
        "the refusal decides the incarnation"
    );
    app.cleanup().await;
}

/// The owner's Stop of a stopped workspace decides its incarnation: the
/// platform does not start it again for the turns the loss left behind.
#[tokio::test]
async fn owner_stop_of_a_stopped_workspace_wins_over_automatic_continuation() {
    use g5_api::services::workspace_vm_service::StopReason;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let lost = agent_turn(
        &app,
        &workspace,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("vm_lost"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 1).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    service
        .stop_as_system_for_tests(workspace.id, Uuid::new_v4(), StopReason::User, None)
        .await
        .expect("a Stop of a stopped workspace is admitted");
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    let record = record.expect("the Stop decided the incarnation");
    assert_eq!(record["outcome"], "stopped");
    assert_eq!(record["cause"], "user");
    assert_eq!(record["epoch"], 1);

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, _) = vm_row(&app, workspace.id).await;
    assert!(
        intent.is_none(),
        "the held turn waits for the owner's Start"
    );
    app.cleanup().await;
}

/// A turn still queued when its VM is lost, as after a restarted VM is lost
/// before dispatch, is stamped at the release and continued like a held one.
#[tokio::test]
async fn queued_turn_lost_with_its_vm_is_continued() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat, slot_size_reported)
         VALUES ($1,$2,'us-central1','ready',now(),true)",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms(
             org_id, workspace_id, host_id, vm_id, epoch, status, started_at, lease_expires_at
         ) VALUES ($1,$2,$3,$4,3,'running',now() - interval '1 minute',now() - interval '1 second')
         ON CONFLICT (workspace_id) DO UPDATE
         SET host_id = EXCLUDED.host_id, vm_id = EXCLUDED.vm_id, epoch = 3, status = 'running',
             started_at = EXCLUDED.started_at, lease_expires_at = EXCLUDED.lease_expires_at",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    let task = agent_turn(&app, &workspace, owner.id, "queued", None, None, None).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(service.reconcile_expired_leases().await.unwrap(), 1);
    let marker: Option<String> = sqlx::query_scalar(
        "SELECT spec #>> '{lifecycle,interrupt_reason}' FROM tasks WHERE id = $1",
    )
    .bind(task)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        marker.as_deref(),
        Some("vm_lost"),
        "the queued turn is stamped at the loss"
    );

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(
        intent.expect("an automatic start")["kind"],
        "automatic_continuation"
    );
    let record = record.expect("the decision record");
    assert_eq!(record["cause"], "vm_lost");
    assert_eq!(record["epoch"], 3);
    assert_eq!(record["count"], 1);
    app.cleanup().await;
}

/// Work held on the owner's decision is not a platform interruption: their
/// Stop, an input of unknown delivery after a loss, or no chat turn at all.
#[tokio::test]
async fn owner_decided_holds_are_left_for_the_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let stopped = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &stopped, "stopped", 1, None).await;
    agent_turn(
        &app,
        &stopped,
        owner.id,
        "waiting",
        Some("interrupted"),
        Some("owner_stop"),
        Some("user"),
    )
    .await;
    let unknown = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &unknown, "failed", 2, None).await;
    let lost = agent_turn(
        &app,
        &unknown,
        owner.id,
        "waiting",
        Some("vm_lost"),
        Some("delivery_unknown"),
        None,
    )
    .await;
    lost_attempt(&app, org.id, lost, 2).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    for (workspace, epoch) in [(&stopped, 1), (&unknown, 2)] {
        let (intent, record) = vm_row(&app, workspace.id).await;
        assert!(intent.is_none(), "no start for {}", workspace.name);
        let record = record.expect("the decision record");
        assert_eq!(record["outcome"], "none");
        assert_eq!(record["epoch"], epoch);
    }
    app.cleanup().await;
}

/// A lost VM whose attempt has not settled yet is not decided: the loss
/// settlement comes first, and the pass reads the workspace again later.
#[tokio::test]
async fn unsettled_loss_waits_for_the_attempt_to_settle() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(&app, &workspace, "failed", 1, None).await;
    let task = agent_turn(&app, &workspace, owner.id, "running", None, None, None).await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        0
    );
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert!(intent.is_none());
    assert!(record.is_none(), "an unsettled incarnation stays undecided");

    sqlx::query(
        "UPDATE tasks SET status = 'waiting', hold_reason = 'vm_lost',
                          continuation_reason = 'vm_lost' WHERE id = $1",
    )
    .bind(task)
    .execute(&app.db)
    .await
    .unwrap();
    lost_attempt(&app, org.id, task, 1).await;
    assert_eq!(
        service.reconcile_automatic_continuations(16).await.unwrap(),
        1
    );
    app.cleanup().await;
}

/// The owner's own Start ends the platform's continuation lineage: the loss
/// count starts over.
#[tokio::test]
async fn owner_start_resets_the_loss_count() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    ended_vm(
        &app,
        &workspace,
        "failed",
        2,
        Some(json!({"epoch": 2, "outcome": "cap_reached", "cause": "vm_lost", "count": 2})),
    )
    .await;
    let service = workspace_vm_service(&app, owner.id, org.id).await;

    service
        .resume(workspace.id, owner.id, Uuid::new_v4(), StartKind::Resume)
        .await
        .expect("the owner's Start is recorded");
    let (intent, record) = vm_row(&app, workspace.id).await;
    assert_eq!(intent.expect("the owner's intent")["kind"], "resume");
    assert!(
        record.is_none(),
        "an owner Start clears the continuation record"
    );
    app.cleanup().await;
}
