//! Platform operator organization (`g5.organization.platform-operator`,
//! G5-Lean#776): identity, authority, the cross-tenant directory, operator
//! creation, operator-only creation and the dev/E2E operator fixture.

use crate::helpers;

use helpers::platform::{create_org_as, make_operator, operator_org};
use helpers::{EnvVarGuard, TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn body(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn membership_role(app: &TestApp, org_id: Uuid, user_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn schema_keeps_one_operator_org_with_the_fixed_slug() {
    let app = TestApp::spawn().await;
    operator_org(&app).await;

    let second = sqlx::query(
        "INSERT INTO orgs (name, slug, kind) VALUES ('Second', 'second-operator', 'operator')",
    )
    .execute(&app.db)
    .await;
    assert!(second.is_err(), "a second operator org must be refused");

    let wrong_slug = sqlx::query("UPDATE orgs SET slug = 'renamed' WHERE kind = 'operator'")
        .execute(&app.db)
        .await;
    assert!(wrong_slug.is_err(), "the operator org's slug is fixed");

    let tenant = TestOrg::create(&app).await;
    let kind: String = sqlx::query_scalar("SELECT kind FROM orgs WHERE id = $1")
        .bind(tenant.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(kind, "tenant", "existing and new orgs default to tenant");
    app.cleanup().await;
}

#[tokio::test]
async fn platform_routes_require_current_operator_membership() {
    let app = TestApp::spawn().await;
    let outsider = TestUser::create(&app).await;
    let request = json!({ "name": "Outsider Org", "adminEmail": outsider.email });

    // No operator org yet, then an operator org the caller is not part of.
    for _ in 0..2 {
        let client = app.authenticated_client(&outsider);
        let (status, _) = body(client.get("/api/v1/platform/orgs").await).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = body(client.post_json("/api/v1/platform/orgs", &request).await).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        operator_org(&app).await;
    }

    // Viewers observe but cannot create.
    let viewer = TestUser::create(&app).await;
    make_operator(&app, &viewer, "viewer").await;
    let client = app.authenticated_client(&viewer);
    let (status, _) = body(client.get("/api/v1/platform/orgs").await).await;
    assert_eq!(status, StatusCode::OK);
    let (status, error) = body(client.post_json("/api/v1/platform/orgs", &request).await).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{error}");

    // A deactivated operator keeps the row but loses platform authority.
    let former = TestUser::create(&app).await;
    let operator_org_id = make_operator(&app, &former, "admin").await;
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = now(), activity_revision = activity_revision + 1
         WHERE org_id = $1 AND user_id = $2",
    )
    .bind(operator_org_id)
    .bind(former.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, _) = body(
        app.authenticated_client(&former)
            .get("/api/v1/platform/orgs")
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let none: i64 = sqlx::query_scalar("SELECT count(*) FROM orgs WHERE name = 'Outsider Org'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(none, 0, "refused requests create nothing");
    app.cleanup().await;
}

#[tokio::test]
async fn directory_lists_only_tenant_platform_facts() {
    let app = TestApp::spawn().await;
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "viewer").await;
    let mut owner = TestUser::create(&app).await;
    let tenant = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(tenant.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();

    let (status, value) = body(
        app.authenticated_client(&operator)
            .get("/api/v1/platform/orgs")
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let orgs = value["data"].as_array().expect("org list");
    assert!(
        orgs.iter().all(|org| org["slug"] != "operator"),
        "the operator org is not a tenant: {orgs:?}"
    );
    let listed = orgs
        .iter()
        .find(|org| org["id"] == tenant.id.to_string())
        .expect("tenant listed");
    assert_eq!(listed["slug"], tenant.slug);
    assert_eq!(listed["memberCount"], 2);
    assert!(listed["createdAt"].is_string());
    let mut keys: Vec<&str> = listed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["createdAt", "id", "memberCount", "name", "slug"],
        "only the tenant-boundary facts are exposed"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn operator_creates_an_org_for_an_existing_owner_without_joining_it() {
    let app = TestApp::spawn().await;
    let operator = TestUser::create(&app).await;
    let operator_org_id = make_operator(&app, &operator, "member").await;
    let owner = TestUser::create(&app).await;

    let response = app
        .authenticated_client(&operator)
        .post_json(
            "/api/v1/platform/orgs",
            &json!({
                "name": "Customer One",
                "slug": "customer-one",
                "adminEmail": format!("  {}  ", owner.email.to_uppercase()),
            }),
        )
        .await;
    let (status, value) = body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    let data = &value["data"];
    assert_eq!(data["org"]["slug"], "customer-one");
    assert_eq!(data["admin"]["userId"], owner.id.to_string());
    assert!(data.get("owner").is_none());
    assert_eq!(data["admin"]["accountCreated"], false);
    assert!(
        data.get("baseWorkspace").is_none(),
        "tenant ids stay inside the tenant"
    );

    let org_id = Uuid::parse_str(data["org"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        membership_role(&app, org_id, owner.id).await.as_deref(),
        Some("admin")
    );
    assert_eq!(
        membership_role(&app, org_id, operator.id).await,
        None,
        "the operator gains no tenant membership"
    );
    let base_owner: Uuid =
        sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE org_id = $1 AND is_base")
            .bind(org_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        base_owner, owner.id,
        "the ordinary cascade ran with the owner"
    );
    assert_eq!(
        membership_role(&app, operator_org_id, operator.id)
            .await
            .as_deref(),
        Some("member")
    );

    // The tenant is now on the directory, and its owner sees it.
    let (_, directory) = body(
        app.authenticated_client(&operator)
            .get("/api/v1/platform/orgs")
            .await,
    )
    .await;
    assert!(directory["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|org| org["slug"] == "customer-one"));
    let (_, me) = body(
        app.authenticated_client(&owner)
            .get("/api/v1/users/me")
            .await,
    )
    .await;
    let membership = me["data"]["orgs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|org| org["orgSlug"] == "customer-one")
        .cloned()
        .expect("owner sees the new org");
    assert_eq!(membership["orgKind"], "tenant");
    assert_eq!(membership["role"], "admin");

    // The slug is unique.
    let (status, _) = body(
        app.authenticated_client(&operator)
            .post_json(
                "/api/v1/platform/orgs",
                &json!({ "name": "Again", "slug": "customer-one", "adminEmail": owner.email }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    app.cleanup().await;
}

#[tokio::test]
async fn operator_creates_the_owner_account_when_none_exists() {
    let app = TestApp::spawn().await;
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "admin").await;
    let email = format!("new-owner-{}@customer.test", Uuid::new_v4());

    let (status, value) = body(
        app.authenticated_client(&operator)
            .post_json(
                "/api/v1/platform/orgs",
                &json!({ "name": "Customer Two", "adminEmail": email.to_uppercase() }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    assert_eq!(value["data"]["admin"]["accountCreated"], true);
    assert_eq!(value["data"]["admin"]["email"], email);

    let (user_id, provider, display_name): (Uuid, Option<String>, String) =
        sqlx::query_as("SELECT id, oauth_provider, display_name FROM users WHERE email = $1")
            .bind(&email)
            .fetch_one(&app.db)
            .await
            .expect("account created with the normalized email");
    assert_eq!(
        provider, None,
        "no sign-in identity until the person signs in"
    );
    assert!(email.starts_with(&display_name));
    let org_id = Uuid::parse_str(value["data"]["org"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        membership_role(&app, org_id, user_id).await.as_deref(),
        Some("admin")
    );
    app.cleanup().await;
}

#[tokio::test]
async fn invalid_owner_email_and_reserved_slugs_are_refused() {
    let app = TestApp::spawn().await;
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "member").await;
    let owner = TestUser::create(&app).await;
    let client = app.authenticated_client(&operator);

    let (status, _) = body(
        client
            .post_json(
                "/api/v1/platform/orgs",
                &json!({ "name": "Bad Email", "adminEmail": "not-an-address" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    for slug in [
        "operator",
        "admin",
        "api",
        "create-org",
        "no-organization",
        "login",
    ] {
        let created = create_org_as(
            &app,
            &operator,
            &owner,
            json!({ "name": "Reserved", "slug": slug }),
        )
        .await;
        assert_eq!(
            created.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "slug {slug} is reserved"
        );
    }
    // A name whose generated slug is reserved is refused too.
    let created = create_org_as(&app, &operator, &owner, json!({ "name": "Operator" })).await;
    assert_eq!(created.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let reserved: i64 = sqlx::query_scalar("SELECT count(*) FROM orgs WHERE name = 'Reserved'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(reserved, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn slugs_stay_reserved_and_fixed_on_rename() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let tenant = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let (status, _) = body(
        client
            .patch_json(
                &format!("/api/v1/orgs/{}", tenant.id),
                &json!({ "slug": "operator" }),
            )
            .await,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "tenants cannot take a reserved slug"
    );

    let operator_org_id = make_operator(&app, &owner, "admin").await;
    let (status, _) = body(
        client
            .patch_json(
                &format!("/api/v1/orgs/{operator_org_id}"),
                &json!({ "slug": "ops" }),
            )
            .await,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the operator slug cannot change"
    );
    let (status, _) = body(
        client
            .patch_json(
                &format!("/api/v1/orgs/{operator_org_id}"),
                &json!({ "name": "Platform team" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the operator org's name can change");
    app.cleanup().await;
}

#[tokio::test]
async fn self_serve_org_creation_does_not_exist() {
    let app = TestApp::spawn().await;
    let user = TestUser::create(&app).await;
    let response = app
        .authenticated_client(&user)
        .post_json(
            "/api/v1/orgs",
            &json!({ "name": "Self Serve", "slug": "self-serve" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orgs WHERE slug = 'self-serve'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}

#[tokio::test]
#[serial_test::serial(e2e_auth)]
async fn e2e_operator_persona_operates_the_platform() {
    let _env = EnvVarGuard::set_many(&[("G5_E2E_AUTH", "true")]);
    let app = TestApp::spawn().await;
    let email = g5_api::services::platform_operator::E2E_OPERATOR_EMAIL;

    for _ in 0..2 {
        g5_api::routes::auth::provision_test_token(&app.state, email)
            .await
            .expect("operator persona provisioning is idempotent");
    }
    let (org_id, user_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT o.id, m.user_id FROM orgs o
         JOIN org_memberships m ON m.org_id = o.id
         JOIN users u ON u.id = m.user_id
         WHERE o.kind = 'operator' AND o.slug = 'operator' AND u.email = $1",
    )
    .bind(email)
    .fetch_one(&app.db)
    .await
    .expect("the persona belongs to the one operator org");
    assert_eq!(
        membership_role(&app, org_id, user_id).await.as_deref(),
        Some("admin")
    );
    let base: i64 =
        sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE org_id = $1 AND is_base")
            .bind(org_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(base, 1, "the operator org keeps its Base workspace");

    // Other personas are not operators.
    g5_api::routes::auth::provision_test_token(&app.state, "demo@g5labs.ai")
        .await
        .unwrap();
    let operators: i64 =
        sqlx::query_scalar("SELECT count(*) FROM org_memberships WHERE org_id = $1")
            .bind(org_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(operators, 1);
    app.cleanup().await;
}

/// Write `files` as the single parentless commit on `main` of `repo_id`'s
/// mirror, as the platform seed's mirror initializer does.
async fn seed_first_commit(app: &TestApp, repo_id: Uuid, files: &[(&str, &str)]) -> String {
    let path = app.repo_path(repo_id).await;
    let repo = git2::Repository::init_bare(&path).expect("init seeded mirror");
    let mut tree = repo.treebuilder(None).unwrap();
    for (name, content) in files {
        let blob = repo.blob(content.as_bytes()).unwrap();
        tree.insert(name, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(tree.write().unwrap()).unwrap();
    let author = git2::Signature::now("G5 Seed", "seed@g5labs.ai").unwrap();
    let commit = repo
        .commit(
            Some("refs/heads/main"),
            &author,
            &author,
            "Initialise repository",
            &tree,
            &[],
        )
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    commit.to_string()
}

/// The operator org's Base exactly as `scripts/seed-prod-org.sh` leaves it:
/// the non-login service user owns it through a `member` membership, the
/// docs member has an empty first commit and, when `with_code`, the internal
/// `code` member (`g5.organization.platform-operator.bootstrap`, G5-Lean#911)
/// has the tenant cascade's README. Returns `(org, base, code repo)`.
async fn seeded_operator_base(app: &TestApp, with_code: bool) -> (Uuid, Uuid, Option<Uuid>) {
    let org_id = operator_org(app).await;
    let service_user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, oauth_provider, oauth_id)
         VALUES ($1, 'G5 Worker Service', 'service', 'g5-worker') RETURNING id",
    )
    .bind(format!("worker-{}@g5.internal", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org_id)
        .bind(service_user)
        .execute(&app.db)
        .await
        .unwrap();
    let docs: Uuid = sqlx::query_scalar(
        "INSERT INTO repos (org_id, name, slug, git_url, default_branch)
         VALUES ($1, 'docs', 'docs', 'file:///var/lib/g5/repos/operator/docs.git', 'main')
         RETURNING id",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let base: Uuid = sqlx::query_scalar(
        "INSERT INTO workspaces (org_id, name, is_base, docs_repo_id, created_by)
         VALUES ($1, 'Base', true, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(docs)
    .bind(service_user)
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
         VALUES ($1, $2, $3, 'docs', 'docs')",
    )
    .bind(org_id)
    .bind(base)
    .bind(docs)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_settings (workspace_id, org_id) VALUES ($1, $2)")
        .bind(base)
        .bind(org_id)
        .execute(&app.db)
        .await
        .unwrap();
    seed_first_commit(app, docs, &[]).await;
    let code = if with_code {
        let code: Uuid = sqlx::query_scalar(
            "INSERT INTO repos (org_id, name, slug, git_url, default_branch)
             VALUES ($1, 'code', 'code', 'g5-local://workspaces/' || $2::text || '.git', 'main')
             RETURNING id",
        )
        .bind(org_id)
        .bind(base)
        .fetch_one(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
             VALUES ($1, $2, $3, 'code', 'code')",
        )
        .bind(org_id)
        .bind(base)
        .bind(code)
        .execute(&app.db)
        .await
        .unwrap();
        seed_first_commit(
            app,
            code,
            &[(
                "README.md",
                "# code\n\nThis is a G5-managed local project repository.\n",
            )],
        )
        .await;
        Some(code)
    } else {
        None
    };
    sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
        .bind(org_id)
        .bind(helpers::TEST_WORKER_IMAGE_REF)
        .execute(&app.db)
        .await
        .unwrap();
    (org_id, base, code)
}

/// Fork `base` as `operator` through the ordinary clone route.
async fn fork_base(app: &TestApp, operator: &TestUser, base: Uuid, name: &str) -> Uuid {
    let (status, body) = body(
        app.authenticated_client(operator)
            .post_json(
                &format!("/api/v1/workspaces/{base}/clone"),
                &json!({ "name": name, "idempotencyKey": format!("fork-{name}") }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    Uuid::parse_str(body["data"]["workspace"]["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn a_child_of_the_seeded_operator_base_starts_on_the_org_default_image() {
    let app = TestApp::spawn().await;
    let (org_id, base, code) = seeded_operator_base(&app, true).await;
    let code = code.unwrap();
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "admin").await;

    let child = fork_base(&app, &operator, base, "operator-child").await;
    let members: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, root_path FROM workspace_repos WHERE workspace_id = $1 ORDER BY role",
    )
    .bind(child)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        members,
        [
            ("code".to_string(), "code".to_string()),
            ("docs".to_string(), "docs".to_string())
        ],
        "the child has both seeded members"
    );
    let path = app.repo_path(code).await;
    let head = git2::Repository::open(path)
        .unwrap()
        .refname_to_id("refs/heads/operator-child")
        .expect("the child's code branch exists")
        .to_string();

    // The runtime peer installs its own running incarnation for the child.
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(child)
        .execute(&app.db)
        .await
        .unwrap();
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(
        &app,
        &helpers::TestWorkspace {
            id: child,
            org_id,
            repo_id: code,
            head_commit: head,
            name: "operator-child".to_string(),
        },
        operator.id,
    )
    .await;
    let client = app.authenticated_client(&operator);
    for action in ["stop", "resume"] {
        let (status, body) = body(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{child}/{action}"),
                    &json!({ "operationId": Uuid::new_v4() }),
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{action}: {body}");
    }
    let starts = runtime.received_host_command_params("vm.start").await;
    let spec = &starts.last().expect("a vm.start was sent")["spec"];
    assert_eq!(
        spec["image"]["ref"],
        json!(helpers::TEST_WORKER_IMAGE_REF),
        "a code member without .g5/workspace.yaml starts on the org default: {spec}"
    );
    let mut roots = spec["repos"]
        .as_array()
        .expect("members")
        .iter()
        .map(|repo| repo["root_path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    roots.sort();
    assert_eq!(roots, ["code", "docs"], "{spec}");
    let source: Value =
        sqlx::query_scalar("SELECT image_source FROM workspace_vms WHERE workspace_id = $1")
            .bind(child)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(source, json!({ "kind": "org_default" }));
    app.cleanup().await;
}

#[tokio::test]
async fn a_child_of_a_docs_only_operator_base_cannot_start() {
    // Ordinary Start keeps requiring a code member; the seed gives Base one
    // instead (G5-Lean#911).
    let app = TestApp::spawn().await;
    let (_, base, _) = seeded_operator_base(&app, false).await;
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "admin").await;
    let child = fork_base(&app, &operator, base, "docs-only-child").await;

    let (status, body) = body(
        app.authenticated_client(&operator)
            .post_json(
                &format!("/api/v1/workspaces/{child}/resume"),
                &json!({ "operationId": Uuid::new_v4() }),
            )
            .await,
    )
    .await;
    assert!(status.is_client_error(), "{status}: {body}");
    assert_eq!(body["error"]["details"]["code"], "IMAGE_INVALID", "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn onboarding_refuses_retired_owner_field_without_creating_an_org() {
    let app = TestApp::spawn().await;
    let operator = TestUser::create(&app).await;
    make_operator(&app, &operator, "member").await;
    let client = app.authenticated_client(&operator);
    let response = client
        .post_json(
            "/api/v1/platform/orgs",
            &json!({
                "name": "Conflicting Admin", "slug": "conflicting-admin",
                "adminEmail": "admin@example.test", "ownerEmail": "other@example.test"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orgs WHERE slug='conflicting-admin'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    app.cleanup().await;
}
