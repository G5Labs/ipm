# Composer seeds belong to the workspace visit that made them

[Issue #1190](https://github.com/G5Labs/G5-Lean/issues/1190): a Get Approvals prompt seeded and sent in one workspace appeared in another workspace's composer after a tab switch. The owner's comment on the issue widens it: every action that reaches the shared prefill handler (Find gaps, Create plan, Generate code, configured ribbon and Guided actions, Code review, Re-run Verification, Build mock, Review meeting, Explain in chat, Discuss) can leak the same way, and a leaked prompt carries another workspace's context and actions. The issue's root cause and fix plan are the starting point; this plan records their validation against the code at `770d803ed` and the adjustments the code required.

## Validation of the issue's analysis

Root cause, confirmed: `WorkspacePage` holds one `chatComposerSeed` for the mounted page and passes it to the desktop and phone `ChatPanel` unchanged; the panel is keyed by workspace id and receives that workspace's draft, so switching to a cached workspace mounts a panel whose draft has applied no token; the child's seed effect runs before the parent's workspace-change cleanup effect, copies the old prompt into the new draft and records the token; nulling the seed afterwards undoes nothing; and an accepted send clears the panel's input and pulse but leaves the page's seed payload in place. The applied-token guard only prevents a second application within one draft; it never establishes that a seed belongs to that draft. A focused integration test reproduced the leak before the fix.

Fix plan, confirmed with four adjustments:

1. Binding the seed to the existing `workspaceViewScope` identity is right: that object is replaced on every visit, so an A → B → A switch cannot revive an abandoned request, and it already scopes the page's retained callbacks. The seed's text, header, attached reference and seeded start become one request record where today three separately cleared owners (`chatComposerSeed`, `chatComposerSeedReferenceRef`, `chatComposerSeedStartRef`) are kept in step by hand.
2. Deriving the seed during render from a scope match is the decisive fix and needs no `ChatPanel` change; the panel's contract stays workspace-agnostic for personal chat and onboarding.
3. Conditional retirement needs two things the plan did not name. Only `ChatPanel` sees the draft being emptied, so the panel reports an emptied seeded draft with its applied token, and the page drops the matching request. The panel's restore of a failed send checks the send generation, which a seed into the open chat does not advance, so the restore also checks that no newer seed was applied meanwhile.
4. The phone layout mounted its `ChatPanel` without the per-workspace draft, so every remount there re-applied the current seed over the person's edits. It now receives the same draft as the desktop panel.

## Governing commitments

- [g5.workbench.chat-actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions), amended: a seed belongs to the workspace visit that made it and to the chat it went into; it never appears in another workspace's composer and a sent seed never returns; clearing the draft, choosing another chat or leaving the workspace drops it with its persona and attached reference. The statement's other commitments keep their wording and move from one long paragraph into bullets, as the writing rules ask of a changed statement; its 2026-10-29 reason already lives in its metadata.
- [g5.workbench.review-prompts](../../ontology/features/ui.g5.md#g5.workbench.review-prompts), unchanged: the header is draft state and lasts as long as the pulse.
- [g5.workspace.ui.chat-corner-button](../../ontology/features/ui.g5.md#g5.workspace.ui.chat-corner-button), unchanged: Dock and Pop out keep the unsent composer text; the per-workspace draft remains its owner.
- [g5.workbench.get-approvals](../../ontology/features/ui.g5.md#g5.workbench.get-approvals) and [g5.ui.workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config), unchanged: the person's Send is the request, under the persona resolved at send time.

## Design

**Owner.** `WorkspacePage` holds at most one *pending seed request*: the workspace visit that made it (`workspaceViewScope`), the one-shot seed (token, text, optional header), the reference that rides the seeded fresh chat's first send, and the chat action's seeded start with the chat it targets (none for a fresh chat). Every consumer of the shared seed path makes one; nothing else writes it.

**Shown.** The composer seed handed to either `ChatPanel` is derived in render: the request's seed when its visit is the current one, otherwise none. A panel mounted for another workspace therefore sees no seed in its very first render, whatever the effect order, and clearing afterwards is hygiene rather than the fix.

**Dropped.** The request is dropped, with its reference and start, when the visit ends (another workspace, or the page unmounts), when the active chat stops being the chat the seed went into (a fresh chat's seed survives only while no chat is selected; an open chat's seed only while that chat is), when the person empties the seeded draft (the panel reports the applied token and the page drops the request carrying it) and when a newer seed replaces it. Dropping passes no seed to the panel; the draft keeps the text already applied as the person's own, with its applied token, so nothing is re-applied and a later send from it is an ordinary send.

**Retired.** A send snapshots the pending request first. An accepted send retires that request only if it is still the pending one, so a late completion never clears a newer seed; the fresh-chat start and the open-chat Guided mark read the snapshot, never the live value. A refused or failed send leaves the request for the retry, and the panel restores the draft, pulse and header only while the same send generation and the same applied seed still hold.

**Panel.** `ChatPanel` keeps the applied token, text, pulse and header on the owner's draft, exactly as before; it gains the emptied-seed report and the newer-seed check in its failure restore. Both layouts hand it the per-workspace draft, so Dock, Pop out and a phone remount keep the person's edits and never replay the template.

### Choices considered

- Clear the page seed on a successful send only: misses every unsent seed, which is the switch-away case the owner reported first. Rejected.
- Run the parent's cleanup before the child's effect (layout effect, or clearing in the child's cleanup): leaves correctness coupled to mount order and still relies on an effect to undo a render. Rejected.
- Remount the whole page per workspace: discards the per-workspace draft map, so a tab return or Dock would lose drafts. Rejected.
- Key the request by `chatComposerScope`: that identity changes with the surface and the new-chat reset on purpose, so Dock or Pop out would orphan a live seed. Rejected.
- Carry the applied token in the send options instead of an emptied-draft report: binds the metadata to the sent draft, but a resend of a saved question answer also pulses and could carry a spent token, and the commitment is about clearing the draft. Rejected.

## Assumptions

- An unsent seed's text and pulse stay in the leaving workspace's own draft, since the draft is the person's; only the request (persona, phase, reference) is dropped, so a send from that draft after returning is an ordinary send. This was the existing behaviour and follows the pulse rule (sent or emptied); the ontology now states it under the "One owner" bullet, so it is a decided rule rather than an open question, and the owner can ask for the pulse to settle with the request if they prefer.
- No backend, prompt or database change.

## Validation

- `pages/WorkspacePageSeedOwnership.test.tsx`: the cached B → A → Get Approvals → Send → B sequence leaves B empty or on its exact prior draft, with no pulse, no header and no start request, and A → B → A revives nothing; switching away with an unsent seed; emptying the seeded draft then sending starts an ordinary chat; a delayed start that completes or fails after a switch touches neither workspace's draft; a failed send in place keeps the edited draft and retries under the persona; an edited seed survives Pop out.
- `components/chat/ChatPanel.test.tsx`: emptying a seeded draft reports its token once; a failed send does not restore over a seed applied meanwhile.
- Existing suites: `WorkspacePageAlignment` (chat actions, Get Approvals, review prompts, stale refusal), `WorkspacePageExplainerChat`, `WorkspacePageDocumentNavigation`, `WorkspacePageChatNavigation`, `WorkspacePageV2`.
- Not run here: the browser flow with two visited workspace tabs and the deployed QA journey; see the PR description.
- CI on the PR: the only failures were unrelated flakes, a `g5-scm` native timing test in the rust lane and the PDF viewer's match-highlighting test in the frontend step ([issue #1196](https://github.com/G5Labs/G5-Lean/issues/1196)); this change touches neither.
