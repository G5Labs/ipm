---
name: commit-only-own-paths-in-shared-checkout
description: A plain `git commit` in a shared checkout sweeps in a peer session's staged changes, and `g5d pr update` auto-saves the rest; commit with explicit pathspecs and check the index first.
metadata:
  type: feedback
---

On 2026-10-26, on PR #712's branch, a peer Claude session (`work-db`) had staged deletions of migration 121 and its test while it folded the migration into 119. My `git add <my paths> && git commit` committed the whole index, so its deletions went into my commit. `g5d pr update` then auto-saved its unstaged migration edits as "Save workspace changes for pull request", and a follow-up `git push` put both on the workspace origin.

**Why:** the user's CLAUDE.md says other agents share the checkout and only your own task's changes may be committed. `git add` limits what you stage, not what is already staged, and `g5d pr update` saves every pending change.

**How to apply:**
- Before committing, run `ListAgents` and `git diff --cached --stat`.
- Commit with `git commit -- <your paths>`, which leaves others' staged work staged.
- Never run `g5d pr update` while a peer has uncommitted changes. Push with git and publish only after coordinating.

Related: [[no-git-stash-in-shared-checkout]], [[g5d-pr-update-rewrites-shas]].
