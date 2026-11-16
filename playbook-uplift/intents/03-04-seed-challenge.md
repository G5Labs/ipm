# Seed challenge — 03 install/update and 04 dependencies

Seed: `docs/ipm-uplift/index.md` (G5-benchmark). Baseline ipm 571e04a1. Every verdict below is from reading source,
tests, changelog, commits and (via read-only `gh`) issues #549, #550, #961, #962 and PR #887 (the native checks were run later; see "Native check results" at the end).
The wiki was **not** read
(network limited to `gh` issue/PR views), so claims that rest on the wiki are marked as such. At writing time no IPM test and no IRIS
check was run. Paths are relative to the IPM checkout.

## Verdicts on seed claims

| Seed claim | Verdict | Evidence | Ontology |
|---|---|---|---|
| Install vs update: when a newer manifest declares `UpdatePackage`, an ordinary install over an existing version directs the operator to `update`. | Confirmed, refined | Guard `Utils/Module.cls#L1219`; test `InstallModule.cls#L99`; README L62. Refinements: fires only when the target is strictly newer (equal version reinstalls, L1213); applies to `load` too; also applies to each dependency load because `cmd=install` and `Force` pass down while `DeveloperMode` is reset (L306-L319). | INST-B27, B31, B32, B16 |
| `-force` allows a forward install that bypasses the steps. | Confirmed, refined | `InstallModule.cls#L105`. Refinement: the bypassed steps are **seeded** by Activate (`Lifecycle/Module.cls#L122`), so a later `update` never runs them. | INST-B28, B29 |
| The same flag still refuses a downgrade. | Confirmed, widened | `InstallModule.cls#L102`; the check (`Utils/Module.cls#L1214`) ignores every flag: `-dev` (`InstallModule.cls#L175`), `update` (`Update.cls#L198`) and even `-DPermitDowngrade=1`. | INST-B24, B26; DEP-B25 / F-DEP-1 |
| The implementation bypasses the forward-install guard in developer mode, an exception to reconcile with public docs. | Confirmed, refined (not a defect by itself) | Code `Utils/Module.cls#L1219` (`'DeveloperMode`). The bypass is asserted by `TestDevMode` (`InstallModule.cls#L178`); issue #550 asks for an override "for example in a dev environment". README L62 names only `-force`; wiki not read. Extra consequence found: a dev install neither runs nor seeds the steps, so a later update runs all of them. | INST-B30, B18, B55; DEC-INST-1, DEC-INST-2 |
| Four contrasting install cases at `InstallModule.cls#L90`. | Confirmed | `TestInstallForceFlag` L90-L109 (older/newer × with/without `-force`). `TestDevMode` (L163) is the matching `-dev` quartet. | INST-B24..B28 |
| Ordered update steps: module authors supply an ordered list of migration methods. | Confirmed, refined | `VersionBase.cls#L6`, `#L16`. Order is: class number (V1, V2, … numeric), then within a class the all-member list before the primary-only list, then author order (`Storage/Module.cls#L114-L139`, `#L181`). | INST-B56..B59 |
| Running `update` applies the registered steps for the version transition. | Refined | There is no from/to filter: update walks **every** `<UpdatePackage>.V<n>` class and runs each step with no successful record (`Storage/Module.cls#L114`, `#L190`). Correctness rests on install seeding. Once one step in a class must run, all later steps in that class run again (`#L189`; asserted `Update.cls#L353`). | INST-B55, B60, B61, B62 |
| The suite includes an interrupted update followed by a successful retry. | Confirmed, weak | `Update.cls#L130-L156`: the job is killed right after signalling, at an uncontrolled point; only the retry status is asserted (L152); "Assert it completed" (L154) has no assertion. | INST-B69; F-INST-3 |
| The retry assertion alone does not establish exactly-once effects. | Confirmed, strengthened | Code shows the opposite of exactly-once for some effects: a failed non-dev update rolls back the step records with everything else (`Utils/Module.cls#L1358`), so steps that worked rerun next time; non-transactional effects ($INCREMENT, files, external calls) survive rollback; a kill between the author method (`Storage/Module.cls#L240`) and the record save (L246) reruns that step. Dev-mode updates keep finished records (no transaction around phases, `Utils/Module.cls#L1338`). | INST-B63..B68; DEC-INST-5 |
| (Version-sensitive) The hook interface distinguishes all-member and primary-only methods; some cases skip mirrors; establish what is promised. | Refined; gap remains | `VersionBase.cls#L8`, `#L18` promise primary-only steps run "when the instance becomes the primary member"; no code does that; `ExecuteUpdateStep` only skips them on a non-primary member (`Storage/Module.cls#L228`). PR #887 lists "Support for mirrored updates" as out of scope. Tests: `Update.cls#L54` skips on mirror members; `#L235` returns silently. No mirror evidence at all. | INST-B72..B75 (B74 = N); F-INST-2; DEC-INST-7 |
| Dependency constraints: resolve the constraints contributed by active paths. | Confirmed | Requirements from all paths are joined with AND (`Storage/Module.cls#L1096-L1107`); scoped requirements are skipped unless their phase is active (`#L954`, `HasScope` L834). Tests `BuildDependencyGraph.cls#L175`, `#L195`. | DEP-B6, B15..B18 |
| Incompatible transitive constraints should identify the conflict. | Confirmed | `BuildDependencyGraph.cls#L352-L353`, `#L363-L366`; error lists constraint sources (`SemanticVersionExpression.And`). | DEP-B7, B8 |
| A phase-limited dependency can introduce a conflict when that phase runs and leave another phase resolvable. | Confirmed | `BuildDependencyGraph.cls#L216` (resolvable without test) and `#L240` (clash with test). | DEP-B19 |
| Anchor: the "0.10.8 correction". | Refined | 0.10.8 (#1192, #1077; commit dfdda468) fixed clashes being **silently dropped** for fuzzy requirements and exact-vs-range intersection, and improved the error text. Loading scoped dependencies in their phase is 0.10.6 #1072. A test note at `BuildDependencyGraph.cls#L232` still calls exact-vs-range a known edge case, which that same commit fixed. | DEP-B7, B9; F-DEP-5 |
| (New) The resolver backtracks. | Contradicted in part | Method comment (`Storage/Module.cls#L867`) says "with backtracking"; code checks a candidate's direct requirements only and otherwise keeps the last candidate (`#L1248-L1291`). It can fail on solvable inputs. | DEP-B12, B13; DEC-DEP-1 |
| Coordinated dependency updates: two branches requiring the same shared dependency to advance together should succeed. | Confirmed, refined | Fix 2aba9825 (PR #1136, issue #430) adds `CoUpdatingModules` so installed requirements of modules in the same batch are ignored (`Storage/Module.cls#L1112`, `Utils/Module.cls#L1397-L1402`). Tests `Update.cls#L657-L663` (simple diamond) and `#L692-L698` (offset diamond), both via `update -path` from folders, not registries. An installed module outside the batch still blocks (`Update.cls#L769`). Rollback of already-moved modules after a failure is not asserted. | DEP-B27..B31 |
| (Brief) Requested version must satisfy other installed modules' requirements. | Confirmed | Top-level check `Utils/Module.cls#L116-L125`; resolver check `Storage/Module.cls#L1109-L1128`; test `Update.cls#L769`, `BuildDependencyGraph.cls#L104`. The top-level error message has no test. | DEP-B22, B23 |
| Lock-file generation: `install -create-lockfile` records resolved module, dependency versions and source repositories in `module-lock.json` under the module root; tests compare generated with expected. | Confirmed, refined | Writer `LockFile.cls#L29-L91`, trigger `Utils/Module.cls#L1416`; tests `LockFile.cls#L348`, `#L354`. Refinements: written before the module's own Activate and outside the transaction (stays on failure); a write error is only printed; a needed module with no registry (loaded from a folder) makes it fail; repository credentials are not exported; dependency order in the file is by **name**, not dependency order as documented. | DEP-B32..B42; F-DEP-2; DEC-DEP-2, DEC-DEP-3 |
| Reinstalling from a lock file is a separate claim; the out-of-order case has an empty body and a TODO for a future `ci`. | Confirmed | `LockFile.cls#L314-L317`; no `ci` command in `Main.cls`; issue #962 open; class comment `LockFile.cls#L2` claims both. Tagged N. | DEP-B43 |

## Behaviours the seed did not cover (mined here)

- Install will not replace a module in developer mode without `-dev` (`Utils/Module.cls#L109`); this blocks `reinstall`
  too, does not block `update`, does not protect a dev-mode **dependency**, and is lifted by `-DPermitDowngrade=1`.
  INST-B33..B38, DEC-INST-3.
- Update returns success without running steps if the update lock is busy. F-INST-1, DEC-INST-6.
- A module first brought in by an update runs its whole step history instead of being seeded. INST-B53, DEC-INST-8.
- `reinstall` help disagrees with itself (installed vs latest version). INST-B39, DEC-INST-4.
- Registry prefix `registry/name` limits only the top module; dependencies may come from any enabled registry.
  INST-B8, B9.
- Platform filtering: an unversioned install silently picks an older compatible build; a dependency never falls back
  to an incompatible build. INST-B10..B12.
- `list-dependents` is case-sensitive against lower-cased names, so its own help example finds nothing. F-DEP-3.
- `PermitDowngrade` relaxes the resolver but the loader still refuses. F-DEP-1.

## Candidate native checks

All run in a scratch namespace with the IPM test fixtures mounted as in CI (`/home/irisowner/zpm/tests/...`). Expected
outcomes are what this revision's code should do by inspection; a different outcome is a finding against the ontology.

| # | Targets | Experiment (ObjectScript / zpm shell) | Expected outcome |
|---|---|---|---|
| 1 | INST-B17, B28, B29 | `repo -n u -fs -path .../_data/update-test/`; `install update-simple 2.1.1`; `install -force update-simple 3.5.0`; read `##class(%IPM.Storage.UpdateStep.PrimaryOnly).GetUpdateStep("UpdatePackage.V3","MethodA")`. | Install OK; V3.MethodA has `TimeStampEnd'=""` and `TimeStampStart=""` (seeded, not run). |
| 2 | INST-B30, B18, B55; DEC-INST-1/2 | `install update-simple 2.1.1`; `install -dev update-simple 3.5.0`; read V3.MethodA record; then `update update-simple 3.5.0` and read V1/V2/V3 records. | Dev install OK; V3 record has no `TimeStampEnd`. After update: V3 steps have `TimeStampStart` set (ran). Repeat from a fresh `install -dev update-simple 1.2.1` then `update ... 2.1.1`: V1 steps also run. |
| 3 | INST-B71; F-INST-1 | `install update-simple 1.2.1`; in a JOB: `lock +^IPM.UpdateLock("update-simple")` and hang 60; in the main process: `set sc=##class(%IPM.Main).Shell("update update-simple 2.1.1")`. | `sc` is OK; module version is 2.1.1; V2 step records have empty `TimeStampStart` (no step ran). |
| 4 | INST-B64..B67; DEC-INST-5 | Fixture module M 1.0.0 → 2.0.0 with `M.Upd.V2` steps: `A` does `set ^Mx=$get(^Mx)+1, ^My=$increment(^My)` and appends a line to a file; `B` throws. `update M 2.0.0` (fails); read `^Mx`, `^My`, file, M's version, step A record. Fix B (republish 2.0.0), `update M 2.0.0` again. | First run: error; version 1.0.0; `^Mx` undefined (rolled back); `^My=1` and file has 1 line (not rolled back); no A record. Second run: OK; `^Mx=1`, `^My=2`, file has 2 lines (A ran twice). With `-dev` on both runs: second run skips A. |
| 5 | INST-B65; F-INST-3 | `install update-step-errors 1.2.1`; `update update-step-errors 2.1.1`; read version and V2 step records. | Error; version stays 1.2.1; no V2 record with `TimeStampEnd`. |
| 6 | INST-B53; DEC-INST-8 | `install update-deps-added 1.2.1` (no deps); `update update-deps-added 2.1.1` (adds `update-simple ^1.0.0`); read `UpdatePackage.V1` step records of `update-simple`. | `TimeStampStart` set on every V1 step (they ran); a plain `install update-simple 1.2.1` in a clean namespace instead leaves them seeded. |
| 7 | INST-B33..B37 | `load -dev <dir of update-simple 1.2.1>` with the same version also in registry `u`; then `install update-simple 1.2.1`; `reinstall update-simple`; `reinstall -dev update-simple`; `install -DPermitDowngrade=1 update-simple 1.2.1`; `update update-simple 2.1.1`. | install and reinstall fail with "Cannot install 'update-simple' over previously installed in developer mode"; `reinstall -dev`, the PermitDowngrade install and update succeed. |
| 8 | INST-B35; DEC-INST-3 | `load -dev .../build-dependency-graph/complex-c/2.0.0`; `install complex-b 2.0.0` from the `build-dependency-graph` fs repo (needs complex-c ^3.0.0). | Install succeeds; complex-c is 3.0.0 with `DeveloperMode=0`; no refusal. |
| 9 | INST-B39; DEC-INST-4 | Registry with X 1.0.0 and 1.1.0; `install X 1.0.0`; `reinstall X`. | X stays 1.0.0 (installed version, not latest). |
| 10 | INST-B8, B9 | Two fs registries r1 (sort first) and r2 both hold X 1.0.0; r2 also holds P (needs X) and r1 does not. `install r2/P`; read `Repository` of P and X. | P from r2; X from r1 (dependencies are not limited to r2). |
| 11 | DEP-B13; DEC-DEP-1 | fs registry: root 1.0.0 needs A ^1 and B *; A 1.1.0 needs C ^2; A 1.0.0 needs C ^1; B 1.0.0 needs C ^1; C 1.0.0 and 2.0.0 exist. `install root`. | Fails with a clash on C (A 1.1.0 chosen greedily) although A 1.0.0 + C 1.0.0 would satisfy all requirements. |
| 12 | DEP-B25; F-DEP-1 | `load .../complex-c/3.0.0`; `load .../complex-a/1.0.0 -DPermitDowngrade=1` (needs complex-c ^2.0.0). | Resolution picks complex-c 2.0.0, then the load fails with "Cannot downgrade complex-c from version 3.0.0 to 2.0.0". |
| 13 | DEP-B39; F-DEP-2 | fs registry: `zz-base` 1.0.0; `aa-mid` 1.0.0 needs zz-base; `root` needs aa-mid. `install -create-lockfile root`; read the `dependencies` keys of `module-lock.json` in order. | Keys are `aa-mid`, `zz-base` (by name), so a dependent comes before what it needs. |
| 14 | DEP-B41; DEC-DEP-3 | `load .../lock-test/mods-base-cases/lock-mod-a-no-deps`; then `install -create-lockfile lock-mod-e-1-dep-0-transient` from `lock-file-base`. | Install succeeds; output has "Error creating lock file ... Tried getting repo """; no new `module-lock.json` written. |
| 15 | DEP-B40; DEC-DEP-2 | Module whose Activate fails (e.g. an `<Invoke>` that throws) with one dependency; `install -create-lockfile` it. | Install fails and rolls back; `module-lock.json` exists in its root. |
| 16 | DEP-B44, B49; F-DEP-3 | `install update-deps 2.1.1` (needs update-simple); `list-dependents update-simple`; `list-dependents Update-Simple`. | Lower case lists update-deps; mixed case prints nothing. |
| 17 | DEP-B17, B19 | `load .../build-dependency-graph/scoped-root/2.0.0` (install succeeds), then `scoped-root test -only`. | Load OK; test phase fails with a clash naming loop-c. |
| 18 | DEP-B23 | `install complex-b 1.0.0` (needs complex-c ^2.0.0, installs 2.0.0); then `install complex-c 3.0.0`. | Fails with "Requested version (complex-c 3.0.0) does not satisfy the requirements of other modules installed in the current namespace (complex-b: ^2.0.0)". |
| 19 | DEP-B31 | Run `TestDependencyVersionsUpdated` steps up to the failing update at `Update.cls#L768`, then read module-a/b/c versions. | All three still at their pre-update versions. |
| 20 | INST-B72..B74 | Needs a two-member mirror; not possible with the single `ipm-uplift-iris` container. | Gap: record as not checkable here. |

## Native check results

Run on IRIS 2026.1 (build 234U), container `ipm-uplift-iris-inst`, IPM 0.10.11-SNAPSHOT at 571e04a1, namespace USER,
each check from a clean namespace (`Uplift.Inst.Checks:Reset`). Code `validation/iris/inst/Uplift/Inst/Checks.cls`,
fixtures `validation/iris/inst/fixtures/`, scripts `validation/iris/inst/6*.script`, logs
`validation/logs/6*-inst-*.log`. The "Expected outcome" column above was a prediction and is left as written; any
difference is stated here. Fixture note: the `update-simple` versions move methods between the all-member and
primary-only lists, and step records are kept per list, so the checks read every step row rather than fixed names.

| # | Result | Observations |
|---|---|---|
| 1 | As predicted. Plain install 2.1.1 → 3.5.0 is refused with the "use update" message; `-force` to 1.2.1 is refused (downgrade); `-force` to 3.5.0 works and every step row, V3.MethodA included, is seeded (end set, start empty). | nc-inst-1-force-install-seeds |
| 2 | As predicted, with detail: after `install -dev 3.5.0` the V3 step has no row at all (not a row without an end time). The update to 3.5.0 ran V3.MethodA, the V1 steps that had no row, and a seeded V1 step after them in the same list; it also cleared developer mode. Fresh `install -dev 1.2.1` left 0 rows; `update 2.1.1` then ran every V1 and V2 step. | nc-inst-2-dev-install-then-update, nc-inst-2b-dev-fresh-then-update |
| 3 | As predicted. Update returned OK after 3.3 s with no message, module at 2.1.1, no V2 row created. Once the lock was free, a second update to 2.1.1 ran the V2 steps. | nc-inst-3-update-lock-busy, nc-inst-3b-update-after-lock-free |
| 4 | As predicted for all four runs. Extra (4e): `update -dev` from a filesystem registry does not set developer mode (new finding F-INST-5; by inspection the same for other registries); the dev runs here used `-path`. | nc-inst-4-nondev-failed-update, nc-inst-4b-nondev-retry, nc-inst-4c-dev-failed-update, nc-inst-4d-dev-retry, nc-inst-4e-dev-update-from-registry |
| 5 | As predicted. Error from V2.MethodB; version stays 1.2.1; no V2 row. | nc-inst-5-failed-step-update |
| 6 | As predicted. The V1 step of update-simple brought in by the update ran; a plain install of update-simple 1.2.1 seeds it. | nc-inst-6-dep-added-by-update, nc-inst-6b-fresh-install-control |
| 7 | As predicted. Also: `install -force` does not lift the guard; the PermitDowngrade install and the plain update both clear developer mode; `update -DPermitDowngrade=1` is rejected. | nc-inst-7-dev-mode-guard |
| 8 | As predicted. complex-c 2.0.0 (dev) became 3.0.0 without developer mode, no message. | nc-inst-8-dev-dependency-replaced |
| 9 | As predicted (with complex-c 2.0.0 and 3.0.0 as the two versions). Reinstall kept 2.0.0. | nc-inst-9-reinstall-version |
| 10 | As predicted: P from nc-inst-r2, X from nc-inst-r1. Extra (10b, 10c): when two registries tie on version, the one created first wins whatever its sort order (new finding F-INST-4 against INST-B4). | nc-inst-10-registry-prefix, nc-inst-10b-registry-tie-break, nc-inst-10c-registry-tie-break-high |
| 11 | As predicted. Clash on nc-dep-c naming nc-dep-a 1.1.0; nothing installed. Control: with nc-dep-a 1.0.0 installed first the same install succeeds. | nc-dep-11-greedy-resolution, nc-dep-11b-solvable-with-pin |
| 12 | As predicted. Control without the flag fails earlier, in resolution ("downgrade not permitted"). | nc-dep-12-permit-downgrade |
| 13 | As predicted. Keys `nc-dep-aa-mid`, `nc-dep-zz-base`; the install itself loaded zz-base first. | nc-dep-13-lockfile-order |
| 14 | As predicted. The fixture folder already holds a committed module-lock.json; its size, hash and time did not change. | nc-dep-14-lockfile-folder-dependency |
| 15 | As predicted. Both modules rolled back; module-lock.json (listing nc-dep-lf-base) left in the folder. | nc-dep-15-lockfile-failed-install |
| 16 | As predicted. | nc-dep-16-list-dependents-case |
| 17 | As predicted. The error names loop-c with both constraints; loop-c stays 1.0.0. | nc-dep-17-scoped-clash |
| 18 | As predicted, message word for word. complex-c stays 2.0.0. | nc-dep-18-installed-requirements |
| 19 | As predicted (all three at pre-update versions), but differs in what it shows: both prescribed failures happen before any module moves (graph clash; module-c manifest refused), so they do not exercise rollback. Added 19d: a dependency moved, then the parent's Activate failed: the dependency went back. 19e (`update -dev -path`): the dependency stayed moved. 19f (`update -dev` from the registry): rolled back like 19d (F-INST-5). 19c: the prescribed final update moves all three. | nc-dep-19-before, nc-dep-19-blocked-update, nc-dep-19b-failed-midway, nc-dep-19c-coordinated-update, nc-dep-19d-rollback-after-dep-moved, nc-dep-19e-dev-path-update-dep-kept, nc-dep-19f-dev-registry-update |
| 20 | Not checkable: single non-mirrored instance. | nc-inst-20-no-mirror |

IPM tests run here (log `validation/logs/64-inst-ipm-tests.log`): `BuildDependencyGraph` (whole class), `DepNoVersion`,
14 `Update` methods (all but `TestInterchangeableNameUpdate`, which publishes to the shared ORAS registry under
un-prefixed names), and LockFile Test01–04, 06, 07, 09, 10. Not run: `InstallModule` (its `OnAfterAllTests`
unpublishes un-prefixed names from the shared ORAS registry), LockFile Test05 (needs the `registry` service, not running
here) and the empty Test08. Results are in `evidence/ipm-tests.json`.

## Review fix checks (stored developer mode)

Run for reviewer finding 7 (`review/REVIEW.md`) on the same container, from a clean namespace. Code
`validation/iris/inst/Uplift/Inst/FixChecks.cls`, script `validation/iris/inst/66-inst-fix-stored-dev.script`, log
`validation/logs/66-inst-fix-stored-dev.log`. Prediction (from `Utils/Module.cls#L1298-L1307`, `#L1219` and
`Lifecycle/Module.cls#L122`, now modelled in `models/verus/update_steps.rs`): with no -dev / -nodev typed, a load or
`update -path` runs in the mode stored on the module, while the install/load guard reads only the typed flag.

| Check | Result | Observation |
|---|---|---|
| `load -dev` nc-inst-m 1.0.0, then a plain `load` of 1.0.0 | As predicted: 1.0.0/dev, 0 step rows (no seeding) | nc-inst-fix-stored-dev-plain-load |
| then a plain `load` of 2.0.0 (names an update package) | As predicted: refused with the "use the 'update' command" message; still 1.0.0/dev | nc-inst-fix-stored-dev-load-newer |
| then `update nc-inst-m -path 2.0.0-bad` (step B throws) | As predicted: error, not undone: 2.0.0/dev, V1.S1 and V2.A records kept, B failed, Mx=1 | nc-inst-fix-stored-dev-update-path-fails |
| then `update nc-inst-m -path 2.0.0-good` | As predicted: OK; only B ran (Mx=1, one file line) | nc-inst-fix-stored-dev-update-path-retry |

Both models replay all four (`inst-conf-nc-inst-fix-*` in `evidence/model-checks-inst.json`).
