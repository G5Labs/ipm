# Iteration 56: localized message (.LOC) resources

Final convergence probe (a different underserved workflow), product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter56/locmsgs.rs), [cases and controls](../../../Verus_verification/consolidated/iter56/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). The owner today is `modules.g5.md#module.processor`, which says only "localization processors". Not sealed and not committed; the main thread integrates it.

## Predictions and observations

Predictions were written from processor descriptions before any body was read. Source predictions were added afterwards.

| Case | Observed | Doc predicted | Source predicted |
|---|---|---|---|
| l1 `load loc56` (A=alpha, B=beta) | A alpha, B beta; `<root>/localize/D56Msgs.xml` written into the source folder | messages present | same, plus the export file |
| l2 edit the .LOC (A=alpha2, drop B, add C), load again | A alpha2, B gone, C gamma | overwritten | domain replaced |
| u1 `uninstall loc56` | document and module gone; A and C remain in `^IRIS.Msg`; export file remains | removed | document deleted; messages unclear |
| r1 load a .LOC module from a read-only folder | fails `Cannot open file '<root>/localize/D56RMsgs.xml'`; nothing installed | succeeds | fails at Activate |
| n1 load a module without .LOC from a read-only folder | succeeds | succeeds | succeeds |

In explore1 the n1 fixture renamed the module but not its document name, so it failed for an unrelated reason. That was fixed before explore2, and both logs are kept.

## Evidence

- Formal: `locmsgs.rs` 4 verified, 0 errors. The generic lemmas are `reload_replaces_domain`, `readonly_root_fails` and `uninstall_keeps_messages`. Four controls were each rejected: `readonly-tolerated`, `reload-merges`, `uninstall-clears-messages` and `no-export-write`. The last one survived the pre-check until export obligations for l1/r1 were added, before the harness run.
- Native: [receipt](native/20261116T182256528947Z/observations.json), 32 fields, all gated. Replay: 27 fields. Flips of `r1.ok`, `u1.msg.A` and `l2.msg.B` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter56/run.py` (12 s).

## New discrepancies

- Proposed **Medium**, E2E reproduced: a module with a `.LOC` resource cannot be loaded from a read-only source folder, because Activate writes the localization export into the module root (r1 against n1).
- Proposed **Low**, E2E reproduced: uninstall leaves the module's messages in the message dictionary (u1).

## Reach candidates

`ResourceProcessor/Default/LocalizedMessages.cls::{OnBeforePhase, OnPhase}` and `ResourceProcessor/LocalizationExport.cls::{OnBeforePhase, GetDomainsForResource, GetMessageFile}`. Strengthened: `Lifecycle/Base.cls::%Clean`.

## Limits

Explicit `LocalizationExport` resources (Domains/Filename), the UDL format, languages other than `en`, `LegacyLocalizedMessages` and `StudioDocument/LocalizedMessages` are not assessed. The message compiler and `ExportDomainList^%occMessages` are providers. No ObjectScript body is verified directly.

## Proposed owner text

New statement in `.g5/ontology/concepts/modules.g5.md`, after `module.processor.cpf`, with the processor list's localization item linking to it:

```
<a id="module.processor.localized-messages"></a>
[D · module.processor.localized-messages]

A `.LOC` resource holds localized messages, which are loaded into the namespace's message dictionary when the module compiles. Loading a changed file replaces that domain's messages for its language, so a message removed from the file disappears.

- During Activate, IPM also writes the file's domains to `<module root>/localize/<name>.xml`. As built, that write is required, so a module with a `.LOC` resource cannot be loaded from a read-only folder (defect C:D-next-1).
- As built, uninstall deletes the `.LOC` document but leaves its messages in the message dictionary and the exported file in place (defect C:D-next-2).
```

Metadata: `agent`, `user-facing-behavior`, Expands [Resource processors](#module.processor). Sources: `ResourceProcessor/Default/LocalizedMessages.cls`, `ResourceProcessor/LocalizationExport.cls::OnBeforePhase`; test `Resources.cls` (deletes the exported file from its fixture). Model: [locmsgs.rs](../../Verus_verification/consolidated/iter56/locmsgs.rs) `reload_replaces_domain`, `readonly_root_fails`, `uninstall_keeps_messages` ([handoff](../../uplift/consolidated/iter56/HANDOFF.md)). Retarget the bindings' owner to this anchor and drop "(proposed)".

C register entries:

- **C:D-next-1, Medium · Decision: Open: "A module with a `.LOC` resource cannot be loaded from a read-only folder".** Found by: source review of `ResourceProcessor/LocalizationExport.cls::OnBeforePhase` (Activate export to Module.Root), iteration56 model `readonly_root_fails`, control `readonly-tolerated`. Reproduced by: E2E reproduced, receipt `iter56/native/20261116T182256528947Z`. `load` of `loc56ro` from a read-only copy fails with `Cannot open file '<root>/localize/D56RMsgs.xml'` and installs nothing (r1). Passing neighbour: a read-only module without `.LOC` loads (n1). The module asks for no export, and installing should not need to write into its source. A repair would skip or tolerate the export when the root is not writable, or write it only on request. Unchecked.
- **C:D-next-2, Low · Decision: Open: "Uninstall leaves a module's localized messages in the message dictionary".** Found by: iteration56 native u1 against the Clean phase deleting only the document, model `uninstall_keeps_messages`, control `uninstall-clears-messages`. Reproduced by: E2E reproduced, same receipt. After `uninstall loc56`, `^IRIS.Msg("D56Dom","en")` still holds A and C, while the document and module record are gone (u1). Passing neighbour: load and reload replace the domain correctly (l1, l2). This contradicts lifecycle Clean removing the module's resources. A repair would delete the document's domains on Clean, unless another module owns them. Unchecked.
