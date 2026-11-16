# Iteration 48: CPF merge timing

Batch 8, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter48/cpfmerge.rs), [cases and controls](../../../Verus_verification/consolidated/iter48/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). New owner: [module.processor.cpf](../../../ontology/concepts/modules.g5.md#module.processor.cpf).

| Case | Observed | Predicted |
|---|---|---|
| l1 `load m48` (CPF A default, CPF B custom) | mapping A created, B absent | same |
| c1 `m48 compile -only` | no change | same |
| p1 `m48 map48` | mapping B created | same |
| x1 `m48 bad48` (unknown section) | fails, exit 7 with details and messages.log pointer | same |

There is no defect. The product's own `CPFMerge.cls` test covers the same rules. The first harness run stopped because the `when-ignored` control survived; it was kept under `attempts/`, and the `after_is_not_before` lemma was added. Evidence: 3 verified, 4/4 controls rejected, [native receipt](native/20261116T175828056236Z/observations.json) with 17 fields, 6 replayed, and the three flips rejected. Limits: After timing comes from source and the product test, not from a native run here. `iris merge` is a provider.
