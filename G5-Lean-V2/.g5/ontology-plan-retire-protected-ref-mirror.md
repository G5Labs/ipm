# Retire the protected-ref mirror; track Base like any other workspace

## Ontology changes and governance

**Comparison base:** `origin/main` at `20df7d80` (branch `retire-protected-ref-mirror`). Intent record: [request](chat_intent/retire-protected-ref-mirror/request.md).

**Ontology changes:**

- **Modified** [`g5.git.mirror`](ontology/software_architecture/durability.g5.md#import-provider-history-only-through-remote-tracking). Provider history reaches a workspace branch only through remote tracking, and no poller writes a workspace branch. It keeps its ID and is retitled.
- **Modified** [`g5.git.mirror.update`](ontology/software_architecture/durability.g5.md#update-from-the-parents-accepted-branch). Update reads the parent's accepted branch; webhook payloads still never become authority.
- **Removed** `g5.git.mirror.freshness`. Mirror freshness state is retired with the mirror; remote-tracking run history records fetch failures.
- **Modified** [`g5.architecture.git_owner`](ontology/software_architecture/git_governance.g5.md#git-policy-owns-authoritative-ref-effects) and its operations row. "Protected-ref import" is dropped, and a gap is recorded.
- **Added** story [`g5.story.base-follows-provider`](ontology/features/workspaces.g5.md#follow-the-provider-in-base), indexed in [core user stories](ontology/concepts/core_user_stories.g5.md).
- **Added** [`g5.workspace.remote.base-parity`](ontology/features/workspaces.g5.md#track-base-like-any-other-workspace) (D) and [`g5.workspace.remote.base-auto-pull-default`](ontology/features/workspaces.g5.md#start-base-with-auto-pull-on) (I).
- **Modified (governing link only)** [`g5.workspace.identity.base`](ontology/features/workspaces.g5.md#keep-base-workspaces-as-protected-readers). The commitment text is unchanged.
- **Governing doc aligned:** [provider transport](../docs/data-model-cleanup-plan.md#provider-transport-and-publication), which the ontology names as owner of tracking mechanics. Its Base-specific coordinated cut and the `repo_mirror_refs` row are replaced. Its dated history and validation sections are left as evidence.

The actual diff at the original handoff (the intent record is a new file, not shown):

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index 7d35333b..64527f3a 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -47,4 +47,5 @@ These links add no separate commitments or implementation percentages.
 - [See each repository's remote-tracking state](../features/workspaces.g5.md#see-each-repositorys-remote-tracking-state).
 - [Resume tracking after resolving a divergence](../features/workspaces.g5.md#resume-tracking-after-resolving-a-divergence).
+- [Follow the provider in Base](../features/workspaces.g5.md#follow-the-provider-in-base).
 
 ## Files, documents and comments
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index be5d52c0..4e15466c 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -167,4 +167,14 @@ Governing concepts: Owner pushes use [current-owner authority](../concepts/secur
 Governing contracts: [complete a proven divergence resolution](#complete-a-proven-divergence-resolution), [publish a resolved divergence by push mode](#publish-a-resolved-divergence-by-push-mode).
 
+### Follow the provider in Base
+
+[I · g5.story.base-follows-provider]
+
+As a team member, I see Base pick up new provider commits on its remote-tracked repositories without anyone acting. I read Base's tracking state the same way as any other workspace's.
+
+Governing concepts: Base stays a [protected reader](#keep-base-workspaces-as-protected-readers); history follows [Git authority](../concepts/data_model.g5.md#git-owns-repository-content).
+
+Governing contracts: [track Base like any other workspace](#track-base-like-any-other-workspace), [start Base with auto-pull on](#start-base-with-auto-pull-on), [status in words](#name-each-repositorys-tracking-state-in-words).
+
 ## Behavioral contracts and design
 
@@ -198,5 +208,5 @@ Base is the ancestor workspace with no parent, regardless of its display name. A
 A connected [Base workspace](#identify-base-by-ancestry) is read-only for every user, reads existing protected/default refs without a working branch or VM and cannot be deleted.
 
-[Git authority](../concepts/data_model.g5.md#git-owns-repository-content) governs repository content; [administrator ownership transfer](../software_architecture/lifecycle.g5.md#preserve-administrator-transfer-of-a-base-workspace) remains a durable control-plane action.
+[Git authority](../concepts/data_model.g5.md#git-owns-repository-content) governs repository content; [administrator ownership transfer](../software_architecture/lifecycle.g5.md#preserve-administrator-transfer-of-a-base-workspace) remains a durable control-plane action. Provider history reaches Base's refs only through [ordinary remote tracking](#track-base-like-any-other-workspace).
 
 ### Omit Chat from Base
@@ -431,4 +441,29 @@ An automatic completion is not a publication request. The resolved tip reaches t
 Implicit publication follows [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); the actor's authority follows [publication under the acting human's provider authority](governance.g5.md#publish-under-the-acting-humans-provider-authority).
 
+### Track Base like any other workspace
+
+[D · g5.workspace.remote.base-parity]
+
+Base's remote-tracked repositories use the same remote tracking as every other workspace, acting under Base's current owner. That covers:
+
+- fetch, classification and fast-forward import, together with the provider observation and synchronization watermark;
+- the remote-tip advisory and the tracking states derived from it;
+- pause, divergence and provider-rewrite approval handling, and sync history;
+- the owner's Pull/Push actions and auto-pull/auto-push controls.
+
+These paths contain no Base-only importer, scheduler, policy setting, acting user or exemption. Each repository imports on its own, not as one multi-repository cut, as other workspaces do.
+
+Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM, such as **Resolve with agent**, is therefore unavailable on Base; [fork](#fork-to-make-my-own-changes) remains the route to edit. Base cannot gain local commits, so its non-fast-forward cases are provider rewrites, which use the explicit rewrite-approval checkpoint without a VM.
+
+Retires the [protected-ref mirror](../software_architecture/durability.g5.md#import-provider-history-only-through-remote-tracking) as a Base importer. Tracking mechanics follow [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); the acting owner's credentials follow [current-owner authority](../concepts/security.g5.md#dispatch-credentials-to-the-intended-owner-environment).
+
+### Start Base with auto-pull on
+
+[I · g5.workspace.remote.base-auto-pull-default]
+
+When a Base repository becomes remote-tracked, its auto-pull starts enabled. The owner can turn it off with the ordinary control. This default is the only remote-tracking difference between Base and other workspaces, which keep their existing auto-pull default.
+
+Applies within [Base parity](#track-base-like-any-other-workspace).
+
 ## Metadata
 
@@ -529,4 +564,10 @@ Implicit publication follows [provider transport](../../../docs/data-model-clean
 - Implemented · `remote_integrations/worker.rs::auto_complete_proven_divergence` completes a proven run before the cycle claim; the activity list labels the `resolved` audit row. Real Postgres/Git worker tests cover an agent rebase pushed through the production Git receiver, externally resolved runs in auto and manual push modes, unproven and merge-commit heads, and provider-rewrite checkpoints. Not exercised in a browser or against a live provider.
 
+### g5.story.base-follows-provider
+
+- human-verified
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user expected QA's Base, which matched GitHub, not to read "ahead", and asked that Base be tracked like any other workspace.
+- Not implemented · see the gaps under g5.workspace.remote.base-parity and g5.workspace.remote.base-auto-pull-default.
+
 ### g5.workspace.identity
 
@@ -754,2 +795,18 @@ Implicit publication follows [provider transport](../../../docs/data-model-clean
 - Design decision for [automatic divergence resolution](../../chat_intent/issue-240/auto-resolve-divergence.md): an automatic path must not bypass the auto-push toggle that governs implicit publication.
 - Implemented · `merge.rs` enqueues `push` only for owner requesters, including the late-confirmation replay. `worker.rs` advances the watermark on manual-mode `WorkspaceAhead` noops. Tests cover manual mode, auto mode without a GitHub App (implicit push refused) and late confirmation. Gap: a successful App-backed implicit publish was not exercised.
+
+### g5.workspace.remote.base-parity
+
+- human-verified
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user said "Ensure that Base is treated just like any other workspace" and then limited that to remote tracking. Per-repository import also matches [H014](../../uplift/g5-lean/candidates/human-decisions.md) (individual repository pushes, no atomic multi-repository cut).
+- Not implemented · Base-only paths remain:
+  - `g5-api-core/src/maintenance/base_auto_pull_scheduler.rs` and `services/workspace_service.rs::pull_remote_repository_set` run a coordinated all-repository Base pull. That pull acts as `workspaces.base_auto_pull_actor_user_id`, gated by `base_auto_pull_enabled`.
+  - `services/remote_integrations/worker.rs` drops Base's background-inbound cycles (`|| state.is_base`).
+  - `maintenance/remote_advisory_pinger.rs` skips Base.
+  - The protected-ref mirror writes Base's default branch directly.
+
+### g5.workspace.remote.base-auto-pull-default
+
+- human-provided
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): "auto-pull should be on by default for Base."
+- Not implemented · Base auto-pull is the workspace-level `base_auto_pull_enabled` flag, not the per-repository `remote_integrations.auto_pull_enabled` that other workspaces use. The QA reset seeds it disabled.
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index 8596e2b5..321351d5 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -178,23 +178,17 @@ Deny protected-ref, tag, branch-deletion and other-workspace network writes, kee
 [Accepted MVP object visibility](../concepts/security.g5.md#accept-repository-level-git-visibility-for-mvp) remains separate from ref-write authority.
 
-## Provider mirrors retain a truthful last good state
+## Import provider history only through remote tracking
 
 [D · g5.git.mirror]
 
-Poll configured protected refs every five minutes with the recorded repository connection and CAS-import provider rewrites.
+Provider history reaches a workspace branch, Base included, only through that member's remote tracking. Remote tracking advances the branch, its provider observation and its synchronization watermark together. No separate poller or protected-ref mirror writes a workspace branch or keeps its own freshness state. A configured protected branch still refuses [network writes](#restrict-network-ref-effects).
 
-External publication belongs to [governance](../features/governance.g5.md).
+Remote tracking follows [Base parity](../features/workspaces.g5.md#track-base-like-any-other-workspace) and [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication). External publication belongs to [governance](../features/governance.g5.md).
 
-## Show failed mirrors as stale
-
-[D · g5.git.mirror.freshness]
-
-Retain last good mirror bytes on missing/failure and show stale after any failure or three polling intervals.
-
-## Use authoritative mirrors for Update
+## Update from the parent's accepted branch
 
 [D · g5.git.mirror.update]
 
-Workspace Update uses the protected-ref mirror; future webhooks may trigger fetch but their payload OIDs never become authority.
+Workspace Update reads its parent workspace's accepted branch. For a child of Base, provider history reaches that branch only through Base's remote tracking. Future webhooks may schedule a remote-tracking fetch, but their payload OIDs never become authority.
 
 ## Keep platform artifacts outside native Git
@@ -370,19 +364,13 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
 ### g5.git.mirror
 
-- agent
-- From [C045–C049](../../uplift/g5-lean/candidates/contracts.md).
-- ~70% · protected_ref_mirror; credential selection can follow the current owner rather than the recorded connection.
-
-### g5.git.mirror.freshness
-
-- agent
-- From [C045–C049](../../uplift/g5-lean/candidates/contracts.md).
-- ~50% · mirror reader/freshness predicate; a failed fetch after a recent success can still appear fresh.
+- human-verified
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): on QA the mirror's direct write left Base's watermark behind, showing a false "ahead", and the user asked to retire it. This reverses the import half of [C045–C049](../../uplift/g5-lean/candidates/contracts.md), where the provider mirror was protected-ref authority.
+- Not implemented · `g5-api-core/src/maintenance/protected_ref_mirror.rs` still polls every repository's protected branches. It CAS-writes `refs/heads/<branch>` through `g5-scm/src/repo.rs::fetch_protected_ref` and records `repo_mirror_refs`. Base's default branch is that same ref.
 
 ### g5.git.mirror.update
 
 - agent
-- From [C045–C049](../../uplift/g5-lean/candidates/contracts.md).
-- ~80% · mirror/Update path; full Update consumer audit outstanding and webhook fetch remains deferred.
+- From [C049](../../uplift/g5-lean/candidates/contracts.md), retargeted from the retired mirror to the parent's accepted branch by [retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md).
+- Unassessed · the Update consumer was not re-audited against parent-branch reads; webhook fetch remains deferred.
 
 ### g5.git.exclusions
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 565e2924..05ffc8eb 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -16,5 +16,5 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
 | Owner | Callable surface | Contract |
 |---|---|---|
-| Git policy / storage adapters | Checked `/git/:repo_id` receive-pack, protected-ref import, publication and merge | [Ref effects](#git-policy-owns-authoritative-ref-effects) |
+| Git policy / storage adapters | Checked `/git/:repo_id` receive-pack, publication and merge | [Ref effects](#git-policy-owns-authoritative-ref-effects) |
 | CoordinatedRepoPublicationService / PreparedBranchCommit / ApprovalReviewService | Prepare/apply; `land*`; approval `admit`, `settle_terminal` | [Governance coordination](#publication-and-approval-coordination) |
 | DocService | `list_docs`, `get_doc`, `live_doc_read`, `sync_view_by_ref`, `create_explainer` | [Documents](#document-operations) |
@@ -30,5 +30,5 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
 [D · g5.architecture.git_owner]
 
-Git owners expose checked receive-pack, protected-ref import, publication and merge
+Git owners expose checked receive-pack, publication and merge
 operations over reusable `RepoStorage`, `GitRepo` and workspace Git mechanisms.
 
@@ -93,5 +93,5 @@ synchronization to API maintenance through shared typed Git readers.
 - agent
 - [Design v9 §5.3–§5.6](../../../docs/lean-backend-design-v9.md#53-receive-pack-policy-the-git-boundary).
-- ~75% · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops`; provider credential selection and equal-OID publication adoption still violate intended authority/proof rules; object-closure isolation is post-MVP.
+- ~75% · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops`; provider credential selection and equal-OID publication adoption still violate intended authority/proof rules; object-closure isolation is post-MVP. Gap: `g5-scm/src/repo.rs::fetch_protected_ref` remains a protected-ref import path until the mirror is retired under [g5.git.mirror](durability.g5.md#import-provider-history-only-through-remote-tracking).
 
 ### g5.architecture.governance_owner
diff --git a/docs/data-model-cleanup-plan.md b/docs/data-model-cleanup-plan.md
index f01e089b..9f27b470 100644
--- a/docs/data-model-cleanup-plan.md
+++ b/docs/data-model-cleanup-plan.md
@@ -26,9 +26,9 @@ See [migration policy](database-migrations.md).
   push/pull, sync history, conflict resolution, provider PRs and approvals.
   Put remote integration behind a canonical workspace/repository abstraction.
-- Base pull and working remote refresh share provider transport and ancestry
-  classification. Base publishes a complete fast-forward/equal cut. An explicit
-  working pull (or enabled auto-pull) also accepts provider fast-forwards into
-  the G5 branch, without changing VM files or saved recovery snapshots. Only
-  diverged histories need an agent-assisted rebase in the running VM.
+- Base and working workspaces share one remote integration: provider transport,
+  ancestry classification and per-repository import. An explicit pull (or
+  enabled auto-pull) accepts provider fast-forwards into the G5 branch, without
+  changing VM files or saved recovery snapshots. Only diverged histories need an
+  agent-assisted rebase in the running VM.
 
 These decisions supersede stored Git-state fields and synchronization retirement
@@ -49,5 +49,4 @@ unrelated product removal or repository relocation.
 | Integration queue, generation and leases | `remote_integration_queue` and `remote_ref_leases` |
 | Exact sync/merge attempt inputs and outcomes | `remote_sync_runs` and `job_merge_runs`; only evidence needed by that operation |
-| Fetch failures/provider-missing diagnostics | OID-free `repo_mirror_refs` ledger |
 | Docs and ontology indexing progress | `projection_state.applied_oid`; the version actually indexed |
 | Flush/fork/attach recovery and immutable boot inputs | Exact operation snapshots and their existing Git pins |
@@ -75,5 +74,5 @@ remains available for delivering provider observations without changing files.
 
 The integration fetches objects and classifies the exact provider tip using the
-shared pull engine. For a working workspace it stores the observation in Git
+shared pull engine. For every workspace, Base included, it stores the observation in Git
 under `refs/g5/provider/<workspace>/heads/<provider-branch>`. If G5's branch is an
 ancestor of the provider tip, explicit pull/push or enabled auto-pull accepts a
@@ -140,8 +139,9 @@ normal triggers after forwarding succeeds. Mere existence of a same-named
 provider branch never creates a binding or grants publication authority.
 
-Base has no VM and no normal VM snapshot. It uses the same fetch/classification
-engine, then publishes its complete protected fast-forward/equal cut with the
-existing membership, permission, policy and Git CAS checks. Base content and
-projections advance only after that successful coordinated publication.
+Base has no VM and no normal VM snapshot. Its remote-tracked repositories use
+this same integration, one repository at a time, acting under Base's current
+owner ([Base parity](../.g5/ontology/features/workspaces.g5.md#track-base-like-any-other-workspace)).
+Auto-pull starts enabled for Base. No mirror or other poller writes a workspace
+branch outside this integration.
 
 ## Reconciliation in the workspace
```

**Amendments after implementation review** ([review](ontology-review-retire-protected-ref-mirror.md)). The original handoff diff above is preserved; these edits follow it.
- **Amended** `g5.workspace.remote.base-parity`, because two of its claims were wrong:
  - "Base cannot gain local commits": a pinned Base can receive merges. Make Remote on Base now requires that Base can fast-forward to the provider branch.
  - "Base's rewrites use the rewrite-approval checkpoint without a VM": the existing approval publishes a VM-prepared candidate and would republish history the provider removed. Rewrite approval is now unavailable on Base.
- **Added** `Q · g5.workspace.remote.base-provider-rewrite`, an open human decision. Meanwhile Base stays paused safely.
- **PR-review amendment:** migration 056 became expand-only, and the planned 057 table drop was withdrawn (step 4). Footers say the drops await a contract migration.
- **Updated** footers to the current implementation: `g5.git.mirror`, `g5.git.mirror.update`, `g5.architecture.git_owner`, the three Base blocks, and the unchanged `g5.architecture.content_reader` and `g5.documents.read.labels`, which cited mirror freshness.

```diff
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 1561242e..5cd2d38f 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -582,5 +582,5 @@ Partial incorporation does not terminalize a future contribution session.
 - agent
 - From [C020–C022](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md). Outcome lineage: [v9](../../../docs/lean-backend-design-v9.md).
-- ~75% · reader DTOs and frontend source markers; failed provider refresh can appear fresh after a recent successful fetch, and complete renderer/source-marker coverage is unassessed.
+- ~75% · reader DTOs and frontend source markers; provider freshness is shown by remote-tracking status rather than a reader marker since the mirror ledger was retired, and complete renderer/source-marker coverage is unassessed.
 
 ### g5.documents.read.live-cache
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 56fc13bd..6339b404 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -454,5 +454,7 @@ Base's remote-tracked repositories use the same remote tracking as every other w
 These paths contain no Base-only importer, scheduler, policy setting, acting user or exemption. Each repository imports on its own, not as one multi-repository cut, as other workspaces do.
 
-Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM, such as **Resolve with agent**, is therefore unavailable on Base; [fork](#fork-to-make-my-own-changes) remains the route to edit. Base cannot gain local commits, so its non-fast-forward cases are provider rewrites, which use the explicit rewrite-approval checkpoint without a VM.
+Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM is therefore unavailable on Base: **Resolve with agent**, and approving a rebased candidate over a provider rewrite. [Fork](#fork-to-make-my-own-changes) remains the route to edit.
+
+Make Remote binds a Base repository only when Base can fast-forward to the provider branch. A Base that is ahead of the provider or has diverged from it is refused, rather than paused as diverged, because Base has no VM to reconcile it. Once remote-tracked, Base gains no local commits: merges into a remote-tracked target are refused and pushes to its protected branch are denied. Its remaining non-fast-forward case is a [provider rewrite](#accept-a-provider-rewrite-into-base).
 
 Retires the [protected-ref mirror](../software_architecture/durability.g5.md#import-provider-history-only-through-remote-tracking) as a Base importer. Tracking mechanics follow [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); the acting owner's credentials follow [current-owner authority](../concepts/security.g5.md#dispatch-credentials-to-the-intended-owner-environment).
@@ -466,4 +468,16 @@ When a Base repository becomes remote-tracked, its auto-pull starts enabled. The
 Applies within [Base parity](#track-base-like-any-other-workspace).
 
+### Accept a provider rewrite into Base
+
+[Q · g5.workspace.remote.base-provider-rewrite]
+
+When the provider rewrites a Base repository's tracked branch, Base pauses for a rewrite review, as any workspace does. The existing approval does not fit Base. It publishes a rebased candidate prepared in a VM and keeps G5's prior history, which would push the history the provider removed back to the provider. Until this is decided, Base stays paused, nothing is lost and no VM-only action is offered.
+
+Options:
+- **Accept the provider tip (recommended).** An owner approval resets Base's branch to the rewritten provider tip. The branch, provider observation and watermark move together, and nothing is pushed. Children keep their own history.
+- **Operator repair only.** Base stays paused until an operator reconciles the branch outside G5.
+
+Settled by a human choice between these. The choice affects [Base parity](#track-base-like-any-other-workspace) and [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication).
+
 ## Metadata
 
@@ -568,5 +582,5 @@ Applies within [Base parity](#track-base-like-any-other-workspace).
 - human-verified
 - [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user expected QA's Base, which matched GitHub, not to read "ahead", and asked that Base be tracked like any other workspace.
-- Not implemented · see the gaps under g5.workspace.remote.base-parity and g5.workspace.remote.base-auto-pull-default.
+- Implemented · Base imports provider fast-forwards through the ordinary worker; a provider rewrite stays paused under g5.workspace.remote.base-provider-rewrite. `remote_repo_tracking_integration.rs::base_tracks_its_provider_through_the_ordinary_worker` covers import, the equal-head watermark convergence and auto-pull off.
 
 ### g5.workspace.identity
@@ -800,14 +814,32 @@ Applies within [Base parity](#track-base-like-any-other-workspace).
 
 - human-verified
-- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user said "Ensure that Base is treated just like any other workspace" and then limited that to remote tracking. Per-repository import also matches [H014](../../uplift/g5-lean/candidates/human-decisions.md) (individual repository pushes, no atomic multi-repository cut).
-- Not implemented · Base-only paths remain:
-  - `g5-api-core/src/maintenance/base_auto_pull_scheduler.rs` and `services/workspace_service.rs::pull_remote_repository_set` run a coordinated all-repository Base pull. That pull acts as `workspaces.base_auto_pull_actor_user_id`, gated by `base_auto_pull_enabled`.
-  - `services/remote_integrations/worker.rs` drops Base's background-inbound cycles (`|| state.is_base`).
-  - `maintenance/remote_advisory_pinger.rs` skips Base.
-  - The protected-ref mirror writes Base's default branch directly.
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): the user said "Ensure that Base is treated just like any other workspace" and then limited that to remote tracking. Per-repository import also matches [H014](../../uplift/g5-lean/candidates/human-decisions.md) (individual repository pushes, no atomic multi-repository cut). The Make Remote fast-forward rule and the unavailable rewrite approval are agent amendments from the implementation review.
+- Implemented · the coordinated Base pull, `base_auto_pull_scheduler`, `workspaces.base_auto_pull_*` (migration 056) and the Base exemptions are removed. The exemptions were in the background scheduler (`remote_integrations/policy.rs::enqueue_background_inbound_rows`), `passive_candidates`, the worker's background-inbound gate, the advisory pinger and `set_auto_pull`. `/sync` queues per repository (`workspace_service.rs::sync_workspace`), and cycles act as the current owner via `POLICY_QUERY`. Base-only refusals that remain:
+  - Make Remote: the default branch, and fast-forward only (`BASE_REMOTE_BIND_REQUIRES_FAST_FORWARD`).
+  - Publish.
+  - Resolve with agent (`merge.rs`).
+  - Approved rebase publish.
+
+  `BranchTrackingPopover.tsx` hides the VM-only actions on Base. Coverage:
+  - Background scheduler: `remote_tracking_background_loop.rs::scheduler_enqueues_base_like_any_workspace`.
+  - Advisory pinger: `remote_advisory_pinger.rs::advisory_ping_observes_base_like_any_workspace`.
+  - Worker: the Base worker test.
+  - Make Remote: `make_remote_outcomes.rs::base_make_remote_requires_fast_forward_and_starts_with_auto_pull_on`.
+  - Frontend: `BranchTrackingInline.test.tsx`.
+
+  Not exercised: fetches after a Base ownership transfer, and a provider rewrite on Base.
 
 ### g5.workspace.remote.base-auto-pull-default
 
 - human-provided
-- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): "auto-pull should be on by default for Base."
-- Not implemented · Base auto-pull is the workspace-level `base_auto_pull_enabled` flag, not the per-repository `remote_integrations.auto_pull_enabled` that other workspaces use. The QA reset seeds it disabled.
+- [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): "auto-pull should be on by default for Base." On the migration, the user said: "During the migration, switch all Base on."
+- Implemented · `remote_integrations/policy.rs::initial_auto_pull_sql` sets the default for every binding insert (`bind_published`, `bind_policy` and both `convergence.rs` inserts). Migration 056 switches existing Base bindings on. Coverage:
+  - `new_bindings_default_auto_pull_on_only_for_base` (Publish path).
+  - `base_make_remote_requires_fast_forward_and_starts_with_auto_pull_on` (Make Remote).
+  - `migration_056_base_remote_tracking_parity.rs` (populated upgrade as a non-BYPASSRLS owner).
+
+### g5.workspace.remote.base-provider-rewrite
+
+- agent, uncertain
+- Implementation review of [retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): `GitRepo::normalize_reviewed_publication_tip` keeps G5's prior history in the approved candidate, so lifting the Base refusal would republish removed provider history.
+- Open · Base pauses with `watermark_violation`. `workspace_service.rs::approved_rebase_publish` refuses Base (`APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE`), and the popover shows a paused notice instead of the approval.
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index 321351d5..134ea4f7 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -366,5 +366,5 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
 - human-verified
 - [Retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): on QA the mirror's direct write left Base's watermark behind, showing a false "ahead", and the user asked to retire it. This reverses the import half of [C045–C049](../../uplift/g5-lean/candidates/contracts.md), where the provider mirror was protected-ref authority.
-- Not implemented · `g5-api-core/src/maintenance/protected_ref_mirror.rs` still polls every repository's protected branches. It CAS-writes `refs/heads/<branch>` through `g5-scm/src/repo.rs::fetch_protected_ref` and records `repo_mirror_refs`. Base's default branch is that same ref.
+- Implemented · the protected-ref mirror loop, `GitRepo::fetch_protected_ref` and the `repo_mirror_refs` ledger (migration 057) are removed. Server-side provider imports into workspace branches remain only in remote integration: worker import, Make Remote and provider-PR merge import, all through `remote_integrations::stage_merge_convergence`. The protected-branch receive-pack guard (`git_policy::protected_branches_for_repo`) is unchanged. `remote_repo_tracking_integration.rs::base_tracks_its_provider_through_the_ordinary_worker` and `workspace_repository_model.rs` cover the outcome.
 
 ### g5.git.mirror.update
@@ -372,5 +372,5 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
 - agent
 - From [C049](../../uplift/g5-lean/candidates/contracts.md), retargeted from the retired mirror to the parent's accepted branch by [retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md).
-- Unassessed · the Update consumer was not re-audited against parent-branch reads; webhook fetch remains deferred.
+- Source-inspected · every reader of a default-branch ref resolves it through Base membership (`workspace_reader.rs`, `remote_integrations/repositories.rs`); no consumer treats it as an independent provider mirror. Webhook fetch remains deferred.
 
 ### g5.git.exclusions
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 4815c8c7..cbcf5535 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -138,5 +138,5 @@ synchronization to API maintenance through shared typed Git readers.
 - agent
 - [Design v9 §5.3–§5.6](../../../docs/lean-backend-design-v9.md#53-receive-pack-policy-the-git-boundary).
-- ~75% · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops`; provider credential selection and equal-OID publication adoption still violate intended authority/proof rules; object-closure isolation is post-MVP. Gap: `g5-scm/src/repo.rs::fetch_protected_ref` remains a protected-ref import path until the mirror is retired under [g5.git.mirror](durability.g5.md#import-provider-history-only-through-remote-tracking).
+- ~75% · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops`; provider credential selection and equal-OID publication adoption still violate intended authority/proof rules; object-closure isolation is post-MVP.
 
 ### g5.architecture.governance_owner
diff --git a/.g5/ontology/software_architecture/workspace.g5.md b/.g5/ontology/software_architecture/workspace.g5.md
index 8d6a86a9..c512e9f0 100644
--- a/.g5/ontology/software_architecture/workspace.g5.md
+++ b/.g5/ontology/software_architecture/workspace.g5.md
@@ -146,5 +146,5 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
 - agent
 - [Design v9 §9.1](../../../docs/lean-backend-design-v9.md#91-reads).
-- ~75% · `backend/g5-api-core/src/services/workspace_reader.rs`; failed mirror fetches can appear fresh, receipt selection/pinning can race pruning, and every document/tree/download caller has not been assessed.
+- ~75% · `backend/g5-api-core/src/services/workspace_reader.rs`; receipt selection/pinning can race pruning, and every document/tree/download caller has not been assessed.
 
 ### g5.architecture.fork_owner
```

**Follow-up: accept provider history on Base** ([intent](chat_intent/retire-protected-ref-mirror/request.md#base-provider-history-acceptance)). This resolves Q2. The previous text is preserved above; these edits follow it.
- **Changed** `g5.workspace.remote.base-provider-rewrite` from Q to D, retitled "Accept provider history into Base". A Base owner action resets Base to the reviewed provider tip, pushes nothing, closes open runs and pins the replaced head. It serves both provider rewrites and a non-fast-forward Make Remote.
- **Amended** `g5.workspace.remote.base-parity`. Make Remote on a Base that is ahead or diverged now binds it paused instead of refusing, and acceptance is Base's only reconciliation.
- **Amended** `g5.workspace.remote.base-auto-pull-default` wording and the story's governing links. `g5.governance.remote.divergence` now points Base to acceptance.

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index d85a42d6..40b6b3a1 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -461,5 +461,5 @@ For every workspace, Base included, explicit pull/push or enabled auto-pull may
 [D · g5.governance.remote.divergence]
 
-Automatic divergence pauses synchronization without launching resolution. An explicit owner action creates or reuses a resolution task in the already-running owner VM, with exact Git inputs pinned and delivered before launch; it preserves existing edits and publishes through G5. Provider history rewrites require separate approval. Viewing status or fetching never starts a VM.
+Automatic divergence pauses synchronization without launching resolution. An explicit owner action creates or reuses a resolution task in the already-running owner VM, with exact Git inputs pinned and delivered before launch; it preserves existing edits and publishes through G5. Provider history rewrites require separate approval. Viewing status or fetching never starts a VM. Base, which has no VM, instead reconciles by [accepting the provider history](workspaces.g5.md#accept-provider-history-into-base).
 
 ## Metadata
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 8fdac652..e2410668 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -159,5 +159,5 @@ As a team member, I see Base pick up new provider commits on its remote-tracked
 Governing concepts: Base stays a [protected reader](#keep-base-workspaces-as-protected-readers); history follows [Git authority](../concepts/data_model.g5.md#git-owns-repository-content).
 
-Governing contracts: [track Base like any other workspace](#track-base-like-any-other-workspace), [start Base with auto-pull on](#start-base-with-auto-pull-on), [status in words](#name-each-repositorys-tracking-state-in-words).
+Governing contracts: [track Base like any other workspace](#track-base-like-any-other-workspace), [start Base with auto-pull on](#start-base-with-auto-pull-on), [accept provider history into Base](#accept-provider-history-into-base), [status in words](#name-each-repositorys-tracking-state-in-words).
 
 ## Behavioral contracts and design
@@ -447,5 +447,5 @@ These paths contain no Base-only importer, scheduler, policy setting, acting use
 Tracking actions and policy controls are durable control-plane actions, not content edits. They stay available to Base's owner under [base protection](#keep-base-workspaces-as-protected-readers), which still forbids content edits, a working branch and a VM. A step that needs a running owner VM is therefore unavailable on Base: **Resolve with agent**, and approving a rebased candidate over a provider rewrite. [Fork](#fork-to-make-my-own-changes) remains the route to edit.
 
-Make Remote binds a Base repository only when Base can fast-forward to the provider branch. A Base that is ahead of the provider or has diverged from it is refused, rather than paused as diverged, because Base has no VM to reconcile it. Once remote-tracked, Base gains no local commits: merges into a remote-tracked target are refused and pushes to its protected branch are denied. Its remaining non-fast-forward case is a [provider rewrite](#accept-a-provider-rewrite-into-base).
+Once remote-tracked, Base gains no local commits: merges into a remote-tracked target are refused and pushes to its protected branch are denied. Base's history can still differ from the provider's in two ways: the provider rewrites the branch, or Make Remote binds a Base that is ahead of or diverged from the provider. Make Remote binds such a Base paused, as it does any workspace. In either case Base reconciles only by [accepting the provider history](#accept-provider-history-into-base).
 
 Retires the [protected-ref mirror](../software_architecture/durability.g5.md#import-provider-history-only-through-remote-tracking) as a Base importer. Tracking mechanics follow [retained remote integration](governance.g5.md#retained-remote-integration); the acting owner's credentials follow [current-owner authority](../concepts/security.g5.md#dispatch-credentials-to-the-intended-owner-environment).
@@ -455,19 +455,22 @@ Retires the [protected-ref mirror](../software_architecture/durability.g5.md#imp
 [I · g5.workspace.remote.base-auto-pull-default]
 
-When a Base repository becomes remote-tracked, its auto-pull starts enabled. The owner can turn it off with the ordinary control. This default is the only remote-tracking difference between Base and other workspaces, which keep their existing auto-pull default.
+When a Base repository becomes remote-tracked, its auto-pull starts enabled. The owner can turn it off with the ordinary control. Other workspaces keep their existing auto-pull default. This default is the only difference between Base's tracking policy and other workspaces'. Base's VM-free reconciliation follows [accept provider history](#accept-provider-history-into-base).
 
 Applies within [Base parity](#track-base-like-any-other-workspace).
 
-### Accept a provider rewrite into Base
+### Accept provider history into Base
 
-[Q · g5.workspace.remote.base-provider-rewrite]
+[D · g5.workspace.remote.base-provider-rewrite]
 
-When the provider rewrites a Base repository's tracked branch, Base pauses for a rewrite review, as any workspace does. The existing approval does not fit Base. It publishes a rebased candidate prepared in a VM and keeps G5's prior history, which would push the history the provider removed back to the provider. Until this is decided, Base stays paused, nothing is lost and no VM-only action is offered.
+When a remote-tracked Base repository's history differs from its provider branch, Base's owner can explicitly **Accept provider history**. This applies when the provider rewrote the branch, and when Make Remote bound a Base that was ahead of or diverged from the provider. Accepting adopts the provider repository and discards Base's local history for that repository:
 
-Options:
-- **Accept the provider tip (recommended).** An owner approval resets Base's branch to the rewritten provider tip. The branch, provider observation and watermark move together, and nothing is pushed. Children keep their own history.
-- **Operator repair only.** Base stays paused until an operator reconciles the branch outside G5.
+- Base's branch is reset to the provider tip the owner reviewed. The branch, provider observation and synchronization watermark move together in one landing, and the pause clears.
+- Nothing is pushed to the provider.
+- Any open resolution or rewrite-review run for that repository closes as resolved by acceptance.
+- The replaced Base head stays pinned in Git, so the discarded commits remain recoverable by an operator. Forks keep their own history.
 
-Settled by a human choice between these. The choice affects [Base parity](#track-base-like-any-other-workspace) and [provider-rewrite approval](governance.g5.md#resolve-divergence-through-an-explicit-owner-action).
+The action is bound to the provider tip and Base head the owner reviewed. If either moved, the action is refused and the owner reviews again. The confirmation states that Base's local commits for that repository will be discarded.
+
+This is the only reconciliation Base offers, because Base has no VM. It replaces, on Base, the rebased-candidate approval and **Resolve with agent** that other workspaces use. Base's owner exercises it as a durable control-plane action under [Base parity](#track-base-like-any-other-workspace).
 
 ## Metadata
@@ -844,5 +847,5 @@ Settled by a human choice between these. The choice affects [Base parity](#track
 ### g5.workspace.remote.base-provider-rewrite
 
-- agent, uncertain
-- Implementation review of [retire the protected-ref mirror](../../chat_intent/retire-protected-ref-mirror/request.md): `GitRepo::normalize_reviewed_publication_tip` keeps G5's prior history in the approved candidate, so lifting the Base refusal would republish removed provider history.
-- Open · Base pauses with `watermark_violation`. `workspace_service.rs::approved_rebase_publish` refuses Base (`APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE`), and the popover shows a paused notice instead of the approval.
+- human-provided, human-verified
+- [Base provider-history acceptance](../../chat_intent/retire-protected-ref-mirror/request.md#base-provider-history-acceptance): the user chose the owner approval that resets Base, and asked that the same action let Make Remote adopt the remote and discard Base's local history. The reviewed-tip binding, the recoverability pin and closing open runs are agent decisions. They follow from the implementation review finding that `GitRepo::normalize_reviewed_publication_tip` keeps G5's prior history, so reusing the rebased-candidate approval would republish removed provider history.
+- Not implemented · `workspace_service.rs::approved_rebase_publish` refuses Base (`APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE`). Make Remote refuses a non-fast-forward Base (`BASE_REMOTE_BIND_REQUIRES_FAST_FORWARD`). The popover shows only a paused notice.
```

**Merge reconciliation with main (G5Labs/G5-Lean#245).** Main moved retained remote integration into [`governance.g5.md`](ontology/features/governance.g5.md#retained-remote-integration) and rewrote the mirror blocks' credential wording and links. This PR's mirror retirement stands. The links now point to the new governance owners, and the obsolete mirror credential footer is superseded. Main's [`g5.governance.remote.pull`](ontology/features/governance.g5.md#publish-incoming-refs-without-changing-live-files) still said "Base publishes a complete protected fast-forward/equal cut", which contradicts `g5.workspace.remote.base-parity`, so it now describes per-repository import for every workspace, Base included.

**Relevant and governing ontology** (commitment → obligation → owner/operation → allowed case / counterexample → conclusion or open assumption):

- [`g5.git.mirror`](ontology/software_architecture/durability.g5.md#import-provider-history-only-through-remote-tracking) → only remote tracking imports provider history into a workspace branch.
  - Owner: remote integration (`remote_integrations` worker and publication).
  - Allowed: the worker fast-forwards Base `main` to the provider tip, writing the branch, provider observation and watermark in one cut.
  - Counterexample: any periodic job that CAS-writes `refs/heads/<default_branch>` from the provider. This includes today's `protected_ref_mirror` and `fetch_protected_ref`.
  - Conclusion: delete the mirror loop, its SCM import helper, the `repo_mirror_refs` ledger and the unused `protected_ref_observation` DTO field.
- [`g5.git.network.ref-scope`](ontology/software_architecture/durability.g5.md#restrict-network-ref-effects) (unchanged) → protected branches still refuse network writes.
  - `repos.protected_branches` and `git_policy::protected_branches_for_repo` stay as receive-pack guards.
  - Counterexample: removing the column along with the mirror.
- [`g5.git.mirror.update`](ontology/software_architecture/durability.g5.md#update-from-the-parents-accepted-branch) → Update and parent comparisons read the parent's accepted branch.
  - For Base's children, that branch moves only through Base remote tracking.
  - **Resolved by source inspection:** every default-branch read goes through Base membership, and no reader treats it as an independent provider mirror.
- [`g5.workspace.remote.base-parity`](ontology/features/workspaces.g5.md#track-base-like-any-other-workspace) → Base uses the ordinary per-repository integration under its current owner.
  - Owners: the integration worker, the advisory pinger, the ordinary sync/Pull route and the per-repository policy row.
  - Allowed: a background-inbound cycle for Base imports a provider fast-forward when auto-pull is on. The advisory pinger also observes Base, so Base can read "behind" until the import lands.
  - Counterexamples, all to be removed:
    - Background cycles dropped for `is_base`.
    - The advisory pinger skipping Base.
    - `base_auto_pull_scheduler`.
    - `pull_remote_repository_set` with its all-or-nothing preflight and `BASE_AUTO_PULL_REQUIRES_COORDINATED_REPOS`.
    - `workspaces.base_auto_pull_enabled` and `base_auto_pull_actor_user_id`.
- [`g5.workspace.remote.base-auto-pull-default`](ontology/features/workspaces.g5.md#start-base-with-auto-pull-on) → binding a Base repository creates its policy with auto-pull on.
  - Allowed: Make Remote, or seeding a Base binding, yields `auto_pull_enabled = true`. The owner may then turn it off.
  - Counterexample: a non-Base workspace's default changing.
- [`g5.workspace.identity.base`](ontology/features/workspaces.g5.md#keep-base-workspaces-as-protected-readers) (unchanged) → Base stays content-read-only, with no working branch or VM, and cannot be deleted.
  - Parity grants no content edit, VM start or push into Base. **Resolve with agent** stays unavailable on Base.
  - Once remote-tracked, Base gains no local commits, so its non-fast-forward cases are provider rewrites, or a Make Remote over existing Base history. Both reconcile only through [accepting the provider history](ontology/features/workspaces.g5.md#accept-provider-history-into-base); the rebased-candidate approval is unavailable on Base.
- [`g5.security.admission`](ontology/concepts/security.g5.md#admit-actions-against-current-authority) and [`g5.security.credentials`](ontology/concepts/security.g5.md#dispatch-credentials-to-the-intended-owner-environment) (unchanged) → tracking actions are durable control-plane actions under ordinary owner authority, and fetches use the current owner's provider credentials.
  - After [Base transfer](ontology/software_architecture/lifecycle.g5.md#preserve-administrator-transfer-of-a-base-workspace), the new owner's credentials apply, as for any transfer.
  - Counterexample: a separately recorded Base actor.
- [`g5.workspace.base-ui.*`](ontology/features/workspaces.g5.md#omit-chat-from-base) and [`g5.architecture.workspace_presentation`](ontology/software_architecture/workspace.g5.md#project-base-presentation-from-workspace-identity) (unchanged) → Base presentation omissions remain.
  - **Resolved by source inspection:** Base's read-only Details gates only the description and policy controls; the remote-tracking card and its controls render for Base.
- [`g5.workspace.remote.base-provider-rewrite`](ontology/features/workspaces.g5.md#accept-provider-history-into-base) → Base's owner explicitly accepts the provider history.
  - Owner: remote integration, behind a Base-only owner route. Non-Base workspaces keep the rebased-candidate approval and Resolve with agent.
  - Allowed cases:
    - After a provider force-push, Base is paused for rewrite review; the owner confirms, and Base is reset to the reviewed provider tip.
    - Make Remote on a Base whose `main` has merged commits the provider lacks binds it paused; the owner confirms, and Base adopts the provider tip.
  - In both cases the branch, provider observation and watermark land together, nothing is pushed, the pause clears, open runs close, and the old head stays pinned.
  - Counterexamples:
    - Acceptance on a non-Base workspace.
    - A request whose reviewed provider tip or Base head no longer matches (refused; review again).
    - Acceptance when Base is not behind a differing provider history (refused).
    - Pushing anything to the provider.
    - Losing the replaced head to GC before an operator can recover it.
- [`g5.workspace.remote-ui.status`](ontology/features/workspaces.g5.md#name-each-repositorys-tracking-state-in-words) (unchanged) → Base derives its pill from the same head, watermark and advisory inputs, so a Base equal to the provider reads healthy.
- [H014](uplift/g5-lean/candidates/human-decisions.md) (individual repository pushes, no atomic multi-repository cut) → supports dropping Base's coordinated all-repository cut.
- [`g5.design.authority`](ontology/concepts/system_design_principles.g5.md#give-each-fact-one-authority) and [`g5.architecture.policy`](ontology/concepts/system_design_principles.g5.md#share-policy-without-erasing-legitimate-exceptions) → one writer and one policy home for Base's synchronization.
  - The single legitimate Base variant is the auto-pull default. Base protection is unaffected by tracking.

## Requested outcome and scope

The user asked to retire the protected-ref mirror and treat Base like any other workspace, except that auto-pull is on by default for Base. They then limited "like any other workspace" to remote tracking. In scope:

- Removing the protected-ref mirror and its state.
- Removing every Base-only remote-tracking path: the coordinated pull, its scheduler, policy columns and actor, the worker and advisory exemptions.
- The Base auto-pull default.
- Migrating existing Base policy.

Out of scope: Base's read-only content, its absent VM and working branch, its fixed default-branch name, deletion protection, Base presentation rules and the protected-branch push guard.

## Design

1. **Retire the mirror.** Remove the maintenance loop and its registration, `fetch_protected_ref` and its private `refs/g5/mirror-fetch/*` namespace, and every `repo_mirror_refs` reader, including `protected_ref_observation` on the binding snapshot DTO. The table is dropped by the later contract migration (step 4). Keep `repos.protected_branches` for receive-pack policy.
2. **One Base sync path.**
   - Base manual Pull and sync requests go through the same per-repository integration enqueue as other workspaces.
   - Remove the coordinated `pull_remote_repository_set` branch, `prepare_base_pull`/`BasePullAdmission` if nothing else uses them, and `base_auto_pull_scheduler`.
   - The background scheduler and worker admit Base background-inbound cycles under the same `auto_pull_enabled` / push-mode gate as any workspace.
   - The advisory pinger includes Base.
3. **Policy home.**
   - Drop `workspaces.base_auto_pull_enabled` and `base_auto_pull_actor_user_id`. Base auto-pull is the per-repository `remote_integrations.auto_pull_enabled`, acting as the workspace's current owner.
   - Policy creation defaults that flag to true when the workspace is Base, and keeps the existing default otherwise.
4. **Migration (expand step).** Migration 056 sets `remote_integrations.auto_pull_enabled = true` for every existing Base binding (the user resolved Q1 this way) and clears the retired `workspaces.base_auto_pull_*` setting. Old API pods select those columns and join `repo_mirror_refs` during a Helm rolling upgrade, so the columns and the table are dropped by a later contract migration once no running image reads them.
5. **Seeds and runbooks.** `scripts/reset-qa.py` already seeds Base's binding with auto-pull on. `scripts/reset-qa.md` explains the expected "Authentication required" pause and the Retry step that clears it once the GitHub App is bound.
6. **Frontend.** Remove Base-specific auto-pull wiring, if any. Base's repository cards use the same tracking pill, detail and controls as other workspaces.

**Rationale.** Two writers updated Base's working ref: the mirror and remote tracking. Only remote tracking keeps the watermark and provider observation consistent with the head. With the mirror and the Base-only path both active, Base's displayed state and its pull policy were wrong:

- QA's Base read "ahead" while equal to GitHub, because the mirror moved `main` but not the watermark.
- The mirror ignored the auto-pull toggle.
- The mirror force-imported provider rewrites, and would discard a pinned Base's merged commits.

## Acceptance outcomes

- No code path writes a workspace branch from the provider except remote integration. The mirror loop, `fetch_protected_ref`, `repo_mirror_refs` and `protected_ref_observation` are gone.
- Base background sync:
  - With Base auto-pull on, a provider fast-forward on Base's tracked branch is imported by an ordinary background cycle.
  - Head, provider observation and watermark advance together, and the pill reads "Tracking healthy".
  - With auto-pull off, Base, like every workspace, never imports automatically; importing a provider fast-forward needs a manual Fetch. In auto push mode a background cycle may still fetch and record the provider observation without changing Base; in manual push mode automatic inbound work is gated off.
- A Base repository whose head equals the provider tip but whose watermark lags reads healthy after one integration cycle (the QA case). The cycle advances the watermark without moving the head.
- Base manual Pull imports per repository. One repository's failure does not block another's import, and it is reported per repository like any workspace.
- Make Remote on Base creates `auto_pull_enabled = true`, and on a non-Base workspace keeps the existing default. The owner can toggle Base auto-pull with the ordinary control. A Base that is ahead of or diverged from the provider binds paused (the follow-up design below replaces the earlier refusal).
- A provider rewrite of Base's tracked branch pauses Base; the branch is not force-imported, and no VM-only action is offered. Reconciliation is the follow-up's Accept provider history. Push to Base's protected branch is still refused.
- A pinned Base repository (no binding) never moves when its provider branch moves.
- After Base ownership transfer, Base fetches use the new owner's credentials.
- Base's read-only presentation is unchanged: no Chat, Owner, VM status or terminal-fork shortcut.

## Risks

- **Pinned Base repositories stop following the provider.** This is intended by parity, but organizations that relied on the mirror must Make Remote on Base. Release notes or operators need to know.
- **Credentials.** Base fetches now use the current Base owner's resolved credentials. Before the GitHub App is bound, a Base cycle with auto-pull on pauses with `auth_required`. The QA reset already seeds Base auto-pull on; its runbook now says to use Retry after binding the App.
- **Rolling upgrade.** Old pods keep working during rollout: 056 drops nothing, and clearing the retired setting stops their coordinated Base pull. The drops wait for a follow-up contract migration.
- **Per-repository Base imports.** These can leave Base's docs and code repositories at different provider moments, as in any workspace (H014).

## Open questions

- **Q1 — existing Base auto-pull value (resolved).** The user chose to switch every existing Base on during the migration ([request](chat_intent/retire-protected-ref-mirror/request.md#migration-decision)).
- **Q2 — accepting a provider rewrite into Base (resolved).** The user chose the owner approval that resets Base, and extended it to Make Remote adoption. See [`g5.workspace.remote.base-provider-rewrite`](ontology/features/workspaces.g5.md#accept-provider-history-into-base) and the follow-up design below.

## Follow-up design: accept provider history on Base

**Outcome.** A Base owner can recover from any history difference between Base and its provider branch without a VM. That covers a provider force-push and a Make Remote over existing Base history. One confirmed action adopts the provider repository and discards Base's local history for that repository.

**Design.**
1. **Owner operation.** A Base-only owner route, `POST /workspaces/:id/repos/:repoId/accept_provider_history` (previewed by `POST .../provider_history_preview`), takes the provider tip and Base head the owner reviewed. Remote integration owns the operation:
   - It verifies that the workspace is Base, the caller is its current owner, the repository is remote-tracked, and Base's history differs from the provider's. That means paused for `watermark_violation` or `diverged`, or a fresh classification that is not a fast-forward.
   - It fetches the exact provider tip with the owner's credentials. A moved tip or head is refused.
2. **One landing.** Remote integration's publication boundary rewrites Base's branch to the provider tip and moves the provider observation and watermark in the same cut, through the existing fenced landing. It also:
   - clears the pause;
   - closes active remote-tracking runs for that repository as resolved;
   - records an integration run that names the old and new heads;
   - writes a pin ref for the replaced head (for example `refs/g5/base-accepted/<run-id>`).

   It pushes nothing. Published projections follow the landing as for any import.
3. **Make Remote on Base.** Remove the `BASE_REMOTE_BIND_REQUIRES_FAST_FORWARD` refusal. A non-fast-forward Base binds paused, as it does for any workspace, and the popover offers acceptance. A Base that can fast-forward binds normally.
4. **Frontend.** On Base, the paused notice becomes an **Accept provider history** action with a confirmation. The confirmation names the repository and provider tip, states that Base's local commits for that repository will be discarded, and says nothing is pushed. It replaces the hidden rebased-candidate approval and Resolve with agent on Base.

**Acceptance outcomes.**
- After a provider force-push, a paused Base is reset to the new tip on acceptance, with head, provider observation and watermark equal. The next cycle is healthy, and the old head stays reachable through the pin.
- Make Remote on a Base that is ahead or diverged binds it paused. Acceptance then adopts the provider tip and Base reads healthy.
- A stale reviewed tip or head is refused with nothing changed, and so is acceptance on a non-Base workspace or a Base that already matches the provider.
- No provider push occurs on any path.

**Open assumptions, resolved during coding.**
- The fenced branch-commit landing is an exact old-OID CAS. Transition kinds are labels, so a reset lands like the existing rebase publication. Tested on both paths.
- No migration is needed. Acceptance reuses the convergence consumer's `imported` run with `trigger_kind = manual`, which records the old and new heads.
- The admission closes open remote-tracking `job_merge_runs` for the repository as `completed` with `blocking_reason = accepted_provider_history`, inside the landing transaction.
- The pin is `refs/g5/base-accepted/<advance-id>`. Repo GC only runs `git gc`, which keeps everything reachable from a ref.

**Risks.** Acceptance discards Base-local commits for that repository. The pin keeps them recoverable for operators, but there is no UI to restore them.

## Follow-up: Base push invariant and co-landed imports

**Request.** Three follow-ups from a review of this branch ([intent](chat_intent/retire-protected-ref-mirror/request.md#base-push-invariant-and-co-landed-imports)). Items 1 and 2 change committed design, so the ontology is amended under the amendment rule before dependent code.

**Ontology amendments.**
- `g5.workspace.remote.base-parity`: parity is pull-side only, and Base never publishes on any path. Base's push mode is always manual. Push to Remote and the auto-push control are refused or absent, every push executor refuses Base, and a Base that is ahead of or diverged from its provider pauses without a resolution run. A provider reset to an older commit Base already holds is named as a third way histories differ. The H014 citation for per-repository import is withdrawn: H014 decides individual repository pushes and no atomic multi-repository cut, so it allows opportunistic co-landing.
- `g5.workspace.remote.base-auto-pull-default`: "the only difference" now reads "on the pull side, the only difference".
- `g5.governance.remote.pull`: adds co-landed imports, the merged-PR hold, and validation of every claimed lease.
- `docs/data-model-cleanup-plan.md` provider transport: co-landing, the merged-PR hold and its gap, prompt retry after a refused landing, and Base's pull-only integration.

### 1. Base never pushes

**Decision: force manual.** Base's push mode is forced to manual rather than ignored, so stored policy and the UI never claim auto-push:
- migration 056 (unmerged) sets existing Bases to `manual`;
- Base creation inserts `manual`;
- setting `auto` on Base is refused.

The safety guarantee does not depend on the mode. It has three layers:
1. **Classifier.** `ClassifyInput.publication_allowed` is false for Base. `WorkspaceAhead` and `Diverged` then map to `DivergedPauseOnly` for every trigger and mode, so Base never yields `Pushed`. A Manual or Push trigger that resumes a pause-only state re-pauses it.
2. **Executors.** `push_workspace_tip` and `prepare_approved_rebase_publish` refuse Base.
3. **Enqueuers.** The owner-requested push enqueues in `merge.rs` skip Base. `push_to_remote` refuses Base with `COORDINATED_PUSH_REQUIRES_NON_BASE`.

The UI hides Push and auto-push on Base. Acceptance's preflight already treats an ahead Base as differing history.

**Tests** (real Postgres and Git):
- lagging watermark, then a provider reset to an older descendant, in auto mode on a background trigger: no push, and Base pauses;
- Make Remote on an ahead Base, then a Manual Pull after its run ends: no push;
- `push_to_remote` on Base is refused;
- a non-Base workspace in auto mode still pushes.

There is also a classifier unit test across every trigger, mode and relation.

### 2. Land due imports together

When a primary cycle classifies `Imported`, the worker opens a short transaction. In it, it selects the workspace's other members that are bound, unpaused and eligible:
- auto-pull is on and the merged-PR hold does not apply; or
- the member has its own queued `manual` row.

For each selected member, the worker tries the remote-pair lease without waiting (the existing insert or expired takeover) under the primary's owner token. A busy lease skips that member. Outside any transaction, the worker resolves each claimed member's credentials and fetches. It keeps only strict fast-forwards (`MirrorAhead`). An auth failure, fetch failure or non-fast-forward is released and left for that member's own cycle, which records the outcome.

`prepare_pull_cut` takes every proposed head and one convergence input per member. `PublicationClaim` gains `co_members`. Each co-member carries its key, policy, remote branch and lease generation, and the landing admission validates each lease and policy under the fence. Renewal covers every co-lease. Cleanup releases them all. Co-member queue rows are left alone: their own cycle then finds `Equal` and records a no-op.

**Prompt retry.** A landing refused by a cut mismatch, a stale claim or a policy change no longer waits for lease expiry. The worker releases every lease, defers the queue row by `BRANCH_LOCK_DEFER_SECS`, and retries with fresh state. Two concurrent cycles in one workspace therefore converge in the next sweep.

**Merged-PR hold.** An automatic import (`auto_pull_enabled` without a Manual or Push trigger) is held for a repository when the workspace is the target of an open workspace PR whose provider row for that repository has a provider PR, while a sibling provider row is not merged, and either:
- the row is `merged`; or
- the PR row is locked by an in-flight provider merge (`FOR SHARE SKIP LOCKED`).

A held cycle classifies as auto-pull off: it records `fetched` and imports nothing. The landing admission rechecks the hold for automatic triggers.

**Recorded gap.** A provider PR merged directly on GitHub is recognized only after G5 records it as merged.

**Tests:**
- two members advance, and one cycle lands both in a single landing;
- one member paused and one failing auth, while the other still imports;
- two concurrent cycles converge without lease-expiry delay;
- a partially merged multi-repository PR is not half-imported.

### 3. Acceptance lock order and cleanups

- **Lock order.** Investigate every path that locks the `workspaces` row together with the fence or the inventory rows. Record the conclusion under Open assumptions, and fix any inversion.
- **Doc comment.** Move `ProviderHistoryPreview` out of `PublishBranchRepoResult`'s doc comment.
- **`remote_sync_runs`.** Check whether rows awaiting review need closing at acceptance.

### Open assumptions (follow-up), resolved during coding
- **H014.** It does not forbid co-landing; confirmed above.
- **Lock order.** The acceptance admission adds no lock order of its own:
  - inventory resolution under the fence already holds the Base `workspaces` row for update, so the admission's ownership read now takes no lock;
  - the landing order is fence → workspace row → mappings → `remote_integrations` → `job_merge_runs` → `repos`.

  Two inversions affected every landing, acceptance included, and are fixed:
  - **Clone-merge.** It took the parent's row lock (the owner check) before the fence; it now takes the fence first.
  - **Inventory.** It locked mappings before the workspace row, the reverse of the policy `load(.., true)` order (`w, m, r, i`) used by the auto-pull toggle, retry and the advisory pinger; it now locks `workspace, mapping`.

  Source-order unit tests pin both. One pre-existing cycle remains and is out of scope: fork admission or finalize, and a source rebase onto Base, lock `repos` and then take a foreign-key or share lock on the Base row. Postgres detects such a cycle (40P01) and aborts one side, and the worker retries.
- **`remote_sync_runs` review state.** Nothing to close. `remote_sync_runs` is an append-only audit with no open or awaiting state. Review state lives in `job_merge_runs` and the pause, both of which acceptance closes, and acceptance appends its own `imported` row.
- **Co-member trigger.** A co-landed member's run records `manual` when its own queued manual pull made it eligible, and otherwise the primary cycle's trigger.
- **Refused landings.** The previous landing error propagated out of the org drain and left the row locked for the 600-second lease. It now releases every lease and defers the row for 2 seconds when the landing is refused (`AuthorityCutStale`, `Conflict` or `GitError`). Other errors keep lease-expiry retry, but co-member leases are released.
- **Test barrier.** The clone-merge test barrier now also pauses worker landings (`install_prepared_landing_barrier`), so a refused landing can be forced deterministically.
- **Fork push mode.** Forks copy their source's push mode, so a fork of Base starts in `auto`, the new-workspace default. During the rolling upgrade, an old pod forking a Base already migrated to manual gives the fork manual mode; the owner can switch it.

## Follow-up: consistent fork capture of a multi-repository cut

**Request.** A fork must freeze either both old heads or both new heads of a multi-repository landing, never a mix, for every source workspace ([intent](chat_intent/retire-protected-ref-mirror/request.md#consistent-fork-capture-of-a-multi-repository-workspace)). There were two holes:
- fork admission read source refs without the commit fence;
- a landing refused at COMMIT restored its refs after the fence was released, so a reader could capture heads that later disappeared.

**Ontology amendments.**
- New `g5.fork.source.cut`: a fork freezes one committed source cut.
- New `g5.git.landing.commit-point`: Git acceptance under the fence is a landing's commit point.

### Excluding rolled-back heads: option (c)

- **(a) is unavailable.** Migration 039 dropped `workspace_repos.head_sha_cache`, no landing writes a head column, and `g5.data.git-authority` forbids a second SQL head registry.
- **(b) is costly.** `ScopedDb::commit` consumes the sqlx transaction and returns its pooled connection, so a session-level fence cannot be released on the same connection after COMMIT. Holding it on a second pooled connection per landing risks starving the pool. Changing ScopedDb to own a raw connection is an invasive change to the authorization boundary.
- **(c) chosen: no retraction after the fence.**
  - Refs moved under the fence are already kept when a request is cancelled before COMMIT, or when the COMMIT outcome is unknown.
  - The post-commit restore was the only path that took refs back after the fence was released. It is removed.
  - In its place, the landing runs `SET CONSTRAINTS ALL IMMEDIATE` before Git moves a ref, so a deferred check refuses the landing with nothing moved.
  - A COMMIT that fails after Git accepted the cut can now only be an infrastructure failure. It leaves the refs ahead of the database, as cancellation does, and the database side converges from the refs.
  - A Git I/O failure partway through `apply_ref_commits` used to leave the written refs in place, a durable torn cut. Those refs are now restored by CAS while the fence is still held.
  - **Effect.** A reader holding the fence sees only complete cuts, and refs never move backwards after it.

### Fenced reads

- **Shared helpers in `workspace_reader.rs`:**
  - `read_committed_branch_heads`: bounded fence, then the refs.
  - `resolve_committed_snapshot_fenced`: the snapshot under the fence.
  - `read_committed_cut`: the fenced snapshot in its own short transaction, rolled back at once.
  - `verify_committed_heads`: rereads a captured vector under the fence, taken with the bounded try-lock in the caller's own transaction and held until it commits. It is for callers that already hold workspace row locks or run slow work.
    - A try-lock never queues behind another lock, so taking it after row locks cannot deadlock. A landing that needs those rows only delays the caller until the bound refuses it.
    - A torn vector cannot still match once the landing that tore it has completed.
    - A first version reread in a separate transaction. Its authorization lock queued behind a concurrent PR close and changed that request's outcome, so it was dropped.
- **Busy errors.** A busy fence refuses with `WORKSPACE_COMMIT_FENCE_BUSY` (`branch_commit::commit_fence_busy`). A changed vector refuses with `WORKSPACE_CUT_CHANGED`.
- **Fork admission.**
  - The read-only capture transaction now ends after its last query.
  - One short fenced transaction holding no row lock then reads every member head. It feeds both the "receipt is current" check and the frozen vector.
  - The fence is released before `lock_repo_rows`, so this adds no cycle with that lock or with the recorded fork/rebase foreign-key cycle.
  - A busy refusal happens before any `workspace_forks` row exists. Replay of an existing operation precedes admission, so retries stay idempotent.

### Reader inventory

**Changed: the vector is persisted or acted on.**
- **Fork admission:** fenced read.
- **Workspace manifest** (`g5d clone`/`refresh` pin these commits) **and source download:** `read_committed_cut`.
- **Clone-merge child read:** `read_committed_cut`, because the child's heads are applied to the parent and never rechecked against the child. The parent side is rechecked by CAS.
- **VM start** (`record_start_intent`): the bounded fence comes first in its short, database-only transaction, before `workspaces` and `workspace_vms` row locks. That order matches source resolution, which takes the fence and then the owner handle.
- **PR create and refresh evidence:** `JobMergeService::verify_committed_cuts` runs last in the apply transaction, after the PR row lock and the evidence writes, so the fences are that transaction's last locks. Provider PRs are created at these heads.
  - Source and target fences are taken in workspace order.
  - Checking earlier, before the PR row, made an automatic refresh holding the PR row meet a manual refresh holding the fence, and the automatic refresh then failed busy.

**Acceptable: read-only, rechecked, or per member.**
- **Display only:**
  - divergence badges;
  - the changes and history views;
  - change graph;
  - parent comparison bases;
  - repository summaries;
  - job output trees;
  - the durable changes view;
  - the docs and ontology reads.
- **Fed to a fenced landing CAS that rechecks every member:**
  - Make Remote;
  - provider-history acceptance;
  - the clone-merge parent;
  - remote-tracking capture;
  - merged-target import, revalidated under both fences;
  - local merge revalidation, already fenced.
- **Per member, a single member, or repository IDs only:**
  - delete preflight counts;
  - commit lists;
  - preview profiles;
  - comments;
  - linked-doc indexing.
- **Review vectors:**
  - the annotation `sourceHeads`, from `source_heads` for agent findings and comments;
  - `publish_review`'s `base_commit_vector`.

  Each member's evidence is exact. The vector only decides when a round becomes outdated: any differing source head outdates it. A torn vector therefore outdates a round sooner, never marks a stale one current.
- **Diagnostic:**
  - the blocked-merge preflight evidence, which the merge refused and recaptures on retry;
  - `merge_provider_pull_requests`' per-member equality checks, whose import is re-fenced.
- **Already complete vectors:** live fork capture (a flush receipt) and fork finalize (the persisted manifest).

### Decisions brought back (accepted)

The user accepted both recommendations ([intent](chat_intent/retire-protected-ref-mirror/request.md#fork-advisories-and-the-explicit-pull-hold)); see the follow-up design below.

1. **Should forking warn when a source member is held, paused or behind?** Recommendation: a non-blocking advisory on the fork dialog and response, like `co_tracking_workspaces`, naming each member with its reason:
   - held by the merged-PR hold;
   - paused, with the pause reason;
   - behind its provider observation.

   Refusing would block forks of a paused Base, whose only fix is the owner's acceptance.
2. **Should an explicit Pull respect the merged-PR hold, and should closing a partly merged PR lift it?**
   - **Pull and the hold.** Recommendation: an explicit Pull keeps bypassing the hold, but the response and the popover name the half-merged PR and say the pull imports that repository alone. A Pull is the owner's choice, and the hold exists for unattended imports.
   - **Closing the PR.** The hold already lifts on close, because it requires `status = 'open'`. Keep that behavior and state it in `g5.governance.remote.pull`, since a closed PR has no merged-PR import left to wait for.

### Open assumptions (fork capture), resolved during coding

- **Authorization queues requests behind a landing.** Workspace-scoped authorization (`authz.lock_user_scope_authority`) takes a share lock on the `workspaces` row, which conflicts with the landing's inventory lock. In practice a fork requested mid-landing therefore waits in authorization and then reads under the fence. The bounded busy refusal applies when the fence is held without the row lock: the moment between a landing's fence and its inventory lock, or fence holders such as worker finalization and VM start. This is pre-existing and acceptable.
- **`verify_committed_heads` has an ABA limit.** It cannot detect a later landing that restores a member's exact earlier OID, which would require a rewrite back to an identical commit between the capture and the check.
- **Partial Git failure.** If restoring a partially written cut also fails, the written refs stay. That is logged as an error and reported in the landing error. It needs a double Git I/O failure.
- **Lock order unchanged.** Every new fence acquisition precedes row locks in its own transaction or is a separate transaction. Bounded polling never waits in Postgres's lock queue, so it cannot form a deadlock cycle.

## Follow-up: fork advisories and the explicit-Pull hold notice

**Request.** The user accepted both recommendations ([intent](chat_intent/retire-protected-ref-mirror/request.md#fork-advisories-and-the-explicit-pull-hold)).

**Ontology amendments.**
- New `g5.fork.source.advisory`.
- `g5.governance.remote.pull` now says that closing the workspace PR lifts the hold, and that an explicit pull bypasses it with a notice naming the PR.

**Design.**
1. **One hold reader.** `remote_integrations::import_hold(conn, workspace_id, repo_id)` returns the holding workspace PR (id and title), using the existing hold query. `automatic_import_held` becomes `import_hold(..).is_some()`, so the worker, the landing admission and every notice share one definition.
2. **Tracking summary.** `BranchTrackingRepo` gains `import_held_by` (`importHeldBy`). The tracking popover shows a notice naming the PR and saying that Pull imports this repository alone.
3. **Pull response.** Each queued `BranchSyncDetail` gains `held_by` (`heldBy`). The Pull notice names each held repository and its PR.
4. **Fork advisories.** `remote_integrations::fork_source_advisories` lists each remote-tracked source member that is held (with the PR), paused (with the reason), or behind its provider observation (the observed tip strictly descends from the head). It serves both:
   - a read-scoped `GET /workspaces/:id/fork_advisories`, which the fork dialog shows before submitting;
   - `sourceAdvisories` on the clone response, computed in the route before forking.

   Neither ever refuses.

**Tests.**
- Backend integration:
  - the advisory lists held, paused and behind members and nothing for a healthy member;
  - the tracking summary and the Pull response name the holding PR;
  - the clone response carries the advisories.
- Frontend unit:
  - the dialog renders the advisories without blocking submit;
  - the popover shows the hold;
  - the Pull notice names the PR.
