# Testing, packaging and publishing

Module authors test, package and ship modules with lifecycle phase commands. Each runs its [phase chain](../architecture/lifecycle.g5.md#lifecycle.chain) on a module already loaded in the namespace. These commands are not recorded in [history](history.g5.md#history.actions).

## Definitions

- **Package**: a `.tgz` archive of a module, ready to install elsewhere.
- **VERIFY namespace**: a throwaway namespace named `<namespace>-VERIFY` that `verify` installs into.

## Release commands

<a id="build"></a>
[I · build]

IPM gives module authors commands to test, package, verify, publish and deploy a loaded module.

## Running tests and reading the report

<a id="build.test"></a>
[I · build.test]

`test <module>` runs the module's unit tests in the current namespace. Test-scoped resources and dependencies load for this phase only. Resource targets and selector precedence follow [test selection](../concepts/testing-policy.g5.md#build.test.selection).

- A module's test classes are loaded at load time only in developer mode. The test phase loads every class under the test resource's directory, then compiles and runs the resource's package.
- As built, the test phase never deletes the test classes it loaded, even for a module not in developer mode. They stay in the namespace, including verify-only packages that did not run, until the module is uninstalled (defect C:D48).

<a id="build.test-output"></a>
[I · build.test-output]

`test` and `verify` print a summary of results.

- `-quiet` shows only the summary and failures. `-verbose` adds per-method results when used with `-format`.
- `-format json`, `yaml` or `toon`, or the `TestReportFormat` setting, picks the summary format.
- `-output-file` writes full results. The file extension picks the format: `.json`, `.yaml`, `.toon` or `.xml` for JUnit.

- The output-file export happens before summary rendering. An unsupported summary format can therefore return an error after writing valid JSON results; the same passing tests with a supported summary format return success.

- Command status and report-file existence are separate observations. Verification uses a different namespace lifecycle, so a test reporting check does not prove verify cleanup.

- All formats print the same method and assertion counts, and JSON keeps assertion and error text exactly. As built, YAML and TOON files drop carriage returns from that text, and YAML descriptions read back with a trailing newline and without the first line's leading spaces (defect C:D53).
- TOON writes one row per assertion. A method with an error and no assertions gets an error row and a status row.

## Building an installable package

<a id="build.package"></a>
[I · build.package]

`package <module>` exports the module and bundles it into a `.tgz` archive. A deployed-item helper currently has a known [error propagation gap](packaging-deployed-code.g5.md#build.package.deployed-status).

- `-path` sets where the archive goes, and only the archive is left there.
- `-export-deps` controls whether dependencies are included, and `-export-python-deps` adds Python dependencies.
- Test-scoped resources are left out unless their phase is requested or test inclusion adds it. `-include-tests` enables inclusion; an explicit `-DIncludeTests=0` overrides a true `<IncludeTests>` value.

<a id="build.package.test-path"></a>
[D · build.package.test-path]

A UnitTest resource's Phase also sets its returned child scope. Export requires both its declared Scope and that returned scope to match the requested phases; a matching test directory is copied into the package with its file contents.

- Test inclusion adds Test and Verify to the export phases. An explicit IncludeTests parameter, including zero, overrides the manifest for that addition; it does not remove a phase already requested.
- Direct child enumeration does not return the test directory itself. Export adds that directory after the scope checks. The checked archives omit it by default, include it by flag or manifest, and omit it again with the explicit zero override.

<a id="build.package.include-refresh"></a>
[D · build.package.include-refresh]

Reloading an existing module with IncludeTests omitted keeps its stored value. An explicit false clears it; a command-only zero override disables test inclusion for that command and leaves the stored value unchanged.

- A later package command with no override therefore includes tests again when the stored value is still true. A fresh module with the field omitted starts false.
- Whether omission should reset this field is an [open policy question](../../Verus_verification/open_questions.md#build.package.include-refresh.policy).

- Working assumption: preserve the checked behavior and use an explicit false to clear it. Resetting on omission would make a removed declaration disable test packaging, but would change existing reload behavior.

## Extracting and copying package contents

<a id="build.archive.empty-file"></a>
[D · build.archive.empty-file]

Extracting a named regular archive entry creates a file even when its size is zero. The next entry is still read; an empty filename in the header, rather than a zero payload size, ends the archive.

<a id="build.copy.overlap"></a>
[D · build.copy.overlap]

Directory copying refuses a target that is the source, contains it or lies inside it, before deleting or copying anything. A separate sibling directory is allowed and receives nested files when the filesystem copy succeeds.

The Python check also detects aliases through device and inode ancestry. If Python or file IDs are unavailable, the fallback compares normalized directory paths, so alias detection is not guaranteed there.

## Verifying a package in a temporary namespace

<a id="build.verify"></a>
[I · build.verify]

`verify <module>` packages the module, installs the package in the VERIFY namespace with journaling off, and runs its integration tests there. With `-only`, the VERIFY namespace loads the module from its folder on disk instead.

## Publishing and naming a release

<a id="build.publish"></a>
[I · build.publish]

`publish <module>` packages the module and sends it to a repository that allows publishing.

- `-repo` picks the repository, and `-use-external-name` publishes under the external name.
- `unpublish [<repo>/]<module> <version|all>` deletes a published package. It asks first unless `-force` is given, and the registry must still allow it.
- `unpublish` fails when the module or version does not exist or the registry refuses.

<a id="build.publish.external-identity"></a>
[D · build.publish.external-identity]

Publishing with `-use-external-name` changes the registry key to the module's external name. It does not rename the module inside the archived manifest.

Installing by that registry key therefore installs the original internal module name. The default publish mode uses the internal name for the key as well. Empty or invalid chosen publish names are refused before publication.

<a id="build.publish.partial-effects"></a>
[D · build.publish.partial-effects]

A failed publish or unpublish does not establish that the registry is unchanged. As built, ORAS writes or deletes tags one at a time; a later failure can follow completed earlier calls.

- ORAS cleanup can fail after the push succeeds. The lifecycle can publish before its optional local version export fails; an in-memory version assignment is not proof that the stored row or manifest was updated.
- Remote publishing and deletion require credentials and exactly HTTP 200 for success, while ORAS permission probing currently reports enabled before attempting the operation. An HTTP error or other returned error may follow external effects.
- These source-backed contracts use explicit provider results. Actual multi-tag failure, cleanup errors and postpublication local export have not been exercised end to end in this iteration.

## Related rules on other pages

<a id="build.test.selection"></a>
- Which tests take part — [build.test.selection](../concepts/testing-policy.g5.md#build.test.selection).

<a id="build.test.outcome"></a>
<a id="build.test.outcome.not-fatal"></a>
<a id="build.test.outcome.empty"></a>
<a id="build.test.outcome.nested"></a>
- What makes a command fail — [build.test.outcome](../concepts/testing-policy.g5.md#build.test.outcome), [build.test.outcome.not-fatal](../concepts/testing-policy.g5.md#build.test.outcome.not-fatal), [build.test.outcome.empty](../concepts/testing-policy.g5.md#build.test.outcome.empty), [build.test.outcome.nested](../concepts/testing-policy.g5.md#build.test.outcome.nested).

<a id="build.test.outcome.exit"></a>
- What a CI job sees — [build.test.outcome.exit](../concepts/testing-policy.g5.md#build.test.outcome.exit).

<a id="build.test-output.run-identity"></a>
<a id="build.test-output.counted-rows"></a>
<a id="build.test-output.counts"></a>
- Which results a report describes — [build.test-output.run-identity](../concepts/testing-policy.g5.md#build.test-output.run-identity), [build.test-output.counted-rows](../concepts/testing-policy.g5.md#build.test-output.counted-rows), [build.test-output.counts](../concepts/testing-policy.g5.md#build.test-output.counts).

<a id="build.package.deployed-status"></a>
- Reporting a failed project export — [build.package.deployed-status](packaging-deployed-code.g5.md#build.package.deployed-status).

<a id="build.makedeployed"></a>
- Choosing which source to omit — [build.makedeployed](packaging-deployed-code.g5.md#build.makedeployed).

## Metadata

### build

- human-provided
- user-facing-behavior
- Children: [test](#build.test), [test output](#build.test-output), [package](#build.package), [verify](#build.verify), [publish](#build.publish), [makedeployed](packaging-deployed-code.g5.md#build.makedeployed).

Sources: `Main.cls:32-58`, `:120-162`; `wiki/00` L3-83. Inventory: CAT-11, CAT-24, CAT-32, CAT-33, CAT-34, CAT-39; A2 1.2 rows 8-11, 13, 14.

### build.test

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Build](#build).
- Children: [selection](../concepts/testing-policy.g5.md#build.test.selection), [outcome](../concepts/testing-policy.g5.md#build.test.outcome).

Sources: `Main.cls:48`; `wiki/00` L36; CHANGELOG 0.10.11 (`#1023`, `#1005`). Inventory: CAT-02, CAT-11; A2 1.2 row 8, OP-LOAD-29. Tests: `Scopes.cls` `TestScopedClassLoadsOnTest`, `UnitTestDirectory.cls`.


Iteration40 test-code lifetime: [unittests.rs](../../Verus_verification/consolidated/iter40/unittests.rs) `test_leaves_test_code`, `non_dev_load_has_no_tests`, `uninstall_clears` ([handoff](../../uplift/consolidated/iter40/HANDOFF.md)).

Findings: [The test phase keeps test classes loaded in a module that is not in developer mode](../../Verus_verification/bugs_issues.md#c-d48)

State machines: [unittests · iter40](../../Verus_verification/models/iter40-unittests/manifest.json)

### build.test-output

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Build](#build).
- Children: [run identity](../concepts/testing-policy.g5.md#build.test-output.run-identity), [counted rows](../concepts/testing-policy.g5.md#build.test-output.counted-rows).

Sources: `Main.cls:76-79`; `wiki/02` L1194-1246; CHANGELOG 0.10.7 (`#971`). Inventory: CAT-39. Test: `TestOutputFormat.cls`. `wiki/02` L1207 and L1386 document `-output-format`, but the flag is `-format`, and the `.csv` example is rejected.

Extension Verus: [thin_surfaces.rs::report_result](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.


Iteration47 formats: [formats.rs](../../Verus_verification/consolidated/iter47/formats.rs) `fixture_rendering`, `summaries_agree`, `error_method_two_toon_rows` ([handoff](../../uplift/consolidated/iter47/HANDOFF.md)).

Findings: [YAML and TOON test reports change assertion message text](../../Verus_verification/bugs_issues.md#c-d53)

State machines: [formats · iter47](../../Verus_verification/models/iter47-formats/manifest.json)

### build.package

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:135-144`; `wiki/02` L803-838; `wiki/03` L720-727; CHANGELOG 0.10.4 (`#611`, `#686`), 0.10.7 (`#843`). Inventory: CAT-32; A2 1.2 row 9. Tests: `IncludeTests.cls`, `StaticFileExport.cls`, `GlobalsPackaging.cls`.

### build.package.test-path

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Packaging](#build.package).

[Formal contract](../../Verus_verification/consolidated/iter10/scope.rs) derives the phase scope, selection, path entry and selected bytes at the archive boundary. [Producer/writer/reader evidence](../../uplift/consolidated/iter10/INDEX.md). XML/path decoding, class enumeration, filesystem copy and tar provider behavior retain explicit premises; no general archive parser or filesystem proof is claimed.

State machines: [scope · iter10](../../Verus_verification/models/iter10-scope/manifest.json)

### build.package.include-refresh

- design-verified

- uncertain
- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Packaging](#build.package).
- Children: [Should a reloaded manifest that omits IncludeTests keep the previous value or reset it to false?](../../Verus_verification/open_questions.md#build.package.include-refresh.policy).

[Formal contract](../../Verus_verification/consolidated/iter10/scope.rs): `refresh_include`, `refresh_and_package`, `refresh_neighbors`. Parsed optional input is distinct from explicit false; the stored field is carried to package selection and output bytes. [Evidence](../../uplift/consolidated/iter10/INDEX.md).

Open questions: [Should a reloaded manifest that omits IncludeTests keep the previous value or reset it to false?](../../Verus_verification/open_questions.md#build.package.include-refresh.policy)

State machines: [scope · iter10](../../Verus_verification/models/iter10-scope/manifest.json)

### build.archive.empty-file

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Packaging](#build.package).

Sources: `Utils/FileBinaryTar.cls::{ExtractStream,ExtractTo}` → `General/Archive.cls::Extract`. Native S6-06 packages the product zero-byte-file fixture, extracts it and checks the empty and following populated files. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `end_header`, `creates_file`, `next_header`; validated header parsing and file-provider success are premises. Compression, checksums and extended headers are outside this proof.

State machines: [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)

### build.copy.overlap

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Packaging](#build.package).

Sources: `Utils/File.cls::{DirectoriesOverlap,DirectoriesOverlapPython,CopyDir}` → FileCopy/CSP copying callers. Native S6-03 checks refusal leaves source nested files intact, and sibling copy contains the nested file. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `below`, `overlap`, `copy_ok`. Path normalization, device/inode ancestry and provider copy status are explicit premises; inode ancestry itself is not proved. [Index](../../uplift/consolidated/iter6/INDEX.md).

State machines: [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)

### build.verify

- human-provided
- user-facing-behavior
- Expands: [Build](#build).

Sources: `Main.cls:52`; `wiki/00` L71-81; CHANGELOG 0.10.4 (`#563`). Inventory: CAT-33; A2 1.2 row 10, OP-LOAD-28 (`Lifecycle/Base.cls:1461-1578`). Observed: the module installed into the VERIFY namespace gets no history record, while its dependencies get records there. The VERIFY namespace is cleared of modules after each test.

### build.publish

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:153-162`, `:197-208`; CHANGELOG 0.10.0 (`#716`, `#769`), 0.10.5 (`#951`), 0.10.11 (`#1115`). Inventory: CAT-24; A2 1.2 rows 11, 14. Tests: `PublishExternalName.cls`, `PublishModuleWithDeps.cls`. The help line "there may only be one of these per namespace" (`Main.cls:57`) may be stale.

### build.publish.external-identity

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [build.publish](#build.publish).

Sources and readers: `Lifecycle/Base.cls::%Publish` selects `Remote.ModuleInfo.Name` but exports `Module.XMLExportToStream` unchanged → `Repo/Oras/PublishService.cls::PublishModule` → `Utils/Module.cls::LoadModuleReference` and `Storage/Module.cls::NameExists` reader. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/webapps.rs): `published_key`, `archive_manifest_name`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [webapps · iter8](../../Verus_verification/models/iter8-webapps/manifest.json)

### build.publish.partial-effects

- design-verified

- agent
- user-facing-behavior
- Expands: [Publishing](#build.publish).

Sources: `Repo/Oras/PublishService.cls::PushOras`, `::DeleteTags`, `::PublishModule`; `Repo/Remote/PublishService.cls`; `Lifecycle/Base.cls::%Publish`. [Actual state and scoped assertions](../../Verus_verification/consolidated/iter2/registry.rs); [source review and withheld mappings](../../uplift/consolidated/iter2/inventory/new-binding-review.json). Remote effects are provider inputs, not implementation proofs. Simplification proposal: expose which external stages completed; no compensation design has been accepted.

State machines: [registry · iter2](../../Verus_verification/models/iter2-registry/manifest.json)

Iteration75 neighbours: [r8b_remote.rs](../../Verus_verification/coverage-20261117/iter75/r8b_remote.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).
