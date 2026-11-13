# Review record: provider-agent lane fan-out

Governing block: [`g5.quality.delivery.ci.provider-agent-parallel`](ontology/concepts/deployment.g5.md#fan-the-provider-agent-lane-out-across-workers), refining [`g5.quality.delivery.ci.provider-agent-overlap`](ontology/concepts/deployment.g5.md#overlap-the-provider-agent-browser-lane-with-the-broad-shards).

## Design basis

After #353 the provider-agent lane is the critical path: run 26765731 had it at 18.5–40.0 min (21.5 min) against shards at 19.0/14.3 min, and #358 is expected to halve the shards. Inside the lane, `chat-restart-continuity.spec.ts` (21 tests) took 15.4 min and `native-requests.spec.ts` (7 tests) 3.9 min, both as serial single-worker chains while the worker sat at load 3–6 on 22 vCPUs. Parallel safety checked by reading code: every test in both suites runs `createWorkspaceClone` + resume in `beforeEach` and deletes its clone in `afterEach`; provider choice goes through the chat launch settings, which `routes/chat.rs::agent_launch_for_session` resolves per chat session; the only cross-test hazard is `restartAssignedApi` in the single "M6/M14 API restart" test, which `docker restart`s the lane API and would interrupt any test in flight. Hence the split: parallel pass excluding that test, then that test alone.

## Verification before the PR run

- `scripts/test-ci-run-playwright-selector.sh` (6 cases: split shape, failure propagation from either pass, default worker count, native-requests single pass, unknown selectors serial, grep kept in sync with the spec title and parallel mode present in both specs).
- `test_cloudbuild_waitfor_order.py` (16 tests incl. the new `test_provider_agent_fanout_fits_its_host_capacity`: seed + workers < pa host slots, seed advertises the slots, the selector loop routes through the helper), `test_rust_e2e_lanes.py`, `test_ci_docker_contract.py`, `bash -n` on all 59 inline step scripts, provider-agent step at 9,350 of 10,000 chars, `cargo test -p g5-e2e ci_e2e_lane_isolation` (4 passed), `cargo fmt --check`, `tsc --noEmit` and eslint on the two specs.
- Not verifiable locally: the lane's wall time with two workers and whether the pa host's five slots suffice; the PR's `g5-ci-e2e` run supplies both (sampler summary and the lane's step timing).
