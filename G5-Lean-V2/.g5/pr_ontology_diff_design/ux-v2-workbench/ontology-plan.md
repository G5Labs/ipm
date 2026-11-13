# UX v2 workbench — implementation plan

## Ontology changes and governance

**Comparison base:** `origin/main` at `137c906e`, branch `ux-v2/implementation-plan`. The
[chat intent](../../chat_intent/ux-v2-workbench/request.md) separates the 2026-10-06 meeting
decisions from this plan's interpretation.

**Ontology changes:**
- New page [`features/workbench.g5.md`](../../ontology/features/workbench.g5.md): three stories
  (`g5.story.see-workspace-phase`, `g5.story.act-in-phase`, `g5.story.review-a-meeting`) and
  ten decisions (`g5.workbench.phases`, `.phases.state`, `.gates`, `.actions`, `.persona`,
  `.landing`, `.meeting`, `.mock`, `.gate`, `.deferred`). Baseline after Phase 0 landed on this
  branch: `.gate` 100%, `.phases.state` 85%, `.phases` 35% (assessed from code and unit/API tests;
  the Playwright flow is written but unexecuted), the rest 0%.
- `root.g5.md` reading map and `concepts/core_user_stories.g5.md` index link the page (done with
  Phase 0).
- `g5.workbench.gates` is amended in place (same ID) from "map phases onto the existing gates" to
  "Bind approvals to phases": the human decided on 2026-10-06 that approvals associate with phases
  and that stage-based approvals are no longer supported. The approval *policy* (which phases gate
  the merge, BE/SE advisory) has one home, `features/governance.g5.md`
  `g5.governance.approval.requirements`; the workbench block owns the axis change and references it. The `WorkspaceStatusBand` stage names are presentation only.
- The plan is reviewed by independent read-only Codex runs on the current files (no carry-over
  between runs); each run's verdict is recorded in the review summary.
- **Amendment 2026-10-09 (Phase 8, base `origin/main` `d5b94253`):** a comparison of the merged
  plan with the mock session's prompts found three divergences the human asked to fix
  ([chat intent](../../chat_intent/ux-v2-workbench/request.md), 2026-10-09). New story
  `g5.story.track-workspace-issues` and decision `g5.workbench.issues` (rail order with Activity
  and Jobs, the Issues & tasks projection, the issue page actions, and why Done / quick-add stay
  disabled); `g5.workbench.deferred` names the tracker-write gap; the Phase 0 rail order and the
  Phase 2 IE button set are amended in place below.
- **Amendment 2026-10-09 (Phase 9, finding C5, base `origin/ux-v2/rail-ribbon-issues-panel`):**
  `g5.workbench.landing` is amended in place (same ID): the timeline shows every durable activity
  kind — the primary issue's discussion, conversations, jobs and tasks, commits, approvals, phase
  changes, pull requests — and a per-person filter row above the feed chooses which kinds are
  shown. The human asked to keep every activity on the timeline and pick what to see with
  checkboxes on top ([chat intent](../../chat_intent/ux-v2-workbench/request.md), 2026-10-09).
  The governing-table row for "Keep shared discussion at the primary issue" is reworded to what
  the code does; the Phase 1 text stands as history.

- **Amendment 2026-10-11 (Phase 10, base `origin/main` `f3389820`):** new decision
  `g5.workbench.shell-chrome` (the alignment shell has no title bar; which control lives where);
  `g5.workbench.tabs` amended in place (the strip's right end hosts the global controls; the
  picker searches), `g5.workbench.header-styles` amended in place (the ribbon's workspace
  controls take the action-bar style), and `g5.workspace.ui.center-tabs` in
  `features/workspaces.g5.md` amended in place (where the Chat toggle lives under alignment).
  The human chose variant B of the reviewed mock `docs/mocks/titlebar-removal/`
  ([chat intent](../../chat_intent/ux-v2-workbench/request.md), 2026-10-11).

- **Amendment 2026-10-12 (Phase 4 re-planned, base `origin/main` `ae98ca1b`):**
  `g5.workbench.meeting` is amended in place (same ID): a meeting's decisions are a
  [referenced question set](../../ontology/features/documents.g5.md#reference-a-question-from-its-set-file)
  with the four meeting options, not a decision document with its own front matter and
  writer; a card action is the owner's Answer of that question (the store's explicit,
  version-checked save). The human asked on 2026-10-07 whether G5 PR #2532 changes Phase 4 and
  whether question answering and meeting decisions can share one format; the question store
  landed as PRs #409, #427 and #442 and the human asked on 2026-10-12 to start Phase 4 in that
  simplified form ([chat intent](../../chat_intent/ux-v2-workbench/request.md), 2026-10-07 and
  2026-10-12). The Phase 4 text below is rewritten; the governing rows for the meeting and the
  durable `.g5` roots are reworded to the new design; the diagram and the Phase 0/1/2 texts stand
  as history.

Diff: `git diff origin/main -- .g5/ontology/features/workbench.g5.md .g5/chat_intent/ux-v2-workbench/`.

### Relevant and governing ontology

| Block | Why relevant | Implementation must enforce or consider |
|---|---|---|
| [Three phases organize a workspace](../../ontology/features/workbench.g5.md#three-phases-organize-a-workspace) | The organizing decision | Chevrons replace the status band under the alignment layout; IE has no post-merge surface. |
| [Bind approvals to phases](../../ontology/features/workbench.g5.md#bind-approvals-to-phases) | Approval meters, requests, decisions, merge admission | `approval_definitions.phases`, `approval_statuses.phase`; `workspaces.stages` and the fork "Governance model" selector removed; merge admission = IE requirements satisfied; BE/SE approvals never block a phase move; request/settlement fingerprints are phase-aware; `reviewed_git_heads` unchanged. |
| [Merge is a recoverable application of pinned intent](../../ontology/features/governance.g5.md#merge-is-a-recoverable-application-of-pinned-intent) | "Current approval checks" | Today no server path evaluates approval requirements at merge (`job_merge_service.rs` has no approval reference; the status band only tints). Phase 3 adds the admission check — IE requirements satisfied and not stale — at every target-advancing merge path, direct and PR. |
| [Organization administrators administer approval requirements](../../ontology/features/governance.g5.md#organization-administrators-administer-approval-requirements) | Amended | Definitions carry `phases`; the admin editor offers a three-phase picker; only IE-bound requirements are merge requirements. |
| [Phase is explicit workspace state](../../ontology/features/workbench.g5.md#phase-is-explicit-workspace-state) | Durable record and its history | `workspace_phase_events` written in the PATCH transaction is the timeline's source; a fork copies an ordinary source's phase, a Base fork starts at `be`, Base is read-only `be`. |
| [Freeze one recoverable fork operation](../../ontology/features/workspaces.g5.md#freeze-one-recoverable-fork-operation) | Fork inherits phase | The inherited phase is resolved once at admission and frozen in the fork manifest (as `target_stages` is today); recovery never re-reads the source; it is not part of the client request digest. Removing `stages` from the digest in Phase 3 changes retry identity for forks admitted before the cutover — pending/aborting manifests are completed on the old digest or aborted explicitly. |
| [Durable Git boundaries](../../ontology/software_architecture/durability.g5.md) | `.g5/workbench/` | Only allow-listed `.g5` paths are durable repository content (`g5-contracts/src/g5_paths.rs` `is_durable_g5_repo_content_path_str`: `workspace.yaml`, `experience/`, `preview_profiles/*.yaml`, `explainer_templates/*.yaml`); unknown `.g5/*` is excluded from staging (`g5-scm/src/repo.rs`), rejected by exact-tree commits (`commit_preparation.rs`) and hidden from Files. The workbench root was added in Phase 2. Phase 4 adds no `.g5` root (amended 2026-10-12): transcripts, question sets and decision documents are ordinary Project Docs under `docs/<workspace-id>/`. |
| [Reserve one exact input revision](../../ontology/features/conversations.g5.md#reserve-one-exact-input-revision) | Persona prefix | `WorkspaceMessenger` claims the request key on a digest of the submitted content before creating the session; the persona choice joins that identity and its instructions are resolved once after the claim, so a lost-response retry returns the original session without re-reading Git. |
| [Ribbon actions are templated conversations](../../ontology/features/workbench.g5.md#ribbon-actions-are-templated-conversations) | Every button | Templates are data served by a backend loader that merges built-ins with committed `.g5/workbench/` files, following `routes/explainer_templates.rs`; prompt context is serialized into message text (`generateChatPrompt.ts` rule); `kind: open` buttons call `WorkspacePage` handlers that already own the dialogs. |
| [A chat carries one persona](../../ontology/features/workbench.g5.md#a-chat-carries-one-persona) | Session contract | V1: `persona` stored on the session and its instructions prepended to the session's first message under the frozen-prompt contract; adapter-level per-turn injection is a follow-up with its own prompt-hash evidence; "follow view" forks or starts, never mutates. |
| [The project timeline is the landing page](../../ontology/features/workbench.g5.md#the-project-timeline-is-the-landing-page) | Default tab | `workspace_activity` becomes the first tab under the alignment layout; the feed gains phase, approval, document and preview entries. |
| [Review a meeting through a question set](../../ontology/features/workbench.g5.md#review-a-meeting-through-a-question-set) | Amended 2026-10-12 | A meeting's decisions are one referenced question set the extraction agent creates through `g5d question create`; the decisions document beside the transcript references each question, so the ordinary card renders it and Accept / Delay / Out of scope / Reject are the owner's [Answer](../../ontology/features/documents.g5.md#answering-is-an-explicit-save-of-one-file) (explicit single-file save, revision-checked, published before "saved"); readers see the committed cards and may [vote](../../ontology/features/documents.g5.md#let-readers-vote-on-referenced-questions). No decision-document format, parser, conflict helper or route. |
| [Agents read and propose through the workspace CLI](../../ontology/features/documents.g5.md#agents-read-and-propose-through-the-workspace-cli) | Extraction writes the set | The extraction template's contract is the existing agent path: create or append the set with `g5d question create` (never edit a `.g5q.json`), then commit and push the transcript, the set and the decisions document with its work; an agent-written answer would be a proposal, so the template never answers a decision. |
| [Mock preview is a preview profile](../../ontology/features/workbench.g5.md#mock-preview-is-a-preview-profile) | Preview Mock | Button enabled only when a `mock` profile is listed; `PreviewControl` gains a fixed-profile entry point (it selects internally today). |
| [Queued intent is separate from delivered history](../../ontology/features/conversations.g5.md#queued-intent-is-separate-from-delivered-history) | Template prompts | A template submits a normal message; no bypass of inbox/delivery. |
| [Keep shared discussion at the primary issue](../../ontology/features/external_issues.g5.md#keep-shared-discussion-at-the-primary-issue) | Timeline | The feed shows the primary issue's comments and events read live from the tracker as references (no copy is stored); a comment is posted through the existing publication path (`g5.external-issues.publication`); G5 keeps no second comment store. Hiding Issue discussion in the filter row hides those rows and the composer in the browser only. |
| [Publish to an explicit authorized audience](../../ontology/features/external_issues.g5.md#publish-to-an-explicit-authorized-audience) | Delay / Out of scope | No external issue creation in V1; there is no create route and the publication contract needs confirmation UI. |
| [Omit Chat from Base](../../ontology/features/workspaces.g5.md#omit-chat-from-base) | Base workspaces | Base shows chevrons read-only and no template actions. |
| [Offer the alignment UI as a workspace-panels layout](../../ontology/features/workbench.g5.md#offer-the-alignment-ui-as-a-workspace-panels-layout) | Every new surface | Derive visibility only from `resolveWorkspaceLayout(...) === "alignment"`; never branch backend behaviour on it; the three existing layouts and their flows pass unchanged. |
| [Open workspaces are tabs](../../ontology/features/workbench.g5.md#open-workspaces-are-tabs) | Phase 7 | The strip is navigation chrome under the alignment layout only; the route is the active tab; the open-tab list is per-user localStorage (`lib/recentWorkspaces.ts` idiom); one page mounted; "+" is a `/fork` page over `user.orgs` + `workspacesQueryOptions` per org with the `CloneDialog` tree, forking after `switchOrg` as `ProjectsPage` does; the workspace bar's Home button is omitted under the layout. |
| [Pin the selected fork source](../../ontology/features/workspaces.g5.md#pin-the-selected-fork-source) | Fork picker | The picker hands `CloneDialog` the listed `WorkspaceSummary`; the dialog's existing retry draft and idempotency key apply unchanged. |
| [The rail lists the workspace's tracker issues](../../ontology/features/workbench.g5.md#the-rail-lists-the-workspaces-tracker-issues) | Phase 8 | Rail entries are presentation over existing surfaces; Issues & tasks reads `GET /workspaces/:id/issue-links`; Import task posts the existing link route with `role: related`; Detach is the existing unlink; Implement is a bare persona start with `startMessageFromIssue`; no new route, store or flag. |
| [Bound primary issue associations](../../ontology/features/external_issues.g5.md#bound-primary-issue-associations) · [Keep external hierarchies and roles distinct](../../ontology/features/external_issues.g5.md#keep-external-hierarchies-and-roles-distinct) | Subtask label | The panel's "Subtask" is the related link of this workspace; the DTO role stays `related`, conflicts keep the shared `IssueLinkConflictNotice` semantics, and nothing is written to the tracker's hierarchy. |
| [Derive completion from the work actually accepted](../../ontology/features/external_issues.g5.md#derive-completion-from-the-work-actually-accepted) · [Keep integration writes at one enforcing boundary](../../ontology/features/external_issues.g5.md#keep-integration-writes-at-one-enforcing-boundary) | Done / Reopen, quick-add | No provider operation closes, reopens or creates an issue (`g5-scm/src/providers/issues.rs` reads and comments only) and no broker exists; the controls render disabled with that reason (`g5.workspace.ui.unavailable` convention) and the write path is V2. |
| [Justify each new mechanism](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Minimal design | One column plus one event table, one session column, one config loader on an existing pattern, one document kind. No new task kind or setting key; one new instance feature and one new layout value. The stage model is removed, not wrapped. |

## Requested outcome

With the "Workspace panels" setting on *Alignment* (offered when the instance advertises `alignment_ui`), a workspace opens on its timeline with a status header, three phase chevrons
with approval meters replace the status band, a per-phase ribbon starts templated chats with the
phase persona or opens the existing approval/PR/rebase/merge/preview surfaces, the chat shows its
persona and can follow the view, and a "Review meeting" flow turns an uploaded transcript into
decision cards. Home rows show the three-phase stepper and the needs-input / working badges.
Everything else in the mock is labelled V2.

## Design

```text
Home (HomePage)                       Workspace (WorkspacePage, workspaceLayout === "alignment")
 recent rows + stepper + badges  ──▶  ┌ chevrons BE · SE · IE  (phase from DTO, PATCH on click)
                                      │   meters: per-phase approval roster (Phase 3)
                                      ├ ribbon (actions.yaml for phase)
                                      │   kind=chat   → start/append session {persona, prompt, context}
                                      │   kind=open   → approvals | PR dialog | rebase/merge | preview(mock profile)
                                      ├ left sidebar (existing sections, workbench order)
                                      ├ center tabs: timeline (default) · docs · meeting · approvals · …
                                      └ chat: persona pill · "follow view"
Backend additions
 workspaces.phase column + workspace_phase_events (written in the PATCH tx) + DTO; fork: copy from ordinary source, Base → be
 approval_definitions.phases[] · approval_statuses.phase · workspaces.stages dropped · merge admission = IE requirements
 chat_sessions.persona; StartSessionRequestDTO.persona in the request digest; persona text prepended to the first message after the key claim
 g5_paths durable allow-list + scm exclude/commit rules: add .g5/workbench/ and .g5/meetings/ (prerequisite of Phases 2 and 4)
 GET /workspaces/:id/workbench → one committed snapshot of .g5/workbench/{actions.yaml,personas/*.md} over built-ins (explainer_templates loader)
 docs upload: accept .txt/.vtt/.srt; transcripts and decision documents live under .g5/meetings/ and are published by explicit Save
 approvals: merge admission evaluator (actual target + pinned source vector) called before provider merge and at local application
 activity feed: PhaseChanged (from workspace_phase_events), PullRequest (from workspace_pull_request_events); previews stay Task entries
Git content (per workspace, defaults shipped in-tree)
 .g5/workbench/actions.yaml   .g5/workbench/personas/{be,se,ie}.md   .g5/preview_profiles/mock.yaml
 .g5/meetings/<date>-<slug>.transcript.txt  +  <date>-<slug>.decisions.md
```

**Owners and boundaries.** Workspace owns `phase`; governance owns gates and merge; conversations
own sessions and persona delivery; documents own transcripts and decision documents; preview owns
the mock profile. The workbench adds presentation and two small records (phase, persona) and
otherwise composes existing operations. Templates and personas are read from the committed view of
the workspace's docs branch (`resolve_branch_committed_view`, as the explainer-template loader does),
never from a live checkout, so they need no VM and a fork sees its source's files. Failure modes are
the owners' own: a template whose session fails to start shows the ordinary chat error; a decision
save against a changed document shows the ordinary version conflict.

## Phases

Each phase is one PR with Playwright flows added to `frontend/e2e/flows/` and MSW unit tests beside the
components. Phases 0, 1, 2, 4 and 5 are visible only under the `alignment` layout; **Phase 3 is a
global governance cutover** that applies under every layout. Sizes are per implementer.

**Dependency order.** 0 → 3 (meters, approvals tab, merge admission all read phase-bound approvals;
Phases 1 and 2 must not build on `approvalStages.ts`, which Phase 3 deletes) → 1 ∥ 2 → 4 → 5.
Phase 1 can run beside Phase 3 by a second implementer if its approval entries use the phase DTO.

**Friday 2026-10-09 slice** — a stretch target, not a commitment, needing the human's confirmation
and two implementers: Phase 0 (done); Phase 3 as **one release unit** — schema, backend, frontend
cutover and merge admission with recovery tests (≈3 days, the critical path); Phase 1 timeline
landing; Phase 2 reduced to the built-in templates and `kind: open` buttons with the **basic business
persona on new sessions** (the V1 commitment); from Phase 4 only transcript upload with the durable
root, discovery and publication plus the extraction template, per the meeting's own expectation.
Deferred past Friday: committed `.g5/workbench/` overrides, persona switching ("follow view"),
decision cards and their polish, Home stepper.

### Phase 0 — Shell: phases, ribbon skeleton, gate (≈1 day)

- Gate: `G5_ALIGNMENT_UI_ENABLED` env → `alignment_ui` in the instance features list
  (`routes/instance.rs`, beside `top_panel`); `WorkspaceLayout` gains `"alignment"`;
  `resolveWorkspaceLayout(topPanelEnabled, alignmentEnabled, stored, default)` returns the instance
  default when `alignment` is stored but the feature is off; the "Workspace panels" select in
  `WorkspaceSettingsContent.tsx` shows "Alignment — phases and action ribbon" only when the feature
  is on. Off by default in every compose/helm/CI file. Playwright specs gate on the Node-side env.
- Backend: migration `059_workspace_phase.sql`: `workspaces.phase` (`be|se|ie`, default `be`) and
  `workspace_phase_events` (org, workspace, from, to, actor, created_at; RLS like
  `workspace_pull_request_events`); `PATCH /workspaces/:id {phase}` (owner only, Base rejected) updates
  the column and inserts the event in one transaction; DTO field on list/detail. Fork: the inherited
  phase (ordinary source's phase, or `be` for a Base source) is resolved once at admission and frozen
  in the fork manifest beside `target_stages`, written from the manifest at finalization, never
  re-read during recovery, and not part of the request digest. Base is always read-only `be`.
- Frontend: `PhaseChevrons` replaces `WorkspaceStatusBand` and `ReposBar` when `workspaceLayout === "alignment"`; phase hues
  (`--g5-phase-be/se/ie`) added to `index.css`; `ActionRibbon` renders from a static default
  `actions.yaml` (buttons disabled until Phase 2 wires them); `ChatPanel` header shows the
  phase scope pill; `LeftSidebar` default order becomes project-docs, ontology, files, tasks
  (amended 2026-10-09 by Phase 8: Activity · Project docs · Ontology · Files · Issues & tasks · Jobs,
  per mock prompts #24 and #26).
- Tests: chevron click persists phase across reload and writes one event row; fork from ordinary
  source copies the phase, fork of Base starts at `be`, a source phase change between fork admission
  and finalization does not change the fork; Base shows chevrons read-only; feature off ⇒
  Settings shows three layouts and a stored `alignment` resolves to the default; `simple/advanced/both`
  unchanged (`workspace.spec.ts`, `settings.spec.ts` guards).

### Phase 1 — Timeline landing (≈1 day)

- `workspace_activity` becomes the default/first center tab under the alignment layout; `WorkspacePlanOverview`
  leaves the landing path (still reachable from the ribbon's Analyze menu).
- Status header: phase, per-phase approval state (from the Phase 3 phase DTO; until Phase 3 lands,
  the header shows the phase only), primary issue title/link, purpose from the primary issue body or
  `README` fallback.
- Activity feed (`g5-api-core/src/services/workspace_activity.rs`, `ActivityEntryBody`, mirrored by the
  frontend union and `WorkspaceActivityPanel` switch): add `PhaseChanged` projected from
  `workspace_phase_events` and `PullRequest` from `workspace_pull_request_events`. These two entry
  kinds are durable data and render in Activity under **every** layout; only the landing behaviour and
  the status header are alignment-only. A PR event is attributed to a provider PR only when the PR has
  exactly one provider repository; otherwise the row shows the aggregate G5 PR. Approvals, chats,
  jobs and tasks are already projected, and preview tasks already appear as `Task` entries, so no
  `PreviewStarted` variant is added — the existing preview task presentation is enriched instead.
  Agent tasks are deliberately represented by their chat-session entry, so meeting extraction and
  update runs appear as conversations, not separate task events. Document events are added only from
  actual creation records or Git evidence, never from document-list timestamps (which `doc_service`
  synthesizes at read time); nothing is synthesized client-side. `WorkspaceActivityPanel`'s
  stage-specific approval presentation is replaced by phase in Phase 3.
- Tests: fresh fork lands on the timeline with "Business Engineering · Gate 1 not requested"; phase
  change appears as an entry.

### Phase 2 — Templated actions and personas (≈1.5 days, with Zhengchun on prompts)

- Prerequisite: `.g5/workbench/` added to the durable `.g5` allow-list (`g5_paths.rs`), the SCM
  exclude rules and exact-tree commit acceptance, with staging/Save/committed-read/Files-visibility
  and fork-inheritance tests.
- `actions.yaml` schema: `phase → groups → actions[{id,label,kind: chat|open,primary,persona,
  prompt,context: current_doc|phase|none,target,phase?,repoId?,profile?}]`. Served by
  `GET /workspaces/:id/workbench`, a loader that merges built-in defaults with committed
  `.g5/workbench/actions.yaml` and `personas/<phase>.md`, copied from
  `routes/explainer_templates.rs` (built-ins + `.g5/explainer_templates/<slug>.yaml`), scoped to the
  workspace's docs repository and read from the committed branch view. Override semantics differ from
  the append-only explainer catalog: a committed `actions.yaml` replaces the built-in action set for
  each phase it names (actions matched by `id` keep built-in behaviour fields they omit), and a
  committed `personas/<phase>.md` replaces that phase's built-in persona. Actions and personas are
  read from one committed snapshot whose revision is retained for the request being resolved.
  Policy, distinct from the append-only explainer catalog: malformed YAML, duplicate ids, unknown
  `kind`/`target`/phase or missing required fields make the configuration **invalid** — the route
  returns the error and the ribbon shows "workbench configuration invalid" with the built-ins, never
  silently restoring actions a team removed; absent configuration means built-ins; authorization or
  transient fetch failure shows the client fallback with a retry, labelled as such.
- `kind: chat`: the browser submits `{actionId, context, configRevision, persona}` plus any typed
  text; after the request key is claimed the server resolves the action's prompt and the persona's
  instructions together from the pinned `configRevision` (rejecting a stale revision before first
  admission), serializes prompt + context into the message text (the `generateChatPrompt.ts` rule:
  complete requests travel as message text) and persists the expanded input and revision for replay.
  If a session with that persona is open, the action appends to it; meeting operations always start
  a dedicated conversation (Phase 4). `kind: open`: the ribbon is an orchestrator inside `WorkspacePage` that
  calls the handlers already owning each surface — Approvals → `workspace_approvals` tab filtered to
  the phase; Create PR and rebase → the existing `PrepareToShipModal` open handler and its prop set;
  Merge to parent → the existing ship surface for the parent PR (`PrepareToShipModal` →
  `useMergeWorkspacePullRequest`), which is the only target-advancing flow; the rebase-confirm
  handlers (`confirmWorkspaceMergeForTarget`, `handleMergeReviewApproval`) only rebase the source and
  stay bound to "Create PR and rebase"; Preview / Preview mock → `PreviewControl`
  extended with a fixed `{repoId, profile}` entry point — preview applications are keyed by
  repository, source and profile and the docs repository is excluded, so the action names the code
  repository (`repoId`, defaulting to the only non-docs repo exposing `mock`, otherwise a picker; a
  docs-only workspace disables the button). Profile discovery for enablement reuses the preview-profile
  list query at page load, not the modal's lazy load. `PreviewControl` compares the active preview's
  `{repoId, profile}` with the requested target: same → open it; different → an explicit
  "close and start mock" transition (one preview at a time); none → start. Build mock's template
  creates or maintains that repository's `.g5/preview_profiles/mock.yaml`, whose command serves the
  generated artifact from Project Docs.
- Backend: `chat_sessions.persona` and `StartSessionRequestDTO.persona` (validated `be|se|ie`). V1
  delivery: `persona` joins the `WorkspaceMessenger` request digest; after the request key is claimed
  the API resolves the persona's instructions once from the committed snapshot, prepends them to the
  first message and persists the expanded input atomically with the session, so a lost-response retry
  returns the original session without re-reading configuration and the frozen prompt
  (`g5.conversation.inbox.prompt`, bytes hashed before spawn, 512 KiB cap including the prefix) is
  untouched. The persona's resolved instructions are also frozen on the session: if the first queued
  input is withdrawn or edited before delivery, the next first-delivered input is bootstrapped from
  those frozen instructions, so a persona-labelled session never reaches the provider without them;
  an ordinary fork whose copied boundary predates the prefix is bootstrapped the same way. Per-turn adapter injection (Claude `--append-system-prompt`,
  Codex instruction file in `g5d-agent-turn`) is a follow-up with its own dispatch-schema, hashing and
  transport tests; it is not required for Friday.
- Chat: persona pill. "Follow view" (after Friday) extends the fork-session operation
  (`chat_service` fork: index + title today, always a fresh session, rejects an empty transcript)
  with an optional `persona`: fork creation, the fork's `persona` and its first new inbox input
  carrying the persona instructions commit together under one request identity, so no persona-labelled
  fork exists without its instructions; the copied transcript, inherited prefix included, is
  historical context; the transcript boundary for an active turn is the last completed assistant
  unit; an empty or queued-only source starts a new session instead. An ordinary fork or carry
  inherits the source persona without re-injecting it.
- V1 button set: BE — New doc, Import ▾ (Upload file, Import user stories), Analyze ▾
  (Completeness of implementation, Test plan), Review meeting, Build mock, Preview mock, Approvals.
  SE — New doc, Import, Analyze, Review meeting, Create implementation plan, Generate code,
  Preview, Approval, Create PR and rebase. IE — Approval, Create PR and rebase, Merge to parent
  (amended 2026-10-09 by Phase 8: IE also carries the shared Author and Analysis groups — New doc,
  Import, Analyze, Review meeting — because mock prompt #26 puts them in every ribbon; the meeting's
  IE cut removed only the IE-specific actions).
- Tests: Build mock starts a `be` session whose first message is the template; Create PR opens the
  ship modal; Preview mock disabled without profile; follow view forks with new persona.

### Phase 3 — Approvals bound to phases (≈3 days)

Removes the planning/finalizing stage model; stage-based approvals are not supported afterwards.

- Migration `060_approval_phases.sql`: `approval_definitions.phases text[] NOT NULL DEFAULT '{ie}'`
  with CHECK `phases <@ '{be,se,ie}'` and no duplicates; an empty array is allowed and means
  attached-but-inert, which is how no-stage and planning-only/`skip` one-stage definitions behave
  today (`applicable_stages` doc, `one_stage_fallback_carry_over_and_skip`). Backfill per
  definition version: `{planning, finalizing} → {be, ie}`, `planning only → {be}` (both
  `carry_over`, which loses its one-stage ship gating, and `skip`, which becomes newly actionable as
  a BE sign-off), `finalizing only → {ie}`, neither → `{}`. Historical decisions keep their mapped
  phase: an old carry-over finalizing decision stays an IE decision and is not turned into a BE
  sign-off. Accepted behaviour changes, listed in the migration header. Drop `stage_planning`, `stage_finalizing`, `planning_fallback`.
  `approval_statuses.phase text` CHECK in (be,se,ie), backfill `planning → be`, `finalizing → ie`,
  drop `stage`; replace the SQL validators that encode the stage contract
  (`approval_review_context_v*_is_valid` and the settlement-evidence validators in
  `001_lean_baseline.sql` ~2159) with phase-aware versions. Drop `workspaces.stages` and its CHECK.
- Backend consumers to cut over (enumerated by review): `approval_config_service.rs`
  (`applicable_stages` → the definition's `phases`; `validate_decision_stage`; the finalizing default
  at the decision route; settlement fingerprints — today stage is encoded only for planning so
  finalizing stays byte-identical — become phase-aware for every phase, with a deliberate fingerprint
  break since no compatibility is kept), `approval_review_service.rs` (settled stage default,
  idempotency comparing `existing.stage`, admission context), `approval_definition_service.rs`,
  `review_approval_sync.rs`, `routes/org_approvals.rs`, `routes/workspace_approvals.rs`,
  `workspace_fork_service.rs` (`stages` leaves the request and the manifest's `target_stages`; the
  request digest is versioned — v2 without `stages` — and replay keeps matching v1 digests for every
  record admitted before the cutover, completed and aborted included, so lost-response retries keep
  their terminal outcome per `g5.fork.replay.terminal`; manifests admitted before the frozen-phase
  field read as `be`),
  the fork route body, every other `stages` reader/writer (`workspace_service.rs`, `org_service.rs` Base creation, `g5-api-data` workspace
  model, fork constructors in `routes/auth.rs` and `routes/workspace_reviews.rs`, VM test fixtures),
  `workspace_activity.rs` (selects `decision.stage`), the shared settlement contract in
  `g5-contracts/src/types/approval_review.rs` (`stage: Option<String>` → `phase: String`),
  `home_service.rs` attention items, agent approval runs.
  **Request identity:** one request chain per (definition version, phase): the request key keeps
  its tenant, workspace, definition-version and actor-slot dimensions and adds `phase`; persistence,
  successors and agent runs carry it; settlement fingerprints include the phase for every phase
  (today stage is encoded only for planning). Cutover uses existing terminal states only: pending
  requests become `superseded` and active review runs are `cancelled` with a cutover reason (the run
  state CHECK and transition trigger in `001_lean_baseline.sql` admit no new state); late settlements
  against them are rejected, and cancelling a run goes through the existing task/job cancel path so
  a late terminal callback is harmless. Historical data: `jobs.approval_context` is immutable and
  SQL-validated, so the baseline migration is not edited; migration 060 adds phase-aware validators
  that accept the old stage-shaped documents and old fingerprint formats as historical evidence while
  new submissions must carry `phase`; Rust reads old contexts through a tolerant decoder
  (`planning → be`, absent/`finalizing → ie`). The migration runs under the restricted migrator role
  with an RLS-aware data scope on populated fixtures and is repeat-no-op.
  **Merge admission:** no server path evaluates approvals at merge today (`job_merge_service.rs`
  target-advancing merge and its admission function do not reference approvals; the ship modal gates
  on PR readiness only, never approvals; the status band only tints). Phase 3 adds one evaluator
  taking the **target and the pinned source head vector**, reusing the existing eligibility
  evaluation (v3 published-source and v4 assigned-source evidence). **Target scope (V1):** admission,
  approval projection, request admission and agent assignment all use the workspace's configured
  target (`merge_target_workspace_id`, else `cloned_from_id`); a PR whose destination differs is
  refused by admission rather than evaluated against requirements the approval page cannot show.
  **Locking:** admission takes the target's requirement locks for every definition attached to the
  target (today's locks are per target + definition, so attachment of a new definition is serialized
  by also locking the target's requirement set), in the same order as attach/retire/publish/withdraw.
  It is invoked before any provider mutation in the PR-merge route (`merge_workspace_pull_request`
  calls the provider before preparing the local merge) and records the admission evidence on the
  pinned merge; local application under the existing commit fences re-reads that recorded evidence,
  so an approval change after provider acceptance does not strand an already-merged provider PR —
  the landing proceeds and the change is shown on the merge record. Source-only rebase stays
  available. It refuses
  when any IE-bound requirement is unsatisfied or stale against the pinned revision; BE/SE
  requirements never block. `reviewed_git_heads` and stale-approval binding are unchanged. Tests: `api_workspace_approvals.rs`, `api_org_approvals.rs`,
  `api_workspace_reviews.rs`, `api_home.rs` rewritten for phases; migration test over all four flag
  combinations × stage count × fallback on seeded rows; direct API merge rejected for pending/stale IE approvals and allowed
  with pending BE/SE approvals; PR-merge route refuses before the provider call; cross-phase request
  isolation and retry; every active run state at cutover and a late settlement after it.
- Frontend: `approvalStages.ts` → `approvalPhases.ts` (`approvalRosterForPhase`); DTOs/zod drop
  `stagePlanning/stageFinalizing/planningFallback/stage/stages` for `phases`/`phase`;
  `OrgApprovalsPageContent` definition editor gets a three-phase picker; `WorkspaceApprovalsPageContent`
  loses the two-stage action guard and filters by phase; `useWorkspaceApprovals` carries `phase` on
  `ApprovalRequestTarget`, human and agent request bodies, bulk requests, pending-request selection,
  successor matching, `approvalHistory` association and query keys (the projection stops sharing
  requests across phases); `useOrgApprovals` and the shared contract tests
  (`g5-common/tests/approval_review_contract.rs`) updated; `CloneDialog` drops the "Governance model" cards, the `stages` draft/body field
  and groups the approvals preview by phase; `WorkspaceStatusBand` (`shipApprovalRoster`) falls back
  to its one-stage titles under the legacy layouts; `PrepareToShipModal` ship roster/readiness,
  `HomePage` ship roster, `PullRequestList` merge-readiness and `WorkspaceActivityPanel` approval
  entries all read IE-only admission so pending BE/SE approvals never produce a shipping warning;
  MSW handlers and fixtures updated.
- Chevron meters from the phase roster for all three phases; Approvals tab accepts a `phase`
  filter; "Request approval" from the ribbon preselects the phase. A forward phase move is never
  blocked by BE/SE approvals; the IE meter and Merge show the admission state.
- Ontology: amend `g5.governance.approval.requirements` (requirements bind to phases; only IE
  requirements gate the merge) and check `g5.governance.merge` wording about "current approval
  checks" still reads correctly.
- Tests (UI): BE approval decided ⇒ BE meter 1/1 and a timeline entry; IE pending ⇒ Merge disabled
  with the admission message and the API refuses it; fork dialog has no governance selector and
  previews approvals by phase.

### Phase 4 — Review meeting (≈1 day, re-planned 2026-10-12)

Rewritten after the question store landed (PRs #409, #427, #442): the 2026-10-07 answer to "does
[G5 PR #2532](https://github.com/G5Labs/G5/pull/2532) change Phase 4?" was that a meeting decision
is a question with a proposed answer, and the human asked for one common format. The earlier text
(a `.g5/meetings/` root, `meeting_transcript`/`meeting_decisions` doc types, a front-matter decision
document rewritten by whole-document Save, draft-versus-durable cards, a shared conflict helper) is
withdrawn: every one of those obligations is discharged by the store, and the phase is now ribbon
data plus frontend wiring.

```text
  Review meeting ▾ ── Upload transcript ──► CreateProjectDocModal (transcript mode)
                                              docs/<ws>/meetings/<name>.md  (live write, VM required)
                                                      │ on success: open the doc, start
                                                      ▼ `extract-decisions` with current_doc = that path
                   ── Extract decisions ──► BE persona chat ──► g5d question create --set meeting-<slug>
                                                               one question per decision:
                                                                 prompt = the decision · context = detail,
                                                                 who decided, target document
                                                                 options accept | delay | out-of-scope | reject
                                                                 free text = the comment
                                                              + docs/<ws>/meetings/<name>.decisions.md
                                                                 one `g5-question-ref` per decision
                                                              + commit and push the three files
                   ── Apply accepted decisions ──► BE persona chat over the decisions document:
                                                   `decision: true` + accept → update the target docs
                                                   delay / out-of-scope → listed with their destination
  decisions document in SpecEditor ──► ReferencedQuestionCard per decision (Accept / Delay / … = Answer)
```

- **Decisions are a question set** (`g5.workbench.meeting`, amended). One set per meeting at
  `docs/<ws>/questions/meeting-<slug>.g5q.json`, created and appended only through `g5d question
  create` (the [agent path](../../ontology/features/documents.g5.md#agents-read-and-propose-through-the-workspace-cli));
  the set id is `meeting-<slug>` where the slug is the first 56 characters of the transcript's
  file stem in lower case with every other character replaced by a hyphen (bounded by the store's
  64-character id; two transcripts whose names differ only in such characters share one set, an
  accepted edge); the four meeting actions are the question's options with fixed ids `accept`,
  `delay`, `out-of-scope`, `reject`; the comment (for Out of scope: the intended project, assignee and
  title) is the answer's free text; detail, who decided it and the target document go in
  `context` — no schema change, no `kind` discriminator, no `source` block (the set title names
  the meeting, the decisions document links the transcript). Re-running "Extract decisions" on
  the same transcript appends to the same set by id and edits wording under the revision rule;
  the store keeps answered entries and shows "answered against an earlier revision". A decision
  is the owner's Answer; an agent answer would be a proposal, so the template never answers.
- **Transcript and decisions document are Project Docs.** The upload stores one transcript at a
  time as Markdown at `docs/<ws>/meetings/<name>.md` (`.txt` and `.md` verbatim; `.vtt`/`.srt`
  wrapped in one fenced block, longer than any backtick run inside, so cues and timestamps render
  as typed); the modal's transcript mode accepts
  those four extensions and nothing else, with its own copy; the ordinary upload mode is
  unchanged. No new doc type, extension or docs-surface root in `doc_service.rs`: the `.md`
  rule already covers both files, Project Docs lists `meetings/` as the "Earlier meetings"
  surface, and `.g5/meetings/` would have bought a second allow-list for nothing. Durability is
  the ordinary one for agent-written docs: the upload is a live write (owner's VM required, like
  every doc creation), the extraction template commits and pushes the transcript, the set and
  the decisions document with its work, and readers see the committed cards.
- **Ribbon.** The shared Analysis group's `review-meeting` becomes `kind: menu` in all three
  phases (shared-group equality kept): Upload transcript (`open`, `target: meeting`), Extract
  decisions (`chat`, `context: current_doc`, `persona: be`), Apply accepted decisions (`chat`,
  `context: current_doc`, `persona: be`; it reads the set with `g5d --json question list`, since
  the plain listing omits each entry's context and so the target document). `ActionRibbon` stops disabling `meeting`;
  `WorkspacePage` opens the modal in transcript mode for it and, after a single successful
  upload, opens the document and starts the current phase's `extract-decisions` action with that
  path as the current-doc context — one user action is upload plus analysis. A committed
  `.g5/workbench/actions.yaml` that drops `extract-decisions` leaves the transcript merely open.
  The prompts are first drafts for Zhengchun like the other built-ins. "Analyzing…" is the
  extraction conversation in the chat panel (the meeting kept V1 at "upload + chat"); the
  decisions document appears in Project Docs when the agent writes it, and the timeline shows the
  extraction and apply conversations and the agent's commit.
- **Cards.** The decisions document renders through the existing SpecEditor card; Accept / Delay
  / Out of scope / Reject is the owner's Answer (explicit single-file save, revision-checked,
  "saved" only after publication, the store's conflict handling); non-owners and a stopped
  workspace see the committed cards read-only and may vote; undecided decisions count as open
  questions on Home attention. No "(N)" counter on the apply action in V1.
- **Deferred inside this phase:** creating external issues or tasks for delayed and out-of-scope
  decisions (tracker write broker, as in Phase 8); Drive import and recording; a meeting tab with
  its own "Analyzing…" state.
- Tests: `workbench_config.rs` built-ins (menu shape, shared groups, generated fallback),
  `ActionRibbon.test.tsx` (menu enabled, items), `CreateProjectDocModal.test.tsx` (transcript
  mode accepts `.txt/.vtt/.srt/.md`, rejects others, `.md` destination under `meetings/`, fence
  for cues), `WorkspacePageAlignment.test.tsx` (Upload transcript opens the modal in transcript
  mode; a finished upload opens the doc and starts `extract-decisions` with the uploaded path),
  `e2e/flows/alignment-shell.spec.ts` (upload a transcript → the extraction chat starts with the
  transcript as context; the fake provider's meeting fixture creates the set and the decisions
  document through `g5d question create`; the cards show the four options; Accept persists as a
  decision and is published).

### Phase 5 — Home stepper and badges (≈0.5 day)

- Recent-workspace rows: three-phase stepper from `phase` + phase rosters; "Working" from
  `WorkspaceSummary.concurrentTasks`; "Needs user input" shows positive evidence only — items of the
  loaded attention page for that workspace (`useHomeAttention` pages 20 items with no per-workspace
  totals) and an "unknown" state otherwise — until a bounded per-workspace attention aggregate exists
  (V2 backlog); no page-walking to populate badges. Clicking a phase chip opens the workspace on that phase (PATCH if owner).
- Tests: row reflects phase and badges from fixtures.

### Phase 6 — Home: Issues and tasks, Running compute (≈1 day)

Added 2026-10-07 at the human's request after a search showed both tiles are projections over existing
APIs, not new infrastructure. Visible only under the alignment layout; classic Home unchanged.

- **Issues and tasks tile** (triage slice, not a backlog): the org's open issues from
  `GET /orgs/:id/issues/search` (`q`, `state`, `assignee=me|none`) — top eight by `updated_at`, filters
  All / Assigned to me / Unassigned, label chips derived client-side from the loaded slice, row anatomy
  state icon · `display_key` · title · label pills · assignee or "unassigned" · age · linked-workspace
  chip (`primary_workspace`); **Start workspace** reuses the existing fork-with-issue path; **Link
  existing** reuses `POST /workspaces/:id/issue-links` with a workspace picker; "View all" opens the
  existing issue search. No quick-add and no sub-issue drag-and-drop: issue creation waits for the
  external-issues publication path (Phase 4a); parent/child is a tracker concept.
- **Running compute tile**: the existing org running-VM inventory (`list_host_workspaces` over
  `workspace_vms` ⋈ `workspaces`: status, `started_at`, `last_activity_at`, host, owner) exposed to
  members as `GET /orgs/:id/compute/running` under `workspaces:read` — same query, filtered to
  workspaces the caller may read — plus the host's slot size (`DEFAULT_WORKER_HOST_SLOT_CPUS` /
  `_MEM_MIB`, instance type on the host record) for "4 vCPU · 16 GB"; rows show status dot, workspace,
  uptime, owner, **Open** and **Shut down** (owner only, `POST /workspaces/:id/stop`, with the
  existing stop confirmation and "shutting down…" state); filters All / Mine. Token cost per workspace
  is not shown (V2: `WorkspaceListItemDTO.cost` exists but is billing-admin territory).
- Tests: tile renders from search fixtures with the three filters and label chips; Start workspace and
  Link existing call the existing paths; compute tile from inventory fixtures, Shut down calls stop and
  shows the transient state, non-owner rows have no Shut down; classic layout unchanged; the new read
  refuses cross-org callers and filters unreadable workspaces (API test).

### Phase 7 — Tab shell and organization fork picker (≈1.5 days)

Pulled back from the V2 backlog on 2026-10-08 (`g5.workbench.tabs`, `g5.story.switch-open-workspaces`);
mock stories B1–B4. Frontend only, gated by `resolveWorkspaceLayout(...) === "alignment"`.

- **Tab state** `lib/workspaceTabs.ts`: `{orgSlug, name}` entries keyed by route path, persisted per
  user at `g5:workspaceTabs:<userId>` (pure list ops + tolerant read + non-fatal write, capped), the
  `recentWorkspaces.ts` idiom. No backend record.
- **Provider** `hooks/useWorkspaceTabs.tsx`: `WorkspaceTabsProvider` mounted once in `App.tsx` inside
  `AuthProvider`/`BrowserRouter`; derives `enabled` from the resolved layout and an org route
  (`/:org/home`, `isWorkspaceShellRoute`, `/fork`), the active tab from `useLocation`, and appends a
  tab whenever a workspace route is visited (deep links, switcher, Home rows and forks all land
  there). Actions: `selectTab` (navigate), `closeTab` (remove; if active, navigate to the left
  neighbour else that org's Home), `openHome` (route org's Home, else the active org's), `openNew`
  (`/fork`). Default context is disabled, so pages render unchanged in tests without the provider.
- **Strip** `components/layout/WorkspaceTabStrip.tsx`, rendered by `AppShell` (which already reads
  a context for the pre-production banner) so Home, the workspace page and the picker get it
  without per-page surgery: pinned Home tab, workspace tabs (× and middle-click close), a transient
  muted "New tab" while `/fork` is routed, and the "+" button. `role=tablist`/`tab`, stable
  test ids.
- **Fork picker** `pages/ForkPickerPage.tsx` at `/fork` (`RequireAuth`; a static route beside
  `/projects`, so no workspace name is reserved): one card per `user.orgs` entry with its role,
  that organization's workspaces via `useQueries(workspacesQueryOptions)` rendered as the
  lineage tree (`buildWorkspaceTreeRows` moves from `CloneDialog.tsx` to `lib/workspaceTree.ts`),
  Base first in bold, children indented with connectors; per-org loading and failure states.
  **Fork** → `switchOrg(orgId)` then `CloneDialog` (as `ProjectsPage` does); `onCloned` navigates
  to `/${orgSlug}/${name}${chatLandingQuery(sessionId)}`, which adds the tab and retires the
  transient New tab. "Create Organization" reuses `CreateOrgDialog` under the Account flyout's
  gate. The picker is reachable only through "+", so no layout branch inside the page.
- **Redundant navigation (B3)**: `WorkspacePage` passes no `onOpenHome` under the alignment
  layout, so `TopBar` hides the Home button. Home's `WorkspaceSwitcher` stays: it is the only
  discovery surface for workspaces that are neither open nor recent (assumption, see risks).
- **Removal**: Home's and the workspace page's remove flows call `closeTab` for the removed
  workspace so a dead tab does not linger; a tab whose workspace is gone otherwise shows the
  page's existing not-found state and closes by hand.
- Tests: `lib/workspaceTabs.test.ts`; `components/layout/WorkspaceTabStrip.test.tsx` (tabs follow
  the route, pinned Home, close fallback left/Home, middle-click, persistence per user, disabled
  under classic layouts); `pages/ForkPickerPage.test.tsx` (orgs with roles, tree order, per-org
  failure, Fork switches org then opens the dialog, success navigates); `TopBar`/alignment page
  test for the hidden Home button; `e2e/flows/alignment-shell.spec.ts` gains the tab flow (open →
  tab, + → fork Base → new tab active and picker gone, close → left neighbour, reload keeps tabs).

### Phase 8 — Rail, IE shared group, workspace Issues & tasks (≈1.5 days)

Added 2026-10-09 at the human's request after the prompt comparison (`g5.workbench.issues`,
`g5.story.track-workspace-issues`). Frontend plus one YAML change; every surface except the
built-in YAML is gated by `resolveWorkspaceLayout(...) === "alignment"`.

- **Rail** (`components/tree/TreeNode.tsx`): `SidebarSectionId` gains `activity` and `issues`, both
  members of every canonical order (so a stored order survives switching layouts) and `hidden`
  under the `default` preset, so classic layouts render unchanged. `WORKBENCH_SECTION_ORDER` =
  activity · project-docs · ontology · files · issues · tasks · explainers · all-docs ·
  source-control; the two outgoing canonical orders join `LEGACY_DEFAULT_SECTION_ORDERS` so a
  never-customised stored order follows the new default. `activity` is a header-only entry: its
  activation calls a new `onOpenActivity` prop (the page opens/focuses `workspace_activity`), it
  has no body and never stays expanded. `tasks` is titled "Jobs" under the workbench preset ("Task
  Queue" otherwise; test id stays `sidebar-jobs-tab`). Rail/overlay display modes treat the two new
  ids like any section.
- **Issues & tasks section** (`components/external-issues/WorkspaceIssuesSection.tsx`, new, sidebar
  variant of the Details panel): rows from `useWorkspaceIssueLinks` — primary first — each with a
  done checkbox that reflects `issue.state === "closed"` and is disabled with the reason "Marking
  issues done from G5 needs the tracker write path, which is not available yet"; `displayKey` in
  mono; title; a PRIMARY / SUBTASK pill (`related` → Subtask); a `closed`/`open` state icon from
  `ExternalIssueParts`. Row click → `onOpenIssue(link)` (the existing `external_issue` tab). Typed
  read errors per row as the Details panel shows them; feature-off and no-org states reused. Header
  action "+ Import task" (owner only; `writeBlockedReason` for a review copy) opens the picker. A
  dashed quick-add row at the bottom ("Add a task…") is disabled with "Adding tasks from G5 needs the
  tracker write path, which is not available yet".
- **Import task picker** (`components/external-issues/ImportTaskPicker.tsx`, new, a `Modal`): one
  `useExternalIssueSearch(orgId, q, "open", "any")` over the organization's domains with a search
  box, rows with checkboxes (already-linked issues shown checked and disabled), a counting button
  "Import N tasks" that posts `POST /workspaces/:id/issue-links {ref: displayKey, role: "related"}`
  per selection in sequence through `useLinkWorkspaceIssue`, per-row success/failure with the shared
  `IssueLinkConflictNotice`, and the link list invalidated as today. Domain failures reuse the Home
  tile's unavailable / partial / empty presentation (`homeTileLogic.ts` helpers where they fit).
- **Issue page actions** (`components/external-issues/ExternalIssueDetailPanel.tsx` gains an optional
  `actions` prop — `{id,label,onSelect,disabled?,title?}[]` rendered as an "Action ▾" menu beside the
  title with the ribbon's dropdown pattern — and an optional `onRequestApproval`): `WorkspacePage`
  supplies them only under the alignment layout, for an issue linked to this workspace (matched by
  `locator.repositoryId` + `number` against the loaded links): Implement in this workspace → the
  persona start used by the ribbon with no `actionId`, `persona = phase = workbenchPhase`, `content =
  startMessageFromIssue(issue)` (Base and non-owners: disabled with the ribbon's reason); Fork in own
  workspace → `CreateWorkspaceDialog` with `initialIssueRef = displayKey`; Mark as done / Reopen →
  disabled with the tracker-write reason; Detach from workspace → the Details panel's confirm-then-
  unlink flow (owner only; the primary link's detach warns that the workspace loses its primary
  issue); Import task → the picker. Request approval → `openWorkspaceApprovalsTab(workbenchPhase)`.
- **One chrome row** (added the same day, mock prompt #49): `TopBar` gains a `toolbar` slot
  rendered between the primary-issue chip and the right-hand controls; under the alignment layout
  `WorkspacePage` passes the toolbar content (VM status strip, the hidden `ReposBar`) there and
  renders no `workspace-toolbar` chrome row, saving the row's height above the chevrons. The classic
  layouts keep both rows byte-identical. Tests: `TopBar.test.tsx` (slot inside the title bar, no
  wrapper without content), `WorkspacePageAlignment.test.tsx` (strip inside the title bar under
  `alignment`, separate chrome row under `both`), `WorkspacePageV2.test.tsx` unchanged.
- **Rail toggle on the rail** (added the same day after design research): under the alignment
  layout the title bar renders no sidebar toggle (`TopBar` `showSidebarToggle={false}`); the
  Activity row carries "Collapse sidebar" (`<<`) as its trailing header action — the same shape as
  the "+" rows — through a new `LeftSidebar` `onCollapse` prop, and a 28px edge strip with
  "Expand sidebar" (`>>`) replaces the hidden rail. Header-only rows keep a hidden spacer in the
  chevron column so Activity's icon and label align with the section rows. No keyboard shortcut.
  Classic layouts keep the title-bar toggle. Tests: `TopBar.test.tsx`, `TreeNode.sectionOrder.test.tsx`
  (spacer, collapse action does not open Activity, absent without a handler),
  `WorkspacePageAlignment.test.tsx` (collapse → edge → expand; classic unchanged).
- **IE ribbon** (`backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml`): `ie:`
  gains the Author and Analysis groups exactly as `se:` has them, before Release; regenerate
  `frontend/src/lib/workbenchBuiltins.generated.ts` with `UPDATE_WORKBENCH_BUILTINS=1 cargo test -p
  g5-api-core frontend_fallback`; a committed `.g5/workbench/actions.yaml` that names `ie` still
  replaces the whole phase.
- **Deferred inside this phase (V2 backlog):** closing, reopening and creating tracker issues from G5
  — needs provider write operations, the integration broker and the administrator-enabled completion
  mapping (`g5.external-issues.completion`, `g5.external-issues.broker`); the disabled controls name
  that gap.
- Tests: `TreeNode.sectionOrder.test.tsx` (workbench order with Activity first and Jobs; Activity
  activation calls `onOpenActivity` and expands nothing; classic order unchanged and the new ids
  hidden; a stored pre-Phase-8 default migrates); `WorkspaceIssuesSection.test.tsx` (primary first,
  Subtask pill, closed issue checked and disabled with reason, read error row, Import task hidden for
  non-owners, quick-add disabled with reason); `ImportTaskPicker.test.tsx` (linked issues pre-checked,
  count button, sequential links, one conflict shows the notice and the rest succeed, Escape closes);
  `ExternalIssueDetailPanel.test.tsx` (menu renders the supplied actions, disabled reasons);
  `WorkspacePageAlignment.test.tsx` (Activity entry opens the timeline tab; Issues & tasks section
  present only under `alignment`; Implement starts a persona session whose content is the issue
  message; Request approval opens the approvals tab on the current phase; Detach unlinks after
  confirmation); `ActionRibbon.test.tsx` IE expectations; `api_workbench.rs` / `workbench_config.rs`
  group counts; `e2e/flows/alignment-shell.spec.ts` asserts the rail order and that Activity lands
  on the timeline (no issue-domain fixture exists, so the issue flows stay in vitest).

### Phase 9 — Timeline filters: every activity kind, checkboxes on top (≈0.5 day)

Added 2026-10-09 at the human's request on finding C5 of the prompt comparison
(`g5.workbench.landing` amended in place). Frontend only; no new route, store, flag, setting or
dependency.

- **What already holds:** `WorkspaceActivityPanel` (the `workspace_activity` center tab) already
  interleaves the primary issue's GitHub discussion with every workspace context kind and carries
  the comment composer; the backend `ActivityEntryBody` and the frontend `ActivityEntryDTO` union
  name the same ten kinds (`issue_opened`, `issue_comment`, `issue_event`, `chat`, `job`, `task`,
  `commit`, `approval_decision`, `phase_changed`, `pull_request`) and the panel's switch renders
  each; nothing is dropped. The Phase 1 wording "references, never duplicates" meant the live
  tracker read with no stored copy, which is what the code does; the governing table now says so.
- **Filter row** (`lib/activityFilters.ts`, `WorkspaceActivityPanel.tsx`): one labelled checkbox
  per group — Issue discussion (`issue_opened`, `issue_comment`, `issue_event`), Conversations
  (`chat`), Jobs & tasks (`job`, `task`), Commits, Approvals, Phase changes, Pull requests — with
  the group's count over the loaded pages (a group with no loaded entry still renders, count 0),
  stable ids `activity-filter-<group>`, and an "All" reset (`activity-filter-all`) shown only while
  some group is off. Default: all checked. The row renders below the `header` slot and outside the
  feed's scroller, so it stays in view while the feed opens scrolled to its newest entries, above
  "Load earlier". The panel is the same component under every layout, so the row renders under
  every layout, as Phase 1's durable kinds do.
- **Filtering** is client-side over the loaded pages: the activity API has no kind filter and gains
  none; a hidden kind stays loaded and is never re-queried. The composer and the viewer's pending
  comments are shown whenever Issue discussion is checked. When the filters hide every loaded
  entry the feed says so (`workspace-activity-all-filtered`) and "Load earlier" stays available.
- **Persistence:** the HIDDEN group ids at `localStorage` `g5:activityFilters:<userId>` through the
  shared tolerant list store (`lib/storedList.ts`, the open-tab idiom): tolerant read, non-fatal
  write, a new group defaults to shown. Without a user id the selection is component state only.
  Per-person browser state like the open-tab list — not a platform record.
- Tests: `WorkspaceActivityPanel.test.tsx` "filter row" (every group checked with its count;
  unchecking hides its rows and keeps the others; All resets; the all-hidden message with Load
  earlier; the stored selection restored for the same person and not another; unreadable storage
  tolerated; composer and pending comments follow Issue discussion; the row sits below the header
  slot and above the scrolling feed); `WorkspacePageAlignment.test.tsx` landing case (row under
  the real `TimelineStatusHeader`, all seven checked); `e2e/flows/alignment-shell.spec.ts` (seven
  checked, unchecking Phase changes hides "Moved to System Engineering", re-checking shows it —
  written, not run here).

### Phase 10 — No title bar: global controls in the strip, workspace controls in the ribbon (≈1 day)

Added 2026-10-11 at the human's request after the mock review (`g5.workbench.shell-chrome`).
Frontend only; alignment layout only; no new route, store, flag or setting. The classic
layouts keep `TopBar` and their toolbar row untouched.

| Control (old title bar) | New home | Why there |
|---|---|---|
| Workers, Notifications, Report, Settings, Account | Right end of the tab strip, small icons, on every shell page | The strip is the one bar every page shares; the page supplies only the handlers it has |
| Owner + Transfer select | Ribbon right end: user-with-key button "Owner: name"; menu "Transfer ownership" with inline confirm | Workspace-bound; same shape as the actions |
| VM pill + Start / Stop | Ribbon right end: one button, label by status | One control for one lifecycle action (`g5.workspace.ui.gates`) |
| Workspace details (info) | Ribbon right end | Human instruction 2026-10-11 ("move workspace info down in the bar") |
| Chat toggle (center strip hidden) | Ribbon right end | Keeps `g5.workspace.ui.center-tabs`'s one-click promise |
| Switcher, Fork | Home directory tile; "+" picker | Discovery and fork belong to the pages that list workspaces |
| Terminal shortcut | Rail: the Files header's trailing action (owner; disabled with the reason while the VM is stopped) | The human dropped the bar icon for icon count; the review found no other click path to the Terminal tab |
| Activity, primary chip, avatars, sidebar toggle | Dropped | Rail owns Activity and lists the primary issue; the rail owns its collapse |

- **Owners.** `AppShell` gains a `shellControls` slot that `WorkspaceTabStrip` renders at its
  right end (`components/layout/ShellControls.tsx` is the shared icon row; `NotificationBell`
  moves out of `TopBar.tsx` so both bars share it). `ActionRibbon` gains a `trailing` slot and
  exports its button shape, so `components/workbench/WorkbenchWorkspaceControls.tsx` (owner menu,
  VM button, Details, Chat) renders in the person's action-bar style without a second style
  table. `WorkspacePage` under alignment renders no `TopBar`, passes the strip its handlers and
  the ribbon its controls, and renders the ribbon for Base too (no groups, controls only).
  `WorkspaceVmStatusStrip` keeps the classic and mobile rows; the VM state table and the
  start/stop admission move to `lib/workspaceVmControls.ts` so both read one rule.
- **Discovery.** `lib/workspaceTree.ts` gains `filterWorkspaceTreeRows` (name or branch match,
  ancestors kept, the switcher's matcher reused). `components/workspace/WorkspaceDirectory.tsx`
  is the shared `ui/SearchField` plus shared `WorkspaceLineageRow`s with Open / Fork / Remove;
  like the switcher it lists other people's review copies nowhere and leaves a modified click
  to the browser. Home renders it as the first alignment tile with New workspace, through the
  dialogs Home already owns for its switcher, and renders `OrganizationHomeTopBar` only under
  the classic layouts. `ForkPickerPage` reads the same filter over every organization through
  the same field and rows, omits review copies (never forkable), keeps per-organization Fork,
  and keeps its own header only while the strip is absent (a direct visit under a classic
  layout).
- **Shared pieces** (independent review 2026-10-11, duplication pass): `ActionRibbon` exports
  `RibbonShapedButton`, the one button the actions and the workspace controls render through;
  `hooks/useDismissOnOutside.ts` is the one outside-click / Escape rule for the ribbon's menus
  and the owner menu; `reviewWorkspace.ts` reads the VM labels from `lib/workspaceVmControls.ts`.
  `lib/workspaceOwner.ts` names the owner and the transfer candidates, and
  `terminalControlState` gates the Terminal, for both the strip and the alignment surfaces, and
  the Start admission reads the live gate's `liveGateCanStart`; a keyboard dismissal of the owner
  menu returns focus to its trigger; `ui/SearchField` serves the switcher too; `createMockWorkspaceVm` is the
  one VM fixture (the msw handler builds on it). Not taken: the strip's small icon buttons do not build on `IconButton` (its
  `className` replaces the base classes, so a size variant would be a primitive change beyond
  this scope).
- **Not changed.** The mobile header, and Base on a phone (the one workspace the desktop
  path serves there) keeps its title bar; `TopBar` under classic layouts (its `toolbar` and
  `showSidebarToggle` props are removed as dead); the fork picker gets no per-organization
  New workspace (Home's dialog is organization-bound; left to a later ask).
- Tests: `WorkbenchWorkspaceControls.test.tsx` (labels by status, admission per role and Base,
  transfer menu → confirm → callback, error line, Chat toggle); `ShellControls.test.tsx`;
  `lib/workspaceVmControls.test.ts`, `lib/workspaceTree.test.ts` (filter keeps ancestors);
  `WorkspaceTabStrip.test.tsx` (trailing slot); `ForkPickerPage.test.tsx` (search narrows every
  organization); `HomePage.test.tsx` (alignment: no top bar, directory search, Open / Fork /
  Remove; classic: top bar unchanged); `WorkspacePageAlignment.test.tsx` (no title bar, strip
  controls open their surfaces, ribbon controls incl. Base, classic row unchanged);
  `e2e/flows/alignment-shell.spec.ts` (title bar absent, Start/Stop button, strip Settings
  opens the settings page — run by CI with the alignment gate on).

### V2 backlog (not in this plan's scope)

Homepage chat; org Issues tab; issue quick-add and Done / Reopen from G5 (need provider write
operations, the integration broker and the completion mapping — Phase 8 shows the controls
disabled); per-org issue views; Summary view with AI impact; ontology
statement review with approval invalidation; Lab menu (cost estimator, ontology graph, formal
specification); IE Check new dependencies / Data migration plan / Deployment plan / Integration
tests as templates once the team decides which to keep; verification seal; running compute tile;
meeting recording and Google Drive import; per-workspace token cost on the compute tile; blocking a forward phase move on that phase's
approvals; per-turn persona injection in the provider adapters.

## Acceptance outcomes

- Under the alignment layout, a fork of Base opens on its timeline in Business Engineering and a
  fork of an ordinary workspace opens in its source's phase; chevrons move the phase, the move
  persists, appears on the timeline and is visible to readers.
- Every V1 ribbon action either starts a chat whose first message is its template under the phase
  persona or opens the existing surface; no button is inert.
- The API refuses a target-advancing merge — direct and PR, before any provider call — while any
  IE-bound requirement is unsatisfied or stale, and allows it with BE/SE requirements pending; every
  shipping surface reflects the same state.
- A transcript becomes decision cards whose statuses are durable Git content.
- Under the alignment layout every open workspace is a tab beside a pinned Home; "+" forks any
  workspace of any organization into a new tab; tabs survive reload per browser and user.
- Under the alignment layout the rail reads Activity · Project docs · Ontology · Files · Issues &
  tasks · Jobs; Activity lands on the timeline; Issues & tasks lists the workspace's links primary
  first, imports backlog issues as subtasks, and an issue's page can implement, fork, detach or
  request approval; every IE ribbon carries New doc, Import, Analyze and Review meeting; the Done
  and quick-add controls are visible, disabled and say why.
- With the feature off, or another layout selected, the layout-gated surfaces are unchanged
  (existing Playwright flows pass unmodified) — except the Phase 3 governance cutover, which is
  global: stage controls disappear and phase-bound approvals apply under every layout.

## Risks and unresolved assumptions

- **Persona per turn** — V1 prepends persona text to the first message only; a long session may
  drift from the persona. Accepted for Friday; adapter injection is the follow-up.
- **Activity feed extension** — `ActivityEntryBody` is a closed enum mirrored in the frontend union;
  each new variant touches both plus `WorkspaceActivityPanel`'s switch. Bounded, but not free.
- **Fingerprint break in Phase 3** — settlement fingerprints change for every phase; no in-flight
  approval request survives the cutover. Acceptable because stage approvals are dropped outright.
- **Templates in Git** — a fork copies the source's `.g5/workbench/`, so Base content becomes the
  org default; confirm that is desired rather than an org-level record.
- **Friday 2026-10-09 target** — Phases 0–3 sum to ≈6.5 implementer-days sequentially; Phase 0 +
  Phase 3 alone are ≈4. The slice under "Phases" is the recommendation; it needs the human's
  confirmation and at least two implementers to hold the date.
- **Durable-path change** — adding `.g5/workbench/` and `.g5/meetings/` to the allow-list touches
  staging, commit preparation and Files visibility for every workspace; it is small but global.
- **Fingerprint and request cutover** — pending approval requests are superseded at the Phase 3
  migration; anyone mid-request re-requests.

- **Home's workspace dropdown (Phase 7)** — mock story B3 removes it; the plan keeps it because
  nothing else lists non-recent, non-open workspaces. Flip by dropping `WorkspaceSwitcher` from
  `OrganizationHomeTopBar` under the layout if the fork picker is judged enough.
- **One mounted page (Phase 7)** — switching tabs navigates; in-progress composer text and
  scroll positions follow the page's existing persistence, not the mock's iframe isolation.

## Review summary

Reviewed by independent read-only Codex runs on the current files (2026-10-06); each run's verdict
is recorded here: run 1 CHANGES_NEEDED (2 blockers), run 2 CHANGES_NEEDED (3 blockers), run 3
CHANGES_NEEDED (3 blockers), run 4 CHANGES_NEEDED with **no blockers** (11 majors, folded in). The
loop stopped at "no blockers" per the human's instruction.

**Phase 0 implementation reviewed** (self-review plus an independent Codex code review, see
[ontology-review.md](ontology-review.md)): one race fixed by the reviewer, three majors and four
minors fixed by the implementer in `a1c1f1b1`, one pre-existing lock-upgrade pattern recorded as
debt; the Playwright flow remains unexecuted until the e2e stack runs with
`G5_ALIGNMENT_UI_ENABLED=true`. Remaining uncertainty: whether the Base-derived
`.g5/workbench/` defaults should instead be an org record; whether blocking a forward phase move on
that phase's approvals is wanted later; and the Friday scope cut above. Reuse assumptions to verify
during coding: the `explainer_templates` loader and `resolve_branch_committed_view` shape, the
`ActivityEntryBody` union, `PrepareToShipModal`'s prop set, `useSaveDocContent` semantics, the
`chat_service` fork copy and the merge admission function's call sites.

**Phase 1 implemented:** `381ed842..2e4dafdc` on `ux-v2/phase-1-timeline` (feed variants
`PhaseChanged`/`PullRequest` with API tests, `TimelineStatusHeader` without approval state,
timeline as the pinned landing tab under `alignment`; classic layouts unchanged — vitest
4328/4329 with the known `ChatPanel` order-dependent failure; Playwright flow extended but
unexecuted, no e2e stack here). Deviations from the phase text: `WorkspacePlanOverview` was
already off the landing path (restored tabs exclude it), so nothing was removed; the
"Business Engineering · Gate 1 not requested" header text waits for Phase 3 — the header
reads "Business Engineering" (Base: "· read-only"); the README fallback uses the existing
file read (`GET /workspaces/:id/repos/:repoId/files/README.md`, live or committed), not a
new route.

**Phase 2 implemented:** `7cec9c77..8e7219bc` plus the metadata commit on
`ux-v2/phase-2-actions` (durable `.g5/workbench/` root; `GET /workspaces/:id/workbench` with
built-ins in `g5-api-core/src/services/workbench_builtins/` and the override/invalid loader
`workbench_config.rs`; `chat_sessions.persona`/`persona_instructions` (migration 060) with the
persona start through the messenger's `claim` → resolve → `send_claimed` split, the delivery
bootstrap in `agent_dispatch.rs`, and the follow-view fork through `MessageTarget::ForkSession`;
the live ribbon, persona pill and fixed preview entry in the frontend). Checks run: `cargo fmt
--all --check`, `cargo clippy` on the five touched crates (clean), `cargo test -p g5-api` for
`api_workbench` (6), `api_workbench_chat` (7), `api_workspace_phase`, `api_workspace_timeline`,
`api_chat_*`, `api_explainer_templates`, `api_task_lifecycle`, `api_workspace_fork_names`,
`route_auth_static` (all green), g5-contracts/g5-scm/g5-api-core unit tests; frontend `tsc`,
`eslint` (0 errors), full `vitest run` 4418/4419 (the known order-dependent `ChatPanel`
"no-longer-visible account" failure passes alone). Unexecuted: the Playwright flow
(`alignment-shell.spec.ts`, no e2e stack here) and any live-provider receipt of the persona
prefix. Deviations from the phase text: menus are a third action `kind: menu` with `items`
(the schema listed only `chat | open`; a menu button executes nothing itself); the start DTO
names the context `workbenchContext` because `context` already carries the message context;
`actionId` is kept on the wire but the Rust field is `workbench_action` (the route-auth audit
treats `*_id` as record identifiers); the stale-revision refusal is 409 `CONFLICT` with
`details.code = WORKBENCH_CONFIG_STALE` (no new error variant); the follow-view fork requires
no `configRevision` (optional, checked when sent) and queues one persona-introduction input
rather than an empty turn; a workspace without a docs repository reads `.g5/workbench/` from
its first repository; Approvals opens the tab unfiltered until Phase 3. Prompt and persona
texts are first drafts for Zhengchun in `workbench_builtins/actions.yaml` and
`workbench_builtins/personas/{be,se,ie}.md`.
Phase 2 reviewed (self + independent Codex): one blocker (persona-prefix overflow stalling tenant
dispatch) and eight further findings fixed in `e9fea214`; see [ontology-review.md](ontology-review.md).
Open design note: follow view queues an introduction input (one acknowledgement turn per switch); the
frozen instructions + delivery bootstrap alone would also hold the invariant.

**Phase 3 implemented:** `ux-v2/phase-3-approvals` (commits A–E listed in the handoff). Schema:
three files instead of one — `066_approval_phases.sql` (additive DDL), `067_approval_phases_cutover.sql`
(owner-form backfill, closure of pending requests and active runs, stage-column drops; the RLS guard
accepts the owner exemption only for a file made solely of NO FORCE/UPDATE/DROP/FORCE statements) and
`068_approval_phase_contracts.sql` (`workspaces.stages` drop, admission columns, phase-aware
`jobs.approval_context` validator added beside the immutable baseline one, since the restricted
migrator owns tables, not functions). Deviations from the phase text: the cutover reason lives in the
066 header, not in a column; `approval_requests.phase` is nullable and never backfilled because the
request update guard admits only pending→terminal transitions, so superseded history keeps `NULL`;
orphaned agent-review jobs are cancelled by a one-shot startup pass
(`maintenance/approval_phase_cutover.rs`) through the ordinary owner cancel — queued work settles at
once, a running attempt gets the durable cancel intent; the approvals tab's phase filter is page
state (`approvalsPhaseFilter`, set by the ribbon action) with `?phase=` as the deep link; open
pull-request summaries also carry the current admission so the PR list and ship modal read one
verdict; `request_digest` v2 is accepted beside the two v1 shapes (stages 1 and 2) so forks admitted
before the cutover replay. Reuse mismatch found while coding: `TaskService::request_workspace_job_cancel`
cancels queued tasks at once but only marks a running attempt for the reconciler, so the job is
terminal only after delivery — accepted, the run is already cancelled and a late settlement is refused.
Checks run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo test -p g5-api` for `migration_045_owner_cleanup` (11), `migration_066_approval_phases` (1),
`api_approval_phase_cutover` (2), `approval_request_retention` (4), `migration_253_approval_requests`,
`migration_218_agent_approval_review`, `api_org_approvals` (8), `api_workspace_approvals` (19),
`api_agent_approval_evaluation` (4), `api_workspace_phase` (8), `api_home` (13), `api_workspace_reviews`,
`api_workspace_pull_requests`, `api_external_issues`, `api_workspace_vms`, g5-common
`approval_review_contract` (5), g5-api-core approval unit tests; frontend `tsc`, `eslint`, full
`vitest run`. Unexecuted: the Playwright flows (`org-approvals.spec.ts`, `alignment-shell.spec.ts`; no
e2e stack here) and the cutover against a production-shaped database.

**Phase 3 review fixes** (coordinator review plus an independent Codex code review, see
[ontology-review.md](ontology-review.md)): the provider merge now validates the recorded admission's
complete identity — source, target and the source head vector of the cut about to be merged — under
the pull request lock before any provider mutation
(`workspace_pull_request_provider_lifecycle.rs::require_admission_for_provider_merge`; a caller
without a recorded admission admits itself for that exact cut); one ordered approval policy lock
protocol (requirement-set lock, then definition locks by id — documented at
`approval_config_service::lock_target_requirement_set`) is taken by attach, retire, definition
publishing, decisions and withdrawals, request creation and cancellation, agent review settlement
and admission; provider outcomes are reconciled before re-admission (`reconcile_provider_acceptance`,
called by the merge route) and a recorded admission for the same cut stands once any provider
repository accepted (partial acceptance, lost acknowledgement); the legacy direct merge
(`POST /workspaces/:id/merge`, `workspace_service::merge_cloned_workspace_into_parent`) was found
unadmitted and now evaluates the IE requirements under the policy locks against its pinned child cut.
Frontend: a decision, withdrawal, request or cancellation invalidates the source's and target's
pull-request summaries and the timeline; the ship modal reads the freshest server verdict (the later
`evaluatedAt`, else the PR's own); a missing verdict is "not evaluated yet" everywhere (ship, PR
list, approvals tab, home) and never recomputed from the roster; `?phase=` is the only phase-filter
authority (the ribbon writes it through the tab navigation, merged into the `?view=` mirror); the
timeline header shows the current phase's meter, "not requested", loading or unavailable;
`approvalPhases.ts` reuses `workbenchDefaults` for the vocabulary and labels; the Playwright picker
flow opens Advanced first and the alignment flow exercises a refused merge through the ship modal.
Partial acceptance is modelled in the API test by the aggregate a failed sibling leaves
(`provider_status = 'partial'` over a merged provider row), not by a second provider repository.
Checks: `cargo fmt --all --check`; `cargo clippy --no-deps` on the six touched crates (clean; the
workspace-wide run fails on pre-existing `g5d-dispatch/src/modules/runner.rs:631`); `cargo test -p
g5-api` for `migration_045_owner_cleanup` (11), `migration_066_approval_phases` (1, both stage counts,
every active run state), `api_approval_phase_cutover` (4), `api_approval_policy_locks` (3),
`approval_request_retention` (4), `migration_253_approval_requests` (1), `migration_218_agent_approval_review`
(1), `api_org_approvals` (8), `api_workspace_approvals` (19), `api_agent_approval_evaluation` (4),
`api_workspace_phase` (8), `api_home` (13), `api_workspace_reviews` (61), `api_workspace_pull_requests`
(64), `api_external_issues` (33), `api_workspace_vms` (55), `api_task_lifecycle` (60); frontend `tsc`,
`eslint` (0 errors), full `vitest run` 4427/4428 (the known order-dependent `ChatPanel` failure).
Unexecuted: the Playwright flows — no e2e images exist on this machine and `just e2e-up` builds the
whole CI image set; the `g5d` CLI paths were not re-run.

**Phase 5 implemented:** `ux-v2/phase-5-home` (stacked on Phase 3). Frontend only:
`components/workbench/HomePhaseStepper.tsx` (`HomePhaseStepper`, `HomeRowBadges`) rendered by
`pages/HomePage.tsx` on each Recent Workspaces row when `resolveWorkspaceLayout` — the same
resolver as the workspace page, instance features plus the stored user layout — yields `alignment`;
classic layouts render Home unchanged (existing assertions untouched). Stepper: three chips from
`WorkspaceSummary.phase` (done / active in the phase hue / todo), Base read-only; a chip PATCHes
through `useUpdateWorkspacePhase` when the caller owns a non-Base workspace and the target differs,
then navigates — the workspace chevrons reflect the move once the detail refetches. Badges: Working
(`concurrentTasks > 0`, pulsing ring, `motion-reduce:animate-none`) else muted Idle; "Needs user
input · N" counted from the FIRST loaded Home attention page for that workspace, nothing while that
query is loading or errored or the page has no item (never "0", no page walk). Deviations from the
phase text: the list DTO carries no per-phase roster, but Home already loads each visible row's
`GET /workspaces/:id/approvals` for its "approvals complete" signal, so the chips reuse that state
through `phaseApprovalMeter` — no additional per-workspace fetch; Home rows are owner-only
(`ownerUserId === user.id`), so the non-owner "navigate only" branch is guarded in code but
covered by the Base case in tests; a narrowed attention filter narrows the count (positive evidence,
lower bound). No new tokens, routes, flags or settings. Checks: `npx tsc --noEmit -p tsconfig.json`,
`npm run lint` (0 errors), `HomePage.test.tsx` 50/50 (9 new), design-token guards, full `vitest run`
4436/4437 (the known order-dependent `ChatPanel` failure). Unexecuted: `alignment-shell.spec.ts`
(Home stepper flow added; no e2e stack here).

**Phase 5 review fixes** (coordinator review plus an independent Codex code review, see
[ontology-review.md](ontology-review.md)): a phase move is now filed under
`workspacePhaseMutationKey(workspaceId)` and every `useUpdateWorkspacePhase` instance for that
workspace reads the latest move's pending/error through React Query's `useMutationState`, so a
Home-started PATCH shows as pending on the workspace chevrons, its refusal appears as the inline alert
after navigation, and `mutate` refuses a second move while one is in flight (both pages disable their
chips meanwhile) — no new state; the pre-existing hook test that expected an instance re-rendered for
workspace B to report A's refusal now asserts that through an A instance, since B's chevrons must not
show A's error. The Needs-input badge keeps the bounded first-page read and carries `title` /
`aria-description` "N items on the first loaded attention page (filter: X)". Base chips are enabled
navigation controls named "<phase> · read-only" and the meter is the chip's accessible description
(`aria-describedby`). The phase icon map is shared from `components/workbench/phaseIcons.ts`.
Tests added: hook (shared pending/error across instances, overlap refusal), workspace page (Home-started
move pending → settled, refused → alert + rollback, second selection ignored), Home (pending chips
refuse a click, one attention request and the All → Approval count change with its note, `simple` /
`advanced` layouts, instance info loading/errored with a stored `alignment`, Base chip keyboard open and
accessible names/descriptions); the Playwright Home flow waits for the PATCH response and asserts the
"Moved to System Engineering" timeline entry and the phase after reload. Checks: `tsc`, `eslint`
(0 errors), targeted `vitest run` 115/115 (HomePage 57, WorkspacePageAlignment 29, workbench
components 24, hook 5), full `vitest run` 4447/4448 (the known `ChatPanel` failure). Unexecuted: the
Playwright flow (no e2e stack here).

**Phase 6 implemented:** `ux-v2/phase-6-home-tiles` (stacked on Phase 5). Backend: one member
read, `GET /orgs/:id/compute/running` (`routes/compute.rs`, `workspaces:read`), over the Workers-page
inventory query extracted into `worker_host_service::list_active_workspace_vms` with a
`VmInventoryVisibility` of org-wide (workers admins; org owners/admins, as the workspace list grants)
or `ReadableBy(user)` (`visibility = 'org'`, or an invite-only workspace the caller owns or is a member
of — the workspace list's predicate, bound as one nullable parameter so the SQL exists once); the
admin route maps the shared row back onto its unchanged DTO (owner e-mail, no slot), the member DTO
adds `slotCpus` / `slotMemMib` / `instanceType` from the placed `worker_hosts` row (`null` before
placement) and drops the e-mail. Frontend: `components/home/HomeIssuesTile.tsx`, `HomeComputeTile.tsx`,
`homeTileLogic.ts` (pure projections) and `HomeTileParts.tsx` (the Recent/Attention header and row
action classes moved out of `HomePage.tsx` unchanged, so the classic tiles' markup is byte-identical);
`HomePage.tsx` renders attention · issues · recent · compute under `alignment` with the full
`HomeIssueSearch` below (the tile's "View all N issues" scrolls to it and focuses its input), classic
keeps recent · attention and the search as before; `hooks/useHome.ts` `useRunningCompute` (5 s poll
while a VM is starting/stopping, else 30 s, which is also the uptime tick). Reuse mismatches found
while coding: (1) the workspace page has **no stop confirmation** — Stop is a direct `POST
/workspaces/:id/stop` with the "Workspace stopped" / "Could not stop workspace" toasts and no
blocked/accept-work-loss branch in the client (`WorkspacePage.tsx::handleStopWorkspace`), so Shut down
mirrors exactly that; (2) the search API has no page-size parameter, so "top eight" is the client's
slice of one response per filter (sorted by `updatedAt`), and the tile does spend one GitHub search
request per configured domain on each Home visit under `alignment` (30 s stale window, no request
without a domain) — a departure from the full search's "visiting Home never spends the shared quota"
rule that the Phase 6 scope accepted; (3) `Link existing` keeps the Workspace Issues panel's semantics
(primary first, `workspace_has_primary` → "Link as related", `issue_has_primary` names the owner), the
picker lists the caller's owned workspaces in Recent Workspaces order. Label chips narrow to issues
carrying every selected label (GitHub `label:a label:b` semantics). Checks: `cargo fmt --all --check`,
`cargo clippy --no-deps -p g5-api -p g5-api-core -p g5-api-routes --all-targets -D warnings` (clean),
`cargo test -p g5-api --test api_compute_running` (2) `--test api_worker_host_workspaces` (3)
`--test route_auth_static` (12); frontend `tsc`, `eslint` (0 errors), `HomePage.test.tsx` 64/64
(7 new), `homeTileLogic.test.ts` 3/3, full `vitest run` 4455/4458 (the known order-dependent `ChatPanel`
failures and a `WorkspacePageAlignment` wait timeout under full-suite load; run alone,
`WorkspacePageAlignment` is 29/29 and `ChatPanel` shows only the known failure). Unexecuted: the Playwright flow
(Home four tiles; the compute row check skips when the clone's VM is not running — no e2e stack here).

**Phase 6 review fixes** (coordinator review plus an independent Codex code review, see
[ontology-review.md](ontology-review.md)): the stop toast now reads the stop route's outcome through
`lib/workspaceStopToast.ts::describeStopOutcome` on both surfaces — `stopping` ⇒ "Shutting down
<name>…" with no flush claim, `stopped` with a flush id ⇒ "The latest flush was recorded.", `stopped`
without one ⇒ "The VM is stopped." (no loss notice: the human declined one under
`g5.lifecycle.deadline.loss`); the workspace page's unconditional "latest flush" toast is corrected
through the same helper. The compute row keeps Shut down disabled from the click until the inventory
drops the row. The Issues tile makes ONE search (no server assignee filter, 5-minute stale window,
explicit Refresh) and derives Assigned to me (by the connected GitHub login, the same account
`assignee=me` uses) and Unassigned on the client as a lower bound over the loaded slice; selected
labels stay as chips with Clear across filter switches; domain failures inside a successful response
are told apart as unavailable (every domain failed: notices and retry), partial (notices above the
rows) and empty; the link conflict presentation moved into the shared
`external-issues/IssueLinkConflictNotice.tsx` (the Workspace Issues panel and the tile both offer
"related" for `workspace_has_primary` and `issue_has_primary`, disabled while a link is pending);
"View all" runs the full search under the tile's assignee (`?issues=`) and labels (`label:`
qualifiers), reusing the cached slice when they match; slot memory is labelled GiB (16384 MiB = 16
GiB). Playwright: the alignment flow starts the clone, waits for `running`, shuts it down from the
compute tile, asserts "Shutting down…", the row leaving, the VM `stopped` and the row absent after
reload; a browser case for Link existing / Start workspace was not added — no e2e flow or fixture
covers issue domains (the stack has no GitHub App), so the vitest coverage stands for those paths.
Checks: `tsc`, `eslint` (0 errors), `HomePage.test.tsx` 69/69 (12 Phase 6), `homeTileLogic.test.ts`
4/4, `components/external-issues` suites, full `vitest run` (totals in the handoff).

**Phase 7 implemented:** `ux-v2/tab-shell` off `origin/main` `8e5f0726`. Frontend only: tab state
`lib/workspaceTabs.ts` over the new shared `lib/storedList.ts`; provider `hooks/useWorkspaceTabs.tsx`;
strip `components/layout/WorkspaceTabStrip.tsx` in `AppShell`; picker `pages/ForkPickerPage.tsx` at
`/fork`; route classification `lib/shellRoutes.ts` (moved out of `App.tsx`); shared helpers extracted
on review — `hooks/useWorkspaceLayout.ts`, `hooks/useSwitchOrgThen.ts`, `lib/workspacePagePath.ts`,
`lib/orgCreation.ts`, `lib/workspaceTree.ts` (`baseFirst` opt-in). Consequential discovery: the
workspace page restored persisted center tabs only for the first workspace of a mounted page and
cleared them on every later switch, which contradicted "a workspace restores its own center tabs when
shown again"; it now restores the destination's own record on every switch (per-workspace keys keep
ids from leaking), under every layout. Two independent Codex runs (correctness, duplication) returned
CHANGES_NEEDED; all 16 findings were verified and fixed (see
[ontology-review.md](ontology-review.md) Phase 7). Checks: `tsc` clean, `npm run lint` clean,
prettier clean on every touched file that was clean on main; full `npx vitest run` 4355/4356 (the
known `ChatPanel` case). The Playwright tab flow is unexecuted here. A fresh independent Codex review
of the pushed PR found two P2 navigation defects (close guard not cleared on Home; static routes
excluded by prefix), both fixed with regression tests in the follow-up commit.

**Phase 8 implemented:** `ux-v2/rail-ribbon-issues-panel` off `origin/main` `d5b94253`. Frontend plus the
built-in YAML: rail ids `activity` / `issues` in both canonical orders (hidden under `default`, outgoing
defaults migrate), `WorkspaceIssuesSection.tsx`, `ImportTaskPicker.tsx`, the shared
`WorkspaceIssueLinkParts.tsx` (read error, unlink confirmation) also used by the Details panel, the
`actions` / `onRequestApproval` props on `ExternalIssueDetailPanel.tsx`, the page wiring in
`WorkspacePage.tsx`, IE's Author and Analysis groups in `workbench_builtins/actions.yaml` with the
regenerated fallback. The implementation was drafted by a Codex run from this plan and then reviewed
line by line by the coordinator, who fixed two findings: the sidebar `issuesSection` object and the
`issuePageControls` callback were recreated every render, defeating `LeftSidebar`'s and `CenterPanel`'s
memoisation (now `useMemo` / `useCallback`), and the picker's button read "Import 1 tasks". The human
declined an independent Codex review for this PR. Checks: `tsc`, `eslint` (0 errors), prettier on
every touched file that was clean on main, targeted `vitest run` 252/252 (external-issues suites,
`TreeNode.sectionOrder`, `Sidebar`, `ActionRibbon`, `workbenchDefaults`, `WorkspacePageAlignment`,
design-token and button-test-id guards), full `vitest run` 4408/4409 (the known order-dependent `ChatPanel` account case), `cargo fmt --all --check`,
`cargo test -p g5-api-core workbench_config` (8) and `frontend_fallback` (drift check). Unexecuted:
`api_workbench.rs` (no local Postgres on this machine; it compiles and runs in CI) and the Playwright
rail assertions (no e2e stack here). Codex's own run record is the
[Phase 8 self-review](ontology-review.md#phase-8--implementation-self-review-2026-10-09).

**Phase 4 implemented (2026-10-12):** `ux-v2/phase-4-meeting` off `origin/main` `ae98ca1b`, after
the re-plan above. Ribbon data (`review-meeting` menu with Upload transcript / Extract decisions /
Apply accepted decisions in the shared Analysis group of all three phases; regenerated client
fallback), the modal's transcript mode, the page's upload-then-extract chaining with the
transcript path as the `current_doc` context, the ribbon enabling `meeting`, and the fake
provider's `meeting_extract` fixture for the e2e lane. Checks run in the worktree: `cargo fmt`,
`cargo clippy -D warnings` on g5-api-core, g5-api-foundation and fake-provider-cli (clean),
`workbench_config` unit tests (8, including the menu shape and the generated fallback),
`fake-provider-cli` meeting fixture tests (2); frontend `tsc` clean, eslint 0 errors on the
touched files, `ActionRibbon` / `workbenchDefaults` / `workbenchMessage` suites,
`CreateProjectDocModal` (28, two new transcript cases) and `WorkspacePageAlignment` (61, one new
meeting flow). Unexecuted: the Playwright meeting flow in `alignment-shell.spec.ts` (no e2e
stack here; it needs the alignment gate, a VM and the fake provider). Deviations from the phase
text: none; one bounded fix beside it — the `current_doc` context used to carry a repository
doc's `overlay:` tab id, which the agent cannot read, and now carries the decoded repository
path. Finding to watch on the first e2e run: the fixture writes the decisions document through
the VM's `project_docs` alias like the approval-review fixture and does not commit, so the
owner's live view shows the cards while a reader would see them only after a push. Two
independent Codex reviews (correctness, duplication) on the first commit returned
CHANGES_NEEDED; their findings — transcript rules leaking into a later sidebar upload, several
transcripts uploading without starting extraction, Apply's plain listing dropping the context,
set ids past the 64-character limit, the alias containment check, a fence a cue could close, and
six reuse points (`findWorkbenchAction` from `journeySteps.ts`, one g5d runner and one Project
Docs resolver in the fake provider, shared e2e doc lookup, the doc fixture factory, the g5d stub
helper) — were fixed in the second commit. A third independent review of that result found one
MAJOR and three MINORs: the three minors (stable question ids in the fixture so a re-extraction
edits instead of appending, a fence computation that could overflow the stack on a subtitle
with very many backtick runs, the fixture's slug trimming diverging from the template's rule)
are fixed in the third commit; the MAJOR — the e2e flow does not exercise the durable reader
path because the fixture writes the decisions document without committing — is left open on
purpose: the fake provider's native-git publication needs an assigned task's `g5d task context`
destination, which a chat-started extraction has no access to, so modelling the agent's commit
there is a separate change with its own evidence. Until then the e2e proves the owner's flow and
the reader path rests on the store's committed-read guarantee.

**Phase 9 implemented:** `ux-v2/activity-filters` stacked on `ux-v2/rail-ribbon-issues-panel`
(PR #406). Frontend only: `lib/activityFilters.ts` (seven groups over the ten kinds, hidden-set
storage at `g5:activityFilters:<userId>` through `lib/storedList.ts`) and the filter row, client-side
filtering, all-hidden message and composer gating in `WorkspaceActivityPanel.tsx`. Kind parity checked
by reading both sides: `ActivityEntryBody` (`workspace_activity.rs`, `tag = "kind"`, snake_case) and
`ActivityEntryDTO` (`lib/types.ts`) name the same ten kinds and the panel's switch renders each — no
mismatch, no backend change. Deviation from the phase brief: the row renders outside the feed's
scroller (below the `header` slot, above the scroller that holds "Load earlier") rather than inside
it, because the feed opens scrolled to its newest entries and a row inside the scroller would start
out of view; and the stored selection is re-read when the signed-in user id resolves after the first
render (state adjusted on the prop change, no effect), since the auth context resolves the person
after mount. Checks (from `frontend/`): `npx tsc --noEmit -p tsconfig.json` clean; `npm run lint`
0 errors (546 pre-existing warnings); `npx prettier --check` clean on the new `lib/activityFilters.ts`
and on `e2e/flows/alignment-shell.spec.ts` (the only touched file that is clean on `origin/main`;
`WorkspaceActivityPanel.tsx`, its test, `WorkspacePageAlignment.test.tsx` and
`TimelineStatusHeader.test.tsx` are unclean on `origin/main` and on this branch, so their hunks were
hand-formatted and no `--write` was run); targeted `npx vitest run` 102/102 (`WorkspaceActivityPanel`
18 — 6 new, `WorkspacePageAlignment` 43, `TimelineStatusHeader` 10, `design-tokens` 19,
`design-token-contrast` 5, `button-testid-coverage` 7); full `npx vitest run` 4414/4415 — the one
failure is the known order-dependent `ChatPanel.test.tsx` "shows a no-longer-visible account as
Unavailable…" case; `git diff --check` clean. Unexecuted: the Playwright assertions in
`alignment-shell.spec.ts` (written; no e2e stack here). `TimelineStatusHeader.test.tsx` was not
changed: the header-above-filters check lives in the panel test (header slot) and in the alignment
landing case (the real header).
