# Review record: E2E Cloud Build speedup (PR #353)

Governing blocks: [`g5.quality.delivery.ci.base-image-mirror`](ontology/concepts/deployment.g5.md#mirror-ci-base-images-by-content-key), [`g5.quality.delivery.ci.provider-agent-overlap`](ontology/concepts/deployment.g5.md#overlap-the-provider-agent-browser-lane-with-the-broad-shards).

## Round 1 — Codex (gpt-6-astra, xhigh, read-only) on commit 6e425c87

Verdict: REQUEST CHANGES. Findings and resolution:

1. **P1** `chat-restart-continuity.spec.ts::restartAssignedApi` hard-asserted `E2E_API_CONTAINER === "g5-api"`, so the lane's export of `g5-api-pa` would fail every continuity test before the restart. Fixed: accept `/^g5-api(-pa)?$/` (dedicated replica or shared-stack rollback).
2. **P1** `tests/e2e/src/scenarios/ci_e2e_lane_isolation.rs` pinned the PA host's `G5D_API_URL=http://g5-api:7500`, its old prerequisites and the provider lane's shard waitFor edges; the Rust E2E gate runs that suite. Fixed: contract now requires `g5-api-pa` for the host URL/git allowlist/alias, `start-pa-api` as a host prerequisite, the `start-pa-api`/`start-pa-client` steps and their wiring, the in-script replica exports, and the new waitFor list. `cargo test -p g5-e2e ci_e2e_lane_isolation`: 4 passed.
3. **P2** The shard-marker poll sat before the lane's value-loop selectors and transfer test, so those still waited on the shards. Fixed: the poll, `plan_playwright_shards.py verify` and the inventory copy moved after the M9/M12 transfer test, immediately before `record_playwright_shard_results.py`; `test_cloudbuild_waitfor_order.py` now asserts the poll is after the transfer grep and before every shard-artifact read.

Codex checked and found sound: OAuth callback/edge upstream/guest git allowlist/seeder target on the PA stack; cookie isolation; mirror freshness + parent-layer acceptance; worker-base status consumers; `build-ci-image` parity across pipelines.

## Round 2 — Codex (gpt-6-astra, xhigh, read-only) on commit 5026c3e3

Verdict: APPROVE, no new actionable findings. Confirmed by source inspection: all three round-1 repairs; PA wiring (OAuth callbacks, edge upstream, guest git URL/allowlist/alias, seeder container) targets the dedicated stack; shared-state scoping (fixture IDs, repositories, org-keyed runtime config, cookie isolation); API pool ceilings 284 of 400 connections; mirror key/lineage/acceptance and lock interaction; marker helper success/timeout/missing-argument behaviour; the length parser agrees with PyYAML (provider step 9,257 chars). Converged after two rounds. Not covered by review: warm-cache timing and the overlapped resource envelope, which the PR's Cloud Build runs supply.

## E2E run 62f02928 on 5026c3e3

Failed at `frontend-test` (unrelated to this PR's files): `WorkspacePageAlignment.test.tsx › lands on the timeline with its status header` expected `data-source="description"` and saw `"none"`. `TimelineStatusHeader` renders `none`/"Loading purpose…" while the external-issue read is pending and the test asserted synchronously after the header's first paint; four other branches on the same main passed the step and the file passed 5/5 locally. Hardened the assertion with `waitFor`. The config itself was accepted by the trigger, and the cold mirror path logged `Base tier not in mirror (building locally)` with the keyed name as designed before the build stopped.

## Rust run d47cfba2 on 5026c3e3

`RUST_FMT` only: two over-long `step(...)` calls in the retargeted contract. `cargo fmt`; green on fc55f922 (97c9d63b, 13 min).

## E2E run 26765731 on 97c9d63b — SUCCESS, 40.3 min (from 59.7)

Phase timings (min from start): Ubuntu tiers pulled from mirror (api shell 0.1–1.4, edge 3.8–4.0); worker base fallback build + publish 3.2–11.1 (still cold for this recipe); stack up at 16.0; shards 18.5–37.6 / 18.5–32.9; provider-agent 18.5–40.0 overlapped; teardown 40.3. Sampler: `min_mem_available_mib=34128` of 44 GB, `max_load1=24.15` on 22 vCPU, no OOM. Lane evidence: `g5-api-pa` healthy, 21 restart-continuity tests passed against it, receipt verified against both shard reports. Cost observed: the provider-agent lane itself is slower under overlap (21.5 vs 16.9 min; chat-restart-continuity 15.4 vs 8.7 min) and is now the long pole. Warm-path run (worker base pulled) follows this commit.

## CodeRabbit (reviews at 23:33 and 23:42 on 6e425c87/5026c3e3)

- Major, `test_cloudbuild_waitfor_order.py`: update the Rust waitFor expectations — already repaired in 5026c3e3 (Codex round 1, finding 2).
- Minor (outside diff), `pull-worker-base-image`: the worker-base key hashed only source files, but `Dockerfile.base` installs unpinned apt packages, so the parent image can change without a source change and a registry hit could reuse a worker base built on an older parent. Accepted: the pull step now ensures `g5/base` (via the mirror) first and folds the resolved image id into the key; `Dockerfile.base` leaves the hash since the parent id subsumes it.
- Trivial, `ensure-base-images.sh`: `docker pull` discarded stderr, so an auth or network failure read as a cache miss. Accepted: stderr stays visible and the miss message says "pull failed or image not found".

## Second optimization round (PR #358)

Warm-path evidence from run 95d0c684 (codex/issue-289 on the merged config): worker base pulled in 2.3 min, stack healthy at 10.1 min, browser tests started at 13.9 min (from 22.3 before #353). Playwright JSON reports from 26765731 show the shards have no idle gaps (shard 1 wall 19.0 = summed 18.9 min; shard 2 14.3 = 14.2) while the sampler series shows load 3–6 of 22 vCPUs and 34 GB free during minutes 19–40; the provider-agent tests ran 1.3–1.9x slower overlapped than alone (uniform across tests, so a shared latency bottleneck, not CPU). Changes: `_PLAYWRIGHT_WORKERS: '2'` with `g5d-host-1` at 8 slots (seed 1 + 4 workers + overlap) and `G5_E2E_HOST_CONCURRENT_JOBS=8`; `pull-worker-base-image` becomes an independent root. Candidate next step, not done: split `chat-restart-continuity.spec.ts` (21 tests, one of which restarts the lane API) so the 20 VM stop/resume tests can run on two workers; that needs the value-loop catalog rows and the lane's per-selector invocation to follow, and the pa host's slot count to rise.

## Trigger-time failure on 6e425c87

`g5-ci-e2e` reported `invalid build: invalid .steps field: build step 64 arg 1 too long (max: 10000)` before any build ran: the provider-agent step's inline script had grown to 10,957 characters. Fixed by moving the three marker polls into `scripts/ci-wait-markers.sh`, trimming in-script comments (rationale moved to the step's trailing waitFor comment, outside the counted arg) and adding `test_inline_step_scripts_fit_cloud_build_arg_limit` (plain-text block-scalar parser validated against PyYAML on all three configs; 400-char margin). Step now 9,257 characters.
