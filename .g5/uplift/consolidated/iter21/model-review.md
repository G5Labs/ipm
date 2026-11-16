# Cached version selection and mutable directory loading

This lane closes iteration19's explicitly unassessed post-cache physical-edit boundary. It keeps the saved cache snapshot separate from current files and derives both the selected cached version and the actual loaded version. Native and model establish a requested-version mismatch in the checked ordinary mutable-directory case. Parent reconciled the existing explicit-version intent and records **C:D32**; no product repair or new policy choice is implemented here.

Product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`; baseline21 `cd87e5b9b84739a9e2113ea5d371acee980099c34b98a9b39dcb2280c3327681` (14515 members); prior20 seal `8e92af95b0d96764e776fc918e7758df7fbb10b6705a3fcc3e9bd03aa1d1cb70` clean-verified. Only the authorized model/cases/model-lane evidence and this review were written by this lane. Parent owns native execution, canonical owners/registers, source-map reconciliation and integration. No product/native/harness/Docker/Git edits or jobs, new trusted declarations or evaluator-private reads were performed here. Archive paths remain **UNASSESSED/UNPURSUED**; only the permitted GetModuleManifest25-30 and GetModuleDirectory48-55 ranges were read, never the excluded adjacent method.

## Frozen prediction and qualified outcomes

The queued gap came from iteration19's own stable-source predicate: it can identify the cached directory/manifest but cannot predict a load after its physical contents change. Before native, the frozen contrast was stable cache1/physical1/request1 versus unchanged cache1/current physical2/request1. Source showed exact-version selection from the cache followed by ordinary loading of the directory's current module.xml/code, without comparing its version back to the request. The working as-built prediction was success with actual2, while intended explicit-request fulfillment remained a separate question until authority reconciliation.

The exact ordinary scenario uses alias `temporal`, module `iter21-cache`, class `Iter21Cache.Payload`, root `/tmp/iter21/repo/` and retained subdirectory `active`. Version1 is `1.0.0`, Description `source-A`, callable101; version2 is `2.0.0`, Description `source-B`, callable202. Both files are replaced by ordinary completed file copies before the next install. There is no force/dev/uninstall, framework-global injection or special cache clearing.

| State/action | Cache/service | Current physical files | Saved installed version / callable |
| --- | --- | --- | --- |
| initial, explicit install1 |1 / A|1 / A|1 /101|
| replace files before next install |1 / A|2 / B|still1 /101|
| explicit install1 against stale cache |1 / A|2 / B|**2 /202**, success|
| reconfigure the same normal filesystem repository |2 / B|2 / B|2 /202 unchanged|
| explicit install1 after reconfigure |2 / B|2 / B|not-found refusal; still2 /202|
| explicit install2 |2 / B|2 / B|2 /202, success|

Native first run `20261115T215817975677Z` established the core contrast, and subsequent118-field final run `20261115T220610978852Z` explicitly reads runtime version1/payload101 after both file copies and before the stale install. The intervening116-field receipt is retained: its observation-extension script initially failed on a dictionary-alias mutation before saving, then the final parser extension was corrected. No source/model/native behavior changed. Actual search shows cache1 before rebuild and cache2 afterward; actual show reads current installed1 then2. Full cached/service manifest text equality is independently observed. Actual image: `sha256:8db58b93cf76245f6cc9cfc16aaa9627391535eab6350134ecc1bbf8bcf8b82f`.

The model's first three replay observations compare actual installed version to the explicit request: stable1=true, stale1=false, new2=true. They do not compare the request to its own selected cache key. Before/after-file-copy runtime framing is independently native-qualified rather than inferred merely from copy success. Intermediate copy windows, partial failures, watchers and concurrent readers are not generalized.

## Model, reuse and intent boundary

`temporal.rs` imports unchanged `iter19/duplicates.rs`, including unchanged owners/semver dependencies. Its generic cache key/admission/indexed-row and saved manifest/directory readers are reused. The stale witness explicitly proves iteration19's `stable_selected_source` false and its old `install_directory.assessed` false. No earlier stable-source guarantee is weakened or silently promoted to a mutable-source promise.

`World` holds `cache` and `current` separately plus the successful installed-record/code projection. A typed Version enum represents only parsed plain1.0.0/2.0.0. `physical_replace` changes the current physical Disk and frames cache/runtime. `configure_same_repo` invokes a fresh purge/build from current files and preserves the installed projection. The cache and physical Disk each contain their own manifest version/Description and the Disk supplies its actual source payload.

`selected_row` uses the cached exact `(root,name,fullVersion)` key. `actual_directory_load` follows that retained directory and reads the current Disk; installed version, Description and callable payload derive from those current contents. `Outcome.selected` derives from the cached row key. The new successful slice requires valid parsed same-name plain1/2 sources, indexed rows, stable path/configuration within a command, successful load/save/compile providers, no UpdatePackage/dependencies/dependent constraints/dev/force, and nondecreasing actual versions. Other guards/errors, missing/changed names, removal, parser/version-range behavior and general cache lifetimes/concurrency remain unassessed. The installed record/payload is a successful-provider bundle, not a claim that arbitrary saved modules imply available code; native method availability and real payload calls are checked separately.

An absent cached key refuses before the loader and frames only this selected cache/current-files/module/code projection. Actual failed-attempt history or other global effects are not asserted unchanged. Reconfiguration rebuilding cache2 removes the old key; that refusal is a not-found result, not an actual-version downgrade refusal.

`listed_version` is a single-root/name/one-cached-row exact plain-version search projection. SQL query/parser/semver matching is a provider port; it is not a general sorting/range proof. `shown_version` reads the saved installed module, so it can disagree with stale search metadata. Generic XML bytes/serialization remain outside the formal manifest projection; full cache/service XML equality stays a native gate.

The existing install help at `Main:428` calls the version argument the version/expression to install, and the pinned-install comment at2640 says filtering uses that version. Parent preserves this requested-version intent alongside the checked as-built mismatch in the canonical owner/C:D32. The revalidation negative control deliberately distinguishes a counterfactual repair from the as-built loader; rejection is not a judgment against repairing the defect. No repair/simplification implementation or new human policy decision is claimed.

## Source roles

| Role | Narrow correspondence |
| --- | --- |
| cache snapshot/new key | inherited `Filesystem.Definition.BuildCache/AddCacheItem` and Cache.RootNameVersion; fresh rebuild only, not generic refresh concurrency |
| same-definition refresh writer | `Definition.OnConfigure:78-85` saves Root/Depth then calls BuildCache(1,1,1) |
| cached exact-version producer | PackageService.ListModules → Cache.OrderedMatchesExecute/Fetch → moduleSqlToList; root/name/exact plain matching is partial/shared, not whole-body sort/parser coverage |
| saved manifest read | `PackageService.GetModuleManifest:25-30` returns the cached row's Manifest |
| saved directory read | `GetModuleDirectory:48-55` returns normalized stored Root+SubDirectory |
| selected-versus-current boundary | `Utils.Module.LoadModuleReference:92-97,116-139` puts selected version in its reference, checks selected requirements and invokes directory loading |
| actual manifest read | `LoadModuleFromDirectory:293-322` calls LoadNewModule; `LoadNewModule:1164,1203-1222` obtains the current directory module and applies actual-version guards |
| actual module/code effect | `LoadNewModule:1250-1252,1290-1304,1330-1346`, actual manifest/module refresh and phases through Activate under successful providers |
| refused exact miss | `Main.Install` exact search/no-result branch, before any selected directory load |
| actual final reader | normal `search` from cached row and `show` from saved current module; native observers parse their actual output independently |

These are hand-translated specification relations plus qualified source/native correspondence, with **zero direct ObjectScript executable-body proofs**. Earlier owners' separately withdrawn lock/constructor correspondence remains withdrawn; verifying its unchanged mathematics does not restore that source claim. Parent owns the19-member partial/narrow link audit and four new locators. Newly bound members, seven newly modeled units/temporal contrasts and118 native-qualified observations are different quantities.

## Verification and replay

Complete positive candidate: **74 verified,0 errors =67 inherited +7 new units**. Four unique focused unchanged-source witnesses each verify one unit; all19 local controls reject semantically with no time/resource/recursion-limit credit. Their bodies call definitions only, with no proof lemmas/broadcasts. Full positive scope/options remain default; negative selectors use `--verify-root --verify-function <selector> --multiple-errors 0`. Only one byte-literal version inequality is computed in each witness, never full Maps/Worlds. Direct suite wall29.346s.

Controls distinguish automatic refresh/compile effects on file edits, wrong physical source code, cached versus current loader fields, request revalidation, live versus saved service metadata, not-found fallback/status/frame, nonpurging rebuilds, runtime clearing, selected-versus-installed equality and search/show source choice. No imported dependency is mutated, so nested imports remain intact.

Final configured **grouped** replay verifies **75 units,0 errors**, with88 assertions in one observation proof plus the74 model units, in2.783s. It matches all118 fields through88 model aliases and30 provider/CLI/raw-status gates. An earlier **per-field** diagnostic also passed162 units,0 errors in3.287s. Its initial metadata incorrectly called that layout grouped; exact original metadata/source/log are preserved under `per-field-replay`, alongside the corrected per-field receipt. The actual grouped source/receipt now match `replay_grouped=true`.

Control rules and formal source did not change when edited-runtime aliases or replay layout/support notes were added. Exact prior cases are retained as `model-lane/controls/config.json`; unchanged control-input digest is `2cdb198b49f2b718ec6346d1bfff91e9e072d9ceae188d82487376b81a46a986`. No extra unchanged control rerun was needed. There were no failed model/control/native outcome predictions in this lane.

Parent integration subsequently exposed an adapter miss: the direct root runner can use private alias `p`, while a generic `use model::*` replay wrapper does not export that alias. The integrated74-unit baseline and19 controls passed before its replay compilation failed. Parent retains the exact failed generated source/log/config and qualifies replay/prelude references through public `model::inherited_duplicates::` in its parent cases only. Owned formal source/cases, expected observations, control rules and native inputs remain frozen. This is a namespace-adapter compile failure, not semantic rejection, and receives no control credit. The valid direct-root75-unit grouped receipt above remains independently reproducible; parent owns the public-wrapper reproduction and its final receipt.

Frozen model SHA256 `0bc4ec8a219b30b806ebdaec4b1d0889e9efa79195ac81cba659531c79e968de`; final cases SHA256 `4ac72d4b67bd00ec2119b47a824c6dd96b2679aeaab1644a00532dccb7adadcc`. Final native observation SHA256 `e21603f2f52d621ecab34b19a9da55b6a1f27f201a8463658db7736a7c94955c`. Exact commands/hashes/logs are in `controls/receipt.json`, `replay-receipt.json` and `final-summary.json` under the model lane.

The updated public scoring guide SHA `8442346f9c3e58a33a5a1aea90a8c40c45bf71683858b6999b3a144f509c581d` was read at its exact known path. Parent retains the previous976 guidance receipt; numbering/product/snapshots were not restarted. This workflow and its source/model cases were builder-selected and already known before any independent evaluator prospective window. No private cases/locators/seeds/ledger were searched or read, and this batch supplies no claim of unseen independent discovery or project-wide convergence.
