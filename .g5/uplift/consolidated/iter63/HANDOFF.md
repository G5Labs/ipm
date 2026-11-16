# Iteration 63: what uninstall removes, by resource kind

Batch 13, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter63/cleanup.rs), [cases and controls](../../../Verus_verification/consolidated/iter63/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [uninstall](../../../ontology/features/uninstall.g5.md#uninstall) ("removes an installed module's resources and its record").

One non-developer module `u63` declares every ordinary kind, plus declared global and package mappings to a run-local database `U63DB`. A developer-mode module `u63d` shows the bookkeeping. The cases are load and uninstall (x1), developer-mode load and uninstall (x2), and reload then `uninstall -purge` (x3).

| Kind | After uninstall (x1; x3 identical) | Status |
|---|---|---|
| `.CLS`, `.PKG` classes | removed | as owned |
| `.INC` | removed | as owned |
| `.MAC` with its `.INT` and `.OBJ` | removed | as owned |
| `.GBL` by name only | data kept | known P:F-UNIN-5, re-observed |
| `.GBL` with `Global="^U63G"` | data deleted | as owned |
| FileCopy file and folder | removed | as owned |
| FileCopy with `KeepOnUninstall="1"` (file, folder) | kept | as owned |
| module record, manifest stream | removed | as owned |
| sync baseline rows (developer mode) | removed (foreign-key cascade) | as owned |
| declared `<Mapping>` global `U63M` and package `U63MP` | **kept**, also after `-purge` | new, proposed Medium |
| developer-mode `^Sources("MODULE","u63d")`, `^Sources("cls","U63D.")` | **kept** | new, proposed Low |

The owner predictions said uninstall removes everything the module set up. Source predictions, written after reading `%Reload` and searching for removal callers, matched every case. The first exploratory run's `^Sources` counter used the wrong key, so a second run dumped the nodes. Both logs are kept under `attempts/`.

Evidence: `cleanup.rs` 5 verified, 0 errors. Five controls were each rejected: `mapping-removed`, `sources-cleared`, `keep-ignored`, `name-only-global-deleted` and `purge-deletes-globals`. [Native receipt](native/20261116T183505649094Z/observations.json): 122 gated fields, 37 replayed in Verus, and flips of `x1.map.pkg`, `x2.dev.sourcesmodule` and `x1.gbl.attr` were each rejected. Command: `python3 .g5/Verus_verification/consolidated/iter63/run.py` (15 s).

Reach: no new members, since every deciding member was already linked. Roles were added to `%Clean`, `%Reload`, `Global.OnPhase`/`OnResolveChildren`, `FileCopy.OnPhase`, `Document.OnPhase`, `AddGlobalMapping` and `AddPackageMapping`.

Limits: table data and `-purge` remain owned by `uninstall.purge.as-built` (P:F-UNIN-2) and were not re-tested. Routine and include mappings declared with `<Mapping Name="X.MAC|INC">` follow the same Reload code but were not run. Config.Map* and $system.OBJ are providers. No ObjectScript body is verified directly.

## Proposed owner text

In `.g5/ontology/features/uninstall.g5.md#uninstall`, add bullets:

- "Uninstall deletes the module's classes, packages, includes and routines (with their compiled forms), its record, its stored manifest and its sync baseline. A global resource's data is deleted only when the resource names it with `Global`. FileCopy targets are deleted unless `KeepOnUninstall` is set."
- "As built, namespace configuration that loading made for the module stays. Declared `<Mapping>` entries remain, also with `-purge` (defect C:D-next-1). A developer-mode module's `^Sources` entries remain and keep pointing at its old folder (defect C:D-next-2)."

Metadata line: "Iteration63: [cleanup.rs](../../Verus_verification/consolidated/iter63/cleanup.rs) `remains`, `code_removed`, `globals_by_declaration`, `filecopy_keep`, `reload_side_effects_outlive_uninstall` ([handoff](../../uplift/consolidated/iter63/HANDOFF.md))." Add Findings lines for both entries.

C register entries:

- **C:D-next-1, Medium · Decision: Open: "Uninstall leaves a module's declared `<Mapping>` entries in the namespace".** Found by: source search (no uninstall caller of `RemoveGlobalMapping`/`RemovePackageMapping` for `Module.Mappings`), then iteration63 model `reload_side_effects_outlive_uninstall` with control `mapping-removed`. Reproduced by: E2E reproduced, receipt `iter63/native/20261116T183505649094Z`. After `load /tmp/iter63/u63` adds `U63M.GBL -> U63DB` and `U63MP.PKG -> U63DB`, both mappings remain after `uninstall u63` (x1) and after `uninstall u63 -purge` (x3). Passing neighbour: every code resource, the record and the non-kept FileCopy targets are removed in the same runs. This is the declared-mapping counterpart of C:D65, whose producer is the automatic mapping in `OnConfigureMappings`. Repair, unchecked: remove the module's declared mappings in Clean unless another installed module declares the same mapping.
- **C:D-next-2, Low · Decision: Open: "Uninstall leaves a developer-mode module's `^Sources` entries".** Found by: source search (no `^Sources` kill for module entries), iteration63 model `reload_side_effects_outlive_uninstall` with control `sources-cleared`. Reproduced by: E2E reproduced, same receipt. After `load u63d -dev` and `uninstall u63d`, `^Sources("MODULE","u63d")=/tmp/iter63/u63d/` and `^Sources("cls","U63D.")=/tmp/iter63/u63d/src/cls/` remain (x2). Passing neighbour: the module's record, class and sync baseline rows are removed. A later document in that package is still mapped to the old folder for source-control export. Repair, unchecked: kill the module's `^Sources` entries in Clean.
