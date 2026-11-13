# IPM Verus uplift: handoff

Initial uplift of InterSystems Package Manager (IPM) at `571e04a1` with the G5-Lean-V2 Verus guide, run on 2026-11-13. Plan, pins and assignments: [uplift-plan.md](uplift-plan.md). Verification runbook: [`../../Verus_verification/README.md`](../../Verus_verification/README.md).

STATUS: DRAFT, pending the round 3 (final) review outcome.

## What was delivered

| Artifact | Path | State |
|---|---|---|
| Governing ontology for all of IPM, in depth for load and history | `.g5/ontology/` (18 pages, 148 blocks: 122 I, 5 D, 21 Q) | Checker clean. Fresh-reader check and round 1 and 2 repairs applied |
| Intent and operation inventories, capability coverage map | `A/` | Done |
| Verus model: L1 contract, L2 as-built machine, executable twins, proofs, controls, D1 repair variant | `.g5/Verus_verification/crates/ipm-load-history/` | 260 verified, 0 errors, no trusted assumptions (`results/verus/20261113T214349Z-asbuilt-proofs-r2`). 12 controls: 11 detected on the final inputs, and control 05 survives as an equivalent mutant |
| Provider premises | `.g5/Verus_verification/premises.md` | P-01..P-11 and the repair premise R-01 confirmed natively. P-12 not run |
| Native validation in IRIS 2026.1 | `.g5/Verus_verification/native/`, `results/native/`, `B/native-report.md`, `B/native-replay.md` | 9 product suites, hypothesis cases a-g, stage C cases. 57 of 59 model cases realized: all agree with the model (1161 fields), and 23 conform to the contract while 34 violate it. 2 approximate |
| Verification coverage map | `.g5/Verus_verification/coverage-map.md` | Final |
| Product defects with repair proposals | `.g5/Verus_verification/bugs_issues.md` | D1-D6, D9-D20, issues I1-I2 |
| Design simplifications | `.g5/Verus_verification/design_simplification.md` | Six proposals |
| Reviews and dispositions | `C/round-1/`, `C/round-2/`, `C/round-3/` | Rounds 1 and 2 done and repaired. Round 3 pending |

## Established claims (supported within scope)

Scope: one `load` with up to two leaf dependencies, in every transaction mode and dev flag, with or without `-force`, from every modeled installed state and fault. A caller-held transaction is excluded unless stated. Claims named after a theorem are proved symbolically in Verus over that domain. The others say what they rest on. All of them are confirmed by native observation of the same cases.

- The transaction level is balanced. Every `LoadNewModule` activation returns at its entry `$TLEVEL` (`theorem_tlevel`).
- A top-level load's history record survives its own rollback and either kind of process end (`theorem_top_record_durable`).
- A load that returns has a finalized record with an end time, and its `Success` equals the returned status (`theorem_top_record_complete`).
- If a load changed module data, its record reads `Committed=1` (`theorem_commit_sound`).
- Finalized records never change, including under foreign edits (`theorem_r2`).
- A load never removes a durable history record (`theorem_r1`).
- Downgrade and update-required refusals write no module data in any mode. This is proved for the listed cases (`case_c11`, `case_c15`), explored exhaustively over the single-fault domain (L1-NS-ALL), and its controls 08 and 09 are detected. It is not a symbolic theorem.
- In developer mode, a failure before the first commit rolls everything back. A failure after it keeps the manifest and, after the R2-01 repair, the phase work done before the failure. This rests on the L1-NAMESPACE clause, checked by the explorer and the case lemmas. Variant 212 (phase work rolled back) is the control.

These rest on IRIS premises P-01..P-11, which were confirmed natively. The SIGKILL rollback is delayed by slot cleanup (105 s to about 10 min observed), and during that window other processes read uncommitted data.

## Confirmed product defects

Twenty entries are in `bugs_issues.md`. Each has a governing source, a native reproduction through the public command, and a repair proposal. Defects that depend on a pending decision are marked.

| Severity for users | Defects |
|---|---|
| Data loss | D2 (a filtered `history delete` can delete every record), D1 (dependency records lost on rollback) |
| Wrong installed state | D4 (decision pending: Q-17), D11, D16, D17, D19 (one namespace's `-DNoTransaction` default applies to every namespace), D14 (decision pending: Q-22) |
| Wrong history content | D9, D10, D12, D13, D6 (decision pending: Q-14) |
| Broken features | D3 (retention purge never deletes), D5, D15 (`.tar.gz`), D18 (decision pending: Q-18), D20 (`GlobalRecords`) |

D7 was withdrawn, because the wiki documents the behavior on purpose. I1 and I2 are issues with no governing requirement.

## Human decisions needed

Ordered by how much they change the verdicts. Each is a Q block in the ontology, and its provisional answer is what the model and native checks used.

| Q | Ontology ID | Provisional answer | What depends on it |
|---|---|---|---|
| Q-17 | `load.refusal.requirements.timing` | Check system requirements before any write, in every mode | D4 |
| Q-22 | `transaction.flag-sources.dependency-defaults` | A dependency's own `<Defaults>` NoTransaction or NoJournal applies inside its parent's transaction, so its changes escape the parent's rollback. Its record must survive. If accepted, D14 becomes intended behavior, and only its lost record (D1) remains a defect | D14 |
| Q-05 / Q-06b | `history.committed.nested` | A record survives a caller's rollback. `Committed` is exact only for actions started outside a caller transaction | V8 (record lost under a caller transaction) |
| Q-08 | `history.record.abnormal-end` | `Committed` reflects kept changes even on an unfinished record | V9 deviation (SIGKILLed dev load reads `Committed=0`) |
| Q-21 | `history.locked.unfinished` | An unfinished record is not finalized | V10 deviation (Terminate finalizes it) |
| Q-14 | `history.phases.returned-errors` | Every started phase records its end and status | D6 |
| Q-18 | `history.read.filter.default-modifiers` | Default modifiers are not `history` filters | D18 |
| Q-02 | `load.refusal.update-required.dev-bypass` | `-dev` lifts the update-required refusal (matches the product; README and help name only `-force`) | docs |
| Q-03 | `dev-mode.overwrite-guard.force` | The guard covers `install` only and `-force` does not lift it. This conflicts with wiki/00 L129, so a maintainer must decide | docs or code |
| Q-01, Q-04, Q-07, Q-09..Q-13, Q-15, Q-16, Q-19, Q-20 | see the coverage map in `A/coverage-map.md` | see the ontology | no current defect verdict |

## Incomplete work and open obligations

- Not modeled, with native or suite evidence only:
  - the history readers (filters, delete, purge, `-globally`, `GlobalRecords`);
  - per-module `<Defaults>` transaction flags (D14 observed natively);
  - the `install`, `update`, `reinstall` and `uninstall` paths;
  - nested dependencies or more than two;
  - legacy XML installs;
  - effects outside the database (Q-04).
- `Committed` implying changed data is not a theorem, because the product violates it (D12).
- Repairs are modeled only for D1 (variant 103). In default and developer modes it meets every dependency clause, including at process ends. Its premise R-01 (a History row written with journaling off survives every rollback and SIGKILL) is CONFIRMED natively (N-P20). In NoTransaction and NoJournal, residual violations come from D12 and the Q-08 deviation.
- Native checks not run:
  - P-12 (IRISTEMP at restart);
  - concurrent loads (Q-16, OBS-H13);
  - self-uninstall (OBS-H15);
  - the legacy XML path (OBS-L16);
  - remote or ORAS repositories as a dependency source;
  - git URL loads (OBS-L13).
- B1-C49T and B1-C49K could only be approximated, with a debugger stall. They agree with the model but are not counted as realized.
- Suspected, from code only: OBS-L4, L5, L7, L9, L11, L14, L15, L17, H8, H12, H13, H15 (listed in `bugs_issues.md`).

## Missing evidence and process record

- **Usage-limit stop.** A service usage limit stopped three delegates at about 18:50 UTC: the A3 ontology author, the B1 modeler and the B2 native validator. The work resumed at 19:13 UTC from the saved artifacts.
  - B2 had finished its phase-1 report.
  - A3 had not started the fresh-reader corrections.
  - B1 had a partial crate with no receipts.
  - No artifact was lost. Every file changed after 18:49 belongs to B2's report or B1's crate.
- **Transcript gap.** The orchestrator's saved transcript records a `TaskStop` and a `WebSearch` before the stop whose content was not kept. No artifact cites them, and no file outside the delegates' write ownership changed in that window. Two orchestrator wait commands were stopped at their time limits. They only waited for files and produced no evidence.
- **Void native runs before the runner had its end marker.**
  - `20261113T173808Z-smoke-history-suite` hung without the ORAS service and is recorded VOID.
  - An earlier History run coincided with a host disk-full event and has no receipt. It is noted in the plan.
- **Superseded or incomplete runs are kept and annotated in `results/index.jsonl`.** These are:
  - early control batches with no verdict;
  - a receipt-parser gap;
  - the first replay attempt (binding errors);
  - the S4 binding;
  - C4 (cleanup wait);
  - a reviewer's invalid control invocation.
- **Expectation provenance.** Some phase-1 cases were evaluated against an expectations file edited after the run. They were replayed with snapshots (`20261113T184230Z-replay-fast`) or superseded by the K-variant replay, and re-evaluations sit beside the originals.
- **Second usage-limit stop.** The orchestrator's process stopped again at about 21:35 UTC, right after the round 2 reviewer wrote its review. It resumed at 21:38 UTC from the saved review and checkout. No delegate was running, and no artifact changed in that window except the reviewer's own files.
- **Final-candidate reruns.** After the round 2 repairs, the orchestrator reran every explorer run and every Verus control on the final inputs (`VV/crates/scripts/final-candidate-batch.sh`; receipts from `20261113T215113Z` to `20261113T215941Z`, including the sanity check). It also re-evaluated the native replays against the final `cases.json` without an IRIS run.
- **Environment.** IRIS Community hit its license connection limit once during the first History suite run. No later run logged a license or disk error.

## Review rounds

Rounds used: 3 of 3 (see `uplift-plan.md`).
- Round 1: an ontology reviewer (4 material, 10 minor findings) and a model and evidence reviewer (5 material, 10 minor). All findings were accepted, five with modifications, and repaired by their owners.
- Round 2: one fresh reviewer reproduced the runbook cold, and every result matched. It found 3 material and 8 minor findings, all accepted and repaired. These were: developer-mode phase work unconstrained in L1 (now control 12), repair proposals with no named rollback-proof write (now named and confirmed as R-01), and D14 reclassified as decision pending (Q-22).
- Round 3: pending.
