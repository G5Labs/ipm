//! Populated upgrade through the production runner, without superuser/BYPASSRLS.
use crate::helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
async fn disk_snapshot_migration_preserves_live_capacity_and_forced_rls() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "146"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let previous_select: Vec<(String, bool)> = sqlx::query_as(
        "SELECT attname::text,has_column_privilege('authz_lookup_definer','public.worker_hosts',attname,'SELECT')
         FROM pg_attribute WHERE attrelid='public.worker_hosts'::regclass AND attnum>0 AND NOT attisdropped ORDER BY attnum",
    ).fetch_all(&db.pool).await.unwrap();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES($1,$2,'Owner')")
        .bind(owner)
        .bind(format!("{owner}@example.test"))
        .execute(&db.pool)
        .await
        .unwrap();
    let mut hosts = Vec::new();
    let mut vms = Vec::new();
    for (slots, v, disk) in [(5_i32, 65536_i32, Some(911_i32)), (2, 10240, None)] {
        let org = Uuid::new_v4();
        let host = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let vm = Uuid::new_v4();
        sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,$2,$2)")
            .bind(org)
            .bind(format!("disk-{org}"))
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
            .bind(org)
            .bind(owner)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO worker_hosts(host_id,org_id,region,instance_type,status,slots_total,scratch_disk_mib,slot_size_reported,boot_disk_gb) VALUES($1,$2,'us-central1','n2-standard-8','ready',$3,$4,true,$5)")
            .bind(host).bind(org).bind(slots).bind(v).bind(disk).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,$3,$4,$4)")
            .bind(workspace).bind(org).bind(format!("disk-{workspace}")).bind(owner).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,host_id,vm_id,epoch,status,lease_expires_at,boot_spec) VALUES($1,$2,$3,$4,7,'running',now()+interval '5 minutes',$5)")
            .bind(org).bind(workspace).bind(host).bind(vm.to_string()).bind(serde_json::json!({"size":{"scratch_disk_mib":v}})).execute(&db.pool).await.unwrap();
        hosts.push((host, slots, v, disk.unwrap_or(120 + 80 * slots)));
        vms.push((workspace, vm.to_string(), v));
    }
    let role = format!("disk_migrator_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS CREATEROLE;
         GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
         GRANT USAGE, CREATE ON SCHEMA public TO {role};
         GRANT USAGE ON SCHEMA authz TO {role};
         GRANT ALL ON public._sqlx_migrations TO {role};
         GRANT g5_tenant, g5_app TO {role} WITH INHERIT FALSE, SET FALSE;
         ALTER TABLE public.worker_hosts OWNER TO {role};
         ALTER FUNCTION public.g5_config_value_is_registered(text,text,jsonb) OWNER TO {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
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
    let membership_sql = "SELECT g.rolname::text,m.admin_option,m.inherit_option,m.set_option
        FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.roleid AND r.rolname='authz_lookup_definer'
        JOIN pg_roles mem ON mem.oid=m.member AND mem.rolname=$1
        JOIN pg_roles g ON g.oid=m.grantor ORDER BY g.rolname";
    let original_membership: Vec<(String, bool, bool, bool)> = sqlx::query_as(membership_sql)
        .bind(&role)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    let migration = "146_host_disk_allocations.sql";
    std::fs::copy(source.join(migration), catalog.path().join(migration)).unwrap();
    for _ in 0..2 {
        g5_api::db::run_migrations_from_path(&migrator, catalog.path())
            .await
            .unwrap();
    }
    for (column, expected) in previous_select {
        let actual: bool = sqlx::query_scalar(
            "SELECT has_column_privilege('authz_lookup_definer','public.worker_hosts',$1,'SELECT')",
        )
        .bind(&column)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(actual, expected, "existing runtime privilege for {column}");
    }
    for (host, slots, v, disk) in hosts {
        let snapshot:(i32,i32,i32,i32,i32,String)=sqlx::query_as("SELECT host_disk_gib,provisioned_slots,scratch_disk_mib,resolved_disk_gib,slots_total,status FROM worker_hosts WHERE host_id=$1")
            .bind(host).fetch_one(&db.pool).await.unwrap();
        assert_eq!(snapshot, (120, slots, v, disk, slots, "ready".into()));
        assert!(
            sqlx::query("UPDATE worker_hosts SET scratch_disk_mib=81920 WHERE host_id=$1")
                .bind(host)
                .execute(&db.pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE worker_hosts SET slots_total=slots_total+1 WHERE host_id=$1")
                .bind(host)
                .execute(&db.pool)
                .await
                .is_err()
        );
    }
    for (workspace, vm, v) in vms {
        let live: (String, String, i64, serde_json::Value) = sqlx::query_as(
            "SELECT status,vm_id,epoch,boot_spec FROM workspace_vms WHERE workspace_id=$1",
        )
        .bind(workspace)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            live,
            (
                "running".into(),
                vm,
                7,
                serde_json::json!({"size":{"scratch_disk_mib":v}})
            )
        );
    }
    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE oid='public.worker_hosts'::regclass",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(forced);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM public.worker_hosts")
            .fetch_one(&migrator)
            .await
            .unwrap(),
        0,
        "RLS is restored for the owner"
    );
    let restored_membership: Vec<(String, bool, bool, bool)> = sqlx::query_as(membership_sql)
        .bind(&role)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        restored_membership, original_membership,
        "restore every membership option and grantor"
    );
    for column in ["host_disk_gib", "provisioned_slots", "resolved_disk_gib"] {
        for privilege in ["SELECT", "UPDATE"] {
            let held: bool = sqlx::query_scalar(
                "SELECT has_column_privilege('authz_lookup_definer','public.worker_hosts',$1,$2)",
            )
            .bind(column)
            .bind(privilege)
            .fetch_one(&db.pool)
            .await
            .unwrap();
            assert!(!held, "temporary {privilege} on {column} removed");
        }
    }
    migrator.close().await;
    db.drop().await;
}
