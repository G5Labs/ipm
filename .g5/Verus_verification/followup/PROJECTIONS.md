# Projections and what survives them (R07)

Every model abstracts. This inventory lists each material projection in the original Verus model (`crates/ipm-load-history`) and the follow-up models (`followup/models/`). For each it states the exact property that survives the projection, the evidence that now strengthens or limits it, and its status:

- discharged: independent source or native evidence establishes what the projection assumes;
- narrowed: the claim was cut down to what the evidence supports;
- conditional: the claim depends on a named premise that stays open;
- open: not established.

Paths are relative to `.g5/Verus_verification/`. `FR` is `followup/results/`.

## Module state

| Projection | What the model keeps | Property that survives | Evidence added | Status |
|---|---|---|---|---|
| Resources folded to one flag `res` | "the module's class `<Pkg>.Main` is compiled" | A clause that needs resources (kind 1 "installed", kind 6) holds for a module whose fixture has one class. It does not show that every resource of a real module (several classes, routines, files, `LoadedResource` rows) is present or absent together | Native fixtures have one class each (`native/fixtures/replay`, `followup/native/fixtures/r05`). P-10 covers class definitions and compiled classes only. Corrected evaluator now actually checks `res` (R01a) | narrowed: "the module's classes" for single-class fixtures. Non-class resources and `LoadedResource` (OBS-L15) open |
| Version as ordinal 1–3 | order and equality of versions | downgrade, update-required and dependency-satisfaction decisions for distinct major versions | Snapshot suffix handled explicitly (`followup/replay/PROJECTION.md`). Semver ranges, prereleases and build metadata are not modeled. Native N-R05-HIGHER and N-R05-HIGHEROK show an exact requirement and a range against an installed higher version | narrowed |
| Module record writes as a counter `wr` | "the record was written" | `kept` (Committed exactness) and `unchanged` see rewrites with equal values | `%OnBeforeSave` stamps `LastUpdated` on every save, one-second resolution (`src/cls/IPM/Storage/Module.cls:1879-1882`). The driver waits 1.1 s (`native/scripts/B2Native/Replay.cls`). The corrected evaluator uses the proxy for both `kept` and `unchanged` and marks it unknown when unobserved (R01c, R01e) | conditional on the proxy: a write that leaves the same `LastUpdated` second is invisible. The 1.1 s wait rules this out for a write by the command |
| Stored `.ZPM` document as its version `doc` | the `<Version>` of the stored document | manifest-only states (kind 2) and NoJournal document survival (B1-C10) | Driver reads the `<Version>` element (`Replay.cls` `DocVersion`) | discharged for the version. Other document contents not compared |
| Three module slots (top, two dependencies) | at most two dependencies, leaves | dependency clauses for up to two dependencies, nested or shared | Follow-up reduction: the top's `LoadDependencies` loads the transitive closure in order with its own params, and nested calls skip installed dependencies (`src/cls/IPM/Storage/Module.cls:1139-1154`, `src/cls/IPM/Utils/Module.cls:1368-1405`). Native N-R05-NESTS, NESTF and DIA confirm order, single load and params | discharged for depth-2 chains and diamonds of up to two loaded dependencies. Larger graphs: by the same reduction, not checked |
| One global transaction mode | all activations share the mode | in the original model, every theorem | Follow-up `models/loadtx.py` resolves the mode per activation from caller flags and both manifests' `<Defaults>` (`Utils/Module.cls:1165-1172`). Native N-R05-PARDEF, Q23F/S, NESTF and archived M5. With per-activation modes the as-built still meets PREFIX and TLEVEL. COMMITTED (30) and repair 103 (818) fail only when a dependency declares NoJournal | narrowed: the original theorems hold for loads whose modules declare no `<Defaults>` transaction flags. The general case is in the follow-up model |
| Installed state before the load: absent, or installed at a version, developer mode or not | top module's prior state; dependencies absent or at the required version | refusal and dependency-skip decisions | Native N-R05-DEVUP (a developer-mode dependency at another version is replaced and its developer mode cleared), DEVSAME, HIGHER, HIGHEROK | narrowed: dependency installed at another version is native-only, not modeled |

## History

| Projection | What the model keeps | Property that survives | Evidence added | Status |
|---|---|---|---|---|
| Phase list as a count and the last phase's status | `ph`, `phst` | "five phases started; the last ended OK/ERR/open" | Driver keeps `x.last_phase_name`. Phase order and per-phase timestamps are not compared | narrowed: phase order and names not established (D10, D6 still visible through the count and last status) |
| Row fields as booleans | `pkg`, `tend`, `fin`, `com`, `ok` in three classes | existence, finalization, end time, package presence, Committed, success class | `Action`, `SourceName`, `CommandString`, `UserName`, `TimeStart` values not compared (Q-15 concerns the source fields) | narrowed |
| IRISTEMP copy as one row per action | `temp` | orphaned temp rows after SIGKILL are visible | P-02 instance-wide IRISTEMP. Concurrent use of the shared temp global by other processes (OBS-H13) is not modeled | conditional on a single loading process |
| Readers: one foreground action plus foreign edits of finalized rows | `theorem_r2` covers edits | finalized rows immutable under edits | Follow-up `models/history_readers.py` adds find, details, delete, global readers and purge as separate commands over distinguishable rows and namespaces (R04). Native N-R04-* | discharged for the reader commands within the model's bounds. Readers running concurrently with a load: open |
| Record identity by slot | one row per action, matched by package | which action a row belongs to | The driver reports only the first row per slot. The corrected evaluator now treats a second row as an evidence problem (`history.S.x.rows`). No saved replay had one | discharged for the saved corpus |

## Processes, cleanup and time

| Projection | What the model keeps | Property that survives | Evidence added | Status |
|---|---|---|---|---|
| Process end collapsed to "rolled back" | original: kill and terminate end states after rollback | the settled state | Follow-up `loadtx.py` separates transient (before IRIS's rollback), settled, returned, caller commit, caller rollback and retry. Transient differs from settled in 5,328 end states. Native SIGKILL cleanup took 105 s to about 10 min (P-05, P-13), and other processes read uncommitted data in that window | discharged in the model. The transient window has no contract clause (no source states what another process may see) |
| Process-end namespace for non-transactional and developer-mode loads | original: `any_alt` or broad alternatives (`contract.rs:184-195`) | nothing (any state accepted) | Follow-up PREFIX clause: exactly the writes whose durability point came before the end. As-built meets it at every cut point (`FR/20261114T002647Z-final-loadtx-model`). Three impossible states that the original accepted are rejected (`FR/20261114T002830Z-final-prefix-cases`) | discharged in the model. Native: the two approximate C49 cases stay APPROXIMATE, and no new process end was run natively (destructive) |
| Rollback semantics | original: nested levels, unjournaled writes survive (P-01, P-03) | correct for the as-built write orders | New P-14 (native N-P21A/N-P21R): a journaled write is undone only if the node still holds its value. As-built is insensitive: the M-BEFORE-IMAGE mutant gives identical results, because as built no unjournaled write follows a journaled one on the same node before a rollback. Repair 103 is sensitive: a Terminate between its journaled mark and the commit leaves `Committed=1` | conditional on P-14, which is inferred from four observations, not from IRIS documentation |
| Restart | not modeled | none | P-12 (IRISTEMP emptied at restart) not run; it needs an instance restart | open |
| Concurrency | one loading process | single-process guarantees only | Concurrent loads (Q-16, OBS-H13) not run | open |
| Several operations | one load, a retry, or a caller transaction | retry from every settled end converges | `loadtx.py` RETRY: 0 failures. Sequences of different commands (update after a failed load, uninstall) not modeled | narrowed |

## Effects outside the database

| Projection | Status |
|---|---|
| Files written into the module root, `FileCopy` targets, pip installs, CPF merges, git clone folders | open (Q-04). No model or native check covers them. The original handoff says the same |
| Registry effects (publish, ORAS, remote repositories as dependency sources) | open. Native dependency sources were filesystem repositories only. The follow-up container did not run ORAS |
| Temporary repository registration (`ipm-temp-modules-N`) | observed: left registered after a process end (I2, archived `results/native/20261113T210229Z-temp-repos-leftover`). Not modeled |
| Global progress (a load eventually finishes) | open. The models check safety at end points, not liveness |

## Replay evaluator

The corrected evaluator's own projection (each observed field, its producer and its observability) is in `followup/replay/PROJECTION.md`.
