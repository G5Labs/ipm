//! Live authority changes must revoke existing event sockets, including
//! indirect subscriptions, without evicting readers who retain access.

use crate::helpers::{TestApp, TestOrg, TestTask, TestUser, TestWorkspace, WsTestClient};
use g5_api::ws::{Channel, WsEvent};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

const SOON: Duration = Duration::from_secs(5);
const REVOCATION_BOUND: Duration = Duration::from_secs(30);

struct Fixture {
    app: TestApp,
    owner: TestUser,
    reader: TestUser,
    org: TestOrg,
    workspace: TestWorkspace,
    channels: Vec<Channel>,
}

impl Fixture {
    async fn new(personal: bool) -> Self {
        let app = TestApp::spawn().await;
        let mut owner = TestUser::create(&app).await;
        let org = TestOrg::create_with_admin(&app, &mut owner).await;
        let reader = TestUser::create_with_scopes(
            &app,
            org.id,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
        )
        .await;
        let workspace = if personal {
            sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
                .bind(org.id)
                .bind(reader.id)
                .execute(&app.db)
                .await
                .unwrap();
            TestWorkspace::create_personal(&app, &org, owner.id).await
        } else {
            TestWorkspace::create(&app, &org).await
        };
        let session = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO chat_sessions (id, org_id, workspace_id, created_by) VALUES ($1,$2,$3,$4)",
        )
        .bind(session)
        .bind(org.id)
        .bind(workspace.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
        let task = TestTask::create(&app, &workspace).await;
        // This fixture has no running VM; transfer must not pretend to drain
        // a running command task that has no execution backend.
        sqlx::query("UPDATE tasks SET status='succeeded', ended_at=now() WHERE id=$1")
            .bind(task.id)
            .execute(&app.db)
            .await
            .unwrap();
        let channels = vec![
            Channel::Workspace(workspace.id),
            Channel::Chat(session),
            Channel::Task(task.id),
        ];
        Self {
            app,
            owner,
            reader,
            org,
            workspace,
            channels,
        }
    }

    async fn connect(&self, user: &TestUser, channels: &[Channel]) -> WsTestClient {
        let mut socket = self.app.ws_connect(user, &channels[0].to_string()).await;
        socket.expect_subscribed().await;
        for channel in &channels[1..] {
            socket.subscribe(&channel.to_string()).await;
            socket.expect_subscribed().await;
        }
        socket
    }

    async fn publish_and_receive(&self, socket: &mut WsTestClient) {
        for channel in &self.channels {
            let event_id = Uuid::new_v4();
            let event = WsEvent::TaskStatus {
                task_id: event_id,
                status: "private-after-change".into(),
            };
            self.app.state.event_bus.publish(channel, &event);
            tokio::time::timeout(SOON, async {
                loop {
                    let received = socket.expect_event("task.status", SOON).await;
                    if received["taskId"] == event_id.to_string() {
                        assert_eq!(received["status"], "private-after-change");
                        break;
                    }
                }
            })
            .await
            .expect("retained socket must receive the newly published event");
        }
    }

    async fn remove_reader(&self) {
        let response = self
            .app
            .authenticated_client(&self.owner)
            .delete(&format!(
                "/api/v1/workspaces/{}/members/{}",
                self.workspace.id, self.reader.id
            ))
            .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{}",
            response.text().await.unwrap()
        );
    }
}

#[tokio::test]
async fn personal_admin_demotion_closes_indirect_sockets_but_preserves_ordinary_access() {
    let f = Fixture::new(true).await;
    let mut owner = f.connect(&f.owner, &f.channels).await;
    let mut revoked = f.connect(&f.reader, &f.channels).await;
    // This socket has only an indirect channel: scope must come from resolution.
    let mut chat_only = f.connect(&f.reader, &f.channels[1..2]).await;
    let other = TestWorkspace::create(&f.app, &f.org).await;
    let mut unaffected = f.connect(&f.reader, &[Channel::Workspace(other.id)]).await;
    let changed = f
        .app
        .authenticated_client(&f.owner)
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", f.org.id, f.reader.id),
            &json!({"role": "member"}),
        )
        .await;
    assert_eq!(
        changed.status(),
        StatusCode::OK,
        "{}",
        changed.text().await.unwrap()
    );
    // Role changes use the existing bounded read lease. Require both direct
    // and indirect subscriptions to close within its 30-second contract.
    tokio::join!(
        revoked.expect_closed(REVOCATION_BOUND),
        chat_only.expect_closed(REVOCATION_BOUND),
    );
    f.publish_and_receive(&mut owner).await;
    f.app.state.event_bus.publish(
        &Channel::Workspace(other.id),
        &WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "retained".into(),
        },
    );
    unaffected.expect_event("task.status", SOON).await;
    let mut denied = f
        .app
        .ws_connect(&f.reader, &f.channels[0].to_string())
        .await;
    assert_eq!(
        denied.expect_event("error", SOON).await["code"],
        "FORBIDDEN"
    );
    let changed = f
        .app
        .authenticated_client(&f.owner)
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", f.org.id, f.reader.id),
            &json!({"role": "admin"}),
        )
        .await;
    assert_eq!(
        changed.status(),
        StatusCode::OK,
        "{}",
        changed.text().await.unwrap()
    );
    let mut reconnected = f.connect(&f.reader, &f.channels).await;
    f.publish_and_receive(&mut reconnected).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn organization_removal_closes_resource_and_user_only_sockets() {
    let f = Fixture::new(false).await;
    let other_org = TestOrg::create(&f.app).await;
    sqlx::query("INSERT INTO org_memberships (org_id,user_id,role) VALUES ($1,$2,'member')")
        .bind(other_org.id)
        .bind(f.reader.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let other_workspace = TestWorkspace::create(&f.app, &other_org).await;
    let other_identity = TestUser {
        id: f.reader.id,
        email: f.reader.email.clone(),
        display_name: f.reader.display_name.clone(),
        jwt: crate::helpers::test_jwt_minter()
            .mint(f.reader.id, other_org.id, vec![Scope::WorkspacesRead], 3600)
            .unwrap(),
    };
    let mut owner = f.connect(&f.owner, &f.channels).await;
    let mut revoked = f.connect(&f.reader, &f.channels).await;
    let mut user_only = f.connect(&f.reader, &[Channel::User(f.reader.id)]).await;
    let mut org_only = f.connect(&f.reader, &[Channel::Org(f.org.id)]).await;
    let mut indirect_other_org = f.connect(&other_identity, &f.channels[1..2]).await;
    let other_channel = Channel::Workspace(other_workspace.id);
    let mut unaffected = f
        .connect(&other_identity, std::slice::from_ref(&other_channel))
        .await;
    let response = f
        .app
        .authenticated_client(&f.owner)
        .delete(&format!(
            "/api/v1/orgs/{}/members/{}",
            f.org.id, f.reader.id
        ))
        .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "{}",
        response.text().await.unwrap()
    );
    f.publish_and_receive(&mut owner).await;
    f.app.state.event_bus.publish(
        &Channel::User(f.reader.id),
        &WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "offboarded".into(),
        },
    );
    revoked.expect_closed(SOON).await;
    user_only.expect_closed(SOON).await;
    org_only.expect_closed(SOON).await;
    indirect_other_org.expect_closed(SOON).await;
    f.app.state.event_bus.publish(
        &other_channel,
        &WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "other-org-retained".into(),
        },
    );
    assert_eq!(
        unaffected.expect_event("task.status", SOON).await["status"],
        "other-org-retained"
    );
    f.app.cleanup().await;
}

#[tokio::test]
async fn removed_visibility_patch_preserves_all_ordinary_readers() {
    let f = Fixture::new(false).await;
    let outsider = TestUser::create_with_scopes(&f.app, f.org.id, &[Scope::WorkspacesRead]).await;
    let mut revoked = f.connect(&outsider, &f.channels).await;
    let mut reader = f.connect(&f.reader, &f.channels).await;
    let mut admin = f.connect(&f.owner, &f.channels).await;
    let response = f
        .app
        .authenticated_client(&f.owner)
        .patch_json(
            &format!("/api/v1/workspaces/{}", f.workspace.id),
            &json!({"visibility":"invite_only"}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        response.text().await.unwrap()
    );
    f.publish_and_receive(&mut revoked).await;
    f.publish_and_receive(&mut reader).await;
    f.publish_and_receive(&mut admin).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn removed_invitation_route_preserves_member_and_admin_streams() {
    for admin in [false, true] {
        let f = Fixture::new(false).await;
        if admin {
            sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
                .bind(f.org.id)
                .bind(f.reader.id)
                .execute(&f.app.db)
                .await
                .unwrap();
        }
        let mut reader = f.connect(&f.reader, &f.channels).await;
        f.remove_reader().await;
        f.publish_and_receive(&mut reader).await;
        f.app.cleanup().await;
    }
}

#[tokio::test]
async fn transfer_preserves_ordinary_read_access_for_previous_owner() {
    let f = Fixture::new(false).await;
    let next = TestUser::create_with_scopes(
        &f.app,
        f.org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(f.workspace.id)
        .bind(f.reader.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    let mut previous = f.connect(&f.reader, &f.channels).await;
    let mut new_owner = f.connect(&next, &f.channels).await;
    let response = f
        .app
        .authenticated_client(&f.reader)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", f.workspace.id),
            &json!({"toUserId":next.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome["data"]["status"], "completed");
    let current_owner: Uuid =
        sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(f.workspace.id)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
    assert_eq!(current_owner, next.id);
    f.publish_and_receive(&mut new_owner).await;
    f.publish_and_receive(&mut previous).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn periodic_reauthorization_revokes_without_an_eviction_hook() {
    let f = Fixture::new(false).await;
    let mut revoked = f.connect(&f.reader, &f.channels).await;
    let mut owner = f.connect(&f.owner, &f.channels).await;
    sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.reader.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    revoked.expect_closed(REVOCATION_BOUND).await;
    // The owner's original connection survives renewal and keeps receiving.
    f.publish_and_receive(&mut owner).await;
    let mut fresh = f.connect(&f.owner, &f.channels).await;
    f.publish_and_receive(&mut fresh).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn old_org_claim_cannot_authorize_new_or_existing_subscriptions() {
    let f = Fixture::new(false).await;
    let mut existing = f.connect(&f.reader, &[Channel::Org(f.org.id)]).await;
    // An already open, otherwise idle socket exercises fresh subscribe using
    // the old claims after a revocation whose writer has no eviction hook.
    let mut idle = f.connect(&f.reader, &[Channel::User(f.reader.id)]).await;
    sqlx::query("DELETE FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(f.org.id)
        .bind(f.reader.id)
        .execute(&f.app.db)
        .await
        .unwrap();
    idle.subscribe(&Channel::Org(f.org.id).to_string()).await;
    // Membership delivery guards close the old session before even a queued
    // subscription response can cross its revoked organization boundary.
    idle.expect_closed(SOON).await;
    existing.expect_closed(REVOCATION_BOUND).await;
    f.app.cleanup().await;
}

#[tokio::test]
async fn blocked_reauthorization_cannot_extend_the_lease() {
    let f = Fixture::new(false).await;
    let mut reader = f.connect(&f.reader, &f.channels).await;
    let mut lock = f.app.db.begin().await.unwrap();
    sqlx::query("LOCK TABLE workspaces IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    reader.expect_closed(REVOCATION_BOUND).await;
    lock.rollback().await.unwrap();
    for channel in &f.channels {
        assert_eq!(
            f.app
                .state
                .event_bus
                .ws_hub()
                .channel_subscriber_count(channel),
            0
        );
    }
    f.app.cleanup().await;
}
