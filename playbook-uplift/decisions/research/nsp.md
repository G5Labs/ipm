# Intent research: modules 07 (namespace), 08 (resources), 09 (sync)

Round 4, lane nsp. Baseline IPM `571e04a1` (v0.10.11-beta.28). This file only proposes. It edits no ontology,
decision, finding or BUGS file.

Sources used for every item: code and comments at `571e04a1`, `git log -S` / `git blame` / `git show`, CHANGELOG,
README, the IPM wiki clone (pages named below), GitHub issues and PRs of `intersystems/ipm` (read with `gh`, never
written to), IPM tests, and new runtime probes on `ipm-uplift-iris-nsp`.

Verdict scale: **RESOLVED-EVIDENCE** (an authoritative source settles intent), **RESOLVED-ASSUMPTION** (no such
source; the most reasonable intent is chosen), **STILL-OPEN** (a real policy choice). No item here is STILL-OPEN.

## New runtime probes (round 4)

Scripts: `validation/iris/r4/nsp/` (class `Uplift/R4Nsp/Probe.cls`, copied to `/tmp/uplift/r4nsp/` in the
container). Logs: `validation/logs/a4-nsp-*.log`. Scratch namespaces `NCR4S`, `NCR4T`, `NCR4U`, `NCR4S2`, `NCR4T2`,
`NCR4U2`, `NCR4L`, `NCR4P`. USER's IPM was checked after the risky probe (`r4-nsp-legacy-cleanup`: `userShell=1`,
`userZpm=0.10.11-SNAPSHOT`, `legacyUSER=0`).

| Observation | Log | Facts |
|---|---|---|
| `r4-nsp-collation-slot-v2` | `a4-nsp-collation-v2.log` | `branchOpensUSER=0` (branch opens `/usr/irissys/mgr/USER`; real dir is `.../mgr/user/`); `branchOpensSrc=1`, `branchOpensNCC=1` (dirs made by `CreateNamespace` are upper case). Source DB new-global collation set to 133 while mapping, back to 5 before creating globals. `A_map=db:USER coll:5` (exact IPM call, source USER); `C_map=db:NCR4S2 coll:133` (exact IPM call shape, source NCR4S2); `D_map=db:NCR4S2 coll:5` (control, set-collation 0). Globals created through the mappings: `C_globColl=5`, `D_globColl=5`, `E_globColl=5` (direct). |
| `r4-nsp-collation-run1-residue` | `a4-nsp-collation-v2.log` | `NCR4SNewColl=5`: run 1 left no changed database setting. |
| `r4-nsp-collation-slot` | `a4-nsp-collation.log` | Run 1. Same mapping result (`B_map coll:133`, `D_map coll:5`) but globals were created while the DB default was 133, so it cannot tell mapping from default. The class load reported a compile error in an unrelated helper (`Count`); superseded by v2. |
| `r4-nsp-legacy-setup` | `a4-nsp-legacy-v2.log` | Fake legacy ZPM in NCR4L only: `%ZPM` package mapped to NCR4L's own DB, class `%ZPM.PackageManager.NcR4Probe` compiled there (`compileCls=ok`; `saveCls` error only because run 1 had saved the definition). `legacyNCR4L=1`, `legacyUSERBefore=0`, `legacyUSERAfterSetup=0`, `legacyNCA=0`. One `^ZPM.Dev.ModuleD` row (layout copied from USER's zpm record, renamed `legacy-probe`): `beforeLegacyRows=1`, `beforeIpmRows=0`. |
| `r4-nsp-legacy-enable-map` | `a4-nsp-legacy-v2.log` | From USER, `enable -map -ns NCR4L`: `cmdStatus=ok`, `hasIPMAfter=1`, `maps=pkg:USER rtn:USER`, `migrateLine=0` (no migration output), `afterLegacyRows=1`, `afterIpmRows=0`, `afterProbeMod=-` (module not known to IPM). The `version` list printed by the command shows `NCR4L ... Installed In: USER-VERIFY`, although it was just mapped from USER (F-NSP-7). |
| `r4-nsp-legacy-migrate-direct` | `a4-nsp-legacy-v2.log` | Control: `MigrateZPMToIPM` run by hand in NCR4L prints "Migrating storage for legacy-probe ... done.", then fails on the missing legacy stream class `%ZPM.PackageManager.Developer.Document.ModuleStream` (a limit of the simulation) and rolls back. So the record is reachable by IPM's migration code; `enable` just never calls it. |
| `r4-nsp-legacy-cleanup` | `a4-nsp-legacy-v2.log` | Fake class, `%ZPM` mapping, legacy row and IPM mappings removed from NCR4L; `legacyNCR4L=0`, `legacyUSER=0`, USER's IPM fine. |
| `r4-nsp-unmap-then-install-guard` | `a4-nsp-unmap-route.log` | NCR4P: `enable -map` then `unmap` (both OK). After unmap `maps=pkg:- rtn:USER` (F-NSP-6), `hasIPMAfterUnmap=1`, and `CheckModuleNamespace()` (the guard of `install`/`reinstall`/`uninstall zpm`) in NCR4P returns "Cannot install 'zpm' in namespace 'NCR4P' because it is mapped from namespace 'USER-VERIFY'". Unmap's own output also said "mapped from USER-VERIFY". |

`a4-nsp-legacy.log` is a first attempt that stopped with `<CLASS DOES NOT EXIST>` (probe bug: a helper class not
visible from NCR4L) after mapping `%ZPM` and saving the class definition; it emitted no observation. v2 reran and
cleaned up. These `r4-nsp-*` lines are in the logs only; the orchestrator must collect them into
`evidence/iris-observations.json` before any `O:`/`O!:` tag below is valid.

---

## Part A, module 07

### DEC-NSP-1 — repo mapping when a mapped namespace gets its own IPM

- **Verdict:** RESOLVED-EVIDENCE (option a).
- **Resolution:** Repo settings are mapped only together with IPM code from the same namespace. A namespace that
  gets its own IPM must not keep a repo mapping to the old source; the supported way there is `unmap` (which removes
  both) and then install (see DEC-NSP-2).
- **Evidence:**
  - README#L69 (the decision row cites #L77; the line is #L69 at `571e04a1`): "Repositories are only mapped if %IPM
    classes and routines are also mapped from the same namespace."
  - PR #663 (https://github.com/intersystems/ipm/pull/663), which added repo mapping: "always unmap repository
    settings when IPM package and routines are unmapped". Unmap help, Main.cls#L767: "If repository settings are
    mapped, will also unmap repository settings."
  - Wiki `02.-CLI-commands.md` (unmap) repeats the same rule.
  - Runtime: O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install (repo mapping stranded).
- **Consequence:** code does not match. The installer path keeps the repo mapping and strands it. Existing defect
  **F-NSP-5** (no new finding).
  - No statement carries `OPEN:DEC-NSP-1`. Cell **NSP-C28** (S4, K1) names it.
  - Propose new statement **NSP-B78** (parent NSP-B47): "A namespace with a local install uses its own repo settings.
    None are mapped from elsewhere." (FK 5.2.) Evidence `I; F:F-NSP-5; O!:nc-nsp-N7-installer-in-mapped`. Source
    `ipm/README.md#L69; ipm/src/cls/IPM/Main.cls#L767`. Notes: PR #663.
  - NSP-C28: Outcome `NSP-B49, NSP-B78`; Decision empty.

### DEC-NSP-2 — may IPM be installed locally in a namespace that gets IPM by mapping?

- **Verdict:** RESOLVED-EVIDENCE.
- **Resolution:** Not directly. While a namespace gets IPM by mapping, IPM refuses to install, reinstall or remove
  itself there and points to the source. To give the namespace its own IPM, the operator first runs `unmap`, then
  installs. The installer and a load of IPM should refuse in the same way.
- **Evidence:**
  - PR #485 (https://github.com/intersystems/ipm/pull/485), comment by the implementer listing how issue #470's
    requirements are met: if IPM is mapped from elsewhere, an install "will abort and prompt the user to go the source
    namespace"; different versions per namespace are reached by "adding the `unmap` command".
  - Commits in PR #485: `faf4ebdc` "disallow management of IPM if mapped from another namespace"; `1de4bd0b`
    "implement an "unmap" command to allow for per-NS upgrade of IPM".
  - Issue #470 (https://github.com/intersystems/ipm/issues/470), maintainer requirements (all checked): upgrades
    only through the namespace that manages IPM; different IPM versions in different namespaces.
  - The help sentence "Namespace-specific installation will override this" (Main.cls#L743) was written in
    `9b19985e` for the old `%ALL` design. There, and now, `enable -map -globally` skips namespaces that already have
    IPM ("Skipping IPM mapping of X - IPM already installed", Main.cls#L3637-3641). Wiki `Installation-Patterns.md`
    explains it the same way: it maps "all namespaces which do not already have an ipm installation". So the
    sentence means an existing local install wins over the mapping. It does not mean installing into a mapped
    namespace.
  - Runtime: O:nc-nsp-N7-install-zpm-mapped (refused), O:nc-nsp-N7-installer-in-mapped and
    O:nc-nsp-installer-repoint (installer path: half state). New: O:r4-nsp-unmap-then-install-guard. Even after
    `unmap`, the guard still refuses, because `unmap` leaves the `%IPM.*` routine mapping (F-NSP-6), and it names
    the wrong source (F-NSP-7).
- **Consequence:** `install`/`reinstall`/`uninstall zpm` match. The installer and `load` of IPM do not (no
  `CheckModuleNamespace` guard): existing **F-NSP-5**. Also the supported route (`unmap`, then install) is blocked
  today by **F-NSP-6**. Add this to F-NSP-6's impact, with O!:r4-nsp-unmap-then-install-guard.
  - `OPEN:DEC-NSP-2`: **NSP-B49**. Cell **NSP-C19** names it.
  - NSP-B49 new text: "IPM refuses its installer in a namespace that gets IPM by mapping. So does a load of IPM."
    (FK 3.7.) Evidence `I; F:F-NSP-5; O!:nc-nsp-N7-installer-in-mapped; O!:nc-nsp-installer-repoint` (drop OPEN; the
    M tag checks the model against today's behaviour, so move it to Notes). Notes: PR #485; the help sentence at
    Main.cls#L743 is about namespaces that already have IPM.
  - Propose **NSP-B77** (parent NSP-B47): "To give a mapped namespace a local install, unmap it first. Then install IPM
    there." (FK 3.9.) Evidence `I; F:F-NSP-6; O!:r4-nsp-unmap-then-install-guard`. Source PR #485, commit `1de4bd0b`.
  - NSP-C19: Outcome `NSP-B49`; Decision empty.

### DEC-NSP-3 — legacy ZPM records reached by `enable -map` / `-community`

- **Verdict:** RESOLVED-EVIDENCE (option a).
- **Resolution:** When IPM becomes available in a namespace that still holds legacy (0.7.x) module records, those
  records are moved into IPM's store, as an IPM load does. `enable -map` and `enable -community` should run the same
  migration for each target they map.
- **Evidence:**
  - Issue #470, maintainer's requirement list, checked as done and closed by PR #485: "After enabling of IPM in a
    namespace with 0.7.x (or earlier) installed, need to run the data migration process."
  - PR #485 commits `bf6e6997` "perform migration in all namespaces when upgrading from ZPM to IPM" and `7b10d493`
    "only perform migration in NS with %IPM mapped from current NS". The mechanism is tied to the IPM load:
    `MapIfLegacy` (Installer.cls#L133-139, module.xml#L24) maps every namespace, then `RunAll` (module.xml#L30,
    Migration.cls#L6-35) migrates each namespace mapped from the source.
  - `EnableIPM` never calls `RunAll` or `MigrateZPMToIPM` (Main.cls#L3553-3708).
  - Runtime: O:r4-nsp-legacy-setup, O:r4-nsp-legacy-enable-map, O:r4-nsp-legacy-migrate-direct (simulated legacy, see
    Part B).
- **Consequence:** code does not match for `enable`. **New finding F-NSP-10 (Low):** `enable -map` (and
  `enable -community`) gives a namespace IPM code but does not migrate its legacy ZPM module records; IPM then does
  not list those modules. Reach is narrow: the upgrade path already maps and migrates every namespace it maps. The
  gap shows for a namespace that was not mapped at upgrade time, or whose migration failed (RunAll collects
  per-namespace errors and goes on). Cause: Main.cls#L3611-3708 has no migration call.
  - `OPEN:DEC-NSP-3`: **NSP-B74**. Cells **NSP-C38**, **NSP-C39**, **NSP-C40** name it.
  - NSP-B74 new text: "enable -map moves the legacy ZPM records of a target into IPM." (FK 7.8.) Evidence
    `I; F:F-NSP-10; O!:r4-nsp-legacy-enable-map`. Source `ipm/src/cls/IPM/Main.cls#L3611; ipm/src/cls/IPM/Utils/Migration.cls#L6`;
    Notes: issue #470; simulated legacy (one record, local `%ZPM` class); real ZPM 0.7.4 not loaded.
  - NSP-C38, C39, C40: keep Outcomes; Decision empty.

### DEC-NSP-4 — `enable -community` when one step fails

- **Verdict:** RESOLVED-ASSUMPTION (option a).
- **Resolution:** If a step fails, `enable -community` stops and returns that error, so `zpm "enable -community":1`
  stops a script.
- **Evidence:** No source speaks to failure. The help and wiki call it "functionally equivalent" to running
  `repo -delete-all`, `repo -reset-defaults` and `enable -map -repos -globally` (Main.cls#L741; wiki
  `02.-CLI-commands.md` enable). The code prints a red line and goes on (Main.cls#L3564-3575, from `fa560e07`).
  IPM's other paths report failure: the local-file branch of `enable` collects and throws (Main.cls#L3904-3917);
  the `zpm` language extension honours a quit-on-error argument (`zpm "...":1`; Main.cls#L3512, L3540, commit
  `cc2e4f6c`). Runtime: O:nc-nsp-N4-community-step-fails.
- **Assumption and alternatives:** Most reasonable because the steps build on each other (reset after delete, then
  share the result to every namespace), and scripts can only see a status. Alternatives: (a') run all three and
  return a combined error. That would also satisfy the ontology statement if it says "ends with an error". (b) keep
  today's behaviour; then the help should say the command never fails. Choosing (b) would turn the finding below
  into a documentation item.
- **Consequence:** code does not match. **New finding F-NSP-11 (Low):** `enable -community` returns OK after a
  failed step (BUGS RC-5 already lists "DEC-NSP-4"; design D2).
  - `OPEN:DEC-NSP-4`: **NSP-B33**. No cell names it.
  - NSP-B33 new text: "If one of its steps fails, the command stops and ends with that error." (FK 3.4.) Evidence
    `I; F:F-NSP-11; O!:nc-nsp-N4-community-step-fails`.

### DEC-NSP-5 — developer-mode load refused by a system need keeps its record

- **Verdict:** RESOLVED-ASSUMPTION (option b).
- **Resolution:** A load refused by a system need leaves no module record, also in developer mode. The check
  should run before the developer-mode commit.
- **Evidence:**
  - Issue #106 (https://github.com/intersystems/ipm/issues/106), maintainer spec: "If the current namespace isn't
    %SYS, it should fail." CHANGELOG#L54: the rule is "to prevent installation in non-%SYS namespaces". PR #1226
    shows the expected refusal; its test (SystemRequirements.cls#L21-36) loads without `-dev`.
  - Wiki `03.-IPM-Manifest-(module.xml).md` `<SystemRequirements>`: requirements "must be validated before module
    installation".
  - For keeping the record: the code comment at Utils/Module.cls#L1310 ("save the module manifest so we can fix it
    even if errors occur afterward"), the `-dev` help (Main.cls#L306) and wiki `00.-Concepts.md` (dev mode: the
    manifest "is committed before resource loading begins"). The aim is to inspect and fix partly loaded code.
  - Runtime: O:nc-nsp-N8-sysns-dev.
- **Assumption and alternatives:** The dev-mode trade-off exists to debug a failed load of the module's own
  resources. A system need concerns where the module may load, not its code. A module left listed with developer
  mode on in a namespace it is barred from contradicts "prevent installation". Alternative (a): keep it on purpose;
  then document that `-dev` can leave a refused module installed. (a) would turn the finding below into
  documentation. The ordering dates from before #106 (Utils/Module.cls#L1309-1321), so it was not chosen for the new
  rule.
- **Consequence:** code does not match. **New finding F-NSP-12 (Low):** in developer mode `CheckSystemRequirements`
  (Utils/Module.cls#L1321) runs after the dev-mode `tcommit` (L1311), so a refused module stays installed, listed and
  in developer mode. This holds for every system need, not only SYSNamespace. BUGS RC-6 already lists "DEC-NSP-5".
  - `OPEN:DEC-NSP-5`: **NSP-B66**, **LOAD-B20**.
  - NSP-B66 new text: "A load refused for a system need keeps no module record. That holds in developer mode too."
    (FK 4.4.) Evidence `I; F:F-NSP-12; O!:nc-nsp-N8-sysns-dev`.
  - LOAD-B20 new text: "The system check runs before the developer-mode commit. So a refused module leaves no record."
    (FK 7.0.) Evidence `I; F:F-NSP-12; O!:nc-nsp-N8-sysns-dev`.

### DEC-NSP-6 — which namespace is the source when two `zpm` records share one IPM database

- **Verdict:** RESOLVED-ASSUMPTION (option a).
- **Resolution:** The source of a mapped namespace is the namespace whose own default code database holds the IPM
  code (USER for USER's database). A namespace that reaches that code by mapping is never a source, even if it has a
  `zpm` record.
- **Evidence:** No source covers the tie. Supporting: the `-map` help "map IPM code from the current
  namespace-default code database" (Main.cls#L736); issue #470 wants "the namespace through which IPM is managed"
  tracked; the ontology's own NSP-D4 says a source namespace "has a local install". USER-VERIFY does not: its code
  comes from USER's database. `AddIPMMappedNamespaces` (Main.cls#L1628-1654, from `9b19985e`, no comment on ties)
  takes the last by name. Issue #1310 shows a field report where every mapped namespace says "Installed In: %SYS"
  after mapping from USER. It may be the same cause, but the report gives too little to confirm it.
- **Runtime:** O:nc-nsp-model-mapinfo, O:nc-nsp-model-repos-premapped; new O:r4-nsp-legacy-enable-map (NCR4L, just
  mapped from USER, listed "Installed In: USER-VERIFY") and O:r4-nsp-unmap-then-install-guard (the install guard
  and `unmap` both name USER-VERIFY).
- **Assumption and alternatives:** (a) follows from the definitions above and is the only reading under which
  `enable -map -repos` from USER can work. Alternative: refuse to guess and report the tie as an error. Either way
  "last by name" is not intended.
- **Consequence:** code does not match; existing **F-NSP-7**. Add the two new observations to F-NSP-7, and note
  the wrong name also reaches the `install zpm` guard message and `unmap` output.
  - `OPEN:DEC-NSP-6`: **NSP-B56**.
  - NSP-B56: keep text. Evidence `I; F:F-NSP-7; O!:nc-nsp-model-mapinfo; O!:r4-nsp-legacy-enable-map`.
  - Propose **NSP-B79** (parent NSP-B55): "The source namespace of a mapped namespace is the one whose own code store
    holds its IPM code." (FK 5.9.) Evidence `I; F:F-NSP-7; O!:nc-nsp-model-mapinfo`. Source
    `ipm/src/cls/IPM/Main.cls#L736; ipm/src/cls/IPM/Main.cls#L1628`.

## Part A, module 08

### DEC-RES-1 — namespace switch inside `exec`

- **Verdict:** RESOLVED-EVIDENCE (option a).
- **Resolution:** `exec`/`cos` runs in an isolated context. A namespace switch made inside it ends with the
  command. The help example "set the namespace to MYAPP" is stale.
- **Evidence:** PR #996 (https://github.com/intersystems/ipm/pull/996), fixing issue #995: "introduces **namespace
  isolation** for the `exec` (and `cos`) command". Its test plan: after `exec zn "%SYS"` the shell "remained in (or
  returned to) the `USER` namespace". CHANGELOG#L142: "#996: Ensure COS commands execute in exec under a dedicated,
  isolated context". Code `c606f500` `new $namespace` (Main.cls#L2435). The help example (Main.cls#L515-516, from
  2022 `b43baa63`) and wiki `02.-CLI-commands.md` (exec) predate the fix. Runtime O:nc-res-N16-exec-namespace-v2.
- **Consequence:** code matches. The help and wiki example are stale: existing **F-RES-6** (documentation).
  - `OPEN:DEC-RES-1`: **RES-B73**. RES-B73: keep text; Evidence `I; O:nc-res-N16-exec-namespace-v2`; Source add PR
    #996.
  - RES-B84: keep (`F:F-RES-6; O!:nc-res-N16-exec-namespace-v2`). Notes: replace "Whether isolation is intended is
    DEC-RES-1" with "Isolation is intended (PR #996). The example is stale; the wiki example too."

### DEC-RES-2 — JSON format of a `run-from-file` file

- **Verdict:** RESOLVED-EVIDENCE (option a).
- **Resolution:** A `.json` command file is one object whose `commands` member is an array of command objects.
- **Evidence:** The help's own JSON example (Main.cls#L364-404) and the wiki example (`02.-CLI-commands.md`,
  run-from-file) both use `{"commands": [...]}`. The code has required a root object since the first import
  (`9713db77` "Code from HSPM"; now Main.cls#L2003-2009). The sentence "a single array" (Main.cls#L352, also in the
  wiki) is the outlier. It can be read as naming the `commands` array. No issue or PR asks for a root array. Runtime
  O:nc-res-N17-run-from-file-v2.
- **Consequence:** code matches. The description sentence and the `.text` example are documentation defects:
  existing **F-RES-5**.
  - `OPEN:DEC-RES-2`: **RES-B77**. RES-B77: keep text; Evidence `I; O:nc-res-N17-run-from-file-v2`; Notes: help
    example, wiki example and code agree.
  - RES-B83: keep (`F:F-RES-5`). Notes: "format question DEC-RES-2" becomes "format is the `commands` object".

### DEC-RES-3 — `package` deploys classes in the current namespace, also in developer mode

- **Verdict:** RESOLVED-EVIDENCE (option a).
- **Resolution:** Packaging a module with deployed resources happens in the current namespace whether or not it is
  in developer mode, so those classes become deployed there. The developer reloads from source afterwards.
- **Evidence:** Issue #815 (https://github.com/intersystems/ipm/issues/815), maintainer comment after internal
  discussion. Packaging and publishing of deployed code is to "happen in the current namespace regardless of
  whether it is in dev mode or not". They weighed a separate namespace and chose this, accepting that it "would
  require a load from source every time". Fixed by PR #898 (`ccc8aacf`, "Packaging/publishing of module with
  deployed code will always occur in current namespace"), which removed `%PrepareDeploy`. The message at
  Base.cls#L1837 (from `5bbcf80c`, 2022) agrees. Runtime O:nc-res-N12b-package-dev.
- **Consequence:** code matches. (F-RES-3, plain `makedeployed` in developer mode, is a separate question. #815
  covers packaging only.)
  - `OPEN:DEC-RES-3`: **RES-B23**. RES-B23: keep text; Evidence `I; O:nc-res-N12b-package-dev`; Source add issue #815,
    PR #898.
  - Cell **RES-C18**: keep Outcome; Decision empty.

## Part A, module 09

### DEC-SYNC-1 — failed load or compile outside `sync`

- **Verdict:** RESOLVED-ASSUMPTION (option a).
- **Resolution:** When `compile`, `load -dev` or `reload` puts edited source on the server and then fails, the
  files it loaded are re-detected by the next `sync`, even after an exact revert. The rule `sync` keeps is "nothing
  to sync means the server matches the disk".
- **Evidence:** Issue #1224 (https://github.com/intersystems/ipm/issues/1224, "Sync doesn't detect and reload on
  revert of failed code") expects a revert to be reloaded. Its fix (PR #1223) covers failures inside `sync` only. The
  wiki (`02.-CLI-commands.md`, sync) scopes the retry rule to sync ("If a sync fails partway, the files ... are
  flagged and retried ... even if you revert them"). Wiki and help (Main.cls#L109-111) describe sync as loading
  "files changed on disk since the last load or sync". PR #1205 states the purpose: agents edit files, then run sync
  to get them "running in IRIS". Nothing addresses a failed `compile`. Runtime O:nc-sync-N18-compile-fail-revert,
  O:nc-nsp-model-sync-reload-revert, O:nc-nsp-model-sync-skipsync-revert.
- **Assumption and alternatives:** (a) extends the maintainers' own #1224 fix to the same symptom reached by another
  command. Without it, `sync` reports "Nothing to sync" over broken server code, which defeats its stated purpose.
  Alternative (b): retry marks stay sync-only and the docs say "after a failed compile, run compile again". (b)
  would turn F-SYNC-1 into a documentation item.
- **Consequence:** code does not match; existing **F-SYNC-1** (design D10).
  - `OPEN:DEC-SYNC-1`: **SYNC-B22**. Cells **SYNC-C36**, **C37**, **C38**, **C39**, **C42** name it.
  - SYNC-B22 new text: "A load or compile outside sync that fails marks the files it put on the server. The next sync
    loads them again." (FK 2.6.) Evidence `I; F:F-SYNC-1; O!:nc-sync-N18-compile-fail-revert`. Move
    `P:sync_lemma_compile_effects` and the `M:` tag to Notes: they prove today's behaviour, and the review rule keeps
    P and F apart.
  - Cells: SYNC-C36, C37, C38 Outcome `SYNC-B22, SYNC-B18`; SYNC-C39 Outcome `SYNC-B22, SYNC-B43, SYNC-B18`;
    SYNC-C42 keep Outcome `SYNC-B49, SYNC-B50`; Decision empty in all five.

### DEC-SYNC-2 — first `sync` with no baseline (after `load -dev -skip-sync`)

- **Verdict:** RESOLVED-EVIDENCE (option b).
- **Resolution:** With no baseline, the first `sync` only records the disk as the baseline and loads nothing. This
  holds after `-skip-sync` too. Edits made before that first sync are not loaded.
- **Evidence:**
  - `-skip-sync` help (Main.cls#L315): "skips recording the file hashes ... The next sync of the module records them
    instead". This says records, not loads. CHANGELOG#L12 says the same.
  - The test added with the flag in PR #1291 (Sync.cls#L72-91), doc comment: "load -skip-sync stamps no baseline.
    The next sync establishes one and later syncs detect edits". It asserts "Baseline established" on the first sync.
  - PR #1205 description: "A pre-feature module self-heals its baseline on first sync ... only the *next* sync after
    a real edit detects changes". Test Sync.cls#L126-133 asserts the absorbed edit. The console says "Baseline
    established. Run sync again to detect changes." (Pipeline.cls#L43-48). Wiki (sync, Limitations) documents the
    same for pre-0.10.10 modules.
  - Issue #1290: the flag exists so HealthShare CI can skip baselining. PR #1291 says "Running `sync` manually will
    sync the files". That is loose wording; the test it added shows what was meant.
  - Runtime O:nc-sync-N19-skip-sync-first-sync.
- **Consequence:** code matches. What remains is documentation: the general sync help (Main.cls#L109-111, "changes
  since the last load/sync") and the wiki do not warn that a `-skip-sync` load is not a baseline. Propose
  re-classifying **F-SYNC-2** as a documentation defect: the help should say edits made before the first sync after
  a `-skip-sync` load are not loaded, as the wiki already says for pre-0.10.10 modules.
  - No `OPEN:DEC-SYNC-2` statement. Cells **SYNC-C29**, **C30**, **C31**, **C32** name it: keep Outcomes
    (`SYNC-B40, SYNC-B41`); Decision empty.
  - SYNC-B15: drop `F:F-SYNC-2` and `M:sync-explore-witness-F-SYNC-2`; Notes: "Named exception: SYNC-B40, SYNC-B41
    (no baseline)." Keep `F:F-SYNC-1` and the rest.
  - SYNC-B40/B41: Notes add "Intended also after `-skip-sync` (PR #1291 test Sync.cls#L72)."
  - Propose **SYNC-B52** (parent SYNC-B40): "The help for sync says that the first sync after a -skip-sync load loads
    nothing." (FK 4.4.) Evidence `I; F:F-SYNC-2`. Source `ipm/src/cls/IPM/Main.cls#L109; ipm/src/cls/IPM/Main.cls#L315`.

### DEC-SYNC-3 — `sync` with no module name when one module fails

- **Verdict:** RESOLVED-EVIDENCE (option b).
- **Resolution:** `sync` with no name syncs every developer-mode module, shows each failure and a summary naming the
  failed modules, and ends without an error status. `sync <module>` returns the error.
- **Evidence:** PR #1205 (https://github.com/intersystems/ipm/pull/1205), the design description: "With no module
  name, `Main.Sync` loops over all `DeveloperMode=1` modules and collects failures without throwing." Code from that
  PR, `5af3beda` (Main.cls#L2383-2410). Runtime O:nc-sync-N20-sync-all-one-fails, O:nc-nsp-fix-sync-N20-summary.
- **Consequence:** code matches. The help (Main.cls#L109-118) and wiki do not say that the no-name form never fails.
  A one-line help note would help (not a defect). BUGS RC-5 lists DEC-SYNC-3 under dropped statuses; with this
  verdict, remove it there.
  - `OPEN:DEC-SYNC-3`: **SYNC-B46**. Keep text; Evidence `I; O:nc-sync-N20-sync-all-one-fails;
    O:nc-nsp-fix-sync-N20-summary; M:nsp-conf-nc-sync-N20-sync-all-one-fails`; Source add PR #1205.

### DEC-SYNC-4 — `reload` of a module not in developer mode

- **Verdict:** RESOLVED-ASSUMPTION (a variant of option a: warn).
- **Resolution:** The reload phase stays a no-op for a module with no root directory when it runs inside a larger
  phase list. The `reload` command run alone should tell the operator that nothing was loaded and why (not in
  developer mode, no root), instead of a silent "SUCCESS".
- **Evidence:** The "For now, no-op if no root is configured" comment comes from the original 2019 import (`2aaa8cca`;
  current Base.cls#L633-634). Help (Main.cls#L42, 2022) says reload "pulls module source code into the namespace from
  disk". The root is only read in developer mode (Base.cls#L627-629). The sibling command `sync` refuses the same
  case with an error: "Module '%1' is not in development mode. Sync requires dev mode." (Pipeline.cls#L29-31). No issue or PR covers it. Runtime
  O:nc-sync-N21-reload-non-dev.
- **Assumption and alternatives:** The reload phase also runs inside `compile` and other phase lists (SYNC-B47:
  Initialize, Reload, Validate, Compile). An error there would break `compile` of every installed non-dev module, so
  the phase must stay quiet. The standalone command is the case that misleads. Alternative: fail the standalone
  command, as `sync` does for a non-dev module. That is stricter for scripts, and the statement text below fits both.
- **Consequence:** code does not match. **New finding F-SYNC-3 (Low):** `reload <module>` of a module not in
  developer mode loads nothing and prints "Reload START ... SUCCESS" with no notice (Base.cls#L633-634).
  - `OPEN:DEC-SYNC-4`: **SYNC-B51**.
  - SYNC-B51 new text: "For a module not in developer mode, reload loads nothing. It tells the operator so." (FK 6.2.)
    Evidence `I; F:F-SYNC-3; O!:nc-sync-N21-reload-non-dev` (move the `M:` tag to Notes).

---

## Part B

### DEC-NSP-3 native run (legacy ZPM records reached by `enable -map`)

**Closed by simulation. Real ZPM 0.7.4 not loaded.** Loading ZPM 0.7.4 (as in `validation/native-gaps-n1.md` G3)
installs `%ZPM` classes for the whole server and replaces the `zpm` language extension (G3 log
`96-migration-ci.log`: "Imported class: %ZPM.Installer", `%ZPM/...` loaded). In `ipm-uplift-iris-nsp` that would
make `HasLegacyZPMPackage` true in USER, so USER's next IPM load would run `enable -globally -map`. It would also
change USER's `zpm` command. That breaks the "keep USER's IPM working" rule, so instead:

- Legacy was simulated in NCR4L only. A namespace-local `%ZPM` package mapping to NCR4L's own database made the
  legacy test (`$order(^oddCOM("%ZPM.PackageManager."))`, Migration.cls#L38-42) true there and false in USER and NCA
  (O:r4-nsp-legacy-setup). One `^ZPM.Dev.ModuleD` row was written in IPM's compatible storage layout.
- `enable -map -ns NCR4L` from USER mapped the code, printed no migration line, and left the record unmigrated
  (O:r4-nsp-legacy-enable-map).
- Calling `MigrateZPMToIPM` by hand in NCR4L found and copied the record, then rolled back because the simulation has
  no legacy stream class (O:r4-nsp-legacy-migrate-direct). This shows the record is reachable; `enable` just does not
  try.
- Cleanup restored NCR4L and confirmed USER (O:r4-nsp-legacy-cleanup).

Limit: real ZPM 0.7 also keeps registry rows in `^%ZPM.Client.ServerDefD` and document streams. The simulation has
neither, so it says nothing about those parts of the migration. The code path that matters (`EnableIPM` has no
migration call) does not depend on them.

### U24 — does `AddGlobalMapping` with the skip flag in the set-collation slot change collation?

**Closed. No collation change for globals; only the stored mapping attribute can change.** O:r4-nsp-collation-slot-v2:

- With source USER, as `enable -map -repos` from USER does, the branch looks for `/usr/irissys/mgr/USER`. USER's
  directory is `.../mgr/user/`, so on this Linux container the branch finds nothing (`branchOpensUSER=0`). The
  mapping is the same as the control (`coll:5`).
- When the source database lives at `mgr/<NAME>` (as for databases made by IPM's `CreateNamespace`, e.g. NCC), the
  branch copies that database's new-global collation into the mapping (`C_map coll:133` vs control `coll:5`). With
  the IRIS default (5) the copied value equals the default.
- A global created through that mapping got the database's own default at creation time (`C_globColl=5`), the same
  as the control. So the mapping's Collation attribute did not change a real global's collation here.

Not tested: a case-insensitive file system (Windows), where USER's lookup would succeed. By the result above, that
would only record 5 (USER's collation). F-NSP-8 stays Low: the only effects are the extra activations and a
`Collation` value written to the CPF mapping. Proposed: add to F-NSP-8 "Collation side effect checked: none on
globals (O:r4-nsp-collation-slot-v2)". Remove U24 from QUESTIONS Part B and the BUGS.md#L244 line. NSP-B28 Notes:
add the observation; tags unchanged.

---

## Proposed new findings (for `findings/07-09.md`; not written there)

| ID | Severity | Statement | Defect |
|---|---|---|---|
| F-NSP-10 | Low | NSP-B74 | `enable -map` / `-community` never migrates legacy ZPM records of a target (Main.cls#L3611-3708 has no `RunAll`/`MigrateZPMToIPM` call); intent issue #470. O!:r4-nsp-legacy-enable-map. |
| F-NSP-11 | Low | NSP-B33 | `enable -community` prints a failed step and returns OK (Main.cls#L3564-3575). O!:nc-nsp-N4-community-step-fails. |
| F-NSP-12 | Low | NSP-B66, LOAD-B20 | In developer mode the system-need check (Utils/Module.cls#L1321) runs after the manifest commit (L1311), so a refused module stays installed. O!:nc-nsp-N8-sysns-dev. |
| F-SYNC-3 | Low | SYNC-B51 | `reload <module>` of a non-dev module silently loads nothing and reports SUCCESS (Base.cls#L633-634). O!:nc-sync-N21-reload-non-dev. |

Changes to existing findings: F-NSP-6 impact gains "the supported unmap-then-install route is blocked"
(O!:r4-nsp-unmap-then-install-guard). F-NSP-7 gains O!:r4-nsp-legacy-enable-map and the wrong name in the install
guard and unmap messages. F-SYNC-2 becomes a documentation defect (DEC-SYNC-2). F-NSP-8: collation checked, none.

## Summary

| ID | Verdict | Resolution | Code matches? |
|---|---|---|---|
| DEC-NSP-1 | RESOLVED-EVIDENCE | Repo mapping goes with the code mapping; a namespace with its own IPM keeps none (README#L69, PR #663). | no (F-NSP-5) |
| DEC-NSP-2 | RESOLVED-EVIDENCE | No direct local install while mapped; unmap first, then install (PR #485). Installer/load should refuse too. | no (F-NSP-5; route blocked by F-NSP-6) |
| DEC-NSP-3 | RESOLVED-EVIDENCE | Enabling IPM where 0.7.x records exist must migrate them (issue #470). | no (new F-NSP-10) |
| DEC-NSP-4 | RESOLVED-ASSUMPTION | `enable -community` stops and returns the error of a failed step. | no (new F-NSP-11) |
| DEC-NSP-5 | RESOLVED-ASSUMPTION | A load refused by a system need leaves no record, also with `-dev` (issue #106 spec, wiki). | no (new F-NSP-12) |
| DEC-NSP-6 | RESOLVED-ASSUMPTION | The source is the namespace whose own code DB holds IPM; never "last by name". | no (F-NSP-7) |
| DEC-RES-1 | RESOLVED-EVIDENCE | `exec` is isolated; a namespace switch ends with it (PR #996). Help/wiki example stale. | yes (doc F-RES-6) |
| DEC-RES-2 | RESOLVED-EVIDENCE | `.json` file = object with a `commands` array (help and wiki examples, code since first import). | yes (doc F-RES-5) |
| DEC-RES-3 | RESOLVED-EVIDENCE | Packaging deploys in the current namespace even in dev mode (issue #815, PR #898). | yes |
| DEC-SYNC-1 | RESOLVED-ASSUMPTION | Files a failed compile/load/reload put on the server are re-detected by the next sync. | no (F-SYNC-1) |
| DEC-SYNC-2 | RESOLVED-EVIDENCE | First sync with no baseline only records it, also after `-skip-sync` (help, PR #1291 test, PR #1205). | yes (F-SYNC-2 becomes doc) |
| DEC-SYNC-3 | RESOLVED-EVIDENCE | `sync` with no name collects failures and returns OK (PR #1205). | yes |
| DEC-SYNC-4 | RESOLVED-ASSUMPTION | Standalone `reload` of a non-dev module tells the operator nothing was loaded. | no (new F-SYNC-3) |
| Part B DEC-NSP-3 | closed (simulated) | `enable -map` leaves a legacy record unmigrated; real ZPM 0.7.4 not loaded (would touch USER). | — |
| Part B U24 | closed | Skip flag in the collation slot changes no global's collation; only the CPF mapping attribute, and only for `mgr/<NAME>` databases. | — |
