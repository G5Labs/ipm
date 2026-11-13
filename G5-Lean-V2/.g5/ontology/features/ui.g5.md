# UI

<a id="g5.ui.terminology"></a>
[I · g5.ui.terminology]

Use consistent terminology for concepts and entities everywhere in the platform.  Don't invent terminology that would confuse a semi-technical user.  

<a id="g5.ui.accessibility"></a>
[I · g5.ui.accessibility]

Follow accessibility guidelines so the interface works for everyone  

<a id="g5.ui.unavailable-actions"></a>
[I · g5.ui.unavailable-actions]

Disable unavailable actions and explain why they are unavailable and how the user can enable them, using a tooltip or similar help.

<a id="g5.ui.workspace-lifecycle-state"></a>
[D · g5.ui.workspace-lifecycle-state]

Start, Stop and ownership-transfer request state belongs to the submitted workspace.
While a request is pending, that workspace's lifecycle controls stay busy across
navigation away and back, even before VM polling catches up; other eligible
workspaces remain independently controllable. Concurrent workspaces retain separate
pending, error and result state in either completion order. Completion refreshes the
originating workspace, and a late response never appears as another workspace's
notification or action state. Ribbon, banner and mobile controls share this rule.
This browser state does not replace [Start consent](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start),
[Stop](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop),
[transfer authority](workspaces.g5.md#g5.workspace.transfer.formal) or server capacity admission.

<a id="g5.ui.capacity-wait-notice"></a>
[D · g5.ui.capacity-wait-notice]

When a workspace start waits for capacity, the workspace page tells the owner with
a pop-up notice that carries the reason. The notice appears once per Start,
whether the Start came from the owner, a Start and send, an agent or a page opened
on a waiting workspace. While the wait lasts, the current reason stays in the
workspace state banner and in the desktop Waiting button's tooltip; when the
reason changes, those update and no new notice appears. The reason is never shown
in the workspace controls row, on desktop or on a phone, so the buttons there do
not move when a wait begins or ends. The composer and the Home personal
panel keep their own [inline progress](chat.g5.md#g5.chat.start-and-send).
Reason: the inline message reflowed the owner, Details and Chat controls whenever
it appeared or cleared ([G5-Lean#983](https://github.com/G5Labs/G5-Lean/issues/983),
owner request, 2026-10-30).

<a id="g5.ui.api-parity"></a>
[I · g5.ui.api-parity]

Make almost every UI action available to chat agents through the same backend API, adding commands to `g5d` where needed. Keep an explicit catalog of UI actions, each marked agent-callable or human-only under [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) and the [human exceptions](../concepts/security.g5.md#g5.security.agent-human-exceptions). A catalogued agent-callable action without a `g5d` command is not implemented.

- Agent-callable: Start, Stop, fork, deleting another workspace in scope (never [its own](workspaces.g5.md#g5.security.agent-workspace-delete)), [managed preview](../software_architecture/workspace_vm.g5.md#g5.preview.agent-control), comments, and requesting or cancelling an approval.
- Human-only: recording an approval decision, turning on merge when ready, merging with an override, marking a [guided step](#g5.ui.journey-evidence), and starting a [Base re-verification run](ontology_and_harness.g5.md#g5.reverification.run).

<a id="g5.ui.launched-chat"></a>
[I · g5.ui.launched-chat]

A chat started by a UI action behaves like any other chat and clearly identifies where it was launched.

It shows a `Started from <action> in <phase>` label naming the action the person clicked and the phase it was clicked in; the label changes no behaviour of the chat.

## Workbench and guided work

<a id="g5.ui.workbench-shell"></a>
[D · g5.ui.workbench-shell]

The workbench is the only workspace shell: phase chevrons and the per-phase action ribbon with the workspace controls at its right end, the tab strip carrying the global controls, and the left sidebar, whose collapse control exists only in its full (stacked) mode, at the top; the icon rails have none. There is no other workspace header layout, no "Workspace panels" setting, no status band, no separate action strip or desktop title bar, no classic organization Home, and no instance feature or environment default that selects between shells. The phone layout is responsive, not a choice: a phone shows the chat-focused layout, and Base, the one workspace a phone renders through the desktop path, keeps its title bar because the strip and ribbon controls are a desktop arrangement. Instance features still gate optional sections (the Guided section's [toggle](#g5.ui.guided-toggle) and kill-switch), never the shell. Reason: user instruction (2026-10-23) to remove every UI that is not the alignment one, after the alignment shell had been the default in dev, QA and test since 2026-10-09.

<a id="g5.ui.workbench-config"></a>
[D · g5.ui.workbench-config]

Load actions and personas from the same committed configuration version. A phase’s explicit action list replaces inherited actions, even when empty; use matching defaults only for omitted behavior. If configuration is invalid, show the error and use the complete built-in fallback. A chat action's expanded prompt must come from the version and phase action the user clicked, even if another phase has the same action ID; the persona instructions of its [seeded start](#g5.workbench.chat-actions) come from the configuration current when the person sends, since the sent text is theirs and the browser already expanded it.

<a id="g5.workbench.chat-actions"></a>
[D · g5.workbench.chat-actions]

A ribbon or Guided chat action never sends on its own. It seeds the chat composer with its complete expanded prompt (the template with its document or phase context, as the server would have expanded it) and Send pulses: in the open chat when that chat already carries the action's persona, otherwise in a fresh chat. The person's send is the request, edits included.

- **Seeded start.** From a fresh chat the send starts the session under the action's persona and the configuration version loaded when the person sends (a stale version is refused once, and the next Send carries the reloaded one), and the Guided step reads Running from that accepted send as it did from an immediate start.
- **Offered while the composer can send.** A chat action is offered only while the composer can send, greyed with the composer's own reason (a pending approval) otherwise; a stopped or failed VM does not grey it, since the seeded composer offers [Start and send](chat.g5.md#g5.chat.start-and-send).
- **One owner.** A seed belongs to the workspace visit that made it and to the chat it went into, so it never appears in another workspace's composer and a sent seed never returns. Clearing the draft, choosing another chat or leaving the workspace drops the seed with its persona and attached reference and starts nothing; text the seed already put in a draft stays that workspace's own draft, pulsing as the next bullet says until it is sent or emptied, and sending it then starts an ordinary chat.
- **One pulse for every seed.** Every composer seed (Explain in chat, Discuss, the review prompt, the Review meeting extraction) pulses Send the same way until the draft is sent or emptied; reduced motion keeps a static ring.
- **Beside the document.** Discuss on a document's question card, meeting cards included, and a finding's chat action seed a fresh chat in the right-edge Chat panel, so the document stays in view; on a phone Chat takes the center.
- **Implement is not a seed.** The Issues panel's Implement launches from an issue's message, not a prompt template, and still starts at once.

<a id="g5.workbench.review-prompts"></a>
[D · g5.workbench.review-prompts]

Code review and Re-run Verification each seed a fresh chat with a fixed prompt, like every [composer seed](#g5.workbench.chat-actions). While the seeded draft waits, a header sits at the very top of that chat, set apart from the chat below in text no smaller than the draft's, and sending or emptying the draft removes it. The owner tunes the prompts' exact wording.

- **Code review:** every review entry point uses the same prompt: the ribbon, the review banner's Run agent review, and the seed on arrival at a review copy. The prompt asks the agent to find the PR's requirements, fix what falls short in a review-and-fix loop of N iterations, each from a fresh session, and record a short review report in Project Docs. Its header offers "Review in a loop with N iterations" (default 1), says more iterations cover more at the cost of more time and tokens, and rewrites the count in the draft when N changes.
- **Re-run Verification:** its prompt asks the agent to follow the [PR incremental TLA+ reverification guidance](../../../.agents/guidance/TLC/pr_incremental_TLA_reverification.md) to its completion gate, stopping early only when truly blocked. Its header states in bold what formal verification does and that it is very expensive. It is offered on the same terms as Code review, in a writable workspace whose chat can send, and only while the [verification switch](#g5.workbench.review-prompts.switch) is on.

<a id="g5.workbench.review-prompts.switch"></a>
[D · g5.workbench.review-prompts.switch]

The verification switch is an organization-only setting, off by default, that only an organization administrator changes, in Admin's Workbench actions section. It governs Re-run Verification and [Base re-verification](#g5.ui.home-reverification), and nothing else: anyone may still ask an agent for the same work in their own words. While it is off or still unread, every workspace shows Re-run Verification greyed, with a tooltip saying the organization admin has disabled it and where an administrator turns it on.

<a id="g5.workbench.find-gaps"></a>
[D · g5.workbench.find-gaps]

Business Engineering's Find gaps plans before it asks. Its prompt has the agent run [ontology planning](../../../.agents/skills/ontology-planning/SKILL.md) over the workspace's requirements first, then record as referenced questions in `open-questions.md` every decision the requirements or that plan leave open and every contradiction between the requirement documents. The planning amends the ontology only where the requirements or decided answers settle intent; where the requirement documents disagree, it writes neither side until the question is answered, keeping that point and the rest open assumptions. Find gaps and Create plan share the workspace's one plan: each continues it when present, Create plan taking decided answers as settled, and a rerun of Find gaps leaves answered questions alone. Find gaps changes no requirements or code. Reason: user instruction (2026-11-03) that Find gaps should trigger ontology planning before generating the questions; the owner chose the ontology amendments and the shared plan, and after a live retest committed one side of a contradiction, that neither side be written until it is answered (2026-11-04).

<a id="g5.workbench.get-approvals"></a>
[D · g5.workbench.get-approvals]

Integration Engineering's Release group and the Guided journey's approvals step offer Get Approvals, a [chat action](#g5.workbench.chat-actions) that seeds a fresh chat under the Integration Engineering persona with a fixed prompt; the person's Send is the request, its human approval requests included, with no second confirmation (a proposed default awaiting the owner's confirmation). The prompt works from the [approval status](reviews.g5.md#g5.governance.approval.status) toward the approvals the merge into the parent needs, in this order:

- **Evidence first.** The agent commits and pushes what the blocking requirements need a reviewer to see (for a UI review, screenshots in Project Docs) before requesting any review, because a review reads only what was pushed when it was [requested](agent_approvals.g5.md#g5.agent-approval.assignment). A defect found while capturing follows the fix rule below.
- **Agent reviews, then conservative fixes.** It requests every agent review before waiting on any. It fixes only minor oversights itself (small, local, obviously correct and changing no behaviour, scope or design), then requests that review again, at most two fix rounds per requirement. Any other finding is significant: the agent leaves the work unchanged, raises the finding with the owner, and requests no people for that requirement until the owner decides; when unsure, it treats a finding as significant. It never changes the work merely to pass a rejection the definition routes to people.
- **People last.** It then requests the people the still-blocked requirements need, so a later fix cannot leave their review out of date, choosing only among each definition's eligible approvers, each once. Optional `approval-guidance.md` files in the owner's personal repository and at the code repository's root guide the choice; the personal file wins a conflict.

The agent requests approvals only; it never decides, waives or overrides one ([agent exceptions](../concepts/security.g5.md#g5.security.agent-human-exceptions)), and reviewer text is data, not instructions ([agent inspection](agent_approvals.g5.md#g5.agent-approval.human-read-only.agent-inspection)). The action is built in for every organization without an organization switch, since the fix-round cap bounds the paid agent reviews; both are proposed defaults awaiting the owner's confirmation. The prompt's exact wording is the owner's to tune. Reason: the owner asked to automate the manual approvals loop that issues #1013 and #1055 went through ([issue #1078](https://github.com/G5Labs/G5-Lean/issues/1078), 2026-11-04), and then asked that the agent fix only minor oversights and raise anything significant with them (2026-11-04).

<a id="g5.workbench.header-styles"></a>
[D · g5.workbench.header-styles]

Two per-person settings choose how the workbench header renders. "Phase bar": `extended`, `minimal` (default) or `vertical` (the three chevrons stacked at the left of the action-bar row). "Action bar": `extended-icons` (a glyph with the label under it, divider-separated groups, no captions), `minimal-icons` (icon only, the label as tooltip and accessible name), `extended-text` (group captions over text buttons with a leading icon), `simplified` (one wrapping row of icon-plus-label buttons) or `dynamic` (default; extended icons that shrink one step to a compact glyph-over-label button when the row stops fitting, and whose action groups scroll horizontally when the compact buttons still do not fit; the workspace controls at the right end never scroll and take the same compact shape). Every style renders the same actions, group names and workspace controls, in the actions' button shape; the fixed styles wrap rather than scroll and `dynamic` scrolls rather than wraps, so the header height stays stable. Unknown stored values fall back to the defaults. Reason: the user chose the fixed styles on the reviewed mock page (2026-10-09) and asked for an Excel-like dynamic style (2026-10-19) because the fixed styles either grow the header or hide labels, and made `dynamic` the default (2026-10-20).

<a id="g5.workbench.ship-actions"></a>
[D · g5.workbench.ship-actions]

System Engineering's Land group offers Rebase; Rebase opens the existing Rebase Workspace dialog, a [source-only rebase](workspaces.g5.md#g5.workspace.rebase) onto a chosen target that never advances it, and is unavailable, saying so, on Base, on a read-only or review copy, and without a target. Integration Engineering's Release group offers [Re-run Verification](#g5.workbench.review-prompts), then Create PR, [Get Approvals](#g5.workbench.get-approvals) and Merge to parent; Create PR and Merge to parent reach the Prepare-to-Ship surface. Get Approvals follows Create PR because a request policy may need an open pull request. Approvals use the [single workspace control](#g5.workbench.gates), outside these phase groups. The guided journey's rebase step is the last System Engineering step, bound to Rebase; the pull-request, approvals and merge steps stay in Integration Engineering, in that order. Reason: the user moved Create PR to Integration Engineering and added Rebase to System Engineering (2026-10-20).

<a id="g5.workbench.ship-to-parent"></a>
[D · g5.workbench.ship-to-parent]

Prepare to Ship's last step has three controls. **Ship to parent** merges the pull request at once and is unavailable, naming the blocker, while something keeps it from merging (a conflict or stale branch, failing or pending required checks, missing approvals, named exactly `Approval(s) required before merging` without repeating requirements, a repository GitHub reports blocked, or a refusal when merging). A permission denial is shown as its own reason and never as a blocker the agent can clear: it does not claim Ship with agent will resolve it, and for a [member](workspaces.g5.md#g5.governance.pr.source-owner-merge) it names that the parent has no approval requirement. **Ship with agent**, the [agent path](../software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent), is always offered once a pull request is open: secondary on the all-green path, so that path stays one click, and the primary control when something blocks, with the blocker named beside it. It is unavailable, saying why, to someone who cannot start the source workspace's agent. While the owner's VM is stopped or failed it reads **Start and ship with agent**, with a note beneath saying it starts the workspace, turns Merge when ready on and queues the agent's first message ([the compound start](../software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent)); while a Start is under way the note shows its progress. The third control switches [merge when ready](../software_architecture/git.g5.md#g5.git.pr-auto-merge) on and off without an agent and, while on, names who turned it on and why it is still waiting. While only overridable blockers hold the ship (G5 requirements an [administrator may waive](workspaces.g5.md#g5.workspace.merge.admin-override) or repositories [GitHub reports blocked](../software_architecture/git.g5.md#g5.git.provider-rules)), a person holding the authority for each sees a warning-styled secondary **Merge with override** beside Ship to parent, never replacing it, which opens the one [override confirmation](workspaces.g5.md#g5.workspace.merge.admin-override.formal); the pull-request list offers it beside Merge on the same terms. A person whose session cannot override, such as one signed in before sessions recorded how they were issued, is asked to sign in again instead. Reason: the user asked for Ship to parent with the agent as the blocked path, an auto-merge switch (2026-10-26), and then for Ship with agent always (2026-10-26).

<a id="g5.workbench.ship-to-parent.stop-vm"></a>
[D · g5.workbench.ship-to-parent.stop-vm]

Prepare to Ship presents [optional VM shutdown](../software_architecture/git.g5.md#g5.git.pr-auto-merge.stop-vm) as a dedicated **Stop VM after merge** setting row above **Ship to parent** and **Ship with agent**.

- A shared border groups the lightly shaded setting, both buttons and **Merge when ready** below them. Its helper reads “Applies to all merge options below.” and wraps naturally on narrow screens. Ship to parent waits while the choice saves; failed saves show an inline error.
- Settings offers the personal **Stop VM after merge** default in **Execution policy**, before **Idle timeout**, with “Default for all your workspaces. You can change it in Prepare to Ship.” It shows On or Off by default, saves immediately and reports a failed save without changing the stored choice. This personal control is available without owning or selecting the current workspace; Idle timeout remains workspace-specific and owner-only.

<a id="g5.workbench.ship-approvals"></a>
[D · g5.workbench.ship-approvals]

Prepare to Ship's ① Approvals row shows `N approved, M rejected, K pending` across required approval requirements, one compact status icon per requirement in the response's order, a message naming requirements still rejected or pending, and View approvals in place; the other sections keep their layout and logic. All of it comes from the one [shared status response](reviews.g5.md#g5.governance.approval.status). While the dialog is visible it refreshes that response immediately, then every second for 10 seconds and every 2 seconds afterward; while hidden, every 15 seconds. Returning to visibility refreshes immediately and restarts the fast period. Refreshes never overlap, close/unmount stops them, and a transient failure keeps the last good data. Other dialogs and data keep their polling.

<a id="g5.ui.guided-toggle"></a>
[D · g5.ui.guided-toggle]

A per-person setting, offered beside the header styles only while the Guided section can exist, shows or hides the Guided section of the left sidebar. It is off by default, unknown stored values fall back to off, and it never widens the instance feature or the layout rules that gate the section. Reason: user request (2026-10-20) for the setting, on by default; turned off by default on user request ([chat intent](../../chat_intent/davide-siderail/request.md), 2026-11-09).

<a id="g5.ui.guided-collapsed"></a>
[D · g5.ui.guided-collapsed]

The Guided section starts collapsed like every other sidebar section when a workspace is opened or forked, in the stacked panel and in both rail modes, where nothing opens the overlay before the person hovers or activates an icon. Its header opens, collapses and closes it exactly as the other sections' headers do, and no collapse is remembered. Reason: user instruction (2026-10-29) reverting the earlier decision that Guided alone opened by default, with a remembered collapse and a rail intro on each open.

<a id="g5.ui.journey-evidence"></a>
[D · g5.ui.journey-evidence]

Mark a guided step complete only from a workspace result or an explicit human mark. Starting an activity is not completion. Apply a pending mark to the workspace where it was made, even after navigation; clearing it does not erase an actual result. An explicit skip is the owner's third mark: it leaves the step out of the phase's count without completing it, is undone like a mark, and a result still completes a skipped step. Guidance and layout grant no permission to change the workspace.

<a id="g5.ui.guided-panel"></a>
[D · g5.ui.guided-panel]

The Guided section reads as one next thing and honest rows. Its summary is the next step's outcome and the phase's count, with that step's button; no label above it. The active phase lists every step; only the next step is open with its note and buttons, the other rows open on their label, and the finished and upcoming phases fold behind their heading and count. A row's state is a word (Done, Next, Running, Skipped, Saving, Not saved, Unknown), never colour alone; a done row is muted, not struck. The ring before a label is the owner's mark control with a hit area of at least 40px: one press marks the step done, a second undoes it, and a fresh mark keeps an Undo link for a short while; a row also offers Skip. A step whose result proves it has no control. A step reads Running while the page knows a session it started for the step's action still has a running task; running is never completion. A fact that failed to load reads Unknown on the rows it affects and as one sentence with Retry in the summary, never as to do; while the marks load, the section shows flat placeholders at the rows' sizes. A failed mark stays on its row as an alert with Retry. A reader sees no controls and is told who runs each step. Movement uses the shared motion tokens, stays under a quarter second, and keeps only colour and opacity under reduced motion. The section is a navigation landmark with a heading per phase, a list per phase, the next step marked current, hidden state prefixes, few Tab stops with arrow keys between rows, and a polite live region for state changes. Reason: the 2026-10-20 review of the shipped panel against the curated UI skills (contrast of dimmed rows, eyebrow and caps tells, one accent, type scale, hit areas, reduced motion, a running state, mark feedback) and the reviewed Mock U ([design](../../pr_ontology_diff_design/guided-journey/ontology-plan.md), mocks M–U).

<a id="g5.ui.home-attention"></a>
[D · g5.ui.home-attention]

Home shows [actionable approvals](workspaces.g5.md#g5.workspace.approval-human-authority) and open [questions](documents/discussion.g5.md#g5.documents.question-decisions) from the visible workspaces captured for that list; show question attention only for owned workspaces. Each row's name is the link to its target, the document for an open question and the workspace's approvals for an approval, and the row's Answer or Review action goes to the same place. An approval row's secondary line shows only the workspace name; hovering that name reveals its full text even when the visible name is truncated. Each row also shows the owning workspace's owner from the organization roster as the [avatar alone](#g5.ui.home-workspaces), the full name as its tooltip and screen-reader text, falling back to the deterministic initial when no owner resolves. A loaded or filtered page count describes that page. An absent badge, loading state or error must not suggest there is no outstanding work.

<a id="g5.ui.home-attention-order"></a>
[D · g5.ui.home-attention-order]

[Needs Your Attention](#g5.ui.home-attention) is ordered by the "Attention updated" time each row shows: Newest by default for everyone, or Oldest. An item whose time changes, such as a re-requested approval, moves to its new place on the next read. The order covers every outstanding item the list captures, not only the rows on screen: each page, including those Show more adds, continues it. Items with the same time keep one fixed order between reads.

<a id="g5.ui.home-attention-order-reverse"></a>
[D · g5.ui.home-attention-order-reverse]

Oldest is exactly [Newest](#g5.ui.home-attention-order) reversed, items with the same time included, so one fixed tie order serves both.

<a id="g5.ui.home-attention-order-cursor"></a>
[D · g5.ui.home-attention-order-cursor]

A page position from one [order](#g5.ui.home-attention-order) never continues the other: such a read is refused rather than mixing two sequences.

<a id="g5.ui.home-attention-order-api-default"></a>
[D · g5.ui.home-attention-order-api-default]

A read of the attention list that names no order lists Oldest first, as the list did before orders existed, so older clients and their page positions keep their sequence. Home always names its [order](#g5.ui.home-attention-order).

<a id="g5.ui.home-attention-order-control"></a>
[D · g5.ui.home-attention-order-control]

Choose the [order](#g5.ui.home-attention-order) with the same pill dropdown and look as the [Workspaces sort](#g5.ui.home-workspaces), offering Newest and Oldest and showing only the chosen name, on both Home layouts.

<a id="g5.ui.home-attention-order-url"></a>
[D · g5.ui.home-attention-order-url]

Keep the chosen [order](#g5.ui.home-attention-order) in the URL, so a reload or link keeps it and a fresh visit to Home shows Newest.

<a id="g5.ui.home-attention-order-url-default"></a>
[D · g5.ui.home-attention-order-url-default]

The [order's URL state](#g5.ui.home-attention-order-url) omits Newest, as the [Workspaces sort](#g5.ui.home-workspaces) omits its default.

<a id="g5.ui.home-attention-order-filter"></a>
[D · g5.ui.home-attention-order-filter]

The kind filter keeps the chosen [order](#g5.ui.home-attention-order); filter and order are independent.

<a id="g5.ui.home-attention-order-reset"></a>
[D · g5.ui.home-attention-order-reset]

Choosing an [order](#g5.ui.home-attention-order) returns the list to its first page, as the Workspaces tile does.

<a id="g5.ui.home-attention-order-scope"></a>
[D · g5.ui.home-attention-order-scope]

The [order](#g5.ui.home-attention-order) applies to Home only; the notification panel's Needs Attention group keeps its own order.

<a id="g5.ui.home-attention-order-agents"></a>
[D · g5.ui.home-attention-order-agents]

Agents get no command for this list until one is requested; the [order](#g5.ui.home-attention-order) stays a parameter of the same backend read, as [API parity](#g5.ui.api-parity) requires.

<a id="g5.ui.home-workspaces"></a>
[D · g5.ui.home-workspaces]

Home lists the organization's workspaces, except [personal workspaces](personal_workspace.g5.md#g5.personal.home), in one table beside Needs Your Attention, Issues and tasks and Running compute, with no separate directory and no full issue search; the Issues and tasks tile carries a compact search of the organization's open issues whose typed terms reach the tracker after a typing pause or on Enter, never mid-qualifier, and whose clearing restores the termless slice without another request, while the assignee and label filters keep narrowing what was loaded; the search sits above the list beside the label chips, and the tile header keeps its title, filters, Create and Refresh on one line. Home defaults to the caller's own once they have finished the [onboarding tour](#g5.ui.onboarding-tour): Issues and tasks opens on Assigned to me and Running compute on Mine, each listed before All; until then all three tiles open on All, so a newcomer sees the organization's work. The URL omits the person's default, so a link without a filter shows the recipient's default. In Running compute the workspace name is the link to the workspace and Stop, offered to the owner alone, is the row's only control; the tile's header offers Stop my workspaces ([stop by user](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user)) to everyone but viewers, beside its filters. Rows are flat, not a lineage tree; the fork picker keeps the tree ([navigation](#g5.workbench.tabs)). Default to the caller's own workspaces (All before the tour is finished), offer All with other people's review copies omitted, a search over names and branches, and a sort: most recently opened by the caller in this browser (else last updated), last updated, created, name. Keep filter and sort in the URL. Each row names the workspace as its Open link; shows the workspace-wide [approvals](reviews.g5.md#g5.governance.approval.requirements) as stacked requirement counts `N approved, M rejected, K pending` from the [shared status](reviews.g5.md#g5.governance.approval.status), without repeating requirements by their recommended phase, nothing when none is required, and an unavailable state with retry when the read failed; shows the owner from the organization roster as the avatar alone, the full name as its tooltip and screen-reader text, so the name column keeps the width and wraps a long name rather than truncating it, and the column needs no heading; and offers Delete only to someone the [removal rule](workspaces.g5.md#g5.workspace.delete) admits, with Fork beside it; [renaming](workspaces.g5.md#g5.workspace.rename) is offered from Workspace details, not from Home. Phase moves are not offered from Home. A tile too narrow for every column gives up the opened time first and the approvals next, keeping the name, the owner and the actions. A row that is running tasks, and only that row, carries a disclosure to its recent tasks whose control also marks the row as working; it never animates continuously, a change on a listed row is announced once, and the list re-reads on focus and on a timer (quickly while anything runs) because no event reaches Home.

<a id="g5.ui.home-paging"></a>
[D · g5.ui.home-paging]

Every Home tile shows a page of rows and a control for the next page. The page size is a per-person setting, five by default, offered on the settings page; the attention tile passes it to the server, the others page what they loaded.

<a id="g5.ui.home-reverification"></a>
[D · g5.ui.home-reverification]

While the [verification switch](#g5.workbench.review-prompts.switch) is on, Home shows Base's [re-verification status](ontology_and_harness.g5.md#g5.reverification.status) as one line of orange text, not a warning and with no threshold, such as "Base has not been re-verified for 12 merged PRs (+3,410 −1,022 lines)". Beside it, a Re-run verification button opens the [dialog](#g5.ui.home-reverification.dialog) for anyone who can fork Base; anyone else sees it greyed with the reason. While a run is in progress, the button is greyed for everyone and the line reads "Verification in progress in workspace X (by Y)", naming the run's workspace and its owner.

- Until the fork has created the workspace, the line says "a workspace being created" and names the person who started the run.

<a id="g5.ui.home-reverification.dialog"></a>
[D · g5.ui.home-reverification.dialog]

The Re-run verification dialog explains what a run does, that it is very expensive, and that it runs in a new workspace forked from Base, which people merge into Base like any pull request. Beside the status's counts, it lists the pull requests merged since the last completed run and the runs, each newest first. Each of its two choices [starts a run](ontology_and_harness.g5.md#g5.reverification.run).

- A pull request shows its title, number, merge date and who merged it. A run shows its kind, workspace, starter, start date and state.
- **Incrementally re-verify the changes since the last re-verification** starts an incremental run. Until a run has completed, it is unavailable and says why.
- **Re-verify the entire codebase** starts a full run.
- A refused start, such as one that lost a race with another run or came after the switch was turned off, stays in the dialog with its reason.
- Both lists page like Home's tiles. They hold at most the newest 200 pull requests and 50 runs, and say so when there are more.

## Settings pages

<a id="g5.ui.settings-page"></a>
[D · g5.ui.settings-page]

The person's Settings view and the organization's Admin view are each one scrollable sheet with a visible index beside it, or above it on narrow screens. The sheet and its index are centered in the space the rail leaves, with a readable sheet width of at most 1200px; wider viewports add margins rather than stretching controls.

- The index lists rendered sections in their order, highlights the section in view, scrolls to and focuses a heading on click, and records it as `?section=<id>`. A deep link applies to the sheet named by `?view=`; a gated section appears in the index exactly while it exists. The index may link separate Admin pages under their own heading and says where the other scope's settings live.
- The strip's settings icon opens the person's Settings from a workspace, Home or the workspace picker; outside a workspace it lands on Base's settings tab. A section has a heading and description, containing label/control rows or one full-width component.
- Settings orders Appearance, Accounts, Defaults, Editor, Access, Notifications and Execution policy, with Accounts deliberately second. Execution policy offers the [personal merge-stop default](#g5.workbench.ship-to-parent.stop-vm), while workspace-specific controls require a workspace. Notifications holds account-wide [delivery preferences](users.g5.md#g5.user.notifications), including “Approval requests”, rather than the Account flyout.
- Admin orders Organization, Workspaces and hosts, Issues when offered, Workbench actions, Defaults, Accounts and API keys, then links Members, Approval definitions, Workers and GitHub App.
- This layout itself does not change settings scopes, save models, or the workspace Details page. Reason: the owner's 2026-10-26 request to ship the settings-page template from the settings-ownership mocks in [issue #746](https://github.com/G5Labs/G5-Lean/issues/746) before moving settings.

<a id="g5.ui.sign-in-landing"></a>
[D · g5.ui.sign-in-landing]

Signing in, and opening the application root while signed in, lands on the active organization's Home, where the [onboarding tour](#g5.ui.onboarding-tour) starts; a route preserved from before sign-in still wins, and an account without memberships lands on the no-organization page, because only operators [create organizations](organizations.g5.md#g5.organization.creation). Reason: user request (2026-10-27); the landing was Base, a workspace that is not where work happens.

<a id="g5.ui.onboarding-tour"></a>
[D · g5.ui.onboarding-tour]

A new person is walked through G5 by one tour of seven steps (six while the person's Guided [setting](#g5.ui.guided-toggle) is off, the default: the tour then ends when the workspace route loads at step 6, and a tour resumed at the seventh step finishes at once), each a card beside a spotlit control, with a step counter and an always-visible Skip; the backdrop blocks the rest of the page, so each step is taken by pressing the spotlit control. It starts on a tenant organization's Home when the [record](users.g5.md#g5.user.onboarding) shows the tour was never shown, on desktop only; the operator organization's Home is the [platform console](organizations.g5.md#g5.organization.platform-operator.home), so its pages host no step. Steps, in order, each advancing on the action it names: (1) the welcome card asks to press the strip's Settings button, because agents cannot run until an account exists; (2) the Theme and Mode rows, Next; (3) the Accounts section, saying the organization already provides a shared account when one exists, otherwise asking for the person's own account and holding Next until one exists; (4) back on Home (the tour navigates), the personal scratchpad chat, explained as reading the organization's code without changing it and as the place ideas leave as issues; (5) the composer, seeded with a message asking the agent to create an issue titled "[Test issue] <first name> onboarding" assigned to the person, advancing when an open issue with that title created after the step began appears in the organization's issue search; the card carries a "Create issue manually" button that moves the spotlight to the Issues tile's Create alone, and the same Create is offered as a fallback after a timeout or a chat error; (6) that issue's row, then Start workspace on its page, advancing when a workspace route loads; while the Start workspace dialog is open the tour keeps a card beside its Fork button, without dim or blocking, saying the person can fork now or close the dialog and work on the issue later; (7) the sidebar's Guided entry (the rail's compass icon, or the full sidebar's section header), ending on that press or on Done. The chat steps' card sits beside the chat panel rather than above its composer, so a started chat's transcript stays visible. A viewer, who has no personal workspace, gets steps 1 to 3 and a closing card whose Done finishes the tour. Every step shown is recorded as reached; Skip records the skip. An unfinished, unskipped tour resumes at its recorded step on the next visit, with a "Continue tour" pill that navigates to the step's page when the person is elsewhere; a Replay control in Settings, Appearance clears the skip and starts the tour again without lowering the record. Pages expose only stable anchors and the composer seed; no page knows the step list. Reason: user request (2026-10-26) for popups that highlight which buttons to press, rewritten by the user into this action-driven flow after reading the research the agent gathered (short tours, an action per step, hand-off to the Guided checklist); the design records the sources.

## Workspace navigation and corner controls

<a id="g5.ui.sidebar-default"></a>
[D · g5.ui.sidebar-default]

The left sidebar defaults to "Full sidebar, icon rail when collapsed": the full sidebar whose collapsed state is the icon rail with an Expand control at its top instead of the bare edge; pressing Expand or any section's icon expands the full sidebar, an icon press on that section opened and in view, and the collapsed rail opens no overlay. Full sidebar, Expand on hover (the icon rail that opens the stacked panel on hover or focus) and Icon rail only remain per-person choices, and unknown stored values fall back to the default. Reason: user request ([chat intent](../../chat_intent/davide-siderail/request.md), 2026-11-09) for the mode and for it as the default; the earlier defaults were the full sidebar, then expand-on-hover (user request, 2026-10-20).

<a id="g5.workbench.tabs"></a>
[D · g5.workbench.tabs]

The shell's open workspace tabs belong to the signed-in person and browser; the route selects the active tab and only that page is mounted. A tab identifies its workspace as the route resolves it, by organization and case-insensitive name, so one workspace is one tab however it was reached. Each workspace tab shows its workspace's [phase](workspaces.g5.md#g5.workspace.phase-state) icon in the phase hue and its name as listed, from the organization's workspace list matched the same way, and no glyph for a workspace that list lacks. A [personal workspace](personal_workspace.g5.md#g5.personal.routes) tab is never in that list and shows its owner's round avatar in the glyph's place instead: the caller's own on `/:org/me` from the signed-in person, a member's on `/:org/users/:handle` resolved through the [organization roster](#g5.ui.home-workspaces), each the OAuth picture or a deterministic initial-and-tint fallback that the same person keeps across surfaces. The avatar is decorative, with the owner's display name as its tooltip; the tab's accessible name stays its label. Show only the route organization's tabs, retaining other organizations' tabs for return. Home and the "+" fork picker use that same organization; the picker opens, forks and creates workspaces and offers no organization creation, which only the [operator console](organizations.g5.md#g5.organization.creation) provides. Closing the active tab selects its left neighbour in that organization's strip, otherwise its Home. The person's browser tabs share one list, so a close in one sticks in all: arriving at a workspace route opens its tab, but a browser tab already showing a workspace whose tab was closed elsewhere keeps showing it under a transient, unsaved tab instead of reopening it. The picker at `/:org/fork` shows only that membership's role and workspace lineage, with search retaining matching ancestors; nonmembers see no workspace list. Opening a name selects that workspace; Fork switches to its organization and uses the ordinary fork dialog. Reserve `fork`, `me` and `users` as organization routes, not workspace names; `/:org/me` and `/:org/users/:handle` open [personal workspaces](personal_workspace.g5.md#g5.personal.routes). A tab follows its workspace through a [rename](#g5.workbench.tabs.rename), closes when its workspace is [deleted](#g5.workbench.tabs.deleted), and the person can [reorder](#g5.workbench.tabs.reorder) the workspace tabs.

Returning through the strip, including the neighbour selected after closing a tab, restores that workspace's last URL, center tabs, selection and whether Chat was in front from this browser session. Match snapshots by workspace ID, not a reused name; another window's saved tabs cannot replace this session's snapshot. Forget return state when a tab leaves the list or the person changes. A fresh open or reload follows the named route, otherwise the timeline; a link or Home row is a fresh open. Restored selection survives only while the returned-to path and branch hint stay unchanged. Every center surface has a URL that reopens it from a reload or a link: content with an identity (spec, document, file, guide, issue, commit and task changes) by path, singleton surfaces by `?view=`, and Chat keeps the content URL with its conversation query. A task route resolves by task ID independently of the paginated task list. A URL naming a surface the workspace does not offer or know opens nothing and falls back to the workspace root.

<a id="g5.workbench.tabs.rename"></a>
[D · g5.workbench.tabs.rename]

A workspace [rename](workspaces.g5.md#g5.workspace.rename) rewrites its tab in place: the tab keeps its position, its return memory and its snapshot under the new name, and a case-only rename only changes the label. A browser arriving through a former name replaces the stale tab instead of adding one; when both names are open, the earlier position is kept and the other entry dropped. Tabs and their memory are stored by name, so without this a rename would leave a dead tab.

<a id="g5.workbench.tabs.deleted"></a>
[D · g5.workbench.tabs.deleted]

Deleting a [workspace](workspaces.g5.md#g5.workspace.delete) closes its tabs. The page or Home row that deletes it closes the tab at once. No event reports a deletion made elsewhere — in another browser, on another device or by another person — so the shell acts on the server's answers. A page whose workspace reads as gone resolves its name once more: when the name no longer resolves, or still names the deleted workspace, the tab closes as the page's own delete closes it, showing the left neighbour, else Home; a name now held by a new workspace shows that workspace under the same tab. A route that never resolved — a stale link, or a tab kept for a workspace deleted while the browser was away — keeps the not-found page under a transient tab and leaves the list. For the tabs not shown, a workspace this browser session once saw in the organization's workspace list and a later list lacks is gone from the person's view: the tab follows the workspace when the list still has it under another name ([rename](#g5.workbench.tabs.rename)), else leaves the list. A workspace never seen listed is not judged by the list: the list omits workspaces a tab can still open (review copies and personal workspaces), and a list read before a fork or creation lacks the workspace just opened. Personal workspace tabs resolve by kind and are never owner-deleted, so they are exempt.

<a id="g5.workbench.tabs.reorder"></a>
[D · g5.workbench.tabs.reorder]

The person drags workspace tabs left or right to rearrange them within the strip, as browser tabs are rearranged, to any place from the first workspace place immediately after Home to the last. Home cannot be dragged or displaced and stays at the left when the workspace tabs overflow; the fork placeholder (New tab) and "+" stay outside the sortable range. Reordering an active or inactive tab keeps the selected workspace, the current URL and the tab's return state. The order survives refresh through the person's stored tab list and follows its organization scope: a move applies to the latest stored list, keeps other organizations' tabs, and never persists the displayed list wholesale, so a concurrent close or rename is never undone by resurrecting a stale or transient tab. Clicking, closing, middle-click closing and arrow-key navigation keep working; a drag never selects or closes a tab, overflowing tabs stay reachable while dragging, and a drag can be cancelled. Detaching tabs and moving them between browser windows are out of scope. The [keyboard move](#g5.workbench.tabs.reorder.keyboard), [anchoring](#g5.workbench.tabs.reorder.anchoring), [cancellation](#g5.workbench.tabs.reorder.cancel), [touch](#g5.workbench.tabs.reorder.touch) and [cap eviction](#g5.workbench.tabs.reorder.eviction) refine it.

<a id="g5.workbench.tabs.reorder.keyboard"></a>
[D · g5.workbench.tabs.reorder.keyboard]

Ctrl/Cmd+Shift+Left/Right moves the focused workspace tab one place, within the same range as a drag, and keeps its focus; unmodified arrows still only move focus. Reason: an accessible alternative to dragging ([accessibility](#g5.ui.accessibility)).

<a id="g5.workbench.tabs.reorder.anchoring"></a>
[D · g5.workbench.tabs.reorder.anchoring]

A move places the stored entry before the first tab shown to its right that is still listed in its organization, else after that organization's last listed tab; a tab no longer listed — including the transient tab — leaves the list unchanged. Reason: the displayed list can hold a transient tab and omits other organizations, so an index into either list misplaces the move, and a single right-hand anchor fails when that neighbour is transient or was just closed.

<a id="g5.workbench.tabs.reorder.cancel"></a>
[D · g5.workbench.tabs.reorder.cancel]

Escape, the browser cancelling the pointer, or a change to the tab list from another browser tab ends a drag without moving anything. Reason: a drag measures the strip once, so a list changed underneath it would land the tab by stale positions.

<a id="g5.workbench.tabs.reorder.touch"></a>
[D · g5.workbench.tabs.reorder.touch]

Touch does not drag tabs. Reason: swiping is the strip's native sideways scroll, which dragging would take over.

<a id="g5.workbench.tabs.reorder.eviction"></a>
[D · g5.workbench.tabs.reorder.eviction]

The tab cap keeps evicting the leftmost tab, which after a reorder is the tab the person placed first rather than necessarily the oldest. Reason: the list records order, not recency; tracking recency for a rarely reached cap was not worth another stored field.

<a id="g5.workbench.landing"></a>
[D · g5.workbench.landing]

The Overview control, a dashboard glyph labelled Overview, leads the ribbon before the phase's first action (New doc) in the ribbon's own button shape and opens the workspace page; the rail has no Overview row (user instructions, 2026-10-29: the control belongs in the top bar, named Overview for what the page shows). The rail reads Guided, Project Docs, Ontology, Files, Issues & tasks, Jobs, Archive and Source Control, in the person's own order; explainers and mocks are Project Docs under their `explainers/` and `mocks/` subfolders, with no section or category row of their own (owner instruction, 2026-10-30). Beneath the phase chevrons the landing shows, in order, the [personal request banner](reviews.g5.md#g5.governance.approval.personal-requests) when it applies, one [summary line](#g5.workbench.landing.summary), the [Primary Issue](#g5.workbench.landing.task), the [Work Description](#g5.workbench.landing.implementation) and Activity, in one page scroll so short windows retain reply access. Task intent and PR implementation stay separate. The engineering phase appears only in the chevrons; the landing adds no second phase or status dashboard. Approval previews follow the [request PR rule](reviews.g5.md#g5.governance.approval.request-pr): without a PR they show human and agent requests and decisions like any others.

Activity shows durable history beneath the task and implementation sections, newest first by last activity: a conversation's last message, a task's end else its start, and the time every other record carries. Its heading and [filters](#g5.workbench.landing.activity-filters) lead directly to the events, without repeated issue/Approvals navigation, a shared-discussion banner, source prose footers or mock interaction instructions. Event titles open their existing destinations; comment timestamps keep exact permalinks and external sources remain identifiable by an accessible icon. The primary issue's opening is a compact event rather than a second task description. Each review reply appears once in Activity; its full thread remains reachable even when the parent event is outside the loaded page. Person-written messages—issue and workspace comments, replies, and approval decisions with written comments—share the same box; events, including uncommented approval decisions and issue openings, stay plain. Current state and each event timestamp appear once. Issue comments and workspace discussion comments/replies offer Reply regardless of author: workspace entries open their canonical thread ready to reply, and issue comments use the [issue reply entry](issues.g5.md#g5.issue.comment-entry). Preserve existing write permissions; unavailable writes cannot become enabled by changing entry type. Retain Edit, errors, historical/stale-review notices and pending-publication recovery. New general workspace comments keep [Reviews](reviews.g5.md#g5.governance.review.comments); issue comment creation and contextual replies follow their [entry owner](issues.g5.md#g5.issue.comment-entry). Existing routes remain valid. [Phase changes](workspaces.g5.md#g5.workspace.phase-state) remain stored/served but never appear as rows or filter choices. Landing versus tab return follows [workspace navigation](#g5.workbench.tabs). Surrounding page controls, including Start, Merge to parent and Guided, are outside this cleanup.

<a id="g5.workbench.landing.activity-filters"></a>
[D · g5.workbench.landing.activity-filters]

Activity's category pills select All or a nonempty subset. All alone is highlighted by default; clicking a category from All selects only that category. Subsequent category clicks add/remove categories, with their histories combined by OR; removing the last category or selecting every category resets to All. All resets categories only; phase is an independent AND restriction. Keep keyboard activation and focus, per-person/browser preferences, loaded-history counts, zero-count choices and filtering of later loaded pages. Migrate existing hidden-group preferences without losing a proper subset; the old hide-everything state becomes All. Filtering neither deletes nor requeries history, and the [Current exception](#g5.ui.activity-current) still applies. Reason: the owner approved the landing cleanup mocks and one-click Tasks selection.

<a id="g5.workbench.landing.summary"></a>
[D · g5.workbench.landing.summary]

The summary line holds the workspace-wide approval summary and the [complete pushed file counts and their navigation](#g5.workbench.change-comparisons.activity), independently of Source Control filters. A known zero total shows only “0 changed files”; loading or unavailable counts never become zero, and a nonzero total keeps the category breakdown. Approvals links to the [shared Approvals destination](#g5.workbench.gates). No popup or return chrome is added. The approval summary reads `Approvals · N approved, M rejected, K pending`, counting the workspace's [outgoing requirements](reviews.g5.md#g5.governance.approval.requirements) once each by their [shared status](reviews.g5.md#g5.governance.approval.status), never reviewers; none attached is no approvals required, and a projection that cannot be read is unavailable, never approved.

<a id="g5.workbench.landing.approval-events"></a>
[D · g5.workbench.landing.approval-events]

An approval Activity event keeps its linked headline (`<approver> approved/rejected/withdrew response for <requirement>`) and existing contextual labels, and shows the decision's summary on the next line when one exists: a human's comment or an agent's [authored summary](agent_approvals.g5.md#g5.agent-approval.assignment). It never shows a raw or clipped report body.

<a id="g5.workbench.landing.task"></a>
[D · g5.workbench.landing.task]

The Primary Issue section (its box formerly labelled Workspace task) shows one title row (the primary issue's [status](#g5.workbench.landing.status), the task title and a numbered link to the issue where it lives, such as “GitHub issue #750”), then the opening of the task body, the collapsed “Linked issues (N)” disclosure when related links exist, and Read more at the bottom. The title and body keep the existing task source: the primary issue, else the workspace description, else the docs README. Without a primary issue the box shows that native title and body with no invented issue. The preview is the reader's own rendering clipped at display time, so its opening, formatting and links match the reader. Linked issues are the workspace's related links, never the primary one; each row is its title (opening that issue's own reader), its status and its numbered link, qualified by repository only when several repositories make the number ambiguous. Close reasons are not shown. The reader shows the full task and the expanded linked list with a single close control. The section heading is a keyboard-operable disclosure: it starts collapsed when Work Description shows a pull request and expanded otherwise, including while that section is loading, unavailable or empty, and a person's manual choice holds across refresh of the same workspace landing without being saved.

<a id="g5.workbench.landing.implementation"></a>
[D · g5.workbench.landing.implementation]

The Work Description section (formerly labelled Implementation description) presents the workspace's non-stacked G5 PR: its open PR, else its latest. It shows the G5 PR's lifecycle [status](#g5.workbench.landing.status) beside its title, the opening of the [canonical G5 description](../software_architecture/git.g5.md#g5.git.pr-description), one row per repository of the PR's recorded vector (repository, its commit SHA, and for a member the PR changes its [repository pull request](../software_architecture/git.g5.md#g5.git.pr-repository)'s own status and its link or name from its adapter: GitHub's numbered link for a published member, the G5 PR's name (`PR3fa9c1`) for a local one; an unchanged member says so), and Read more. The G5 status and each repository pull request's status stay distinct. The box stays expanded whatever the status, offers no create or update action (Prepare to Ship and Pull Requests own those) and before the first PR says only “No implementation description yet.” The reader repeats the same rows after the full description; when [descriptions differ or are unchecked](../software_architecture/git.g5.md#g5.git.pr-description), it offers one G5 Description tab and one tab per provider publication, otherwise it shows a single description with a “Matches GitHub” hint. Stacked PRs (an ordered dependency between PRs) are out of this presentation's scope; several repositories in one G5 PR do not form a stack. Reason: issue #816; stacks follow in #818. Repository name, SHA and repository pull request status/link use aligned columns in both card and reader, including narrow containers. Reserve the status column on an unchanged member's row, which shows its SHA and says it is unchanged. Commit history is reached through Source Control; the implementation card and reader offer no separate history link.

<a id="g5.ui.deleted-workspace-prs"></a>
[D · g5.ui.deleted-workspace-prs]

The UI shows the same pull requests that tools read, including those whose source, target or both are [deleted](../software_architecture/git.g5.md#g5.git.pr-reads). Surfaces for acting on pull requests (the incoming list, Prepare to Ship, Home) show open ones, and [deletion closes](workspaces.g5.md#g5.workspace.delete.pull-requests) every open pull request of a deleted workspace, so those surfaces leave them off without a filter of their own. Where the UI shows pull-request history, pull requests with a deleted side are grouped apart from the rest and name the deleted workspace. Reason: owner decision (2026-10-30): the UI and tools return the same things, and history is grouped separately or left off where the UI's purpose is action.

<a id="g5.workbench.landing.status"></a>
[D · g5.workbench.landing.status]

Issues, linked issues, the G5 PR and provider PRs share one status vocabulary, each state with one icon, colour and label wherever it appears: Open, Draft (an actual provider draft only), Merged (observed merge evidence, PRs only), Closed, and Unavailable when the state could not be established, never assumed Open.

<a id="g5.ui.activity-current"></a>
[D · g5.ui.activity-current]

Ongoing work sits at the top of the Activity record as ordinary rows marked Current: every conversation with a queued, running or waiting agent task and every preview or command task in one of those states, newest last activity first, each row carrying its state (running, waiting for input, queued; a preview row its [run stage](#g5.ui.activity.preview-row) instead). It is the same projection served by its own read without a GitHub call, refreshed while shown, never a filter kind and never hidden by the filters; there is no separate box or heading. An entry shows once: as a Current row while ongoing, in the record at its last-activity position afterwards. Nothing is persisted; a failed Current read is shown as unavailable with Retry, never as nothing running. Reason: the user chose mock B (2026-10-20) over a separate Current box.

<a id="g5.ui.activity.preview-row"></a>
[D · g5.ui.activity.preview-row]

A preview run is one Activity row, named by its profile (the task is named `Preview · <profile>`, or `default profile`, at creation; the repository is not shown). While ongoing it carries its stage in place of a bare running: queued, starting, building, serving on its port, closing. Ended, it states how it ended in the words of the [run log](#g5.preview.run-log): ended on its own, closed by the owner (naming a cleanup that failed or was killed), stopped by the platform with the retained [stop reason](../software_architecture/workspace_vm.g5.md#g5.preview.service.stop-reason), failed with the application's exit status or signal, the module's cause, a time or output limit, lost supervision or a lost VM or host (before or after the module started; naming a stop under way, while a Close cut short by the loss reads as closed with its cleanup unfinished), or did not start. Stage and ending are derived from the task's events, spec and outcome; nothing is persisted and no extra rows mark readiness or closing. The application's output and its stderr tail stay in the run log; the row carries one sentence, in the ordinary body colour. On the page and in the [rail](#g5.ui.activity-rail), the owner's row opens the preview dialog, with its log when the run has one (ongoing, failed with its VM still holding output, or a close whose cleanup failed or was killed) and the launcher otherwise, never the task's Workspace changes; a reader without the preview control sees the page row as plain text and the rail row opens the Activity page like a command row.

<a id="g5.ui.activity-rail"></a>
[D · g5.ui.activity-rail]

Superseded (2026-10-29): the rail has no Activity section and no Overview row. The ribbon's [Overview control](#g5.workbench.landing) opens the workspace page, which is the landing page and the complete record, with Current rows at its top ([Activity current](#g5.ui.activity-current)); the full sidebar's collapse control sits at the top of the stacked panel (the icon rails have none, owner instruction 2026-10-29). The earlier panel (Open workspace page, Current rows, then the latest comments, approvals, pull requests and guided steps, at most eight) was the user's instruction of 2026-10-20; the owner moved the entry into the top bar and removed it from the left bar on 2026-10-29. Reconciled with the [issue447 direction](../../chat_intent/qa-review-prototype/request.md) that renamed the row Workspace: the user chose option A of the [rail-entry mock](../../pr_ontology_diff_design/activity-current-rail/mocks/rail-entry-options.html) (2026-10-20), keeping the Workspace label and glyph on the panel; Approvals stays a header-only destination.

<a id="g5.quality.platform-reports.trigger"></a>
[D · g5.quality.platform-reports.trigger]

One shell controller owns the [bug-report](issues.g5.md#g5.issue.platform-bug) triggers, drawer and Ctrl/Cmd+Shift+B shortcut. The page chrome carries the steady trigger. Show the floating bottom-left trigger only when no chrome trigger is mounted or a modal-tier overlay covers it; exclude the report drawer itself from that condition. A loading screen that page chrome will replace counts as chrome, so the fallback does not flash before the chrome mounts; a load-failure screen does not, so the fallback stays reachable when the chrome never arrives. Remove the fallback when the chrome becomes reachable again, and keep the drawer usable above floating panels. An overlay above the floating trigger's own layer leaves only the shortcut available.

<a id="g5.workspace.base-ui.chat"></a>
[I · g5.workspace.base-ui.chat]

Base shows no Chat tab, page, panel or window. Its sole Chat affordance is the disabled [corner button](#g5.workspace.ui.chat-corner-button), which explains Base's read-only state and opens nothing. This presentation rule does not change conversation read authority.

<a id="g5.workspace.ui.chat-corner-button"></a>
[D · g5.workspace.ui.chat-corner-button]

On desktop workspace routes, offer a secondary bottom-right Chat button that opens the existing floating Chat panel at the right edge, below all workspace controls and down to the bottom. Move Chat out of the center, keep its last active session (or the no-session landing), and focus an enabled composer. Keep drag, resize, Dock and Close; the panel resizes from every edge, so at the right edge it grows from the edges facing the content, and it reopens at its last width in this browser. The button only opens, with no new shortcut or Escape action. It can be dragged anywhere in the viewport to uncover what it hides, even while disabled: a short press still opens, a drag never does, and the spot is remembered in this browser and kept inside the viewport. Hide it on mobile, while Chat is already the center surface, while that panel is open or while a modal-tier overlay covers the page. On Base, for readers and under read-only or freezing policy, show it disabled with the existing write gate's reason; a stopped VM alone does not disable it. Other Chat reading entry points retain their authority, subject to the Base rule. Dock and Pop out move the same conversation between the center and the floating panel in either direction, from the no-session landing too and while an agent is running, and keep the unsent composer text; pending references and attachments are not kept. Tab return does not restore the floating placement: the panel returns closed. The [chat bubble stack](#g5.workspace.ui.chat-bubble-stack) sits above the button and moves with it.

<a id="g5.workspace.ui.chat-bubble-stack"></a>
[D · g5.workspace.ui.chat-bubble-stack]

Above the enabled Chat corner button, stack one round bubble per chat of the workspace, every chat at any time, in the [list's order](chat.g5.md#g5.chat.activity.order): the most recent chat sits nearest the button. The bubble's face says what the chat is doing: working while its turn is active, waiting for your answer while its turn [waits for the person](chat.g5.md#g5.chat.activity.awaiting-input), else idle, at rest or with its latest result still [uninspected](chat.g5.md#g5.chat.inspection) by them; a ring marks working and waiting, a dot the uninspected result, and hovering or focusing a bubble shows the chat title and the state beside it at once, which its accessible name also carries. As many bubbles show as the person's count setting asks; the rest fold into a count past the farthest bubble, and pressing the count unfolds every chat until the same spot folds them back, so an unfolded column may reach past the viewport. Pressing a bubble opens that chat in the floating panel exactly as the corner button does, acknowledging nothing by itself. While Chat is the center surface, where the button has no job, the stack stays at the button's spot and a press opens that chat in the center. Every bubble is a handle for the pair: a drag that starts on any bubble moves the stack and the button together, as a drag on the button does, and the drag's own press opens nothing; the lone stack of the center surface drags the same way, to the same remembered spot the button later takes. A button dragged into the top half of the viewport has its stack hang below it, nearest bubble first, so every bubble stays on screen. The floating panel's Close only closes the panel: no control removes a chat from the stack or keeps it there, since every chat always has its bubble. The stack is absent on mobile, while the floating panel or a covering overlay is open and while the button is disabled, and draws only on the chats the list already loads, the picker's page of most active chats, without changing which chats it shows; a chat beyond that page has no bubble.

Two per-person settings, stored like the [header styles](#g5.workbench.header-styles), shape the stack. "Chat bubbles": `off` (the corner button alone, today's behavior), `still` (one face per state from the supplied robot images), `animated` (each state alternates its two frames) or `3d` (default; a robot head drawn by the `robot-heads` library: working, happy for waiting, sleeping for idle). "Chat bubbles shown": a whole number from 1 to 50, the list's page size, default 5. Unknown or out-of-range stored values fall back to the defaults; the earlier "Chat bubble stack" layout setting (column or collapsed) is gone and a stored value of it means nothing. Under the browser's reduced-motion preference, `animated` shows still frames and `3d` holds its pose. Reason: owner request (2026-10-29) with the names and store chosen on the reviewed mock, and the owner's 2026-11-09 request for a count setting in place of the layout and a count that unfolds; see the [chat intent](../../chat_intent/chat-bubble-stack/request.md).

<a id="g5.workspace.ui.fork-landing"></a>
[D · g5.workspace.ui.fork-landing]

A fork made in the UI, from whichever page its dialog was opened, lands on Chat as the center surface of the new workspace. With Start work chosen it lands on the chat the fork started. Otherwise it lands on a fresh composer, pre-typed and never sent: with an issue attached, the issue's suggested first message, the same text the dialog's Start work box offers; without one, empty, with the placeholder "What's your idea?" instead of the usual prompt until a chat exists. A chat that failed to start lands its unsent message the same way. The fork dialog keeps its Start work message box and launch controls, and Start work still defaults to on when an issue is attached. The fresh composer is an ordinary new chat: on a stopped VM the owner sees the pre-typed text with [Start and send](chat.g5.md#g5.chat.start-and-send), which starts the workspace with the first message; a composer that cannot send (a reader's) shows its reason first. `?view=chat` names Chat in the center without a session, as a `?session=` hint names one; a content tab replaces either hint. Reason: the owner's request of 2026-11-03 ([record](../../chat_intent/davide-landing-fork/request.md)).

<a id="g5.ui.review-sidebar"></a>
[D · g5.ui.review-sidebar]

Shared file and node discussion sidebars use their full available parent height and one sidebar scrollbar for threads and suggestions, including the final card's controls. A suggestion list below an editor keeps its independent height policy.

## Authentication continuity

<a id="g5.ui.authentication-continuity"></a>
[D · g5.ui.authentication-continuity]

Ignore refresh or profile responses from an older sign-in context. A temporary refresh failure keeps the current session; an explicit refusal ends it only if it is still the session being refreshed. Retry authenticated requests only for the same user and organization, allowing their credentials to renew.

AuthProvider keeps initial refresh and profile failures unresolved and retries temporary
unavailability until recovery, a current refusal, sign-out or replacement context.
A profile 401 permits one credential renewal; a second 401 for that newly issued
credential ends the current session, even across temporary profile failures. If the
credential expires while waiting, allow renewal again. Refusals for a superseded token
cannot end its replacement session, even when the authorization claims are unchanged.

<a id="g5.ui.authentication-continuity.adoption"></a>
[D · g5.ui.authentication-continuity.adoption]

Every token-adoption path preserves the same person's mounted page and unsent work
through organization changes while immediately publishing the new organization and
scopes. A different person or actual sign-out clears the outgoing person's state.

<a id="g5.ui.authentication-continuity.draft-owner"></a>
[D · g5.ui.authentication-continuity.draft-owner]

Profile availability never determines draft ownership; retain the isolation rules for
[file drafts](documents/files.g5.md#g5.documents.files-view.drafts) and
[comment drafts](documents/discussion.g5.md#g5.documents.comments.drafts).

## Preview and publication feedback

<a id="g5.ui.publication-activity"></a>
[D · g5.ui.publication-activity]

Show unresolved automatic PR-publication failures as the latest failure plus a count since the last completed publication. Clear them when publication completes or the PR ends; a private push or a push to just one repository is not enough. Do not show [a refresh still settling](../software_architecture/git.g5.md#g5.governance.pr.follow-push) as a failure.

<a id="g5.preview.profile-label"></a>
[D · g5.preview.profile-label]

Show the running preview’s selected profile, including when it uses the workspace default, and show its description only after it loads. Keep full-stack, full-stack-no-worker and frontend-only distinguishable so the owner knows which environment is running.

<a id="g5.preview.trigger-state"></a>
[D · g5.preview.trigger-state]

The preview trigger shows the workspace preview’s state without opening its dialog: the glyph is unlit with no preview, and lit amber while starting or closing, green while serving, and red after a run that failed, a close whose cleanup failed, or a close cut short by a lost VM. The red state belongs to that run and lasts until the next start replaces it; it is hidden while the owner’s VM is stopped, when the trigger is disabled and shows no state, and shows again once the VM runs. The owner otherwise had to open the dialog to learn whether the preview was serving.

<a id="g5.preview.trigger-state.accessible"></a>
[D · g5.preview.trigger-state.accessible]

Colour is never the trigger state’s only cue: the tooltip and the accessible description say the [state](#g5.preview.trigger-state), and the port while serving; the accessible name stays the plain label.

<a id="g5.preview.trigger-state.freshness"></a>
[D · g5.preview.trigger-state.freshness]

The trigger reads the same preview state the dialog shows, at the VM status poll’s cadence while the owner’s VM runs and more often while the dialog is open on a live preview; a stopped VM is not polled. One read serves every mounted trigger, so showing the [state](#g5.preview.trigger-state) costs one light request per cadence per open workspace page.

<a id="g5.preview.run-log"></a>
[D · g5.preview.run-log]

The preview dialog shows the run’s log on request (a Log toggle beside Stop preview; a View log link in the failure notices): a stage strip (Queued, Starting, Building, Serving, Closing, Closed or Failed) from the preview view and the task’s lifecycle timeline entries, its failure notice worded by the same helper as the [Activity row](#g5.ui.activity.preview-row) (the log reads the module's outcome, which lacks the retained stop reason and the actor, and now also notices a timed-out or interrupted run), and below it the application’s output as a read-only terminal view attached to the guest’s [hosted session](../software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail) `preview:<task id>`, following new output until the owner scrolls up. While building, serving or closing the view is live; closing shows the shutdown and cleanup output with Closing current. Nothing of the output is read from or written to the task timeline. Attachment and fallback follow [g5.preview.run-log.attach](#g5.preview.run-log.attach); ended runs follow [g5.preview.run-log.ended](#g5.preview.run-log.ended).

<a id="g5.preview.run-log.attach"></a>
[D · g5.preview.run-log.attach]

A live run’s first attach waits until the run is known to be dispatched (the task timeline shows the module started, or the view reports ready), because the guest hosts the session only from then; an ended run attaches at once. A refused attach on a live run is then retried on a short cadence, bounded. Waiting and retrying never rebuild the terminal: the panel stays mounted at its height and its header says the log is waiting for the application to start, so the dialog does not flicker while the task is queued. When the attach stays refused, or an ended run’s attach is refused, or the VM is stopped or replaced, the log shows the stages and a one-line notice with no terminal: on a running VM the line says the session is no longer hosted; otherwise “Output is not retained after the VM stops.” Only an ended run with a failure outcome also shows that outcome’s stderr tail ([g5.preview.run-log.ended](#g5.preview.run-log.ended)); a live run has no diagnostic source besides the session.

<a id="g5.preview.run-log.ended"></a>
[D · g5.preview.run-log.ended]

A run that failed while its VM runs shows Failed, the ended session’s retained output, then the notice with the exit status and the 4 KiB stderr tail from the outcome; a close whose cleanup failed shows the same with its cleanup notice. A cleanly closed preview shows the launcher only, with no link to the last run’s log even though the session is retained (owner decision).

<a id="g5.preview.dialog-controls"></a>
[D · g5.preview.dialog-controls]

The preview dialog is one shell with one dismiss: closing it never stops the preview, and the action that stops the application and runs its cleanup is named Stop preview, so the two are not confused. Open preview is offered only while the application serves and no stop is in flight; while it is not offered it says so itself (“Starting…” with a spinner during startup) on a neutral surface, never relying on opacity or colour alone, and Stop preview stays a full-contrast action beside it during startup and serving, carrying the pending label while the stop runs. The lifecycle is spelled out once in a live status line; the actions wrap on a narrow viewport rather than overflow. The shell is wide only while a [run log](#g5.preview.run-log) is rendered in it: Stop preview on a clean run returns the dialog to the launcher at its narrow width with the Log toggle collapsed for the next start, while a failed run’s log keeps the wide shell with its notice. While the owner's VM is not running the launcher stays open on the committed applications and its action is [Start workspace and preview](../software_architecture/workspace_vm.g5.md#g5.preview.start-and-preview) (Preview once running during a Start); a preview recorded that way shows the Start's progress in the status line until the VM runs, with Stop preview beside it, and a Start that ended without a VM shows as a did-not-start last run with the launcher offered again.

<a id="g5.workbench.gates"></a>
[D · g5.workbench.gates]

Approvals belong to the whole workspace, independently of its Business, System or Integration Engineering phase. Show each required approval type once through the shared Approvals destination; phase chevrons and Guided steps do not repeat approval meters or rosters. A phase move never requires, creates, duplicates or settles a review. The request picker lists each type once, ordering recommendations for the current phase first, then other associations in Business/System/Integration order. Required types show their “Recommended for …” association without phase grouping.

Place one Approvals control after phase action icons and before owner/VM/details/chat controls in every phase; omit duplicate rail/configured approval-opening actions while preserving non-approval configuration. The landing's approval summary and approval Activity rows link to that same destination; they are navigation, not another control or roster. [Target requirement policy](reviews.g5.md#g5.governance.approval.requirements) owns merge admission and [addressed requests](reviews.g5.md#g5.governance.approval.request) own decisions.

<a id="g5.workbench.journey.gates"></a>
[D · g5.workbench.journey.gates]

Guided keeps its engineering steps and explicit phase moves. Reach requested reviewers through the [shared workspace Approvals destination](#g5.workbench.gates); do not repeat rosters between phases. Step completion follows [journey evidence](#g5.ui.journey-evidence), independently of approval state.

<a id="g5.workbench.review-prototype"></a>
[D · g5.workbench.review-prototype]

The production review experience and independent demo follow the same landing, [request authority](reviews.g5.md#g5.governance.approval.request) and [captured revision rules](reviews.g5.md#g5.workspace.review-revisions). Ontology and review comparisons expose their selected scope without redundant return links. Source Control and landing use the [pushed comparison](#g5.workbench.change-comparisons); explicit parent comparisons and other review readers retain their own scopes. Ordinary browsing and Source Control follow [repository browsing](#g5.workbench.repository-browsing) and [change selection](#g5.workbench.change-selection). Unsent formal-review drafts stay private to the person, workspace, PR, type and addressed request across navigation/reload; submitting one never publishes another. Present revision history through the complete-vector table: rows are observed workspace revisions, columns are repositories, unchanged members stay explicit, and consecutive recorded vectors define history comparisons. The landing presents the current PR's recorded vector as its [repository rows](#g5.workbench.landing.implementation), not a separate pushed-revision popup. Show actual push evidence separately from commit times and leave absent timestamps unknown; never synthesize snapshots by combining independent commit times.

## Repository browsing and changes

<a id="g5.workbench.repository-browsing"></a>
[I · g5.workbench.repository-browsing]

Project Docs and Files open their complete authorized trees in the left menu using the Settings click/hover preference; opening a menu preserves current content, and selecting a file opens its ordinary reader tab. Project Docs includes associated managed documents under its workspace root. Omit comparison selectors and routine working-source labels; retain access, failure and truncation notices.

<a id="g5.workbench.change-selection"></a>
[I · g5.workbench.change-selection]

Source Control groups changes by repository, initially selected, with a heading checkbox and visible independently selectable children: Not yet pushed first, then real pushed commits with author and Committed date. Heading mixed state derives from children. All changes selects or clears both available repository choices and the [file-type choices](#g5.workbench.change-selection.types); its checked/mixed state reflects both groups, including when no repository choices are available. Clicking a mixed All changes selects everything.

Keep this selector when the VM is stopped. Show Not yet pushed disabled whenever it is unavailable, without private filenames or invented counts; explain owner/running-VM availability through contextual help.

<a id="g5.workbench.change-selection.apply"></a>
[I · g5.workbench.change-selection.apply]

Expands [change selection](#g5.workbench.change-selection): repository/commit and file-type checkbox edits stay pending until Show. Clearing repository/commit choices disables that action; selecting no types remains applicable. Type-only edits can apply to an existing captured comparison, including an empty one, while its represented selection and authority still match. Only the latest requested comparison may replace the applied result. Successful apply acknowledges the applied choices and clears their pending warning; edits made during the request remain pending. Pending edits preserve the applied diff, summary and scroll position.

<a id="g5.workbench.change-selection.recovery"></a>
[I · g5.workbench.change-selection.recovery]

Expands [change selection](#g5.workbench.change-selection): the checklist captures pushed history without enumerating or fingerprinting local files. Not yet pushed means local work captured when Show runs. Show checks pushed source tips and the effective merge bases; a target-tip advance with the same merge base does not invalidate the list. If the represented history changed, refresh the list once, preserve valid choices, custom exclusions and file types, identify the affected repositories and ask the person to review their choices and click Show. Do not automatically apply or loop. Remove only commits confirmed unavailable, name each removed revision and select no replacement; incomplete history and failed reads do not prove absence. Retain the previous comparison, counts and view position until a successful Show, labeled as previous after a refresh. Local reads retain request-local consistency checks across current and retained guest runtimes, without retaining an unpushed baseline between requests. Failed reads retain the previous result and offer retry; [access withdrawal](#g5.workbench.change-access) overrides retention.

<a id="g5.workbench.change-selection.types"></a>
[I · g5.workbench.change-selection.types]

Expands [change selection](#g5.workbench.change-selection): Code, Ontology and Docs start checked. Show only matching filenames and their grouped diffs, deriving both panes from the applied commit comparison and selected types. Applying additional commits or types restores matching files; exclusions remove them from both panes. Selecting no types shows an explicit empty matching view. Heading file and line totals count visible files; the tree summary shows selected-type unique counts and matching/overall comparison totals. Clear a file selection that no longer matches and clamp restored scroll to the visible content. Put category definitions and filter meaning behind accessible contextual help, without routine explanatory paragraphs or comparison controls in ordinary browsing.

<a id="g5.workbench.change-selection.navigation"></a>
[I · g5.workbench.change-selection.navigation]

Expands [change selection](#g5.workbench.change-selection): Show opens a changed-file tree with an accessible resizable separator beside one scrollable stream of the [matching diffs](#g5.workbench.change-selection.types). File links jump within the stream; View file opens the ordinary reader and preserves diff selection/scroll on return.

Omit a fixed comparison-direction subtitle in the center pane. Contextual help explains net and per-commit comparisons; patch labels retain their actual endpoints.

<a id="g5.workbench.change-selection.rendering"></a>
[D · g5.workbench.change-selection.rendering]

Expands [change selection](#g5.workbench.change-selection): keep the matching filename tree and card anchors available while rendering text bodies near the diff viewport. Retain offscreen heights so file jumps and reader return preserve the scroll stream. Reuse unchanged tree metadata, cards and hunk content across comparison responses, while updating changed bytes and commit labels. One observer per pane limits presentation work without extra reads or persistent state; unavailable observers use eager rendering. This avoids mounting every line of a large comparison and blocking selection changes.

<a id="g5.workbench.change-comparisons"></a>
[I · g5.workbench.change-comparisons]

Complete selections use the unique Git merge base of captured pushed source and readable configured merge-target heads (the explicit target takes precedence over clone lineage) through the pushed source, optionally stacked with available unpushed work. Not yet pushed starts at that pushed tip; Show refuses the request when that option is selected and local HEAD does not descend from the tip. Missing or ambiguous comparisons remain explicitly unavailable per repository. Merged workspaces whose latest merged PR still matches their pushed source, and merged historical PRs, use final recorded source and preflight parent cuts; later workspace pushes resume live capture. Closed unmerged historical comparisons are unavailable. Never use post-merge target heads as historical parent cuts. Local commits remain unpushed until internally pushed. Arbitrary selections group the [selected commits’ own patches](#g5.workbench.change-history) by file, keeping labels/endpoints distinct and skipped commits excluded. Counts use unique repository/path identities with renames counted once and net reversions removed. [Categories](#g5.workbench.change-comparisons.categories) partition those identities; [Activity](#g5.workbench.change-comparisons.activity) summarizes the complete pushed scope. Actual push evidence alone supports push times; complete revision vectors remain in PR context. Personal workspaces use [their unpushed comparison](#g5.workbench.change-comparisons.personal).

<a id="g5.workbench.change-comparisons.read-efficiency"></a>
[D · g5.workbench.change-comparisons.read-efficiency]

Build only the comparison data each reader consumes: Activity needs the validated unique merge base and net file identities, without unused commit rows. Source Control loads one pushed checklist capture and checks its source tips and effective merge bases before Show, deriving full commit rows only for a list response. It may reuse a successful immutable comparison within one HTTP request for identical repository/source/parent/saved-base inputs; each capture still checks current access, heads, live authority and represented Git objects before the existing consistency decision. Do not reuse errors or replace independent selection/approval validation. This avoids repeated derivation without introducing a cache lifecycle across requests.

<a id="g5.workbench.change-comparisons.categories"></a>
[I · g5.workbench.change-comparisons.categories]

Expands [change comparisons](#g5.workbench.change-comparisons): classify each grouped file consistently in Source Control and Activity. The supported [ontology path policy](documents/ontology.g5.md#g5.documents.ontology.current-format) takes precedence; remaining docs-role repository files are Docs and all others Code, including configuration, tests, assets and ordinary documentation. Use canonical rename destinations and the retained path for deletions. Derive categories and counts from authorized paths and repository metadata without content reads, extra Git comparisons, count requests, scans, polling or durable filters. Group and count once per applied snapshot; pending edits do not rerender expensive diff bodies.

<a id="g5.workbench.change-comparisons.activity"></a>
[I · g5.workbench.change-comparisons.activity]

Expands [change comparisons](#g5.workbench.change-comparisons): Activity shows the complete pushed unique-file total followed by Code, Ontology and Docs counts that partition it. Partial availability appends + to the total with one contextual-help icon naming unavailable repositories and causes; category counts stay unmarked. With no available repository show unavailable, never zero. Always include all pushed changes, independently of pending or applied Source Control type choices. Code and total open the singleton Source Control tab with a consumed one-time selection: Code only, or all three types, with Not yet pushed unselected. Recapture current revisions on entry; reload keeps the applied selection and later user edits per browser tab. Carry no selection in URL parameters. Docs opens the document browser; Ontology keeps Diff Graph and its saved-current-content comparison. Remove the parent-advanced note.

<a id="g5.workbench.change-comparisons.personal"></a>
[D · g5.workbench.change-comparisons.personal]

Specializes [change comparisons](#g5.workbench.change-comparisons): [personal workspaces](personal_workspace.g5.md#g5.personal.workbench) have no fork baseline. Their outgoing comparison is empty and their Changes selector offers Not yet pushed, comparing their private member's unpushed work with its captured internal pushed tip; it does not invent a workspace base or a pushed-since-fork history. After Save, a subsequent Show excludes the published personal work. Reference checkouts and personal mounts remain outside Changes under the [member](personal_workspace.g5.md#g5.workspace.identity.reference) and [mount](personal_workspace.g5.md#g5.personal.mount) rules. A missing baseline in an ordinary workspace remains an explicit failure.

<a id="g5.workbench.change-checkpoint"></a>
[I · g5.workbench.change-checkpoint]

Capture pushed history on navigation and reload. A completed explicit Save changes, invoked through the agent (chat or `g5d workspace save`), checks that baseline against current pushed history. The Source Control menu offers no Save action; Show is its normal action and performs [history recovery](#g5.workbench.change-selection.recovery) before applying a comparison. Committing happens through the agent or other workspace surfaces.

<a id="g5.workbench.change-checkpoint.detection"></a>
[I · g5.workbench.change-checkpoint.detection]

Expands [the Save checkpoint](#g5.workbench.change-checkpoint): compare pushed source tips and effective merge bases. An unchanged history preserves the checklist; changed history visibly updates it and captures its represented baseline. Local file edits alone do not invalidate the list.

<a id="g5.workbench.change-checkpoint.selection"></a>
[I · g5.workbench.change-checkpoint.selection]

Expands [the Save checkpoint](#g5.workbench.change-checkpoint): preserve selected stable commits, include new children in fully selected repos and retain custom exclusions. Do not infer which newly pushed commits came from a custom Not yet pushed selection, because the list does not enumerate local commits. The [applied diff](#g5.workbench.change-selection.apply) stays stable until Show.

<a id="g5.workbench.change-checkpoint.failure"></a>
[I · g5.workbench.change-checkpoint.failure]

Expands [the Save checkpoint](#g5.workbench.change-checkpoint): failed saves/checks never mark unavailable repositories current. Available repositories may refresh alongside explicitly unavailable rows.

<a id="g5.workbench.change-access"></a>
[I · g5.workbench.change-access]

Only the running owner sees private local names, counts and bytes. Collaborators and stopped owners see pushed content. Browsing never starts compute. Stop, transfer and access loss invalidate private menu/result/tab data immediately, including late responses, overriding snapshot retention. Missing baselines/revisions, live/member read failures and bounded/truncated results remain explicit limitations and cannot be represented as complete zero totals.

<a id="g5.workbench.change-history"></a>
[I · g5.workbench.change-history]

Default history shows source first-parent commits unreachable from the captured parent, newest first, then the Merge base commit with its real summary, author, committed time, SHA and opt-in checkbox. Parent merges show only Git remerge patches, annotated Merged from the parent; other merges and selected history show labeled first-parent patches. All changes excludes opted-in history and the merge-base commit and clears their explicit selections, preserving the net merge-base-to-source diff. Selecting the merge-base commit uses the existing per-commit patch behavior without changing the comparison base. The original fork has a distinct Workspace base row, combined with Merge base in one commit row and checkbox when equal; unavailable objects keep their SHA and disabled status. Older history is lazy, collapsed and paginated against immutable per-repository cuts: Since workspace base (excluding defaults and merge base), Workspace base, Earlier history. A rewritten-away base stays marked not in current history above one Show history section. Expanding or paging never changes selections. Base and personal workspaces have no outgoing parent comparison or fork marker and expose one collapsed Show history section; landing has no counts. Keep Show on the same row as All changes and make wide file diffs keyboard-accessible horizontal scroll areas. Growing history must preserve complete commit ordering and captured cuts without repeatedly walking previous pages while its index is retained. History page entries carry commit metadata; resolve their changed-file paths and original patches when applying a selection. Bound retained server indexes, and preserve page contents after eviction or restart; cold reads may rebuild. Offscreen rendering optimizations preserve keyboard access, focus and selection.

<a id="g5.workbench.approval-readers"></a>
[I · g5.workbench.approval-readers]

Inspect review and Read report open read-only readers in the shared modal shell over the unchanged Approvals page, with ×, outside-click and Escape dismissal, internal scrolling, focus handling and restoration. Inspection is available before a report exists and shows captured request/policy/revisions or Not captured yet, plus failure stage/cause; IDs and transcript start collapsed. Reports preserve exact retained bytes, Markdown View/Raw and expandable frontmatter. Add no editing, resubmission or new-tab action; standalone report links remain valid. Choose human reviewer at random appears only for any-one requirements with human choices and selects only humans.

## Metadata

### g5.ui.terminology

- human-provided
- Source: [Ben, line 324](../../../docs/ben_new_ontology.md#L324).

### g5.ui.accessibility

- human-provided
- Source: [Ben, line 325](../../../docs/ben_new_ontology.md#L325).

### g5.ui.unavailable-actions

- human-provided
- Source: [Ben, line 326](../../../docs/ben_new_ontology.md#L326).

### g5.ui.workspace-lifecycle-state

- agent
- Source: [issue #835](https://github.com/G5Labs/G5-Lean/issues/835), requested for implementation by the owner.
- Design: [workspace lifecycle state](../../pr_ontology_diff_design/i835-unnecessary-vm-start-mutation/ontology-plan.md).

### g5.ui.capacity-wait-notice

- human-provided, agent
- Source: [G5-Lean#983](https://github.com/G5Labs/G5-Lean/issues/983), filed by the owner with the pop-up as the asked-for form.
- Agent choices (review of PR #998): once per Start rather than per reason or per poll, because a queued start's reason can alternate and a placement can bounce back to the queue; "once" is kept by the start intent's id for the life of the page load, so moving around the app and back does not repeat the notice while a reload may. On a phone the state banner sits directly above the lifecycle controls, which is where the reason stays in view there.
- Evidence: `WorkspacePageLifecycle.test.tsx` ("capacity wait notice"), `WorkbenchWorkspaceControls.test.tsx`, `WorkspaceVmStatusStrip.test.tsx`.

### g5.ui.api-parity

- human-provided
- Source: [Ben, line 327](../../../docs/ben_new_ontology.md#L327).
- Refined for workspace operations and child workspaces by [G5-Lean#710](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request), whose comments hold the UI-versus-VM capability audit; boundaries are [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) and [human exceptions](../concepts/security.g5.md#g5.security.agent-human-exceptions).
- Operations on the agent's workspace and its owner's other workspaces (children at first, [owner scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) since 2026-10-26), owner-readable chat reads, merge into the agent's workspace and stop by user are [implemented](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#implementation) for the VM credential. [Managed preview](../software_architecture/workspace_vm.g5.md#g5.preview.agent-control) is implemented without browser access. Files (list, read, write, delete, rename) reuse the UI's file operations, writes in the agent's own workspace and its owner's other workspaces within its scope, and are [audited](../concepts/security.g5.md#g5.security.agent-audit). Forking with a linked issue and the [member lookup](organizations.g5.md#g5.organization.agent-member-directory) are implemented ([increment 6](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-6-fork-with-an-issue-and-member-lookup)). Code gap: comments, settings and other reads remain public-API only.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 32 (UI-Q1).
- Starting a Base re-verification run is human-only for now, by the owner's decision of 2026-11-06 ([re-verification chat intent](../../chat_intent/periodical-reverification/request.md)): the VM's fork route passes no run, and the clone route that starts one takes only a person's session. Restructured into bullets the same day to meet the writing rules, with no change of meaning.

### g5.ui.launched-chat

- human-provided
- Source: [Ben, line 328](../../../docs/ben_new_ontology.md#L328).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 33 (UI-Q2).

### g5.ui.workbench-shell

- human-verified
- "Check how complex it is to remove all UIs that are not alignment ... otherwise do it in this session" ([chat intent](../../chat_intent/alignment-only-ui/request.md), 2026-10-23); what counts as "not alignment", and the phone layout and Base's phone title bar staying, are the agent's interpretation recorded there and in the [design](../../pr_ontology_diff_design/alignment-only-ui/ontology-plan.md), which lists every retired setting, surface, feature and environment variable. Implemented in PR #778; the ReposBar's Download code action moved to the Workspace Details header (`workspace-download-source`) so the one shell keeps it. Not re-homed: the classic Source Control section's paginated Git commit history with commit tabs (the workbench section offers file views, Manage current changes and the Diff Graph; its Commit History opens the revision dialog); `e2e/flows/source-control-history.spec.ts` retired, decision open. The tab strip carries `data-surface="chrome"` for themes that paint chrome. `useWorkbenchConfig` validates the payload shape (a malformed body is the error state, ribbon on built-ins; `useWorkbench.test.ts`).
- The former gate contract (`g5.workbench.gate`, 2026-10-09 "Set also the default for Workspace panels=alignment") lives in [PR #559's ontology](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workbench.g5.md).
- Found while moving the tests to the one shell: the view-param mirror in `WorkspacePage.tsx` cleared the landing's stale `?view=activity` from a render that predated a session selection's own URL write and dropped the `?session=` hint; it now waits while `pendingSessionSelectionRef` is set (`WorkspacePageChatNavigation.test.tsx`, `WorkspacePageBasePull.test.tsx`).

### g5.ui.workbench-config

- agent
- [WorkbenchConfig](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks complete fallback, an intentionally empty phase and starts tied to the clicked phase/version. YAML parsing and the full set of templates are outside the model.
- "remove the analyze button and actions from the workbench" (chat, 2026-10-29): the built-in catalog (`workbench_builtins/actions.yaml` and its generated client fallback `lib/workbenchBuiltins.generated.ts`) carries no Analyze menu, Completeness of implementation or Test plan action in any phase; the Analysis group keeps Find gaps, Review meeting and the hidden Extract decisions. A committed `.g5/workbench/actions.yaml` may still define its own.
- [Issue #1142](https://github.com/G5Labs/G5-Lean/issues/1142) (2026-11-06): a seeded start pinned the version of the click, which every docs commit (each question answer is one) made stale; the [design](../../pr_ontology_diff_design/issue-1142/ontology-plan.md) moves the seeded start's persona to the version current at the send and defers a content-derived version to the persona rework.

### g5.workbench.chat-actions

- human-provided
- "Don't submit chat prompts immediately when clicking the buttons like review meeting/build mock. Make the send button pulse instead after the text box is pre-populated with the prompt" ([chat intent](../../chat_intent/davide-ui-assorted/request.md), 2026-10-29); the seeded-start ownership and the one-pulse-for-every-seed rule are the agent's [design](../../pr_ontology_diff_design/davide-ui-assorted/ontology-plan.md). `handleWorkbenchChat` and `handleSendChatMessage` in `pages/WorkspacePage.tsx`; the pulse in `components/chat/ChatPanel.tsx` (`g5-send-pulse`). Tests: `pages/WorkspacePageAlignment.test.tsx` ("alignment ribbon actions", "Review meeting"), `components/chat/ChatPanel.test.tsx`.
- Discuss and a finding's chat action beside the document: the owner's [preview review of 2026-11-02](../../chat_intent/ontology-formal-verification/findings-triage.md#preview-review-2-november-2026), decision 27. `handleDiscussInChat` in `pages/WorkspacePage.tsx`; test `pages/WorkspacePageDocumentNavigation.test.tsx`.
- The send-time version: [issue #1142](https://github.com/G5Labs/G5-Lean/issues/1142) and its [design](../../pr_ontology_diff_design/issue-1142/ontology-plan.md); the owner's fix proposal in the issue is the authority. Tests in `pages/WorkspacePageAlignment.test.tsx` (seeded send after a revision change, stale refusal then a successful second Send).
- Seed ownership: [issue #1190](https://github.com/G5Labs/G5-Lean/issues/1190), where a Get Approvals prompt sent in one workspace filled another workspace's composer after a tab switch and the owner noted that every action on the shared prefill path is affected, and its [design](../../pr_ontology_diff_design/prefill-prompts-leak-into-another-workspace/ontology-plan.md). The pending seed request and the derived `chatComposerSeed` in `pages/WorkspacePage.tsx`; `onComposerSeedEmptied` and the failure restore in `components/chat/ChatPanel.tsx`. Tests: `pages/WorkspacePageSeedOwnership.test.tsx`, `components/chat/ChatPanel.test.tsx` ("composer seed").

### g5.workbench.review-prompts

- human-provided
- The prompts, the iteration option and the bold notice are from the 2026-11-01 [chat intent](../../chat_intent/prompt-tuning-for-button/request.md); the [design](../../pr_ontology_diff_design/prompt-tuning-for-button/ontology-plan.md) is the agent's. The prompts and headers are in `lib/reviewPrompts.ts`; the header is rendered by `components/chat/ComposerSeedHeader.tsx` and lives on the composer draft in `components/chat/ChatPanel.tsx`; the `verification` target is in `backend/g5-api-foundation/src/contracts/workbench.rs` and `workbench_builtins/actions.yaml`. Tests: `lib/reviewPrompts.test.ts`, `ChatPanel.test.tsx` (composer seed header), `WorkspacePageAlignment.test.tsx`, `WorkspacePageV2.test.tsx` (review copy arrival), `ActionRibbon.test.tsx`, `workbench_config.rs` and `g5-api/tests/api_workbench.rs`.
- Header text no smaller than the draft's is the owner's request of 2026-11-01.
- Children: [verification switch](#g5.workbench.review-prompts.switch).

### g5.workbench.review-prompts.switch

- human-provided
- Expands: [review prompts](#g5.workbench.review-prompts).
- Source: the 2026-11-01 [chat intent](../../chat_intent/prompt-tuning-for-button/request.md). The user chose greyed over hidden and off by default, so the very expensive verification run is not one click away for every member. Base re-verification joined it by owner decision 7 of the [re-verification chat intent](../../chat_intent/periodical-reverification/request.md) (2026-11-06). Split from the review prompts on 2026-11-06, to meet the writing rules and because Home and the run start share it.
- The organization switch is `verification_action_enabled`, registered in `ORG_ONLY_BOOLEAN_SETTINGS` (`config_service.rs`), which refuses it at a workspace or personal scope and requires a boolean; organization config writes already require organization administration, and members read it. The client reads it with `lib/verificationActionFlags.ts` in `WorkspacePage.tsx`'s action gate and `HomePage.tsx`; the server reads it for Home's status (`routes/home.rs`) and a run's start (the clone route); the Admin control is `components/org/OrgWorkbenchActionsSection.tsx`, whose row describes both uses. Tests: `verificationActionFlags.test.ts`, `OrgWorkbenchActionsSection.test.tsx`, `OrgAdminPageContent.test.tsx` (index order), `WorkspacePageAlignment.test.tsx` (greyed by default, enabled when on), `config_service.rs`, `g5-api/tests/api_org_admin_authority.rs` (`verification_action_switch_is_an_admin_set_org_only_boolean`), `HomePage.test.tsx` and `g5-api/tests/api_base_reverification.rs`.

### g5.workbench.find-gaps

- human-provided
- "The Find gaps button should also trigger the ontology planning before generating the questions" and the owner's two decisions on the proposal ([chat intent](../../chat_intent/find-gaps-planning/request.md), 2026-11-03), and the owner's 2026-11-04 instruction to write neither side of a contradiction between the documents; [design](../../pr_ontology_diff_design/find-gaps-planning/ontology-plan.md). Implemented in the `open-questions` and `create-implementation-plan` prompts of `workbench_builtins/actions.yaml` and its generated client fallback. The prompts' exact wording is the owner's to tune.

### g5.workbench.get-approvals

- human-provided
- Requested by the owner in [issue #1078](https://github.com/G5Labs/G5-Lean/issues/1078) (2026-11-04), whose prompt is the shipped one with field names corrected against the approvals response. The guidance-file precedence is the owner's decision. Three choices are the issue's proposed defaults and still await the owner's confirmation: no second confirmation before human requests, built in for every organization, and no organization switch.
- Facts relied on: `requestNeeds.outstanding` is what the Approvals panel's "Request outstanding approvals" sends (`requirement_status.rs`), and `randomHumanFrom` asks for one random pick, which the guidance replaces; `--reviewer` takes user UUIDs; `--retry` accepts only a review whose stage is `failed` (`approval_review_service.rs`); some definitions reject on purpose to route a change to people (for example the "UI changes" and "Ontology Alignment" gates). Waiting is a polling loop in the prompt; a `g5d workspace approvals --watch` like `g5d pr checks --watch` would make it sturdier and is unbuilt.
- The action and prompt are in `workbench_builtins/actions.yaml` (with the regenerated `lib/workbenchBuiltins.generated.ts`); the icon in `components/workbench/workbenchIcons.ts`; the Guided `ie-approvals` step (marks-only, so approval state never completes it) in `lib/journeySteps.ts`. Tests: `workbench_config.rs` (`builtins_validate_and_cover_the_v1_button_set`: placement, persona, context and the prompt's ordering and limits), `g5-api/tests/api_workbench.rs`, `ActionRibbon.test.tsx`, `journeySteps.test.ts`, `WorkspacePageAlignment.test.tsx` (seeded not sent, IE send, greyed for a reader). The label fits without breaking a word in every action-bar style; the fixed styles wrap one more row at narrower widths, as they are meant to, and `dynamic` is unchanged. The agent loop itself is prompt behaviour, not exercised by tests.

### g5.workbench.header-styles

- human-provided
- Styles and defaults from the 2026-10-09 [chat intent](../../chat_intent/header-style-settings/request.md) and [design](../../pr_ontology_diff_design/header-style-settings/ontology-plan.md); restored here after the 2026-10-13 rebuild dropped the block. `dynamic` became the default on 2026-10-20 ([chat intent](../../chat_intent/ribbon-settings-defaults/request.md)). `dynamic` from the 2026-10-19 [chat intent](../../chat_intent/dynamic-action-bar/request.md) and [design](../../pr_ontology_diff_design/dynamic-action-bar/ontology-plan.md).
- The settings are offered unconditionally since the [one-shell decision](#g5.ui.workbench-shell) (2026-10-23).
- `UserSettings.phaseBarStyle`/`actionBarStyle` (`lib/types.ts`, `lib/workbenchDefaults.ts`, normalized in `hooks/useUserSettings.ts`); selects in `components/settings/WorkspaceSettingsContent.tsx`; `PhaseChevrons` and `ActionRibbon` `variant`, the ribbon owning the dynamic density by measuring its own row; `WorkbenchWorkspaceControls` takes the ribbon's effective style. Tests: `ActionRibbon.test.tsx`, `PhaseChevrons.test.tsx`, `useUserSettings.test.ts`, `WorkspaceSettingsContent.test.tsx`, `WorkspacePageAlignment.test.tsx`, `e2e/flows/alignment-shell.spec.ts`. Not implemented by decision: an overflow menu for `simplified`; group collapse for `dynamic`.

### g5.workbench.ship-actions

- human-provided
- "Move create PR in Integration engineering, and add rebase in System engineering" ([chat intent](../../chat_intent/ribbon-settings-defaults/request.md), 2026-10-20). Re-run Verification before Create PR is from the 2026-11-01 [chat intent](../../chat_intent/prompt-tuning-for-button/request.md). Get Approvals between Create PR and Merge to parent, and the Guided approvals step, are from [issue #1078](https://github.com/G5Labs/G5-Lean/issues/1078) (2026-11-04). `rebase` target in `backend/g5-api-foundation/src/contracts/workbench.rs` and `workbench_builtins/actions.yaml`; `WorkspacePage.tsx` opens `WorkspaceMergeConfirmModal` for it; `lib/journeySteps.ts` moves the rebase step. Covered by `ActionRibbon.test.tsx`, `WorkspacePageAlignment.test.tsx`, `journeySteps.test.ts` and `e2e/flows/alignment-shell.spec.ts`. The October 15 workspace-wide amendment owns approval placement; this action-group integration preserves the October 20 Rebase/Create PR intent without restoring phase-specific Approval actions.

### g5.workbench.ship-approvals

- human-provided
- Source: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), Merge to parent section and refresh schedule.

### g5.workbench.ship-to-parent

- Approval blocker copy amendment: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974).
- human-provided (Ship to parent, Ship with agent always offered and the resolution of any blocker, the auto-merge switch); agent (which states count as blockers, primary/secondary emphasis, the unavailable state)
- Source: [ship-to-parent request](../../chat_intent/merge-when-ready/request.md#follow-up-two-ship-controls), [three controls](../../chat_intent/merge-when-ready/request.md#follow-up-three-ship-controls). `PrepareToShipModal.tsx` (Ship section), `MergeWhenReady.tsx` (the switch); `PrepareToShipModal.test.tsx`.
- Start and ship with agent: owner decision of 2026-11-05 ([decisions](../../chat_intent/ship-with-agent-wo-vm/request.md)); `PrepareToShipModal.tsx` reads the live gate; mock in the docs repository (`ship-with-agent-without-vm.md`, workspace 15159c69).
- Merge with override control: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) (secondary, warning-styled, beside Merge, only when an overridable requirement blocks) and [#550](https://github.com/G5Labs/G5-Lean/issues/550) (the GitHub override beside Merge); one control for both is agent design. Implemented · `MergeOverrideDialog.tsx` (`MergeOverrideAction`), offered from the server preview's `authority` only (`lib/mergeOverride.ts`).
- Permission-denial copy amendment: the owner's [#1104](https://github.com/G5Labs/G5-Lean/issues/1104) decision 4. A 403 from Ship to parent shows its own reason (members: the parent has no [approval requirement](workspaces.g5.md#g5.governance.pr.source-owner-merge), derived from the approvals section) and never the "Ship with agent resolves it" suffix. Implemented · `PrepareToShipModal.tsx`; `PrepareToShipModal.test.tsx` exercises the 403 path.

### g5.workbench.ship-to-parent.stop-vm

- The owner approved the [personal default in Execution policy before Idle timeout](../../chat_intent/stop-vm-after-auto-merge/request.md#personal-default-2026-11-10); the default and save/error states follow the reviewed mock.

- human-verified
- Expands: [Shipping to the parent](#g5.workbench.ship-to-parent).
- Source: [owner's approved layout and scope hint](../../chat_intent/stop-vm-after-auto-merge/request.md#shared-setting-layout-2026-11-09); [selected design](../../pr_ontology_diff_design/stop-vm-after-auto-merge/ontology-plan.md).
- Implemented in `PrepareToShipModal.tsx` and `MergeWhenReady.tsx`, following the dedicated-setting mock; [design and validation](../../pr_ontology_diff_design/stop-vm-after-auto-merge/ontology-plan.md).

### g5.ui.guided-toggle

- human-provided
- "Add a setting to enable/disable the guided experience in the left menu (default enabled)" ([chat intent](../../chat_intent/ribbon-settings-defaults/request.md), 2026-10-20). `UserSettings.showGuidedJourney` (`hooks/useUserSettings.ts`), the switch in `components/settings/WorkspaceSettingsContent.tsx`, the gate in `WorkspacePage.tsx`; covered by `useUserSettings.test.ts`, `WorkspaceSettingsContent.test.tsx` and `WorkspacePageAlignment.test.tsx`.
- "Turn off Guided experience by default" and "If the guided entry is off remove the last step from the Onboarding tour" ([chat intent](../../chat_intent/davide-siderail/request.md), 2026-11-09). `lib/defaults.ts`; the six-step tour in `components/onboarding/OnboardingTour.tsx` (`stepTotal`, finishing on the workspace route); covered by `useUserSettings.test.ts`, `WorkspaceSettingsContent.test.tsx` and `OnboardingTour.test.tsx`. The e2e flows pin the Guided section on in `e2e/auth.setup.ts` because the Guided flows drive it.

### g5.ui.guided-collapsed

- human-provided
- "when a workspace is forked/started for the first time, no need to have the guided journey expanded (this was a previous decision which I'm reverting now)" (chat, 2026-10-29). `defaultCollapsedSections` in `components/tree/TreeNode.tsx` seeds every section collapsed; the rail intro flag and the remembered-collapse browser key are gone. Tests: `components/tree/TreeNode.sectionOrder.test.tsx` ("Guided section"), `e2e/flows/guided-journey.spec.ts`.

### g5.ui.journey-evidence

- agent
- [JourneyEvidence](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks that a pending mark stays with its original workspace and starting an activity cannot count as completion. Competing marks, event ordering and each completion evaluator are outside the model.
- Skip (2026-10-20, Mock U [design](../../pr_ontology_diff_design/guided-journey/ontology-plan.md)): migration `098_workspace_journey_skipped.sql` (`skipped` column, done-or-skipped check), `JourneyMarkDTO.skipped` and `SetJourneyMarkRequestDTO.skipped` in `g5-api-foundation/src/contracts/workbench.rs`, `WorkspaceService::set_journey_mark` compares `(done, skipped)` and refuses both, the timeline's `journey_step` carries `skipped`; `lib/journeySteps.ts` reads a skipped status that leaves the count and the next step while evidence still wins. Covered by `tests/api_workspace_journey.rs` (skip, idempotent, refused together, undone), `lib/journeySteps.test.ts`, `WorkspaceActivityPanel.test.tsx`.

### g5.ui.guided-panel

- human-verified
- "let's go ahead and implement mock U" (2026-10-20) after the mock-based review of M–U; the rules behind U come from the curated UI skills study recorded in `mocks/README-mocks-m-t.md` of the [design folder](../../pr_ontology_diff_design/guided-journey/ontology-plan.md). `components/guided/GuidedJourneySection.tsx` (rows, ring control, fold, status words, summary, skeleton, error lines, live region, roving arrows), `index.css` (`--g5-ease-out`, `--g5-ease-move`, `--g5-d-*`, `g5-guided-enter`, `g5-guided-breathe`, reduced-motion rules), `hooks/useTasks.ts` `useWorkspaceActiveSessionIds`, `pages/WorkspacePage.tsx` (mark failure state, the sessions started per action, retries, owner name). Tests: `GuidedJourneySection.test.tsx`, `WorkspacePageAlignment.test.tsx`, `e2e/flows/guided-journey.spec.ts`. Limits: Running is page memory (sessions do not record the action that started them), so a reload forgets it; the Undo window is client time; no "+N" roster disclosure yet (the roster component is shared).

- PR #448 integration retains these panel states and owner/reader controls; approval placement follows [workspace-wide gates](#g5.workbench.gates), so phase roster rows remain absent. The integrated 4,891-case frontend suite on main `60e99ccce`, full frontend types and production build pass; exact-head Linux browser/receipt qualification remains pending.

### g5.ui.home-attention

- Source: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)) section 5: workspace-only approval caption; the owner's prepared patch.
- "the open questions should be clickable to go to the document directly" ([chat intent](../../chat_intent/home-tile-defaults/request.md), 2026-10-23): the row name in `HomePage.tsx` links where the Answer / Review action does.
- agent
- Source: [workbench design](../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md).
- [HomeAttention](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/platform/README.md) checks the captured audience, owned questions and honest page counts, including failed reads and retries. It assumes correct approval/question inputs and does not check later access revocation or cross-page consistency.

### g5.ui.home-attention-order

- human-verified
- Owner answers of 2026-10-20 to issue [#664](https://github.com/G5Labs/G5-Lean/issues/664) in the Project Docs question set `docs/b66a2daf-1314-439d-b6d1-c1f8c6bacc8c/questions/home-attention-sort.g5q.json` (docs repository, `open-questions.md` beside it): Newest for everyone (Q1), the "Attention updated" time (Q2), any stable tie order (Q3) and all outstanding items (Q6). [Design](../../pr_ontology_diff_design/issue-664/ontology-plan.md).
- `HomeAttentionOrder` and the order-carrying cursor in `home_service.rs` own the comparison; `actionable_for_org` (approval heap), `materialize_home_question_candidates` (workspace walk, sets before docs under Newest) and `open_project_doc_question_sources_bounded` take it; `routes/home.rs` reads `order`. Evidence for this block and the order blocks below: `api_home::organization_home_attention_newest_is_oldest_reversed_across_every_page` (two approval requests made together, so tied, tied docs and sets in one workspace and a second workspace, walked at limits 1, 2, 3 and 50, both orders, both kinds) and `…_cursor_never_crosses_orders`, plus `home_service::tests`. The HomeAttention model above does not cover order.

### g5.ui.home-attention-order-reverse

- agent
- Chosen within Q3's "any stable order", so Newest and Oldest share one tie order. `HomeAttentionOrder` reverses one total key `(actionable_at, kind_rank, workspace_id, id)`.

### g5.ui.home-attention-order-cursor

- agent
- The cursor carries its order; a cursor from the other order is refused with 422 (`…_cursor_never_crosses_orders`).

### g5.ui.home-attention-order-api-default

- agent
- Keeps the pre-#664 API sequence for clients that name no order; a cursor without an order continues Oldest, and an unknown order is refused with 400 (`…_cursor_never_crosses_orders`). `useHome.ts` always sends `order`.

### g5.ui.home-attention-order-control

- human-verified
- Same question set: the Workspaces pill dropdown (Q8), the names "Newest" and "Oldest" without "first" (Q9, the owner's revision) and both layouts (Q10); reviewed with the clickable mocks under `mocks/` in that Project Docs folder. Open agent assumption A3 in the design: no extra announcement after a switch.
- `HomeSortSelect` in `components/home/HomeTileParts.tsx` is both tiles' sort pill. Evidence for the control blocks below: `HomePage.test.tsx` "Needs Your Attention order" (both layouts, URL, filter, Show more, switch-back to page one, failed switch with Retry) and `e2e/flows/alignment-shell.spec.ts` "Needs Your Attention opens on Newest…", which passed first time in the CI e2e lane (build `a8cf7c82`, commit `567e7913`).

### g5.ui.home-attention-order-url

- human-verified
- Same question set: in the page address like Workspaces, surviving reload and links, with a fresh visit showing the default (Q4). `pages/HomePage.tsx` reads and writes the URL.

### g5.ui.home-attention-order-url-default

- agent
- Follows the Workspaces sort's URL convention; the parameter is `attentionSort` (`pages/HomePage.tsx`).

### g5.ui.home-attention-order-filter

- human-verified
- Same question set: keep the chosen order (Q5).

### g5.ui.home-attention-order-reset

- human-verified
- Same question set: return to the first page (Q7). `pages/HomePage.tsx` drops the chosen order's cached pages before switching; `hooks/useHome.ts` keys the read by order.

### g5.ui.home-attention-order-scope

- human-verified
- Same question set: Home only (Q11).

### g5.ui.home-attention-order-agents

- human-verified
- Same question set: UI only for now (Q12). The backend-parameter clause applies the existing [API parity](#g5.ui.api-parity) rule rather than adding one.

### g5.ui.home-workspaces

- Requirement counts amendment: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974).
- All-before-the-tour default: [onboarding tour chat intent](../../chat_intent/onboarding-tour/request.md), 2026-10-26; `HomePage.tsx` passes the default from the user record to the three tiles.
- "Everything should default to your stuff", "everything should be clickable as a name", "instead of the open button put a stop button", "put only the avatar/initials for the owner, with a tooltip" and "wrap the workspace name if too long instead of truncating" ([chat intent](../../chat_intent/home-tile-defaults/request.md), 2026-10-23): `HomeIssuesTile.tsx` (default `me`, search in the toolbar band), `HomeComputeTile.tsx` (default `mine`, name link, Stop), `HomeWorkspacesTile.tsx` (avatar-only owner cell, wrapping name); `HomePage.test.tsx` and the mock-preview `home.spec.ts` assert the defaults.
- Rename sat between Delete and Fork from PR #756 until the owner moved it to Workspace details ([follow-up in the rename request](../../chat_intent/rename-workspace/request.md), 2026-10-24; [design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md)); `HomePage.test.tsx` asserts the row offers Delete and Fork only.
- "add a little search bar in the homepage for the issue panel (there was a full one that was removed recently, but we don't want it that big)" ([chat intent](../../chat_intent/home-issue-search-tab-icons/request.md), 2026-10-20): the compact field in `HomeIssuesTile.tsx` reuses the retired search's pause, Enter and unfinished-qualifier rules (`lib/issueSearchQuery.ts`); `HomePage.test.tsx` ("searches typed terms") covers Enter, clearing from the cache and the qualifier pause. The search text is not URL state and is lost through a Connect GitHub round trip.
- human-verified
- [Human instructions and agent interpretation](../../chat_intent/home-workspaces-tile/request.md), 2026-10-15, with the reviewed mock variants and the [design](../../pr_ontology_diff_design/home-workspaces-tile/ontology-plan.md). The human chose the table, the flat list, the three-gate approvals cell with all green when complete, the chevron-with-dot control and dropping the Activity column and issue chip; URL state for filter and sort follows the design guidance read during the mock review. `components/home/HomeWorkspacesTile.tsx`, `ApprovalGates.tsx`, `HomeWorkspaceJobsPanel.tsx` and `pages/HomePage.tsx`; `pages/HomePage.test.tsx` ("alignment Home tiles", "Workspaces tile") and `e2e/flows/alignment-shell.spec.ts` (the Home tiles flow, run by CI with the alignment gate on). The owner name depends on the members endpoint being readable by every member; the working flag is the list DTO's running-task count.
- Reconciled with the [issue447 human approval direction](../../chat_intent/qa-review-prototype/request.md): phases recommend when to request workspace-wide approvals and do not partition target merge requirements. The table therefore shows one workspace-wide status; its focused Home regression covers aggregate approval/rejection, no requirements and failed-read retry.
- [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) (human-verified) adds the personal-workspace exclusion from the table; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md). Implemented; locations and tests are in the [personal workspace metadata](personal_workspace.g5.md#metadata).

### g5.ui.home-paging

- human-provided
- "Add pagination to all the four cards and limit them to 5. Add a setting in the settings page to set the pagination to a higher number" ([chat intent](../../chat_intent/home-workspaces-tile/request.md)). `UserSettings.homeRowsPerTile` (`hooks/useUserSettings.ts`, `components/settings/WorkspaceSettingsContent.tsx`), `HomeShowMore` in `components/home/HomeTileParts.tsx`, `hooks/useHome.ts` (attention `limit`); tests in `useUserSettings.test.ts`, `WorkspaceSettingsContent.test.tsx`, `HomePage.test.tsx`.

### g5.ui.home-reverification

- human-provided
- user-facing-behavior
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decisions 1, 5, 6 and 7. Greying the button with a reason for people who cannot fork Base, rather than hiding it, is the agent's choice, following the switch's greyed-over-hidden choice. Design: [Base re-verification plan](../../pr_ontology_diff_design/periodical-reverification/ontology-plan.md).
- Defines the `g5.ui.home-reverification` level for [ID levels](ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [dialog](#g5.ui.home-reverification.dialog).
- Implemented: `components/home/HomeReverificationStatus.tsx`, `hooks/useBaseReverification.ts`, `lib/baseReverification.ts`; `pages/HomePage.tsx` passes the switch, read as the workspace page reads it. Tests: `HomeReverification.test.tsx`, `HomePage.test.tsx` (nothing and no status read unless the switch is exactly on; the line above the tiles), `e2e/mock-preview/base-reverification.spec.ts` (off by default, on from Admin) and `e2e/flows/base-reverification.spec.ts` (the live stack).

### g5.ui.home-reverification.dialog

- human-provided
- user-facing-behavior
- Expands: [Home re-verification](#g5.ui.home-reverification).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decisions 3, 8, 9 and 11. The choice labels, the rows' details and listing runs beside pull requests are the agent's wording of those decisions, as the [plan](../../pr_ontology_diff_design/periodical-reverification/ontology-plan.md#status) gives them. The owner tunes the explanation's exact text.
- Implemented in `components/home/HomeReverificationDialog.tsx`: the start posts the ordinary fork with `reverification`, keeps the idempotency key after a network error or 5xx, still offering that choice once its run reads in progress so a retry replays the same fork, rotates the key after a refusal, and stays open on its last status if the switch goes off. Tests: `HomeReverification.test.tsx` (lists, paging, caps, refusals, a replay after an unknown outcome, switch turned off meanwhile, the landing and its fallback when the response names no run) and both e2e specs.

### g5.ui.settings-page

- human-provided
- "implement the new settings page, just for the user and for the org, but not for the workspace … the only change in this PR should be the UI for the settings, showing the menu on the left and the new revamped elements in the scrollable page" ([chat intent](../../chat_intent/settings-page-ui/request.md), 2026-10-26), with the section order, Admin index links and unchanged save models answered there; mocks and the eventual target state in [issue #746](https://github.com/G5Labs/G5-Lean/issues/746). `components/settings/SettingsPageShell.tsx` (shell with the section registry, section, row, scroll-spy, view-scoped `?section=`, deep-link re-alignment while content settles until the person scrolls), `components/settings/WorkspaceSettingsContent.tsx`, `components/org/OrgAdminPageContent.tsx`, `OrgRuntimeConfigSection.tsx`, `OrgIssuesSettingsSection.tsx`, `OrgWorkbenchActionsSection.tsx` (Workbench actions, added 2026-11-01 for [Re-run Verification](#g5.workbench.review-prompts)); `pages/WorkspacePage.tsx` drops `section` with the view hint it belongs to; `pages/HomePage.tsx` and `pages/ForkPickerPage.tsx` wire the strip's settings button to `/:org/settings` (user request 2026-10-26). Tests: `SettingsPageShell.test.tsx`, `WorkspaceSettingsContent.test.tsx`, `OrgAdminPageContent.test.tsx`; `e2e/flows/provider-accounts.spec.ts` (order, index, click writes `?section=`; unexecuted here: no local e2e stack). Codex review round 1 (2026-10-26) found the hidden narrow-screen index, the cross-sheet `?section=` leak, the scrollbar-drag and focus gaps and the ungated Admin Accounts section; all fixed in the same PR.
- human-provided, 2026-10-31: a Notifications section was added to Settings for the account-wide [approval-request email toggle](users.g5.md#g5.user.notifications), by owner instruction to put it in Settings rather than the Account flyout ([issue #991](https://github.com/G5Labs/G5-Lean/issues/991)). `WorkspaceSettingsContent.tsx` section `id="notifications"`, writing `PATCH /users/me` and invalidating `current-user`.

### g5.ui.sign-in-landing

- human-provided
- "after login the first page shown should be the homepage" ([chat intent](../../chat_intent/onboarding-tour/request.md), 2026-10-27). `App.tsx` `getDefaultAuthenticatedPath`; `App.test.tsx` ("dedicated application root").
- agent, 2026-10-27: rebasing issue #776 over this block replaced "lands on organization creation" with the no-organization page, since #776 removed self-serve creation ([organization creation](organizations.g5.md#g5.organization.creation)).

### g5.ui.onboarding-tour

- human-provided
- User request (2026-10-27) after PR #858: a "Create issue manually" button on step 5 that highlights the Issues tile's Create, the step 5 card no longer covering the started chat, a card for step 6's fork dialog ("start now or work on it later"), and step 7 highlighting the sidebar's compass instead of the checklist's first step. `OnboardingTour.tsx` (`BESIDE_CHAT`/`BESIDE_DIALOG` placement, `overOverlay`, `secondary`), `lib/onboarding.ts` (`ONBOARDING_ISSUE_MANUAL_COPY`, `ONBOARDING_START_DIALOG_COPY`).
- [Chat intent](../../chat_intent/onboarding-tour/request.md) and [design](../../pr_ontology_diff_design/onboarding-tour/ontology-plan.md), 2026-10-26. `lib/onboarding.ts` (steps, marker, defaults), `components/onboarding/OnboardingTour.tsx` (controller, spotlight, cards, resume pill), `hooks/useAuth.tsx` (`applyProfile`), `pages/HomePage.tsx` (tile defaults, composer seed), `components/settings/WorkspaceSettingsContent.tsx` (Tour row). Tests: `lib/onboarding.test.ts`, `components/onboarding/OnboardingTour.test.tsx`, `pages/HomePage.test.tsx`, `e2e/flows/onboarding-tour.spec.ts`.
- agent, 2026-10-27: rebasing issue #776 over this block limited the tour to tenant organizations. The operator Home is the platform console, which carries none of the tour's anchors, and a blocking welcome card there would stop operators from using it. `OnboardingTour.tsx` treats operator-organization routes as hosting no step and addresses only tenant organizations; `OnboardingTour.test.tsx` ("hosts no step in the operator organization").

### g5.ui.sidebar-default

- human-provided
- "Change the default mode of 'Left sidebar' to 'Expand on hover'" ([chat intent](../../chat_intent/ribbon-settings-defaults/request.md), 2026-10-20). `lib/defaults.ts`; covered by `useUserSettings.test.ts` and `WorkspaceSettingsContent.test.tsx`. The e2e flows pin the full sidebar in `e2e/auth.setup.ts` because they click the stacked panel.
- "Let's add one more [mode] that is like full sidebar, but when collapsed it shows the icons like in Icon rail only + [the Expand button]. When collapsed and a button is clicked, it expands like full sidebar", then "make collapse-to-rail the default option" ([chat intent](../../chat_intent/davide-siderail/request.md), 2026-11-09). `lib/defaults.ts`; `SidebarDisplayMode` `collapse-to-rail` in `lib/types.ts`; the collapsed rail and its `onExpand` in `components/tree/TreeNode.tsx` (`LeftSidebar`), rendered by `pages/WorkspacePage.tsx` in place of the collapsed edge; covered by `Sidebar.test.tsx`, `TreeNode.sectionOrder.test.tsx`, `WorkspaceSettingsContent.test.tsx` and `WorkspacePageAlignment.test.tsx`.

### g5.workbench.tabs

- human-verified
- "change the icon on the top tabs to be the icon of BE/SE/IE instead of the 'Databricks' icon" ([chat intent](../../chat_intent/home-issue-search-tab-icons/request.md), 2026-10-20): `WorkspaceTabStrip.tsx` reads the route organization's workspace list (`workspacesQueryOptions`, the query Home shares; a phase move already invalidates it) and renders `PHASE_ICONS`; `WorkspaceTabStrip.test.tsx` covers the three cases (listed with a phase, listed without one, not listed).
- Case-insensitive tab identity (issue #736, "Base tab doesn't have any icon", 2026-10-23): the settings redirects and default landing route `/:org/base` while the workspace is listed as `Base`, and the server resolves names case-insensitively, so an exact-name match found no phase and could open a second Base tab. `lib/workspaceTabs.ts::workspaceTabKey` folds the name for identity and memory keys (`useWorkspaceTabs.tsx`), while a visit stays keyed by the name as routed so a link under another spelling is a fresh open of the same tab (a CodeRabbit finding on PR #737); the strip matches the list by folded name and shows the listed name. Covered by `workspaceTabs.test.ts` and the issue-#736 and "another spelling" cases in `WorkspaceTabStrip.test.tsx`.
- "remove also the create organization button in /g5labs/fork" ([chat intent](../../chat_intent/ribbon-settings-defaults/request.md), 2026-10-20): `pages/ForkPickerPage.tsx` dropped the button and dialog; "instead of the create organization button, let's have a create workspace button" (same day): a New workspace button opens `CreateWorkspaceDialog` after the organization switch, covered by `ForkPickerPage.test.tsx`.
- Carries the tab-return amendment and its landing exception from [main's contract and evidence](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workbench.g5.md), PR #559. `useWorkspaceTabs.tsx`, `WorkspacePage.tsx` and their strip/page tests cover session snapshots, workspace identity, closed-tab forgetting and fresh-open versus return behavior. Browser-session memory is not reload persistence; a workspace deleted elsewhere is handled by [g5.workbench.tabs.deleted](#g5.workbench.tabs.deleted).
- Center surface URLs (issue #561, [design](../../pr_ontology_diff_design/center-surface-urls/ontology-plan.md)): the path/`?view=` split and root fallback are agent interpretations of the issue. `tabToPath`/`tabToViewParam` and `WorkspacePageAlignment.test.tsx` cover the URL forms, Base refusal and unknown identities; `workspace.spec.ts` lands Pull Requests and guide links in the browser.
- The human requested complete Job/AQL removal with disposable job history and manual host/worker reprovisioning. Task changes replace Job surfaces and resolve directly by task ID outside the loaded history page. `api_workspace_tasks.rs`, `useTasks.test.ts`, `WorkspacePageV2.test.tsx` and `e2e/flows/tasks.spec.ts` cover authorization, pagination and direct task navigation.
- Cross-tab close (issue #461, human-requested fix after a QA repro): only an arrival at a workspace route opens its tab; the transient tab for a workspace closed in another browser tab is an agent interpretation recommended by an independent Fable review over persisted tombstones or a local overlay. `WorkspaceTabStrip.test.tsx` covers the storage-event close, in-workspace navigation, leaving and reopening, and closing the transient tab.
- Organization scope follows [PR #567's contract and evidence](https://github.com/G5Labs/G5-Lean/blob/d679e862f75bad311af814cf0558a8f75025581c/.g5/ontology/features/workbench.g5.md): the route owns the scope, other organizations' tabs remain stored, and the picker checks membership. Upstream strip, picker and route tests cover that scope; backend validation reserves `fork`. A workspace already named `fork` remains shadowed by the picker until renamed; this rebase does not repair that upstream limitation.
- [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) (human-verified) adds the personal-workspace `me` and `users` route reservations; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md). Implemented; locations and tests are in the [personal workspace metadata](personal_workspace.g5.md#metadata).
- Personal-tab owner avatar (human-verified): the owner asked for the owner's avatar on personal workspace tabs and confirmed the open assumptions — 14 px, both the caller's own `/:org/me` and other members' `/:org/users/:handle`, decorative with the display-name tooltip — through the `personal-tab-avatar` question set and chat (2026-10-29); [mock](../../../docs/mocks/personal-tab-avatar/README.md). `WorkspaceTabStrip.tsx` (`OwnTabAvatar` from `useAuth`, `MemberTabAvatar` resolving the member through `personalWorkspaceQueryOptions` and the organization roster) reuses `IdentityAvatar`, whose image is `draggable={false}` so a tab-reorder drag begun on it is not cancelled (a Codex review finding). Covered by the personal-tab-avatar cases in `WorkspaceTabStrip.test.tsx`.

- October 19 integration: task-output changes keep the canonical task path without the workspace-level `?view=changes` hint. An older task absent from the first history page reproduces the Cloud failure before correction; both URL forms and all 225 affected navigation cases pass locally. [Current integration evidence](../../../docs/review-flow.md#october-19-latest-main-integration); real task-history browser qualification awaits the next published head.


### g5.workbench.tabs.rename

- agent
- Agent interpretation of the [rename request](../../chat_intent/rename-workspace/request.md) ([design](../../pr_ontology_diff_design/rename-workspace/ontology-plan.md)): tabs are stored by name, so a rename must rewrite them; case-only renames follow [g5.workbench.tabs](#g5.workbench.tabs)' case-insensitive identity. `renameWorkspaceTab` (`lib/workspaceTabs.ts`, covered by `workspaceTabs.test.ts`) and the provider's `renameTab` (`useWorkspaceTabs.tsx`) rewrite the tab and its session memory; `WorkspacePage.tsx` calls it when a former name resolves (`WorkspacePageFormerName.test.tsx`) and Home after a rename. The provider's memory re-keying is not covered by an executed test.

### g5.workbench.tabs.deleted

- human-verified
- Expands: [g5.workbench.tabs](#g5.workbench.tabs).
- [Issue #837](https://github.com/G5Labs/G5-Lean/issues/837) "Deleting a workspace should also close its tabs" (2026-10-26). The deleting page and Home row already closed the tab; the server-answer paths and the list reconciliation, including the once-seen rule, are agent design within that request. `WorkspacePage.tsx` closes or unlists the routed tab from its resolution and detail reads (`WorkspacePageDeleted.test.tsx`); `useWorkspaceTabs.tsx` reconciles the other tabs with the organization's workspace list (`WorkspaceTabStrip.test.tsx`, "workspaces deleted elsewhere"). Gaps: a tab never seen listed in this browser session stays until it is opened; a deletion is noticed only on the next read of the workspace (the page's detail poll) or of the list (focus, Home's poll).

### g5.workbench.tabs.reorder

- human-provided
- [G5-Lean#820](https://github.com/G5Labs/G5-Lean/issues/820) (2026-10-25, its requested behavior, acceptance criteria and implementation considerations); [design](../../pr_ontology_diff_design/issue-820/ontology-plan.md).
- `lib/workspaceTabs.ts::moveWorkspaceTab`, the provider's `moveTab` (`hooks/useWorkspaceTabs.tsx`) and `useTabReorder` in `components/layout/WorkspaceTabStrip.tsx`, with Home outside `workspace-tab-scroller`. Tests: `workspaceTabs.test.ts`, the "reordering" cases of `WorkspaceTabStrip.test.tsx` (first and last places, persistence across a remount, fixed Home, active-tab and return-state preservation, close after reorder, click after a drag, middle-click and close, personal workspace, transient tab, edge auto-scroll) and `e2e/flows/alignment-shell.spec.ts` ("workspace tabs reorder by dragging beside a fixed Home and keep their order", passed locally in Chromium against a native stack).

### g5.workbench.tabs.reorder.keyboard

- agent
- Proposed in the [design](../../pr_ontology_diff_design/issue-820/ontology-plan.md) to serve [g5.ui.accessibility](#g5.ui.accessibility). `moveByKey` in `WorkspaceTabStrip.tsx`; the keyboard case in `WorkspaceTabStrip.test.tsx` and the browser flow.

### g5.workbench.tabs.reorder.anchoring

- agent
- The issue's latest-list and no-resurrection requirements, realized in the [design](../../pr_ontology_diff_design/issue-820/ontology-plan.md). `moveWorkspaceTab` and `moveTab`; `workspaceTabs.test.ts` (followers, other organizations, missing tabs, personal routes) and the transient-tab case in `WorkspaceTabStrip.test.tsx`.

### g5.workbench.tabs.reorder.cancel

- agent
- The issue asks that a drag can be cancelled; the triggers are agent choices. `useTabReorder`; the Escape, pointer-cancel and mid-drag storage-event cases in `WorkspaceTabStrip.test.tsx`, Escape in the browser flow.

### g5.workbench.tabs.reorder.touch

- agent
- `onPointerDown` in `WorkspaceTabStrip.tsx` ignores touch pointers. Not covered by an executed test.

### g5.workbench.tabs.reorder.eviction

- agent
- Existing `addWorkspaceTab` cap behaviour, kept unchanged and recorded because a reorder changes which tab it evicts.

### g5.workbench.landing

- Source: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): the selected personal banner A above the summary, with the rest of the landing unchanged.
- human-provided: the owner's 2026-10-30 instruction retired the rail's Explainers section and the Project Docs source-category row (All / User / Auxiliary / Unlabeled): explainers live in the Project Docs folder like mocks, and rows keep their source markers. `TreeNode.tsx` lists one docs pool by folder; persisted orders that carried `explainers` migrate as uncustomised or drop the id.
- human-provided: owner decision of 2026-10-26 boxes every person-written Activity message and keeps events plain; [the continuing design](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md#content-and-navigation-changes) records the refinement.
- human-verified: the owner's [approved Activity cleanup](../../chat_intent/issue-816-workspace-landing/activity-cleanup.md), with the surrounding controls explicitly excluded. [Continuing design and mocks](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md#activity-cleanup-and-filter-proposal--october-25-follow-up). The earlier page-level comment-navigation links are superseded; comment-entry owners and recovery remain.
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) replaced the status header and its phase/approval badge with the landing summary, task and implementation sections, and the filter checkboxes with pills; the composition is split into the `g5.workbench.landing.*` blocks.
- [Issue 704 follow-up](../../chat_intent/issue-704/approvals-follow-up.md) consolidates general comment creation in Reviews; the agent-selected split places GitHub issue comment creation under Issues. Existing discussion, reply and publication-recovery owners remain.
- human-verified
- [PR #567's Activity amendment and evidence](https://github.com/G5Labs/G5-Lean/blob/d679e862f75bad311af814cf0558a8f75025581c/.g5/ontology/features/workbench.g5.md) retains phase recording while removing its display. `activityFilters.ts` and `WorkspaceActivityPanel.tsx` exclude phase changes before grouping; backend timeline projection remains. Upstream component/page tests and the browser flow distinguish the visible status header from hidden events; the browser filter round trip allows an empty visible feed. No new browser execution is claimed here.
- PR448 [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) adds the shared landing/task and implementation readers while preserving main’s organization-scoped tabs and durable-but-hidden phase events. Owner/nonowner component and browser evidence remains scoped in [the existing design](../../../docs/review-flow.md).
- The 2026-10-20 [activity design](../../pr_ontology_diff_design/activity-current-rail/ontology-plan.md) ([chat intent](../../chat_intent/activity-current-rail/request.md)) established newest-first ordering. Its composer-above-record placement is superseded by issue 704's dedicated comment destinations. `workspace_activity.rs` orders and pages conversations on `chat_sessions.updated_at`; `compose_page` returns newest first with the last-entry cursor. `WorkspaceActivityPanel.tsx` retains that order, pending-publication recovery above the list and Load earlier below. Ordering evidence remains in `api_workspace_timeline.rs`, `api_external_issues.rs` and `workspace_activity.rs`; current `WorkspaceActivityPanel.test.tsx` checks destination links and pending receipts instead of an Activity composer. `WorkspaceIssuesPanel.test.tsx` checks preserved GitHub draft/operation identity and refreshed admission after unlinking. The native Chromium journey verifies Activity navigation to persisted workspace comments in both layouts. The moved GitHub composer is exercised locally with mocked HTTP; no live GitHub publication is claimed.
- Merged 2026-10-20 with PR448: newest-first order kept by human decision; PR448's workspace discussion, reply, approval-request and workspace-created entries page newest first with the rest, and its single page scroll, phase filter, review-snapshot refresh and comment anchors carry over (its scroll-to-bottom and scroll restoration are dropped because earlier entries load below). Approval titles no longer name a phase. `api_workspace_timeline.rs` and `api_external_issues.rs` expectations adapted to newest first.

### g5.workbench.landing.activity-filters

- human-verified: the owner approved implementation of the [mocked filter transitions](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md#filter-interaction); [chat direction](../../chat_intent/issue-816-workspace-landing/activity-cleanup.md). The Current exception is inherited from `g5.ui.activity-current`.

### g5.workbench.landing.approval-events

- human-provided
- Source: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), Workspace Activity.

### g5.workbench.landing.summary

- Requirement counts replace the earlier state words: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974).
- human-provided: [issue #841](https://github.com/G5Labs/G5-Lean/issues/841) supersedes #816’s fork comparison and captured-head URL links, removes the parent-advanced note and corrects Code/Docs destinations.
- human-provided
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) section 1; [design](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md). The original parent-advancement disclosure came from [the review-flow design](../../../docs/review-flow.md) and is superseded by #841.
- human-verified: known-zero category simplification is part of the [approved cleanup](../../chat_intent/issue-816-workspace-landing/activity-cleanup.md).

### g5.workbench.landing.task

- Source: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)) sections 12–13: Primary Issue label and its conditional disclosure.
- human-provided
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) section 2; [design](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md). The task source order predates the issue and is retained by its instruction to preserve task source semantics.

### g5.workbench.landing.implementation

- Source: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)) section 12: Work Description label.
- human-provided
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) sections 3 and 5 (G5 without GitHub); [design](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md). The open-else-latest PR choice is an agent decision.
- human-provided: owner follow-up on 2026-10-27 removes the Commit History link and asks whether the SHA alignment covers the Activity card and full-description reader.
- Rows for [repository pull requests](../software_architecture/git.g5.md#g5.git.pr-repository) amended by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented in `ImplementationSection.tsx` (local status and the G5 PR's `PR` name, Unchanged), tested in `WorkspaceLanding.test.tsx`.

### g5.ui.deleted-workspace-prs

- human-provided
- The owner, 2026-10-30: "UI and the tool should return the same things. To avoid confusing the user in the UI, historical PRs could be grouped separately - or left off if the goal of the UI is to allow actions on the PRs" ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#deleted-workspaces-2026-10-29)). That action surfaces need no filter, because they already request open PRs (`PullRequestList.tsx`, `WorkspacePage.tsx`, `useHomeWorkspaceSignals.ts`), is agent observation.
- Implemented: summaries carry `sourceWorkspaceDeleted`/`targetWorkspaceDeleted` (`lib/types.ts`); deletion closes the PRs, so the open-PR surfaces leave them off. No UI surface lists PR history today (the landing shows one PR, open else latest), so the grouping has no surface to apply to yet.
- Gap: nothing in the UI reads the deleted-side flags yet, so the landing's latest-PR fallback and Activity's PR events can show a PR closed by its target's deletion without naming the deleted workspace.

### g5.workbench.landing.status

- human-provided
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) section 5.

### g5.ui.activity-current

- human-verified
- "add an 'activity' for ongoing chats/tasks that are ongoing, so mark them as 'current' and keep them always at the top" ([chat intent](../../chat_intent/activity-current-rail/request.md), 2026-10-20); the ongoing predicate, the separate read, the show-once rule and the ordinary-row presentation (mock B) are agent decisions in the [design](../../pr_ontology_diff_design/activity-current-rail/ontology-plan.md).
- `workspace_activity.rs::current_entries` (conversations joined to a queued/running/waiting agent task, carrying `taskStatus`; preview and command tasks in those states; newest first, no cursor), served by `GET /workspaces/:id/activity/current` (`external_issues.rs::get_workspace_activity_current`, `WorkspacesRead`, manifest arm in `route_auth_static.rs`); client `useWorkspaceActivityCurrent` in `hooks/useExternalIssues.ts` (5 s poll, invalidates the record when a key leaves; `useChat.ts` invalidates it with the task list on send, start and stop); `WorkspaceActivityPanel.tsx` renders the entries first with a `Current · Running/Waiting/Queued` badge and a left accent (`data-current`), outside the filters, their keys left out of the record. Tests: `api_workspace_timeline.rs::current_lists_ongoing_conversations_and_tasks_only`, `useWorkspaceActivityCurrent.test.tsx`, `WorkspaceActivityPanel.test.tsx` ("ongoing work"). Current rows render even while the record read is loading or failed. Limits: "waiting" is the task's own state (input or a manual retry), shown as one word; the set is bounded by execution slots, not paged; a conversation that starts and ends between two 5 s polls moves above a loaded page's cursor and reappears only on the record's next refetch (focus, send/stop, or staleness).

### g5.ui.activity.preview-row

- human-verified
- Source: the owner's 2026-10-29 request for better observation of preview runs in Activity and the decisions in the question set `preview-activity` (stage badge, ending verbs with a cause sentence, profile-only identity named at task creation, one-line failure detail, rows opening the preview dialog), with the chat decisions to drop the repository name, keep failure sentences unhighlighted and never open the task's empty Workspace changes ([chat intent](../../chat_intent/preview-status-in-activity/request.md), [design](../../pr_ontology_diff_design/preview-status-in-activity/ontology-plan.md), [mock](../../pr_ontology_diff_design/preview-status-in-activity/mocks/preview-activity-states.html)). The cause taxonomy, the lost-VM close semantics (`detail.close = lost`), the normalization of other writers' stop reasons and the log-availability rule are agent decisions recorded in the design's review rounds.
- Implemented: `services/workspace/preview_run.rs` (`preview_task_name`, `project_preview_run`, `preview_closing`, `preview_close`; owned by the Workspace context so `workspace_activity.rs` and the Agents' `task_lifecycle.rs` share one derivation, the latter using it at insert, in `apply_vm_loss` and in `workspace_preview_view`); `workspace_activity.rs` (`TASK_ROW_SELECT` reads spec, outcome and the started/ready event facts; `Task.preview`); `lib/previewRunWords.ts` (stage words, titles, ending sentences, `previewRunHasLog`, `previewEndingFromOutcome`), used by `WorkspaceActivityPanel.tsx` (`activity-open-preview-<task>` button, `currentBadgeWord`), `ActivityRailSection.tsx` and `previewRunLogModel.ts::previewFailure`; `PreviewControl.tsx` `openRequest` applied through `WorkspacePage.tsx::handleOpenPreview`; the preview view carries the same `ending`, so the dialog's failed-run notice uses the row's sentence and offers a log only when the run kept one. Tests: `task_lifecycle.rs::preview_run_projection_names_stages_and_endings`, `workspace_activity.rs::a_preview_row_carries_its_stage_while_live_and_its_ending_afterwards`, `api_workspace_timeline.rs::preview_rows_carry_their_stage_and_ending`, `previewRunWords.test.ts`, `WorkspaceActivityPanel.test.tsx` ("preview runs"), `ActivityRailSection.test.tsx` ("preview runs"), `PreviewControl.test.tsx` ("Activity row's request"), `PreviewRunLog.test.tsx` (existing notices kept; a timed-out notice added). A VM lost during a Close is recorded on the outcome as `detail.close = lost` by `apply_vm_loss`, so it is not mistaken for a clean close. Limits: the stage refreshes with the Current read's 5 s poll; the dialog shows the workspace's current preview, so a row for an older run opens the dialog's launcher or last-run notice rather than that run's log; an ended run whose VM is gone has no retained output behind its notice.

### g5.ui.activity-rail

- human-verified
- "make the activity button on the side an actual panel like the others, showing only the most important events (comments, approvals, pull requests and guided steps), with a button/link on top to the full activity page" (same [chat intent](../../chat_intent/activity-current-rail/request.md), 2026-10-20); Current in the rail, the eight-row cap and page 0 as the source are agent decisions in the [design](../../pr_ontology_diff_design/activity-current-rail/ontology-plan.md).
- "By workspace row pinned first, I meant add it as an icon before 'New doc' in the top bar, and remove it from the left bar" ([chat intent](../../chat_intent/davide-ui-assorted/request.md), 2026-10-29): `ActivityRailSection.tsx` and the sidebar's `activity` section are gone; `pages/WorkspacePage.tsx` renders `ribbon-overview` through `ActionRibbon`'s `leading` slot and `components/tree/TreeNode.tsx` offers `sidebar-collapse` at the top of the rail and the stacked panel. Earlier evidence, retained for the superseded panel: `components/external-issues/ActivityRailSection.tsx` (`activity-rail-open-full`, one `activity-rail-rows` list: `activity-rail-current-<key>` rows with a state word, a chat row a `?session=` link, then up to `RAIL_EVENT_ROWS` (8) `activity-rail-event-<key>` rows of the four kinds from page 0 of the shared record query, newest first; placeholders, `activity-rail-error` with Retry, `activity-rail-issue-error` when GitHub's discussion is missing from the page, `activity-rail-empty` only when a complete page holds nothing), rendered by `pages/WorkspacePage.tsx` as `LeftSidebar`'s `activitySection`; `components/tree/TreeNode.tsx` treats `activity` (titled Workspace, `LayoutDashboard`) as an ordinary section and `isWorkspaceDestination` as approvals only (collapsed by default, rail overlay, header keeps Collapse sidebar; the former header-only row and `onOpenActivity` short-circuits removed). Tests: `ActivityRailSection.test.tsx`, `TreeNode.sectionOrder.test.tsx`, `WorkspacePageAlignment.test.tsx` (section then link opens the page), `e2e/flows/alignment-shell.spec.ts` and `guided-journey.spec.ts` (updated, unexecuted here).

### g5.quality.platform-reports.trigger

- human-verified
- [Human choices and explicit agent interpretations](../../chat_intent/shell-corner-buttons/request.md); [upstream evidence](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/platform_reports.g5.md), PR #560. The modal choice is human; the no-chrome fallback and shortcut-only limit preserve the existing shell. Dialog-stack, report-button and App tests cover visibility and focus restoration; the browser assertion is gated by the advertised reporting feature.
- 2026-10-22 follow-up ([issue #701](https://github.com/G5Labs/G5-Lean/issues/701): the fallback flashed for about a second during load): the user chose to keep this rule and stop the flash rather than restore an always-on corner trigger. The sign-in and workspace loading screens hold the chrome trigger's place; a fixed delay (flash returns on slow loads) and a route-based chrome predicate (no fallback on a workspace load failure) were rejected. Report-button tests cover placeholder handoff to chrome and to a page without chrome; App tests observe no fallback during a workspace load and the fallback on a workspace load failure.

### g5.workspace.base-ui.chat

- human-verified
- Preserves the [Base presentation request](../../chat_intent/main/base-read-only-ui.md), with the later [disabled-button exception](../../chat_intent/shell-corner-buttons/request.md). This is an upstream amendment to presentation, not to Ben's Base mutation or VM authority.

### g5.workspace.ui.chat-corner-button

- human-verified
- [Human choices and agent interpretations](../../chat_intent/shell-corner-buttons/request.md); [upstream evidence](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workspaces.g5.md), PR #560. The human chose the existing floating panel and confirmed the disabled read-only button; composer focus and no shortcut/Escape were delegated choices. Workspace page tests cover layout, gates, session/focus and toolbar clearance. The floating placement is intentionally not restored on tab return; upstream recorded the browser flow as written but locally unexecuted.
- 2026-10-15 follow-up (edge resizing and remembered width): workspace page tests cover dragging the left and top edges with the right and bottom edges anchored, the width bounds, the remembered width on reopen and Pop out keeping its default; see the [shell corner buttons design](../../pr_ontology_diff_design/shell-corner-buttons/ontology-plan.md#resizing-follow-up-2026-10-15).
- 2026-10-20 follow-up (movable button, hidden while Chat holds the center, draft kept across Dock and Pop out): [human instructions](../../chat_intent/shell-corner-buttons/request.md#user-instructions-2026-10-20); `CornerButton` unit tests cover the drag threshold, the remembered and clamped spot, the swallowed post-drag click and moving while disabled; workspace page tests cover absence while Chat is the center surface under both layouts, the remembered spot and the draft surviving Pop out, Dock and the corner open; the alignment e2e flow drags the button before opening; see the [design follow-up](../../pr_ontology_diff_design/shell-corner-buttons/ontology-plan.md#movable-button-follow-up-2026-10-20).
- 2026-10-29: the button's portal became a column that carries the [bubble stack](#g5.workspace.ui.chat-bubble-stack) above the button at the same remembered offset (`CornerButton.test.tsx`, "carries a stack above the button that follows a drag").
- 2026-11-09: the per-workspace draft stays the owner of the editable text and the applied seed across Dock and Pop out, and the phone layout receives the same draft ([issue #1190 design](../../pr_ontology_diff_design/prefill-prompts-leak-into-another-workspace/ontology-plan.md)); `pages/WorkspacePageSeedOwnership.test.tsx` ("Pop out").
- 2026-11-03: the stack became a drag handle too, in both the button and the center case (`useCornerDrag` in `CornerButton.tsx`; `CornerButton.test.tsx`, "drags the button and stack together from a bubble in the stack" and "drags the lone stack from a bubble to a spot the button later takes"). Owner report: the center surface's stack could not be moved.

### g5.workspace.ui.chat-bubble-stack

- human-verified
- [Human choices and agent interpretations](../../chat_intent/chat-bubble-stack/request.md); [design](../../pr_ontology_diff_design/chat-bubble-stack/ontology-plan.md); [reviewed mock](../../../docs/mocks/chat-bubble-stack/README.md). The owner chose the names Off / Still / Animated / 3D, the cap of five, the layout setting with Column as default, approvals counting as waiting, `happy` as the 3D waiting face, the per-browser store, `3d` as the default style (2026-10-29, second round), the stack staying while Chat holds the center and the hover label instead of a tooltip (third round), on 2026-11-03 a bubble for every chat at any time, most recent first, dropping the Minimize control of the second round and the Close dismissal with it, and on 2026-11-09 the "Chat bubbles shown" count (default 5) in place of the column/collapsed layout setting, with the count unfolding every chat; the reduced-motion fallback, the stack taking the button's spot in the center case, the 1–50 range and the unfolded column folding back only from the same control (a remount starts folded) are agent choices.
- 2026-11-09: `UserSettings.chatBubbleLimit` (`lib/types.ts::isChatBubbleLimit`, `useUserSettings.test.ts`), the number input in `WorkspaceSettingsContent.tsx` (`WorkspaceSettingsContent.test.tsx`), and `ChatBubbleStack`'s `limit` prop with the unfold/fold control (`ChatBubbleStack.test.tsx`, "shows five by default, and the count unfolds every chat until folded back"). `ChatBubbleStack` the type and the collapsed layout are removed.
- 2026-11-03: `chatBubbleState` is never empty (`chatSessionActivity.test.ts`); `chatBubbles` keeps the list's order (`ChatBubbleStack.test.tsx`, "keeps the list's order and gives every chat a bubble"); the page suite covers the quiet chat's bubble and Close leaving the stack alone ("gives the quiet chat a bubble too, and Close only closes the panel"). `lib/chatBubbleMemory.ts` and `hooks/useChatBubbleMemory.ts` were removed with the control.
- `components/chat/ChatBubbleStack.tsx` (faces, order, count and unfolding, reduced motion) with `ChatBubbleStack.test.tsx`; `lib/chatSessionActivity.ts::chatBubbleState` with `chatSessionActivity.test.ts`; `UserSettings.chatBubbleStyle`/`chatBubbleLimit` (`lib/types.ts`, `lib/defaults.ts`, `hooks/useUserSettings.ts`, `useUserSettings.test.ts`) and the selects in `components/settings/WorkspaceSettingsContent.tsx` (`WorkspaceSettingsContent.test.tsx`); `WorkspacePage.tsx` feeds the stack into the corner button and opens the pressed chat at the right edge (`WorkspacePageV2.test.tsx`, corner Chat button suite). Faces ship as hashed assets under `src/assets/chat-bubbles/`; `robot-heads` is a lazy chunk. The drag from any bubble is `useCornerDrag` in `components/ui/CornerButton.tsx` (`CornerButton.test.tsx`).

### g5.workspace.ui.fork-landing

- human-verified
- [Owner instructions and agent interpretations](../../chat_intent/davide-landing-fork/request.md). The owner chose the existing center placement, a placeholder rather than typed text, the issue suggestion as the draft and the unchanged fork dialog; the landing's scope across fork entry points and the router-state hand-off are agent interpretations.
- `lib/workspaceLanding.ts` (`forkLandingQuery`, `forkLandingState`, `readForkLandingState`) with `workspaceLanding.test.ts`; `CloneDialog.tsx` reports the landing (`CloneDialog.test.tsx`: issue without Start work, started chat, failed chat start); `WorkspacePage.tsx` resolves `?view=chat`, seeds the first draft from the router state and holds the placeholder (`WorkspacePageV2.test.tsx`, "fork landing"); `ChatPanel.tsx` takes `composerPlaceholder` (`ChatPanel.test.tsx`).

### g5.ui.review-sidebar

- agent
- Source: [issue #574](https://github.com/G5Labs/G5-Lean/issues/574) and [implementation design](../../pr_ontology_diff_design/fix-suggestions-render/ontology-plan.md).

### g5.ui.authentication-continuity

- agent
- Recovery: [issue #698](https://github.com/G5Labs/G5-Lean/issues/698), [design](../../pr_ontology_diff_design/issue-698/ontology-plan.md). `useAuth.tsx` owns transition and retry state; auth regressions and two-tab Chromium checks exercise recovery, isolation and stale completions; [evidence and limits](../../pr_ontology_diff_design/issue-698/ontology-plan.md#implementation-evidence).
- [ClientAuth](../../uplift/ben-ontology/reviews/identity-full.md) checks that stale refresh/profile responses are ignored, temporary failure preserves a session, and retries keep the same user and organization. It does not show that every caller captures that identity before its first write.

### g5.ui.authentication-continuity.adoption

- agent
- Same-person continuity required by [issue #698](https://github.com/G5Labs/G5-Lean/issues/698); [design](../../pr_ontology_diff_design/issue-698/ontology-plan.md) retains the existing person/organization isolation boundaries. Shared token adoption updates context before asynchronous profile loading.

### g5.ui.authentication-continuity.draft-owner

- agent
- From [issue #698](https://github.com/G5Labs/G5-Lean/issues/698) and the [draft owners](documents/files.g5.md#g5.documents.files-view.drafts). `RequireAuth` consumes token identity while a profile is unresolved; [implementation evidence](../../pr_ontology_diff_design/issue-698/ontology-plan.md#implementation-evidence).

### g5.ui.publication-activity

- agent
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/workbench.g5.md).

### g5.preview.profile-label

- agent
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/preview_terminal.g5.md).

### g5.preview.trigger-state

- human-provided
- Source: owner request in chat, 2026-10-20 (“turn on the icon like the computer is on to indicate that there is a preview running”; the dot-free lit screen chosen from [the mock](../../../docs/mocks/preview-live-indicator/README.md)).
- Implementation: `frontend/src/components/preview/PreviewMonitorIcon.tsx`, `previewIndicator.ts` (`previewIndicatorState`), `PreviewControl.tsx` (a stopped VM forces the unlit state). Tests: `PreviewControl.test.tsx` trigger-state cases, `previewIndicator.test.ts`.
- Assumption: the red last-failed state persists until the next start; the owner has not said whether seeing the notice should clear it.
- Amendment (agent, 2026-10-29): the red state also covers a close cut short by a lost VM, with its own tooltip; an agent decision in the [preview-row design](../../pr_ontology_diff_design/preview-status-in-activity/ontology-plan.md) (review round 2), not an owner statement.

### g5.preview.trigger-state.accessible

- agent
- Source: [accessibility](#g5.ui.accessibility) applied to the trigger state; the name stays the label because the ribbon and its tests address the control by it.
- Implementation: `PreviewControl.tsx` trigger `title` and `aria-describedby`. Tests: `PreviewControl.test.tsx` trigger-state cases.

### g5.preview.trigger-state.freshness

- agent
- Source: design choice for the owner’s request; the alternative, carrying the preview state in the VM status response, adds no request but couples the two reads and was not needed.
- Implementation: `PreviewControl.tsx` preview query (`enabled`, `refetchInterval`); `frontend/src/test/msw/handlers.ts` default preview handler.

### g5.preview.run-log

- human-verified
- Source: owner request of 2026-10-20 (a run log in the dialog), first delivered in #655 as per-line task events and withdrawn the same day after the [finding](../../chat_intent/preview-status-observation/request.md) that they blocked the shared reconciler; redesigned at the owner’s direction on the terminal path ([design](../../pr_ontology_diff_design/preview-status-observation/ontology-plan.md)).
- 2026-10-29: the failure notice's wording moved to `lib/previewRunWords.ts`, shared with the Activity row (`g5.ui.activity.preview-row`); `previewFailure` now also notices timed-out and interrupted outcomes.
- Implementation: `frontend/src/components/preview/PreviewRunLog.tsx` and `previewRunLogModel.ts` (stages); `PreviewControl.tsx` (Log toggle, View log links, wider dialog while the log is open); `frontend/src/components/execution/WorkspaceTerminalPanel.tsx` and `useWorkspaceTerminalSocket.ts` (`terminalId`, `readOnly`, reset redraw with the “earlier output” line, `closed`/`session_ended`, follow-scroll, header with the session name, read-only badge, attachment state and “X of 4 MiB retained”; `terminalViewer.ts`). Tests: `PreviewRunLog.test.tsx`, `PreviewControl.test.tsx` run-log cases, `WorkspaceTerminalPanel.viewer.test.tsx`, `useWorkspaceTerminalSocket.test.tsx` hosted cases; Playwright `workspace-preview.spec.ts` run-log case.

### g5.preview.run-log.attach

- agent
- Source: the first CI run of the Playwright run-log case (2026-10-21) opened the log while the task was still queued and latched the guest’s refusal as final; the retry is bounded so an older dispatcher still degrades to the diagnostics view the owner asked for. The two notices were separated at CodeRabbit’s observation that a running VM contradicted “after the VM stops”.
- Amended 2026-10-27 (agent) after the owner’s QA finding ([request](../../chat_intent/preview-ux-post-improvements/request.md), [design](../../pr_ontology_diff_design/preview-ux-post-improvements/ontology-plan.md)): each retry remounted the terminal panel and the wait replaced it with a line of text, so the dialog flickered two or three times after Start preview, and the first two attaches were always refused because the task was not yet dispatched.
- Implementation: `PreviewRunLog.tsx` (`ATTACH_RETRY_DELAY_MS`, `ATTACH_RETRY_LIMIT`, the attach state and `onViewerState`; the panel’s `attach` and `pendingStateText` props), `previewRunLogModel.ts` (`previewRunStarted`), `WorkspaceTerminalPanel.tsx` (`attach` keeps the surface while the socket is closed). Tests: `PreviewRunLog.test.tsx` (held until started; retry while live keeps one panel instance; refused after the run ended; stopped VM), `PreviewControl.test.tsx` stopped-VM case, Playwright `workspace-preview.spec.ts` (one viewer element from Start to attached).

### g5.preview.run-log.ended

- human-verified
- Source: the owner’s 2026-10-20 design, including the decision not to link a cleanly closed run’s retained log ([design](../../pr_ontology_diff_design/preview-status-observation/ontology-plan.md)).
- Implementation: `previewRunLogModel.ts` (`previewFailure` from the outcome event), `PreviewRunLog.tsx` (failure notice and tail), `PreviewControl.tsx` (notices with View log; the launcher after a clean close). Tests: `PreviewRunLog.test.tsx` failed and cleanup-failed cases, `PreviewControl.test.tsx` “opens a failed run’s log” and “stays quiet about a clean close” cases.

### g5.preview.dialog-controls

- human-provided
- Amended 2026-11-05 (agent) for [Start workspace and preview](../software_architecture/workspace_vm.g5.md#g5.preview.start-and-preview): the launcher on a stopped VM and the pending-start status line.
- Source: [issue #786](https://github.com/G5Labs/G5-Lean/issues/786) (checked on QA 2026-10-24: a green but disabled Open, a quiet but active Close, a bordered card inside the dialog shell with a horizontal scrollbar); the owner chose the button-label cue over a header pill from [the mock](../../../docs/mocks/preview-dialog-and-tab/README.md) on 2026-10-26.
- Implementation: `frontend/src/components/preview/PreviewControl.tsx` renders into the shared `Modal` (size `sm`, or `xl` while `logShown`; `labelledBy`) with no inner card; `starting`/`openReady`/`stopping` drive the action row; `logOpen` resets when a run ends with no notice. Tests: `PreviewControl.test.tsx` (“Starting…” inert Open with Stop available; × dismisses without a cancel; “Stopping…” on Stop alone; wide while a log is rendered, narrow after a clean stop, wide for a failed run’s log), `workspace-preview.spec.ts` state text and width after Stop.
- Amended 2026-10-27 (agent) after the owner’s QA finding ([request](../../chat_intent/preview-ux-post-improvements/request.md)): the shell stayed wide after Stop preview because the width followed the Log toggle’s state, which lived as long as the dialog, not the rendered log.
- The issue’s second half, opening the preview in a top-level G5 app tab, was designed and implemented on 2026-10-26 ([design](../../pr_ontology_diff_design/preview-ux-improvement/ontology-plan.md), the owner’s answers in the [chat intent](../../chat_intent/preview-ux-improvement/request.md)) and reverted the same day at the owner’s request (“revert the tab change for now”); Open preview keeps the browser-tab path until it is picked up again. PR #693’s Log toggle sits in the same wrapping action row, integrated here on top of main.

### g5.workbench.gates

- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) adds the landing approval summary as a link to the same destination.
- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); the October15 correction supersedes per-phase approval rosters. Component, HTTP and localhost checks pass; exact-head Cloud qualification is recorded in [the design](../../../docs/review-flow.md).

### g5.workbench.journey.gates

- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md) supersedes the earlier phase-roster presentation.

### g5.workbench.review-prototype

- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) retired the landing's “PR submitted” vector popup. The owner's 2026-10-27 follow-up removes the implementation's Commit History shortcut; retained revision evidence and immutable history comparison links remain supported.
- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); the standalone entry point remains independently buildable. Runtime and stopped review flows, exact retry/privacy tests and remaining required Cloud evidence are linked from [the existing design](../../../docs/review-flow.md).

### g5.workbench.repository-browsing

- human-verified
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); implementation decisions and validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

### g5.workbench.change-selection

- human-verified
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); implementation decisions and validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).
- human-provided: [preview follow-up](../../chat_intent/issue-750/preview-selection.md), 2026-10-25, retains disabled Unpushed rows and contextual availability help, and uses All changes for repository and file-type choices in place of a separate Clear control.

### g5.workbench.change-selection.apply

- human-verified
- Expands: [g5.workbench.change-selection](#g5.workbench.change-selection).
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).
- The [scope amendment](https://github.com/G5Labs/G5-Lean/issues/750#issuecomment-6391210783) distinguishes empty repository/commit selection from empty file-type selection and retains applied summary/scroll while either group is pending.
- Agent clarification from the [preview follow-up](../../chat_intent/issue-750/preview-selection.md): an already captured empty comparison remains a valid target for type-only apply; clearing available commit choices still cannot request an empty comparison.

### g5.workbench.change-selection.recovery

- human-provided
- Expands: [g5.workbench.change-selection](#g5.workbench.change-selection).
- [Issue #931 additional scope](https://github.com/G5Labs/G5-Lean/issues/931#issuecomment-6483979793); the [existing design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#stale-checklist-recovery--931-additional-scope) records the mock proposal and verification scope.
- [Human chat refinement](../../chat_intent/issue-931/missing-commits.md), 2026-10-29: automatically unselect missing commits without replacements, rather than retaining checked unavailable rows at the bottom of the list.
- Human-approved performance amendment, 2026-10-30: [tip-based design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#tip-based-checklist-recovery-and-loading-performance--931-amendment) supersedes the manual stale-live-revision interaction. The earlier mock remains evidence for the superseded interaction.

### g5.workbench.change-selection.types

- human-verified
- Expands: [g5.workbench.change-selection](#g5.workbench.change-selection).
- [Issue #750 scope amendment](https://github.com/G5Labs/G5-Lean/issues/750#issuecomment-6391210783), addressing [issue #769](https://github.com/G5Labs/G5-Lean/issues/769); design and validation scope in [the existing plan](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md#file-type-emphasis-and-activity-amendment).
- The [matching-files follow-up](../../chat_intent/issue-750/matching-files.md), 2026-10-25, replaces emphasis with consistent filtering of filenames and detailed diffs, including growing selections.

### g5.workbench.change-selection.navigation

- human-verified
- Expands: [g5.workbench.change-selection](#g5.workbench.change-selection).
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).
- human-provided: remove the direction subtitle per the [preview follow-up](../../chat_intent/issue-750/preview-selection.md).

### g5.workbench.change-selection.rendering

- agent
- Expands: [g5.workbench.change-selection](#g5.workbench.change-selection).
- Addresses the owner's [rendering-efficiency follow-up](../../chat_intent/issue-750/matching-files.md); design and measured scope are in [the existing plan](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md#rendering-efficiency-validation--2026-10-25).

### g5.workbench.change-comparisons

- human-verified
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); implementation decisions and validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

### g5.workbench.change-comparisons.read-efficiency

- human-provided: owner authorizes the first two initial-load optimizations on 2026-10-27.
- agent: shared validated boundary and request-local reuse; scope, availability semantics and validation are in the [continuing design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#initial-comparison-loading).

### g5.workbench.change-comparisons.categories

- human-provided
- Expands: [g5.workbench.change-comparisons](#g5.workbench.change-comparisons).
- Classification, identity and efficiency requirements come from the [scope amendment](https://github.com/G5Labs/G5-Lean/issues/750#issuecomment-6391210783); [implementation design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md#file-type-emphasis-and-activity-amendment).

### g5.workbench.change-comparisons.activity

- human-provided
- Expands: [g5.workbench.change-comparisons](#g5.workbench.change-comparisons).
- [Issue #750 scope amendment](https://github.com/G5Labs/G5-Lean/issues/750#issuecomment-6391210783) requires complete pushed/base category counts independent of emphasis choices.

### g5.workbench.change-comparisons.personal

- agent
- Reconciles the personal workbench's retained Changes surface and absence of a fork with issue #750 during the current-main integration. This is an agent design specialization, not a new human decision.
- Design and validation scope: [existing #750 plan](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md#personal-workspace-integration).

### g5.workbench.change-checkpoint

- human-verified
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); implementation decisions and validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).
- human-provided (chat, 2026-10-23): remove Save changes from the menu and keep only Show Changes; this supersedes the issue's UI-button entry point.
- [Issue #931 additional scope](https://github.com/G5Labs/G5-Lean/issues/931#issuecomment-6483979793) adds inline read-only stale-list recovery, without a normal refresh control.

### g5.workbench.change-checkpoint.detection

- human-verified
- Expands: [g5.workbench.change-checkpoint](#g5.workbench.change-checkpoint).
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

### g5.workbench.change-checkpoint.selection

- human-verified
- Expands: [g5.workbench.change-checkpoint](#g5.workbench.change-checkpoint).
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

### g5.workbench.change-checkpoint.failure

- human-verified
- Expands: [g5.workbench.change-checkpoint](#g5.workbench.change-checkpoint).
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

### g5.workbench.change-access

- human-verified
- [Issue #750](https://github.com/G5Labs/G5-Lean/issues/750); implementation decisions and validation scope in [the design](../../pr_ontology_diff_design/issue-750-simplify-workspace-browsing-and-source-control/ontology-plan.md).

- Material implementation gap: dirty submodule directories currently refuse live reads; recursive submodule content comparisons are not implemented.

### g5.workbench.change-history

- human-provided
- [Issue #841](https://github.com/G5Labs/G5-Lean/issues/841).
- [Issue #931](https://github.com/G5Labs/G5-Lean/issues/931) supersedes only the non-selectable merge-base boundary: display the actual commit and allow explicit selection. The [existing design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#merge-base-commit-row--931) retains the comparison and history grouping.
- Owner follow-up requests growing Base-history efficiency and its risk assessment; index ownership, limits and validation are recorded in the [existing design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#growing-base-history). Bounded index retention and native offscreen containment are agent decisions.
- Human chat follow-up, 2026-10-27: place the existing action beside All changes and shorten its label to Show for a compact row.

### g5.workbench.approval-readers

- × and outside-click dismissal amendment: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974).
- human-provided
- [Issue #841](https://github.com/G5Labs/G5-Lean/issues/841).
