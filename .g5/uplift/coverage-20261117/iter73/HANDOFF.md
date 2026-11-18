# Iteration 73 handoff: review-71 repairs to iteration69 and the `generate -export` -> `load` round trip

Builder session, Opus 5.5 (high). Product pin `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, unchanged; shipped tests unchanged. Branch `consolidated_uplift`. Iteration69's model, receipts and handoff are kept unchanged as history.

## 1. Review-71 section C: items and resolutions

| Item | Resolution |
| --- | --- |
| C1: `devtools.generate.resources#2` full only by native string match | `TestEntry` now has Name (tests folder + directory), processor, Package and Phase. They are decided by the AddPackages tests-folder branch (`add_packages`). `test_class_has_unittest_entry` holds for every tests listing. Controls `tests-branch-swapped`, `unittest-name-without-folder` and `unittest-phase-other` are rejected by the general theorems alone. Native m1/m4 observe each field separately and read the entry back after `load`, and three of those fields are flipped. #2 is now full. |
| C2: general theorems do not pin the key, test exclusion or extension filter; no general positive theorem | `grouping.rs` now holds only definitions and general theorems, and the instances live in `instances.rs`. So a control on `grouping.rs` is judged by the general theorems alone. All 22 mutants are rejected that way, including `top-level-only-key`, `tests-not-excluded` and `unsupported-kept` (table in section 4). The tests-folder test is computed from the tests answer and the file path (`under_tests`), not taken as `in_tests`. The general positive theorem is `class_loads_when_no_clash`: with no group-name clash and files in package folders, every kept class is written and found by `load`. Grades are general, not instance-scoped, except the flat-class sub-case of #14 (section 7). |
| C3: thin replay flips; collapse key order hand-written | The key order is derived. `order_key` renders `pkg:/<directory>:<package>`, `lex_lt` is character-code order and `keys_sorted` states the `$order` premise. `collapse_earlier_key_loses` proves that the key sorting earlier loses. Native m7 (`a` vs `a/b`) and m8 (`a` vs `ab`) discriminate this from a "later directory name" order. Both were predicted before the run. There are 16 flipped-field controls (was 3): five C:D73 load fields, three collapse winners, two class-after-load fields, three UnitTest fields, two export fields and the backup. |
| C4: C:D72 severity and trigger | Severity is now High, with the basis "incorrect results, E2E reproduced". The trigger is reworded in the finding and ontology B7 to "groups from two directories get the same package name". It gives the review's x/y example, the no-loss counter-example and which directory wins. |
| C5: C:D73 severity; `generate -export` trigger needs a receipt | Severity is now Medium (visible misbehaviour). `Expfilename` placement is modeled (`exported`). The general theorem `export_dotted_item_fails_load` covers this, and native x1-x4 is the committed receipt (section 2). The ontology B8 and the finding name the export trigger. |
| C6: readability of the block; module.xml backup | The block now opens with what `generate` produces. The grouping examples say "classes". Directory is qualified to files stored in package folders. The rename of an existing `module.xml` to `module_<timestamp>.xml` is stated, confirmed in source (`SaveFile:661-665`) and natively (m8), and modeled (`save_file`). |

## 2. Export round trip: predictions vs observations

Predictions are in `predictions.md`. The table was written before reading `ExportOneResource`/`Expfilename` and before any iteration73 run. Disclosure: review-71 C5 had already said that dotted routines are written flat. The m7/m8 and backup predictions were added after reading the deciding code but before any native run.

```
namespace items --ExportOneResource/Expfilename--> <src>/<ext>/...   (cls: package folders; mac/inc: flat)
      |                                                   |
      |                               ReadItems: Directory = path minus one folder per name dot
      |                                                   |
      |                             module.xml: Ixa.P.PKG Directory=cls ; Ixa.R.Rtn.MAC (no Directory)
      v                                                   v
  DeleteExported (sanity pre.* = 0)        load: Document reader looks in mac/ + Ixa/R/Rtn.(xml|mac) -> not found
```

| Case | Prediction | Observation | Held |
| --- | --- | --- | --- |
| x1 class `Ixa.P.Q` + dotted routine `Ixa.R.Rtn` | routine flat in `mac/`, no Directory, `load` fails | `Ixa.R.Rtn.MAC -> mac/Ixa.R.Rtn.mac`, `<Resource Name="Ixa.R.Rtn.MAC"/>`, "Resource path '/tmp/iter73/x1/src/mac/Ixa/R/Rtn.xml' not found", neither item back | yes |
| x2 + undotted routine `IxbRtn` | Directory `mac`, loads | as predicted, both items back | yes |
| x3 + undotted include `IxcInc` | Directory `inc`, loads | as predicted | yes |
| x4 + dotted include `Ixd.I.Inc` | flat, no Directory, fails | as predicted | yes |
| export target | source folder or module folder (uncertain) | the answered source folder | resolved |
| m7 / m8 collapse | `a` kept / `ab` kept | as predicted | yes |
| m8 backup | one backup holding the old content | `module_1154716478026513909.xml` (a `%PosixTime` integer) | yes |

No miss. The ontology alone predicted that all four export cases load, which is refuted for x1 and x4.

## 3. Formal artifacts

`.g5/Verus_verification/coverage-20261117/iter73/`:

- `grouping.rs` has definitions and general theorems only (96 verified, 0 errors).
- `instances.rs` has the neighbours, native transcriptions, `native_keys_ok` (keys complete, used, distinct and in `$order`), `collapse_winners` and `export_round_trips` (102 verified with the imported module).
- `cases.json` is generated by `make_cases.py`. `publish.py` writes the viewer manifest and rows.

Verus is 0.2026.09.27.3cf1832. There is no `assume`, `admit` or `external_body`. Instance facts use `by (compute_only)`.

General theorems (for every file sequence):

| Theorem | Statement |
| --- | --- |
| `group_key_separates`, `group_key_joins` | A file outside (directory, top-level) never changes a group; a kept packaged class seeds or narrows its own group |
| `tests_folder_files_never_written`, `unsupported_never_written`, `unlisted_never_written` | such a file is in no group and no leftover |
| `group_characterized`, `class_in_own_group`, `singles_exact` | as iteration69, re-proved with the computed tests exclusion |
| `layout_directory`, `package_layout_round_trip`, `layout_loads` | in package layout, Directory is the containing folder and `load` succeeds |
| `class_loads_when_no_clash` | general positive theorem (C2) |
| `test_class_has_unittest_entry`, `source_listing_adds_no_unittest`, `tests_listing_never_resources` | UnitTest fields; the tests listing adds no resources |
| `writer_one_entry_per_name`, `collapse_kept_is_last`, `collapse_keeps_unique`, `same_name_two_directories_keeps_one`, `collapse_earlier_key_loses` | writer collapse and its winner (C:D72) |
| `reader_directory_rule`, `root_class_beside_cls_folder` | reader rule and root-class case (C:D73) |
| `export_layout`, `export_dotted_item_fails_load`, `export_round_trip_loads` | export placement and round trip (C:D73) |
| `missing_source_writes_nothing`, `save_keeps_previous` | missing source folder; module.xml backup |

Premises and limits:

- `listed` (the list-only load) and directory existence are provider inputs.
- `keys_sorted` is the premise that IRIS orders non-numeric local subscripts by character code. m3, m7 and m8 observe it, and `keys_ok` with `keys_sorted` determine the key list.
- `seg_chars` is the character table for the fixture names.
- The tests-folder prefix assumes normalized paths. Case-insensitive file systems and symbolic links are outside the model.
- UnitTest entries are not collapsed. This is by inspection of `GetSortKey` (the OREF suffix).
- Write order is not modeled beyond membership.
- The reader's `.xml`-first extension probe and the `rtn`/`mac` alternates are not modeled.

## 4. Controls

Command (repository root):

```sh
VERUS=~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus IPM_CALL_TIMEOUT=1500 \
  python3 .g5/Verus_verification/coverage-20261117/iter73/run.py
```

`run.json`: ok, 198 verified (96 + 102), 26 controls, 139 native fields, 121 replayed, 281 s wall.

Each mutant of `grouping.rs` was verified against `grouping.rs` alone, which holds no instance assertions. All were rejected. The failing theorems come from the logs in `proofs/controls/grouping.rs/`.

| Mutant | Failing general theorems |
| --- | --- |
| name-top-level-only | group_characterized, group_key_joins |
| seed-with-top-level | group_characterized, group_key_joins |
| directory-only-key | group_characterized, group_key_separates |
| **top-level-only-key** (survived in iter69) | group_key_separates, class_loads_when_no_clash, written_entry_resolves |
| drop-leftovers | singles_exact |
| writer-keys-by-directory | same_name_two_directories_keeps_one, collapse_earlier_key_loses, lemma_drop_key_count |
| tests-leftovers-kept | tests_listing_never_resources |
| listing-ignored | unlisted_never_written, class_loads_when_no_clash, written_entry_resolves |
| **unsupported-kept** (survived in iter69) | unsupported_never_written, export_dotted_item_fails_load, written_entry_resolves, lemma_singles_no_pkg, collapse_earlier_key_loses |
| **tests-not-excluded** (survived in iter69) | tests_folder_files_never_written |
| reader-root-before-ext-dir | reader_directory_rule, root_class_beside_cls_folder, export_dotted_item_fails_load |
| missing-source-continues | missing_source_writes_nothing |
| directory-keeps-whole-path | package_layout_round_trip, layout_directory, export_layout |
| tests-prefix-from-scan-folder (new) | tests_folder_files_never_written |
| tests-branch-swapped (new) | test_class_has_unittest_entry, source_listing_adds_no_unittest |
| unittest-name-without-folder (new) | test_class_has_unittest_entry |
| unittest-phase-other (new) | test_class_has_unittest_entry |
| collapse-keeps-first (new) | writer_one_entry_per_name, collapse_kept_is_last, collapse_keeps_unique |
| keys-sorted-descending (new) | lemma_keys_sorted |
| export-nests-all (new) | export_layout |
| export-classes-flat (new) | export_layout |
| no-backup (new) | save_keeps_previous |

Dependency controls on `instances.rs` (grouping.rs altered):

- `key-separator-below-slash` (`:` -> `-`) and `key-separator-above-letters` (`:` -> `~`) change only the key rendering. They leave the general theorems true and are rejected by `native_keys_ok` (`keys_sorted` of m7 or m8). This is the instance-level check that the separator decides m7 and m8.
- `export-nests-all-instances` and `unittest-name-without-folder-instances` are rejected inside the imported grouping.rs general theorems (`export_layout`, `test_class_has_unittest_entry`). They add nothing beyond the grouping.rs controls.

## 5. Native receipt and replay

- Harness: `common/harness.py` (container `ipmcov20261117-iter73-<stamp>`, `--network none`, image `ipmcont20261116-iris:571e04a1-fresh`, id `sha256:7da4480b…`, source pin verified). The harness gained an optional `flip_fields` list. Without it, the first three fields are flipped as before, so earlier iterations replay unchanged.
- Receipt: `native/20261118T034517721195Z/observations.json`. An earlier `--native-only` run at `20261118T034334203431Z` is kept. It gave the same 139 values.
- Cases: m1-m6 are iteration69's fixtures under `/tmp/iter73`. The new cases are m7, m8 (with an old module.xml) and x1-x4. The x cases are seeded from `native/seed/`, then `generate <x> -export 00000,<prefix>`, `DeleteExported`, then `load`.
- Sanity fields, checked but not replayed: seed items exist (8 fields = 1), they are gone before `load` (8 = 0), and no `int` folder was exported (x1, x2).
- Replay: all 121 fields are asserted against `instances.rs` expressions computed by `compute_only` (`replay/native.rs`: 103 verified, 0 errors).
- Flips, each rejected with a passing neighbour: `m2a.load.ok`, `m2b.load.ok`, `x1.load.ok`, `x4.load.ok`, `x2.load.ok`, `m3.Id1.B.PKG@y`, `m7.Im7.PKG@a`, `m8.Im8.PKG@ab`, `class.Id1.B.C`, `class.Im7.Y`, `m4.unittest.name`, `m4.unittest.phase`, `m1.unittest.package`, `x1.file.Ixa.R.Rtn.flat`, `x1.Ixa.R.Rtn.MAC` and `m8.backup.old`.
- Unsuccessful attempts: none. Before the native run, a local bridge confirmed that every expected value in `cases.json` equals the model value (223 verified).

## 6. Findings (`bugs_issues.md`)

The findings were snapshotted to `/tmp/ipmcov73-findings-prev` before editing. IDs, anchors and Run lines are preserved.

- **C:D72** goes from Medium to High and moves to the end of the High block. The title is reworded to "drops one directory's classes when groups from two directories get the same package name". The finding adds the severity basis, the review's example and counter-example, the derived winner, the iteration73 general theorems and controls, and native m7/m8 with flips. It remains E2E reproduced.
- **C:D73** goes from Low to Medium and moves to the end of the Medium block. The title now includes "dotted routines that `generate -export` writes". It adds the export trigger and the committed receipt (x1, x4, with neighbours x2, x3). The scratch-probe paragraph is replaced. The finding notes that a Directory alone would not repair dotted routines. It remains E2E reproduced.

Format checks (`verification_root=.g5/Verus_verification`, guidance scripts from `$G5_HARNESS/guidance/scripts`):

- `link_verification.py --links-only` added one link (`devtools.authoring.generate` -> `cov20261117-iter73-grouping`).
- `check_ontology.py`: 0 issues and 45 writing warnings, the same as the baseline.
- `check_findings.py --previous /tmp/ipmcov73-findings-prev`: 38 problems. All are the pre-existing "Run: line away from status line" on older entries, the same count as before the edit.

## 7. Ontology and census

Ontology `devtools.generate.resources` (body `bc896f1e` -> `3f47b28d`, units P1, B1-B9):

- The block opens with what `generate` produces and the `module_<timestamp>.xml` rename.
- The tests sentence moved to B2.
- The grouping examples say "classes".
- Directory is qualified to "a file stored in package folders".
- The C:D72 bullet uses the two-directories-same-group-name trigger and "while `load` reports success".
- The C:D73 bullet names the export trigger.

Metadata: the model and theorem list point to iter73 (iter69 is kept as history), Sources add `ExportOneResource,Expfilename`, the Findings titles are updated and the iter73 manifest is added. `devtools.authoring.generate`: body unchanged; its metadata gains the export model link and a C:D73 Findings line. Manifest: `models/cov20261117-iter73-grouping/` (46 runs). There are 46 rows in `results/viewer-index.jsonl`.

Census (`census.py count`: no ERROR or WARN lines, no stale blocks; selftest passes):

| Obligation | Before (HEAD, iteration72) | After |
| --- | --- | --- |
| resources#1 tests-folder exclusion | partial | full (computed `under_tests`, general theorem, two general-only controls) |
| resources#2 UnitTest entries | partial | full (fields modeled, three general-only controls, native per field and after load, flips) |
| resources#5 skips | partial | partial (extension and unlisted rules now general; gap is the readable-name decision, a provider input) |
| resources#3, #4, #7-#12 | full | full (re-based on iter73 general theorems; #4 unchanged) |
| resources#11 tests-folder leftovers | full, instance-scoped | full, general |
| resources#13 C:D72 | full | full (reworded; winner derived) |
| resources#14 C:D73 root/flat class | full | full (the flat-class sub-case is instance-scoped, m2b) |
| resources#15 module.xml backup (new, P1) | - | partial (the file name format is native-only; rename failure unmodeled) |
| resources#16 lead summary (new, P1) | - | full by composition of #7, #10, #2 |
| resources#17 C:D73 via `generate -export` (new, B8) | - | full |
| authoring.generate#9 exported items become resources | partial, unchecked | partial, proved (gap narrowed to DFI and the documented composition with iter53 selection) |

Totals: assessed 1097 -> 1100, full 583 -> 587, partial 350 -> 349, absent 164 -> 164. Assessed-scope coverage goes from 0.5314 to 0.5336. #16 is a composed lead, so its credit rests on #7, #10 and #2.

Bindings delta (`bindings.json`, rows S73-01..06): 16 distinct members, 0 newly reached.

- `Expfilename`, `ExportOneResource` and `RelativeFolder` (reached by iteration53) gain an active behavioral binding to the placement and UnitTest-name claims.
- `SaveFile` gains a decide role.
- `ReadResources`/`ReadItems` now bind the computed tests exclusion instead of an `in_tests` input.
- `ProcessResources` and `BuildRelationshipSorter` bind the derived collapse order.

Material predictions gained:

- which directory wins a clash (m7/m8);
- that `generate -export` output fails `load` for dotted routines and include files;
- the UnitTest Name for tests inside the source folder.

## 8. Residuals and recommendations

1. **DFI export (source-only lead).** `-export` with flag byte 5 = 0 exports DeepSee items to `src/dfi/`. `ReadItems` lists only cls, mac, inc and int, so by the model they never become resources and `load` does not bring them back. There is no native DFI case. It needs a DeepSee item fixture to confirm before it becomes a finding.
2. The backup file name (`module_<%PosixTime>.xml`) and a failed rename are native-only or unmodeled (#15).
3. Whether two UnitTest entries with one Name (two top-level test packages in one tests directory) both survive is by inspection only. No native case covers it.
4. Overlapping package resources across directories (`A.B` in `x` with `A.B.C` in `y`) are predicted lossless by the model. This was not run natively.
5. As in iteration69: whether the UnitTest processor loads tests-folder routines at `test` time, the `.int` extension, Windows separators and case-insensitive file systems.
6. Readable-name versus body-broken classes stay a provider input (#5).
7. No independent review of this iteration yet. The master schedules it.

The next material gap by this evidence is outside this family. The generate workflow is now general for grouping, collapse, export placement and UnitTest fields. The queue's D2 (a normal-mode parent install replacing a developer-mode dependency) is the strongest open discovery lead.

## 9. Cost

Wall time is about 55 minutes (about 03:10Z to 04:05Z, 2026-11-18). Harness runs took 9 s (native only) and 281 s (full). The mutant pre-check took about 1 minute on 8 threads. Token use is not visible to this session. Both `ipmcov20261117-iter73-*` containers were removed by the harness. No other container or image was touched. Scratch is under `/tmp/ipmcov73-*`.
