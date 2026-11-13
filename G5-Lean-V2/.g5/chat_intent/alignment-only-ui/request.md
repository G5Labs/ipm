# Remove the non-alignment UIs — request and interpretation

## User instructions (2026-10-23)

- "Check how complex it is to remove all UIs that are not alignment. If it's not too
  complex spawn an opus subagent to do it, otherwise do it in this session."

## Agent interpretation

- "UIs that are not alignment" are the classic workspace header layouts of the
  "Workspace panels" setting (`simple`, `advanced`, `both`) and everything only they
  render: the stage-flow status band, the ReposBar action strip with the VM status strip
  toolbar, the desktop workspace title bar, the classic organization Home (its top bar,
  Recent Workspaces and full issue search), the layout select itself, the status-band
  collapse preference and the instance gates that chose between them (`top_panel`,
  `alignment_ui`, `g5_ui_v3`, `G5_DEFAULT_WORKSPACE_LAYOUT`).
- The responsive phone layout is not a layout choice and stays, including the title bar
  Base keeps on a phone; the Guided section keeps its own kill-switch.
- The removal was judged too large for an unattended subagent (a 13k-line page, about
  15k lines of tests pinned to the classic layouts, every CI browser lane running under
  `both`, ontology lineage), so it is done in this session.
