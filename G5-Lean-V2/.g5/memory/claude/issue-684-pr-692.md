---
name: issue-684-pr-692
description: "Workspace state for issue 684 (QA deploy Helm-before-companions + narrower queue gate), PR #692 opened 2026-10-21; what is left is live acceptance after merge."
metadata:
  node_type: memory
  type: project
  originSessionId: 9ea3d5b8-1a3d-4c16-8e04-1a4203e13c6e
  modified: 2026-10-21T05:28:57.333Z
---

Issue G5Labs/G5-Lean#684 is implemented on branch
`issue-684-qa-deploy-run-helm-before-workload-template` and published as PR #692
(2026-10-21). Scope delivered: `resolve-image-refs` waits on `push-g5d-g5dev` instead of
`publish-workload-templates`; `wait-deploy-turn` reads `steps[].status` and blocks only
while the older build's `smoke-test` is non-terminal, via the new
`scripts/deploy_queue_blockers.py`. New ontology block
`g5.quality.delivery.qa-deploy-order` in `.g5/ontology/features/operations.g5.md`; the
`g5.quality.delivery.promotion` block the issue cites was dropped in the 2026-10 ontology
rebuild, and the former `concepts/deployment.g5.md` no longer exists.

**Why:** the owner may ask for follow-up on this PR (CI, review threads, live acceptance).

**How to apply:** live acceptance is unexecuted until merge: look for a `g5-deploy-qa`
build whose `helm-deploy` starts right after `push-g5d-g5dev` and whose successor clears
`wait-deploy-turn` once the older `smoke-test` ends. The issue's optional follow-ups
(skip publication when the companion tag exists; parallel companion publication) were
deliberately left out. See [[g5-ci-cloudbuild-triage]] and
[[background-watches-die-with-session]] for CI watching.
