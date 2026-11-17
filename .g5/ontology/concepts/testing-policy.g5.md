# Test selection and results

Which tests run under `test`, `verify` or `sync -test`? The rules below cover selection, failure status and the results a caller can read.

## Definitions

A test resource names a class or a package of test classes. A run is one invocation of the test runner. Nested test commands can produce several runs in one process.

## Which tests take part

<a id="module.resource.scope"></a>
[I · module.resource.scope]

A resource with `Scope="test"` stays out of ordinary installs. It loads and compiles in the Test phase, and packaging leaves it out unless tests are included.

<a id="build.test.selection"></a>
[D · build.test.selection]

A test resource must declare exactly one of Class and Package. The requested phase is matched without regard to case against the declared phase list, which accepts only lowercase `test` and `verify`. A Class resource ignores Suite and Case path selectors.

- Only the request is lowercased. A stored `TEST`, which validation rejects, would never match. This is why an unvalidated declaration is outside the selection rule, not an exception to it.

- A Package resource uses Suite if that key is defined, otherwise Case. Suite slashes become dots. Even a defined empty Suite takes precedence over Case and leaves path filtering off.
- Package selection compares raw prefixes in either direction, so package `A` can be selected by `AB`. Changed-class ownership is stricter: it accepts `A` or `A.` followed by a class name, and rejects `AB`.
- These are two different readers. Selecting a test resource does not establish that sync assigns a changed class to it, and converting a file path does not itself establish class ownership.

## What makes a command fail

<a id="build.test.outcome"></a>
[D · build.test.outcome]

A test command fails when any test it ran failed. Each case error, each method error and each failed assertion counts as one failure, and the error reads "N failure(s)."

- Every in-scope test resource runs, even after an earlier one fails, and the command reports one combined failure. [`sync -test`](#devtools.sync.test-stop) stops at the first failing resource instead.
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

<a id="build.test.outcome.nested"></a>
[D · build.test.outcome.nested]

Test results gather in a per-process list. Each test resource counts only the results added after it started, then removes them.

- A test command run inside a test method, such as `verify` inside `verify`, counts and removes only its own results. The outer run's earlier results stay in the list. The outer test learns about the inner failure from the inner command's status, not from the list.
- A reader that started from an empty list removes the whole list. When failures are not fatal, or when the report step fails first, nothing is removed. A later test command in the same process starts after those leftover entries and ignores them.
- `FAILED` lines are printed only for results after the start, and empty entries are skipped.

<a id="devtools.sync.test-stop"></a>
[D · devtools.sync.test-stop]

`sync -test` runs changed test classes one test resource at a time and stops at the first resource whose tests fail. Later test resources do not run in that sync. [`test`](#build.test.outcome) runs every resource instead.

- File changes are already recorded before tests run, so a test failure does not undo the sync.
- A test resource whose phase list lacks `test` is skipped and reported as verify-only.

## What a CI job sees

<a id="build.test.outcome.exit"></a>
[D · build.test.outcome.exit]

A CI job sees a test failure only through the command status. `%IPM.Main:Shell(command, 1)` ends the process with exit code 1 when the command fails. `zpm "<command>"` and `Shell(command)` without the second argument end with exit code 0 even when tests fail.

- `zpm "<command>",1` does not work as a CI form: it exits 0 after a failure. The `$zpm()` function form raises an error instead of returning the status. Both are owned by [shell entry forms](../root.g5.md#ipm.shell.forms).

## Which results a report describes

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

<a id="build.test-output.counts"></a>
[D · build.test-output.counts]

As built, a failed test method counts as failed even if every assertion passed. A case's nonempty hook error adds one failed method to the summary.

- A blank error category hides the aggregate error message; it does not change a failed status to passed. JSON files contain the full result tree; device JSON always has a summary and selects method details or failures from its detail and verbose settings.
- `verify` creates its NoJournal load parameter when it creates a fresh VERIFY namespace. Reuse and SkipLoad take other setup paths; the plan does not establish namespace cleanup after errors or all serialized formats.

## Metadata

### module.resource.scope

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Resources](data-policy.g5.md#module.resource).

Sources: CHANGELOG 0.10.11 (`#1023`), 0.10.7 (`#843`); `ResourceProcessor/Default/Document.cls:195-198`. Inventory: CAT-02, A1 seed 7; OP-LOAD-24. Tests: `Scopes.cls`, `IncludeTests.cls`. Open elsewhere: `Scope="verify"` is deferred (PR #1319).

### build.test.selection

- design-verified

- agent
- Expands: [Testing](../features/build-and-publish.g5.md#build.test).

Sources: `ResourceProcessor/Test.cls::{TestsShouldRun,%OnValidateObject,OwnsClassName,OnSync}`. Verus `public_repairs.rs::{test_selected,valid_test_target,owns_class,test_neighbors}`. Native Class ignore, phase case, raw-prefix, dot-boundary and empty-Suite contrasts are in [the iteration 3 index](../../uplift/consolidated/iter3/INDEX.json). Relative-path conversion remains a separate source-only boundary; no complete test-run or sync lifecycle proof.

Iteration 4 phase-domain repair: the phase admission producer is `Test.cls` property `Phase As ListOfOptions(VALUELIST=",test,verify")`. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{admitted,phase_enabled,lemma_admitted_agree,phase_neighbors}` proves that iteration 3's folded reading equals the as-built reading on admitted declarations and differs on a stored `TEST`. Native P-stored-test (valid, runs for a `TEST` request) and P-stored-TEST (invalid, does not run) on a real installed Test processor. Controls `stored-phase-folded` and `request-not-lowercased` are rejected.

State machines: [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### build.test.outcome

- design-verified

- agent
- user-facing-behavior
- Expands: [Testing](../features/build-and-publish.g5.md#build.test).
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

### build.test.outcome.nested

- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).

Direct decision: `Test/Manager.cls::{GetAllTestsStatus,OutputFailures,SaveResult}`; writers `ResourceProcessor/Test.cls::{OnPhase,OnSyncRunTests}` (initialize and record the start). Native A1-A4 use real failing and passing result indexes in seeded slots; D1-D3 count printed `FAILED` lines from a start slot with and without an empty middle slot. Verus `{get_all_status,displayed_lines,lemma_nested_keeps_parent,lemma_suffix_ignores_prefix,accumulator_neighbors}`; controls `nested-clears-parent`, `nested-counts-parent-slots` and `display-includes-start-slot` rejected. An actual `verify` nested inside a running test method was not executed; the nesting is reproduced at the reader with seeded slots. Answers public cases I2E09 and I3F02.

### devtools.sync.test-stop

- design-verified

- agent
- user-facing-behavior
- Expands: [Sync](../features/developer-tools.g5.md#devtools.sync).

Direct decision: `General/Sync/Pipeline.cls::SyncRunTests` (`$$$ThrowOnError` per resource). Contrast: `Lifecycle/Base.cls::%Test` uses `$$$ADDSC`. Verus [test_outcome.rs](../../Verus_verification/consolidated/iter4/test_outcome.rs) `sync_invoked` versus `lifecycle_invoked`; controls `sync-runs-all-resources` and `lifecycle-stops-at-first-failure` rejected. Native evidence covers only the `test` side (T1); the sync side is source-only.

State machines: [test outcome · iter4](../../Verus_verification/models/iter4-test-outcome/manifest.json)

### build.test.outcome.exit

- agent
- user-facing-behavior
- Expands: [Test outcome](#build.test.outcome).

Direct decision: `Main.cls::Shell` (`pTerminateOnError` calls `$system.Process.Terminate($job,1)`); caller `Main.cls::LanguageExtensions` (`ZPM` passes its arguments through and discards the status). Native E1 exit 1, E2 exit 0 for a passing run, E3 exit 0 without the flag. Verus `exit_code`; control `exit-ignores-terminate-flag` rejected. Related: [ipm.shell](../root.g5.md#ipm.shell).

### build.test-output.run-identity

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](../features/build-and-publish.g5.md#build.test-output).

Verus [report_identity.rs](../../Verus_verification/consolidated/iter17/report_identity.rs) models persistent run identity, completion tracking, the two reader choices and explicit-ID export. [Iteration17](../../uplift/consolidated/iter17/HANDOFF.md) records ordinary Shell producers, actual raw rows, file/device JSON and the valid alone neighbors. The earlier seeded accumulator model is retained; it did not establish this actual allocation schedule. No direct implementation-body proof or general provider guarantee is claimed.

Findings: [C:D31 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d31). The linked entry qualifies the caller, reader and providers.

Findings: [C:S21 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s21).

State machines: [report identity · iter17](../../Verus_verification/models/iter17-report-identity/manifest.json)

### build.test-output.counted-rows

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](../features/build-and-publish.g5.md#build.test-output).

Verus [report_identity.rs](../../Verus_verification/consolidated/iter17/report_identity.rs) represents raw row decoding and derived summary/failure counts. The pre-native two/three count prediction was refuted by the provider's duration row; the original fixture and raw log remain in [iteration17](../../uplift/consolidated/iter17/HANDOFF.md). Timing text and serialization/persistence internals are explicit boundaries.

State machines: [report identity · iter17](../../Verus_verification/models/iter17-report-identity/manifest.json)

### build.test-output.counts

- design-verified

- agent
- user-facing-behavior
- Expands: [Test output](../features/build-and-publish.g5.md#build.test-output).

Sources: `Test/Abstract.cls::BuildResultTree`, `::GetSummary`; `Test/JsonOutput.cls`; `Lifecycle/Base.cls::%Verify`. [Verus state and reader projections](../../Verus_verification/consolidated/iter2/readers.rs); [actual seeded native readers](../../uplift/consolidated/iter2/native/receipt.json). Full serialization, namespace lifecycle and all format branches remain open.

State machines: [readers · iter2](../../Verus_verification/models/iter2-readers/manifest.json)
