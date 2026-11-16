# Iteration 58: DeepSeeItem (.DFI) resources

Batch 12, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter58/dfiitems.rs), [cases and controls](../../../Verus_verification/consolidated/iter58/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). Not sealed; for integration by the main thread.

| Case | Observed | Owner/doc predicted | Source predicted |
|---|---|---|---|
| n1 `load m58n` (non-dev, explicit DashN) | DashN created | created | created |
| d1 `load m58 -dev`: explicit A, B; wildcard `IPMCont58W-*.DFI` with W1.xml, W2.dfi, W3.XML, W4.DFI | A, B, W1, W4 created; W2, W3 absent | all created | W2, W3 skipped |
| r1 edit A's title, `m58 reload -only` | A updated (T2) | updated | updated |
| r2 delete B's file, reload | fails "Resource path ... not found", B kept | unspecified | fails (explicit resource without file) |
| u1 `uninstall m58` | A, B removed; W1, W4 remain | removed | explicit removed |
| u2 `uninstall m58n` | DashN removed | removed | removed |

New discrepancies, both reproduced end to end:
1. On Linux, a wildcard DFI resource skips files named `.dfi` or `.XML`. `OnPhase` builds `n.xml;n.dfi` and then appends `n.XML;n.DFI` without a `;`, so the patterns become `n.xml`, `n.dfin.XML`, `n.DFI` (W2, W3). Proposed Low: declared content is silently missing, and conventional `.xml` exports are unaffected.
2. Uninstall leaves the items a wildcard DFI resource loaded (W1, W4 after u1), while explicit items are removed. Proposed Low.

Evidence: `dfiitems.rs` 3 verified, 0 errors; controls `separator-present`, `wildcard-cleaned`, `explicit-kept` and `lowercase-only` were each rejected. [Native receipt](native/20261116T182720911100Z/observations.json) has 48 gated fields. 12 replay in Verus, and flips of `d1.item.IPMCont58W.W2`, `u1.item.IPMCont58W.W1` and `d1.item.IPMCont58W.W3` were each rejected. Command: `python3 .g5/Verus_verification/consolidated/iter58/run.py` (12 s).

Reach candidates (newly reached): `DeepSeeItem.cls::OnPhase`, `::%OnNew`, `::OnResolveChildren`, `::OnGetUniqueName`. Limits: Keywords filtering, Format/UDL export (excluded) and non-Linux pattern behaviour are not assessed.

## Proposed owner text

New statement in `.g5/ontology/concepts/modules.g5.md` after `module.processor.cpf`, with the processor list's DFI mention linking to it:

```
<a id="module.processor.deepsee"></a>
[D · module.processor.deepsee]

A `.DFI` resource loads an Analytics folder item, such as a dashboard or pivot, from `<SourcesRoot>/dfi/`, where `-` in the item name becomes a folder separator. A name containing `*` loads every matching file in that folder.

- As built on Linux, a wildcard resource loads only files ending in `.xml` or `.DFI`; files ending in `.dfi` or `.XML` are silently skipped (defect C:D-next-1).
- Uninstall removes explicitly named items. As built, items loaded through a wildcard resource remain (defect C:D-next-2).
```

Metadata: `agent`, `user-facing-behavior`, Expands [Resource processors](#module.processor). Sources: `ResourceProcessor/Default/DeepSeeItem.cls`; product fixture `resource-tests`. Model: [dfiitems.rs](../../Verus_verification/consolidated/iter58/dfiitems.rs) `wildcard_extension_neighbors`, `uninstall_neighbors`, `leftover_after_uninstall` ([handoff](../../uplift/consolidated/iter58/HANDOFF.md)). Findings lines to both entries.

C register entries:
- **Low · Decision: Open — "On Linux, a wildcard DFI resource skips `.dfi` and `.XML` files".** Found by: source review of `DeepSeeItem.cls::OnPhase` (UNIX branch appends patterns without `;`), model `wildcard_extension_neighbors`, control `separator-present`. Reproduced by: E2E reproduced, receipt `iter58/native/20261116T182720911100Z`: W2.dashboard.dfi and W3.dashboard.XML are not created while W1.xml and W4.DFI are (d1). Passing neighbour: explicit `.xml` items load (d1, n1). Repair, unchecked: add the missing `;`.
- **Low · Decision: Open — "Uninstall leaves DFI items loaded through a wildcard resource".** Found by: native u1, model `leftover_after_uninstall`, control `wildcard-cleaned`. Reproduced by: E2E reproduced, same receipt: W1 and W4 remain after `uninstall m58` while A and B are removed (u1). Repair, unchecked: resolve wildcard DFI children at Clean from the namespace folder items matching the pattern.
