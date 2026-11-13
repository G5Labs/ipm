# Testing, packaging and publishing

Module authors test, package and ship modules with lifecycle phase commands. Each runs its [phase chain](../concepts/lifecycle.g5.md#lifecycle.chain) on a module already loaded in the namespace. These commands are not recorded in [history](history.g5.md#history.actions).

## Definitions

- **Package**: a `.tgz` archive of a module, ready to install elsewhere.
- **VERIFY namespace**: a throwaway namespace named `<namespace>-VERIFY` that `verify` installs into.

## Statements

<a id="build"></a>
[I · build]

IPM gives module authors commands to test, package, verify, publish and deploy a loaded module.

<a id="build.test"></a>
[I · build.test]

`test <module>` runs the module's unit tests in the current namespace. Test-scoped resources and dependencies load for this phase only.

<a id="build.test-output"></a>
[I · build.test-output]

`test` and `verify` print a summary of results.

- `-quiet` shows only the summary and failures. `-verbose` adds per-method results when used with `-format`.
- `-format json`, `yaml` or `toon`, or the `TestReportFormat` setting, picks the summary format.
- `-output-file` writes full results. The file extension picks the format: `.json`, `.yaml`, `.toon` or `.xml` for JUnit.

<a id="build.package"></a>
[I · build.package]

`package <module>` exports the module and bundles it into a `.tgz` archive.

- `-path` sets where the archive goes, and only the archive is left there.
- `-export-deps` controls whether dependencies are included, and `-export-python-deps` adds Python dependencies.
- Test-scoped resources are left out unless `-include-tests` or `<IncludeTests>` is set.

<a id="build.verify"></a>
[I · build.verify]

`verify <module>` packages the module, installs the package in the VERIFY namespace with journaling off, and runs its integration tests there. With `-only`, the VERIFY namespace loads the module from its folder on disk instead.

<a id="build.publish"></a>
[I · build.publish]

`publish <module>` packages the module and sends it to a repository that allows publishing.

- `-repo` picks the repository, and `-use-external-name` publishes under the external name.
- `unpublish [<repo>/]<module> <version|all>` deletes a published package. It asks first unless `-force` is given, and the registry must still allow it.
- `unpublish` fails when the module or version does not exist or the registry refuses.

<a id="build.makedeployed"></a>
[I · build.makedeployed]

`makedeployed <module>` turns resources marked `Deploy="true"` into compiled-only code. `-recurse` does the same for the module's dependencies.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). These blocks are outside the formal scope.

### build

- human-provided
- user-facing-behavior
- Children: [test](#build.test), [test output](#build.test-output), [package](#build.package), [verify](#build.verify), [publish](#build.publish), [makedeployed](#build.makedeployed).

Sources: `Main.cls:32-58`, `:120-162`; `wiki/00` L3-83. Inventory: CAT-11, CAT-24, CAT-32, CAT-33, CAT-34, CAT-39; A2 1.2 rows 8-11, 13, 14.

### build.test

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:48`; `wiki/00` L36; CHANGELOG 0.10.11 (`#1023`, `#1005`). Inventory: CAT-02, CAT-11; A2 1.2 row 8, OP-LOAD-29. Tests: `Scopes.cls` `TestScopedClassLoadsOnTest`, `UnitTestDirectory.cls`.

### build.test-output

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:76-79`; `wiki/02` L1194-1246; CHANGELOG 0.10.7 (`#971`). Inventory: CAT-39. Test: `TestOutputFormat.cls`. `wiki/02` L1207 and L1386 document `-output-format`, but the flag is `-format`, and the `.csv` example is rejected.

### build.package

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:135-144`; `wiki/02` L803-838; `wiki/03` L720-727; CHANGELOG 0.10.4 (`#611`, `#686`), 0.10.7 (`#843`). Inventory: CAT-32; A2 1.2 row 9. Tests: `IncludeTests.cls`, `StaticFileExport.cls`, `GlobalsPackaging.cls`.

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

### build.makedeployed

- human-provided
- user-facing-behavior
- Expands: [Build](#build).

Sources: `Main.cls:189-195`; `wiki/05`; CHANGELOG 0.10.4 (`#815`). Inventory: CAT-34; A2 1.2 row 13.
