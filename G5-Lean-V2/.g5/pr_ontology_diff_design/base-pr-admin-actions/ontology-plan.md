# Organization administrators act on Base pull requests

## Ontology changes and governance

Comparison base: `origin/main` at `795d1ab6` (the branch's ontology matches it in these files). Intent: [request](../../chat_intent/base-pr-admin-actions/request.md).

Changed blocks:

- New [`g5.governance.pr.base-admin`](../../ontology/features/governance.g5.md#organization-administrators-act-on-pull-requests-into-base), linked from the story `g5.story.merge-authorized-work`.
- [`g5.workspace.ui.reader-publication`](../../ontology/features/workspaces.g5.md#publication-controls-are-the-owners) names the exception and records the implementation gap.

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 26e93097..2822da9b 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -60,7 +60,7 @@ Governing contracts: [approval is a human authority](governance.g5.md#approval-i
 
 As an authorized owner, I can merge reviewed work without a retry applying it twice.
 
-Governing contracts: [merge is a recoverable application of pinned intent](governance.g5.md#merge-is-a-recoverable-application-of-pinned-intent), [organization administrators administer approval requirements](#organization-administrators-administer-approval-requirements).
+Governing contracts: [merge is a recoverable application of pinned intent](governance.g5.md#merge-is-a-recoverable-application-of-pinned-intent), [organization administrators administer approval requirements](#organization-administrators-administer-approval-requirements), [organization administrators act on pull requests into Base](#organization-administrators-act-on-pull-requests-into-base).
 
 ### Review an updated source
 
@@ -466,6 +466,16 @@ Workspace-target merge requires its owner’s barrier authorization and exposes
 
 Apply the [failure and retry principle](../concepts/system_design_principles.g5.md#make-failures-and-retries-explicit).
 
+### Organization administrators act on pull requests into Base
+
+[D · g5.governance.pr.base-admin]
+
+An administrator of [Base](workspaces.g5.md#keep-base-workspaces-as-protected-readers)'s organization may refresh, close and merge open pull requests whose target is Base, as Base's owner may, independently of ownership. The rule covers only Base as the pull request's target: pull requests into other workspaces stay their owner's, and a pull request's source side keeps its own authority.
+
+A merge keeps the same [merge admission](#organization-administrators-administer-approval-requirements) and runs under the [acting administrator's provider authority](#publish-under-the-acting-humans-provider-authority), attributed to them. The rule confers no Base content-write, [live compute](../concepts/security.g5.md#one-eligible-owner-uses-live-compute), remote-binding or source-side publication authority and never reaches another organization's workspaces.
+
+Reason: Base has no VM to barrier and its owner is a durable control-plane record (hidden from Base presentation), so integrating into the organization's root is organization administration, like [requirement administration](#organization-administrators-administer-approval-requirements) and [Base transfer](../software_architecture/lifecycle.g5.md#preserve-administrator-transfer-of-a-base-workspace).
+
 ## Retained remote integration
 
 ### Bind provider synchronization explicitly
@@ -804,6 +814,12 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 - From [C142–C143](../../uplift/g5-lean/candidates/contracts.md).
 - ~50% · legacy merge service; redesigned schema/crash recovery unassessed.
 
+### g5.governance.pr.base-admin
+
+- human-provided, human-verified
+- Showing the Show All PRs actions to organization administrators on Base is human-provided ([request](../../chat_intent/base-pr-admin-actions/request.md)); the matching server authority, its Base-target scope and the retained admission, provider authority and exclusions are human-verified inference recorded there. Design: [plan](../../pr_ontology_diff_design/base-pr-admin-actions/ontology-plan.md).
+- Gap · not implemented: the merge, refresh and close handlers in `backend/g5-api-routes/src/routes/workspaces.rs` open `workspaces:write` scopes and call `ensure_workspace_owner`, and `PullRequestList.tsx` offers the actions to the owner only.
+
 ### g5.governance.remote.binding
 
 - agent
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 9bb3994f..c76dd993 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -528,7 +528,7 @@ The banner and the VM strip name the reported state in its own words and offer o
 
 [I · g5.workspace.ui.reader-publication]
 
-Create pull request, Rebase & Merge, Prepare to ship, Fetch and Push to remote, auto-pull and auto-push, Retry sync, Approve rebased publication, pull-request refresh, close and merge, Publish branch and Make remote are offered only to the current owner. The server refuses them to anyone else, so a reader does not see them. Synchronizing an existing remote binding (Fetch, Push, auto-pull, auto-push, Retry sync, Approve rebased publication) and acting on pull requests that target a workspace check ownership alone, so an owner keeps them under a read-only or freezing policy and Base's owner keeps its fetch controls; source-side publication (Create pull request, Rebase & Merge, Prepare to ship, Publish branch, Make remote) also needs a writable source.
+Create pull request, Rebase & Merge, Prepare to ship, Fetch and Push to remote, auto-pull and auto-push, Retry sync, Approve rebased publication, pull-request refresh, close and merge, Publish branch and Make remote are offered only to the current owner. The server refuses them to anyone else, so a reader does not see them. The one exception: an organization administrator also gets refresh, close and merge on pull requests into Base, because the server [admits them](governance.g5.md#organization-administrators-act-on-pull-requests-into-base). Synchronizing an existing remote binding (Fetch, Push, auto-pull, auto-push, Retry sync, Approve rebased publication) and acting on pull requests that target a workspace check ownership alone (plus the Base administrator exception), so an owner keeps them under a read-only or freezing policy and Base's owner keeps its fetch controls; source-side publication (Create pull request, Rebase & Merge, Prepare to ship, Publish branch, Make remote) also needs a writable source.
 
 ### Show the current review copy in the switcher
 
@@ -995,7 +995,7 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 
 - human-provided
 - Decision sheet policy "Publication controls for readers": hide them ([request](../../chat_intent/workspace-button-gates/request.md)).
-- Implemented · `ReposBar` and Prepare to ship take the owner gates in `WorkspacePage.tsx`; `BranchActionsMenu.tsx`, `MakeRemoteDialog.tsx`, `RemoteTrackingActions.tsx`, `BranchAutoPullControl.tsx` and `PullRequestList.tsx` read the gate and render nothing for a reader (Base keeps pull controls for its owner). `PullRequestList.test.tsx` covers the reader case; the Details chips are inspected only ([review](../../ontology-review-workspace-button-gates.md)).
+- Implemented · `ReposBar` and Prepare to ship take the owner gates in `WorkspacePage.tsx`; `BranchActionsMenu.tsx`, `MakeRemoteDialog.tsx`, `RemoteTrackingActions.tsx`, `BranchAutoPullControl.tsx` and `PullRequestList.tsx` read the gate and render nothing for a reader (Base keeps pull controls for its owner). `PullRequestList.test.tsx` covers the reader case; the Details chips are inspected only ([review](../../ontology-review-workspace-button-gates.md)). Gap · the Base administrator exception is not implemented; `PullRequestList.tsx` still gates on `gate.owner` ([request](../../chat_intent/base-pr-admin-actions/request.md)).
 
 ### g5.workspace.ui.review-copy-row
 
```

### Relevant and governing ontology

| Block | Why relevant | Implementation must |
|---|---|---|
| [`g5.governance.approval.requirements`](../../ontology/features/governance.g5.md#organization-administrators-administer-approval-requirements) | Precedent for org-admin authority on Base without ownership; owns merge admission | Use the same locked `org:admin` check (`require_org_admin_on_conn`); keep IE admission unchanged for admin merges |
| [`g5.governance.publish.actor`](../../ontology/features/governance.g5.md#publish-under-the-acting-humans-provider-authority) | Merge resolves provider credentials for the actor | Never substitute the Base owner's credentials; an admin with no usable credential gets the existing `action_required`/conflict result |
| [`g5.governance.merge.target`](../../ontology/features/governance.g5.md#respect-live-target-ownership-during-merge) | Workspace-target merges need the owner's barrier | Base has no VM or barrier; the admin path applies only when the target is Base |
| [`g5.workspace.identity.base`](../../ontology/features/workspaces.g5.md#keep-base-workspaces-as-protected-readers) | Base identity and read-only status | Identify Base by `is_base`; grant no content write |
| [`g5.security.tenant.write`](../../ontology/concepts/security.g5.md#separate-membership-from-write-permission) and [`g5.security.tenant.rls`](../../ontology/concepts/security.g5.md#enforce-tenant-scope-inside-database-access) | Membership is not write authority; scoped DB access | Admit only effective `org:admin` in Base's own org, inside the scoped transaction |
| [`g5.workspace.ui.reader-publication`](../../ontology/features/workspaces.g5.md#publication-controls-are-the-owners) | UI offers exactly what the server admits | Show the row actions to an org admin only on Base; everything else stays owner-only |

## Outcome and design

On Base's Show All PRs tab, an organization administrator (org role `owner` or `admin`) sees and can use Refresh, Close and Merge on each open pull request, as Base's owner can. Nothing changes for other workspaces or for other controls.

```
PullRequestList (Base tab)                     merge/refresh/close handlers (workspaces.rs)
canAct = owner || (kind=base && orgAdmin) ──►  admit_target_pr_action(workspace, pr)
                                                 ├─ owner      → workspaces:write scope (as today)
                                                 └─ non-owner  → org:admin scope
                                                                 + require_org_admin_on_conn
                                                                 + workspace.is_base
                                                                 + pr.target_workspace_id = workspace
                                               → unchanged services, actor = request user
```

The backend trace found that every owner assumption sits in the route handlers. `scoped_db_for_workspace(…"workspaces:write")` checks the owner, and `ensure_workspace_owner` repeats it. The merge, refresh and close services, `JobMergeService` and row-level security key only on the org, and provider credentials already resolve for the acting user. So one route helper decides admission once, and each handler uses it for every scope it opens; merge opens seven. The non-owner branch opens an `org:admin` scope, as the approval-requirements routes do. It requires a locked `org:admin` check in Base's org, and requires the pull request's target, not its source, to be this Base. That target check matters because refresh and close match the path workspace as source or target. The route-layer token scope stays `workspaces:write`, which admin role defaults include. The frontend adds an `orgAdmin` flag to the page's live gate, from the existing `routeOrgMembership` role check that Transfer uses. `PullRequestList` admits it only when the gate kind is `base`.

No new state, endpoint or credential path is added.

### Acceptance

- API: an org admin who doesn't own Base can refresh, close and merge (local and provider-first) a pull request into Base. Event and merge records attribute the admin.
- API refusals:
  - an org member who isn't an admin;
  - an org admin acting on a pull request into a non-Base workspace they don't own;
  - an org admin naming Base as the source path;
  - an admin of another organization.
  Each refusal changes no state.
- IE approval requirements still block an admin's merge.
- UI: on Base, an org admin sees the three actions and a plain member doesn't. On a non-Base workspace, a non-owner admin still doesn't see them.

### Risks

- An admin without a personal GitHub connection, in an org with no GitHub App binding, can't complete a provider merge. They get the existing "connect GitHub" result, which is the intended behavior under `g5.governance.publish.actor`.
- The static guard in `backend/g5-api/tests/route_auth_static.rs` requires `workspaces:write` in these handlers' scope window. It must accept the helper rather than being loosened for other routes.

## Review summary

- **Challenged:** `g5.governance.merge.target` (the owner barrier) doesn't apply because Base has no VM. The trace found no barrier or owner check on the Base merge path, only Base provider-history acceptance (`publication.rs:142`), which is a different operation.
- **Verify during coding:**
  - an `org:admin` scope gives the services everything they need, since no downstream SQL reads `workspaces:write` from the session context;
  - Base's pending-transfer gate (`WorkspaceStopping`) isn't needed on the admin path, because the admin's authority doesn't depend on who owns Base;
  - the frontend `routeOrgMembership` is the workspace's org, not just the active org.
- **Resolved:** the user then asked for Pull and auto-pull too (see the follow-up below); other Base owner controls stay the owner's.

## Follow-up: Pull and auto-pull

Intent: [request](../../chat_intent/base-pr-admin-actions/request.md#pull-and-auto-pull).

Ontology changes:

- New [`g5.workspace.remote.base-admin-pull`](../../ontology/features/workspaces.g5.md#let-organization-administrators-pull-base) in `features/workspaces.g5.md`.
- [`g5.workspace.remote.base-parity`](../../ontology/features/workspaces.g5.md#track-base-like-any-other-workspace) names administrators as sharing Pull and auto-pull.
- [`g5.workspace.ui.reader-publication`](../../ontology/features/workspaces.g5.md#publication-controls-are-the-owners) lists both Base exceptions.

| Block | Why relevant | Implementation must |
|---|---|---|
| [`g5.workspace.remote.base-parity`](../../ontology/features/workspaces.g5.md#track-base-like-any-other-workspace) | Base cycles act under Base's current owner, with no Base-only acting user | Keep the cycle's acting user and credentials the owner's; the administrator only triggers or configures |
| [`g5.governance.pr.base-admin`](../../ontology/features/governance.g5.md#organization-administrators-act-on-pull-requests-into-base) | Same admission rule | Reuse its admission; require only that the path workspace is live Base |

Design: the PR helper becomes `scoped_db_for_owner_or_base_admin(…, BaseAdminAction)`. `RemoteTracking` requires live Base; `PullRequest(id)` additionally requires the PR's target to be Base. `/sync` and `/repos/:repoId/auto_pull` open their scope through it, and their remaining checks (Git-operation mutation policy, repository membership) are unchanged. The trace found no actor in the manual sync path: it queues the same integration cycle auto-pull runs, and the worker stamps `acting_user_id` from the current owner. `set_auto_pull` uses the actor only in its audit log. In the UI, `ownerOrBaseAdmin(gate)` gates the PR row actions, the Pull section (Base has no Push) and the auto-pull toggle.

Risk: when the organization has no GitHub App binding, an administrator's Pull spends Base's owner's personal SCM connection. That is what background auto-pull already does without anyone clicking, so a manual Pull only changes when it runs.

Acceptance: an administrator who doesn't own Base toggles auto-pull and Pulls, and the import lands, attributed to the owner. A member, an administrator without `org:admin` in the token, and an administrator on a working workspace are refused, with nothing changed.

## Follow-up: Make Remote and Accept provider history

Intent: [request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-and-accept-provider-history), then [the Make Remote button and adopting after binding](../../chat_intent/base-pr-admin-actions/request.md#make-remote-button-and-adopting-after-binding).

### Ontology changes and governance

Comparison base: `issue-379-g5d-pr-cannot-read-inline-review-comments` at `0fc207f1`.

Changes in `features/workspaces.g5.md`:

- [`g5.workspace.remote.base-admin-pull`](../../ontology/features/workspaces.g5.md#let-organization-administrators-track-base) is retitled "Let organization administrators track Base". It now admits Make Remote and Accept provider history under the administrator's own provider authority.
- [`g5.workspace.remote.base-parity`](../../ontology/features/workspaces.g5.md#track-base-like-any-other-workspace): Make Remote on Base never creates the provider branch.
- [`g5.workspace.remote.base-provider-rewrite`](../../ontology/features/workspaces.g5.md#accept-provider-history-into-base): the administrator is an actor, and the actor's authority is rechecked at landing.
- New [`g5.workspace.remote.adopt-provider-history`](../../ontology/features/workspaces.g5.md#adopt-the-provider-history-instead-of-resolving): the alternative to Resolve with agent on any workspace, run in the owner's VM.
- [`g5.workspace.remote-ui.actions`](../../ontology/features/workspaces.g5.md#render-each-tracking-action-once): repository actions are labelled card buttons, never chip glyphs, starting with Make Remote.
- [`g5.workspace.ui.gates`](../../ontology/features/workspaces.g5.md#offer-an-action-by-what-its-operation-needs) and [`g5.workspace.ui.reader-publication`](../../ontology/features/workspaces.g5.md#publication-controls-are-the-owners) list the new admissions.

Change in `features/governance.g5.md`:

- [`g5.governance.remote.divergence`](../../ontology/features/governance.g5.md#resolve-divergence-through-an-explicit-owner-action) links to adopting.

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index cf292bb1..1ed7b0ab 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -571,7 +571,7 @@ For every workspace, Base included, explicit pull/push or enabled auto-pull may
 
 [D · g5.governance.remote.divergence]
 
-Automatic divergence pauses synchronization without launching resolution. An explicit owner action creates or reuses a resolution task in the already-running owner VM, with exact Git inputs pinned and delivered before launch; it preserves existing edits and publishes through G5. Provider history rewrites require separate approval. Viewing status or fetching never starts a VM. Base, which has no VM, instead reconciles by [accepting the provider history](workspaces.g5.md#accept-provider-history-into-base).
+Automatic divergence pauses synchronization without launching resolution. An explicit owner action creates or reuses a resolution task in the already-running owner VM, with exact Git inputs pinned and delivered before launch; it preserves existing edits and publishes through G5. Provider history rewrites require separate approval. Viewing status or fetching never starts a VM. The owner may instead [adopt the provider history](workspaces.g5.md#adopt-the-provider-history-instead-of-resolving), discarding the local commits. Base, which has no VM, reconciles only by [accepting the provider history](workspaces.g5.md#accept-provider-history-into-base).
 
 ## Metadata
 
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index b4757832..6f5895b4 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -435,6 +435,8 @@ The card head holds only the pill and expander. Each remote-tracking mutation ap
 
 A head-level Push, Pull or Retry would duplicate the same workspace action shown in the expanded detail.
 
+Repository actions are labelled buttons on the card, never glyphs on the repository chip. A local-only card offers **Make Remote** as a button beside its local-only pill, styled like the other branch actions.
+
 ### Complete a proven divergence resolution
 
 [D · g5.workspace.remote.divergence-auto-complete]
@@ -450,6 +452,21 @@ If the proof passes but the completion cannot be applied:
 
 This covers divergence runs only. A provider-rewrite approval checkpoint stays an explicit approval, because resolving divergence never approves a provider rewrite ([provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication)).
 
+### Adopt the provider history instead of resolving
+
+[D · g5.workspace.remote.adopt-provider-history]
+
+When a repository is paused as diverged with an open divergence run, its owner can **Adopt provider history** as the alternative to Resolve with agent. The owner reviews the provider tip and the local commits that would be discarded, then confirms. The owner's running VM resets that member branch to the reviewed provider tip and pushes it to G5 under an exact lease on the reviewed head. Nothing is pushed to the provider. The run completes through the [ordinary completion proof](#complete-a-proven-divergence-resolution), the pause clears, and the replaced head stays pinned with the run's inputs.
+
+Adopting needs [live attachment](#offer-an-action-by-what-its-operation-needs), as Resolve with agent does. It is refused, with nothing changed, when:
+- the member checkout has uncommitted changes or commits G5 lacks;
+- the provider tip or G5 head moved since the review;
+- the run is a provider-rewrite checkpoint, which keeps its own approval.
+
+When Make Remote binds a repository paused, it offers both choices at once. It recommends adopting when the discarded history is only the repository's initial commit. On Base, the same review and confirmation lands without a VM under [accept provider history](#accept-provider-history-into-base).
+
+Reason: a workspace whose new repository is bound to an existing provider repository usually holds only an initialization commit, which no agent can usefully rebase; the intended outcome is the provider's history. The reset runs in the VM because a server-side rewind behind a running checkout would be undone by the guest's next rebase or forced push.
+
 ### Publish a resolved divergence by push mode
 
 [D · g5.workspace.remote.divergence-auto-publish]
@@ -467,13 +484,13 @@ Base's remote-tracked repositories use the same pull-side remote tracking as eve
 - fetch, classification and fast-forward import, together with the provider observation and synchronization watermark;
 - the remote-tip advisory and the tracking states derived from it;
 - pause handling and sync history;
-- the Pull action and auto-pull control, which an organization administrator [shares with Base's owner](#let-organization-administrators-pull-base).
+- the Pull action and auto-pull control, which an organization administrator [shares with Base's owner](#let-organization-administrators-track-base).
 
 These paths contain no Base-only importer, scheduler, policy setting, acting user or exemption. Base imports, including [co-landed imports](governance.g5.md#retained-remote-integration), follow the same rules as any workspace's.
 
-**Base never publishes to its provider, on any path.** No trigger or push mode makes a Base cycle push: Base's push mode is always manual, Push to Remote and the auto-push control are refused or absent, and every push executor refuses Base as a backstop. A Base that holds history beyond its provider, whether ahead or diverged, pauses without a resolution run.
+**Base never publishes to its provider, on any path.** No trigger or push mode makes a Base cycle push, and Make Remote on Base binds only an existing provider branch, never creating one: Base's push mode is always manual, Push to Remote and the auto-push control are refused or absent, and every push executor refuses Base as a backstop. A Base that holds history beyond its provider, whether ahead or diverged, pauses without a resolution run.
 
-Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner (Pull and auto-pull also to [organization administrators](#let-organization-administrators-pull-base)) under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM is therefore unavailable on Base: **Resolve with agent**, and approving a rebased candidate over a provider rewrite. [Fork](#fork-to-make-my-own-changes) remains the route to edit.
+Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner (Pull, auto-pull, Make Remote and Accept provider history also to [organization administrators](#let-organization-administrators-track-base)) under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM is therefore unavailable on Base: **Resolve with agent**, and approving a rebased candidate over a provider rewrite. [Fork](#fork-to-make-my-own-changes) remains the route to edit.
 
 Once remote-tracked, Base gains no local commits: merges into a remote-tracked target are refused and pushes to its protected branch are denied. Base's history can still differ from the provider's in three ways: the provider rewrites the branch, the provider resets to an older commit that Base already holds, or Make Remote binds a Base that is ahead of or diverged from the provider. Make Remote binds such a Base paused, as it does any workspace. In either case Base reconciles only by [accepting the provider history](#accept-provider-history-into-base).
 
@@ -487,28 +504,28 @@ When a Base repository becomes remote-tracked, its auto-pull starts enabled. The
 
 Applies within [Base parity](#track-base-like-any-other-workspace).
 
-### Let organization administrators pull Base
+### Let organization administrators track Base
 
 [D · g5.workspace.remote.base-admin-pull]
 
-An administrator of Base's organization may Pull Base and turn each of its repositories' auto-pull on or off, as Base's owner may, independently of ownership. The administrator only triggers or configures the import: every cycle still acts under Base's current owner under [Base parity](#track-base-like-any-other-workspace), so manual Pull changes only when the import that auto-pull would run happens. This confers no other tracking authority (Accept provider history, Retry sync, Make Remote, binding changes), no authority on other workspaces and none in another organization; admission follows the same organization-level rule as [pull requests into Base](governance.g5.md#organization-administrators-act-on-pull-requests-into-base).
+An administrator of Base's organization may, as Base's owner may and independently of ownership, Pull Base, turn each of its repositories' auto-pull on or off, Make Remote on a Base repository and [Accept provider history](#accept-provider-history-into-base). Pull and auto-pull only trigger or configure the import: every cycle still acts under Base's current owner under [Base parity](#track-base-like-any-other-workspace), so manual Pull changes only when the import that auto-pull would run happens. Make Remote and Accept provider history are the administrator's own decisions: they run under the administrator's [provider authority](governance.g5.md#publish-under-the-acting-humans-provider-authority) and are attributed to them, and the bound repository's later cycles act under Base's owner. This confers no other tracking authority (Retry sync, changes to an existing binding), no authority on other workspaces and none in another organization; admission follows the same organization-level rule as [pull requests into Base](governance.g5.md#organization-administrators-act-on-pull-requests-into-base).
 
-Reason: an organization administrator untangling Base's pull requests needs Base current with its provider without waiting for, or impersonating, Base's owner.
+Reason: an organization administrator untangling Base's pull requests needs Base current with its provider, connected to it and reconciled with its history, without waiting for, or impersonating, Base's owner. Using the owner's provider authority for a binding the administrator chose would spend the owner's credentials on the administrator's decision.
 
 ### Accept provider history into Base
 
 [D · g5.workspace.remote.base-provider-rewrite]
 
-When a remote-tracked Base repository's history differs from its provider branch, Base's owner can explicitly **Accept provider history**. This applies when the provider rewrote the branch, and when Make Remote bound a Base that was ahead of or diverged from the provider. Accepting adopts the provider repository and discards Base's local history for that repository:
+When a remote-tracked Base repository's history differs from its provider branch, Base's owner, or an [administrator of its organization](#let-organization-administrators-track-base), can explicitly **Accept provider history**. This applies when the provider rewrote the branch, and when Make Remote bound a Base that was ahead of or diverged from the provider. Accepting adopts the provider repository and discards Base's local history for that repository:
 
-- Base's branch is reset to the provider tip the owner reviewed. The branch, provider observation and synchronization watermark move together in one landing, and the pause clears.
+- Base's branch is reset to the provider tip the actor reviewed. The branch, provider observation and synchronization watermark move together in one landing, and the pause clears.
 - Nothing is pushed to the provider.
 - Any open resolution or rewrite-review run for that repository closes as resolved by acceptance.
 - The replaced Base head stays pinned in Git, so the discarded commits remain recoverable by an operator. Forks keep their own history.
 
-The action is bound to the provider tip and Base head the owner reviewed. If either moved, the action is refused and the owner reviews again. The confirmation states that Base's local commits for that repository will be discarded.
+The action is bound to the provider tip and Base head the actor reviewed. If either moved, or the actor is no longer Base's owner or an administrator of its organization, the action is refused and the actor reviews again. The confirmation states that Base's local commits for that repository will be discarded.
 
-This is the only reconciliation Base offers, because Base has no VM. It replaces, on Base, the rebased-candidate approval and **Resolve with agent** that other workspaces use. Base's owner exercises it as a durable control-plane action under [Base parity](#track-base-like-any-other-workspace).
+This is the only reconciliation Base offers, because Base has no VM. It replaces, on Base, the rebased-candidate approval and **Resolve with agent** that other workspaces use. It is a durable control-plane action under [Base parity](#track-base-like-any-other-workspace). Make Remote on Base leads straight into it when the binding pauses.
 
 ### Offer an action by what its operation needs
 
@@ -517,7 +534,7 @@ This is the only reconciliation Base offers, because Base has no VM. It replaces
 The workspace page gates each control by the authority its server operation checks, through three gates and nothing else:
 
 - **Durable authority**: the viewer is the current owner and the workspace is not Base, read-only or freezing. It admits platform-record and Git actions: comments, review decisions other than applying a text suggestion, session management, queued input, task and job controls, pull-request creation, lifecycle, sharing and policy. Synchronizing an existing remote binding, acting on pull requests that target the workspace, and lifecycle itself check ownership alone, as [publication controls](#publication-controls-are-the-owners) states.
-- **Live attachment**: the owner's VM is running. It admits only what the [owner handle](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) requires: saving a file, doc or ontology node, Changes-panel commit, push and update, terminal, preview, answering an agent's permission or question, applying a text suggestion, discarding from a review copy and resolving a divergence with an agent. Each of these also needs durable authority except preview and the terminal, which need no writable policy, so a read-only or freezing owner keeps them while the VM runs and, while it does not, sees the VM state and Start rather than a policy change. A dialog or confirmation kept open through a state change applies the gate again at submission and keeps its draft.
+- **Live attachment**: the owner's VM is running. It admits only what the [owner handle](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) requires: saving a file, doc or ontology node, Changes-panel commit, push and update, terminal, preview, answering an agent's permission or question, applying a text suggestion, discarding from a review copy and resolving a divergence with an agent or by [adopting the provider history](#adopt-the-provider-history-instead-of-resolving). Each of these also needs durable authority except preview and the terminal, which need no writable policy, so a read-only or freezing owner keeps them while the VM runs and, while it does not, sees the VM state and Start rather than a policy change. A dialog or confirmation kept open through a state change applies the gate again at submission and keeps its draft.
 - **VM state**: the reported lifecycle status, which decides messages and the one valid lifecycle button.
 
 A single read-only flag that also stands for "VM not running" hid durable actions while stopped and said "read-only" when the cause was the VM; the three gates keep the page truthful about the reason. Readers get read affordances, [replies](documents.g5.md#let-readers-reply-in-threads) and [file suggestions](governance.g5.md#suggest-file-changes-from-files) only.
@@ -538,7 +555,7 @@ The banner and the VM strip name the reported state in its own words and offer o
 
 [I · g5.workspace.ui.reader-publication]
 
-Create pull request, Rebase & Merge, Prepare to ship, Fetch and Push to remote, auto-pull and auto-push, Retry sync, Approve rebased publication, pull-request refresh, close and merge, Publish branch and Make remote are offered only to the current owner. The server refuses them to anyone else, so a reader does not see them. The exceptions are on Base, where an organization administrator also gets pull-request refresh, close and merge ([pull requests into Base](governance.g5.md#organization-administrators-act-on-pull-requests-into-base)) and Fetch with auto-pull ([Base pulls](#let-organization-administrators-pull-base)), because the server admits them. Synchronizing an existing remote binding (Fetch, Push, auto-pull, auto-push, Retry sync, Approve rebased publication) and acting on pull requests that target a workspace check ownership alone (plus the Base administrator exceptions), so an owner keeps them under a read-only or freezing policy and Base's owner keeps its fetch controls; source-side publication (Create pull request, Rebase & Merge, Prepare to ship, Publish branch, Make remote) also needs a writable source.
+Create pull request, Rebase & Merge, Prepare to ship, Fetch and Push to remote, auto-pull and auto-push, Retry sync, Approve rebased publication, pull-request refresh, close and merge, Publish branch and Make remote are offered only to the current owner. The server refuses them to anyone else, so a reader does not see them. The exceptions are on Base, where an organization administrator also gets pull-request refresh, close and merge ([pull requests into Base](governance.g5.md#organization-administrators-act-on-pull-requests-into-base)) and Fetch with auto-pull, Make Remote and Accept provider history ([Base tracking](#let-organization-administrators-track-base)), because the server admits them. Make remote on Base needs Base's owner or such an administrator rather than a writable source. Synchronizing an existing remote binding (Fetch, Push, auto-pull, auto-push, Retry sync, Approve rebased publication) and acting on pull requests that target a workspace check ownership alone (plus the Base administrator exceptions), so an owner keeps them under a read-only or freezing policy and Base's owner keeps its fetch controls; source-side publication (Create pull request, Rebase & Merge, Prepare to ship, Publish branch, Make remote) also needs a writable source.
 
 ### Show the current review copy in the switcher
 
@@ -913,7 +930,7 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 ### g5.workspace.remote-ui.actions
 
 - human-verified
-- [Issue 240](../../chat_intent/issue-240/request.md) adopts the source branch's final removal of head-level repository actions; stay-open and focus return were added in [review](../../ontology-review-issue-240.md) finding 4.
+- [Issue 240](../../chat_intent/issue-240/request.md) adopts the source branch's final removal of head-level repository actions; stay-open and focus return were added in [review](../../ontology-review-issue-240.md) finding 4. The labelled Make Remote button is human-provided ([request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-button-and-adopting-after-binding)); Gap · `WorkspaceDetailsPanel.tsx` still mounts `MakeRemoteMenu`'s cloud glyph on the chip, and Publish stays a chip control in `BranchActionsMenu.tsx`.
 - Implemented · the card head holds only the pill and expander; mutations render once in BranchTrackingPopover.tsx, whose close-on-action callback was removed; BranchTrackingInline.tsx restores focus to the expander. Unit-tested. The client does not gate these controls for nonowners (unchanged); server enforcement was not reviewed.
 
 ### g5.workspace.remote.divergence-auto-complete
@@ -922,6 +939,11 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 - [Automatic divergence resolution](../../chat_intent/issue-240/auto-resolve-divergence.md); the proof reuses the existing completion check described in the cleanup plan's reconciliation section.
 - Implemented · `remote_integrations/worker.rs::auto_complete_proven_divergence` reuses `merge.rs` prepare/apply with `CompletionRequester::Automatic`. The `resolved` status comes from migrations 049–050. Retries count on `remote_integration_queue.auto_completion_attempts` (migration 051). Failures surface in `job_merge_runs.blocking_reason`, shown by `BranchTrackingPopover.tsx`. Worker tests cover the race replay and both the backoff and terminal fault paths.
 
+### g5.workspace.remote.adopt-provider-history
+
+- human-provided: "When Make remote completes it usually then offers the "use an agent to reconcile conflicts" - it should offer the alternative of adopt revision history (especially if the local repo is just the default init commit)" ([request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-button-and-adopting-after-binding)). Running the reset in the owner VM, the live-attachment gate, the refusals and reusing the completion proof are agent decisions from the code trace in the [plan](../../pr_ontology_diff_design/base-pr-admin-actions/ontology-plan.md#follow-up-make-remote-and-accept-provider-history).
+- Gap · not implemented: no guest `workspace.changes` operation resets a member branch to a given commit (the actions are `get`, `push`, `refresh`, `update`, `switch_back`, `commit` and `rebase`), and `capture_provider_history_acceptance` refuses non-Base with `PROVIDER_HISTORY_ACCEPTANCE_REQUIRES_BASE`.
+
 ### g5.workspace.remote.divergence-auto-publish
 
 - agent
@@ -933,7 +955,7 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 - human-verified
 - [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user said "Ensure that Base is treated just like any other workspace" and then limited that to remote tracking. The unavailable rebased-candidate approval is an agent amendment from the implementation review. Binding a non-fast-forward Base paused for acceptance follows the user's [provider-history acceptance](../../chat_intent/retire-protected-ref-mirror/request.md#base-provider-history-acceptance) decision. The user then limited parity to the pull side with nothing pushed on any path ([push invariant](../../chat_intent/retire-protected-ref-mirror/request.md#base-push-invariant-and-co-landed-imports)). Forcing Base's push mode to manual, rather than ignoring it, is an agent decision so stored policy and the UI never claim auto-push. The earlier citation of [H014](../../uplift/g5-lean/candidates/human-decisions.md) for per-repository imports is withdrawn: H014 decides individual repository pushes and no atomic multi-repository cut, which co-landed imports respect.
 - Implemented · the coordinated Base pull, `base_auto_pull_scheduler`, the use of `workspaces.base_auto_pull_*` and the Base exemptions are removed. Migration 056 clears those columns, and a later contract migration drops them once no running image reads them. The exemptions were in the background scheduler (`remote_integrations/policy.rs::enqueue_background_inbound_rows`), `passive_candidates`, the worker's background-inbound gate, the advisory pinger and `set_auto_pull`. `/sync` queues per repository (`workspace_service.rs::sync_workspace`), and cycles act as the current owner via `POLICY_QUERY`. Base-only refusals that remain:
-  - Make Remote: the default branch only. A non-fast-forward Base binds paused for acceptance.
+  - Make Remote: the default branch only. A non-fast-forward Base binds paused for acceptance. Gap · an absent provider branch is still created by `create_only_publish_head`, which does not refuse Base.
   - Publish.
   - Resolve with agent (`merge.rs`).
   - Approved rebase publish.
@@ -972,13 +994,16 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 
 - human-provided, human-verified
 - "also enable Base's Pull and auto-pull to be done by org admins" ([request](../../chat_intent/base-pr-admin-actions/request.md#pull-and-auto-pull)). Keeping the import's acting user as Base's owner is human-verified inference from Base parity; the reason is the user's stated need to untangle a G5 PR. Design: [plan](../../pr_ontology_diff_design/base-pr-admin-actions/ontology-plan.md#follow-up-pull-and-auto-pull).
-- Implemented · `workspaces.rs::{sync_workspace, set_branch_repo_auto_pull}` open their scope through `scoped_db_for_owner_or_base_admin` with `BaseAdminAction::RemoteTracking`; the service paths are unchanged (`sync_workspace` records no actor, `set_auto_pull` audits the request actor). The UI offers the Pull section (Base shows no Push) and the Auto-pull toggle through `workspaceLiveGate.tsx::ownerOrBaseAdmin` in `RemoteTrackingActions.tsx` and `BranchAutoPullControl.tsx`. Tests: `remote_repo_tracking_integration.rs::org_admin_pulls_base_and_sets_its_auto_pull_as_base_owner` (admin toggle and Pull, owner as `remote_sync_runs.acting_user_id`, member, token-scope and non-Base refusals), `route_auth_static.rs`, `RemoteTrackingActions.test.tsx`, `BranchAutoPullControl.test.tsx`. No live browser check.
+- human-provided: Make Remote and adopting the provider history ("As an admin, I need to be able to "Make Remote" on the Base workspace and to adopt revision history", [request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-and-accept-provider-history)). Acting under the administrator's own provider authority is agent inference from `g5.governance.publish.actor`. Design: [plan](../../pr_ontology_diff_design/base-pr-admin-actions/ontology-plan.md#follow-up-make-remote-and-accept-provider-history).
+- Gap · Make Remote and Accept provider history are not implemented for administrators: `promote_branch_repo_remote_tracking`, `preview_provider_history_acceptance` and `accept_provider_history` check ownership, the `BaseAcceptance` landing admission requires `owner_user_id = actor`, and the page offers Make remote only with a writable source.
+- Implemented (Pull, auto-pull) · `workspaces.rs::{sync_workspace, set_branch_repo_auto_pull}` open their scope through `scoped_db_for_owner_or_base_admin` with `BaseAdminAction::RemoteTracking`; the service paths are unchanged (`sync_workspace` records no actor, `set_auto_pull` audits the request actor). The UI offers the Pull section (Base shows no Push) and the Auto-pull toggle through `workspaceLiveGate.tsx::ownerOrBaseAdmin` in `RemoteTrackingActions.tsx` and `BranchAutoPullControl.tsx`. Tests: `remote_repo_tracking_integration.rs::org_admin_pulls_base_and_sets_its_auto_pull_as_base_owner` (admin toggle and Pull, owner as `remote_sync_runs.acting_user_id`, member, token-scope and non-Base refusals), `route_auth_static.rs`, `RemoteTrackingActions.test.tsx`, `BranchAutoPullControl.test.tsx`. No live browser check.
 
 ### g5.workspace.remote.base-provider-rewrite
 
 - human-provided, human-verified
 - [Base provider-history acceptance](../../chat_intent/retire-protected-ref-mirror/request.md#base-provider-history-acceptance): the user chose the owner approval that resets Base, and asked that the same action let Make Remote adopt the remote and discard Base's local history. The reviewed-tip binding, the recoverability pin and closing open runs are agent decisions. They follow from the implementation review finding that `GitRepo::normalize_reviewed_publication_tip` keeps G5's prior history, so reusing the rebased-candidate approval would republish removed provider history.
-- Implemented · the routes are `POST /workspaces/:id/repos/:repoId/provider_history_preview` and `POST .../accept_provider_history` (`routes/workspaces.rs`). They are owner-only Git operations served by `WorkspaceService::preview_provider_history_acceptance` and `::accept_provider_history`, which share a Base-only preflight that classifies against a fresh provider tip and counts the discarded commits.
+- human-provided: administrators may also accept ([request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-and-accept-provider-history)); see `g5.workspace.remote.base-admin-pull` for the gap.
+- Implemented (owner) · the routes are `POST /workspaces/:id/repos/:repoId/provider_history_preview` and `POST .../accept_provider_history` (`routes/workspaces.rs`). They are owner-only Git operations served by `WorkspaceService::preview_provider_history_acceptance` and `::accept_provider_history`, which share a Base-only preflight that classifies against a fresh provider tip and counts the discarded commits.
   - `remote_integrations::prepare_provider_history_acceptance` lands one cut through the existing convergence consumer, which moves the branch, provider observation and watermark, clears the pause and records an `imported` run. It adds a pin at `refs/g5/base-accepted/<advance-id>`.
   - The `BaseAcceptance` admission (`remote_integrations/publication.rs`) rechecks Base ownership under the fence and closes open remote-tracking `job_merge_runs` as completed with `accepted_provider_history`.
   - Make Remote no longer refuses a non-fast-forward Base. `BranchTrackingPopover.tsx` offers review then confirmation.
@@ -1011,7 +1036,7 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 
 - human-provided
 - Decision sheet policy "Publication controls for readers": hide them ([request](../../chat_intent/workspace-button-gates/request.md)).
-- Implemented · `ReposBar` and Prepare to ship take the owner gates in `WorkspacePage.tsx`; `BranchActionsMenu.tsx`, `MakeRemoteDialog.tsx`, `RemoteTrackingActions.tsx`, `BranchAutoPullControl.tsx` and `PullRequestList.tsx` read the gate and render nothing for a reader (Base keeps pull controls for its owner). `PullRequestList.test.tsx` covers the reader case; the Details chips are inspected only ([review](../../ontology-review-workspace-button-gates.md)). `PullRequestList.tsx`, `RemoteTrackingActions.tsx` and `BranchAutoPullControl.tsx` also offer the Base exceptions through `ownerOrBaseAdmin` ([request](../../chat_intent/base-pr-admin-actions/request.md)); their tests cover the administrator, plain member and non-Base cases.
+- Implemented · `ReposBar` and Prepare to ship take the owner gates in `WorkspacePage.tsx`; `BranchActionsMenu.tsx`, `MakeRemoteDialog.tsx`, `RemoteTrackingActions.tsx`, `BranchAutoPullControl.tsx` and `PullRequestList.tsx` read the gate and render nothing for a reader (Base keeps pull controls for its owner). Gap · `MakeRemoteDialog.tsx` requires `canWrite`, so Make remote is offered on Base to no one ([request](../../chat_intent/base-pr-admin-actions/request.md#make-remote-and-accept-provider-history)). `PullRequestList.test.tsx` covers the reader case; the Details chips are inspected only ([review](../../ontology-review-workspace-button-gates.md)). `PullRequestList.tsx`, `RemoteTrackingActions.tsx` and `BranchAutoPullControl.tsx` also offer the Base exceptions through `ownerOrBaseAdmin` ([request](../../chat_intent/base-pr-admin-actions/request.md)); their tests cover the administrator, plain member and non-Base cases.
 
 ### g5.workspace.ui.review-copy-row
 
```

| Block | Why relevant | Implementation must |
|---|---|---|
| [`g5.workspace.remote.base-admin-pull`](../../ontology/features/workspaces.g5.md#let-organization-administrators-track-base) | Grants the administrator authority | Admit through `scoped_db_for_owner_or_base_admin` (`RemoteTracking`) on Base only; pass the request user as actor |
| [`g5.workspace.remote.base-parity`](../../ontology/features/workspaces.g5.md#track-base-like-any-other-workspace) | Base never publishes; default branch only | Refuse Make Remote on Base when the provider branch is absent, before any push |
| [`g5.workspace.remote.base-provider-rewrite`](../../ontology/features/workspaces.g5.md#accept-provider-history-into-base) | Base's server-side executor | Keep reviewed-tip binding, pin and run closure; widen only the landing admission |
| [`g5.workspace.remote.adopt-provider-history`](../../ontology/features/workspaces.g5.md#adopt-the-provider-history-instead-of-resolving) | Non-Base executor | Reset in the guest under an exact lease; refuse dirty or unpushed checkouts and rewrite checkpoints; push nothing to the provider |
| [`g5.workspace.remote.divergence-auto-complete`](../../ontology/features/workspaces.g5.md#complete-a-proven-divergence-resolution) | Completes the adopted run | Reuse the proof unchanged (`head == mirror_tip` already passes) |
| [`g5.governance.remote.divergence`](../../ontology/features/governance.g5.md#resolve-divergence-through-an-explicit-owner-action) | Explicit owner action in the running VM, inputs pinned first | Reuse `start_remote_tracking_merge`'s fresh pull, fence, owner handle and `pin_inputs` |
| [`g5.governance.publish.actor`](../../ontology/features/governance.g5.md#publish-under-the-acting-humans-provider-authority) | Whose credentials fetch | Resolve for the acting human; never substitute the owner's |
| [`g5.workspace.remote-ui.actions`](../../ontology/features/workspaces.g5.md#render-each-tracking-action-once) | Where Make Remote renders | Labelled card button; no chip glyph |
| [`g5.workspace.ui.gates`](../../ontology/features/workspaces.g5.md#offer-an-action-by-what-its-operation-needs) | Adopting needs live attachment off Base | Disabled with the VM reason and Start when the VM isn't running |

### Outcome and design

**Make Remote is a card button.** A local-only repository card shows a labelled **Make Remote** button beside its "Local only" pill, styled like Pull and Push. It opens the existing dialog. The chip loses its cloud glyph. The button is offered to the owner of a writable workspace, and on Base to the owner or an organization administrator. Review copies never get it.

**After binding, choose how to reconcile.** When Make Remote returns `merge_required`, the dialog shows a review of the provider history: the provider tip, and the number of local commits it would discard. It offers two choices:
- **Adopt provider history** (confirm). This is marked *Recommended* when the discarded history is only the repository's initial (root) commit.
- **Resolve with agent** (the existing launcher; not on Base).

The repository card's paused detail offers the same pair later.

```
MakeRemote button ─► dialog ─► POST remote_tracking ──► bound / merge_required
                                     │ merge_required
                                     ▼
              ReconcileChoice (shared by dialog and paused card detail)
              ├─ preview: POST provider_history_preview  (tip, discarded, initialOnly)
              ├─ Adopt:   POST accept_provider_history {expectedProviderTip, expectedHead}
              │            ├─ Base:     server landing (unchanged, owner or org admin)
              │            └─ non-Base: run-start path → guest workspace.changes "adopt"
              │                         lease(expectedHead) push tip to G5
              │                         → worker auto-completes run, pause clears
              └─ Resolve with agent (non-Base only, unchanged)
```

**One endpoint, two executors.** `provider_history_preview` and `accept_provider_history` stop refusing non-Base workspaces. On non-Base they require an open divergence run that is not a `reset_below_baseline` checkpoint, and they admit the owner only. Base keeps its server landing.

On non-Base, accept reuses the guarded start path of `start_remote_tracking_merge` (`merge.rs`): a fresh provider pull, the commit fence, the owner handle and revalidation. It pins the inputs with `pin_inputs` so the replaced head stays recoverable. Instead of launching an agent, it calls a new guest `workspace.changes` operation, `adopt`, with the reviewed head and tip. The guest does five things:
1. Refreshes the provider refs.
2. Checks the association, a clean tree, and that the checkout head equals the reviewed G5 head.
3. Pushes the tip to G5 with `PushUpdate::Lease{expected}`, skipping `push`'s ancestry check.
4. Hard-resets the checkout.
5. Restores everything if the push is refused, as `rebase` does.

The existing worker proof then completes the run. A run completed with the new head equal to the provider tip classifies as `Equal`, so nothing is pushed to the provider. The receive path's lease is the only G5-side write, so there is no new landing code.

**Administrator authority on Base is unchanged from the earlier pass.**
- The three routes use the owner-or-Base-admin admission.
- The `BaseAcceptance` landing admission accepts the owner or an effective org admin.
- Make Remote on Base creates an absent provider branch only after the actor confirms it.

No new state or tables are added. The only new mechanism is the guest `adopt` operation. It is justified because the existing operations can only fast-forward (`push`) or replay local commits (`rebase`), and a server-side rewind behind a running VM would be undone.

### Acceptance

- **API, Base:**
  - A non-owner org admin uses Make Remote: a fast-forward Base binds with auto-pull on; an ahead Base binds paused, and preview then accept resets it, pins the old head and pushes nothing.
  - A stale review is refused.
  - Refusals change nothing: a plain member, a token without `org:admin`, an admin of another org, an admin demoted before landing, and an unconfirmed absent provider branch.
- **API and guest, non-Base, with a real VM or guest harness:**
  - A workspace holding only an init commit uses Make Remote, gets `merge_required`, and the preview reports `initialOnly`.
  - Accepting moves G5 and the guest checkout to the provider tip, completes the run, clears the pause and pushes nothing to the provider. The old head stays pinned.
  - Refusals change nothing: a dirty checkout, an unpushed guest commit, a stale tip or head, a stopped VM, a rewrite checkpoint, and a non-owner admin.
- **UI:**
  - A local-only card shows the "Make Remote" button and the chip has no glyph. Visibility for owner, admin on Base, member and review copy is as above.
  - `merge_required` shows the review with Adopt (recommended for an init-only history) and Resolve with agent. Base shows Adopt only.
  - The paused card detail offers the same pair.
  - Adopt off Base while stopped is disabled with the VM reason.

### Risks

- Adopting discards local commits. The confirmation states the count, and the run's pinned inputs (non-Base) or the acceptance pin (Base) keep them recoverable by an operator.
- Running agent sessions in the same VM could commit between the review and the reset. The exact lease and head check refuse that case rather than discard the new work.
- An admin without a usable provider credential, in an org with no GitHub App, gets the existing "connect GitHub" refusal.
- `route_auth_static.rs` guards the changed handlers' scope windows.

### Review summary

- **Challenged:**
  - *A server-side rewind for non-Base.* Rejected: the trace found no server path that moves a VM workspace backwards. The guest's `rebase` would replay discarded commits, and receive-pack accepts a forced push.
  - *A new completion path.* Unneeded: the existing proof admits `head == mirror_tip` without a task.
  - *Base parity's "acting under Base's current owner".* It governs pull cycles. Make Remote and accepting are the actor's explicit decisions.
- **Answered by the user** ([decisions](../../chat_intent/base-pr-admin-actions/request.md#publish-absent-provider-branch-and-credentials)):
  - Publish also becomes a labelled card button.
  - Make Remote on Base with an absent provider branch asks the actor to confirm, then creates the branch with one create-only push (`createProviderBranch`; refusal code `PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION`). This replaces the proposed refusal.
  - An administrator's actions use the administrator's own credentials and are attributed to them.
- **Resolved:** Make Remote and the acceptance preflight pass `Some(actor)` to `resolve_remote_tip_for_row`.
- **Verify during coding:**
  - the landing admission's org-admin read adds no lock-order edge;
  - the guest can fetch the run's pinned `inputs/{run}/remote` ref when the provider tip is not on a provider head;
  - open review rounds and G5 PRs tolerate a non-fast-forward head move of the source;
  - `branch.remoteEligible` and `isRemoteTracking` let a Base card reach the button.
