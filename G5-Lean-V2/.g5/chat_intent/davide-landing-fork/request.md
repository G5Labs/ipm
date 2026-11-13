# Fork landing on the center chat — request and interpretation

## Owner instructions (2026-11-03)

- "When you fork a workspace, land on a full screen chat. There can be 3 scenario:
  1. No issue attached: just open a chat with a pre-typed prompt 'What's your idea?'
  2. Issue attached: open a chat with a pre-typed prompt for the issue
  3. Issue attached, start work selected when forking: open the chat working"
- "by full screen I mean that are two chats modes: one is the floating dialog, the
  other is the 'full-screen' (or better say full page) mode"
- Answering the agent's questions:
  1. "'full page' is the existing center placement"
  2. "What's your idea?" is "a composer placeholder"
  3. On sending from a stopped VM: "someone else will change VMs to start
     automatically when you press a send button in the chat"
  4. Scenario 2's draft: "Reuse the existing issue suggestion"
  5. Fork dialog: "keep today's message box and launch controls"

## Agent interpretation

- The landing applies to every fork made in the UI: the fork dialog opened from a
  workspace, Home, Projects, the fork picker and an issue page, including Create
  workspace (a fork of Base) started from an issue, which used to land on the
  issue section.
- Scenario 3 is today's `?session=` landing, which already places Chat in the
  center; scenarios 1 and 2 land on `?view=chat` with the draft carried once in
  router state, so a reload of the landing entry seeds the same draft and any
  later navigation does not.
- The placeholder belongs to the fork landing, not to every new chat: the usual
  prompt stays elsewhere. A disabled composer still shows its reason first, so on
  a stopped VM the placeholder waits for the Send-starts-the-VM change.
- The ontology owner is [g5.workspace.ui.fork-landing](../../ontology/features/ui.g5.md#g5.workspace.ui.fork-landing).
