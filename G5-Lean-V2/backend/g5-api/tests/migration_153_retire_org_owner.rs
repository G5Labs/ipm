//! Populated upgrade through the production runner, followed by the real API.
use crate::helpers::{self, TestApp, TestDb, TestUser};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

const MIGRATION: &str = "153_retire_org_owner.sql";

fn copy_through(source: &std::path::Path, catalog: &std::path::Path, end: &str) {
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && name.to_string_lossy().as_ref() < end
        {
            std::fs::copy(entry.path(), catalog.join(name)).unwrap();
        }
    }
}

async fn retained_data(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object(
        'members', (SELECT jsonb_agg(to_jsonb(m)-'role' ORDER BY org_id,user_id) FROM org_memberships m),
        'users', (SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM users u),
        'workspaces', (SELECT jsonb_agg(to_jsonb(w) ORDER BY id) FROM workspaces w),
        'grants', (SELECT jsonb_agg(to_jsonb(g) ORDER BY id) FROM user_scope_grants g),
        'keys', (SELECT jsonb_agg(to_jsonb(k) ORDER BY id) FROM api_keys k),
        'refresh', (SELECT jsonb_agg(to_jsonb(r) ORDER BY token_hash) FROM refresh_tokens r),
        'history', (SELECT jsonb_agg(to_jsonb(h) ORDER BY id) FROM workspace_pull_requests h),
        'domains', (SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM issue_domains d))")
        .fetch_one(pool).await.unwrap()
}

fn client_user(id: Uuid, org: Uuid) -> TestUser {
    TestUser {
        id,
        email: format!("{id}@test.invalid"),
        display_name: "Legacy administrator".into(),
        jwt: helpers::test_jwt_minter()
            .mint(
                id,
                org,
                vec![
                    Scope::OrgAdmin,
                    Scope::WorkspacesRead,
                    Scope::WorkspacesWrite,
                ],
                3600,
            )
            .unwrap(),
    }
}

/// Match the production migrator without superuser, inherited definer, or RLS bypass.
async fn restricted_migrator(db: &TestDb) -> (String, PgPool) {
    let role = format!("owner_retirement_migrator_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT USAGE, CREATE ON SCHEMA public TO {role}; ALTER SCHEMA authz OWNER TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         ALTER TABLE public.orgs OWNER TO {role}; ALTER TABLE public.org_memberships OWNER TO {role};
         ALTER FUNCTION public.g5_resolve_eligible_workspace_creator() OWNER TO {role};"
    )).execute(&db.pool).await.unwrap();
    let migrator = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            db.pool
                .connect_options()
                .as_ref()
                .clone()
                .username(&role)
                .password("migration_test"),
        )
        .await
        .unwrap();
    let privileges: (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname=current_user")
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(privileges, (false, false));
    (role, migrator)
}

#[tokio::test]
async fn upgrade_converts_every_owner_preserves_attribution_and_uses_application_offboarding() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::tempdir().unwrap();
    // Exercise the immutable 137 attribution backfill with real legacy Owners.
    copy_through(&source, catalog.path(), "137");
    let db = TestDb::create_from_migrations(catalog.path()).await;
    sqlx::query("INSERT INTO oauth_scopes(scope,description,category) VALUES('org:admin','Org administration','test') ON CONFLICT DO NOTHING")
        .execute(&db.pool).await.unwrap();
    let mut tenants = Vec::new();
    let mut existing_credentials = Vec::new();
    let mut unaffected = Vec::new();
    for roles in [
        vec!["owner"],
        vec!["owner", "owner"],
        vec!["owner", "admin"],
    ] {
        let org = Uuid::new_v4();
        sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Legacy tenant',$2)")
            .bind(org)
            .bind(format!("upgrade-{org}"))
            .execute(&db.pool)
            .await
            .unwrap();
        let mut users = Vec::new();
        for role in roles {
            let user = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO users(id,email,display_name) VALUES($1,$2,'Legacy administrator')",
            )
            .bind(user)
            .bind(format!("{user}@test.invalid"))
            .execute(&db.pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,$3)")
                .bind(org)
                .bind(user)
                .bind(role)
                .execute(&db.pool)
                .await
                .unwrap();
            users.push(user);
        }
        let owner = users[0];
        for (role, inactive) in [("member", false), ("viewer", false), ("admin", true)] {
            let user = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO users(id,email,display_name) VALUES($1,$2,'Unchanged member')",
            )
            .bind(user)
            .bind(format!("{user}@test.invalid"))
            .execute(&db.pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role,deactivated_at) VALUES($1,$2,$3,CASE WHEN $4 THEN now() END)")
                .bind(org).bind(user).bind(role).bind(inactive).execute(&db.pool).await.unwrap();
            unaffected.push((org, user, role, inactive));
        }
        let key = format!(
            "{}{}{}",
            g5_auth::API_KEY_PREFIX,
            owner.simple(),
            owner.simple()
        );
        let key_hash = hex::encode(Sha256::digest(key.as_bytes()));
        existing_credentials.push((client_user(owner, org), key));
        let base = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let personal = Uuid::new_v4();
        sqlx::raw_sql(&format!(
            "INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by,is_base,personal_user_id) VALUES
                ('{base}','{org}','Base','{owner}','{owner}',true,NULL),
                ('{workspace}','{org}','ordinary','{owner}','{owner}',false,NULL),
                ('{personal}','{org}','personal','{owner}','{owner}',false,'{owner}');
             INSERT INTO workspace_pull_requests(org_id,source_workspace_id,target_workspace_id,local_merge_status,title,created_by,last_preflight_fingerprint)
                VALUES('{org}','{workspace}','{base}','ready','Historical attribution','{owner}','legacy');
             INSERT INTO user_scope_grants(org_id,user_id,scope,granted_by) VALUES('{org}','{owner}','org:admin','{owner}');
             INSERT INTO api_keys(org_id,user_id,name,key_prefix,key_hash,scopes)
                VALUES('{org}','{owner}','Retained credential','g5_test','{key_hash}',ARRAY['org:admin','workspaces:read']);
             INSERT INTO refresh_tokens(token_hash,user_id,org_id,expires_at)
                VALUES('{owner}','{owner}','{org}',now()+interval '1 day');"
        )).execute(&db.pool).await.unwrap();
        tenants.push((org, users, workspace));
    }
    sqlx::query("INSERT INTO orgs(name,slug,kind) VALUES('Operator','operator','operator')")
        .execute(&db.pool)
        .await
        .unwrap();
    copy_through(&source, catalog.path(), "153");
    g5_api::db::run_migrations_from_path(&db.pool, catalog.path())
        .await
        .unwrap();
    for (org, users, _) in &tenants {
        let attribution: Uuid =
            sqlx::query_scalar("SELECT configured_by FROM issue_domains WHERE org_id=$1")
                .bind(org)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(
            attribution, users[0],
            "137's existing attribution remains readable"
        );
    }
    let before = retained_data(&db.pool).await;
    let security_sql = "SELECT relname::text,relrowsecurity,relforcerowsecurity FROM pg_class WHERE oid IN ('orgs'::regclass,'org_memberships'::regclass,'workspaces'::regclass) ORDER BY relname";
    let security_before: Vec<(String, bool, bool)> = sqlx::query_as(security_sql)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    let checksums: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let fks_before: Vec<(String,String)> = sqlx::query_as("SELECT conname::text,pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='workspaces'::regclass AND contype='f' ORDER BY conname")
        .fetch_all(&db.pool).await.unwrap();
    let (role, migrator) = restricted_migrator(&db).await;
    let membership_sql = "SELECT g.rolname::text,m.admin_option,m.inherit_option,m.set_option
        FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.roleid AND r.rolname='authz_lookup_definer'
        JOIN pg_roles mem ON mem.oid=m.member AND mem.rolname=$1
        JOIN pg_roles g ON g.oid=m.grantor ORDER BY g.rolname";
    let privileges_before: Vec<(String, bool, bool, bool)> = sqlx::query_as(membership_sql)
        .bind(&role)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    let update_before: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('authz_lookup_definer','org_memberships','role','UPDATE')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let generation_update_before: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('authz_lookup_definer','orgs','access_generation','UPDATE')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let schema_create_sql = "SELECT has_schema_privilege('authz_lookup_definer','authz','CREATE')";
    let schema_create_before: bool = sqlx::query_scalar(schema_create_sql)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(
        !schema_create_before,
        "exercise restoration of the temporary CREATE grant"
    );
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    for _ in 0..2 {
        g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap();
    }
    assert_eq!(
        retained_data(&db.pool).await,
        before,
        "conversion changes only membership roles"
    );
    let privileges_after: Vec<(String, bool, bool, bool)> = sqlx::query_as(membership_sql)
        .bind(&role)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(privileges_after, privileges_before);
    assert_eq!(
        sqlx::query_scalar::<_, bool>(schema_create_sql)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        schema_create_before
    );

    assert_eq!(sqlx::query_scalar::<_,bool>(
        "SELECT has_column_privilege('authz_lookup_definer','orgs','access_generation','UPDATE')")
        .fetch_one(&db.pool).await.unwrap(), generation_update_before);
    assert_eq!(
        sqlx::query_scalar::<_, bool>(
            "SELECT has_column_privilege('authz_lookup_definer','org_memberships','role','UPDATE')"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        update_before
    );
    assert_eq!(
        sqlx::query_as::<_, (String, bool, bool)>(security_sql)
            .fetch_all(&db.pool)
            .await
            .unwrap(),
        security_before
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, Vec<u8>)>(
            "SELECT version,checksum FROM _sqlx_migrations WHERE version<153 ORDER BY version"
        )
        .fetch_all(&db.pool)
        .await
        .unwrap(),
        checksums
    );
    assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT conname::text,pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='workspaces'::regclass AND contype='f' ORDER BY conname").fetch_all(&db.pool).await.unwrap(), fks_before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM org_memberships WHERE role='owner'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    let no_policy: bool =
        sqlx::query_scalar("SELECT to_regprocedure('authz.check_org_admin_continuity()') IS NULL")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(no_policy);
    for (org, user, role, inactive) in unaffected {
        let state: (String,bool) = sqlx::query_as("SELECT role,deactivated_at IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2")
            .bind(org).bind(user).fetch_one(&db.pool).await.unwrap();
        assert_eq!(state, (role.into(), inactive));
    }
    let live_owner_branches: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
        WHERE n.nspname IN ('public','authz') AND p.prokind='f' AND pg_get_functiondef(p.oid) LIKE '%''owner''%'")
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(
        live_owner_branches, 0,
        "live SQL consumers use the final roles"
    );
    for (org, users, _) in &tenants {
        let admins: Vec<Uuid> = sqlx::query_scalar("SELECT user_id FROM org_memberships WHERE org_id=$1 AND role='admin' AND deactivated_at IS NULL ORDER BY user_id")
            .bind(org).fetch_all(&db.pool).await.unwrap();
        let mut expected = users.clone();
        expected.sort();
        assert_eq!(admins, expected);
    }
    migrator.close().await;
    let app = TestApp::spawn_with_database(db).await;
    for (index, ((org, users, _), (owner, key))) in
        tenants.iter().zip(&existing_credentials).enumerate()
    {
        let path = format!("/api/v1/orgs/{org}/members/{}", owner.id);
        let response = app
            .client
            .get(format!("{}/api/v1/orgs/{org}/members", app.base_url))
            .bearer_auth(key)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let error =
            sqlx::query("UPDATE org_memberships SET role='owner' WHERE org_id=$1 AND user_id=$2")
                .bind(org)
                .bind(owner.id)
                .execute(&app.db)
                .await
                .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.constraint()),
            Some("org_memberships_role_check")
        );
        if index == 2 {
            // A converted Owner still must resolve owned workspaces before removal.
            let other = client_user(users[1], *org);
            let response = app.authenticated_client(&other).delete(&path).await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }
        if index == 1 {
            // The second historical Owner has no owned workspaces. Another
            // active Admin can remove them through the ordinary API, with no
            // ownership handover. Reinvite the same account for activity checks.
            let response = app
                .authenticated_client(owner)
                .delete(&format!("/api/v1/orgs/{org}/members/{}", users[1]))
                .await;
            assert_eq!(
                response.status(),
                StatusCode::NO_CONTENT,
                "{}",
                response.text().await.unwrap()
            );
            let response = app
                .authenticated_client(owner)
                .post_json(
                    &format!("/api/v1/orgs/{org}/members"),
                    &json!({"email":format!("{}@test.invalid",users[1]),"role":"admin"}),
                )
                .await;
            assert!(
                response.status().is_success(),
                "{}",
                response.text().await.unwrap()
            );
        }
        if users.len() == 1 {
            let response = app
                .authenticated_client(owner)
                .patch_json(&path, &json!({"role":"member"}))
                .await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert!(response
                .text()
                .await
                .unwrap()
                .contains("at least one active Admin"));
        } else {
            let other = client_user(users[1], *org);
            // Converted Owners have ordinary Admin activity management, retaining
            // workspace identity and cleanup holds through reactivation.
            let response = app
                .authenticated_client(&other)
                .post_json(
                    &format!("{path}/deactivate"),
                    &json!({"activityRevision":0}),
                )
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{}",
                response.text().await.unwrap()
            );
            let response = app
                .authenticated_client(&other)
                .post_json(
                    &format!("{path}/reactivate"),
                    &json!({"activityRevision":1}),
                )
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{}",
                response.text().await.unwrap()
            );
            let response = app.authenticated_client(&other).delete(&path).await;
            assert_eq!(
                response.status(),
                StatusCode::CONFLICT,
                "workspace transfer/cleanup still required"
            );
            let response = app
                .authenticated_client(&other)
                .patch_json(&path, &json!({"role":"member"}))
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{}",
                response.text().await.unwrap()
            );
        }
        let response = app
            .authenticated_client(owner)
            .patch_json(&path, &json!({"role":"owner"}))
            .await;
        // Sole Admin remains authorized; demoted former Admin's additive grant
        // is also preserved and still admits this validation.
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            response.text().await.unwrap()
        );
        let response = app
            .authenticated_client(owner)
            .post_json(
                &format!("/api/v1/orgs/{org}/members"),
                &json!({"email":"new-owner@example.test","role":"owner"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    app.cleanup().await;
}

#[tokio::test]
async fn invalid_tenants_fail_preflight_without_converting_or_reactivating_anyone() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::tempdir().unwrap();
    copy_through(&source, catalog.path(), "153");
    let db = TestDb::create_from_migrations(catalog.path()).await;
    sqlx::query("INSERT INTO oauth_scopes(scope,description,category) VALUES('org:admin','Org administration','test') ON CONFLICT DO NOTHING")
        .execute(&db.pool).await.unwrap();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Preflight')")
        .bind(user)
        .bind(format!("{user}@test.invalid"))
        .execute(&db.pool)
        .await
        .unwrap();
    let valid = Uuid::new_v4();
    sqlx::raw_sql(&format!(
        "INSERT INTO orgs(id,name,slug) VALUES('{valid}','Valid','{valid}');
        INSERT INTO org_memberships(org_id,user_id,role) VALUES('{valid}','{user}','owner');"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    std::fs::copy(source.join(MIGRATION), catalog.path().join(MIGRATION)).unwrap();
    let (_, migrator) = restricted_migrator(&db).await;
    for invalid_kind in ["empty", "inactive-admin", "grant-and-invitation"] {
        let org = Uuid::new_v4();
        sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,$2,$3)")
            .bind(org)
            .bind(invalid_kind)
            .bind(org.to_string())
            .execute(&db.pool)
            .await
            .unwrap();
        if invalid_kind != "empty" {
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role,deactivated_at) VALUES($1,$2,$3,CASE WHEN $3='admin' THEN now() END)")
                .bind(org).bind(user).bind(if invalid_kind=="inactive-admin" {"admin"} else {"member"}).execute(&db.pool).await.unwrap();
            if invalid_kind == "grant-and-invitation" {
                sqlx::raw_sql(&format!("INSERT INTO user_scope_grants(org_id,user_id,scope,granted_by) VALUES('{org}','{user}','org:admin','{user}');
                    INSERT INTO org_invitations(org_id,email,role,invited_by) VALUES('{org}','pending@example.test','admin','{user}');"))
                    .execute(&db.pool).await.unwrap();
            }
        }
        let before = retained_data(&db.pool).await;
        let error = g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("requires explicit repair"),
            "{error}"
        );
        assert!(error.to_string().contains(&org.to_string()), "{error}");
        assert_eq!(retained_data(&db.pool).await, before);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT role FROM org_memberships WHERE org_id=$1")
                .bind(valid)
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            "owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE version=153")
                .fetch_one(&db.pool)
                .await
                .unwrap(),
            0
        );
        sqlx::query("DELETE FROM orgs WHERE id=$1")
            .bind(org)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    migrator.close().await;
    db.drop().await;
}
