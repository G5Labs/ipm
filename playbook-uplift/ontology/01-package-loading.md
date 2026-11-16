# 01 — Package loading

Module root: LOAD-0

Scope: the `load` command and the shared routine (`%IPM.Utils.Module:LoadNewModule`) that `install`, `update` and
dependency loads also end in. History fields are module 02; install-versus-update rules are module 03; dependency
resolution is module 04. Models: `models/explorer` (L2 over the IRIS contract) and `models/verus/load_general.rs`.

## Glossary
| Term | Defined by |
|---|---|
| load | LOAD-D1 |
| module folder | LOAD-D2 |
| archive | LOAD-D3 |
| developer mode | LOAD-D4 |
| no-transaction load | LOAD-D5 |
| refusal | LOAD-D6 |
| kept | LOAD-D7 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| LOAD-0 | SYS-0 | R | IPM can bring a module into a namespace from files on disk. | I | ipm/src/cls/IPM/Main.cls#L268 | Module root. |
| LOAD-D1 | LOAD-0 | D | A **load** is the command that reads a module from disk and installs it. | I | ipm/src/cls/IPM/Main.cls#L268-L320 | `load <path>`. Install, update and dependency loads reuse the same routine (`Utils/Module.cls#L1152`). |
| LOAD-D2 | LOAD-0 | D | A **module folder** is a folder that holds a manifest file. | I | ipm/src/cls/IPM/Main.cls#L318 | `module.xml` at the folder's top. |
| LOAD-D3 | LOAD-0 | D | An **archive** is one packed file that holds a module folder. | I | ipm/src/cls/IPM/Main.cls#L2466-L2521 | `.tgz` or `.tar.gz`. |
| LOAD-D4 | LOAD-0 | D | **Developer mode** is a way to load a module so a developer can fix it in place. | I | ipm/src/cls/IPM/Main.cls#L306 | `-dev`. Help: "transactions will not be rolled back if installation fails". |
| LOAD-D5 | LOAD-0 | D | A **no-transaction load** is a load that runs without a transaction. | I | ipm/src/cls/IPM/Utils/Module.cls#L1172 | `-DNoTransaction=1`, or `-DNoJournal=1` (which also turns journaling off for the process, `#L1191-L1193`). |
| LOAD-D6 | LOAD-0 | D | A **refusal** is a failure that IPM finds before it changes anything. | I | ipm/src/cls/IPM/Utils/Module.cls#L1213-L1222 | Path checks, downgrade and update-step guards. |
| LOAD-D7 | LOAD-0 | D | A change is **kept** when it is still there after the command is over. | I | ipm/src/cls/IPM/General/AbstractHistory.cls#L41 | Database changes only (PR #578 review; DEC-HIST-5 resolved). Intent: DEC-HIST-5 resolved (evidence), decisions/research/hist.md. |
| LOAD-B1 | LOAD-0 | B | A load needs a full path to a module folder or an archive. A short path is refused. | O:ipm-load-relative-path-refused; I | ipm/src/cls/IPM/Main.cls#L2444-L2452 | "The specified path must be absolute." A URL starting with http is also accepted (LOAD-B3). |
| LOAD-B2 | LOAD-0 | B | A load of an archive unpacks it into the module root under its name and version. Any old copy there is deleted first. | I; O:nc-gap-P4-txn-good-archive; O:nc-gap-P4-txn-bad-archive-same-version; F:F-LOAD-3 | ipm/src/cls/IPM/Main.cls#L2498-L2515 | The folder is replaced before any check, and a rollback does not restore it. Committed covers database changes only (DEC-HIST-5 resolved), so this stays a defect of its own (F-LOAD-3). |
| LOAD-B3 | LOAD-0 | B | A load from a web address first copies the code to a temp folder. Then it loads that folder. | I; O:nc-gap-P5-url-ok-txn; O!:nc-gap-P5-url-missing; F:F-LOAD-1; F:F-LOAD-2 | ipm/src/cls/IPM/Main.cls#L2347-L2373 | `git clone`, optional `-branch`. Needs git on the server. |
| LOAD-B4 | LOAD-0 | B | If the module folder has a .modules folder, IPM uses it as a registry for this load only. | I | ipm/src/cls/IPM/Main.cls#L2523-L2530 | `TempLocalRepoManager`, cleaned up after the load. |
| LOAD-B5 | LOAD-0 | B | The manifest file must hold exactly one module. Otherwise the load fails. | I | ipm/src/cls/IPM/Utils/Module.cls#L1254-L1264 | "No module definition found." or "module.xml contains multiple documents." |
| LOAD-B6 | LOAD-0 | B | A load first records the new manifest. Then it loads the needs of the module. Then it runs the phases up to Activate. | O:ipm-load-ok; O:ipm-dep-ok; T:Test.PM.Integration.History | ipm/src/cls/IPM/Utils/Module.cls#L1251-L1346 | Phases Initialize, Reload, Validate, Compile, Activate are recorded in history. |
| LOAD-B7 | LOAD-B6 | B | A normal load runs inside one transaction. It commits only after Activate works. | O:ipm-load-ok; M:explore-L1-SUCCESS-STATE; P:g_refines; M:fold-abstraction-check-general; O:nc-gap-P9-chain-ok-txn | ipm/src/cls/IPM/Utils/Module.cls#L1195, #L1347-L1351 | Premise P1 (no caller transaction). Generalized (any dependency count, any prior state; load_general.rs) under P1c, P2, P3, PC, PI. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| LOAD-B8 | LOAD-B7 | B | When a normal load fails, nothing it did stays. This includes its dependencies. | O:ipm-activate-fails; O:ipm-dep-parent-fails; O:ipm-missing-dependency; M:explore-L1-ATOMIC; O:nc-gap-P9-upgrade-parent-fails-txn; O:nc-gap-P9-two-deps-parent-fails-txn; O:nc-gap-P5-url-fail-txn; M:fold-abstraction-check-general; O!:r4-hist-post-commit-throw-r4h-msg2-txn; F:F-LOAD-4 | ipm/src/cls/IPM/Utils/Module.cls#L1358-L1361 | Rolls back to the level it started at. In transactional mode it needs only the built-in premises P1–P3 (`lemma_refines_txn_mode`); P2 excludes a terminal operator commit. Generalized: any number of dependencies, pre-installed versions restored (O:nc-gap-P9-upgrade-parent-fails-txn). Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. No P since round 4: lemma g_refines (premises P1, P2, P3, PC, PI) proves this only under PM, and PM fails. A manifest whose AfterInstallMessage has a $$$ macro that does not compile makes a normal load fail after its last commit with every change kept (r4-hist-post-commit-throw-r4h-msg2-txn; F-LOAD-4; LOAD-N4). Part B U1, decisions/research/hist.md. |
| LOAD-B30 | LOAD-B8 | B | The compiled code of a failed normal load is undone as well. | O:ipm-activate-fails; O:ipm-dep-parent-fails; O:nc-gap-P9-two-deps-parent-fails-txn | ipm/src/cls/IPM/Utils/Module.cls#L1358-L1361 | Observed only: no model has a compiled-code component (review finding 6). |
| LOAD-N4 | LOAD-B8 | N | A normal load can fail after its last commit. Then its changes stay, and its record is committed. | O:r4-hist-post-commit-throw-r4h-msg2-txn; I | ipm/src/cls/IPM/Utils/Module.cls#L1348-L1354; ipm/src/cls/IPM/Utils/Module.cls#L2781 | Part B U1, decisions/research/hist.md. WriteAfterInstallMessage runs after the outer TCOMMIT; a $$$ macro that does not compile throws there. Observed: command error, module installed, class compiled, record success err, committed 1, ended, finalized. The same in no-transaction and developer mode (r4-hist-post-commit-throw-r4h-msg2-notxn, -dev). This exception is the defect F-LOAD-4 against LOAD-B8; it is not tagged F here because the code does what this row says. |
| LOAD-B9 | LOAD-B7 | B | When a normal load crashes, nothing it did stays. | O:ipm-halt-txn; O:ipm-terminate-txn; O:ipm-halt-txn-crash-in-dependency; M:explore-L1-ATOMIC; P:g_refines; O:nc-gap-P1-docker-txn-after; O:nc-gap-P1-force-txn-after | ipm/src/cls/IPM/Utils/Module.cls#L1195 | Holds when the open transaction rolls back (a job, or a terminal answer of R). See LOAD-N2. Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| LOAD-B10 | LOAD-B6 | B | A load is refused if the module is already installed at a newer version. | O:ipm-dev-downgrade-refused; X:ipm/tests/integration_tests/Test/PM/Integration/History.cls#L311; M:explore-L1-REFUSE | ipm/src/cls/IPM/Utils/Module.cls#L1213-L1217 | Also with `-force` and `-dev` (module 03). The installed version stays. Versions are not represented in load_general.rs; the version guard is modelled in update_steps.rs. |
| LOAD-B11 | LOAD-B6 | B | A load of a newer version with update steps is refused. The operator must use update, or add -force. | I; X:ipm/tests/integration_tests/Test/PM/Integration/InstallModule.cls#L90 | ipm/src/cls/IPM/Utils/Module.cls#L1218-L1222 | Only for `load` and `install`, only when the new manifest names an UpdatePackage. Developer mode skips this guard (LOAD-B31). |
| LOAD-B12 | LOAD-B11 | B | With -force, a newer version loads without running its update steps. | I; X:ipm/tests/integration_tests/Test/PM/Integration/InstallModule.cls#L90 | ipm/src/cls/IPM/Main.cls#L2453 | Module 03 records that skipped steps are then marked done. |
| LOAD-B31 | LOAD-B11 | B | A load -dev of a newer version with update steps goes ahead. It does not run the update steps. | I | ipm/src/cls/IPM/Utils/Module.cls#L1219 | Intended: TestDevMode doc comment (InstallModule.cls#L161-L162) covers install and load; only install -dev is asserted (L178). Steps seeded per DEC-INST-2 (F-INST-8 today). README L62 names only -force (F-INST-11). Intent: DEC-INST-1 resolved (evidence), decisions/research/inst.md. |
| LOAD-B13 | LOAD-B6 | B | A newer version with no update steps loads like a first install. It replaces the old record. | I; O:nc-gap-P9-upgrade-ok-txn | ipm/src/cls/IPM/Utils/Module.cls#L1250-L1251 | `OBJ.Load` of module.xml overwrites the stored module. |
| LOAD-B14 | LOAD-B6 | B | Loading the same version again reloads and recompiles it. | I | ipm/src/cls/IPM/Utils/Module.cls#L1213-L1222 | No guard fires when versions are equal. |
| LOAD-B15 | LOAD-B6 | B | The load returns to the transaction level it started at. It never undoes work that it did not start. | O:ipm-failed-load-inside-caller-transaction-committed; I; O:ipm-load-keeps-caller-work | ipm/src/cls/IPM/Utils/Module.cls#L1162, #L1358 | `tInitTLevel`; rolls back one level at a time (O:env-rollback-one-level). Native: a caller's own write made before a failed load survives the load and the caller's commit. |
| LOAD-B16 | LOAD-0 | B | In developer mode, the module record is saved and kept first. This happens before its needs load. | O:ipm-dev-activate-fails; O:ipm-dev-missing-dependency; M:explore-L1-DEV-KEEP; O:nc-gap-P1-docker-dev-after; O:nc-gap-P9-chain-c-fails-dev | ipm/src/cls/IPM/Utils/Module.cls#L1298-L1319 | First TCOMMIT at #L1311. A later failure, even a missing dependency, leaves the record (with no compiled code). No P: the L1 does not express the ordering (review finding 5). |
| LOAD-B17 | LOAD-0 | B | In developer mode, the needed modules load in their own transaction. It is kept if they all load. | O:ipm-dev-dep-parent-fails; O:nc-gap-P9-upgrade-dep-fails-dev | ipm/src/cls/IPM/Utils/Module.cls#L1318, #L1335-L1339 | No P: the L1 does not express the dependency transaction (review finding 5). |
| LOAD-B18 | LOAD-0 | B | In developer mode, Activate runs with no transaction. Its changes stay if it fails. | O:ipm-dev-activate-fails; O:ipm-terminate-dev; O:ipm-halt-dev; O:nc-gap-P1-docker-dev-after; O:nc-gap-P4-dev-bad-archive-same-version | ipm/src/cls/IPM/Utils/Module.cls#L1335-L1346 | Compiled classes stay (`UpParent.Hooks` compiled after the failure). |
| LOAD-B19 | LOAD-0 | B | If the load is refused early in developer mode, nothing stays. | O:ipm-dev-downgrade-refused; X:ipm/tests/integration_tests/Test/PM/Integration/History.cls#L304; P:g_refines | ipm/src/cls/IPM/Utils/Module.cls#L1195-L1217 | Premises P1, P2, P3, PC, PI of models/verus/load_general.rs `g_refines` (any number of dependencies, any prior state; in transactional mode PC and PI hold trivially). PM fails (F-LOAD-4): a post-commit error reports failure with changes kept; the history record stays honest. |
| LOAD-B20 | LOAD-0 | B | The system check runs before the developer-mode commit. So a refused module leaves no record. | I; F:F-NSP-12; O!:nc-nsp-N8-sysns-dev | ipm/src/cls/IPM/Utils/Module.cls#L1311, #L1321 | Code: the check (L1321) runs after the first developer-mode commit (L1311), so a refused module keeps its record (F-NSP-12). Module 07 (NSP-B66). Intent: issue #106 spec, wiki 03 SystemRequirements. Intent: DEC-NSP-5 resolved (assumption), decisions/research/nsp.md. |
| LOAD-B21 | LOAD-0 | B | A plain load keeps the developer mode of an installed module. Only -nodev turns it off. | O:ipm-dev-then-plain-load; I | ipm/src/cls/IPM/Utils/Module.cls#L1298-L1307; ipm/src/cls/IPM/Main.cls#L307 | `DeveloperMode` comes from the stored record when the flag is absent (code since the 2021 import, 9713db77). Intended: help -nodev 'Disables the DeveloperMode flag' (Main.cls#L307); wiki 00 Developer Mode (a dev-mode module is not overwritten by a non-dev install). A plain update from a registry does clear it (Utils/Module.cls#L309-L310; F-INST-5). Intent: DEC-LOAD-2 resolved (evidence), decisions/research/hist.md. |
| LOAD-B22 | LOAD-0 | B | A module from a read-only registry loads in normal mode. | I | ipm/src/cls/IPM/Utils/Module.cls#L1277-L1288 | |
| LOAD-B23 | LOAD-0 | B | A no-transaction load keeps every change it makes, even when it fails. | O:ipm-notxn-activate-fails; O:ipm-nojournal-activate-fails; O:ipm-halt-notxn; O:ipm-terminate-notxn; X:ipm/tests/integration_tests/Test/PM/Integration/History.cls#L326; O:nc-gap-P1-docker-notxn-after; O:nc-gap-P9-chain-c-fails-notxn; O:nc-gap-P5-url-fail-notxn | ipm/src/cls/IPM/Utils/Module.cls#L1172, #L1225-L1228 | Applies to `-DNoJournal=1` too. |
| LOAD-B24 | LOAD-0 | B | The needs of a no-transaction load also run with no transaction. | I; O:nc-gap-P9-chain-c-fails-notxn; O:nc-gap-P9-two-deps-parent-fails-notxn | ipm/src/cls/IPM/Utils/Module.cls#L306-L320 | All params pass down except developer mode, UpdateSnapshots and IgnoreInstalled. |
| LOAD-B25 | LOAD-B6 | B | Needed modules load before the module itself, one at a time, in order. | O:ipm-dep-ok; I; O:nc-gap-P9-chain-ok-txn; O:nc-gap-P9-chain-c-fails-txn | ipm/src/cls/IPM/Utils/Module.cls#L1368-L1413 | Each dependency load writes its own history record (module 02). |
| LOAD-B26 | LOAD-B25 | B | A needed module from a registry loads in normal mode. A snapshot from a local folder keeps developer mode. | I; O:nc-gap-P9-chain-ok-dev | ipm/src/cls/IPM/Utils/Module.cls#L128-L139, #L306-L314 | |
| LOAD-B27 | LOAD-B25 | B | If a needed module cannot be found, the load fails. | O:ipm-missing-dependency; O:ipm-dev-missing-dependency | ipm/src/cls/IPM/Utils/Module.cls#L1380-L1381 | "Could not find satisfactory version of up-dep in any repositories." |
| LOAD-B28 | LOAD-0 | B | Install from a registry and update end in the same load routine. | O:ipm-install-from-fs-repo; O:ipm-update-with-path | ipm/src/cls/IPM/Utils/Module.cls#L40-L211, ipm/src/cls/IPM/Main.cls#L4438-L4446 | Update runs ApplyUpdateSteps in place of Activate (`#L1345`). |
| LOAD-B29 | LOAD-B28 | B | An update with a path must point at the module being updated. | I | ipm/src/cls/IPM/Utils/Module.cls#L1269-L1275 | Name or ExternalName must match. |
| LOAD-N1 | LOAD-B15 | N | IPM does not shield a load from a caller transaction. If the caller rolls back, the load is undone too. | O:ipm-load-inside-caller-transaction-rolled-back; P:g_caller_rollback_leaves_no_trace | ipm/src/cls/IPM/Main.cls#L857 | Its history record is undone as well (module 02, HIST-N1). Premise P1 of the models. The proof states exactly this row (a caller rollback leaves no trace); premises: none. Allowed and to be documented (wiki 00 expects an outer wrapping transaction); F-HIST-3 is now a documentation gap. Intent: DEC-HIST-4 resolved (assumption), decisions/research/hist.md. |
| LOAD-N2 | LOAD-B9 | N | At a terminal, a crash asks the operator to commit or undo. If the operator commits, a half-done load stays. | O:ipm-halt-txn-operator-commits; I | validation/logs/32-halt-commit-a.log | IRIS behaviour, premise P2 of the models. A documented operator error; IPM does not guard against it (F-HIST-4 is now documentation). Intent: DEC-LOAD-1 resolved (assumption), decisions/research/hist.md. |
| LOAD-N3 | LOAD-B16 | N | If a developer-mode load crashes while its needs load, the module record stays and the needs are undone. | O:ipm-halt-dev-crash-in-dependency; O:nc-gap-P9-chain-c-fails-dev | ipm/src/cls/IPM/Utils/Module.cls#L1311-L1338 | Expected by developer mode; the defect is in its history record (F-HIST-2). |

## States
| ID | State |
|---|---|
| S1 | The module is not installed. |
| S2 | An older version is installed, and the new manifest has no update steps. |
| S3 | An older version is installed, and the new manifest has update steps. |
| S4 | The same version is installed. |
| S5 | A newer version is installed. |
| S6 | The module is installed in developer mode. |

## Commands
| ID | Command |
|---|---|
| K1 | `load <folder>` |
| K2 | `load -dev <folder>` |
| K3 | `load <folder> -DNoTransaction=1` |
| K4 | `load -force <folder>` |
| K5 | `load <archive>` |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| LOAD-C1 | S1 | K1 | LOAD-B7, LOAD-B8, LOAD-B9 | |
| LOAD-C2 | S1 | K2 | LOAD-B16, LOAD-B17, LOAD-B18 | |
| LOAD-C3 | S1 | K3 | LOAD-B23 | |
| LOAD-C4 | S1 | K4 | LOAD-B7, LOAD-B8 | |
| LOAD-C5 | S1 | K5 | LOAD-B2, LOAD-B7 | |
| LOAD-C6 | S2 | K1 | LOAD-B13, LOAD-B8 | |
| LOAD-C7 | S2 | K2 | LOAD-B13, LOAD-B16 | |
| LOAD-C8 | S2 | K3 | LOAD-B13, LOAD-B23 | |
| LOAD-C9 | S2 | K4 | LOAD-B13 | |
| LOAD-C10 | S2 | K5 | LOAD-B2, LOAD-B13 | |
| LOAD-C11 | S3 | K1 | LOAD-B11 | |
| LOAD-C12 | S3 | K2 | LOAD-B31 |  |
| LOAD-C13 | S3 | K3 | LOAD-B11 | |
| LOAD-C14 | S3 | K4 | LOAD-B12 | |
| LOAD-C15 | S3 | K5 | LOAD-B2, LOAD-B11 | |
| LOAD-C16 | S4 | K1 | LOAD-B14 | |
| LOAD-C17 | S4 | K2 | LOAD-B14, LOAD-B16 | |
| LOAD-C18 | S4 | K3 | LOAD-B14, LOAD-B23 | |
| LOAD-C19 | S4 | K4 | LOAD-B14 | |
| LOAD-C20 | S4 | K5 | LOAD-B2, LOAD-B14 |  |
| LOAD-C21 | S5 | K1 | LOAD-B10 | |
| LOAD-C22 | S5 | K2 | LOAD-B10, LOAD-B19 | |
| LOAD-C23 | S5 | K3 | LOAD-B10 | |
| LOAD-C24 | S5 | K4 | LOAD-B10 | |
| LOAD-C25 | S5 | K5 | LOAD-B2, LOAD-B10 |  |
| LOAD-C26 | S6 | K1 | LOAD-B21 |  |
| LOAD-C27 | S6 | K2 | LOAD-B16, LOAD-B17, LOAD-B18 | |
| LOAD-C28 | S6 | K3 | LOAD-B21, LOAD-B23 |  |
| LOAD-C29 | S6 | K4 | LOAD-B21 |  |
| LOAD-C30 | S6 | K5 | LOAD-B2, LOAD-B21 |  |
