//! Default deployment capabilities must agree with the shipped UI.
use crate::helpers;

use helpers::{EnvVarGuard, TestApp};
use reqwest::StatusCode;
use serde_json::Value;

#[tokio::test]
#[serial_test::serial]
async fn standard_features_are_advertised_even_with_retired_off_settings() {
    let _legacy_settings = EnvVarGuard::set_many(&[
        ("G5_CODEX_UI_ENABLED", "false"),
        ("G5_MAKE_REMOTE_ENABLED", "false"),
    ]);
    let app = TestApp::spawn().await;
    let response = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .expect("instance info");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("instance info JSON");
    let features = body["data"]["features"].as_array().expect("feature list");
    for feature in ["codex_ui", "make_remote"] {
        assert!(
            features.iter().any(|value| value == feature),
            "missing standard feature {feature}: {body}"
        );
    }
    app.cleanup().await;
}
