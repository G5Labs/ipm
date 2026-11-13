//! Declarative test data factories for integration tests.
//!
//! Provides `TestUser`, `TestOrg`, `TestWorkspace`, and `TestTask` structs
//! with `create()` methods that insert test data directly into Postgres
//! and mint JWTs for authentication.
//!
//! All fixture methods are `async` and take `&TestApp` as their first
//! parameter (for database access). Fixtures insert rows directly via sqlx
//! — they do NOT go through the HTTP API.

use g5_api_core::services::git::repo_store::TenantRepoStorage;
use g5_auth::Scope;
use g5_contracts::types::AuthorityTarget;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

use super::{TestApp, TEST_PRIVATE_KEY_PEM, TEST_PUBLIC_KEY_PEM};

// ---------------------------------------------------------------------------
// TestUser — User fixture factory
// ---------------------------------------------------------------------------

/// A test user fixture with a pre-minted JWT for authentication.
pub struct TestUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub jwt: String,
}

impl TestUser {
    /// Creates a new test user with full-access scopes.
    ///
    /// 1. Generates a random UUID for the user ID.
    /// 2. Generates a unique email: `test-{uuid}@g5.test`.
    /// 3. Sets `display_name` to `"Test User"`.
    /// 4. Inserts directly into the `users` table.
    /// 5. Mints a JWT with full access scopes and a placeholder org ID.
    pub async fn create(app: &TestApp) -> Self {
        let id = Uuid::new_v4();
        let email = format!("test-{}@g5.test", Uuid::new_v4());
        let display_name = "Test User".to_string();
        let oauth_id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO users (id, email, display_name, oauth_provider, oauth_id)
            VALUES ($1, $2, $3, 'github', $4)
            "#,
        )
        .bind(id)
        .bind(&email)
        .bind(&display_name)
        .bind(&oauth_id)
        .execute(&app.db)
        .await
        .expect("Failed to insert test user");

        // Mint JWT with full access scopes
        let minter = super::test_jwt_minter();
        let scopes = full_access_scopes();
        let jwt = minter
            .mint(id, Uuid::nil(), scopes, 3600)
            .expect("Failed to mint test JWT");

        Self {
            id,
            email,
            display_name,
            jwt,
        }
    }

    /// Creates a test user with specific scopes and org membership.
    ///
    /// Same as `create()` but:
    /// - Creates an `org_memberships` row linking the user to the given org.
    /// - Mints a JWT with only the specified scopes.
    /// - Uses the provided `org_id` in the JWT claims.
    pub async fn create_with_scopes(app: &TestApp, org_id: Uuid, scopes: &[Scope]) -> Self {
        let id = Uuid::new_v4();
        let email = format!("test-{}@g5.test", Uuid::new_v4());
        let display_name = "Test User".to_string();
        let oauth_id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO users (id, email, display_name, oauth_provider, oauth_id)
            VALUES ($1, $2, $3, 'github', $4)
            "#,
        )
        .bind(id)
        .bind(&email)
        .bind(&display_name)
        .bind(&oauth_id)
        .execute(&app.db)
        .await
        .expect("Failed to insert test user");

        // Create org membership
        sqlx::query(
            r#"
            INSERT INTO org_memberships (org_id, user_id, role)
            VALUES ($1, $2, 'member')
            "#,
        )
        .bind(org_id)
        .bind(id)
        .execute(&app.db)
        .await
        .expect("Failed to insert org membership");

        let minter = super::test_jwt_minter();
        let jwt = minter
            .mint(id, org_id, scopes.to_vec(), 3600)
            .expect("Failed to mint test JWT");

        Self {
            id,
            email,
            display_name,
            jwt,
        }
    }
    /// Adds an active Admin, including a successor for revocation fixtures.
    pub async fn create_admin(app: &TestApp, org_id: Uuid) -> Self {
        let user = Self::create_with_scopes(app, org_id, &full_access_scopes()).await;
        sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(user.id)
            .execute(&app.db)
            .await
            .expect("Failed to promote test administrator");
        user
    }

    /// Grant current database authority without changing this user's token.
    pub async fn grant_org_scope(&self, app: &TestApp, org_id: Uuid, scope: Scope) {
        sqlx::query("INSERT INTO oauth_scopes (scope, description, category) VALUES ($1, $2, 'test') ON CONFLICT (scope) DO NOTHING")
            .bind(scope.as_str())
            .bind(format!("Test scope for {scope}"))
            .execute(&app.db)
            .await
            .expect("Failed to seed scope catalog");
        sqlx::query("INSERT INTO user_scope_grants (org_id, user_id, scope, granted_by, expires_at) VALUES ($1, $2, $3, $2, now() + interval '1 hour')")
            .bind(org_id)
            .bind(self.id)
            .bind(scope.as_str())
            .execute(&app.db)
            .await
            .expect("Failed to grant current org scope");
    }
}

// ---------------------------------------------------------------------------
// TestOrg — Organization fixture factory
// ---------------------------------------------------------------------------

/// A test organization fixture.
pub struct TestOrg {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

impl TestOrg {
    /// Creates a new test organization.
    ///
    /// 1. Generates a random UUID.
    /// 2. Generates a unique name and slug.
    /// 3. Atomically inserts the organization and its initial active Admin.
    pub async fn create(app: &TestApp) -> Self {
        let mut admin = TestUser::create(app).await;
        sqlx::query("UPDATE users SET email=$2 WHERE id=$1")
            .bind(admin.id)
            .bind(format!("fixture-admin-{}@g5.test", admin.id))
            .execute(&app.db)
            .await
            .unwrap();
        Self::create_with_admin(app, &mut admin).await
    }

    pub async fn create_with_admin(app: &TestApp, user: &mut TestUser) -> Self {
        Self::create_for(app, user, "admin").await
    }

    async fn create_for(app: &TestApp, user: &mut TestUser, role: &str) -> Self {
        let id = Uuid::new_v4();
        let name = format!("test-org-{}", nanoid::nanoid!(8));
        let slug = name.clone();
        let mut tx = app.db.begin().await.unwrap();
        sqlx::query("INSERT INTO orgs (id, name, slug) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(&name)
            .bind(&slug)
            .execute(&mut *tx)
            .await
            .expect("Failed to insert test org");
        sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(user.id)
            .bind(role)
            .execute(&mut *tx)
            .await
            .expect("Failed to insert initial administrator");
        tx.commit().await.unwrap();
        seed_org_llm_credential(app, id).await;
        user.jwt = super::test_jwt_minter()
            .mint(user.id, id, full_access_scopes(), 3600)
            .expect("Failed to re-mint JWT with org_id");
        Self { id, name, slug }
    }
}

/// Seed a real, ConfigService-compatible `ANTHROPIC_API_KEY` secret on
/// `org_configs` for the given org using the test app's SCM token manager so
/// the resulting `secrets_enc` blob decrypts back to a non-empty value (the
/// shape the shared agent-launch resolver requires for the default direct
/// Anthropic route). The fixture also supplies the safe provider permission
/// modes that production requires callers to configure explicitly. Idempotent:
/// a second call replaces the existing secret map while preserving settings.
/// Seed a Claude org LLM credential with a caller-supplied key value.
pub async fn seed_org_llm_credential_value(app: &TestApp, org_id: Uuid, api_key: &str) {
    seed_org_llm_credentials(app, org_id, &[("ANTHROPIC_API_KEY", api_key)]).await;
}

pub async fn seed_org_llm_credential(app: &TestApp, org_id: Uuid) {
    seed_org_llm_credentials(
        app,
        org_id,
        &[("ANTHROPIC_API_KEY", "test-anthropic-api-key")],
    )
    .await;
}

/// Seed BOTH a Claude and a Codex org LLM credential, so provider launch
/// resolution passes for either provider and provider-specific downstream
/// contracts (e.g. the preview chat-attach claude_code-only 409) are reachable.
pub async fn seed_org_llm_credentials_all_providers(app: &TestApp, org_id: Uuid) {
    seed_org_llm_credentials(
        app,
        org_id,
        &[
            ("ANTHROPIC_API_KEY", "test-anthropic-api-key"),
            ("OPENAI_API_KEY", "test-openai-api-key"),
        ],
    )
    .await;
}

/// Shared body: encrypt and upsert the given secret map onto the org config.
async fn seed_org_llm_credentials(app: &TestApp, org_id: Uuid, entries: &[(&str, &str)]) {
    let manager = app
        .state
        .scm_token_manager
        .as_ref()
        .expect("test app should configure G5_SCM_ENCRYPTION_KEY");
    let mut secrets: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (key, value) in entries {
        secrets.insert((*key).to_string(), (*value).to_string());
    }
    let payload = serde_json::to_string(&secrets).expect("serialize secret map");
    let encrypted = manager
        .encrypt_token(&payload)
        .expect("encrypt secret map for test fixture");
    let secret_keys: Vec<String> = secrets.keys().cloned().collect();

    sqlx::query(
        "INSERT INTO org_configs (org_id, settings_json, secrets_enc, secret_keys) \
         VALUES (\
           $1, \
           '{\"claude_permission_mode\":\"default\",\"codex_permission_mode\":\"never/workspace-write\"}'::jsonb, \
           $2, \
           $3\
         ) \
         ON CONFLICT (org_id) DO UPDATE SET \
           secrets_enc = EXCLUDED.secrets_enc, \
           secret_keys = EXCLUDED.secret_keys",
    )
    .bind(org_id)
    .bind(&encrypted)
    .bind(&secret_keys)
    .execute(&app.db)
    .await
    .expect("should seed org LLM credentials for agentic jobs");
}

// ---------------------------------------------------------------------------
// TestWorkspace — Workspace fixture factory
// ---------------------------------------------------------------------------

/// A test workspace fixture.
pub struct TestWorkspace {
    pub id: Uuid,
    pub org_id: Uuid,
    pub repo_id: Uuid,
    pub head_commit: String,
    pub name: String,
}

impl TestWorkspace {
    /// Creates a new test workspace in the given org.
    ///
    /// 1. Generates a random UUID.
    /// 2. Generates a unique name.
    /// 3. Inserts a non-base row into `workspaces`.
    /// 4. Inserts a default `workspace_settings` row.
    pub async fn create(app: &TestApp, org: &TestOrg) -> Self {
        Self::create_kind(app, org, false, None, None).await
    }

    pub async fn create_named(app: &TestApp, org: &TestOrg, name: &str) -> Self {
        Self::create_kind(app, org, false, Some(name), None).await
    }

    pub async fn create_base(app: &TestApp, org: &TestOrg) -> Self {
        Self::create_kind(app, org, true, None, None).await
    }

    /// A private identity from birth, with its own repository (never a
    /// post-creation kind change or generic invitation fixture).
    pub async fn create_personal(app: &TestApp, org: &TestOrg, owner: Uuid) -> Self {
        Self::create_kind(app, org, false, None, Some(owner)).await
    }

    async fn create_kind(
        app: &TestApp,
        org: &TestOrg,
        is_base: bool,
        name: Option<&str>,
        personal_user_id: Option<Uuid>,
    ) -> Self {
        let id = Uuid::new_v4();
        let suffix = nanoid::nanoid!(8);
        let name = if is_base {
            "main".to_owned()
        } else {
            name.map(str::to_owned)
                .unwrap_or_else(|| format!("test-workspace-{}", suffix))
        };

        // Final Lean workspaces always have an owner. Prefer an existing user,
        // then an existing member. Most route tests create their acting user
        // before this fixture, so this makes that user the legitimate current
        // workspace owner instead of preserving the retired any-editor write
        // model. Lower-level storage tests with no member get a real fixture
        // owner rather than bypassing the production constraint.
        let owner_user_id = if let Some(owner) = personal_user_id {
            owner
        } else if let Some(owner_user_id) = sqlx::query_scalar::<_, Uuid>(
            "SELECT m.user_id FROM org_memberships m JOIN users u ON u.id=m.user_id \
             WHERE m.org_id = $1 \
             ORDER BY (u.email LIKE 'fixture-admin-%@g5.test'), m.created_at, m.user_id LIMIT 1",
        )
        .bind(org.id)
        .fetch_optional(&app.db)
        .await
        .expect("Failed to resolve test workspace owner")
        {
            owner_user_id
        } else {
            let owner_user_id = Uuid::new_v4();
            let owner_email = format!("workspace-owner-{owner_user_id}@example.test");
            sqlx::query(
                "INSERT INTO users (id, email, display_name) \
                 VALUES ($1, $2, 'Workspace Fixture Owner')",
            )
            .bind(owner_user_id)
            .bind(owner_email)
            .execute(&app.db)
            .await
            .expect("Failed to insert test workspace owner");
            sqlx::query(
                "INSERT INTO org_memberships (org_id, user_id, role) \
                 VALUES ($1, $2, 'admin')",
            )
            .bind(org.id)
            .bind(owner_user_id)
            .execute(&app.db)
            .await
            .expect("Failed to insert test workspace owner membership");
            owner_user_id
        };

        sqlx::query(
            r#"
            INSERT INTO workspaces (
                id, org_id, name, is_base,
                owner_user_id, created_by, personal_user_id
            )
            VALUES ($1, $2, $3, $5, $4, $4, $6)
            "#,
        )
        .bind(id)
        .bind(org.id)
        .bind(&name)
        .bind(owner_user_id)
        .bind(is_base)
        .bind(personal_user_id)
        .execute(&app.db)
        .await
        .expect("Failed to insert test workspace");

        // Insert default workspace settings
        sqlx::query(
            r#"
            INSERT INTO workspace_settings (org_id, workspace_id)
            VALUES ($1, $2)
            "#,
        )
        .bind(org.id)
        .bind(id)
        .execute(&app.db)
        .await
        .expect("Failed to insert workspace settings");

        // Create a repo and canonical member so the fixture has a complete
        // repository association.
        let repo_id = Uuid::new_v4();
        let slug = format!("test-repo-{}", suffix);
        let repo_path = app
            .repo_storage
            .init_repo(&TenantRepoStorage::test_key(org.id, repo_id))
            .await
            .expect("Failed to initialize test repo storage");
        let head_commit = seed_materialized_repo(&repo_path, "README.md", "test workspace\n");
        run_git(
            &repo_path,
            &["update-ref", &format!("refs/heads/{name}"), &head_commit],
        );
        let git_url = repo_path.display().to_string();

        sqlx::query(
            r#"
            INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
            VALUES ($1, $2, $3, $4, $5, 'main')
            "#,
        )
        .bind(repo_id)
        .bind(org.id)
        .bind(&slug)
        .bind(&slug)
        .bind(git_url)
        .execute(&app.db)
        .await
        .expect("Failed to insert test repo");

        sqlx::query(
            "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
             VALUES ($1, $2, $3, '', 'code')",
        )
        .bind(org.id)
        .bind(id)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .expect("insert canonical workspace repository");
        run_git(&repo_path, &["checkout", &name]);

        Self {
            id,
            org_id: org.id,
            repo_id,
            head_commit,
            name,
        }
    }

    /// Give a lifecycle fixture a real parent with the same canonical members.
    pub async fn attach_parent(&self, app: &TestApp) -> Uuid {
        self.attach_parent_with(app, false).await
    }

    /// [`Self::attach_parent`], the parent being a Base workspace when `is_base`.
    pub async fn attach_parent_with(&self, app: &TestApp, is_base: bool) -> Uuid {
        let parent_id = Uuid::new_v4();
        let parent_name = format!("parent-{parent_id}");
        sqlx::query("INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by,docs_repo_id) SELECT $2,org_id,$3,$4,owner_user_id,created_by,docs_repo_id FROM workspaces WHERE id=$1")
            .bind(self.id).bind(parent_id).bind(&parent_name).bind(is_base).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
            .bind(self.org_id)
            .bind(parent_id)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1")
            .bind(self.id).bind(parent_id).execute(&app.db).await.unwrap();
        let repo_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT repo_id FROM workspace_repos WHERE workspace_id=$1")
                .bind(self.id)
                .fetch_all(&app.db)
                .await
                .unwrap();
        for repo_id in repo_ids {
            let path = app.repo_path(repo_id).await;
            let head = git2::Repository::open(path)
                .unwrap()
                .refname_to_id(&format!("refs/heads/{}", self.name))
                .unwrap()
                .to_string();
            super::seed_branch_ref(app, repo_id, &parent_name, &head).await;
        }
        sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
            .bind(self.id)
            .bind(parent_id)
            .execute(&app.db)
            .await
            .unwrap();
        parent_id
    }

    pub fn git_head(&self, app: &TestApp) -> String {
        let path = app.repo_storage.test_path(self.org_id, self.repo_id);
        git2::Repository::open(path)
            .unwrap()
            .refname_to_id(&format!("refs/heads/{}", self.name))
            .unwrap()
            .to_string()
    }

    /// Attach a second, docs-role repository to the workspace and make it the
    /// workspace's `docs_repo_id`, so loaders that read "the docs repository"
    /// (workbench configuration, Project Docs) have a distinct target from the
    /// fixture's code repository. Returns the new repository id.
    pub async fn add_docs_repo(&self, app: &TestApp) -> Uuid {
        let repo_id = Uuid::new_v4();
        let slug = format!("test-docs-repo-{}", nanoid::nanoid!(8));
        let repo_path = app
            .repo_storage
            .init_repo(&TenantRepoStorage::test_key(self.org_id, repo_id))
            .await
            .expect("Failed to initialize test docs repo storage");
        let head_commit = seed_materialized_repo(&repo_path, "README.md", "docs repository\n");
        run_git(
            &repo_path,
            &[
                "update-ref",
                &format!("refs/heads/{}", self.name),
                &head_commit,
            ],
        );
        run_git(&repo_path, &["checkout", &self.name]);
        sqlx::query(
            "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch) \
             VALUES ($1, $2, $3, $4, $5, 'main')",
        )
        .bind(repo_id)
        .bind(self.org_id)
        .bind(&slug)
        .bind(&slug)
        .bind(repo_path.display().to_string())
        .execute(&app.db)
        .await
        .expect("Failed to insert test docs repo");
        sqlx::query(
            "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role) \
             VALUES ($1, $2, $3, 'docs', 'docs')",
        )
        .bind(self.org_id)
        .bind(self.id)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .expect("insert docs workspace repository");
        sqlx::query("UPDATE workspaces SET docs_repo_id = $2 WHERE id = $1")
            .bind(self.id)
            .bind(repo_id)
            .execute(&app.db)
            .await
            .expect("set workspace docs repository");
        repo_id
    }

    /// Commit `contents` at `file_path` on one of the workspace's repositories
    /// (`repo_id`), advancing that branch head. Returns the new commit sha.
    pub fn commit_file_in_repo(
        &self,
        app: &TestApp,
        repo_id: Uuid,
        file_path: &str,
        contents: &str,
    ) -> String {
        let repo_path = app.repo_storage.test_path(self.org_id, repo_id);
        if let Some(parent) = Path::new(file_path).parent() {
            std::fs::create_dir_all(repo_path.join(parent)).expect("create parent dirs");
        }
        std::fs::write(repo_path.join(file_path), contents).expect("write committed file");
        run_git(&repo_path, &["add", file_path]);
        run_git(
            &repo_path,
            &["commit", "-m", &format!("commit {file_path}")],
        );
        git_stdout(&repo_path, &["rev-parse", "HEAD"])
    }

    /// Commit `contents` at `file_path` on the workspace's code repository
    /// and advance its Git ref to the new head, so
    /// committed-repo readers (and the golden trigger's declaration seed)
    /// see the file at the branch head. Returns the new commit sha.
    pub async fn commit_file(&mut self, app: &TestApp, file_path: &str, contents: &str) -> String {
        let repo_path = app.repo_path(self.repo_id).await;
        if let Some(parent) = Path::new(file_path).parent() {
            std::fs::create_dir_all(repo_path.join(parent)).expect("create parent dirs");
        }
        std::fs::write(repo_path.join(file_path), contents).expect("write committed file");
        run_git(&repo_path, &["add", file_path]);
        run_git(
            &repo_path,
            &["commit", "-m", &format!("commit {file_path}")],
        );
        let sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
        self.head_commit = sha.clone();
        sha
    }
}

// ---------------------------------------------------------------------------
// Persistent task/draft scope fixtures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TestTask {
    pub id: Uuid,
    pub org_id: Uuid,
    pub status: String,
}

impl TestTask {
    pub async fn create(app: &TestApp, workspace: &TestWorkspace) -> Self {
        let id = Uuid::new_v4();
        let mut db = app.scoped_db_for_org(workspace.org_id).await;
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, org_id, workspace_id, name, kind, status,
                created_by, created_at, updated_at
            )
            SELECT $1, $2, $3, $4, 'command', 'running', owner_user_id, now(), now()
            FROM workspaces
            WHERE id = $3
            "#,
        )
        .bind(id)
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(format!("test-task-{}", nanoid::nanoid!(8)))
        .execute(db.conn())
        .await
        .expect("Failed to insert test task");
        db.commit().await.expect("Failed to commit test task");

        Self {
            id,
            org_id: workspace.org_id,
            status: "running".to_string(),
        }
    }

    /// Creates a minimal current-schema running task for relational API tests.
    pub async fn create_unbound(app: &TestApp, workspace: &TestWorkspace) -> Self {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, org_id, workspace_id, name, kind, status,
                created_by
            )
            SELECT $1, $2, $3, $4, 'command', 'running', owner_user_id
            FROM workspaces
            WHERE id = $3
            "#,
        )
        .bind(id)
        .bind(workspace.org_id)
        .bind(workspace.id)
        .bind(format!("test-task-{}", nanoid::nanoid!(8)))
        .execute(&app.db)
        .await
        .expect("Failed to insert unbound test task");
        Self {
            id,
            org_id: workspace.org_id,
            status: "running".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn seed_materialized_repo(repo_path: &Path, file_path: &str, contents: &str) -> String {
    run_git(repo_path, &["init"]);
    run_git(repo_path, &["config", "user.email", "g5@test.local"]);
    run_git(repo_path, &["config", "user.name", "G5 Test"]);
    run_git(repo_path, &["checkout", "-B", "main"]);
    std::fs::write(repo_path.join(file_path), contents).expect("Failed to seed test repo file");
    run_git(repo_path, &["add", file_path]);
    run_git(repo_path, &["commit", "-m", "seed test workspace repo"]);
    git_stdout(repo_path, &["rev-parse", "HEAD"])
}

fn run_git(repo_path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout should be utf-8")
        .trim()
        .to_string()
}

/// Returns the full set of OAuth scopes for test convenience.
///
/// These scopes grant full access to all resources, matching the scopes
/// listed in the helpers spec.
fn full_access_scopes() -> Vec<Scope> {
    vec![
        Scope::WorkspacesRead,
        Scope::WorkspacesWrite,
        Scope::WorkspacesAdmin,
        Scope::TasksRead,
        Scope::TasksExecute,
        Scope::SessionsRead,
        Scope::SessionsWrite,
        Scope::BillingAdmin,
        Scope::OrgRead,
        Scope::OrgAdmin,
        Scope::UsersRead,
        Scope::WorkersRead,
        Scope::WorkersAdmin,
        Scope::SettingsRead,
        Scope::SettingsWrite,
    ]
}
