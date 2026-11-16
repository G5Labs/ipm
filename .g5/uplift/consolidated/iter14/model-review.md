# Protection model lane: provisional variant and native miss

The first protection.rs prototype derives exact-target marker OR target-equals/ancestor-of a protected root over normalized lowercase whole-segment paths. It represents marker addition/removal between copy and Clean, directory/file source branches, and exact versus nested synthetic markers. Normalization, root enumeration, file layout and successful OS calls are named provider boundaries. It includes source-qualified KeepOnUninstall and Overlay branches without claiming new native checks for those inputs.

The initial copy rule inferred that a requested delete-first operation actually clears the directory. It proved six units with no errors. The exact source and log are preserved as model-lane/expected-clear.rs, baseline-1.log and expected-clear-receipt.json.

The parent's native run 20261115T182900393138Z refuted this inference: unprotected Overlay=0 directory loads retained the existing sentinel; the nested-marker directory also retained its synthetic child marker. Clean removed the ordinary and nested directories. No failed expectation is withdrawn. At this checkpoint the model lane has not independently run or reproduced native evidence, and final provider semantics are awaiting the parent's calibration.

The final model should distinguish the producer's requested delete-first argument from the operation performed by the actual CopyDir wrapper and provider. A provider outcome parameter that simply accepts any surviving file would make this case vacuous. Instead transcribe the wrapper's actual argument positions and select a named provider behavior confirmed by direct valid-neighbor calibration, preserving the expected-clear variant as a rejected alternative. Classifier and Clean are independent of the successful-copy preservation/clearing distinction and should retain their specific obligations.

## Calibrated Unix copy boundary

The parent reports direct calibration in native stamp `20261115T183029469530Z`: CopyDir with its fifth flag 0 and 1 both succeeded, retained unrelated sentinel files and overwrote the same-named payload with source contents. The pinned compiled `%Library.File.CopyDir` signature is `(pSource,pTarget,pOverlay=0,&pCreated,pDeleteBeforeCopy=0)`. The Unix wrapper supplies third argument 1 and passes its `pDeleteFirst` into the fifth argument. The final model therefore has `delete_first` (the processor's requested flag), `unix_args` (the wrapper's actual argument positions), and `unix_copy` (the calibrated provider effect). It never supplies the desired resulting filesystem as a premise.

`unix_copy` preserves unrelated exact markers, nested markers and sentinels for either fifth-argument value, and writes the selected distinct payload. This is a sharp rule for the measured successful Unix domain. The flag can affect how a matching destination file is replaced; this presence-only abstraction does not assert equal inodes, permissions, intermediate states or failure behavior. The old expected-clear source remains intact as a refuted alternative. The current `unix-delete-before-clears-tree` control rejects that alternative explicitly.

The source's Overlay property documentation says false fully replaces the destination and removes other files. That inferred intent must remain separate from the pinned Unix behavior; preserving it is the parent's C:D29 disposition. The model change does not authorize changing the requirement. Windows explicitly removes a target directory in the wrapper's source, but no Windows machine or native result is claimed here.

## Final API and qualification

Files tracks `dir`, `payload`, `sentinel`, `exact_marker`, `nested_marker`. SourceKind is Directory or File after the actual NormalizeNames producer. The generic classifier is `protected(target,roots,files)`: exact-target marker OR equality/ancestry of a supplied protected root over whole segments. Native root normalization and database enumeration qualify provider inputs; they are not body proofs. The classifier does not recursively inspect nested unconfigured markers.

The selected states are `ordinary_loaded/after`, `marked_loaded/after`, `added_ready/after`, `removed_ready/after`, `nested_loaded/after`, `file_loaded/after`, with `before(marker,nested)` for the initial states. `added_ready` and `removed_ready` are independent marker-writer steps between copy and Clean, so the final protection decision is not cached at load. The file case deletes the distinct copied payload while preserving its enclosing marked directory and unrelated files. No real database file is modeled or modified.

Final lifecycle predictions retain sentinel/nested files on successful Unix copy. Unprotected directory Clean then removes them; a newly added exact marker protects Clean, while a removed exact marker no longer does. KeepOnUninstall and Overlay=true have source-qualified formal branches; the new full native worlds all use Keep=false, Overlay=false. The sibling-path classifier witness is source/formal coverage only. Direct file copies assume the selected payload is distinct from sentinel and marker paths.

The final direct model has seven verified units, zero errors and fourteen semantic controls rejected. Exact records are under `model-lane/final-controls/receipt.json`; no assume/admit/external_body was added. `protection-cases.json` proposes 123 replay fields (three classifier observations and six tracked fields over twenty snapshots); content bytes, module rows, command statuses, raw normalization/case and database-list observations remain native qualification. Parent integration owns final native/replay/source-hash verification.

## Scoped source roles

- Utils/File.cls::IsProtectedDirectory: exact marker, static roots and configured database roots; `protected`, `ancestor_of_root`.
- Utils/File.cls::DatabaseDirectories and ::NormalizeForComparison: explicit enumeration/normalization ports, not new verified bodies.
- ResourceProcessor/FileCopy.cls::NormalizeNames: observed directory/file branch input; no generic raw-path parser claim.
- ResourceProcessor/FileCopy.cls::DoCopy: derives requested replacement flag, suppresses it for protected directories; `delete_first`.
- Utils/File.cls::CopyDir: Unix passes overlay=1 and fifth per-file deletion argument; `unix_args`, `unix_copy`. No general filesystem compensation, OS error or Windows claim.
- ResourceProcessor/FileCopy.cls::OnPhase: KeepOnUninstall, single-file deletion and current directory-protection decision; `clean`.
- Storage/Module.cls::ExecutePhases, Lifecycle/Base.cls::%Clean and installed module deletion: sampled full-caller qualification owned by parent. No ObjectScript implementation-body proof is claimed.
