# Make Remote is an organization-administrator action (issue #1096)

Source intent: [G5Labs/G5-Lean#1096](https://github.com/G5Labs/G5-Lean/issues/1096).

## Problem

Any workspace owner can run **Make Remote** today. Make Remote binds a workspace
repository to an external provider branch, creates that branch on the provider when it
is missing, and re-points the repository's URL when asked
([g5.git.make-remote](../../ontology/software_architecture/git.g5.md#g5.git.make-remote),
[g5.git.make-remote-create](../../ontology/software_architecture/git.g5.md#g5.git.make-remote-create)).
That is a provider write and a change to the repository's shared binding, so it should
be an organization administrator's decision. Only the `git_url` re-point path is
admin-gated today (`maybe_adopt_remote_repoint`, `missing_scopes:
["org_admin_make_remote"]`); a non-admin owner can still bind to the repo's existing URL
and create a missing provider branch.

## Decision (owner, 2026-11-04)

Make Remote requires an organization **administrator**, and off Base additionally the
workspace **owner** ("block admin-but-not-owner"): on an ordinary workspace a non-admin
owner cannot and a non-owner admin cannot, so only an owner who is an admin can; on
**Base** an administrator may run it as its owner does, preserving the established
base-admin behavior (`g5.governance.pr.base-admin`, `g5.workspace.remote.base-admin-pull`)
and its tests. This keeps the acting identity the owner's off Base (no cross-owner
credential plumbing) and the acting administrator's on Base (its confirmed-admin gate).

The literal "block admin-but-not-owner" is read as targeting ordinary workspaces, not as
tearing up Base's base-admin rule; Base is the one place where non-owner-admin action is
the intended, pre-existing behavior. If Base should also be owner-only, that is a
one-line gate change (owner-only route gate) — flagged, not assumed.

### Tight abstraction: Publish is already a separate operation

The issue's premise that "the same promote endpoint also serves Publish" is outdated.
In the current code Publish and Make Remote are **separate endpoints and separate
service methods**:

- Make Remote (and the `git_url` re-point): `POST …/repos/:repoId/remote_tracking` →
  `promote_branch_repo_to_remote_tracking`.
- Publish: `POST …/repos/:repoId/publish` → `publish_branch_repo`, owner-gated
  (`ensure_workspace_mutable`), which pushes a new `g5/<branch>` ref to the repository's
  **existing** `git_url` only (no re-point). It cannot introduce a new external binding,
  so it cannot be used to bypass the Make Remote admin rule.

So `remote_tracking` is *always* Make Remote; no stored-state classification and no
client label are needed. This is the tight abstraction — the two operations stay
separate, each with its own authority.

### Enforcement (server is the boundary)

1. **Route** `promote_branch_repo_remote_tracking`: unchanged — the existing
   `scoped_db_for_owner_or_base_admin(RemoteTracking)` gate. `scoped_db_for_workspace`
   with `workspaces:write` already enforces current ownership
   (`require_current_workspace_owner`, since `scopes_require_current_workspace_owner`
   includes `workspaces:write`), so this gate admits exactly the **owner on any
   workspace** or a **non-owner administrator on Base**. A non-owner admin on an
   ordinary workspace is therefore already refused here.
2. **Service** `promote_branch_repo_to_remote_tracking` (the one new enforcement): after
   the clean/materialized/not-paused preflight and **before** `maybe_adopt_remote_repoint`
   and `resolve_remote_tip_for_row` (the first re-point and the first `ls-remote` /
   create-push), require `actor_is_org_admin(row.org_id, actor_user_id)`; otherwise
   return `Forbidden { missing_scopes: ["org_admin_make_remote"] }` with no provider I/O.
   Combined with the gate: ordinary workspace needs owner **and** admin, Base needs
   admin. Covers the existing-URL bind and the re-point alike, and every caller (UI, API,
   any agent/CLI path). The existing re-point admin check in `maybe_adopt_remote_repoint`
   stays as a narrower guard (now subsumed).

The acting provider identity is unchanged (`actor_user_id`): off Base the actor is the
owner; on Base the acting administrator, under the confirmed-admin gate. No credential
plumbing.

### UI

- `makeRemoteOffered` (workspaceLiveGate): always requires `orgAdmin`; off Base also the
  writable owner (`canWrite`), and on Base any administrator (`ownerOrBaseAdmin`, as
  today). **Hidden** otherwise (owner decision) — a non-admin owner and a non-owner admin
  on an ordinary workspace both lose the button; never on a review copy or before review
  identity is known.
- `PublishBranchMenu` (owner, `canWrite`) is unchanged.

## Ontology changes

- [g5.git.make-remote](../../ontology/software_architecture/git.g5.md#g5.git.make-remote)
  (I): administrator-and-owner intent on every workspace kind; Publish stays the owner's
  separate operation binding the inherited remote.
- [g5.git.remote-binding](../../ontology/software_architecture/git.g5.md#g5.git.remote-binding)
  (D): Make Remote and Publish are separate operations (Publish binds the existing URL,
  never a new one); the administrator rule is enforced before any provider I/O; acting
  identity is the owner's.
- [g5.git.base-create-exception](../../ontology/software_architecture/git.g5.md#g5.git.base-create-exception)
  (D): administrator-only everywhere, and additionally owner-only **off** Base; on Base a
  non-owner administrator may run it (base-admin), and Base keeps its confirm-before-push
  gate as its distinction.

## Assumptions / tensions

- [g5.workspace.base](../../ontology/features/workspaces.g5.md#g5.workspace.base) (owner
  is the only user who may modify a workspace's branch state, Base excepted) is preserved:
  **off Base** Make Remote now requires the owner, so no non-owner modifies the binding
  there; **on Base** a non-owner administrator may bind through the established base-admin
  path, which is exactly the Base exception that statement already carves out.

## Tests

`backend/g5-api/tests/make_remote_outcomes.rs` (+ UI tests):
- non-admin owner refused on Make Remote, existing-URL path, provider untouched;
- non-admin owner refused on Make Remote, `git_url` re-point path, provider untouched;
- non-owner admin refused on Make Remote (blocked before the service);
- owner-admin succeeds (existing-URL bind / create);
- Publish (`/publish`) by a non-admin owner still succeeds;
- UI: Make Remote hidden for a non-admin owner and for a non-owner admin; shown for an
  owner-admin; Publish shown for the owner.
