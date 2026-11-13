//! Integration tests for `GET /api/v1/intent-log` (Intent Log projection).
//!
//! Test view: `views/test_views/intent_log.test_view.md` — TC-ILOG-001
//! (mixed-source chronological table), TC-ILOG-002 (no-mutation), TC-ILOG-004
//! (filters/diagnostics), TC-ILOG-012 (branch-level auth), TC-ILOG-101
//! (normalization), TC-ILOG-103 (legacy `external_design_doc` shape).
//!
//! Each test spawns a full `TestApp`, seeds committed `intent_record` /
//! `intent_grounding` JSONL into the workspace branch's git repo, and asserts at
//! the HTTP + JSON-shape level.

use g5_semantic::local_grounding::jsonl_store::encode_file_name;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

use crate::helpers;

use helpers::{seed_workspace_authority_head, TestApp, TestOrg, TestTask, TestUser, TestWorkspace};

// ─── Seeding helpers ─────────────────────────────────────────────────────────

fn git(repo_path: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn git_head(repo_path: &Path) -> String {
    String::from_utf8(git(repo_path, &["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

fn write_jsonl(repo_path: &Path, relative: &str, lines: &[String]) {
    let path = repo_path.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut body = lines.join("\n");
    body.push('\n');
    std::fs::write(path, body).unwrap();
}

fn write_text(repo_path: &Path, relative: &str, body: &str) {
    let path = repo_path.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn ir_path(subfolder: &str, file_name: &str) -> String {
    format!(
        "intent_record/{}/{}.jsonl",
        subfolder,
        encode_file_name(file_name)
    )
}

fn ig_path(subfolder: &str, target_file: &str) -> String {
    format!(
        "intent_grounding/{}/{}.jsonl",
        subfolder,
        encode_file_name(target_file)
    )
}

/// Commits everything under the repo and advances the workspace Git ref.
async fn commit_and_advance(
    app: &TestApp,
    repo_path: &Path,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> String {
    git(repo_path, &["add", "."]);
    git(repo_path, &["commit", "-m", "seed intent records"]);
    let head = git_head(repo_path);
    seed_workspace_authority_head(app, workspace_id, repo_id, &head).await;
    head
}

async fn assert_head_stable(repo_path: &Path, expected: &str) {
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(
        git_head(repo_path),
        expected,
        "Git head changed unexpectedly"
    );
}

async fn get_intent_log(app: &TestApp, user: &TestUser, query: &str) -> Value {
    let resp = app.authenticated_client(user).get(query).await;
    assert_eq!(resp.status(), StatusCode::OK);
    resp.json().await.unwrap()
}

fn seed_missing_provenance_records(repo_path: &Path) -> String {
    let spec = ".semlift/specs/provenance.semlift.md";
    write_text(
        repo_path,
        spec,
        "[I1. Missing provenance]\nBackfill this block from git blame.\n\n[I2. Existing actor]\nPreserve the explicit actor while filling time.\n",
    );
    write_jsonl(
        repo_path,
        &ir_path("ontology", spec),
        &[
            json!({"file_type":"ontology","file_name":spec,"intent_record_number":"I1","is_human":true}).to_string(),
            json!({"file_type":"ontology","file_name":spec,"intent_record_number":"I2","is_human":true,"created_by":"Manual Author"}).to_string(),
        ],
    );
    ir_path("ontology", spec)
}

fn seed_single_missing_provenance_record(repo_path: &Path, spec: &str, label: &str) -> String {
    write_text(
        repo_path,
        spec,
        &format!("[I1. {label}]\nBackfill this block from git blame.\n"),
    );
    write_jsonl(
        repo_path,
        &ir_path("ontology", spec),
        &[json!({"file_type":"ontology","file_name":spec,"intent_record_number":"I1","is_human":true}).to_string()],
    );
    ir_path("ontology", spec)
}

const TS: &str = "2026-05-26T14:54:07.628594+00:00";

fn rows_of(body: &Value) -> &Vec<Value> {
    body["data"]["rows"].as_array().expect("data.rows array")
}

fn find_kind<'a>(body: &'a Value, kind: &str) -> Option<&'a Value> {
    rows_of(body).iter().find(|r| r["intentKind"] == kind)
}

fn find_kind_block<'a>(body: &'a Value, kind: &str, block_id: &str) -> Option<&'a Value> {
    rows_of(body)
        .iter()
        .find(|r| r["intentKind"] == kind && r["blockId"] == block_id)
}

// ─── TC-ILOG-001 / 101 / 103 — normalization + author-type eligibility + trace

#[tokio::test]
async fn test_intent_log_lists_human_records_with_normalization() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    // Ontology: one human (I1) + one machine/agent (I2, hidden by default).
    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/foo.semlift.md"),
        &[
            json!({"file_type":"ontology","file_name":".semlift/specs/foo.semlift.md","intent_record_number":"I1","content":"Initialize the module.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            json!({"file_type":"ontology","file_name":".semlift/specs/foo.semlift.md","intent_record_number":"I2","content":"Generated.","is_human":false,"created_by":"Codex","created_at":TS}).to_string(),
        ],
    );
    // View (human).
    write_jsonl(
        &repo_path,
        &ir_path("views", "views/pm_views/bar.pm_view.md"),
        &[json!({"file_type":"view","file_name":"views/pm_views/bar.pm_view.md","intent_record_number":"I1","content":"As a user I can audit intent.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string()],
    );
    // Chat: C1/C2 are human; C3 is a supervisor-agent decision.
    write_jsonl(
        &repo_path,
        &ir_path("chat", "sess-1"),
        &[
            json!({"file_type":"chat","file_name":"sess-1","intent_record_number":"C1","content":"Add the intent log","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            json!({"file_type":"chat","file_name":"sess-1","intent_record_number":"C2","content":"Follow-up without grounding","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            json!({"file_type":"chat","file_name":"sess-1","intent_record_number":"C3","content":"Accepted supervisor decision","is_human":true,"created_by":"human_like_agent:supervisor","created_at":TS}).to_string(),
        ],
    );
    // External design doc (legacy file_type spelling; human) → external_doc/design_doc.
    write_jsonl(
        &repo_path,
        &ir_path("external_design_doc", "designs/baz.md"),
        &[json!({"file_type":"external_design_doc","file_name":"designs/baz.md","intent_record_number":"dd1","content":"# Design","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string()],
    );
    // Grounding edge: view I1 → ontology I1 (makes both endpoints traceable).
    write_jsonl(
        &repo_path,
        &ig_path("ontology", ".semlift/specs/foo.semlift.md"),
        &[json!({"source_file_type":"view","target_file_type":"ontology","source_file":"views/pm_views/bar.pm_view.md","target_file":".semlift/specs/foo.semlift.md","source_id":"I1","target_id":"I1","relationship":"grounds","relevance":90}).to_string()],
    );
    // Grounding edge: chat C1 → view I1 (makes the chat source traceable).
    write_jsonl(
        &repo_path,
        &ig_path("views", "views/pm_views/bar.pm_view.md"),
        &[json!({"source_file_type":"chat","target_file_type":"view","source_file":"sess-1","target_file":"views/pm_views/bar.pm_view.md","source_id":"C1","target_id":"I1","relationship":"requests","relevance":90}).to_string()],
    );

    commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let resp = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();

    // Default view is authorType=human: the machine ontology row and
    // supervisor-agent chat row are hidden until explicitly requested.
    let rows = rows_of(&body);
    assert_eq!(
        rows.len(),
        5,
        "expected 5 default human-author rows, got {}: {body}",
        rows.len()
    );
    assert!(
        rows.iter().all(|r| r["authorType"] == "human"),
        "non-human authorType row leaked into the default log"
    );

    // Ontology row: traceable (grounding edge targets it).
    let ont = find_kind(&body, "ontology").expect("ontology row");
    assert_eq!(ont["traceStatus"], "traceable");
    assert_eq!(ont["blockId"], "I1");

    // External row: normalized to external_doc / design_doc (TC-ILOG-103).
    let ext = find_kind(&body, "external_doc").expect("external_doc row");
    assert_eq!(ext["artifactType"], "design_doc");

    // Chat rows: grounded chat intent is traceable; ungrounded chat stays honest.
    let chat_grounded = find_kind_block(&body, "chat", "C1").expect("grounded chat row");
    assert_eq!(chat_grounded["traceStatus"], "traceable");
    let chat_unlinked = find_kind_block(&body, "chat", "C2").expect("unlinked chat row");
    assert_eq!(chat_unlinked["traceStatus"], "unlinked");

    let agent_body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&authorType=agent"),
    )
    .await;
    let agent_rows = rows_of(&agent_body);
    assert_eq!(agent_rows.len(), 1);
    assert_eq!(agent_rows[0]["blockId"], "I2");
    assert_eq!(agent_rows[0]["authorType"], "agent");
    assert_eq!(agent_rows[0]["isHuman"], false);

    let supervisor_body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&authorType=supervisor_agent"),
    )
    .await;
    let supervisor_rows = rows_of(&supervisor_body);
    assert_eq!(supervisor_rows.len(), 1);
    assert_eq!(supervisor_rows[0]["blockId"], "C3");
    assert_eq!(supervisor_rows[0]["intentKind"], "chat");
    assert_eq!(supervisor_rows[0]["authorType"], "supervisor_agent");

    // Well-formed sources → no diagnostics.
    assert_eq!(
        body["data"]["meta"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    app.cleanup().await;
}

// ─── TC-ILOG-004 / D8 — malformed JSONL → partial rows + diagnostics ─────────

#[tokio::test]
async fn test_intent_log_partial_rows_and_diagnostics_on_malformed() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/foo.semlift.md"),
        &[
            json!({"file_type":"ontology","file_name":".semlift/specs/foo.semlift.md","intent_record_number":"I1","content":"Valid.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            "this is not valid json".to_string(),
        ],
    );
    commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let resp = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();

    // The valid row is still returned (partial success).
    assert_eq!(rows_of(&body).len(), 1);
    // The malformed line is surfaced as a diagnostic, not silently dropped.
    let diags = body["data"]["meta"]["diagnostics"].as_array().unwrap();
    assert!(!diags.is_empty(), "expected a malformed-source diagnostic");
    assert!(diags[0]["sourcePath"]
        .as_str()
        .unwrap()
        .contains("ontology"));

    app.cleanup().await;
}

// ─── TC-ILOG-012 — auth (no token, missing param, cross-org) ─────────────────

#[tokio::test]
async fn test_intent_log_auth_and_required_param() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;

    // 1. No auth → 401.
    let unauth = app
        .client
        .get(format!(
            "{}/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}",
            app.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    // 2. Missing required workspace/repository parameters → 400.
    let missing = app
        .authenticated_client(&user)
        .get("/api/v1/intent-log")
        .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    // 3. A user from a different org cannot read this branch (branch-level auth).
    let mut other = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other).await;
    let _ = TestWorkspace::create(&app, &other_org).await; // unrelated workspace
    let cross = app
        .authenticated_client(&other)
        .get(&format!(
            "/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"
        ))
        .await;
    assert!(
        !cross.status().is_success(),
        "cross-org read must be denied, got {}",
        cross.status()
    );

    app.cleanup().await;
}

// ─── TC-ILOG-002 — reading does not mutate when metadata is present ──────────

#[tokio::test]
async fn test_intent_log_read_does_not_mutate_with_present_metadata() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    // Records already carry created_at/created_by → no git-blame backfill is
    // triggered, so the read must leave the branch head untouched.
    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/foo.semlift.md"),
        &[json!({"file_type":"ontology","file_name":".semlift/specs/foo.semlift.md","intent_record_number":"I1","content":"Has metadata.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string()],
    );
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let resp = app
        .authenticated_client(&user)
        .get(&format!(
            "/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(rows_of(&resp.json::<Value>().await.unwrap()).len(), 1);

    // Give any (incorrectly-spawned) background task a chance to run, then assert
    // the Git ref is unchanged.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let head_after = git_head(&repo_path);
    assert_eq!(
        head_before, head_after,
        "read must not advance the branch head"
    );

    app.cleanup().await;
}

// ─── TC-ILOG-004 — filters / facets (source kind, time, repo) ────────────────

#[tokio::test]
async fn test_intent_log_applies_source_time_and_repo_filters() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    write_text(
        &repo_path,
        ".semlift/specs/filter-source.semlift.md",
        "[I3. Different block]\nThe filter-only record points at a missing block.\n",
    );
    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/filter-source.semlift.md"),
        &[
            json!({"file_type":"ontology","file_name":".semlift/specs/filter-source.semlift.md","intent_record_number":"I1","content":"In range.","is_human":true,"created_by":"Ziniu","created_at":"2026-06-01T12:00:00Z"}).to_string(),
            json!({"file_type":"ontology","file_name":".semlift/specs/filter-source.semlift.md","intent_record_number":"I2","content":"Too old.","is_human":true,"created_by":"Ziniu","created_at":"2026-04-01T12:00:00Z"}).to_string(),
            json!({"file_type":"ontology","file_name":".semlift/specs/filter-source.semlift.md","intent_record_number":"I99","is_human":true}).to_string(),
        ],
    );
    commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!(
            "/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&repo={repo_id}&timeRange=custom&from=2026-05-01&to=2026-06-02"
        ),
    )
    .await;
    let rows = rows_of(&body);
    assert_eq!(rows.len(), 1, "time+repo filters should return only I1");
    assert_eq!(rows[0]["blockId"], "I1");
    assert!(
        body["data"]["meta"]["facets"]["repo"]
            .as_array()
            .unwrap()
            .iter()
            .any(|bucket| bucket["count"] == 1),
        "repo facet should be computed over the filtered universe"
    );

    let git_rows = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution&authorType=unknown"),
    )
    .await;
    let rows = rows_of(&git_rows);
    assert_eq!(
        rows.len(),
        1,
        "sourceKind=git_attribution and authorType=unknown should isolate the missing-metadata row"
    );
    assert_eq!(rows[0]["blockId"], "I99");

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_git_fallback_participates_in_actor_filter_and_default_sort() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    write_text(
        &repo_path,
        ".semlift/specs/git-sort.semlift.md",
        "[I1. Git authored]\nThis block should sort by git blame time.\n\n[I2. Explicit old]\nThis block has an explicit old record timestamp.\n",
    );
    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/git-sort.semlift.md"),
        &[
            json!({"file_type":"ontology","file_name":".semlift/specs/git-sort.semlift.md","intent_record_number":"I1","is_human":true}).to_string(),
            json!({"file_type":"ontology","file_name":".semlift/specs/git-sort.semlift.md","intent_record_number":"I2","content":"Explicit old.","is_human":true,"created_by":"Ziniu","created_at":"2026-01-01T00:00:00Z"}).to_string(),
        ],
    );
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;
    let _task = TestTask::create(&app, &workspace).await;

    let sorted = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"),
    )
    .await;
    let rows = rows_of(&sorted);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]["blockId"], "I1",
        "git-derived time must participate in default chronological sort"
    );
    assert_eq!(rows[0]["occurredAtSource"], "git_blame");

    let actor_filtered = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&actor=G5%20Test"),
    )
    .await;
    let rows = rows_of(&actor_filtered);
    assert_eq!(
        rows.len(),
        1,
        "git-derived actor must participate in actor filtering"
    );
    assert_eq!(rows[0]["blockId"], "I1");
    assert_eq!(rows[0]["actorSource"], "git_blame");
    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_dedup_is_identity_only_for_non_chat() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    // The committed source file is the single content authority
    // (grounding_identity_and_routing D10); the ledger carries three copies of
    // the same natural key with divergent stored bodies that must never
    // surface anywhere.
    write_text(
        &repo_path,
        "views/pm_views/dedup.pm_view.md",
        "[I1. Dedup directive]\nSame directive.\n",
    );
    write_jsonl(
        &repo_path,
        &ir_path("views", "views/pm_views/dedup.pm_view.md"),
        &[
            json!({"file_type":"view","file_name":"views/pm_views/dedup.pm_view.md","intent_record_number":"I1","content":"Same directive.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            json!({"file_type":"view","file_name":"views/pm_views/dedup.pm_view.md","intent_record_number":"I1","content":"Same directive.","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
            json!({"file_type":"view","file_name":"views/pm_views/dedup.pm_view.md","intent_record_number":"I1","content":"STORED_CONTENT_MUST_NOT_BE_SERVED","is_human":true,"created_by":"Ziniu","created_at":TS}).to_string(),
        ],
    );
    commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"),
    )
    .await;
    let rows = rows_of(&body);
    assert_eq!(
        rows.len(),
        1,
        "non-chat dedup is identity-only (D10 / issue #1603 Q-3): duplicate \
         ledger copies of one block collapse even when stored bodies diverge"
    );
    assert_eq!(
        rows[0]["content"].as_str(),
        Some("Same directive."),
        "content comes from the committed source file, never a stored body"
    );
    assert_eq!(rows[0]["title"].as_str(), Some("Dedup directive"));
    assert_eq!(rows[0]["blockLabel"].as_str(), Some("I1"));
    assert!(
        !body
            .to_string()
            .contains("STORED_CONTENT_MUST_NOT_BE_SERVED"),
        "stored bodies must not leak anywhere in the projection response"
    );

    app.cleanup().await;
}

// ─── TC-ILOG-002 — git attribution is a read-only projection ───────────────

#[tokio::test]
async fn test_intent_log_projects_git_provenance_without_committing() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;
    seed_missing_provenance_records(&repo_path);
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution"),
    )
    .await;
    assert_eq!(rows_of(&body).len(), 2);
    assert!(
        rows_of(&body)
            .iter()
            .any(|row| row["actorSource"] == "git_blame"),
        "read should expose git-blame provenance without rewriting history"
    );
    assert_head_stable(&repo_path, &head_before).await;
    let second = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}"),
    )
    .await;
    assert_eq!(rows_of(&second).len(), 2);
    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_multiple_git_attribution_files_do_not_commit() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    seed_single_missing_provenance_record(
        &repo_path,
        ".semlift/specs/provenance-a.semlift.md",
        "A",
    );
    seed_single_missing_provenance_record(
        &repo_path,
        ".semlift/specs/provenance-b.semlift.md",
        "B",
    );
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution"),
    )
    .await;
    assert_eq!(rows_of(&body).len(), 2);

    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_external_design_git_attribution_does_not_commit() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    write_text(
        &repo_path,
        "designs/provenance.md",
        "[I1. External provenance]\nBackfill this external design intent from git blame.\n",
    );
    let record_path = ir_path("external_design_doc", "designs/provenance.md");
    write_jsonl(
        &repo_path,
        &record_path,
        &[json!({"file_type":"external_design_doc","file_name":"designs/provenance.md","intent_record_number":"I1","is_human":true}).to_string()],
    );
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution"),
    )
    .await;
    let rows = rows_of(&body);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["intentKind"], "external_doc");
    assert_eq!(rows[0]["artifactType"], "design_doc");
    assert_eq!(rows[0]["occurredAtSource"], "git_blame");
    assert_eq!(rows[0]["actorSource"], "git_blame");

    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_ambiguous_git_attribution_does_not_mutate_branch() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;

    write_text(
        &repo_path,
        ".semlift/specs/ambiguous.semlift.md",
        "[I2. Different block]\nThe requested block is absent.\n",
    );
    write_jsonl(
        &repo_path,
        &ir_path("ontology", ".semlift/specs/ambiguous.semlift.md"),
        &[json!({"file_type":"ontology","file_name":".semlift/specs/ambiguous.semlift.md","intent_record_number":"I1","is_human":true}).to_string()],
    );
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution&authorType=unknown"),
    )
    .await;
    assert_eq!(rows_of(&body).len(), 1);
    assert_eq!(rows_of(&body)[0]["actorSource"], "unknown");
    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}

#[tokio::test]
async fn test_intent_log_git_attribution_with_task_does_not_mutate_branch() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let workspace_id = workspace.id;
    let repo_id = workspace.repo_id;
    let repo_path = app.repo_path(repo_id).await;
    seed_missing_provenance_records(&repo_path);
    let head_before = commit_and_advance(&app, &repo_path, workspace_id, repo_id).await;
    let _task = TestTask::create(&app, &workspace).await;

    let body = get_intent_log(
        &app,
        &user,
        &format!("/api/v1/intent-log?workspaceId={workspace_id}&repoId={repo_id}&sourceKind=git_attribution"),
    )
    .await;
    assert_eq!(rows_of(&body).len(), 2);
    assert_head_stable(&repo_path, &head_before).await;

    app.cleanup().await;
}
