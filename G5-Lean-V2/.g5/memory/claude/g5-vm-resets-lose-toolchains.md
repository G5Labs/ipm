---
name: g5-vm-resets-lose-toolchains
description: A G5 MicroVM reset loses node_modules and Rust toolchains, and can also lose uncommitted work (fresh clone at the pushed head).
metadata:
  node_type: memory
  type: project
  originSessionId: 696c299f-b4d3-466f-9422-ad8ea5f52629
  modified: 2026-10-08T02:02:11.888Z
---

When a G5 session resumes in a new MicroVM, the repo checkout, the authorized
branch and uncommitted working-tree changes survive (`/work/.g5/layout.json`
records the `restore` head and `dirty` flag per repo), but **anything installed
into the guest does not**. Observed on 2026-10-08 resuming G5-Lean #349:
`frontend/node_modules` was empty and `rustup` had no toolchain, so the prior
session's work was committed-but-unvalidated and nothing was pushed.

**But not always:** on 2026-10-09 (base-pr-admin-actions) the new VM was a
fresh clone at the last pushed commit (`layout.json` `restore: null`, reflog
starting at the clone), so ~2 hours of uncommitted tests and ontology edits
after the last checkpoint were gone, and the session transcript JSONL was new
too, so nothing was recoverable.

**Why:** local files, caches and unpushed commits are not durable; only the
remote Git branch is.

**How to apply:** on resuming a session that has local changes, check
`git status`, `git rev-list --count origin/<branch>...HEAD` and whether the
toolchains exist *before* believing a failure is a code defect. Restore with
`cd frontend && npm ci` (~2 min) and see [[rust-toolchain-split-in-vm]]. Then
commit and push early and often (after every substantive step, even untested) — a validated-but-unpushed branch is one reset away from
nothing. `g5d push` does not work in the platform agent checkout ("No
workspace.yaml found"); push the authorized branch with plain
`git push origin <branch>` to the `127.0.0.1:7101` proxy remote, and use
`g5d pr create` for the PR.
