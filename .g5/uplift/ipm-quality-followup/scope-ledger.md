# Scope ledger: load and history uplift, follow-up finding R08

This ledger answers audit finding R08 (`closure.md`, subcases R08a to R08d). It gives a disposition for every A1 commitment and catalog row, every A2 observation and command, and every coverage-map row that the ID registry marks partial, unmatched, unowned, without an operation, unestablished or not run. Section 2 traces the in-scope parts of load paths the formal model leaves out to their operations, commit points, records, readers and baseline evidence. Section 3 gives the counts. Section 4 lists what stays unestablished.

The formal scope stays package loading and transaction history. Nothing here proposes a whole-product proof.

## How to read this ledger

Inputs: `uplift/ipm-quality-followup/work/id-registry.json` (835 IDs) and its summary, the A1, A2 and A3 files under `uplift/ipm-verus-20261113/A/`, `Verus_verification/coverage-map.md`, the ontology, `uplift/ipm-quality-followup/work/followup-findings.md` (E-, L- and H- findings) and the follow-up receipts. Paths are relative to `.g5/`. Product locators are relative to `src/cls/IPM/` at pin `571e04a1` unless they name another root. `git diff 571e04a1 -- src/ preload/` is empty, so the working tree is the pin.

Abbreviations:

| Short form | Meaning |
|---|---|
| A2:n, A3:n | `uplift/ipm-verus-20261113/A/operation-inventory.md` and `A/coverage-map.md`, line n |
| VV:n | `Verus_verification/coverage-map.md`, line n |
| B1 | the original Verus model `Verus_verification/crates/ipm-load-history` (theorems, `case_*` lemmas, controls, V1 to V12) |
| `VR/<suffix>` | original native run `Verus_verification/results/native/20261113T<suffix>` |
| `FR/` | `Verus_verification/followup/results/` |
| `LT` | follow-up load model `Verus_verification/followup/models/loadtx.py`, current receipt `FR/20261114T002647Z-final-loadtx-model` |
| `HR` | follow-up history-reader model `Verus_verification/followup/models/history_readers.py`, receipt `FR/20261114T002647Z-final-history-readers-model` |
| `R05`, `R04`, `P21` | follow-up native runs `FR/native/20261113T234817Z-r05-modes-nesting`, `FR/native/20261113T234810Z-r04-history-readers`, `FR/native/20261113T234716Z-p21-ordering-r2` |
| D-, I- numbers | defects and issues in `Verus_verification/bugs_issues.md` |
| E-, L-, H- numbers | findings in `uplift/ipm-quality-followup/work/followup-findings.md` |

Scope class:

- formal: load, formal: history. The block is in the formal scope (A3 section 7 lists).
- in-scope part. The feature is outside the model, but this part of it changes what a load or the history log does.
- excluded feature. The whole feature is outside the formal model.
- structural. No behavior to govern.

Disposition. One value per row, chosen by these rules in order:

1. not run (reason). An open part needs a run that source reading cannot settle. None was made.
2. static review only. An open part rests on reading the source alone.
3. future or not implemented. An open part has no operation.
4. decision pending Q-nn. Every open part is established by a model or a run and waits only on a maintainer decision.
5. Otherwise the weakest evidence over the open parts: native or suite evidence only, modeled in original model (B1), or modeled in follow-up model (`LT`, `HR`). The follow-up label is used where the follow-up model is what covers the open part.

A row without an open part takes the weakest evidence over its content. For an in-scope part, that means its in-scope content. An excluded feature with no open part is static review only, because A1 to A3 only read it. A2 command rows have no status of their own. Their disposition names the evidence that exercises the command's in-scope part. Their open items sit on the OBS and coverage rows they cite.

Regression or scope exclusion:

- regression. A source shows the behavior used to work. The only one is D15: `git show f83b207e` replaces the two-piece test `$piece(path,".",*-1,*)="tar.gz"` with the one-piece `extension="tar.gz"` at `Main.cls:2466-2467`.
- scope exclusion. The open part, or the whole row when there is none, lies in a feature outside the formal model.
- n/a. Everything else, including confirmed defects in scope that never worked.

## 1. Dispositions

### 1a. A1 commitments (35)

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| INT-LOAD-01 | `load` from a folder, `.tgz`/`.tar.gz` archive or git URL, with dependencies | formal: load | static review only | Folder: B1 `case_c01_conforms`, native A0 (`VR/182759Z-cases-a`). `.tar.gz`: D15, stage C S1 (`VR/204152Z-stagec`), cause `Main.cls:2466-2467`. Git URL: section 2.2 | regression (`.tar.gz`, `f83b207e`) | D15 fix. Git URL native case not run. It needs an HTTP git remote. The follow-up environment runs only the IRIS and ORAS containers |
| INT-LOAD-02 | Empty and relative paths refused and a missing path fails | formal: load | native or suite evidence only | D17: stage C S4R, S4N1, S4N2 (`VR/204405Z-stagec2`). Check at `Main.cls:2445-2452`. B1 treats path validity as one input bit | n/a | D17 fix. No product test |
| INT-LOAD-03 | Archive unpacked to `<ModuleRoot>/<name>/<version>/` and temp folder removed | formal: load | static review only | OBS-L7: folder deleted before any version guard (`Main.cls:2507-2509`, `Utils/Module.cls:236-242`). `.tgz` unpack: suite FileBinaryTar (`VR/182337Z-suite-verify-FileBinaryTar`) | n/a | Q-04 decision. Ordinary native pair (archive refused by a version guard after its folder was replaced) not run |
| INT-LOAD-04 | `load` and `install` share one loading path | formal: load | static review only | Legacy XML bypass: section 2.3. Install and update: suites InstallModule, Update (`VR/182208Z-suite-verify-InstallModule`, `VR/182433Z-suite-verify-Update`) | n/a | Legacy XML native case not run, because it needs a registry serving a non-gzip payload (R05d deferred). Install and update paths not modeled |
| INT-LOAD-05 | `-dev` keeps entries on failure | formal: load | modeled in original model | B1 controls 01 and 02, `case_c26_dev_failure_keeps_phase_work`. Native A2, A5, C2. Developer-mode configurations in `LT` | n/a | None |
| INT-LOAD-06 | Developer mode commits the manifest before resources load | formal: load | decision pending Q-05 | C1 and C2 at `Utils/Module.cls:1309-1319`, `:1335-1339`. Q-05: B1 V8, native D1 caller case (`VR/184632Z-repro-caller-tx-history-lost`), caller-transaction configurations in `LT` | n/a | Q-05 decision |
| INT-LOAD-07 | Developer mode is not propagated to dependencies | formal: load | static review only | Rule: B1, native A4, suite LoadModuleWithDeps. OBS-L4 is code-read only (`Utils/Module.cls:258`, `:309-311`). Related new finding L-5: N-R05-DEVUP in `R05` | n/a | `update m -dev` from a registry not run (R05d deferred). L-5 has no governing requirement (section 4) |
| INT-LOAD-08 | Transactions, `NoTransaction`, `NoJournal`, flag sources | formal: load | static review only | Per-activation modes: `LT` (L-1 to L-3), N-R05-PARDEF, N-R05-Q23F, N-R05-Q23S in `R05`. D19: stage C X1. Non-database effects (Q-04): reading and A2 premise P-7 only | n/a | Q-04 decision. No native check of non-database effects |
| INT-LOAD-09 | Downgrade always refused | formal: load | modeled in original model | B1 `case_c11_downgrade_refused`, control 08. Native g6, g7 (`VR/182832Z-cases-dg`). Guard at `Utils/Module.cls:1213-1217` | n/a (`PermitDowngrade` bypass removed in `f83b207e`, see section 4) | Maintainer view on OBS-L14 |
| INT-LOAD-10 | Newer version with `UpdatePackage` refused unless `-force` | formal: load | decision pending Q-02 | B1 `case_c15_update_refused`, control 09, Q-02 parameter. Native g3, g4, g5 | n/a | Q-02 decision |
| INT-LOAD-11 | System requirements validated before installation | formal: load | decision pending Q-17 | D4: B1 V3, `case_c23_sysreq_dev_keeps_manifest`. Native B2-B4 (`VR/182820Z-cases-b`, `VR/184630Z-repro-sysreq-dev-manifest-left`) | n/a | Q-17 decision |
| INT-LOAD-12 | Developer-mode module not overwritten by a normal install | formal: load | static review only | Guard at `Utils/Module.cls:109-112` needs `params("Install")`. Top-level guard is code-read only. Dependencies are never guarded (L-5, N-R05-DEVUP and N-R05-DEVSAME in `R05`) | n/a | Q-03 decision. Native top-level `install` over a developer-mode module not run (ordinary) |
| INT-LOAD-13 | Read-only repository forces normal mode | formal: load | static review only | `Utils/Module.cls:1277-1288`. Not modeled, not run | n/a | Optional native case: `install m -dev` from a read-only repository |
| INT-LOAD-14 | Dependencies one at a time, `.modules` temporary repository | formal: load | native or suite evidence only | Order: N-R05-NESTF, N-R05-NESTS, N-R05-DIA (`R05`). D13: B1 `case_c29_error_after_commit`, replay `VR/193952Z-replay-return-b`. I2: `VR/210229Z-temp-repos-leftover`. Repository lifecycle not modeled (section 2.4) | n/a | D13 fix. I2 needs a governing requirement |
| INT-LOAD-15 | Other load options, lock file, `-nodev` | formal: load | static review only | Lock file: section 2.5. Q-01 consistent natively (g8, g9) | n/a | Native `-create-lockfile` cases (first plain load, rollback after the write) not run. `-env`, `-bypass-py-deps`, `-skip-sync` have no evidence. Q-01 decision |
| INT-LOAD-16 | Install and load check write permission first | formal: load | static review only | Section 2.1: check only at `Utils/Module.cls:228-233`, none on the `load` archive path `Main.cls:2505-2513` | n/a | Native case not run, because it needs an unwritable `ModuleRoot` (instance-wide setting plus container file permissions) |
| INT-LOAD-17 | Every `load` produces a history record | formal: history | modeled in original model | B1 `theorem_top_record_durable`. Record created at `Main.cls:2420`, before the path checks | n/a | None |
| INT-LOAD-18 | Same-version install or load succeeds | formal: load | modeled in original model | B1 installed-equal state. Native g2 | n/a | Product test (maintainers) |
| INT-HIST-01 | `install`, `load`, `uninstall`, `update` are recorded | formal: history | native or suite evidence only | Load: B1. Others: suite History (`VR/175623Z-smoke-history-suite-oras`). `reinstall` logs as `install` (`Main.cls:2698`) | n/a | None required. Non-load writers not modeled |
| INT-HIST-02 | Failed actions logged and their record survives the rollback | formal: history | modeled in original model | D1: B1 V1, V2. Native A1-A3 (`VR/182759Z-cases-a`, `VR/184617Z-repro-dep-history-lost`). Per-activation D1 in `LT` (L-2, L-4), N-R05-Q23F, N-R05-NESTF. Q-05: B1 V8, native D1 case | n/a | D1 fix. Repair 103 needs the guarded close (L-12) and holds only for a global mode (L-9). Q-05 decision |
| INT-HIST-03 | Record created at start and filled in at end | formal: history | modeled in original model | D10: B1 `kill_rec`, B1-C44..C48 T/K. Native C1, C3, C4R. Q-08: V9. Q-21: V10. PREFIX at process ends: `LT`, `FR/20261114T002830Z-final-prefix-cases` | n/a | D10 fix. Q-08, Q-21 decisions |
| INT-HIST-04 | Record fields | formal: history | native or suite evidence only | D9: B1 V7, native g3, g6, g7, R4. Q-15 label and source: native A0 only | n/a | Q-15 decision |
| INT-HIST-05 | `Committed` means changes kept | formal: history | static review only | Legacy XML (OBS-L16) code-read only (section 2.3). Q-05: V8, native D1 case. Q-17: D4. Q-07: no evidence. Dependency `Committed` per activation: L-8 to L-10 and L-12 in `LT`, P-14 in `P21` | n/a | Legacy XML native case not run (registry needed). Q-05, Q-07, Q-17 decisions |
| INT-HIST-06 | Uninstall `Committed` equals success | in-scope part | static review only | OBS-L17, OBS-H8: skip with OK at `Lifecycle/Base.cls:290-293`. No transaction in `Storage/Module.cls:727-758`. Plain uninstall records: suite History | n/a | Ordinary native case (uninstall a required test module without `-force`) not run |
| INT-HIST-07 | `Success` and `Committed` answer different questions | formal: history | modeled in original model | B1 failure-plus-commit cases. Native A5, B3, B4 | n/a | None |
| INT-HIST-08 | Dependency installs logged as their own records | formal: history | native or suite evidence only | D1: B1 V1, `LT`. Q-06: the record is lost (D1), so its `Success` cannot be observed. Q-15: native A0 only | n/a | D1 fix. Q-06, Q-15 decisions |
| INT-HIST-09 | When a record exists | formal: history | static review only | Refusals before the log: `update` at `Main.cls:4415-4424` before `:4438`, `reinstall` at `:2686-2688`, `uninstall` at `:2710-2712` before `Storage/Module.cls:734` | n/a | Q-09 decision. Ordinary native check not run |
| INT-HIST-10 | `history` actions, sort, limit, filters | formal: history | modeled in follow-up model | D2: `HR`, N-R04-SEL, N-R04-SEL2 in `R04`. D18: stage C H2, Q-18 commands in `HR`. Valid neighbor: 69 commands, 0 violations | n/a | D2 fix. Q-10, Q-18 decisions |
| INT-HIST-11 | `-globally` reads every namespace | formal: history | static review only | D20: stage C H10, GLOBAL clause in `HR`. OBS-H11 code-read only (`History.cls:28`, H-5) | n/a | Read-only native check of `Records` not run (ordinary). D20 fix |
| INT-HIST-12 | `history delete` deletes the filtered records | formal: history | decision pending Q-11, Q-20 | DEL clause in `HR`. N-R04-DEL in `R04`. Q-20: N-P15 (`VR/181129Z-premises`), QA option in `HR` | n/a | Q-11, Q-20 decisions |
| INT-HIST-13 | Retention purge deletes older records | formal: history | static review only | D3: N-R04-CUTOFF in `R04`, PURGE clause in `HR`, archived F1. D5: F0. A purge error stops the command: code-read only (`Main.cls:862-865`) | n/a | D3, D5 fixes. Q-13, Q-20 decisions. Purge failure not run, because no ordinary input makes it fail |
| INT-HIST-14 | Finalized records cannot be edited | formal: history | decision pending Q-21 | N-P13, B1 `theorem_r2`. Q-21: B1 V10, native C2 | n/a | Q-21 decision |
| INT-HIST-15 | History survives IPM's own uninstall | formal: history | not run: destructive self-uninstall, concurrent cleanup | `Storage/Module.cls:630-643`, `Lifecycle/Module.cls:51`. N-P06, N-P16, N-P17 (`VR/181129Z-premises`) | n/a | Run in a disposable namespace or instance. Q-16 decision |
| INT-HIST-16 | History is scoped per namespace | formal: history | native or suite evidence only | N-P06, N-P17. Suite History `TestMultipleNamespaces`. Row identity H-6 in `HR` | n/a | None |
| INT-HIST-17 | IRIS audit database proposed, not implemented | excluded feature | future or not implemented | A3:58, A3:130 | scope exclusion | None |

### 1b. A1 catalog (42)

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| CAT-01 | Module identity | excluded feature | static review only | A3:67, OP-LOAD-09 | scope exclusion | None. Manifest refusals during a load are on `load.refusal.manifest` |
| CAT-02 | Resources and scope | excluded feature | future or not implemented | A3:68 (`Scope="verify"` deferred) | scope exclusion | None |
| CAT-03 | Dependencies in the manifest | in-scope part | static review only | Load use: B1 (two leaf dependencies), nesting reduction in `LT`, `R05`. Phase-scoped dependencies: code and tests only (`Lifecycle/Base.cls:1261`, `:1557`) | scope exclusion | Document dependency `Scope` (maintainers) |
| CAT-04 | Defaults and parameters | in-scope part | modeled in follow-up model | Per-key `<Defaults>` merge (`Utils/Module.cls:1165-1169`) in `LT`. N-R05-PARDEF in `R05` | n/a | Q-22, Q-23 decisions. Placeholders and dotted names stay excluded |
| CAT-05 | System requirements | formal: load | decision pending Q-17 | As INT-LOAD-11 | n/a | Q-17 decision |
| CAT-06 | Invoke and custom phases | in-scope part | decision pending Q-14 | D6: B1 V5, `case_c28_phase_left_open`, `VR/184634Z-repro-activate-invoke-phase-open` | n/a | Q-14 decision |
| CAT-07 | Resource processors | excluded feature | static review only | OBS-L8 code-read (`Lifecycle/Base.cls:1081`). `<WSGIApplication>` has no processor | scope exclusion | OBS-L8 has its own row |
| CAT-08 | Python dependencies | excluded feature | static review only | A3:74. pip effects stay after a rollback (Q-04) | scope exclusion | Q-04 decision |
| CAT-09 | Update steps | excluded feature | static review only | OBS-L9, OBS-L5 code-read (A2:580, A2:593). Mirroring is future | scope exclusion | None in scope |
| CAT-10 | Mapping and placement | excluded feature | future or not implemented | A3:76, A3:133 | scope exclusion | None |
| CAT-11 | Lifecycle phases | in-scope part | future or not implemented | `register` phase in help only (A3:77, A3:134). Load chain: B1 | scope exclusion | None |
| CAT-12 | load (index of A1 section 2) | formal: load | modeled in original model | Open parts are on INT-LOAD-01 to INT-LOAD-18 | n/a | See the INT-LOAD rows |
| CAT-13 | install | in-scope part | native or suite evidence only | Suite InstallModule. Section 2.8 | n/a | Install path not modeled |
| CAT-14 | reinstall | in-scope part | static review only | Pins the installed version (`Main.cls:2695-2697`), logs as `install` | n/a | Q-19 decision. No native case |
| CAT-15 | update | in-scope part | static review only | OBS-L4, OBS-L5 code-read. OBS-L6 refuted, D16 native (stage C S3, S3N1). Suite Update | n/a | `update m -dev` from a registry not run. D16 fix |
| CAT-16 | uninstall | in-scope part | static review only | OBS-L17 code-read (`Lifecycle/Base.cls:290-293`). Nothing reads `-purge` | n/a | Ordinary native case for OBS-L17. `-purge` stays excluded |
| CAT-17 | orphans | excluded feature | static review only | A3:83 | scope exclusion | None |
| CAT-18 | Dependency resolution | in-scope part | native or suite evidence only | Selection outcomes: N-R05-HIGHER, N-R05-HIGHEROK, N-R05-DIA, N-R05-DEVUP, N-R05-DEVSAME (`R05`). Suite DependencyResolution had no effective test (E-5). Section 2.7 | n/a | Remote and ORAS dependency sources not run (R05d) |
| CAT-19 | Versions | excluded feature | static review only | No A2 operation (A3:85) | scope exclusion | None |
| CAT-20 | Repository configuration | excluded feature | future or not implemented | `-app`, `-local`, `-info`, `-p4` in help only (A3:86, A3:135) | scope exclusion | None |
| CAT-21 | Default registry | excluded feature | static review only | A3:87 | scope exclusion | None |
| CAT-22 | ORAS registries | excluded feature | static review only | A3:88 | scope exclusion | ORAS as a dependency source not run (R05d) |
| CAT-23 | Filesystem repositories, `.modules` | in-scope part | native or suite evidence only | Native A0. D13, I2 as INT-LOAD-14. Section 2.4 | n/a | D13 fix. I2 requirement |
| CAT-24 | publish and unpublish | excluded feature | static review only | A3:90 | scope exclusion | None |
| CAT-25 | Discovery and inspection | excluded feature | static review only | A3:91 | scope exclusion | None |
| CAT-26 | Settings | in-scope part | native or suite evidence only | D5: native F0 (`VR/184628Z-repro-config-delete-retain`) | n/a | D5 fix |
| CAT-27 | default-modifiers | in-scope part | native or suite evidence only | D19: stage C X1. D18: stage C H2, Q-18 commands in `HR`. Section 2.6 | n/a | D19 fix. Q-18 decision |
| CAT-28 | Install `-D` flags | in-scope part | static review only | `NoLock=1` turns compile locking on (`Lifecycle/Base.cls:1194-1198`). `PermitDowngrade`, `IgnoreInstalled` code-only (A3 disagreement 1). `NoTransaction`, `NoJournal`: B1 and `LT` | scope exclusion | None in scope |
| CAT-29 | Namespace ownership | excluded feature | static review only | A3:95 | scope exclusion | None |
| CAT-30 | Developer mode | formal: load | static review only | Q-01 consistent natively (g8, g9). Q-03 guard code-read only (INT-LOAD-12) | n/a | Q-01, Q-03 decisions |
| CAT-31 | sync | excluded feature | static review only | A3:97 | scope exclusion | None |
| CAT-32 | package | excluded feature | static review only | A3:98 | scope exclusion | None |
| CAT-33 | verify | in-scope part | static review only | Module loaded with `NoJournal=1` and no record (`Lifecycle/Base.cls:1527`, `:1551`). Dependency rows from `:1557`. A3 disagreement 3 | n/a | No native case |
| CAT-34 | Deployed code | excluded feature | static review only | A3:100 | scope exclusion | None |
| CAT-35 | Lock files | in-scope part | static review only | Section 2.5. Install from a lock file is future | n/a | Native lock-file cases not run (ordinary) |
| CAT-36 | Authoring utilities | excluded feature | static review only | `run-from-file` help disagrees with the format (A3:102) | scope exclusion | None |
| CAT-37 | Environment files | excluded feature | static review only | A3:103. `-env` has no evidence (VV:31) | scope exclusion | None |
| CAT-38 | Installer and self-upgrade | in-scope part | static review only | Installer runs `load` through `Shell` (`preload/cls/IPM/Installer.cls:126-131`, OP-LOAD-27) | n/a | None |
| CAT-39 | Test output | excluded feature | static review only | A3:105 | scope exclusion | None |
| CAT-40 | Analytics | excluded feature | static review only | Payload undocumented (A3:106) | scope exclusion | None |
| CAT-41 | Scripting entry points | in-scope part | static review only | Purge runs before each `Shell` command and its error is returned (`Main.cls:862-865`) | n/a | Q-13 decision |
| CAT-42 | history (index of A1 section 3) | formal: history | modeled in original model | Writer: B1. Readers: `HR`. Open parts are on the INT-HIST rows | n/a | See the INT-HIST rows |

### 1c. A2 observations (34)

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| OBS-L1 | Dependency history lost when the parent rolls back | formal: history | modeled in original model | D1. B1 V1, V2. Native A1-A3. `LT` L-2, L-4 | n/a | D1 fix. Repair conditions L-9, L-12 |
| OBS-L2 | `.tar.gz` not recognized by `load` | formal: load | native or suite evidence only | D15: stage C S1, S1N (`VR/204152Z-stagec`). `Main.cls:2466-2467` | regression (`.tar.gz`, `f83b207e`) | D15 fix |
| OBS-L3 | Plain `load` does not persist Root/Repository/Deployed | formal: load | static review only | `Utils/Module.cls:1296-1304`, `:1332`, `LockFile.cls:82`. Section 2.5 | n/a | Ordinary native case not run |
| OBS-L4 | `update m -dev` from a registry ends in normal mode | in-scope part | static review only | `Main.cls:2623-2630`, `Utils/Module.cls:258`, `:309-311` | n/a | Native case needs a registry source (R05d) |
| OBS-L5 | New dependency during `update` runs all its update steps | excluded feature | static review only | A2:580, `Utils/Module.cls:316-317` | scope exclusion | None in scope |
| OBS-L6 | `update -path` mismatch guard runs after writes | in-scope part | native or suite evidence only | Prediction refuted. D16 crash at `Utils/Module.cls:1232` (stage C S3, S3N1) | n/a | D16 fix |
| OBS-L7 | Archive paths delete the installed folder before any guard | formal: load | static review only | `Main.cls:2507-2509`, `Utils/Module.cls:236-242` | n/a | Q-04 decision. Ordinary native pair not run |
| OBS-L8 | `%Compile` ignores processor `OnPhase("Compile")` status | in-scope part | static review only | `Lifecycle/Base.cls:1081`. A failing processor leaves the load and its record successful | n/a | Ordinary native case with a failing test processor not run |
| OBS-L9 | Update steps skipped when the lock times out | excluded feature | static review only | A2:593 | scope exclusion | None in scope |
| OBS-L10 | `-dev` commits the manifest before requirements | formal: load | decision pending Q-17 | D4, as INT-LOAD-11 | n/a | Q-17 decision |
| OBS-L11 | Lock file written before the parent's commit | formal: load | static review only | `Utils/Module.cls:1415-1422` runs before C3 (`:1347-1351`). Section 2.5 | n/a | Ordinary native case not run |
| OBS-L12 | Uncacheable `.modules` makes a committed load report `<INVALID OREF>` | formal: load | modeled in original model | D13: B1 `case_c29_error_after_commit`, `VR/193952Z-replay-return-b`. `TempLocalRepoManager.cls:64-68`, `Main.cls:2530` | n/a | D13 fix |
| OBS-L13 | URL loads keep clones and ignore the git exit code | formal: load | static review only | Section 2.2 (`Main.cls:2352-2371`, `Utils/Module.cls:2409-2421`) | n/a | Native case needs an HTTP git remote |
| OBS-L14 | `PermitDowngrade` does not reach the downgrade refusal | formal: load | static review only | Honored by `Storage/Module.cls:1157`, ignored by `Utils/Module.cls:1213-1217`. Check moved out of `LoadModuleReference` in `f83b207e` | n/a (section 4) | Maintainer view on `PermitDowngrade` |
| OBS-L15 | `LoadedResource` rows survive a rolled-back load | excluded feature | static review only | A2:608. Verus premise P-10 puts `LoadedResource` out of scope | scope exclusion | Q-04 decision |
| OBS-L16 | Legacy XML installs: no transaction, `Committed=0` on failure | formal: load | static review only | Section 2.3 (`Utils/Module.cls:156-185`, `:207-209`, `Main.cls:2666-2670`) | n/a | Native case needs a registry serving a non-gzip payload (R05d) |
| OBS-L17 | Uninstalling a required module reports success | in-scope part | static review only | `Lifecycle/Base.cls:290-293`, `Storage/Module.cls:745-756` | n/a | Ordinary native case not run |
| OBS-H1 | One invalid filter column widens `find` and `delete` | formal: history | modeled in follow-up model | D2: `HR`, N-R04-SEL, N-R04-SEL2 (`R04`). Archived E1, E2, E4, E8 | n/a | D2 fix |
| OBS-H2 | Namespace default modifiers become history filters | formal: history | decision pending Q-18 | D18: stage C H2. Default-modifier commands in `HR`. N-R04-UNKNOWN shows the hidden SQL error | n/a | Q-18 decision |
| OBS-H3 | Auto-purge deletes nothing | formal: history | modeled in follow-up model | D3: N-R04-CUTOFF (`R04`), PURGE clause in `HR`, archived F1 | n/a | D3 fix |
| OBS-H4 | `config delete HistoryRetain` restores the first value | in-scope part | native or suite evidence only | D5: native F0 | n/a | D5 fix |
| OBS-H5 | Same as OBS-L1, seen from the history side | formal: history | modeled in original model | Alias of OBS-L1 (D1) | n/a | As OBS-L1 |
| OBS-H6 | The durable row lags until `%OnClose` | formal: history | modeled in original model | D10 for SIGKILL, refuted for Terminate (N-P10). B1 `kill_rec`. Native C1, C3, C4R | n/a | D10 fix |
| OBS-H7 | Action and source attribution for nested logs | formal: history | static review only | Stored label and source: native A0. Listing shows `n/a` unless `Action=install`: code-read (`History.cls:461-463`) | n/a | Q-15 decision |
| OBS-H8 | Recursive uninstall records skipped dependencies as successes | in-scope part | static review only | `Lifecycle/Base.cls:290-293` reached from the recursive path (A2:639) | n/a | Ordinary native case not run |
| OBS-H9 | `history delete -globally` needs no filter or confirmation | formal: history | modeled in follow-up model | QG option in `HR` (documented, D block) | n/a | None |
| OBS-H10 | Global readers and namespaces without IPM | formal: history | static review only | D20: stage C H10, GLOBAL clause in `HR`. Unreachable "IPM is not enabled" message: code-read | n/a | D20 fix |
| OBS-H11 | `Records` declares `CONTAINID=1`, first column `Action` | formal: history | static review only | `History.cls:28`. H-5 | n/a | Read-only native check not run (ordinary) |
| OBS-H12 | AutoPurge runs only from `Shell` and its failure blocks the command | formal: history | static review only | `Main.cls:862-865` | n/a | Q-13 decision. Purge failure not run |
| OBS-H13 | Full cleanup kills the instance-wide history temp global | formal: history | not run: concurrent loads during a destructive cleanup | `Lifecycle/Module.cls:51`. IRISTEMP sharing: N-P06, N-P17 | n/a | Disposable instance run. Q-16 decision |
| OBS-H14 | Invoke failures leave the phase unfinalized | formal: history | decision pending Q-14 | D6: B1 V5, `case_c28_phase_left_open`, `VR/184634Z-repro-activate-invoke-phase-open` | n/a | Q-14 decision |
| OBS-H15 | Self-uninstall reports success before it runs | formal: history | not run: self-uninstall is destructive | `Storage/Module.cls:630-643`, `Main.cls:2746-2749`. Persist step only: N-P16 | n/a | Disposable namespace run |
| OBS-H16 | A caller's transaction absorbs IPM's commits and the row | formal: history | decision pending Q-05 | B1 V8. Native D1 case. Caller-rollback configurations in `LT` | n/a | Q-05 decision |
| OBS-X1 | "Namespace" default modifiers stored in a `^%` global | in-scope part | native or suite evidence only | D19: stage C X1. `inc/IPM/Common.inc:31` | n/a | D19 fix |

### 1d. A2 commands (39)

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| CMD-1 | `help` | excluded feature | static review only | A2:45 | scope exclusion | None |
| CMD-2 | `quit` | structural | static review only | A3:117 | n/a | None |
| CMD-3 | `module-action` | excluded feature | static review only | Runs phases with no history log (A2:47) | scope exclusion | None |
| CMD-4 | `init` | excluded feature | static review only | A2:48 | scope exclusion | None |
| CMD-5 | `reload` | excluded feature | static review only | A2:49 | scope exclusion | None |
| CMD-6 | `sync` | excluded feature | static review only | A2:50 | scope exclusion | None |
| CMD-7 | `compile` | excluded feature | static review only | A2:51 | scope exclusion | None |
| CMD-8 | `test` | in-scope part | static review only | Phase-scoped dependency loads, each with its own `load` record, outside any IPM transaction (`Lifecycle/Base.cls:1261`, OP-LOAD-29) | n/a | No native case |
| CMD-9 | `package` | excluded feature | static review only | A2:53 | scope exclusion | None |
| CMD-10 | `verify` | in-scope part | static review only | `Lifecycle/Base.cls:1527`, `:1551`, `:1557` (OP-LOAD-28) | n/a | No native case |
| CMD-11 | `publish` | excluded feature | static review only | A2:55 | scope exclusion | None |
| CMD-12 | `update` | in-scope part | native or suite evidence only | Suite Update. Stage C S3, S3N1. Section 2.8 | n/a | Update path not modeled. OBS-L4, OBS-L5 not run. Q-09 |
| CMD-13 | `makedeployed` | excluded feature | static review only | A2:57 | scope exclusion | None |
| CMD-14 | `unpublish` | excluded feature | static review only | A2:58 | scope exclusion | None |
| CMD-15 | `config` | in-scope part | native or suite evidence only | D5: F0. Suite CLI `TestAutoPurgeOldHistory` | n/a | D5 fix |
| CMD-16 | `repo` | excluded feature | static review only | Read-only forcing inside a load is on INT-LOAD-13 | scope exclusion | None |
| CMD-17 | `load` | formal: load | modeled in original model | B1 final proof `Verus_verification/results/verus/20261113T222933Z-asbuilt-proofs-r3` (263 verified), rechecked in `FR/20261113T232529Z-r12-asbuilt-proofs`. `LT`. Replay re-evaluation `FR/20261114T002646Z-final-replay-reeval` | n/a | Open parts are on the INT-LOAD rows |
| CMD-18 | `import` | excluded feature | static review only | No module record, no history (A2:62) | scope exclusion | None |
| CMD-19 | `run-from-file` | excluded feature | static review only | Each line runs through `Shell`, so CAT-41 applies per line | scope exclusion | None |
| CMD-20 | `install` | in-scope part | native or suite evidence only | Suite InstallModule. Section 2.8 | n/a | Install path not modeled |
| CMD-21 | `reinstall` | in-scope part | static review only | `Main.cls:2673-2699`. Section 2.8 | n/a | Q-19, Q-09 decisions |
| CMD-22 | `uninstall` | in-scope part | native or suite evidence only | Suite History uninstall records. Section 2.8 | n/a | OBS-L17, OBS-H8, OBS-H15 open |
| CMD-23 | `exec` | excluded feature | static review only | A2:67 | scope exclusion | None |
| CMD-24 | `orphans` | excluded feature | static review only | A2:68 | scope exclusion | None |
| CMD-25 | `list-installed` | excluded feature | static review only | A2:69 | scope exclusion | None |
| CMD-26 | `list-dependents` | excluded feature | static review only | A2:70 | scope exclusion | None |
| CMD-27 | `show` | excluded feature | static review only | A2:71 | scope exclusion | None |
| CMD-28 | `default-modifiers` | in-scope part | native or suite evidence only | D19: X1. D18: H2. Section 2.6 | n/a | D19 fix. Q-18 decision |
| CMD-29 | `arrange` | excluded feature | static review only | A2:73 | scope exclusion | None |
| CMD-30 | `dependency-analyzer` | excluded feature | static review only | A2:74 | scope exclusion | None |
| CMD-31 | `version` | excluded feature | static review only | A2:75 | scope exclusion | None |
| CMD-32 | `generate` | excluded feature | static review only | A2:76 | scope exclusion | None |
| CMD-33 | `search` | excluded feature | static review only | A2:77 | scope exclusion | None |
| CMD-34 | `namespace` | excluded feature | static review only | A2:78 | scope exclusion | None |
| CMD-35 | `enable` | excluded feature | static review only | Honors `NoTransaction`/`NoJournal` (A2:79) but not through `LoadNewModule` | scope exclusion | None |
| CMD-36 | `unmap` | excluded feature | static review only | A2:80 | scope exclusion | None |
| CMD-37 | `module-version` | excluded feature | static review only | A2:81 | scope exclusion | None |
| CMD-38 | `information` | excluded feature | static review only | A2:82 | scope exclusion | None |
| CMD-39 | `history` | formal: history | modeled in follow-up model | `HR`. `R04` (7 cases, all conform) | n/a | Open parts are on INT-HIST-10 to INT-HIST-13 |

### 1e. A3 coverage-map rows (125)

The ID cell keeps the registry ID as written and adds the source line in parentheses. Section 1 rows repeat the A1 IDs. Each takes the disposition of its A1 row, because its partial reason is the A1 row's open part.

Section 1, commitment map (51):

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| INT-LOAD-01 (A3:17) | `load` from a folder, `.tgz`/`.tar.gz` archive or git URL, with dependencies | formal: load | static review only | Partial reason as A3:17. Evidence as INT-LOAD-01 in table 1a | regression (`.tar.gz`, `f83b207e`) | As INT-LOAD-01 in table 1a |
| INT-LOAD-02 (A3:18) | Empty and relative paths refused and a missing path fails | formal: load | native or suite evidence only | Partial reason as A3:18. Evidence as INT-LOAD-02 in table 1a | n/a | As INT-LOAD-02 in table 1a |
| INT-LOAD-03 (A3:19) | Archive unpacked to `<ModuleRoot>/<name>/<version>/` and temp folder removed | formal: load | static review only | Partial reason as A3:19. Evidence as INT-LOAD-03 in table 1a | n/a | As INT-LOAD-03 in table 1a |
| INT-LOAD-04 (A3:20) | `load` and `install` share one loading path | formal: load | static review only | Partial reason as A3:20. Evidence as INT-LOAD-04 in table 1a | n/a | As INT-LOAD-04 in table 1a |
| INT-LOAD-06 (A3:22) | Developer mode commits the manifest before resources load | formal: load | decision pending Q-05 | Partial reason as A3:22. Evidence as INT-LOAD-06 in table 1a | n/a | As INT-LOAD-06 in table 1a |
| INT-LOAD-07 (A3:23) | Developer mode is not propagated to dependencies | formal: load | static review only | Partial reason as A3:23. Evidence as INT-LOAD-07 in table 1a | n/a | As INT-LOAD-07 in table 1a |
| INT-LOAD-08 (A3:24) | Transactions, `NoTransaction`, `NoJournal`, flag sources | formal: load | static review only | Partial reason as A3:24. Evidence as INT-LOAD-08 in table 1a | n/a | As INT-LOAD-08 in table 1a |
| INT-LOAD-10 (A3:26) | Newer version with `UpdatePackage` refused unless `-force` | formal: load | decision pending Q-02 | Partial reason as A3:26. Evidence as INT-LOAD-10 in table 1a | n/a | As INT-LOAD-10 in table 1a |
| INT-LOAD-11 (A3:27) | System requirements validated before installation | formal: load | decision pending Q-17 | Partial reason as A3:27. Evidence as INT-LOAD-11 in table 1a | n/a | As INT-LOAD-11 in table 1a |
| INT-LOAD-12 (A3:28) | Developer-mode module not overwritten by a normal install | formal: load | static review only | Partial reason as A3:28. Evidence as INT-LOAD-12 in table 1a | n/a | As INT-LOAD-12 in table 1a |
| INT-LOAD-14 (A3:30) | Dependencies one at a time, `.modules` temporary repository | formal: load | native or suite evidence only | Partial reason as A3:30. Evidence as INT-LOAD-14 in table 1a | n/a | As INT-LOAD-14 in table 1a |
| INT-LOAD-15 (A3:31) | Other load options, lock file, `-nodev` | formal: load | static review only | Partial reason as A3:31. Evidence as INT-LOAD-15 in table 1a | n/a | As INT-LOAD-15 in table 1a |
| INT-LOAD-16 (A3:32) | Install and load check write permission first | formal: load | static review only | Partial reason as A3:32. Evidence as INT-LOAD-16 in table 1a | n/a | As INT-LOAD-16 in table 1a |
| INT-LOAD-18 (A3:34) | Same-version install or load succeeds | formal: load | modeled in original model | Partial reason as A3:34. Evidence as INT-LOAD-18 in table 1a | n/a | As INT-LOAD-18 in table 1a |
| A1 2.2 valid completion and neighbors (A3:35) | Load outcome after failures | formal: load | modeled in original model | B1 L1-NAMESPACE failures per mode. Native A1, A5, B3, B4 | n/a | Product tests that read module state after failures (maintainers) |
| INT-HIST-02 (A3:42) | Failed actions logged and their record survives the rollback | formal: history | modeled in original model | Partial reason as A3:42. Evidence as INT-HIST-02 in table 1a | n/a | As INT-HIST-02 in table 1a |
| INT-HIST-03 (A3:43) | Record created at start and filled in at end | formal: history | modeled in original model | Partial reason as A3:43. Evidence as INT-HIST-03 in table 1a | n/a | As INT-HIST-03 in table 1a |
| INT-HIST-04 (A3:44) | Record fields | formal: history | native or suite evidence only | Partial reason as A3:44. Evidence as INT-HIST-04 in table 1a | n/a | As INT-HIST-04 in table 1a |
| A1 3.2 Phases (A3:45) | Phase entries in the record | formal: history | decision pending Q-14 | D6: B1 V5, `case_c28_phase_left_open`, repro run | n/a | Q-14 decision |
| INT-HIST-05 (A3:46) | `Committed` means changes kept | formal: history | static review only | Partial reason as A3:46. Evidence as INT-HIST-05 in table 1a | n/a | As INT-HIST-05 in table 1a |
| INT-HIST-06 (A3:47) | Uninstall `Committed` equals success | in-scope part | static review only | Partial reason as A3:47. Evidence as INT-HIST-06 in table 1a | n/a | As INT-HIST-06 in table 1a |
| INT-HIST-08 (A3:49) | Dependency installs logged as their own records | formal: history | native or suite evidence only | Partial reason as A3:49. Evidence as INT-HIST-08 in table 1a | n/a | As INT-HIST-08 in table 1a |
| INT-HIST-09 (A3:50) | When a record exists | formal: history | static review only | Partial reason as A3:50. Evidence as INT-HIST-09 in table 1a | n/a | As INT-HIST-09 in table 1a |
| INT-HIST-10 (A3:51) | `history` actions, sort, limit, filters | formal: history | modeled in follow-up model | Partial reason as A3:51. Evidence as INT-HIST-10 in table 1a | n/a | As INT-HIST-10 in table 1a |
| INT-HIST-11 (A3:52) | `-globally` reads every namespace | formal: history | static review only | Partial reason as A3:52. Evidence as INT-HIST-11 in table 1a | n/a | As INT-HIST-11 in table 1a |
| INT-HIST-12 (A3:53) | `history delete` deletes the filtered records | formal: history | decision pending Q-11, Q-20 | Partial reason as A3:53. Evidence as INT-HIST-12 in table 1a | n/a | As INT-HIST-12 in table 1a |
| INT-HIST-13 (A3:54) | Retention purge deletes older records | formal: history | static review only | Partial reason as A3:54. Evidence as INT-HIST-13 in table 1a | n/a | As INT-HIST-13 in table 1a |
| INT-HIST-14 (A3:55) | Finalized records cannot be edited | formal: history | decision pending Q-21 | Partial reason as A3:55. Evidence as INT-HIST-14 in table 1a | n/a | As INT-HIST-14 in table 1a |
| INT-HIST-15 (A3:56) | History survives IPM's own uninstall | formal: history | not run: destructive self-uninstall, concurrent cleanup | Partial reason as A3:56. Evidence as INT-HIST-15 in table 1a | n/a | As INT-HIST-15 in table 1a |
| INT-HIST-17 (A3:58) | IRIS audit database proposed, not implemented | excluded feature | future or not implemented | Partial reason as A3:58. Evidence as INT-HIST-17 in table 1a | scope exclusion | As INT-HIST-17 in table 1a |
| CAT-02 (A3:68) | Resources and scope | excluded feature | future or not implemented | Partial reason as A3:68. Evidence as CAT-02 in table 1b | scope exclusion | As CAT-02 in table 1b |
| CAT-03 (A3:69) | Dependencies in the manifest | in-scope part | static review only | Partial reason as A3:69. Evidence as CAT-03 in table 1b | scope exclusion | As CAT-03 in table 1b |
| CAT-05 (A3:71) | System requirements | formal: load | decision pending Q-17 | Partial reason as A3:71. Evidence as CAT-05 in table 1b | n/a | As CAT-05 in table 1b |
| CAT-06 (A3:72) | Invoke and custom phases | in-scope part | decision pending Q-14 | Partial reason as A3:72. Evidence as CAT-06 in table 1b | n/a | As CAT-06 in table 1b |
| CAT-07 (A3:73) | Resource processors | excluded feature | static review only | Partial reason as A3:73. Evidence as CAT-07 in table 1b | scope exclusion | As CAT-07 in table 1b |
| CAT-09 (A3:75) | Update steps | excluded feature | static review only | Partial reason as A3:75. Evidence as CAT-09 in table 1b | scope exclusion | As CAT-09 in table 1b |
| CAT-10 (A3:76) | Mapping and placement | excluded feature | future or not implemented | Partial reason as A3:76. Evidence as CAT-10 in table 1b | scope exclusion | As CAT-10 in table 1b |
| CAT-11 (A3:77) | Lifecycle phases | in-scope part | future or not implemented | Partial reason as A3:77. Evidence as CAT-11 in table 1b | scope exclusion | As CAT-11 in table 1b |
| CAT-14 (A3:80) | reinstall | in-scope part | static review only | Partial reason as A3:80. Evidence as CAT-14 in table 1b | n/a | As CAT-14 in table 1b |
| CAT-15 (A3:81) | update | in-scope part | static review only | Partial reason as A3:81. Evidence as CAT-15 in table 1b | n/a | As CAT-15 in table 1b |
| CAT-16 (A3:82) | uninstall | in-scope part | static review only | Partial reason as A3:82. Evidence as CAT-16 in table 1b | n/a | As CAT-16 in table 1b |
| CAT-19 (A3:85) | Versions | excluded feature | static review only | Partial reason as A3:85. Evidence as CAT-19 in table 1b | scope exclusion | As CAT-19 in table 1b |
| CAT-20 (A3:86) | Repository configuration | excluded feature | future or not implemented | Partial reason as A3:86. Evidence as CAT-20 in table 1b | scope exclusion | As CAT-20 in table 1b |
| CAT-26 (A3:92) | Settings | in-scope part | native or suite evidence only | Partial reason as A3:92. Evidence as CAT-26 in table 1b | n/a | As CAT-26 in table 1b |
| CAT-27 (A3:93) | default-modifiers | in-scope part | native or suite evidence only | Partial reason as A3:93. Evidence as CAT-27 in table 1b | n/a | As CAT-27 in table 1b |
| CAT-28 (A3:94) | Install `-D` flags | in-scope part | static review only | Partial reason as A3:94. Evidence as CAT-28 in table 1b | scope exclusion | As CAT-28 in table 1b |
| CAT-30 (A3:96) | Developer mode | formal: load | static review only | Partial reason as A3:96. Evidence as CAT-30 in table 1b | n/a | As CAT-30 in table 1b |
| CAT-35 (A3:101) | Lock files | in-scope part | static review only | Partial reason as A3:101. Evidence as CAT-35 in table 1b | n/a | As CAT-35 in table 1b |
| CAT-36 (A3:102) | Authoring utilities | excluded feature | static review only | Partial reason as A3:102. Evidence as CAT-36 in table 1b | scope exclusion | As CAT-36 in table 1b |
| CAT-40 (A3:106) | Analytics | excluded feature | static review only | Partial reason as A3:106. Evidence as CAT-40 in table 1b | scope exclusion | As CAT-40 in table 1b |
| CAT-41 (A3:107) | Scripting entry points | in-scope part | static review only | Partial reason as A3:107. Evidence as CAT-41 in table 1b | n/a | As CAT-41 in table 1b |

Section 2, operations without an owner (9):

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| OP-LOAD-08 (A3:116) | OP-LOAD-08 `ResourceReference:MarkResourceForUpdate` | in-scope part | native or suite evidence only | A3 called it structural. D16 crashes inside this block (`Utils/Module.cls:1231-1232`), stage C S3, S3N1 | n/a | D16 fix. Reclassified, see section 2.8 |
| CMD-2 (A3:117) | CMD-2 `quit` | structural | static review only | A3:117. `Main.cls:1076-1077` returns from the shell loop | n/a | None |
| EP %IPM.Main:ShellInternal (A3:118) | EP `%IPM.Main:ShellInternal` | structural | static review only | Merges namespace defaults (`Main.cls:1067-1070`) and skips the purge | n/a | None. Governed through `ipm.shell` |
| EP .ZPM document (%IPM.StudioDocument.Module) (A3:119) | EP `.ZPM` document (`%IPM.StudioDocument.Module`) | structural | modeled in original model | Nested transaction inside the manifest load (`Utils/Module.cls:1251`). Manifest refusals modeled in B1 | n/a | None |
| EP other @API methods: LoadedResource Validate/Clear/ReportConflicts, LogManager, Utils.Build, DependencyAnalyzer run methods, XSLTProvider, LocalizationExport, TempFileManager, GetDependentsList/GetDependentsAsTree (A3:120) | EP other `@API` methods: `LoadedResource` `Validate`/`Clear`/`ReportConflicts` ... | excluded feature | static review only | A3:120 | scope exclusion | `LoadedResource` survival has its own row (OBS-L15) |
| EP SQL procedures and queries (GET_DEFAULT_RESOURCE_PROCESSOR, OrphanedResources, VersionRequirements and others) (A3:121) | EP SQL procedures and queries (`GET_DEFAULT_RESOURCE_PROCESSOR` ... | excluded feature | static review only | A3:121 | scope exclusion | None |
| EP source-control hooks (%IPM.ExtensionBase.SourceControl.Interface) (A3:122) | EP source-control hooks (`%IPM.ExtensionBase.SourceControl.Interface`) | excluded feature | static review only | A3:122 (no human contract recovered) | scope exclusion | None in scope |
| Lifecycle.Module:%Activate Studio project creation in developer mode (A3:123) | `Lifecycle.Module:%Activate` Studio project creation in developer mode | excluded feature | static review only | A3:123 | scope exclusion | None |
| Lifecycle.Module:%Activate update-step seeding (A3:124) | `Lifecycle.Module:%Activate` update-step seeding | excluded feature | static review only | A3:124. Interacts with `-force` on INT-LOAD-10 | scope exclusion | None in scope |

Section 3, commitments without an operation (10):

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| INT-HIST-17 (A3:130) | INT-HIST-17 IRIS audit database for history | excluded feature | future or not implemented | A3:130 | scope exclusion | None |
| CAT-35 (A3:131) | CAT-35 install from a lock file (issue #962) | in-scope part | future or not implemented | A3:131 (install from a lock file) | scope exclusion | None |
| CAT-09 (A3:132) | CAT-09 mirrored updates; `VersionBase.cls:18` steps "when becoming primary" | excluded feature | future or not implemented | A3:132 (mirrored updates) | scope exclusion | None |
| CAT-10 (A3:133) | CAT-10 `${authUnauthenticated}` | excluded feature | future or not implemented | A3:133 | scope exclusion | None |
| CAT-11 (A3:134) | CAT-11 `register` phase | excluded feature | future or not implemented | A3:134 | scope exclusion | None |
| CAT-20 (A3:135) | CAT-20 `repo -app`, `-local`, `-info`, `-p4` | excluded feature | future or not implemented | A3:135 | scope exclusion | None |
| CAT-07 (A3:136) | CAT-07 `<WSGIApplication>` (`wiki/06`) | excluded feature | future or not implemented | A3:136 | scope exclusion | None |
| CAT-16 (A3:137) | CAT-16 `uninstall -purge` | excluded feature | future or not implemented | A3:137 (`uninstall m -purge` sets a key nothing reads) | scope exclusion | None |
| CAT-16 (A3:138) | CAT-16 uninstall prevented for dependents | in-scope part | static review only | OBS-L17 (`Lifecycle/Base.cls:290-293`) | n/a | Ordinary native case not run |
| CAT-37 (A3:139) | CAT-37 environment-file wiki page | excluded feature | future or not implemented | A3:139 (the help points to a wiki page that does not exist) | scope exclusion | Documentation (maintainers) |

Section 4, partially covered clauses (32):

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| load.path (A3:147) | `.tar.gz` archives | formal: load | native or suite evidence only | D15: stage C S1, S1N | regression (`.tar.gz`, `f83b207e`) | D15 fix |
| load.refusal.path (A3:148) | only absolute paths | formal: load | native or suite evidence only | D17: stage C S4R | n/a | D17 fix |
| load.refusal, load.refusal.manifest (A3:149) | refusals keep no module data in any mode | formal: load | modeled in original model | D11, D12: B1 L1-NAMESPACE refusals, B1-C08 to C10, C51, C52, replays | n/a | D11, D12 fixes |
| load.refusal.requirements (A3:150) | checked before installation | formal: load | decision pending Q-17 | D4: V3, native B2-B4 | n/a | Q-17 decision |
| load.refusal.downgrade (A3:151) | no bypass | formal: load | static review only | OBS-L14 code-read | n/a (section 4) | Maintainer view on `PermitDowngrade` |
| load.path.archive, install.package (A3:152) | rollback leaves the namespace as it was | formal: load | static review only | OBS-L7 code-read (section 2.1) | n/a | Q-04 decision. Ordinary native pair not run |
| load.dependencies.local-repo (A3:153) | removed even after an error | formal: load | native or suite evidence only | D13 replay, I2 native (section 2.4) | n/a | D13 fix. I2 requirement |
| load.write-check (A3:154) | checked before installing | formal: load | static review only | Section 2.1 | n/a | Native case needs an unwritable `ModuleRoot` |
| load.core (A3:155) | one loading path | formal: load | static review only | Section 2.3 | n/a | Native case needs a registry (R05d) |
| load.outcome (A3:156) | normal mode rolls back the whole load, dependencies included | formal: load | modeled in follow-up model | D14 and the Q-23 case per activation in `LT` (Q-22/Q-23 options). Native M5, M5N, N-R05-Q23F, N-R05-Q23S. L-4 (broken closure) holds under either Q-22 option | n/a | Q-22, Q-23 decisions. L-4 (drafted as D21) needs a disposition |
| deps.lockfile (A3:157) | file in the module root | formal: load | static review only | Section 2.5 | n/a | Ordinary native case not run |
| dev-mode.not-propagated, update.dev-mode (A3:158) | `update -dev` | formal: load | static review only | OBS-L4 code-read | n/a | Native case needs a registry source (R05d) |
| dev-mode.sticky (A3:159) | plain load keeps mode | formal: load | static review only | Q-01 consistent natively (g8, g9). `Root`/`Repository` not saved: code-read (section 2.5) | n/a | Q-01 decision |
| update.steps (A3:160) | steps from current to new version | excluded feature | static review only | OBS-L9, OBS-L5 code-read | scope exclusion | None in scope |
| update.refusal (A3:161) | `-path` mismatch refused with its message, no module data | in-scope part | native or suite evidence only | D16: stage C S3, S3N1 | n/a | D16 fix |
| module.processor (A3:162) | processor errors fail the phase | in-scope part | static review only | OBS-L8 code-read (`Lifecycle/Base.cls:1081`) | n/a | Ordinary native case not run |
| history.survives-rollback (A3:163) | dependency records survive a parent rollback | formal: history | modeled in original model | D1: B1 V1, V2. Native A1-A3. `LT` | n/a | D1 fix |
| history.record, history.phases (A3:164) | running or never-finished actions listed with their started phases | formal: history | modeled in original model | D10: B1 `kill_rec`. Native C1, C3, C4R | n/a | D10 fix |
| history.record.abnormal-end (A3:165) | `Committed` on an unfinished record | formal: history | decision pending Q-08 | B1 V9. Native C4R | n/a | Q-08 decision |
| history.phases (A3:166) | every phase records its end | formal: history | decision pending Q-14 | D6: B1 V5 | n/a | Q-14 decision |
| history.committed, history.committed.nested, history.dependencies.parent-rollback (A3:167) | kept changes, including dependencies undone by the parent | formal: history | static review only | Legacy XML part code-read (section 2.3). Nested parts: V8, native D1 case, `LT` | n/a | Legacy XML native case. Q-05, Q-06 decisions |
| history.committed.uninstall, uninstall.dependents, uninstall.recurse (A3:168) | success means removed | in-scope part | static review only | OBS-L17, OBS-H8 code-read | n/a | Ordinary native case not run |
| history.fields (A3:169) | source empty for load | formal: history | static review only | Stored source: native A0. Listing `n/a`: code-read (`History.cls:461-463`) | n/a | Q-15 decision |
| history.fields (A3:170) | `Package` empty only before any name is known | formal: history | modeled in original model | D9: B1 V7. Native g3, g6, g7, R4 | n/a | D9 fix |
| history.locked (A3:171) | only ended actions are finalized | formal: history | decision pending Q-21 | B1 V10. Native C2 | n/a | Q-21 decision |
| history.read.filter, history.delete (A3:172) | filters apply | formal: history | modeled in follow-up model | D2: `HR`, N-R04-SEL, N-R04-SEL2. Hidden query error: N-R04-UNKNOWN (H-3) | n/a | D2 fix. Q-10 decision |
| history.read.filter (A3:173) | explicit filters only | formal: history | decision pending Q-18 | D18: stage C H2. Q-18 commands in `HR` | n/a | Q-18 decision |
| history.read.api (A3:174) | `GlobalRecords` reads all namespaces | formal: history | static review only | D20 in `HR` and stage C H10. `Records` ID column: code-read (`History.cls:28`) | n/a | Read-only native check not run |
| history.retention (A3:175) | deletes old records | formal: history | static review only | D3: `HR`, N-R04-CUTOFF. D5: F0. Error blocks commands: code-read (`Main.cls:862-865`) | n/a | D3, D5 fixes. Q-13 decision |
| history.keep-on-ipm-uninstall, uninstall.ipm (A3:176) | log kept; uninstall recorded | formal: history | not run: destructive self-uninstall, concurrent cleanup | `Storage/Module.cls:630-643`, `Lifecycle/Module.cls:51` | n/a | Disposable namespace or instance run. Q-16 decision |
| settings.default-modifiers (A3:177) | current namespace only | in-scope part | native or suite evidence only | D19: stage C X1 | n/a | D19 fix |
| settings (A3:178) | `delete` restores the default | in-scope part | native or suite evidence only | D5: native F0 | n/a | D5 fix |

Section 9, native status rows that do not conform (23):

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| load.refusal.update-required.dev-bypass (Q-02), dev-mode.sticky (Q-01) (A3:248) | consistent with the provisional answer (g5, g8, g9) | formal: load | decision pending Q-01, Q-02 | Native g5, g8, g9 (`VR/182832Z-cases-dg`). B1 Q-02 parameter | n/a | Q-01, Q-02 decisions |
| load.refusal.requirements.timing (Q-17) (A3:249) | confirmed defect D4 pending the decision (B2-B4); B0, B1 conform | formal: load | decision pending Q-17 | D4: native B2-B4. B1 V3 | n/a | Q-17 decision |
| load.path (.tar.gz) (A3:252) | confirmed defect D15 (S1); `.tgz` neighbour S1N conforms | formal: load | native or suite evidence only | D15: `VR/204152Z-stagec` | regression (`.tar.gz`, `f83b207e`) | D15 fix |
| load.refusal, load.refusal.manifest (A3:253) | confirmed defects D11 (S2, S2B, B1-C10, B1-C52) and D12 (B1-C08); S2N conforms | formal: load | modeled in original model | D11, D12: B1 cases and replays | n/a | D11, D12 fixes |
| update.refusal (-path mismatch) (A3:254) | confirmed defect D16 (S3, S3N1); OBS-L6 refuted; S3N2 conforms | in-scope part | native or suite evidence only | D16: `VR/204152Z-stagec` | n/a | D16 fix |
| load.refusal.path (A3:255) | confirmed defect D17 (S4R); S4N1, S4N2 conform | formal: load | native or suite evidence only | D17: `VR/204405Z-stagec2` | n/a | D17 fix |
| load.dependencies.local-repo (A3:256) | confirmed defect D13 (B1-C29); issue I2 (leftover temporary repositories after ... | formal: load | native or suite evidence only | D13: `VR/193952Z-replay-return-b`. I2: `VR/210229Z-temp-repos-leftover` | n/a | D13 fix. I2 requirement |
| transaction.flag-sources.dependency-defaults (Q-22), transaction.flag-sources.dependency-no-transaction (Q-23, NoTransaction variant not run), load.outcome (A3:257) | M5 keeps a `NoJournal` dependency after the parent's rollback (D14, decision ... | formal: load | modeled in follow-up model | `LT` Q-22/Q-23 options. Q-23 case now run: N-R05-Q23F, N-R05-Q23S (`R05`). L-4 holds under either Q-22 option | n/a | Q-22, Q-23 decisions. L-4 disposition |
| transaction.flag-sources, settings.default-modifiers (A3:258) | confirmed defect D19 (X1, instance-wide defaults) | in-scope part | native or suite evidence only | D19: `VR/204152Z-stagec` X1 | n/a | D19 fix |
| history.read.filter.default-modifiers (Q-18) (A3:259) | D18, decision pending (H2) | formal: history | decision pending Q-18 | D18: stage C H2. `HR` | n/a | Q-18 decision |
| history.read.global, history.read.api (A3:260) | `find -globally` conforms; `GlobalRecords` confirmed defect D20 (H10) | formal: history | modeled in follow-up model | D20: GLOBAL clause in `HR`, archived H10 | n/a | D20 fix |
| history.record (A3:261) | D10 named (C1, C3, C4R); model cases C44-C48 T/K agree | formal: history | modeled in original model | D10: B1 `kill_rec`, C44-C48 T/K | n/a | D10 fix |
| legacy XML (load.core) (A3:262) | not run | formal: load | static review only | Section 2.3. Still not run natively | n/a | Native case needs a registry serving a non-gzip payload (R05d) |
| history.survives-rollback, history.dependencies, history.dependencies.parent-rollback (A3:263) | confirmed defect D1 (A1-A3) | formal: history | modeled in original model | D1: B1 V1, V2. `LT` | n/a | D1 fix |
| history.committed.nested (Q-05) (A3:264) | deviates (D1 caller case); D0 conforms | formal: history | decision pending Q-05 | B1 V8. Native D1 case. `LT` caller configurations | n/a | Q-05 decision |
| history.record, history.record.abnormal-end (Q-08), history.locked.unfinished (Q-21), history.survives-rollback.temp-copy (A3:265) | C1, C3, C4R show no phases; C2 finalized without end; C4R `Committed=0` ... | formal: history | modeled in original model | D10 (`kill_rec`), Q-08 (V9), Q-21 (V10). Temp-copy premise refuted for Terminate (N-P10) | n/a | D10 fix. Q-08, Q-21 decisions |
| history.fields (A3:266) | deviation D9 (refused loads record no package) | formal: history | modeled in original model | D9: B1 V7 | n/a | D9 fix |
| history.read.filter.invalid-column (Q-10) (A3:267) | confirmed defect D2 (E1, E2, E4, E8); E3/E7 differ | formal: history | modeled in follow-up model | D2: `HR`, N-R04-SEL, N-R04-SEL2 | n/a | D2 fix. Q-10 decision |
| history.delete (A3:268) | E5, E6 conform; E4 under D2 | formal: history | modeled in follow-up model | DEL clause in `HR`. N-R04-DEL | n/a | D2 fix |
| history.delete.running-records (Q-20) (A3:269) | observed: the record disappears and is not recreated (N-P15) | formal: history | decision pending Q-20 | N-P15. QA option in `HR` (H-7) | n/a | Q-20 decision |
| history.retention, history.retention.effect, settings (A3:270) | confirmed defects D3 (F1) and D5 (F0); purge failure not run | formal: history | static review only | D3, D5 as INT-HIST-13. Purge failure: code-read only | n/a | D3, D5 fixes. Q-13 decision |
| history.phases, history.phases.returned-errors (Q-14) (A3:271) | confirmed defect D6; reader defect D8 | formal: history | native or suite evidence only | D6: B1 V5. I1 (unfinished phase display): native `VR/184444Z-cases-hr` only | n/a | Q-14 decision. I1 requirement |
| history.keep-on-ipm-uninstall and its Q-16, uninstall cases (A3:274) | not run (self-uninstall persist step probed in N-P16; IRISTEMP sharing ... | formal: history | not run: destructive self-uninstall, concurrent cleanup | N-P06, N-P16, N-P17 (`VR/181129Z-premises`) | n/a | Disposable namespace or instance run. Q-16 decision |

### 1f. Verus coverage-map rows (38)

The short statement is the row's reliance text from `Verus_verification/coverage-map.md`.

| ID | Short statement | Scope class | Disposition | Evidence pointer | Regression or scope exclusion | Remaining work |
|---|---|---|---|---|---|---|
| load.path, load.path.archive (VV:29) | `.tgz` supported. `.tar.gz` confirmed violation D15 (regression `f83b207e`). ... | formal: load | static review only | `.tgz`: suite FileBinaryTar, S1N. `.tar.gz`: D15 (S1). Git URL: section 2.2 | regression (`.tar.gz`, `f83b207e`) | D15 fix. Git URL native case needs an HTTP git remote |
| load.core (VV:30) | unestablished for install and update beyond the suites | formal: load | static review only | Install, update: suites only. Legacy XML: section 2.3 | n/a | Install and update paths not modeled. Legacy XML native case (R05d) |
| load.options (VV:31) | supported for those flags. `-env`, `-bypass-py-deps`, `-create-lockfile` ... | formal: load | static review only | `-create-lockfile`: section 2.5. `-env`, `-bypass-py-deps`, `-skip-sync`: no evidence | n/a | Native option cases not run |
| load.refusal (VV:32) | supported for downgrade and update-required in every mode. Confirmed ... | formal: load | native or suite evidence only | D11, D12: B1. D16: stage C S3 only. D4 under Q-17 | n/a | D11, D12, D16 fixes. Q-17 decision |
| load.refusal.path (VV:33) | relative nonexistent and absolute paths supported. Existing relative path ... | formal: load | native or suite evidence only | D17: stage C S4R | n/a | D17 fix |
| load.refusal.manifest (VV:34) | default and `-dev` supported. NoTransaction and NoJournal: confirmed violations ... | formal: load | modeled in original model | D11, D12, D9: B1 cases, replays C08-C10, C51, C52 | n/a | D9, D11, D12 fixes |
| load.refusal.update-required (VV:36) | supported. Q-02 provisional matches the product (unresolved authority for the ... | formal: load | decision pending Q-02 | B1 `case_c15_update_refused`, native g3-g5 | n/a | Q-02 decision |
| load.refusal.requirements (VV:37) | confirmed violation D4 under Q-17 provisional (decision pending) | formal: load | decision pending Q-17 | D4: V3, native B2-B4 | n/a | Q-17 decision |
| load.outcome (VV:39) | supported for database state within scope (P-05, P-06b confirmed). Confirmed ... | formal: load | static review only | Database state: B1 and `LT` (PREFIX holds, L-7). D13, I2: native. D14: `LT`. Non-database effects (Q-04): reading only | n/a | Q-04, Q-22 decisions. D13 fix |
| load.dependencies (VV:41) | module state supported. Dependency history: confirmed violation D1 | formal: load | modeled in original model | D1: B1 V1, V2. Nesting reduction in `LT`, checked by N-R05-NESTF, N-R05-NESTS, N-R05-DIA | n/a | D1 fix |
| load.dependencies.local-repo (VV:42) | supported by observation only | formal: load | native or suite evidence only | Native A0. Section 2.4 | n/a | Lifecycle not modeled |
| load.write-check (VV:43) | unestablished | formal: load | static review only | Section 2.1 | n/a | Native case needs an unwritable `ModuleRoot` |
| transaction (VV:49) | supported, conditional on P-01 | formal: load | modeled in original model | B1 `theorem_tlevel`, control 04. P-01 confirmed | n/a | None |
| transaction.rollback-scope (VV:50) | unresolved authority | formal: load | static review only | A2 premise P-7 and reading only | n/a | Q-04 decision |
| transaction.flag-sources (VV:52) | confirmed violation D19 (namespace defaults shared by every namespace). ... | formal: load | native or suite evidence only | D19: stage C X1 only. Per-activation `<Defaults>` (D14, Q-23) now in `LT` (L-1 to L-4) | n/a | D19 fix. Q-22, Q-23 decisions |
| dev-mode.overwrite-guard (VV:55) | unestablished. Q-03 unresolved authority | formal: load | static review only | Top-level guard code-read (`Utils/Module.cls:109-112`). Dependencies never guarded: N-R05-DEVUP (L-5) | n/a | Q-03 decision. L-5 requirement |
| dev-mode.sticky (VV:56) | provisional answer matches the product | formal: load | decision pending Q-01 | Native g8, g9. B1 Q-01 parameter | n/a | Q-01 decision |
| history.actions (VV:62) | load supported. Others by suite only | formal: history | native or suite evidence only | Load: B1. Others: suite History | n/a | Non-load writers not modeled |
| history.record (VV:63) | record existence supported for top-level actions. Unfinished record content ... | formal: history | modeled in original model | D10: B1 `kill_rec`, `theorem_top_record_durable` | n/a | D10 fix |
| history.record.abnormal-end (VV:64) | confirmed violation under the provisional answer (decision pending) | formal: history | decision pending Q-08 | B1 V9. Native C4R | n/a | Q-08 decision |
| history.record.unlogged-refusals (VV:65) | unresolved authority | formal: history | static review only | Section 2.8 (refusals before the log) | n/a | Q-09 decision |
| history.survives-rollback (VV:66) | top-level supported. Dependencies: confirmed violation D1 | formal: history | modeled in original model | D1: B1 `theorem_r1`, V1, V2. `LT` | n/a | D1 fix |
| history.fields (VV:68) | Success supported. Package on refusals: confirmed violation D9 | formal: history | modeled in original model | D9: B1 V7 | n/a | D9 fix |
| history.fields.dependency-source (VV:69) | unresolved authority | formal: history | decision pending Q-15 | Native A0 | n/a | Q-15 decision |
| history.phases (VV:70) | confirmed violation D6 under Q-14 provisional (decision pending) | formal: history | decision pending Q-14 | D6: B1 V5, `case_c28_phase_left_open` | n/a | Q-14 decision |
| history.committed (VV:71) | "changed implies committed" supported. "Committed implies changed": confirmed ... | formal: history | modeled in original model | D12: B1. Dependency `Committed` per activation: L-8 to L-10, L-12 in `LT`, P-14 in `P21` | n/a | D12 fix. Repair 103 limits (L-9, L-12) |
| history.committed.uninstall (VV:73) | by suite only | in-scope part | native or suite evidence only | Suite History | n/a | Uninstall not modeled. OBS-L17, OBS-H8 |
| history.committed.nested (VV:75) | confirmed deviation under the provisional answer (decision pending) | formal: history | decision pending Q-05 | B1 V8. Native D1 case | n/a | Q-05 decision |
| history.committed.side-effects (VV:76) | unresolved authority | formal: history | static review only | No evidence. Reading only | n/a | Q-07 decision |
| history.dependencies (VV:77) | confirmed violation D1 | formal: history | modeled in original model | D1: B1 V1. `LT` | n/a | D1 fix |
| history.namespace (VV:78) | supported by suite and premise | formal: history | native or suite evidence only | N-P06, N-P17. Suite History | n/a | None |
| history.read, .find, .filter, .details (VV:79) | confirmed violation D2. Default modifiers as filters: D18 (decision Q-18). ... | formal: history | native or suite evidence only | D2, D18 now in `HR` and `R04`. I1 display: native only | n/a | D2 fix. Q-18 decision. I1 requirement |
| history.read.global, .api (VV:80) | `-globally` supported. `GlobalRecords`: confirmed violation D20 | formal: history | modeled in follow-up model | D20: GLOBAL clause in `HR`, archived H10. `-globally` find in `HR` | n/a | D20 fix |
| history.delete (VV:81) | confirmed violation D2 | formal: history | modeled in follow-up model | D2: DEL clause in `HR`, N-R04-DEL, archived X3 | n/a | D2 fix |
| history.delete.running-records (VV:82) | unresolved authority | formal: history | decision pending Q-20 | N-P15. QA option in `HR` | n/a | Q-20 decision |
| history.locked.unfinished (VV:84) | confirmed violation under the provisional answer (decision pending) | formal: history | decision pending Q-21 | B1 V10. Native C2 | n/a | Q-21 decision |
| history.retention (VV:85) | confirmed violations D3, D5 | formal: history | native or suite evidence only | D3: `HR`, N-R04-CUTOFF. D5: native F0 only | n/a | D3, D5 fixes |
| history.keep-on-ipm-uninstall (VV:86) | unestablished. Self-uninstall suspected (OBS-H15) | formal: history | not run: destructive self-uninstall | N-P16 persist step only | n/a | Disposable namespace run. Q-16 decision |

## 2. In-scope parts of other load paths

Every path below enters the shared core or the shared history writer. The diagram shows where each part sits against the commit points C1, C2 and C3 of `LoadNewModule`. Line numbers are `Main.cls` (M) and `Utils/Module.cls` (U).

```
Shell(cmd)                                    M:857
 |- AutoPurgeOldHistory, error stops cmd      M:862-865                 2.10
 '- ShellInternal: merge ^%IPM.DefaultModifiers  M:1067-1070            2.6
     '- Load / Install / Update (Uninstall: section 2.8)
         |- history record Init (durable twin)   M:2420, M:2566, M:4438  2.10
         |- path checks, git clone               M:2444-2464             2.2
         |- load archive copied to ModuleRoot    M:2505-2514             2.1
         |  or LoadModuleReference               U:40-211
         |     |- developer-mode guard           U:109                   2.7
         |     |- archive: write check           U:228-233               2.1
         |     '- legacy XML, no LoadNewModule   U:156-185               2.3
         |- .modules repository created          M:2528                  2.4
         |- LoadNewModule                        U:1152-1364
         |    |- <Defaults> merged under params  U:1165-1169             2.6
         |    |- tstart, or NoJournal, or none   U:1191-1196
         |    |- guards, marker if no tx         U:1213-1227
         |    |- manifest load and save          U:1251-1307
         |    |- developer mode: C1, tstart      U:1309-1319
         |    |- requirements                    U:1321
         |    |- LoadDependencies                U:1328 -> U:1368-1423   2.7
         |    |    |- per dependency: own record, own LoadNewModule
         |    |    '- lock file written          U:1415-1422             2.5
         |    |- developer mode: C2              U:1335-1339
         |    |- phases through Activate         U:1346
         |    |- normal mode: C3, SetCommitted   U:1347-1351
         |    '- roll back to the entry level    U:1358-1362
         |- .modules repository CleanUp          M:2530                  2.4
         '- Finalize, %OnClose copies to the twin   M:2427, M:2430       2.10
```

### 2.1 Write check (INT-LOAD-16, `load.write-check`)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-19 `LoadModuleFromArchive`. OP-LOAD-02 `LoadInternal` archive branch. |
| Code | Registry archives (top-level `install`, `update`, and dependencies from a remote or ORAS repository) check `Writeable` on `ModuleRoot`, or its parent before first use, and refuse with "Insufficient file system permissions" (`Utils/Module.cls:225-233`). Then they delete and recreate `<ModuleRoot>/<name>/<version>` (`:235-246`). `load` of a `.tgz` has no check (`Main.cls:2505-2513`). A permission problem there surfaces as an error from `RemoveDirectoryTree` or `CreateDirectoryChain`, possibly after an existing folder was deleted. A folder `load` copies nothing. |
| Commit point | Both run before `LoadNewModule` starts a transaction. For a top-level action they run at the caller's level. For a dependency they run inside the parent's transaction, but their effects are files, which no rollback restores (Q-04). |
| Record and readers | A refusal reaches `Finalize` with the error: top-level `install` at `Main.cls:2667`, dependency at `Utils/Module.cls:207-209`. The record reads `Success` = error and `Committed=0`. No module data was written. |
| Baseline | None. Not modeled (VV:43). No native run. |
| Disposition | static review only. The check exists on one of the two archive paths. |

### 2.2 Git URL loads (OBS-L13)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-03 `LoadFromRepo`. OP-LOAD-02. |
| Code | A nonexistent path that starts with `http` passes the path checks (`Main.cls:2446-2452`). A scheme containing `http` calls `LoadFromRepo` (`:2462-2464`). It creates `mgr/Temp/<timestamp>/` (`:2352-2353`) and runs `git clone` through `RunCommandViaZF` (`:2356-2358`). That method stores the exit code in `pRetCode` and returns only the status of reading its output files (`Utils/Module.cls:2409-2421`). The caller does not ask for the code. Nothing removes the folder. |
| Commit point | The clone runs after the record exists (`Main.cls:2420`) and before `LoadNewModule`, so it is outside any IPM transaction. After a failed clone there is no `module.xml`. `GetModuleObjectFromPath` then throws (`Utils/Module.cls:341-343`, called at `:1164`) before the transaction, the guards and the no-transaction marker. |
| Record and readers | Failed clone: `Success` = error, `Committed=0`, empty `Package` (the name is set only at `:1293`). The message is "File not found to obtain module object", not git's. Successful clone: a folder load from the clone. In developer mode `Root` points into `mgr/Temp`. |
| Baseline | None. Listed in the original handoff under native checks not run. |
| Disposition | static review only. A native case needs an HTTP git remote. The follow-up environment runs only the IRIS and ORAS containers. |

### 2.3 Legacy XML loads (OBS-L16)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-18, legacy branch. |
| Code | Reached only from `LoadModuleReference` when the repository client has no `GetModuleDirectory` (remote or ORAS) and the payload is not gzip (`Utils/Module.cls:142-172`). It runs `$system.OBJ.Load` on the XML, then `LegacyCompat.UpdateSuperclassAndCompile` (`:175-185`). `LoadNewModule` is never called. |
| Commit point | No `tstart`, no marker, no phases, no dependency load, no lock file and no `<Defaults>` merge. A top-level `install` writes are durable at once. As a dependency, the writes sit inside the parent's transaction and roll back with it. |
| Record and readers | Top-level: `Install` finalizes with the returned status (`Main.cls:2666-2670`), so a failure after partial loading reads `Committed=0` while the loaded classes stay (OBS-L16). Dependency: its own record is finalized at `Utils/Module.cls:207-209`. `SetName` never runs, so `Package` is the requested name. |
| Baseline | None (A3:262 "not run"). |
| Disposition | static review only. A native case needs a registry that serves a non-gzip payload. R05d deferred registry sources. |

### 2.4 Temporary repository cleanup (INT-LOAD-14, `load.dependencies.local-repo`)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-04 `TempLocalRepoManager`. |
| Code | Top-level load: the manager is created before `LoadNewModule` and `CleanUp` runs after it (`Main.cls:2527-2531`). Dependencies: `Utils/Module.cls:137-140` (repository folder) and `:256-259` (archive). Creation is skipped when the folder is missing or another repository already has that root (`TempLocalRepoManager.cls:33-47`). Otherwise `BuildCache` saves the definition in its own transaction (`Repo/Filesystem/Definition.cls:132-176`) at sort order -1000 (`TempLocalRepoManager.cls:28`). `CleanUp` deletes it (`:73-86`). If creation fails, `%OnNew` returns an error (`:64-68`) and `%New` yields `""`. `CleanUp` at `Main.cls:2530` then throws `<INVALID OREF>`. |
| Commit point | Top-level: the repository is durable before the load's `tstart` (`Utils/Module.cls:1195`) and is deleted after the load committed or rolled back. A process end between `Main.cls:2528` and `:2530` leaves it registered at first priority (I2). Dependencies: created and deleted inside the parent's open transaction. |
| Record and readers | D13: the `<INVALID OREF>` comes after C3 and `SetCommitted` (`Utils/Module.cls:1347-1351`), so the record reads `Success` = error and `Committed=1` with the module installed. Dependency records name the repository as their source (`ipm-temp-modules-1`, native A0, Q-15). `repo -list` shows a leftover repository. |
| Baseline | D13: B1 `case_c29_error_after_commit`, `VR/193952Z-replay-return-b`. I2: `VR/210229Z-temp-repos-leftover`. The repository lifecycle is not modeled. |
| Disposition | native or suite evidence only. |

### 2.5 Lock files (OBS-L3, OBS-L11, `deps.lockfile`)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-13 `LockFile:CreateLockFileForModule`. |
| Code | `LoadNewModule` names the lock-file module once, for the top module (`Utils/Module.cls:1323-1327`). `LoadDependencies` writes the file after every dependency is loaded and only prints an error (`:1415-1422`). The path is `module.Root` plus `module-lock.json` (`LockFile.cls:82`). `Root` is set and saved only when `params("DeveloperMode")` is defined (`Utils/Module.cls:1298-1304`). `install` always defines it (`Main.cls:2623`). Dependencies get it from `LoadModuleFromDirectory` (`Utils/Module.cls:309-311`). `load` and `update -path` without `-dev` or `-nodev` do not, so a first load writes the file relative to the process directory (OBS-L3). |
| Commit point | The file is written inside the load's open transaction: before C3 in normal mode, after C1 and before C2 in developer mode. A later failure rolls back the database but not the file (OBS-L11). |
| Record and readers | None. A lock-file error does not change the record. No IPM operation reads the file. Install from a lock file is future (CAT-35). |
| Baseline | None (VV:31). |
| Disposition | static review only. Both native cases are ordinary loads and were not run. |

### 2.6 Namespace defaults (OBS-X1, OBS-H2, CAT-27)

| Aspect | Trace |
|---|---|
| Operations | CMD-28 `ProcessDefaultModifiers`. EP `ShellInternal`. |
| Code | Stored in `^%IPM.DefaultModifiers` (`inc/IPM/Common.inc:31`, `Main.cls:1682-1696`). `ShellInternal` merges them into every command's `data` before the parsed command (`Main.cls:1067-1070`). Every `-D` name without a dot is also copied under `data("zpm",...)` (`CLI.cls:815-819`). `load` copies `data` into its params (`Main.cls:2454`). `LoadNewModule` merges the module's `<Defaults>` under those params (`Utils/Module.cls:1165-1169`). A dependency receives its parent's merged params (`:1328`, `:1411`, `:308`). Order of precedence: command line, namespace defaults, the top module's `<Defaults>`, the dependency's own `<Defaults>`. |
| Commit point | The merged `NoTransaction` and `NoJournal` decide whether an activation opens a transaction (`:1172`, `:1191-1196`). So they decide which of C1, C2 and C3 exist and whether the marker runs (`:1226-1227`). |
| Record and readers | `history` takes `data("zpm")` as its filter (`Main.cls:4136`), so stored defaults become filters (D18). An unknown column gives an SQL error result that `find` shows as an empty list with OK status (H-3, N-R04-UNKNOWN). The global is shared by every namespace (D19). |
| Baseline | D19: stage C X1 (`VR/204152Z-stagec`). D18: stage C H2. Flag precedence per activation: `LT` (L-1), N-R05-PARDEF. Reader side: `HR`. |
| Disposition | native or suite evidence only for D19. D18 waits on Q-18. |

### 2.7 Dependency selection (CAT-18, `deps`, `load.dependencies`)

| Aspect | Trace |
|---|---|
| Operations | OP-LOAD-13 `LoadDependencies`. OP-LOAD-18 `LoadModuleReference`. |
| Code | `LoadDependencies` builds the graph (`Utils/Module.cls:1380`), flattens it in dependency order (`:1391`), skips modules the graph marked installed (`:1407-1410`) and loads the rest with the top's params (`:1411`). The graph keeps an installed version that satisfies the range (`Storage/Module.cls:1139-1155`). `PermitDowngrade` relaxes only the graph (`:1157`). Per dependency, the developer-mode guard needs `params("Install")` (`Utils/Module.cls:109`). `load` never sets it (`Main.cls:2457-2460`) and `install` removes it before the top's activation (`Utils/Module.cls:318-319`), so it never applies to a dependency (L-5). Then the check against installed modules' requirements (`:116-126`) and the `LoadNewModule` guards (`:1213-1222`). |
| Commit point | Every dependency loads inside the top's transaction. In developer mode that is the second transaction after C1, committed at C2. A dependency's own `tcommit` is nested. A dependency whose own `<Defaults>` set `NoJournal` writes outside the journal and escapes the rollback (D14). Its own `NoTransaction` only skips its own `tstart` (N-R05-Q23F). |
| Record and readers | Each dependency has its own record (`Utils/Module.cls:63-70`). Its Action is `install` only under `install`. Its durable twin is inserted at the parent's level (`HistoryTemp.cls:21`) and is lost when the parent rolls back (D1). |
| Baseline | B1 (up to two leaf dependencies). `LT` with the nesting reduction (L-3). `R05`: N-R05-NESTF, N-R05-NESTS, N-R05-DIA, N-R05-HIGHER, N-R05-HIGHEROK, N-R05-DEVUP, N-R05-DEVSAME, N-R05-PARDEF, N-R05-Q23F, N-R05-Q23S, all conforming. |
| Disposition | native or suite evidence only for selection outcomes. Remote and ORAS sources were not run (R05d). |

### 2.8 install, reinstall, update and uninstall

| Command | Record | Refusals before the record exists | Shared core and commit points | Evidence |
|---|---|---|---|---|
| `install` | `InstallInit` at `Main.cls:2566`, before the argument checks at `:2572-2585`. `SetSource`, `SetVersion` at `:2631-2632`. Finalize at `:2667` and `:2670`. | None. Its own argument refusals are recorded. | `LoadQualifiedReference` with the log (`:2633`), then the folder, archive or legacy XML branch (2.1, 2.3). C1 to C3 as for `load`. | Suite InstallModule. Not modeled. |
| `reinstall` | Calls `Install` (`Main.cls:2698`), so Action is `install`. | "not currently installed" at `:2686-2688` (Q-09). | As `install`, pinned to the installed version (`:2695-2697`, Q-19). | None beyond `install`. |
| `update` | `UpdateInit` at `Main.cls:4438`, then `Load` for `-path` or `Install`, with that log. | `PermitDowngrade` at `:4415-4417`, module not installed at `:4422-4424` (Q-09). | `LoadNewModule` with `isUpdate`. It marks resources (`Utils/Module.cls:1231-1248`) and crashes at `:1232` when the archive holds another, uninstalled module (D16). It runs ApplyUpdateSteps (`:1345`). | Suite Update. Stage C S3, S3N1. |
| `uninstall` | `UninstallInit` at `Storage/Module.cls:734`, Finalize at `:750-756`. | No module name at `Main.cls:2710-2712` (Q-09). | No `LoadNewModule` and no transaction, so `Committed` comes only from Finalize. A required module is skipped with OK (`Lifecycle/Base.cls:290-293`). Self-uninstall finalizes and persists before Clean (`Storage/Module.cls:630-643`). Full cleanup kills the shared temp global (`Lifecycle/Module.cls:51`). | Suite History. OBS-L17, OBS-H8 code-read. OBS-H13, OBS-H15 not run. |

### 2.9 test and verify dependency loads (CMD-8, CMD-10)

`%Test` and `%Verify` call `LoadDependencies` with a phase list (`Lifecycle/Base.cls:1261`, `:1557`). The command opens no IPM transaction, so each dependency is a top-level activation with its own `load` record. `verify` loads the module into `<NS>-VERIFY` with `NoJournal=1` (`:1527`) through `LoadModuleFromDirectory` with no log (`:1551`), so the module itself gets no record there. Static review only.

### 2.10 The shared history writer and its readers

- `Init` saves the IRISTEMP row and its durable twin at the current `$TLEVEL` (`AbstractHistory.cls:51-63`, `HistoryTemp.cls:9-26`). A twin saved inside a parent's transaction rolls back with it (D1).
- `SetCommitted` and `Finalize` write only the IRISTEMP row (`AbstractHistory.cls:119-134`). `Finalize` sets `Committed` to `Committed` or OK.
- `%OnClose` copies the IRISTEMP row into the twin, sets `Finalized=1` and deletes the temp row (`HistoryTemp.cls:30-73`).
- Readers: `history find` and `details` (`Main.cls:4127-4149`, `History.cls:93-148`), `delete` (`History.cls:151-159`, `:199-230`), the purge (`History.cls:162-181`, called at `Main.cls:862`), and the `Records` and `GlobalRecords` queries. All are in `HR` and the native run `R04`. `Records` (OBS-H11) is code-read only.

### 2.11 What the trace adds or corrects

1. OP-LOAD-08 is not structural. Its block dereferences `moduleCurrent`, which is `""` when the archive holds a module that is not installed (`Utils/Module.cls:1207`, `:1231-1232`). That is the D16 crash site. Section 1e reclassifies the A3:116 row as an in-scope part.
2. The top-level `.modules` repository is committed before the load's transaction starts. This is why a process end leaves it registered (I2). Dependency repositories live inside the parent's transaction.
3. A failed git clone fails before any transaction or marker, so its record reads `Committed=0` with an empty `Package`. It is not a D12 case. The clone folder also stays after a successful clone.
4. `install` records its own argument refusals because `InstallInit` runs first. `update`, `reinstall` and `uninstall` refuse before their record exists. This asymmetry is the as-built side of Q-09.
5. The lock file is a file write inside the open transaction. Its folder depends on whether the command defined `DeveloperMode`, so OBS-L3's wrong folder applies to `load` and `update -path` only.
6. Locator drift with no effect on any conclusion. Finding L-1 and the `loadtx.py` docstring cite `Utils/Module.cls:1323` for the dependency call, which is at `:1328` (`:1323` drops `ModuleName`). The `history_readers.py` docstring cites `Main.cls:4135` for the filter merge (it is at `:4136`) and `:4161-4173` for delete (it is at `:4164-4176`).

## 3. Counts

Each row has exactly one disposition. Denominators are the rows in the family. The A3 and VV families count coverage rows, so an ID can appear in more than one family.

### 3.1 By family and disposition

| Family | Rows | Original model | Follow-up model | Native or suite | Static only | Not run | Future | Decision pending |
|---|---|---|---|---|---|---|---|---|
| A1 commitments | 35 | 7/35 | 1/35 | 6/35 | 14/35 | 1/35 | 1/35 | 5/35 |
| A1 catalog | 42 | 2/42 | 1/42 | 5/42 | 28/42 | 0/42 | 4/42 | 2/42 |
| A2 observations | 34 | 4/34 | 3/34 | 4/34 | 17/34 | 2/34 | 0/34 | 4/34 |
| A2 commands | 39 | 1/39 | 1/39 | 5/39 | 32/39 | 0/39 | 0/39 | 0/39 |
| A3 section 1 rows | 51 | 4/51 | 1/51 | 6/51 | 26/51 | 1/51 | 5/51 | 8/51 |
| A3 section 2 rows | 9 | 1/9 | 0/9 | 1/9 | 7/9 | 0/9 | 0/9 | 0/9 |
| A3 section 3 rows | 10 | 0/10 | 0/10 | 0/10 | 1/10 | 0/10 | 9/10 | 0/10 |
| A3 section 4 rows | 32 | 4/32 | 2/32 | 6/32 | 14/32 | 1/32 | 0/32 | 5/32 |
| A3 section 9 rows | 23 | 5/23 | 4/23 | 6/23 | 2/23 | 1/23 | 0/23 | 5/23 |
| VV coverage rows | 38 | 8/38 | 2/38 | 9/38 | 9/38 | 1/38 | 0/38 | 9/38 |
| All rows | 313 | 36/313 | 15/313 | 48/313 | 150/313 | 7/313 | 19/313 | 38/313 |

Totals over all 313 rows: 36 modeled in original model, 15 modeled in follow-up model, 48 native or suite evidence only, 150 static review only, 7 not run, 19 future or not implemented, 38 decision pending.

### 3.2 By scope class, and regression versus scope exclusion

| Family | Rows | formal: load | formal: history | in-scope part | excluded feature | structural | Regression | Scope exclusion | n/a |
|---|---|---|---|---|---|---|---|---|---|
| A1 commitments | 35 | 17 | 16 | 1 | 1 | 0 | 1 | 1 | 33 |
| A1 catalog | 42 | 3 | 1 | 17 | 21 | 0 | 0 | 24 | 18 |
| A2 observations | 34 | 9 | 15 | 7 | 3 | 0 | 1 | 3 | 30 |
| A2 commands | 39 | 1 | 1 | 8 | 28 | 1 | 0 | 28 | 11 |
| A3 section 1 rows | 51 | 17 | 13 | 12 | 9 | 0 | 1 | 12 | 38 |
| A3 section 2 rows | 9 | 0 | 0 | 1 | 5 | 3 | 0 | 5 | 4 |
| A3 section 3 rows | 10 | 0 | 0 | 2 | 8 | 0 | 0 | 9 | 1 |
| A3 section 4 rows | 32 | 13 | 13 | 5 | 1 | 0 | 1 | 1 | 30 |
| A3 section 9 rows | 23 | 8 | 13 | 2 | 0 | 0 | 1 | 0 | 22 |
| VV coverage rows | 38 | 17 | 20 | 1 | 0 | 0 | 1 | 0 | 37 |
| All rows | 313 | 85 | 92 | 56 | 76 | 4 | 6 | 83 | 224 |

6 rows are marked regression. All of them are D15 (`.tar.gz`) or rows that carry it. 83 rows are scope exclusions. 224 rows are n/a.

### 3.3 Decision-pending rows by question

A row can wait on more than one question, so this table counts row-question pairs. No row waits only on Q-22 or Q-23. The rows for D14 and the Q-23 case (A3:156, A3:257) are modeled in follow-up model, because L-4 holds under either Q-22 option.

| Q | Rows | Row IDs |
|---|---|---|
| Q-01 | 2 | load.refusal.update-required.dev-bypass (Q-02), dev-mode.sticky (Q-01) (A3:248); dev-mode.sticky (VV:56) |
| Q-02 | 4 | INT-LOAD-10; INT-LOAD-10 (A3:26); load.refusal.update-required.dev-bypass (Q-02), dev-mode.sticky (Q-01) (A3:248); load.refusal.update-required (VV:36) |
| Q-05 | 5 | INT-LOAD-06; OBS-H16; INT-LOAD-06 (A3:22); history.committed.nested (Q-05) (A3:264); history.committed.nested (VV:75) |
| Q-08 | 2 | history.record.abnormal-end (A3:165); history.record.abnormal-end (VV:64) |
| Q-11 | 2 | INT-HIST-12; INT-HIST-12 (A3:53) |
| Q-14 | 6 | CAT-06; OBS-H14; A1 3.2 Phases (A3:45); CAT-06 (A3:72); history.phases (A3:166); history.phases (VV:70) |
| Q-15 | 1 | history.fields.dependency-source (VV:69) |
| Q-17 | 8 | INT-LOAD-11; CAT-05; OBS-L10; INT-LOAD-11 (A3:27); CAT-05 (A3:71); load.refusal.requirements (A3:150); load.refusal.requirements.timing (Q-17) (A3:249); load.refusal.requirements (VV:37) |
| Q-18 | 3 | OBS-H2; history.read.filter (A3:173); history.read.filter.default-modifiers (Q-18) (A3:259) |
| Q-20 | 4 | INT-HIST-12; INT-HIST-12 (A3:53); history.delete.running-records (Q-20) (A3:269); history.delete.running-records (VV:82) |
| Q-21 | 4 | INT-HIST-14; INT-HIST-14 (A3:55); history.locked (A3:171); history.locked.unfinished (VV:84) |

## 4. What remains unestablished

94 rows in formal scope or in an in-scope part are static review only or not run (A1 commitments 15, A1 catalog 10, A2 observations 16, A2 commands 3, A3 section 1 rows 22, A3 section 3 rows 1, A3 section 4 rows 14, A3 section 9 rows 3, VV coverage rows 10). They come down to these items.

Rests on reading only:

- Git URL loads (OBS-L13, section 2.2). The clone stays and the git exit code is ignored. Needs an HTTP git remote to run.
- Legacy XML loads (OBS-L16, section 2.3). No transaction, `Committed=0` after partial writes. Needs a registry serving a non-gzip payload.
- Write check (INT-LOAD-16, section 2.1). Only registry archives are checked. Needs an unwritable `ModuleRoot`.
- Lock files (OBS-L3, OBS-L11, section 2.5). Wrong folder on a first plain load, file kept after a rollback. Ordinary, not run.
- Archive folder replaced before the version guards (OBS-L7) and every other non-database effect of a rollback (Q-04).
- `update m -dev` from a registry ends in normal mode (OBS-L4). Needs a registry source.
- Processor `Compile` errors ignored, so the load and its record read success (OBS-L8). Ordinary, not run.
- `PermitDowngrade` reaches the graph but not the downgrade refusal (OBS-L14).
- Uninstall of a required module, directly or through `-recurse`, reads success (OBS-L17, OBS-H8). Ordinary, not run.
- Top-level developer-mode overwrite guard (INT-LOAD-12, Q-03) and read-only repository forcing (INT-LOAD-13).
- Unrecorded refusals of `update`, `reinstall` and `uninstall` (Q-09) and the version `reinstall` uses (Q-19).
- A purge error stops the user's command (OBS-H12, Q-13). No ordinary input makes the purge fail.
- `Records` declares an ID column it does not return (OBS-H11) and the unreachable "IPM is not enabled" message (rest of OBS-H10).
- Listing shows source `n/a` for non-install records (OBS-H7, A3:169).
- `test` and `verify` dependency loads and the VERIFY namespace load (CMD-8, CMD-10, CAT-33).
- `-env`, `-bypass-py-deps` and `-skip-sync` (VV:31). `Committed` for non-database changes (Q-07).

Not run, because the run would be destructive or concurrent:

- Self-uninstall (OBS-H15) and concurrent loads during a full cleanup (OBS-H13, Q-16). Both need a disposable namespace or instance.

Outside this ledger's row families, still open from other findings:

- P-12 (IRISTEMP at restart), concurrency and global progress (R07c).
- B1-C49T and B1-C49K stay approximate (R06f).
- Remote and ORAS repositories as dependency sources (R05d).
- The install, update, reinstall and uninstall paths are not modeled. Suites cover them.

Established, but without a governing requirement or with a classification left open:

- L-5. A dependency installed in developer mode at another version is replaced and its developer mode cleared (N-R05-DEVUP). No requirement covers it. The developer-mode guard never applies to dependencies. Needs a new question or a decision under Q-03. The R09 rewrite of `bugs_issues.md` drafts it as I3.
- L-4. With a dependency's own `NoJournal`, a failed parent leaves that dependency installed while its own dependency rolls back (N-R05-NESTF). This holds under either Q-22 option. The R09 rewrite drafts it as D21.
- L-9 and L-12. Repair variant 103 meets the dependency clauses only when every activation uses the load's mode. It also needs a guarded close. The original handoff's "meets every dependency clause in default and developer modes" holds only for a global mode.
- I1 and I2 have no governing requirement.
- OBS-L14 is marked n/a, not regression. Before `f83b207e`, `LoadModuleReference` skipped its downgrade check under `PermitDowngrade`. That commit moved the check into `LoadNewModule`, where `PermitDowngrade` is not read. A3 disagreement 1 keeps `PermitDowngrade` out of documented intent. A maintainer comment on PR #887 says the version check "has always been present". If maintainers treat `PermitDowngrade` as supported, this becomes a second regression.

The R09 rewrite of `Verus_verification/bugs_issues.md` was uncommitted while this ledger was written. It keeps the D and I numbers cited here and drafts new IDs for items this ledger names by finding: D21 (L-4), D22 (L-8), I3 (L-5), I4 (H-5), I5 (OBS-L8), I6 (files outliving a rollback, Q-04), I7 (OBS-L14) and I8 (`NoLock`).

New evidence that appeared while this ledger was written and is not used here: `FR/native/20261114T000144Z-replay-return-rerun` and `FR/native/20261114T000251Z-replay-c51-c52-rerun`.

## 5. Citation check

Every product locator in section 2 was read in the source at the pin. These were checked line by line:

| Locator | Confirmed content |
|---|---|
| `Main.cls:2352-2358` | `mgr/Temp/<timestamp>/` created, `git clone` run through `RunCommandViaZF` with no exit code requested |
| `Utils/Module.cls:2409-2421` | exit code stored in `pRetCode`, method returns only the file-read status |
| `Main.cls:2466-2467` | `extension = $piece(path,".",*)`, so `"tar.gz"` never matches |
| `Main.cls:2505-2514` | existing `<ModuleRoot>/<name>/<version>` removed, recreated and filled with no write check |
| `Utils/Module.cls:228-233` | `Writeable` check and "Insufficient file system permissions" refusal |
| `Main.cls:2527-2531` | `.modules` manager created before `LoadNewModule`, `CleanUp` status added after it |
| `TempLocalRepoManager.cls:64-68`, `:73-86` | creation failure returns an error from `%OnNew`. `CleanUp` deletes the definition |
| `Repo/Filesystem/Definition.cls:132`, `:176` | `BuildCache` has its own `tstart` and `tcommit` |
| `Utils/Module.cls:1298-1304` | `Root` and `Repository` saved only when `params("DeveloperMode")` is defined |
| `Utils/Module.cls:1415-1422` | lock file written at the end of `LoadDependencies`, errors only printed |
| `LockFile.cls:82` | lock path is `module.Root` plus the file name |
| `Main.cls:1067-1070` | namespace defaults merged before the parsed command |
| `CLI.cls:815-819` | each `-D` name duplicated under `zpm` |
| `Main.cls:4136` | `history` filter is `data("zpm")` |
| `Utils/Module.cls:1165-1169` | module `<Defaults>` merged under the received params |
| `Utils/Module.cls:109` | developer-mode guard needs `params("Install")` |
| `Utils/Module.cls:318-319` | `Install` removed before the top's `LoadNewModule` |
| `Utils/Module.cls:156-185` | legacy XML branch loads with `$system.OBJ.Load`, no `LoadNewModule` |
| `Utils/Module.cls:1231-1232` | `moduleCurrent.GetResolvedReferences` with no check that `moduleCurrent` exists |
| `Main.cls:2566`, `:2572` | `InstallInit` before the "No module name specified" refusal |
| `Main.cls:4415-4424`, `:4438` | `update` refusals before `UpdateInit` |
| `Storage/Module.cls:734`, `:750-756` | uninstall record created inside `Storage.Module:Uninstall`, finalized with the status |
| `Lifecycle/Base.cls:290-293` | "Clean SKIPPED - required by" with `pSkip=1` and no error |
| `Lifecycle/Module.cls:51` | `kill ^IRIS.Temp.HistoryTempD` |
| `History.cls:28` | `Records` declares `CONTAINID = 1` with `Action` first |
| `Main.cls:862-865` | purge runs first and its error is returned before the command |
| `HistoryTemp.cls:21` | durable twin saved at `Init` |

`git show f83b207e` was read for the D15 regression and for the `PermitDowngrade` change on OBS-L14.
