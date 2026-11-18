# Updating a module

`update` moves an installed module to a newer version and runs the update steps its author wrote. It uses the [shared loading path](load.g5.md#load.core) and adds the ApplyUpdateSteps phase.

## Definitions

- **Update step**: a method a module author writes to move data or settings from one version to the next.
- **Update package**: the class package a module names in `<UpdatePackage>` to hold its update steps.

## Choosing and admitting an update

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

## Applying and recording update steps

<a id="update.steps"></a>
[I · update.steps]

A module that declares `<UpdatePackage>` provides update steps.

- Each `V<major>` class in that package extends `%IPM.General.Update.VersionBase` and lists its step methods in order.
- Steps must be safe to run again, and they report errors by throwing.
- `update` runs the steps needed to move from the current version to the new one.

- Working assumption: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.
- Working assumption: A developer-mode install or load seeds update steps like any install; a later update runs only newer steps.
- Working assumption: A module that `update` installs for the first time has its steps seeded, as on a fresh install.
- Working assumption: If the update lock is busy, `update` fails with an error that says so; it never reports success.

<a id="update.steps.shared-execution"></a>
[D · update.steps.shared-execution]

The update-step lock belongs to the module, while a step record belongs to its class and method. Two modules sharing a due step can therefore enter it at the same time and both report success; serial calls enter once and then reuse its completion.

- In the checked two-process schedule, B saves its completion and home first. A's later save restores A as the home. This is observed for the AnyMember store and this save order, not a general rule for every field or storage class.
- Two calls for the same module share a lock. The second can wait out its timeout and return success without entering the step.
- Steps must remain safe to repeat. The probe counts entries separately from an idempotent payload; two entries do not establish business corruption or violate a promised at-most-once policy.
- [Shared ownership policy](../../Verus_verification/open_questions.md#update.steps.shared-ownership) remains open. Initial row-insertion races, callback failure, mirrors and concurrent deletion are separate boundaries.

- Working assumption: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.

<a id="update.steps.records"></a>
[D · update.steps.records]

A saved update-step record is unique by class and method within each step kind, rather than by module. Registering that key for another module moves its home to that module and keeps its completion and status; seeding a successful completed record does not run it again.

- The two kinds, any-member and primary-only, use separate stores. Deleting a module removes the records whose current home is that module; records moved to another module survive until that home is deleted.
- This describes successful registration, seeding and deletion in one namespace. [Shared execution](#update.steps.shared-execution) covers selected concurrent callers; general concurrency safety and mirror delivery remain open.

## Metadata

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

Sources: messages `Main.cls:4415-4423`, `Utils/Module.cls:1269-1275`. Inventory: CAT-15; OP-LOAD-21, OP-LOAD-10. Tests: `Update.cls` `TestUpdateOfUninstalledModule`, `TestUpdateNonMatchingTarballFails` (status only), `TestUpdateToOlderVersion`; suite PASS 15 of 15 (`20261113T182433Z-suite-verify-Update`). Observed: the first two refusals happen before a history record exists ([Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?](../../Verus_verification/open_questions.md#history.record.unlogged-refusals)). Confirmed product defect D16: `update A -path` with module B's archive crashes with `<INVALID OREF>LoadNewModule+67` before the mismatch check, because IPM opens and uses the installed module under B's name while B is not installed (`Utils/Module.cls:1207`, `:1232`). Under `-DNoTransaction=1` the record reads `Committed=1`, and B's archive stays unpacked under `<ModuleRoot>`. No module data is kept, which refutes OBS-L6 (stage C cases S3 and S3N1; S3N2, a matching archive, updates; `20261113T204152Z-stagec`).

### update.dev-mode

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: CHANGELOG 0.10.5 (`#1036`). Inventory: CAT-15, INT-LOAD-07; OP-LOAD-20. Test: `Update.cls` `TestDevModePropagation`. Suspected gap, not run natively by B2: `update <m> -dev` from a registry leaves the module itself in normal mode (OBS-L4).

Findings: [P:F-INST-5 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-inst-5). The linked entry qualifies the caller, reader and providers.

### update.requirements

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: CHANGELOG 0.10.7 (`#1119`, `#430`); issue #430; PR #1136. Inventory: CAT-15, CAT-18, A1 seed 5. Tests: `Update.cls` `TestSharedTransitiveDependencyUpdate`, `TestDependencyVersionsUpdated`.

### update.steps

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Update](#update).

Sources: `wiki/03` L365-380; `wiki/08` L14-61; `Main.cls:166-167` ("runs all of the update steps from the current version to the newer version"); `General/Update/VersionBase.cls:1-24`. Inventory: CAT-09, A1 seed 3; A2 1.3 update steps (`Storage.Module:HandleAllUpdateSteps`, `ExecuteUpdateStep`). Tests: `Update.cls` `TestAllNoErrorModulesNoMirror`, `TestUpdateAfterInterruptedUpdate`; `InstallModule.cls` `TestUpdateStepsSeededOnInstall`. Observed: steps run from a record of completed steps, not a version range. A normal install marks all current steps as done, and `update` runs from the first step not yet done. Suspected gaps, not run natively by B2: if the update lock is busy for 3 seconds, steps are skipped and the update still succeeds (OBS-L9). A dependency newly installed during `update` runs all its steps instead of marking them done (OBS-L5). Mirrored instances are future work (`wiki/08` L5). Primary-only steps are skipped on other mirror members, and nothing runs them later despite `General/Update/VersionBase.cls:18`.

Findings: [P:F-INST-6 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-inst-6). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-INST-7 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-inst-7). The linked entry qualifies the caller, reader and providers.

Open questions: [Should an update whose step lock is busy fail or report success?](../../Verus_verification/open_questions.md#update.steps.busy-policy), [Should a developer-mode install seed update-step records?](../../Verus_verification/open_questions.md#update.steps.dev-seeding-policy), [Should a dependency first installed by update run its old steps or seed them?](../../Verus_verification/open_questions.md#update.steps.fresh-seeding-policy), [Should two modules be allowed to share an update-step class and method, or should the second registration fail?](../../Verus_verification/open_questions.md#update.steps.shared-ownership)

### update.steps.shared-execution

- design-verified

- uncertain
- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Step records](#update.steps.records).

[Formal contract](../../Verus_verification/consolidated/iter11/shared_steps.rs): `lock_key`, `begin`, `enter`, `finish`, `same_attempt`, `serial`, `shared_key_neighbors`. A native schedule starts B after A enters, then finishes B before A, and compares module locks, persisted rows, command status and an idempotent payload. The model folds each read/register/due decision before body entry, not arbitrary interleavings; its Snapshot option transcribes the measured stale HomeModule write. [Index](../../uplift/consolidated/iter11/INDEX.md).

Open questions: [Should two modules be allowed to share an update-step class and method, or should the second registration fail?](../../Verus_verification/open_questions.md#update.steps.shared-ownership)

State machines: [shared steps · iter11](../../Verus_verification/models/iter11-shared-steps/manifest.json)

### update.steps.records

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Update steps](#update.steps).
- Children: [Should two modules be allowed to share an update-step class and method, or should the second registration fail?](../../Verus_verification/open_questions.md#update.steps.shared-ownership).

[Formal contract](../../Verus_verification/consolidated/iter9/ownership.rs): `get_add`, `seed`, `delete_module`, `cascade_preserves_other_home`, `both_moved`, `cascade_neighbors`. The home must exist and persistence must succeed. Generated uniqueness/FK cascade is an IRIS boundary validated against both native stores; class-list parsing and mirrors are outside this contract. [Bindings and observations](../../uplift/consolidated/iter9/INDEX.md).

State machines: [ownership · iter9](../../Verus_verification/models/iter9-ownership/manifest.json)

Iteration75 neighbours: [r8b_records.rs](../../Verus_verification/coverage-20261117/iter75/r8b_records.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).
