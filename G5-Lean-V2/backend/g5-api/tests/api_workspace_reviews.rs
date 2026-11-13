//! Review workspace routes (`/api/v1/workspaces/:id/review`).
//!
//! A reviewer who does not own a workspace cannot write to it (single-writer
//! rule). Start forks the source into a reviewer-owned review workspace and
//! resumes it; later Starts reopen the same workspace; Finish hands the review
//! back as a pull request into the source when the review workspace diverged
//! and stops its VM. Nothing new is stored: the review is the completed fork
//! whose idempotency key carries the review marker.

use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

fn data(body: &Value) -> &Value {
    body.get("data").unwrap_or(body)
}

#[tokio::test]
async fn retired_agent_review_endpoints_are_not_exposed() {
    let fixture = publish_per_edit_doc_with_count("Old text.\n", "New text.\n", 1).await;
    let token = fixture
        .app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(
            fixture.source.org_id,
            fixture.source.id,
            fixture.owner.id,
            Uuid::new_v4(),
            1,
            3600,
        )
        .unwrap();
    let client = reqwest::Client::new();
    let before: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM review_publications), (SELECT count(*) FROM comment_index), (SELECT count(*) FROM comment_replies)")
        .fetch_one(&fixture.app.db).await.unwrap();
    for (method, suffix, body) in [
        (
            reqwest::Method::POST,
            "comments".to_string(),
            json!({"path":PER_EDIT_DOC,"line":1,"content":"Retired"}),
        ),
        (
            reqwest::Method::POST,
            "publish-findings".to_string(),
            json!({"summary":"Retired","findings":[]}),
        ),
        (
            reqwest::Method::GET,
            format!(
                "rounds/{}",
                fixture.parent["publicationId"].as_str().unwrap()
            ),
            json!(null),
        ),
        (
            reqwest::Method::POST,
            format!(
                "suggestions/{}/decide",
                fixture.parent["id"].as_str().unwrap()
            ),
            json!({"action":"reply","content":"Retired"}),
        ),
    ] {
        let response = client
            .request(
                method,
                format!(
                    "{}/api/internal/workspaces/{}/review/{suffix}",
                    fixture.app.base_url, fixture.source.id
                ),
            )
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap();
        // Unknown internal paths are refused by authentication before routing.
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{suffix}");
    }
    let after: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM review_publications), (SELECT count(*) FROM comment_index), (SELECT count(*) FROM comment_replies)")
        .fetch_one(&fixture.app.db).await.unwrap();
    assert_eq!(before, after);
    fixture.cleanup().await;
}

/// Existing open PR for the review-copy integration tests. PR publication
/// itself is exercised through the public API in api_workspace_review_flow.
async fn seed_open_review_pr(
    app: &TestApp,
    source: &TestWorkspace,
    target: &TestWorkspace,
    owner: Uuid,
) {
    sqlx::query(
        "INSERT INTO workspace_pull_requests \
         (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status, \
          provider_status, title, last_preflight_fingerprint, created_by) \
         VALUES ($1, $2, $3, $4, 'open', 'ready', 'local_only', 'Review copy test', 'review-test', $5)",
    )
    .bind(Uuid::new_v4()).bind(source.org_id).bind(source.id).bind(target.id).bind(owner)
    .execute(&app.db).await.expect("existing open source PR");
}

const PER_EDIT_DOC: &str = "docs/per-edit-review.md";

/// A real published text change. The runtime peer scripts the guest transport;
/// publication, durable child decisions and thread storage use the application.
struct PerEditReview {
    app: TestApp,
    owner: TestUser,
    reviewer: TestUser,
    source: TestWorkspace,
    parent: Value,
    base_head: String,
    review_workspace_id: Uuid,
    _review_peer: TestWorkspaceRuntimePeer,
}

impl PerEditReview {
    async fn cleanup(self) {
        drop(self._review_peer);
        self.app.cleanup().await;
    }
}

async fn publish_per_edit_doc(before: &str, after: &str) -> PerEditReview {
    publish_per_edit_doc_with_count(before, after, 2).await
}

async fn publish_per_edit_doc_with_count(
    before: &str,
    after: &str,
    expected_edits: usize,
) -> PerEditReview {
    init_tracing();
    let app = TestApp::spawn().await;
    let owned_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&app.db)
        .await
        .unwrap();
    eprintln!("PER_EDIT_TEST_DATABASE={owned_database}");
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET docs_repo_id = $1 WHERE id = $2")
        .bind(source.repo_id)
        .bind(source.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE workspace_repos SET role = 'docs' WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(source.id)
    .bind(source.repo_id)
    .execute(&app.db)
    .await
    .unwrap();
    let repo_path = app.repo_path(source.repo_id).await;
    let base_head = commit_file_on(
        &repo_path,
        &source.head_commit,
        PER_EDIT_DOC,
        before.as_bytes(),
        "owner: create project doc",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &base_head).await;
    let reviewer = create_member(&app, &org).await;
    let client = app.authenticated_client(&reviewer);
    let (status, started) = start_review(&client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let review_workspace_id = Uuid::parse_str(
        data(&started)["review"]["reviewWorkspaceId"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let (copy_id, copy_head) = review_branch(&app, review_workspace_id, source.repo_id).await;
    assert_eq!(copy_head, base_head);
    let review_head = commit_file_on(
        &repo_path,
        &copy_head,
        PER_EDIT_DOC,
        after.as_bytes(),
        "reviewer: separate document edits",
    );
    helpers::seed_workspace_authority_head(&app, copy_id, source.repo_id, &review_head).await;
    let review_peer = attach_copy_peer(
        &app,
        review_workspace_id,
        source.repo_id,
        reviewer.id,
        &review_head,
    )
    .await;
    let (status, published) = post(
        &client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({
            "summary": "Independent edits",
            "verdict": "request_changes",
            "notes": {"docs/per-edit-review.md": "Please consider each change separately."}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{published}");
    let parents: Vec<&Value> = data(&published)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["path"] == PER_EDIT_DOC && row["kind"] == "hunk")
        .collect();
    assert_eq!(
        parents.len(),
        1,
        "the two edits deliberately occupy one Git hunk: {published}"
    );
    let parent = parents[0].clone();
    assert_eq!(
        parent["edits"].as_array().unwrap().len(),
        expected_edits,
        "one authoritative identity per discrete edit: {parent}"
    );
    let ids: Vec<&str> = parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| edit["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        expected_edits
    );
    assert!(ids.iter().all(|id| Uuid::parse_str(id).is_ok()), "{ids:?}");
    PerEditReview {
        app,
        owner,
        reviewer,
        source,
        parent,
        base_head,
        review_workspace_id,
        _review_peer: review_peer,
    }
}

fn document_edit<'a>(parent: &'a Value, proposed: &str) -> &'a Value {
    parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|edit| {
            edit["afterText"]
                .as_str()
                .unwrap_or_default()
                .contains(proposed)
        })
        .unwrap_or_else(|| panic!("missing edit {proposed:?}: {parent}"))
}

fn document_edit_status<'a>(parent: &'a Value, id: &str) -> &'a str {
    parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|edit| edit["id"] == id)
        .unwrap_or_else(|| panic!("missing edit {id}: {parent}"))["status"]
        .as_str()
        .unwrap()
}

fn per_edit_route(fixture: &PerEditReview, action: &str) -> String {
    format!(
        "/api/v1/workspaces/{}/review/suggestions/{}/{action}",
        fixture.source.id,
        fixture.parent["id"].as_str().unwrap()
    )
}

async fn read_current_per_edit_parent(fixture: &PerEditReview) -> Value {
    let client = fixture.app.authenticated_client(&fixture.owner);
    let (status, listed) = get(
        &client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?currentOnly=true",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    data(&listed)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == fixture.parent["id"])
        .unwrap_or_else(|| panic!("pending edit disappeared after source-head advance: {listed}"))
        .clone()
}

async fn seed_per_edit_checkpoint(fixture: &PerEditReview, content: &str) {
    // This test peer's changes RPC is scripted, not receive-pack. Explicitly
    // move the authoritative Git ref to challenge currentOnly and tree badges.
    let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
    let head = commit_file_on(
        &repo_path,
        &fixture.base_head,
        PER_EDIT_DOC,
        content.as_bytes(),
        "owner: checkpoint accepted edit",
    );
    helpers::seed_workspace_authority_head(
        &fixture.app,
        fixture.source.id,
        fixture.source.repo_id,
        &head,
    )
    .await;
}

/// Put the review copy on the actual owner checkpoint, then exercise both
/// publication consumers. No parent/child metadata is patched in this helper.
async fn continue_per_edit_review(fixture: &PerEditReview, proposed: &str) -> (Value, Value) {
    let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
    let (_, source_head) =
        review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
    let review_head = commit_file_on(
        &repo_path,
        &source_head,
        PER_EDIT_DOC,
        proposed.as_bytes(),
        "reviewer: continue against owner checkpoint",
    );
    helpers::seed_workspace_authority_head(
        &fixture.app,
        fixture.review_workspace_id,
        fixture.source.repo_id,
        &review_head,
    )
    .await;
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let (status, published) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/publish",
            fixture.review_workspace_id
        ),
        json!({"summary":"Continue pending document edits", "verdict":"request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{published}");
    let parent = data(&published)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == PER_EDIT_DOC && row["kind"] == "hunk")
        .unwrap()
        .clone();
    (data(&preview).clone(), parent)
}

fn parent_edit<'a>(parent: &'a Value, id: &str) -> &'a Value {
    parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|edit| edit["id"] == id)
        .unwrap_or_else(|| panic!("missing durable child {id}: {parent}"))
}

async fn ordered_document_continuation_case(identical: bool, restored: bool) {
    {
        let before = "The red fox.\nBridge one.\nBridge two.\nBridge three.\nBridge four.\n";
        let second = if identical {
            "Same added block."
        } else {
            "Second added block."
        };
        let first = if identical {
            "Same added block."
        } else {
            "First added block."
        };
        let after = format!("{}\n{first}\n\n{second}\n", before.replace("red", "blue"));
        let fixture = publish_per_edit_doc_with_count(before, &after, 3).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let edits = fixture.parent["edits"].as_array().unwrap();
        let replacement = document_edit(&fixture.parent, "blue")["id"]
            .as_str()
            .unwrap();
        let additions: Vec<_> = edits
            .iter()
            .filter(|edit| edit["beforeText"] == "")
            .collect();
        assert_eq!(additions.len(), 2);
        let rejected = additions[0]["id"].as_str().unwrap();
        let pending = additions[1]["id"].as_str().unwrap();
        for (id, text) in [
            (rejected, "Retain the rejected addition's discussion."),
            (pending, "Keep the pending addition's discussion."),
        ] {
            let (status, reply) = post(
                &owner,
                &per_edit_route(&fixture, "replies"),
                json!({"editId":id,"content":text}),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{reply}");
        }
        let (status, rejected_result) = post(
            &owner,
            &per_edit_route(&fixture, "reject"),
            json!({"editId":rejected,"reason":"Only this paragraph is rejected."}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{rejected_result}");
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":replacement}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let prior = data(&accepted)["suggestion"].clone();
        let canonical = if restored {
            before.to_string()
        } else {
            before.replace("red", "blue")
        };
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &canonical)
            .await;
        seed_per_edit_checkpoint(&fixture, &canonical).await;
        let proposed = format!("{canonical}\n{first}\n\n{second}\n");
        let (preview, successor) = continue_per_edit_review(&fixture, &proposed).await;
        let change = &preview["files"][0]["changes"][0];
        assert_eq!(change["editCount"], 2, "{preview}");
        assert_eq!(
            change["carriedEditCount"], 2,
            "complete proven ordered units must carry: {preview}"
        );
        assert_eq!(change["carriedOpenEditCount"], 1, "{preview}");
        assert_ne!(
            successor["anchorStart"], fixture.parent["anchorStart"],
            "a real smaller Git hunk must bypass whole-parent carry"
        );
        assert_eq!(successor["edits"][0]["id"], rejected);
        assert_eq!(successor["edits"][1]["id"], pending);
        for id in [rejected, pending] {
            let old = parent_edit(&prior, id);
            let new = parent_edit(&successor, id);
            for field in ["status", "statusBy", "statusAt", "commentId"] {
                assert_eq!(new[field], old[field], "{field}: {successor}");
            }
            let (status, replies) = get(
                &owner,
                &format!(
                    "/api/v1/workspaces/{}/review/suggestions/{}/replies?editId={id}",
                    fixture.source.id,
                    successor["id"].as_str().unwrap()
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{replies}");
            assert!(replies.to_string().contains("addition's discussion"));
        }
        assert_eq!(document_edit_status(&successor, rejected), "rejected");
        assert_eq!(document_edit_status(&successor, pending), "open");
        let (status, listed) = get(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions?openOnly=true",
                fixture.source.id
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(
            data(&listed)["suggestions"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row["edits"].as_array().unwrap())
                .filter(|edit| edit["status"] == "open")
                .count(),
            1,
            "{listed}"
        );
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_ordered_identical_continuation_preserves_decisions() {
    ordered_document_continuation_case(true, false).await;
}

#[tokio::test]
async fn document_edit_distinct_continuation_preserves_decisions() {
    ordered_document_continuation_case(false, false).await;
}

#[tokio::test]
async fn document_edit_restored_continuation_preserves_decisions() {
    ordered_document_continuation_case(false, true).await;
}

#[tokio::test]
async fn document_edit_changed_identical_group_does_not_guess_prior_decisions() {
    let before = "The red fox.\nBridge one.\nBridge two.\nBridge three.\nBridge four.\n";
    let after = format!(
        "{}\nSame added block.\n\nSame added block.\n",
        before.replace("red", "blue")
    );
    let fixture = publish_per_edit_doc_with_count(before, &after, 3).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let addition = &fixture.parent["edits"][1];
    let (status, rejected) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId":addition["id"],"reason":"Reject only the first copy."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":document_edit(&fixture.parent,"blue")["id"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let canonical = before.replace("red", "blue");
    seed_per_edit_checkpoint(&fixture, &canonical).await;
    let proposed = format!("{canonical}\nSame added block.\n");
    let (preview, successor) = continue_per_edit_review(&fixture, &proposed).await;
    assert_eq!(
        preview["files"][0]["changes"][0]["carriedEditCount"], 0,
        "one copy cannot establish which of two prior copies remains: {preview}"
    );
    assert_eq!(successor["edits"][0]["status"], "open");
    assert_ne!(
        successor["edits"][0]["id"],
        fixture.parent["edits"][1]["id"]
    );
    assert_ne!(
        successor["edits"][0]["id"],
        fixture.parent["edits"][2]["id"]
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_distinct_remaining_subset_keeps_its_unique_identity_and_discussion() {
    let before = "The red fox.\nBridge one.\nBridge two.\nBridge three.\nBridge four.\n";
    let after = format!(
        "{}\nFirst added block.\n\nSecond added block.\n",
        before.replace("red", "blue")
    );
    let fixture = publish_per_edit_doc_with_count(before, &after, 3).await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let pending = document_edit(&fixture.parent, "Second added block.")["id"]
        .as_str()
        .unwrap();
    let (status, reply) = post(
        &owner,
        &per_edit_route(&fixture, "replies"),
        json!({"editId":pending,"content":"Retain this uniquely distinct remaining edit."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let (status,rejected) = post(&owner,&per_edit_route(&fixture,"reject"),json!({"editId":document_edit(&fixture.parent,"First added block.")["id"],"reason":"Reviewer will remove this block."})).await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":document_edit(&fixture.parent,"blue")["id"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let old = parent_edit(&data(&accepted)["suggestion"], pending).clone();
    let canonical = before.replace("red", "blue");
    seed_per_edit_checkpoint(&fixture, &canonical).await;
    let (preview, successor) =
        continue_per_edit_review(&fixture, &format!("{canonical}\nSecond added block.\n")).await;
    assert_eq!(
        preview["files"][0]["changes"][0]["carriedEditCount"], 1,
        "{preview}"
    );
    let retained = parent_edit(&successor, pending);
    assert_eq!(retained["status"], "open");
    assert_eq!(retained["commentId"], old["commentId"]);
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_middle_blocks_remain_separate_for_partial_and_edited_acceptance() {
    for mode in [
        "first-only",
        "second-only",
        "first-then-second",
        "second-then-first",
        "edited-first",
    ] {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let proposed = "Intro.\n\nFirst added block.\n\nSecond added block.\n\nQuiet.\n\nTail.\n";
        let fixture = publish_per_edit_doc(before, proposed).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let first = document_edit(&fixture.parent, "First added block.")["id"]
            .as_str()
            .unwrap();
        let second = document_edit(&fixture.parent, "Second added block.")["id"]
            .as_str()
            .unwrap();
        let order: Vec<_> = match mode {
            "first-only" | "edited-first" => vec![first],
            "second-only" => vec![second],
            "first-then-second" => vec![first, second],
            _ => vec![second, first],
        };
        for id in &order {
            let body = if mode == "edited-first" {
                json!({"editId":id,"afterLines":"Edited first block.\n\n"})
            } else {
                json!({"editId":id})
            };
            let (status, accepted) = post(&owner, &per_edit_route(&fixture, "accept"), body).await;
            assert_eq!(status, StatusCode::OK, "{accepted}");
        }
        if order.len() == 1 {
            let rejected_id = if order[0] == first { second } else { first };
            let (status, rejected) = post(
                &owner,
                &per_edit_route(&fixture, "reject"),
                json!({"editId":rejected_id,"reason":"Only accept the other block."}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{rejected}");
        }
        let expected = match mode {
            "first-only" => "Intro.\n\nFirst added block.\n\nQuiet.\n\nTail.\n",
            "second-only" => "Intro.\n\nSecond added block.\n\nQuiet.\n\nTail.\n",
            "edited-first" => "Intro.\n\nEdited first block.\n\nQuiet.\n\nTail.\n",
            _ => proposed,
        };
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected),
            "a selected block must retain its canonical paragraph boundary: {mode}"
        );
        seed_per_edit_checkpoint(&fixture, expected).await;
        let reloaded = read_current_per_edit_parent(&fixture).await;
        for id in [first, second] {
            assert_eq!(
                document_edit_status(&reloaded, id),
                if order.contains(&id) {
                    "accepted"
                } else {
                    "rejected"
                }
            );
        }
        assert_eq!(reloaded["expectedBeforeLines"], expected);
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_accepted_continuation_context_maps_to_original_capture() {
    for (identical, reverse) in [(false, false), (false, true), (true, false), (true, true)] {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let first = if identical {
            "Same added block."
        } else {
            "First added block."
        };
        let second = if identical {
            "Same added block."
        } else {
            "Second added block."
        };
        let proposed = format!("Intro.\n\n{first}\n\n{second}\n\nQuiet.\n\nTail.\n");
        let fixture = publish_per_edit_doc(before, &proposed).await;
        let (other, other_peer) =
            publish_competing_document_proposal(&fixture, "Intro.\n\nSleepy.\n\nTail.\n").await;
        let independent = document_edit(&other, "Sleepy")["id"].as_str().unwrap();
        let additions = fixture.parent["edits"].as_array().unwrap();
        let chosen = if reverse { 1 } else { 0 };
        let pending = 1 - chosen;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":additions[chosen]["id"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let chosen_text = if chosen == 0 { first } else { second };
        let partial = format!("Intro.\n\n{chosen_text}\n\nQuiet.\n\nTail.\n");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(partial.as_str())
        );
        seed_per_edit_checkpoint(&fixture, &partial).await;
        let (preview, successor) = continue_per_edit_review(&fixture, &proposed).await;
        assert_eq!(
            preview["files"][0]["changes"][0]["carriedEditCount"], 1,
            "{preview}"
        );
        assert_ne!(
            successor["contentHash"], fixture.parent["contentHash"],
            "this regression requires a new capture hash"
        );
        assert_eq!(
            successor["edits"][0]["id"], additions[pending]["id"],
            "{successor}"
        );
        let successor_id = successor["id"].as_str().unwrap();
        let (status, settled) = post(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{successor_id}/accept",
                fixture.source.id
            ),
            json!({"editId":additions[pending]["id"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{settled}");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(proposed.as_str())
        );
        seed_per_edit_checkpoint(&fixture, &proposed).await;
        let url = format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            other["id"].as_str().unwrap()
        );
        if !identical && !reverse {
            // Inject damaged continuation metadata after a real publication
            // and acceptance. Neither a foreign UUID nor changed proposal
            // meaning can borrow the old source coordinates to write B.
            let successor_uuid = Uuid::parse_str(successor_id).unwrap();
            let saved: Value =
                sqlx::query_scalar("SELECT edits FROM review_suggestions WHERE id=$1")
                    .bind(successor_uuid)
                    .fetch_one(&fixture.app.db)
                    .await
                    .unwrap();
            let original_uuid = Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap();
            let old_saved: Value =
                sqlx::query_scalar("SELECT edits FROM review_suggestions WHERE id=$1")
                    .bind(original_uuid)
                    .fetch_one(&fixture.app.db)
                    .await
                    .unwrap();
            assert_eq!(old_saved[pending]["status"], "superseded");
            let writes = peer
                .received_rpc_methods()
                .await
                .iter()
                .filter(|method| method.as_str() == "fs.write")
                .count();
            for fault in [
                "unrelated-identity",
                "changed-proposal",
                "contradictory-replacement",
            ] {
                let mut damaged = saved.clone();
                if fault == "unrelated-identity" {
                    damaged[0]["id"] = json!(Uuid::new_v4().to_string());
                }
                if fault == "changed-proposal" {
                    damaged[0]["afterText"] = json!("Different proposal.\n\n");
                }
                sqlx::query("UPDATE review_suggestions SET edits=$2 WHERE id=$1")
                    .bind(successor_uuid)
                    .bind(&damaged)
                    .execute(&fixture.app.db)
                    .await
                    .unwrap();
                if fault == "contradictory-replacement" {
                    let mut contradictory = old_saved.clone();
                    contradictory[pending]["status"] = json!("accepted");
                    contradictory[pending]["acceptedAfterText"] =
                        json!("Contradictory replacement.\n\n");
                    contradictory[pending]["writtenHash"] = saved[0]["writtenHash"].clone();
                    sqlx::query("UPDATE review_suggestions SET edits=$2 WHERE id=$1")
                        .bind(original_uuid)
                        .bind(&contradictory)
                        .execute(&fixture.app.db)
                        .await
                        .unwrap();
                }
                let (status, refused) = post(&owner, &url, json!({"editId":independent})).await;
                assert_eq!(
                    status,
                    StatusCode::CONFLICT,
                    "{fault} must not manufacture applicable cross-capture context: {refused}"
                );
                assert_eq!(
                    peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                        .await
                        .as_deref(),
                    Some(proposed.as_str())
                );
                assert_eq!(
                    peer.received_rpc_methods()
                        .await
                        .iter()
                        .filter(|method| method.as_str() == "fs.write")
                        .count(),
                    writes
                );
                sqlx::query("UPDATE review_suggestions SET edits=$2 WHERE id=$1")
                    .bind(successor_uuid)
                    .bind(&saved)
                    .execute(&fixture.app.db)
                    .await
                    .unwrap();
                sqlx::query("UPDATE review_suggestions SET edits=$2 WHERE id=$1")
                    .bind(original_uuid)
                    .bind(&old_saved)
                    .execute(&fixture.app.db)
                    .await
                    .unwrap();
            }
        }
        let (status, accepted) = post(&owner, &url, json!({"editId":independent})).await;
        assert_eq!(status,StatusCode::OK,"an accepted carried child must remain context for an older independent capture: {accepted}");
        let expected = proposed.replacen("Quiet.", "Sleepy.", 1);
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], independent),
            "accepted"
        );
        drop(peer);
        drop(other_peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_identical_continuation_keeps_exact_proof_and_refuses_ambiguous_groups() {
    for mode in ["expanded", "owner-override", "restored"] {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let proposed = "Intro.\n\nSame added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
        let fixture = publish_per_edit_doc(before, proposed).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let ids = fixture.parent["edits"].as_array().unwrap();
        let body = if mode == "owner-override" {
            json!({"editId":ids[0]["id"],"afterLines":"Same added block.\n\nSame added block.\n\n"})
        } else {
            json!({"editId":ids[0]["id"]})
        };
        let (status, accepted) = post(&owner, &per_edit_route(&fixture, "accept"), body).await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let canonical = match mode {
            "owner-override" => proposed,
            "restored" => before,
            _ => "Intro.\n\nSame added block.\n\nQuiet.\n\nTail.\n",
        };
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, canonical)
            .await;
        seed_per_edit_checkpoint(&fixture, canonical).await;
        let continued = if mode == "restored" {
            "Intro.\n\nSame added block.\n\nQuiet.\n\nTail.\n"
        } else {
            "Intro.\n\nSame added block.\n\nSame added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n"
        };
        let (preview, successor) = continue_per_edit_review(&fixture, continued).await;
        if mode == "owner-override" {
            // The edited A1 contains two copies; A2 is still exactly before
            // Quiet. Direct correspondence requires no boundary equivalence.
            let old_point = data(&accepted)["suggestion"]["edits"][1]["currentStart"]
                .as_u64()
                .unwrap() as usize;
            let base: usize = canonical
                .split_inclusive('\n')
                .take(successor["anchorStart"].as_u64().unwrap() as usize - 1)
                .map(str::len)
                .sum();
            let next_point = base + successor["edits"][0]["start"].as_u64().unwrap() as usize;
            assert_eq!(old_point, canonical.find("Quiet.").unwrap());
            assert_eq!(
                next_point, old_point,
                "the new hunk must prove the exact pending source point"
            );
            assert_eq!(successor["edits"][0]["beforeText"], ids[1]["beforeText"]);
            assert_eq!(successor["edits"][0]["afterText"], ids[1]["afterText"]);
            assert_eq!(successor["edits"][0]["id"], ids[1]["id"]);
            assert_eq!(preview["files"][0]["changes"][0]["carriedEditCount"], 1);
            drop(peer);
            fixture.cleanup().await;
            continue;
        }
        assert_eq!(
            preview["files"][0]["changes"][0]["carriedEditCount"], 0,
            "{mode} does not prove an unchanged pending identical group: {preview}"
        );
        for next in successor["edits"].as_array().unwrap() {
            assert!(
                ids.iter().all(|old| old["id"] != next["id"]),
                "{mode} must not guess an identity: {successor}"
            );
            assert_eq!(next["status"], "open");
        }
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(canonical)
        );
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_continuation_uses_exact_captured_blob_receipt_witness() {
    let before =
        "The red fox.\nBridge one.\nBridge two.\nBridge three.\nBridge four.\nThe quiet dog.\n";
    let fixture = publish_per_edit_doc(
        before,
        &before.replace("red", "blue").replace("quiet", "sleepy"),
    )
    .await;
    let (old, old_peer) =
        publish_competing_document_proposal(&fixture, &before.replace("red", "green")).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let pending = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let (status, reply) = post(
        &owner,
        &per_edit_route(&fixture, "replies"),
        json!({"editId":pending,"content":"Preserve my independent quiet-word discussion."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let (status, accepted) = post(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            old["id"].as_str().unwrap()
        ),
        json!({"editId":document_edit(&old,"green")["id"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    seed_per_edit_checkpoint(&fixture, before).await;
    let (new, new_peer) =
        publish_competing_document_proposal(&fixture, &before.replace("red", "violet")).await;
    let (status, accepted) = post(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            new["id"].as_str().unwrap()
        ),
        json!({"editId":document_edit(&new,"violet")["id"],"afterLines":"purple"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(
        data(&accepted)["writtenHash"].as_str().unwrap().len(),
        40,
        "fixture version is a SHA1 Git blob OID, not the 64-byte raw digest"
    );
    let prior = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&prior, pending), "open");
    assert_eq!(
        prior["expectedBeforeLines"], before,
        "metadata-only reads cannot choose between contradictory accepted histories"
    );
    let canonical = before.replace("red", "purple");
    seed_per_edit_checkpoint(&fixture, &canonical).await;
    let continued = canonical.replace("quiet", "sleepy");
    let (preview, successor) = continue_per_edit_review(&fixture, &continued).await;
    assert_eq!(
        preview["files"][0]["changes"][0]["carriedEditCount"], 1,
        "the exact captured Git blob OID can establish the current accepted receipt: {preview}"
    );
    assert_eq!(preview["files"][0]["changes"][0]["carriedOpenEditCount"], 1);
    let retained = parent_edit(&successor, pending);
    assert_eq!(
        retained["commentId"],
        parent_edit(&prior, pending)["commentId"]
    );
    let (status, accepted) = post(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            successor["id"].as_str().unwrap()
        ),
        json!({"editId":pending}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(continued.as_str())
    );
    drop(peer);
    drop(old_peer);
    drop(new_peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_carried_identical_insertion_keeps_edited_original_group_order() {
    for reverse in [false, true] {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let proposed = "Intro.\n\nSame added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
        let fixture = publish_per_edit_doc(before, proposed).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let chosen = usize::from(reverse);
        let pending = 1 - chosen;
        let ids = fixture.parent["edits"].as_array().unwrap();
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":ids[chosen]["id"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let partial = "Intro.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
        seed_per_edit_checkpoint(&fixture, partial).await;
        let (preview, successor) = continue_per_edit_review(&fixture, proposed).await;
        assert_eq!(
            preview["files"][0]["changes"][0]["carriedEditCount"], 1,
            "{preview}"
        );
        assert_eq!(successor["edits"][0]["id"], ids[pending]["id"]);
        let retained_start = successor["edits"][0]["start"].clone();
        let retained_end = successor["edits"][0]["end"].clone();
        let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
        let (_, source_head) =
            review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
        let advanced_head = commit_file_on(
            &repo_path,
            &source_head,
            "docs/unrelated-checkpoint.md",
            b"Unrelated owner checkpoint.\n",
            "owner: advance another document",
        );
        assert_ne!(advanced_head, source_head);
        helpers::seed_workspace_authority_head(
            &fixture.app,
            fixture.source.id,
            fixture.source.repo_id,
            &advanced_head,
        )
        .await;
        let (again_preview, again) = continue_per_edit_review(&fixture, proposed).await;
        assert_ne!(again["id"], successor["id"]);
        assert_eq!(
            again_preview["files"][0]["changes"][0]["carriedEditCount"], 1,
            "unchanged publication after an unrelated source-head move keeps the carried unit: {again_preview}"
        );
        assert_eq!(again["edits"][0]["id"], ids[pending]["id"]);
        assert_eq!(again["edits"][0]["start"], retained_start);
        assert_eq!(again["edits"][0]["end"], retained_end);
        let (status, accepted) = post(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{}/accept",
                fixture.source.id,
                again["id"].as_str().unwrap()
            ),
            json!({"editId":ids[pending]["id"],"afterLines":"Different added block.\n\n"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let expected = if reverse {
            "Intro.\n\nDifferent added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n"
        } else {
            "Intro.\n\nSame added block.\n\nDifferent added block.\n\nQuiet.\n\nTail.\n"
        };
        assert_eq!(peer.file_content(fixture.source.repo_id,PER_EDIT_DOC).await.as_deref(),Some(expected),"retaining a child identity must retain its original position when its replacement is edited: reverse={reverse}");
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_equivalent_group_carry_preserves_the_entire_new_proposal() {
    let before = "Intro.\n\nQuiet.\n\nTail.\n";
    let proposed = "Intro.\n\nSame added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
    let fixture = publish_per_edit_doc(before, proposed).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let pending = fixture.parent["edits"][0]["id"].clone();
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":fixture.parent["edits"][1]["id"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let partial = "Intro.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
    seed_per_edit_checkpoint(&fixture, partial).await;
    // A new change to the accepted canonical block makes the previously
    // equivalent insertion points produce different complete proposals.
    let continued = "Intro.\n\nOther added block.\n\nSame added block.\n\nQuiet.\n\nTail.\n";
    let (preview, successor) = continue_per_edit_review(&fixture, continued).await;
    assert_eq!(
        preview["files"][0]["changes"][0]["carriedEditCount"], 0,
        "a changed proposal cannot guess A1's identity from nearby identical text: {preview}"
    );
    assert!(successor["edits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|edit| edit["id"] != pending));
    let (status, accepted) = post(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            successor["id"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(continued)
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn project_doc_edits_keep_mixed_decisions_discussions_and_source_head_visibility() {
    let before = "new line from reviewer.\n";
    let after = "suggest to add this line from reviewer.\n\none more thing....\n";
    let fixture = publish_per_edit_doc(before, after).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let replacement = document_edit(&fixture.parent, "suggest to add this")["id"]
        .as_str()
        .unwrap();
    let addition = document_edit(&fixture.parent, "one more thing")["id"]
        .as_str()
        .unwrap();
    for (id, text) in [
        (replacement, "Only discuss the phrase."),
        (addition, "Only discuss the added block."),
    ] {
        let (status, reply) = post(
            &owner,
            &per_edit_route(&fixture, "replies"),
            json!({"editId": id, "content": text}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{reply}");
    }
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": replacement}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(data(&accepted)["suggestion"]["status"], "open");
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], replacement),
        "accepted"
    );
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], addition),
        "open"
    );
    let partial = "suggest to add this line from reviewer.\n";
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial)
    );
    seed_per_edit_checkpoint(&fixture, partial).await;
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&current, addition), "open");
    let (status, counts) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/counts",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{counts}");
    let count = data(&counts)["paths"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == PER_EDIT_DOC)
        .expect("remaining edit keeps its badge");
    assert_eq!(count["openCount"], 1, "{counts}");
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let change = data(&preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == PER_EDIT_DOC)
        .unwrap()["changes"][0]
        .clone();
    assert_eq!(change["editCount"], 2, "{preview}");
    assert_eq!(change["carriedEditCount"], 2, "{preview}");
    assert_eq!(change["carriedOpenEditCount"], 1, "{preview}");
    let (status, republished) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/publish",
            fixture.review_workspace_id
        ),
        json!({"summary": "Repeat the same review after one decision", "verdict": "request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{republished}");
    let (status, carried) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=true",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{carried}");
    let open_parents: Vec<&Value> = data(&carried)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["path"] == PER_EDIT_DOC)
        .collect();
    assert_eq!(
        open_parents.len(),
        1,
        "repeated Push review must not duplicate pending work: {carried}"
    );
    assert_eq!(
        document_edit_status(open_parents[0], replacement),
        "accepted"
    );
    assert_eq!(document_edit_status(open_parents[0], addition), "open");
    let current_parent_id = open_parents[0]["id"].as_str().unwrap();
    assert_eq!(open_parents[0]["supersedes"], fixture.parent["id"]);
    assert_eq!(
        open_parents[0]["authorLabel"],
        fixture.parent["authorLabel"]
    );
    let (status, history) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=false",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    let retired = data(&history)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parent| parent["id"] == fixture.parent["id"])
        .unwrap();
    assert_eq!(retired["status"], "superseded");
    assert_eq!(document_edit_status(retired, replacement), "accepted");
    assert_eq!(document_edit_status(retired, addition), "superseded");
    let (status, summary) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/publications",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    let summaries = data(&summary)["publications"].as_array().unwrap();
    let old_round = summaries
        .iter()
        .find(|round| round["id"] == fixture.parent["publicationId"])
        .unwrap();
    assert_eq!(old_round["acceptedCount"], 1, "{summary}");
    assert_eq!(old_round["supersededCount"], 1, "{summary}");
    assert_eq!(old_round["openCount"], 0, "{summary}");
    let current_round = summaries
        .iter()
        .find(|round| round["id"] == open_parents[0]["publicationId"])
        .unwrap();
    assert_eq!(current_round["acceptedCount"], 1, "{summary}");
    assert_eq!(current_round["openCount"], 1, "{summary}");
    assert_eq!(current_round["supersededCount"], 0, "{summary}");
    // A source-head advance creates a successor publication under the
    // existing review lifecycle. Child identity and discussion carry, while
    // mutations address the current parent's authority after refreshing.
    let (status, stale_decision) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId": addition, "reason": "A stale parent cannot decide its successor."}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_decision}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial)
    );
    let (status, rejected) = post(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{current_parent_id}/reject",
            fixture.source.id
        ),
        json!({"editId": addition, "reason": "Keep the original paragraph boundary."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(data(&rejected)["suggestion"]["status"], "partial");
    assert_eq!(
        document_edit_status(&data(&rejected)["suggestion"], replacement),
        "accepted"
    );
    assert_eq!(
        document_edit_status(&data(&rejected)["suggestion"], addition),
        "rejected"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial),
        "reject leaves canonical content untouched"
    );
    for (id, own, other) in [
        (
            replacement,
            "Only discuss the phrase.",
            "Only discuss the added block.",
        ),
        (
            addition,
            "Only discuss the added block.",
            "Only discuss the phrase.",
        ),
    ] {
        let (status, thread) = get(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{current_parent_id}/replies?editId={id}",
                fixture.source.id
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{thread}");
        assert!(thread.to_string().contains(own), "{thread}");
        assert!(
            !thread.to_string().contains(other),
            "child discussions stay independent: {thread}"
        );
    }
    let (status, reloaded) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reloaded}");
    let parent = data(&reloaded)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == current_parent_id)
        .unwrap();
    assert_eq!(parent["status"], "partial");
    assert_eq!(document_edit_status(parent, replacement), "accepted");
    assert_eq!(document_edit_status(parent, addition), "rejected");
    drop(peer);
    fixture.cleanup().await;
}

/// Another actual reviewer publication against this fixture's original source.
async fn publish_competing_document_proposal(
    fixture: &PerEditReview,
    after: &str,
) -> (Value, TestWorkspaceRuntimePeer) {
    let org = TestOrg {
        id: fixture.source.org_id,
        name: String::new(),
        slug: String::new(),
    };
    let reviewer = create_member(&fixture.app, &org).await;
    let client = fixture.app.authenticated_client(&reviewer);
    let (_, source_head) =
        review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
    let (status, started) = start_review(&client, fixture.source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{started}");
    let workspace_id = Uuid::parse_str(
        data(&started)["review"]["reviewWorkspaceId"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let (copy_id, copy_head) =
        review_branch(&fixture.app, workspace_id, fixture.source.repo_id).await;
    assert_eq!(copy_head, source_head);
    let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
    let head = commit_file_on(
        &repo_path,
        &copy_head,
        PER_EDIT_DOC,
        after.as_bytes(),
        "competing reviewer: same source",
    );
    helpers::seed_workspace_authority_head(&fixture.app, copy_id, fixture.source.repo_id, &head)
        .await;
    let peer = attach_copy_peer(
        &fixture.app,
        workspace_id,
        fixture.source.repo_id,
        reviewer.id,
        &head,
    )
    .await;
    let (status, published) = post(
        &client,
        &format!("/api/v1/workspaces/{workspace_id}/review/publish"),
        json!({"summary":"Competing captured-source edit", "verdict":"request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{published}");
    let parent = data(&published)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == PER_EDIT_DOC && row["kind"] == "hunk")
        .unwrap()
        .clone();
    (parent, peer)
}

#[tokio::test]
async fn document_edit_cross_parent_winner_preserves_disjoint_sibling_applicability() {
    for winner in ["green", "emerald-colored"] {
        let before = "The red fox jumps over the quiet dog.\n";
        let fixture =
            publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
        let partial = format!("The {winner} fox jumps over the quiet dog.\n");
        let (other, other_peer) = publish_competing_document_proposal(&fixture, &partial).await;
        let winner_id = document_edit(&other, winner)["id"].as_str().unwrap();
        let losing_id = document_edit(&fixture.parent, "blue")["id"]
            .as_str()
            .unwrap();
        let sibling_id = document_edit(&fixture.parent, "sleepy")["id"]
            .as_str()
            .unwrap();
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let client = fixture.app.authenticated_client(&fixture.owner);
        let other_url = format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            other["id"].as_str().unwrap()
        );
        let (status, accepted) = post(&client, &other_url, json!({"editId":winner_id})).await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(partial.as_str())
        );
        seed_per_edit_checkpoint(&fixture, &partial).await;
        let current = read_current_per_edit_parent(&fixture).await;
        assert_eq!(document_edit_status(&current, losing_id), "obsolete");
        assert_eq!(document_edit_status(&current, sibling_id), "open");
        let writes_before = peer
            .received_rpc_methods()
            .await
            .iter()
            .filter(|m| m.as_str() == "fs.write")
            .count();
        let url = per_edit_route(&fixture, "accept");
        // Accepted context must not permit the owner's selected target to be
        // overwritten after an unrelated ordinary edit or a duplicated frame.
        for stale in [
            format!("The {winner} fox jumps over the loud dog.\n"),
            partial.repeat(2),
        ] {
            peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &stale)
                .await;
            let (status, refused) = post(&client, &url, json!({"editId":sibling_id})).await;
            assert_eq!(status, StatusCode::CONFLICT, "{refused}");
            assert_eq!(
                peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                    .await
                    .as_deref(),
                Some(stale.as_str())
            );
            assert_eq!(
                peer.received_rpc_methods()
                    .await
                    .iter()
                    .filter(|m| m.as_str() == "fs.write")
                    .count(),
                writes_before
            );
        }
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &partial)
            .await;
        let (status, accepted) = post(&client, &url, json!({"editId":sibling_id})).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "known accepted context must not invalidate disjoint Bquiet: {accepted}"
        );
        let complete = format!("The {winner} fox jumps over the sleepy dog.\n");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(complete.as_str())
        );
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], losing_id),
            "obsolete"
        );
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], sibling_id),
            "accepted"
        );
        assert_eq!(data(&accepted)["suggestion"]["status"], "partial");
        assert_eq!(current["expectedBeforeLines"], partial);
        let sibling = current["edits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == sibling_id)
            .unwrap();
        assert_eq!(
            &partial[sibling["currentStart"].as_u64().unwrap() as usize
                ..sibling["currentEnd"].as_u64().unwrap() as usize],
            "quiet"
        );
        let settled = read_current_per_edit_parent(&fixture).await;
        assert_eq!(settled["expectedBeforeLines"], complete);
        drop(peer);
        drop(other_peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_restored_captured_source_does_not_replay_historical_acceptance() {
    for action in ["selected-edit", "parent-edit", "fresh-selected-edit"] {
        let before = "The red fox jumps over the quiet dog.\n";
        let fixture =
            publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
        let (other, other_peer) = publish_competing_document_proposal(
            &fixture,
            "The emerald-colored fox jumps over the quiet dog.\n",
        )
        .await;
        let winner = document_edit(&other, "emerald-colored")["id"]
            .as_str()
            .unwrap();
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let winner_url = format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            other["id"].as_str().unwrap()
        );
        let (status, accepted) = post(&owner, &winner_url, json!({"editId":winner})).await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        // This is an ordinary owner revert, not a reversal of the durable decision.
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        seed_per_edit_checkpoint(&fixture, before).await;
        let sibling = document_edit(&fixture.parent, "sleepy")["id"]
            .as_str()
            .unwrap();
        let losing = document_edit(&fixture.parent, "blue")["id"]
            .as_str()
            .unwrap();
        let current = read_current_per_edit_parent(&fixture).await;
        assert_eq!(document_edit_status(&current, sibling), "open");
        assert_eq!(document_edit_status(&current, losing), "obsolete");
        let mut fresh_peer = None;
        let expected = if action == "fresh-selected-edit" {
            let (fresh, attached) = publish_competing_document_proposal(
                &fixture,
                "The violet fox jumps over the quiet dog.\n",
            )
            .await;
            fresh_peer = Some(attached);
            let edit = document_edit(&fresh, "violet")["id"].as_str().unwrap();
            let url = format!(
                "/api/v1/workspaces/{}/review/suggestions/{}/accept",
                fixture.source.id,
                fresh["id"].as_str().unwrap()
            );
            let (status, accepted) =
                post(&owner, &url, json!({"editId":edit,"afterLines":"purple"})).await;
            assert_eq!(status, StatusCode::OK,
                "historical accepted source must not block a fresh captured-source edit: {accepted}");
            "The purple fox jumps over the quiet dog.\n"
        } else {
            let body = if action == "selected-edit" {
                json!({"editId":sibling,"afterLines":"awake"})
            } else {
                json!({"afterLines":"The red fox jumps over the awake dog.\n"})
            };
            let (status, accepted) = post(&owner, &per_edit_route(&fixture, "accept"), body).await;
            assert_eq!(status, StatusCode::OK,
                "a restored exact captured basis must apply only the selected open delta: {accepted}");
            assert_eq!(
                document_edit_status(&data(&accepted)["suggestion"], losing),
                "obsolete"
            );
            assert_eq!(
                document_edit_status(&data(&accepted)["suggestion"], sibling),
                "accepted"
            );
            "The red fox jumps over the awake dog.\n"
        };
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected)
        );
        let writes = peer
            .received_rpc_methods()
            .await
            .iter()
            .filter(|m| m.as_str() == "fs.write")
            .count();
        let (status, duplicate) = post(&owner, &winner_url, json!({"editId":winner})).await;
        assert_eq!(status, StatusCode::OK, "{duplicate}");
        assert_eq!(data(&duplicate)["alreadyApplied"], true);
        assert_eq!(
            document_edit_status(&data(&duplicate)["suggestion"], winner),
            "accepted"
        );
        assert_eq!(
            peer.received_rpc_methods()
                .await
                .iter()
                .filter(|m| m.as_str() == "fs.write")
                .count(),
            writes
        );
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected)
        );
        drop(fresh_peer);
        drop(peer);
        drop(other_peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_cross_parent_context_keeps_ordered_same_point_added_blocks() {
    for (identical, reverse) in [(false, false), (false, true), (true, false), (true, true)] {
        let before = "The red fox jumps over the quiet dog.\n";
        let fixture =
            publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
        let added = if identical {
            "The red fox jumps over the quiet dog.\n\nSame added block.\n\nSame added block.\n"
        } else {
            "The red fox jumps over the quiet dog.\n\nFirst added block.\n\nSecond added block.\n"
        };
        let (other, other_peer) = publish_competing_document_proposal(&fixture, added).await;
        let edits = other["edits"].as_array().unwrap();
        assert_eq!(
            edits.len(),
            2,
            "distinct ordered added-block decisions: {other}"
        );
        assert_eq!(edits[0]["start"], edits[1]["start"]);
        assert_ne!(edits[0]["id"], edits[1]["id"]);
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let other_url = format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            other["id"].as_str().unwrap()
        );
        let order = if reverse { [1, 0] } else { [0, 1] };
        for index in order {
            let edit = &edits[index];
            let (status, accepted) = post(&owner, &other_url, json!({"editId":edit["id"]})).await;
            assert_eq!(status, StatusCode::OK, "{accepted}");
        }
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(added)
        );
        seed_per_edit_checkpoint(&fixture, added).await;
        let sibling = document_edit(&fixture.parent, "sleepy")["id"]
            .as_str()
            .unwrap();
        let writes = peer
            .received_rpc_methods()
            .await
            .iter()
            .filter(|m| m.as_str() == "fs.write")
            .count();
        let first_only = if identical {
            "The red fox jumps over the quiet dog.\n\nSame added block.\n"
        } else {
            "The red fox jumps over the quiet dog.\n\nFirst added block.\n"
        };
        for stale in [
            added.replacen("quiet", "loud", 1),
            added.repeat(2),
            first_only.to_string(),
        ] {
            peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &stale)
                .await;
            let (status, refused) = post(
                &owner,
                &per_edit_route(&fixture, "accept"),
                json!({"editId":sibling}),
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{refused}");
            assert_eq!(
                peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                    .await
                    .as_deref(),
                Some(stale.as_str())
            );
            assert_eq!(
                peer.received_rpc_methods()
                    .await
                    .iter()
                    .filter(|m| m.as_str() == "fs.write")
                    .count(),
                writes
            );
        }
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, added)
            .await;
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":sibling}),
        )
        .await;
        assert_eq!(status, StatusCode::OK,
            "same-parent ordered accepted blocks must remain source context for a disjoint edit: {accepted}");
        let expected = added.replacen("quiet", "sleepy", 1);
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected.as_str())
        );
        let settled = read_current_per_edit_parent(&fixture).await;
        assert_eq!(settled["expectedBeforeLines"], expected);
        assert_eq!(document_edit_status(&settled, sibling), "accepted");
        drop(peer);
        drop(other_peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_current_written_receipt_proves_restored_source_new_winner() {
    let before = "The red fox jumps over the quiet dog.\n";
    let fixture = publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
    let (old_winner, old_peer) =
        publish_competing_document_proposal(&fixture, "The green fox jumps over the quiet dog.\n")
            .await;
    let old_id = document_edit(&old_winner, "green")["id"].as_str().unwrap();
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let old_url = format!(
        "/api/v1/workspaces/{}/review/suggestions/{}/accept",
        fixture.source.id,
        old_winner["id"].as_str().unwrap()
    );
    let (status, accepted) = post(&owner, &old_url, json!({"editId":old_id})).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    seed_per_edit_checkpoint(&fixture, before).await;
    let (new_winner, new_peer) =
        publish_competing_document_proposal(&fixture, "The violet fox jumps over the quiet dog.\n")
            .await;
    let new_id = document_edit(&new_winner, "violet")["id"].as_str().unwrap();
    let new_url = format!(
        "/api/v1/workspaces/{}/review/suggestions/{}/accept",
        fixture.source.id,
        new_winner["id"].as_str().unwrap()
    );
    let (status, accepted) = post(
        &owner,
        &new_url,
        json!({"editId":new_id,"afterLines":"purple"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let expected_hash = data(&accepted)["writtenHash"].as_str().unwrap();
    assert_eq!(
        peer.seed_file(
            fixture.source.repo_id,
            PER_EDIT_DOC,
            "The purple fox jumps over the quiet dog.\n"
        )
        .await,
        expected_hash
    );
    let sibling = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let writes = peer
        .received_rpc_methods()
        .await
        .iter()
        .filter(|m| m.as_str() == "fs.write")
        .count();
    // A plausible proposal string alone cannot disambiguate historical receipts.
    peer.seed_file(
        fixture.source.repo_id,
        PER_EDIT_DOC,
        "The purple fox jumps over the loud dog.\n",
    )
    .await;
    let (status, refused) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":sibling}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        peer.received_rpc_methods()
            .await
            .iter()
            .filter(|m| m.as_str() == "fs.write")
            .count(),
        writes
    );
    peer.seed_file(
        fixture.source.repo_id,
        PER_EDIT_DOC,
        "The purple fox jumps over the quiet dog.\n",
    )
    .await;
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":sibling}),
    )
    .await;
    assert_eq!(status, StatusCode::OK,
        "an exact current receipt/version witness must retain the independent pending edit: {accepted}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some("The purple fox jumps over the sleepy dog.\n")
    );
    let losing = document_edit(&fixture.parent, "blue")["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], losing),
        "obsolete"
    );
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], sibling),
        "accepted"
    );
    drop(peer);
    drop(old_peer);
    drop(new_peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_cross_parent_context_survives_publication_continuation() {
    for rejected in [false, true] {
        let before = "The red fox jumps over the quiet dog.\n";
        let fixture =
            publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
        let partial = "The emerald fox jumps over the quiet dog.\n";
        let complete = "The emerald fox jumps over the sleepy dog.\n";
        let (other, other_peer) = publish_competing_document_proposal(&fixture, partial).await;
        let winner = document_edit(&other, "emerald")["id"].as_str().unwrap();
        let sibling = document_edit(&fixture.parent, "sleepy")["id"]
            .as_str()
            .unwrap();
        let thread = document_edit(&fixture.parent, "sleepy")["commentId"].clone();
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let url = format!(
            "/api/v1/workspaces/{}/review/suggestions/{}/accept",
            fixture.source.id,
            other["id"].as_str().unwrap()
        );
        let (status, body) = post(&owner, &url, json!({"editId":winner})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, reply) = post(&owner, &per_edit_route(&fixture, "replies"), json!({"editId":sibling,"content":"Keep this independent discussion across the accepted winner."})).await;
        assert_eq!(status, StatusCode::CREATED, "{reply}");
        if rejected {
            let (status, refusal) = post(
                &owner,
                &per_edit_route(&fixture, "reject"),
                json!({"editId":sibling,"reason":"Keep the quiet dog."}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{refusal}");
        }
        seed_per_edit_checkpoint(&fixture, partial).await;
        let prior = read_current_per_edit_parent(&fixture).await;
        assert_eq!(prior["expectedBeforeLines"], partial);
        let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
        let (_, source_head) =
            review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
        let review_head = commit_file_on(
            &repo_path,
            &source_head,
            PER_EDIT_DOC,
            complete.as_bytes(),
            "reviewer: retain independent quiet edit on accepted context",
        );
        helpers::seed_workspace_authority_head(
            &fixture.app,
            fixture.review_workspace_id,
            fixture.source.repo_id,
            &review_head,
        )
        .await;
        let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
        let (status, preview) = post(
            &reviewer,
            &format!(
                "/api/v1/workspaces/{}/review/preview",
                fixture.review_workspace_id
            ),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        let change = &data(&preview)["files"][0]["changes"][0];
        assert_eq!(change["editCount"], 1);
        assert_eq!(change["carriedEditCount"], 1);
        assert_eq!(change["carriedOpenEditCount"], usize::from(!rejected));
        let (status, republished) = post(
            &reviewer,
            &format!(
                "/api/v1/workspaces/{}/review/publish",
                fixture.review_workspace_id
            ),
            json!({"summary":"Continue independent dog edit", "verdict":"request_changes"}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{republished}");
        let successor = data(&republished)["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["path"] == PER_EDIT_DOC)
            .unwrap();
        let expected_status = if rejected { "rejected" } else { "open" };
        assert_eq!(
            document_edit_status(successor, sibling),
            expected_status,
            "accepted cross-parent context must not create a fresh quiet edit: {republished}"
        );
        let retained = successor["edits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|edit| edit["id"] == sibling)
            .unwrap();
        assert_eq!(retained["commentId"], thread);
        let old = prior["edits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|edit| edit["id"] == sibling)
            .unwrap();
        assert_eq!(retained["statusBy"], old["statusBy"]);
        assert_eq!(retained["statusAt"], old["statusAt"]);
        let successor_id = successor["id"].as_str().unwrap();
        let (status, replies) = get(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{successor_id}/replies?editId={sibling}",
                fixture.source.id
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{replies}");
        assert!(replies.to_string().contains("independent discussion"));
        let (status, listed) = get(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions?currentOnly=true",
                fixture.source.id
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert!(data(&listed)["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == successor["id"]
                && document_edit_status(row, sibling) == expected_status));
        if !rejected {
            let (status, accepted) = post(
                &owner,
                &format!(
                    "/api/v1/workspaces/{}/review/suggestions/{successor_id}/accept",
                    fixture.source.id
                ),
                json!({"editId":sibling}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{accepted}");
            assert_eq!(
                peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                    .await
                    .as_deref(),
                Some(complete)
            );
        } else {
            assert_eq!(
                peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                    .await
                    .as_deref(),
                Some(partial)
            );
        }
        drop(peer);
        drop(other_peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_repeated_passages_keep_independent_publication_preview_and_legacy_units() {
    let before = format!(
        "{}\n",
        std::iter::repeat_n("old keep", 260)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let after = before.replace("old", "new");
    let fixture = publish_per_edit_doc_with_count(&before, &after, 260).await;
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(data(&preview)["files"][0]["changes"][0]["editCount"], 260);
    assert_eq!(
        data(&preview)["files"][0]["changes"][0]["carriedEditCount"],
        260
    );
    let threads: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
        .fetch_one(&fixture.app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE review_suggestions SET edits = '[]'::jsonb WHERE id = $1")
        .bind(Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap())
        .execute(&fixture.app.db)
        .await
        .unwrap();
    let normalized = read_current_per_edit_parent(&fixture).await;
    assert_eq!(normalized["edits"].as_array().unwrap().len(), 260);
    let again = read_current_per_edit_parent(&fixture).await;
    assert_eq!(normalized["edits"], again["edits"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM comment_index")
            .fetch_one(&fixture.app.db)
            .await
            .unwrap(),
        threads,
        "legacy viewing must not create threads"
    );
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let selected = &normalized["edits"][130];
    let selected_id = selected["id"].as_str().unwrap();
    let (status, applied) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":selected_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let mut expected = before.clone();
    expected.replace_range(
        selected["start"].as_u64().unwrap() as usize..selected["end"].as_u64().unwrap() as usize,
        selected["afterText"].as_str().unwrap(),
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(expected.as_str())
    );
    seed_per_edit_checkpoint(&fixture, &expected).await;
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(
        current["edits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edit| edit["status"] == "accepted")
            .count(),
        1
    );
    assert_eq!(
        current["edits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edit| edit["status"] == "open")
            .count(),
        259
    );
    drop(peer);
    fixture.cleanup().await;
}

async fn qualify_block_preview_and_legacy_normalization(
    fixture: &PerEditReview,
    count: usize,
) -> Value {
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(data(&preview)["files"][0]["changes"][0]["editCount"], count);
    assert_eq!(
        data(&preview)["files"][0]["changes"][0]["carriedEditCount"],
        count
    );
    let threads: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
        .fetch_one(&fixture.app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE review_suggestions SET edits = '[]'::jsonb WHERE id = $1")
        .bind(Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap())
        .execute(&fixture.app.db)
        .await
        .unwrap();
    let normalized = read_current_per_edit_parent(fixture).await;
    assert_eq!(normalized["edits"].as_array().unwrap().len(), count);
    let again = read_current_per_edit_parent(fixture).await;
    assert_eq!(normalized["edits"], again["edits"]);
    for (original, restored) in fixture.parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .zip(normalized["edits"].as_array().unwrap())
    {
        for field in ["id", "start", "end", "beforeText", "afterText"] {
            assert_eq!(original[field], restored[field], "normalization {field}");
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM comment_index")
            .fetch_one(&fixture.app.db)
            .await
            .unwrap(),
        threads,
        "read normalization cannot create threads"
    );
    normalized
}

#[tokio::test]
async fn document_edit_whitespace_boundaries_keep_exact_bytes_through_accept_and_reload() {
    for (before, after) in [
        ("Intro.\n\nLGTM\n\nEnd.\n", "Intro.\n\n LGTM\n\nEnd.\n"),
        ("Intro.\n\nLGTM\n", "Intro.\n\n\nLGTM\n"),
        ("Intro.\n\n\nLGTM\n", "Intro.\n\nLGTM\n"),
        ("LGTM\n", "\nLGTM\n"),
        ("LGTM\n", "LGTM\n\n"),
        ("LGTM\n\n", "LGTM\n"),
        ("LGTM\n", " LGTM\n"),
        ("LGTM", " LGTM"),
        ("LGTM", "LGTM\n\n"),
        ("", "\n"),
        ("\n", ""),
        ("é🦀\r\n", " \té🦀\r\n"),
    ] {
        let fixture = publish_per_edit_doc_with_count(before, after, 1).await;
        assert_eq!(
            fixture.parent["afterLines"].as_str(),
            Some(after),
            "publication retains exact proposal bytes"
        );
        let id = fixture.parent["edits"][0]["id"].as_str().unwrap();
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let (body, expected) = if before == "LGTM\n" && after == " LGTM\n" {
            (json!({"editId":id,"afterLines":"  "}), "  LGTM\n")
        } else {
            (json!({"editId":id}), after)
        };
        let (status, accepted) = post(&owner, &per_edit_route(&fixture, "accept"), body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{before:?} -> {after:?}: {accepted}"
        );
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected),
            "{before:?} -> {after:?}"
        );
        seed_per_edit_checkpoint(&fixture, expected).await;
        let reloaded = read_current_per_edit_parent(&fixture).await;
        assert_eq!(document_edit_status(&reloaded, id), "accepted");
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_whitespace_recovery_requires_exact_eof_bytes() {
    let fixture = publish_per_edit_doc_with_count("LGTM", " LGTM", 1).await;
    let id = fixture.parent["edits"][0]["id"].as_str().unwrap();
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, " LGTM\n")
        .await;
    let (status, refused) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":id}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a matching line with different EOF bytes is not an application receipt: {refused}"
    );
    assert_eq!(
        document_edit_status(&read_current_per_edit_parent(&fixture).await, id),
        "open"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(" LGTM\n")
    );
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, " LGTM")
        .await;
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(data(&accepted)["alreadyApplied"], true);
    assert_eq!(
        document_edit_status(&read_current_per_edit_parent(&fixture).await, id),
        "accepted"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(" LGTM")
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_whitespace_missing_or_duplicate_source_stays_open_and_discussable() {
    let fixture = publish_per_edit_doc_with_count("LGTM\n", " LGTM\n", 1).await;
    let id = fixture.parent["edits"][0]["id"].as_str().unwrap();
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    for current in ["Changed.\n", "LGTM\n\nLGTM\n"] {
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, current)
            .await;
        let (status, refused) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":id}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{current:?}: {refused}");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(current)
        );
        assert_eq!(
            document_edit_status(&read_current_per_edit_parent(&fixture).await, id),
            "open"
        );
    }
    let (status, reply) = post(
        &owner,
        &per_edit_route(&fixture, "replies"),
        json!({"editId":id,"content":"Please refresh the original source context."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let (status, rejected) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId":id,"reason":"The current location is unresolved."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(
        document_edit_status(&read_current_per_edit_parent(&fixture).await, id),
        "rejected"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some("LGTM\n\nLGTM\n")
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_retained_prefix_suffix_blocks_publish_partial_edit_reject_and_reload() {
    for newline in ["\n", "\r\n"] {
        let separator = newline.repeat(2);
        for (boundary, prefix, suffix) in [
            (
                "start",
                String::new(),
                format!("Quiet.{separator}Tail.{newline}"),
            ),
            (
                "middle",
                format!("Intro.{separator}"),
                format!("Quiet.{separator}Tail.{newline}"),
            ),
            (
                "end",
                format!("Intro.{separator}Quiet.{newline}"),
                String::new(),
            ),
        ] {
            for first_block in ["Quiet. added", "Added Quiet."] {
                let before = format!("{prefix}{suffix}");
                let (after, first_only) = if boundary == "end" {
                    let prefix = prefix.trim_end_matches(['\r', '\n']);
                    (format!("{prefix}{separator}{first_block}{separator}Second added block.{newline}"), format!("{prefix}{separator}{first_block}{newline}"))
                } else {
                    (format!("{prefix}{first_block}{separator}Second added block.{separator}{suffix}"), format!("{prefix}{first_block}{separator}{suffix}"))
                };
                let fixture = publish_per_edit_doc(&before, &after).await;
                let normalized = qualify_block_preview_and_legacy_normalization(&fixture, 2).await;
                assert!(
                    normalized["edits"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|edit| edit["beforeText"] == ""),
                    "retained canonical block cannot become a replacement: {normalized}"
                );
                let first = document_edit(&normalized, first_block)["id"]
                    .as_str()
                    .unwrap();
                let second = document_edit(&normalized, "Second added block.")["id"]
                    .as_str()
                    .unwrap();
                let peer = TestWorkspaceRuntimePeer::attach(
                    &fixture.app,
                    &fixture.source,
                    fixture.owner.id,
                )
                .await;
                peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &before)
                    .await;
                let owner = fixture.app.authenticated_client(&fixture.owner);
                let edited = newline == "\r\n";
                let edited_delta = document_edit(&normalized, first_block)["afterText"]
                    .as_str()
                    .unwrap()
                    .replace(first_block, "Owner edited block.");
                let body = if edited {
                    json!({"editId":first,"afterLines":edited_delta})
                } else {
                    json!({"editId":first})
                };
                let (status, accepted) =
                    post(&owner, &per_edit_route(&fixture, "accept"), body).await;
                assert_eq!(status, StatusCode::OK, "{accepted}");
                assert_eq!(
                    document_edit_status(&data(&accepted)["suggestion"], second),
                    "open"
                );
                let expected = if edited {
                    first_only.replace(first_block, "Owner edited block.")
                } else {
                    first_only
                };
                assert_eq!(
                    peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                        .await
                        .as_deref(),
                    Some(expected.as_str()),
                    "{boundary}, {first_block:?}, {newline:?}"
                );
                seed_per_edit_checkpoint(&fixture, &expected).await;
                let pending = read_current_per_edit_parent(&fixture).await;
                assert_eq!(document_edit_status(&pending, first), "accepted");
                assert_eq!(document_edit_status(&pending, second), "open");
                let (status, rejected) = post(
                    &owner,
                    &per_edit_route(&fixture, "reject"),
                    json!({"editId":second,"reason":"Only keep the chosen whole block."}),
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{rejected}");
                let reloaded = read_current_per_edit_parent(&fixture).await;
                assert_eq!(document_edit_status(&reloaded, first), "accepted");
                assert_eq!(document_edit_status(&reloaded, second), "rejected");
                assert_eq!(reloaded["status"], "partial");
                assert_eq!(
                    peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                        .await
                        .as_deref(),
                    Some(expected.as_str())
                );
                drop(peer);
                fixture.cleanup().await;
            }
        }
    }
}

#[tokio::test]
async fn document_edit_complete_replacement_excess_blocks_publish_each_decision_order() {
    for newline in ["\n", "\r\n"] {
        let separator = newline.repeat(2);
        for (prefix, suffix) in [
            (String::new(), newline.to_owned()),
            (String::new(), format!("{separator}Tail.{newline}")),
            (
                format!("Intro.{separator}"),
                format!("{separator}Tail.{newline}"),
            ),
        ] {
            for addition in [
                "addition".to_owned(),
                format!("```js{newline}addition{newline}{newline}more{newline}```"),
                format!("- addition{newline}{newline}  continuation{newline}- second"),
            ] {
                for first in [0, 1] {
                    let before = format!("{prefix}old{suffix}");
                    let after = format!("{prefix}replacement{separator}{addition}{suffix}");
                    let fixture = publish_per_edit_doc(&before, &after).await;
                    let normalized =
                        qualify_block_preview_and_legacy_normalization(&fixture, 2).await;
                    let edits = normalized["edits"].as_array().unwrap();
                    assert_eq!(edits[0]["beforeText"], "old");
                    assert_eq!(edits[0]["afterText"], "replacement");
                    assert_eq!(edits[1]["beforeText"], "");
                    let ids = [
                        edits[0]["id"].as_str().unwrap(),
                        edits[1]["id"].as_str().unwrap(),
                    ];
                    let peer = TestWorkspaceRuntimePeer::attach(
                        &fixture.app,
                        &fixture.source,
                        fixture.owner.id,
                    )
                    .await;
                    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &before)
                        .await;
                    let owner = fixture.app.authenticated_client(&fixture.owner);
                    let edited = first == 0 && newline == "\r\n";
                    let body = if edited {
                        json!({"editId":ids[first],"afterLines":"owner rewrite"})
                    } else {
                        json!({"editId":ids[first]})
                    };
                    let (status, applied) =
                        post(&owner, &per_edit_route(&fixture, "accept"), body).await;
                    assert_eq!(status, StatusCode::OK, "{applied}");
                    assert_eq!(
                        document_edit_status(&data(&applied)["suggestion"], ids[1 - first]),
                        "open"
                    );
                    let expected = if first == 0 {
                        format!(
                            "{prefix}{}{suffix}",
                            if edited {
                                "owner rewrite"
                            } else {
                                "replacement"
                            }
                        )
                    } else {
                        format!("{prefix}old{separator}{addition}{suffix}")
                    };
                    assert_eq!(
                        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                            .await
                            .as_deref(),
                        Some(expected.as_str()),
                        "first={first}, {before:?} => {after:?}"
                    );
                    seed_per_edit_checkpoint(&fixture, &expected).await;
                    let pending = read_current_per_edit_parent(&fixture).await;
                    assert_eq!(document_edit_status(&pending, ids[first]), "accepted");
                    assert_eq!(document_edit_status(&pending, ids[1 - first]), "open");
                    // LF completes the opposite decision order; CRLF rejects the
                    // sibling and proves the owner override survives reload.
                    let action = if newline == "\n" { "accept" } else { "reject" };
                    let (status, final_decision) = post(
                        &owner,
                        &per_edit_route(&fixture, action),
                        json!({"editId":ids[1-first],"reason":"Keep only this selected unit."}),
                    )
                    .await;
                    assert_eq!(status, StatusCode::OK, "{final_decision}");
                    let final_content = if action == "accept" {
                        &after
                    } else {
                        &expected
                    };
                    assert_eq!(
                        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                            .await
                            .as_deref(),
                        Some(final_content.as_str())
                    );
                    seed_per_edit_checkpoint(&fixture, final_content).await;
                    let current = read_current_per_edit_parent(&fixture).await;
                    assert_eq!(document_edit_status(&current, ids[first]), "accepted");
                    assert_eq!(
                        document_edit_status(&current, ids[1 - first]),
                        if action == "accept" {
                            "accepted"
                        } else {
                            "rejected"
                        }
                    );
                    drop(peer);
                    fixture.cleanup().await;
                }
            }
        }
    }
}

#[tokio::test]
async fn document_edit_same_coordinate_fragments_keep_unique_durable_choices() {
    let before = "-\n";
    let after = "- m\n\n  continuation\n- y\n<div m\n\n  continuation\n```\n";
    let fixture = publish_per_edit_doc_with_count(before, after, 6).await;
    let normalized = qualify_block_preview_and_legacy_normalization(&fixture, 6).await;
    let repeated = normalized["edits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edit| edit["afterText"] == "\n\n  continuation")
        .collect::<Vec<_>>();
    assert_eq!(repeated.len(), 2);
    assert_ne!(repeated[0]["id"], repeated[1]["id"]);
    let first = repeated[0]["id"].as_str().unwrap();
    let second = repeated[1]["id"].as_str().unwrap();
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":second}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], first),
        "open"
    );
    let expected = "-\n\n  continuation\n";
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(expected)
    );
    seed_per_edit_checkpoint(&fixture, expected).await;
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&current, second), "accepted");
    assert_eq!(document_edit_status(&current, first), "open");
    let (status, rejected) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId":first,"reason":"One repeated source fragment only."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(
        document_edit_status(&data(&rejected)["suggestion"], second),
        "accepted"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(expected)
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_native_fixture_keeps_rewrite_and_prefix_blocks_separate() {
    let before = "old\n\nQuiet.\n";
    let after = "replacement\n\nQuiet. added\n\nSecond added block.\n\nQuiet.\n";
    let fixture = publish_per_edit_doc_with_count(before, after, 3).await;
    let normalized = qualify_block_preview_and_legacy_normalization(&fixture, 3).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let ids: Vec<_> = normalized["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| edit["id"].as_str().unwrap())
        .collect();
    for (id, expected) in [
        (ids[0], "replacement\n\nQuiet.\n"),
        (ids[1], "replacement\n\nQuiet. added\n\nQuiet.\n"),
    ] {
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":id}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], ids[2]),
            "open"
        );
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected)
        );
        seed_per_edit_checkpoint(&fixture, expected).await;
        let current = read_current_per_edit_parent(&fixture).await;
        assert_eq!(document_edit_status(&current, ids[2]), "open");
    }
    let (status, rejected) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId":ids[2],"reason":"Keep two independent chosen units."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_standalone_rule_blocks_have_independent_published_decisions() {
    for (addition, expected) in [
        ("\n---\n\nFirst paragraph\n", "Title\n\nFirst paragraph\n"),
        ("\nFirst paragraph\n\n---\n", "Title\n\n---\n"),
        ("\n---\n\n***\n", "Title\n\n***\n"),
    ] {
        let before = "Title\n";
        let after = format!("{before}{addition}");
        let fixture = publish_per_edit_doc_with_count(before, &after, 2).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let owner = fixture.app.authenticated_client(&fixture.owner);
        let first = fixture.parent["edits"][0]["id"].as_str().unwrap();
        let second = fixture.parent["edits"][1]["id"].as_str().unwrap();
        let (status, rejected) = post(
            &owner,
            &per_edit_route(&fixture, "reject"),
            json!({"editId":first,"reason":"Leave out only this standalone block."}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{rejected}");
        assert_eq!(
            document_edit_status(&data(&rejected)["suggestion"], second),
            "open"
        );
        let (status, accepted) = post(
            &owner,
            &per_edit_route(&fixture, "accept"),
            json!({"editId":second}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], first),
            "rejected"
        );
        assert_eq!(
            document_edit_status(&data(&accepted)["suggestion"], second),
            "accepted"
        );
        assert_eq!(data(&accepted)["suggestion"]["status"], "partial");
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(expected)
        );
        seed_per_edit_checkpoint(&fixture, expected).await;
        let current = read_current_per_edit_parent(&fixture).await;
        assert_eq!(document_edit_status(&current, first), "rejected");
        assert_eq!(document_edit_status(&current, second), "accepted");
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_parent_withdraw_retires_all_remaining_open_units_only() {
    let before = "The red fox watches the quiet dog beside a small pond.\n";
    let fixture = publish_per_edit_doc_with_count(
        before,
        "The blue fox watches the sleepy dog beside a large pond.\n",
        3,
    )
    .await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let accepted_id = document_edit(&fixture.parent, "blue")["id"]
        .as_str()
        .unwrap();
    let (status, accepted) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":accepted_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let applied = peer
        .file_content(fixture.source.repo_id, PER_EDIT_DOC)
        .await
        .unwrap();
    let threads: Vec<Value> = data(&accepted)["suggestion"]["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| edit["commentId"].clone())
        .collect();
    let writes_before = peer
        .received_rpc_methods()
        .await
        .iter()
        .filter(|m| m.as_str() == "fs.write")
        .count();
    let (status, withdrawn) =
        post(&reviewer, &per_edit_route(&fixture, "withdraw"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{withdrawn}");
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(current["status"], "withdrawn");
    assert_eq!(document_edit_status(&current, accepted_id), "accepted");
    assert_eq!(
        current["edits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edit| edit["status"] == "withdrawn")
            .count(),
        2
    );
    assert_eq!(
        current["edits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edit| edit["commentId"].clone())
            .collect::<Vec<_>>(),
        threads
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(applied.as_str())
    );
    assert_eq!(
        peer.received_rpc_methods()
            .await
            .iter()
            .filter(|m| m.as_str() == "fs.write")
            .count(),
        writes_before
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn same_line_document_edits_apply_in_either_order_and_parent_actions_preserve_decisions() {
    for reverse in [false, true] {
        let before = "The red fox jumps over the quiet dog.\n";
        let after = "The blue fox jumps over the sleepy dog.\n";
        let fixture = publish_per_edit_doc(before, after).await;
        let peer =
            TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
        peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
            .await;
        let client = fixture.app.authenticated_client(&fixture.owner);
        let blue = document_edit(&fixture.parent, "blue")["id"]
            .as_str()
            .unwrap();
        let sleepy = document_edit(&fixture.parent, "sleepy")["id"]
            .as_str()
            .unwrap();
        let first = if reverse { sleepy } else { blue };
        let second = if reverse { blue } else { sleepy };
        let first_body = if reverse {
            json!({"editId": first})
        } else {
            json!({"editId": first, "afterLines": "green"})
        };
        let (status, accepted) = post(
            &client,
            &per_edit_route(&fixture, "accept"),
            first_body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        let partial = if reverse {
            "The red fox jumps over the sleepy dog.\n"
        } else {
            "The green fox jumps over the quiet dog.\n"
        };
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(partial)
        );
        seed_per_edit_checkpoint(&fixture, partial).await;
        let current = read_current_per_edit_parent(&fixture).await;
        assert_eq!(
            document_edit_status(&current, second),
            "open",
            "sharing a line never obsoletes a sibling"
        );
        let writes_before = peer
            .received_rpc_methods()
            .await
            .iter()
            .filter(|method| method.as_str() == "fs.write")
            .count();
        let (status, retried) =
            post(&client, &per_edit_route(&fixture, "accept"), first_body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a lost response is settled, not reapplied: {retried}"
        );
        assert_eq!(
            document_edit_status(&data(&retried)["suggestion"], first),
            "accepted"
        );
        assert_eq!(
            peer.received_rpc_methods()
                .await
                .iter()
                .filter(|method| method.as_str() == "fs.write")
                .count(),
            writes_before
        );
        let (status, different_retry) = post(
            &client,
            &per_edit_route(&fixture, "accept"),
            json!({"editId": first, "afterLines": "overwrite settled decision"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{different_retry}");
        let (status, opposite) = post(
            &client,
            &per_edit_route(&fixture, "reject"),
            json!({"editId": first, "reason": "Too late"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{opposite}");
        // Legacy parent controls have an explicit narrower meaning: only the
        // still-open child, preserving an accepted owner-edited replacement.
        let (status, parent_accept) =
            post(&client, &per_edit_route(&fixture, "accept"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{parent_accept}");
        assert_eq!(data(&parent_accept)["suggestion"]["status"], "accepted");
        let final_bytes = if reverse {
            after
        } else {
            "The green fox jumps over the sleepy dog.\n"
        };
        assert_eq!(
            peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                .await
                .as_deref(),
            Some(final_bytes)
        );
        drop(peer);
        fixture.cleanup().await;
    }
}

#[tokio::test]
async fn document_edit_write_and_checkpoint_failures_do_not_decide_unchosen_siblings() {
    let before = "new line from reviewer.\n";
    let fixture = publish_per_edit_doc(
        before,
        "suggest to add this line from reviewer.\n\none more thing....\n",
    )
    .await;
    let peer = TestWorkspaceRuntimePeer::attach_with_changes_failure(
        &fixture.app,
        &fixture.source,
        fixture.owner.id,
        "push",
    )
    .await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let first = document_edit(&fixture.parent, "suggest to add this")["id"]
        .as_str()
        .unwrap();
    let second = document_edit(&fixture.parent, "one more thing")["id"]
        .as_str()
        .unwrap();
    peer.refuse_writes(fixture.source.repo_id, PER_EDIT_DOC)
        .await;
    let (status, failed) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": first}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{failed}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(before)
    );
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&current, first), "open");
    assert_eq!(document_edit_status(&current, second), "open");
    peer.allow_writes(fixture.source.repo_id, PER_EDIT_DOC)
        .await;
    let (status, applied) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": first}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(data(&applied)["checkpointPending"], true);
    assert_eq!(
        document_edit_status(&data(&applied)["suggestion"], first),
        "accepted"
    );
    assert_eq!(
        document_edit_status(&data(&applied)["suggestion"], second),
        "open"
    );
    let methods_before = peer.received_rpc_methods().await;
    let (status, duplicate) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": first}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate}");
    assert_eq!(
        data(&duplicate)["checkpointPending"],
        true,
        "retry retains the real unpublished receipt"
    );
    assert_eq!(
        peer.received_rpc_methods().await,
        methods_before,
        "duplicate decision performs no guest call"
    );
    let (status, opposite) = post(
        &client,
        &per_edit_route(&fixture, "reject"),
        json!({"editId": first}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{opposite}");
    let partial = "suggest to add this line from reviewer.\n";
    peer.allow_changes().await;
    let saved = client.put_json(&format!("/api/v1/workspaces/{}/files/{PER_EDIT_DOC}?repoId={}", fixture.source.id, fixture.source.repo_id), &json!({"content": partial, "encoding": "utf-8", "expectedHash": null, "expectedVmId": peer.vm_id(), "commitAndPush": true})).await;
    assert_eq!(saved.status(), StatusCode::OK);
    seed_per_edit_checkpoint(&fixture, partial).await;
    let remaining = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&remaining, second), "open");
    let (status, rejected) = post(
        &client,
        &per_edit_route(&fixture, "reject"),
        json!({"editId": second}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(data(&rejected)["suggestion"]["status"], "partial");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial)
    );
    drop(peer);
    fixture.cleanup().await;
}

async fn seed_legacy_document_sibling(fixture: &PerEditReview) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO review_suggestions (id, org_id, publication_id, repo_id, path, kind, before_lines, after_lines, anchor_start, content_hash, anchor_context, author_label) SELECT $1, org_id, publication_id, repo_id, path, kind, before_lines, after_lines, anchor_start, content_hash, anchor_context, author_label FROM review_suggestions WHERE id = $2")
        .bind(id).bind(Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap())
        .execute(&fixture.app.db).await.unwrap();
    id
}

#[tokio::test]
async fn document_edit_metadata_counts_normalize_once_without_source_projection() {
    let fixture = publish_per_edit_doc(
        "The red fox jumps over the quiet dog.\n",
        "The blue fox jumps over the sleepy dog.\n",
    )
    .await;
    let mut legacy = Vec::new();
    for _ in 0..64 {
        legacy.push(seed_legacy_document_sibling(&fixture).await);
    }
    let client = fixture.app.authenticated_client(&fixture.owner);
    let replies_before: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_replies")
        .fetch_one(&fixture.app.db)
        .await
        .unwrap();
    let threads_before: i64 = sqlx::query_scalar("SELECT count(*) FROM comment_index")
        .fetch_one(&fixture.app.db)
        .await
        .unwrap();
    let traffic_before = fixture._review_peer.received_rpc_methods().await;
    let base = format!("/api/v1/workspaces/{}/review", fixture.source.id);
    // First metadata read materializes legacy identities without creating threads.
    let (status, first) = get(&client, &format!("{base}/publications")).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let counts = &data(&first)["publications"][0];
    assert_eq!(counts["openCount"], 130);
    let persisted: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "SELECT id, xmin::text, edits FROM review_suggestions WHERE id = ANY($1) ORDER BY id",
    )
    .bind(&legacy)
    .fetch_all(&fixture.app.db)
    .await
    .unwrap();
    assert_eq!(persisted.len(), 64);
    assert!(persisted
        .iter()
        .all(|(_, _, edits)| edits.as_array().unwrap().len() == 2));
    eprintln!("MATERIALIZED_METADATA_READS_BEGIN");
    for _ in 0..2 {
        let (status, listed) = get(&client, &format!("{base}/publications")).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(data(&listed)["publications"][0]["openCount"], 130);
        let (status, badges) = get(&client, &format!("{base}/suggestions/counts")).await;
        assert_eq!(status, StatusCode::OK, "{badges}");
        assert_eq!(data(&badges)["paths"][0]["openCount"], 130);
    }
    eprintln!("MATERIALIZED_METADATA_READS_END");
    let unchanged: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "SELECT id, xmin::text, edits FROM review_suggestions WHERE id = ANY($1) ORDER BY id",
    )
    .bind(&legacy)
    .fetch_all(&fixture.app.db)
    .await
    .unwrap();
    assert_eq!(
        persisted, unchanged,
        "already-materialized metadata reads do not rewrite ledgers"
    );
    assert_eq!(
        fixture._review_peer.received_rpc_methods().await,
        traffic_before,
        "metadata reads call no VM"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM comment_index")
            .fetch_one(&fixture.app.db)
            .await
            .unwrap(),
        threads_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM comment_replies")
            .fetch_one(&fixture.app.db)
            .await
            .unwrap(),
        replies_before
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_current_badges_keep_row_eligibility_after_partial_checkpoint() {
    use g5_api::services::workspace_review_service as reviews;
    let before = "The red fox jumps over the quiet dog.\n";
    let partial = "The blue fox jumps over the quiet dog.\n";
    let fixture = publish_per_edit_doc(before, "The blue fox jumps over the sleepy dog.\n").await;
    let parent_id = Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap();
    let publication_id =
        Uuid::parse_str(fixture.parent["publicationId"].as_str().unwrap()).unwrap();
    let first = document_edit(&fixture.parent, "blue")["id"]
        .as_str()
        .unwrap();
    let pending = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    assert_eq!(fixture.parent["authorKind"], "human");
    let comment_id = Uuid::new_v4();
    sqlx::query("INSERT INTO review_suggestions (id, org_id, publication_id, repo_id, path, kind, before_lines, anchor_start, author_label, note) VALUES ($1, $2, $3, $4, $5, 'comment', $6, 1, 'Reviewer', 'This comment belongs to the original source commit.')")
        .bind(comment_id).bind(fixture.source.org_id).bind(publication_id)
        .bind(fixture.source.repo_id).bind(PER_EDIT_DOC).bind(before.trim_end())
        .execute(&fixture.app.db).await.unwrap();
    let zero = seed_legacy_document_sibling(&fixture).await;
    sqlx::query("UPDATE review_suggestions SET before_lines = after_lines, edits = '[]'::jsonb WHERE id = $1")
        .bind(zero).execute(&fixture.app.db).await.unwrap();
    let ids = [parent_id, comment_id, zero];
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let base = format!("/api/v1/workspaces/{}/review", fixture.source.id);
    for (stage, expected_count) in [4, 1, 0].into_iter().enumerate() {
        let persisted: Vec<(Uuid, String, Value, String)> = sqlx::query_as(
            "SELECT id, xmin::text, edits, status FROM review_suggestions WHERE id = ANY($1) ORDER BY id",
        ).bind(&ids[..]).fetch_all(&fixture.app.db).await.unwrap();
        assert_eq!(persisted.len(), 3);
        assert!(persisted
            .iter()
            .find(|row| row.0 == zero)
            .unwrap()
            .2
            .as_array()
            .unwrap()
            .is_empty());
        let source_rpc = peer.received_rpc_methods().await;
        let review_rpc = fixture._review_peer.received_rpc_methods().await;
        for _ in 0..2 {
            let (status, listed) =
                get(&owner, &format!("{base}/suggestions?currentOnly=true")).await;
            assert_eq!(status, StatusCode::OK, "{listed}");
            let rows = data(&listed)["suggestions"].as_array().unwrap();
            let shown: std::collections::HashSet<_> = rows
                .iter()
                .map(|row| Uuid::parse_str(row["id"].as_str().unwrap()).unwrap())
                .collect();
            let expected: std::collections::HashSet<_> = match stage {
                0 => ids.into_iter().collect(),
                1 => [parent_id].into_iter().collect(),
                _ => std::collections::HashSet::new(),
            };
            assert_eq!(
                shown, expected,
                "per-row current-only eligibility: {listed}"
            );
            let listed_open: usize = rows
                .iter()
                .map(|row| {
                    let edits = row["edits"].as_array().unwrap();
                    if edits.is_empty() {
                        usize::from(row["status"] == "open")
                    } else {
                        edits.iter().filter(|edit| edit["status"] == "open").count()
                    }
                })
                .sum();
            assert_eq!(listed_open, expected_count);
            if stage == 1 {
                assert_eq!(document_edit_status(&rows[0], first), "accepted");
                assert_eq!(document_edit_status(&rows[0], pending), "open");
            }
            let (status, counts) = get(&owner, &format!("{base}/suggestions/counts")).await;
            assert_eq!(status, StatusCode::OK, "{counts}");
            let badges = data(&counts)["paths"].as_array().unwrap();
            if expected_count == 0 {
                assert!(
                    badges.is_empty(),
                    "superseded and outdated-comment-only rows add no badge: {counts}"
                );
            } else {
                assert_eq!(badges.len(), 1, "{counts}");
                assert_eq!(badges[0]["repoId"], fixture.source.repo_id.to_string());
                assert_eq!(badges[0]["path"], PER_EDIT_DOC);
                assert_eq!(badges[0]["openCount"], expected_count, "a continuing ledger must not make its old comment or empty sibling current: {counts}");
            }
        }
        let unchanged: Vec<(Uuid, String, Value, String)> = sqlx::query_as(
            "SELECT id, xmin::text, edits, status FROM review_suggestions WHERE id = ANY($1) ORDER BY id",
        ).bind(&ids[..]).fetch_all(&fixture.app.db).await.unwrap();
        assert_eq!(
            persisted, unchanged,
            "repeated metadata reads must not normalize or rewrite zero-delta siblings"
        );
        assert_eq!(
            peer.received_rpc_methods().await,
            source_rpc,
            "metadata calls no source VM"
        );
        assert_eq!(
            fixture._review_peer.received_rpc_methods().await,
            review_rpc,
            "metadata calls no review VM"
        );
        if stage == 0 {
            let (status, accepted) = post(
                &owner,
                &per_edit_route(&fixture, "accept"),
                json!({"editId": first}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{accepted}");
            assert_eq!(
                document_edit_status(&data(&accepted)["suggestion"], first),
                "accepted"
            );
            assert_eq!(
                document_edit_status(&data(&accepted)["suggestion"], pending),
                "open"
            );
            assert_eq!(
                peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
                    .await
                    .as_deref(),
                Some(partial)
            );
            // The scripted peer does not implement receive-pack. Advance the
            // authoritative ref only after observing the actual accepted bytes.
            seed_per_edit_checkpoint(&fixture, partial).await;
        } else if stage == 1 {
            let mut db = fixture
                .app
                .scoped_db_for_user_org(fixture.reviewer.id, fixture.source.org_id)
                .await;
            let heads =
                reviews::source_heads(&mut db, &fixture.app.repo_storage, fixture.source.id)
                    .await
                    .unwrap();
            let successor = reviews::create_publication(
                &mut db,
                reviews::NewPublication {
                    org_id: fixture.source.org_id,
                    source_workspace_id: fixture.source.id,
                    review_workspace_id: Some(fixture.review_workspace_id),
                    reviewer_user_id: fixture.reviewer.id,
                    author_kind: "human".into(),
                    author_label: fixture.parent["authorLabel"].as_str().unwrap().into(),
                    base_commit_vector: reviews::annotation_vector(&heads),
                    summary: "Empty successor round retires the pending proposal.".into(),
                    verdict: Some("comment_only".into()),
                    scores: None,
                    kind: reviews::PUBLICATION_KIND_ROUND.into(),
                },
            )
            .await
            .unwrap();
            db.commit().await.unwrap();
            let superseded: Option<Uuid> =
                sqlx::query_scalar("SELECT superseded_by FROM review_publications WHERE id = $1")
                    .bind(publication_id)
                    .fetch_one(&fixture.app.db)
                    .await
                    .unwrap();
            assert_eq!(superseded, Some(successor.id));
        }
    }
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_foreign_child_ids_cannot_escape_the_authorized_parent() {
    let fixture = publish_per_edit_doc(
        "The red fox jumps over the quiet dog.\n",
        "The blue fox jumps over the sleepy dog.\n",
    )
    .await;
    let sibling = seed_legacy_document_sibling(&fixture).await;
    read_current_per_edit_parent(&fixture).await;
    let foreign: Value = sqlx::query_scalar("SELECT edits FROM review_suggestions WHERE id = $1")
        .bind(sibling)
        .fetch_one(&fixture.app.db)
        .await
        .unwrap();
    let child = foreign[0]["id"].as_str().unwrap();
    assert!(fixture.parent["edits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|edit| edit["id"] != child));
    let client = fixture.app.authenticated_client(&fixture.owner);
    let (status, body) = get(
        &client,
        &format!("{}?editId={child}", per_edit_route(&fixture, "replies")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = post(
        &client,
        &per_edit_route(&fixture, "replies"),
        json!({"editId":child,"content":"Foreign child reply"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = post(
        &client,
        &per_edit_route(&fixture, "reject"),
        json!({"editId":child}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    let (status, body) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId":child}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let current = read_current_per_edit_parent(&fixture).await;
    assert!(current["edits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|edit| edit["status"] == "open"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM comment_replies WHERE content = 'Foreign child reply'"
        )
        .fetch_one(&fixture.app.db)
        .await
        .unwrap(),
        0
    );
    assert!(
        peer.received_rpc_methods().await.is_empty(),
        "a foreign child is refused before VM mutation"
    );
    drop(peer);
    fixture.cleanup().await;
}

/// Metadata-only no-op legacy reads must neither wait on a VM-backed decision
/// nor rewrite the same empty ledger on every poll.
#[tokio::test]
async fn document_edit_noop_legacy_reads_do_not_lock_or_rewrite() {
    let fixture = publish_per_edit_doc(
        "The red fox jumps over the quiet dog.\n",
        "The blue fox jumps over the sleepy dog.\n",
    )
    .await;
    let id = Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE review_suggestions SET before_lines = after_lines, edits = '[]'::jsonb WHERE id = $1")
        .bind(id).execute(&fixture.app.db).await.unwrap();
    let before: String =
        sqlx::query_scalar("SELECT xmin::text FROM review_suggestions WHERE id = $1")
            .bind(id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    let mut blocker = fixture.app.db.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("review-decide:{}", fixture.source.id))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let client = fixture.app.authenticated_client(&fixture.owner);
    let base = format!("/api/v1/workspaces/{}/review", fixture.source.id);
    let read = async {
        for suffix in ["suggestions", "publications", "suggestions/counts"] {
            let (status, body) = get(&client, &format!("{base}/{suffix}")).await;
            assert_eq!(status, StatusCode::OK, "{suffix}: {body}");
        }
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), read).await;
    blocker.rollback().await.unwrap();
    result.expect("no-op legacy reads must finish while another decision holds its lock");
    let after: String =
        sqlx::query_scalar("SELECT xmin::text FROM review_suggestions WHERE id = $1")
            .bind(id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "an empty legacy proposal needs no normalization write"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn existing_open_document_parent_is_normalized_once_without_losing_parent_discussion() {
    let fixture = publish_per_edit_doc(
        "The red fox jumps over the quiet dog.\n",
        "The blue fox jumps over the sleepy dog.\n",
    )
    .await;
    let parent_id = Uuid::parse_str(fixture.parent["id"].as_str().unwrap()).unwrap();
    let client = fixture.app.authenticated_client(&fixture.owner);
    let (status, original) = post(
        &client,
        &per_edit_route(&fixture, "replies"),
        json!({"content": "Original whole-change context."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    // Reproduce an existing pre-migration open parent; no UI fabricates IDs.
    sqlx::query("UPDATE review_suggestions SET edits = '[]'::jsonb WHERE id = $1")
        .bind(parent_id)
        .execute(&fixture.app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET mutation_policy = 'read_only' WHERE id = $1")
        .bind(fixture.source.id)
        .execute(&fixture.app.db)
        .await
        .unwrap();
    let threads_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM comment_index WHERE workspace_id = $1")
            .bind(fixture.source.id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    let (_, head_before) =
        review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
    let first = read_current_per_edit_parent(&fixture).await;
    let second = read_current_per_edit_parent(&fixture).await;
    assert_eq!(
        first["edits"], second["edits"],
        "normalization persists identities, not fresh render indices"
    );
    assert_eq!(first["edits"].as_array().unwrap().len(), 2);
    assert!(first["edits"].as_array().unwrap().iter().all(|edit| {
        edit["status"] == "open"
            && edit["statusAt"].is_null()
            && edit["statusBy"].is_null()
            && edit["commentId"].is_null()
    }));
    let threads_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM comment_index WHERE workspace_id = $1")
            .bind(fixture.source.id)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
    assert_eq!(threads_after, threads_before);
    let (_, head_after) =
        review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
    assert_eq!(
        head_after, head_before,
        "read-only normalization writes no source commit"
    );
    assert_eq!(
        first["commentId"], fixture.parent["commentId"],
        "parent provenance stays accessible"
    );
    let blue = document_edit(&first, "blue")["id"].as_str().unwrap();
    let sleepy = document_edit(&first, "sleepy")["id"].as_str().unwrap();
    sqlx::query("UPDATE workspaces SET mutation_policy = 'read_write' WHERE id = $1")
        .bind(fixture.source.id)
        .execute(&fixture.app.db)
        .await
        .unwrap();
    let (status, reply) = post(
        &client,
        &per_edit_route(&fixture, "replies"),
        json!({"editId": blue, "content": "New phrase discussion."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let with_thread = read_current_per_edit_parent(&fixture).await;
    assert!(document_edit(&with_thread, "blue")["commentId"].is_string());
    assert!(document_edit(&with_thread, "sleepy")["commentId"].is_null());
    let (_, parent_thread) = get(&client, &per_edit_route(&fixture, "replies")).await;
    assert!(parent_thread
        .to_string()
        .contains("Original whole-change context."));
    assert!(!parent_thread.to_string().contains("New phrase discussion."));
    let (_, blue_thread) = get(
        &client,
        &format!("{}?editId={blue}", per_edit_route(&fixture, "replies")),
    )
    .await;
    assert!(blue_thread.to_string().contains("New phrase discussion."));
    assert!(!blue_thread
        .to_string()
        .contains("Original whole-change context."));
    let (_, sleepy_thread) = get(
        &client,
        &format!("{}?editId={sleepy}", per_edit_route(&fixture, "replies")),
    )
    .await;
    assert!(!sleepy_thread.to_string().contains("New phrase discussion."));
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, republished) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/publish",
            fixture.review_workspace_id
        ),
        json!({"summary": "Same edits again", "verdict": "request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{republished}");
    let (status, listed) = get(
        &client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=true",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let rows: Vec<&Value> = data(&listed)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["path"] == PER_EDIT_DOC)
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "repeat publication never duplicates actionable copies: {listed}"
    );
    let ids: Vec<&Value> = rows[0]["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| &edit["id"])
        .collect();
    assert!(
        ids.contains(&&json!(blue)) && ids.contains(&&json!(sleepy)),
        "publication keeps child identities: {listed}"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn concurrent_document_edit_decisions_serialize_without_relabeling_applied_content() {
    let before = "The red fox jumps over the quiet dog.\n";
    let after = "The blue fox jumps over the sleepy dog.\n";
    let fixture = publish_per_edit_doc(before, after).await;
    let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &fixture.app,
        &fixture.source,
        fixture.owner.id,
        "fs.write",
    )
    .await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let first = document_edit(&fixture.parent, "blue")["id"]
        .as_str()
        .unwrap();
    let second = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let accept_url = per_edit_route(&fixture, "accept");
    let reject_url = per_edit_route(&fixture, "reject");
    let accept = post(&client, &accept_url, json!({"editId": first}));
    let reject_during_write = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !peer
                .received_rpc_methods()
                .await
                .iter()
                .any(|method| method == "fs.write")
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("accept reached the guest write under its decision lock");
        let reject = post(
            &client,
            &reject_url,
            json!({"editId": first, "reason": "Concurrent rejection"}),
        );
        let release = async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND NOT granted AND database = (SELECT oid FROM pg_database WHERE datname = current_database()))").fetch_one(&fixture.app.db).await.unwrap();
                    if waiting { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }).await.expect("the competing reject waits on the authoritative decision lock");
            peer.release_one_rpc();
        };
        let (result, ()) = tokio::join!(reject, release);
        result
    };
    let ((accepted_status, accepted), (rejected_status, rejected)) =
        tokio::join!(accept, reject_during_write);
    assert_eq!(accepted_status, StatusCode::OK, "{accepted}");
    assert_eq!(rejected_status, StatusCode::CONFLICT, "{rejected}");
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], first),
        "accepted"
    );
    assert_eq!(
        document_edit_status(&data(&accepted)["suggestion"], second),
        "open"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some("The blue fox jumps over the quiet dog.\n")
    );
    peer.release_rpc_barrier();
    let (status, remainder) = post(&client, &accept_url, json!({"editId": second})).await;
    assert_eq!(status, StatusCode::OK, "{remainder}");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(after)
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn simultaneous_sibling_document_accepts_compose_their_individual_deltas() {
    let before = "The red fox jumps over the quiet dog.\n";
    let after = "The blue fox jumps over the sleepy dog.\n";
    let fixture = publish_per_edit_doc(before, after).await;
    let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &fixture.app,
        &fixture.source,
        fixture.owner.id,
        "fs.write",
    )
    .await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, before)
        .await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let first = document_edit(&fixture.parent, "blue")["id"]
        .as_str()
        .unwrap();
    let second = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let url = per_edit_route(&fixture, "accept");
    let release = async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND NOT granted AND database = (SELECT oid FROM pg_database WHERE datname = current_database()))").fetch_one(&fixture.app.db).await.unwrap();
                if waiting && peer.received_rpc_methods().await.iter().any(|method| method == "fs.write") { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("one sibling waits while the other writes");
        peer.release_one_rpc();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while peer
                .received_rpc_methods()
                .await
                .iter()
                .filter(|method| method.as_str() == "fs.write")
                .count()
                < 2
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the second sibling re-reads and writes after the first settles");
        peer.release_one_rpc();
    };
    let (a, b, ()) = tokio::join!(
        post(&client, &url, json!({"editId": first})),
        post(&client, &url, json!({"editId": second})),
        release
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(b.0, StatusCode::OK, "{}", b.1);
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(after)
    );
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(current["status"], "accepted");
    assert_eq!(document_edit_status(&current, first), "accepted");
    assert_eq!(document_edit_status(&current, second), "accepted");
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn document_edit_recovery_requires_its_captured_source_frame() {
    let leading: String = (1..=7)
        .map(|line| format!("Leading owner context {line}.\n"))
        .collect();
    let trailing: String = (1..=7)
        .map(|line| format!("Trailing owner context {line}.\n"))
        .collect();
    let before = format!("{leading}The red fox.\nThe quiet dog.\n{trailing}");
    let after = before.replace("red", "blue").replace("quiet", "sleepy");
    let fixture = publish_per_edit_doc(&before, &after).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    let client = fixture.app.authenticated_client(&fixture.owner);
    let first = document_edit(&fixture.parent, "blue");
    let first_id = first["id"].as_str().unwrap();
    let second_id = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let captured_block = fixture.parent["beforeLines"].as_str().unwrap();
    let applied_block = first["expectedAfterLines"].as_str().unwrap();
    // The target was removed. Identical proposed bytes in another frame are
    // not evidence that this selected edit was written at its original site.
    let archive =
        format!("Unrelated archive begins.\n\n{applied_block}\nUnrelated archive ends.\n");
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &archive)
        .await;
    let (status, refused) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": first_id}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "proposal elsewhere is not an applied receipt: {refused}"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(archive.as_str())
    );
    let current = read_current_per_edit_parent(&fixture).await;
    assert_eq!(document_edit_status(&current, first_id), "open");
    assert_eq!(document_edit_status(&current, second_id), "open");
    assert!(!peer
        .received_rpc_methods()
        .await
        .iter()
        .any(|method| method == "fs.write"));

    // Reproduce a real write followed by a lost response/transaction: only
    // the chosen delta landed, and the original surrounding source remains.
    let recovered = before.replacen(captured_block, applied_block, 1);
    assert_ne!(recovered, before);
    assert!(!recovered.contains("sleepy"));
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &recovered)
        .await;
    let (status, settled) = post(
        &client,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": first_id}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "proven post-write recovery settles exactly that edit: {settled}"
    );
    assert_eq!(data(&settled)["alreadyApplied"], true);
    assert_eq!(
        document_edit_status(&data(&settled)["suggestion"], first_id),
        "accepted"
    );
    assert_eq!(
        document_edit_status(&data(&settled)["suggestion"], second_id),
        "open"
    );
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(recovered.as_str())
    );
    assert!(
        !peer
            .received_rpc_methods()
            .await
            .iter()
            .any(|method| method == "fs.write"),
        "recovery acknowledges existing bytes without a duplicate source write"
    );
    drop(peer);
    fixture.cleanup().await;
}

#[tokio::test]
async fn republishing_a_shrunken_document_hunk_preserves_the_rejected_edit() {
    let leading: String = (1..=7)
        .map(|line| format!("Leading owner context {line}.\n"))
        .collect();
    let trailing: String = (1..=7)
        .map(|line| format!("Trailing owner context {line}.\n"))
        .collect();
    let before = format!(
        "{leading}The red fox.\nFirst unchanged bridge.\nSecond unchanged bridge.\nThird unchanged bridge.\nThe quiet dog.\n{trailing}"
    );
    let after = before.replace("red", "blue").replace("quiet", "sleepy");
    let fixture = publish_per_edit_doc(&before, &after).await;
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    peer.seed_file(fixture.source.repo_id, PER_EDIT_DOC, &before)
        .await;
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let rejected = document_edit(&fixture.parent, "blue");
    let rejected_id = rejected["id"].as_str().unwrap();
    let accepted_id = document_edit(&fixture.parent, "sleepy")["id"]
        .as_str()
        .unwrap();
    let (status, reply) = post(
        &owner,
        &per_edit_route(&fixture, "replies"),
        json!({"editId": rejected_id, "content": "The fox must stay red."}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply}");
    let (status, refused) = post(
        &owner,
        &per_edit_route(&fixture, "reject"),
        json!({"editId": rejected_id, "reason": "Keep the original colour."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refused}");
    let (status, settled) = post(
        &owner,
        &per_edit_route(&fixture, "accept"),
        json!({"editId": accepted_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{settled}");
    assert_eq!(data(&settled)["suggestion"]["status"], "partial");
    let partial = before.replace("quiet", "sleepy");
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial.as_str())
    );
    seed_per_edit_checkpoint(&fixture, &partial).await;

    // The reviewer updates their copy onto the owner's actual new Git head,
    // retaining their rejected proposal. Git now generates a smaller hunk:
    // the accepted dog change is common to both source and review branches.
    let repo_path = fixture.app.repo_path(fixture.source.repo_id).await;
    let (_, source_head) =
        review_branch(&fixture.app, fixture.source.id, fixture.source.repo_id).await;
    let updated_review_head = commit_file_on(
        &repo_path,
        &source_head,
        PER_EDIT_DOC,
        after.as_bytes(),
        "reviewer: update from owner while retaining the red fox proposal",
    );
    helpers::seed_workspace_authority_head(
        &fixture.app,
        fixture.review_workspace_id,
        fixture.source.repo_id,
        &updated_review_head,
    )
    .await;
    assert_eq!(
        git_stdout(
            &repo_path,
            &["merge-base", &source_head, &updated_review_head]
        ),
        source_head
    );
    let remaining_diff = git_stdout(
        &repo_path,
        &[
            "diff",
            "--unified=3",
            &source_head,
            &updated_review_head,
            "--",
            PER_EDIT_DOC,
        ],
    );
    assert!(fixture.parent["beforeLines"]
        .as_str()
        .unwrap()
        .contains("quiet"));
    assert!(remaining_diff.contains("-The red fox."), "{remaining_diff}");
    assert!(
        remaining_diff.contains("+The blue fox."),
        "{remaining_diff}"
    );
    assert!(
        !remaining_diff.contains("sleepy") && !remaining_diff.contains("quiet"),
        "the remaining Git hunk excludes the settled sibling's old frame: {remaining_diff}"
    );
    let reviewer = fixture.app.authenticated_client(&fixture.reviewer);
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let change = data(&preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == PER_EDIT_DOC)
        .unwrap()["changes"][0]
        .clone();
    assert!(
        change["publishedIn"].is_null(),
        "a source-head advance follows the existing publication currency policy: {preview}"
    );
    assert_eq!(change["carried"]["suggestionId"], fixture.parent["id"]);
    assert_eq!(change["carried"]["note"], fixture.parent["note"]);
    let (status, republished) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/publish",
            fixture.review_workspace_id
        ),
        json!({"summary": "Update from the owner", "verdict": "request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{republished}");
    let successors: Vec<&Value> = data(&republished)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|parent| parent["path"] == PER_EDIT_DOC && parent["kind"] == "hunk")
        .collect();
    assert_eq!(successors.len(), 1, "{republished}");
    let successor = successors[0];
    assert_eq!(successor["note"], fixture.parent["note"]);
    assert_eq!(
        document_edit_status(successor, rejected_id),
        "rejected",
        "hunk context changes must not reopen a rejected edit: {republished}"
    );
    let carried_rejection = document_edit(successor, "blue");
    assert_eq!(carried_rejection["commentId"], rejected["commentId"]);
    assert_eq!(carried_rejection["statusBy"], fixture.owner.id.to_string());
    assert!(carried_rejection["statusAt"].is_string());
    let (status, open) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=true",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{open}");
    assert!(
        data(&open)["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|parent| parent["path"] != PER_EDIT_DOC),
        "neither settled edit should appear as a fresh proposal: {open}"
    );
    let successor_id = successor["id"].as_str().unwrap();
    let (status, thread) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{successor_id}/replies?editId={rejected_id}",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{thread}");
    assert!(thread.to_string().contains("The fox must stay red."));
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let change = data(&preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == PER_EDIT_DOC)
        .unwrap()["changes"][0]
        .clone();
    assert_eq!(change["publishedIn"]["suggestionId"], successor["id"]);
    assert_eq!(
        change["publishedIn"]["publicationId"],
        successor["publicationId"]
    );
    assert_eq!(change["publishedIn"]["status"], "rejected");
    assert_eq!(change["carried"]["note"], fixture.parent["note"]);

    // A further round retains the rejected fox edit and adds a genuinely
    // new bridge edit inside the same Git hunk. Only one of its two units
    // carries a decision; neither a whole-parent marker nor its note may
    // claim that this newly combined proposal was already offered.
    let fresh_after = after.replacen("First unchanged bridge.", "First updated bridge.", 1);
    let fresh_review_head = commit_file_on(
        &repo_path,
        &updated_review_head,
        PER_EDIT_DOC,
        fresh_after.as_bytes(),
        "reviewer: add a nearby bridge edit after the fox decision",
    );
    helpers::seed_workspace_authority_head(
        &fixture.app,
        fixture.review_workspace_id,
        fixture.source.repo_id,
        &fresh_review_head,
    )
    .await;
    let (status, preview) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/preview",
            fixture.review_workspace_id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let change = data(&preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == PER_EDIT_DOC)
        .unwrap()["changes"][0]
        .clone();
    assert_eq!(change["editCount"], 2, "{preview}");
    assert_eq!(change["carriedEditCount"], 1, "{preview}");
    assert_eq!(change["carriedOpenEditCount"], 0, "{preview}");
    assert_eq!(
        change["editCount"].as_u64().unwrap() - change["carriedEditCount"].as_u64().unwrap(),
        1,
        "only the bridge edit is newly offered"
    );
    assert!(change["publishedIn"].is_null(), "{preview}");
    assert!(change["carried"].is_null(), "{preview}");
    let (status, fresh_round) = post(
        &reviewer,
        &format!(
            "/api/v1/workspaces/{}/review/publish",
            fixture.review_workspace_id
        ),
        json!({"summary": "One new bridge edit", "verdict": "request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{fresh_round}");
    let fresh_parent = data(&fresh_round)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parent| parent["path"] == PER_EDIT_DOC && parent["kind"] == "hunk")
        .unwrap();
    assert_eq!(document_edit_status(fresh_parent, rejected_id), "rejected");
    assert_eq!(
        document_edit(fresh_parent, "blue")["commentId"],
        rejected["commentId"]
    );
    let bridge = document_edit(fresh_parent, "updated");
    assert_eq!(bridge["status"], "open");
    assert_ne!(bridge["id"], rejected_id);
    assert_ne!(bridge["id"], accepted_id);
    assert_eq!(fresh_parent["status"], "open");
    assert_eq!(fresh_parent["note"], "");
    let (status, open) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=true",
            fixture.source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{open}");
    let open_edits: Vec<&Value> = data(&open)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|parent| parent["path"] == PER_EDIT_DOC)
        .flat_map(|parent| parent["edits"].as_array().unwrap())
        .filter(|edit| edit["status"] == "open")
        .collect();
    assert_eq!(open_edits.len(), 1, "{open}");
    assert_eq!(open_edits[0]["id"], bridge["id"]);
    assert_eq!(
        peer.file_content(fixture.source.repo_id, PER_EDIT_DOC)
            .await
            .as_deref(),
        Some(partial.as_str()),
        "Push review preserves the owner's independently decided document"
    );
    drop(peer);
    fixture.cleanup().await;
}

/// Surface server-side error logs (AppError logs internal errors at `error`)
/// in the test output so a 500 names its cause: the shared process-wide
/// subscriber carries that formatting layer.
fn init_tracing() {
    helpers::tracing_capture();
}

fn git_stdout(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {repo_path:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is utf-8")
        .trim()
        .to_string()
}

#[tokio::test]
async fn editor_save_reports_push_failure_and_plain_file_writes_remain_uncommitted() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let peer =
        TestWorkspaceRuntimePeer::attach_with_changes_failure(&app, &source, owner.id, "push")
            .await;
    let client = app.authenticated_client(&owner);
    let url = format!(
        "/api/v1/workspaces/{}/files/save.txt?repoId={}",
        source.id, source.repo_id
    );
    let missing_vm = client
        .put_json(
            &url,
            &json!({"content":"saved bytes\n","encoding":"utf-8","expectedHash":null,"commitAndPush":true}),
        )
        .await;
    assert_eq!(missing_vm.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let stale_vm = client
        .put_json(
            &url,
            &json!({"content":"saved bytes\n","encoding":"utf-8","expectedHash":null,"expectedVmId":Uuid::new_v4(),"commitAndPush":true}),
        )
        .await;
    assert_eq!(stale_vm.status(), StatusCode::CONFLICT);
    assert_eq!(peer.file_content(source.repo_id, "save.txt").await, None);
    let response = client
        .put_json(
            &url,
            &json!({"content":"saved bytes\n","encoding":"utf-8","expectedHash":null,"expectedVmId":peer.vm_id(),"commitAndPush":true}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        peer.file_content(source.repo_id, "save.txt")
            .await
            .as_deref(),
        Some("saved bytes\n")
    );
    let calls = peer.received_rpc_methods().await;
    assert!(calls.contains(&"modules.invoke:workspace.changes/commit".into()));
    assert!(calls.contains(&"modules.invoke:workspace.changes/push".into()));

    // The browser still has expectedHash=null after that failed response.
    // Exact same bytes retry the push instead of getting FILE_EXISTS.
    let retry = client
        .put_json(
            &url,
            &json!({"content":"saved bytes\n","encoding":"utf-8","expectedHash":null,"expectedVmId":peer.vm_id(),"commitAndPush":true}),
        )
        .await;
    let retry_status = retry.status();
    let retry_body: Value = retry.json().await.unwrap();
    assert_eq!(retry_status, StatusCode::CONFLICT, "{retry_body}");
    assert_ne!(retry_body["error"]["code"], "FILE_EXISTS", "{retry_body}");
    let retried_calls = peer.received_rpc_methods().await;
    assert_eq!(
        retried_calls
            .iter()
            .filter(|method| method.as_str() == "modules.invoke:workspace.changes/push")
            .count(),
        2,
    );

    let plain = client
        .put_json(
            &format!(
                "/api/v1/workspaces/{}/files/plain.txt?repoId={}",
                source.id, source.repo_id
            ),
            &json!({"content":"draft\n","encoding":"utf-8","expectedHash":null}),
        )
        .await;
    assert_eq!(plain.status(), StatusCode::OK);
    let after = peer.received_rpc_methods().await;
    assert_eq!(
        after
            .iter()
            .filter(|method| method.as_str() == "modules.invoke:workspace.changes/commit")
            .count(),
        2,
        "ordinary API writes do not enter the editor Save commit flow"
    );
}

/// A plain org member (not the workspace owner) with a token for `org`.
async fn create_member(app: &TestApp, org: &TestOrg) -> TestUser {
    let mut member = TestUser::create(app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .expect("insert member membership");
    member.jwt = helpers::test_jwt_minter()
        .mint(
            member.id,
            org.id,
            vec![
                Scope::WorkspacesRead,
                Scope::WorkspacesWrite,
                Scope::TasksRead,
                Scope::TasksExecute,
                Scope::SessionsRead,
                Scope::SessionsWrite,
                Scope::OrgRead,
            ],
            3600,
        )
        .expect("mint member token");
    member
}

async fn start_review(
    client: &helpers::AuthenticatedClient,
    source_id: Uuid,
) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/review/start"),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn review_context(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Value {
    let response = client
        .get(&format!("/api/v1/workspaces/{workspace_id}/review"))
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "review context read failed: {body}");
    body
}

/// A few bytes with a PNG signature and a NUL: binary to git and to the
/// text check, so a review that adds it publishes an artifact.
const MOCK_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00mock";

/// Land one commit that adds `path` with `content` on top of `parent` in the
/// repository mirror, the way a push would leave it, and return its OID. The
/// caller seeds the workspace branch authority with it.
fn commit_file_on(
    repo_path: &Path,
    parent: &str,
    path: &str,
    content: &[u8],
    message: &str,
) -> String {
    let blob = {
        let output = Command::new("git")
            .args(["hash-object", "-w", "--stdin"])
            .current_dir(repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn git hash-object");
        use std::io::Write;
        output
            .stdin
            .as_ref()
            .expect("stdin")
            .write_all(content)
            .expect("write blob");
        let output = output.wait_with_output().expect("git hash-object");
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    let index_file = repo_path.join(format!("test-index-{}", Uuid::new_v4()));
    let index = index_file.to_string_lossy().to_string();
    let git_env = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .env("GIT_INDEX_FILE", &index)
            .current_dir(repo_path)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git_env(&["read-tree", parent]);
    git_env(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("100644,{blob},{path}"),
    ]);
    let tree = git_env(&["write-tree"]);
    let commit = git_stdout(
        repo_path,
        &[
            "-c",
            "user.email=test@g5.test",
            "-c",
            "user.name=Test",
            "commit-tree",
            &tree,
            "-p",
            parent,
            "-m",
            message,
        ],
    );
    let _ = std::fs::remove_file(&index_file);
    commit
}

#[tokio::test]
async fn start_forks_a_reviewer_owned_review_workspace_and_reopen_reuses_it() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let reviewer = create_member(&app, &org).await;
    let reviewer_client = app.authenticated_client(&reviewer);
    let owner_client = app.authenticated_client(&owner);

    // The reviewer cannot write to the source directly (single-writer rule).
    let denied = reviewer_client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", source.id),
            &json!({}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    // Two overlapping Starts (a double click, two tabs) converge on one
    // review workspace: both compute the same fork generation and idempotency
    // key, so the fork service admits a single operation.
    let (first, second) = tokio::join!(
        start_review(&reviewer_client, source.id),
        start_review(&reviewer_client, source.id),
    );
    for (status, body) in [&first, &second] {
        assert!(
            matches!(*status, StatusCode::CREATED | StatusCode::OK),
            "overlapping start failed: {body}"
        );
    }
    assert_eq!(
        data(&first.1)["review"]["reviewWorkspaceId"],
        data(&second.1)["review"]["reviewWorkspaceId"],
        "overlapping starts must share one review workspace"
    );
    // Exactly one of them forked; which one wins the admission lock is not
    // deterministic.
    let created_count = [&first.1, &second.1]
        .iter()
        .filter(|body| data(body)["created"] == true)
        .count();
    assert_eq!(created_count, 1, "exactly one overlapping start forks");
    let (status, body) = if data(&first.1)["created"] == true {
        first
    } else {
        second
    };
    let review = &data(&body)["review"];
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(data(&body)["created"], true);
    assert_eq!(review["sourceWorkspaceId"], source.id.to_string());
    assert_eq!(review["sourceWorkspaceName"], source.name.as_str());
    assert_eq!(review["reviewerUserId"], reviewer.id.to_string());
    assert_eq!(
        review["reviewWorkspaceName"],
        format!("{}-review-test-user", source.name)
    );
    let review_workspace_id: Uuid = review["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("review workspace id");
    let (owner_user_id, merge_target, cloned_from): (Option<Uuid>, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT owner_user_id, merge_target_workspace_id, cloned_from_id \
             FROM workspaces WHERE id = $1",
        )
        .bind(review_workspace_id)
        .fetch_one(&app.db)
        .await
        .expect("load review workspace");
    assert_eq!(owner_user_id, Some(reviewer.id), "reviewer owns the fork");
    assert_eq!(merge_target, Some(source.id), "merge target is the source");
    assert_eq!(cloned_from, Some(source.id));
    let fork_key: String = sqlx::query_scalar(
        "SELECT idempotency_key FROM workspace_forks \
         WHERE target_workspace_id = $1 AND status = 'completed'",
    )
    .bind(review_workspace_id)
    .fetch_one(&app.db)
    .await
    .expect("completed review fork");
    assert!(
        fork_key.starts_with("review-workspace:"),
        "fork carries the review marker: {fork_key}"
    );

    // Reopen: no second fork, same review workspace.
    let (status, body) = start_review(&reviewer_client, source.id).await;
    assert_eq!(status, StatusCode::OK, "reopen failed: {body}");
    assert_eq!(data(&body)["created"], false);
    assert_eq!(
        data(&body)["review"]["reviewWorkspaceId"],
        review_workspace_id.to_string()
    );
    let fork_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_forks WHERE source_workspace_id = $1 AND requested_by = $2",
    )
    .bind(source.id)
    .bind(reviewer.id)
    .fetch_one(&app.db)
    .await
    .expect("count review forks");
    assert_eq!(fork_count, 1);

    // Context from the source: the reviewer sees their review, the owner none.
    let context = review_context(&reviewer_client, source.id).await;
    assert_eq!(
        data(&context)["asSource"]["reviewWorkspaceId"],
        review_workspace_id.to_string()
    );
    assert!(data(&context)["asReview"].is_null());
    let owner_context = review_context(&owner_client, source.id).await;
    assert!(data(&owner_context)["asSource"].is_null());
    assert!(data(&owner_context)["asReview"].is_null());

    // Context from the review workspace: it knows its origin.
    let review_context_body = review_context(&reviewer_client, review_workspace_id).await;
    assert_eq!(
        data(&review_context_body)["asReview"]["sourceWorkspaceId"],
        source.id.to_string()
    );
    assert_eq!(
        data(&review_context_body)["asReview"]["reviewerDisplayName"],
        "Test User"
    );

    // A deleted review workspace is not reopened: the next Start forks a
    // fresh copy under a new operation identity instead of replaying the
    // completed fork of the tombstoned workspace.
    sqlx::query("UPDATE workspaces SET deleted_at = now(), deleted_by = $2 WHERE id = $1")
        .bind(review_workspace_id)
        .bind(reviewer.id)
        .execute(&app.db)
        .await
        .expect("tombstone the review workspace");
    let gone = review_context(&reviewer_client, source.id).await;
    assert!(data(&gone)["asSource"].is_null());
    let (status, body) = start_review(&reviewer_client, source.id).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "start after delete failed: {body}"
    );
    assert_eq!(data(&body)["created"], true);
    let replacement_id = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .expect("replacement review workspace id")
        .to_string();
    assert_ne!(replacement_id, review_workspace_id.to_string());

    // A review workspace transferred away is no longer the reviewer's review.
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&replacement_id).unwrap())
        .bind(owner.id)
        .execute(&app.db)
        .await
        .expect("transfer the replacement review workspace");
    let transferred = review_context(&reviewer_client, source.id).await;
    assert!(
        data(&transferred)["asSource"].is_null(),
        "a transferred review workspace is not offered for reopen: {transferred}"
    );
}

async fn review_branch(app: &TestApp, review_workspace_id: Uuid, repo_id: Uuid) -> (Uuid, String) {
    let branch: String = sqlx::query_scalar("SELECT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id JOIN repos r ON r.id=m.repo_id WHERE m.workspace_id=$1 AND m.repo_id=$2")
        .bind(review_workspace_id).bind(repo_id).fetch_one(&app.db).await.unwrap();
    let head = g5_scm::GitRepo::open(&app.repo_path(repo_id).await)
        .unwrap()
        .read_branch_oid(&branch)
        .unwrap()
        .unwrap();
    (review_workspace_id, head)
}

async fn post(
    client: &helpers::AuthenticatedClient,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = client.post_json(path, &body).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn get(client: &helpers::AuthenticatedClient, path: &str) -> (StatusCode, Value) {
    let response = client.get(path).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn push_review_publishes_suggestions_the_owner_accepts_into_the_running_workspace() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    // A source file the review will change, present before the fork.
    let base_content = "fn main() {\n    println!(\"hello\");\n}\n";
    let source_commit = commit_file_on(
        &repo_path,
        &source.head_commit,
        "src/main.rs",
        base_content.as_bytes(),
        "add main",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &source_commit).await;

    let reviewer = create_member(&app, &org).await;
    let reviewer_client = app.authenticated_client(&reviewer);
    let owner_client = app.authenticated_client(&owner);

    let (status, body) = start_review(&reviewer_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "start review failed: {body}");
    let review_workspace_id: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("review workspace id");

    // The review changes main.rs and adds a notes file, pushed to the review branch.
    let (review_branch_id, review_head) =
        review_branch(&app, review_workspace_id, source.repo_id).await;
    assert_eq!(
        review_head, source_commit,
        "the fork starts at the source head"
    );
    let changed_content = "fn main() {\n    println!(\"hello, review\");\n}\n";
    let c1 = commit_file_on(
        &repo_path,
        &review_head,
        "src/main.rs",
        changed_content.as_bytes(),
        "review: greet",
    );
    let c2 = commit_file_on(&repo_path, &c1, "REVIEW.md", b"# Notes\n", "review: notes");
    // A binary the repository cannot take as a proposal still reaches the
    // owner: it becomes an artifact to open or download. (A text file such
    // as REVIEW.md is proposed as a new file; the owner decides.)
    let c3 = commit_file_on(&repo_path, &c2, "mock.png", MOCK_PNG, "review: mockup");
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c3).await;
    // The owner keeps working after the fork. Source-only commits are not
    // the reviewer's proposal: the push compares the review branch with the
    // common ancestor, never with the moved source head, so they must not
    // come back as inverse suggestions that would undo the owner's work.
    let owner_commit = commit_file_on(
        &repo_path,
        &source_commit,
        "OWNER.md",
        b"owner work after the fork\n",
        "owner: notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &owner_commit).await;

    // The reviewer's review workspace is running: Push review saves their
    // current work as a round (`workspace.changes` commit, then push) before
    // it diffs the review branch; the reviewer never touches git. The test
    // peer scripts both operations as succeeded; the branches seeded above
    // stand in for what the push would have published.
    let review_workspace_name: String =
        sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
            .bind(review_workspace_id)
            .fetch_one(&app.db)
            .await
            .expect("review workspace name");
    // Start review left a queued incarnation; the peer stands in for the
    // placed, running one.
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(review_workspace_id)
        .execute(&app.db)
        .await
        .expect("clear the queued review incarnation");
    // Discard from my copy writes the copy's working tree: without a
    // running copy it is refused before anything is saved or diffed.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/changes/discard"),
        json!({"keys": ["0123456789abcdef0123456789abcdef"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");
    let review_workspace = TestWorkspace {
        id: review_workspace_id,
        org_id: source.org_id,
        repo_id: source.repo_id,
        head_commit: c3.clone(),
        name: review_workspace_name,
    };
    // The peer answers every Changes operation with one committed member,
    // the way the module reports a save that made a commit.
    let review_peer = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &review_workspace,
        reviewer.id,
        json!([{"repo_id": source.repo_id, "committed": true, "head": c3, "files": 1}]),
    )
    .await;

    // A read-only copy is not saved on the reviewer's behalf: the save obeys
    // the copy's mutation policy like any Changes action, before any
    // commit or push reaches the VM.
    sqlx::query("UPDATE workspaces SET mutation_policy = 'read_only' WHERE id = $1")
        .bind(review_workspace_id)
        .execute(&app.db)
        .await
        .expect("set the review copy read-only");
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_READ_ONLY", "{body}");
    assert!(
        !review_peer
            .received_rpc_methods()
            .await
            .iter()
            .any(|m| m.starts_with("modules.invoke:workspace.changes/")),
        "a read-only copy is never committed or pushed"
    );
    sqlx::query("UPDATE workspaces SET mutation_policy = 'read_write' WHERE id = $1")
        .bind(review_workspace_id)
        .execute(&app.db)
        .await
        .expect("restore the review copy");

    // The dialog's preview saves the same way, so it shows the latest edits.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "preview failed: {body}");
    assert_eq!(data(&body)["saved"]["workspace"], "live", "{body}");
    assert_eq!(
        data(&body)["saved"]["committed"],
        true,
        "the flattened report's members carry the commit: {body}"
    );
    // The preview lists each change with its text and a key the reviewer's
    // panel uses to note or withhold it before pushing.
    let previewed_main = data(&body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/main.rs")
        .expect("src/main.rs is previewed")
        .clone();
    assert_eq!(previewed_main["kind"], "hunk");
    assert_eq!(previewed_main["changes"].as_array().unwrap().len(), 1);
    let main_change = &previewed_main["changes"][0];
    assert_eq!(main_change["kind"], "hunk");
    assert_eq!(main_change["anchorStart"], 1);
    assert!(main_change["afterLines"]
        .as_str()
        .unwrap()
        .contains("hello, review"));
    let main_change_key = main_change["key"].as_str().unwrap().to_string();
    assert_eq!(main_change_key.len(), 32);
    // The preview names where the copy stands against the source: the
    // reviewer's tools tell "the source moved on" from it.
    assert_eq!(
        data(&body)["baseCommitVector"][0]["sourceHead"],
        owner_commit,
        "{body}"
    );
    assert_eq!(
        data(&body)["baseCommitVector"][0]["mergeBase"],
        source_commit,
        "{body}"
    );
    let driven = review_peer.received_rpc_methods().await;
    let commit_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/commit")
        .expect("preview committed the review workspace");
    let push_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/push")
        .expect("preview pushed the review workspace");
    assert!(commit_at < push_at, "commit before push: {driven:?}");

    // Discard from my copy: the reverse of a previewed change, written into
    // the copy's running working tree. The hunk goes back to the source's
    // lines; the proposed new file is deleted. The copy is saved first,
    // like a preview, so the keys name the current tree.
    let notes_key = data(&body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "REVIEW.md")
        .and_then(|file| file["key"].as_str())
        .map(str::to_string)
        .expect("the new file carries its key");
    let artifact_key = data(&body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "mock.png")
        .and_then(|file| file["key"].as_str())
        .map(str::to_string)
        .expect("the artifact carries its key");
    assert!(
        previewed_main["key"].is_null(),
        "a changed file answers per hunk: {previewed_main}"
    );
    review_peer
        .seed_file(source.repo_id, "src/main.rs", changed_content)
        .await;
    review_peer
        .seed_file(source.repo_id, "REVIEW.md", "# Notes\n")
        .await;
    let discard_url = format!("/api/v1/workspaces/{review_workspace_id}/review/changes/discard");
    let (d_status, d_body) = post(&reviewer_client, &discard_url, json!({"keys": []})).await;
    assert_eq!(
        d_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "nothing named: {d_body}"
    );
    let (d_status, d_body) = post(
        &owner_client,
        &discard_url,
        json!({"keys": [main_change_key.clone()]}),
    )
    .await;
    assert_eq!(
        d_status,
        StatusCode::FORBIDDEN,
        "only the reviewer writes their copy: {d_body}"
    );
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [main_change_key.clone(), "0123456789abcdef0123456789abcdef"]}),
    )
    .await;
    assert_eq!(d_status, StatusCode::CONFLICT, "{d_body}");
    assert!(
        d_body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("open Push review again"),
        "a stale key refuses the whole request: {d_body}"
    );
    // An artifact's key is its bytes: replaced in the copy after the
    // preview, it no longer answers to the old key, so the replacement is
    // never deleted on the strength of a preview that never saw it.
    let replaced = commit_file_on(
        &repo_path,
        &c3,
        "mock.png",
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00other",
        "review: replace the mockup",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &replaced).await;
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [artifact_key.clone()]}),
    )
    .await;
    assert_eq!(
        d_status,
        StatusCode::CONFLICT,
        "the old artifact key is stale: {d_body}"
    );
    assert!(
        d_body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("open Push review again"),
        "{d_body}"
    );
    let (p_status, p_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(p_status, StatusCode::OK, "{p_body}");
    let replaced_key = data(&p_body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "mock.png")
        .and_then(|file| file["key"].as_str())
        .map(str::to_string)
        .expect("the replaced artifact carries a key");
    assert_ne!(
        replaced_key, artifact_key,
        "different bytes, different key: {p_body}"
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c3).await;
    // An artifact larger than one runtime reply is discarded on its git
    // blob id alone: the peer refuses a whole-file read of it, so a
    // successful discard proves no content was read.
    let big = format!("{}\n", "b".repeat(4 * 1024 * 1024));
    let c_big = commit_file_on(&repo_path, &c3, "big.txt", big.as_bytes(), "review: big");
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c_big).await;
    review_peer.seed_file(source.repo_id, "big.txt", &big).await;
    let (big_status, big_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(big_status, StatusCode::OK, "{big_body}");
    let big_file = data(&big_body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "big.txt")
        .cloned()
        .expect("the big file is previewed");
    assert_eq!(big_file["kind"], "artifact", "{big_body}");
    let big_key = big_file["key"].as_str().unwrap().to_string();
    let (big_status, big_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [big_key.clone()]}),
    )
    .await;
    assert_eq!(big_status, StatusCode::OK, "{big_body}");
    assert_eq!(
        data(&big_body)["discarded"].as_array().map(Vec::len),
        Some(1),
        "{big_body}"
    );
    assert_eq!(
        data(&big_body)["discarded"][0]["key"],
        big_key,
        "{big_body}"
    );
    assert!(
        review_peer
            .file_content(source.repo_id, "big.txt")
            .await
            .is_none(),
        "the large artifact is deleted from the copy"
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c3).await;
    // One file's write failing (its version moved under the discard) is
    // that file's outcome, not the request's: the others are still
    // discarded, and the response says what happened to each.
    review_peer
        .refuse_writes(source.repo_id, "src/main.rs")
        .await;
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [main_change_key.clone(), notes_key.clone()]}),
    )
    .await;
    assert_eq!(d_status, StatusCode::OK, "{d_body}");
    let discarded = data(&d_body)["discarded"].as_array().cloned().unwrap();
    assert_eq!(discarded.len(), 1, "{d_body}");
    assert_eq!(discarded[0]["key"], notes_key, "{d_body}");
    let skipped_now = data(&d_body)["skipped"].as_array().cloned().unwrap();
    assert_eq!(skipped_now.len(), 1, "{d_body}");
    assert_eq!(skipped_now[0]["key"], main_change_key, "{d_body}");
    assert!(
        skipped_now[0]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("changed in your copy while discarding"),
        "{d_body}"
    );
    assert_eq!(
        review_peer
            .file_content(source.repo_id, "src/main.rs")
            .await
            .as_deref(),
        Some(changed_content),
        "the refused file is untouched"
    );
    assert!(
        review_peer
            .file_content(source.repo_id, "REVIEW.md")
            .await
            .is_none(),
        "the other file was still discarded"
    );
    review_peer
        .allow_writes(source.repo_id, "src/main.rs")
        .await;
    // A read the runtime cannot serve (a disconnect, a lost disk) is that
    // file's outcome as well; the file the discard already handled stays
    // reported.
    review_peer
        .seed_file(source.repo_id, "REVIEW.md", "# Notes\n")
        .await;
    review_peer
        .refuse_reads(source.repo_id, "src/main.rs")
        .await;
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [main_change_key.clone(), notes_key.clone()]}),
    )
    .await;
    assert_eq!(d_status, StatusCode::OK, "{d_body}");
    let discarded = data(&d_body)["discarded"].as_array().cloned().unwrap();
    assert_eq!(discarded.len(), 1, "{d_body}");
    assert_eq!(discarded[0]["key"], notes_key, "{d_body}");
    let skipped_now = data(&d_body)["skipped"].as_array().cloned().unwrap();
    assert_eq!(skipped_now.len(), 1, "{d_body}");
    assert_eq!(skipped_now[0]["key"], main_change_key, "{d_body}");
    let read_reason = skipped_now[0]["reason"].as_str().unwrap_or_default();
    assert!(
        read_reason.starts_with("src/main.rs: ") && !read_reason.contains("no longer in your copy"),
        "the runtime's own word, not a missing file: {d_body}"
    );
    assert_eq!(
        review_peer
            .file_content(source.repo_id, "src/main.rs")
            .await
            .as_deref(),
        Some(changed_content),
        "the unreadable file is untouched"
    );
    review_peer.allow_reads(source.repo_id, "src/main.rs").await;
    review_peer
        .seed_file(source.repo_id, "REVIEW.md", "# Notes\n")
        .await;
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [main_change_key.clone(), notes_key.clone()]}),
    )
    .await;
    assert_eq!(d_status, StatusCode::OK, "{d_body}");
    let discarded = data(&d_body)["discarded"].as_array().cloned().unwrap();
    assert_eq!(discarded.len(), 2, "{d_body}");
    assert!(
        discarded.iter().any(|item| item["key"] == main_change_key
            && item["kind"] == "hunk"
            && item["path"] == "src/main.rs"),
        "{d_body}"
    );
    assert!(
        discarded.iter().any(|item| item["key"] == notes_key
            && item["kind"] == "new_file"
            && item["path"] == "REVIEW.md"),
        "{d_body}"
    );
    assert_eq!(
        data(&d_body)["skipped"].as_array().map(Vec::len),
        Some(0),
        "{d_body}"
    );
    assert_eq!(
        review_peer
            .file_content(source.repo_id, "src/main.rs")
            .await
            .as_deref(),
        Some(base_content),
        "the hunk went back to the source's lines in the copy"
    );
    assert!(
        review_peer
            .file_content(source.repo_id, "REVIEW.md")
            .await
            .is_none(),
        "the proposed new file is deleted from the copy"
    );
    let driven = review_peer.received_rpc_methods().await;
    assert!(
        driven.iter().any(|m| m == "fs.write") && driven.iter().any(|m| m == "fs.delete"),
        "the copy's tree was written and the file deleted: {driven:?}"
    );
    // A file that changed in the copy between the save and the read (an
    // editor, an agent) is no longer what any key describes: the discard
    // leaves it alone and says so, nothing written or deleted. Here the
    // scripted save never moves the copy's branch, so the live files
    // (already reverted above) differ from the saved tree the diff names.
    let writes_before = driven.iter().filter(|m| *m == "fs.write").count();
    let deletes_before = driven.iter().filter(|m| *m == "fs.delete").count();
    review_peer
        .seed_file(source.repo_id, "REVIEW.md", "# Notes, edited meanwhile\n")
        .await;
    let (d_status, d_body) = post(
        &reviewer_client,
        &discard_url,
        json!({"keys": [main_change_key.clone(), notes_key.clone()]}),
    )
    .await;
    assert_eq!(d_status, StatusCode::OK, "{d_body}");
    assert_eq!(
        data(&d_body)["discarded"].as_array().map(Vec::len),
        Some(0),
        "{d_body}"
    );
    let skipped_now = data(&d_body)["skipped"].as_array().cloned().unwrap();
    assert_eq!(skipped_now.len(), 2, "{d_body}");
    assert!(
        skipped_now.iter().all(|item| item["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("changed in your copy since the preview")),
        "{d_body}"
    );
    let driven = review_peer.received_rpc_methods().await;
    assert_eq!(
        driven.iter().filter(|m| *m == "fs.write").count(),
        writes_before,
        "a changed file is not written: {driven:?}"
    );
    assert_eq!(
        driven.iter().filter(|m| *m == "fs.delete").count(),
        deletes_before,
        "a changed file is not deleted: {driven:?}"
    );
    assert_eq!(
        review_peer
            .file_content(source.repo_id, "REVIEW.md")
            .await
            .as_deref(),
        Some("# Notes, edited meanwhile\n"),
        "the edited file stays"
    );

    // Push review.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({
            "summary": "First round",
            "verdict": "request_changes",
            "notes": {"src/main.rs": "a path note the change note outranks"},
            "changeNotes": {(main_change_key.clone()): "Greet the reviewer too"},
            "findings": [{"title": "Greeting is bland", "severity": "P2", "dimension": "UX", "paths": ["src/main.rs", "mock.png"]}]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "publish failed: {body}");
    let driven = review_peer.received_rpc_methods().await;
    assert_eq!(
        driven
            .iter()
            .filter(|m| *m == "modules.invoke:workspace.changes/commit")
            .count(),
        11,
        "publish saved the round again (after the three previews and the seven discards that \
         reached the save, the stale-key one included): {driven:?}"
    );
    // Update from the source saves the reviewer's work the same way before
    // it rebases, so work in progress never makes the rebase refuse a
    // "dirty" tree.
    let (update_status, update_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/changes/update"),
        json!({}),
    )
    .await;
    assert_eq!(
        update_status,
        StatusCode::OK,
        "update failed: {update_body}"
    );
    let driven = review_peer.received_rpc_methods().await;
    let last_commit = driven
        .iter()
        .rposition(|m| m == "modules.invoke:workspace.changes/commit")
        .expect("update saved the review workspace");
    let update_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/update")
        .expect("update rebased the review workspace");
    assert!(last_commit < update_at, "save before update: {driven:?}");

    let publication = &data(&body)["publication"];
    assert_eq!(publication["round"], 1);
    assert_eq!(publication["verdict"], "request_changes");
    assert_eq!(publication["authorKind"], "human");
    let findings = data(&body)["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0]["paths"],
        json!(["src/main.rs", "mock.png"]),
        "{body}"
    );
    let finding_id = findings[0]["id"].as_str().unwrap().to_string();
    let suggestions = data(&body)["suggestions"].as_array().expect("suggestions");
    assert_eq!(
        suggestions.len(),
        3,
        "one hunk, one repository new file and one artifact: {body}"
    );
    let artifact = suggestions
        .iter()
        .find(|s| s["kind"] == "artifact")
        .unwrap_or_else(|| panic!("artifact missing: {body}"));
    assert_eq!(artifact["path"], "mock.png");
    assert_eq!(artifact["bytes"], MOCK_PNG.len());
    assert!(artifact["contentType"]
        .as_str()
        .unwrap()
        .starts_with("image/png"));
    assert_eq!(artifact["blobOid"].as_str().unwrap().len(), 40);
    let artifact_id = artifact["id"].as_str().unwrap().to_string();

    // The next preview says which changes round 1 already carries: the
    // reviewer's panel marks them "In round 1" and counts only new ones.
    let round1_publication_id = publication["id"].as_str().unwrap().to_string();
    let round1_ids: Vec<String> = suggestions
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    let (preview_status, preview_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(preview_status, StatusCode::OK, "{preview_body}");
    let previewed = data(&preview_body)["files"].as_array().unwrap().clone();
    assert_eq!(previewed.len(), 3, "{preview_body}");
    for file in &previewed {
        let published = if file["kind"] == "artifact" {
            assert!(file["changes"].as_array().unwrap().is_empty());
            file["publishedIn"].clone()
        } else {
            file["changes"][0]["publishedIn"].clone()
        };
        assert_eq!(published["round"], 1, "{}: {preview_body}", file["path"]);
        assert_eq!(
            published["publicationId"], round1_publication_id,
            "{preview_body}"
        );
        assert!(
            round1_ids.contains(&published["suggestionId"].as_str().unwrap().to_string()),
            "{}: names its row of round 1: {preview_body}",
            file["path"]
        );
        assert_eq!(published["status"], "open", "{preview_body}");
        if file["kind"] == "new_file" {
            assert_eq!(
                file["publishedIn"], published,
                "a new file answers at the file level too: {preview_body}"
            );
        } else if file["kind"] == "hunk" {
            assert!(
                file["publishedIn"].is_null(),
                "a changed file answers per hunk: {preview_body}"
            );
        }
    }
    assert_eq!(
        previewed.iter().find(|f| f["path"] == "mock.png").unwrap()["publishedIn"]["suggestionId"],
        artifact_id,
        "the artifact is matched by its blob: {preview_body}"
    );

    assert!(
        suggestions.iter().all(|s| s["path"] != "OWNER.md"),
        "the owner's own commit must not be proposed back: {body}"
    );
    assert_eq!(
        publication["baseCommitVector"][0]["mergeBase"], source_commit,
        "compared against the fork point"
    );
    assert_eq!(
        publication["baseCommitVector"][0]["sourceHead"], owner_commit,
        "records the source head at push time"
    );
    assert!(
        data(&body)["skipped"].as_array().unwrap().is_empty(),
        "{body}"
    );
    let hunk = suggestions
        .iter()
        .find(|s| s["kind"] == "hunk")
        .expect("hunk");
    let new_file = suggestions
        .iter()
        .find(|s| s["kind"] == "new_file")
        .expect("new file");
    assert_eq!(hunk["path"], "src/main.rs");
    assert!(hunk["beforeLines"]
        .as_str()
        .unwrap()
        .contains("println!(\"hello\")"));
    assert!(hunk["afterLines"]
        .as_str()
        .unwrap()
        .contains("hello, review"));
    assert_eq!(hunk["anchorStart"], 1);
    assert_eq!(hunk["findingId"], finding_id);
    assert_eq!(hunk["note"], "Greet the reviewer too");
    assert!(
        hunk["commentId"].is_string(),
        "every suggestion has a thread"
    );
    assert_eq!(new_file["path"], "REVIEW.md");
    assert_eq!(new_file["afterLines"], "# Notes\n");
    assert!(new_file["findingId"].is_null());
    let hunk_id = hunk["id"].as_str().unwrap().to_string();
    let new_file_id = new_file["id"].as_str().unwrap().to_string();

    // The owner opens the artifact: the bytes come back with their type,
    // never through Git.
    let response = owner_client
        .get(&format!(
            "/api/v1/workspaces/{}/review/suggestions/{artifact_id}/artifact",
            source.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .starts_with("image/png"));
    assert_eq!(response.bytes().await.unwrap().as_ref(), MOCK_PNG);

    // The artifact lives outside the repository's Git object store, so
    // workspace access is the only way to it.
    let artifact_oid = artifact["blobOid"].as_str().unwrap().to_string();
    assert!(app
        .repo_storage
        .artifact_dir(
            &g5_api_core::services::git::repo_store::TenantRepoStorage::test_key(
                source.org_id,
                source.repo_id,
            ),
        )
        .expect("local artifact store")
        .join(&artifact_oid[..2])
        .join(&artifact_oid[2..4])
        .join(&artifact_oid)
        .is_file());

    // The owner sees the publication with counts and the review context's open count.
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = &data(&body)["publications"][0];
    assert_eq!(listed["openCount"], 3);
    assert_eq!(listed["round"], 1);
    assert_eq!(data(&body)["findings"][0]["status"], "open");
    let context = review_context(&owner_client, source.id).await;
    assert_eq!(data(&context)["openSuggestions"], 3);
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/suggestions/counts", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["paths"].as_array().unwrap().len(), 3);

    // The owner was notified of the publication.
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/notifications", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.to_string().contains("published review 1"),
        "owner notified of the publication: {body}"
    );

    // Accept needs the owner's VM: refused while stopped, nothing changes.
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{hunk_id}/accept",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING");

    // A failed push leaves the written file available for a selected-file
    // retry, but the applied suggestion is already a terminal decision.
    let peer =
        TestWorkspaceRuntimePeer::attach_with_changes_failure(&app, &source, owner.id, "push")
            .await;
    peer.seed_file(source.repo_id, "src/main.rs", base_content)
        .await;
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{hunk_id}/accept",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["suggestion"]["status"], "accepted");
    assert_eq!(data(&body)["checkpointPending"], true);
    assert_eq!(
        peer.file_content(source.repo_id, "src/main.rs")
            .await
            .as_deref(),
        Some(changed_content)
    );
    let (status, rejected) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{hunk_id}/reject",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");

    peer.allow_changes().await;
    let retry = owner_client
        .put_json(
            &format!(
                "/api/v1/workspaces/{}/files/src/main.rs?repoId={}",
                source.id, source.repo_id
            ),
            &json!({"content":changed_content,"encoding":"utf-8","expectedHash":null,"expectedVmId":peer.vm_id(),"commitAndPush":true}),
        )
        .await;
    let retry_status = retry.status();
    let retry_body: Value = retry.json().await.unwrap();
    assert_eq!(retry_status, StatusCode::OK, "retry failed: {retry_body}");
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{hunk_id}/accept",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "accepted proposal was retried: {body}"
    );
    assert_eq!(
        peer.file_content(source.repo_id, "src/main.rs")
            .await
            .as_deref(),
        Some(changed_content),
        "the hunk was written into the owner's working tree"
    );
    // A new file is created by accept.
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{new_file_id}/accept",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "accept new file failed: {body}");
    assert_eq!(
        peer.file_content(source.repo_id, "REVIEW.md")
            .await
            .as_deref(),
        Some("# Notes\n")
    );
    let owner_git_calls: Vec<String> = peer
        .received_rpc_methods()
        .await
        .into_iter()
        .filter(|method| {
            method == "modules.invoke:workspace.changes/commit"
                || method == "modules.invoke:workspace.changes/push"
        })
        .collect();
    assert_eq!(
        owner_git_calls,
        [
            "modules.invoke:workspace.changes/commit",
            "modules.invoke:workspace.changes/push",
            "modules.invoke:workspace.changes/commit",
            "modules.invoke:workspace.changes/push",
            "modules.invoke:workspace.changes/commit",
            "modules.invoke:workspace.changes/push",
        ],
        "failed pushes retry the same affected path before accepting"
    );

    // The finding settled to accepted; the reviewer was told. The finding
    // also names the mockup, whose artifact stays open (it has no accept):
    // supporting material never keeps a finding open once its fixes are in.
    let (_, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(data(&body)["findings"][0]["status"], "accepted", "{body}");
    assert_eq!(data(&body)["publications"][0]["acceptedCount"], 2);
    // The artifact has no accept: it stays open for the owner to read.
    assert_eq!(data(&body)["publications"][0]["openCount"], 1);
    let (artifact_status, artifact_finding): (String, Option<Uuid>) =
        sqlx::query_as("SELECT status, finding_id FROM review_suggestions WHERE id = $1")
            .bind(artifact_id.parse::<Uuid>().unwrap())
            .fetch_one(&app.db)
            .await
            .expect("artifact row");
    assert_eq!(
        (artifact_status.as_str(), artifact_finding),
        ("open", Some(finding_id.parse::<Uuid>().unwrap())),
        "the artifact is the finding's open supporting document"
    );
    // The decision is filed in the reviewer's copy: that is the inbox a
    // returning reviewer reads. W's inbox is the owner's page.
    let (_, body) = get(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/notifications"),
    )
    .await;
    assert!(
        body.to_string().contains("accepted your suggestion"),
        "reviewer notified of the accept in their copy: {body}"
    );
    let (_, body) = get(
        &reviewer_client,
        &format!("/api/v1/workspaces/{}/notifications", source.id),
    )
    .await;
    assert!(
        !body.to_string().contains("accepted your suggestion"),
        "the decision is not filed on W for the reviewer: {body}"
    );

    // Threads: the decision is recorded as a reply; people can reply too.
    let (status, body) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{hunk_id}/replies",
            source.id
        ),
        json!({"content": "Thanks!"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let replies = data(&body)["replies"].as_array().unwrap();
    assert!(replies
        .iter()
        .any(|r| r["content"].as_str().unwrap().starts_with("Accepted")));
    assert!(replies.iter().any(|r| r["content"] == "Thanks!"));

    // A second round supersedes the first. The reviewer edited the greeting
    // since, so that hunk is a new proposal and arrives open; REVIEW.md is
    // unchanged and the owner already accepted it, so its round-2 row is
    // born accepted (the owner is not asked twice); the artifact is reused.
    let edited_content = "fn main() {\n    println!(\"hello, review!\");\n}\n";
    let c4 = commit_file_on(
        &repo_path,
        &c3,
        "src/main.rs",
        edited_content.as_bytes(),
        "review: greet louder",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c4).await;
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({
            "summary": "Second round",
            "verdict": "approve",
            "notes": {"src/main.rs": "Louder greeting", "REVIEW.md": "Notes for the owner"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(data(&body)["publication"]["round"], 2);
    let round2_rows = data(&body)["suggestions"].as_array().unwrap().clone();
    let round2_hunk = round2_rows
        .iter()
        .find(|s| s["kind"] == "hunk")
        .unwrap_or_else(|| panic!("round 2 hunk: {body}"));
    assert_eq!(
        round2_hunk["status"], "open",
        "an edited hunk is new: {body}"
    );
    assert_eq!(round2_hunk["note"], "Louder greeting", "{body}");
    assert!(
        round2_hunk["afterLines"]
            .as_str()
            .unwrap()
            .contains("hello, review!"),
        "{body}"
    );
    let round2_new_file = round2_rows
        .iter()
        .find(|s| s["kind"] == "new_file")
        .unwrap_or_else(|| panic!("round 2 new file: {body}"));
    assert_eq!(
        round2_new_file["status"], "accepted",
        "an unchanged, accepted proposal keeps its decision: {body}"
    );
    assert_eq!(
        round2_new_file["statusBy"],
        owner.id.to_string(),
        "decided by the owner, not reopened: {body}"
    );
    assert!(round2_new_file["statusAt"].is_string(), "{body}");
    assert_eq!(round2_new_file["supersedes"], new_file_id, "{body}");
    assert_eq!(round2_new_file["note"], "Notes for the owner", "{body}");
    let round2_new_file_id = round2_new_file["id"].as_str().unwrap().to_string();
    let (_, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    let publications = data(&body)["publications"].as_array().unwrap();
    assert_eq!(publications.len(), 2);
    assert_eq!(publications[1]["round"], 1);
    assert!(
        publications[1]["supersededBy"].is_string(),
        "round 1 superseded by round 2: {body}"
    );
    assert_eq!(
        publications[0]["openCount"], 1,
        "only the edited hunk is open in round 2: {body}"
    );
    assert_eq!(publications[0]["acceptedCount"], 1, "{body}");

    // Round 2 retained the unchanged artifact without moving its row out of
    // round 1 (a re-push reuses the open row). The preview still knows it
    // is pushed: it names round 1 for the artifact and round 2 for the hunk
    // that round 2 re-published, so nothing here counts as new.
    let round2_publication_id = publications[0]["id"].as_str().unwrap().to_string();
    let (_, preview_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let previewed = data(&preview_body)["files"].as_array().unwrap().clone();
    let artifact_file = previewed
        .iter()
        .find(|f| f["path"] == "mock.png")
        .unwrap_or_else(|| panic!("artifact previewed: {preview_body}"));
    assert_eq!(
        artifact_file["publishedIn"]["publicationId"], round1_publication_id,
        "a retained artifact is in the round that first pushed it: {preview_body}"
    );
    assert_eq!(artifact_file["publishedIn"]["round"], 1, "{preview_body}");
    assert_eq!(
        artifact_file["publishedIn"]["suggestionId"], artifact_id,
        "{preview_body}"
    );
    let hunk_file = previewed
        .iter()
        .find(|f| f["kind"] == "hunk")
        .unwrap_or_else(|| panic!("hunk previewed: {preview_body}"));
    assert_eq!(
        hunk_file["changes"][0]["publishedIn"]["publicationId"], round2_publication_id,
        "the re-published hunk is in round 2: {preview_body}"
    );
    assert_eq!(
        hunk_file["changes"][0]["publishedIn"]["round"], 2,
        "{preview_body}"
    );

    // A withdrawn row is no longer the reviewer's proposal: the preview
    // offers that change as new again, so it can be pushed once more,
    // while the retained artifact stays in the round that pushed it.
    let withdrawn_id = hunk_file["changes"][0]["publishedIn"]["suggestionId"]
        .as_str()
        .unwrap()
        .to_string();
    let (withdraw_status, withdraw_body) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{withdrawn_id}/withdraw",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(withdraw_status, StatusCode::OK, "{withdraw_body}");
    let (again_status, again_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert_eq!(again_status, StatusCode::OK, "{again_body}");
    for file in data(&again_body)["files"].as_array().unwrap() {
        if file["kind"] == "hunk" {
            assert!(
                file["changes"][0]["publishedIn"].is_null(),
                "a withdrawn proposal is new again: {again_body}"
            );
        } else if file["kind"] == "artifact" {
            assert_eq!(
                file["publishedIn"]["round"], 1,
                "the artifact stays in round 1: {again_body}"
            );
        }
    }

    // A key the preview listed that the regenerated round no longer has
    // (the reviewer edited that change since) refuses the push: the note or
    // exclusion would land nowhere.
    let (stale_status, stale_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"excludedChanges": ["0123456789abcdef0123456789abcdef"]}),
    )
    .await;
    assert_eq!(stale_status, StatusCode::CONFLICT, "{stale_body}");
    assert!(
        stale_body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("open Push review again"),
        "{stale_body}"
    );

    // A round is about the source commit it reviewed. The publications
    // list names the source's current commit, and a file page (currentOnly)
    // shows a round's rows only while the source is still at that commit:
    // once the owner commits again, they leave the files and stay on the
    // Reviews page under their commit.
    let repo_key = source.repo_id.to_string();
    let current_head = data(&body)["sourceHeads"][&repo_key]
        .as_str()
        .expect("the source's current head")
        .to_string();
    assert_eq!(current_head.len(), 40);
    // When that commit was made, so a decision can be placed before or
    // after the source's last commit.
    let head_time_before = data(&body)["sourceHeadTimes"][&repo_key]
        .as_str()
        .expect("the source head's commit time")
        .to_string();
    let parsed_before = chrono::DateTime::parse_from_rfc3339(&head_time_before)
        .unwrap_or_else(|e| panic!("RFC 3339: {head_time_before}: {e}"));
    let (_, again) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(
        data(&again)["sourceHeadTimes"][&repo_key],
        head_time_before,
        "the time is the commit's, not the read's"
    );
    assert_eq!(
        publications[0]["baseCommitVector"][0]["sourceHead"], current_head,
        "round 2 reviewed the current commit"
    );
    let (_, current_rows) = get(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?currentOnly=true",
            source.id
        ),
    )
    .await;
    assert!(
        data(&current_rows)["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["publicationId"] == publications[0]["id"]),
        "round 2's rows are current: {current_rows}"
    );
    // Each listed row names the reviewer who published it, so the card in
    // the file can offer that reviewer Withdraw
    // (g5.governance.review-suggestions.display).
    for row in data(&current_rows)["suggestions"].as_array().unwrap() {
        let publication = publications
            .iter()
            .find(|publication| publication["id"] == row["publicationId"])
            .expect("the row's publication is listed");
        assert_eq!(
            row["reviewerUserId"], publication["reviewerUserId"],
            "row {row} names its reviewer"
        );
        assert!(row["reviewerUserId"].is_string(), "row {row}");
    }
    let owner_again = commit_file_on(
        &repo_path,
        &current_head,
        "OWNER-AGAIN.md",
        b"handled the review\n",
        "owner: after the review",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &owner_again).await;
    let (_, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(data(&body)["sourceHeads"][&repo_key], owner_again);
    let head_time_after = data(&body)["sourceHeadTimes"][&repo_key]
        .as_str()
        .expect("the new head's commit time")
        .to_string();
    let parsed_after = chrono::DateTime::parse_from_rfc3339(&head_time_after)
        .unwrap_or_else(|e| panic!("RFC 3339: {head_time_after}: {e}"));
    assert!(
        parsed_after >= parsed_before,
        "the new commit is not older than the one before: {head_time_after} vs {head_time_before}"
    );
    // Round 2 reviewed the previous commit: the preview marks nothing as
    // "in round 2" any more, so the reviewer's next push starts under the
    // new commit with every change counted as new.
    let (_, preview_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    assert!(
        data(&preview_body)["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| {
                file["publishedIn"].is_null()
                    && file["changes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|change| change["publishedIn"].is_null())
            }),
        "after the owner's commit no round is current: {preview_body}"
    );
    // Yet a push would still continue the unchanged proposal and inherit
    // its note: the preview says so apart from currency, so the panel can
    // show and clear that note. The hunk's round-2 row was withdrawn, so
    // nothing is carried for it.
    let previewed = data(&preview_body)["files"].as_array().unwrap().clone();
    let notes_file = previewed
        .iter()
        .find(|f| f["path"] == "REVIEW.md")
        .cloned()
        .unwrap_or_else(|| panic!("REVIEW.md previewed: {preview_body}"));
    assert_eq!(
        notes_file["carried"]["note"], "Notes for the owner",
        "a push carries the note across the owner's commit: {preview_body}"
    );
    assert_eq!(
        notes_file["carried"]["suggestionId"], round2_new_file_id,
        "{preview_body}"
    );
    assert_eq!(notes_file["carried"]["round"], 2, "{preview_body}");
    assert_eq!(
        notes_file["changes"][0]["carried"], notes_file["carried"],
        "a new file answers at both levels: {preview_body}"
    );
    let main_file = previewed
        .iter()
        .find(|f| f["path"] == "src/main.rs")
        .cloned()
        .unwrap_or_else(|| panic!("src/main.rs previewed: {preview_body}"));
    assert!(
        main_file["changes"][0]["carried"].is_null(),
        "a withdrawn row is not continued: {preview_body}"
    );
    let (_, current_rows) = get(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?currentOnly=true",
            source.id
        ),
    )
    .await;
    assert!(
        data(&current_rows)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "after the owner's next commit no earlier row is current: {current_rows}"
    );
    let (_, all_rows) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/suggestions", source.id),
    )
    .await;
    assert!(
        !data(&all_rows)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the Reviews page keeps them: {all_rows}"
    );
    // The owner keeps the round's artifact: no VM needed, it becomes a kept
    // review artifact (kind=artifact, status=accepted), which outlives the
    // round and the commit; the file badge on it is gone.
    let (status, kept) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{artifact_id}/accept",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{kept}");
    assert_eq!(data(&kept)["suggestion"]["status"], "accepted");
    assert!(data(&kept)["writtenHash"].is_null());
    let (_, kept_list) = get(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?kind=artifact&status=accepted",
            source.id
        ),
    )
    .await;
    let kept_ids: Vec<&str> = data(&kept_list)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert_eq!(kept_ids, vec![artifact_id.as_str()], "{kept_list}");
    let (_, counts) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/suggestions/counts", source.id),
    )
    .await;
    assert!(
        data(&counts)["paths"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["path"] != "mock.png"),
        "a kept artifact carries no file badge: {counts}"
    );

    // Pushing the kept artifact again with the same bytes keeps it kept:
    // the new row is born accepted and continues the kept row.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "Same mockup again"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let same_bytes = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("the artifact is pushed again: {body}"));
    assert_eq!(same_bytes["status"], "accepted", "still kept: {body}");
    assert_eq!(same_bytes["statusBy"], owner.id.to_string(), "{body}");
    assert_eq!(same_bytes["supersedes"], artifact_id, "{body}");
    let same_bytes_id = same_bytes["id"].as_str().unwrap().to_string();
    let kept_round = data(&body)["publication"]["round"].as_i64().unwrap();

    // The preview knows the kept artifact is in the round: it names the
    // kept row with its status instead of counting the file as new again.
    let (_, preview_body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let previewed_artifact = data(&preview_body)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "mock.png")
        .cloned()
        .unwrap_or_else(|| panic!("artifact previewed: {preview_body}"));
    assert_eq!(
        previewed_artifact["publishedIn"]["status"], "accepted",
        "a kept artifact is in its round, kept: {preview_body}"
    );
    assert_eq!(
        previewed_artifact["publishedIn"]["suggestionId"], same_bytes_id,
        "{preview_body}"
    );
    assert_eq!(
        previewed_artifact["publishedIn"]["round"], kept_round,
        "{preview_body}"
    );

    // Different bytes are a new proposal: the owner decided on the old
    // ones, so the replacement arrives open, continuing the same thread.
    let c5 = commit_file_on(
        &repo_path,
        &c4,
        "mock.png",
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00mock-v2",
        "review: mockup v2",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c5).await;
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "Mockup v2"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let changed = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("the changed artifact is pushed: {body}"));
    assert_eq!(changed["status"], "open", "new bytes, new decision: {body}");
    assert!(changed["statusBy"].is_null(), "{body}");
    assert_eq!(
        changed["supersedes"], same_bytes_id,
        "continues the thread of the kept row: {body}"
    );
    assert_ne!(changed["blobOid"], same_bytes["blobOid"], "{body}");

    // The owner rejects the new bytes and the reviewer restores the old
    // ones. The latest row at the path is the rejected one with other
    // bytes, so the restored file is a new proposal: the preview says so
    // (the historical kept rows with these bytes do not make it pushed),
    // and the push creates an open row after the rejected one.
    let changed_id = changed["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{changed_id}/reject",
            source.id
        ),
        json!({"reason": "The first mockup was better"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, inbox) = get(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/notifications"),
    )
    .await;
    assert!(
        inbox.to_string().contains("rejected your suggestion")
            && inbox.to_string().contains("The first mockup was better"),
        "the rejection and its reason reach the reviewer's copy: {inbox}"
    );
    let c6 = commit_file_on(
        &repo_path,
        &c5,
        "mock.png",
        MOCK_PNG,
        "review: mockup v1 again",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c6).await;
    let (_, restored_preview) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let restored = data(&restored_preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "mock.png")
        .cloned()
        .unwrap_or_else(|| panic!("artifact previewed: {restored_preview}"));
    assert!(
        restored["publishedIn"].is_null(),
        "restored bytes are a new proposal, not the kept history: {restored_preview}"
    );
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "Mockup v1 again"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let restored_row = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("the restored artifact is pushed: {body}"));
    assert_eq!(restored_row["status"], "open", "{body}");
    assert!(restored_row["statusBy"].is_null(), "{body}");
    assert_eq!(restored_row["supersedes"], changed_id, "{body}");

    // The owner commits again: the restored artifact's round is outdated.
    // Its open row cannot be reused (a push refuses an outdated one), so the
    // preview says the artifact is new, and the push creates a new open row.
    let restored_id = restored_row["id"].as_str().unwrap().to_string();
    let owner_third = commit_file_on(
        &repo_path,
        &owner_again,
        "OWNER-THIRD.md",
        b"and again\n",
        "owner: once more",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &owner_third).await;
    let (_, outdated_preview) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let outdated = data(&outdated_preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "mock.png")
        .cloned()
        .unwrap_or_else(|| panic!("artifact previewed: {outdated_preview}"));
    assert!(
        outdated["publishedIn"].is_null(),
        "an open row of an outdated round is not reused: {outdated_preview}"
    );
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "After the owner's next commit"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let fresh = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("the artifact is pushed anew: {body}"));
    assert_eq!(fresh["status"], "open", "{body}");
    assert!(fresh["statusBy"].is_null(), "{body}");
    assert_eq!(fresh["supersedes"], restored_id, "{body}");
    assert_ne!(fresh["id"], restored_id, "{body}");

    // A rejected, B pushed and withdrawn, A restored: the withdrawn row is
    // no proposal, so the latest row at the path is A's rejected one. The
    // preview says so (in the round, rejected; a push carries that row),
    // and the push is born rejected.
    let fresh_id = fresh["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{fresh_id}/reject",
            source.id
        ),
        json!({"reason": "Still the first mockup"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c7 = commit_file_on(
        &repo_path,
        &c6,
        "mock.png",
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00mock-v2",
        "review: mockup v2 once more",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c7).await;
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "Mockup v2 once more"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let b_row = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("B pushed: {body}"));
    assert_eq!(b_row["status"], "open", "{body}");
    let b_id = b_row["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{b_id}/withdraw",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c8 = commit_file_on(
        &repo_path,
        &c7,
        "mock.png",
        MOCK_PNG,
        "review: mockup v1 restored",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c8).await;
    let (_, a_preview) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let a_file = data(&a_preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "mock.png")
        .cloned()
        .unwrap_or_else(|| panic!("artifact previewed: {a_preview}"));
    assert_eq!(
        a_file["publishedIn"]["status"], "rejected",
        "the withdrawn B does not hide A's rejection: {a_preview}"
    );
    assert_eq!(
        a_file["publishedIn"]["suggestionId"], fresh_id,
        "{a_preview}"
    );
    assert_eq!(a_file["carried"]["suggestionId"], fresh_id, "{a_preview}");
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"summary": "Mockup v1 restored"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let a_again = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "artifact")
        .cloned()
        .unwrap_or_else(|| panic!("A pushed again: {body}"));
    assert_eq!(
        a_again["status"], "rejected",
        "carries A's rejection: {body}"
    );
    assert_eq!(a_again["statusBy"], owner.id.to_string(), "{body}");
    assert_eq!(a_again["supersedes"], fresh_id, "{body}");
}

#[tokio::test]
async fn owner_rejects_while_review_workspaces_are_restricted() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let reviewer = create_member(&app, &org).await;
    let reviewer_client = app.authenticated_client(&reviewer);
    let owner_client = app.authenticated_client(&owner);
    // A source file of repeated blocks, present before the fork: the same
    // edit to two of its blocks is the same text at two places.
    let block = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\niota\nkappa\n";
    let blocks = block.repeat(5);
    let with_blocks = commit_file_on(
        &repo_path,
        &source.head_commit,
        "blocks.txt",
        blocks.as_bytes(),
        "add blocks",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &with_blocks).await;

    let (status, body) = start_review(&reviewer_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "start review failed: {body}");
    let review_workspace_id: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("review workspace id");
    let (review_branch_id, review_head) =
        review_branch(&app, review_workspace_id, source.repo_id).await;
    assert_eq!(
        review_head, with_blocks,
        "the fork starts at the source head"
    );
    let c1 = commit_file_on(
        &repo_path,
        &review_head,
        "docs/plan.md",
        b"# Plan\n",
        "review: plan",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c1).await;
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"notes": {"docs/plan.md": "Plans belong here"}}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        data(&body)["suggestions"][0]["note"],
        "Plans belong here",
        "{body}"
    );
    let suggestion_id = data(&body)["suggestions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Only the owner decides; the reviewer cannot accept or reject.
    let (status, _) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/reject",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/reject",
            source.id
        ),
        json!({"reason": "We keep plans in the wiki"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["suggestion"]["status"], "rejected");
    let (_, body) = get(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/replies",
            source.id
        ),
    )
    .await;
    assert!(
        body.to_string()
            .contains("Rejected: We keep plans in the wiki"),
        "{body}"
    );

    // Pushing again with the same content does not reopen the decision: the
    // round-2 row carries the rejection, so the owner is not asked twice and
    // the round shows nothing open.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let again = &data(&body)["suggestions"][0];
    assert_eq!(again["status"], "rejected", "{body}");
    // The panel's drafts are cleared by a push, so this push named no note:
    // the row keeps the one the proposal had.
    assert_eq!(again["note"], "Plans belong here", "note carried: {body}");
    assert_eq!(again["statusBy"], owner.id.to_string(), "{body}");
    assert!(again["statusAt"].is_string(), "{body}");
    assert_eq!(again["supersedes"], suggestion_id, "{body}");
    let (_, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(data(&body)["publications"][0]["round"], 2, "{body}");
    assert_eq!(data(&body)["publications"][0]["openCount"], 0, "{body}");
    assert_eq!(data(&body)["publications"][0]["rejectedCount"], 1, "{body}");

    // The same edit to another occurrence of a repeated block is the same
    // text at another place: the owner's decision on the first occurrence
    // does not follow it. Round 3 edits block 2 and the owner rejects it;
    // round 4 reverts that and edits block 3 instead, and that row is open.
    let edit_blocks = |indices: &[usize]| -> String {
        let mut lines: Vec<String> = blocks.lines().map(str::to_string).collect();
        for index in indices {
            lines[index * 10 + 2] = "GAMMA".to_string();
        }
        format!("{}\n", lines.join("\n"))
    };
    let edit_block = |index: usize| edit_blocks(&[index]);
    let c2 = commit_file_on(
        &repo_path,
        &c1,
        "blocks.txt",
        edit_block(1).as_bytes(),
        "review: shout in block 2",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c2).await;
    // An explicit empty note is the reviewer clearing it: it replaces the
    // carried one.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"notes": {"docs/plan.md": ""}}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let plan_again = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "docs/plan.md")
        .cloned()
        .unwrap_or_else(|| panic!("plan row: {body}"));
    assert_eq!(plan_again["note"], "", "note cleared: {body}");
    assert_eq!(plan_again["status"], "rejected", "{body}");
    let block2 = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "blocks.txt")
        .cloned()
        .unwrap_or_else(|| panic!("block 2 hunk: {body}"));
    assert_eq!(block2["status"], "open", "{body}");
    let block2_id = block2["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{block2_id}/reject",
            source.id
        ),
        json!({"reason": "Not in block 2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c3 = commit_file_on(
        &repo_path,
        &c2,
        "blocks.txt",
        edit_block(2).as_bytes(),
        "review: shout in block 3 instead",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c3).await;
    // Before the push, the preview already treats the moved edit as a new
    // proposal: the same text at another anchor is not "in round 3".
    let (_, moved_preview) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/preview"),
        json!({}),
    )
    .await;
    let moved = data(&moved_preview)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "blocks.txt")
        .cloned()
        .unwrap_or_else(|| panic!("blocks.txt previewed: {moved_preview}"));
    assert!(
        moved["changes"][0]["publishedIn"].is_null(),
        "the moved edit is new; block 2's row is not referenced: {moved_preview}"
    );
    assert!(
        moved["changes"][0]["carried"].is_null(),
        "a push would continue nothing for the moved edit: {moved_preview}"
    );
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let block3 = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "blocks.txt")
        .cloned()
        .unwrap_or_else(|| panic!("block 3 hunk: {body}"));
    assert_eq!(
        block3["beforeLines"], block2["beforeLines"],
        "the same text: {body}"
    );
    assert_eq!(block3["afterLines"], block2["afterLines"], "{body}");
    assert_ne!(
        block3["anchorStart"], block2["anchorStart"],
        "at another place: {body}"
    );
    assert_eq!(
        block3["status"], "open",
        "the rejection of block 2 does not follow the edit to block 3: {body}"
    );
    assert!(block3["statusBy"].is_null(), "{body}");

    // Round 5 notes the block-3 edit and the owner rejects it. Round 6 adds
    // the identical edit to block 2, generated first in the file: it must
    // not take the block-3 row's thread by nearest anchor. The unchanged
    // block-3 row keeps its rejection and note; block 2 arrives open.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({"notes": {"blocks.txt": "Shout here"}}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let noted = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "blocks.txt")
        .cloned()
        .unwrap_or_else(|| panic!("block 3 row of round 5: {body}"));
    assert_eq!(noted["status"], "open", "{body}");
    assert_eq!(noted["note"], "Shout here", "{body}");
    let noted_id = noted["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{noted_id}/reject",
            source.id
        ),
        json!({"reason": "Not in block 3 either"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let c4 = commit_file_on(
        &repo_path,
        &c3,
        "blocks.txt",
        edit_blocks(&[1, 2]).as_bytes(),
        "review: shout in blocks 2 and 3",
    );
    helpers::seed_workspace_authority_head(&app, review_branch_id, source.repo_id, &c4).await;
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/review/publish"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let block_rows: Vec<Value> = data(&body)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["path"] == "blocks.txt")
        .cloned()
        .collect();
    assert_eq!(block_rows.len(), 2, "two separate hunks: {body}");
    let kept_block3 = block_rows
        .iter()
        .find(|s| s["anchorStart"] == block3["anchorStart"])
        .unwrap_or_else(|| panic!("block 3 row of round 6: {body}"));
    let new_block2 = block_rows
        .iter()
        .find(|s| s["anchorStart"] != block3["anchorStart"])
        .unwrap_or_else(|| panic!("block 2 row of round 6: {body}"));
    assert_eq!(
        new_block2["beforeLines"], kept_block3["beforeLines"],
        "the same text: {body}"
    );
    assert!(
        new_block2["anchorStart"].as_i64().unwrap() < kept_block3["anchorStart"].as_i64().unwrap(),
        "block 2 comes first in the file: {body}"
    );
    assert_eq!(
        kept_block3["status"], "rejected",
        "kept its decision: {body}"
    );
    assert_eq!(kept_block3["statusBy"], owner.id.to_string(), "{body}");
    assert_eq!(kept_block3["note"], "Shout here", "kept its note: {body}");
    assert_eq!(kept_block3["supersedes"], noted_id, "{body}");
    assert_eq!(new_block2["status"], "open", "a new proposal: {body}");
    assert!(new_block2["statusBy"].is_null(), "{body}");
    assert_eq!(new_block2["note"], "", "{body}");

    // A review workspace is lightweight: it cannot be forked, transferred or
    // used as a pull request source, and cannot itself be reviewed.
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/clone"),
        json!({"name": "nested-review", "idempotencyKey": Uuid::new_v4().to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "REVIEW_WORKSPACE_RESTRICTED",
        "{body}"
    );
    let (status, body) = post(
        &reviewer_client,
        &format!("/api/v1/workspaces/{review_workspace_id}/pull-requests"),
        json!({"targetWorkspaceId": source.id}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "REVIEW_WORKSPACE_RESTRICTED",
        "{body}"
    );
    let peer = attach_copy_peer(
        &app,
        review_workspace_id,
        source.repo_id,
        reviewer.id,
        &review_head,
    )
    .await;
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(
            org.id,
            review_workspace_id,
            reviewer.id,
            peer.vm_id(),
            1,
            3600,
        )
        .unwrap();
    let response = app
        .client
        .post(format!(
            "{}/api/internal/workspaces/{review_workspace_id}/pull-requests",
            app.base_url
        ))
        .bearer_auth(token)
        .json(&json!({"targetWorkspaceId":source.id}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "REVIEW_WORKSPACE_RESTRICTED",
        "{body}"
    );
    assert!(
        peer.received_rpc_methods().await.is_empty(),
        "review refusal must precede source save"
    );
    let (status, body) = start_review(&owner_client, review_workspace_id).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["details"]["code"], "REVIEW_WORKSPACE_RESTRICTED",
        "{body}"
    );

    // The workspace list still tags the review workspace.
    let listing = owner_client
        .get(&format!("/api/v1/orgs/{}/workspaces", org.id))
        .await;
    let listing: Value = listing.json().await.unwrap_or(Value::Null);
    let is_review = |id: &str| -> Option<bool> {
        data(&listing)
            .as_array()
            .into_iter()
            .flatten()
            .find(|ws| ws["id"] == id)
            .and_then(|ws| ws["isReview"].as_bool())
    };
    assert_eq!(is_review(&review_workspace_id.to_string()), Some(true));
    assert_eq!(is_review(&source.id.to_string()), Some(false));
}

/// Push `sha` to `refs/heads/<branch>` of the workspace's repository over
/// the smart-HTTP route as `jwt`, the way the owner's VM (or laptop) does,
/// so the push runs through `receive_pack` and what it tells reviewers.
async fn push_branch_as(
    app: &TestApp,
    repo_path: &Path,
    repo_id: Uuid,
    jwt: &str,
    sha: &str,
    branch: &str,
) {
    // Asynchronous: the server answering the push runs on this same test
    // runtime, so a blocking child would wait for it forever.
    let output = tokio::process::Command::new("git")
        .args([
            "-c",
            &format!("http.extraHeader=Authorization: Bearer {jwt}"),
            "push",
            "--quiet",
            &format!("{}/git/{repo_id}", app.base_url),
            &format!("{sha}:refs/heads/{branch}"),
        ])
        .current_dir(repo_path)
        .output()
        .await
        .expect("run git push");
    assert!(
        output.status.success(),
        "push failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The notifications a user holds on `workspace_id`, as the list route
/// returns them.
async fn notifications_of(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Vec<Value> {
    let (status, body) = get(
        client,
        &format!("/api/v1/workspaces/{workspace_id}/notifications"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = data(&body)
        .as_array()
        .cloned()
        .or_else(|| data(&body)["notifications"].as_array().cloned())
        .or_else(|| data(&body)["items"].as_array().cloned())
        .unwrap_or_default();
    items
}

#[tokio::test]
async fn owner_commits_from_the_reviews_page_and_reviewers_learn_the_base_moved() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let owner_client = app.authenticated_client(&owner);

    // Two reviewers each hold a live copy of W.
    let riley = create_member(&app, &org).await;
    let sam = create_member(&app, &org).await;
    let riley_client = app.authenticated_client(&riley);
    let sam_client = app.authenticated_client(&sam);
    let (status, body) = start_review(&riley_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let riley_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("riley's copy");
    let (status, body) = start_review(&sam_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let sam_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("sam's copy");

    let commit_url =
        |workspace_id: Uuid| format!("/api/v1/workspaces/{workspace_id}/changes/commit");

    // Only the owner commits W; a reviewer is refused.
    let (status, body) = post(
        &riley_client,
        &commit_url(source.id),
        json!({"message": "not mine"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // A review copy is saved by Push review; it has no commit of its own.
    let (status, body) = post(
        &riley_client,
        &commit_url(riley_copy),
        json!({"message": "saving my review"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Push review"),
        "{body}"
    );
    // A commit needs a message, checked before the VM is asked anything.
    let (status, body) = post(&owner_client, &commit_url(source.id), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = post(
        &owner_client,
        &commit_url(source.id),
        json!({"message": "   "}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // And a running VM: the commit happens in the working tree.
    let (status, body) = post(
        &owner_client,
        &commit_url(source.id),
        json!({"message": "Handled round 1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_NOT_RUNNING", "{body}");

    // With the owner's VM attached, Commit is `workspace.changes` commit
    // with the message, then push: the branch tip moves.
    let handled = commit_file_on(
        &repo_path,
        &source.head_commit,
        "HANDLED.md",
        b"round 1 handled\n",
        "Handled round 1",
    );
    let peer = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &source,
        owner.id,
        json!([{"repo_id": source.repo_id, "committed": true, "head": handled, "files": 2}]),
    )
    .await;
    let (status, body) = post(
        &owner_client,
        &commit_url(source.id),
        json!({"message": "Handled round 1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let members = data(&body)["members"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(members.len(), 1, "{body}");
    assert_eq!(members[0]["committed"], true, "{body}");
    assert_eq!(members[0]["pushed"], true, "{body}");
    assert_eq!(members[0]["head"], handled, "{body}");
    assert_eq!(data(&body)["status"], "succeeded", "{body}");
    let driven = peer.received_rpc_methods().await;
    let commit_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/commit")
        .expect("the VM committed");
    let push_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/push")
        .expect("the VM pushed");
    assert!(commit_at < push_at, "commit before push: {driven:?}");

    // The peer's push is scripted; the real one reaches the repository's
    // receive-pack, and that is what tells every reviewer W has a new
    // commit: their copy wants an Update, their rounds so far are outdated.
    // The notice lives in the copy's inbox, where a reviewer who was away
    // reads it on reopening the copy.
    assert!(
        notifications_of(&riley_client, riley_copy)
            .await
            .iter()
            .all(|n| !n.to_string().contains("has a new commit")),
        "nothing before the branch moves"
    );
    push_branch_as(
        &app,
        &repo_path,
        source.repo_id,
        &owner.jwt,
        &handled,
        &source.name,
    )
    .await;
    for (client, copy, who) in [
        (&riley_client, riley_copy, "riley"),
        (&sam_client, sam_copy, "sam"),
    ] {
        let notices: Vec<Value> = notifications_of(client, copy)
            .await
            .into_iter()
            .filter(|n| {
                n["title"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("has a new commit")
            })
            .collect();
        assert_eq!(
            notices.len(),
            1,
            "{who} told once, in their copy: {notices:?}"
        );
        assert!(
            notifications_of(client, source.id)
                .await
                .iter()
                .all(|n| !n.to_string().contains("has a new commit")),
            "{who}: the source's inbox is the owner's page, not where the reviewer reads"
        );
        let notice = &notices[0];
        assert_eq!(
            notice["title"],
            format!("{} has a new commit", source.name),
            "{notice}"
        );
        let message = notice["message"].as_str().unwrap_or_default();
        assert!(
            message.contains(&handled[..7]),
            "{who}: names the commit: {notice}"
        );
        assert!(
            message.contains("Update your review copy"),
            "{who}: {notice}"
        );
        assert!(message.contains("outdated"), "{who}: {notice}");
        assert_eq!(
            notice["actions"][0]["target"],
            format!("workspace:{copy}?view=review"),
            "{who}: opens their own copy: {notice}"
        );
        assert_eq!(notice["actions"][0]["action"], "navigate", "{notice}");
    }
    // The owner moved the branch; they are not told about their own commit.
    assert!(
        notifications_of(&owner_client, source.id)
            .await
            .iter()
            .all(|n| !n.to_string().contains("has a new commit")),
        "the actor is not notified"
    );

    // Ordinary reviewers keep read access to later source commits.
    let handled_again = commit_file_on(
        &repo_path,
        &handled,
        "HANDLED-2.md",
        b"round 2 handled\n",
        "Handled round 2",
    );
    push_branch_as(
        &app,
        &repo_path,
        source.repo_id,
        &owner.jwt,
        &handled_again,
        &source.name,
    )
    .await;
    for (client, copy, who) in [
        (&riley_client, riley_copy, "riley"),
        (&sam_client, sam_copy, "sam"),
    ] {
        let notices: Vec<Value> = notifications_of(client, copy)
            .await
            .into_iter()
            .filter(|n| {
                n["title"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("has a new commit")
            })
            .collect();
        assert_eq!(
            notices.len(),
            2,
            "{who}: ordinary readers receive the new source activity: {notices:?}"
        );
        assert!(
            notices.iter().any(|notice| notice["message"]
                .as_str()
                .unwrap_or_default()
                .contains(&handled_again[..7])),
            "{who}: the new commit is disclosed: {notices:?}"
        );
    }

    // A clean tree commits nothing but is still pushed: a commit whose push
    // failed earlier must reach the branch when the owner presses "Commit
    // and push" again, and an up-to-date branch moves nothing.
    let clean_peer_before = peer.received_rpc_methods().await.len();
    drop(peer);
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("clear the first incarnation");
    let peer = TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &source,
        owner.id,
        json!([{"repo_id": source.repo_id, "committed": false, "head": handled, "files": 0}]),
    )
    .await;
    let (status, body) = post(
        &owner_client,
        &commit_url(source.id),
        json!({"message": "nothing to say"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["members"][0]["committed"], false, "{body}");
    assert_eq!(data(&body)["members"][0]["pushed"], true, "{body}");
    let driven = peer.received_rpc_methods().await;
    assert!(
        driven
            .iter()
            .any(|m| m == "modules.invoke:workspace.changes/push"),
        "a clean commit still pushes: {driven:?} (earlier peer saw {clean_peer_before})"
    );
}

/// Stand a running incarnation up for a review copy: the queued row Start
/// review left is replaced by the test peer, which answers every Changes
/// operation with one committed member at `head`.
async fn attach_copy_peer(
    app: &TestApp,
    copy_id: Uuid,
    repo_id: Uuid,
    reviewer_id: Uuid,
    head: &str,
) -> TestWorkspaceRuntimePeer {
    let name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
        .bind(copy_id)
        .fetch_one(&app.db)
        .await
        .expect("copy name");
    let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM workspaces WHERE id = $1")
        .bind(copy_id)
        .fetch_one(&app.db)
        .await
        .expect("copy org");
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
        .bind(copy_id)
        .execute(&app.db)
        .await
        .expect("clear the queued incarnation");
    let copy = TestWorkspace {
        id: copy_id,
        org_id,
        repo_id,
        head_commit: head.to_string(),
        name,
    };
    TestWorkspaceRuntimePeer::attach_with_changes_report(
        app,
        &copy,
        reviewer_id,
        json!([{"repo_id": repo_id, "committed": true, "head": head, "files": 1}]),
    )
    .await
}

async fn approval_rows_of(app: &TestApp, source_id: Uuid, user_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM approval_statuses \
         WHERE source_workspace_id = $1 AND actor_user_id = $2",
    )
    .bind(source_id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("count approval rows")
}

/// A round pushed with the verdict "approve" is the reviewer's approval on
/// every requirement of W's merge target that lists them; other verdicts and
/// unlisted reviewers leave the approvals alone, and a reviewer who already
/// approved at this commit is not recorded twice.
#[tokio::test]
async fn a_review_copy_verdict_is_feedback_even_for_a_configured_reviewer() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let owner_client = app.authenticated_client(&owner);
    // W merges into the target; the requirement lives on the target and
    // names Riley, not Sam.
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target.id)
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("set the merge target");
    let riley = create_member(&app, &org).await;
    let sam = create_member(&app, &org).await;
    let riley_client = app.authenticated_client(&riley);
    let sam_client = app.authenticated_client(&sam);
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/orgs/{}/approval-definitions", org.id),
        json!({"name": "Sign-off", "humanUserIds": [riley.id]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let definition_id = data(&body)["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
        json!({"definitionId": definition_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A file with room for two separate hunks in one round.
    let lines: String = (1..=40).map(|i| format!("line {i}\n")).collect();
    let source_commit = commit_file_on(
        &repo_path,
        &source.head_commit,
        "notes.txt",
        lines.as_bytes(),
        "add notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &source_commit).await;

    let (status, body) = start_review(&riley_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let riley_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("riley's copy");
    let (status, body) = start_review(&sam_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let sam_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("sam's copy");
    let (riley_branch_id, riley_head) = review_branch(&app, riley_copy, source.repo_id).await;
    assert_eq!(riley_head, source_commit);
    let edited = lines
        .replace("line 3\n", "line three\n")
        .replace("line 30\n", "line thirty\n");
    let r1 = commit_file_on(
        &repo_path,
        &riley_head,
        "notes.txt",
        edited.as_bytes(),
        "review: two hunks",
    );
    helpers::seed_workspace_authority_head(&app, riley_branch_id, source.repo_id, &r1).await;
    let _riley_peer = attach_copy_peer(&app, riley_copy, source.repo_id, riley.id, &r1).await;
    let publish_url = |copy: Uuid| format!("/api/v1/workspaces/{copy}/review/publish");

    // The owner commits after the fork. Riley's copy does not contain that
    // commit, so an approve verdict pushed without an Update reviews an
    // older source and must not stand as an approval of the newer one.
    let owner_commit = commit_file_on(
        &repo_path,
        &source_commit,
        "OWNER.md",
        b"after the fork\n",
        "owner: notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &owner_commit).await;
    let (status, body) = post(
        &riley_client,
        &publish_url(riley_copy),
        json!({"summary": "Ship it (stale)", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_ne!(
        data(&body)["publication"]["baseCommitVector"][0]["mergeBase"],
        data(&body)["publication"]["baseCommitVector"][0]["sourceHead"],
        "the copy is behind the source: {body}"
    );
    assert_eq!(
        approval_rows_of(&app, source.id, riley.id).await,
        0,
        "a copy behind the source approves nothing"
    );

    // Updating the copy changes the reviewed content, but does not turn its
    // broad verdict into a requested, type-specific formal approval.
    let r2 = commit_file_on(
        &repo_path,
        &owner_commit,
        "notes.txt",
        edited.as_bytes(),
        "review: two hunks, rebased",
    );
    helpers::seed_workspace_authority_head(&app, riley_branch_id, source.repo_id, &r2).await;
    // Riley publishes feedback and both suggestions.
    let (status, body) = post(
        &riley_client,
        &publish_url(riley_copy),
        json!({"summary": "Ship it", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        data(&body)["publication"]["baseCommitVector"][0]["mergeBase"],
        data(&body)["publication"]["baseCommitVector"][0]["sourceHead"],
        "the copy includes the source's tip: {body}"
    );
    assert_eq!(
        data(&body)["suggestions"].as_array().unwrap().len(),
        2,
        "{body}"
    );
    let owner_notice = notifications_of(&owner_client, source.id)
        .await
        .into_iter()
        .find(|n| {
            n["title"]
                .as_str()
                .unwrap_or_default()
                .contains("published review 2")
        })
        .expect("the owner is told about round 2");
    let message = owner_notice["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("2 suggestions in 1 file ")
            || message.ends_with("2 suggestions in 1 file"),
        "the file plural follows the file count: {message}"
    );
    assert!(!message.contains("1 files"), "{message}");
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let requirements = data(&body)["outgoing"]["requirements"]
        .as_array()
        .cloned()
        .expect("outgoing requirements");
    let requirement = requirements
        .iter()
        .find(|r| r["definitionId"] == definition_id || r["definition"]["id"] == definition_id)
        .unwrap_or_else(|| panic!("the requirement is listed: {body}"));
    assert_ne!(requirement["state"], "approved", "{requirement}");
    let riley_slot = requirement["approvers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["userId"] == riley.id.to_string())
        .unwrap_or_else(|| panic!("riley is an approver: {requirement}"));
    assert_ne!(riley_slot["state"], "approved", "{riley_slot}");
    assert_eq!(approval_rows_of(&app, source.id, riley.id).await, 0);

    // Sam is not listed: their approve verdict records nothing.
    let (sam_branch_id, sam_head) = review_branch(&app, sam_copy, source.repo_id).await;
    let s1 = commit_file_on(
        &repo_path,
        &sam_head,
        "SAM.md",
        b"looks fine\n",
        "review: sam's note",
    );
    helpers::seed_workspace_authority_head(&app, sam_branch_id, source.repo_id, &s1).await;
    let _sam_peer = attach_copy_peer(&app, sam_copy, source.repo_id, sam.id, &s1).await;
    let (status, body) = post(
        &sam_client,
        &publish_url(sam_copy),
        json!({"summary": "Also fine", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(approval_rows_of(&app, source.id, sam.id).await, 0);

    // Neither verdict changes formal decisions, including repeated rounds.
    let (status, body) = post(
        &riley_client,
        &publish_url(riley_copy),
        json!({"summary": "One more thing", "verdict": "request_changes"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(approval_rows_of(&app, source.id, riley.id).await, 0);
    let (status, body) = post(
        &riley_client,
        &publish_url(riley_copy),
        json!({"summary": "Fine after all", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(approval_rows_of(&app, source.id, riley.id).await, 0);

    app.cleanup().await;
}

/// A broad copy verdict neither approves other types/phases nor overwrites
/// existing evidence. Formal decisions require an explicit selected task.
#[tokio::test]
async fn a_review_copy_verdict_leaves_each_requirement_and_phase_independent() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let owner_client = app.authenticated_client(&owner);
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target.id)
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("source merging into the target");
    let riley = create_member(&app, &org).await;
    let riley_client = app.authenticated_client(&riley);
    // Both phases: IE already decided by Riley at this commit, so only BE
    // is still theirs to settle. BE-only: one phase.
    let mut definition_ids = Vec::new();
    for (name, phases) in [
        ("Both phases", json!(["be", "ie"])),
        ("BE phase", json!(["be"])),
    ] {
        let (status, body) = post(
            &owner_client,
            &format!("/api/v1/orgs/{}/approval-definitions", org.id),
            json!({
                "name": name,
                "humanUserIds": [riley.id],
                "phases": phases,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = data(&body)["id"].as_str().unwrap().to_string();
        let (status, body) = post(
            &owner_client,
            &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
            json!({"definitionId": id}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        definition_ids.push(id);
    }
    let both_id = definition_ids[0].clone();

    seed_open_review_pr(&app, &source, &target, owner.id).await;
    for definition_id in &definition_ids {
        let (status, body) = post(
            &owner_client,
            &format!("/api/v1/workspaces/{}/approvals/requests", source.id),
            json!({"definitionId": definition_id, "definitionVersion": 1, "phase": "be", "actorUserIds": [riley.id]}),
        ).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let source_commit = commit_file_on(
        &repo_path,
        &source.head_commit,
        "notes.txt",
        b"line 1\nline 2\nline 3\n",
        "add notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &source_commit).await;
    // Riley's IE decision on "Both phases", at the current commit, the way
    // the button would have recorded it.
    let heads = json!([{
        "repoId": source.repo_id.to_string(),
        "branchName": source.name,
        "workspaceId": source.id.to_string(),
        "headCommit": source_commit,
    }]);
    sqlx::query(
        "INSERT INTO approval_statuses \
            (id, org_id, source_workspace_id, approval_definition_id, \
             approval_definition_version, outcome, comment, actor_type, actor_user_id, \
             reviewed_git_heads, phase) \
         VALUES ($1, $2, $3, $4::uuid, 1, 'approved', 'Signed off', 'human', $5, $6, 'ie')",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(source.id)
    .bind(&both_id)
    .bind(riley.id)
    .bind(&heads)
    .execute(&app.db)
    .await
    .expect("riley's earlier IE decision");

    let (status, body) = start_review(&riley_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let riley_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("riley's copy");
    let (riley_branch_id, riley_head) = review_branch(&app, riley_copy, source.repo_id).await;
    let r1 = commit_file_on(
        &repo_path,
        &riley_head,
        "notes.txt",
        b"line one\nline 2\nline 3\n",
        "review: a word",
    );
    helpers::seed_workspace_authority_head(&app, riley_branch_id, source.repo_id, &r1).await;
    let _riley_peer = attach_copy_peer(&app, riley_copy, source.repo_id, riley.id, &r1).await;
    let (status, body) = post(
        &riley_client,
        &format!("/api/v1/workspaces/{riley_copy}/review/publish"),
        json!({"summary": "Plans look right", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The existing evidence is unchanged, with no decision for either BE type.
    let mut rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT approval_definition_id::text, phase, COALESCE(comment, '') \
         FROM approval_statuses WHERE source_workspace_id = $1 AND actor_user_id = $2",
    )
    .bind(source.id)
    .bind(riley.id)
    .fetch_all(&app.db)
    .await
    .expect("riley's decision rows");
    rows.sort();
    let mut expected = vec![(both_id.clone(), "ie".to_string(), "Signed off".to_string())];
    expected.sort();
    assert_eq!(
        rows, expected,
        "all formal decisions stay independent of the round verdict"
    );
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_requests WHERE source_workspace_id = $1 AND state = 'pending'")
        .bind(source.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(
        pending, 2,
        "both specifically requested BE types remain pending"
    );

    app.cleanup().await;
}

/// A broad round cannot silently retarget a pending request to a new version.
#[tokio::test]
async fn a_review_copy_verdict_does_not_retarget_an_invalidated_request() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let owner_client = app.authenticated_client(&owner);
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target.id)
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("set the merge target");
    let riley = create_member(&app, &org).await;
    let riley_client = app.authenticated_client(&riley);
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/orgs/{}/approval-definitions", org.id),
        json!({"name": "Sign-off", "humanUserIds": [riley.id]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let definition_id = data(&body)["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
        json!({"definitionId": definition_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // A request to Riley at v1, then v2 invalidates everything before it.
    seed_open_review_pr(&app, &source, &target, owner.id).await;
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals/requests", source.id),
        json!({"definitionId": definition_id, "definitionVersion": 1, "phase": "ie"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let v1_request = data(&body)["requests"][0]["requestId"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/orgs/{}/approval-definitions/{definition_id}/versions",
            org.id
        ),
        json!({"name": "Sign-off", "humanUserIds": [riley.id], "invalidatesPrevious": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let source_commit = commit_file_on(
        &repo_path,
        &source.head_commit,
        "notes.txt",
        b"line 1\nline 2\n",
        "add notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &source_commit).await;
    let (status, body) = start_review(&riley_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let riley_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("riley's copy");
    let (riley_branch_id, riley_head) = review_branch(&app, riley_copy, source.repo_id).await;
    let r1 = commit_file_on(
        &repo_path,
        &riley_head,
        "notes.txt",
        b"line one\nline 2\n",
        "review: a word",
    );
    helpers::seed_workspace_authority_head(&app, riley_branch_id, source.repo_id, &r1).await;
    let _riley_peer = attach_copy_peer(&app, riley_copy, source.repo_id, riley.id, &r1).await;
    let (status, body) = post(
        &riley_client,
        &format!("/api/v1/workspaces/{riley_copy}/review/publish"),
        json!({"summary": "Fine", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT approval_definition_version, outcome FROM approval_statuses \
         WHERE source_workspace_id = $1 AND actor_user_id = $2",
    )
    .bind(source.id)
    .bind(riley.id)
    .fetch_all(&app.db)
    .await
    .expect("riley's decision rows");
    assert!(
        rows.is_empty(),
        "the round cannot create an unrequested v2 decision: {rows:?}"
    );
    let v1_state: String =
        sqlx::query_scalar("SELECT state FROM approval_requests WHERE id = $1::uuid")
            .bind(&v1_request)
            .fetch_one(&app.db)
            .await
            .expect("v1 request");
    assert_eq!(
        v1_state, "pending",
        "the retired request is not what was settled"
    );
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let requirement = data(&body)["outgoing"]["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|r| r["definition"]["id"] == definition_id)
        .unwrap_or_else(|| panic!("the requirement is listed: {body}"));
    assert_ne!(requirement["state"], "approved", "{requirement}");

    app.cleanup().await;
}

/// A broad round remains feedback when a definition changes its recommendation.
#[tokio::test]
async fn a_review_copy_verdict_does_not_retarget_a_request_to_another_phase() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let owner_client = app.authenticated_client(&owner);
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id = $1 WHERE id = $2")
        .bind(target.id)
        .bind(source.id)
        .execute(&app.db)
        .await
        .expect("source merging into the target");
    let riley = create_member(&app, &org).await;
    let riley_client = app.authenticated_client(&riley);
    // v1 recommends IE; Riley's addressed request is pinned to that version.
    seed_open_review_pr(&app, &source, &target, owner.id).await;
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/orgs/{}/approval-definitions", org.id),
        json!({
            "name": "Sign-off",
            "humanUserIds": [riley.id],
            "phases": ["ie"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let definition_id = data(&body)["id"].as_str().unwrap().to_string();
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals/requirements", target.id),
        json!({"definitionId": definition_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals/requests", source.id),
        json!({"definitionId": definition_id, "definitionVersion": 1, "phase": "ie"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let v1_request = data(&body)["requests"][0]["requestId"]
        .as_str()
        .unwrap()
        .to_string();
    // v2 changes the recommendation to BE without invalidating v1.
    // A review-copy verdict still cannot settle Riley's addressed request.
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/orgs/{}/approval-definitions/{definition_id}/versions",
            org.id
        ),
        json!({
            "name": "Sign-off",
            "humanUserIds": [riley.id],
            "invalidatesPrevious": false,
            "phases": ["be"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let source_commit = commit_file_on(
        &repo_path,
        &source.head_commit,
        "notes.txt",
        b"line 1\nline 2\n",
        "add notes",
    );
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &source_commit).await;
    let (status, body) = start_review(&riley_client, source.id).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let riley_copy: Uuid = data(&body)["review"]["reviewWorkspaceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("riley's copy");
    let (riley_branch_id, riley_head) = review_branch(&app, riley_copy, source.repo_id).await;
    let r1 = commit_file_on(
        &repo_path,
        &riley_head,
        "notes.txt",
        b"line one\nline 2\n",
        "review: a word",
    );
    helpers::seed_workspace_authority_head(&app, riley_branch_id, source.repo_id, &r1).await;
    let _riley_peer = attach_copy_peer(&app, riley_copy, source.repo_id, riley.id, &r1).await;
    let (status, body) = post(
        &riley_client,
        &format!("/api/v1/workspaces/{riley_copy}/review/publish"),
        json!({"summary": "Plans are fine", "verdict": "approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let rows: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT approval_definition_version, phase, outcome FROM approval_statuses \
         WHERE source_workspace_id = $1 AND actor_user_id = $2",
    )
    .bind(source.id)
    .bind(riley.id)
    .fetch_all(&app.db)
    .await
    .expect("riley's decision rows");
    assert!(
        rows.is_empty(),
        "a review-copy verdict cannot create a formal decision: {rows:?}"
    );
    let v1_state: (String, String) =
        sqlx::query_as("SELECT state, phase FROM approval_requests WHERE id = $1::uuid")
            .bind(&v1_request)
            .fetch_one(&app.db)
            .await
            .expect("v1 request");
    assert_eq!(
        v1_state,
        ("pending".to_string(), "ie".to_string()),
        "the addressed request and its immutable compatibility phase are left alone"
    );
    let (status, body) = get(
        &owner_client,
        &format!("/api/v1/workspaces/{}/approvals", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let requirements = data(&body)["outgoing"]["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(requirements.len(), 1, "one workspace-wide type: {body}");
    let requirement = &requirements[0];
    assert_eq!(requirement["definition"]["id"], definition_id);
    assert_eq!(requirement["definition"]["latestVersion"], 2);
    assert_eq!(requirement["definition"]["phases"], json!(["be"]));
    assert_eq!(requirement["requests"][0]["requestId"], v1_request);
    assert!(requirement["decisionLog"].as_array().unwrap().is_empty());
    assert_ne!(requirement["state"], "approved", "{requirement}");
    assert_eq!(data(&body)["outgoing"]["mergeAdmission"]["admitted"], false);

    app.cleanup().await;
}

// ─── Suggestion threads (`.g5/ontology-plan-comments.md`) ────────────────────

async fn review_notice_count(app: &TestApp, user_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE recipient_user_id = $1 AND category = 'review'",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("review notices")
}

async fn thread_replies(
    client: &helpers::AuthenticatedClient,
    source_id: Uuid,
    suggestion_id: Uuid,
) -> Vec<Value> {
    let (status, body) = get(
        client,
        &format!("/api/v1/workspaces/{source_id}/review/suggestions/{suggestion_id}/replies"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    data(&body)["replies"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A comments-only publication by `reviewer` on `source` with one open
/// comment suggestion and its thread, written as rows so the decision and
/// reply routes can be driven between two different people.
async fn seed_comment_suggestion(
    app: &TestApp,
    source: &TestWorkspace,
    reviewer_id: Uuid,
    round: i32,
) -> (Uuid, Uuid) {
    let publication_id: Uuid = sqlx::query_scalar(
        "INSERT INTO review_publications
             (org_id, source_workspace_id, reviewer_user_id, author_kind, author_label, round)
         VALUES ($1, $2, $3, 'human', 'Reviewer', $4)
         RETURNING id",
    )
    .bind(source.org_id)
    .bind(source.id)
    .bind(reviewer_id)
    .bind(round)
    .fetch_one(&app.db)
    .await
    .expect("publication");
    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO comment_index
             (org_id, workspace_id, author_id, scope, repo_id, file_path, line_number, content, origin_branch)
         VALUES ($1, $2, $3, 'suggestion', $4, 'README.md', 1, 'Is this heading right?', 'main')
         RETURNING id",
    )
    .bind(source.org_id)
    .bind(source.id)
    .bind(reviewer_id)
    .bind(source.repo_id)
    .fetch_one(&app.db)
    .await
    .expect("thread");
    let suggestion_id: Uuid = sqlx::query_scalar(
        "INSERT INTO review_suggestions
             (org_id, publication_id, repo_id, path, kind, before_lines, anchor_start,
              author_kind, author_label, note, comment_id)
         VALUES ($1, $2, $3, 'README.md', 'comment', 'test workspace', 1,
                 'human', 'Reviewer', 'Is this heading right?', $4)
         RETURNING id",
    )
    .bind(source.org_id)
    .bind(publication_id)
    .bind(source.repo_id)
    .bind(comment_id)
    .fetch_one(&app.db)
    .await
    .expect("suggestion");
    (suggestion_id, comment_id)
}

/// g5.governance.review-thread.notices and .access: each reply and decision
/// tells the other party once and never the actor; any reader replies; the
/// anchored-comment routes cannot reach the thread.
#[tokio::test]
async fn suggestion_replies_and_decisions_tell_the_other_party_once() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let reviewer = create_member(&app, &org).await;
    let reader = create_member(&app, &org).await;
    let owner_client = app.authenticated_client(&owner);
    let reviewer_client = app.authenticated_client(&reviewer);
    let reader_client = app.authenticated_client(&reader);

    let (resolved_id, resolved_thread) =
        seed_comment_suggestion(&app, &source, reviewer.id, 1).await;
    let (withdrawn_id, _) = seed_comment_suggestion(&app, &source, reviewer.id, 2).await;

    // The owner resolves: the reviewer hears once, the owner not at all.
    let (owner_before, reviewer_before) = (
        review_notice_count(&app, owner.id).await,
        review_notice_count(&app, reviewer.id).await,
    );
    let (status, body) = post(
        &owner_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{resolved_id}/resolve",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        review_notice_count(&app, reviewer.id).await,
        reviewer_before + 1
    );
    assert_eq!(review_notice_count(&app, owner.id).await, owner_before);

    // The reviewer withdraws: the owner hears once.
    let (status, body) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{withdrawn_id}/withdraw",
            source.id
        ),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(review_notice_count(&app, owner.id).await, owner_before + 1);
    assert_eq!(
        review_notice_count(&app, reviewer.id).await,
        reviewer_before + 1
    );

    // A reader who is neither owner nor reviewer cannot decide.
    let (open_id, _) = seed_comment_suggestion(&app, &source, reviewer.id, 3).await;
    for action in ["reject", "resolve", "withdraw"] {
        let (status, body) = post(
            &reader_client,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{open_id}/{action}",
                source.id
            ),
            json!({}),
        )
        .await;
        assert!(
            status == StatusCode::FORBIDDEN,
            "a reader's {action} is refused: {status} {body}"
        );
    }

    // A third reader replies on a decided suggestion: both parties hear once.
    let (status, body) = post(
        &reader_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{resolved_id}/replies",
            source.id
        ),
        json!({"content": "I agree with resolving this"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(review_notice_count(&app, owner.id).await, owner_before + 2);
    assert_eq!(
        review_notice_count(&app, reviewer.id).await,
        reviewer_before + 2
    );
    assert_eq!(review_notice_count(&app, reader.id).await, 0);

    // The reviewer replies: only the owner hears.
    let (status, _) = post(
        &reviewer_client,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions/{resolved_id}/replies",
            source.id
        ),
        json!({"content": "Thanks"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(review_notice_count(&app, owner.id).await, owner_before + 3);
    assert_eq!(
        review_notice_count(&app, reviewer.id).await,
        reviewer_before + 2
    );

    let replies = thread_replies(&owner_client, source.id, resolved_id).await;
    let authors: Vec<&str> = replies
        .iter()
        .map(|reply| {
            assert!(reply.get("authorKind").is_none());
            reply["authorId"].as_str().unwrap()
        })
        .collect();
    let owner_id = owner.id.to_string();
    let reader_id = reader.id.to_string();
    let reviewer_id = reviewer.id.to_string();
    assert_eq!(
        authors,
        vec![owner_id.as_str(), reader_id.as_str(), reviewer_id.as_str(),],
        "the resolve note, then the two replies: {replies:?}"
    );

    // The thread is reached only through its suggestion.
    for status in [
        owner_client
            .get(&format!("/api/v1/comments/{resolved_thread}/replies"))
            .await
            .status(),
        owner_client
            .post_json(
                &format!("/api/v1/comments/{resolved_thread}/replies"),
                &json!({"content": "around the review"}),
            )
            .await
            .status(),
        owner_client
            .patch_json(
                &format!("/api/v1/comments/{resolved_thread}"),
                &json!({"resolved": true}),
            )
            .await
            .status(),
        owner_client
            .delete(&format!("/api/v1/comments/{resolved_thread}"))
            .await
            .status(),
    ] {
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(
        thread_replies(&owner_client, source.id, resolved_id)
            .await
            .len(),
        3
    );
}

/// g5.documents.comments.replies and g5.workspace.identity.base: Base is
/// read-only for every user, so its review threads accept no message, and
/// decisions that would write a thread note are refused before any effect.
/// A suggestion whose thread row is gone (`comment_id` is set null on
/// delete) writes no note, so only the decision's own check refuses it;
/// keeping an artifact is refused the same way, threaded or not.
#[tokio::test]
async fn base_review_threads_accept_no_message() {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let reviewer = create_member(&app, &org).await;
    let owner_client = app.authenticated_client(&owner);
    let reviewer_client = app.authenticated_client(&reviewer);
    let (threaded_id, _) = seed_comment_suggestion(&app, &base, reviewer.id, 1).await;
    let (threadless_id, _) = seed_comment_suggestion(&app, &base, reviewer.id, 2).await;
    let (artifact_id, _) = seed_comment_suggestion(&app, &base, reviewer.id, 3).await;
    let (threadless_artifact_id, _) = seed_comment_suggestion(&app, &base, reviewer.id, 4).await;
    for id in [artifact_id, threadless_artifact_id] {
        sqlx::query(
            "UPDATE review_suggestions
             SET kind = 'artifact', blob_oid = repeat('a', 40), blob_sha256 = repeat('b', 64),
                 bytes = 3, content_type = 'text/plain'
             WHERE id = $1",
        )
        .bind(id)
        .execute(&app.db)
        .await
        .expect("make an artifact");
    }
    for id in [threadless_id, threadless_artifact_id] {
        sqlx::query("UPDATE review_suggestions SET comment_id = NULL WHERE id = $1")
            .bind(id)
            .execute(&app.db)
            .await
            .expect("detach the thread");
    }

    for suggestion_id in [artifact_id, threadless_artifact_id] {
        let (status, body) = post(
            &owner_client,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/accept",
                base.id
            ),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "keep on base: {body}");
        let status: String =
            sqlx::query_scalar("SELECT status FROM review_suggestions WHERE id = $1")
                .bind(suggestion_id)
                .fetch_one(&app.db)
                .await
                .expect("artifact status");
        assert_eq!(status, "open", "no artifact was kept on base");
    }

    for suggestion_id in [threaded_id, threadless_id] {
        let (status, body) = post(
            &reviewer_client,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/replies",
                base.id
            ),
            json!({"content": "on base"}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "reply on base: {body}");
        for (client, action) in [
            (&owner_client, "reject"),
            (&owner_client, "resolve"),
            (&reviewer_client, "withdraw"),
        ] {
            let (status, body) = post(
                client,
                &format!(
                    "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/{action}",
                    base.id
                ),
                json!({}),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{action} on base: {body}");
        }
        let status: String =
            sqlx::query_scalar("SELECT status FROM review_suggestions WHERE id = $1")
                .bind(suggestion_id)
                .fetch_one(&app.db)
                .await
                .expect("suggestion status");
        assert_eq!(status, "open", "nothing was decided on base");
    }
    let notices: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE recipient_user_id = ANY($1)")
            .bind(vec![owner.id, reviewer.id])
            .fetch_one(&app.db)
            .await
            .expect("notices");
    assert_eq!(notices, 0, "a refused decision notifies nobody");
}

// ─── Push review from FILES (g5.governance.file-suggestions) ────────────────

/// Largest text a FILES push carries.
const FILE_TEXT_LIMIT: usize =
    g5_api::services::workspace_review_service::MAX_FILE_SUGGESTION_TEXT_BYTES;

fn file_suggestions_url(source_id: Uuid) -> String {
    format!("/api/v1/workspaces/{source_id}/review/file-suggestions")
}

fn suggestion_url(source_id: Uuid, suggestion: &Value, action: &str) -> String {
    format!(
        "/api/v1/workspaces/{source_id}/review/suggestions/{}/{action}",
        suggestion["id"].as_str().expect("suggestion id")
    )
}

/// A source its owner holds with no running VM, `files` committed on its
/// branch, and an org member who reads it but does not own it.
struct FilesFixture {
    app: TestApp,
    org: TestOrg,
    owner: TestUser,
    reader: TestUser,
    source: TestWorkspace,
    repo_path: std::path::PathBuf,
    head: String,
}

async fn files_fixture(files: &[(&str, &[u8])]) -> FilesFixture {
    init_tracing();
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let source = TestWorkspace::create(&app, &org).await;
    let reader = create_member(&app, &org).await;
    let repo_path = app.repo_path(source.repo_id).await;
    let mut head = source.head_commit.clone();
    for (path, content) in files {
        head = commit_file_on(
            &repo_path,
            &head,
            path,
            content,
            &format!("owner: add {path}"),
        );
    }
    helpers::seed_workspace_authority_head(&app, source.id, source.repo_id, &head).await;
    FilesFixture {
        app,
        org,
        owner,
        reader,
        source,
        repo_path,
        head,
    }
}

impl FilesFixture {
    /// The owner commits `content` to `path`; the branch moves to it.
    async fn owner_commits(&mut self, path: &str, content: &[u8]) {
        self.head = commit_file_on(
            &self.repo_path,
            &self.head,
            path,
            content,
            &format!("owner: change {path}"),
        );
        helpers::seed_workspace_authority_head(
            &self.app,
            self.source.id,
            self.source.repo_id,
            &self.head,
        )
        .await;
    }

    async fn vm_rows(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM workspace_vms WHERE workspace_id = $1")
            .bind(self.source.id)
            .fetch_one(&self.app.db)
            .await
            .expect("count incarnations")
    }
}

/// The file as `client` reads it in FILES.
async fn read_file(
    client: &helpers::AuthenticatedClient,
    source: &TestWorkspace,
    path: &str,
) -> Value {
    let (status, body) = get(
        client,
        &format!(
            "/api/v1/workspaces/{}/repos/{}/files/{path}",
            source.id, source.repo_id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    data(&body).clone()
}

/// Push review from FILES; `options` adds `note` or `allowStaleBase`.
async fn push_file_suggestion(
    client: &helpers::AuthenticatedClient,
    source: &TestWorkspace,
    path: &str,
    base: &str,
    content: &str,
    options: Value,
) -> (StatusCode, Value) {
    let mut body = json!({
        "repoId": source.repo_id,
        "path": path,
        "baseContent": base,
        "content": content,
    });
    if let (Some(body), Some(options)) = (body.as_object_mut(), options.as_object()) {
        body.extend(options.clone());
    }
    post(client, &file_suggestions_url(source.id), body).await
}

/// `(field, message)` of a FILES push refusal, as the page shows it.
fn refusal(body: &Value) -> (&str, &str) {
    (
        body["error"]["details"]["field"]
            .as_str()
            .unwrap_or_default(),
        body["error"]["details"]["message"]
            .as_str()
            .unwrap_or_default(),
    )
}

/// The author and opening message of a suggestion's thread.
async fn thread_opening(app: &TestApp, suggestion: &Value) -> (Uuid, String) {
    let comment_id =
        Uuid::parse_str(suggestion["commentId"].as_str().expect("suggestion thread")).unwrap();
    sqlx::query_as(
        "SELECT author_id, content FROM comment_index WHERE id = $1 AND scope = 'suggestion'",
    )
    .bind(comment_id)
    .fetch_one(&app.db)
    .await
    .expect("suggestion thread")
}

/// g5.governance.file-suggestions and .record: a reader's FILES edit becomes
/// hunk suggestions on the source while the owner's VM is stopped, in its
/// own annotation publication without a review workspace. The note opens
/// each thread, the owner is told once, and nothing is written to the
/// source.
#[tokio::test]
async fn a_reader_pushes_a_files_edit_as_suggestions_while_the_owner_vm_is_stopped() {
    const GUIDE: &str = "docs/guide.md";
    let base = "# Guide\n\nInstall the tool.\nRun the tool.\n\n## Usage\n\nCall it once.\n\
                Call it twice.\n\n## Notes\n\nNothing yet.\n";
    let edited = base
        .replace("Run the tool.", "Run the tool daily.")
        .replace("Nothing yet.", "Read the FAQ first.");
    let fixture = files_fixture(&[(GUIDE, base.as_bytes())]).await;
    let app = &fixture.app;
    let source = &fixture.source;
    let reader = app.authenticated_client(&fixture.reader);
    let owner = app.authenticated_client(&fixture.owner);
    assert_eq!(fixture.vm_rows().await, 0, "the owner's VM is stopped");

    // The reader starts from the file as FILES shows it to them.
    let started = read_file(&reader, source, GUIDE).await;
    assert_eq!(started["source"], "committed", "{started}");
    let (status, body) = push_file_suggestion(
        &reader,
        source,
        GUIDE,
        started["content"].as_str().unwrap(),
        &edited,
        json!({"note": "  Two small fixes for new readers.  "}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let publication = &data(&body)["publication"];
    assert_eq!(publication["kind"], "annotation", "{body}");
    assert!(publication["reviewWorkspaceId"].is_null(), "{body}");
    assert!(publication["supersededBy"].is_null(), "{body}");
    assert_eq!(publication["sourceWorkspaceId"], source.id.to_string());
    assert_eq!(publication["reviewerUserId"], fixture.reader.id.to_string());
    assert_eq!(publication["authorKind"], "human");
    assert_eq!(publication["verdict"], "comment_only");
    assert_eq!(
        publication["baseCommitVector"],
        json!([{"repoId": source.repo_id, "sourceHead": fixture.head}]),
        "the vector names the source head at the push"
    );
    let suggestions = data(&body)["suggestions"].as_array().cloned().unwrap();
    assert_eq!(suggestions.len(), 2, "one per Git hunk: {body}");
    for suggestion in &suggestions {
        assert_eq!(suggestion["kind"], "hunk", "{suggestion}");
        assert_eq!(suggestion["path"], GUIDE);
        assert_eq!(suggestion["repoId"], source.repo_id.to_string());
        assert_eq!(suggestion["publicationId"], publication["id"]);
        assert_eq!(suggestion["status"], "open");
        assert_eq!(suggestion["note"], "Two small fixes for new readers.");
        let edits = suggestion["edits"].as_array().unwrap();
        assert!(!edits.is_empty(), "per-edit identities: {suggestion}");
        for edit in edits {
            assert_eq!(edit["status"], "open", "{suggestion}");
            assert!(
                edit["commentId"].is_string(),
                "each edit has a thread: {suggestion}"
            );
        }
        assert_eq!(
            thread_opening(app, suggestion).await,
            (
                fixture.reader.id,
                "Two small fixes for new readers.".to_string()
            ),
            "the note opens the thread, written by the reader"
        );
    }
    assert_eq!(suggestions[0]["anchorStart"], 1);
    assert!(suggestions[0]["afterLines"]
        .as_str()
        .unwrap()
        .contains("Run the tool daily.\n"));
    assert!(suggestions[1]["beforeLines"]
        .as_str()
        .unwrap()
        .contains("Nothing yet.\n"));
    assert!(suggestions[1]["afterLines"]
        .as_str()
        .unwrap()
        .contains("Read the FAQ first.\n"));

    // The owner is told once, the reader not at all.
    let edits: usize = suggestions
        .iter()
        .map(|s| s["edits"].as_array().unwrap().len().max(1))
        .sum();
    let notices: Vec<Value> = notifications_of(&owner, source.id)
        .await
        .into_iter()
        .filter(|notice| notice["category"] == "review")
        .collect();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(
        notices[0]["title"],
        format!(
            "{} suggested changes to {GUIDE}",
            fixture.reader.display_name
        )
    );
    assert_eq!(
        notices[0]["message"],
        format!("{edits} suggestions in {}", source.name)
    );
    assert_eq!(
        notices[0]["actions"][0]["target"],
        format!("workspace:{}?view=reviews", source.id)
    );
    assert_eq!(review_notice_count(app, fixture.reader.id).await, 0);

    // The owner's FILES page lists them while the VM stays stopped.
    let (status, listed) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?repoId={}&path={GUIDE}&currentOnly=true",
            source.id, source.repo_id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        data(&listed)["suggestions"].as_array().map(Vec::len),
        Some(2),
        "{listed}"
    );
    let (status, counts) = get(
        &owner,
        &format!("/api/v1/workspaces/{}/review/suggestions/counts", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{counts}");
    assert_eq!(
        data(&counts)["paths"],
        json!([{"repoId": source.repo_id, "path": GUIDE, "openCount": edits}])
    );

    // Records only: the branch, the file and the VM are as they were.
    let (_, head_now) = review_branch(app, source.id, source.repo_id).await;
    assert_eq!(head_now, fixture.head);
    assert_eq!(read_file(&reader, source, GUIDE).await["content"], base);
    assert_eq!(fixture.vm_rows().await, 0, "no VM was started");
    fixture.app.cleanup().await;
}

/// g5.governance.file-suggestions and g5.documents.files-view.text-only:
/// each refusal says why, and a refused push records nothing.
#[tokio::test]
async fn file_suggestions_refuse_owners_closed_sources_non_readers_and_non_text() {
    const DOC: &str = "docs/a.md";
    let text = "first line\nsecond line\n";
    let edited = "first line\nsecond line, edited\n";
    let big = format!("{}\n", "x".repeat(1023)).repeat(2049);
    let fixture = files_fixture(&[
        (DOC, text.as_bytes()),
        ("bin/data.bin", &b"\x00\x01\x02 text\n"[..]),
        ("docs/latin1.txt", &b"caf\xe9\n"[..]),
        ("docs/big.md", big.as_bytes()),
    ])
    .await;
    let app = &fixture.app;
    let source = &fixture.source;
    let reader = app.authenticated_client(&fixture.reader);
    let owner = app.authenticated_client(&fixture.owner);

    // The owner edits files directly.
    let (status, body) = push_file_suggestion(&owner, source, DOC, text, edited, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "CONFLICT", "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("The workspace owner edits files directly"),
        "{body}"
    );

    // Not text (the current file or either text), nothing or too much to
    // propose, and bad paths or notes.
    let many_lines = format!("{text}{}", "added\n".repeat(3_001));
    let oversized = "y".repeat(FILE_TEXT_LIMIT + 1);
    let long_note = "n".repeat(10_001);
    for (path, base, content, options, field, message) in [
        (
            "bin/data.bin",
            "\u{0}\u{1}\u{2} text\n",
            "\u{0}\u{1}\u{2} text!\n",
            json!({}),
            "path",
            "bin/data.bin contains a NUL byte, so it is not text".to_string(),
        ),
        (
            "docs/latin1.txt",
            "café\n",
            "cafés\n",
            json!({}),
            "path",
            "docs/latin1.txt is not valid UTF-8 text".to_string(),
        ),
        (
            "docs/big.md",
            "x\n",
            "y\n",
            json!({}),
            "path",
            format!(
                "docs/big.md is {} bytes; at most {FILE_TEXT_LIMIT} bytes can be read here",
                big.len()
            ),
        ),
        (
            DOC,
            text,
            "first line\n\u{0}\n",
            json!({}),
            "content",
            "Your edited text contains a NUL byte, so it is not text".to_string(),
        ),
        (
            DOC,
            "first line\u{0}\n",
            edited,
            json!({}),
            "baseContent",
            "The text you started from contains a NUL byte, so it is not text".to_string(),
        ),
        (
            DOC,
            text,
            oversized.as_str(),
            json!({}),
            "content",
            "Your edited text is larger than 2 MiB".to_string(),
        ),
        (
            DOC,
            text,
            text,
            json!({}),
            "content",
            "No changes to suggest".to_string(),
        ),
        (
            DOC,
            text,
            "first line\nsecond line",
            json!({}),
            "content",
            "No changes to suggest".to_string(),
        ),
        (
            DOC,
            text,
            many_lines.as_str(),
            json!({}),
            "content",
            "This change is too large to suggest".to_string(),
        ),
        (
            "",
            text,
            edited,
            json!({}),
            "path",
            "File path must not be empty".to_string(),
        ),
        (
            "../a.md",
            text,
            edited,
            json!({}),
            "path",
            "Path traversal is not allowed".to_string(),
        ),
        (
            DOC,
            text,
            edited,
            json!({"note": long_note}),
            "note",
            "The note is longer than 10000 characters".to_string(),
        ),
    ] {
        let (status, body) =
            push_file_suggestion(&reader, source, path, base, content, options).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}: {message}: {body}"
        );
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR", "{body}");
        assert_eq!(refusal(&body), (field, message.as_str()), "{path}");
        // The page shows the response's message: it is the reason itself.
        assert_eq!(body["error"]["message"], message.as_str(), "{body}");
    }

    // A file that does not exist, and a repository outside the source.
    let (status, body) =
        push_file_suggestion(&reader, source, "docs/missing.md", text, edited, json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = post(
        &reader,
        &file_suggestions_url(source.id),
        json!({"repoId": Uuid::new_v4(), "path": DOC, "baseContent": text, "content": edited}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["details"]["field"], "repoId", "{body}");

    // Bytes that are not UTF-8 cannot travel as JSON text at all.
    let mut raw = format!(
        r#"{{"repoId":"{}","path":"{DOC}","baseContent":"first line\nsecond line\n","content":"caf"#,
        source.repo_id
    )
    .into_bytes();
    raw.push(0xe9);
    raw.extend_from_slice(br#"\n"}"#);
    let response = reader
        .post_body(&file_suggestions_url(source.id), "application/json", raw)
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Closed sources take no new review rows: read-only, freezing and Base.
    for (policy, transition) in [("read_only", None), ("read_write", Some("freezing"))] {
        sqlx::query(
            "UPDATE workspaces SET mutation_policy = $2, mutation_policy_transition = $3 \
             WHERE id = $1",
        )
        .bind(source.id)
        .bind(policy)
        .bind(transition)
        .execute(&app.db)
        .await
        .expect("close the source");
        let (status, body) =
            push_file_suggestion(&reader, source, DOC, text, edited, json!({})).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{policy} {transition:?}: {body}"
        );
        assert_eq!(body["error"]["code"], "WORKSPACE_READ_ONLY", "{body}");
    }
    sqlx::query(
        "UPDATE workspaces SET mutation_policy = 'read_write', mutation_policy_transition = NULL \
         WHERE id = $1",
    )
    .bind(source.id)
    .execute(&app.db)
    .await
    .expect("reopen the source");
    let base_workspace = TestWorkspace::create_base(app, &fixture.org).await;
    let (status, body) = push_file_suggestion(
        &reader,
        &base_workspace,
        "README.md",
        "test workspace\n",
        "test workspace, edited\n",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "WORKSPACE_READ_ONLY", "{body}");

    // Only someone who can read the source suggests to it.
    let private = TestWorkspace::create_personal(app, &fixture.org, fixture.owner.id).await;
    let (status, body) = push_file_suggestion(
        &reader,
        &private,
        "README.md",
        "test workspace\n",
        "test workspace, edited\n",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN", "{body}");

    // No refusal left a row, a thread or a notice behind.
    let publications: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_publications")
        .fetch_one(&app.db)
        .await
        .expect("count publications");
    assert_eq!(publications, 0);
    let threads: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM comment_index WHERE scope = 'suggestion'")
            .fetch_one(&app.db)
            .await
            .expect("count threads");
    assert_eq!(threads, 0);
    assert_eq!(review_notice_count(app, fixture.owner.id).await, 0);
    fixture.app.cleanup().await;
}

/// g5.governance.file-suggestions.stale and .base: a push whose starting
/// text is no longer the reader's file conflicts and names the current
/// version. Pushed anyway, it is recorded against the reader's text, and
/// the owner's Accept applies it only where that text still places: an
/// edit whose lines remain applies there, while a pure insertion into a
/// file that is no longer empty and a block that now appears twice keep
/// the existing refusals.
#[tokio::test]
async fn a_stale_file_suggestion_conflicts_until_pushed_anyway_and_places_only_where_its_text_matches(
) {
    use sha2::Digest as _;
    const PLAN: &str = "docs/plan.md";
    const EMPTY: &str = "docs/empty.md";
    const TWINS: &str = "docs/twins.md";
    let plan = "# Plan\n\nalpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\n";
    let twins = "alpha\ntarget line\nomega\n";
    let mut fixture = files_fixture(&[
        (PLAN, plan.as_bytes()),
        (EMPTY, &b""[..]),
        (TWINS, twins.as_bytes()),
    ])
    .await;
    let reader = fixture.app.authenticated_client(&fixture.reader);
    let owner = fixture.app.authenticated_client(&fixture.owner);
    let started_plan = read_file(&reader, &fixture.source, PLAN).await;
    assert_eq!(
        read_file(&reader, &fixture.source, EMPTY).await["content"],
        ""
    );

    // The owner moves on: an intro above the plan, text in the empty file
    // and a second copy of the twins' block.
    let owner_plan = format!("Owner's intro.\n{plan}");
    let owner_twins = format!("{twins}{twins}");
    fixture.owner_commits(PLAN, owner_plan.as_bytes()).await;
    fixture.owner_commits(EMPTY, b"owner text\n").await;
    fixture.owner_commits(TWINS, owner_twins.as_bytes()).await;
    assert_ne!(
        read_file(&reader, &fixture.source, PLAN).await["hash"],
        started_plan["hash"]
    );

    let reader_plan = plan.replace("theta", "THETA");
    let mut pushed = Vec::new();
    for (path, started, proposed) in [
        (PLAN, plan, reader_plan.as_str()),
        (EMPTY, "", "reader line\n"),
        (TWINS, twins, "alpha\nchanged line\nomega\n"),
    ] {
        // From the old start: refused, naming the version the reader reads.
        let (status, body) =
            push_file_suggestion(&reader, &fixture.source, path, started, proposed, json!({}))
                .await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}: {body}");
        assert_eq!(body["error"]["code"], "FILE_VERSION_CONFLICT", "{body}");
        let current = read_file(&reader, &fixture.source, path).await;
        assert_eq!(
            body["error"]["details"],
            json!({"path": path, "hash": current["hash"]}),
            "{body}"
        );
        // Push anyway: recorded against the text the reader started from.
        let (status, body) = push_file_suggestion(
            &reader,
            &fixture.source,
            path,
            started,
            proposed,
            json!({"allowStaleBase": true}),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{path}: {body}");
        let suggestions = data(&body)["suggestions"].as_array().cloned().unwrap();
        assert_eq!(suggestions.len(), 1, "{body}");
        assert_eq!(
            suggestions[0]["contentHash"],
            format!("{:x}", sha2::Sha256::digest(started.as_bytes())),
            "anchored on the reader's starting text: {body}"
        );
        pushed.push(suggestions[0].clone());
    }
    assert_eq!(pushed[1]["beforeLines"], "", "{}", pushed[1]);
    assert_eq!(pushed[1]["anchorStart"], 1, "{}", pushed[1]);

    // The owner decides in their running workspace, which holds the new
    // versions.
    let peer =
        TestWorkspaceRuntimePeer::attach(&fixture.app, &fixture.source, fixture.owner.id).await;
    let repo_id = fixture.source.repo_id;
    peer.seed_file(repo_id, PLAN, &owner_plan).await;
    peer.seed_file(repo_id, EMPTY, "owner text\n").await;
    peer.seed_file(repo_id, TWINS, &owner_twins).await;

    // The plan's lines are still there, once: the edit applies there,
    // under the owner's intro.
    let (status, body) = post(
        &owner,
        &suggestion_url(fixture.source.id, &pushed[0], "accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["suggestion"]["status"], "accepted", "{body}");
    assert_eq!(
        peer.file_content(repo_id, PLAN).await,
        Some(format!("Owner's intro.\n{reader_plan}"))
    );

    for (suggestion, path, live, reason) in [
        (
            &pushed[1],
            EMPTY,
            "owner text\n".to_string(),
            "captured source no longer matches",
        ),
        (
            &pushed[2],
            TWINS,
            owner_twins.clone(),
            "source is ambiguous",
        ),
    ] {
        let (status, body) = post(
            &owner,
            &suggestion_url(fixture.source.id, suggestion, "accept"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}: {body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains(reason),
            "{path}: {body}"
        );
        assert_eq!(
            peer.file_content(repo_id, path).await,
            Some(live),
            "{path} is untouched"
        );
        let row_status: String =
            sqlx::query_scalar("SELECT status FROM review_suggestions WHERE id = $1")
                .bind(Uuid::parse_str(suggestion["id"].as_str().unwrap()).unwrap())
                .fetch_one(&fixture.app.db)
                .await
                .expect("suggestion status");
        assert_eq!(row_status, "open", "{path} stays open for attention");
    }
    drop(peer);
    fixture.app.cleanup().await;
}

/// A reader starts from pushed content even while stopped dirty work is retained.
/// The private flush snapshot cannot become that reader's suggestion baseline.
#[tokio::test]
async fn pushed_content_is_the_readers_start_while_stopped_dirty_work_is_retained() {
    let fixture = files_fixture(&[]).await;
    let app = &fixture.app;
    let source = &fixture.source;
    let branch_head = fixture.head.clone();
    let snapshot = commit_file_on(
        &fixture.repo_path,
        &branch_head,
        "README.md",
        b"saved dirty work\n",
        "flush snapshot",
    );
    let flush = Uuid::new_v4();
    git_stdout(
        &fixture.repo_path,
        &[
            "update-ref",
            &format!("refs/g5/flush/{}/{flush}/snapshot", source.id),
            &snapshot,
        ],
    );
    let receipts = json!([{
        "repo_id": source.repo_id, "branch_oid": branch_head, "local_branch_oid": branch_head,
        "local_head_oid": branch_head, "head_ref": format!("refs/heads/{}", source.name),
        "sequencer": "none", "snapshot_oid": snapshot, "dirty": true,
        "dirty_paths": ["README.md"]
    }]);
    sqlx::query(
        "INSERT INTO workspace_flushes(flush_id,org_id,workspace_id,origin,vm_id,epoch,reason,\
         required_repo_ids,repos,complete_at,request_key)
         VALUES ($1,$2,$3,'flush',$4,1,'shutdown',$5,$6,now(),'file-suggestion-flush')",
    )
    .bind(flush)
    .bind(source.org_id)
    .bind(source.id)
    .bind(Uuid::new_v4().to_string())
    .bind(vec![source.repo_id])
    .bind(&receipts)
    .execute(&app.db)
    .await
    .expect("flush receipt");
    sqlx::query(
        "INSERT INTO workspace_vms(org_id,workspace_id,status,last_flush_id) \
         VALUES ($1,$2,'stopped',$3)",
    )
    .bind(source.org_id)
    .bind(source.id)
    .bind(flush)
    .execute(&app.db)
    .await
    .expect("stopped incarnation");

    let reader = app.authenticated_client(&fixture.reader);
    let view = read_file(&reader, source, "README.md").await;
    assert_ne!(view["source"], "flush", "{view}");
    assert_eq!(view["content"], "test workspace\n");

    for actor in [&fixture.reader, &fixture.owner] {
        let changes = app
            .authenticated_client(actor)
            .get(&format!("/api/v1/workspaces/{}/changes", source.id))
            .await;
        assert_eq!(changes.status(), StatusCode::OK);
        let body: Value = changes.json().await.unwrap();
        let report = data(&body);
        assert!(report["flush_id"].is_null(), "{body}");
        let member = report["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["repo_id"] == source.repo_id.to_string())
            .unwrap();
        assert_eq!(member["internal_ref"], branch_head);
        for field in ["snapshot_oid", "local_head_oid", "dirty", "sequencer"] {
            assert!(member[field].is_null(), "private {field}: {body}");
        }
    }

    let (status, body) = push_file_suggestion(
        &reader,
        source,
        "README.md",
        "test workspace\n",
        "test workspace, reviewed\n",
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "fresh against the pushed branch: {body}"
    );
    assert_eq!(
        data(&body)["publication"]["baseCommitVector"],
        json!([{"repoId": source.repo_id, "sourceHead": branch_head}]),
        "the vector names the branch head, not the snapshot commit"
    );
    let suggestion = &data(&body)["suggestions"][0];
    assert_eq!(suggestion["beforeLines"], "test workspace\n");
    assert_eq!(suggestion["afterLines"], "test workspace, reviewed\n");

    let (status, body) = push_file_suggestion(
        &reader,
        source,
        "README.md",
        "saved dirty work\n",
        "saved dirty work, reviewed\n",
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the private flush text is not this reader's baseline: {body}"
    );
    assert_eq!(body["error"]["code"], "FILE_VERSION_CONFLICT", "{body}");
    assert_eq!(body["error"]["details"]["hash"], view["hash"], "{body}");
    fixture.app.cleanup().await;
}

/// g5.governance.file-suggestions.additive: every push adds suggestions in
/// its own publication. Neither a later push nor the same reader's rounds,
/// in place or from a review workspace, supersede one.
#[tokio::test]
async fn file_suggestion_pushes_add_up_and_no_round_supersedes_them() {
    use g5_api::services::workspace_review_service as reviews;
    const NOTES: &str = "docs/notes.md";
    let base = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    let fixture = files_fixture(&[(NOTES, base.as_bytes())]).await;
    let app = &fixture.app;
    let source = &fixture.source;
    let reader = app.authenticated_client(&fixture.reader);
    let owner = app.authenticated_client(&fixture.owner);
    let mut push_ids = Vec::new();
    for proposed in [base.replace("two", "TWO"), base.replace("nine", "NINE")] {
        let (status, body) =
            push_file_suggestion(&reader, source, NOTES, base, &proposed, json!({})).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        push_ids.push(data(&body)["publication"]["id"].clone());
    }

    // The reader's rounds retire earlier rounds of their own shape only:
    // the third (in place again) supersedes the first.
    let copy = TestWorkspace::create(app, &fixture.org).await;
    for review_workspace_id in [None, Some(copy.id), None] {
        let mut db = app.scoped_db_for_org(fixture.org.id).await;
        reviews::create_publication(
            &mut db,
            reviews::NewPublication {
                org_id: fixture.org.id,
                source_workspace_id: source.id,
                review_workspace_id,
                reviewer_user_id: fixture.reader.id,
                author_kind: "human".to_string(),
                author_label: "reader".to_string(),
                base_commit_vector: json!([]),
                summary: "a round".to_string(),
                verdict: Some("comment_only".to_string()),
                scores: None,
                kind: reviews::PUBLICATION_KIND_ROUND.to_string(),
            },
        )
        .await
        .expect("record a round");
        db.commit().await.expect("commit the round");
    }

    let (status, body) = get(
        &owner,
        &format!("/api/v1/workspaces/{}/review/publications", source.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let publications = data(&body)["publications"].as_array().cloned().unwrap();
    assert_eq!(publications.len(), 5, "{body}");
    for id in &push_ids {
        let push = publications
            .iter()
            .find(|publication| publication["id"] == *id)
            .expect("the push's publication");
        assert_eq!(push["kind"], "annotation", "{push}");
        assert!(
            push["supersededBy"].is_null(),
            "a push is never superseded: {push}"
        );
        assert_eq!(push["openCount"], 1, "{push}");
    }
    assert_eq!(
        publications
            .iter()
            .filter(|p| p["kind"] == "round" && !p["supersededBy"].is_null())
            .count(),
        1,
        "rounds still supersede rounds of their shape: {body}"
    );
    let (status, listed) = get(
        &owner,
        &format!(
            "/api/v1/workspaces/{}/review/suggestions?openOnly=true&path={NOTES}",
            source.id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        data(&listed)["suggestions"].as_array().map(Vec::len),
        Some(2),
        "{listed}"
    );
    fixture.app.cleanup().await;
}

/// CRLF bytes survive a FILES push and the owner's Accept, which writes
/// the running workspace, then commits and pushes as for rounds. The reader
/// withdraws another push of theirs, and the obsolete notice of a
/// suggestion without a review workspace points back to FILES while a
/// round's keeps its advice.
#[tokio::test]
async fn the_owner_accepts_a_crlf_file_suggestion_live_and_the_reader_withdraws_another() {
    const WINDOWS: &str = "docs/windows.txt";
    const OTHER: &str = "docs/other.md";
    let crlf = "one\r\ntwo\r\nthree\r\n";
    let proposed = "one\r\n2\r\nthree\r\n";
    let other = "keep\nchange me\nkeep\n";
    let fixture = files_fixture(&[(WINDOWS, crlf.as_bytes()), (OTHER, other.as_bytes())]).await;
    let app = &fixture.app;
    let source = &fixture.source;
    let reader = app.authenticated_client(&fixture.reader);
    let owner = app.authenticated_client(&fixture.owner);

    let (status, body) =
        push_file_suggestion(&reader, source, WINDOWS, crlf, proposed, json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let windows = data(&body)["suggestions"][0].clone();
    assert_eq!(
        windows["beforeLines"], crlf,
        "carriage returns kept: {windows}"
    );
    assert_eq!(windows["afterLines"], proposed, "{windows}");
    assert_eq!(windows["edits"][0]["beforeText"], "two", "{windows}");
    assert_eq!(windows["edits"][0]["afterText"], "2", "{windows}");
    assert_eq!(
        thread_opening(app, &windows).await,
        (fixture.reader.id, format!("Suggested change to {WINDOWS}")),
        "without a note the thread opens with the default text"
    );

    let (status, body) = push_file_suggestion(
        &reader,
        source,
        OTHER,
        other,
        "keep\nchanged\nkeep\n",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let withdrawn = data(&body)["suggestions"][0].clone();

    // Only the reader withdraws their suggestion; the owner hears once.
    let (status, body) = post(
        &owner,
        &suggestion_url(source.id, &withdrawn, "withdraw"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let owner_before = review_notice_count(app, fixture.owner.id).await;
    let (status, body) = post(
        &reader,
        &suggestion_url(source.id, &withdrawn, "withdraw"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["suggestion"]["status"], "withdrawn", "{body}");
    assert_eq!(
        review_notice_count(app, fixture.owner.id).await,
        owner_before + 1
    );

    // Accept is the owner's, in their running workspace.
    let peer = TestWorkspaceRuntimePeer::attach(app, source, fixture.owner.id).await;
    peer.seed_file(source.repo_id, WINDOWS, crlf).await;
    peer.seed_file(source.repo_id, OTHER, other).await;
    let (status, body) = post(
        &owner,
        &suggestion_url(source.id, &withdrawn, "accept"),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a withdrawn suggestion: {body}"
    );
    let (status, body) = post(
        &owner,
        &suggestion_url(source.id, &windows, "accept"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["suggestion"]["status"], "accepted", "{body}");
    assert_eq!(data(&body)["checkpointPending"], false, "{body}");
    assert_eq!(
        peer.file_content(source.repo_id, WINDOWS).await.as_deref(),
        Some(proposed),
        "the CRLF bytes are written"
    );
    assert_eq!(
        peer.file_content(source.repo_id, OTHER).await.as_deref(),
        Some(other),
        "the withdrawn suggestion changed nothing"
    );
    let driven = peer.received_rpc_methods().await;
    let write_at = driven
        .iter()
        .position(|m| m == "fs.write")
        .expect("written into the working tree");
    let commit_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/commit")
        .expect("committed");
    let push_at = driven
        .iter()
        .position(|m| m == "modules.invoke:workspace.changes/push")
        .expect("pushed");
    assert!(write_at < commit_at && commit_at < push_at, "{driven:?}");
    let windows_id = Uuid::parse_str(windows["id"].as_str().unwrap()).unwrap();
    assert!(
        thread_replies(&owner, source.id, windows_id)
            .await
            .iter()
            .any(|reply| reply["content"] == format!("Accepted: committed and pushed {WINDOWS}.")),
        "the decision is recorded in the thread"
    );
    assert!(
        notifications_of(&reader, source.id)
            .await
            .iter()
            .any(|notice| notice["title"]
                == format!("{} accepted your suggestion", fixture.owner.display_name)),
        "the reader, who holds no review copy, is told in the source"
    );

    // A row without per-edit identities still takes the obsolete path when
    // its lines are gone: a round from a review workspace keeps its advice,
    // a publication without one points back to FILES.
    let copy = TestWorkspace::create(app, &fixture.org).await;
    peer.seed_file(source.repo_id, "docs/legacy.md", "other\n")
        .await;
    for (review_workspace_id, kind, round, reviewer_step, owner_step) in [
        (
            None,
            "annotation",
            10,
            "Suggest the change again from FILES.".to_string(),
            "Ask the reviewer to suggest the change again",
        ),
        (
            Some(copy.id),
            "round",
            11,
            format!(
                "Update your review workspace from {} and push again.",
                source.name
            ),
            "Ask the reviewer to update their copy and push again",
        ),
    ] {
        let publication_id: Uuid = sqlx::query_scalar(
            "INSERT INTO review_publications (org_id, source_workspace_id, review_workspace_id, \
             reviewer_user_id, author_kind, author_label, round, kind) \
             VALUES ($1, $2, $3, $4, 'human', 'reader', $5, $6) RETURNING id",
        )
        .bind(source.org_id)
        .bind(source.id)
        .bind(review_workspace_id)
        .bind(fixture.reader.id)
        .bind(round)
        .bind(kind)
        .fetch_one(&app.db)
        .await
        .expect("legacy publication");
        let suggestion_id: Uuid = sqlx::query_scalar(
            "INSERT INTO review_suggestions (org_id, publication_id, repo_id, path, kind, \
             before_lines, after_lines, anchor_start, author_kind, author_label) \
             VALUES ($1, $2, $3, 'docs/legacy.md', 'hunk', $4, $4, 1, 'human', 'reader') \
             RETURNING id",
        )
        .bind(source.org_id)
        .bind(publication_id)
        .bind(source.repo_id)
        .bind("gone\n")
        .fetch_one(&app.db)
        .await
        .expect("legacy suggestion");
        let (status, body) = post(
            &owner,
            &format!(
                "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/accept",
                source.id
            ),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{kind}: {body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .ends_with(&format!("marked obsolete. {owner_step}")),
            "{kind}: {body}"
        );
        let message = format!(
            "{}: the lines changed since you published. {reviewer_step}",
            source.name
        );
        assert!(
            notifications_of(&reader, source.id)
                .await
                .iter()
                .any(|notice| notice["message"] == message),
            "{kind}: {message}"
        );
    }
    drop(peer);
    fixture.app.cleanup().await;
}

/// The route's own body limit admits two texts at the 2 MiB text limit in
/// JSON's worst-case escaping, so a small edit to a file just under 2 MiB
/// is accepted, while each text stays bounded on its own.
#[tokio::test]
async fn a_small_edit_to_a_file_near_the_text_limit_fits_the_request_body_limit() {
    const LARGE: &str = "docs/large.md";
    let large: String = (0..62_000)
        .map(|n| format!("line {n:06} of a large document\n"))
        .collect();
    assert!(
        large.len() > 1_900_000 && large.len() < FILE_TEXT_LIMIT,
        "{}",
        large.len()
    );
    let fixture = files_fixture(&[(LARGE, large.as_bytes())]).await;
    let source = &fixture.source;
    let reader = fixture.app.authenticated_client(&fixture.reader);

    // Both texts make about 3.8 MB of JSON, past axum's 2 MB default.
    let edited = large.replace(
        "line 031000 of a large document\n",
        "line 031000 of a larger document\n",
    );
    let (status, body) =
        push_file_suggestion(&reader, source, LARGE, &large, &edited, json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let suggestions = data(&body)["suggestions"].as_array().cloned().unwrap();
    assert_eq!(suggestions.len(), 1, "{body}");
    assert_eq!(suggestions[0]["anchorStart"], 30_998);
    assert_eq!(
        suggestions[0]["afterLines"]
            .as_str()
            .unwrap()
            .lines()
            .nth(3),
        Some("line 031000 of a larger document")
    );

    // The worst case, two 2 MiB texts of control characters sent as six
    // bytes each, still reaches the handler (refused only as unchanged).
    let controls = format!("{}\n", "\u{1}".repeat(63)).repeat(FILE_TEXT_LIMIT / 64);
    assert_eq!(controls.len(), FILE_TEXT_LIMIT);
    let (status, body) = push_file_suggestion(
        &reader,
        source,
        "README.md",
        &controls,
        &controls,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(refusal(&body), ("content", "No changes to suggest"));
    fixture.app.cleanup().await;
}

// ─── The audience grant is held through reader writes (G5-Lean#660) ──────────
//
// `g5.security.current-authority` under `g5.workspace.audience`: a reader's
// reply, FILES push, withdrawal and round publication rely on their membership
// to the source. The membership is held on the writing transaction, so a
// revocation already in flight makes the write wait and then refuses it with
// nothing recorded, while an admitted write commits.

async fn reactivate_reader(app: &TestApp, source_id: Uuid, reader_id: Uuid) {
    sqlx::query("UPDATE org_memberships SET deactivated_at=NULL WHERE org_id=(SELECT org_id FROM workspaces WHERE id=$1) AND user_id=$2")
        .bind(source_id).bind(reader_id).execute(&app.db).await.unwrap();
}

fn client_for(app: &TestApp, user: &TestUser) -> helpers::AuthenticatedClient {
    helpers::AuthenticatedClient::new(
        reqwest::Client::new(),
        app.base_url.clone(),
        user.jwt.clone(),
    )
}

/// Runs `request` while the reader's membership to `source_id` is being
/// revoked: the revoke is in flight when the request arrives, the request
/// waits on the audience hold, and the revoke commits first.
async fn race_revocation<Fut>(
    app: &TestApp,
    source_id: Uuid,
    reader_id: Uuid,
    request: Fut,
) -> (StatusCode, Value)
where
    Fut: std::future::Future<Output = (StatusCode, Value)> + Send + 'static,
{
    let mut revoke = app.db.begin().await.expect("revoker transaction");
    let removed =
        sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id = (SELECT org_id FROM workspaces WHERE id = $1) AND user_id = $2")
            .bind(source_id)
            .bind(reader_id)
            .execute(&mut *revoke)
            .await
            .expect("revoke in flight")
            .rows_affected();
    assert_eq!(removed, 1);
    let pending = tokio::spawn(request);
    helpers::wait_for_lock_wait(app).await;
    assert!(
        !pending.is_finished(),
        "the write waits for the in-flight revocation"
    );
    revoke.commit().await.expect("the revocation commits first");
    pending.await.expect("the request completes")
}

async fn posted(
    client: helpers::AuthenticatedClient,
    path: String,
    body: Value,
) -> (StatusCode, Value) {
    let response = client.post_json(&path, &body).await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn publications_of(app: &TestApp, source_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM review_publications WHERE source_workspace_id = $1")
        .bind(source_id)
        .fetch_one(&app.db)
        .await
        .expect("count publications")
}

async fn suggestion_status(app: &TestApp, suggestion_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM review_suggestions WHERE id = $1")
        .bind(Uuid::parse_str(suggestion_id).unwrap())
        .fetch_one(&app.db)
        .await
        .expect("suggestion status")
}

async fn replies_of(
    client: &helpers::AuthenticatedClient,
    source_id: Uuid,
    suggestion_id: &str,
) -> usize {
    let (status, body) = get(
        client,
        &format!("/api/v1/workspaces/{source_id}/review/suggestions/{suggestion_id}/replies"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    data(&body)["replies"].as_array().map_or(0, Vec::len)
}

#[tokio::test]
async fn a_readers_review_writes_hold_their_membership_until_commit() {
    const GUIDE: &str = "docs/guide.md";
    let base = "# Guide\n\nInstall the tool.\nRun the tool.\n\n## Notes\n\nNothing yet.\n";
    let fixture = files_fixture(&[(GUIDE, base.as_bytes())]).await;
    let app = &fixture.app;
    let source = &fixture.source;
    let reader_id = fixture.reader.id;
    reactivate_reader(app, source.id, reader_id).await;
    let reader = app.authenticated_client(&fixture.reader);

    // Admitted with the membership held: the reader's push is recorded.
    let edited = base.replace("Run the tool.", "Run the tool daily.");
    let (status, pushed) =
        push_file_suggestion(&reader, source, GUIDE, base, &edited, json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{pushed}");
    let suggestion_id = data(&pushed)["suggestions"][0]["id"]
        .as_str()
        .expect("suggestion id")
        .to_string();
    let publications = publications_of(app, source.id).await;
    let replies = replies_of(&reader, source.id, &suggestion_id).await;
    let reply_path = format!(
        "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/replies",
        source.id
    );
    let withdraw_path = format!(
        "/api/v1/workspaces/{}/review/suggestions/{suggestion_id}/withdraw",
        source.id
    );

    // A FILES push whose membership is being revoked waits, then is refused
    // with nothing recorded.
    let (status, body) = race_revocation(
        app,
        source.id,
        reader_id,
        posted(
            client_for(app, &fixture.reader),
            file_suggestions_url(source.id),
            json!({
                "repoId": source.repo_id,
                "path": GUIDE,
                "baseContent": base,
                "content": base.replace("Install the tool.", "Fetch the tool."),
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(publications_of(app, source.id).await, publications);

    // A reply likewise.
    reactivate_reader(app, source.id, reader_id).await;
    let (status, body) = race_revocation(
        app,
        source.id,
        reader_id,
        posted(
            client_for(app, &fixture.reader),
            reply_path.clone(),
            json!({ "content": "while being removed" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    reactivate_reader(app, source.id, reader_id).await;
    assert_eq!(
        replies_of(&reader, source.id, &suggestion_id).await,
        replies
    );

    // A withdrawal likewise: the suggestion stays open.
    let (status, body) = race_revocation(
        app,
        source.id,
        reader_id,
        posted(
            client_for(app, &fixture.reader),
            withdraw_path.clone(),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(suggestion_status(app, &suggestion_id).await, "open");

    // With no revocation in flight, the same writes commit.
    reactivate_reader(app, source.id, reader_id).await;
    let (status, body) = post(&reader, &reply_path, json!({ "content": "still active" })).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        replies_of(&reader, source.id, &suggestion_id).await,
        replies + 1
    );
    let (status, body) = post(&reader, &withdraw_path, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(suggestion_status(app, &suggestion_id).await, "withdrawn");

    // After a committed revocation nothing is admitted.
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id = (SELECT org_id FROM workspaces WHERE id = $1) AND user_id = $2")
        .bind(source.id)
        .bind(reader_id)
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = post(&reader, &reply_path, json!({ "content": "after removal" })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    fixture.app.cleanup().await;
}

#[tokio::test]
async fn a_reviewers_round_holds_their_membership_until_commit() {
    let before = "The red fox.\nBridge one.\nBridge two.\nBridge three.\nBridge four.\n";
    let after = format!("{}\nFirst added block.\n", before.replace("red", "blue"));
    let fixture = publish_per_edit_doc(before, &after).await;
    let app = &fixture.app;
    let source_id = fixture.source.id;
    let reviewer_id = fixture.reviewer.id;
    reactivate_reader(app, source_id, reviewer_id).await;
    let rounds = publications_of(app, source_id).await;

    // The reviewer's next round is committed on their copy.
    let repo_path = app.repo_path(fixture.source.repo_id).await;
    let (_, copy_head) =
        review_branch(app, fixture.review_workspace_id, fixture.source.repo_id).await;
    let review_head = commit_file_on(
        &repo_path,
        &copy_head,
        PER_EDIT_DOC,
        format!("{after}\nSecond added block.\n").as_bytes(),
        "reviewer: another round",
    );
    helpers::seed_workspace_authority_head(
        app,
        fixture.review_workspace_id,
        fixture.source.repo_id,
        &review_head,
    )
    .await;

    // Publishing it while the membership is being revoked waits on the
    // source's audience hold, then is refused: no round is recorded.
    let publish_path = format!(
        "/api/v1/workspaces/{}/review/publish",
        fixture.review_workspace_id
    );
    let (status, body) = race_revocation(
        app,
        source_id,
        reviewer_id,
        posted(
            client_for(app, &fixture.reviewer),
            publish_path.clone(),
            json!({ "summary": "round two", "verdict": "comment_only" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(publications_of(app, source_id).await, rounds);

    // Reactivated, the same round publishes.
    reactivate_reader(app, source_id, reviewer_id).await;
    let reviewer = app.authenticated_client(&fixture.reviewer);
    let (status, body) = post(
        &reviewer,
        &publish_path,
        json!({ "summary": "round two", "verdict": "comment_only" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(publications_of(app, source_id).await, rounds + 1);

    fixture.cleanup().await;
}
