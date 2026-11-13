---
name: no-git-stash-in-shared-checkout
description: Never create a stash in the checkout, not even as a placeholder in a command — it also sweeps my own uncommitted tracked edits; compare with git show HEAD:path or a scratch worktree, and if a stash was created, pop it straight back.
metadata:
  node_type: memory
  type: feedback
  originSessionId: 3bb15e4c-f1d6-4ec1-9fec-8d303284694e
  modified: 2026-10-27T04:56:00.000Z
---

On 2026-10-23 (issue #750), a Codex agent in the same MicroVM edited a file and committed it seconds after I ran `git stash`/`git stash pop` to compare a test against the prior state. My stash briefly swept its uncommitted edit, and a later `stash push` silently saved nothing. Nothing was lost, but only by luck of the timing.

**Why:** stash rewrites the shared working tree and index that the other agent is using right then; `ps` showed `codex exec resume` running since well before my session.

**How to apply:** before Git operations, check `ps -eo pid,lstart,cmd | grep -iE "claude|codex"` and the reflog. Compare baselines with `git show HEAD:<path> | <tool> --stdin…` or `git worktree add`, never `git stash`. If a modified file appears that you didn't touch, read its diff and leave it to its author. Related: [[g5d-pr-update-rewrites-shas]].


**Second incident (2026-10-27, rich-text-editing):** I typed `git stash -q 2>/dev/null; echo "(skipped stash)"` as a placeholder inside a lint command. It ran, and all five of my tracked edits (DocumentPreview, FileView, fileDrafts, CodeEditor, index.css) silently vanished; untracked new files stayed, so the component tests failed with "no buttons" and I spent a round of debugging on the wrong cause. `git stash list` showed it; `git stash pop` restored everything. Symptom to recognize: `git status` shows only new/untracked files when you expect tracked modifications. Never put `git stash` in a command at all, even behind an echo.
