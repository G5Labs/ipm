# Dependency resolution and lock files

Before it loads any dependency, IPM works out which version of each dependency to use. [Loading](load.g5.md#load.dependencies) owns the order and failure rules of the dependency loads themselves.

## Definitions

- **Dependency tree**: a module's dependencies, their dependencies, and so on.
- **Lock file**: `module-lock.json`, a record of the exact dependency versions a module was installed with.

## Statements

<a id="deps"></a>
[I · deps]

Before it loads any dependency, IPM resolves the module's whole dependency tree across the enabled repositories, one level at a time.

- It passes every parameter except [developer mode](../concepts/transactions-and-dev-mode.g5.md#dev-mode.not-propagated) on to the dependencies.
- A dependency limited to the `test` or `verify` phase is loaded only for that phase.

<a id="deps.conflict"></a>
[I · deps.conflict]

When no version can satisfy every requirement, IPM stops the load with an error that names the modules whose requirements conflict.

- The conflict fails the load, so [what a failed load keeps](load.g5.md#load.outcome) applies.
- In developer mode, resolution happens after the [first commit](load.g5.md#load.outcome.dev), so the saved manifest stays.

<a id="deps.installed"></a>
[I · deps.installed]

A dependency version must also satisfy the requirements of modules already installed in the namespace. Otherwise the load fails with "Requested version ... does not satisfy the requirements of other modules installed in the current namespace", like a [conflict](#deps.conflict).

<a id="deps.lockfile"></a>
[I · deps.lockfile]

`-create-lockfile` on `install`, `load` or `update` writes `module-lock.json` in the module root. It lists the resolved dependency versions and the repositories they came from, without credentials.

- Only the top-level module gets a lock file.
- Installing from a lock file is planned but not available.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Resolution is used by the formal scope through [load](load.g5.md#load.dependencies). The graph algorithm itself is outside it.

### deps

- human-provided
- user-facing-behavior
- test-coverage
- Children: [conflict](#deps.conflict), [installed requirements](#deps.installed), [lock file](#deps.lockfile).

Sources: CHANGELOG 0.9.0 (`#261`), 0.10.5 (`#316`, `#481`, `#1015`); `wiki/03` L243-250. Inventory: CAT-18; OP-LOAD-13 (`Storage.Module:BuildDependencyGraph`, `Storage/Module.cls:898`). Tests: `BuildDependencyGraph.cls` `TestScopedDepExcludedWhenPhaseInactive`, `TestInstalledVersionUsedWhenCompatible`; `Scopes.cls` `TestScopedDependencyLoadingTest`. Observed: phase-limited dependencies are described only by code and tests. Each phase-scoped dependency that `test` or `verify` loads gets a `load` history record (OP-LOAD-29). No human source says when resolution runs relative to the manifest save. The statement says only that it precedes any dependency load (reconstruction check R1).

### deps.conflict

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Dependency resolution](#deps).

Sources: CHANGELOG 0.10.8 (`#1192`, `#1077`); `Storage/Module.cls:954-959`. Timing: maintainer comment `Utils/Module.cls:1316-1317` places "the dependency resolution process" in the second developer-mode transaction, after the first commit. Reconstruction check row R1. Inventory: OP-LOAD-13 (`BuildDependencyGraph` runs inside `LoadDependencies`, `Utils/Module.cls:1328`). Inventory: CAT-18, A1 seed 4. Tests: `BuildDependencyGraph.cls` `TestInstalledDirectVersionConflictWithAndWithoutIgnoreFlag`, `TestInstalledModuleConstraintConflictWithAndWithoutIgnoreFlag`. A test comment (`BuildDependencyGraph.cls:231-232`) still calls an exact-versus-range case a known edge case that the CHANGELOG says is fixed.

### deps.installed

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Dependency resolution](#deps).

Sources: message `Utils/Module.cls:116-126`. Inventory: INT-LOAD-14; OP-LOAD-18 (`GetRequiredVersionExpression`). Test: `BuildDependencyGraph.cls` `TestCoUpdatingModulesExcludedFromInstalledCheck`. Modules being updated together are excluded from this check ([update requirements](update.g5.md#update.requirements)).

### deps.lockfile

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Dependency resolution](#deps).

Sources: CHANGELOG 0.10.5 (`#961`); issue #962 (open, install from a lock file); `Main.cls:314`, `:442`, `:185`; maintainer comment `Utils/Module.cls:1324-1325`. Inventory: CAT-35, INT-LOAD-15, A1 seed 6; OP-LOAD-13 (`LockFile:CreateLockFileForModule`). Tests: `LockFile.cls` `Test01Module0Dependencies` to `Test06ModuleMultipleVersions`. Observed: a lock-file error is only printed and does not fail the load. The file is written before the load commits, so it stays after a rollback (OBS-L11). Suspected gap, not run natively by B2: a first plain `load -lock` writes the file relative to the process folder, because the module root is not yet saved (OBS-L3). `General/LockFile.cls:2` mentions installing from a lock file, which does not exist yet.
