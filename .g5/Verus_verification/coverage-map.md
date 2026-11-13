# Verification coverage map: IPM load and history

Maps each in-scope ontology block to its formal support, its connection to the product and what another change may rely on. The capability-level map of all of IPM (stage A) is `.g5/uplift/ipm-verus-20261113/A/coverage-map.md`.

## Abbreviations and conventions

- **Formal support:**
  - Theorems are in `crates/ipm-load-history/src/proofs.rs` and case lemmas in `cases.rs`.
  - The domain is every well-formed configuration: all transaction modes, dev flags, `-force`, installed states, a caller transaction or none, up to two leaf dependencies, and all modeled faults.
  - Theorems are symbolic over that domain. Explorer runs are bounded and exhaustive within the single-fault domain listed in `B/model-notes.md` section 3.
  - Final proof receipt: `results/verus/20261113T194721Z-asbuilt-proofs-l1rev1` (248 verified, 0 errors, no `assume`, `admit` or `external_body`).
- **Product connection:**
  - Bindings are in `crates/ipm-load-history/BINDINGS.md`.
  - Native runs are under `results/native/`. "Replay" means B2's replay of `cases/cases.json` (`B/native-replay.md`).
- **Reliance:**
  - **supported**: within the stated scope.
  - **conditional**: on a named premise or open question.
  - **confirmed violation**: the product contradicts the clause, with a defect ID from `bugs_issues.md`.
  - **unresolved authority**: a human decision is needed.
  - **unestablished**: no formal or native evidence.
- Premises P-01..P-11 used by the theorems are CONFIRMED natively (`premises.md`), except P-06c (unverified) and P-09/P-10 (partial). P-12 (IRISTEMP at restart) is not run.

## Package loading

| Ontology block | Formal support | Product connection | Reliance |
|---|---|---|---|
| `load` (load from a path, with dependencies) | L1-STATUS and L1-NAMESPACE success. `case_c01_conforms`. Explorer L1 runs over the conforming domain pass | Bound steps `Main:Load`, `LoadNewModule`, `LoadModuleReference`. Native A0 and replay C01. Suites LoadModuleWithDeps, InstallModule, History | supported (≤2 leaf dependencies, no nested dependencies) |
| `load.path`, `load.path.archive` | Not modeled (path validity is one input bit) | `.tgz`: suite FileBinaryTar `TestLoadDestinationDirectory` (PASS). `.tar.gz`: B2 stage C case (pending) | `.tgz` supported by test. `.tar.gz` suspected regression (`f83b207e`), see `bugs_issues.md` |
| `load.core` (install, load and update share one path) | Only `load` is modeled | Install and update paths: product suites only | unestablished for install and update beyond the suites |
| `load.options` | `-dev`, `-nodev`, `-force` modeled | Native g4, g5, g8, g9 | supported for those flags. `-env`, `-bypass-py-deps`, `-create-lockfile`, `-skip-sync` unestablished |
| `load.refusal` (a refusal writes no module data) | L1-NAMESPACE refusals. Controls 08 and 09 detected | Native B1, g3, g6, g7. Replay C11-C18 | supported for downgrade and update-required in every mode. Requirements: confirmed violation D4 (decision Q-17). Manifest under NoTransaction: `Committed=1` with nothing kept (V4; native pending) |
| `load.refusal.path` | Path-invalid input | B2 stage C case (pending) | pending |
| `load.refusal.manifest` | Unparseable, no-document and validation-refused manifests modeled. V4 | Suite History `TestDevMode` (malformed manifest, PASS). Replay C08/C09 | database outcome supported. `Committed` exactness confirmed violation in model (V4); native pending |
| `load.refusal.downgrade` | `case_c11_downgrade_refused`, control 08 | Native g6, g7 | supported. Record names no package (D9) |
| `load.refusal.update-required` (+ Q dev-bypass) | `case_c15_update_refused`, control 09, Q-02 parameter | Native g3, g4, g5 | supported. Q-02 provisional matches the product (unresolved authority for the docs) |
| `load.refusal.requirements` (+ Q timing) | V3, `case_c23_sysreq_dev_keeps_manifest` | Native B1-B4, `repro/sysreq-dev-manifest-left.cos` | confirmed violation D4 under Q-17 provisional (decision pending) |
| `load.same-version` (D) | Modeled installed-equal state | Native g2 | supported |
| `load.outcome` (what a failure keeps, by mode) | L1-NAMESPACE failures, `theorem_commit_sound`, explorer invariants for default, NoTransaction and NoJournal. Process ends: kill-like and terminate-like | Native A1, A5, C1-C4R, D0. Replay T and K variants | supported for database state within scope, conditional on P-05 and P-06b. Non-database effects (Q-04) unestablished. Error after the final commit reports failure with the module installed (V6, native pending) |
| `load.outcome.dev`, `.early-failure` | `case_c29_error_after_commit`, controls 01 and 04 | Native A2, A5, C2. Suites History `TestDevModeRollbackNotCommitted`, `TestDevModeFailureAfterCommit` | supported |
| `load.dependencies` | Up to two leaf dependencies. V1, V2 | Native A0-A5. Suite LoadModuleWithDeps | module state supported. Dependency history: confirmed violation D1 |
| `load.dependencies.local-repo` | Not modeled | Native A0 (dependencies from `.modules`) | supported by observation only |
| `load.write-check` | Not modeled | None | unestablished |

## Transactions and developer mode

| Ontology block | Formal support | Product connection | Reliance |
|---|---|---|---|
| `transaction` (one transaction, rolled back on failure) | `theorem_tlevel`. Rollback loop, control 04 detected | Premises N-P01..N-P03 | supported, conditional on P-01 |
| `transaction.rollback-scope` (Q-04) | Not modeled | None | unresolved authority |
| `transaction.opt-out` (NoTransaction, NoJournal) | Explorer invariants per mode. P-03 | Native B3, B4, N-P07 | supported |
| `transaction.flag-sources` | Not modeled | None (`^%` placement of default modifiers is a B2 stage C case) | unestablished |
| `dev-mode.keep-on-failure` | C1/C2 commit steps, controls 01 and 02 | Native A5, C2 | supported |
| `dev-mode.not-propagated` (+ D local-snapshot) | Dependencies are non-dev except a snapshot under `-dev` | Native A4. Suite LoadModuleWithDeps | supported |
| `dev-mode.overwrite-guard` (+ Q force) | Not modeled (install only) | None | unestablished. Q-03 unresolved authority |
| `dev-mode.sticky` (Q-01) | Q-01 parameter | Native g8, g9 | provisional answer matches the product |

## Transaction history

| Ontology block | Formal support | Product connection | Reliance |
|---|---|---|---|
| `history.actions` | Load actions only | Suite History (load, install, uninstall, update) | load supported. Others by suite only |
| `history.record` (record from start; unfinished record lists its phases) | `theorem_top_record_durable`, `theorem_top_record_complete` (no caller transaction) | Native C1-C4R, replay T and K variants | record existence supported for top-level actions. Content of unfinished records: confirmed violations (no phases and no package after a kill, C1/C4R) |
| `history.record.abnormal-end` (Q-08) | Q-08 parameter. V9 | Native C4R | confirmed violation under the provisional answer (decision pending) |
| `history.record.unlogged-refusals` (Q-09) | Not modeled (update, uninstall, reinstall) | None | unresolved authority |
| `history.survives-rollback` | `theorem_top_record_durable`, `theorem_r1`. V1, V2 | Native A1-A3, `repro/dep-history-lost.cos` | top-level supported. Dependencies: confirmed violation D1 |
| `history.survives-rollback.temp-copy` (D) | P-02, P-06a, P-07. Control 06 detected | N-P04, N-P09, N-P14 | supported for top-level actions at `$TLEVEL` 0 |
| `history.fields` | Success equals the returned status (`theorem_top_record_complete`). Package rule: V7 | Native g3, R1-R4 | Success supported. Package on refusals: confirmed violation D9 |
| `history.fields.dependency-source` (Q-15) | Not modeled | Native A0 (`Action=load`, source `ipm-temp-modules-1`) | unresolved authority |
| `history.phases` (+ Q returned errors) | V5, `case_c28_phase_left_open` | `repro/activate-invoke-phase-open.cos` | confirmed violation D6 under Q-14 provisional |
| `history.committed` | `theorem_commit_sound` (changed implies `Committed=1`). Controls 01-03 detected. Exactness: V4 | Native A1, A5, B3, B4, g3. Suite History | "changed implies committed" supported. "Committed implies changed" confirmed violation in model (V4) |
| `history.committed.marking` (D) | SetCommitted sites bound | BINDINGS | supported |
| `history.committed.uninstall` | Not modeled | Suite History | by suite only |
| `history.committed.vs-success` | Covered by the cases with failure plus commit | Native A5, B3, B4 | supported |
| `history.committed.nested` (Q-05, caller transaction) | Q-05 and Q-06b parameters. V8 | Native D1, `repro/caller-tx-history-lost.cos` | confirmed deviation under the provisional answer (decision pending) |
| `history.committed.side-effects` (Q-07) | Not modeled | None | unresolved authority |
| `history.dependencies` (+ Q parent-rollback) | V1 | Native A0-A5 | confirmed violation D1 |
| `history.namespace` | Not modeled | N-P06, N-P17. Suite History `TestMultipleNamespaces` | supported by suite and premise |
| `history.read`, `.find`, `.filter`, `.details` | Not modeled | Native E0-E8, R1-R4. Suite History `TestHistoryModifiers` (partial) | filters: confirmed violation D2. Unfinished phase display: D8. Default modifiers as filters (Q-18): B2 stage C case (pending) |
| `history.read.global`, `.api` | Not modeled | Suite History `TestMultipleNamespaces` | partial. `GlobalRecords` across namespaces unestablished |
| `history.delete` (+ Qs, D global-unfiltered) | Not modeled | Native E4-E7. Suite History | confirmed violation D2 |
| `history.delete.running-records` (Q-20) | P-07 branch reachable only from a foreign delete | N-P15 | unresolved authority |
| `history.locked` | `theorem_r2`, control 10 detected | N-P13. Suite History `TestFinalizedRecordsLocked` | supported |
| `history.locked.unfinished` (Q-21) | Q-FIN parameter. V10 | Native C2 | confirmed violation under the provisional answer (decision pending) |
| `history.retention` (+ Q effect) | Not modeled | Native F0, F1. Suite CLI `TestAutoPurgeOldHistory` (observes no deletion) | confirmed violations D3, D5 |
| `history.keep-on-ipm-uninstall` (+ Q other namespaces) | Not modeled | N-P16 (persist step only) | unestablished. Self-uninstall suspected (OBS-H15) |

## Status

DRAFT. Pending B2's replay of the SIGKILL variants and stage C cases, and the round 1 model review.
