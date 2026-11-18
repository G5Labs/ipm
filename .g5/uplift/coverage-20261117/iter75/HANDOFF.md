# Iteration75: targeted model repairs that restore partial obligations to full

Builder session `860dd5d9`, Opus 5.5 high, with four forked sub-agents (R345, R67, R8A, R8B). Starting commit `6240e1ba`, product pin `571e04a1` (unchanged). No inherited model under `.g5/Verus_verification/consolidated/`, `crates/` or `playbook-uplift/` was edited, and no product code or shipped test was edited. No Docker, no native runs. Verus: the pinned `0.2026.09.27.3cf1832` x86 build throughout (`verus_sha256 2fe2fca3…` in every receipt).

Scope: HANDOFF-72 §6 items 1 to 8 and the item-9 citation check, in priority order (security and data loss first).

## 1. Result

45 obligations were regraded, all from partial: 44 are now full and 1 stays partial. Every new full grade has a mutation of its deciding computation that is killed, logged in [mutations.jsonl](mutations.jsonl).

| | full | partial | absent | assessed | whole-foundation coverage | full with passing check |
|---|---|---|---|---|---|---|
| before ([before-counts.json](before-counts.json), `6240e1ba`) | 605 | 341 | 159 | 1105 | 0.5475 | 544 |
| after (`census/counts.json`) | 649 | 297 | 159 | 1105 | 0.5873 | 586 |

Design checks moved from proved 643 / bounded 19 / refuted 66 / unchecked 218 / none 159 to 647 / 19 / 67 / 213 / 159. `census.py count` prints no ERROR, WARN or STALE line (265 blocks: 258 enumerated, 7 structural). `census.py selftest` detects 9 controls. No ontology statement text changed, so no block body hash moved.

Census delta per obligation (old -> new grade/check). The full list with mutation ids is in [delta.json](delta.json).

| item | obligation | old | new |
|---|---|---|---|
| 1 | repo.oras.integrity#1 | partial/unchecked | full/proved |
| 1 | repo.oras.integrity#3 | partial/proved | full/proved |
| 1 | repo.oras.integrity#7 | partial/unchecked | partial/proved |
| 1 | repo.oras.archive-boundary#2 | partial/proved | full/proved |
| 2 | uninstall.cleanup#1 | partial/unchecked | full/refuted |
| 2 | uninstall.cleanup#2 | partial/unchecked | full/proved |
| 3 | artifact.filter#1, #2, #3, #4 | partial/proved | full/proved |
| 4 | module.csp.lifecycle#1, #3 | partial/proved | full/proved |
| 4 | devtools.generate.webapps#3, #4 | partial/proved | full/proved |
| 5 | repo.filesystem#4 | partial/proved | full/proved |
| 5 | repo.filesystem#8 | partial/refuted | full/refuted |
| 6 | module.env#4, #10 | partial/proved | full/proved |
| 7 | lifecycle.custom#4 | partial/proved | full/proved |
| 7 | lifecycle.custom#9 | partial/unchecked | full/proved |
| 7 | module.global.roundtrip#1, module.processor.cpf#2, lifecycle.source-control.editor#7, module.lifecycle-class#1 | partial/proved | full/proved |
| 8 | devtools.authoring.import#1, #3; repo.oras.tag-decode#1, #3; repo.publish-cache#1, #3; repo.oras#5; settings.report-format#1; build.test.selection#1 | partial/proved | full/proved |
| 8 | module.requirement.values#4; module.parameter.expressions#6, #8; deps#4; update.steps.records#1; load.dependencies.local-repo#6; devtools.sync#5; devtools.sync.compile-after-resolve#5; devtools.authoring.module-version#2; build.publish.partial-effects#4; artifact.local-content#1 | partial/proved | full/proved |
| 9 | load.same-version#1 | partial/proved | full/proved |

repo.oras.integrity#1 was not named in §6 item 1. It shares the file and the product method (`Pull`), and its gap (no explicit-tag input) closed with one successor function, so it was added to item 1.

Pages that moved: package-sources 39 -> 49 full, developer-tools 52 -> 59, artifact-download 12 -> 17, manifest 23 -> 28, lifecycle 37 -> 41, and +1 or +2 on load, resources, uninstall, web-applications, testing-policy, build-and-publish, dependencies, update and settings.

## 2. How each repair was made

Per obligation, the builder or fork:
1. read the deciding product lines and wrote down what the product does for the missing neighbour;
2. asserted that behaviour in a new file under [coverage-20261117/iter75/](../../../Verus_verification/coverage-20261117/iter75/), which imports the inherited model through `#[path]`;
3. re-ran the iteration-72 survivor as a named mutant of the deciding computation, with the new file as the Verus target, and added mutants of its own;
4. wrote a ledger entry (`ledger/<group>.json`, format in [ledger/FORMAT.md](ledger/FORMAT.md)).

[apply.py](apply.py) applies the ledgers to the census records. It sets basis `new:iteration-75 (...)` followed by the earlier basis, support `mutation-checked`, the mutation ids and the instance-scoped flag. It also adds one "Iteration75 neighbours" metadata line per owning block (30 blocks) and writes [bindings.json](bindings.json) (101 rows: formal declaration -> product member and lines -> role). [check.py](check.py) checks that every iteration-75 model has a 0-error receipt whose input hashes match the files now. It also checks that every mutation cited for a full grade was killed in its last run and that every model locator resolves. [summarize.py](summarize.py) prints §3.

Tools:
- [mutate.py](mutate.py) follows the iteration-72 style. It adds `--target`, so that an inherited file is mutated in a scratch copy while the iteration-75 neighbour file is verified.
- [run_verus.py](run_verus.py) writes receipts to `receipts/`. Each receipt records the command, the Verus hash, the sha256 of every input in the `#[path]` closure, the raw log, and the verified and error counts.

Successor definitions were written where the inherited definition was not faithful to the product or lacked an input the product reads:

| successor | replaces | why |
|---|---|---|
| `oras_integrity.rs::pull_effects` | iter68 `as_built::observe` | `observe` takes the size verdict as the fixture bool `size_matches`. The successor computes the status from the manifest layers and the pulled file through the inherited `verify_layers`. |
| `oras_integrity.rs::pull_tag` | iter2 `last_tag` alone | `Pull` lines 785-787 use a given tag as is and call `GetLatestTag` only for an empty tag. |
| `global_path.rs::global_path` | iter6 `workflows.rs::global_path` | The product turns an explicitly empty Directory into `gbl/` (Global.cls:106-111). The inherited model drops the segment. |
| `webapps_neighbours.rs::generated_path75`, `replace_root` | iter8 `manifest_path` | The product uses `$replace` (ModuleTemplate.cls:222), which rewrites every occurrence of the CSP root. The inherited model tests a prefix. |
| `lifecycle_neighbours.rs` token model | iter30 `phases.rs` single CUSTOM token | Two distinct custom phases could not be written. |
| `env_loader.rs` | `env_config.rs` readability input | File presence and parse success are derived from a filesystem map (EnvironmentConfig.cls:12-27). |
| `lifecycle_neighbours.rs::set_primary` | iter67 `primary_after_set` (agrees) | The primary is decided from the class dictionary (CompositeMethodOverrides.cls:81-103). |
| `r8b_records.rs::on_new75`, `build_cache` | the written-down failure branch of the temporary repository creation (inherited `verus/temp_repo.rs::cleanup` kept) | The missing repository row is derived from the BuildCache TSTART/TROLLBACK position. |
| `r8b_remote.rs::remote_unpublish` | iter2 `remote_delete(authorized, …)` | Authorization is computed from the credential fields (PublishService.cls:56-74). |
| `r9_same_version.rs::reload75` | iter18 `reloading.rs` constant `skip_equal_load` | The product guard (Utils/Module.cls:1211-1222) over `semver.rs::follows`. |

## 3. Per repair: product lines, neighbour, mutations

Generated by `summarize.py`. "Instance-scoped" marks a grade resting on concrete instances that cover every case the obligation states.


### R12: HANDOFF-72 §6 items 1 and 2

- **repo.oras.integrity#3**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:1014-1021 (skip non-tar mediaType; first tar layer returns manifest_size == size and manifest_hash == expected_hash; no tar layer returns False)
  - Neighbour: size_and_digest_each_decide: size-only and digest-only mismatches of the first tar layer are refused, a non-tar layer carrying the pulled size and digest does not stand in, a later matching tar layer does not rescue either single mismatch
  - Mutations: R1-m01 killed, R1-m02 killed, R1-m03 killed, R1-m04 killed
- **repo.oras.integrity#7**: partial/unchecked -> partial/proved.
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:1014-1021
  - Neighbour: size_and_digest_each_decide (retaining both checks is now pinned: dropping either comparison fails)
  - Mutations: R1-m01 killed, R1-m02 killed
  - Gap: "Expose the tag-order and file/layer mismatches" is a documentation policy with no formal observation, and the alternatives in the bullet stay undecided (open question repo.oras.integrity.policy). Retaining both checks is pinned (R1-m01/m02 killed).
- **repo.oras.integrity#1**: partial/unchecked -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:785-787 (empty tag replaced by GetLatestTag), 920-923 (tags[-1] if tags else "")
  - Neighbour: tag_selection_neighbours: no tag picks the last listed tag ([2,1] -> 1, [1,2] -> 2, [] -> none); an explicit tag is used as given whatever the provider lists
  - Mutations: R1-m09 killed, R1-m10 killed, R1-m11 killed
- **repo.oras.archive-boundary#2**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:826-829 (Pull throws when not verified), 239-246 (GetModule throws the failed status before returning the stream); src/cls/IPM/Utils/Module.cls:142-172 (only a returned stream reaches LoadModuleFromArchive)
  - Neighbour: failed_check_keeps_installed_root: on a writable root a matching archive reaches the loader and replaces the root; a size-only or digest-only mismatch releases nothing, never enters the loader and leaves the root; refused_archive_never_replaces_root states it for every manifest
  - Mutations: R1-m01 killed, R1-m02 killed, R1-m05 killed, R1-m06 killed, R1-m07 killed, R1-m08 killed
- **uninstall.cleanup#1**: partial/unchecked -> full/refuted (instance-scoped).
  - Product: src/cls/IPM/Lifecycle/Base.cls:314 (tRemoveMapped = Clean.GlobalScope), 375-378 and 414-417 (IsMapped items skipped with a WARNING unless tRemoveMapped); src/cls/IPM/Main.cls:500 (-global sets Clean.GlobalScope)
  - Neighbour: mapped_code_needs_global: uninstall without -global keeps a mapped class and succeeds; with -global it is deleted; unmapped code is deleted either way
  - Mutations: R2-m01 killed, R2-m02 killed, R2-m05 killed, R2-m06 killed
  - Refutation: The block says uninstall deletes the module's classes, packages, includes and routines; as built, mapped items are skipped with a WARNING unless -global is given (Lifecycle/Base.cls:375-378, 414-417).
  - Ontology repair: uninstall.cleanup P1: state that mapped code is skipped unless -global (HANDOFF-72 §5 item 10, first omission).
- **uninstall.cleanup#2**: partial/unchecked -> full/proved (instance-scoped).
  - Product: src/cls/IPM/ResourceProcessor/Default/Global.cls:76-80 (Clean: kill @..Global only when Global is non-empty and Preserve is off)
  - Neighbour: preserve_keeps_global_data: a Global-attribute resource with Preserve keeps its data with and without -global; without Preserve it is killed; unin_witness_global_kept (inherited) keeps a name-only global
  - Mutations: R2-m03 killed, R2-m04 killed
  - Ontology repair: uninstall.cleanup P1: state that Global data is kept when Preserve is set (HANDOFF-72 §5 item 10, second omission).
- Not done: repo.oras.archive-boundary#4: outside item 1: needs declared_blob derived from the pull manifest versus the metadata manifest (a larger successor of iter68 changed_tag)

### R345: HANDOFF-72 section 6 items 3, 4 and 5

- **artifact.filter#1**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls:344-346 refuses only when criteria.name is "" and criteria.properties is empty; :338-339 stores each @key as a property
  - Neighbour: property_alone_is_enough: [@p] and [repo,@p] and [name] parse; [repo] and [] are refused
  - Mutations: R3-m01 killed, R3-m06 killed
- **artifact.filter#2**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls:341-342 throws on any other key while iterating, so a valid name before or after it does not help; :362-364 throws for a condition that is an object but not a %DynamicObject (an array)
  - Neighbour: unsupported_key_or_operator_fails: [name, other] and [other, name] refused; name or @property with an array condition refused; [name, @p scalar] accepted
  - Mutations: R3-m02 killed, R3-m03 killed
- **artifact.filter#3**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls:327-342 accepts repo, name, path, type and @-prefixed keys; :334-337 refuses type unless ParseCondition gives "file" (plain or $eq) and adds no criterion
  - Neighbour: allowed_keys_and_type_file: all five keys together parse; type=file plain and via $eq accepted with criteria equal to the name alone; other type refused
  - Mutations: R3-m04 killed
- **artifact.filter#4**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls:358-375 returns a plain value; requires a %DynamicObject of size 1 (arrays and empty or multi-operator objects throw); throws for an object operand; $match only with allowMatch, which :331-332 sets for name and path only
  - Neighbour: condition_shapes: scalar and single $eq accepted for any key, $match for name/path only (path accepted, @property and type refused), $other, empty, two-operator, object-operand and array refused
  - Mutations: R3-m02 killed, R3-m05 killed
- **module.csp.lifecycle#1**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/CSPApplication.cls:147-152 calls CreateOrUpdateCSPApp before Configure/Activate, which at :335-373 modifies an existing application and creates a missing one; :153-156 deletes it before Unconfigure/Clean
  - Neighbour: load_creates_and_uninstall_deletes: from no application a load creates one (any DeployPath), over an existing one a load keeps one, and uninstall leaves none
  - Mutations: R4-m01 killed, R4-m03 killed
- **module.csp.lifecycle#3**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/CSPApplication.cls:106-108 defaults an empty Directory (no DeployPath; StudioDocument/Module.cls:549 renames DeployPath to Directory) to the resource folder, and :137-140 turns copying off when Directory is that folder; with a DeployPath :159-163 and :196-197 copy the source folder after Reload
  - Neighbour: no_deploy_path_no_copy: without DeployPath the deploy files are unchanged, also after a source edit; with DeployPath each load copies the current source
  - Mutations: R4-m02 killed
- **devtools.generate.webapps#3**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/ModuleTemplate.cls:203-205 lists IsNameSpaceDefault, Name, NameSpace and Path as skipped; :229 continues past them in the property loop, while :230-232 writes other properties that differ from Security.Applications defaults
  - Neighbour: namespace_and_default_marker_left_out: provider NamespaceDefault=1 and Namespace=99 (both non-default) are absent; a differing Description is written
  - Mutations: R4-m04 killed, R4-m05 killed
- **devtools.generate.webapps#4**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/ModuleTemplate.cls:220-223 writes Path only when DispatchClass is empty and Path is not, as $replace(props("Path"), cspDir, "{$cspdir}"); $replace substitutes every occurrence of the CSP root, so a path outside the root is written as is, a path under it becomes {$cspdir}..., and a path containing the root after its start gets {$cspdir} in the middle
  - Neighbour: path_neighbours: under root [1,2,3] -> {$cspdir}3; outside [7,8] -> literal; dispatch -> none; no or empty path -> none; as built [7,1,2,3] -> 7{$cspdir}3
  - Mutations: R4-m06b killed, R4-m07b killed, R4-m08b killed
  - Finding: Low, candidate: AddWebApps uses $replace rather than a prefix test, so an app path such as /backup/usr/irissys/csp/app/ is declared as /backup{$cspdir}app/ and resolves under another instance's install directory on install. Source-only; not natively reproduced.
  - Ontology repair: devtools.generate.webapps B1: as built the declaration replaces every occurrence of the CSP root in the old path ($replace, ModuleTemplate.cls:222), not only a leading one; a path that contains the root after its start gets {$cspdir} in the middle.
- **repo.filesystem#4**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Repo/Filesystem/Definition.cls:170-175 aggregates every AddCacheItem status and throws after the loop; AddCacheItem :225-227 returns the failed %Save of a row whose Name fails %IPM.DataType.ModuleName validation, while :196-200 skips an unloadable file; :179 rolls back the purge and saves; ScanDirectory :329 skips manifests deeper than a nonzero Depth
  - Neighbour: invalid_name_fails_whole_refresh: [valid, invalid-name at depth 2] with Depth 0 -> !ok and old rows kept; Depth 1 -> ok and cache = {valid}; [valid, unreadable] -> ok
  - Mutations: R5-m01 killed, R5-m02 killed, R5-m03 killed, R5-m04 killed
- **repo.filesystem#8**: partial/refuted -> full/refuted.
  - Product: src/cls/IPM/Repo/Filesystem/Definition.cls:170-179 as above: the refresh fails as a whole and rolls back, so a valid module beside an invalid-name one is not offered
  - Neighbour: intended_skip_refuted: a new repository with [valid, invalid-name] fails, is not created and does not hold the valid key, while intended_add would skip the invalid entry and cache the valid one
  - Mutations: R5-m01 killed, R5-m02 killed

### R67: HANDOFF-72 §6 items 6 and 7

- **module.env#4**: partial/proved -> full/proved.
  - Product: General/EnvironmentConfig.cls:12-14 throws FileDoesNotExist when %File.Exists(path) is false; 15-27 a present JSON object is linked, parsed, merged and evaluated; Lifecycle/Base.cls:2188-2200 clears then loads each path. For a missing path the command fails; the same path holding an object loads its values.
  - Neighbour: missing_versus_readable: path 3 absent -> !ok and empty config; path 1 holding {x:1} -> ok with x=1; path 3 after inserting {x:1} -> ok (presence is the deciding state)
  - Mutations: R6-m01 killed, R6-m02 killed
- **module.env#10**: partial/proved -> full/proved.
  - Product: General/EnvironmentConfig.cls:12-25 a missing file, an unparsable file or an array root throws; 28-31 any throw clears ..Config and rethrows; Base.cls:2193 clears before the first file. Two object files merge key by key.
  - Neighbour: later_files: [1,2] -> ok with x=1 and y=2; [1,3 missing], [1,5 not JSON] and [1,4 array root] -> !ok and empty config; path 4 is present and parses, so the array-root failure is decided by content, not by the read flag
  - Mutations: R6-m01 killed, R6-m02 killed, R6-m03 killed, R6-m04 killed
- **lifecycle.custom#4**: partial/proved -> full/proved.
  - Product: Storage/Module.cls ExecutePhases: with -only the list is $listtostring-joined and matched as one name by Lifecycle/Base.cls::MatchSinglePhase (192-214); a lone custom name is returned as-is and found by GetCustomPhases (413-443), so it runs; "Mycustom,Compile" is no phase and fails. Without -only, GetCompletePhases (139-162) returns a lone non-standard name as-is and drops it from a multi-name union.
  - Neighbour: custom_alone_or_beside: [CUSTOM] with and without -only -> ok, one custom hook; [CUSTOM,Compile] -only -> !ok, no custom or compile hook; [Compile,CUSTOM] complete -> ok, no custom hook
  - Mutations: R7-m05 killed, R7-m06 killed
- **lifecycle.custom#9**: partial/unchecked -> full/proved.
  - Product: Lifecycle/Base.cls::GetCompletePhases (139-162): with two names the single-name branch is skipped and GetCompletePhasesForOne (165-189) returns "" for each custom name, so the phase list is empty; ExecutePhases runs nothing and returns $$$OK. With -only the joined name is no phase and fails.
  - Neighbour: two_custom_names: declared customs {100,102}; [100,102] complete -> ok, no custom or standard phase runs; [100,102] -only -> !ok, none runs; [100] -> runs 100; [102] -only -> runs 102
  - Mutations: R7-m07 killed, R7-m08 killed
  - Ontology repair: lifecycle.custom B1 (HANDOFF-72 §5 item 8): state that two custom names together run nothing and the command succeeds (with -only it fails).
- **module.global.roundtrip#1**: partial/proved -> full/proved.
  - Product: ResourceProcessor/Default/Global.cls:98-115 OnItemRelativePath: SourcesRoot (with "/" added), then Directory with "" replaced by gbl/ (106-111; InitialExpression gbl/ at 18 for an absent attribute), then the translated name and .xml. Export (OnResolveChildren 89-94) and Reload (59-72) both use it.
  - Neighbour: path_neighbours/round_trip_neighbours: absent Directory -> src/gbl/name; declared [3] or [3,4] -> src/3[/4]/name; explicitly empty -> src/gbl/name; no SourcesRoot -> gbl/name; export with Directory 3 is read back with Directory 3 and not with the default
  - Mutations: R7-m01 killed, R7-m02 killed, R7-m03 killed, R7-m04 killed
  - Ontology repair: module.global.roundtrip P1 could say that an empty Directory also gives gbl/ (clarification, not a contradiction).
- **module.processor.cpf#2**: partial/proved -> full/proved.
  - Product: ResourceProcessor/CPF.cls:24-32 OnBeforePhase merges only when When="Before", Phase matches and no CustomPhase; 34-42 OnAfterPhase only when When="After".
  - Neighbour: before_and_after: Before merges before its phase and not after; After the reverse; neither merges at another phase
  - Mutations: R7-m09 killed
- **lifecycle.source-control.editor#7**: partial/proved -> full/proved.
  - Product: ExtensionBase/CompositeMethodOverrides.cls:81-103 SetPrimaryExtensionClass: a name with no class definition (including "") quits with $$$OK and leaves the primary; a class not extending %Studio.Extension.Base returns an error and leaves it; otherwise the primary global is set.
  - Neighbour: primary_neighbours: empty name -> (old, ok); extension class name -> (new, ok); non-extension class -> (old, error); inherited primary_after_set(1,2,true)==2
  - Mutations: R7-m10 killed, R7-m11 killed
- **module.lifecycle-class#1**: partial/proved -> full/proved.
  - Product: Storage/Module.cls:1428-1449 LifecycleGet: the first compiled class among OverrideLifecycleClass and LifecycleClass (short names under %IPM.Lifecycle); with neither, LifecycleClass becomes "Module" and %IPM.Lifecycle.Module is used.
  - Neighbour: lifecycle_class_neighbours: resolve(None, compiled or not) == Module; resolve(Some(n), true) == Named(n); resolve(Some(n), false) == Module
  - Mutations: R7-m12 killed

### R8A: HANDOFF-72 §6 item 8 (first half)

- **devtools.authoring.import#1**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Main.cls:3334 (tRecurse = not -norecurse), 3352-3358 (an existing directory goes to $system.OBJ.ImportDir with tRecurse); recursion itself is ImportDir behaviour, observed by iteration-49 native case i2 (sub/ items compiled without -norecurse)
  - Neighbour: recursive_import_reaches_subdirectory: a sub/ item is defined and compiled without -norecurse, not defined with it, defined but not compiled with -nocompile
  - Mutations: R8a-m01 killed
- **devtools.authoring.import#3**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Main.cls:3350-3374: neither file nor directory exists, so tSC = FileDoesNotExist and $$$ThrowOnError before any ImportDir/Load or UserAction call
  - Neighbour: missing_path_defines_nothing: for every item and flag combination a missing path defines and compiles nothing, fails, makes no source-control call; an existing file defines the item
  - Mutations: R8a-m02 killed
- **repo.oras.tag-decode#1**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:270-280 TagForReference (tag = $$$Semver2Tag(version), src/inc/IPM/Common.inc:82 replaces + with _; deployed appends "__" and the platform; a source reference ignores PlatformVersion)
  - Neighbour: source_tag_is_encoded_version: a source tag of 1.2.3-alpha+build.7 is the encoded version (not the raw one), a deployed tag is encoded + __platform, a source reference carrying a platform still gets the bare version
  - Mutations: R8a-m03 killed
- **repo.oras.tag-decode#3**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:273-276: only a Deployed reference with an empty PlatformVersion throws; a source reference is tagged without a platform
  - Neighbour: only_deployed_reference_needs_platform: deployed without platform is refused in both models, deployed with platform and a source package without platform are tagged
  - Mutations: R8a-m04 killed, R8a-m05 killed
- **repo.publish-cache#1**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Remote/PublishManager.cls:12-43 CheckServiceCache: a client in ServiceCache is returned with pAvailable = 1 (lines 38-41) without reading Enabled or calling IsAvailable; only an uncached enabled repository is probed (lines 26-36)
  - Neighbour: retained_remote_client_is_not_reprobed: a retained client stays available with the repository disabled and the provider unavailable, and the provider is not called; a fresh enabled manager calls the provider and its answer decides
  - Mutations: R8a-m06 killed, R8a-m08 killed
- **repo.publish-cache#3**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PublishManager.cls:4-20 CheckServiceCache ("TODO actually implement cache"): Enabled is read and IsAvailable called on every call, pAvailable is its answer
  - Neighbour: oras_manager_probes_each_call: enabled with an unavailable probe is unavailable, enabled with an available probe is available, disabled is unavailable; the Remote manager stays available on the same probe sequence
  - Mutations: R8a-m07 killed
- **repo.oras#5**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Oras/PackageService.cls:93-109 IsAvailable: transport error 0; 200-299 1; 401 1 only with a WWW-Authenticate header; otherwise 0 (CheckNoPath throws for a URL with a path, outside this obligation)
  - Neighbour: oras_available_neighbors (exec): 200 and 299 available; 401 with header available; 401 without header, 403 with header, 300 with header and a transport failure unavailable
  - Mutations: R8a-m09 killed, R8a-m10 killed, R8a-m11 killed
- **settings.report-format#1**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/UniversalSettings.cls:225-233 SetTestReportFormat: a non-empty value whose $zconvert(val,"l") is not in Test/Abstract.cls:7 VALIDFORMATS "json,yaml,toon" is refused; otherwise SetValue stores val as supplied
  - Neighbour: yaml_and_toon_accepted: "YAML", "yaml" and "ToOn" accepted and stored as supplied, "yam" refused; config_set_stores_yaml_and_toon: the playbook config set path stores yaml and toon and keeps an unknown value out
  - Mutations: R8a-m12 killed, R8a-m13 killed, R8a-m14 killed
- **build.test.selection#1**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/ResourceProcessor/Test.cls:615-621 %OnValidateObject: neither or both of Package and Class gives "EXACTLY ONE ..." error
  - Neighbour: package_only_test_target_valid: Package only and Class only valid; both and neither refused
  - Mutations: R8a-m15 killed

### R8B: HANDOFF-72 §6 item 8 (second half) and item 9

- **module.requirement.values#4**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/SystemRequirements.cls:61-70 CheckInteroperability: required = (Interoperability="enabled") || Health; fails only when IsInteroperabilityEnabled() is false. Health=true with Interoperability="disabled" passes on an interoperability-enabled namespace.
  - Neighbour: health_interop_neighbors: Health=On, Interoperability=disabled gives [Interoperability] on a namespace without interoperability and no failure on an enabled one (Health instance); without Health, disabled never fails
  - Mutations: R8b-m01 killed
- **module.parameter.expressions#6**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Utils/Module.cls:2746-2785 %EvaluateMacro/EvaluateMacroHelper: a generated class returns $$$name under #IfDef and "" under #Else.
  - Neighbour: defined_macro_gives_value (general) and macro_neighbors: a defined macro gives its definition, an undefined one gives ""
  - Mutations: R8b-m02 killed
- **module.parameter.expressions#8**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/Module.cls:1885-1894 %EvaluateExpression: $zstrip leading whitespace, return @expr, "" when it throws.
  - Neighbour: expression_success_gives_value (general) and expression_neighbors: " 1+2" with an interpreter giving 3 evaluates to 3; a failing expression gives ""
  - Mutations: R8b-m03 killed
- **deps#4**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/Module.cls:834-855 HasScope (empty scope counts; otherwise the scope must be in pPhases, case-insensitive) used at :955 and :1256 to skip a requirement.
  - Neighbour: scoped_requirement_neighbors: the same manifest classifies the test-scoped requirement under {..., test} and not under the load phases; the unscoped one in both
  - Mutations: R8b-m04 killed
- **update.steps.records#1**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Storage/Module.cls:259-272 GetAddUpdateStep opens the row by the unique (ClassName, MethodName) index (UpdateStep/AnyMember.cls:8, 18-30) or creates it, sets HomeModule and saves; TimeStampEnd and Status are untouched.
  - Neighbour: get_add_keeps_completion (general): registration moves home and keeps completed and ok; registration_neighbors: done, due and failed rows for module 1 keep their state under module 2
  - Mutations: R8b-m05 killed, R8b-m06 killed
- **load.dependencies.local-repo#6**: partial/proved -> full/proved.
  - Product: src/cls/IPM/General/TempLocalRepoManager.cls:49-86 (%OnNew calls CleanUp on a Create failure; CleanUp warns when %DeleteId fails); Repo/Filesystem/Definition.cls:117-180 BuildCache saves the definition inside its own TSTART and rolls back to the entry level on failure, so the object keeps its id.
  - Neighbour: temp_repo_neighbors: failed creation leaves no row, keeps the id and warns; successful creation has the row and its cleanup removes it silently; skipped creation is silent
  - Mutations: R8b-m07 killed, R8b-m08 killed
- **devtools.sync#5**: partial/proved -> full/proved.
  - Product: src/cls/IPM/General/Sync/Pipeline.cls:140-149 (SyncApplyDeletes then CommitChanges); Storage/FileHash.cls:394-405 CommitChanges deletes the baseline row of every routed deleted path when processDeletes; SyncApplyDeletes (:535-578) deletes the server document only for compilable resources.
  - Neighbour: delete_flag_removes_baseline / no_flag_keeps_baseline (general) and baseline_neighbors: with -delete both a compilable and a non-compilable deleted file lose their baseline (only the compilable one loses its document); without -delete both keep it
  - Mutations: R8b-m09 killed, R8b-m10 killed
  - Ontology repair: devtools.sync B (the -delete bullet): as built, -delete also drops the baseline row of a deleted file routed to a non-compilable resource, whose server item stays; "(and their baselines)" reads as only the removed items' baselines.
- **devtools.sync.compile-after-resolve#5**: partial/proved -> full/proved.
  - Product: src/cls/IPM/General/Sync/Pipeline.cls:690-749 SyncCompile skips a resource with Processor.CompileFromProject (:710-712) before ResolveChildren, so it neither joins the compile set nor warns; undefined .CLS children are skipped (:731-736).
  - Neighbour: project_resource_neighbors: a CompileFromProject resource whose children resolve is not compiled and not warned about; the same resource without CompileFromProject is compiled
  - Mutations: R8b-m11 killed
- **devtools.authoring.module-version#2**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Main.cls:4031-4049 bump/parse, then WithPrerelease and WithBuild (General/SemanticVersion.cls:245-265; each keeps the other part; WithBump* clear both, :212-243).
  - Neighbour: pre_and_build_applied: patch -prerelease rc gives 1.2.4-rc, patch -build b1 gives 1.2.4+b1, minor with both gives 1.3.0-rc+b1, no flags gives 1.2.4 (with -force, so prerelease order is not involved)
  - Mutations: R8b-m12 killed, R8b-m13 killed, R8b-m14 killed
- **build.publish.partial-effects#4**: partial/proved -> full/proved (instance-scoped).
  - Product: src/cls/IPM/Repo/Remote/PublishService.cls:14-35 Publish and :56-74 Unpublish: ((Username="")||(Password=""))&&(Token="") throws before the request; success only on status 200.
  - Neighbour: unpublish_credential_neighbors: delete succeeds with user+password or token at 200; fails with user only, password only, nothing, or status 204; publish uses the same guard
  - Mutations: R8b-m15 killed, R8b-m16 killed
- **artifact.local-content#1**: partial/proved -> full/proved.
  - Product: src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls:32-43 OnAfterPhase(Activate): DownloadArtifact only when <Root>/<resource name>/ does not exist; the copy runs when the folder exists afterwards (:47-63).
  - Neighbour: local_folder_neighbors: an absent folder requests a download (and copies after a completed one); a present folder requests none and is copied; nothing outside Activate
  - Mutations: R8b-m17 killed, R8b-m18 killed
- **load.same-version#1**: partial/proved -> full/proved.
  - Product: src/cls/IPM/Utils/Module.cls:1211-1222 LoadNewModule: refuse when fromVersion.Follows(toVersion); refuse install/load without -force/-dev when toVersion.Follows(fromVersion) and UpdatePackage is set; no equal-version branch, so the loader proceeds.
  - Neighbour: same_version_reload_neighbor: install 1.0.0 from alpha, then 1.0.0 from beta succeeds with payload 2 and supplier beta; guard_neighbors: equal passes under both SemVerPostRelease values with an update package, first install passes, 2.0.0->1.0.0 refused; downgrade_refused: a refused attempt leaves the world unchanged
  - Mutations: R9-m01 killed, R9-m02 killed, R9-m03 killed, R9-m04 killed


## 4. Mutations

There were 94 runs over 91 ids, all logged in [mutations.jsonl](mutations.jsonl) with the model and target hashes, the diff, the obligations each bears on and the failing locations. In the last run per id, 89 were killed and 2 survived. Scratch copies are under `/tmp/ipmcov75-mut/` and are not committed. Each diff in the log is enough to reproduce its run.

- **Survivors R4-m06 and R4-m07.** These ran against the first draft of `webapps_neighbours.rs`, whose neighbours asserted `replace_root` rather than `generated_path75`. The neighbours were strengthened, and the same changes re-run as R4-m06b and R4-m07b are killed. The survivors stay in the log and are not cited by any grade.
- **Re-runs of R8b-m01 to m03.** They first ran before a comment-only edit to `r8b_manifest.rs` and were re-run against the final file, where they are killed again. The last run is the one cited.
- **Kills that come only from an inherited lemma in the target's closure.**
  - R2-m04 (name-only global data killed) fails the playbook `unin_witness_global_kept`, which uninstall.cleanup#2 cites.
  - R6-m03 (array root accepted) fails the inherited `any_file_error_clears`. Without the array check the product also fails, through the MergeDynamicObjects mismatch, so for module.env#10 this mutant is close to equivalent.
- **R2-m01** (mapped code deleted without -global) is killed at an intermediate assertion of `mapped_code_needs_global`. Under the mutant, the lemma's ensures clause is false as well.
- **Re-runs of iteration-72 survivors.** Every survivor named in HANDOFF-72 §6 for these items was re-run as a named mutant and is now killed: G4-m06/m08/m09/m10, G4-m32/m37/m38/m39, G4-m67/m68, G4-m73/m77, G4-m86, G4-m115/m116/m117, G4-m128, G7-m18/m30, G7-m76/m77, G5-m01/m02/m10/m11/m14/m16/m17/m43, G6-m15/m58/m73/m77/m78/m94, G2-m07/m38, G3-m12/m25/m43/m73, M-m01/m08/m09/m12/m13.

## 5. Refutations and findings

- **uninstall.cleanup#1: full, refuted.** The block says uninstall deletes the module's classes, packages, includes and routines. As built, `%Clean` skips every item that `%Library.RoutineMgr.IsMapped` reports, with a WARNING, unless `Clean.GlobalScope` is set (Lifecycle/Base.cls:314, 375-378, 414-417). Only `-global` sets it (Main.cls:500). The as-built claim in the recorded cases is proved. The unqualified wording is refuted. This is a deliberate guard, because mapped code belongs to another database, so it is not a defect.
- **repo.filesystem#8: full, refuted (was partial, refuted).** The intended skip of an invalid-name module is now refuted by a neighbour. As built, `build_cache([valid, invalid-name])` fails, rolls back and does not offer the valid module (Repo/Filesystem/Definition.cls:170-179). The policy stays in the open-questions register (repo.filesystem.bad-module).
- **No new C:D finding was filed.** One unconfirmed candidate is kept here rather than in `bugs_issues.md`, because it is model-only (`Reproduced by: none`). The setup_foundation findings rule keeps unconfirmed allegations outside the active defect list.
  - Candidate, Low. `ModuleTemplate.cls::AddWebApps` writes `$replace(props("Path"), cspDir, "{$cspdir}")` (line 222). It replaces every occurrence of the instance's CSP root, not a leading one. An application path such as `/backup/usr/irissys/csp/app/` is declared as `/backup{$cspdir}app/`, and on another instance that resolves under that instance's install directory. A path under the root, a path not containing the root and a dispatch application behave as documented.
  - The candidate is asserted as built by `webapps_neighbours.rs::path_neighbours`. To confirm it, run `generate` natively on an application whose Path contains the CSP root after its start. Then file it as the next free C:D id, which is C:D74 at `6240e1ba`, after the snapshot and the three format checks.
- `bugs_issues.md` was not edited, so the findings snapshot and format checks were not needed. `check_ontology.py` was run after the metadata edits (§10).

## 6. Ontology text repairs exposed (not applied)

1. **uninstall.cleanup P1.** Mapped code is skipped unless `-global` (the refutation of #1), and Global data is kept when Preserve is set (#2 omission). This is HANDOFF-72 §5 item 10, first and second omissions.
2. **devtools.generate.webapps B1.** The old path's CSP root is replaced wherever it occurs (`$replace`), not only as a prefix. See the candidate in §5.
3. **lifecycle.custom B1** (HANDOFF-72 §5 item 8). Two custom names together run nothing and the command succeeds. With `-only` the command fails.
4. **module.global.roundtrip P1.** An explicitly empty Directory also gives `gbl/`. This is a clarification, not a contradiction.
5. **devtools.sync, the -delete bullet.** As built, `-delete` also drops the baseline row of a deleted file routed to a non-compilable resource, whose server item stays. "(and their baselines)" reads as only the removed items' baselines.

Still open from HANDOFF-72 §5 and not touched here: items 1-7, 9, 11-19.

## 7. Premises and native coverage

No native run was made. Where a full grade rests on a provider or IRIS behaviour, the basis names the premise and the current receipt that covers it:

| obligation | premise | covered by |
|---|---|---|
| repo.oras.archive-boundary#2 | Matching archive replaces a writable root; a size refusal keeps it | iteration-68 native receipt (`consolidated/iter68/native/observations.json`). The digest-only case is not observed natively. It is decided by the same product line (PackageService.cls:1020), and the bundled oras 0.1.30 `download_blob` (provider.py:446-474) writes the blob without checking its digest, so a digest-only mismatch reaches IPM's check. The size comparison as integers is covered by the observed valid replacement. |
| repo.oras.integrity#1 | Provider tag order is an input | iteration-2 native raw.log (`TAG-g5iter2order value=1.0.0`, `TAG-g5iter2sorted value=2.0.0`) |
| uninstall.cleanup#1 | `IsMapped` is an environment input | No native receipt observes the skip. The grade is model and source only for the mapped case. |
| module.csp.lifecycle#3 | Copy into a DeployPath that does not exist yet | iteration-57 native case a1. The source path that creates the directory was not identified. |
| lifecycle.custom#4 | Unknown lifecycle method name raises an error | inherited phases.rs premise, native case c10 |
| lifecycle.custom#9 | none (empty phase list is product code) | No native case runs two custom names. |
| lifecycle.source-control.editor#7 | `%ExistsId("")` is false | source reading |
| devtools.authoring.import#1, #3 | ImportDir recursion; missing-path failure | iteration-49 native cases i2, i6 |
| module.parameter.expressions#6, #8 | Preprocessor and interpreter values are inputs | native evaluator receipt 20261114T025636Z (E10/E11/E13/E14/E16/E17) |
| load.dependencies.local-repo#6 | `%DeleteId` of a missing row errors; the in-memory id survives rollback | C:D2 native receipt (TR7, TR2/TR7b) |
| repo.filesystem#4, #8 | Failed refresh creates nothing | consolidated round 1 native TR7/TR7b |
| load.same-version#1 | From and to are the parsed installed and new versions; success path | iteration-18 native probes |

## 8. Items not done and residual gaps

- **repo.oras.integrity#7 stays partial.** "Retain size and digest checking" is now mutation-checked, but "expose the tag-order and file/layer mismatches" is a documentation policy with no formal observation. The alternatives stay undecided (open question repo.oras.integrity.policy).
- **repo.oras.archive-boundary#4** is not in §6 item 1 and was not attempted. It needs `declared_blob` derived from the pull manifest versus the metadata manifest.
- **Residual narrowings recorded by the forks, none of which changes a stated case:**
  - deps#4 represents a scope list as one token.
  - devtools.sync#5 does not represent a SyncCompile failure before the baseline commit.
  - `remote_delete_enabled` still takes the CheckUnpublishEnabled credential guard as an input. build.publish.partial-effects#4 is carried by the new `remote_unpublish`.
  - module.csp.lifecycle#1 does not separate Activate from the rest of load.
- **HANDOFF-72 §6 item 10** (larger, high consequence) is out of scope. The transaction-scope writer model is assignment iteration-76.

## 9. Limitations

- **Single author per group.** Each fork's grades were judged by that fork. I re-read the deciding product lines for `global_path.rs`, the custom-phase successor in `lifecycle_neighbours.rs`, `r9_same_version.rs::reload75` and the `$replace` claim, and all agreed with the product. The other files were checked only by `check.py`: receipts are current, cited mutants were killed and locators resolve.
- **Instance-scoped grades.** 14 of the 44 new fulls rest on concrete instances that cover the stated cases, and their records carry `instance_scoped`. The other 30 were not marked instance-scoped by their author. I checked that claim only for my own six and for the general lemmas `refused_archive_never_replaces_root`, `get_add_keeps_completion`, `downgrade_refused` and the import#3 missing-path lemma.
- **Interpretation.** I applied the HANDOFF-72 §2 product-literal interpretation unchanged. Each mutation alters the transcribed deciding branch (a comparison, a guard, a flag hand-off), not a fixture value.
- **Concurrent iteration.** Iteration-76 may edit census records and ontology pages on the same branch. This commit touches only the census records, counts and ontology metadata lines listed in §10.

## 10. Checks run

- `python3 -I .g5/uplift/coverage-20261117/iter75/check.py`: 19 models, every receipt 0 errors with current input hashes; every cited mutation killed; every locator resolves.
- `python3 -I .g5/uplift/coverage-20261117/census/census.py count`: no ERROR, WARN or STALE line. `selftest`: 9 controls detected.
- `check_ontology.py .g5/ontology`: 0 issues and 45 writing warnings, the same warnings as before the metadata edits.

## 11. Cost

- Wall time: about 05:10–05:45 UTC, about 35 minutes.
- Fork sub-agent tokens: R345 197k, R67 188k, R8A 172k, R8B 242k, about 0.8M in total. The builder session's own tokens and currency are not visible to me.
- Verus: 94 mutation runs and 20 receipt runs. Most finish in under 5 s. The uninstall runs take 17–40 s.

## Files

- Models: `.g5/Verus_verification/coverage-20261117/iter75/*.rs` (19 files).
- Here:
  - `ledger/*.json`: per-group entries, bindings rows, not-done lists and cost. `ledger/R12_make.py` regenerates R12.
  - `apply.py`, `delta.json`, `bindings.json`: the census regrade, metadata links and bindings.
  - `mutate.py`, `mutations.jsonl`, `run_verus.py`, `receipts/`, `common.py`: mutations and receipts.
  - `check.py`, `summarize.py`, `before-counts.json`.
