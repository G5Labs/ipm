# Trust inventory

Everything a "proved" or "model-checked" claim in this experiment rests on without itself being proved.
No `assume`, `admit` or `external_body` appears in any `models/verus/*.rs` (enforced by scripts/record_proofs.py). The bounded `load_history.rs` is retired to `models/verus/superseded/`; rows T1/T4/T8 below describe its successor `load_general.rs` (T13).

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| T1 | The Verus L2 (originally `load_history.rs`, now `models/verus/load_general.rs`, see T13) transcribes `LoadNewModule`, `LoadModuleReference`, `Main.Load` and `HistoryTemp` faithfully | Written by hand from the source (line anchors in comments); ObjectScript has no verifier | Folded-vs-fine abstraction check (240 configurations, 0 mismatches); the fine model conforms to 25 IPM scenarios on IRIS |
| T2 | The fine L2 (`models/explorer/l2_load.py`) transcribes the same code | Hand-written | Trace conformance on 25 scenarios; 8 code mutants each contradicted by IRIS scenarios or L1 rules |
| T3 | The IRIS environment contract (`models/explorer/irisenv.py`) | Hand-written from IRIS behaviour | 9 differential probes on IRIS 2026.1, 0 mismatches; 9 mutants each killed |
| T4 | The folding of a dependency's nested transaction into one Verus step, and at most one open parent level | Simplification for proof cost | T1's abstraction check; probe env-nested-commit-outer-rollback |
| T5 | Premise P1 (start at `$TLEVEL` 0) | Not enforced by IPM | O:ipm-load-inside-caller-transaction-rolled-back shows the failure mode; DEC-HIST-4 |
| T6 | Premise P2 (a crash rolls back) | True for jobs and default terminal answers only | O:ipm-halt-txn (rollback), O:ipm-halt-txn-operator-commits (commit); DEC-LOAD-1 |
| T7 | Premise P3 (no crash between outer TCOMMIT and SetCommitted) | Two adjacent statements; not injectable from outside | Explorer (F-HIST-7); no native check |
| T8 | One module plus at most one dependency, modules absent beforehand (Verus); one dependency level, pre-state absent or newer (explorer) | Bounds | Lifted in round 3 for the proof layer by T13 (unbounded dependencies, any prior state); the cross-check and explorer remain bounded (0-3 dependencies); chains and upgrades observed natively (nc-gap-P9-*) |
| T9 | The scenario harness (`validation/iris/Uplift/Probe/*.cls`, fixtures) reads state correctly | Probe code | Each probe's raw log is kept; scenario expectations were not written in advance, the model was compared after |
| T10 | Uplift probe classes `Uplift.Probe.Rec/TempRec` mirror `History/HistoryTemp` storage and trigger | Same storage pattern | The same behaviours were then observed on IPM's real classes (ipm-finalized-history-row, crash scenarios) |
| T11 | Ontology evidence of modules 03–09 | Written by three builder agents by inspection | Gate checks tags are well-formed; X anchors were checked by the builders to be assertion lines; the orchestrator re-read four findings' code |
| T12 | The readability gate's syllable count | Heuristic Flesch–Kincaid (overcounts some compounds, e.g. "namespace" = 3) | Conservative: overcounting raises the grade |

## Added in round 3 (closing gaps, expanding the proof boundary)

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| T13 | `models/verus/load_general.rs` (generalized L2: any number of dependencies, any prior state, caller transactions, process and instance crashes, both crash resolutions, the commit window, the proposed fixes) transcribes the same code as T1 | Hand-written | `models/explorer/fold_check_general.py`: literal Python transcription vs the fine L2 on 35,892 configurations, 0 mismatches (it found and fixed two divergences: an operator commit keeps a crashed dependency's write; the first fix design was dishonest) |
| T14 | Instance-crash rule of the IRIS contract (no %OnClose, IRISTEMP wiped, startup rollback) | From IRIS behaviour | O:nc-gap-P1-* (docker restart and `iris force`, three modes); 4 P1 restart scenarios replayed through the fine L2 with 0 mismatches |
| T15 | Probe patches p8a (halt after the outer TCOMMIT) and p8b (forced deployed-export error) in a separate container | Needed to reach a two-statement window and an error path; inert unless their global is set | Control observations O:nc-gap-P8-commit-window-control, O:nc-gap-P8-deployed-export-control; diffs in `validation/iris/n2/patches/` |
| T16 | Probe code `Uplift.N2.*`, `N2H.Hooks` (native lane n2), `Uplift.Probe.WriteSet` | Probe code | Raw logs kept; write sets are hashes of every global in the namespace's databases, IRISSYS, IRISTEMP, IRISSECURITY, IRISLOCALDATA and IRISAUDIT (`validation/logs/57-write-sets-all.log`, `58-write-sets-webapp.log`). The first rounds missed IRISSECURITY (review REVIEW-2 N1); all 46 commands were re-measured. Not scanned: read-only library databases (IRISLIB, ENSLIB) |
| T17 | The write-set classification of globals (`semantics/semantics.json`, `global_classes`) | Hand classification | `scripts/semantics_gate.py` refuses any measured global that is not classified |
| T18 | The fix variants in `l2_load.py` (`fix-dev-persist`, `fix-dep-repersist`) stand for the proposed IPM change | They are designs, not IPM code | Proved in Verus (`g_fixed_refines_strict`), explored on the fine L2 (section G), cross-checked against the Verus fix (T13). Not implemented in IPM. |

## Added in round 4 (resolving the open decisions)

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| T19 | The 44 decision verdicts in `decisions/research/{hist,inst,repo,nsp}.md`: each lane's reading of code, git history, the IPM wiki, GitHub issues/PRs and its own IRIS runs | Written by four research agents; "evidence" verdicts cite a maintainer statement, wiki page, changelog, test or commit message; "assumption" verdicts give the alternatives | Re-checked by the orchestrator against GitHub: issue #58 comments (block and warn; unskip `TestUninstallDependencyDoesError`), commit `3174d72d` (drops `ORDER BY PublishOrder, ID`), PR #1205 body ("collects failures without throwing"; DEC-SYNC-3 downgraded to assumption because of it), issues #815, #470, #628, #1299, PR #1303, commits `d6d346728`/`e232e5056` (the removed `TestSimultaneousUpdateFails`), and the post-commit observations of F-LOAD-4. Other citations were not re-read by a second party |
| T20 | `validation/logs/SUPERSEDED.tsv`: observation lines of probe runs that their lanes declared invalid (probe bugs, confounded runs) are skipped by `scripts/collect_evidence.py` | Each line has a reason; logs are never edited | The collector prints how many lines it skipped; each listed name that has a valid rerun resolves to that rerun |
| T21 | IPM's CI at `571e04a1` (run 42293577988) as evidence for test-suite behaviour on amd64 and 8 IRIS images | Downloaded with `gh run view --log`; extract kept in `validation/ci/ci-571e04a1-extract.txt` | Counts are reproducible from the full log; the full log is not kept in the experiment (80 MB) |
