# Sidebar collapse-to-rail mode and Guided default — request and interpretation

## User instructions (2026-11-09)

- "Today we have these options for the side bar: [Full sidebar, Expand on hover, Icon rail only].
  Let's add one more that is like full sidebar, but when collapsed it shows the icons like in
  Icon rail only + [the Expand sidebar button]. When collapsed and a button is clicked, it
  expands like full sidebar."
- "In the same PR also turn off Guided experience by default."
- Follow-up: "make collapse-to-rail the default option. If the guided entry is off remove the last
  step from the Onboarding tour, don't say that the checklist is turned on in Settings."

## Interpretation

- The fourth Left sidebar choice, "Full sidebar, icon rail when collapsed" (`collapse-to-rail`),
  behaves exactly like Full sidebar while open: the stacked panel in flow, its collapse control at
  the top, the resizable width. Collapsed, the sidebar is the icon rail of Icon rail only with the
  Expand sidebar button above the icons; it replaces the bare collapsed edge the full sidebar
  shows. Pressing Expand or any section icon expands the full sidebar; a section icon also opens
  that section and brings it into view. The collapsed rail never opens the single-section overlay.
- The new mode is the default for people who never chose a mode; stored choices are kept.
- Guided experience is off by default for people who never set it; a stored choice is kept.
  Because the onboarding tour's seventh step ends on the sidebar's Guided entry, the tour has six
  steps while the setting is off: the counter says "of 6" and the tour finishes when the
  workspace route loads after Start workspace. A tour resumed at the seventh step while the
  setting is off finishes at once. The earlier interim card that pointed at Settings is gone.
