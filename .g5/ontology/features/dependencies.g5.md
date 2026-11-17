# Resolving dependencies and using lock files

Before it loads any dependency, IPM works out which version of each dependency to use. [Loading](load.g5.md#load.dependencies) owns the order and failure rules of the dependency loads themselves.

## Definitions

- **Dependency tree**: a module's dependencies, their dependencies, and so on.
- **Lock file**: `module-lock.json`, a record of the exact dependency versions a module was installed with.

## Choosing compatible dependencies

<a id="deps"></a>
[I · deps]

Before it loads any dependency, IPM resolves the module's whole dependency tree across the enabled repositories, one level at a time.

- It passes every parameter except [developer mode](../concepts/transactions-and-dev-mode.g5.md#dev-mode.not-propagated) on to the dependencies.
- A dependency limited to the `test` or `verify` phase is loaded only for that phase.
- ORAS dependency selection also applies [platform filtering across versions](../architecture/package-sources.g5.md#repo.oras.selection-modes); a version range does not authorize an incompatible deployed artifact.

<a id="deps.conflict"></a>
[I · deps.conflict]

When no version can satisfy every requirement, IPM stops the load with an error that names the modules whose requirements conflict.

- The conflict fails the load, so [what a failed load keeps](load.g5.md#load.outcome) applies.
- In developer mode, resolution happens after the [first commit](load.g5.md#load.outcome.dev), so the saved manifest stays.

<a id="deps.installed"></a>
[I · deps.installed]

A dependency version must also satisfy the requirements of modules already installed in the namespace. Otherwise the load fails with "Requested version ... does not satisfy the requirements of other modules installed in the current namespace", like a [conflict](#deps.conflict).

- Resolution picks the highest offered version that meets the parent's range, every installed module's range and the no-downgrade floor. An installed requirement can therefore select an older version than the newest offer.
- An explicit `install <module> <version>` is refused with that message when the version breaks an installed module's requirement, and the installed version stays.
- As built, `install <module>` without a version takes the newest offer and is refused if it breaks an installed requirement, even when an older offer satisfies all of them. Whether it should fall back is the [unversioned-install question](../../Verus_verification/open_questions.md#deps.installed.unversioned-policy).
- As built, `load <directory>` does not apply this check to the module it loads. Whether it should is the [directory-load question](../../Verus_verification/open_questions.md#deps.installed.directory-policy).

- Working assumption: keep the as-built refusal, so the user sees the conflict and chooses a version explicitly.
- Working assumption: as built, the load succeeds without a warning, and installed modules keep requirements the namespace no longer satisfies.

## Recording and reusing exact versions

<a id="deps.lockfile"></a>
[I · deps.lockfile]

`-create-lockfile` on `install`, `load` or `update` writes `module-lock.json` in the module root. It lists the resolved dependency versions and the repositories they came from, without credentials.

- Only the top-level module gets a lock file.
- [Stored identities and serialized ordering](#deps.lockfile.content) determine the file contents; the internal dependency order is not kept as JSON key order.
- Installing from a lock file is planned but not available.

- As built, lock-file writing happens before later lifecycle phases. A later failed load can roll its module record back while the lock file remains.

- A write error is logged and can be swallowed rather than failing the load. When the new stored module still has an empty Root, native `load -lock` wrote relative `module-lock.json` in the process working directory instead of the requested module root.

- Root can be populated later in the load. A nonempty stored Root at return does not show which directory the earlier lock-file writer used.

- A direct helper with a populated Root is a distinct neighbor. The intended root-location rule above is preserved; this observation is a defect, not an exception authorizing another location.

- Working assumption: A needed module loaded from a folder is written to the lock file with an empty registry.

<a id="deps.lockfile.content"></a>
[D · deps.lockfile.content]

The lock-file writer opens installed modules by name, so their stored versions, repositories and declared requirements supply the entries. Repeated references to the same name keep one keyed entry; reference metadata does not replace the stored values.

- The graph helper emits dependencies before their dependents, keeping its scan position as nodes are removed. The JSON object's keys are serialized in name order, so a parent can appear before its leaf. This is inherited P:F-DEP-2; no current lock-file installer consumes that order.
- Each repository is included once, including the base module's repository. Export uses the repository kind's field mapping and omits the named username, password and token fields. Allowed text fields, including URL, are retained; arbitrary secrets embedded in those strings are not scrubbed by this projection.
- The normal CLI fixtures, a reverse-name graph and direct writer helpers were checked separately. Root placement, write-error handling and use during later lifecycle failure retain the earlier qualifications above.

## Metadata

### deps

- human-provided
- user-facing-behavior
- test-coverage
- Children: [conflict](#deps.conflict), [installed requirements](#deps.installed), [lock file](#deps.lockfile).

Sources: CHANGELOG 0.9.0 (`#261`), 0.10.5 (`#316`, `#481`, `#1015`); `wiki/03` L243-250. Inventory: CAT-18; OP-LOAD-13 (`Storage.Module:BuildDependencyGraph`, `Storage/Module.cls:898`). Tests: `BuildDependencyGraph.cls` `TestScopedDepExcludedWhenPhaseInactive`, `TestInstalledVersionUsedWhenCompatible`; `Scopes.cls` `TestScopedDependencyLoadingTest`. Observed: phase-limited dependencies are described only by code and tests. Each phase-scoped dependency that `test` or `verify` loads gets a `load` history record (OP-LOAD-29). No human source says when resolution runs relative to the manifest save. The statement says only that it precedes any dependency load (reconstruction check R1).

Findings: [P:F-DEP-1 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-dep-1). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-DEP-7 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-dep-7). The linked entry qualifies the caller, reader and providers.

### deps.conflict

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Dependency resolution](#deps).

Sources: CHANGELOG 0.10.8 (`#1192`, `#1077`); `Storage/Module.cls:954-959`. Timing: maintainer comment `Utils/Module.cls:1316-1317` places "the dependency resolution process" in the second developer-mode transaction, after the first commit. Reconstruction check row R1. Inventory: OP-LOAD-13 (`BuildDependencyGraph` runs inside `LoadDependencies`, `Utils/Module.cls:1328`). Inventory: CAT-18, A1 seed 4. Tests: `BuildDependencyGraph.cls` `TestInstalledDirectVersionConflictWithAndWithoutIgnoreFlag`, `TestInstalledModuleConstraintConflictWithAndWithoutIgnoreFlag`. A test comment (`BuildDependencyGraph.cls:231-232`) still calls an exact-versus-range case a known edge case that the CHANGELOG says is fixed.

### deps.installed

- design-verified

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Dependency resolution](#deps).

Sources: message `Utils/Module.cls:116-126`. Inventory: INT-LOAD-14; OP-LOAD-18 (`GetRequiredVersionExpression`). Test: `BuildDependencyGraph.cls` `TestCoUpdatingModulesExcludedFromInstalledCheck`. Modules being updated together are excluded from this check ([update requirements](update.g5.md#update.requirements)).

Model: [installed.rs](../../Verus_verification/consolidated/iter31/installed.rs) `resolve_b`, `reference_ok`, `install_b`, `load_b_directory`. Iteration31 native cases S1-S7 on a local filesystem repository replay the chosen version, refusals and the directory bypass ([handoff](../../uplift/consolidated/iter31/HANDOFF.md)).

Open questions: [Should `load <directory>` refuse or warn when the loaded module's version breaks a requirement that installed modules declare on it?](../../Verus_verification/open_questions.md#deps.installed.directory-policy), [When the newest offer of a module breaks an installed module's requirement, should `install <module>` without a version fall back to the newest offer that satisfies every requirement?](../../Verus_verification/open_questions.md#deps.installed.unversioned-policy)

State machines: [installed · iter31](../../Verus_verification/models/iter31-installed/manifest.json), [dependents · iter33](../../Verus_verification/models/iter33-dependents/manifest.json)

### deps.lockfile

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Dependency resolution](#deps).

Sources: CHANGELOG 0.10.5 (`#961`); issue #962 (open, install from a lock file); `Main.cls:314`, `:442`, `:185`; maintainer comment `Utils/Module.cls:1324-1325`. Inventory: CAT-35, INT-LOAD-15, A1 seed 6; OP-LOAD-13 (`LockFile:CreateLockFileForModule`). Tests: `LockFile.cls` `Test01Module0Dependencies` to `Test06ModuleMultipleVersions`. Observed: a lock-file error is only printed and does not fail the load. The file is written before the load commits, so it stays after a rollback (OBS-L11). Suspected gap, not run natively by B2: a first plain `load -lock` writes the file relative to the process folder, because the module root is not yet saved (OBS-L3). `General/LockFile.cls:2` mentions installing from a lock file, which does not exist yet.

Extension Verus: [thin_surfaces.rs::dependency_status](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::write_file](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [P:F-DEP-2 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-dep-2). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-DEP-8 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-dep-8). The linked entry qualifies the caller, reader and providers.

Findings: [C:D16 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d16). The linked entry qualifies the caller, reader and providers.

Open questions: [Should a lock file accept a directory-loaded dependency with no registry?](../../Verus_verification/open_questions.md#deps.lockfile.local-supplier-policy)

### deps.lockfile.content

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Lock files](#deps.lockfile).

[Formal contract](../../Verus_verification/consolidated/iter12/lockfile.rs): `invert`, `walk`, `flatten`, `add_dependency`, `add_repo`, `public_repo`, `json_keys`. Finite fuel exposes incomplete exploration rather than success; the named graph fixtures complete within recorded bounds. Name collation, JSON encoding and write providers remain boundaries. Native CLI installs generate new files after copied old files are removed; actual JSON readers compare identity, versions, requirements, order and whitelisted fields. Duplicate/forged reference metadata is a direct-helper API case, not an asserted normal caller input. [Index and scopes](../../uplift/consolidated/iter12/INDEX.md).

P:F-DEP-2's earlier [formal ordering witness](../../../playbook-uplift/models/verus/dep_resolve.rs) and native nc-dep-13-lockfile-order are inherited evidence. Current work strengthens producer-to-artifact correspondence and canonical ownership; it does not claim the ordering discrepancy as new.

State machines: [lockfile · iter12](../../Verus_verification/models/iter12-lockfile/manifest.json), [dep_resolve · inherited playbook](../../Verus_verification/models/playbook-dep-resolve/manifest.json)
