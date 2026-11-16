# Iteration 51: `information` top-level modules

Batch 9, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter51/toplevel.rs), [cases](../../../Verus_verification/consolidated/iter51/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). The owner bullet is in [inspect](../../../ontology/features/inspection.g5.md#inspect). `information` had no product test and no binding.

| Case | Observed | Predicted |
|---|---|---|
| `info` with top51 depending on dep51 and MixedDep51 | IPM, Ext51Label and top51 listed; both dependencies hidden | owner: same. Source: MixedDep51 listed, because names are compared case-sensitively (refuted, since stored names are lowercase) |
| `info -v` | `Ext51Label (ext51)` | same |
| version of `1.0.0+snapshot` | `1.0.0` | build dropped |

There is no defect. Evidence: 2 verified, 3/3 controls rejected, [native receipt](native/20261116T180139215751Z/observations.json) with 21 fields, 11 replayed, and the three flips rejected. Limits: intro message content beyond the system mode is not assessed.
