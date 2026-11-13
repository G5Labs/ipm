//! Durable remote-pair lease schema and generation-fencing contracts.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestWorkspace};
use uuid::Uuid;

#[tokio::test]
async fn live_owner_excludes_takeover_and_expiry_fences_the_stale_generation() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let remote_branch = "main";
    let first_owner = Uuid::new_v4();
    let second_owner = Uuid::new_v4();

    let first_generation: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO remote_ref_leases (
            repo_id, remote_branch, org_id, owner_token,
            lease_generation, lease_expires_at
        )
        VALUES ($1, $2, $3, $4, 1, now() + interval '10 minutes')
        RETURNING lease_generation
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .bind(org.id)
    .bind(first_owner)
    .fetch_one(&app.db)
    .await
    .expect("first owner claims the pair");
    assert_eq!(first_generation, 1);

    let live_takeover: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO remote_ref_leases (
            repo_id, remote_branch, org_id, owner_token,
            lease_generation, lease_expires_at
        )
        VALUES ($1, $2, $3, $4, 1, now() + interval '10 minutes')
        ON CONFLICT (repo_id, remote_branch) DO UPDATE
           SET owner_token = EXCLUDED.owner_token,
               lease_generation =
                   remote_ref_leases.lease_generation + 1,
               lease_expires_at = EXCLUDED.lease_expires_at
         WHERE remote_ref_leases.lease_expires_at <= now()
        RETURNING lease_generation
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .bind(org.id)
    .bind(second_owner)
    .fetch_optional(&app.db)
    .await
    .expect("live takeover query succeeds without taking ownership");
    assert!(live_takeover.is_none());

    sqlx::query(
        r#"
        UPDATE remote_ref_leases
        SET lease_expires_at = now() - interval '1 second'
        WHERE repo_id = $1 AND remote_branch = $2
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .execute(&app.db)
    .await
    .expect("expire the first generation");

    let second_generation: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO remote_ref_leases (
            repo_id, remote_branch, org_id, owner_token,
            lease_generation, lease_expires_at
        )
        VALUES ($1, $2, $3, $4, 1, now() + interval '10 minutes')
        ON CONFLICT (repo_id, remote_branch) DO UPDATE
           SET owner_token = EXCLUDED.owner_token,
               lease_generation =
                   remote_ref_leases.lease_generation + 1,
               lease_expires_at = EXCLUDED.lease_expires_at
         WHERE remote_ref_leases.lease_expires_at <= now()
        RETURNING lease_generation
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .bind(org.id)
    .bind(second_owner)
    .fetch_one(&app.db)
    .await
    .expect("expired pair admits a new generation");
    assert_eq!(second_generation, first_generation + 1);

    let stale_release = sqlx::query(
        r#"
        DELETE FROM remote_ref_leases
        WHERE repo_id = $1
          AND remote_branch = $2
          AND owner_token = $3
          AND lease_generation = $4
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .bind(first_owner)
    .bind(first_generation)
    .execute(&app.db)
    .await
    .expect("stale release is a valid no-op");
    assert_eq!(stale_release.rows_affected(), 0);

    let current_owner: Uuid = sqlx::query_scalar(
        r#"
        SELECT owner_token
        FROM remote_ref_leases
        WHERE repo_id = $1 AND remote_branch = $2
        "#,
    )
    .bind(workspace.repo_id)
    .bind(remote_branch)
    .fetch_one(&app.db)
    .await
    .expect("read the surviving generation");
    assert_eq!(current_owner, second_owner);

    app.cleanup().await;
}
