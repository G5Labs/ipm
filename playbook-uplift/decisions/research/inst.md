# Intent research — install/update (03) and dependencies (04)

Baseline: IPM `571e04a1` (v0.10.11-beta.28). Lane: install/update. Researched 2026-11-13.

Scope: Part A decisions DEC-INST-1..8 and DEC-DEP-1..3; Part B items U17 and U22. Nothing in `ontology/`,
`decisions/*.md`, `findings/` or `BUGS.md` was edited; every change below is a proposal.

Sources used for every item: IPM code and comments at 571e04a1, `git log -S` / `git blame`, the 100 commits of PR #887
(fetched read-only with `gh api`), PR #963 and its review threads, CHANGELOG, README, the wiki clone (with its own git
history), GitHub issues/PRs/discussions (`gh`, read only), IPM tests, and two new IRIS probes (below).

Paths: `ipm/...` is the IPM checkout; `wiki:<page>` is the wiki clone
(`.../scratchpad/src-wiki/ipm.wiki/<page>.md`); `EXP/` is the experiment root.

## New IRIS runs (this round)

- Script `EXP/validation/iris/r4/inst/a2-inst-u17-u22.script`, class `EXP/validation/iris/r4/inst/Uplift/A2/Inst.cls`,
  fixtures `EXP/validation/iris/r4/inst/fixtures/` (copied to `/tmp/uplift/r4inst/` in `ipm-uplift-iris-inst`).
- Log `EXP/validation/logs/a2-inst-u17-u22.log` (IRIS 2026.1 build 234U, IPM 571e04a1, namespace USER).
- Observations (`OBS|r4-inst-*` lines in that log): `r4-inst-u17-oras-setup`, `r4-inst-u17-oras-dev-update`,
  `r4-inst-u17-oras-dev-failed-update`, `r4-inst-u17-oras-cleanup`, `r4-inst-u22-dev-kill-before-record-save`,
  `r4-inst-u22-dev-retry`, `r4-inst-u22-nondev-kill-before-record-save`, `r4-inst-u22-nondev-retry`.
- They are **not yet** in `evidence/iris-observations.json`. Every `O:r4-inst-*` / `O!:r4-inst-*` tag proposed below
  needs that record first, or the gate will reject it.
- Clean-up: the run ends with `Reset` (uninstall all, delete registries and step records, delete fixture packages) and
  unpublishes both ORAS images (`r4-inst-u17-oras-cleanup`: tags `<404>`).

---

## DEC-INST-1 — Is the `-dev` bypass of the update-package guard public contract?

- **Verdict**: RESOLVED-EVIDENCE (option A: keep it, and document it).
- **Resolution**: `install -dev` or `load -dev` of a newer version that names an update package goes ahead without
  running the steps. This is intended; README and help only fail to say so.
- **Evidence**:
  - Test doc comment, same PR as the feature: "Verify that running install/load when a module is already installed
    (no matter the version) / succeeds in dev mode and fails otherwise" (`ipm/tests/integration_tests/Test/PM/Integration/InstallModule.cls#L161-L162`),
    with the assert at `#L178`.
  - PR #887 history: the guard carried the comment "Unless, they use the -force flag or are installing in Developer
    Mode" (commits `e4e5b3ef8`, `1f4c09a66`, merged via `3dbbfbfe7`/`4ecaf9f33`; dropped in the squash `f83b207e`, code unchanged).
  - Issue #550 (the feature request): override and do a fresh install "for example in a dev environment".
  - Code: `ipm/src/cls/IPM/Utils/Module.cls#L1219` (`'$get(params("DeveloperMode"),0)`), blame `f83b207e` (PR #887).
  - Gap only: README `ipm/README.md#L62` and wiki:08 "Backwards Compatibility" name only `-force`; the error text at
    `#L1220` mentions only `-force`.
  - IRIS: `nc-inst-2-dev-install-then-update` (existing).
- **Consequence**: code matches. Documentation gap (no F tag): README L62, the `install`/`load` `-dev` help, and the
  guard's error text do not mention the `-dev` bypass. Suggest a doc finding, e.g. **F-INST-11 (doc drift)**: "README
  and help name only -force as a way past the update-package guard; -dev also passes it (TestDevMode)."
  Note the interaction with DEC-INST-2: under its resolution the bypassed steps are seeded, so `-dev` then behaves like
  `-force` for the step records.
  - Ontology: `INST-B30` (03) — remove `OPEN:DEC-INST-1`; keep text "Install -dev also moves to the newer version. It
    does not run the update steps." Evidence: `X:tests/integration_tests/Test/PM/Integration/InstallModule.cls#L178; O:nc-inst-2-dev-install-then-update`
    plus `I`. Move `P:inst_refines`/`M:` to Notes only if DEC-INST-2's change makes the model differ (it does: the
    model has dev installs leaving records unchanged). Notes: "Intended (TestDevMode doc comment L161-L162; PR #887;
    issue #550). README L62 omits it (doc drift)."
  - Cells naming it: `INST-C21` (S4×K3), `INST-C33` (S6×K3), `LOAD-C12` (01, S3×K2, empty outcome). Proposal: clear
    the Decision column of INST-C21/INST-C33; for LOAD-C12 add a new statement and use it as the outcome:
    - `LOAD-B31 | LOAD-B11 | B | A load -dev of a newer version with update steps goes ahead. It does not run the update steps. | I | ipm/src/cls/IPM/Utils/Module.cls#L1219 | Intended: TestDevMode doc comment (InstallModule.cls#L161-L162) covers install and load; only install -dev is asserted (L178). Steps seeded per DEC-INST-2.` (FK grade 3.0)
    - `LOAD-B11` Notes: replace "(DEC-INST-1)" with "(LOAD-B31)".

## DEC-INST-2 — Should an install in developer mode seed update steps?

- **Verdict**: RESOLVED-ASSUMPTION (option A: seed, as non-dev installs do).
- **Resolution**: Any install or load that is not an update seeds the steps of the version it puts in place, in
  developer mode too. A later update then runs only steps that are new since that install.
- **Evidence** (none speaks of dev installs directly):
  - `update` help: "Also runs all of the update steps from the current version to the newer version"
    (`ipm/src/cls/IPM/Main.cls#L167`; same text in wiki:02 `update`). Steps of versions at or below the installed one
    are outside that range; IPM has no version filter (`Storage/Module.cls#L114`), so only seeding keeps them out.
  - IPM's own update tests assert "Seeded update step ... was correctly not rerun on update"
    (`ipm/tests/integration_tests/Test/PM/Integration/Update.cls#L467`), including the `-dev` update runs of
    `NormalUpdateTestSuite` (`#L285-L313`) — but there the install is non-dev.
  - Issue #550: an override is a "fresh install (bypassing upgrade steps)"; a fresh non-dev install seeds
    (`Lifecycle/Module.cls#L122`, test `InstallModule.cls#L214`).
  - The dev condition was added in PR #887 commit `b5cb11a17` ("Incorporate first round of reviews") with no stated
    reason; no review comment discusses it.
  - IRIS: `nc-inst-2b-dev-fresh-then-update` (0 rows, then V1 and V2 run), `nc-inst-2-dev-install-then-update`,
    `nc-inst-fix-stored-dev-plain-load`; this round `r4-inst-u22-dev-kill-before-record-save` again shows `V1.S1`
    run by the first update after `load -dev 1.0.0` (`V1.S1=start:1`).
- **Assumption and alternatives**: Most reasonable because the documented contract of `update` is "from the current
  version", and nothing documents dev mode as a way to replay old steps. Alternatives: (B) keep and document "a dev
  install leaves all steps due" — would be chosen if maintainers want `load -dev` + `update` as a way to exercise every
  step while developing them; (C) seed only steps at or below the installed version — almost the same as A, since
  every step class should be at or below the installed version. If B is chosen, INST-B18 stays as written today.
- **Consequence**: code does not match. **New defect F-INST-8**: "An install or load in developer mode seeds no update
  step (Lifecycle/Module.cls#L122 tests DeveloperMode), so the next update reruns steps of versions already in place."
  - Ontology: `INST-B18` (03) → text "An install in developer mode seeds update steps too. A later update then runs only
    newer steps." (FK 5.8). Evidence: `I; F:F-INST-8; O!:nc-inst-2b-dev-fresh-then-update; O!:nc-inst-2-dev-install-then-update; O!:nc-inst-fix-stored-dev-plain-load`
    (and `O!:r4-inst-u22-dev-kill-before-record-save` once recorded). Move `P:inst_refines` and `M:` to Notes as
    "current behaviour"; witness `inst_witness_dev_install_leaves_steps_due` stays in Notes only.
  - Cells naming it: `INST-C3`, `INST-C15`, `INST-C35`, `INST-C36` → clear the Decision column (outcomes are non-empty).

## DEC-INST-3 — Should install refuse to replace a needed module that is in developer mode?

- **Verdict**: RESOLVED-ASSUMPTION (option A: refuse, naming it), backed by the wiki's general rule.
- **Resolution**: A module in developer mode is not overwritten by a non-dev install, whether it is the named module or
  a needed one. Install should stop and name the needed module.
- **Evidence**:
  - wiki:00.-Concepts "Developer Mode": a module loaded in dev mode "cannot be overwritten by a non-dev install"
    (added by maintainer isc-dchui, wiki commit `cfacdd8` "Improve docs on dev mode", 2026-05-13). It does not limit
    this to the named module. The same page says dev mode is for your own module, not "modules you are consuming as
    dependencies".
  - Code: the guard (`ipm/src/cls/IPM/Utils/Module.cls#L109`) needs `pParams("Install")`, which
    `LoadModuleFromDirectory` removes for dependency loads (`#L317-L319`), and `#L310` resets their DeveloperMode.
    Guard added in 2022 (`d541c5b4`, "deployed modules support"); the `Install` flag was already top-level only then,
    and no commit, test or issue discusses dependencies.
  - IRIS: `nc-inst-8-dev-dependency-replaced` (replaced silently, dev mode lost).
- **Assumption and alternatives**: The wiki rule is general and silent replacement loses the developer's working
  copy link (`^Sources`) without a message, which is the harm the guard exists to stop. It is an assumption because
  the wiki may only have had the named module in mind. Alternatives: (B) allow but warn — chosen if blocking would stop
  routine installs where a dev-mode library is a shared dependency; (C) keep silent — no source supports it.
- **Consequence**: code does not match. **New defect F-INST-9**: "Install silently replaces a needed module that is in
  developer mode and clears its developer mode; the dev-mode guard (Utils/Module.cls#L109) runs only for the named
  module."
  - Ontology: `INST-B35` (03) → text "The rule covers needed modules too. Install will not replace a needed module in
    developer mode. The error names it." (FK 4.1). Evidence: `I; F:F-INST-9; O!:nc-inst-8-dev-dependency-replaced`.
  - Cells: none name DEC-INST-3.
  - Side note (not assigned here, worth a doc finding): the same wiki paragraph says "Uninstall first, or use
    `-force`", but `-force` does not lift the guard (`INST-B33`, `nc-inst-7-dev-mode-guard`).

## DEC-INST-4 — What does `reinstall` fetch?

- **Verdict**: RESOLVED-EVIDENCE (option A: the installed version; fix the description).
- **Resolution**: `reinstall M` installs again the version of M that is installed now. The "latest version" wording is
  stale.
- **Evidence**:
  - Summary "Reinstalls the already installed version" (`ipm/src/cls/IPM/Main.cls#L448`) and both examples
    "Reinstalls the currently-installed version" (`#L455`, wiki:02 `reinstall` example). Only the description
    (`#L450`, copied to wiki:02) says "latest".
  - Code unchanged since the first import of HSPM code (`9713db77`, 2021, `_ZPM/PackageManager.cls` `Reinstall`:
    "Find the currently-installed version"); now `Main.cls#L2694-L2698`.
  - Maintainers describe it as essentially calling `install` (discussion #942, table row `reinstall`; issue #939
    "Decide what to do with reinstall", closed into #942). Nobody proposes "latest"; the open idea is
    uninstall-then-install.
  - IRIS: `nc-inst-9-reinstall-version`.
- **Consequence**: code matches. Doc drift (no F tag): `Main.cls#L450` and wiki:02 `reinstall` description. Fold into
  the proposed doc finding F-INST-11 or a separate one.
  - Ontology: `INST-B39` (03) — remove `OPEN:DEC-INST-4`; Evidence `I; O:nc-inst-9-reinstall-version`; Notes: "Intended
    (summary L448, examples L455; discussion #942). Description L450 and wiki say 'latest': doc drift."
  - Cells naming it: `INST-C10`, `INST-C16`, `INST-C22`, `INST-C28`, `INST-C40` → clear the Decision column.

## DEC-INST-5 — What delivery promise do update steps have?

- **Verdict**: RESOLVED-EVIDENCE (option A: at-least-once; steps must be idempotent).
- **Resolution**: A step may run more than once (after a failed, rolled-back or cut-off update). Module authors must
  write steps that are safe to rerun.
- **Evidence**:
  - wiki:08 (the update guide, written by isc-jili, a PR #887 reviewer, in Sept 2025: wiki commits `26ab143`,
    `7ab6259`): best practice "**Idempotent steps.** Reruns shouldn't cause side effects", and the example step comment
    "Idempotent; throw on error". The same page says rollback support "will be added in future versions".
  - Code comment of the same PR: "idempotent Invokes are still a good way" (`ipm/src/cls/IPM/Utils/Module.cls#L1344`,
    added at reviewer isc-tleavitt's request in PR #887).
  - IRIS: `nc-inst-4-nondev-failed-update`, `nc-inst-4b-nondev-retry`, and this round's U22 (below): in developer mode,
    a kill after the step ended and before its record was saved reruns the step, so even its database write happens
    twice (`r4-inst-u22-dev-retry`: `X=2`, `aRuns=2`, `fileLines=2`).
- **Consequence**: code matches the resolved intent (IPM does at-least-once). The gap is documentation inside IPM:
  `VersionBase` (`ipm/src/cls/IPM/General/Update/VersionBase.cls#L6-L20`) does not say steps must be idempotent; only
  the wiki does. Optional doc note, no F tag.
  - Ontology: `INST-B67` (03) → text "An update step can run more than once. This happens when an update fails or is
    cut off and is run again. So each step must be safe to run again." (FK 2.1). Evidence:
    `I; O:nc-inst-4-nondev-failed-update; O:nc-inst-4b-nondev-retry` (+ `O:r4-inst-u22-dev-kill-before-record-save; O:r4-inst-u22-dev-retry`
    once recorded). Remove `OPEN:DEC-INST-5` and `O!:nc-inst-4b-nondev-retry` (the observation now supports the
    statement). Notes: "Intent: wiki:08 'Idempotent steps'. Witness inst_witness_failed_update_repeats_effects now
    supports the statement."
  - Cells naming it: `INST-C41`, `INST-C42` → clear the Decision column.

## DEC-INST-6 — What should `update` do when the update lock is busy? (High)

- **Verdict**: RESOLVED-ASSUMPTION (option a: fail with a clear error).
- **Resolution**: If the update lock cannot be taken, `update` should report failure saying the lock is busy (and, as a
  failure, not leave the module moved with its steps unrun). It must not report success.
- **Evidence**:
  - The PR #887 authors wrote a test for exactly this, `TestSimultaneousUpdateFails`, asserting the second concurrent
    update fails: `AssertStatusNotOK(..., "Attempting to update ... while another update of that module in the same
    namespace is in-progress failed.")` (added `d6d346728`, 2025-09-18, Emma Neil; removed `e232e5056`,
    2025-09-22, "Removed simultaneous update test"). The PR body then lists "Preventing two updates from being
    completed on the same module in the same namespace at the same time" as out of scope.
  - The earliest lock code in the PR said "Attempt to acquire lock, quit if we cannot" (`8ac85b59d` diff); the
    `return sc` with `sc=$$$OK` (`ipm/src/cls/IPM/Storage/Module.cls#L99-L108`) dates from `1adf42d1e`/`c39dff0a7`.
  - `update` help promises to run the steps (`Main.cls#L167`).
  - No issue discusses it (searched "concurrent update", "simultaneous", "update lock").
  - IRIS: `nc-inst-3-update-lock-busy`, `nc-inst-3b-update-after-lock-free`.
- **Assumption and alternatives**: An assumption because the test was removed, and removal fits either "we no longer
  promise this" or "flaky test". Failure is still the only outcome any source endorses; silent success contradicts the
  help text and leaves due steps with no prompt. Alternatives: (b) wait longer, then fail — same contract, different
  timeout; (c) skip with a warning and a non-OK status — practically (a). Only "keep success" is ruled out.
- **Consequence**: code does not match. Existing **F-INST-1** stands (High).
  - Ontology: `INST-B71` (03) — keep text; Evidence `F:F-INST-1; F:F-INST-6; O!:nc-inst-3-update-lock-busy; O!:nc-inst-model-class-number-collision`
    (remove `OPEN:DEC-INST-6`). Add a child:
    `INST-B81 | INST-B71 | B | If another process holds the update lock, update fails with an error. It says the lock is busy. | F:F-INST-1; O!:nc-inst-3-update-lock-busy | ipm/src/cls/IPM/Storage/Module.cls#L105-L108 | Intent: removed test TestSimultaneousUpdateFails (PR #887 d6d346728, e232e5056).` (FK 4.3)
  - Cells naming it: `INST-C23` → clear the Decision column; add INST-B81 to its outcome.

## DEC-INST-7 — Are primary-only steps promised to run when a mirror member becomes primary?

- **Verdict**: RESOLVED-EVIDENCE (option A: not promised at this revision; fix the VersionBase comment).
- **Resolution**: IPM does not run skipped primary-only steps by itself when a server becomes primary. Mirror support
  is future work.
- **Evidence**:
  - wiki:08: "Logging, rollback, and mirroring support will be added in future versions" (wiki commit `7ab6259`).
  - PR #887 body, out of scope: "Support for mirrored updates".
  - Issue #1257 (open, by isc-jili): a future `-status` modifier, because on mirrors backup and primary "will be in
    different update states" — mirror handling is still being designed.
  - Code: no role-change hook anywhere; `ExecuteUpdateStep` only skips (`ipm/src/cls/IPM/Storage/Module.cls#L228`).
    The promise "this will run only when the instance becomes the primary member" (`VersionBase.cls#L18`) was added in
    PR #887 commit `1adf42d1e` with no implementation.
  - Not checkable on IRIS Community (`nc-mirror-license-refused`).
- **Consequence**: code matches the resolved intent; the VersionBase comment does not. **F-INST-2** should be reworded
  from "claimed but not implemented" to "doc drift + test gap": the comment at `VersionBase.cls#L18` promises a hook that
  is future work; no test covers mirrors.
  - Ontology: `INST-B74` (03) → text "IPM does not run them on its own when that server becomes the primary." (FK 5.0).
    Evidence: `I` (drop `N`, `OPEN:DEC-INST-7`, `F:F-INST-2`). Notes: "Intent: wiki:08 'mirroring support will be added
    in future versions'; PR #887 out of scope. VersionBase.cls#L18 says otherwise (doc drift, F-INST-2)."
  - Cells: none name DEC-INST-7.

## DEC-INST-8 — When `update` brings in a module not installed before, run or seed its steps?

- **Verdict**: RESOLVED-ASSUMPTION (option A: seed, as a fresh install would).
- **Resolution**: A module that an update installs for the first time is a fresh install of that module, so its update
  steps are seeded, not run.
- **Evidence**:
  - `update` help: steps run "from the current version to the newer version" (`Main.cls#L167`); a module with no
    installed version has no "current version".
  - Issue #550: IPM should have "separate commands for upgrading a package vs fresh install"; fresh installs seed
    (`Lifecycle/Module.cls#L122`; test `InstallModule.cls#L214`; `nc-inst-6b-fresh-install-control`).
  - The `update-deps-added` fixture exists for this case, but `TestAllNoErrorModulesNoMirror`
    (`Update.cls#L68-L71`) lists only `UpdateDepsAdded.*` steps and never the added module's steps, so the test is
    silent.
  - Cause in code: `Update=1` passes down to dependency loads (`Utils/Module.cls#L308`, `#L316`), so Activate skips seeding and
    ApplyUpdateSteps runs everything (`Lifecycle/Module.cls#L122`, `Storage/Module.cls#L114`).
  - Related open issue #868 (removing dependencies dropped by an update) shows the maintainers treat dependency changes
    under update as unfinished work; it says nothing about added ones.
  - IRIS: `nc-inst-6-dep-added-by-update`.
- **Assumption and alternatives**: Most reasonable because a step describes a migration from an older version of the
  same module, and there is none here. Alternative (B) run all — fits a case where the new module takes over data that
  an older version of the parent created (resources can move between modules, changelog #1128); then the author would
  need the old steps. If maintainers choose B, INST-B53 stays as written and needs a doc note.
- **Consequence**: code does not match. **New defect F-INST-10**: "update runs every update step of a module it
  installs for the first time (it is not seeded), including steps for versions that never existed in the namespace."
  - Ontology: `INST-B53` (03) → text "A module that update adds for the first time has its update steps seeded. This is
    what a fresh install does." (FK 2.6). Evidence: `I; F:F-INST-10; O!:nc-inst-6-dep-added-by-update`. Move
    `P:inst_fresh_update_runs_all` and `M:` to Notes as current behaviour.
  - Cells: none name DEC-INST-8.

## DEC-DEP-1 — Must resolution find a solution whenever one exists?

- **Verdict**: RESOLVED-EVIDENCE (option B: yes; greedy failure on a solvable graph is a bug).
- **Resolution**: If some choice of versions meets every requirement, install should find one (preferring the highest
  compatible versions), not fail on the first greedy pick.
- **Evidence**:
  - Issue #1299 (open, label `bug`, by maintainer isc-jlechtne): same shape as `nc-dep-11` (the highest ModD needs a
    ModB that clashes); expected behaviour: install the lower version that "is compatible across the entire install
    dependency graph".
  - Issue #1301 (open, isc-kiyer): IPM "can fail an installation even when a compatible graph exists"; user story: try
    the next-highest version when the highest "introduces a transitive conflict".
  - PR #1303 (open, isc-kiyer, 2026-11-03, "Fixes #1299", "Closes #1301"): return all versions "so dependency
    resolution can backtrack from incompatible candidates". Not merged at 571e04a1.
  - Method comment "with backtracking" (`ipm/src/cls/IPM/Storage/Module.cls#L867`, added `4cace764`, PR #975).
  - IRIS: `nc-dep-11-greedy-resolution`, `nc-dep-11b-solvable-with-pin`.
- **Consequence**: code does not match. **New defect F-DEP-7**: "Resolution is greedy and fails on some solvable
  graphs (nc-dep-11; upstream bug #1299, fix PR #1303 open)." Severity per BUGS.md judgement (an install fails that
  should work; workaround: pin first).
  - Ontology: `DEP-B13` (04) — keep text "If some choice of versions meets every requirement, IPM finds one."; Evidence
    `I; F:F-DEP-7; O!:nc-dep-11-greedy-resolution` (remove `OPEN:DEC-DEP-1`). Notes: add "Intent: issue #1299 (bug),
    #1301; fix PR #1303 open."
  - Cells naming it: `DEP-C7`, `DEP-C8` → clear the Decision column; add DEP-B13 to their outcomes if not there.

## DEC-DEP-2 — Should a failed install leave a lock file behind?

- **Verdict**: RESOLVED-EVIDENCE (option A: write the lock file only after the whole install succeeds).
- **Resolution**: The lock file is written after the install has finished well; a failed install writes none.
- **Evidence**:
  - PR #963 review (2025-11-12): reviewer isc-jili asked "if install fails, will this leave a partial/incorrect
    lockfile?"; the author isc-jlechtne answered "No. I put the CreateLockFile check at the end of Install() so if it
    fails then will do so prior to creating the lock file" (review comments 2519413247 / 2519893430, `Main.cls`).
  - Original code: "After install of modules and dependencies is completed, create the lock file" at the end of
    `LoadNewModule`. Commit `49fc412e3` (2025-12-11, "Moving CreateLockFileForModule() call to where dependency graph
    was created") moved it into `LoadDependencies`, before the module's own Activate, to reuse the graph. Nothing
    records a decision to give up the "only on success" property. Squash `a6aadecd` (PR #963).
  - Now: `ipm/src/cls/IPM/Utils/Module.cls#L1415-L1421` (in `LoadDependencies`, called at `#L1328`), before Activate
    (`#L1346`).
  - IRIS: `nc-dep-15-lockfile-failed-install`.
- **Consequence**: code does not match. **New defect F-DEP-8**: "A failed install with -create-lockfile leaves a new
  module-lock.json behind; the file is written before the module's Activate and outside the transaction, contrary to
  the design stated in PR #963."
  - Ontology: `DEP-B40` (04) → text "IPM writes the lock file only after the whole install works. A failed install
    leaves no new lock file." (FK 2.4). Evidence: `I; F:F-DEP-8; O!:nc-dep-15-lockfile-failed-install`.
  - Cells naming it: `DEP-C10` → clear the Decision column.

## DEC-DEP-3 — `-create-lockfile` when a needed module has no registry

- **Verdict**: RESOLVED-ASSUMPTION (option A: write the entry with an empty registry).
- **Resolution**: A needed module that was loaded from a folder appears in the lock file with no registry, the same
  way the base module already does; the lock file is still written.
- **Evidence**:
  - The writer already handles this case for the base module: "Skip undefined repositories as that means the module
    was installed via the zpm "load" command" (`ipm/src/cls/IPM/General/LockFile.cls#L74-L78`). For each needed module
    it calls `AddRepositoryToLockFile` unguarded (`#L72`), which throws "Tried getting repo "" ..." (`#L115`).
  - `load -create-lockfile` is a supported command (`Main.cls#L314`), so lock files for folder-loaded modules are meant
    to exist.
  - Issue #549 (purpose: "reliably reproduce dependencies"); PR #963 review asked whether a lock-file error should fail
    the install (comment 2582727709); unanswered, and the merged code keeps "print and go on" (`Utils/Module.cls#L1417-L1421`).
  - IRIS: `nc-dep-14-lockfile-folder-dependency`.
- **Assumption and alternatives**: Consistency with the base-module rule in the same method is the best guide; the
  missing guard looks like an oversight. Alternatives: (B) fail the command — would be right if maintainers decide a lock
  file must name a registry for every entry (needed for a future `ci`, issue #962); then the base-module rule should
  change too. (C) keep — leaves the user with no file and only a printed error.
- **Consequence**: code does not match. **New defect F-DEP-9**: "With -create-lockfile, a needed module loaded from a
  folder (no registry) makes lock-file creation throw, so no file is written; the base module in the same case is
  written with an empty registry (LockFile.cls#L74-L78 vs #L72)."
  - Ontology: `DEP-B41` (04) → text "Say a needed module came from a folder, with no registry. The lock file still
    lists it, with no registry named." (FK 3.7). Evidence: `I; F:F-DEP-9; O!:nc-dep-14-lockfile-folder-dependency`.
  - Cells naming it: `DEP-C16` → clear the Decision column.

---

# Part B

## U17 — F-INST-5 for remote and ORAS registries

- **Status**: CLOSED for ORAS (observed); remote (HTTP registry) not run, same code path.
- **Run** (`EXP/validation/logs/a2-inst-u17-u22.log`, ORAS zot `http://oras:5000`, namespace `nc-a2-ns17`; only the
  ORAS registry configured):
  - `r4-inst-u17-oras-setup`: two fixture modules published at 1.0.0 and 2.0.0 (tags `1.0.0 2.0.0`).
  - `r4-inst-u17-oras-dev-update`: `install nc-a2-o17 1.0.0` (Repository `nc-a2-o17`), then `update -dev nc-a2-o17
    2.0.0` → OK, `versionAfter=2.0.0`, `devModeAfter=0`. Developer mode was not set.
  - `r4-inst-u17-oras-dev-failed-update`: `update -dev nc-a2-o17f 2.0.0` whose V2.B throws → error,
    `versionAfter=1.0.0`, `V2.A=none` (the step that worked has no record). The update was rolled back, i.e. ran as a
    transactional non-dev update. In developer mode it should have stayed at 2.0.0 with V2.A kept (INST-B68).
  - Same outcome as the filesystem runs `nc-inst-4e-dev-update-from-registry`, `nc-dep-19f-dev-registry-update`.
- **Code** for the remote case: a client without `GetModuleDirectory` (remote and ORAS) goes through
  `LoadModuleFromArchive` (`ipm/src/cls/IPM/Utils/Module.cls#L141-L173`), which calls `LoadModuleFromDirectory` without
  the override (`#L258`, default 0 in the signature at `#L293-L296`); for a load without `Install`, `#L309-L310` sets DeveloperMode to that 0.
  The remote client takes the identical branch, so the outcome follows by inspection.
- **Intent check** (F-INST-5 is a defect): `update -dev` "Sets the DeveloperMode flag for the module's lifecycle"
  (`Main.cls#L184`); test `TestDevModePropagation` asserts the named module is in developer mode after `update -dev
  -path` (`Update.cls#L713-L719`); changelog #1036 only stops propagation to dependencies; wiki:00.-Concepts lists
  `update` among the commands `-dev` applies to. The wiki rule "Read-only repository sources always force
  DeveloperMode=0" does not cover these registries (the ORAS registry here is writable: publish worked).
- **Proposals**: `INST-B52` Notes: replace "by inspection the same for other registries" with "observed for filesystem
  and ORAS registries (r4-inst-u17-oras-dev-update); remote not run, same code path (#L258)"; add
  `O!:r4-inst-u17-oras-dev-update` to INST-B52 and `O!:r4-inst-u17-oras-dev-failed-update` to INST-B68's Notes (or
  Evidence if INST-B68 carries F:F-INST-5). `findings/03-04.md` F-INST-5 "Not run: remote and ORAS registries" →
  "ORAS run (r4-inst-u17-*); remote by inspection". BUGS.md "Remote and ORAS registries for F-INST-5" → remote only.
  QUESTIONS U17 → closed for ORAS.

## U22 — External kill between an update step and its record save

- **Status**: CLOSED for developer mode (the case where the exact point matters); for a transactional (non-dev) update
  the exact point could not be hit by this method, and it does not matter there (see below).
- **Method** (deterministic, no timing race): step `NcA2U22.Upd.V2:A` does its effects (ordinary global `^NcA2U22X`, a
  file line, an IRISTEMP run counter), then reports its pid and waits. The probe takes the lock on that step's record
  (`^IPM.UpdateStep.AnyMemberD(id)`) and releases the step. The step returns; `ExecuteUpdateStep` then blocks in the
  record's `%Save` (`ipm/src/cls/IPM/Storage/Module.cls#L244-L246`) waiting for that lock. The probe checks the state
  and terminates the process (`$system.Process.Terminate`), then retries.
- **Developer mode** (`load -dev 1.0.0`, `update -dev -path 2.0.0` in a job):
  - `r4-inst-u22-dev-kill-before-record-save`: `probeHoldsRecordLock=1`, `beforeKill.processState=LOCKW`,
    `beforeKill.V2.A=start:0/end:0` (record exists from `GetAddUpdateStep`, the run was never saved),
    `terminate=1`, `processGone=1`, `jobEnded=no`. After the kill: `X=1`, `fileLines=1`, `aRuns=1`, version `2.0.0/dev`,
    `V2.A=start:0/end:0`.
  - `r4-inst-u22-dev-retry`: `update -dev -path 2.0.0` → OK; `V2.A=start:1/end:1/ok:1`, `X=2`, `fileLines=2`, `aRuns=2`.
  - So a step that finished but whose record was not saved runs again, and in developer mode even its database write is
    applied twice. This is consistent with the at-least-once intent (DEC-INST-5).
- **Non-dev** (`r4-inst-u22-nondev-*`): the probe could not take the record lock (`probeHoldsRecordLock=0`): inside the
  update's transaction IRIS keeps the record lock from `GetAddUpdateStep`'s save until commit, so `%Save` never waits.
  The step went on, the update finished (`beforeKill.jobEnded=ok`, `terminate=-3`), and the retry ran nothing new
  (`r4-inst-u22-nondev-retry`). For this mode the exact point is not distinguishable: until the final `tcommit`
  (`Utils/Module.cls#L1347-L1348`) a kill rolls back the step's database effects and its record alike, as a halt inside the
  step does (`nc-gap-P6-*-halt-in-step`); file and `$INCREMENT` effects stay. Not run at the exact point.
- **Proposals**: `INST-B67` add `O:r4-inst-u22-dev-kill-before-record-save; O:r4-inst-u22-dev-retry` (after recording);
  its Notes "the kill-between-step-and-record ... not run" → "run in developer mode (r4-inst-u22-dev-*): the step
  reran and its global write happened twice". QUESTIONS U22 → closed for developer mode; non-dev argued (whole
  transaction rolls back).

---

## Proposed new findings (for `findings/03-04.md` / `BUGS.md`; IDs are proposals — check for clashes with other lanes)

| ID | Decision | One line |
|---|---|---|
| F-INST-8 | DEC-INST-2 | An install or load in developer mode seeds no update step, so the next update reruns steps of versions already in place (`Lifecycle/Module.cls#L122`). |
| F-INST-9 | DEC-INST-3 | Install silently replaces a needed module in developer mode and clears its developer mode; the guard (`Utils/Module.cls#L109`) covers only the named module. |
| F-INST-10 | DEC-INST-8 | Update runs every step of a module it installs for the first time instead of seeding them (`Utils/Module.cls#L316`, `Lifecycle/Module.cls#L122`). |
| F-INST-11 (doc drift, no F tag) | DEC-INST-1, DEC-INST-4 | README L62/help omit the `-dev` bypass; `reinstall` description (`Main.cls#L450`, wiki) says "latest" while it reinstalls the installed version. |
| F-DEP-7 | DEC-DEP-1 | Greedy resolution fails on solvable graphs (upstream bug #1299; PR #1303 open). |
| F-DEP-8 | DEC-DEP-2 | A failed install leaves a new lock file; PR #963's stated design was "only after install". |
| F-DEP-9 | DEC-DEP-3 | A needed module with no registry makes lock-file creation throw; the base module in that case is written with an empty registry. |
| F-INST-1 (existing) | DEC-INST-6 | Stands; intent now: fail when the lock is busy. |
| F-INST-2 (existing, reword) | DEC-INST-7 | From "claimed but not implemented" to "VersionBase.cls#L18 promises future work; mirror test gap". |

Side observation (not assigned): wiki:00.-Concepts says `-force` lifts the dev-mode install guard; code does not
(`INST-B33`, `nc-inst-7-dev-mode-guard`).

## Summary

| ID | Verdict | Resolution (one line) | Code matches? |
|---|---|---|---|
| DEC-INST-1 | RESOLVED-EVIDENCE | `-dev` bypass of the update-package guard is intended; document it. | yes (docs lag) |
| DEC-INST-2 | RESOLVED-ASSUMPTION | Dev installs seed update steps like other installs. | no (F-INST-8) |
| DEC-INST-3 | RESOLVED-ASSUMPTION | Install refuses to replace a needed module in dev mode, naming it. | no (F-INST-9) |
| DEC-INST-4 | RESOLVED-EVIDENCE | `reinstall` reinstalls the installed version; fix the description. | yes (doc drift) |
| DEC-INST-5 | RESOLVED-EVIDENCE | At-least-once; steps must be idempotent (wiki:08). | yes |
| DEC-INST-6 | RESOLVED-ASSUMPTION | A busy update lock makes update fail with a clear error. | no (F-INST-1) |
| DEC-INST-7 | RESOLVED-EVIDENCE | No become-primary hook is promised yet; fix VersionBase comment. | yes (comment drift, F-INST-2) |
| DEC-INST-8 | RESOLVED-ASSUMPTION | A module first brought in by update is seeded, like a fresh install. | no (F-INST-10) |
| DEC-DEP-1 | RESOLVED-EVIDENCE | Resolution must find a compatible graph when one exists (#1299 bug). | no (F-DEP-7) |
| DEC-DEP-2 | RESOLVED-EVIDENCE | Lock file only after the whole install succeeds (PR #963). | no (F-DEP-8) |
| DEC-DEP-3 | RESOLVED-ASSUMPTION | List a folder-loaded dependency with an empty registry. | no (F-DEP-9) |
| U17 | closed (ORAS run; remote by inspection) | `update -dev` from ORAS is not in dev mode and rolls back. | no (F-INST-5) |
| U22 | closed for dev mode; non-dev argued | Kill after step, before record save: the step reruns, its global write twice. | yes (at-least-once) |
