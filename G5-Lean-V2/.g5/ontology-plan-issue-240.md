# Issue 240: inline remote-tracking status on Workspace Details

## Ontology changes and governance

**Comparison base:** `origin/main` at `67d2dbcc`.

**Ontology changes:** Add story `g5.story.see-remote-tracking-state` and decisions `g5.workspace.remote-ui.status`, `.detail`, `.unknown` and `.actions` to [workspaces](ontology/features/workspaces.g5.md), with their metadata, and index the story in [core user stories](ontology/concepts/core_user_stories.g5.md). No existing commitment changes meaning. Lean's ontology had no owner for remote-tracking presentation, so these are new blocks rather than edits. The actual ontology diff at the coding handoff is:

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index aedd8c02..ee7d4bea 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -44,6 +44,7 @@ These links add no separate commitments or implementation percentages.
 - [Choose whether to accept work loss](../features/workspaces.g5.md#choose-whether-to-accept-work-loss).
 - [Resume after receiving ownership](../features/workspaces.g5.md#resume-after-receiving-ownership).
 - [Reboot onto the selected image](../features/workspaces.g5.md#reboot-onto-the-selected-image).
+- [See each repository's remote-tracking state](../features/workspaces.g5.md#see-each-repositorys-remote-tracking-state).
 
 ## Files, documents and comments
 
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index e179dbda..8a024cb2 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -146,6 +146,16 @@ Governing concepts: [Live owner authority](../concepts/security.g5.md#one-eligib
 
 Governing contracts: [reboot through explicit stop then start](../software_architecture/lifecycle.g5.md#reboot-through-explicit-stop-then-start).
 
+### See each repository's remote-tracking state
+
+[I · g5.story.see-remote-tracking-state]
+
+As someone viewing a workspace, I can read each member repository's remote-tracking state in words on Workspace Details and expand its detail and actions in place, without any of it clipping off the page.
+
+Governing concepts: Mutating actions use [current-owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute); nonowners [read durable state](../concepts/security.g5.md#serve-nonowners-from-durable-state).
+
+Governing contracts: [status in words](#name-each-repositorys-tracking-state-in-words), [inline detail](#disclose-tracking-detail-inline), [unknown state](#show-unknown-tracking-state-as-unknown), [actions once](#render-each-tracking-action-once).
+
 ## Behavioral contracts and design
 
 <a id="g5.story.work-across-repositories"></a>
@@ -349,6 +359,36 @@ Reserve workspace names case-insensitively across the org, even with disjoint me
 
 This uniqueness rule governs [name admission](workspaces.g5.md#checked-fork-owns-branch-creation).
 
+### Name each repository's tracking state in words
+
+[D · g5.workspace.remote-ui.status]
+
+On Workspace Details, each remote-tracked repository card shows an always-visible status pill: a colored dot and a full-text label for its highest-priority state — review required, blocked (naming the pause reason), behind, ahead, then healthy. Ahead and behind stay binary, without commit counts. A repository without a remote binding reads as local only. Color is never the only signal, and the pill uses design tokens so it reads correctly in light and dark themes.
+
+Tracking states, pause reasons and their actions belong to retained [remote integration](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); these presentation blocks change none of them.
+
+### Disclose tracking detail inline
+
+[D · g5.workspace.remote-ui.detail]
+
+Each card discloses its tracking detail — status banners, merge-run cards with their commits and resolution actions, Push/Pull and auto-pull/auto-push controls, retry and recent activity — in normal page flow behind an accessible expander, collapsed by default. Expanding grows the page, which scrolls; the detail never floats over the page or clips at the viewport edge. Repositories remain one stacked card each at any count.
+
+A floating popover anchored to a small glyph could render off the bottom of the full-page panel and hid state behind an icon.
+
+### Show unknown tracking state as unknown
+
+[D · g5.workspace.remote-ui.unknown]
+
+While tracking status is loading or unavailable, the card shows an explicit pill saying so and, when unavailable, a retry that refetches it. It shows no expander or mutation until the state needed to derive them is known, and never guesses an action from missing or stale data.
+
+### Render each tracking action once
+
+[D · g5.workspace.remote-ui.actions]
+
+The card head holds only the pill and expander. Each remote-tracking mutation appears once, inside the expanded detail, keeping its existing state preconditions, visibility and feedback; for example, a rewrite-approval checkpoint still exposes only its approval action.
+
+A head-level Push, Pull or Retry would duplicate the same workspace action shown in the expanded detail.
+
 ## Metadata
 
 ### g5.story.fork-to-make-my-own-changes
@@ -435,6 +475,12 @@ This uniqueness rule governs [name admission](workspaces.g5.md#checked-fork-owns
 - From [current VM API](../../../docs/workspace-vm-api-v9.md) lifecycle amendment.
 - ~90% · workspace_vm_service.rs, useWorkspace.ts; Stop/Resume composition replaces Restart; full host/runtime reboot behavior was not executed.
 
+### g5.story.see-remote-tracking-state
+
+- human-verified
+- [Issue 240](../../chat_intent/issue-240/request.md) ports the G5 branch's Workspace Details Repositories redesign.
+- Unassessed · not yet implemented; WorkspaceDetailsPanel.tsx still opens a portaled BranchTrackingPopover from the BranchTrackingIcon glyph.
+
 ### g5.workspace.identity
 
 - human-verified
@@ -590,3 +636,27 @@ This uniqueness rule governs [name admission](workspaces.g5.md#checked-fork-owns
 - agent
 - [C1 rationale](../../uplift/g5-lean/investigations/workspaces.md#reverse-discovered-consequential-decisions).
 - ~90% · workspace_fork_service and serialized name constraint.
+
+### g5.workspace.remote-ui.status
+
+- human-verified
+- [Issue 240](../../chat_intent/issue-240/request.md); the source redesign records the user's choice to keep ahead/behind binary.
+- Unassessed · not yet implemented; the glyph in BranchTrackingIcon.tsx carries state today.
+
+### g5.workspace.remote-ui.detail
+
+- human-verified
+- [Issue 240](../../chat_intent/issue-240/request.md); the source redesign records the user's choices of collapsed-by-default detail and stacked cards.
+- Unassessed · not yet implemented; BranchTrackingPopover.tsx renders as a portaled floating popover.
+
+### g5.workspace.remote-ui.unknown
+
+- human-verified
+- [Issue 240](../../chat_intent/issue-240/request.md) adopts the source branch's final loading/error rule.
+- Unassessed · not yet implemented.
+
+### g5.workspace.remote-ui.actions
+
+- human-verified
+- [Issue 240](../../chat_intent/issue-240/request.md) adopts the source branch's final removal of head-level repository actions.
+- Unassessed · not yet implemented.
```

**Review amendments:** The diff above is the original coding handoff. The [implementation review](ontology-review-issue-240.md) led to two agent amendments within the requested intent, and all five footer entries were reassessed as implemented:

```diff
-While tracking status is loading or unavailable, the card shows an explicit pill saying so and, when unavailable, a retry that refetches it. …
+While tracking status is loading, unavailable or not yet reported for a repository the workspace marks remote-tracked, the card shows an explicit pill saying so and, when unavailable, a retry that refetches it. …
```

```diff
 The card head holds only the pill and expander. Each remote-tracking mutation appears once, inside the expanded detail, keeping its existing state preconditions, visibility and feedback; for example, a rewrite-approval checkpoint still exposes only its approval action.
+The detail stays open after an action so that feedback remains visible; when the acted-on control disappears, keyboard focus returns to the card's expander.
```

- **Not yet reported (finding 3):** a tracked repository briefly had no feed row (right after Make remote), and the card showed nothing at all. That contradicted the always-visible pill.
- **Stay open and return focus (finding 4):** the floating popover used to close after an action and return focus to its trigger. The inline detail stays open so its feedback remains visible, so focus needed a defined destination.

The original delivery is evaluated against the handoff above; these amendments describe the repaired delivery.

**Relevant and governing ontology** (commitment → obligation → owner/operation → allowed case / counterexample → conclusion or open assumption):

- [`g5.workspace.remote-ui.status`](ontology/features/workspaces.g5.md#name-each-repositorys-tracking-state-in-words) → one always-visible, full-text pill per remote-tracked card, with priority review > blocked > behind > ahead > healthy, binary ahead/behind, "local only" for unbound repositories, and theme tokens → Workspace Details repository row, via a new status pill and one shared state derivation → allowed: warn-toned dot plus "Remote moved — your branch is behind"; counterexamples: a colored dot with no words, or "3 behind" → **open assumption A1**.
- [`g5.workspace.remote-ui.detail`](ontology/features/workspaces.g5.md#disclose-tracking-detail-inline) → the existing tracking detail sits in page flow behind an `aria-expanded` expander, collapsed by default, one stacked card per repository → reuse the existing popover body through an inline variant → allowed: expanding pushes later cards down and the page scrolls; counterexamples: a `document.body` portal, absolute or fixed positioning, closing on outside click or Escape → conclusion.
- [`g5.workspace.remote-ui.unknown`](ontology/features/workspaces.g5.md#show-unknown-tracking-state-as-unknown) → while status is loading, show a "Loading tracking status…" pill; on error, show "Tracking status unavailable" plus a retry that invalidates the query; show no expander or mutation until the state is known → allowed: an error pill with Retry that becomes the normal pill once the refetch succeeds; counterexample: a Pull button rendered from missing branch data → conclusion.
- [`g5.workspace.remote-ui.actions`](ontology/features/workspaces.g5.md#render-each-tracking-action-once) → the card head holds only the pill and expander; each mutation renders once inside the detail and keeps its current preconditions → allowed: Push inside the expanded detail for an unpaused ahead branch; counterexamples: a head-level Push/Pull/Retry, or a generic Push to Remote next to the rewrite-approval card → **open assumption A2**.
- [`g5.security.owner`](ontology/concepts/security.g5.md#one-eligible-owner-uses-live-compute) and [`g5.security.read.durable`](ontology/concepts/security.g5.md#serve-nonowners-from-durable-state) → only the current owner mutates, and nonowners read durable state → the redesign keeps each control's existing visibility and adds no query family → counterexample: the inline body exposes a mutation the floating popover hid from a nonowner → **A2**.
- [`g5.workspace.identity.base`](ontology/features/workspaces.g5.md#keep-base-workspaces-as-protected-readers) and the `g5.workspace.base-ui.*` blocks → Base stays read-only in presentation → the pill may show Base tracking state, but the redesign adds no Base control → conclusion.
- [`g5.design.reuse`](ontology/concepts/system_design_principles.g5.md#reuse-established-infrastructure), [`g5.architecture.policy`](ontology/concepts/system_design_principles.g5.md#share-policy-without-erasing-legitimate-exceptions) and [`g5.design.minimal-mechanism`](ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) → reuse the popover body, hooks and mutations, and extract one shared status derivation instead of keeping parallel rules in the icon and popover; no backend, API or new hook family → conclusion.
- Retained remote integration ([provider transport and publication](../docs/data-model-cleanup-plan.md#provider-transport-and-publication)) → tracking states, pause reasons, **Resolve with agent** on auto-push divergence and the provider-rewrite approval checkpoint stay as they are → presentation only → conclusion; **open assumption A1** covers the mapping onto pill classes.

**Open assumptions to settle by inspecting source during coding:**

- **A1 — state mapping.** Lean's current `BranchTrackingIcon` state derivation maps onto the five pill classes plus "local only". Lean has diverged from G5 (for example, fetched-but-not-imported observations and Resolve with agent). Any Lean-specific state gets its own accurate label within the nearest priority class and is never dropped. If a state does not fit the priority order, amend `g5.workspace.remote-ui.status` first.
- **A2 — action visibility.** The inline body keeps exactly the controls, preconditions and owner/Base visibility the floating popover has today. Inspect `BranchTrackingPopover` and `RemoteTrackingActions` gating before reusing them.
- **A3 — single consumer.** `WorkspaceDetailsPanel` is the only consumer of `BranchTrackingIcon`, and `ReposBar` does not render tracking state. If that holds, retire the icon and its test and repoint the popover tests at the inline body.
- **A4 — rewrite copy.** If the rewrite-approval card attributes a provider rewrite to "this workspace", correct the wording to name the remote. This is a bounded copy fix within existing intent, carried over from the source branch's UI.
- **A5 — tokens.** Use Lean's existing semantic design tokens for ok/info/warn/blocked/muted tones. If Lean lacks one, add it through the existing token system rather than using raw palette classes.

## Requested outcome and scope

The user asked to port the G5 branch `g5/feat/ben_force_pull-deaddc18`'s "better UI for remote branch management" into Lean and chose **UI redesign only** ([request](chat_intent/issue-240/request.md)).

**In scope:** the Workspace Details Repositories card presentation, which covers the status pill, inline disclosure, unknown-state handling, a single set of actions, theme tokens and retiring the glyph.

**Out of scope:** the source branch's backend direction-typed watermark (`remote_rebased` inbound adoption and the `ambiguous_rewrite` run), its "Adopt remote / Keep local" chooser, and any API, schema or worker change.

## Design

Port the final state of the source branch (G5 `center-panel#D77`), adapted to Lean's current components rather than copied:

1. **Shared status derivation.** Put one pure function beside the tracking components. It maps a repository's branch data, sync runs and active merge runs to `{kind, label, tone}` using the existing icon's rules. The pill and any popover banner that shows the same priority both consume it.
2. **`RemoteStatusPill`.** Renders a dot, the full-text label and a tone token, with `data-testid="repo-status-pill"`. Loading and error use the same component with explicit labels; the error variant gets an adjacent Retry.
3. **`BranchTrackingInline`.** Renders the pill plus an expander button (`aria-expanded`, `aria-controls`, a per-repository accessible name, `data-testid="repo-tracking-expander"`). When expanded, it mounts the `BranchTrackingPopover` body in page flow. Because the body now has no other consumer (A3), its floating chrome and close-on-action callback were removed outright rather than kept as a second variant. All existing inner test ids and hooks stay.
4. **`WorkspaceDetailsPanel`.** Replace the glyph with `BranchTrackingInline` for remote-tracked rows. Unbound rows show the "local only" pill beside the existing Make remote affordance. Retire `BranchTrackingIcon` per A3.
5. **Tokens.** Convert raw palette classes in the popover body, `RemoteTrackingActions`, `BranchAutoPullControl` and `RemoteConflictAgentLauncher` to semantic tokens (A5). Formerly slate or rose filled buttons use `--g5-text0` fill with `--g5-bg0` text, the only filled pairing the token contrast guard keeps at AA in every palette.

Suggested labels, which the implementation may refine to match Lean's states: "Tracking healthy"; "Ahead — local commits to push"; "Remote moved — your branch is behind"; "Review required"; the pause-reason wording for blocked (for example "Paused — merge required", "Authentication required", "Push failed", "Fetch failed"); and "Local only — not published".

**Why:** the full-page details panel made the anchored floating popover clip off-screen, and a small glyph hides state behind decoding. Reusing the existing body keeps behavior and tests stable while changing only the presentation.

## Acceptance outcomes

- Each remote-tracked repository card shows a text pill whose state matches the prior icon's derivation for every state fixture, and unbound repositories read "Local only — not published".
- Detail is collapsed on load; the expander toggles `aria-expanded` and reveals the same merge cards, SHA rows, actions and recent activity, identified by the existing test ids, in page flow; nothing is portaled to `document.body`.
- Loading and error show explicit pills. Error offers Retry, and neither state renders an expander or mutation.
- The card head has no Push, Pull or Retry. Each action appears once inside the detail. The rewrite-approval state still exposes only the approval action.
- No raw palette classes remain in the pill, the inline wrapper, the popover body or `RemoteTrackingActions`, and the surface is legible in light and dark themes.
- Frontend `tsc`, `eslint` and the affected `vitest` suites pass: new suites for the derivation, pill and inline wrapper, plus the repointed popover and `WorkspaceDetailsPanel` suites. Affected E2E selectors, such as the `data-wiring` chip whitelist, are updated.

## Risks

- **Divergence from G5.** Lean's popover (~975 lines) and icon have Lean-specific states and actions, so a straight file copy would regress them. Port by adapting.
- **Test churn.** Tests that open the popover through the icon must be repointed at the inline body without weakening their assertions.
- **Ontology gap.** Remote-integration behavior (states, pauses, approval checkpoint) still has no ontology owner in Lean and is governed by the cleanup-plan document. This change does not uplift it; a later uplift should.

## Unresolved questions

A1–A3 and A5 were confirmed in source; A4 did not apply.

**Transient poll failure (review finding 12).** Both feeds poll every 15 seconds. When one background poll fails over data that is already cached, the card switches to "Tracking status unavailable" and unmounts an open detail, which discards a half-finished confirm step or push notice until the next successful poll.
- This matches the previous popover's error behavior and the current `g5.workspace.remote-ui.unknown` rule.
- The alternative is to keep the detail over cached data and disable only its mutations until a refresh succeeds.
- This needs a human decision; changing it would amend `.unknown`.
