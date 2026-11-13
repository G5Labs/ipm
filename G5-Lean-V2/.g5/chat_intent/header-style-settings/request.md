# Workbench header styles — request and interpretation

## User instructions (2026-10-09)

- Personalize the phase chevrons on top of a workspace and the action buttons under them
  with settings "similar to Left sidebar and Workspace panels" on the settings page.
- Mocks were reviewed first (artifact "Workbench Header Options"); the vertical phase bar
  was corrected twice: "I actually meant vertical, still like a chevron, but all 3 phases
  should be the same height of a big icon", then "don't abbreviate to BE/SE/IE, have the
  full names with the icons too on the right. We can remove the branch name on the right if
  there is no space", then "but now it's not a chevron anymore?" (the rows must keep the
  right-pointing chevron silhouette).
- Implement, for the phase bar: Extended, Minimal, Vertical. For the action bar: Extended
  icons "but without author/analysis/mock" (no group captions), Minimal icons, Extended text
  "but add the icons here too like in simplified", Simplified.
- "Remove the branch chip unconditionally."
- "Set the default to phase bar=minimal and action bar = extended icons. Set also the default
  for Workspace panels=alignment."

## Agent interpretation

- Two new per-person, per-browser settings beside Workspace panels, offered only while the
  alignment layout is offered, because they only change the alignment header.
- Extended keeps today's chevrons; Minimal drops the deliverable subline; Vertical stacks the
  three right-pointing chevrons, each with the phase icon and full name, on the left of the
  action-bar row so no top row remains. Approval meters stay beside the name in every style.
- Action icons are client presentation keyed by the built-in action id with a fallback by
  target and kind, so a committed `actions.yaml` needs no icon field and unknown actions
  still get a glyph. Minimal icons carry the label as the tooltip and accessible name.
- Simplified is one wrapping row of icon-plus-label buttons with group dividers; the
  Office-style overflow menu is not implemented.
- The branch chip and the page's member-branch lookup for it are removed.
- "Default for Workspace panels" is the instance default: the server's
  `G5_DEFAULT_WORKSPACE_LAYOUT` fallback and the hosted QA, dev and test values become
  `alignment`; the client's fallback for an absent value matches. CI keeps `both` pinned so
  the classic-layout e2e flows stay valid; a stored or default `alignment` without the
  `alignment_ui` feature still resolves to `advanced`.
