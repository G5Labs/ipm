//! Outbound email delivery and the invitation email
//! (`.g5/ontology/software_architecture/email.g5.md`,
//! `.g5/ontology/features/organizations.g5.md#g5.organization.invitation-email`).
//!
//! Every case runs the real routes, `OrgService`, `email_deliveries` table and
//! worker against live Postgres. Provider behavior comes from the capture
//! transport or from a `wiremock` server speaking Resend's `POST /emails`
//! contract (JSON `{id}` on success, `{name, message}` errors,
//! `Idempotency-Key` header).

use crate::helpers;

use std::time::Duration;

use g5_api::config::{AppConfig, EmailConfig, EmailTransportKind, EmailVolumeLimits};
use g5_api::services::email::{EmailBatchResult, EmailWorker};
use helpers::{test_config, TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SENDER_DOMAIN: &str = "mail.g5labs.test";
/// Invitations' mailbox at the configured domain, with the default display name.
const SENDER: &str = "G5 <invitations@mail.g5labs.test>";
const RESEND_KEY: &str = "re_test_key_that_must_not_be_logged";

fn capture_config() -> AppConfig {
    let mut config = test_config();
    config.email = EmailConfig {
        transport: EmailTransportKind::Capture,
        from_domain: Some(SENDER_DOMAIN.to_string()),
        ..EmailConfig::default()
    };
    config
}

fn resend_config(api_url: &str) -> AppConfig {
    let mut config = test_config();
    config.email = EmailConfig {
        transport: EmailTransportKind::Resend,
        from_domain: Some(SENDER_DOMAIN.to_string()),
        resend_api_key: Some(RESEND_KEY.to_string()),
        resend_api_url: api_url.to_string(),
        ..EmailConfig::default()
    };
    config
}

fn worker(app: &TestApp) -> EmailWorker {
    app.state
        .email
        .worker(
            app.state.system_pool.clone(),
            g5_api::services::email_gates(),
        )
        .expect("email is enabled")
}

/// A fresh handle and worker built from the same configuration, as after a
/// process restart.
fn restarted_worker(app: &TestApp, config: &AppConfig) -> EmailWorker {
    g5_api::services::Email::from_config(&config.email, &app.base_url)
        .expect("config")
        .worker(
            app.state.system_pool.clone(),
            g5_api::services::email_gates(),
        )
        .expect("email is enabled")
}

async fn invite(app: &TestApp, admin: &TestUser, org: Uuid, email: &str) -> reqwest::Response {
    app.authenticated_client(admin)
        .post_json(
            &format!("/api/v1/orgs/{org}/members"),
            &json!({ "email": email, "role": "member" }),
        )
        .await
}

async fn invite_ok(app: &TestApp, admin: &TestUser, org: Uuid, email: &str) -> Uuid {
    let response = invite(app, admin, org, email).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    Uuid::parse_str(
        body["data"]["invitation"]["id"]
            .as_str()
            .expect("invitation id"),
    )
    .unwrap()
}

async fn resend(app: &TestApp, admin: &TestUser, org: Uuid, invitation: Uuid) -> StatusCode {
    app.authenticated_client(admin)
        .post_json(
            &format!("/api/v1/orgs/{org}/invitations/{invitation}/resend"),
            &json!({}),
        )
        .await
        .status()
}

#[derive(Debug, sqlx::FromRow)]
struct Delivery {
    id: Uuid,
    org_id: Uuid,
    status: String,
    attempts: i32,
    sender: String,
    recipient: String,
    subject: Option<String>,
    text_body: Option<String>,
    outcome_uncertain: bool,
    terminal_reason: Option<String>,
    last_error: Option<String>,
    provider_message_id: Option<String>,
    next_in_secs: f64,
}

async fn deliveries(app: &TestApp, invitation: Uuid) -> Vec<Delivery> {
    sqlx::query_as(
        "SELECT id, org_id, status, attempts, sender, recipient, subject, text_body,
                outcome_uncertain, terminal_reason, last_error, provider_message_id,
                extract(epoch FROM next_attempt_at - now())::float8 AS next_in_secs
           FROM email_deliveries
          WHERE purpose_kind = 'org_invitation' AND purpose_key = $1
          ORDER BY created_at, id",
    )
    .bind(invitation.to_string())
    .fetch_all(&app.db)
    .await
    .unwrap()
}

async fn only_delivery(app: &TestApp, invitation: Uuid) -> Delivery {
    let mut rows = deliveries(app, invitation).await;
    assert_eq!(rows.len(), 1, "expected one delivery: {rows:?}");
    rows.remove(0)
}

async fn make_due(app: &TestApp) {
    sqlx::query("UPDATE email_deliveries SET next_attempt_at = now() WHERE status = 'queued'")
        .execute(&app.db)
        .await
        .unwrap();
}

fn new_address() -> String {
    format!("invitee-{}@example.com", Uuid::new_v4().simple())
}

async fn setup(config: AppConfig) -> (TestApp, TestUser, TestOrg) {
    let app = TestApp::spawn_with_config(config).await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    (app, owner, org)
}

fn resend_ok(id: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "id": id }))
}

fn resend_error(status: u16, name: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({ "statusCode": status, "name": name, "message": "test" }))
}

fn idempotency_keys(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .map(|request| {
            request
                .headers
                .get("Idempotency-Key")
                .expect("every send carries an idempotency key")
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

// ===========================================================================
// Committed handoff
// ===========================================================================

#[tokio::test]
async fn a_new_pending_invitation_records_one_email_that_the_worker_sends() {
    let (app, owner, org) = setup(capture_config()).await;
    let address = new_address();
    let invitation = invite_ok(&app, &owner, org.id, &address).await;

    let queued = only_delivery(&app, invitation).await;
    assert_eq!(queued.status, "queued");
    assert_eq!(queued.org_id, org.id);
    assert_eq!(queued.recipient, address);
    assert_eq!(queued.sender, SENDER);
    let subject = queued.subject.clone().expect("rendered at request time");
    assert!(subject.contains(&org.name), "{subject}");

    let result = worker(&app).process_due(10).await.unwrap();
    assert_eq!(result.sent, 1);

    let sent = only_delivery(&app, invitation).await;
    assert_eq!(sent.status, "sent");
    assert_eq!(sent.attempts, 1);
    assert!(!sent.outcome_uncertain);
    assert!(
        sent.subject.is_none() && sent.text_body.is_none(),
        "content cleared"
    );
    assert_eq!(
        sent.provider_message_id.as_deref(),
        Some(format!("capture-{}", sent.id).as_str())
    );

    let captured = app.state.email.capture().unwrap().messages();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].recipient, address);
    assert_eq!(captured[0].sender, SENDER);
    assert_eq!(captured[0].subject, subject);
    assert_eq!(captured[0].idempotency_key, format!("g5-email-{}", sent.id));

    app.cleanup().await;
}

#[tokio::test]
async fn the_sender_is_the_consumers_mailbox_at_the_configured_domain_and_name() {
    let mut config = capture_config();
    config.email.from_name = "Acme Platform".to_string();
    config.email.from_domain = Some("notify.acme.test".to_string());
    let (app, owner, org) = setup(config).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    let expected = "Acme Platform <invitations@notify.acme.test>";
    assert_eq!(only_delivery(&app, invitation).await.sender, expected);
    worker(&app).process_due(10).await.unwrap();
    let captured = app.state.email.capture().unwrap().messages();
    assert_eq!(captured[0].sender, expected);

    app.cleanup().await;
}

#[tokio::test]
async fn a_rolled_back_invite_records_no_email() {
    use g5_api::services::org_service::InviteMemberRequest;
    let (app, owner, org) = setup(capture_config()).await;
    let address = new_address();

    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(owner.id),
            &[g5_contracts::types::ids::OrgId::from_uuid(org.id)],
            &["org:admin".into()],
        )
        .await
        .unwrap();
    let mut db = g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
        .await
        .unwrap();
    let response = app
        .state
        .org_service
        .invite_member(
            &mut db,
            org.id,
            owner.id,
            InviteMemberRequest {
                email: address.clone(),
                role: "member".into(),
            },
        )
        .await
        .unwrap();
    let invitation = response.invitation.expect("pending invitation").id;
    // The request is visible inside the invite's own transaction...
    let inside: i64 =
        sqlx::query_scalar("SELECT count(*) FROM email_deliveries WHERE purpose_key = $1")
            .bind(invitation.to_string())
            .fetch_one(db.conn())
            .await
            .unwrap();
    assert_eq!(inside, 1);
    // ...and gone with it when the transaction does not commit.
    drop(db);

    assert!(deliveries(&app, invitation).await.is_empty());
    let result = worker(&app).process_due(10).await.unwrap();
    assert_eq!(result, EmailBatchResult::default());
    assert!(app.state.email.capture().unwrap().messages().is_empty());

    app.cleanup().await;
}

#[tokio::test]
async fn immediate_membership_existing_members_and_existing_pending_invites_send_nothing() {
    let (app, owner, org) = setup(capture_config()).await;
    let existing_user = TestUser::create(&app).await;
    let address = new_address();

    // An existing platform user becomes a member immediately.
    let response = invite(&app, &owner, org.id, &existing_user.email).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert!(body["data"]["membership"].is_object());
    // Inviting that member again, and re-inviting a pending address, change nothing.
    assert_eq!(
        invite(&app, &owner, org.id, &existing_user.email)
            .await
            .status(),
        StatusCode::OK
    );
    let invitation = invite_ok(&app, &owner, org.id, &address).await;
    let again = invite_ok(&app, &owner, org.id, &address).await;
    assert_eq!(invitation, again);

    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM email_deliveries")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(total, 1, "only the new pending invitation is emailed");

    app.cleanup().await;
}

#[tokio::test]
async fn an_address_g5_cannot_email_still_gets_its_invitation() {
    let (app, owner, org) = setup(capture_config()).await;
    let invitation = invite_ok(&app, &owner, org.id, "someone@localhost").await;
    assert!(deliveries(&app, invitation).await.is_empty());
    let status: String = sqlx::query_scalar("SELECT status FROM org_invitations WHERE id = $1")
        .bind(invitation)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_invites_for_one_address_record_one_email() {
    let (app, owner, org) = setup(capture_config()).await;
    let address = new_address();
    let (first, second) = tokio::join!(
        invite(&app, &owner, org.id, &address),
        invite(&app, &owner, org.id, &address)
    );
    assert!(first.status().is_success() || second.status().is_success());
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM email_deliveries d JOIN org_invitations i
             ON d.purpose_key = i.id::text
          WHERE i.email = $1",
    )
    .bind(&address)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(total, 1);

    app.cleanup().await;
}

#[tokio::test]
async fn disabled_email_records_nothing_and_invites_still_work() {
    let (app, owner, org) = setup(test_config()).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    assert!(deliveries(&app, invitation).await.is_empty());
    assert!(app
        .state
        .email
        .worker(
            app.state.system_pool.clone(),
            g5_api::services::email_gates()
        )
        .is_none());
    assert_eq!(
        resend(&app, &owner, org.id, invitation).await,
        StatusCode::OK
    );
    app.cleanup().await;
}

// ===========================================================================
// Restart recovery
// ===========================================================================

#[tokio::test]
async fn a_committed_request_is_sent_by_a_worker_started_later() {
    let config = capture_config();
    let (app, owner, org) = setup(config.clone()).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    // No worker ran before the "restart"; a new process's worker sends it.
    let result = restarted_worker(&app, &config)
        .process_due(10)
        .await
        .unwrap();
    assert_eq!(result.sent, 1);
    assert_eq!(only_delivery(&app, invitation).await.status, "sent");

    app.cleanup().await;
}

#[tokio::test]
async fn an_expired_lease_is_reclaimed_with_the_same_identity_and_marked_uncertain() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_ok("re-msg-1"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    // A worker crashed mid-send: the row is `sending` with an expired lease.
    sqlx::query(
        "UPDATE email_deliveries
            SET status = 'sending', attempts = 1, first_attempt_at = now(),
                lease_expires_at = now() - interval '1 second'
          WHERE purpose_key = $1",
    )
    .bind(invitation.to_string())
    .execute(&app.db)
    .await
    .unwrap();

    let result = worker(&app).process_due(10).await.unwrap();
    assert_eq!(result.sent, 1);
    let sent = only_delivery(&app, invitation).await;
    assert_eq!(sent.status, "sent");
    assert_eq!(sent.attempts, 2);
    assert!(
        sent.outcome_uncertain,
        "the crashed attempt may have reached the provider"
    );
    assert_eq!(sent.provider_message_id.as_deref(), Some("re-msg-1"));
    let requests = provider.received_requests().await.unwrap();
    assert_eq!(
        idempotency_keys(&requests),
        vec![format!("g5-email-{}", sent.id)]
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_worker_that_lost_its_lease_cannot_record_over_the_new_owner() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_ok("re-msg-slow").set_delay(Duration::from_millis(1500)))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    let stale = worker(&app);
    let fresh = worker(&app);
    let stale_run = tokio::spawn(async move { stale.process_due(1).await.unwrap() });
    // Let the first worker claim and start sending, then expire its lease.
    tokio::time::timeout(Duration::from_secs(10), async {
        while only_delivery(&app, invitation).await.status != "sending" {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the first worker claims the request");
    sqlx::query("UPDATE email_deliveries SET lease_expires_at = now() WHERE purpose_key = $1")
        .bind(invitation.to_string())
        .execute(&app.db)
        .await
        .unwrap();
    let fresh_result = fresh.process_due(1).await.unwrap();
    let stale_result = stale_run.await.unwrap();

    assert_eq!(stale_result.lost, 1, "stale completion must be refused");
    assert_eq!(fresh_result.sent, 1);
    let sent = only_delivery(&app, invitation).await;
    assert_eq!((sent.status.as_str(), sent.attempts), ("sent", 2));
    let keys = idempotency_keys(&provider.received_requests().await.unwrap());
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1], "both attempts share one identity");

    app.cleanup().await;
}

// ===========================================================================
// Transient, permanent and configuration failures
// ===========================================================================

#[tokio::test]
async fn a_provider_outage_retries_the_same_message_until_it_is_accepted() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(503, "service_unavailable"))
        .up_to_n_times(1)
        .mount(&provider)
        .await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header_exists("Idempotency-Key"))
        .respond_with(resend_ok("re-msg-2"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let address = new_address();
    let invitation = invite_ok(&app, &owner, org.id, &address).await;

    let first = worker(&app).process_due(10).await.unwrap();
    assert_eq!(first.retried, 1);
    let queued = only_delivery(&app, invitation).await;
    assert_eq!(queued.status, "queued");
    assert!(queued.outcome_uncertain, "a 5xx may have sent the message");
    assert_eq!(
        queued.last_error.as_deref(),
        Some("http_503:service_unavailable")
    );
    assert!(
        queued.next_in_secs > 15.0,
        "backed off: {}",
        queued.next_in_secs
    );
    // Not due yet: nothing happens.
    assert_eq!(worker(&app).process_due(10).await.unwrap().claimed, 0);

    make_due(&app).await;
    let second = worker(&app).process_due(10).await.unwrap();
    assert_eq!(second.sent, 1);
    let sent = only_delivery(&app, invitation).await;
    assert_eq!((sent.status.as_str(), sent.attempts), ("sent", 2));

    let requests = provider.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let keys = idempotency_keys(&requests);
    assert_eq!(keys[0], keys[1]);
    assert_eq!(
        requests[0].body, requests[1].body,
        "identical payload on retry"
    );
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(body["from"], SENDER);
    assert_eq!(body["to"], json!([address]));
    assert_eq!(
        requests[1].headers.get("authorization").unwrap(),
        &format!("Bearer {RESEND_KEY}")
    );
    // Retries never add requests.
    assert_eq!(deliveries(&app, invitation).await.len(), 1);

    app.cleanup().await;
}

#[tokio::test]
async fn a_permanent_rejection_fails_after_one_attempt() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(422, "missing_required_field"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    assert_eq!(worker(&app).process_due(10).await.unwrap().failed, 1);
    make_due(&app).await;
    assert_eq!(worker(&app).process_due(10).await.unwrap().claimed, 0);
    let failed = only_delivery(&app, invitation).await;
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.terminal_reason.as_deref(), Some("rejected"));
    assert_eq!(
        failed.last_error.as_deref(),
        Some("http_422:missing_required_field")
    );
    assert!(!failed.outcome_uncertain);
    assert!(failed.subject.is_none(), "content cleared");
    assert_eq!(provider.received_requests().await.unwrap().len(), 1);
    // The invitation itself is untouched.
    let status: String = sqlx::query_scalar("SELECT status FROM org_invitations WHERE id = $1")
        .bind(invitation)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "pending");

    app.cleanup().await;
}

#[tokio::test]
async fn rejected_credentials_are_retried_and_rate_limits_honor_retry_after() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(401, "missing_api_key"))
        .up_to_n_times(1)
        .mount(&provider)
        .await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(429, "rate_limit_exceeded").insert_header("retry-after", "7"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;

    assert_eq!(worker(&app).process_due(10).await.unwrap().retried, 1);
    let after_401 = only_delivery(&app, invitation).await;
    assert_eq!(after_401.status, "queued");
    assert_eq!(
        after_401.last_error.as_deref(),
        Some("http_401:missing_api_key")
    );
    assert!(!after_401.outcome_uncertain);

    make_due(&app).await;
    assert_eq!(worker(&app).process_due(10).await.unwrap().retried, 1);
    let after_429 = only_delivery(&app, invitation).await;
    assert_eq!(
        after_429.last_error.as_deref(),
        Some("http_429:rate_limit_exceeded")
    );
    assert!(
        (5.0..=8.0).contains(&after_429.next_in_secs),
        "Retry-After honored: {}",
        after_429.next_in_secs
    );

    app.cleanup().await;
}

#[tokio::test]
async fn retries_stop_inside_the_idempotency_window_and_keep_the_uncertain_flag() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(500, "application_error"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    // The first attempt was almost 23 hours ago.
    sqlx::query(
        "UPDATE email_deliveries
            SET first_attempt_at = now() - interval '22 hours 59 minutes 50 seconds',
                attempts = 12
          WHERE purpose_key = $1",
    )
    .bind(invitation.to_string())
    .execute(&app.db)
    .await
    .unwrap();

    assert_eq!(worker(&app).process_due(10).await.unwrap().failed, 1);
    let failed = only_delivery(&app, invitation).await;
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.terminal_reason.as_deref(), Some("retry_window"));
    assert!(failed.outcome_uncertain, "possibly sent");
    make_due(&app).await;
    assert_eq!(
        worker(&app).process_due(10).await.unwrap().claimed,
        0,
        "never retried"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_retry_claimed_after_the_window_ends_without_sending() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_ok("re-msg-late"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    // An uncertain retry the worker only reaches after the window (an outage).
    sqlx::query(
        "UPDATE email_deliveries
            SET first_attempt_at = now() - interval '23 hours 1 minute',
                attempts = 3, outcome_uncertain = true, last_error = 'http_503:unknown'
          WHERE purpose_key = $1",
    )
    .bind(invitation.to_string())
    .execute(&app.db)
    .await
    .unwrap();

    assert_eq!(worker(&app).process_due(10).await.unwrap().failed, 1);
    let failed = only_delivery(&app, invitation).await;
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.terminal_reason.as_deref(), Some("retry_window"));
    assert!(failed.outcome_uncertain, "possibly sent");
    assert_eq!(failed.last_error.as_deref(), Some("http_503:unknown"));
    assert!(
        provider.received_requests().await.unwrap().is_empty(),
        "nothing sent outside the idempotency window"
    );

    app.cleanup().await;
}

// ===========================================================================
// Invalidated invitations
// ===========================================================================

#[tokio::test]
async fn invalidated_invitations_cancel_their_queued_email_without_sending() {
    let (app, owner, org) = setup(capture_config()).await;
    let revoked = invite_ok(&app, &owner, org.id, &new_address()).await;
    let accepted = invite_ok(&app, &owner, org.id, &new_address()).await;
    let declined = invite_ok(&app, &owner, org.id, &new_address()).await;
    let expired = invite_ok(&app, &owner, org.id, &new_address()).await;
    let swept = invite_ok(&app, &owner, org.id, &new_address()).await;

    let response = app
        .authenticated_client(&owner)
        .delete(&format!("/api/v1/orgs/{}/invitations/{revoked}", org.id))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    for (id, status) in [
        (accepted, "accepted"),
        (declined, "declined"),
        (swept, "expired"),
    ] {
        sqlx::query("UPDATE org_invitations SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
    }
    // Past its deadline but not yet swept by the hourly GC.
    sqlx::query(
        "UPDATE org_invitations SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(expired)
    .execute(&app.db)
    .await
    .unwrap();

    let result = worker(&app).process_due(10).await.unwrap();
    assert_eq!(result.cancelled, 5);
    assert_eq!(result.sent, 0);
    assert!(app.state.email.capture().unwrap().messages().is_empty());
    for (id, reason) in [
        (revoked, "cancelled:invitation_revoked"),
        (accepted, "cancelled:invitation_accepted"),
        (declined, "cancelled:invitation_declined"),
        (expired, "cancelled:invitation_expired"),
        (swept, "cancelled:invitation_expired"),
    ] {
        let row = only_delivery(&app, id).await;
        assert_eq!(row.status, "cancelled");
        assert_eq!(row.terminal_reason.as_deref(), Some(reason));
        assert!(row.subject.is_none());
    }

    app.cleanup().await;
}

#[tokio::test]
async fn an_invitation_accepted_before_sending_cancels_its_email() {
    let (app, owner, org) = setup(capture_config()).await;
    let invitee = TestUser::create(&app).await;
    // Created for the address before that user existed in this org.
    let invitation = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at)
         VALUES ($1, $2, $3, 'member', $4, 'pending', now() + interval '7 days')",
    )
    .bind(invitation)
    .bind(org.id)
    .bind(&invitee.email)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        resend(&app, &owner, org.id, invitation).await,
        StatusCode::OK
    );
    assert_eq!(only_delivery(&app, invitation).await.status, "queued");

    let response = app
        .authenticated_client(&invitee)
        .post_json(
            &format!("/api/v1/invitations/{invitation}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(worker(&app).process_due(10).await.unwrap().cancelled, 1);
    assert_eq!(
        only_delivery(&app, invitation)
            .await
            .terminal_reason
            .as_deref(),
        Some("cancelled:invitation_accepted")
    );

    app.cleanup().await;
}

// ===========================================================================
// Resend contract and duplicate suppression
// ===========================================================================

#[tokio::test]
async fn resend_coalesces_with_a_live_email_and_sends_a_new_one_after_it_ends() {
    let (app, owner, org) = setup(capture_config()).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    let first = only_delivery(&app, invitation).await;
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM org_invitations WHERE id = $1")
            .bind(invitation)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Repeated presses while the first email is queued add nothing.
    for _ in 0..3 {
        assert_eq!(
            resend(&app, &owner, org.id, invitation).await,
            StatusCode::OK
        );
    }
    let rows = deliveries(&app, invitation).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, first.id);

    assert_eq!(worker(&app).process_due(10).await.unwrap().sent, 1);
    // After it was sent, resend is a new message identity.
    assert_eq!(
        resend(&app, &owner, org.id, invitation).await,
        StatusCode::OK
    );
    let rows = deliveries(&app, invitation).await;
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[1].id, first.id);
    assert_eq!(rows[1].status, "queued");
    assert_eq!(worker(&app).process_due(10).await.unwrap().sent, 1);

    let captured = app.state.email.capture().unwrap().messages();
    assert_eq!(captured.len(), 2);
    assert_ne!(captured[0].idempotency_key, captured[1].idempotency_key);
    // Resend still extends expiry.
    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM org_invitations WHERE id = $1")
            .bind(invitation)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(after >= before);

    app.cleanup().await;
}

#[tokio::test]
async fn resend_of_a_non_pending_invitation_is_refused_and_records_nothing() {
    let (app, owner, org) = setup(capture_config()).await;
    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    sqlx::query("UPDATE org_invitations SET status = 'declined' WHERE id = $1")
        .bind(invitation)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        resend(&app, &owner, org.id, invitation).await,
        StatusCode::CONFLICT
    );
    assert_eq!(deliveries(&app, invitation).await.len(), 1);
    app.cleanup().await;
}

// ===========================================================================
// Volume limit (g5.organization.invitation-email-limit)
// ===========================================================================

async fn fill_volume(app: &TestApp, org: Uuid, count: i64) {
    sqlx::query(
        "INSERT INTO email_deliveries (
             id, org_id, purpose_kind, purpose_key, sender, recipient, status,
             completed_at, created_at
         )
         SELECT gen_random_uuid(), $1, 'org_invitation', gen_random_uuid()::text,
                'x@example.com', 'y@example.com', 'sent', now(),
                now() - interval '23 hours' + make_interval(secs => n)
           FROM generate_series(1, $2) AS n",
    )
    .bind(org)
    .bind(count)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_hundred_and_first_invitation_email_in_a_day_is_refused_with_nothing_written() {
    let (app, owner, org) = setup(capture_config()).await;
    let other_owner_org = {
        let mut other = TestUser::create(&app).await;
        let other_org = TestOrg::create_with_admin(&app, &mut other).await;
        (other, other_org)
    };
    let pending = invite_ok(&app, &owner, org.id, &new_address()).await;
    fill_volume(&app, org.id, 98).await;

    // Two concurrent invites at 99 of 100: exactly one is admitted.
    let (a, b) = (new_address(), new_address());
    let (first, second) = tokio::join!(
        invite(&app, &owner, org.id, &a),
        invite(&app, &owner, org.id, &b)
    );
    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS]);
    let refused = if first.status() == StatusCode::TOO_MANY_REQUESTS {
        (&a, first)
    } else {
        (&b, second)
    };
    let retry_after: u64 = refused
        .1
        .headers()
        .get("retry-after")
        .expect("Retry-After")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    // The oldest counted request leaves the window in about an hour.
    assert!((3000..=3700).contains(&retry_after), "{retry_after}");
    let written: i64 = sqlx::query_scalar("SELECT count(*) FROM org_invitations WHERE email = $1")
        .bind(refused.0)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(written, 0, "the refused invitation is not created");

    // A resend that coalesces into a live email is not a new request.
    assert_eq!(resend(&app, &owner, org.id, pending).await, StatusCode::OK);
    // A resend that would add one is refused and does not extend expiry.
    assert_eq!(worker(&app).process_due(10).await.unwrap().sent, 2);
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM org_invitations WHERE id = $1")
            .bind(pending)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        resend(&app, &owner, org.id, pending).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM org_invitations WHERE id = $1")
            .bind(pending)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(before, after);

    // Another organization is unaffected.
    let (other, other_org) = other_owner_org;
    invite_ok(&app, &other, other_org.id, &new_address()).await;

    app.cleanup().await;
}

#[tokio::test]
async fn a_local_stack_with_volume_limits_off_admits_invitations_past_the_cap() {
    let mut config = capture_config();
    config.email.volume_limits = EmailVolumeLimits::Off;
    let (app, owner, org) = setup(config).await;
    fill_volume(&app, org.id, 100).await;

    let invitation = invite_ok(&app, &owner, org.id, &new_address()).await;
    assert_eq!(only_delivery(&app, invitation).await.status, "queued");
    assert_eq!(worker(&app).process_due(10).await.unwrap().sent, 1);

    app.cleanup().await;
}

// ===========================================================================
// Observability and data
// ===========================================================================

#[tokio::test]
async fn administrators_see_each_invitations_latest_email_status() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(422, "validation_error"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let failed = invite_ok(&app, &owner, org.id, &new_address()).await;
    worker(&app).process_due(10).await.unwrap();
    let queued = invite_ok(&app, &owner, org.id, &new_address()).await;

    let body: Value = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/orgs/{}/invitations", org.id))
        .await
        .json()
        .await
        .unwrap();
    let find = |id: Uuid| {
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|inv| inv["id"] == id.to_string())
            .unwrap()["emailDelivery"]
            .clone()
    };
    assert_eq!(find(failed)["status"], "failed");
    assert_eq!(find(failed)["reason"], "rejected");
    assert_eq!(find(failed)["outcomeUncertain"], false);
    assert_eq!(find(queued)["status"], "queued");
    assert!(find(queued)["updatedAt"].is_string());

    app.cleanup().await;
}

#[tokio::test]
async fn delivery_logs_carry_no_credential_recipient_or_content() {
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_error(503, "service_unavailable"))
        .up_to_n_times(1)
        .mount(&provider)
        .await;
    Mock::given(method("POST"))
        .and(path("/emails"))
        .respond_with(resend_ok("re-msg-logs"))
        .mount(&provider)
        .await;
    let (app, owner, org) = setup(resend_config(&provider.uri())).await;
    let address = new_address();

    // The process's one subscriber records the email module's target
    // (`helpers::tracing_capture`); a scoped subscriber would race the
    // callsite interest cache with tests running in parallel.
    let events = helpers::tracing_capture();

    let invitation = invite_ok(&app, &owner, org.id, &address).await;
    worker(&app).process_due(10).await.unwrap();
    make_due(&app).await;
    worker(&app).process_due(10).await.unwrap();
    let delivery = only_delivery(&app, invitation).await;
    assert_eq!(delivery.status, "sent");
    let delivery = delivery.id;

    // The email module's own events, from every test in the process: none may
    // carry this test's secrets, and this delivery's must be among them.
    let output: String = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.target == "g5_api::email")
        .map(|event| format!("{} {:?}\n", event.level, event.fields))
        .collect();
    let mine: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.contains(&invitation.to_string()) || line.contains(&delivery.to_string())
        })
        .collect();
    assert!(
        mine.iter()
            .any(|line| line.contains("Email delivery requested")),
        "{output}"
    );
    assert!(
        mine.iter()
            .any(|line| line.contains("Email accepted by provider")),
        "{output}"
    );
    for secret in [RESEND_KEY, address.as_str(), org.name.as_str()] {
        assert!(
            !output.contains(secret),
            "logs leaked {secret:?}:\n{output}"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn email_deliveries_are_accessible_only_to_the_system_role() {
    let app = TestApp::spawn().await;
    for privilege in ["SELECT", "INSERT", "UPDATE", "DELETE"] {
        for (role, expected) in [("g5_app", true), ("g5_tenant", false)] {
            let granted: bool =
                sqlx::query_scalar("SELECT has_table_privilege($1, 'public.email_deliveries', $2)")
                    .bind(role)
                    .bind(privilege)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(granted, expected, "{role} {privilege}");
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn ended_requests_are_deleted_after_thirty_days() {
    let (app, _owner, org) = setup(capture_config()).await;
    fill_volume(&app, org.id, 2).await;
    sqlx::query(
        "UPDATE email_deliveries SET completed_at = now() - interval '31 days'
          WHERE id = (SELECT id FROM email_deliveries ORDER BY created_at LIMIT 1)",
    )
    .execute(&app.db)
    .await
    .unwrap();
    worker(&app).purge_ended().await;
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM email_deliveries")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(left, 1);
    app.cleanup().await;
}

// ===========================================================================
// End to end: invite, inspect the message, follow its link through login
// ===========================================================================

fn oauth_attempt_cookie(response: &reqwest::Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.contains("g5_oauth_"))
        .and_then(|value| value.split(';').next())
        .expect("OAuth initiation should set an attempt cookie")
        .to_string()
}

#[tokio::test]
async fn an_invitee_follows_the_emailed_link_through_login_into_the_organization() {
    let github = MockServer::start().await;
    let mut config = capture_config();
    config.auth.github = Some(g5_api::config::OAuthProviderConfig {
        client_id: "test-github-client-id".to_string(),
        client_secret: "test-github-client-secret".to_string(),
        authorize_url: Some(format!("{}/login/oauth/authorize", github.uri())),
        token_url: Some(format!("{}/login/oauth/access_token", github.uri())),
        userinfo_url: Some(format!("{}/user", github.uri())),
    });
    let (app, owner, org) = setup(config).await;
    let address = new_address();
    invite_ok(&app, &owner, org.id, &address).await;
    assert_eq!(worker(&app).process_due(10).await.unwrap().sent, 1);

    // Inspect the delivered message.
    let message = app.state.email.capture().unwrap().messages().remove(0);
    assert_eq!(message.recipient, address);
    assert_eq!(message.sender, SENDER);
    assert!(message.subject.contains(&org.name));
    assert!(message.text.contains(&owner.display_name) || message.text.contains(&owner.email));
    let link = message
        .text
        .lines()
        .find(|line| line.starts_with("http"))
        .expect("the message carries a link")
        .to_string();
    assert!(message.html.contains(&format!("href=\"{link}\"")));
    let link = reqwest::Url::parse(&link).unwrap();
    assert_eq!(
        link.as_str(),
        format!("{}/{}", app.base_url, org.slug),
        "the link opens the organization's page on the public URL"
    );
    let linked_slug = link.path().trim_start_matches('/').to_string();
    // The link carries no credential: following it grants nothing by itself.
    assert!(link.query().is_none());

    // The organization page requires login; the invitee signs in through
    // GitHub with the invited, verified address.
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "invitee-token", "token_type": "bearer", "scope": "user:email"
        })))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 77_001, "login": "invitee", "name": "Invitee", "email": address,
            "avatar_url": "https://example.com/invitee.png"
        })))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "email": address, "primary": true, "verified": true }
        ])))
        .mount(&github)
        .await;
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let login = browser
        .get(format!("{}/api/v1/auth/login/github", app.base_url))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_redirection());
    let state = reqwest::Url::parse(login.headers()["location"].to_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.to_string())
        .unwrap();
    let callback = browser
        .get(format!(
            "{}/api/v1/auth/callback/github?code=invitee-code&state={state}",
            app.base_url
        ))
        .header("cookie", oauth_attempt_cookie(&login))
        .send()
        .await
        .unwrap();
    assert!(callback.status().is_redirection(), "{}", callback.status());

    // Login accepted the invitation into the organization the link named.
    let (role, status): (String, String) = sqlx::query_as(
        "SELECT m.role, i.status
           FROM org_memberships m
           JOIN users u ON u.id = m.user_id
           JOIN orgs o ON o.id = m.org_id
           JOIN org_invitations i ON i.org_id = o.id AND lower(i.email) = lower(u.email)
          WHERE o.slug = $1 AND lower(u.email) = lower($2)",
    )
    .bind(&linked_slug)
    .bind(&address)
    .fetch_one(&app.db)
    .await
    .expect("the invitee is a member of the linked organization");
    assert_eq!((role.as_str(), status.as_str()), ("member", "accepted"));

    app.cleanup().await;
}
