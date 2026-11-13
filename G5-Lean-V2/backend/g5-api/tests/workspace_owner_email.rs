//! `g5d workspace email-owner`: the workspace agent emails the workspace's current owner
//! (`.g5/ontology/features/workspaces.g5.md#g5.workspace.owner-email`).
//!
//! Every case runs the real incarnation route, the workspace service, the
//! `email_deliveries` table and the worker against live Postgres, with the
//! capture transport standing in for the provider.

use crate::helpers;

use g5_api::config::{AppConfig, EmailConfig, EmailTransportKind};
use helpers::{test_config, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

const SENDER_DOMAIN: &str = "mail.g5labs.test";

fn capture_config() -> AppConfig {
    let mut config = test_config();
    config.email = EmailConfig {
        transport: EmailTransportKind::Capture,
        from_domain: Some(SENDER_DOMAIN.to_string()),
        ..EmailConfig::default()
    };
    config
}

struct Fixture {
    app: TestApp,
    owner: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
    token: String,
}

impl Fixture {
    async fn new(config: AppConfig) -> Self {
        let app = TestApp::spawn_with_config(config).await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create_named(&app, &org, "feature/login").await;
        let token = agent_token(&app, org.id, workspace.id, owner.id).await;
        Self {
            app,
            owner,
            org,
            workspace,
            token,
        }
    }

    async fn email(&self, body: &Value) -> (StatusCode, Value) {
        self.email_as(&self.token, self.workspace.id, body).await
    }

    async fn email_as(&self, token: &str, workspace_id: Uuid, body: &Value) -> (StatusCode, Value) {
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/internal/workspaces/{workspace_id}/owner-email",
                self.app.base_url
            ))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn deliveries(&self) -> Vec<(Uuid, String, String, Option<String>)> {
        sqlx::query_as(
            "SELECT id, recipient, status, terminal_reason FROM email_deliveries
              WHERE purpose_kind = 'workspace_owner_message'
              ORDER BY created_at, id",
        )
        .fetch_all(&self.app.db)
        .await
        .unwrap()
    }

    async fn send_due(&self) -> g5_api::services::email::EmailBatchResult {
        self.app
            .state
            .email
            .worker(
                self.app.state.system_pool.clone(),
                g5_api::services::email_gates(),
            )
            .expect("email is enabled")
            .process_due(10)
            .await
            .unwrap()
    }
}

/// A running VM for the workspace and its owner's incarnation token, as the
/// workspace agent holds it.
async fn agent_token(app: &TestApp, org_id: Uuid, workspace_id: Uuid, owner: Uuid) -> String {
    let host_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported) \
         VALUES ($1, $2, 'us-central1', 'ready', now(), true)",
    )
    .bind(host_id)
    .bind(org_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_vms \
         (org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at) \
         VALUES ($1, $2, $3, $4, 1, 'running', now() + interval '1 hour')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(host_id)
    .bind(vm_id.to_string())
    .execute(&app.db)
    .await
    .unwrap();
    app.state
        .jwt_minter
        .as_ref()
        .expect("test jwt minter")
        .mint_workspace_incarnation(org_id, workspace_id, owner, vm_id, 1, 3600)
        .unwrap()
}

fn message(key: &str) -> Value {
    json!({
        "subject": "Tests pass",
        "body": "All 42 tests pass.\n<a href=\"https://evil.example\">Log in to G5</a>\nBcc: someone@example.com",
        "idempotencyKey": key,
    })
}

#[tokio::test]
async fn the_owner_receives_the_agents_message_with_attribution_and_a_workspace_link() {
    let fx = Fixture::new(capture_config()).await;

    let (status, body) = fx.email(&message("k-1")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["status"], "queued");
    let id = body["data"]["deliveryId"].as_str().unwrap().to_string();
    let rows = fx.deliveries().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0.to_string(), id);
    assert_eq!(rows[0].1, fx.owner.email, "the recipient is the owner");

    assert_eq!(fx.send_due().await.sent, 1);
    let captured = fx.app.state.email.capture().unwrap().messages();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.recipient, fx.owner.email);
    assert_eq!(sent.sender, format!("G5 Agent <agent@{SENDER_DOMAIN}>"));
    assert_eq!(sent.subject, "[feature/login] Tests pass");
    assert!(
        sent.text.starts_with(&format!(
            "Message from the agent in your G5 workspace feature/login ({}). G5 did not write \
             it.\n\nAll 42 tests pass.\n",
            fx.org.name
        )),
        "{}",
        sent.text
    );
    assert!(sent.text.contains(&format!(
        "Sent by the agent in your G5 workspace feature/login ({})",
        fx.org.name
    )));
    let link = format!("{}/{}/feature%2Flogin", fx.app.base_url, fx.org.slug);
    assert!(sent.text.contains(&link), "{}", sent.text);
    assert!(
        !sent.html.contains("<a href=\"https://evil.example\">"),
        "agent text is never HTML: {}",
        sent.html
    );
    assert!(sent
        .html
        .contains("&lt;a href=&quot;https://evil.example&quot;&gt;"));

    fx.app.cleanup().await;
}

async fn add_provider_repo(app: &TestApp, org: Uuid, full_name: &str) {
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider, provider_full_name) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, 'main', 'github', $2)",
    )
    .bind(org)
    .bind(full_name)
    .bind(full_name.replace('/', "-").to_lowercase())
    .bind(format!("https://github.com/{full_name}.git"))
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_markdown_body_links_only_to_g5_and_the_organizations_repositories() {
    let fx = Fixture::new(capture_config()).await;
    add_provider_repo(&fx.app, fx.org.id, "acme/api").await;
    // Another organization's repository is not this organization's scope.
    let mut stranger = TestUser::create(&fx.app).await;
    let other_org = TestOrg::create_with_admin(&fx.app, &mut stranger).await;
    add_provider_repo(&fx.app, other_org.id, "rival/app").await;

    let workspace_url = format!("{}/{}/feature%2Flogin", fx.app.base_url, fx.org.slug);
    let source = format!(
        "## Digest\n\n| PR | State |\n|---|---|\n| [#7](https://github.com/acme/api/pull/7) | merged |\n\n\
         - [workspace]({workspace_url})\n\
         - [rival](https://github.com/rival/app/pull/1)\n\
         - [https://github.com/acme/api](https://evil.example/login)\n\n<img src=x onerror=alert(1)>\n"
    );
    let (status, body) = fx
        .email(&json!({
            "subject": "Daily digest",
            "body": source,
            "format": "markdown",
            "idempotencyKey": "md-1",
        }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(fx.send_due().await.sent, 1);
    let sent = &fx.app.state.email.capture().unwrap().messages()[0];
    let html = &sent.html;

    let notice = html
        .find("Message from the agent in your G5 workspace")
        .unwrap();
    assert!(notice < html.find("<h2 ").unwrap(), "{html}");
    assert!(html.contains("<table "), "{html}");
    assert!(
        html.contains("<a href=\"https://github.com/acme/api/pull/7\""),
        "{html}"
    );
    // The test server's public URL is `http`, so even its own links are not
    // clickable; the unit tests cover an `https` G5 host.
    assert!(workspace_url.starts_with("http://"));
    assert!(
        html.contains(&format!("workspace ({workspace_url})")),
        "{html}"
    );
    assert!(
        html.contains("rival (https://github.com/rival/app/pull/1)"),
        "{html}"
    );
    assert!(!html.contains("href=\"https://github.com/rival"), "{html}");
    assert!(
        html.contains("https://github.com/acme/api (https://evil.example/login)"),
        "{html}"
    );
    assert!(!html.contains("href=\"https://evil.example"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(
        html.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{html}"
    );
    // The text part is the agent's source after the notice.
    assert!(
        sent.text.contains(&source.trim_end().to_string()),
        "{}",
        sent.text
    );

    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_markdown_body_outside_the_bounds_is_refused_with_its_reason_and_nothing_written() {
    let fx = Fixture::new(capture_config()).await;
    for (source, reason) in [
        (
            "- a\n  - b\n    - c\n",
            "lists nest at most 2 levels (line 3)",
        ),
        (
            "See [docs][d].\n\n[d]: https://example.com\n",
            "reference-style link definitions",
        ),
    ] {
        let (status, body) = fx
            .email(&json!({
                "subject": "s",
                "body": source,
                "format": "markdown",
                "idempotencyKey": "bad",
            }))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body.to_string().contains(reason), "{body}");
    }
    let (status, body) = fx
        .email(&json!({ "subject": "s", "body": "x", "format": "html", "idempotencyKey": "k" }))
        .await;
    assert!(status.is_client_error(), "{status} {body}");
    assert!(fx.deliveries().await.is_empty());

    // The same nesting is ordinary text without the Markdown format.
    let (status, body) = fx
        .email(&json!({ "subject": "s", "body": "- a\n  - b\n    - c", "idempotencyKey": "plain" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_repeated_idempotency_key_returns_the_earlier_request_and_sends_nothing_more() {
    let fx = Fixture::new(capture_config()).await;
    let (status, first) = fx.email(&message("retry-1")).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(fx.send_due().await.sent, 1);

    // A lost response retried after the message was sent.
    let (status, again) = fx.email(&message("retry-1")).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["data"]["status"], "existing");
    assert_eq!(again["data"]["deliveryId"], first["data"]["deliveryId"]);
    assert_eq!(fx.deliveries().await.len(), 1);
    assert_eq!(fx.send_due().await.claimed, 0);
    assert_eq!(fx.app.state.email.capture().unwrap().messages().len(), 1);

    // A new key is a new message.
    let (status, _) = fx.email(&message("retry-2")).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(fx.deliveries().await.len(), 2);

    fx.app.cleanup().await;
}

#[tokio::test]
async fn only_the_live_incarnation_of_this_workspace_may_send() {
    let fx = Fixture::new(capture_config()).await;
    let other = TestWorkspace::create_named(&fx.app, &fx.org, "other").await;

    // The token names another workspace than the path.
    let (status, body) = fx.email_as(&fx.token, other.id, &message("k")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // The VM's lease ran out: the incarnation is no longer live.
    sqlx::query("UPDATE workspace_vms SET lease_expires_at = now() - interval '1 second' WHERE workspace_id = $1")
        .bind(fx.workspace.id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    let (status, body) = fx.email(&message("k")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // A person's session is not a workspace credential.
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/internal/workspaces/{}/owner-email",
            fx.app.base_url, fx.workspace.id
        ))
        .bearer_auth(&fx.owner.jwt)
        .json(&message("k"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_client_error(), "{}", response.status());

    assert!(fx.deliveries().await.is_empty());
    fx.app.cleanup().await;
}

#[tokio::test]
async fn invalid_requests_and_disabled_email_are_refused_with_nothing_written() {
    let fx = Fixture::new(capture_config()).await;
    for bad in [
        json!({ "subject": "", "body": "x", "idempotencyKey": "k" }),
        json!({ "subject": "two\nlines", "body": "x", "idempotencyKey": "k" }),
        json!({ "subject": "s", "body": "x".repeat(20_001), "idempotencyKey": "k" }),
        json!({ "subject": "s", "body": "x", "idempotencyKey": "has space" }),
        json!({ "subject": "s", "body": "x", "idempotencyKey": "k", "to": "a@b.c" }),
    ] {
        let (status, body) = fx.email(&bad).await;
        assert!(status.is_client_error(), "{status} {body}");
    }
    assert!(fx.deliveries().await.is_empty());
    fx.app.cleanup().await;

    let fx = Fixture::new(test_config()).await;
    let (status, body) = fx.email(&message("k")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("not enabled"), "{body}");
    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_transfer_deletion_or_email_change_before_sending_cancels_the_message() {
    let fx = Fixture::new(capture_config()).await;

    // Ownership moves to another member.
    fx.email(&message("transfer")).await;
    let other = TestUser::create(&fx.app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(fx.org.id)
        .bind(other.id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(fx.workspace.id)
        .bind(other.id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    assert_eq!(fx.send_due().await.cancelled, 1);
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(fx.workspace.id)
        .bind(fx.owner.id)
        .execute(&fx.app.db)
        .await
        .unwrap();

    // The owner's login email changes.
    fx.email(&message("email-change")).await;
    sqlx::query("UPDATE users SET email = $2 WHERE id = $1")
        .bind(fx.owner.id)
        .bind(format!("moved-{}@example.com", Uuid::new_v4().simple()))
        .execute(&fx.app.db)
        .await
        .unwrap();
    assert_eq!(fx.send_due().await.cancelled, 1);

    // The workspace is deleted.
    sqlx::query("UPDATE users SET email = $2 WHERE id = $1")
        .bind(fx.owner.id)
        .bind(&fx.owner.email)
        .execute(&fx.app.db)
        .await
        .unwrap();
    let (status, body) = fx.email(&message("delete")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(fx.workspace.id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    assert_eq!(fx.send_due().await.cancelled, 1);

    let reasons: Vec<Option<String>> = fx.deliveries().await.into_iter().map(|row| row.3).collect();
    assert_eq!(
        reasons,
        [
            Some("cancelled:owner_changed".to_string()),
            Some("cancelled:owner_changed".to_string()),
            Some("cancelled:workspace_deleted".to_string()),
        ]
    );
    assert!(fx.app.state.email.capture().unwrap().messages().is_empty());
    fx.app.cleanup().await;
}

async fn fill(app: &TestApp, org: Uuid, recipient: &str, count: i64) {
    sqlx::query(
        "INSERT INTO email_deliveries (
             id, org_id, purpose_kind, purpose_key, sender, recipient, status,
             completed_at, created_at
         )
         SELECT gen_random_uuid(), $1, 'workspace_owner_message', gen_random_uuid()::text,
                'x@example.com', $2, 'sent', now(),
                now() - interval '23 hours' + make_interval(secs => n)
           FROM generate_series(1, $3) AS n",
    )
    .bind(org)
    .bind(recipient)
    .bind(count)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_fifty_first_message_to_one_owner_in_a_day_is_refused() {
    let fx = Fixture::new(capture_config()).await;
    fill(&fx.app, fx.org.id, &fx.owner.email, 49).await;

    // Two concurrent requests at 49 of 50: exactly one is admitted.
    let (first, second) = (message("a"), message("b"));
    let (a, b) = tokio::join!(fx.email(&first), fx.email(&second));
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::TOO_MANY_REQUESTS]
    );
    // A retry of the admitted key still answers; it is not a new request.
    let admitted = if a.0 == StatusCode::CREATED { "a" } else { "b" };
    assert_eq!(fx.email(&message(admitted)).await.0, StatusCode::OK);
    assert_eq!(fx.deliveries().await.len(), 50);

    fx.app.cleanup().await;
}

#[tokio::test]
async fn the_five_hundred_and_first_message_in_an_organization_is_refused() {
    let fx = Fixture::new(capture_config()).await;
    // Other owners' messages fill the organization's allowance.
    fill(&fx.app, fx.org.id, "someone-else@example.com", 500).await;
    let (status, body) = fx.email(&message("k")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM email_deliveries WHERE recipient = $1")
            .bind(&fx.owner.email)
            .fetch_one(&fx.app.db)
            .await
            .unwrap(),
        0
    );
    fx.app.cleanup().await;
}
