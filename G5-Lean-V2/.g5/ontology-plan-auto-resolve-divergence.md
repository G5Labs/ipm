# Automatic divergence resolution

## Ontology changes and governance

**Comparison base:** the `ben/issue-240-remote-branch-ui` working tree after the issue-240 review. These edits are purely additive on top of it.

**Ontology changes:** Add story `g5.story.resume-after-divergence` and decisions `g5.workspace.remote.divergence-auto-complete` and `g5.workspace.remote.divergence-auto-publish` to [workspaces](ontology/features/workspaces.g5.md), with their metadata, and index the story in [core user stories](ontology/concepts/core_user_stories.g5.md). No existing commitment changes meaning. The actual diff at handoff is:

```diff
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -157,2 +157,12 @@
 Governing contracts: [status in words](#name-each-repositorys-tracking-state-in-words), [inline detail](#disclose-tracking-detail-inline), [unknown state](#show-unknown-tracking-state-as-unknown), [actions once](#render-each-tracking-action-once).
+
+### Resume tracking after resolving a divergence
+
+[I · g5.story.resume-after-divergence]
+
+As an owner, when my resolved rebase of a diverged branch reaches G5, remote tracking resumes without a separate confirmation.
+
+Governing concepts: Owner pushes use [current-owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute); history follows [Git authority](../concepts/data_model.g5.md#git-owns-repository-content).
+
+Governing contracts: [complete a proven divergence resolution](#complete-a-proven-divergence-resolution), [publish a resolved divergence by push mode](#publish-a-resolved-divergence-by-push-mode).
 
@@ -391,2 +401,18 @@
 
+### Complete a proven divergence resolution
+
+[D · g5.workspace.remote.divergence-auto-complete]
+
+When an accepted update moves a repository that is paused for divergence past the workspace tip its open divergence run captured, G5 runs the run's completion proof without waiting for a human. The proof is the same one manual confirmation uses: the new head descends from the captured provider tip, linearly without merge commits when an agent resolved it. A passing head completes the run and clears the pause. A failing head leaves the run open and the repository paused. Manual confirmation remains available and applies the same proof.
+
+This covers divergence runs only. A provider-rewrite approval checkpoint stays an explicit approval, because resolving divergence never approves a provider rewrite ([provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication)).
+
+### Publish a resolved divergence by push mode
+
+[D · g5.workspace.remote.divergence-auto-publish]
+
+An automatic completion is not a publication request. The resolved tip reaches the provider only through the member's existing push policy: auto-push publishes it as the next fast-forward, and manual push mode keeps it in G5 until the owner pushes. A manual confirmation keeps its current meaning as an explicit publication request by the confirming owner.
+
+Implicit publication follows [provider transport](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); the actor's authority follows [publication under the acting human's provider authority](governance.g5.md#publish-under-the-acting-humans-provider-authority).
+
 ## Metadata
@@ -482,2 +508,8 @@
 - Implemented · WorkspaceDetailsPanel.tsx renders BranchTrackingInline or a local-only RemoteStatusPill under each repo chip; unit-tested. Browser check covered only local-only cards; tracked cards, in-flow scrolling and theme contrast were not browser-verified.
+
+### g5.story.resume-after-divergence
+
+- human-verified
+- [Automatic divergence resolution](../../chat_intent/issue-240/auto-resolve-divergence.md) asks G5 to discover a resolved force-push instead of requiring a click.
+- Unassessed · not yet implemented; completion currently requires `POST …/merge_runs/:id/complete` from the Mark resolved control.
 
@@ -662 +694,13 @@
 - Implemented · the card head holds only the pill and expander; mutations render once in BranchTrackingPopover.tsx, whose close-on-action callback was removed; BranchTrackingInline.tsx restores focus to the expander. Unit-tested. The client does not gate these controls for nonowners (unchanged); server enforcement was not reviewed.
+
+### g5.workspace.remote.divergence-auto-complete
+
+- human-verified
+- [Automatic divergence resolution](../../chat_intent/issue-240/auto-resolve-divergence.md); the proof reuses the existing completion check described in the cleanup plan's reconciliation section.
+- Unassessed · not yet implemented; `remote_integrations/merge.rs::prepare_external_remote_tracking_merge_completion` holds the proof, reachable only from the manual complete route.
+
+### g5.workspace.remote.divergence-auto-publish
+
+- agent
+- Design decision for [automatic divergence resolution](../../chat_intent/issue-240/auto-resolve-divergence.md): an automatic path must not bypass the auto-push toggle that governs implicit publication.
+- Unassessed · not yet implemented; manual completion enqueues a `push` integration under the confirming owner's push authority.
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -47,2 +47,3 @@
 - [See each repository's remote-tracking state](../features/workspaces.g5.md#see-each-repositorys-remote-tracking-state).
+- [Resume tracking after resolving a divergence](../features/workspaces.g5.md#resume-tracking-after-resolving-a-divergence).
 
```

**Relevant and governing ontology** (commitment → obligation → owner/operation → allowed case / counterexample → conclusion or open assumption):

- [`g5.workspace.remote.divergence-auto-complete`](ontology/features/workspaces.g5.md#complete-a-proven-divergence-resolution) → run the existing completion proof when an accepted update moves a repository paused for divergence past the captured workspace tip.
  - Owner: the branch integration worker, while processing the wake that the accepted update already queues. It reuses the existing prepare/apply completion.
  - Allowed: the resolution agent rebases onto the captured provider tip without merge commits and pushes with its lease. The run completes and the pause clears.
  - Counterexamples: a head that does not descend from the captured provider tip; an agent resolution containing a merge commit; any `reset_below_baseline` run. All of these stay open.
  - **A1, A2, A4.**
- [`g5.workspace.remote.divergence-auto-publish`](ontology/features/workspaces.g5.md#publish-a-resolved-divergence-by-push-mode) → automatic completion enqueues no `push` upgrade.
  - Under auto-push, the accepted update's own commit wake publishes the fast-forward. Under manual push mode, nothing reaches the provider until the owner pushes.
  - Counterexample: automatic completion enqueuing `push` under manual mode.
  - **A3.**
- [`g5.governance.publish.actor`](ontology/features/governance.g5.md#publish-under-the-acting-humans-provider-authority) → implicit publication uses the member's existing auto-push authority. Automatic completion fabricates no confirming human: `confirmed_by` stays empty and the completion is recorded as automatic. **A2.**
- [`g5.security.owner`](ontology/concepts/security.g5.md#one-eligible-owner-uses-live-compute) and the [Git write boundary](ontology/software_architecture/durability.g5.md#git-network-writes-are-bounded-by-ownership-and-cas) → the triggering update is already an admitted owner or current-VM write. Automatic completion adds no write authority. Conclusion.
- [`g5.architecture.recovery`](ontology/concepts/system_design_principles.g5.md#make-failures-and-retries-explicit) → automatic completion is replay-safe. The existing locked status guard and `AlreadyTerminal` path make a racing manual confirmation and automatic completion produce exactly one completion. Replaying a wake after completion is a no-op. Conclusion; the race needs a test.
- [`g5.design.reuse`](ontology/concepts/system_design_principles.g5.md#reuse-established-infrastructure) → one proof shared by manual and automatic completion; no second ancestry rule. Conclusion.
- [`g5.workspace.remote-ui.actions`](ontology/features/workspaces.g5.md#render-each-tracking-action-once) → Mark resolved stays as a manual trigger of the same proof. The card, pill and resolution link reflect the completed run without a reload. **A5.**
- Retained remote integration ([provider transport](../docs/data-model-cleanup-plan.md#provider-transport-and-publication), [reconciliation in the workspace](../docs/data-model-cleanup-plan.md#reconciliation-in-the-workspace)) → the existing completion check verifies both captured tips are ancestors of the published result. Provider rewrites keep their explicit approval checkpoint. The pause suspends provider fetches until reconciliation; the proven push is that reconciliation. Conclusion.

**Open assumptions to settle by inspecting source during coding:**

- **A1 — wake reaches the worker.** An accepted workspace-branch update queues an integration wake even while the member is paused. The worker must evaluate the proof for a paused-diverged member with an open `job_rebase_preflight` run before its pause skip, rather than dropping the wake.
- **A2 — actorless completion.** `apply_external_remote_tracking_merge_completion` currently needs an acting user, for push-authority capture and `confirmed_by`, and always upgrades the wake to `push`. The automatic path needs a variant with no human actor that records the completion as automatic and skips the upgrade. If no column can record the completion source, add one in a migration.
- **A3 — publication by push mode.** The accepted update's commit wake survives the completion. Under auto-push it publishes the resolved tip as a fast-forward; under manual mode it publishes nothing. Verify against the queue's trigger precedence.
- **A4 — running resolution task.** The resolution task may still be running when its push lands. Completion neither waits for the task nor changes it, matching manual confirmation today.
- **A5 — UI refresh.** Merge-run and sync-run polling or event invalidation removes the completed card and returns the pill to its tracking state without a reload.

**Implementation reconciliation:** the governing blocks did not change. Their footers now record where each is implemented and its verification limits. How the open assumptions resolved:

- **A1 — resolved without depending on wake survival.** `worker.rs::auto_complete_proven_divergence` runs between picking a queue row and claiming it. It selects the member's open `job_rebase_preflight` run only while the member is paused `diverged`. After a completion clears the pause, the claim loads the unpaused state and the same wake runs as an ordinary cycle.
- **A2 — `CompletionRequester::{Owner, Automatic}` replaces the acting-user parameter** of the existing prepare/apply completion.
  - Owner keeps its behavior: push-authority fence, `confirmed_by`, and the `push` upgrade.
  - Automatic skips all three, refuses any phase other than `job_rebase_preflight`, and writes a `resolved` audit row in the same transaction.
  - Migrations `049_remote_sync_resolved_status.sql` (`NOT VALID`) and `050_validate_remote_sync_resolved_status.sql` admit the status; they were renumbered from 043 (and later 046/047) as `origin/main` added migrations. The activity list labels it "Divergence resolved — tracking resumed".
- **A3 — holds.** Manual push mode leaves the provider unchanged. In auto mode, the ordinary cycle attempts the implicit push, which needs the org's GitHub App. Without an App (as in the worker test fixture) that cycle records `push_failed` "Automatic push requires a GitHub App", and the owner's explicit Push publishes. Automatic completion never falls back to the owner's personal connection. Previously the manual click published through that connection.
- **A4 — holds.** Completion neither reads nor changes the resolution task.
- **A5 — covered.** The UI picks up the completed run through existing polling; only the new activity label was added.

**Review repairs** ([review](ontology-review-auto-resolve-divergence.md)). None changes a governing block's meaning:

- **Late or raced owner confirmation.** It replays without error, and, as `divergence-auto-publish` already requires, it still queues that owner's authorized push.
- **Watermark in manual mode.** After an automatic completion, the watermark catches up to the provider tip the workspace contains, so tracking reads ahead rather than behind.
- **Failed apply.** If the proof passes but applying the completion fails, the wake is deferred for a later sweep instead of being consumed.
- **Lock-time recheck.** Automatic completion rechecks pause and phase under the row lock.
- **Audit insert.** It requires exactly one row.
- **CLI waits.** They skip `resolved` rows.

## Requested outcome and scope

The user asked for divergence to resolve automatically, because G5 can see the resolving force-push to the workspace branch ([request](chat_intent/issue-240/auto-resolve-divergence.md)).

**In scope:** automatic completion of open divergence (`job_rebase_preflight`) runs that pass the existing proof, and publication by push mode.

**Out of scope:**
- automatic approval of provider-rewrite (`reset_below_baseline`) checkpoints;
- divergence paused without a run (question Q1);
- any change to the proof itself.

## Design

1. **Detect.** When the branch integration worker takes a wake for a member paused for divergence that has an open divergence run, it runs the existing completion preparation against the current published head, and only if that head differs from the run's captured workspace tip.
2. **Complete.** If the proof passes, apply the completion without a human actor: mark the run completed as automatic, leave `confirmed_by` empty, clear the pause, and emit the usual events. If it fails, leave everything unchanged; the next accepted update tries again.
3. **Publish by policy.** Do not upgrade the wake to `push`. The worker continues the cycle for the accepted update, so auto-push forwards the resolved tip as a fast-forward and manual mode keeps it in G5.
4. **Manual path unchanged.** Mark resolved keeps running the same proof and remains an explicit publication request by the confirming owner.

**Why:** the proof already decides everything a human click adds, and waiting for the click leaves tracking paused after the resolution has landed. Deriving publication from push mode keeps the auto-push toggle as the only switch for implicit publication.

## Acceptance outcomes

Real Postgres/Git worker tests, in the style of `remote_tracking_background_loop.rs` / `remote_tracking_merge_active.rs`:

- An agent-style linear rebase onto the captured provider tip, pushed with its lease, completes the run automatically and clears the pause. With auto-push, the provider branch fast-forwards to the resolved tip. `confirmed_by` is empty and the completion is recorded as automatic.
- The same push in manual push mode completes the run and clears the pause, and the provider branch does not move until the owner pushes.
- A push that does not descend from the captured provider tip, or an agent resolution with a merge commit, leaves the run `waiting_for_human` and the member paused.
- A `reset_below_baseline` run never completes automatically.
- A manual confirmation racing an automatic completion yields exactly one completion and no error visible to the user.
- Frontend: after the automatic completion, the card, pill and resolution link update without a reload (existing polling).

## Risks

- **Early push.** An agent that pushes an intermediate, proof-passing result before finishing its own checks completes the run early. That is the same guarantee as a human clicking after seeing the push, and later pushes proceed as normal updates.
- **Coordinated divergence.** In multi-repo divergence each repository's run completes independently when its own proof passes. No cross-repo cut is added.

## Unresolved questions

- **Q1 — divergence without a run.** Auto-push divergence can pause without opening a resolution run, and the owner may then rebase by hand. Should automatic resume also cover that case, proving against the latest provider observation? Recommendation: a follow-up that reuses the same proof.
- **Q2 — where to land it.** This worktree's branch already carries issue 240 plus two bug fixes (Changes-read coalescing and the resolution-session link). Recommendation: implement on its own branch and PR, stacked on or after issue 240.
