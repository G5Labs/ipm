//! The zstd body boundary of the provider session-file routes (design v9
//! §8.4): `PUT /api/internal/chat-sessions/:id/session-files` decodes the
//! uploaded frame before anything is stored, and
//! `GET .../session-files/content` serves the stored raw bytes as a frame the
//! worker decodes. A body that expands past the 128 MiB cap answers 413, a
//! body that is not one well-formed zstd stream answers 422, and neither
//! stores a row; a well-formed upload round-trips with its digest and size.

use crate::helpers;

use g5_internal_api::session_files::{
    decode_session_file_body, encode_session_file_body, sha256_hex, CHAT_SESSION_FILE_MAX_BYTES,
    SESSION_FILE_CONTENT_TYPE, SESSION_FILE_PROVIDER_HEADER, SESSION_FILE_SHA256_HEADER,
    SESSION_FILE_SIZE_HEADER,
};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

const PROVIDER_SESSION_ID: &str = "codex-thread-1";
const RELATIVE_PATH: &str = "sessions/2026/09/15/rollout-x-codex-thread-1.jsonl";
const ZSTD_FRAME_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

struct Fixture {
    app: TestApp,
    token: String,
    session_id: Uuid,
}

impl Fixture {
    /// A live workspace incarnation (worker host, running VM, incarnation
    /// token) and one chat session in its workspace: the fence every
    /// session-file route checks before touching the body.
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
        Self {
            app,
            token,
            session_id,
        }
    }

    fn query(pairs: &[(&str, &str)]) -> String {
        let mut url = reqwest::Url::parse("http://unused/").unwrap();
        url.query_pairs_mut().extend_pairs(pairs);
        url.query().unwrap_or_default().to_string()
    }

    /// `PUT` one session-file body under the declared digest, as the worker
    /// does after a turn (a one-version-back worker: no bundle generation).
    async fn put(&self, sha256: &str, body: Vec<u8>) -> reqwest::Response {
        let query = Self::query(&[
            ("provider", "codex"),
            ("provider_session_id", PROVIDER_SESSION_ID),
            ("path", RELATIVE_PATH),
            ("sha256", sha256),
        ]);
        self.app
            .client
            .put(format!(
                "{}/api/internal/chat-sessions/{}/session-files?{query}",
                self.app.base_url, self.session_id
            ))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, SESSION_FILE_CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .expect("session-file upload response")
    }

    async fn content(&self) -> reqwest::Response {
        let query = Self::query(&[
            ("provider_session_id", PROVIDER_SESSION_ID),
            ("path", RELATIVE_PATH),
        ]);
        self.app
            .client
            .get(format!(
                "{}/api/internal/chat-sessions/{}/session-files/content?{query}",
                self.app.base_url, self.session_id
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("session-file content response")
    }

    async fn stored_row_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM chat_session_files WHERE session_id = $1")
            .bind(self.session_id)
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }
}

async fn error_json(response: reqwest::Response, expected: StatusCode) -> Value {
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, expected, "unexpected response body: {body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn a_body_that_expands_past_the_cap_is_refused_with_413_before_anything_is_stored() {
    let fixture = Fixture::new().await;

    // A frame of a few KiB that decompresses to one byte over the cap.
    let body =
        encode_session_file_body(&vec![0u8; CHAT_SESSION_FILE_MAX_BYTES as usize + 1]).unwrap();
    assert!(
        body.len() < 64 * 1024,
        "oversized frame is {} bytes",
        body.len()
    );
    let response = fixture.put(&"0".repeat(64), body).await;
    let error = error_json(response, StatusCode::PAYLOAD_TOO_LARGE).await;
    assert_eq!(error["error"]["code"], "SESSION_FILE_TOO_LARGE");
    assert_eq!(
        error["error"]["details"]["limitBytes"],
        CHAT_SESSION_FILE_MAX_BYTES
    );
    assert_eq!(fixture.stored_row_count().await, 0);

    fixture.app.cleanup().await;
}

#[tokio::test]
async fn a_body_that_is_not_one_well_formed_zstd_stream_is_refused_with_422() {
    let fixture = Fixture::new().await;
    let bytes = b"{\"type\":\"session_meta\"}\n".repeat(64);
    let frame = encode_session_file_body(&bytes).unwrap();
    let mut trailing = frame.clone();
    trailing.extend_from_slice(b"not a zstd frame");
    for (label, body) in [
        ("plain bytes", b"plain".to_vec()),
        ("truncated frame", frame[..frame.len() / 2].to_vec()),
        ("frame with trailing garbage", trailing),
        ("empty body", Vec::new()),
    ] {
        let response = fixture.put(&sha256_hex(&bytes), body).await;
        let error = error_json(response, StatusCode::UNPROCESSABLE_ENTITY).await;
        assert_eq!(
            error["error"]["details"]["code"], "session_file_body_invalid",
            "{label}: {error}"
        );
        assert_eq!(error["error"]["details"]["field"], "body", "{label}");
    }
    assert_eq!(fixture.stored_row_count().await, 0);

    fixture.app.cleanup().await;
}

#[tokio::test]
async fn a_well_formed_upload_round_trips_through_content_with_its_digest_and_size() {
    let fixture = Fixture::new().await;
    let bytes = b"{\"type\":\"session_meta\",\"id\":\"codex-thread-1\"}\n{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":\"Resume where we left off.\"}}\n".to_vec();
    let sha256 = sha256_hex(&bytes);

    let response = fixture
        .put(&sha256, encode_session_file_body(&bytes).unwrap())
        .await;
    let status = response.status();
    let accepted: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["changed"], true);
    assert_eq!(accepted["size"], bytes.len());
    assert_eq!(accepted["sha256"], sha256);
    assert_eq!(accepted["relativePath"], RELATIVE_PATH);
    assert_eq!(fixture.stored_row_count().await, 1);

    let response = fixture.content().await;
    assert_eq!(response.status(), StatusCode::OK);
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .unwrap_or_else(|| panic!("missing {name} header"))
    };
    assert_eq!(
        header(reqwest::header::CONTENT_TYPE.as_str()),
        SESSION_FILE_CONTENT_TYPE
    );
    assert_eq!(header(SESSION_FILE_PROVIDER_HEADER), "codex");
    assert_eq!(header(SESSION_FILE_SHA256_HEADER), sha256);
    assert_eq!(header(SESSION_FILE_SIZE_HEADER), bytes.len().to_string());
    let body = response.bytes().await.unwrap();
    assert_eq!(
        &body[..4],
        &ZSTD_FRAME_MAGIC,
        "content is served as a zstd frame"
    );
    assert_eq!(decode_session_file_body(&body).unwrap(), bytes);

    fixture.app.cleanup().await;
}
