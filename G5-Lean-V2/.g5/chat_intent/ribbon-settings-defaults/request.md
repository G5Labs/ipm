# Ribbon ship actions, guided toggle and layout defaults — request and interpretation

## User instructions (2026-10-20)

- "Move create PR in Integration engineering, and add rebase in System engineering."
- "Add a setting to enable/disable the guided experience in the left menu (default enabled)."
- "Change the default mode of 'Left sidebar' to 'Expand on hover'."
- "Change the default mode for 'action bar' to 'Dynamic'."
- "remove also the create organization button in http://localhost:3000/g5labs/fork"
- "instead of the create organization button, let's have a create workspace button"

## Agent interpretation

- System Engineering's Land group keeps Approval and gains Rebase, a new `rebase` open
  target that reaches the existing Rebase Workspace dialog (the ReposBar's Rebase & Merge),
  a source-only rebase that never advances the target. Integration Engineering's Release
  group keeps Approval, Create PR (renamed from "Create PR and rebase"; it still opens the
  Prepare-to-Ship surface, whose rebase section stays) and Merge to parent.
- The guided journey's "Rebase on the parent" step moves from Integration to System
  Engineering (`se-rebase`, bound to Rebase); earlier `ie-rebase` marks are orphaned, which
  only affects a marks-only step.
- The guided experience is a per-person boolean beside the header styles, on by default,
  offered only where the Guided section can exist; it hides the section without touching
  the instance feature.
- The fork picker swaps its Create Organization button for New workspace (Home's Create
  workspace dialog, after the same organization switch a fork makes); organizations are still
  created from the Account flyout and `/create-org`.
- The sidebar and action-bar defaults change for people with no stored choice; stored
  choices are kept. The e2e flows pin the full sidebar in the shared auth storage state
  because they click the stacked panel.
