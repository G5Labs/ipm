# A2 Operation inventory: IPM at 571e04a1

Stage A2 of the IPM Verus uplift (`../uplift-plan.md`). Question: which operations exist, from entry points through guards, writers, commit boundaries, external effects and final readers. Breadth covers all of IPM. Depth (exhaustive) covers package loading and transaction history.

- Product revision: `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. HEAD `5bf22282` changes no product file. All `file:line` references are repo-relative at that revision.
- Method: built from code and tests only, entry points first (`XData Commands` → `ShellInternal` dispatch → handlers → writers → readers). A1's inventory, README, CHANGELOG, wiki and `docs/ipm-uplift/index.md` were not read while building it. A final pass over `docs/ipm-uplift/index.md` was made afterwards. Anything that pass added is marked `(added after index pass)`.
- Everything below is observed behavior from reading code. Nothing here establishes intent. Where a statement depends on IRIS engine semantics that the code assumes but does not establish, it cites a premise `P-n` from the next section. Stage B2 should confirm those natively.

## 0. Engine premises the code relies on

| ID | Premise | Where the code relies on it |
|---|---|---|
| P-1 | Globals named `^IRIS.Temp*` map to the IRISTEMP database in every namespace, so they are instance-wide and shared across namespaces. | `HistoryTemp` storage (`src/cls/IPM/General/HistoryTemp.cls:124-128`), `LoadedResource` storage (`src/cls/IPM/Storage/LoadedResource.cls:167-169`), `^IRIS.TempZPMRInst` (`src/cls/IPM/Repo/Remote/PackageService.cls:142`) |
| P-2 | `TROLLBACK` does not undo sets or kills in IRISTEMP. | Class doc `HistoryTemp.cls:1`, `AbstractHistory.cls:4`. Comment in `LoadedResource.cls:61` ("Since using IRISTEMP, need an alternative approach to remove any rows") |
| P-3 | IRISTEMP is emptied on instance restart. | Implied by the "temporary" design. Nothing in IPM cleans orphaned `HistoryTemp` rows except the full-cleanup `kill` (`src/cls/IPM/Lifecycle/Module.cls:51`) |
| P-4 | Nested `TSTART`/`TCOMMIT` only change `$TLEVEL` until the outermost `TCOMMIT`. `TROLLBACK 1` undoes only the innermost level. | `LoadNewModule` comments `src/cls/IPM/Utils/Module.cls:1316-1317` |
| P-5 | `$INCREMENT` on an ID counter is not undone by rollback. | Row IDs of `^IPM.General.HistoryD` allocated inside a parent transaction |
| P-6 | A process that halts or is killed with `$TLEVEL>0` has its open transaction rolled back. | Process-death paths (OP-HIST-26) |
| P-7 | Class compilation, CPF writes (`Config.CPF.Write`), file-system writes, `pip` installs and HTTP calls are not, or are only partly, undone by `TROLLBACK`. Multicompile worker jobs may not share the caller's transaction. | `%Initialize` forces `/multicompile=0` only when `$tlevel` (`src/cls/IPM/Lifecycle/Base.cls:601`). `%Compile` does not (`Base.cls:1198-1199`) |
| P-8 | `%Library.Persistent` `Foreach = row/object` triggers fire for object `%Save` as well as SQL `UPDATE`. | `History` trigger `src/cls/IPM/General/History.cls:15-22`, exercised by `tests/integration_tests/Test/PM/Integration/History.cls:830-886` |
| P-9 | `%Reload` of an object whose row no longer exists returns an error. Whether a swizzled reference oref (such as `PersistedTwin`) survives `%Reload` and can be re-saved over a deleted row is unconfirmed. | `HistoryTemp.PersistToTwin` (`HistoryTemp.cls:30-59`) |
| P-10 | `^%`-prefixed globals such as `^%IPM.DefaultModifiers` are by default stored in a system database and are visible from every namespace. | `$$$ZPMDefaultModifiers` (`src/inc/IPM/Common.inc:31`), handlers named `*NamespaceDefaultModifiers` (`src/cls/IPM/Main.cls:1682-1696`) |

## 1. Entry points and breadth

### 1.1 Shell dispatch

- `%IPM.Main:Shell` (`src/cls/IPM/Main.cls:857-877`, `@API.Method`). It runs `%IPM.General.History:AutoPurgeOldHistory` first (`Main.cls:862-865`) and returns that status without running the command if it fails. It then calls `ShellInternal`. If `pTerminateOnError` is set and an exception occurred, it calls `$system.Process.Terminate($job,1)`. If `pHaltOnComplete` is set, it halts.
- `%IPM.Main:ShellInternal` (`Main.cls:1020-1165`). Steps:
  - Creates the EnvironmentConfig singleton reference.
  - Parses the command with `%IPM.CLI:%ParseCommandInput`.
  - Merges namespace default modifiers into `tCommandInfo("data")` (`Main.cls:1067-1069`).
  - Runs `new $$$ZPMCommandToLog` and sets it to the raw command string (`Main.cls:1072-1074`). That variable is `%IPMCommandToLog` (`src/inc/IPM/Common.inc:12`) and is read by `AbstractHistory.Init`.
  - Dispatches on the command name (`Main.cls:1076-1142`). Phase commands in `STANDARDPHASES` (`Main.cls:10`) go to `RunOnePhase`.
  - Catches every exception. A Ctrl-C (`$$$ERCTRLC`) returns silently. Any other exception goes to `DisplayError`.
  - After each command, checks `^||%IPM.SelfUninstallFlag` and exits the shell if it is set (`Main.cls:1151-1157`).
- `%IPM.CLI:%ParseCommandInput` and `SetData` (`src/cls/IPM/CLI.cls:635-640`, `CLI.cls:815-827`). A `-D<name>=<value>` without a dot is stored twice: as `data(<name>)` and as `data("zpm",<name>)`.

### 1.2 Shell commands (39 commands in `XData Commands`, `Main.cls:13-845`)

"Stores written" lists the main persistent or external effects only. Lifecycle phase effects are listed once in §1.3.

| # | Command (aliases) | Handler symbol (file:line) | Stores / effects written | Readers of that state | Notes |
|---|---|---|---|---|---|
| 1 | `help` (`?`) | `%IPM.CLI:%Help` (`CLI.cls:15`), dispatched at `Main.cls:1078-1079` | none (device only) | — | Reads `XData Commands` |
| 2 | `quit` (`q`,`exit`) | `ShellInternal` returns (`Main.cls:1076-1077`) | none | — | |
| 3 | `module-action` (default) | `Main:ModuleAction` (`Main.cls:2830-2857`) → `%IPM.Storage.Module:ExecutePhases` (`Storage/Module.cls:447`) | Whatever the listed phases write. EnvironmentConfig singleton (process memory) | Phase readers | No history log (`pLog=""`). `-only` runs single phases. Phase names are case-folded |
| 4 | `init` (`initialize`) | `Main:Init` (`Main.cls:1756-1970`) | Rebuilds `%IPM.Repo.Filesystem.Cache` indices. Writes `IConfigurable` settings. Writes `%ZLANGC00`/`%ZLANGF00` routines via `UpdateLanguageExtensions` (`Main.cls:3464-3503`, own tx). Writes the source-control and menu extension class (`%Studio.SourceControl.Interface`, `^%SYS("SourceControl")`) | `zpm` command routine, IDE source-control hooks, `config list` | Interactive prompts unless `-noprompt` |
| 5 | `reload` | `Main:RunOnePhase` (`Main.cls:2753-2766`) → `ExecutePhases(["Reload"], complete)` | Initialize+Reload writes (§1.3) | `list-installed`, `sync`, later phases | |
| 6 | `sync` | `Main:Sync` (`Main.cls:2375-2412`) → `ExecutePhases(["Sync"])` → `%IPM.Lifecycle.Base:%Sync` (`Base.cls:579`) → `%IPM.General.Sync.Pipeline:Run` (`General/Sync/Pipeline.cls:11`) | Loads and compiles changed files (`Pipeline.cls:521`). Deletes documents with `-delete` (`Pipeline.cls:565`). Reloads `module.xml` (`Pipeline.cls:247`). Writes `^IPM.Storage.FileHashD` stamps (`Pipeline.cls:45`) | Next `sync`, `-test` run | No transaction. With no module name, syncs every dev-mode module in dependency order |
| 7 | `compile` | `RunOnePhase` → Initialize…Compile | Compiled code. FileHash stamps in dev mode (`Base.cls:1238-1244`) | | |
| 8 | `test` | `RunOnePhase` → …Activate, Test (`Base.cls:1256`) | Loads test-scoped dependencies through `LoadDependencies` (each creates a history row with Action `load`). Writes `^UnitTestRoot` (`ResourceProcessor/Test.cls:209`), `%UnitTest` results and the output file | Test report | |
| 9 | `package` | `RunOnePhase` → …Package (`Lifecycle/Module.cls:134`) | `.tgz` at `-path` or temp dir. Runs MakeDeployed if a resource is marked deployable | `publish`, `verify` | |
| 10 | `verify` | `RunOnePhase` → …Package, Verify (`Base.cls:1461-1578`) | Creates the `<NS>-VERIFY` namespace (CPF, databases, mappings). Installs the module there with `NoJournal=1` (`Base.cls:1527`). Writes test results | | Loads in the other namespace without a history log for the module itself |
| 11 | `publish` | `RunOnePhase` → Publish (`Base.cls:1580`) | Package file. External upload (HTTP or ORAS) | Remote `search`/`install` | |
| 12 | `update` | `Main:Update` (`Main.cls:4413-4447`) | Deep: OP-LOAD-21 | | History Action `update` |
| 13 | `makedeployed` | `RunOnePhase` → MakeDeployed (`Base.cls:1825-1921`) | `Module.Deployed=1` and `SystemRequirements.VersionString` `%Save`. `$system.OBJ.MakeClassDeployed`. `%Routine.Delete` (non-dev) | `list-installed`, packaging | `-recurse` |
| 14 | `unpublish` | `Main:Unpublish` (`Main.cls:2768-2828`) | External registry delete | Remote `search` | Prompts unless `-force` |
| 15 | `config` | `Main:Config` (`Main.cls:2063-2093`) → `%IPM.Repo.UniversalSettings` `UpdateOne`/`ResetToDefault`/`PrintList`/`PrintOne` (`Repo/UniversalSettings.cls`) | `^IPM.settings(key)` and `^IPM.settings(key,"default")` in `%SYS` | Every `GetValue` caller: history limit and retention, pip caller, ModuleRoot, colors | See OP-HIST-22 |
| 16 | `repo` (`repository`) | `Main:Repository` (`Main.cls:2095-2284`), plus `%IPM.Repo.Definition` subclasses | `^IPM.Repo.DefinitionD/I`. Filesystem cache `^IPM.Repo.Filesystem.CacheD` (BuildCache, own tx, `Repo/Filesystem/Definition.cls:117-181`). `-delete-all` runs `%DeleteExtent` | `install`, `search`, `update`, `BuildDependencyGraph`, `history` SetSource | |
| 17 | `load` | `Main:Load` (`Main.cls:2415-2431`) | Deep: OP-LOAD-01…29 | | History Action `load` |
| 18 | `import` | `Main:Import` (`Main.cls:3326-3418`) | `$system.OBJ.ImportDir`/`Load` (+compile). Kills `^Sources(0,"FileTimeStamp",item)` | | No module record, no history |
| 19 | `run-from-file` (`run`) | `Main:ProcessRunFromFile` (`Main.cls:1972-2035`) | Runs each line through `Main:Shell` (nested: own AutoPurge check, own CommandString) | | `-dry-run` |
| 20 | `install` | `Main:Install` (`Main.cls:2554-2671`) | Deep: OP-LOAD-17/18 | | History Action `install` |
| 21 | `reinstall` | `Main:Reinstall` (`Main.cls:2673-2699`) → `Install` | Deep | | Logged as `install` |
| 22 | `uninstall` | `Main:Uninstall` (`Main.cls:2701-2751`) → `Storage.Module:Uninstall` (`Storage/Module.cls:727-758`) or `Utils.Module:UninstallAll` (`Utils/Module.cls:709-830`) | Deletes classes and routines (`Base.cls:363-428`). Deletes module record and ModuleStream (`StudioDocument/Module.cls:205-227`, own tx). Runs Unconfigure hooks. Processor Clean effects (files, web apps, CPF). Writes history rows. IPM full cleanup kills IPM globals (`Lifecycle/Module.cls:32-56`) | `list-installed`, `history` | No transaction around the phase (Clean). See OP-HIST-23/24. `FileCopy` Clean keeps targets when `KeepOnUninstall` is set (default 0) and never removes a protected IRIS/IPM directory (`ResourceProcessor/FileCopy.cls:36`, `222-240`). (added after index pass) |
| 23 | `exec` (`cos`) | `Main:ExecuteCOS` (`Main.cls:2433-2438`) | Arbitrary (`xecute`) | | `new $namespace` |
| 24 | `orphans` | `Main:ListOrphans` (`Main.cls:3268`) → `Utils.Module` `OrphanedResources` query (`Utils/Module.cls:2138`) | none | | |
| 25 | `list-installed` (`list`) | `Main:ListInstalled` (`Main.cls:2939-3013`) | none | | Reads `%IPM_Storage.ModuleItem`. `-showupstream` queries repos. `-python` lists libraries |
| 26 | `list-dependents` (`dependents`) | `Main:ListDependents` (`Main.cls:3150-3233`) | none | | `-repos` searches registries |
| 27 | `show` | `%IPM.General.ModuleSummary:Show` (`General/ModuleSummary.cls:9`), dispatched at `Main.cls:1100-1101` | none | | |
| 28 | `default-modifiers` | `Main:ProcessDefaultModifiers` (`Main.cls:1725-1754`) | `^%IPM.DefaultModifiers` (`Common.inc:31`, P-10) | `ShellInternal` merges it into every command (`Main.cls:1067-1069`), including `load` and `history` | See OBS-H2 and OBS-X1 |
| 29 | `arrange` | `Main:Arrange` (`Main.cls:3278-3324`) → `Utils.Module:ExportDocumentForObject` (`Utils/Module.cls:1055`) | `module.xml` and resources on disk via source-control export | | |
| 30 | `dependency-analyzer` (`deps`) | `Main:RunDependencyAnalyzer` (`Main.cls:2859`) → `%IPM.DependencyAnalyzer` | Scratch data in `^IRIS.Temp.IPMDependency` (`DependencyAnalyzer.cls:230-231`) | | |
| 31 | `version` (`ver`) | `Main:Version` (`Main.cls:1593`, `@API.Method`) | none | | |
| 32 | `generate` (`gen`) | `Main:GenerateModuleXML` (`Main.cls:1420-1560`) → `%IPM.Storage.ModuleTemplate` | `module.xml` file (and exported resources with `-export`) | `load` | Interactive |
| 33 | `search` (`find`) | `Main:Search` (`Main.cls:2037-2061`) | none (HTTP to registries) | | |
| 34 | `namespace` (`zn`) | `Main:Namespace` (`Main.cls:1199-1245`) | Process `$namespace` | | |
| 35 | `enable` | `Main:EnableIPM` (`Main.cls:3553-3925`) | Installs IPM into other namespaces (`OBJ.LoadStream`/`Load` at `Main.cls:3885`, `3904`). Writes `Config.MapPackages`, `MapRoutines`, `MapGlobals` (CPF). Activation via `PerformBatchActivation` (`Utils/Module.cls:1888-1933`, own tx). Optional repo-global mapping (`Main.cls:3674-3690`). `-community` runs three nested `ShellInternal` commands (`Main.cls:3564-3575`) | `GetMapInfo`, `version`, `CheckModuleNamespace` | Honors `NoTransaction`/`NoJournal` (`Main.cls:3588`) |
| 36 | `unmap` | `Main:UnmapIPM` (`Main.cls:3927-4009`) | Removes mappings. `PerformBatchActivation` | | |
| 37 | `module-version` (`modver`) | `Main:ModuleVersion` (`Main.cls:4011-4104`) | `Module.Version`/`VersionString` `%Save` and `StudioDocument.Module:UpdateVersion` (`.ZPM` XML) in its own tx (`Main.cls:4077-4097`). Source-control checkout | `list-installed`, `publish` | |
| 38 | `information` (`info`) | `Main:Information` (`Main.cls:4360-4411`) | none | | |
| 39 | `history` (`log`) | `Main:History` (`Main.cls:4127-4180`) | Deep: OP-HIST-13…18 | | |

### 1.3 Non-shell entry points

| Entry point | Symbol (file:line) | Writes / effects | Notes |
|---|---|---|---|
| Programmatic shell | `%IPM.Main:Shell` (`Main.cls:857`) | As the command | Only entry that runs AutoPurge |
| OS-shell wrapper | `%IPM.Main:ShellScript` (`Main.cls:4182-4224`) | Output log file. Terminates the process on error | Calls `Shell` |
| Test and internal entry | `%IPM.Main:ShellInternal` (`Main.cls:1020`) | As the command | No AutoPurge. Used by `EnableIPM -community` |
| `zpm` command routine | Generated from `Main:LanguageExtensions` (`Main.cls:3505-3551`) into `%ZLANGC00` (`ZPM(pArgs...)`) and `%ZLANGF00` (`$$ZPM`) by `UpdateLanguageExtensions` (`Main.cls:3464-3503`, tstart 3472, tcommit 3495, rollback 3498-3501) | Routine save and compile | The command form discards `Shell`'s status. The function form returns it. Falls back to a "not enabled" message |
| Other `@API` methods | `Main:GetVersion` (900), `Main:Version` (1593), `Main:IsAvailable` (1663), `Storage.Module:ExecutePhases` (447), `Storage.Module:Uninstall` (727), `Utils.Module:GetDependentsList` (460), `GetDependentsAsTree` (506), `History` queries `Records` (26) and `GlobalRecords` (233), `LoadedResource` `Validate`/`Clear`/`ReportConflicts` (76/116/135), `LogManager` Trace/Info/Warning/Error, `Utils.Build` InitializeBuildNamespace/InitializeNamespace/CleanUpNamespace/SwitchToBuildNamespace/SwitchToNamespace/RetrieveBuiltDatabase, `DependencyAnalyzer` RunForAllModules/RunForAllReferences/RunForInvalidReferences, `XSLTProvider`, `LocalizationExport` Get/SaveMessageFile, `TempFileManager` | Per method | `ExecutePhases` and `Uninstall` are the API-level writers |
| SQL procedures | `GET_DEFAULT_RESOURCE_PROCESSOR` (`Base.cls:119`), `Main` queries `ActiveNamespaces`, `SourceControlClasses`, `ServerMenuExtensionClasses`, `Utils.Module` `GetAvailableModuleNames`/`GetModuleList`/`ListProcessorAttributes`/`OrphanedResources`, `Storage.Module` `VersionRequirements` (`Storage/Module.cls:1813`) | read-only | |
| Installer bootstrap | `IPM.Installer:setup` (`preload/cls/IPM/Installer.cls:69`), manifest `XData PM` (17-44). Steps: maps `%IPM` package and routines, `CleanupOldMappings` (481, kills `^oddStudioDocument` entries), imports `src`, `ForceCompileEvenInSYS` (46), `UpdateLanguageExtensions`, `ZPMInit` (110-124: `^IPM.settings` defaults including `DefaultLogEntryLimit=20` and `ModuleRoot`), `ZPMLoad` (126-131: `Shell("load <dir> -DDeveloperMode=0 -DNoMapping=1")`) | Mappings, code, settings, then a full `load` (history row `load`) | `EndCompile` (277-299) auto-installs when `VERSION`/`FILESCOUNT` are set. `Map` (89), `MapIfLegacy` (133: `Shell("enable -globally -map")`), `InstallORASIfMissing` (141), `ResetPythonPermissions` (180), `InstallScripts` (550) |
| IPM's own module.xml Invokes | `module.xml` lines 23-32, 59 | `Map` before Reload. `MapIfLegacy` after Compile (a nested `Shell` inside IPM's own load). `ResetPythonPermissions`, `InstallORASIfMissing` and `InstallScripts` after Activate. `UpdateLanguageExtensions` and `Migration.RunAll` (`Utils/Migration.cls:6`; own tx at 120-168, 184-201) in the default Invoke phase (`Configure`, `Storage/InvokeReference.cls:12,17`) | These run inside the load's transaction in non-dev mode |
| Lifecycle phases (`%IPM.Lifecycle.Base`) | `%Clean` 305, `%ExportData` 507, `%Configure` 512 (InstallerClass `OnInstallComponent`/`OnConfigureComponent`), `%Unconfigure` 545, `%Sync` 579, `%Initialize` 584 (preload ImportDir), `%Reload` 615, `%Validate` 1045, `%Compile` 1052, `%Activate` 1251, `%Test` 1256, `%Export` 1283, `%Package` 1450, `%Verify` 1461, `%Publish` 1580, `%Installer` 1701, `%Document` 1719 (own tx 1724/1817/1821), `%MakeDeployed` 1825, `%ApplyUpdateSteps` 1923 | Per phase | Phase expansion in `GetCompletePhasesForOne` (`Base.cls:165-189`). `%DispatchMethod` (64-77) runs nested `ExecutePhases` without a log |
| Lifecycle overrides | `%IPM.Lifecycle.Module` `%Clean` (8-93), `%Activate` (95-132: calls `..Configure`, Studio project in dev mode, seeds update steps), `%Package` (134-208). `DeployedModule` (deploy by default). `StudioProject` `%Package` (18) | | |
| Resource processor hooks | `Abstract` OnBeforePhase/OnPhase/OnAfterPhase/OnConfigureMappings (`ResourceProcessor/Abstract.cls:168,192,200,209`). `Default.Document` OnPhase (Reload: load sources, `^Sources`, `LoadedResource` tracking, deployed project install with `Module.Deployed=1` save, 59-245) and OnAfterPhase (478). `Default.Global` (`^Sources("GBL")`, OBJ.Load /nocompile). `Default.LocalizedMessages`. `Default.Class`/`Package`/`Routine` OnConfigureMappings. `FileCopy` (Activate copy, Clean delete). `Copy`. `CSPApplication` and `WebApplication` (`Security.Applications` in `%SYS` on Activate/Configure, delete on Clean). `CPF` (merge CPF before/after phase or custom phase). `SystemSetting` (`^%SYS("CSP",…)` at Compile). `PythonWheel` (pip at Initialize, `PythonWheel.cls:29-52`). `Test`. `ArtifactoryTarball`. `ModuleExport`. `LocalizationExport`. `LegacyLocalizedMessages` (own tx 58/112/118). `CustomPhaseMixin` | Files, `%SYS` config, CPF, pip, globals | Called from `ExecutePhases` (`Storage/Module.cls:587-600`, `673-686`) and from inside phases |
| Update steps | `Storage.Module:HandleAllUpdateSteps` (94-161, lock `^IPM.UpdateLock(name)` timeout 3 s via `$$$LockUpdateModule`, `Common.inc:105`), `HandleUpdateStepsFromList` (172), `SeedUpdateStep` (203), `ExecuteUpdateStep` (223; primary-only steps are skipped on a mirror member that is not primary, 228-233, added after index pass), `GetAddUpdateStep` (259). Storage `^IPM.UpdateStep.AnyMemberD`, `^IPM.UpdateStep.PrimaryOnlyD`. Class list from `%IPM.General.Update.VersionBase:GetUpdatePackageClassList` | Step rows. The user step's own effects | Seeded in `Lifecycle.Module:%Activate` (122-127). Run in `%ApplyUpdateSteps` |
| Source-control hooks | `%IPM.ExtensionBase.SourceControl.Interface` `%OnBeforePhase`/`%OnAfterPhase`/`%OnChangeModuleRoot`/`%OnModifyResources` (40/52/64/73) | Extension-defined | |
| `.ZPM` document (IDE/OBJ.Load) | `%IPM.StudioDocument.Module` `Save` (15), `ImportStream` (149-202, tstart 184, tcommit 194, rollback 198-200), `UpdatePersistentFromStream` (26-107), `OnCompileDocument` (109-146, `^IPM.StuDoc.Module("compile",…)`), `Delete` (205-227, own tx) | `^IPM.StudioDoc.ModuleStreamD`, `^IPM.Storage.ModuleD` | Every `$system.OBJ.Load` of a `module.xml` goes through here |

## 2. Package loading (depth)

### 2.1 Call graph

```
zpm "load <path> [flags]"  ─┐
Main:Shell ─ AutoPurge ─ ShellInternal ─ Main:Load ──── HistoryTemp.LoadInit("")
                                            └ LoadInternal ─ (URL) LoadFromRepo: git clone
                                                           ─ (.tgz) Archive.Extract → copy to ModuleRoot/<n>/<v>
                                                           ─ TempLocalRepoManager(<dir>/.modules, first)
                                                           └ Utils.Module:LoadNewModule(dir, params, "", log)
zpm "install <m> [ver]" ─ Main:Install ─ HistoryTemp.InstallInit(m) ─ SearchRepositoriesForModule
                                       └ LoadQualifiedReference ─ LoadModuleReference(prelogged)
                                            ├ filesystem repo: LoadModuleFromDirectory(repo dir)
                                            ├ remote/ORAS: GetModule → LoadModuleFromArchive → LoadModuleFromDirectory
                                            └ legacy XML payload: OBJ.Load + compile (no LoadNewModule)
zpm "update <m>" ─ Main:Update ─ HistoryTemp.UpdateInit(m) ─ Load(-path) or Install(registry)

LoadNewModule
  tstart T1 | NoJournal: JournalManager(0)          (1191-1196)
  guards: downgrade, update-via-install/load         (1213-1223)
  OBJ.Load module.xml → .ZPM ImportStream (nested tx) (1251)
  module save (if DeveloperMode param)               (1298-1307)
  dev: tcommit C1, tstart T2                         (1309-1319)
  CheckSystemRequirements                            (1321)
  LoadDependencies ─ per dep: LoadModuleReference(no log → own HistoryTemp) ─ … ─ LoadNewModule (nested)
                   └ lock file                        (1328, 1368-1423)
  OBJ.Load module.xml again; %Reload                 (1330-1333)
  dev: tcommit C2                                    (1335-1339)
  ExecutePhases(Activate | ApplyUpdateSteps, log)    (1345-1346)
     Initialize, Reload, *, Validate, Compile, Activate[, ApplyUpdateSteps]
  non-dev: tcommit C3                                (1347-1352)
  on error: trollback 1 while $tlevel > tInitTLevel  (1358-1362)
```

### 2.2 Operations

**OP-LOAD-01 `Main:Load`.** `Main.cls:2415-2431`.
- Inputs: `pCommandInfo` and an optional `pLog` (passed by `update`).
- Writes: when there is no `pLog`, `HistoryTemp.LoadInit("")` (`Main.cls:2420`). This is outside any try and at the caller's `$tlevel` (0 from the shell). It writes an IRISTEMP row and a durable namespace twin (OP-HIST-01).
- Next it calls `LoadInternal` inside try. On an exception it calls `log.Finalize(Ex.AsStatus())`, wrapped in `$$$ThrowOnError` (`2427`), and rethrows. On success it calls `log.Finalize($$$OK)` (`2430`).
- Commit boundary: none of its own. The history twin is updated at `%OnClose` when `log` leaves scope: when `Load` returns, or when `Update` returns if the log was passed in.
- Failure: if `Finalize` itself fails (for example, the temp row was removed), the Finalize error replaces the original exception.

**OP-LOAD-02 `Main:LoadInternal` (path classification and flags).** `Main.cls:2440-2532`.
- Guards:
  - Empty path throws "No path has been specified." (`2447-2448`).
  - A path that does not exist, is not absolute and is not a URL throws "The specified path must be absolute." (`2449-2452`).
- Flags:
  - `-force` sets `tParams("Force")` (`2453`, `2461`).
  - For a non-update call it sets `cmd="load"` and `Load=1` (`2457-2460`).
  - All `-D` data, including defaults merged by ShellInternal, flows through `tParams`. That includes `DeveloperMode` (`-dev`/`-nodev`), `Verbose`, `NoTransaction`, `NoJournal`, `CreateLockFile`, `SkipSync`, `BypassPyDeps`, `ExtraPipFlags`, `EnvFiles`, `Branch` and `PermitDowngrade`.
- URL scheme containing `http` goes to `LoadFromRepo` (`2462-2465`, OP-LOAD-03).
- Archive branch (`2466-2521`): runs when the file exists and `$piece(path,".",*)` is `tgz`. The `tar.gz` test can never match (OBS-L2). Steps:
  1. Extract to a temp dir (`2474`).
  2. Find `module.xml`, using the last match as top-level (`2479-2486`). None found throws "No module.xml file found in archive."
  3. Parse it with `GetModuleObjectFromPath` (`2501`). Not found throws "No Module element found…" (`2502-2504`).
  4. Compute `<ModuleRoot>/<name>/<version>/` (`2505-2506`).
  5. Delete that directory if it exists (`2507-2509`). This happens before any version guard.
  6. Create it, `CopyDir` into it, and remove the temp tree (`2510-2515`).
  7. On error, remove the temp tree and rethrow (`2516-2520`).
  All writes are files and are never rolled back.
- Temp repo: `TempLocalRepoManager(<dir>/.modules, useFirst=1)` (`2527-2528`, OP-LOAD-04).
- Then `LoadNewModule(dir, .tParams, , pLog)` (`2529`) and `tmpRepoMgr.CleanUp()` added to the status (`2530`). Throws the combined status (`2531`).

**OP-LOAD-03 `Main:LoadFromRepo` (git URL).** `Main.cls:2347-2373`.
- Creates `<install>/mgr/Temp/<ztimestamp>/`.
- Runs `cd <tmp> && git clone <url> [-b <branch>]` through `Utils.Module:RunCommandViaZF` (`Utils/Module.cls:2387-2421`, `$zf(-1)` shell). The OS return code is not checked.
- Prints stderr, waits 2 s and returns `<tmp>/<repo>`. A failed clone shows up later as "File not found to obtain module object".
- The clone directory is never removed and becomes the module root.

**OP-LOAD-04 `%IPM.General.TempLocalRepoManager`.** `General/TempLocalRepoManager.cls`.
- `%OnNew` (49-71): skips when the directory is missing or a filesystem repo with that Root already exists (`SkipCreate` 33-47). Otherwise `Create` (13-31) finds a free `ipm-temp-modules-<n>` name and saves a `%IPM.Repo.Filesystem.Definition` (`^IPM.Repo.DefinitionD`) with sort order -1000 (top-level) or +1000 (nested). It then runs `BuildCache(1,1,1)`, which writes `^IPM.Repo.Filesystem.CacheD` in its own nested tx (`Repo/Filesystem/Definition.cls:132-180`).
- `CleanUp` (73-86): `%DeleteId` of the repo.
- Transaction context: at top level it is created at `$tlevel` 0 (durable). For dependencies (`Utils/Module.cls:138`, `257`) it is created inside the parent's T1/T2.
- If `%OnNew` fails, `%New` returns `""` and the later `.CleanUp()` raises `<INVALID OREF>` (OBS-L12).
- There is no `%OnClose`. A process death between create and cleanup leaves the repo definition behind.
- Reader: `Repo.Manager:SearchRepositoriesForModule` during dependency resolution.

**OP-LOAD-05 `LoadNewModule` setup and transaction start.** `Utils/Module.cls:1152-1196`.
- Records `tInitTLevel=$tlevel` (`1162`).
- Parses `module.xml` without loading it (`GetModuleObjectFromPath`, `334-348`). This throws "File not found to obtain module object: …" if the file is missing.
- Merges the module's `<Defaults><Parameter>` values (`Storage.Module:GetDefaultParameters` 799-831) under the caller's params (`1166-1169`). A module can therefore declare `NoTransaction` or `NoJournal` for itself (documented in `Utils/Module.cls:1033-1041`).
- `tUseTransactions = '(NoTransaction || NoJournal)` (`1172`).
- LogManager threshold (`1180-1187`).
- `NoJournal≠0` creates `JournalManager(0)`, which disables process journaling until the object closes when `LoadNewModule` returns (`1191-1193`, OP-LOAD-26). Otherwise, if transactions are used, it runs `tstart` (T1) (`1194-1196`).
- Note: T1 starts before any guard, so guard refusals roll back nothing but do happen inside a transaction.

**OP-LOAD-06 version guards.** `Utils/Module.cls:1198-1223`.
- Opens the installed module by the name in the new `module.xml`.
- Downgrade refusal: if the installed version `Follows` the new one, it throws "Cannot downgrade %1 from version %2 to %3. To downgrade a module, you must first run the uninstall command…" (`1214-1217`). There is no bypass. `PermitDowngrade` is not consulted (OBS-L14).
- Update-via-install/load refusal: when `cmd` is `install` or `load`, `Force` is not set, `DeveloperMode` is not set, the version goes up, and the module has an `UpdatePackage`, it throws "Cannot perform an update of %1 … use the 'update' command … use the -force flag" (`1218-1222`).
- Both throw inside T1. The catch rolls back and the error status is returned.

**OP-LOAD-07 "kept without transaction" marker.** `Utils/Module.cls:1225-1228`. When there are no transactions and a log exists, it calls `pLog.SetCommitted()` (IRISTEMP) before any further write.

**OP-LOAD-08 mark resources for update.** `Utils/Module.cls:1230-1248`. Only for `update` with a `ModuleName` param.
- `moduleCurrent.GetResolvedReferences` is followed by `ResourceReference:MarkResourceForUpdate` (`Storage/ResourceReference.cls:366-377`, `%Save` of `MarkedForUpdate=1`).
- Store: `^IPM.Storage.ResourceReferenceC` in T1.
- Reader: the conflict check at `ResourceReference.cls:467`.

**OP-LOAD-09 manifest load.** `Utils/Module.cls:1251-1264`.
- `$system.OBJ.Load(dir_"module.xml", "-d")` → `StudioDocument.Module:Save` → `ImportStream` (`StudioDocument/Module.cls:149-202`):
  - nested `tstart` (184)
  - `ModuleStream.%Save` (`^IPM.StudioDoc.ModuleStreamD`)
  - `UpdatePersistentFromStream` (26-107): clears Dependencies, Resources, Defaults, Mappings and Invokes, saves the existing module, then correlates the XML into it and `%Save`s. This writes `^IPM.Storage.ModuleD` and child storage. `%OnAddToSaveSet` updates `Hash` and `LastModifiedTimestamp`. `%OnBeforeSave` updates `LastUpdated` (`Storage/Module.cls:1600-1629`, `1879-1883`). `%OnValidateObject` refuses on lifecycle, packaging or NamespaceConfig mismatch, a duplicate resource owned by another module, or an ExternalName conflict (`1510-1591`).
  - `tcommit` (194). On error, `trollback 1` down to the entry level (198-200).
- Post-load guards: "No module definition found." (`1255-1257`, `1262-1264`) and "module.xml contains multiple documents." (`1258-1260`).
- Store: T1 in transaction modes, durable otherwise.
- Minor: `ImportStream` calls `ModuleStream.NameOpen(tName,.tSC)` (163), which passes the status variable as the concurrency argument.

**OP-LOAD-10 post-load checks and module record save.** `Utils/Module.cls:1266-1307`.
- Path/module mismatch for `update -path`: throws "The specified path does not contain a module matching the specified module to update…" (`1269-1275`). This runs after the manifest was already loaded (OBS-L6).
- Read-only repository forces `params("DeveloperMode")=0` (`1277-1288`).
- `NameOpen(tModuleName)` (`1290`). Then `pLog.SetName`/`SetVersion` (IRISTEMP, `1292-1295`).
- `tModule.Repository=pRepository` is set in memory (`1296`).
- Only if `params("DeveloperMode")` is defined: sets `Deployed=0`, `DeveloperMode`, `Root=pDirectory` and calls `%Save` (`1298-1304`). Otherwise `tDeveloperMode=+stored DeveloperMode` (`1306`). That makes dev mode sticky for a plain `load`, and Root and Repository are not saved here (OBS-L3).
- `DeveloperMode` is always defined for `install`/`reinstall` (`Main.cls:2623`) and for dependencies (`Utils/Module.cls:310`). It is defined for `load` only with `-dev`/`-nodev` or a default modifier.

**OP-LOAD-11 dev-mode commit C1.** `Utils/Module.cls:1309-1319`.
- In dev mode with transactions: `tcommit`. This is durable only if `tInitTLevel=0` (P-4).
- Then `pLog.SetCommitted()` and `tstart` (T2).
- Comment (1310): "save the module manifest so we can fix it even if errors occur afterward."

**OP-LOAD-12 `Storage.Module:CheckSystemRequirements`.** `Storage/Module.cls:337-354`.
- Refusals: "Module '%1' not found." or the `SystemRequirements.CheckRequirements` status. These are thrown at `Utils/Module.cls:1321`.
- Read-only. Runs inside T1 (non-dev) or T2 (dev).
- One of the checks is `CheckSYSNamespace`: "The module requires the %SYS namespace. Current namespace is …" when `SYSNamespace` is set outside `%SYS` (`Storage/SystemRequirements.cls:133-139`). (added after index pass)

**OP-LOAD-13 `Utils.Module:LoadDependencies`.** `Utils/Module.cls:1368-1423`, called at `1328` after `kill params("ModuleName")` and setting `LockFileModule` (`1323-1327`).
- `pModule.BuildDependencyGraph` (`Storage/Module.cls:898`) runs repository searches (HTTP for remote and ORAS repos) and refuses on unsatisfiable requirements. Its status is thrown.
- `ConstructInvertedDependencyGraph` and `GetFlatDependencyListFromInvertedDependencyGraph` (1428-1500). Sets `pParams("CoUpdatingModules")`.
- For each entry that has a `ServerName`: `LoadModuleReference(...)` with no log (`1411`). The dependency therefore creates its own HistoryTemp (OP-HIST-02). Its status is thrown.
- Lock file: if `CreateLockFile` is set and this is the `LockFileModule`, it calls `LockFile:CreateLockFileForModule` (`General/LockFile.cls:29-90`). That writes `<module.Root>module-lock.json` (`LockFile.cls:7`, `82-89`). Errors are only printed (`1417-1421`).
  - The file is written before the parent's lifecycle and commit, so it survives a later rollback (OBS-L11).
  - No in-product reader of the lock file was found.
- Transaction context: everything here runs inside the parent's T1 (non-dev) or T2 (dev). It is durable immediately with NoTransaction, and unjournaled with NoJournal.

**OP-LOAD-14 manifest reload and dev commit C2.** `Utils/Module.cls:1330-1339`.
- Runs `OBJ.Load module.xml` again (same writes as OP-LOAD-09), then `tModule.%Reload()`. The reload discards the in-memory `Repository` set at 1296.
- In dev mode with transactions: `tcommit` (C2), which makes the dependency loads durable at top level. The following phases then run outside any IPM transaction.

**OP-LOAD-15 lifecycle and non-dev commit C3.** `Utils/Module.cls:1340-1354`.
- Sets `params("RootDirectory")=pDirectory`. Phase is `ApplyUpdateSteps` if `Update`, else `Activate` (`1345`).
- `ExecutePhases(name, [phase], complete=1, .params, pLog)` (`1346`, OP-LOAD-23) runs Initialize, Reload, `*` (module object refresh, not logged), Validate, Compile, Activate[, ApplyUpdateSteps]. Each phase gets `InitPhase`/`FinalizePhase` on `pLog`.
- On success, in non-dev mode with transactions: `tcommit` (C3), then `pLog.SetCommitted()` (`1347-1352`).
- Then `$$$ThrowOnError(tSC)` and `WriteAfterInstallMessage` (`Storage/Module.cls:356-361`).

**OP-LOAD-16 error and rollback.** `Utils/Module.cls:1355-1363`.
- The catch converts any exception to a status. After the catch, `trollback 1` runs while `$tlevel>tInitTLevel`. The method returns the status and does not throw.
- What survives a rollback: IRISTEMP writes (history temp, `LoadedResource`, install token; P-2), files (ModuleRoot extraction, FileCopy targets, CSP copies, lock file, pip, git clone), CPF file changes, `$increment`ed IDs, HTTP effects (P-5, P-7). Also anything a nested non-transactional dependency wrote before the parent opened T1, which is nothing in practice because T1 opens first.

**OP-LOAD-17 `Main:Install`.** `Main.cls:2554-2671`.
- Parses `registry/name` (`2561-2563`). Creates `InstallInit(name)` unless a log was passed (`2565-2569`). This happens before every guard.
- Guards, all of which throw:
  - "No module name specified." (`2573-2575`)
  - for `zpm`, `CheckModuleNamespace` ("Cannot install '%1' in namespace '%2' because it is mapped from namespace '%3'.", `2534-2552`)
  - "No repositories are configured and enabled in this namespace." (`2581-2586`)
  - prompt cancel gives "Operation cancelled." (`2613`)
  - platform-incompatible message (`2636-2651`)
  - "'%1' not found in any repository." or "%1 %2 not found in any repository." (`2654-2659`)
- Search: `Repo.Utils:SearchRepositoriesForModule` (`2596`). The first result is used unless `-prompt` with several results.
- Params:
  - lifecycle defaults from `^SYS("PackageManager","DefaultParameters")` (`Base.cls:81-86`)
  - `DeveloperMode=devMode` and `Force`
  - for non-update calls `cmd="install"` and `Install=1` (`2621-2630`).
- `log.SetSource(server)`, `log.SetVersion(version)` (IRISTEMP, `2631-2632`). Then `LoadQualifiedReference(tResult, .tParams, , log)` (`2633`).
- Finalize as in `Load` (`2666-2670`). An empty prompt choice (`tValue=""`) leaves `tResult=""` and finalizes `$$$OK` with nothing installed.

**OP-LOAD-18 `Utils.Module:LoadQualifiedReference` / `LoadModuleReference`.** `Utils/Module.cls:23-38`, `40-211`.
- Log handling: if a log was passed, it is "prelogged" and not finalized here (`51`). Otherwise `cmd="install"` gives `InstallInit`, and any other value gives `LoadInit` (`63-68`). Then `SetSource(server)` and `SetVersion(version)` (`69-70`).
- Repository availability: `Manager:CheckServiceCache` (`Repo/Manager.cls:18-49`). Refusal: "Repository '%1' is unavailable." (`87-91`).
- Fetch name: the module is fetched by the name the repository uses, taken from the dependency graph entry (`$listget(pDependencyGraph(name),8,name)`, `93-94`). For ORAS, `ModuleNameFromRepository` strips the configured registry namespace prefix (`Repo/Oras/PackageService.cls:14-22`). (added after index pass)
- Dev-mode guard, skipped with `PermitDowngrade`: for a top-level `install` (`Install=1`) of anything other than `zpm` over an installed dev-mode module without `-dev`, it refuses "Cannot install '%1' over previously installed in developer mode" (`103-114`). The comment above it says "Make sure we're not downgrading."
- Requirement guard: `GetRequiredVersionExpression` (`384-458`) uses `VersionRequirementsFunc` over installed modules, excluding co-installed modules in the graph. Refusal: "Requested version (%1 %2) does not satisfy the requirements of other modules installed in the current namespace (%3)." (`118-126`).
- Filesystem repo (`Supports("GetModuleDirectory")`, `Repo/Filesystem/PackageService.cls:58-62`):
  - directory from the filesystem cache (`48-55`)
  - dev mode for the dependency only if it is a snapshot and the caller is in dev mode (`131-136`)
  - TempLocalRepoManager for `.modules` (nested sort order)
  - `LoadModuleFromDirectory(dir, …, pServerName, log)` (`137-140`). The module root is the repository's own directory.
- Remote/ORAS:
  - `GetModule` (`Repo/Remote/PackageService.cls:92-157`): HTTP GET of `packages/<n>/<v>[/<platform>]`, download to a temp `.tgz`, SHA1 check ("Download module, hash mismatch."), store the install token in `^IRIS.TempZPMRInst(name,version)` (IRISTEMP, `142`). ORAS: `Repo/Oras/PackageService.cls:233-247`.
  - Gzip magic sniffing (`156-163`). Copy to a stream (`164-170`).
  - Archive goes to `LoadModuleFromArchive` (`173`). Legacy XML goes to `$system.OBJ.Load` without compile, then `LegacyCompat:UpdateSuperclassAndCompile` (`175-184`). The legacy path has no `LoadNewModule`, no transaction, no lifecycle, and no `SetName`/`SetCommitted`.
  - `CollectAnalytics("install", …, success, msg)` (`193-195`): an HTTP POST to the registry and Google when analytics are enabled (`Remote/PackageService.cls:244-273`). `SendAnalyticsToRegistry` reads and kills the token (`Remote/PackageService.cls:207-208`).
- Exceptions are converted to status. For `ZSOAP` errors, `%objlasterror` is used (`200-206`). For a non-prelogged log, `Finalize(tSC)` and its status are added (`207-209`).
- `%OnClose` of a dependency's log happens when `LoadModuleReference` returns, at the parent's `$tlevel`.

**OP-LOAD-19 `Utils.Module:LoadModuleFromArchive`.** `Utils/Module.cls:213-267`.
- Guard: "Insufficient file system permissions to write to '%1'. Cannot install module '%2'." (`228-233`).
- Removes `<ModuleRoot>/<n>/<v>/` if present, recreates it and extracts into it (`235-251`). Files are not transactional.
- TempLocalRepoManager for `.modules`, then `LoadModuleFromDirectory(dir, .pParams, , pRepository, pLog)`, then CleanUp (`256-259`). Returns a status.
- Also called by `%Verify`'s `OnInstallFromPayload` (`Base.cls:1688-1699`) with no log.

**OP-LOAD-20 `Utils.Module:LoadModuleFromDirectory` (parameter propagation).** `Utils/Module.cls:293-330`.
- Copies `pParams` to `tParams`.
- If `Install` is not set (dependencies and `update`), it sets `tParams("DeveloperMode")=pOverrideDeveloperMode` and drops `UpdateSnapshots` and `IgnoreInstalled` (`309-314`).
- `Update` forces `cmd="update"`. Otherwise `Install` is removed (`316-320`).
- Calls `LoadNewModule(dir, .tParams, pRepository, pLog)` (`322`).
- Consequence: everything in the parent's params reaches the dependency's `LoadNewModule`, including `NoTransaction`, `NoJournal`, `Update`, `CreateLockFile`, `LockFileModule`, `Force`, `BypassPyDeps` and `Verbose`.

**OP-LOAD-21 `Main:Update`.** `Main.cls:4413-4447`.
- Guards, thrown before any history log exists:
  - "The 'PermitDowngrade' flag is not allowed for the update command." (`4415-4417`)
  - "Module %1 has not been installed. Please use the install or load commands…" (`4421-4423`, checks Name or ExternalName).
- Sets `data("cmd")="update"`, `Update=1`, `ModuleName` (`4435-4437`). Creates `UpdateInit(moduleName)` (`4438`).
- `-path` goes to `Load(.pCommandInfo, log)` (`4442`). Otherwise `Install(.pCommandInfo, log)` (`4445`).
- In `LoadNewModule`, `isUpdate` selects the `ApplyUpdateSteps` phase list, and `Lifecycle.Module:%Activate` does not seed steps when `Update` is set (`Lifecycle/Module.cls:122`).
- Because `Update` propagates (OP-LOAD-20), dependencies also run `ApplyUpdateSteps` (OBS-L5). For the registry path the top-level module's `DeveloperMode` comes from `pOverrideDeveloperMode`, not from `-dev` (OBS-L4).

**OP-LOAD-22 `Main:Reinstall`.** `Main.cls:2673-2699`.
- Defaults `UpdateSnapshots=1`. Guard: "Module '%1' is not currently installed." Runs `CheckModuleNamespace` for `zpm`. Sets `version=<installed>` and calls `Install` (logged as `install`).

**OP-LOAD-23 lifecycle phases run by a load.** `Storage.Module:ExecutePhases` (`Storage/Module.cls:447-720`) with phases from `GetCompletePhasesForOne("activate"|"applyupdatesteps")` (`Base.cls:165-189`).
- `ExecutePhases`:
  - open by Name or ExternalName ("Module '%1' not found."), then `%Reload`
  - `RootDirectory` override; optional lifecycle override (`481-517`)
  - parameter merge: lifecycle defaults, then module defaults, then caller params (`527-539`)
  - per phase: `InitPhase` (`570-572`), source-control `%OnBeforePhase`, `OnBeforeResourceProcessing`, processor `OnBeforePhase`, Invoke `OnBeforePhase`, lifecycle `OnBeforePhase`, `%<Phase>`, `OnAfterPhase`, Invoke `OnAfterPhase`, processor `OnAfterPhase`, `OnAfterResourceProcessing`, source-control `%OnAfterPhase` (`575-691`)
  - catch: `FinalizePhase(error)` (`692-697`). Success: `FinalizePhase(OK)` (`709-711`). Returns a status.
- `%Initialize` (`Base.cls:584-613`): processor `OnPhase("Initialize")` statuses are accumulated. `PythonWheel` pip installs happen here. Preload `ImportDir` adds `/multicompile=0` when `$tlevel`, then `UpdateSuperclassAndCompile`.
- `%Reload` (`Base.cls:615-751`):
  - `OBJ.Load module.xml` from the root (a third manifest write)
  - name and document-count guards ("Module name mismatch.", "module.xml is malformed; it may only contain one document.")
  - mappings via `AddGlobalMapping`/`AddRoutineMapping`/`AddPackageMapping`: `Config.Map*` with `$$$CPFSave`, then `ActivateMappings`, which writes the CPF file and runs `MoveToActive` (`Utils/Module.cls:1736-1812`, `1869-1886`)
  - Python requirements (OP-LOAD-25)
  - module `%Save` of `DeveloperMode` and `Root` (`705-707`)
  - in dev mode, `^Sources("MODULE",…)` (`713-717`)
  - processor `OnConfigureMappings` and `OnPhase("Reload")`
  - source-control `%OnModifyResources`
- `%Validate`: no-op.
- `%Compile` (`Base.cls:1052-1249`): processor `OnPhase("Compile")` (its status is ignored, OBS-L8), dependency-ordered `UpdateSuperclassAndCompile` passes, cycle refusal "Failed to resolve compile order…", and in dev mode `FileHash:StampModule` (non-fatal).
- `Lifecycle.Module:%Activate` (`Lifecycle/Module.cls:95-132`): calls `..Configure`, a nested `ExecutePhases` without a log, so Configure and its default-phase Invokes are not recorded as a phase. Creates the Studio project in dev mode (errors ignored). When not `Update` and not dev mode, seeds update steps (`HandleAllUpdateSteps(,1)`).
- `%ApplyUpdateSteps` (`Base.cls:1923-1937`) runs `HandleAllUpdateSteps(,0)`:
  - takes `lock +^IPM.UpdateLock(name):3`. On timeout it returns `$$$OK` without running anything (`Storage/Module.cls:105-108`, OBS-L9)
  - runs from the first unrun or failed step onward (`172-198`)
  - each step `%Save`s its status. A failing step throws (`223-252`).

**OP-LOAD-24 resource processor effects during load.**
- `Default.Document:OnPhase("Reload")` (`ResourceProcessor/Default/Document.cls:59-211`): loads source files, sets `^Sources` in dev mode, and calls `LoadedResource:TrackResourceNames` (`Storage/LoadedResource.cls:26-71`). That method deletes and re-inserts rows in IRISTEMP (P-2), so they are not rolled back.
- `LoadFromDeployedProject` (216-245): `%Studio.Project:InstallFromFile` and a `Module.Deployed=1` `%Save`.
- `Default.Document:OnPhase("Compile")` (`Document.cls:194-206`) marks a scoped, non-generated, non-preload resource as handled. It is therefore not compiled during `load`/`install`. Its own phase (for example `test`) loads its scoped dependencies, then loads and compiles it. (added after index pass)
- `FileCopy` (Activate copy), `CSPApplication`/`WebApplication` (`Security.Applications` in `%SYS`, file copies), `CPF` (merge), `SystemSetting` (`^%SYS("CSP",…)` at Compile), `Test` (compile test classes).
- File and CPF effects are outside rollback (P-7).

**OP-LOAD-25 Python dependencies.** `Base.cls:753-814`, `Utils/Module.cls:2485-2536`.
- Runs only when `cmd` is `install`, `load` or `update`. `BypassPyDeps` skips it. Requires `<root>/requirements.txt`.
- `pip install -r requirements.txt -t <mgr>/python --python-version … --only-binary=:all: <ExtraPipFlags>` through `ResolvePipCaller` (`816-835`).
- Refusal: "Embedded Python is not available in this instance." A pip failure is thrown from Reload.
- These are external, non-transactional writes.

**OP-LOAD-26 `%IPM.Utils.JournalManager`.** `Utils/JournalManager.cls:9-75`.
- `%New(0)` records the initial flag and sets bit 1024 of the process partition with `view 0:-1:4`. `%OnClose` restores the flag.
- Lifetime is the local `tJournalMgr` in `LoadNewModule`. Dependency history twins written during a NoJournal load are therefore unjournaled.

**OP-LOAD-27 installer bootstrap load.** `preload/cls/IPM/Installer.cls:126-131` runs `Shell("load <dir> -DDeveloperMode=0 -DNoMapping=1")`. That is a full OP-LOAD-01 path with AutoPurge and a history row. IPM's own Invokes then run nested shells and transactions inside T1 (§1.3).

**OP-LOAD-28 load into the VERIFY namespace.** `Base.cls:1477-1553`.
- Runs in `<NS>-VERIFY` with `NoJournal=1`, through `LoadModuleFromDirectory` or `OnInstallFromPayload` (`LoadModuleFromArchive`). No history row is written for the module. Its dependencies get history rows in the VERIFY namespace.

**OP-LOAD-29 phase-scoped dependency loads.** `%Test` (`Base.cls:1261`) and `%Verify` (`1557`) call `LoadDependencies(module, PhaseList, .pParams)`. Run from the `test`/`verify` commands they execute outside any IPM transaction. Each dependency creates a `load` history row (`cmd` is `test`/`verify`).

### 2.3 Commit-point matrix for `LoadNewModule` by mode

Columns:
- **A** default: transactions, not dev mode.
- **B** `-dev` (or a stored dev module re-loaded without `-dev`/`-nodev`) with transactions.
- **C** `-DNoTransaction=1` (any dev setting).
- **D** `-DNoJournal=1`.
- **N** a dependency load nested in A or B.

"Durable" means it survives the operation's own error path. `T1`/`T2` mean rolled back with that transaction.

| Step (file:line) | A | B | C | D | N (dependency) |
|---|---|---|---|---|---|
| History Init twin insert (`Main.cls:2420`, `2566`, `4438`) | durable at tl0 | durable | durable | durable, journaled | inside parent T1/T2 (`Utils/Module.cls:65-67`) |
| Archive extract, ModuleRoot delete+copy, git clone (`Main.cls:2467-2521`, `Utils/Module.cls:235-251`) | file, durable | file | file | file | file, survives parent rollback |
| Temp `.modules` repo create and cleanup (`Main.cls:2528-2530`) | durable at tl0 | durable | durable | durable | parent T1/T2 |
| Transaction start (`1191-1196`) | `tstart` T1 | `tstart` T1 | none | journaling off | nested `tstart` (level+1), unless the dependency's own defaults say NoTransaction |
| Guards (`1213-1223`) refuse | rollback T1, Committed=0 | rollback T1, Committed=0 | nothing written, but Committed=0 | Committed=0 | rollback own level only |
| `SetCommitted` before writes (`1226-1228`) | — | — | yes (IRISTEMP) | yes | yes if the dependency is non-transactional (still inside the parent's tx) |
| MarkResourceForUpdate (`1245`) | T1 | T1 | durable | durable, unjournaled | parent tx |
| Manifest `OBJ.Load` (`1251`) and its nested ImportStream tx | T1 | T1 | own small tx, durable | durable, unjournaled | parent tx |
| Module save of DeveloperMode/Root/Repository (`1303`) | T1 | T1 | durable | durable | parent tx |
| **C1** `tcommit`, SetCommitted, `tstart` T2 (`1309-1319`) | — | **durable commit** of manifest and module | — | — | dev dependency: nested, not durable |
| CheckSystemRequirements (`1321`) refuses | rollback T1 | rollback T2, manifest kept, Committed=1 | Committed=1 | Committed=1 | own level |
| LoadDependencies and dependency history (`1328`) | T1 | T2 | durable | durable, unjournaled | parent tx |
| Lock file (`1418`) | file, survives rollback | file | file | file | n/a (top module only) |
| Manifest reload (`1330`) | T1 | T2 | durable | durable | parent tx |
| **C2** `tcommit` (`1338`) | — | **durable commit** of dependencies | — | — | dev dependency: nested |
| ExecutePhases writes (`1346`) | T1 (except P-7 effects) | no tx: each write durable as made | durable | durable, unjournaled | parent tx (dev dependency: parent's T2) |
| **C3** `tcommit`, SetCommitted (`1347-1352`) | **durable commit** of everything | — | — | — | nested, not durable until parent's C3/C2 |
| Error after a step (`1358-1362`) | everything since T1 rolled back | before C1: all; C1–C2: dependencies, sysreq, reload; after C2: nothing | nothing | nothing | own level only, parent decides |
| Finalize, CleanUp, `%OnClose` PersistToTwin (`Main.cls:2427-2430`, `HistoryTemp.cls:62-73`) | tl0, durable | tl0 | tl0 | tl0, journaled | inside parent tx (twin update rolled back with parent) |

Where each source variant enters this matrix:
- `load <dir>`: root is the directory itself. The temp repo is first in search order.
- `load <x.tgz>`: root is `<ModuleRoot>/<n>/<v>`, replaced before T1.
- `load <git url>`: root is the temp clone.
- `install` from a filesystem repo: root is the repo directory. `DeveloperMode` is always defined.
- `install` from a remote/ORAS repo: root is `<ModuleRoot>/<n>/<v>`, replaced before T1. Analytics are sent after the load.
- `install` of a legacy XML payload: none of the matrix applies. There is no transaction.
- `update -path` follows `load`. `update` from a registry follows `install`, with `DeveloperMode` overridden.

## 3. Transaction history (depth)

### 3.1 Stores and classes

| Class | Storage | Scope | Notes |
|---|---|---|---|
| `%IPM.General.AbstractHistory` (`General/AbstractHistory.cls:8`) | NoExtent | — | Properties: Action (`VALUELIST ",load,install,uninstall,update"`), Package, Version (`Required`), SourceName/Moniker/Details, TimeStart, TimeEnd, UserName, Success (`%Status`, initially 0), Committed (initially 0), CommandString (`Required`), Phases (list of `HistoryPhase`) (`12-49`) |
| `%IPM.General.HistoryTemp` (`General/HistoryTemp.cls:3`) | `^IRIS.Temp.HistoryTempD/I/S` (124-128) | IRISTEMP, instance-wide (P-1), not rolled back (P-2), lost on restart (P-3) | Adds `PersistedTwin` (6) |
| `%IPM.General.History` (`General/History.cls:4`) | `^IPM.General.HistoryD/I/S` (583-587) | Namespace default global database, journaled, transactional | Adds `Finalized` (13). Indices on Action, Package, Version (7-11). Trigger `PreventSQLUpdates` (15-22) |
| `%IPM.General.HistoryPhase` (`General/HistoryPhase.cls`) | Serial, embedded in Phases | — | Name, TimeStart, TimeEnd, Success (initially 0) |
| Settings | `^IPM.settings("DefaultLogEntryLimit")`, `^IPM.settings("history_retain")`, `^IPM.settings("history_retain","lastPurge")` in `%SYS` | Instance-wide | `Repo/UniversalSettings.cls:25,46,53`. `History.cls:183-197` |

### 3.2 Operations

**OP-HIST-01 `AbstractHistory:Init` and `HistoryTemp:Init` (create).** `AbstractHistory.cls:51-63`, `HistoryTemp.cls:9-26`.
- `##super` creates the temp row: Action, Package, `CommandString=$get(%IPMCommandToLog,"<unset>")`, UserName, placeholder Version `0.0.0-0` via `SetVersion()` (no reload, since the row has no ID yet), then `%Save`. This is the IRISTEMP write.
- `HistoryTemp.Init` then builds a `History` twin copying TimeStart, Action, Package, CommandString, UserName and Version, and calls `%Save`. This is a namespace write at the caller's `$tlevel`.
- It links `PersistedTwin` and saves the temp row again.
- Throws on any save failure. If the twin save fails, the temp row is deleted when the object closes.

**OP-HIST-02 Init wrappers and every caller.**

| Caller | Call (file:line) | Action | Package passed | `$tlevel` at Init (twin insert) | Finalize site | `%OnClose` (twin persist) |
|---|---|---|---|---|---|---|
| `load` (shell or `Main:Load`) | `LoadInit("")` `Main.cls:2420` | load | `""` | caller's (0 from shell) | `Main.cls:2427/2430` | on `Load` return, tl0 |
| `install`, `reinstall` | `InstallInit(name)` `Main.cls:2566` | install | typed name, registry prefix stripped | 0 | `2667/2670` | on `Install` return |
| `update` | `UpdateInit(name)` `Main.cls:4438` | update | typed name (may be ExternalName) | 0 | inside Load/Install | on `Update` return |
| Dependency of load/install/update/test/verify | `InstallInit` (cmd=install) or `LoadInit` (otherwise) `Utils/Module.cls:63-68` | install or load | dependency name | parent's: 1 in A/B, 0 in C/D or outside a load | `Utils/Module.cls:207-209` | on `LoadModuleReference` return, parent's level |
| `uninstall <m>`, `Storage.Module:Uninstall` API, each module of `uninstall -all` | `UninstallInit(m)` `Storage/Module.cls:734` | uninstall | m | 0 (no tx) | `Storage/Module.cls:750-756` | on `Uninstall` return |
| Recursive dependency uninstall (`-recurse`) | `UninstallInit(dep)` `Base.cls:440` | uninstall | dependency | 0 | `Base.cls:446/465/475/484` | on next loop reassignment or `%Clean` return |
| Self-uninstall (`uninstall zpm`) | top-level row above, then explicit finalization `Storage/Module.cls:630-643` | uninstall | zpm | 0 | `Storage/Module.cls:635` before Clean | explicit `PersistToTwin` at 638. `%OnClose` is skipped once the class is gone |

Not logged: `module-action`, phase commands, `sync`, `import`, `enable`/`unmap`, `module-version`, the VERIFY-namespace load of the module itself (OP-LOAD-28), and refusals in `Main:Update` (4415-4423), `Main:Uninstall` (2709-2712) and `Main:Reinstall` (2685-2688) that throw before Init.

**OP-HIST-03 `SetSource`.** `AbstractHistory.cls:86-97`. Runs `%Reload`, opens the repository definition, refuses with "Repository '%1' is not defined." (returned as a status, which callers throw), sets SourceName/Moniker/Details and saves the temp row. Callers: `Main.cls:2631`, `Utils/Module.cls:69`.

**OP-HIST-04 `SetVersion`.** `AbstractHistory.cls:99-110`. Runs `%Reload` if the row has an ID. Empty input becomes `0.0.0-0`. Accepts an object or a string. Saves the temp row. Callers: Init, `Main.cls:2632`, `Utils/Module.cls:70`, `Utils/Module.cls:1294`, `Storage/Module.cls:737`, `Base.cls:452`.

**OP-HIST-05 `SetName`.** `AbstractHistory.cls:112-117`. Caller: `Utils/Module.cls:1293` (lower-cased name from the loaded document).

**OP-HIST-06 `SetCommitted`.** `AbstractHistory.cls:119-125`. Doc: "Call wherever the action's changes become durable". Callers: `Utils/Module.cls:1227` (no-tx, before writes), `1313` (after C1), `1350` (after C3). For a nested dependency, C1 and C3 are not durable (P-4).

**OP-HIST-07 `Finalize`.** `AbstractHistory.cls:127-134`. `%Reload`, `Success=status`, `Committed = Committed || ISOK(status)`, `TimeEnd=now`, then `%Save` of the temp row. Returns the save status. Callers are listed in OP-HIST-02. A second Finalize overwrites Success and TimeEnd.

**OP-HIST-08 `InitPhase`.** `AbstractHistory.cls:136-142`. Inserts a `HistoryPhase` and saves, without `%Reload`. Caller: `Storage/Module.cls:571` (every phase except `*`). `Base.cls:441` (recursive uninstall, Clean).

**OP-HIST-09 `FinalizePhase`.** `AbstractHistory.cls:144-173`.
- Updates the last phase if its name matches. Otherwise it scans the list.
- Refusals: "No phase logs found"; "No log for phase %1 found". The scan dereferences `phase.Name` on the empty end-of-list value before checking `key=""`, so the second message is reached only as `<INVALID OREF>`. No product caller reaches that case.
- Callers: `Storage/Module.cls:634`, `695`, `710`; `Base.cls:445`, `464`, `474`, `483`.
- Not called on the Invoke error-status `quit` paths in `ExecutePhases` (`603-610`, `662-671`) (OBS-H14).

**OP-HIST-10 `PersistToTwin`.** `HistoryTemp.cls:30-59`.
- Runs only if `%IPM.General.History` is compiled and `PersistedTwin` resolves. It runs `%Reload` (throws on a missing row), copies every field including Phases, sets `PersistedTwin.Finalized=1`, and deep-saves the temp row, which also saves the twin.
- The twin save happens at the caller's `$tlevel`. Returns a status.
- No guard against an already-finalized twin. A second persist would be refused by the trigger.
- Callers: `%OnClose` and the self-uninstall path (`Storage/Module.cls:638`).

**OP-HIST-11 `%OnClose`.** `HistoryTemp.cls:62-73`. Runs when the last reference to the log oref is released. If the History class exists, it runs `PersistToTwin` and then `%DeleteId` of the temp row. The status is returned to the object system and no caller sees it. If the class does not exist (self-uninstall), nothing is persisted and the temp row is not deleted.

**OP-HIST-12 Trigger `PreventSQLUpdates`.** `History.cls:15-22`. On UPDATE (row/object), an old `Finalized=1` gives `%ok=0`, "Cannot edit finalized history log records" (SQLCODE -132 in the test). DELETE and INSERT are not covered. Tested by `History.cls:830-886` (test).

**OP-HIST-13 `history` / `history find` (namespace).** `Main.cls:4127-4145`, then `History:GetHistory` (`History.cls:132-148`) and `ConstructSQLWhere` (`93-126`).
- Inputs:
  - `-sort asc` gives ascending order. Any other value gives descending.
  - `-limit n`: empty uses `DefaultLogEntryLimit` (seeded to 20 by the installer). `0` means all. Otherwise `+n`.
  - filters come from `data("zpm",col)` (`Main.cls:4136`).
- Filter grammar per column: leading `>=`, `<=`, `<>`, `>`, `<`, `=` or `*` (LIKE). Values are parameterized. Column names must match `^[a-zA-Z][a-zA-Z0-9]*$`. The loop *quits* at the first key that fails that check or has no value (`103-105`) (OBS-H1).
- Query: `SELECT [TOP ?] * FROM %IPM_General.History WHERE … ORDER BY ID ASC|DESC`. SQLCODE is not checked.
- Display: `DisplayQueryResult` (`492-516`), `ShowColumns` (`334-382`), and `*ToString` (`425-490`). `VersionToString` opens each row by ID. `SourceToString` prints `n/a` unless Action is `install`. `SuccessToString` shows `GetErrorText` for non-OK. `CommittedToString`.

**OP-HIST-14 `history find -globally`.** `History:GetHistoryGlobally` (`295-317`) iterates `%SYS.Namespace_List()` and runs `GetHistory` in each namespace. `DisplayArray` (`518-532`) switches back into each namespace to fetch. `limit` applies per namespace. The `rs=$$$NULLOREF` branch never fires because `%ExecDirect` returns an error result set (OBS-H10).

**OP-HIST-15 `history details <id> [-showPhases]`.** `History:DisplayOneRecord` (`319-329`) runs `SELECT TOP 1 * … WHERE ID=?` in the current namespace only. Not found: "No record found with ID %1…". `ShowPhases` (`384-423`) decodes the `$list` of serial phases and shows a duration when TimeEnd is set.

**OP-HIST-16 `history schema`.** `Main.cls:4150-4163` lists columns of `%IPM_General.History` from `$system.SQL.Schema.GetAllColumns`, skipping `x__` columns. The list includes names such as `Version_Major` that the filter regex rejects (OBS-H1).

**OP-HIST-17 `history delete` (namespace).** `Main.cls:4164-4176`, then `History:DeleteHistory` (`151-159`).
- Guard: no filter, no `-globally` and no `-confirm` throws "No filter specified. Should specify at least one column to filter on or pass -confirm to delete all records in the current namespace." The guard tests `$data(filter)\2` (any subscript present), not whether `ConstructSQLWhere` kept any condition.
- `DELETE FROM %IPM_General.History WHERE …`. Throws on SQLCODE<0. Returns `%ROWCOUNT`.
- The delete is not blocked by Finalized and can remove live, unfinalized twins.

**OP-HIST-18 `history delete -globally`.** `History:DeleteHistoryGlobally` (`199-230`). For each namespace where the class exists it runs `DeleteHistory`. Errors are swallowed per namespace (printed with `-verbose`). There is no `-confirm` requirement (OBS-H9).

**OP-HIST-19 `Records` query (`@API.Query`).** `History.cls:26-86`. `CONTAINID=1`, but the first ROWSPEC column is Action and the ROWSPEC has no ID, Phases or Finalized. Execute calls `GetHistory` with an empty filter.

**OP-HIST-20 `GlobalRecords` query (`@API.Query`).** `History.cls:233-291`. Execute calls `GetHistoryGlobally`. Fetch calls `RecordsFetch` on each namespace's result set without switching `$namespace` (contrast `DisplayArray`). Close throws if namespaces remain.

**OP-HIST-21 `AutoPurgeOldHistory`.** `History.cls:162-181`, with `SetHistoryLastPurge`/`GetHistoryLastPurge` (`183-197`).
- Called only from `Main:Shell` (`Main.cls:862`).
- Skips if it already ran today (instance-wide `^IPM.settings("history_retain","lastPurge")` in `%SYS`). Skips if `+HistoryRetain<=0`.
- Builds `filter("TimeStart")="<='"_DATEADD("d",-days,+$h)_"'"`, which includes literal quote characters (OBS-H3). Calls `DeleteHistoryGlobally(.filter,0)` and stamps lastPurge.
- An exception becomes the returned status, which makes `Shell` return before running the user's command (OBS-H12).
- Not filtered by Finalized.

**OP-HIST-22 retention and limit settings.** `config set|get|delete HistoryRetain|DefaultLogEntryLimit`:
- `UniversalSettings:UpdateOne` (no validation of numeric values), `PrintOne`, `ResetToDefault`, `GetHistoryRetain` (`220-223`).
- `SetValue` stores the first value ever written as `(key,"default")` (`SetValue`), and `ResetToDefault` restores that value (OBS-H4).
- `DefaultLogEntryLimit` is seeded to 20 by `Installer:ZPMInit`. `HistoryRetain` is not seeded.

**OP-HIST-23 self-uninstall and full cleanup.**
- `Main:Uninstall` for `zpm` (`Main.cls:2713-2747`):
  - lists other modules, then asks for full cleanup unless `-force`
  - full cleanup runs `UninstallAll` (per-module logs)
  - sets `^||%IPM.SelfUninstallFlag=1` *before* the uninstall runs, then calls `Storage.Module:Uninstall`.
- In `ExecutePhases`, Clean on `zpm` with a log runs `FinalizePhase(Clean,OK)`, `Finalize(OK)`, `PersistToTwin`, `%DeleteId` and sets the local `pLog=""`, all before `%Clean` executes (`Storage/Module.cls:630-643`). Post-phase callbacks are then skipped.
- `Storage.Module:Uninstall` still holds the oref and calls `Finalize` inside a bare try/catch (`750-756`). `%Reload` fails on the deleted row and the error is swallowed.
- `Lifecycle.Module:%Clean` full cleanup kills IPM globals, including `^IRIS.Temp.HistoryTempD` (instance-wide, P-1), and keeps `^IPM.General.HistoryD` and `^IPM.settings` (`Lifecycle/Module.cls:32-55`).

**OP-HIST-24 recursive dependency uninstall logs.** `Base.cls:431-490`. Per installed dependency:
- `UninstallInit`, `InitPhase("Clean")`.
- Opening the module fails: FinalizePhase and Finalize with the error, then continue.
- `SetVersion` fails: quit with no Finalize.
- `Lifecycle.Clean` fails: finalize with the error and quit.
- `NameDelete` runs unless the dependency was marked Skip. A failure finalizes with the error.
- Otherwise `FinalizePhase(OK)` and `Finalize(OK)`. That includes the Skip case (OBS-H8).

**OP-HIST-25 command string capture.** `$$$ZPMCommandToLog` (`%IPMCommandToLog`) is `new`ed per command in `ShellInternal` (`Main.cls:1072-1074`).
- Nested `Shell` calls (run-from-file, Installer, Invokes) log their own command string.
- Dependency logs inherit the parent command string.
- Callers that do not go through the shell (API `Storage.Module:Uninstall`, `ExecutePhases`, VERIFY loads) get `"<unset>"`.

**OP-HIST-26 abnormal-end paths.**
- Process death after Init and before `%OnClose`:
  - IRISTEMP holds the temp row with the latest Package/Version/Source/Committed/Phases. It is never cleaned up until restart or full cleanup (P-3).
  - The durable twin keeps only its Init values: Package (`""` for `load`), Version `0.0.0-0`, Success 0, Committed 0, TimeEnd empty, Finalized 0, no phases. That is true even if the load committed at C1/C3 (OBS-H6).
  - Open transactions are rolled back (P-6). A dependency twin inserted in the parent's transaction is rolled back too.
- Instance crash: IRISTEMP is lost, so twins stay at their Init values.
- Ctrl-C: `LoadNewModule` catches it and rolls back. `ShellInternal` returns silently. Objects close normally, so the twin is persisted.
- `pTerminateOnError`/`halt` in `Shell` run after `ShellInternal` has returned, so logs are already closed.
- Self-uninstall: `%OnClose` is skipped once the class is deleted (OP-HIST-23).
- Caller-held transaction: if code wraps `Shell("load …")` in its own `TSTART`, the top-level twin insert, every IPM "commit" and the final persist all sit inside the caller's transaction (P-4). `Committed=1` can then be rolled back together with the row (OBS-H16).

### 3.3 Where each history field is written

| Field | Writers (file:line) | Store at write | Reaches durable `History` | Inside caller's tx? |
|---|---|---|---|---|
| Action | Init (`AbstractHistory.cls:56`) | temp, and twin at `HistoryTemp.cls:16` | twin insert at Init. Copied again at persist | Twin insert is at the caller's level: top-level tl0, dependency inside the parent's T1/T2 |
| Package | Init (`:57`). `SetName` (`Utils/Module.cls:1293`) | temp. Twin gets the Init value | Only via `PersistToTwin` | Persist at `%OnClose` level (top tl0, dependency inside parent tx) |
| CommandString, UserName, TimeStart | Init (`:58-59`, InitialExpression) | temp and twin | Init and persist | as above |
| Version | Init placeholder (`:60`). `Main.cls:2632`. `Utils/Module.cls:70`, `1294`. `Storage/Module.cls:737`. `Base.cls:452` | temp. Twin gets the placeholder | persist | as above |
| SourceName/Moniker/Details | `SetSource` (`Main.cls:2631`, `Utils/Module.cls:69`) | temp | persist | as above |
| Committed | `SetCommitted` (`Utils/Module.cls:1227/1313/1350`). `Finalize` OR (`AbstractHistory.cls:131`). Self-uninstall `Finalize(OK)` | temp | persist | SetCommitted for a dependency happens at a nested level (not durable, P-4) |
| Success, TimeEnd | `Finalize` (`AbstractHistory.cls:130,132`) | temp | persist | as above |
| Phases[] (Name, TimeStart) | `InitPhase` (`Storage/Module.cls:571`, `Base.cls:441`) | temp | persist | IRISTEMP survives rollback (P-2), so a rolled-back top-level load keeps its phase list |
| Phases[].TimeEnd/Success | `FinalizePhase` (see OP-HIST-09) | temp | persist | as above |
| Finalized (History only) | `PersistToTwin` (`HistoryTemp.cls:53`) | twin | that save | same as persist |
| PersistedTwin (Temp only) | `HistoryTemp.Init` (`:23`) | temp | — | — |

## 4. Observations (observed behavior only)

Each entry gives the path and a concrete input. "Code-read" means it was derived from source. "Native" means it depends on a premise in §0 and should be replayed in B2.

### Load path

- **OBS-L1 Dependency history is lost when the parent rolls back** (code-read, P-2, P-4, P-5).
  - Path: the dependency log's twin insert (`Utils/Module.cls:65-67` → `HistoryTemp.cls:21`) and its `%OnClose` persist (`HistoryTemp.cls:67`) both run inside the parent's T1 (non-dev) or T2 (dev). The parent's `trollback 1` loop (`Utils/Module.cls:1358-1362`) removes both, and the temp row was already deleted at `%OnClose`.
  - Input: `load /dir/parent`, where `parent` depends on `dep` from a configured repo, `dep` loads cleanly, and `parent` has a class that fails to compile. Expected observation: no history row for `dep` and a gap in `ID`s.
  - Dev-mode variant: `load -dev /dir/parent` with two dependencies where the second fails. Both dependency rows disappear.
  - Contrast: the class doc says HistoryTemp lives in IRISTEMP "to avoid being affected by TROLLBACK" (`HistoryTemp.cls:1`).
- **OBS-L2 `.tar.gz` archives are not recognized by `load`** (code-read).
  - Path: `extension = $piece(path,".",*)` yields `gz`, so `extension="tar.gz"` never matches (`Main.cls:2466-2467`).
  - Input: `load /tmp/mod-1.0.0.tar.gz`. Expected: the path is treated as a directory and fails with "File not found to obtain module object: /tmp/mod-1.0.0.tar.gz/module.xml" (`Utils/Module.cls:341-343`).
- **OBS-L3 A plain `load <dir>` does not persist Root/Repository/Deployed in `LoadNewModule`** (code-read).
  - Path: the save at `Utils/Module.cls:1298-1304` only runs when `params("DeveloperMode")` is defined. The in-memory `Repository=""` (`1296`) is discarded by `%Reload` (`1332`).
  - Effects:
    1. `load /dir/m -lock` on a fresh namespace builds the lock path from `module.Root` while Root is still empty (`LockFile.cls:82`). That writes `module-lock.json` relative to the process's current directory instead of `/dir/m/`.
    2. Re-loading from a directory a module first installed from a registry leaves `Repository` set to the old registry name. `%Reload` sets Root later, but not Repository.
  - Input for (1): `load /dir/m -lock` with `m` not yet installed. Input for (2): `install m` from repo `r1`, then `load /src/m`. `list-installed -repository r1` still lists `m`. Both need native confirmation.
- **OBS-L4 `update <m> -dev` from a registry ends in non-dev mode** (code-read).
  - Path: `Main:Install` sets `DeveloperMode=devMode` but no `Install` flag for updates (`Main.cls:2623-2630`). `LoadModuleFromDirectory` then overwrites it with `pOverrideDeveloperMode`, default 0, from `LoadModuleFromArchive` (`Utils/Module.cls:258`, `309-311`).
  - Input: `update m -dev` with `m` on a remote registry. The module record ends with `DeveloperMode=0`.
- **OBS-L5 A dependency newly installed during `update` runs all its update steps instead of seeding them** (code-read).
  - Path: `Update=1` propagates to dependency loads (`Utils/Module.cls:316-317`), which select `ApplyUpdateSteps` (`1345`). `%Activate` skips seeding when `Update` is set (`Lifecycle/Module.cls:122`). `HandleUpdateStepsFromList` runs every step without `TimeStampEnd` (`Storage/Module.cls:190-195`).
  - Input: `update A`, where A's new version adds dependency B (not installed) and B declares an `UpdatePackage` with steps.
- **OBS-L6 The `update -path` module-mismatch guard runs after the manifest and archive were written** (code-read).
  - Path: the archive is extracted to `<ModuleRoot>/B/<v>` (`Main.cls:2505-2515`), the manifest is loaded (`Utils/Module.cls:1251`), and only then does the mismatch throw (`1271-1274`).
  - With `-DNoTransaction=1`, B's `.ZPM` manifest stays in `^IPM.Storage.ModuleD` and the history row reads Committed=1 (set at `1227`).
  - Input: `update A -path /tmp/B-1.0.0.tgz -DNoTransaction=1`.
- **OBS-L7 Archive installs and loads delete the installed module's directory before any guard; rollback does not restore it** (code-read, P-7).
  - Path: `RemoveDirectoryTree(<ModuleRoot>/<n>/<v>)` at `Main.cls:2507-2509` and `Utils/Module.cls:236-242` precedes `LoadNewModule`.
  - Input: `m 1.0.0` is installed from a remote registry (root `<ModuleRoot>/m/1.0.0/`). Then `load /tmp/m-1.0.0.tgz` with different content that fails to compile. The database state rolls back, but the installed module's files are now the failed archive's files.
- **OBS-L8 `%Compile` ignores the status of resource processors' `OnPhase("Compile")`** (code-read).
  - Path: `do tResource.Processor.OnPhase("Compile",.pParams,.tHandled)` (`Base.cls:1081`).
  - Input: a custom processor whose `OnPhase("Compile")` returns an error status. Load reports Compile SUCCESS.
- **OBS-L9 Update steps are silently skipped when the update lock is not acquired in 3 s** (code-read).
  - Path: `lock +^IPM.UpdateLock(name):3`. If `'$test`, `return sc` with `sc=$$$OK` (`Storage/Module.cls:105-108`, `Common.inc:105`).
  - Input: run `update m` while another process holds `^IPM.UpdateLock("m")`. The update succeeds with no steps run, and history shows success.
- **OBS-L10 `-dev` commits the manifest before system requirements are checked** (code-read, consistent with the comment at `1310`). A `load -dev` of a module whose `SystemRequirements` fail leaves the module record committed and history Committed=1 (`1309-1321`). Listed for the matrix. Not an inconsistency with the comment.
- **OBS-L11 Lock file is written before the parent's commit** (code-read).
  - Path: `LockFile:CreateLockFileForModule` runs at the end of `LoadDependencies` (`Utils/Module.cls:1416-1422`), before `ExecutePhases`.
  - Input: `install m -lock` (or `load /dir/m -nodev -lock`) where `m` fails to compile. The non-dev transaction rolls the install back, but `module-lock.json` remains in the module root.
- **OBS-L12 A `.modules` directory that cannot be cached makes a successful load report `<INVALID OREF>`** (code-read, needs native confirmation).
  - Path: when `TempLocalRepoManager:%OnNew` returns an error, `%New` yields `""` (`TempLocalRepoManager.cls:64-68`), and `tmpRepoMgr.CleanUp()` (`Main.cls:2530`) throws after `LoadNewModule` already committed.
  - Input: `/dir/m/.modules/x/module.xml` is malformed, and `m` has no dependencies.
  - Expected: history Committed=1 with Success `<INVALID OREF>`, and the module installed.
- **OBS-L13 URL loads leave clones behind and ignore the git exit code** (code-read). Path: `Main.cls:2352-2372`. Input: `load https://host/bad/repo.git`. A temp directory is created and kept. The failure surfaces only as "File not found to obtain module object".
- **OBS-L14 `PermitDowngrade` does not reach `LoadNewModule`'s downgrade refusal** (code-read).
  - Path: `BuildDependencyGraph` honors `pPermitDowngrade` (`Storage/Module.cls:1157`), and `LoadModuleReference` skips its dev-mode guard (`Utils/Module.cls:103`). `LoadNewModule` refuses any downgrade unconditionally (`1214-1217`).
  - Input: `install A -DPermitDowngrade=1`, where A requires `B <1.5` and B 2.0 is installed. Fails with "Cannot downgrade b…".
- **OBS-L15 `LoadedResource` rows survive a rolled-back load** (code-read, P-2).
  - Path: `TrackResourceNames` writes `^IRIS.Temp.IPM.LoadedResourceD` during Reload (`Document.cls:186`).
  - Input: a non-dev `load /dir/m` that fails at Compile. `LoadedResource:ReportConflicts`/`Validate` (`@API`) still list m's resources.
- **OBS-L16 Legacy XML payload installs run outside any transaction but record `Committed=0` on failure** (code-read). Path: `Utils/Module.cls:175-184` has no `SetCommitted`, and `Finalize` ORs `ISOK` only. Contrast the property doc: "Whether any of the action's changes were kept, either by a commit or by writing outside a transaction" (`AbstractHistory.cls:41-43`). Input: a registry serving a non-gzip XML module that loads and then fails to compile.
- **OBS-L17 Uninstalling a module that others require, without `-force`, reports success** (code-read).
  - Path: `Storage.Module:Uninstall` sets `Clean.Level=1` (`Storage/Module.cls:742`). `CheckBeforeClean` sets Skip with OK status (`Base.cls:290-294`). `Lifecycle.Module:%Clean` quits with OK (`Lifecycle/Module.cls:12-15`).
  - Input: B depends on A; run `uninstall A`. The output says "Clean SKIPPED - required by 1 other module", the command succeeds, and history records uninstall A as Success with Committed=1. A remains installed.

### History

- **OBS-H1 One invalid filter column silently widens `history find` and `history delete`** (code-read).
  - Path: `ConstructSQLWhere` *quits* on the first column that fails `^[a-zA-Z][a-zA-Z0-9]*$` (`History.cls:103-105`). Remaining filters are dropped and the clause stays `WHERE 1 = 1`. The guard in `Main.cls:4165-4167` passes because `filter` has subscripts.
  - Input: `history delete -DVersion_Major=1` deletes every row in the namespace. `history schema` advertises `Version_Major`.
  - Variant: `history find -DCommitted=1 -DVersion_Major=0` filters only on Committed.
- **OBS-H2 Namespace default modifiers become history filters** (code-read).
  - Path: defaults are merged into `data` for every command (`Main.cls:1067-1069`). `-D` names without a dot are duplicated under `data("zpm",…)` (`CLI.cls:815-819`). `History` uses `data("zpm")` as the filter (`Main.cls:4136`).
  - Input: run `default-modifiers -set -DNoTransaction=1 -DUpdateSnapshots=1`, which is the product's own help example (`Main.cls:628-629`). Then `history find` prints nothing, because SQL on an unknown column fails and SQLCODE is unchecked. `history delete -confirm` throws a SQL error.
- **OBS-H3 Auto-purge likely deletes nothing** (needs native confirmation).
  - Path: `filter("TimeStart") = "<='"_cutoff_"'"` (`History.cls:173`). `ConstructSQLWhere` strips only `<=` and binds `'YYYY-MM-DD HH:MM:SS'` including the quotes (`History.cls:107-109`). `lastPurge` is still stamped (`176`).
  - Input: `config set HistoryRetain 1`, back-date a row's TimeStart, clear `^IPM.settings("history_retain","lastPurge")` in `%SYS`, then run any `zpm` command. No test covers AutoPurge.
- **OBS-H4 `config delete HistoryRetain` restores the first value ever set** (code-read).
  - Path: `SetValue` seeds `(key,"default")` on the first set (`UniversalSettings.cls` `SetValue`), and `ResetToDefault` reads it.
  - Input: `config set HistoryRetain 30`, `config set HistoryRetain 0`, `config delete HistoryRetain`. The value returns to 30 and purge is re-enabled.
- **OBS-H5** Same as OBS-L1, seen from the history side. Nested logs are durable only if the parent commits. A dependency's `Committed=1` set at a nested level (`Utils/Module.cls:1313/1350`) is never independently durable.
- **OBS-H6 The durable row lags until `%OnClose`** (code-read).
  - Path: all post-Init setters save only the IRISTEMP row (`AbstractHistory.cls:86-173`). The twin is written only at Init and at persist. The comment "Save it now in case Finalize isn't called" (`AbstractHistory.cls:61`, `141`) protects only the temp row.
  - Input: `load -dev /dir/m`, then terminate that process from another process (`$system.Process.Terminate(pid)`) during the Compile phase, after C1 and C2. The module record and its dependencies stay committed. The durable history row shows Package "", Version 0.0.0-0, Success 0, Committed 0, no TimeEnd and no phases. The IRISTEMP row with the real values stays orphaned until restart.
- **OBS-H7 Action and source attribution for nested logs** (code-read).
  - Dependencies of `load`, `update`, `test` and `verify` are logged as `load`, because only `cmd="install"` selects `InstallInit` (`Utils/Module.cls:64-68`). The dependency was still fetched from a registry and has SourceName set.
  - The display shows `n/a` for Source whenever Action≠install (`History.cls:461-463`). That includes a top-level `update` from a registry, which did call `SetSource` (`Main.cls:2631`).
  - Input: `update A` from a registry, then `history find`. Source shows `n/a`, and A's new dependencies appear as `load`.
- **OBS-H8 Recursive uninstall records skipped dependencies as successful uninstalls** (code-read). Path: `Base.cls:471-484`. Input: A and C both depend on B. `uninstall A -recurse` leaves B installed (required by C), yet B has an uninstall row with Success and Committed=1.
- **OBS-H9 `history delete -globally` needs no filter or confirmation** (code-read, consistent with the `-confirm` help text "Ignored if -globally is passed too"). Path: `Main.cls:4165`. Input: `history delete -g` deletes every history row in every namespace that has the class.
- **OBS-H10 Global readers and namespaces without IPM** (needs native confirmation).
  - `GetHistoryGlobally` never gets `$$$NULLOREF`, so "IPM is not enabled for this namespace" (`History.cls:496-499`) is unreachable.
  - `GlobalRecordsFetch` (`253-281`) fetches result sets prepared in other namespaces without switching `$namespace`, unlike `DisplayArray` (`528`).
  - Input: `SELECT * FROM %IPM_General.History_GlobalRecords(0,0)` (or `CALL`) from USER with history rows in a second namespace. Compare with `history find -g`.
- **OBS-H11 `Records` declares `CONTAINID=1` while its first column is `Action`** (code-read, `History.cls:28`). Input: consume `%IPM_General.History_Records` through a client that honors CONTAINID.
- **OBS-H12 AutoPurge runs only from `Shell`, and its failure blocks the command** (code-read). Path: `Main.cls:862-865`. `ShellInternal` and API writers never purge. Input: make `DeleteHistoryGlobally` throw before its per-namespace try. The user's command is not run and `Shell` returns the purge error. Purge also deletes unfinalized rows of in-flight operations started before the cutoff.
- **OBS-H13 Full cleanup kills the instance-wide history temp global** (code-read, P-1).
  - Path: `kill ^IRIS.Temp.HistoryTempD` (`Lifecycle/Module.cls:51`) removes in-flight temp rows of every namespace and resets the ID counter. A live HistoryTemp oref in another process may then `%Reload` a different log's row that reuses its ID.
  - Input: `uninstall zpm` with full cleanup while another namespace runs `install`.
- **OBS-H14 Invoke failures leave the phase unfinalized** (code-read).
  - Path: the `quit:$$$ISERR(tSC)` exits at `Storage/Module.cls:608-610` and `669-671` bypass the catch, so `FinalizePhase` is never called. The phase keeps `TimeEnd=""` and `Success=0`.
  - Input: a module with `<Invoke Class="Invoke.Test" Method="ReturnError" Phase="Activate" When="After" CheckStatus="1"/>` (adapting `tests/integration_tests/Test/PM/Integration/_data/invoke/failure/ReturnErrorCheckStatus`). Run `load <dir>`, then `history details <id> -phases`.
- **OBS-H15 Self-uninstall reports success before it runs and exits the shell even on failure** (code-read).
  - Path: `Finalize($$$OK)` and `PersistToTwin` happen before `%Clean` (`Storage/Module.cls:630-640`). `^||%IPM.SelfUninstallFlag` is set before the uninstall call (`Main.cls:2746-2749`). If Clean fails, the persisted row stays Success with Committed=1 and the shell still prints "IPM has been uninstalled from this namespace. Exiting shell..." (`Main.cls:1152-1155`).
  - Input: install a module whose manifest declares a dependency on `zpm`, then run `uninstall zpm` and answer No to the full-cleanup prompt. `CheckBeforeClean` skips with an OK status (`Base.cls:290-294`). IPM stays installed. The history row reads uninstall zpm Success with Committed=1, and the shell exits saying IPM was uninstalled. Afterwards `%OnClose` still runs `PersistToTwin` on the already-deleted temp row, so `%Reload` throws inside `%OnClose`. The effect of that is unconfirmed.
- **OBS-H16 A caller's transaction absorbs IPM's commits and the history row** (code-read, P-4).
  - Path: `tInitTLevel` makes every IPM `tcommit` nested. Init and persist run at the caller's level.
  - Input: `tstart`, `do ##class(%IPM.Main).Shell("load /dir/m")`, `trollback`. Neither the module nor any history row remains, although the in-flight log had Committed=1. The same applies to `Shell("load …")` issued from an Invoke inside another module's non-dev load.

### Cross-cutting

- **OBS-X1 "Namespace" default modifiers are stored in a `^%` global** (needs native confirmation, P-10).
  - Path: `$$$ZPMDefaultModifiers = ^%IPM.DefaultModifiers` (`Common.inc:31`).
  - Input: `default-modifiers -set -DNoTransaction=1` in USER, then `default-modifiers -get` in another IPM namespace. If the setting is visible there, every load in every namespace becomes non-transactional.

## 5. Coverage limits

- **Not traced to the line:**
  - `BuildDependencyGraph`/`ProcessSingleDependencyIterative` internals (`Storage/Module.cls:898-1300`) beyond their guard/throw contract.
  - Repo search and caching internals (`Repo/Manager.cls`, `Repo/Oras/*`, `Repo/Remote/*`) beyond `GetModule`/`CheckServiceCache`.
  - `%Export`/`%Package`/`%Publish`/`%Verify` bodies.
  - `Sync.Pipeline` and `FileHash` internals.
  - `EnableIPM`/`UnmapIPM` beyond their write sets.
  - `Init` prompts.
  - Each resource processor's full logic (only their write classes and phases are listed).
  - `ModuleTemplate`, `DependencyAnalyzer`, `Migration` steps.
  - The CLI parser grammar beyond `-D` handling.
- **Engine semantics:** §0 premises P-1…P-10 are assumed from IRIS behavior and from IPM's own comments, not verified here. In particular: IRISTEMP versus rollback, compile and CPF behavior inside transactions, multicompile workers, `%Reload` of swizzled references (P-9, which decides whether `PersistToTwin` can resurrect a deleted twin), `%OnClose` exceptions, and `^%` global placement.
- **Concurrency:** two processes loading or updating the same module, the `ipm-temp-modules-N` name race, lock-escalation and lock-retention behavior of `%Save` inside T1, and `^IPM.UpdateLock` contention were identified but not analyzed.
- **Self-upgrade:** loading IPM's own package recompiles `%IPM.General.HistoryTemp`/`History` while a log oref of that class is live. Not traced.
- **Interactive paths:** `%Library.Prompt` behavior under non-interactive `Shell` calls (install `-prompt`, uninstall `zpm` confirmation, `init`) not traced.
- **Platform:** Windows-specific branches (`ResetProtectedPermissions`, path handling) and VMS `RunCommandViaZF` were not traced.
- **Tests:** existing tests were read only to locate fixtures and the tested surface (`tests/integration_tests/Test/PM/Integration/History.cls`, `Uninstall.cls`). They do not bound this inventory. AutoPurge, nested-log rollback, filter validation, `GlobalRecords` and abnormal end have no tests.

## 6. Counts

| Area | Operations |
|---|---|
| Shell commands (breadth table) | 39 |
| Non-shell entry-point groups (§1.3) | 13 |
| Load depth (OP-LOAD-01…29) | 29 |
| History depth (OP-HIST-01…26) | 26 |
| Observations | 17 load, 16 history, 1 cross-cutting |
