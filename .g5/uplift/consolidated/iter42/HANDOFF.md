# Iteration 42: Initialize and module preload code

Batch 6, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter42/initialize.rs), [cases and controls](../../../Verus_verification/consolidated/iter42/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). New owner: [lifecycle.initialize](../../../ontology/concepts/lifecycle.g5.md#lifecycle.initialize).

## Gap

`Lifecycle/Base.cls::%Initialize` had no binding. The ontology said only that "Initialize brings in preload code".

## Predictions and observations

| Case | Observed | Owner/CHANGELOG predicted | Source predicted |
|---|---|---|---|
| setup `load m42 -dev` | Boot 1 | loaded | loaded |
| i1 edit Boot to 2, `m42 initialize -only` | skipped, Boot 1 | 2 | skipped (no RootDirectory) |
| i2 `m42 compile` | skipped, Boot 1 | 2 (chain includes Initialize) | skipped |
| i3 `load <dir> -dev` | Boot 2 | 2 | 2 |
| i4 edit to 3, `m42 initialize -only -DRootDirectory=<dir>` | Boot 3 | 3 | 3 |
| b1 `load m42b`, broken preload class | succeeds, class uncompiled | load fails | succeeds (status swallowed) |
| c1 `load m42c`, same error in `src` | fails, module not installed | fails | fails |

Explore1's "broken" class (`quit 1 +`) compiled, so it did not test the error path. The fixture was corrected to an undefined property type before explore2. The first harness run failed before any proof because the model file name `preload.rs` clashed with the product's `preload/` directory at the harness working directory. Both attempts are kept.

## Evidence

- Formal: `initialize.rs` 4 verified, 0 errors. Four controls were each rejected: `module-action-uses-module-root`, `preload-error-returned`, `broken-compiled-anyway` and `refresh-ignores-disk`.
- Native: [receipt](native/20261116T174706181264Z/observations.json), 70 fields, all gated. Replay: 20 fields in Verus ([receipt](replay/receipt.json)). Flips of `b1.ok`, `i2.boot` and `c1.ok` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter42/run.py` (12 s).

## Findings

- [C:D46](../BUGS.md#c-d46), Medium, E2E reproduced: a preload compile error does not fail the load.
- [C:D47](../BUGS.md#c-d47), Low, E2E reproduced: module actions never refresh a developer-mode module's preload code.

## Reach

[Project map](project-map.json): 454/1216 (iteration39: 452, none lost). This map excludes the unsealed lanes 40 and 41. Newly reached: `Lifecycle/Base.cls::%Initialize` and `Utils/Module.cls::RemoveUnsupportedFileErrors`.

## Limits

Resource processors' own Initialize hooks, unsupported-file filtering contents and preload in non-developer reinstall are not assessed. ImportDir and the compiler are providers. No ObjectScript body is verified directly.
