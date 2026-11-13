// Shared real HTTP/Git fixture for workspace review acceptance.
// Each integration-test binary uses a different subset of this fixture.
#![allow(dead_code)]
use crate::helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
pub async fn body(response: reqwest::Response, status: StatusCode) -> Value {
    let actual = response.status();
    let value: Value = response.json().await.expect("JSON response");
    assert_eq!(actual, status, "{value}");
    value["data"].clone()
}
/// Capture the explicit decision input through the production review read.
pub async fn heads(app: &TestApp, actor: &TestUser, workspace: Uuid) -> Value {
    body(
        app.authenticated_client(actor)
            .get(&format!("/api/v1/workspaces/{workspace}/review-flow"))
            .await,
        StatusCode::OK,
    )
    .await["heads"]
        .clone()
}
pub struct Flow {
    pub owner: TestUser,
    pub reviewer: TestUser,
    pub second: TestUser,
    pub visitor: TestUser,
    pub org: TestOrg,
    pub parent: TestWorkspace,
    pub source: Uuid,
    pub docs: Uuid,
    pub definition: String,
}
impl Flow {
    /// Publish through the same fenced multi-repository landing writer used by
    /// server producers. The candidate is an actual Git commit, not a fake SHA.
    pub async fn publish_file(
        &self,
        app: &TestApp,
        repo_id: Uuid,
        path: &str,
        bytes: &[u8],
    ) -> Value {
        self.publish_file_change(app, repo_id, path, Some(bytes))
            .await
    }

    pub async fn delete_file(&self, app: &TestApp, repo_id: Uuid, path: &str) -> Value {
        self.publish_file_change(app, repo_id, path, None).await
    }

    async fn publish_file_change(
        &self,
        app: &TestApp,
        repo_id: Uuid,
        path: &str,
        bytes: Option<&[u8]>,
    ) -> Value {
        use g5_api::services::branch_commit::prepare_branch_commit;
        use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput};
        let old = self.overview(app, &self.owner).await;
        let h = old["heads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["repoId"] == repo_id.to_string())
            .unwrap();
        let old_sha = h["headCommit"].as_str().unwrap();
        let storage_path = app.repo_path(repo_id).await;
        let repo = git2::Repository::open(storage_path).unwrap();
        let parent = repo
            .find_commit(git2::Oid::from_str(old_sha).unwrap())
            .unwrap();
        let mut index = git2::Index::new().unwrap();
        index.read_tree(&parent.tree().unwrap()).unwrap();
        if let Some(bytes) = bytes {
            let entry = git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: bytes.len() as u32,
                id: repo.blob(bytes).unwrap(),
                flags: 0,
                flags_extended: 0,
                path: path.as_bytes().to_vec(),
            };
            index.add(&entry).unwrap();
        } else {
            index.remove_path(std::path::Path::new(path)).unwrap();
        }
        let tree = repo.find_tree(index.write_tree_to(&repo).unwrap()).unwrap();
        let signature = git2::Signature::now("Review test", "review@g5.test").unwrap();
        let candidate = repo
            .commit(
                None,
                &signature,
                &signature,
                "Review publication",
                &tree,
                &[&parent],
            )
            .unwrap()
            .to_string();
        let inputs = old["heads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| WorkspaceHeadRepoInput {
                repo_id: Uuid::parse_str(h["repoId"].as_str().unwrap()).unwrap(),
                expected_old_head: h["headCommit"].as_str().unwrap().to_owned(),
                proposed_new_head: if h["repoId"] == repo_id.to_string() {
                    candidate.clone()
                } else {
                    h["headCommit"].as_str().unwrap().to_owned()
                },
            })
            .collect();
        let db = app.scoped_db_for_org(self.org.id).await;
        let scope = db.reopener();
        db.rollback().await.unwrap();
        let prepared = prepare_branch_commit(
            &scope,
            &app.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: self.source,
            },
            inputs,
            "review_acceptance",
        )
        .await
        .unwrap();
        prepared
            .land(
                app.scoped_db_for_org(self.org.id).await,
                &app.state.event_bus,
                &app.repo_storage,
            )
            .await
            .unwrap();
        self.overview(app, &self.owner).await
    }
    pub async fn new(app: &TestApp) -> Self {
        Self::with_parent_kind(app, false).await
    }

    pub async fn with_base_parent(app: &TestApp) -> Self {
        Self::with_parent_kind(app, true).await
    }

    async fn with_parent_kind(app: &TestApp, base: bool) -> Self {
        let mut owner = TestUser::create(app).await;
        let org = TestOrg::create_with_admin(app, &mut owner).await;
        let reader = async {
            TestUser::create_with_scopes(app, org.id, &[g5_auth::Scope::WorkspacesRead]).await
        };
        let reviewer = reader.await;
        let second =
            TestUser::create_with_scopes(app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
        let visitor =
            TestUser::create_with_scopes(app, org.id, &[g5_auth::Scope::WorkspacesRead]).await;
        let mut parent = if base {
            TestWorkspace::create_base(app, &org).await
        } else {
            TestWorkspace::create(app, &org).await
        };
        parent
            .commit_file(app, "review.txt", "before\nreview\n")
            .await;
        let docs = parent.add_docs_repo(app).await;
        let client = app.authenticated_client(&owner);
        let fork = body(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{}/clone", parent.id),
                    &json!({"name":"review-flow","idempotencyKey":Uuid::new_v4().to_string()}),
                )
                .await,
            StatusCode::CREATED,
        )
        .await;
        let source = Uuid::parse_str(fork["workspace"]["id"].as_str().unwrap()).unwrap();
        let definition=body(client.post_json(&format!("/api/v1/orgs/{}/approval-definitions",org.id),&json!({"name":"Security review","humanUserIds":[owner.id,reviewer.id,second.id,visitor.id],"phases":["be","se","ie"]})).await,StatusCode::CREATED).await["id"].as_str().unwrap().to_owned();
        body(
            client
                .post_json(
                    &format!("/api/v1/workspaces/{}/approvals/requirements", parent.id),
                    &json!({"definitionId":definition}),
                )
                .await,
            StatusCode::OK,
        )
        .await;
        Self {
            owner,
            reviewer,
            second,
            visitor,
            org,
            parent,
            source,
            docs,
            definition,
        }
    }
    pub fn path(&self, suffix: &str) -> String {
        format!("/api/v1/workspaces/{}/{}", self.source, suffix)
    }
    pub async fn overview(&self, app: &TestApp, actor: &TestUser) -> Value {
        body(
            app.authenticated_client(actor)
                .get(&self.path("review-flow"))
                .await,
            StatusCode::OK,
        )
        .await
    }
    pub async fn submit(&self, app: &TestApp) -> Value {
        body(app.authenticated_client(&self.owner).post_json(&self.path("pull-requests"),&json!({"targetWorkspaceId":self.parent.id,"description":"Implementation decisions"})).await,StatusCode::OK).await
    }
    pub async fn request(&self, app: &TestApp, phase: &str) -> Vec<String> {
        let response=app.authenticated_client(&self.owner).post_json(&self.path("approvals/requests"),&json!({"definitionId":self.definition,"definitionVersion":1,"phase":phase,"actorUserIds":[self.reviewer.id,self.second.id]})).await;
        let status = response.status();
        assert!(matches!(status, StatusCode::OK | StatusCode::CREATED));
        let value = body(response, status).await;
        assert_eq!(
            status == StatusCode::OK,
            value["requests"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["reused"] == true)
        );
        value["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["requestId"].as_str().unwrap().to_owned())
            .collect()
    }
}

impl Flow {
    pub async fn primary_request(&self, app: &TestApp, phase: &str) -> Value {
        self.request(app, phase).await;
        self.overview(app, &self.reviewer).await["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["actorUserId"] == self.reviewer.id.to_string() && r["state"] == "pending")
            .unwrap()
            .clone()
    }
    pub async fn decision(
        &self,
        app: &TestApp,
        actor: &TestUser,
        request: &Value,
        outcome: &str,
    ) -> reqwest::Response {
        let view = self.overview(app, actor).await;
        app.authenticated_client(actor).post_json(&self.path("approvals/decisions"),&json!({"definitionId":request["definitionId"],"requestId":request["id"],"phase":request["phase"],"outcome":outcome,"comment":"Billing scope confirmed.","reviewedGitHeads":view["heads"]})).await
    }
    pub async fn state(&self, app: &TestApp, actor: &TestUser) -> Value {
        body(
            app.authenticated_client(actor)
                .get(&self.path("approvals"))
                .await,
            StatusCode::OK,
        )
        .await
    }
}
