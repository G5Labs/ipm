# IPM iteration 5 handoff

Product unchanged at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. Iteration 4's seal is the comparison checkpoint; the live tree matched it except for the 45 known excluded links. New register entries: C:D23-D26, C:Q15, C:S14b and C:S17-S19. No policy was accepted and no repair applied.

## Review of iteration 4 and the two challenges

Iteration 4's claims were retested where their inputs enter:

- Exit code (`build.test.outcome.exit`) was checked at the real CLI. `zpm "<cmd>",1` does not terminate, and `$zpm()` raises `<COMMAND>`. The second is a regression from commit `8eb520fd`, and it also refutes the inherited root note that the function form returns the status. The iteration 4 sentence stands with a limitation; the root note is corrected.
- Registry credentials (C:S14) were checked with a redirect. A same-origin URL that redirects elsewhere still leaks, so the iteration 4 proposal is insufficient and is revised as C:S14b. C:D20 gains evidence.

## New source-led workflow: web applications

```
  load/install ─► Configure, Activate: OnBeforePhase ─► Security.Applications Exists? ─► Modify declared keys | Create
                  Validate: lifecycle never calls the processor's check (C:D23)
  failure ─► transaction rollback removes the app; -DNoTransaction keeps app and module
  upgrade without the app ─► app stays; uninstall knows only the new manifest (C:D26)
  uninstall ─► OnBeforePhase("Clean") deletes by name ─► %Clean ─► dependent check: skip and success (C:D25)
```

| Kind | Owner | Distinction a reader can now predict |
|---|---|---|
| New | [module.webapp](../../../ontology/concepts/modules.g5.md#module.webapp) | An administrator's same-named app is adopted (declared keys only) and later deleted (C:Q15); a second module claiming the name is refused |
| New | [module.webapp.failure](../../../ontology/concepts/modules.g5.md#module.webapp.failure) | Rolled back with the transaction; kept with `-DNoTransaction=1` |
| New | [module.webapp.upgrade](../../../ontology/concepts/modules.g5.md#module.webapp.upgrade) | Orphaned app after an upgrade drops it (C:D26) |
| New | [module.webapp.validation](../../../ontology/concepts/modules.g5.md#module.webapp.validation) | Unknown attributes accepted silently (C:D23) |
| Promotion | [uninstall.dependents.as-built](../../../ontology/features/uninstall.g5.md#uninstall.dependents.as-built) | P:F-UNIN-1, P:F-UNIN-6 and G:OBS-L17 as C:D25 with fresh native and Verus evidence; the intent block keeps the refusal |
| Challenge | [ipm.shell.forms](../../../ontology/root.g5.md#ipm.shell.forms) | Only `Shell()` exposes failure; `$zpm()` broken (C:D24) |
| Challenge | [repo.remote.download.redirect](../../../ontology/concepts/repositories.g5.md#repo.remote.download.redirect) | Credentials follow redirects; C:S14 to C:S14b |

## Evidence

`python3 .g5/uplift/consolidated/iter5/run_all.py` runs the contracts, a fresh-container native suite, the replay and the validator. It covers 7 verified units with 16 wrong variants rejected, 18 observation cases plus 5 entry-form sessions, and 30 replayed fields with 3 flipped controls rejected. The [index](INDEX.json) separates direct deciders from readers. Iteration 4's full command was replayed unchanged in a scratch extraction ([inherited-replay](inherited-replay/)).

## Blind spots and where this stopped

- Web applications: `CSPApplication` (deprecated) file purge, JWT/CORS settings, developer-mode commit points, `-DNoJournal=1`, the `update` command and registry upgrades, `clean` at level 0 and `-recurse` were not re-run.
- Redirects on metadata, listing and `_ping` requests, plus HTTPS, are untested. Why `zpm "…",1` ignores the flag was not traced.
- Not started: mirrored update steps, ZPM-to-IPM migration and Python wheel installation.

## Owner decisions needed

C:Q13, C:Q14 (now per redirect hop), C:Q15, and all earlier open questions.
