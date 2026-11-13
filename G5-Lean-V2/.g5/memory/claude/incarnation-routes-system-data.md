---
name: incarnation-routes-system-data
description: "Workspace-incarnation routes that touch system tables (orgs, users, email_deliveries) need a system-pool ScopedDb plus current_incarnation_on_system; the tenant-pool helper and plain system tx both fail."
metadata:
  node_type: memory
  type: project
  originSessionId: 6d51ceb7-e70d-4da7-9d88-d278ea06b47a
  modified: 2026-10-19T04:46:36.675Z
---

In g5-api, `scoped_db_for_workspace_incarnation` (routes/internal.rs) runs on the
**tenant pool**. The tenant role gets "permission denied" on system tables such as
`orgs`, `users` and `email_deliveries`. Seen 2026-10-19 building `g5d workspace email-owner`
(#621). Two other approaches fail too:

- **A plain `system_pool.begin()` transaction:** with no organization context,
  row-level security hides `workspaces` rows, so the lookups silently return nothing
  and the route answers 403.
- **`workspace_incarnation::current_incarnation(db, …, Share)` on a system ScopedDb:**
  the system role can't take `FOR SHARE` on `workspaces` ("permission denied for table
  workspaces"). The resulting 500 is **not logged** at the test's WARN level.

**Why:** the pools differ in role and privileges, and none of these failures name the
real cause at the HTTP level.

**How to apply:**

- **Route:** open `db::scoped::scoped_db_for_system_org_with(authz, &system_pool, org, scopes)`,
  then call `current_incarnation_on_system(db.conn(), claims, IncarnationLock::Share)`,
  a SECURITY DEFINER lookup.
- **Background code:** a gate on the bare system pool, such as an email `DeliveryGate`,
  must read workspaces through `authz.lookup_workspace_runtime_authority($1)`.
- **Debugging a silent 500:** call the service directly in a throwaway test and print
  the error.

Related: [[route-auth-static-audit-gotchas]].
