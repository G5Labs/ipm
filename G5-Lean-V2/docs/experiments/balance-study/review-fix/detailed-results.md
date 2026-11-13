# Independent review and repair comparison

2026-09-28. Supervisor record; never a worker input.
The frozen [protocol](protocol.md) compares three fresh GPT-6 Sol/xhigh reviewers per
condition: normal review with ontology, the full old review skill, and the full revised
review skill. GPT-6 Astra/xhigh supervises and independently evaluates the deliveries.

**This compares review procedures when every worker has ontology; it has no code-only
condition.** Results differed by defect, and no procedure consistently repaired every
obligation. Every delivery retained consequential defects. The revised runs all preserved the accepted model; the old-skill
runs all enforced the tested job cap. One run in each skill condition produced an unsupported
restriction that rejected valid successful-command reuse. These are observed differences
in a small pilot, not proof that either instruction version caused them.

The selected next experiment compares usual rule retrieval, direct governing references
and included block bodies. It addresses the difference between failing to retrieve a rule
and failing to apply one already read. Next experiment, not yet run (`NEXT-EXPERIMENT.md`).
No experimental application repair or further harness change was promoted to the maintained
system. All nine deliveries and source adjudications are complete; private services are stopped.

## What was compared

Nine fresh workers each reviewed and repaired a separate copy of the same initial retry
implementation. All received the same source, approved plan and corrected ontology.

| Condition | Shared inputs | Review procedure |
|---|---|---|
| Ontology + normal review | Code, plan and ontology | Normal review and repair; explicitly told to consult governing ontology |
| Ontology + old full skill | Code, plan and ontology | Follow the preserved old review skill |
| Ontology + revised full skill | Code, plan and ontology | Follow the preserved revised review skill |

Each condition had three independent workers. Each saved findings before repairs; Astra
then checked the final code independently. The comparison tests what the review skill
adds when ontology is already available, not whether ontology beats code-only review.

## What the final repairs actually achieved

Each fraction counts independently verified repairs of that row's requirement among the
three attempts in that condition. For example, job-cap enforcement at 3/3 means all three
workers repaired it; 0/3 means none did. These are observed repair counts for this task,
not an overall quality score or a prediction of success on other tasks.

| Outcome | Ontology + normal review | Ontology + old full skill | Ontology + revised full skill |
|---|---:|---:|---:|
| Accepted model survives an ordinary configuration change; current account checks retained | 1/3 | 0/3 | 3/3 |
| New work permitted during a valid future shutdown warning | 0/3 | 0/3 | 0/3 |
| Job cap enforced in sequential reservation check | 0/3 | 3/3 | 0/3 |
| TaskService owns task and initial-input allocation | 2/3 | 3/3 | 2/3 |
| Saved browser request replays without a new preview | 3/3 | 3/3 | 3/3 |
| Stopped-workspace parent still exposes that recovery | 2/3 | 3/3 | 1/3 |
| Synchronization progress describes the selected retry subset | 0/3 | 0/3 | 0/3 |

The launch runtime check mainly changes the model field; source tracing checks retention
of current account authority. The cap result is not a race-safety certificate. In u02,
the source adjudicator identified a possible stale-count interleaving that was not executed.
Allocation ownership is a source-level architectural finding, distinct from atomicity.
Stopped-parent closure is traced through actual UI callers, separately from the component
test. One normal run, u05, restored synchronization context but copied all source targets,
so its progress display can include work outside the retry. That is a partial repair,
not complete closure and not evidence of executing the extra work.

### Initial findings versus later discoveries

All nine initial checkpoints have empty production patches. Their saved reports are
unchanged. This table lists the previously known target groups the workers identified;
additional findings are discussed below and in individual adjudications.

| Run / condition | In the preserved initial findings | Added during repair/review |
|---|---|---|
| u01 / normal | Preview-dependent recovery | No additional known target |
| u02 / old | Job cap, input ownership, preview recovery | Stopped-parent recovery path |
| u03 / revised | Preview recovery | Accepted-launch drift |
| u04 / revised | Input ownership, preview recovery | Accepted-launch drift, stopped parent |
| u05 / normal | Input ownership, preview recovery | Accepted-launch drift, stopped parent, sync context |
| u06 / old | Input ownership | Job cap, preview and stopped-parent recovery |
| u07 / old | Accepted-launch drift, job cap, preview recovery | Input ownership, stopped parent |
| u08 / revised | Input ownership, accepted-launch drift, preview recovery | No additional known target |
| u09 / normal | Stopped-parent recovery | Input ownership, preview recovery |

Finding an issue did not ensure its repair. u07 identified launch drift, then compared
freshly resolved defaults with the accepted choice and permanently failed the queued task
if they differed. That avoids running a different model, but does not honor a still-valid
accepted model after an ordinary configuration change. Its appended plan amendment allows
refusal; the original approved freeze commitment still governs this comparison.

### New findings, incorrect claims and regressions

The review produced useful findings beyond the known list. Read-authorized receipt recovery
was repaired in several runs; u06 also recovered an original bound input after an ordinary
later task shares the chat session. u04 repaired a workspace lock-upgrade deadlock and
Unicode key-length validation. u09 found a mixed-prerequisite case: a blocked child can
record the other failed prerequisite as its cause, so the initial traversal silently drops
it instead of identifying the blocking dependency. Permission-policy validation before
allocation was another supported improvement. These results are separated from broader
security claims, schema hardening and test-build repairs in the source reports.

- **Two executed regressions:** u03 (revised) and u07 (old) ban command work anywhere in
  the source or successful prerequisite history. Both then reject an already successful
  bootstrap/command prerequisite or unrelated successful sibling, although neither would
  be retried. The same independent control passes on baseline and the other seven runs.
  Count this as one unsupported restriction per delivery, not a bug per test assertion.
- **Other source-supported repair problems:** u03 removes the fresh-request escape after
  a definitive rejection and reload; u05 projects the original job's whole sync scope onto
  a subset retry. These were source-adjudicated, not independently executed browser cases.
- **Corrections were retained:** u05 withdrew an overbroad missing-credential allegation;
  existing strict resolution already requires ordinary credentials. u08 acknowledged that
  its malformed template-pair example is blocked by a database constraint. u09 correctly
  withdrew its suspicion that a failed task must have an outcome payload; normal lifecycle
  paths permit a null outcome. Those corrections do not erase the initial records.
- **Hardening is not automatically a demonstrated bug:** composite lineage constraints,
  tool-list validation, attachment restrictions and several u06 source-history claims had
  narrower evidence or unestablished production reachability. The reports preserve those
  distinctions. u06's required fingerprint also creates a conditional upgrade-compatibility
  issue for baseline-format saved requests; this feature is experimental, and no deployed
  baseline receipts were established.

See the adjudication ledger (`adjudication-ledger.json`) and source reports
u01 (`source-adjudications/u01.md`), u02 (`source-adjudications/u02.md`),
u03 (`source-adjudications/u03.md`), u04 (`source-adjudications/u04.md`),
u05 (`source-adjudications/u05.md`), u06 (`source-adjudications/u06.md`),
u07 (`source-adjudications/u07.md`), u08 (`source-adjudications/u08.md`),
u09 (`source-adjudications/u09.md`) for claim-level evidence and uncertainty.

### Independent executed outcomes

The following counts use the original twelve outcomes with the documented request-shape
normalization where needed; the separate command control is not included in the denominator.
The unmodified raw runs are retained in test-results.json (`test-results.json`). Equal
totals can hide different repairs: an old-skill delivery can pass 10/12 by repairing the
cap while a revised-skill delivery passes 10/12 by repairing accepted-model preservation.

| Delivery | Original twelve outcomes passed | Remaining failures | Successful-command control |
|---|---:|---|---|
| Untouched baseline | 9/12 | Launch, nudge, cap | Pass |
| u01 / normal | 9/12 | Launch, nudge, cap | Pass |
| u02 / old | 10/12 | Launch, nudge | Pass |
| u03 / revised | 10/12 | Nudge, cap | **Fail** |
| u04 / revised | 10/12 | Nudge, cap | Pass |
| u05 / normal | 10/12 | Nudge, cap | Pass |
| u06 / old | 10/12 | Launch, nudge | Pass |
| u07 / old | 10/12 | Launch, nudge | **Fail** |
| u08 / revised | 10/12 | Nudge, cap | Pass |
| u09 / normal | 9/12 | Launch, nudge, cap | Pass |

Baseline and all nine final deliveries pass the independent TypeScript build check. The
normalized recovery component test fails on baseline and passes on every final delivery;
the three unresolved parent visibility cases show why that does not prove the whole UI.
For u06, the unchanged raw request suite reports 1/12 and its raw UI mock fails because
both omit its new fingerprint. The normalized suite reaches the intended assertions;
those raw failures are neither deleted nor counted as eleven product defects.

## Retrieval and effort

All nine public traces contain the input-allocation, job-cap and accepted-launch passages
before the initial findings, but workers apply them unevenly. None contains the exact
future-nudge passage. Exact-text visibility is evidence of exposure, not attention or
understanding; absence can reflect truncation or equivalent wording. The
retrieval audit (`retrieval-observations.json`) records every matching call separately.
This supports investigating both retrieval and application, without claiming either is
the sole cause or that ontology distracted reviewers from code.

| Condition | Median worker elapsed | Range | Total output tokens |
|---|---:|---:|---:|
| Ontology + normal review | 11m46s | 9m33s–19m24s | 104,568 |
| Ontology + old full skill | 20m28s | 14m45s–21m31s | 171,650 |
| Ontology + revised full skill | 17m00s | 13m59s–18m16s | 147,695 |

The nine Sol workers used **423,913 reported output tokens** and **2h26m41s of summed
session time**. Parallel sessions make that sum longer than elapsed study wall time.
Initial review checkpoints occurred after 1m54s–3m44s; subsequent intervals include
repairs, later discoveries and verification. Exact per-worker phase times, cumulative
input/cache/output/reasoning categories and Astra costs are in
effort-summary.json (`effort-summary.json`) and effort.json (`effort.json`).
Output includes the reported reasoning category; do not add it again. Input totals repeat
context and include cache reads. These are observed effort measures, not dollar costs,
matched-budget quality comparisons or general speed estimates.

| Astra work | Summed recorded turn time | Reported output tokens |
|---|---:|---:|
| Protocol audit | 2m50s | 3,316 |
| Nine source adjudications | 36m39s | 94,985 |
| Root preparation, checking and synthesis (snapshot) | 77m16s | 120,995 |

Astra total at 2026-09-28T18:44:16.131400+00:00: **219,296 reported output tokens**. These session times overlap and must not be read as additional elapsed study wall time.

Astra preparation, protocol audit, source evaluation and synthesis are accounted separately
from Sol. Root time includes infrastructure, checks and waiting. A usage limit interrupted
supervision after all nine workers finished; the user resumed it. Turn-interval accounting
excludes that roughly 31-minute gap while preserving raw session spans and the brief u06
adjudication follow-up. Interruption record (`interruptions.json`). Root usage is a snapshot
before the final reply; the report does not present supervision as free.

## What was held fixed

Every worker receives the initial retry implementation `844536fe77da51dda540bd7a9c2fba3e5b539069`,
the original approved plan, original change patch, corrected ontology body text, ordinary
source/docs, and equivalent private PostgreSQL, Redis, build and frontend environments.
All may repair and verify. The nine packets share 2,450 byte-identical common files.
Prior findings, author explanation, repaired implementation, evaluator tests and study
history are excluded. Each repository has a fresh root commit, no inherited history and
no remote. Initial reports are frozen before production repair; later discoveries remain
distinguishable from those initial reports.

The old harness is `ca6a58282268b4ed6baa5749ec81ee6d75d32b50`; the revised harness is
`9eff15f18dc924e01b21a896523a40f6e939bb20`. Exact skills are retained in harness (`harness/`).
Only the review skill differs between the two skill conditions. Planning revisions are
preserved but not assigned. This tests incremental review guidance over shared ontology
knowledge, not the full design-to-code workflow or ontology versus no ontology.

Old ontology metadata leaked specific prior findings. Its detail was uniformly withheld
from every packet, preserving governing bodies, stable IDs and author categories. This
also removes helpful old source locations; it is a deliberate sanitized experiment, not
ordinary review of the complete repository. See input audit (`input-audit.json`),
redactions (`ontology-redactions.json`), manifest (`manifest.json`) and
baseline preservation (`baseline-preservation.json`).

Three counterbalanced waves use normal/old/revised, revised/normal/old, old/revised/normal.
This is not random allocation. u09 started a few minutes after its wave peers while an
Astra evaluator released an agent slot. No worker received another worker's findings or
evaluator feedback, and no arbitrary short time cutoff was imposed.

## How evidence is separated

The original twelve API/database/dispatch functions run unchanged on baseline and final
code. Source review separately checks ownership, UI parent visibility, synchronization
projection and new claims. An additional component test covers a lost submission response,
reload and unavailable fresh preview; an additional allowed-behavior test preserves a
successful command prerequisite or sibling without retrying it. Tests run in a separate
evaluator checkout from pinned final commits. Worker test claims are not evaluator results.

One delivery added a required preview fingerprint. The original fixtures omit it and fail
before reaching their intended assertions. Those raw failures remain recorded. A separate,
uniform adapter supplies the public preview's fingerprint without changing assertions;
the normalized suite contains the original twelve plus the command control. See
[evaluator corrections](evaluator-corrections.md) and normalization freeze (`compatible-freeze.json`).
The distinction prevents a request-shape mismatch from becoming eleven alleged defects.

Each source adjudicator receives only an ID, the candidate code, original plan and original
governing bodies, frozen initial findings and final reports. The condition mapping and
skill files are withheld. Reports can reveal workflow/style clues, so this is partial
de-identification, not complete blinding. The preparing supervisor knows all conditions
and prior target defects. Original contracts govern; a worker cannot retroactively justify
an invented restriction by adding it to ontology.

## Limits

Three runs per condition on one already studied feature support a pilot comparison, not
a general reliability rate or causal model-speed estimate. Added diagnostics were selected
adaptively; their selection and corrections are preserved. Shared host load and differing
repair scope affect elapsed time. Filesystem separation is enforced by directory ownership
and instructions, not sandbox permissions; ambient tool/skill descriptions and parent cwd
remain available. Public access screens cannot prove no hidden or indirect leakage.

The API tests use synthetic terminal history and VM rows. No real guest/provider execution
or full browser journey is established. The launch test mainly changes a model field;
the cap test is sequential after a confirmed start, not a concurrency stress proof. Some
negative tests accept any client error and do not enumerate every possibly leaked row.
Source-supported races or regressions are labeled separately from reproduced failures.

## Records

- Frozen setup (`freeze.json`), evaluation freeze (`evaluation-freeze.json`),
  protocol audit notes (`evaluator-notes.md`), dispatch records (`dispatch-wave-3.json`).
- Initial/final deliveries (`deliveries/`), independent source adjudications (`source-adjudications/`),
  test outcomes (`test-results.json`), concise execution evidence (`test-evidence/`).
- Effort (`effort.json`), rule retrieval (`retrieval-observations.json`),
  preservation audit (`preservation-audit.json`), visible access audit (`visible-access-audit.json`),
  final integrity/service audit (`final-verification.json`).
- Local full public histories and runtime receipts: `../../runs/2026-09-28-review-fix/`.
  Private reasoning is excluded from exports. Local paths and service dependencies must
  be adjusted to reproduce elsewhere; evaluation and delivery scripts deliberately refuse overwriting receipts; derived summaries
  can be refreshed from retained evidence.

Experimental application and ontology repairs remain isolated. The maintained application,
ontology and harness receive no changes from this comparison.
