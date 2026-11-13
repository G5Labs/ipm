//! Org membership is read from `org_memberships` on every check since the
//! Redis membership cache was removed (G5Labs/G5-Lean#405): a removed member
//! loses access on their next capability mint, with no 60 s lag.

use crate::helpers;

use g5_authz::AuthzError;
use g5_contracts::types::ids::{OrgId, UserId};
use helpers::{TestApp, TestOrg, TestUser};

#[tokio::test]
async fn a_removed_member_fails_the_next_membership_check_and_mint() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create(&app).await;
    sqlx::query("INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .expect("add member");
    let authz = app.state.authz.clone();
    let actor = UserId::from_uuid(member.id);
    let org_id = OrgId::from_uuid(org.id);

    assert!(authz.is_org_member(actor, org_id).await.unwrap());
    authz
        .mint_db_capability(actor, &[org_id], &["*".to_string()])
        .await
        .expect("a member mints a capability");

    sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .expect("remove member");

    assert!(
        !authz.is_org_member(actor, org_id).await.unwrap(),
        "membership is authoritative on every check; nothing may cache the old answer"
    );
    assert!(matches!(
        authz
            .mint_db_capability(actor, &[org_id], &["*".to_string()])
            .await,
        Err(AuthzError::NotMember(_))
    ));
    app.cleanup().await;
}
