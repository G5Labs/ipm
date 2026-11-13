# Workspace button gates — what each control does when the VM is running, stopped, or in a review copy

## Human request

On 2026-10-06 the user asked for an audit before any change:

> check all the buttons in the current UI, nowadays some buttons are just go missing in when VM is gone versus VM is attached and also for the review workspace, some buttons are missing too. I want you to list all the buttons, what is current behavior in workspace with VM/without VM/review workspace, and what is your proposal. You need to understand what can and cannot do in workspace with/without VM and also review workspace to make sure you make right proposal..

The audit (published as a private page, "Workspace Button Audit") found that the page's single `workspaceReadOnly` flag folds Base, read-only policy, freezing, non-ownership and "VM not running" together, so a stopped VM hides durable actions, and that the server's review-copy refusals are enforced only in code. The user then asked for a decision sheet:

> i want to audit all the buttons behavior, can you ask generate a html that i can pick my expected behavior?

They filled the sheet ("Workspace Button Decisions", 52 button rows and 7 policies) and asked:

> I chose the html, can you take a look, pull the latest repo and implement it.

### Policies the user chose

| Policy | Choice |
|---|---|
| Temporarily unavailable actions (VM stopped) | Show disabled with the reason and a Start action |
| Intermediate VM states | Show each state with its own message and the single valid action |
| Review-copy exclusions | Hide all of them on a copy |
| Chat composer with the VM stopped | Disabled with a Start action; keep the draft |
| Publication controls for readers | Hide them for readers |
| Finish review | Implement as Stop plus an offered Remove |
| Review copy in the workspace switcher | List it as the current row under its source |

### Per-button picks that differ from the agent's proposal

- Explain in chat, "Review with my agent" / "Review Workspace" / "Discuss with agent", and the starter prompts: disabled with reason and Start while the VM is stopped (proposal had them enabled, seeding a disabled composer). "Chat about this" stays enabled.
- Workspace Details actions on a review copy (Make read-only / editable, Edit description, Link / Unlink issue): "Show, disabled with reason + Start action".
- Update from source and Discard on a stopped copy: note "Start your review workspace to update."
- Accept suggestions: note "Accept suggestions should both support for with VM and without VM if it is a pure suggestion."

Every other pick equals the proposal: durable actions enabled without a VM (Create PR, Rebase & Merge, Prepare to ship, session rename and new session, review-gap Answer/Fix/Implement, comments, reject/resolve/withdraw/reply, task and job controls, remote sync, PR list); VM-bound actions disabled with reason and Start (composer, agent answers, file and doc writes, editor Save, Generate, explainer, preview, terminal, Resolve with agent, Commit and Accept on the source); review-copy exclusions hidden (Fork current, Rebase & Merge, Create PR, Prepare to ship, Transfer, Publish branch, Make remote, Approvals tab).

## Agent interpretation

- **Three gates replace one flag.** Durable authority (owner, not Base, not read-only, not freezing) gates platform-record and Git actions. Live attachment (durable authority plus a running VM) gates only what the server's owner handle requires. The VM state itself drives messages and the one valid lifecycle button.
- **A Start action "in reach"** means a Start button inside the control's own empty state where it has one (composer, terminal, FILES Edit, Reviews panel, status band hints) and otherwise the workspace banner's Start directly above the content. The banner's Start and every inline Start are the header's Start (`POST /resume`), never an implicit start.
- **Details actions on a copy.** The pick names a Start action, but Make read-only, Edit description and issue links need no VM. The agent reads the pick as "disabled with a reason on a review copy": a copy follows its source, and a read-only copy would refuse Push review. No Start is offered there.
- **"Pure suggestion" accept.** Comment and artifact suggestions already resolve or keep without a VM; a text suggestion is applied into the working tree, which the server only does on a running VM. The pick keeps Accept disabled-with-Start, and nothing in the server changes.
- **Finish review** reuses the existing Stop and Remove operations on the copy: no new route or state.
- **Review identity on first paint** comes from the workspace list's `isReview` flag together with the review context query, so the hidden set does not flicker while the review query loads.

## Follow-ups after merge (2026-10-07)

> the notification message like Start the workspace to generate. Start is too many here, can you just make it when user trying to click a grey out button and show there instead of always show it blindly?

> and also fix the base workspace, the PLAN shows not implemented.

The user attached a screenshot of a stopped workspace whose status band showed "Start the workspace to upload docs. Start", "Start the workspace to generate. Start" and "Start the workspace to preview. Start" beside three disabled buttons at once. On Base, the Plan card read "<not implemented yet>".

### Agent interpretation

- A disabled control keeps its reason on hover (`title`); clicking it (or Enter/Space on it) reveals the reason and the Start action in a small popover anchored to that control. Nothing shows until the control is tried. The terminal panel, whose whole body is the empty state, keeps its inline reason and Start; the banner keeps its Start.
- The Plan card on Base read the placeholder because Generate is omitted there and Base has no chat for View changes; the card now states the gate reason ("Read-only workspace. Fork workspace to make edits.") instead.
