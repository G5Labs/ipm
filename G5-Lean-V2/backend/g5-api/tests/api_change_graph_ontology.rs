//! Diff Graph over current-format ontology through real HTTP, database authority
//! and Git history (`.g5/ontology/features/diff_graph.g5.md`). The workspace
//! runtime is the transport double used by the other live-file suites.
use crate::helpers;
use g5_auth::Scope;
use g5_semantic::ontology_comparison;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

const ROOT: &str = ".g5/ontology/root.g5.md";
const X: &str = ".g5/ontology/features/x.g5.md";
const Y: &str = ".g5/ontology/features/y.g5.md";
const META: &str = ".g5/ontology/concepts/meta.g5.md";
const BROKEN: &str = ".g5/ontology/concepts/broken.g5.md";
const OLD: &str = ".g5/ontology/features/old.g5.md";
const NEW: &str = ".g5/ontology/features/new.g5.md";
const NOTES: &str = ".g5/ontology/notes.md";
const CHAT: &str = ".g5/chat_intent/w/request.md";
const PLAN: &str = ".g5/pr_ontology_diff_design/w/ontology-plan.md";
const LIB: &str = "src/lib.rs";

/// One ontology page: `(id, kind, body, provenance)` statements and a heading.
fn page(heading: &str, statements: &[(&str, &str, &str, &str)]) -> String {
    let mut body = format!("# {heading}\n\n");
    let mut footer = String::from("## Metadata\n\n");
    for (id, kind, text, provenance) in statements {
        body.push_str(&format!(
            "<a id=\"{id}\"></a>\n[{kind} · {id}]\n\n{text}\n\n"
        ));
        footer.push_str(&format!("### {id}\n\n- {provenance}\n\n"));
    }
    format!("{body}{footer}")
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        // Fixture commits must not take the VM's ambient identity.
        .env_remove("GIT_AUTHOR_NAME")
        .env_remove("GIT_AUTHOR_EMAIL")
        .env_remove("GIT_COMMITTER_NAME")
        .env_remove("GIT_COMMITTER_EMAIL")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

async fn repo_path(app: &TestApp, repo: Uuid) -> std::path::PathBuf {
    app.repo_path(repo).await
}

async fn rename(app: &TestApp, repo: Uuid, from: &str, to: &str) -> String {
    let path = repo_path(app, repo).await;
    git(&path, &["mv", from, to]);
    git(&path, &["commit", "-m", "rename"]);
    git(&path, &["rev-parse", "HEAD"])
}

async fn head(app: &TestApp, repo: Uuid) -> String {
    git(&repo_path(app, repo).await, &["rev-parse", "HEAD"])
}

async fn graph(client: &helpers::AuthenticatedClient, ws: Uuid, query: &str) -> Value {
    let response = client
        .get(&format!(
            "/api/v1/workspaces/{ws}/source-control/change-graph?comparison=workspace{query}"
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, 200, "{body}");
    body["data"].clone()
}

fn node<'a>(graph: &'a Value, path: &str) -> &'a Value {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["path"] == path)
        .unwrap_or_else(|| panic!("no node for {path}: {graph}"))
}

fn has_node(graph: &Value, path: &str) -> bool {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["path"] == path)
}

fn records(node: &Value) -> Vec<(String, String, String, bool)> {
    let mut rows: Vec<_> = node["changedRecords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            (
                record["id"].as_str().unwrap().to_string(),
                record["changeState"].as_str().unwrap().to_string(),
                record["kind"].as_str().unwrap_or_default().to_string(),
                record["isHuman"].as_bool().unwrap(),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Evidence rows as `(source path, source id, relationship, target path,
/// target id, change state, target deleted)`.
fn evidence(graph: &Value) -> Vec<(String, String, String, String, String, String, bool)> {
    let path_of = |id: &Value| {
        graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == *id)
            .map(|node| node["path"].as_str().unwrap().to_string())
            .unwrap()
    };
    let mut rows = Vec::new();
    for edge in graph["edges"].as_array().unwrap() {
        let source = path_of(&edge["sourceNodeId"]);
        let target = path_of(&edge["targetNodeId"]);
        for row in edge["evidence"].as_array().unwrap() {
            rows.push((
                source.clone(),
                row["sourceId"].as_str().unwrap().to_string(),
                row["relationship"].as_str().unwrap().to_string(),
                target.clone(),
                row["targetId"].as_str().unwrap().to_string(),
                row["changeState"].as_str().unwrap().to_string(),
                row["targetDeleted"].as_bool().unwrap_or(false),
            ));
        }
    }
    rows.sort();
    rows
}

struct Fixture {
    app: TestApp,
    user: TestUser,
    child: TestWorkspace,
    docs: Uuid,
}

/// A parent and child with representative current-format ontology, intents,
/// legacy layouts and code; the child then edits, moves and breaks pages.
async fn committed_fixture() -> Fixture {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let docs = child.add_docs_repo(&app).await;
    let req = format!("docs/{}/req.md", child.id);
    let questions = format!("docs/{}/questions/scope.g5q.json", child.id);

    child
        .commit_file(
            &app,
            ROOT,
            &page(
                "Root",
                &[
                    (
                        "a",
                        "I",
                        "Alpha cites [x](features/x.g5.md#x.one).",
                        "agent",
                    ),
                    (
                        "b",
                        "D",
                        "Beta has an old broken link to [gone](gone.g5.md).",
                        "agent",
                    ),
                    ("c", "I", "Gamma will be deleted.", "agent"),
                ],
            ),
        )
        .await;
    child
        .commit_file(
            &app,
            X,
            &page("X", &[("x.one", "I", "Target statement.", "agent")]),
        )
        .await;
    child
        .commit_file(
            &app,
            META,
            &page("Meta", &[("m.one", "I", "Stable body.", "agent")]),
        )
        .await;
    child
        .commit_file(
            &app,
            BROKEN,
            &page("Broken", &[("k.one", "I", "Fine before.", "agent")]),
        )
        .await;
    child
        .commit_file(
            &app,
            OLD,
            &page("Old", &[("o.one", "I", "Moving page.", "agent")]),
        )
        .await;
    child
        .commit_file(&app, NOTES, "# Notes\n\nPlain page.\n")
        .await;
    child
        .commit_file(&app, CHAT, "# Request\n\nOriginal ask.\n")
        .await;
    child.commit_file(&app, PLAN, "# Plan\n\nDraft.\n").await;
    child.commit_file(&app, LIB, "pub fn lib() {}\n").await;
    child
        .commit_file(&app, "src/main.rs", "fn main() {}\n")
        .await;
    child
        .commit_file(
            &app,
            ".semlift/specs/root.semlift.md",
            "[I1. Legacy]\n\nOld.\n",
        )
        .await;
    child
        .commit_file(&app, "intent_record/ontology/root.jsonl", "{}\n")
        .await;
    child.commit_file_in_repo(&app, docs, &req, "# Requirement\n\nOriginal.\n");
    child.commit_file_in_repo(&app, docs, &questions, "{\"questions\":[]}\n");
    child.attach_parent(&app).await;

    child
        .commit_file(
            &app,
            ROOT,
            &page(
                "Root renamed",
                &[
                    (
                        "a",
                        "I",
                        "Alpha, edited, cites [x](features/x.g5.md#x.one), [code](/src/lib.rs), \
                         [the ask](../chat_intent/w/request.md) and [nothing](nope.g5.md#zz).",
                        "human-provided",
                    ),
                    ("b", "D", "Beta has an old broken link to [gone](gone.g5.md).", "agent"),
                    ("d", "Q", "Delta question? Expands [a](#a)", "agent"),
                ],
            )
            .replace(
                "### a\n\n- human-provided\n",
                "### a\n\n- human-provided\n- [Permalink](https://github.com/G5Labs/G5-Lean/blob/abc/.g5/ontology/root.g5.md)\n",
            ),
        )
        .await;
    child
        .commit_file(
            &app,
            Y,
            &page(
                "Y",
                &[(
                    "y.one",
                    "I",
                    &format!(
                        "Cites [deleted gamma](../root.g5.md#c) and [the requirement](g5://file/{req}?repo={docs}).",
                    ),
                    "human-verified",
                )],
            ),
        )
        .await;
    child
        .commit_file(
            &app,
            META,
            &page("Meta", &[("m.one", "I", "Stable body.", "human-verified")]),
        )
        .await;
    child
        .commit_file(
            &app,
            BROKEN,
            "# Broken\n\n<a id=\"k.one\"></a>\n[I · k.one]\n\nNo footer any more.\n",
        )
        .await;
    rename(&app, child.repo_id, OLD, NEW).await;
    child
        .commit_file(&app, NOTES, "# Notes\n\nEdited plain page.\n")
        .await;
    child
        .commit_file(&app, CHAT, "# Request\n\nRevised ask.\n")
        .await;
    child.commit_file(&app, PLAN, "# Plan\n\nRevised.\n").await;
    child
        .commit_file(&app, "src/main.rs", "fn main() { lib(); }\n")
        .await;
    child
        .commit_file(
            &app,
            ".semlift/specs/root.semlift.md",
            "[I1. Legacy]\n\nNew.\n",
        )
        .await;
    child
        .commit_file(&app, "intent_record/ontology/root.jsonl", "{\"x\":1}\n")
        .await;
    child.commit_file_in_repo(&app, docs, &req, "# Requirement\n\nRevised.\n");
    child.commit_file_in_repo(&app, docs, &questions, "{\"questions\":[1]}\n");
    Fixture {
        app,
        user,
        child,
        docs,
    }
}

#[tokio::test]
async fn ontology_files_statements_columns_and_links_follow_the_current_format() {
    let Fixture {
        app,
        user,
        child,
        docs,
    } = committed_fixture().await;
    let client = app.authenticated_client(&user);
    let data = graph(&client, child.id, "").await;
    let req = format!("docs/{}/req.md", child.id);
    let questions = format!("docs/{}/questions/scope.g5q.json", child.id);

    // Columns and grouping (g5.diff-graph.columns).
    for (path, column, group) in [
        (ROOT, "ontology", Value::Null),
        (Y, "ontology", json!("features")),
        (META, "ontology", json!("concepts")),
        (BROKEN, "ontology", json!("concepts")),
        (OLD, "ontology", json!("features")),
        (NEW, "ontology", json!("features")),
    ] {
        let found = node(&data, path);
        assert_eq!(found["column"], column, "{path}");
        assert_eq!(found["ontologyGroup"].clone(), group, "{path}");
        assert_eq!(found["route"]["fileType"], "ontology", "{path}");
    }
    assert_eq!(node(&data, ROOT)["title"], "root");
    for (path, column, group) in [
        (NOTES, "intents", "other"),
        (CHAT, "intents", "conversations"),
        (PLAN, "intents", "documents"),
        (req.as_str(), "intents", "documents"),
        (questions.as_str(), "intents", "other"),
    ] {
        let found = node(&data, path);
        assert_eq!(
            (found["column"].as_str(), found["intentGroup"].as_str()),
            (Some(column), Some(group)),
            "{path}"
        );
    }
    for path in [
        "src/main.rs",
        ".semlift/specs/root.semlift.md",
        "intent_record/ontology/root.jsonl",
    ] {
        assert_eq!(node(&data, path)["column"], "source_code", "{path}");
    }

    // Statement changes from the shared rule (g5.diff-graph.ontology-nodes).
    assert_eq!(
        records(node(&data, ROOT)),
        vec![
            ("a".into(), "modified".into(), "I".into(), true),
            ("c".into(), "deleted".into(), "I".into(), false),
            ("d".into(), "added".into(), "Q".into(), false),
        ]
    );
    let meta = node(&data, META);
    assert_eq!(meta["changeState"], "modified");
    assert!(
        records(meta).is_empty(),
        "metadata-only changes flag no statement"
    );
    let broken = node(&data, BROKEN);
    assert!(broken["comparisonWarning"]
        .as_str()
        .unwrap()
        .contains("Missing or ambiguous footer metadata"));
    assert!(records(broken).is_empty());
    assert_eq!(node(&data, OLD)["changeState"], "deleted");
    assert_eq!(node(&data, NEW)["changeState"], "added");
    assert_eq!(
        records(node(&data, NEW)),
        vec![("o.one".into(), "added".into(), "I".into(), false)]
    );

    // Typed links, context nodes, deleted and missing targets.
    let rows = evidence(&data);
    let row = |source: &str,
               id: &str,
               kind: &str,
               target: &str,
               target_id: &str,
               state: &str,
               deleted: bool| {
        (
            source.to_string(),
            id.to_string(),
            kind.to_string(),
            target.to_string(),
            target_id.to_string(),
            state.to_string(),
            deleted,
        )
    };
    for expected in [
        row(ROOT, "a", "reference", X, "x.one", "unchanged", false),
        row(ROOT, "a", "implementation", LIB, "", "added", false),
        row(ROOT, "a", "provenance", CHAT, "", "added", false),
        row(Y, "y.one", "reference", ROOT, "c", "added", true),
        row(Y, "y.one", "project_document", &req, "", "added", false),
    ] {
        assert!(
            rows.contains(&expected),
            "missing {expected:?} in {rows:#?}"
        );
    }
    assert!(
        !rows.iter().any(|r| r.2 == "expands"),
        "a same-file Expands link is not drawn"
    );
    assert!(
        !rows.iter().any(|r| r.3.contains("github")),
        "permalinks are external"
    );
    let x = node(&data, X);
    assert_eq!(
        (x["changeState"].as_str(), x["connectionState"].as_str()),
        (Some("unchanged"), Some("bridge"))
    );
    let lib = node(&data, LIB);
    assert_eq!(lib["column"], "source_code");
    assert_eq!(lib["changeState"], "unchanged");
    assert_eq!(
        node(&data, ROOT)["missingTargets"],
        json!([{"statementId": "a", "href": "nope.g5.md#zz", "text": "nothing"}]),
        "only the edited broken link is reported; the unchanged one in b is not"
    );
    assert!(node(&data, Y).get("missingTargets").is_none());
    let docs_req = node(&data, &req);
    assert_eq!(docs_req["route"]["repoId"], docs.to_string());

    // Live content: no running VM, so committed heads, said so.
    for repo in data["range"]["liveContent"].as_array().unwrap() {
        assert_eq!(repo["includesUncommitted"], false);
        assert_eq!(repo["reason"], "vm_not_running");
    }

    // Details list changed statements and the file-level Other changes note.
    let root_id = node(&data, ROOT)["id"].as_str().unwrap().to_string();
    let details: Value = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/change-graph/nodes/{root_id}/details?comparison=workspace&revision={}",
            child.id, data["revision"].as_str().unwrap()
        ))
        .await
        .json()
        .await
        .unwrap();
    let details = &details["data"];
    assert_eq!(details["kind"], "semantic_diff", "{details}");
    let ids: Vec<_> = details["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["a", "c", "d"]);
    assert_eq!(details["records"][0]["recordKind"], "I");
    assert!(
        details["otherFileChanges"].is_object(),
        "the heading changed: {details}"
    );
    let broken_id = node(&data, BROKEN)["id"].as_str().unwrap().to_string();
    let broken_details: Value = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/change-graph/nodes/{broken_id}/details?comparison=workspace&revision={}",
            child.id, data["revision"].as_str().unwrap()
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(broken_details["data"]["kind"], "file_diff");

    // Human intent lens: ontology files with a changed human statement only.
    let lensed = graph(&client, child.id, "&humanIntent=true").await;
    let mut roots: Vec<_> = lensed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["connectionState"] != "bridge")
        .map(|n| n["path"].as_str().unwrap().to_string())
        .collect();
    roots.sort();
    assert_eq!(roots, vec![Y.to_string(), ROOT.to_string()]);
    assert_eq!(
        records(node(&lensed, ROOT)),
        vec![("a".into(), "modified".into(), "I".into(), true)]
    );
    assert!(
        has_node(&lensed, CHAT),
        "links of kept files remain, with their context"
    );
    assert_eq!(node(&lensed, CHAT)["changeState"], "modified");

    // Uncommitted only is refused rather than answered with an empty graph.
    let refused = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/change-graph?comparison=uncommitted",
            child.id
        ))
        .await;
    assert_eq!(refused.status(), 409);
    let refused: Value = refused.json().await.unwrap();
    assert_eq!(refused["error"]["code"], "DIFF_GRAPH_UNAVAILABLE");
    assert_eq!(
        refused["error"]["details"]["reason"],
        "uncommitted_unsupported"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn all_changes_and_diff_graph_report_the_same_statement_changes() {
    let Fixture {
        app, user, child, ..
    } = committed_fixture().await;
    let client = app.authenticated_client(&user);
    let data = graph(&client, child.id, "").await;
    let overview: Value = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/ontology-changes",
            child.id
        ))
        .await
        .json()
        .await
        .unwrap();
    let mut compared_files = 0;
    for file in overview["data"]["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        // The overview ships bytes; the editor compares them with the same rule.
        match ontology_comparison::compare(file["baseline"].as_str(), file["current"].as_str()) {
            Ok(comparison) => {
                let mut expected: Vec<_> = comparison
                    .changes
                    .iter()
                    .map(|c| (c.id.clone(), format!("{:?}", c.change).to_lowercase()))
                    .collect();
                expected.sort();
                if expected.is_empty() && !comparison.other_text_changed() {
                    assert!(!has_node(&data, path), "{path} has nothing to show");
                    continue;
                }
                let actual: Vec<_> = records(node(&data, path))
                    .into_iter()
                    .map(|r| (r.0, r.1))
                    .collect();
                assert_eq!(actual, expected, "{path}");
                compared_files += 1;
            }
            Err(_) => assert!(node(&data, path)["comparisonWarning"].is_string(), "{path}"),
        }
    }
    assert!(compared_files >= 5, "{overview}");
    // Plain Markdown under .g5/ontology is not an ontology file for either view.
    assert!(!overview["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["path"] == NOTES));
    app.cleanup().await;
}

#[tokio::test]
async fn all_changes_layers_saved_edits_for_every_file_for_the_owner() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let reader = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let docs = child.add_docs_repo(&app).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child.commit_file(&app, "src/a.rs", "a\n").await;
    child.commit_file(&app, "src/gone.rs", "gone\n").await;
    child.attach_parent(&app).await;
    child
        .commit_file(&app, "src/committed.rs", "committed\n")
        .await;
    let code_head = head(&app, child.repo_id).await;
    let docs_head = head(&app, docs).await;

    let members = json!([
        {"repo_id": child.repo_id, "head_oid": code_head, "errors": [], "dirty": [
            {"status": " M", "path": "src/a.rs"},
            {"status": "??", "path": "src/new.rs"},
            {"status": " D", "path": "src/gone.rs"},
            {"status": " M", "path": ROOT}
        ]},
        {"repo_id": docs, "head_oid": docs_head, "errors": [], "dirty": []}
    ]);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app, &child, owner.id, members,
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page(
                "Root",
                &[("a", "I", "Saved, not committed.", "human-provided")],
            ),
        )
        .await;
    runtime
        .seed_file(child.repo_id, "src/a.rs", "a, edited\n")
        .await;
    runtime
        .seed_file(child.repo_id, "src/new.rs", "new\n")
        .await;

    let data = graph(&app.authenticated_client(&owner), child.id, "").await;
    for repo in data["range"]["liveContent"].as_array().unwrap() {
        assert_eq!(repo["includesUncommitted"], true, "{repo}");
        assert_eq!(repo["ontologyLive"], true, "{repo}");
    }
    for (path, state, origin) in [
        ("src/a.rs", "modified", "uncommitted"),
        ("src/new.rs", "added", "uncommitted"),
        ("src/gone.rs", "deleted", "uncommitted"),
        ("src/committed.rs", "added", "committed"),
        (ROOT, "modified", "uncommitted"),
    ] {
        let found = node(&data, path);
        assert_eq!(
            (
                found["changeState"].as_str(),
                found["changeOrigin"].as_str()
            ),
            (Some(state), Some(origin)),
            "{path}"
        );
    }
    assert_eq!(
        records(node(&data, ROOT)),
        vec![("a".into(), "modified".into(), "I".into(), true)]
    );

    // A reader sees committed heads (g5.workspace.durable-read), said so.
    let read = graph(&app.authenticated_client(&reader), child.id, "").await;
    assert!(!has_node(&read, "src/a.rs") && !has_node(&read, ROOT));
    assert!(has_node(&read, "src/committed.rs"));
    for repo in read["range"]["liveContent"].as_array().unwrap() {
        assert_eq!(
            (
                repo["includesUncommitted"].as_bool(),
                repo["reason"].as_str()
            ),
            (Some(false), Some("not_owner"))
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn unpublished_local_commits_keep_ontology_live_and_say_other_files_are_committed() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child
        .commit_file(
            &app,
            X,
            &page(
                "X",
                &[("x.one", "I", "Will be deleted in the VM.", "agent")],
            ),
        )
        .await;
    child.commit_file(&app, "src/a.rs", "a\n").await;
    child.attach_parent(&app).await;
    let members = json!([
        {"repo_id": child.repo_id, "head_oid": "1111111111111111111111111111111111111111", "errors": [],
         "against_internal": {"ahead": 1, "behind": 0}, "dirty": [
            {"status": " M", "path": "src/a.rs"}
        ]}
    ]);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app, &child, owner.id, members,
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page(
                "Root",
                &[("a", "I", "Edited in an unpublished commit.", "agent")],
            ),
        )
        .await;
    runtime
        .seed_file(child.repo_id, "src/a.rs", "a, edited\n")
        .await;

    let data = graph(&app.authenticated_client(&owner), child.id, "").await;
    let status = &data["range"]["liveContent"][0];
    assert_eq!(
        (
            status["includesUncommitted"].as_bool(),
            status["reason"].as_str(),
            status["ontologyLive"].as_bool()
        ),
        (Some(false), Some("unpublished_commits"), Some(true))
    );
    assert_eq!(
        records(node(&data, ROOT)),
        vec![("a".into(), "modified".into(), "I".into(), false)]
    );
    assert_eq!(
        node(&data, X)["changeState"],
        "deleted",
        "a page absent from the VM is deleted"
    );
    assert!(!has_node(&data, "src/a.rs"), "other files stay committed");
    app.cleanup().await;
}

#[tokio::test]
async fn rebase_preview_flags_same_statement_and_same_file_conflicts_in_one_ontology_lane() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    let base = page(
        "Root",
        &[("a", "I", "A.", "agent"), ("b", "I", "B.", "agent")],
    );
    let x_base = page(
        "X",
        &[
            ("x.one", "I", "One.", "agent"),
            ("x.two", "I", "Two.", "agent"),
        ],
    );
    child.commit_file(&app, ROOT, &base).await;
    let base_commit = child.commit_file(&app, X, &x_base).await;
    let parent = child.attach_parent(&app).await;
    child
        .commit_file(&app, ROOT, &base.replace("A.", "A, source."))
        .await;
    child
        .commit_file(&app, X, &x_base.replace("One.", "One, source."))
        .await;

    // The parent changes the same statement in root and another statement in x.
    let path = repo_path(&app, child.repo_id).await;
    git(
        &path,
        &["checkout", "-q", "-b", "target-side", &base_commit],
    );
    std::fs::write(path.join(ROOT), base.replace("A.", "A, target.")).unwrap();
    std::fs::write(path.join(X), x_base.replace("Two.", "Two, target.")).unwrap();
    git(&path, &["commit", "-qam", "Target change"]);
    let parent_head = git(&path, &["rev-parse", "HEAD"]);
    git(&path, &["checkout", "-q", &child.name]);
    helpers::seed_branch_ref(
        &app,
        child.repo_id,
        &format!("parent-{parent}"),
        &parent_head,
    )
    .await;

    let response = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/workspaces/{}/rebase-merge/change-graph?targetWorkspaceId={parent}",
            child.id
        ))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, 200, "{body}");
    let data = &body["data"];
    for side in ["source", "target"] {
        for found in data[side]["nodes"].as_array().unwrap() {
            assert_eq!(found["column"], "ontology", "{found}");
        }
    }
    let root_id = node(&data["source"], ROOT)["id"].clone();
    let x_id = node(&data["source"], X)["id"].clone();
    let conflict = |id: &Value| {
        data["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["artifactKey"] == *id)
            .cloned()
            .unwrap()
    };
    assert_eq!(conflict(&root_id)["kind"], "same_block");
    assert_eq!(conflict(&root_id)["overlappingBlockIds"], json!(["a"]));
    assert_eq!(conflict(&x_id)["kind"], "same_node");
    app.cleanup().await;
}

#[tokio::test]
async fn unavailable_status_and_failed_live_reads_never_pass_for_complete() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child.commit_file(&app, "src/a.rs", "a\n").await;
    child.attach_parent(&app).await;
    let code_head = head(&app, child.repo_id).await;
    // `git status` could not be collected: the member reports `dirty: null`.
    let members = json!([
        {"repo_id": child.repo_id, "head_oid": code_head, "errors": ["STATUS_UNAVAILABLE"], "dirty": null}
    ]);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app, &child, owner.id, members,
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page("Root", &[("a", "I", "Saved edit.", "agent")]),
        )
        .await;
    runtime
        .seed_file(child.repo_id, "src/a.rs", "a, saved\n")
        .await;
    let client = app.authenticated_client(&owner);

    let data = graph(&client, child.id, "").await;
    let status = &data["range"]["liveContent"][0];
    assert_eq!(
        (
            status["includesUncommitted"].as_bool(),
            status["reason"].as_str(),
            status["ontologyLive"].as_bool()
        ),
        (Some(false), Some("status_unavailable"), Some(true))
    );
    assert_eq!(
        records(node(&data, ROOT)),
        vec![("a".into(), "modified".into(), "I".into(), false)]
    );
    assert!(!has_node(&data, "src/a.rs"));

    // A live page that cannot be read refuses the graph instead of omitting it.
    runtime.refuse_reads(child.repo_id, ROOT).await;
    let refused = client
        .get(&format!(
            "/api/v1/workspaces/{}/source-control/change-graph?comparison=workspace",
            child.id
        ))
        .await;
    assert!(!refused.status().is_success());
    app.cleanup().await;
}

#[tokio::test]
async fn a_failed_changes_report_keeps_ontology_live_and_other_files_committed() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child.commit_file(&app, "src/a.rs", "a\n").await;
    child.attach_parent(&app).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_failure(
        &app, &child, owner.id, "get",
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page("Root", &[("a", "I", "Saved edit.", "agent")]),
        )
        .await;
    runtime
        .seed_file(child.repo_id, "src/a.rs", "a, saved\n")
        .await;

    let data = graph(&app.authenticated_client(&owner), child.id, "").await;
    let status = &data["range"]["liveContent"][0];
    assert_eq!(
        (
            status["includesUncommitted"].as_bool(),
            status["reason"].as_str(),
            status["ontologyLive"].as_bool()
        ),
        (Some(false), Some("report_unavailable"), Some(true))
    );
    assert_eq!(
        records(node(&data, ROOT)),
        vec![("a".into(), "modified".into(), "I".into(), false)]
    );
    assert!(!has_node(&data, "src/a.rs"));
    app.cleanup().await;
}

#[tokio::test]
async fn vanished_unreadable_and_behind_dirty_paths_degrade_without_failing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child.commit_file(&app, "src/a.rs", "a\n").await;
    child
        .commit_file(&app, "src/vanished.rs", "here at the head\n")
        .await;
    child.attach_parent(&app).await;
    let code_head = head(&app, child.repo_id).await;
    let members = json!([
        {"repo_id": child.repo_id, "head_oid": code_head, "errors": [], "dirty": [
            {"status": " M", "path": "src/vanished.rs"},
            {"status": " M", "path": "src/a.rs"}
        ]}
    ]);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app, &child, owner.id, members,
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    runtime
        .seed_file(child.repo_id, "src/a.rs", "a, saved\n")
        .await;
    let client = app.authenticated_client(&owner);

    // A path listed by git status but gone when read is a deletion in the VM.
    let data = graph(&client, child.id, "").await;
    assert_eq!(data["range"]["liveContent"][0]["includesUncommitted"], true);
    let vanished = node(&data, "src/vanished.rs");
    assert_eq!(
        (
            vanished["changeState"].as_str(),
            vanished["changeOrigin"].as_str()
        ),
        (Some("deleted"), Some("uncommitted"))
    );
    assert_eq!(node(&data, "src/a.rs")["changeOrigin"], "uncommitted");

    // An unreadable saved file degrades the repository's other files, said so.
    runtime.refuse_reads(child.repo_id, "src/a.rs").await;
    let data = graph(&client, child.id, "").await;
    let status = &data["range"]["liveContent"][0];
    assert_eq!(
        (
            status["includesUncommitted"].as_bool(),
            status["reason"].as_str(),
            status["ontologyLive"].as_bool()
        ),
        (Some(false), Some("report_unavailable"), Some(true))
    );
    assert!(!has_node(&data, "src/a.rs") && !has_node(&data, "src/vanished.rs"));
    app.cleanup().await;
}

#[tokio::test]
async fn a_vm_behind_the_published_head_is_reported_as_synchronizing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut child = TestWorkspace::create(&app, &org).await;
    child
        .commit_file(
            &app,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    child.attach_parent(&app).await;
    let members = json!([
        {"repo_id": child.repo_id, "head_oid": "2222222222222222222222222222222222222222", "errors": [],
         "against_internal": {"ahead": 0, "behind": 3}, "dirty": []}
    ]);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app, &child, owner.id, members,
    )
    .await;
    runtime
        .seed_file(
            child.repo_id,
            ROOT,
            &page("Root", &[("a", "I", "Committed.", "agent")]),
        )
        .await;
    let data = graph(&app.authenticated_client(&owner), child.id, "").await;
    assert_eq!(
        data["range"]["liveContent"][0]["reason"],
        "workspace_synchronizing"
    );
    app.cleanup().await;
}
