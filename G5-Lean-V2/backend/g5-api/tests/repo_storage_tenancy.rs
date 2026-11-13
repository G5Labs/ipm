//! Repository storage tenancy (`g5.repo.storage-tenancy`): every repository
//! lives under its organization's directory, the store names a repository only
//! for an organization the caller is scoped to, and the Git interface refuses
//! another organization's repository with every credential type.

use crate::helpers;
use g5_api::ws::host_ctl::ControlPlaneHostState;
use g5_api_core::services::git::repo_store::TenantRepoStorage;
use g5_api_core::services::workspace_reader::CommittedRepoReadView;
use g5_authz::{DbCapability, ScopedDb};
use g5_contracts::error::G5Error;
use g5_contracts::types::ids::{SessionId, UserId};
use g5_contracts::types::{HistoricalGitVersion, HistoricalReadReason};
use g5_internal_api::host_ctl::HostHeartbeat;
use helpers::{
    TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer, TEST_PRIVATE_KEY_PEM,
};
use reqwest::StatusCode;
use uuid::Uuid;

struct Tenant {
    owner: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
}

async fn tenant(app: &TestApp) -> Tenant {
    let mut owner = TestUser::create(app).await;
    let org = TestOrg::create_with_admin(app, &mut owner).await;
    let workspace = TestWorkspace::create(app, &org).await;
    Tenant {
        owner,
        org,
        workspace,
    }
}

#[tokio::test]
async fn repositories_live_under_their_organization_and_keys_refuse_other_scopes() {
    let app = TestApp::spawn().await;
    let a = tenant(&app).await;
    let b = tenant(&app).await;

    // The fixture's repository is materialized at `<base>/<org>/<repo>`.
    let path = app.repo_path(a.workspace.repo_id).await;
    assert!(path.join("HEAD").exists() || path.join(".git").exists());
    assert!(path.ends_with(format!("{}/{}", a.org.id, a.workspace.repo_id)));

    // A tenant transaction keys only its own organization.
    let db = app.scoped_db_for_org(a.org.id).await;
    assert!(TenantRepoStorage::key(&db, a.org.id, a.workspace.repo_id).is_ok());
    assert!(matches!(
        TenantRepoStorage::key(&db, b.org.id, b.workspace.repo_id),
        Err(G5Error::NotFound { .. })
    ));
    assert!(matches!(
        app.repo_storage
            .repo_path(&db, b.org.id, b.workspace.repo_id),
        Err(G5Error::NotFound { .. })
    ));

    // An in-scope organization paired with another tenant's repository names
    // `<a>/<repo-of-b>`, which does not exist: no read reaches B's history.
    let crossed = app
        .repo_storage
        .repo_path(&db, a.org.id, b.workspace.repo_id)
        .expect("an in-scope organization is keyed");
    assert!(!crossed.exists());
    let view = CommittedRepoReadView::from_historical(
        &db,
        a.org.id,
        &HistoricalGitVersion {
            repo_id: b.workspace.repo_id,
            head_commit: b.workspace.head_commit.clone(),
            reason: HistoricalReadReason::OperationValidation,
        },
    )
    .expect("the version is well formed");
    assert!(view
        .read_file(&app.repo_storage, "README.md")
        .await
        .is_err());
    db.rollback().await.unwrap();

    // An unscoped handle covers no organization.
    let unscoped = ScopedDb::unscoped(
        &app.db,
        DbCapability {
            org_ids: vec![],
            actor_id: UserId::from_uuid(a.owner.id),
            scopes: vec![],
            session_id: SessionId::new(),
            ttl_secs: 60,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        TenantRepoStorage::key(&unscoped, a.org.id, a.workspace.repo_id),
        Err(G5Error::NotFound { .. })
    ));
    unscoped.rollback().await.unwrap();

    app.cleanup().await;
}

async fn advertise(app: &TestApp, repo_id: Uuid, auth: Credential<'_>) -> (StatusCode, String) {
    let request = app.client.get(format!(
        "{}/git/{repo_id}/info/refs?service=git-upload-pack",
        app.base_url
    ));
    let request = match auth {
        Credential::Bearer(token) => request.bearer_auth(token),
        Credential::Basic(token) => request.basic_auth("x-access-token", Some(token)),
    };
    let response = request.send().await.expect("advertisement request");
    let status = response.status();
    (status, response.text().await.unwrap_or_default())
}

enum Credential<'a> {
    Bearer(&'a str),
    Basic(&'a str),
}

fn refused(case: &str, own: (StatusCode, String), other: (StatusCode, String)) {
    assert_eq!(
        own.0,
        StatusCode::OK,
        "{case}: its own repository: {}",
        own.1
    );
    assert!(
        matches!(other.0, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "{case}: another organization's repository must be refused: {} {}",
        other.0,
        other.1
    );
    assert!(
        !other.1.contains("refs/heads/"),
        "{case}: no ref of another organization is advertised"
    );
}

#[tokio::test]
async fn git_refuses_another_organizations_repository_with_every_credential() {
    let app = TestApp::spawn().await;
    let a = tenant(&app).await;
    let b = tenant(&app).await;

    // A user who belongs only to A.
    refused(
        "user",
        advertise(&app, a.workspace.repo_id, Credential::Bearer(&a.owner.jwt)).await,
        advertise(&app, b.workspace.repo_id, Credential::Bearer(&a.owner.jwt)).await,
    );
    // A user's request for a repository RLS cannot see reads as missing.
    assert_eq!(
        advertise(&app, b.workspace.repo_id, Credential::Bearer(&a.owner.jwt))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // A's current workspace incarnation.
    let runtime = TestWorkspaceRuntimePeer::attach(&app, &a.workspace, a.owner.id).await;
    let incarnation = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
        .expect("construct incarnation minter")
        .mint_workspace_incarnation(
            a.org.id,
            a.workspace.id,
            a.owner.id,
            runtime.vm_id(),
            1,
            3600,
        )
        .expect("mint incarnation token");
    refused(
        "workspace incarnation",
        advertise(&app, a.workspace.repo_id, Credential::Basic(&incarnation)).await,
        advertise(&app, b.workspace.repo_id, Credential::Basic(&incarnation)).await,
    );

    // A host-source grant for A's Base.
    let base = TestWorkspace::create_base(&app, &a.org).await;
    sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id=$1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    let host = Uuid::new_v4();
    let image = format!("example.invalid/environment@sha256:{}", "a".repeat(64));
    sqlx::query(
        "INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,created_by_user_id,base_image_ref)
         VALUES($1,$2,'us-central1','ready',now(),$3,$4)",
    )
    .bind(host)
    .bind(a.org.id)
    .bind(a.owner.id)
    .bind(&image)
    .execute(&app.db)
    .await
    .unwrap();
    let grant = app
        .host_control_backend()
        .with_source_preparation_for_tests(app.repo_storage.clone(), app.base_url.clone())
        .heartbeat(
            host,
            &HostHeartbeat {
                refresh_source_preparation: true,
                slots_in_use: 0,
                boots_in_progress: 0,
                load: 0.,
                memory_free_mib: 0,
                disk_free_mib: 0,
                vms: vec![],
                retained_roots: vec![],
            },
        )
        .await
        .unwrap()
        .source_preparation
        .expect("Base prepares before the first VM");
    refused(
        "host source",
        advertise(&app, base.repo_id, Credential::Basic(&grant.read_token)).await,
        advertise(
            &app,
            b.workspace.repo_id,
            Credential::Basic(&grant.read_token),
        )
        .await,
    );

    app.cleanup().await;
}
