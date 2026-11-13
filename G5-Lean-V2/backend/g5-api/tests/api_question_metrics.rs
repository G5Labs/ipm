//! Open-question metrics and Home count referenced questions once
//! (`g5.documents.question.metrics`), over the real API, Postgres and Git.

use crate::helpers;

use g5_contracts::types::question_set::{
    question_set_path, render_file, Answer, Provenance, Question, QuestionOption, QuestionSetFile,
    QuestionStatus, SetMeta,
};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

fn run_git(repo_path: &std::path::Path, args: &[&str]) {
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

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

/// Commit `files` on a fresh docs repository attached to `workspace`.
async fn attach_docs(
    app: &TestApp,
    org: &TestOrg,
    workspace: &TestWorkspace,
    files: &[(String, String)],
) -> Uuid {
    let repo_id = Uuid::new_v4();
    let path = app.init_repo(org.id, repo_id).await;
    run_git(&path, &["init"]);
    run_git(&path, &["checkout", "-B", "main"]);
    std::fs::write(path.join("README.md"), "docs\n").unwrap();
    for (file, content) in files {
        let target = path.join(file);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, content).unwrap();
    }
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "seed docs"]);
    let head = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&path)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let name = format!("docs-{repo_id}");
    sqlx::query(
        "INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider) \
         VALUES ($1, $2, $3, $3, $4, 'main', NULL)",
    )
    .bind(repo_id)
    .bind(org.id)
    .bind(&name)
    .bind(path.display().to_string())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role) \
         VALUES ($1,$2,$3,'.g5/docs','docs')",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE workspaces SET docs_repo_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(repo_id)
        .execute(&app.db)
        .await
        .unwrap();
    helpers::seed_workspace_authority_head(app, workspace.id, repo_id, &head).await;
    repo_id
}

fn question(id: &str, prompt: &str) -> Question {
    let mut question = Question::new_open(id, prompt);
    question.options = vec![QuestionOption {
        id: "keep".to_string(),
        label: "Keep".to_string(),
        description: String::new(),
        tradeoffs: None,
        risks: None,
        recommended: None,
        extra: Default::default(),
    }];
    question
}

/// One open, one proposed and one answered entry.
fn set_file(set_id: &str) -> String {
    let mut proposed = question("proposed-1", "Proposed by the agent?");
    proposed.revision = 2;
    proposed.status = QuestionStatus::Proposed;
    proposed.answer = Answer {
        selected_option_ids: vec!["keep".to_string()],
        answered_by: Some(Provenance::agent("u")),
        answered_at_revision: Some(2),
        version: 1,
        ..Answer::default()
    };
    let mut answered = question("answered-1", "Already decided?");
    answered.revision = 2;
    answered.status = QuestionStatus::Answered;
    answered.answer = Answer {
        selected_option_ids: vec!["keep".to_string()],
        answered_by: Some(Provenance::user("u")),
        answered_at_revision: Some(2),
        version: 1,
        ..Answer::default()
    };
    let mut file = QuestionSetFile::new(SetMeta {
        id: set_id.to_string(),
        title: format!("Set {set_id}"),
        created_at: "2026-10-09T00:00:00Z".to_string(),
        created_by: None,
        extra: Default::default(),
    });
    file.questions = vec![question("open-1", "Still open?"), proposed, answered];
    render_file(&file)
}

fn reference_doc(title: &str, path: &str, ids: &[&str], embedded_open: bool) -> String {
    let mut doc = format!("# {title}\n\n");
    for (index, id) in ids.iter().enumerate() {
        doc.push_str(&format!(
            "[Q{}. Referenced {id}]\n\n```g5-question-ref\n{{\"path\": \"{path}\", \"id\": \"{id}\"}}\n```\n\n",
            index + 1
        ));
    }
    if embedded_open {
        doc.push_str(
            "[Q9. Embedded]\n\n```g5-question\nschema: 1\nid: embedded-1\nstatus: open\nprompt: Embedded?\noptions:\n  - label: Keep\n    description: Keep it.\nanswer:\n  selected: []\n  text: \"\"\n  rationale: \"\"\n  answered_by: null\n  answered_at: null\n```\n",
        );
    }
    doc
}

#[tokio::test]
async fn metrics_count_each_set_entry_once_and_flag_invalid_sets() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let ws = workspace.id.to_string();
    let own_set = question_set_path(&ws, "plan");
    // An inherited set authored by a parent workspace and referenced here.
    let parent_set = question_set_path("parent-ws", "old");
    // A sibling's set nobody here references is not counted.
    let sibling_set = question_set_path("sibling-ws", "theirs");
    let broken_set = question_set_path(&ws, "broken");
    attach_docs(
        &app,
        &org,
        &workspace,
        &[
            (own_set.clone(), set_file("plan")),
            (parent_set.clone(), set_file("old")),
            (sibling_set, set_file("theirs")),
            (broken_set, "{ broken".to_string()),
            (
                format!("docs/{ws}/a-plan.md"),
                reference_doc("A plan", &own_set, &["open-1", "proposed-1"], true),
            ),
            (
                format!("docs/{ws}/b-plan.md"),
                reference_doc("B plan", &own_set, &["open-1"], false),
            ),
            (
                format!("docs/{ws}/c-inherited.md"),
                reference_doc("Inherited", &parent_set, &["open-1"], false),
            ),
        ],
    )
    .await;
    let client = app.authenticated_client(&owner);

    let response = client
        .get(&format!(
            "/api/v1/orgs/{}/workspaces/open-question-metrics?workspaceIds={}",
            org.id, workspace.id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let metric = &data(&body)[0];
    // Embedded: one open fence. Own set: open + proposed (answered excluded),
    // counted once although two documents reference it. Inherited set: open
    // + proposed. Sibling set: not counted. Broken set: flagged.
    assert_eq!(metric["documentOpenQuestions"], 1 + 2 + 2, "{metric}");
    assert_eq!(metric["openQuestionsTotal"], 5);
    assert_eq!(metric["questionSetsIncomplete"], true);
    let sets = metric["questionSetSources"].as_array().unwrap();
    let by_path = |path: &str| {
        sets.iter()
            .find(|source| source["path"] == path)
            .unwrap_or_else(|| panic!("{path} in {sets:?}"))
    };
    let own = by_path(&own_set);
    assert_eq!(own["openQuestionCount"], 2);
    assert_eq!(own["referencedBy"].as_array().unwrap().len(), 2);
    assert_eq!(own["openQuestions"][1]["status"], "proposed");
    assert_eq!(
        by_path(&parent_set)["referencedBy"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(by_path(&question_set_path(&ws, "broken"))["invalid"], true);
    assert!(sets
        .iter()
        .all(|source| !source["path"].as_str().unwrap().contains("sibling-ws")));
    // The embedded family is unchanged by the references.
    let docs = metric["projectDocSources"].as_array().unwrap();
    assert_eq!(docs.len(), 1, "{docs:?}");
    assert_eq!(docs[0]["openQuestionCount"], 1);

    // Home lists each set with open entries once, linked to a referencing
    // Project Doc.
    let response = client
        .get(&format!("/api/v1/orgs/{}/home/attention", org.id))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = data(&body)["items"].as_array().unwrap().clone();
    let set_items = items
        .iter()
        .filter(|item| {
            item["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("open_question_set:"))
        })
        .collect::<Vec<_>>();
    assert_eq!(set_items.len(), 2, "{items:?}");
    let own_item = set_items
        .iter()
        .find(|item| item["id"].as_str().unwrap().ends_with(&own_set))
        .unwrap();
    assert_eq!(own_item["openQuestionCount"], 2);
    assert!(own_own_doc_ids(own).contains(&own_item["sourceId"].as_str().unwrap().to_string()));

    app.cleanup().await;
}

fn own_own_doc_ids(source: &Value) -> Vec<String> {
    source["referencedBy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}
