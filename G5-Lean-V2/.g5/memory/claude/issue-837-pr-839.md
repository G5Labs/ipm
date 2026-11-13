---
name: issue-837-pr-839
description: "Issue #837 (deleting a workspace closes its tabs) shipped as PR #839 on 2026-10-26; design decisions, deliberate limits, and the ship loop state."
metadata:
  node_type: memory
  type: project
  originSessionId: c02e9a96-3671-4931-ba94-7f3117d9bcc1
  modified: 2026-10-26T17:32:19.580Z
---

Issue G5Labs/G5-Lean#837 "Deleting a workspace should also close its tabs" is PR #839 (branch issue-837-deleting-a-workspace-should-also-close-its-tabs, created 2026-10-26). Contract block: `g5.workbench.tabs.deleted` in `.g5/ontology/features/ui.g5.md`.

**Why:** The in-app delete paths (Home row, workspace page dialog) already called `closeTab` since 2026-10-08; the owner's gap was deletions this browser did not perform. The org workspace list is NOT a safe deletion signal on its own: it omits agent-approval review copies and invite-only workspaces, and a stale list predates a fork. So the design is: the page closes the active tab on a confirmed 404 (re-resolve then `closeTab`), unlists a never-resolved route's tab without navigating (not-found page stays under a transient tab), and the provider prunes only tabs whose workspace was *seen listed* earlier in the session (same id under another name = rename).

**How to apply:** Keep the once-seen rule when touching `useWorkspaceTabs.tsx` reconciliation; do not close tabs on list absence alone. Ship loop: watch CI on #839, address bot findings per [[bot-review-scope-discipline]], then merge. Known limits (documented in the block's metadata): never-seen tabs stay until opened; detection waits for the detail poll (5 s) or a list refetch.
