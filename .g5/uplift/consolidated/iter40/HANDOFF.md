# Iteration 40: UnitTest resource processor from load to test phase to uninstall

Batch 6 lane, product `571e04a1` unchanged. Not sealed; the main thread integrates the results. [Model](../../../Verus_verification/consolidated/iter40/unittests.rs), [cases and controls](../../../Verus_verification/consolidated/iter40/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [build.test](../../../ontology/features/build-and-publish.g5.md#build.test). Selection and failure counting are already owned by build.test.selection and build.test.outcome and are not re-modelled.

## Predictions and observations

Owner predictions come from build.test and the processor DESCRIPTION ("For modules in development mode, the unit tests are always loaded and are not deleted after they are run"). They were written before the bodies were read. Source predictions were added after.

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| n0 `load t40` (not dev) | test classes absent | absent | absent |
| n1 `test t40 -only` | test package ran once; verify package did not run; both test packages defined and the test package compiled afterwards | tests deleted after the run | both remain |
| n2 `uninstall t40` | test classes removed | — | — |
| d0 `load t40 -dev` | both test packages defined, not compiled | loaded | loaded |
| d1 `test t40 -only` | test package ran; classes remain | remain | remain |
| d2 `uninstall t40` | test classes removed | — | — |

Exploratory run 1 matched every source prediction, and `native_expected` was copied from it. The verify phase itself was not run because it needs package/verify-namespace traversal, which is excluded scope. Phase="verify" appears only as the resource the test phase must skip.

## Evidence

- Formal: `unittests.rs` 4 verified, 0 errors. Five controls were each rejected: `delete-checked-before-load`, `non-dev-reload-loads-tests`, `verify-resource-runs-in-test`, `load-only-own-package` and `uninstall-keeps-tests`. Run 1 failed before proving because the model named `tests.rs` produced an executable that collided with the repository's `tests` directory. It is kept in `attempts/`, and the model was renamed.
- Native: [receipt](native/20261116T174722753053Z/observations.json), 48 fields, all gated.
- Replay: 42 fields ([receipt](replay/receipt.json)). Flips of `n1.tests`, `n1.vtests` and `d0.testscompiled` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter40/run.py` (13 s).

## Discrepancy

In `OnPhase`, the "package already has classes, so don't delete" check runs after `LoadTestDirectory` has loaded those very classes. `/nodelete` is therefore always passed, and the deletion branch is unreachable for both Package and Class resources. In a non-developer module, `test` leaves every package under the test directory loaded, including a verify-only package that never ran, until uninstall. That contradicts the DESCRIPTION's developer-mode-only retention. The consequence is limited: code persists in the namespace until uninstall removes it.

## Reach candidates (against the iter39 map)

Newly reached: `ResourceProcessor/Test.cls::OnAfterPhase` and `::%OnNew`. Already linked and gaining this role: `OnPhase`, `TestsShouldRun`, `Test/Manager.cls::LoadTestDirectory`, `OnResolveChildren` and `Lifecycle/Base.cls::%Clean`.

## Limits

One shared test directory with two packages. The Class form, Suite/Case selectors (owned), failing tests (owned), the sync test paths and the verify phase are not run here. The `%UnitTest.Manager` run and `/nodelete` handling are provider premises. No ObjectScript body is verified directly.

## Proposed owner text

- **`build.test`, added bullets:** "A module's test classes are loaded at load time only in developer mode. The test phase loads every class under the test resource's directory, then compiles and runs the resource's package." "As built, the test phase never deletes the test classes it loaded, even for a module not in developer mode, so they stay in the namespace, including verify-only packages that did not run, until the module is uninstalled (defect C:D-next)." Metadata: Model [unittests.rs](../../Verus_verification/consolidated/iter40/unittests.rs) `test_leaves_test_code`, `non_dev_load_has_no_tests`, `uninstall_clears`.
- **C register entry (Low · Decision: Open):** "C:D-next: The test phase keeps test classes loaded in a module that is not in developer mode." Found by: source review of `ResourceProcessor/Test.cls::OnPhase` (existence check after `LoadTestDirectory`) and iteration40 model `test_leaves_test_code` with control `delete-checked-before-load`. Reproduced by: E2E reproduced, receipt `iter40/native/20261116T174722753053Z`. After `load t40` (not dev) and `test t40 -only`, `T40.Tests.Basic` and the never-run `T40.VTests.Basic` are defined and the former compiled (n1). The passing neighbour is the plain load with no tests (n0); `uninstall` removes them (n2). Repair, unchecked: take the existence check before loading the test directory.
