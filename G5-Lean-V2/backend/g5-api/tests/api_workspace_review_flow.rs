//! Real HTTP/Postgres/Git acceptance for stopped workspace reviews. Running VM
//! comparisons additionally live in the workspace-review browser value loops.
use crate::helpers;
use helpers::TestApp;
use reqwest::StatusCode;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::helpers::review_flow;
use review_flow::{body, Flow};

#[tokio::test]
async fn a_decision_requires_the_reviewed_vector_after_a_push() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let task = f.primary_request(&app, "se").await;
    let captured = f.overview(&app, &f.reviewer).await;
    let pushed = f.publish_file(&app, f.docs, "later.md", b"H2\n").await;
    assert_ne!(captured["heads"], pushed["heads"]);
    let mut decision = json!({"definitionId":f.definition,"requestId":task["id"],"outcome":"approved","phase":"se"});
    let reviewer = app.authenticated_client(&f.reviewer);
    for null in [false, true] {
        if null {
            decision["reviewedGitHeads"] = Value::Null;
        }
        let response = reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            response.text().await.unwrap()
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM approval_statuses WHERE source_workspace_id=$1",
        )
        .bind(f.source)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(count, 0);
        let state: String = sqlx::query_scalar("SELECT state FROM approval_requests WHERE id=$1")
            .bind(Uuid::parse_str(task["id"].as_str().unwrap()).unwrap())
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(state, "pending");
    }
    let vector = captured["heads"].as_array().unwrap();
    let mut forged = vector.clone();
    forged[0]["workspaceId"] = json!(Uuid::new_v4());
    let mut invalid_sha = vector.clone();
    invalid_sha[0]["headCommit"] = json!("not-a-sha");
    for invalid in [
        json!([]),
        json!([vector[0].clone(), vector[0].clone()]),
        json!([vector[0].clone()]),
        json!(forged),
        json!(invalid_sha),
    ] {
        decision["reviewedGitHeads"] = invalid;
        let response = reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            response.text().await.unwrap()
        );
    }
    decision["reviewedGitHeads"] = captured["heads"].clone();
    body(
        reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await,
        StatusCode::OK,
    )
    .await;
    let recorded: Value = sqlx::query_scalar(
        "SELECT reviewed_git_heads FROM approval_statuses WHERE source_workspace_id=$1",
    )
    .bind(f.source)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(recorded, captured["heads"]);
    assert_ne!(recorded, pushed["heads"]);
    // Equal lost-response retries keep the original revision after another push.
    let newest = f.publish_file(&app, f.docs, "newest.md", b"H3\n").await;
    body(
        reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await,
        StatusCode::OK,
    )
    .await;
    decision["reviewedGitHeads"] = pushed["heads"].clone();
    body(
        reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    let fresh = f.primary_request(&app, "ie").await;
    decision["requestId"] = fresh["id"].clone();
    decision["reviewedGitHeads"] = newest["heads"].clone();
    body(
        reviewer
            .post_json(&f.path("approvals/decisions"), &decision)
            .await,
        StatusCode::OK,
    )
    .await;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM approval_statuses WHERE source_workspace_id=$1 AND reviewed_git_heads=$2")
        .bind(f.source).bind(&newest["heads"]).fetch_one(&app.db).await.unwrap();
    assert_eq!(count, 1);
    app.cleanup().await;
}

#[tokio::test]
async fn docs_push_preserves_captured_review_vector_history_and_personal_checkpoint() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    let task = f.primary_request(&app, "se").await;
    let captured = f.overview(&app, &f.reviewer).await;
    let pushed = f
        .publish_file(&app, f.docs, "plan.md", b"New implementation plan\n")
        .await;
    assert_eq!(pushed["heads"].as_array().unwrap().len(), 2);
    let all_files = body(
        app.authenticated_client(&f.reviewer)
            .get(&f.path("review-flow/changes?mode=all"))
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(all_files["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| { file["repoId"] == f.docs.to_string() && file["path"] == "plan.md" }));
    let code = |v: &Value| {
        v["heads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["repoId"] == f.parent.repo_id.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(code(&captured), code(&pushed));
    assert!(pushed["pushedAt"][f.docs.to_string()].is_string());
    let decision = json!({"definitionId":f.definition,"requestId":task["id"],"phase":"se","outcome":"approved","reviewedGitHeads":captured["heads"]});
    let reviewer = app.authenticated_client(&f.reviewer);
    for _ in 0..2 {
        body(
            reviewer
                .post_json(&f.path("approvals/decisions"), &decision)
                .await,
            StatusCode::OK,
        )
        .await;
    }
    let mut retargeted = decision.clone();
    retargeted["reviewedGitHeads"] = pushed["heads"].clone();
    body(
        reviewer
            .post_json(&f.path("approvals/decisions"), &retargeted)
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    let checkpoint = body(
        reviewer
            .get(&f.path(&format!(
                "review-flow/changes?mode=last_review&definitionId={}&phase=se",
                f.definition
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(checkpoint["beforeHeads"], captured["heads"]);
    assert_eq!(checkpoint["afterHeads"], pushed["heads"]);
    assert_eq!(checkpoint["files"].as_array().unwrap().len(), 1);
    assert_eq!(checkpoint["files"][0]["path"], "plan.md");
    let other = body(
        app.authenticated_client(&f.second)
            .get(&f.path(&format!(
                "review-flow/changes?mode=last_review&definitionId={}&phase=se",
                f.definition
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(other["baselineAvailable"], false);
    let newer = f
        .publish_file(&app, f.parent.repo_id, "review.txt", b"Code change\n")
        .await;
    assert_eq!(
        newer["pushedAt"][f.docs.to_string()],
        pushed["pushedAt"][f.docs.to_string()]
    );
    let history = body(
        reviewer.get(&f.path("review-flow/history")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(history["rows"][0]["heads"], newer["heads"]);
    assert_eq!(history["rows"][1]["heads"], pushed["heads"]);
    let id = history["rows"][0]["id"].as_str().unwrap();
    let diff = body(
        reviewer
            .get(&f.path(&format!("review-flow/changes?mode=history&revisionId={id}")))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(diff["beforeHeads"], pushed["heads"]);
    assert_eq!(diff["files"][0]["repoId"], f.parent.repo_id.to_string());
    app.cleanup().await;
}

#[tokio::test]
async fn binary_files_and_cancelled_requests_have_honest_durable_projections() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.submit(&app).await;
    f.publish_file(&app, f.parent.repo_id, "image.bin", &[0, 255, 10])
        .await;
    let reader = app.authenticated_client(&f.reviewer);
    let diff = body(
        reader
            .get(&f.path(&format!(
                "review-flow/changes?mode=base&repoId={}&path=image.bin",
                f.parent.repo_id
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(diff["file"]["binary"], true);
    assert!(diff["file"]["afterCommit"].is_string());
    let task = f.primary_request(&app, "be").await;
    let id = task["id"].as_str().unwrap();
    body(
        app.authenticated_client(&f.owner)
            .post_json(
                &f.path(&format!("approvals/requests/{id}/cancel")),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    let timeline = body(reader.get(&f.path("activity")).await, StatusCode::OK).await;
    let events = timeline["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["requestId"] == id)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert_eq!(events.iter().filter(|e| e["cancelled"] == true).count(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn stopped_pr_requests_and_canonical_replies_survive_phase_moves() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let initial = f.overview(&app, &f.visitor).await;
    assert!(initial["pr"].is_null());
    assert_eq!(initial["heads"].as_array().unwrap().len(), 2);
    assert_eq!(initial["baseHeads"].as_array().unwrap().len(), 2);
    let owner = app.authenticated_client(&f.owner);
    let reviewer = app.authenticated_client(&f.reviewer);
    let visitor = app.authenticated_client(&f.visitor);
    let request = json!({"definitionId":f.definition,"definitionVersion":1,"phase":"se","actorUserIds":[f.reviewer.id]});
    // No PR is required to request (g5.governance.approval.request-pr); this
    // test follows PR-bound requests, so the early one is cancelled.
    let early = body(
        owner
            .post_json(&f.path("approvals/requests"), &request)
            .await,
        StatusCode::CREATED,
    )
    .await;
    body(
        owner
            .post_json(
                &f.path(&format!(
                    "approvals/requests/{}/cancel",
                    early["requests"][0]["requestId"].as_str().unwrap()
                )),
                &json!({}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    f.submit(&app).await;
    body(owner.post_json(&f.path("approvals/requests"),&json!({"definitionId":f.definition,"definitionVersion":1,"phase":"se","actorUserIds":[f.owner.id]})).await,StatusCode::UNPROCESSABLE_ENTITY).await;
    let mut decision_ids = Vec::new();
    for phase in ["be", "se", "ie"] {
        body(
            owner
                .patch_json(
                    &format!("/api/v1/workspaces/{}", f.source),
                    &json!({"phase":phase}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        f.request(&app, phase).await;
        let state = f.overview(&app, &f.reviewer).await;
        let task = state["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["state"] == "pending" && r["actorUserId"] == f.reviewer.id.to_string())
            .unwrap();
        let request_id = task["id"].clone();
        let heads = state["heads"].clone();
        let decision = json!({"definitionId":f.definition,"requestId":request_id,"phase":phase,"outcome":"approved","comment":format!("Reviewed {phase}"),"reviewedGitHeads":heads});
        body(
            owner
                .post_json(&f.path("approvals/decisions"), &decision)
                .await,
            StatusCode::FORBIDDEN,
        )
        .await;
        assert!(!visitor
            .post_json(&f.path("approvals/decisions"), &decision)
            .await
            .status()
            .is_success());
        body(
            reviewer
                .post_json(&f.path("approvals/decisions"), &decision)
                .await,
            StatusCode::OK,
        )
        .await;
        body(
            reviewer
                .post_json(&f.path("approvals/decisions"), &decision)
                .await,
            StatusCode::OK,
        )
        .await;
        let activity = body(visitor.get(&f.path("activity")).await, StatusCode::OK).await;
        let event = activity["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry["kind"] == "approval_decision"
                    && entry["comment"] == format!("Reviewed {phase}")
            })
            .unwrap();
        assert_eq!(event["pullRequestId"], state["pr"]["id"]);
        let after = f.overview(&app, &f.owner).await;
        let rows = after["decisions"].as_array().unwrap();
        assert_eq!(
            rows.iter().filter(|d| d["requestId"] == request_id).count(),
            1
        );
        let row = rows.iter().find(|d| d["requestId"] == request_id).unwrap();
        assert_eq!(row["reviewedGitHeads"], heads);
        assert_eq!(
            after["requests"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["state"] == "pending")
                .count(),
            1
        );
        let comment = row["id"].as_str().unwrap();
        let reply_id = Uuid::new_v4();
        let reply = json!({"id":reply_id,"content":"Owner response"});
        body(
            owner
                .post_json(&format!("/api/v1/comments/{comment}/replies"), &reply)
                .await,
            StatusCode::CREATED,
        )
        .await;
        body(
            owner
                .post_json(&format!("/api/v1/comments/{comment}/replies"), &reply)
                .await,
            StatusCode::CREATED,
        )
        .await;
        let replies = body(
            visitor
                .get(&format!("/api/v1/comments/{comment}/replies"))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(replies.as_array().unwrap().len(), 1);
        let checkpoint = body(
            reviewer
                .get(&f.path(&format!(
                    "review-flow/changes?mode=last_review&definitionId={}&phase={phase}",
                    f.definition
                )))
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(checkpoint["beforeHeads"], heads);
        assert!(checkpoint["files"].as_array().unwrap().is_empty());
        decision_ids.push(row["id"].clone());
    }
    let timeline = body(visitor.get(&f.path("activity")).await, StatusCode::OK).await;
    for id in decision_ids {
        assert_eq!(
            timeline["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "approval_decision" && e["thread"]["id"] == id)
                .count(),
            1,
            "{timeline}"
        );
    }
    assert_eq!(
        f.overview(&app, &f.owner).await["decisions"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    app.cleanup().await;
}

#[tokio::test]
async fn readers_comment_on_real_pushed_content_and_retry_without_duplicate_timeline_threads() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let reader = app.authenticated_client(&f.reviewer);
    let owner = app.authenticated_client(&f.owner);
    let view = f.overview(&app, &f.reviewer).await;
    let head = view["heads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    let id = Uuid::new_v4();
    let content = "before\nreview\n";
    let comment = json!({"id":id,"scope":"line","repoId":f.parent.repo_id,"filePath":"review.txt","lineNumber":2,"lineHash":hex::encode(Sha256::digest(b"review")),"contentHash":hex::encode(Sha256::digest(content.as_bytes())),"anchorContext":content,"commitSha":head["headCommit"],"content":"Explain this decision"});
    for _ in 0..2 {
        body(
            reader.post_json(&f.path("comments"), &comment).await,
            StatusCode::CREATED,
        )
        .await;
    }
    let timeline = body(owner.get(&f.path("activity")).await, StatusCode::OK).await;
    assert_eq!(
        timeline["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "workspace_comment" && e["comment"]["id"] == id.to_string())
            .count(),
        1
    );
    let historical = body(
        reader
            .get(&f.path(&format!(
                "review-flow/changes?repoId={}&path=review.txt&commentId={id}",
                f.parent.repo_id
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(historical["file"]["after"], content);
    let pushed = f
        .publish_file(&app, f.parent.repo_id, "review.txt", b"newer\ncontent\n")
        .await;
    let retained = body(
        reader
            .get(&f.path(&format!(
                "review-flow/changes?repoId={}&path=review.txt&commentId={id}",
                f.parent.repo_id
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(retained["file"]["after"], content);
    assert_eq!(retained["afterHeads"], historical["afterHeads"]);
    let current_head = pushed["heads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["repoId"] == f.parent.repo_id.to_string())
        .unwrap()["headCommit"]
        .clone();
    sqlx::query("DELETE FROM workspace_revision_events WHERE workspace_id=$1 AND heads @> $2")
        .bind(f.source)
        .bind(json!([{"repoId":f.parent.repo_id,"headCommit":current_head}]))
        .execute(&app.db)
        .await
        .unwrap();
    let current_id = Uuid::new_v4();
    let mut current_comment = comment.clone();
    current_comment["id"] = json!(current_id);
    current_comment["commitSha"] = current_head;
    current_comment["lineHash"] = json!(hex::encode(Sha256::digest(b"content")));
    current_comment["contentHash"] = json!(hex::encode(Sha256::digest(b"newer\ncontent\n")));
    current_comment["anchorContext"] = json!("newer\ncontent\n");
    body(
        reader
            .post_json(&f.path("comments"), &current_comment)
            .await,
        StatusCode::CREATED,
    )
    .await;
    let current = body(
        reader
            .get(&f.path(&format!(
                "review-flow/changes?repoId={}&path=review.txt&commentId={current_id}",
                f.parent.repo_id
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(current["file"]["after"], "newer\ncontent\n");
    let mut wrong = comment.clone();
    wrong["id"] = json!(Uuid::new_v4());
    wrong["commitSha"] = json!("f".repeat(40));
    body(
        reader.post_json(&f.path("comments"), &wrong).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let feedback =
        json!({"id":Uuid::new_v4(),"scope":"workspace","content":"General feedback before PR"});
    body(
        reader.post_json(&f.path("comments"), &feedback).await,
        StatusCode::CREATED,
    )
    .await;
    assert!(f.overview(&app, &f.reviewer).await["decisions"]
        .as_array()
        .unwrap()
        .is_empty());
    for mode in ["uncommitted", "unpushed"] {
        body(
            reader
                .get(&f.path(&format!("review-flow/changes?mode={mode}")))
                .await,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    let history = body(
        reader.get(&f.path("review-flow/history")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(history["rows"][0]["heads"].as_array().unwrap().len(), 2);
    app.cleanup().await;
}

#[tokio::test]
async fn review_file_refuses_an_oversized_blob_before_loading_the_comparison() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let oversized = vec![b'x'; 32 * 1024 * 1024 + 1];
    f.publish_file(&app, f.docs, "oversized.txt", &oversized)
        .await;
    let response = app
        .authenticated_client(&f.reviewer)
        .get(&f.path(&format!(
            "review-flow/changes?mode=all&repoId={}&path=oversized.txt",
            f.docs
        )))
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let error: Value = response.json().await.unwrap();
    assert_eq!(error["error"]["code"], "FILE_TOO_LARGE");
    app.cleanup().await;
}

/// A journal outage must not roll back a successfully accepted Git publication.
#[tokio::test]
async fn revision_observation_failure_does_not_block_publication() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let before = f.overview(&app, &f.owner).await;
    sqlx::query(&format!("ALTER TABLE workspace_revision_events ADD CONSTRAINT simulated_observation_failure CHECK (workspace_id <> '{}') NOT VALID", f.source))
        .execute(&app.db).await.unwrap();
    let after = f
        .publish_file(
            &app,
            f.docs,
            "plan.md",
            b"Accepted despite observation failure\n",
        )
        .await;
    assert_ne!(after["heads"], before["heads"]);
    let history = body(
        app.authenticated_client(&f.owner)
            .get(&f.path("review-flow/history"))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_ne!(history["rows"][0]["heads"], after["heads"]);
    let diff = body(
        app.authenticated_client(&f.reviewer)
            .get(&f.path(&format!(
                "review-flow/changes?mode=base&repoId={}&path=plan.md",
                f.docs
            )))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        diff["file"]["after"],
        "Accepted despite observation failure\n"
    );
    app.cleanup().await;
}
