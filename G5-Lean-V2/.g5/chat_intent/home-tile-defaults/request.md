# Home tile defaults, name links and owner avatars — request and interpretation

## User instructions (2026-10-23)

- "The headers are in two separate rows, fix it. You can maybe move the search bar or the
  refresh button next to the labels" (the Issues and tasks tile header on Home).
- "Everything should default to your stuff, so Issues should default to 'Assigned to me' and
  that option needs to go before 'All', and running compute needs to default to 'Mine' and
  that option needs to go before 'All'."
- "Everything should be clickable as a name, so the open questions should be clickable to go
  to the document directly, and the running compute should be clickable to go to the
  workspace directly. Then in running compute, instead of the open button put a stop button."
- "The owner column in workspaces takes quite some space. Put only the avatar/initials for
  the owner, with a tooltip for the full name. The saved space can be used for the workspace
  name, so it's not truncated like now. Wrap the workspace name if too long instead of
  truncating."

## Agent interpretation

- The Issues and tasks header keeps the title, the assignee filters, Create and Refresh on
  one line; the compact search moves into the band above the list, to the right of the
  label chips, which wrap among themselves without pushing it down.
- Assigned to me and Mine are the initial filters and the first chips; All stays one press
  away. Without a connected GitHub account the Issues tile therefore opens on its connect
  hint.
- The Needs Your Attention row name links where its action does (the document for an open
  question, the approvals view for an approval); the Review / Answer action stays.
- The Running compute row name links to the workspace; Open goes; Stop (the existing
  owner-only stop, relabelled) is the row's only control.
- The Workspaces owner cell is the avatar alone with the full name as its tooltip and as
  screen-reader text; the name column takes the freed width and wraps long names.
