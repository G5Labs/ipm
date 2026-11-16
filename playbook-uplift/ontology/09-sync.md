# 09 — Developer sync and file baselines

Module root: SYNC-0

Scope: the developer `sync` command and its file baselines (issue #1224), `compile`, `-skip-sync` on load,
`reload`, failed compile then retry, exact-byte revert, and a later test failure. Baseline: ipm 571e04a1.
Evidence was first by inspection of source and tests at that revision. Native checks N1-N22 (intents/07-09-seed-challenge.md) were then run on IRIS 2026.1 build 234U in container `ipm-uplift-iris-nsp` (N1 in a throwaway fresh container); `O:` tags name their observations (logs `validation/logs/8*-nsp-*.log`), `T:` tags name IPM test classes run here (log 892-nsp-ipm-tests). Model evidence (round 3): `P:` tags name lemmas of `models/verus/sync_baseline.rs` (premises in Notes; trust items in `models/verus/sync_baseline.TRUST.md`), `M:` tags name checks of `models/explorer/nsp/check.py` (`evidence/model-checks-nsp.json`); model probes are `nc-nsp-model-sync-*` (log `validation/logs/86-nsp-model-sync.log`).

## Glossary
| Term | Defined by |
|---|---|
| developer mode | SYNC-D1 |
| baseline | SYNC-D2 |
| tracked folder | SYNC-D3 |
| retry mark | SYNC-D4 |
| test mark | SYNC-D5 |
| server copy | SYNC-D6 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| SYNC-0 | SYS-0 | R | The sync command loads only the module files that changed on disk. This part says how IPM spots those files and what a failure leaves. | I | ipm/src/cls/IPM/Main.cls#L108; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L7 | Seed "Developer sync and retry". Changelog #1117, #1290. |
| SYNC-D1 | SYNC-0 | D | **Developer mode** is a way to load a module so its source stays easy to edit and debug. | I | ipm/src/cls/IPM/Main.cls#L306 | `-dev`; the module keeps its root folder. |
| SYNC-D2 | SYNC-0 | D | The **baseline** of a file is a stored hash of its bytes. IPM uses it to spot changes. | I | ipm/src/cls/IPM/Storage/FileHash.cls#L17 | SHA-1, one `%IPM.Storage.FileHash` row per module and path. |
| SYNC-D3 | SYNC-0 | D | A **tracked folder** is a folder that a resource of the module says it owns. | I | ipm/src/cls/IPM/Storage/FileHash.cls#L76 | `GetSyncDirectory()` per resource processor. |
| SYNC-D4 | SYNC-0 | D | A **retry mark** on a file says that its last sync failed. | I | ipm/src/cls/IPM/Storage/FileHash.cls#L20 | `SyncFailed`. |
| SYNC-D5 | SYNC-0 | D | A **test mark** on a test file says it was synced but its tests did not run. | I | ipm/src/cls/IPM/Storage/FileHash.cls#L26 | `TestPending`. |
| SYNC-D6 | SYNC-0 | D | The **server copy** of a file is the code that IRIS holds for it. | I | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L212 | |
| SYNC-B1 | SYNC-0 | B | sync works only on a module in developer mode. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L596; T:Test.PM.Integration.Sync; P:sync_lemma_sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L29 | P: the two-way L1 outcome of sync_lemma_sync says a sync of a module not in developer mode reports failure (it changes nothing: sync_state returns the module as it was). Requires premise BASE, which holds trivially outside developer mode. |
| SYNC-B2 | SYNC-B1 | B | sync of a module that is not installed fails. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L584; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/Storage/Module.cls#L461 | |
| SYNC-B3 | SYNC-0 | B | A good load or compile in developer mode records a baseline. It does so for each file in a tracked folder. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L60; T:Test.PM.Integration.Sync; P:sync_lemma_compile_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Lifecycle/Base.cls#L1235; ipm/src/cls/IPM/Storage/FileHash.cls#L103 | Code files (cls, inc, mac, int, xml, rtn) plus module.xml. Model: models/verus/sync_baseline.rs (hash modelled as the bytes; SHA-1 trusted collision-free on these files). |
| SYNC-B4 | SYNC-B3 | B | IPM records the baseline only after the compile works. | I; P:sync_lemma_compile_effects; M:nsp-conf-nc-sync-N18-compile-fail-revert | ipm/src/cls/IPM/Lifecycle/Base.cls#L1238 | The stamp is at the end of `%Compile`; a thrown compile error skips it. |
| SYNC-B5 | SYNC-B3 | B | If the baseline cannot be saved, IPM warns. The compile still counts as a success. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1239 | |
| SYNC-B6 | SYNC-B3 | B | With -skip-sync, a load records no baseline. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L76; T:Test.PM.Integration.Sync; O:nc-sync-N19-skip-sync-first-sync; P:sync_lemma_compile_effects; O:nc-nsp-model-sync-skipsync-revert; M:nsp-conf-nc-sync-N19-skip-sync-first-sync | ipm/src/cls/IPM/Main.cls#L315; ipm/CHANGELOG.md#L12 | Also on install, update, reinstall and module actions. Native N19: no baseline rows after `load -dev -skip-sync`. Also over an existing baseline: the earlier hash stays (nc-nsp-model-sync-skipsync-revert). |
| SYNC-B7 | SYNC-0 | B | sync only looks in tracked folders. It ignores all other files. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L215; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L56 | E.g. `node_modules/` is never walked. |
| SYNC-B8 | SYNC-0 | B | sync finds each file whose bytes differ from its baseline. It loads and compiles those files. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L156; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L169; T:Test.PM.Integration.Sync; P:sync_lemma_success_effects; M:sync-explore-refinement; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Storage/FileHash.cls#L228; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L481 | UDL and XML class files alike. P: on a sync that works, every changed owned file is loaded and gets its new baseline; the failure path is SYNC-B16. |
| SYNC-B9 | SYNC-B8 | B | It then compiles the whole module, so code that uses a changed file is built again too. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L193; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L135; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L742 | Flags `cku`; superclass and include edits reach their users. |
| SYNC-B10 | SYNC-B8 | B | When nothing changed, sync says there is nothing to sync. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L103; T:Test.PM.Integration.Sync; O:nc-sync-N22-retry-mark-after-compile; M:nsp-conf-nc-sync-N22-retry-mark-after-compile; M:sync-transcription-check | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L92 |  |
| SYNC-B11 | SYNC-B8 | B | A new file in a tracked folder is loaded too. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L861; T:Test.PM.Integration.Sync; P:sync_lemma_success_effects; M:sync-explore-refinement | ipm/src/cls/IPM/Storage/FileHash.cls#L260 | A file with no baseline counts as changed. P: an owned file with no baseline row is detected and loaded. |
| SYNC-B12 | SYNC-B8 | B | sync also works on the members of a package resource in the folder it names. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L833; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/Storage/FileHash.cls#L80 | |
| SYNC-B13 | SYNC-B8 | B | A changed file that no resource owns is not synced. sync warns and finds it again next time. | I | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L123; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L448 | Its hash is not saved. |
| SYNC-B14 | SYNC-B8 | B | When a sync works, IPM saves the new baselines. | I; O:nc-sync-N22-retry-mark-after-compile; P:sync_lemma_success_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L146 | `CommitChanges`, before any test runs. Native N22: the stored hash changes to the new bytes after the sync. |
| SYNC-B15 | SYNC-0 | B | sync applies each change made on disk since the last load or sync. | F:F-SYNC-1; M:sync-explore-witness-F-SYNC-1; O!:nc-nsp-model-sync-reload-revert; O!:nc-nsp-model-sync-skipsync-revert | ipm/src/cls/IPM/Main.cls#L111; ipm/CHANGELOG.md#L53 | Help: "Detects files changed on disk since the last load/sync". The code compares with the last saved baseline. Native N18 confirms F-SYNC-1. Named exception: SYNC-B40, SYNC-B41 (no baseline; intended, DEC-SYNC-2). sync_lemma_sync, sync_lemma_compile and sync_lemma_reload prove 'a sync that reports success leaves every owned file's server copy equal to its disk bytes' only under the invariant 'an unmarked baseline equals the server copy' and the defect premise OUT (F-SYNC-1) and premise BASE (the no-baseline branch, now the named exception SYNC-B40); those premises leave out exactly the defect cases, so this is not a proof of the statement (no P tag). O! marks the two observations that show the defect. Witnesses: sync_witness_failed_compile_then_revert, sync_witness_reload_then_revert, sync_witness_skip_sync_load_then_revert (F-SYNC-1); sync_witness_first_sync_absorbs_edit shows the exception SYNC-B41. The explorer finds stale server copies after a successful sync only on traces where OUT or BASE failed. Intent: DEC-SYNC-2 resolved (evidence), decisions/research/nsp.md. |
| SYNC-B16 | SYNC-0 | B | If a sync fails, IPM saves no new baselines. The changed files are found again next time. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L650; T:Test.PM.Integration.Sync; O:nc-sync-N22-retry-mark-after-compile; P:sync_lemma_failed_sync_effects; M:sync-explore-refinement; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L146; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L210 | Native N22: after the failed sync the stored hash is still the load-time one. |
| SYNC-B17 | SYNC-B16 | B | IPM also puts a retry mark on each file that the failed sync was applying. | I; O:nc-sync-N22-retry-mark-after-compile; O:nc-sync-N20-sync-all-one-fails; P:sync_lemma_failed_sync_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile; M:nsp-conf-nc-sync-N20-sync-all-one-fails | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L217; ipm/src/cls/IPM/Storage/FileHash.cls#L420 | Only files that already have a baseline; a file with none already reads as changed. Native N20, N22: SyncFailed=1 after a failed sync. |
| SYNC-B18 | SYNC-B17 | B | A file with a retry mark is synced again, even when its bytes match the baseline. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L695; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L700; T:Test.PM.Integration.Sync; P:sync_lemma_success_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Storage/FileHash.cls#L273 | Issue #1224 exact-byte revert. Seed confirmed. Positive witness: sync_witness_failed_sync_is_retried. |
| SYNC-B19 | SYNC-B17 | B | A sync that works clears the retry mark. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L708; T:Test.PM.Integration.Sync; O:nc-sync-N22-retry-mark-after-compile; P:sync_lemma_success_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Storage/FileHash.cls#L386 |  |
| SYNC-B20 | SYNC-B17 | B | A load or compile that works keeps any retry or test mark. The next sync handles that file. | I; O:nc-sync-N22-retry-mark-after-compile; P:sync_lemma_compile_effects; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Storage/FileHash.cls#L154 | `StampOneFileWithHash` sets only `Hash`. Harmless extra work; not tested. Native N22: after a good `compile` the row has the new hash and still SyncFailed=1; the next sync says "Retrying 1 file(s)" and lists the file. |
| SYNC-B21 | SYNC-B17 | B | If IPM cannot set a retry mark, it warns. It still reports the first error. | I | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L218 | |
| SYNC-B22 | SYNC-B17 | B | A load or compile outside sync that fails marks the files it put on the server. The next sync loads them again. | I; F:F-SYNC-1; O!:nc-sync-N18-compile-fail-revert | ipm/src/cls/IPM/Lifecycle/Base.cls#L1235; ipm/src/cls/IPM/Lifecycle/Base.cls#L1245 | Code: retry marks are set only by a failed sync (Pipeline.cls#L210-L223), so after a failed `compile`, `load -dev` or `reload` an exact revert is not seen (F-SYNC-1). Native N18: a failed `compile` leaves SyncFailed=0 and the old hash. Current behaviour, not tags: sync_lemma_compile_effects (a load or compile never sets or clears a retry mark; no premises) and M:nsp-conf-nc-sync-N18-compile-fail-revert. Intent: issue #1224 (a reverted file is reloaded); sync help 'since the last load/sync'. Intent: DEC-SYNC-1 resolved (assumption), decisions/research/nsp.md. |
| SYNC-B23 | SYNC-0 | B | Without -test, sync loads changed test files but runs no tests. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L293; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L206 | |
| SYNC-B24 | SYNC-B23 | B | IPM then puts a test mark on those files and says so. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L320; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L158; ipm/src/cls/IPM/General/Sync/Pipeline.cls#L195 | |
| SYNC-B25 | SYNC-B23 | B | A sync without -test leaves each test mark in place. | I; P:sync_lemma_test_marks | ipm/src/cls/IPM/Storage/FileHash.cls#L298 | Pass 4 runs only with -test. |
| SYNC-B26 | SYNC-B23 | B | With -test, sync runs the tests in changed test files after it saves the files. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L305; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L205 | |
| SYNC-B27 | SYNC-B26 | B | Files with a test mark get their tests run once. Then the mark is cleared. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L329; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L337; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/Storage/FileHash.cls#L295; ipm/src/cls/IPM/Storage/FileHash.cls#L388 | |
| SYNC-B28 | SYNC-B26 | B | Changed test classes run in one batch. Each class runs under one resource only. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L380; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L434; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L611 | |
| SYNC-B29 | SYNC-B26 | B | Only the test resource that owns a changed class runs. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L398; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L628 | |
| SYNC-B30 | SYNC-B26 | B | Tests for the verify phase are skipped and listed. | I | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L648 | "use 'verify' to run". |
| SYNC-B31 | SYNC-B26 | B | A failing test fails the sync command. The files stay synced and get no retry mark. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L723; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L724; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L732; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L733; T:Test.PM.Integration.Sync; P:sync_lemma_sync; P:sync_lemma_success_effects | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L150 | Seed "later test failure" confirmed. P: sync_lemma_sync's L1 outcome (both directions) says a sync that applied changes reports failure when a test it ran failed, and success when the loads, compile and tests all worked (premise BASE named; it holds whenever a baseline exists, which is the case here). sync_lemma_success_effects says the files of a sync whose load and compile worked get their baseline and lose the retry mark, whatever the tests do. Controls sync-test-failure-reports-success and sync-worked-reports-failure are killed in sync_lemma_sync. |
| SYNC-B32 | SYNC-0 | B | Without -delete, sync leaves the server copy of a deleted file in place. It warns about the file. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L228; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L94 | |
| SYNC-B33 | SYNC-B32 | B | With -delete, sync removes the server copy and compiles the module again. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L233; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L246; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L874; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L140 | |
| SYNC-B34 | SYNC-B33 | B | If that compile breaks code that used the deleted file, the sync fails. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L261; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L143 | |
| SYNC-B35 | SYNC-B32 | B | Only sync with -delete removes the server copy of a file gone from disk. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L615 | Reload has no delete step. For a single-file resource, the missing file makes Reload fail (Document.cls#L180). |
| SYNC-B36 | SYNC-0 | B | When module.xml changes, sync reloads the manifest. It warns that a full reload may be needed. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L277; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L230 | |
| SYNC-B37 | SYNC-B36 | B | A resource that the new manifest adds is synced in the same run. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L621; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L50 | |
| SYNC-B38 | SYNC-B36 | B | Edits to module.xml and to a class at the same time are both synced. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L754; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L50 | |
| SYNC-B39 | SYNC-0 | B | A changed preload file is synced. IPM warns that its setup steps did not run again. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L777; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L795; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L127 | No warning when no preload file changed (Sync.cls#L810). |
| SYNC-B40 | SYNC-0 | N | With no baseline at all, the first sync only records one. It loads nothing. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L82; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L123; T:Test.PM.Integration.Sync; O:nc-sync-N19-skip-sync-first-sync; P:sync_lemma_first_sync; M:nsp-conf-nc-sync-N19-skip-sync-first-sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L43 | "Baseline established. Run sync again to detect changes." Meant for modules loaded before sync existed; `-skip-sync` makes it the normal path. Native N19: first sync says "Baseline established" and the class still returns "original". Intended also after `-skip-sync` (PR #1291 test Sync.cls#L72; PR #1205). Intent: DEC-SYNC-2 resolved (evidence), decisions/research/nsp.md. |
| SYNC-B41 | SYNC-B40 | N | An edit made before that first sync goes into the baseline. It does not reach the server copy. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L133; T:Test.PM.Integration.Sync; O:nc-sync-N19-skip-sync-first-sync; P:sync_lemma_first_sync; M:nsp-conf-nc-sync-N19-skip-sync-first-sync | ipm/src/cls/IPM/General/Sync/Pipeline.cls#L45 | The test asserts the later "Nothing to sync"; that the server copy stays old is inferred. See F-SYNC-2. Native N19: the edit made before the first sync never reaches the server copy; the second sync says "Nothing to sync". Proof: sync_witness_first_sync_absorbs_edit. Intended also after `-skip-sync` (PR #1291 test Sync.cls#L72; PR #1205). Intent: DEC-SYNC-2 resolved (evidence), decisions/research/nsp.md. |
| SYNC-B42 | SYNC-B40 | B | Edits made after that first sync are found as usual. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L90; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L142; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/Storage/FileHash.cls#L228 | |
| SYNC-B52 | SYNC-B40 | B | The help for sync says that the first sync after a -skip-sync load loads nothing. | I; F:F-SYNC-2 | ipm/src/cls/IPM/Main.cls#L109; ipm/src/cls/IPM/Main.cls#L315 | The general sync help ('changes since the last load/sync') and the wiki do not warn that a -skip-sync load is not a baseline; the wiki says so only for pre-0.10.10 modules (F-SYNC-2, documentation). Intent: DEC-SYNC-2 resolved (evidence), decisions/research/nsp.md. |
| SYNC-B43 | SYNC-0 | B | sync with no module name syncs each module in developer mode. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L447; T:Test.PM.Integration.Sync; O:nc-sync-N20-sync-all-one-fails | ipm/src/cls/IPM/Main.cls#L2383 | |
| SYNC-B44 | SYNC-B43 | B | A module is synced after the modules it needs. | I; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L509; X:tests/integration_tests/Test/PM/Integration/Sync.cls#L510; T:Test.PM.Integration.Sync | ipm/src/cls/IPM/Utils/Module.cls#L1504 | |
| SYNC-B45 | SYNC-B43 | B | If one module fails, sync goes on to the rest. Its summary names the failed module. | I; O:nc-nsp-fix-sync-N20-summary; O:nc-sync-N20-sync-all-one-fails; M:nsp-conf-nc-nsp-fix-sync-N20-summary | ipm/src/cls/IPM/Main.cls#L2396; ipm/src/cls/IPM/General/Sync/Summary.cls#L155 | A fatal test failure is shown as a test tally, not as an error. Native N20: summary lists "sync-test ERROR" and "Errors: sync-test"; sync-dep-c was still synced. The first capture's summaryLine (nc-sync-N20-sync-all-one-fails) took the wrong output line ("[USER/sync-test]Sync START"); the summary values here come from the re-capture nc-nsp-fix-sync-N20-summary (log 80-nsp-fix-probes): "Modules checked: 2 (sync-test, sync-dep-c)", "sync-test ERROR", "Errors: sync-test"; sync-dep-c reads c-edited. |
| SYNC-B46 | SYNC-B45 | B | The command still ends as a success when a module failed. | I; O:nc-sync-N20-sync-all-one-fails; O:nc-nsp-fix-sync-N20-summary; M:nsp-conf-nc-sync-N20-sync-all-one-fails | ipm/src/cls/IPM/Main.cls#L2398; PR #1205 | Errors are displayed, never thrown. With a module name, a failure is thrown (L2381). Native N20: `sync` returned OK; `sync sync-test` returned the error. Re-capture nc-nsp-fix-sync-N20-summary: `sync` again returned OK with "Errors: sync-test" in its summary. PR #1205: with no module name, Main.Sync 'collects failures without throwing'; that says the loop goes on, not that the final status is OK, so the OK status is an assumption. Help and wiki do not say it (doc note). Intent: DEC-SYNC-3 resolved (assumption), decisions/research/nsp.md. |
| SYNC-B47 | SYNC-0 | B | compile in developer mode loads the files of the module from disk. Then it compiles them. | I; O:nc-sync-N22-retry-mark-after-compile; M:nsp-conf-nc-sync-N22-retry-mark-after-compile | ipm/src/cls/IPM/Lifecycle/Base.cls#L174; ipm/src/cls/IPM/Lifecycle/Base.cls#L615; ipm/src/cls/IPM/Lifecycle/Base.cls#L1052 | Phases Initialize, Reload, Validate, Compile. Native N22: `compile` picked up the edited file (GetValue "fixed"). |
| SYNC-B48 | SYNC-B47 | B | When such a compile fails, the loaded files stay on the server. | I; O:nc-sync-N18-compile-fail-revert; P:sync_lemma_compile_effects; M:nsp-conf-nc-sync-N18-compile-fail-revert | ipm/src/cls/IPM/Main.cls#L306; ipm/src/cls/IPM/Utils/Module.cls#L1335 | No transaction wraps reload and compile of a dev-mode module. Native N18: after the failed compile the server class has the broken superclass and is not compiled. |
| SYNC-B49 | SYNC-0 | B | reload loads the manifest and files from disk. It does not compile them. | I; O:nc-sync-N21-reload-non-dev; O:nc-nsp-model-sync-reload-revert; M:nsp-conf-nc-nsp-model-sync-reload-revert | ipm/src/cls/IPM/Main.cls#L42; ipm/src/cls/IPM/Lifecycle/Base.cls#L171 |  |
| SYNC-B50 | SYNC-B49 | B | The reload command saves no baseline. | I; P:sync_lemma_reload_effects; O:nc-nsp-model-sync-reload-revert; M:nsp-conf-nc-nsp-model-sync-reload-revert | ipm/src/cls/IPM/Lifecycle/Base.cls#L1235 | Only Compile stamps. A later sync sees the files as changed and does them again. Native: after reload and an exact revert, sync says Nothing to sync while the server keeps the reloaded source (F-SYNC-1, reload case). |
| SYNC-B51 | SYNC-B49 | B | For a module not in developer mode, reload loads nothing. It tells the operator so. | I; F:F-SYNC-3; O!:nc-sync-N21-reload-non-dev | ipm/src/cls/IPM/Lifecycle/Base.cls#L627; ipm/src/cls/IPM/Lifecycle/Base.cls#L634 | Code: 'For now, no-op if no root is configured' (Base.cls#L633-L634) and a silent 'Reload START ... SUCCESS' (F-SYNC-3). The phase stays quiet inside larger phase lists (compile); the standalone command must say nothing was loaded. Native N21: status OK, server source unchanged. Current behaviour, not tags: M:nsp-conf-nc-sync-N21-reload-non-dev; sync_lemma_reload_effects proves reload changes nothing outside developer mode. A failing standalone command (as sync does for a non-dev module, Pipeline.cls#L29-L31) also fits. Intent: DEC-SYNC-4 resolved (assumption), decisions/research/nsp.md. |

## States
| ID | State |
|---|---|
| S1 | The file on disk matches its baseline and its server copy. |
| S2 | The file was edited on disk after its baseline was saved. |
| S3 | The last sync of the file failed, so it has a retry mark; its bytes may again match the baseline. |
| S4 | The file is a test file that has a test mark. |
| S5 | The module has no baseline, for example after a load with -skip-sync; the file may have been edited since. |
| S6 | A load or compile outside sync put edited bytes on the server and failed; the file was then put back to its baseline bytes. |
| S7 | The file was deleted from disk. |

## Commands
| ID | Command |
|---|---|
| K1 | `sync <module>` |
| K2 | `sync <module> -test` |
| K3 | `sync <module> -delete` |
| K4 | `sync` (all developer-mode modules) |
| K5 | `compile <module>` (developer mode) |
| K6 | `load -dev <module root>` again |
| K7 | `reload <module>` |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| SYNC-C1 | S1 | K1 | SYNC-B10 | |
| SYNC-C2 | S1 | K2 | SYNC-B10 | |
| SYNC-C3 | S1 | K3 | SYNC-B10 | |
| SYNC-C4 | S1 | K4 | SYNC-B43, SYNC-B10 | |
| SYNC-C5 | S1 | K5 | SYNC-B47, SYNC-B3 | |
| SYNC-C6 | S1 | K6 | SYNC-B3 | |
| SYNC-C7 | S1 | K7 | SYNC-B49, SYNC-B50 | |
| SYNC-C8 | S2 | K1 | SYNC-B8, SYNC-B9, SYNC-B14 | |
| SYNC-C9 | S2 | K2 | SYNC-B8, SYNC-B26 | |
| SYNC-C10 | S2 | K3 | SYNC-B8, SYNC-B14 | |
| SYNC-C11 | S2 | K4 | SYNC-B43, SYNC-B8 | |
| SYNC-C12 | S2 | K5 | SYNC-B47, SYNC-B3, SYNC-B48 | |
| SYNC-C13 | S2 | K6 | SYNC-B3 | |
| SYNC-C14 | S2 | K7 | SYNC-B49, SYNC-B50 | |
| SYNC-C15 | S3 | K1 | SYNC-B18, SYNC-B19 | |
| SYNC-C16 | S3 | K2 | SYNC-B18, SYNC-B26 | |
| SYNC-C17 | S3 | K3 | SYNC-B18 | |
| SYNC-C18 | S3 | K4 | SYNC-B43, SYNC-B18 | |
| SYNC-C19 | S3 | K5 | SYNC-B47, SYNC-B20 | |
| SYNC-C20 | S3 | K6 | SYNC-B20 | |
| SYNC-C21 | S3 | K7 | SYNC-B49, SYNC-B50 | |
| SYNC-C22 | S4 | K1 | SYNC-B25 | |
| SYNC-C23 | S4 | K2 | SYNC-B27 | |
| SYNC-C24 | S4 | K3 | SYNC-B25 | |
| SYNC-C25 | S4 | K4 | SYNC-B43, SYNC-B25 | |
| SYNC-C26 | S4 | K5 | SYNC-B47, SYNC-B20 | |
| SYNC-C27 | S4 | K6 | SYNC-B20 | |
| SYNC-C28 | S4 | K7 | SYNC-B49, SYNC-B50 | |
| SYNC-C29 | S5 | K1 | SYNC-B40, SYNC-B41 |  |
| SYNC-C30 | S5 | K2 | SYNC-B40, SYNC-B41 |  |
| SYNC-C31 | S5 | K3 | SYNC-B40, SYNC-B41 |  |
| SYNC-C32 | S5 | K4 | SYNC-B43, SYNC-B40 |  |
| SYNC-C33 | S5 | K5 | SYNC-B47, SYNC-B3 | |
| SYNC-C34 | S5 | K6 | SYNC-B3, SYNC-B6 | |
| SYNC-C35 | S5 | K7 | SYNC-B49, SYNC-B50 | |
| SYNC-C36 | S6 | K1 | SYNC-B22, SYNC-B18 |  |
| SYNC-C37 | S6 | K2 | SYNC-B22, SYNC-B18 |  |
| SYNC-C38 | S6 | K3 | SYNC-B22, SYNC-B18 |  |
| SYNC-C39 | S6 | K4 | SYNC-B22, SYNC-B43, SYNC-B18 |  |
| SYNC-C40 | S6 | K5 | SYNC-B47, SYNC-B3 | |
| SYNC-C41 | S6 | K6 | SYNC-B3 | |
| SYNC-C42 | S6 | K7 | SYNC-B49, SYNC-B50 |  |
| SYNC-C43 | S7 | K1 | SYNC-B32 | |
| SYNC-C44 | S7 | K2 | SYNC-B32 | |
| SYNC-C45 | S7 | K3 | SYNC-B33, SYNC-B34 | |
| SYNC-C46 | S7 | K4 | SYNC-B43, SYNC-B32 | |
| SYNC-C47 | S7 | K5 | SYNC-B47, SYNC-B35 | |
| SYNC-C48 | S7 | K6 | SYNC-B35 | |
| SYNC-C49 | S7 | K7 | SYNC-B35 | |
