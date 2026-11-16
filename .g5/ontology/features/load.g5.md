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

- `-dev` and `-nodev` turn [developer mode](../concepts/transactions-and-dev-mode.g5.md#dev-mode) on or off. Without either, see [plain load keeps mode](../../Verus_verification/open_questions.md#dev-mode.sticky).
- `-force` allows a newer version without running its update steps ([update required](#load.refusal.update-required)).
- `-env` names `;`-separated JSON files of settings the module can read ([environment files](../concepts/modules.g5.md#module.env)).
- `-quiet` and `-verbose` set how much output appears.
- `-bypass-py-deps` skips [Python dependencies](../concepts/modules.g5.md#module.python), and `-extra-pip-flags` passes flags to pip.
- `-create-lockfile` writes the module's [lock file](dependencies.g5.md#deps.lockfile).
- `-skip-sync` leaves recording file hashes for `sync` to the next `sync`, in developer mode.
- `-D<name>=<value>` passes a parameter to the module and its phases, including the [transaction flags](../concepts/transactions-and-dev-mode.g5.md#transaction.opt-out).
- `-synchronous` is deprecated and ignored.

- Working assumption: yes, it keeps the stored mode.


## Refusals


<a id="load.refusal"></a>
[I · load.refusal]

IPM refuses a load with an error when the request is not allowed. A refusal happens before IPM writes any module data, so in every mode the installed module's data stays as it was and the record reads uncommitted.

- This covers the refusals below and the [developer-mode overwrite guard](../concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard).
- When the requirements check runs is open ([timing](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)).
- Files written before the checks, such as an unpacked archive, follow the [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).
- A [dependency conflict](dependencies.g5.md#deps.conflict) is found while dependencies load, so [what a failed load keeps](#load.outcome) applies to it.
- Every refused attempt still gets a [history record](history.g5.md#history.actions).

- Working assumption: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.


<a id="load.refusal.path"></a>
[I · load.refusal.path]

`load` accepts only absolute paths and git URLs. It refuses an empty path, and a path that does not exist fails with an error instead of reporting success.


<a id="load.refusal.manifest"></a>
[I · load.refusal.manifest]

IPM refuses a module whose manifest is missing or invalid, for example malformed XML, no `<Module>` element or more than one document. It rejects such a manifest before it touches any resource.


<a id="load.refusal.downgrade"></a>
[I · load.refusal.downgrade]

IPM never replaces an installed module with an older version. It refuses with "Cannot downgrade" and tells the user to uninstall first, then install or load the older version. Neither `-force` nor `-dev` lifts this.

- As built, the check uses [version order](../concepts/versions.g5.md#version.order), which does not order prereleases of two different versions. So `1.0.0-rc` replaces an installed `2.0.0-beta` without a refusal. This is a known defect.


<a id="load.refusal.update-required"></a>
[I · load.refusal.update-required]

IPM refuses to load or install a newer version of an installed module when the new manifest declares `<UpdatePackage>`, and points the user to `update`. `-force` allows it, and the new version's update steps are then not run.

- Working assumption: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.


<a id="load.refusal.requirements"></a>
[I · load.refusal.requirements]

IPM refuses a module whose [system requirements](../concepts/modules.g5.md#module.requirement) are not met. For example, a module that must be installed in `%SYS` fails to load in any other namespace and loads in `%SYS`.

- Working assumption: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.


<a id="load.same-version"></a>
[D · load.same-version]

Loading or installing the version that is already installed succeeds without `-force` or `-dev`, and loads that version again. Automation often reinstalls the same version and expects success.

- A different qualified filesystem supplier can change the loaded files at the same version; [current and historical source readers](../concepts/repositories.g5.md#repo.origin.same-version) distinguish those actions.

## What a failed load keeps


<a id="load.outcome"></a>
[I · load.outcome]

What a failed load keeps depends on how it ran.

| How the load ran | What a failure keeps |
|---|---|
| Normal mode, in a transaction | No module data. The whole load, including dependencies loaded for it, is rolled back. |
| Developer mode, in a transaction | Depends on when it fails. See [developer-mode commits](#load.outcome.dev). |
| `-DNoTransaction=1` or `-DNoJournal=1` | Everything written before the failure. |

- A [refusal](#load.refusal) keeps no module data in any mode. The requirements check is the open exception.
- A process that dies mid-load ends like a failure at that point. The open transaction is rolled back, so the table applies, and with `-DNoTransaction=1` or `-DNoJournal=1` nothing is rolled back and the namespace may be left inconsistent.
- Whether a dependency's own `<Defaults>` transaction flags change this is open: `NoJournal` ([dependency defaults](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults)) and `NoTransaction` ([dependency NoTransaction](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-no-transaction)).
- Files and other changes outside the database follow the [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).
- The [history record](history.g5.md#history.committed) shows whether anything was kept. After a dying process, see [record](history.g5.md#history.record).

- This rollback rule applies to database writes made before the final commit, with journaling enabled. A failed after-install message or cleanup after that commit returns an error with the module kept ([late errors](#load.status.after-commit)).
- Child flags are resolved separately on each activation. An absent parent key permits the child's own default; a defined parent value, including zero, wins.
- Child `NoTransaction` skips its own `TSTART` and leaves an existing parent transaction open. Child `NoJournal` can make its writes survive parent rollback. Their intended policy stays in the linked child questions.
- Class descriptors and usable code are separate outcomes. After two-resource compile failure, ordinary rollback removes both classes.
- Without transactions, or after the developer commit, both descriptors may remain while only the first class runs. This relies on the observed IRIS compiler behavior, not on a proof of the compiler.

- Working assumption: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
- Working assumption: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.


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

A `.modules` folder in the module root serves as a temporary repository for that load's dependencies. IPM removes this temporary repository when loading finishes, even after an error.

- IPM creates none when the folder is missing or another repository already uses that folder. That other repository stays.
- The temporary repository takes the first unused name `ipm-temp-modules-N`.
- A dependency's own `.modules` folder gets the same treatment while that dependency loads.
- Which copy of a dependency wins is owned by [search order](../concepts/repositories.g5.md#repo.search-order). Whether the `.modules` copy should win a tie is [open](../../Verus_verification/open_questions.md#load.dependencies.local-repo.precedence).

- Cache creation failure can leave no row but retain its allocated id in the manager. Cleanup may warn about that missing row (C:D2). At the top-level cleanup call the module can already be committed; at a dependency call the parent rolls back, retaining its first manifest commit only in developer mode.
- A higher candidate version wins before repository order. Equal versions choose the older repository id in the native observations; temporary sort position does not force local priority (C:Q1).

- Working assumption: the copy in the older repository, as built. The changelog says ties go to the repository created first.


<a id="load.status"></a>
[I · load.status]

A load's result tells the caller whether the load took effect. Success means the module and its dependencies are installed and every phase finished.

- An error means the module data was not kept, as described in [what a failed load keeps](#load.outcome).

- As built, an error can also follow the final commit. The caller must read the stored module and committed history result separately from status ([late errors](#load.status.after-commit)); a successful message and a brace-free macro are distinct valid neighbors.


<a id="load.status.after-commit"></a>
[I · load.status.after-commit]

Some work runs after the final commit, so it cannot undo the load. A failure there must not report the kept load as failed.

- This work is: removing the temporary repository, printing the [after-install message](#load.after-install-message), and finishing the history record.
- As built, a failure in this work returns an error although the module stays installed and its history record reads committed. This is a known defect.
- Inside a parent's load, the same failure in a dependency fails the dependency. The parent then rolls back as [a failed load](#load.outcome): fully in normal mode, and back to its first commit in developer mode.


<a id="load.after-install-message"></a>
[I · load.after-install-message]

After a module loads, IPM prints the module's after-install message with its [expressions](../concepts/modules.g5.md#module.parameter.expressions) filled in. A message without a `{` is printed as written.


<a id="load.write-check"></a>
[I · load.write-check]

Before installing files, `install` and `load` check that IPM can write to the target folder.



<a id="load.reload.root"></a>
[D · load.reload.root]

Reload uses an explicit root when present. Without one, it falls back to the module's stored root only in developer mode.

It loads environment configuration before checking whether a root remains empty. With no root it then succeeds without reading module.xml; configuration errors can still fail it first. With a root it attempts to read that manifest and reports a read error if it is missing.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md). Every block on this page is in the formal scope. Native results of stage B2 phase 1 are cited per block by run, as `<time>-<name>`. The full run ID is `20261113T<time>-<name>` under `.g5/Verus_verification/results/native/`, and the [B2 report](../../uplift/ipm-verus-20261113/B/native-report.md) explains each case and defect. Formal (B1) and native evidence per block, with reliance status, is in the [verification coverage map](../../Verus_verification/coverage-map.md). B2 phase 2 and stage C runs are listed in [B/native-replay.md](../../uplift/ipm-verus-20261113/B/native-replay.md) sections 3-7, and defects `D10`-`D20`, `I1` and `I2` in [VV/bugs_issues.md](../../Verus_verification/bugs_issues.md).

### load

- human-provided
- user-facing-behavior
- test-coverage
- Children: [path](#load.path), [shared core](#load.core), [options](#load.options), [refusals](#load.refusal), [same version](#load.same-version), [outcome](#load.outcome), [dependencies](#load.dependencies), [write check](#load.write-check).

Sources: `Main.cls:268-276` (summary and description); `wiki/02` L564-568; vendor manual AIPM. Inventory: CAT-12, INT-LOAD-01; OP-LOAD-01, OP-LOAD-02. Tests: `History.cls` `TestSimpleHistory`; `LoadModuleWithDeps.cls` `TestLoadModuleWithDependencies`. Native evidence: valid completions conform in cases A0, B0, C0 and g1 (`182759Z-cases-a`, `182820Z-cases-b`, `182903Z-cases-c-terminate`, `182832Z-cases-dg`). Suites pass: History 13 of 13 (`175623Z-smoke-history-suite-oras`) and LoadModuleWithDeps 2 of 2 (`182313Z-suite-verify-LoadModuleWithDeps`).

Findings: [P:F-LOAD-4 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-load-4). The linked entry qualifies the caller, reader and providers.

State machines: [Load outcome contracts](../../Verus_verification/models/load-outcomes/manifest.json)

### load.path

- human-provided
- user-facing-behavior
- Expands: [Load](#load).
- Children: [archive](#load.path.archive).

Sources: `Main.cls:278-291` (examples include `.tgz`, `.tar.gz` and git URLs), `:305` (`-branch`); `wiki/02` L599-604; CHANGELOG 0.10.0 (`#474`), 0.10.4 (`#930`). Inventory: INT-LOAD-01; OP-LOAD-02, OP-LOAD-03. The `path` parameter help (`Main.cls:318`) still mentions only a folder. Confirmed product defect D15: `.tar.gz` is refused (stage C case S1, "File not found to obtain module object: module.xml"; neighbour S1N with `.tgz` loads; `20261113T204152Z-stagec`). Cause: it is not recognized as an archive, because only the last extension is compared (`Main.cls:2466-2467`, OBS-L2). Support was added by `280c8d68` for CHANGELOG 0.10.0 `#474` ("compatibility to load '.tar.gz' archives"). Commit `f83b207e` (PR #887) replaced the working two-piece test `$piece(...,".",*-1,*)="tar.gz"` with the single-piece one. The help example `Main.cls:282` still shows a `.tar.gz` load. Observed: a git URL is cloned into `mgr/Temp/<timestamp>/`, the clone's exit code is not checked and the clone is never removed (OBS-L13).

### load.path.archive

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Path](#load.path).

Sources: CHANGELOG 0.10.7 (`#945`), 0.10.11 (`#1183`); `wiki/02` L649-653; comments `Main.cls:2498-2500`. Inventory: INT-LOAD-03; OP-LOAD-02. Test, `.tgz` only: `FileBinaryTar.cls` `TestLoadDestinationDirectory` checks the `<ModuleRoot>/<name>/<version>/` destination and the in-place folder load (PASS 5 of 5, `182337Z-suite-verify-FileBinaryTar`). `InstallModule`, `DeployedItems`, `GlobalsPackaging` and `FileCopy` tests also load `.tgz` files. No test loads a `.tar.gz` file. Observed: an existing `<ModuleRoot>/<name>/<version>/` is deleted and replaced before any version check, and the copy stays after a later failure (OBS-L7, see [rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope)).

### load.core

- human-verified
- architecture-decision
- Expands: [Load](#load).

Sources: `Main.cls:273` and `:407` (load and install descriptions); `wiki/00` L43-56 (phase chains); `General/AbstractHistory.cls:20-27` (source recorded for install). Inventory: INT-LOAD-04; OP-LOAD-05 to OP-LOAD-16 (`%IPM.Utils.Module:LoadNewModule`, `Utils/Module.cls:1152-1364`), OP-LOAD-17 to OP-LOAD-21. Observed: a legacy XML package from a registry is loaded without this path, with no transaction, lifecycle or committed marking (`Utils/Module.cls:175-184`, OBS-L16).

### load.options

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Load](#load).

Sources: `Main.cls:304-315`; CHANGELOG `#704`, `#1260`, `#595`, `#647`, `#961`, `#1290`, `#885`. Inventory: INT-LOAD-15, INT-LOAD-14; OP-LOAD-02. `wiki/02` L589-590 still describes `-synchronous` as active. Native evidence: `-nodev` (g9) sets normal mode, and a plain load (g8) keeps the stored developer mode (`182832Z-cases-dg`). `-create-lockfile`, `-env` and the pip options were not run.

Open questions: [When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?](../../Verus_verification/open_questions.md#dev-mode.sticky)

### load.refusal

- uncertain
- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Load](#load).
- Children: [path](#load.refusal.path), [manifest](#load.refusal.manifest), [downgrade](#load.refusal.downgrade), [update required](#load.refusal.update-required), [requirements](#load.refusal.requirements).

Sources: downgrade message `Utils/Module.cls:1214-1217`; `wiki/03` L260-261 ("validated before module installation"); CHANGELOG 0.10.11 (`#1237`). Inventory: INT-LOAD-02, -09, -10, -11. Tests: `History.cls` `TestDevModeRollbackNotCommitted`, `TestDevMode`. The tests check status and history only, not the installed module afterwards. The requirements refusal is the open exception ([Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)). Reconstruction check R6 and R7: the no-write rule now covers every refusal and the overwrite guard, and is limited to module data. Sources for the timing: PR #1322 body (committed marking "before writing when there is no transaction" follows the version checks); `wiki/09` L133. Confirmed product defects against this rule. D11: manifest refusals under `-DNoTransaction=1` or `-DNoJournal=1` keep module data ([manifest](#load.refusal.manifest)). D12: a no-transaction refusal after the marker reads `Committed=1` with nothing kept (model case B1-C08, `20261113T193952Z-replay-return-b`). D16: `update -path` with another module's archive crashes before the mismatch check ([update refusal](update.g5.md#update.refusal)). D17: an existing relative path is accepted ([path](#load.refusal.path)). Native evidence: version refusals keep no module data in any mode (g3, g6, g7; `182832Z-cases-dg`). The requirements exception is confirmed as B2 D4 ([Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)). Refused loads record no package name (D9, [fields](history.g5.md#history.fields)).

Open questions: [Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)

### load.refusal.path

- human-provided
- user-facing-behavior
- Expands: [Refusals](#load.refusal).

Sources: CHANGELOG 0.10.4 (`#706`: "load now only accepts absolute paths"), 0.10.0 (`#726`: a nonexistent path "now returns an error instead of silently failing"); messages `Main.cls:2447-2452`. Inventory: INT-LOAD-02; OP-LOAD-02. Observed: an existing relative path is accepted, because the check refuses a path only when it is neither existing, nor absolute, nor a URL. An absolute path that does not exist fails later, when IPM reads the manifest. No test. Confirmed product defect D17: an existing relative path loads, with `Committed=1` (stage C case S4R; neighbours S4N1, a missing relative path refused, and S4N2, an absolute path loaded; `20261113T204405Z-stagec2`). Observed cause: the path is refused only when it is not absolute, not a URL and does not exist (`Main.cls:2447-2452`).

### load.refusal.manifest

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).

Sources: `wiki/09` L114-133 (malformed manifest "rejected before any resource was touched"); messages `Utils/Module.cls:1255-1264`, `:341-343`. Inventory: A1 2.3 "Manifest" row; OP-LOAD-05, OP-LOAD-09. Test: `History.cls` `TestDevMode` (malformed manifest, with and without `-dev`). Confirmed product defect D11: with `-DNoTransaction=1`, a two-document manifest (S2) and a manifest with no module definition (S2B) are refused, but the module record, `.ZPM` document and extra class stay, and the record reads committed (`20261113T204152Z-stagec`; model case B1-C52, `20261113T205137Z-replay-c51-c52`). With `-DNoJournal=1`, a validation refusal keeps the `.ZPM` document (B1-C10, `20261113T193952Z-replay-return-b`). The default-mode neighbour S2N keeps nothing. Observed cause: the late checks run after the manifest is saved (`Utils/Module.cls:1251-1260`). Reconstruction check R6 added the wiki's timing.

### load.refusal.downgrade

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).

Sources: message `Utils/Module.cls:1214-1217`; `-force` help `Main.cls:313`, `:441` ("newer version"); PR #887 review r2392741268 by isc-jili ("check for newVer<currVer has always been present"). Inventory: INT-LOAD-09; OP-LOAD-06. Tests: `InstallModule.cls` `TestInstallForceFlag`, `TestDevMode`; `History.cls` `TestDevModeRollbackNotCommitted`. Observed: `-DPermitDowngrade=1` does not lift this refusal, though dependency resolution honors it (OBS-L14). No human source defines `PermitDowngrade` beyond `update` refusing it. Native evidence: g6 (`-force`) and g7 (`-dev`) are refused and leave 3.5.0 installed, with `Committed=0` (`182832Z-cases-dg`, `184230Z-replay-fast`). Suite InstallModule passes (`182208Z-suite-verify-InstallModule`). Consolidated round 5 held-back case (`20261114T034428Z-r5-heldback`, ARM64): `2.0.0-beta` then `1.0.0-rc` installs `1.0.0-rc` (HB2), while `2.0.0` then `1.0.0` (HR2) and `2.0.0-beta` then `1.0.0` (HM2) are refused. Both inherited models encode versions as totally ordered values (G's `ver` field, P's integers per trust item D10), so both predict the refusal for HB2. Model `Verus_verification/consolidated/models/semver.py` (`_heldback`). Defect C:D15.

Extension Verus: [semver.rs::load_refused](../../Verus_verification/consolidated/verus/semver.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D15 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d15). The linked entry qualifies the caller, reader and providers.

### load.refusal.update-required

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).
- Children: [Should `-dev` also lift the update-required refusal?](../../Verus_verification/open_questions.md#load.refusal.update-required.dev-bypass).

Sources: README L62; `-force` help `Main.cls:313`, `:441`; message `Utils/Module.cls:1218-1222`; CHANGELOG 0.10.4 (`#550`); `wiki/08` L3; PR #887 review r2388507362 by isc-tleavitt. Inventory: INT-LOAD-10; OP-LOAD-06. Test: `InstallModule.cls` `TestInstallForceFlag`. Observed: a forced normal-mode install records every update step as already done, so they never run (`Lifecycle/Module.cls:122-127`). Native evidence: g3 refused and kept 1.2.1, and g4 (`-force`) loaded 2.1.1 (`182832Z-cases-dg`).

Open questions: [Should `-dev` also lift the update-required refusal?](../../Verus_verification/open_questions.md#load.refusal.update-required.dev-bypass)

### load.refusal.requirements

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Refusals](#load.refusal).
- Children: [Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing).

Sources: `wiki/03` L260-269; CHANGELOG 0.10.10 (`#106`), 0.10.0 (`#469`), 0.9.0 (`#364`), 0.10.4 (`#874`). Inventory: INT-LOAD-11, CAT-05; OP-LOAD-12. Test: `SystemRequirements.cls` `TestSYSNamespaceRequirement` (status and message only). Native evidence: B1 (normal mode) refused with "requires the %SYS namespace" and kept nothing. Suite SystemRequirements passes (`182820Z-cases-b`, `182248Z-suite-verify-SystemRequirements`).

Open questions: [Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)

### load.same-version

- design-verified

- human-provided
- architecture-decision
- Expands: [Load](#load).

Sources: PR #887 review r2388507362 by isc-tleavitt, 2025-09-29 ("someone's automation ... does an 'install' of the same version every time and makes sure that it succeeds"); r2392741268 by isc-jili ("removed check for newVer==currVer"). Inventory: INT-LOAD-18; OP-LOAD-06. This supersedes issue #903 and the CHANGELOG 0.10.4 `#903` entry, which said a same-version install should fail. No product-authored test covers a same-version install. Earlier native g2 reloads 1.2.1 successfully (`182832Z-cases-dg`); [iteration18](../../uplift/consolidated/iter18/HANDOFF.md) now checks actual qualified repository installs at1.0.0 across two suppliers without uninstall, plus missing-qualifier refusal. [Verus](../../Verus_verification/consolidated/iter18/reloading.rs) retains the declared successful-provider domain and bounds the new version relation to the checked plain release.

State machines: [reloading · iter18](../../Verus_verification/models/iter18-reloading/manifest.json)

### load.outcome

- design-verified

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Load](#load).
- Children: [developer-mode commits](#load.outcome.dev).

Sources: `wiki/00` L125-126, L145-155; CHANGELOG 0.10.11 (`#1237`). Inventory: INT-LOAD-08, A1 2.2 and 2.3; commit-point matrix in A2 2.3; OP-LOAD-05, -07, -11, -14, -15, -16. Tests: `History.cls` `TestFailingPhases` (normal mode), `TestNoTransactionFailure`. The tests check status and history only. Observed: a load nested inside a caller's transaction cannot make its own commits final (P-4, OBS-H16, see [When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?](../../Verus_verification/open_questions.md#history.committed.nested)). Reconstruction check R5 added the dying-process rule from `wiki/00` L147 ("a failure mid-way through is fully rolled back") and L155 (with `NoJournal`, "if the process crashes mid-install, changes cannot be rolled back and the namespace may be left in an inconsistent state"). That IRIS rolls back a dead process's open transaction is premise P-6. Observed: the `.modules` temporary repository and unpacked files stay after a killed load. Native evidence: A1 (normal mode, parent fails after its dependency loaded) leaves the modules and classes absent. C1 and C3 (terminated or killed in Activate) are rolled back. B3 and B4 keep partial writes (`182759Z-cases-a`, `182903Z-cases-c-terminate`, `182927Z-cases-c-sigkill`, `182820Z-cases-b`). Premise P-6 is confirmed and refined (round 1 R1-ONT-14, N-P12, N-P18). After a SIGKILL, IRIS rolls back only when it cleans up the dead process's slot, which took 105 s, 226 s and about 10 minutes in three runs, and other processes can read the uncommitted writes until then (`181724Z-premise-p18-sigkill-r3`, `182927Z-cases-c-sigkill`, `183827Z-cases-c4r`). Compiled classes of a rolled-back normal-mode load were absent afterwards (A1, C1). Round 2 R2-03 and round 3 R3-03: whether a dependency's own `<Defaults>` `NoJournal` escapes the normal-mode rollback is Q-22, and whether its `NoTransaction` suppresses the parent's transaction is Q-23 ([Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults)). Stage C M5 kept a `NoJournal` dependency installed after the parent's rollback (D14, `20261113T204405Z-stagec2`). D13 reports failure after a committed load ([local repository](#load.dependencies.local-repo)).

Extension Verus: [partial_compile.rs::compile](../../Verus_verification/consolidated/verus/partial_compile.rs), [temp_repo.rs::postcommit_cleanup](../../Verus_verification/consolidated/verus/temp_repo.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults), [Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-no-transaction)

State machines: [failkeep · iter64](../../Verus_verification/models/iter64-failkeep/manifest.json)

### load.outcome.dev

- design-verified

- human-verified
- user-facing-behavior
- Expands: [Outcome](#load.outcome).
- Children: [early failure](#load.outcome.dev.early-failure).

Sources: `wiki/00` L126; `wiki/09` L86; maintainer comments `Utils/Module.cls:1310`, `:1316-1317`, `:1336-1337` ("commit load of dependencies (if all went well) and don't wrap reload/compile ... in a transaction; we want to be able to debug!"). Inventory: INT-LOAD-06; OP-LOAD-11, OP-LOAD-14. Test: `History.cls` `TestDevModeFailureAfterCommit`. Observed: a developer-mode dependency commits inside its parent's transaction, so its own commits are not final (P-4). Native evidence: A2 (dependency fails) keeps the parent manifest and drops the dependency. A5 (parent fails after the dependency transaction) keeps the manifest, classes and dependency. C2 and C4R keep the module after termination (`182759Z-cases-a`, `182903Z-cases-c-terminate`, `183827Z-cases-c4r`).

State machines: [failkeep · iter64](../../Verus_verification/models/iter64-failkeep/manifest.json)

### load.outcome.dev.early-failure

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer-mode commits](#load.outcome.dev).

Sources: CHANGELOG 0.10.11 (`#1237`: "A -dev action rolled back before its first commit reads uncommitted"); issue #1237 by isc-dchui ("Everything in that window runs inside a transaction that the catch block rolls back ... regardless of developer mode"). Inventory: INT-LOAD-05, INT-LOAD-09; OP-LOAD-06, OP-LOAD-16. Tests: `History.cls` `TestDevModeRollbackNotCommitted`, `TestDevMode`. Native evidence: g7 (`load -dev` of an older version) is refused with `Committed=0` (`182832Z-cases-dg`).

### load.dependencies

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Load](#load).
- Children: [local repository](#load.dependencies.local-repo).

Sources: `Main.cls:270-271`, `:312`; CHANGELOG 0.10.5 (`#885`); `wiki/00` L126. Inventory: INT-LOAD-14; OP-LOAD-13, OP-LOAD-18, OP-LOAD-20. Tests: `LoadModuleWithDeps.cls` `TestLoadModuleWithDependencies`; `History.cls` `TestWithDependencies`. Native evidence: A0 and A4 conform. A3 (a dependency's Activate fails) fails the parent (`182759Z-cases-a`). Suite LoadModuleWithDeps passes (`182313Z-suite-verify-LoadModuleWithDeps`).

Extension Verus: [loadtx.rs::survived](../../Verus_verification/consolidated/verus/loadtx.rs), [temp_repo.rs::dependency_result](../../Verus_verification/consolidated/verus/temp_repo.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### load.dependencies.local-repo

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Dependencies](#load.dependencies).

- Children: [When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?](../../Verus_verification/open_questions.md#load.dependencies.local-repo.precedence).

Consolidated round 1 (native run `20261114T025102Z-r1-temp-repo`, model `Verus_verification/consolidated/models/temp_repo.py`): TR1 no folder, no repository; TR2 removed after use; TR3 a leftover `ipm-temp-modules-1` makes the new one `-2` and the leftover stays; TR4 an existing repository at the same root is used and kept; TR7 a failed creation leaves no row but prints "Failed to clean up repository" for it (C:D2). The earlier text said the folder is searched first; TR5 shows an equal version in an older repository wins, so that clause moved to the precedence question. Issue I2's "resolve dependencies from that folder first" holds only for versions no older repository offers. Sources: PR #726 body by isc-shuliu; CHANGELOG 0.10.0 (`#726`, `#729`); comment `Main.cls:2523-2524`. Inventory: INT-LOAD-14; OP-LOAD-04. Confirmed product defect D13 (model case B1-C29, `20261113T193952Z-replay-return-b`): a load that committed reports `<INVALID OREF>` when the temporary repository could not be created. Earlier code reading: if the `.modules` folder cannot be cached, the temporary repository is never created and the final cleanup fails after the load has committed (OBS-L12). Issue I2: a load ended by Terminate or SIGKILL leaves its temporary repository registered at first priority, so later installs resolve dependencies from that folder first (six leftovers after the B2 replays, `20261113T210229Z-temp-repos-leftover`). No source covers a process end.

Extension Verus: [temp_repo.rs::cleanup](../../Verus_verification/consolidated/verus/temp_repo.rs), [temp_repo.rs::on_new](../../Verus_verification/consolidated/verus/temp_repo.rs), [temp_repo.rs::skip_create](../../Verus_verification/consolidated/verus/temp_repo.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D2 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d2). The linked entry qualifies the caller, reader and providers.

Open questions: [When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?](../../Verus_verification/open_questions.md#load.dependencies.local-repo.precedence)

### load.status

- agent
- user-facing-behavior
- Children: [after the commit](#load.status.after-commit).

Sources: the policy row "Error reported after the final commit" in the [consolidation plan](../../uplift/consolidated/PLAN.md#policy-assumptions-the-plan-carries) carries both arms' working assumption (G D13 supported; P F-LOAD-4). `wiki/00` L147 (a failure is fully rolled back) implies an error means nothing was kept. No maintainer statement on errors after the commit was found.

### load.status.after-commit

- agent
- user-facing-behavior
- Expands: [Load status](#load.status).

Sources: `Main.cls:2528-2531`, `Utils/Module.cls:138-140`, `:1348-1354`, `:257-259`, `Main.cls:2430`. Native, consolidated round 1 (IRIS 2026.1 build 234U, ARM64): TR7 (top level, temporary repository cannot be created) reports `<INVALID OREF>` with the module installed and its record `success:0 committed:1`; P2 in transaction, no-transaction and developer mode (after-install message does not compile) report the same split; Dtxn and Ddev (a dependency's `.modules` cannot be cached) fail the dependency inside the parent: normal mode removes both, developer mode keeps the parent at its first commit. Runs `20261114T025102Z-r1-temp-repo`, `20261114T025551Z-r1-eval-postcommit`. Earlier: G D13 (x86-64), P F-LOAD-4 (ARM64). Not observed: a failing cleanup of an existing temporary repository, the archive path, analytics.

Extension Verus: [attr_evaluator.rs::after_message](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### load.after-install-message

- agent
- user-facing-behavior
- Expands: [Load](#load).

Sources: `Storage/Module.cls:84`, `:356-361`; `Utils/Module.cls:1354`. Native P1, P3, P4 (`20261114T025551Z-r1-eval-postcommit`): `Done {$namespace} {name}` prints `Done USER pc-ok`; `Done $$$OK` without a brace prints unchanged.

### load.write-check

- human-provided
- user-facing-behavior
- Expands: [Load](#load).

Sources: CHANGELOG 0.10.7 (`#1053`); issue #1053. Inventory: INT-LOAD-16; OP-LOAD-19. Observed: an explicit check exists only for repository archives (`Utils/Module.cls:228-233`). An archive `load` relies on folder-creation errors (`Main.cls:2510`). Not run by B2.

### load.reload.root

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [load.core](#load.core).

Sources and readers: `Lifecycle/Base.cls::%Reload` → `LoadEnvironmentConfig`, `$system.OBJ.Load` and `Storage/Module.cls::ExecutePhases`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `reload`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [moderefresh · iter65](../../Verus_verification/models/iter65-moderefresh/manifest.json), [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)
