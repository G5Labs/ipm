// Public test-only RSA keys. Never use these fixtures for deployment credentials.
pub const TENANT: &str = "72f988bf-86f1-41af-91ab-2d7cd011db47";
pub const OBJECT: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
pub const CLIENT: &str = "test-microsoft-client-id";

pub fn claims(nonce: &str) -> serde_json::Value {
    let now = jsonwebtoken::get_current_timestamp();
    serde_json::json!({
        "iss": format!("https://login.microsoftonline.com/{TENANT}/v2.0"),
        "aud": CLIENT, "tid": TENANT, "oid": OBJECT,
        "sub": "application-specific-sub-is-not-the-identity",
        "nbf": now - 10, "exp": now + 600, "nonce": nonce,
        "email": "azure.login@g5.dev", "name": "Azure Login User"
    })
}

pub fn signed(claims: &serde_json::Value, key: usize) -> String {
    let pem = match key {
        1 => include_bytes!("test-key-1.pem").as_slice(),
        2 => include_bytes!("test-key-2.pem").as_slice(),
        _ => panic!("unknown fixture key"),
    };
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(format!("key-{key}"));
    jsonwebtoken::encode(&header, claims, &jsonwebtoken::EncodingKey::from_rsa_pem(pem).unwrap()).unwrap()
}

pub fn jwk(key: usize) -> serde_json::Value {
    serde_json::from_str(match key {
        1 => include_str!("test-key-1.json"),
        2 => include_str!("test-key-2.json"),
        _ => panic!("unknown fixture key"),
    }).unwrap()
}

pub async fn keys(server: &wiremock::MockServer, keys: &[usize]) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/keys"))
        .respond_with(wiremock::ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"keys": keys.iter().map(|key| jwk(*key)).collect::<Vec<_>>()})))
        .mount(server).await;
}
