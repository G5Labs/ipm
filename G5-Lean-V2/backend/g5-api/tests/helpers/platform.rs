//! Platform-operator fixtures (`g5.organization.platform-operator`).
//!
//! Organizations are created only by platform operators
//! (`g5.organization.creation`). Tests that need an organization born from the
//! full creation cascade create it through `POST /api/v1/platform/orgs` as a
//! fixture operator, with the test's user as owner.

use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{TestApp, TestUser};

/// The operator organization, inserted the way the platform seed identifies
/// it (`kind = 'operator'`, slug `operator`) when the test database has none.
pub async fn operator_org(app: &TestApp) -> Uuid {
    sqlx::query(
        "INSERT INTO orgs (name, slug, kind)
         VALUES ('Platform operator', 'operator', 'operator')
         ON CONFLICT DO NOTHING",
    )
    .execute(&app.db)
    .await
    .expect("insert operator org");
    sqlx::query_scalar("SELECT id FROM orgs WHERE kind = 'operator'")
        .fetch_one(&app.db)
        .await
        .expect("operator org exists")
}

/// Give `user` an operator membership with `role`.
pub async fn make_operator(app: &TestApp, user: &TestUser, role: &str) -> Uuid {
    let org_id = operator_org(app).await;
    sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(org_id)
    .bind(user.id)
    .bind(role)
    .execute(&app.db)
    .await
    .expect("insert operator membership");
    org_id
}

/// The operator API's answer, read like a `reqwest::Response`.
pub struct OperatorCreatedOrg {
    status: StatusCode,
    body: Value,
}

impl OperatorCreatedOrg {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub async fn json<T: serde::de::DeserializeOwned>(self) -> serde_json::Result<T> {
        serde_json::from_value(self.body)
    }
}

/// Create an organization owned by `owner` through the operator API, as a
/// fresh fixture operator. `body` carries `name` and optionally `slug`.
///
/// A successful response is returned in the creation-cascade shape the
/// former self-serve route returned (`org`, `baseWorkspace`, `docsRepo`,
/// `codeRepo`) plus the operator API's `owner`, so cascade assertions stay
/// direct. Errors are returned unchanged.
pub async fn create_org_via_operator(
    app: &TestApp,
    owner: &TestUser,
    body: Value,
) -> OperatorCreatedOrg {
    let operator = TestUser::create(app).await;
    make_operator(app, &operator, "member").await;
    create_org_as(app, &operator, owner, body).await
}

/// [`create_org_via_operator`] for an existing fixture `operator`.
pub async fn create_org_as(
    app: &TestApp,
    operator: &TestUser,
    owner: &TestUser,
    body: Value,
) -> OperatorCreatedOrg {
    let mut request = body;
    request["adminEmail"] = json!(owner.email);
    let response = app
        .authenticated_client(operator)
        .post_json("/api/v1/platform/orgs", &request)
        .await;
    let status = response.status();
    let mut value: Value = response.json().await.unwrap_or(Value::Null);
    if status != StatusCode::CREATED {
        return OperatorCreatedOrg {
            status,
            body: value,
        };
    }
    let org_id = Uuid::parse_str(value["data"]["org"]["id"].as_str().expect("org id"))
        .expect("org id is a UUID");
    let (workspace_id, workspace_name): (Uuid, String) =
        sqlx::query_as("SELECT id, name FROM workspaces WHERE org_id = $1 AND is_base")
            .bind(org_id)
            .fetch_one(&app.db)
            .await
            .expect("Base workspace");
    value["data"]["baseWorkspace"] = json!({ "id": workspace_id, "name": workspace_name });
    for (role, key) in [("docs", "docsRepo"), ("code", "codeRepo")] {
        let (id, name, git_url): (Uuid, String, String) = sqlx::query_as(
            "SELECT r.id, r.name, r.git_url FROM workspace_repos m
             JOIN repos r ON r.id = m.repo_id
             WHERE m.workspace_id = $1 AND m.role = $2",
        )
        .bind(workspace_id)
        .bind(role)
        .fetch_one(&app.db)
        .await
        .expect("Base repository");
        value["data"][key] = json!({ "id": id, "name": name, "gitUrl": git_url });
    }
    OperatorCreatedOrg {
        status,
        body: value,
    }
}

/// Admit `email` at its next OAuth sign-in (`g5.user.admission`) with a
/// pending `member` invitation to a fresh organization, returning that
/// organization's id.
pub async fn invite_for_sign_in(app: &TestApp, email: &str) -> Uuid {
    let mut inviter = TestUser::create(app).await;
    let org = super::TestOrg::create_with_admin(app, &mut inviter).await;
    sqlx::query(
        "INSERT INTO org_invitations (org_id, email, role, invited_by, status, expires_at)
         VALUES ($1, $2, 'member', $3, 'pending', now() + interval '7 days')",
    )
    .bind(org.id)
    .bind(email)
    .bind(inviter.id)
    .execute(&app.db)
    .await
    .expect("insert sign-in invitation");
    org.id
}
