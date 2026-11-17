# Uninstalling a module

`uninstall` removes modules from a namespace. It runs the Clean [lifecycle phase](../architecture/lifecycle.g5.md#lifecycle) and is recorded in [history](history.g5.md#history.actions).

## Definitions

- **Dependent**: an installed module that lists another module as a dependency.

## Removing module resources

<a id="uninstall"></a>
[I · uninstall]

`uninstall <module>` removes an installed module's resources and its record from the current namespace. A module name is required unless `-all` is given.

- Uninstall runs without a transaction, so a failure can leave some resources removed.
- For uninstall, the history `Committed` value [only shows success](history.g5.md#history.committed.uninstall).

<a id="uninstall.cleanup"></a>
[D · uninstall.cleanup]

Uninstall deletes the module's classes, packages, includes and routines with their compiled forms, its record, its stored manifest and its sync baseline. A global resource's data is deleted only when the resource names it with `Global`, and FileCopy targets are deleted unless `KeepOnUninstall` is set.

- As built, declared `<Mapping>` entries stay in the namespace, also with `-purge` (defect C:D67).
- As built, a developer-mode module's `^Sources` entries stay and keep pointing at its old folder (defect C:D68).

<a id="uninstall.package-ownership"></a>
[D · uninstall.package-ownership]

Uninstall expands a package without selecting resources separately claimed by another stored reference, including a nested package. The checked ASCII names match without regard to case.

- A separately declared child of the same module is also left out of its parent package's expansion. Its own declaration still selects it, so uninstall removes it.
- The checked uninstall removes the selected classes and module record while leaving another module's nested class callable. Routine/include deletion, provider failures and resource hooks are separate boundaries.

<a id="uninstall.class-delete-error"></a>
[D · uninstall.class-delete-error]

When an editing lock prevents a class from being deleted, uninstall prints a warning, reports success and removes the module and manifest records while the class remains callable. Without the lock, the checked uninstall removes the class.

- The failure was calibrated with the actual class-deletion API before the full uninstall. The status and surviving code are separate observations; other deletion errors may have different effects.
- This is C:D28's reachable discrepancy against the general removal rule. Whether a warning should permit removing the metadata is an [open policy question](../../Verus_verification/open_questions.md#uninstall.class-delete-error.policy).

- Working assumption: preserve and disclose the checked behavior pending an owner decision. Returning an error would make incomplete removal visible to scripts; retaining metadata would preserve a retry route. Earlier external effects may already have occurred.

## Checking and removing dependents

<a id="uninstall.dependents"></a>
[I · uninstall.dependents]

IPM prevents uninstalling a module that other installed modules depend on, unless `-force` is given. The intended refusal is an error that names the dependents.

- As built, the dependency check comes too late. The module's web applications are deleted and its before-Clean hooks run first. The command then prints "Clean SKIPPED - required by …", keeps the module and returns success. See [the as-built gap](#uninstall.dependents.as-built).

- Working assumption: A need declared only for a phase (test, verify) does not block uninstall.

<a id="uninstall.dependents.as-built"></a>
[D · uninstall.dependents.as-built]

A blocked uninstall deletes the module's web applications, keeps the module installed and reports success. The dependent module's own applications are untouched.

- `-force` removes the module. A dependent stays installed with its dependency gone.
- This is the inherited finding P:F-UNIN-1 (deletion before the check) and P:F-UNIN-6 (success instead of an error), registered as C:D25. The proposed repair checks dependents before any resource processor runs.

<a id="uninstall.recurse"></a>
[I · uninstall.recurse]

`-recurse` also uninstalls the module's dependencies, except those other installed modules still need. `-force` removes those too.

<a id="uninstall.child-error"></a>
[D · uninstall.child-error]

If a resource processor returns an error while resolving its children, uninstall can still report success and delete the module and manifest records while leaving that resource's code installed. Packaging propagates the same resolution error.

- The failed hook returns before its local child list is copied to the caller. Clean ignores the returned status and then sees no children to remove.
- A successful hook selects the class and the checked uninstall removes it. The failure was reproduced through a supported custom processor; it does not show that the default class processor normally fails.
- This violates the removal promised by [uninstall](#uninstall). C:D27 records the reachable discrepancy; success alone cannot establish cleanup.

## Bulk and global removals

<a id="uninstall.all"></a>
[I · uninstall.all]

`-all` uninstalls every module in the namespace except IPM itself.

- As built, without force, a failed module blocks later attempts on dependencies that still need it. With force, IPM removes that failed module from its traversal graph and continues, while retaining the error in its final status.
- Traversal progress is not proof of removal from the installed-module store. A failed provider can leave resources or its row behind; those final effects must be read separately. [Scoped traversal state and valid neighbor](../../Verus_verification/consolidated/iter2/readers.rs).

<a id="uninstall.global"></a>
[I · uninstall.global]

IPM skips modules with global scope unless `-global` is given.

## Purging data or removing IPM itself

<a id="uninstall.purge"></a>
[I · uninstall.purge]

`-purge` deletes the data in the module's tables during uninstall.

- The pinned implementation leaves table data after uninstall, including with `-purge`. See [the checked data-retention gap](#uninstall.purge.as-built).

<a id="uninstall.purge.as-built"></a>
[D · uninstall.purge.as-built]

An ordinary persistent-class module retains its rows after uninstall with or without `-purge`; loading the same schema makes those rows readable again. The command sets `Purge`, while the installer callback receives only `Configure.Purge`, and ordinary class deletion does not purge its extent.

- A supported installer can delete its own data. In the checked fixture, `-DConfigure.Purge=1` reaches that callback and deletes its rows; the same setting without an installer leaves them intact. This is inherited P:F-UNIN-2, not a new finding or a product repair.

<a id="uninstall.ipm"></a>
[I · uninstall.ipm]

`uninstall zpm` removes IPM from the namespace. It first asks whether to also remove all other modules and IPM's metadata, which never includes the [history log](../concepts/data-policy.g5.md#history.keep-on-ipm-uninstall), and then the shell exits.

## Metadata

### uninstall

- human-provided
- user-facing-behavior
- test-coverage
- Children: [cleanup](#uninstall.cleanup), [dependents](#uninstall.dependents), [recurse](#uninstall.recurse), [all](#uninstall.all), [global](#uninstall.global), [purge](#uninstall.purge), [IPM itself](#uninstall.ipm).

Sources: `Main.cls:483-507`; `wiki/02` L1251-1288; CHANGELOG 0.10.5 (`#1018`); `General/AbstractHistory.cls:43`. Inventory: CAT-16; A2 1.2 row 22 (`Main:Uninstall`, `Storage.Module:Uninstall`), OP-HIST-02. Tests: `Unit/CLI.cls` `TestUninstallWithoutModuleName`; `History.cls` `TestSimpleHistory`. `FileCopy` targets marked `KeepOnUninstall` stay ([file copy](resources.g5.md#module.processor.file-copy)).

Findings: [P:F-UNIN-5 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-5). The linked entry qualifies the caller, reader and providers.

### uninstall.cleanup

- design-verified

- agent
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Lifecycle/Base.cls::%Clean`, `::%Reload`; `ResourceProcessor/Default/Global.cls`; `ResourceProcessor/FileCopy.cls`. Model: [cleanup.rs](../../Verus_verification/consolidated/iter63/cleanup.rs) `remains`, `code_removed`, `globals_by_declaration`, `filecopy_keep`, `reload_side_effects_outlive_uninstall` ([handoff](../../uplift/consolidated/iter63/HANDOFF.md)). Automatic mappings are C:D65; a name-only global keeping its data is P:F-UNIN-5.

Findings: [Uninstall leaves a module's declared `<Mapping>` entries in the namespace](../../Verus_verification/bugs_issues.md#c-d67)

Findings: [Uninstall leaves a developer-mode module's `^Sources` entries](../../Verus_verification/bugs_issues.md#c-d68)

State machines: [cleanup · iter63](../../Verus_verification/models/iter63-cleanup/manifest.json)

### uninstall.package-ownership

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Uninstall](#uninstall).

The product's `PkgCaseUninstall` tests supply the ownership requirement. [Formal contract](../../Verus_verification/consolidated/iter9/ownership.rs): `protects`, `claimed`, `children`, `module_children`, `clean`, `package_neighbors`. Segment decoding, IRIS name collation and deletion providers remain boundaries; the model carries the selected set through all of a module's references. [Role bindings and native evidence](../../uplift/consolidated/iter9/INDEX.md).

State machines: [ownership · iter9](../../Verus_verification/models/iter9-ownership/manifest.json)

### uninstall.class-delete-error

- design-verified

- uncertain
- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Uninstall](#uninstall).
- Children: [Should uninstall report an error and keep its metadata when a class cannot be deleted, or warn and remove the metadata as it does now?](../../Verus_verification/open_questions.md#uninstall.class-delete-error.policy).

[Formal contract](../../Verus_verification/consolidated/iter11/deletion.rs): `delete_class`, `uninstall`, `deletion_neighbors`, `locked_retains_class`. Actual dictionary lock acquisition, direct delete refusal, subsequent warning/status, metadata and code were checked. This is one non-projection fixture class; other provider errors, mapped resources, extent deletion and earlier hooks are not inferred. [Index](../../uplift/consolidated/iter11/INDEX.md).

Findings: [C:D28 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d28). The linked entry qualifies the caller, reader and providers.

Open questions: [Should uninstall report an error and keep its metadata when a class cannot be deleted, or warn and remove the metadata as it does now?](../../Verus_verification/open_questions.md#uninstall.class-delete-error.policy)

State machines: [deletion · iter11](../../Verus_verification/models/iter11-deletion/manifest.json)

### uninstall.dependents

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:486-487`, `:501`; `wiki/02` L1253; CHANGELOG 0.10.4 (`#058`). Inventory: CAT-16. Suspected gap, not run natively by B2: IPM skips the Clean phase with "Clean SKIPPED" and reports success, so the module stays and history records a successful, committed uninstall (`Lifecycle/Base.cls:290-293`, OBS-L17). The maintainer test that expects an error is skipped as "does not yet exist" (`InstallModule.cls` `TestUninstallDependencyDoesError`).

- Children: [as-built gap](#uninstall.dependents.as-built).

Iteration 5: the suspected gap above is now native-confirmed (W6), and the intended error is supported by the help text, `#058`, issue #58 and the skipped maintainer test.

Findings: [P:F-UNIN-4 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-4). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-UNIN-6 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-6). The linked entry qualifies the caller, reader and providers.

Findings: [C:S17 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s17).

Open questions: [Should a phase-scoped dependency block uninstall?](../../Verus_verification/open_questions.md#uninstall.dependents.scoped-policy)

### uninstall.dependents.as-built

- design-verified

- agent
- user-facing-behavior
- Expands: [Dependents](#uninstall.dependents).

Direct decision: `Storage/Module.cls::ExecutePhases` (processor `OnBeforePhase("Clean")` before `%Clean`), `Lifecycle/Base.cls::{%Clean,CheckBeforeClean}` (skip, not error). Deleting reader: `ResourceProcessor/WebApplication.cls::DeleteWebApp`. Provenance: P:F-UNIN-1 and P:F-UNIN-6 (`playbook-uplift/BUGS.md`, P's proofs `unin_witness_blocked_deletes_webapp` and native `nc-unin-1-blocked-uninstall`), G:OBS-L17 (success half, then unrun). Iteration 5 adds a fresh-container re-observation (W6 blocked, W7 forced) and a Verus contract [webapp.rs](../../Verus_verification/consolidated/iter5/webapp.rs) `{uninstall,repaired_uninstall,lemma_blocked_uninstall_deletes}`; controls `blocked-uninstall-keeps-app`, `blocked-uninstall-fails`, `repair-still-deletes` rejected. CSP application purge and invoke hooks before the check are P's evidence, not re-run here.

Findings: [P:F-UNIN-1 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-1). The linked entry qualifies the caller, reader and providers.

State machines: [webapp · iter5](../../Verus_verification/models/iter5-webapp/manifest.json)

### uninstall.recurse

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:502`; `wiki/00` L26 (Clean). Inventory: CAT-16; OP-HIST-24. Observed: a dependency kept because another module needs it gets a successful uninstall record (OBS-H8).

Findings: [P:F-UNIN-9 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-9). The linked entry qualifies the caller, reader and providers.

### uninstall.child-error

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Uninstall](#uninstall).

[Formal contract](../../Verus_verification/consolidated/iter10/errors.rs): `resolve`, `package`, `uninstall`, `returned_error_neighbors`, `failed_hook_leaves_code`. A supported processor returns an error after enumeration; the actual persisted ProcessorClass dispatch and caller statuses are checked natively, with hook-called counters, absent metadata and a callable surviving class. [Evidence index](../../uplift/consolidated/iter10/INDEX.md). Proposed `repaired_uninstall` checks the status for this single resource; it does not promise compensation for earlier provider effects. No product repair applied.

Findings: [C:D27 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d27). The linked entry qualifies the caller, reader and providers.

Findings: [C:S20 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s20).

State machines: [errors · iter10](../../Verus_verification/models/iter10-errors/manifest.json)

### uninstall.all

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:499`; CHANGELOG 0.10.4 (`#908`). Inventory: CAT-16; A2 1.2 row 22 (`Utils.Module:UninstallAll`). Tests: `Uninstall.cls` `TestSimpleUninstallAll`, `TestUninstallAllNoForce`, `TestUninstallAllForce`. Each module gets its own history record.

### uninstall.global

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:500`; `wiki/00` L26; `wiki/03` (`GlobalScope`). Inventory: CAT-10, CAT-16.

### uninstall.purge

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: CLI help `Main.cls:505` promises table-data deletion. Inventory: CAT-16. B2 originally left the parameter mismatch unrun; P later confirmed it as F-UNIN-2. Iteration 27 connects the actual producer, installer argument and post-reload SQL reader. The intended promise above remains unchanged. PR #887's scope exclusion does not implement that promise.

### uninstall.purge.as-built

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Purge](#uninstall.purge).

[Scoped contract](../../Verus_verification/consolidated/iter27/purge.rs) carries the literal alias/dotted parameter path through unconfigure, the selected fixture callback, class deletion and restoration of the same schema to its SQL row/value reader. [Evidence index](../../uplift/consolidated/iter27/INDEX.md). The fixture callback explicitly invokes `%DeleteExtent` only when its argument is true; it is a provider calibration, not automatic product behavior. Successful unmapped deletion, no hooks other than the named fixture and stable schema/storage are premises. Mapped resources, failures, projections, cascades and arbitrary installer behavior remain outside this scope.

Findings: [P:F-UNIN-2 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-unin-2). The linked entry qualifies the caller, reader and providers.

State machines: [purge · iter27](../../Verus_verification/models/iter27-purge/manifest.json)

### uninstall.ipm

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: prompt `Main.cls:2735`; CHANGELOG 0.10.7 (`#1057`), 0.10.10 (`#1227`, `#1228`). Inventory: CAT-16, INT-HIST-15; OP-HIST-23. Suspected gap, not run natively by B2: the record is finalized as successful before Clean runs, and the shell exits saying IPM was uninstalled even if Clean was skipped (OBS-H15).
