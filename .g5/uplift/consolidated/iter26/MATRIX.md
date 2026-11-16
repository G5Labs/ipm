# Selected synchronization tracking neighbors

| Stage | Active | zeta count | alpha count | Stored zeta classes |
|---|---:|---:|---:|---|
| inactive AddSkipped | 0 | 0 | 0 | none |
| Begin | 1 | 0 | 0 | none |
| zeta Z, alpha Alpha, zeta A, zeta Z | 1 | 3 | 1 | Z,A,Z at1,2,3 |
| Report in either module order | 1 | 3 | 1 | unchanged |
| End, then inactive add | 0 | 0 | 0 | none |
| Begin, then zeta Z | 1 | 1 | 0 | Z at1 |

Report module sections follow supplied order; skipped classes follow stored numeric order and retain duplicates. The store is job-local and ends/reset clears it. In the ordinary directory caller, named sync skips a verify-only class without aggregate summary; namespace sync reports it and clears tracking. Actual class Marker values advance1→2→3 while the test method counter stays0. These runtime effects are native qualifications distinct from the stored-row model.
