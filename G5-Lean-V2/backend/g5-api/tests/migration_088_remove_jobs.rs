//! Retire execution graphs on populated tenants using the production migrator role.
use crate::helpers::TestDb;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
async fn retirement_cancels_graphs_preserves_independent_state_and_restores_rls() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let catalog = tempfile::TempDir::new().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "sql")
            && entry.file_name().to_string_lossy().as_ref() < "088"
        {
            std::fs::copy(entry.path(), catalog.path().join(entry.file_name())).unwrap();
        }
    }
    let db = TestDb::create_from_migrations(catalog.path()).await;
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    let workspace = Uuid::new_v4();
    let job = Uuid::new_v4();
    let retired = Uuid::new_v4();
    let dependent = Uuid::new_v4();
    let live = Uuid::new_v4();
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Retirement',$1::text)")
        .bind(org)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users(id,email,display_name) VALUES($1,$1::text || '@test.invalid','Owner')",
    )
    .bind(user)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'retirement',$3,$3)").bind(workspace).bind(org).bind(user).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO jobs(id,org_id,workspace_id,name,job_type,status,created_by) VALUES($1,$2,$3,'Old executor','custom','running',$4)").bind(job).bind(org).bind(workspace).bind(user).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by,scope_type,title) VALUES($1,$2,$3,$4,'workspace','Retained conversation')").bind(session).bind(org).bind(workspace).bind(user).execute(&db.pool).await.unwrap();
    for (id, parent, state) in [
        (retired, Some(job), "running"),
        (dependent, None, "queued"),
        (live, None, "running"),
    ] {
        sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,job_id,name,kind,status,attempt,vm_id,epoch,start_confirmed,started_at,created_by) VALUES($1,$2,$3,$4,'Turn','command',$5,1,$6,1,true,now(),$7)")
            .bind(id).bind(org).bind(workspace).bind(parent).bind(state).bind(Uuid::new_v4().to_string()).bind(user).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO task_attempts(org_id,task_id,attempt,vm_id,epoch,start_confirmed,started_at) SELECT org_id,id,attempt,vm_id,epoch,start_confirmed,started_at FROM tasks WHERE id=$1")
            .bind(id).execute(&db.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO task_dependencies(org_id,workspace_id,task_id,depends_on_task_id) VALUES($1,$2,$3,$4)").bind(org).bind(workspace).bind(dependent).bind(retired).execute(&db.pool).await.unwrap();

    for (index, task) in [(0, retired), (1, live)] {
        sqlx::query("INSERT INTO session_inputs(org_id,session_id,input_index,content,input_origin,delivery,task_id,attempt,reservation_id,reservation_revision) VALUES($1,$2,$3,'Queued input','human','reserved',$4,1,$5,1)").bind(org).bind(session).bind(index).bind(task).bind(Uuid::new_v4()).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO provider_requests(org_id,task_id,attempt,provider_request_id,kind,payload,display,response_deadline_at) VALUES($1,$2,1,'permission','permission','{}','{}',now()+interval '1 hour')").bind(org).bind(task).execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO usage_records(org_id,workspace_id,task_id,job_id,session_id,provider,model,input_tokens,output_tokens,total_tokens,cost_usd,request_type) VALUES($1,$2,$3,$4,$5,'codex','test',10,5,15,0.5,'chat')").bind(org).bind(workspace).bind(task).bind(if task==retired {Some(job)}else{None}).bind(session).execute(&db.pool).await.unwrap();
    }
    for kind in ["job", "workspace", "remote_tracking"] {
        sqlx::query("INSERT INTO job_merge_runs(org_id,workspace_id,source_kind,source_job_id,source_workspace_id,target_workspace_id,source_branch_name,status) VALUES($1,$2,$3,$4,$2,$2,'main','completed')").bind(org).bind(workspace).bind(kind).bind(if kind=="job" {Some(job)}else{None}).execute(&db.pool).await.unwrap();
    }
    // A legacy workspace merge whose executor was a job cannot survive as a
    // live workspace run after its executor is retired.
    sqlx::query("INSERT INTO job_merge_runs(org_id,workspace_id,source_kind,merge_job_id,source_workspace_id,target_workspace_id,source_branch_name,status) VALUES($1,$2,'workspace',$3,$2,$2,'main','running')")
        .bind(org).bind(workspace).bind(job).execute(&db.pool).await.unwrap();
    // Populate each approval retirement branch: an old-only request is removed,
    // a request also owned by a live task survives, and human history is retained.
    let definition = Uuid::new_v4();
    sqlx::query("INSERT INTO approval_definitions(id,version,org_id,name,humans_list,created_by,agent_type,agent_prompt,agent_harness,agent_model,agent_effort) VALUES($1,1,$2,'Retirement review',ARRAY[$3]::uuid[],$3,'simple','Review','claude_code','test-model','high')")
        .bind(definition).bind(org).bind(user).execute(&db.pool).await.unwrap();
    let human_request = Uuid::new_v4();
    let retired_request = Uuid::new_v4();
    let task_request = Uuid::new_v4();
    for (request, actor, actor_user) in [
        (human_request, "human", Some(user)),
        (retired_request, "agent", None),
        (task_request, "agent", None),
    ] {
        sqlx::query("INSERT INTO approval_requests(id,org_id,source_workspace_id,approval_definition_id,approval_definition_version,request_key,actor_type,actor_user_id,requested_by,phase) VALUES($1,$2,$3,$4,1,$5,$6,$7,$8,'ie')")
            .bind(request).bind(org).bind(workspace).bind(definition)
            .bind(format!("{}{}",request.simple(),request.simple()))
            .bind(actor).bind(actor_user).bind(user).execute(&db.pool).await.unwrap();
    }
    let linked_job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,org_id,workspace_id,name,job_type,status,created_by) VALUES($1,$2,$3,'Previous review','custom','failed',$4)")
        .bind(linked_job).bind(org).bind(workspace).bind(user).execute(&db.pool).await.unwrap();
    for (run_job, request) in [(job, retired_request), (linked_job, task_request)] {
        sqlx::query("INSERT INTO approval_review_runs(job_id,org_id,source_workspace_id,approval_request_id,definition_id,definition_version,request_key) VALUES($1,$2,$3,$4,$5,1,$6)")
            .bind(run_job).bind(org).bind(workspace).bind(request).bind(definition)
            .bind(format!("{}{}",request.simple(),request.simple()))
            .execute(&db.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO approval_request_tasks(org_id,approval_request_id,source_workspace_id,task_id) VALUES($1,$2,$3,$4)")
        .bind(org).bind(task_request).bind(workspace).bind(live).execute(&db.pool).await.unwrap();
    let human_status: Uuid = sqlx::query_scalar("INSERT INTO approval_statuses(org_id,source_workspace_id,approval_definition_id,approval_definition_version,outcome,actor_type,actor_user_id,reviewed_git_heads,phase) VALUES($1,$2,$3,1,'approved','human',$4,'[]','ie') RETURNING id")
        .bind(org).bind(workspace).bind(definition).bind(user).fetch_one(&db.pool).await.unwrap();
    let heads = json!([{"repoId":"old-repo","branchName":"main","workspaceId":workspace,"headCommit":"a".repeat(40)}]);
    let evidence = json!({
        "schemaVersion":1,"targetWorkspaceId":workspace,"baseHeads":heads,
        "artifact":{"path":format!("docs/{workspace}/approval-review-{job}.md"),"commitSha":"a".repeat(40),"blobOid":"b".repeat(40)},
        "validationDisposition":"valid","executionReceipt":null,"attemptCount":1,
        "contractVersions":{"reviewContext":1,"reviewPolicy":1,"output":1},"failure":null
    });
    sqlx::query("INSERT INTO approval_statuses(org_id,source_workspace_id,approval_definition_id,approval_definition_version,outcome,actor_type,agent_job_id,agent_approval_run_details,reviewed_git_heads,phase) VALUES($1,$2,$3,1,'approved','agent',$4,$5,$6,'ie')")
        .bind(org).bind(workspace).bind(definition).bind(job).bind(evidence).bind(heads)
        .execute(&db.pool).await.unwrap();
    // Durable permissions outlive workers. Exercise both an old-only tenant and
    // a tenant with task grants already present, including duplicate API scopes.
    sqlx::raw_sql(
        "INSERT INTO oauth_scopes(scope,description,category) VALUES
        ('jobs:read','Old read','jobs'),('jobs:execute','Old execute','jobs'),
        ('jobs:cancel','Old cancel','jobs'),('tasks:read','Task read','tasks'),
        ('tasks:execute','Task execute','tasks'),('workspaces:read','Read','workspaces')
        ON CONFLICT DO NOTHING; ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let second_org = Uuid::new_v4();
    let second_workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO orgs(id,name,slug) VALUES($1,'Second',$1::text)")
        .bind(second_org)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(second_org)
        .bind(user)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'second',$3,$3)")
        .bind(second_workspace).bind(second_org).bind(user).execute(&db.pool).await.unwrap();
    for (tenant, space) in [(org, workspace), (second_org, second_workspace)] {
        let mut scopes = vec![
            "jobs:read",
            "jobs:execute",
            "jobs:cancel",
            "workspaces:read",
        ];
        if tenant == org {
            scopes.extend(["tasks:read", "tasks:execute"]);
        }
        sqlx::query("INSERT INTO api_keys(org_id,user_id,name,key_prefix,key_hash,scopes) VALUES($1,$2,'Migrated key',$1::text,$1::text,$3)")
            .bind(tenant).bind(user).bind(&scopes).execute(&db.pool).await.unwrap();
        for scope in scopes {
            sqlx::query("INSERT INTO org_default_scopes(org_id,scope) VALUES($1,$2)")
                .bind(tenant)
                .bind(scope)
                .execute(&db.pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO user_scope_grants(org_id,user_id,scope,granted_by,expires_at) VALUES($1,$2,$3,$2,'2099-01-01')")
                .bind(tenant).bind(user).bind(scope).execute(&db.pool).await.unwrap();
            sqlx::query("INSERT INTO workspace_permission_overrides(workspace_id,user_id,scope,granted_by) VALUES($1,$2,$3,$2)")
                .bind(space).bind(user).bind(scope).execute(&db.pool).await.unwrap();
        }
    }
    sqlx::query("INSERT INTO api_keys(org_id,user_id,name,key_prefix,key_hash,scopes) VALUES($1,$2,'Cancel only','cancel','cancel',ARRAY['jobs:cancel'])")
        .bind(org).bind(user).execute(&db.pool).await.unwrap();
    // Migration 087 grants the credential helper column-level authority. The
    // retirement's temporary table privileges must preserve those exact reads
    // and writes when it restores the definer's grants.
    let column_privileges_sql = "SELECT attname::text, privilege, has_column_privilege('authz_lookup_definer',attrelid,attname,privilege) FROM pg_attribute CROSS JOIN unnest(ARRAY['SELECT','UPDATE','INSERT','REFERENCES']) privilege WHERE attrelid='public.api_keys'::regclass AND attnum>0 AND NOT attisdropped ORDER BY attname,privilege";
    let column_privileges_before: Vec<(String, String, bool)> =
        sqlx::query_as(column_privileges_sql)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    let api_privileges_before: Vec<(String,bool)> = sqlx::query_as("SELECT p,has_table_privilege('authz_lookup_definer','public.api_keys',p) FROM unnest(ARRAY['SELECT','UPDATE','DELETE']) p ORDER BY p")
        .fetch_all(&db.pool).await.unwrap();
    let role = format!("retire_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN PASSWORD 'migration_test' NOSUPERUSER NOBYPASSRLS; GRANT authz_lookup_definer TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE; GRANT USAGE, CREATE ON SCHEMA public TO {role}; GRANT USAGE ON SCHEMA authz TO {role};")).execute(&db.pool).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname='public'")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for table in tables {
        sqlx::raw_sql(&format!("ALTER TABLE public.\"{table}\" OWNER TO {role}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let functions: Vec<String> = sqlx::query_scalar("SELECT oid::regprocedure::text FROM pg_proc WHERE pronamespace='public'::regnamespace AND prokind='f' AND (proname LIKE 'approval_review_%' OR proname LIKE 'agent_approval_%' OR proname IN ('reject_approval_context_update','validate_agent_approval_status_v3_run','validate_agent_approval_status_v4_run'))").fetch_all(&db.pool).await.unwrap();
    for function in functions {
        sqlx::raw_sql(&format!("ALTER FUNCTION {function} OWNER TO {role}"))
            .execute(&db.pool)
            .await
            .unwrap();
    }
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
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(
        visible, 0,
        "migration role cannot see tenants before the cutover"
    );
    std::fs::copy(
        source.join("088_remove_jobs.sql"),
        catalog.path().join("088_remove_jobs.sql"),
    )
    .unwrap();
    let result = g5_api::db::run_migrations_from_path(&migrator, catalog.path()).await;
    assert!(result.is_ok(), "{result:?}");
    g5_api::db::run_migrations_from_path(&migrator, catalog.path())
        .await
        .unwrap();
    for (tenant, space) in [(org, workspace), (second_org, second_workspace)] {
        let expected = vec!["tasks:execute", "tasks:read", "workspaces:read"];
        let scopes: Vec<String> = sqlx::query_scalar(
            "SELECT scopes FROM api_keys WHERE org_id=$1 AND name='Migrated key'",
        )
        .bind(tenant)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            scopes, expected,
            "API keys must use current task permissions"
        );
        for table in ["org_default_scopes", "user_scope_grants"] {
            let scopes: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT scope FROM {table} WHERE org_id=$1 ORDER BY scope"
            ))
            .bind(tenant)
            .fetch_all(&db.pool)
            .await
            .unwrap();
            assert_eq!(scopes, expected, "{table}");
        }
        let scopes: Vec<String> = sqlx::query_scalar(
            "SELECT scope FROM workspace_permission_overrides WHERE workspace_id=$1 ORDER BY scope",
        )
        .bind(space)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert_eq!(scopes, expected);
    }
    let cancel_scopes: Vec<String> =
        sqlx::query_scalar("SELECT scopes FROM api_keys WHERE name='Cancel only'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(
        cancel_scopes.is_empty(),
        "retired cancel must not grant task execution"
    );
    let old_scopes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM oauth_scopes WHERE scope LIKE 'jobs:%'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(old_scopes, 0);
    let registry: Vec<String> =
        sqlx::query_scalar("SELECT scope FROM oauth_scopes WHERE category='tasks' ORDER BY scope")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(registry, vec!["tasks:execute", "tasks:read"]);
    let metadata_retained: bool = sqlx::query_scalar("SELECT bool_and(granted_by=$1 AND expires_at='2099-01-01'::timestamptz) FROM user_scope_grants")
        .bind(user).fetch_one(&db.pool).await.unwrap();
    assert!(metadata_retained);
    let api_privileges_after: Vec<(String,bool)> = sqlx::query_as("SELECT p,has_table_privilege('authz_lookup_definer','public.api_keys',p) FROM unnest(ARRAY['SELECT','UPDATE','DELETE']) p ORDER BY p")
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(api_privileges_before, api_privileges_after);
    let column_privileges_after: Vec<(String, String, bool)> =
        sqlx::query_as(column_privileges_sql)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(column_privileges_before, column_privileges_after);
    for tenant in [org, second_org] {
        let scopes: Vec<String> =
            sqlx::query_scalar("SELECT scopes FROM authz.lookup_api_key_by_hash($1)")
                .bind(tenant.to_string())
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(
            scopes,
            vec!["tasks:execute", "tasks:read", "workspaces:read"]
        );
    }

    for id in [retired, dependent] {
        let row: (String, Option<String>, Option<i64>, bool) =
            sqlx::query_as("SELECT status,vm_id,epoch,start_confirmed FROM tasks WHERE id=$1")
                .bind(id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(row, ("cancelled".into(), None, None, false));
        let ended: bool =
            sqlx::query_scalar("SELECT ended_at IS NOT NULL FROM task_attempts WHERE task_id=$1")
                .bind(id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(ended);
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id=$1")
            .bind(live)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "running"
    );
    let states:Vec<(String,String)>=sqlx::query_as("SELECT delivery,status FROM session_inputs i JOIN provider_requests p ON p.task_id=i.task_id ORDER BY i.input_index").fetch_all(&db.pool).await.unwrap();
    assert_eq!(
        states,
        vec![
            ("withdrawn".into(), "invalidated".into()),
            ("reserved".into(), "pending".into())
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM chat_sessions WHERE id=$1")
            .bind(session)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sum(cost_usd)::text FROM usage_records")
            .fetch_one(&db.pool)
            .await
            .unwrap()
            .parse::<f64>()
            .unwrap(),
        1.0
    );
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT source_kind FROM workspace_merge_runs ORDER BY source_kind")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(kinds, vec!["remote_tracking", "workspace"]);
    let requests: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM approval_requests ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    let mut retained_requests = vec![human_request, task_request];
    retained_requests.sort();
    assert_eq!(
        requests, retained_requests,
        "only the old-run-only request is retired"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT task_id FROM approval_request_tasks WHERE approval_request_id=$1"
        )
        .bind(task_request)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        live
    );
    let decisions: Vec<(Uuid, String, Option<Uuid>, String)> =
        sqlx::query_as("SELECT id,actor_type,actor_user_id,outcome FROM approval_statuses")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        decisions,
        vec![(human_status, "human".into(), Some(user), "approved".into())]
    );
    let invalid_agent = sqlx::query("INSERT INTO approval_statuses(org_id,source_workspace_id,approval_definition_id,approval_definition_version,outcome,actor_type,reviewed_git_heads,phase) VALUES($1,$2,$3,1,'approved','agent','[]','ie')")
        .bind(org).bind(workspace).bind(definition).execute(&db.pool).await.unwrap_err();
    assert_eq!(
        invalid_agent
            .as_database_error()
            .and_then(|e| e.constraint()),
        Some("approval_statuses_human_actor")
    );
    let retired_objects:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='public' AND (table_name IN ('jobs','job_merge_runs','approval_review_runs') OR column_name IN ('job_id','source_job_id','merge_job_id','agent_job_id','concurrent_jobs'))").fetch_one(&db.pool).await.unwrap();
    assert_eq!(retired_objects, 0);
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(visible, 0, "FORCE RLS restored");
    let membership:(bool,bool,bool)=sqlx::query_as("SELECT admin_option, inherit_option, set_option FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.member JOIN pg_roles granted ON granted.oid=m.roleid WHERE r.rolname=$1 AND granted.rolname='authz_lookup_definer'").bind(&role).fetch_one(&db.pool).await.unwrap();
    assert_eq!(
        membership,
        (true, false, false),
        "temporary SET capability is restored"
    );
    migrator.close().await;
    sqlx::raw_sql(&format!(
        "REASSIGN OWNED BY {role} TO CURRENT_USER; DROP OWNED BY {role}; DROP ROLE {role};"
    ))
    .execute(&db.pool)
    .await
    .unwrap();
    db.drop().await;
}
