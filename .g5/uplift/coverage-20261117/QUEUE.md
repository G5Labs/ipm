# Queue (master-maintained)

## Census scope and denominator (published at iteration70, commit 69e06436)

All 265 canonical I/D blocks in 27 pages (975 units) are enumerated: 258 enumerated, 7 structural, 0 unenumerated, 0 stale. 1,076 counted obligations (aliases excluded): full 697, partial 214, absent 165, unassessed 0. Whole-foundation coverage 697/1,076 = 64.78% (no unassessed, so lower = upper bound). Design checks: proved 619, bounded 18, refuted 60, unchecked 214, none 165; full with a passing check 637. Replay: `python3 -I .g5/uplift/coverage-20261117/census/census.py count`; per-block table in [census/COUNTS.md](census/COUNTS.md). Later seals report retained-obligation changes separately from scope changes.

Grades are pending the checkpoint review (review-71); fork-graded full obligations are only sample-checked.

## Material gaps, ranked (from iteration68 §4 and iteration70 §4)

| # | Family | Key obligations | Status |
| --- | --- | --- | --- |
| 1 | `generate` grouping, emitted name, leftovers | devtools.generate.resources#2,#3,#5,#7,#8 | closed by iteration69 (C:D72, C:D73); residual: `-export` placement and export->load round trip |
| 2 | History rows and Committed for install, update, uninstall; commands without rows | history.actions#2,#3,#5; history.record#2,#8; history.committed#3; committed.uninstall#1; history.phases#3,#4 | iteration72 |
| 3 | Archive and git load paths, write check | load.path#2-4; load.path.archive#1-3; load.write-check#1; install.package#1 | queued |
| 4 | IPM installer and setup, setting seeding | setup#1; setup.installer#1-7; settings#6,#7,#12 | queued |
| 5 | Rollback boundary: NoJournal dependency defaults; effects outside the database (lock file, pip, FileCopy around failing Activate) | transaction.flag-sources#5,#7,#9; history.committed#14; transaction.rollback-scope#1; history.committed.side-effects#1; deps.lockfile#4,#6; artifact.failure#5; file-copy#5,#6 | queued |
| 6 | Dependency version conflict | deps.conflict#1-3 | queued |
| 7 | Update refusals and update-specific resolution | update.refusal#3-5; update.requirements#1,#2; update.dev-mode#1 | queued |
| 8 | ORAS validated layer vs loaded layer; auth/lookup | repo.oras.integrity#4; repo.oras.archive-boundary#3; repo.oras#2,#3 | queued |
| 9 | Read-only repository forces normal mode | repo.read-only#1; dev-mode#8 | queued (small; pair with 7 or 3) |
| 10 | History deletion and retention | history.delete#2,#3,#6,#8,#9; delete.global-unfiltered#1; history.retention#1-3,#5 | queued |
| 11 | uninstall -all traversal; before-Clean hook order | uninstall.all#1-3; uninstall.dependents#5 | queued |
| 12 | System requirement kinds (Version, IPMVersion, PythonVersion) | module.requirement#1,#2; load.refusal.requirements#1 | queued |
| 13 | Path refusals; temporary local repository | load.refusal.path#1-3; load.dependencies.local-repo#2,#3 | queued |
| 14 | Repository selection filters; concurrent cache refresh | repo.search-order#1,#2; repo.prereleases#1,#2; repo.filesystem#7 | queued |
| 15 | ExternalName uniqueness; enable -community | module.identity#2; namespace.enable#9,#11 | queued |
| 16 | Sync aggregate status; module-version atomicity; stale dependents after warned compile | devtools.sync#20; module-version#1; compile-after-resolve#3 | queued |
| 17 | Ontology accuracy repairs (no model needed) | iteration68 findings 1-10, iteration70 findings 1-23 | queued as one repair batch |

## Leads not yet assessed

- Residual unlinked members from the earlier run ([residual-final.json](../consolidated/common30/residual-final.json)).
- Independent source-first discovery: not yet run in this loop.
