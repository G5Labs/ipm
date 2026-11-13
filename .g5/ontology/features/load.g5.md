# Loading a package

`load` puts a module into the current namespace from a local path. It shares its core with `install` and `update`, so the rules on this page also govern those commands unless a rule says otherwise. This page is in the formal scope of the uplift, together with the [history log](history.g5.md).

```
load <path>
  │
  ├─ history record created ............ every attempt, see history.record
  ├─ path checked, archive unpacked
  ├─ transaction started ............... unless NoTransaction / NoJournal
  ├─ version checks ─────────────────── refusals: downgrade, update required
  ├─ manifest saved
  │    └─ developer mode: first commit, then a second transaction
  ├─ system requirements checked ────── refusal
  ├─ dependencies loaded, one at a time
  │    └─ developer mode: second commit
  ├─ phases: Initialize → Reload → Validate → Compile → Activate
  │    (update adds ApplyUpdateSteps; developer mode runs these outside a transaction)
  └─ normal mode: commit, or roll back on any failure
```

The diagram shows the order at the pinned revision. The blocks below say which parts are required.

## Definitions

- **Manifest**: the module's `module.xml` file.
- **Module root**: the folder IPM loads a module's files from.
- **ModuleRoot setting**: the folder where IPM unpacks archives, by default `<data directory>/ipm/`. See [settings](../concepts/settings.g5.md#settings).
- **Installed module**: a module with the same name already in the namespace.
- **Refusal**: IPM stops the load with an error because the request is not allowed.
- **Module data**: what a load writes to the namespace's databases for a module, such as its manifest record and its code. Files on disk and the history log are not module data.
- **First commit**: in developer mode, the point where IPM commits the saved manifest, before it loads dependencies and resources.

## What a load does

<a id="load"></a>
[I · load]

`load <path>` loads a module into the current namespace. IPM also loads the module's dependencies, if it can find them in the repositories set up with `repo`.

<a id="load.path"></a>
[I · load.path]

The path can be one of:

- A folder that contains `module.xml`.
- A `.tgz` or `.tar.gz` archive that contains `module.xml`.
- A git repository URL. `-branch` picks the branch.

<a id="load.path.archive"></a>
[I · load.path.archive]

IPM unpacks an archive into `<ModuleRoot>/<name>/<version>/`, as `install` does, and loads the module from there. A folder load stays where it is. IPM removes its temporary unpacking folder whether the load succeeds or fails.

<a id="load.core"></a>
[I · load.core]

`load`, `install`, `update` and dependency loads share one loading path.

- `load` takes a local path. `install` finds the module in a repository and records where it came from.
- IPM checks the request, saves the manifest, loads dependencies, then runs the [lifecycle phases](../concepts/lifecycle.g5.md#lifecycle.chain) Initialize, Reload, Validate, Compile and Activate.
- `update` also runs ApplyUpdateSteps.
- Rules that apply only to `install` are owned on the [install page](install.g5.md) and the [overwrite guard](../concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard).

<a id="load.options"></a>
[I · load.options]

Other `load` options:

- `-dev` and `-nodev` turn [developer mode](../concepts/transactions-and-dev-mode.g5.md#dev-mode) on or off. Without either, see [plain load keeps mode](../concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky).
- `-force` allows a newer version without running its update steps ([update required](#load.refusal.update-required)).
- `-env` names `;`-separated JSON files of settings the module can read ([environment files](../concepts/modules.g5.md#module.env)).
- `-quiet` and `-verbose` set how much output appears.
- `-bypass-py-deps` skips [Python dependencies](../concepts/modules.g5.md#module.python), and `-extra-pip-flags` passes flags to pip.
- `-create-lockfile` writes the module's [lock file](dependencies.g5.md#deps.lockfile).
- `-skip-sync` leaves recording file hashes for `sync` to the next `sync`, in developer mode.
- `-D<name>=<value>` passes a parameter to the module and its phases, including the [transaction flags](../concepts/transactions-and-dev-mode.g5.md#transaction.opt-out).
- `-synchronous` is deprecated and ignored.

## Refusals

<a id="load.refusal"></a>
[I · load.refusal]

IPM refuses a load with an error when the request is not allowed. A refusal happens before IPM writes any module data, so in every mode the installed module's data stays as it was and the record reads uncommitted.

- This covers the refusals below and the [developer-mode overwrite guard](../concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard).
- When the requirements check runs is open ([timing](#load.refusal.requirements.timing)).
- Files written before the checks, such as an unpacked archive, follow the [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).
- A [dependency conflict](dependencies.g5.md#deps.conflict) is found while dependencies load, so [what a failed load keeps](#load.outcome) applies to it.
- Every refused attempt still gets a [history record](history.g5.md#history.actions).

<a id="load.refusal.path"></a>
[I · load.refusal.path]

`load` accepts only absolute paths and git URLs. It refuses an empty path, and a path that does not exist fails with an error instead of reporting success.

<a id="load.refusal.manifest"></a>
[I · load.refusal.manifest]

IPM refuses a module whose manifest is missing or invalid, for example malformed XML, no `<Module>` element or more than one document. It rejects such a manifest before it touches any resource.

<a id="load.refusal.downgrade"></a>
[I · load.refusal.downgrade]

IPM never replaces an installed module with an older version. It refuses with "Cannot downgrade" and tells the user to uninstall first, then install or load the older version. Neither `-force` nor `-dev` lifts this.

<a id="load.refusal.update-required"></a>
[I · load.refusal.update-required]

IPM refuses to load or install a newer version of an installed module when the new manifest declares `<UpdatePackage>`, and points the user to `update`. `-force` allows it, and the new version's update steps are then not run.

<a id="load.refusal.update-required.dev-bypass"></a>
[Q · load.refusal.update-required.dev-bypass]

Should `-dev` also lift the update-required refusal?

- Provisional answer: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.
- Reason for doubt: the README and the `-force` help name only `-force`.
- Alternative: only `-force` lifts it, as the README says.
- Related: a `-dev` install from a [read-only repository](../concepts/repositories.g5.md#repo.read-only) is checked before developer mode is turned off for it.

<a id="load.refusal.requirements"></a>
[I · load.refusal.requirements]

IPM refuses a module whose [system requirements](../concepts/modules.g5.md#module.requirement) are not met. For example, a module that must be installed in `%SYS` fails to load in any other namespace and loads in `%SYS`.

<a id="load.refusal.requirements.timing"></a>
[Q · load.refusal.requirements.timing]

Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?

- Provisional answer: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.
- Reason: keeping changes for debugging is meant for failures of a module that could be installed.
- Alternative: accept that a refused developer-mode or `-DNoTransaction=1` load keeps its saved manifest, since developer mode keeps what it did.

<a id="load.same-version"></a>
[D · load.same-version]

Loading or installing the version that is already installed succeeds without `-force` or `-dev`, and loads that version again. Automation often reinstalls the same version and expects success.

## What a failed load keeps

<a id="load.outcome"></a>
[I · load.outcome]

What a failed load keeps depends on how it ran.

| How the load ran | What a failure keeps |
|---|---|
| Normal mode, in a transaction | Nothing in the database. The whole load, including dependencies loaded for it, is rolled back. |
| Developer mode, in a transaction | Depends on when it fails. See [developer-mode commits](#load.outcome.dev). |
| `-DNoTransaction=1` or `-DNoJournal=1` | Everything written before the failure. |

- A [refusal](#load.refusal) keeps no module data in any mode. The requirements check is the open exception.
- A process that dies mid-load ends like a failure at that point. The open transaction is rolled back, so the table applies, and with `-DNoTransaction=1` or `-DNoJournal=1` nothing is rolled back and the namespace may be left inconsistent.
- Files and other changes outside the database follow the [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).
- The [history record](history.g5.md#history.committed) shows whether anything was kept. After a dying process, see [record](history.g5.md#history.record).

<a id="load.outcome.dev"></a>
[I · load.outcome.dev]

In developer mode, a load commits in steps so the developer can debug a failure.

- First commit: IPM commits the saved manifest before it loads dependencies and resources, so the developer can fix the module after a failure.
- Dependencies then load in a second transaction. It commits only if every dependency load succeeds. Otherwise all of them are rolled back and the manifest stays.
- The module's own phases, Initialize through Activate, run outside a transaction. Whatever they did before a failure stays.
- These commits happen only when transactions are on.

<a id="load.outcome.dev.early-failure"></a>
[I · load.outcome.dev.early-failure]

A developer-mode load that fails or is refused before its first commit is rolled back in full, like a normal-mode load. For example, loading an older version with `-dev` is refused and leaves the installed version in place.

## Dependencies

<a id="load.dependencies"></a>
[I · load.dependencies]

IPM loads a module's dependencies before running the module's own phases.

- Dependencies load one at a time, in dependency order.
- A failed dependency load fails the whole load.
- Each dependency load gets its own [history record](history.g5.md#history.dependencies).
- [Resolution and conflicts](dependencies.g5.md#deps) and [which parameters pass down](../concepts/transactions-and-dev-mode.g5.md#dev-mode.not-propagated) are owned elsewhere.

<a id="load.dependencies.local-repo"></a>
[I · load.dependencies.local-repo]

A `.modules` folder in the module root serves as a temporary repository that IPM searches first for dependencies. IPM removes this temporary repository when loading finishes, even after an error.

<a id="load.write-check"></a>
[I · load.write-check]

Before installing files, `install` and `load` check that IPM can write to the target folder.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md). Every block on this page is in the formal scope, and its model and native evidence are pending stage B.

### load

- human-provided
- user-facing-behavior
- test-coverage
- Children: [path](#load.path), [shared core](#load.core), [options](#load.options), [refusals](#load.refusal), [same version](#load.same-version), [outcome](#load.outcome), [dependencies](#load.dependencies), [write check](#load.write-check).

Sources: `Main.cls:268-276` (summary and description); `wiki/02` L564-568; vendor manual AIPM. Inventory: CAT-12, INT-LOAD-01; OP-LOAD-01, OP-LOAD-02. Tests: `History.cls` `TestSimpleHistory`; `LoadModuleWithDeps.cls` `TestLoadModuleWithDependencies`.

### load.path

- human-provided
- user-facing-behavior
- Expands: [Load](#load).
- Children: [archive](#load.path.archive).

Sources: `Main.cls:278-291` (examples include `.tgz`, `.tar.gz` and git URLs), `:305` (`-branch`); `wiki/02` L599-604; CHANGELOG 0.10.0 (`#474`), 0.10.4 (`#930`). Inventory: INT-LOAD-01; OP-LOAD-02, OP-LOAD-03. The `path` parameter help (`Main.cls:318`) still mentions only a folder. Suspected gap, stage B to confirm: `.tar.gz` is not recognized as an archive, because only the last extension is compared (`Main.cls:2466-2467`, OBS-L2). Observed: a git URL is cloned into `mgr/Temp/<timestamp>/`, the clone's exit code is not checked and the clone is never removed (OBS-L13).

### load.path.archive

- human-provided
- user-facing-behavior
- Expands: [Path](#load.path).

Sources: CHANGELOG 0.10.7 (`#945`), 0.10.11 (`#1183`); `wiki/02` L649-653; comments `Main.cls:2498-2500`. Inventory: INT-LOAD-03; OP-LOAD-02. No test loads an archive with `load`. Observed: an existing `<ModuleRoot>/<name>/<version>/` is deleted and replaced before any version check, and the copy stays after a later failure (OBS-L7, see [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope)).

### load.core

- human-verified
- architecture-decision
- Expands: [Load](#load).

Sources: `Main.cls:273` and `:407` (load and install descriptions); `wiki/00` L43-56 (phase chains); `General/AbstractHistory.cls:20-27` (source recorded for install). Inventory: INT-LOAD-04; OP-LOAD-05 to OP-LOAD-16 (`%IPM.Utils.Module:LoadNewModule`, `Utils/Module.cls:1152-1364`), OP-LOAD-17 to OP-LOAD-21. Observed: a legacy XML package from a registry is loaded without this path, with no transaction, lifecycle or committed marking (`Utils/Module.cls:175-184`, OBS-L16).

### load.options

- human-provided
- user-facing-behavior
- Expands: [Load](#load).

Sources: `Main.cls:304-315`; CHANGELOG `#704`, `#1260`, `#595`, `#647`, `#961`, `#1290`, `#885`. Inventory: INT-LOAD-15, INT-LOAD-14; OP-LOAD-02. `wiki/02` L589-590 still describes `-synchronous` as active.

### load.refusal

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Load](#load).
- Children: [path](#load.refusal.path), [manifest](#load.refusal.manifest), [downgrade](#load.refusal.downgrade), [update required](#load.refusal.update-required), [requirements](#load.refusal.requirements).

Sources: downgrade message `Utils/Module.cls:1214-1217`; `wiki/03` L260-261 ("validated before module installation"); CHANGELOG 0.10.11 (`#1237`). Inventory: INT-LOAD-02, -09, -10, -11. Tests: `History.cls` `TestDevModeRollbackNotCommitted`, `TestDevMode`. The tests check status and history only, not the installed module afterwards. The requirements refusal is the open exception ([timing](#load.refusal.requirements.timing)). Reconstruction check R6 and R7: the no-write rule now covers every refusal and the overwrite guard, and is limited to module data. Sources for the timing: PR #1322 body (committed marking "before writing when there is no transaction" follows the version checks); `wiki/09` L133. Suspected gaps, stage B to confirm: the late manifest checks ("No module definition found", "multiple documents", `Utils/Module.cls:1255-1264`) and the `update -path` module mismatch (`:1269-1275`, OBS-L6) run after the manifest is saved. Under `-DNoTransaction=1` the manifest then stays and the record reads committed.

### load.refusal.path

- human-provided
- user-facing-behavior
- Expands: [Refusals](#load.refusal).

Sources: CHANGELOG 0.10.4 (`#706`: "load now only accepts absolute paths"), 0.10.0 (`#726`: a nonexistent path "now returns an error instead of silently failing"); messages `Main.cls:2447-2452`. Inventory: INT-LOAD-02; OP-LOAD-02. Observed: an existing relative path is accepted, because the check refuses a path only when it is neither existing, nor absolute, nor a URL. An absolute path that does not exist fails later, when IPM reads the manifest. No test.

### load.refusal.manifest

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).

Sources: `wiki/09` L114-133 (malformed manifest "rejected before any resource was touched"); messages `Utils/Module.cls:1255-1264`, `:341-343`. Inventory: A1 2.3 "Manifest" row; OP-LOAD-05, OP-LOAD-09. Test: `History.cls` `TestDevMode` (malformed manifest, with and without `-dev`). Suspected gap, stage B to confirm: the "no module definition" and "multiple documents" checks run after the manifest is loaded, so with `-DNoTransaction=1` the manifest stays and the record reads committed. Reconstruction check R6 added the wiki's timing.

### load.refusal.downgrade

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).

Sources: message `Utils/Module.cls:1214-1217`; `-force` help `Main.cls:313`, `:441` ("newer version"); PR #887 review r2392741268 by isc-jili ("check for newVer<currVer has always been present"). Inventory: INT-LOAD-09; OP-LOAD-06. Tests: `InstallModule.cls` `TestInstallForceFlag`, `TestDevMode`; `History.cls` `TestDevModeRollbackNotCommitted`. Observed: `-DPermitDowngrade=1` does not lift this refusal, though dependency resolution honors it (OBS-L14). No human source defines `PermitDowngrade` beyond `update` refusing it.

### load.refusal.update-required

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).
- Children: [developer-mode bypass](#load.refusal.update-required.dev-bypass).

Sources: README L62; `-force` help `Main.cls:313`, `:441`; message `Utils/Module.cls:1218-1222`; CHANGELOG 0.10.4 (`#550`); `wiki/08` L3; PR #887 review r2388507362 by isc-tleavitt. Inventory: INT-LOAD-10; OP-LOAD-06. Test: `InstallModule.cls` `TestInstallForceFlag`. Observed: a forced normal-mode install records every update step as already done, so they never run (`Lifecycle/Module.cls:122-127`).

### load.refusal.update-required.dev-bypass

- agent
- uncertain
- user-facing-behavior
- Expands: [Update required](#load.refusal.update-required).

Sources: A1 Q-02; test `InstallModule.cls` `TestDevMode` ("succeeds in dev mode and fails otherwise"); issue #903 by isc-eneil; README L62. Observed: the check skips `-dev` (`Utils/Module.cls:1219`), and a `-dev` install leaves the update steps pending for a later `update`. A read-only repository forces normal mode only after this check (`Utils/Module.cls:1277-1288`).

### load.refusal.requirements

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).
- Children: [timing](#load.refusal.requirements.timing).

Sources: `wiki/03` L260-269; CHANGELOG 0.10.10 (`#106`), 0.10.0 (`#469`), 0.9.0 (`#364`), 0.10.4 (`#874`). Inventory: INT-LOAD-11, CAT-05; OP-LOAD-12. Test: `SystemRequirements.cls` `TestSYSNamespaceRequirement` (status and message only).

### load.refusal.requirements.timing

- agent
- uncertain
- user-facing-behavior
- Expands: [Requirements](#load.refusal.requirements).

Sources: A1 Q-17; `wiki/03` L260-261; issue #363 comment by isc-tleavitt. Observed: the check runs after the manifest is saved and after the developer-mode first commit (`Utils/Module.cls:1251-1321`). A refused `-dev` or `-DNoTransaction=1` load therefore keeps the manifest, and its record reads committed (OBS-L10). A refused normal-mode load is rolled back. No test covers the state after a refusal.

### load.same-version

- human-provided
- architecture-decision
- Expands: [Load](#load).

Sources: PR #887 review r2388507362 by isc-tleavitt, 2025-09-29 ("someone's automation ... does an 'install' of the same version every time and makes sure that it succeeds"); r2392741268 by isc-jili ("removed check for newVer==currVer"). Inventory: INT-LOAD-18; OP-LOAD-06. This supersedes issue #903 and the CHANGELOG 0.10.4 `#903` entry, which said a same-version install should fail. No test covers a same-version install.

### load.outcome

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Load](#load).
- Children: [developer-mode commits](#load.outcome.dev).

Sources: `wiki/00` L125-126, L145-155; CHANGELOG 0.10.11 (`#1237`). Inventory: INT-LOAD-08, A1 2.2 and 2.3; commit-point matrix in A2 2.3; OP-LOAD-05, -07, -11, -14, -15, -16. Tests: `History.cls` `TestFailingPhases` (normal mode), `TestNoTransactionFailure`. The tests check status and history only. Observed: a load nested inside a caller's transaction cannot make its own commits final (P-4, OBS-H16, see [nested commits](history.g5.md#history.committed.nested)). Reconstruction check R5 added the dying-process rule from `wiki/00` L147 ("a failure mid-way through is fully rolled back") and L155 (with `NoJournal`, "if the process crashes mid-install, changes cannot be rolled back and the namespace may be left in an inconsistent state"). That IRIS rolls back a dead process's open transaction is premise P-6, for stage B. Observed: the `.modules` temporary repository and unpacked files stay after a killed load.

### load.outcome.dev

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Outcome](#load.outcome).
- Children: [early failure](#load.outcome.dev.early-failure).

Sources: `wiki/00` L126; `wiki/09` L86; maintainer comments `Utils/Module.cls:1310`, `:1316-1317`, `:1336-1337` ("commit load of dependencies (if all went well) and don't wrap reload/compile ... in a transaction; we want to be able to debug!"). Inventory: INT-LOAD-06; OP-LOAD-11, OP-LOAD-14. Test: `History.cls` `TestDevModeFailureAfterCommit`. Observed: a developer-mode dependency commits inside its parent's transaction, so its own commits are not final (P-4).

### load.outcome.dev.early-failure

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer-mode commits](#load.outcome.dev).

Sources: CHANGELOG 0.10.11 (`#1237`: "A -dev action rolled back before its first commit reads uncommitted"); issue #1237 by isc-dchui ("Everything in that window runs inside a transaction that the catch block rolls back ... regardless of developer mode"). Inventory: INT-LOAD-05, INT-LOAD-09; OP-LOAD-06, OP-LOAD-16. Tests: `History.cls` `TestDevModeRollbackNotCommitted`, `TestDevMode`.

### load.dependencies

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Load](#load).
- Children: [local repository](#load.dependencies.local-repo).

Sources: `Main.cls:270-271`, `:312`; CHANGELOG 0.10.5 (`#885`); `wiki/00` L126. Inventory: INT-LOAD-14; OP-LOAD-13, OP-LOAD-18, OP-LOAD-20. Tests: `LoadModuleWithDeps.cls` `TestLoadModuleWithDependencies`; `History.cls` `TestWithDependencies`.

### load.dependencies.local-repo

- human-provided
- user-facing-behavior
- Expands: [Dependencies](#load.dependencies).

Sources: PR #726 body by isc-shuliu; CHANGELOG 0.10.0 (`#726`, `#729`); comment `Main.cls:2523-2524`. Inventory: INT-LOAD-14; OP-LOAD-04. Suspected gap, stage B to confirm: if the `.modules` folder cannot be cached, the temporary repository is never created and the final cleanup fails after the load has committed (OBS-L12). Observed: a process that dies mid-load leaves the temporary repository defined.

### load.write-check

- human-provided
- user-facing-behavior
- Expands: [Load](#load).

Sources: CHANGELOG 0.10.7 (`#1053`); issue #1053. Inventory: INT-LOAD-16; OP-LOAD-19. Observed: an explicit check exists only for repository archives (`Utils/Module.cls:228-233`). An archive `load` relies on folder-creation errors (`Main.cls:2510`).
