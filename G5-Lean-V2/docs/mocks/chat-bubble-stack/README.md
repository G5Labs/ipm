# Chat bubble stack mock

Sample-data prototype (2026-10-29) for stacking every ongoing chat of a
workspace as its own round bubble above the bottom-right Chat corner button
(`frontend/src/components/ui/CornerButton.tsx`, ontology
`g5.workspace.ui.chat-corner-button`). It does not call G5.

- Each bubble's face shows the chat's state: **working** (gears), **waiting
  for your answer** (question mark; a question or a permission/approval
  prompt) or **idle** (sleeping; the turn finished and the chat waits for
  your next command). A ring colours working (blue) and waiting (amber,
  pulsing); a corner dot marks an unseen result. Hover shows the chat title
  and state; click opens that chat in the floating panel.
- The style is a user setting with four values: **Off** (today's button
  only), **Still**, **Animated** (the two gear, question and sleeping frames
  alternate) and **3D** ([robot-heads](https://robot-heads.fayaz.workers.dev/),
  MIT, React 18+, states working / happy / sleeping; loaded from esm.sh in
  the mock only). The stack layout (column by default, or collapsed to one
  bubble plus a count) is a second setting. Both live with the other
  appearance settings, per browser.

Open `index.html` in a browser. The toolbar switches the style, the number
of chats, the stack layout (column, or collapsed to one bubble plus a count
that unfolds on hover), the 3D head shape, the theme, reduced motion and
whether the hover labels are shown. Query parameters preselect the same (for
example `?style=animated&chats=5&theme=dark&tips=shown`). The notes under
the frames list the data sources and the questions to confirm before
implementation.

`img/` holds the seven supplied robot images scaled to 96 and 192 px
(1x/2x): `working-a/b`, `question-a/b`, `sleeping-a/b`. Still uses
`working-b`, `question-b` and `sleeping-a`; Animated alternates `a` and `b`.

Snapshots (rendered from `index.html`):

- `still-light.png`, `animated-dark.png` (5 chats), `3d-light.png`,
  `3d-round-dark.png`, `collapsed.png`, `off.png`
