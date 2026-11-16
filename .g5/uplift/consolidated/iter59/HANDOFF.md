# Iteration 59: automatic resource mappings and code placement

Batch 12, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter59/mappings.rs), [cases and controls](../../../Verus_verification/consolidated/iter59/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [module.placement](../../../ontology/concepts/modules.g5.md#module.placement), which says only that `NoMapping` turns off automatic mappings.

## Gap

`OnConfigureMappings` in the default resource processors, `Document.ResourceIsMappedToDefaultDB` and the related uninstall branches had no binding. No owner said what triggers an automatic mapping or what uninstall does with it.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| a1 load `%ZI59.PKG` (resolves to IRISSYS by default) | mapping %ZI59 -> USER created; class in USER only | automatic mapping | same |
| a2 uninstall | class removed; mapping stays | unspecified | stays (no removal path) |
| b1 administrator maps P59 -> DB59; load `P59.Sub.PKG` | mapping P59.Sub -> USER created; class in USER | same | same |
| b2 uninstall | mapping P59.Sub stays, so P59.Sub keeps resolving to USER instead of DB59 | unspecified | stays |
| c1 load `%ZI59C.PKG` with `-DNoMapping=1` | no mapping; class compiled into IRISSYS, visible from %SYS | no mapping | same |
| c2 uninstall | "WARNING: Skipping mapped resource"; module record gone, class stays in IRISSYS | unspecified | round 2: same |
| c3/c4 reinstall, `uninstall -global` | class removed | | same |
| d1 ordinary `N59.PKG` | no mapping | no mapping | same |
| e1/e2/e3 GlobalScope `%ZI59G.PKG` | no mapping; skipped without `-global`; removed with it | owner `uninstall.global` | same |

All predictions matched. Explore1 and explore2 logs are under `attempts/`. The administrator mapping in b1 was created with Config APIs inside the disposable container.

## Evidence

- Formal: `mappings.rs` 5 verified, 0 errors. Five controls were each rejected: `mapping-removed-on-uninstall`, `nomapping-ignored`, `mapped-code-deleted`, `global-not-skipped` and `always-map`.
- Native: [receipt](native/20261116T182911128903Z/observations.json), 433 fields gated. Replay: 280 fields in Verus, and flips of `b2.b.map`, `c2.c.insys` and `e2.e.module` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter59/run.py` (17 s).

## New discrepancies

Two, both E2E reproduced and both new: no register entry covers mapping cleanup or NoMapping placement.

## Reach candidates

Newly reached: `Default/Package.cls::OnConfigureMappings`, `Default/Document.cls::ResourceIsMappedToDefaultDB`, `Utils/Module.cls::AddPackageMapping` (if not already linked), and `Lifecycle/Base.cls::CheckBeforeClean`. Strengthened: `Default/Document.cls::OnPhase` and `Lifecycle/Base.cls::%Clean`. The Class, Routine and LocalizedMessages variants of `OnConfigureMappings` apply the same rule by source inspection and are deliberately not bound.

## Limits

Only package resources were exercised. Routine and include mappings, `%ALL`, `ForceLocalScope` and `<Mappings>` are not assessed. Config mapping APIs, `GetPackageDest` and the compiler's placement are providers.

## Proposed owner text

Add to `.g5/ontology/concepts/modules.g5.md#module.placement`, after the `NoMapping` bullet:

- "When a resource would resolve to a database other than the namespace's own routine database, such as a `%Z` package or a subpackage of a package an administrator mapped elsewhere, loading adds a mapping of that package to the namespace's routine database. `NoMapping` or `GlobalScope` skips this, so such code is stored where the package resolves, for example IRISSYS."
- "As built, uninstall never removes those mappings, so a later reader of the package keeps resolving to the namespace's database, overriding the administrator's mapping (defect C:D-next-1)."
- "As built, uninstall skips classes that resolve outside the namespace's database unless `-global` is given, even for a module without global scope. A module installed with `NoMapping` therefore leaves its code behind, shared by every namespace (defect C:D-next-2)."

Metadata line: "Iteration59: [mappings.rs](../../Verus_verification/consolidated/iter59/mappings.rs) `needs_map`, `load`, `uninstall`, `mapping_outlives_uninstall`, `nomapping_code_orphaned`, `global_module_skipped` ([handoff](../../uplift/consolidated/iter59/HANDOFF.md))." plus Findings lines for both entries.

C register entries:

- **C:D-next-1, Medium, Decision: Open: "Uninstall leaves the package mappings IPM created, overriding an administrator's mapping".** Found by: source review (no removal caller for mappings added by `OnConfigureMappings`), iteration59 model `mapping_outlives_uninstall`, control `mapping-removed-on-uninstall`. Reproduced by: E2E reproduced, receipt `iter59/native/20261116T182911128903Z`. After `uninstall m59a` the USER mapping `%ZI59 -> USER` remains (a2). With an administrator mapping `P59 -> DB59`, `load m59b` adds `P59.Sub -> USER`, and after `uninstall m59b` it remains, so `P59.Sub` still resolves to USER rather than DB59 (b2). Passing neighbour: an ordinary package gets no mapping (d1). Repair, unchecked: record the mappings a load created and remove them on uninstall unless another module needs them.
- **C:D-next-2, Medium, Decision: Open: "Uninstall of a NoMapping module leaves its code outside the namespace".** Found by: native exploration (c2) and source review of `Lifecycle/Base.cls::%Clean` (`IsMapped` classes skipped unless `Clean.GlobalScope`), iteration59 model `nomapping_code_orphaned`, control `mapped-code-deleted`. Reproduced by: E2E reproduced, same receipt. `load m59c -DNoMapping=1` compiles `%ZI59C.Main` into IRISSYS. `uninstall m59c` prints "Skipping mapped resource", removes the module record and leaves the class visible from %SYS and every namespace (c2). Passing neighbour: `uninstall m59c -global` removes it (c4). Repair, unchecked: delete classes the module itself installed regardless of where they resolve, or refuse NoMapping placement outside the namespace database.
