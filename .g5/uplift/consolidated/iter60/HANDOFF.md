# Iteration 60: legacy localized-messages processor

Batch 12, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter60/legacymsgs.rs), [cases and controls](../../../Verus_verification/consolidated/iter60/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). This lane is not sealed; the main thread integrates it. Iteration56 owns the default `.LOC` processor. This lane covers `<Resource ... ProcessorClass="LegacyLocalizedMessages" includeFiles="..." [merge="1"]/>`, which reads a message XML file at the module root and generates include files of message-name macros.

## Predictions and observations

| Case | Observed | Doc predicted | Source predicted |
|---|---|---|---|
| p1 `includeFiles="L60Inc1"`, two named messages | both in `^IRIS.Msg`; `L60Inc1.INC` holds both macros; child `L60Inc1.INC` | same | same |
| p2 `includeFiles="L60DomA:L60IncA,L60DomB:L60IncB"` | both domains loaded; IncA holds only A, IncB only B; the single child is `L60IncA,L60DomB.INC` | separate includes | same, but the child list was predicted to contain IncA |
| p3 reload p1 with ErrTwo removed, text changed, extra id 98 set beforehand | ErrTwo and id 98 gone, text updated, include lacks ErrTwo | replaced | replaced |
| p4 `merge="1"` with pre-existing id 99 in the mapped domain | id 99 kept with the loaded message | merged | merged |
| u1 `uninstall lm60` (mapped form) | `L60IncA.INC` and `L60IncB.INC` both remain | unspecified | IncA deleted, IncB kept (refuted: both kept) |
| u2 `uninstall ls60` (single form) | `L60Inc1.INC` deleted | unspecified | deleted |

Messages remain in `^IRIS.Msg` after both uninstalls. That is the C:D59 class seen with the default processor, so it is not re-filed. Explore1 refuted the u1 source prediction, and explore2 added child-name readers. Both logs are kept under `attempts/`.

## Evidence

- Formal: `legacymsgs.rs` 6 verified, 0 errors. The generic lemmas are `single_include_cleaned`, `one_mapped_entry_cleaned`, `mapped_includes_leak` and `replace_and_merge`. Four controls were each rejected: `children-per-entry`, `merge-ignored`, `replace-keeps-old` and `single-not-child`.
- Native: [receipt](native/20261116T182833808281Z/observations.json), 106 gated fields, network none, source matched the pin. Replay: 12 fields ([receipt](replay/receipt.json)). Flips of `p2.child.IncA`, `u1.incB.B` and `u2.inc1.one` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter60/run.py` (12 s).

## New discrepancy

One new Low defect, E2E reproduced (see the register entry below). With two or more `domain:include` entries, `OnResolveChildren` takes `$piece(includeFiles,":",2)` of the whole attribute, so the only child is the garbage name `L60IncA,L60DomB.INC`. Uninstall therefore leaves every generated include. The `ExtensionBase/Utils.cls` include-owner lookup, which uses the same children, cannot attribute them to the module either; that lookup was read but not run.

## Reach candidates

Newly reached: `LegacyLocalizedMessages.cls::{OnBeforePhase, GetDomainMap, GetDomains, MergeMessageDomains, OnResolveChildren}`. Strengthened: `Lifecycle/Base.cls::%Clean`.

## Limits

Validation of malformed `includeFiles` is not exercised; its check reads only the first entry. Neither are languages other than `en`, the LocalizationExport processor (`Export`, `SaveMessageFile`, `%OnValidateObject`, which are not on this path), or the include-owner lookup. `Import^%occMsgXML` and `GenerateInclude^%occMsgXML` are providers. No ObjectScript body is verified directly.

## Proposed owner text

New child statement in `.g5/ontology/concepts/modules.g5.md`, after `module.processor.localized-messages`:

```
<a id="module.processor.localized-messages.legacy"></a>
[D · module.processor.localized-messages.legacy]

A resource with `ProcessorClass="LegacyLocalizedMessages"` loads a message XML file from the module root when the module compiles and generates include files of message-name macros: one include for `includeFiles="<include>"`, or one per target for `includeFiles="<domain>:<include>,..."`. Each domain in the file replaces that domain's messages, except that with `merge="1"` earlier messages of the mapped domains are kept.

- Uninstall deletes the generated include of the single form. As built, with two or more `<domain>:<include>` entries no generated include is recognized as the module's, so uninstall leaves them all (defect C:D-next).
```

Metadata: `agent`, `user-facing-behavior`, Expands [Localized messages](#module.processor.localized-messages). Sources: `ResourceProcessor/LegacyLocalizedMessages.cls` (`OnBeforePhase`, `OnResolveChildren`, description and `merge` comment). Model: [legacymsgs.rs](../../Verus_verification/consolidated/iter60/legacymsgs.rs) `replace_and_merge`, `single_include_cleaned`, `mapped_includes_leak` ([handoff](../../uplift/consolidated/iter60/HANDOFF.md)). Findings line to the new entry. The bindings already use the anchor `module.processor.localized-messages.legacy`.

Proposed C register entry, **Low · Decision: Open**: "Uninstall leaves the includes generated by a multi-entry LegacyLocalizedMessages resource".

- Found by: iteration60 exploration (explore1 refuting the source prediction), then source review of `ResourceProcessor/LegacyLocalizedMessages.cls::OnResolveChildren`, with the iteration60 model `mapped_includes_leak` and control `children-per-entry`.
- Reproduced by: E2E reproduced, receipt `iter60/native/20261116T182833808281Z`. For `includeFiles="L60DomA:L60IncA,L60DomB:L60IncB"`, the resource's only resolved child is `L60IncA,L60DomB.INC` (p2), and after `uninstall lm60` both `L60IncA.INC` and `L60IncB.INC` remain (u1). Passing neighbour: the single-include form resolves `L60Inc1.INC` and uninstall deletes it (p1, u2).
- Repair, unchecked: take the include name from each list entry, `$piece(tInclude,":",2)`, instead of from the whole attribute.
