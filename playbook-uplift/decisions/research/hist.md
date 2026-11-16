# Research: history, loading, semantics and scope decisions (lane "hist")

Baseline IPM `571e04a1`. Researcher: hist lane, 2026-11-13. This file proposes resolutions only. It edits no
ontology, decision, finding or BUGS file.

Sources used for every item: IPM code and comments at `571e04a1`, `git log -S`/`blame`, `CHANGELOG.md`, the IPM wiki
clone (pages `09.-Diagnostics-and-Troubleshooting.md`, `00.-Concepts.md`, `02.-CLI-commands.md`), GitHub
(intersystems/ipm issues and PRs, read only), IPM tests, and new runs on `ipm-uplift-iris`.

Key external sources (quoted only in short phrases):

- **PR #578 "History Log"** (https://github.com/intersystems/ipm/pull/578, merged 2025-12-11, commit `8a88d5bf`).
  Review thread:
  - TODO list in the PR body includes "Record recursively loaded/installed modules".
  - isc-kiyer asked whether a rolled-back install also rolls back history. isc-shuliu: history saving is
    "performed outside of the transaction", so no.
  - isc-dchui (2025-11-24): phase logging "requires temporarily storing in IRISTEMP to avoid losing data to
    transaction rollback".
  - isc-dchui on `Committed` (AbstractHistory.cls:43): Committed is about "whether changes have been made to the
    database because of an action"; a user wants "actions [that] have failed but have been committed so that I can
    debug or manually undo them".
  - isc-kiyer on History.cls:4: a trigger to protect history from tampering, "So then only inserts and deletes are
    allowed?" isc-dchui added the trigger.
  - isc-dchui on the Package placeholder (AbstractHistory.cls:70): the empty string default is "easy enough to check for".
  - isc-dchui (thread on purge): automatic purge might "defeat the point of logging all actions for an audit trail".
- **Issue #1237** "History log 'committed' column is inaccurate" and **PR #1322** (commit `5d27e15b`, 2026-11-06):
  expected behaviour "`Committed` should reflect whether the action's changes were actually retained"; option 2
  "Set `Committed` at each commit point". PR #1322 implements `SetCommitted` "at each commit, or before writing when
  there is no transaction". CHANGELOG `#1237` entry (CHANGELOG.md#L45).
- **Wiki `09.-Diagnostics-and-Troubleshooting.md`** (commit c5e53e7, 2026-09-03, by isc-dchui, from issue #1234):
  - "IPM records every `install`, `load`, `uninstall`, and `update` it attempts, whether or not the attempt succeeded."
  - "a process that dies mid-install still leaves a record behind, with an empty end time and a placeholder version."
  - "The log is durable across install failures." IPM "copies the result into `%IPM_General.History` outside the
    transaction, so failures remain visible after the rollback."
  - Package column: "May be empty if the action failed before a name was known, for example when loading a directory
    that has no valid `module.xml`."
  - Committed column: "Whether the action's work was permanently written to disk." Section "Committed vs. Success":
    "`Committed` is whether the action's changes survived. Normally the two agree, because a failed install rolls back
    its transaction".
  - `history delete -confirm` deletes every record in the namespace.
- **Wiki `00.-Concepts.md`**, Developer Mode and `-D` flags:
  - "A module loaded in dev mode cannot be overwritten by a non-dev install (you'll get `ERROR! Cannot install 'X' over
    previously installed in developer mode`). Uninstall first, or use `-force`."
  - `-DNoTransaction=1`: "Use when some modules in the install conflict with an outer wrapping transaction".

New runtime evidence (this lane). Scripts in `validation/iris/r4/hist/`, probe class `Uplift.R4.Hist`
(`validation/iris/r4/hist/Uplift/R4/Hist.cls`), Invoke target `Uplift.R4.Hooks` (`hooks/Hooks.cls`), fixtures in
`validation/iris/r4/hist/fixtures/`. Container `ipm-uplift-iris` only. Observations `OBS|r4-hist-*`:

| Script | Log | Observations |
|---|---|---|
| `01-post-commit.script` | `validation/logs/a1-hist-post-commit.log` | `r4-hist-macro-probe-*`, `r4-hist-post-commit-throw-txn` (control: message without `{`, no throw) |
| `02-post-commit-brace.script` | `validation/logs/a1-hist-post-commit-brace.log` | `r4-hist-macro-probe-brace-iserr`, `r4-hist-post-commit-throw-r4h-msg2-txn`, `-notxn`, `-dev` |
| `03-sem2-irissys.script` | `validation/logs/a1-hist-sem2-irissys.log` | superseded (diff routine was wrong: indirection on private arrays); ignore |
| `04-sem2-irissys-b.script` | `validation/logs/a1-hist-sem2-irissys-b.log` | `r4-hist-sem2-noop`, `-evaluate-macro-fails`, `-evaluate-macro`, `-load-rest-app`, `-uninstall-rest-app` |
| `05-delete-running.script` | `validation/logs/a1-hist-delete-running.log` | aborted run (probe bug, `<INVALID OREF>`); its `r4-hist-delete-in-compile-dev-*` lines are invalid (job blocked by the stuck txn job) and superseded by `07` |
| `06-cleanup.script` | `validation/logs/a1-hist-cleanup-1.log` | cleanup only |
| `07-delete-running-b.script` | `validation/logs/a1-hist-delete-running-b.log` | `r4-hist-delete-in-compile-txn-*`, `-dev-*`, `r4-hist-delete-dep-row-dev-*` (its `txn`/`notxn` lines are confounded, superseded by 08 and 09) |
| `08-delete-dep-row-txn.script` | `validation/logs/a1-hist-delete-dep-row-txn.log` | `r4-hist-delete-dep-row-txn-during`, `-after-release`, `-later` |
| `09-delete-dep-row-notxn.script` | `validation/logs/a1-hist-delete-dep-row-notxn.log` | `r4-hist-delete-dep-row-notxn-during`, `-after-release` |
| `10`/`11-u19-*.script` + host transcript | `validation/logs/a1-hist-u19-start-txn.log`, `a1-hist-u19-stop-txn-host.log`, `a1-hist-u19-after-txn.log` | `r4-hist-graceful-stop-txn-during`, `-after` |
| `12-u19-start-txn-wait20.script`, `11-u19-after.script` + host transcript | `validation/logs/a1-hist-u19-start-txn-wait20.log`, `a1-hist-u19-stop-txn-wait20-host.log`, `a1-hist-u19-after-txn-wait20.log` | `r4-hist-graceful-stop-txn-wait20s-during`, `-after` |

Cleanup: `13-final-cleanup.script` (`a1-hist-cleanup-final.log`; two terminal lines failed with `<SYNTAX>`, no
effect) and `14-final-cleanup-b.script` (`a1-hist-cleanup-final-b.log`). End state in USER: only IPM installed, repo
`r4hist` removed, `Uplift.R4.*` classes deleted, `%IPM.Generated.MacroEval` absent, 0 history rows (as at the start).

Note for the collector: `07` and `08` both emit `r4-hist-delete-dep-row-txn-*`; keep the `08` lines (latest, clean).
The `07` `notxn` lines are wrong (the "dependency" row id was the parent's); keep the `09` lines.

---

## Part A

### DEC-HIST-1 — Dependency records when the parent rolls back

- **Verdict:** RESOLVED-EVIDENCE.
- **Resolution:** Keep them (option A). A dependency load's record must survive the parent's rollback. It keeps its
  own Success (the dependency's own action worked) and reads Committed = 0, because its changes did not survive.
- **Evidence:**
  - PR #578 body: TODO "Record recursively loaded/installed modules" (checked). Review: history saving is "performed
    outside of the transaction"; IRISTEMP is used "to avoid losing data to transaction rollback".
  - `AbstractHistory.cls#L4` (HistoryTemp in IRISTEMP "so that history logs are not affected when transactions are
    rolled back due to install/load failure").
  - Wiki 09: IPM records every action "whether or not the attempt succeeded"; failures "remain visible after the
    rollback".
  - IPM test `History.cls#L716-L775` (`TestWithDependencies`) asserts one record per dependency action, but only on
    success.
  - Sub-question: wiki 09 "Committed vs. Success": "`Success` is the status the action returned. `Committed` is
    whether the action's changes survived." So Success = OK, Committed = 0 is the documented reading.
- **Consequence:** code does not match. Existing defect **F-HIST-1** stands. Implementation note: `Finalize`
  (`AbstractHistory.cls#L131`, `Committed = Committed || ISOK(status)`) runs inside the parent's transaction, so a fix
  must also set Committed from the outcome after the parent's rollback (the models' fix already does this).
  - OPEN statements: `HIST-B19`.
  - Proposed `HIST-B19` (text unchanged): "A dependency record stays when the parent load fails." Evidence: `O!:ipm-dep-parent-fails; F:F-HIST-1; O!:nc-gap-P9-chain-c-fails-txn; O!:nc-gap-P9-two-deps-parent-fails-txn; O!:nc-gap-P9-upgrade-parent-fails-txn` (drop `OPEN:DEC-HIST-1`).
  - New `HIST-B32` (parent `HIST-B19`, kind B): "A dependency undone by its parent keeps its own result. Its record says it worked, but not committed." (grade 4.96) Evidence: `F:F-HIST-1; O!:ipm-dep-parent-fails`. Source: wiki 09 "Committed vs. Success". Notes: answers the sub-question.
  - Cells `HIST-C30`, `HIST-C31`: outcome `HIST-B19, HIST-B32`; clear the Decision column.

### DEC-HIST-2 — Committed after a crash or restart once a developer-mode or no-transaction load kept changes

- **Verdict:** RESOLVED-EVIDENCE (the intended reading). The fix design (A or B) is an implementation choice, not intent.
- **Resolution:** Once a load's changes are durable, its record must read Committed = 1, even if the process then
  crashes or IRIS restarts. Option A (save the durable row when `SetCommitted` runs at `$TLEVEL` 0) is the simplest
  way that also covers restarts.
- **Evidence:**
  - `SetCommitted` comment (`AbstractHistory.cls#L119`): "Call wherever the action's changes become durable, so a later
    failure still reads as committed." Added by PR #1322 (`5d27e15b`).
  - Issue #1237 expected behaviour: Committed "should reflect whether the action's changes were actually retained";
    option 2 "Set `Committed` at each commit point".
  - Property comment `AbstractHistory.cls#L41-L42`: "Whether any of the action's changes were kept ... A
    non-committed load, install or update wrote nothing or was rolled back."
  - Wiki 09: a process that dies mid-install "still leaves a record behind"; Committed is "whether the action's
    changes survived".
- **Consequence:** code does not match. Existing defects **F-HIST-2** and **F-HIST-8**. (`SetCommitted` writes only
  the IRISTEMP copy; the durable row learns it at `%OnClose`, which a restart skips and which a developer-mode crash
  runs inside the second transaction.)
  - OPEN statements: `HIST-B11`.
  - Proposed `HIST-B11` (text unchanged): "For a load, install or update, committed off means none of its changes were kept." Evidence: `O!:ipm-halt-dev-crash-in-dependency; F:F-HIST-2; O!:nc-gap-P1-docker-notxn-after; O!:nc-gap-P1-docker-dev-after; F:F-HIST-8; O!:r4-hist-graceful-stop-txn-after` (drop `OPEN:DEC-HIST-2`; the last tag only if U19 below is accepted as a new instance).
  - Cell `HIST-C24`: clear the Decision column (outcome `HIST-B11`).

### DEC-HIST-3 — Must a record name the module once IPM read the manifest?

- **Verdict:** RESOLVED-EVIDENCE for commands that end; for a process that dies mid-action the docs accept
  placeholders.
- **Resolution:** A record of a command that ends (success, failure or refusal) must name the module and version
  whenever IPM could read them from the manifest. The name may be empty only when it was never known (no or
  malformed `module.xml`). A record left by a process that dies mid-action may keep the placeholder name and version.
- **Evidence:**
  - Wiki 09, Package column: may be empty "if the action failed before a name was known", with the example of a
    directory without a valid `module.xml`. Version: `<unset>` "when the action failed before the version was
    resolved".
  - Wiki 09: a process that dies mid-install leaves a record "with an empty end time and a placeholder version", and
    a record is "filled in when it finishes".
  - Property comment `AbstractHistory.cls#L14` (empty name "e.g. when loading a nonexistent directory");
    `LoadInit` comment `#L72` ("Package name may not known at this point").
  - IPM test `History.cls#L251-L299` (`TestDevMode`) asserts an empty package only for `malformed-module` (manifest
    unreadable). `TestDevModeRollbackNotCommitted` (`#L304-L313`, downgrade refusal) does not check the package.
  - The name is known before the refusal guards: `newModuleObj.Name` at `Utils/Module.cls#L1207`, guards at
    `#L1213-L1222`, `SetName` only at `#L1293`.
- **Consequence:** code does not match for refusals. Existing defect **F-HIST-5**, narrowed to "a refused or failed
  command that ends after the manifest was read is unnamed". Crash records (`ipm-halt-txn`, restarts) match the docs
  and are no longer a defect instance. URL loads whose clone failed (`nc-gap-P5-url-missing`) never read a manifest,
  so they match (their defect is F-LOAD-1).
  - OPEN statements: `HIST-B7`.
  - Proposed `HIST-B7`: "Once IPM has read the manifest, the record of a command that ends names the module and its version." (grade 7.35) Evidence: `O!:ipm-dev-downgrade-refused; F:F-HIST-5`. Source `ipm/src/cls/IPM/Utils/Module.cls#L1207, #L1293`; wiki 09 Package column.
  - New `HIST-N4` (parent `HIST-B7`, kind N): "A crash can leave the record with no module name and a placeholder version." (grade 6.73) Evidence: `O:ipm-halt-txn; O:nc-gap-P1-docker-txn-after; I`. Source: wiki 09 "dies mid-install".

### DEC-HIST-4 — Commands started inside a caller's transaction

- **Verdict:** RESOLVED-ASSUMPTION.
- **Resolution:** Allow it (option B). The command's work and its record follow the caller's transaction: if the
  caller rolls back, both are undone. This must be documented, and the wiki sentence that history is copied
  "outside the transaction" needs a caveat.
- **Evidence:**
  - Wiki 00 explicitly expects an "outer wrapping transaction" (`-DNoTransaction=1` advice), so refusing (option A)
    contradicts documented use.
  - `LoadNewModule` is built for nesting: `tInitTLevel` and roll back "one level at a time" (`Utils/Module.cls#L1162`,
    `#L1358-L1361`); dependency loads rely on it.
  - Nothing in code, PRs or wiki discusses a caller rollback. The history design talks about the load's own
    rollback ("rolled back due to install/load failure", `AbstractHistory.cls#L4`).
- **Assumption and alternatives:** A caller rollback is a deliberate request to undo everything, including the
  command; keeping a record that says Committed = 1 would then be false, and IRIS offers no in-process way to write
  durable data outside an open transaction (only IRISTEMP or another process). Alternatives: (A) refuse when
  `$TLEVEL` > 0: breaks the documented outer-transaction use; (C) write the record outside the caller's transaction
  (via a job, or an IRISTEMP record reconciled later): gives an audit-grade trail (the "audit trail" intent in PR #578),
  but then Committed must be recomputed after the caller's outcome. If a maintainer picks C, F-HIST-3 stays a defect.
- **Consequence:** code matches B. **F-HIST-3** becomes a documentation gap, not a code defect (wiki 09 overstates
  "outside the transaction").
  - OPEN statements: `HIST-N1`, `LOAD-N1`.
  - Proposed `HIST-N1` (kind N, parent `HIST-B1`): "If the caller rolls back, the record of the command is undone too." (grade 4.91) Evidence: `O:ipm-load-inside-caller-transaction-rolled-back; P:g_caller_rollback_leaves_no_trace`.
  - Proposed `LOAD-N1` (text unchanged): drop `OPEN:DEC-HIST-4`; Evidence `O:ipm-load-inside-caller-transaction-rolled-back; P:g_caller_rollback_leaves_no_trace`; Notes: drop "Intent open".
  - Cells `HIST-C12`, `HIST-C19`, `HIST-C26`, `HIST-C33`, `HIST-C40`: outcome `HIST-N1`; clear Decision.

### DEC-HIST-5 — Does Committed cover file changes?

- **Verdict:** RESOLVED-EVIDENCE.
- **Resolution:** Committed describes database changes only (option A). File effects outside the database are not
  covered; this should be documented.
- **Evidence:** PR #578 review, isc-dchui: Committed is about changes "made to the database because of an action"
  (AbstractHistory.cls:43). Wiki 09: "Normally the two agree, because a failed install rolls back its transaction".
  PR #1322: Committed is marked "at each commit, or before writing when there is no transaction".
  `AbstractHistory.cls#L41` ("by a commit or by writing outside a transaction"). The wiki column text "permanently
  written to disk" is looser, but its explanation is about transactions.
- **Consequence:** Committed matches. **F-LOAD-3** stays a defect on its own: a failing normal archive load replaces
  the installed module's folder and nothing restores it, while the wiki says a failed normal load "rolls the namespace
  back to its previous state" (00.-Concepts). DEC-HIST-5 no longer covers it.
  - No statement carries `OPEN:DEC-HIST-5`. Proposed: cells `LOAD-C20`, `LOAD-C25` clear the Decision column; Notes
    of `HIST-D4` and `LOAD-D7`: "Database changes only (PR #578 review; DEC-HIST-5 resolved)."

### DEC-HIST-6 — `history delete` of a running command's record

- **Verdict:** RESOLVED-ASSUMPTION.
- **Resolution:** The delete wins (option A): an operator may delete any record, finished or running, and the command
  then ends without one. A delete that hits a record still locked by an open transaction may fail with a lock error;
  that is acceptable (nothing is deleted, the error is shown).
- **Evidence:**
  - PR #578: the tamper-protection design allows "only inserts and deletes"; deletes of finalized rows are allowed by
    design (`History.cls#L15-L22`, update trigger only).
  - Wiki 09 documents `history delete -confirm` (all records) without exceptions.
  - Runtime, today's behaviour:
    - delete while the job waits before Compile, txn and developer mode: the delete wins, the load succeeds, no record
      (`r4-hist-delete-in-compile-txn-after-release`, `r4-hist-delete-in-compile-dev-after-release`); same as
      Activate in `nc-gap-P2-*`.
    - delete of a dependency's row while it runs inside the parent's transaction (txn and developer mode): refused
      after 10 s, `SQLCODE -110` "Unable to acquire lock"; the parent's row delete works; after release the
      dependency row is finalized normally and the parent has no record (`r4-hist-delete-dep-row-txn-after-release`
      in `a1-hist-delete-dep-row-txn.log`; `r4-hist-delete-dep-row-dev-after-release` in `a1-hist-delete-running-b.log`).
    - no-transaction load: both deletes win, no record left (`r4-hist-delete-dep-row-notxn-after-release`).
- **Assumption and alternatives:** The operator's explicit delete is the most recent intent, and deletes are already
  allowed on any record; a record that re-appears (C) would surprise, and blocking (B) needs new state. Alternatives:
  (B) a running record cannot be deleted: add a guard (`TimeEnd` empty and its HistoryTemp exists); today this
  happens by accident only for rows written inside an open transaction. (C) re-create at close: change
  `PersistToTwin` to re-insert when the twin is gone. If B or C is chosen, F-HIST-10 stays a defect.
- **Consequence:** code matches A for rows saved at `$TLEVEL` 0. **F-HIST-10** becomes intended behaviour; its second
  half (a `-DPackage=` filter cannot match a running row, since the name lives only in IRISTEMP until close) is a
  usability gap that belongs with F-HIST-5/RC-2. Inference, not run: a `history delete -confirm` while any dependency
  row is inside an open transaction fails as a whole with `-110` (SQL statement atomicity).
  - No statement carries `OPEN:DEC-HIST-6`. Proposed:
  - `HIST-B1`: drop `O!:nc-gap-P2-txn-by-package-after-release; F:F-HIST-10`; Notes: "An operator's delete can remove it, even while the command runs (HIST-B33)."
  - New `HIST-B33` (parent `HIST-B25`, kind B): "If you delete the record of a command that still runs, the delete wins. The command then ends with no record." (grade 3.68) Evidence: `O:nc-gap-P2-dev-confirm-after-release; O:nc-gap-P2-txn-by-package-after-release; O:r4-hist-delete-in-compile-txn-after-release; O:r4-hist-delete-in-compile-dev-after-release; O:r4-hist-delete-dep-row-notxn-after-release`.
  - New `HIST-N5` (parent `HIST-B33`, kind N): "A record written inside an open transaction is locked. A delete of it fails until that transaction ends." (grade 6.28) Evidence: `O:r4-hist-delete-dep-row-txn-after-release; O:r4-hist-delete-dep-row-dev-after-release`. Notes: SQLCODE -110 after the 10 s lock timeout; applies to dependency rows (saved inside the parent's transaction).
  - Cells `HIST-C13`, `HIST-C20`, `HIST-C27`: outcome `HIST-B33`; `HIST-C34`: outcome `HIST-B33, HIST-N5`; clear Decision.

### DEC-LOAD-1 — Operator commits at a crash prompt

- **Verdict:** RESOLVED-ASSUMPTION.
- **Resolution:** Document it as an operator error (option A). IPM does not guard against it.
- **Evidence:** None in IPM code, PRs, issues or wiki. The prompt is IRIS terminal behaviour (SYS-D16), outside IPM.
  Wiki 00 says normal mode rolls back on failure; it assumes the transaction is not committed by hand.
- **Assumption and alternatives:** The operator explicitly chose to commit a half-done transaction; IPM cannot see
  that choice. Alternatives: (B) mark the log committed before each region a halt could commit: makes the record
  honest but adds writes at every step and blurs Committed for normal crashes; (C) run loads in a job: changes
  interactive behaviour for everyone. If B is chosen, F-HIST-4 stays a defect.
- **Consequence:** code matches A. **F-HIST-4** becomes a documented operator error.
  - OPEN statements: `HIST-N2`, `LOAD-N2`.
  - Proposed `HIST-N2` (kind N, parent `HIST-B11`): "If the operator commits at a crash prompt, a half-done load stays. Its record may say it was not committed." (grade 4.24) Evidence: `O:ipm-halt-txn-operator-commits; I`.
  - Proposed `LOAD-N2` (text unchanged): drop `OPEN:DEC-LOAD-1`; Evidence `O:ipm-halt-txn-operator-commits; I`.
  - Cells `HIST-C11`, `HIST-C25`: clear Decision; `HIST-C32` (now empty): outcome `HIST-N2`.

### DEC-LOAD-2 — A plain load over a developer-mode module

- **Verdict:** RESOLVED-EVIDENCE.
- **Resolution:** Keep developer mode (option A). Only `-nodev` (or an uninstall) switches it off.
- **Evidence:**
  - Code since the first public import: `else set tDeveloperMode = +tModule.DeveloperMode` (`Utils/Module.cls#L1305-L1307`;
    present in `9713db77` "Code from HSPM", 2021; kept by `fbbb71e2` "Fix order params are applied", 2025-09).
  - Help text: `-nodev` "Disables the DeveloperMode flag" (`Main.cls#L307`), i.e. an explicit switch exists for turning it off.
  - Wiki 00: a dev-mode module "cannot be overwritten by a non-dev install"; the user must "Uninstall first, or use
    `-force`" (guard at `Utils/Module.cls#L109-L111`). So plain commands are meant not to silently drop developer mode.
- **Consequence:** `load` matches. The `update` path does not: `LoadModuleFromDirectory` replaces `DeveloperMode` with
  the override (0 for a registry directory) for every non-install load (`Utils/Module.cls#L309-L310`), so a plain
  update from a registry clears developer mode. Same code as **F-INST-5**; propose adding this case to F-INST-5
  (not a new ID).
  - OPEN statements: `LOAD-B21`.
  - Proposed `LOAD-B21` (text unchanged): "A plain load keeps the developer mode of an installed module. Only -nodev turns it off." Evidence: `O:ipm-dev-then-plain-load; I` (drop `OPEN:DEC-LOAD-2`). Source add `ipm/src/cls/IPM/Main.cls#L307`; Notes: wiki 00 Developer Mode.
  - Cells `LOAD-C26`, `LOAD-C28`, `LOAD-C29`, `LOAD-C30`: clear Decision.

### DEC-SEM-1 — `^IRIS.Temp.IPM.LoadedResourceD` after commands

- **Verdict:** RESOLVED-ASSUMPTION (strong code support, no statement).
- **Resolution:** A deliberate per-namespace tracking table in IRISTEMP (option A). It lives until IRIS restarts, an
  explicit `Clear`, or IPM's own full cleanup. It should be documented as IPM working data, not residue.
- **Evidence:**
  - `Storage/LoadedResource.cls`: `TrackResourceNames` (`#L26-L71`) deletes and re-inserts the rows of one
    module/resource on every load, so it keeps a current list across commands. `Validate` (`#L76-L111`) compares
    rows of different modules, which needs rows from earlier commands. `Clear` (`#L116-L130`) is a separate
    `@API.Method`. Foreign keys cascade on module and resource delete (`#L14`, `#L16`).
  - Comment `#L63`: "Since using IRISTEMP, need an alternative approach..." (IRISTEMP chosen on purpose).
  - IPM self-uninstall full cleanup kills it with other "operational globals" (`Lifecycle/Module.cls#L52`, commit
    `ffba34ec`, PR #1106), so maintainers see it as IPM state.
  - Class dates from the 2021 HSPM import (`9713db77`); no later design change.
- **Assumption and alternatives:** The table only makes sense if it outlives one command. Alternative (B): clear at
  the end of each command: `Validate`/`ReportConflicts` would then see only the current command's resources.
- **Consequence:** code matches A. No ontology statement carries `OPEN:DEC-SEM-1`. Proposed: in
  `semantics/coverage.json` replace `DEC:DEC-SEM-1` by `I` with source `ipm/src/cls/IPM/Storage/LoadedResource.cls#L26-L130`;
  classify `IpmWorkingData` as allowed to persist.

### DEC-SEM-2 — IRISSYS code globals written by uninstall of a web-app module

- **Verdict:** RESOLVED-ASSUMPTION.
- **Resolution:** Not an IPM write. IPM's uninstall writes no IRISSYS code global; the one-off writes seen in one run
  came from IRIS itself. Accept (option A) and keep IRISSYS code globals out of IPM's write frame, with a note.
- **Evidence:**
  - The IRISSYS code writes appear only in `validation/logs/55-write-sets-review.log` (`ws-uninstall-webapp`). Later
    runs of the same measurement show none: `57-write-sets-all.log` and `58-write-sets-webapp.log` (the record kept in
    `evidence/iris-observations.json` is the 58 one; it lists no IRISSYS global).
  - New per-class diff of IRISSYS `^oddDEF`, `^oddCOM`, `^rOBJ`, `^rINDEX`, `^rINDEXCLASS`, `^ISC.Src.Jrn`, `^oddDEP`
    (5375 entries): nothing changed for load or uninstall of `rest-app` (`r4-hist-sem2-load-rest-app`,
    `r4-hist-sem2-uninstall-rest-app`).
  - IPM's only runtime compile on this path is `%IPM.Generated.MacroEval` (`Utils/Module.cls#L2746-L2785`), used to
    evaluate `$$$` macros such as rest-app's `AutheEnabled="#{$$$AutheCache + ...}"`. It is compiled and deleted where
    `%IPM` lives, not in IRISSYS (`r4-hist-sem2-evaluate-macro`: no IRISSYS change).
- **Assumption and alternatives:** the 55 change is most likely IRIS-internal (first-use compilation after the
  instance started); the cause was not identified. Alternative (B): rerun the 55 sequence on a fresh instance with a
  per-class diff to name the classes.
- **Consequence:** IPM matches. Side observation (new, low): a `$$$` macro that fails to compile leaves
  `%IPM.Generated.MacroEval` defined until the next successful evaluation deletes it (`r4-hist-sem2-evaluate-macro-fails`:
  `macroEvalClassNow=10`; cause `$$$ThrowOnError` at `#L2781` runs before the `Delete` at `#L2783`).
  - No ontology statement carries `OPEN:DEC-SEM-2`.

### DEC-SYS-1 — Lexical scope for "Definitions first"

- **Verdict:** RESOLVED-ASSUMPTION.
- **Resolution:** Lexical scope (option A): a statement may use a term defined by an ancestor, or by an earlier
  sibling of itself or of any ancestor. A child inherits what is in scope at its parent's position.
- **Evidence:** playbook §3.1 (`omnisaas/playbook/verification-playbook.txt#L216-L221`): a statement may use terms
  defined "by its ancestors (its parent chain)" or "by an earlier statement at its own level under the same parent";
  "Never use a term first and define it later." The gate text (`#L228-L229`) repeats this for "the module glossary".
  The text names the system root (`#L205-L207`, "as pg-verify's `SYS-*` rows do") but does not say how `SYS-*`
  definitions reach module statements.
- **Assumption and alternatives:** Read literally, the rule makes definitions under a module root (`XXX-D*`, siblings
  of the behaviours) unusable by any nested behaviour, and `SYS-D*` unusable by any module statement. That defeats the
  layout the same section prescribes (definitions first, behaviours nested under them). The rule's stated purpose is
  order: define before use. Lexical scope keeps that purpose: every term still has a definition that comes earlier and
  encloses the use. Measured impact of the strict reading (B) on this ontology (copy of `scripts/ontology_index.py`
  with the ancestor-sibling clause removed): 1040 violations in 585 statements; 256 of those statements fail on a
  module's own definitions, not only on `SYS-D*`. Under (B) every module would need its definitions re-parented into a
  chain above all behaviours, or repeated.
- **Consequence:** the index script matches A. Proposed: add one sentence to the playbook §3.1 (owner's choice):
  "A child may also use any term that is in scope for its parent." No ontology change.

---

## Part B

### U1 — Can anything fail after the final commit in `LoadNewModule`? (premise PM)

- **Status: CLOSED. PM is false.**
- **Runtime:** fixture `r4h-msg2` has `<AfterInstallMessage>Installed at {$root} $$$ISERR</AfterInstallMessage>`. A
  normal load compiles and activates, commits (`#L1348`), calls `SetCommitted`, then `WriteAfterInstallMessage`
  (`#L1354`) → `%Evaluate` (`Storage/Module.cls#L1833`) → `%EvaluateMacro` → `EvaluateMacroHelper` compiles a class
  with `Quit $$$ISERR`, which fails ("MPP5611 Function macro missing arguments") and is thrown by `$$$ThrowOnError`
  (`Utils/Module.cls#L2781`). The catch turns it into the command's status.
  - `r4-hist-post-commit-throw-r4h-msg2-txn`: command error; module installed; class compiled; record
    `success:err/committed:1/ended:1/finalized:1`, all five phases ok. Same for `-notxn` and `-dev`.
  - Control: a message with no `{` is never evaluated (`%Evaluate` returns early), so `$$$` alone does not throw
    (`r4-hist-post-commit-throw-txn`, `r4-hist-macro-probe-iserr`: `moduleEvaluate=ok`). `#{1/0}` is caught
    (`%EvaluateExpression`). This is why the earlier attempt (`ipm-post-commit-message-fails`) did not throw.
- **Other post-commit failure points (code only, not run):** `tmpRepoMgr.CleanUp()` after `LoadNewModule`
  (`Main.cls#L2530`, `Utils/Module.cls#L140`, `#L259`) adds its error to an already committed load; `Finalize` and
  `%OnClose` run after the commit too; `CollectAnalytics` (`Utils/Module.cls#L193-L195`) is inside the try for remote
  registries.
- **History is honest** (Committed = 1, the changes stayed). What breaks is the command result: a normal load reports
  failure with every change kept, which contradicts LOAD-B8 and wiki 00 ("any failure rolls the namespace back").
- **New defect (proposed F-LOAD-4, low):** "A normal load can fail after its final commit. A manifest whose
  AfterInstallMessage has a `$$$` macro that does not compile (or a failing `.modules` cleanup) makes the command
  return an error while the module stays installed and the record says committed."
  - Proposed ontology: `LOAD-B8` add `O!:r4-hist-post-commit-throw-r4h-msg2-txn; F:F-LOAD-4`. New `LOAD-N4` (parent
    `LOAD-B8`, kind N): "A normal load can fail after its last commit. Then its changes stay, and its record is committed." (grade 3.0) Evidence `O:r4-hist-post-commit-throw-r4h-msg2-txn; F:F-LOAD-4`.
  - In every Notes field that says "Outside the model: PM, nothing fails after the final commit
    (load_general.TRUST.md G7)", replace with "PM fails (F-LOAD-4): a post-commit error reports failure with changes
    kept; the history record stays honest." `load_general.TRUST.md` G7 and QUESTIONS U1 should cite these OBS names.

### U19 — Graceful `iris stop` with an open load transaction

- **Status: CLOSED for transactional loads (two runs, two timings, same result); IRIS is back up and IPM works**
  (`zpm` version listed in every namespace after each restart, `a1-hist-u19-after-txn.log`,
  `a1-hist-u19-after-txn-wait20.log`). Container `ipm-uplift-iris` restarted twice (IRIS only; the container kept
  running); IRIS running since 2026-11-13 23:41:55.
- **What ran:** a normal (transactional) load of `r4h-dep-wait` waits in Activate (Invoke `Wait`, ends by itself after
  120 s, or 20 s in the second run). From the host: `iris stop IRIS quietly`, then `iris start IRIS`.
- **Run 1 (wait 120 s)** (`a1-hist-u19-stop-txn-host.log`, messages.log excerpt in it):
  - IRIS logged "Waiting for open transactions to commit" and listed the job, for about 2 minutes; then "Stopping User
    Jobs", "Waiting for users to stop", "1 user process(es) remain" for about 3 more minutes; then shut down
    ("Transaction rollback not required at next startup"). `iris stop` took 320 s. The container kept running.
  - After restart (`r4-hist-graceful-stop-txn-after`): module `r4h-dep-wait` installed and its class compiled (the
    load's transaction committed), but the record reads `load//`, committed 0, not ended, not finalized; IRISTEMP is
    empty. So a graceful stop can leave a **committed normal load recorded as Committed = 0, unnamed and unlocked**:
    the F-HIST-8 shape, now for a transactional load. The commit landed and `%OnClose` never ran (the stop of user
    jobs came right after the commit window that F-HIST-7 describes, and the job then did not end until forced).
- **Run 2 (wait 20 s)** (`a1-hist-u19-stop-txn-wait20-host.log`, `r4-hist-graceful-stop-txn-wait20s-after`): same
  result. The open-transaction wait ended at 23:36:34, right when the Invoke returned (the load committed); then
  "Stopping User Jobs" and "1 user process(es) remain...(repeated 58 times)" until 23:41:53, when IRIS shut down
  anyway (`iris stop` took 332 s). While it waited, the job's OS state was a timed sleep (`hrtimer_nanosleep`), not
  IPM code (IPM has no `hang` on this path; its only `hang` is `Main.cls#L2371`, git clone). After restart: module and
  class kept, record `load//`, committed 0, not ended, not finalized. Both runs started with `r4h-dep-wait`
  uninstalled (`a1-hist-delete-dep-row-notxn.log` and `a1-hist-u19-after-txn.log` end with its uninstall), so "kept"
  means the load's own commit.
- **Reading:** graceful shutdown waits for open transactions to commit, then stops jobs. A load that commits during
  that wait is stopped right after its commit, i.e. exactly inside the commit-to-close window (RC-2). Hypothesis, not
  proven: the job then blocks on its first durable write after the commit (the `%OnClose` save of the twin row),
  because shutdown has already stopped user updates; it never ends until IRIS forces it down. This widens
  F-HIST-7 from "two statements" to "every graceful stop that waits for a load". Proposed: add as an instance of
  F-HIST-7/F-HIST-8 (same root cause RC-2), not a new ID; the DEC-HIST-2 option A fix (persist the durable row at the
  `$TLEVEL` 0 `SetCommitted`) covers it, since `SetCommitted` runs right after the outer `TCOMMIT`.
- **Ontology effect:** `HIST-B12` ("Outside a crash while a developer-mode load has needs loading, that holds") and
  `HIST-B11` are contradicted for a transactional load stopped gracefully after its commit. Proposed: add
  `O!:r4-hist-graceful-stop-txn-after; O!:r4-hist-graceful-stop-txn-wait20s-after; F:F-HIST-7` to `HIST-B11` and
  `HIST-N3`; in `HIST-B12` Notes, state that premise P3 (no crash in the commit window) also excludes a graceful
  `iris stop` that waits for the load's transaction. `LOAD-B7`/`LOAD-B9` are not affected (the changes were
  committed and kept as the load intended).
- **Not run:** `iris force` in developer and no-transaction modes. P1's `docker restart` runs (which became forced
  shutdowns) already cover those modes (`nc-gap-P1-docker-dev-after`, `-notxn-after`).

### U20 — `history delete` during other phases, or of a dependency's row

- **Status: CLOSED.** See DEC-HIST-6 evidence. Summary: a delete during Compile (txn and developer mode) wins like in
  Activate; a dependency's row written inside the parent's open transaction cannot be deleted until that transaction
  ends (SQLCODE -110 after 10 s), so it survives; in a no-transaction load every row can be deleted and the delete wins.

---

## Summary

| ID | Verdict | Resolution (one line) | Code matches? |
|---|---|---|---|
| DEC-HIST-1 | RESOLVED-EVIDENCE | Dependency records survive the parent's rollback; Success kept, Committed 0 | no (F-HIST-1) |
| DEC-HIST-2 | RESOLVED-EVIDENCE | Committed = 1 as soon as changes are durable, even across crash or restart | no (F-HIST-2, F-HIST-8) |
| DEC-HIST-3 | RESOLVED-EVIDENCE | A command that ends after reading the manifest names the module; crash records may keep placeholders | no for refusals (F-HIST-5, narrowed) |
| DEC-HIST-4 | RESOLVED-ASSUMPTION | Allow caller transactions; work and record follow the caller; document it | yes (F-HIST-3 becomes a doc gap) |
| DEC-HIST-5 | RESOLVED-EVIDENCE | Committed covers database changes only | yes (F-LOAD-3 stays a separate defect) |
| DEC-HIST-6 | RESOLVED-ASSUMPTION | The delete wins; a lock error on a row inside an open transaction is acceptable | yes (F-HIST-10 becomes intended) |
| DEC-LOAD-1 | RESOLVED-ASSUMPTION | Operator commit at a crash prompt is a documented operator error | yes (F-HIST-4 becomes doc) |
| DEC-LOAD-2 | RESOLVED-EVIDENCE | Plain load keeps developer mode; only -nodev turns it off | load yes; update no (extend F-INST-5) |
| DEC-SEM-1 | RESOLVED-ASSUMPTION | LoadedResource is a deliberate IRISTEMP tracking table, lives until restart | yes |
| DEC-SEM-2 | RESOLVED-ASSUMPTION | IRISSYS code writes are not IPM's (not reproduced); accept | yes |
| DEC-SYS-1 | RESOLVED-ASSUMPTION | Lexical scope: children inherit what is in scope for their parent | yes (script) |
| U1 | CLOSED | PM false: an AfterInstallMessage `$$$` compile error fails a committed load | new F-LOAD-4 |
| U19 | CLOSED | Graceful stop can leave a committed normal load recorded Committed 0, unfinalized | RC-2 instance (F-HIST-7/8) |
| U20 | CLOSED | Delete wins except rows locked inside an open transaction (-110) | per DEC-HIST-6 |
