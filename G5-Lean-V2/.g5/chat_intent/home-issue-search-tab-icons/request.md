# Home issue search and phase tab icons — request and interpretation

## User instructions (2026-10-20)

- "create a PR to add a little search bar in the homepage for the issue panel (there was a
  full one that was removed recently, but we don't want it that big)"
- "change the icon on the top tabs to be the icon of BE/SE/IE instead of the 'Databricks'
  icon. We don't want this icon" (the `lucide-layers` glyph on every workspace tab).

## Agent interpretation

- The Issues and tasks tile gains a compact search field in its header, not a section of
  its own: typed terms go to the tile's one organization search (GitHub issue syntax, as
  the retired full search did) after a typing pause or on Enter, an unfinished qualifier
  never searches on a pause, and clearing returns to the cached termless slice at once.
  The assignee and label filters keep narrowing whatever the search loaded on the client.
  The search text is page state, not URL state.
- Every workspace tab shows its workspace's phase icon (briefcase, blocks, plug) in the
  phase hue, read from the organization's workspace list that Home already loads; a
  workspace the list lacks shows no glyph, and an older server without `phase` shows
  Business Engineering, the DTO default. The switcher and lineage rows keep their glyph.
