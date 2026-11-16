# Transactions and developer mode

How IPM protects a namespace when a package change fails, and how developer mode changes that protection. The [load page](../features/load.g5.md#load.outcome) owns the exact points where a load commits. The [history page](../features/history.g5.md#history.committed) owns how the log reports what was kept.

## Definitions

- **Package change**: a `load`, `install`, `reinstall` or `update` of a module, including the dependency loads it starts.
- **Transaction**: an IRIS database transaction. A rollback undoes the database writes made inside it.
- **Normal mode**: a package change without developer mode.
- **Developer mode**: a module setting, turned on with `-dev`, for a module someone is actively changing in this namespace.

## Transactions

<a id="transaction"></a>
[I · transaction]

By default, IPM runs each package change as one IRIS transaction, so a failure rolls back the change's database writes.

- This includes the dependencies loaded for the module.
- [Developer mode](#dev-mode) and [turning transactions off](#transaction.opt-out) change this.
- What a rollback does not undo is open ([rollback scope](#transaction.rollback-scope)).


<a id="transaction.rollback-scope"></a>
[D · transaction.rollback-scope]

A database rollback undoes journaled writes in its transaction. It does not undo filesystem copies, lock files, CPF effects, pip installs or remote calls.

- The database-only interpretation of the history bit is source-backed (P DEC-HIST-5). New native FileCopy tests kept a copied file after a failing load rolled its module record back; a deferred-copy neighbor left no file.

- A direct missing-source copy fails without a copied target file; the successful-copy neighbor leaves its file after database rollback. Directory creation and partial operating-system errors are separate effects.
- [CPF observations](modules.g5.md#module.processor.cpf-effects) now include a real role-creation merge and an invalid-section neighbor on a disposable provider. Child exit and applied configuration are separate outputs; other CPF effects remain external premises.
- A normal-mode rollback also undoes declared `<Mapping>` entries and a web application created during the load. A CPF merge, FileCopy targets and the localization export file written in Activate remain.
- The broader policy question remains [external effects](../../Verus_verification/open_questions.md#transaction.rollback-scope.policy). These observed outcomes do not ratify compensation policy.

- Working assumption: database rollback is the existing boundary; external effects need separate receipts and must be reported.


<a id="transaction.opt-out"></a>
[I · transaction.opt-out]

Users can turn off the transaction for a package change.

- `-DNoTransaction=1` runs without a transaction, so a failure leaves a partial install.
- `-DNoJournal=1` turns off journaling for the process during the change. It also turns off the transaction, because a rollback needs the journal.
- With either flag, if the process crashes during the change, nothing can be rolled back and the namespace may be left inconsistent.
- Both apply to `install`, `load`, `reinstall` and `update`. They are expected to change in IPM 1.0.0.


<a id="transaction.flag-sources"></a>
[I · transaction.flag-sources]

These flags can come from the command line, from the namespace's [default modifiers](settings.g5.md#settings.default-modifiers) or from the module's own `<Defaults>`. A value given on the command line wins over the other two.

- How a dependency's own `<Defaults>` interact with its parent's transaction is open for `NoJournal` ([dependency defaults](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults)) and for `NoTransaction` ([dependency NoTransaction](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-no-transaction)).

- The effective values are merged by key at each `LoadNewModule` activation. A parent parameter that exists, even zero, overrides the child's default.

- If it is absent, the child's own value applies. Child `NoTransaction` does not cancel an already-open parent transaction; child `NoJournal` disables process journaling during that activation and can escape rollback.

- These are settled source/native facts; the linked questions ask whether to preserve that policy.

- Working assumption: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
- Working assumption: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
- Working assumption: Require closure consistency as intent; record the observed violating state rather than assuming a repair.


## Developer mode


<a id="dev-mode"></a>
[I · dev-mode]

Developer mode (`-dev`) marks a module as one a developer is actively changing in this namespace.

- IPM sets up `^Sources` so each resource maps back to its file on disk.
- Installer methods and `<Invoke>` code receive the developer mode flag.
- The module is marked as not deployed, so its source stays editable.
- It applies to `load`, `install`, `reinstall`, `update` and lifecycle phase commands. `-nodev` turns it off.
- Modules from a [read-only repository](repositories.g5.md#repo.read-only) always install in normal mode.

- Working assumption: yes, it keeps the stored mode.


<a id="dev-mode.source-files"></a>
[D · dev-mode.source-files]

A successful developer-mode reinstall records the module root and maps its loaded class to the selected source directory. In the checked directory modules, a following normal reinstall is refused and preserves developer mode, the root, source mappings and callable code.

- Source mappings follow the directory actually selected for loading. Resource path readers describe the configured layout and do not search fallback folders: a class loaded directly from `src` can have a declared path under `src/cls` that does not exist.
- With an explicit `code` directory, the mapping and resource path readers agree on the file under `src/code`; the checked reader opens that file and reads its class and method source.
- This covers two ordinary, nondeployed filesystem modules without dependencies. External source-control plug-ins and IDE interpretation of the mappings, installer or Invoke flags, trigger scheduling and general failure behavior remain outside this qualification. A declared path that differs from a loader fallback is not classified as a product defect here.


<a id="dev-mode.keep-on-failure"></a>
[I · dev-mode.keep-on-failure]

A failed developer-mode change keeps what it already did, so the developer can inspect and fix it. In normal mode, a failed change inside a transaction is rolled back.

- Exception: a developer-mode load that fails before its first commit is rolled back in full ([early failure](../features/load.g5.md#load.outcome.dev.early-failure)).
- Exception: its dependency loads commit together, so one failed dependency rolls back all of them.
- The [load page](../features/load.g5.md#load.outcome.dev) says what is kept at each point.


<a id="dev-mode.not-propagated"></a>
[I · dev-mode.not-propagated]

Developer mode is not passed on to dependencies, but every other `load`, `install` and `update` parameter is. Dependencies a developer only uses should stay in normal mode, so rollback and deployment work as usual.


<a id="dev-mode.not-propagated.local-snapshot"></a>
[D · dev-mode.not-propagated.local-snapshot]

Exception: when a developer-mode load pulls a `+snapshot` dependency from a filesystem repository, that dependency also loads in developer mode.


<a id="dev-mode.overwrite-guard"></a>
[I · dev-mode.overwrite-guard]

A module installed in developer mode cannot be overwritten by a normal-mode install. IPM refuses with "Cannot install 'X' over previously installed in developer mode".

- Whether `-force` lifts this, and whether it covers `load`, is open ([force and load](../../Verus_verification/open_questions.md#dev-mode.overwrite-guard.force)).

- Working assumption: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
- Working assumption: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md).

### transaction

- human-provided
- user-facing-behavior
- test-coverage
- Children: [rollback scope](#transaction.rollback-scope), [turning transactions off](#transaction.opt-out), [flag sources](#transaction.flag-sources).

Sources: `wiki/00` L125-126, L145-149 ("the entire load/install is wrapped in a transaction; any failure rolls the namespace back"). Inventory: INT-LOAD-08; OP-LOAD-05, OP-LOAD-15, OP-LOAD-16. Test: `History.cls` `TestFailingPhases` (normal-mode failure reads uncommitted). Native evidence (B2 phase 1, run IDs under `.g5/Verus_verification/results/native/`): a normal-mode failure after a dependency loaded leaves the modules and classes absent (case A1, `20261113T182759Z-cases-a`). Rollback premises N-P01 to N-P03 are confirmed (`20261113T181129Z-premises`). Formal and native evidence, with reliance status, is in the [verification coverage map](../../Verus_verification/coverage-map.md).

### transaction.rollback-scope

- design-verified

- agent
- uncertain
- user-facing-behavior
- Expands: [Transactions](#transaction).

Sources: A1 Q-04; `wiki/00` L126; CHANGELOG 0.10.11 (`#1183`, temp folder cleanup). Observed: archive loads delete and replace `<ModuleRoot>/<name>/<version>/` before any version check, and a rollback does not restore it (OBS-L7). Lock files, `LoadedResource` rows in IRISTEMP and the git clone folder also survive a rollback (OBS-L11, OBS-L13, OBS-L15, premises P-2 and P-7). Affects [load outcome](../features/load.g5.md#load.outcome) and [committed side effects](../features/history.g5.md#history.committed.side-effects).


Iteration64: [failkeep.rs](../../Verus_verification/consolidated/iter64/failkeep.rs) `normal_compile_failure_keeps_only_cpf`, `late_failure_keeps_external_effects`, `developer_failure_keeps_database_effects` ([handoff](../../uplift/consolidated/iter64/HANDOFF.md)).

Open questions: [Should a failed package change compensate external effects that the database cannot roll back?](../../Verus_verification/open_questions.md#transaction.rollback-scope.policy)

State machines: [failkeep · iter64](../../Verus_verification/models/iter64-failkeep/manifest.json)

### transaction.opt-out

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Transactions](#transaction).

Sources: `wiki/00` L139-155; `wiki/02` L447, L595, L907, L1367; `Main.cls:628-630` example. Inventory: INT-LOAD-08, CAT-28; OP-LOAD-05, OP-LOAD-07, OP-LOAD-26. Test: `History.cls` `TestNoTransactionFailure`. The help for `reinstall` and `update` lists these flags only through the wiki. Native evidence: with journaling turned off by the `NoJournal` path, a rollback does not revert writes, and the flag is restored afterwards (N-P07, `20261113T181129Z-premises`). B3 and B4 keep partial writes (`20261113T182820Z-cases-b`). Reconstruction check R5 added the crash trade-off from `wiki/00` L155. The wiki states it for `NoJournal`, and it holds for `NoTransaction` because there is no transaction to roll back (`wiki/00` L147). Observed: the journaling flag is restored when the load's journal manager object is released (`Utils/JournalManager.cls`, OP-LOAD-26).

Extension Verus: [loadtx.rs::settle](../../Verus_verification/consolidated/verus/loadtx.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### transaction.flag-sources

- uncertain
- human-verified
- user-facing-behavior
- Expands: [Transactions](#transaction).
- Children: [Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults), [Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-no-transaction).

Sources: `wiki/00` L161-180 (command line, `default-modifiers`, `<Defaults>`); `wiki/03` L252-258 (`-D` overrides `<Defaults>`); maintainer comment `Main.cls:1067-1070` ("so they can be overwritten by commands"). Inventory: INT-LOAD-08, CAT-04, CAT-27; OP-LOAD-05 (`Utils/Module.cls:1166-1169`). Observed: a module's own `<Defaults>` can turn transactions off for itself (`Utils/Module.cls:1033-1041`). Confirmed product defect D19: default modifiers are stored in `^%IPM.DefaultModifiers`, which maps to IRISSYS, so defaults set in one namespace apply in every namespace (stage C case X1, `20261113T204152Z-stagec`). A `-DNoTransaction=1` default set anywhere makes every load non-transactional. Reconstruction check R13: no human source sets precedence between default modifiers and `<Defaults>`. Observed: default modifiers enter the command data, which overrides the module's `<Defaults>`, so the order is command line, then default modifiers, then `<Defaults>` (`Main.cls:1067-1069`; `Utils/Module.cls:1166-1169`). Stage B carries this as a condition, not a rule.

Extension Verus: [loadtx.rs::effective](../../Verus_verification/consolidated/verus/loadtx.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [May an escaped NoJournal dependency remain without its own dependencies?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-closure), [Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-defaults), [Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?](../../Verus_verification/open_questions.md#transaction.flag-sources.dependency-no-transaction)

### dev-mode

- uncertain
- human-provided
- user-facing-behavior
- Children: [source files](#dev-mode.source-files), [keep on failure](#dev-mode.keep-on-failure), [not passed to dependencies](#dev-mode.not-propagated), [overwrite guard](#dev-mode.overwrite-guard), [When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?](../../Verus_verification/open_questions.md#dev-mode.sticky).

Sources: `Main.cls:306`, `:434` (`-dev` help); `wiki/00` L107-135; `Main.cls:262` (read-only repositories). Inventory: CAT-30, INT-LOAD-05, INT-LOAD-13; OP-LOAD-10. Observed: a read-only repository forces normal mode only after the update-required check, so `install -dev` from one still skips that check (Q-02).

Open questions: [When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?](../../Verus_verification/open_questions.md#dev-mode.sticky)

### dev-mode.source-files

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Developer mode](#dev-mode).

[Iteration 25](../../uplift/consolidated/iter25/HANDOFF.md) and its [source-file contract](../../Verus_verification/consolidated/iter25/developer_sources.rs) connect the reinstall mode request, the existing overwrite guard, `Lifecycle.Base.%Reload` mode/root/module-map writes, document resource-map writes and resource processor metadata/path readers. Constructor extension and directory rules, selected import directories and declared directories remain separate. Typed map values, global module NoFolders state, source-control descriptors, item/sync paths, actual explicit-layout file contents and refusal frames are natively qualified. The older SDK ISC/File readers use a different mapping format and returned empty; no modern external consumer or generic source-control integration is inferred from those attempts.

State machines: [developer sources · iter25](../../Verus_verification/models/iter25-developer-sources/manifest.json)

### dev-mode.keep-on-failure

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer mode](#dev-mode).

Sources: `-dev` help `Main.cls:306`, `:434` ("transactions will not be rolled back if installation fails"); issue #363 comment by isc-tleavitt, 2025-09-15 ("we don't remove these entries when installing in dev mode (to support debugging), but will in non-dev mode"); `wiki/00` L125-126; CHANGELOG 0.10.4 (`#363`), 0.10.11 (`#1237`). Inventory: INT-LOAD-05. Test: `History.cls` `TestDevModeFailureAfterCommit`. The `-dev` help for `reinstall` and `update` lacks the rollback sentence. Reconstruction check R8 added the dependency exception from the maintainer comment `Utils/Module.cls:1336-1337` ("commit load of dependencies (if all went well)"). Native evidence: A2, A5, C2 and C4R keep what developer mode had committed (`20261113T182759Z-cases-a`, `20261113T182903Z-cases-c-terminate`, `20261113T183827Z-cases-c4r`).

### dev-mode.not-propagated

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer mode](#dev-mode).
- Children: [local snapshot exception](#dev-mode.not-propagated.local-snapshot).

Sources: CHANGELOG 0.10.5 (`#316`, `#1036`); `wiki/00` L133; commit `0e25c23d` comment "Propagate all params except developer mode". Inventory: INT-LOAD-07; OP-LOAD-20. Tests: `LoadModuleWithDeps.cls` `TestParamPropagation`; `Update.cls` `TestDevModePropagation`. Suspected gap, not run natively by B2: `update <m> -dev` from a registry leaves the top-level module itself in normal mode (OBS-L4). Native evidence: A4 (`load -dev` with a dependency) leaves the dependency in normal mode (`20261113T182759Z-cases-a`). Suites LoadModuleWithDeps and Update pass (`20261113T182313Z-suite-verify-LoadModuleWithDeps`, `20261113T182433Z-suite-verify-Update`).

### dev-mode.not-propagated.local-snapshot

- human-verified
- architecture-decision
- Expands: [Not passed to dependencies](#dev-mode.not-propagated).

Sources: maintainer comment `Utils/Module.cls:131-135` ("also load the snapshot version of that dependency in developer mode. This doesn't make sense if we're loading the dependency from a remote server"). Inventory: INT-LOAD-07; OP-LOAD-18. No test.

### dev-mode.overwrite-guard

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Developer mode](#dev-mode).
- Children: [May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?](../../Verus_verification/open_questions.md#dev-mode.overwrite-guard.force).

Sources: `wiki/00` L129; message `Utils/Module.cls:109-112`. Inventory: INT-LOAD-12; OP-LOAD-18. Observed: the guard runs only for a top-level `install`, never for IPM itself, and `-DPermitDowngrade=1` skips it. No test.

Open questions: [May a parent replace a dependency that is already in developer mode?](../../Verus_verification/open_questions.md#dev-mode.overwrite-guard.dependency-policy), [May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?](../../Verus_verification/open_questions.md#dev-mode.overwrite-guard.force)
