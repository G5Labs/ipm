# Home workspaces tile — design

Request: [chat intent](../../chat_intent/home-workspaces-tile/request.md). Mocks reviewed with the
human on 2026-10-15 (variants A–M; M on the flat table chosen).

## Governing blocks

- New in `features/ui.g5.md`: `g5.ui.home-workspaces` (the one Home list: columns, filter, sort,
  search, approvals cell, disclosure, actions) and `g5.ui.home-paging` (rows per tile and the
  setting). Main's ontology was rebuilt on 2026-10-13 (`5c66d337a`); the former
  `g5.workbench.shell-chrome` and `g5.workbench.phases` Home prose no longer exists, so nothing
  is amended there.
- Unchanged consumers: `g5.workbench.tabs` (the picker keeps the lineage tree and its own
  search; a Home row stays a fresh open), `g5.ui.home-attention` (its page count now describes a
  page of the person's size), the approvals authority in `workspaces.g5.md` (meters are read,
  never written here).

## Design

- **Owner.** `components/home/HomeWorkspacesTile.tsx` owns the list: filter, sort, search, page
  and the per-row reads (approvals, open PRs, open-question totals for the visible rows; the
  recent jobs of a working row). `pages/HomePage.tsx` supplies the organization's workspace list,
  the member roster (`useOrgMembers`, the same roster the ribbon's Owner control reads), the
  removal rule and the fork dialog. Classic Home keeps `RecentWork` unchanged.
- **Ordering.** Recently opened = the per-browser open record (`lib/recentWorkspaces.ts`), else
  `lastUpdated`; Recently updated; Created; Name. A flat list: forks sort on their own merit, and
  a row's subline names its parent is not needed because the fork picker keeps the tree.
  Base sorts like any row.
- **Filter.** Mine = owned by the caller (review copies included); All = everyone's, other
  people's review copies omitted (the rule the directory and the picker already apply).
- **Approvals cell.** `phaseApprovalMeter` per phase over the loaded approvals state: approved →
  green, pending or outdated → amber, rejected → red, no requirement → faded. Each gate shows its
  own phase, so a workspace whose only requirement (IE, say) is approved shows one green gate
  beside two faded ones; the human's "all green" is the fully required, fully approved row. The
  cell is empty when nothing at all is required (Base, no merge target), shows placeholder gates
  while the state loads, and says Unavailable with a retry when the read failed, so the column never shifts and a
  failure never reads as nothing outstanding.
- **Disclosure.** `concurrentTasks > 0` is "working": the row renders the chevron with the dot and
  loads its recent jobs; otherwise a spacer, no jobs request. The tile keeps the previous working
  set and announces a change through one polite status region.
- **URL state.** `?workspaces=all` and `?sort=` (defaults omitted) through `useSearchParams`
  with `replace`; the page size is `UserSettings.homeRowsPerTile` (5, 10, 20, 50; default 5),
  normalized like the other settings and offered on the workspace settings page. The attention
  tile passes it as the server `limit`; the issues and compute tiles page their loaded rows.
- **Removed.** `WorkspaceDirectory`, `HomePhaseStepper`/`HomeRowBadges`, the alignment full issue
  search and the Issues tile's View all (the full search was its only target under alignment).

## Assumptions and acceptance

- The members endpoint is readable by every organization member (the ribbon already relies on it);
  a missing member falls back to the raw owner id, as `ownerName` does.
- The working flag comes from the list DTO's `concurrentTasks`, as the former Working badge did.
- Tests: `HomePage.test.tsx` (tile order, filter, sort, search, paging, gates, disclosure, owner,
  delete/fork visibility, URL state, no full search), tile tests for paging, settings
  normalization, and `e2e/flows/alignment-shell.spec.ts` (the Home tiles flow).
