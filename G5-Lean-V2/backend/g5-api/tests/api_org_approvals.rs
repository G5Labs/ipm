//! Integration tests for the org approval-definition endpoints
//! (`/api/v1/orgs/:orgId/approval-definitions`, B6 governance UI PR-1).
//!
//! Scenario ownership: `test_contract/org_approvals_management` D1–D4.
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! exercises the real request→route→middleware→service→database→response
//! cycle. No mocks, stubs, or fake persistence.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn definitions_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/approval-definitions")
}

fn versions_path(org_id: Uuid, definition_id: &str) -> String {
    format!("/api/v1/orgs/{org_id}/approval-definitions/{definition_id}/versions")
}

/// Member-role fixture: real membership row plus a JWT without `org:admin`.
async fn member_user(app: &TestApp, org_id: Uuid) -> TestUser {
    TestUser::create_with_scopes(
        app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await
}

async fn definition_row_count(app: &TestApp, org_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM approval_definitions WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&app.db)
        .await
        .expect("count approval_definitions")
}

// ===========================================================================
// test_contract/org_approvals_management D1
// ===========================================================================

/// Owner creates a definition (two member approvers, no agent); a plain
/// member lists it with profile-enriched approvers; the database holds
/// exactly one append-only row `(id, version=1)`.
#[tokio::test]
async fn create_and_list_definition_v1() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let owner_client = app.authenticated_client(&owner);
    let member_client = app.authenticated_client(&member);

    // Library starts empty for members.
    let response = member_client.get(&definitions_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    assert_eq!(body["data"].as_array().expect("array").len(), 0);

    // Owner creates v1 with owner + member as approvers.
    let response = owner_client
        .post_json(
            &definitions_path(org.id),
            &json!({
                "name": "Production merge sign-off",
                "description": "Required for merges into the production base workspace.",
                "humanUserIds": [owner.id, member.id],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse JSON");
    let data = &body["data"];
    assert_eq!(data["name"], "Production merge sign-off");
    assert_eq!(data["latestVersion"], 1);
    assert_eq!(data["invalidatesPrevious"], false);
    assert_eq!(data["requireAllReviewers"], false);
    assert_eq!(data["allowSelfApproval"], false);
    assert_eq!(data["humanApprovalsOverrideAgent"], true);
    assert_eq!(data["firstCreatedBy"]["userId"], owner.id.to_string());
    let definition_id = data["id"].as_str().expect("id").to_string();

    // Member sees it, profile-enriched, in humans_list order.
    let response = member_client.get(&definitions_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    let list = body["data"].as_array().expect("array");
    assert_eq!(list.len(), 1);
    let humans = list[0]["humans"].as_array().expect("humans");
    assert_eq!(humans.len(), 2);
    assert_eq!(humans[0]["userId"], owner.id.to_string());
    assert_eq!(humans[0]["stillMember"], true);
    assert!(humans[0]["email"].as_str().unwrap().contains("@g5.test"));
    assert_eq!(humans[1]["userId"], member.id.to_string());

    // Exactly one append-only row at version 1.
    let rows: Vec<(Uuid, i32, bool)> = sqlx::query_as(
        "SELECT id, version, invalidates_previous FROM approval_definitions WHERE org_id = $1",
    )
    .bind(org.id)
    .fetch_all(&app.db)
    .await
    .expect("fetch rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0.to_string(), definition_id);
    assert_eq!(rows[0].1, 1);
    assert!(!rows[0].2);

    app.cleanup().await;
}

// ===========================================================================
// test_contract/org_approvals_management D9
// ===========================================================================

#[tokio::test]
async fn agent_launch_defaults_explicit_values_and_invalid_shapes_are_typed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    // Old clients that send only the existing simple-agent pair receive the
    // exact server defaults on the immutable definition version.
    let response = client
        .post_json(
            &definitions_path(org.id),
            &json!({
                "name": "Default agent launch",
                "agentType": "simple",
                "agentPrompt": "Review the candidate."
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("default launch response");
    assert_eq!(body["data"]["agentHarness"], "claude_code");
    assert_eq!(body["data"]["agentModel"], "claude-opus-5-5");
    assert_eq!(body["data"]["agentEffort"], "high");
    assert!(body["data"].get("agentMode").is_none());

    let response = client
        .post_json(
            &definitions_path(org.id),
            &json!({
                "name": "Explicit Codex launch",
                "agentType": "simple",
                "agentPrompt": "Review the candidate.",
                "agentHarness": "codex",
                "agentModel": "gpt-5.6-sol",
                "agentEffort": "ultra"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("explicit launch response");
    assert_eq!(body["data"]["agentHarness"], "codex");
    assert_eq!(body["data"]["agentModel"], "gpt-5.6-sol");
    assert_eq!(body["data"]["agentEffort"], "ultra");

    let response = client
        .post_json(
            &definitions_path(org.id),
            &json!({ "name": "Human only", "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("human definition response");
    assert!(body["data"]["agentHarness"].is_null());
    assert!(body["data"]["agentModel"].is_null());
    assert!(body["data"]["agentEffort"].is_null());

    for invalid in [
        json!({
            "name": "Partial launch",
            "agentType": "simple",
            "agentPrompt": "Review.",
            "agentHarness": "claude_code",
            "agentEffort": "high"
        }),
        json!({
            "name": "Unsupported effort",
            "agentType": "simple",
            "agentPrompt": "Review.",
            "agentHarness": "claude_code",
            "agentModel": "claude-sonnet-4-6",
            "agentEffort": "ultra"
        }),
        json!({
            "name": "Mode is not configurable",
            "agentType": "simple",
            "agentPrompt": "Review.",
            "agentHarness": "claude_code",
            "agentModel": "claude-opus-5-5",
            "agentEffort": "high",
            "agentMode": "plan"
        }),
    ] {
        let response = client.post_json(&definitions_path(org.id), &invalid).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid definition launch must fail closed: {invalid}"
        );
    }
    assert_eq!(definition_row_count(&app, org.id).await, 3);

    app.cleanup().await;
}

// ===========================================================================
// test_contract/org_approvals_management D2
// ===========================================================================

/// A plain member's create and publish attempts are rejected with no write,
/// while the same member's library read succeeds.
#[tokio::test]
async fn member_write_rejected_read_allowed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let owner_client = app.authenticated_client(&owner);
    let member_client = app.authenticated_client(&member);

    // Seed one definition as owner so publish has a target.
    let response = owner_client
        .post_json(
            &definitions_path(org.id),
            &json!({ "name": "Schema-change review", "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse JSON");
    let definition_id = body["data"]["id"].as_str().expect("id").to_string();

    // Member create → 403, nothing written.
    let response = member_client
        .post_json(
            &definitions_path(org.id),
            &json!({ "name": "Member-created", "humanUserIds": [member.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Member publish → 403, nothing written.
    let response = member_client
        .post_json(
            &versions_path(org.id, &definition_id),
            &json!({ "name": "Member edit", "humanUserIds": [member.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    assert_eq!(definition_row_count(&app, org.id).await, 1);

    // Member read still succeeds.
    let response = member_client.get(&definitions_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);

    app.cleanup().await;
}

// ===========================================================================
// test_contract/org_approvals_management D3
// ===========================================================================

/// Publishing an edit appends `(id, v2)` without touching v1; the list
/// returns latest content with v1 "created by" provenance.
#[tokio::test]
async fn publish_appends_version_preserves_provenance() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = member_user(&app, org.id).await;
    let admin = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgAdmin,
        ],
    )
    .await;
    // Promote the second writer to admin role so role and scope agree.
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(admin.id)
        .execute(&app.db)
        .await
        .expect("promote admin");

    let owner_client = app.authenticated_client(&owner);
    let admin_client = app.authenticated_client(&admin);

    let response = owner_client
        .post_json(
            &definitions_path(org.id),
            &json!({
                "name": "Security-sensitive paths",
                "description": "Auth, tenancy, and RLS surfaces.",
                "humanUserIds": [owner.id],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse JSON");
    let definition_id = body["data"]["id"].as_str().expect("id").to_string();
    let v1_created_at = body["data"]["firstCreatedAt"].as_str().unwrap().to_string();

    // Admin (not the creator) publishes v2 with a changed approver set + agent.
    let response = admin_client
        .post_json(
            &versions_path(org.id, &definition_id),
            &json!({
                "name": "Security-sensitive paths",
                "description": "Auth, tenancy, and RLS surfaces.",
                "humanUserIds": [owner.id, member.id],
                "agentType": "simple",
                "agentPrompt": "Approve only when auth/tenancy/RLS surfaces are untouched or reviewed.",
                "invalidatesPrevious": true,
                "humanApprovalsOverrideAgent": false,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    let data = &body["data"];
    assert_eq!(data["latestVersion"], 2);
    assert_eq!(data["invalidatesPrevious"], true);
    assert_eq!(data["humanApprovalsOverrideAgent"], false);
    assert_eq!(data["agentType"], "simple");
    // Latest-version provenance is the admin; v1 provenance stays the owner.
    assert_eq!(data["createdBy"]["userId"], admin.id.to_string());
    assert_eq!(data["firstCreatedBy"]["userId"], owner.id.to_string());
    assert_eq!(data["firstCreatedAt"], v1_created_at.as_str());

    // v1 row untouched; two rows total.
    let rows: Vec<(i32, Vec<Uuid>, bool, bool)> = sqlx::query_as(
        "SELECT version, humans_list, invalidates_previous, human_approvals_override_agent \
         FROM approval_definitions WHERE org_id = $1 AND id = $2::uuid ORDER BY version",
    )
    .bind(org.id)
    .bind(&definition_id)
    .fetch_all(&app.db)
    .await
    .expect("fetch versions");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, 1);
    assert_eq!(rows[0].1, vec![owner.id]);
    assert!(!rows[0].2);
    assert!(rows[0].3);
    assert_eq!(rows[1].0, 2);
    assert_eq!(rows[1].1, vec![owner.id, member.id]);
    assert!(rows[1].2);
    assert!(!rows[1].3);

    // Publishing against a missing definition id → 404.
    let response = admin_client
        .post_json(
            &versions_path(org.id, &Uuid::new_v4().to_string()),
            &json!({ "name": "Ghost", "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

/// A raced publish loses with 409 and writes nothing durable, while the
/// winning transaction commits cleanly (real overlapping transactions:
/// transaction A holds the uncommitted next version while request B runs
/// through the real route).
#[tokio::test]
async fn raced_publish_conflicts_without_durable_write() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    let response = owner_client
        .post_json(
            &definitions_path(org.id),
            &json!({ "name": "Raced definition", "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse JSON");
    let definition_id: Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();

    // Transaction A: insert (id, v2) and stay open.
    let mut tx_a = app.db.begin().await.expect("begin tx A");
    sqlx::query(
        "INSERT INTO approval_definitions \
         (id, version, org_id, name, description, humans_list, agent_type, agent_prompt, \
          invalidates_previous, created_by) \
         VALUES ($1, 2, $2, 'Raced definition', NULL, $3, NULL, NULL, false, $4)",
    )
    .bind(definition_id)
    .bind(org.id)
    .bind(vec![owner.id])
    .bind(owner.id)
    .execute(&mut *tx_a)
    .await
    .expect("tx A inserts v2 uncommitted");

    // Request B runs concurrently through the real route; its insert blocks
    // on A's uncommitted (id, 2) key until A commits, then resolves to a
    // conflict.
    let b_path = versions_path(org.id, &definition_id.to_string());
    let b_body = json!({
        "name": "Raced definition (loser edit)",
        "humanUserIds": [owner.id],
        "invalidatesPrevious": true,
    });
    let b = tokio::spawn({
        let client = app.authenticated_client(&owner);
        async move { client.post_json(&b_path, &b_body).await }
    });

    // Deterministic sync: wait until B's insert is actually blocked on A's
    // uncommitted key (a backend waiting on a Lock for an approval_definitions
    // INSERT) before committing A. A fixed sleep would race loaded CI runners:
    // if A commits before B reaches the lock, B computes MAX+1 = 3 and wins
    // with 200 instead of conflicting.
    let waited = async {
        for _ in 0..100 {
            let blocked: Option<i64> = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pg_stat_activity \
                 WHERE wait_event_type = 'Lock' \
                   AND query ILIKE '%INSERT INTO approval_definitions%'",
            )
            .fetch_optional(&app.db)
            .await
            .expect("probe pg_stat_activity");
            if blocked.unwrap_or(0) > 0 {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }
    .await;
    assert!(waited, "request B never blocked on transaction A's lock");
    tx_a.commit().await.expect("tx A commits cleanly");

    let response = b.await.expect("join B");
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "raced publish must fail fast with 409"
    );

    // Loser wrote nothing durable: exactly v1 + A's v2, with A's content.
    let rows: Vec<(i32, String, bool)> = sqlx::query_as(
        "SELECT version, name, invalidates_previous \
         FROM approval_definitions WHERE org_id = $1 AND id = $2 ORDER BY version",
    )
    .bind(org.id)
    .bind(definition_id)
    .fetch_all(&app.db)
    .await
    .expect("fetch rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0, 2);
    assert_eq!(rows[1].1, "Raced definition");
    assert!(
        !rows[1].2,
        "winner's invalidates_previous=false must survive"
    );

    app.cleanup().await;
}

// ===========================================================================
// test_contract/org_approvals_management D7
// ===========================================================================

/// Creating a definition without `phases` binds Integration Engineering;
/// publishing a version that binds all three phases round-trips exactly
/// through the listing while v1 provenance and the v1 binding survive
/// (`g5.workbench.gates`).
#[tokio::test]
async fn phase_binding_defaults_and_roundtrip() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    // Create without `phases` → the IE default that keeps every new
    // definition a merge requirement.
    let response = client
        .post_json(
            &definitions_path(org.id),
            &json!({ "name": "Phase defaults", "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("parse JSON");
    let data = &body["data"];
    assert_eq!(data["phases"], json!(["ie"]));
    let definition_id = data["id"].as_str().expect("id").to_string();

    // Publish v2 binding every phase.
    let response = client
        .post_json(
            &versions_path(org.id, &definition_id),
            &json!({
                "name": "Phase defaults",
                "humanUserIds": [owner.id],
                "phases": ["be", "se", "ie"],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    let data = &body["data"];
    assert_eq!(data["latestVersion"], 2);
    assert_eq!(data["phases"], json!(["be", "se", "ie"]));

    // The listing's latest version carries exactly those values; v1 provenance
    // stays the owner.
    let response = client.get(&definitions_path(org.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("parse JSON");
    let list = body["data"].as_array().expect("array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["latestVersion"], 2);
    assert_eq!(list[0]["phases"], json!(["be", "se", "ie"]));
    assert_eq!(list[0]["firstCreatedBy"]["userId"], owner.id.to_string());

    // Durable rows: v1 keeps the default, v2 carries the new binding; an
    // empty binding (attached but inert) is stored as such.
    let response = client
        .post_json(
            &versions_path(org.id, &definition_id),
            &json!({
                "name": "Phase defaults",
                "humanUserIds": [owner.id],
                "phases": [],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let rows: Vec<(i32, Vec<String>)> = sqlx::query_as(
        "SELECT version, phases \
         FROM approval_definitions WHERE org_id = $1 AND id = $2::uuid ORDER BY version",
    )
    .bind(org.id)
    .bind(&definition_id)
    .fetch_all(&app.db)
    .await
    .expect("fetch versions");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], (1, vec!["ie".to_string()]));
    assert_eq!(
        rows[1],
        (
            2,
            vec!["be".to_string(), "se".to_string(), "ie".to_string()]
        )
    );
    assert_eq!(rows[2], (3, Vec::<String>::new()));

    app.cleanup().await;
}

/// An out-of-vocabulary or duplicated phase is a typed 422 naming the field
/// and writes no `approval_definitions` row.
#[tokio::test]
async fn phase_binding_validation_rejects_bad_phases() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    for phases in [json!(["planning"]), json!(["ie", "ie"])] {
        let response = client
            .post_json(
                &definitions_path(org.id),
                &json!({
                    "name": "Bad phases",
                    "humanUserIds": [owner.id],
                    "phases": phases,
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let error: Value = response.json().await.expect("parse JSON");
        let details = error["error"]["details"].to_string();
        assert!(
            details.contains("phases"),
            "422 should name phases; got {details}"
        );
    }

    assert_eq!(definition_row_count(&app, org.id).await, 0);

    app.cleanup().await;
}

// ===========================================================================
// test_contract/org_approvals_management D4
// ===========================================================================

/// Malformed definitions are rejected with typed 422s naming the offending
/// field, and no `approval_definitions` row is written.
#[tokio::test]
async fn validation_rejects_malformed_requests() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let outsider = TestUser::create(&app).await; // no membership in org
    let client = app.authenticated_client(&owner);

    let cases: Vec<(Value, &str)> = vec![
        (json!({ "name": "   ", "humanUserIds": [owner.id] }), "name"),
        (json!({ "name": "No approvers" }), "humanUserIds"),
        (
            json!({ "name": "Agent no prompt", "humanUserIds": [owner.id], "agentType": "simple" }),
            "agentPrompt",
        ),
        (
            json!({ "name": "Prompt no agent", "humanUserIds": [owner.id], "agentPrompt": "check" }),
            "agentPrompt",
        ),
        (
            json!({ "name": "Bad agent kind", "humanUserIds": [owner.id], "agentType": "swarm", "agentPrompt": "check" }),
            "agentType",
        ),
        (
            json!({ "name": "Duplicates", "humanUserIds": [owner.id, owner.id] }),
            "humanUserIds",
        ),
        (
            json!({ "name": "Non-member approver", "humanUserIds": [outsider.id] }),
            "humanUserIds",
        ),
    ];

    for (body, field) in cases {
        let response = client.post_json(&definitions_path(org.id), &body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "case should fail validation: {body}"
        );
        let error: Value = response.json().await.expect("parse JSON");
        let details = error["error"]["details"].to_string();
        assert!(
            details.contains(field),
            "422 for {body} should name field {field}; got {details}"
        );
    }

    assert_eq!(definition_row_count(&app, org.id).await, 0);

    app.cleanup().await;
}
