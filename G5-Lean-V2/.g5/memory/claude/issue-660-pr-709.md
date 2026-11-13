---
name: issue-660-pr-709
description: "Workspace FVB-i660-F1 state — issue #660 credential/mutation authority repairs shipped as PR #709 on 2026-10-22; design decisions, deferred items and the ship-readiness loop."
metadata:
  node_type: memory
  type: project
  originSessionId: a5249e53-7356-4b88-a7f7-a5c0aae6489a
  modified: 2026-10-22T03:13:28.230Z
---

Issue [G5Labs/G5-Lean#660](https://github.com/G5Labs/G5-Lean/issues/660) → PR
[#709](https://github.com/G5Labs/G5-Lean/pull/709), branch
`FVB-i660-F1-credential-and-mutation-authority`, created 2026-10-22. Design:
`.g5/pr_ontology_diff_design/issue-660/ontology-plan.md`; intent:
`.g5/chat_intent/issue-660/request.md`.

**Decisions to defend in review:** delegated API keys cannot create or revoke keys
(person session only; no parent/child key lineage); keys previously created by keys
are unattributable and stay ordinary org keys; viewer ceiling = read-only (`*:read`)
additive scopes, members/admins keep grants; refresh credentials are SHA-256 digests
with migration 105 rewriting rows in place (one-way, single-deploy switch);
`decline_invitation` was an extra sibling made conditional.

**State 2026-10-22 06:20 UTC:** head `2e1c2e05`, g5-ci-rust and g5-ci-e2e green, merge
state clean, owner emailed. CodeRabbit: 3 Minor + 1 Trivial fixed and resolved; 2 Major
on migration 105 replied inline (staged cutover deferred as the owner's practice
decision, thread left open; lock duration measured 34 µs/row, accepted). No Codex
review arrived. Local full suite 1663/1664 (api_intent_log Git-identity VM mismatch).

**Owner's ship loop:** keep `g5-ci-rust`, `g5-ci-e2e` green, no merge conflicts
(main moved once already; merged `a6e8aa7e`), answer every Codex P0/P1 and
CodeRabbit Critical/Major inline, validate findings against the design before
changing code. Email the owner with `g5d workspace email-owner` when ready or blocked.

**Why:** the owner asked for design → implement → PR → ready-to-ship with reviewer
triage; a resumed session needs the agreed scope to avoid re-litigating it.
**How to apply:** re-check `g5d pr checks` fresh at session start; do not expand the
PR for out-of-scope reviewer requests, reply "not applicable, out of scope".
Related: [[bot-review-scope-discipline]], [[sqlx-prepare-after-schema-change]].
