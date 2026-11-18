# Iteration 74 handoff: History records of install, update and uninstall

Builder session, Opus 5.5 (high). Product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, unchanged; shipped tests unchanged. Branch `consolidated_uplift`. Verus `0.2026.09.27.3cf1832` (September pin) for every run.

## 1. Scope and result

The assignment covered which commands write History records, when the record is created, and what install, update and uninstall write into it. A new model, `actions.rs`, adds the Main.Install / Update / Reinstall / Uninstall argument checks, the LoadNewModule commit marks under the install and update callers, the logged phase lists and the dispatch of phase commands. It shares two existing models instead of duplicating them:
- `consolidated/verus/history.rs` supplies the row and `finalize` (Committed || ISOK).
- `playbook-uplift/models/verus/uninstall.rs` supplies Clean, its dependency recursion and the rows it appends.

```
command ──parser──► dispatch (Main.cls:1087-1141)
  │ empty required name: no record (install/update/reinstall)
  ├─ install ──► InstallInit ─► name? repos? search ─► SetVersion(found) ─► LoadModuleReference refusals
  │                 │                                                 └─► LoadNewModule ─► Finalize ─► %OnClose
  ├─ update ──► PermitDowngrade? installed? ─(no record)                     │ guards (1213-1222)
  │                 └─► UpdateInit ─► Install(with update log) ───────────────┤ NoTx mark (1227)
  ├─ reinstall ─► installed? ─(no record) ─► Install                         │ names (1293-1294)
  ├─ uninstall ─► name? ─(no record)                                          │ dev commit+mark (1311-1313)
  │                 └─► Storage.Uninstall: UninstallInit ─► NameOpen ─► Clean │ phases (1346): Activate | ApplyUpdateSteps chain
  │                        │                     (dependency records,         │ final commit+mark (1348-1350)
  │                        │                      Base.cls:440, under -r)     │ rollback rest (1358-1361)
  │                        └─► Finalize(tSC): Committed = ISOK (no mark)
  ├─ <module> clean ─► ExecutePhases, no log ─► %Clean recursion (Recurse=1): one uninstall record per dependency
  ├─ <module> test/verify ─► LoadDependencies ─► one load record per scoped dependency installed
  └─ list, sync, <module> reload/compile/...: no record
```

## 2. Predictions vs observations

The predictions were written before reading the deciding code ([predictions.md](predictions.md)). Its exposure section lists what the census gap texts and the existing models had already revealed, so the predictions marked (E) are not blind. The results table is appended to the same file.

- **Held:**
  - Install success and not found (P1, P3).
  - Update success, and no record for an update of a module that is not installed or with `-DPermitDowngrade=1` (P5, P6).
  - Uninstall: not found, success, failure after a deletion, no name (P7-P10).
  - `list`, `reload` and `compile` write no record (P12, P13).
  - A Compile failure lists four phases (P14).
  - The record is created at the start (P15).
  - Reinstall of a module that is not installed writes no record (P16).
- **Missed:**
  - A repeated install reinstalls; it is not skipped (P2).
  - `install` with no name is refused by the command parser, so there is no record (P3b). Only a direct `##class(%IPM.Main).Install` call records it.
  - An update that fails in ApplyUpdateSteps is rolled back and reads Committed 0 (P4). The census gap's contrast assumed Committed 1. Committed 1 holds only with `-DNoTransaction=1`.
  - `<module> clean` writes an uninstall record for each dependency it cleans and removes that dependency (P11). This settles iteration70 finding 17.
- **Not predicted:**
  - `<module> test` writes a `load` record for a test-scoped dependency it installs (t1).
  - While an action runs, the permanent row has no phases. Only the IRISTEMP copy lists them. This matches the already refuted history.record#6.
  - Under `uninstall -r`, the top record is created before its dependency records (n5).
  - A downgrade refusal records the requested version and reads Committed 0 even with `-DNoTransaction=1`, because the guard runs before the no-transaction mark (i6, i7).

## 3. Formal artifacts

[actions.rs](../../../Verus_verification/coverage-20261117/iter74/actions.rs): 99 verified, 0 errors. The count includes the shared `history.rs` and `uninstall.rs` modules, which are verified with it. There is no `assume`, `admit` or `external_body`. Concrete facts use `by (compute_only)`, so the Verus interpreter is in the trusted base for them.

Domain:
- Install and update: arbitrary inputs. These are the name, the search result (repository count, found version, LoadModuleReference refusal), the manifest name and version `(text, rank)`, the installed version, the update package, `-force`, `-dev` and snapshot, `-DNoTransaction`/`-DNoJournal`, a module.xml load failure, a failure in requirements or dependencies, a failing phase index and kind (thrown or returned), and a post-commit failure.
- Uninstall: every `uninstall.rs` world whose target is not IPM itself.
- Out of the domain: `update -path`, `uninstall -all`, self-uninstall, read-only repositories (which force developer mode off) and abnormal process ends. Abnormal ends stay with the load crate.

General theorems (all inputs in the domain):

| Theorem | Statement | Premises |
|---|---|---|
| `install_always_recorded` | Main.Install writes one finalized install record for every input. The CLI writes one exactly when the name is non-empty; `api_install` always does | none |
| `update_recorded_iff` | Update writes a record exactly when the name is non-empty, `-PermitDowngrade` is absent and the module is installed; it is an update record | none |
| `reinstall_not_installed` | Reinstall of a name that is not installed writes nothing | name not installed |
| `install_committed_is_kept` | Committed == kept ∨ early_marker for install and update; ok ⇒ kept; Success from the result | none |
| `install_refusal_uncommitted` | Any refusal before LoadNewModule's mark leaves Committed 0 and no phases | a refusal |
| `install_phases` | The logged phases are the run list cut at the failing phase (i+1 entries, last failed) | reaches the phases |
| `run_shape` | For every phase list and failure index: prefix of the list, last entry failed and ended iff thrown, all earlier ended OK | none |
| `logged_lists` | Activate chain = 5 logged phases, ApplyUpdateSteps = 6, Clean = 1, Reload/Compile contain neither Test nor Verify | none |
| `uninstall_committed_is_success` | For every world and name: at least one record, the first is the uninstall record of the typed name, Committed == Success == Clean OK, no phase iff not found, else [Clean] | none |
| `uninstall_one_record` | Without -r, exactly one record | target is not IPM |
| `uninstall_no_name` | No name: no record | none |
| `dep_rec_committed` | A dependency uninstall record reads Committed exactly when its Clean returned OK, with [Clean] | none |
| `clean_without_deps_writes_nothing` | `<module> clean` of a module without dependencies writes no record | fuel > 0 |
| `other_commands_no_record` | list, sync, reload, compile, activate write none; a test command writes exactly the scoped dependency records | none |
| `dep_load_label` | A phase-scoped dependency record is a load record | none |

Neighbour theorems:
- Compute mode: `install_neighbours` (i1-i7), `update_neighbours` (u1-u5), `dev_failure_committed`, `t1_facts`.
- Proved over `uninstall.rs` worlds: `n1_facts`, `n2_facts`, `n5_facts`, `c1_facts`, `uninstall_fields`.

Model limits:
- `kept` is an annotation of transaction semantics: rollback to the initial level, the commits at :1311 and :1348, nothing rolled back without a transaction. It is not a database model.
- The phase-scoped dependency list for `test`/`verify` is an input.
- `uninstall.rs` has no resource-processor failure, so native n4 is replayed from its observed Clean status.
- Records of nested dependencies are ordered exactly only for one nesting level. `uninstall.rs` appends a nested record after its own dependencies; the product creates it before them.
- Time stamps, UserName, CommandString and Source fields are not modelled.

## 4. Controls

The harness ran 24 controls on [actions.rs](../../../Verus_verification/coverage-20261117/iter74/actions.rs), each rejected by the model's own theorems ([proofs/models.json](proofs/models.json)). There were 21 one-line mutants and 3 dependency controls on the shared models:

| Control | Altered rule | Rejected by (first failure) |
|---|---|---|
| install-init-after-name-check | Main.Install creates its record after the name check (mutant) | `install_always_recorded` (install-init-after-name-check.rs:423) |
| update-init-before-permit-check | UpdateInit before the -PermitDowngrade refusal (mutant) | `update_neighbours` (update-init-before-permit-check.rs:895) |
| update-init-before-installed-check | UpdateInit before the installed check (mutant) | `update_neighbours` (update-init-before-installed-check.rs:895) |
| reinstall-enters-install | reinstall of a name that is not installed reaches Install (mutant) | `reinstall_not_installed` (reinstall-enters-install.rs:437) |
| uninstall-sets-committed | uninstall marks its record committed at the start (mutant) | `uninstall_committed_is_success` (uninstall-sets-committed.rs:495) |
| uninstall-top-created-last | top uninstall record created after the dependency records (mutant) | `uninstall_committed_is_success` (uninstall-top-created-last.rs:508) |
| uninstall-init-after-lookup | uninstall record created only when the module is found (mutant) | `uninstall_committed_is_success` (uninstall-init-after-lookup.rs:494) |
| uninstall-no-name-recorded | uninstall with no name is recorded (mutant) | `uninstall_no_name` (uninstall-no-name-recorded.rs:512) |
| update-without-apply-update-steps | update runs the Activate chain only (mutant) | `update_neighbours` (update-without-apply-update-steps.rs:890) |
| chain-drops-apply-update-steps | GetCompletePhasesForOne(applyupdatesteps) without ApplyUpdateSteps (mutant) | `logged_lists` (chain-drops-apply-update-steps.rs:369) |
| star-logged | "*" logged as a phase (mutant) | `logged_lists` (star-logged.rs:368) |
| failure-does-not-stop | a failed phase does not stop the loop (mutant) | `install_neighbours` (failure-does-not-stop.rs:872) |
| thrown-failure-left-open | a thrown failure leaves the phase open (mutant) | `install_neighbours` (thrown-failure-left-open.rs:872) |
| no-commit-mark-without-transaction | no SetCommitted without a transaction (:1227) (mutant) | `update_neighbours` (no-commit-mark-without-transaction.rs:894) |
| no-commit-mark-dev | no SetCommitted at the developer-mode commit (:1313) (mutant) | `dev_failure_committed` (no-commit-mark-dev.rs:906) |
| guard-after-commit-mark | downgrade/update-required guard after the no-transaction mark (mutant) | `install_neighbours` (guard-after-commit-mark.rs:875) |
| install-version-not-from-search | install keeps the placeholder version until LoadNewModule (mutant) | `install_neighbours` (install-version-not-from-search.rs:875) |
| clean-writes-own-record | `<module> clean` writes a record for the module itself (mutant) | `clean_without_deps_writes_nothing` (clean-writes-own-record.rs:579) |
| phase-command-records | reload/compile/other phase commands write a record (mutant) | `other_commands_no_record` (phase-command-records.rs:592) |
| test-scoped-deps-unrecorded | `test` installs scoped dependencies without records (mutant) | `other_commands_no_record` (test-scoped-deps-unrecorded.rs:599) |
| scoped-dep-recorded-as-install | scoped dependency record labelled install (mutant) | `t1_facts` (scoped-dep-recorded-as-install.rs:944) |
| finalize-without-isok | history.rs finalize without the ISOK term (dependency) | `uninstall_committed_is_success` (actions.rs:495) |
| finalize-status-only | history.rs finalize from the final status only (dependency) | `update_neighbours` (actions.rs:894, u3 must read committed); also history.rs's own `finalize_preserves_committed_on_late_error` |
| clean-recursion-no-dependency-record | uninstall.rs dep_step appends no record (dependency) | `c1_facts` (actions.rs:838); also two uninstall.rs witnesses |

Supplementary mutants were run with [mutate.py](mutate.py), a copy of iter72's, on the final model `e08b08d4…` ([mutations/S.jsonl](mutations/S.jsonl)). 11 of 12 were killed:
- S-m01, S-m02: the parser accepts an empty install or update name.
- S-m03: a dependency record is marked committed before its Clean.
- S-m04, S-m05: list or sync writes a record.
- S-m06: a not-found uninstall lists Clean.
- S-m07: the uninstall version is set without NameOpen.
- S-m09: the dev commit is not counted as kept.
- S-m10, S-m11, S-m12: an update record labelled install, an uninstall record labelled load, a scoped dependency record labelled install.

**S-m08 survived.** It lets an update keep `-dev` for a non-snapshot. No case distinguishes a developer-mode update, whose mode LoadModuleReference overrides (F-INST-5); see §9.

## 5. Native correspondence

Harness: `.g5/Verus_verification/coverage-20261117/common/harness.py`, unchanged. Container `ipmcov20261117-iter74-<stamp>`, `--network none`, image `ipmcont20261116-iris:571e04a1-fresh` (`sha256:7da4480b…`). The source pin was verified (`source-pin.json`, no mismatches).

Command (repository root):

```sh
VERUS=~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus IPM_CALL_TIMEOUT=1500 \
  python3 .g5/Verus_verification/coverage-20261117/iter74/run.py
```

Result [run.json](run.json): ok, 99 verified, 24 controls, 363 native fields, 358 replayed, 1520 s wall. Native stamp `20261118T044846583014Z` ([observations.json](native/20261118T044846583014Z/observations.json)).

Fixtures ([native/](../../../Verus_verification/coverage-20261117/iter74/native/)):
- A filesystem repository holding h74a, h74b, h74c, h74u 1.0.0/1.1.0 and h74f 1.0.0/1.1.0.
- Local modules h74d, h74m, h74x, h74s, h74z, h74y and h74t.
- Classes outside every module: `IPMCov.Reader74` (the History reader, `OBS|key|value`, plus the mid-action probe), `IPMCov.FailProc74` (a resource processor whose Clean fails) and `IPMCov.Inst74` (an installer whose undo hook fails).

| Case | Command | Observed |
|---|---|---|
| i1, i2 | `install h74a` twice | install row each, Success, Committed, 5 phases OK |
| i3 | `install h74nosuch` | row, Package typed, `0.0.0-0`, error, uncommitted, no phases |
| i4 / i4api | `install` / `##class(%IPM.Main).Install` with no name | parser error, no row / one row, empty Package, error |
| i5 | `install h74c` (missing superclass) | 4 phases, Compile ended with an error, uncommitted, not installed |
| i6, i7 | `install h74u 1.0.0` over 1.1.0, then again with `-DNoTransaction=1` | downgrade refusal: row with Version 1.0.0, uncommitted in both, 1.1.0 stays |
| r1 | `reinstall h74nosuch` | no row |
| u1 | `update h74u 1.1.0` | update row, 6 phases ending in ApplyUpdateSteps, committed, 1.1.0 |
| u2, u3 | `update h74f 1.1.0` (step throws), then with `-DNoTransaction=1` | error, ApplyUpdateSteps ended with an error; uncommitted and 1.0.0 stays / committed and 1.1.0 kept |
| u4, u5 | `update h74nosuch`, `update h74u -DPermitDowngrade=1` | no row |
| mid.* | probes inside Activate, ApplyUpdateSteps and Clean | permanent row in progress (Success 0, no end, not finalized) with 0 phases; IRISTEMP copy 5 / 6 / 1 phases |
| n1 | `uninstall h74s` | row, [Clean OK], committed, removed |
| n2 | `uninstall h74nosuch` | row, no phases, placeholder, error, uncommitted |
| n3 | `uninstall` | no row |
| n4 | `uninstall h74x` (processor fails after the FileCopy target is deleted) | row, [Clean error], uncommitted; files gone, class and record stay |
| n5 | `uninstall h74y -r` (h74z's undo hook fails) | 2 rows, h74y first then h74z, both failed and uncommitted; H74y.A deleted, both records stay |
| c1 | `h74m clean` | 1 row: uninstall h74d, Success, committed, [Clean OK]; h74d removed, h74m stays |
| c2 | `list`, `h74a reload`, `h74a compile`, `reload h74a`, `sync` | no row |
| t1 | `h74t test` (test-scoped h74b) | 1 row: load h74b, Success, committed, 5 phases; h74b installed |

Replay: all 358 replayed fields are asserted against actions.rs expressions in one bridge (`replay/native.rs`: 100 verified, 0 errors). Model values come from `compute_only` preludes, or from the proved lemmas `uninstall_fields` and `t1_facts` for the uninstall.rs worlds. n4's record is replayed from its observed Clean status (§9 item 4). The native-only state facts (`n4.file`, `n4.installed.h74x`, `n4.class.H74x.A`, `n4pre.file`, `t1pre.installed.h74b`) are checked only as native expectations.

Flipped deciding fields: 25 flips, each rejected with a passing neighbour: `i3.rows` (1 to 0), `i4.rows` (0 to 1), `i4api.rows` (1 to 0), `u4.rows` (0 to 1), `u5.rows` (0 to 1), `r1.rows` (0 to 1), `u2.r1.committed` (0 to 1), `u3.r1.committed` (1 to 0), `u1.r1.nph` (6 to 7), `u1.r1.ph6` (6 to 7), `i5.r1.nph` (4 to 5), `i5.r1.ph4.ended` (1 to 0), `i6.r1.ver` (1 to 0), `i7.r1.committed` (0 to 1), `n1.r1.committed` (1 to 0), `n2.rows` (1 to 0), `n3.rows` (0 to 1), `n4.r1.committed` (0 to 1), `n5.r1.committed` (0 to 1), `n5.r1.pkg` (1 to 0), `c1.rows` (1 to 0), `c2.rows` (0 to 1), `t1.rows` (1 to 0), `t1.r1.action` (1 to 0), `mid.install.permph` (0 to 1).

The expectations are not blind. A scratch container (`ipmcov20261117-iter74-explore`, removed) ran the same cases before the model was written. Two earlier harness runs stopped before the native step and are kept in [attempts/](attempts/):
- One mutant's search string was not unique.
- Control `scoped-dep-recorded-as-install` survived because only the replay asserted the label. `dep_load_label` and a label clause in `t1_facts` were added.

## 6. Bindings delta

[bindings.json](bindings.json) uses the iter73 format: rows S74-01..S74-09, 65 roles with lines and decide/write/observe.
- **Newly reached member** (no earlier binding in the consolidated project map): `General/AbstractHistory.cls::UpdateInit` (write, 81-84).
- **Already-reached members gaining an active behavioural binding** to iteration74 claims (28):
  - Main.cls: `Install`, `Update`, `Reinstall`, `Uninstall`, `RunOnePhase`, `ModuleAction`, `ShellInternal`.
  - AbstractHistory: `Init`, `InstallInit`, `UninstallInit`, `SetVersion`, `SetName`, `SetCommitted`, `Finalize`.
  - HistoryTemp: `Init`, `PersistToTwin`, `%OnClose`.
  - Storage/Module: `ExecutePhases`, `Uninstall`.
  - Lifecycle/Base: `GetCompletePhases`, `GetCompletePhasesForOne`, `%Clean`, `%Test`, `%Verify`, `%Unconfigure`.
  - Utils/Module: `LoadNewModule`, `LoadModuleReference`, `LoadModuleFromDirectory`.

The distinct total is 29 members, 1 of them new. The shared models' own bindings (playbook uninstall.rs, consolidated history.rs) are unchanged.

## 7. Census delta

`census.py count` prints no ERROR or WARN lines (265 blocks: 258 enumerated, 7 structural, 0 stale), and `census.py selftest` reports 9 controls detected.
- Totals moved from 1100 assessed (full 587, partial 349, absent 164; coverage 0.5336) to 1105 (full 605, partial 341, absent 159; coverage 0.5475).
- Design checks moved from proved 627 / bounded 19 / refuted 64 / unchecked 226 / none 164 to 643 / 19 / 66 / 218 / 159.
- `features/history.g5.md` moved from 51 full, 28 partial and 27 absent to 69, 20 and 22.

`census.py inventory` was rerun after the ontology edits. Re-accounted units:
- history.actions: P1, B1, a new B2 (as-built refusals), B3 (dependencies) and B4 (other commands).
- history.committed.marking: P1 (#1, #3), a new B1 (uninstall, #4) and B2 (the working assumption, alias #2).
- history.committed.uninstall: P1 (#1 and the new #2).

Every grade marked full below is supported by a rejected control or a killed mutant that alters the deciding computation (support `mutation-checked`).

| Obligation | Before | After |
|---|---|---|
| history.actions#2 (install/update reaching the init) | partial | full (proved; text lists the post-init refusals) |
| history.actions#3 (every uninstall) | partial | full (proved) |
| history.actions#5 (list, sync, phase commands) | absent | full (proved; narrowed to "no record of their own") |
| history.actions#6 (reinstall refusal) | full (refuted) | full (refuted; re-based on actions.rs too) |
| history.actions#7 (update refusals) | absent | full (refuted) |
| history.actions#8 (uninstall with no name, as built) | new | full (proved) |
| history.actions#9 (parser refuses empty names; API install recorded) | new | full (proved) |
| history.actions#10 (`<module> clean` dependency records) | new | full (proved, instance-scoped for the count) |
| history.actions#11 (`test`/`verify` dependency load records) | new | partial (scoped list is an input; verify not run) |
| history.record#2 (created at start, filled at end) | absent | full (proved) |
| history.record#8 (working assumption: record refusals) | absent | full (refuted) |
| history.phases#3 (install 5, update +ApplyUpdateSteps, uninstall Clean) | absent | full (proved) |
| history.phases#4 (failed action lists up to the failing phase) | partial | full (proved) |
| history.committed#3 (install/update Committed = kept) | partial | full (proved, with the early-marker disjunct) |
| history.committed#4 (successful action committed) | partial | full (proved) |
| history.committed.marking#3 (install/update marks) | partial | full (proved) |
| history.committed.marking#4 (uninstall: no early mark) | partial | full (proved; text follows the ontology edit) |
| history.committed.uninstall#1 | partial | full (proved) |
| history.committed.uninstall#2 (dependency uninstall records) | new | full (proved) |
| history.fields#1 (Action label) | partial | full (proved) |
| history.fields#3, history.phases#1 | partial | partial (gaps narrowed) |

The census records are rewritten by [census_update.py](census_update.py) after `census.py inventory`.

## 8. Ontology and findings

Edits to `features/history.g5.md` (pre-edit copy in `/tmp/ipmcov74-ontology-prev`):
- **history.actions:**
  - B1 gains examples of recorded refusals.
  - A new as-built bullet lists the refusals that come before the record. These are update with `-PermitDowngrade` or not installed, reinstall not installed, uninstall with no name, and parser-refused empty names. It links the open question.
  - The "other commands" bullet now says those commands write no record of their own, and that `<module> clean` and `<module> test`/`verify` record the dependency changes they make. The earlier text ("lifecycle phase commands are not recorded") was wrong for clean and test.
- **history.committed.marking:** the statement now scopes the early marks to load, install and update. A new bullet says that for uninstall nothing marks Committed early and it comes from the final status, as PR #1322 states ("Uninstall is unchanged"). The census units are re-accounted: P1 (#1, #3), B1 (#4), B2 (the working assumption).
- **history.committed.uninstall:** extended to the dependency uninstall records of `uninstall -r` and `<module> clean`.
- **Metadata:** six blocks gain an "Iteration74 actions" line naming the declarations and native cases, and the state-machine link `models/cov20261117-iter74-history-actions/manifest.json`.

`open_questions.md`, history.record.unlogged-refusals: the reason line said "`install` with no name is already recorded". It now says this is true only for a direct class-method call, because the command parser refuses it. The question stays open.

Findings:
- **No new product defect.** Every divergence found is either an ontology statement corrected to the as-built rule (clean and test dependency records, uninstall Committed), or a policy already in the open-questions register (unrecorded argument refusals, Q-09).
- The skipped-dependency success record (P:F-UNIN-6 inside -r) and the open phase after a returned Invoke error (history.phases.returned-errors) are existing entries.
- No C:D id was used, so C:D74 stays free.

Format checks, pre-edit snapshots in `/tmp/ipmcov74-findings-prev` and `/tmp/ipmcov74-ontology-prev`, `verification_root=.g5/Verus_verification`:
- `check_ontology.py`: 0 issues, 45 writing warnings (baseline 45). An intermediate edit had 4 sentences in the marking statement; the uninstall sentence moved into a bullet.
- `check_findings.py --previous`: 38 problems, all the pre-existing "Run: line away from status line" on older entries (baseline 38); `bugs_issues.md` is unchanged.
- `link_verification.py --links-only --dry-run`: 0 links, 0 changes (the linker targets TLA+/TLC models; Verus links are written by hand, as in iterations 69 and 73).

## 9. Residual work

1. **Developer-mode update** (S-m08 survives). LoadModuleReference passes `-dev` only for a snapshot (F-INST-5), and no case distinguishes it, so history.committed#3 is unchecked for a developer-mode update. A native `update -dev` of a snapshot and of a release would close it.
2. **Phase-scoped dependencies.** `test`/`verify` dependency records take the scoped list as input (history.actions#11 partial). Modelling BuildDependencyGraph's phase filter, plus a native `verify` case, would close it.
3. **Not modelled:** `update -path` (Main.Load with the update log), `uninstall -all`, self-uninstall's early finalize, read-only repositories forcing developer mode off, and abnormal ends of install/update/uninstall (the crate covers load only).
4. **n4 replay.** A resource-processor failure in Clean is not in uninstall.rs, so n4 is replayed from its observed Clean status. n5 covers the same history decision with a fully computed failure.
5. **Nested record order.** Exact only for one nesting level (§3 limits).
6. **Unmodelled fields.** UserName, CommandString and Source fields (history.fields#8, #11, #5) are untouched. The early-marker reachability for install from a repository (a module.xml that lists but does not load) is not established.
7. **Review.** No independent read-only review has been run on this iteration; the master schedules it.

Recommended next gap: the `update` developer-mode and `-path` variants together (items 1 and 3a). They share LoadNewModule and the update log and decide Committed and the phase list for a common workflow. The other candidate is history.fields#11/#8 (CommandString, UserName), which is cheap but less consequential.

## 10. Cost

Wall time is about 70 minutes (about 03:58Z to 05:10Z, 2026-11-18). Token use is not visible to this session.
- The exploration container ran for about 22 minutes.
- Three harness runs: 3 min (stopped on a non-unique mutant), 7.5 min (stopped on a surviving control) and 25 min (final: proofs, 24 controls, native, replay and 25 flips).
- Supplementary mutants took about 4 minutes per batch, twice.
All `ipmcov20261117-*` containers were removed. No other container or image was touched.

