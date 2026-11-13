# Ontology mode in the personal chat

## Human request (2026-10-28)

The owner asked to add an "Ontology mode" toggle to the personal chat. In this mode
the chat follows a specific workflow: what the person asks for is shown as proposed
ontology changes. The owner suggested reusing the generated `ontology-plan.md`.

After brainstorming, the owner stated the intended flow: "enable this mode in personal
chat and then after interactively finalizing the ontology changes, create an issue with
the ontology changes, which then the user can start a workspace to work on the issue."

Answers to the agent's questions, the same day:

1. Who may start the workspace from the issue: **anyone**.
2. Which blocks a proposal may carry: **both** intent (I) and decision (D) blocks.
3. If the issue is edited before a workspace starts, **the edited issue wins**.
4. Trackers: **both** GitHub and G5's native tracker.

The owner then asked for the design plan. After reading it, the owner decided:
"the agent should create the issue", replacing the plan's person-created issue
through the prefilled browser form.

Asked what the ontology says about confirming before the agent creates an issue, the
owner chose "option 2": the agent creates right away for trusted destinations and asks
for confirmation of the exact text only for untrusted ones. This keeps their 2026-10-13
`payload-confirmation` decision ([issue 478](../issue-478-create-issue/request.md#clarifications)).

## Follow-up after the mocks (2026-10-28)

Reviewing the clickable mocks, which showed a proposal bar and a separate Review view, the owner asked:
"I thought the ontology changes are presented in the chat?" Asked to choose, the owner decided:

1. Placement: **in chat only**. There is no separate Review view.
2. Each turn's card: **the turn's changes plus a total** for the whole proposal.
3. **A slim bar stays** above the composer: title, total counts, and the issue link with Start
   workspace once the issue exists.
4. The issue text is shown in the chat **only if the destination is untrusted**, where it must be
   confirmed. For trusted destinations, the person reads the issue after creation.

## Answers on the open questions (2026-10-28)

1. Cards: the newest is expanded and earlier ones are collapsed ("Sounds good").
2. The Ontology-mode hint line under the composer stays ("Hint is fine").
3. A changed proposal after the issue exists: "Always update the issue". Asked how, the owner chose
   **add a comment** (not replace the description). For an untrusted destination, the new text is
   confirmed in the chat as on creation.
4. Conflicts with Base: "the conflict should have been shown and resolved in the personal chat",
   checked on **every reply**.
5. The owner asked what the three options are for producing a card's turn changes. They were
   explained, and the choice stays with System Engineering.

### Scope

Asked what happens when a started workspace finds a conflict, the owner said: "The focus of this
workspace is the personal chat and up to creating the issue. Everything after that is
out-of-scope." Application by a started workspace is therefore not designed or built here.

## Extension to workspace chats (2026-10-28)

The owner then asked: "What if we apply the card to workspace chat as well? For every ontology
changes, each turn makes, we show the card. If the turn doesn't make any ontology change, then no
card is shown." Asked to decide, the owner chose:

1. **Extend this workspace's scope** to cover changes cards in every workspace chat, rather than
   making it a separate follow-up.
2. A workspace card shows **only this chat's agent's changes**, not edits made at the same time by the
   person or other chats.
3. A workspace card shows **no total**.

Answers to the remaining questions, the same day:

- The agent posts the update comment **only when asked**, not after every change.
- Asked about a public repository refusing the update comment, the owner asked whether `g5d` could
  update the issue instead. Findings: `g5d` can create, comment (`-F` file, idempotency key), close
  and reopen, but not edit. The ontology gives agents no issue edit ([native writes](../../ontology/features/issues.g5.md#g5.issue.native.writes)).
  G5 never creates an issue in a public repository ([creation admission](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission)),
  so a refused comment happens only if the repository becomes public later. Then the agent reports it.
- The owner asked for options and a recommendation on the System Engineering questions. They are
  in the [design plan](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#system-engineering-questions)
  as business recommendations, for System Engineering to decide.

## Inline editing on the card (2026-10-28)

The owner asked: "Can we allow inline ontology statement editing on the card? Maybe part of flow 2?"
The agent noted that this conflicts with "the person changes the proposal by talking to the agent", with
cards being a fixed record of their turn, and with workspace cards showing only the agent's changes.
It offered opening the editor instead. The owner chose **inline editing on the newest card only**, in
**both chats**. Asked whether the newest card stays editable after later turns that produce no card
(such as a code-only turn), the owner answered **yes**.

## Governing statements on the card (2026-10-28)

The owner asked: "In the card, should we also add unchanged but relevant / governing ontology statements?
How do you propose to do that?" Given the options (links only, agent-selected, agent-selected and checked),
the owner chose:

1. **The agent picks them, and the platform checks them.** The agent names the governing statements with
   a one-line reason each, and the platform confirms they exist and shows their text.
2. **The issue lists them by ID and reason**, without their text.

## Scope narrowed to workspace chats (2026-10-29)

The owner asked: "Can we limit our changes to regular workspaces and not personal chat/workspace?" Asked
to decide, the owner chose:

1. **Remove the personal-chat intent** from this branch's ontology and design: Ontology mode, proposals,
   the Base check, and creating and updating an ontology proposal issue. It stays recoverable in Git and
   in the sections above.
2. **The personal chat shows no changes cards.**
3. **Only the workspace owner may edit a statement on a card.** Others reading the chat see the card
   read-only.
4. **Keep the mock's personal-chat flow, marked out of scope.**

Asked whether editing is allowed while the agent waits for an answer to its question card, the owner asked
why it mattered. The existing rule already covers it (a waiting turn is still running, so the card is
read-only), and it was left at that.

## Changes between the turn's start and end (2026-10-29)

The owner decided: "Let's make the card contain changes from start and end of the turn. I think it would be
difficult to just get this chat's agent's changes." This replaces the 2026-10-28 choice of this chat's agent's
changes only. The card now shows every ontology change between the turn's start and its end, whoever made it.
The owner also asked how Claude Code and Codex compute a turn's file diffs. The answer is in the
[design plan](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#system-engineering-questions).
Asked whether a card should say when someone else may also have written during the turn, the owner decided
**no note**.

## Remaining questions (2026-10-29)

1. A turn the person stopped, or one that failed, gets **no card**, even if it changed the ontology. The
   changes it left behind show in Compare.
2. Large cards are **capped with "Show more"**, first at 20 statements, then lowered by the owner to **10**. The owner
   chose this over the offered cap of 50 with a link to Compare. That link would have opened All changes, the whole
   workspace's change against its parent, not the turn's.
4. The card's link to All changes is labelled **"View all changes"**, the existing page's own label, rather than
   "Open Compare".
3. Next step: **wait for the owner's instruction** before handing off or writing a requirement document.

Applying a proposal in a started workspace stays out of scope. Once built, its reply would get a card
like any other turn.

## The turn's start snapshot (2026-10-29)

On question A, the owner found a full copy of the ontology per turn too large (about 860 KiB here) and asked:
"Can we store the current git commit at the start of the turn? It seems cheaper. If there are dirty files in
.g5/ontology/**/*.g5.md then maybe store those in addition to the git commit sha?" The agent added two
refinements: the base is a commit the server already has, so a continuation after VM loss can still read it,
and every kind of difference counts (staged, unstaged, untracked, deleted, renamed). Asked why file text is
needed at all, the owner accepted a patch against the base for tracked files and full text only for new
untracked files. The owner: "Sounds good. Record."

## Telling the next agent of card edits (2026-10-29)

On question D, the owner chose option 1, "For D, 1.": the platform adds a short note to the next turn's
input naming the statements the person edited on a card and their files, so the next reply does not undo them.

## Handing over the governing list (2026-10-29)

On question E, the owner asked what the `AskUserQuestion` card uses. It uses Claude Code's built-in tool, which
the runner pauses and records as a turn event; Codex has no equivalent, and G5 gives agents no MCP tools. The
owner chose option 2, "do g5d": the agent records the list on its turn with a `g5d` command.

## System Engineering decisions (2026-10-29)

The owner, who is also System Engineering, approved the choices on A, D and E: "I'm SE. I approve." Asked which
finished turns get a card, the owner confirmed that only a turn ending successfully does: a turn that timed out
or exceeded a limit counts as failed, and a cancelled one as stopped. The owner also asked for the mocks to be
updated with Show more after 10 statements and the "View all changes" link.

## Recording card edits (2026-10-29)

The owner asked how a card edit is saved and why a record is needed besides the file. A card edit is written
into the ontology file; a record with the card keeps the "Edited by you" mark and the agent's text after a
reload or with the VM stopped, and tells the next agent of the edit. Offered a design without a record, where
the card and the next agent's note would compare the card's text with the file instead, the owner decided:
"Let's do with record." The edit is recorded only after its file write is confirmed, and a retry completes a
failed record without writing again.

## Quieter edit mark (2026-10-30)

Seeing the "Edited by you" badge and the expandable "Agent's text" on an edited statement, the owner asked
what they were and how necessary, considering removing them or making them less apparent but clearer. Offered
removing both, a quiet mark that opens a diff, keeping only the mark, or an inline diff, the owner chose the
quiet mark "but I think no need to support clicking it": a muted "Edited" with an explanatory tooltip, and no
view of the agent's original text on the card.

## Agent interpretation

- The issue must be self-contained: anyone may start the workspace, and only the
  owner's own workspaces can read the personal repository.
- `ontology-plan.md` describes owners and interactions in code the personal chat cannot
  change. The proposal carries intent and decisions; the started workspace writes its
  plan with `ontology-planning`, using the issue as its intent source. The plan's
  "Ontology changes" table format is reused as the proposal's summary.
- The started workspace's agent applies the proposal (rather than the platform at fork
  time), because issue text is context rather than instructions and Base may have moved.
