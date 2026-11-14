# Open questions for the IPM maintainers

Baseline: IPM `571e04a1`. Rewritten 2026-11-13 (round 4).

All 44 intent decisions of the earlier version were researched in IPM's code and comments, git history, the IPM wiki,
GitHub issues and PRs, IPM's CI at this commit, and new IRIS runs (`decisions/research/*.md`). **None is left open.**
24 were settled by an authoritative source (a maintainer statement, the wiki, the changelog, a test, a commit or PR
message); 20 were settled by a stated assumption, chosen as the most reasonable reading, and the work proceeded on it.
The full record of each is in the "Resolutions" section of `decisions/*.md`.

What remains here:
- **Part A. Assumptions to confirm.** The 20 resolved-by-assumption intents, each with its alternatives and what
  would change in `BUGS.md` if a maintainer chose otherwise. They are resolved for this run; they are listed so a
  maintainer can overrule them cheaply.
- **Part B. Unresolved verification items.** What this run could not establish, and what its claims rest on.

---

# Part A. Assumptions to confirm (resolved, not open)

Most worth a maintainer's look first, because a different choice adds or removes a Medium-or-higher defect:
DEC-INST-6, DEC-INST-2, DEC-INST-8, DEC-INST-3, DEC-REPO-3, DEC-REPO-5, DEC-SYNC-1, DEC-NSP-6.

| ID | Assumed intent | Why this reading | Alternatives, and what changes if chosen |
|---|---|---|---|
| DEC-INST-6 | If the update lock is busy, `update` fails with an error that says so; it never reports success. | The PR #887 authors wrote `TestSimultaneousUpdateFails` asserting exactly this (commit `d6d346728`), removed it four days later (`e232e5056`) and put concurrency out of scope; the update help promises the steps run. | Wait longer, then fail: same contract. Only "keep success" is ruled out. If it were chosen, F-INST-1 (High) would become a documentation defect. |
| DEC-INST-2 | A developer-mode install or load seeds update steps like any install; a later update runs only newer steps. | Update help: steps run "from the current version to the newer version"; `Update.cls#L467` asserts seeded steps do not rerun. | Keep "a dev install leaves all steps due" (so `load -dev` + update exercises every step): F-INST-8 withdrawn. |
| DEC-INST-8 | A module that `update` installs for the first time has its steps seeded, as on a fresh install. | Update help's "from the current version"; issue #550 separates updates from fresh installs. | Run them all (if the new module adopts data an older parent created, changelog #1128): F-INST-10 withdrawn. |
| DEC-INST-3 | Install refuses to replace a needed module that is in developer mode, and names it. | Wiki Concepts: a dev-mode module "cannot be overwritten by a non-dev install". | Allow with a warning: F-INST-9 narrows to the missing message. |
| DEC-REPO-3 | Changing an ORAS registry's address clears its stored credentials, as for a remote registry. | Standard practice (credentials belong to a host); the remote registry already does it (`Remote/Definition.cls#L81-L86`). | Keep and warn (operator only renamed the host): F-REPO-11 becomes a missing warning. |
| DEC-REPO-5 | ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version. | Today a deployed version cannot be unpublished by version at all, which no source intends. | Require the platform to be named; the plain form must still not fail as "not found". |
| DEC-SYNC-1 | Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`. | Sync help "since the last load/sync"; issue #1224's expected behaviour for the same symptom inside sync. | Retry marks stay sync-only and the docs say "after a failed compile, run compile again": F-SYNC-1 becomes documentation. |
| DEC-NSP-6 | The source of a mapped namespace is the namespace whose own code database holds the IPM code. | `-map` help and issue #470; "last by name" has no source. | Report a tie as an error instead of guessing. Either way F-NSP-7 stays a defect. |
| DEC-NSP-5 | A load refused by a system requirement leaves no module record, also with `-dev`. | Issue #106 ("it should fail"), changelog ("prevent installation"), wiki ("validated before module installation"). | Keep the record on purpose in dev mode (help: "transactions will not be rolled back"): F-NSP-12 becomes documentation. |
| DEC-NSP-4 | If a step of `enable -community` fails, the command stops and returns that error. | Scripts use `zpm "...":1` to stop on errors; every other `enable` path returns its error. | Run all three steps and return a combined error (also fine); or keep OK and document: F-NSP-11 becomes documentation. |
| DEC-SYNC-3 | `sync` with no module name reports each failure and a summary, then returns OK. | PR #1205: it "collects failures without throwing" (says the loop continues; silent on the final status). | Return an error when any module fails, like `sync <module>`: a new Low defect. |
| DEC-SYNC-4 | A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists. | Help: reload "pulls module source code into the namespace from disk"; the code's "For now" no-op. | Fail the standalone command (stricter for scripts). |
| DEC-DEP-3 | A needed module loaded from a folder is written to the lock file with an empty registry. | The writer already does this for the base module (`LockFile.cls#L74-L78`). | Fail the command (if a future `ci` needs a registry for every entry, issue #962): F-DEP-9 changes accordingly. |
| DEC-UNIN-3 | A need declared only for a phase (test, verify) does not block uninstall. | The question's premise was wrong: uninstall, `show` and `list-dependents` already agree on this (IRIS `r4-repo-unin3-*`). | Count scoped needs: three readers would become defects. |
| DEC-HIST-4 | A command may run inside a caller's transaction; its work and its record follow the caller. Document it. | Developer documentation uses outer transactions (wiki page 00, "outer wrapping transaction"). | Refuse at `$TLEVEL` > 0, or write the record outside the caller's transaction: F-HIST-3 becomes a code defect again. |
| DEC-HIST-6 | `history delete` of a running record wins; the command then ends with no record. | Operators may delete any record; a lock error on a row inside an open transaction is acceptable. | A running record cannot be deleted, or it reappears at close: F-HIST-10 becomes a defect again. |
| DEC-LOAD-1 | Answering "Commit" at a crash prompt is an operator error to document. | It is an IRIS terminal choice; guarding it costs writes on every load. | Mark the log committed before each region a halt could commit: F-HIST-4 becomes a defect again. |
| DEC-SEM-1 | `^IRIS.Temp.IPM.LoadedResourceD` is a deliberate IRISTEMP tracking table that lives until IRIS restarts. | `TrackResourceNames` / `Validate` / `Clear` design. | Clear it per command: the INV-TEMP entries of `semantics/` become findings. |
| DEC-SEM-2 | The IRISSYS code-global writes seen once during a web-app uninstall are not IPM's. | Not reproduced in two later write-set runs or in a per-class diff of 5,375 entries (`r4-hist-sem2-*`). | Rerun the first sequence on a fresh instance with a per-class diff to name the classes. |
| DEC-SYS-1 | (Method, not IPM.) Lexical scope: a statement may use a term in scope for its parent. | The strict reading gives 1,040 violations in 585 statements, 256 of them on a module's own definitions. | Strict reading: definitions re-parented or repeated under each module root. |

---

# Part B. Unresolved verification items

"Proved" in this experiment means: proved about a hand-written model of the IPM code, under named premises, with
mutation controls killed. The model's faithfulness is checked by cross-checks and IRIS conformance, never proved.

## Closed in round 4 (for the record)

| Item | How it was closed |
|---|---|
| PM: nothing fails after the final commit | **False.** A `$$$` macro that does not compile in an after-install message fails a committed load (F-LOAD-4). PM is now a defect premise (`load_general.TRUST.md` G7). |
| Mirrors and primary-only steps | Intent settled: not promised at this revision (wiki page 08); the comment is wrong (F-INST-2). Running them still needs a licence (below). |
| `Test.PM.Migration` skip counts | 0 skipped assertions on all 8 IRIS images in IPM's CI at this commit (`validation/ci/`). |
| F-INST-5 on ORAS | Same defect (`r4-inst-u17-*`). |
| A kill between an update step and its record save | Developer mode: the step reruns (at least once, as intended). Non-dev: the record lock is held until commit, so the kill rolls back like a halt in the step. |
| Graceful `iris stop` during a load | A new instance of RC-2 (F-HIST-7). |
| `history delete` in other phases / of a dependency row | The delete wins; a row inside an open transaction is lock-protected (SQLCODE -110). |
| Full cleanup with mapped namespaces | Mapped namespaces lose IPM silently; the warning is lost through F-NSP-7. |
| Scoped needs and uninstall | Scoped needs never block (DEC-UNIN-3). |
| Legacy records reached by `enable` | Not migrated (F-NSP-10), shown with a simulated legacy install. |
| F-NSP-8's collation side effect | No effect on globals. |
| `TestWSGIApp` in IPM's CI | Skipped in all 16 runs at this commit: the test is vacuous. |
| F-RES-7 on amd64 | The wheel test passes on CI's amd64 runners, so the defect is ARM64-only. |

## Still unresolved

| # | Item | Why it is still open |
|---|---|---|
| V1 | Mirrors: primary-only update steps on a real mirror | Community Edition refuses mirroring. The intent is settled; the behaviour on a member that becomes primary is unobserved. |
| V2 | F-INST-5 on a remote (HTTP) registry | No remote registry server was run; by reading it takes the same code path as ORAS and folders. |
| V3 | F-NSP-10 with a real ZPM 0.7.4 install | Simulated in a scratch namespace (real ZPM replaces the server-wide `zpm` command); legacy registry rows and document streams were not simulated. |
| V4 | F-LOAD-4's other post-commit paths (`.modules` cleanup, analytics) | By reading only; not triggered. |
| V5 | The cause of the graceful-stop hang (F-HIST-7) | IRIS waited for the commit, then the job slept about 5 minutes until forced down; the cause was not identified. |
| V6 | F-RES-1 outside a probe patch | No outside trigger makes `AddItemToProject` fail; confirmed only with patch p8b. |
| V7 | Premise PI-dep of the proved history fix (D1) | Proved necessary for any design that keeps pending dependency rows in IRISTEMP (`g_instance_crash_loses_pending`). A durable pending marker written before the dependency's transaction is not modelled. |
| V8 | P3, the commit window (F-HIST-7) | Inherent: IRISTEMP and the namespace database cannot be written atomically. Proofs hold only outside it. |
| V9 | Resolver premises PU and PR | Not enforced by IPM; hold in every explored instance; not probed on IRIS. |
| V10 | INST-B80 (update outcome both ways) has no P | Its L1 rule rests on a self-set ghost; the two-way update outcome is observed, not proved. |
| V11 | Unconstrained L1 fields in load_general (record names its module; failed dependency record's end flag) | Not proved; no P cites them. |
| V12 | Update-lock mutual exclusion; resolver termination | Single-process model; safety only. |
| V13 | Ontology rows of modules 03–09 tagged by inspection | Written by builder agents; the gate checks form and anchors, reviews sampled them; not every row was re-read by a second party. |
| V14 | Research verdicts of round 4 | The key external sources were re-checked against GitHub (TRUST.md T19); other citations rest on one research agent's reading. |
| V15 | Other IRIS versions and architectures for the defects | Defects were run on IRIS 2026.1 ARM64 (load/history also 2025.3). IPM's CI covers its own tests on 8 images, not these defects. |
| V16 | Bounds and scope | Explorer bounds (0–3 dependencies; small resolver and namespace instances); write-set scans skip read-only library databases; named exclusions in `ontology/OUT-OF-SCOPE.md`; areas not modelled at all (uninstall full cleanup and crashes, CPF merges, wheels, author hooks at Clean, concurrent commands, sync `-delete`). |
| V17 | Severity ratings in `BUGS.md` | This run's judgement; maintainers may rate impact differently. |
| V18 | Playbook 3.8 sign-off | Two independent reviews refused sign-off; all their findings were fixed (`review/LEDGER.md`); no third review was run (by instruction), so the round-3 and round-4 changes are not independently reviewed. |
