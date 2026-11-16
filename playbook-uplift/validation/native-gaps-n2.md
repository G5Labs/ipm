# Native gap checks, round n2 (P1–P9)

Builder: native-validation builder n2, 2026-11-13. IRIS Community 2026.1 (build 234U, ARM64 image
`containers.intersystems.com/intersystems/iris-community:2026.1`). IPM from the pinned archive of `571e04a1`, installed
with `IPM.Installer.setup` as in `validation/logs/00-install-ipm.log` (`validation/logs/98-n2-00-install-ipm-*.log`).

Containers (all on `ipm-uplift-net`, all removed at the end):

| Container | Used for | IPM |
|---|---|---|
| `ipm-uplift-n2-a` | P1–P6, P9 (git 2.43.0 installed with apt for P5) | pinned archive, unpatched |
| `ipm-uplift-n2-p` | P8 only | pinned archive **with the two probe patches** below |
| `ipm-uplift-n2-c3` | P7 (throwaway; IPM removed and reinstalled three times) | pinned archive, unpatched |

Two earlier P7 containers (`-n2-c`, `-n2-c2`) were discarded with their logs: the first had a probe bug (no
observation was written before its full cleanup), the second listed killed globals as present. Every P7 result below
comes from `-n2-c3`.

Harness: classes `Uplift.N2.Base` (helpers; `Observe` generalizes `Uplift.Probe.Scenario:Observe` to any module and
any `N2*` class, and adds `invokesRan` = each Invoke that ran, with its `$TLEVEL`), `Uplift.N2.Gaps` (P1–P6, P8, P9),
`Uplift.N2.Self` (P7, reads globals only) in `validation/iris/n2/Uplift/N2/`. Fixtures in
`validation/iris/n2/fixtures/`. The Invoke target `N2H.Hooks` is compiled into the namespace directly (it is not a
module resource). A failure is switched on with `^N2Fail(<module>)`, set outside any load. Scripts:
`validation/iris/n2/NN-*.script`. Logs: `validation/logs/98-n2-*.log` (IRIS sessions) and `99-n2-*.log` (host-side
restart transcripts). All observations are named `nc-gap-P<k>-*` and are in `evidence/iris-observations.json` (67
records; `scripts/collect_evidence.py` was re-run).

Notation in observation values: a history row is
`id<ID>/<action>/<package>/<version>/success:<ok|err>/committed:<0|1>/ended:<0|1>/finalized:<0|1>/phases:<name>:<ok|err>[(open)] ...`.
`(open)` = the phase has no end time. `mod:<name>` = module record (`/dev` = developer mode). `cls:<class>` = compiled
or defined, with `V:` = the version string the compiled marker class returns.

---

## P1. IRIS restart while a load hangs in Activate — **closed**

**What ran.** A job loads `n2-wait` (its Invoke waits for a release flag in IRISTEMP, at most 300 s). The script
waits until the job is inside Activate and records the state. From the host it then runs either `docker restart`
(modes txn, dev, notxn) or `iris force IRIS quietly` followed by `iris start IRIS` inside the container (mode txn).
A second session reads the state again. Scripts `20-p1-start-<kind>-<mode>.script`, `21-p1-after.script`. Driver
transcript with `messages.log` excerpts: `99-n2-20-p1-<kind>-<mode>-restart.log`.

**Result.**
- `docker restart` returned in about 1.3 s. IRIS logged "Waiting for open transactions to commit", was then stopped
  hard, and at startup logged "Previous system shutdown was abnormal" and "Rolling back transactions". So
  `docker restart` behaved like `iris force` here (both are crash restarts).
- **txn** (both restart kinds): the module record and its class are gone (rolled back). The history row stays as it
  was saved at Init: `load//0.0.0`, committed 0, no end time, **finalized 0**, no phases. IRISTEMP is empty
  (`historyTempLeft=0`). Because the row is not finalized, an SQL UPDATE of it succeeds (`sqlUpdateSQLCODE=0`).
- **dev** and **notxn** (docker restart): the module record (`1.0.0/dev` or `1.0.0`) and its compiled class **stay**,
  but the row says committed 0, no package, version 0.0.0, unfinalized. The IRISTEMP copy that held Committed = 1 was
  lost with IRISTEMP. IRIS does not run `%OnClose` for processes killed by a shutdown, unlike a `halt`
  (compare O:ipm-halt-notxn, O:ipm-terminate-notxn, which are committed).
- During the hang (`*-during`), other processes already see the uncommitted module record and compiled class
  (IRIS has no read isolation). The durable row has no package name while the command runs.

Observations: `nc-gap-P1-docker-txn-during/-after`, `nc-gap-P1-docker-dev-during/-after`,
`nc-gap-P1-docker-notxn-during/-after`, `nc-gap-P1-force-txn-during/-after`.

**Candidate new finding (N2-F1): a restart during a load leaves an unlocked Init row that says "not committed",
even when the load's changes were kept.** Root cause RC-2 (Committed lives only in IRISTEMP until the log closes),
reached without a dependency: any developer-mode load past its first commit, or any no-transaction load past
`#L1225`, killed by an IRIS restart. Also the row is never finalized, so the edit trigger (HIST-B22) does not protect it.

## P2. `history delete` of a running record (DEC-HIST-6) — **closed**

**What ran.** A job loads `n2-wait` and waits in Activate. Another process:
(a) txn mode: `history delete -DPackage=n2-wait`, then `history delete -DID=<row id>`;
(b) developer mode: `history delete -confirm`.
Then the job is released and the row is read again. Script `14-p2.script`, log `98-n2-14-p2.log`.

**Result.** `-DPackage=n2-wait` deletes 0 records: while the command runs, the durable row has an empty package
(the name is set only in the IRISTEMP copy, `SetName` at `Utils/Module.cls#L1293`). `-DID=<id>` and `-confirm` both
delete the running row. After release the job ends with success (`jobResult=ok`), its IRISTEMP copy is removed, and
**the row does not come back** (`rowExistsAfterRelease=0`, `historyRows=0`). Cause in code: `%OnClose` →
`PersistToTwin` reloads the temp record; the twin reference no longer opens, and the `if (..PersistedTwin '= "")`
guard (`HistoryTemp.cls#L34`) skips the write. So today's behaviour is DEC-HIST-6 option (A) "the delete wins", and
the successful load then has no history record at all. The IRIS contract's re-save behaviour
(O:env-resave-after-rollback) does not apply because the twin is re-opened, not re-saved from memory.

Observations: `nc-gap-P2-txn-by-package-during`, `nc-gap-P2-txn-by-package-after-release`,
`nc-gap-P2-dev-confirm-during`, `nc-gap-P2-dev-confirm-after-release`.

## P3. Phase record for a phase-level Invoke that fails by returned status — **closed**

**What ran.** Fixtures with `<Invoke Class="N2H.Hooks" Method="Boom" Phase="Activate" When="After" CheckStatus="1"/>`
(`n2-ph-after`; modes txn, dev, notxn), `When="Before"` (`n2-ph-before`; txn) and `Phase="Compile" When="After"`
(`n2-ph-compile`; txn). Each load is followed by `history details <id> -phases`. Script `10-p3.script`, log
`98-n2-10-p3.log`.

**Result.** In all five runs the failing phase is **left open**: `Activate:err(open)` (or `Compile:err(open)`), no end
time. `history details` shows its result as `ERROR #00: (no error description)`: this is the initial value 0 of
`HistoryPhase.Success`, not the Invoke's error. The record's overall Success holds the real error, and the record is
ended and finalized. The cause is the `quit:$$$ISERR(tSC)` at `Storage/Module.cls#L671` (after the loop at #L669) and
`#L610` (Before): the quit leaves the `try` without `FinalizePhase`, and the error is rethrown at `#L707`. For
comparison, an Invoke on the default phase (Configure, nested in Activate) closes Activate with its error
(O:ipm-activate-fails). Committed values follow the mode as usual (0 / 1 / 1).

Observations: `nc-gap-P3-n2-ph-after-txn`, `-dev`, `-notxn`, `nc-gap-P3-n2-ph-before-txn`, `nc-gap-P3-n2-ph-compile-txn`.

**Candidate new finding (N2-F2): a phase that fails by an Invoke's returned status stays open in history, with a
placeholder error.** Severity low (display/audit).

## P4. Archive loads (DEC-HIST-5) — **closed**

**What ran.** `tar czf` builds `n2-arc-1.0.0.tgz` (good: `payload.txt` = `payload-good`, marker V = `1.0.0`) and a
second archive of the same name and version whose Activate fails (`payload-bad`, marker source V = `1.0.0-bad`).
Load good; write `local-edit.txt` into `<ModuleRoot>/n2-arc/1.0.0/`; load bad, in txn mode and (after a reset) in
developer mode. ModuleRoot is `/usr/irissys/ipm/`. Script `11-p4.script`, log `98-n2-11-p4.log`.

**Result.**
- Good load: files at `/usr/irissys/ipm/n2-arc/1.0.0/`, module record `Root` points there, committed 1.
- Bad load, **txn**: the load fails and rolls back. The record is still `1.0.0` with the same `Root`, and the compiled
  class returns `V:1.0.0` (good). But the folder was replaced: `payload.txt` = `payload-bad`, the marker source in the
  folder is the bad one, and `local-edit.txt` is gone. History: `committed:0`. So **the installed module's root folder
  now holds the sources of a load that history says left nothing**, and they differ from the compiled code.
- Bad load, **dev**: the bad code is kept (`V:1.0.0-bad`), the folder is the bad one, committed 1 (consistent).
- The temp extraction folder under `mgr/Temp` is removed in both cases.

Observations: `nc-gap-P4-txn-good-archive`, `nc-gap-P4-txn-bad-archive-same-version`, `nc-gap-P4-dev-good-archive`,
`nc-gap-P4-dev-bad-archive-same-version`.

Observed fact for DEC-HIST-5: Committed today describes database changes only. A failed transactional archive load
reports Committed 0 after deleting and rewriting the module root folder of the installed module.

## P5. URL (git) loads — **closed**

**What ran.** git 2.43.0 installed in `ipm-uplift-n2-a` with `apt-get` (`98-n2-05-git-install.log`). Two bare repos
(`n2-git`, whose Activate works, and `n2-gitfail`, whose Activate fails) are served by dumb HTTP
(`git update-server-info`, `python3 -m http.server 8765` in the same container; setup and a probe clone in
`98-n2-15-p5-setup.log`). Loads: `load http://localhost:8765/n2-git.git` (txn, `-dev`), `n2-gitfail.git` (txn,
`-DNoTransaction=1`) and a URL that does not exist. Script `15-p5.script`, log `98-n2-15-p5.log`.

**Result.**
- Success (txn and dev): module installed, one history row, committed 1, `SourceName` empty. The module record's
  `Root` is the clone in `/usr/irissys/mgr/Temp/<timestamp>/n2-git/`. **Every URL load leaves its clone there**
  (`newTempDirs=1` in all five cases, including failures); nothing removes it, and the installed module's Root depends
  on it.
- Failure, txn: rolled back, committed 0, the clone stays. Failure, notxn: module and class kept, committed 1.
- Missing repository: `git clone` prints `fatal: repository ... not found`, but IPM goes on. The command fails later
  with "File not found to obtain module object: .../mgr/Temp/<ts>/n2-nope/module.xml", the row is unnamed
  (`load//0.0.0`, an F-HIST-5 instance), and an empty temp folder is left. Cause: `LoadFromRepo` calls
  `RunCommandViaZF` (`Main.cls#L2358`), which returns OK whatever git's exit code is (`Utils/Module.cls#L2409`,
  `pRetCode` is not checked).

Observations: `nc-gap-P5-url-ok-txn`, `nc-gap-P5-url-ok-dev`, `nc-gap-P5-url-fail-txn`, `nc-gap-P5-url-fail-notxn`,
`nc-gap-P5-url-missing`.

**Candidate new findings:** (N2-F3) a URL load ignores a failed `git clone` and reports a misleading error;
(N2-F4) URL loads leave their clone in `mgr/Temp` and point the module's Root at it.

## P6. Update step ends between its effect and its record save (INST-B67, INST-B69) — **closed**

**What ran.** `n2-upd` 1.0.0 (update package `N2Upd.Upd`, step `V1.S1`) is loaded. A job then runs
`update n2-upd -path .../n2-upd-2.0.0/` (txn, `-dev`, `-DNoTransaction=1`). Step `V2.A` increments an ordinary global
`^N2UpdX`, does `$INCREMENT(^N2UpdY)`, appends a line to `/tmp/n2-upd-steps.txt`, then `halt`s when `^N2HaltStep="A"`
(the process ends after the effect and before `ExecuteUpdateStep` saves the record, `Storage/Module.cls#L244-L246`).
Then the flag is cleared and the same update is run again in the session. Script `12-p6.script`, log `98-n2-12-p6.log`.

**Result.**

| Mode | After the halt | After the retry |
|---|---|---|
| txn | version 1.0.0, `N2Upd.Upd.V2` class gone, no step records for V2, `^N2UpdX` undefined (rolled back), `^N2UpdY`=1, file 1 line. Row `update/n2-upd/0.0.0`, committed 0, ended 0, **finalized 0**, no phases | 2.0.0; A ran again (`stepARuns=2`): `^N2UpdY`=2, file 2 lines, `^N2UpdX`=1; A and B records ok |
| dev | 2.0.0/dev kept, `^N2UpdX`=1, `^N2UpdY`=1, 1 line; V2.A record exists with no start and no end (`start:0/end:0/ok:1`, created by `GetAddUpdateStep` #L270), no B record. Row committed 1, ended 0, finalized 1, `ApplyUpdateSteps:err(open)` | A ran again: `^N2UpdX`=2, `^N2UpdY`=2, 2 lines; all records ok |
| notxn | same as dev (not in developer mode) | same as dev |

So a step whose process ends after its effect and before its record is saved **runs again** on the next update. Its
non-transactional effects happen twice in every mode, and its transactional effects too in dev and notxn modes.
The retry finishes the update (INST-B69: version 2.0.0, every step record ok). Side fact: in developer mode the load
of 1.0.0 does not seed `V1.S1` (`Lifecycle/Module.cls#L122`), so the update runs it (`start:1`).

Observations: `nc-gap-P6-txn-halt-in-step`, `nc-gap-P6-txn-retry`, `nc-gap-P6-dev-halt-in-step`,
`nc-gap-P6-dev-retry`, `nc-gap-P6-notxn-halt-in-step`, `nc-gap-P6-notxn-retry`.

## P7. Self-uninstall with full cleanup and with -force — **closed**

**What ran** (throwaway container `-n2-c3`, in this order). Each round loads `n2-solo`, the chain `n2-ca`→`n2-cb`→`n2-cc`
(from filesystem registry `n2`) and `n2-upd`, sets a user global `^N2UserData`, records the state from globals only,
removes IPM, then reinstalls IPM with `IPM.Installer.setup` and reads the state again.
1. `uninstall zpm -force` (`41-p7-force-a/b`).
2. `uninstall zpm -force -DClean.FullCleanup=1` (`42-p7-force-fullcleanup-a/b`), then a probe that adds the registry
   again and loads a module with update steps.
3. Cause check (`43-p7-fscache-cause`): add the registry with the leftover cache globals, then again after killing
   `^IPM.Repo.Filesystem.CacheI` and `CacheS` by hand.
4. `uninstall zpm` answered `Yes` at the full-cleanup prompt (`40-p7-full-a/b`), then the same probe.

**Result.**
- `-force`: the module list is printed, no question is asked, no full cleanup. "WARNING: 5 module(s) orphaned" is
  printed and the shell exits. `%IPM` classes are gone; module records, their classes, registry definitions, step
  records and history stay. After reinstall all 5 modules are listed again (UNIN-B76).
- `-force -DClean.FullCleanup=1`: **full cleanup without any question** ("Performing full cleanup: uninstalling all
  modules..."). Every module is uninstalled first (5 uninstall rows), then IPM.
- `Yes` at the prompt: the same full cleanup as above.
- After a full cleanup: no module records, no `N2*` classes, no `%IPM` classes. History is kept (5 → 11 rows; the last
  row is `uninstall/zpm`, committed 1, ended, finalized). `^IPM.settings` in `%SYS` is kept, and so is the user global.
  The namespace's `%IPM` package and routine mappings stay. **Left behind:** `^IPM.Repo.Filesystem.CacheI` and
  `^IPM.Repo.Filesystem.CacheS` (the cleanup kills only `CacheD`, `Lifecycle/Module.cls#L41-L43`).
- After reinstall, adding a filesystem registry fails with `ERROR #5809: Object to Load not found, class
  '%IPM.Repo.Filesystem.Cache', ID '7'` ... `'12'`, and the next `install` says "No repositories are configured and
  enabled in this namespace." Loading a module with update steps works. Cause confirmed: after killing the two
  leftover globals by hand, the same registry add works and `install n2-cb` succeeds
  (`nc-gap-P7-fscache-stale` vs `-repaired`).
- `^||%IPM.SelfUninstallFlag` is cleared in every case.

Observations: `nc-gap-P7-force-prepare`, `nc-gap-P7-force`, `nc-gap-P7-force-after-reinstall`,
`nc-gap-P7-force-fullcleanup-prepare`, `nc-gap-P7-force-fullcleanup`, `nc-gap-P7-force-fullcleanup-after-reinstall`,
`nc-gap-P7-force-fullcleanup-post-probe`, `nc-gap-P7-fscache-stale`, `nc-gap-P7-fscache-repaired`,
`nc-gap-P7-full-prepare`, `nc-gap-P7-full-cleanup-yes`, `nc-gap-P7-full-cleanup-after-reinstall`,
`nc-gap-P7-full-cleanup-post-probe`.

**Candidate new finding (N2-F5): a full cleanup leaves the filesystem registry cache index. After IPM is reinstalled,
filesystem registries cannot be added in that namespace** (until the two globals are killed by hand).

## P8. Probe-patched IPM (playbook §4.2 trace conformance) — **closed**

**Patches** (applied to a copy of the archive; unified diffs in `validation/iris/n2/patches/`; checkout untouched):
- `p8a-crash-after-outer-commit.diff`: `Utils/Module.cls`, one line after the outer `tcommit` (#L1348), before
  `SetCommitted`: `if $get(^UpliftCrashAt)="after-commit" { halt }`.
- `p8b-force-deployed-export-error.diff`: `ResourceProcessor/Default/Document.cls`, inside `OnExportDeployedItem`
  just before `AddItemToProject` (#L318): `if $get(^UpliftForceDeployErr) { $$$ThrowStatus(...) }`. So the
  deployed-export call made at #L255 returns an error with `pItemHandled = 0`, which is the case F-RES-1 describes
  (the switch is in the callee, not at #L256-L257).

Both switches are inert unless their global is set. Install log `98-n2-00-install-ipm-probe.log`; run log
`98-n2-30-p8-probe.log` (script `30-p8-probe.script`).

**(a) F-HIST-7 / premise P3.** A job loads `n2-solo` with `^UpliftCrashAt="after-commit"`. Result: the job halts
after the commit. The module and class are installed, but the row is `success:err/committed:0/ended:0/finalized:1` with
every phase ok. The control run without the switch gives committed 1, ended 1. **F-HIST-7 confirmed natively**: the
committed load is recorded as not committed (`%OnClose` ran at the halt, with no transaction open, and persisted the
IRISTEMP copy before `SetCommitted`).

**(b) F-RES-1.** `package n2-dpl -v -path /tmp/n2pkg/<case>/` on a module whose class is `Deploy="true"`.
Control: the `.tgz` holds `studio-project/Deployed.xml` and `module.xml`, and no file contains the method source.
With the forced error: **the command returns OK**, prints "Exporting 'N2Dpl.Marker.CLS' to .../src/cls/N2Dpl/Marker.cls",
and the `.tgz` holds `src/cls/N2Dpl/Marker.cls` with the source (`ClassMethod V`) and no `Deployed.xml`.
**F-RES-1 confirmed natively.**

Observations: `nc-gap-P8-commit-window-control`, `nc-gap-P8-commit-window-after-commit`,
`nc-gap-P8-deployed-export-control`, `nc-gap-P8-deployed-export-forced-error`.

**Trusted items to add to TRUST.md:** the two probe patches above (they add one line each and change nothing unless
`^UpliftCrashAt` / `^UpliftForceDeployErr` is set; checked by the control runs `nc-gap-P8-commit-window-control`
(committed 1, ended 1, as O:ipm-load-ok) and `nc-gap-P8-deployed-export-control` (object code only, as RES-B22)). Also trusted: the
`N2H.Hooks` Invoke target and the `Uplift.N2.*` probe classes (same role as T9).

## P9. Model bounds — **closed**

**What ran.** For each mode (txn, `-dev`, `-DNoTransaction=1`), with registry `n2` holding `n2-cb` (needs `n2-cc`),
`n2-cc`, `n2-pd` 1.0.0 and 1.1.0, `n2-d1`, `n2-d2`:
- chain-ok / chain-c-fails: load `n2-ca` (needs `n2-cb` → `n2-cc`); `n2-cc`'s Activate fails in the second.
- upgrade-ok / upgrade-parent-fails / upgrade-dep-fails: `n2-pd` 1.0.0 is installed first (plain load); then load
  `n2-pa` (needs `n2-pd` 1.1.0); either nothing fails, or `n2-pa` fails, or `n2-pd` 1.1.0 fails.
- two-deps-parent-fails: `n2-tp` needs `n2-d1` and `n2-d2`; both work, then `n2-tp` fails.
Script `13-p9.script`, log `98-n2-13-p9.log`.

**Structure (all modes).** A dependency chain is **flattened**. The top module's `LoadDependencies` loads the whole
graph in order (`n2-cc`, then `n2-cb`), each at the same nesting level (`invokesRan=n2-cc:2 n2-cb:2 n2-ca:1` in
txn). So a chain of any depth gives one level of nested transactions under the parent. When `n2-cc` fails,
`n2-cb` is never started.

| Case | txn | dev | notxn |
|---|---|---|---|
| chain-ok | 3 rows ok; all installed | 3 rows ok; `n2-ca` dev, deps normal mode | 3 rows ok |
| chain-c-fails | **1 row** (`n2-ca`, committed 0, no phases); nothing installed | 1 row (`n2-ca`, committed 1, no phases); `n2-ca` record kept **without its class**; deps absent | 2 rows (`n2-ca` committed 1, no phases; `n2-cc` err, committed 1); `n2-ca` record without class, `n2-cc` installed |
| upgrade-ok | `n2-pd` → 1.1.0; 3 rows | same | same |
| upgrade-parent-fails | `n2-pd` back to **1.0.0** (class V 1.0.0); the 1.1.0 row is lost (2 rows) | `n2-pd` 1.1.0 kept, `n2-pa` dev kept; 3 rows | `n2-pd` 1.1.0, `n2-pa` kept; 3 rows |
| upgrade-dep-fails | `n2-pd` stays 1.0.0; `n2-pa` row only (no phases), the failing 1.1.0 row is lost | `n2-pd` 1.0.0; `n2-pa` record kept without class; the 1.1.0 row lost | `n2-pd` **1.1.0 kept although its Activate failed** (row err, committed 1); `n2-pa` record without class |
| two-deps-parent-fails | **1 row**; nothing installed; both dependency rows lost | 3 rows; all kept; parent err committed 1 | 3 rows; all kept |

Observations: `nc-gap-P9-<case>-<mode>` for the 6 cases × 3 modes (18 records).

For the generalized model: F-HIST-1 holds for any number of dependencies, for chains, for upgrades of a pre-installed
dependency, and for a failing dependency's own record. In txn mode the parent's rollback also restores a pre-installed
dependency's old version (atomicity holds beyond the one-dependency, absent-pre-state bound). A failure inside
dependency loading leaves the parent's row with no phases. In dev mode the parent's manifest record survives a failed
dependency without its classes (LOAD-B16).

---

## Mapping table (for tagging)

"Supports" = the observation is evidence the statement holds as written; "refines" = holds, with the stated
precision; "contradicts" = the observation shows a case where it does not hold.

| Observation(s) | Statement / finding / decision | Effect |
|---|---|---|
| nc-gap-P1-*-txn-after (docker, force) | LOAD-B9 | supports (crash by IRIS restart rolls the load back) |
| nc-gap-P1-*-after (all 4) | HIST-B2, HIST-B10, HIST-B21 | supports (row survives; no end time; IRISTEMP empty) |
| nc-gap-P1-docker-dev-after, nc-gap-P1-docker-notxn-after | HIST-B11, HIST-B17 | contradicts (kept module, committed 0; HIST-B17 holds only when `%OnClose` runs, not on an IRIS restart) |
| nc-gap-P1-docker-dev-after, nc-gap-P1-docker-notxn-after | LOAD-B16, LOAD-B18, LOAD-B23 | supports (changes stay across a restart) |
| nc-gap-P1-*-after | HIST-B7 (F-HIST-5) | supports F-HIST-5 (restart rows unnamed) |
| nc-gap-P1-*-after (`sqlUpdateSQLCODE=0`, finalized 0) | HIST-B22, HIST-B29 | refines: a row left by a restart is never locked and stays editable |
| nc-gap-P1-* | F-HIST-2 / RC-2 | new instance → candidate N2-F1; DEC-HIST-2's fix (write the twin at the commit) would also cover it |
| nc-gap-P1-*-during | HIST-B7 | refines: the durable row has no package while the command runs |
| nc-gap-P2-txn-by-package-* | HIST-B25, HIST-B23 | refines: a `-DPackage=` filter cannot match a running command (package empty until close) |
| nc-gap-P2-*-after-release | DEC-HIST-6 | observed fact: option (A) today; the delete wins; a successful load is left with no record |
| nc-gap-P2-*-after-release | HIST-B1 | contradicts (in this event, E6 during H2/H3, the command ends with no record) |
| nc-gap-P2-*-after-release | HIST-B21 | supports |
| nc-gap-P3-* (5) | HIST-B6 | contradicts / refines: a phase failing by an Invoke's returned status (Before or After, any phase) stays open with placeholder `ERROR #00`; candidate N2-F2 |
| nc-gap-P3-n2-ph-after-txn/-dev/-notxn | HIST-B9, HIST-B12, HIST-B14, HIST-B16 | supports |
| nc-gap-P4-*-good-archive | LOAD-D3, LOAD-B2 | supports (unpacked to `<ModuleRoot>/<name>/<version>/`, Root points there) |
| nc-gap-P4-txn-bad-archive-same-version | LOAD-B2 | supports natively (old copy deleted: local file gone; replaced before the failure) |
| nc-gap-P4-txn-bad-archive-same-version | LOAD-D7, HIST-D5, DEC-HIST-5 | observed fact for DEC-HIST-5: Committed 0 after the module root folder was rewritten; installed record's Root folder no longer matches its compiled code |
| nc-gap-P4-txn-bad-archive-same-version | LOAD-B8, LOAD-B14 | supports for database state (record, class rolled back); file state not restored |
| nc-gap-P4-dev-bad-archive-same-version | LOAD-B18, HIST-B14 | supports |
| nc-gap-P5-url-ok-txn, -ok-dev | LOAD-B3 | supports, refines: the clone stays in `mgr/Temp/<ts>/<repo>/` and becomes the module Root; candidate N2-F4 |
| nc-gap-P5-url-* | HIST-B5 | supports (SourceName empty for load, also for URL loads) |
| nc-gap-P5-url-fail-txn | LOAD-B8, HIST-B12 | supports (database); the clone folder stays (file effect, DEC-HIST-5) |
| nc-gap-P5-url-fail-notxn | LOAD-B23, HIST-B16 | supports |
| nc-gap-P5-url-missing | LOAD-B3 | contradicts in part: a failed clone is not reported; candidate N2-F3 |
| nc-gap-P5-url-missing | HIST-B7 / F-HIST-5 | supports F-HIST-5 |
| nc-gap-P6-*-halt-in-step, *-retry | INST-B67, DEC-INST-5 | supports; closes the "kill-between-step-and-record case was not run" note: the step reruns and its effects happen twice (all modes for `$INCREMENT` and file; dev/notxn also for ordinary globals) |
| nc-gap-P6-*-retry | INST-B69 | supports natively, with assertions on version 2.0.0 and every step record (F-INST-3 gap closed for this case) |
| nc-gap-P6-txn-halt-in-step | INST-B65, INST-B66, LOAD-B9 | supports (version, class, step records rolled back) |
| nc-gap-P6-txn-halt-in-step | HIST-B11/B12, RC-1 | supports: update row stays at Init (`update/n2-upd/0.0.0`, unfinalized), like O:ipm-halt-txn |
| nc-gap-P6-dev-halt-in-step, notxn-halt-in-step | INST-B68, INST-B63, HIST-B17, HIST-B10 | supports (kept, committed 1, no end); refines INST-B63: a step cut off before its record save leaves a record with no start/end and Status OK, and reruns because it has no end time |
| nc-gap-P6-dev-* (V1.S1 `start:1`) | INST seeding (Lifecycle/Module.cls#L122) | side fact: developer-mode load does not seed steps |
| nc-gap-P7-force | UNIN-B54, UNIN-B59, UNIN-B60, UNIN-B61, UNIN-B51 | supports |
| nc-gap-P7-force-after-reinstall | UNIN-B76 | supports (records kept, all 5 re-adopted) |
| nc-gap-P7-force-fullcleanup | UNIN-B60 | refines: with -force no question is asked; `-DClean.FullCleanup=1` then does a full cleanup |
| nc-gap-P7-full-cleanup-yes, -force-fullcleanup | UNIN-B55, UNIN-B56, UNIN-B57, UNIN-B58, UNIN-B61, HIST-B30 | supports (asked; others uninstalled first, each with its own row; history and `%SYS` `^IPM.settings` kept; registry definitions deleted; shell exits; IPM's own row finalized) |
| nc-gap-P7-full-cleanup-yes, -force-fullcleanup, *-post-probe, nc-gap-P7-fscache-stale/-repaired | UNIN-D12, UNIN-B58 | contradicts "all IPM data": `^IPM.Repo.Filesystem.CacheI/CacheS` are left and break `repo -fs` after reinstall; candidate N2-F5 |
| nc-gap-P7-full-cleanup-after-reinstall | UNIN-B76 | supports the contrast: after a full cleanup only `zpm` is listed |
| nc-gap-P8-commit-window-after-commit | F-HIST-7, HIST-N3, premise P3 (T7) | confirms natively (with probe patch p8a): committed load, row committed 0, ended 0 |
| nc-gap-P8-commit-window-control | HIST-B8, HIST-B9 | supports (patch inert when off) |
| nc-gap-P8-deployed-export-forced-error | F-RES-1, RES-B25 | confirms F-RES-1 natively; contradicts RES-B25 (packaging succeeds and ships source) |
| nc-gap-P8-deployed-export-control | RES-B22 | supports (the package holds `studio-project/Deployed.xml` and no source) |
| nc-gap-P9-chain-*-txn, two-deps-*-txn, upgrade-*-txn | F-HIST-1, HIST-B19, PD | confirms F-HIST-1 beyond the bound: every dependency row is lost, for 2 dependencies, chains and upgrades |
| nc-gap-P9-*-dev, *-notxn | HIST-B20 | supports (dependency rows whose changes were kept survive) |
| nc-gap-P9-upgrade-parent-fails-txn, upgrade-dep-fails-txn | LOAD-B8 | supports beyond bound T8: a pre-installed dependency is restored to its older version |
| nc-gap-P9-upgrade-ok-* | DEP-B4, LOAD-B13 | supports (pre-installed older dependency upgraded to the exact version) |
| nc-gap-P9-chain-* | DEP-B14, LOAD-B25, DEP-B1 | supports; refines the model bound: transitive dependencies are flattened into one level |
| nc-gap-P9-chain-c-fails-* , upgrade-dep-fails-* | HIST-B6 | refines: a failure while dependencies load leaves the parent's row with no phases |
| nc-gap-P9-chain-c-fails-dev, upgrade-dep-fails-dev | LOAD-B16, LOAD-B17 | supports (record kept, class not compiled, dependency transaction rolled back) |
| nc-gap-P9-*-dev | LOAD-B26 | supports (registry dependencies load in normal mode) |
| nc-gap-P9-*-notxn | LOAD-B23, LOAD-B24 | supports (dependencies also keep their changes, including a failed one) |

## Candidate new findings (IDs to be assigned by the orchestrator)

| Tag | Statement | Evidence | Code |
|---|---|---|---|
| N2-F1 | An IRIS restart during a developer-mode or no-transaction load keeps its changes, but the history row says Committed 0, has no package and stays unlocked | nc-gap-P1-docker-dev-after, nc-gap-P1-docker-notxn-after | RC-2; `AbstractHistory.cls#L120-L125`, `HistoryTemp.cls#L62-L72` |
| N2-F2 | A phase that fails by an Invoke's returned status stays open in history, with placeholder error `#00` | nc-gap-P3-* | `Storage/Module.cls#L610`, `#L671` |
| N2-F3 | A URL load ignores a failed `git clone`; the error names a missing module.xml | nc-gap-P5-url-missing | `Main.cls#L2358`, `Utils/Module.cls#L2409` |
| N2-F4 | URL loads leave their clone in `mgr/Temp` and point the module Root at it | nc-gap-P5-url-* | `Main.cls#L2352-L2359` |
| N2-F5 | Full cleanup leaves `^IPM.Repo.Filesystem.CacheI/CacheS`; after reinstall `repo -fs` fails (#5809) | nc-gap-P7-*-post-probe, nc-gap-P7-fscache-stale/-repaired | `Lifecycle/Module.cls#L41-L43` |

## Not done / limits

- Only IRIS 2026.1. `docker restart` crashed IRIS here (shutdown did not wait); a graceful `iris stop` that waits for
  open transactions was not tried. `iris force` was run for txn mode only.
- P2 covers a delete while the job waits in Activate; a delete during another phase or of a dependency's row was not run.
- P5 used dumb HTTP on localhost in the same container (not a separate server container) and no `-branch`.
- P7 ran in `USER` with no other namespace mapped to it; a full cleanup with mapped namespaces was not run.
- The P8 patches are probes, not fixes. Both checks rely on the control runs to show the patches are inert when off.
