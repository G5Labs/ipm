//! Integration tests for API-key CRUD routes.
//!
//! These routes must operate through the tenant-scoped authz session path, so
//! successful create/list/revoke coverage here catches RLS drift between route
//! auth context and table policies.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};

fn response_data(body: &Value) -> &Value {
    body.get("data")
        .expect("response body should include a top-level data field")
}

#[tokio::test]
async fn test_api_key_routes_support_tenant_scoped_crud() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);

    let create_response = client
        .post_json(
            &format!("/api/v1/orgs/{}/api-keys", org.id),
            &json!({
                "name": "Tenant-scoped API key",
                "scopes": ["workspaces:read"],
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);

    let create_body: Value = create_response
        .json()
        .await
        .expect("create api key response should be valid JSON");
    let api_key_data = response_data(&create_body);
    let api_key_id = api_key_data
        .pointer("/apiKey/id")
        .and_then(Value::as_str)
        .expect("create response should include apiKey.id")
        .to_string();
    let key = api_key_data
        .get("key")
        .and_then(Value::as_str)
        .expect("create response should include plaintext key");
    assert!(key.starts_with("g5k_"), "API key should use g5k_ prefix");

    let list_response = client
        .get(&format!("/api/v1/orgs/{}/api-keys", org.id))
        .await;
    assert_eq!(list_response.status(), StatusCode::OK);

    let list_body: Value = list_response
        .json()
        .await
        .expect("list api keys response should be valid JSON");
    let api_keys = response_data(&list_body)
        .as_array()
        .expect("list api keys should return an array");
    assert!(
        api_keys.iter().any(|api_key| {
            api_key.get("id").and_then(Value::as_str) == Some(api_key_id.as_str())
                && api_key.get("revokedAt").and_then(Value::as_str).is_none()
        }),
        "list response should include the newly-created active API key"
    );

    let revoke_response = client
        .delete(&format!("/api/v1/orgs/{}/api-keys/{}", org.id, api_key_id))
        .await;
    assert_eq!(revoke_response.status(), StatusCode::OK);

    let list_after_revoke = client
        .get(&format!("/api/v1/orgs/{}/api-keys", org.id))
        .await;
    assert_eq!(list_after_revoke.status(), StatusCode::OK);

    let list_after_revoke_body: Value = list_after_revoke
        .json()
        .await
        .expect("post-revoke api key list response should be valid JSON");
    let api_keys_after_revoke = response_data(&list_after_revoke_body)
        .as_array()
        .expect("post-revoke api key list should return an array");
    assert!(
        api_keys_after_revoke.iter().any(|api_key| {
            api_key.get("id").and_then(Value::as_str) == Some(api_key_id.as_str())
                && api_key.get("revokedAt").and_then(Value::as_str).is_some()
        }),
        "revoked API key should remain visible to the tenant with revokedAt set"
    );

    app.cleanup().await;
}
