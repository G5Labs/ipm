//! The production application owns continuity; no SQL continuity trigger is installed.
use crate::helpers::{TestApp, TestDb, TestOrg, TestUser, TestWorkspace};
use g5_api::services::org_service::{
    MemberChange, MembershipRetirement, OrgService, RetiredPersonalWorkspace,
};
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use uuid::Uuid;

async fn scoped(app: &TestApp, pool: &sqlx::PgPool, org: Uuid, actor: Uuid) -> g5_authz::ScopedDb {
    let capability = app
        .state
        .authz
        .mint_db_capability(
            g5_contracts::types::ids::UserId::from_uuid(actor),
            &[g5_contracts::types::ids::OrgId::from_uuid(org)],
            &["org:admin".into()],
        )
        .await
        .unwrap();
    g5_authz::ScopedDb::from_capability(pool, capability)
        .await
        .unwrap()
}

/// Exercise actual retirement, optionally suspending after its successful SQL.
struct Retirement {
    suspend: bool,
    reached: Arc<Notify>,
}

#[async_trait::async_trait]
impl MembershipRetirement for Retirement {
    async fn retire_personal_workspace(
        &self,
        conn: &mut sqlx::PgConnection,
        org: Uuid,
        user: Uuid,
        actor: Uuid,
    ) -> Result<Option<RetiredPersonalWorkspace>, G5Error> {
        let retired =
            g5_api::services::personal_workspace::retire_in_tx(conn, org, user, actor).await?;
        assert!(retired.is_some(), "test reaches successful retirement SQL");
        self.reached.notify_one();
        if self.suspend {
            std::future::pending::<()>().await;
        }
        Ok(retired)
    }
}

async fn admins(app: &TestApp, org: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM org_memberships WHERE org_id=$1 AND role = 'admin' AND deactivated_at IS NULL")
        .bind(org).fetch_one(&app.db).await.unwrap()
}

async fn snapshot(app: &TestApp, org: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('generation', access_generation,
        'members', (SELECT jsonb_agg(to_jsonb(m) ORDER BY user_id) FROM org_memberships m WHERE org_id=o.id),
        'workspaces', (SELECT jsonb_agg(to_jsonb(w) ORDER BY id) FROM workspaces w WHERE org_id=o.id),
        'vms', (SELECT jsonb_agg(to_jsonb(v) ORDER BY workspace_id) FROM workspace_vms v WHERE org_id=o.id),
        'grants', (SELECT jsonb_agg(to_jsonb(g) ORDER BY id) FROM user_scope_grants g WHERE org_id=o.id))
        FROM orgs o WHERE id=$1")
        .bind(org).fetch_one(&app.db).await.unwrap()
}

#[tokio::test]
async fn only_active_admin_memberships_count_and_refusals_commit_no_changes() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    TestWorkspace::create_personal(&app, &org, admin.id).await;
    let inactive = TestUser::create_admin(&app, org.id).await;
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(inactive.id)
        .execute(&app.db)
        .await
        .unwrap();
    let delegate = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    delegate
        .grant_org_scope(&app, org.id, Scope::OrgAdmin)
        .await;
    let invitation = app
        .authenticated_client(&admin)
        .post_json(
            &format!("/api/v1/orgs/{}/members", org.id),
            &json!({"email":"pending-admin@example.test","role":"admin"}),
        )
        .await;
    assert!(invitation.status().is_success());
    sqlx::query(
        "UPDATE org_memberships SET deactivation_cleanup_id=$3 WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(admin.id)
    .bind(Uuid::new_v4())
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(
        admins(&app, org.id).await,
        1,
        "an execution hold is not deactivation"
    );
    let trigger_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_trigger WHERE tgname LIKE '%org_admin_continuity%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(trigger_count, 0);
    let before = snapshot(&app, org.id).await;
    // No retirement owner is configured: a continuity refusal must precede it.
    for change in [
        MemberChange::Role("member".into()),
        MemberChange::Role("viewer".into()),
        MemberChange::Remove,
    ] {
        let mut db = scoped(&app, &app.system_pool, org.id, delegate.id).await;
        let error = OrgService::new()
            .change_member(&mut db, org.id, delegate.id, admin.id, change)
            .await
            .unwrap_err();
        assert!(matches!(error, G5Error::Conflict { .. }), "{error}");
        assert!(error.to_string().contains("at least one active Admin"));
        db.commit().await.unwrap();
        assert_eq!(snapshot(&app, org.id).await, before);
    }
    // The retained self-action restriction also leaves activity/holds untouched.
    let mut db = scoped(&app, &app.system_pool, org.id, admin.id).await;
    let error = OrgService::new()
        .change_member(
            &mut db,
            org.id,
            admin.id,
            admin.id,
            MemberChange::Activity {
                active: false,
                expected_revision: 0,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, G5Error::Conflict { .. }));
    db.commit().await.unwrap();
    assert_eq!(snapshot(&app, org.id).await, before);
    app.cleanup().await;
}

#[tokio::test]
async fn retirement_error_and_cancellation_roll_back_even_if_the_caller_commits() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let target = TestUser::create_admin(&app, org.id).await;
    let personal = TestWorkspace::create_personal(&app, &org, target.id).await;
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,start_intent) VALUES($1,$2,$3)")
        .bind(org.id)
        .bind(personal.id)
        .bind(json!({"id":Uuid::new_v4(),"kind":"resume",
            "status":"queued_for_capacity","owner_user_id":target.id,"lifecycle_generation":0,
            "attempts":0,"created_at":chrono::Utc::now(),
            "expires_at":chrono::Utc::now()+chrono::TimeDelta::minutes(15)}))
        .execute(&app.db)
        .await
        .unwrap();
    let ordinary = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(ordinary.id)
        .bind(target.id)
        .execute(&app.db)
        .await
        .unwrap();
    let before = snapshot(&app, org.id).await;
    for suspend in [false, true] {
        let reached = Arc::new(Notify::new());
        let service = OrgService::new().with_membership_retirement(Arc::new(Retirement {
            suspend,
            reached: reached.clone(),
        }));
        let mut db = scoped(&app, &app.system_pool, org.id, admin.id).await;
        if suspend {
            tokio::time::timeout(Duration::from_secs(10), async {
                tokio::select! {
                    result = service.change_member(&mut db, org.id, admin.id, target.id, MemberChange::Remove) => {
                        panic!("retirement must suspend after its SQL: {result:?}");
                    }
                    () = reached.notified() => {}
                }
                // Dropping the in-flight change cancels it only after retirement SQL.
            })
            .await
            .expect("retirement reaches successful SQL before cancellation");
        } else {
            let error = service
                .change_member(&mut db, org.id, admin.id, target.id, MemberChange::Remove)
                .await
                .unwrap_err();
            assert!(matches!(error, G5Error::Conflict { .. }), "{error}");
            tokio::time::timeout(Duration::from_secs(10), reached.notified())
                .await
                .expect("retirement SQL precedes the refusal");
        }
        db.commit().await.unwrap();
        assert_eq!(snapshot(&app, org.id).await, before, "suspended={suspend}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn competing_mixed_reductions_use_fresh_authority_after_the_org_lock() {
    let app = TestApp::spawn().await;
    for isolation in ["READ COMMITTED", "REPEATABLE READ", "SERIALIZABLE"] {
        for second_change in [
            MemberChange::Role("viewer".into()),
            MemberChange::Remove,
            MemberChange::Activity {
                active: false,
                expected_revision: 0,
            },
        ] {
            let mut a = TestUser::create(&app).await;
            let org = TestOrg::create_with_admin(&app, &mut a).await;
            let b = TestUser::create_admin(&app, org.id).await;
            // Both capabilities and the second snapshot predate the winning change.
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .after_connect(move |conn, _| {
                    Box::pin(async move {
                        sqlx::query(&format!(
                            "SET default_transaction_isolation = '{isolation}'"
                        ))
                        .execute(conn)
                        .await?;
                        Ok(())
                    })
                })
                .connect_with(app.system_pool.connect_options().as_ref().clone())
                .await
                .unwrap();
            let mut first = scoped(&app, &app.system_pool, org.id, b.id).await;
            let mut second = scoped(&app, &pool, org.id, a.id).await;
            let actual: String = sqlx::query_scalar("SHOW transaction_isolation")
                .fetch_one(second.conn())
                .await
                .unwrap();
            assert_eq!(actual.to_uppercase(), isolation);
            sqlx::query("SELECT 1 FROM org_memberships WHERE org_id=$1")
                .bind(org.id)
                .execute(second.conn())
                .await
                .unwrap();
            app.state
                .org_service
                .change_member(
                    &mut first,
                    org.id,
                    b.id,
                    a.id,
                    MemberChange::Role("member".into()),
                )
                .await
                .unwrap();
            {
                let competing = app.state.org_service.change_member(
                    &mut second,
                    org.id,
                    a.id,
                    b.id,
                    second_change,
                );
                tokio::pin!(competing);
                assert!(
                    tokio::time::timeout(Duration::from_millis(50), &mut competing)
                        .await
                        .is_err()
                );
                first.commit().await.unwrap();
                let error = tokio::time::timeout(Duration::from_secs(5), &mut competing)
                    .await
                    .unwrap()
                    .unwrap_err();
                if isolation == "READ COMMITTED" {
                    assert!(matches!(error, G5Error::Forbidden { .. }), "{error}");
                } else {
                    assert!(
                        matches!(error, G5Error::DatabaseRetryable { .. }),
                        "{isolation}: {error}"
                    );
                }
            }
            second.rollback().await.unwrap();
            assert_eq!(admins(&app, org.id).await, 1);
            pool.close().await;
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn http_retries_whole_transactions_and_stops_after_three_failures() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let target = TestUser::create_admin(&app, org.id).await;
    TestWorkspace::create_personal(&app, &org, target.id).await;
    // A nontransactional sequence injects a transient storage failure AFTER
    // retirement SQL. This is fault injection, not a continuity constraint.
    sqlx::raw_sql("CREATE SEQUENCE retry_fault; GRANT USAGE, SELECT ON SEQUENCE retry_fault TO g5_app;
        CREATE FUNCTION inject_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF nextval('retry_fault') <= 1 THEN RAISE EXCEPTION 'injected serialization failure' USING ERRCODE='40001'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER retry_fault BEFORE UPDATE OF role ON org_memberships FOR EACH ROW EXECUTE FUNCTION inject_retry_fault();")
        .execute(&app.db).await.unwrap();
    let path = format!("/api/v1/orgs/{}/members/{}", org.id, target.id);
    let response = app
        .authenticated_client(&admin)
        .patch_json(&path, &json!({"role":"viewer"}))
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let attempts: i64 = sqlx::query_scalar("SELECT last_value FROM retry_fault")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(admins(&app, org.id).await, 1);
    sqlx::raw_sql("CREATE OR REPLACE FUNCTION inject_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN PERFORM nextval('retry_fault'); RAISE EXCEPTION 'persistent serialization failure' USING ERRCODE='40001'; END $$;
        ALTER SEQUENCE retry_fault RESTART;")
        .execute(&app.db).await.unwrap();
    let before = snapshot(&app, org.id).await;
    let response = app
        .authenticated_client(&admin)
        .patch_json(&path, &json!({"role":"member"}))
        .await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT last_value FROM retry_fault")
            .fetch_one(&app.db)
            .await
            .unwrap(),
        3
    );
    assert_eq!(snapshot(&app, org.id).await, before);
    app.cleanup().await;
}

#[tokio::test]
async fn waiting_reduction_counts_the_committed_set_even_when_actor_retains_an_additive_grant() {
    let app = TestApp::spawn().await;
    for change in [
        MemberChange::Role("member".into()),
        MemberChange::Role("viewer".into()),
        MemberChange::Remove,
    ] {
        let mut a = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut a).await;
        a.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
        let b = TestUser::create_admin(&app, org.id).await;
        TestWorkspace::create_personal(&app, &org, b.id).await;
        let mut first = scoped(&app, &app.system_pool, org.id, a.id).await;
        let mut second = scoped(&app, &app.system_pool, org.id, a.id).await;
        app.state
            .org_service
            .change_member(
                &mut first,
                org.id,
                a.id,
                a.id,
                MemberChange::Role("member".into()),
            )
            .await
            .unwrap();
        {
            let pending =
                app.state
                    .org_service
                    .change_member(&mut second, org.id, a.id, b.id, change);
            tokio::pin!(pending);
            assert!(
                tokio::time::timeout(Duration::from_millis(50), &mut pending)
                    .await
                    .is_err()
            );
            first.commit().await.unwrap();
            let error = pending.await.unwrap_err();
            assert!(matches!(error, G5Error::Conflict { .. }), "{error}");
            assert!(error.to_string().contains("at least one active Admin"));
        }
        let before = snapshot(&app, org.id).await;
        second.commit().await.unwrap();
        assert_eq!(snapshot(&app, org.id).await, before);
        assert_eq!(admins(&app, org.id).await, 1);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn http_retry_repeats_admission_after_the_actor_loses_authority() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let other = TestUser::create_admin(&app, org.id).await;
    let target = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    TestWorkspace::create_personal(&app, &org, target.id).await;
    let mut gate = app.db.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(11061111)")
        .execute(&mut *gate)
        .await
        .unwrap();
    sqlx::raw_sql(&format!(
        "CREATE SEQUENCE admission_retry;
        GRANT USAGE, SELECT ON SEQUENCE admission_retry TO g5_app;
        CREATE FUNCTION admission_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF NEW.user_id='{target}'::uuid AND nextval('admission_retry')=1 THEN
            PERFORM pg_advisory_xact_lock(11061111);
            RAISE EXCEPTION 'retry after competing authority change' USING ERRCODE='40001';
        END IF; RETURN NEW; END $$;
        CREATE TRIGGER admission_retry BEFORE UPDATE OF role ON org_memberships
        FOR EACH ROW EXECUTE FUNCTION admission_retry_fault();",
        target = target.id
    ))
    .execute(&app.db)
    .await
    .unwrap();
    let request = app
        .client
        .patch(format!(
            "{}/api/v1/orgs/{}/members/{}",
            app.base_url, org.id, target.id
        ))
        .bearer_auth(&admin.jwt)
        .json(&json!({"role":"viewer"}));
    let first = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !sqlx::query_scalar::<_, bool>("SELECT is_called FROM admission_retry")
            .fetch_one(&app.db)
            .await
            .unwrap()
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let request = app
        .client
        .patch(format!(
            "{}/api/v1/orgs/{}/members/{}",
            app.base_url, org.id, admin.id
        ))
        .bearer_auth(&other.jwt)
        .json(&json!({"role":"member"}));
    let competing = tokio::spawn(async move { request.send().await.unwrap() });
    // Observe the competing supported operation queued on the org row before
    // releasing the fault. Its commit precedes the route's next admission.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
            AND wait_event_type='Lock' AND query LIKE 'UPDATE orgs SET access_generation = access_generation%')")
            .fetch_one(&app.db).await.unwrap() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    sqlx::query("SELECT pg_advisory_unlock(11061111)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), competing)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let response = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT role FROM org_memberships WHERE org_id=$1 AND user_id=$2"
        )
        .bind(org.id)
        .bind(target.id)
        .fetch_one(&app.db)
        .await
        .unwrap(),
        "member"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM workspaces WHERE org_id=$1 AND personal_user_id=$2 AND deleted_at IS NULL")
        .bind(org.id).bind(target.id).fetch_one(&app.db).await.unwrap(), 1);
    drop(gate);
    app.cleanup().await;
}

#[tokio::test]
async fn operator_exception_whole_org_deletion_and_manual_sql_boundary_are_explicit() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    sqlx::query("UPDATE orgs SET kind='operator', slug='operator' WHERE id=$1")
        .bind(org.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = scoped(&app, &app.system_pool, org.id, admin.id).await;
    app.state
        .org_service
        .change_member(
            &mut db,
            org.id,
            admin.id,
            admin.id,
            MemberChange::Role("member".into()),
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    assert_eq!(admins(&app, org.id).await, 0);
    // An authorized whole-organization teardown remains legal, including cascades.
    let mut tenant_admin = TestUser::create(&app).await;
    let tenant = TestOrg::create_with_admin(&app, &mut tenant_admin).await;
    sqlx::query("DELETE FROM orgs WHERE id=$1")
        .bind(tenant.id)
        .execute(&app.db)
        .await
        .unwrap();
    // Privileged SQL is intentionally outside application policy enforcement.
    let tenant = TestOrg::create_with_admin(&app, &mut tenant_admin).await;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(tenant_admin.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(admins(&app, tenant.id).await, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn membership_transition_refuses_an_unconfigured_retirement_owner() {
    use g5_api::services::org_service::{MemberChange, OrgService};

    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let before = snapshot(&app, org.id).await;
    for change in [MemberChange::Remove, MemberChange::Role("viewer".into())] {
        let capability = app
            .state
            .authz
            .mint_db_capability(
                g5_contracts::types::ids::UserId::from_uuid(admin.id),
                &[g5_contracts::types::ids::OrgId::from_uuid(org.id)],
                &["org:admin".into()],
            )
            .await
            .unwrap();
        let mut db = g5_authz::ScopedDb::from_capability(&app.state.system_pool, capability)
            .await
            .unwrap();
        let error = OrgService::new()
            .change_member(&mut db, org.id, admin.id, member.id, change)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("retirement owner is not configured"),
            "{error}"
        );
        // Even a caller committing after the configuration error cannot
        // publish a partial membership transition.
        db.commit().await.unwrap();
        assert_eq!(snapshot(&app, org.id).await, before);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn http_refusal_rolls_back_personal_retirement_and_access_generation() {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let delegate = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    delegate
        .grant_org_scope(&app, org.id, Scope::OrgAdmin)
        .await;
    let response = app
        .authenticated_client(&admin)
        .get(&format!("/api/v1/orgs/{}/me", org.id))
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let before = snapshot(&app, org.id).await;
    let client = app.authenticated_client(&delegate);
    let path = format!("/api/v1/orgs/{}/members/{}", org.id, admin.id);
    for response in [
        client.patch_json(&path, &json!({"role":"member"})).await,
        client.patch_json(&path, &json!({"role":"viewer"})).await,
        client.delete(&path).await,
    ] {
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(response
            .text()
            .await
            .unwrap()
            .contains("at least one active Admin"));
        assert_eq!(snapshot(&app, org.id).await, before);
    }
    let response = app
        .authenticated_client(&admin)
        .post_json(
            &format!("{path}/deactivate"),
            &json!({"activityRevision":0}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "self-action restriction remains"
    );
    assert_eq!(snapshot(&app, org.id).await, before);
    // A second active Admin permits ordinary offboarding, including retirement.
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(delegate.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = client.delete(&path).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(admins(&app, org.id).await, 1);
    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE org_id=$1 AND personal_user_id=$2 AND deleted_at IS NULL")
        .bind(org.id).bind(admin.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(live, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_http_self_reductions_preserve_exactly_one_active_admin() {
    let app = TestApp::spawn().await;
    for (first_role, second_role) in [
        (Some("member"), Some("viewer")),
        (None, None),
        (Some("member"), None),
    ] {
        let mut a = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut a).await;
        let b = TestUser::create_admin(&app, org.id).await;
        let ca = app.authenticated_client(&a);
        let cb = app.authenticated_client(&b);
        let pa = format!("/api/v1/orgs/{}/members/{}", org.id, a.id);
        let pb = format!("/api/v1/orgs/{}/members/{}", org.id, b.id);
        assert_eq!(admins(&app, org.id).await, 2);
        let (ra, rb) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                async {
                    match first_role {
                        Some(role) => ca.patch_json(&pa, &json!({"role":role})).await,
                        None => ca.delete(&pa).await,
                    }
                },
                async {
                    match second_role {
                        Some(role) => cb.patch_json(&pb, &json!({"role":role})).await,
                        None => cb.delete(&pb).await,
                    }
                }
            )
        })
        .await
        .expect("both concurrent self-reductions finish");
        let sa = ra.status();
        let sb = rb.status();
        assert_ne!(sa.is_success(), sb.is_success(), "{sa}, {sb}");
        let refused = if sa.is_success() { rb } else { ra };
        assert_eq!(refused.status(), StatusCode::CONFLICT);
        assert!(refused
            .text()
            .await
            .unwrap()
            .contains("at least one active Admin"));
        assert_eq!(admins(&app, org.id).await, 1);
        let survivor: Uuid = sqlx::query_scalar(
            "SELECT user_id FROM org_memberships WHERE org_id=$1 AND role='admin' AND deactivated_at IS NULL",
        )
        .bind(org.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(survivor, if sa.is_success() { b.id } else { a.id });
    }
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_http_demotions_reauthorize_the_waiting_actor() {
    let app = TestApp::spawn().await;
    let mut a = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut a).await;
    let b = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(b.id)
        .execute(&app.db)
        .await
        .unwrap();
    let ca = app.authenticated_client(&a);
    let cb = app.authenticated_client(&b);
    let pa = format!("/api/v1/orgs/{}/members/{}", org.id, a.id);
    let pb = format!("/api/v1/orgs/{}/members/{}", org.id, b.id);
    let request = json!({"role":"member"});
    let (ra, rb) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(ca.patch_json(&pb, &request), cb.patch_json(&pa, &request))
    })
    .await
    .unwrap();
    let mut statuses = [ra.status().as_u16(), rb.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 403]);
    assert_eq!(admins(&app, org.id).await, 1);
    app.cleanup().await;
}

#[tokio::test]
async fn reset_snapshot_maps_historical_owners_to_admin_without_changing_other_roles() {
    let db = TestDb::create().await;
    let script = include_str!("../../../scripts/reset-dev-environment.sh");
    let query = script
        .split_once("PEOPLE_SQL=\"\n")
        .unwrap()
        .1
        .split_once(";\"")
        .unwrap()
        .0;
    let mut tx = db.pool.begin().await.unwrap();
    // Snapshot input can precede conversion. Temporary tables model that input
    // without weakening the live three-role constraint or assigning Owner.
    sqlx::raw_sql(
        "CREATE TEMP TABLE users(id integer, email text, oauth_provider text) ON COMMIT DROP;
         CREATE TEMP TABLE org_memberships(user_id integer, role text) ON COMMIT DROP;
         CREATE TEMP TABLE org_invitations(email text, role text, status text, expires_at timestamptz) ON COMMIT DROP;
         INSERT INTO users VALUES
           (1,'Admin@example.test',NULL),(2,'Member@example.test',NULL),
           (3,'Owner@example.test',NULL),(4,'Viewer@example.test',NULL),
           (5,'service@example.test','service');
         INSERT INTO org_memberships VALUES
           (1,'admin'),(2,'member'),(3,'owner'),(3,'member'),(4,'viewer'),(5,'admin');
         INSERT INTO org_invitations VALUES
           ('owner@example.test','member','pending',NULL),
           ('Pending@example.test','admin','pending',NULL),
           ('expired@example.test','admin','pending',now()-interval '1 day');",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let rows: Vec<(String, String)> = sqlx::query_as(query).fetch_all(&mut *tx).await.unwrap();
    let expected: Vec<_> = [
        ("Admin@example.test", "admin"),
        ("Member@example.test", "member"),
        ("Owner@example.test", "admin"),
        ("Pending@example.test", "admin"),
        ("Viewer@example.test", "viewer"),
    ]
    .into_iter()
    .map(|(email, role)| (email.to_owned(), role.to_owned()))
    .collect();
    assert_eq!(rows, expected);
    tx.rollback().await.unwrap();
    db.drop().await;
}

#[tokio::test]
async fn self_hosted_bootstrap_creates_admins_and_restores_them_on_restart() {
    let db = TestDb::create().await;
    // Execute the shipped bootstrap statement so this checks the producer,
    // including its conflict branch, against the final schema.
    let script = include_str!("../../../infra/self-hosted/scripts/g5-selfhost.sh");
    let sql = format!(
        "WITH org_row AS ({}",
        script
            .split_once("bootstrap_self_hosted_defaults() {")
            .unwrap()
            .1
            .split_once("WITH org_row AS (")
            .unwrap()
            .1
            .split_once("\nSQL\n")
            .unwrap()
            .0
    )
    .replace(":'org_name'", "'Bootstrap tenant'")
    .replace(":'org_slug'", "'bootstrap-tenant'")
    .replace(":'admin_email'", "'bootstrap@example.test'")
    .replace(":'admin_name'", "'Bootstrap administrator'");
    let membership = "SELECT m.org_id,m.user_id,m.role FROM org_memberships m
        JOIN orgs o ON o.id=m.org_id WHERE o.slug='bootstrap-tenant'";
    sqlx::raw_sql(&sql).execute(&db.pool).await.unwrap();
    let created: (Uuid, Uuid, String) = sqlx::query_as(membership)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(created.2, "admin");
    sqlx::raw_sql(&sql).execute(&db.pool).await.unwrap();
    let restarted: (Uuid, Uuid, String) = sqlx::query_as(membership)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(restarted, created);

    let successor = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Successor')")
        .bind(successor)
        .bind(format!("{successor}@test.invalid"))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'admin')")
        .bind(created.0)
        .bind(successor)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE org_memberships SET role='member',deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(created.0)
        .bind(created.1)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::raw_sql(&sql).execute(&db.pool).await.unwrap();
    let promoted: (Uuid, Uuid, String) = sqlx::query_as(&format!("{membership} AND m.user_id=$1"))
        .bind(created.1)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(promoted, (created.0, created.1, "admin".into()));
    let inactive: bool = sqlx::query_scalar(
        "SELECT deactivated_at IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(created.0)
    .bind(created.1)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(
        inactive,
        "bootstrap reruns do not reactivate an existing person"
    );
    db.drop().await;
}
