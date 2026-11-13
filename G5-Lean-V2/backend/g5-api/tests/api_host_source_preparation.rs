//! Real database and Git HTTP authorization for optional host repository seeds.
use crate::helpers;
use g5_api::ws::host_ctl::ControlPlaneHostState;
use g5_internal_api::host_ctl::HostHeartbeat;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use uuid::Uuid;

#[tokio::test]
async fn source_grant_is_read_only_scoped_live_and_membership_bound() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let base = TestWorkspace::create_base(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let host = Uuid::new_v4();
    let image = format!("example.invalid/environment@sha256:{}", "a".repeat(64));
    sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id=$1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,created_by_user_id,base_image_ref)
        VALUES($1,$2,'us-central1','ready',now(),$3,$4)")
        .bind(host).bind(org.id).bind(user.id).bind(&image).execute(&app.db).await.unwrap();
    let backend = app
        .host_control_backend()
        .with_source_preparation_for_tests(app.repo_storage.clone(), app.base_url.clone());
    let heartbeat = HostHeartbeat {
        refresh_source_preparation: true,
        slots_in_use: 0,
        boots_in_progress: 0,
        load: 0.,
        memory_free_mib: 0,
        disk_free_mib: 0,
        vms: vec![],
        retained_roots: vec![],
    };
    let grant = backend
        .heartbeat(host, &heartbeat)
        .await
        .unwrap()
        .source_preparation
        .expect("unique Base prepares before first VM");
    assert_eq!(grant.source_workspace_id, base.id);
    assert_eq!(grant.repositories.len(), 1);
    assert_eq!(grant.repositories[0].branch_oid, base.head_commit);
    let client = reqwest::Client::new();
    let read = |repo| {
        client
            .get(format!(
                "{}/git/{repo}/info/refs?service=git-upload-pack",
                app.base_url
            ))
            .basic_auth("x-access-token", Some(&grant.read_token))
    };
    assert_eq!(
        read(base.repo_id).send().await.unwrap().status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        read(other.repo_id).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let push = client
        .get(format!(
            "{}/git/{}/info/refs?service=git-receive-pack",
            app.base_url, base.repo_id
        ))
        .basic_auth("x-access-token", Some(&grant.read_token))
        .send()
        .await
        .unwrap();
    assert_eq!(push.status(), reqwest::StatusCode::FORBIDDEN);
    let ordinary_api = client
        .get(format!(
            "{}/api/v1/orgs/{}/worker-hosts",
            app.base_url, org.id
        ))
        .bearer_auth(&grant.read_token)
        .send()
        .await
        .unwrap();
    assert!(
        !ordinary_api.status().is_success(),
        "source token cannot become a user/host API credential"
    );
    sqlx::query("UPDATE workspace_repos SET root_path='changed' WHERE workspace_id=$1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        read(base.repo_id).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id=$1")
        .bind(base.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE worker_hosts SET last_heartbeat=now()-interval '31 seconds' WHERE host_id=$1",
    )
    .bind(host)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        read(base.repo_id).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    // A newer heartbeat restores host liveness, but another actor cannot inherit its grant.
    sqlx::query(
        "UPDATE worker_hosts SET last_heartbeat=now(), created_by_user_id=NULL WHERE host_id=$1",
    )
    .bind(host)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        read(base.repo_id).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert!(backend
        .heartbeat(host, &heartbeat)
        .await
        .unwrap()
        .source_preparation
        .is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn absent_base_uses_only_a_matching_admitted_boot_and_rechecks_custody() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspace_repos SET root_path='code' WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    let host = Uuid::new_v4();
    let image = format!("example.invalid/environment@sha256:{}", "a".repeat(64));
    sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,status,last_heartbeat,created_by_user_id,base_image_ref)
        VALUES($1,$2,'us-central1','ready',now(),$3,$4)")
        .bind(host).bind(org.id).bind(user.id).bind(&image).execute(&app.db).await.unwrap();
    let backend = app
        .host_control_backend()
        .with_source_preparation_for_tests(app.repo_storage.clone(), app.base_url.clone());
    let heartbeat = HostHeartbeat {
        refresh_source_preparation: true,
        slots_in_use: 0,
        boots_in_progress: 0,
        load: 0.,
        memory_free_mib: 0,
        disk_free_mib: 0,
        vms: vec![],
        retained_roots: vec![],
    };
    assert!(backend
        .heartbeat(host, &heartbeat)
        .await
        .unwrap()
        .source_preparation
        .is_none());
    // This database fixture represents the control plane's accepted boot, not a
    // host-supplied repository list. Starting is sufficient; Ready is not needed.
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,image_ref,boot_spec,source_manifest_digest)
        VALUES($1,$2,$3,$4,1,'starting',$5,'{}',$6)")
        .bind(org.id).bind(workspace.id).bind(host).bind(Uuid::new_v4().to_string()).bind(&image)
        .bind("b".repeat(64)).execute(&app.db).await.unwrap();
    let grant = backend
        .heartbeat(host, &heartbeat)
        .await
        .unwrap()
        .source_preparation
        .expect("admitted source fallback");
    assert_eq!(grant.source_workspace_id, workspace.id);
    let client = reqwest::Client::new();
    let read = || {
        client
            .get(format!(
                "{}/git/{}/info/refs?service=git-upload-pack",
                app.base_url, workspace.repo_id
            ))
            .basic_auth("x-access-token", Some(&grant.read_token))
    };
    assert_eq!(
        read().send().await.unwrap().status(),
        reqwest::StatusCode::OK
    );
    sqlx::query("UPDATE workspace_vms SET image_ref='other-image' WHERE workspace_id=$1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        read().send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert!(backend
        .heartbeat(host, &heartbeat)
        .await
        .unwrap()
        .source_preparation
        .is_none());
    app.cleanup().await;
}
