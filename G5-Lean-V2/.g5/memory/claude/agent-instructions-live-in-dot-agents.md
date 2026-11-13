---
name: agent-instructions-live-in-dot-agents
description: "Workspace agent instructions (the harness text listing g5d commands) live in the repo at .agents/AGENTS.md; this VM's grep (ugrep) skips hidden directories, so a plain grep -r misses them."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 6d51ceb7-e70d-4da7-9d88-d278ea06b47a
  modified: 2026-10-19T05:05:56.330Z
---

The instructions G5 gives workspace agents live in this repo at `.agents/AGENTS.md`.
They include the `g5d issue`, `g5d question` and `g5d workspace email-owner` guidance and the
"G5 workspace" rules. `infra/docker/Dockerfile.g5d` and packer copy that file into
the worker runtime's `assets/harness/AGENTS.md`, which becomes the providers'
`CLAUDE.md` and `AGENTS.md`. The repository-root `AGENTS.md` is different: it governs
development of this repo. `.agents/skills/` holds the ontology skills.

**Why:** on 2026-10-19 (#621) I reported this harness text as "outside this repo"
because `grep -r` found nothing. In this VM `grep` is ugrep, which skips hidden
directories by default. The owner corrected me: "agent instructions should live in
.agents/".

**How to apply:**

- When a feature adds or changes an agent-facing command or rule, update
  `.agents/AGENTS.md` in the same PR, in the existing dense bullet style.
- When searching for text that may live in dot-directories (`.agents`, `.g5`,
  `.github`), pass `--hidden` or search the directory explicitly.

Related: [[incarnation-routes-system-data]].
