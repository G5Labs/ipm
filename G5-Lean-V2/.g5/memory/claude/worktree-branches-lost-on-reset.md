---
name: worktree-branches-lost-on-reset
description: Subagent work on local worktree branches is lost when a VM restarts as a fresh clone; merge it into the authorized branch and push at every checkpoint.
metadata:
  node_type: memory
  type: feedback
  originSessionId: 3b3796e3-749b-4617-b6bb-690542ab3824
  modified: 2026-10-18T13:59:29.991Z
---

On 2026-10-18 (G5-Lean#624), a runtime-slice subagent worked for about 2 hours in a local worktree branch (`g5-624-runtime`). The G5 rule allows pushing only the authorized branch, so those commits were never pushed. The next VM was a fresh clone at the pushed head, which restores only the authorized branch, so the ~1,700 lines of work were gone. The frontend agent's work survived only because it had already been merged into the authorized branch and pushed.

**Why:** a resumed MicroVM keeps the checkout, but a fresh clone at the pushed head keeps only what is on the remote, and which one a session gets is not under its control. Local worktrees, local branches and `/tmp` are therefore not durable ([[g5-vm-resets-lose-toolchains]]).

**How to apply:** when parallel agents work in worktrees, have them commit small steps, and merge their branch into the authorized branch and push at least every 30–45 minutes, even WIP. Alternatively, have the agent work directly in the main checkout on disjoint files. Never leave hours of agent work only on a local branch.
