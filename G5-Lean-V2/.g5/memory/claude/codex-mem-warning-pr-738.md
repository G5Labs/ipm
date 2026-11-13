---
name: codex-mem-warning-pr-738
description: "PR #738 (branch codex-mem-warning) silences shared_memory_skipped for by-design disabled memory; ship-readiness loop started 2026-10-23."
metadata:
  node_type: memory
  type: project
  originSessionId: 3eeea6a2-3aef-422c-aa81-a1e3ad32803f
  modified: 2026-10-23T00:56:23.238Z
---

Branch `codex-mem-warning`, GitHub PR G5Labs/G5-Lean #738, opened 2026-10-23. Adds
`memory::Binding::Disabled` (Codex turns + formal reviews) so the "Shared Codex memory is
unsupported" notice no longer posts; `shared_memory_skipped` fires only for real
`memory::prepare` skips. No linked issue or Project Docs.

**Why:** the owner (zcl-g5) asked to file the PR and drive it to ship-ready: green checks
(g5-ci-rust, g5-ci-e2e), inline replies to Codex P0/P1 and CodeRabbit Critical/Major,
no merge conflicts; email the owner when done or blocked.

**How to apply:** on resume, re-check `g5d pr checks`, `g5d pr view 738 --json mergeable`
and `g5d pr view 738 -c` fresh. Facts worth knowing: formal reviews set
`CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` themselves (formal_review.rs) and pass their own
`--settings`, never `claude_settings`; the journal kind for notices is `module.notice`.
Apply [[bot-review-scope-discipline]] to bot findings.

First e2e run (build 7d17888a, 2026-10-23) failed only because the browser shard receipt
rejected a retry-passed `workspace-preview.spec.ts › M12 Stop fences a live Preview…`
(VM status poll timeout); every provider-agent spec passed. Reran via a new push. The one
CodeRabbit thread (Trivial: non-empty replay) was declined inline — unreachable after the
succeeded-outcome assertion.
