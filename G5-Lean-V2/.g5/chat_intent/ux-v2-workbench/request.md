# UX v2 workbench — request and interpretation

## User instructions (2026-10-06)

- Implement in G5-Lean the UI mocked in `~/UXv2` (`index.html` tab shell + Home, `workspace.html` workbench, `ontology.js` review prototype; stories in `USER-STORIES.md`).
- Use the meeting notes in `G5-Transcript/G5/meetings`, chiefly "UI Discussion — 2026-10-06" (Gemini notes), as the source of decisions.
- Deliver an implementation plan with phases.

## Decisions taken in the 2026-10-06 UI discussion (aligned)

- Workspace work runs in three phases: Business Engineering, System Engineering, Integration Engineering, shown as three tabs across the top. BE = alignment on business objectives and functional requirements (requirement doc + mock); SE = locally tested prototype; IE = pre-merge planning, rebase and merge. IE stops at merge; no production ownership.
- The project timeline replaces the current overview page as the landing page of a workspace, with the workspace's status (phase, gate) and its primary issue/description at the top.
- Ribbon buttons are prepackaged chat templates ("the chat as a predefined prompt"), not bespoke features; which IE buttons become templates is still to be decided.
- V1 keeps only Create PR, Rebase and Merge from Integration Engineering; the rest of IE is deferred.
- Chat personas (a different system prompt / agent markdown per phase) are in V1; the business persona is needed for the Jada persona. Zhengchun owns the persona prompt.
- Homepage chat is deferred to V2.
- V1 relies on the existing external issue tracker integration; no local issues.
- Review meeting: upload a transcript, extract decisions, then Accept / Delay (to-do) / Out of scope (issue outside the workspace) / Reject each; V1 may start with upload + chat and grow a UI frame.
- Build Mock is a templated chat; Preview Mock is the ordinary Preview with the mock wired in, fast to start; grey it out when no mock exists.
- Davide pushes a non-mock implementation to the platform by Friday 2026-10-09.

## Later user instructions (2026-10-06, same day)

- Approvals are associated with phases instead of stages; breaking stage-based approvals is fine, they will not be supported.
- Remove the "number of stages" (Governance model) selection when forking a workspace.
- Use a new flag, `G5_ALIGNMENT_UI_ENABLED`, and a Settings option under "Workspace panels" to switch to the alignment UI.
- Have Codex review the implementation plan (advisory loop to convergence).

## Later user instructions (2026-10-07, Phase 4)

- After the Phase 4 requirement count (7 from the meeting, 1 from the mock, 11 created by the
  agent and the review loop): "if we want to do something like this
  https://github.com/G5Labs/G5/pull/2532, does anything changes? Can we have a common format for
  question answering and meeting decisions?" The agent answered that a meeting decision is a
  question with a proposed answer and recommended Phase 4a (question store) before Phase 4b (the
  review meeting as its first consumer). The question store was then designed and merged on its
  own (`.g5/ontology-plan-question-store.md`, PRs #409, #427, #442).

## Later user instructions (2026-10-08)

- Implement the tab shell and organization fork picker from the mock as a new PR; every
  other phase of the plan is merged. (Asked after confirming that no planned phase
  introduced the mock's top tab bar for switching between workspaces.)

## Later user instructions (2026-10-09)

After comparing the plan with the mock session's 56 prompts (`session-prompts.json`) and its
change log (`ux-changes.jsonl`), fix three divergences in one PR:

- Left rail (prompts #24, #26; change log W08): Activity at the very top, opening in the work
  area; one "Issues & tasks" item ("we don't distinguish between issues and task"); a Jobs item.
  The plan's Phase 0 order (project docs, ontology, files, tasks, issues) contradicted it.
- Integration Engineering ribbon (prompt #26; change log W03): "New doc and import and analyze
  should be in each ribbon", with Review meeting in the same group. The plan's V1 set gave IE
  only Approval, Create PR and rebase, Merge to parent.
- Workspace Issues & tasks panel (prompts #26, #29, #30, #52; change log W10, W12): a flat list
  under the primary issue, quick-add line at the bottom, "+ Import task" picker over the
  backlog, mark done from the sidebar, and an Action ▾ on the issue page (Implement in this
  workspace, Fork in own workspace, Mark as done / Reopen, Detach from workspace, Import task)
  plus Request approval. The plan had neither built it nor labelled it V2.
- Same day, after the Phase 8 draft: move the title bar (`data-surface="titlebar"`: sidebar
  toggle, G5, workspace switcher, Activity, primary issue chip, avatars, bell, settings, account)
  and merge it with the workspace toolbar below it (`workspace-toolbar`: VM status and controls)
  "so we don't occupy too much vertical space" (mock prompt #49 asked the same of the mock).

Later the same day, on finding C5 of that comparison (prompt #26: "The activity log of the
primary one is the same as the activity log for the workspace. When clicking on activity log it
essentially shows the primary issue page"; change log W13: tracker comments and events
interleaved with labelled workspace context plus a composer, where the plan's Phase 1 text
described a feed that "references, never duplicates, issue discussion"):

- "work on C5 too. Let's keep all the activities there (both the changes, issues, comments,
  etc..) with checkboxes on top on what to show".

Later the same day, after web research on sidebar toggle placement and tree-row alignment:

- Move the sidebar collapse control out of the title bar onto the rail's own top row: a `<<`
  as the Activity row's trailing action, a `>>` at the collapsed edge; keep Activity's icon
  aligned with the section rows by reserving the chevron's slot. "No need to add keyboard
  shortcuts."

## Later user instructions (2026-10-11)

After reviewing an interactive mock (`docs/mocks/titlebar-removal/`, variants A and B):

- Remove the whole workspace title bar (`data-surface="titlebar"`) under the alignment layout.
- The workspace switcher dropdown and the Fork button disappear from the workspace page;
  searching and forking go on the Home page and in the "+" tab, "and the functionalities are
  preserved there".
- The Activity button disappears (the rail already has Activity). The primary issue chip and
  the online-collaborator avatars are not needed.
- Owner + Transfer become one control: a user-with-key icon with a dropdown that says
  "transfer ownership" and lists the people to click.
- "VM: stopped" + Start become one button that says Start or Stop by the VM status.
- The moved controls use the ribbon's own format: big icon with the text below. The Terminal
  button is dropped (too many icons).
- Variant B is implemented: the former right-hand icons (Workers, Notifications, bug report,
  Settings, Account) go on the very top, at the right end of the workspace tab strip; ownership,
  VM and Workspace details stay in the phase/action bar.

## Later user instructions (2026-10-12)

- "Now that PR #409, #427 and #442 have merged, we can start to implement the phase 4 of the
  plan", in the simplified form discussed on 2026-10-07.

## Agent interpretation

- Scope V1 to what the meeting kept and what the current backend supports with small additions; everything else from the mock is a labelled V2 backlog.
- Approval definitions bind to zero or more phases (`phases ⊆ {be,se,ie}`; empty = attached but inert, as today), decisions settle one phase, `workspaces.stages` is dropped; migration maps planning→be, finalizing→ie. Merge admission = IE requirements satisfied; BE/SE approvals are advisory sign-offs and never block a phase move in V1. (Superseded: the first draft mapped phases onto the two existing gates.)
- Phase is explicit, user-moved workspace state stored as a platform record, not derived from gates or Git.
- Templated actions and personas are workspace Git content with shipped defaults, so users can change the menus as the meeting anticipated.
- Davide (2026-10-06): do not reuse `g5_ui_v3` (already on in dev/test/CI/QA). The flag is `G5_ALIGNMENT_UI_ENABLED`, and Settings gets an option under "Workspace panels" to switch to the alignment UI. Interpretation: the flag advertises an `alignment_ui` instance feature that offers a fourth `alignment` value in the existing `workspaceLayout` select; the current UI stays the default.
- Davide (2026-10-09): rail, IE shared group and the workspace Issues & tasks panel are Phase 8.
  Interpretation: the rail entries are presentation over existing surfaces (Activity opens the
  timeline tab; Jobs is the existing Task Queue renamed under the workbench preset; Issues & tasks
  renders the workspace's external issue links, primary first, related links presented as
  subtasks of this workspace without claiming tracker hierarchy). Import task links existing
  tracker issues as related through the existing link route. Implement, Fork, Detach and Request
  approval reuse the persona start, the fork dialog, unlink and the approvals tab. Done / Reopen
  and quick-add would write to the tracker; the external-issues contracts keep completion Manual
  until an administrator maps it and route every integration write through one broker, neither of
  which exists, so both controls stay visible but disabled with that reason until the broker lands
  (V2 backlog, now named).
- Davide (2026-10-09, rail toggle): the control sits on the panel it controls, as the
  research's product cases converge on; the chevron column stays reserved for section state,
  so the collapse control is a trailing header action like the existing "+" rows, and a leaf
  row keeps the expander's slot (Fluent, React Aria, Primer tree guidance). Under alignment
  the title bar renders no sidebar toggle; classic layouts are unchanged.
- Davide (2026-10-09): one chrome row. Interpretation: under the alignment layout the
  `TopBar` takes the toolbar content inline (a `toolbar` slot) and the page renders no second
  chrome row; the classic layouts keep both rows unchanged.
- Davide (2026-10-09, C5): the timeline keeps every activity kind — issue discussion
  (opened, comments, events), conversations, jobs and tasks, commits, approvals, phase changes,
  pull requests — and gains a row of checkboxes above the feed choosing which groups to show
  (Phase 9). Interpretation: the data side was already in place (the Phase 1 feed interleaves
  the primary issue's live discussion with the workspace context and carries the composer), so
  the plan's wording is corrected and the filter row is added; filtering is client-side over
  the loaded pages because the activity API has no kind filter, hidden kinds are never
  re-queried, and the selection is per-person browser state like the open-tab list — not a
  platform record, not a setting.
- Davide (2026-10-08): the tab shell is pulled back from the V2 backlog as Phase 7. Interpretation: tabs are navigation chrome gated by the alignment layout; the route is the active tab; the open-tab list is per-browser client state like the recent-opens record; only the routed page is mounted; "+" is a `/fork` page over the person's organizations that reuses the fork dialog after switching the session organization; the workspace bar's Home button goes under this layout while Home's workspace dropdown stays as the discovery surface.
- Davide (2026-10-11): the tab strip is the one bar left above the page, so it hosts the global
  controls on every shell page (Home, workspace, "+"); the ribbon's right end hosts the
  workspace-bound controls in the action-bar style. Interpretation: the title bar stays as it is
  under the classic layouts, which this change does not touch; "search and fork on Home" is a
  workspace directory tile with a search box replacing Home's switcher dropdown, and the "+" fork
  picker gains the same search across organizations. The Chat toggle the title bar showed while
  the center tab strip is hidden moves into the ribbon's workspace controls.
- Davide (2026-10-07 and 2026-10-12, Phase 4): one common format. Interpretation: a meeting's
  decisions are one referenced question set created by the extraction agent through
  `g5d question create` (Accept / Delay / Out of scope / Reject as the options, the comment as
  free text, detail, decider and target document in the context), and a decisions document under
  Project Docs `meetings/` that references each question, so the ordinary card renders it and a
  card action is the owner's Answer. Transcripts are Markdown Project Docs under `meetings/`;
  no `.g5/meetings/` root, no new doc type and no decision-document format. "Review meeting" is
  a ribbon menu (Upload transcript, Extract decisions, Apply accepted decisions); the upload
  starts the extraction with the uploaded document as context. Issue creation for delayed and
  out-of-scope decisions still waits for the tracker write broker.
