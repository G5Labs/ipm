//! Real API -> checked fork -> VM -> configured native provider fixture ->
//! confined MCP tools -> exact evidence -> seal -> stop/reap/delete.
//! The model is deterministic; the workflow and runtime owners are production.
use crate::helpers::api_client::TestApiClient;
use g5_contracts::{
    claude_models::CLAUDE_DIRECT_CHAT_DEFAULT_MODEL, codex_models::CODEX_DIRECT_CHAT_DEFAULT_MODEL,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use std::time::{Duration, Instant};
use uuid::Uuid;
fn data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

// Keep enough evidence in captured test output to diagnose a harness timeout,
// even when it fires before this phase's deadline or VM cleanup removes state.
async fn review_runtime_state(review_id: &Value) -> Value {
    let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
        let review_id = Uuid::parse_str(
            review_id
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("review identity missing"))?,
        )?;
        let mut db = PgConnection::connect(&crate::helpers::env::e2e_db_url()).await?;
        let mut inventory = db.begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *inventory)
            .await?;
        sqlx::query("SET LOCAL row_security = off")
            .execute(&mut *inventory)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '4s'")
            .execute(&mut *inventory)
            .await?;
        let state: Value = sqlx::query_scalar(
            "SELECT jsonb_build_object(
                'workspaceId', review.review_workspace_id,
                'vmStatus', vm.status, 'vmId', vm.vm_id, 'epoch', vm.epoch,
                'hostId', vm.host_id, 'leaseExpiresAt', vm.lease_expires_at,
                'startStatus', vm.start_intent -> 'status',
                'hostRefusals', vm.start_intent -> 'host_refusals',
                'vmError', vm.last_error,
                'taskId', review.task_id, 'taskStatus', task.status,
                'attempt', task.attempt, 'startConfirmed', task.start_confirmed,
                'taskUpdatedAt', task.updated_at,
                'inputs', (SELECT jsonb_agg(jsonb_build_object(
                    'id', input.id, 'delivery', input.delivery,
                    'deliveredAt', input.delivered_at))
                    FROM session_inputs input WHERE input.task_id = task.id))
             FROM agent_approval_reviews review
             LEFT JOIN workspace_vms vm ON vm.workspace_id = review.review_workspace_id
             LEFT JOIN tasks task ON task.id = review.task_id
             WHERE review.id = $1",
        )
        .bind(review_id)
        .fetch_one(&mut *inventory)
        .await?;
        inventory.rollback().await?;
        Ok::<Value, anyhow::Error>(state)
    })
    .await;
    match snapshot {
        Ok(Ok(state)) => state,
        Ok(Err(error)) => json!({"diagnosticError": error.to_string()}),
        Err(_) => json!({"diagnosticError": "runtime snapshot timed out"}),
    }
}

/// Settlement and resource retirement have independent, bounded contracts.
async fn wait_for_report(
    api: &TestApiClient,
    report_path: &str,
    phase: &str,
    budget: Duration,
    ready: impl Fn(&Value) -> anyhow::Result<bool>,
) -> anyhow::Result<Value> {
    let started = Instant::now();
    let mut last_logged_at = None;
    let mut last_logged_state = Value::Null;
    let mut last_state = json!({"stage": "not_observed"});
    // A diagnostic must not postpone observing a ready report. Keep at most
    // one in flight; dropping the set cancels it when this phase returns.
    let mut diagnostics = tokio::task::JoinSet::new();
    eprintln!(
        "formal review waiting: phase={phase}, api={}, budget={budget:?}",
        api.base_url()
    );
    tokio::time::timeout(budget, async {
        loop {
            let report = api
                .authed_get(report_path)
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let report = data(&report);
            last_state = json!({
                "reviewId": report["reviewId"],
                "stage": report["stage"],
                "result": report["result"],
                "startedAt": report["startedAt"],
                "endedAt": report["endedAt"],
                "failureStage": report["failureStage"],
                "failureCause": report["failureCause"],
                "cleanupPending": report["cleanupPending"],
                "cleanupError": report["cleanupError"],
                "cleanupOverdue": report["cleanupOverdue"],
            });
            let elapsed = started.elapsed();
            if last_logged_state != last_state
                || last_logged_at.is_none_or(|at| elapsed.saturating_sub(at) >= Duration::from_secs(30))
            {
                eprintln!(
                    "formal review progress: phase={phase}, elapsed={elapsed:?}, report={last_state}"
                );
                while diagnostics.try_join_next().is_some() {}
                if diagnostics.is_empty() {
                    let review_id = report["reviewId"].clone();
                    let phase = phase.to_owned();
                    diagnostics.spawn(async move {
                        let runtime = review_runtime_state(&review_id).await;
                        eprintln!(
                            "formal review runtime: phase={phase}, elapsed={elapsed:?}, review={review_id}, runtime={runtime}"
                        );
                    });
                }
                last_logged_at = Some(elapsed);
                last_logged_state = last_state.clone();
            }
            anyhow::ensure!(
                !["failed", "cancelled"].contains(&report["stage"].as_str().unwrap_or("")),
                "formal review failed: {report}"
            );
            if ready(report)? {
                return Ok::<Value, anyhow::Error>(report.clone());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!("formal review {phase} deadline expired; last state: {last_state}")
    })?
}

#[tokio::test]
async fn explicit_formal_reviews_keep_verdict_and_evidence_after_isolated_resources_disappear(
) -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    let org = api.use_seeded_org().await?;
    // These synthetic provider transports use ready API-key credentials.
    // Codex otherwise defaults to subscription login, absent in a fresh VM.
    let config_path = format!("{}/api/v1/orgs/{}/config", api.base_url(), org.id);
    let original: Value = api
        .authed_get(&config_path)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let original = data(&original);
    let subscription = original["settings"].get("codex_subscription_enabled");
    let added_key = !original["secretKeys"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("organization secret inventory missing"))?
        .iter()
        .any(|key| key == "OPENAI_API_KEY");
    let mut setup = json!({"settings":{"codex_subscription_enabled":false}});
    if added_key {
        setup["setSecrets"] = json!({"OPENAI_API_KEY":"sk-e2e-formal-fixture"});
    }
    api.authed_put(&config_path)
        .json(&setup)
        .send()
        .await?
        .error_for_status()?;
    let result = run_formal_reviews(&api, org.id).await;
    // Restore only fields this fixture owns, including when a case fails.
    let mut restore = match subscription {
        Some(value) => json!({"settings":{"codex_subscription_enabled":value}}),
        None => json!({"removeSettingKeys":["codex_subscription_enabled"]}),
    };
    if added_key {
        restore["removeSecretKeys"] = json!(["OPENAI_API_KEY"]);
    }
    api.authed_put(&config_path)
        .json(&restore)
        .send()
        .await?
        .error_for_status()?;
    result
}

async fn run_formal_reviews(api: &TestApiClient, org_id: Uuid) -> anyhow::Result<()> {
    let base = api
        .list_workspaces(org_id)
        .await?
        .into_iter()
        .find(|w| w.is_base)
        .ok_or_else(|| anyhow::anyhow!("seeded Base missing"))?;
    let target = api.create_workspace(org_id, "formal-review-target").await?;
    for provider in ["claude_code", "codex"] {
        for verdict in ["approved", "rejected"] {
            eprintln!(
                "formal review preparing {provider}/{verdict}: api={}, org={org_id}, target={}",
                api.base_url(),
                target.id
            );
            let source_name = format!("formal-source-{}", Uuid::new_v4().simple());
            // Exercise ordinary parent inheritance and explicit override precedence
            // through both native providers without adding more VM executions.
            let inherits_target = verdict == "approved";
            let parent = if inherits_target {
                target.id
            } else {
                Uuid::parse_str(&base.id)?
            };
            let mut clone_request = json!({
                "name": source_name,
                "idempotencyKey": Uuid::new_v4(),
                "source": "branch",
            });
            if !inherits_target {
                clone_request["mergeTargetWorkspaceId"] = json!(target.id);
            }
            let cloned = api
                .authed_post(&format!(
                    "{}/api/v1/workspaces/{parent}/clone",
                    api.base_url()
                ))
                .json(&clone_request)
                .send()
                .await?
                .error_for_status()?;
            let source: Value = cloned.json().await?;
            let source_id = data(&source)["workspace"]["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("source id missing: {source}"))?
                .to_string();
            let before = api
                .authed_get(&format!("{}/api/v1/workspaces/{source_id}", api.base_url()))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let target_before = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{}",
                    api.base_url(),
                    target.id
                ))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let model = if provider == "codex" {
                CODEX_DIRECT_CHAT_DEFAULT_MODEL
            } else {
                CLAUDE_DIRECT_CHAT_DEFAULT_MODEL
            };
            let definition_response=api.authed_post(&format!("{}/api/v1/orgs/{}/approval-definitions",api.base_url(),org_id)).json(&json!({"name":format!("Formal {provider} {verdict} {}",Uuid::new_v4()),"agentType":"simple","agentHarness":provider,"agentModel":model,"agentEffort":"high","agentPrompt":format!("G5_FORMAL_APPROVAL_E2E:{verdict}\nExercise the synthetic provider transport over the exact assigned source/base objects."),"phases":["ie"]})).send().await?;
            let definition_status = definition_response.status();
            let definition: Value = definition_response.json().await?;
            anyhow::ensure!(
                definition_status.is_success(),
                "definition admission failed: {definition_status} {definition}"
            );
            anyhow::ensure!(
                data(&definition)["agentHarness"] == provider
                    && data(&definition)["agentModel"] == model,
                "definition did not freeze the selected provider/model: {definition}"
            );
            let definition_id = data(&definition)["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("definition id missing"))?;
            api.authed_post(&format!(
                "{}/api/v1/workspaces/{}/approvals/requirements",
                api.base_url(),
                target.id
            ))
            .json(&json!({"definitionId":definition_id}))
            .send()
            .await?
            .error_for_status()?;
            let request = json!({"definitionId":definition_id,"definitionVersion":1,"phase":"ie"});
            let path = format!(
                "{}/api/v1/workspaces/{source_id}/approvals/reviews",
                api.base_url()
            );
            let admitted = api
                .authed_post(&path)
                .json(&request)
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let request_id = data(&admitted)["requestId"].clone();
            let review = data(&admitted)["reviewId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("review id missing"))?;
            anyhow::ensure!(
                request_id != review,
                "attempt identity must differ from its request"
            );
            // Identical admission retries reuse this active attempt.
            let replay = api
                .authed_post(&path)
                .json(&request)
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            anyhow::ensure!(
                data(&replay)["requestId"] == request_id && data(&replay)["reviewId"] == review,
                "request retry duplicated review"
            );
            let report_path = format!("{path}/{review}/report");
            // Capacity/VM readiness and durable task acceptance have their
            // own workflow budgets; neither consumes the synthetic run check.
            wait_for_report(
                api,
                &report_path,
                &format!("{provider}/{verdict} capture"),
                Duration::from_secs(5 * 60),
                |report| {
                    Ok(["start", "execute", "harvest", "seal", "effective"]
                        .contains(&report["stage"].as_str().unwrap_or("")))
                },
            )
            .await?;
            wait_for_report(
                api,
                &report_path,
                &format!("{provider}/{verdict} readiness"),
                Duration::from_secs(15 * 60),
                |report| {
                    Ok(["execute", "harvest", "seal", "effective"]
                        .contains(&report["stage"].as_str().unwrap_or("")))
                },
            )
            .await?;
            let effective = wait_for_report(
                api,
                &report_path,
                &format!("{provider}/{verdict} effectiveness"),
                Duration::from_secs(240),
                |report| Ok(report["stage"] == "effective"),
            )
            .await?;
            anyhow::ensure!(
                effective["result"] == verdict,
                "wrong effective decision: {effective}"
            );
            // A slow execution must not consume the separate five-minute
            // cleanup-attempt budget, or hide a change to a sealed verdict.
            let report = wait_for_report(
                api,
                &report_path,
                &format!("{provider}/{verdict} cleanup"),
                Duration::from_secs(300),
                |report| {
                    anyhow::ensure!(
                        report["stage"] == "effective"
                            && report["result"] == effective["result"]
                            && report["sha256"] == effective["sha256"]
                            && report["assignment"] == effective["assignment"],
                        "cleanup changed sealed review evidence: {report}"
                    );
                    Ok(report["cleanupPending"] == false)
                },
            )
            .await?;
            anyhow::ensure!(
                report["result"] == verdict,
                "wrong substantive decision: {report}"
            );
            let document = report["document"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("retained report missing"))?;
            anyhow::ensure!(
                report["sha256"] == format!("{:x}", Sha256::digest(document.as_bytes())),
                "exact report digest mismatch"
            );
            anyhow::ensure!(
                report["assignment"]["resolution"]["sourceWorkspaceId"] == source_id,
                "source identity retargeted"
            );
            anyhow::ensure!(
                report["assignment"]["resolution"]["targetWorkspaceId"] == target.id.to_string(),
                "formal assignment changed the resolved merge target"
            );
            anyhow::ensure!(
                report["execution"]["launch"]["resolved"]["provider"]["value"] == provider,
                "configured provider changed: {report}"
            );
            if provider == "codex" {
                anyhow::ensure!(
                    report["execution"]["launch"]["resolved"]["authMode"] == "open_ai_api_key",
                    "synthetic Codex fixture did not select its API-key route"
                );
            }
            let resolution = &report["assignment"]["resolution"];
            anyhow::ensure!(
                resolution["schemaVersion"] == 6,
                "new review used an old assignment contract"
            );
            let captured: g5_contracts::types::ApprovalReviewResolutionV2 =
                serde_json::from_value(resolution.clone())?;
            captured.validate().map_err(anyhow::Error::msg)?;
            g5_contracts::types::validate_agent_approval_markdown_v2(
                document.as_bytes(),
                &captured,
            )
            .map_err(anyhow::Error::msg)?;
            let names = resolution["repositoryNames"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("repository names missing"))?;
            for head in resolution["sourceHeads"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("source heads missing"))?
            {
                let repo_id = head["repoId"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("repo identity missing"))?;
                anyhow::ensure!(
                    names
                        .get(repo_id)
                        .and_then(Value::as_str)
                        .is_some_and(|name| !name.trim().is_empty()),
                    "readable name missing for {repo_id}"
                );
            }
            let yaml = document
                .strip_prefix("---\n")
                .and_then(|text| text.split_once("\n---").map(|(frontmatter, _)| frontmatter))
                .ok_or_else(|| anyhow::anyhow!("report frontmatter missing"))?;
            let frontmatter: Value = serde_yaml::from_str(yaml)?;
            anyhow::ensure!(
                frontmatter["schemaVersion"] == 6
                    && frontmatter["workspaceBase"].is_object()
                    && frontmatter["target"].is_object()
                    && frontmatter["mergeBase"] == resolution["mergeBase"]
                    && frontmatter["assignedAt"] == resolution["assignedAt"]
                    && frontmatter["submittedAt"].is_string(),
                "report lost frozen comparison or platform timestamps"
            );
            // Schemas 5 and 6: the agent authors a separate one-line summary of at
            // most 250 characters (g5.agent-approval.assignment).
            let summary = frontmatter["summary"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("report summary missing: {frontmatter}"))?;
            anyhow::ensure!(
                !summary.trim().is_empty()
                    && summary.chars().count() <= 250
                    && !summary.contains('\n'),
                "report summary violates its contract: {summary:?}"
            );
            let review_uuid = Uuid::parse_str(review)?;
            let child =
                g5_contracts::types::ApprovalReviewIdentity::new(review_uuid).workspace_id();
            // Human denial alone cannot distinguish an active private child
            // from a deleted one. Inspect through the test operator, refusing
            // silent RLS filtering if that connection lacks inventory authority.
            let mut db = PgConnection::connect(&crate::helpers::env::e2e_db_url()).await?;
            let mut inventory = db.begin().await?;
            sqlx::query("SET TRANSACTION READ ONLY")
                .execute(&mut *inventory)
                .await?;
            sqlx::query("SET LOCAL row_security = off")
                .execute(&mut *inventory)
                .await?;
            let topology: (Uuid, Option<Uuid>) = sqlx::query_as(
                "SELECT cloned_from_id,merge_target_workspace_id FROM workspaces WHERE id=$1",
            )
            .bind(Uuid::parse_str(&source_id)?)
            .fetch_one(&mut *inventory)
            .await?;
            anyhow::ensure!(
                topology == (parent, (!inherits_target).then_some(target.id)),
                "source did not preserve the tested parent/override configuration: {topology:?}"
            );
            let removed: (bool, bool) = sqlx::query_as(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=$1 AND org_id=$2 AND agent_approval_review_id=$3 AND deleted_at IS NOT NULL),
                        NOT EXISTS(SELECT 1 FROM workspace_vms WHERE workspace_id=$1)",
            )
            .bind(child)
            .bind(org_id)
            .bind(review_uuid)
            .fetch_one(&mut *inventory)
            .await?;
            anyhow::ensure!(
                removed == (true, true),
                "review resources remain: tombstoned={}, VM absent={}",
                removed.0,
                removed.1
            );
            inventory.rollback().await?;
            let mut cursor = None;
            let mut saw_input = false;
            let mut saw_event = false;
            for _ in 0..20 {
                let mut request = api
                    .authed_get(&format!("{path}/{review}/transcript"))
                    .query(&[("limit", "50")]);
                if let Some(cursor) = &cursor {
                    request = request.query(&[("cursor", cursor)]);
                }
                let response = request.send().await?.error_for_status()?;
                let bytes = response.bytes().await?;
                anyhow::ensure!(
                    bytes.len() <= 1024 * 1024,
                    "transcript page exceeds byte budget"
                );
                let page: Value = serde_json::from_slice(&bytes)?;
                for item in data(&page)["items"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("transcript items missing"))?
                {
                    saw_input |= item["kind"] == "input"
                        && item["content"] == report["assignment"]["userPrompt"];
                    saw_event |= item["kind"] == "event";
                }
                cursor = data(&page)["nextCursor"].as_str().map(str::to_string);
                if cursor.is_none() {
                    break;
                }
            }
            anyhow::ensure!(
                saw_input && saw_event,
                "post-cleanup transcript lost the accepted prompt or task events"
            );
            for repo in report["assignment"]["resolution"]["sourceHeads"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("source vector missing"))?
            {
                let repo = repo["repoId"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("repo id missing"))?;
                for (side, heads, sha_key) in [
                    ("source", &resolution["sourceHeads"], "headCommit"),
                    (
                        "workspaceBase",
                        &resolution["workspaceBaseHeads"],
                        "headCommit",
                    ),
                    ("parent", &resolution["targetHeads"], "headCommit"),
                    ("base", &resolution["mergeBase"]["repositories"], "gitSha"),
                ] {
                    let expected = heads
                        .as_array()
                        .and_then(|heads| heads.iter().find(|head| head["repoId"] == repo))
                        .and_then(|head| head[sha_key].as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("assigned {side} commit missing for {repo}")
                        })?;
                    let commits: Value = api
                        .authed_get(&format!("{path}/{review}/commits"))
                        .query(&[("repoId", repo), ("side", side), ("operation", "paths")])
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    anyhow::ensure!(
                        data(&commits)["repoId"] == repo
                            && data(&commits)["commit"] == expected
                            // An available empty Git tree has no paths.
                            && data(&commits)["content"].is_string(),
                        "post-cleanup {side} commits unavailable: {commits}"
                    );
                }
            }
            for suffix in ["", "/vm", "/tasks", "/sessions", "/settings"] {
                let response = api
                    .authed_get(&format!(
                        "{}/api/v1/workspaces/{child}{suffix}",
                        api.base_url()
                    ))
                    .send()
                    .await?;
                anyhow::ensure!(
                    !response.status().is_success(),
                    "owner/admin inspected temporary workspace through {suffix}"
                );
            }
            let after = api
                .authed_get(&format!("{}/api/v1/workspaces/{source_id}", api.base_url()))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let target_after = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{}",
                    api.base_url(),
                    target.id
                ))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            anyhow::ensure!(
                data(&before)["branches"] == data(&after)["branches"],
                "review changed source branches"
            );
            anyhow::ensure!(
                data(&target_before)["branches"] == data(&target_after)["branches"],
                "review changed target branches"
            );
            let history = api
                .authed_get(&format!(
                    "{}/api/v1/workspaces/{source_id}/approvals",
                    api.base_url()
                ))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            anyhow::ensure!(
                history.to_string().contains(review),
                "history lost the deleted review's evidence"
            );
            // The decision exposes the exact authored summary, never a report
            // clip (g5.agent-approval.evidence).
            anyhow::ensure!(
                history
                    .to_string()
                    .contains(&serde_json::to_string(summary)?),
                "decision summary differs from the report's authored summary"
            );
        }
    }
    Ok(())
}
