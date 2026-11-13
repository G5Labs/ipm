---
name: issue-736-pr-737
description: "Issue #736 (Base tab had no icon) fixed in PR #737 on 2026-10-23; root cause was case-sensitive tab/list name matching; CI watch pending at handoff."
metadata:
  node_type: memory
  type: project
  originSessionId: 438e333f-3051-437f-9176-ab1541f0dc35
  modified: 2026-10-23T02:30:49.811Z
---

Issue G5Labs/G5-Lean#736 "Base tab doesn't have any icon" is fixed by PR #737 (branch
`issue-736-base-tab-doesn-t-have-any-icon`, created 2026-10-23). Head 06cf2116 passed
g5-ci-rust and g5-ci-e2e; head bd8c443e added the CodeRabbit-prompted visit-key fix (visit
keyed by the routed name, memory by the folded identity) and its thread is answered and
resolved.

Root cause: the app routes Base at `/:org/base` (settings redirects, default landing) while
the workspace is listed as `Base`; the server resolves names case-insensitively but the tab
strip matched the list by exact name, so Base got no phase glyph and a Home row could open a
second Base tab. Fix: `workspaceTabKey` folds the name for tab identity and memory keys; the
strip matches the list by folded name and shows the listed name.

**Why:** a lower-case route segment for a workspace is normal in this app, so any per-name
lookup against the workspace list must fold case like `forgetWorkspaceResolutions` does.

**How to apply:** when a strip or list feature keys by workspace name, use `workspaceTabKey`
or `toLowerCase()` on both sides. At handoff the CI watch on #737 had not yet reported;
re-check `g5d pr checks` fresh before trusting an earlier watch (see
[[background-watches-die-with-session]]).
