//! Bounded secret display hints on the scoped-config routes
//! (`test_contract/credentials_visibility.semlift.md` **I1**;
//! `security_policy/secret_custody_and_redaction` **D9**, config route **D6**,
//! `config_service` **D9**).
//!
//! ## What this proves
//!
//! The org/workspace/user config responses carry `secretKeyHints` (own scope)
//! and `effectiveSecretKeyHints` (winning value) mapping each hint-eligible
//! secret to exactly its first 12 characters — and nothing longer: no response
//! field anywhere may carry the secret remainder. Ineligible values (below the
//! 24-char floor, multiline, or `{`/`[`-leading) have no hint while still being
//! listed in `secretKeys`; the retired `CODEX_AUTH_JSON` key is rejected.
//!
//! Runs against live Postgres like every file in this directory:
//! `cargo nextest run -p g5-api --test integration -E 'test(/^api_config_secret_hints::/)'`.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Gateway-shaped bearer stored at org scope (eligible: 41 chars, single line).
const ORG_GATEWAY_TOKEN: &str = "sk-or-v1-0123456789abcdef0123456789abcdef";
/// Subscription-OAuth-shaped token stored at user scope (eligible).
const USER_OAUTH_TOKEN: &str = "sk-ant-oat01-0123456789abcdefghijklmnopqrstuv";

const ORG_GATEWAY_HINT: &str = "sk-or-v1-012";
const USER_OAUTH_HINT: &str = "sk-ant-oat01";

fn hint_of<'a>(body: &'a Value, field: &str, key: &str) -> Option<&'a str> {
    body["data"][field][key].as_str()
}

/// Every scoped-config response — GET and PUT — must carry
/// `Cache-Control: no-store` (config route D6): these bodies hold secret key
/// names and secret-derived hints, so no cache may retain them.
fn assert_no_store(response: &reqwest::Response) {
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "scoped-config responses must be Cache-Control: no-store"
    );
}

/// No serialized response field may carry secret material beyond the
/// sanctioned 12-character hint (D9's required evidence: sweep the whole
/// client-bound shape, not just the hint fields). D9 forbids every other
/// derivation — longer prefixes, the remainder, suffixes, hashes of the
/// value or remainder, and any recombined projection. This helper is
/// targeted regression evidence for those named projections (plus a tree
/// walk confining the sanctioned prefix to the two hint maps), not an
/// exhaustive proof over all derivable projections; the log/DOM surfaces
/// are owned by the manual leak-sweep scenario (test_contract I3).
fn assert_only_sanctioned_hints(body: &Value, secrets: &[&str]) {
    use sha2::{Digest, Sha256};

    let serialized = body.to_string();
    for secret in secrets {
        assert!(
            !serialized.contains(secret),
            "response must never carry a full secret value"
        );
        let thirteen: String = secret.chars().take(13).collect();
        assert!(
            !serialized.contains(&thirteen),
            "response must never carry more than the first 12 characters of a secret"
        );
        // Remainder/suffix substring checks need a distinctive needle: for
        // short (ineligible) secrets the remainder can be a 2-3 character
        // fragment that legitimately occurs in unrelated JSON, so only
        // assert absence when the needle is long enough to be meaningful.
        const DISTINCTIVE_NEEDLE_CHARS: usize = 8;
        let remainder: String = secret.chars().skip(12).collect();
        if remainder.chars().count() >= DISTINCTIVE_NEEDLE_CHARS {
            assert!(
                !serialized.contains(&remainder),
                "response must never carry the secret remainder"
            );
        }
        let suffix: String = {
            let chars: Vec<char> = secret.chars().collect();
            chars[chars.len().saturating_sub(12)..].iter().collect()
        };
        if suffix.chars().count() >= DISTINCTIVE_NEEDLE_CHARS && suffix != *secret {
            assert!(
                !serialized.contains(&suffix),
                "response must never carry a secret suffix"
            );
        }
        for hashed in [
            format!("{:x}", Sha256::digest(secret.as_bytes())),
            format!("{:x}", Sha256::digest(remainder.as_bytes())),
        ] {
            assert!(
                !serialized.contains(&hashed),
                "response must never carry a hash of a secret or its remainder"
            );
        }

        // The sanctioned 12-character prefix may appear ONLY inside the two
        // hint maps — nowhere else in the client-bound shape.
        let prefix: String = secret.chars().take(12).collect();
        assert_prefix_confined_to_hint_maps(&body["data"], &prefix, "data");
    }
}

/// Walk every string field outside `secretKeyHints` / `effectiveSecretKeyHints`
/// and assert the sanctioned prefix does not appear there.
fn assert_prefix_confined_to_hint_maps(value: &Value, prefix: &str, path: &str) {
    match value {
        Value::String(s) => assert!(
            !s.contains(prefix),
            "secret-derived prefix leaked outside the hint maps at {path}"
        ),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_prefix_confined_to_hint_maps(item, prefix, &format!("{path}[{i}]"));
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                if key == "secretKeyHints" || key == "effectiveSecretKeyHints" {
                    continue;
                }
                assert_prefix_confined_to_hint_maps(item, prefix, &format!("{path}.{key}"));
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn scoped_config_responses_carry_bounded_hints_and_never_the_remainder() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$1 WHERE id=$2")
        .bind(user.id)
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let client = app.authenticated_client(&user);

    let org_path = format!("/api/v1/orgs/{}/config", org.id);
    let workspace_path = format!(
        "/api/v1/workspaces/{}/config?orgId={}",
        workspace.id, org.id
    );
    let user_path = format!("/api/v1/users/me/config?orgId={}", org.id);

    // Seed the same key at two scopes with distinguishable token shapes.
    let seed_org = app
        .authenticated_client(&owner)
        .put_json(
            &org_path,
            &json!({ "setSecrets": { "ANTHROPIC_AUTH_TOKEN": ORG_GATEWAY_TOKEN } }),
        )
        .await;
    assert_eq!(seed_org.status(), StatusCode::OK);
    assert_no_store(&seed_org);
    let seed_user = client
        .put_json(
            &user_path,
            &json!({ "setSecrets": { "ANTHROPIC_AUTH_TOKEN": USER_OAUTH_TOKEN } }),
        )
        .await;
    assert_eq!(seed_user.status(), StatusCode::OK);
    assert_no_store(&seed_user);

    // Org scope: own hint and effective hint are both the org value's prefix
    // (org-only effective context).
    let org_response = client.get(&org_path).await;
    assert_no_store(&org_response);
    let org_body: Value = org_response.json().await.unwrap();
    assert_eq!(
        hint_of(&org_body, "secretKeyHints", "ANTHROPIC_AUTH_TOKEN"),
        Some(ORG_GATEWAY_HINT)
    );
    assert_eq!(
        hint_of(&org_body, "effectiveSecretKeyHints", "ANTHROPIC_AUTH_TOKEN"),
        Some(ORG_GATEWAY_HINT)
    );
    assert_only_sanctioned_hints(&org_body, &[ORG_GATEWAY_TOKEN, USER_OAUTH_TOKEN]);

    // User scope: own hint is the user value's prefix, and the user scope wins
    // the effective merge over the org scope.
    let user_response = client.get(&user_path).await;
    assert_no_store(&user_response);
    let user_body: Value = user_response.json().await.unwrap();
    assert_eq!(
        hint_of(&user_body, "secretKeyHints", "ANTHROPIC_AUTH_TOKEN"),
        Some(USER_OAUTH_HINT)
    );
    assert_eq!(
        hint_of(
            &user_body,
            "effectiveSecretKeyHints",
            "ANTHROPIC_AUTH_TOKEN"
        ),
        Some(USER_OAUTH_HINT)
    );
    assert_only_sanctioned_hints(&user_body, &[ORG_GATEWAY_TOKEN, USER_OAUTH_TOKEN]);

    // Workspace scope: no own secret → empty own-hints map, while the full
    // org → workspace → user effective chain still resolves the user winner.
    let workspace_response = client.get(&workspace_path).await;
    assert_no_store(&workspace_response);
    let workspace_body: Value = workspace_response.json().await.unwrap();
    assert_eq!(
        workspace_body["data"]["secretKeyHints"],
        json!({}),
        "a scope without its own secrets must return an empty own-hints map"
    );
    assert_eq!(
        hint_of(
            &workspace_body,
            "effectiveSecretKeyHints",
            "ANTHROPIC_AUTH_TOKEN"
        ),
        Some(USER_OAUTH_HINT)
    );
    assert!(workspace_body["data"]["effectiveSecretKeys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k == "ANTHROPIC_AUTH_TOKEN"));
    assert_only_sanctioned_hints(&workspace_body, &[ORG_GATEWAY_TOKEN, USER_OAUTH_TOKEN]);

    // The no-store contract covers every scoped-config handler, success and
    // error alike (config route D6): workspace PUT success, plus an error
    // response from the same router (unknown workspace id).
    let workspace_put = client
        .put_json(&workspace_path, &json!({ "settings": {} }))
        .await;
    assert_eq!(workspace_put.status(), StatusCode::OK);
    assert_no_store(&workspace_put);

    let error_response = client
        .get(&format!(
            "/api/v1/workspaces/{}/config?orgId={}",
            uuid::Uuid::new_v4(),
            org.id
        ))
        .await;
    assert!(
        !error_response.status().is_success(),
        "unknown workspace id must not resolve a config"
    );
    assert_no_store(&error_response);

    // Same contract on the org and user routers' error paths: an org the
    // caller has no membership in, and a user request whose required orgId
    // query parameter is missing (extractor-level rejection).
    let org_error = client
        .get(&format!("/api/v1/orgs/{}/config", uuid::Uuid::new_v4()))
        .await;
    assert!(
        !org_error.status().is_success(),
        "foreign org id must not resolve a config"
    );
    assert_no_store(&org_error);

    let user_error = client.get("/api/v1/users/me/config").await;
    assert!(
        !user_error.status().is_success(),
        "missing orgId query must be rejected"
    );
    assert_no_store(&user_error);

    app.cleanup().await;
}

#[tokio::test]
async fn ineligible_secret_values_are_listed_without_hints() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let org_path = format!("/api/v1/orgs/{}/config", org.id);

    // Codex auth.json is task-bound and must be rejected even when its shape is
    // valid. This is distinct from three allowed generic secrets whose values
    // are ineligible for display hints.
    let codex_auth_json = json!({
        "tokens": {
            "access_token": "a".repeat(64),
            "refresh_token": "r".repeat(64),
        }
    })
    .to_string();
    let short_value = "sk-short-value";
    let multiline_value = "sk-ant-api03-first-line-is-long-enough\nsecond-line";
    let brace_value = "{\"not\":\"codex-but-json-shaped-and-long-enough\"}";

    let retired_key = client
        .put_json(
            &org_path,
            &json!({ "setSecrets": {
                "CODEX_AUTH_JSON": codex_auth_json
            } }),
        )
        .await;
    assert_eq!(retired_key.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_no_store(&retired_key);

    let seed = client
        .put_json(
            &org_path,
            &json!({ "setSecrets": {
                "SHORT_TOKEN": short_value,
                "MULTILINE_TOKEN": multiline_value,
                "BRACE_TOKEN": brace_value,
            } }),
        )
        .await;
    assert_eq!(seed.status(), StatusCode::OK);

    let response = client.get(&org_path).await;
    assert_no_store(&response);
    let body: Value = response.json().await.unwrap();
    let secret_keys: Vec<&str> = body["data"]["secretKeys"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(!secret_keys.contains(&"CODEX_AUTH_JSON"));
    for key in ["SHORT_TOKEN", "MULTILINE_TOKEN", "BRACE_TOKEN"] {
        assert!(
            secret_keys.contains(&key),
            "{key} must still be listed as configured"
        );
        assert!(
            body["data"]["secretKeyHints"][key].is_null(),
            "{key} must have no hint (ineligible value)"
        );
        assert!(
            body["data"]["effectiveSecretKeyHints"][key].is_null(),
            "{key} must have no effective hint (ineligible value)"
        );
    }
    assert_only_sanctioned_hints(&body, &[short_value, multiline_value, brace_value]);

    app.cleanup().await;
}

#[tokio::test]
async fn retired_codex_auth_cleanup_is_lossless_across_all_config_scopes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    let workspace_path = format!(
        "/api/v1/workspaces/{}/config?orgId={}",
        workspace.id, org.id
    );
    let user_path = format!("/api/v1/users/me/config?orgId={}", org.id);
    assert_eq!(
        client
            .put_json(&workspace_path, &json!({"settings": {"keep": "workspace"}}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .put_json(&user_path, &json!({"settings": {"keep": "user"}}))
            .await
            .status(),
        StatusCode::OK
    );

    let manager = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test app configures the secret encryption key");
    let encrypt = |keep_key: &str, keep_value: &str, retired_key: &str| {
        let secrets = HashMap::from([
            (keep_key.to_string(), keep_value.to_string()),
            (
                retired_key.to_string(),
                json!({"tokens":{"access_token":"legacy"}}).to_string(),
            ),
        ]);
        manager
            .encrypt_token(&serde_json::to_string(&secrets).unwrap())
            .unwrap()
    };
    let mut seeded = app.scoped_db_for_user_org(user.id, org.id).await;

    let org_updated = sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secrets_enc, secret_keys) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (org_id) DO UPDATE SET settings_json = EXCLUDED.settings_json, \
         secrets_enc = EXCLUDED.secrets_enc, secret_keys = EXCLUDED.secret_keys",
    )
    .bind(org.id)
    .bind(json!({" CODEX_AUTH_JSON ": "must-be-removed"}))
    .bind(encrypt("KEEP_ORG", "org-secret", "CODEX_AUTH_JSON"))
    .bind(vec!["KEEP_ORG", "CODEX_AUTH_JSON"])
    .execute(seeded.conn())
    .await
    .unwrap();
    assert_eq!(org_updated.rows_affected(), 1);
    let workspace_updated = sqlx::query(
        "INSERT INTO workspace_configs \
         (workspace_id, org_id, settings_json, secrets_enc, secret_keys) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (workspace_id) DO UPDATE SET settings_json = EXCLUDED.settings_json, \
         secrets_enc = EXCLUDED.secrets_enc, secret_keys = EXCLUDED.secret_keys",
    )
    .bind(workspace.id)
    .bind(org.id)
    // Simulate a stale plaintext index: the retired key exists only inside
    // encrypted payload bytes, so cleanup must not trust secret_keys/settings.
    .bind(json!({"keep_setting": "workspace"}))
    .bind(encrypt(
        "KEEP_WORKSPACE",
        "workspace-secret",
        " CODEX_AUTH_JSON ",
    ))
    .bind(vec!["KEEP_WORKSPACE"])
    .execute(seeded.conn())
    .await
    .unwrap();
    assert_eq!(workspace_updated.rows_affected(), 1);
    let user_updated = sqlx::query(
        "INSERT INTO user_configs \
         (org_id, user_id, settings_json, secrets_enc, secret_keys) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (org_id, user_id) DO UPDATE SET settings_json = EXCLUDED.settings_json, \
         secrets_enc = EXCLUDED.secrets_enc, secret_keys = EXCLUDED.secret_keys",
    )
    .bind(org.id)
    .bind(user.id)
    .bind(json!({"CODEX_AUTH_JSON ": "must-be-removed"}))
    .bind(encrypt("KEEP_USER", "user-secret", "CODEX_AUTH_JSON "))
    .bind(vec!["KEEP_USER", "CODEX_AUTH_JSON "])
    .execute(seeded.conn())
    .await
    .unwrap();
    assert_eq!(user_updated.rows_affected(), 1);
    seeded.commit().await.unwrap();

    let candidate_scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope_kind FROM authz.list_retired_codex_auth_config_rows() ORDER BY scope_kind",
    )
    .fetch_all(&app.system_pool)
    .await
    .unwrap();
    assert_eq!(candidate_scopes, vec!["org", "user", "workspace"]);

    assert_eq!(
        app.state
            .config_service
            .purge_retired_codex_auth_json(&app.system_pool)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        app.state
            .config_service
            .purge_retired_codex_auth_json(&app.system_pool)
            .await
            .unwrap(),
        0,
        "cleanup must be idempotent"
    );

    let mut verified = app.scoped_db_for_user_org(user.id, org.id).await;
    for (table, predicate, keep_key, keep_value) in [
        (
            "org_configs",
            format!("org_id = '{}'", org.id),
            "KEEP_ORG",
            "org-secret",
        ),
        (
            "workspace_configs",
            format!("workspace_id = '{}'", workspace.id),
            "KEEP_WORKSPACE",
            "workspace-secret",
        ),
        (
            "user_configs",
            format!("org_id = '{}' AND user_id = '{}'", org.id, user.id),
            "KEEP_USER",
            "user-secret",
        ),
    ] {
        let row = sqlx::query(&format!(
            "SELECT settings_json, secrets_enc, secret_keys FROM {table} WHERE {predicate}"
        ))
        .fetch_one(verified.conn())
        .await
        .unwrap();
        use sqlx::Row;
        let settings: Value = row.try_get("settings_json").unwrap();
        let encrypted: Vec<u8> = row
            .try_get::<Option<Vec<u8>>, _>("secrets_enc")
            .unwrap()
            .expect("sibling secret remains encrypted");
        let secret_keys: Vec<String> = row.try_get("secret_keys").unwrap();
        let decoded: HashMap<String, String> = serde_json::from_str(
            &manager
                .decrypt_token(&encrypted)
                .expect("decrypt scrubbed map"),
        )
        .unwrap();
        assert_eq!(decoded.get(keep_key).map(String::as_str), Some(keep_value));
        assert_eq!(secret_keys, vec![keep_key]);
        assert!(decoded.keys().all(|key| key.trim() != "CODEX_AUTH_JSON"));
        assert!(settings
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key.trim() != "CODEX_AUTH_JSON"));
    }
    verified.commit().await.unwrap();

    app.cleanup().await;
}
