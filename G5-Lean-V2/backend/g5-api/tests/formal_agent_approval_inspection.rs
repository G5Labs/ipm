//! Retained inspection after real coordinator cleanup, with ordinary task retention.
use super::*;
use g5_contracts::types::AgentApprovalOutcome;
use std::collections::HashSet;

#[tokio::test]
async fn retained_empty_tree_paths_are_available_after_review_cleanup() {
    let f = Fixture::new().await;
    let repository_path = f.app.repo_path(f.source.repo_id).await;
    let empty = {
        let repository = git2::Repository::open(&repository_path).unwrap();
        let parent = repository
            .find_commit(git2::Oid::from_str(&f.source.head_commit).unwrap())
            .unwrap();
        let tree_id = repository.treebuilder(None).unwrap().write().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        assert!(tree.is_empty());
        let signature = git2::Signature::now("Empty review", "empty@example.invalid").unwrap();
        repository
            .commit(
                None,
                &signature,
                &signature,
                "Empty reviewed tree",
                &tree,
                &[&parent],
            )
            .unwrap()
            .to_string()
    };
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &empty).await;
    let review = f.request(None).await;
    f.reconcile().await;
    f.fail_deadline(review).await;
    assert_eq!(f.report(review).await["cleanupPending"], false);
    let response = f.app.authenticated_client(&f.owner).get(&format!(
        "/api/v1/workspaces/{}/approvals/reviews/{review}/commits?repoId={}&side=source&operation=paths",
        f.source.id, f.source.repo_id,
    )).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["repoId"], f.source.repo_id.to_string());
    assert_eq!(body["data"]["commit"], empty);
    assert_eq!(body["data"]["content"], "");
    f.app.cleanup().await;
}

async fn page(f: &Fixture, user: &TestUser, review: Uuid, suffix: &str) -> Value {
    let response = f
        .app
        .authenticated_client(user)
        .get(&format!(
            "/api/v1/workspaces/{}/approvals/reviews/{review}/transcript{suffix}",
            f.source.id,
        ))
        .await;
    let status = response.status();
    let bytes = response.bytes().await.unwrap();
    assert!(
        bytes.len() <= 1024 * 1024,
        "response budget: {}",
        bytes.len()
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::OK, "{value}");
    value["data"].clone()
}
async fn seed_history(f: &Fixture, review: Uuid, count: i64) -> Uuid {
    let (task,session):(Uuid,Uuid)=sqlx::query_as(
        "SELECT r.task_id,t.chat_session_id FROM agent_approval_reviews r JOIN tasks t ON t.id=r.task_id WHERE r.id=$1",
    ).bind(review).fetch_one(&f.app.db).await.unwrap();
    let prompt = f.report(review).await["assignment"]["userPrompt"]
        .as_str()
        .unwrap()
        .to_string();
    sqlx::query(
        r#"
        INSERT INTO session_inputs(id,org_id,session_id,input_index,content,input_origin,task_id,attempt)
        VALUES($1,$2,$3,0,$4,'workflow',$5,1)
        "#,
    ).bind(Uuid::new_v4()).bind(f.org.id).bind(session).bind(prompt).bind(task).execute(&f.app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO chat_messages(id,org_id,session_id,role,content,message_index) VALUES($1,$2,$3,'assistant','Agent-authored free text remains visible.',0)",
    ).bind(Uuid::new_v4()).bind(f.org.id).bind(session).execute(&f.app.db).await.unwrap();
    for seq in 1..=count {
        let (kind, content, payload) = match seq {
            1 => (
                "tool_call",
                "Used source",
                json!({"call_id":"source-call","name":"mcp__formal_review__read_review","input":{"operation":"file","cut":"source"}}),
            ),
            2 => (
                "tool_call",
                "Used target",
                json!({"call_id":"target-call","name":"mcp__formal_review__read_review","input":{"operation":"file","cut":"target"}}),
            ),
            3 => (
                "tool_call",
                "Used diff",
                json!({"call_id":"diff-call","name":"mcp__formal_review__read_review","input":{"operation":"diff","cut":"source"}}),
            ),
            4 => (
                "tool_result",
                "RAW SOURCE",
                json!({"call_id":"source-call","name":"mcp__formal_review__read_review","output":"RAW SOURCE"}),
            ),
            5 => (
                "tool_result",
                "RAW TARGET",
                json!({"call_id":"target-call","name":"mcp__formal_review__read_review","output":"RAW TARGET"}),
            ),
            6 => (
                "tool_result",
                "RAW DIFF",
                json!({"call_id":"diff-call","name":"mcp__formal_review__read_review","output":"RAW DIFF"}),
            ),
            7 => (
                "checkpoint",
                "RAW TARGET PROGRESS",
                json!({"call_id":"target-call","name":"mcp__formal_review__read_review","content":"RAW TARGET PROGRESS"}),
            ),
            8 => (
                "checkpoint",
                "RAW SOURCE PROGRESS",
                json!({"call_id":"source-call","name":"mcp__formal_review__read_review","content":"RAW SOURCE PROGRESS"}),
            ),
            _ => ("output", "ordinary output", json!({"content":"safe"})),
        };
        let content = if seq >= 10 {
            "\u{1}".repeat(70000)
        } else {
            content.to_string()
        };
        sqlx::query(
            r#"
            INSERT INTO task_events(id,org_id,task_id,entry_type,content,metadata,attempt,vm_id,seq)
            VALUES($1,$2,$3,$4,$5,$6,1,'inspection-vm',$7)
            "#,
        ).bind(Uuid::new_v4()).bind(f.org.id).bind(task).bind(kind).bind(content)
         .bind(json!({"kind":if kind=="tool_call" {"tool.started"}else if kind=="tool_result" {"tool.completed"}else if kind=="checkpoint" {"tool.progress"}else{"message.delta"},"payload":payload}))
         .bind(seq).execute(&f.app.db).await.unwrap();
    }
    task
}

#[tokio::test]
async fn retained_transcript_paginates_bounds_bytes_and_gates_target_results_across_pages() {
    let f = Fixture::with_connections(8).await;
    let review = f.settle(AgentApprovalOutcome::Approved).await;
    assert_eq!(f.report(review).await["cleanupPending"], false);
    seed_history(&f, review, 25).await;
    let member =
        TestUser::create_with_scopes(&f.app, f.org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    // Retained target content is redacted when the target is no longer readable.
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let mut all = vec![];
    let mut cursor = None;
    let mut seen = HashSet::new();
    for _ in 0..40 {
        // A small limit ensures call/results land on different pages.
        let suffix = format!(
            "?limit=3{}",
            cursor
                .as_ref()
                .map(|cursor| format!("&cursor={cursor}"))
                .unwrap_or_default()
        );
        let result = page(&f, &member, review, &suffix).await;
        assert_eq!(result["eventsTrimmed"], false);
        let items = result["items"].as_array().unwrap();
        assert!(items.len() <= 3);
        for item in items {
            assert!(seen.insert(item["id"].clone().to_string()));
            all.push(item.clone());
        }
        cursor = result["nextCursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    assert!(cursor.is_none());
    assert_eq!(all.len(), 27);
    assert_eq!(all[0]["kind"], "input");
    assert!(all[0]["content"].as_str().unwrap().contains("Review"));
    let serialized = serde_json::to_string(&all).unwrap();
    assert!(serialized.contains("RAW SOURCE"));
    assert!(serialized.contains("RAW SOURCE PROGRESS"));
    assert!(!serialized.contains("RAW TARGET"));
    assert!(!serialized.contains("RAW TARGET PROGRESS"));
    assert!(!serialized.contains("RAW DIFF"));
    assert!(serialized.contains("Agent-authored free text"));
    assert!(all.iter().any(|row| row["contentTruncated"] == true));
    // Restore target availability to prove that the same retained evidence is
    // readable once current target authority succeeds again.
    sqlx::query("UPDATE workspaces SET deleted_at=NULL WHERE id=$1")
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let owner = page(&f, &f.owner, review, "?limit=200").await;
    assert!(
        owner["items"].as_array().unwrap().len() < 27,
        "byte budget must end early"
    );
    assert!(owner["nextCursor"].is_string());
    assert!(owner.to_string().contains("RAW TARGET"));
    assert!(owner.to_string().contains("RAW TARGET PROGRESS"));
    let invalid = f
        .app
        .authenticated_client(&member)
        .get(&format!(
            "/api/v1/workspaces/{}/approvals/reviews/{review}/transcript?cursor=bad",
            f.source.id
        ))
        .await;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let commits = format!(
        "/api/v1/workspaces/{}/approvals/reviews/{review}/commits?repoId={}",
        f.source.id, f.source.repo_id
    );
    for suffix in ["&side=base", "&operation=diff"] {
        let denied = f
            .app
            .authenticated_client(&member)
            .get(&format!("{commits}{suffix}"))
            .await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    }
    assert_eq!(
        f.app
            .authenticated_client(&member)
            .get(&commits)
            .await
            .status(),
        StatusCode::OK
    );
    let wrong_repo = f
        .app
        .authenticated_client(&member)
        .get(&commits.replace(&f.source.repo_id.to_string(), &Uuid::new_v4().to_string()))
        .await;
    assert_eq!(wrong_repo.status(), StatusCode::UNPROCESSABLE_ENTITY);
    f.app.cleanup().await;
}

/// `GET` an agent route of `caller`'s workspace with that VM's credential.
async fn agent_get(f: &Fixture, token: &str, caller: Uuid, leaf: &str) -> (StatusCode, Value) {
    let response = f
        .app
        .client
        .get(format!(
            "{}/api/internal/workspace-agent/{caller}/{leaf}",
            f.app.base_url
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn vm_token(f: &Fixture, workspace: Uuid, owner: Uuid, vm: Uuid) -> String {
    f.app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, workspace, owner, vm, 1, 3600)
        .unwrap()
}

async fn owned_workspace(f: &Fixture, owner: Uuid) -> TestWorkspace {
    let workspace = TestWorkspace::create(&f.app, &f.org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(owner)
        .execute(&f.app.db)
        .await
        .unwrap();
    workspace
}

/// G5-Lean#1019: the agent in the reviewed workspace reads what the person
/// reads there, through the same relation as `approvals`, and nothing more.
#[tokio::test]
async fn an_agent_reads_review_evidence_on_any_readable_workspace() {
    let f = Fixture::with_connections(8).await;
    let review = f.settle(AgentApprovalOutcome::Rejected).await;
    seed_history(&f, review, 12).await;
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&f.app, &f.source, f.owner.id).await;
    let token = vm_token(&f, f.source.id, f.owner.id, runtime.vm_id());
    let reviews = format!("workspaces/{}/approvals/reviews/{review}", f.source.id);

    let (status, body) = agent_get(&f, &token, f.source.id, &format!("{reviews}/report")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let person = f.report(review).await;
    assert_eq!(body["data"]["result"], "rejected");
    assert!(body["data"]["document"]
        .as_str()
        .is_some_and(|d| !d.is_empty()));
    for field in [
        "reviewId",
        "stage",
        "result",
        "document",
        "sha256",
        "assignment",
    ] {
        assert_eq!(body["data"][field], person[field], "{field}");
    }

    // The transcript is the owner's view, merge-target output included, and pages.
    let (status, body) = agent_get(
        &f,
        &token,
        f.source.id,
        &format!("{reviews}/transcript?limit=200"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"], page(&f, &f.owner, review, "?limit=200").await);
    assert!(body["data"].to_string().contains("RAW TARGET"));
    let (status, first) = agent_get(
        &f,
        &token,
        f.source.id,
        &format!("{reviews}/transcript?limit=2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let cursor = first["data"]["nextCursor"].as_str().unwrap();
    let (status, next) = agent_get(
        &f,
        &token,
        f.source.id,
        &format!("{reviews}/transcript?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["data"]["items"][0], body["data"]["items"][2]);
    let (status, _) = agent_get(
        &f,
        &token,
        f.source.id,
        &format!("{reviews}/transcript?cursor=bad"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let commits = format!(
        "{reviews}/commits?repoId={}&side=source&operation=paths",
        f.source.repo_id
    );
    let (status, body) = agent_get(&f, &token, f.source.id, &commits).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let person = f
        .app
        .authenticated_client(&f.owner)
        .get(&format!("/api/v1/{commits}"))
        .await;
    assert_eq!(person.status(), StatusCode::OK);
    let person: Value = person.json().await.unwrap();
    assert_eq!(body["data"], person["data"]);

    // Another of the owner's workspaces has no such review: not found.
    let sibling = owned_workspace(&f, f.owner.id).await;
    for leaf in ["report", "transcript"] {
        let (status, body) = agent_get(
            &f,
            &token,
            f.source.id,
            &format!(
                "workspaces/{}/approvals/reviews/{review}/{leaf}",
                sibling.id
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{leaf} {body}");
    }
    // The review workspace itself is never a target.
    let (status, _) = agent_get(
        &f,
        &token,
        f.source.id,
        &format!(
            "workspaces/{}/approvals/reviews/{review}/report",
            f.child(review).await
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Another member's agent now reads the reviewed workspace's evidence when
    // the member can read it in the UI, as `approvals` does: #1099 widened the
    // agent read scope to `g5.security.agent-read-scope`.
    let member = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[
            g5_auth::Scope::WorkspacesRead,
            g5_auth::Scope::WorkspacesWrite,
        ],
    )
    .await;
    assert_eq!(
        f.app
            .authenticated_client(&member)
            .get(&format!("/api/v1/{reviews}/report"))
            .await
            .status(),
        StatusCode::OK,
        "the member reads the report as a person"
    );
    let theirs = owned_workspace(&f, member.id).await;
    let their_runtime = helpers::TestWorkspaceRuntimePeer::attach(&f.app, &theirs, member.id).await;
    let their_token = vm_token(&f, theirs.id, member.id, their_runtime.vm_id());
    for leaf in [
        "approvals".to_string(),
        format!("approvals/reviews/{review}/report"),
        format!("approvals/reviews/{review}/transcript"),
        format!(
            "approvals/reviews/{review}/commits?repoId={}&side=source&operation=paths",
            f.source.repo_id
        ),
    ] {
        let (status, body) = agent_get(
            &f,
            &their_token,
            theirs.id,
            &format!("workspaces/{}/{leaf}", f.source.id),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{leaf} {body}");
    }
    // A workspace the member cannot read (another's personal one) is still refused.
    let hidden = TestWorkspace::create_personal(&f.app, &f.org, f.owner.id).await;
    let (status, _) = agent_get(
        &f,
        &their_token,
        theirs.id,
        &format!("workspaces/{}/approvals", hidden.id),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // A credential names only its own workspace as the caller.
    let (status, _) = agent_get(&f, &their_token, f.source.id, &format!("{reviews}/report")).await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
        "{status}"
    );
    drop(their_runtime);
    drop(runtime);
    f.app.cleanup().await;
}

#[tokio::test]
async fn ordinary_timeline_cleanup_keeps_transcript_inspection_and_marks_trimmed_history() {
    let f = Fixture::with_connections(8).await;
    let review = f.settle(AgentApprovalOutcome::Approved).await;
    let task = seed_history(&f, review, 110).await;
    sqlx::query("UPDATE tasks SET updated_at=now()-interval '31 days' WHERE id=$1")
        .bind(task)
        .execute(&f.app.db)
        .await
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let runner = tokio::spawn(
        g5_api::maintenance::timeline_cleanup::run_timeline_cleanup_loop(
            g5_api::maintenance::TenantMaintenanceDeps {
                system_pool: f.app.system_pool.clone(),
                tenant_pool: f.app.state.tenant_pool.clone(),
                authz: f.app.state.authz.clone(),
                repo_storage: f.app.state.repo_storage.clone(),
            },
            g5_api::maintenance::timeline_cleanup::TimelineCleanupConfig {
                interval: std::time::Duration::from_millis(20),
                ..Default::default()
            },
            cancel.clone(),
        ),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM task_events WHERE task_id=$1")
                    .bind(task)
                    .fetch_one(&f.app.db)
                    .await
                    .unwrap();
            if count == 100 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    runner.await.unwrap();
    let retained = page(&f, &f.owner, review, "?limit=200").await;
    assert_eq!(retained["eventsTrimmed"], true);
    assert_eq!(retained["items"][0]["kind"], "input");
    assert!(f.report(review).await.get("items").is_none());
    f.app.cleanup().await;
}

#[tokio::test]
async fn private_evidence_refs_survive_cleanup_source_ref_loss_and_gc_and_support_frozen_retry() {
    let mut f = Fixture::with_connections(8).await;
    f.source
        .commit_file(&f.app, "retained.txt", "The frozen source.\n")
        .await;
    let parent_only = {
        let path = f.app.repo_path(f.source.repo_id).await;
        let repo = git2::Repository::open(path).unwrap();
        let old: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
            .bind(f.target)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
        let parent = repo
            .find_reference(&format!("refs/heads/{old}"))
            .unwrap()
            .peel_to_commit()
            .unwrap();
        let sig = git2::Signature::now("Parent", "parent@example.invalid").unwrap();
        let commit = repo
            .commit(
                None,
                &sig,
                &sig,
                "Parent moved independently",
                &parent.tree().unwrap(),
                &[&parent],
            )
            .unwrap()
            .to_string();
        (old, commit)
    };
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &parent_only.0, &parent_only.1).await;
    let review = f.request(None).await;
    f.reconcile().await;
    let assignment = f.report(review).await["assignment"].clone();
    f.fail_deadline(review).await;
    assert_eq!(f.report(review).await["cleanupPending"], false);
    let runtime = helpers::TestWorkspaceRuntimePeer::attach(&f.app, &f.source, f.owner.id).await;
    let vm_token = f
        .app
        .state
        .jwt_minter
        .as_ref()
        .unwrap()
        .mint_workspace_incarnation(f.org.id, f.source.id, f.owner.id, runtime.vm_id(), 1, 3600)
        .unwrap();
    let path = f.app.repo_path(f.source.repo_id).await;
    let raw = git2::Repository::open(&path).unwrap();
    let names = raw
        .references()
        .unwrap()
        .map(|r| r.unwrap().name().unwrap().to_string())
        .collect::<Vec<_>>();
    let evidence = names
        .iter()
        .filter(|name| name.starts_with("refs/g5/approval-evidence/"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(evidence.len(), 3);
    for token in [&f.owner.jwt, &vm_token] {
        let advertised = f
            .app
            .client
            .get(format!(
                "{}/git/{}/info/refs?service=git-upload-pack",
                f.app.base_url, f.source.repo_id
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        let status = advertised.status();
        let body = advertised.text().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body.contains("refs/g5/approval-evidence/"));
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args([
                "-c",
                &format!("http.extraHeader=Authorization: Bearer {token}"),
                "push",
            ])
            .arg(format!("{}/git/{}", f.app.base_url, f.source.repo_id))
            .arg(format!("{}:{}", f.source.head_commit, evidence[0]))
            .output()
            .await
            .unwrap();
        assert!(!output.status.success(), "private evidence push admitted");
    }
    for name in names {
        if !name.starts_with("refs/g5/approval-evidence/") {
            raw.find_reference(&name).unwrap().delete().unwrap();
        }
    }
    drop(raw);
    for args in [
        ["reflog", "expire", "--expire=now", "--all"].as_slice(),
        ["gc", "--prune=now"].as_slice(),
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for side in ["source", "base"] {
        let response = f
            .app
            .authenticated_client(&f.owner)
            .get(&format!(
                "/api/v1/workspaces/{}/approvals/reviews/{review}/commits?repoId={}&side={side}",
                f.source.id, f.source.repo_id
            ))
            .await;
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!body["data"]["content"].as_str().unwrap().is_empty());
    }
    let advertised = f
        .app
        .authenticated_client(&f.owner)
        .get(&format!(
            "/git/{}/info/refs?service=git-upload-pack",
            f.source.repo_id
        ))
        .await;
    let status = advertised.status();
    let body = advertised.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("refs/g5/approval-evidence/"));
    let retry = f.request(Some(review)).await;
    f.reconcile().await;
    let frozen = f.report(retry).await;
    assert_eq!(frozen["stage"], "start", "{frozen}");
    assert_eq!(frozen["assignment"], assignment);
    assert_eq!(
        git2::Repository::open(&path)
            .unwrap()
            .references_glob("refs/g5/approval-evidence/*")
            .unwrap()
            .count(),
        3
    );
    // Retention does not grant new authority after the source itself is deleted.
    sqlx::query("UPDATE workspaces SET deleted_at=now() WHERE id=$1")
        .bind(f.source.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let denied = f
        .app
        .authenticated_client(&f.owner)
        .get(&format!(
            "/api/v1/workspaces/{}/approvals/reviews/{review}/commits?repoId={}",
            f.source.id, f.source.repo_id
        ))
        .await;
    assert!(!denied.status().is_success());
    drop(runtime);
    f.app.cleanup().await;
}

/// Parent HEAD deliberately diverges, so using it as the review base changes
/// the diff. Exercise capture, report validation, retained inspection and
/// authorized resource discovery through production HTTP routes.
#[tokio::test]
async fn workspace_base_names_and_review_inventory_survive_capture_and_cleanup() {
    let f = Fixture::with_connections(2).await;
    let path = f.app.repo_path(f.source.repo_id).await;
    let make_commit = |file: &str, content: &str| {
        let repo = git2::Repository::open(&path).unwrap();
        let parent = repo
            .find_commit(git2::Oid::from_str(&f.source.head_commit).unwrap())
            .unwrap();
        let mut tree = repo.treebuilder(Some(&parent.tree().unwrap())).unwrap();
        tree.insert(file, repo.blob(content.as_bytes()).unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let sig = git2::Signature::now("Review test", "review@example.invalid").unwrap();
        repo.commit(None, &sig, &sig, file, &tree, &[&parent])
            .unwrap()
            .to_string()
    };
    let source = make_commit("source-only.txt", "Source change\n");
    let parent = make_commit("parent-only.txt", "Unrelated parent change\n");
    let target_branch: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id=$1")
        .bind(f.target)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &source).await;
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &target_branch, &parent).await;
    sqlx::query("UPDATE repos SET name='Readable app' WHERE id=$1")
        .bind(f.source.repo_id)
        .execute(&f.app.db)
        .await
        .unwrap();
    // Names address the workspace in the UI; Git capture must keep using
    // each immutable branch, even when both sides were renamed before review.
    let client = f.app.authenticated_client(&f.owner);
    for (workspace_id, name, branch) in [
        (f.source.id, "renamed-review-source", f.source.name.as_str()),
        (f.target, "renamed-review-target", target_branch.as_str()),
    ] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/rename"),
                &json!({"name": name}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let stored: (String, String) =
            sqlx::query_as("SELECT name, branch_name FROM workspaces WHERE id=$1")
                .bind(workspace_id)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        assert_eq!(stored, (name.to_string(), branch.to_string()));
    }
    let review = f.request(None).await;
    f.reconcile().await;
    let report = f.report(review).await;
    let resolution = &report["assignment"]["resolution"];
    assert_eq!(resolution["schemaVersion"], 6);
    assert_eq!(resolution["sourceHeads"][0]["headCommit"], source);
    assert_eq!(resolution["targetHeads"][0]["headCommit"], parent);
    assert_eq!(
        resolution["workspaceBaseHeads"][0]["headCommit"],
        f.source.head_commit
    );
    assert_eq!(
        resolution["repositoryNames"][f.source.repo_id.to_string()],
        "Readable app"
    );
    let client = f.app.authenticated_client(&f.owner);
    let endpoint = format!(
        "/api/v1/workspaces/{}/approvals/reviews/{review}/commits?repoId={}",
        f.source.id, f.source.repo_id
    );
    let diff: Value = client
        .get(&format!("{endpoint}&operation=diff"))
        .await
        .json()
        .await
        .unwrap();
    assert!(diff["data"]["content"]
        .as_str()
        .unwrap()
        .contains("source-only.txt"));
    assert!(!diff["data"]["content"]
        .as_str()
        .unwrap()
        .contains("parent-only.txt"));
    let base: Value = client
        .get(&format!("{endpoint}&side=base"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(base["data"]["commit"], f.source.head_commit);

    let child: Uuid =
        sqlx::query_scalar("SELECT review_workspace_id FROM agent_approval_reviews WHERE id=$1")
            .bind(review)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    let viewer =
        TestUser::create_with_scopes(&f.app, f.org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    let outsider =
        TestUser::create_with_scopes(&f.app, f.org.id, &[g5_auth::Scope::WorkspacesRead]).await;
    sqlx::query("UPDATE workspaces SET created_by=$2 WHERE id=$1")
        .bind(child)
        .bind(viewer.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO usage_records(id,org_id,workspace_id,provider,model,input_tokens,output_tokens,total_tokens,cost_usd,request_type) VALUES($1,$2,$3,'anthropic','review',0,0,0,2.50,'chat'),($4,$2,$3,'anthropic','review',0,0,0,0.75,'chat')").bind(Uuid::new_v4()).bind(f.org.id).bind(child).bind(Uuid::new_v4()).execute(&f.app.db).await.unwrap();
    let host = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,slot_size_reported,slot_cpus,slot_mem_mib) VALUES($1,$2,'us-central1','ready',now(),true,4,8192)").bind(host).bind(f.org.id).execute(&f.app.db).await.unwrap();
    sqlx::query("UPDATE workspace_vms SET host_id=$3,vm_id=$4,epoch=1,status='running',started_at=now() WHERE org_id=$1 AND workspace_id=$2").bind(f.org.id).bind(child).bind(host).bind(Uuid::new_v4().to_string()).execute(&f.app.db).await.unwrap();
    // Home fetches both inventories concurrently. Neither may acquire a
    // second request-pool connection while retaining its first one.
    let workspace_path = format!("/api/v1/orgs/{}/workspaces", f.org.id);
    let compute_path = format!("/api/v1/orgs/{}/compute/running", f.org.id);
    let (workspaces, compute) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(client.get(&workspace_path), client.get(&compute_path))
    })
    .await
    .expect("concurrent Home inventories must not exhaust the two-connection request pool");
    assert_eq!(workspaces.status(), StatusCode::OK);
    assert_eq!(compute.status(), StatusCode::OK);
    for user in [&f.owner, &viewer, &outsider] {
        let client = f.app.authenticated_client(user);
        for suffix in ["workspaces", "compute/running"] {
            let response = client
                .get(&format!("/api/v1/orgs/{}/{suffix}", f.org.id))
                .await;
            assert_eq!(response.status(), StatusCode::OK);
            let value: Value = response.json().await.unwrap();
            let row = value["data"].as_array().unwrap().iter().find(|row| {
                row[if suffix == "workspaces" {
                    "id"
                } else {
                    "workspaceId"
                }] == child.to_string()
            });
            {
                let row = row.expect("review resource is discoverable");
                assert_eq!(row["agentReview"]["reviewId"], review.to_string());
                assert_eq!(
                    row["agentReview"]["sourceWorkspaceId"],
                    f.source.id.to_string()
                );
                assert_eq!(
                    row["agentReview"]["sourceWorkspaceName"],
                    "renamed-review-source"
                );
                if suffix == "workspaces" {
                    assert_eq!(row["isReview"], true);
                    assert_eq!(row["createdBy"], viewer.id.to_string());
                    assert_eq!(row["ownerUserId"], f.owner.id.to_string());
                    assert_eq!(row["cost"], 3.25);
                } else {
                    assert_eq!(row["slotCpus"], 4);
                }
            }
        }
        assert!(
            !client
                .get(&format!("/api/v1/workspaces/{child}"))
                .await
                .status()
                .is_success(),
            "inventory grants no live access"
        );
        assert!(
            !client
                .post_json(
                    &format!("/api/v1/workspaces/{child}/rename"),
                    &json!({"name": "must-remain-private"}),
                )
                .await
                .status()
                .is_success(),
            "inventory grants no rename authority"
        );
    }
    sqlx::query(
        "UPDATE workspace_vms SET status='stopped',host_id=NULL,vm_id=NULL WHERE workspace_id=$1",
    )
    .bind(child)
    .execute(&f.app.db)
    .await
    .unwrap();
    let compute: Value = client
        .get(&format!("/api/v1/orgs/{}/compute/running", f.org.id))
        .await
        .json()
        .await
        .unwrap();
    assert!(compute["data"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["workspaceId"] != child.to_string()));
    f.terminal_receipt(review, AgentApprovalOutcome::Approved)
        .await;
    f.reconcile().await;
    f.reconcile().await;
    f.reconcile().await; // Retirement is independent of verdict settlement.
    sqlx::query("UPDATE repos SET name='Renamed after capture' WHERE id=$1")
        .bind(f.source.repo_id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let retained = f.report(review).await;
    assert_eq!(retained["stage"], "effective");
    assert_eq!(retained["cleanupPending"], false, "{retained}");
    let document = retained["document"].as_str().unwrap();
    assert!(document.contains("repoName: Readable app"));
    assert!(document.contains("workspaceBase:"));
    assert!(!document.contains("Renamed after capture"));
    let list: Value = client
        .get(&format!("/api/v1/orgs/{}/workspaces", f.org.id))
        .await
        .json()
        .await
        .unwrap();
    assert!(list["data"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["id"] != child.to_string()));
    let base: Value = client
        .get(&format!("{endpoint}&side=base"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        base["data"]["commit"], f.source.head_commit,
        "base evidence survives cleanup"
    );
    // Integrating parent history changes the computed merge base, but must not
    // change the saved workspace base used by a fresh review.
    let integrated = {
        let repo = git2::Repository::open(&path).unwrap();
        let parent_commit = repo
            .find_commit(git2::Oid::from_str(&parent).unwrap())
            .unwrap();
        let mut tree = repo
            .treebuilder(Some(&parent_commit.tree().unwrap()))
            .unwrap();
        tree.insert(
            "source-only.txt",
            repo.blob(b"Integrated source change\n").unwrap(),
            0o100644,
        )
        .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let sig = git2::Signature::now("Review test", "review@example.invalid").unwrap();
        repo.commit(
            None,
            &sig,
            &sig,
            "Integrate parent",
            &tree,
            &[&parent_commit],
        )
        .unwrap()
        .to_string()
    };
    helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &integrated).await;
    let next = f.request(None).await;
    f.reconcile().await;
    let next_report = f.report(next).await;
    assert_eq!(
        next_report["assignment"]["resolution"]["workspaceBaseHeads"][0]["headCommit"],
        f.source.head_commit
    );
    assert_eq!(
        next_report["assignment"]["resolution"]["targetHeads"][0]["headCommit"],
        parent
    );
    let diff: Value = client
        .get(&format!(
            "/api/v1/workspaces/{}/approvals/reviews/{next}/commits?repoId={}&operation=diff",
            f.source.id, f.source.repo_id
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        !diff["data"]["content"]
            .as_str()
            .unwrap()
            .contains("parent-only.txt"),
        "merge-base comparison excludes integrated parent changes"
    );
    f.fail_deadline(next).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn schema4_rebased_and_parent_merged_assignments_match_source_control_and_survive_frozen_retry(
) {
    helpers::tracing_capture();
    let fixture = g5_scm::comparison_fixture::comparison_fixture();
    for name in ["rebase-834", "parent-merge"] {
        let case = fixture.scenarios.iter().find(|c| c.name == name).unwrap();
        let mut f = Fixture::with_connections(4).await;
        let path = f.app.repo_path(f.source.repo_id).await;
        let imported = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["fetch", "--quiet", "--no-tags"])
            .arg(fixture.directory.path())
            .arg("+refs/heads/*:refs/fixtures/841/*")
            .output()
            .unwrap();
        assert!(
            imported.status.success(),
            "{}",
            String::from_utf8_lossy(&imported.stderr)
        );
        let fork = fixture.heads[case.base].clone();
        let parent = fixture.heads[case.parent].clone();
        let source = fixture.heads[case.source].clone();
        helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &fork).await;
        let target_branch: String =
            sqlx::query_scalar("SELECT branch_name FROM workspaces WHERE id=$1")
                .bind(f.target)
                .fetch_one(&f.app.db)
                .await
                .unwrap();
        helpers::seed_branch_ref(&f.app, f.source.repo_id, &target_branch, &parent).await;
        f.refork_source().await;
        f.source.head_commit = fork.clone();
        helpers::seed_branch_ref(&f.app, f.source.repo_id, &f.source.name, &source).await;
        let client = f.app.authenticated_client(&f.owner);
        let root = format!("/api/v1/workspaces/{}", f.source.id);
        let menu = helpers::review_flow::body(
            client.get(&format!("{root}/source-control/menu")).await,
            StatusCode::OK,
        )
        .await;
        let repo = &menu["repos"][0];
        let oracle = g5_scm::GitRepo::open(&path)
            .unwrap()
            .pushed_comparison(&source, &parent, Some(&fork), 250)
            .unwrap();
        assert_eq!(
            repo["base"], oracle.merge_base,
            "{}: menu={menu} expected parent={parent}",
            case.name
        );
        assert_eq!(repo["commits"], json!(oracle.commits));
        let applied=helpers::review_flow::body(client.post_json(&format!("{root}/source-control/apply"),&json!({"repos":[{"repoId":repo["repoId"],"base":repo["base"],"pushed":repo["pushed"],"commits":[],"includeUnpushed":false,"liveFingerprint":null,"allPushed":true}]})).await,StatusCode::OK).await;
        let mut paths = applied["patches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["file"]["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths, case.paths);
        let review = f.request(None).await;
        f.reconcile().await;
        let captured = f.report(review).await;
        let assignment = captured["assignment"].clone();
        let resolution = &assignment["resolution"];
        assert_eq!(resolution["schemaVersion"], 6);
        assert!(
            chrono::DateTime::parse_from_rfc3339(resolution["assignedAt"].as_str().unwrap())
                .is_ok()
        );
        assert_eq!(resolution["sourceHeads"][0]["headCommit"], source);
        assert_eq!(resolution["targetHeads"][0]["headCommit"], parent);
        assert_eq!(resolution["workspaceBaseHeads"][0]["headCommit"], fork);
        assert_eq!(
            resolution["mergeBase"]["repositories"][0]["gitSha"],
            oracle.merge_base
        );
        let diff = helpers::review_flow::body(
            client
                .get(&format!(
                    "{root}/approvals/reviews/{review}/commits?repoId={}&operation=diff",
                    f.source.repo_id
                ))
                .await,
            StatusCode::OK,
        )
        .await;
        let parsed =
            g5_scm::source_control_patch(diff["content"].as_str().unwrap().as_bytes()).unwrap();
        assert_eq!(
            parsed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            case.paths
        );
        f.fail_deadline(review).await;
        assert!(f.report(review).await["document"].is_null());
        helpers::seed_branch_ref(
            &f.app,
            f.source.repo_id,
            &target_branch,
            &fixture.heads["E"],
        )
        .await;
        helpers::seed_branch_ref(
            &f.app,
            f.source.repo_id,
            &f.source.name,
            &fixture.heads["Q3"],
        )
        .await;
        let retry = f.request(Some(review)).await;
        f.reconcile().await;
        assert_eq!(f.report(retry).await["assignment"], assignment);
        f.terminal_receipt(retry, AgentApprovalOutcome::Approved)
            .await;
        f.reconcile().await;
        f.reconcile().await;
        f.reconcile().await;
        let retained = f.report(retry).await;
        assert_eq!(retained["stage"], "effective", "{retained}");
        assert_eq!(retained["assignment"], assignment);
        let report = retained["document"].as_str().unwrap();
        assert!(report.contains("schemaVersion: 6"));
        assert!(report.contains("summary:"));
        assert!(report.contains("submittedAt:"));
        let invocation: g5_contracts::types::FormalApprovalInvocation =
            serde_json::from_value(assignment).unwrap();
        g5_contracts::types::validate_agent_approval_markdown_v2(
            report.as_bytes(),
            &invocation.resolution,
        )
        .unwrap();
        f.app.cleanup().await;
    }
}
