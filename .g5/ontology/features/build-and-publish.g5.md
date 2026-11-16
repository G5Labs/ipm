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

`test <module>` runs the module's unit tests in the current namespace. Test-scoped resources and dependencies load for this phase only. Resource targets and selector precedence follow [test selection](#build.test.selection).

- A module's test classes are loaded at load time only in developer mode. The test phase loads every class under the test resource's directory, then compiles and runs the resource's package.
- As built, the test phase never deletes the test classes it loaded, even for a module not in developer mode. They stay in the namespace, including verify-only packages that did not run, until the module is uninstalled (defect C:D48).


<a id="build.test.selection"></a>
[D · build.test.selection]

A test resource must declare exactly one of Class and Package. The requested phase is matched without regard to case against the declared phase list, which accepts only lowercase `test` and `verify`. A Class resource ignores Suite and Case path selectors.

- Only the request is lowercased. A stored `TEST`, which validation rejects, would never match. This is why an unvalidated declaration is outside the selection rule, not an exception to it.

- A Package resource uses Suite if that key is defined, otherwise Case. Suite slashes become dots. Even a defined empty Suite takes precedence over Case and leaves path filtering off.
- Package selection compares raw prefixes in either direction, so package `A` can be selected by `AB`. Changed-class ownership is stricter: it accepts `A` or `A.` followed by a class name, and rejects `AB`.
- These are two different readers. Selecting a test resource does not establish that sync assigns a changed class to it, and converting a file path does not itself establish class ownership.


<a id="build.test.outcome"></a>
[D · build.test.outcome]

A test command fails when any test it ran failed. Each case error, each method error and each failed assertion counts as one failure, and the error reads "N failure(s)."

- Every in-scope test resource runs, even after an earlier one fails, and the command reports one combined failure. [`sync -test`](developer-tools.g5.md#devtools.sync.test-stop) stops at the first failing resource instead.
- Failures are counted after the summary is printed. A report file from `-output-file` is written before that, so a `.json` or `.xml` file exists for a failing run. An unsupported extension such as `.csv` fails the command after the tests ran, even when all of them passed, and writes no file.
- The red `FAILED` lines appear only when no summary format is active. They list the same rows that are counted.
- A CI job learns the result only from the [command status and exit code](#build.test.outcome.exit).


<a id="build.test.outcome.not-fatal"></a>
[D · build.test.outcome.not-fatal]

`-DUnitTest.FailuresAreFatal=0` makes a run with failing tests succeed. The summary still says "Some tests FAILED", but no failure is counted, no `FAILED` lines are printed and the process exit code stays 0.


<a id="build.test.outcome.empty"></a>
[D · build.test.outcome.empty]

A Package test resource whose package has no test classes prints a warning and is skipped. The command can still succeed, so a CI job can pass with no tests run, for example after a package rename.

- A Suite selector that names a folder that does not exist is different. The command fails with "Directory name ... is invalid" before any test runs.
- Whether an empty test package should fail the run is an [open question](../../Verus_verification/open_questions.md#build.test.outcome.empty.policy).

- Working assumption: keep the warning and the passing status. The alternative is to fail, or to fail only when a run executes no tests at all. Either alternative changes current green builds whose test package is intentionally empty.


<a id="build.test.outcome.exit"></a>
[D · build.test.outcome.exit]

A CI job sees a test failure only through the command status. `%IPM.Main:Shell(command, 1)` ends the process with exit code 1 when the command fails. `zpm "<command>"` and `Shell(command)` without the second argument end with exit code 0 even when tests fail.

- `zpm "<command>",1` does not work as a CI form: it exits 0 after a failure. The `$zpm()` function form raises an error instead of returning the status. Both are owned by [shell entry forms](../root.g5.md#ipm.shell.forms).


<a id="build.test.outcome.nested"></a>
[D · build.test.outcome.nested]

Test results gather in a per-process list. Each test resource counts only the results added after it started, then removes them.

- A test command run inside a test method, such as `verify` inside `verify`, counts and removes only its own results. The outer run's earlier results stay in the list. The outer test learns about the inner failure from the inner command's status, not from the list.
- A reader that started from an empty list removes the whole list. When failures are not fatal, or when the report step fails first, nothing is removed. A later test command in the same process starts after those leftover entries and ignores them.
- `FAILED` lines are printed only for results after the start, and empty entries are skipped.


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


<a id="build.test-output.run-identity"></a>
[D · build.test-output.run-identity]

The device summary and exported report are intended to describe the selected test run. In the pinned implementation they can identify different runs when a test method runs another test command.

- The device summary uses the last tracked completed run, falling back to the latest stored run only when tracking has no ID. File export omits that ID and uses the numerically latest stored run instead.
- A passing or failing run alone has the same ID in both readers. In the checked nested sequence, the failing outer run is allocated before a passing child. The child completes first; the outer finishes last and supplies the tracked device ID. The device reports the outer failures and the command fails, but the outer report file contains the passing child's class and results.
- This is [C:D31](../../Verus_verification/bugs_issues.md), not an accepted change to the reporting intent. Supplying the outer ID explicitly to the export API writes the outer results in the checked comparison. The product call site remains unchanged. Other formats and general nested lifecycle/provider failures are not newly qualified here.


<a id="build.test-output.counted-rows"></a>
[D · build.test-output.counted-rows]

The summary's assertion count includes every numeric result entry under a method, regardless of its action name. An entry passes when its stored status is nonzero. A method's own status is read separately; an aggregate failure message without an error category does not add a separate error row.

- The checked IRIS build appends a passing `LogMessage` duration entry. A method with two passing assertion calls therefore reports three passing assertions. One passing and two failing calls report four entries, two failed.
- These counts describe stored result entries, not a guarantee that every entry came from an assertion call. Assertion execution, ID allocation and duration logging remain provider boundaries qualified by the pinned native observations.


<a id="build.package"></a>
[I · build.package]

`package <module>` exports the module and bundles it into a `.tgz` archive. A deployed-item helper currently has a known [error propagation gap](#build.package.deployed-status).

- `-path` sets where the archive goes, and only the archive is left there.
- `-export-deps` controls whether dependencies are included, and `-export-python-deps` adds Python dependencies.
- Test-scoped resources are left out unless their phase is requested or test inclusion adds it. `-include-tests` enables inclusion; an explicit `-DIncludeTests=0` overrides a true `<IncludeTests>` value.


<a id="build.package.deployed-status"></a>
[D · build.package.deployed-status]

As built, the document exporter overwrites the deployed-item helper's returned error with success before deciding whether to export source. A successful source fallback can therefore hide that earlier error; a source-export error still fails the operation.

- The actual inherited exporter was exercised with a controlled failing helper override and a real successful source export. This confirms C:D19's local status suppression, not an ordinary default-helper failure in a complete package command.
- The later Studio project check is an independent reader. It removes missing items and may return success with fewer items, so it is not proof that a discarded item or helper error will be caught. Source files, final project contents and command status must be assessed separately.
- The proposed repair is to preserve the helper status. It has not been applied to the product. An error after a provider's partial write may leave that write; no compensation is inferred.


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

`makedeployed <module>` turns eligible resources selected by the [deployment rules](../concepts/modules.g5.md#module.deployed) into compiled-only code. A class deployment parameter can override `Deploy="true"`. `-recurse` also considers the module's dependencies.


<a id="build.publish.partial-effects"></a>
[D · build.publish.partial-effects]

A failed publish or unpublish does not establish that the registry is unchanged. As built, ORAS writes or deletes tags one at a time; a later failure can follow completed earlier calls.

- ORAS cleanup can fail after the push succeeds. The lifecycle can publish before its optional local version export fails; an in-memory version assignment is not proof that the stored row or manifest was updated.
- Remote publishing and deletion require credentials and exactly HTTP 200 for success, while ORAS permission probing currently reports enabled before attempting the operation. An HTTP error or other returned error may follow external effects.
- These source-backed contracts use explicit provider results. Actual multi-tag failure, cleanup errors and postpublication local export have not been exercised end to end in this iteration.


<a id="build.test-output.counts"></a>
[D · build.test-output.counts]

As built, a failed test method counts as failed even if every assertion passed. A case's nonempty hook error adds one failed method to the summary.

- A blank error category hides the aggregate error message; it does not change a failed status to passed. JSON files contain the full result tree; device JSON always has a summary and selects method details or failures from its detail and verbose settings.
- `verify` creates its NoJournal load parameter when it creates a fresh VERIFY namespace. Reuse and SkipLoad take other setup paths; the plan does not establish namespace cleanup after errors or all serialized formats.


<a id="build.archive.empty-file"></a>
[D · build.archive.empty-file]

Extracting a named regular archive entry creates a file even when its size is zero. The next entry is still read; an empty filename in the header, rather than a zero payload size, ends the archive.


<a id="build.copy.overlap"></a>
[D · build.copy.overlap]

Directory copying refuses a target that is the source, contains it or lies inside it, before deleting or copying anything. A separate sibling directory is allowed and receives nested files when the filesystem copy succeeds.

The Python check also detects aliases through device and inode ancestry. If Python or file IDs are unavailable, the fallback compares normalized directory paths, so alias detection is not guaranteed there.


<a id="build.publish.external-identity"></a>
[D · build.publish.external-identity]

Publishing with `-use-external-name` changes the registry key to the module's external name. It does not rename the module inside the archived manifest.

Installing by that registry key therefore installs the original internal module name. The default publish mode uses the internal name for the key as well. Empty or invalid chosen publish names are refused before publication.


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

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). The inherited scope excluded these blocks; iteration 2 adds only the explicitly linked scoped contracts.

### build

- human-provided
- user-facing-behavior
- Children: [test](#build.test), [test output](#build.test-output), [package](#build.package), [verify](#build.verify), [publish](#build.publish), [makedeployed](#build.makedeployed).

Sources: `Main.cls:32-58`, `:120-162`; `wiki/00` L3-83. Inventory: CAT-11, CAT-24, CAT-32, CAT-33, CAT-34, CAT-39; A2 1.2 rows 8-11, 13, 14.

### build.test

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Build](#build).
- Children: [selection](#build.test.selection), [outcome](#build.test.outcome).

Sources: `Main.cls:48`; `wiki/00` L36; CHANGELOG 0.10.11 (`#1023`, `#1005`). Inventory: CAT-02, CAT-11; A2 1.2 row 8, OP-LOAD-29. Tests: `Scopes.cls` `TestScopedClassLoadsOnTest`, `UnitTestDirectory.cls`.


Iteration40 test-code lifetime: [unittests.rs](../../Verus_verification/consolidated/iter40/unittests.rs) `test_leaves_test_code`, `non_dev_load_has_no_tests`, `uninstall_clears` ([handoff](../../uplift/consolidated/iter40/HANDOFF.md)).

Findings: [The test phase keeps test classes loaded in a module that is not in developer mode](../../Verus_verification/bugs_issues.md#c-d48)

State machines: [unittests · iter40](../../Verus_verification/models/iter40-unittests/manifest.json)

### build.test.selection

- design-verified

- agent
- Expands: [Testing](#build.test).

Sources: `ResourceProcessor/Test.cls::{TestsShouldRun,%OnValidateObject,OwnsClassName,OnSync}`. Verus `public_repairs.rs::{test_selected,valid_test_target,owns_class,test_neighbors}`. Native Class ignore, phase case, raw-prefix, dot-boundary and empty-Suite contrasts are in [the iteration 3 index](../../uplift/consolidated/iter3/INDEX.json). Relative-path conversion remains a separate source-only boundary; no complete test-run or sync lifecycle proof.

Iteration 4 phase-domain repair: the phase admission producer is `Test.cls` property `Phase As ListOfOptions(VALUELIST=",test,verify")`. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{admitted,phase_enabled,lemma_admitted_agree,phase_neighbors}` proves that iteration 3's folded reading equals the as-built reading on admitted declarations and differs on a stored `TEST`. Native P-stored-test (valid, runs for a `TEST` request) and P-stored-TEST (invalid, does not run) on a real installed Test processor. Controls `stored-phase-folded` and `request-not-lowercased` are rejected.

State machines: [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### build.test.outcome

- design-verified

- agent
- user-facing-behavior
- Expands: [Testing](#build.test).
- Children: [not fatal](#build.test.outcome.not-fatal), [empty package](#build.test.outcome.empty), [exit code](#build.test.outcome.exit), [nested runs](#build.test.outcome.nested).

Direct decision: `ResourceProcessor/Test.cls::{OnPhase,ReportTestResults}`, `Test/Manager.cls::GetAllTestsStatus`. Readers and callers: `Lifecycle/Base.cls::%Test` (accumulates every resource with `$$$ADDSC`), `Storage/Module.cls::ExecutePhases`, `Test/Manager.cls::OutputFailures`, `Test/Abstract.cls::BuildResultTree`. Verus [test_outcome.rs](../../Verus_verification/consolidated/iter4/test_outcome.rs) `{failures,resource_ok,all_ok,lifecycle_invoked,sync_invoked,report_written,outcome_neighbors}`. Native T1 (4 failures, later resources still run), T5 (`.csv` after passing tests), T6 (`.json` written, still failing), replayed as Verus assertions. Contrast with `sync -test` is source-only for the throw (`General/Sync/Pipeline.cls::SyncRunTests`). Discovery in iteration 4; this also answers the public panel's two-processor aggregation question.

State machines: [test outcome · iter4](../../Verus_verification/models/iter4-test-outcome/manifest.json)

### build.test.outcome.not-fatal

- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).

Direct decision: `ResourceProcessor/Test.cls::ReportTestResults` (`FailuresAreFatal` defaults to 1). Native T2 and E4 (exit 0 with terminate flag). Verus `resource_ok` with `Opts.fatal=false`; control `not-fatal-still-fails` rejected.

### build.test.outcome.empty

- uncertain
- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).
- Children: [Should a declared test package with no test classes fail `test` and `verify`?](../../Verus_verification/open_questions.md#build.test.outcome.empty.policy).

Direct decision: `ResourceProcessor/Test.cls::OnPhase` (no-class warning and skip; Suite folder load before it). Native T1 (warning, run continues) and T4 (`ERROR #5007` for a named missing suite folder). Verus `load_fails`, `resource_ok`; controls `empty-package-fails` and `missing-suite-dir-skipped` rejected. A Suite folder that exists but holds no classes is source-only.

Open questions: [Should a declared test package with no test classes fail `test` and `verify`?](../../Verus_verification/open_questions.md#build.test.outcome.empty.policy)

### build.test.outcome.exit

- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).

Direct decision: `Main.cls::Shell` (`pTerminateOnError` calls `$system.Process.Terminate($job,1)`); caller `Main.cls::LanguageExtensions` (`ZPM` passes its arguments through and discards the status). Native E1 exit 1, E2 exit 0 for a passing run, E3 exit 0 without the flag. Verus `exit_code`; control `exit-ignores-terminate-flag` rejected. Related: [ipm.shell](../root.g5.md#ipm.shell).

### build.test.outcome.nested

- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).

Direct decision: `Test/Manager.cls::{GetAllTestsStatus,OutputFailures,SaveResult}`; writers `ResourceProcessor/Test.cls::{OnPhase,OnSyncRunTests}` (initialize and record the start). Native A1-A4 use real failing and passing result indexes in seeded slots; D1-D3 count printed `FAILED` lines from a start slot with and without an empty middle slot. Verus `{get_all_status,displayed_lines,lemma_nested_keeps_parent,lemma_suffix_ignores_prefix,accumulator_neighbors}`; controls `nested-clears-parent`, `nested-counts-parent-slots` and `display-includes-start-slot` rejected. An actual `verify` nested inside a running test method was not executed; the nesting is reproduced at the reader with seeded slots. Answers public cases I2E09 and I3F02.

### build.test-output

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Build](#build).
- Children: [run identity](#build.test-output.run-identity), [counted rows](#build.test-output.counted-rows).

Sources: `Main.cls:76-79`; `wiki/02` L1194-1246; CHANGELOG 0.10.7 (`#971`). Inventory: CAT-39. Test: `TestOutputFormat.cls`. `wiki/02` L1207 and L1386 document `-output-format`, but the flag is `-format`, and the `.csv` example is rejected.

Extension Verus: [thin_surfaces.rs::report_result](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.


Iteration47 formats: [formats.rs](../../Verus_verification/consolidated/iter47/formats.rs) `fixture_rendering`, `summaries_agree`, `error_method_two_toon_rows` ([handoff](../../uplift/consolidated/iter47/HANDOFF.md)).

Findings: [YAML and TOON test reports change assertion message text](../../Verus_verification/bugs_issues.md#c-d53)

State machines: [formats · iter47](../../Verus_verification/models/iter47-formats/manifest.json)

### build.test-output.run-identity

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](#build.test-output).

Verus [report_identity.rs](../../Verus_verification/consolidated/iter17/report_identity.rs) models persistent run identity, completion tracking, the two reader choices and explicit-ID export. [Iteration17](../../uplift/consolidated/iter17/HANDOFF.md) records ordinary Shell producers, actual raw rows, file/device JSON and the valid alone neighbors. The earlier seeded accumulator model is retained; it did not establish this actual allocation schedule. No direct implementation-body proof or general provider guarantee is claimed.

Findings: [C:D31 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d31). The linked entry qualifies the caller, reader and providers.

Findings: [C:S21 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s21).

State machines: [report identity · iter17](../../Verus_verification/models/iter17-report-identity/manifest.json)

### build.test-output.counted-rows

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](#build.test-output).

Verus [report_identity.rs](../../Verus_verification/consolidated/iter17/report_identity.rs) represents raw row decoding and derived summary/failure counts. The pre-native two/three count prediction was refuted by the provider's duration row; the original fixture and raw log remain in [iteration17](../../uplift/consolidated/iter17/HANDOFF.md). Timing text and serialization/persistence internals are explicit boundaries.

State machines: [report identity · iter17](../../Verus_verification/models/iter17-report-identity/manifest.json)

### build.package

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Build](#build).

Sources: `Main.cls:135-144`; `wiki/02` L803-838; `wiki/03` L720-727; CHANGELOG 0.10.4 (`#611`, `#686`), 0.10.7 (`#843`). Inventory: CAT-32; A2 1.2 row 9. Tests: `IncludeTests.cls`, `StaticFileExport.cls`, `GlobalsPackaging.cls`.

### build.package.deployed-status

- agent
- Expands: [Packaging](#build.package).

Sources: `ResourceProcessor/Default/Document.cls::{OnExportItem,OnExportDeployedItem}`; `Storage/Module.cls::AddItemToProject`; `Lifecycle/Base.cls::{ExportSingleModule,%Export}`; native `%Studio.Project.Check` documentation and observations. Verus `resources.rs::{export_item,final_export,check_project,overwritten_error_and_final_reader,project_check_prunes_missing}`. The final checker status remains an explicit provider result; only its missing-item pruning is modeled. [C:D19](../../Verus_verification/bugs_issues.md) and [repair proposal C:S13](../../Verus_verification/design_simplification.md) retain that limit.

Findings: [C:D19 — partial reproduced](../../Verus_verification/bugs_issues.md#c-d19). The linked entry qualifies the caller, reader and providers.

Findings: [C:S13 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s13).

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

### build.publish.partial-effects

- design-verified

- agent
- user-facing-behavior
- Expands: [Publishing](#build.publish).

Sources: `Repo/Oras/PublishService.cls::PushOras`, `::DeleteTags`, `::PublishModule`; `Repo/Remote/PublishService.cls`; `Lifecycle/Base.cls::%Publish`. [Actual state and scoped assertions](../../Verus_verification/consolidated/iter2/registry.rs); [source review and withheld mappings](../../uplift/consolidated/iter2/inventory/new-binding-review.json). Remote effects are provider inputs, not implementation proofs. Simplification proposal: expose which external stages completed; no compensation design has been accepted.

State machines: [registry · iter2](../../Verus_verification/models/iter2-registry/manifest.json)

### build.test-output.counts

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](#build.test-output).

Sources: `Test/Abstract.cls::BuildResultTree`, `::GetSummary`; `Test/JsonOutput.cls`; `Lifecycle/Base.cls::%Verify`. [Verus state and reader projections](../../Verus_verification/consolidated/iter2/readers.rs); [actual seeded native readers](../../uplift/consolidated/iter2/native/receipt.json). Full serialization, namespace lifecycle and all format branches remain open.

State machines: [readers · iter2](../../Verus_verification/models/iter2-readers/manifest.json)

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

### build.publish.external-identity

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [build.publish](#build.publish).

Sources and readers: `Lifecycle/Base.cls::%Publish` selects `Remote.ModuleInfo.Name` but exports `Module.XMLExportToStream` unchanged → `Repo/Oras/PublishService.cls::PublishModule` → `Utils/Module.cls::LoadModuleReference` and `Storage/Module.cls::NameExists` reader. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/webapps.rs): `published_key`, `archive_manifest_name`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [webapps · iter8](../../Verus_verification/models/iter8-webapps/manifest.json)

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
