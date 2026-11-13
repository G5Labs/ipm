---
name: system-pool-rls-on-workspaces
description: Raw SELECTs on `workspaces` over `state.system_pool` return no rows (tenant RLS); use authz.lookup_* functions or a scoped_db_for_system_org read.
metadata:
  type: reference
---

On the API's `system_pool`, a raw `SELECT ... FROM workspaces` (and joins on it) silently
returns no rows: `tenant_isolation` RLS filters by `authz.current_org_ids()`, which is empty
there. `workspace_vms` reads do work on it (push admission locks that row directly). Found
2026-10-19 while building the personal mount push (#624): the personal-kind check came back
`None`.

**How to apply:** inside a system-pool transaction, reach workspace rows through the
security-definer `authz.lookup_workspace_runtime_authority(...)` family, or open
`scoped_db_for_system_org(state, org_id, ...)` for the read. Route handlers must not run that
SQL themselves: `route_sql_guard` and `selected_org_scope_audit` (g5-api integration) fail;
move it into the owning core service. Related: [[rust-and-integration-test-setup]].
