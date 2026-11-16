# Seed challenge — modules 07 (namespace), 08 (resources), 09 (sync)

Seed: docs/ipm-uplift/index.md ("Other intent already recovered"). Each seed row in these areas is split into its
claims, re-derived from source, tests, changelog and history at ipm 571e04a1, and given a verdict: **confirmed**,
**refined** (how), or **contradicted** (evidence). "X" means a test asserts it but was not run here. The verdicts
below were made by inspection; the candidate native checks were later run on IRIS (see "Native check results" at the end).

## Seed: Namespace ownership (→ 07)

| # | Seed claim | Verdict | Evidence | Statements |
|---|---|---|---|---|
| 1 | A fresh 0.9+ installation can keep the IPM version local to a namespace. | Confirmed | Installer maps `%IPM` to the current namespace's own routine DB and loads with `-DNoMapping=1` (Installer.cls#L25-29, L130); README#L68-70. | NSP-B1..B3 |
| 2 | ...and registry settings local to a namespace. | Refined | True for repo definitions (`^IPM.Repo.Definition*`, namespace globals). Not true for the IPM settings (`config`: default registry URL, pip caller, ModuleRoot, etc.), which live in `^IPM.settings` in %SYS and are server-wide (UniversalSettings.cls#L1, L174-176). A fresh install creates no repo at all; it only stores the community URL as the default (Installer.cls#L110-123). | NSP-B4..B8 |
| 3 | An operator can opt into global mapping. | Confirmed, refined | `enable -map -globally` (Main.cls#L3611-3708) includes %SYS (only `%ALL` dropped), skips namespaces with their own install, keeps an existing mapping unchanged even if it points at another source (Utils/Module.cls#L1631), and refuses `-version`. `-repos` is separate and only applies where code is mapped from the same namespace and no repos exist. X: BatchActivate.cls#L153-154, #L186-190. | NSP-B18..B29 |
| 4 | Upgrading a legacy installation restores global code mapping for compatibility. | Confirmed, refined | `MapIfLegacy` (Installer.cls#L133-139) runs `enable -globally -map` after Compile whenever `%ZPM.PackageManager.*` classes exist (Migration.cls#L38-42). Because those classes are server-wide, the trigger is "legacy classes exist anywhere", so any later IPM load (including `enable -ns X` copies) while they remain also maps globally (NSP-B75, inferred). Data migration (`RunAll`) copies module records and old repo entries (X: tests/migration/.../ModuleStorage.cls#L7, #L41); X: GloballyEnabled.cls#L6-7 (CI-only suite). | NSP-B11..B17, NSP-B74, NSP-B75 |
| 5 | Distinguish that transition from enabling the community registry. | Confirmed | Legacy upgrade maps code only, no `-repos`, and does not reset repos (Installer.cls#L136). Only `enable -community` runs `repo -delete-all`, `repo -reset-defaults`, `enable -map -repos -globally` (Main.cls#L3564-3575). The old ZPM repo entry is migrated, keeping its URL (ModuleStorage.cls#L41). `enable -community` continues past a failed step and returns success (DEC-NSP-4). | NSP-B13, B14, B30..B33 |

New in this area (not in seed): `install/reinstall/uninstall zpm` refused in a mapped namespace while the installer
and `load` are not (DEC-NSP-2); repo mapping can be stranded when a mapped namespace gets its own install
(DEC-NSP-1); `enable` from a registry ignores the load status (F-NSP-1); `version` prints build metadata against its
help (F-NSP-3); `unmap` never touches a source namespace and does not check for modules left behind.

## Seed: Manifest system requirements (→ 07)

| # | Seed claim | Verdict | Evidence | Statements |
|---|---|---|---|---|
| 6 | A module declaring `SYSNamespace="true"` refuses to load in an ordinary IRIS namespace. | Confirmed, refined | `CheckSYSNamespace` (SystemRequirements.cls#L133-139); X: SystemRequirements.cls#L28-29 (status and error text). Refinement: in developer mode the manifest record is committed before the check (Utils/Module.cls#L1311 vs L1321), so a refused `-dev` load leaves a module record (DEC-NSP-5); the test uses a plain load only. The check runs before dependencies are loaded. | NSP-B62, B64, B66 |
| 7 | It can load in `%SYS` when its other requirements are met. | Confirmed | X: SystemRequirements.cls#L35-36. In that test %SYS reaches IPM by mapping (`enable -map -globally`, L6). | NSP-B63 |
| 8 | The integration case checks the refusal and calls an installed class in the permitted namespace. | Confirmed | SystemRequirements.cls#L28-36. | — |
| 9 | (Brief) IRIS version/product requirements. | Refined | Version need compares major.minor only (L44-59); `Health` true/false/omitted (L72-83, X Health.cls#L18/#L27/#L37); Health true implies Interoperability; there is no `Product` attribute any more (a `Product` slot remains only in storage, L150-152). All failing needs are reported together. IPMVersion need ignores mapped IPM (F-NSP-2). | NSP-B65, B67..B73 |

## Seed: Resource scope (→ 08)

| # | Seed claim | Verdict | Evidence | Statements |
|---|---|---|---|---|
| 10 | An ordinary install leaves a test-scoped class absent. | Confirmed | Document.cls#L71-75, L194-198; X: Scopes.cls#L64 (class), #L71 (package). | RES-B2 |
| 11 | Running the module's `test` phase must load and compile the class... | Confirmed | Document.cls#L75 (`tInCurrentPhase`), L184; X: Scopes.cls#L100. Test-scoped dependencies are installed first (Base.cls#L1261; X: Scopes.cls#L129). | RES-B4, B5 |
| 12 | ...including after a developer-mode install. | Confirmed | X: Scopes.cls#L84-86 → #L96, #L100. Note `tSkipScoped` (Base.cls#L625) is dead code; dev mode does not load scoped resources. | RES-B3 |
| 13 | Test resources are excluded from packages by default. | Confirmed | Base.cls#L1306-1316; X: IncludeTests.cls#L29. | RES-B13 |
| 14 | ...and can be requested by flag or manifest. | Confirmed, refined | X: IncludeTests.cls#L54 (flag), #L104 (manifest). Refinement: both add Test **and** Verify scopes; the flag can only turn inclusion on, it cannot turn off a manifest `IncludeTests=1`; `publish` accepts the same flag. | RES-B14..B16 |

New in this area: deployed export swallows errors and could ship source (F-RES-1); `register` phase documented but
missing (F-RES-2); `makedeployed` in dev mode deploys classes despite its message (F-RES-3); packaging with Deploy
resources removes class source from the developer namespace (DEC-RES-3); `-bypass-py-deps` skips only
requirements.txt, wheels still install (X ProcessPythonWheel.cls#L175); wheels reinstall on every action that runs
Initialize; hooks: status checked only when declared `%Status`, thrown errors always fail, no up-front validation of
Class/Method; custom phases run alone and only on request (X CustomPhase.cls#L14/#L21/#L28); `exec` isolation vs
stale help (DEC-RES-1); `run-from-file` stops at first failure and its help examples disagree with the code
(F-RES-5, DEC-RES-2).

## Seed: Developer sync and retry (→ 09)

| # | Seed claim | Verdict | Evidence | Statements |
|---|---|---|---|---|
| 15 | After a failed compile, the next `sync` must reconsider the affected file. | Confirmed for failures **inside sync**; contradicted for other failed compiles | Sync failure → no commit, retry mark (Pipeline.cls#L146, L210-223); X: Sync.cls#L650. A failed `compile`, `load -dev` or `reload` outside sync sets no mark and leaves the old baseline (Base.cls#L1235-1246), so a revert is missed (F-SYNC-1, DEC-SYNC-1). | SYNC-B16..B22 |
| 16 | This includes restoring its exact earlier bytes: a hash match can otherwise hide stale server state. | Confirmed (sync path) | Pass 3 forces `SyncFailed` rows (FileHash.cls#L273-293); X: Sync.cls#L687 (hash equals baseline), #L695-700 (reported and live), #L708 (mark cleared). Same hazard remains for `-skip-sync` first sync (F-SYNC-2) and non-sync failures (F-SYNC-1). | SYNC-B18, B19 |
| 17 | A failing test after successful compilation has different consequences for the file baseline. | Confirmed | Baselines committed before tests (Pipeline.cls#L146-153); failure is not marked for retry; X: Sync.cls#L723-724, #L732-733. Further: a plain `sync` of a changed test file sets a test mark so the next `sync -test` runs it once (X: Sync.cls#L320, #L329, #L337). | SYNC-B23..B31 |

New in this area: `sync` (all modules) returns success when a module failed (DEC-SYNC-3); `reload` of a non-dev
module is a silent no-op (DEC-SYNC-4); a successful compile keeps retry and test marks (harmless re-sync);
unrouted changed files are warned and re-detected; module.xml changes reload the manifest within the same sync.

## Candidate native checks

Run in a scratch namespace on the experiment's IRIS (orchestrator-owned). Expected outcome is what the ontology
statement (intent) predicts; "code predicts" is where inspection suggests otherwise.

| # | Statement(s) | Experiment | Expected (intent) | Code predicts |
|---|---|---|---|---|
| N1 | NSP-B1..B4 | Fresh IRIS, load the IPM installer in USER. Then in %SYS: `Config.MapPackages.Exists(ns,"%IPM")` for every other namespace; in USER `repo -list`. | Only USER has a `%IPM` mapping (to its own DB); no repos listed; `^|"%SYS"|IPM.settings("default_registry")` = community URL. | Same. |
| N2 | NSP-B18, B20, B29 | From USER (IPM installed): create NS A (empty) and B (`enable -map` already from another source C). Run `enable -map -ns A,B`. Read `##class(%SYS.Namespace).GetPackageDest(ns,"%IPM")`. | A points at USER's DB; B behaviour per decision. | A → USER DB; B stays on C's DB although the command printed "Mapping ... to B". |
| N3 | NSP-B23..B25 | `enable -map -repos -ns A` where A already has one repo of its own. | A keeps its repo; "Skipping repository mapping ... IPM repositories found". | Same. |
| N4 | NSP-B33 / DEC-NSP-4 | Force one of the three sub-commands to throw (e.g. hold a lock on `^IPM.Repo.DefinitionD` from another process so `repo -delete-all` fails), then check the status of `##class(%IPM.Main).Shell("enable -community")`. Note: an empty default registry does not count, because `repo -reset-defaults` then only writes "Error retrieving default registry URL" and quits without an error (Main.cls#L2225-2227). | (per decision) | Red error line printed; later steps still run; status OK. |
| N5 | NSP-B40 / F-NSP-1 | `enable -ns T -v <bad version> -remote <repo>` against a registry stub returning a non-compilable installer. | Error status, T not reported enabled. | "IPM enabled for namespace T", status OK. |
| N6 | NSP-B41..B44 | After N2, `unmap -ns A` then `unmap -ns USER`. | A loses `%IPM` and repo mappings; USER untouched with "No mapping found". | Same. |
| N7 | NSP-B47, B49 / DEC-NSP-2 | In mapped namespace A: `install zpm` vs. loading the installer XML. | per decision | `install` refused with "mapped from namespace USER"; installer run gives A a local install. |
| N8 | NSP-B62, B66 / DEC-NSP-5 | In USER: `load -dev <sys-ns-module>`; then `list`. | Load refused; per decision whether `sys-ns-module` is listed. | Refused, but `sys-ns-module` record present (DeveloperMode=1). |
| N9 | NSP-B72 / F-NSP-2 | `enable -map -ns A`; in A load a copy of `version-req-test/version-req-satisfied` with `IPMVersion=">=0.9"`. | Loads. | Fails: current IPM version read as empty/0.0.0. |
| N10 | NSP-B57 / F-NSP-3 | Install a `zpm-registry` or `zpm` version with `+build`; run `version`. | No `+build` shown. | `+build` shown. |
| N11 | RES-B2..B5 | Already X in Scopes.cls; run `Test.PM.Integration.Scopes` as T evidence. | Pass. | Pass. |
| N12 | RES-B23 / DEC-RES-3, RES-B31 / F-RES-3 | `load -dev` deployed-mappings fixture; (a) `makedeployed deployed-mappings`; (b) fresh, `package deployed-mappings -path /tmp/p`. Check `%Dictionary.ClassDefinition.%OpenId("DeployMapA.First").Deployed`. | (a) 0 per message; (b) per decision. | (a) 1; (b) 1. |
| N13 | RES-B49 / F-RES-2 | `<any module> register`. | Package registered in local cache. | `<METHOD DOES NOT EXIST>`. |
| N14 | RES-B67, B63 | Module with a wheel and requirements.txt; `load -bypass-py-deps`; then `<module> compile -v`. | Wheel installed; requirements skipped; compile runs pip again for the wheel. | Same. |
| N15 | RES-B36..B39 | Already X in Invoke.cls; plus a hook method that throws with `CheckStatus="0"`. | Load fails. | Fails (exception caught to status). |
| N16 | RES-B73 / DEC-RES-1 | `zpm "cos set $namespace=""%SYS"""` then `write $namespace` in the same process. | per decision | Original namespace. |
| N17 | RES-B75..B77 / F-RES-5 | `run-from-file` with (a) `cmds.text`, (b) JSON root array, (c) `.txt` whose 2nd of 3 lines fails. | (c): stop after line 2. | (a) "Unsupported file extension", (b) "Expected JSON object", (c) third command not run. |
| N18 | SYNC-B15 / F-SYNC-1 | sync-test fixture `-dev`; make SuperClass extend a missing class; `compile sync-test` (fails); restore exact bytes; `sync sync-test -verbose`. | "Updated: src/cls/SyncTest/SuperClass.cls"; class compiled, `GetValue()` = "original". | "Nothing to sync"; class not compiled. |
| N19 | SYNC-B15, B40, B41 / F-SYNC-2 | `load -dev -skip-sync` sync-test; edit SuperClass "original"→"edited"; `sync`; `sync`; call `GetValue()`. | "edited". | "original"; second sync "Nothing to sync". |
| N20 | SYNC-B46 / DEC-SYNC-3 | Two dev modules, one with a broken edit; `zpm "sync":1`. | per decision | Process continues; summary shows ERROR; status OK. |
| N21 | SYNC-B51 / DEC-SYNC-4 | `load` (no -dev) any fixture; `reload <module>`. | per decision | "Reload START ... SUCCESS", nothing loaded. |
| N22 | SYNC-B20 | sync-test: break SuperClass, `sync` (fails, retry mark); fix to new bytes; `compile sync-test` (works); `sync -verbose`. | Harmless re-sync of SuperClass ("Retrying 1 file(s)"). | Same. |

## Native check results

Run 2026-11-13 on IRIS Community 2026.1 build 234U, IPM 0.10.11-SNAPSHOT (571e04a1), container `ipm-uplift-iris-nsp`
(N1 in a throwaway fresh container). Probes: `validation/iris/nsp/` (classes `Uplift.Nsp.*`, scripts 80-94); logs:
`validation/logs/8*-nsp-*.log`. Scratch namespaces NCA, NCB, NCD, NCE, NCT (no IPM; the `%IPM` package mapping that
`%IPM.Test.Utils.CreateNamespace` adds was removed first), NCC (own clean local install), NCX (re-point probe).
"As code predicted" means the "Code predicts" column above held.

| Check | Result | Observations |
|---|---|---|
| N1 | As intended. Only USER has `%IPM` package and routine mappings (to USER); %SYS resolves `%IPM` to IRISLIB; no repos ("No repositories found."); default registry is the community URL in `^IPM.settings` of %SYS. | O:nc-nsp-N1-fresh-install-all-ns (the first read, O:nc-nsp-N1-fresh-install, listed no namespaces through SQL and is not used for mappings) |
| N2 | As code predicted. NCA resolves to USER's database; NCB stays on NCC's database although "Mapping %IPM package in USER equivalently to NCB" was printed; NCC skipped ("IPM already installed"). | O:nc-nsp-N2-enable-map |
| N3 | Differs. NCA keeps its own repo ("IPM repositories found") and NCB (code from NCC) is skipped, as predicted. But NCD and NCE, which had no IPM, got the code mapping and no repo mapping in the same run ("IPM not mapped from source namespace"); running the command again mapped their repos. New F-NSP-4. | O:nc-nsp-N3-enable-map-repos, O:nc-nsp-N3-enable-map-repos-rerun |
| N4 | As code predicted. With another job holding `^IPM.Repo.DefinitionD`, each step printed a red error (`%DeleteExtent could not delete all instances`, then `Failed to acquire read lock`), the next step still ran, and the command returned OK. The mapping step also skipped repos of newly mapped namespaces (F-NSP-4). | O:nc-nsp-N4-community-step-fails |
| N5 | As code predicted. Stub built: python http.server in the container serving `packages/zpm/9.9.9/installer` (a class with a missing superclass) behind Remote repo `nc-nsp-stub`. Compile error shown, then "IPM enabled for namespace NCT", status OK, NCT without IPM. | O:nc-nsp-N5-enable-remote-broken |
| N6 | Differs. `unmap -ns NCE,NCA` removed the package mapping (and NCE's repo mapping) but left the `%IPM.*` routine mapping; IPM still runs in NCA. `unmap -ns USER`: "No mapping found", USER untouched, as predicted. New F-NSP-6. | O:nc-nsp-N6-unmap-mapped, O:nc-nsp-N6-unmap-recheck-nca, O:nc-nsp-N6-unmap-recheck-nce, O:nc-nsp-N6-shell-after-unmap, O:nc-nsp-N6-unmap-source |
| N7 | Differs in part. `install zpm` refused as predicted. The installer returned OK but did not give NCD a local install: its mappings still point at USER, it gained a `zpm` record, and the install compiles through the mapping into USER's database (shown by the re-point probe, where USER's `%IPM.Main` compile time moved); `unmap -ns NCD` then found "No mapping". `%Installer` never re-points an existing mapping. New F-NSP-5. | O:nc-nsp-N7-install-zpm-mapped, O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install, O:nc-nsp-installer-repoint, O:nc-nsp-installer-existing-pkg-mapping |
| N8 | As code predicted. Refused; `sys-ns-module` record kept with developer mode on and listed. Plain load leaves no record. | O:nc-nsp-N8-sysns-dev |
| N9 | As code predicted, after a fixture change. `>=0.9` is refused in USER too (0.10.11-SNAPSHOT is a prerelease), so `>=0.10.11-SNAPSHOT` was used: loads in USER, refused in NCA with "Current IPM version is .0.0". | O:nc-nsp-N9-ipmversion-mapped, O:nc-nsp-N9-ipmversion-mapped-snap |
| N10 | As code predicted. `version` shows 1.2.3+ncbuild; `info` shows 1.2.3. | O:nc-nsp-N10-version-build |
| N11 | As intended. Test.PM.Integration.Scopes: 6 methods pass, 65 assertions pass, 1 skipped (registry setup; TEST_REGISTRY unset, the rest of that method ran). | T:Test.PM.Integration.Scopes, O:nc-nsp-ipm-test-run-1-assert-status |
| N12 | As code predicted. (a) `makedeployed` on a `-dev` module printed "will only report" yet set Deployed 0 to 1 and hid the source; (b) `package` of a `-dev` module: Deployed=1; no "Module package generated" line without `-v`. | O:nc-res-N12a-makedeployed-dev, O:nc-res-N12b-package-dev, O:nc-res-N12c-package-verbose |
| N13 | As code predicted, plus a difference. Lone `register`: `<METHOD DOES NOT EXIST>` `*%Register`. Help example `clean register`: OK, runs Clean through Package, no Register step. | O:nc-res-N13-register |
| N14 | As intended. Wheel installed, requirements.txt skipped with a message; `compile -v` installs the wheel again; without the flag the offline requirements.txt fails the load. | O:nc-res-N14-bypass-py-deps |
| N15 | As intended. Test.PM.Integration.Invoke: 2 methods, 16 assertions pass. A hook that throws with CheckStatus 0 fails the load. Extra: a missing hook method on a custom phase loads and fails only when that phase is run (any case). | T:Test.PM.Integration.Invoke, O:nc-nsp-ipm-test-run-4-assert-status, O:nc-res-N15-hook-throw-and-badname |
| N16 | As code predicted. INSIDE:%SYS during the command, USER after it. | O:nc-res-N16-exec-namespace-v2 (first attempt, O:nc-res-N16-exec-namespace, failed in the harness capture and is not used) |
| N17 | As code predicted. (a) "Unsupported file extension: text"; (b) "Expected JSON object at root level"; (c) line 1 ran, line 2 failed, line 3 not run; a `commands` object runs. | O:nc-res-N17-run-from-file-v2 (first run reused a repo path and is not used) |
| N18 | As code predicted. "Nothing to sync"; the server keeps the broken class (superclass NcMissing.Base, not compiled). | O:nc-sync-N18-compile-fail-revert |
| N19 | As code predicted. "Baseline established", then "Nothing to sync"; GetValue() "original". | O:nc-sync-N19-skip-sync-first-sync |
| N20 | As code predicted. Process continued, summary "sync-test ERROR", other module synced, status OK. | O:nc-sync-N20-sync-all-one-fails |
| N21 | As code predicted. "Reload START ... SUCCESS", status OK, edited file not loaded. | O:nc-sync-N21-reload-non-dev |
| N22 | As intended. Compile keeps SyncFailed=1; next sync: "Retrying 1 file(s)", file updated, mark cleared. | O:nc-sync-N22-retry-mark-after-compile |
| Sync suite | Test.PM.Integration.Sync is self-contained: 34 methods, 176 assertions pass, none skipped. | T:Test.PM.Integration.Sync, O:nc-nsp-ipm-test-run-5-assert-status |
| Open question: re-point | Settled: `%Installer` ClassMapping and RoutineMapping only add a missing mapping; an existing `%IPM` mapping is kept (iris.cpf unchanged). | O:nc-nsp-installer-repoint, O:nc-nsp-N7-installer-in-mapped |
| Open question: MigrateRepoDefinitions | Settled: on success it returns an OK status; `$$$ThrowOnError` expands to `Set sc=...`, so `sc` is defined. | O:nc-nsp-migrate-repo-defs-success |

Not run: legacy ZPM upgrade (NSP-B11..B17, B74, B75; needs a ZPM 0.7 install) and F-RES-1 (needs a patched processor).


## Review fixes (2026-11-13, REVIEW.md findings 1, 8, 13-15)

Run on `ipm-uplift-iris-nsp` (IRIS 2026.1, IPM 571e04a1). Probe class `validation/iris/nsp/Uplift/Nsp/Fix.cls`; scripts
`validation/iris/nsp/80-84-nsp-fix-*.script`; logs `validation/logs/80-84-nsp-fix-*.log`.

| Check | Result | Observations |
|---|---|---|
| N20 re-capture | The first capture's `summaryLine` matched the wrong output line ("[USER/sync-test]Sync START"). Re-run of the same steps, reading each summary line by its own label: "Modules checked: 2 (sync-test, sync-dep-c)", "Modules updated: 1", "sync-test ERROR", "Errors: sync-test"; `sync` returned OK; sync-dep-c reads c-edited; sync-test has SyncFailed=1. | O:nc-nsp-fix-sync-N20-summary |
| NSP-B3 with other namespaces | The installer ran in a new namespace NCFRESH3 (no %IPM mapping) while 17 other namespaces existed, 9 of them running IPM. No other namespace's %IPM package, routine or repo mapping, cpf IPM lines, or %IPM / %IPM.Main.1 / ^IPM.Repo.DefinitionD destination changed; NCFRESH3 got pkg and rtn mappings to its own database and a zpm record. | O:nc-nsp-fix-B3-fresh-install-many-ns |
| T reruns | 13 IPM test classes cited by T tags in 07-09 whose earlier runs were in removed containers, rerun one class per `zpm verify -only -quiet -DUnitTest.Case=<Class>` call: BatchActivate, SystemRequirements, Health, Interoperability, VersionReq, IncludeTests, DeployedMappings, DeployedPackageLoad, DeployedItems, CustomPhase, ExternalNameModuleAction, CopyDirOverlap, Generate. All OK; per-class assertions from `^UnitTest.Result` (log 54-unittest-skips-ipm-uplift-iris-nsp): 0 failed and 0 skipped for each. None needed the absent `registry` service. Scopes (run earlier here) keeps its 1 skipped assertion (Base.Setup's registry step). Test.PM.Migration was not rerun: it needs legacy ZPM 0.7 from the community registry and the `registry` service. | T:Test.PM.Integration.* (logs 82-84-nsp-fix-tests-*) |
