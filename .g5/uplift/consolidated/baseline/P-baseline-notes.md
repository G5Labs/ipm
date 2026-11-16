# P arm frozen coverage baseline: notes

Arm: P (playbook), imported at `playbook-uplift/` (fdd36690). Product: IPM `571e04a1`, `src/cls/IPM/`.
Denominator: `.g5/uplift/consolidated/coverage/members.tsv` (1,216 members). Table: `P-baseline.tsv` (118 rows, 113 distinct members).

**Qualifying count: 64 / 1,216 (5.26%).** 12 members are `ambiguous`. 37 further members are cited but do not qualify: 31 with a non-qualifying candidate row and 6 with only an exclusion row. Three qualifying members (LoadNewModule, LoadModuleReference, Main.Sync) also carry an exclusion row for a path the P citation excludes.

Nothing was run. Verus, IRIS, the explorers and git were not invoked. Every judgment rests on reading the model sources and on recorded results: `models/verus/results/*.json` (all 8 files `all_killed_in_expected_place: true`; baselines 18/28/17/8/39/27/51/39 verified, 0 errors), `evidence/proofs-*.json`, `evidence/model-checks*.json`, `evidence/iris-observations.json` (415 records; logs under `playbook-uplift/validation/logs/`) and the explorer REPORT.md files. The generator `gen_p_baseline.py` (scratchpad, not committed) checks every cited name against those files. Each model function named in `model_ref` exists. Each lemma is `verified`. Each Verus control is `killed`. Each model check exists with status `pass`. Each observation exists. The member ids come from members.tsv.

## Column conventions

- `source_slice` paths are relative to `src/cls/IPM/`, like the member ids. The slice is the cited range clipped to the member's start..end. The text in parentheses records how the citation was written.
- `model_ref` paths are relative to the repo root (`playbook-uplift/models/...`).
- `binding` lists native observations grouped by log (`playbook-uplift/validation/logs/*.log`), then any static `file:line`, then the untested premises.
- Edge types: `behavior` means rules 1–4 are met and qualifies = yes. `candidate` with ambiguous means one rule is in genuine doubt. `candidate` with no means a rule is clearly missing, and the reason column names which one. `exclusion` means the P citation itself says the path is not modelled.

## How rule 3 was applied (the deciding rule)

All 195 Verus controls are killed, so every proved lemma in the 8 files counts as a checked assertion. On its own, that is not enough for credit. A member qualifies only when a check would change its verdict if *that member's* modelled decision changed. The check can be one of four kinds:

- a Verus control that mutates the transcription of the member's own decision and is killed;
- an explorer, twin or fine-machine mutant of that decision that IRIS observations reject (`nsp-code-mutants-rejected`, `nsp-env-mutants-killed`, `repo-twin-mutants-rejected`, `inst-dep-code-mutants-rejected`, `code-mutants-rejected`);
- a proved witness or lemma clause whose distinguishing fact is that member's output;
- a conformance replay whose compared field is decided only by that member's branch.

These cases do not earn credit:

- A conformance replay that merely passes through the member.
- A forwarder (MarkFailed, %ApplyUpdateSteps, MapPackage/RoutineEquivalently, ActivateMappings, ProcessDefaultModifiers).
- A check that targets the caller's decision to call it. PerformBatchActivation, RemovePackageMapping and RemoveGlobalMapping are marked ambiguous for this reason.

Dispatchers (`%Compile`, `%Reload`, `ExecutePhases`, `EnableIPM`, `ShellInternal`) are credited only for the branch that is modelled, and the slice names that branch.

## Citation resolution

The following citation sources were scanned: the 8 `models/verus/*.rs` files and their `*.TRUST.md`, the top-level `TRUST.md`, all `models/explorer/**` files (`irisenv.py`, `l2_load.py`, `explore.py`, `inst/`, `nsp/`, `repo/`, the REPORT.md files), `semantics/COVERAGE.md`, and the `inst-dep-conformance-coverage` and `nsp-observations-not-expressed` records. `superseded/load_history.rs` is retired, so it was not used. COVERAGE.md binds invariants to commands, not members. Its only line anchors are `Main.cls#L3998` (UnmapIPM, already counted) and `FileHash.cls#L35` (an index declaration, not a member).

| Form | Instances, and how each was resolved |
|---|---|
| Full path with `#L` range | Matched directly to the overlapping member. |
| Range overrunning the member | `Storage/Module.cls#L898-L1048` gives BuildDependencyGraph, which closes on line 1047, so the range is clipped. `VersionBase.cls#L34-L60` and `#L33-L52` give GetUpdatePackageClassList (34-58). `FileHash.cls#L341-L362` gives CompareOneFile (341-361). `#L1068-L1295` gives ProcessSingleDependencyIterative (starts at 1069). `#L258-L272` gives GetAddUpdateStep (starts at 259). `ExecutePhases #L446-L720` gives a member that starts at 447. |
| Method name + relative range | `HasScope (#L834-L855)`, `SeedUpdateStep (#L203-L220)`, `GetAddUpdateStep (#L258-L272)`, `CompareOneFile #L341-L362`, `StampOneFileWithHash #L154-L170`, `HasBaseline (FileHash.cls#L501-L510)`, `AddRoutineMapping #L1690`, `AddPackageMapping Module.cls#L1631`. Each resolves through the file named in the same comment or file header. All of them land inside the named member. |
| Method name before the file range | `dep_resolve.rs` line 1: "Storage/Module.cls BuildDependencyGraph #L898-L1048 and ProcessSingleDependencyIterative #L1068-L1295". Resolved as above. |
| Multi-member ranges, split by overlap | `Storage/Module.cls#L94-L250` gives HandleAll/HandleFromList/Seed/ExecuteUpdateStep, and each was judged separately. `FileHash.cls#L420-L460` gives MarkFailed (a forwarder, no credit) and MarkPathSetFailed (the marking itself, credited). `Main.cls #L1682-L1696` and `#L1691-L1693` give Get/Set/DeleteNamespaceDefaultModifiers. `Repo/Http/Definition.cls PublishingServer #L144-L180` gives the Query declaration (one-line overlap, no logic, no credit), PublishingServerExecute (credited) and PublishingServerFetch (ambiguous). `Lifecycle/Base.cls %Clean #L305-L507` gives %Clean plus a one-line overlap with %ExportData (no credit). `Repo/UniversalSettings.cls GetValue/GetDefaultValue #L150-L164` gives two members. |
| Short filename, ambiguous | `Module.cls#L1869-L1918`, `#L1707-L1724`, `#L1736`, `#L1631`, `#L1690` in ns_mapping.rs. Line by line these hit Storage/Module.cls members (%Evaluate, %OnBeforeSave, GetStudioProject) as well as Utils/Module.cls ones. They were resolved to Utils/Module.cls by the method names in the same comment (ActivateMappings, PerformBatchActivation, RemoveRoutineMapping, AddGlobalMapping, AddPackageMapping, AddRoutineMapping). `Module.cls#L12-L15` and `#L17` in uninstall.rs would also hit Utils/Module.cls::CreateAndGetSSLConfig (10-21). They were resolved to Lifecycle/Module.cls %Clean from the header "Lifecycle/Module.cls %Clean #L8-L90". `Base.cls#...` resolves to Lifecycle/Base.cls. `Document.cls` resolves to ResourceProcessor/Default/Document.cls. `Definition.cls SortOrder #L87-L94` resolves to Repo/Definition.cls::SortOrder by method name, since five classes are named Definition.cls. |
| Call-site citations | The cited range lies in the caller, not the member. Examples: `MarkTestPending (#L158-L163)` and `SyncReportUnroutedPaths ... (Pipeline.cls#L123)` are lines in Pipeline.Run. `RemovePackageMapping(...) (#L3992)` and `RemoveGlobalMapping (#L3980)` are in UnmapIPM. `CheckSystemRequirements #L1321` and `GetDefaultParameters, #L1166` are in LoadNewModule. Such a citation fails rule 1 for the callee unless another file anchors the callee's body. ns_fine.py does this for RemovePackageMapping `#L1646-L1671` and RemoveGlobalMapping `#L1783`. |
| Name-only citations | GetListModules, SyncDispatchProcessors, SyncCompile, ConstructInvertedDependencyGraph, GetFlatDependencyListFromInvertedDependencyGraph, GetChildren, GetOrderedResourceList, ResolveChildren, SemanticVersion.Follows and SemanticVersionExpression.And. All of these are `candidate` or `exclusion`. |
| Anchors that are not members | `LockFile.cls#L24/#L25` (Property declarations), `HistoryTemp.cls#L124` (Storage block; target of the `temp-in-namespace-db` mutant), `Main.cls#L307` in inst/check.py (XData command spec), `Main.cls#L56-L57` (Help text, L1 only), `FileHash.cls#L35` (Index), `inc/IPM/Common.inc#L31`, `preload/cls/IPM/Installer.cls` (outside the denominator; nsp_lemma_installer and installer mutants are not counted). |
| Comment lines that drift | sync_baseline.rs cites `Pipeline.cls#L123`, `#L149` and `#L158-L163`. At 571e04a1 the calls are at about 126, 149 and 160. The offset stays inside Run (11-225), so only the callee attributions above are affected. |

## Ambiguous matches (12)

| Member | What would settle it |
|---|---|
| Main.cls::GetMapInfo | It aggregates GetListModules and AddIPMMappedNamespaces. The mapinfo-first-source mutant isolates the last-source-wins decision, and that decision is in AddIPMMappedNamespaces. A check on its own pIsMappedFrom construction would settle it. |
| Utils/Module.cls::AddPackageMapping | The create-only-when-absent rule (#L1631) is modelled and called natively, but no control or mutant targets it. AddRoutineMapping has one. |
| Utils/Module.cls::AddGlobalMapping | Activation-unless-skip is modelled. The slot effect is checked only through MapGlobalEquivalently. |
| Utils/Module.cls::PerformBatchActivation | `enable-no-batch-activation` and the `no-batch-activation` mutant drop the call. A no-op callee would fail the same checks, but P files the effect under environment contract E2. |
| Utils/Module.cls::RemovePackageMapping, RemoveGlobalMapping | The killed controls are phrased as UnmapIPM not removing. The callee's effect is modelled, at #L1646-L1671 and #L1783 in ns_fine.py. |
| Repo/UniversalSettings.cls::GetDefaultValue | A one-line accessor. impl-reset-to-value targets ResetToDefault's choice of node. |
| Repo/Http/Definition.cls::PublishingServerFetch | The `$order` key ordering (key_lt) is modelled and replay case c3 depends on it. No control mutates key_lt. |
| Main.cls::ShellInternal | Only the merge branch #L1066-L1070 is modelled. It is covered by a repo_dm_laws clause ("typed option wins") with no control and no native override run. |
| Storage/Module.cls::GetAddUpdateStep | New records are due (`visit`), which the DEC-INST-2 witness uses implicitly. No control or mutant isolates it. |
| Main.cls::Update | The not-installed refusal (#L4423) is in `refuses`, but L1 `blocked` restates the same clause. There is no control, no mutant and no replayed observation. |
| Utils/Module.cls::BuildAllDependencyGraphs | The drop-on-error branch (#L615-L622) is encoded only as the input shape `graph = None`, and the F-UNIN-4 witness and nc-unin-5 depend on it. Graph construction is data (U3). |

## Exclusions (cited path excluded)

- Utils/Module.cls::UninstallAll: uninstall.TRUST U9 and PNF.
- Main.cls::Reinstall: `#L2697`, listed under not expressed in inst/check.py.
- Main.cls::LoadInternal: `#L2530`, G7/PM. Its `#L2449` relative-path branch is a separate candidate, because path_ok is an input.
- Utils/Module.cls::EvaluateMacroHelper: `#L2781`, PM, known false, F-LOAD-4.
- Utils/Module.cls::LoadNewModule: `#L1353-L1354` and the G8 points. This is in addition to its behaviour edge.
- Utils/Module.cls::LoadModuleReference: `#L109` guard and `#L116-L124` version check. This is in addition to its behaviour edge.
- General/LockFile.cls::CreateLockFileForModule: lock-file writing.
- Main.cls::Sync: Verus S4. The fine-machine edge is credited separately.
- SemanticVersion.Follows and SemanticVersionExpression.And: trusted abstractions D2/D10.
- Storage/Module.cls::GetDefaultParameters: U8 assumption.

## Qualifying members (64)

| Area | Members |
|---|---|
| Namespace mapping (8) | Main.EnableIPM, Main.UnmapIPM, Main.CheckModuleNamespace, Main.AddIPMMappedNamespaces, SystemRequirements.CheckIPMVersion, Utils.Module.RemoveRoutineMapping, Utils.Module.AddRoutineMapping, Utils.Build.MapGlobalEquivalently |
| Settings, default modifiers, registries (15) | Main.Config, UniversalSettings.UpdateOne/ResetToDefault/SetValue/SetTestReportFormat/GetValue, Main.Search, Lifecycle.Base.%Publish, Repo.Http.Definition.PublishingServerExecute, Repo.Remote.Definition.PublishingServer, Repo.Oras.Definition.PublishingServer, Repo.Manager.SearchRepositoriesForModule, Main.Set/Delete/GetNamespaceDefaultModifiers |
| Developer sync (11) | Main.Sync, Sync.Pipeline.Run, FileHash.ComputeChanges/CompareOneFile/CommitChanges/MarkPathSetFailed/StampModule/StampOneFileWithHash/HasBaseline, Lifecycle.Base.%Compile (stamp branch), Lifecycle.Base.%Reload (file-load branch) |
| Scopes and packaging (5) | Lifecycle.Base.GetCompletePhasesForOne, ResourceReference.IsInScope, Default.Document.OnPhase, Lifecycle.Base.%Export, Lifecycle.Base.ExportSingleModule |
| Load, install, update steps (8) | Utils.Module.LoadNewModule, Utils.Module.LoadModuleReference, Storage.Module.HandleAllUpdateSteps/HandleUpdateStepsFromList/SeedUpdateStep/ExecuteUpdateStep, Lifecycle.Module.%Activate, Update.VersionBase.GetUpdatePackageClassList |
| Dependency resolution (4) | Storage.Module.BuildDependencyGraph, ProcessSingleDependencyIterative, HasScope; Utils.Module.GetDependentsList |
| Uninstall and clean (10) | Main.Uninstall, Storage.Module.Uninstall, Storage.Module.ExecutePhases (Clean branches), Lifecycle.Module.%Clean, Lifecycle.Base.CheckBeforeClean, Lifecycle.Base.%Clean, FileCopy.OnPhase, Default.Global.OnPhase, WebApplication.OnBeforePhase, StudioDocument.Module.Delete |
| Load history (3) | AbstractHistory.Finalize, HistoryTemp.Init, HistoryTemp.%OnClose |

## Comparison with the prior pass

The prior pass credited 13 members, and all 13 hold up on re-judgment. Two of them are among the most doubtful: Main.Sync, which rests on a fine-machine replay only and is excluded from the Verus model by S4, and GetCompletePhasesForOne, discussed below.

The other 51 were not listed before. Most have the same kind of evidence as the original 13: a killed control or an IRIS-rejected mutant on their own transcription, plus a replayed native observation. HasScope, SeedUpdateStep and ProcessSingleDependencyIterative were missed because they are cited in relative, method-name or overrunning form. A line-range scanner also drops most of the fine-machine-anchored helpers, such as RemoveRoutineMapping, MapGlobalEquivalently and MarkPathSetFailed.

## Most doubtful `yes` judgments

1. **Main.cls::Sync.** It rests only on the fine-machine replays of N20 and the N20 summary (`syncAll=ok`). There is no mutant and no proof, and sync_baseline.TRUST S4 says the branch is not checked by the Verus model.
2. **Utils/Build.cls::MapGlobalEquivalently.** The only discriminating check is an environment mutant (`global-map-honours-skip`) killed by a direct native call. P treats it as environment fact E3', and the Verus L1 cannot see it because activation is folded to the end of the command.
3. **Lifecycle/Base.cls::GetCompletePhasesForOne.** The L1 reuses `phases(c)`, so the refinement cannot discriminate the phase lists. Only `lemma_contains_facts`, which states the transcribed lists concretely, kills `verify-runs-test-phase`. There is no native replay, only static binding.
4. **Storage/FileHash.cls::CompareOneFile.** No control mutates the hash comparison. Credit rests on the F-SYNC-1 witnesses and the N18 replay, whose outcome also passes through ComputeChanges and Run.
5. **Main.cls::GetNamespaceDefaultModifiers.** The distinguishing fact of the F-REPO-5 witness and nc-repo-7 is that the storage is one shared `^%` global. That comes from the macro shared by Get, Set and Delete plus an IRIS fact, not from a decision inside this two-line method.

Also borderline:

- GetValue: its cross-namespace behaviour has static binding only.
- DeleteNamespaceDefaultModifiers and HasBaseline: no control mutates them, though there is a lemma clause or witness plus a native observation.
- AddRoutineMapping: the only-when-absent branch has static binding only.
- Repo/Oras/Definition.cls::PublishingServer: its controls are kind-generic.
- Default/Global.cls::OnPhase: witness-only.
- The five scope members: static binding only. IPM's own Scopes and IncludeTests suites pass natively but are not replayed against the model.

## Native-platform caveat

The native observations are IRIS 2026.1 build 234U on ARM64, with some repeated on 2025.3. The bindings name observation ids, not platforms. A consolidated rerun on the canonical platform could change F-RES-7-like outcomes but none of the rows above.
