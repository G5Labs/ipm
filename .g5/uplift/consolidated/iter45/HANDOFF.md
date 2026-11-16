# Iteration 45: sync file discovery

Batch 7, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter45/scan.rs), [cases and controls](../../../Verus_verification/consolidated/iter45/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [devtools.sync](../../../ontology/features/developer-tools.g5.md#devtools.sync).

Iteration32 bound sync change detection and routing but treated the directory walk as a premise. This iteration binds the walk, path normalization and document naming in `Storage/FileHash.cls`.

| Case | Observed | Predicted (owner and source) |
|---|---|---|
| f1 edit a class two folders deep | synced (Inner 2) | synced |
| f2 edit an include file | synced (include 2) | synced |
| f3 new `Up.CLS` with uppercase extension | synced (Up 3) | synced |
| f4 edit `Top.cls`, then make it unreadable | sync fails "Cannot open file", Top stays 1 | fails |
| f5 restore access, sync again | synced (Top 4) | retried |

Every prediction matched, and this lane found no defect.

Evidence: `scan.rs` 5 verified. Four controls were each rejected: `unreadable-ignored`, `skip-dirs-walked`, `new-file-needs-baseline` and `uppercase-ext-ignored`. The [native receipt](native/20261116T175348033084Z/observations.json) has 49 gated fields. Six fields replay in Verus, and flips of `f4.ok`, `f1.inner` and `f3.up` were each rejected. Command: `python3 .g5/Verus_verification/consolidated/iter45/run.py` (14 s).

Reach: see the [project map](project-map.json), which excludes the unsealed lanes 43 and 44. Limits: the SQL fallback walker and `DeduplicateScanDirs` with nested declared directories are not exercised. Python `os.walk` and `hashlib` are providers.
