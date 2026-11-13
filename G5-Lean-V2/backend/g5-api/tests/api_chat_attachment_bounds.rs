//! Internal attachment uploads bound the HTTP body before JSON extraction,
//! while retaining the 5 MiB decoded limit and live-incarnation checks (#541).

use crate::helpers;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use bytes::Bytes;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::{Body, StatusCode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_ATTACHMENT_BYTES: usize = 5 * 1024 * 1024;
const UPLOAD_BODY_LIMIT_BYTES: usize = MAX_ATTACHMENT_BYTES.div_ceil(3) * 4 + 64 * 1024;

struct Fixture {
    app: TestApp,
    token: String,
    workspace_id: Uuid,
    session_id: Uuid,
    message_id: Uuid,
}

impl Fixture {
    async fn new() -> Self {
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let host_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO worker_hosts(host_id, org_id, region, status, last_heartbeat)
             VALUES ($1,$2,'us-central1','ready',now())",
        )
        .bind(host_id)
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO workspace_vms(org_id, workspace_id, host_id, vm_id, epoch, status, lease_expires_at)
             VALUES ($1,$2,$3,$4,1,'running',now() + interval '1 hour')",
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(host_id)
        .bind(vm_id.to_string())
        .execute(&app.db)
        .await
        .unwrap();
        let token = app
            .state
            .jwt_minter
            .as_ref()
            .unwrap()
            .mint_workspace_incarnation(org.id, workspace.id, owner.id, vm_id, 1, 3600)
            .unwrap();
        let session_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO chat_sessions(id, org_id, workspace_id, created_by)
             VALUES ($1,$2,$3,$4)",
        )
        .bind(session_id)
        .bind(org.id)
        .bind(workspace.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO chat_messages(id, org_id, session_id, role, content, message_index)
             VALUES ($1,$2,$3,'assistant','Screenshot',0)",
        )
        .bind(message_id)
        .bind(org.id)
        .bind(session_id)
        .execute(&app.db)
        .await
        .unwrap();
        Self {
            app,
            token,
            workspace_id: workspace.id,
            session_id,
            message_id,
        }
    }

    fn payload(&self, content_base64: &str) -> Value {
        json!({
            "messageId": self.message_id,
            "fileName": "screenshot.png",
            "contentType": "image/png",
            "contentBase64": content_base64,
        })
    }

    async fn upload(&self, session_id: Uuid, body: Vec<u8>, chunked: bool) -> reqwest::Response {
        let body = if chunked {
            // No Content-Length: the server must count received bytes too.
            let chunks = Bytes::from(body);
            let stream = futures_util::stream::unfold(chunks, |mut remaining| async move {
                if remaining.is_empty() {
                    None
                } else {
                    let chunk = remaining.split_to(remaining.len().min(16 * 1024));
                    Some((Ok::<_, std::io::Error>(chunk), remaining))
                }
            });
            Body::wrap_stream(stream)
        } else {
            Body::from(body)
        };
        self.app
            .client
            .post(format!(
                "{}/api/internal/chat-sessions/{session_id}/attachments",
                self.app.base_url
            ))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .expect("upload response")
    }

    async fn attachment_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM chat_attachments WHERE session_id = $1")
            .bind(self.session_id)
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn oversized_agent_attachment_json_is_rejected_before_validation() {
    let fixture = Fixture::new().await;
    let oversized = serde_json::to_vec(&fixture.payload(&"A".repeat(8 * 1024 * 1024))).unwrap();
    for chunked in [false, true] {
        let response = fixture
            .upload(fixture.session_id, oversized.clone(), chunked)
            .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(
            status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "chunked={chunked}: {body}"
        );
        assert_eq!(fixture.attachment_count().await, 0);

        // Broken JSON and a nonexistent session would otherwise produce a
        // parse or session error. The body limit must reject before either.
        let response = fixture
            .upload(
                Uuid::new_v4(),
                oversized[..oversized.len() - 1].to_vec(),
                chunked,
            )
            .await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(fixture.attachment_count().await, 0);
    }
    fixture.app.cleanup().await;
}

#[tokio::test]
async fn agent_attachment_body_limit_includes_json_envelope_and_is_inclusive() {
    let fixture = Fixture::new().await;
    let mut payload = fixture.payload("AQID");
    payload["ignored"] = json!("");
    let envelope_size = serde_json::to_vec(&payload).unwrap().len();
    payload["ignored"] = json!("x".repeat(UPLOAD_BODY_LIMIT_BYTES - envelope_size));
    let mut body = serde_json::to_vec(&payload).unwrap();
    assert_eq!(body.len(), UPLOAD_BODY_LIMIT_BYTES);

    let response = fixture.upload(fixture.session_id, body.clone(), true).await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", response.text().await.unwrap());
    assert_eq!(fixture.attachment_count().await, 1);

    // One more byte of valid JSON whitespace crosses the transport limit,
    // even though the attachment content itself is only three bytes.
    body.push(b' ');
    let response = fixture.upload(fixture.session_id, body, true).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(fixture.attachment_count().await, 1);
    fixture.app.cleanup().await;
}

#[tokio::test]
async fn maximum_sized_agent_attachment_upload_and_download_succeed() {
    let fixture = Fixture::new().await;
    let content = vec![0xab; MAX_ATTACHMENT_BYTES];
    let encoded = BASE64.encode(&content);
    let response = fixture
        .upload(
            fixture.session_id,
            serde_json::to_vec(&fixture.payload(&encoded)).unwrap(),
            false,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let uploaded: Value = response.json().await.unwrap();
    let attachment_id = uploaded["attachmentId"].as_str().unwrap();
    assert_eq!(uploaded["sizeBytes"], MAX_ATTACHMENT_BYTES);
    assert_eq!(
        uploaded["sha256"],
        format!("{:x}", Sha256::digest(&content))
    );
    assert_eq!(fixture.attachment_count().await, 1);

    let response = fixture
        .app
        .client
        .get(format!(
            "{}/api/internal/chat-attachments/{attachment_id}/content",
            fixture.app.base_url
        ))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let downloaded: Value = response.json().await.unwrap();
    assert_eq!(downloaded["contentBase64"].as_str().unwrap(), encoded);
    assert_eq!(downloaded["sha256"], uploaded["sha256"]);
    assert_eq!(downloaded["sizeBytes"], MAX_ATTACHMENT_BYTES);
    fixture.app.cleanup().await;
}

#[tokio::test]
async fn bounded_agent_attachment_upload_retains_semantic_and_incarnation_checks() {
    let fixture = Fixture::new().await;
    for decoded_size in [MAX_ATTACHMENT_BYTES + 1, MAX_ATTACHMENT_BYTES + 4] {
        // Both fit the HTTP limit; one extra decoded byte also fits the
        // encoded cap, so it exercises the independent decoded-size check.
        let body =
            serde_json::to_vec(&fixture.payload(&BASE64.encode(vec![0; decoded_size]))).unwrap();
        assert!(body.len() < UPLOAD_BODY_LIMIT_BYTES);
        let response = fixture.upload(fixture.session_id, body, false).await;
        let status = response.status();
        let error: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
        assert_eq!(error["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(fixture.attachment_count().await, 0);
    }

    sqlx::query("UPDATE workspace_vms SET epoch = epoch + 1 WHERE workspace_id = $1")
        .bind(fixture.workspace_id)
        .execute(&fixture.app.db)
        .await
        .unwrap();
    let response = fixture
        .upload(
            fixture.session_id,
            serde_json::to_vec(&fixture.payload("AQID")).unwrap(),
            false,
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.attachment_count().await, 0);
    fixture.app.cleanup().await;
}
