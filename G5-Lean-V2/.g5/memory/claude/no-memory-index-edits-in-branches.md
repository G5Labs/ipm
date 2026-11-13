---
name: no-memory-index-edits-in-branches
description: Never change .g5/memory/claude/MEMORY.md in a feature branch; index edits there cause artificial merge conflicts.
metadata:
  node_type: memory
  type: feedback
  originSessionId: 609cd645-6f21-4544-8740-e6f4278c9f6b
  modified: 2026-10-27T01:09:55.339Z
---

Do not modify `.g5/memory/claude/MEMORY.md` in a feature branch, and do not commit index
lines there. New memory files are fine; leave the index pointer out.

**Why:** the owner said (2026-10-27, PR #811) that branch edits to MEMORY.md are "a source
of artificial merge conflicts": every concurrent branch appends to the same lines, and
G5's Rebase-and-merge lands each commit, so even a later revert keeps the conflict.

**How to apply:** when saving a memory in a branch workspace, write the memory file only
and tell the owner it is unindexed. If a branch commit already touched MEMORY.md, strip it
from that commit during the next rebase (restore the parent's version) rather than adding
a revert commit. Related: [[g5d-publish-after-rebase]].
