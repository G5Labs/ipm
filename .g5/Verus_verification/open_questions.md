# Open questions for the maintainers: IPM load and history

This file lists the decisions that the verification uplift of InterSystems Package Manager (IPM) needs from its maintainers. It covers the product at commit `571e04a1` (tag `v0.10.11-beta.28`). Each open question has a provisional answer that the formal model and the native checks used. You can answer a question without reading the uplift. Each section gives the question, what each source says, what the product does now and what each choice changes.

Conventions:

- The ontology block is the canonical text of each question. Paths such as `ontology/features/history.g5.md:151` are relative to `.g5/` and the links open the block.
- Product paths such as `Utils/Module.cls:1168` are under `src/cls/IPM/` at `571e04a1`. Test classes are under `tests/integration_tests/Test/PM/Integration/`.
- `wiki/NN Lx` is line x of page NN of the project wiki at revision `781e98fd` (2026-11-04). Authors and dates of wiki lines come from `git blame` of the wiki.
- `#NNN` is an issue or pull request on github.com/intersystems/ipm. Quotes were read from the public pages for this review.
- Short uplift paths such as `handoff.md:67`, `A/coverage-map.md`, `B/model-notes.md` and `C/round-2/` are under `.g5/uplift/ipm-verus-20261113/`. `bugs_issues.md` and `design_simplification.md` sit next to this file.
- `RN` is `.g5/Verus_verification/results/native/` (uplift runs). `FR` is `.g5/Verus_verification/followup/results/` (this follow-up). Follow-up native cases such as `N-R05-DEVUP` are in `FR/native/20261113T234817Z-r05-modes-nesting` (R05) and `FR/native/20261113T234810Z-r04-history-readers` (R04).
- Labels L-1 to L-12, H-1 to H-8 and E-1 to E-5 are findings of this follow-up in `.g5/uplift/ipm-quality-followup/work/followup-findings.md`.
- Finding IDs refer to the rewritten [bugs_issues.md](bugs_issues.md). It keeps the original D1-D20, I1 and I2 and adds D21-D30 and I3-I8. Suspected findings keep their A2 IDs (OBS-L*, OBS-H*). The original file is in git at commit `ae619795`. A finding whose verdict depends on a question is "conditional" there.
- Model results come from the original crate `crates/ipm-load-history` (59 cases, `cases/cases.json` sha256 `10410864…`) and from the follow-up models `followup/models/loadtx.py` (receipt `FR/20261113T235302Z-loadtx-model`) and `followup/models/history_readers.py` (receipt `FR/20261113T234915Z-history-readers-model`). "End states" are the outcomes the loadtx model explores: 62,764 for the product as built.
- Q-24 to Q-31 are new in this file. They continue the uplift's numbering and have no ontology block yet.

Question types:

- Policy disagreement between sources: two maintainer sources say different things.
- Design choice: no source covers the case, so the maintainers choose.
- Unknown platform behavior: the answer depends on IRIS behavior nobody has observed.
- Missing test evidence: the intent is clear enough, but the product behavior was not observed.

A section names a main type and, where it applies, a second one.

## Crosswalk

Every question identifier used in the uplift, the follow-up and this file.

| Identifier | Where it is used | Ontology block (file:line under `ontology/`) | Status | Section |
|---|---|---|---|---|
| Q-01 | A1 (`uplift/ipm-verus-20261113/A/intent-inventory.md:190`), ontology, model `q01_keep`, handoff collapsed row (`handoff.md:67`) | [`dev-mode.sticky`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky) (`concepts/transactions-and-dev-mode.g5.md:116`) | open, product matches the provisional answer | Q-01 |
| Q-02 | A1 (`:191`), ontology, model `q02_dev_bypass`, handoff `:65` | [`load.refusal.update-required.dev-bypass`](../ontology/features/load.g5.md#load.refusal.update-required.dev-bypass) (`features/load.g5.md:114`) | open, product matches | Q-02 |
| Q-03 | A1 (`:192`, provisional "doc error"), ontology (maintainer conflict since R1-ONT-05), handoff `:66` | [`dev-mode.overwrite-guard.force`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.force) (`concepts/transactions-and-dev-mode.g5.md:107`) | open, product matches, A1 wording superseded | Q-03 |
| Q-04 | A1 (`:193`), ontology, handoff `:67` | [`transaction.rollback-scope`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope) (`concepts/transactions-and-dev-mode.g5.md:24`) | open, product matches | Q-04 |
| Q-05 | A1 (`:194`, then also covered developer-mode dependencies), ontology, model `q05_caller_exact`, handoff `:60` row "Q-05 / Q-06b", original `bugs_issues.md` deviation row "Q-05" (now D25) | [`history.committed.nested`](../ontology/features/history.g5.md#history.committed.nested) (`features/history.g5.md:151`) | open, renamed scope in round 1 (caller-held transactions only) | Q-05 |
| Q-06 | A1 (`:195`, then asked about survival), ontology, absent from the handoff table | [`history.dependencies.parent-rollback`](../ontology/features/history.g5.md#history.dependencies.parent-rollback) (`features/history.g5.md:176`) | open, renamed scope (only `Success`; survival became the I block `history.survives-rollback`) | Q-06 |
| Q-06b | model only (`crates/ipm-load-history/src/contract.rs:35`), handoff `:60` | survival half of `history.committed.nested` (`features/history.g5.md:151`) | alias of Q-05 | Q-06b |
| Q-07 | A1 (`:196`), ontology, handoff `:67` | [`history.committed.side-effects`](../ontology/features/history.g5.md#history.committed.side-effects) (`features/history.g5.md:161`) | open, product matches | Q-07 |
| Q-08 | A1 (`:197`, provisional "unknown outcome"), ontology (flipped by R1-ONT-02), model `q08_exact`, handoff `:61`, original `bugs_issues.md` row "Q-08" (now D26) | [`history.record.abnormal-end`](../ontology/features/history.g5.md#history.record.abnormal-end) (`features/history.g5.md:58`) | open, product departs | Q-08 |
| Q-09 | A1 (`:198`, provisional "not required"), ontology (flipped by R1-ONT-06), handoff `:67` | [`history.record.unlogged-refusals`](../ontology/features/history.g5.md#history.record.unlogged-refusals) (`features/history.g5.md:49`) | open, product departs (code reading) | Q-09 |
| Q-10 | A1 (`:199`), ontology, handoff `:67` | [`history.read.filter.invalid-column`](../ontology/features/history.g5.md#history.read.filter.invalid-column) (`features/history.g5.md:214`) | open, product departs | Q-10 |
| Q-11 | A1 (`:200`), ontology, handoff `:67` | [`history.delete.confirm-with-filter`](../ontology/features/history.g5.md#history.delete.confirm-with-filter) (`features/history.g5.md:262`) | open, product matches | Q-11 |
| Q-12 | A1 (`:201`), handoff `:67` "(Q-12 became a decision)" | D block [`history.delete.global-unfiltered`](../ontology/features/history.g5.md#history.delete.global-unfiltered) (`features/history.g5.md:271`) | resolved, became a D block (A4 R10) | Q-12 |
| Q-13 | A1 (`:202`, two-part question), ontology (narrowed by A4 R10), handoff `:67` | [`history.retention.effect`](../ontology/features/history.g5.md#history.retention.effect) (`features/history.g5.md:307`) | open, product departs (code reading) | Q-13 |
| Q-14 | A1 (`:203`), ontology, model `q14_end`, handoff `:63` | [`history.phases.returned-errors`](../ontology/features/history.g5.md#history.phases.returned-errors) (`features/history.g5.md:115`) | open, product departs, sources point to the provisional answer | Q-14 |
| Q-15 | A1 (`:204`, proposed label `install`), A4 R9, ontology (R1-ONT-07, R3-09), handoff `:67` | [`history.fields.dependency-source`](../ontology/features/history.g5.md#history.fields.dependency-source) (`features/history.g5.md:95`) | open, product matches | Q-15 |
| Q-16 | A1 (`:205`), ontology, handoff `:67` | [`history.keep-on-ipm-uninstall.other-namespaces`](../ontology/features/history.g5.md#history.keep-on-ipm-uninstall.other-namespaces) (`features/history.g5.md:321`) | open, product departs (code reading) | Q-16 |
| Q-17 | A1 (`:206`), ontology, model `q17_before`, handoff `:57` | [`load.refusal.requirements.timing`](../ontology/features/load.g5.md#load.refusal.requirements.timing) (`features/load.g5.md:129`) | open, product departs | Q-17 |
| Q-18 | A3 (`A/coverage-map.md:189`, from OBS-H2), ontology, handoff `:64` | [`history.read.filter.default-modifiers`](../ontology/features/history.g5.md#history.read.filter.default-modifiers) (`features/history.g5.md:223`) | open, product departs | Q-18 |
| Q-19 | A3 (`A/coverage-map.md:190`, from A1 conflict 27), ontology, handoff `:67` | [`reinstall.version`](../ontology/features/install.g5.md#reinstall.version) (`features/install.g5.md:43`) | open, product matches | Q-19 |
| Q-20 | A4 R12 (`A/reconstruction-dispositions.md:20`), ontology, handoff `:67` | [`history.delete.running-records`](../ontology/features/history.g5.md#history.delete.running-records) (`features/history.g5.md:276`) | open, product matches for `history delete` | Q-20 |
| Q-21 | C round 1 R1-ONT-02 (`C/round-1/repairs-ontology.md:3`), ontology, handoff `:62` | [`history.locked.unfinished`](../ontology/features/history.g5.md#history.locked.unfinished) (`features/history.g5.md:290`) | open, product departs | Q-21 |
| Q-FIN | model only (`contract.rs:47`) | as Q-21 | alias of Q-21 | Q-FIN |
| "new Q (round 1)" | original `bugs_issues.md` deviation row (git `ae619795`, line 194), now D27 | as Q-21 | label for Q-21 | Q-21 |
| Q-22 | C round 2 R2-03 (`C/round-2/repairs-ontology.md:7`, then about `NoTransaction` or `NoJournal`), narrowed by R3-03, handoff `:58`, loadtx option "Q22=A/B" | [`transaction.flag-sources.dependency-defaults`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults) (`concepts/transactions-and-dev-mode.g5.md:50`) | open, split in round 3 (now `NoJournal` only), product matches | Q-22 |
| Q-23 | C round 3 R3-03 (`C/round-2/repairs-ontology.md:20`), handoff `:59`, loadtx option "Q23=A/B" | [`transaction.flag-sources.dependency-no-transaction`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction) (`concepts/transactions-and-dev-mode.g5.md:59`) | open, split from Q-22, product departs (now observed natively) | Q-23 |
| "Q-05 / Q-06b" | handoff `:60` | `history.committed.nested` | one row for Q-05 and its alias | Q-05 |
| collapsed row | handoff `:67` (Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20) | see each | eleven questions in one row, "no current defect verdict" | each |
| Q-24 | this file (L-5) | none yet; nearest [`dev-mode.overwrite-guard`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard) (`concepts/transactions-and-dev-mode.g5.md:100`) | new, open | Q-24 |
| Q-25 | this file (L-4) | none yet; nearest Q-22 | new, open (only if Q-22 is A) | Q-25 |
| Q-26 | this file (L-8) | none yet; nearest [`history.committed`](../ontology/features/history.g5.md#history.committed) (`features/history.g5.md:125`) | new, open (only if Q-22 is A) | Q-26 |
| Q-27 | this file (H-4), readers model parameter QE | none yet; nearest [`history.delete`](../ontology/features/history.g5.md#history.delete) (`features/history.g5.md:253`) | new, open | Q-27 |
| Q-28 | this file (H-5, H-6, OBS-H11) | none yet; nearest [`history.read.api`](../ontology/features/history.g5.md#history.read.api) (`features/history.g5.md:247`) | new, open; the `details` part is settled by `wiki/09` L171 | Q-28 |
| Q-29 | this file (H-7), readers model parameter QA | none yet; nearest Q-20 and [`history.retention`](../ontology/features/history.g5.md#history.retention) (`features/history.g5.md:299`) | new, open | Q-29 |
| Q-30 | this file (L-9 to L-12) | none yet; nearest [`history.survives-rollback`](../ontology/features/history.g5.md#history.survives-rollback) (`features/history.g5.md:67`) | new, open, design choice | Q-30 |
| Q-31 | this file (raised inside `bugs_issues.md` D12) | none yet; nearest D block [`history.committed.marking`](../ontology/features/history.g5.md#history.committed.marking) (`features/history.g5.md:136`) | new, open | Q-31 |

Model parameters and option labels:

| Name | Where | Question |
|---|---|---|
| `q01_keep`, `q02_dev_bypass`, `q05_caller_exact`, `q06b_caller_survive`, `q08_exact`, `q14_end`, `q17_before`, `qfin_unended` | `crates/ipm-load-history/src/contract.rs:21-52` (struct `Q`, provisional values in `qprov`), alternatives in `explore.rs:43-55` (`q_alternatives`) | Q-01, Q-02, Q-05, Q-06b (Q-05), Q-08, Q-14, Q-17, Q-FIN (Q-21) |
| "Q22=A", "Q22=B", "Q23=A", "Q23=B" | `followup/models/loadtx.py` docstring | Q-22 and Q-23. A is the ontology's provisional answer for both. The product does Q-22 A and Q-23 B |
| QG, QE, QA | `followup/models/history_readers.py:31-36`, `:192` | QG is the Q-12 D block. QE (default "report") is Q-27. QA (default "preserve-active") is Q-29 and the purge half of Q-20 |

Coverage of the final question set:

- 22 of 22 ontology Q blocks have their own section (Q-01 to Q-11 and Q-13 to Q-23).
- 1 of 1 former question that became a D block (Q-12) has a short section marked resolved.
- 2 of 2 model-only identifiers (Q-06b, Q-FIN) have alias sections.
- 17 of 17 A1 rows (Q-01 to Q-17) and 4 of 4 other labels map to a section. These are the original `bugs_issues.md` row labels "Q-05", "Q-08" and "new Q (round 1)" and the handoff row "Q-05 / Q-06b". So does the handoff's collapsed row.
- 8 new questions (Q-24 to Q-31). Seven come from this follow-up's findings (L-4, L-5, L-8, L-9 with L-10 to L-12, H-4, H-5 with H-6, H-7). One (Q-31) was left inside the D12 entry of `bugs_issues.md`.

## Decisions ordered by consequence

Ranked by how many defect verdicts, model results and native outcomes each decision changes.

1. Q-22 and Q-23, taken together, with Q-25 and Q-26 that follow from them. They decide the class of D14, D22 and D28, the repair of D21 and the shape of the D1 repair. In the follow-up model the product conforms to the "what a process end or return leaves" clause only under Q-22 A with Q-23 B. Choosing Q-22 B makes 2,440 of 62,764 end states violations. Choosing Q-23 A (the provisional answer) makes 5,780 violations. Native evidence: M5, N-R05-NESTF, N-R05-Q23F.
2. Q-30, the D1 repair design. D1 loses history records, which is data loss. Every variant of the proposed repair leaves violations unless Q-22 and Q-23 are settled one way.
3. Q-05 with its alias Q-06b. Decides D25. Under the alternative of Q-06b, 4 of the 59 original model cases change verdict (B1-C42, C43, C50T, C50K), more than any other parameter. It also decides whether IPM may write history outside a caller's transaction, which the repair needs. IPM's own installer runs nested shell commands inside a load (`ontology/features/ipm-setup.g5.md:43`).
4. Q-17. Decides D4 (native B2, B3, B4) and model cases B1-C23 and C24.
5. Q-08 and Q-21. They decide D26 (C4R) and D27 (C2) and part of the D10 repair. They also decide whether the guarded D1 repair may leave dependency rows unfinalized at a process end. Q-FIN's alternative changes 2 model verdicts (B1-C46T, C48T).
6. Q-14. Decides whether D6 is a supported defect or documented behavior. One native reproduction, one model case (B1-C28).
7. Q-18. Decides D18. Because of D19, one namespace's defaults break `history` in every namespace.
8. Q-10, Q-27 and Q-28, the history readers. They decide D24, the repair of D23 and issue I4. In the readers model the product returns OK for 104 `find` and 52 global-delete commands where the contract wants an error (unknown column or default modifier). D2 and D20 are supported defects regardless.
9. Q-31. Decides the repair path of D12 (B1-C08).
10. Q-24. A developer's in-place debugging setup is overwritten without notice (N-R05-DEVUP).
11. Q-13, Q-20 and Q-29, the purge. Q-13 decides the suspected OBS-H12. The others matter once D3 is fixed, because today the purge deletes nothing.
12. Q-01, Q-02, Q-03, Q-11 and Q-19. The product matches the provisional answer, so the decision mostly fixes documentation. Q-01 and Q-02 each change one model case.
13. Q-04, Q-06, Q-07, Q-09, Q-15 and Q-16. No native verdict depends on them. Q-09 and Q-16 decide the suspected D29 and OBS-H13. Q-04 decides issue I6 and Q-15 decides OBS-H7. Q-06 cannot be observed until D1 is fixed.

## Questions from the uplift

### Q-01: Does a plain `load` keep developer mode?

- Canonical: [`dev-mode.sticky`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky), `ontology/concepts/transactions-and-dev-mode.g5.md:116`.
- Question: when a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?
- Type: policy disagreement between sources. An issue's wording reads wider than its fix.
- Status: open. The product already does the provisional answer, so the decision only confirms it.
- Interpretations and authority:
  - A (provisional), keep the stored mode. The fix for #892 removed the default that put every `load` in developer mode (`git show f83b207e`, #887, removed `set tParams("DeveloperMode") = $get(tParams("DeveloperMode"), 1)`). `-nodev` exists to turn it off (help `Main.cls:307`).
  - B, a plain load always uses normal mode. Issue #892 (isc-jili, 2025-09-16): "The module should only be DevMode if the -dev flag is included in the command."
- Product behavior: A. `LoadNewModule` keeps the stored `DeveloperMode` when the command does not set it (`Utils/Module.cls:1298-1307`). Native g8 keeps developer mode and g9 (`-nodev`) clears it (`RN/20261113T182832Z-cases-dg`).
- Checked in this follow-up: the #892 thread reports a load that turned developer mode on. isc-kiyer closed it as fixed by #890. It says nothing about a module already in developer mode. `git log -S'name="nodev"'` shows `-nodev` in the help long before the fix (commit `56346977`, 2022-11-03).
- Most reasonable assumption: A. Under B, every plain reload during development would drop developer mode.
- Decision requested: A, keep the stored mode and document that `-nodev` turns it off. Or B, make a plain load reset to normal mode. B changes `LoadNewModule` and makes model case B1-C20 a violation.
- Findings affected: none. Related: OBS-L3 (suspected), a plain load does not save `Root` and `Repository`.
- Stakes: developers who reload a module they are editing. Under B, a plain `load` silently turns off the `^Sources` mapping and the keep-on-failure behavior.

### Q-02: Does `-dev` lift the update-required refusal?

- Canonical: [`load.refusal.update-required.dev-bypass`](../ontology/features/load.g5.md#load.refusal.update-required.dev-bypass), `ontology/features/load.g5.md:114`.
- Question: should `-dev`, like `-force`, allow a `load` or `install` of a newer version whose manifest declares `<UpdatePackage>`?
- Type: policy disagreement between sources.
- Status: open. The product does the provisional answer, so the decision confirms it and fixes the docs.
- Interpretations and authority:
  - A (provisional), `-dev` lifts it. Test `InstallModule.cls` `TestDevMode` asserts "Trying to install newer version of already existing update-simple with -dev flag succeeds" (`:177-178`, added in `f83b207e`, #887). The squash message of `f83b207e` lists "Testing installing older & newer modules in -dev and -force tests".
  - B, only `-force` lifts it. README L62 ("This behavior can be overridden with -force flag"), the `-force` help (`Main.cls:313`, `:441`), the refusal text itself (`Utils/Module.cls:1220`, "please use the -force flag") and `wiki/08` L3 ("blocked by default") name only `-force`.
- Product behavior: A. The guard skips developer mode (`Utils/Module.cls:1219`) and the update steps stay pending for a later `update`. Native g5 (`load -dev` of 3.x over 2.1.1) loads (`RN/20261113T182832Z-cases-dg`). A read-only repository forces normal mode only after this check (`Utils/Module.cls:1277-1288`), so `install -dev` from one still bypasses it.
- Checked in this follow-up: the #903 and #887 threads. #903 (isc-eneil, 2025-09-24) asks IPM to "redirect the user to use the -dev or -force flags or run `reinstall`", but it is about installing a module that is already installed, a case the D block `load.same-version` later allowed. So #903 supports A less directly than the ontology metadata suggests. No visible #887 comment names `-dev` as a bypass of this refusal.
- Most reasonable assumption: A, because a maintainer test asserts it. The README, help and message are incomplete.
- Decision requested: A, keep the bypass and add `-dev` to README L62, the `-force` help and the refusal text. Or B, remove the bypass at `Utils/Module.cls:1219` and change `TestDevMode`. Under B, model case B1-C17 becomes a violation.
- Findings affected: none in `bugs_issues.md`.
- Stakes: developers loading a newer version of a module they work on. Under A the update steps do not run until `update`, so data may lag the code.

### Q-03: Scope of the developer-mode overwrite guard and `-force`

- Canonical: [`dev-mode.overwrite-guard.force`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.force), `ontology/concepts/transactions-and-dev-mode.g5.md:107`.
- Question: may `-force` let a normal-mode install overwrite a module installed in developer mode? Does the guard cover `load`?
- Type: policy disagreement between sources.
- Status: open. The product does the provisional answer. Q-24 adds the dependency case.
- Interpretations and authority:
  - A (provisional), keep the current scope. The guard covers top-level `install` only and `-force` does not lift it. Authority: the code since the guard was added (`d541c5b4`, 2022-08-23, `If $Get(pParams("Install")),tInstModule.DeveloperMode`) and the `-force` help, which speaks only of newer versions without update steps (`Main.cls:441`).
  - B, `-force` lifts it. `wiki/00` L129 (isc-dchui, 2026-05-13, wiki commit `cfacdd8` "Improve docs on dev mode"): "A module loaded in dev mode cannot be overwritten by a non-dev install ... Uninstall first, or use `-force`."
- Product behavior: A. The guard (`Utils/Module.cls:109-111`) needs `params("Install")`, skips IPM itself and lets `-dev` through. It never reads `Force`. `load` never sets `Install` (`Main.cls:2455-2460`). Not run natively.
- Checked in this follow-up: `git log -S'over previously installed in developer mode'` shows the guard added in `d541c5b4` and changed in `57f5bc02` (2022-11-04) to skip IPM and allow `-dev`. No commit ever tied it to `-force`. No issue or PR referenced by the sources discusses it. The follow-up also found that the guard never protects a dependency (L-5, Q-24).
- Most reasonable assumption: neither side is clearly stronger. The wiki line is the newest maintainer statement and the code is the oldest.
- Decision requested: A, keep the code and change `wiki/00` L129 to "Uninstall first, or install with `-dev`". Or B, let `-force` lift the guard at `Utils/Module.cls:109` and decide whether `load` is covered. Either way, answer Q-24.
- Findings affected: none in `bugs_issues.md`.
- Stakes: developers and scripts that install over a module someone is editing. Under B, `-force` also means "discard developer mode", which its help does not say.

### Q-04: What does a rollback promise beyond the database?

- Canonical: [`transaction.rollback-scope`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope), `ontology/concepts/transactions-and-dev-mode.g5.md:24`.
- Question: does a rollback of a failed package change promise more than undoing the database writes made inside its transaction?
- Type: policy disagreement between sources (a wording in the wiki against the code and the only cleanup promise). Second type: missing test evidence, because file effects after a rollback were never run.
- Status: open. The product does the provisional answer, so the decision confirms it, most likely with a wiki wording change.
- Interpretations and authority:
  - A (provisional), database writes only. Files copied into the module root or by `FileCopy`, pip installs, CPF merges, a git clone folder, IRISTEMP data and registry calls stay. The only cleanup IPM promises is the temporary folder of an unpacked archive: CHANGELOG 0.10.11 `#1183` ("cleans up its temp directory on failure").
  - B, the whole namespace returns to its previous state. `wiki/00` L126 (isc-dchui, 2026-09-21): "any failure rolls the namespace back to its previous state".
- Product behavior: A. Classes and compiled classes are rolled back (premise P-10, cases A1 and C1). By code reading, an archive load deletes and replaces `<ModuleRoot>/<name>/<version>/` before any version check and a rollback does not restore it (OBS-L7). Lock files, `LoadedResource` rows and git clone folders also stay (OBS-L11, OBS-L15, OBS-L13).
- Checked in this follow-up: issue #1183 (bdeboe, 2026-05-28) reports extracted files left in the temp folder after a load. isc-tleavitt answered "we need to fix cleanup". Nothing in it says a rollback restores files. No other referenced source speaks to files.
- Empirical unknowns: none for the database. The file effects (OBS-L7, OBS-L13) were never run natively.
- Most reasonable assumption: A. IRIS transactions cannot undo file writes and restoring files would be a new feature.
- Decision requested: A, state that a rollback covers database data only and reword `wiki/00` L126. Or B, make rollback restore files, which needs a new mechanism (snapshot and restore of the module folder at least).
- Findings affected: I6 (files and clone folders outlive the rollback, aliases OBS-L7, OBS-L11, OBS-L13), conditional on this question. OBS-L15 is out of scope there. Q-07 follows from this answer.
- Stakes: anyone whose install fails after files were copied. Under A, a failed archive install can leave the previous version's folder replaced while the database still names the old version.

### Q-05: What happens to a record under a caller's own transaction?

- Canonical: [`history.committed.nested`](../ontology/features/history.g5.md#history.committed.nested), `ontology/features/history.g5.md:151`. Model parameters `q05_caller_exact` and `q06b_caller_survive` (see Q-06b).
- Question: when a caller wraps an IPM command in its own transaction and later rolls back, must the history record survive? What should `Committed` say?
- Type: design choice. No source says whether running IPM inside a caller's transaction is supported.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), the record survives and `Committed` is exact only for actions started outside a caller transaction. Issue #366 (isc-tleavitt, 2023): "if install rolls back we still need the history".
  - B, compare `$TLEVEL` at the end with its value at the start, so any commit nested in a caller transaction reads uncommitted. Issue #1237 option 1, which it called "the smallest change that fixes both directions". PR #1322 chose per-commit marking instead (D block `history.committed.marking`).
  - C, caller-held transactions are unsupported, so the record may be lost with them.
- Product behavior: C in effect. Native case D1 (`tstart`, `load`, `trollback`) leaves no record. One ID was allocated and in flight the record read `Committed=1` and `Finalized=1` (`RN/20261113T184632Z-repro-caller-tx-history-lost`). D0 (`tcommit`) conforms. IRIS premise P-11 (nested commits are undone by an outer rollback) is confirmed.
- Checked in this follow-up: the #366, #1237 and #1322 threads. #366 notes the rollback as "a minor wrinkle from a transaction handling perspective" and says no more. #1237 and #1322 discuss only IPM's own transaction. In the follow-up loadtx model, every D1 repair variant still loses the top record under a caller's rollback (the saved examples of the 846 record-existence violations are of that kind), because the repair changes only dependency rows.
- Most reasonable assumption: A for survival, because #366 asks for history that survives rollback in general. IPM's own installer runs nested shell commands inside a load (`ontology/features/ipm-setup.g5.md:43`), so the case is not only exotic callers.
- Decision requested: A, records survive a caller's rollback. That needs history written outside the caller's transaction (for example with journaling off, as in `design_simplification.md` S2). `Committed` is then documented as exact only at `$TLEVEL` 0. Or C, declare caller-held transactions unsupported and document that records may be lost. B is a third option that changes only `Committed`.
- Findings affected: D25 (conditional on this question, formerly the deviation row "Q-05", model V8), D1 (same mechanism), Q-30.
- Stakes: tools and installers that call `%IPM.Main:Shell()` inside their own transaction. Under C, an audit trail misses those actions.

### Q-06: What `Success` does a dependency record show after its parent's rollback?

- Canonical: [`history.dependencies.parent-rollback`](../ontology/features/history.g5.md#history.dependencies.parent-rollback), `ontology/features/history.g5.md:176`.
- Question: a dependency that loaded successfully is undone by its parent's rollback. Its record survives and reads uncommitted. Should its `Success` keep the dependency's own status, or record the parent's failure?
- Type: missing test evidence. The value cannot be observed until D1 is fixed.
- Status: open. Sources point to the provisional answer.
- Interpretations and authority:
  - A (provisional), keep the dependency's own status. `wiki/09` L84: "`Success` is the status the action returned." The property comment `General/AbstractHistory.cls:38` says the same.
  - B, record the parent's failure, so a reader sees that the successful load was undone. No source.
- Product behavior: not observable. The record is lost (D1, cases A1 to A3, `RN/20261113T182759Z-cases-a`).
- Checked in this follow-up: no new source. The D1 repair proposal and model variant 103 keep the dependency's own `Success`. `Committed=0` already shows the undo.
- Most reasonable assumption: A.
- Decision requested: A, own status, with `Committed=0` as the sign of the undo. Or B, write the parent's error into the dependency's record, which the D1 repair would have to do on the rollback path (and cannot do at a process end).
- Findings affected: D1, whose entry names this question for the `Success` of the surviving record.
- Stakes: readers of the log after a failed install. Under A they must read `Committed`, not `Success`, to see that a dependency was undone.

### Q-07: Does `Committed` cover changes outside the database?

- Canonical: [`history.committed.side-effects`](../ontology/features/history.g5.md#history.committed.side-effects), `ontology/features/history.g5.md:161`.
- Question: does `Committed` also count file and operating-system changes such as copied files or pip installs?
- Type: design choice.
- Status: open. The product does the provisional answer, so the decision confirms it.
- Interpretations and authority:
  - A (provisional), no. The property comment `General/AbstractHistory.cls:41-43` ("either by a commit or by writing outside a transaction") and the PR #1322 body define it by commits and by writes outside a transaction.
  - B, yes, count file and OS changes too. No source asks for it, but the `wiki/09` L215-219 recipe ("find failures whose changes were kept") is the use case it would serve.
- Product behavior: A. `Committed` is set only at commits and at the no-transaction marker.
- Checked in this follow-up: #1237 and #1322 do not mention files or pip.
- Most reasonable assumption: A, consistent with Q-04 A.
- Decision requested: A, document that `Committed` is about database data. Or B, set `Committed` whenever a non-database effect is made, which needs every such effect to report itself.
- Findings affected: none.
- Stakes: users following the `wiki/09` recipe after a failed install that pip-installed packages. Under A that record reads uncommitted.

### Q-08: What should `Committed` say on a record whose action never ended?

- Canonical: [`history.record.abnormal-end`](../ontology/features/history.g5.md#history.record.abnormal-end), `ontology/features/history.g5.md:58`.
- Question: after a process dies mid-action, should the record's `Committed` reflect the changes that were kept, or is it meaningless?
- Type: design choice. Second type: unknown platform behavior, because what IRISTEMP holds after an instance restart (premise P-12) was never run.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), `Committed` reflects the changes kept, even on an unfinished record. Issue #1237 (isc-dchui): "`Committed` should reflect whether the action's changes were actually retained." `wiki/09` L215-219 recipe `-Dcommitted=1 -DSuccess="0*"` finds failures that kept changes.
  - B, an empty end time means the outcome is unknown, so readers must not trust `Committed=0`. This was A1's provisional answer (`A/intent-inventory.md:197`).
- Product behavior: departs from A. C4R, a developer-mode load killed with SIGKILL in Activate after both commits, keeps its module while the record reads `Committed=0` (`RN/20261113T183827Z-cases-c4r`). C2, ended by `$SYSTEM.Process.Terminate`, reads `Committed=1`. The durable row keeps its start values until `%OnClose` (D10).
- Checked in this follow-up: #1237 does not discuss crashes. The SIGKILL rollback delay is known (105 s to about 10 min, premise P-13). The follow-up model shows that a process end leaves exactly the writes whose durability point came before it (L-7), so A is implementable.
- Most reasonable assumption: A, because it is what the documented recipe needs.
- Decision requested: A, write `Committed` to the durable row at each commit, as the D10 repair proposes. Or B, document that `Committed` on a record with no end time means nothing.
- Findings affected: D26 (conditional on this question, formerly the deviation row "Q-08", model V9), D10, Q-30.
- Stakes: administrators investigating a killed install. Under the current product they see "uncommitted" for a module that is installed.

### Q-09: Should argument refusals of `update`, `reinstall` and `uninstall` be recorded?

- Canonical: [`history.record.unlogged-refusals`](../ontology/features/history.g5.md#history.record.unlogged-refusals), `ontology/features/history.g5.md:49`.
- Question: should IPM record `update`, `reinstall` and `uninstall` attempts that it refuses because of their arguments, as it records refused loads and installs?
- Type: design choice (what counts as an attempt). Second type: missing test evidence, because no native run covers it.
- Status: open. The product departs from the provisional answer by code reading. No defect is filed.
- Interpretations and authority:
  - A (provisional), record them. `wiki/09` L18: IPM records every action "it attempts, whether or not the attempt succeeded". Issue #366: "we still want to log failed installations". `install` with no name is already recorded (`Main.cls:2565-2572`).
  - B, argument refusals are not attempts, because they come before the action starts. A1's provisional answer.
- Product behavior: B. `update` refuses `PermitDowngrade` and a module that is not installed before creating a record (`Main.cls:4416`, `:4423`, record created at `:4438`). `reinstall` of a module that is not installed and `uninstall` with no name are refused the same way (`Main.cls:2685-2688`, `:2709-2712`).
- Checked in this follow-up: line numbers re-read at `571e04a1`. No issue covers these refusals.
- Most reasonable assumption: A is the plainer reading of the wiki, but the difference is small.
- Decision requested: A, create the record before these checks. Or B, document that argument errors are not recorded.
- Findings affected: D29 (suspected, conditional on this question).
- Stakes: low. Users looking for failed commands in the log.

### Q-10: How must `history` treat unusable filter columns and query errors?

- Canonical: [`history.read.filter.invalid-column`](../ontology/features/history.g5.md#history.read.filter.invalid-column), `ontology/features/history.g5.md:214`.
- Question: what should happen to a filter on a column that does not exist? What should `find` do when it cannot run its query?
- Type: design choice. The part about `Version_Major` is settled: the I block `history.read.filter` requires version-part filters and D2 is a supported defect against it.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), never drop a filter silently and report a failed query instead of showing an empty log. Within A, the alternatives are to refuse an unknown column with an error (the D2 repair proposal) or to ignore it with a warning and apply the rest.
  - The sources: `wiki/09` L90 ("Use `history schema` to list the filterable column names"), L112 (filter on `Version_Major`). No source covers unknown columns.
- Product behavior: `ConstructSQLWhere` stops at the first key that is not alphanumeric and drops it and every later key in collation order (`General/History.cls:103-105`, H-1). An unknown column gives `find` an empty listing with an OK status (E3, H-3), while `delete` fails with SQLCODE -29 (E7). `GetHistory` never checks `%SQLCODE` (`General/History.cls:132-148`).
- Checked in this follow-up: native N-R04-UNKNOWN (no rows, SQLCODE -29 hidden, OK status), N-R04-SEL (all 9 rows returned for a filter that should match 1) and N-R04-SEL2 (the version filter dropped, 2 rows instead of 1). The readers model reproduces the archived D2 cases.
- Most reasonable assumption: A, with an error for unknown columns, because `delete` already fails that way and a warning is easy to miss in scripts.
- Decision requested: refuse an unknown column with an error in both `find` and `delete`, or warn and apply the remaining filters. Either way, `find` must stop reporting success for a failed query.
- Findings affected: D24 (conditional on this question, but a defect under every listed option), D2 (supported regardless), D18, D23 (Q-27).
- Stakes: anyone filtering or deleting history. A silently widened filter is how D2 deletes a whole namespace's records.

### Q-11: Does `-confirm` with a filter delete everything?

- Canonical: [`history.delete.confirm-with-filter`](../ontology/features/history.g5.md#history.delete.confirm-with-filter), `ontology/features/history.g5.md:262`.
- Question: does `history delete -confirm` with a filter delete every record in the namespace or only the matching ones?
- Type: policy disagreement between sources. Second type: missing test evidence, because no native run combined `-confirm` with a filter.
- Status: open. The product does the provisional answer, so the decision confirms it and fixes the help text.
- Interpretations and authority:
  - A (provisional), only the matching ones. `wiki/09` L45-46 (isc-dchui, 2026-09-03): `-confirm` is "Required by `history delete` when no filter is given". L187 says the same.
  - B, everything. Help `Main.cls:825`: "If present, will delete all records in the current namespace."
- Product behavior: A by code reading. The guard (`Main.cls:4165-4166`) only refuses a delete with no filter and no `-confirm`. The delete always applies the filter (`Main.cls:4168-4171`). Native E5 and E6 cover the two neighbors, not this combination.
- Checked in this follow-up: the readers model covers local deletes with and without `-confirm`. For valid filters the product matches A (69 commands, H-8).
- Most reasonable assumption: A. The wiki is the later and more precise statement.
- Decision requested: A, reword the help to "required to delete without a filter". Or B, make `-confirm` ignore filters, which no user is likely to expect.
- Findings affected: none.
- Stakes: low, a help-text fix.

### Q-12: Global unfiltered delete without `-confirm` (resolved)

- Canonical: D block [`history.delete.global-unfiltered`](../ontology/features/history.g5.md#history.delete.global-unfiltered), `ontology/features/history.g5.md:271`. Former A1 Q-12 (`A/intent-inventory.md:201`).
- Status: resolved by the sources. `wiki/09` L195 (isc-dchui, 2026-09-03): "`-globally` bypasses the `-confirm` requirement, so `history delete -globally` with no filter deletes every record on the instance without prompting." Help `Main.cls:825`: "Ignored if -globally is passed too." A4 disposition R10 turned it into a D block.
- Open proposal, not a question: require `-confirm` for an unfiltered global delete (ontology metadata). The readers model keeps this as parameter QG. `bugs_issues.md` withdraws OBS-H9 for the same reason.
- Related new question: Q-27 (what a global delete does with errors).

### Q-13: May a failed purge stop the user's command?

- Canonical: [`history.retention.effect`](../ontology/features/history.g5.md#history.retention.effect), `ontology/features/history.g5.md:307`.
- Question: when the daily history purge fails, should the command the user typed still run?
- Type: design choice. Second type: missing test evidence, because no purge failure was run.
- Status: open. The product departs from the provisional answer by code reading.
- Interpretations and authority:
  - A (provisional), the command still runs. No source lets housekeeping block other commands.
  - B, report the purge error and stop, as a signal that retention is broken.
- Product behavior: B. `Shell` returns the purge error before running the command (`Main.cls:862-865`). The `zpm "<command>"` form discards that status (`ontology/root.g5.md`, `ipm.shell` metadata), so by code reading the command does nothing and says nothing. The purge date is stamped only after a successful delete (`General/History.cls:174-176`), so the failure repeats on every command that day.
- Checked in this follow-up: `git log -S'AutoPurgeOldHistory'` shows the early return was added with the purge itself (`2cef3073`, #992, 2026-03-09, co-authored by isc-dchui). The #992 and #977 threads do not say why. Since #1107, per-namespace delete errors are swallowed (Q-27), so a returned purge error can only come from listing namespaces (`General/History.cls:206-207`) or from writing the purge date in `%SYS` (`:183-189`). The reads of the purge date and of `HistoryRetain` run before the `try` (`:165`, `:168`), so their errors throw instead of returning a status.
- Most reasonable assumption: A.
- Decision requested: A, run the command and report the purge error as a warning. Or B, keep blocking and document it, including that `zpm "<command>"` hides the error.
- Findings affected: OBS-H12 (suspected, conditional on this question), D3 (today the purge deletes nothing, so the path is rarely reached), D23 (Q-27).
- Stakes: every IPM user on an instance with `HistoryRetain` set. One broken purge stops all IPM commands in every namespace.

### Q-14: Must every started phase record its end and status?

- Canonical: [`history.phases.returned-errors`](../ontology/features/history.g5.md#history.phases.returned-errors), `ontology/features/history.g5.md:115`.
- Question: must a phase record its end time and status when an `<Invoke>` returns an error status instead of throwing one?
- Type: design choice. The sources point one way.
- Status: open. The product departs from the provisional answer. This follow-up found one more source for it.
- Interpretations and authority:
  - A (provisional), yes. `wiki/09` L161 (isc-dchui): "A phase with no elapsed time never finished, which localizes a hang or a crash to that phase." Test `History.cls` `TestFailingPhases` asserts that a failed Activate phase has an end time and an error status (`History.cls:702-706`).
  - B, accept open phases for returned errors and document it. No source. Round 1 (R1-MOD-12) kept the question open because `wiki/09` L161 only says how to read an open phase.
- Product behavior: departs. The Invoke loops leave the `try` with `quit:$$$ISERR(tSC)` (`Storage/Module.cls:608-610`, `:669-671`), then `$$$ThrowOnError` (`:707`) throws before `FinalizePhase` (`:709-710`). The `catch` that finalizes the phase (`:692-695`) runs only for thrown errors. D6 (native repro `activate-invoke-phase-open`).
- Checked in this follow-up: `git log -S` shows the `catch` with `FinalizePhase` came with the history feature (`8a88d5bf`, #578, 2025-12-11), while the `quit:$$$ISERR` loops are older. So the gap is an interaction nobody chose. `TestFailingPhases` uses an Invoke on the default phase, which the product finalizes correctly.
- Most reasonable assumption: A. The decision would most likely confirm it. D6 would then be a supported defect instead of a conditional one.
- Decision requested: A, finalize the phase on every error path (`design_simplification.md` S5). Or B, document that a returned Invoke error leaves the phase open.
- Findings affected: D6 (conditional on this question), I1 (display of an open phase), model case B1-C28.
- Stakes: anyone diagnosing a failed install with `history details -phases`. An open phase sends them looking for a hang or crash that did not happen.

### Q-15: How are repository-pulled dependencies labeled and shown?

- Canonical: [`history.fields.dependency-source`](../ontology/features/history.g5.md#history.fields.dependency-source), `ontology/features/history.g5.md:95`.
- Question: how should the log label and show a dependency pulled from a repository during a `load` or `update`? What source does an `update` from a registry store?
- Type: policy disagreement between sources.
- Status: open. The product does the provisional answer, so the decision confirms it.
- Interpretations and authority:
  - A (provisional), keep the `load` label and the stored source. Listings show a source only for installs. Test `History.cls` `TestWithDependencies` expects a `load` record for the dependency of a `load` (`History.cls:750-770`). `wiki/09` L76: `Source` is "`n/a` for anything other than `install`".
  - B, label them `install` with their source. Issue #366: "'install' (including as a dependency)".
  - C, keep `load` and show the stored source for any record that has one. Conflicts with `wiki/09` L76.
  - D, clear the source for `load` records. Property comment `General/AbstractHistory.cls:20-27` ("For load and uninstall this will be empty").
- Product behavior: A. Dependencies of `load` and `update` are labeled `load` and store a repository source (`Utils/Module.cls:63-69`). Listings print `n/a` for any non-install record (`General/History.cls:457-462`). Native A0: `Action=load`, `SourceName=ipm-temp-modules-1`.
- Checked in this follow-up: #366 lists "Source registry information (for install only)". No later source changes it.
- Most reasonable assumption: A, because the maintainer test is the most specific source.
- Decision requested: A, keep and document that a dependency of a `load` is logged as `load`. Or B or C, which change `TestWithDependencies` or `wiki/09` L76.
- Findings affected: OBS-H7 (conditional on this question: a defect only under B or C).
- Stakes: low. Users asking "where did this module come from" for a dependency.

### Q-16: May uninstalling IPM delete other namespaces' in-progress records?

- Canonical: [`history.keep-on-ipm-uninstall.other-namespaces`](../ontology/features/history.g5.md#history.keep-on-ipm-uninstall.other-namespaces), `ontology/features/history.g5.md:321`.
- Question: may a full self-uninstall of IPM in one namespace delete the in-progress history copies of actions running in other namespaces?
- Type: design choice. Second type: missing test evidence (not run).
- Status: open. The product departs from the provisional answer by code reading.
- Interpretations and authority:
  - A (provisional), no, because history belongs to each namespace (`history.namespace`).
  - B, accept it, because removing IPM is rare.
- Product behavior: full cleanup kills `^IRIS.Temp.HistoryTempD` (`Lifecycle/Module.cls:51`), which is one instance-wide store (premise P-02, N-P06, N-P17). The maintainer comment at `:54` preserves only the permanent log.
- Checked in this follow-up: `git log -S` shows the kill came with the self-uninstall fix (`ffba34ec`, #1106 for #1057, 2026-04-16). Issue #1057 does not mention history.
- Most reasonable assumption: A, because the cost of a fix is small (kill only this namespace's rows).
- Decision requested: A, delete only rows that belong to the namespace being cleaned. Or B, document that uninstalling IPM can blank other namespaces' running records.
- Findings affected: OBS-H13 (suspected, conditional on this question).
- Stakes: low frequency. A running install elsewhere loses its in-progress record and ends with a durable row holding only its start values.

### Q-17: Must system requirements be checked before anything is written?

- Canonical: [`load.refusal.requirements.timing`](../ontology/features/load.g5.md#load.refusal.requirements.timing), `ontology/features/load.g5.md:129`.
- Question: must IPM check system requirements before it writes anything, also in developer mode and without a transaction?
- Type: policy disagreement between sources.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), yes. `wiki/03` L261 (Emma Neil, 2026-04-02): requirements "must be validated before module installation".
  - B, a refused developer-mode or `-DNoTransaction=1` load may keep its saved manifest, because developer mode keeps what it did. Issue #363 (isc-tleavitt, 2025-09-15): "we don't remove these entries when installing in dev mode (to support debugging), but will in non-dev mode."
- Product behavior: B. The check runs after the manifest import and after the developer-mode first commit (`Utils/Module.cls:1309-1321`). Refused `-dev`, `-DNoTransaction=1` and `-DNoJournal=1` loads keep the module record with no classes. `list-installed` shows the module and its record reads `Committed=1` (D4, cases B2 to B4).
- Checked in this follow-up: the order is original. `git show 9713db77` (the 2021 HSPM import) already checks after the developer-mode commit. The #363 thread does not mention system requirements.
- Most reasonable assumption: A, because a module that fails its requirements was never installable and the #363 rule is about failures of an installable module.
- Decision requested: A, check requirements on the parsed manifest before the transaction and the no-transaction marker (`design_simplification.md` S1). Or B, document that developer-mode and no-transaction loads keep a manifest-only module after a requirements refusal.
- Findings affected: D4 (conditional on this question), D12 (shares the marker), model cases B1-C23 and C24.
- Stakes: developers loading a module in the wrong namespace (for example one that needs `%SYS`). Under B they get a half-installed module that `list-installed` shows.

### Q-18: Do namespace default modifiers act as `history` filters?

- Canonical: [`history.read.filter.default-modifiers`](../ontology/features/history.g5.md#history.read.filter.default-modifiers), `ontology/features/history.g5.md:223`.
- Question: should the namespace's default modifiers (such as `-DNoTransaction=1`) become filters of `history find` and `history delete`?
- Type: design choice.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), no. Default modifiers are meant for package changes. The help of `history` (`Main.cls:820`) says filters come from "the -Dcol=val syntax".
  - B, apply them and document it. No source.
- Product behavior: B. Defaults merge into every command's data (`Main.cls:1067-1069`) and `history` reads that data as its filter (`Main.cls:4136`). After the product's own help example `default-modifiers -set -DNoTransaction=1 -DUpdateSnapshots=1`, `history find` shows no rows with an OK status and `history delete -confirm` fails (D18, stage C H2).
- Checked in this follow-up: `git log -S` shows the default merge dates from the 2021 import and the filter read from the history feature (`8a88d5bf`, #578). The interaction looks unplanned. D19 stores defaults instance-wide, so a default set in one namespace breaks `history` in all of them.
- Most reasonable assumption: A.
- Decision requested: A, `history` reads only its own command-line filters. Or B, document that defaults filter history, which also needs Q-10 so that a non-column default is reported.
- Findings affected: D18 (conditional on this question), D19, D24 (Q-10).
- Stakes: every user who sets a default modifier. Their log looks empty.

### Q-19: Does `reinstall` install the installed version or the latest one?

- Canonical: [`reinstall.version`](../ontology/features/install.g5.md#reinstall.version), `ontology/features/install.g5.md:43`.
- Question: does `reinstall <module>` install the version already installed, or the latest available version?
- Type: policy disagreement between sources.
- Status: open. The product does the provisional answer, so the decision confirms it and fixes the docs.
- Interpretations and authority:
  - A (provisional), the installed version. Help summary `Main.cls:448` ("Reinstalls the already installed version", added `7afb109f`, 2022-04-28) and both examples (`Main.cls:455-458`).
  - B, the latest version. Help description `Main.cls:450-451` ("from the latest version available in a configured repository", present since the 2021 import), copied to `wiki/02` L880 (2024-06-12). `wiki/01` L40-43 tells users to run `zpm "reinstall zpm"` to "update zpm to newer version".
- Product behavior: A. `Reinstall` pins the installed version (`Main.cls:2695-2697`). The 2021 import (`9713db77`) did the same.
- Checked in this follow-up: `git log -S` for each wording and for the pinning line. The code has pinned the version since 2021, so the description has been wrong all along.
- Most reasonable assumption: A.
- Decision requested: A, fix the help description, `wiki/02` L880 and the `wiki/01` upgrade recipe (use `update` or `install` instead). Or B, make `reinstall` take the latest version, which changes what scripts that reinstall get.
- Findings affected: none.
- Stakes: users following `wiki/01` to upgrade IPM. Under A they reinstall the same version and believe they upgraded.

### Q-20: May a delete or purge remove a running action's record?

- Canonical: [`history.delete.running-records`](../ontology/features/history.g5.md#history.delete.running-records), `ontology/features/history.g5.md:276`.
- Question: may `history delete` or the daily purge remove the record of an action that is still running? What should that action's final write do then?
- Type: design choice.
- Status: open. For `history delete` the product does the provisional answer, so the decision confirms it. The purge half is Q-29.
- Interpretations and authority:
  - A (provisional), any matching record may go and the running action still finishes normally whether or not its record comes back. Reason given: deleting is an explicit user choice.
  - B, skip records of running actions, or always write the final result back.
- Product behavior: A. Neither `history delete` nor the purge checks `Finalized` (OP-HIST-17, OP-HIST-21). N-P15: deleting the durable row before the action's end copy makes the record disappear and the end copy raises nothing.
- Checked in this follow-up: no source covers it. The readers model default (QA) preserves in-progress rows during the purge, which is option B for the purge (see Q-29).
- Most reasonable assumption: A for explicit deletes.
- Decision requested: A, confirm and document. Or B, add `Finalized=1` to the delete query, or recreate the row at the end.
- Findings affected: none. The D3 entry names this question for the purge once D3 is fixed. Q-21 (a record finalized without an end) and Q-29 interact.
- Stakes: low. An administrator who deletes history while an install runs loses that install's record.

### Q-21: May IPM finalize the record of an action that never ended?

- Canonical: [`history.locked.unfinished`](../ontology/features/history.g5.md#history.locked.unfinished), `ontology/features/history.g5.md:290`. Model alias Q-FIN. Label "new Q (round 1)" in the original `bugs_issues.md`.
- Question: may IPM finalize, and so lock against updates, a record whose action has no end time?
- Type: design choice.
- Status: open. The product departs from the provisional answer.
- Interpretations and authority:
  - A (provisional), no. The ontology defines a finalized record as one whose action ended and `wiki/09` L197 says a finalized record rejects updates, so a locked unfinished record could never be corrected.
  - B, finalize whenever IPM releases the record and accept locked records without an end time.
- Product behavior: B. `PersistToTwin` sets `Finalized=1` without checking the end time (`General/HistoryTemp.cls:53`, since #578). `$SYSTEM.Process.Terminate` runs `%OnClose` before IRIS rolls back (premise P-06b), so C2 is finalized with an open Activate phase and no end time (`RN/20261113T182903Z-cases-c-terminate`).
- Checked in this follow-up: the guarded D1 repair (L-12) leaves dependency rows unfinalized at such a Terminate. That is what A requires, so Q-21 also decides whether the guarded repair's behavior is a feature or a gap.
- Most reasonable assumption: A.
- Decision requested: A, set `Finalized` only when the action ended. Or B, document that a record can be locked with no end time.
- Findings affected: D27 (conditional on this question, formerly the deviation row "new Q (round 1)", model V10), D10, Q-30. Model cases B1-C46T and C48T change verdict under B.
- Stakes: administrators who find an unfinished record after a terminated install. Under B it cannot be corrected.

### Q-22: Does a dependency's own `NoJournal` default escape its parent's rollback?

- Canonical: [`transaction.flag-sources.dependency-defaults`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults), `ontology/concepts/transactions-and-dev-mode.g5.md:50`.
- Question: does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes stay when the parent's normal-mode load rolls back?
- Type: policy disagreement between sources.
- Status: open. The product does the provisional answer (A). If A is confirmed, Q-25 and Q-26 need answers too.
- Interpretations and authority:
  - A (provisional), yes. `wiki/00` L175 (isc-dchui, 2026-05-13): `<Defaults>` are "applied whenever this module is installed". `wiki/00` L155: with `NoJournal`, "changes cannot be rolled back". Issue #779 (isc-tleavitt, 2025-04-08) asked that `<Defaults>` be "respected in load/install". PR #879 (isc-dchui) did it. The maintainer doc comment `Utils/Module.cls:1033-1040` shows `<Defaults>` with `NoTransaction`.
  - B, the parent's transaction governs every nested load, so the dependency's `NoJournal` is ignored or the dependency is refused while a parent transaction is open. `wiki/00` L126 and L147: the whole load is one transaction and "a failure mid-way through is fully rolled back". `load.outcome`: "including dependencies loaded for it". Maintainer comment from the 2021 import, `Utils/Module.cls:1316-1317`: "Nested transactions in underlying calls to LoadNewModule during the dependency resolution process may be rolled back at the end of this method."
- Product behavior: A. Each activation merges its own `<Defaults>` under the parameters it received (`Utils/Module.cls:1166-1169`). Caller flags win over the parent's defaults, which win over the dependency's own, key by key (L-1, N-R05-PARDEF). A dependency's `NoJournal` turns journaling off inside the parent's open transaction (`:1191-1193`). Native M5 keeps the dependency after the parent's rollback (`RN/20261113T204405Z-stagec2`).
- Checked in this follow-up: #779, #879, #952 and #316. None discusses a dependency's defaults inside a parent transaction. #779 is about top-level modules. The follow-up modeled per-activation modes (loadtx.py) and ran 10 native cases (all as predicted). Findings: the top-level load loads the whole dependency closure with its own parameters, so a middle dependency's defaults do not reach its own dependencies (L-3). After a failed parent, a `NoJournal` dependency can stay installed while its own dependency is rolled back (L-4, N-R05-NESTF, Q-25). The top record then reads `Committed=0` (L-8, Q-26). In the model, with Q-23 as built, the product conforms to the end-state clause under A and violates it in 2,440 of 62,764 end states under B.
- Empirical unknowns: none for the database effect (premise P-03, N-P07). Remote and ORAS repositories were not run as dependency sources.
- Most reasonable assumption: no source decides it. A follows the flag-specific wiki lines and the product. B follows the general rule and the oldest code comment.
- Decision requested: A, the escape is intended. D14 becomes documented behavior (its lost record stays D1). Q-25 and Q-26 then need answers. Or B, the parent governs. D14 is then a defect and the fix ignores or refuses dependency-level `NoJournal` while a parent transaction is open. Under B, Q-25 and Q-26 disappear and the D1 repair works as proved (Q-30). `design_simplification.md` S8 makes Q-22 and Q-23 one load-wide rule.
- Findings affected: D14 (conditional: a defect only under B), D21 (broken dependency closure, Q-25), D22 (top record `Committed`, Q-26), D1, Q-30.
- Stakes: module authors who put `NoJournal` in `<Defaults>` to speed up installs and users whose install of a parent fails. Under A, part of the dependency tree stays installed after a "rolled back" failure, with no record (D1).

### Q-23: Does a dependency's own `NoTransaction` default suppress its parent's transaction?

- Canonical: [`transaction.flag-sources.dependency-no-transaction`](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction), `ontology/concepts/transactions-and-dev-mode.g5.md:59`.
- Question: should a dependency's own `<Defaults>` `NoTransaction` make the whole package change run without a transaction?
- Type: policy disagreement between sources.
- Status: open. The product departs from the provisional answer. This follow-up observed it natively for the first time.
- Interpretations and authority:
  - A (provisional), yes. `wiki/00` L149 (isc-dchui, 2026-09-21) gives the flag's purpose: "Use when some modules in the install conflict with an outer wrapping transaction". `wiki/00` L175: `<Defaults>` apply "whenever this module is installed". Without suppression, the flag does nothing for a dependency.
  - B, the flag affects only the dependency's own load and the parent's transaction still wraps and undoes it. `wiki/00` L147, `load.outcome`, the comment `Utils/Module.cls:1316-1317`. A user can pass `-DNoTransaction=1` for the whole command, because caller flags win (L-1).
- Product behavior: B. The dependency skips only its own `tstart` (`Utils/Module.cls:1172`, `:1194-1196`). Its writes stay journaled inside the parent's transaction and are undone by the parent's rollback (`:1358-1362`). Its no-transaction marker first sets `Committed=1` (`:1224-1227`) and then its record is lost (D1). N-R05-Q23F (parent fails): the dependency is rolled back and has no row. N-R05-Q23S (parent succeeds): the dependency is installed with `Committed=1` (L-2).
- Checked in this follow-up: the native pair above and the loadtx model. Under the provisional answer (with Q-22 A), the product violates the end-state clause in 5,780 of 62,764 end states. That is the largest single verdict change of any decision.
- Most reasonable assumption: no source decides it. A follows the wiki's stated purpose. B follows the product and the general rollback rule.
- Decision requested: A, suppression. The parent must know a dependency's flag before it starts its transaction, which needs one plan per load computed up front (S8). A failure then leaves a partial install. Or B, document that `NoTransaction` in `<Defaults>` has no effect when the module is installed as a dependency of a transactional load and point users to `-DNoTransaction=1`. Under B, the D1 repair must also undo the early `Committed=1` of such a dependency (L-10).
- Findings affected: D28 (conditional on this question: a defect under A), D1, Q-30.
- Stakes: module authors whose code cannot run inside a wrapping transaction. Today their `<Defaults>` do not help when someone installs their module as a dependency.

### Q-06b (alias of Q-05)

- Model-only parameter `q06b_caller_survive` (`crates/ipm-load-history/src/contract.rs:35`), first defined in `uplift/ipm-verus-20261113/B/model-notes.md:36`. It is the record-survival half of Q-05 ("the load's records must survive a rollback of a caller-held transaction"). It is not Q-06.
- Provisional value: true (records survive). Under the alternative, model cases B1-C42, C43, C50T and C50K change verdict.
- Answer it with Q-05.

### Q-FIN (alias of Q-21)

- Model-only parameter `qfin_unended` (`contract.rs:47`), first defined in `B/model-notes.md:40`. It asks whether a record whose action never ended may be finalized. Provisional value: false.
- Under the alternative, model cases B1-C46T and C48T change verdict.
- Answer it with Q-21.

## New questions from this follow-up

### Q-24: May loading a parent replace a dependency that is in developer mode?

- Canonical: no ontology block yet. Source: follow-up L-5. Nearest blocks: [`dev-mode.overwrite-guard`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard) (`ontology/concepts/transactions-and-dev-mode.g5.md:100`) and [`dev-mode.not-propagated`](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.not-propagated) (`:90`).
- Question: when a parent needs another version of a dependency that is installed in developer mode, should IPM replace it and clear its developer mode without notice?
- Type: policy disagreement between sources. No source addresses dependencies directly.
- Status: open. No provisional answer exists.
- Interpretations and authority:
  - A, refuse (or ask for `-force`), as for a top-level install. `wiki/00` L129: "A module loaded in dev mode cannot be overwritten by a non-dev install". Maintainer comment `Utils/Module.cls:1317`: "To debug a dependency, that module itself should be loaded in 'developer mode'", which is the setup this replacement destroys.
  - B, replace it in normal mode, as today. `wiki/00` L133: do not use `-dev` "for modules you are consuming as dependencies". `dev-mode.not-propagated`: dependencies install in normal mode.
  - C, replace it but warn and name the module whose developer mode was cleared.
- Product behavior: B. N-R05-DEVUP: a dependency in developer mode at 1.0.0, required at 2.0.0, is replaced and its `DeveloperMode` set to 0. The guard (`Utils/Module.cls:109`) needs `params("Install")`, which `load` never sets (`Main.cls:2455-2460`) and `install` removes before the top module's activation (`Utils/Module.cls:318-319`), so it never sees a dependency. By code reading, `Root` also moves to the repository folder (`Utils/Module.cls:1298-1303`). N-R05-DEVSAME: at the required version the dependency is skipped and keeps developer mode.
- Checked in this follow-up: code, native pair above, #316, #952 and #1036 (none mentions it), git history of the guard (Q-03).
- Most reasonable assumption: none. The wiki's general guard rule favors A and the not-propagated rule favors B.
- Decision requested: A, extend the guard to dependencies, so the parent's load fails with the guard's message unless the developer uninstalls or forces. Or B, keep replacing and document it, preferably with the warning of C. This answer also completes Q-03.
- Findings affected: I3 (issue). Its entry asks this question.
- Stakes: developers debugging a dependency in place, as the code comment suggests. Under B, their next load of the parent silently switches that dependency to the repository version.

### Q-25: If a `NoJournal` dependency escapes, may its own dependencies be rolled back?

- Canonical: no ontology block yet. Source: follow-up L-4 (with L-3). Nearest block: Q-22 and [`load.outcome`](../ontology/features/load.g5.md#load.outcome) (`ontology/features/load.g5.md:145`).
- Question: if Q-22 is answered A, may a failed load leave the escaped `NoJournal` dependency installed while that dependency's own dependency is rolled back?
- Type: design choice, which follows from Q-22 A.
- Status: open. It does not arise under Q-22 B. The sources lean against A: `bugs_issues.md` files the broken closure as D21, supported under both options of Q-22.
- Interpretations and authority:
  - A, accept it and document it. `wiki/00` L155 already warns that `NoJournal` may leave "an inconsistent state" (stated for crashes).
  - B, keep the closure consistent: a `NoJournal` dependency's own dependencies load under its `NoJournal` too, so they also stay.
  - C, avoid the case by answering Q-22 with B.
  - Against A: [`uninstall.dependents`](../ontology/features/uninstall.g5.md#uninstall.dependents) (`ontology/features/uninstall.g5.md:20`) says "IPM prevents uninstalling a module that other installed modules depend on". `load.dependencies` loads dependencies before the module and fails the whole load when one fails. The D21 entry reads these as implying that an installed module has its dependencies.
- Product behavior: N-R05-NESTF, chain m -> d1 (own `NoJournal`) -> d2, parent fails: d1 stays installed with its class and d2 is rolled back. Neither dependency has a record (D1). Mechanism: the top-level `LoadDependencies` loads the whole closure in dependency order with the top's parameters, so d2 loads before d1. d1's own `LoadDependencies` then skips d2 (`Utils/Module.cls:1368-1405`, `Storage/Module.cls:1139-1154`; L-3). The model shows that rerunning the same load without the fault installs every module (L-7).
- Checked in this follow-up: code, native NESTF and NESTS, model.
- Most reasonable assumption: not A. B or C keeps the namespace consistent and IPM's other rules protect dependency closures.
- Decision requested (only if Q-22 is A): A, document that a failed load can leave an escaped module without its dependencies until the next load. Or B, propagate a dependency's own `NoJournal` to the dependencies it needs.
- Findings affected: D21 (supported), D14.
- Stakes: users of applications that call into d2 through d1. Until a later load resolves d1's requirements again, that code fails at run time.

### Q-26: What `Committed` should a failed load's record show when only a `NoJournal` dependency's changes were kept?

- Canonical: no ontology block yet. Source: follow-up L-8. Nearest block: [`history.committed`](../ontology/features/history.g5.md#history.committed) (`ontology/features/history.g5.md:125`).
- Question: if Q-22 is answered A, should the top record of a failed load read `Committed=1` when the only changes kept belong to a dependency whose own `NoJournal` escaped the rollback?
- Type: design choice. The sources define `Committed` per action and do not say whether a parent's action includes its dependencies' writes.
- Status: open. It does not arise under Q-22 B.
- Interpretations and authority:
  - A, yes. Property comment `General/AbstractHistory.cls:41`: "Whether any of the action's changes were kept". `wiki/09` L215-219: the recipe for "an install left the namespace in a bad state" looks for committed failures. Today the dependency's own record is lost (D1), so the top record is the only one left. The follow-up model's contract uses this reading ("top: any module").
  - B, no. The top record covers its own module and the dependency's own record (once D1 is fixed) shows `Committed=1`. `history.dependencies`: dependency installs have their own records.
- Product behavior: the top record reads `Committed=0` (N-R05-NESTF row 1, archived M5). In the model this happens in 30 end states. It also appears in the guarded D1 repair variant. Only the one-plan-per-load variant (M-GLOBAL-MODE) removes it.
- Checked in this follow-up: model and native NESTF. #1237 does not discuss dependencies.
- Most reasonable assumption: no source. B is consistent with per-action records but depends on fixing D1 first.
- Decision requested (only if Q-22 is A): A, mark the top record committed when any dependency's changes were kept. Or B, keep it per module and rely on the dependency's own record.
- Findings affected: D22 (conditional on Q-22, read with option A of this question; under B it reduces to D1), D14, D1, Q-30.
- Stakes: administrators using the `wiki/09` recipe after a failed install. Under the current product, nothing in the log says that anything was kept.

### Q-27: Should `history delete -globally` report errors other than missing permission?

- Canonical: no ontology block yet. Source: follow-up H-4. Nearest blocks: [`history.delete`](../ontology/features/history.g5.md#history.delete) (`ontology/features/history.g5.md:253`) and the Q-12 D block.
- Question: when a global delete fails in one namespace for a reason other than missing permission, should it report that namespace's error instead of skipping it silently?
- Type: design choice. The stated intent points one way. Second type: missing test evidence, because the follow-up container has one IPM namespace.
- Status: open. The product departs from its own stated intent.
- Interpretations and authority:
  - A, report non-permission errors per namespace and return an error after trying every namespace. Code comments `General/History.cls:213` ("handle permission errors in locked-down namespaces") and `:221` ("Ignore permission errors in locked namespaces") and the verbose message "Skipped <ns> (no permission)". `wiki/09` L195: `-verbose` shows "which namespaces were skipped for lack of permission". The local delete already fails on an SQL error (`General/History.cls:151-159`). Q-10's provisional answer: never hide a failed query.
  - B, keep skipping every failing namespace and document it.
- Product behavior: B. The `catch` ignores every exception (`General/History.cls:214-225`). By code reading and in the readers model, `history delete -globally` with an unusable column prints "Deleted 0 record(s)" with no error while the local delete fails (52 global-delete commands in the model). Not run natively.
- Checked in this follow-up: `git log -S'DeleteHistoryGlobally'` and `git show f522900e`. The catch-all came with #1107 (isc-dchui, 2026-04-23), commit "Fix cleanup timing and history deletion permission bugs". The #1107 page does not describe the bug. The purge calls the same method (`General/History.cls:175`), so under A a purge error would surface, which is Q-13.
- Most reasonable assumption: A, because both the comment and the wiki name only permission.
- Decision requested: A, catch only the permission error, report the others per namespace and return an error. Or B, document that `-globally` skips any namespace where the delete fails. With the D2 repair (column check before any query), the unusable-column case is refused up front, so A then matters for other errors such as locks or read-only databases.
- Findings affected: D23 (suspected; filed with no decision, from the code comment and the wiki), D2, D3, OBS-H12 (Q-13).
- Stakes: administrators who believe a global delete or purge removed old records everywhere.

### Q-28: Should the global history readers return each row's namespace and ID?

- Canonical: no ontology block yet. Sources: follow-up H-5, H-6 and OBS-H11. Nearest blocks: [`history.read.api`](../ontology/features/history.g5.md#history.read.api) (`ontology/features/history.g5.md:247`) and [`history.read.global`](../ontology/features/history.g5.md#history.read.global) (`:237`).
- Question: should the class query `GlobalRecords` return each row's namespace and record ID? Should `Records` return the ID column it declares?
- Type: design choice (API shape).
- Status: open. The `history details` part is settled: `wiki/09` L171 documents that IDs are unique only within a namespace and that `details` reads the current namespace.
- Interpretations and authority:
  - A, add `Namespace` and `ID` to `GlobalRecords` and make `Records`' declared ID column real. Issue #366 (isc-tleavitt) listed a "Namespace" field and asked for "a cross-namespace class query wrapper around that API to get all history at the instance level". PR #578 later stored history per namespace (`history.namespace` metadata), which removed the column. `wiki/09` L171: an ID "is only meaningful in the namespace it was listed under".
  - B, keep the documented shape (`wiki/09` L211 names the queries and their `ascend` and `limit` arguments only) and state that rows cannot be attributed.
- Product behavior: the `GlobalRecords` row has no namespace or ID column (`General/History.cls:233-235`) and the query returns the current namespace's rows once per namespace (D20). `Records` declares `CONTAINID = 1` while its first column is `Action` (`General/History.cls:26-28`, OBS-H11, not run). `history find -globally` lists rows under their namespace correctly (archived H10).
- Checked in this follow-up: code. Both queries came with #578 (`8a88d5bf`). No test calls either query (search of `tests/`). The #578 thread does not discuss these columns.
- Most reasonable assumption: A, because without it a fixed D20 still returns rows nobody can place.
- Decision requested: A, add the columns (an API change for SQL callers that select `*`). Or B, document the limits.
- Findings affected: I4 (the missing columns, aliases H-5 and OBS-H11), D20.
- Stakes: programs that audit history across namespaces, the persona of #366.

### Q-29: May the retention purge remove records of actions that have not ended?

- Canonical: no ontology block yet. Source: follow-up H-7. Nearest blocks: Q-20 and [`history.retention`](../ontology/features/history.g5.md#history.retention) (`ontology/features/history.g5.md:299`).
- Question: once the purge works (D3 fixed), may it delete records with no end time that started before the cutoff?
- Type: design choice.
- Status: open. Q-20's provisional answer allows it, but its reason ("deleting is an explicit user choice") does not apply to the automatic purge.
- Interpretations and authority:
  - A, purge by start time only. `wiki/09` L185: "It compares against `TimeStart`". `history.retention`: records "that started more than that many days ago". Q-20 provisional.
  - B, skip records whose action has not ended. `wiki/09` L229-233 uses unfinished records to find killed processes and N-P15 shows a running action's record disappears completely if removed.
- Product behavior: today the purge deletes nothing (D3). Its filter is `TimeStart` only (`General/History.cls:172-173`), so after the D3 fix it follows A. The readers model with D3 fixed (variant M-NO-QUOTES) purges the in-progress row.
- Checked in this follow-up: native N-R04-CUTOFF (the quoted cutoff selects 0 rows, the unquoted one selects the 40-day-old row; H-2). The readers model's purge contract defaults to B (`followup/models/history_readers.py:226`, `qa="preserve-active"`), so its one purge verdict for M-NO-QUOTES depends on this question. #977 and #992 do not discuss unfinished records.
- Most reasonable assumption: A. Records of crashed actions would otherwise never age out and a running action older than the retention window is rare.
- Decision requested: A, purge by age and document it. Or B, add "has ended" to the purge filter and say how records of crashed actions are removed.
- Findings affected: D3 (its fix makes this reachable and its entry names Q-20 for it).
- Stakes: low frequency. A very long or hung action loses its record when the daily purge runs.

### Q-30: How should D1 be repaired?

- Canonical: no ontology block yet. Sources: follow-up L-9, L-10, L-11 and L-12, `bugs_issues.md` D1 repair proposal, `design_simplification.md` S2 and S8. Nearest block: [`history.survives-rollback`](../ontology/features/history.g5.md#history.survives-rollback) (`ontology/features/history.g5.md:67`).
- Question: which repair design should make dependency records survive a parent's rollback (D1) while their `Committed` stays true?
- Type: design choice for the maintainers. Second type: unknown platform behavior, because history rows written with journaling off do not reach mirror members and mirroring was never tested.
- Status: open.
- What the follow-up established, so the choice is between designs that work:
  - The proposed mechanism (variant 103) writes the row with journaling off and marks `Committed=1` with a journaled update as the row's last write before the commit. That rule is load-bearing: a rollback restores a journaled write only if the row still holds the value that write set (new premise P-14, native N-P21A and N-P21R, `FR/native/20261113T234716Z-p21-ordering-r2`). A mutant that writes the row unjournaled between mark and commit is detected (1,004 against 962 `Committed` violations).
  - A naive fix that only makes the row survive marks rolled-back dependencies `Committed=1` through `Finalize`'s "`Committed` or OK" (`General/AbstractHistory.cls:127-133`) and the no-transaction marker (2,712 violations, L-10). So `design_simplification.md` S3 is part of any repair.
  - Variant 103 as proposed has two gaps. With a dependency's own `NoJournal` default (Q-22 A), its surviving row reads `Committed=0` although its changes were kept (818 dependency-clause violations, L-9). At a Terminate between the mark and its commit, IRIS runs the top-level `%OnClose` first (premise P-06b). Finalizing dependency rows there unjournaled keeps the mark after the rollback (60 end states, L-12).
  - The guarded variant finalizes dependency rows at the close only when no transaction of the load is open. It leaves 141 violations, all with a dependency's own `NoJournal` default. Part of that drop comes from rows left unfinalized at such a Terminate, which is what Q-21's provisional answer requires.
  - One transaction plan per load (variant M-GLOBAL-MODE, S8) has no `Committed` violation under any option of Q-22 and Q-23.
  - In every repair variant, the top record is still lost under a caller's rollback, because variant 103 changes only dependency rows. That part is Q-05.
- Options:
  - A, one transaction plan per load (S8) plus variant 103 with the guarded close, the ordering rule and S3. The repair then runs in the domain where it was proved. It needs Q-22 and Q-23 answered as one rule.
  - B, keep per-activation modes (Q-22 A) and extend variant 103: a dependency whose own `NoJournal` applies marks its row committed at its own close, plus the guarded close, the ordering rule and S3. No model variant covers this extension yet.
- Decision requested: A or B. In both cases also confirm three things. First, IPM may write history rows with journaling off. Such rows also survive a caller's rollback (Q-05) and do not replicate to mirror members. Second, the ordering rule and the guarded close are part of the repair. Third, the naive fix is rejected.
- Findings affected: D1. Through the same mechanism (S2) also D10, D12, D22, D25, D26 and D27. The handoff's statement that variant 103 "meets every dependency clause" in default and developer modes (`uplift/ipm-verus-20261113/handoff.md:80`) holds only with one mode per load.
- Stakes: everyone who relies on the log after a failed install with dependencies. A wrong repair turns "record lost" into "record says committed for a module that was rolled back", which is harder to notice.

### Q-31: May the no-transaction `Committed` marker move after all refusals?

- Canonical: no ontology block yet. Source: `bugs_issues.md` D12, whose decision field says the repair changes the D block `history.committed.marking`, "which needs maintainer agreement". Nearest blocks: D block [`history.committed.marking`](../ontology/features/history.g5.md#history.committed.marking) (`ontology/features/history.g5.md:136`) and [`history.committed`](../ontology/features/history.g5.md#history.committed) (`:125`).
- Question: should a `-DNoTransaction=1` or `-DNoJournal=1` load that is refused before it writes anything read `Committed=0`, which means setting the marker only after every refusal check?
- Type: policy disagreement between sources.
- Status: open.
- Interpretations and authority:
  - A, a refusal that wrote nothing reads uncommitted in every mode. Property comment `General/AbstractHistory.cls:41-42` ("A non-committed load, install or update wrote nothing or was rolled back"). Issue #1237: `Committed` "should reflect whether the action's changes were actually retained". The I block `history.committed` states it as its last bullet.
  - B, a no-transaction run reads committed once it starts. CHANGELOG 0.10.11 `#1237`: "one run with `-DNoTransaction=1` or `-DNoJournal=1` reads committed". PR #1322 body: it marks "before writing when there is no transaction".
- Product behavior: B. The marker runs at `Utils/Module.cls:1224-1227`, before the manifest import (`:1251`), so a later refusal reads `Committed=1` with nothing kept (D12, model case B1-C08, `RN/20261113T193952Z-replay-return-b`).
- Checked in this follow-up: the #1237 and #1322 text. Neither mentions refusals. The evaluator rerun kept the D12 verdict (`FR/20261113T232332Z-replay-reeval`).
- Most reasonable assumption: A, because the CHANGELOG sentence describes failures that ran and S1 (validate before the first write) keeps the marker "before writing" while moving every refusal ahead of it.
- Decision requested: A, validate first (S1) so the marker follows every refusal and refine the D block to "after validation, before the first write". Or B, keep the marker where it is and withdraw D12 as documented behavior.
- Findings affected: D12 (filed as supported; under B it would be withdrawn), D11 (moving the marker alone is unsafe until D11 is fixed), D4, Q-17.
- Stakes: users of `-DNoTransaction=1` (also when set as a default modifier, D19). A refused load shows up in the "committed failures" recipe although nothing changed.

## Historical files and statuses that lag the final evidence

Files under `.g5/uplift/ipm-verus-20261113/` are the record of the original uplift and are not edited. These statements in them lag what this follow-up found:

- `handoff.md:59`, `:86` and `:98`: Q-23 "not observed natively". It is now observed (N-R05-Q23F and N-R05-Q23S, L-2).
- `handoff.md:67`: the collapsed row says "no current defect verdict" for eleven questions. The product departs from the provisional answers of Q-09, Q-13 and Q-16 by code reading and of Q-10 natively (E3, N-R04-UNKNOWN). Q-06 is missing from the table.
- `handoff.md:72-76`: history readers, per-module `<Defaults>` flags, nested dependencies and dependencies already installed at another version or in developer mode are "not modeled". They are now modeled (`followup/models/loadtx.py`, `followup/models/history_readers.py`) and run natively (R05 and R04 cases).
- `handoff.md:80`: variant 103 "meets every dependency clause" in default and developer modes. True only with one mode per load (L-9) and with the guarded close (L-12).
- `handoff.md:96`: the repair's write ordering "was not probed natively". It is now probed (P-14, L-11).
- `handoff.md:14`: lists premises P-01 to P-11 and P-12 but omits P-13 (E-5).
- `A/coverage-map.md:238`: "Q-01 to Q-19 need maintainer answers. Q-02, Q-17, Q-10 and Q-06 change required outcomes most." Q-12 is closed and Q-20 to Q-23 are missing. The ranking is superseded by the ordering above.
- `A/coverage-map.md:223` and `:257`: Q-23's "NoTransaction variant not run". Now run.
- `A/coverage-map.md:6`: attributes Q-21 to Q-23 to round 1. Q-22 and Q-23 come from `C/round-2/repairs-ontology.md`.
- `A/intent-inventory.md:190-206`: A1 provisional answers later changed for Q-03, Q-05, Q-06, Q-08, Q-09, Q-12, Q-13 and Q-15 (see the crosswalk).
- `B/model-notes.md:5` and `:150`: per-module `<Defaults>` transaction flags "not representable". The follow-up model represents them.
- `B/native-report.md:216`: D6 as a plain confirmed defect, while `bugs_issues.md` files it as conditional on Q-14.

Files outside that folder that other owners may update:

- `ontology/concepts/transactions-and-dev-mode.g5.md:184`: "The `NoTransaction` variant of stage C case M5 was not run natively." Now run.
- The ontology has no blocks for Q-24 to Q-31.
- The original `bugs_issues.md` (git `ae619795`) labeled Q-21 "new Q (round 1)", had no Q-23 row and said the effect of later unjournaled updates "was not probed". The rewritten file replaces these with D27, D28 and the P-14 rule.

## Considered and not raised as questions

- D13's note "If maintainers consider an error after the final commit acceptable, record that as a Q". Not raised, because `load.outcome` already requires the returned status and the namespace to agree.
- I1 (text shown for an open phase) and I2 (temporary repository left after a process end). They have no governing requirement and stay issues in `bugs_issues.md`.
- L-6 (an installed higher version that does not satisfy the requirement is refused). It matches `deps.installed`.
- L-7 (a process end leaves exactly the writes made durable before it). This sharpens the uplift's contract. `wiki/00` L155 already warns about crashes without a transaction.
- E-5 (the `DependencyResolution` suite ran no effective test although IPM printed "1 passed"). A test-suite fact, not a policy question.

## Counts

- Sections: 22 ontology Q blocks, 1 resolved former Q (Q-12), 2 aliases (Q-06b, Q-FIN) and 8 new questions (Q-24 to Q-31).
- Resolved: 1 (Q-12, by `wiki/09` L195 and the help). Partly settled: the `details` part of Q-28 (`wiki/09` L171).
- Open: 30 (all 22 ontology questions and the 8 new ones).
  - By main type: policy disagreement between sources 12 (Q-01, Q-02, Q-03, Q-04, Q-11, Q-15, Q-17, Q-19, Q-22, Q-23, Q-24, Q-31). Design choice 17 (Q-05, Q-07, Q-08, Q-09, Q-10, Q-13, Q-14, Q-16, Q-18, Q-20, Q-21, Q-25, Q-26, Q-27, Q-28, Q-29, Q-30). Missing test evidence 1 (Q-06). Unknown platform behavior 0.
  - With a second type: missing test evidence for Q-04, Q-09, Q-11, Q-13, Q-16 and Q-27. Unknown platform behavior for Q-08 (IRISTEMP at restart) and Q-30 (mirroring).
- Cross-check with the rewritten `bugs_issues.md`: its 11 conditional findings (D4, D6, D14, D18, D22, D24, D25, D26, D27, D28, OBS-H7), its question-dependent suspected findings (D29, OBS-H12, OBS-H13) and issue I6 each name a question that has a section here. Issue I3 asks Q-24.
- Of the 22 ontology questions, the product already does the provisional answer in 10, so the decision only confirms it (Q-01, Q-02, Q-03, Q-04, Q-07, Q-11, Q-15, Q-19, Q-20, Q-22). It departs in 11 (Q-05, Q-08, Q-09, Q-10, Q-13, Q-14, Q-16, Q-17, Q-18, Q-21, Q-23). One cannot be observed until D1 is fixed (Q-06).
