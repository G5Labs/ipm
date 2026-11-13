# Assorted UI fixes — request and interpretation

## User instructions (2026-10-29)

- "when the left bar is set to 'expand on hover', clicking on files doesn't work"
- "when a workspace is forked/started for the first time, no need to have the guided journey
  expanded (this was a previous decision which I'm reverting now)"
- "move the workspace icon [the rail's `sidebar-rail-activity` button, labelled Workspace] in
  the icon bar as the first button. Give me few options for the name based on the content and
  I'll pick one"
- "the personal workspace should have a icon in the top bar. Ask me if anything is ambiguous
  or unclear"
- "Don't submit chat prompts immediately when clicking the buttons like review meeting/build
  mock. Make the send button pulse instead after the text box is pre-populated with the prompt"
- "remove the analyze button and actions from the workbench"
- "during the tutorial some buttons are supposed to be pulsing but they are not. Verify that
  visually"
- "let's call the workspace row Overview"
- "enable internal issue tracking by default if no external issue tracking is enabled" (the
  Issues switch on the Admin page, `<org>/Base?view=admin`)
- "By workspace row pinned first, I meant add it as an icon before 'New doc' in the top bar,
  and remove it from the left bar. Also gray out the review meeting button if the workspace is
  not started"
- "This button to open/close the sidebar [the chevrons-left Collapse sidebar control] should be
  there only in full sidebar mode"

## Agent interpretation

- Files in the hover rail: a press on a non-focusable row blurred the focused rail control
  and the overlay closed before the click landed; the blur now defers to the press rule.
- Guided collapsed: every section starts collapsed on open and fork, in the stacked panel
  and both rail modes; the rail intro and the remembered collapse go with the reverted
  decision ([g5.ui.guided-collapsed](../../ontology/features/ui.g5.md#g5.ui.guided-collapsed)).
- Overview in the top bar: the ribbon gets a fixed Overview control (dashboard glyph) before
  the phase's first action, opening the workspace page; the sidebar's Activity section, its
  panel and the earlier pin-first logic are removed, and the sidebar's collapse control moves
  to the top of the stacked panel, in full sidebar mode only. The center tab keeps its name.
- Review meeting greyed while the workspace VM is not running (and in a read-only workspace),
  with the reason as its tooltip.
- Analyze removed from the built-in catalog only; a committed `.g5/workbench/actions.yaml`
  may still define its own actions.
- Tour pulse: the ring animated but its easing hid it for most of each cycle; the easing
  and keyframes changed, measured in the mock app.
- Chat actions seed the composer: see the
  [design](../../pr_ontology_diff_design/davide-ui-assorted/ontology-plan.md). The
  Issues panel's Implement start, which launches from an issue rather than a prompt
  template, is outside the instruction and keeps starting at once.
- Personal workspace icon in the top bar: pending the owner's answers (glyph, scope).
- Issues by default: "internal issue tracking" is G5 issues, so the switch reads on unless
  set off and every tenant organization has the native domain (keyed from its slug, trusted)
  from creation, with a migration for existing organizations without any domain. An
  organization with an external tracker already configured is untouched.
