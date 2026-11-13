//! SEC-004 regressions through the shared production TCP listener and middleware.
use crate::helpers;

use g5_api::middleware::test_capture::{captured_g5_audit_events, Events};
use helpers::{test_config, TestApp, TestOrg, TestUser};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use serde_json::{json, Value};

fn capture() -> &'static Events {
    helpers::tracing_capture()
}

#[tokio::test]
async fn rotating_headers_and_new_connections_share_the_cli_start_peer_bucket() {
    let mut config = test_config();
    config.server.rate_limit_enabled = true;
    config.server.rate_limit_rpm = 5;
    let app = TestApp::spawn_with_config(config).await;
    let org = TestOrg::create(&app).await;
    // Twenty concurrent requests, each with its own connection and spoofed IP.
    // Complete before the first token refill (12 seconds at 5 RPM).
    let mut requests = tokio::task::JoinSet::new();
    for index in 0..20 {
        let url = format!("{}/api/v1/auth/cli/start", app.base_url);
        let body = json!({"orgId": org.id});
        requests.spawn(async move {
            reqwest::Client::new()
                .post(url)
                .header("x-forwarded-for", format!("198.51.100.{index}"))
                .header("x-real-ip", format!("192.0.2.{index}"))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status()
        });
    }
    let statuses = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut statuses = vec![];
        while let Some(result) = requests.join_next().await {
            statuses.push(result.unwrap());
        }
        statuses
    })
    .await
    .expect("burst must finish before refill");
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::OK).count(), 5);
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
            .count(),
        15
    );
    let retry = app
        .client
        .post(format!("{}/api/v1/auth/cli/start", app.base_url))
        .json(&json!({"orgId": org.id}))
        .send()
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(retry.headers().get("retry-after").is_some());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM cli_device_authorizations")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 5, "rejected requests must not provision device codes");
    app.cleanup().await;
}

#[tokio::test]
async fn trusted_suffix_preserves_clients_but_prefixes_and_ip_spellings_do_not_rotate_buckets() {
    for (hops, proxy_cidr) in [
        (1, "127.0.0.1/32"),
        (2, "127.0.0.1/32"),
        (1, "::ffff:127.0.0.1/128"),
        (2, "::ffff:127.0.0.0/120"),
    ] {
        let mut config = test_config();
        config.server.rate_limit_enabled = true;
        config.server.rate_limit_rpm = 1;
        config.server.trusted_proxy_cidrs = vec![proxy_cidr.parse().unwrap()];
        config.server.trusted_proxy_xff_hops = hops;
        let app = TestApp::spawn_with_config(config).await;
        let org = TestOrg::create(&app).await;
        for (client_ip, expected) in [
            ("203.0.113.9", StatusCode::OK),
            ("::ffff:203.0.113.9", StatusCode::TOO_MANY_REQUESTS),
            ("203.0.113.10", StatusCode::OK),
            ("2001:db8::1", StatusCode::OK),
            ("2001:0DB8:0:0:0:0:0:1", StatusCode::TOO_MANY_REQUESTS),
        ] {
            let mut headers = HeaderMap::new();
            headers.append(
                "x-forwarded-for",
                HeaderValue::from_str(&format!("spoof-{}, {client_ip}", uuid::Uuid::new_v4()))
                    .unwrap(),
            );
            if hops == 2 {
                headers.append("x-forwarded-for", HeaderValue::from_static("192.0.2.80"));
            }
            let response = app
                .client
                .post(format!("{}/api/v1/auth/cli/start", app.base_url))
                .headers(headers)
                .json(&json!({"orgId": org.id}))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                expected,
                "hops={hops}, proxy={proxy_cidr}, client={client_ip}"
            );
        }
        // Invalid suffix and no header both consume the same peer fallback.
        for (xff, expected) in [
            (Some("garbage"), StatusCode::OK),
            (None, StatusCode::TOO_MANY_REQUESTS),
        ] {
            let mut request = app
                .client
                .post(format!("{}/api/v1/auth/cli/start", app.base_url))
                .json(&json!({"orgId": org.id}));
            if let Some(xff) = xff {
                request = request.header("x-forwarded-for", xff);
            }
            assert_eq!(request.send().await.unwrap().status(), expected);
        }
        app.cleanup().await;
    }
}

#[tokio::test]
async fn audit_telemetry_and_api_key_usage_agree_even_when_rate_limiting_is_disabled() {
    let events = capture();
    for hops in [0, 1, 2] {
        let mut config = test_config();
        assert!(!config.server.rate_limit_enabled);
        if hops > 0 {
            config.server.trusted_proxy_cidrs = vec!["127.0.0.1/32".parse().unwrap()];
            config.server.trusted_proxy_xff_hops = hops;
        }
        let app = TestApp::spawn_with_config(config).await;
        let mut user = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut user).await;
        let created = app
            .authenticated_client(&user)
            .post_json(
                &format!("/api/v1/orgs/{}/api-keys", org.id),
                &json!({"name": "client IP regression", "scopes": ["org:admin"]}),
            )
            .await;
        assert_eq!(created.status(), StatusCode::OK);
        let key: Value = created.json().await.unwrap();
        let key_id: uuid::Uuid = key["data"]["apiKey"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let xff = if hops == 2 {
            "198.51.100.99, ::ffff:203.0.113.9, 192.0.2.80"
        } else {
            "198.51.100.99, ::ffff:203.0.113.9"
        };
        let expected = if hops == 0 {
            "127.0.0.1"
        } else {
            "203.0.113.9"
        };
        // Exercise a successful mutation within the key's organization. Account
        // profile updates require a person session and cannot serve this fixture.
        let response = app
            .client
            .patch(format!("{}/api/v1/orgs/{}", app.base_url, org.id))
            .bearer_auth(key["data"]["key"].as_str().unwrap())
            .header("x-forwarded-for", xff)
            .header("x-real-ip", "192.0.2.99")
            .json(&json!({"name": "IP attribution regression"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let used: Option<String> =
            sqlx::query_scalar("SELECT host(last_used_ip) FROM api_keys WHERE id = $1")
                .bind(key_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(used.as_deref(), Some(expected));
        let body = json!({
            "sessionId": user.id.to_string(),
            "events": [{"seq": 1, "tsMs": 1, "kind": "click", "target": "test"}],
        });
        let response = app
            .client
            .post(format!("{}/api/v1/telemetry/ui-events", app.base_url))
            .bearer_auth(&user.jwt)
            .header("x-forwarded-for", xff)
            .header("x-real-ip", "192.0.2.99")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let captured = captured_g5_audit_events(events);
        let user_id = user.id.to_string();
        for event in ["user_action.http", "ui.action"] {
            let entry = captured
                .iter()
                .find(|entry| {
                    entry.field("event") == Some(event)
                        && entry.field("user_id") == Some(user_id.as_str())
                        && (event == "ui.action" || entry.field("auth_kind") == Some("api_key"))
                })
                .unwrap_or_else(|| panic!("missing {event} event"));
            assert_eq!(
                entry.field("client_ip"),
                Some(expected),
                "hops={hops}, event={event}"
            );
        }
        app.cleanup().await;
    }
}
