# Follow-up review of the IPM Verus uplift: handoff

This is the handoff of the follow-up review of the original IPM Verus uplift (`ae61979578ecbc23d47b3af44c076c105663f38c`, product `571e04a1`). The review was driven by the operator audit's twelve findings R01–R12. It is separately attributable: the original submission, its receipts and its historical notes under `.g5/uplift/ipm-verus-20261113/` and `.g5/Verus_verification/` are unchanged, except the two owner files the request replaced (`bugs_issues.md`, `design_simplification.md`). Their originals are in git at `ae619795`.

STATUS: complete. All twelve audit findings are closed with evidence, with the residuals listed below. Two independent reviews ran, and their material findings were repaired and rerun on the final inputs. Thirty maintainer decisions remain open (`open_questions.md`). The repairs made after the two reviews were validated by reruns and consistency checks, not by a third reviewer.

## Where to start

| Reader | File |
|---|---|
| Maintainer: product defects and issues | `.g5/Verus_verification/bugs_issues.md` |
| Maintainer: decisions requested | `.g5/Verus_verification/open_questions.md` |
| Maintainer: design proposals | `.g5/Verus_verification/design_simplification.md` |
| Verifier: how to rerun everything | `.g5/Verus_verification/followup/README.md` |
| Auditor: what each finding needed and got | `closure.md` (ledger), `audit-validation.md` (each audit claim checked against the sources), this file |
| Reviewer record | `review/` |

```
 audit R01–R12 ──▶ closure.md (ledger, committed first: 8cf8c8af)
        │
        ├─▶ audit-validation.md  each claim confirmed / narrowed against the original sources
        │
        ├─▶ Verus_verification/followup/
        │      replay/   corrected evaluator, controls, re-evaluation      (R01–R03)
        │      models/   history readers, load/transaction modes, prefix  (R04–R06)
        │      native/   ordinary native cases in the dedicated container (R04, R05, R06e)
        │      PROJECTIONS.md                                             (R07)
        ├─▶ scope-ledger.md                                               (R08)
        ├─▶ bugs_issues.md, design_simplification.md, open_questions.md   (R09–R11)
        └─▶ review/ (independent review), this handoff                    (R12)
```

## What changed about the original conclusions

- The native replay verdicts stand. With the corrected evaluator, the 59 final-run cases keep their verdicts (57 exact, 2 approximate), and every case's failed clauses equal the model's own violated clauses. Re-executing the 45 return-path cases with the committed driver reproduced every observed field. The original evaluator's weaknesses (R01–R03) were real but latent: no observation IPM produced hit them. One would have mis-scored a future native check of the D1 repair (R02a).
- Three superseded-run cases (B1-C19, C20, C40 in attempt 1) become INDETERMINATE. The original reported two of them as Committed violations on missing evidence.
- The round-three repairs hold on the final inputs: 263 Verus obligations, 0 errors on identical proof inputs. Controls 01, 12 and 13 are detected, and repair 103 passes in the transactional modes. The handoff's statement that the round-three reviewer reproduced "263/0" was wrong: that reviewer reproduced 260/0 before the repairs.
- The D1 repair proposal (variant 103) needs three corrections:
  - It meets the dependency clauses only when every activation uses the load's mode. A dependency that declares its own NoJournal breaks it.
  - Its top-level close must not write dependency rows while the load's transaction is open. Premise P-14, observed natively, makes a Terminate in that window leave `Committed=1` on a rolled-back dependency.
  - It keeps the original handoff's SIGKILL residual for loads without a transaction.
- The original contract's "any state" after a process end is replaced in the follow-up model by an effect-prefix clause. As built, the product meets it at every cut point.
- New product findings, from source, models and 21 new native cases:
  - D21: a failed load can leave a module installed without its dependency.
  - D22: the top record reads "nothing kept" while a dependency's changes were kept.
  - D28: Q-23 observed natively for the first time.
  - I3 and D31: developer-mode dependencies are replaced silently.
  - D23: a global delete hides every error.
  - I4: global readers carry no namespace or ID.
  - D24, D29, D30, I5–I8, and others filed for the first time.

## Counts

| Kind | Count | Where |
|---|---|---|
| Formal proofs (Verus) | 263 obligations, 0 errors (original crate, unchanged, reverified). No new Verus proof | `Verus_verification/followup/results/20261113T232529Z-r12-asbuilt-proofs` |
| Original controls rerun | Verus 01, 12, 13 and sanity detected; explorer controls 01, 12, 13 detected; as-built passes and violations as expected (19 runs) | `followup/results/20261113T2325*`–`2330*` |
| Evaluator controls | 293 (decoder 175, differential against the model 59, paired L1 45, evidence 14), all pass | `followup/results/20261114T002645Z-final-replay-controls` |
| Model checks | load model: 1,692 configurations, 62,764 end states as built, 9 variants, 12 correspondence rows. Mutants: 3 detected, 1 equivalent and refuted natively. History model: 524 commands per variant, valid neighbor 69/69, 2 wrong variants refuted. Prefix cases: 6/6 | `followup/results/20261114T004842Z-final-loadtx-model`, `20261114T002647Z-final-history-readers-model`, `20261114T005359Z-final-prefix-cases`, `20261114T005807Z-final-committed-breakdown` |
| Exact native cases | original 57 exact and 2 approximate (unchanged). Follow-up: 21 new, of which R05 10/10 and R04 7/7 conform to predictions written before the runs. N-P21: 2 conform and 2 refute the before-image prediction, which establishes P-14. 45 re-executed, all identical | `followup/results/native/` |
| Questions | 30 open (22 from the ontology, 8 new) and 1 resolved (Q-12). Types: 11 policy disagreements, 18 design choices, 1 missing test evidence | `Verus_verification/open_questions.md` |
| Product findings | 54 entries: 14 supported, 13 conditional, 7 issues without a governing requirement, 13 suspected, 3 withdrawn or refuted, 4 out of scope | `Verus_verification/bugs_issues.md` |
| Design proposals | 10 (S1–S10), each with owner, old complexity, preserved behavior, tradeoffs, decisive pair, choices and evidence. 4 have model evidence | `Verus_verification/design_simplification.md` |
| Scope rows | 313, of which 93 in-scope rows rest on static review only or were not run | `scope-ledger.md` |

There is no product-wide coverage percentage. Denominators are the declared inventories above. "All listed items covered" does not mean every requirement was discovered.

## Coverage and correctness, with bounds

| Area | What is established | Bounds and limits |
|---|---|---|
| Replay correspondence | per-clause agreement between the model's verdict and the product's observation for 59 cases | single-class fixtures, filesystem dependency sources, at most two dependencies, one fault |
| Load with per-activation modes, nesting, process ends | PREFIX, TLEVEL and RETRY hold as built under the product's policy. Committed and record survival fail exactly where the findings say | 0–2 dependencies (nested and shared graphs reduce to this, confirmed natively for depth 2 and a diamond), one Activate fault, caller transaction or not, every cut point with Terminate or SIGKILL. Not modeled: other installed versions (native only), manifest refusals (original crate), two faults, concurrency, restart |
| History consumers | selection, deletion, purge selection, global readers and details against a contract with affected and preserved sets | 4 namespaces, 6 rows, filters of up to two keys. Global readers native only from the archived H10 run (no second IPM namespace in the dedicated container) |
| Premises | P-14 (new, native) and P-06c (applied to JournalManagers, matches N-P19) | P-14 is inferred from four observations, not from IRIS documentation. P-12 (restart) not run |
| Effects outside the database, registry sources, concurrency, liveness | not established | `Verus_verification/followup/PROJECTIONS.md` |

## Per audit finding

| Finding | Disposition | Evidence | Residual |
|---|---|---|---|
| R01 | closed | corrected evaluator `followup/replay/` and `PROJECTION.md`; controls; re-evaluation `followup/results/20261114T002646Z-final-replay-reeval/delta.md` | write count stays a proxy (`LastUpdated`), resources a one-class projection |
| R02 | closed | per-slot `kept`, per-clause comparison, contrast controls | R02a and R02b latent in the corpus |
| R03 | closed | schedule bound to the script hash, markers, conflicts, staleness, provenance, snapshot hash; 45/45 re-execution | T/K and C49 driver revisions unrecoverable (process termination not re-run) |
| R04 | closed | `followup/models/history_readers.py`, native R04 | global readers natively only from H10 |
| R05 | closed except R05d | `followup/models/loadtx.py`, native R05 | remote and ORAS dependency sources not run |
| R06 | closed except R06f | PREFIX clause, prefix cases, P-14, Terminate window, guarded and global variants | C49T/K stay APPROXIMATE |
| R07 | closed | `followup/PROJECTIONS.md` | concurrency, restart, files, registry, liveness open |
| R08 | closed | `scope-ledger.md` | 93 in-scope rows static or not run |
| R09 | closed | `Verus_verification/bugs_issues.md` | suspected entries need ordinary runs |
| R10 | closed | `Verus_verification/design_simplification.md` | S1, S5, S6, S7, S10 not modeled |
| R11 | closed | `Verus_verification/open_questions.md` | 30 decisions for the maintainers |
| R12 | closed | reruns, trace, two reviews (`review/README.md`), this file | post-review repairs not re-reviewed by a third reviewer |

Each claim the audit made was checked against the original sources first (`audit-validation.md`). All were confirmed. Three were narrowed:

- R02b: the qualified-row handling matched Verus, and the gaps were missing-data handling and the lost question identity.
- R03d: the original did not rely on the return code alone.
- The added suite-headline check: eight of nine suite invocations passed. DependencyResolution ran no effective test.

## Decisions for the maintainers, most consequential first

1. Q-22 and Q-23, with Q-25 and Q-26: may a dependency's own `<Defaults>` NoJournal escape its parent's rollback, and does its NoTransaction suppress the parent's transaction? These decide D14, D21, D22, D28 and the shape of the D1 repair. S8 makes them one load-wide rule.
2. Q-30: the D1 repair design (per-dependency modes, guarded close, the SIGKILL residual).
3. Q-05 and Q-06b: may history survive a caller's own rollback (D25)?
4. Q-17: system requirements before any write (D4).
5. Q-08 and Q-21: what an unfinished record says (D26, D27).
6. Q-14, Q-18, Q-10, Q-27, Q-28, Q-31, Q-24, then the purge questions Q-13, Q-20, Q-29, then the documentation-only questions. The full ordering and each decision's options are in `open_questions.md`.

## Not done, and why

- No new native process termination, history widening delete or purge deletion was run, because these outcomes are destructive. Their selection was observed read-only and their effect modeled, and the archived observations stand.
- The original Verus contract still contains `any_alt` for process ends. The follow-up's PREFIX clause is in the Python model. Porting it into `contract.rs` would change the original's formal inputs and needs a new proof run, which is left as an obligation for a future change.
- The ontology was not edited. Q-24–Q-31 have no ontology block, and the historical statuses that lag are listed in `open_questions.md`.
- One source annotation in `followup/native/make_sessions.py` cites `Utils/Module.cls:1323` for the call that is at `:1328`. It is the text of expectations that were already run, so it was left as recorded.
- A second IPM namespace was not created for global-reader cases, because that needs a second IPM install.

## Process record

- The closure ledger was committed first (`8cf8c8af`). Expectations for every follow-up native run were committed before the run (`104fa5e2`, `be6be6f9`) and snapshotted into each run's `inputs/`.
- Native runs used only `IRIS_CONTAINER`, `IRIS_NETWORK` and `IPM_UPLIFT_WORK`. The runner has no defaults, labels its container and refuses others. The container and network were removed at the end. No other workload's container, network or image was touched. The original product image was used read-only.
- Errors found during the work and kept on record:
  - two control expectations of my own were wrong and corrected before the first receipt;
  - three model bugs (trace bookkeeping, L1 durability with a caller transaction, the repair's Terminate close) were found by analysis before any cited receipt;
  - one native driver syntax error (`20261113T234651Z-p21-ordering`, superseded by `-r2`, expectations unchanged);
  - stale counts in `MODELS.md`;
  - the P-06c close order and the finalized-only Committed check, both found by the independent reviews.

  Superseded receipts are kept and marked in `Verus_verification/followup/results/index.jsonl`.
- Delegation: an ID registry, the scope ledger and drafts of `bugs_issues.md` and `open_questions.md` were written by separate agents from briefs in `work/` and reviewed before acceptance. The two independent reviews were fresh agents that did not write any reviewed artifact.
