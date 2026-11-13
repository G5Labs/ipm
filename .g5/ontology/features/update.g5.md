# Updating

`update` moves an installed module to a newer version and runs the update steps its author wrote. It uses the [shared loading path](load.g5.md#load.core) and adds the ApplyUpdateSteps phase.

## Definitions

- **Update step**: a method a module author writes to move data or settings from one version to the next.
- **Update package**: the class package a module names in `<UpdatePackage>` to hold its update steps.

## Statements

<a id="update"></a>
[I · update]

`update <module> [<version>]` updates an installed module to a newer version and runs its update steps. Without a version, it updates to the latest available. `-path <archive>` updates from a local archive instead of a repository.

- History records it as `update`.

<a id="update.refusal"></a>
[I · update.refusal]

IPM refuses an update when:

- The module is not installed. IPM points the user to `install` or `load`.
- `-DPermitDowngrade` is given.
- The `-path` archive holds a different module from the one named.
- The target version is [older than the installed one](load.g5.md#load.refusal.downgrade).

Like a [refused load](load.g5.md#load.refusal), a refused update keeps no module data.

<a id="update.dev-mode"></a>
[I · update.dev-mode]

`update` does not pass developer mode on to dependencies.

<a id="update.requirements"></a>
[I · update.requirements]

`update` checks version requirements against the versions modules will have after the update, not the ones installed now. Shared dependencies that must move in lock-step with each other update together instead of failing.

<a id="update.steps"></a>
[I · update.steps]

A module that declares `<UpdatePackage>` provides update steps.

- Each `V<major>` class in that package extends `%IPM.General.Update.VersionBase` and lists its step methods in order.
- Steps must be safe to run again, and they report errors by throwing.
- `update` runs the steps needed to move from the current version to the new one.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). `update` is in the formal scope where it uses the shared loading path and the history log. Update steps themselves are outside it.

### update

- human-provided
- user-facing-behavior
- test-coverage
- Children: [refusals](#update.refusal), [developer mode](#update.dev-mode), [requirements](#update.requirements), [update steps](#update.steps).

Sources: `Main.cls:164-187`; `wiki/02` L1341-1371; `wiki/08`; CHANGELOG 0.10.4 (`#550`). Inventory: CAT-15; OP-LOAD-21 (`Main:Update`). Tests: `History.cls` `TestUpdate`; `Update.cls`. The help example has no description of `-path` behavior beyond overriding the version.

### update.refusal

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: messages `Main.cls:4415-4423`, `Utils/Module.cls:1269-1275`. Inventory: CAT-15; OP-LOAD-21, OP-LOAD-10. Tests: `Update.cls` `TestUpdateOfUninstalledModule`, `TestUpdateNonMatchingTarballFails` (status only), `TestUpdateToOlderVersion`; suite PASS 15 of 15 (`20261113T182433Z-suite-verify-Update`). Observed: the first two refusals happen before a history record exists ([unlogged refusals](history.g5.md#history.record.unlogged-refusals)). Confirmed product defect D16: `update A -path` with module B's archive crashes with `<INVALID OREF>LoadNewModule+67` before the mismatch check, because IPM opens and uses the installed module under B's name while B is not installed (`Utils/Module.cls:1207`, `:1232`). Under `-DNoTransaction=1` the record reads `Committed=1`, and B's archive stays unpacked under `<ModuleRoot>`. No module data is kept, which refutes OBS-L6 (stage C cases S3 and S3N1; S3N2, a matching archive, updates; `20261113T204152Z-stagec`).

### update.dev-mode

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: CHANGELOG 0.10.5 (`#1036`). Inventory: CAT-15, INT-LOAD-07; OP-LOAD-20. Test: `Update.cls` `TestDevModePropagation`. Suspected gap, not run natively by B2: `update <m> -dev` from a registry leaves the module itself in normal mode (OBS-L4).

### update.requirements

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: CHANGELOG 0.10.7 (`#1119`, `#430`); issue #430; PR #1136. Inventory: CAT-15, CAT-18, A1 seed 5. Tests: `Update.cls` `TestSharedTransitiveDependencyUpdate`, `TestDependencyVersionsUpdated`.

### update.steps

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: `wiki/03` L365-380; `wiki/08` L14-61; `Main.cls:166-167` ("runs all of the update steps from the current version to the newer version"); `General/Update/VersionBase.cls:1-24`. Inventory: CAT-09, A1 seed 3; A2 1.3 update steps (`Storage.Module:HandleAllUpdateSteps`, `ExecuteUpdateStep`). Tests: `Update.cls` `TestAllNoErrorModulesNoMirror`, `TestUpdateAfterInterruptedUpdate`; `InstallModule.cls` `TestUpdateStepsSeededOnInstall`. Observed: steps run from a record of completed steps, not a version range. A normal install marks all current steps as done, and `update` runs from the first step not yet done. Suspected gaps, not run natively by B2: if the update lock is busy for 3 seconds, steps are skipped and the update still succeeds (OBS-L9). A dependency newly installed during `update` runs all its steps instead of marking them done (OBS-L5). Mirrored instances are future work (`wiki/08` L5). Primary-only steps are skipped on other mirror members, and nothing runs them later despite `General/Update/VersionBase.cls:18`.
