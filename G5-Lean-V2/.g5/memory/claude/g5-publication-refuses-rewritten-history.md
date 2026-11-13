---
name: g5-publication-refuses-rewritten-history
description: g5d pr update fails with RECONCILIATION_REQUIRED after a squash/rebase force-push; G5 only publishes fast-forwards of the GitHub PR head.
metadata:
  node_type: memory
  type: reference
  originSessionId: 5f0f317c-e617-421c-a045-a95b1a085a79
  modified: 2026-10-23T15:41:07.292Z
---

`g5d pr update` (and the UI's Update PR) pushes to the provider branch `g5/<branch>`
only when the PR's current GitHub head is an ancestor of the new commit
(`g5-scm/src/native/member/transport.rs`, `push_commit_inner`). Otherwise it fails with
`Conflict ... ["RECONCILIATION_REQUIRED"]`. There is no force option.

Seen 2026-10-23 on #624 / PR #645: the user asked to squash 48 commits and rebase on main.
Force-pushing the workspace branch (`refs/heads/<branch>`) worked, but GitHub kept the old head.
The user then moved the provider branch to the new head outside G5. After that,
`refs/g5/provider/.../heads/g5/<branch>` matched, CI ran on the squashed head, and later
fast-forward `g5d pr update` pushes published normally.

**How to apply:** before squashing or rebasing a branch that has an open G5 PR, say that
G5 cannot republish the rewritten history to that PR. Options: merge the old head with
`-s ours`, which keeps the old commits in GitHub's list; `g5d pr create --new`, which
replaces only a PR that was merged or closed outside G5; or the user force-pushes the
provider branch outside G5. Related: [[g5d-pr-update-rewrites-shas]].
