# Round 1 ontology repairs (A3)

Repairs the "A3" items of [dispositions.md](dispositions.md) for the findings in [review-ontology.md](review-ontology.md). No ID was renamed or removed. One block was added (`history.locked.unfinished`, Q-21). Observed behavior, defects and B2 run IDs are in metadata only. Checker: 18 files, 147 blocks (122 I, 5 D, 20 Q), 0 issues, 0 writing warnings.

| Finding | Changed IDs | What changed |
|---|---|---|
| R1-ONT-01 | `history.survives-rollback`, `history.dependencies.parent-rollback`, `history.committed`, `history.committed.nested`, `history.survives-rollback.temp-copy`, `history.dependencies` | Survival is now intent for any rollback that undoes the action, including a parent's rollback of a dependency. Source is issue #366, and provenance is now `human-verified`. Q-06 is narrowed to the dependency record's `Success`, with provisional answer "its own status". `history.committed`: a dependency undone by its parent reads uncommitted. Q-05 now covers only caller-held transactions: provisionally the record survives (#366) and `Committed` is exact only outside a caller transaction. The D `temp-copy` is limited to top-level actions started outside any transaction. B2 D1 is recorded as a confirmed defect with runs. |
| R1-ONT-02 (as modified) | `history.record`, `history.record.abnormal-end`, `history.locked`, new `history.locked.unfinished` | The placeholder version is now permitted, not required. Q-08's provisional answer now follows #1237 and the `wiki/09` recipe: `Committed` reflects kept changes even on an unfinished record. "Unknown outcome" is the alternative. New Q-21 asks whether an unfinished record may be finalized, with provisional answer "no" by the page's definition. The OBS-H6 notes are replaced by B2's Terminate and SIGKILL split (C1, C2, C3, C4R; N-P10 refuted, N-P18). |
| R1-ONT-03 | `history.fields` | `Package` is empty only if the action failed before any name was known. A refusal after reading the manifest records the name. Deviation D9 is noted with B2 g3, g6, g7 and R4 runs. |
| R1-ONT-04 (metadata) | `load.path` | Cites `280c8d68` and CHANGELOG `#474` for `.tar.gz` support, `f83b207e` (PR #887) for the single-piece check, and help `Main.cls:282`. Status: not run, queued for B2 phase 2. |
| R1-ONT-05 | `dev-mode.overwrite-guard.force` | The reason now follows the code and the `-force` help, and flags the conflict with maintainer-authored `wiki/00` L129 for a maintainer. The guard has not checked `-force` since `d541c5b4`. The wiki is no longer called an error. |
| R1-ONT-06 | `history.record.unlogged-refusals` | Provisional answer: record argument refusals. Alternative: they are not attempts. |
| R1-ONT-07 | `history.fields.dependency-source` | Provisional answer: keep the `load` label (`TestWithDependencies` L752-770) with the stored source and the wiki's `n/a` display rule. The #366 wording "install" is the alternative. |
| R1-ONT-08 | `load.path.archive` | Added `test-coverage` for `.tgz` only (`FileBinaryTar` `TestLoadDestinationDirectory`, run `182337Z-suite-verify-FileBinaryTar`). Removed "No test loads an archive". |
| R1-ONT-09 | `load.outcome` | Table row now says "No module data". |
| R1-ONT-10 | `history.read.filter.invalid-column`, `history.read.filter` | The Q now covers query errors in `find`, with provisional answer "report the error". B2 E3 and E7 and D2 are cited. |
| R1-ONT-11 | `history.delete`, `history.read.filter`, `history.read.find` | Test notes now say "partial" and what the test does not exercise. |
| R1-ONT-12 | Every load and history block, plus `transaction`, `transaction.opt-out`, `dev-mode.*`, `settings`, `update.refusal` | "Pending stage B" and "stage B to confirm" are replaced by B2 status with run IDs: confirmed (D1-D6, D9), conforms, consistent with the provisional answer (Q-01, Q-02), or not run (queued for phase 2). Page preambles define the run-ID short form and link the B2 report. |
| R1-ONT-14 | `load.outcome` | Metadata records the refined P-6: after SIGKILL, rollback waits for slot cleanup (105 s, 226 s, about 10 min), and other processes can read the uncommitted writes until then. |

Coverage map: header counts, rows INT-HIST-02, -03, -04, -08, -13 and -14, the question table (Q-05, Q-06, Q-08, Q-09, Q-10, Q-15, new Q-21), section 4 (confirmed defects) and a new section 9 with native status per owner block.

For B1, these provisional or intent changes may move L1 expectations:
- Dependency-record survival is now I (R1-ONT-01).
- Q-08's default is now "reflects kept changes" (R1-ONT-02).
- Q-21's default is "no finalize without end" (R1-ONT-02).
- Q-09's default is now "record argument refusals" (R1-ONT-06).
- Q-15's default is now "keep `load`" (R1-ONT-07).
- `Package` is set after the manifest is read (R1-ONT-03).
