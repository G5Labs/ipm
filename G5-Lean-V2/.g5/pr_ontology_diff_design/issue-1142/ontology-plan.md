# Seeded chat actions send under the current workbench version

[Issue #1142](https://github.com/G5Labs/G5-Lean/issues/1142): a seeded chat action
(Find gaps, Create plan, Generate mock, Extract decisions, Import user stories, Get
Approvals) stores the workbench `configRevision` at click time and sends it on every
Send. The revision is the docs branch head plus the built-ins hash, so any docs commit
between click and Send (every question answer is one) makes the server refuse the start
with 409 `WORKBENCH_CONFIG_STALE`, and the seed keeps the stale pin until the draft is
cleared. The owner's issue text names the fix; this plan adopts it.

Scope decision (owner, 2026-11-06 chat): fix the bug now in its own PR; the persona
rework the 2026-10-30 standup aligned on (phase labels removed from chat, persona as
soft guidance, user-level steering templates) comes later. That rework decides what
becomes of the configuration version pin and the stale refusal, so the owner's optional
hardening (a revision derived from the workbench files' content rather than the docs
head) is deferred to it rather than built for a mechanism it may remove.

## Governing commitments

- [g5.ui.workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config):
  actions and personas load from one committed configuration version; a chat action's
  expanded prompt comes from the version and phase action the person clicked. Amended
  for what a seeded start's persona is resolved against.
- [g5.workbench.chat-actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions):
  the person's send is the request, edits included; from a fresh chat it starts the
  session under the action's persona. Amended: "from the same configuration version"
  becomes the version loaded when the person sends.
- [g5.chat.start-and-send](../../ontology/features/chat.g5.md#g5.chat.start-and-send):
  the seeded send may also carry Start consent; unchanged.
- [g5.conversation.inbox.reservation](../../ontology/features/chat.g5.md): the start's
  request key is claimed on an identity that includes the pinned revision, and the stale
  refusal rolls the claim back before any session exists (`api_workbench_chat.rs`,
  `a_stale_revision_is_refused_before_any_session_exists`). Unchanged and relied on: a
  refused seeded send leaves nothing behind, so sending again with another revision is a
  new, safe submission.

## What the version pin guarantees, and for whom

The server resolves two things from the pinned revision: the action's template by id
(immediate ribbon starts and Implement) and the persona's instructions, which it prepends
to the first input. A seeded send carries no action id: the browser expanded the template
at click time and the person edited the text, so the text is the request. The pin then
protects only the persona instructions, which the person never sees. A pin frozen at
click time therefore buys a seeded send nothing observable and costs the failure in the
issue. For immediate starts the pin stays meaningful: the template id must resolve against
the configuration the person saw.

## Design

**Seeded send pins the version loaded at send time** (owner: `WorkspacePage`, the seeded
start beside the composer seed). The seeded start keeps the action id, persona, ribbon
phase and target session; it drops `configRevision`. The seeded send reads the loaded
workbench configuration's revision when the person sends, exactly as the immediate ribbon
start and Follow view read it at click time. A send while the configuration is not loaded
fails with the existing "Workbench unavailable" reason and keeps the draft. On a stale
refusal the existing hook behaviour applies (the configuration cache is dropped and
refetched, the toast names the retry); the seed stays sendable and the next Send carries
the refreshed revision, so the person is never stuck. No automatic retry: the residual
stale window is one poll after a real workbench edit, a deploy or, until the deferred
hardening, an unrelated docs commit; the person's second Send is the retry.

**A refused seeded send keeps pulsing** (owner: `ChatPanel`, the pulse). The pulse lasts
until the draft is sent or emptied; a refused send is neither. `ChatPanel` restored the
pulse only for seeds with a header (the review prompts), so a ribbon seed lost its pulse
on the refusal. It now restores the pulse whenever the seed was pulsing, and the header
when there was one.

**The refusal is traceable.** `ensure_workbench_revision` logs the requested and current
revision at warn level when it refuses, since the issue could only be traced from request
timing.

### Choices considered

- Pin the click-time version and retry once after a refusal, or re-expand the template
  on refusal: a re-expansion cannot merge the person's edits, and a retry still sends the
  old text under the new persona, which is what the live pin does without a second
  mechanism. Rejected.
- Make `configRevision` optional for persona starts without an action id: changes the
  request contract and the replay identity for one caller when the live pin already
  satisfies it. Rejected.
- Derive the revision from the four workbench files' content so unrelated commits never
  invalidate a pin: correct and cheap, but it hardens a mechanism the persona rework may
  remove. Deferred to that rework, which must state what happens to the version pin.

## Assumptions

- The persona instructions a team commits are meant to apply to the next start, not to
  the start a person had seeded before the commit. The owner's issue text states the live
  revision is acceptable for seeded sends.
- Edited seeded text stays the complete request (carried over from the
  [seeded start design](../davide-ui-assorted/ontology-plan.md#design)).
- Fifteen-second configuration polling stays. Until the deferred hardening, an immediate
  ribbon start within one poll of an unrelated docs commit still gets the refusal once.

## Validation

- `api_workbench_chat.rs`: the stale refusal test and its claim rollback still pass.
- `WorkspacePageAlignment.test.tsx`: the seeded send carries the revision served at send
  time, including when the served revision changed between the click and the send; a
  stale 409 on a seeded send leaves the draft and pulse in place, and the next Send
  carries the refreshed revision and starts the session; immediate starts are unchanged.
- `useWorkbench.test.ts` unchanged.
