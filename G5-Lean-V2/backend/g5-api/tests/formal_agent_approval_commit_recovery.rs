//! Durable coordinator facts converge after real committed-but-lost ACKs.
use super::*;
use g5_contracts::types::{AgentApprovalOutcome, ApprovalReviewIdentity};
use helpers::commit_ack_proxy::CommitAckProxy;

#[tokio::test]
async fn task_bind_candidate_seal_and_deletion_recover_one_effect_after_lost_commit_ack() {
    for phase in ["task", "artifact", "seal", "delete"] {
        let proxy = CommitAckProxy::start().await;
        let copy = proxy.clone();
        let mut config = helpers::test_config();
        config.tenant_database.max_connections = 12;
        let app = TestApp::spawn_with_tenant_transport(
            config,
            std::sync::Arc::new(move |url| copy.rewrite(url)),
        )
        .await;
        let f = Fixture::with_app(app).await;
        let review = f.request(None).await;
        f.reconcile().await;
        let mut runtime = None;
        if phase == "task" {
            let child = TestWorkspace {
                id: f.child(review).await,
                org_id: f.org.id,
                repo_id: f.source.repo_id,
                head_commit: f.source.head_commit.clone(),
                name: ApprovalReviewIdentity::new(review).workspace_branch(),
            };
            sqlx::query("DELETE FROM workspace_vms WHERE workspace_id=$1 AND vm_id IS NULL")
                .bind(child.id)
                .execute(&f.app.db)
                .await
                .unwrap();
            runtime =
                Some(helpers::TestWorkspaceRuntimePeer::attach(&f.app, &child, f.owner.id).await);
        } else {
            f.terminal_receipt(review, AgentApprovalOutcome::Approved)
                .await;
            if phase != "artifact" {
                f.reconcile().await;
            }
            if phase == "delete" {
                f.reconcile().await;
            }
        }
        let pattern = match phase {
            "task" => "INSERT INTO approval_request_tasks",
            "artifact" => "INSERT INTO agent_approval_artifacts",
            "seal" => "UPDATE agent_approval_reviews SET stage = 'effective'",
            _ => "UPDATE workspaces SET deleted_at",
        };
        let prior = proxy.drop_count();
        proxy.arm(pattern);
        f.reconcile().await;
        proxy.wait_for_drop(prior).await;
        let report = f.report(review).await;
        assert_eq!(
            report["stage"],
            match phase {
                "task" => "start",
                "artifact" => "seal",
                _ => "effective",
            },
            "{phase}: {report}"
        );
        if phase == "task" {
            f.reconcile().await;
        } else {
            if phase == "artifact" {
                f.reconcile().await;
            }
            if phase == "delete" {
                let retry = f
                    .app
                    .authenticated_client(&f.owner)
                    .post_json(
                        &format!(
                            "/api/v1/workspaces/{}/approvals/reviews/{review}/cleanup",
                            f.source.id
                        ),
                        &json!({}),
                    )
                    .await;
                assert_eq!(retry.status(), StatusCode::OK);
            }
            f.reconcile().await;
            assert_eq!(f.report(review).await["cleanupPending"], false, "{phase}");
        }
        let counts:(i64,i64,i64,i64)=sqlx::query_as(
            r#"
            SELECT (SELECT count(*) FROM tasks WHERE workspace_id = r.review_workspace_id),
                (SELECT count(*) FROM agent_approval_artifacts WHERE approval_request_id = r.approval_request_id),
                (SELECT count(*) FROM approval_statuses WHERE agent_review_id = r.id),
                (SELECT count(*) FROM workspaces WHERE id = r.review_workspace_id AND deleted_at IS NOT NULL AND deletion_operation_id = r.id)
            FROM agent_approval_reviews r WHERE r.id = $1
            "#,
        ).bind(review).fetch_one(&f.app.db).await.unwrap();
        assert_eq!(
            counts,
            if phase == "task" {
                (1, 0, 0, 0)
            } else {
                (1, 1, 1, 1)
            },
            "{phase}"
        );
        assert_eq!(proxy.drop_count(), prior + 1);
        drop(runtime);
        f.app.cleanup().await;
        drop(proxy);
    }
}
