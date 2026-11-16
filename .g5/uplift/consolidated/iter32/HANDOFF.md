# Iteration 32: developer-mode sync deletion, manifest change, layout and order

Batch 2, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter32/sync.rs), [cases and controls](../../../Verus_verification/consolidated/iter32/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json), owner [devtools.sync](../../../ontology/features/developer-tools.g5.md#devtools.sync).

## Gap

Sync change detection, retry and test routing were already owned, but the `-delete` path, the manifest-change path, whole-namespace ordering and the scan directory had no binding: `SyncApplyDeletes`, `SyncBuildReverseIndex`, `SyncRoutePathSet`, `SyncCheckModuleXml`, `SyncCommitModuleXml`, `CollectScanDirs` and `GetDevModeModulesInDependencyOrder`.

## Predictions and observations

Owner predictions were written before any Pipeline body was read. Source predictions were added after reading it. Both are in the ledger.

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| s1 delete `B.cls`, `sync syncc` | B kept, "1 deleted file(s) detected but not applied" | B kept | same |
| s2 `sync syncc -delete` | B removed, A and C kept | removed | same |
| s3 edit A, delete C, `-delete` | A=2, C removed | same | same |
| s4 delete a single-class resource file | D removed | removed | same |
| s5 manifest adds `SyncE.PKG` | E loaded, value 5 | unspecified | loaded as new |
| f1 flat layout `src/SyncF/A.cls` edited, `sync syncf` | "Nothing to sync.", A still 1 | edit synced | (not predicted; found by explore1) |
| f2 flat layout delete, `-delete` | "Nothing to sync.", B kept | removed | (found by explore1) |
| f3 `load syncf -dev` | A=2 | loaded | loaded |
| o whole-namespace order | oy before ox; tx before ty | dependency order | tx before ty |

Exploratory run 1 placed every module in the flat layout, and every sync reported "Nothing to sync". That miss led to the source trace behind C:D36, and the fixture then split into a conventional module (`syncc`) and a flat one (`syncf`). Both exploratory logs are kept under `attempts/`.

## Evidence

- Formal: `sync.rs` 15 verified, 0 errors. Seven controls were each rejected by an assertion: `delete-without-flag`, `scan-uses-load-fallback`, `nothing-ignores-deletes`, `new-resource-needs-baseline`, `delete-keeps-document`, `dev-edge-filter-removed` and `row-order-only`. The generic lemmas cover any file set: no deletion without `-delete`, untracked files are never changed, and `-delete` removes a deleted compilable file's document.
- Native: [receipt](native/20261116T171504430334Z/observations.json), 206 fields, all equal to pre-run gates, network none, source bytes matched the pin.
- Replay: 191 fields asserted in Verus ([receipt](replay/receipt.json)). Flips of `s1.notapplied`, `s2.exists.SyncC.B` and `f1.value.SyncF.A` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter32/run.py` from the repository root (25 s).

## Findings

- [C:D36](../BUGS.md#c-d36), Medium, E2E reproduced: a flat-layout module loads, but sync never sees its edits or deletions.
- [C:D37](../BUGS.md#c-d37), Low, E2E reproduced: whole-namespace order ignores dependencies through a module not in developer mode.
- Both are discoveries from this search.

## Reach

[Project map](project-map.json): 420/1216 (iteration31: 413, none lost). Newly reached: `SyncApplyDeletes`, `SyncBuildReverseIndex`, `SyncCheckModuleXml`, `SyncCommitModuleXml`, `SyncRoutePathSet`, `FileHash.CollectScanDirs` and `GetDevModeModulesInDependencyOrder`. These already-linked members gained this active role: `Pipeline.Run`, `Main.Sync`, `Document.GetSyncDirectory`, `Document.OnItemRelativePath`, `Document.OnPhase`, `FileHash.ComputeChanges` and `FileHash.CommitChanges`. That gives 7 new plus 7 strengthened, 14 distinct.

## Limits

Files are abstracted to single-class documents with value-as-hash. Non-compilable resources, preload and path-style resources, explicit Directory spellings, `-test` and retries are outside this lane or owned elsewhere. The SQL row order, SHA-1, `$system.OBJ.Delete` and the compiler are native-qualified premises. No ObjectScript body is verified directly.
