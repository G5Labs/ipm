<!-- continuation-pointer:start -->
> **Continuation pointer (2026-11-16 Linux run).** Current: batch 5. Lane 38 (history renderers) is running as a delegated lane; lane 39 (module manifest document editing) is in progress in the main thread. Sealed: iteration30 `6122cdf1`, 31 `aab17e2c`, 32 `4641563d`, 33 `c74d9c41`, 34 `276b7229`, 37 `9f91469d`, 36 `3445c460`, 35 in the commit "Iteration35 seal" (see `git log`). Material outcome: 445/1216 locator members (+38 from 407); new defects C:D34-C:D42 (C:D39 and C:D40 High); questions C:Q24-C:Q27. Cost: native+formal runs under 30 s each, except about 4 min of replay compute for iteration35; delegated lanes ~2.1M tokens in total; main-thread tokens not available. Next: lanes 38 and 39, then the convergence test.
<!-- continuation-pointer:end -->

# IPM consolidation: final handoff

> Subsequent Verus extension: see [checked models, before/after measurements and residual gaps](extension/REPORT.md). The historical consolidation below remains preserved. The extension records current behavior reach155/1,216 and scoped Verus representation174/1,216 (audited inherited baseline100); all inherited models and the git index remain unchanged.

Plan: [PLAN.md](PLAN.md). IPM stays at `571e04a1` and unchanged. The playbook foundation is imported intact at
`playbook-uplift/` (staged, subtree `744959d5` = `fdd36690:playbook-uplift`); G stays in `.g5/`. New work: the
consolidated foundation in [`.g5/Verus_verification/consolidated/`](../../Verus_verification/consolidated/README.md),
coverage in [`coverage/`](coverage/), rounds in [`rounds/`](rounds/), findings in [BUGS.md](BUGS.md),
[DESIGN-SIMPLIFICATIONS.md](DESIGN-SIMPLIFICATIONS.md) and [QUESTIONS.md](QUESTIONS.md). Nothing is committed.

## Coverage result

Denominator: the frozen 1,216 members (`coverage/members.tsv`). Counting rule: [`coverage/count.py`](coverage/count.py)
(a member counts only with a source slice, its own model function, an as-built twin matching every claimed native
field, and a wrong variant of that member rejected by a native field; seeded controls pass).

| Measure | Value |
|---|---|
| Baseline B (both arms, regraded under the evidence rule) | 81 members (6.66%): G 23, P 64, 6 in both |
| Newly modeled | **68 members** (5.59 points): R1 13, R2 20, R3 13, R4 14, R5 8 |
| Lost | 0 |
| Net gain | **+68** against the target +61 (met, narrowly: see weaker credits below); the stretch +122 is not met |
| B plus new | 149 of 1,216 (12.25%) |
| Credited this run but already in B (reconfirmed, not counted) | 3 (`Main.LoadInternal`, `LoadModuleReference`, `Repo/Manager.SearchRepositoriesForModule`) |

By subsystem: General 48, Utils 8, Storage 5, Repo 3, Main 2, Lifecycle 1, StudioDocument 1. By family: semantic
versions 20, owners outside load 14, history records and display 13, attribute evaluator 9, environment
configuration 8, temporary repository 4. Operation families with a newly checked outcome include several outside
load/history and resolver/sync: install-time expression evaluation, `module-version`, the filesystem repository
cache and its lock, the `%ZLANG` routines, and environment files.

Separate measures, not to be added together:

- **Obligation coverage.** Native fields compared by the seven families: 574; wrong variants rejected: 125 of 127
  (survivors named in each round). New obligations include post-commit status at two call sites in three modes,
  dependency-level failure in normal and developer mode, two-resource partial compile, filtered delete with exact
  identities, purge marker, and the held-back downgrade case.
- **Native correspondence.** Every one of the 68 rests on a native IRIS observation on ARM64, IRIS 2026.1 build 234U,
  from a fresh install; receipts in `native/results/`.
- **Mechanical product-body proof.** 0 members. No ObjectScript body was checked mechanically in either arm or here.
  No new Verus proof was written in the consolidation; the inherited 263 (G) and 227 with 195 killed controls (P) were
  reproduced ([baseline/RECEIPTS.md](baseline/RECEIPTS.md)).

Weaker credits, stated so a reader can discount them: the five `SemanticVersion.With*` builders and EnvironmentConfig
`Clear` and `IsEmpty` (small methods whose effect is observed but narrow), `History.SetHistoryLastPurge` (only the
marker it writes is observed; its consequence is masked by P's F-HIST-6) and `History.DisplayQueryResult` (only a
cosmetic variant is rejected). **Without these nine the gain is +59, two short of the target.** The target is met
only if these narrow credits are accepted.

## Independent checks

- **Second reader, 16 stated members** ([rounds/independent-subset-check.md](rounds/independent-subset-check.md)),
  judged from source and raw logs before the round handoffs: 13 hold (10 clean, 3 weak: `SkipCreate`,
  `SetHistoryLastPurge`, `DisplayQueryResult`), 3 should not count. Dispositions: `WriteAfterInstallMessage` withdrawn
  (its rejecting fields belong to `%Evaluate` and `LoadNewModule`); LockManager `%OnNew` withdrawn (its only rejecting
  case, LK5, never opened a transaction because the terminal refuses `TSTART`; the rerun in a method shows the lock
  held inside the transaction, so the release mode is not distinguished); `ScanDirectory` kept on new evidence (the
  same tree fails with `Depth=0` and builds with `Depth=1`, run `20261114T034941Z-r4-lock-depth`). `SkipCreate`'s
  variant now respects the unique `Root` index and is rejected by TR4's status. The agreement figure covers these 16
  only.
- **Inherited checkers.** [round1/checker-calibration.md](round1/checker-calibration.md) found wrong observations
  accepted by G's three evaluators and P's explorers. [round1/checker-repairs.md](round1/checker-repairs.md) repairs
  G's (`native/evaluate.py`, `evaluate_replay.py`, `evaluate_suite.py`, `cases/expected-cases.json`, and the two
  mislabelled controls in saved verdicts): all eight calibrated G wrong observations are now rejected; 58 of 61 saved
  evaluations reproduce unchanged and the other 3 differ only by later expectation edits; stagec S4 becomes
  INCOMPLETE, which exposes an earlier vacuous pass. Spot-checked here: cases-a keeps A0 CONFORMS and A1 DEVIATES, and
  the open-transaction, surviving-row and duplicate-line variants are rejected. P's checkers stay as imported, with
  their gaps listed in the calibration report. Two cautions: the new `Package` checks in
  `expected-cases.json` were written after those runs (from the fixture definitions, not from observations), so the
  evaluator correctly reports them as not written before the run; and `evaluate.py` rewrites a run's `verdict.json` in
  place, so reruns belong in a copy (a spot-check here rewrote `cases-a/verdict.json`, which was restored with
  `git checkout`).
- **Twin errors found and fixed during the work**: environment case U9 predicted in a state the run never had; the
  invalid LK5; both are recorded in the round handoffs.

## What each round found

| Round | Distinction IPM makes that no model did | Wrong outcome an existing check accepted | Defects and questions |
|---|---|---|---|
| [R1](rounds/R1.md) | Temporary `.modules` repository lifecycle; which copy wins; expression evaluation; status after the final commit | G's ontology "`.modules` is searched first"; "compiled descriptor means usable"; G's `evaluate.py` accepting five wrong observations | C:D2-C:D5, C:Q1-C:Q2 |
| [R2](rounds/R2.md) | Which versions a requirement admits and how requirements combine | P's resolver premise D2 (exact algebra) and P's ontology example `>=2.0`; a dependency declared `<2` installs `2.5.0` | C:D6-C:D10, C:Q3 |
| [R3](rounds/R3.md) | What `history find`/`details` show; install/uninstall record fields; purge marker | An unfinished record reads as an error; a filter matching nothing deletes all (G:D2 on ARM64) | C:D12 |
| [R4](rounds/R4.md) | `module-version`, cache refresh under a held lock, `%ZLANG` rewrite; table of all 18 `TSTART` sites | "Unchanged version is skipped"; "a refresh under another process's lock succeeds" | C:D13, C:Q4-C:Q5 |
| [R5](rounds/R5.md) | Environment files across commands and at compile time | Held-back case both models get wrong: `2.0.0-beta` then `1.0.0-rc` is not refused | C:D14-C:D15, C:Q6 |

The highest-impact new defect is C:D6 (High): an operator on a partial version is dropped, so a dependency declared
`<2` installs `2.x`, and `>=1.2` refuses `1.3.0`.

## Proof boundaries and limits

- The twins are hand transcriptions checked on finite native fixtures. A match shows correspondence for the observed
  cases; it does not prove the ObjectScript for other inputs.
- Several abstractions are premises: module-name validity as "no whitespace", ICU replacement-text rules, the
  `%LockId` timeout, the fixture-specific expression grammar in `#{}`, and name-ordered resolution for the diamond only.
  The multicompile premise was checked natively (M1-M2).
- Not exercised: the archive post-commit site, a failing `CleanUp` of an existing repository, analytics (remote
  registry), rollback branches of `module-version` and the language-extension update, two IPM namespaces, concurrency
  beyond one lock holder, registry adapters, install-time resources (FileCopy, CPF, web applications).
- Policy questions for maintainers (carried from the plan, unchanged): dependency transaction flags, audit records
  under a caller's rollback, errors after the final commit, a busy update lock. New ones: C:Q1-C:Q6.

## Unmodeled dispositions (by area)

| Area | Disposition |
|---|---|
| Registry adapters (HTTP, ORAS, remote), publish | External boundary; P baseline evidence for selection; row-order premise without a creation-order run |
| Install-time resource processors (FileCopy, CPF, CSP/web apps, Python wheels) | Missing model; expression targets now owned by the attribute-evaluator family |
| ExtensionBase and source-control hooks | Missing intent (G gap, P exclusion) |
| Migration from ZPM | Lower priority; P's simulated fixture |
| ModuleSummary, inspection, test output formats | Lower priority (presentation); contract examples would suffice |
| SemanticVersion Index/Find (21) | Unresolved: not yet traced to a consequential reader |
| LogManager, Singleton internals, TempFileManager, Utils/File | Unresolved; reached through modeled callers but their own effects are not distinguished |

## Unresolved items from the plan's audit list

1 reproduced (RECEIPTS §2). 2-3: each of the 37 accepted links has a label in the graders' TSVs, which also resolve
the named false negatives. 4: seeded controls pass (`count.py --selftest`). 5: `TSTART` table done (R4); lock
acquisitions, external processes, file writes and object saves were not enumerated. 6: write sets not joined to
members. 7: only the frozen denominator is reported. 8: done for a stated subset of 16 (above).

## State left behind

- Nothing committed. Staged: the 869 `playbook-uplift/` files. Modified: seven G ontology pages, G's three native
  evaluators, `expected-cases.json` and three saved `verdict.json` control records (checker repairs). New, untracked:
  `.g5/uplift/consolidated/` and `.g5/Verus_verification/consolidated/`. IPM product files are unchanged.
- Container `ipm-consolidated-iris` is still running, with snapshot image `ipm-consolidated-iris:571e04a1-fresh`
  (`native/iris.sh reset` restores it). Stop it with `docker rm -f ipm-consolidated-iris` when no longer needed.
- Effort: about one hour by the container clock from the IPM install to the last native run, plus preparation and
  review. Delegated agents used about 1.9 million tokens (baseline graders 0.47M and 0.52M, checker calibration 0.37M,
  evaluator repair 0.31M, independent subset check 0.27M); the main thread's usage is not available here.
