# Iteration 37: Copy resource processor from load to uninstall

Batch 4, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter37/copy.rs), [cases and controls](../../../Verus_verification/consolidated/iter37/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). New owner: [module.processor.copy](../../../ontology/concepts/modules.g5.md#module.processor.copy).

## Gap

`ResourceProcessor/Copy.cls` had no binding and no ontology owner. Its intent is stated only in its description and property comments.

## Predictions and observations

| Case | Observed | Doc/owner predicted | Source predicted |
|---|---|---|---|
| o1 copy CopyA from CPSRC, target holds older C2 | C1..C3 copied and compiled, C2 replaced (20) | replaced | replaced |
| o2 `Overwrite="0"`, target holds D1 | D1 kept (4), D2 copied (41) | kept | kept |
| o3 target N1 changed after the source | replaced (50) | replaced | kept, because `/checkuptodate` skips it (refuted) |
| o4 package class owned by another module | Owned kept (6), Free copied (61) | not stated | excluded |
| u1 `uninstall cpa` | C1..C3 deleted, including pre-existing C2 | unspecified | deleted |
| u2 `uninstall cpb` | D1 and D2 deleted | unspecified | D1 deleted too (added before run 5) |
| u4 `uninstall cpd` | Owned kept, Free deleted | | Owned kept |

Exploratory run 1 used `<Attribute Name="Source">` child elements. They were not stored, the processor's required Source was empty and validation failed. The fixture now uses XML attributes on `<Resource>`. Whether `<Attribute>` children are meant to work in this manifest form was not investigated. All attempts are kept under `attempts/`.

## Evidence

- Formal: `copy.rs` 5 verified, 0 errors. The generic lemmas cover any class universe: Overwrite false keeps an existing class, an owned class is never copied or deleted, Overwrite true replaces, and uninstall deletes a class the load kept. Five controls were each rejected: `overwrite-ignored`, `overwrite-false-copies-nothing`, `owned-copied`, `uninstall-deletes-owned` and `uninstall-only-copied`.
- Native: [receipt](native/20261116T173211781373Z/observations.json), 136 fields, all gated. The run creates namespace CPSRC in the disposable container.
- Replay: 128 fields in Verus ([receipt](replay/receipt.json)). Flips of `u2.defined.CopyB.D1`, `o2.value.CopyB.D1` and `o4.value.CopyD.Owned` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter37/run.py` (17 s).

## Findings

- [C:D40](../BUGS.md#c-d40), High, E2E reproduced: uninstalling a Copy resource deletes classes that `Overwrite="0"` kept and the module never wrote. Discovered in this search.

## Reach

[Project map](project-map.json): 437/1216 (iteration34: 433, none lost). This map excludes the still-unsealed lanes 35 and 36. Newly reached: `Copy.OnAfterPhase`, `Copy.OnResolveChildren`, `Copy.OnPhase` and `Copy.TargetSet`. Strengthened with this role: `Lifecycle/Base.cls::%Clean`. That gives 4 new plus 1 strengthened.

## Limits

Classes only. The `CompileAfter` and `Flags` attributes, routines and mapped classes are not assessed. The export/load providers and timestamp handling are native-qualified. No ObjectScript body is verified directly.
