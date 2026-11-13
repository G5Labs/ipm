# Workspace audience cutover (issue #1107)

The matching application and migration 149 remove workspace visibility and reader
invitations. Every ordinary workspace and Base becomes readable by eligible active
organization members. This includes existing chats, comments, artifacts and retained
PR history. Personal workspaces stay private to their owner and active organization
owners/admins; formal review runtimes keep their separate access restrictions.

## Preflight

No deployed-data inventory has been performed as part of this change. Before rollout,
run the following read-only inventory on the **pre-149** database using an operator
identity that can see all organizations. Verify that the identity is not silently
filtered by FORCE RLS. Keep the output in the deployment's access-controlled records;
it contains workspace identities and grants. Do not paste it into a public PR.

```sql
SELECT w.org_id, w.id, w.name,
       CASE WHEN w.agent_approval_review_id IS NOT NULL THEN 'formal_review'
            WHEN w.personal_user_id IS NOT NULL THEN 'personal'
            WHEN w.is_base THEN 'base' ELSE 'ordinary' END AS category,
       w.visibility, w.deleted_at IS NOT NULL AS deleted,
       w.owner_user_id, w.personal_user_id, w.agent_approval_review_id,
       (SELECT count(*) FROM workspace_members m WHERE m.workspace_id = w.id) AS reader_grants,
       (SELECT count(*) FROM workspace_pull_requests p
        WHERE p.source_workspace_id = w.id OR p.target_workspace_id = w.id) AS retained_prs
FROM workspaces w ORDER BY w.org_id, category, w.deleted_at NULLS FIRST, w.id;

SELECT w.org_id, w.id AS workspace_id, m.user_id, m.role
FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
ORDER BY w.org_id, w.id, m.user_id;
```

Assess every formerly invite-only ordinary/Base workspace, including deleted records.
Confirm that the requested organization audience applies to its existing content.
Surface any requirement to retain selective ordinary access: it conflicts with complete
removal and cannot be resolved by a hidden replacement setting. Check malformed or
unexpected personal/formal identities before migration; neither may be reclassified
as ordinary to make rollout pass.

## Cutover

1. Take and verify a recoverable database backup. Record the old and candidate releases.
2. Stop admission and drain/stop **all** old API instances and background processes.
   The ordinary Helm pre-upgrade migration while old replicas serve is insufficient:
   old queries reference the columns and table being removed. Pause automatic rollout
   until this coordinated window is arranged.
3. Apply the new migration with this release's `g5-db-migrate`. Historical migrations
   and checksums stay unchanged. A five-second DDL lock timeout rolls back the entire
   migration; end the blocker and retry. Do not disable checksum validation.
4. Start only the matching release. Confirm the retired table/column are absent and
   rerunning the migrator is a no-op. Verify with real ordinary, personal and formal
   fixtures: ordinary member read succeeds; another member's personal read and Git
   read fail; personal owner/admin read succeeds; formal runtime reads/mutations fail;
   retained evidence still observes source and target access. Old visibility patches
   and workspace-member requests must fail, while description and phase updates work.
5. Reopen traffic after those checks. Recheck event subscriptions, background recovery,
   personal creation/mounts and retained PR reads.

## Recovery limits

Before reopening traffic, restore the pre-cutover database and old application
together if needed. The dropped invitation rows are recoverable only from that backup;
do not reconstruct a partial old schema beside the new application. After reopening,
prefer forward repair. Restoring the old access model cannot undo information already
read during the wider audience. A successful migration or local test is not evidence
that deployed content was inventoried, that this cutover occurred, or that production
access behavior was observed.
