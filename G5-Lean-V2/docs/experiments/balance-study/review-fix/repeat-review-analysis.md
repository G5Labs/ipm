# Would combining the existing reviews have helped?

This is a retrospective calculation, not a new review experiment. We combined the known
issues identified by the three workers in each procedure. The independent assessments
decide whether a finding is valid; no real consolidator or combined patch was tested.

| Procedure, all with ontology | Best initial report | Union of three initial reports | Best completed report | Union of three completed reports |
|---|---:|---:|---:|---:|
| Normal review | 2 | 3 | 5 | 5 |
| Old skill | 3 | 4 | 5 | 5 |
| Revised skill | 3 | 3 | 4 | 4 |

Counts are distinct recognized issues among the seven tracked outcomes. They do not
include every additional discovery or indicate successful repair. The best individual
is chosen after seeing results; it is not a deployable selection method.

Combining initial reports adds coverage for normal and old-skill review. Combining the
completed reports does not exceed the best individual within any of the three procedures.
Every group still misses the shutdown-warning exception. Combining all nine completed
reports would identify six of the seven outcomes, but no final delivery correctly repairs
the synchronization subset display. A union of patches is not an established working fix.

This supports two competing hypotheses: independent reviews can recover complementary
findings, but the same procedure can leave shared blind spots. It does not prove that a
new set of three reviews will behave the same way. Completed reports include discoveries
during repair, time was not matched, and the calculation uses independent adjudication
that an ordinary consolidator would not receive. False findings and consolidation losses
must therefore be measured in the new experiment.

Calculation (`repeat-review-analysis.json`), source ledger (`adjudication-ledger.json`),
planned controlled comparisons (`NEXT-EXPERIMENT.md`).
