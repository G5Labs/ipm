# Evidence for where discovery and repair failed

This is a reporting audit of the completed experiment, not another worker run. Read the
[README](README.md) for the explanation. The original findings, patches, test results and
source assessments are unchanged.

## How the stages were distinguished

- **Rule available:** the common packet contained the original plan and corrected ontology.
- **Rule retrieved:** its text appeared in a public tool result. This proves exposure,
  not attention or understanding. A search hit can return a rule without the whole block.
- **Issue found:** an actionable violation was identified in the initial or later worker
  report and supported by the independent source assessment. Mentioning the topic or
  checking a related behavior is not enough.
- **Issue repaired:** the final implementation satisfies the specific evaluated requirement.
  Runtime checks and source-only conclusions are distinguished below.

An unreported violation is classified as missed in the preserved findings; this does not
claim access to private reasoning. Initial means before production changes. Later means
identified during the subsequent review/repair work, before evaluator feedback.

## Which workers found and fixed each issue?

| Issue | Found initially | Found later | Correctly repaired within the stated checks | Remaining failure |
|---|---|---|---|---|
| Accepted model preservation | u07, u08 | u03, u04, u05 | u03, u04, u05, u08 | u07 detected drift but rejected still-valid accepted work. u01, u02, u06, u09 did not identify drift. |
| Future shutdown warning | None | None | None | All nine missed the exception and retained the refusal. |
| Workflow concurrency limit | u02, u07 | u06 | u02, u06, u07 | u01, u03, u04, u05, u08, u09 did not identify missing agent cap enforcement. |
| Task/initial-input ownership | u02, u04, u05, u06, u08 | u07, u09 | u02, u04, u05, u06, u07, u08, u09 | u01, u03 did not identify the ownership violation. |
| Recovery without preview | u01, u02, u03, u04, u05, u07, u08 | u06, u09 | All nine | None at the direct replay component boundary. |
| Recovery visible after stop | u09 | u02, u04, u05, u06, u07 | u02, u04, u05, u06, u07, u09 | u01, u03, u08 fixed the component but missed its parent's visibility gate. |
| Synchronization subset display | None | u05 | None fully | u05 copied whole-source context instead of the selected subset; the other eight did not report the missing context. |

These sets reproduce the adjudication ledger (`adjudication-ledger.json`). UI recovery was
one broad original target; it is split here because a working component and a reachable
component are different requirements. Discovery of missing synchronization context does
not mean u05 also recognized the subset error in its own repair.

## What text was actually returned?

The retrieval audit (`retrieval-observations.json`) records exact phrases, call IDs,
timestamps and whether each appeared before the findings checkpoint.

| Requirement | Text checked | Visible result |
|---|---|---|
| Accepted model | Plan: “explicitly validate and freeze the new accepted launch specification” | Returned before findings for all nine. This is plan exposure, not proof that all retrieved the separate ontology launch block. |
| Concurrency limit | Ontology: “enforcing workspace slots and optional job caps for agents and commands” | Returned before findings for all nine. |
| Task/input ownership | Ontology: “TaskService's complete allocation operation composes task/initial-input creation” | Returned before findings for all nine. |
| Future shutdown warning | Ontology: “During a valid future-deadline nudge, the VM remains running with admission open” | No visible exact match in any run. |
| Browser recovery | Plan: “Keep the client key stable across an uncertain submission and offer safe recovery after reload” | Returned before findings for eight; no exact match for u03. |
| Progress | Plan: “count only new tasks in this run's progress” | Returned before findings for eight; no exact match for u03. This is a broad requirement, not a direct diagnosis of missing synchronization context. |

A supplementary inspection searched all nine final public histories' tool outputs,
case-insensitively, for `nudge`, `future.deadline` and `g5.lifecycle.deadline`; none matched.
No explicit cat/sed/rg command naming `lifecycle.g5.md` was found. This strengthens the
observable retrieval-gap finding, but truncation or differently expressed information
could still hide exposure. We do not label absence as proof the agent knew nothing of it.
Public histories are retained locally under `../../runs/2026-09-28-review-fix/`.

## What supports the final verdicts?

- **Accepted model:** the same independent dispatch test changes defaults after acceptance.
  Source inspection checks use of the accepted model and continued account authorization.
  u07's source report traces its permanent-failure path; its own final report calls it repaired.
- **Shutdown warning:** independent API check plus source tracing of the real lifecycle
  writer that leaves admission open until the future deadline.
- **Concurrency:** independent sequential reservation check with two ready descendants and
  a limit of one. This is not proof of concurrent race safety; u02 has an unexecuted race concern.
- **Ownership:** independent source tracing of the task and first-input writes, not a
  runtime inference from atomicity or rollback.
- **Recovery:** independent direct-component replay check; source tracing of the real parent
  establishes stopped-page visibility. These do not constitute a full browser/Stop journey.
- **Synchronization:** independent source tracing into the progress builder and its UI.
  u05's copied-JSON check uses a generic job and does not execute the affected sync projection.

All verdicts use the original governing rules. A worker's later plan or ontology amendment
cannot redefine a failed requirement. The independent suite used a documented uniform
request-format adapter for u06's new fingerprint; original raw failures were preserved.
Test results (`test-results.json`), [adapter and evaluator corrections](evaluator-corrections.md).

## Direct evidence by worker

| Worker | Procedure, all with ontology | Initial findings | Final worker report | Independent source assessment |
|---|---|---|---|---|
| u01 | Normal | Initial (`deliveries/u01/initial/initial-findings.md`) | Final (`deliveries/u01/final/review.md`) | Assessment (`source-adjudications/u01.md`) |
| u02 | Old skill | Initial (`deliveries/u02/initial/initial-findings.md`) | Final (`deliveries/u02/final/review.md`) | Assessment (`source-adjudications/u02.md`) |
| u03 | Revised skill | Initial (`deliveries/u03/initial/initial-findings.md`) | Final (`deliveries/u03/final/review.md`) | Assessment (`source-adjudications/u03.md`) |
| u04 | Revised skill | Initial (`deliveries/u04/initial/initial-findings.md`) | Final (`deliveries/u04/final/review.md`) | Assessment (`source-adjudications/u04.md`) |
| u05 | Normal | Initial (`deliveries/u05/initial/initial-findings.md`) | Final (`deliveries/u05/final/review.md`) | Assessment (`source-adjudications/u05.md`) |
| u06 | Old skill | Initial (`deliveries/u06/initial/initial-findings.md`) | Final (`deliveries/u06/final/review.md`) | Assessment (`source-adjudications/u06.md`) |
| u07 | Old skill | Initial (`deliveries/u07/initial/initial-findings.md`) | Final (`deliveries/u07/final/review.md`) | Assessment (`source-adjudications/u07.md`) |
| u08 | Revised skill | Initial (`deliveries/u08/initial/initial-findings.md`) | Final (`deliveries/u08/final/review.md`) | Assessment (`source-adjudications/u08.md`) |
| u09 | Normal | Initial (`deliveries/u09/initial/initial-findings.md`) | Final (`deliveries/u09/final/review.md`) | Assessment (`source-adjudications/u09.md`) |

The source assessments also distinguish additional supported findings, unsupported claims,
new regressions and unproven hardening. The seven outcomes above are not the entire review scope.
