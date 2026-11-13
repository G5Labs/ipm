//! Production admission, checked Git fork, isolation, timeout/cancellation and
//! recovery boundaries. Live VM/provider execution is covered in Rust E2E.
use crate::helpers::{self, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "formal_agent_approval_acceptance.rs"]
mod acceptance;
#[path = "formal_agent_approval_authority.rs"]
mod authority;
#[path = "formal_agent_approval_commit_recovery.rs"]
mod commit_recovery;
#[path = "formal_agent_approval_inspection.rs"]
mod inspection;

struct Fixture {
    app: TestApp,
    owner: TestUser,
    org: TestOrg,
    source: TestWorkspace,
    target: Uuid,
    definition: Uuid,
}
impl Fixture {
    async fn new() -> Self {
        let _ = helpers::tracing_capture();
        Self::with_connections(2).await
    }
    async fn with_connections(max: u32) -> Self {
        let mut config = helpers::test_config();
        config.database.max_connections = max;
        config.tenant_database.max_connections = max;
        config.authz_database.max_connections = max;
        let app = TestApp::spawn_with_config(config).await;
        Self::with_app(app).await
    }
    async fn with_app(app: TestApp) -> Self {
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let source = TestWorkspace::create(&app, &org).await;
        let target = source.attach_parent(&app).await;
        let lineage = source.attach_parent(&app).await;
        assert_ne!(lineage, target);
        sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
            .bind(source.id)
            .bind(target)
            .execute(&app.db)
            .await
            .unwrap();
        let client = app.authenticated_client(&owner);
        let response=client.post_json(&format!("/api/v1/orgs/{}/approval-definitions",org.id),&json!({"name":"Frozen formal policy","agentType":"simple","agentPrompt":"Review only the supplied cut.","phases":["ie"]})).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        let definition = body["data"]["id"].as_str().unwrap().parse().unwrap();
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{target}/approvals/requirements"),
                &json!({"definitionId":definition}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let mut fixture = Self {
            app,
            owner,
            org,
            source,
            target,
            definition,
        };
        fixture.refork_source().await;
        fixture
    }
    /// A production-created workspace carries the immutable saved base.
    async fn fork_source(&self) -> (Uuid, String) {
        let name = format!("source-{}", Uuid::new_v4());
        let response = self.app.authenticated_client(&self.owner).post_json(
            &format!("/api/v1/workspaces/{}/clone", self.source.id),
            &json!({"name":name,"idempotencyKey":Uuid::new_v4().to_string(),"mergeTargetWorkspaceId":self.target})
        ).await;
        let value = helpers::review_flow::body(response, StatusCode::CREATED).await;
        (
            value["workspace"]["id"].as_str().unwrap().parse().unwrap(),
            name,
        )
    }
    async fn refork_source(&mut self) {
        let (id, name) = self.fork_source().await;
        self.source.id = id;
        self.source.name = name;
        // Runtime peers install their own incarnation. Preserve the fixture's
        // unassigned source shape without removing any active VM.
        sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1 AND status='stopped' AND vm_id IS NULL")
            .bind(self.source.id).execute(&self.app.db).await.unwrap();
        let path = self.app.repo_path(self.source.repo_id).await;
        let repo = git2::Repository::open(path).unwrap();
        repo.set_head(&format!("refs/heads/{}", self.source.name))
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
    }
    async fn request_response(&self, retry: Option<Uuid>) -> (StatusCode, Value) {
        let response = self.app.authenticated_client(&self.owner)
            .post_json(
                &format!("/api/v1/workspaces/{}/approvals/reviews", self.source.id),
                &json!({"definitionId":self.definition,"definitionVersion":1,"phase":"ie","retryReviewId":retry}),
            ).await;
        (response.status(), response.json().await.unwrap())
    }
    async fn request(&self, retry: Option<Uuid>) -> Uuid {
        let (status, body) = self.request_response(retry).await;
        assert!(status.is_success(), "{status} {body}");
        assert_ne!(body["data"]["requestId"], body["data"]["reviewId"]);
        body["data"]["reviewId"].as_str().unwrap().parse().unwrap()
    }
    async fn fail_deadline(&self, review: Uuid) {
        sqlx::query("UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1")
            .bind(review).execute(&self.app.db).await.unwrap();
        self.reconcile().await;
        assert_eq!(self.report(review).await["stage"], "failed");
        self.reconcile().await;
    }
    async fn gate(&self) -> Value {
        let response = self
            .app
            .authenticated_client(&self.owner)
            .get(&format!("/api/v1/workspaces/{}/approvals", self.source.id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        body["data"]["outgoing"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["definition"]["id"] == self.definition.to_string())
            .unwrap()
            .clone()
    }
    async fn settle(&self, verdict: g5_contracts::types::AgentApprovalOutcome) -> Uuid {
        let review = self.request(None).await;
        self.reconcile().await;
        self.terminal_receipt(review, verdict).await;
        self.reconcile().await;
        self.reconcile().await;
        assert_eq!(self.report(review).await["stage"], "effective");
        self.reconcile().await;
        review
    }
    async fn request_id(&self, review: Uuid) -> Uuid {
        sqlx::query_scalar("SELECT approval_request_id FROM agent_approval_reviews WHERE id=$1")
            .bind(review)
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }
    async fn reconcile(&self) {
        g5_api::services::approval_review_service::workflow::run_once(&self.app.state)
            .await
            .unwrap();
    }
    async fn report(&self, review: Uuid) -> Value {
        let response = self
            .app
            .authenticated_client(&self.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/approvals/reviews/{review}/report",
                self.source.id
            ))
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].clone()
    }
    async fn child(&self, review: Uuid) -> Uuid {
        sqlx::query_scalar("SELECT review_workspace_id FROM agent_approval_reviews WHERE id=$1")
            .bind(review)
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn checked_review_fork_is_requester_owned_target_parented_and_inaccessible_to_all_humans() {
    let f = Fixture::new().await;
    let review = f.request(None).await;
    f.reconcile().await;
    let child = f.child(review).await;
    let record:(Uuid,Uuid,String,Option<Uuid>)=sqlx::query_as("SELECT owner_user_id,cloned_from_id,push_mode,agent_approval_review_id FROM workspaces WHERE id=$1").bind(child).fetch_one(&f.app.db).await.unwrap();
    assert_eq!(
        record,
        (f.owner.id, f.target, "manual".into(), Some(review))
    );
    let report = f.report(review).await;
    assert_eq!(report["stage"], "start");
    assert_eq!(
        report["assignment"]["resolution"]["sourceHeads"][0]["headCommit"],
        f.source.head_commit
    );
    assert_eq!(
        report["assignment"]["resolution"]["targetWorkspaceId"],
        f.target.to_string()
    );
    let hidden_session = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(hidden_session)
        .bind(f.org.id)
        .bind(child)
        .bind(f.owner.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chat_messages(id,org_id,session_id,role,content,message_index) VALUES($1,$2,$3,'assistant','private review',0)")
        .bind(Uuid::new_v4()).bind(f.org.id).bind(hidden_session).execute(&f.app.db).await.unwrap();
    sqlx::query("INSERT INTO session_inputs(id,org_id,session_id,input_index,content,input_origin) VALUES($1,$2,$3,0,'private review input','human')")
        .bind(Uuid::new_v4()).bind(f.org.id).bind(hidden_session).execute(&f.app.db).await.unwrap();
    let mut admin = TestUser::create(&f.app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
        .bind(f.org.id)
        .bind(admin.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    admin.jwt = helpers::test_jwt_minter()
        .mint(
            admin.id,
            f.org.id,
            g5_auth::Scope::all()
                .iter()
                .copied()
                .filter(|scope| *scope != g5_auth::Scope::WorkerHost)
                .collect(),
            3600,
        )
        .unwrap();
    for user in [&f.owner, &admin] {
        let client = f.app.authenticated_client(user);
        for path in [
            format!("/api/v1/workspaces/{child}"),
            format!("/api/v1/workspaces/{child}/vm"),
            format!("/api/v1/workspaces/{child}/tasks"),
            format!("/api/v1/workspaces/{child}/sessions"),
            format!(
                "/api/v1/workspaces/{child}/files/README.md?repoId={}",
                f.source.repo_id
            ),
            format!("/api/v1/workspaces/{child}/settings"),
        ] {
            let response = client.get(&path).await;
            assert!(
                !response.status().is_success(),
                "human read admitted {path}"
            );
        }
        for (path, body) in [
            (format!("/api/v1/workspaces/{child}/vm/resume"), json!({})),
            (
                format!("/api/v1/workspaces/{child}/clone"),
                json!({"name":"human-review-escape","idempotencyKey":Uuid::new_v4()}),
            ),
            (format!("/api/v1/workspaces/{child}/delete"), json!({})),
            (
                format!("/api/v1/workspaces/{child}/sessions/start"),
                json!({"content":"Mutate review"}),
            ),
        ] {
            let response = client.post_json(&path, &body).await;
            assert!(
                !response.status().is_success(),
                "human mutation admitted {path}"
            );
        }
        let advertised = client
            .get(&format!(
                "/git/{}/info/refs?service=git-upload-pack",
                f.source.repo_id
            ))
            .await;
        assert_eq!(advertised.status(), StatusCode::OK);
        let refs = advertised.text().await.unwrap();
        assert!(
            !refs.contains(&format!("agent-review-{}", child.simple())),
            "{refs}"
        );
        assert!(!refs.contains(&format!("refs/g5/forks/{child}")), "{refs}");
        assert!(!refs.contains("refs/g5/approval-evidence/"), "{refs}");
        assert!(
            !refs.contains(&format!("g5-agent-review-base-{}", review.simple())),
            "{refs}"
        );
        let mut db = f.app.scoped_db_for_user_org(user.id, f.org.id).await;
        let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(child)
            .fetch_one(db.conn())
            .await
            .unwrap();
        assert_eq!(seen, 0, "RLS denies owner/admin inventory too");
        for table in ["chat_sessions", "chat_messages", "session_inputs"] {
            let key = if table == "chat_sessions" {
                "id"
            } else {
                "session_id"
            };
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE {key}=$1"))
                    .bind(hidden_session)
                    .fetch_one(db.conn())
                    .await
                    .unwrap();
            assert_eq!(count, 0, "nullable task ownership must not expose {table}");
        }
        db.rollback().await.unwrap();
    }
    let source_head = f.source.git_head(&f.app);
    assert_eq!(source_head, f.source.head_commit);
    sqlx::query(
        "UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(review)
    .execute(&f.app.db)
    .await
    .unwrap();
    f.reconcile().await;
    f.reconcile().await; // cleanup is an independent due pass
    let failed = f.report(review).await;
    assert_eq!(
        failed["stage"],
        "failed",
        "{}",
        json!({"stage":failed["stage"],"failure":failed["failureCause"],"cleanup":failed["cleanupError"]})
    );
    assert_eq!(failed["failureStage"], "start");
    assert!(failed["result"].is_null());
    assert_eq!(
        failed["cleanupPending"],
        false,
        "{}",
        json!({"stage":failed["stage"],"failure":failed["failureCause"],"cleanup":failed["cleanupError"]})
    );
    let statuses: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE source_workspace_id=$1")
            .bind(f.source.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(statuses, 0);
    f.app.cleanup().await;
}

#[tokio::test]
async fn failed_execution_retry_keeps_its_frozen_vectors_while_new_review_captures_current_inputs()
{
    let f = Fixture::new().await;
    let first = f.request(None).await;
    f.reconcile().await;
    let original = f.report(first).await;
    sqlx::query(
        "UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(first)
    .execute(&f.app.db)
    .await
    .unwrap();
    f.reconcile().await;
    f.reconcile().await; // cleanup is an independent due pass
    let failed = f.report(first).await;
    assert_eq!(
        failed["cleanupPending"],
        false,
        "{}",
        json!({"stage":failed["stage"],"failure":failed["failureCause"],"cleanup":failed["cleanupError"]})
    );
    let path = f.app.repo_path(f.source.repo_id).await;
    let repository = git2::Repository::open(path).unwrap();
    let old = repository
        .find_commit(git2::Oid::from_str(&f.source.head_commit).unwrap())
        .unwrap();
    let tree = old.tree().unwrap();
    let signature = git2::Signature::now("Test", "test@example.invalid").unwrap();
    let advanced = repository
        .commit(
            None,
            &signature,
            &signature,
            "Later source input",
            &tree,
            &[&old],
        )
        .unwrap()
        .to_string();
    drop(tree);
    drop(old);
    drop(repository);
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &advanced).await;
    let retried = f.request(Some(first)).await;
    assert_ne!(retried, first);
    f.reconcile().await;
    let frozen = f.report(retried).await;
    assert_eq!(frozen["stage"], "start", "{frozen}");
    assert_eq!(frozen["assignment"], original["assignment"]);
    assert_eq!(f.source.git_head(&f.app), advanced);
    let cancel = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/approvals/requests/{}/cancel",
                f.source.id,
                f.request_id(retried).await
            ),
            &json!({}),
        )
        .await;
    assert_eq!(cancel.status(), StatusCode::OK);
    f.reconcile().await;
    assert_eq!(f.report(retried).await["stage"], "cancelled");
    let current = f.request(None).await;
    f.reconcile().await;
    let now = f.report(current).await;
    assert_eq!(
        now["assignment"]["resolution"]["sourceHeads"][0]["headCommit"],
        advanced
    );
    assert_ne!(
        now["assignment"]["resolution"]["resolutionId"],
        original["assignment"]["resolution"]["resolutionId"]
    );
    f.app.cleanup().await;
}

#[tokio::test]
async fn clone_parent_without_target_override_supports_the_complete_formal_review_lifecycle() {
    let f = Fixture::new().await;
    // Ordinary child workspaces inherit their merge destination from lineage.
    sqlx::query(
        "UPDATE workspaces SET merge_target_workspace_id=NULL,cloned_from_id=$2 WHERE id=$1",
    )
    .bind(f.source.id)
    .bind(f.target)
    .execute(&f.app.db)
    .await
    .unwrap();
    assert_eq!(f.gate().await["state"], "pending");

    let review = f.request(None).await;
    f.reconcile().await;
    let child = f.child(review).await;
    let parent: Uuid = sqlx::query_scalar("SELECT cloned_from_id FROM workspaces WHERE id=$1")
        .bind(child)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(parent, f.target);
    let report = f.report(review).await;
    assert_eq!(report["stage"], "start", "{report}");
    assert_eq!(
        report["assignment"]["resolution"]["targetWorkspaceId"],
        f.target.to_string()
    );

    f.terminal_receipt(review, g5_contracts::types::AgentApprovalOutcome::Approved)
        .await;
    f.reconcile().await;
    f.reconcile().await;
    assert_eq!(f.report(review).await["stage"], "effective");
    f.reconcile().await;
    assert_eq!(f.report(review).await["cleanupPending"], false);
    assert_eq!(f.gate().await["state"], "approved");
    f.app.cleanup().await;
}

#[tokio::test]
async fn deleted_target_override_never_falls_back_to_a_valid_clone_parent() {
    let f = Fixture::new().await;
    let invalid = TestWorkspace::create(&f.app, &f.org).await;
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(invalid.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2,cloned_from_id=$3 WHERE id=$1")
        .bind(f.source.id)
        .bind(invalid.id)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    // Requirements exist on the live parent, so fallback would wrongly admit.
    let (status, body) = f.request_response(None).await;
    assert!(!status.is_success(), "{status} {body}");
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_approval_reviews")
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(reviews, 0);
    f.app.cleanup().await;
}

#[tokio::test]
async fn invalid_target_and_capture_deadline_never_fall_back_or_manufacture_rejection() {
    let f = Fixture::new().await;
    sqlx::query(
        "UPDATE workspaces SET merge_target_workspace_id=NULL,cloned_from_id=NULL WHERE id=$1",
    )
    .bind(f.source.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/approvals/reviews", f.source.id),
            &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(f.source.id)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let review = f.request(None).await;
    sqlx::query(
        "UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(review)
    .execute(&f.app.db)
    .await
    .unwrap();
    f.reconcile().await;
    f.reconcile().await; // cleanup is an independent due pass
    let report = f.report(review).await;
    assert_eq!(report["stage"], "failed");
    assert_eq!(report["failureStage"], "capture");
    assert_eq!(report["cleanupPending"], false);
    assert!(report["result"].is_null());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE agent_approval_review_id=$1")
            .bind(review)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(count, 0);
    f.app.cleanup().await;
}

// Inject only the provider's already-terminal receipt. The production owner
// still validates, persists, seals, settles and disposes the real checked fork.
impl Fixture {
    async fn terminal_receipt(
        &self,
        review: Uuid,
        verdict: g5_contracts::types::AgentApprovalOutcome,
    ) -> String {
        let document =
            terminal_receipt_for(&self.app, self.org.id, self.owner.id, review, verdict).await;
        assert_eq!(self.report(review).await["stage"], "harvest");
        document
    }
}

#[tokio::test]
async fn candidate_precedes_seal_and_cleanup_failure_cannot_change_either_effective_verdict() {
    use g5_contracts::types::AgentApprovalOutcome;
    for verdict in [
        AgentApprovalOutcome::Approved,
        AgentApprovalOutcome::Rejected,
    ] {
        let f = Fixture::new().await;
        let review = f.request(None).await;
        f.reconcile().await;
        let document = f.terminal_receipt(review, verdict).await;
        let statuses: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM approval_statuses WHERE source_workspace_id=$1",
        )
        .bind(f.source.id)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
        assert_eq!(statuses, 0);
        f.reconcile().await;
        let candidate = f.report(review).await;
        assert_eq!(candidate["stage"], "seal");
        assert!(candidate["result"].is_null());
        assert_eq!(candidate["document"], document);
        sqlx::raw_sql("CREATE FUNCTION fail_review_disposal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.agent_approval_review_id IS NOT NULL AND NEW.deleted_at IS NOT NULL THEN RAISE EXCEPTION 'injected disposal failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_review_disposal BEFORE UPDATE ON workspaces FOR EACH ROW EXECUTE FUNCTION fail_review_disposal();").execute(&f.app.db).await.unwrap();
        tokio::join!(f.reconcile(), f.reconcile());
        let effective = f.report(review).await;
        assert_eq!(effective["stage"], "effective", "{effective}");
        assert_eq!(effective["result"], serde_json::to_value(verdict).unwrap());
        let child = f.child(review).await;
        let policy: String =
            sqlx::query_scalar("SELECT mutation_policy FROM workspaces WHERE id=$1")
                .bind(child)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(policy, "read_only");
        f.reconcile().await;
        let pending = f.report(review).await;
        assert_eq!(pending["result"], effective["result"]);
        assert_eq!(pending["cleanupPending"], true);
        assert!(pending["cleanupError"]
            .as_str()
            .unwrap()
            .contains("injected disposal failure"));
        sqlx::raw_sql("DROP TRIGGER fail_review_disposal ON workspaces; DROP FUNCTION fail_review_disposal();").execute(&f.app.db).await.unwrap();
        let retry = f
            .app
            .authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/approvals/reviews/{review}/cleanup",
                    f.source.id
                ),
                &json!({}),
            )
            .await;
        assert_eq!(retry.status(), StatusCode::OK);
        f.reconcile().await;
        let retained = f.report(review).await;
        assert_eq!(retained["cleanupPending"], false, "{retained}");
        assert_eq!(retained["document"], document);
        assert_eq!(retained["sha256"], candidate["sha256"]);
        let cancellation = f
            .app
            .authenticated_client(&f.owner)
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/approvals/requests/{}/cancel",
                    f.source.id,
                    f.request_id(review).await
                ),
                &json!({}),
            )
            .await;
        assert!(!cancellation.status().is_success());
        assert_eq!(f.report(review).await["result"], effective["result"]);
        let statuses: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE agent_review_id=$1")
                .bind(review)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(statuses, 1);
        // New assignments use schema 6 (schema 5's frontmatter): the decision
        // stores the exact authored summary, never a clip of the report body.
        let (schema, comment): (Value, Option<String>) = sqlx::query_as(
            "SELECT r.invocation->'resolution'->'schemaVersion', s.comment FROM approval_statuses s JOIN agent_approval_reviews r ON r.id=s.agent_review_id WHERE s.agent_review_id=$1",
        )
        .bind(review)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
        assert_eq!(schema, json!(6));
        assert_eq!(comment.as_deref(), Some(TEST_AUTHORED_SUMMARY));
        assert!(document.contains(&format!("summary: '{TEST_AUTHORED_SUMMARY}'")));
        let activity = helpers::review_flow::body(
            f.app
                .authenticated_client(&f.owner)
                .get(&format!("/api/v1/workspaces/{}/activity", f.source.id))
                .await,
            StatusCode::OK,
        )
        .await;
        let decision = activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "approval_decision" && entry["actorType"] == "agent")
            .unwrap_or_else(|| panic!("{activity}"));
        assert_eq!(decision["summary"], TEST_AUTHORED_SUMMARY, "{decision}");
        assert_eq!(decision["comment"], Value::Null, "{decision}");
        assert_eq!(decision["historical"], false, "{decision}");
        // A legacy (schema 4) agent decision exposes no summary or clip.
        // Assignments are immutable; this fixture bypasses the guard only to
        // stand in for a row written before schema 5.
        sqlx::raw_sql(&format!(
            "BEGIN; ALTER TABLE agent_approval_reviews DISABLE TRIGGER formal_review_history; \
             UPDATE agent_approval_reviews SET invocation=jsonb_set(invocation,'{{resolution,schemaVersion}}','4') WHERE id='{review}'; \
             ALTER TABLE agent_approval_reviews ENABLE TRIGGER formal_review_history; COMMIT;"
        ))
        .execute(&f.app.db)
        .await
        .unwrap();
        let activity = helpers::review_flow::body(
            f.app
                .authenticated_client(&f.owner)
                .get(&format!("/api/v1/workspaces/{}/activity", f.source.id))
                .await,
            StatusCode::OK,
        )
        .await;
        let decision = activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "approval_decision" && entry["actorType"] == "agent")
            .unwrap();
        assert_eq!(decision["summary"], Value::Null, "{decision}");
        assert_eq!(decision["comment"], Value::Null, "{decision}");
        assert_eq!(f.source.git_head(&f.app), f.source.head_commit);
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn persistence_and_sealing_failures_retain_stage_without_authoring_a_rejection() {
    for stage in ["harvest", "seal"] {
        let f = Fixture::new().await;
        let review = f.request(None).await;
        f.reconcile().await;
        f.terminal_receipt(review, g5_contracts::types::AgentApprovalOutcome::Approved)
            .await;
        if stage == "harvest" {
            sqlx::raw_sql("CREATE FUNCTION fail_review_persist() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected persistence failure'; END $$; CREATE TRIGGER fail_review_persist BEFORE INSERT ON agent_approval_artifacts FOR EACH ROW EXECUTE FUNCTION fail_review_persist();").execute(&f.app.db).await.unwrap();
        } else {
            f.reconcile().await;
            sqlx::query("UPDATE tasks SET status='failed' WHERE id=(SELECT task_id FROM agent_approval_reviews WHERE id=$1)").bind(review).execute(&f.app.db).await.unwrap();
        }
        f.reconcile().await;
        let failed = f.report(review).await;
        assert_eq!(failed["stage"], "failed", "{failed}");
        assert_eq!(failed["failureStage"], stage);
        assert!(failed["result"].is_null());
        if stage == "seal" {
            assert!(failed["document"].as_str().is_some());
        }
        let statuses: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE agent_review_id=$1")
                .bind(review)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(statuses, 0);
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn ready_formal_tasks_use_harness_modes_without_ordinary_conversation_settings() {
    for (provider, model, mode) in [
        (
            "claude_code",
            g5_contracts::claude_models::CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
            "default",
        ),
        (
            "codex",
            g5_contracts::codex_models::CODEX_DIRECT_CHAT_DEFAULT_MODEL,
            "never/read-only",
        ),
    ] {
        for configured in [false, true] {
            let mut f = Fixture::with_connections(8).await;
            sqlx::query("UPDATE org_configs SET settings_json=settings_json-'claude_permission_mode'-'codex_permission_mode' WHERE org_id=$1")
                .bind(f.org.id).execute(&f.app.db).await.unwrap();
            if configured {
                sqlx::query("UPDATE org_configs SET settings_json=settings_json || $2 WHERE org_id=$1")
                    .bind(f.org.id).bind(json!({"claude_permission_mode":"bypassPermissions","codex_permission_mode":"never/danger-full-access"}))
                    .execute(&f.app.db).await.unwrap();
            } else {
                let ordinary = f
                    .app
                    .authenticated_client(&f.owner)
                    .post_json(
                        &format!("/api/v1/workspaces/{}/sessions/start", f.source.id),
                        &json!({"content":"Ordinary conversation still needs its own mode"}),
                    )
                    .await;
                assert_eq!(ordinary.status(), StatusCode::UNPROCESSABLE_ENTITY);
                assert!(ordinary
                    .text()
                    .await
                    .unwrap()
                    .contains("missing_agent_permission_mode"));
            }
            let definition = f.app.authenticated_client(&f.owner).post_json(
                &format!("/api/v1/orgs/{}/approval-definitions", f.org.id),
                &json!({"name":"Fixed review launch","agentType":"simple","agentHarness":provider,"agentModel":model,"agentEffort":"high","agentPrompt":"Review the frozen cut.","phases":["ie"]}),
            ).await;
            assert_eq!(definition.status(), StatusCode::CREATED);
            let definition: Value = definition.json().await.unwrap();
            f.definition = definition["data"]["id"].as_str().unwrap().parse().unwrap();
            let attached = f
                .app
                .authenticated_client(&f.owner)
                .post_json(
                    &format!("/api/v1/workspaces/{}/approvals/requirements", f.target),
                    &json!({"definitionId":f.definition}),
                )
                .await;
            assert_eq!(attached.status(), StatusCode::OK);
            let review = f.request(None).await;
            f.reconcile().await;
            let child = TestWorkspace {
                id: f.child(review).await,
                org_id: f.org.id,
                repo_id: f.source.repo_id,
                head_commit: f.source.head_commit.clone(),
                name: format!("agent-review-{}", f.child(review).await.simple()),
            };
            // The checked fork leaves an unassigned stopped placeholder;
            // replace only that placeholder with the real-mux test peer.
            let removed = sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1 AND status='stopped' AND vm_id IS NULL AND host_id IS NULL")
                .bind(child.id).execute(&f.app.db).await.unwrap();
            assert_eq!(removed.rows_affected(), 1);
            let runtime =
                helpers::TestWorkspaceRuntimePeer::attach(&f.app, &child, f.owner.id).await;
            f.reconcile().await;
            let report = f.report(review).await;
            assert_eq!(
                report["stage"], "start",
                "{provider}, configured={configured}: {report}"
            );
            let spec: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id=(SELECT task_id FROM agent_approval_reviews WHERE id=$1)")
                .bind(review).fetch_one(&f.app.db).await.unwrap();
            assert_eq!(
                spec["agent"]["agentLaunch"]["permissionMode"],
                json!({"source":"explicit","value":mode}),
                "{spec}"
            );
            assert_eq!(spec["agent"]["formalReview"], report["assignment"]);
            // Codex reviews run only over App Server (`g5.agent-approval.codex-launch`);
            // a review VM started before that module refuses rather than using exec.
            // A Claude review VM whose catalog has only the schema 5 modules
            // refuses a schema 6 review (`g5.agent-approval.contract-rollout`).
            let stale = configured;
            if stale {
                sqlx::query(
                    "UPDATE workspace_vms SET module_catalog_version=$2 WHERE workspace_id=$1",
                )
                .bind(child.id)
                .bind(if provider == "codex" {
                    "2026.10.27-merge-base-preview-origin"
                } else {
                    "2026.10.30-agent-turn-question-gate"
                })
                .execute(&f.app.db)
                .await
                .unwrap();
            }
            let mut db =
                g5_api::db::scoped::scoped_db_for_system_org(&f.app.state, f.org.id, &["*"])
                    .await
                    .unwrap();
            let dispatch = f
                .app
                .state
                .task_service
                .reserve_agent_dispatch(&mut db)
                .await
                .unwrap();
            db.commit().await.unwrap();
            if stale {
                assert!(dispatch.is_none(), "a stale review VM received {provider}");
                let detail: String = sqlx::query_scalar("SELECT outcome->>'detail' FROM tasks WHERE id=(SELECT task_id FROM agent_approval_reviews WHERE id=$1)")
                    .bind(review).fetch_one(&f.app.db).await.unwrap();
                assert!(detail.contains("Retry the review"), "{detail}");
            } else {
                let dispatch = dispatch.expect("formal task is dispatchable");
                let (version, transport) = if provider == "codex" {
                    ("1.0.14", json!("app_server_stdio_v1"))
                } else {
                    ("1.0.13", Value::Null)
                };
                assert_eq!(dispatch.invoke_params["version"], version, "{provider}");
                assert_eq!(
                    dispatch.invoke_params["input"]["provider"]["options"]["codex_transport"],
                    transport,
                    "{provider}"
                );
            }
            drop(runtime);
            f.app.cleanup().await;
        }
    }
}

#[tokio::test]
async fn the_review_vm_never_mounts_the_owners_personal_repository() {
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
    // Every ordinary workspace of this owner now mounts it (g5.personal.mount).
    let me = f
        .app
        .authenticated_client(&f.owner)
        .get(&format!("/api/v1/orgs/{}/me", f.org.id))
        .await;
    assert_eq!(me.status(), StatusCode::OK);
    // A member at the workspace root would overlap the mount and leave it out.
    sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id IN ($1,$2)")
        .bind(f.source.id)
        .bind(f.target)
        .execute(&f.app.db)
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
    f.reconcile().await; // capture and checked fork
    f.reconcile().await; // Start
    let commands = host.received_host_command_params("vm.start").await;
    assert_eq!(commands.len(), 1, "{commands:?}");
    let spec = &commands[0].get("command").unwrap_or(&commands[0])["spec"];
    assert_eq!(spec["repos"].as_array().unwrap().len(), 1, "{spec}");
    // Execution reads only the reviewed content (g5.agent-approval.execution).
    assert!(
        spec.get("reference_repos").is_none(),
        "a review VM has no personal mount: {spec}"
    );
    assert_eq!(f.report(review).await["stage"], "start");
    drop(host);
    f.app.cleanup().await;
}

#[tokio::test]
async fn formal_readiness_replays_one_capacity_intent_and_cancellation_disposes_only_its_workspace()
{
    let f = Fixture::new().await;
    g5_api::services::worker_host_service::WorkerHostService::new()
        .reconcile_unconfigured_org_defaults(
            &f.app.db,
            "local-dev",
            &format!(
                "mirror.gcr.io/library/buildpack-deps@sha256:{}",
                "a".repeat(64)
            ),
        )
        .await
        .unwrap();
    let review = f.request(None).await;
    f.reconcile().await;
    let child = f.child(review).await;
    f.reconcile().await;
    f.reconcile().await;
    let report = f.report(review).await;
    assert_eq!(report["stage"], "start", "{report}");
    let intent: Value =
        sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
            .bind(child)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(intent["id"], review.to_string(), "{intent}");
    let unrelated: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id IN ($1,$2)")
            .bind(f.source.id)
            .bind(f.target)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(unrelated, 0);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE workspace_id=$1")
        .bind(child)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(tasks, 0, "capacity wait cannot start the model");
    let cancel = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/approvals/requests/{}/cancel",
                f.source.id,
                f.request_id(review).await
            ),
            &json!({}),
        )
        .await;
    assert_eq!(cancel.status(), StatusCode::OK);
    f.reconcile().await;
    f.reconcile().await; // cleanup is an independent due pass
    let report = f.report(review).await;
    assert_eq!(report["stage"], "cancelled");
    assert_eq!(report["cleanupPending"], false, "{report}");
    assert!(report["result"].is_null());
    assert_eq!(f.source.git_head(&f.app), f.source.head_commit);
    f.app.cleanup().await;
}

#[tokio::test]
async fn a_failed_readiness_names_its_cause_and_leaves_retry_available() {
    // G5-Lean#1048: each pass replays the review's one Start intent. Once a
    // Stop, expiry or failed boot has ended it, the review fails with that
    // cause, not the replay's raw refusal; a deadline reached while the
    // workspace still waits names that wait, and a replay never overwrites
    // a Start that replaced the review's. A frozen retry is admitted.
    for (case, expected) in [
        ("stop", "stopped before it was ready"),
        ("expiry", "no host had a free slot"),
        ("boot", "failed to boot"),
        ("deadline", "no VM became available"),
        ("replaced", "another start replaced"),
    ] {
        let f = Fixture::new().await;
        g5_api::services::worker_host_service::WorkerHostService::new()
            .reconcile_unconfigured_org_defaults(
                &f.app.db,
                "local-dev",
                &format!(
                    "mirror.gcr.io/library/buildpack-deps@sha256:{}",
                    "a".repeat(64)
                ),
            )
            .await
            .unwrap();
        let review = f.request(None).await;
        f.reconcile().await;
        let child = f.child(review).await;
        f.reconcile().await;
        let report = f.report(review).await;
        assert_eq!(report["stage"], "start", "{case}: {report}");
        let status: String = sqlx::query_scalar(
            "SELECT start_intent ->> 'status' FROM workspace_vms WHERE workspace_id=$1",
        )
        .bind(child)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
        assert_eq!(status, "queued_for_capacity", "{case}");
        match case {
            "stop" => {
                // The platform context the reconciler uses; RLS hides the
                // review workspace from ordinary system scope.
                let db =
                    g5_api::db::scoped::scoped_db_for_system_org(&f.app.state, f.org.id, &["*"])
                        .await
                        .unwrap();
                let scope = db.reopener();
                db.rollback().await.unwrap();
                g5_api::services::WorkspaceVmService::new(
                    g5_api::services::WorkspaceVmServiceConfig {
                        system_pool: f.app.state.system_pool.clone(),
                        scope,
                        hosts: f.app.state.preview_tunnels.clone(),
                        minter: f.app.state.jwt_minter.clone().unwrap(),
                        runtime: f.app.state.workspace_runtime.clone(),
                        agent_work: f.app.state.agent_work.clone(),
                        repo_storage: f.app.state.repo_storage.clone(),
                        git_external_url: f.app.state.git_external_url.clone(),
                        ws_hub: f.app.state.event_bus.ws_hub().clone(),
                    },
                )
                .stop_for_capacity(
                    child,
                    Uuid::new_v4(),
                    chrono::Utc::now() + chrono::TimeDelta::minutes(5),
                )
                .await
                .unwrap();
            }
            "expiry" => {
                // The expiry sweep's write; an active intent cannot carry a
                // lapsed expiry, so it only ever reads this way once swept.
                sqlx::query("UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"expired\"') WHERE workspace_id=$1")
                    .bind(child).execute(&f.app.db).await.unwrap();
            }
            "replaced" => {
                // Another Start's settled intent; the replay must not
                // overwrite it with the review's. The fence lets an intent be
                // replaced only once it has settled.
                for statement in [
                    "UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{status}','\"cancelled\"') WHERE workspace_id=$1",
                    "UPDATE workspace_vms SET start_intent=jsonb_set(start_intent,'{id}',to_jsonb(gen_random_uuid()::text)) WHERE workspace_id=$1",
                ] {
                    sqlx::query(statement)
                        .bind(child)
                        .execute(&f.app.db)
                        .await
                        .unwrap();
                }
            }
            "deadline" => {
                // The readiness budget runs out while the workspace still
                // waits for capacity; its consent is live, not ended.
                sqlx::query("UPDATE agent_approval_reviews SET deadline_at=now()-interval '1 second' WHERE id=$1")
                    .bind(review).execute(&f.app.db).await.unwrap();
            }
            _ => {
                sqlx::query("UPDATE workspace_vms SET status='failed',start_intent=jsonb_set(start_intent,'{status}','\"failed\"') WHERE workspace_id=$1")
                    .bind(child).execute(&f.app.db).await.unwrap();
            }
        }
        let intent: Value =
            sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
                .bind(child)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(
            intent["id"] == review.to_string(),
            case != "replaced",
            "{case}: {intent}"
        );
        f.reconcile().await;
        // The failing pass leaves the recorded intent as it found it, so a
        // replay can neither revive the review's nor overwrite a replacement.
        let replayed: Value =
            sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
                .bind(child)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(replayed, intent, "{case}");
        f.reconcile().await; // cleanup is an independent due pass
        let failed = f.report(review).await;
        assert_eq!(failed["stage"], "failed", "{case}: {failed}");
        assert_eq!(failed["failureStage"], "start", "{case}: {failed}");
        let cause = failed["failureCause"].as_str().unwrap();
        assert!(cause.contains(expected), "{case}: {cause}");
        assert!(!cause.contains("Conflict"), "{case}: {cause}");
        let intent: Value =
            sqlx::query_scalar("SELECT start_intent FROM workspace_vms WHERE workspace_id=$1")
                .bind(child)
                .fetch_optional(&f.app.db)
                .await
                .unwrap()
                .unwrap_or(Value::Null);
        assert!(
            !["pending", "queued_for_capacity", "starting"]
                .contains(&intent["status"].as_str().unwrap_or_default()),
            "{case}: the ended consent stays ended: {intent}"
        );
        let retried = f.request(Some(review)).await;
        assert_ne!(retried, review);
        f.reconcile().await;
        let frozen = f.report(retried).await;
        assert_eq!(frozen["stage"], "start", "{case}: {frozen}");
        assert_eq!(frozen["assignment"], failed["assignment"]);
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn a_detached_fork_cannot_publish_after_its_capture_deadline() {
    let f = Fixture::with_connections(12).await;
    let review = f.request(None).await;
    let child = f.child(review).await;
    let target_branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(f.target)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    // Freeze the known target cut and gate the detached resumer after fork
    // admission. Admission now takes a source SHARE lock to exclude deletion;
    // let it commit before blocking finalization's separate source lock.
    sqlx::query("UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),target_heads=$2,deadline_at=now()+interval '5 seconds' WHERE id=$1")
        .bind(review)
        .bind(json!([{"repoId":f.source.repo_id,"headCommit":f.source.head_commit,"branchName":target_branch,"workspaceId":f.target}]))
        .execute(&f.app.db).await.unwrap();
    let mut resume_hold = f.app.db.begin().await.unwrap();
    let resume_holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *resume_hold)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('g5:workspace-fork-resume:' || $1::text, 0))")
        .bind(child)
        .execute(&mut *resume_hold)
        .await
        .unwrap();
    let state = f.app.state.clone();
    let work = tokio::spawn(async move {
        g5_api::services::approval_review_service::workflow::run_once(&state).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(resume_holder)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fork admission must commit before the detached resumer waits");
    let mut hold = f.app.db.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR NO KEY UPDATE")
        .bind(f.source.id)
        .execute(&mut *hold)
        .await
        .unwrap();
    resume_hold.rollback().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) AND query LIKE '%FOR SHARE%')")
                .bind(holder).fetch_one(&f.app.db).await.unwrap();
            if blocked { break; }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.expect("detached finalizer must reach the source lock before timeout");
    let source_path = f.app.repo_path(f.source.repo_id).await;
    let has_proof = || {
        let repo = git2::Repository::open(&source_path).unwrap();
        let exists = repo
            .find_reference(&format!("refs/g5/forks/{child}"))
            .is_ok();
        exists
    };
    assert!(has_proof());
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let stage: String =
                sqlx::query_scalar("SELECT stage FROM agent_approval_reviews WHERE id=$1")
                    .bind(review)
                    .fetch_one(&f.app.db)
                    .await
                    .unwrap();
            if stage == "failed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("coordinator must expire capture while its finalizer is detached");
    hold.rollback().await.unwrap();
    work.await.unwrap().unwrap();
    // Resume independently in case cleanup and the detached abort crossed.
    sqlx::query("UPDATE agent_approval_reviews SET cleanup_due_at=now() WHERE id=$1")
        .bind(review)
        .execute(&f.app.db)
        .await
        .unwrap();
    f.reconcile().await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id=$1")
        .bind(child)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "late finalization must never publish an ordinary child"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM workspace_forks WHERE id=$1")
        .bind(child)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(status, "aborted");
    let report = f.report(review).await;
    assert_eq!(report["stage"], "failed");
    assert_eq!(report["failureStage"], "capture");
    assert_eq!(report["cleanupPending"], false, "{report}");
    assert!(!has_proof());
    f.app.cleanup().await;
}

#[tokio::test]
async fn a_blocked_capture_does_not_delay_another_reviews_deadline() {
    let f = Fixture::with_connections(12).await;
    let blocked = f.request(None).await;
    let other = TestWorkspace::create(&f.app, &f.org).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(other.id)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/approvals/reviews", other.id),
            &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let expired: Uuid = body["data"]["reviewId"].as_str().unwrap().parse().unwrap();
    sqlx::query(
        "UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(expired)
    .execute(&f.app.db)
    .await
    .unwrap();
    let mut hold = f.app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(f.source.id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let state = f.app.state.clone();
    let work = tokio::spawn(async move {
        g5_api::services::approval_review_service::workflow::run_once(&state).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let stage: String =
                sqlx::query_scalar("SELECT stage FROM agent_approval_reviews WHERE id=$1")
                    .bind(expired)
                    .fetch_one(&f.app.db)
                    .await
                    .unwrap();
            if stage == "failed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("unrelated deadline must advance while capture is blocked");
    assert!(!work.is_finished(), "first capture is still held");
    hold.rollback().await.unwrap();
    work.await.unwrap().unwrap();
    assert_eq!(f.report(blocked).await["stage"], "start");
    let failure: String =
        sqlx::query_scalar("SELECT failure_stage FROM agent_approval_reviews WHERE id=$1")
            .bind(expired)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(failure, "capture");
    f.app.cleanup().await;
}

/// Exercise the production tick loop: a review admitted after another capture
/// has blocked must be discovered before that first owner's operation returns.
#[tokio::test]
async fn the_live_coordinator_captures_new_reviews_while_an_owner_operation_is_blocked() {
    let f = Fixture::with_connections(12).await;
    // A checked clone supplies both the repository set and saved base.
    let other = f.fork_source().await.0;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(other)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let blocked = f.request(None).await;
    sqlx::query(
        "UPDATE agent_approval_reviews SET deadline_at=now()+interval '15 seconds' WHERE id=$1",
    )
    .bind(blocked)
    .execute(&f.app.db)
    .await
    .unwrap();
    let mut hold = f.app.db.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(f.source.id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let coordinator = g5_api::services::approval_review_service::workflow::spawn(
        f.app.state.clone(),
        cancel.clone(),
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the first capture must enter its held owner operation");
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{other}/approvals/reviews"),
            &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let later: Uuid = body["data"]["reviewId"].as_str().unwrap().parse().unwrap();
    let progressed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let ready: bool = sqlx::query_scalar(
                "SELECT stage='start' AND target_heads IS NOT NULL FROM agent_approval_reviews WHERE id=$1",
            )
            .bind(later)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
            if ready {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    let observed: (String, bool) = sqlx::query_as(
        "SELECT stage,target_heads IS NOT NULL FROM agent_approval_reviews WHERE id=$1",
    )
    .bind(later)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    cancel.cancel();
    hold.rollback().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), coordinator)
        .await
        .expect("coordinator stops after the held operation is released")
        .unwrap();
    f.app.cleanup().await;
    assert!(
        progressed.is_ok(),
        "a later review was never captured while another owner was blocked: {observed:?}"
    );
}

#[tokio::test]
async fn failed_attempts_leave_one_pending_request_and_overlap_preserves_the_active_attempt() {
    use g5_contracts::types::AgentApprovalOutcome;
    let f = Fixture::new().await;
    f.settle(AgentApprovalOutcome::Approved).await;
    let gate = f.gate().await;
    assert_eq!(gate["state"], "approved");
    assert_eq!(gate["decisionLog"][0]["actorType"], "agent");
    assert_eq!(gate["decisionLog"][0]["superseded"], false);
    let first = f.request(None).await;
    let request = f.request_id(first).await;
    f.reconcile().await;
    let assignment = f.report(first).await["assignment"].clone();
    f.fail_deadline(first).await;
    let pending: String = sqlx::query_scalar("SELECT state FROM approval_requests WHERE id=$1")
        .bind(request)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(pending, "pending");
    assert_eq!(f.gate().await["state"], "approved");
    let retry = f.request(Some(first)).await;
    assert_ne!(retry, first);
    assert_eq!(f.request_id(retry).await, request);
    let (status, duplicate) = f.request_response(Some(first)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate["data"]["reused"], true);
    assert_eq!(duplicate["data"]["reviewId"], retry.to_string());
    let (status, error) = f.request_response(None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    f.reconcile().await;
    assert_eq!(f.report(retry).await["assignment"], assignment);
    f.fail_deadline(retry).await;
    let current = f.request(None).await;
    assert_eq!(f.request_id(current).await, request);
    assert_ne!(current, retry);
    let (status, duplicate) = f.request_response(None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate["data"]["reviewId"], current.to_string());
    let (status, error) = f.request_response(Some(first)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    let attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_approval_reviews WHERE approval_request_id=$1",
    )
    .bind(request)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(attempts, 3, "overlap must never create another attempt");
    f.reconcile().await;
    f.terminal_receipt(current, AgentApprovalOutcome::Rejected)
        .await;
    f.reconcile().await;
    f.reconcile().await;
    let terminal = f.report(current).await;
    let ended = terminal["endedAt"].clone();
    assert!(ended.is_string(), "{terminal}");
    assert_eq!(f.gate().await["state"], "rejected");
    let settled: String = sqlx::query_scalar("SELECT state FROM approval_requests WHERE id=$1")
        .bind(request)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    assert_eq!(settled, "settled");
    f.reconcile().await;
    assert_eq!(f.report(current).await["endedAt"], ended);
    let changed = sqlx::query(
        "UPDATE agent_approval_reviews SET ended_at=ended_at+interval '1 second' WHERE id=$1",
    )
    .bind(current)
    .execute(&f.app.db)
    .await
    .unwrap_err();
    assert_eq!(
        changed.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    f.app.cleanup().await;
}

#[tokio::test]
async fn current_inputs_after_a_failure_capture_the_moved_cut_under_the_same_request() {
    use g5_contracts::types::AgentApprovalOutcome;
    let mut f = Fixture::new().await;
    f.settle(AgentApprovalOutcome::Approved).await;
    let first = f.request(None).await;
    let request = f.request_id(first).await;
    f.reconcile().await;
    let original = f.report(first).await["assignment"].clone();
    f.fail_deadline(first).await;
    let advanced = f
        .source
        .commit_file(&f.app, "moved.txt", "fresh pushed cut\n")
        .await;
    // Under IGNORE the earlier effective verdict keeps counting after the
    // move (g5.governance.approval.source-commit-policy); only currentness
    // reports the changed cut.
    let moved = f.gate().await;
    assert_eq!(moved["state"], "approved");
    assert_eq!(moved["decisionLog"][0]["superseded"], false);
    assert_eq!(moved["currentness"]["state"], "changed");
    let current = f.request(None).await;
    assert_eq!(f.request_id(current).await, request);
    f.reconcile().await;
    let captured = f.report(current).await;
    assert_eq!(
        captured["assignment"]["resolution"]["sourceHeads"][0]["headCommit"],
        advanced
    );
    assert_ne!(
        captured["assignment"]["resolution"]["resolutionId"],
        original["resolution"]["resolutionId"]
    );
    f.app.cleanup().await;
}

#[tokio::test]
async fn unclaimed_capture_time_is_not_spent_while_all_owner_slots_are_busy() {
    let f = Fixture::with_connections(12).await;
    let mut sources = vec![f.source.id];
    for _ in 0..3 {
        sources.push(f.fork_source().await.0);
    }
    let waiting_source = f.fork_source().await.0;
    for source in sources.iter().chain(std::iter::once(&waiting_source)) {
        sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
            .bind(source)
            .bind(f.target)
            .execute(&f.app.db)
            .await
            .unwrap();
    }
    let mut reviews = Vec::new();
    for source in &sources {
        let response = f
            .app
            .authenticated_client(&f.owner)
            .post_json(
                &format!("/api/v1/workspaces/{source}/approvals/reviews"),
                &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.unwrap();
        reviews.push(
            body["data"]["reviewId"]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        );
    }
    let mut hold = f.app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=ANY($1) FOR UPDATE")
        .bind(&sources)
        .execute(&mut *hold)
        .await
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let coordinator = g5_api::services::approval_review_service::workflow::spawn(
        f.app.state.clone(),
        cancel.clone(),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_approval_reviews WHERE id=ANY($1) AND claimed_at IS NOT NULL")
                .bind(&reviews).fetch_one(&f.app.db).await.unwrap();
            if count == 4 { break; }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.expect("all four owner slots must have started their blocked captures");
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{waiting_source}/approvals/reviews"),
            &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let waiting: Uuid = body["data"]["reviewId"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE agent_approval_reviews SET deadline_at=now()-interval '1 second' WHERE id=$1 AND claimed_at IS NULL")
        .bind(waiting).execute(&f.app.db).await.unwrap();
    let released_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    hold.rollback().await.unwrap();
    let progressed = tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let row: (String, Option<chrono::DateTime<chrono::Utc>>) =
                sqlx::query_as("SELECT stage,claimed_at FROM agent_approval_reviews WHERE id=$1")
                    .bind(waiting)
                    .fetch_one(&f.app.db)
                    .await
                    .unwrap();
            if row.0 == "start" {
                assert!(row.1.unwrap() >= released_at);
                break;
            }
            assert_ne!(
                row.0, "failed",
                "unclaimed waiting time must not expire capture"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    cancel.cancel();
    coordinator.await.unwrap();
    f.app.cleanup().await;
    progressed
        .expect("the queued review gets its own capture budget when a slot becomes available");
}

/// The summary a schema 5 test receipt authors beside its body.
pub(crate) const TEST_AUTHORED_SUMMARY: &str =
    "Reviewed the frozen comparison: the exact evidence satisfies every required criterion.";

async fn terminal_receipt_for(
    app: &TestApp,
    org: Uuid,
    owner: Uuid,
    review: Uuid,
    verdict: g5_contracts::types::AgentApprovalOutcome,
) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let (assignment, child): (Value, Uuid) = sqlx::query_as(
        "SELECT invocation,review_workspace_id FROM agent_approval_reviews WHERE id=$1",
    )
    .bind(review)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let invocation: g5_contracts::types::FormalApprovalInvocation =
        serde_json::from_value(assignment).unwrap();
    let document = if invocation.resolution.has_authored_summary() {
        g5_contracts::types::render_agent_approval_report_v5(
            &invocation.resolution,
            verdict,
            TEST_AUTHORED_SUMMARY,
            "## Review findings\n\nExact original evidence: é.",
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        )
        .unwrap()
    } else if invocation.resolution.schema_version == 4 {
        g5_contracts::types::render_agent_approval_report_v4(
            &invocation.resolution,
            verdict,
            "## Review findings\n\nExact original evidence: é.",
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        )
        .unwrap()
    } else {
        format!(
            "---\n{}\n---\n\n## Review findings\n\nExact original evidence: é.\n",
            g5_contracts::types::render_agent_approval_frontmatter_v2(
                &invocation.resolution,
                verdict
            )
            .unwrap()
        )
    };
    let session = Uuid::new_v4();
    let task = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(session)
        .bind(org)
        .bind(child)
        .bind(owner)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by,chat_session_id) VALUES($1,$2,$3,'Formal receipt','agent','succeeded',$4,$5)").bind(task).bind(org).bind(child).bind(owner).bind(session).execute(&app.db).await.unwrap();
    let output = json!({"result":{"approval_artifact":{"contractVersion":invocation.contract_version,"resolutionId":invocation.resolution.resolution_id,"contentBase64":base64::engine::general_purpose::STANDARD.encode(document.as_bytes()),"sha256":format!("{:x}",Sha256::digest(document.as_bytes()))}}});
    sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt,ended_at,reason,outcome,policy_snapshot_ref) VALUES($1,$2,1,now(),'exit_code',$3,$4)").bind(org).bind(task).bind(output).bind(json!({"agent_launch":{"resolved":{"provider":{"value":"claude_code"}}}})).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE agent_approval_reviews SET task_id=$2,stage='execute' WHERE id=$1")
        .bind(review)
        .bind(task)
        .execute(&app.db)
        .await
        .unwrap();
    g5_api::services::approval_review_service::workflow::run_once(&app.state)
        .await
        .unwrap();
    document
}

pub(crate) async fn settle_for_quorum(
    app: &TestApp,
    owner: &TestUser,
    source: &TestWorkspace,
    definition: Uuid,
    outcome: &str,
) {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/approvals/reviews", source.id),
            &json!({"definitionId":definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{status} {body}");
    let review: Uuid = body["data"]["reviewId"].as_str().unwrap().parse().unwrap();
    let reconcile = || g5_api::services::approval_review_service::workflow::run_once(&app.state);
    reconcile().await.unwrap();
    if outcome == "deferred" {
        sqlx::query("UPDATE agent_approval_reviews SET claimed_at=COALESCE(claimed_at,now()),deadline_at=now()-interval '1 second' WHERE id=$1")
            .bind(review).execute(&app.db).await.unwrap();
    } else {
        terminal_receipt_for(
            app,
            source.org_id,
            owner.id,
            review,
            if outcome == "approved" {
                g5_contracts::types::AgentApprovalOutcome::Approved
            } else {
                g5_contracts::types::AgentApprovalOutcome::Rejected
            },
        )
        .await;
    }
    for _ in 0..3 {
        reconcile().await.unwrap();
    }
    let (stage, result): (String, Option<String>) =
        sqlx::query_as("SELECT stage,result FROM agent_approval_reviews WHERE id=$1")
            .bind(review)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        stage,
        if outcome == "deferred" {
            "failed"
        } else {
            "effective"
        }
    );
    assert_eq!(
        result.as_deref(),
        if outcome == "deferred" {
            None
        } else {
            Some(outcome)
        }
    );
}

#[tokio::test]
async fn legacy_agent_request_keeps_its_identity_across_current_and_frozen_attempts() {
    let f = Fixture::with_connections(8).await;
    let (request, key) = legacy_agent_request(&f).await;
    let first = f.request(None).await;
    assert_eq!(f.request_id(first).await, request);
    f.reconcile().await;
    let assignment = f.report(first).await["assignment"].clone();
    f.fail_deadline(first).await;
    let retry = f.request(Some(first)).await;
    assert_eq!(f.request_id(retry).await, request);
    let replay = f.request(Some(first)).await;
    assert_eq!(retry, replay, "identical active retry reuses its attempt");
    let (status, _) = f.request_response(None).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "current inputs cannot replace an active frozen retry"
    );
    f.reconcile().await;
    assert_eq!(f.report(retry).await["assignment"], assignment);
    let identity: (String, String) =
        sqlx::query_as("SELECT request_key,phase FROM approval_requests WHERE id=$1")
            .bind(request)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(identity, (key, "be".into()));
    // Race the two public entry points against the same legacy slot. One
    // attempt wins; the other must refuse without replacing its assignment.
    f.fail_deadline(retry).await;
    let (frozen, current) =
        tokio::join!(f.request_response(Some(retry)), f.request_response(None),);
    let outcomes = [frozen, current];
    assert_eq!(
        outcomes
            .iter()
            .filter(|(status, _)| status.is_success())
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|(status, _)| *status == StatusCode::CONFLICT)
            .count(),
        1
    );
    let accepted = outcomes
        .iter()
        .find(|(status, _)| status.is_success())
        .unwrap();
    assert_eq!(accepted.1["data"]["requestId"], request.to_string());
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_approval_reviews WHERE approval_request_id=$1 AND stage NOT IN ('effective','failed','cancelled')")
        .bind(request).fetch_one(&f.app.db).await.unwrap();
    assert_eq!(active, 1);
    f.app.cleanup().await;
}

async fn legacy_agent_request(f: &Fixture) -> (Uuid, String) {
    let key = g5_contracts::types::ApprovalRequestKeyV2 {
        schema_version: g5_contracts::types::APPROVAL_REQUEST_KEY_SCHEMA_VERSION,
        org_id: f.org.id,
        source_workspace_id: f.source.id,
        approval_definition_id: f.definition,
        approval_definition_version: 1,
        phase: "be".into(),
        actor_slot: "agent".into(),
    }
    .digest()
    .unwrap();
    let request: Uuid = sqlx::query_scalar("INSERT INTO approval_requests(org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,requested_by,phase) VALUES($1,$2,$3,1,$4,'agent',$5,'be') RETURNING id")
        .bind(f.org.id).bind(f.source.id).bind(f.definition).bind(&key).bind(f.owner.id)
        .fetch_one(&f.app.db).await.unwrap();
    (request, key)
}

#[tokio::test]
async fn legacy_admission_holds_its_durable_commit_confirmation_fence() {
    use g5_api::services::approval_review_service::CreateApprovalReviewRequest;
    let f = Fixture::with_connections(8).await;
    let (request, key) = legacy_agent_request(&f).await;
    let (writer, admitted) = f
        .app
        .state
        .approval_review_service
        .admit(
            f.app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
            &f.app.state.system_pool,
            f.source.id,
            f.owner.id,
            CreateApprovalReviewRequest {
                client_request_id: None,
                definition_id: f.definition,
                definition_version: 1,
                agent_launch: None,
                retry_review_id: None,
                phase: "ie".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(admitted.request_id, request);
    assert_eq!(admitted.request_key(), key);
    let mut probe = f.app.scoped_db_for_org(f.org.id).await;
    let acquired: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1::text,0))")
            .bind(&key)
            .fetch_one(probe.conn())
            .await
            .unwrap();
    probe.rollback().await.unwrap();
    assert!(
        !acquired,
        "commit confirmation must wait for this admission's transaction"
    );
    writer.commit().await.unwrap();
    let mut confirmation = f.app.scoped_db_for_org(f.org.id).await;
    let recovered = f
        .app
        .state
        .approval_review_service
        .confirm_admission_after_commit_error(&mut confirmation, f.source.id, &admitted)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.review_id, admitted.review_id);
    confirmation.rollback().await.unwrap();
    f.app.cleanup().await;
}

#[tokio::test]
async fn reused_operation_commit_confirmation_requires_its_durable_input_binding() {
    use g5_api::services::approval_review_service::CreateApprovalReviewRequest;
    let f = Fixture::new().await;
    let existing = f.request(None).await;
    let operation = Uuid::new_v4();
    let mut rolled_back = None;
    for (phase, commit) in [("ie", false), ("be", true)] {
        let (writer, admitted) = f
            .app
            .state
            .approval_review_service
            .admit(
                f.app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
                &f.app.state.system_pool,
                f.source.id,
                f.owner.id,
                CreateApprovalReviewRequest {
                    client_request_id: Some(operation),
                    definition_id: f.definition,
                    definition_version: 1,
                    agent_launch: None,
                    retry_review_id: None,
                    phase: phase.into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(admitted.review_id, existing);
        assert!(admitted.reused);
        if commit {
            writer.commit().await.unwrap();
        } else {
            writer.rollback().await.unwrap();
        }
        let mut confirmation = f.app.scoped_db_for_org(f.org.id).await;
        let recovered = f
            .app
            .state
            .approval_review_service
            .confirm_admission_after_commit_error(&mut confirmation, f.source.id, &admitted)
            .await
            .unwrap();
        assert_eq!(
            recovered.is_some(),
            commit,
            "an existing run alone cannot confirm a new alias"
        );
        confirmation.rollback().await.unwrap();
        if !commit {
            rolled_back = Some(admitted);
        }
    }
    // Legacy phase names normalize to the same workspace-wide operation.
    // Once that exact alias commits, the original receipt is confirmable too.
    let mut confirmation = f.app.scoped_db_for_org(f.org.id).await;
    assert!(f
        .app
        .state
        .approval_review_service
        .confirm_admission_after_commit_error(&mut confirmation, f.source.id, &rolled_back.unwrap())
        .await
        .unwrap()
        .is_some());
    confirmation.rollback().await.unwrap();
    f.app.cleanup().await;
}

#[tokio::test]
async fn operation_replay_retains_terminal_receipts_and_aliases_without_another_paid_run() {
    use g5_contracts::types::AgentApprovalOutcome;
    let f = Fixture::new().await;
    let client = f.app.authenticated_client(&f.owner);
    let url = format!("/api/v1/workspaces/{}/approvals/reviews", f.source.id);
    let operation = Uuid::new_v4();
    let body =
        json!({"clientRequestId":operation,"definitionId":f.definition,"definitionVersion":1});
    // Two identical concurrent requests, then another operation alias of the
    // still-active run. All must survive terminal settlement as one receipt.
    let (a, b) = tokio::join!(client.post_json(&url, &body), client.post_json(&url, &body));
    assert!(a.status().is_success());
    assert!(b.status().is_success());
    let a: Value = a.json().await.unwrap();
    let b: Value = b.json().await.unwrap();
    assert_eq!(a["data"]["reviewId"], b["data"]["reviewId"]);
    let review: Uuid = a["data"]["reviewId"].as_str().unwrap().parse().unwrap();
    let alias =
        json!({"clientRequestId":Uuid::new_v4(),"definitionId":f.definition,"definitionVersion":1});
    let response = client.post_json(&url, &alias).await;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await.unwrap();
    assert_eq!(receipt["data"]["reviewId"], a["data"]["reviewId"]);
    f.reconcile().await;
    f.terminal_receipt(review, AgentApprovalOutcome::Approved)
        .await;
    f.reconcile().await;
    f.reconcile().await;
    assert_eq!(f.report(review).await["stage"], "effective");
    for input in [&body, &alias] {
        let response = client.post_json(&url, input).await;
        assert_eq!(response.status(), StatusCode::OK);
        let replay: Value = response.json().await.unwrap();
        assert_eq!(replay["data"]["reviewId"], a["data"]["reviewId"]);
        assert_eq!(replay["data"]["status"], "settled");
    }
    let gate = f.gate().await;
    assert_eq!(gate["decisionLog"][0]["agentReviewId"], review.to_string());
    assert_eq!(gate["decisionLog"][0]["requestId"], a["data"]["requestId"]);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_approval_reviews WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    // An operation ID cannot be repurposed for a new definition/assignment.
    let mut unequal = body.clone();
    unequal["definitionVersion"] = json!(2);
    assert_eq!(
        client.post_json(&url, &unequal).await.status(),
        StatusCode::CONFLICT
    );
    let fresh=client.post_json(&url,&json!({"clientRequestId":Uuid::new_v4(),"definitionId":f.definition,"definitionVersion":1})).await;
    assert_eq!(fresh.status(), StatusCode::CREATED);
    let fresh: Value = fresh.json().await.unwrap();
    assert_ne!(fresh["data"]["reviewId"], a["data"]["reviewId"]);
    f.app.cleanup().await;
}

#[tokio::test]
async fn operation_replay_after_failure_does_not_retry_execution() {
    let f = Fixture::new().await;
    let client = f.app.authenticated_client(&f.owner);
    let url = format!("/api/v1/workspaces/{}/approvals/reviews", f.source.id);
    let body =
        json!({"clientRequestId":Uuid::new_v4(),"definitionId":f.definition,"definitionVersion":1});
    let response = client.post_json(&url, &body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: Value = response.json().await.unwrap();
    let review: Uuid = receipt["data"]["reviewId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    f.fail_deadline(review).await;
    let response = client.post_json(&url, &body).await;
    assert_eq!(response.status(), StatusCode::OK);
    let replay: Value = response.json().await.unwrap();
    assert_eq!(replay["data"]["reviewId"], receipt["data"]["reviewId"]);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_approval_reviews WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    f.app.cleanup().await;
}

/// Full production SPA + real HTTP/Git/Postgres. The formal execution's
/// terminal artifact is supplied through the existing coordinator fixture;
/// VM/provider qualification remains in the Cloud E2E lane.
#[tokio::test]
#[ignore = "requires installed frontend dependencies and Playwright Chromium"]
async fn unified_approvals_native_browser_journey() {
    use helpers::review_flow::Flow;
    let mut config = helpers::test_config();
    // This fixture serves plain HTTP and installs the development refresh cookie.
    config.auth.cookie_secure = false;
    config
        .cors
        .allowed_origins
        .push("http://127.0.0.1:5174".into());
    let app = TestApp::spawn_with_config(config).await;
    let flow = Flow::new(&app).await;
    let owner_refresh_token = helpers::insert_refresh_token(&app, flow.owner.id).await;
    flow.submit(&app).await;
    sqlx::query("UPDATE users SET display_name='Ben reviewer' WHERE id=$1")
        .bind(flow.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET display_name='Rahul reviewer' WHERE id=$1")
        .bind(flow.second.id)
        .execute(&app.db)
        .await
        .unwrap();
    let client = app.authenticated_client(&flow.owner);
    let response=client.post_json(&format!("/api/v1/orgs/{}/approval-definitions",flow.org.id),&json!({
        "name":"Mixed approval", "humanUserIds":[flow.reviewer.id,flow.second.id],
        "requireAllReviewers":false,"agentType":"simple","agentPrompt":"Review the captured change."
    })).await;
    let mixed = helpers::review_flow::body(response, StatusCode::CREATED).await;
    let definition: Uuid = mixed["id"].as_str().unwrap().parse().unwrap();
    helpers::review_flow::body(
        client
            .post_json(
                &format!(
                    "/api/v1/workspaces/{}/approvals/requirements",
                    flow.parent.id
                ),
                &json!({"definitionId":definition}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let overview = flow.overview(&app, &flow.owner).await;
    // The existing fork owner requires a writer token for the caller's own
    // copy; this reader still has no write authority over the source.
    let visitor_token = helpers::test_jwt_minter()
        .mint(
            flow.visitor.id,
            flow.org.id,
            vec![
                g5_auth::Scope::WorkspacesRead,
                g5_auth::Scope::WorkspacesWrite,
                g5_auth::Scope::TasksRead,
                g5_auth::Scope::TasksExecute,
                g5_auth::Scope::SessionsRead,
                g5_auth::Scope::SessionsWrite,
                g5_auth::Scope::OrgRead,
                g5_auth::Scope::UsersRead,
                g5_auth::Scope::SettingsRead,
                g5_auth::Scope::SettingsWrite,
                g5_auth::Scope::WorkersRead,
            ],
            3600,
        )
        .unwrap();
    let fixture = json!({
        "org":flow.org.slug,"workspace":"review-flow","workspaceId":flow.source,"definitionId":definition,
        "ownerToken":flow.owner.jwt,"ownerRefreshToken":owner_refresh_token,
        "reviewerToken":flow.reviewer.jwt,"visitorToken":visitor_token,
        "reviewerId":flow.reviewer.id,
    });
    let frontend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend");
    let mut vite = tokio::process::Command::new("node")
        .args([
            "node_modules/vite/bin/vite.js",
            "--host",
            "127.0.0.1",
            "--port",
            "5174",
            "--strictPort",
        ])
        .current_dir(&frontend)
        .env("API_URL", &app.base_url)
        .env("VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        assert!(
            vite.try_wait().unwrap().is_none(),
            "Vite exited before readiness"
        );
        if app.client.get("http://127.0.0.1:5174").send().await.is_ok() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "Vite failed to start");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    async fn browser(frontend: &std::path::Path, phase: &str, input: &Value) -> Value {
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::new("node")
            .args(["e2e/approvals-native.mjs", phase])
            .current_dir(frontend)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .await
            .unwrap();
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(180),
            child.wait_with_output(),
        )
        .await
        .expect("native browser deadline")
        .unwrap();
        assert!(
            output.status.success(),
            "native approval browser {phase} failed"
        );
        serde_json::from_slice(&output.stdout).expect("browser result")
    }
    let first = browser(&frontend, "request", &fixture).await;
    let review: Uuid = first["reviewId"].as_str().unwrap().parse().unwrap();
    let f = Fixture {
        app,
        owner: flow.owner,
        org: flow.org,
        source: TestWorkspace {
            id: flow.source,
            org_id: flow.parent.org_id,
            repo_id: flow.parent.repo_id,
            head_commit: overview["heads"][0]["headCommit"]
                .as_str()
                .unwrap()
                .to_owned(),
            name: "review-flow".into(),
        },
        target: flow.parent.id,
        definition,
    };
    f.reconcile().await;
    let (child, name): (Uuid, String) = sqlx::query_as("SELECT w.id,w.name FROM workspaces w JOIN agent_approval_reviews r ON r.review_workspace_id=w.id WHERE r.id=$1").bind(review).fetch_one(&f.app.db).await.unwrap();
    let host = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,slot_size_reported,slot_cpus,slot_mem_mib) VALUES($1,$2,'us-central1','ready',now(),true,4,8192)").bind(host).bind(f.org.id).execute(&f.app.db).await.unwrap();
    sqlx::query("UPDATE workspace_vms SET host_id=$3,vm_id=$4,epoch=1,status='running',started_at=now() WHERE org_id=$1 AND workspace_id=$2").bind(f.org.id).bind(child).bind(host).bind(Uuid::new_v4().to_string()).execute(&f.app.db).await.unwrap();
    let mut inventory = fixture.clone();
    inventory["childId"] = json!(child);
    inventory["childName"] = json!(name);
    browser(&frontend, "inventory", &inventory).await;
    sqlx::query(
        "UPDATE workspace_vms SET status='stopped',host_id=NULL,vm_id=NULL WHERE workspace_id=$1",
    )
    .bind(child)
    .execute(&f.app.db)
    .await
    .unwrap();
    f.terminal_receipt(review, g5_contracts::types::AgentApprovalOutcome::Approved)
        .await;
    f.reconcile().await;
    f.reconcile().await;
    assert_eq!(f.report(review).await["stage"], "effective");
    let mut replay = fixture;
    replay["submission"] = first["submission"].clone();
    replay["reviewId"] = first["reviewId"].clone();
    browser(&frontend, "replay", &replay).await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_approval_reviews WHERE source_workspace_id=$1",
    )
    .bind(f.source.id)
    .fetch_one(&f.app.db)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "settled lost-response replay must not pay for another run"
    );
    vite.kill().await.unwrap();
    f.app.cleanup().await;
}
