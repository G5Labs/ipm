# Workspace

<a id="g5.workspace.identity"></a>
[I · g5.workspace.identity]

A workspace has an organization, a name that is unique among the organization's live workspaces regardless of letter case, an owner who is a member, and creation / update times.  The name is how people and URLs address the workspace; the id is its durable identity, and the owner may [rename](#g5.workspace.rename) it.  

<a id="g5.workspace.kind"></a>
[D · g5.workspace.kind]

A workspace's registry kind is Base, working or personal. It is fixed at creation and decides which rules apply; a name, owner or ancestry never implies a kind. Every workspace other than Base and [personal workspaces](personal_workspace.g5.md) is a working workspace. Personal workspaces have no parent and follow the [personal exceptions](personal_workspace.g5.md#g5.personal.refusals) to fork, stages, rebase, merge, transfer, delete and Project Docs.

<a id="g5.workspace.branch"></a>
[I · g5.workspace.branch]

A workspace has a unique branch name which it owns across all the organization repos.  This branch name is derived from the workspace name when the workspace is created, including a uniqueifier, and never changes afterwards: [renaming](#g5.workspace.rename) the workspace leaves every repository branch as it is.  

<a id="g5.workspace.parent"></a>
[I · g5.workspace.parent]

Workspaces, other than the root "Base" workspace, all have a parent workspace from which they were forked.  

<a id="g5.workspace.fork"></a>
[I · g5.workspace.fork]

A member with read access to a workspace can create a new child workspace through forking, becoming the owner of the child.  Forking does not directly impact the state of the parent workspace.  

<a id="g5.workspace.fork-state"></a>
[I · g5.workspace.fork-state]

When a workspace is forked from a parent, it forks the branch state of the parent workspace branches. It \[optionally\] inherits any dirty file state from the parent at the instant it was forked.

<a id="g5.workspace.ancestor-read"></a>
[I · g5.workspace.ancestor-read]

A child workspace has read access to its parents up the chain to the Base workspace.  

<a id="g5.workspace.base"></a>
[I · g5.workspace.base]

The "Base" workspace is read-only, owned by an organization admin, and cannot attach a workspace VM.  Its branch state can only be modified by approved merge operations or external repository imports.  

<a id="g5.workspace.mutation-authority"></a>
[I · g5.workspace.mutation-authority]

The current workspace owner is the only user that can modify the workspace branch state or send api calls to a workspace VM that modify its state.  The only exception is the Base workspace, which can also be modified by organization admins.  Merging a pull request into the workspace instead follows [merge authority](#g5.workspace.merge), which also admits the pull request's [source owner](#g5.governance.pr.source-owner-merge) when the workspace has an attached requirement; a merge advances only the committed branch cut and refuses on target drift, never reaching the owner's live VM or unsaved work.  

<a id="g5.workspace.transfer"></a>
[I · g5.workspace.transfer]

The active workspace owner or an active organization administrator can explicitly transfer an eligible workspace to another active, owner-eligible member of the same organization, including from a deactivated owner. This process terminates any currently running workspace VM before the transfer is complete to avoid leaking owner-specific credentials. Deactivation never performs a transfer.

<a id="g5.workspace.interaction"></a>
[I · g5.workspace.interaction]

A workspace supports a variety of modification mechanisms such as chat, preview, and shell access through api calls to an attached workspace vm.  

<a id="g5.workspace.git-authority"></a>
[I · g5.workspace.git-authority]

The Repo branch metadata is the authority on workspace repo state.  This state is not duplicated elsewhere in platform metadata.  

<a id="g5.workspace.issue"></a>
[I · g5.workspace.issue]

A workspace may be associated with a primary issue, which it uniquely owns in the platform.  A workspace may have any number of related issues.  The state of the primary issue reflects the state of the workspace and the platform helps maintain this linkage  

<a id="g5.workspace.stages"></a>
[I · g5.workspace.stages]

A workspace has Business, System and Integration Engineering phases, each with associated operations. Approval status never gates moving between phases; configured target requirements gate merge under the [workspace-wide approval policy](reviews.g5.md#g5.governance.approval.requirements).

<a id="g5.workspace.rebase"></a>
[I · g5.workspace.rebase]

A workspace can be rebased on its parent to pick up changes.  This rebase operation acts like a regular fast forward rebase between the workspace branch and the workspace parent's branch for all repos, using an agent to resolve any conflicts that arise.  

<a id="g5.workspace.merge"></a>
[I · g5.workspace.merge]

A workspace can be merged into its parent.  This operation may be gated by configured required approvals and automated proof of quality (eg passing tests, verified behavior).  For repos with external integrations, this can involve external pull request creation.  Merge authority is the parent's owner (administrators for [Base](#g5.governance.pr.base-admin)) and, when the parent has an attached requirement, the [source workspace's owner](#g5.governance.pr.source-owner-merge).  

<a id="g5.workspace.review"></a>
[I · g5.workspace.review]

A workspace can be a "review" workspace which is an ephemeral workspace used for users to review its parent.  Review workspaces work similarly to regular workspaces, except file changes are observed by the parent and shown to users as "suggestions" when viewing the parent files.  Review workspaces can be merged into the parent, but it isn't required.  

<a id="g5.workspace.delete"></a>
[I · g5.workspace.delete]

A non-Base workspace can be deleted by its owner.  Deleting a workspace cleans up repo branches and some of the database footprint, but may retain information for longitudinal analysis.  Deleting a parent cascades to deleting its children (no orphans).  The existance of non-review child workspaces owned by other users requires additional confirmation prior to delete.  Actual deletion may be delayed to support an "un-delete" operation.  Deleting a workspace stops its workspace vm, if any.  

<a id="g5.workspace.delete.pull-requests"></a>
[D · g5.workspace.delete.pull-requests]

Deleting a workspace first closes every open pull request into or out of it, as an ordinary close does, each with a closed event that names the deletion, and commits that before it removes the workspace; a deletion that fails afterwards is retried by deleting again. A pull request that opens in between closes with the removal, and once a person's deletion has reserved the workspace no new pull request opens into or out of it. The close holds no other lock while it waits on a pull request. Nothing else about its pull requests changes. Provider pull requests are left as an ordinary close leaves them. The records stay [readable to tools](../software_architecture/git.g5.md#g5.git.pr-reads). Approval evidence keeps naming the reviewed commits; once collection removes a commit that only the deleted workspace's refs held, that evidence reads as unavailable, as [missing evidence](reviews.g5.md#g5.governance.approval.evidence) does. Pull requests left open by workspaces deleted before this rule are closed once, the same way. Reason: owner decision (2026-10-29); an open pull request whose workspace is gone can never be merged, refreshed or closed, and once closed it stays off every surface that acts on open pull requests.

<a id="g5.workspace.delete.admission"></a>
[D · g5.workspace.delete.admission]

Workspace deletion validates and retains current eligibility before stopping compute or discarding work, including force requests. A refusal for live children, active merges or unfinished forks leaves runtime and pending work intact. Once admitted, competing operations cannot invalidate eligibility or restart compute before removal; an ordinary retry joins the admitted operation, a force retry may supersede it, and final removal rechecks current authority and the exact current operation identity. Workspace service owns admission and removal; VM lifecycle supplies stop outcomes. This does not settle the existing cascade implementation gap.

<a id="g5.workspace.project-docs"></a>
[I · g5.workspace.project-docs]

Each workspace has a Project Docs folder where the workspace stores artifacts for aligning on intent.  

<a id="g5.workspace.project-docs-path"></a>
[I · g5.workspace.project-docs-path]

The Project Docs folder for a workspace is stored in the docs repo under a path determined by the workspace id  

<a id="g5.workspace.project-docs-agent"></a>
[I · g5.workspace.project-docs-agent]

Agents working on a workspace vm should be guided to put their documents into the workspace's project docs folder and discouraged from modifying documents in other workspace's project docs folders  

<a id="g5.workspace.durable-read"></a>
[I · g5.workspace.durable-read]

Non-owners querying workspace file state see the most recently committed git branch state  

<a id="g5.workspace.owner-read"></a>
[I · g5.workspace.owner-read]

Owners querying workspace file state use the workspace VM file access to see current dirty state if the VM is up, otherwise they also see the most recent git state.

## Access and collaboration

<a id="g5.workspace.configuration-authority"></a>
[D · g5.workspace.configuration-authority]

Only the current workspace owner may create, update or delete workspace-scoped configuration, including provider accounts and credentials. Workspace access, a write scope, past ownership and organization administration confer no bypass. Hold current ownership through a configuration mutation's commit, serialize with transfer, and refuse mutations while transfer is pending. Organization configuration is managed in [Admin](organizations.g5.md#g5.settings.organization-configuration); personal configuration remains owned by its user.

<a id="g5.workspace.audience"></a>
[D · g5.workspace.audience]

Workspace audience is fixed by registry identity, never configurable. Ordinary working workspaces and Base are readable by eligible active organization members under the current credential and scope rules, and so are their branches through [repository permissions](repositories.g5.md#g5.repo.permissions); ordinary review copies retain their separate operation restrictions. [Personal workspaces](personal_workspace.g5.md#g5.personal.access) admit only their owner and active organization Admins. Dedicated [formal agent review runtimes](agent_approvals.g5.md#g5.agent-approval.human-read-only) retain their restrictive workflow authority, taking precedence over ordinary owner or administrator admission; source-history readers may inspect retained evidence, with target-side content requiring target read access. There are no workspace reader invitations or visibility settings, and G5 offers no per-workspace read restriction until a private-workspace feature protects records and Git branches together. Read access permits existing reader actions, never ownership, approval decisions or agent mutation authority. Creator and former-owner status alone grant no special access. Direct reads, discovery, retained history and equivalent SQL authorization use this same rule; transactional reader writes preserve current authority through commit.

<a id="g5.workspace.content-policy"></a>
[D · g5.workspace.content-policy]

The owner may make a non-Base workspace's application content read-only and restore editing, independently of attached compute. Read-only blocks application content and workspace-owned discussion writes. Authorized teardown, administration, personal preferences, Git synchronization and pure projection of accepted Git content remain governed by their own rules. This is not filesystem immutability or permission to bypass Base and VM authority.

<a id="g5.workspace.content-policy.transition"></a>
[D · g5.workspace.content-policy.transition]

Changing to read-only preserves existing work. Close admission to new user content work while already-admitted agent and background work may finish. Complete the change only after rechecking quiescence with competing admission excluded. Each policy write holds current ownership and excludes pending transfer through commit. A drain's completion and timeout cleanup apply only to its exact transition identity; cancellation or a newer transition makes an old worker inert. Timeout or explicit cancellation restores editing without discarding work. A lost drain worker leaves a visible transition that the current owner can recover by restoring editing; automatic process-restart recovery is not promised.

<a id="g5.workspace.content-policy.transition-authority"></a>
[D · g5.workspace.content-policy.transition-authority]

Ownership-transfer admission invalidates a pending [policy drain](#g5.workspace.content-policy.transition), including when ownership later returns to the same actor. Its old worker cannot complete, clear a newer transition or emit a policy-change event for a rejected write. The current owner can recover the visible transition by restoring editing.

<a id="g5.workspace.rename"></a>
[D · g5.workspace.rename]

The current owner may rename a non-Base, non-[personal](personal_workspace.g5.md#g5.personal.refusals) workspace, including a read-only workspace or an ordinary reviewer-owned review copy, to a name that passes the same validity and reserved-segment rules as creation and that no workspace in the organization holds or [reserves](#g5.workspace.former-names). [Formal agent-review workspaces](agent_approvals.g5.md#g5.agent-approval.human-read-only) accept no human change, rename included. Base keeps its name as a product rule: it is the organization's fixed root and a navigation anchor. Only the name changes, in a transaction that touches no repository: the [branch](#g5.workspace.branch), VM, Project Docs, links, tabs and history follow the unchanged workspace id, and browsers showing the workspace learn the new name. Rename is owner housekeeping under the configuration-authority gate, refused while an ownership transfer is pending; it is not an application content write under the [content policy](#g5.workspace.content-policy). It is offered from the Overview of Workspace details, as a Name row above the description with its former names beneath, and nowhere else. Each rename is recorded durably, so the order of renames and a rename back remain visible. Names are chosen before the work is understood, while branch names are shared with Git clients and remotes, so the name moves and the branch stays.

<a id="g5.workspace.former-names"></a>
[D · g5.workspace.former-names]

A renamed workspace keeps every former name for as long as it exists: a URL or CLI reference by a former name resolves to the workspace and is replaced by its current name, and no other workspace in the organization may take a former name, as a live name or through a fork. The workspace may take one of its own former names back. A workspace being [deleted](#g5.workspace.delete) reserves its current and former names until its deletion completes, when its branches are gone and its repository membership is removed; then the names retire. Reservation, rather than release on reuse, keeps a link in an email, issue or pull request from ever pointing at a different workspace.

<a id="g5.workspace.review-decisions"></a>
[D · g5.workspace.review-decisions]

The source owner decides acceptance, rejection and resolution; the reviewer may withdraw their own open suggestions. Reader replies carry no decision. Each suggestion keeps its discussion and decision history across its displays, and a decided suggestion cannot be reopened by a competing or stale action. Independently selectable edits settle only the selected edits.

<a id="g5.workspace.review-acceptance"></a>
[D · g5.workspace.review-acceptance]

Accepting a file suggestion applies the selected change in the owner's ready workspace, refusing missing or ambiguous source context instead of guessing. Preserve selected replacement bytes, including whitespace at file boundaries. It then uses [selected-file Save](documents/files.g5.md#g5.documents.selected-save), including accepted new files. Applied changes remain accepted when publication fails and expose the remaining publication work. Interrupted acceptance must recover prior application before a later decision can describe those bytes as rejected or withdrawn; retries must not apply the change twice. Record-only artifact decisions do not require a file write.

<a id="g5.workspace.review-rounds"></a>
[D · g5.workspace.review-rounds]

Review copy pushes record committed work as successive rounds. A new round supersedes the same reviewer’s earlier open changes and findings from the same kind of review; standalone annotations and persistent comments/artifacts remain. Continue discussion without merging distinct proposals. Carry a decision and unchanged note only to the same proposal at its evidenced occurrence and content; changed proposals need a fresh decision. A round’s verdict remains [feedback rather than formal approval](#g5.workspace.approval-review-verdict-question).

<a id="g5.workspace.review-discard"></a>
[D · g5.workspace.review-discard]

A reviewer may discard selected previewed changes from their own review copy without changing the source. Refuse stale selections and preserve files changed since the preview instead of reversing at a guessed location. Report completed and skipped changes separately; one failure does not conceal changes already discarded. Use the shared [file mutation rules](../software_architecture/workspace_vm.g5.md#g5.runtime.files.mutation).

## Fork, transfer and phase

<a id="g5.workspace.fork-state.formal"></a>
[D · g5.workspace.fork-state.formal]

Expands [fork state](#g5.workspace.fork-state): freeze the complete set of repository versions for the operation before creating child branches. Fresh live capture requires source ownership; authorized readers may select branch state or an eligible retained durable snapshot. Retries use those frozen versions and captured dirty state. Publish the child only when every member is present; matching branch content alone grants no right to adopt or clean up a branch without this operation’s ownership proof.

<a id="g5.workspace.transfer.formal"></a>
[D · g5.workspace.transfer.formal]

Expands [ownership transfer](#g5.workspace.transfer): exclude new owner operations while transfer is pending, terminate the old compute authority before committing the new owner, and recheck and hold the initiator's authority and target's active, eligible organization membership through that commit, including resumed transfers. Replayed transfer requests preserve their identity; failed or pending stop cannot be reported as a completed transfer. Removing membership or demoting a member to viewer must refuse while they own a nondeleted working workspace. Complete explicit transfers or deletions first; membership changes never choose a replacement owner. Their [personal workspace](personal_workspace.g5.md#g5.personal.retirement) is retired in the same change instead and never blocks it. [Deactivation](organizations.g5.md#g5.organization.deactivation.retention) preserves role eligibility and is not blocked by retained ownership. Base, review and deleted workspaces remain ineligible for this ordinary transfer path.

<a id="g5.workspace.owner-limit"></a>
[D · g5.workspace.owner-limit]

Enforce the [owner limit](organizations.g5.md#g5.organization.owner-limits) on workspaces: by default an owner may hold 50 in the organization; administrators may set 0–1000. Count their live non-Base workspaces, including review and approval-review workspaces, plus their pending forks and pending transfers to them. Admitting a new fork or a new transfer to an owner at the limit is refused before anything is written; finalizing an admitted fork, its retries and resumed transfers are never refused, so no operation is stranded. Lowering the limit removes nothing.

<a id="g5.workspace.phase-state"></a>
[D · g5.workspace.phase-state]

The current owner explicitly selects the workspace phase; it is durable platform state, not a value derived from Git or approvals. Record one transition event with each actual phase change. Base remains read-only in Business Engineering; a fork of Base starts there, while an ordinary fork inherits the source phase captured at admission. A phase move never requires or settles an approval.

## Approval policy and evidence

<a id="g5.workspace.approval-administration"></a>
[D · g5.workspace.approval-administration]

Organization administrators attach and retire requirements for merges into a workspace. Workspace ownership or a workspace-scoped grant does not confer that organization-level authority.

<a id="g5.workspace.approval-policy"></a>
[D · g5.workspace.approval-policy]

Apply the [target requirements](reviews.g5.md#g5.governance.approval.requirements), [combined approver quorum](reviews.g5.md#g5.governance.approval.quorum) and [reviewed-version policy](#g5.workspace.approval-definitions) to one protected view of the configured target, definitions and decisions. Serialize competing attachments, publications and withdrawals before recording pinned merge admission; provider recovery retains the [accepted admission](../software_architecture/git.g5.md#g5.git.pr-recovery-cut).

<a id="g5.workspace.approval-human-authority"></a>
[D · g5.workspace.approval-human-authority]

A configured human with current workspace read access, other than an organization viewer, may decide their [addressed active request](reviews.g5.md#g5.governance.approval.request), including its requested/current self-approval policy. An organization viewer's decision is refused however it is submitted, and a member demoted to viewer while their request waits can no longer decide it, because viewers are read-only by role and a decision is authoritative. Ownership, requirement administration, agent review and native tool permission do not substitute for that person’s decision. Stopped compute alone does not prevent durable review or discussion.

<a id="g5.workspace.approval-definitions"></a>
[D · g5.workspace.approval-definitions]

Publishing appends an immutable definition version without replacing prior definitions or decisions. The latest version supplies configured reviewers, [quorum](reviews.g5.md#g5.governance.approval.quorum) and recommendation. A version marked as invalidating prior decisions sets their eligibility cutoff; a non-invalidating publication does not itself erase eligible earlier decisions. [Recommendation selection](reviews.g5.md#g5.governance.approval.requirements) does not bind requests or merge evaluation to a phase.

<a id="g5.workspace.approval-request-settlement"></a>
[D · g5.workspace.approval-request-settlement]

[Addressed human requests](reviews.g5.md#g5.governance.approval.request) settle independently; exact operation retries preserve request/decision identity and [note identity](reviews.g5.md#g5.governance.approval.request-note). Legacy phase fields remain immutable compatibility/history data. A later explicit owner request may start another review; meeting quorum never silently settles another recipient’s request.

<a id="g5.workspace.approval-cut-eligibility"></a>
[D · g5.workspace.approval-cut-eligibility]

Evaluate eligible definition versions and the latest eligible decisions independently of phase. Human decisions capture the [selected complete pushed vector](reviews.g5.md#g5.governance.approval.version); whether a later source vector keeps them counting belongs to the [source-commit policy](reviews.g5.md#g5.governance.approval.source-commit-policy). An agent decision counts only as an [effective formal verdict](agent_approvals.g5.md#g5.agent-approval.effectiveness) and records its frozen assignment’s source cut; malformed stored cut evidence reads as unknown currentness and fails closed under MAKE_PENDING. Preserve legacy human evidence without retroactively assigning it an open PR or personal review checkpoint.

A person's Undo withdraws every approval they gave for that requirement, whatever cut each names; none of them counts again until the person decides anew, even if the source returns to a cut they approved, and history keeps them.

<a id="g5.workspace.approval-review-verdict-question"></a>
[D · g5.workspace.approval-review-verdict-question]

A review-copy round’s verdict is feedback and grants no formal approval. Each formal decision requires the person’s [addressed request](reviews.g5.md#g5.governance.approval.request), current read access and eligible requested/current self policy. Publishing suggestions cannot settle or retarget a formal request; explicit requested decisions retain their captured vectors and independent history.

## Merge authority and override

<a id="g5.governance.pr.base-admin"></a>
[D · g5.governance.pr.base-admin]

An administrator of [Base](#g5.workspace.base)'s organization may refresh, close and merge open pull requests whose target is Base, as Base's owner may, independently of ownership. The rule covers only Base as the pull request's target: pull requests into other workspaces stay their owner's, and a pull request's source side keeps its own authority. A merge keeps the same [admission](#g5.workspace.approval-policy) and is attributed to the acting administrator. The rule confers no Base content-write, live compute, remote-binding or source-side publication authority and never reaches another organization's workspaces.

<a id="g5.governance.pr.source-owner-merge"></a>
[D · g5.governance.pr.source-owner-merge]

The current owner of a pull request's source workspace, as an active organization member, may **merge** it into its parent (every parent, including [Base](#g5.workspace.base)) whenever the parent has at least one attached [approval requirement](reviews.g5.md#g5.governance.approval.requirements). The merge keeps the same [admission](#g5.workspace.approval-policy) and every other merge gate and is attributed to the person; it grants no refresh, close or other write to the parent (the source side only gains merge). This ordinary merge authority also qualifies the person for the separate override rules: an organization administrator may waive [G5 requirements](#g5.workspace.merge.admin-override), and any authorized person may request [GitHub bypass](../software_architecture/git.g5.md#g5.git.provider-override) under their own GitHub permissions. Source ownership alone supplies neither organization administration nor GitHub bypass rights. A parent with no attached requirement delegates nothing: only its own merge authorities — its owner, or an administrator for Base — merge into it. Reason: attaching a requirement is the act that grants which child workspaces may merge in ([owner decision](../../chat_intent/issue-1104/request.md)). Open assumptions: only [administrators attach requirements](#g5.workspace.approval-administration), so an administrator may grant this on a workspace without its owner's agreement; and any attached requirement counts — agent-only or self-approval-allowing definitions included — so a member could land work with nobody else involved.

<a id="g5.workspace.merge.admin-override"></a>
[D · g5.workspace.merge.admin-override]

An organization administrator may **merge with override** any pull request they currently have [ordinary merge authority](#g5.workspace.merge) over, including a [source-owner merge](#g5.governance.pr.source-owner-merge) into another person's non-Base parent, while G5's own requirements block it: unmet [approval requirements](reviews.g5.md#g5.governance.approval.requirements) and the organization's [external-checks policy](../software_architecture/git.g5.md#g5.git.external-check-policy). It adds no merge authority ([target scope](#g5.workspace.merge.admin-override.scope-question)). It waives only G5-imposed policy. The provider's rules (branch protection, rulesets) stay GitHub's: no merge [bypasses them silently](../software_architecture/git.g5.md#g5.git.provider-rules), and bypassing them is the separate [GitHub override](../software_architecture/git.g5.md#g5.git.provider-override) under the person's own GitHub identity. One confirmation can carry both overrides, each under its own authority. Mechanical preconditions (conflicts, stale or moved pinned versions, current merge authority, pending transfer, [recovery and idempotency](../software_architecture/git.g5.md#g5.git.merge-recovery)) still apply. A later G5 merge gate is overridable, reporting its blockers to the override, unless its own block says it is not. One authority rule decides who may make each override; the merge, the confirmation's preview and the UI's offer all ask it, so changing who may override changes that rule alone.

The override is a human decision made only in the web UI, under the [browser-session rule](../concepts/security.g5.md#g5.security.browser-session-actions). Workspace agents, [merge when ready](../software_architecture/git.g5.md#g5.git.pr-auto-merge), Ship with agent, `g5d` and API keys never make it, whatever the caller's role, and nothing acting for an administrator inherits it. Reason: an administrator sometimes has to land work while an approver is away, an agent reviewer is broken or CI is red for an unrelated reason, and weakening the configuration for everyone or going around G5 leaves no honest record ([issue #873](https://github.com/G5Labs/G5-Lean/issues/873)).

<a id="g5.workspace.merge.admin-override.formal"></a>
[D · g5.workspace.merge.admin-override.formal]

Expands [g5.workspace.merge.admin-override](#g5.workspace.merge.admin-override). These rules govern one override confirmation, whether it carries this override, the [GitHub override](../software_architecture/git.g5.md#g5.git.provider-override) or both.

- **Shown set.** Before confirming, the person sees the current blockers of the pull request's current source version, grouped by the authority that may waive them. G5's: each unmet requirement with its state (waiting, not requested, rejected, no eligible reviewer) and, while the organization's policy is on, each failing, pending or unreadable check by repository. GitHub's: each repository GitHub reports as blocked, with the state and required checks GitHub reports. The action is offered only when the person holds the authority for every group present. Mechanical blockers are listed as not overridable and keep the merge unavailable. Confirming requires a non-empty reason and ticking a confirmation that the listed requirements and rules will be waived.
- **Re-checked at execution.** The merge runs the ordinary merge body. Every new override requires current ordinary merge authority; waiving G5 requirements additionally requires current organization administration. Preserve the authority each waiver relies on through each transactional mutation, including after preparation and before provider dispatch. A preview is not a grant: loss of membership or the ownership or attached requirement establishing merge authority refuses before new effects. A target ownership change alone does not revoke an otherwise authorized source owner. GitHub decides its bypass under the acting person's own identity. It reads checks and GitHub's merge state fresh and evaluates approval admission under the [ordinary serialization](#g5.workspace.approval-policy); the resulting blockers and source version must equal the shown set, otherwise it refuses with no state change and returns the current set to show instead. An empty set refuses too: the ordinary merge applies.
- **Record.** An admitted override is part of the [merge admission](../software_architecture/git.g5.md#g5.git.pr-recovery-cut) for that exact cut: actor, time, reason, whether the actor owns the source workspace (self-override), the waived G5 requirements and check states, and the GitHub-blocked repositories with GitHub's state. Recording it appends a pull-request event, so an override that GitHub then refuses stays visible. Recovery after a provider accepted that cut retains the override like any admission; whoever finishes that recovery completes the overridden merge without making a new override. Every other attempt evaluates admission without one.
- **No decision.** The override records no approval or rejection for anyone. When a merge that waived G5 requirements lands, the pull request's pending review requests close as [bypassed by override](reviews.g5.md#g5.governance.approval.request-status).
- **Flagging.** The pull request, both workspaces' Activity (the source among its pull request's events; the target among the merges that landed in it, named by their source), `g5d pr view` and its JSON, and the [merge announcement](../software_architecture/git.g5.md#g5.git.pr-merge-announcement) say "Merged with override by X: reason", naming whether G5's requirements, GitHub's rules or both were waived, and list what was waived.

<a id="g5.workspace.merge.admin-override.scope-question"></a>
[D · g5.workspace.merge.admin-override.scope-question]

Override scope follows current [ordinary merge authority](#g5.workspace.merge), including the [source-owner path](#g5.governance.pr.source-owner-merge) into another person's parent. Organization administration authorizes waiving G5 requirements within that scope; GitHub bypass remains the acting person's provider permission. Neither override grants a new merge destination or other writes to the parent. Like an ordinary merge, it advances only the committed cut and refuses on target drift rather than touching a live target. Reason: the owner explicitly chose this policy to resolve [issue #1116](../../chat_intent/issue-1116/override-policy.md), superseding the earlier Base-or-owned-target restriction.

<a id="g5.workspace.merge.admin-override.self-question"></a>
[D · g5.workspace.merge.admin-override.self-question]

An administrator may override requirements on their own pull request (one whose source workspace they own); the record and notifications flag it as a self-override. Reason: a small organization may have a single administrator.

<a id="g5.workspace.merge.admin-override.opt-out-question"></a>
[D · g5.workspace.merge.admin-override.opt-out-question]

Organizations cannot disable the override or require a second administrator in the first version; the durable record keeps either addition possible later.

<a id="g5.story.merge-with-override"></a>
[I · g5.story.merge-with-override]

A person lands a pull request that only G5's own requirements or GitHub's rules block, when they hold the authority to waive each: an organization administrator for G5's, and GitHub's own bypass rights, under their GitHub account, for GitHub's. In the web UI they see exactly which approvals, checks and GitHub rules they would waive, give a reason and confirm deliberately. The merge is flagged as an override wherever it is shown, and the people whose requirements or reviews it bypassed are told.

Governing contracts: [the G5 override](#g5.workspace.merge.admin-override), [the GitHub override](../software_architecture/git.g5.md#g5.git.provider-override), [the confirmation](#g5.workspace.merge.admin-override.formal), [browser-only actions](../concepts/security.g5.md#g5.security.browser-session-actions), [announcement](../software_architecture/git.g5.md#g5.git.pr-merge-announcement).

## Agent operation

<a id="g5.story.agent-fork-child"></a>
[I · g5.story.agent-fork-child]

A workspace agent can fork its workspace, another workspace within its [scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) or Base into a new workspace of its owner, with a chat prompt for the new workspace's agent and, as in the UI's fork, an existing issue linked to it as primary or related. The fork follows [fork state](#g5.workspace.fork-state.formal); launching the prompt is a labelled compound [Start](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start), and a retry of the accepted request recovers the same child and input without new Start consent.

Governing contracts: [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope), [owner opt-in](../concepts/security.g5.md#g5.security.agent-other-workspaces), [admitted agent work](../concepts/security.g5.md#g5.security.agent-admitted-work).

<a id="g5.story.agent-probe-child"></a>
[I · g5.story.agent-probe-child]

A workspace agent can discover its child workspaces and any workspace it may read, and read their lifecycle, chats, tasks, results and blockers without starting or prolonging their compute.

Governing contracts: [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope), [agent reads](../concepts/security.g5.md#g5.security.agent-read-scope).

<a id="g5.story.agent-chat-child"></a>
[I · g5.story.agent-chat-child]

A workspace agent can start new chats and send follow-ups in its owner's other workspaces within its [scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) through the shared [chat owners](chat.g5.md#g5.chat.ownership). Queued input alone never supplies Start consent. A fork's first chat, a new chat or a follow-up whose permission mode no agent turn can run, or whose launch choices fail validation, is refused before anything is created or queued, naming the modes that can run or the invalid choice.

Governing contracts: [agent chat control](chat.g5.md#g5.security.agent-chat-control).

<a id="g5.story.agent-merge-child"></a>
[I · g5.story.agent-merge-child]

A workspace agent can merge a child into the agent's own workspace through the same pull-request merge as the UI, with its [approvals](#g5.workspace.approval-human-authority) and [recovery](../software_architecture/git.g5.md#g5.git.merge-recovery). Its request is not an approval.

Governing contracts: [merge](#g5.workspace.merge), [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope).

<a id="g5.story.merge-when-ready"></a>
[I · g5.story.merge-when-ready]

A person shipping a pull request — its [source owner](#g5.governance.pr.source-owner-merge) or someone who may already merge the target — clicks Ship to parent, or Ship with agent, which is always offered and is the way forward when something blocks the ship: merge when ready turns on and the source workspace's agent keeps CI green and the branch rebased in a loop, and G5 merges the pull request into its parent once it is ready and the person still holds merge authority, without the agent gaining authority over the parent. The person and the agent learn that it merged. The agent never turns merge when ready on itself.

Governing contracts: [source-owner merge](#g5.governance.pr.source-owner-merge), [merge when ready](../software_architecture/git.g5.md#g5.git.pr-auto-merge), [with an agent](../software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent), [announcement](../software_architecture/git.g5.md#g5.git.pr-merge-announcement), [human decisions](../concepts/security.g5.md#g5.security.agent-human-exceptions).

<a id="g5.security.agent-workspace-delete"></a>
[I · g5.security.agent-workspace-delete]

A workspace agent may [delete](#g5.workspace.delete) another workspace within its [scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) but never the workspace its own VM credential identifies, including through compound paths; the refusal precedes any effect. A cascade that needs confirmation for another owner's children waits for the human. File deletion and the agent's own Stop are unaffected.

<a id="g5.story.stop-workspaces-by-user"></a>
[I · g5.story.stop-workspaces-by-user]

A user, any of their workspace agents or an organization administrator can stop every workspace that user owns in the organization with one request, for a runaway or any other reason.

Governing contracts: [stop by user](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user), [its release](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user.release).

## Owner email

<a id="g5.workspace.owner-email"></a>
[D · g5.workspace.owner-email]

An agent in a workspace's live VM can email that workspace's current owner with `g5d workspace email-owner`, giving a one-line subject and a body. The recipient is always the owner's login email, looked up when the request is made; the command takes no recipient, so it cannot reach collaborators, other members or outside addresses. A VM that is not the workspace's live incarnation is refused. Each message names the workspace and organization, links to the workspace, and says that the workspace's agent wrote it and that G5 did not; that notice comes [before the agent's text](../software_architecture/email.g5.md#g5.email.authored-content). It arrives as [outbound email](../software_architecture/email.g5.md#g5.email.consumers) and grants nothing. A request that repeats an idempotency key returns the earlier request instead of sending again.

<a id="g5.workspace.owner-email-format"></a>
[D · g5.workspace.owner-email-format]

The body is plain text by default. With `--format markdown` it is rendered as the email module's [Markdown subset](../software_architecture/email.g5.md#g5.email.markdown); the subject stays plain text. Its [clickable links](../software_architecture/email.g5.md#g5.email.link-policy) may point at G5 or at a Git-provider repository the workspace's organization has added, read when the request is made. The 20,000-character body limit applies to the source. A body the module refuses is refused before anything is recorded, and g5d exits 2 with the reason.

<a id="g5.workspace.owner-email-validity"></a>
[D · g5.workspace.owner-email-validity]

A queued owner email is [still wanted](../software_architecture/email.g5.md#g5.email.validity) only while the workspace exists and its current owner's login email is the recorded recipient. A deletion, an ownership transfer or an owner's email change before sending cancels it. A live workspace's owner is always a member of its organization, so membership needs no separate check. A VM that stops after requesting does not cancel it.

<a id="g5.workspace.owner-email-limit"></a>
[D · g5.workspace.owner-email-limit]

At most 50 new agent-to-owner emails may go to one owner within an organization, and 500 in total within that organization, in any rolling 24 hours ([volume limits](../software_architecture/email.g5.md#g5.email.volume-limits)). A request beyond either is refused as rate limited with a retry time, and nothing is written. Repeating an earlier request's idempotency key is not a new request.

<a id="g5.workspace.owner-email-preference"></a>
[D · g5.workspace.owner-email-preference]

Agent-to-owner email is mail the owner asked for: their own agent writes only to them, usually because they told it to report back. It therefore has no unsubscribe link and no preference store. The owner stops it by instructing their agents, and a later [notification settings](users.g5.md#g5.user.notifications) design may add a switch. This settles the [preference question](../software_architecture/email.g5.md#g5.email.preference-question) for this kind only.

## Metadata

### g5.workspace.identity

- human-provided
- Source: [Ben, line 53](../../../docs/ben_new_ontology.md#L53).
- Rename amendment, human-verified: case-insensitive uniqueness among live workspaces, the name as the URL address and owner renaming follow the [rename request and decisions](../../chat_intent/rename-workspace/request.md) (2026-10-23) and the [design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md). Migration 108 adds the partial unique index on live names ignoring case and refuses to apply over existing case-only duplicates (`migration_108_workspace_rename.rs`); `resolve_slugs` answers former names with the current name (`api_workspace_rename.rs`).

### g5.workspace.kind

- human-verified
- Source: [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) "Identity and naming"; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md#identity). The kind markers are `workspaces.is_base` and, since migration 110, `workspaces.personal_user_id`; the [personal identity metadata](personal_workspace.g5.md#g5.personal.identity) names their immutability triggers.

### g5.workspace.branch

- human-provided
- Source: [Ben, line 54](../../../docs/ben_new_ontology.md#L54).
- Fixed-at-creation amendment, human-verified: "rename a workspace must NOT touch git" ([rename request](../../chat_intent/rename-workspace/request.md), 2026-10-22). Migration 108 adds the immutable `workspaces.branch_name`, backfilled from the name, null for Base, defaulted from the name on insert; the two `authz` runtime lookups and every Git-facing reader (remote integrations, merge, reader and manifest, fork capture and branch ownership, push targets, VM start and rebase, incarnation, deletion, approval config, the review service's `workspace_owner_and_branch`) read it ([design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md)). `api_workspace_rename.rs` checks the branch, manifest and forks after a rename; `migration_108_workspace_rename.rs` checks the backfill, immutability and the definer lookup.

### g5.workspace.parent

- human-provided
- Source: [Ben, line 55](../../../docs/ben_new_ontology.md#L55).

### g5.workspace.fork

- human-provided
- Source: [Ben, line 56](../../../docs/ben_new_ontology.md#L56).

### g5.workspace.fork-state

- Formal child: [g5.workspace.fork-state.formal](#g5.workspace.fork-state.formal).

- human-provided
- Source: [Ben, line 57](../../../docs/ben_new_ontology.md#L57).

### g5.workspace.ancestor-read

- human-provided
- Source: [Ben, line 58](../../../docs/ben_new_ontology.md#L58).

### g5.workspace.base

- human-provided
- Source: [Ben, line 59](../../../docs/ben_new_ontology.md#L59).

### g5.workspace.mutation-authority

- human-provided
- Source: [Ben, line 60](../../../docs/ben_new_ontology.md#L60).
- Agent validation: [workspace audit](../../uplift/ben-ontology/investigations/workspace.md), WorkspaceAuthority checks owner, tenant, epoch and transfer-pending guards for regular runtime calls, not every endpoint.

### g5.workspace.transfer

- Formal child: [g5.workspace.transfer.formal](#g5.workspace.transfer.formal).

- human-provided
- Source: [Ben, line 61](../../../docs/ben_new_ontology.md#L61).
- Administrative transfer and active-recipient requirement: [member-deactivation request](../../chat_intent/member-removal/request.md#human-instructions). Transfer now locks active initiator and eligible recipient memberships through ownership commit, including after a pending drain.

### g5.workspace.interaction

- human-provided
- Source: [Ben, line 62](../../../docs/ben_new_ontology.md#L62).

### g5.workspace.git-authority

- human-provided
- Source: [Ben, line 63](../../../docs/ben_new_ontology.md#L63).

### g5.workspace.issue

- human-provided
- Source: [Ben, line 64](../../../docs/ben_new_ontology.md#L64).

### g5.workspace.stages

- human-provided
- Source: [Ben, line 65](../../../docs/ben_new_ontology.md#L65).
- The October15 correction in [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) supersedes the earlier optional phase-gating statement; phase associations recommend request timing only.

### g5.workspace.rebase

- human-provided
- Source: [Ben, line 66](../../../docs/ben_new_ontology.md#L66).

### g5.workspace.merge

- human-provided
- Source: [Ben, line 67](../../../docs/ben_new_ontology.md#L67).

### g5.workspace.review

- human-provided
- Source: [Ben, line 68](../../../docs/ben_new_ontology.md#L68).
- Code gap: [WORKSPACE-REVIEW-MERGE](../../../docs/code_gaps.md): current review policy forbids merge. The source permission remains governing.

### g5.workspace.delete

- human-provided
- Source: [Ben, line 69](../../../docs/ben_new_ontology.md#L69).
- Code gap: [WORKSPACE-CASCADE](../../../docs/code_gaps.md): deletion refuses live children instead of cascading. Intended/as-built acceptance models are separate; the human rule is preserved.

### g5.workspace.delete.pull-requests

- human-provided
- The owner, 2026-10-29: "deletion should close open PRs - nothing more to do" ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#deleted-workspaces-2026-10-29)). Reading "nothing more to do" as also settling provider PRs and approval commits is agent interpretation of the agent's preceding question.
- Closing first, in its own transaction, so a failed deletion is simply retried: the owner, 2026-10-30 ("order PR close first then the delete"). Blocking new PRs into a workspace whose deletion is admitted is agent design that keeps the close-first step complete.
- Implemented: `close_pull_requests_for_deletion` (`workspace_service.rs`, after deletion's checks, used by the user and agent delete paths) and again `close_deleted_workspace_pull_requests` (`coordinated_repo_publication.rs`) in `delete_workspace`'s tombstone transaction; PR creation holds both live workspace rows `FOR SHARE` and refuses a target whose deletion is reserved (`ensure_workspace_not_deleting`; the agent delete path reserves nothing, so only the tombstone close covers it); test `workspace_pull_request_auto_merge::no_pull_request_opens_into_a_workspace_being_deleted`; migration `145_pull_request_numbers.sql` closes earlier deletions' open PRs. Tests: `api_pull_request_listing::deletion_closes_open_prs_and_keeps_them_readable_from_either_side`, `migration_145_pull_request_numbers::numbers_pull_requests_and_closes_those_of_deleted_workspaces`.
- The one-time close of earlier deletions' open PRs is agent design: without it they would reappear on action surfaces once [reads include deleted workspaces](../software_architecture/git.g5.md#g5.git.pr-reads).

### g5.workspace.delete.admission

- agent
- Source: [issue #694](https://github.com/G5Labs/G5-Lean/issues/694), [design](../../pr_ontology_diff_design/issue-694/ontology-plan.md). Retained admission closes the race between eligibility validation and VM stop without holding database locks across runtime RPCs.

### g5.workspace.project-docs

- human-provided
- Source: [Ben, line 70](../../../docs/ben_new_ontology.md#L70).

### g5.workspace.project-docs-path

- human-provided
- Source: [Ben, line 71](../../../docs/ben_new_ontology.md#L71).

### g5.workspace.project-docs-agent

- human-provided
- Source: [Ben, line 72](../../../docs/ben_new_ontology.md#L72).

### g5.workspace.durable-read

- human-provided
- Source: [Ben, line 73](../../../docs/ben_new_ontology.md#L73).

### g5.workspace.owner-read

- human-provided
- Source: [Ben, line 74](../../../docs/ben_new_ontology.md#L74).

### g5.workspace.configuration-authority

- human-verified
- Source: [issue #577 owner/configuration decision](../../chat_intent/issue-577/request.md), [implementation design](../../pr_ontology_diff_design/issue-577/ontology-plan.md).
- Agent validation: [issue #577 results](../../pr_ontology_diff_design/issue-577/ontology-plan.md) exercise provider-account create/update/delete and raw configuration through real API/Postgres, including non-owner administrators, former/new owners, pending transfer and transaction-held authority. Production bundling remains limited by the test VM's memory ceiling.

### g5.workspace.audience

- human-verified
- security
- The reader rule and the retirement of invite-only audiences are the owner's decision 5 of 2026-10-30, human-provided ([request](../../chat_intent/security-invariants/request.md#decisions-the-owner-has-made-2026-10-30)); the fixed-by-identity model and its mechanics are agent design that the owner confirmed under issue #1107 below.
- Source: [issue #1107](https://github.com/G5Labs/G5-Lean/issues/1107), owner-confirmed removal of configurable sharing while preserving personal privacy and formal review isolation; [design and rollout boundary](../../pr_ontology_diff_design/issue-1107/ontology-plan.md). Supersedes the configurable audience in [workspace sharing v9](../../../docs/workspace-sharing-v9.md).
- [WorkspaceAudience](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked direct grants, owner-only audience changes, former-owner revocation and a separate authorized child-origin read. Mapping ancestor authority to human browsing remains unresolved; admission checks do not prove stream revocation or every RLS path.

- Formal-review exception: [g5.agent-approval.human-read-only](agent_approvals.g5.md#g5.agent-approval.human-read-only) ([issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [owner amendments](../../chat_intent/issue-640/review-decisions.md)).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 38 (OR-Q4).

### g5.workspace.content-policy

- agent
- Source: human [button-gates request](../../chat_intent/workspace-button-gates/request.md), [amended plan](../../ontology-plan-workspace-button-gates.md) and `workspace_mutation_policy` lines 1–109; detailed exceptions are recovered agent design at medium confidence.
- [ContentPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked application-write refusal while separately authorized Git operations remain possible. It does not classify every route or prove native filesystem immutability, Base exceptions or VM authority.

### g5.workspace.content-policy.transition

- agent
- Authority and operation fencing: [issue #694](https://github.com/G5Labs/G5-Lean/issues/694), [design](../../pr_ontology_diff_design/issue-694/ontology-plan.md).
- Source: `workspace_service` lines 2249–2825 and migration 164 at `5e309160`, which specifies manual crash recovery; its referenced original transition design is unavailable, so recovery detail has medium confidence.
- [ContentPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked draining admitted work without losing drafts, locked final quiescence, owner recovery after driver loss and restoration from read-only. Admission holds the inspected workspace lock through task creation; other producers, timeout bounds and automatic restart recovery are not proved.

### g5.workspace.content-policy.transition-authority

- agent
- Source: [issue #694](https://github.com/G5Labs/G5-Lean/issues/694), [reconciled design](../../pr_ontology_diff_design/issue-694/ontology-plan.md). The shared transition block owns per-write authority and exact-operation fencing; this specialization preserves transfer invalidation from PR #715.

### g5.workspace.rename

- human-verified
- Source: [rename request and decisions](../../chat_intent/rename-workspace/request.md) (2026-10-22, 2026-10-23); read-only and ordinary review-copy renaming are the human's decisions 3 and 4, and the formal agent-review exclusion and Base-as-product-rule wording follow the review corrections the human forwarded. Owner gate, transfer refusal, the database-only transaction and the durable rename record are agent interpretation recorded in the [design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md).
- `POST /api/v1/workspaces/:id/rename` (`routes/workspaces.rs`, `WorkspaceService::rename_workspace`, `workspace_names.rs`); the rename log is `workspace_renames`. The Rename control and dialog: `WorkspaceDetailsPanel.tsx` (Overview Name row, moved there from Home rows on the owner's 2026-10-24 instruction), `RenameWorkspaceDialog.tsx`. Tests: `api_workspace_rename.rs` (owner, Base, validity, pending transfer, read-only, conflicts), `WorkspaceDetailsPanel.test.tsx`, `RenameWorkspaceDialog.test.tsx`. Not covered by an executed browser flow: the CI flow only asserts the Rename action is offered.

### g5.workspace.former-names

- human-verified
- Source: decision 1 in the [rename request](../../chat_intent/rename-workspace/request.md) (2026-10-23): reserve while the workspace exists, retire after completed deletion. Taking an own former name back and the meaning of completed deletion are agent interpretation in the [design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md).
- `workspace_former_names` (migration 108) with `workspace_names::name_holder` shared by rename and fork claims under one organization lock; deletion removes the rows with the repository membership (`workspace_service.rs`); `resolve_slugs` and `g5d clone` bare names fall back to former names; the PR body document is read under former names too. Tests: `api_workspace_rename.rs` (rename cycles, reserved names refused for renames and forks, release at completed deletion), `WorkspacePageFormerName.test.tsx` (URL replacement).

### g5.workspace.review-decisions

- agent
- Source: human [comments request](../../chat_intent/comments-design/request.md), [comments plan](../../ontology-plan-comments.md) and transaction-scoped decision serialization in `workspace_reviews`/`workspace_review_service`.
- [ReviewAcceptance](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked selected-edit settlement and refusal to reopen decided rows. Its two-edit abstraction excludes full round carry-forward and artifact delivery; interrupted file application has the gap recorded under [acceptance](#g5.workspace.review-acceptance).

### g5.workspace.review-acceptance

- agent
- Source: human [issue-238 request](../../chat_intent/issue-238-plain/request.md), [accepted-file plan](../../pr_ontology_diff_design/issue-238-plain/ontology-plan.md) and `workspace_reviews` lines 1396–1818/1912–2090.
- Whitespace byte preservation: [issue #574](https://github.com/G5Labs/G5-Lean/issues/574) and [boundary design](../../pr_ontology_diff_design/fix-suggestions-render/ontology-plan.md).
- [ReviewAcceptance](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) found that a crash after the guest write can leave an open row that later records rejection/withdrawal; the intended comparator preserves applied truth and reaches an Accept retry without duplicate application. [WORKSPACE-REVIEW-SETTLEMENT](../../../docs/code_gaps.md) remains, alongside selected Save’s blob/native-writer gaps; source matching, artifact decisions and full edit composition are not proved.

### g5.workspace.review-rounds

- agent
- Source: [bounded audit](../../uplift/ben-ontology/quantitative/workspace.md), [comments design](../../ontology-plan-comments.md) and commits `2ba49163`, `a8596de1`, `ac3ea1d2`, ` lines 1469f0ea`.
- [ReviewRounds](../../uplift/ben-ontology/quantitative-refinement/workspace.md) checked supersession, distinct discussion threads and occurrence/content-bound decision and note retention, including abort/retry and repeated pushes. Reviewer, author kind and copy/in-place partition are fixed; serialized publication is assumed, while composed child edits, arbitrary hunk matching and artifact delivery are excluded.

### g5.workspace.review-discard

- agent
- Source: commits `11c2310d`, `f43da016`, ` lines 9851d021`, `workspace_reviews::discard_review_changes` and [bounded audit](../../uplift/ben-ontology/quantitative/workspace.md).
- [ReviewDiscard](../../uplift/ben-ontology/quantitative-refinement/workspace.md) checked stale-preview refusal, selected changes only, source isolation and truthful partial results, including interruption followed by a fresh remaining selection. Atomic version-check/replacement is the intended shared primitive: native check/rename and check/delete gaps remain, with no whole-request rollback or durable retry receipt claimed.

### g5.workspace.fork-state.formal

- agent
- Source: [workspace reconstruction](../../uplift/ben-ontology/investigations/workspace.md) and [source/model review](../../uplift/ben-ontology/reviews/workspace-git-full.md).
- ForkManifest checked frozen complete-version publication, operation-owned branch adoption/cleanup and crash/retry completion; live capture is an input premise. DirtyFork checked owner-only live capture, eligible reader snapshot/branch paths and inheritance of frozen dirty bytes; native capture atomicity is not proved.
- Expands: [g5.workspace.fork-state](#g5.workspace.fork-state) (formal).

### g5.workspace.transfer.formal

- agent
- Source: [workspace reconstruction](../../uplift/ben-ontology/investigations/workspace.md), [sharing v9](../../../docs/workspace-sharing-v9.md) and [backend v9](../../../docs/lean-backend-design-v9.md).
- WorkspaceAuthority checked logical stop-before-transfer and current-owner admission; physical termination, transfer idempotency and eligible-member constraints are separate inspected boundaries. [OwnerMembership](../../uplift/ben-ontology/quantitative-refinement/identity.md) checked refusal of owner removal/demotion despite competing creation or transfer, and removal after explicit transfer/deletion; PostgreSQL foreign-key/key-lock behavior is assumed, not executed.
- Expands: [g5.workspace.transfer](#g5.workspace.transfer) (formal).
- [Deactivation design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#transfer-and-presentation) keeps removal/demotion protection and ordinary transfer exclusions while requiring active actor/recipient checks through commit. `WorkspaceVmService::transfer` uses ordered membership locks at admission and final commit; the acquisition trigger additionally rejects inactive ownership. [Integration evidence](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence) covers current and revoked authority. Earlier WorkspaceAuthority/OwnerMembership models do not establish these changes.
- The personal-workspace exception to owner removal comes from [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) "Lifecycle" (human-verified); implemented by `personal_workspace::retire_in_tx` in the membership routes.

### g5.workspace.owner-limit

- human-verified
- Source: [owner request and decisions](../../chat_intent/owner-resource-limits/request.md) (default, review workspaces count); refusing transfers to an owner at the limit is agent design ([design](../../pr_ontology_diff_design/owner-resource-limits/ontology-plan.md#workspaces-per-owner)).
- Checked in `WorkspaceForkService::admit_operation` and `WorkspaceVmService::transfer` under a per-user advisory lock; the setting is `max_workspaces_per_owner` (migration 118).

### g5.workspace.phase-state

- agent
- Source: [Workbench human record](../../chat_intent/ux-v2-workbench/request.md#L105), [retained phase design](../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md#L164).
- [PR #567](https://github.com/G5Labs/G5-Lean/blob/d679e862f75bad311af814cf0558a8f75025581c/.g5/ontology/features/workbench.g5.md) keeps the event record and API projection; its display is governed by [Activity](ui.g5.md#g5.workbench.landing).
- Validation: [PhaseState](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) checks locked owner admission, atomic phase/event settlement and frozen fork inheritance; database transaction atomicity is a premise. No forced automatic phase progression is required.

### g5.workspace.approval-administration

- agent
- Source: [coverage C02/F1](../../uplift/ben-ontology/reviews/coverage-identity.md#f1), correction `46eaf0cb` and retained requirement policy.
- [ApprovalPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) distinguished administrator policy edits from owner review decisions. General role ceilings and the unresolved viewer policy remain separate; current `require_requirement_admin` uses locked organization authority.

### g5.workspace.approval-policy

- Current combined human/agent quorum and its new verification limits are owned by [approval quorum](reviews.g5.md#g5.governance.approval.quorum). Earlier all-required ApprovalPolicy checks below are historical for quorum semantics.

- agent
- Source: [UX-v2 request](../../chat_intent/ux-v2-workbench/request.md) and [approval design](../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md); IE-only/advisory semantics are agent interpretation, not attributed human text.
- [ApprovalPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked configured-target admission and all required human/agent approvals against concurrent requirement changes, publications, decisions and Undo. Source heads and actor eligibility are fixed inputs; reviewed-cut/artifact validation and Git merge effects are not re-proved.
- The latest [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) supersedes IE-only/all-human/phase-bound interpretations. Archived phase-policy checks describe the prior contract, not qualification of this amendment; actual API/pinned/provider-first regressions and current Cloud evidence are recorded in [the existing design](../../../docs/review-flow.md).

### g5.workspace.approval-human-authority

- agent
- Source: [coverage C02/F1](../../uplift/ben-ontology/reviews/coverage-identity.md#f1) and the human correction reported in `46eaf0cb`.
- [ApprovalPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked that another actor cannot fill a human reviewer’s slot. Request/phase/version settlement belongs to [approval requests](#g5.workspace.approval-request-settlement).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 39 (WS-Q6).

### g5.workspace.approval-definitions

- agent
- Source: [coverage C02/F1](../../uplift/ben-ontology/reviews/coverage-identity.md#f1), former version contract and definition service.
- [ApprovalPolicy](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked append-only publication, invalidating/non-invalidating versions and fresh approval after invalidation. Inspected definition locks serialize publishers; a later serialized publication may append normally, so the old unconditional-conflict comment is not governing intent.

### g5.workspace.approval-request-settlement

- agent
- Source and checks: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md).
- ApprovalRequest checked the addressed actor, phase, definition version, equal retry and preserved requester across a continuing chain. Its bounded chain excludes requestless decisions, merge admission and membership-revocation races.
- The October15 [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) replaces phase-specific identity. Earlier ApprovalRequest phase/chain modeling remains historical evidence only; real HTTP retries, phase moves and immutable settlement regressions qualify the current producer boundary.

### g5.workspace.approval-cut-eligibility

- agent
- Source and checks: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md).
- ApprovalEligibility checked complete source-version matching, definition eligibility and evidence-generation rules, including acceptance of compatible legacy records and refusal of malformed modern evidence. Artifact validity is abstracted; producer execution, cryptography and all membership races are not proved.
- The latest [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) removes the phase axis and preserves the existing agent evidence generations. Archived checks of phase/Undo paths do not establish the new request lifecycle; the amendment’s API/agent/evaluator tests provide scoped execution evidence.

- Agent clause reconciled with issue #640 and the [owner amendments](../../chat_intent/issue-640/review-decisions.md); Job-era run-detail and publication evidence was removed by migration 088.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 40 (WS-Q7).

### g5.workspace.approval-review-verdict-question

- human-provided; the latest review direction resolves the prior question.
- Source: [issue278 H1/H2](../../ontology-review-issue-278.inputs.md), its [unresolved sync/withdrawal review](../../ontology-review-issue-278.r3.md) and alternate-writer commits `11c2310d`, `f43da016`, ` lines 598b13ba`.
- [ReviewVerdictSync](../../uplift/ben-ontology/quantitative-refinement/workspace.md) checked exact-version, Approve-only and phase-local settlement, with valid request and owner paths; observed requestless nonowner fallback fails only the provisional authority comparison. Membership/definitions are fixed and withdrawal is unmodeled; this does not settle the Q or reproduce a runtime exploit.
- Resolved by [issue447 human direction](../../chat_intent/qa-review-prototype/request.md): broad verdicts are feedback; formal decisions require addressed requests. Earlier provisional requestless-owner/phase-local model branches are superseded, not runtime qualification. Real review-copy regressions preserve publication while leaving formal requests pending.

### g5.governance.pr.base-admin

- human-provided, human-verified
- Restored from the pre-reorganization governance page (`5c66d337a^:.g5/ontology/features/governance.g5.md`), where showing the Show All PRs actions to administrators on Base was human-provided ([request](../../chat_intent/base-pr-admin-actions/request.md)) and the server authority, Base-target scope and exclusions were human-verified ([plan](../../pr_ontology_diff_design/base-pr-admin-actions/ontology-plan.md)). The old claim that the merge "runs under the acting administrator's provider authority" is not restored: ordinary merges in App-bound organizations call GitHub as the App installation, and only the [GitHub override](../software_architecture/git.g5.md#g5.git.provider-override) uses the person's identity ([#550](https://github.com/G5Labs/G5-Lean/issues/550)). Restored by [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) ([design](../../pr_ontology_diff_design/issue-873/ontology-plan.md)).
- Implemented · `routes/workspaces.rs::scoped_db_for_owner_or_base_admin` → `scoped_db_for_base_admin` → `admit_base_admin` (`workspace_mutation_policy.rs::base_admin_subject_org`, `require_org_admin_on_conn`); `PullRequestMerger::WhenReady` re-admits the same way. UI gate: `workspaceLiveGate.tsx::ownerOrBaseAdmin`, `PullRequestList.tsx`. Tests: `api_workspace_pull_requests.rs` Base-admin cases, `route_auth_static.rs::owner_or_base_admin_admission_keeps_owner_and_base_admin_checks`.

### g5.governance.pr.source-owner-merge

- Override eligibility clarified by the owner's [policy decision for #1116](../../chat_intent/issue-1116/override-policy.md); source ownership still grants no other target writes.
- human-provided (owner decisions 1–2, 2026-11-05) · [intent](../../chat_intent/issue-1104/request.md), [design](../../pr_ontology_diff_design/issue-1104/ontology-plan.md).
- Open assumptions (admin-only attachment, any requirement kind) are the owner's, kept explicit and unresolved.
- Implemented · one predicate `workspace_mutation_policy.rs::source_owner_may_merge` (`is_source_owner_of_pull_request` ∧ `target_has_attached_requirement`); the merge surfaces ask it through `routes/workspaces.rs::scoped_db_for_source_owner_merge` (`PullRequestMerger::Person`/`WhenReady::target_db`), which opens the member's write capability with `scoped_db_for_user_actor_in_org` because the `workspaces:write` scope admits only the target's owner. Enable calls the same helper without the requirement check, so it reuses `is_source_owner_of_pull_request` alone. Tests: `workspace_pull_request_auto_merge.rs` member immediate-merge, enable, background-merge and lapse cases; `route_auth_static.rs`.

### g5.workspace.merge.admin-override

- human-verified (the owner's [issue #873](https://github.com/G5Labs/G5-Lean/issues/873), drafted in their personal notes); the browser-session mechanism is agent design.
- One localized authority rule: human-provided, the owner's implementation instruction ([intent](../../chat_intent/issue-873/request.md)).
- Design: [original plan](../../pr_ontology_diff_design/issue-873/ontology-plan.md) and [current override policy](../../pr_ontology_diff_design/issue-1116/ontology-plan.md). Implemented · shared authority in `services/git/merge_override.rs`, consulted by the preview and execution, including transactional rechecks of a new G5 waiver.

### g5.workspace.merge.admin-override.formal

- Expands: [g5.workspace.merge.admin-override](#g5.workspace.merge.admin-override) (formal)
- human-verified (shown set, re-check, record, bypassed requests and flagging from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873)); agent (source version in the shown set, recovery retention, the attempt event, empty set refusing).
- Current ordinary merge and applicable override authority through execution clarified for [issue #1116](https://github.com/G5Labs/G5-Lean/issues/1116), under [current authority](../concepts/security.g5.md#g5.security.current-authority); [design and validation](../../pr_ontology_diff_design/issue-1116/ontology-plan.md).
- Implemented · preview and merge `routes/workspace_merge_override.rs`; `merge_override::apply_override` under the admission locks (`WorkspaceMergeService::admit_target_merge`); `merge_overridden` event and `bypassed` requests in `coordinated_repo_publication.rs`. Tests: `workspace_merge_override.rs`, `api_workspace_pull_requests.rs::a_github_blocked_repository_refuses_the_merge_until_a_github_override`.
- Checkbox confirmation instead of typing the workspace name: human-provided, the owner's answer in the question set `admin-merge-override` (docs repository, `docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`) (2026-10-27).
- One confirmation and record for both overrides, each under its own authority: human-verified from [#873](https://github.com/G5Labs/G5-Lean/issues/873) coordination and [#550](https://github.com/G5Labs/G5-Lean/issues/550) (included in this design at the owner's request, 2026-10-27); offering it only to a person holding every present group's authority is agent design.

### g5.workspace.merge.admin-override.scope-question

- human-provided
- Source: the owner's [policy decision for #1116](../../chat_intent/issue-1116/override-policy.md): ordinary merge authority determines eligible targets.
- History: this replaces the Base-or-owned-target restriction chosen for [issue #873](https://github.com/G5Labs/G5-Lean/issues/873), decision 1 in the workspace question set `admin-merge-override` on 2026-10-27 ([intent](../../chat_intent/issue-873/request.md)). The ID keeps its former question name.

### g5.workspace.merge.admin-override.self-question

- human-provided: the owner chose the recommended option of [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) decision 2 in the workspace question set `admin-merge-override` (docs repository, `docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`) on 2026-10-27 ([intent](../../chat_intent/issue-873/request.md)). The ID keeps its former question name.

### g5.workspace.merge.admin-override.opt-out-question

- human-provided: the owner chose the recommended option of [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) decision 3 in the workspace question set `admin-merge-override` (docs repository, `docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`) on 2026-10-27 ([intent](../../chat_intent/issue-873/request.md)). The ID keeps its former question name.

### g5.story.merge-with-override

- human-verified; from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) and [#550](https://github.com/G5Labs/G5-Lean/issues/550).

### g5.story.agent-fork-child

- human-provided
- Source: [child workspace operations](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request). Replay without new Start consent applies [`g5.lifecycle.start`](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start).
- Implemented: `POST /api/internal/workspace-agent/:id/fork` ([routes](../../../backend/g5-api-routes/src/routes/workspace_agent.rs)) runs the durable `WorkspaceForkService::fork_workspace` as the owner, then `WorkspaceVmService::launch_forked_workspace`, whose intent is derived from the fork's chat and admitted `FirstOnly`; the UI's `clone_workspace` uses the same launch, closing its replay-restart gap. [Design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#fork-and-launch).
- Evidence: `a_fork_with_a_prompt_replays_without_reviving_a_stopped_launch` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)).
- Fork sources (2026-10-26): the owner's decision 2 (Base plus owned workspaces) and owner-scope instruction ([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)); the request's `sourceWorkspaceId` (`g5d workspace fork --from`) defaults to the caller and is admitted as Base or under `agent_target`; a personal source is refused by the fork itself. Evidence: `a_personal_agent_forks_base_and_manages_its_owners_workspaces`.
- The linked issue: the owner's 2026-10-26 request ([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)); the route passes the request's issue to the same fork admission and finalization the UI's dialog uses ([increment 6](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-6-fork-with-an-issue-and-member-lookup)). Evidence: `a_fork_links_its_issue_inside_the_fork_as_the_ui_fork_does`.

### g5.story.agent-probe-child

- human-provided
- Source: [child workspace operations](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request).
- Implemented: `children` and `workspaces/:targetId` agent routes; the probe composes workspace, VM view, tasks and chats from the database only ([scope](../../../backend/g5-api-core/src/db/workspace_agent_scope.rs)). A failed task carries its outcome's reason and text detail (`TaskListEntry.failure`), which `g5d workspace show` prints and the UI's task row shows on hover.
- Evidence: `operations_reach_this_workspace_and_its_owners_other_workspaces`, `reads_reach_every_workspace_the_owner_can_read_and_start_nothing`. The probe's `relation` is `self`, `child` or `owned` when the agent may operate on the workspace, else `other`.

### g5.story.agent-chat-child

- human-provided
- Source: [child workspace operations](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request).
- Implemented: new chats through `WorkspaceMessenger::send`; follow-ups through `routes/chat.rs::queue_follow_up`, shared with the UI's `send_message`, with keys claimed via `MessageTarget::ExistingSession`. A compound `start` starts only on first acceptance.
- Agent amendment (2026-10-24): refusing unrunnable permission modes up front follows a preview run in which a child's `acceptEdits` turns were queued and then failed dispatch admission with no visible reason ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-5-runnable-launches)). The runnable set is `agent_dispatch::agent_turn_permission_modes`, checked against admission by a unit test; the routes resolve the target's effective launch (a follow-up uses the chat's fixed mode). Since main's provider catalogs offer only executable modes (2026-10-26 rebase), a new launch outside them fails launch validation, which the routes also return before the fork or chat exists; the runnable check still catches a chat whose fixed mode predates the catalog.
- Gap: agent follow-ups queue; they are not steered into a running Claude turn as the composer's can be.
- Evidence: `child_chats_and_follow_ups_queue_once_and_never_start_the_child`, `launches_no_agent_turn_can_run_are_refused_before_anything_is_queued`.

### g5.story.agent-merge-child

- human-provided
- Source: [child workspace operations](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request). Using the UI's pull-request merge is agent design: [merge into a running parent](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#merge-into-a-running-parent).
- Implemented: `PullRequestCaller::WorkspaceChild` publishes a child's PR into the caller; `routes/workspaces.rs::merge_pull_request_as` is the UI's merge, shared with `PullRequestMerger::WorkspaceAgent` for PRs targeting the caller.
- Evidence: `an_agent_publishes_and_merges_a_childs_pull_request_into_its_workspace`.

### g5.story.merge-when-ready

- human-provided
- Source: [merge-when-ready request](../../chat_intent/merge-when-ready/request.md); the agent's part (CI and rebase) is human-provided, G5 performing the merge is agent design in [the plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md).
- Amended 2026-11-05 ([#1104](https://github.com/G5Labs/G5-Lean/issues/1104)): a shipping person is now the source owner or a target merger, with merge authority rechecked at merge time.

### g5.security.agent-workspace-delete

- human-provided
- Source: [child-only workspace deletion](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-instructions-recorded-by-earlier-sessions), recorded by an earlier session as the owner's instruction.
- Implemented: `workspaces/:targetId/delete` is `OtherOnly` and runs the UI's Stop then `delete_workspace`. `WORKSPACE-CASCADE` (see [`g5.workspace.delete`](#g5.workspace.delete)) still refuses a workspace with live children, and a personal workspace's deletion is refused by its [refusals](personal_workspace.g5.md#g5.personal.refusals). Reaching the owner's workspaces beyond children follows the 2026-10-26 [owner scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) instruction and decision 1.
- Evidence: `an_agent_deletes_a_child_but_not_a_child_with_children`; self-delete refusal in `operations_reach_this_workspace_and_its_owners_other_workspaces`.

### g5.story.stop-workspaces-by-user

- human-verified
- Source: ["stop workspaces by user"](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request); the user proposed the primitive and its non-runaway uses. The set of callers is agent design.
- Implemented: `g5d workspace stop-all`, `POST /api/v1/orgs/:orgId/members/:userId/stop-workspaces` and the Members page action.

### g5.workspace.owner-email

- human-verified
- Source: owner request (2026-10-19) to "design a 'g5d' cli command for sending email to the workspace owner", and the owner's approval of the design: "both of your recommendations look fine. build it" ([chat intent](../../chat_intent/issue-621/request.md#local-cap-and-owner-email-command)). Owner-only recipient, plain text, the attribution footer and idempotency are the agent's [design](../../pr_ontology_diff_design/issue-621/ontology-plan.md#g5d-owner-email).
- Implementation: `g5d workspace email-owner` (`backend/g5d/src/cli/email_owner.rs`), `POST /api/internal/workspaces/:id/owner-email`, `services/workspace/workspace_owner_email.rs`; agent guidance in `.agents/AGENTS.md`.
- Evidence: `backend/g5-api/tests/workspace_owner_email.rs`, and unit tests in the g5d command, the service and the email templates.
- Amended by [issue #834](https://github.com/G5Labs/G5-Lean/issues/834): bodies may be Markdown (see [`g5.workspace.owner-email-format`](#g5.workspace.owner-email-format)), and the authorship notice also comes before the text.

### g5.workspace.owner-email-format

- human-verified
- Source: [issue #834](https://github.com/G5Labs/G5-Lean/issues/834) proposes `--format markdown` with `text` as the default, links to the public G5 host or "a Git-provider repository attached to the workspace's organization", an unchanged limit on the source, and exit 2 on refusal. The owner's request is quoted there. The owner chose every Git-provider repository of the organization over only the workspace's repositories (decision, 2026-10-26); deriving each scope from `repos.git_url` without `.git` is the agent's [design](../../pr_ontology_diff_design/issue-834/ontology-plan.md#repository-scopes).
- Implementation: `g5d workspace email-owner --format`, the route's `format` field, and `organization_repository_scopes` in `services/workspace/workspace_owner_email.rs`.
- Evidence: `workspace_owner_email::a_markdown_body_links_only_to_g5_and_the_organizations_repositories` (another organization's repository is not clickable) and `a_markdown_body_outside_the_bounds_is_refused_with_its_reason_and_nothing_written` (422, nothing recorded), plus the g5d parser test for `--format`.

### g5.workspace.owner-email-validity

- agent
- Source: [design](../../pr_ontology_diff_design/issue-621/ontology-plan.md#validity). Membership needs no check because `workspaces_owner_same_org_fk` requires a live workspace's owner to be an eligible member.
- Evidence: `workspace_owner_email::a_transfer_deletion_or_email_change_before_sending_cancels_the_message`.

### g5.workspace.owner-email-limit

- human-verified
- Source: owner accepted the recommended `outbound-email` / `owner-email-limits` option, 50 per owner and 500 per organization (chat, 2026-10-19). The limits are required by `g5.email.volume-limits`, because the agent controls this kind's volume.
- Evidence: `workspace_owner_email::the_fifty_first_message_to_one_owner_in_a_day_is_refused` and `the_five_hundred_and_first_message_in_an_organization_is_refused`.

### g5.workspace.owner-email-preference

- human-verified
- Source: owner accepted the recommended `outbound-email` / `owner-email-preference` option, "No opt-out now: it is owner-requested mail" (chat, 2026-10-19).
