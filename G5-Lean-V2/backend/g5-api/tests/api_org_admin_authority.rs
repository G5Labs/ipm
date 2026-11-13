//! SEC-H02: exercise every org:admin/workers:admin/billing:admin route with authority
//! from another organization, using the production HTTP and database boundary.
//! SEC-H01 (#452): organization Runtime Config requires current target-org administration.
use crate::helpers;

use g5_auth::Scope;
use helpers::{
    test_config, test_jwt_minter, AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace,
};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use uuid::Uuid;

async fn set_role(app: &TestApp, org: Uuid, user: Uuid, role: &str) {
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1,$2,$3) ON CONFLICT (org_id,user_id) DO UPDATE SET role=EXCLUDED.role")
        .bind(org).bind(user).bind(role).execute(&app.db).await.unwrap();
}

fn client_for(app: &TestApp, user: Uuid, org: Uuid, scopes: &[Scope]) -> AuthenticatedClient {
    AuthenticatedClient::new(
        reqwest::Client::new(),
        app.base_url.clone(),
        test_jwt_minter()
            .mint(user, org, scopes.to_vec(), 3600)
            .unwrap(),
    )
}

async fn configured_app() -> TestApp {
    let mut config = test_config();
    config.github_app = Some(g5_api::config::GithubAppConfig {
        app_id: "123456".into(),
        private_key_pem: helpers::github_key::TEST_GITHUB_APP_PRIVATE_KEY.into(),
        app_slug: "g5-test".into(),
        oauth_client_id: "test".into(),
        oauth_client_secret: "test".into(),
        oauth_callback_url: "http://localhost/api/v1/github-app/oauth/callback".into(),
        connection_state_secret: "integration-state-secret-of-at-least-32-bytes".into(),
        // An unauthorized request must never reach an external provider.
        api_base_url: "http://127.0.0.1:1".into(),
        web_base_url: "http://127.0.0.1:1".into(),
    });
    TestApp::spawn_with_config(config).await
}

fn admin_routes(org: Uuid, user: Uuid, ws: Uuid) -> Vec<(Method, String, Value)> {
    let o = format!("/api/v1/orgs/{org}");
    let w = format!("/api/v1/workspaces/{ws}");
    let id = Uuid::new_v4();
    vec![
        (
            Method::PUT,
            format!("{o}/config"),
            json!({"settings":{"OPENAI_BASE_URL":"https://attacker.invalid"}}),
        ),
        (Method::GET, format!("{w}/settings/spend"), json!({})),
        (
            Method::GET,
            format!("{w}/settings/spend/history"),
            json!({}),
        ),
        (Method::PATCH, o.clone(), json!({"name":"unauthorized"})),
        (
            Method::POST,
            format!("{o}/members"),
            json!({"email":"new@g5.test","role":"admin"}),
        ),
        (
            Method::PATCH,
            format!("{o}/members/{user}"),
            json!({"role":"admin"}),
        ),
        (Method::DELETE, format!("{o}/members/{user}"), json!({})),
        (Method::GET, format!("{o}/invitations"), json!({})),
        (
            Method::POST,
            format!("{o}/invitations/{id}/resend"),
            json!({}),
        ),
        (Method::DELETE, format!("{o}/invitations/{id}"), json!({})),
        (Method::GET, format!("{o}/admin-settings"), json!({})),
        (
            Method::PUT,
            format!("{o}/admin-settings"),
            json!({"maxRunningVms":100,"defaultIdleTimeoutSecs":60}),
        ),
        (Method::GET, format!("{o}/api-keys"), json!({})),
        (
            Method::POST,
            format!("{o}/api-keys"),
            json!({"name":"unauthorized","scopes":["org:admin"]}),
        ),
        (Method::DELETE, format!("{o}/api-keys/{id}"), json!({})),
        (Method::GET, format!("{o}/worker-hosts"), json!({})),
        (
            Method::POST,
            format!("{o}/worker-hosts"),
            json!({"region":"us-central1"}),
        ),
        (
            Method::GET,
            format!("{o}/worker-hosts/workspaces"),
            json!({}),
        ),
        (
            Method::POST,
            format!("{o}/worker-hosts/capacity-stop"),
            json!({"workspaceId":ws,"operationId":id}),
        ),
        (
            Method::PUT,
            format!("{o}/worker-hosts/image-policy"),
            json!({"baseImageRef":helpers::TEST_WORKER_IMAGE_REF}),
        ),
        (Method::DELETE, format!("{o}/worker-hosts/{id}"), json!({})),
        (
            Method::POST,
            format!("{o}/worker-hosts/{id}/drain"),
            json!({}),
        ),
        (
            Method::POST,
            format!("{o}/worker-hosts/{id}/runtime-labels/refresh"),
            json!({}),
        ),
        (
            Method::POST,
            format!("{o}/approval-definitions"),
            json!({"name":"unauthorized"}),
        ),
        (
            Method::POST,
            format!("{o}/approval-definitions/{id}/versions"),
            json!({"name":"unauthorized","invalidatesPrevious":false}),
        ),
        (
            Method::POST,
            format!("{w}/approvals/requirements"),
            json!({"definitionId":id}),
        ),
        (
            Method::POST,
            format!("{w}/approvals/requirements/{id}/retire"),
            json!({}),
        ),
        (
            Method::PUT,
            format!("{w}/permissions/{user}"),
            json!({"scopes":["org:admin"]}),
        ),
        (Method::DELETE, format!("{w}/permissions/{user}"), json!({})),
        (
            Method::PUT,
            format!("{o}/scopes"),
            json!({"scopes":["org:admin"]}),
        ),
        (
            Method::PUT,
            format!("{o}/scopes/members/{user}/scopes"),
            json!({"scopes":["org:admin"]}),
        ),
        (Method::GET, format!("{o}/github-app/binding"), json!({})),
        (
            Method::POST,
            format!("{o}/github-app/connection-attempt"),
            json!({}),
        ),
        (
            Method::POST,
            format!("{o}/github-app/reconnect-attempt"),
            json!({}),
        ),
        (
            Method::GET,
            format!("{o}/github-app/binding/repositories"),
            json!({}),
        ),
        (
            Method::PUT,
            format!("{o}/github-app/binding/coverage"),
            json!({"bindingVersion":1,"githubRepoIds":[]}),
        ),
        (
            Method::POST,
            format!("{o}/github-app/binding/refresh"),
            json!({"bindingVersion":1}),
        ),
        (
            Method::POST,
            format!("{o}/github-app/binding/disconnect"),
            json!({"bindingVersion":1}),
        ),
        (
            Method::GET,
            format!("{o}/github-app/binding/management-url"),
            json!({}),
        ),
        (
            Method::POST,
            format!("{o}/issue-domains"),
            json!({"githubRepoId":"123"}),
        ),
        (
            Method::PATCH,
            format!("{o}/issue-domains/{id}"),
            json!({"contentTrusted":true}),
        ),
        (Method::DELETE, format!("{o}/issue-domains/{id}"), json!({})),
    ]
}

#[tokio::test]
async fn sec_h02_every_admin_route_rejects_other_org_authority() {
    let app = configured_app().await;
    let mut attacker = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut attacker).await;
    let target = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &target).await;
    let victim = TestUser::create(&app).await;
    set_role(&app, target.id, victim.id, "member").await;
    let http = reqwest::Client::new();
    for role in ["viewer", "member"] {
        set_role(&app, target.id, attacker.id, role).await;
        let mut failures = Vec::new();
        for (method, path, body) in admin_routes(target.id, victim.id, workspace.id) {
            let response = http
                .request(method.clone(), format!("{}{path}", app.base_url))
                .bearer_auth(&attacker.jwt)
                .json(&body)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let response_body = response.text().await.unwrap();
            if status != StatusCode::FORBIDDEN || response_body.contains("org_membership") {
                failures.push(format!("{role}: {method} {path}: {status} {response_body}"));
            }
        }
        // Keep the attacker's membership intact throughout the matrix, then
        // exercise the audit's self-promotion exploit explicitly.
        let response = app
            .authenticated_client(&attacker)
            .patch_json(
                &format!("/api/v1/orgs/{}/members/{}", target.id, attacker.id),
                &json!({"role":"admin"}),
            )
            .await;
        if response.status() != StatusCode::FORBIDDEN {
            failures.push(format!("{role}: self-promotion: {}", response.status()));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        let current: String =
            sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id=$1 AND user_id=$2")
                .bind(target.id)
                .bind(attacker.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(current, role);
        assert_eq!(
            app.authenticated_client(&attacker)
                .get(&format!("/api/v1/orgs/{}/api-keys", source.id))
                .await
                .status(),
            StatusCode::OK
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_current_target_grants_and_token_limits_are_both_required() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut user).await;
    let target = TestOrg::create(&app).await;
    let client = app.authenticated_client(&user);
    let path = format!("/api/v1/orgs/{}/api-keys", target.id);
    for role in ["admin", "member", "viewer"] {
        set_role(&app, target.id, user.id, role).await;
        let expected = if role == "admin" {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        // Reuse the exact token and capability cache through promotions/demotions.
        assert_eq!(client.get(&path).await.status(), expected, "{role}");
    }
    // Additive grants resolve within the role ceiling
    // (`g5.organization.viewer-ceiling`): the viewer above stays refused with
    // the same grant; a member gains the administrative scope from it.
    user.grant_org_scope(&app, target.id, Scope::OrgAdmin).await;
    assert_eq!(client.get(&path).await.status(), StatusCode::FORBIDDEN);
    set_role(&app, target.id, user.id, "member").await;
    assert_eq!(client.get(&path).await.status(), StatusCode::OK);
    let limited = client_for(&app, user.id, source.id, &[Scope::WorkspacesRead]);
    assert_eq!(limited.get(&path).await.status(), StatusCode::FORBIDDEN);
    sqlx::query("UPDATE user_scope_grants SET expires_at=now()-interval '1 second' WHERE org_id=$1 AND user_id=$2")
        .bind(target.id).bind(user.id).execute(&app.db).await.unwrap();
    assert_eq!(client.get(&path).await.status(), StatusCode::FORBIDDEN);
    sqlx::query("INSERT INTO org_default_scopes (org_id,scope) VALUES ($1,'org:admin')")
        .bind(target.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(client.get(&path).await.status(), StatusCode::OK);
    sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(target.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(client.get(&path).await.status(), StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_api_key_is_bound_to_issuing_org_and_current_creator_authority() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut user).await;
    let target = TestOrg::create(&app).await;
    set_role(&app, target.id, user.id, "admin").await;
    let path = format!("/api/v1/orgs/{}/api-keys", source.id);
    let response = app
        .authenticated_client(&user)
        .post_json(
            &path,
            &json!({"name":"admin","scopes":["org:admin","workers:admin","workspaces:read"]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let key = body["data"]["key"].as_str().expect("plaintext key");
    let client = AuthenticatedClient::new(reqwest::Client::new(), app.base_url.clone(), key.into());
    assert_eq!(client.get(&path).await.status(), StatusCode::OK);
    assert_eq!(
        client
            .get(&format!("/api/v1/orgs/{}/api-keys", target.id))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let workspace = TestWorkspace::create(&app, &source).await;
    assert_eq!(
        client
            .get(&format!("/api/v1/workspaces/{}", workspace.id))
            .await
            .status(),
        StatusCode::OK
    );
    let foreign_workspace = TestWorkspace::create(&app, &target).await;
    let foreign_path = format!("/api/v1/workspaces/{}", foreign_workspace.id);
    assert_eq!(
        app.authenticated_client(&user)
            .get(&foreign_path)
            .await
            .status(),
        StatusCode::OK,
        "the browser token can access the target workspace through current membership"
    );
    assert_eq!(
        client.get(&foreign_path).await.status(),
        StatusCode::NOT_FOUND,
        "the API key must not resolve an entity outside its issuing org"
    );
    let successor = TestUser::create_with_scopes(&app, source.id, &[Scope::OrgAdmin]).await;
    set_role(&app, source.id, successor.id, "admin").await;
    set_role(&app, source.id, user.id, "member").await;
    assert_eq!(client.get(&path).await.status(), StatusCode::FORBIDDEN);
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_member_mutation_rechecks_actor_after_concurrent_demotion() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let target = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    set_role(&app, org.id, actor.id, "admin").await;
    let mut demotion = app.db.begin().await.unwrap();
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(actor.id)
        .execute(&mut *demotion)
        .await
        .unwrap();
    // Call the production service boundary directly to place demotion between
    // route admission and mutation, without relying on HTTP scheduling.
    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(actor.id),
            &[g5_contracts::types::ids::OrgId::from_uuid(org.id)],
            &["org:admin".into()],
        )
        .await
        .unwrap();
    let mut db = g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
        .await
        .unwrap();
    {
        let service = &app.state.org_service;
        let mutation = service.change_member(
            &mut db,
            org.id,
            actor.id,
            target.id,
            g5_api::services::org_service::MemberChange::Role("admin".into()),
        );
        tokio::pin!(mutation);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut mutation)
                .await
                .is_err(),
            "actor authority should wait on the demotion row lock"
        );
        demotion.commit().await.unwrap();
        assert!(matches!(
            mutation.await,
            Err(g5_contracts::error::G5Error::Forbidden { .. })
        ));
    }
    let role: String =
        sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(target.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role, "member");
    // The denied transaction is dropped before database cleanup.
    drop(db);
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_multi_org_capability_requires_authority_in_every_org() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut user).await;
    let target = TestOrg::create(&app).await;
    set_role(&app, target.id, user.id, "member").await;
    let auth = g5_api::middleware::AuthContext::User(g5_auth::JwtClaims {
        sub: user.id.to_string(),
        org: source.id.to_string(),
        scopes: vec![Scope::OrgAdmin],
        iat: 0,
        exp: 4_102_444_800,
        iss: None,
        aud: None,
        jti: None,
        token_use: None,
        client: None,
    });
    let orgs = [source.id, target.id];
    assert!(
        g5_api::db::scoped::scoped_db_for_orgs(&app.state, &auth, &orgs, &["org:admin"])
            .await
            .is_err()
    );
    set_role(&app, target.id, user.id, "admin").await;
    g5_api::db::scoped::scoped_db_for_orgs(&app.state, &auth, &orgs, &["org:admin"])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_org_config_requires_current_target_admin() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut user).await;
    let target = TestOrg::create(&app).await;
    let client = app.authenticated_client(&user);
    let path = format!("/api/v1/orgs/{}/config", target.id);
    let update = json!({"settings":{"securityRegression":"allowed"}});

    set_role(&app, target.id, user.id, "viewer").await;
    assert_eq!(
        client.put_json(&path, &update).await.status(),
        StatusCode::FORBIDDEN
    );
    set_role(&app, target.id, user.id, "member").await;
    assert_eq!(
        client.put_json(&path, &update).await.status(),
        StatusCode::FORBIDDEN
    );
    set_role(&app, target.id, user.id, "admin").await;
    assert_eq!(
        client.put_json(&path, &update).await.status(),
        StatusCode::OK
    );
    let limited = client_for(&app, user.id, source.id, &[Scope::WorkspacesRead]);
    assert_eq!(
        limited.put_json(&path, &update).await.status(),
        StatusCode::FORBIDDEN
    );
    let stale_target_token = client_for(&app, user.id, target.id, &[Scope::WorkspacesWrite]);
    set_role(&app, target.id, user.id, "viewer").await;
    assert_eq!(
        stale_target_token.put_json(&path, &update).await.status(),
        StatusCode::FORBIDDEN
    );

    user.grant_org_scope(&app, target.id, Scope::WorkspacesWrite)
        .await;
    assert_eq!(
        client.put_json(&path, &update).await.status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE user_scope_grants SET expires_at=now()-interval '1 second' WHERE org_id=$1 AND user_id=$2")
        .bind(target.id).bind(user.id).execute(&app.db).await.unwrap();
    assert_eq!(
        client
            .put_json(&path, &json!({"settings":{"securityRegression":"denied"}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let value: String = sqlx::query_scalar(
        "SELECT settings_json->>'securityRegression' FROM org_configs WHERE org_id=$1",
    )
    .bind(target.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(value, "allowed");
    app.cleanup().await;
}

async fn actor_db(app: &TestApp, org: Uuid, actor: Uuid) -> g5_authz::ScopedDb {
    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(actor),
            &[g5_contracts::types::ids::OrgId::from_uuid(org)],
            &["org:admin".into()],
        )
        .await
        .unwrap();
    g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
        .await
        .unwrap()
}

#[tokio::test]
async fn sec_h01_org_config_mutations_cannot_redirect_inherited_credentials() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let victim_workspace = TestWorkspace::create(&app, &org).await;
    let path = format!("/api/v1/orgs/{}/config", org.id);
    let owner_client = app.authenticated_client(&owner);
    let trusted = json!({
        "settings": {
            "ANTHROPIC_BASE_URL": "https://anthropic.example.test",
            "OPENAI_BASE_URL": "https://openai.example.test"
        },
        "setSecrets": {
            "ANTHROPIC_API_KEY": "org-anthropic-credential",
            "OPENAI_API_KEY": "org-openai-credential"
        }
    });
    assert_eq!(
        owner_client.put_json(&path, &trusted).await.status(),
        StatusCode::OK
    );
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM org_configs c WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();

    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let member_client = app.authenticated_client(&member);
    // A stale/elevated token must also fail the current role check, after
    // passing the route's scope guard.
    let elevated = client_for(&app, member.id, org.id, &[Scope::OrgAdmin]);
    for role in ["member", "viewer"] {
        set_role(&app, org.id, member.id, role).await;
        for client in [&member_client, &elevated] {
            assert_eq!(member_client.get(&path).await.status(), StatusCode::OK);
            for body in [
                json!({"settings":{"ANTHROPIC_BASE_URL":"https://attacker.invalid"}}),
                json!({"settings":{"OPENAI_BASE_URL":"https://attacker.invalid"}}),
                json!({"settings":{"arbitrarySetting":"injected"}}),
                json!({"removeSettingKeys":["ANTHROPIC_BASE_URL", "OPENAI_BASE_URL"]}),
                json!({"setSecrets":{"ANTHROPIC_API_KEY":"replacement"}}),
                json!({"setSecrets":{"OPENAI_BASE_URL":"https://attacker.invalid"}}),
                json!({"removeSecretKeys":["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]}),
            ] {
                let response = client.put_json(&path, &body).await;
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{role}: {body}: {}",
                    response.text().await.unwrap()
                );
                let after: Value =
                    sqlx::query_scalar("SELECT to_jsonb(c) FROM org_configs c WHERE org_id=$1")
                        .bind(org.id)
                        .fetch_one(&app.db)
                        .await
                        .unwrap();
                assert_eq!(
                    after, before,
                    "denial must preserve settings, ciphertext and audit fields"
                );
            }
        }
    }

    // Exercise the launch consumer with a different
    // workspace owner's identity and no workspace/user overrides or account.
    let mut db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let launch = app
        .state
        .config_service
        .resolve_effective_for_agent_launch(
            &mut db,
            org.id,
            Some(victim_workspace.id),
            Some(owner.id),
            &[],
            &g5_contracts::types::AgentLaunchPreferencesV1::default(),
            &app.state.provider_account_service,
        )
        .await
        .unwrap();
    for effective in [launch] {
        assert!(effective.account.is_none());
        for (endpoint, credential, expected) in [
            (
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_API_KEY",
                "org-anthropic-credential",
            ),
            ("OPENAI_BASE_URL", "OPENAI_API_KEY", "org-openai-credential"),
        ] {
            assert_eq!(effective.settings[endpoint], trusted["settings"][endpoint]);
            assert_eq!(effective.secrets[credential], expected);
            assert!(!effective.secrets.contains_key(endpoint));
        }
    }
    db.commit().await.unwrap();

    member.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
    assert_eq!(
        elevated.put_json(&path, &trusted).await.status(),
        StatusCode::FORBIDDEN,
        "a scope grant alone does not confer the required Admin role"
    );

    // Both permitted roles can write and remove settings and secrets using
    // the same token; demotion immediately revokes that authority.
    for role in ["admin", "member"] {
        set_role(&app, org.id, member.id, role).await;
        let expected = if role == "member" {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::OK
        };
        for body in [
            json!({"settings":{"testSetting":"value"},"setSecrets":{"TEST_KEY":"value"}}),
            json!({"removeSettingKeys":["testSetting"],"removeSecretKeys":["TEST_KEY"]}),
        ] {
            let response = elevated.put_json(&path, &body).await;
            assert_eq!(
                response.status(),
                expected,
                "{role}: {}",
                response.text().await.unwrap()
            );
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h01_member_keeps_personal_and_owned_workspace_config_writes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&member);
    for path in [
        format!("/api/v1/users/me/config?orgId={}", org.id),
        format!(
            "/api/v1/workspaces/{}/config?orgId={}",
            workspace.id, org.id
        ),
    ] {
        let response = client
            .put_json(
                &path,
                &json!({
                    "settings":{"personalSetting":"value"}, "setSecrets":{"PERSONAL_KEY":"value"}
                }),
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h01_org_config_rechecks_concurrent_demotion() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor =
        TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin, Scope::WorkspacesWrite])
            .await;
    set_role(&app, org.id, actor.id, "admin").await;
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM org_configs c WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let mut demotion = app.db.begin().await.unwrap();
    let demotion_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *demotion)
        .await
        .unwrap();
    sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(actor.id)
        .execute(&mut *demotion)
        .await
        .unwrap();

    // Route admission sees the still-committed admin role. The mutation's
    // transaction must wait for the row lock, then reject the demoted actor.
    let client = app.authenticated_client(&actor);
    let path = format!("/api/v1/orgs/{}/config", org.id);
    let body = json!({"settings":{"OPENAI_BASE_URL":"https://attacker.invalid"}});
    let request = client.put_json(&path, &body);
    tokio::pin!(request);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            tokio::select! {
                response = &mut request => panic!("mutation escaped the authority lock: {}", response.status()),
                _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
            }
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))",
            ).bind(demotion_pid).fetch_one(&app.db).await.unwrap();
            if waiting {
                break;
            }
        }
    }).await.expect("config mutation must reach the authority lock");
    demotion.commit().await.unwrap();
    assert_eq!(request.await.status(), StatusCode::FORBIDDEN);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM org_configs c WHERE org_id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(after, before);
    app.cleanup().await;
}

async fn pending_invitation(app: &TestApp, org: Uuid, actor: Uuid, email: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO org_invitations (id, org_id, email, role, invited_by, status, expires_at) VALUES ($1,$2,$3,'member',$4,'pending',now()+interval '1 day')")
        .bind(id).bind(org).bind(email).bind(actor).execute(&app.db).await.unwrap();
    id
}

#[tokio::test]
async fn sec_h02_all_org_mutations_wait_for_current_actor_authority() {
    use g5_api::services::org_service::{InviteMemberRequest, UpdateOrgRequest};
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let target = TestUser::create(&app).await;
    let invitation = pending_invitation(&app, org.id, actor.id, "pending@g5.test").await;
    for action in [
        "update",
        "invite_existing",
        "invite_new",
        "resend",
        "revoke",
    ] {
        set_role(&app, org.id, actor.id, "admin").await;
        let mut demotion = app.db.begin().await.unwrap();
        sqlx::query("UPDATE org_memberships SET role='member' WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(actor.id)
            .execute(&mut *demotion)
            .await
            .unwrap();
        let mut db = actor_db(&app, org.id, actor.id).await;
        {
            let mutation = async {
                let service = &app.state.org_service;
                match action {
                    "update" => service
                        .update_org(
                            &mut db,
                            org.id,
                            actor.id,
                            UpdateOrgRequest {
                                name: Some("unauthorized".into()),
                                slug: None,
                            },
                        )
                        .await
                        .map(|_| ()),
                    "invite_existing" => service
                        .invite_member(
                            &mut db,
                            org.id,
                            actor.id,
                            InviteMemberRequest {
                                email: target.email.clone(),
                                role: "admin".into(),
                            },
                        )
                        .await
                        .map(|_| ()),
                    "invite_new" => service
                        .invite_member(
                            &mut db,
                            org.id,
                            actor.id,
                            InviteMemberRequest {
                                email: "new@g5.test".into(),
                                role: "admin".into(),
                            },
                        )
                        .await
                        .map(|_| ()),
                    "resend" => service
                        .resend_invitation(&mut db, invitation, actor.id)
                        .await
                        .map(|_| ()),
                    "revoke" => {
                        service
                            .revoke_invitation(&mut db, invitation, actor.id)
                            .await
                    }
                    _ => unreachable!(),
                }
            };
            tokio::pin!(mutation);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), &mut mutation)
                    .await
                    .is_err(),
                "{action} must wait for the actor row"
            );
            demotion.commit().await.unwrap();
            assert!(
                matches!(
                    mutation.await,
                    Err(g5_contracts::error::G5Error::Forbidden { .. })
                ),
                "{action}"
            );
        }
        drop(db);
    }
    let name: String = sqlx::query_scalar("SELECT name FROM orgs WHERE id=$1")
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name, org.name);
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM org_memberships WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(target.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(members, 0);
    let invitations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM org_invitations WHERE org_id=$1")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(invitations, 1);
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_waiting_admin_cannot_mutate_an_accepted_invitation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    set_role(&app, org.id, actor.id, "admin").await;
    for action in ["resend", "revoke"] {
        let recipient = TestUser::create(&app).await;
        let invitation = pending_invitation(&app, org.id, actor.id, &recipient.email).await;
        let expires: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT expires_at FROM org_invitations WHERE id=$1")
                .bind(invitation)
                .fetch_one(&app.db)
                .await
                .unwrap();
        let mut blocker = app.db.begin().await.unwrap();
        sqlx::query(
            "SELECT user_id FROM org_memberships WHERE org_id=$1 AND user_id=$2 FOR UPDATE",
        )
        .bind(org.id)
        .bind(actor.id)
        .execute(&mut *blocker)
        .await
        .unwrap();
        let mut db = actor_db(&app, org.id, actor.id).await;
        {
            let mutation = async {
                if action == "resend" {
                    app.state
                        .org_service
                        .resend_invitation(&mut db, invitation, actor.id)
                        .await
                        .map(|_| ())
                } else {
                    app.state
                        .org_service
                        .revoke_invitation(&mut db, invitation, actor.id)
                        .await
                }
            };
            tokio::pin!(mutation);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), &mut mutation)
                    .await
                    .is_err()
            );
            let response = app
                .authenticated_client(&recipient)
                .post_json(
                    &format!("/api/v1/invitations/{invitation}/accept"),
                    &json!({}),
                )
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{}",
                response.text().await.unwrap()
            );
            blocker.commit().await.unwrap();
            assert!(
                matches!(
                    mutation.await,
                    Err(g5_contracts::error::G5Error::Conflict { .. })
                ),
                "{action}"
            );
        }
        drop(db);
        let row: (String, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT status, expires_at FROM org_invitations WHERE id=$1")
                .bind(invitation)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert_eq!(row, ("accepted".into(), expires));
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_concurrent_admin_demotions_preserve_retryability_and_authority() {
    use g5_api::services::permission_service::PermissionService;
    use g5_contracts::error::G5Error;
    let app = TestApp::spawn().await;
    let mut a = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut a).await;
    let b = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    set_role(&app, org.id, a.id, "admin").await;
    set_role(&app, org.id, b.id, "admin").await;
    let mut a_db = actor_db(&app, org.id, a.id).await;
    let mut b_db = actor_db(&app, org.id, b.id).await;
    // Hold the same production actor locks before both requests reach the
    // member UPDATE, deterministically producing the mutual-demotion cycle.
    PermissionService::require_org_admin_on_conn(a_db.conn(), a.id, org.id)
        .await
        .unwrap();
    PermissionService::require_org_admin_on_conn(b_db.conn(), b.id, org.id)
        .await
        .unwrap();
    let both = async {
        tokio::join!(
            async {
                let result = app
                    .state
                    .org_service
                    .change_member(
                        &mut a_db,
                        org.id,
                        a.id,
                        b.id,
                        g5_api::services::org_service::MemberChange::Role("member".into()),
                    )
                    .await;
                if result.is_ok() {
                    a_db.commit().await.unwrap();
                } else {
                    drop(a_db);
                }
                result
            },
            async {
                let result = app
                    .state
                    .org_service
                    .change_member(
                        &mut b_db,
                        org.id,
                        b.id,
                        a.id,
                        g5_api::services::org_service::MemberChange::Role("member".into()),
                    )
                    .await;
                if result.is_ok() {
                    b_db.commit().await.unwrap();
                } else {
                    drop(b_db);
                }
                result
            }
        )
    };
    let (a_result, b_result) = tokio::time::timeout(std::time::Duration::from_secs(15), both)
        .await
        .unwrap();
    let loser = match (a_result, b_result) {
        (Err(G5Error::DatabaseRetryable { .. }), Ok(_)) => &a,
        (Ok(_), Err(G5Error::DatabaseRetryable { .. })) => &b,
        other => panic!("one transaction must succeed and one be retryable: {other:?}"),
    };
    // A retry must re-authorize the stale admin token after the winning
    // transaction has demoted its holder.
    let response = app
        .authenticated_client(loser)
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", org.id, loser.id),
            &json!({"role":"admin"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let admins: i64 =
        sqlx::query_scalar("SELECT count(*) FROM org_memberships WHERE org_id=$1 AND role='admin'")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(admins, 1);
    app.cleanup().await;
}

fn authority_auth(user: Uuid, org: Uuid) -> g5_api::middleware::AuthContext {
    g5_api::middleware::AuthContext::User(g5_auth::JwtClaims {
        sub: user.to_string(),
        org: org.to_string(),
        scopes: vec![Scope::OrgAdmin, Scope::BillingAdmin],
        iat: 0,
        exp: 4_102_444_800,
        iss: None,
        aud: None,
        jti: None,
        token_use: None,
        client: None,
    })
}

#[tokio::test]
async fn sec_h02_repository_scope_checks_target_authority_and_workspace_binding() {
    use g5_api::db::scoped::scoped_db_for_workspace_repo;
    let app = TestApp::spawn().await;
    let mut actor = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut actor).await;
    let mut owner = TestUser::create(&app).await;
    let target = TestOrg::create_with_admin(&app, &mut owner).await;
    set_role(&app, target.id, actor.id, "member").await;
    let granted = TestWorkspace::create(&app, &target).await;
    let sibling = TestWorkspace::create(&app, &target).await;
    let foreign = TestOrg::create(&app).await;
    let foreign_workspace = TestWorkspace::create(&app, &foreign).await;
    let auth = authority_auth(actor.id, source.id);

    let result = scoped_db_for_workspace_repo(
        &app.state,
        &auth,
        granted.id,
        granted.repo_id,
        &["billing:admin"],
    )
    .await;
    assert_eq!(
        result
            .err()
            .expect("source-org authority must not authorize target repository")
            .inner
            .status_code(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("INSERT INTO oauth_scopes (scope,description,category) VALUES ('billing:admin','Billing administration','billing') ON CONFLICT DO NOTHING")
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_permission_overrides (workspace_id,user_id,scope,granted_by) VALUES ($1,$2,'billing:admin',$3)")
        .bind(granted.id).bind(actor.id).bind(owner.id).execute(&app.db).await.unwrap();
    scoped_db_for_workspace_repo(
        &app.state,
        &auth,
        granted.id,
        granted.repo_id,
        &["billing:admin"],
    )
    .await
    .unwrap()
    .commit()
    .await
    .unwrap();
    let result = scoped_db_for_workspace_repo(
        &app.state,
        &auth,
        sibling.id,
        sibling.repo_id,
        &["billing:admin"],
    )
    .await;
    assert_eq!(
        result
            .err()
            .expect("workspace override must not authorize sibling repository")
            .inner
            .status_code(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("DELETE FROM workspace_permission_overrides WHERE workspace_id=$1 AND user_id=$2")
        .bind(granted.id)
        .bind(actor.id)
        .execute(&app.db)
        .await
        .unwrap();
    let result = scoped_db_for_workspace_repo(
        &app.state,
        &auth,
        granted.id,
        granted.repo_id,
        &["billing:admin"],
    )
    .await;
    assert_eq!(
        result
            .err()
            .expect("cached capability must not retain revoked override")
            .inner
            .status_code(),
        StatusCode::FORBIDDEN
    );

    // Exercise the current HTTP repository consumer too: valid cross-org reads
    // remain available, but neither a sibling nor foreign repository ID binds.
    let client = app.authenticated_client(&actor);
    for (workspace, repo, expected) in [
        (granted.id, granted.repo_id, StatusCode::OK),
        (granted.id, sibling.repo_id, StatusCode::NOT_FOUND),
        (granted.id, foreign_workspace.repo_id, StatusCode::NOT_FOUND),
        (
            foreign_workspace.id,
            foreign_workspace.repo_id,
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = client
            .get(&format!(
                "/api/v1/workspaces/{workspace}/repos/{repo}/preview-profiles"
            ))
            .await;
        assert_eq!(
            response.status(),
            expected,
            "{}",
            response.text().await.unwrap()
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_external_issue_switch_requires_current_target_admin() {
    let app = TestApp::spawn().await;
    let mut actor = TestUser::create(&app).await;
    let source = TestOrg::create_with_admin(&app, &mut actor).await;
    let target = TestOrg::create(&app).await;
    let client = app.authenticated_client(&actor);
    let path = format!("/api/v1/orgs/{}/config", target.id);
    for role in ["viewer", "member", "admin", "member"] {
        set_role(&app, target.id, actor.id, role).await;
        let expected = if role == "admin" {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        for body in [
            json!({"settings":{"external_issues_enabled":true}}),
            json!({"settings":{"external_issues_enabled":false}}),
            json!({"removeSettingKeys":["external_issues_enabled"]}),
        ] {
            let response = client.put_json(&path, &body).await;
            assert_eq!(
                response.status(),
                expected,
                "{role}: {}",
                response.text().await.unwrap()
            );
        }
    }
    set_role(&app, target.id, actor.id, "admin").await;
    let limited = client_for(&app, actor.id, source.id, &[Scope::WorkspacesRead]);
    assert_eq!(
        limited
            .put_json(&path, &json!({"settings":{"external_issues_enabled":true}}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let stored: Option<bool> = sqlx::query_scalar("SELECT (settings_json->>'external_issues_enabled')::boolean FROM org_configs WHERE org_id=$1")
        .bind(target.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        stored, None,
        "denied writes must not restore the removed switch"
    );
    app.cleanup().await;
}

/// The Re-run Verification switch (ui g5.workbench.review-prompts): only an
/// organization administrator sets it, every member reads it from the
/// organization's config, and no workspace or personal scope can carry it.
#[tokio::test]
async fn verification_action_switch_is_an_admin_set_org_only_boolean() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&member);
    let org_path = format!("/api/v1/orgs/{}/config", org.id);
    let on = json!({"settings":{"verification_action_enabled":true}});

    // Off until an administrator sets it: a member reads no value.
    let read = member_client.get(&org_path).await;
    assert_eq!(read.status(), StatusCode::OK);
    let body: Value = read.json().await.unwrap();
    assert!(body["data"]["settings"]
        .get("verification_action_enabled")
        .is_none());

    let denied = member_client.put_json(&org_path, &on).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let response = admin_client.put_json(&org_path, &on).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let body: Value = member_client.get(&org_path).await.json().await.unwrap();
    assert_eq!(
        body["data"]["settings"]["verification_action_enabled"],
        true
    );

    let not_boolean = admin_client
        .put_json(
            &org_path,
            &json!({"settings":{"verification_action_enabled":"true"}}),
        )
        .await;
    assert_eq!(not_boolean.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(not_boolean
        .text()
        .await
        .unwrap()
        .contains("invalid_setting_type"));

    // A member's own config and the workspace owner's (here the administrator)
    // accept other settings, but never this key.
    for (client, path) in [
        (
            &member_client,
            format!("/api/v1/users/me/config?orgId={}", org.id),
        ),
        (
            &admin_client,
            format!(
                "/api/v1/workspaces/{}/config?orgId={}",
                workspace.id, org.id
            ),
        ),
    ] {
        let other = client
            .put_json(&path, &json!({"settings":{"personalSetting":"value"}}))
            .await;
        assert_eq!(other.status(), StatusCode::OK, "{path}");
        let response = client.put_json(&path, &on).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
        assert!(
            response.text().await.unwrap().contains("org_only_setting"),
            "{path}"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_workspace_admission_holds_live_context_through_permission_mutation() {
    use g5_api::db::scoped::scoped_system_db_for_workspace;
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let victim = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    set_role(&app, org.id, actor.id, "admin").await;
    victim
        .grant_org_scope(&app, org.id, Scope::WorkspacesRead)
        .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let auth = authority_auth(actor.id, org.id);
    let mut db = scoped_system_db_for_workspace(&app.state, &auth, workspace.id, &["org:admin"])
        .await
        .unwrap();
    let mut deleter = app.db.acquire().await.unwrap();
    let deletion = sqlx::query("UPDATE workspaces SET deleted_at=clock_timestamp() WHERE id=$1")
        .bind(workspace.id)
        .execute(&mut *deleter);
    tokio::pin!(deletion);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut deletion)
            .await
            .is_err(),
        "soft deletion escaped the admitted workspace transaction"
    );
    app.state
        .permission_service
        .set_workspace_override(
            &mut db,
            org.id,
            workspace.id,
            victim.id,
            vec!["workspaces:read".into()],
            actor.id,
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), deletion)
        .await
        .unwrap()
        .unwrap();
    assert!(
        scoped_system_db_for_workspace(&app.state, &auth, workspace.id, &["org:admin"])
            .await
            .is_err(),
        "a new mutation must reject the tombstoned workspace"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_workspace_admission_waits_for_deletion_and_rechecks_its_outcome() {
    use g5_api::db::scoped::{
        authorize_org_scopes, scoped_db_for_workspace, scoped_system_db_for_workspace,
    };
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let actor = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::OrgAdmin,
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
        ],
    )
    .await;
    set_role(&app, org.id, actor.id, "admin").await;
    let auth = authority_auth(actor.id, org.id);
    for tenant in [false, true] {
        for commit_delete in [true, false] {
            let workspace = TestWorkspace::create(&app, &org).await;
            let mut deletion = app.db.begin().await.unwrap();
            sqlx::query("UPDATE workspaces SET deleted_at=clock_timestamp() WHERE id=$1")
                .bind(workspace.id)
                .execute(&mut *deletion)
                .await
                .unwrap();
            let admission = async {
                if tenant {
                    // The collaborative comment path admits visibility first,
                    // then write authority on the same tenant connection.
                    let mut db = scoped_db_for_workspace(
                        &app.state,
                        &auth,
                        workspace.id,
                        &["workspaces:read"],
                    )
                    .await?;
                    authorize_org_scopes(
                        &mut db,
                        &auth,
                        org.id,
                        Some(workspace.id),
                        &["workspaces:write"],
                    )
                    .await?;
                    Ok(db)
                } else {
                    scoped_system_db_for_workspace(&app.state, &auth, workspace.id, &["org:admin"])
                        .await
                }
            };
            tokio::pin!(admission);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), &mut admission)
                    .await
                    .is_err(),
                "tenant={tenant}: admission did not wait for the workspace deletion"
            );
            if commit_delete {
                deletion.commit().await.unwrap();
            } else {
                deletion.rollback().await.unwrap();
            }
            let result = tokio::time::timeout(Duration::from_secs(3), admission)
                .await
                .unwrap();
            if commit_delete {
                assert!(
                    result.is_err(),
                    "tenant={tenant}: committed tombstone was admitted"
                );
            } else {
                result
                    .expect("rolled-back deletion must leave admission usable")
                    .commit()
                    .await
                    .unwrap();
            }
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_shared_admission_holds_authority_until_permission_mutation_commits() {
    use g5_api::db::scoped::{scoped_system_db_for_org, scoped_system_db_for_workspace};
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let victim = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let auth = authority_auth(actor.id, org.id);
    for source in ["role", "grant", "default", "workspace"] {
        set_role(
            &app,
            org.id,
            actor.id,
            if source == "role" { "admin" } else { "member" },
        )
        .await;
        actor.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
        if source != "grant" {
            sqlx::query("DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(actor.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        if source == "default" {
            sqlx::query("INSERT INTO org_default_scopes (org_id,scope) SELECT $1,'org:admin' WHERE $2::uuid IS NOT NULL")
                .bind(org.id).bind(actor.id).execute(&app.db).await.unwrap();
        }
        if source == "workspace" {
            sqlx::query("INSERT INTO workspace_permission_overrides (workspace_id,user_id,scope,granted_by) VALUES ($1,$2,'org:admin',$2)")
                .bind(workspace.id).bind(actor.id).execute(&app.db).await.unwrap();
        }
        let mut db = if source == "workspace" {
            scoped_system_db_for_workspace(&app.state, &auth, workspace.id, &["org:admin"])
                .await
                .unwrap()
        } else {
            scoped_system_db_for_org(&app.state, &auth, org.id, &["org:admin"])
                .await
                .unwrap()
        };
        let sql = match source {
            "role" => "UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2",
            "grant" => "DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2",
            "default" => "DELETE FROM org_default_scopes WHERE org_id=$1 AND $2::uuid IS NOT NULL",
            _ => "DELETE FROM workspace_permission_overrides WHERE workspace_id=$1 AND user_id=$2",
        };
        let mut revoker = app.db.acquire().await.unwrap();
        let revocation = sqlx::query(sql)
            .bind(if source == "workspace" {
                workspace.id
            } else {
                org.id
            })
            .bind(actor.id)
            .execute(&mut *revoker);
        tokio::pin!(revocation);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut revocation)
                .await
                .is_err(),
            "{source} revocation escaped the admitted transaction"
        );
        app.state
            .permission_service
            .set_user_scopes(
                &mut db,
                org.id,
                victim.id,
                vec!["org:admin".into()],
                None,
                actor.id,
            )
            .await
            .unwrap();
        db.commit().await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), revocation)
            .await
            .unwrap()
            .unwrap();
        assert!(
            scoped_system_db_for_workspace(&app.state, &auth, workspace.id, &["org:admin"])
                .await
                .is_err(),
            "{source}: revoked authority admitted a new transaction"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_workspace_scope_grant_is_resolved_only_for_its_workspace() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::BillingAdmin]).await;
    set_role(&app, org.id, user.id, "member").await;
    let granted = TestWorkspace::create(&app, &org).await;
    let sibling = TestWorkspace::create(&app, &org).await;
    user.grant_org_scope(&app, org.id, Scope::BillingAdmin)
        .await;
    sqlx::query("DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_permission_overrides (workspace_id,user_id,scope,granted_by) VALUES ($1,$2,'billing:admin',$2)")
        .bind(granted.id).bind(user.id).execute(&app.db).await.unwrap();
    let client = app.authenticated_client(&user);
    for suffix in ["settings/spend", "settings/spend/history"] {
        let response = client
            .get(&format!("/api/v1/workspaces/{}/{suffix}", granted.id))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        assert_eq!(
            client
                .get(&format!("/api/v1/workspaces/{}/{suffix}", sibling.id))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let auth = authority_auth(user.id, org.id);
    assert!(
        g5_api::db::scoped::scoped_db_for_org(&app.state, &auth, org.id, &["billing:admin"])
            .await
            .is_err()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_admission_waits_for_revocation_and_checks_the_locked_version() {
    use g5_api::db::scoped::scoped_system_db_for_org;
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let auth = authority_auth(actor.id, org.id);
    for source in ["role", "grant", "default", "expiry"] {
        set_role(
            &app,
            org.id,
            actor.id,
            if source == "role" { "admin" } else { "member" },
        )
        .await;
        actor.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
        if matches!(source, "role" | "default") {
            sqlx::query("DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(actor.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        if source == "default" {
            sqlx::query("INSERT INTO org_default_scopes (org_id,scope) VALUES ($1,'org:admin')")
                .bind(org.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        let mut revocation = app.db.begin().await.unwrap();
        let sql = match source {
            "role" => "UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2",
            "grant" => "DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2",
            "default" => "DELETE FROM org_default_scopes WHERE org_id=$1 AND $2::uuid IS NOT NULL",
            _ => "UPDATE user_scope_grants SET expires_at=clock_timestamp()-interval '1 second' WHERE org_id=$1 AND user_id=$2",
        };
        sqlx::query(sql)
            .bind(org.id)
            .bind(actor.id)
            .execute(&mut *revocation)
            .await
            .unwrap();
        let admission = scoped_system_db_for_org(&app.state, &auth, org.id, &["org:admin"]);
        tokio::pin!(admission);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut admission)
                .await
                .is_err(),
            "{source}: admission did not wait for revocation"
        );
        revocation.commit().await.unwrap();
        assert!(
            matches!(admission.await, Err(error) if error.to_string().contains("org:admin")),
            "{source}: revoked authority was accepted"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_org_service_holds_additive_admin_authority_until_commit() {
    use g5_api::services::org_service::UpdateOrgRequest;
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let actor = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    set_role(&app, org.id, actor.id, "member").await;
    for source in ["grant", "default"] {
        actor.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
        if source == "default" {
            sqlx::query("DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(actor.id)
                .execute(&app.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO org_default_scopes (org_id,scope) VALUES ($1,'org:admin')")
                .bind(org.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        let mut db = actor_db(&app, org.id, actor.id).await;
        app.state
            .org_service
            .update_org(
                &mut db,
                org.id,
                actor.id,
                UpdateOrgRequest {
                    name: Some(format!("Authorized {source}")),
                    slug: None,
                },
            )
            .await
            .unwrap();
        let sql = if source == "grant" {
            "DELETE FROM user_scope_grants WHERE org_id=$1 AND user_id=$2"
        } else {
            "DELETE FROM org_default_scopes WHERE org_id=$1 AND $2::uuid IS NOT NULL"
        };
        let mut conn = app.db.acquire().await.unwrap();
        let revocation = sqlx::query(sql)
            .bind(org.id)
            .bind(actor.id)
            .execute(&mut *conn);
        tokio::pin!(revocation);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut revocation)
                .await
                .is_err(),
            "{source}: service released its authority early"
        );
        db.commit().await.unwrap();
        revocation.await.unwrap();
    }
    app.cleanup().await;
}

#[tokio::test]
async fn sec_h02_tenant_scope_lookup_is_bound_to_verified_actor_org_and_workspace() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let other = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &other).await;
    let lookup = "SELECT role, granted_scopes FROM authz.lock_user_scope_authority($1,$2,$3,ARRAY['org:admin'])";
    let no_context = sqlx::query_as::<_, (String, Vec<String>)>(lookup)
        .bind(user.id)
        .bind(org.id)
        .bind(None::<Uuid>)
        .fetch_optional(&app.state.tenant_pool)
        .await
        .unwrap();
    assert!(no_context.is_none());
    let auth = authority_auth(user.id, org.id);
    let mut db = g5_api::db::scoped::scoped_db_for_org(&app.state, &auth, org.id, &[])
        .await
        .unwrap();
    for (actor, target, ws) in [
        (Uuid::new_v4(), org.id, None),
        (user.id, other.id, None),
        (user.id, org.id, Some(workspace.id)),
    ] {
        let result = sqlx::query_as::<_, (String, Vec<String>)>(lookup)
            .bind(actor)
            .bind(target)
            .bind(ws)
            .fetch_optional(db.conn())
            .await
            .unwrap();
        assert!(
            result.is_none(),
            "lookup exposed authority outside the verified context"
        );
    }
    let result = sqlx::query_as::<_, (String, Vec<String>)>(lookup)
        .bind(user.id)
        .bind(org.id)
        .bind(None::<Uuid>)
        .fetch_optional(db.conn())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.0, "admin");
    let privileges: (bool, bool) = sqlx::query_as("SELECT has_table_privilege(current_user, 'public.org_memberships', 'SELECT'), has_table_privilege(current_user, 'public.user_scope_grants', 'SELECT')").fetch_one(db.conn()).await.unwrap();
    assert_eq!(
        privileges,
        (false, false),
        "tenant must not gain direct authority-table access"
    );
    db.commit().await.unwrap();
    app.cleanup().await;
}
