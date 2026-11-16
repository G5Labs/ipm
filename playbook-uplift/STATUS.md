# Status: IPM playbook uplift

Baseline: IPM `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (tag v0.10.11-beta.28, manifest 0.10.11-SNAPSHOT); the checkout
was not modified. Native checks: IRIS Community 2026.1.0.234.1com (build 234U, ARM64), repeated for the load/history
scenarios on 2025.3.0.226.0com (build 226U); IPM installed from the pinned source. Verus 0.2026.09.27.3cf1832.
Numbers below are from the last full rerun from clean (`scripts/full_rerun.sh`, log `validation/logs/49-full-rerun.log`,
2026-11-14T00:42Z, `fail=0`).

**Database.** IPM keeps its state in IRIS globals; there is no PostgreSQL, so the playbook's pgspec library does not
apply and is not used. Its role is played by an IRIS environment contract built here (`models/explorer/irisenv.py`,
folded into `models/verus/load_general.rs`) and validated against IRIS itself (TRUST.md T3, T14).

## Phases

| Phase | Scope | Gate result |
|---|---|---|
| 3.1 Uplift | All of IPM, modules 00–09 | Pass. 730 statements, 389 state×command cells (0 on a human decision: all 44 decisions resolved in round 4), index regenerates without edits, self-test kills its 4 injected violations. The gate also forbids P with F, requires every premise of a cited lemma to be named in Notes, and separates refuting observations (O!). Named exclusions: `ontology/OUT-OF-SCOPE.md`. |
| 3.2 Environment contract | IRIS transactions, IRISTEMP, `$INCREMENT`, `%OnClose`, process and instance crash, object re-save, UPDATE trigger, namespace mappings | Pass for the contracts used. Load/history probes plus mapping probes, 0 outcomes outside the models; every environment mutant contradicted by a probe. Not a complete IRIS model. |
| 3.3 L1/L2 and refinement | Loading and history, update steps, resolver, uninstall, settings, namespace mapping, sync baselines, resource scopes | 8 Verus files, 227 obligations, 0 errors, no assume/admit/external_body; 195 of 195 controls killed in the expected lemma. Each refinement holds under named premises; each defect premise has a proved witness or characterization. |
| 3.3a System semantics | 13 axioms, 11 mechanisms, 2 lifecycles, 11 invariants | Pass. Write sets measured on IRIS for 46 commands (`ws-*`), scanning IRISSECURITY, IRISLOCALDATA and IRISAUDIT as well; 135 (invariant, command) pairs, 0 problems, every one covered by evidence that resolves; read-only commands measured to change no IPM fact; failed runs labelled as refusals or failure runs. |
| 3.4–3.6 | Not applicable as written | IPM is existing ObjectScript, not a generated service. L3 is tied by trace conformance on real runs (playbook 4.2). |
| 3.7 Validation | All modules | 415 IRIS observations (11 lines of invalid probe runs skipped via `validation/logs/SUPERSEDED.tsv`); IPM's CI at this commit read for its test skips (`validation/ci/`); 113 IPM test classes recorded (109 pass; 4 classified failures); 245 model-check records (234 pass; the rest are deliberate counterexample checks). Full rerun from clean recorded. |
| 3.8 Account | This file, BUGS.md, DESIGN-SIMPLIFICATIONS.md, QUESTIONS.md, TRUST.md, per-model `*.TRUST.md`, TEST-REFLECTION.md | **Findings addressed, sign-off not obtained.** Both independent reviews REFUSED (review/REVIEW.md: 2 blockers, 7 major; review/REVIEW-2.md). Every finding of both is fixed and mapped in review/LEDGER.md; no third review was run (by instruction), so the fixes to the second review are not independently checked. |

## Evidence per module (from ontology/INDEX.md)

| Module | Statements | P | M | O | O! | T | F | OPEN |
|---|---|---|---|---|---|---|---|---|
| 00 System root | 18 | 0 | 0 | 2 | 0 | 0 | 0 | 0 |
| 01 Package loading | 43 | 4 | 5 | 26 | 3 | 1 | 4 | 0 |
| 02 Transaction history | 49 | 10 | 11 | 27 | 8 | 12 | 8 | 0 |
| 03 Install and update | 93 | 26 | 22 | 33 | 8 | 14 | 8 | 0 |
| 04 Dependencies | 63 | 6 | 11 | 17 | 6 | 22 | 6 | 0 |
| 05 Uninstall | 101 | 12 | 18 | 29 | 12 | 8 | 17 | 0 |
| 06 Registries and settings | 115 | 9 | 8 | 23 | 12 | 17 | 15 | 0 |
| 07 Namespace ownership | 93 | 10 | 17 | 26 | 12 | 11 | 16 | 0 |
| 08 Resources and packaging | 96 | 9 | 0 | 20 | 5 | 25 | 8 | 0 |
| 09 Developer sync | 59 | 18 | 21 | 16 | 3 | 32 | 4 | 0 |
| **Total** | **730** | **104** | **113** | **219** | **69** | **142** | **86** | **0** |

P = proved in Verus with controls killed and premises named; M = model-checked; O = observed on IRIS; O! = an IRIS
observation that refutes the statement (always with F or OPEN); T = IPM test run here; F = contradicted by the code.

## Proof files

| File | Obligations | Controls | What it proves (premises) |
|---|---|---|---|
| load_general.rs | 39 | 24 | Loading and history over any number of dependencies, any prior state, caller transactions, process and instance crashes (P1, P2, P3, PC, PI; strict: PD, PI-dep); proved fixes for DEC-HIST-1/2 remove PC, PD, PI |
| update_steps.rs | 39 | 28 | Update guard, transaction, seeding, lock, step walk; outcome both ways (PL, PF5) |
| dep_resolve.rs | 27 | 17 | Resolver soundness; failures carry a reason (PU, PR) |
| uninstall.rs | 51 | 37 | Clean/uninstall lifecycle, dependents, kept files, self-uninstall, outcome both ways |
| settings.rs | 28 | 32 | Config and default-modifier laws, registry selection |
| ns_mapping.rs | 18 | 23 | enable/unmap/installer mapping effects (SRC, UNIQ, TGT, INFO; defect premises REPOS, RTN, INST, OWN) |
| sync_baseline.rs | 17 | 24 | Sync baselines and retry marks, outcome both ways (OUT, BASE) |
| scopes.rs | 8 | 10 | Resource scopes by command; packaging inclusion (no premises) |

## Main guarantees and failures (loading and history)

| Guarantee | Level | Status | Premises |
|---|---|---|---|
| A failed or crashed normal load leaves module records and dependencies as before | P, M, O | Holds | P1, P2, P3 |
| Compiled code of a failed normal load is undone | O | Holds (observed only) | — |
| Success is recorded exactly when the command worked; never after a crash | P, M | Holds | P1, P2, P3, PC, PI |
| Committed = 0 means nothing kept (load, install, update) | P, M, O | Fails for a developer-mode crash while needs load (F-HIST-2) and after an IRIS restart in developer or no-transaction mode (F-HIST-8); holds otherwise | PC, PI |
| A dependency's record survives the parent's rollback | P (witness), M, O | Fails (F-HIST-1); the proved fix keeps it, except across an IRIS restart | PD |
| History follows a caller's transaction (intended, DEC-HIST-4) | P (characterization), O | Holds as intended; undocumented (F-HIST-3) | P1 |
| Auto-purge deletes old records | O | Fails: deletes nothing (F-HIST-6) | — |
| Unrelated modules and older history are never changed | P | Holds | none |

Premises: P1 no caller transaction for a crash and no caller rollback; P2 a crash rolls back; P3 no crash between the
outer TCOMMIT and SetCommitted (observed with a probe patch); PC no crash while a developer-mode load has its
dependency transaction open; PI no IRIS restart after a developer-mode or no-transaction load kept a change; PD no
dependency loaded inside a transaction that later rolls back; PI-dep (fixed design) no IRIS restart after a dependency
began; PM (outside the model) nothing fails after the final commit: **false** in round 4 (F-LOAD-4), so a defect premise.

## Findings and decisions

The three deliverables of this run (no IPM source was changed):

- **BUGS.md**: 80 finding IDs: 65 code defects (6 High, 36 Medium, 23 Low), 11 documentation defects, 2 test gaps,
  1 withdrawn as intended (F-HIST-10), 1 refuted (F-REPO-6). 64 of the 65 code defects were seen on IRIS (2 only
  with a probe patch, 1 with a simulated legacy install), 1 only in an IPM test run; 32 have a proved witness. Grouped
  into 12 shared root causes; defects whose intent rests on an assumption are marked †.
- **DESIGN-SIMPLIFICATIONS.md**: 17 designs covering all 65 code defects, each tied to the resolved intent it
  implements; D1 (history) is a proved design.
- **QUESTIONS.md**: no open intent decision remains. The 44 decisions of `decisions/` were researched in round 4
  (code, git history, IPM wiki, GitHub issues/PRs, IPM's CI, new IRIS runs; `decisions/research/`): 24 resolved by an
  authoritative source, 20 by a stated assumption, each listed with its alternatives for a maintainer to confirm.
  18 verification items remain unresolved (mirrors, a remote HTTP registry, real ZPM 0.7.4, the graceful-stop hang,
  proof premises and bounds, 3.8 sign-off).

Round 4 also showed premise PM ("nothing fails after the final commit") false (F-LOAD-4), so every load/history
refinement claim now names PM as a defect premise (`models/verus/load_general.TRUST.md` G7).
