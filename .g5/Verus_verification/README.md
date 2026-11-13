# IPM Verus foundation: package loading and transaction history

Formal and native verification of IPM's `load` path and its transaction history log, produced by the uplift in `.g5/uplift/ipm-verus-20261113/`. The governing intent is the ontology in `.g5/ontology/`. Start at `features/load.g5.md` and `features/history.g5.md`.

```
 ontology (intent, Q = open decisions)
     │
     ▼
 L1 contract ─── crates/ipm-load-history/src/contract.rs   (Q parameters, provisional defaults)
     ▲ abstraction (durable store, end states)
 L2 as-built ─── crates/ipm-load-history/src/model.rs      (IRIS process + IPM steps, executable twins)
     │ BINDINGS.md: each step → file:line at 571e04a1
     ▼
 L3 product  ─── native/ (IRIS 2026.1 container)           cases/cases.json replayed at Shell, History, module rows
```

## Pins

| Item | Value |
|---|---|
| Product | `intersystems/ipm` `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, tree `9840c894a84a289d470f4447fa5f54f303741050` |
| Verus | `0.2026.09.27.3cf1832` at `~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/`, bundled Z3 4.16.0. The `verus` on `PATH` is a newer build and is not used. |
| Rust | `1.98.1-x86_64-unknown-linux-gnu` |
| Crates | `vstd =0.0.0-2026-09-20-0158`. Build offline. `crates/Cargo.lock`. |
| Vendored library | `vendor/explorer` from G5Labs bank-of-ziniu `0171d816…`, per-file hashes in `SOURCE.json`. `actors` was evaluated and not used (reason in `SOURCE.json`). |
| IRIS | `containers.intersystems.com/intersystems/iris-community:2026.1` digest `sha256:546fe52b…3a24fdf`, product image `ipm-verus-uplift-20261113:571e04a1` |
| ORAS test registry | zot `ghcr.io/project-zot/zot-linux-amd64@sha256:b68f727f…4c02` |

## Layout

| Path | Contents |
|---|---|
| `crates/ipm-load-history/` | Verus crate: `contract.rs` (L1), `model.rs` (L2 and twins), `proofs.rs`, `cases.rs` (case lemmas), `explore.rs` and `src/bin/b1.rs` (explorer harness and case generator), `BINDINGS.md` |
| `crates/scripts/receipt.py` | Runs one check and writes its receipt |
| `premises.md` | IRIS provider premises P-01..P-12 with native verdicts |
| `cases/cases.json` | Model-derived native cases: inputs, L1-permitted outcome, L2 prediction |
| `native/run-iris.sh` | Starts the disposable IRIS and ORAS containers. Runs a terminal script with a disk watchdog, receipt and `NATIVE-END` check. |
| `native/evaluate.py`, `evaluate_suite.py` | Evaluates `OBS` lines against expectations written before the run |
| `native/repro/*.cos` | Standalone reproductions of the product defects |
| `results/verus/`, `results/native/` | One directory per run: command, inputs, raw log, `receipt.json`, `verdict.json` (native) |
| `results/index.jsonl` | One line per run, with superseded runs annotated |
| `coverage-map.md` | Clause → model property → operations → evidence → what a consumer may rely on |
| `bugs_issues.md` | Product defects with repair proposals |
| `design_simplification.md` | Simplifications suggested by the model |

## Prerequisites

- The pinned Verus directory above, including `cargo-verus` and `z3`.
- The rustup toolchain `1.98.1-x86_64-unknown-linux-gnu` (`rustup toolchain install 1.98.1`).
- The crates `vstd =0.0.0-2026-09-20-0158` and `verus_state_machines_macros =0.0.0-2026-09-06-0133` already in the local cargo registry, because builds run offline. Fetch them once with network access (`cargo fetch` in `crates/` without `CARGO_NET_OFFLINE`).
- Docker with BuildKit (the product `Dockerfile` uses `RUN --mount`), and network access to pull the IRIS and zot images once.
- Python 3 for `receipt.py` and the evaluators.

Running the runbook writes new receipt directories under `results/` and appends lines to the tracked `results/index.jsonl`. The inputs digest of a new receipt can differ from the final receipt's when helper scripts changed and the proof sources did not. Compare the `.rs`, `Cargo.toml` and `Cargo.lock` lines of `inputs.sha256`.

## Reproduce one formal obligation and its control

Working directory: `.g5/Verus_verification/crates`. Each command writes a new receipt directory under `results/verus/` and prints its path. A timeout or missing summary is recorded as INCOMPLETE, never as a pass.

1. As-built proofs. This covers `theorem_commit_sound`, the obligation that a top-level load which changed module data records `Committed=1`, along with every other theorem and case lemma.

   ```sh
   python3 -I scripts/receipt.py verus repro-asbuilt as-built "verified, 0 errors" "runbook: as-built proofs"
   ```
   Expected: `verified 263, errors 0`, matching the final receipt `results/verus/20261113T222933Z-asbuilt-proofs-r3`. It takes under a minute when the build is cached, and a few minutes from a cold build.

2. Its distinguishing control. Control 01 removes the `SetCommitted` call after the developer-mode first commit (`Utils/Module.cls:1312-1314`).

   ```sh
   python3 -I scripts/receipt.py verus repro-control-01 control-01 "failure of lemma_lnm0_b" "runbook: control 01" --features control-01
   ```
   Expected: errors in `lemma_lnm0_b` and one case lemma. On the final inputs that lemma is `case_c26_dev_failure_keeps_phase_work` (final receipt `results/verus/20261113T223505Z-final-r3-control-01-verus`). Verus stops evaluating case lemmas at the first one that fails, so which case lemma appears depends on source order. Verus prints no summary line when a lemma fails, so the receipt parser reports the failing lemmas instead.

3. A bounded counterexample for defect D1 (dependency records lost on a parent rollback).

   ```sh
   python3 -I scripts/receipt.py explore asbuilt-r3-dep as-built "violation of R3-dep" "runbook: D1 counterexample"
   ```
   Expected: `VIOLATION` of R3-dep with a 15-step shortest trace in the raw log. The script builds the explorer harness `b1` with the pinned toolchain. The first argument is the explorer run name defined in `src/bin/b1.rs`.

4. The proposed D1 repair, modeled as variant 103 and kept separate from the as-built model.

   ```sh
   python3 -I scripts/receipt.py explore repair-103-dep-clauses repair-103 pass "runbook: D1 repair variant"
   ```
   Expected: `PASS`, exhaustive (189,577 states). The as-built counterpart `asbuilt-dep-clauses` reports a VIOLATION.

## Reproduce one native defect

Working directory: `.g5/Verus_verification/native`. `up` recreates the containers from the pinned image and discards earlier container state.

```sh
# Once, if the image is missing:
#   git archive 571e04a19aba9f9b4bc0ea23fbe09182038f4e55 | tar -x -C DIR && chmod -R a+rX DIR && docker build -t ipm-verus-uplift-20261113:571e04a1 DIR
./run-iris.sh up                                    # expect: "up: ipm-verus-uplift-iris healthy"
printf '%s\n' 'zpm "load /home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/simplest-module/1.0.0/"' 'write !,"NATIVE-END",!' 'halt' > /tmp/seed.cos
./run-iris.sh exec seed-history /tmp/seed.cos       # gives the history at least one record
./run-iris.sh exec repro-history-delete-underscore repro/history-delete-underscore.cos
python3 -I evaluate.py ../results/native/<run-id printed above> cases/expected-repro.json
```

Expected: `process_outcome` COMPLETED, and case X3 DEVIATES (final receipt: `results/native/20261113T184623Z-repro-history-delete-underscore`; reproduced at `20261113T211000Z-…` by the round 2 reviewer). `total.after` is 0 while `total.before` is at least 1 and `truth` is 0, so `history delete -DVersion_Major=99` deleted every record (defect D2). The script backs the history up and restores it, and `total.restored` equals `total.before`. The counts depend on the container's state: the image already holds some records, so `up` plus one seed gave 4 in the round 2 reproduction and 81 in the original run. A run with `total.before=0` is reported INCOMPLETE by its guard. `repro/history-filter-widened.cos` also needs a record whose major version is not 1, so seed `…/simplest-module/2.0.0/` as well.

To replay the model's cases, see `.g5/uplift/ipm-verus-20261113/B/native-replay.md`. Each case's script, expectations and verdict are listed there.

## Reading a verdict

- `receipt.json` `process_outcome` (native) and `exit_code` (Verus) describe the process only.
- The substantive verdict is `observed` (Verus) or `verdict.json` (native).
- A native case is CONFORMS, DEVIATES or INCOMPLETE. INCOMPLETE covers a missing field, a failed harness field, a missing marker, a license, disk or timeout stop, or a non-COMPLETED process.
- `l1_vs_l2` in `cases.json` says whether the as-built model meets the contract. B2 compares the product with both `l2_predicted` (binding check) and `l1_permitted` (contract check).

## Status

See `coverage-map.md` for what each claim rests on, and `.g5/uplift/ipm-verus-20261113/handoff.md` for the uplift's final status, open decisions and incomplete work.
