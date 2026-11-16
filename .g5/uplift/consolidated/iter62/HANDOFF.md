# Iteration 62: Interoperability lookup-table resources

Batch 13 lane, product `571e04a1` unchanged. Not sealed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter62/interopres.rs), [cases and controls](../../../Verus_verification/consolidated/iter62/cases.json), [bindings](bindings.json), [ledger](working-ledger.json).

Setup: inside the disposable container, USER was made interoperability-enabled from `%SYS` with `%EnsembleMgr.EnableNamespace("USER",1)`; the image's IPM in USER was used unchanged.

| Case | Observed | Predicted (owner / source) |
|---|---|---|
| a1 `I62A.LUT` in `src/i14y/` | entries a=1, b=2 | loaded / loaded |
| m1 `I62M.LUT` in `src/misc/` (no i14y) | k=7 | — / loaded via misc probe |
| x1 `Directory="tables"` | k=8 | loaded / loaded |
| f1 file directly under `src/` | k=9 | — / loaded via the Document "" fallback |
| a2 edited file (a=10, b removed, c=3), load again | a=10, b absent, c=3 | replaced / unknown |
| a3, m3 uninstall | tables and `.LUT` document removed; others intact | removed / removed |
| h1 custom `I62H.HL7` schema | load fails: #5844 "No user defined document class in this namespace" | loaded |

No new discrepancy. h1 is a provider boundary: IRIS Community has no HL7 document class, so HL7/X12/ESD resources need an IRIS for Health instance; recorded, not replayed.

Evidence: `interopres.rs` 4 verified; controls `misc-not-probed`, `no-root-fallback`, `reload-merges`, `uninstall-keeps` each rejected. [Native receipt](native/20261116T183351877491Z/observations.json), 92 gated fields; 11 replayed; flips of `a2.I62A.b`, `a3.I62A.a`, `f1.I62F.k` rejected. Explore1/explore2 logs kept under `attempts/`. Command: `python3 .g5/Verus_verification/consolidated/iter62/run.py`.

Reach candidate: `ResourceProcessor/Default/Interoperability.cls::DirectoryGet` (newly reached); `Document.OnPhase` and `Lifecycle/Base.cls::%Clean` strengthened.

Limits: HL7, X12 and ESD documents not exercised (provider absent); identifier translation (`-`, `(`, `)`) in file names not tested.

## Proposed owner text

New statement in `.g5/ontology/concepts/modules.g5.md`, after `module.processor.deepsee`, with the processor list gaining "`.LUT`, `.HL7`, `.X12` and `.ESD` interoperability documents ([interoperability documents](#module.processor.interoperability))":

```
<a id="module.processor.interoperability"></a>
[D · module.processor.interoperability]

Interoperability documents such as `.LUT` lookup tables load from `<SourcesRoot>/i14y/<name>.xml`, or from `<SourcesRoot>/misc/` when only that folder exists, unless the resource declares a Directory. With neither folder present, a file directly under SourcesRoot is also found.

- Loading a changed lookup table replaces the whole table, so a removed key disappears, and uninstall removes the table.
- HL7, X12 and ESD documents need the matching document class in the namespace; on an instance without it the load fails.
```

Metadata: `agent`, `user-facing-behavior`, Expands [Resource processors](#module.processor). Sources: `ResourceProcessor/Default/Interoperability.cls::DirectoryGet`; `Lifecycle/Base.cls::GetDefaultResourceProcessor`. Model: [interopres.rs](../../Verus_verification/consolidated/iter62/interopres.rs) `directory_neighbors`, `reload_replaces`, `uninstall_clears` ([handoff](../../uplift/consolidated/iter62/HANDOFF.md)).

No register entry (existing registers have no interoperability-resource entry; nothing new to file).
