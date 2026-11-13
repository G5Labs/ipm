# Chat actions seed the composer

Source: [chat intent](../../chat_intent/davide-ui-assorted/request.md) (2026-10-29): "Don't
submit chat prompts immediately when clicking the buttons like review meeting/build mock.
Make the send button pulse instead after the text box is pre-populated with the prompt".

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workbench.chat-actions` (new, `ui.g5.md`) | A ribbon or Guided chat action seeds the composer with its complete expanded prompt and pulses Send; the person's send starts the chat under the action's persona from the same configuration version, with the text as sent. A seeded composer pulses Send until sent or cleared, whichever seeded it. | The user's instruction; one rule for every seed keeps the affordance consistent. |
| [g5.ui.workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config) (amended) | The clicked version and phase action now govern the expanded prompt and the persona of the seeded start, not a server-side template start. | The start no longer names an action; the guarantee moves to the expansion. |
| [g5.workbench.meeting](../../ontology/features/documents.g5.md#g5.workbench.meeting) (amended) | The dialog's primary action seeds the composer with the extraction prompt over the chosen transcript; the person sends. | Same rule as the ribbon; the dialog stays the moment the transcript is chosen. |

## Governing ontology

- [g5.ui.workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config): actions
  and personas from one committed configuration version; the clicked phase action selects the
  template even when another phase shares its id.
- [g5.ui.guided-panel](../../ontology/features/ui.g5.md#g5.ui.guided-panel): a step reads
  Running while the page knows a session it started for the step's action; starting is never
  completion ([g5.ui.journey-evidence](../../ontology/features/ui.g5.md#g5.ui.journey-evidence)).
- [g5.workbench.meeting](../../ontology/features/documents.g5.md#g5.workbench.meeting): the
  extraction template is a hidden ribbon action the dialog starts by id.

## Design

**Owner.** `WorkspacePage` already owns the composer seed (a one-shot token with text, plus a
reference that rides the seeded send) and the chat send for the composer. It gains the
*seeded start*: the persona, ribbon phase and configuration revision the seeded send must
carry, and the action id whose Guided step becomes Running once the send is accepted. The
seeded start lives beside the seeded reference and shares its lifecycle: dropped on a
workspace change and whenever the seed can no longer be the next send. `ChatPanel` owns only
the pulse: it knows a seed was applied and whether the draft has been sent or cleared.

**Operation sketch.** A chat action (ribbon button, menu item, Guided step button, the
Review meeting dialog's extraction):

1. The page expands the prompt in the browser (`workbenchActionMessage`, the existing mirror
   of the server's expansion) from the clicked phase's action and its context (current
   document, phase), on the loaded configuration revision; an unavailable workbench
   configuration is the existing error toast.
2. If the open chat already carries the action's persona, the seed goes into that chat's
   composer; otherwise a fresh chat's composer, as other seeds do today. The seeded start
   records the persona, phase, revision and action id, and for the open chat its session id.
3. Send is the person's. A fresh seeded composer starts the session through the workbench
   start with persona, phase, revision, the text as sent (edits included), the composer's
   references and attachments, and the explicit launch confirmation ribbon starts use today;
   no action id or context travel, because the browser expanded them into the text. An open
   chat's send is the ordinary message. On acceptance the page remembers the action's session,
   so the Guided step reads Running exactly as it did for an immediate start.
4. Nothing is remembered or started when the person clears the draft, picks another chat or
   leaves the workspace; the seeded start goes with the seed. A later ordinary send in the
   fresh composer starts a plain chat.

**Pulse.** `ChatPanel` marks Send with a pulsing ring from the moment a seed is applied until
the draft is sent or emptied; edits keep it, since the ask is to send. Reduced motion keeps a
static ring. The ribbon's busy state for chat actions goes: seeding is immediate, and the
spinner only ever covered the server start.

**Outside the instruction.** The Issues panel's Implement start launches from an issue's
message under a persona, not from a prompt template; it keeps starting at once.

## Assumptions

- Edited seeded text is the complete request. The server no longer expands a template for
  these starts; a committed configuration's prompt still reaches the agent verbatim through
  the browser's expansion of that revision.
- Pulsing is one affordance for every seed (Explain in chat, Discuss, the review prompt, the
  D60 explainer prompts) rather than ribbon seeds alone.

## Tests

- `pages/WorkspacePageAlignment.test.tsx`: Build mock and the persona-overridden action seed
  a fresh composer (no start, Send pulsing), and the send starts under the action's persona
  with the expanded text and no action id; an open chat with the persona receives the seed in
  place; the Review meeting dialog seeds the extraction prompt with the transcript as context;
  Implement still starts at once.
- `components/chat/ChatPanel.test.tsx`: the pulse follows a seed and ends on send or clear.
