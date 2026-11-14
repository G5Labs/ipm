# Trust inventory: developer sync model (module 09)

File: `models/verus/sync_baseline.rs` (17 verified, 0 errors, 0.5 s; 24 controls, each killed in the one lemma it
targets). Verus 0.2026.09.27.3cf1832, `--rlimit 30`, no resource-limit or timeout result. No `assume`, `admit`,
`external_body` or `external` (checked by `scripts/record_proofs.py`: `trusted_items` is empty). Everything below is
what a P or M tag in `ontology/09-sync.md` rests on without being proved. Written for REVIEW.md finding 15; items
S1-S6 moved here from `ns_mapping.TRUST.md`, S7-S9 are new.

What is proved. `sync_lemma_sync`, `sync_lemma_compile`, `sync_lemma_reload` and `sync_lemma_edit`: each L2 step
refines its L1 step and keeps the invariant "an unmarked baseline of an owned file equals its server copy". The L1 of
sync has two parts: (a) loading puts only current disk bytes on the server, and a sync that reports success leaves
every owned file's server copy equal to its disk bytes; (b) the outcome, both directions (REVIEW.md finding 1): the
sync reports success exactly when the module is in developer mode and every step it ran worked (nothing to apply, or
the loads and compile worked and, with -test, the tests passed). The statement-level lemmas (`sync_lemma_*_effects`,
`sync_lemma_first_sync`, `sync_lemma_test_marks`) state per-file effects of one step directly on L2 and need no
premise. The `sync_witness_*` functions are proved defect witnesses (F-SYNC-1, F-SYNC-2) and one positive witness;
they are cited in findings and Notes, never as P on the statement they contradict.

Premises are declared at the top of `sync_baseline.rs` as `// PREMISE <spec fn> = <LABEL>` (`premise_out` and
`premise_out_reload` = OUT, `premise_base` = BASE); the ontology gate requires each P row to name every label its
lemma requires.

Finding 1 check. Before the outcome clause was added, the control "a failing test no longer fails the sync"
(`sync-test-failure-reports-success`) verified with 0 errors (17 verified), i.e. it survived every proof; "a sync
that worked reports failure" was killed only in a concrete witness. With the clause, all five outcome mutants
(`sync-worked-reports-failure`, `sync-test-failure-reports-success`, `sync-compile-failure-reports-success`,
`sync-nothing-to-sync-reports-failure`, `sync-not-dev-reports-success`) are killed in `sync_lemma_sync`. The explorer
checks the same two-way outcome on every explored sync, for both test results and outside developer mode (12,688
checks, 0 violations; M:sync-explore-refinement).

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| S1 | The Verus L2 of `sync_baseline.rs` transcribes Pipeline.Run, FileHash (ComputeChanges passes 1-4, CommitChanges, MarkFailed, MarkTestPending, StampModule) and the lifecycle stamp | Hand-written | `sync_twin.py` (literal transcription) vs the fine machine `sync_fine.py` (five passes with the `seen` set, real bytes and SHA-1) on every 4-command trace from both initial loads: 32,768 commands, 0 mismatches (M:sync-transcription-check); fine machine conforms to 7 sync observations (part B) |
| S2 | A baseline hash is modelled as the bytes (SHA-1 has no collision on the files involved) | Standard | The fine machine computes SHA-1 of the real fixture bytes and matches the hash prefixes IRIS stored (`a5f640cc47`, `1e296a57d9`) |
| S3 | Compiler stand-in: a module compiles iff no server source extends the missing class; a failed compile keeps the old compiled code; in Verus the compile outcome and, on failure, the set of files already loaded are free step arguments | Environment freedom in Verus; stand-in only in the fine machine | O:nc-sync-N18-compile-fail-revert, N20, N22 (part B) |
| S4 | Sync scope: files are never deleted from disk (`-delete` not modelled); module.xml changes, preload warnings, unrouted-file warnings, sync with no module name (SYNC-B43..B46: the command reports success even when a module fails, DEC-SYNC-3; the L1 outcome below is for one module's sync), test execution beyond its result (the test outcome is a free step argument; only test marks and the reported result are modelled) | Scope | Not checked by the model; IPM's Sync test class covers several (T:Test.PM.Integration.Sync) |
| S5 | Sync premises: OUT (defect, F-SYNC-1: no load, compile or reload outside sync that records no baseline puts non-baseline bytes on the server) and BASE (defect, F-SYNC-2: a sync with no baseline runs only when server equals disk) | Not enforced by IPM | Dropping either is killed in its lemma; OUT fails in O:nc-sync-N18-compile-fail-revert, O:nc-nsp-model-sync-reload-revert, O:nc-nsp-model-sync-skipsync-revert; BASE fails in O:nc-sync-N19-skip-sync-first-sync. Explorer: stale server copies after a successful sync occur only on traces where OUT or BASE failed (M:sync-explore-refinement) |
| S6 | Sync exploration bounds | Bounds | One owned class plus one owned, owned-test or unowned file; bytes 1-3; depth 4 from `load -dev` and `load -dev -skip-sync`; 1,248 states, 9,497 transitions |
| S7 | The outcome inputs: `ok` (the loads and compile worked), `tests_ok` (the tests run passed) and `ran` (`sync_ran`: a baseline exists and some owned file is changed, new or marked) are the environment's outcome for the step; L1 (b) is stated relative to them | Verus has no compiler or test runner; the outcome is what the command must report given what happened | The fine machine replays the reported status of N18-N22 and the N20 re-capture (part B); `sync-explore-refinement` checks every explored sync |
| S8 | Outcomes of `compile`, `load -dev` and `reload` are not modelled as reported results (the compile outcome is the step argument itself; `reload` of a module not in developer mode reports success: SYNC-B51, DEC-SYNC-4) | Scope | O:nc-sync-N21-reload-non-dev; fine machine part B |
| S9 | The sync probe harness (`validation/iris/nsp/Uplift/Nsp/Sync.cls`, `Model.cls`, `Fix.cls`) reads IRIS state correctly | Probe code | Raw logs kept (`validation/logs/894-nsp-sync-checks.log`, `86-nsp-model-sync*.log`, `80-nsp-fix-probes.log`); the first N20 capture read the wrong summary line, which the re-capture `nc-nsp-fix-sync-N20-summary` replaced (each summary line read by its own label) |
