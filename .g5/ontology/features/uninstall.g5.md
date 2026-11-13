# Uninstalling

`uninstall` removes modules from a namespace. It runs the Clean [lifecycle phase](../concepts/lifecycle.g5.md#lifecycle) and is recorded in [history](history.g5.md#history.actions).

## Definitions

- **Dependent**: an installed module that lists another module as a dependency.

## Statements

<a id="uninstall"></a>
[I · uninstall]

`uninstall <module>` removes an installed module's resources and its record from the current namespace. A module name is required unless `-all` is given.

- Uninstall runs without a transaction, so a failure can leave some resources removed.
- For uninstall, the history `Committed` value [only shows success](history.g5.md#history.committed.uninstall).

<a id="uninstall.dependents"></a>
[I · uninstall.dependents]

IPM prevents uninstalling a module that other installed modules depend on, unless `-force` is given.

<a id="uninstall.recurse"></a>
[I · uninstall.recurse]

`-recurse` also uninstalls the module's dependencies, except those other installed modules still need. `-force` removes those too.

<a id="uninstall.all"></a>
[I · uninstall.all]

`-all` uninstalls every module in the namespace except IPM itself.

<a id="uninstall.global"></a>
[I · uninstall.global]

IPM skips modules with global scope unless `-global` is given.

<a id="uninstall.purge"></a>
[I · uninstall.purge]

`-purge` deletes the data in the module's tables during uninstall.

<a id="uninstall.ipm"></a>
[I · uninstall.ipm]

`uninstall zpm` removes IPM from the namespace. It first asks whether to also remove all other modules and IPM's metadata, which never includes the [history log](history.g5.md#history.keep-on-ipm-uninstall), and then the shell exits.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Uninstall is in the formal scope only through its history records.

### uninstall

- human-provided
- user-facing-behavior
- test-coverage
- Children: [dependents](#uninstall.dependents), [recurse](#uninstall.recurse), [all](#uninstall.all), [global](#uninstall.global), [purge](#uninstall.purge), [IPM itself](#uninstall.ipm).

Sources: `Main.cls:483-507`; `wiki/02` L1251-1288; CHANGELOG 0.10.5 (`#1018`); `General/AbstractHistory.cls:43`. Inventory: CAT-16; A2 1.2 row 22 (`Main:Uninstall`, `Storage.Module:Uninstall`), OP-HIST-02. Tests: `Unit/CLI.cls` `TestUninstallWithoutModuleName`; `History.cls` `TestSimpleHistory`. `FileCopy` targets marked `KeepOnUninstall` stay ([file copy](../concepts/modules.g5.md#module.processor.file-copy)).

### uninstall.dependents

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:486-487`, `:501`; `wiki/02` L1253; CHANGELOG 0.10.4 (`#058`). Inventory: CAT-16. Suspected gap, stage B to confirm: IPM skips the Clean phase with "Clean SKIPPED" and reports success, so the module stays and history records a successful, committed uninstall (`Lifecycle/Base.cls:290-293`, OBS-L17). The maintainer test that expects an error is skipped as "does not yet exist" (`InstallModule.cls` `TestUninstallDependencyDoesError`).

### uninstall.recurse

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: `Main.cls:502`; `wiki/00` L26 (Clean). Inventory: CAT-16; OP-HIST-24. Observed: a dependency kept because another module needs it gets a successful uninstall record (OBS-H8).

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

Sources: `Main.cls:505`. Inventory: CAT-16. Suspected gap, stage B to confirm: `-purge` sets a parameter that nothing reads, because the only reader looks for `Configure.Purge` (`Lifecycle/Base.cls:554`). PR #887 listed deleting a module's data on uninstall as out of scope.

### uninstall.ipm

- human-provided
- user-facing-behavior
- Expands: [Uninstall](#uninstall).

Sources: prompt `Main.cls:2735`; CHANGELOG 0.10.7 (`#1057`), 0.10.10 (`#1227`, `#1228`). Inventory: CAT-16, INT-HIST-15; OP-HIST-23. Suspected gap, stage B to confirm: the record is finalized as successful before Clean runs, and the shell exits saying IPM was uninstalled even if Clean was skipped (OBS-H15).
