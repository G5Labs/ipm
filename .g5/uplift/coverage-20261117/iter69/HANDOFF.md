# Iteration 69 handoff: `generate` resource grouping

Builder session, Opus 5.5 (high). Product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, unchanged; shipped tests unchanged. Branch `consolidated_uplift`.

## 1. Scope and result

This iteration targeted `devtools.generate.resources` (#2, #3, #5, #7, #8) and the grouping alias in `devtools.authoring.generate`. The grouping decision is now modeled over sequences of real (path segments, name segments, extension) items, through to the `module.xml` writer and to `load` of the generated module. The model replaces iteration53's Boolean `shared_resource`.

```
source tree --ReadFolder--> files --ReadItems--> items(file)=name, Directory
     |                                   |  skip: unsupported ext, under tests dir, listing fails
     |                         GroupClasses + SharedPackage
     |                       /                         \
     |      packages(dir, top) = deepest shared      leftovers stay in items
     |               |                                        |
     |       AddPackages: <pkg>.PKG, Directory        ReadResources merge: <item>, Directory
     |               \______________________   _______________/
     |                                      v v
     |                 ProcessResources -> ResourceReference objects
     |                                      |
     |       Module.CalculatedResourcesGet: one entry per GetSortKey (name+ext), later wins   <- C:D72
     |                                      v
     |                               module.xml (SaveFile)
     |                                      v
     +-- tests folder --> same grouping --> UnitTest entries; test leftovers dropped
                                            v
                        load: Document %OnNew/OnPhase picks Directory | ext folder | root    <- C:D73
```

## 2. Predictions vs observations

Predictions were written before reading the deciding code (`predictions.md`). The disclosure there notes that iteration68 had partly exposed this area to the census and the master. The results table is appended to `predictions.md`. In summary:

- Held: two top-level packages give two package resources (N2). A.B.C with A.D.E gives A.PKG (N4b). Tests inside the source folder become a UnitTest only (N7). Routines and include files are written individually and `.txt` is skipped (N8). A missing source folder fails (N10).
- Missed: the group name is the deepest shared package, not the top-level package (N1). A lone class gives `A.B.PKG`, not a `.CLS` (N3). "Directory" is the folder above the package folders, not the file's folder (N4).
- Not predicted by either source: the writer collapse that drops a directory's classes (C:D72), the `load` failures for root and flat layouts (C:D73), and listing of a class with a broken body.

## 3. Formal artifacts

`.g5/Verus_verification/coverage-20261117/iter69/grouping.rs` (48 verified, 0 errors; Verus 0.2026.09.27.3cf1832, sha256 `2fe2fca3…1bc854`). No `assume`, `admit` or `external_body`. Concrete facts use `by (compute_only)`, so the Verus interpreter is in the trusted base for them.

Domain: arbitrary `Seq<File>`. Name and path segments are ints that stand for non-empty strings, equal exactly when the strings are equal. `File.listed` is a provider input (the `$system.OBJ.Load` list-only result). `in_tests` is the tests-directory prefix test. The AddPackages key list `keys` is an input. `keys_ok` states its premise (complete, used, distinct), and `native_keys_ok` discharges it for every native fixture.

General theorems (all file sequences):

| Theorem | Statement | Premises |
|---|---|---|
| `lemma_shared_prefix` | `shared(a,b)` is a prefix of both and contains every common prefix | none |
| `group_characterized` | The group exists iff it has a member. Its name starts with the top-level segment, is a package of every member and is the deepest such, so it is independent of file order. | none |
| `class_in_own_group`, `class_written_before_collapse` | Every kept packaged class has a written entry for its own (directory, top-level), whose name is a package of the class | `keys_ok` |
| `singles_exact` | Leftovers are written exactly for kept non-grouped items, with their own name, extension and directory | none |
| `writer_one_entry_per_name` | `count_name(collapse(es), n, x) == [has_name(es, n, x)]` | none |
| `same_name_two_directories_keeps_one` | Two directories whose groups share a name keep that name for at most one of them (as-built divergence) | `keys_ok`, directories differ, names equal |
| `package_layout_round_trip` | A file at `d + package` gets Directory `d`, and `load` finds it | `d` non-empty |

Neighbour theorems (compute): `grouping_names` (N1-N5b), `same_package_two_directories_loses_class`, `leftovers_and_skips`, `load_resolution`, `tests_and_status`, `m3_keeps_one_directory`.

Model limits:
- The written order is insertion order, not the `GetSortKey` order. All claims are about membership and counts.
- Name case is not modeled. Classes are listed as `.cls` and routines as `.MAC`, and the reader uppercases on load.
- The reader model covers the directory choice only. It does not model the extension fallback (`.xml` in error text) or `rtn` folders.
- A PKG entry "resolves" when some listed class lies under it.

## 4. Controls

Each control is a one-line mutation of `grouping.rs`, run by the harness. All 13 were rejected with an assertion failure.

| Control | Altered rule | Rejected by |
|---|---|---|
| name-top-level-only | group name = top-level package | `grouping_names` (N1 gives [101] instead of [101,12]) |
| seed-with-top-level | seed group with the top level | `grouping_names` (N3) |
| directory-only-key | group by directory only | `grouping_names` (N2) |
| top-level-only-key | group by top level only | `grouping_names` (N5b) |
| directory-keeps-whole-path | Directory = file's folder | `grouping_names` |
| drop-leftovers | leftovers not written | `leftovers_and_skips` |
| writer-keys-by-directory | writer keeps one entry per (name, directory) | `same_package_two_directories_loses_class` |
| tests-leftovers-kept | test leftovers written as resources | `tests_and_status` |
| listing-ignored | skip by extension only | `leftovers_and_skips` |
| unsupported-kept | every extension supported | `leftovers_and_skips` |
| tests-not-excluded | no tests-directory exclusion | `leftovers_and_skips` |
| reader-root-before-ext-dir | reader uses the source root for empty Directory | `load_resolution` |
| missing-source-continues | missing source folder does not fail | `tests_and_status` |

Compute-mode rejections stop at the first failing assertion, so their logs have no summary count (`receipts/README.md`).

## 5. Native correspondence

Harness: `.g5/Verus_verification/coverage-20261117/common/harness.py`, a copy of `consolidated/common30/harness.py` with `PREFIX='ipmcov20261117-'` and output under this directory. Container `ipmcov20261117-iter69-<stamp>`, `--network none`, image `ipmcont20261116-iris:571e04a1-fresh` (`sha256:7da4480b…`). The source pin was verified.

Command (repository root):

```sh
VERUS=~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus IPM_CALL_TIMEOUT=1500 \
  python3 .g5/Verus_verification/coverage-20261117/iter69/run.py
```

Result `run.json`: ok, 48 verified, 13 controls, 59 native fields, 59 replayed, 35 s wall.

Fixtures (`native/`) are driven by piped interactive `generate`: name, version, description, keywords, source, tests, web apps, CORS N, JWT N, dependencies.

| Case | Tree | Observed |
|---|---|---|
| m1 | Groups N1-N4, routine, include, txt, headerless class; tests outside src with leftovers | 5 PKG + MAC + INC (7 resources), 1 UnitTest; load ok |
| m3 | Id1.B in x and y; Id2.B in x, Id2.D in y | `Id1.B.PKG` only with Directory y; after load `Id1.B.C` absent, others present |
| m4 | tests inside src with leftovers | `Ie1.PKG`, UnitTest `/src/tests/` Ie1T; load ok |
| m2a / m2e | root class with / without a cls folder | `Ir2a.cls` (no Directory): load fails / `Ir2e.cls`: loads (`User.Ir2e`) |
| m2b | flat `flat/Ik2b.P.Q.cls` | `Ik2b.P.PKG` no Directory; load fails |
| m5 | good, body-broken, headerless classes | `Ig1.PKG`, `Ig2.PKG`; no Ig3 |
| m6 | missing source folder | "Directory …/nosrc/ doesn't exist"; no module.xml |

Replay: all 59 fields are asserted against `grouping.rs` expressions, with each model value computed by `compute_only` (`replay/native.rs`: 49 verified, 0 errors). Three flipped deciding fields were each rejected with a passing neighbour: `m1.Ia1.B.PKG@cls` (1 to 0), `m3.Id1.B.PKG@x` (0 to 1) and `class.Id1.B.C` (0 to 1).

Unsuccessful attempt preserved in `native-attempts/`. The first run's observer looked for `Ir2e` instead of `User.Ir2e`. A probe container confirmed that IRIS compiles package-less classes into `User`. All other fields matched.

The expectations are not blind. Scratch exploration (`/tmp/ipmcov69-explore`, containers `ipmcov20261117-iter69-explore`/`-probe`, not committed) ran before the model was written and first revealed C:D72 and C:D73.

## 6. Bindings delta

`bindings.json` (iteration67 format plus per-symbol `roles` with lines and decide/write/observe):

- Newly reached members (no earlier binding in the consolidated project map): `Storage/Module.cls::CalculatedResourcesGet` (observe, 38-47), `Utils/CodeGeneration.cls::BuildRelationshipSorter` (decide, 4-31) and `Storage/ResourceReference.cls::GetSortKey` (decide, 509-537).
- These already-reached members gain an active behavioral binding to iteration69 claims:
  - ModuleTemplate: `ReadItems` (decide 458-487), `GroupClasses` (decide 494-515), `SharedPackage` (decide 518-528), `AddPackages` (write 413-438), `ReadResources` (decide/write 397-408), `ProcessResources` (write 530-549) and `ReadFolder` (decide 555-558).
  - `Main.cls::GenerateModuleXML` (observe 1487, 1556-1557).
  - Document: `%OnNew` (decide 464-471) and `OnPhase` (decide 70-130, observe 145).

The distinct total is 14 members, 3 of them new. The earlier iter53 and iter6 bindings stay. iter53 `shared_resource` is superseded for the name and the directory key.

## 7. Census delta

All ERROR-free (`census.py count`), with no stale blocks. Counts moved from 280 assessed (174 full, 63 partial, 43 absent) to 286 (185 full, 60 partial, 41 absent). Assessed-scope coverage went from 0.6214 to 0.6469.

`devtools.generate.resources` was rewritten (body hash `8cb8214e` to `bc896f1e`; units P1 and B1-B8), basis `new:iteration-69`:

| Obligation | Before | After |
|---|---|---|
| #1 tests kept out of application resources | full | full (re-based, adds iter69 models) |
| #2 UnitTest entries | partial | full (Phase/ProcessorClass constants checked natively) |
| #3 missing source folder | partial | full |
| #4 missing tests folder | full | full |
| #5 skips (unsupported, unlisted; body-broken listed) | partial | full (listing is a provider input, observed) |
| #6 working assumption | alias #5 | alias #5 |
| #7 grouping key (directory, top-level) | partial | full |
| #8 group name = deepest shared package | absent | full |
| #9 Directory attribute | (new) | full |
| #10 leftovers written individually | (new) | full |
| #11 tests-folder leftovers written nowhere | (new) | full |
| #12 one resource per name | (new) | full |
| #13 as built C:D72 | (new) | full (as-built proved; refutes the coverage promise) |
| #14 as built C:D73 | (new) | full |

`devtools.authoring.generate#9` ("exported items become resources") moves from absent to partial. The gap is `ExportOneResource` file placement, which is unmodeled. iter53 t3 shows the exported routine written without a Directory. #3 and #4 remain aliases of resources #7 and #2.

## 8. Ontology and findings

The ontology change is to `devtools.generate.resources` only. It replaces the "shared top-level package … no general proof" bullet with the as-built rule. It adds the Directory, leftovers, one-per-name (C:D72) and load (C:D73) statements. The open-question link is kept. Metadata gains the model link, findings back-links and the model manifest `models/cov20261117-iter69-grouping`.

New findings (`bugs_issues.md`):

- **C:D72 (Medium, E2E reproduced).** One package in two directories: module.xml keeps one directory, and the other directory's classes are silently missing after `load`.
- **C:D73 (Low, E2E reproduced).** A package-less root class beside a `cls` folder, or a flat-named class file, gets no Directory, and `load` fails. Inherited iter53 evidence shows `generate -export` produces a flat dotted routine with no Directory. That export-to-load failure has only a scratch probe, not a committed receipt.

Intended policy versus defect:
- C:D72 contradicts the coverage promise, so it is a defect.
- C:D73's root-class case contradicts the evident intent. Its flat-layout case may be unsupported input, and the finding says so.
- Tests-folder leftovers are recorded as as-built behavior only, with no defect claim. Whether the UnitTest processor loads them at test time was not checked.
- Repairs are proposed but not checked.

Format checks:
- `check_ontology.py`: 0 issues, 45 writing warnings (the baseline was 45). The `devtools.generate.resources` warning about the undefined ID level `devtools.generate` was already present.
- `check_findings.py --previous /tmp/ipmcov69-findings-prev`: 38 problems, all pre-existing "Run: line away from status line" on older entries. The new entries put Run directly after the status line.
- `link_verification.py --links-only --dry-run`: 0 changes.

## 9. Residual work

1. Order of written resources (`GetSortKey`: PKG/CLS, MAC, INC, other extensions, then `/` paths; lowercase `.cls` leftovers sort with "other") is observed but not modeled.
2. `ExportOneResource` file placement, to close `devtools.authoring.generate#9` and the export-to-load round trip for dotted routines (C:D73 extension). A receipt-backed native case is needed.
3. Whether the UnitTest processor loads tests-folder routines at `test` time (the consequence of #11).
4. Overlapping package resources across directories (A.PKG in x with A.F.PKG in y) were not exercised. Neither were the `.int` extension, Windows path separators or case-insensitive file systems.
5. The reader model is a simplification of `Document.OnPhase`. The extension fallback and the `rtn`/`mac` alternates are unmodeled.
6. No independent read-only review has been run on this iteration. The master schedules it.

Other workflows were not sampled. The next material gap, by this iteration's evidence, is the `-export` placement plus round trip (item 2). It shares the C:D73 reader and could reuse `grouping.rs`.

## 10. Cost

Wall time is about 25 minutes (01:30Z to 01:55Z, 2026-11-18). Harness runs took 22 s for the failed attempt and 35 s for the final run. Exploration containers ran for about 3 minutes. Token use is not visible to this session. All `ipmcov20261117-*` containers were removed. No other container or image was touched.
