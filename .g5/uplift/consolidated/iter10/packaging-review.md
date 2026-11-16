# UnitTest packaging producer and archive boundary

Builder-selected continuation of the read-only survey of product `IncludeTests.cls`, pinned to product `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. No evaluator-private cases were consulted. This report records the scope implementation; the parent iteration harness owns final native results and merged receipts.

## Predictions frozen before implementation inspection

The existing canonical `build.package` says that test resources are omitted by default and included by the flag or manifest. It does not state the explicit-zero override. The inherited `playbook-uplift/models/verus/scopes.rs::{export_phases,exported,res_witness_explicit_zero_overrides_manifest}` already represents selection from a preclassified `Res.scope`, including explicit zero. It does not derive the UnitTest processor's returned scope or show a directory resource reaching the archive. This is strengthened formal correspondence and a newly represented producer/consumer boundary, not a new discovery of flag precedence.

| Case | Inputs | Predicted selected file in archive | Predicted bytes match original |
|---|---|---|---|
| pkg-default | Phase=test, manifest unset, flag absent | absent | false (file absent) |
| pkg-flag | Phase=test, include flag | present | true |
| pkg-manifest | Phase=test, manifest IncludeTests=1 | present | true |
| pkg-zero | Phase=test, manifest=1, explicit IncludeTests=0 | absent | false (file absent) |
| pkg-verify-flag | Phase=verify, include flag | present | true |
| pkg-verify-default | Phase=verify, manifest unset, flag absent | absent | false (file absent) |

Each package command is expected to succeed and retain module.xml. Native `Packaging.cls` copies the product test-output-format fixture into a distinct temporary folder for each row, changes only the named inputs, and uses the actual package command. Python tarfile inspects the actual .tgz member and compares its SHA-256 with the original copied test file. It accepts the two ordinary tar spellings with or without a leading `./`. It does not infer copying from directory existence or a successful command.

The direct scope companion changes the persisted resource's declared Scope to verify while its UnitTest Phase remains test. GetResolvedReferences with only Test must refuse the path at the resource filter; only Verify must refuse at the returned-scope filter; both phases must admit it. The top-level Scope metadata must not appear as a resource. The companion restores declared Scope before packaging. Direct ResolveChildren omits the path and retains its returned Scope metadata.

## Formal and implementation roles

| Formal function | Exact source member | Audited role |
|---|---|---|
| unit_children | ResourceProcessor/Test.cls::OnResolveChildren (300); Storage/ResourceReference.cls::ResolveChildren (287) | Returned Scope is derived from UnitTest.Phase. Direct path key absent. Namespace document enumeration is outside this path-only model. |
| has_scope, references | Storage/Module.cls::HasScope (834), ::GetResolvedReferences (1349) | Apply both scopes; strip top-level metadata; re-add the accepted path. |
| export_phases | Lifecycle/Base.cls::%Export (1283) | Explicit IncludeTests parameter wins; otherwise read manifest. Add Test and Verify when selected. This selector is inherited semantics. |
| path_has_processor, copied, directory_prefix | Lifecycle/Base.cls::ExportSingleModule (2030) | Re-added path has scalar module identity and no Processor metadata. It reaches the unhandled fallback; normalize the directory boundary and copy its file payloads. |
| test_path_export_handled | ResourceProcessor/Test.cls::OnExportItem (363) | Separately observed defensive path API returns OK/unhandled. It is not claimed to be invoked for the re-added entry. |
| package, archive | Lifecycle/Module.cls::%Package (134); General/Archive.cls::Create (5) | Fresh export directory, archive selected content and manifest; native archive membership and bytes discharge only the sampled provider correspondence. |
| request/consumer | Main.cls::ModuleAction (2830); actual tarfile reader in native fixture | Command parameters and -only PhaseList enter the pipeline; the final archive reader distinguishes omission from inclusion. |

Source locators are scoped inspection bindings, not whole-method implementation proofs. No new trusted declarations were added. All proofs concern hand-written Verus specifications. Direct ObjectScript proofs: zero.

## Checks and limits

The final direct baseline verified six units with zero errors. Eleven altered rules were assertion-rejected: ignore either scope, replace Phase producer, leak metadata, omit path re-addition, mark the defensive path handled, suppress fallback copy, ignore include flag, ignore explicit zero, erase payload and fail to normalize the directory prefix. The exact mutations are in scope-cases.json. Direct commands and logs are in native/packaging/proof-attempts/receipt.json. Five initial baseline attempts are retained, including finite-set constructor/type and proof-computation limitations; they were resolved without assumptions.

This model intentionally covers path entries and file-map content. It does not prove XML parsing, case folding, raw list decoding, actual namespace document enumeration, generic path normalization, archive encoding/decoding, directory-copy atomicity or missing/failed providers. File-map keys are normalized provider outputs. directory_prefix represents the trailing separator added for the copy branch; a separate witness refuses the /testsSibling neighbor for /tests. The native matrix uses the normalized /tests/ fixture path, so the no-trailing-separator witness remains source/formal coverage only.

Native returned-error behavior is owned by the parent's errors.rs work. This scope model is the successful-resolver/successful-filesystem domain. General PKG/document exports, dependencies, generated resources, symlinks, archive traversal and arbitrary path syntax remain open. Existing IncludeTests suite success is inherited evidence, not a newly run suite. No native run was launched by this subagent; the merged iteration run must establish final native agreement and source hashes.

## Retained native misses and strengthened final matrix

The parent's first combined native run rejected the original archive observations. Raw headers used repeated separators, such as `tests/Test/Output/Format//AfterAllReturn.cls`, so a literal tarfile membership lookup reported absence even when the product's extractor would recover the file. The parent retains those failed observations and logs the raw headers. Final Packaging uses the actual `General.Archive.Extract` and checks the recovered path and bytes. The formal file map describes that normalized consuming filesystem result; it does not prove literal header-name identity or the extractor.

The continuation also exposed a real missing input producer: when a reload omits IncludeTests, the existing stored true value survives. `StudioDocument/Module.cls::UpdatePersistentFromStream` (26) opens the existing module, clears its collections but not IncludeTests, then imports using `Storage/Module.cls::XMLNew` (1642), which returns that same object. Absence therefore differs from explicit false. A command-only IncludeTests=0 changes package selection without clearing the stored true value.

The original final verify-default request omitted the manifest element after earlier true manifests; its fresh-default prediction was wrong for that history. This miss is retained in the failed native run. The final seven-request matrix makes the history explicit:

| Label | Incoming manifest | Command override | Stored after refresh | Selected file after extraction |
|---|---|---|---|---|
| pkg-default | absent, starting stored false | absent | false | absent |
| pkg-flag | absent | true | false | present |
| pkg-manifest | true | absent | true | present |
| pkg-zero | true | false | true | absent |
| pkg-sticky | absent | absent | true | present |
| pkg-verify-flag | explicit false | true | false | present |
| pkg-verify-default | explicit false | absent | false | absent |

`refresh_include`, `refresh_and_package` and `matrix_prior` derive these persisted states without taking selection as an input. Explicit false in the two final rows is an independent reset variant, not a claim that the earlier omission cleared anything. Fresh/omitted versus true/omitted and true/explicit-false are formal neighbors; the matrix also carries command-only zero through to the subsequent omitted refresh.

Final direct verification now has nine verified units, zero errors and fourteen assertion-rejected controls. The added controls reject clearing on omission, ignoring explicit false and writing a command override into stored IncludeTests. `native/packaging/proof-attempts/receipt-sticky-final.json` pins this model. Earlier direct receipts remain historical, with their original source hashes. Additional failed proof attempts were resolved using explicit sequence/map facts; no proof assumption was added. The parent owns the final integrated replay against native observations and must report its actual result separately.
