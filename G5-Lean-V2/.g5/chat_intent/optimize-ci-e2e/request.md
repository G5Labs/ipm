# CI E2E runtime (issue #773)

## Human instruction (2026-10-23)

Asked how to proceed with no saved plan and 24 open questions (Project Docs
`open-questions.md`, all undecided), the owner answered:

> just follow the doc `g5-ci-e2e-investigation.md` to implement, that should be clear enough already

The investigation is the Project Doc
`docs/4e9d4a67-4270-4156-8f28-0212f26c5a8a/g5-ci-e2e-investigation.md`; the source issue is
[G5Labs/G5-Lean#773](https://github.com/G5Labs/G5-Lean/issues/773).

## Interpretation (agent)

- The investigation's ordered recommendations are the scope: duration-based browser
  rebalancing first; then one execution owner for the duplicated formal-review test,
  review-fixture phase measurement and the RV9 terminal-reuse experiment; then the
  release/frontend-test overlap canary; bounded failures at the confirmed terminal
  provider condition with retry and capacity-wait time reported separately.
- The investigation keeps whole-file ownership, the exact test-ID partition, all
  receipts and per-test fixture isolation. Fixture reuse (shared actors or a shared
  starting point) is gated on its own measurement ("instrument these phases first"),
  so only the measurement ships here; the open questions on reuse stay open.
- It says to retain CI image layering "independently" and to evaluate more workers
  "only then". Neither is part of this change.
- It treats 35–40 minutes as a warm-run experimental target, not a pass/fail gate.
  The PR's own CI runs are the canaries; the open questions on target, evidence and
  retry policy stay open and are not answered by this instruction.
