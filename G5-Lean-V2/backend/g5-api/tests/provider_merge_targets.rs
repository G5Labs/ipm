//! Real provider lifecycle, HTTP, PostgreSQL and Git. The provider double
//! records the actual target at effect time independently of G5's evidence.
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::Notify;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

type Acceptance = Arc<Mutex<Option<(String, String)>>>;

fn accept(remote: &std::path::Path, source: &str, branch: &str) -> (String, String) {
    let repo = git2::Repository::open_bare(remote).unwrap();
    let before = repo
        .find_reference(&format!("refs/heads/{branch}"))
        .unwrap()
        .target()
        .unwrap();
    let parent = repo.find_commit(before).unwrap();
    let source = repo.find_commit(source.parse().unwrap()).unwrap();
    let ancestor = repo
        .find_commit(repo.merge_base(parent.id(), source.id()).unwrap())
        .unwrap();
    let mut index = repo
        .merge_trees(
            &ancestor.tree().unwrap(),
            &parent.tree().unwrap(),
            &source.tree().unwrap(),
            None,
        )
        .unwrap();
    assert!(!index.has_conflicts());
    let tree = repo.find_tree(index.write_tree_to(&repo).unwrap()).unwrap();
    let signature = git2::Signature::now("Provider", "provider@example.test").unwrap();
    let sha = repo
        .commit(
            Some(&format!("refs/heads/{branch}")),
            &source.author(),
            &signature,
            "provider rebase",
            &tree,
            &[&parent],
        )
        .unwrap();
    (before.to_string(), sha.to_string())
}

async fn pull(
    provider: &ProviderFixture,
    number: i64,
    source: &str,
    target: &str,
    accepted: Acceptance,
) {
    let source = source.to_owned();
    let target = target.to_owned();
    let base = provider.fixture.base_head.clone();
    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/api/pulls/{number}")))
        .respond_with(move |_: &wiremock::Request| {
            let result = accepted.lock().unwrap();
            ResponseTemplate::new(200).set_body_json(json!({
                "number":number,"html_url":format!("https://github.com/acme/api/pull/{number}"),
                "state":if result.is_some(){"closed"}else{"open"},"merged":result.is_some(),"mergeable":false,
                "head":{"sha":source},"base":{"ref":target,"sha":base},
                "merge_commit_sha":result.as_ref().map(|(_,sha)|sha)
            }))
        })
        .mount(&provider.github)
        .await;
}

async fn success(
    provider: &ProviderFixture,
    number: i64,
    source: &str,
    target: &str,
    accepted: Acceptance,
) {
    pull(provider, number, source, target, accepted.clone()).await;
    let remote = provider.remote.path().join("remote.git");
    let source = source.to_owned();
    let target = target.to_owned();
    Mock::given(method("PUT"))
        .and(path(format!("/repos/acme/api/pulls/{number}/merge")))
        .respond_with(move |_: &wiremock::Request| {
            let mut result = accepted.lock().unwrap();
            assert!(result.is_none(), "a provider PR merges at most once");
            let outcome = accept(&remote, &source, &target);
            let sha = outcome.1.clone();
            *result = Some(outcome);
            ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":sha}))
        })
        .expect(1)
        .mount(&provider.github)
        .await;
}

async fn source_pr(
    provider: &ProviderFixture,
    name: &str,
    target: Uuid,
    remote_target: &str,
    number: i64,
) -> (Uuid, Uuid, String) {
    let app = &provider.app;
    let f = &provider.fixture;
    let source = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,branch_name,owner_user_id,created_by,cloned_from_id)
        SELECT $2,org_id,$3,$3,owner_user_id,created_by,$4 FROM workspaces WHERE id=$1",
    )
    .bind(f.source.id)
    .bind(source)
    .bind(name)
    .bind(target)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
        .bind(f.org.id)
        .bind(source)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role)
        SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1",
    )
    .bind(f.source.id)
    .bind(source)
    .execute(&app.db)
    .await
    .unwrap();
    for repo in [f.source.repo_id, f.other_repo_id] {
        let base = head(app, repo, "main").await;
        helpers::seed_branch_ref(app, repo, name, &base).await;
    }
    let source_head = commit(app, f.source.repo_id, name, &format!("{name}.txt")).await;
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,$4)")
        .bind(f.org.id).bind(source).bind(f.source.repo_id).bind(name).execute(&app.db).await.unwrap();
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &provider.remote.path().join("remote.git"),
        &[
            "fetch",
            mirror.to_str().unwrap(),
            &format!("refs/heads/{name}:refs/heads/{name}"),
        ],
    );
    let coordinator = &app.state.coordinated_repo_publication_service;
    let db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    let prepared = coordinator
        .prepare_workspace_pull_request_create(
            db,
            &app.state.workspace_merge_service,
            app.repo_storage.clone(),
            source,
            target,
            f.owner.id,
            None,
            None,
        )
        .await
        .unwrap();
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    let created = coordinator
        .apply_prepared_workspace_pull_request_create(
            &mut db,
            &app.state.workspace_merge_service,
            prepared,
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    let pr = created.pull_request.id;
    sqlx::query("INSERT INTO workspace_pull_request_provider_repos(org_id,pull_request_id,repo_id,provider,provider_repo_full_name,
        source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha)
        VALUES($1,$2,$3,'github','acme/api',$4,$5,$6,$7,'open',$8,$9)")
        .bind(f.org.id).bind(pr).bind(f.source.repo_id).bind(name).bind(remote_target).bind(number)
        .bind(format!("https://github.com/acme/api/pull/{number}"))
        .bind(&source_head).bind(&f.base_head).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspace_pull_requests SET provider_status='open' WHERE id=$1")
        .bind(pr)
        .execute(&app.db)
        .await
        .unwrap();
    (pr, source, source_head)
}

async fn other_target(provider: &ProviderFixture, name: &str, remote_branch: &str) -> Uuid {
    let app = &provider.app;
    let f = &provider.fixture;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,branch_name,owner_user_id,created_by)
        SELECT $2,org_id,$3,$3,owner_user_id,created_by FROM workspaces WHERE id=$1",
    )
    .bind(f.source.id)
    .bind(id)
    .bind(name)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO workspace_settings(org_id,workspace_id) VALUES($1,$2)")
        .bind(f.org.id)
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role)
        SELECT org_id,$2,repo_id,root_path,role FROM workspace_repos WHERE workspace_id=$1",
    )
    .bind(f.base_id)
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
    for repo in [f.source.repo_id, f.other_repo_id] {
        helpers::seed_branch_ref(app, repo, name, &head(app, repo, "main").await).await;
    }
    sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,$4)")
        .bind(f.org.id).bind(id).bind(f.source.repo_id).bind(remote_branch).execute(&app.db).await.unwrap();
    git2::Repository::open_bare(provider.remote.path().join("remote.git"))
        .unwrap()
        .reference(
            &format!("refs/heads/{remote_branch}"),
            f.base_head.parse().unwrap(),
            true,
            "fixture",
        )
        .unwrap();
    id
}

async fn ship(provider: &ProviderFixture, target: Uuid, pr: Uuid) -> (StatusCode, Value) {
    let response = provider
        .app
        .authenticated_client(&provider.fixture.owner)
        .post_json(
            &format!("/api/v1/workspaces/{target}/pull-requests/{pr}/merge"),
            &json!({}),
        )
        .await;
    (response.status(), response.json().await.unwrap())
}

fn restarted_service(provider: &ProviderFixture) -> g5_api::services::workspace_pull_request_provider_lifecycle::WorkspacePullRequestProviderLifecycleService{
    g5_api::services::workspace_pull_request_provider_lifecycle::WorkspacePullRequestProviderLifecycleService::new(
        provider.app.state.system_pool.clone(),provider.app.repo_storage.clone(),provider.github.uri(),
        "https://github.com".into(),"http://localhost".into(),provider.app.state.repository_credentials.clone(),
        Arc::new(g5_api::services::approval_config_service::ApprovalMergeAdmission::default()),
    )
}

async fn refuse_reauthorization(provider: &ProviderFixture) {
    use g5_contracts::error::G5Error;
    let app = &provider.app;
    let f = &provider.fixture;
    let result = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .merge_provider_pull_requests(
            app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
            f.pull_request_id,
            &[
                g5_api::services::coordinated_repo_publication::ProviderMergeTargetHead {
                    repo_id: f.source.repo_id,
                    head_commit: f.base_head.clone(),
                },
            ],
            f.owner.id,
            || async {
                Err(G5Error::Forbidden {
                    missing_scopes: Vec::new(),
                })
            },
        )
        .await;
    assert!(matches!(result, Err(G5Error::Forbidden { .. })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_refusal_releases_the_target_and_a_rolled_back_acceptance_recovers_its_checked_target(
) {
    if isolated_provider_test("provider_merge_targets::a_fresh_refusal_releases_the_target_and_a_rolled_back_acceptance_recovers_its_checked_target") {return;}
    let provider = ProviderFixture::create(true, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    let count = Arc::new(AtomicUsize::new(0));
    let (calls, state, remote, source) = (
        count.clone(),
        accepted.clone(),
        provider.remote.path().join("remote.git"),
        f.source_head.clone(),
    );
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(move |_: &wiremock::Request| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return ResponseTemplate::new(405)
                    .set_body_json(json!({"message":"Policy refused this invocation"}));
            }
            let result = accept(&remote, &source, "upstream");
            let sha = result.1.clone();
            *state.lock().unwrap() = Some(result);
            ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":sha}))
        })
        .expect(2)
        .mount(&provider.github)
        .await;
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let first:(Uuid,bool)=sqlx::query_as("SELECT merge_attempt_id,merge_pending FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert!(
        !first.1,
        "a known refusal of a fresh attempt releases the target"
    );
    refuse_reauthorization(&provider).await;
    let pending: bool = sqlx::query_scalar(
        "SELECT merge_pending FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        !pending,
        "failed reauthorization before any send releases a fresh reservation"
    );
    let moved = commit(app, f.source.repo_id, "main", "target-moved.txt").await;
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &provider.remote.path().join("remote.git"),
        &[
            "fetch",
            mirror.to_str().unwrap(),
            "refs/heads/main:refs/heads/upstream",
        ],
    );
    let targets = [
        g5_api::services::coordinated_repo_publication::ProviderMergeTargetHead {
            repo_id: f.source.repo_id,
            head_commit: moved.clone(),
        },
    ];
    let db = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .merge_provider_pull_requests(
            app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
            f.pull_request_id,
            &targets,
            f.owner.id,
            || async { Ok(app.scoped_db_for_user_org(f.owner.id, f.org.id).await) },
        )
        .await
        .unwrap();
    // The provider effect succeeded, but the transaction recording it is lost.
    db.rollback().await.unwrap();
    let pending:(Uuid,String,String,bool,bool)=sqlx::query_as("SELECT merge_attempt_id,target_head_sha,merge_target_head_sha,merge_pending,merge_accepted FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_ne!(pending.0, first.0);
    assert_eq!(
        pending.1, f.base_head,
        "publication target survived the rollback"
    );
    assert_eq!(
        pending.2, moved,
        "the newer checked target was already durable"
    );
    assert!(pending.3 && !pending.4);
    let restarted = restarted_service(&provider);
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    restarted
        .reconcile_provider_acceptance(&mut db, f.pull_request_id, f.owner.id)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let actual = accepted.lock().unwrap().clone().unwrap();
    assert_eq!(actual.0, moved);
    assert_eq!(
        body["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"],
        actual.0
    );
    assert_eq!(head(app, f.source.repo_id, "main").await, actual.1);
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "accepted retry imports without another provider merge"
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn losing_the_database_connection_fences_a_late_completion_after_recovery() {
    if isolated_provider_test("provider_merge_targets::losing_the_database_connection_fences_a_late_completion_after_recovery") {return;}
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    let seen = Arc::new(Notify::new());
    let (signal, state, remote, source) = (
        seen.clone(),
        accepted.clone(),
        provider.remote.path().join("remote.git"),
        f.source_head.clone(),
    );
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(move |_: &wiremock::Request| {
            let result = accept(&remote, &source, "upstream");
            let sha = result.1.clone();
            *state.lock().unwrap() = Some(result);
            signal.notify_one();
            ResponseTemplate::new(200)
                .set_body_json(json!({"merged":true,"sha":sha}))
                .set_delay(Duration::from_secs(5))
        })
        .expect(1)
        .mount(&provider.github)
        .await;
    let pid = std::sync::atomic::AtomicI32::new(0);
    let targets = [
        g5_api::services::coordinated_repo_publication::ProviderMergeTargetHead {
            repo_id: f.source.repo_id,
            head_commit: f.base_head.clone(),
        },
    ];
    let first = async {
        app.state
            .workspace_pull_request_provider_lifecycle_service
            .merge_provider_pull_requests(
                app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
                f.pull_request_id,
                &targets,
                f.owner.id,
                || async {
                    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
                    pid.store(
                        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
                            .fetch_one(db.conn())
                            .await
                            .unwrap(),
                        Ordering::SeqCst,
                    );
                    Ok(db)
                },
            )
            .await
    };
    let recovery = async {
        seen.notified().await;
        let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
            .bind(pid.load(Ordering::SeqCst))
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert!(terminated);
        let (status, body) = f.ship(app).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    };
    let (stale, body) = tokio::join!(first, recovery);
    assert!(
        stale.is_err(),
        "the disconnected invocation cannot commit its late response"
    );
    let actual = accepted.lock().unwrap().clone().unwrap();
    assert_eq!(
        body["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"],
        actual.0
    );
    let settled:(bool,bool,String)=sqlx::query_as("SELECT merge_pending,merge_accepted,target_head_sha FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(settled, (false, true, actual.0));
    assert_eq!(head(app, f.source.repo_id, "main").await, actual.1);
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_superseded_completion_cannot_overwrite_the_current_execution_status() {
    if isolated_provider_test("provider_merge_targets::a_superseded_completion_cannot_overwrite_the_current_execution_status") { return; }
    assert_superseded_completion_preserves_status(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_superseded_refusal_cannot_overwrite_the_current_execution_status() {
    if isolated_provider_test("provider_merge_targets::a_superseded_refusal_cannot_overwrite_the_current_execution_status") { return; }
    assert_superseded_completion_preserves_status(true).await;
}

async fn assert_superseded_completion_preserves_status(refused: bool) {
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    let seen = Arc::new(Notify::new());
    let (signal, state, remote, source) = (
        seen.clone(),
        accepted.clone(),
        provider.remote.path().join("remote.git"),
        f.source_head.clone(),
    );
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(move |_: &wiremock::Request| {
            let response = if refused {
                ResponseTemplate::new(405).set_body_json(json!({"message":"Merge refused"}))
            } else {
                let result = accept(&remote, &source, "upstream");
                let sha = result.1.clone();
                *state.lock().unwrap() = Some(result);
                ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":sha}))
            };
            signal.notify_one();
            response.set_delay(Duration::from_secs(5))
        })
        .expect(1)
        .mount(&provider.github)
        .await;
    let execution = Uuid::new_v4();
    let supersede = async {
        seen.notified().await;
        // Inject a superseding execution after dispatch, while the earlier
        // provider response is delayed. Exercise completion fencing even when
        // an independent writer has replaced the captured execution identity.
        let updated = tokio::time::timeout(
            Duration::from_secs(3),
            sqlx::query(
                "UPDATE workspace_pull_request_provider_repos
                 SET merge_execution_id=$2, provider_status='open', last_error='current execution pending'
                 WHERE pull_request_id=$1 AND merge_pending",
            )
            .bind(f.pull_request_id)
            .bind(execution)
            .execute(&app.db),
        )
        .await
        .expect("the provider row can be superseded before its response")
        .unwrap();
        assert_eq!(updated.rows_affected(), 1);
    };
    let ((status, body), ()) = tokio::join!(f.ship(app), supersede);
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let retained: (Uuid, String, Option<String>, bool, bool) = sqlx::query_as(
        "SELECT merge_execution_id,provider_status,last_error,merge_pending,merge_accepted
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        retained,
        (
            execution,
            "open".into(),
            Some("current execution pending".into()),
            true,
            false
        ),
        "a stale completion cannot report failure for the current execution"
    );
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    if refused {
        provider.github.verify().await;
        provider.github.reset().await;
        success(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    }
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        head(app, f.source.repo_id, "main").await,
        accepted.lock().unwrap().as_ref().unwrap().1
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unknown_merge_keeps_its_target_across_retry_and_late_provider_effect() {
    if isolated_provider_test("provider_merge_targets::an_unknown_merge_keeps_its_target_across_retry_and_late_provider_effect") { return; }
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let (competing, _, competing_source) =
        source_pr(&provider, "competitor", f.base_id, "upstream", 22).await;
    let independent_target = other_target(&provider, "alternate-target", "alternate").await;
    let shared_target = other_target(&provider, "shared-target", "upstream").await;
    let (shared, _, shared_source) = source_pr(
        &provider,
        "shared-competitor",
        shared_target,
        "upstream",
        24,
    )
    .await;
    let (independent, _, independent_source) = source_pr(
        &provider,
        "independent",
        independent_target,
        "alternate",
        23,
    )
    .await;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    let released = Arc::new(Notify::new());
    let completed = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let source = f.source_head.clone();
    let remote = provider.remote.path().join("remote.git");
    let (release, done, state, count) = (
        released.clone(),
        completed.clone(),
        accepted.clone(),
        calls.clone(),
    );
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(move |_: &wiremock::Request| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                let (release, done, state, remote, source) = (
                    release.clone(),
                    done.clone(),
                    state.clone(),
                    remote.clone(),
                    source.clone(),
                );
                tokio::spawn(async move {
                    release.notified().await;
                    *state.lock().unwrap() = Some(accept(&remote, &source, "upstream"));
                    done.notify_one();
                });
                // The client loses the result before the provider effect occurs.
                ResponseTemplate::new(200).set_body_string("lost response")
            } else {
                ResponseTemplate::new(405)
                    .set_body_json(json!({"message":"This retry was refused"}))
            }
        })
        .expect(2)
        .mount(&provider.github)
        .await;
    let competing_acceptance: Acceptance = Arc::default();
    success(
        &provider,
        22,
        &competing_source,
        "upstream",
        competing_acceptance.clone(),
    )
    .await;
    let independent_acceptance: Acceptance = Arc::default();
    success(
        &provider,
        23,
        &independent_source,
        "alternate",
        independent_acceptance.clone(),
    )
    .await;

    pull(
        &provider,
        24,
        &shared_source,
        "upstream",
        Acceptance::default(),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/24/merge"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&provider.github)
        .await;
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    f.assert_unpublished(app).await;
    let original:(Uuid,Uuid,String,bool)=sqlx::query_as("SELECT merge_attempt_id,merge_execution_id,merge_target_head_sha,merge_pending FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(original.2, f.base_head);
    assert!(original.3);
    assert!(
        accepted.lock().unwrap().is_none(),
        "provider effect is still in flight"
    );
    let (status, body) = ship(&provider, f.base_id, competing).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("Another provider merge"),
        "the shared target reservation must block this competitor: {body}"
    );
    assert!(competing_acceptance.lock().unwrap().is_none());
    let (status, body) = ship(&provider, shared_target, shared).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("Another provider merge"),
        "another G5 workspace tracking the same remote target must share exclusion: {body}"
    );
    assert!(
        !body.to_string().contains(&f.pull_request_id.to_string()),
        "target contention must not expose another workspace's PR identity: {body}"
    );
    let (status, body) = ship(&provider, independent_target, independent).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(independent_acceptance.lock().unwrap().is_some());

    refuse_reauthorization(&provider).await;
    let pending: bool = sqlx::query_scalar(
        "SELECT merge_pending FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        pending,
        "failed reauthorization cannot release an earlier unknown invocation"
    );
    // A definite refusal of a later invocation cannot settle the first one.
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let resumed:(Uuid,Uuid,bool,bool)=sqlx::query_as("SELECT merge_attempt_id,merge_execution_id,merge_pending,merge_uncertain FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(resumed.0, original.0);
    assert_ne!(resumed.1, original.1);
    assert!(resumed.2 && resumed.3);
    for statement in ["DELETE FROM workspace_pull_requests WHERE id=$1",
                      "UPDATE workspace_pull_request_provider_repos SET source_head_sha=repeat('a',40) WHERE pull_request_id=$1"] {
        assert!(sqlx::query(statement).bind(f.pull_request_id).execute(&app.db).await.is_err());
    }
    // Bootstrap's system role may update repositories without directly reading
    // provider PRs. The narrow trigger must still see and protect this attempt.
    let can_read: bool = sqlx::query_scalar(
        "SELECT has_any_column_privilege(current_user, 'public.workspace_pull_request_provider_repos', 'SELECT')",
    )
    .fetch_one(&app.system_pool)
    .await
    .unwrap();
    assert!(!can_read, "system callers do not gain provider PR access");
    let capability = app
        .state
        .authz
        .mint_db_capability(f.owner.id.into(), &[f.org.id.into()], &["*".to_string()])
        .await
        .unwrap();
    let mut system = g5_authz::ScopedDb::from_capability(&app.system_pool, capability)
        .await
        .unwrap();
    let error = sqlx::query("UPDATE repos SET provider_full_name='acme/replacement' WHERE id=$1")
        .bind(f.source.repo_id)
        .execute(system.conn())
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514"),
        "the guard must refuse pending identity changes even for the system role: {error}"
    );
    system.rollback().await.unwrap();
    // Production refresh refuses instead of changing the source evidence.
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    let result = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .ensure_automatic_refresh_allowed(&mut db, f.pull_request_id, f.owner.id)
        .await;
    assert!(result.is_err());
    db.rollback().await.unwrap();

    let admission: Value =
        sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let advanced = commit(app, f.source.repo_id, &f.source.name, "after-unknown.txt").await;
    assert_ne!(advanced, f.source_head);
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let retained: (bool, String, Value) = sqlx::query_as(
        "SELECT member.merge_pending,member.source_head_sha,pr.merge_admission
         FROM workspace_pull_request_provider_repos member
         JOIN workspace_pull_requests pr ON pr.id=member.pull_request_id WHERE pr.id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(retained, (true, f.source_head.clone(), admission.clone()));

    released.notify_one();
    completed.notified().await;
    let actual = accepted.lock().unwrap().clone().unwrap();
    assert_eq!(actual.0, f.base_head);
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let settled: (bool, bool, String, String) = sqlx::query_as(
        "SELECT merge_pending,merge_accepted,merge_target_head_sha,source_head_sha
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        settled,
        (false, true, actual.0.clone(), f.source_head.clone())
    );
    let retained_admission: Value =
        sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(retained_admission, admission);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    assert_eq!(head(app, f.source.repo_id, &f.source.name).await, advanced);

    // Continue the original import schedule only after this fixture explicitly
    // restores H1. The route itself neither rewinds H2 nor admits it as H1.
    let mirror = app.repo_path(f.source.repo_id).await;
    git(
        &mirror,
        &[
            "update-ref",
            &format!("refs/heads/{}", f.source.name),
            &f.source_head,
            &advanced,
        ],
    );
    GitRepo::open(&mirror)
        .unwrap()
        .checkout_branch_force(&f.source.name)
        .unwrap();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"],
        actual.0
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "reconciliation did not send another merge"
    );

    // A stale prepared target cannot be admitted after the first merge lands.
    let result = app
        .state
        .workspace_pull_request_provider_lifecycle_service
        .merge_provider_pull_requests(
            app.scoped_db_for_user_org(f.owner.id, f.org.id).await,
            competing,
            &[
                g5_api::services::coordinated_repo_publication::ProviderMergeTargetHead {
                    repo_id: f.source.repo_id,
                    head_commit: f.base_head.clone(),
                },
            ],
            f.owner.id,
            || async { Ok(app.scoped_db_for_user_org(f.owner.id, f.org.id).await) },
        )
        .await;
    assert!(result.is_err());
    assert!(competing_acceptance.lock().unwrap().is_none());
    let (status, body) = ship(&provider, f.base_id, competing).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let second = competing_acceptance.lock().unwrap().clone().unwrap();
    assert_eq!(second.0, actual.1);
    assert_eq!(
        body["data"]["pullRequest"]["providerRepos"][0]["mergeTargetHeadSha"],
        second.0
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

async fn passing_provider_checks(provider: &ProviderFixture) {
    let f = &provider.fixture;
    crate::api_pull_request_reads::revision_rows(&provider.github,&f.source_head,
        json!([{"id":7,"name":"tests","head_sha":f.source_head,"status":"completed","conclusion":"success"}]),json!([])).await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/branches/upstream"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name":"upstream","protection":{"enabled":false}})),
        )
        .mount(&provider.github)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/rules/branches/upstream"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&provider.github)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_uses_target_reservation_and_imports_provider_history() {
    if isolated_provider_test("provider_merge_targets::merge_when_ready_uses_target_reservation_and_imports_provider_history") {return;}
    let provider = ProviderFixture::create_with_app(false, false, true).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    success(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    passing_provider_checks(&provider).await;
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(g5_api::auto_merge::run_once(&app.state).await.unwrap(), 0);
    sqlx::query("UPDATE workspace_pull_requests SET auto_merge_observed_since=now()-interval '3 minutes' WHERE id=$1")
        .bind(f.pull_request_id).execute(&app.db).await.unwrap();
    let completed = g5_api::auto_merge::run_once(&app.state).await.unwrap();
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status,auto_merge_reason FROM workspace_pull_requests WHERE id=$1")
            .bind(f.pull_request_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(completed, 1, "{row:?}");
    assert_eq!(row.0, "merged");
    let actual = accepted.lock().unwrap().clone().unwrap();
    let recorded:(bool,bool,String)=sqlx::query_as("SELECT merge_pending,merge_accepted,merge_target_head_sha FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(recorded, (false, true, actual.0));
    assert_eq!(head(app, f.source.repo_id, "main").await, actual.1);
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_when_ready_recovers_the_recorded_cut_after_approval_withdrawal() {
    if isolated_provider_test("provider_merge_targets::merge_when_ready_recovers_the_recorded_cut_after_approval_withdrawal") { return; }
    let provider = ProviderFixture::create_with_app(false, false, true).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_string("lost response"))
        .expect(1)
        .mount(&provider.github)
        .await;
    passing_provider_checks(&provider).await;
    let (requirement, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Automatic recovery sign-off",
        &["ie"],
    )
    .await;
    decide(app, &f.owner, &reviewer, f.source.id, &requirement, "ie").await;
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/auto-merge",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(g5_api::auto_merge::run_once(&app.state).await.unwrap(), 0);
    sqlx::query("UPDATE workspace_pull_requests SET auto_merge_observed_since=now()-interval '3 minutes' WHERE id=$1")
        .bind(f.pull_request_id).execute(&app.db).await.unwrap();
    assert_eq!(g5_api::auto_merge::run_once(&app.state).await.unwrap(), 0);
    let before: (Value, Uuid, bool) = sqlx::query_as(
        "SELECT pr.merge_admission,p.merge_attempt_id,p.merge_pending
         FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id
         WHERE pr.id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(before.0["admitted"], true);
    assert!(before.2, "the first dispatch retains its unknown outcome");
    let denied = decide_outcome(
        app,
        &f.owner,
        &reviewer,
        f.source.id,
        &requirement,
        "ie",
        "rejected",
    )
    .await;
    assert_eq!(
        denied["data"]["outgoing"]["mergeAdmission"]["admitted"],
        false
    );
    provider.github.verify().await;
    provider.github.reset().await;
    success(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    passing_provider_checks(&provider).await;
    sqlx::query("UPDATE workspace_pull_requests SET auto_merge_attempted_at=now()-interval '1 day' WHERE id=$1")
        .bind(f.pull_request_id).execute(&app.db).await.unwrap();
    assert_eq!(g5_api::auto_merge::run_once(&app.state).await.unwrap(), 1);
    let after: (Value, Uuid, bool, String, String) = sqlx::query_as(
        "SELECT pr.merge_admission,p.merge_attempt_id,p.merge_pending,p.merge_target_head_sha,pr.status
         FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id
         WHERE pr.id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(after.0, before.0, "recovery retains its original admission");
    assert_eq!(
        after.1, before.1,
        "recovery resumes the same provider attempt"
    );
    assert!(!after.2);
    assert_eq!(after.3, f.base_head);
    assert_eq!(after.4, "merged");
    assert_eq!(
        head(app, f.source.repo_id, "main").await,
        accepted.lock().unwrap().as_ref().unwrap().1
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_local_pr_retains_the_attempt_until_provider_reconciliation() {
    if isolated_provider_test("provider_merge_targets::closing_a_local_pr_retains_the_attempt_until_provider_reconciliation") { return; }
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let accepted: Acceptance = Arc::default();
    pull(&provider, 21, &f.source_head, "upstream", accepted.clone()).await;
    Mock::given(method("PUT"))
        .and(path("/repos/acme/api/pulls/21/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_string("lost response"))
        .expect(1)
        .mount(&provider.github)
        .await;
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let response = post_pr(
        app,
        &f.owner.jwt,
        &format!(
            "/api/v1/workspaces/{}/pull-requests/{}/close",
            f.base_id, f.pull_request_id
        ),
        &json!({}),
    )
    .await;
    let status = response.status();
    let closed: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{closed}");
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", f.source.id),
            &json!({"force":true}),
        )
        .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("provider"),
        "deletion must refuse before deleting Git refs: {body}"
    );
    assert_eq!(head(app, f.source.repo_id, "feature").await, f.source_head);
    let restarted = restarted_service(&provider);
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    restarted
        .reconcile_provider_acceptance(&mut db, f.pull_request_id, f.owner.id)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let pending: bool = sqlx::query_scalar(
        "SELECT merge_pending FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        pending,
        "local close and an open provider read cannot release an unknown effect"
    );
    let actual = accept(
        &provider.remote.path().join("remote.git"),
        &f.source_head,
        "upstream",
    );
    *accepted.lock().unwrap() = Some(actual.clone());
    // The real retry reconciles the provider even though local closure refuses
    // landing; resolving target ownership must not require reopening this PR.
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let recovered: (String, bool, bool, String) = sqlx::query_as(
        "SELECT pr.status,p.merge_pending,p.merge_accepted,p.merge_target_head_sha FROM workspace_pull_requests pr JOIN workspace_pull_request_provider_repos p ON p.pull_request_id=pr.id WHERE pr.id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(recovered, ("closed".into(), false, true, actual.0.clone()));
    assert_eq!(
        head(app, f.source.repo_id, "main").await,
        f.base_head,
        "reconciliation never grants local landing authority to a closed PR"
    );
    let response = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", f.source.id),
            &json!({"operationId":Uuid::new_v4(),"force":true}),
        )
        .await;
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let retained: (bool, bool, String) = sqlx::query_as(
        "SELECT merge_pending,merge_accepted,merge_target_head_sha
         FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1",
    )
    .bind(f.pull_request_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        retained,
        (false, true, actual.0),
        "terminal acceptance permits cleanup and retains the provider audit"
    );
    provider.github.verify().await;
    provider.app.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_real_partial_batch_retains_each_target_and_its_admission_through_recovery() {
    if isolated_provider_test("provider_merge_targets::a_real_partial_batch_retains_each_target_and_its_admission_through_recovery") { return; }
    let provider = ProviderFixture::create(false, false).await;
    provider.github.reset().await;
    let app = &provider.app;
    let f = &provider.fixture;
    let other_base = head(app, f.other_repo_id, "main").await;
    let other_source = commit(app, f.other_repo_id, "feature", "second-provider.txt").await;
    let other_mirror = app.repo_path(f.other_repo_id).await;
    let other_remote = tempfile::tempdir().unwrap();
    git(
        other_remote.path(),
        &[
            "clone",
            "--bare",
            other_mirror.to_str().unwrap(),
            "remote.git",
        ],
    );
    let other_path = other_remote.path().join("remote.git");
    git2::Repository::open_bare(&other_path)
        .unwrap()
        .reference(
            "refs/heads/upstream",
            other_base.parse().unwrap(),
            true,
            "fixture",
        )
        .unwrap();
    let other_url = format!("https://github.com/g5-other-{}/other.git", f.org.id);
    g5_scm::native::redirect_transport_for_tests(&other_url, &other_path);
    sqlx::query(
        "UPDATE repos SET git_url=$2,provider='github',provider_full_name='acme/other' WHERE id=$1",
    )
    .bind(f.other_repo_id)
    .bind(other_url)
    .execute(&app.db)
    .await
    .unwrap();
    for (workspace, branch) in [(f.source.id, "feature"), (f.base_id, "upstream")] {
        sqlx::query("INSERT INTO remote_integrations(org_id,workspace_id,repo_id,remote_branch) VALUES($1,$2,$3,$4)")
            .bind(f.org.id).bind(workspace).bind(f.other_repo_id).bind(branch).execute(&app.db).await.unwrap();
    }
    GitRepo::open(&other_mirror)
        .unwrap()
        .write_baseline_ref("main", &other_base)
        .unwrap();
    sqlx::query("INSERT INTO workspace_pull_request_provider_repos(org_id,pull_request_id,repo_id,provider,provider_repo_full_name,source_remote_branch,target_remote_branch,provider_pr_number,provider_pr_url,provider_status,source_head_sha,target_head_sha) VALUES($1,$2,$3,'github','acme/other','feature','upstream',24,'https://github.com/acme/other/pull/24','open',$4,$5)")
        .bind(f.org.id).bind(f.pull_request_id).bind(f.other_repo_id).bind(&other_source).bind(&other_base).execute(&app.db).await.unwrap();
    let refreshed = app
        .authenticated_client(&f.owner)
        .post_json(
            &format!(
                "/api/v1/workspaces/{}/pull-requests/{}/refresh",
                f.base_id, f.pull_request_id
            ),
            &json!({}),
        )
        .await;
    let status = refreshed.status();
    let body: Value = refreshed.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    let (requirement, reviewer) = attach_phase_requirement(
        app,
        &f.owner,
        f.org.id,
        f.base_id,
        "Partial batch sign-off",
        &["ie"],
    )
    .await;
    decide(app, &f.owner, &reviewer, f.source.id, &requirement, "ie").await;
    let calls = Arc::new(AtomicUsize::new(0));
    let members = [
        (
            f.source.repo_id,
            "acme/api",
            21,
            f.source_head.clone(),
            f.base_head.clone(),
            provider.remote.path().join("remote.git"),
            Acceptance::default(),
        ),
        (
            f.other_repo_id,
            "acme/other",
            24,
            other_source,
            other_base.clone(),
            other_path,
            Acceptance::default(),
        ),
    ];
    for (_, name, number, source, base, remote, state) in &members {
        let (pull_name, number, pull_source, pull_base, observed) = (
            name.to_string(),
            *number,
            source.clone(),
            base.clone(),
            state.clone(),
        );
        Mock::given(method("GET")).and(path(format!("/repos/{name}/pulls/{number}")))
            .respond_with(move |_: &wiremock::Request| {
                let accepted = observed.lock().unwrap();
                ResponseTemplate::new(200).set_body_json(json!({"number":number,
                        "html_url":format!("https://github.com/{pull_name}/pull/{number}"),
                    "state":if accepted.is_some(){"closed"}else{"open"},"merged":accepted.is_some(),"mergeable":false,
                        "head":{"sha":pull_source},"base":{"ref":"upstream","sha":pull_base},
                    "merge_commit_sha":accepted.as_ref().map(|(_,sha)|sha)}))
            }).mount(&provider.github).await;
        let (source, remote, state, count) =
            (source.clone(), remote.clone(), state.clone(), calls.clone());
        Mock::given(method("PUT"))
            .and(path(format!("/repos/{name}/pulls/{number}/merge")))
            .respond_with(move |request: &wiremock::Request| {
                assert_eq!(request.body_json::<Value>().unwrap()["sha"], source);
                // Independent of repository iteration order, one real provider
                // accepts and the other loses its response before its effect.
                if count.fetch_add(1, Ordering::SeqCst) == 1 {
                    return ResponseTemplate::new(200).set_body_string("lost response");
                }
                let mut accepted = state.lock().unwrap();
                assert!(accepted.is_none(), "a provider PR merges at most once");
                let result = accept(&remote, &source, "upstream");
                let sha = result.1.clone();
                *accepted = Some(result);
                ResponseTemplate::new(200).set_body_json(json!({"merged":true,"sha":sha}))
            })
            .mount(&provider.github)
            .await;
    }
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(head(app, f.source.repo_id, "main").await, f.base_head);
    assert_eq!(
        head(app, f.other_repo_id, "main").await,
        other_base,
        "a partial batch advances no local target"
    );
    let pending: (i64, i64) = sqlx::query_as("SELECT count(*) FILTER (WHERE merge_pending),count(*) FILTER (WHERE merge_accepted) FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1")
        .bind(f.pull_request_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(pending, (1, 1));
    for (repo, _, _, _, base, _, state) in &members {
        let recorded: (String, bool, bool) = sqlx::query_as("SELECT merge_target_head_sha,merge_pending,merge_accepted FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
            .bind(f.pull_request_id).bind(repo).fetch_one(&app.db).await.unwrap();
        let accepted = state.lock().unwrap().is_some();
        assert_eq!(recorded, (base.clone(), !accepted, accepted));
    }
    let denied = decide_outcome(
        app,
        &f.owner,
        &reviewer,
        f.source.id,
        &requirement,
        "ie",
        "rejected",
    )
    .await;
    assert_eq!(
        denied["data"]["outgoing"]["mergeAdmission"]["admitted"],
        false
    );
    // Reconstruct the lifecycle over the same database before resuming the
    // unknown member; no live caller or session lock is recovery authority.
    let restarted = restarted_service(&provider);
    let mut db = app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    restarted
        .reconcile_provider_acceptance(&mut db, f.pull_request_id, f.owner.id)
        .await
        .unwrap();
    db.commit().await.unwrap();
    let (status, body) = f.ship(app).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "the accepted member is never merged twice"
    );
    assert_eq!(body["data"]["pullRequest"]["status"], "merged");
    assert_eq!(body["data"]["run"]["approvalAdmission"]["admitted"], true);
    for (repo, _, _, _, _, _, state) in &members {
        let actual = state.lock().unwrap().clone().unwrap();
        assert_eq!(head(app, *repo, "main").await, actual.1);
        let retained: (String, bool, bool) = sqlx::query_as("SELECT merge_target_head_sha,merge_pending,merge_accepted FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND repo_id=$2")
            .bind(f.pull_request_id).bind(repo).fetch_one(&app.db).await.unwrap();
        assert_eq!(retained, (actual.0, false, true));
    }
    provider.app.cleanup().await;
}
