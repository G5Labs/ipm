use crate::helpers;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{json, Value};

use g5_api::errors::G5Error;
use g5_api::services::self_hosted_license_service::{
    sha256_hex, MintedRegistryToken, RegistryTokenIssuer,
};
use helpers::TestUser;

#[derive(Clone)]
struct TestRegistryTokenIssuer {
    minted_token: String,
    expires_in: u64,
    requested_service_accounts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl RegistryTokenIssuer for TestRegistryTokenIssuer {
    async fn mint_access_token(
        &self,
        service_account_email: &str,
        _lifetime: Duration,
    ) -> Result<MintedRegistryToken, G5Error> {
        self.requested_service_accounts
            .lock()
            .expect("test issuer mutex should not be poisoned")
            .push(service_account_email.to_string());

        Ok(MintedRegistryToken {
            access_token: self.minted_token.clone(),
            expires_in: self.expires_in,
        })
    }
}

#[tokio::test]
async fn hosted_cloud_features_disabled_still_provisions_license_and_registry_token() {
    let admin_token = "admin-secret";
    let requested_service_accounts = Arc::new(Mutex::new(Vec::new()));
    let issuer = Arc::new(TestRegistryTokenIssuer {
        minted_token: "ya29.integration-test".to_string(),
        expires_in: 1800,
        requested_service_accounts: requested_service_accounts.clone(),
    });

    let mut config = helpers::test_config();
    config.self_hosted.license_admin_token_sha256 = Some(sha256_hex(admin_token));
    config.self_hosted.registry_token_lifetime = Duration::from_secs(1800);

    let app = helpers::TestApp::spawn_with_self_hosted_registry_token_issuer(config, issuer).await;

    let instance_response = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .expect("instance info request should send");
    assert_eq!(instance_response.status(), StatusCode::OK);
    let instance_body: Value = instance_response
        .json()
        .await
        .expect("instance info response should be JSON");
    let instance_data = &instance_body["data"];
    assert_eq!(instance_data["selfHosted"], false);
    assert!(
        !instance_data["features"]
            .as_array()
            .expect("features is an array")
            .iter()
            .any(|feature| feature == "cloud_workers"),
        "ENABLE_CLOUD_FEATURES=false should not advertise cloud_workers"
    );

    let license_payload = json!({
        "customer_id": "acme",
        "customer_name": "Acme Corp",
        "contact_email": "admin@acme.example",
        "product": "g5-self-hosted",
        "version": "latest",
        "requested_by": "ci@g5labs.ai",
        "registry": {
            "location": "us-central1",
            "project": "g5-dev-1ccc",
            "repository": "g5",
            "puller_service_account": "g5-registry-puller@g5-dev-1ccc.iam.gserviceaccount.com"
        }
    });

    let unauthorized_admin_response = app
        .client
        .post(format!("{}/api/v1/admin/licenses", app.base_url))
        .json(&license_payload)
        .send()
        .await
        .expect("admin license request should send");
    helpers::assert_json_error(
        unauthorized_admin_response,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
    )
    .await;

    let license_response = app
        .client
        .post(format!("{}/api/v1/admin/licenses", app.base_url))
        .bearer_auth(admin_token)
        .json(&license_payload)
        .send()
        .await
        .expect("admin license request should send");
    assert_eq!(license_response.status(), StatusCode::OK);
    let license_body: Value = license_response
        .json()
        .await
        .expect("license response should be JSON");
    let license_key = license_body
        .get("license_key")
        .and_then(Value::as_str)
        .expect("license response should include plaintext license key");
    assert!(license_key.starts_with("g5_live_"));
    assert_eq!(license_body["customer_id"], "acme");
    assert_eq!(
        license_body["registry"]["puller_service_account"],
        "g5-registry-puller@g5-dev-1ccc.iam.gserviceaccount.com"
    );

    let unauthorized_token_response = app
        .client
        .post(format!("{}/api/v1/registry/token", app.base_url))
        .bearer_auth("g5_live_wrong")
        .json(&json!({
            "product": "g5-self-hosted",
            "version": "1.0.7",
            "platform": "linux/amd64"
        }))
        .send()
        .await
        .expect("registry token request should send");
    helpers::assert_json_error(
        unauthorized_token_response,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
    )
    .await;

    let registry_token_response = app
        .client
        .post(format!("{}/api/v1/registry/token", app.base_url))
        .bearer_auth(license_key)
        .json(&json!({
            "product": "g5-self-hosted",
            "version": "1.0.7",
            "platform": "linux/amd64"
        }))
        .send()
        .await
        .expect("registry token request should send");
    assert_eq!(registry_token_response.status(), StatusCode::OK);
    let token_body: Value = registry_token_response
        .json()
        .await
        .expect("registry token response should be JSON");

    assert_eq!(token_body["registry"], "us-central1-docker.pkg.dev");
    assert_eq!(token_body["username"], "oauth2accesstoken");
    assert_eq!(token_body["password"], "ya29.integration-test");
    assert_eq!(token_body["expires_in"], 1800);
    assert_eq!(token_body["product"], "g5-self-hosted");
    assert_eq!(token_body["release"]["version"], "1.0.7");
    assert_eq!(
        token_body["release"]["image_refs"]["api"],
        "us-central1-docker.pkg.dev/g5-dev-1ccc/g5/api:v1.0.7"
    );
    assert_eq!(
        token_body["release"]["image_refs"]["edge"],
        "us-central1-docker.pkg.dev/g5-dev-1ccc/g5/edge:v1.0.7"
    );
    assert_eq!(
        token_body["release"]["image_refs"]["worker"],
        "us-central1-docker.pkg.dev/g5-dev-1ccc/g5/g5d:v1.0.7"
    );
    assert_eq!(
        token_body["release"]["image_refs"]["migration"],
        "us-central1-docker.pkg.dev/g5-dev-1ccc/g5/api:v1.0.7"
    );

    assert_eq!(
        *requested_service_accounts
            .lock()
            .expect("test issuer mutex should not be poisoned"),
        vec!["g5-registry-puller@g5-dev-1ccc.iam.gserviceaccount.com".to_string()]
    );

    app.cleanup().await;
}

#[tokio::test]
async fn self_hosted_customer_deployment_hides_delivery_and_gcp_control_routes() {
    let admin_token = "admin-secret";
    let mut config = helpers::test_config();
    config.is_self_hosted = true;
    config.enable_cloud_features = true;
    config.self_hosted.license_admin_token_sha256 = Some(sha256_hex(admin_token));

    let app = helpers::TestApp::spawn_with_config(config).await;

    let instance_response = app
        .client
        .get(format!("{}/api/v1/instance/info", app.base_url))
        .send()
        .await
        .expect("instance info request should send");
    assert_eq!(instance_response.status(), StatusCode::OK);
    let instance_body: Value = instance_response
        .json()
        .await
        .expect("instance info response should be JSON");
    let instance_data = &instance_body["data"];
    assert_eq!(instance_data["selfHosted"], true);
    assert!(
        !instance_data["features"]
            .as_array()
            .expect("features is an array")
            .iter()
            .any(|feature| feature == "cloud_workers"),
        "customer self-hosted deployments must not advertise cloud_workers"
    );

    let license_payload = json!({
        "customer_id": "acme",
        "customer_name": "Acme Corp",
        "contact_email": "admin@acme.example",
        "product": "g5-self-hosted",
        "version": "latest",
        "requested_by": "ci@g5labs.ai",
        "registry": {
            "location": "us-central1",
            "project": "g5-dev-1ccc",
            "repository": "g5",
            "puller_service_account": "g5-registry-puller@g5-dev-1ccc.iam.gserviceaccount.com"
        }
    });

    let admin_response = app
        .client
        .post(format!("{}/api/v1/admin/licenses", app.base_url))
        .bearer_auth(admin_token)
        .json(&license_payload)
        .send()
        .await
        .expect("admin license request should send");
    assert_eq!(admin_response.status(), StatusCode::NOT_FOUND);

    let registry_response = app
        .client
        .post(format!("{}/api/v1/registry/token", app.base_url))
        .bearer_auth("g5_live_not_relevant_here")
        .json(&json!({
            "product": "g5-self-hosted",
            "version": "1.0.7",
            "platform": "linux/amd64"
        }))
        .send()
        .await
        .expect("registry token request should send");
    assert_eq!(registry_response.status(), StatusCode::NOT_FOUND);

    let user = TestUser::create(&app).await;
    let auth_client = app.authenticated_client(&user);
    let worker_host_response = auth_client
        .post_json(
            &format!("/api/v1/orgs/{}/worker-hosts", uuid::Uuid::new_v4()),
            &json!({ "region": "us-central1" }),
        )
        .await;
    assert_eq!(worker_host_response.status(), StatusCode::NOT_FOUND);

    // Self-hosted deployments create no organizations after installation
    // (g5.organization.creation), even for a platform operator.
    helpers::platform::make_operator(&app, &user, "admin").await;
    let create_org_response = auth_client
        .post_json(
            "/api/v1/platform/orgs",
            &json!({ "name": "Extra Org", "adminEmail": user.email }),
        )
        .await;
    helpers::assert_json_error(create_org_response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    app.cleanup().await;
}
