# Preserve the deployed Filestore backup behavior in #1094

## Ontology changes and governance

Comparison: PR head `10ab61d90058de066d9ee2c30945c4f180cc6855`.
The original PR had no saved design; this bounded correction reconstructs scope
from [#1092](https://github.com/G5Labs/G5-Lean/issues/1092), the pinned
[handoff](https://github.com/G5Labs/G5-Lean/tree/1076d9a923e8202794b8bd4f37fdfeec81daf53e/docs/filestore-backup-terraform-handoff),
and the user's request to fix the reviewed compatibility/safety defects.

Changed block: `g5.quality.delivery.backups.filestore` in
[operations](../../ontology/features/operations.g5.md#g5.quality.delivery.backups.filestore).
It now explicitly preserves deployed names/labels/runtime authority, full elapsed
retention, deletion rechecks and deadlines, and requires readiness before the
Workflow handover. The diff is recorded in `ontology.diff`; existing SQL backup
policy and other deployment behavior do not change.

| Governing rule | Obligation |
| --- | --- |
| Filestore backup adoption above | Preserve the running automation while changing its management owner. |
| Parent `g5.quality.delivery.backups` | Leave existing SQL backup/PITR configuration unchanged. |
| Repository durability | Native backups do not establish SQL/Git coordinated recovery; #977–980 remain separate. |
| #1092 execution boundary | Code/review only: no apply/import, deletion, deployment or merge. |

## Correction

`Scheduler → fixed deployed Workflow → READY backup → rechecked retention`

Reuse the pinned, already-deployed Workflow verbatim, its five environment
variables and both exact custom-role definitions. Keep the PR's module wiring,
state ownership, additive IAM members and resource imports. Extend tests around
the real YAML for live-shape backups, same-day replay, full retention boundary,
changed ownership, 404 replay, pagination, partial deletion failure and deadline.

The provider cannot import this Workflow. Document a controlled adoption using
a reviewed pinned checkout, backed-up live definition/state, verified bootstrap
and a ready saved platform plan. Keep automatic applies paused during handover;
delete only when the controlled apply is ready, recover immediately on failure,
verify execution, then resume applies/merge. No cloud action is part of this fix.

## Initial findings and acceptance

The reviewed version changed labels/names (stranding existing backups), shortened
retention to a calendar-day cutoff, removed per-delete rechecks and the operation
deadline, and introduced `file.instances.get`. All 11 prior tests passed because
they encoded the rewrite. These findings are retained in `review.md`.

Accept when the Workflow matches the pinned/live source, exact runtime permissions
and environment values match the handoff, regression checks discriminate the
removed protections, and changed Terraform roots validate and pass focused tests.
Real adoption and destructive retention are unexecuted operator checks, never
substituted by mocks. Local regression and independent review results are in `review.md`;
Claude Code follow-up has no remaining actionable findings; updated-head CI
is pending.

## Review summary

The bounded correction restores the original adoption contract without new cloud
resources or recovery machinery. The corrected module preserves observed Workflow logging/metadata; history
export and final-merge-plan checks are part of the runbook. Live workflow/source
metadata was read-only verified in QA, staging and prod on 2026-11-04. Cloud rollout remains pending.
