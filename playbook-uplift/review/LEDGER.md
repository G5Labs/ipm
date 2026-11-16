# Review ledger

Every finding of the two independent reviews, with its fix and where to check it. Status is the orchestrator's claim;
the next reviewer verifies it.

## First review (review/REVIEW.md, REFUSE)

| # | Finding | Fix | Check |
|---|---|---|---|
| 1 | L1s one-directional on Success | Outcome rules both ways in load_general (`worked`, crash never ok, dependency rows), update_steps (`fault`), dep_resolve (failure reason), uninstall, settings, sync; reviewer's mutants added as controls | controls/*.json (`ok-recorded-as-failure`, `crash-records-success`, …); explorer rule L1-SUCCESS-EXACT. Residual: update_steps' `fault` is self-set, so INST-B80 has no P (REVIEW-2 N2) |
| 2 | 3.3a coverage not honest; missing writers | Gate rejects failed runs unless labelled; verdicts must match evidence kinds; re-measured successful repo create/modify/delete, update with steps, `-force` uninstall, web-app and FileCopy uninstalls, web-app load; UpdateSteps class and INV-STEPS; INV-MOD/INV-DEP dev/no-transaction verdicts are named exceptions | scripts/semantics_gate.py, semantics/coverage.json, COVERAGE.md (135 pairs) |
| 3 | P on contradicted statements | Gate forbids P with F; rows fixed | scripts/ontology_index.py; self-test `p-with-f` |
| 4 | Witness P on behaviour rows | HIST-N1..N3 rewritten as intent (B, F/OPEN, O!); INST-B67, DEP-B13 rewritten; witnesses cited in findings | ontology/02, 03, 04 |
| 5 | P citations not stating the statement; premises missing | Gate requires each lemma premise (incl. via wrappers) named in Notes; LOAD-B10/B16/B17, HIST-B1/B9 lost P | `lemma_premises` in the gate; self-test `premise-not-named` |
| 6 | Compiled code claimed proved | LOAD-B8 split; LOAD-B30 (compiled code) observed only | ontology/01 |
| 7 | Stored developer mode | Modelled in update_steps (dev_eff); IRIS check nc-inst-fix-stored-dev-* | models/verus/update_steps.rs |
| 8 | Evidence quality | O! refuting tag; T records carry skipped_assertions; cited classes rerun (0 skips except Scopes 1; Migration unknown); N20 and NSP-B3 re-captured; over-claiming T rows narrowed | evidence/ipm-tests.json; INDEX "T tags on test runs with skipped assertions" |
| 9 | Spec vs intent | HIST-B11..B13 scoped to load/install/update, HIST-B31 uninstall; REPO-B59 restated with F-REPO-3; LOAD-B20 OPEN; named exclusions in ontology/OUT-OF-SCOPE.md | — |
| 10 | Post-commit failure | Premise PM; two native attempts did not trigger it | load_general.TRUST.md G7; O:ipm-post-commit-message-fails |
| 11 | Other L2 omissions | Recorded | load_general.TRUST.md G8 |
| 12 | Update step fails by throwing | INST-D4, INST-B64 | ontology/03 |
| 13 | Loose control locations | Every expect list narrowed; locations matched on the primary error line | models/verus/controls.py |
| 14 | Decisions missing from cells; missing F tags | Gate requires a cell to name the decision its outcome waits on; F tags added | gate rule; ontology cells |
| 15 | Missing TRUST files | One per proof file | models/verus/*.TRUST.md |
| 16 | STATUS stale | Regenerated from the full rerun | STATUS.md |

## Second review (review/REVIEW-2.md, REFUSE)

| # | Finding | Fix | Check |
|---|---|---|---|
| N1 | Write-set probe missed IRISSECURITY | Probe scans IRISSECURITY, IRISLOCALDATA, IRISAUDIT; all commands re-measured; web-app load measured; `^SECURITY` classified as instance configuration | validation/logs/57-*, 58-*; O:ws-load-webapp, O:ws-uninstall-webapp |
| N2 | INST-B80 rests on a self-set ghost | P dropped from INST-B80 with the reason in Notes | ontology/03 INST-B80 |
| N3 | Verdicts without matching evidence | Gate enforces verdict vocabulary and evidence kinds; `S:` statement references; entries corrected (observation-only claims now "observed"; `-force` is a named exception citing UNIN-B35; INV-STEPS states the rerun after a failed update) | scripts/semantics_gate.py `VERDICT_NEEDS` |
| N4 | RES-B11 contradicted by the scopes model | Generated resources compile but are never loaded or exported; two new controls | models/verus/scopes.rs; controls/scopes.json |
| N5 | Premise detection gaps; label collisions; no self-tests | Transitive closure over spec fn bodies; ambiguous labels require the file name; two new self-test cases; self-test now compares against the clean baseline | gate |
| N6 | T skips per class / unknown | Every cited class known except Test.PM.Migration (needs legacy ZPM) | INDEX |
| N7 | P with OPEN; cells on OPEN rows | Gate requires "intent open" in Notes; cells name the decision | gate |
| N8 | Retired load_history cited | TRUST.md and Notes updated | — |
| N9 | dep_resolve, scopes TRUST | Added | models/verus/dep_resolve.TRUST.md, scopes.TRUST.md |
| N10 | Unconstrained L1 fields | Recorded (no P relies on them) | load_general.TRUST.md G10 |
| N11 | PM not named | Named in Notes of every row citing g_refines | ontology/01, 02 |
| N12 | KeepOnUninstall for clean excluded | UNIN-B82 (inspection) | ontology/05 |
| — | HIST-B25, LOAD-B15 over-claims | New IRIS observations | O:ipm-history-delete-needs-confirm, O:ipm-load-keeps-caller-work |
