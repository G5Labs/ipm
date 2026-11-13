//! Workbench timeline (workbench `g5.workbench.landing`): the Activity feed
//! projects phase moves (`workspace_phase_events`) and pull-request lifecycle
//! events (`workspace_pull_request_events`) as `phase_changed` and
//! `pull_request` entries, interleaved with the existing workspace context in
//! one chronological order, and never shows another workspace's or another
//! organization's rows.
//!
//! Each test spawns a full `TestApp` with an isolated Postgres database and
//! drives the real HTTP endpoints. No mocks.

use crate::helpers;

use chrono::{DateTime, Utc};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn activity_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/activity")
}

async fn activity(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Vec<Value> {
    let response = client.get(&activity_path(workspace_id)).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["entries"]
        .as_array()
        .expect("entries array")
        .clone()
}

fn of_kind<'a>(entries: &'a [Value], kind: &str) -> Vec<&'a Value> {
    entries
        .iter()
        .filter(|entry| entry["kind"] == json!(kind))
        .collect()
}

async fn insert_chat(app: &TestApp, workspace: &TestWorkspace, owner: Uuid, at: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions \
         (id, org_id, workspace_id, created_by, scope_type, title, message_count, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'workspace', 'Implementation chat', 4, $5::timestamptz, $5::timestamptz)",
    )
    .bind(id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(owner)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert chat");
    id
}

/// A pull request from `source` into `target`, as the create route records it.
async fn insert_pull_request(
    app: &TestApp,
    source: &TestWorkspace,
    target: &TestWorkspace,
    owner: Uuid,
    title: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_pull_requests \
         (id, org_id, source_workspace_id, target_workspace_id, status, local_merge_status, \
          provider_status, title, last_preflight_fingerprint, created_by) \
         VALUES ($1, $2, $3, $4, 'open', 'ready', 'local_only', $5, 'fp-test', $6)",
    )
    .bind(id)
    .bind(source.org_id)
    .bind(source.id)
    .bind(target.id)
    .bind(title)
    .bind(owner)
    .execute(&app.db)
    .await
    .expect("insert pull request");
    id
}

async fn insert_pull_request_event(
    app: &TestApp,
    org_id: Uuid,
    pull_request_id: Uuid,
    event_type: &str,
    actor: Option<Uuid>,
    message: Option<&str>,
    at: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspace_pull_request_events \
         (id, org_id, pull_request_id, event_type, actor_id, message, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)",
    )
    .bind(id)
    .bind(org_id)
    .bind(pull_request_id)
    .bind(event_type)
    .bind(actor)
    .bind(message)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert pull request event");
    id
}

async fn insert_provider_repo(
    app: &TestApp,
    org_id: Uuid,
    pull_request_id: Uuid,
    repo_id: Uuid,
    full_name: &str,
    number: i64,
) {
    sqlx::query(
        "INSERT INTO workspace_pull_request_provider_repos \
         (org_id, pull_request_id, repo_id, provider, provider_repo_full_name, \
          source_remote_branch, target_remote_branch, provider_pr_number, provider_pr_url, \
          provider_status) \
         VALUES ($1, $2, $3, 'github', $4, 'g5/source', 'main', $5, \
                 'https://github.com/' || $4 || '/pull/' || $5::text, 'open')",
    )
    .bind(org_id)
    .bind(pull_request_id)
    .bind(repo_id)
    .bind(full_name)
    .bind(number)
    .execute(&app.db)
    .await
    .expect("insert provider repo");
}

fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[tokio::test]
async fn a_phase_move_appears_as_a_phase_changed_entry_with_its_actor() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    assert!(
        of_kind(&activity(&client, workspace.id).await, "phase_changed").is_empty(),
        "a workspace that never moved has no phase entry"
    );

    let response = client
        .patch_json(
            &format!("/api/v1/workspaces/{}", workspace.id),
            &json!({ "phase": "se" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let entries = activity(&client, workspace.id).await;
    let moves = of_kind(&entries, "phase_changed");
    assert_eq!(moves.len(), 1, "{entries:?}");
    let entry = moves[0];
    assert_eq!(entry["fromPhase"], json!("be"));
    assert_eq!(entry["toPhase"], json!("se"));
    assert_eq!(entry["source"], json!("workspace"));
    assert_eq!(entry["actor"]["userId"], json!(owner.id.to_string()));
    assert_eq!(entry["actor"]["displayName"], json!(owner.display_name));
    let event_id: Uuid =
        sqlx::query_scalar("SELECT id FROM workspace_phase_events WHERE workspace_id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("the durable event row");
    assert_eq!(
        entry["key"],
        json!(format!("phase:{event_id}")),
        "the entry is keyed by the durable record it projects"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn pull_request_events_appear_with_their_pull_request_and_skip_sync_noise() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let pull_request = insert_pull_request(&app, &source, &target, owner.id, "Ship the fix").await;
    insert_provider_repo(&app, org.id, pull_request, source.repo_id, "acme/web", 77).await;
    let created = insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "created",
        Some(owner.id),
        None,
        "2026-03-01T10:00:00Z",
    )
    .await;
    // Sync bookkeeping: recorded, but not a reader-meaningful state change.
    insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "refreshed",
        Some(owner.id),
        None,
        "2026-03-01T10:01:00Z",
    )
    .await;
    insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "provider_updated",
        None,
        None,
        "2026-03-01T10:02:00Z",
    )
    .await;
    let blocked = insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "merge_blocked",
        None,
        Some("conflict in README.md"),
        "2026-03-01T10:03:00Z",
    )
    .await;

    let entries = activity(&client, source.id).await;
    let events = of_kind(&entries, "pull_request");
    assert_eq!(
        events
            .iter()
            .map(|entry| entry["eventType"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["merge_blocked", "created"],
        "{entries:?}"
    );
    // Newest first: the creation is the older of the two.
    let first = events[1];
    assert_eq!(first["key"], json!(format!("pull-request:{created}")));
    assert_eq!(first["pullRequestId"], json!(pull_request.to_string()));
    assert_eq!(first["title"], json!("Ship the fix"));
    assert_eq!(first["targetWorkspaceName"], json!(target.name));
    assert_eq!(first["repositoryCount"], json!(1));
    assert_eq!(first["providerRepoFullName"], json!("acme/web"));
    assert_eq!(first["providerPrNumber"], json!(77));
    assert_eq!(
        first["providerPrUrl"],
        json!("https://github.com/acme/web/pull/77")
    );
    assert_eq!(first["actor"]["displayName"], json!(owner.display_name));
    let second = events[0];
    assert_eq!(second["key"], json!(format!("pull-request:{blocked}")));
    assert_eq!(second["message"], json!("conflict in README.md"));
    assert_eq!(second["actor"], Value::Null, "a system event has no actor");

    // The target workspace's own feed does not carry its incoming PR events:
    // the projection follows the source workspace.
    assert!(of_kind(&activity(&client, target.id).await, "pull_request").is_empty());

    app.cleanup().await;
}

/// Provider lifecycle events are written per repository but identify only the
/// aggregate pull request, so a PR spanning two repositories cannot attribute
/// one provider PR to an event: the entry carries the repository count and
/// null provider fields instead of another repository's number and link.
#[tokio::test]
async fn a_multi_repository_pull_request_names_no_single_provider_pr() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let pull_request = insert_pull_request(&app, &source, &target, owner.id, "Two repos").await;
    insert_provider_repo(&app, org.id, pull_request, source.repo_id, "acme/web", 77).await;
    insert_provider_repo(&app, org.id, pull_request, target.repo_id, "acme/api", 9).await;
    insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "provider_failed",
        None,
        Some("push rejected"),
        "2026-03-01T10:00:00Z",
    )
    .await;

    let entries = activity(&client, source.id).await;
    let events = of_kind(&entries, "pull_request");
    assert_eq!(events.len(), 1, "{entries:?}");
    let entry = events[0];
    assert_eq!(entry["eventType"], json!("provider_failed"));
    assert_eq!(entry["title"], json!("Two repos"));
    assert_eq!(entry["repositoryCount"], json!(2));
    assert_eq!(entry["providerRepoFullName"], Value::Null);
    assert_eq!(entry["providerPrNumber"], Value::Null);
    assert_eq!(entry["providerPrUrl"], Value::Null);
    assert_eq!(entry["message"], json!("push rejected"));

    app.cleanup().await;
}

/// A completed publication: the full-flow `provider_updated` the PR owner
/// records after a successful create, update or automatic refresh.
async fn insert_completed_publication(
    app: &TestApp,
    org_id: Uuid,
    pull_request_id: Uuid,
    at: &str,
) -> Uuid {
    let id = insert_pull_request_event(
        app,
        org_id,
        pull_request_id,
        "provider_updated",
        None,
        None,
        at,
    )
    .await;
    sqlx::query(
        "UPDATE workspace_pull_request_events SET source_heads = '[]'::jsonb WHERE id = $1",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .expect("mark completed publication");
    id
}

/// Repeated publication failures read as one unresolved entry per pull
/// request: a later failure supersedes an earlier one, a completed
/// publication or the pull request's end resolves them, and the entry counts
/// the failures since the last completed publication (issue #519).
#[tokio::test]
async fn publication_failures_collapse_per_pull_request_until_resolved() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let pull_request = insert_pull_request(&app, &source, &target, owner.id, "Ship").await;
    insert_provider_repo(&app, org.id, pull_request, source.repo_id, "acme/web", 491).await;
    let fail = |message: &'static str, at: &'static str| {
        insert_pull_request_event(
            &app,
            org.id,
            pull_request,
            "provider_failed",
            None,
            Some(message),
            at,
        )
    };
    let failures = |entries: Vec<Value>| -> Vec<Value> {
        of_kind(&entries, "pull_request")
            .into_iter()
            .filter(|entry| entry["eventType"] == json!("provider_failed"))
            .cloned()
            .collect()
    };

    // Resolved by the completed publication that followed it.
    fail("first", "2026-05-01T09:11:00Z").await;
    insert_completed_publication(&app, org.id, pull_request, "2026-05-01T09:12:00Z").await;
    assert!(failures(activity(&client, source.id).await).is_empty());

    // Three failures since then, of alternating kinds, read as the newest.
    fail("CONFLICT", "2026-05-01T10:30:00Z").await;
    fail("INTERNAL_ERROR", "2026-05-01T10:31:00Z").await;
    let newest = fail("CONFLICT again", "2026-05-01T11:27:00Z").await;
    // A per-repository provider event is not a completed publication.
    insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "provider_updated",
        None,
        None,
        "2026-05-01T11:28:00Z",
    )
    .await;
    let shown = failures(activity(&client, source.id).await);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["key"], json!(format!("pull-request:{newest}")));
    assert_eq!(shown[0]["message"], json!("CONFLICT again"));
    assert_eq!(shown[0]["failureCount"], json!(3));
    assert_eq!(
        shown[0]["firstFailedAt"]
            .as_str()
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok()),
        DateTime::parse_from_rfc3339("2026-05-01T10:30:00Z").ok()
    );
    // Other lifecycle entries carry no failure count.
    let created = insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "created",
        None,
        None,
        "2026-05-01T09:00:00Z",
    )
    .await;
    let entries = activity(&client, source.id).await;
    let opened = of_kind(&entries, "pull_request")
        .into_iter()
        .find(|entry| entry["key"] == json!(format!("pull-request:{created}")))
        .expect("created entry");
    assert_eq!(opened["failureCount"], Value::Null);
    assert_eq!(opened["firstFailedAt"], Value::Null);

    // A later completed publication resolves them all.
    insert_completed_publication(&app, org.id, pull_request, "2026-05-01T11:40:00Z").await;
    assert!(failures(activity(&client, source.id).await).is_empty());

    // A failure the pull request outlived is not shown once it has ended.
    fail("last", "2026-05-01T12:00:00Z").await;
    assert_eq!(failures(activity(&client, source.id).await).len(), 1);
    sqlx::query("UPDATE workspace_pull_requests SET status = 'merged' WHERE id = $1")
        .bind(pull_request)
        .execute(&app.db)
        .await
        .expect("merge pull request");
    assert!(failures(activity(&client, source.id).await).is_empty());

    app.cleanup().await;
}

/// Mixed sources with shared timestamps across more than one page: walking the
/// cursor yields every entry exactly once, newest first within each page and
/// across pages (same contract as `api_external_issues` for tasks alone).
#[tokio::test]
async fn pages_walk_mixed_sources_without_duplicates_or_gaps() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let pull_request = insert_pull_request(&app, &source, &target, owner.id, "Ship").await;
    // 30 minutes; each minute holds a phase event and a PR event (same
    // created_at), and three of them a chat as well: 63 entries over a page
    // size of 50, with ties crossing the page boundary wherever it falls.
    let mut expected: Vec<String> = Vec::new();
    for minute in 0..30 {
        let at = format!("2026-04-01T10:{minute:02}:00Z");
        let phase_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspace_phase_events \
             (org_id, workspace_id, from_phase, to_phase, actor_user_id, created_at) \
             VALUES ($1, $2, 'be', 'se', $3, $4::timestamptz) RETURNING id",
        )
        .bind(org.id)
        .bind(source.id)
        .bind(owner.id)
        .bind(&at)
        .fetch_one(&app.db)
        .await
        .expect("seed phase event");
        expected.push(format!("phase:{phase_id}"));
        let event_id = insert_pull_request_event(
            &app,
            org.id,
            pull_request,
            if minute % 2 == 0 {
                "created"
            } else {
                "source_rebased"
            },
            Some(owner.id),
            None,
            &at,
        )
        .await;
        expected.push(format!("pull-request:{event_id}"));
        if minute % 10 == 0 {
            let chat = insert_chat(&app, &source, owner.id, &at).await;
            expected.push(format!("chat:{chat}"));
        }
    }
    assert_eq!(expected.len(), 63);

    let mut pages: Vec<Vec<Value>> = Vec::new();
    let mut before: Option<String> = None;
    loop {
        let path = match &before {
            Some(cursor) => format!("{}?before={}", activity_path(source.id), urlencode(cursor)),
            None => activity_path(source.id),
        };
        let response = client.get(&path).await;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        assert_eq!(status, StatusCode::OK, "{body}");
        let entries = body["data"]["entries"].as_array().expect("entries").clone();
        assert!(
            !entries.is_empty(),
            "a page before the beginning is never empty"
        );
        pages.push(entries);
        match body["data"]["earlierCursor"].as_str() {
            Some(cursor) => before = Some(cursor.to_string()),
            None => break,
        }
        assert!(
            pages.len() <= 3,
            "64 entries including workspace creation fit in two pages"
        );
    }
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].len(), 50, "the first page is full");
    assert_eq!(pages[1].len(), 14, "the second page holds the rest");

    // Newest first within a page and across pages (the later-fetched page is
    // the older one).
    let in_order: Vec<&Value> = pages.iter().flatten().collect();
    let times: Vec<&str> = in_order
        .iter()
        .map(|entry| entry["at"].as_str().unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] >= pair[1]), "{times:?}");
    let mut keys: Vec<String> = in_order
        .iter()
        .map(|entry| entry["key"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys.len(), 64, "no gaps");
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 64, "no duplicates");
    expected.push(format!("created:{}", source.id));
    expected.sort();
    assert_eq!(keys, expected, "exact membership");

    app.cleanup().await;
}

#[tokio::test]
async fn workbench_entries_interleave_with_existing_context_in_time_order() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let target = TestWorkspace::create(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let chat = insert_chat(&app, &source, owner.id, "2026-03-01T09:00:00Z").await;
    sqlx::query(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id, created_at) \
         VALUES ($1, $2, 'be', 'se', $3, '2026-03-01T10:00:00Z'::timestamptz)",
    )
    .bind(org.id)
    .bind(source.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("seed phase event");
    let pull_request = insert_pull_request(&app, &source, &target, owner.id, "Ship").await;
    insert_pull_request_event(
        &app,
        org.id,
        pull_request,
        "created",
        Some(owner.id),
        None,
        "2026-03-01T11:00:00Z",
    )
    .await;
    sqlx::query(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id, created_at) \
         VALUES ($1, $2, 'se', 'ie', $3, '2026-03-01T12:00:00Z'::timestamptz)",
    )
    .bind(org.id)
    .bind(source.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("seed phase event");

    let entries = activity(&client, source.id).await;
    let kinds: Vec<&str> = entries
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        // The workspace row itself is created now, after every seeded event.
        [
            "workspace_created",
            "phase_changed",
            "pull_request",
            "phase_changed",
            "chat"
        ],
        "{entries:?}"
    );
    assert_eq!(entries[4]["sessionId"], json!(chat.to_string()));
    let times: Vec<DateTime<Utc>> = entries
        .iter()
        .map(|entry| {
            DateTime::parse_from_rfc3339(entry["at"].as_str().unwrap())
                .unwrap()
                .with_timezone(&Utc)
        })
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] >= pair[1]));
    assert_eq!(entries[3]["toPhase"], json!("se"));
    assert_eq!(entries[1]["toPhase"], json!("ie"));

    app.cleanup().await;
}

/// A conversation sits at its last message, not its creation
/// (ui `g5.workbench.landing`): continuing an old chat moves it to the top,
/// and a page cursor taken after it skips it on the next page.
#[tokio::test]
async fn a_conversation_is_ordered_and_paged_by_its_last_activity() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let old_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T08:00:00Z").await;
    let later_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T09:00:00Z").await;
    let phase: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id, created_at) \
         VALUES ($1, $2, 'be', 'se', $3, '2026-03-01T10:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .fetch_one(&app.db)
    .await
    .expect("seed phase event");
    // The old chat continues after everything else.
    sqlx::query(
        "UPDATE chat_sessions SET updated_at = '2026-03-01T11:00:00Z'::timestamptz, \
         message_count = 6 WHERE id = $1",
    )
    .bind(old_chat)
    .execute(&app.db)
    .await
    .expect("continue the old chat");

    let entries = activity(&client, workspace.id).await;
    let keys: Vec<&str> = entries
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            // The workspace row itself is created now, after every seed.
            format!("created:{}", workspace.id),
            format!("chat:{old_chat}"),
            format!("phase:{phase}"),
            format!("chat:{later_chat}"),
        ],
        "{entries:?}"
    );
    assert_eq!(entries[1]["at"], json!("2026-03-01T11:00:00Z"));
    assert_eq!(entries[1]["lastActivityAt"], entries[1]["at"]);
    assert_eq!(entries[1]["messageCount"], json!(6));

    // A cursor at the continued chat's position pages past it.
    let cursor = format!("2026-03-01T11:00:00.000000Z|chat:{old_chat}");
    let response = client
        .get(&format!(
            "{}?before={}",
            activity_path(workspace.id),
            urlencode(&cursor)
        ))
        .await;
    let body: Value = response.json().await.unwrap_or(Value::Null);
    let older: Vec<&str> = body["data"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(older.len(), 2, "{body}");
    assert_eq!(older[1], format!("chat:{later_chat}"));

    app.cleanup().await;
}

fn current_path(workspace_id: Uuid) -> String {
    format!("{}/current", activity_path(workspace_id))
}

async fn current(client: &helpers::AuthenticatedClient, workspace_id: Uuid) -> Vec<Value> {
    let response = client.get(&current_path(workspace_id)).await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"]["entries"]
        .as_array()
        .expect("entries array")
        .clone()
}

/// `tasks` row; `chat_session` binds an agent task to its conversation.
#[allow(clippy::too_many_arguments)]
async fn insert_task(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner: Uuid,
    name: &str,
    kind: &str,
    status: &str,
    chat_session: Option<Uuid>,
    at: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks \
         (id, org_id, workspace_id, name, kind, status, chat_session_id, created_by, \
          created_at, started_at, ended_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz, \
                 CASE WHEN $6 IN ('queued') THEN NULL ELSE $9::timestamptz END, \
                 CASE WHEN $6 IN ('queued', 'running', 'waiting') THEN NULL \
                      ELSE $9::timestamptz + interval '1 minute' END)",
    )
    .bind(id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(name)
    .bind(kind)
    .bind(status)
    .bind(chat_session)
    .bind(owner)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert task");
    id
}

/// Ongoing work (ui `g5.ui.activity-current`): conversations with a queued,
/// running or waiting agent task and preview/command tasks in those states,
/// newest last activity first, each conversation carrying its task's status;
/// finished work and other workspaces' work are absent.
#[tokio::test]
async fn current_lists_ongoing_conversations_and_tasks_only() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    assert!(
        current(&client, workspace.id).await.is_empty(),
        "nothing runs yet"
    );

    let running_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T09:00:00Z").await;
    insert_task(
        &app,
        &workspace,
        owner.id,
        "turn",
        "agent",
        "running",
        Some(running_chat),
        "2026-03-01T09:00:00Z",
    )
    .await;
    let waiting_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T10:00:00Z").await;
    insert_task(
        &app,
        &workspace,
        owner.id,
        "turn",
        "agent",
        "waiting",
        Some(waiting_chat),
        "2026-03-01T10:00:00Z",
    )
    .await;
    let finished_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T12:00:00Z").await;
    insert_task(
        &app,
        &workspace,
        owner.id,
        "turn",
        "agent",
        "succeeded",
        Some(finished_chat),
        "2026-03-01T12:00:00Z",
    )
    .await;
    let idle_chat = insert_chat(&app, &workspace, owner.id, "2026-03-01T12:30:00Z").await;
    let preview = insert_task(
        &app,
        &workspace,
        owner.id,
        "Preview",
        "preview",
        "running",
        None,
        "2026-03-01T11:00:00Z",
    )
    .await;
    let queued = insert_task(
        &app,
        &workspace,
        owner.id,
        "cargo test",
        "command",
        "queued",
        None,
        "2026-03-01T08:00:00Z",
    )
    .await;
    insert_task(
        &app,
        &workspace,
        owner.id,
        "done",
        "command",
        "succeeded",
        None,
        "2026-03-01T13:00:00Z",
    )
    .await;
    let elsewhere = insert_chat(&app, &other, owner.id, "2026-03-01T14:00:00Z").await;
    insert_task(
        &app,
        &other,
        owner.id,
        "turn",
        "agent",
        "running",
        Some(elsewhere),
        "2026-03-01T14:00:00Z",
    )
    .await;

    let entries = current(&client, workspace.id).await;
    let keys: Vec<&str> = entries
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            format!("task:{preview}"),
            format!("chat:{waiting_chat}"),
            format!("chat:{running_chat}"),
            format!("task:{queued}"),
        ],
        "{entries:?}"
    );
    assert_eq!(entries[0]["status"], json!("running"));
    assert_eq!(entries[1]["taskStatus"], json!("waiting"));
    assert_eq!(entries[2]["taskStatus"], json!("running"));
    assert_eq!(entries[3]["status"], json!("queued"));
    assert!(
        !keys.iter().any(|key| key.ends_with(&idle_chat.to_string())
            || key.ends_with(&finished_chat.to_string())),
        "{entries:?}"
    );

    // The record still lists every conversation, without a task status.
    let record = activity(&client, workspace.id).await;
    let chats = of_kind(&record, "chat");
    assert_eq!(chats.len(), 4, "{record:?}");
    assert!(chats.iter().all(|chat| chat.get("taskStatus").is_none()));

    // Another organization's member cannot read it.
    let mut stranger = TestUser::create(&app).await;
    let _other_org = TestOrg::create_with_admin(&app, &mut stranger).await;
    let response = app
        .authenticated_client(&stranger)
        .get(&current_path(workspace.id))
        .await;
    assert!(
        matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "{}",
        response.status()
    );

    app.cleanup().await;
}

#[tokio::test]
async fn another_organizations_events_never_appear() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    let mut stranger = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut stranger).await;
    let other_target = TestWorkspace::create(&app, &other_org).await;
    let other_source = TestWorkspace::create(&app, &other_org).await;
    sqlx::query(
        "INSERT INTO workspace_phase_events \
         (org_id, workspace_id, from_phase, to_phase, actor_user_id) \
         VALUES ($1, $2, 'be', 'se', $3)",
    )
    .bind(other_org.id)
    .bind(other_source.id)
    .bind(stranger.id)
    .execute(&app.db)
    .await
    .expect("seed other org phase event");
    let other_pr = insert_pull_request(
        &app,
        &other_source,
        &other_target,
        stranger.id,
        "Other org PR",
    )
    .await;
    insert_pull_request_event(
        &app,
        other_org.id,
        other_pr,
        "created",
        Some(stranger.id),
        None,
        "2026-03-01T10:00:00Z",
    )
    .await;

    // The first organization's workspace sees none of it.
    let entries = activity(&client, workspace.id).await;
    assert!(of_kind(&entries, "phase_changed").is_empty(), "{entries:?}");
    assert!(of_kind(&entries, "pull_request").is_empty(), "{entries:?}");

    // And the other organization's workspace is not readable at all.
    let response = client.get(&activity_path(other_source.id)).await;
    assert!(
        matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "{}",
        response.status()
    );

    // Its own owner sees its own rows, so the rows exist and the filter is
    // tenant scoping, not a missing projection.
    let other_entries = activity(&app.authenticated_client(&stranger), other_source.id).await;
    assert_eq!(of_kind(&other_entries, "phase_changed").len(), 1);
    assert_eq!(of_kind(&other_entries, "pull_request").len(), 1);

    app.cleanup().await;
}

/// A preview `tasks` row with the spec and outcome the lifecycle writes, and
/// optionally the module's `invocation.started` and `module.preview.ready`
/// events, so the Activity projection can derive its stage and ending.
#[allow(clippy::too_many_arguments)]
async fn insert_preview_task(
    app: &TestApp,
    workspace: &TestWorkspace,
    owner: Uuid,
    status: &str,
    spec: Value,
    outcome: Option<Value>,
    at: &str,
    events: &[&str],
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks \
         (id, org_id, workspace_id, name, kind, status, spec, outcome, created_by, \
          created_at, started_at, ended_at) \
         VALUES ($1, $2, $3, 'Preview · full-stack', 'preview', $4, $5, $6, $7, $8::timestamptz, \
                 CASE WHEN $4 IN ('queued') THEN NULL ELSE $8::timestamptz END, \
                 CASE WHEN $4 IN ('queued', 'running', 'waiting') THEN NULL \
                      ELSE $8::timestamptz + interval '1 minute' END)",
    )
    .bind(id)
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(status)
    .bind(spec)
    .bind(outcome)
    .bind(owner)
    .bind(at)
    .execute(&app.db)
    .await
    .expect("insert preview task");
    for (seq, kind) in events.iter().enumerate() {
        sqlx::query(
            "INSERT INTO task_events(
                 id, org_id, task_id, entry_type, content, metadata, attempt, vm_id, seq)
             VALUES ($1, $2, $3, 'checkpoint', $4, jsonb_build_object('kind', $4), 1, 'vm-1', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace.org_id)
        .bind(id)
        .bind(kind)
        .bind(seq as i64)
        .execute(&app.db)
        .await
        .expect("insert task event");
    }
    id
}

/// A preview run's Activity row (ui `g5.ui.activity.preview-row`): its
/// profile and port, its stage while live (from the task's events and its
/// cancel request) and a structured ending afterwards (from its outcome);
/// command tasks carry no `preview` object.
#[tokio::test]
async fn preview_rows_carry_their_stage_and_ending() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let spec = json!({"preview": {"profile": "full-stack", "port": 3000}, "lifecycle": {}});
    let closing_spec = json!({"preview": {"profile": "full-stack", "port": 3000},
                              "lifecycle": {"cancel_requested": true}});

    let starting = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "running",
        spec.clone(),
        None,
        "2026-03-02T09:00:00Z",
        &[],
    )
    .await;
    let building = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "running",
        spec.clone(),
        None,
        "2026-03-02T09:10:00Z",
        &["invocation.started"],
    )
    .await;
    let serving = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "running",
        spec.clone(),
        None,
        "2026-03-02T09:20:00Z",
        &["invocation.started", "module.preview.ready"],
    )
    .await;
    let closing = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "running",
        closing_spec,
        None,
        "2026-03-02T09:30:00Z",
        &["invocation.started", "module.preview.ready"],
    )
    .await;
    let command = insert_task(
        &app,
        &workspace,
        owner.id,
        "cargo test",
        "command",
        "running",
        None,
        "2026-03-02T09:40:00Z",
    )
    .await;

    let entries = current(&client, workspace.id).await;
    let by_key = |id: Uuid| {
        entries
            .iter()
            .find(|entry| entry["key"] == json!(format!("task:{id}")))
            .cloned()
            .unwrap_or_else(|| panic!("task:{id} in {entries:?}"))
    };
    assert_eq!(by_key(starting)["preview"]["stage"], json!("starting"));
    assert_eq!(by_key(building)["preview"]["stage"], json!("building"));
    assert_eq!(by_key(serving)["preview"]["stage"], json!("serving"));
    assert_eq!(by_key(closing)["preview"]["stage"], json!("closing"));
    assert_eq!(by_key(serving)["preview"]["profile"], json!("full-stack"));
    assert_eq!(by_key(serving)["preview"]["port"], json!(3000));
    assert!(by_key(serving)["preview"].get("ending").is_none());
    assert_eq!(by_key(serving)["name"], json!("Preview · full-stack"));
    assert!(by_key(command).get("preview").is_none(), "{entries:?}");

    let closed = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "cancelled",
        spec.clone(),
        Some(
            json!({"status": "cancelled", "reason": "owner_cancel", "detail": {"close": "killed"}}),
        ),
        "2026-03-02T10:00:00Z",
        &["invocation.started", "module.preview.ready"],
    )
    .await;
    let idle = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "cancelled",
        spec.clone(),
        Some(json!({"status": "cancelled", "reason": "idle_stop"})),
        "2026-03-02T10:10:00Z",
        &[],
    )
    .await;
    let crashed = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "failed",
        spec.clone(),
        Some(
            json!({"status": "failed", "result": {"status": "app_exited", "app_exit_code": 7,
                    "stderr_tail": "boom", "stderr_tail_truncated": false, "message": "exited"}}),
        ),
        "2026-03-02T10:20:00Z",
        &["invocation.started"],
    )
    .await;
    let unadmitted = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "failed",
        spec.clone(),
        Some(
            json!({"status": "failed", "reason": "dispatch_admission_failed",
                    "detail": "Conflict: a preview is already running"}),
        ),
        "2026-03-02T10:30:00Z",
        &[],
    )
    .await;
    let exited = insert_preview_task(
        &app,
        &workspace,
        owner.id,
        "succeeded",
        spec,
        Some(json!({"status": "succeeded"})),
        "2026-03-02T10:40:00Z",
        &["invocation.started"],
    )
    .await;

    let record = activity(&client, workspace.id).await;
    let row = |id: Uuid| {
        record
            .iter()
            .find(|entry| entry["key"] == json!(format!("task:{id}")))
            .cloned()
            .unwrap_or_else(|| panic!("task:{id} in {record:?}"))
    };
    let ending = |id: Uuid| row(id)["preview"]["ending"].clone();
    assert!(row(closed)["preview"].get("stage").is_none());
    assert_eq!(ending(closed)["kind"], json!("closed"));
    assert_eq!(ending(closed)["close"], json!("killed"));
    assert_eq!(ending(idle)["kind"], json!("stopped"));
    assert_eq!(ending(idle)["reason"], json!("idle_stop"));
    assert_eq!(
        row(idle)["reason"],
        json!("idle_stop"),
        "the flat reason stays for command tasks' readers"
    );
    assert_eq!(ending(crashed)["kind"], json!("failed"));
    assert_eq!(ending(crashed)["cause"], json!("app_exited"));
    assert_eq!(ending(crashed)["exitCode"], json!(7));
    assert!(
        ending(crashed).get("stderrTail").is_none(),
        "the tail stays in the run log"
    );
    assert_eq!(ending(unadmitted)["kind"], json!("did_not_start"));
    assert_eq!(
        ending(unadmitted)["message"],
        json!("a preview is already running")
    );
    assert_eq!(ending(exited)["kind"], json!("ended"));
    // Ended runs are not Current.
    let live = current(&client, workspace.id).await;
    assert!(!live
        .iter()
        .any(|entry| entry["key"] == json!(format!("task:{closed}"))));
}
