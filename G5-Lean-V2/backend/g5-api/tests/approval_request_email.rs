//! Emailing a human reviewer when their approval is requested
//! (`.g5/ontology/features/reviews.g5.md#g5.governance.approval.request-email`),
//! and the workspace owner when a reviewer decides
//! (`.g5/ontology/features/reviews.g5.md#g5.governance.approval.decision-email`).
//!
//! Every case runs the real approvals route, the `email_deliveries` table and
//! the delivery worker/gate against live Postgres, with the capture transport
//! standing in for the provider.

use crate::helpers;
use crate::helpers::review_flow::Flow;

use g5_api::config::{AppConfig, EmailConfig, EmailTransportKind};
use helpers::{test_config, TestApp};
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

/// Queued approval-request deliveries: `(recipient, status, terminal_reason)`.
async fn deliveries(app: &TestApp) -> Vec<(String, String, Option<String>)> {
    deliveries_of(app, "approval_request").await
}

/// Queued approval-decision deliveries: `(recipient, status, terminal_reason)`.
async fn decision_deliveries(app: &TestApp) -> Vec<(String, String, Option<String>)> {
    deliveries_of(app, "approval_decision").await
}

async fn deliveries_of(app: &TestApp, kind: &str) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT recipient, status, terminal_reason FROM email_deliveries
          WHERE purpose_kind = $1
          ORDER BY created_at, id",
    )
    .bind(kind)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

async fn send_due(app: &TestApp) {
    app.state
        .email
        .worker(
            app.state.system_pool.clone(),
            g5_api::services::email_gates(),
        )
        .expect("email is enabled")
        .process_due(10)
        .await
        .unwrap();
}

/// A new request emails each addressed reviewer exactly once, from the
/// dedicated approvals sender with the fixed subject prefix, and never the
/// requesting owner.
#[tokio::test]
async fn a_new_request_emails_each_reviewer_once() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;

    let mut queued = deliveries(&app).await;
    queued.sort();
    assert_eq!(queued.len(), 2, "one email per newly addressed reviewer");
    let recipients: Vec<&str> = queued.iter().map(|row| row.0.as_str()).collect();
    assert!(recipients.contains(&f.reviewer.email.as_str()));
    assert!(recipients.contains(&f.second.email.as_str()));
    // The requesting owner, though a configured reviewer, is never emailed.
    assert!(!recipients.contains(&f.owner.email.as_str()));

    send_due(&app).await;
    let captured = app.state.email.capture().unwrap().messages();
    assert_eq!(captured.len(), 2);
    for sent in &captured {
        assert_eq!(
            sent.sender,
            format!("G5 Approvals <approvals@{SENDER_DOMAIN}>")
        );
        assert!(
            sent.subject
                .starts_with("[G5 Approval] review-flow: Security review requested by "),
            "{}",
            sent.subject
        );
        assert!(sent.text.contains("requested your approval"));
    }
    app.cleanup().await;
}

/// A reused request (a replay of the same pending request) sends no new email.
#[tokio::test]
async fn a_reused_request_sends_nothing_more() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let first = f.request(&app, "be").await;
    assert_eq!(deliveries(&app).await.len(), 2);

    // Requesting the same pending reviewers again reuses the requests.
    let again = f.request(&app, "be").await;
    assert_eq!(first.len(), again.len());
    assert_eq!(deliveries(&app).await.len(), 2, "reuse queues no new email");

    send_due(&app).await;
    assert_eq!(app.state.email.capture().unwrap().messages().len(), 2);
    app.cleanup().await;
}

/// A reviewer who turned the preference off is skipped at queue time, while a
/// co-reviewer who left it on is still emailed.
#[tokio::test]
async fn a_reviewer_with_the_preference_off_is_skipped() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    sqlx::query("UPDATE users SET email_on_approval_request = false WHERE id = $1")
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();

    f.request(&app, "be").await;
    let queued = deliveries(&app).await;
    assert_eq!(queued.len(), 1, "only the opted-in reviewer is queued");
    assert_eq!(queued[0].0, f.second.email);
    app.cleanup().await;
}

/// The delivery gate never reads the request, so an email queued for a request
/// that is settled before delivery is still sent; the gate needs no authority
/// over tenant approval records.
#[tokio::test]
async fn a_request_settled_before_delivery_is_still_emailed() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    assert_eq!(deliveries(&app).await.len(), 2);

    // The owner cancels the reviewer's pending request before it is delivered.
    let reviewer_request = reviewer_request_id(&app, &f).await;
    let owner = app.authenticated_client(&f.owner);
    let cancel = owner
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/approvals/requests/{}/cancel",
                f.source, reviewer_request
            ),
            &serde_json::json!({}),
        )
        .await;
    assert!(cancel.status().is_success(), "cancel the reviewer request");

    send_due(&app).await;
    let rows = deliveries(&app).await;
    for email in [&f.reviewer.email, &f.second.email] {
        let row = rows.iter().find(|row| &row.0 == email).unwrap();
        assert_eq!(row.1, "sent", "{email}");
    }
    app.cleanup().await;
}

/// The delivery gate cancels a queued email when the reviewer is no longer an
/// active member of the organization.
#[tokio::test]
async fn the_gate_cancels_when_the_reviewer_leaves_the_organization() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    sqlx::query("UPDATE org_memberships SET deactivated_at = now() WHERE user_id = $1")
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();

    send_due(&app).await;
    let rows = deliveries(&app).await;
    let reviewer_row = rows.iter().find(|row| row.0 == f.reviewer.email).unwrap();
    assert_eq!(reviewer_row.1, "cancelled");
    assert_eq!(reviewer_row.2.as_deref(), Some("cancelled:ineligible"));
    let second_row = rows.iter().find(|row| row.0 == f.second.email).unwrap();
    assert_eq!(second_row.1, "sent");
    app.cleanup().await;
}

/// The delivery gate cancels a queued email when the recipient turns the
/// preference off after it was queued.
#[tokio::test]
async fn the_gate_cancels_when_the_preference_is_turned_off() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    sqlx::query("UPDATE users SET email_on_approval_request = false WHERE id = $1")
        .bind(f.reviewer.id)
        .execute(&app.db)
        .await
        .unwrap();

    send_due(&app).await;
    let rows = deliveries(&app).await;
    let reviewer_row = rows.iter().find(|row| row.0 == f.reviewer.email).unwrap();
    assert_eq!(reviewer_row.1, "cancelled");
    assert_eq!(reviewer_row.2.as_deref(), Some("cancelled:setting_off"));
    app.cleanup().await;
}

/// A deployment with email disabled still creates the request with no error to
/// the requester, and records no delivery.
#[tokio::test]
async fn email_disabled_still_creates_the_request() {
    let app = TestApp::spawn().await; // default config: email disabled
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let requests = f.request(&app, "be").await;
    assert_eq!(requests.len(), 2, "requests are created without email");
    assert!(
        deliveries(&app).await.is_empty(),
        "no delivery is recorded when email is disabled"
    );
    app.cleanup().await;
}

/// The pending request id addressed to the reviewer.
async fn reviewer_request_id(app: &TestApp, f: &Flow) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM approval_requests
          WHERE source_workspace_id = $1 AND actor_user_id = $2 AND state = 'pending'",
    )
    .bind(f.source)
    .bind(f.reviewer.id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// The email quotes the requester's note and the PR's canonical description
/// under labels after G5's opening line, and links the Settings page holding
/// the preference. The note stays plain text; the description renders as the
/// Markdown subset, whose links are clickable only into G5 and the
/// organization's own repositories.
#[tokio::test]
async fn the_email_quotes_the_note_and_pull_request_description() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider, provider_full_name) \
         VALUES (gen_random_uuid(), $1, 'acme/api', 'acme-api', 'https://github.com/acme/api.git', 'main', 'github', 'acme/api')",
    )
    .bind(f.org.id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE workspace_pull_requests SET description_override = $2 WHERE source_workspace_id = $1",
    )
    .bind(f.source)
    .bind(
        "## Implementation decisions\n\n**Fixes** [#12](https://github.com/acme/api/issues/12); \
         see [login](https://evil.example/login).",
    )
    .execute(&app.db)
    .await
    .unwrap();
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/approvals/requests", f.source),
            &serde_json::json!({
                "definitionId": f.definition,
                "definitionVersion": 1,
                "phase": "be",
                "actorUserIds": [f.reviewer.id],
                "note": "Please check the <migration> first.",
            }),
        )
        .await;
    assert!(response.status().is_success(), "request with a note");

    send_due(&app).await;
    let captured = app.state.email.capture().unwrap().messages();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    let opening = sent.text.find("requested your approval").unwrap();
    let note = sent
        .text
        .find("Please check the <migration> first.")
        .unwrap();
    let description = sent.text.find("Implementation decisions").unwrap();
    assert!(opening < note && note < description, "{}", sent.text);
    assert!(sent.text.contains("Note from "));
    assert!(sent.text.contains("Pull request description:"));
    assert!(sent
        .text
        .contains("under Notifications in your G5 Settings"));
    assert!(sent.text.contains("/settings\n"), "{}", sent.text);
    assert!(sent
        .html
        .contains("Please check the &lt;migration&gt; first."));
    assert!(!sent.html.contains("<migration>"));
    assert!(
        sent.html.contains("Implementation decisions</h2>"),
        "{}",
        sent.html
    );
    assert!(sent.html.contains("<strong>Fixes</strong>"));
    assert!(sent
        .html
        .contains("href=\"https://github.com/acme/api/issues/12\""));
    assert!(!sent.html.contains("href=\"https://evil.example"));
    assert!(sent.html.contains("login (https://evil.example/login)"));
    // The text part keeps the description's source as written.
    assert!(sent
        .text
        .contains("## Implementation decisions\n\n**Fixes**"));
    app.cleanup().await;
}

/// Pending request of `actor` in the flow, as the overview returns it.
async fn pending_request_of(app: &TestApp, f: &Flow, actor: Uuid) -> serde_json::Value {
    f.overview(app, actor_user(f, actor)).await["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["actorUserId"] == actor.to_string() && r["state"] == "pending")
        .unwrap()
        .clone()
}

fn actor_user(f: &Flow, actor: Uuid) -> &helpers::TestUser {
    [&f.owner, &f.reviewer, &f.second]
        .into_iter()
        .find(|user| user.id == actor)
        .unwrap()
}

/// Record `outcome` on `actor`'s pending request and assert it was accepted.
async fn decide(app: &TestApp, f: &Flow, actor: Uuid, outcome: &str) -> serde_json::Value {
    let request = pending_request_of(app, f, actor).await;
    let response = f
        .decision(app, actor_user(f, actor), &request, outcome)
        .await;
    assert!(response.status().is_success(), "{outcome} decision");
    request
}

/// Approving and requesting changes each email the workspace owner once, from
/// the approvals sender with the shared subject prefix and the reviewer's
/// comment; replaying a decision sends nothing more.
#[tokio::test]
async fn each_decision_emails_the_owner_once() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    let approved = decide(&app, &f, f.reviewer.id, "approved").await;
    decide(&app, &f, f.second.id, "rejected").await;

    let queued = decision_deliveries(&app).await;
    assert_eq!(queued.len(), 2, "one email per settled request");
    assert!(queued.iter().all(|row| row.0 == f.owner.email));

    // A lost-acknowledgement retry of the same decision coalesces.
    let replay = f.decision(&app, &f.reviewer, &approved, "approved").await;
    assert!(replay.status().is_success(), "replayed decision");
    assert_eq!(decision_deliveries(&app).await.len(), 2);

    send_due(&app).await;
    // Replaying after delivery still sends nothing more (single-use key).
    let replay = f.decision(&app, &f.reviewer, &approved, "approved").await;
    assert!(
        replay.status().is_success(),
        "replayed decision after delivery"
    );
    assert_eq!(decision_deliveries(&app).await.len(), 2);

    let captured = app.state.email.capture().unwrap().messages();
    let decisions: Vec<_> = captured
        .iter()
        .filter(|sent| sent.recipient == f.owner.email)
        .collect();
    assert_eq!(decisions.len(), 2);
    for sent in &decisions {
        assert_eq!(
            sent.sender,
            format!("G5 Approvals <approvals@{SENDER_DOMAIN}>")
        );
        assert!(sent.text.contains("Comment from "), "{}", sent.text);
        assert!(sent.text.contains("Billing scope confirmed."));
        assert!(sent.text.contains("on your G5 workspace review-flow"));
    }
    let subjects: Vec<&str> = decisions.iter().map(|sent| sent.subject.as_str()).collect();
    assert!(subjects
        .iter()
        .any(|s| s.starts_with("[G5 Approval] review-flow: Security review approved by ")));
    assert!(
        subjects
            .iter()
            .any(|s| s
                .starts_with("[G5 Approval] review-flow: Security review changes requested by "))
    );
    app.cleanup().await;
}

/// A withdrawal settles the request but is not emailed.
#[tokio::test]
async fn a_withdrawal_sends_nothing() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    decide(&app, &f, f.reviewer.id, "deferred").await;
    assert!(decision_deliveries(&app).await.is_empty());
    app.cleanup().await;
}

/// The owner deciding their own request, where self approval is allowed, is
/// not emailed to them.
#[tokio::test]
async fn the_owners_own_decision_sends_nothing() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let client = app.authenticated_client(&f.owner);
    let published = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/approval-definitions/{}/versions",
                f.org.id, f.definition
            ),
            &serde_json::json!({"name":"Security review", "humanUserIds":[f.owner.id, f.reviewer.id],
                "phases":["be","se","ie"], "allowSelfApproval":true}),
        )
        .await;
    assert!(published.status().is_success(), "allow self approval");
    let requested = client
        .post_json(
            &f.path("approvals/requests"),
            &serde_json::json!({"definitionId":f.definition,"definitionVersion":2,
                "phase":"be","actorUserIds":[f.owner.id]}),
        )
        .await;
    assert!(requested.status().is_success(), "self request");
    decide(&app, &f, f.owner.id, "approved").await;
    assert!(decision_deliveries(&app).await.is_empty());
    app.cleanup().await;
}

/// An owner with the shared approval-email preference off is not emailed, and
/// one who turns it off before delivery has the queued email cancelled.
#[tokio::test]
async fn the_owners_preference_governs_decision_emails() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    decide(&app, &f, f.reviewer.id, "approved").await;
    assert_eq!(decision_deliveries(&app).await.len(), 1);

    sqlx::query("UPDATE users SET email_on_approval_request = false WHERE id = $1")
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    decide(&app, &f, f.second.id, "approved").await;
    let queued = decision_deliveries(&app).await;
    assert_eq!(
        queued.len(),
        1,
        "skipped at queue time once the setting is off"
    );

    send_due(&app).await;
    let queued = decision_deliveries(&app).await;
    assert_eq!(queued[0].1, "cancelled");
    assert_eq!(queued[0].2.as_deref(), Some("cancelled:setting_off"));
    app.cleanup().await;
}

/// The delivery gate cancels a queued decision email once the owner's account
/// email has changed, re-checking the recipient without reading the request.
/// (Membership is re-checked by the same gate, covered for reviewers above;
/// the flow's owner also owns the organization and cannot be deactivated.)
#[tokio::test]
async fn the_gate_cancels_when_the_owners_email_changes() {
    let app = TestApp::spawn_with_config(capture_config()).await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    decide(&app, &f, f.reviewer.id, "rejected").await;
    sqlx::query("UPDATE users SET email = 'moved-' || email WHERE id = $1")
        .bind(f.owner.id)
        .execute(&app.db)
        .await
        .unwrap();

    send_due(&app).await;
    let queued = decision_deliveries(&app).await;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].1, "cancelled");
    assert_eq!(queued[0].2.as_deref(), Some("cancelled:recipient_changed"));
    app.cleanup().await;
}

/// A deployment with email disabled still records the decision.
#[tokio::test]
async fn email_disabled_still_records_the_decision() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.request(&app, "be").await;
    decide(&app, &f, f.reviewer.id, "approved").await;
    assert!(decision_deliveries(&app).await.is_empty());
    let view = f.overview(&app, &f.owner).await;
    assert_eq!(view["decisions"].as_array().unwrap().len(), 1);
    app.cleanup().await;
}
