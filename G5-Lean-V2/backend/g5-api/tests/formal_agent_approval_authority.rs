//! Probe the database boundary independently of the route checks.
use super::*;
use g5_authz::ScopedDb;
use g5_contracts::types::{formal_review_read_scope, formal_review_runtime_scope};
use sqlx::{postgres::PgArguments, query::Query, Postgres};

async fn context(f: &Fixture, kind: &str, own: Uuid, other: Uuid) -> ScopedDb {
    if kind == "human" {
        return f.app.scoped_db_for_user_org(f.owner.id, f.org.id).await;
    }
    let scope = match kind {
        "platform" => "*".into(),
        "runtime" => formal_review_runtime_scope(own),
        "other" => formal_review_runtime_scope(other),
        "read" => formal_review_read_scope(own),
        "ordinary" => "docs:write".into(),
        _ => unreachable!(),
    };
    g5_api::db::scoped::scoped_db_for_system_org(&f.app.state, f.org.id, &[&scope])
        .await
        .unwrap()
}

async fn write_probe(
    db: &mut ScopedDb,
    query: Query<'_, Postgres, PgArguments>,
    allowed: bool,
    retention_fk: bool,
    label: &str,
) {
    sqlx::query("SAVEPOINT authority_probe")
        .execute(db.conn())
        .await
        .unwrap();
    let result = query.execute(db.conn()).await;
    sqlx::query("ROLLBACK TO SAVEPOINT authority_probe")
        .execute(db.conn())
        .await
        .unwrap();
    sqlx::query("RELEASE SAVEPOINT authority_probe")
        .execute(db.conn())
        .await
        .unwrap();
    match result {
        Ok(result) => assert_eq!(result.rows_affected(), u64::from(allowed), "{label}"),
        Err(error) => {
            let code = error.as_database_error().and_then(|e| e.code());
            // An admitted physical DELETE still respects retained-history FKs.
            assert!(
                (!allowed && code.as_deref() == Some("42501"))
                    || (allowed
                        && retention_fk
                        && matches!(code.as_deref(), Some("23503" | "23001"))),
                "{label}: {error}"
            );
        }
    }
}

async fn request_on(f: &Fixture, source: Uuid) -> Uuid {
    let response = f
        .app
        .authenticated_client(&f.owner)
        .post_json(
            &format!("/api/v1/workspaces/{source}/approvals/reviews"),
            &json!({"definitionId":f.definition,"definitionVersion":1,"phase":"ie"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    body["data"]["reviewId"].as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn review_resources_require_exact_context_and_read_contexts_cannot_write() {
    let f = Fixture::with_connections(12).await;
    let other_source = f.source.attach_parent(&f.app).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(other_source)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let review = f.request(None).await;
    let other_review = request_on(&f, other_source).await;
    f.reconcile().await;
    let own = f.child(review).await;
    let other = f.child(other_review).await;
    let contexts = [
        ("platform", true, true),
        ("runtime", true, true),
        ("read", true, false),
        ("ordinary", false, false),
        ("other", false, false),
        ("human", false, false),
    ];
    for (kind, readable, writable) in contexts {
        let mut db = context(&f, kind, own, other).await;
        let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(own)
            .fetch_one(db.conn())
            .await
            .unwrap();
        assert_eq!(
            seen,
            i64::from(readable),
            "{kind}: own workspace visibility"
        );
        let seen_other: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(other)
            .fetch_one(db.conn())
            .await
            .unwrap();
        assert_eq!(
            seen_other,
            i64::from(matches!(kind, "platform" | "other")),
            "{kind}: other workspace visibility"
        );
        write_probe(
            &mut db,
            sqlx::query("UPDATE workspaces SET description='authority probe' WHERE id=$1")
                .bind(own),
            writable,
            false,
            kind,
        )
        .await;
        write_probe(
            &mut db,
            sqlx::query("DELETE FROM workspaces WHERE id=$1").bind(own),
            writable,
            true,
            kind,
        )
        .await;
        db.rollback().await.unwrap();
    }
    // The proposed marker must be checked even though the new row is not yet
    // visible to a definer lookup of existing workspace rows.
    let future_source = f.source.attach_parent(&f.app).await;
    sqlx::query("UPDATE workspaces SET merge_target_workspace_id=$2 WHERE id=$1")
        .bind(future_source)
        .bind(f.target)
        .execute(&f.app.db)
        .await
        .unwrap();
    let future_review = request_on(&f, future_source).await;
    let future = f.child(future_review).await;
    for (kind, _, writable) in contexts {
        let mut db = context(&f, kind, future, other).await;
        write_probe(&mut db, sqlx::query(
            "INSERT INTO workspaces(id,org_id,name,is_base,owner_user_id,created_by,agent_approval_review_id) VALUES($1,$2,$3,false,$4,$4,$5)",
        ).bind(future).bind(f.org.id).bind(format!("authority-{future}")).bind(f.owner.id).bind(future_review), writable, false, kind).await;
        db.rollback().await.unwrap();
    }
    let session = Uuid::new_v4();
    let message = Uuid::new_v4();
    let task = Uuid::new_v4();
    let event = Uuid::new_v4();
    let vm = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by) VALUES($1,$2,$3,$4)")
        .bind(session)
        .bind(f.org.id)
        .bind(own)
        .bind(f.owner.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chat_messages(id,org_id,session_id,role,content,message_index) VALUES($1,$2,$3,'assistant','private',0)")
        .bind(message).bind(f.org.id).bind(session).execute(&f.app.db).await.unwrap();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'Authority','command','running',$4)")
        .bind(task).bind(f.org.id).bind(own).bind(f.owner.id).execute(&f.app.db).await.unwrap();
    sqlx::query("INSERT INTO task_events(id,org_id,task_id,entry_type,content,metadata,attempt,vm_id,seq) VALUES($1,$2,$3,'output','private','{}',1,$4,1)")
        .bind(event).bind(f.org.id).bind(task).bind(&vm).execute(&f.app.db).await.unwrap();
    sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id) VALUES($1,$2) ON CONFLICT(workspace_id) DO NOTHING")
        .bind(f.org.id).bind(own).execute(&f.app.db).await.unwrap();
    let revision = Uuid::new_v4();
    sqlx::query("INSERT INTO workspace_revision_events(id,org_id,workspace_id,observed_at,heads) VALUES($1,$2,$3,now(),'[]')")
        .bind(revision).bind(f.org.id).bind(own).execute(&f.app.db).await.unwrap();
    for (kind, readable, writable) in contexts {
        let mut db = context(&f, kind, own, other).await;
        for (table, key, id) in [
            ("chat_messages", "id", message),
            ("task_events", "id", event),
            ("workspace_vms", "workspace_id", own),
            ("workspace_revision_events", "id", revision),
        ] {
            let seen: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE {key}=$1"))
                    .bind(id)
                    .fetch_one(db.conn())
                    .await
                    .unwrap();
            assert_eq!(seen, i64::from(readable), "{kind}: {table} visibility");
        }
        write_probe(&mut db, sqlx::query("INSERT INTO workspace_revision_events(org_id,workspace_id,observed_at,heads) VALUES($1,$2,now(),'[]')")
            .bind(f.org.id).bind(own), writable, false, kind).await;
        write_probe(
            &mut db,
            sqlx::query("UPDATE chat_messages SET content='changed' WHERE id=$1").bind(message),
            writable,
            false,
            kind,
        )
        .await;
        write_probe(&mut db, sqlx::query("INSERT INTO task_events(id,org_id,task_id,entry_type,content,metadata,attempt,vm_id,seq) VALUES($1,$2,$3,'output','probe','{}',1,$4,2)")
            .bind(Uuid::new_v4()).bind(f.org.id).bind(task).bind(&vm), writable, false, kind).await;
        write_probe(
            &mut db,
            sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1").bind(own),
            writable,
            false,
            kind,
        )
        .await;
        db.rollback().await.unwrap();
    }
    f.app.cleanup().await;
}
