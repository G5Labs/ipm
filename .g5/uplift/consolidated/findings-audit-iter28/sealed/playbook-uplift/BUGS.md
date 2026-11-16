# IPM defects found by the playbook uplift

Baseline: IPM `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (tag v0.10.11-beta.28). The IPM checkout was **not**
changed; every defect below is documented, not fixed. Native checks ran on IRIS Community 2026.1 (build 234U, ARM64),
with the load and history scenarios repeated on 2025.3. Paths are relative to the IPM checkout (`src/cls/IPM/...`
is shortened to the class path). Full write-ups, with every observation name, are in `findings/*.md`; the human
choices that some fixes wait on are in `QUESTIONS.md`; designs that remove whole groups of these defects are in
`DESIGN-SIMPLIFICATIONS.md`.

**Revision 2026-11-13 (round 4).** All 44 open intent decisions were researched (code, git history, the IPM wiki,
GitHub issues and PRs, IPM's CI at this commit, and new IRIS runs) and resolved: 24 by an authoritative source, 20 by
a stated assumption (`decisions/*.md` "Resolutions", `decisions/research/*.md`). Judged against those intents, 15 new
defects were added, 5 earlier code defects turned out to be intended behaviour with stale or missing documentation,
and one was withdrawn as intended. A defect whose intent rests on an **assumption** is marked † below; if the IPM
maintainers choose another option, that row may stop being a defect (`QUESTIONS.md` lists the assumptions).

## How to read this file

**Evidence** (strongest first; a finding lists every kind it has):

| Mark | Meaning |
|---|---|
| **Proved** | A Verus lemma proves that a reachable run of the hand-written model of the IPM code breaks the intended rule (a *witness*). The model's faithfulness is trusted, not proved (TRUST.md). |
| **IRIS** | Seen on a real IRIS instance running the pinned IPM; raw logs kept (`validation/logs/`), observation records in `evidence/iris-observations.json`. |
| **Patched probe** | Seen on IRIS with a one-line probe patch in a throwaway container copy, to reach a window that cannot be reached from outside (diffs in `validation/iris/n2/patches/`). |
| **Model-checked** | Found by the Python explorers (bounded). |
| **Test run** | An IPM test class run here shows it. |
| **Inspection** | Read in the code only. |

**Severity** is this run's judgement, for the IPM maintainers to revise:
- **High**: silent wrong result on a common path that loses or exposes data, or removes something the operator needs.
- **Medium**: wrong result or misleading report on a less common path, or one the operator can notice and repair.
- **Low**: cosmetic, rare, or the operator gets a clear (if wrong) failure.

## Summary

80 finding IDs: **65 code defects**, 11 documentation defects (help, wiki, comments or README disagree with intended
code), 2 test gaps, 1 withdrawn as intended (F-HIST-10), 1 refuted (F-REPO-6). Of the 65 code defects, 64 were seen
on IRIS (2 only with a probe patch, 1 with a simulated legacy install) and 1 only in an IPM test run; 32 also have a
proved witness.

| Module | High | Medium | Low | Documentation | Test gap | Withdrawn / refuted |
|---|---|---|---|---|---|---|
| 01 Package loading | 0 | 2 | 2 | | | |
| 02 Transaction history | 0 | 5 | 2 | 2 | | 1 |
| 03 Install and update | 2 | 6 | 0 | 2 | 1 | |
| 04 Dependencies | 0 | 1 | 5 | 2 | 1 | |
| 05 Uninstall | 3 | 5 | 3 | | | |
| 06 Registries and settings | 1 | 6 | 3 | 1 | | 1 |
| 07 Namespace ownership | 0 | 7 | 4 | 1 | | |
| 08 Resources and packaging | 0 | 3 | 3 | 2 | | |
| 09 Developer sync | 0 | 1 | 1 | 1 | | |
| **Total** | **6** | **36** | **23** | **11** | **2** | **2** |

### The six High defects

| ID | One line |
|---|---|
| F-INST-1 † | `update` reports success and runs no update step when the update lock is busy. (Intent: the PR #887 authors' own removed test asserted that a concurrent update fails.) |
| F-INST-6 | A helper class named like `V3Helper` in the update package hides step class `V3`; `update` reports success without running its steps. |
| F-UNIN-1 | A blocked uninstall (a dependent exists) still deletes the module's web and CSP applications, then reports success. |
| F-UNIN-4 | If any dependent's dependency graph cannot be built, uninstall removes a module that dependent still needs. |
| F-UNIN-8 | `uninstall <module> -r` on a module that lists `zpm` as a dependency removes IPM from the namespace. |
| F-REPO-10 | The publish flag is ignored when an older registry of the same kind exists: `publish` goes to the oldest one. A regression: commit `3174d72d` (Aug 2024) dropped `ORDER BY PublishOrder, ID`. |

F-REPO-3 was High; the fallback it described is intended (PR #332), so only its stale help text remains (Low,
documentation).

## Shared root causes

Count root causes, not findings (playbook 5c). Most defects fall into a few patterns; `DESIGN-SIMPLIFICATIONS.md`
gives one design per pattern.

| Cause | What goes wrong | Findings |
|---|---|---|
| RC-1 History rows written inside open transactions | The durable history row (`%IPM.General.History`, namespace database) is inserted and updated at whatever `$TLEVEL` is open, so a rollback removes it. Only the IRISTEMP copy is immune, and `%OnClose` deletes that. | F-HIST-1, F-HIST-2 (F-HIST-3, a caller's rollback, is intended and only undocumented) |
| RC-2 Committed lives only in IRISTEMP until the log closes | `SetCommitted` writes IRISTEMP; the durable row learns it only at `%OnClose`. | F-HIST-2, F-HIST-7; a graceful `iris stop` during a load (`r4-hist-graceful-stop-txn-after`) |
| RC-3 Auto-purge quotes a bound value | F-HIST-6 | F-HIST-6 |
| RC-4 An instance crash runs no close code and wipes IRISTEMP | The history design assumed `%OnClose` runs at every crash. | F-HIST-8 |
| RC-5 A `%Status` is dropped | Called with `do`, overwritten with `$$$OK`, or replaced by `$$$OK` on a timeout. | F-INST-1, F-NSP-1, F-NSP-11, F-RES-1, F-REPO-8, F-LOAD-1, F-HIST-6 (per-namespace errors), F-HIST-9, F-SYNC-3 (a no-op reported as SUCCESS) |
| RC-6 Act first, check later | Side effects run before the check that may cancel the command. | F-UNIN-1, F-LOAD-3, F-UNIN-6 (skip reported as success), F-DEP-8 (lock file before Activate), F-NSP-12 (dev-mode commit before the system check) |
| RC-7 Identity by typed string | Guards compare the typed name rather than the resolved module. | F-UNIN-3, F-UNIN-8, F-DEP-3 |
| RC-8 Wrong argument slot | A positional argument lands in the wrong parameter. | F-NSP-6, F-NSP-8 |
| RC-9 Selection without a total order | A query or subscript order decides which registry wins. | F-INST-4, F-REPO-10, F-NSP-7 |
| RC-10 Documented option accepted but not implemented | The option is parsed, stored or shown, and nothing reads it. | F-UNIN-2, F-REPO-1, F-REPO-2, F-DEP-1, F-RES-2 |
| RC-11 Several code paths decide developer mode and step seeding | The typed flag and the stored flag are read in different places, and one place overrides both. | F-INST-5, F-INST-8, F-INST-9, F-INST-10 (update-step seeding and the dev-mode guard each read a different flag) |
| RC-12 Work after the final commit can fail | `LoadNewModule` evaluates the after-install message (and, by reading, cleans `.modules` and collects analytics) after `TCOMMIT` + `SetCommitted`, inside the same `try`. | F-LOAD-4 |

---

## 01 Package loading

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-LOAD-1 | Low | A failed `git clone` in a URL load is not reported. The load then fails with "No module.xml" and writes a history record with no module name. | IRIS (`nc-gap-P5-url-missing`) | `Main.cls#L2358`, `Utils/Module.cls#L2409` (clone return code unchecked) |
| F-LOAD-2 | Medium | Every URL load leaves its clone in `mgr/Temp/<timestamp>/<repo>/`, and that temporary folder becomes the module's Root, so later `reload`/`sync` read from a temp folder. | IRIS (`nc-gap-P5-url-ok-txn`, `-ok-dev`) | `Main.cls#L2352-L2359` |
| F-LOAD-3 | Medium | A failing archive load of an installed name and version replaces the installed module's folder before any check. The database rolls back, so the installed record's folder no longer matches its compiled code, and history says Committed = 0. | IRIS (`nc-gap-P4-txn-bad-archive-same-version`) | `Main.cls#L2507-L2513` |
| F-LOAD-4 | Low | A load can fail **after** its final commit: an after-install message with `{` and a `$$$` macro that does not compile throws in `%EvaluateMacro`. The command returns an error and history records failure, while the module and its compiled classes stay installed (committed = 1). Seen in transactional, no-transaction and developer modes. Also leaves `%IPM.Generated.MacroEval` defined. | IRIS (`r4-hist-post-commit-throw-r4h-msg2-txn`, `-notxn`, `-dev`) | `Utils/Module.cls#L1348-L1354`, `#L2781` |

Resolved intents: Committed covers database changes only (DEC-HIST-5, PR #578), so F-LOAD-3 is a defect of the load itself, not of history; a plain `load` keeps developer mode (DEC-LOAD-2).

## 02 Transaction history

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-HIST-1 | Medium | Dependency history records vanish when the parent load rolls back, though `AbstractHistory.cls#L4` says history is kept apart from rollbacks. A failing dependency's own record is lost too. | Proved (`g_dependency_records_lost_unless_fixed`); IRIS (`ipm-dep-parent-fails`); model-checked (109 runs) | `Utils/Module.cls#L64-L68` (log created inside the parent's transaction, `#L1195`), rollback `#L1358-L1361` |
| F-HIST-2 | Medium | A developer-mode load that crashes while loading dependencies keeps its module record, but history says Committed = 0 and names no package. | Proved (`g_dev_crash_dishonest_unless_fixed`); IRIS (`ipm-halt-dev-crash-in-dependency`) | `Utils/Module.cls#L1311-L1313` (SetCommitted in IRISTEMP only), next `TSTART` `#L1318` |
| F-HIST-5 | Low | A refused or failed command that ended after reading the manifest leaves a record with no module name (wiki page 09 allows placeholders only for a process that dies mid-action). | IRIS (`ipm-dev-downgrade-refused`, `ipm-halt-txn`, `ipm-terminate-txn`); model-checked | `Main.cls#L2420` (`LoadInit("")`), `SetName` only at `Utils/Module.cls#L1293`, after the guards at `#L1213-L1222` |
| F-HIST-6 | Medium | History auto-purge never deletes anything: the cut-off date is quoted inside a bound parameter, so the filter matches no row. The command reports success. History grows without bound although `HistoryRetain` is set. | IRIS (`ipm-history-autopurge`, `ipm-history-purge-filter-cause`) | `History.cls#L173` (quotes), `#L107-L109` (already binds) |
| F-HIST-7 | Low | A crash between the outer `TCOMMIT` and `SetCommitted` leaves Committed = 0 for a committed load. A graceful `iris stop` during a load did the same: IRIS waited for the commit, then forced the job down after about 5 minutes; the record stayed `load//`, Committed = 0, not finalized (cause not identified). | Proved (`g_window_crash_dishonest`); patched probe p8a (`nc-gap-P8-commit-window-after-commit`); IRIS graceful stop (`r4-hist-graceful-stop-txn-after`) | `Utils/Module.cls#L1348-L1350` |
| F-HIST-8 | Medium | After an IRIS restart, a developer-mode or no-transaction load that kept its changes reads Committed = 0, with no name, no end time and an unlocked row that SQL can still edit. | Proved (`g_instance_crash_dishonest_unless_fixed`); IRIS (`nc-gap-P1-docker-dev-after`, `-notxn-after`) | RC-4; `AbstractHistory.cls#L120-L125`, `HistoryTemp.cls#L62-L72` |
| F-HIST-9 | Medium | A phase that fails through an `<Invoke>`'s returned status stays open in history with placeholder error `ERROR #00`. | IRIS (`nc-gap-P3-n2-ph-*`, all modes) | `Storage/Module.cls#L669`, `#L671` (only the catch calls `FinalizePhase`) |

Documentation, not code (intent resolved):
- **F-HIST-3.** A command run inside a caller's transaction loses its history if the caller rolls back. Intended
  (DEC-HIST-4, assumption): the work and the record follow the caller; the wiki's "outside the transaction" sentence
  needs that caveat.
- **F-HIST-4.** Answering "Commit" at the crash prompt keeps a half-done load marked Committed = 0. An operator error
  to document (DEC-LOAD-1, assumption).

Withdrawn as intended: **F-HIST-10** (`history delete` of a running record wins; DEC-HIST-6, assumption). A row
written inside an open transaction cannot be deleted until it ends (SQLCODE -110), which is acceptable.

## 03 Install and update

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-INST-1 † | **High** | `update` reports success without running update steps when the update lock is busy for 3 s. No message unless verbose. The steps stay due, and nothing prompts a rerun. | Proved (`inst_witness_busy_lock_reports_success`); IRIS (`nc-inst-3-update-lock-busy`, `-3b-...`) | `Storage/Module.cls#L105-L108` (`return sc` with `sc = $$$OK`); `inc/IPM/Common.inc#L105` |
| F-INST-4 | Medium | Between registries that offer the same version, the registry created first wins; `OverriddenSortOrder` has no effect. Deleting and re-adding a registry changes which copy installs. | IRIS (`nc-inst-10b-registry-tie-break`, `-10c-...`) | `Repo/Manager.cls#L148` (`versions(...,repoID,name)` read back in subscript order; SQL order at `#L70` lost) |
| F-INST-5 | Medium | `update -dev` from a registry does not run in developer mode, and a plain `update` of a module stored in developer mode clears it. Only `update -dev -path` keeps it. | Proved (`inst_witness_dev_update_from_registry_rolls_back`); IRIS (`nc-inst-4e-...`, `nc-dep-19f-...`, `nc-inst-7-...`) | `Utils/Module.cls#L128-L136`, `#L307-L310`, `#L293-L296` |
| F-INST-6 | **High** | A class of the update package named `V<n><something>` (e.g. `V3Helper`) replaces step class `V<n>` in the step list; update then reports success without running `V<n>`'s steps. | Proved (`inst_witness_helper_class_hides_steps`); IRIS (`nc-inst-model-class-number-collision`) | `General/Update/VersionBase.cls#L34-L60` (`$zstrip(...,"*E'N")`, later row overwrites) |
| F-INST-7 | Medium | A class of the update package whose name starts with `V` but has no digit (e.g. `Validator`) makes every update of the module fail with `<SUBSCRIPT>`, naming no class. | Proved (`inst_witness_class_without_number_fails`); IRIS (`nc-inst-model-class-without-number`) | `VersionBase.cls#L51-L52` |
| F-INST-8 † | Medium | A developer-mode install (or a plain `load` of a module stored in developer mode) seeds no update steps, so the next update reruns every step from V1, including steps of versions already in place. | Proved (`inst_witness_dev_install_leaves_steps_due`); IRIS (`nc-inst-2b-dev-fresh-then-update`, `nc-inst-2-dev-install-then-update`, `r4-inst-u22-dev-kill-before-record-save`) | `Lifecycle/Module.cls#L122` |
| F-INST-9 † | Medium | Install silently replaces a needed module that is in developer mode and clears its developer mode; the guard covers only the named module. Wiki Concepts: a dev-mode module "cannot be overwritten by a non-dev install". | IRIS (`nc-inst-8-dev-dependency-replaced`) | `Utils/Module.cls#L109`, `#L310`, `#L317-L319` |
| F-INST-10 † | Medium | An update that brings in a module for the first time runs all of that module's update steps instead of seeding them as a fresh install does. | Proved (`inst_fresh_update_runs_all`); IRIS (`nc-inst-6-dep-added-by-update` vs `-6b-fresh-install-control`) | `Utils/Module.cls#L308`, `#L316`; `Lifecycle/Module.cls#L122` |

Documentation and test gaps in this module:
- **F-INST-2 (documentation + test gap).** `VersionBase.cls#L18` says primary-only steps run "when the instance becomes
  the primary member"; no code reacts to a role change, and none is intended yet (wiki page 08: mirroring support
  "will be added in future versions"; DEC-INST-7), so the comment is wrong. `TestUpdateToSameVersionTwiceNoMirror` returns early on a mirror member
  without a skip mark, so it would read as a pass. Not checkable here: Community Edition refuses mirroring. DEC-INST-7.
- **F-INST-3 (gap).** `TestUpdateAfterInterruptedUpdate` (`Update.cls#L130`) kills the job at an uncontrolled point and
  only asserts the retry returns OK; the "Assert it completed" comment (L154) has no assertion. `SingleFailedUpdateTest`
  asserts only a not-OK status; `AssertUpdateStepsExist` checks rows exist, not that steps ran.
  `TestAllErrorModulesNoMirror` has no mirror guard despite its name. Native checks now cover what the tests leave out.
- **F-INST-11 (documentation).** The `-dev` bypass of the update-package guard is intended (DEC-INST-1: `TestDevMode`
  comment, PR #887, issue #550) but the README, the `-dev` help and the guard's error text name only `-force`; the
  `reinstall` description says "latest" although reinstall intentionally puts back the installed version (DEC-INST-4).

Also resolved here: update steps are delivered at least once and must be safe to rerun (DEC-INST-5, wiki page 08:
"Idempotent steps"), so the observed reruns after a failed update are intended.

## 04 Dependencies

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-DEP-1 | Low | `PermitDowngrade` is half implemented: the resolver drops the "≥ installed" bound and picks an older version, then the load refuses any downgrade. | Proved (`dep_witness_permit_downgrade_refused`); IRIS (`nc-dep-12-permit-downgrade`) | `Storage/Module.cls#L1157`, `Utils/Module.cls#L1214`, `#L103` |
| F-DEP-2 | Low | Lock-file dependencies are written in name order, not dependency order, although `LockFile.cls#L24` promises least-dependent first. Latent (no `ci` command yet); fixtures happen to sort correctly. | Proved (`dep_witness_lockfile_name_order`); IRIS (`nc-dep-13-lockfile-order`) | `LockFile.cls#L25`, `#L69` (array keyed by name) |
| F-DEP-3 | Low | `list-dependents` with a capitalised name (as in the help examples) prints nothing. | Proved (`dep_witness_list_dependents_case`); IRIS (`nc-dep-16-list-dependents-case`) | `Main.cls#L3154`; `Utils/Module.cls#L482`, `#L529` |
| F-DEP-7 | Medium | The resolver is greedy (newest first, no backtracking past one level) and fails on solvable inputs, although its comment says "with backtracking". Known upstream: open issue #1299, fix PR #1303 open. | Proved (`dep_witness_greedy_rejects_solvable`); IRIS (`nc-dep-11-greedy-resolution`, `-11b-solvable-with-pin`); explorer (2,040 of 70,656 instances) | `Storage/Module.cls#L1248-L1291` |
| F-DEP-8 | Low | A failed `install -create-lockfile` leaves a new lock file behind (written before the module's own Activate, outside the transaction). PR #963's author said a failed install leaves none. | IRIS (`nc-dep-15-lockfile-failed-install`) | `Utils/Module.cls#L1415-L1421` |
| F-DEP-9 † | Low | A needed module loaded from a folder (no registry) makes lock-file creation throw, so no file is written; the base module's empty registry is already handled. | IRIS (`nc-dep-14-lockfile-folder-dependency`) | `LockFile.cls#L72`, `#L115` |


Stale comments (no behaviour change needed, code and tests agree):
- **F-DEP-4.** `Storage/Module.cls#L892` says an empty phase list means all scoped dependencies count; `HasScope`
  (`#L834-L855`) and the test say the opposite.
- **F-DEP-5.** `BuildDependencyGraph.cls#L232` (test) says exact-versus-range has a known Intersect edge case; the same
  commit fixed it.

Test gap:
- **F-DEP-6.** `DependencyResolution.TestEverything` asserts nothing (annotation extraction commented out, L17/L21);
  `LockFile.Test05...` swallows exceptions; `Test08ModuleDependenciesOutOfOrder` is empty. No IPM test covers
  `list-dependents`, the "Requested version ... does not satisfy" error, lock-file creation on update, or rollback
  after a failed coordinated update (the last three are now observed natively).

## 05 Uninstall

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-UNIN-1 | **High** | A blocked uninstall (a dependent exists, or global scope) still deletes the module's web and CSP applications and runs `Clean`/`Unconfigure` hooks, then prints `Clean SUCCESS`. `<module> clean` does the same to the module it runs on; so does `uninstall -r` for a need it keeps. | Proved (`unin_witness_blocked_deletes_webapp`, `_clean_own_module`, `_recurse_skipped_dep`); IRIS (`nc-unin-1-blocked-uninstall`, `nc-unin-2-clean-own-module`, `nc-repo-model-unin-recurse-skipped-dep`) | `Storage/Module.cls#L587-L600` (OnBeforePhase) runs before `Lifecycle/Base.cls#L258-L302` (CheckBeforeClean) |
| F-UNIN-2 | Medium | `uninstall -purge` deletes no table data. Nothing reads the `Purge` parameter; `%Clean` deletes classes without the extent flag `e`. (An installer can read `pVars("Purge")`.) | IRIS (`nc-unin-3-uninstall-plain`, `-purge`; IRIS fact `nc-unin-env-obj-delete-e-flag`) | `Main.cls#L505`; `Lifecycle/Base.cls#L387`, `#L554-L561` |
| F-UNIN-3 | Medium | The self-uninstall guard (mapping check, module list, full-cleanup question, shell exit) runs only when the typed name is exactly `zpm`. `uninstall IPM`/`ipm`/`ZPM` removes IPM's classes with no question and leaves the shell running. | Proved (`unin_witness_external_name_bypass`); IRIS (`nc-unin-self-external-name-IPM`; mapped namespaces refuse, `nc-unin-4-*`) | `Main.cls#L2712` |
| F-UNIN-4 | **High** | Uninstall without `-force` removes a module that a dependent still needs if that dependent's dependency graph fails to build (e.g. a registry is missing): graph errors are ignored. | Proved (`unin_witness_unresolvable_dependent`); IRIS (`nc-unin-5-unresolvable-dependent`) | `Utils/Module.cls#L460-L499`, `#L615-L622` (`pErrorList` unread) |
| F-UNIN-5 | Medium | A global resource declared only by name (`<Resource Name="X.GBL"/>`, the usual form) keeps its data after uninstall; only resources with the `Global` attribute are killed. | Proved (`unin_witness_global_kept`); IRIS (`nc-unin-6-global-resources`) | `ResourceProcessor/Default/Global.cls#L76-L80` |
| F-UNIN-6 | Medium | A blocked uninstall returns OK, prints `Clean SUCCESS` and logs `uninstall/success` while the module stays. The code comment at `Lifecycle/Base.cls#L276` says it returns an error; IPM's own test asserts success. | Proved (in `unin_witness_blocked_deletes_webapp`, `_recurse_skipped_dep`); IRIS (`nc-unin-1-blocked-uninstall`) | `Lifecycle/Base.cls#L290-L301`; `Storage/Module.cls#L746-L755`. Intent (DEC-UNIN-1, issue #58): block with an error naming the dependents; IPM's own skipped test `TestUninstallDependencyDoesError` asserts that |
| F-UNIN-7 | Low | `orphans` with no `-type` lists IRIS-shipped and mapped interoperability items (BPL/DTL views of `Ens*`, X12 schemas, CSP defaults) that no module could own. `-type CLS` is correct. | IRIS (`nc-unin-10-orphans`, `-detail`) | `Utils/Module.cls#L2152-L2153` |
| F-UNIN-8 | **High** | `uninstall <module> -r` on a module that lists `zpm` among its dependencies (and is the only one) removes IPM's classes, then fails with `<CLASS DOES NOT EXIST>`; both records stay and IPM must be reinstalled. | Proved (`unin_witness_recurse_removes_ipm`); IRIS (`nc-repo-model-unin-recurse-ipm-dependency`) | `Lifecycle/Base.cls#L431-L490`, `#L472` |
| F-UNIN-9 | Low | A dependency removed by `uninstall -r` keeps its stored manifest in `^IPM.StudioDoc.ModuleStreamD`; only the top module's manifest is deleted. | Proved (`unin_witness_recurse_leaves_manifest`); IRIS (`nc-repo-model-unin-recurse-removed-dep`) | `Lifecycle/Base.cls#L472` vs `StudioDocument/Module.cls#L205-L227` |
| F-UNIN-10 | Medium | A full cleanup leaves the folder-registry cache index (`^IPM.Repo.Filesystem.CacheI`, `CacheS`). After IPM is reinstalled, adding a folder registry fails with `ERROR #5809` until the globals are killed by hand. | IRIS (`nc-gap-P7-fscache-stale` vs `-repaired`, `nc-gap-P7-*-post-probe`) | `Lifecycle/Module.cls#L41-L43` (kills only `CacheD`) |
| F-UNIN-11 | Low | `zpm clean` prints "WARNING: N module(s) orphaned" while removing nothing, and goes down the self-uninstall path (by reading, its after-phase hooks do not run). Intended: an ordinary level-0 Clean (DEC-UNIN-2). | IRIS (`nc-unin-9-clean-on-ipm`); inspection | `Lifecycle/Module.cls#L27-L81`; `Storage/Module.cls#L630-L655` |

## 06 Registries and settings

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-REPO-1 | Medium | `-token-auth-method` is ignored for remote registries: the token is always sent as an `apikey` header, so a registry expecting `bearer` rejects it. | IRIS (`nc-repo-4-token-auth-method`) | `Repo/Remote/Definition.cls#L60-L115`; `Http/PackageService.cls#L59-L69` |
| F-REPO-2 | Medium | `-sslconfig` and `-publish-url` are documented, accepted and ignored. The SSL configuration is always one named after the host, created with defaults. | IRIS (`nc-repo-5-sslconfig-publish-url`) | `Http/Definition.cls#L38`, `#L46`; `Http/PackageService.cls#L18-L27` |
| F-REPO-4 | Low | `config delete` restores the first value ever set, not the factory default, for keys the installer does not seed (`TerminalPrompt`, `PublishTimeout`, `HistoryRetain`). | Proved (`repo_witness_delete_restores_first_value`); IRIS (`nc-repo-6-config-delete`) | `UniversalSettings.cls#L166-L177` |
| F-REPO-5 | Medium | `default-modifiers` are documented per namespace but stored in `^%IPM.DefaultModifiers`, which IRIS keeps in IRISSYS: setting `-DNoTransaction=1` in one namespace affects every namespace. | Proved (`repo_witness_dm_shared`); IRIS (`nc-repo-7-default-modifiers-namespaces`) | `inc/IPM/Common.inc#L31`; `Main.cls#L1682-L1696` |
| F-REPO-7 | Medium | A disabled registry is still contacted (credentials sent) by `search` and by `publish -r`. Install and unpublish refuse it. | Proved (`repo_witness_disabled_registry_used`); IRIS (`nc-repo-2-disabled-registry`) | `Main.cls#L2047`; `Utils/Module.cls#L916-L930`; `Lifecycle/Base.cls#L1598-L1600`. DEC-REPO-2 |
| F-REPO-8 | Low | `config` with an unknown key or a bad value prints a message and returns OK, so scripts cannot detect typos. | Proved (`repo_witness_config_errors_report_ok`); IRIS (`nc-repo-8-config-errors`) | `Main.cls#L2063-L2093`. DEC-REPO-4 |
| F-REPO-9 | Low | `repo -type <not a moniker>` silently drops the value and prompts "Which sort of repository...". In a script the next line is read as the answer. The intended "Invalid type" error branch is unreachable. | IRIS (`nc-repo-10-invalid-type`) | `Main.cls#L2099-L2112`, `#L2274-L2277` |
| F-REPO-10 | **High** (regression from `3174d72d`) | The publish flag on a registry is ignored if an older enabled registry of the same kind exists: each kind's query has no `ORDER BY` and only its first row is read, so `publish` goes to the oldest one. | Proved (`repo_witness_publish_flag_ignored`); IRIS (`nc-repo-model-publish-row-order`) | `Http/Definition.cls#L146-L162`; `Remote/Definition.cls#L117-L129`; `Oras/Definition.cls#L143-L155` |
| F-REPO-11 † | Medium | Changing an ORAS registry's URL keeps its stored credentials, which are then sent to the new host; a remote registry clears them. | IRIS (`nc-repo-3-url-change-credentials`) | `Oras/Definition.cls#L87-L141` |
| F-REPO-12 † | Medium | A deployed module version in an ORAS registry cannot be unpublished by version: only its platform tag `V__<platform>` exists, `unpublish R/M V` says "not found", and only `all` removes it. | IRIS (`nc-repo-9-unpublish-deployed-version`) | `Oras/PublishService.cls#L72-L81` |

Refuted: **F-REPO-6** (an invalid `-type` calls `Configure` on an unrelated class). On IRIS the value is matched
against monikers and never reaches that branch (`nc-repo-10-invalid-type`); the real behaviour is F-REPO-9.

Documentation: **F-REPO-3.** The publish fallback (flag, then token, then user/password, then any enabled registry) is
intended (DEC-REPO-1: PR #332, approved by a maintainer); the help text "the repository for which deployment is
enabled. Currently, there may only be one of these per namespace" (`Main.cls#L56-L57`) is stale, and the code allows
one flag holder per registry kind.

Accepted for now, not a defect: registry passwords and tokens are stored in plain text in `^IPM.Repo.DefinitionD`
(DEC-REPO-6). The planned store is the IRIS security wallet (open issue #628); IRIS 2026.1 ships the wallet API, but
IPM still supports IRIS versions without it.

## 07 Namespace ownership

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-NSP-1 | Medium | `enable` from a registry ignores the install status and always prints "IPM enabled for namespace X" and returns OK, even when the installer failed to compile. | IRIS (`nc-nsp-N5-enable-remote-broken`) | `Main.cls#L3885-L3886` (the local-file path at `#L3904-L3917` does it right) |
| F-NSP-2 | Medium | In a namespace that gets IPM by mapping, an `IPMVersion` requirement is checked against an empty version, so every module with such a requirement is refused ("Current IPM version is .0.0"). | Proved (`nsp_witness_version_empty_when_mapped`); IRIS (`nc-nsp-N9-ipmversion-mapped-snap`) | `Storage/SystemRequirements.cls#L91-L95` (current-namespace query; `GetIPMVersionInfo` at `Main.cls#L984-L1013` already handles mapping) |
| F-NSP-4 | Medium | `enable -map -repos` (and `enable -community`) on a namespace with no IPM maps the code but skips the registries; a second run is needed. The repo step reads the active mapping, but new mappings are activated only at the end. | Proved (`nsp_witness_repos_second_run`); IRIS (`nc-nsp-N3-*`, `nc-nsp-N4-community-step-fails`) | `Main.cls#L3661`, `#L1636`, `#L3698` |
| F-NSP-5 | Medium | Running the IPM installer in a namespace that gets IPM by mapping keeps the mappings, so it recompiles the source namespace's IPM classes, and gives the namespace its own `zpm` record. Afterwards `unmap` says "No mapping found" and cannot remove the mappings. | Proved (`nsp_witness_installer_in_mapped`); IRIS (`nc-nsp-N7-*`, `nc-nsp-installer-repoint`) | `Installer.cls#L25-L29` (`%Installer` mappings are add-only). Intent (DEC-NSP-1/2, PR #485, README#L69): unmap first, then install; repo settings are mapped only with the code. After `unmap`, the `install zpm` guard still refuses (`r4-nsp-unmap-then-install-guard`) |
| F-NSP-6 | Medium | `unmap` leaves the `%IPM.*` routine mapping: the namespace still runs the source's IPM and is still reported as mapped. Argument-slot bug: the third argument of `RemoveRoutineMapping` is the routine type, not skip-activation. | Proved (`nsp_witness_unmap_leaves_routines`); IRIS (`nc-nsp-N6-*`, `nc-nsp-model-env-rtn-slot`) | `Main.cls#L3998`; `Utils/Module.cls#L1707-L1720` |
| F-NSP-7 † | Medium | When two namespaces with a `zpm` record share one IPM database (e.g. IPM's own `USER-VERIFY` test namespace), the last by name is taken as the source of every namespace mapped to it: `enable -map -repos` from the real source maps no registries, and `version` names the wrong source. | Proved (`nsp_witness_repos_tie_other_source`); IRIS (`nc-nsp-model-mapinfo`, `-repos-premapped`) | `Main.cls#L1628-L1654`, `#L3661`. † Also loses the self-uninstall "Will affect the following namespaces" warning (U21) |
| F-NSP-8 | Low | Argument-slot bug: `MapGlobalEquivalently` passes skip-activation into `AddGlobalMapping`'s set-collation slot, so each repo global mapping activates the configuration on its own (three extra activations). End state is the same; the collation side effect was not checked. | IRIS (`nc-nsp-model-env-repo-activation`); transcription check (0 end-state mismatches) | `Utils/Build.cls#L628`; `Utils/Module.cls#L1736-L1742` |
| F-NSP-9 | Medium | On the upgrade path from ZPM 0.7, `${ipmdir}` in the FileCopy targets is not expanded: a literal `mgr/${ipmdir}bin/` folder is created, the `ipm`/`iriscli` scripts are not installed, and the installer says they were. | IRIS (`nc-mig-ipmdir-placeholder-unexpanded`, `nc-mig-ipm-cli-scripts`) | `validation/native-gaps-n1.md` G3 |
| F-NSP-10 | Low | `enable -map` and `enable -community` never migrate a target namespace's legacy ZPM module records (a maintainer requirement in issue #470). Rare: the upgrade path migrates the namespaces it maps. | IRIS, simulated legacy records (`r4-nsp-legacy-setup`, `r4-nsp-legacy-enable-map`) | `Main.cls#L3553-L3708` |
| F-NSP-11 † | Low | `enable -community` prints each failed step in red, runs the next, and returns OK. | IRIS (`nc-nsp-N4-community-step-fails`) | `Main.cls#L3564-L3575` |
| F-NSP-12 † | Low | In developer mode, a load refused by a system requirement (e.g. `SYSNamespace`) stays installed: the requirement check runs after the dev-mode manifest commit. | IRIS (`nc-nsp-N8-sysns-dev`) | `Utils/Module.cls#L1311` vs `#L1321` |

Documentation defect: **F-NSP-3** (Low). `version` prints build labels such as `+snapshot` although its help says it
leaves them out (`Main.cls#L685`; `information` strips them). IRIS: `nc-nsp-N10-version-build`.

## 08 Resources and packaging

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-RES-1 | Medium | If adding a deployed item to the package fails, the error is overwritten with OK and the item's **source** is shipped instead of object code, with no `Deployed.xml`. Old (from the original import). | Patched probe p8b (`nc-gap-P8-deployed-export-forced-error`, control `-control`) | `ResourceProcessor/Default/Document.cls#L256-L257` |
| F-RES-2 | Low | The documented `register` phase and its help example do not exist: `<module> register` fails with `<METHOD DOES NOT EXIST>`; `<module> clean register` returns OK and registers nothing. | IRIS (`nc-res-N13-register`) | `DataType/PhaseName.cls#L13`; `Lifecycle/Base.cls#L64-L76`, `#L142`; help `Main.cls#L53`, `#L64` |
| F-RES-3 | Medium | `makedeployed` on a developer-mode module says it "will only report what WOULD be deployed", then deploys the classes (source no longer readable) and marks the module deployed. | IRIS (`nc-res-N12a-makedeployed-dev`) | `Lifecycle/Base.cls#L1836-L1837` vs `#L1878-L1897` |
| F-RES-4 | Low | Without `-v`, `package` never says where it wrote the package file, under a comment `// Always show this message`. | IRIS (`nc-res-N12b-package-dev`, `-N12c-package-verbose`) | `Lifecycle/Module.cls#L183-L184` |
| F-RES-7 | Medium | IPM's tar writer cuts file names of 100 characters or more to 99 and has no long-name header, so such files are lost from packages. The ARM64 `markupsafe` wheel name is exactly 100 characters, so `ProcessPythonWheel#TestPackageWithPythonDeps` fails on ARM64 (IPM's CI is amd64). | IRIS (`nc-suite-tar-name-100-truncated`); test run (`Test.PM.Integration.ProcessPythonWheel`) | `Utils/FileBinaryTar.cls#L7`, `#L469` |
| F-RES-8 | Low | Nested `-quiet` runs fail with `<NOTOPEN>`: the inner output suppressor closes the null device the outer one still uses. | Test run (`TestOutputFormat#TestQuietOutput` under `-quiet`; passes with `-v`) | `Utils/OutputSuppressor.cls#L25-L28` |

Documentation defects:
- **F-RES-5.** `run-from-file` help shows `MyCommands.text` and says the file holds "a single array"; the code accepts
  only `.txt`/`.json` with a root object holding a `commands` array (`Main.cls#L352`, `#L365` vs `#L1986-L2017`).
  IRIS: `nc-res-N17-run-from-file-v2`. The object format is intended (DEC-RES-2: help example, wiki, code since the first import).
- **F-RES-6.** The `exec` help example "set the namespace to MYAPP" has no lasting effect since PR #996 isolated
  `$namespace` (`Main.cls#L515-L516` vs `#L2433-L2438`). IRIS: `nc-res-N16-exec-namespace-v2`. Isolation is intended (DEC-RES-1, PR #996).

## 09 Developer sync

| ID | Sev. | Defect | Evidence | Code |
|---|---|---|---|---|
| F-SYNC-1 † | Medium | A failed `compile`, `load -dev` or `reload` that puts edited source on the server records no baseline and no retry mark. If the file is then reverted to its exact earlier bytes, `sync` says "Nothing to sync" while the server keeps the broken source. | Proved (`sync_witness_failed_compile_then_revert`, `_reload_then_revert`, `_skip_sync_load_then_revert`); IRIS (`nc-sync-N18-compile-fail-revert`, `nc-nsp-model-sync-reload-revert`, `-skipsync-revert`) | `Lifecycle/Base.cls#L1235-L1244`; `Pipeline.cls#L149`, `#L210-L223`. † Intent extends the #1224 fix to these loaders (DEC-SYNC-1) |
| F-SYNC-3 † | Low | `reload` of a module not in developer mode (no root) loads nothing and reports SUCCESS. | IRIS (`nc-sync-N21-reload-non-dev`) | `Lifecycle/Base.cls#L633-L634` |

Documentation: **F-SYNC-2.** After `load -dev -skip-sync`, the first `sync` only records a baseline, so edits made
before it are absorbed. That is the intended design (DEC-SYNC-2: PR #1205 self-heal, the PR #1291 test comment); the
`-skip-sync` help line should say so. `sync` with no module name returning OK after reporting each failure is also
taken as intended (DEC-SYNC-3, assumption).

## IPM test-suite issues found while running it

Not product defects, but they hide or fake results:

| Test | Issue | Evidence |
|---|---|---|
| `Test.PM.Integration.Module` | Fails when run alone: its manifest needs an interoperability-enabled namespace and never enables it; it passes in the full run only because `Health` runs first and enables it. | `validation/logs/95-suite-int-Module*.log`; `native-gaps-n1.md` |
| `InstallApplication#TestWSGIApp` | Reads the IRIS version as the 9th word of `$zversion`, which is "Containers)" on container builds, so the test skips itself. **Confirmed vacuous in IPM's CI**: skipped in all 16 runs at this commit (8 images). | `InstallApplication.cls#L36`; `validation/ci/ci-571e04a1-extract.txt` |
| `Update#TestUpdateToSameVersionTwiceNoMirror` | Returns early on a mirror member without a skip mark. | F-INST-2 |
| `DependencyResolution#TestEverything`, `LockFile#Test05`, `LockFile#Test08` | Assert nothing, swallow errors, or are empty; `TestEverything` is also skipped in CI ("Registry Server not defined"). | F-DEP-6; CI extract |
| `InstallModule#TestUninstallDependencyDoesError` | Skipped in CI: "The functionality for preventing an uninstall of a dependency module does not yet exist." It encodes the intended behaviour that F-UNIN-6 breaks. | CI extract; issue #58 |
| `Update` failure and interruption tests | Assert only statuses. | F-INST-3 |
| `PublishModuleWithDeps` (L25-L26) | Asserts that a blocked uninstall succeeds; written before blocking existed, so it records old behaviour and locks in F-UNIN-6. | F-UNIN-6 |

## Not established

- **Mirrors** (F-INST-2): Community Edition refuses mirroring, so primary-only steps were not run.
- **Remote (HTTP) registries for F-INST-5**: ORAS was run (same defect); remote takes the same code path by reading.
- **F-NSP-10 with a real ZPM 0.7.4 install**: simulated in a scratch namespace to avoid replacing the server-wide
  `zpm` command; legacy registry rows and document streams were not simulated.
- **F-LOAD-4's other paths** (`.modules` cleanup, analytics): by reading only.
- **The cause of the graceful-stop hang** in F-HIST-7: not identified.
