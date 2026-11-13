//! Votes on referenced questions over the real API and Postgres
//! (`g5.documents.question.votes`).
//!
//! Anyone who can read the workspace votes, changes and withdraws a vote with
//! no running VM; the tally lists every voter for every reader; a vote is
//! validated against the set the voter resolves and never touches the set
//! file or the owner's answer; a non-member is refused; a fork starts empty.

use crate::helpers;

use std::time::Duration;

use g5_contracts::types::question_set::question_set_path;
use helpers::question_sets::{data, json_of, plan_set, Fixture};
use helpers::{TestOrg, TestUser, TestWorkspaceRuntimePeer};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn vote_body(path: &str, options: &[&str]) -> Value {
    json!({
        "path": path,
        "questionId": "pick-0123abcd",
        "expectedRevision": 1,
        "selectedOptionIds": options,
        "text": "",
    })
}

async fn member_with(fx: &Fixture, scopes: &[g5_auth::Scope]) -> TestUser {
    TestUser::create_with_scopes(&fx.app, fx.org.id, scopes).await
}

/// An organization member holding the read scope only: the vote routes need
/// nothing more.
async fn member(fx: &Fixture) -> TestUser {
    member_with(fx, &[g5_auth::Scope::WorkspacesRead]).await
}

#[tokio::test]
async fn readers_and_the_owner_vote_without_compute_and_everyone_sees_the_tally() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    fx.commit_docs(&path, &plan_set("plan")).await;
    let reader = member(&fx).await;
    let owner_client = fx.app.authenticated_client(&fx.owner);
    let reader_client = fx.app.authenticated_client(&reader);
    let mut ws = fx
        .app
        .ws_connect(&reader, &format!("workspace:{}", fx.workspace.id))
        .await;

    // The owner votes on the committed set without a VM.
    let (status, body) = json_of(
        owner_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["set-file"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["vote"]["userId"], fx.owner.id.to_string());
    assert_eq!(
        data(&body)["vote"]["selectedOptionIds"],
        json!(["set-file"])
    );
    assert_eq!(data(&body)["vote"]["questionRevision"], 1);
    let event = ws
        .expect_event("question.vote", Duration::from_secs(5))
        .await;
    assert_eq!(event["path"], path);
    assert_eq!(event["questionId"], "pick-0123abcd");

    // A reader votes the other way, with text.
    let mut reader_vote = vote_body(&path, &["database"]);
    reader_vote["text"] = json!("  keeps the checkout small  ");
    let (status, body) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &reader_vote)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["vote"]["text"], "keeps the checkout small");
    assert_eq!(data(&body)["vote"]["displayName"], "Test User");

    // Every reader sees the whole tally, with the viewer marked.
    let (status, body) = json_of(
        reader_client
            .get(&format!("{}?path={path}", fx.questions_url("/votes")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let view = data(&body);
    assert_eq!(view["viewerId"], reader.id.to_string());
    let votes = view["votes"].as_array().unwrap();
    assert_eq!(votes.len(), 2, "{view}");
    let by: Vec<&str> = votes
        .iter()
        .map(|v| v["userId"].as_str().unwrap())
        .collect();
    assert!(by.contains(&fx.owner.id.to_string().as_str()));
    assert!(by.contains(&reader.id.to_string().as_str()));

    // A changed vote replaces the row; a withdrawal removes it; withdrawing
    // again is a no-op.
    let (status, body) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["set-file"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = json_of(
        owner_client
            .get(&format!("{}?path={path}", fx.questions_url("/votes")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let votes = data(&body)["votes"].as_array().unwrap();
    assert_eq!(votes.len(), 2);
    assert!(votes
        .iter()
        .all(|v| v["selectedOptionIds"] == json!(["set-file"])));
    let withdraw = json!({"path": path, "questionId": "pick-0123abcd"});
    let (status, body) = json_of(
        reader_client
            .post_json(&fx.questions_url("/unvote"), &withdraw)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["withdrawn"], true);
    let (status, body) = json_of(
        reader_client
            .post_json(&fx.questions_url("/unvote"), &withdraw)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["withdrawn"], false);
    let (status, body) = json_of(
        reader_client
            .get(&format!("{}?path={path}", fx.questions_url("/votes")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["votes"].as_array().unwrap().len(), 1);

    // Nothing started compute, and the set file and its answer are untouched.
    let vms: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_vms WHERE workspace_id = $1")
        .bind(fx.workspace.id)
        .fetch_one(&fx.app.db)
        .await
        .unwrap();
    assert_eq!(vms, 0, "voting must never start compute");
    let (status, body) = json_of(
        reader_client
            .get(&format!("{}?path={path}", fx.questions_url("/set")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["questions"][0]["status"], "open");
    assert_eq!(data(&body)["questions"][0]["decision"], false);

    fx.app.cleanup().await;
}

#[tokio::test]
async fn a_vote_is_validated_against_the_set_the_voter_resolves() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    let committed = plan_set("plan");
    fx.commit_docs(&path, &committed).await;
    let reader = member(&fx).await;
    let owner_client = fx.app.authenticated_client(&fx.owner);
    let reader_client = fx.app.authenticated_client(&reader);

    for (body, code) in [
        (vote_body(&path, &["cloud"]), "unknown-option"),
        (vote_body(&path, &[]), "empty-vote"),
        (vote_body(&path, &["set-file", "database"]), "single-select"),
        (
            json!({"path": path, "questionId": "nope", "selectedOptionIds": ["set-file"]}),
            "question-missing",
        ),
        (
            json!({"path": "docs/../x.g5q.json", "questionId": "pick-0123abcd", "selectedOptionIds": ["set-file"]}),
            "invalid-question-set-path",
        ),
        (
            json!({"path": path, "questionId": "pick-0123abcd", "selectedOptionIds": ["set-file"], "userId": Uuid::new_v4()}),
            "voter-not-accepted",
        ),
    ] {
        let (status, response) = json_of(
            reader_client
                .post_json(&fx.questions_url("/vote"), &body)
                .await,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{code}: {response}"
        );
        assert_eq!(response["error"]["details"]["error"], code, "{response}");
    }

    // The revision the voter saw must be current; the refusal carries the
    // current entry like an answer's.
    let mut stale = vote_body(&path, &["set-file"]);
    stale["expectedRevision"] = json!(7);
    let (status, response) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &stale)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert_eq!(response["error"]["code"], "QUESTION_CONFLICT");
    assert_eq!(response["error"]["details"]["code"], "revision-moved");
    assert_eq!(response["error"]["details"]["currentRevision"], 1);
    assert_eq!(
        response["error"]["details"]["current"]["id"],
        "pick-0123abcd"
    );
    // Without a revision the vote lands against the current one.
    let mut unpinned = vote_body(&path, &["set-file"]);
    unpinned.as_object_mut().unwrap().remove("expectedRevision");
    let (status, response) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &unpinned)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(data(&response)["vote"]["questionRevision"], 1);

    // An unknown set is not found; a vote on it is refused the same way.
    let missing = question_set_path(&fx.workspace.id.to_string(), "missing");
    let (status, response) = json_of(
        reader_client
            .post_json(
                &fx.questions_url("/vote"),
                &vote_body(&missing, &["set-file"]),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");

    // The attached owner resolves the live file: an option the live file
    // adds is votable for the owner and unknown to a reader of the committed
    // cut; no reader request reaches the VM.
    let runtime = TestWorkspaceRuntimePeer::attach(&fx.app, &fx.workspace, fx.owner.id).await;
    let live = committed.replace(
        "\"label\": \"Database\"",
        "\"label\": \"Database\"\n        },\n        {\n          \"description\": \"\",\n          \"id\": \"cloud\",\n          \"label\": \"Cloud\"",
    );
    assert_ne!(live, committed);
    runtime.seed_file(fx.docs_repo_id, &path, &live).await;
    let before = runtime.received_rpc_methods().await.len();
    let (status, response) = json_of(
        owner_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["cloud"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let after_owner = runtime.received_rpc_methods().await.len();
    assert!(after_owner > before, "the owner's vote read the live set");
    let (status, response) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["cloud"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(response["error"]["details"]["error"], "unknown-option");
    assert_eq!(runtime.received_rpc_methods().await.len(), after_owner);

    // A set that no longer parses takes no votes, but keeps the ones it has.
    runtime
        .seed_file(fx.docs_repo_id, &path, "{ not a set")
        .await;
    let (status, response) = json_of(
        owner_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["set-file"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    assert_eq!(
        response["error"]["details"]["error"],
        "question-set-invalid"
    );
    let (status, response) = json_of(
        owner_client
            .get(&format!("{}?path={path}", fx.questions_url("/votes")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    // The reader's unpinned vote and the owner's live-only vote both stand.
    assert_eq!(data(&response)["votes"].as_array().unwrap().len(), 2);

    fx.app.cleanup().await;
}

#[tokio::test]
async fn non_members_cannot_vote_and_a_fork_starts_without_votes() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    fx.commit_docs(&path, &plan_set("plan")).await;
    let owner_client = fx.app.authenticated_client(&fx.owner);
    let (status, body) = json_of(
        owner_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["set-file"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A person from another organization neither votes, withdraws nor reads
    // the tally; neither does an organization member of a deactivated organization membership while deactivated. The owner's row is untouched throughout.
    let mut stranger = TestUser::create(&fx.app).await;
    let _other_org = TestOrg::create_with_admin(&fx.app, &mut stranger).await;
    let uninvited = member(&fx).await;
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(fx.org.id)
        .bind(uninvited.id)
        .execute(&fx.app.db)
        .await
        .unwrap();
    let withdraw = json!({"path": path, "questionId": "pick-0123abcd"});
    for (who, user) in [("stranger", &stranger), ("uninvited member", &uninvited)] {
        let client = fx.app.authenticated_client(user);
        for (leaf, body) in [
            ("/vote", Some(vote_body(&path, &["database"]))),
            ("/unvote", Some(withdraw.clone())),
            ("/votes", None),
        ] {
            let response = match body {
                Some(body) => client.post_json(&fx.questions_url(leaf), &body).await,
                None => {
                    client
                        .get(&format!("{}?path={path}", fx.questions_url(leaf)))
                        .await
                }
            };
            let (status, body) = json_of(response).await;
            assert!(
                matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
                "{who} {leaf}: {status} {body}"
            );
        }
    }
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM question_votes WHERE workspace_id = $1 AND set_path = $2",
    )
    .bind(fx.workspace.id)
    .bind(&path)
    .fetch_one(&fx.app.db)
    .await
    .unwrap();
    assert_eq!(rows, 1);

    // Reactivated, the ordinary reader votes and withdraws without an active membership.
    reactivate_reader(&fx, uninvited.id).await;
    let invited_client = fx.app.authenticated_client(&uninvited);
    let (status, body) = json_of(
        invited_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["database"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = json_of(
        invited_client
            .post_json(&fx.questions_url("/unvote"), &withdraw)
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["withdrawn"], true);

    // A fork inherits the set file, not the votes.
    let (status, cloned) = json_of(
        owner_client
            .post_json(
                &format!("/api/v1/workspaces/{}/clone", fx.workspace.id),
                &json!({"name": "vote-fork", "idempotencyKey": Uuid::new_v4()}),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{cloned}");
    let child_id: Uuid = data(&cloned)["workspace"]["id"]
        .as_str()
        .or_else(|| data(&cloned)["id"].as_str())
        .unwrap()
        .parse()
        .unwrap();
    let (status, body) = json_of(
        owner_client
            .get(&format!(
                "/api/v1/workspaces/{child_id}/questions/votes?path={path}"
            ))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["votes"], json!([]));
    // The child's reader votes on the inherited set in the child only.
    let (status, body) = json_of(
        owner_client
            .post_json(
                &format!("/api/v1/workspaces/{child_id}/questions/vote"),
                &vote_body(&path, &["database"]),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = json_of(
        owner_client
            .get(&format!("{}?path={path}", fx.questions_url("/votes")))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let parent_votes = data(&body)["votes"].as_array().unwrap();
    assert_eq!(parent_votes.len(), 1);
    assert_eq!(parent_votes[0]["selectedOptionIds"], json!(["set-file"]));

    fx.app.cleanup().await;
}

// ─── The audience grant is held through votes (G5-Lean#660) ─────────────────
//
// `g5.security.current-authority` under `g5.workspace.audience`: a reader's
// vote and withdrawal rely on their active membership to the workspace. The
// active membership is held on the writing transaction, so a revocation already in
// flight makes the write wait and then refuses it with nothing written, while
// an admitted write commits.

async fn reactivate_reader(fx: &Fixture, reader_id: Uuid) {
    sqlx::query("UPDATE org_memberships SET deactivated_at=NULL WHERE org_id=$1 AND user_id=$2")
        .bind(fx.org.id)
        .bind(reader_id)
        .execute(&fx.app.db)
        .await
        .unwrap();
}

/// Runs `request` while the reader's active membership is being revoked: the revoke
/// is in flight when the request arrives, the request waits on the audience
/// hold, and the revoke commits first.
async fn race_revocation(
    fx: &Fixture,
    reader_id: Uuid,
    path: String,
    body: Value,
    jwt: String,
) -> (StatusCode, Value) {
    let mut revoke = fx.app.db.begin().await.expect("revoker transaction");
    let removed = sqlx::query(
        "UPDATE org_memberships SET deactivated_at=now() WHERE org_id = $1 AND user_id = $2",
    )
    .bind(fx.org.id)
    .bind(reader_id)
    .execute(&mut *revoke)
    .await
    .expect("revoke in flight")
    .rows_affected();
    assert_eq!(removed, 1);
    let client =
        helpers::AuthenticatedClient::new(reqwest::Client::new(), fx.app.base_url.clone(), jwt);
    let pending = tokio::spawn(async move {
        let response = client.post_json(&path, &body).await;
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    });
    helpers::wait_for_lock_wait(&fx.app).await;
    assert!(
        !pending.is_finished(),
        "the write waits for the in-flight revocation"
    );
    revoke.commit().await.expect("the revocation commits first");
    pending.await.expect("the request completes")
}

async fn vote_rows(fx: &Fixture, reader_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM question_votes WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(fx.workspace.id)
    .bind(reader_id)
    .fetch_one(&fx.app.db)
    .await
    .expect("count votes")
}

#[tokio::test]
async fn a_readers_vote_and_withdrawal_hold_their_active_membership_until_commit() {
    let fx = Fixture::new().await;
    let path = fx.set_path();
    fx.commit_docs(&path, &plan_set("plan")).await;
    let reader = member(&fx).await;
    reactivate_reader(&fx, reader.id).await;
    let reader_client = fx.app.authenticated_client(&reader);

    // A vote whose active membership is being revoked waits, then is refused with
    // no row written.
    let (status, body) = race_revocation(
        &fx,
        reader.id,
        fx.questions_url("/vote"),
        vote_body(&path, &["database"]),
        reader.jwt.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(vote_rows(&fx, reader.id).await, 0);

    // Reactivated, the vote commits.
    reactivate_reader(&fx, reader.id).await;
    let (status, body) = json_of(
        reader_client
            .post_json(&fx.questions_url("/vote"), &vote_body(&path, &["database"]))
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(vote_rows(&fx, reader.id).await, 1);

    // A withdrawal likewise waits and is refused: the vote stays.
    let (status, body) = race_revocation(
        &fx,
        reader.id,
        fx.questions_url("/unvote"),
        json!({ "path": path, "questionId": "pick-0123abcd" }),
        reader.jwt.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(vote_rows(&fx, reader.id).await, 1);

    // Reactivated, the withdrawal commits.
    reactivate_reader(&fx, reader.id).await;
    let (status, body) = json_of(
        reader_client
            .post_json(
                &fx.questions_url("/unvote"),
                &json!({ "path": path, "questionId": "pick-0123abcd" }),
            )
            .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data(&body)["withdrawn"], json!(true));
    assert_eq!(vote_rows(&fx, reader.id).await, 0);

    fx.app.cleanup().await;
}
