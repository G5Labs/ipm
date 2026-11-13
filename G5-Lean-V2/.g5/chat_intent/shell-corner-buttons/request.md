# Shell corner buttons — request and interpretation

## User instructions (2026-10-20)

- "make the bottom-right Chat button movable, so it can be moved if it hides other UI
  elements."
- "don't display the bottom right chat button if you are already in the big chat page."
- "audit if we can click from small floating chat (I open a small chat, I want to make it
  big after/while chatting) to have it in the big chat window and viceversa."

Agent interpretation: drag the button itself (no handle), remember the spot per browser as
offsets from the home corner, keep a short press as a click; "big chat page" is the center
Chat surface (`centerChatActive`); the audit found Dock and Pop out already switch both
ways and fixed the unsent draft being lost by the remount.

## User instructions (2026-10-15)

Report-a-bug trigger:

- Asked whether the top-right and bottom-left bug-report icons differ (they do not: one
  controller, two placements) and for alternatives to the duplicate. Chose "the first
  option": show the floating button only while a modal is open.

Chat corner button, in the same PR:

- "I want to add a chat button in the bottom left, specular to the current report bug, and
  when clicked it opens the chat as a right panel." Corrected: "I meant bottom-right."
- "chat button hides during modals."
- "the button hides when the panel it opens is open."
- "Workspace routes only."
- "Base and read-only workspaces show a greyed-out button with a read only tooltip for the
  reason."
- "third entry point, for a little different experience because this opens the right panel
  instead of the center one."
- Alignment and central-chat layouts: "open the right panel."
- "mobile: hide."
- "existing right panel" (not a new overlay drawer).
- Focus target, keyboard shortcut and Escape: "do what's best."
- "Session choice on open: last active session."
- "Visual weight: secondary like the bug button."
- "yes go to planning."
- 2026-10-15, after reading the plan: "keep a greyed-out corner button" in read-only workspaces;
  asked for a mock of the two right-panel options.
- 2026-10-15: "let's go with a. Implement this + the bug report changes."
- 2026-10-15: stack the work on top of PR #559 (tab return to the last page).

## Agent interpretation

- The bug-report fallback also has to stay on pages that mount no chrome trigger (settings,
  account connection, CLI authorization): there the floating button is the only trigger, so
  "only while a modal is open" is read as "only while the chrome trigger is absent or
  covered".
- "Modal" is read as any overlay registered at the modal tier or above in the dialog stack,
  other than the report drawer itself, because that stack is what decides which overlay owns
  keyboard focus; the settings flyout sits below the modal tier and does not count.
- No docked right chat panel exists any more: since chat became the central surface
  (commit `07a865f24`, 2026-09-22) Chat renders in the center or, through Pop out, as a
  draggable floating panel. "The existing right panel" was settled by the user as that floating
  panel anchored to the right edge, after a mock of both options.
- Read-only and reader workspaces keep Chat open for reading through the other entry
  points, so a disabled corner button there hides a surface the title bar still offers. The
  user confirmed the greyed-out button after reading that note, so it is a decision.
- "Do what's best": focus lands in the composer when it is enabled; no keyboard shortcut,
  because the button is always on screen whenever it could open anything; no Escape, because
  the floating chat panel is not a dialog and does not take one today.
