# Review harness simplification

Scope: the [2026-10-30 request](../../chat_intent/g5-lean/review-simplification-20261030.md)
and its harness changes, not a review of the whole product or the other pending edits.

## Changes and fixes

- Replaced the discoverable ontology-review skill with short
  [deep-review guidance](../../../.agents/guidance/deep-review.md): native review/fix
  rounds, confirmed PR defects bounded by ontology and original requirements, default
  maximum 10 rounds, and a concise report. Unrequired quality-of-life features are excluded.
- Updated both instruction entrypoints, downlift's review link, the engineering
  persona/action prompts and the generated frontend fallback. Ordinary implementation
  finishes with native review.
- Retired the skill from all three generated mirrors and updated the packaged skill
  inventory check. Dedicated approval review remains separate.
- Removed the standalone implementation-testing checklist and its callers. Ontology-
  and requirements-based acceptance remains in agent instructions; planning retains
  explicit scope for expensive verification, and model-checking owns runtime trajectories.

## Ontology vetted within this scope

- `g5.harness.guidance`: amended to planning-only skill discovery and ordinary review guidance.
- `g5.harness.guidance-delivery`: checked source packaging, provider installation,
  retirement and preservation of unrelated owner files through existing tests.
- `g5.workflow.ontology.review.conformance` and `g5.workflow.ontology.review.architecture`:
  retained review against relevant ontology and requirements, including confirmed
  design and duplication defects.
- `g5.ontology.maintenance`: owning guidance statement and supporting source links updated.

## Validation

The four mirror tests, two runtime-staging tests, frontend fallback parity test and
all 12 tests compiled directly from the real harness module passed. Skill validation,
local link targets and scoped whitespace/format checks passed. The full harness crate
test is blocked on macOS by the existing Linux-only `libc::ptsname_r` call in
`g5d-foundation`.

After removing the testing checklist, all six mirror/staging tests, skill validation,
affected-link checks and scoped diff checks passed again. The standalone PR TLA
reverification guide was confirmed byte-for-byte unchanged by that cleanup.

In the earlier follow-up, four paragraphs of the standalone PR TLA reverification guide were generalized:
experiment labels and named-file dependencies became ordinary verification-task
language and baseline/risk inputs. Its five stages, obligations, gates, restrictions
and verdicts remain intact. Overlap with other guidance is retained so this procedure
stays standalone. No TLA+/TLC was run.
