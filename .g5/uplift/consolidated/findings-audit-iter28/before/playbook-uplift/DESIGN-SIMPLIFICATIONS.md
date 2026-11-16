# Design simplifications for IPM

Baseline: IPM `571e04a1`. None of these were applied to IPM; the checkout is unchanged. Each design below removes a
**group** of defects from `BUGS.md` by changing one rule, instead of patching each defect. Proposals for the IPM
maintainers to judge, not decisions. Revision 2026-11-13 (round 4): every intent these designs waited on is now
resolved (`decisions/*.md` "Resolutions"); the last column names the resolved intent each design implements, with
"assumption" where the intent was assumed rather than sourced. New defects of round 4 are added to the designs they
fit, and D14–D16 are new.

How strong is the case for each design:

| Mark | Meaning |
|---|---|
| **Proved design** | The changed design is modelled in Verus and proved to meet the intended rule (L1), with its remaining premises named. Only D1 has this. |
| **Premise-scoped** | The current code is proved to meet L1 under a named premise, and that premise excludes exactly the defect case. So the fix only has to make the premise case behave; the rest of the behaviour is already proved. The changed design itself is not modelled. |
| **Observed cause** | The cause was confirmed on IRIS (for example by repairing the state by hand), not modelled. |
| **Inspection** | Read in the code only. |

## Overview

| # | Design | Removes | Strength | Resolved intent it implements |
|---|---|---|---|---|
| D1 | Durable history writes only at `$TLEVEL` 0; reconcile at the next command | F-HIST-1, -2, -8; shrinks F-HIST-7 | Proved design | DEC-HIST-1 A, DEC-HIST-2 A (evidence: PR #578, wiki 09); DEC-HIST-4 B (assumption: caller transactions allowed and documented) |
| D2 | Never drop a `%Status`; never report a no-op as SUCCESS | F-INST-1, F-NSP-1, F-NSP-11, F-RES-1, F-REPO-8, F-LOAD-1, F-HIST-9, F-SYNC-3; parts of F-HIST-6 | Premise-scoped (F-INST-1) / IRIS | DEC-INST-6 fail (assumption), DEC-REPO-4 error (evidence), DEC-NSP-4 fail (assumption), DEC-SYNC-4 warn (assumption); `sync` with no name keeps returning OK (DEC-SYNC-3, assumption) |
| D3 | Check before acting in Clean; a blocked uninstall fails; fail closed | F-UNIN-1, -4, -6 | Premise-scoped | DEC-UNIN-1 a (evidence: issue #58, `TestUninstallDependencyDoesError`) |
| D4 | Identify IPM by resolved module, not by typed name; normalise names once | F-UNIN-3, -8, -11, F-DEP-3 | Premise-scoped | DEC-UNIN-2: `zpm clean` is an ordinary level-0 Clean (evidence) |
| D5 | One ordered registry selection used by every command, `Enabled` applied once | F-INST-4, F-REPO-7, -10 | Premise-scoped | DEC-REPO-1: keep the fallback order (evidence: PR #332); DEC-REPO-2: never use a disabled registry (evidence) |
| D6 | Select update step classes by exact name `V<digits>` | F-INST-6, -7 | Proved witnesses; one-line change | — |
| D7 | Resolve developer mode once; seed update steps whenever a version arrives without running them | F-INST-5, -8, -9, -10 | Premise-scoped | DEC-INST-1 (evidence), DEC-INST-2, -3, -8 (assumption), DEC-LOAD-2 (evidence) |
| D8 | Stage file-system effects and apply them after the database outcome | F-LOAD-2, -3, F-DEP-8; F-LOAD-1 with D2 | Observed cause | DEC-HIST-5 A (evidence), DEC-DEP-2 A (evidence: PR #963) |
| D9 | One mapping helper with one signature; decide from the planned state | F-NSP-4, -6, -7, -8 | Premise-scoped | DEC-NSP-6 a (assumption) |
| D10 | Record the sync baseline wherever source reaches the server | F-SYNC-1 | Premise-scoped | DEC-SYNC-1 a (assumption); DEC-SYNC-2: the first sync after `-skip-sync` only records (evidence), so no change there |
| D11 | Remove or implement half-built options | F-UNIN-2, F-REPO-1, -2, -9, F-DEP-1, F-RES-2 | Inspection / IRIS | DEC-INST-7: fix the mirror comment (evidence) |
| D12 | Reuse existing helpers instead of near-duplicates | F-NSP-2, F-UNIN-9, F-UNIN-10, F-HIST-6, F-NSP-9, F-REPO-11 | IRIS / inspection | DEC-REPO-3 a (assumption) |
| D13 | Small local fixes with no shared pattern | F-REPO-4, -5, -12, F-RES-3, -4, -7, -8, F-UNIN-5, -7, F-DEP-2, -9, F-HIST-5 | IRIS / inspection | DEC-HIST-3 (evidence), DEC-REPO-5 (assumption), DEC-DEP-3 (assumption) |
| D14 | Nothing that can fail runs after the final commit | F-LOAD-4 | IRIS | — |
| D15 | Check every requirement before any commit, in every mode | F-NSP-12 | IRIS | DEC-NSP-5 (assumption) |
| D16 | Use the existing backtracking fix for resolution | F-DEP-7 | Proved witness; upstream PR #1303 | DEC-DEP-1 (evidence: issue #1299) |
| D17 | Run the legacy-record migration wherever IPM is enabled | F-NSP-10 | IRIS (simulated legacy) | DEC-NSP-3 (evidence: issue #470) |

No design is needed for F-HIST-3, F-HIST-4, F-HIST-10, F-SYNC-2 or F-REPO-3: their behaviour is intended, and only
the documentation changes. F-HIST-7 (the commit window: IRISTEMP and the namespace database cannot be written
atomically, so some order of the two writes always has a window) is not removable by any in-process design; D1
shrinks it for developer-mode and no-transaction loads only. F-NSP-5 is handled by D9's resolved intent (DEC-NSP-1/2:
unmap first, then install; the installer and `load` refuse while mapped), plus the D9 fix that lets `unmap` finish.

---

## D1. Durable history writes only at `$TLEVEL` 0; reconcile at the next command

**Problem (RC-1, RC-2, RC-4).** The durable history row lives in the namespace database and is written at whatever
transaction level is open (Init, `%OnClose`, `PersistToTwin`), so a rollback takes it with it. `Committed` lives only
in IRISTEMP until `%OnClose`, and an instance crash runs no `%OnClose` and wipes IRISTEMP.

**Design (the `fix` configuration of `models/verus/load_general.rs`).**
1. When `SetCommitted` runs at `$TLEVEL` 0, also save the durable twin at once (`Utils/Module.cls#L1311-L1313`, the
   developer-mode manifest commit, before the next `TSTART`; and the no-transaction path). This is DEC-HIST-2 option A,
   about two lines.
2. Keep dependency rows pending in IRISTEMP. After the parent's outcome is known (after its rollback, or at the next
   command after a crash), re-persist them at `$TLEVEL` 0, with `Committed` taken from **whether the dependency is
   installed at the logged version now**, not from the logged flag. (Keeping the logged flag was the first version of
   this design; the explorer showed it dishonest after a parent rollback.)

**Removes.** F-HIST-1 (dependency rows survive the parent's rollback), F-HIST-2 (developer-mode crash in dependencies),
F-HIST-8 (restart after a developer-mode or no-transaction load). Premises PC, PD and PI are no longer needed.

**Evidence.** Proved: `g_fixed_refines_strict` (any number of dependencies, any prior state, process and instance
crashes, both crash resolutions), `g_instance_crash_honest_when_fixed`. Cross-checked against the fine model on
35,892 configurations with 0 mismatches (`fold_check_general.py`) and explored in section G of `explore.py`
(`fix-dev-persist`, `fix-dep-repersist`). Not implemented or run in IPM (TRUST.md T18).

**Limits that remain, proved.**
- `g_instance_crash_loses_pending`: an instance crash after a dependency load began loses its pending IRISTEMP row,
  for **any** in-process design that keeps pending rows in IRISTEMP. Premise PI-dep stays. Closing it needs a durable
  pending marker written at `$TLEVEL` 0 before the dependency's transaction starts; not modelled (QUESTIONS.md U5).
- P1 (no caller transaction), P2 (a crash rolls back), P3 (the commit window) stay. Under the resolved intents P1
  and P2 describe accepted behaviour (DEC-HIST-4 B, DEC-LOAD-1 A: document them), not defects.
- A dependency loaded and then undone by its parent reads Success = 1, Committed = 0: the documented reading
  (DEC-HIST-1, wiki page 09 "Committed vs. Success").
- A graceful `iris stop` during a load is another instance of RC-2 (`r4-hist-graceful-stop-txn-after`): step 1 covers
  developer-mode and no-transaction loads; for a transactional load the window between `TCOMMIT` and the twin save
  remains, as in F-HIST-7.

**Simpler than today.** One rule ("durable history is written only at `$TLEVEL` 0") replaces the current reliance on
`%OnClose` timing, which differs between method return, process halt, terminal prompt and instance crash.

## D2. Never drop a `%Status`

**Problem (RC-5).** Statuses are discarded in three ways: a status method is called with `do`; `set tSC = $$$OK`
follows the call; or a timeout returns the `$$$OK` the variable was initialised with.

| Site | Defect |
|---|---|
| `Storage/Module.cls#L105-L108` lock timeout returns `sc = $$$OK` | F-INST-1 |
| `Main.cls#L3885-L3886` `do $system.OBJ.LoadStream(...)` | F-NSP-1 |
| `ResourceProcessor/Default/Document.cls#L256-L257` status overwritten | F-RES-1 |
| `Main.cls#L2063-L2093` `do UpdateOne/PrintOne/ResetToDefault` | F-REPO-8 |
| `Main.cls#L2358`, `Utils/Module.cls#L2409` clone return code unchecked | F-LOAD-1 |
| `Storage/Module.cls#L669`, `#L671` returned error quits before `FinalizePhase` | F-HIST-9 |
| `History.cls` `DeleteHistoryGlobally` swallows per-namespace errors | part of F-HIST-6 |

**Design.** One rule: every `%Status` is either returned, combined (`$$$ADDSC`), or written to the history and turned
into a non-OK command status. Aggregate commands (`enable -community`, `sync` with no module) return an error when any
part fails (DEC-NSP-4, DEC-SYNC-3 decide whether that is wanted). Enforce with a lint over `do ##class(...).<m>(...)`
and `do ..<m>(...)` where `<m>` returns `%Status`, and over `set tSC = $$$OK` directly after `set tSC = ...`. For
F-HIST-9, call `FinalizePhase` in a `finally`-style path shared by the catch and the early quit.

**Evidence.** F-INST-1 is premise-scoped: `inst_refines` proves every update step meets L1 under PL (the lock is
granted); making the timeout an error removes the only case PL excludes. The others are inspection plus IRIS.

**Resolved intents.** A busy update lock fails the update with a clear error (DEC-INST-6, assumption from the PR #887
authors' removed `TestSimultaneousUpdateFails`); `config` errors return an error status (DEC-REPO-4); a failed
`enable -community` step fails the command (DEC-NSP-4, assumption); `reload` of a module without a root says nothing
was loaded (DEC-SYNC-4, assumption). `sync` with no module name keeps reporting each failure and returning OK
(DEC-SYNC-3, assumption), so it is not changed.

## D3. Check before acting in Clean; "skipped" is its own outcome; fail closed

**Problem (RC-6).** `ExecutePhases` calls every resource processor's `OnBeforePhase("Clean")` (web and CSP
applications deleted, files purged, `Unconfigure` hooks run) before `%Clean` asks `CheckBeforeClean` whether the
module may be removed at all. When the check says no, the command still returns OK and logs a successful uninstall.
The dependents check also ignores modules whose dependency graph failed to build.

**Design.**
1. Run the dependents and global-scope check first, before any `OnBeforePhase("Clean")`, for the top module and for
   each dependency in `-r`.
2. A blocked top-level uninstall returns an error naming the dependents and logs a failed uninstall (DEC-UNIN-1,
   resolved from issue #58 and IPM's skipped `TestUninstallDependencyDoesError`). Inside `-r`, keeping a need that
   another module still uses stays a success, but must not delete the need's web applications.
3. Fail closed: if any installed module's graph cannot be built, treat it as a possible dependent (or fail the
   command and name it), instead of dropping it.

**Removes.** F-UNIN-1, F-UNIN-6, F-UNIN-4.

**Evidence (premise-scoped).** `unin_refines_blocked_unchanged` (premises PG, PW), `unin_refines_clean_keeps_target`
(PCW, PF), `unin_refines_dependents_protect` (PG), `unin_refines_honest` and `unin_refines_rows_honest` (PB, PG, PI,
PNF; PR). PW/PCW exclude modules with web apps, PG excludes failed graphs, PB excludes blocked uninstalls: exactly the
defect cases. Explorer: `repo-explore-unin-blocked-unchanged`, 0 violations under the premises, 271,984 without.

**Note.** IPM's test `PublishModuleWithDeps.cls#L25-L26` asserts the current success; it predates the block and must
change, and `TestUninstallDependencyDoesError` can then be unskipped.

## D4. Identify IPM by resolved module, not by typed name; normalise names once

**Problem (RC-7).** The self-uninstall guard runs only when the typed name is exactly `zpm` (`Main.cls#L2712`), but
lookups are case-blind and accept the external name `IPM`. The recursive clean of `-r` never reaches that guard at
all. `list-dependents` compares the typed name with lower-cased graph keys.

**Design.**
1. Resolve the typed name to a module record once, at the command boundary (case-blind name, then external name), and
   pass the record on.
2. Put the self-uninstall guard on the resolved record ("this is IPM's own record"), inside the Clean path, so it
   covers `uninstall IPM`, `uninstall ZPM` and IPM reached as a dependency in `-r` (refuse to remove IPM there, or
   ask).
3. `list-dependents` and every other name comparison use the resolved, lower-cased name.

**Removes.** F-UNIN-3, F-UNIN-8, F-DEP-3, and F-UNIN-11: with the guard keyed on "IPM is really being removed",
`zpm clean` becomes an ordinary level-0 Clean with no orphan warning (DEC-UNIN-2).

**Evidence (premise-scoped).** `unin_refines_self_guard` under PN (the operator types exactly `zpm`) and
`unin_refines_ipm_kept` under PI (IPM is not a dependency removed by `-r`); both premises name the defect cases.
IRIS: `nc-unin-env-name-case` shows the lookups are case-blind.

## D5. One ordered registry selection used by every command

**Problem (RC-9, plus inconsistent `Enabled` checks).** Install collects candidates in a subscript keyed by registry
ID, so the creation order beats `OverriddenSortOrder`. Publish reads only the first row of a per-kind query that has no
`ORDER BY`, so the publish flag on a newer registry is ignored, and falls back to any registry with credentials when
no registry has the flag. `search` and `publish -r` ignore `Enabled`; install and unpublish honour it.

**Design.** One query (or one method) returns enabled registries in a total order: (publish flag where publishing),
sort order, name, ID. Every command (install, search, publish, unpublish) uses it, so `Enabled` and the order are
applied in one place. With no flagged registry, the fallback (token, then user and password, then any) is kept: it
is intended (DEC-REPO-1, PR #332); the help text changes instead. Disabled registries are never used, by any command
(DEC-REPO-2); `repo -list` still shows them.

**Removes.** F-INST-4, F-REPO-10 (restoring the `ORDER BY PublishOrder, ID` that commit `3174d72d` dropped), F-REPO-7.

**Evidence (premise-scoped).** `repo_refines_publish_default` under PH, PU, PK (PK excludes F-REPO-10, PH excludes
F-REPO-3), `repo_refines_publish_named` and `repo_refines_search` under PE (PE excludes F-REPO-7);
`repo_install_enabled_only` (no premise) shows install already honours `Enabled`. Note from the decisions file:
the order alone fixes F-REPO-10.

## D6. Select update step classes by exact name `V<digits>`

**Problem.** `GetUpdatePackageClassList` takes every class whose name starts with `<UpdatePackage>.V`, strips
everything but digits, and keys the list by that number. `V3Helper` overwrites `V3`; `Validator` gives an empty
subscript and `<SUBSCRIPT>`.

**Design.** Match the last name piece against `V` followed only by digits (`?1"V"1.N`); ignore other classes. Reject
(with the class name) two step classes with the same number, e.g. `V3` and `V03`.

**Removes.** F-INST-6, F-INST-7.

**Evidence.** Proved witnesses `inst_witness_helper_class_hides_steps`, `inst_witness_class_without_number_fails`; the
L1 rule "every step class is walked" holds once the list contains only `V<digits>` classes (the model's class list is
the only input that differs). IRIS probes in `65-inst-model-order.log`.

## D7. Resolve developer mode once, at the command boundary

**Problem (RC-11).** The effective developer mode is decided in several places: the typed `-dev`/`-nodev`, the mode
stored on the module, `LoadModuleReference` forcing 0 for non-snapshot registry versions, `LoadModuleFromDirectory`
overriding it for every load that is not a top-level install, and `Main:Install` passing `DeveloperMode=0`. The
update guard reads only the typed flag; seeding reads only the stored flag.

**Design.** Compute one value at the command boundary (typed flag, else stored mode, else 0) and pass it down; the
registry rule (dependencies from a registry load in normal mode) applies to dependencies only, never to the named
module.

Add one seeding rule: whenever a module version arrives without its update steps being run (any install, a dev
install, or a module that an update brings in for the first time), seed the steps of every version up to it
(DEC-INST-2, DEC-INST-8, assumptions). Extend the dev-mode guard to needed modules: refuse to replace a dependency
that is in developer mode, naming it (DEC-INST-3, assumption; wiki Concepts). Keep the `-dev` bypass of the update
guard and document it (DEC-INST-1, evidence), and keep developer mode on a plain `load` (DEC-LOAD-2, evidence).

**Removes.** F-INST-5, F-INST-8, F-INST-9, F-INST-10.

**Evidence (premise-scoped).** `update_steps.rs` proves every step meets L1 under PF5 ("the run has the typed flag,
else the stored mode"), which is exactly this design; PF5 fails today only for registry loads
(`inst-explore-refinement-without-pf5`, 76,348 violating transitions).

## D8. Stage file-system effects and apply them after the database outcome

**Problem (RC-6 for files).** File effects are never rolled back. An archive load deletes and recreates
`<ModuleRoot>/<name>/<version>/` before any check; a URL load clones into `mgr/Temp/...` and points the module Root
there; `install -create-lockfile` writes the lock file before the module's own Activate.

**Design.** Extract, clone or write into a staging folder; after the database commit, move it into place (or record
the final path); after a rollback, delete the staging folder. Check the clone's return code (D2).

**Removes.** F-LOAD-3, F-LOAD-2, F-LOAD-1 (with D2), F-DEP-8 (write the lock file only after the whole install
succeeds, as PR #963's author described; DEC-DEP-2).

**Evidence.** IRIS observations of each case; not modelled. Committed covers database changes only (DEC-HIST-5,
resolved from PR #578); with staging, file changes also follow the database outcome, so history and disk agree.

## D9. One mapping helper with one signature; decide from the planned state

**Problem (RC-8, RC-9).** `RemoveRoutineMapping`'s third parameter is the routine type, `RemovePackageMapping`'s is
skip-activation; `AddGlobalMapping`'s fourth is set-collation, its fifth skip-activation. Two call sites pass the flag
in the wrong slot. Separately, the `-repos` step decides with the *active* mapping while new mappings wait for one
batch activation at the end, and the "source" of a mapped namespace is chosen as the last namespace by name sharing
the database.

**Design.**
1. One helper, `Map(kind, ns, name, dest, skipActivation)` / `Unmap(kind, ns, name, skipActivation)`, for package,
   routine and global mappings, so the flag is always in the same slot (or named arguments via an object).
2. Decide the repo step from the planned set of mappings (the batch being built), not from the active configuration.
3. A namespace is a source only if its own routine database holds `%IPM` (DEC-NSP-6, assumption).

**Removes.** F-NSP-6, F-NSP-8 (part 1), F-NSP-4 (part 2), F-NSP-7 (part 3); with the rule below, F-NSP-5.

**Evidence (premise-scoped).** `nsp_lemma_unmap` under RTN and INFO, `nsp_lemma_enable` and `nsp_lemma_community`
under REPOS, and the UNIQ premise for F-NSP-7; each premise names the defect case. IRIS slot probe
`nc-nsp-model-env-rtn-slot` (with type ALL the mapping is removed).

F-NSP-5 (installer in a mapped namespace), resolved intent (DEC-NSP-1, DEC-NSP-2, evidence: PR #485, README#L69):
a namespace that gets IPM by mapping must be unmapped before it gets its own IPM, and repo settings are mapped only
together with the code. So the installer and `load` of IPM refuse while `%IPM` is mapped (as `install zpm` already
does), and part 1 above must make `unmap` complete, since after `unmap` the `install zpm` guard still refuses today
(`r4-nsp-unmap-then-install-guard`).

## D10. Record the sync baseline wherever source reaches the server

**Problem.** The rule "an unmarked baseline equals the server copy" is kept only by `sync` itself. `compile`,
`load -dev` and `reload` put source on the server without updating the baseline or setting a retry mark when they
fail; a module with no baseline has its first sync stamp the disk as baseline without loading.

**Design.** One function, called by every loader (sync, compile, `load -dev`, `reload`), records what reached the
server: the new hash on success, a retry mark on failure. `load -dev -skip-sync` stamps the baseline at load time
(the bytes it loaded), so the first sync compares against what is on the server.

**Removes.** F-SYNC-1 (DEC-SYNC-1, assumption). The `-skip-sync` stamping change above is **not** wanted: the first
sync only records a baseline by design (DEC-SYNC-2, evidence), so F-SYNC-2 is fixed in the help text instead.

**Evidence (premise-scoped).** `sync_lemma_sync`, `sync_lemma_compile`, `sync_lemma_reload` prove L1 under OUT (no
loader outside sync puts non-baseline bytes on the server) and BASE; the design makes both premises true by
construction. Fine-model refinement: 0 violations on premise-respecting traces.

## D11. Remove or implement half-built options

**Problem (RC-10).** Options that are documented and accepted but do nothing are worse than missing options: the
operator believes they worked.

| Option | Today | Simplest honest fix |
|---|---|---|
| `uninstall -purge` (F-UNIN-2) | nothing reads `Purge` | pass the `e` flag to `$system.OBJ.Delete` when set (IRIS `nc-unin-env-obj-delete-e-flag`), or remove the option |
| `-token-auth-method` for remote (F-REPO-1) | ignored | read it in `Remote/Definition:OnConfigure` as ORAS does, or reject it for remote |
| `-sslconfig`, `-publish-url` (F-REPO-2) | ignored | implement, or remove from help and reject |
| `repo -type <bad>` (F-REPO-9) | silently dropped, then a prompt | fail with the existing "Invalid type" message; never prompt when not interactive |
| `PermitDowngrade` (F-DEP-1) | resolver allows, loader refuses | remove the flag, or make the loader honour it |
| `register` phase (F-RES-2) | listed in help and phase lists, no `%Register` | remove from help, `GetCompletePhasesForOne` and `MatchSinglePhase`, or implement |
| Primary-only steps on role change (F-INST-2, now documentation) | comment only | fix the `VersionBase.cls#L18` comment (DEC-INST-7, resolved: mirroring is future work) |

## D12. Reuse existing helpers instead of near-duplicates

| Defect | Existing helper that already does it right |
|---|---|
| F-NSP-2 `CheckIPMVersion` reads the current namespace only | `GetIPMVersionInfo` (`Main.cls#L984-L1013`) already resolves a mapped namespace's IPM |
| F-UNIN-9 `-r` deletes a dependency's record with `NameDelete`, leaving its manifest | `%IPM.StudioDocument.Module:Delete` (used for the top module) deletes both |
| F-UNIN-10 full cleanup kills only `CacheD` | kill the class's whole extent (`%KillExtent`) or all three storage globals |
| F-HIST-6 auto-purge quotes the value | `ConstructSQLWhere` already binds it; pass the raw date |
| F-NSP-9 `${ipmdir}` not expanded on the 0.7 upgrade path | the normal install path's placeholder expansion |
| F-RES-1, F-NSP-1 | the local-file branch of `EnableIPM` (`Main.cls#L3904-L3917`) already keeps every status |
| F-REPO-11 ORAS keeps credentials on a URL change | the remote registry's `OnConfigure` (`Remote/Definition.cls#L81-L86`) already clears them; share it |

## D13. Small local fixes with no shared pattern

| Defect | Fix |
|---|---|
| F-REPO-4 `config delete` restores the first value set | seed a factory default for every key in the installer; never infer a default from the first set |
| F-REPO-5 default modifiers shared by all namespaces | store them in a non-`%` global (per namespace, as documented), or change the help to say instance-wide |
| F-RES-3 `makedeployed` in dev mode deploys | honour the message: report only, unless packaging (DEC-RES-3 decides packaging) |
| F-RES-4 package path shown only with `-v` | drop the `:tVerbose` condition, as the comment says |
| F-RES-7 tar names ≥ 100 characters cut | use the ustar `prefix` field (paths up to 255) or a GNU long-name header |
| F-RES-8 nested `-quiet` | the suppressor restores the previous device instead of closing a shared null device |
| F-UNIN-5 name-only global resources kept | derive the global from the resource name when `Global` is not set |
| F-UNIN-7 `orphans` lists IRIS items | apply the mapped/system filters to every document type, or default to `CLS` |
| F-DEP-2 lock file in name order | write dependencies as an ordered array in install order |
| F-HIST-5 records without a name | call `SetName` from the manifest read before the guards (DEC-HIST-3, evidence: wiki 09) |
| F-REPO-12 ORAS unpublish of a deployed version | delete the plain tag and every `V__<platform>` tag of that version (DEC-REPO-5, assumption) |
| F-DEP-9 lock file with a folder-loaded need | write the entry with an empty registry, as already done for the base module (DEC-DEP-3, assumption) |
| F-NSP-3, F-RES-5, F-RES-6, F-DEP-4, F-DEP-5 | documentation and comment edits only |

## D14. Nothing that can fail runs after the final commit

**Problem (RC-12).** `LoadNewModule` commits and sets Committed, then, inside the same `try`, evaluates the
after-install message (`%EvaluateMacro`), and by code reading cleans `.modules` and collects analytics. A throw there
reports the command as failed with every change kept (F-LOAD-4; seen in all three modes). The model premise PM
("nothing fails after the final commit") is false.

**Design.** Evaluate the after-install message (and anything else that can fail) **before** the final commit, or run
post-commit work in its own `try` whose failure is printed as a warning and never changes the command's status or the
history record. Also delete `%IPM.Generated.MacroEval` when its compile fails.

**Removes.** F-LOAD-4, and with it premise PM in the load/history proofs.

## D15. Check every requirement before any commit, in every mode

**Problem.** In developer mode the manifest is committed (`Utils/Module.cls#L1311`) before `CheckSystemRequirements`
(`#L1321`) runs, so a module refused by a system requirement stays installed (F-NSP-12).

**Design.** Run `CheckSystemRequirements` (and every other refusal check) before the developer-mode manifest commit.
The intent that a refused load leaves no module, in every mode, is taken from issue #106, the changelog ("prevent
installation") and the wiki ("validated before module installation") (DEC-NSP-5, assumption).

## D16. Use the backtracking resolver

**Problem.** The resolver is greedy and fails on solvable inputs (F-DEP-7); upstream knows it (issue #1299, open) and
has a fix in review (PR #1303, open). Its method comment already promises backtracking.

**Design.** Adopt real backtracking (the open PR or equivalent). The proofs here show a successful resolution is
always sound (`dep_refines`) and every failure names a reason (DEP-B54); after the change, the completeness check
`dep-explore-completeness` (2,040 of 70,656 instances fail today) is the acceptance test.

## D17. Run the legacy-record migration wherever IPM is enabled

**Problem.** `EnableIPM` (`Main.cls#L3553-L3708`) never runs `MigrateZPMToIPM`; only an IPM load does. A namespace that
reaches IPM by `enable -map`/`-community` keeps legacy ZPM module records that IPM does not list (F-NSP-10; observed
with a simulated legacy install).

**Design.** Call the same migration (`%IPM.Utils.Migration` `RunAll`) for each target namespace in `EnableIPM`, as a
maintainer required in issue #470 (DEC-NSP-3).
