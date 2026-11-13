# Coverage map (stage A3)

Joins the A1 [intent inventory](intent-inventory.md) and the A2 [operation inventory](operation-inventory.md) to the governing ontology in [`.g5/ontology`](../../../ontology/root.g5.md). This map locates evidence. The ontology bodies govern.

- Product pin `571e04a1`; wiki `781e98fd`. Locators as in A1 and A2.
- Ontology: 18 pages, 147 blocks (122 I, 5 D, 20 Q). Q-01 to Q-17 come from A1, and Q-12 is now closed as a decision. Q-18 and Q-19 are new in A3. Q-20 came from the A4 [reconstruction check](reconstruction-check.md) (row dispositions in [reconstruction-dispositions.md](reconstruction-dispositions.md)), and Q-21 from stage C round 1 (`../C/round-1/repairs-ontology.md`).
- Native evidence: B2 phase 1 results ([native report](../B/native-report.md)) are cited on the owning blocks and summarized in section 9. Where section 9 gives a status, it replaces `pending B` for native evidence. Formal (B1) evidence is still pending.
- Columns. **Commitment**: A1 ID and summary. **Governing source**: the strongest human source checked by A3. **Owner**: ontology block IDs. **Operations**: A2 IDs with the main symbol. `CMD-n` is row n of A2 section 1.2, and `EP` names a row of A2 section 1.3. **Model/evidence**: `pending B` for the formal scope (load and history), `out of formal scope` otherwise. Existing product tests are cited in the ontology metadata. **Status**: `covered`, `partial: <what is missing>` or `unmatched`.
- "Missing" in a status means a clause is unresolved (Q), contradicted by observed code (suspected gap, stage B to confirm), or has no operation. It never means the ontology adopted the observed behavior.

## 1. Commitment map

### 1.1 Package loading (formal scope)

| Commitment | Governing source | Owner | Operations | Model/evidence | Status |
|---|---|---|---|---|---|
| INT-LOAD-01 `load` from folder, archive or git URL, plus dependencies | `Main.cls:268-291` help; `wiki/02` L564-604 | `load`, `load.path` | OP-LOAD-01 `Main:Load`; OP-LOAD-02 `Main:LoadInternal`; OP-LOAD-03 `Main:LoadFromRepo` | pending B | partial: `.tar.gz` not recognized (OBS-L2); git clone kept and exit code ignored (OBS-L13, no intent either way) |
| INT-LOAD-02 empty and relative paths refused; missing path fails | CHANGELOG `#706`, `#726`; messages `Main.cls:2447-2452` | `load.refusal.path` | OP-LOAD-02 | pending B | partial: an existing relative path is accepted; no test |
| INT-LOAD-03 archive unpacked to `<ModuleRoot>/<name>/<version>/`; temp folder removed | CHANGELOG `#945`, `#1183`; `wiki/02` L649-653 | `load.path.archive`, `transaction.rollback-scope` | OP-LOAD-02 | pending B | partial: install folder replaced before version checks and not restored (OBS-L7, Q-04); no `load` archive test |
| INT-LOAD-04 one loading path for load, install, update, dependencies | `Main.cls:273`, `:407`; `wiki/00` L43-56 (class b) | `load.core`, `install`, `update`, `lifecycle.chain` | OP-LOAD-05 to OP-LOAD-16 `Utils.Module:LoadNewModule`; OP-LOAD-17 to OP-LOAD-21 | pending B | partial: legacy XML packages bypass it (OBS-L16) |
| INT-LOAD-05 `-dev` keeps entries on failure; normal mode removes them | `-dev` help `Main.cls:306`, `:434`; issue #363 comment (isc-tleavitt); `wiki/00` L125-126 | `dev-mode`, `dev-mode.keep-on-failure`, `load.outcome.dev` | OP-LOAD-10, OP-LOAD-11, OP-LOAD-14, OP-LOAD-15, OP-LOAD-16 | pending B | covered |
| INT-LOAD-06 developer-mode commits: manifest, then dependencies; phases outside a transaction | `wiki/00` L126; maintainer comments `Utils/Module.cls:1310-1337` | `load.outcome.dev`, `load.outcome.dev.early-failure` | OP-LOAD-11 (first commit, `:1311`); OP-LOAD-14 (second commit, `:1338`) | pending B | partial: nested dependency commits are not final (Q-05) |
| INT-LOAD-07 developer mode not passed to dependencies | CHANGELOG `#316`, `#1036`; `wiki/00` L133 | `dev-mode.not-propagated`, `dev-mode.not-propagated.local-snapshot`, `update.dev-mode` | OP-LOAD-18 `LoadModuleReference`; OP-LOAD-20 `LoadModuleFromDirectory` | pending B | partial: `update -dev` from a registry ends in normal mode (OBS-L4) |
| INT-LOAD-08 one transaction; `NoTransaction`; `NoJournal`; flag sources | `wiki/00` L139-180; `Main.cls:628-630` | `transaction`, `transaction.opt-out`, `transaction.flag-sources`, `transaction.rollback-scope` | OP-LOAD-05; OP-LOAD-07; OP-LOAD-16; OP-LOAD-26 `JournalManager` | pending B | partial: rollback scope (Q-04); default-modifier storage scope (OBS-X1) |
| INT-LOAD-09 downgrade always refused | message `Utils/Module.cls:1214-1217`; PR #887 r2392741268 | `load.refusal.downgrade`, `load.outcome.dev.early-failure` | OP-LOAD-06 | pending B | covered (`PermitDowngrade` note, disagreement 1) |
| INT-LOAD-10 newer version with `UpdatePackage` refused unless `-force` | README L62; `Main.cls:313`, `:441` | `load.refusal.update-required`, `load.refusal.update-required.dev-bypass` | OP-LOAD-06 | pending B | partial: `-dev` bypass unresolved (Q-02) |
| INT-LOAD-11 system requirements refused | `wiki/03` L260-269; CHANGELOG `#106` | `load.refusal.requirements`, `load.refusal.requirements.timing`, `module.requirement` | OP-LOAD-12 `Storage.Module:CheckSystemRequirements` | pending B | partial: check runs after the manifest write in developer and no-transaction modes (Q-17, OBS-L10) |
| INT-LOAD-12 developer-mode module not overwritten by normal install | `wiki/00` L129; message `Utils/Module.cls:109-112` | `dev-mode.overwrite-guard`, `dev-mode.overwrite-guard.force` | OP-LOAD-18 | pending B | partial: `-force` and `load` coverage unresolved (Q-03); no test |
| INT-LOAD-13 read-only repository forces normal mode | `Main.cls:262`; `wiki/00` L135 | `repo.read-only`, `dev-mode` | OP-LOAD-10 | pending B | covered (ordering against Q-02 noted) |
| INT-LOAD-14 dependencies one at a time; `.modules` temporary repository | CHANGELOG `#885`; PR #726 body | `load.dependencies`, `load.dependencies.local-repo`, `deps.installed` | OP-LOAD-04 `TempLocalRepoManager`; OP-LOAD-13 `LoadDependencies` | pending B | partial: cleanup can fail after commit (OBS-L12) |
| INT-LOAD-15 other options, lock file, `-nodev` | `Main.cls:304-315` | `load.options`, `deps.lockfile`, `module.env`, `module.python`, `dev-mode.sticky` | OP-LOAD-02; OP-LOAD-13 `LockFile:CreateLockFileForModule`; OP-LOAD-25 | pending B (lock file, developer-mode choice) | partial: plain load keeps developer mode (Q-01); lock file path and survival after rollback (OBS-L3, OBS-L11) |
| INT-LOAD-16 write permission checked | CHANGELOG `#1053` | `load.write-check` | OP-LOAD-19 `LoadModuleFromArchive` | pending B | partial: no explicit check on the `load` archive path |
| INT-LOAD-17 every load recorded | `Main.cls:820` | `history.actions` | OP-LOAD-01 (`LoadInit`, `Main.cls:2420`) | pending B | covered |
| INT-LOAD-18 same version allowed | PR #887 r2388507362, r2392741268 | `load.same-version` | OP-LOAD-06 | pending B | partial: no test |
| A1 2.2 valid completion and neighbors | tests `History.cls` | `load.outcome`, `load.outcome.dev`, `load.outcome.dev.early-failure`, `history.committed` | OP-LOAD-06, OP-LOAD-11, OP-LOAD-15, OP-LOAD-16 | pending B | partial: tests do not read module state after failures |

### 1.2 Transaction history (formal scope)

| Commitment | Governing source | Owner | Operations | Model/evidence | Status |
|---|---|---|---|---|---|
| INT-HIST-01 install, load, uninstall, update recorded; others not | `Main.cls:820`; `wiki/09` L18; issue #366 | `history`, `history.actions` | OP-HIST-02 Init callers | pending B | covered |
| INT-HIST-02 record survives any rollback that undoes its action, including a parent's rollback of a dependency | issue #366; `wiki/09` L18, L22 | `history.survives-rollback`, `history.survives-rollback.temp-copy`, `history.committed.nested` (caller transactions) | OP-HIST-01 `HistoryTemp:Init`; OP-HIST-10 `PersistToTwin`; OP-HIST-11 `%OnClose` | B2: confirmed defect D1 (A1-A3); caller case D1 deviates; formal pending | partial: dependency records lost (D1); caller transactions open (Q-05) |
| INT-HIST-03 record created at start; running and abnormally ended actions listed with `Success` 0, no end time and started phases; unfinished records not finalized | `wiki/09` L20, L72, L161, L229-233; `AbstractHistory.cls:6-7`, `:32`, `:61` | `history.record`, `history.record.abnormal-end`, `history.survives-rollback.temp-copy`, `history.phases`, `history.locked.unfinished` | OP-HIST-01; OP-HIST-07 `Finalize`; OP-HIST-10; OP-HIST-11; OP-HIST-26 | B2: C1, C3, C4R deviate on phases (no phases listed); C2 finalized without end; C4R `Committed=0` after kept changes | partial: phase listing after a crash; Q-08; Q-21 |
| INT-HIST-04 and A1 3.2 fields; `Package` empty only before any name is known | `AbstractHistory.cls:11-49`; `wiki/09` L63-78 | `history.fields`, `history.fields.dependency-source` | OP-HIST-01, OP-HIST-03 `SetSource`, OP-HIST-04 `SetVersion`, OP-HIST-05 `SetName`, OP-HIST-07, OP-HIST-25 | B2: refused loads record no package (D9: g3, g6, g7, R4) | partial: D9; dependency label (Q-15) |
| A1 3.2 `Phases` | `wiki/09` L135-161 | `history.phases`, `history.phases.returned-errors` | OP-HIST-08 `InitPhase`; OP-HIST-09 `FinalizePhase`; OP-LOAD-23 `ExecutePhases` | pending B | partial: returned Invoke errors leave a phase open (Q-14, OBS-H14) |
| INT-HIST-05 `Committed` means changes kept | `AbstractHistory.cls:41-42`; CHANGELOG `#1237`; issue #1237; PR #1322 | `history.committed`, `history.committed.marking`, `history.committed.nested`, `history.committed.side-effects` | OP-HIST-06 `SetCommitted`; OP-HIST-07; OP-LOAD-07, OP-LOAD-11, OP-LOAD-15 | pending B | partial: nested transactions (Q-05, OBS-H16); side effects (Q-07); legacy XML (OBS-L16); requirement refusal reads committed (Q-17) |
| INT-HIST-06 uninstall `Committed` equals success | `AbstractHistory.cls:43`; PR #1322 | `history.committed.uninstall`, `uninstall` | OP-HIST-02; OP-HIST-24; CMD-22 `Main:Uninstall` | pending B | partial: skipped uninstall reads success while the module stays (OBS-L17, OBS-H8) |
| INT-HIST-07 `Success` and `Committed` differ | `Main.cls:831`; `wiki/09` L80-86 | `history.committed.vs-success` | OP-HIST-07 | pending B | covered |
| INT-HIST-08 dependency installs recorded | issue #366; PR #578 TODO | `history.dependencies`, `history.dependencies.parent-rollback`, `history.fields.dependency-source` | OP-HIST-02; OP-LOAD-13; OP-LOAD-18 | B2: A0, A4, A5 conform; D1 under parent rollback | partial: D1; Q-06 (`Success`), Q-15 |
| INT-HIST-09 when a record exists | code (class b/d) | `history.record`, `history.record.unlogged-refusals`, `history.actions` | OP-HIST-02; OP-LOAD-01; OP-LOAD-17; OP-LOAD-21 | pending B | partial: update and uninstall argument refusals unrecorded (Q-09) |
| INT-HIST-10 `history` actions, sort, limit, filters | `Main.cls:818-843`; `wiki/09` L24-112 | `history.read`, `history.read.find`, `history.read.filter`, `history.read.filter.invalid-column`, `history.read.filter.default-modifiers`, `history.read.details` | OP-HIST-13 `GetHistory`/`ConstructSQLWhere`; OP-HIST-15; OP-HIST-16 | pending B | partial: dropped filters (Q-10, OBS-H1); default modifiers as filters (Q-18, OBS-H2) |
| INT-HIST-11 `-globally` and API | `wiki/09` L163-171, L199-211; issue #366 | `history.read.global`, `history.read.api` | OP-HIST-14; OP-HIST-19 `Records`; OP-HIST-20 `GlobalRecords` | pending B | partial: `GlobalRecords` and `Records` shape (OBS-H10, OBS-H11) |
| INT-HIST-12 delete with `-confirm` and `-globally` | `Main.cls:825`; `wiki/09` L187-195 | `history.delete`, `history.delete.confirm-with-filter`, `history.delete.global-unfiltered` (D), `history.delete.running-records` | OP-HIST-17 `DeleteHistory`; OP-HIST-18 `DeleteHistoryGlobally` | pending B | partial: Q-11; deleting a running action's record (Q-20) |
| INT-HIST-13 retention purge | `wiki/09` L173-185; CHANGELOG `#992`; `Main.cls:839-841` | `history.retention`, `history.retention.effect`, `history.delete.running-records`, `ipm.shell` | OP-HIST-21 `AutoPurgeOldHistory`; OP-HIST-22 | B2: confirmed defects D3 (F1) and D5 (F0); purge failure not run | partial: D3, D5; Q-13; Q-20 |
| INT-HIST-14 finalized records cannot be edited | `wiki/09` L197; PR #578 reviews | `history.locked`, `history.locked.unfinished` | OP-HIST-12 trigger `PreventSQLUpdates`; OP-HIST-10 | B2: N-P13 confirmed; C2 finalized an unfinished record | partial: Q-21 |
| INT-HIST-15 history survives IPM uninstall | `Main.cls:2735`; comment `Lifecycle/Module.cls:54` | `history.keep-on-ipm-uninstall`, `history.keep-on-ipm-uninstall.other-namespaces`, `uninstall.ipm` | OP-HIST-23 | pending B | partial: shared IRISTEMP global killed (Q-16, OBS-H13); success recorded before Clean (OBS-H15) |
| INT-HIST-16 per namespace | `History.cls:3`; `wiki/09` L20; PR #578 | `history.namespace` | OP-HIST-02 | pending B | covered |
| INT-HIST-17 IRIS audit database | issue #366 comment | none (future note in `history` metadata) | none | out of formal scope | unmatched: future option only |
| A1 3.3 valid completion and neighbors | tests `History.cls` | `history.fields`, `history.record`, `history.locked`, `history.delete` | OP-HIST-01 to OP-HIST-17 | pending B | covered |

### 1.3 Capability catalog (breadth)

Model/evidence is `out of formal scope` unless the row says otherwise.

| Commitment | Governing source | Owner | Operations | Model/evidence | Status |
|---|---|---|---|---|---|
| CAT-01 module identity, `ExternalName` | `wiki/03` L99-115 | `module`, `module.identity` | OP-LOAD-09 (`StudioDocument.Module:ImportStream`, `Storage.Module:%OnValidateObject`) | out of formal scope | covered |
| CAT-02 resources and scope | `wiki/03` L279-343; CHANGELOG `#1023` | `module.resource`, `module.resource.scope`, `build.test`, `build.package` | OP-LOAD-24 `Default.Document:OnPhase` | out of formal scope | partial: `Scope="verify"` deferred |
| CAT-03 dependencies in the manifest | `wiki/03` L243-250 | `module.dependency`, `deps` | OP-LOAD-13 | out of formal scope (used by load: pending B) | partial: dependency `Scope` described only by code and tests |
| CAT-04 defaults and parameters | `wiki/03` L252-258 | `module.parameter`, `transaction.flag-sources` | OP-LOAD-05 `GetDefaultParameters`; OP-LOAD-23 | flag sources pending B | covered |
| CAT-05 system requirements | `wiki/03` L260-272 | `module.requirement`, `load.refusal.requirements` | OP-LOAD-12 | pending B | partial: Q-17 |
| CAT-06 Invoke and custom phases | `wiki/03` L345-362; `wiki/00` L85-103 | `module.invoke`, `lifecycle.custom` | OP-LOAD-23 | out of formal scope | partial: returned errors leave phase open (Q-14) |
| CAT-07 resource processors | `wiki/03` L581-785 | `module.processor`, `module.processor.file-copy` | OP-LOAD-24; A2 1.3 processor hooks | out of formal scope | partial: Compile ignores processor status (OBS-L8); `<WSGIApplication>` has no processor |
| CAT-08 Python dependencies | `wiki/03` L117-178; `wiki/07` | `module.python` | OP-LOAD-25 | out of formal scope | covered (rollback scope Q-04) |
| CAT-09 update steps | `wiki/08`; `wiki/03` L365-380 | `update.steps` | EP update steps (`Storage.Module:HandleAllUpdateSteps`, `ExecuteUpdateStep`) | out of formal scope | partial: lock timeout skips steps (OBS-L9); new dependency runs all steps (OBS-L5); mirroring future |
| CAT-10 mapping and placement | `wiki/03` L188-241 | `module.placement`, `uninstall.global`, `settings.install-flags` | OP-LOAD-23 (`%Reload` mappings) | out of formal scope | partial: `${authUnauthenticated}` not implemented |
| CAT-11 lifecycle phases | `wiki/00` L3-83; `Main.cls:32-83` | `lifecycle`, `lifecycle.chain`, `lifecycle.only`, `lifecycle.module-action`, `lifecycle.hooks` | CMD-3 `Main:ModuleAction`; CMD-5, CMD-7 `RunOnePhase`; EP lifecycle phases | load chain pending B | partial: `register` phase documented but missing |
| CAT-12 load | section 1.1 | `load.*` | OP-LOAD-01 to OP-LOAD-16 | pending B | see 1.1 |
| CAT-13 install | `Main.cls:406-445`; README L62 | `install`, `install.refusal`, `install.package` | OP-LOAD-17 `Main:Install`; OP-LOAD-18; OP-LOAD-19 | shared path pending B | covered |
| CAT-14 reinstall | `Main.cls:447-481` | `reinstall`, `reinstall.version` | OP-LOAD-22 `Main:Reinstall` | shared path pending B | partial: version unresolved (Q-19) |
| CAT-15 update | `Main.cls:164-187`; `wiki/08` | `update`, `update.refusal`, `update.dev-mode`, `update.requirements` | OP-LOAD-21 `Main:Update` | shared path and history pending B | partial: OBS-L4, OBS-L5, OBS-L6 |
| CAT-16 uninstall | `Main.cls:483-507`; CHANGELOG `#058` | `uninstall`, `uninstall.*` | CMD-22 (`Main:Uninstall`, `Storage.Module:Uninstall`, `Utils.Module:UninstallAll`); OP-HIST-23; OP-HIST-24 | history records pending B | partial: dependents not protected (OBS-L17); `-purge` has no reader |
| CAT-17 orphans | `Main.cls:524-537` | `inspect` | CMD-24 `Main:ListOrphans` | out of formal scope | covered |
| CAT-18 dependency resolution | CHANGELOG `#1192`, `#1077`, `#430`, `#316`, `#261` | `deps`, `deps.conflict`, `deps.installed`, `update.requirements`, `repo.search-order` | OP-LOAD-13 `BuildDependencyGraph` | out of formal scope | covered |
| CAT-19 versions | `Main.cls:260-261`; CHANGELOG 0.9.0 | `version`, `version.range`, `version.post-release`, `repo.prereleases` | no A2 operation traced (`%IPM.General.SemanticVersion`) | out of formal scope | partial: no operation in A2 |
| CAT-20 repository configuration | `Main.cls:222-266` | `repo`, `repo.manage` | CMD-16 `Main:Repository` | out of formal scope | partial: `-app`, `-local`, `-info`, `-p4` not implemented |
| CAT-21 default registry | README L23-30 | `repo.default-registry` | EP installer (`Installer.cls:114`) | out of formal scope | covered |
| CAT-22 ORAS registries | CHANGELOG `#1267`, `#1269`, `#1281`, `#1115` | `repo.oras` | OP-LOAD-18 (ORAS `GetModule`, `ModuleNameFromRepository`) | out of formal scope | covered |
| CAT-23 filesystem repositories, `.modules` | PR #726 | `repo.filesystem`, `load.dependencies.local-repo` | OP-LOAD-04; OP-LOAD-18 | `.modules` pending B | covered |
| CAT-24 publish and unpublish | `Main.cls:153-162`, `:197-208` | `build.publish` | CMD-11; CMD-14 `Main:Unpublish` | out of formal scope | covered |
| CAT-25 discovery and inspection | `Main.cls:539-616`, `:683-724` | `inspect` | CMD-1, CMD-25 to CMD-27, CMD-31, CMD-33, CMD-34, CMD-38 | out of formal scope | covered |
| CAT-26 settings | `wiki/02` L93-141 | `settings` | CMD-15 `Main:Config`; OP-HIST-22 | history keys pending B | partial: `config delete` restores first value (OBS-H4) |
| CAT-27 default modifiers | `Main.cls:617-637`; `wiki/02` L143-166 | `settings.default-modifiers`, `transaction.flag-sources`, `history.read.filter.default-modifiers` | CMD-28 `Main:ProcessDefaultModifiers`; `ShellInternal` merge (`Main.cls:1067-1069`) | flag sources and history filters pending B | partial: storage scope (OBS-X1); history filters (Q-18) |
| CAT-28 install `-D` flags | `wiki/00` L139-180 | `transaction.opt-out`, `settings.install-flags` | OP-LOAD-05; OP-LOAD-26 | `NoTransaction`/`NoJournal` pending B | partial: `NoLock` undocumented and inverted; `PermitDowngrade`, `IgnoreInstalled` code-only (disagreement 1) |
| CAT-29 namespace ownership | README L23-70 | `namespace`, `namespace.*` | CMD-35 `Main:EnableIPM`; CMD-36 `Main:UnmapIPM`; OP-LOAD-17 `CheckModuleNamespace` | out of formal scope | covered |
| CAT-30 developer mode | `wiki/00` L107-135 | `dev-mode`, `dev-mode.*`, `load.outcome.dev` | OP-LOAD-10, OP-LOAD-11, OP-LOAD-14, OP-LOAD-18, OP-LOAD-20 | pending B | partial: Q-01, Q-03 |
| CAT-31 sync | `Main.cls:108-119`; issue #1224 | `devtools.sync`, `devtools.sync.retry` | CMD-6 `Main:Sync`, `Sync.Pipeline:Run` | out of formal scope | covered |
| CAT-32 package | `Main.cls:135-144` | `build.package` | CMD-9 | out of formal scope | covered |
| CAT-33 verify | `Main.cls:52`; `wiki/00` L71-81 | `build.verify` | CMD-10; OP-LOAD-28 | out of formal scope | covered (disagreement 3) |
| CAT-34 deployed code | `wiki/05`; CHANGELOG `#1158`, `#1242`, `#1279` | `module.deployed`, `build.makedeployed`, `install.package` | CMD-13; OP-LOAD-24 | out of formal scope | covered |
| CAT-35 lock files | CHANGELOG `#961`; issue #962 | `deps.lockfile` | OP-LOAD-13 `LockFile:CreateLockFileForModule` | written inside a load: pending B | partial: OBS-L3, OBS-L11; install from lock file is future |
| CAT-36 authoring utilities | `Main.cls:85-99`, `:322-404`, `:639-705` | `devtools.authoring`, `devtools.scripting` | CMD-4, CMD-18, CMD-19, CMD-23, CMD-29, CMD-30, CMD-32, CMD-37 | out of formal scope | partial: `run-from-file` help disagrees with the format |
| CAT-37 environment files | `Main.cls:66`; CHANGELOG `#1260` | `module.env` | OP-LOAD-02 (`EnvFiles`); `ShellInternal` EnvironmentConfig | out of formal scope | covered |
| CAT-38 installer and self-upgrade | README L13-42; `Installer.cls` | `setup`, `setup.installer`, `namespace.legacy` | EP installer bootstrap; OP-LOAD-27 | its load pending B | covered |
| CAT-39 test output | `Main.cls:76-79`; `wiki/02` L1194-1246 | `build.test-output` | CMD-8; CMD-10 | out of formal scope | covered |
| CAT-40 analytics | `wiki/02` L111, L124-138 | `settings.analytics` | OP-LOAD-18 `CollectAnalytics` | out of formal scope | partial: payload undocumented |
| CAT-41 scripting entry points | `wiki/01` L109-167; CHANGELOG `#1186` | `ipm.shell` | A2 1.1 `Main:Shell`, `ShellInternal`; EP `ShellScript`, `zpm` routine | purge-before-command pending B | partial: purge failure blocks the command (Q-13) |
| CAT-42 history | section 1.2 | `history.*` | OP-HIST-01 to OP-HIST-26 | pending B | see 1.2 |

## 2. A2 operations without an ontology owner

Every OP-LOAD and OP-HIST operation has an owner except OP-LOAD-08. Every shell command except `quit` has an owner. Owners by operation are in section 1. The table lists the rest.

| Operation | Disposition | Reason |
|---|---|---|
| OP-LOAD-08 `ResourceReference:MarkResourceForUpdate` | structural | Internal marker so `update` does not report its own resources as conflicts. No human statement. Its writes ride the load's transaction, which `load.outcome` covers. |
| CMD-2 `quit` | structural | Leaves the shell. No behavior to govern. |
| EP `%IPM.Main:ShellInternal` | structural | Internal entry used by tests and `enable -community`. It skips the history purge (OBS-H12). Governed through `ipm.shell`. |
| EP `.ZPM` document (`%IPM.StudioDocument.Module`) | structural | Storage of the manifest. Its nested transaction is part of OP-LOAD-09, owned by `load.refusal.manifest` and `load.outcome`. |
| EP other `@API` methods: `LoadedResource` `Validate`/`Clear`/`ReportConflicts`, `LogManager`, `Utils.Build`, `DependencyAnalyzer` run methods, `XSLTProvider`, `LocalizationExport`, `TempFileManager`, `GetDependentsList`/`GetDependentsAsTree` | excluded | Internal or library APIs with no recovered human intent. `LoadedResource` rows survive a rollback (OBS-L15), noted under `transaction.rollback-scope`. |
| EP SQL procedures and queries (`GET_DEFAULT_RESOURCE_PROCESSOR`, `OrphanedResources`, `VersionRequirements` and others) | excluded | Read-only helpers behind owned commands. |
| EP source-control hooks (`%IPM.ExtensionBase.SourceControl.Interface`) | gap | Only `init` and `import` mention source control. No human contract for the hook calls was recovered. Outside the formal scope. |
| `Lifecycle.Module:%Activate` Studio project creation in developer mode | excluded | Editor convenience, errors ignored. No human statement. |
| `Lifecycle.Module:%Activate` update-step seeding | structural, noted | Observed in `update.steps` metadata. No human statement says a normal install marks steps as done. |

## 3. A1 commitments without an operation

| Commitment | Owner | Disposition |
|---|---|---|
| INT-HIST-17 IRIS audit database for history | `history` metadata | Future option. No block, because it is not a current commitment. |
| CAT-35 install from a lock file (issue #962) | `deps.lockfile` bullet | Future. Stated as "planned but not available". |
| CAT-09 mirrored updates; `VersionBase.cls:18` steps "when becoming primary" | `update.steps` metadata | Future (`wiki/08` L5). The class comment's promise has no operation. |
| CAT-10 `${authUnauthenticated}` | `module.placement` metadata | Documented, not implemented. Not stated as intent. |
| CAT-11 `register` phase | `lifecycle` metadata | Documented in help, not implemented. Not stated as intent. |
| CAT-20 `repo -app`, `-local`, `-info`, `-p4` | `repo.manage` metadata | Documented in help, not implemented. Not stated as intent. |
| CAT-07 `<WSGIApplication>` (`wiki/06`) | `module.processor` metadata | No processor. Not stated as intent. |
| CAT-16 `uninstall -purge` | `uninstall.purge` | Operation accepts the flag, but nothing reads it. Suspected gap. |
| CAT-16 uninstall prevented for dependents | `uninstall.dependents` | Operation skips instead of refusing. Suspected gap. |
| CAT-37 environment-file wiki page | `module.env` metadata | Documentation missing. |

## 4. Partially covered clauses inside linked owners

Each row is a clause of an owned block that the A2 operations do not fully satisfy, or that stays open. Stage B should confirm each suspected gap at the real handler and store.

| Owner | Clause | What is missing | Evidence |
|---|---|---|---|
| `load.path` | `.tar.gz` archives | Not recognized as an archive | OBS-L2 |
| `load.refusal.path` | only absolute paths | An existing relative path is accepted | OP-LOAD-02 |
| `load.refusal`, `load.refusal.manifest` | refusals keep no module data in any mode | Under `-DNoTransaction=1`, late manifest checks leave the manifest and read committed | OP-LOAD-09, A1 2.3, A4 R6 |
| `load.refusal.requirements` | checked before installation | Checked after the manifest write and the developer-mode first commit (confirmed, B2 D4: B2-B4) | Q-17, OBS-L10 |
| `load.refusal.downgrade` | no bypass | Holds. `PermitDowngrade` honored by the graph but not the guard | OBS-L14 |
| `load.path.archive`, `install.package` | rollback leaves the namespace as it was | Install folder replaced before version checks | OBS-L7, Q-04 |
| `load.dependencies.local-repo` | removed even after an error | Cleanup fails with `<INVALID OREF>` when the folder cannot be cached | OBS-L12 |
| `load.write-check` | checked before installing | Only repository archives are checked | OP-LOAD-19 |
| `load.core` | one loading path | Legacy XML packages bypass it | OBS-L16 |
| `deps.lockfile` | file in the module root | Wrong folder on a first plain load; survives rollback | OBS-L3, OBS-L11 |
| `dev-mode.not-propagated`, `update.dev-mode` | `update -dev` | Module itself ends in normal mode when updated from a registry | OBS-L4 |
| `dev-mode.sticky` | plain load keeps mode | Open, and `Root` and `Repository` are not saved | Q-01, OBS-L3 |
| `update.steps` | steps from current to new version | Lock timeout skips steps; new dependency runs all steps | OBS-L9, OBS-L5 |
| `update.refusal` | `-path` mismatch keeps no module data | Checked after the manifest write | OBS-L6, A4 R6 |
| `module.processor` | processor errors fail the phase | Compile ignores processor status | OBS-L8 |
| `history.survives-rollback` | dependency records survive a parent rollback | Lost (confirmed, B2 D1) | OBS-L1, OBS-H5, B2 A1-A3 |
| `history.record`, `history.phases` | running or never-finished actions listed with their started phases | Permanent record keeps only start values, so no phases and an empty package show until the end | OBS-H6 |
| `history.record.abnormal-end` | `Committed` on an unfinished record | Open | Q-08 |
| `history.phases` | every phase records its end | Returned Invoke errors leave the phase open (confirmed, B2 D6) | Q-14, OBS-H14 |
| `history.committed`, `history.committed.nested`, `history.dependencies.parent-rollback` | kept changes, including dependencies undone by the parent | Nested commits for every dependency and caller transactions; legacy XML failure reads uncommitted | Q-05, Q-06, OBS-H5, OBS-H16, OBS-L16 |
| `history.committed.uninstall`, `uninstall.dependents`, `uninstall.recurse` | success means removed | Skipped uninstall reads success and committed | OBS-L17, OBS-H8 |
| `history.fields` | source empty for load | Dependency records labeled `load` carry a source, listing shows `n/a` | Q-15, OBS-H7 |
| `history.fields` | `Package` empty only before any name is known | Refused loads whose manifest was read record no package (confirmed, D9) | B2 g3, g6, g7, R4 |
| `history.locked` | only ended actions are finalized | `$SYSTEM.Process.Terminate` finalizes an unfinished developer-mode record (B2 C2) | Q-21 |
| `history.read.filter`, `history.delete` | filters apply | First invalid column drops the rest, widening delete (confirmed, B2 D2); `find` hides query errors (E3) | Q-10, OBS-H1 |
| `history.read.filter` | explicit filters only | Default modifiers become filters | Q-18, OBS-H2 |
| `history.read.api` | `GlobalRecords` reads all namespaces | Fetch without switching namespace; `Records` ID column | OBS-H10, OBS-H11 |
| `history.retention` | deletes old records | Cutoff likely matches nothing; error blocks commands; `config delete` restores first value | OBS-H3, Q-13, OBS-H12, OBS-H4 |
| `history.keep-on-ipm-uninstall`, `uninstall.ipm` | log kept; uninstall recorded | Shared IRISTEMP global killed; success recorded before Clean | Q-16, OBS-H13, OBS-H15 |
| `settings.default-modifiers` | current namespace only | Stored in a `^%` global, possibly instance-wide | OBS-X1, P-10 |
| `settings` | `delete` restores the default | Restores the first value ever set (confirmed, B2 D5) | OBS-H4 |

## 5. Where A1 and A2 disagree

| # | Topic | A1 | A2 | Resolution |
|---|---|---|---|---|
| 1 | `PermitDowngrade`, `IgnoreInstalled`, `UpdateSnapshots` as documented flags | CAT-28 lists them as documented (class a), citing `wiki/00` L139-180 and `wiki/02` | OBS-L14 treats `PermitDowngrade` as an internal parameter the guard ignores | A3 searched the wiki, README, CHANGELOG and help. Only `NoTransaction`, `NoJournal`, `NoMapping` and `NoLock` are documented, plus `UpdateSnapshots` in an example. `PermitDowngrade` appears only in `update`'s refusal. These stay out of intent. OBS-L14 is a note on `load.refusal.downgrade`. |
| 2 | Dependency records after a parent rollback | INT-HIST-02 commits to survival; dependency case is hypothesis Q-06 | OBS-L1 concludes from code that they are lost | Not a conflict of intent. The top-level rule is `history.survives-rollback`. The dependency case is Q `history.dependencies.parent-rollback`, provisional "must survive", with A2's reading as a suspected gap for B. |
| 3 | History in the VERIFY namespace | CAT-33: records land in the VERIFY namespace | OP-LOAD-28: no record for the module itself, records only for its dependencies | A3 read `Lifecycle/Base.cls:1523-1551`. The module loads with no log, so A2 is right. Noted in `build.verify` and `history.actions`. |
| 4 | Phase commands and history | INT-HIST-01: phase commands not logged | OP-LOAD-29: `test` and `verify` log their scoped dependency loads as `load` | Consistent. Phase commands are not logged, and dependency installs are, per issue #366. Noted in `history.actions`. |
| 5 | Scope of default modifiers | CAT-27: "in the current namespace" (class a), storage unverified | P-10, OBS-X1: `^%` global may be shared by the instance | Intent stays "current namespace" in `settings.default-modifiers`. Storage scope is a suspected gap for B. |
| 6 | Default modifiers as history filters | CAT-27 notes the merge as a fact, no Q | OBS-H2 shows it breaks `history` | New Q-18 `history.read.filter.default-modifiers`, provisional "no". |
| 7 | `reinstall` version | Conflict 27, no Q | OP-LOAD-22 pins the installed version | New Q-19 `reinstall.version`, provisional "installed version". |
| 8 | `uninstall zpm` record | INT-HIST-15: finalized before Clean (neutral) | OBS-H15: success recorded even if Clean is skipped | Suspected gap noted in `uninstall.ipm` and `history.keep-on-ipm-uninstall`. |
| 9 | `http` paths | INT-LOAD-02: a relative path starting with `http` is not taken for a URL (CHANGELOG `#487`) | OP-LOAD-02: any path whose scheme contains `http` goes to the git loader | The later decision (`#706`, absolute paths only) governs. `load.refusal.path` states it, and the `#487` clause is not carried separately. |
| 10 | Module record after a plain load | Q-01 covers only developer-mode stickiness | OBS-L3: `Root` and `Repository` not saved | Noted under `dev-mode.sticky` and `deps.lockfile`. No new Q, because `load.core` already says install records its source. |

No disagreement is left unresolved. Disagreements 2, 5 and 8 depend on stage B's native results.

## 6. Questions and their owners

| Q | Question | Owner block | Page |
|---|---|---|---|
| Q-01 | Does a plain `load` keep developer mode? | `dev-mode.sticky` | concepts/transactions-and-dev-mode |
| Q-02 | Does `-dev` lift the update-required refusal? | `load.refusal.update-required.dev-bypass` | features/load |
| Q-03 | Does `-force` lift the overwrite guard, and does it cover `load`? | `dev-mode.overwrite-guard.force` | concepts/transactions-and-dev-mode |
| Q-04 | What does a rollback promise beyond the database? | `transaction.rollback-scope` | concepts/transactions-and-dev-mode |
| Q-05 | Under a caller-held transaction, does the record survive the caller's rollback, and what does `Committed` say? (dependencies settled by `history.survives-rollback` and `history.committed` in round 1) | `history.committed.nested` | features/history |
| Q-06 | What `Success` does a dependency record undone by its parent show? (survival is intent since round 1) | `history.dependencies.parent-rollback` | features/history |
| Q-07 | Does `Committed` cover non-database changes? | `history.committed.side-effects` | features/history |
| Q-08 | What does `Committed` say on an unfinished record? (provisional since round 1: the kept changes) | `history.record.abnormal-end` | features/history |
| Q-09 | Should `update`, `reinstall` and `uninstall` argument refusals be recorded? (provisional since round 1: yes) | `history.record.unlogged-refusals` | features/history |
| Q-10 | Filters on `Version_Major`, unknown columns and query errors in `find` | `history.read.filter.invalid-column` | features/history |
| Q-11 | `-confirm` with a filter | `history.delete.confirm-with-filter` | features/history |
| Q-12 (closed) | Global unfiltered delete without `-confirm` | `history.delete.global-unfiltered`, now a D block: documented in `wiki/09` L195 and `Main.cls:825`. Requiring `-confirm` stays a proposal in its metadata. | features/history |
| Q-13 | May a purge failure stop the user's command? (deleting old records is required by `history.retention`) | `history.retention.effect` | features/history |
| Q-14 | Must every started phase record its end? | `history.phases.returned-errors` | features/history |
| Q-15 | Source and label of repository-pulled dependency records (provisional since round 1: keep `load`, per `TestWithDependencies`) | `history.fields.dependency-source` | features/history |
| Q-16 | May IPM uninstall delete other namespaces' in-progress records? | `history.keep-on-ipm-uninstall.other-namespaces` | features/history |
| Q-17 | Must requirements be checked before any write? | `load.refusal.requirements.timing` | features/load |
| Q-18 (new) | Should default modifiers act as `history` filters? | `history.read.filter.default-modifiers` | features/history |
| Q-19 (new) | Does `reinstall` use the installed or the latest version? | `reinstall.version` | features/install |
| Q-20 (new, A4 R12) | May a delete or purge remove a running action's record, and what does its final write do? | `history.delete.running-records` | features/history |
| Q-21 (new, C round 1) | May IPM finalize the record of an action that never ended? | `history.locked.unfinished` | features/history |

## 7. In-scope ontology blocks

These blocks are in the formal scope. Stage B links model and native evidence to them.

- Load: `load`, `load.path`, `load.path.archive`, `load.core`, `load.options`, `load.refusal`, `load.refusal.path`, `load.refusal.manifest`, `load.refusal.downgrade`, `load.refusal.update-required`, `load.refusal.update-required.dev-bypass` (Q), `load.refusal.requirements`, `load.refusal.requirements.timing` (Q), `load.same-version` (D), `load.outcome`, `load.outcome.dev`, `load.outcome.dev.early-failure`, `load.dependencies`, `load.dependencies.local-repo`, `load.write-check`.
- Transactions and developer mode used by load: `transaction`, `transaction.rollback-scope` (Q), `transaction.opt-out`, `transaction.flag-sources`, `dev-mode`, `dev-mode.keep-on-failure`, `dev-mode.not-propagated`, `dev-mode.not-propagated.local-snapshot` (D), `dev-mode.overwrite-guard`, `dev-mode.overwrite-guard.force` (Q), `dev-mode.sticky` (Q), `repo.read-only`, `deps.installed`, `deps.lockfile`.
- History: `history`, `history.actions`, `history.record`, `history.record.unlogged-refusals` (Q), `history.record.abnormal-end` (Q), `history.survives-rollback`, `history.survives-rollback.temp-copy` (D), `history.fields`, `history.fields.dependency-source` (Q), `history.phases`, `history.phases.returned-errors` (Q), `history.committed`, `history.committed.marking` (D), `history.committed.uninstall`, `history.committed.vs-success`, `history.committed.nested` (Q), `history.committed.side-effects` (Q), `history.dependencies`, `history.dependencies.parent-rollback` (Q), `history.namespace`, `history.read`, `history.read.find`, `history.read.filter`, `history.read.filter.invalid-column` (Q), `history.read.filter.default-modifiers` (Q), `history.read.details`, `history.read.global`, `history.read.api`, `history.delete`, `history.delete.confirm-with-filter` (Q), `history.delete.global-unfiltered` (D), `history.delete.running-records` (Q), `history.locked`, `history.locked.unfinished` (Q), `history.retention`, `history.retention.effect` (Q), `history.keep-on-ipm-uninstall`, `history.keep-on-ipm-uninstall.other-namespaces` (Q).
- Shared path through other commands: `install`, `install.package`, `reinstall`, `update`, `update.refusal`, `update.dev-mode`, `uninstall` and `uninstall.ipm` (history records only), `ipm.shell` (purge before each command).

## 8. Open work for later stages

- A4 fresh-reader reconstruction is done ([reconstruction-check.md](reconstruction-check.md)). A3 adjudicated rows R1-R14 in [reconstruction-dispositions.md](reconstruction-dispositions.md) and repaired the owning blocks.
- Stage B: confirm every suspected gap in section 4 natively, and link model and native evidence to the in-scope blocks in section 7.
- Human decisions: Q-01 to Q-19 need maintainer answers. Q-02, Q-17, Q-10 and Q-06 change required outcomes most.

## 9. Native evidence status after B2 phase 1 (stage C round 1)

Runs are `20261113T<suffix>` under `.g5/Verus_verification/results/native/`. The owning blocks carry the details, and the [B2 report](../B/native-report.md) is the source. Formal (B1) evidence is linked at handoff.

| Owner block | Status | Runs |
|---|---|---|
| `load`, `load.dependencies`, `dev-mode.not-propagated` | conforms (A0, A4, B0, C0, g1); suites History, LoadModuleWithDeps, Update pass | `182759Z-cases-a`, `175623Z-smoke-history-suite-oras`, `182313Z-suite-verify-LoadModuleWithDeps`, `182433Z-suite-verify-Update` |
| `load.refusal.downgrade`, `load.refusal.update-required`, `load.same-version`, `load.outcome.dev.early-failure` | conforms (g2-g4, g6, g7) | `182832Z-cases-dg`, `184230Z-replay-fast`, `182208Z-suite-verify-InstallModule` |
| `load.refusal.update-required.dev-bypass` (Q-02), `dev-mode.sticky` (Q-01) | consistent with the provisional answer (g5, g8, g9) | `182832Z-cases-dg` |
| `load.refusal.requirements.timing` (Q-17) | confirmed defect D4 pending the decision (B2-B4); B0, B1 conform | `182820Z-cases-b`, `184630Z-repro-sysreq-dev-manifest-left` |
| `load.outcome`, `load.outcome.dev`, `transaction`, `transaction.opt-out` | conforms (A1, A2, A5, B3, B4, C1, C3); P-6 refined: SIGKILL rollback waits for slot cleanup | `182759Z-cases-a`, `182820Z-cases-b`, `182903Z-cases-c-terminate`, `182927Z-cases-c-sigkill`, `181724Z-premise-p18-sigkill-r3`, `181129Z-premises` |
| `load.path.archive` | `.tgz` covered by suite FileBinaryTar | `182337Z-suite-verify-FileBinaryTar` |
| `load.path` (`.tar.gz`), `load.refusal.path`, `load.refusal.manifest` late checks, `update.refusal` `-path` mismatch, `load.dependencies.local-repo` cleanup, legacy XML (`load.core`), `history.read.filter.default-modifiers` (Q-18) | not run; queued for B2 phase 2 | none |
| `history.survives-rollback`, `history.dependencies`, `history.dependencies.parent-rollback` | confirmed defect D1 (A1-A3) | `182759Z-cases-a`, `184617Z-repro-dep-history-lost` |
| `history.committed.nested` (Q-05) | deviates (D1 caller case); D0 conforms | `182832Z-cases-dg`, `184632Z-repro-caller-tx-history-lost` |
| `history.record`, `history.record.abnormal-end` (Q-08), `history.locked.unfinished` (Q-21), `history.survives-rollback.temp-copy` | C1, C3, C4R show no phases; C2 finalized without end; C4R `Committed=0`; temp-copy premise refuted for Terminate (N-P10) | `182903Z-cases-c-terminate`, `182927Z-cases-c-sigkill`, `183827Z-cases-c4r`, `181129Z-premises` |
| `history.fields` | deviation D9 (refused loads record no package) | `182832Z-cases-dg`, `184444Z-cases-hr` |
| `history.read.filter.invalid-column` (Q-10) | confirmed defect D2 (E1, E2, E4, E8); E3/E7 differ | `182845Z-cases-ef`, `184621Z-repro-history-filter-widened`, `184623Z-repro-history-delete-underscore` |
| `history.delete` | E5, E6 conform; E4 under D2 | `182845Z-cases-ef` |
| `history.delete.running-records` (Q-20) | observed: the record disappears and is not recreated (N-P15) | `181129Z-premises` |
| `history.retention`, `history.retention.effect`, `settings` | confirmed defects D3 (F1) and D5 (F0); purge failure not run | `184626Z-repro-autopurge-noop`, `184628Z-repro-config-delete-retain`, `182514Z-suite-test-CLI` |
| `history.phases`, `history.phases.returned-errors` (Q-14) | confirmed defect D6; reader defect D8 | `184444Z-cases-hr` |
| `history.locked` | conforms (N-P13) | `181129Z-premises` |
| `history.read.details` | conforms (R1, R2); D7 withdrawn | `184444Z-cases-hr` |
| `history.read.global`, `history.read.api`, `history.keep-on-ipm-uninstall` and its Q-16, uninstall cases | not run (self-uninstall persist step probed in N-P16; IRISTEMP sharing confirmed in N-P06, N-P17) | `181129Z-premises` |
