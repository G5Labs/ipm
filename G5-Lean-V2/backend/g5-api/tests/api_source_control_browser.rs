//! Real HTTP/Postgres/Git repository checklist and selection acceptance.
use crate::helpers;
use crate::helpers::{
    review_flow::{body, Flow},
    TestApp,
};
use reqwest::StatusCode;
use serde_json::{json, Value};
async fn menu(app: &TestApp, f: &Flow) -> Value {
    body(
        app.authenticated_client(&f.reviewer)
            .get(&f.path("source-control/menu"))
            .await,
        StatusCode::OK,
    )
    .await
}
fn selection(menu: &Value, include: impl Fn(&Value) -> bool) -> Value {
    json!({"repos":menu["repos"].as_array().unwrap().iter().filter(|r| include(r)).map(|r| json!({"repoId":r["repoId"],"base":r["base"],"pushed":r["pushed"],"commits":r["commits"].as_array().unwrap().iter().map(|c| &c["sha"]).collect::<Vec<_>>(),"includeUnpushed":false,"liveFingerprint":null})).collect::<Vec<_>>()})
}
#[tokio::test]
async fn merge_base_commit_is_opt_in_without_changing_the_default_net_diff() {
    let app = TestApp::spawn().await;
    for combined in [true, false] {
        let f = Flow::new(&app).await;
        if !combined {
            f.publish_file(&app, f.parent.repo_id, "history-c.txt", b"C\n")
                .await;
            f.publish_file(&app, f.parent.repo_id, "history-d.txt", b"D\n")
                .await;
            let shared = f
                .publish_file(&app, f.parent.repo_id, "merge-base-only.txt", b"M\n")
                .await;
            let head = shared["heads"]
                .as_array()
                .unwrap()
                .iter()
                .find(|h| h["repoId"] == f.parent.repo_id.to_string())
                .unwrap()["headCommit"]
                .as_str()
                .unwrap();
            let branch: String =
                sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id=$1")
                    .bind(f.parent.id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            helpers::seed_branch_ref(&app, f.parent.repo_id, &branch, head).await;
        }
        f.publish_file(&app, f.parent.repo_id, "outgoing.txt", b"Y\n")
            .await;
        let current = menu(&app, &f).await;
        let repo = current["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repoId"] == f.parent.repo_id.to_string())
            .unwrap();
        let comparison = &repo["comparison"];
        let merge_base = repo["base"].as_str().unwrap();
        assert_eq!(comparison["mergeBase"], merge_base);
        assert_eq!(repo["base"] == repo["workspaceBase"], combined);
        assert_eq!(
            comparison["sinceWorkspaceBaseCount"],
            if combined { 0 } else { 2 }
        );
        assert!(!repo["commits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["sha"] == merge_base));
        let path = app.repo_path(f.parent.repo_id).await;
        let (parent, own_path) = {
            let git = git2::Repository::open(path).unwrap();
            let commit = git
                .find_commit(git2::Oid::from_str(merge_base).unwrap())
                .unwrap();
            assert_eq!(
                comparison["mergeBaseCommit"],
                json!({
                    "sha": merge_base,
                    "summary": commit.summary().unwrap(),
                    "authorName": commit.author().name().unwrap(),
                    "committedAt": commit.time().seconds(),
                    "parents": commit.parent_ids().map(|id| id.to_string()).collect::<Vec<_>>()
                })
            );
            (
                commit.parent_id(0).unwrap().to_string(),
                if combined {
                    "review.txt"
                } else {
                    "merge-base-only.txt"
                },
            )
        };
        let reader = app.authenticated_client(&f.reviewer);
        let mut defaults = selection(&current, |r| r["repoId"] == f.parent.repo_id.to_string());
        defaults["repos"][0]["allPushed"] = json!(true);
        let net = body(
            reader
                .post_json(&f.path("source-control/apply"), &defaults)
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(net["fileCount"], 1);
        assert_eq!(net["patches"].as_array().unwrap().len(), 1);
        assert_eq!(net["patches"][0]["file"]["path"], "outgoing.txt");
        assert_eq!(net["patches"][0]["before"], merge_base);
        assert_eq!(net["patches"][0]["after"], repo["pushed"]);
        assert_eq!(net["patches"][0]["label"], "Pushed changes vs merge base");
        let mut explicit = defaults.clone();
        explicit["repos"][0]["commits"] = json!([merge_base]);
        explicit["repos"][0]["allPushed"] = json!(false);
        let own = body(
            reader
                .post_json(&f.path("source-control/apply"), &explicit)
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(own["fileCount"], 1);
        assert_eq!(own["patches"].as_array().unwrap().len(), 1);
        assert_eq!(own["patches"][0]["file"]["path"], own_path);
        assert_eq!(own["patches"][0]["before"], parent);
        assert_eq!(own["patches"][0]["after"], merge_base);
        assert_eq!(
            own["selectedCommits"][format!("{}:{merge_base}", f.parent.repo_id)]["sha"],
            merge_base
        );
        let restored = body(
            reader
                .post_json(&f.path("source-control/apply"), &defaults)
                .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(
            restored, net,
            "All changes retains the exact net representation and counts"
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn personal_source_control_has_no_outgoing_comparison_and_browses_history() {
    async fn refused(response: reqwest::Response) -> Value {
        assert_eq!(response.status(), StatusCode::CONFLICT);
        response.json().await.unwrap()
    }
    let app = TestApp::spawn().await;
    let mut owner = helpers::TestUser::create(&app).await;
    let org = helpers::TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let personal = body(
        client.get(&format!("/api/v1/orgs/{}/me", org.id)).await,
        StatusCode::OK,
    )
    .await;
    let id = personal["workspaceId"].as_str().unwrap();
    let path = format!("/api/v1/workspaces/{id}/source-control");
    let menu = body(client.get(&format!("{path}/menu")).await, StatusCode::OK).await;
    let repos = menu["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "reference checkouts are not members");
    let repo = &repos[0];
    assert_eq!(repo["unpushedOnly"], true);
    assert!(repo["base"].is_null());
    assert!(repo["error"].is_null(), "{repo}");
    assert_eq!(repo["commits"], json!([]));
    assert!(repo["live"].is_null());
    let unchanged = body(
        client
            .post_json(
                &format!("{path}/check"),
                &json!({"fingerprint":menu["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unchanged, json!({"changed":false}));
    let mut request = json!({"repos":[{"repoId":repo["repoId"],"base":null,"pushed":repo["pushed"],"commits":[],"includeUnpushed":true,"liveFingerprint":"unavailable"}]});
    let stopped = refused(client.post_json(&format!("{path}/apply"), &request).await).await;
    assert!(stopped.to_string().contains("running owner"), "{stopped}");
    request["repos"][0]["commits"] = json!([repo["pushed"]]);
    request["repos"][0]["includeUnpushed"] = json!(false);
    let committed = body(
        client.post_json(&format!("{path}/apply"), &request).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(committed["selectedCommits"].as_object().unwrap().len(), 1);
    request["repos"][0]["includeUnpushed"] = json!(true);
    request["repos"][0]["commits"] = json!([]);
    request["repos"][0]["allPushed"] = json!(true);
    let all = refused(client.post_json(&format!("{path}/apply"), &request).await).await;
    assert!(all.to_string().contains("No outgoing"), "{all}");
    request["repos"][0]["allPushed"] = json!(false);
    request["repos"][0]["base"] = repo["pushed"].clone();
    let fabricated = refused(client.post_json(&format!("{path}/apply"), &request).await).await;
    assert!(
        fabricated.to_string().contains("workspace base"),
        "{fabricated}"
    );
    let outsider = helpers::TestUser::create(&app).await;
    let denied = app
        .authenticated_client(&outsider)
        .get(&format!("{path}/menu"))
        .await;
    assert!([StatusCode::FORBIDDEN, StatusCode::NOT_FOUND].contains(&denied.status()));
    app.cleanup().await;
}

#[tokio::test]
async fn source_control_menu_is_repository_scoped_and_net_selection_excludes_reverted_files() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let initial = menu(&app, &f).await;
    let first = f
        .publish_file(&app, f.parent.repo_id, "same.txt", b"one\n")
        .await;
    f.publish_file(&app, f.docs, "same.txt", b"docs\n").await;
    let skipped = f
        .publish_file(
            &app,
            f.parent.repo_id,
            "skipped-only.txt",
            b"excluded from subset\n",
        )
        .await;
    let third = f
        .publish_file(&app, f.parent.repo_id, "same.txt", b"two\n")
        .await;
    f.publish_file(&app, f.parent.repo_id, "reverted.txt", b"temporary\n")
        .await;
    f.delete_file(&app, f.parent.repo_id, "reverted.txt").await;
    let current = menu(&app, &f).await;
    assert_eq!(current["private"], false);
    assert_eq!(current["repos"].as_array().unwrap().len(), 2);
    for r in current["repos"].as_array().unwrap() {
        assert!(r["live"].is_null());
        assert!(r["error"].is_null(), "{r}");
    }
    let reader = app.authenticated_client(&f.reviewer);
    let net = body(
        reader
            .post_json(
                &f.path("source-control/apply"),
                &selection(&current, |_| true),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(net["fileCount"], 3);
    assert_eq!(net["patches"].as_array().unwrap().len(), 3);
    assert!(net["patches"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["file"]["path"] != "reverted.txt"));
    assert_eq!(net["private"], false);
    let unchanged = body(
        reader
            .post_json(
                &f.path("source-control/check"),
                &json!({"fingerprint":current["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unchanged, json!({"changed":false}));
    let changed = body(
        reader
            .post_json(
                &f.path("source-control/check"),
                &json!({"fingerprint":initial["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(changed["changed"], true);
    assert_eq!(changed["menu"]["fingerprint"], current["fingerprint"]);
    let overview = body(
        reader.get(&f.path("source-control/checkpoint")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        overview["generation"], 0,
        "ordinary publications do not advance the Save checkpoint"
    );
    // Arbitrary/nonconsecutive selection keeps each commit's patch separately.
    let mut subset = selection(&current, |r| r["repoId"] == f.parent.repo_id.to_string());
    let published_head = |overview: &Value| {
        overview["heads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["repoId"] == f.parent.repo_id.to_string())
            .unwrap()["headCommit"]
            .clone()
    };
    let selected = [published_head(&first), published_head(&third)];
    subset["repos"][0]["commits"] = json!(selected);
    let patch = body(
        reader
            .post_json(&f.path("source-control/apply"), &subset)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(patch["fileCount"], 1);
    let patches = patch["patches"].as_array().unwrap();
    assert_eq!(
        patches.len(),
        2,
        "selected commits must keep separate patches"
    );
    for (p, sha) in patches.iter().zip(&selected) {
        assert_eq!(&p["after"], sha);
        assert_eq!(p["file"]["path"], "same.txt");
        // Publication receipts and the initial fixture tip are independent
        // of the menu's parent metadata and the apply implementation.
        let before = if sha == &selected[0] {
            json!(f.parent.head_commit)
        } else {
            published_head(&skipped)
        };
        assert_eq!(p["before"], before);
        assert!(p["label"]
            .as_str()
            .unwrap()
            .starts_with(&sha.as_str().unwrap()[..7]));
    }
    assert!(patches[0]["file"]["diff"]["hunks"]
        .to_string()
        .contains("one"));
    assert!(patches[1]["file"]["diff"]["hunks"]
        .to_string()
        .contains("two"));
    app.cleanup().await;
}
#[tokio::test]
async fn source_control_refuses_private_selections_nonmembers_missing_bases_and_stopped_saves() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "file", b"saved\n")
        .await;
    let current = menu(&app, &f).await;
    let reader = app.authenticated_client(&f.reviewer);
    let mut private = selection(&current, |r| r["repoId"] == f.parent.repo_id.to_string());
    private["repos"][0]["includeUnpushed"] = json!(true);
    body(
        reader
            .post_json(&f.path("source-control/apply"), &private)
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    let mut foreign = selection(&current, |_| true);
    foreign["repos"][0]["repoId"] = json!(uuid::Uuid::new_v4());
    body(
        reader
            .post_json(&f.path("source-control/apply"), &foreign)
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    let mut unavailable = selection(&current, |_| true);
    unavailable["repos"][0]["commits"] = json!(["0".repeat(40)]);
    let recovered = body(
        reader
            .post_json(&f.path("source-control/apply"), &unavailable)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(recovered["refreshed"], true);
    assert!(
        recovered.get("patches").is_none(),
        "An unavailable revision cannot produce comparison content"
    );
    let repo_id = unavailable["repos"][0]["repoId"].as_str().unwrap();
    assert_eq!(
        recovered["missingCommits"][repo_id],
        json!(["0".repeat(40)])
    );
    let response = app
        .authenticated_client(&f.owner)
        .post_json(&f.path("source-control/save"), &json!({}))
        .await;
    assert!(!response.status().is_success());
    assert_eq!(
        body(
            app.authenticated_client(&f.owner)
                .get(&f.path("source-control/checkpoint"))
                .await,
            StatusCode::OK
        )
        .await["generation"],
        0
    );
    // A standard workspace with no readable parent has unavailable comparison
    // rows. An unchanged checklist remains a valid Save/check observation.
    let missing_path = format!("/api/v1/workspaces/{}/source-control", f.parent.id);
    let missing = body(
        reader.get(&format!("{missing_path}/menu")).await,
        StatusCode::OK,
    )
    .await;
    let repos = missing["repos"].as_array().unwrap();
    assert!(
        !repos.is_empty(),
        "missing baselines must remain visible per repo"
    );
    assert!(repos
        .iter()
        .all(|repo| { repo["base"].is_null() && repo["error"].is_string() }));
    body(
        reader
            .post_json(
                &format!("{missing_path}/check"),
                &json!({"fingerprint":missing["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    app.cleanup().await;
}

/// The live transport is scripted here; commit/push mechanics are covered by
/// native Git tests and the production browser/CLI journey separately.
#[tokio::test]
async fn source_control_button_and_agent_share_only_successful_save_checkpoints() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let detail = body(
        app.authenticated_client(&f.owner)
            .get(&format!("/api/v1/workspaces/{}", f.source))
            .await,
        StatusCode::OK,
    )
    .await;
    let source = helpers::TestWorkspace {
        id: f.source,
        org_id: f.org.id,
        repo_id: f.parent.repo_id,
        head_commit: f.parent.head_commit.clone(),
        name: detail["name"].as_str().unwrap().into(),
    };
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(f.source)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_changes_report(
        &app,
        &source,
        f.owner.id,
        json!([]),
    )
    .await;
    let ui = body(
        app.authenticated_client(&f.owner)
            .post_json(&f.path("source-control/save"), &json!({}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ui["generation"], 1);
    let token = app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.source, f.owner.id, peer.vm_id(), 1, 3600)
        .unwrap();
    let internal = app
        .client
        .post(format!(
            "{}/api/internal/workspaces/{}/source-control/save",
            app.base_url, f.source
        ))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(body(internal, StatusCode::OK).await["generation"], 2);
    let mut overlapping = Vec::new();
    for index in 0..4 {
        let client = app.client.clone();
        let (url, token) = if index % 2 == 0 {
            (
                format!("{}{}", app.base_url, f.path("source-control/save")),
                f.owner.jwt.clone(),
            )
        } else {
            (
                format!(
                    "{}/api/internal/workspaces/{}/source-control/save",
                    app.base_url, f.source
                ),
                token.clone(),
            )
        };
        overlapping.push(tokio::spawn(async move {
            let response = client
                .post(url)
                .bearer_auth(token)
                .json(&json!({}))
                .send()
                .await
                .unwrap();
            body(response, StatusCode::OK).await
        }));
    }
    let mut generations = Vec::new();
    for request in overlapping {
        generations.push(request.await.unwrap()["generation"].as_i64().unwrap());
    }
    generations.sort();
    assert_eq!(generations, vec![3, 4, 5, 6]);
    let checkpoint = body(
        app.authenticated_client(&f.reviewer)
            .get(&f.path("source-control/checkpoint"))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(checkpoint["generation"], 6);
    let invoked = peer.invoke_params().await;
    assert_eq!(
        invoked
            .iter()
            .filter(|p| p["operation"] == "commit")
            .count(),
        6
    );
    assert_eq!(
        invoked.iter().filter(|p| p["operation"] == "push").count(),
        6
    );
    app.cleanup().await;
}

/// Force both writers to queue behind an existing reader. Taking permission
/// SHARE locks before UPDATE used to deadlock as soon as that reader released.
#[tokio::test]
async fn source_control_checkpoint_writers_lock_before_permission_admission() {
    use g5_api::{db::scoped::scoped_db_for_workspace_update, middleware::AuthContext};
    use std::time::Duration;
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let mut blocker = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR SHARE")
        .bind(f.source)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let auth = AuthContext::User(g5_auth::JwtClaims {
        sub: f.owner.id.to_string(),
        org: f.org.id.to_string(),
        scopes: vec![g5_auth::Scope::WorkspacesWrite],
        iat: 0,
        exp: 4_102_444_800,
        iss: None,
        aud: None,
        jti: None,
        token_use: None,
        client: None,
    });
    let mut writers = Vec::new();
    for _ in 0..2 {
        let state = app.state.clone();
        let auth = auth.clone();
        let workspace = f.source;
        writers.push(tokio::spawn(async move {
            let db =
                scoped_db_for_workspace_update(&state, &auth, workspace, &["workspaces:write"])
                    .await
                    .expect("competing checkpoint admission must succeed without retry");
            db.commit().await.unwrap();
        }));
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let queued: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database()
                 AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM workspaces WHERE id=$1 AND org_id=$2 FOR UPDATE%'")
                .fetch_one(&app.db).await.unwrap();
            if queued == 2 { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("both writers must actually contend for the workspace row");
    blocker.commit().await.unwrap();
    for writer in writers {
        tokio::time::timeout(Duration::from_secs(5), writer)
            .await
            .unwrap()
            .unwrap();
    }
    app.cleanup().await;
}

#[tokio::test]
async fn source_control_landing_entry_recaptures_the_current_pushed_vector() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "counted", b"counted\n")
        .await;
    let counted = menu(&app, &f).await;
    let heads = counted["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["repoId"].as_str().unwrap().to_owned(),
                r["pushed"].clone(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    f.publish_file(&app, f.docs, "later", b"newly captured scope\n")
        .await;
    let snapshot = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/pushed"), &json!({"heads":heads}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(snapshot["fileCount"], 2);
    assert!(snapshot["patches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["file"]["path"] == "later"));
    app.cleanup().await;
}

#[tokio::test]
async fn source_control_and_activity_share_supported_path_categories() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let cases = [
        (f.parent.repo_id, "README.md", "code"),
        (f.parent.repo_id, ".g5/ontology/current.g5.md", "ontology"),
        (f.parent.repo_id, ".g5/ontology/notes.md", "code"),
        (f.parent.repo_id, ".semlift/specs/legacy.md", "code"),
        (f.docs, "assets/logo.bin", "docs"),
        (f.docs, ".g5/ontology/current.g5.md", "ontology"),
        (f.docs, ".g5/pr_ontology_diff_design/plan.md", "docs"),
    ];
    for (repo, path, _) in cases {
        f.publish_file(
            &app,
            repo,
            path,
            b"category comes from path, not these bytes\n",
        )
        .await;
    }
    f.publish_file(&app, f.parent.repo_id, "reverted.tmp", b"temporary\n")
        .await;
    f.delete_file(&app, f.parent.repo_id, "reverted.tmp").await;
    let current = menu(&app, &f).await;
    let reader = app.authenticated_client(&f.reviewer);
    let snapshot = body(
        reader
            .post_json(
                &f.path("source-control/apply"),
                &selection(&current, |_| true),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(snapshot["fileCount"], cases.len());
    let activity = body(
        reader.get(&f.path("review-flow/changes?mode=base")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(activity["files"].as_array().unwrap().len(), cases.len());
    let expected = cases
        .into_iter()
        .map(|(repo, path, kind)| ((repo.to_string(), path.to_owned()), kind.to_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let applied = snapshot["patches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                (
                    p["repoId"].as_str().unwrap().to_owned(),
                    p["fileKey"].as_str().unwrap().to_owned(),
                ),
                p["kind"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let summary = activity["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                (
                    p["repoId"].as_str().unwrap().to_owned(),
                    p["path"].as_str().unwrap().to_owned(),
                ),
                p["kind"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(applied, expected);
    assert_eq!(summary, expected);
    app.cleanup().await;
}

#[tokio::test]
async fn activity_net_counts_do_not_require_unused_intermediate_commit_trees() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    let first = f
        .publish_file(&app, f.parent.repo_id, "net.txt", b"first\n")
        .await;
    let middle = first["heads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["repoId"] == f.parent.repo_id.to_string())
        .unwrap()["headCommit"]
        .as_str()
        .unwrap();
    f.publish_file(&app, f.parent.repo_id, "net.txt", b"final\n")
        .await;
    let client = app.authenticated_client(&f.reviewer);
    let before = body(
        client.get(&f.path("review-flow/changes?mode=base")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(before["files"].as_array().unwrap().len(), 1);
    let path = app.repo_path(f.parent.repo_id).await;
    let object_path = {
        let repo = git2::Repository::open(&path).unwrap();
        let commit = repo
            .find_commit(git2::Oid::from_str(middle).unwrap())
            .unwrap();
        let object = commit.tree_id().to_string();
        repo.path()
            .join("objects")
            .join(&object[..2])
            .join(&object[2..])
    };
    std::fs::remove_file(object_path).unwrap();
    let after = body(
        client.get(&f.path("review-flow/changes?mode=base")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(after["files"], before["files"]);
    assert_eq!(after["baselineAvailable"], true);
    assert!(after["unavailableRepositories"]
        .as_array()
        .unwrap()
        .is_empty());
    let checklist = menu(&app, &f).await;
    let repo = checklist["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    assert!(
        repo["error"].is_string(),
        "commit rows still validate their own trees: {repo}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn pushed_checklists_and_checks_do_not_inspect_live_files() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "pushed.txt", b"pushed\n")
        .await;
    let client = app.authenticated_client(&f.owner);
    let detail = body(
        client
            .get(&format!("/api/v1/workspaces/{}", f.source))
            .await,
        StatusCode::OK,
    )
    .await;
    let source = helpers::TestWorkspace {
        id: f.source,
        org_id: f.org.id,
        repo_id: f.parent.repo_id,
        head_commit: f.parent.head_commit.clone(),
        name: detail["name"].as_str().unwrap().into(),
    };
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(f.source)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_invoke_failure(
        &app,
        &source,
        f.owner.id,
        "Live reads must not occur",
    )
    .await;
    let listed = body(
        client.get(&f.path("source-control/menu")).await,
        StatusCode::OK,
    )
    .await;
    for repo in listed["repos"].as_array().unwrap() {
        assert_eq!(repo["live"], json!({"selectable":true}));
    }
    let unchanged = body(
        client
            .post_json(
                &f.path("source-control/check"),
                &json!({"fingerprint":listed["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unchanged, json!({"changed":false}));
    let shown = body(
        client
            .post_json(
                &f.path("source-control/apply"),
                &selection(&listed, |_| true),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(!shown["patches"].as_array().unwrap().is_empty());
    assert!(peer.invoke_params().await.is_empty());
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn source_control_keeps_available_repositories_when_a_member_head_is_missing() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "available.txt", b"available\n")
        .await;
    let before = menu(&app, &f).await;
    let branch: String = sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id=$1")
        .bind(f.source)
        .fetch_one(&app.db)
        .await
        .unwrap();
    helpers::seed_branch_ref(&app, f.docs, &branch, &"0".repeat(40)).await;
    let partial = menu(&app, &f).await;
    let repos = partial["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);
    assert!(repos
        .iter()
        .find(|r| r["repoId"] == f.docs.to_string())
        .unwrap()["error"]
        .is_string());
    assert!(repos
        .iter()
        .find(|r| r["repoId"] == f.parent.repo_id.to_string())
        .unwrap()["error"]
        .is_null());
    let client = app.authenticated_client(&f.reviewer);
    let changes = body(
        client.get(&f.path("review-flow/changes?mode=base")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(changes["baselineAvailable"], true);
    assert_eq!(changes["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        changes["unavailableRepositories"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        changes["unavailableRepositories"][0]["repoId"],
        f.docs.to_string()
    );
    let pushed = body(
        client
            .post_json(&f.path("source-control/pushed"), &json!({}))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pushed["fileCount"], 1);
    let checked = body(
        client
            .post_json(
                &f.path("source-control/check"),
                &json!({"fingerprint":before["fingerprint"]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(checked["changed"], true);
    assert_eq!(checked["menu"]["repos"], partial["repos"]);
    let failed_refresh = client
        .post_json(
            &f.path("source-control/check"),
            &json!({"fingerprint":"", "selected": {f.docs.to_string(): ["0".repeat(40)]}}),
        )
        .await;
    assert_eq!(failed_refresh.status(), StatusCode::CONFLICT);
    let error: Value = failed_refresh.json().await.unwrap();
    assert!(error["error"]["details"].get("kind").is_none(), "{error}");
    assert!(
        error.get("data").is_none(),
        "An unreadable repository cannot replace the list or prove selected commits missing"
    );
    helpers::seed_branch_ref(&app, f.parent.repo_id, &branch, &"0".repeat(40)).await;
    let unavailable = body(
        client.get(&f.path("review-flow/changes?mode=base")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unavailable["baselineAvailable"], false);
    assert_eq!(
        unavailable["unavailableRepositories"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    body(
        client
            .post_json(&f.path("source-control/pushed"), &json!({}))
            .await,
        StatusCode::CONFLICT,
    )
    .await;
    app.cleanup().await;
}

#[tokio::test]
async fn base_source_control_has_only_lazy_history_and_no_outgoing_comparison() {
    let app = TestApp::spawn().await;
    let mut owner = helpers::TestUser::create(&app).await;
    let org = helpers::TestOrg::create_with_admin(&app, &mut owner).await;
    let base = helpers::TestWorkspace::create_base(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let root = format!("/api/v1/workspaces/{}", base.id);
    let menu = body(
        client.get(&format!("{root}/source-control/menu")).await,
        StatusCode::OK,
    )
    .await;
    let repo = &menu["repos"][0];
    assert_eq!(repo["unpushedOnly"], true);
    assert!(repo["base"].is_null());
    assert!(repo["workspaceBase"].is_null());
    assert_eq!(repo["commits"], json!([]));
    let page = body(
        client
            .post_json(
                &format!("{root}/source-control/history"),
                &json!({"repoId":repo["repoId"],"pushed":repo["pushed"],"section":"history"}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(page["commits"].as_array().unwrap().len(), 1);
    assert!(
        page["commits"][0].get("paths").is_none(),
        "history lists metadata only"
    );
    let applied = body(
        client
            .post_json(
                &format!("{root}/source-control/apply"),
                &json!({"repos":[{
                    "repoId":repo["repoId"], "base":null, "pushed":repo["pushed"],
                    "commits":[page["commits"][0]["sha"]], "includeUnpushed":false,
                    "liveFingerprint":null
                }]}),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(applied["selectedCommits"]
        .as_object()
        .unwrap()
        .values()
        .any(|commit| commit["paths"]
            .as_array()
            .is_some_and(|paths| paths.contains(&json!("README.md")))));
    let outsider = helpers::TestUser::create(&app).await;
    let denied = app
        .authenticated_client(&outsider)
        .post_json(
            &format!("{root}/source-control/history"),
            &json!({"repoId":repo["repoId"],"pushed":repo["pushed"],"section":"history"}),
        )
        .await;
    assert!(
        [StatusCode::FORBIDDEN, StatusCode::NOT_FOUND].contains(&denied.status()),
        "a warm ordering cache never grants access"
    );
    let changes = body(
        client
            .get(&format!("{root}/review-flow/changes?mode=base"))
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(changes["baselineAvailable"], false);
    app.cleanup().await;
}

#[tokio::test]
async fn merged_comparisons_use_final_preflight_cuts_until_the_next_source_push() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "branch.txt", b"branch\n")
        .await;
    let captured = menu(&app, &f).await;
    let source = f.overview(&app, &f.owner).await["heads"].clone();
    let target = body(
        app.authenticated_client(&f.owner)
            .get(&format!("/api/v1/workspaces/{}/review-flow", f.parent.id))
            .await,
        StatusCode::OK,
    )
    .await["heads"]
        .clone();
    let mut landed = source.clone();
    for h in landed.as_array_mut().unwrap() {
        h["workspaceId"] = json!(f.parent.id);
        h["branchName"] = target
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["repoId"] == h["repoId"])
            .unwrap()["branchName"]
            .clone();
    }
    let preflight = json!({"sourceKind":"workspace","sourceWorkspaceId":f.source,"targetWorkspaceId":f.parent.id,"decision":"cleanMergeNoGitConflicts","canDirectMerge":true,"preflightFingerprint":"final-cut","repoResults":source.as_array().unwrap().iter().map(|s| { let t=target.as_array().unwrap().iter().find(|t|t["repoId"]==s["repoId"]).unwrap(); json!({"repoId":s["repoId"],"sourceWorkspaceId":f.source,"targetWorkspaceId":f.parent.id,"sourceBranchName":s["branchName"],"targetBranchName":t["branchName"],"sourceHead":s["headCommit"],"targetHead":t["headCommit"],"outcome":"cleanMerge","conflictPaths":[],"message":null}) }).collect::<Vec<_>>(),"message":"Final pre-merge cut"});
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_pull_requests(id,org_id,source_workspace_id,target_workspace_id,status,local_merge_status,provider_status,title,source_heads,target_heads,preflight,last_preflight_fingerprint,created_by) VALUES($1,$2,$3,$4,'merged','merged','local_only','Captured merge',$5,$6,$7,'final-cut',$8)")
        .bind(id).bind(f.org.id).bind(f.source).bind(f.parent.id).bind(&source).bind(&landed).bind(preflight).bind(f.owner.id).execute(&app.db).await.unwrap();
    // The landed target already contains the source. Using target_heads would
    // erase the branch delta; the final preflight retains its actual parent.
    for h in landed.as_array().unwrap() {
        helpers::seed_branch_ref(
            &app,
            uuid::Uuid::parse_str(h["repoId"].as_str().unwrap()).unwrap(),
            h["branchName"].as_str().unwrap(),
            h["headCommit"].as_str().unwrap(),
        )
        .await;
    }
    let merged = menu(&app, &f).await;
    assert_eq!(merged["repos"], captured["repos"]);
    let client = app.authenticated_client(&f.owner);
    let historical_path = f.path(&format!("review-flow/changes?mode=base&pullRequestId={id}"));
    let frozen = body(client.get(&historical_path).await, StatusCode::OK).await;
    assert_eq!(frozen["files"].as_array().unwrap().len(), 1);
    f.publish_file(&app, f.parent.repo_id, "later.txt", b"new branch work\n")
        .await;
    let active = menu(&app, &f).await;
    let active_repo = active["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    assert_eq!(active_repo["commits"].as_array().unwrap().len(), 1);
    assert_eq!(
        active_repo["base"],
        landed
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["repoId"] == f.parent.repo_id.to_string())
            .unwrap()["headCommit"]
    );
    let still_frozen = body(client.get(&historical_path).await, StatusCode::OK).await;
    assert_eq!(still_frozen["files"], frozen["files"]);
    assert_eq!(still_frozen["beforeHeads"], frozen["beforeHeads"]);
    assert_eq!(still_frozen["afterHeads"], frozen["afterHeads"]);
    sqlx::query("UPDATE workspace_pull_requests SET status='closed',local_merge_status='closed' WHERE id=$1").bind(id).execute(&app.db).await.unwrap();
    let closed = body(client.get(&historical_path).await, StatusCode::OK).await;
    assert_eq!(closed["baselineAvailable"], false);
    app.cleanup().await;
}

#[tokio::test]
async fn refresh_validates_selected_history_without_writes_or_substitution() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "refresh.txt", b"unchanged\n")
        .await;
    let before = menu(&app, &f).await;
    let repo = before["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    let tip = repo["pushed"].as_str().unwrap();
    let base = repo["comparison"]["mergeBase"].as_str().unwrap();
    let path = app.repo_path(f.parent.repo_id).await;
    let orphan = {
        let git = git2::Repository::open(&path).unwrap();
        let tree = git
            .find_commit(git2::Oid::from_str(tip).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        let signature = git2::Signature::now("Refresh test", "refresh@g5.dev").unwrap();
        git.commit(
            None,
            &signature,
            &signature,
            "Unreachable selected commit",
            &tree,
            &[],
        )
        .unwrap()
        .to_string()
    };
    let refs = || {
        let git = git2::Repository::open(&path).unwrap();
        let mut refs = git
            .references()
            .unwrap()
            .map(|r| {
                let r = r.unwrap();
                (
                    r.name().unwrap().to_owned(),
                    r.target().map(|id| id.to_string()),
                )
            })
            .collect::<Vec<_>>();
        refs.sort();
        refs
    };
    let before_refs = refs();
    let reader = app.authenticated_client(&f.reviewer);
    let checkpoint = body(
        reader.get(&f.path("source-control/checkpoint")).await,
        StatusCode::OK,
    )
    .await;
    let missing = "0".repeat(40);
    let refreshed = body(reader.post_json(&f.path("source-control/check"), &json!({"fingerprint":"", "selected":{ f.parent.repo_id.to_string(): [base, tip, orphan, missing] }})).await, StatusCode::OK).await;
    assert_eq!(refreshed["changed"], true);
    assert_eq!(
        refreshed["missingCommits"][f.parent.repo_id.to_string()],
        json!([orphan, missing])
    );
    let validated = refreshed["menu"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    let ids = validated["historyCommits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["sha"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![base, tip]);
    assert_eq!(validated["commits"], repo["commits"]);
    assert_eq!(refs(), before_refs);
    assert_eq!(
        body(
            reader.get(&f.path("source-control/checkpoint")).await,
            StatusCode::OK
        )
        .await,
        checkpoint
    );
    assert_eq!(menu(&app, &f).await, before);
    let rejected = reader
        .post_json(
            &f.path("source-control/check"),
            &json!({"fingerprint":"", "selected": { uuid::Uuid::new_v4().to_string(): [tip] }}),
        )
        .await;
    assert_eq!(rejected.status(), StatusCode::CONFLICT);
    let error: Value = rejected.json().await.unwrap();
    assert!(error["error"]["details"].get("kind").is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn refresh_confirms_commit_availability_without_reading_live_files() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "refresh-live-read.txt", b"stable\n")
        .await;
    let before = menu(&app, &f).await;
    let client = app.authenticated_client(&f.owner);
    let detail = body(
        client
            .get(&format!("/api/v1/workspaces/{}", f.source))
            .await,
        StatusCode::OK,
    )
    .await;
    let workspace = helpers::TestWorkspace {
        id: f.source,
        org_id: f.org.id,
        repo_id: f.parent.repo_id,
        head_commit: f.parent.head_commit.clone(),
        name: detail["name"].as_str().unwrap().into(),
    };
    sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
        .bind(f.source)
        .execute(&app.db)
        .await
        .unwrap();
    let peer = helpers::TestWorkspaceRuntimePeer::attach_with_invoke_failure(
        &app,
        &workspace,
        f.owner.id,
        "Live read unavailable",
    )
    .await;
    let selected = before["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
        .unwrap()["pushed"]
        .clone();
    let response = client
        .post_json(
            &f.path("source-control/check"),
            &json!({"fingerprint":"", "selected": {f.parent.repo_id.to_string(): [selected]}}),
        )
        .await;
    let result = body(response, StatusCode::OK).await;
    assert_eq!(result["missingCommits"], json!({}));
    assert_eq!(
        result["menu"]["repos"][0]["live"],
        json!({"selectable":true})
    );
    assert!(peer.invoke_params().await.is_empty());
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn show_returns_one_refreshed_list_when_pushed_history_changed() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "first.txt", b"first\n")
        .await;
    let before = menu(&app, &f).await;
    let mut requested = selection(&before, |_| true);
    requested["fingerprint"] = before["fingerprint"].clone();
    f.publish_file(&app, f.docs, "next.txt", b"next\n").await;
    let client = app.authenticated_client(&f.owner);
    let refreshed = body(
        client
            .post_json(&f.path("source-control/apply"), &requested)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(refreshed["refreshed"], true);
    assert!(refreshed.get("patches").is_none());
    assert_ne!(refreshed["menu"]["fingerprint"], before["fingerprint"]);
    assert_eq!(refreshed["missingCommits"], json!({}));
    let shown = body(
        client
            .post_json(
                &f.path("source-control/apply"),
                &selection(&refreshed["menu"], |_| true),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(shown.get("refreshed").is_none());
    assert!(!shown["patches"].as_array().unwrap().is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn target_advance_with_the_same_merge_base_does_not_refresh_the_checklist() {
    let app = TestApp::spawn().await;
    let mut f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "source-only.txt", b"source\n")
        .await;
    let before = menu(&app, &f).await;
    let old_code = before["repos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
        .unwrap();
    let history = json!({"repoId":old_code["repoId"],"pushed":old_code["pushed"],"parent":old_code["parent"],"base":old_code["base"],"workspaceBase":old_code["workspaceBase"],"section":"earlier","cursor":null});
    let page_before = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/history"), &history)
            .await,
        StatusCode::OK,
    )
    .await;
    f.parent
        .commit_file(&app, "target-only.txt", "target\n")
        .await;
    let after = menu(&app, &f).await;
    let code = |menu: &Value| {
        menu["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
            .unwrap()
            .clone()
    };
    assert_ne!(code(&before)["parent"], code(&after)["parent"]);
    assert_eq!(code(&before)["base"], code(&after)["base"]);
    assert_eq!(before["fingerprint"], after["fingerprint"]);
    let mut request = selection(&before, |_| true);
    request["fingerprint"] = before["fingerprint"].clone();
    let shown = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/apply"), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert!(shown.get("refreshed").is_none());
    assert_eq!(shown["fileCount"], 1);
    let page_after = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/history"), &history)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(page_before, page_after);
    let mut ignored_parent = history.clone();
    ignored_parent["parent"] = json!("0".repeat(40));
    let current = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/history"), &ignored_parent)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        current, page_before,
        "A parent hint cannot select content outside the authorized current source/base"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn show_recovers_a_missing_restored_selection_after_recapturing_current_tips() {
    let app = TestApp::spawn().await;
    let f = Flow::new(&app).await;
    f.publish_file(&app, f.parent.repo_id, "restored.txt", b"original\n")
        .await;
    let original = menu(&app, &f).await;
    let code = |menu: &Value| {
        menu["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|repo| repo["repoId"] == f.parent.repo_id.to_string())
            .unwrap()
            .clone()
    };
    let original_tip = code(&original)["pushed"].as_str().unwrap().to_owned();
    f.publish_file(&app, f.parent.repo_id, "restored.txt", b"rewritten away\n")
        .await;
    let obsolete = code(&menu(&app, &f).await)["pushed"].clone();
    let branch: String = sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id=$1")
        .bind(f.source)
        .fetch_one(&app.db)
        .await
        .unwrap();
    helpers::seed_branch_ref(&app, f.parent.repo_id, &branch, &original_tip).await;
    let current = menu(&app, &f).await;
    let request = json!({"fingerprint":current["fingerprint"],"repos":[{
        "repoId":f.parent.repo_id,"base":code(&current)["base"],"pushed":original_tip,
        "commits":[obsolete],"includeUnpushed":false,"allPushed":false
    }]});
    let result = body(
        app.authenticated_client(&f.reviewer)
            .post_json(&f.path("source-control/apply"), &request)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(result["refreshed"], true);
    assert_eq!(
        result["missingCommits"][f.parent.repo_id.to_string()],
        json!([obsolete])
    );
    assert!(result.get("patches").is_none());
    assert_eq!(result["menu"]["fingerprint"], current["fingerprint"]);
    app.cleanup().await;
}

#[tokio::test]
async fn show_checks_older_guest_reads_but_uses_one_inspection_when_verified_natively() {
    let app = TestApp::spawn().await;
    for (verified, after_fingerprint, after_status, expected) in [
        (true, "changed", "failed", StatusCode::OK),
        (false, "before", "succeeded", StatusCode::OK),
        (false, "changed", "succeeded", StatusCode::CONFLICT),
        (false, "before", "failed", StatusCode::CONFLICT),
    ] {
        let f = Flow::new(&app).await;
        let owner = app.authenticated_client(&f.owner);
        let detail = body(
            owner.get(&format!("/api/v1/workspaces/{}", f.source)).await,
            StatusCode::OK,
        )
        .await;
        let source = helpers::TestWorkspace {
            id: f.source,
            org_id: f.org.id,
            repo_id: f.parent.repo_id,
            head_commit: f.parent.head_commit.clone(),
            name: detail["name"].as_str().unwrap().into(),
        };
        sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1")
            .bind(f.source)
            .execute(&app.db)
            .await
            .unwrap();
        let pushed = menu(&app, &f).await["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repoId"] == f.parent.repo_id.to_string())
            .unwrap()["pushed"]
            .clone();
        let mut live = json!({"selectable":true,"headCommit":pushed,"fingerprint":"before","localCommits":[],"paths":[],"patches":[]});
        if verified {
            live["stateVerified"] = json!(true);
        }
        let peer = helpers::TestWorkspaceRuntimePeer::attach_with_review_outcomes(&app, &source, f.owner.id, json!({
            "source_diff":{"status":"succeeded","result":live,"detail":{}},
            "source_state":{"status":after_status,"result":{"fingerprint":after_fingerprint,"paths":[]},"detail":{}}
        })).await;
        let current = body(
            owner.get(&f.path("source-control/menu")).await,
            StatusCode::OK,
        )
        .await;
        let repo = current["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["repoId"] == f.parent.repo_id.to_string())
            .unwrap();
        let response = owner.post_json(&f.path("source-control/apply"), &json!({"fingerprint":current["fingerprint"],"repos":[{
            "repoId":f.parent.repo_id,"base":repo["base"],"pushed":pushed,"commits":[],"includeUnpushed":true,"allPushed":false
        }]})).await;
        let status = response.status();
        let result: Value = response.json().await.unwrap();
        assert_eq!(status, expected, "{result}");
        if expected == StatusCode::OK {
            assert_eq!(result["data"]["fileCount"], 0);
            assert_eq!(result["data"]["private"], true);
        } else {
            assert!(result["data"].get("patches").is_none());
            assert!(result["error"]["message"].as_str().unwrap().contains(
                if after_status == "succeeded" {
                    "Files changed"
                } else {
                    "inspection failed"
                }
            ));
        }
        let modes = peer
            .invoke_params()
            .await
            .iter()
            .filter_map(|p| p["input"]["review_mode"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        assert_eq!(
            modes,
            if verified {
                vec!["source_diff"]
            } else {
                vec!["source_diff", "source_state"]
            }
        );
        drop(peer);
    }
    app.cleanup().await;
}
