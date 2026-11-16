# Iteration 39: editing a module manifest as its `.ZPM` document

Batch 5, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter39/document.rs), [cases and controls](../../../Verus_verification/consolidated/iter39/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). New owner: [module.identity.document](../../../ontology/concepts/modules.g5.md#module.identity.document).

## Gap

`StudioDocument/*` (45 members) had no binding, and the ontology said only that the manifest is a `<Name>.ZPM` document. Editors save manifests through this document API.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| e1 save version 1.1.0 | record 1.1.0, text 1.1.0 | updated | updated at save |
| e2 save malformed XML | refused; record 1.1.0; stored text empty | unchanged | first: unchanged; after explore2: text empty |
| e3 save `<Name>m39other</Name>`, 1.2.0 | success; record m39 at 1.2.0; text says m39other | refused | refused (mismatch guard), then refuted; round 2: accepted under the old name |
| e4 save 0.5.0 | accepted | unspecified | accepted |
| e5 save empty text | record version 0.0.1+snapshot, resource kept | unspecified | same |
| e6 delete the document | record gone, class still compiled | unspecified | same |

Explore1 left e3 ambiguous because it reused version 1.1.0. Explore2 added text readers and exposed the empty text after e2. Explore3 used 1.2.0 for e3 after both outcomes were re-predicted. All three logs are kept.

## Evidence

- Formal: `document.rs` 6 verified, 0 errors. The generic lemmas cover any existing record: a valid edit updates, a refused save empties the text, another name is accepted under the old name, an empty save resets the version, and delete keeps classes. Five controls were each rejected: `stream-clear-rolled-back`, `name-guard-effective`, `downgrade-refused`, `empty-clears-resources` and `delete-removes-classes`.
- Native: [receipt](native/20261116T174319930857Z/observations.json), 84 fields, all gated.
- Replay: 75 fields in Verus ([receipt](replay/receipt.json)). Flips of `e2.doctext.empty`, `e3.version` and `e3.doctext.other` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter39/run.py` (12 s).

## Findings

- [C:D44](../BUGS.md#c-d44), Medium, E2E reproduced: a refused save empties the stored document text.
- [C:D45](../BUGS.md#c-d45), Low, E2E reproduced: a manifest naming a different module is saved under the old name.
- Both are discoveries from this search.

## Reach

[Project map](project-map.json): 452/1216 (iteration38: 449, none lost). Newly reached: `StudioDocument/Module.cls::Save`, `::Exists` and `StudioDocument/Abstract.cls::Load`. Strengthened with this role: `ImportStream`, `UpdatePersistentFromStream` and `Delete`, which were already linked through module-version and uninstall paths. That gives 3 new plus 3 strengthened.

## Limits

Editing goes through `%RoutineMgr` in the server process, not an actual IDE. Compile-time document handling (`OnCompileDocument`), new-document creation, locking and source-control hooks are not assessed. XML/XSLT parsing is a provider. No ObjectScript body is verified directly.
