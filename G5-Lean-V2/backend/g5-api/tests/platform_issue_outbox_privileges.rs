//! Live-Postgres regression for the platform-report outbox privilege boundary.
//!
//! `platform_issue_outbox` intentionally has no tenant key or RLS policy: it is
//! system metadata delivered by the server-side worker to the configured G5
//! GitHub repository. Migration 173 must therefore grant CRUD only to the
//! `g5_app` system role and explicitly deny every table privilege to the
//! tenant-facing `g5_tenant` role.

use crate::helpers;

use helpers::TestDb;
use sqlx::Row;

const SYSTEM_ONLY_TABLES: [&str; 2] = [
    "public.platform_issue_outbox",
    "public.platform_issue_delivery_tombstone",
];
const CRUD_PRIVILEGES: [&str; 4] = ["SELECT", "INSERT", "UPDATE", "DELETE"];

#[tokio::test]
async fn platform_issue_delivery_tables_are_accessible_only_to_g5_app() {
    let test_db = TestDb::create().await;

    for table in SYSTEM_ONLY_TABLES {
        for privilege in CRUD_PRIVILEGES {
            assert_table_privilege(&test_db, table, "g5_app", privilege, true).await;
            assert_table_privilege(&test_db, table, "g5_tenant", privilege, false).await;
        }
    }

    test_db.drop().await;
}

async fn assert_table_privilege(
    test_db: &TestDb,
    table: &str,
    role: &str,
    privilege: &str,
    expected: bool,
) {
    let row = sqlx::query("SELECT has_table_privilege($1, $2, $3) AS has_table_privilege")
        .bind(role)
        .bind(table)
        .bind(privilege)
        .fetch_one(&test_db.pool)
        .await
        .expect("check platform_issue_outbox privilege");
    let has_privilege: bool = row.get("has_table_privilege");

    assert_eq!(
        has_privilege, expected,
        "{role} {privilege} privilege on {table} must be {expected}"
    );
}
