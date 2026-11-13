---
name: live-workspaces-need-member-owner
description: Every live workspace (Base included) needs an owner who is an active owner/admin/member of its org; seeding an ownerless Base fails on workspaces_owner_same_org_fk.
metadata:
  type: reference
---

`g5_resolve_eligible_workspace_creator` (migration 003) sets `owner_user_id := created_by`,
`workspaces_active_owner_required` (008) requires it on live workspaces, and the deferred
`workspaces_owner_same_org_fk` needs an owner-eligible, non-deactivated `org_memberships`
row (099). A SQL seed that creates a Base as a membership-less service user fails at
commit. Found 2026-10-24 on #776: the operator-org seed keeps `worker@g5.internal` as a
`member` only so it can own Base.

**How to apply:** when seeding orgs by SQL, give the Base creator an owner-eligible
membership; don't plan "ownerless Base" without relaxing those constraints. Related:
[[issue-776-platform-operator]].
