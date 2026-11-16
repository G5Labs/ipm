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
- The broader policy question remains [external effects](#transaction.rollback-scope.policy). These observed outcomes do not ratify compensation policy.

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

- How a dependency's own `<Defaults>` interact with its parent's transaction is open for `NoJournal` ([dependency defaults](#transaction.flag-sources.dependency-defaults)) and for `NoTransaction` ([dependency NoTransaction](#transaction.flag-sources.dependency-no-transaction)).

- The effective values are merged by key at each `LoadNewModule` activation. A parent parameter that exists, even zero, overrides the child's default.

- If it is absent, the child's own value applies. Child `NoTransaction` does not cancel an already-open parent transaction; child `NoJournal` disables process journaling during that activation and can escape rollback.

- These are settled source/native facts; the linked questions ask whether to preserve that policy.

<a id="transaction.flag-sources.dependency-defaults"></a>
[Q · transaction.flag-sources.dependency-defaults]

Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?

- Provisional answer: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
- Reason: the wiki says flags embedded in `module.xml` are "applied whenever this module is installed", and that with `NoJournal` "changes cannot be rolled back". These statements are specific to the flags, while the whole-load rollback rule is general.
- Alternative: the parent's transaction governs every nested load, so a dependency's `<Defaults>` `NoJournal` is ignored, or the dependency is refused, while a parent transaction is open. This keeps [the whole-load rollback](../features/load.g5.md#load.outcome).

Unresolved Q-25: [May an escaped NoJournal dependency remain without its own dependencies?](#transaction.flag-sources.dependency-closure)

<a id="transaction.flag-sources.dependency-no-transaction"></a>
[Q · transaction.flag-sources.dependency-no-transaction]

Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?

- Provisional answer: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
- Reason: the wiki gives `NoTransaction`'s purpose as modules in the install that "conflict with an outer wrapping transaction", and says `<Defaults>` apply "whenever this module is installed". Without suppression, the flag of a dependency would have no effect, because the dependency would still run inside the parent's transaction.
- Alternative: the flag affects only the dependency's own load, so the parent's transaction still wraps it and undoes it on failure. A user whose install contains a conflicting module passes `-DNoTransaction=1` for the whole command.

## Developer mode

<a id="dev-mode"></a>
[I · dev-mode]

Developer mode (`-dev`) marks a module as one a developer is actively changing in this namespace.

- IPM sets up `^Sources` so each resource maps back to its file on disk.
- Installer methods and `<Invoke>` code receive the developer mode flag.
- The module is marked as not deployed, so its source stays editable.
- It applies to `load`, `install`, `reinstall`, `update` and lifecycle phase commands. `-nodev` turns it off.
- Modules from a [read-only repository](repositories.g5.md#repo.read-only) always install in normal mode.

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

- Whether `-force` lifts this, and whether it covers `load`, is open ([force and load](#dev-mode.overwrite-guard.force)).

Unresolved Q-24: [May a parent replace a dependency that is already in developer mode?](#dev-mode.overwrite-guard.dependency-policy)

<a id="dev-mode.overwrite-guard.force"></a>
[Q · dev-mode.overwrite-guard.force]

May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?

- Provisional answer: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
- Reason: this follows the product as built since the guard was added, and the `-force` help, which speaks only of newer versions without update steps. The wiki's "Uninstall first, or use `-force`" conflicts with both, and a maintainer must decide which is meant.
- Alternative: let `-force` lift the guard as the wiki says, and decide whether `load` is covered.

<a id="dev-mode.sticky"></a>
[Q · dev-mode.sticky]

When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?

- Provisional answer: yes, it keeps the stored mode.
- Reason: the fix this follows stopped plain loads from turning developer mode on. It did not ask plain loads to turn it off, and `-nodev` exists for that.
- Alternative: a plain load always uses normal mode, following that fix's wording that a module "should only be DevMode if the -dev flag is included".

<a id="transaction.rollback-scope.policy"></a>
[Q · transaction.rollback-scope.policy]

Should a failed package change compensate external effects that the database cannot roll back?

- Evidence: copied files and `module-lock.json` survived native database rollback; arbitrary hooks and remote calls can have further effects.
- Working assumption: database rollback is the existing boundary; external effects need separate receipts and must be reported.
- Alternative: add explicit compensating actions for each external writer.
- Affected claims: load failure atomicity, lock-file location and status, file copy, CPF and history `Committed`.

<a id="dev-mode.overwrite-guard.dependency-policy"></a>
[Q · dev-mode.overwrite-guard.dependency-policy]

May a parent replace a dependency that is already in developer mode?

- Evidence: Source guard checks only the top-level Install parameter; native N-R05-DEVUP replaces the dependency at another version and clears developer mode. Same-version DEVSAME skips it.
- Working assumption: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.
- Alternative: Allow top-level guard only, with a warning when dependency developer mode is cleared.
- Affected claims: install refusal, dependency replacement and developer working copy preservation. Alias Q-24.

<a id="transaction.flag-sources.dependency-closure"></a>
[Q · transaction.flag-sources.dependency-closure]

May an escaped NoJournal dependency remain without its own dependencies?

- Evidence: N-R05-NESTF leaves d1 installed and rolls d2 back because the flat closure loads d2 under parent parameters before d1.
- Working assumption: Require closure consistency as intent; record the observed violating state rather than assuming a repair.
- Alternative: Document temporary inconsistency after failure, or use one load-wide transaction mode.
- Affected claims: dependency satisfaction, mixed-mode rollback and retry. Alias Q-25.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) (`CAT`, `INT`, `Q`), [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md) (`OP`, `OBS`, `P`) and the [coverage map](../../uplift/ipm-verus-20261113/A/coverage-map.md).

### transaction

- human-provided
- user-facing-behavior
- test-coverage
- Children: [rollback scope](#transaction.rollback-scope), [turning transactions off](#transaction.opt-out), [flag sources](#transaction.flag-sources).

Sources: `wiki/00` L125-126, L145-149 ("the entire load/install is wrapped in a transaction; any failure rolls the namespace back"). Inventory: INT-LOAD-08; OP-LOAD-05, OP-LOAD-15, OP-LOAD-16. Test: `History.cls` `TestFailingPhases` (normal-mode failure reads uncommitted). Native evidence (B2 phase 1, run IDs under `.g5/Verus_verification/results/native/`): a normal-mode failure after a dependency loaded leaves the modules and classes absent (case A1, `20261113T182759Z-cases-a`). Rollback premises N-P01 to N-P03 are confirmed (`20261113T181129Z-premises`). Formal and native evidence, with reliance status, is in the [verification coverage map](../../Verus_verification/coverage-map.md).

### transaction.rollback-scope

- agent
- uncertain
- user-facing-behavior
- Expands: [Transactions](#transaction).

Sources: A1 Q-04; `wiki/00` L126; CHANGELOG 0.10.11 (`#1183`, temp folder cleanup). Observed: archive loads delete and replace `<ModuleRoot>/<name>/<version>/` before any version check, and a rollback does not restore it (OBS-L7). Lock files, `LoadedResource` rows in IRISTEMP and the git clone folder also survive a rollback (OBS-L11, OBS-L13, OBS-L15, premises P-2 and P-7). Affects [load outcome](../features/load.g5.md#load.outcome) and [committed side effects](../features/history.g5.md#history.committed.side-effects).


Iteration64: [failkeep.rs](../../Verus_verification/consolidated/iter64/failkeep.rs) `normal_compile_failure_keeps_only_cpf`, `late_failure_keeps_external_effects`, `developer_failure_keeps_database_effects` ([handoff](../../uplift/consolidated/iter64/HANDOFF.md)).
### transaction.opt-out

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Transactions](#transaction).

Sources: `wiki/00` L139-155; `wiki/02` L447, L595, L907, L1367; `Main.cls:628-630` example. Inventory: INT-LOAD-08, CAT-28; OP-LOAD-05, OP-LOAD-07, OP-LOAD-26. Test: `History.cls` `TestNoTransactionFailure`. The help for `reinstall` and `update` lists these flags only through the wiki. Native evidence: with journaling turned off by the `NoJournal` path, a rollback does not revert writes, and the flag is restored afterwards (N-P07, `20261113T181129Z-premises`). B3 and B4 keep partial writes (`20261113T182820Z-cases-b`). Reconstruction check R5 added the crash trade-off from `wiki/00` L155. The wiki states it for `NoJournal`, and it holds for `NoTransaction` because there is no transaction to roll back (`wiki/00` L147). Observed: the journaling flag is restored when the load's journal manager object is released (`Utils/JournalManager.cls`, OP-LOAD-26).

Extension Verus: [loadtx.rs::settle](../../Verus_verification/consolidated/verus/loadtx.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### transaction.flag-sources

- human-verified
- user-facing-behavior
- Expands: [Transactions](#transaction).
- Children: [dependency defaults](#transaction.flag-sources.dependency-defaults), [dependency NoTransaction](#transaction.flag-sources.dependency-no-transaction).

Sources: `wiki/00` L161-180 (command line, `default-modifiers`, `<Defaults>`); `wiki/03` L252-258 (`-D` overrides `<Defaults>`); maintainer comment `Main.cls:1067-1070` ("so they can be overwritten by commands"). Inventory: INT-LOAD-08, CAT-04, CAT-27; OP-LOAD-05 (`Utils/Module.cls:1166-1169`). Observed: a module's own `<Defaults>` can turn transactions off for itself (`Utils/Module.cls:1033-1041`). Confirmed product defect D19: default modifiers are stored in `^%IPM.DefaultModifiers`, which maps to IRISSYS, so defaults set in one namespace apply in every namespace (stage C case X1, `20261113T204152Z-stagec`). A `-DNoTransaction=1` default set anywhere makes every load non-transactional. Reconstruction check R13: no human source sets precedence between default modifiers and `<Defaults>`. Observed: default modifiers enter the command data, which overrides the module's `<Defaults>`, so the order is command line, then default modifiers, then `<Defaults>` (`Main.cls:1067-1069`; `Utils/Module.cls:1166-1169`). Stage B carries this as a condition, not a rule.

Extension Verus: [loadtx.rs::effective](../../Verus_verification/consolidated/verus/loadtx.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### transaction.flag-sources.dependency-defaults

- agent
- uncertain
- architecture-decision
- Expands: [Flag sources](#transaction.flag-sources).

Sources: new question Q-22 from stage C round 2 finding R2-03. Round 3 finding R3-03 limited it to `NoJournal`, the flag that can escape a parent's rollback, and moved `NoTransaction` to [Q-23](#transaction.flag-sources.dependency-no-transaction). `wiki/00` L147 (whole-load rollback), L149 (`NoTransaction` "when some modules in the install conflict with an outer wrapping transaction"), L155 (with `NoJournal`, "changes cannot be rolled back"), L175 (`<Defaults>` "applied whenever this module is installed"); maintainer doc comment `Utils/Module.cls:1033-1040` (example `<Defaults>` with `NoTransaction`); [load outcome](../features/load.g5.md#load.outcome). No maintainer statement on dependencies inside a parent transaction was found. The provisional answer follows the sources specific to `<Defaults>` and keeps the record-survival rule of [survives rollback](../features/history.g5.md#history.survives-rollback).

Observed (stage C cases M5 and M5N, `20261113T204405Z-stagec2`). In M5 a normal-mode parent fails in Activate, and its dependency's `<Defaults>` set `NoJournal=1`. The parent is rolled back, while the dependency stays installed with its class and `.ZPM` document. No `<ROLLFAIL>` occurs, `$TLEVEL` returns to 0 and the journaling flag is restored. The dependency's history record is lost, because it was journaled and rolled back (D1). M5N, the same without `<Defaults>`, rolls everything back and also loses the dependency record (D1). Code: the dependency's `<Defaults>` are merged under the parent's parameters (`Utils/Module.cls:1166-1169`) and turn journaling off inside the open transaction (`:1191-1193`; premise N-P07). B2 files this as D14, "confirmed, decision pending". The model cannot represent this case (`B/model-notes.md`).

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-defaults). The recorded assumption is provisional; no new maintainer approval was found.

### transaction.flag-sources.dependency-no-transaction

- agent
- uncertain
- architecture-decision
- Expands: [Flag sources](#transaction.flag-sources).

Sources: new question Q-23 from stage C round 3 finding R3-03. Main source: `wiki/00` L149 ("Use when some modules in the install conflict with an outer wrapping transaction"). Also L147 (whole-load rollback), L175 (`<Defaults>` "applied whenever this module is installed") and the maintainer doc comment `Utils/Module.cls:1033-1040`, whose example `<Defaults>` sets `NoTransaction`. Checked together with [Q-22](#transaction.flag-sources.dependency-defaults): both provisional answers honor a module's own transaction flags when it is installed as a dependency.

Observed by code reading: a dependency whose `<Defaults>` set only `NoTransaction` skips its own `tstart`, but its writes stay journaled inside the parent's open transaction and are undone with it (`Utils/Module.cls:1191-1196`; premise N-P01). Its no-transaction marker first sets `Committed=1` (`:1226-1228`), and its record is then lost with the parent's rollback (D1). The `NoTransaction` variant of stage C case M5 was not run natively.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-no-transaction). The recorded assumption is provisional; no new maintainer approval was found.

Findings: [G:D28 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d28). The linked entry qualifies the caller, reader and providers.

### dev-mode

- human-provided
- user-facing-behavior
- Children: [source files](#dev-mode.source-files), [keep on failure](#dev-mode.keep-on-failure), [not passed to dependencies](#dev-mode.not-propagated), [overwrite guard](#dev-mode.overwrite-guard), [plain load keeps mode](#dev-mode.sticky).

Sources: `Main.cls:306`, `:434` (`-dev` help); `wiki/00` L107-135; `Main.cls:262` (read-only repositories). Inventory: CAT-30, INT-LOAD-05, INT-LOAD-13; OP-LOAD-10. Observed: a read-only repository forces normal mode only after the update-required check, so `install -dev` from one still skips that check (Q-02).

### dev-mode.source-files

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Developer mode](#dev-mode).

[Iteration 25](../../uplift/consolidated/iter25/HANDOFF.md) and its [source-file contract](../../Verus_verification/consolidated/iter25/developer_sources.rs) connect the reinstall mode request, the existing overwrite guard, `Lifecycle.Base.%Reload` mode/root/module-map writes, document resource-map writes and resource processor metadata/path readers. Constructor extension and directory rules, selected import directories and declared directories remain separate. Typed map values, global module NoFolders state, source-control descriptors, item/sync paths, actual explicit-layout file contents and refusal frames are natively qualified. The older SDK ISC/File readers use a different mapping format and returned empty; no modern external consumer or generic source-control integration is inferred from those attempts.

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

- human-provided
- user-facing-behavior
- Expands: [Developer mode](#dev-mode).
- Children: [force and load](#dev-mode.overwrite-guard.force).

Sources: `wiki/00` L129; message `Utils/Module.cls:109-112`. Inventory: INT-LOAD-12; OP-LOAD-18. Observed: the guard runs only for a top-level `install`, never for IPM itself, and `-DPermitDowngrade=1` skips it. No test.

### dev-mode.overwrite-guard.force

- agent
- uncertain
- user-facing-behavior
- Expands: [Overwrite guard](#dev-mode.overwrite-guard).

Sources: A1 Q-03; `wiki/00` L129 (maintainer-authored, isc-dchui, 2026-05-13); `wiki/05` (names only uninstalling); `-force` help `Main.cls:441`; `Utils/Module.cls:103-114`. The guard has never checked `-force` since it was introduced in commit `d541c5b4` (2022-08-23). Round 1 R1-ONT-05: the wiki line is a conflict for a maintainer, not a documentation error. Not run by B2.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.overwrite-guard.force). The recorded assumption is provisional; no new maintainer approval was found.

### dev-mode.sticky

- agent
- uncertain
- user-facing-behavior
- Expands: [Developer mode](#dev-mode).

Sources: A1 Q-01; CHANGELOG 0.10.4 (`#892`: "no longer sets Developer Mode unless -dev flag is set"); issue #892; commit `f83b207e`. Observed: a load without `-dev` or `-nodev` keeps the stored mode and does not save `Root` or `Repository` (`Utils/Module.cls:1298-1307`, OBS-L3). Affects [load options](../features/load.g5.md#load.options). Native evidence: g8 (plain `load` of a developer-mode module) keeps developer mode, consistent with the provisional answer, and g9 (`-nodev`) turns it off (`20261113T182832Z-cases-dg`).

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.sticky). The recorded assumption is provisional; no new maintainer approval was found.

### transaction.rollback-scope.policy

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [transaction.rollback-scope](#transaction.rollback-scope).

Evidence: Native held-out FileCopy/load receipts in ../../uplift/consolidated/extension/REPORT.md. No product repair applied.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.rollback-scope.policy). The recorded assumption is provisional; no new maintainer approval was found.

### dev-mode.overwrite-guard.dependency-policy

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [dev-mode.overwrite-guard](#dev-mode.overwrite-guard).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-24, source/native cases named in the block; [extension evidence](../../uplift/consolidated/extension/REPORT.md).

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.overwrite-guard.dependency-policy). The recorded assumption is provisional; no new maintainer approval was found.

Findings: [G:I3 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-i3). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-INST-9 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-inst-9). The linked entry qualifies the caller, reader and providers.

### transaction.flag-sources.dependency-closure

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [transaction.flag-sources.dependency-defaults](#transaction.flag-sources.dependency-defaults).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-25, source/native cases named in the block; [extension evidence](../../uplift/consolidated/extension/REPORT.md).

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-closure). The recorded assumption is provisional; no new maintainer approval was found.

Findings: [G:D21 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d21). The linked entry qualifies the caller, reader and providers.
