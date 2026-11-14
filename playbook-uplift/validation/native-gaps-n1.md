# Native gaps closed by builder n1 (G1–G4)

Date 2026-11-13. IPM source: `git archive 571e04a1` (scratchpad tar), checkout untouched. All containers and networks
were named `ipm-uplift-n1-*` and have been removed, with the two images built here and the pulled 2025.3 image.
Evidence collected with `python3 -I scripts/collect_evidence.py` (319 observations, 125 test records after the last run).
Scripts: `validation/iris/v2/` (G1 copies), `validation/iris/n1/` (G2–G4, comparison and cross-reference helpers).

| Gap | Result |
|---|---|
| G1 second IRIS version | **Closed.** IRIS Community 2025.3: every orchestrator probe and scenario reran; 38 of 38 observations have the same values as on 2026.1; History class passes. |
| G2 rest of IPM's suites | **Closed.** 52 integration classes, one per call: 49 pass, 3 fail. Same suite as one verify call in CI form: 262 of 263 methods pass. Unit suite: 152 of 152 methods pass. One failure is a product defect, two come from how the harness ran them. |
| G3 legacy upgrade ZPM 0.7.4 → this IPM | **Closed.** CI migration job reproduced; IPM's migration test passes (5 methods, 24 assertions). 13 `nc-mig-*` observations. One new defect: the CLI scripts are not installed on this path. |
| G4 mirrors | **Not closable.** Community Edition refuses mirroring: `ERROR #2099: Mirroring is unavailable for current license` (`nc-mirror-license-refused`). |

## G1. Second IRIS version (logs 90–94)

**Picking the version** (`90-v2-image-select.log`). containers.intersystems.com lists iris-community 2025.2, 2025.3,
2026.1, 2026.2 (and latest-*). `docker pull ...:2024.1` and `...:2025.1` failed with "not found". IPM's
`images.yml` skips 2025.2 (expired license), so **2025.3** is the oldest version CI still targets. Build:
`2025.3.0.226.0com`, "IRIS for UNIX (Ubuntu Server LTS for ARM64 Containers) 2025.3 (Build 226U) Thu Nov 13 2025",
image digest `sha256:4d0acf3e7138c89bf4062a585038558efe4296e828ebe73a30c8b33f30600f94`.

**Setup.** Container `ipm-uplift-n1-iris2025` with zot `ipm-uplift-n1-oras1` (alias `oras`, IPM's tests/oras config)
on network `ipm-uplift-n1-g1net`. IPM installed from the archive with `IPM.Installer.setup("/home/irisowner/zpm/",3)`
→ 0.10.11-SNAPSHOT, then `^UpliftObsSuffix="@2025.3"` (`v2/00-install-ipm.script`, `90-v2-install-ipm.log`).

**Runs, scripts unchanged unless noted.** `20-env-probes` → `90-v2-env-probes.log` (9 obs); `30-ipm-scenarios` →
`90-v2-ipm-scenarios.log` (18 obs); `31-halt-{dev,notxn,txn}-a/b`, `32-halt-commit-a/b`, `33-reload`,
`33-dephalt-{0,1}-a/b` → `91-v2-*.log` (7 obs); `34-extra` → `92-v2-extra.log` (4 obs);
`v2/35-autopurge-cause.script` (only change: the suffix is appended to the observation name, as Env.Emit does) →
`92-v2-autopurge-cause.log`; `v2/11-ipm-history-class.script` (only change: prints
`RESULT Test.PM.Integration.History@2025.3` so the collector records it) → `93-v2-ipm-history-class.log`:
13 methods, 450 assertions, all pass. (A first run with the unchanged script also passed; its log was overwritten by
the second run.)

**Comparison** (`n1/compare_versions.py` → `94-v2-compare-2026.1-vs-2025.3.log`): 38 observations, **38 same, 0
different**. The only key that differs is `cls:UpDepHalt.Hooks`, which the current Scenario.cls reports but the older
probe that made the 2026.1 `30-*` logs did not; it is listed as NOTE and not compared. So F-HIST-1 to F-HIST-6 and
the environment contract reproduce on 2025.3 exactly.

| Observation (2025.3) | Ontology statements | Note |
|---|---|---|
| `env-finalized-trigger@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-halt-open-transaction@2025.3` | SYS-D16 supports | same values as 2026.1 |
| `env-iristemp-survives-rollback@2025.3` | HIST-D8 supports | same values as 2026.1 |
| `env-nested-commit-outer-rollback@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-onclose-inside-caller-transaction@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-onclose-outside-transaction@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-resave-after-rollback@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-rollback-all@2025.3` | (environment contract, `models/explorer`; no statement cites it directly) | same values as 2026.1 |
| `env-rollback-one-level@2025.3` | LOAD-B15 supports (cited in its Notes); environment contract | same values as 2026.1 |
| `ipm-activate-fails@2025.3` | LOAD-B8 supports; HIST-B3 supports; HIST-B9 supports; HIST-B12 supports | same values as 2026.1 |
| `ipm-dep-ok@2025.3` | LOAD-B6 supports; LOAD-B25 supports; HIST-D9 supports; HIST-B18 supports; HIST-B20 supports | same values as 2026.1 |
| `ipm-dep-parent-fails@2025.3` | LOAD-B8 supports; HIST-B3 supports; HIST-B19 contradicts (F-HIST-1) | same values as 2026.1 |
| `ipm-dev-activate-fails@2025.3` | LOAD-B16 supports; LOAD-B18 supports; HIST-B14 supports | same values as 2026.1 |
| `ipm-dev-dep-parent-fails@2025.3` | LOAD-B17 supports; HIST-B18 supports; HIST-B20 supports | same values as 2026.1 |
| `ipm-dev-downgrade-refused@2025.3` | LOAD-B10 supports; LOAD-B19 supports; HIST-B7 contradicts (F-HIST-5); HIST-B12 supports; HIST-B15 supports | same values as 2026.1 |
| `ipm-dev-missing-dependency@2025.3` | LOAD-B16 supports; LOAD-B27 supports; HIST-B14 supports | same values as 2026.1 |
| `ipm-dev-then-plain-load@2025.3` | LOAD-B21 supports | same values as 2026.1 |
| `ipm-failed-load-inside-caller-transaction-committed@2025.3` | LOAD-B15 supports | same values as 2026.1 |
| `ipm-finalized-history-row@2025.3` | HIST-D7 supports; HIST-B22 supports; HIST-B25 supports; HIST-B29 supports | same values as 2026.1 |
| `ipm-halt-dev@2025.3` | LOAD-B18 supports; HIST-B10 supports | same values as 2026.1 |
| `ipm-halt-dev-crash-in-dependency@2025.3` | LOAD-N3 supports; HIST-B11 contradicts (F-HIST-2) | same values as 2026.1 |
| `ipm-halt-notxn@2025.3` | LOAD-B23 supports; HIST-B17 supports | same values as 2026.1 |
| `ipm-halt-txn@2025.3` | LOAD-B9 supports; HIST-B2 supports; HIST-B7 contradicts (F-HIST-5); HIST-B12 supports; HIST-B21 supports | same values as 2026.1 |
| `ipm-halt-txn-crash-in-dependency@2025.3` | LOAD-B9 supports | same values as 2026.1 |
| `ipm-halt-txn-operator-commits@2025.3` | LOAD-N2 supports (F-HIST-4 reproduced); HIST-N2 supports (F-HIST-4 reproduced) | same values as 2026.1 |
| `ipm-history-autopurge@2025.3` | HIST-B26 contradicts (F-HIST-6) | same values as 2026.1 |
| `ipm-history-purge-filter-cause@2025.3` | HIST-B26 contradicts (F-HIST-6) | same values as 2026.1 |
| `ipm-install-from-fs-repo@2025.3` | LOAD-B28 supports; HIST-B1 supports | same values as 2026.1 |
| `ipm-load-inside-caller-transaction-rolled-back@2025.3` | SYS-D17 supports; LOAD-N1 supports (F-HIST-3 reproduced); HIST-N1 supports (F-HIST-3 reproduced) | same values as 2026.1 |
| `ipm-load-ok@2025.3` | LOAD-B6 supports; LOAD-B7 supports; HIST-B1 supports; HIST-B6 supports; HIST-B8 supports; HIST-B21 supports | same values as 2026.1 |
| `ipm-load-relative-path-refused@2025.3` | LOAD-B1 supports | same values as 2026.1 |
| `ipm-missing-dependency@2025.3` | LOAD-B8 supports; LOAD-B27 supports | same values as 2026.1 |
| `ipm-nojournal-activate-fails@2025.3` | LOAD-B23 supports; HIST-B16 supports | same values as 2026.1 |
| `ipm-notxn-activate-fails@2025.3` | LOAD-B23 supports; HIST-B13 supports; HIST-B16 supports | same values as 2026.1 |
| `ipm-terminate-dev@2025.3` | LOAD-B18 supports; HIST-B17 supports | same values as 2026.1 |
| `ipm-terminate-notxn@2025.3` | LOAD-B23 supports; HIST-B10 supports; HIST-B17 supports | same values as 2026.1 |
| `ipm-terminate-txn@2025.3` | LOAD-B9 supports; HIST-B2 supports; HIST-B10 supports; HIST-B21 supports | same values as 2026.1 |
| `ipm-update-with-path@2025.3` | LOAD-B28 supports; HIST-B1 supports | same values as 2026.1 |
| `Test.PM.Integration.History@2025.3` (T) | HIST-B1, B4, B5, B6, B8, B9, B14, B15, B16, B23, B24, B25, LOAD-B6 supports (the statements already tagged T:Test.PM.Integration.History) | 13/13 methods |

"Contradicts" rows are the existing findings reproduced on the second version; no new contradiction.

## G2. IPM's remaining suites (logs 95-suite-*)

**Setup, as in main.yml and docker-compose.yml** (`95-suite-build-images.log`, `95-suite-setup.log`). Private network
`ipm-uplift-n1-net2`:
- `registry`: image built from `tests/registry/Dockerfile` (REGISTRY namespace, IPM from source, zpm-registry 1.3.4
  from the community registry, admin/SYS), started with the CI `-a ... UnExpireUserPasswords("*")` command, alias
  `registry`. CI's extra `zpm "install zpm-registry"` in USER prints "IPM is not enabled in namespace USER" (logged).
- `oras`: `zot-linux-arm64:latest` with IPM's `tests/oras/zot-config.json` and `htpasswd`, alias `oras`.
- test container: image built from IPM's root `Dockerfile` (BASE iris-community:2026.1, runs `iris.script`:
  Installer.setup, community repo, vscode-per-namespace-settings, TestCoverage), started with `TEST_REGISTRY=http://registry:52773/registry/`,
  `TEST_REGISTRY_USER=admin`, `TEST_REGISTRY_PASSWORD=SYS`; source copied to /home/irisowner/zpm and chmod 777 as
  CI does. Reachability checked from the test container: registry `_ping` 200, `/v2/` on oras 200.

**Runs.** Scripts in `n1/suite/` (one per class), runner `n1/run_suite.sh`, 900 s timeout each.
- Unit, whole (`zpm test -only -quiet`): `95-suite-unit-ALL.log`: 152 methods, 1152 assertions, all pass.
- Unit, each of the 33 classes (`-DUnitTest.Case=...`): 33 OK (`95-suite-unit-*.log`; SemVer.Abstract has 0 methods).
- Integration, each of the 52 classes (`zpm verify -only -quiet -DUnitTest.Case=...`): **49 OK, 3 fail**
  (`95-suite-int-<Class>.log`).
- Follow-ups: the two harness-sensitive failures without `-quiet` (`n1/suite2/`, logs `95-suite-int-*-verbose.log`);
  the whole verify phase in CI form, `verify zpm -v -only` in one call (`95-suite-int-ALL-ci.log`, 2700 s timeout):
  **263 methods, 262 pass, 1 fail; 2943 assertions, 2 fail**; per class 50 passed, 3 passed with a skipped method,
  1 failed (ProcessPythonWheel).

Previously-unrun classes now passing (TEST-REFLECTION §C): InstallModule 7/7 methods, Oras 28/28, PublishModuleWithDeps
1/1, LockFile 10/10 (whole class, Test05 included), OrasDeployedPlatformFilter 7/7, and every other class except the
three below.

### Failures and their classification

| Test | Message | Class | Evidence |
|---|---|---|---|
| ProcessPythonWheel#TestPackageWithPythonDeps (per class and in the CI-form run) | `Loaded packaged module => ERROR #5001: Wheel file ".../wheels/markupsafe-3.0.4-cp312-cp312-manylinux2014_aarch64.manylinux_2_17_aarch64.manylinux_2_28_aarch64.whl" not found.` | **Product defect** (shows on ARM64 only) | The wheel name is exactly 100 characters. IPM's tar writer cuts every TRUNCATE header field to MAXLEN-1, so a 100-char `name` becomes 99 chars (`ipm/src/cls/IPM/Utils/FileBinaryTar.cls#L7`, `#L469`); there is no long-name (GNU/pax) header for longer names. Reproduced in `95-suite-tar-name-length.log`: `%IPM.General.Archive` Create+Extract of 98/99/100-char names gives `name100Extracted=0;truncated99CharFileExists=1`, and GNU `tar tzvf` of the IPM-written archive shows the entry as `...aaaa.wh` (OBS `nc-suite-tar-name-100-truncated`). The amd64 wheel name (`...x86_64.manylinux_2_28_x86_64.whl`) is 97 chars, so CI on ubuntu-latest does not hit it. |
| Module (per class only) | 12 failed assertions; root cause `Loaded varresolver module ... => ERROR #5001: This module requires an interoperability-enabled namespace. USER-VERIFY is not interoperability-enabled.` | **Environment (test-order dependency in the test)** | The generated manifest needs `Interoperability="enabled"` (`tests/.../Module.cls#L165`) but the test never enables it. In one verify run, Health's `OnBeforeAllTests` enables Interop for USER-VERIFY first (`Health.cls#L8`), and Module then passes (`95-suite-int-ALL-ci.log`: TestNestedPlaceholderVar assertions pass). Fails alone with or without `-quiet` (`95-suite-int-Module-verbose.log`). |
| TestOutputFormat#TestQuietOutput (per class with `-quiet` only) | `AssertTrue - summary reports 12 failed methods in quiet mode`; inner run printed `ERROR! ObjectScript error: <NOTOPEN>%OnClose+2^%IPM.Utils.OutputSuppressor.1` | **Environment (harness flag)**, exposing a small product defect | With `-v` instead of `-quiet` (the CI form) the class passes 17/17 (`95-suite-int-TestOutputFormat-verbose.log`) and it passes in `95-suite-int-ALL-ci.log`. The test runs `test ... -quiet` inside the outer `-quiet` run; the inner `OutputSuppressor` closes the null device the outer one still uses, then `use`s it (`ipm/src/cls/IPM/Utils/OutputSuppressor.cls#L25-L28`). Nested `-quiet` runs therefore fail; CI never nests them. |

Skipped methods seen in the CI-form run (pass counts include them): InstallApplication#TestWSGIApp ("only supported
in 2024+. Current version: Containers)": the test reads the version as `($zversion," ",9)`, which is
"Containers)" on this container build, `InstallApplication.cls#L36`; by inspection the amd64 container string has the
same word count, so the test is likely skipped in CI too, i.e. vacuous); InstallModule#TestUninstallDependencyDoesError
("functionality ... does not yet exist"); Resources#TestResourceTypesOnIRISForHealth (Health only);
ProcessPythonWheel#TestWheelAndReqsPresentOnline ("until python wheels fix is merged").

### G2 mapping to ontology

Class-level results are in `evidence/ipm-tests.json` as `Test.PM.Integration.<Class>` / `Test.PM.Unit.<Class>`.

| Test or observation | Statement | Relation |
|---|---|---|
| T:Test.PM.Integration.BatchActivate (pass) | NSP-B18, NSP-B23, NSP-B43 | supports (now run; X → T) |
| T:Test.PM.Integration.CopyDirOverlap | RES-B55 | supports |
| T:Test.PM.Integration.CustomPhase | RES-B42 | supports |
| T:Test.PM.Integration.DeployedItems | RES-B28 | supports |
| T:Test.PM.Integration.DeployedMappings | RES-B17, RES-B26, RES-B27 | supports |
| T:Test.PM.Integration.DeployedPackageLoad | RES-B22 | supports |
| T:Test.PM.Integration.ExternalNameModuleAction | RES-B47 | supports |
| T:Test.PM.Integration.FileCopy (whole class) | RES-B51 (and the existing UNIN-B18, B19, B22) | supports |
| T:Test.PM.Integration.Generate | RES-B81, RES-B82 | supports |
| T:Test.PM.Integration.Health | NSP-B68, NSP-B69 | supports |
| T:Test.PM.Integration.IncludeTests | RES-B13, RES-B14, RES-B15 | supports |
| T:Test.PM.Integration.InstallModule | INST-B17, INST-B24, INST-B26, INST-B27, INST-B28, INST-B30 | supports |
| T:Test.PM.Integration.Interoperability | NSP-B71 | supports |
| T:Test.PM.Integration.LoadModuleWithDeps | INST-B19 | supports |
| T:Test.PM.Integration.Oras | REPO-B17, B18, B19, B23, B24, B50, B52, B60, B64, B68, B71, B72 | supports |
| T:Test.PM.Integration.OrasDeployedPlatformFilter | DEP-B10, INST-B10, INST-B11, INST-B12 | supports |
| T:Test.PM.Unit.SemVer.Expressions | DEP-B9 | supports |
| T:Test.PM.Integration.SystemRequirements | NSP-B62, NSP-B63 | supports |
| T:Test.PM.Integration.VersionReq | NSP-B73 | supports |
| T:Test.PM.Integration.WebApplicationExport | UNIN-B11 | supports |
| ProcessPythonWheel methods TestPythonWheelResource (L113), TestWheelAndReqsPresentOffline (L175), TestNoWheelAndReqsPresentOffline (L187) passed; class failed | RES-B60, RES-B67, RES-B69 | supports at method level only (pass lines in `95-suite-int-ALL-ci.log`); no class-level T, the class record is "fail" |
| ProcessPythonWheel#TestPackageWithPythonDeps fail + O:nc-suite-tar-name-100-truncated | RES-B65 ("Packaging copies each wheel file into the package file"), RES-B71 ("... puts them in the package file") | **contradicts** for file names of 100+ characters (stored as a 99-char name). New finding candidate. |
| same | RES-B61 ("A missing wheel file fails the load") | supports |
| Module per-class fail; pass in whole run | (no statement cites Module.cls) | test isolation issue, no tag |
| TestOutputFormat fail with nested -quiet | (no statement covers -quiet nesting) | candidate finding (low): nested quiet runs raise `<NOTOPEN>` |

Statements already tagged T for other classes (BuildDependencyGraph, Sync, Update, LockFile, Show, Scopes, Invoke, ...)
were re-confirmed (all pass here).

## G3. Legacy upgrade ZPM 0.7.4 → this IPM (logs 96-migration-*)

Followed main.yml's "Restart temporary registry", "Test and publish to temporary registry" and "Clean install v0.7.x
and test migration to latest" (`96-migration-setup.log`):
1. Fresh `registry` container (same image as G2) on `ipm-uplift-n1-net2`.
2. Publisher `ipm-uplift-n1-pub` (base 2026.1): the archive with `module.xml` `<Version>` set to
   `0.10.11-beta.29` exactly as CI's "Setup version" does for a push after tag v0.10.11-beta.28; installed with
   Installer.setup (`96-migration-publish-install.log`), then `install dsw`, `install sslclient`, `publish zpm -v
   -export-python-deps` and `publish sslclient -v` → both OK; registry lists zpm 0.10.11-beta.29 and sslclient 1.0.4
   (`96-migration-publish.log`).
3. Migration container `ipm-uplift-n1-mig` (base 2026.1, test package in /tmp/test-package). Script
   `n1/96-migration-ci.script` = CI's heredoc without the `:1` halt-on-error suffixes, so every step is logged
   (`96-migration-ci.log`): ZPM 0.7.4 loaded from pm.community.intersystems.com into %SYS; `install dsw` (with
   mdx2json), repo `registry` → http://registry:52773/registry/, `install sslclient`; `install zpm -v` downloaded
   0.10.11-beta.29 from the local registry and installed it **in %SYS** (where 0.7.4 lived); `MapIfLegacy` printed
   "Mapping %IPM package in %SYS equivalently to USER"; `RunAll` migrated %SYS (zpm, repo registry) and USER (dsw,
   mdx2json, sslclient, repo registry). `test ipm-migration-v0.7-to-v0.9 -only`: **5 methods, 24 assertions, all pass**
   (GloballyEnabled, ModuleStorage), `RESULT ... OK`.
   The script's first probe line (`STEP legacy-version`) failed with `<INVALID OREF>` because it looked for the 0.7
   zpm record in USER; it was diagnostic only and changed nothing.
4. Probes after the upgrade: `96-migration-probes.log`, `-probes2.log`, `-probes3.log`.

**Collector artifact (needs the orchestrator):** the RESULT label `Test.PM.Migration (ipm-migration-v0.7-to-v0.9 test
phase) OK` contains spaces, so `collect_evidence.py` stores it as `Test.PM.Migration` with status **fail** and detail
"(ipm-migration... phase) OK". The log shows a pass (lines "Methods: 5 total, 5 passed, 0 failed", "RESULT ... OK").
Please correct that record or treat it as pass; I did not edit logs or scripts/.

Also: `nc-mig-ipm-mapped-globally` (first probe) reports `allNamespacesShareUserIPMdb=0` only because
`%SYS.Namespace.ListAll` returns an implicit entry `^^/usr/irissys/mgr/`; use `nc-mig-ipm-mapped-globally-configured`
(Config.Namespaces only) instead.

Other messages on this path: "ORAS installation timed out and may not have completed successfully." was printed, yet
`IPM.Installer.ORASInstalled()` returned 1 afterwards (`nc-mig-oras-python-installed`).

**New defect (no covering statement).** On the 0.7 upgrade path the `FileCopy` targets `${ipmdir}bin/ipm` and
`${ipmdir}bin/iriscli` (`ipm/module.xml#L57-L58`) are not expanded: the legacy lifecycle prints
"Copying .../ipm to /usr/irissys/mgr/${ipmdir}bin/ipm as file" and creates a literal `${ipmdir}bin` folder.
`InstallScripts` then warns "source script not found: /usr/irissys/ipm/zpm/0.10.11-beta.29/bin/ipm" and still prints
"Installed ipm and iriscli scripts to /home/irisowner/.local/bin/" (`ipm/preload/cls/IPM/Installer.cls#L586-L603`);
no script is there (`nc-mig-ipmdir-placeholder-unexpanded`: literalDirScriptExists=1, expectedBinScriptExists=0,
localBinIpm=0). Not seen on a fresh install (logs 00-*: "Installed ipm and iriscli scripts" with no warning).

| Test or observation | Values | Statement | Relation |
|---|---|---|---|
| T:Test.PM.Migration.GloballyEnabled (via the migration package test phase) | pass | NSP-B11 (X:GloballyEnabled.cls#L7) | supports (now run) |
| T:Test.PM.Migration.ModuleStorage | pass (TestSSLClient, TestDSW, TestZPM, TestRepos) | NSP-B15 (X:ModuleStorage.cls#L7, #L41) | supports |
| O:nc-mig-ipm-mapped-globally-configured | %SYS and USER both resolve %IPM to IRISSYS | NSP-B11, NSP-B19 (list includes %SYS) | supports |
| O:nc-mig-package-mappings | `%SYS>%IPM@IRISSYS USER>%IPM@IRISSYS` | NSP-B11 | supports |
| O:nc-mig-ipm-upgraded-in-sys | zpm 0.10.11-beta.29 recorded in %SYS, root `.modules/%SYS/zpm/...` | NSP-B11 | supports (context: the upgrade lands where 0.7 lived) |
| O:nc-mig-legacy-classes-remain | hasLegacyZPMPackage=1 after migration | NSP-B12; NSP-B75 | supports NSP-B12; shows NSP-B75's trigger stays true for every later IPM load (not itself tested) |
| O:nc-mig-module-records | %SYS: ipm=1 (zpm), legacy=1; USER: ipm=4 (dsw, mdx2json, sslclient + test pkg), legacy=3 | NSP-B15, NSP-B17 | supports (copy ran in both namespaces whose %IPM comes from IRISSYS) |
| O:nc-mig-repos-user, O:nc-mig-repos-sys | `registry` → http://registry:52773/registry/, user admin, enabled | NSP-B15 ("old registry entry keeps its URL") | supports |
| O:nc-mig-community-registry-enabled | 0 | NSP-B14 | supports |
| O:nc-mig-repo-storage-not-mapped | USER repo global in USER db, %SYS in IRISSYS, each has its own row | NSP-B13 | supports |
| O:nc-mig-ipmdir-placeholder-unexpanded, O:nc-mig-ipm-cli-scripts | scripts not installed, success printed | none (gap) | **new contradiction** of the installer's own message; candidate statement under NSP-B11: "After an upgrade from legacy ZPM, the ipm and iriscli scripts are installed." → contradicted |
| O:nc-mig-oras-python-installed | 1 | none | info |

Not exercised: NSP-B16 (one module fails to copy), NSP-B74 (`enable -map` and legacy records), NSP-B10.

## G4. Mirrors (logs 97-mirror-*)

Container `ipm-uplift-n1-mirA` (2026.1, hostname mira, ISCAgent running) on `ipm-uplift-n1-mnet`.
`$system.License.Decode()`: "Platform=IRIS Community", "Server=Single", Licensed Users=8 (`97-mirror-create-a.log`).
Enabling `%Service_Mirror` → `ERROR #2099: Mirroring is unavailable for current license`;
`SYS.Mirror.CreateNewMirrorSet("UPLIFTMIR","MIRA",...)` → `ERROR #2066: Mirroring Service is required but not enabled`;
`IsMember=0`, member status "Not initialized". Re-run as an observation in `97-mirror-refusal.log`
(`nc-mirror-license-refused`). A second member was not started since the first refused. Closing G4 needs a licensed
(non-Community) IRIS image and key.

| Observation | Statement | Relation |
|---|---|---|
| O:nc-mirror-license-refused | INST-B72, INST-B73, INST-B74, INST-B75 | neither; records why they stay I/N (no mirror possible on Community) |

## Revision

- 2026-11-13: created by builder n1.
