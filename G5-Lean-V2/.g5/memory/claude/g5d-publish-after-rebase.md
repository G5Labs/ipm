---
name: g5d-publish-after-rebase
description: "After rebasing a published G5 branch, `g5d pr update` refuses with RECONCILIATION_REQUIRED until `g5d workspace refresh` records the force-pushed tip."
metadata:
  node_type: memory
  type: reference
  originSessionId: 15961965-e3c0-40a6-b3f9-a58776087e65
  modified: 2026-10-23T20:24:20.099Z
---

G5's publication push (`g5-scm` `push_commit`) requires its recorded branch head to be
an ancestor of the new head. A rebase breaks that, so `g5d pr update` exits 4 with
`["RECONCILIATION_REQUIRED"]`, and GitHub keeps the old head as CONFLICTING, even
after `git push --force-with-lease` to the workspace origin succeeded (seen on PR #781,
2026-10-23).

**How to apply:**
1. `git push --force-with-lease=<branch>:<pre-rebase tip> origin <branch>`.
2. `g5d workspace refresh`, which records the new tip.
3. `g5d pr update`. It then publishes, and GitHub turns MERGEABLE.
4. Read HEAD again afterwards ([[g5d-pr-update-rewrites-shas]]).
