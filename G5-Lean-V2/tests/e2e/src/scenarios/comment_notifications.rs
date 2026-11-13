//! E2E tests for Comment, Reply, @Mention, and Notification subsystem.
//!
//! Tests cover:
//! 1. Comment CRUD (create, list, update/resolve, delete)
//! 2. Reply threading (add reply, list replies)
//! 3. @Mention notification creation and delivery
//! 4. Notification REST API (list, mark-read, mark-all-read)
//! 5. Comment resolution via PATCH (contract alignment)
//!
//! Requires `G5_E2E_AUTH=true` on the g5-api to obtain test JWTs.

use anyhow::Context;
use uuid::Uuid;

use crate::helpers::api_client::TestApiClient;

const TEST_LINE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const TEST_CONTENT_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const TEST_ANCHOR_CONTEXT: &str = "{\"before\":[],\"line\":\"test\",\"after\":[]}";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a test API client with a user JWT.
///
/// Uses `obtain_test_tokens` so both the access *and* refresh tokens belong
/// to the test-token user.  Without this, `switch_active_org` (called by
/// `create_org`) would 403 because the refresh token still belonged to the
/// OAuth stub user from `from_env_authed`.
async fn authed_client(email: &str) -> anyhow::Result<TestApiClient> {
    let api = TestApiClient::from_env_authed().await?;
    let (access, refresh) = api.obtain_test_tokens(email).await?;
    Ok(api.with_auth_tokens(access, refresh))
}

/// Extract data[0].id from a list response envelope.
fn first_id(resp: &serde_json::Value) -> Option<Uuid> {
    resp["data"][0]["id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
}

/// Extract data.id from a single-item response envelope.
fn data_id(resp: &serde_json::Value) -> Option<Uuid> {
    resp["data"]["id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
}

// ---------------------------------------------------------------------------
// 1. Comment CRUD Lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_comment_create_list_resolve_delete() -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let api = TestApiClient::from_env_authed().await?;
        crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

        // Attempt to get user tokens — skip if test-token endpoint is not available
        let (access, refresh) = match api.obtain_test_tokens("comment-test@g5.dev").await {
            Ok(t) => t,
            Err(_) => {
                eprintln!("SKIP: G5_E2E_AUTH not enabled (no test-token endpoint)");
                return Ok(());
            }
        };
        let api = api.with_auth_tokens(access, refresh);

        // Create org + workspace
        let org = api
            .create_org("comment-e2e")
            .await
            .context("comment lifecycle: create_org")?;
        let repo_id = api.get_docs_repo_id(org.id).await?;
        let ws = api
            .create_workspace(org.id, "comment-ws")
            .await
            .context("comment lifecycle: create_workspace")?;
        let ws_id = ws.id;

        // 1. Create a node-scoped comment (no real node — should fail with NotFound)
        let fake_node = Uuid::new_v4();
        let create_result = api
            .create_comment(ws_id, Some(fake_node), "Hello world", None)
            .await;

        // Expect failure because the node doesn't exist in this workspace
        assert!(create_result.is_err(), "Expected NotFound for fake node_id");

        // 2. Create a line-scoped comment on a file (no node required)
        let body = serde_json::json!({
            "scope": "line",
            "content": "This line needs review",
            "repoId": repo_id,
            "filePath": "src/main.rs",
            "lineNumber": 42,
            "lineHash": TEST_LINE_HASH,
            "contentHash": TEST_CONTENT_HASH,
            "anchorContext": TEST_ANCHOR_CONTEXT,
        });
        let resp = api
            .authed_post(&format!(
                "{}/api/v1/workspaces/{}/comments",
                api.base_url(),
                ws_id
            ))
            .json(&body)
            .send()
            .await
            .context("comment lifecycle: create line comment request")?
            .error_for_status()
            .context("comment lifecycle: create line comment response status")?;
        let create_resp: serde_json::Value = resp.json().await?;
        let comment_id = data_id(&create_resp).expect("comment should have an id");

        // 3. List comments
        let list_resp = api.list_comments(ws_id, None).await?;
        let comments = list_resp["data"].as_array().expect("data should be array");
        assert!(!comments.is_empty(), "should have at least one comment");
        assert!(
            comments
                .iter()
                .any(|c| { c["id"].as_str() == Some(&comment_id.to_string()) }),
            "created comment should appear in listing"
        );

        // 4. Resolve the comment via PATCH
        let update_resp = api
            .update_comment(comment_id, serde_json::json!({ "resolved": true }))
            .await?;
        assert_eq!(
            update_resp["data"]["resolved"].as_bool(),
            Some(true),
            "comment should be resolved"
        );

        // 5. Delete the comment
        let delete_resp = api.delete_comment(comment_id).await?;
        assert_eq!(
            delete_resp["data"]["deleted"].as_bool(),
            Some(true),
            "delete should return deleted=true"
        );

        // 6. Verify it's gone
        let list_resp2 = api.list_comments(ws_id, None).await?;
        let comments2 = list_resp2["data"].as_array().expect("data should be array");
        assert!(
            !comments2
                .iter()
                .any(|c| { c["id"].as_str() == Some(&comment_id.to_string()) }),
            "deleted comment should not appear in listing"
        );

        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("comment lifecycle scenario timed out after 120s")?
}

// ---------------------------------------------------------------------------
// 2. Reply Threading
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_reply_add_and_list() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let tokens = match api.obtain_test_tokens("reply-test@g5.dev").await {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SKIP: G5_E2E_AUTH not enabled");
            return Ok(());
        }
    };
    let api = api.with_auth_tokens(tokens.0, tokens.1);

    let org = api.create_org("reply-e2e").await?;
    let repo_id = api.get_docs_repo_id(org.id).await?;
    let ws = api.create_workspace(org.id, "reply-ws").await?;
    let ws_id = ws.id;

    // Create a line comment
    let body = serde_json::json!({
        "scope": "line",
        "content": "Original comment",
        "repoId": repo_id,
        "filePath": "src/lib.rs",
        "lineNumber": 10,
        "lineHash": TEST_LINE_HASH,
        "contentHash": TEST_CONTENT_HASH,
        "anchorContext": TEST_ANCHOR_CONTEXT,
    });
    let resp = api
        .authed_post(&format!(
            "{}/api/v1/workspaces/{}/comments",
            api.base_url(),
            ws_id
        ))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let create_resp: serde_json::Value = resp.json().await?;
    let comment_id = data_id(&create_resp).expect("comment id");

    // Add two replies
    let reply1 = api.add_reply(comment_id, "First reply", None).await?;
    let reply1_id = data_id(&reply1).expect("reply1 id");

    let reply2 = api.add_reply(comment_id, "Second reply", None).await?;
    let reply2_id = data_id(&reply2).expect("reply2 id");

    assert_ne!(reply1_id, reply2_id, "replies should have different IDs");

    // List replies
    let replies_resp = api.get_replies(comment_id).await?;
    let replies = replies_resp["data"]
        .as_array()
        .expect("data should be array");
    assert_eq!(replies.len(), 2, "should have exactly 2 replies");

    // Verify ordering (oldest first)
    assert_eq!(
        replies[0]["content"].as_str(),
        Some("First reply"),
        "first reply should come first"
    );
    assert_eq!(
        replies[1]["content"].as_str(),
        Some("Second reply"),
        "second reply should come second"
    );

    // Verify parent comment reply count updated
    let list_resp = api.list_comments(ws_id, None).await?;
    let comments = list_resp["data"].as_array().unwrap();
    let our_comment = comments
        .iter()
        .find(|c| c["id"].as_str() == Some(&comment_id.to_string()))
        .expect("comment should exist");
    assert_eq!(
        our_comment["replyCount"].as_i64(),
        Some(2),
        "reply count should be 2"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Notification CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notification_list_and_mark_read() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let tokens = match api.obtain_test_tokens("notif-test@g5.dev").await {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SKIP: G5_E2E_AUTH not enabled");
            return Ok(());
        }
    };
    let api = api.with_auth_tokens(tokens.0, tokens.1);

    let org = api.create_org("notif-e2e").await?;
    let ws = api.create_workspace(org.id, "notif-ws").await?;
    let ws_id = ws.id;

    // List notifications (should be empty for new workspace)
    let list_resp = api.list_notifications(ws_id, false).await?;
    let notifications = list_resp["data"].as_array().expect("data should be array");
    assert!(
        notifications.is_empty(),
        "new workspace should have no notifications"
    );

    // Mark-all-read on empty workspace should succeed
    let mark_all = api.mark_all_notifications_read(ws_id).await?;
    assert_eq!(
        mark_all["data"]["updated"].as_u64(),
        Some(0),
        "mark-all-read with no notifications should return 0"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 4. @Mention triggers notification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mention_in_comment_creates_notification() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    // Create two users
    let (token_a, refresh_a) = match api.obtain_test_tokens("alice-mention@g5.dev").await {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SKIP: G5_E2E_AUTH not enabled");
            return Ok(());
        }
    };
    let (token_b, refresh_b) = api.obtain_test_tokens("bob-mention@g5.dev").await?;

    let api_alice = TestApiClient::from_env_authed()
        .await?
        .with_auth_tokens(token_a, refresh_a);
    let api_bob = TestApiClient::from_env_authed()
        .await?
        .with_auth_tokens(token_b, refresh_b);

    // Alice creates org + workspace
    let org = api_alice.create_org("mention-e2e").await?;
    let repo_id = api_alice.get_docs_repo_id(org.id).await?;
    let ws = api_alice.create_workspace(org.id, "mention-ws").await?;
    let ws_id = ws.id;

    api_alice
        .authed_post(&format!(
            "{}/api/v1/orgs/{}/members",
            api_alice.base_url(),
            org.id
        ))
        .json(&serde_json::json!({
            "email": "bob-mention@g5.dev",
            "role": "member",
        }))
        .send()
        .await?
        .error_for_status()?;
    api_bob.switch_active_org(org.id).await?;

    // Get Bob's user ID from the /api/v1/users/me endpoint
    let bob_me_resp = api_bob
        .authed_get(&format!("{}/api/v1/users/me", api_bob.base_url()))
        .send()
        .await?;

    // If users/me endpoint doesn't work, skip the test
    if !bob_me_resp.status().is_success() {
        eprintln!("SKIP: /api/v1/users/me not available");
        return Ok(());
    }

    let bob_me: serde_json::Value = bob_me_resp.json().await?;
    let bob_id_str = bob_me["data"]["id"].as_str();
    if bob_id_str.is_none() {
        eprintln!("SKIP: Could not determine Bob's user ID");
        return Ok(());
    }
    let bob_id: Uuid = bob_id_str.unwrap().parse()?;

    // Alice creates a comment mentioning Bob (using explicit IDs)
    let body = serde_json::json!({
        "scope": "line",
        "content": "Hey @bob-mention, please review this",
        "repoId": repo_id,
        "filePath": "src/lib.rs",
        "lineNumber": 5,
        "lineHash": TEST_LINE_HASH,
        "contentHash": TEST_CONTENT_HASH,
        "anchorContext": TEST_ANCHOR_CONTEXT,
        "mentionUserIds": [bob_id],
    });
    let create_resp = api_alice
        .authed_post(&format!(
            "{}/api/v1/workspaces/{}/comments",
            api_alice.base_url(),
            ws_id
        ))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let _create_data: serde_json::Value = create_resp.json().await?;

    let notifications = api_bob
        .poll_until(
            || {
                let api_bob = api_bob.clone();
                Box::pin(async move {
                    let resp = api_bob.list_notifications(ws_id, false).await?;
                    let notifications = resp["data"].as_array().cloned().unwrap_or_default();
                    Ok((!notifications.is_empty()).then_some(notifications))
                })
            },
            10,
        )
        .await?;

    let n = &notifications[0];
    assert_eq!(
        n["category"].as_str(),
        Some("mention"),
        "notification category should be 'mention'"
    );
    assert!(
        n["title"].as_str().unwrap_or("").contains("mentioned"),
        "notification title should mention 'mentioned'"
    );

    let notif_id: Uuid = n["id"].as_str().unwrap().parse()?;
    let mark_resp = api_bob.mark_notification_read(notif_id).await?;
    assert_eq!(
        mark_resp["data"]["read"].as_bool(),
        Some(true),
        "mark-read should succeed"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Comment resolution via PATCH (contract test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_comment_resolve_via_patch() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let tokens = match api.obtain_test_tokens("resolve-test@g5.dev").await {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SKIP: G5_E2E_AUTH not enabled");
            return Ok(());
        }
    };
    let api = api.with_auth_tokens(tokens.0, tokens.1);

    let org = api.create_org("resolve-e2e").await?;
    let repo_id = api.get_docs_repo_id(org.id).await?;
    let ws = api.create_workspace(org.id, "resolve-ws").await?;
    let ws_id = ws.id;

    // Create comment
    let body = serde_json::json!({
        "scope": "line",
        "content": "Needs fix",
        "repoId": repo_id,
        "filePath": "src/bug.rs",
        "lineNumber": 1,
        "lineHash": TEST_LINE_HASH,
        "contentHash": TEST_CONTENT_HASH,
        "anchorContext": TEST_ANCHOR_CONTEXT,
    });
    let resp = api
        .authed_post(&format!(
            "{}/api/v1/workspaces/{}/comments",
            api.base_url(),
            ws_id
        ))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let create_resp: serde_json::Value = resp.json().await?;
    let comment_id = data_id(&create_resp).expect("comment id");

    // Resolve via PATCH (not POST /resolve)
    let patch_resp = api
        .update_comment(comment_id, serde_json::json!({ "resolved": true }))
        .await?;
    assert_eq!(
        patch_resp["data"]["resolved"].as_bool(),
        Some(true),
        "resolved should be true after PATCH"
    );
    assert!(
        patch_resp["data"]["resolvedAt"].as_str().is_some(),
        "resolvedAt should be set"
    );

    // Unresolve
    let unresolve_resp = api
        .update_comment(comment_id, serde_json::json!({ "resolved": false }))
        .await?;
    assert_eq!(
        unresolve_resp["data"]["resolved"].as_bool(),
        Some(false),
        "resolved should be false after unresolve"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Reply content validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_reply_empty_content_rejected() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let tokens = match api.obtain_test_tokens("empty-reply@g5.dev").await {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SKIP: G5_E2E_AUTH not enabled");
            return Ok(());
        }
    };
    let api = api.with_auth_tokens(tokens.0, tokens.1);

    let org = api.create_org("empty-reply-e2e").await?;
    let repo_id = api.get_docs_repo_id(org.id).await?;
    let ws = api.create_workspace(org.id, "empty-reply-ws").await?;
    let ws_id = ws.id;

    // Create a comment
    let body = serde_json::json!({
        "scope": "line",
        "content": "Test comment",
        "repoId": repo_id,
        "filePath": "src/test.rs",
        "lineNumber": 1,
        "lineHash": TEST_LINE_HASH,
        "contentHash": TEST_CONTENT_HASH,
        "anchorContext": TEST_ANCHOR_CONTEXT,
    });
    let resp = api
        .authed_post(&format!(
            "{}/api/v1/workspaces/{}/comments",
            api.base_url(),
            ws_id
        ))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let create_resp: serde_json::Value = resp.json().await?;
    let comment_id = data_id(&create_resp).expect("comment id");

    // Try to add an empty reply — should fail
    let reply_result = api.add_reply(comment_id, "   ", None).await;
    assert!(
        reply_result.is_err(),
        "Empty reply content should be rejected"
    );

    Ok(())
}
