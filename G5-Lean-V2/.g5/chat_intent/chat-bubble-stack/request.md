# Chat bubble stack — request and interpretation

## User instructions (2026-10-29)

- "Each workspace has a floating chat button on the bottom right. I want to stack all
  the ongoing chats, so each one of them gets its own bubble on top of the existing
  floating one and I can easily open which chat is going on."
- "I want the icon of the ongoing chat to be one of the attached images depending on
  whether the chat is working, idle or waiting for a question." Five robot images were
  attached (two with gears, two with a question mark, one sleeping), later two sleeping
  frames.
- "These stacked bubbles should be configurable in the settings with three values: None
  (same as current behavior), 'static', 'animated' and '3d' (with 3d images taken from
  https://robot-heads.fayaz.workers.dev/). You can search the web for better names for
  the options."
- "In 'static' mode use only the ChatGPT Image Oct 29, 2026, 12_17_27 PM-*.png images.
  In 'animated' mode, when working, there are two images with gears, alternate between
  them so it seems like it's animated. Same for the question and sleeping."
- "Start with a mock so I can inspect it visually."
- After the mock (`docs/mocks/chat-bubble-stack/`), answering the agent's questions:
  "waiting for a question" includes permission or approval prompts; use the newly
  attached sleeping frames; use `happy` as the 3D question face; make the stack layout a
  setting, default plain column; store it like other appearance settings; option names
  Off / Still / Animated / 3D; a cap of 5 bubbles; "Go ahead with the implementation".
- Asked, about the agent's "waiting" wording: "you mean waiting on a command right?"
  (not answered before authorization; see interpretation).
- After the PR (#953) was published: "when I open a chat in the floating panel, add a
  minus button, next to the x (close) button. So minus still shows it in the stack, and x
  removes it. Also make 3d the default selected option."
- Third round: "show the chat stack even when you are in the big chat page. Clicking on
  one of the stacks in that case opens the corresponding chat into the big page." and
  "when you hover a chat, show the name immediately, like in the mock, instead of a
  tooltip".
- 2026-11-03 (PR with the drag fix): "when in full-page chat mode (the docked/center
  chat), there is no way to move the floating chat icons with the little robots. Make
  sure the floating icons can be moved not just from the bottom bubble, but from any
  robot avatar bubble"; then "in the same PR let's change the avatar bubbles to always
  have one bubble for each chat (sorted by most recent), even when idle/closed. Remove
  the minus button that was introduced in PR #953 in the floating chat, since there
  will always be avatars for every single chat at any time".
- 2026-11-09, after the bubbles stopped opening chats outside the center ("Did something
  break there?", fixed in the same PR): "remove this setting 'Chat bubble stack'. Instead
  add a setting for how many avatars should be displayed (default 5). And also make the +
  symbol when more than 5 chats clickable to it gets expanded to show all of them".

## Agent interpretation

- "Ongoing" was first read from the existing chat activity: a chat whose turn is active,
  whose running turn waits for the person, or whose latest result this person has not
  yet inspected, with an inspected, idle chat getting no bubble. Superseded on
  2026-11-03: every listed chat has a bubble at any time, in the list's most-recent
  order; the sleeping face is any idle chat, with a dot while its result is uninspected,
  and the cap of five with the count still bounds the column (agent choice). On 2026-11-09
  the cap became the "Chat bubbles shown" setting (1–50, default 5) and the count unfolds
  every chat until folded back; the column/collapsed layout setting was removed.
- "Waiting for a question" is a held turn: a pending provider approval request or an
  `AskUserQuestion` whose answer the person has not given, as the open chat already
  detects for its composer. The owner confirmed approvals count.
- The 12:17:27 PM batch is the second upload (sleeping, gears B, question B); Still uses
  those plus the owner's later sleeping frame 1; Animated alternates each pair.
- "3D" uses the `robot-heads` npm package behind the site the owner linked (MIT, React
  18+, 2D canvas), states `working`, `happy` and `sleeping`.
- Both settings are per browser like the other appearance settings; Animated and 3D
  fall back to Still under the browser's reduced-motion preference (agent choice).
- Minimize pinned the chat and Close dismissed it until its activity changed, per browser
  and per workspace. Removed on 2026-11-03 at the owner's request: with a bubble for every
  chat, Close only closes the panel and nothing pins or dismisses a bubble.
- Dragging (2026-11-03): any bubble, above the button or in the center surface's lone
  stack, is a handle for the whole group; the spot is the one the button remembers.
