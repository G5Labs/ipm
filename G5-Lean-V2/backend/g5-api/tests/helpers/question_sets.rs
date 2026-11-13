//! A workspace with a docs repository whose committed branch a test can
//! advance, for the referenced-question suites (`api_question_sets.rs`,
//! `api_question_votes.rs`).

use g5_contracts::types::question_set::{
    question_set_path, render_file, Question, QuestionOption, QuestionSetFile, SetMeta,
};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

use super::{TestApp, TestOrg, TestUser, TestWorkspace};

pub fn run_git(repo_path: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .env("GIT_AUTHOR_NAME", "G5 Test")
        .env("GIT_AUTHOR_EMAIL", "g5@test.local")
        .env("GIT_COMMITTER_NAME", "G5 Test")
        .env("GIT_COMMITTER_EMAIL", "g5@test.local")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

pub fn git_stdout(repo_path: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
}

pub fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

pub async fn json_of(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

/// A workspace with a docs repository whose committed branch the test can
/// advance.
pub struct Fixture {
    pub app: TestApp,
    pub owner: TestUser,
    pub org: TestOrg,
    pub workspace: TestWorkspace,
    pub docs_repo_id: Uuid,
    pub docs_path: std::path::PathBuf,
}

impl Fixture {
    pub async fn new() -> Self {
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let workspace = TestWorkspace::create(&app, &org).await;
        let docs_repo_id = Uuid::new_v4();
        let docs_path = app
            .repo_storage
            .init_repo(
                &g5_api_core::services::git::repo_store::TenantRepoStorage::test_key(
                    org.id,
                    docs_repo_id,
                ),
            )
            .await
            .expect("initialize docs repository");
        run_git(&docs_path, &["init"]);
        run_git(&docs_path, &["checkout", "-B", "main"]);
        std::fs::write(docs_path.join("README.md"), "docs\n").unwrap();
        run_git(&docs_path, &["add", "README.md"]);
        run_git(&docs_path, &["commit", "-m", "seed docs"]);
        let head = git_stdout(&docs_path, &["rev-parse", "HEAD"]);
        let name = format!("docs-{docs_repo_id}");
        sqlx::query(
            "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
             VALUES ($1, $2, $3, $3, $4, 'main', NULL)",
        )
        .bind(docs_repo_id)
        .bind(org.id)
        .bind(&name)
        .bind(docs_path.display().to_string())
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) \
             VALUES ($1,$2,$3,'.g5/docs','docs')",
        )
        .bind(org.id)
        .bind(workspace.id)
        .bind(docs_repo_id)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query("UPDATE workspaces SET docs_repo_id = $2 WHERE id = $1")
            .bind(workspace.id)
            .bind(docs_repo_id)
            .execute(&app.db)
            .await
            .unwrap();
        super::seed_workspace_authority_head(&app, workspace.id, docs_repo_id, &head).await;
        Self {
            app,
            owner,
            org,
            workspace,
            docs_repo_id,
            docs_path,
        }
    }

    /// Commit `content` at `path` on the docs branch and advance the cut.
    pub async fn commit_docs(&self, path: &str, content: &str) {
        let file = self.docs_path.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
        run_git(&self.docs_path, &["add", path]);
        run_git(
            &self.docs_path,
            &["commit", "-m", &format!("commit {path}")],
        );
        let head = git_stdout(&self.docs_path, &["rev-parse", "HEAD"]);
        super::seed_workspace_authority_head(
            &self.app,
            self.workspace.id,
            self.docs_repo_id,
            &head,
        )
        .await;
    }

    pub fn set_path(&self) -> String {
        question_set_path(&self.workspace.id.to_string(), "plan")
    }

    pub fn questions_url(&self, leaf: &str) -> String {
        format!("/api/v1/workspaces/{}/questions{leaf}", self.workspace.id)
    }
}

/// A set with one open question offering two options.
pub fn plan_set(set_id: &str) -> String {
    let mut question = Question::new_open("pick-0123abcd", "Which storage backend?");
    question.options = ["Set file", "Database"]
        .iter()
        .map(|label| QuestionOption {
            id: label.to_lowercase().replace(' ', "-"),
            label: (*label).to_string(),
            description: String::new(),
            tradeoffs: None,
            risks: None,
            recommended: None,
            extra: Default::default(),
        })
        .collect();
    let mut file = QuestionSetFile::new(SetMeta {
        id: set_id.to_string(),
        title: "Plan".to_string(),
        created_at: "2026-10-09T00:00:00Z".to_string(),
        created_by: None,
        extra: Default::default(),
    });
    file.questions.push(question);
    render_file(&file)
}
