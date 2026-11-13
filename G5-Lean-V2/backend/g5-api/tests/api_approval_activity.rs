//! The org-wide approval activity feed (`g5.governance.approval.activity-feed`,
//! G5-Lean#1099): one agent read over every workspace its owner can read.
use crate::helpers::{self, TestApp, TestOrg, TestUser, TestWorkspace};
use chrono::{DateTime, Duration, SubsecRound, Utc};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

struct Feed {
    app: TestApp,
    org: TestOrg,
    /// The caller's own workspace and its agent token.
    caller: TestWorkspace,
    reader: TestUser,
    token: String,
    _peer: helpers::TestWorkspaceRuntimePeer,
}

impl Feed {
    /// An org with an admin owner and a plain member whose agent reads the feed.
    async fn new() -> Self {
        let app = TestApp::spawn().await;
        let mut admin = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut admin).await;
        let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
        let caller = TestWorkspace::create(&app, &org).await;
        set_owner(&app, caller.id, reader.id).await;
        let peer = helpers::TestWorkspaceRuntimePeer::attach(&app, &caller, reader.id).await;
        let token = app
            .state
            .jwt_minter
            .as_ref()
            .unwrap()
            .mint_workspace_incarnation(org.id, caller.id, reader.id, peer.vm_id(), 1, 3600)
            .unwrap();
        Self {
            app,
            org,
            caller,
            reader,
            token,
            _peer: peer,
        }
    }

    async fn workspace(&self, owner: Uuid) -> TestWorkspace {
        let workspace = TestWorkspace::create(&self.app, &self.org).await;
        sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
            .bind(workspace.id)
            .bind(owner)
            .execute(&self.app.db)
            .await
            .unwrap();
        workspace
    }

    async fn definition(&self, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO approval_definitions (id, version, org_id, name, created_by, humans_list) \
             VALUES ($1, 1, $2, $3, $4, ARRAY[$4]::uuid[])",
        )
        .bind(id)
        .bind(self.org.id)
        .bind(name)
        .bind(self.reader.id)
        .execute(&self.app.db)
        .await
        .unwrap();
        id
    }

    #[allow(clippy::too_many_arguments)]
    async fn request(
        &self,
        workspace: Uuid,
        definition: Uuid,
        recipient: Option<Uuid>,
        requester: Uuid,
        state: &str,
        created_at: DateTime<Utc>,
        closed_at: Option<DateTime<Utc>>,
        note: Option<&str>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let actor_type = if recipient.is_some() {
            "human"
        } else {
            "agent"
        };
        // A settled request needs a fingerprint; terminal non-settled states and
        // pending leave it null (chk_approval_request_lifecycle).
        let fingerprint = (state == "settled")
            .then(|| format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()));
        sqlx::query(
            "INSERT INTO approval_requests \
             (id, org_id, source_workspace_id, approval_definition_id, approval_definition_version, \
              request_key, actor_type, actor_user_id, requested_by, state, created_at, closed_at, note, settlement_fingerprint) \
             VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(self.org.id)
        .bind(workspace)
        .bind(definition)
        .bind(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
        .bind(actor_type)
        .bind(recipient)
        .bind(requester)
        .bind(state)
        .bind(created_at)
        .bind(closed_at)
        .bind(note)
        .bind(fingerprint)
        .execute(&self.app.db)
        .await
        .unwrap();
        id
    }

    async fn pending(
        &self,
        workspace: Uuid,
        definition: Uuid,
        recipient: Uuid,
        created_at: DateTime<Utc>,
    ) -> Uuid {
        self.request(
            workspace,
            definition,
            Some(recipient),
            self.reader.id,
            "pending",
            created_at,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn decision(
        &self,
        workspace: Uuid,
        definition: Uuid,
        reviewer: Uuid,
        outcome: &str,
        comment: Option<&str>,
        created_at: DateTime<Utc>,
        request_id: Option<Uuid>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO approval_statuses \
             (id, org_id, source_workspace_id, approval_definition_id, approval_definition_version, \
              outcome, comment, actor_type, actor_user_id, reviewed_git_heads, created_at, request_id, phase) \
             VALUES ($1, $2, $3, $4, 1, $5, $6, 'human', $7, '[]'::jsonb, $8, $9, 'ie')",
        )
        .bind(id)
        .bind(self.org.id)
        .bind(workspace)
        .bind(definition)
        .bind(outcome)
        .bind(comment)
        .bind(reviewer)
        .bind(created_at)
        .bind(request_id)
        .execute(&self.app.db)
        .await
        .unwrap();
        id
    }

    async fn get(&self, query: &str) -> (StatusCode, Value) {
        let response = self
            .app
            .client
            .get(format!(
                "{}/api/internal/workspace-agent/{}/approval-activity?{query}",
                self.app.base_url, self.caller.id
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn make_admin(&self, user: Uuid) {
        sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
            .bind(self.org.id)
            .bind(user)
            .execute(&self.app.db)
            .await
            .unwrap();
    }

    /// Let seed rows keep their explicit `created_at`: the insert-fact triggers
    /// otherwise force it to `now()`. This workspace's database is per-test, so
    /// the change is isolated.
    async fn allow_historical_timestamps(&self) {
        for stmt in [
            "ALTER TABLE approval_requests DISABLE TRIGGER trg_approval_requests_insert_facts",
            "ALTER TABLE approval_statuses DISABLE TRIGGER trg_approval_statuses_created_at",
        ] {
            sqlx::query(stmt).execute(&self.app.db).await.unwrap();
        }
    }
}

async fn set_owner(app: &TestApp, workspace: Uuid, owner: Uuid) {
    sqlx::query("UPDATE workspaces SET owner_user_id = $2, created_by = $2 WHERE id = $1")
        .bind(workspace)
        .bind(owner)
        .execute(&app.db)
        .await
        .unwrap();
}

/// Workspace ids present as `requested` events, in order.
fn requested_workspaces(body: &Value) -> Vec<String> {
    body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "requested")
        .map(|event| event["workspace"]["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_feed_shows_only_readable_workspaces_and_labels_untrusted_text() {
    let f = Feed::new().await;
    let other = TestUser::create_with_scopes(&f.app, f.org.id, &[Scope::WorkspacesRead]).await;
    let definition = f.definition("Ontology Alignment").await;
    let now = Utc::now();
    let at = now - Duration::hours(1);

    let visible = f.workspace(other.id).await;
    let hidden = TestWorkspace::create_personal(&f.app, &f.org, other.id).await;

    f.request(
        f.caller.id,
        definition,
        Some(f.reader.id),
        f.reader.id,
        "pending",
        at,
        None,
        Some("please look\nsecond line"),
    )
    .await;
    f.request(
        visible.id,
        definition,
        Some(other.id),
        other.id,
        "pending",
        at,
        None,
        None,
    )
    .await;
    f.request(
        hidden.id,
        definition,
        Some(other.id),
        other.id,
        "pending",
        at,
        None,
        None,
    )
    .await;

    // A formal review workspace never appears: its source request plus a review
    // row make it a review workspace, excluded by the audience.
    let review_ws = f.workspace(f.reader.id).await;
    let source_request = f
        .request(
            f.caller.id,
            definition,
            Some(f.reader.id),
            f.reader.id,
            "settled",
            at,
            Some(at),
            None,
        )
        .await;
    let review_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agent_approval_reviews \
         (id, org_id, source_workspace_id, approval_request_id, requested_by, target_workspace_id, \
          review_workspace_id, definition_snapshot) \
         VALUES ($1, $2, $3, $4, $5, $3, $6, '{}'::jsonb)",
    )
    .bind(review_id)
    .bind(f.org.id)
    .bind(f.caller.id)
    .bind(source_request)
    .bind(f.reader.id)
    .bind(review_ws.id)
    .execute(&f.app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE workspaces SET agent_approval_review_id = $2 WHERE id = $1")
        .bind(review_ws.id)
        .bind(review_id)
        .execute(&f.app.db)
        .await
        .unwrap();
    f.request(
        review_ws.id,
        definition,
        Some(f.reader.id),
        f.reader.id,
        "pending",
        at,
        None,
        None,
    )
    .await;

    // The member reads own and ordinary workspaces, never another member's
    // personal workspace or a review.
    let (status, body) = f.get("kind=requested").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seen = requested_workspaces(&body);
    assert!(seen.contains(&f.caller.id.to_string()));
    assert!(seen.contains(&visible.id.to_string()));
    assert!(
        !seen.contains(&hidden.id.to_string()),
        "another member's personal workspace is unreadable"
    );
    assert!(
        !seen.contains(&review_ws.id.to_string()),
        "formal review workspace excluded"
    );

    // The own-workspace note is labelled untrusted and carries the full text.
    let own = body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| !event["note"].is_null())
        .unwrap_or_else(|| panic!("a noted event: {body}"));
    assert_eq!(own["workspace"]["id"], f.caller.id.to_string());
    assert_eq!(own["note"]["contentTrusted"], false);
    assert_eq!(own["note"]["text"], "please look\nsecond line");

    // An administrator sees the personal workspace too, from the next call.
    f.make_admin(f.reader.id).await;
    let (_, body) = f.get("kind=requested").await;
    assert!(requested_workspaces(&body).contains(&hidden.id.to_string()));

    f.app.cleanup().await;
}

#[tokio::test]
async fn window_edges_paging_and_summary_agree() {
    let f = Feed::new().await;
    f.allow_historical_timestamps().await;
    let definition = f.definition("Security Review").await;
    // Postgres stores microseconds; align the boundary so the exact-since row
    // is not truncated below it.
    let now = Utc::now().trunc_subsecs(6);
    let since = now - Duration::hours(10);
    let until = now - Duration::hours(1);

    // One request exactly at --since (included), one at --until (excluded).
    f.request(
        f.caller.id,
        definition,
        Some(f.reader.id),
        f.reader.id,
        "settled",
        since,
        Some(since),
        None,
    )
    .await;
    let at_until = f
        .request(
            f.caller.id,
            definition,
            Some(f.reader.id),
            f.reader.id,
            "pending",
            until,
            None,
            None,
        )
        .await;

    // A request before the window decided inside it: only the decision shows,
    // with latency.
    let before = now - Duration::hours(20);
    let decided_at = now - Duration::hours(5);
    let old_request = f
        .request(
            f.caller.id,
            definition,
            Some(f.reader.id),
            f.reader.id,
            "settled",
            before,
            Some(decided_at),
            None,
        )
        .await;
    f.decision(
        f.caller.id,
        definition,
        f.reader.id,
        "approved",
        Some("ok"),
        decided_at,
        Some(old_request),
    )
    .await;

    let window = format!(
        "since={}&until={}",
        urlencoding(&since.to_rfc3339()),
        urlencoding(&until.to_rfc3339())
    );
    let (status, body) = f.get(&window).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let events = body["data"]["events"].as_array().unwrap();
    let ids: Vec<String> = events
        .iter()
        .map(|event| event["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!ids.contains(&at_until.to_string()), "--until is exclusive");
    let decision = events
        .iter()
        .find(|event| event["kind"] == "decided")
        .unwrap_or_else(|| panic!("a decision in the window: {body}"));
    assert_eq!(
        decision["latencySeconds"].as_i64().unwrap(),
        (decided_at - before).num_seconds()
    );

    // Paging: a size-1 walk visits the same events, once each, newest first.
    let mut walked = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let query = match &cursor {
            Some(after) => format!("{window}&limit=1&after={}", urlencoding(after)),
            None => format!("{window}&limit=1"),
        };
        let (_, page) = f.get(&query).await;
        for event in page["data"]["events"].as_array().unwrap() {
            walked.push(event["id"].as_str().unwrap().to_string());
        }
        match page["data"]["nextCursor"].as_str() {
            Some(next) => cursor = Some(next.to_string()),
            None => break,
        }
    }
    assert_eq!(walked, ids, "paged walk equals one page");

    // Summary totals equal the walk over the same filter.
    let walk_decided = events
        .iter()
        .filter(|event| event["kind"] == "decided")
        .count() as i64;
    let (_, summary) = f.get(&format!("{window}&summary=true")).await;
    assert_eq!(
        summary["data"]["summary"]["decided"].as_i64().unwrap(),
        walk_decided
    );

    f.app.cleanup().await;
}

#[tokio::test]
async fn reviewer_me_pending_and_bounds() {
    let f = Feed::new().await;
    let other = TestUser::create_with_scopes(&f.app, f.org.id, &[Scope::WorkspacesRead]).await;
    let definition = f.definition("UI changes").await;
    let now = Utc::now();

    // A request addressed to the owner on someone else's readable workspace.
    let visible = f.workspace(other.id).await;
    f.pending(
        visible.id,
        definition,
        f.reader.id,
        now - Duration::hours(2),
    )
    .await;
    // One addressed to someone else: not listed with --reviewer @me.
    f.pending(visible.id, definition, other.id, now - Duration::hours(3))
        .await;

    let (status, body) = f.get("reviewer=@me&pending=true&summary=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pending = body["data"]["pending"].as_array().unwrap();
    assert_eq!(pending.len(), 1, "only the owner's request: {body}");
    assert_eq!(pending[0]["workspace"]["id"], visible.id.to_string());

    // The window cap and the limit bound are refused with clear errors.
    let wide = format!(
        "since={}",
        urlencoding(&(now - Duration::days(40)).to_rfc3339())
    );
    let (status, body) = f.get(&wide).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.to_string().contains("31-day"), "{body}");

    let (status, _) = f.get("limit=500").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    f.app.cleanup().await;
}

fn urlencoding(value: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", [("v", value)])
        .unwrap()
        .query()
        .unwrap()
        .trim_start_matches("v=")
        .to_string()
}
