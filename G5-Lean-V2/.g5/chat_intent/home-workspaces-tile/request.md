# Home workspaces tile — request and interpretation

## User instructions (2026-10-15)

- "Let's create a PR to remove the workspaces directory from the homepage
  (`<section data-testid="home-workspace-directory" …>`). We should have only the Recent
  Workspaces one. Merge workspaces and recent workspaces into one. There should be only 4 areas.
  By default should filter the workspaces I created. The default ordering in workspaces should
  be most recently opened by me with option to select. Remove the buttons open in BE SE, IE,
  etc. Instead show what phases are still missing approvals (if there are none missing — we
  don't have to show any status). In addition to the fork button, have a delete button on the
  left of it (missing if it's not your workspace). The owner should be shown left to the action
  button like in running compute."
- Answers to the design questions: owner (not creator) with a Mine / All toggle, All hiding other
  people's review copies; sort options Recently opened (default), Recently updated, Created, Name,
  as a flat list without the lineage tree; keep the search box; "name is the Open link. the
  disclosure shows only if there are running jobs"; delete owner-only; owner name joined on the
  client from the organization members; "Ignore classic layout, it's fine to break"; "keep all.
  Add pagination to all the four cards and limit them to 5. Add a setting in the settings page to
  set the pagination to a higher number".
- "Also remove the search github issue tile at the bottom of the home screen."
- On the approvals mocks: "add one more mock where this is more like a table so the various things
  are aligned"; "The disclosure for both jobs and children together is confusing"; "can we simplify
  this by showing less info? … drop the activity column … remove the issues from next to the
  name"; "also replace 'Clear' in the approvals with all green phases, so everything keeps being
  aligned"; "go ahead" on variant M of the mock page (chevron with a presence dot, animated once,
  on the flat table layout).

## Interpretation

- One Workspaces tile replaces both the directory and Recent Workspaces under the alignment layout:
  a table with a header row (Workspace, Approvals, Opened, Owner, actions), the search field, a
  Mine / All filter and a sort select, five rows per page with "Show N more".
- The Approvals cell is three phase gates (BE · SE · IE) coloured approved / waiting / rejected,
  faded for a phase without a requirement, empty for a workspace with no requirement at all.
- The leading control is a chevron with a presence dot, rendered only while the workspace has
  running tasks; it discloses the workspace's recent jobs. Nothing loops: the dot pulses once
  when the state changes and a polite live region names the change.
- Filter and sort live in the URL (`?workspaces=all`, `?sort=updated|created|name`), per the Web
  Interface Guidelines read during the design; the search text stays local.
- The rows-per-tile number is a user setting (default 5) applied to all four tiles.
- The full issue search below the grid is gone under alignment; the Issues tile loses "View all".
  Classic Home is untouched.
