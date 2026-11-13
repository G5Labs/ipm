# Review of #1094 preservation correction

## Initial findings (head 10ab61d90)

- New names/labels exclude the deployed daily/initial backups from retention.
- Calendar-date cutoff can expire a backup before 30 elapsed days.
- Deletion no longer rechecks ownership/state/source/share/age or skips 404.
- Operation polling has no deadline before the separate READY timeout.
- Runtime adds file.instances.get despite the adoption-only scope.
- Handover previously asked to delete before waiting for merge-triggered apply.

Evidence: read-only comparison against all three deployed workflows and backup
metadata; the original 11 tests passed; evaluated predicates reproduced the
legacy-label exclusion and early expiry. No cloud mutation was performed.

## Correction and verification

The corrected Workflow is byte-identical to pinned handoff commit
`1076d9a923e8202794b8bd4f37fdfeec81daf53e` (SHA-256
`34f8ee2098bba0388b907e1cf1cc05233065504d5ee95ccd7668958fa11e9f15`).
Read-only observations of all three deployed definitions matched that source.
The original five environment values and both runtime permission sets are
restored. Existing daily/initial backups remain recognized and same-day names
remain reusable.

| Commitment | Actual path and check | Result |
| --- | --- | --- |
| Original names/ownership and initial/daily retention | Workflow configure/create/is_expired; same-day 409 and synthetic live-shape inventories | Passed |
| Full elapsed 30 days/manual preservation | is_expired full timestamps; exact boundary, subsecond, recent/manual/foreign cases | Passed |
| Source/share/labels/state/age at deletion | delete_expired reread and is_expired; each mutable attribute changed after listing | Passed |
| Replay/partial failure | 404 at reread/delete continues; failed create/list/delete operation stops before further effects | Passed |
| READY before complete pagination and checked deletion | Scripted public main flow enforces ordered calls across multiple pages/candidates | Passed |
| Bounded creation/readiness/deletion | Real YAML control flow with virtual clock reaches 24-hour failure; per-delete budget checked | Passed |
| Unchanged runtime authority and fixed config | Compared exact permission sets; module assertions and root wiring tests | Passed |
| Recoverable handover, existing data unchanged | README coordinates writers, existing state, bootstrap and saved apply before deletion, with recovery and execution checks | QA adopted and verified below; staging/production pending |

Validation: 17 Python offline tests passed with both ordinary Python and
`python3 -S` (gcloud YAML fallback); 56 Terraform mock plan tests passed across the
changed modules/roots; ten directories passed init with backend disabled,
validate and formatting. Versions: Terraform 1.9.8, google 5.45.2, google-beta
7.46.1, kubernetes 2.38.0 and random 3.9.1. Staging/prod platform roots have no
individual test runs; their validation and shared release-platform tests passed.

An independent reviewer reran the then-current 16 behavioral tests and confirmed
closure of the original four workflow findings. The later addition of
`test_workflow_matches_pinned_deployed_handoff` raised the final suite to 17 by
checking the source digest; the earlier and final test snapshots confirm this
one-test difference. In-memory mutations of naming, retention boundary,
deletion recheck and operation deadline fail the applicable tests. No mutated
source was written. No new actionable defect was found in that bounded review.

Claude Code's first pass found no Terraform/Workflow code defect, but requested
more complete metadata/history preservation and clearer handover verification.
The correction now preserves the observed Workflow description, ownership label
and LOG_ERRORS_ONLY setting. The pinned provider lacks a history-level field;
the runbook requires checking Basic default behavior and refuses unexpected
history/encryption/IAM drift before deletion. Execution summaries and needed
details must be exported because recreation cannot recover execution history.
The runbook now reviews actual retention candidates before verification and
requires a fresh plan of the exact merge commit before resuming writers.
Additional tests pin the Workflow source digest, metadata, attachment, HTTP
headers/deadline and a nine-digit exact-boundary timestamp (sub-microsecond
Workflows behavior remains unverified). Claude Code follow-up reported **no remaining actionable findings** after these
corrections. This was source review only, not test execution by Claude. Subsequent test-only adjustments normalize timestamp fixtures to microsecond
precision on older Python. The optional computed service-account equality check
was inspected in source instead: provider plan mocks leave it unknown; a mocked
apply passed all assertions but could not tear down due to prevent_destroy.
Production destruction guards were retained, the suite uses plan mocks again,
and actual attachment remained a live adoption verification requirement; QA's
subsequent operator verification is recorded below.
All six checks passed on corrected head
`dd39b03987f6782747e850464f68dcf818d2b59a`.

## Incremental handover review (2026-11-05 UTC)

CodeRabbit's [follow-up](https://github.com/G5Labs/G5-Lean/pull/1094#discussion_r4475386574)
identified that the pre-deletion check listed only ACTIVE executions. A QUEUED
backup could therefore be missed and lost when the Workflow was deleted. This
was a later runbook finding; the restored Workflow itself is unchanged.

The correction requires both ACTIVE and QUEUED executions to drain while new
executions are prevented, and requires the list command to succeed and return no
matches before deletion. Google's [execution states](https://docs.cloud.google.com/workflows/docs/reference/executions/rest/v1/projects.locations.workflows.executions#State)
and [gcloud filter syntax](https://docs.cloud.google.com/sdk/gcloud/reference/topic/filters)
confirm `state=ACTIVE OR state=QUEUED` covers both states. The installed gcloud
filter parser accepted the expression: ACTIVE and QUEUED matched, while all five
other current enum states did not. `git diff --check` passed. Source inspection
confirmed the change is limited to this review record and the README; existing
Workflow/Terraform test evidence remains applicable.

Operator preflight then found that `gcloud workflows get-iam-policy` is not a
valid command. The [Workflows API](https://docs.cloud.google.com/workflows/docs/reference/rest/v1/projects.locations.workflows)
has no Workflow-resource IAM policy methods; [Workflows roles](https://docs.cloud.google.com/workflows/docs/access-control)
are granted at project level or above. The runbook now exports project IAM,
compares the relevant backup/runtime/apply bindings and preserves existing
members and complete conditions. Post-handover and recovery checks use that
same scope; rollback explicitly must not reapply an entire exported project
policy. Inherited and service-account-level attachment grants still require
effective-permission verification. Official command/API documentation and the
installed gcloud source confirm the replacement command; the project policy
reader requests version 3 so conditional bindings are retained. The earlier
Claude review did not catch the invalid Workflow IAM command; its original
review result above is retained as historical evidence, not closure of this
later finding.

Claude Code's supplemental documentation review found four issues: the private
parser-fix scope, the
16-to-17 test sequence, history repair placed before recreation, and ambiguous
merge ordering. Its follow-up confirmed those corrections and identified a
remaining paused-Scheduler verification gap. The runbook now preserves a prior
pause while requiring an approved manual execution and the same backup checks;
an unverified handover cannot unblock merge or target applies. A local review
also clarified QA-first merge: staging/production can follow while held, and
both target handovers and exact-final-merge-commit plans must pass before target
applies or the release train resumes. The final supplemental documentation
review reported **no remaining actionable findings**. This was text-only review
with tools disabled; the prior runtime/source review remains clean. Documentation
review is separate from the live operator evidence.

## Authorized QA handover observations (2026-11-05 UTC)

The operator's full platform apply succeeded and adopted all eight backup
resources at state serial 28. The subsequent strict metadata check failed.
Audit evidence shows the completed Workflow creation at 01:58:51 UTC returned
ACTIVE with `executionHistoryLevel` omitted; the recovery update at 01:59:06 UTC
returned explicit `EXECUTION_HISTORY_BASIC`. The observed Terraform-managed
source, environment, service account, labels and log level match the original.
Recovery restored the original metadata and enabled Scheduler without a second
deletion. These observations do not establish more about the failed comparison's
cause. The runbook now documents a supported in-place update to explicit Basic
when needed for strict metadata verification, keeping provider 5.45.2.
The fresh read-only post-recovery plan passed with no changes (build
`ec37e603-93c4-43ff-bfbc-41b1717237ef`, plan SHA-256
`8d5b218dd68280775d2557e3ace03665f15f5d0d55a885df62de983a18bcd165`).
The first backup-verification build stopped before invocation on nine-digit
timestamp parsing. The parser correction changed only the private, one-off
Cloud Build verification script in `qa-backup-verification-build.json`; it was
not a repository change and has no commit. It changed neither the checked-in
Workflow nor `scripts/test_filestore_backup_workflow.py`. The operator checked
three fractional-second lengths and shell/Python syntax before retrying.
The local and actual QA Workflow source still match SHA-256
`34f8ee2098bba0388b907e1cf1cc05233065504d5ee95ccd7668958fa11e9f15`.
Repository changes after tested head `dd39b03987f6782747e850464f68dcf818d2b59a`
are limited to this review record and the README. After that tooling-only fix,
verification build
`6ea29e8d-fba5-4726-8047-f190fc04d138` succeeded. Workflow execution
`projects/89784110315/locations/us-central1/workflows/g5-filestore-backup/executions/0c434009-2ddc-4ba4-a8f9-c610c2c31f33`
reached SUCCEEDED. The daily `g5-lean-qa-repos-daily-2026-11-05` backup is READY;
the inventory grew from two to three backups, `deleted` was empty and all
original backup names remain present. The backup Scheduler is ENABLED.

Staging/production adoption remains pending. All four Terraform/release-train
triggers and the release-train Scheduler remain held pending merge and checks.
No merge has been performed.

## Limits

Offline tests execute a small interpreter of the actual YAML; they do not prove
Cloud Workflows compilation, HTTP retry timing, IAM integration or real API
concurrency. The original correction was code/review only; the later authorized
QA operator handover, clean post-recovery plan and successful live backup
verification are recorded above. Staging/production adoption and live
destructive-retention testing remain unexecuted. The QA run deleted no backups;
it does not establish live expiry/deletion behavior.
