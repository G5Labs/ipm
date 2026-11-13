# IPM consolidated Verus uplift plan

Initial uplift of InterSystems Package Manager (IPM) using the G5-Lean-V2 Verus uplift guide (`G5-Lean-V2/.agents/guidance/Verus/uplift.md` and its linked guides). G5-Lean-V2 is methodology only. IPM is the product.

## Request and scope

| Item | Decision |
|---|---|
| Requested by | Repository owner, session of 2026-11-13. |
| Intent recovery and ontology | Across IPM: the root must let a reader find and reconstruct the major capabilities. |
| Formal modeling and native validation | Package loading (`load`, and the shared `LoadModuleReference` / `LoadNewModule` path it uses, including dependency loads) and transaction history (`%IPM.General.AbstractHistory`, `HistoryTemp`, `History`, the `history` command and its readers). Same scope as the playbook arm. |
| Frontend | Not applicable. IPM is an ObjectScript backend with a terminal shell. |
| Product repairs | Not authorized. Defects are recorded with repair proposals. |
| New runtime checks | Authorized for native validation inside `.g5/Verus_verification/native/`, run in a disposable IRIS container. No new tests are added to the product's `tests/` tree. |
| Excluded inputs | Other experiment arms' outputs and evaluator material. Workers receive only the inputs listed in `sources.md`. |

## Baseline revisions

| Input | Pin |
|---|---|
| Product | `intersystems/ipm` at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (tag `v0.10.11-beta.28`, manifest `0.10.11-SNAPSHOT`). Branch `experiment/ipm-consolidated-verus-20261113`; its only extra commit `5bf22282` adds `G5-Lean-V2/` and changes no product file. |
| Wiki | `ipm.wiki.git` at `781e98fd6c968cc96ae2b4008b24d09998644bb8` (2026-11-04), local copy `../ipm-wiki`. |
| Starting evidence | `../docs/ipm-uplift/index.md`, sha256 `d01d51460e575b5334aff65dd3f41bbb81c50f3b03f96b566f6b5b2355745ca4`. |
| Methodology | `G5-Lean-V2/.agents/guidance` at commit `5bf22282`. |
| Verus | `0.2026.09.27.3cf1832` (bank-of-ziniu `setup-verus.sh` pin), binary at `~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus`, bundled Z3 4.16.0, Rust `1.98.1-x86_64-unknown-linux-gnu`. The `verus` on PATH is a newer build (`0.2026.10.04`) and is not used. |
| Verus crates | `vstd =0.0.0-2026-09-20-0158`, `verus_state_machines_macros =0.0.0-2026-09-06-0133` (bank-of-ziniu workspace pins, cached locally). |
| Consolidated libraries | G5Labs `bank-of-ziniu` snapshot `0171d8169291ab4ac6a873943ef5d3fb35831e52` (archive sha256 `ae65c7ed…f75ac157`) at `~/experiments/bank-of-ziniu`, crate `actors`, Cargo.lock sha256 `9e63f5a8…88722`. Whether `actors` applies is a stage-B decision recorded in `SOURCE.json`. |
| Native runtime | `containers.intersystems.com/intersystems/iris-community:2026.1` digest `sha256:546fe52b9e68ed810ac848faec1ffe230b84ba4e66ac15fb8e76dfa0f3a24fdf` (IRIS 2026.1 Build 234U). Product image `ipm-verus-uplift-20261113:571e04a1` built from `git archive 571e04a1` with the repository `Dockerfile` and `iris.script`. |

## Execution budget and constraints

- Host: 10 vCPU, about 48 GB available memory, shared with other workloads.
- Disk: the root volume is shared and nearly full. Free space swung between 0 and about 5 GB during setup. A first History-suite run on 2026-11-13 coincided with a disk-full event and its result is void. Every IRIS run goes through the watchdog runner, which stops the run below a free-space floor and records the stop as an incomplete check.
- One IRIS container at a time (`ipm-verus-uplift-iris`). Verus runs use `--num-threads 4`. At most three delegated agents run concurrently, and expensive runs are queued by the orchestrator.
- A timeout, disk-full or resource stop is an incomplete check, never a pass.

## Stages and assignments

| Stage | Worker | Question | Write ownership | Status |
|---|---|---|---|---|
| A1 | Intent recovery agent | Which commitments do human-authored sources establish across IPM, in depth for load and history? | `A/intent-inventory.md`, `A/sources-intent.md` | done 2026-11-13: 18 load and 17 history commitments, 42-row catalog, Q-01..Q-17 |
| A2 | Operation inventory agent | Which operations exist from entry points through writers, commit boundaries, effects and final readers? Deep for load and history. Must not read A1. | `A/operation-inventory.md` | done 2026-11-13: 39 commands, 29 load and 26 history operations, 34 observations |
| A3 | Ontology author | Reconcile A1 and A2 into one coverage map and write the governing ontology. | `.g5/ontology/**`, `A/coverage-map.md` | done: 18 pages, 146 blocks (122 I, 5 D, 19 Q), A4 rows R1-R14 adjudicated in `A/reconstruction-dispositions.md` |
| A4 | Fresh reader | Reconstruct the major functionality from the ontology alone, then compare with A1/A2. | `A/reconstruction-check.md` | done: 6 material corrections (R1-R6), 8 minor |
| B1 | Verus modeler | L1 contract, L2 as-built transitions, refinement and controls for load and history. | `.g5/Verus_verification/crates/**`, `vendor/**`, `SOURCE.json`, `premises.md`, `cases/**`, `results/verus/**`, `B/model-notes.md` | resumed 19:15 UTC after usage-limit stop; proofs partly green, no receipts yet |
| B2 | Native validator | Premise checks on IRIS transactions and IRISTEMP, existing suites, and native replay of model cases at the real handler, store and reader. | `.g5/Verus_verification/native/**`, `.g5/Verus_verification/results/native/**`, `B/native-report.md` | phase 1 done (`B/native-report.md`: 18 premises, 9 suites, cases a-g, defects D1-D8); phase 2 replays `cases/cases.json` |
| C | Fresh independent reviewers, then repair agents | Coverage and fidelity challenge per round. | `C/round-<n>/**`; repairs in owning artifacts | pending |
| Handoff | Orchestrator, then a reproduction reviewer | Runbook, coverage map, receipts, findings. | `.g5/Verus_verification/README.md`, `coverage-map.md`, `bugs_issues.md`, `design_simplification.md` | pending |

## Review-round counter

Rounds used: **1 of 3** (round 1 started 2026-11-13 19:25 UTC; ontology domain first, model and evidence domain after B1 and the B2 replay). A resumed worker continues from this counter.

## Open obligations

Updated at each handoff. See `C/` for findings and dispositions once stage C starts.

## Log

- 2026-11-13 19:15 UTC: resumed after a service usage limit stopped A3, B1 and B2 at about 18:50. B2 had finished its report. A3 had not started the A4 corrections. B1 had a partial crate and no receipts. Before the summary, the orchestrator's saved transcript records a TaskStop and a WebSearch whose content was not kept. No file changed after 18:49 except B2's report and B1's `proofs.rs`, so no artifact depends on the missing part. B1 was told to use B2's premise results and not to read B2's case observations until `cases.json` is written.

- 2026-11-13: native baseline. `Test.PM.Integration.History` needs the ORAS service. With zot attached it passed 13/13 methods and 450 assertions (`results/native/20261113T175623Z-smoke-history-suite-oras`). Two earlier attempts are void: one coincided with a host disk-full event, one hung without ORAS. The runner now requires a `NATIVE-END` marker because a killed session exited 0.

- 2026-11-13: plan created. Toolchain and IRIS image pinned. IPM image built and IPM 0.10.11-SNAPSHOT confirmed installed in USER.
