//! Real API/database contract for agent approval history and evaluation.
//!
//! Scenario ownership: `test_contract/agent_approval_review` D5. Each test
//! uses the real workspace-approval route and an isolated PostgreSQL database.
//! Durable status fixtures are inserted directly as terminal-writer output;
//! no evaluator dependency is mocked or stubbed.

use crate::helpers;
use crate::helpers::review_flow;

async fn human_review_fixture(
    app: &TestApp,
) -> (TestUser, TestOrg, TestWorkspace, TestWorkspace, TestUser) {
    let f = review_flow::Flow::new(app).await;
    f.submit(app).await;
    set_merge_target(app, f.source, f.parent.id).await;
    let heads = f.overview(app, &f.owner).await;
    let h = heads["heads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    let source = TestWorkspace {
        id: f.source,
        org_id: f.org.id,
        repo_id: f.parent.repo_id,
        head_commit: h["headCommit"].as_str().unwrap().to_owned(),
        name: h["branchName"].as_str().unwrap().to_owned(),
    };
    g5_scm::GitRepo::open(&app.repo_path(source.repo_id).await)
        .unwrap()
        .checkout_branch_force(&source.name)
        .unwrap();
    (f.owner, f.org, f.parent, source, f.reviewer)
}

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn approvals_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals")
}

fn requirements_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requirements")
}

fn requests_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/approvals/requests")
}

async fn set_merge_target(app: &TestApp, source: Uuid, target: Uuid) {
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target)
        .bind(source)
        .execute(&app.db)
        .await
        .expect("set merge target");
}

async fn create_definition(
    app: &TestApp,
    owner: &TestUser,
    org_id: Uuid,
    name: &str,
    human_user_ids: &[Uuid],
    agent: bool,
    require_all_reviewers: bool,
) -> Uuid {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({
                "name": name,
                "requireAllReviewers": require_all_reviewers,
                "humanUserIds": human_user_ids,
                "agentType": agent.then_some("simple"),
                "agentPrompt": agent.then_some("Review the exact committed cut."),
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("definition response");
    Uuid::parse_str(body["data"]["id"].as_str().expect("definition id")).expect("definition UUID")
}

async fn attach_definition(
    app: &TestApp,
    owner: &TestUser,
    target_workspace_id: Uuid,
    definition_id: Uuid,
) {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &requirements_path(target_workspace_id),
            &json!({ "definitionId": definition_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_human_request(
    app: &TestApp,
    owner: &TestUser,
    workspace_id: Uuid,
    definition_id: Uuid,
) -> Uuid {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &requests_path(workspace_id),
            &json!({ "definitionId": definition_id, "definitionVersion": 1, "phase": "ie" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("request response");
    Uuid::parse_str(
        body["data"]["requests"][0]["requestId"]
            .as_str()
            .expect("request id"),
    )
    .expect("request UUID")
}

async fn current_heads(app: &TestApp, workspace_id: Uuid) -> Value {
    let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT m.org_id, m.repo_id, CASE WHEN w.is_base THEN r.default_branch ELSE w.name END FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id JOIN repos r ON r.id=m.repo_id WHERE w.id=$1 ORDER BY m.repo_id")
        .bind(workspace_id).fetch_all(&app.db).await.unwrap();
    Value::Array(rows.into_iter().map(|(org_id, repo_id, branch_name)| {
        let path = app.repo_storage.test_path(org_id, repo_id);
        let head = git2::Repository::open(path).unwrap().refname_to_id(&format!("refs/heads/{branch_name}")).unwrap();
        json!({ "repoId":repo_id, "workspaceId":workspace_id, "branchName":branch_name, "headCommit":head.to_string() })
    }).collect())
}

async fn state(app: &TestApp, user: &TestUser, workspace_id: Uuid) -> Value {
    let response = app
        .authenticated_client(user)
        .get(&approvals_path(workspace_id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.expect("approval state")
}

fn requirement(body: &Value, definition_id: Uuid) -> &Value {
    body["data"]["outgoing"]["requirements"]
        .as_array()
        .expect("requirements")
        .iter()
        .find(|entry| entry["definition"]["id"] == definition_id.to_string())
        .expect("definition requirement")
}

/// One requirement's state in the response's merge admission.
fn admission_state(body: &Value, definition_id: Uuid) -> &str {
    body["data"]["outgoing"]["mergeAdmission"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["definitionId"] == definition_id.to_string())
        .unwrap()["state"]
        .as_str()
        .unwrap()
}

fn request(requirement: &Value, request_id: Uuid) -> &Value {
    requirement["requests"]
        .as_array()
        .expect("approval requests")
        .iter()
        .find(|entry| entry["requestId"] == request_id.to_string())
        .expect("approval request")
}

/// Under `on_source_commit_vector_change = IGNORE` a counted approval keeps
/// counting after one repository of the multi-repository vector moves; the
/// move is reported only as commit currentness
/// (g5.governance.approval.source-commit-policy).
#[tokio::test]
async fn human_history_shape_is_preserved_and_source_moves_keep_approval() {
    let app = TestApp::spawn().await;
    let (owner, org, target, mut source, reviewer) = human_review_fixture(&app).await;
    let definition_id = create_definition(
        &app,
        &owner,
        org.id,
        "Human only",
        &[reviewer.id],
        false,
        false,
    )
    .await;
    attach_definition(&app, &owner, target.id, definition_id).await;
    let request_id = create_human_request(&app, &owner, source.id, definition_id).await;
    let reviewed = current_heads(&app, source.id).await;
    assert!(
        reviewed.as_array().unwrap().len() > 1,
        "multi-repository vector"
    );

    let response = app
        .authenticated_client(&reviewer)
        .post_json(
            &format!("{}/decisions", approvals_path(source.id)),
            &json!({
                "definitionId": definition_id,
                "requestId": request_id,
                "reviewedGitHeads": reviewed,
                "outcome": "approved",
                "phase": "ie"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("decision response");
    let req = requirement(&body, definition_id);
    assert_eq!(req["state"], "approved");
    assert_eq!(req["currentness"]["state"], "matching");
    assert_eq!(request(req, request_id)["state"], "settled");
    let entry = &req["decisionLog"][0];
    assert_eq!(entry["actorType"], "human");
    assert_eq!(entry["actor"]["userId"], reviewer.id.to_string());
    assert_eq!(entry["agentJobId"], Value::Null);
    assert!(entry.get("agentApprovalRunDetails").is_none());

    // Only one repository moves.
    source
        .commit_file(
            &app,
            "approval-state.txt",
            "advanced source
",
        )
        .await;
    let moved = current_heads(&app, source.id).await;
    let unchanged = moved
        .as_array()
        .unwrap()
        .iter()
        .filter(|head| reviewed.as_array().unwrap().contains(head))
        .count();
    assert_eq!(unchanged, moved.as_array().unwrap().len() - 1);
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(
        req["state"], "approved",
        "a source move alone keeps approval"
    );
    assert_eq!(req["approvalPath"], Value::Null);
    assert_eq!(req["currentness"]["state"], "changed");
    assert_eq!(
        req["currentness"]["notCurrent"][0]["userId"],
        reviewer.id.to_string()
    );
    assert_eq!(
        req["callerCanDecide"], false,
        "a later push does not create a new review request"
    );
    assert_eq!(req["decisionLog"][0]["superseded"], false);
    assert_eq!(admission_state(&body, definition_id), "approved");
    assert_eq!(
        body["data"]["outgoing"]["requestPolicy"]["onSourceCommitVectorChange"],
        "IGNORE"
    );

    // The locked merge gate agrees on the moved vector.
    let pinned: Vec<g5_contracts::types::MergeSourceHead> = serde_json::from_value(moved).unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    g5_api::services::approval_config_service::lock_merge_admission(&mut db, org.id, target.id)
        .await
        .unwrap();
    let gate = g5_api::services::approval_config_service::evaluate_merge_admission(
        &mut db, org.id, source.id, target.id, &pinned,
    )
    .await
    .unwrap();
    let gate_state = |gate: &g5_api::services::approval_config_service::MergeAdmissionEvidence| {
        gate.requirements
            .iter()
            .find(|r| r.definition_id == definition_id)
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(gate_state(&gate), "approved");
    // MAKE_PENDING remains a supported policy: the same evidence is pending.
    let gate = g5_api::services::approval_config_service::evaluate_merge_admission_with_policy(
        &mut db,
        org.id,
        source.id,
        target.id,
        &pinned,
        g5_contracts::types::SourceCommitVectorPolicy::MakePending,
    )
    .await
    .unwrap();
    assert_eq!(gate_state(&gate), "pending");
    assert!(!gate.admitted);
    assert_eq!(
        gate.reason.as_deref(),
        Some("Approval(s) required before merging")
    );
    db.rollback().await.unwrap();

    app.cleanup().await;
}

/// One requirement's projected state under the other `PR_REQUIRED` value.
pub(crate) async fn state_with_pr_required(
    app: &TestApp,
    org_id: Uuid,
    source_id: Uuid,
    caller: Uuid,
    definition_id: Uuid,
    pr_required: bool,
) -> String {
    let service = g5_api::services::approval_config_service::ApprovalConfigService::new(
        app.repo_storage.clone(),
        g5_contracts::types::ApprovalPolicy {
            pr_required,
            ..Default::default()
        },
    );
    let mut db = app.scoped_db_for_org(org_id).await;
    let state = service
        .workspace_state(
            &mut db,
            &app.state.system_pool,
            &app.state.approval_definition_service,
            org_id,
            source_id,
            caller,
        )
        .await
        .unwrap();
    db.rollback().await.unwrap();
    state
        .outgoing
        .unwrap()
        .requirements
        .into_iter()
        .find(|r| r.definition.id == definition_id)
        .unwrap()
        .state
}

/// The evaluator's path to approval and request needs for an overridable
/// agent rejection, then a human approval that supersedes it. The settled
/// verdict survives the PR closing or merging under either `PR_REQUIRED`
/// value, and a flag change alone never alters it
/// (g5.governance.approval.request-pr).
#[tokio::test]
async fn agent_rejection_path_and_needs_then_human_supersedes() {
    for (pr_required, ending) in [
        (false, "closed"),
        (false, "merged"),
        (true, "closed"),
        (true, "merged"),
    ] {
        agent_rejection_then_human_supersedes_across_pr_end(pr_required, ending).await;
    }
}

async fn agent_rejection_then_human_supersedes_across_pr_end(pr_required: bool, ending: &str) {
    let mut config = helpers::test_config();
    config.approval_policy.pr_required = pr_required;
    let app = TestApp::spawn_with_config(config).await;
    let (owner, org, target, source, reviewer) = human_review_fixture(&app).await;
    let definition_id = create_definition(
        &app,
        &owner,
        org.id,
        "UI changes",
        &[reviewer.id],
        true,
        false,
    )
    .await;
    attach_definition(&app, &owner, target.id, definition_id).await;
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(req["state"], "pending");
    assert_eq!(req["approvalPath"], Value::Null, "untouched");
    assert_eq!(req["requestNeeds"]["outstanding"]["agent"], true);

    crate::api_formal_agent_approvals::settle_for_quorum(
        &app,
        &owner,
        &source,
        definition_id,
        "rejected",
    )
    .await;
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(req["state"], "rejected");
    assert_eq!(req["approvalPath"]["kind"], "agentRejected");
    assert_eq!(req["approvalPath"]["humanOverride"], true);
    assert_eq!(req["approvalPath"]["agentName"], "UI changes agent");
    assert_eq!(req["agentEscalation"]["state"], "needsHumanEscalation");
    assert_eq!(req["requestNeeds"]["outstanding"]["agent"], false);
    assert_eq!(
        req["requestNeeds"]["outstanding"]["randomHumanFrom"],
        json!([reviewer.id.to_string()])
    );

    let request_id = create_human_request(&app, &owner, source.id, definition_id).await;
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(req["state"], "rejected");
    assert_eq!(
        req["requestNeeds"]["outstanding"]["randomHumanFrom"],
        json!([]),
        "a waiting human request covers the requirement"
    );
    assert_eq!(
        req["agentEscalation"]["state"], "needsHumanEscalation",
        "a waiting human request does not settle the escalation"
    );

    let response = app
        .authenticated_client(&reviewer)
        .post_json(
            &format!("{}/decisions", approvals_path(source.id)),
            &json!({"definitionId": definition_id, "requestId": request_id,
                "reviewedGitHeads": current_heads(&app, source.id).await,
                "phase": "ie", "outcome": "approved"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(req["state"], "approved", "the human supersedes the agent");
    assert_eq!(req["approvalPath"], Value::Null);
    assert_eq!(req["agentEscalation"]["state"], "overriddenByHuman");
    assert!(req["decisionLog"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["actorType"] == "agent" && d["outcome"] == "rejected"));
    assert_eq!(admission_state(&body, definition_id), "approved");

    // The PR-bound PR closes or merges; the settled decisions keep their effect.
    let pr_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM workspace_pull_requests WHERE source_workspace_id=$1 AND status='open'",
    )
    .bind(source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let human_pr: Option<Uuid> = sqlx::query_scalar(
        "SELECT pull_request_id FROM approval_statuses WHERE source_workspace_id=$1 AND actor_type='human'",
    )
    .bind(source.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(human_pr, Some(pr_id), "the human decision is PR-bound");
    if ending == "closed" {
        let response = app
            .authenticated_client(&owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{pr_id}/close",
                    source.id
                ),
                &json!({}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    } else {
        sqlx::query("UPDATE workspace_pull_requests SET status='merged' WHERE id=$1")
            .bind(pr_id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    let body = state(&app, &owner, source.id).await;
    let req = requirement(&body, definition_id);
    assert_eq!(
        req["state"], "approved",
        "{ending} PR, PR_REQUIRED={pr_required}"
    );
    assert_eq!(req["approvalPath"], Value::Null);
    let log = req["decisionLog"].as_array().unwrap();
    assert!(log
        .iter()
        .any(|d| d["actorType"] == "agent" && d["outcome"] == "rejected"));
    assert!(log
        .iter()
        .any(|d| d["actorType"] == "human" && d["outcome"] == "approved"));
    assert_eq!(admission_state(&body, definition_id), "approved");
    let pinned: Vec<g5_contracts::types::MergeSourceHead> =
        serde_json::from_value(current_heads(&app, source.id).await).unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    g5_api::services::approval_config_service::lock_merge_admission(&mut db, org.id, target.id)
        .await
        .unwrap();
    let gate = g5_api::services::approval_config_service::evaluate_merge_admission(
        &mut db, org.id, source.id, target.id, &pinned,
    )
    .await
    .unwrap();
    db.rollback().await.unwrap();
    assert_eq!(
        gate.requirements
            .iter()
            .find(|r| r.definition_id == definition_id)
            .unwrap()
            .state,
        "approved"
    );
    assert_eq!(
        state_with_pr_required(
            &app,
            org.id,
            source.id,
            owner.id,
            definition_id,
            !pr_required
        )
        .await,
        "approved",
        "a flag change alone keeps the settled verdict"
    );
    app.cleanup().await;
}

/// Real addressed decisions and persisted agent output must produce the same
/// combined quorum on the outgoing page and the pinned merge admission.
#[tokio::test]
async fn combined_quorum_human_veto_override_and_all_agree_with_merge_admission() {
    let app = TestApp::spawn().await;
    let (owner, org, target, source, reviewer) = human_review_fixture(&app).await;
    let heads = current_heads(&app, source.id).await;
    let pinned_heads: Vec<g5_contracts::types::MergeSourceHead> =
        serde_json::from_value(heads.clone()).unwrap();
    let cases = [
        (false, true, None, Some("approved"), "approved"),
        (true, true, None, Some("approved"), "pending"),
        (false, true, Some("approved"), None, "approved"),
        (false, true, Some("approved"), Some("rejected"), "approved"),
        (false, false, Some("approved"), Some("rejected"), "rejected"),
        (false, true, Some("rejected"), Some("approved"), "rejected"),
        (false, true, Some("rejected"), Some("rejected"), "rejected"),
        (true, true, Some("approved"), Some("rejected"), "rejected"),
        (true, true, Some("approved"), Some("approved"), "approved"),
        (false, true, None, None, "pending"),
        (false, true, None, Some("rejected"), "rejected"),
        (false, true, None, Some("deferred"), "pending"),
        (false, true, Some("approved"), Some("deferred"), "approved"),
    ];
    for (index, (all, override_agent, human, agent, expected)) in cases.into_iter().enumerate() {
        let response = app
            .authenticated_client(&owner)
            .post_json(
                &format!("/api/v1/orgs/{}/approval-definitions", org.id),
                &json!({
                    "name": format!("Combined {index}"),
                    "humanUserIds": [reviewer.id],
                    "agentType": "simple", "agentPrompt": "Review this exact cut.",
                    "requireAllReviewers": all,
                    "humanApprovalsOverrideAgent": override_agent,
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let created: Value = response.json().await.unwrap();
        let definition_id = Uuid::parse_str(created["data"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(
            created["data"]["humanApprovalsOverrideAgent"],
            override_agent
        );
        attach_definition(&app, &owner, target.id, definition_id).await;
        let request_id = create_human_request(&app, &owner, source.id, definition_id).await;
        if let Some(outcome) = human {
            let response = app
                .authenticated_client(&reviewer)
                .post_json(
                    &format!("{}/decisions", approvals_path(source.id)),
                    &json!({"definitionId": definition_id, "requestId": request_id,
                        "reviewedGitHeads": heads,
                        "phase": "ie", "outcome": outcome, "comment": "Combined quorum review evidence."}),
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        if let Some(outcome) = agent {
            crate::api_formal_agent_approvals::settle_for_quorum(
                &app,
                &owner,
                &source,
                definition_id,
                outcome,
            )
            .await;
        }
        let body = state(&app, &owner, source.id).await;
        let outgoing = requirement(&body, definition_id);
        assert_eq!(outgoing["state"], expected, "outgoing case {index}");
        // The escalation display (#1095) follows the same counted evidence
        // and names the counted agent decision; it never changes the status.
        let escalation = match index {
            3 => json!("overriddenByHuman"),
            10 => json!("needsHumanEscalation"),
            _ => Value::Null,
        };
        assert_eq!(
            outgoing["agentEscalation"]["state"], escalation,
            "escalation case {index}"
        );
        if !escalation.is_null() {
            let agent_decision = outgoing["decisionLog"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["actorType"] == "agent")
                .unwrap();
            assert!(agent_decision["agentReviewId"].is_string());
            assert_eq!(
                outgoing["agentEscalation"]["agentReviewId"],
                agent_decision["agentReviewId"]
            );
        }
        let admission = body["data"]["outgoing"]["mergeAdmission"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definitionId"] == definition_id.to_string())
            .unwrap();
        assert_eq!(admission["state"], expected, "admission case {index}");
        // The page admission above derives from the displayed roster. Exercise
        // the merge gate's independent definition/status SQL under its policy
        // locks as well, using the exact same pinned source vector.
        let mut db = app.scoped_db_for_org(org.id).await;
        g5_api::services::approval_config_service::lock_merge_admission(&mut db, org.id, target.id)
            .await
            .unwrap();
        let gate = g5_api::services::approval_config_service::evaluate_merge_admission(
            &mut db,
            org.id,
            source.id,
            target.id,
            &pinned_heads,
        )
        .await
        .unwrap();
        let gate_requirement = gate
            .requirements
            .iter()
            .find(|r| r.definition_id == definition_id)
            .unwrap();
        assert_eq!(gate_requirement.state, expected, "locked gate case {index}");
        assert_eq!(
            gate.admitted,
            body["data"]["outgoing"]["mergeAdmission"]["admitted"]
                .as_bool()
                .unwrap()
        );
        db.rollback().await.unwrap();

        // Overriding an agent never rewrites its original result or settles a
        // human request that has not received that person's own decision.
        assert_eq!(
            request(outgoing, request_id)["state"],
            if human.is_some() {
                "settled"
            } else {
                "pending"
            }
        );
        if let Some(outcome) = agent.filter(|outcome| *outcome != "deferred") {
            assert!(outgoing["decisionLog"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["actorType"] == "agent" && d["outcome"] == outcome));
        }
    }
    app.cleanup().await;
}
