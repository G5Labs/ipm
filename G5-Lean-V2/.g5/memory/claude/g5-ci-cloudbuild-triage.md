---
name: g5-ci-cloudbuild-triage
description: "How to read g5-ci-rust/e2e Cloud Build failures fast: the assert-ci-status step names the failed stage; the e2e value-loop receipt fails on retry-passed (flaky) mandatory specs."
metadata:
  node_type: memory
  type: reference
  originSessionId: cd9ca33f-4417-4104-a26f-c451a1174fcd
  modified: 2026-10-24T04:21:57.875Z
---

When a `g5-ci-rust` or `g5-ci-e2e` Cloud Build fails, don't scan test output first: the
final `assert-ci-status` docker step prints `CI stages failed: <STAGE>` (e.g. `RUST_FMT`,
`RUST_CLIPPY`, `VALUE_LOOP_MATRIX`, `PROVIDER_AGENT_E2E`) near the TOP of
`gcloud logging read` output (newest-first). Reproduce fmt/clippy locally with the pinned
toolchain (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
— CI runs the whole workspace).

The e2e value-loop receipt requires its mandatory Playwright specs (e.g.
`e2e/flows/terminal.spec.ts`) to pass **without retry**; a flaky retry-pass fails
`PROVIDER_AGENT_E2E` and cascades every later selector to `not_run` in
`VALUE_LOOP_MATRIX`. A first-attempt `queued_for_capacity` VM-start wait in the e2e stack
is a known environmental flake — re-push/rerun rather than chasing it in the diff (each
e2e host has 3 slots; compare `vm.capacity-wait=` shard timings across other PRs' builds).
One branch-side cause to rule out first: a spec the branch added that starts a VM and
tears down with a single unawaited stop can hold a slot for the rest of its shard; poll
the stop until `stopped` (#645, 2026-10-24). To
list every first-attempt failure with its error at once, parse the `G5_TEST_END {json}`
lines (`status` failed/timedOut, `errors`). `POST /resume` "Timeout 60000ms" is the same
class of flake: resume waits up to 125 s (START_REPLY_SECS) for the host's start reply,
longer than the specs' 60 s request timeout. Seen 2026-10-23 on #645 (6 flakes) and 058cbb2f. See
[[cloudbuild-log-reader-sa]] for log access.

`VALUE_LOOP_RUST_API` can fail alone (all Playwright stages green) on
`fork_recovery::test_failed_live_flush_then_api_restart_forks_one_complete_child` with
"operation timed out" sending `POST …/resume` to the restarted `g5-api-re`. Seen on PR #774
(2026-10-23) for a frontend-only diff whose identical code passed twice before: a flake,
likely the same resume-wait class as the Playwright `POST /resume` timeouts above.
The step's log is `gcloud logging read … AND textPayload:"value-loop-rust-api-profiles"`.
Neither `g5d` nor the reader key can rerun a build, so retrigger with a real commit, not an
empty one.

Another receipt rejection (PR #840, 2026-10-26): shard-2 `personal-workspace.spec.ts` ›
"Start and send from Home starts the personal VM and queues the message" missed its 20 s
message-render deadline after the personal VM reached running ("No messages yet"), passed
on retry in 12 s, and the provider-agent step then logged `browser shard receipt rejected:
e2e/flows/personal-workspace.spec.ts:<ids> did not pass once without skip or retry` →
`PROVIDER_AGENT_E2E: FAILED` with every value-loop selector `not_run`, although all of
that step's own Playwright runs passed. Fast check: `grep 'browser shard receipt rejected'`
in the build log; a dependency-only diff cannot cause it. The spec's wait was last tuned
on main in b65bdae7b (2026-10-26).

A `g5-ci-rust` failure in `rust-deny` (`advisories FAILED`) on a branch that changed no
dependencies is usually a RustSec advisory published since the last green run (e.g.
RUSTSEC-2026-0381/0382 on 2026-10-20, fixed on main in b3afb0a2). Check
`git log origin/main -- Cargo.lock deny.toml` first and merge main rather than bumping
crates on the branch. Exception (2026-10-26, PR #712): RUSTSEC-2026-0391/0398/0403/0410 hit
zstd 0.13 with no fix on main, so the branch bumped `zstd = "0.14"` in g5d and g5-internal-api
(API-compatible; `cargo update -p zstd`).

The npm counterpart: `g5-ci-e2e` failing early in step `frontend-lint` (no
`assert-ci-status` lines) is usually `npm audit --omit=dev --audit-level=high` on a new
GitHub advisory, e.g. GHSA-mrhx-c7xc-m3w5 (zod <4.6.6) on 2026-10-23. Check
`git log origin/main -- frontend/package*.json` first; if main has no fix, bump only the
flagged direct dependency (main fixed zod in aec065134, ^4.6.7) and reproduce the step locally:
`rm -rf node_modules && npm ci && npm run lint && npm audit --omit=dev --audit-level=high`.

Fastest full read: `gcloud builds log <id> --region=us-central1 > f.txt`, then grep `CI stage`,
nextest `FAIL [`, and for Rust E2E `failureCause`. A formal review stuck or failed at
`start` is often a host boot failure: grep `g5d::workspace::repo_cache` for repeated
"transient 403" on a repo id. That means the VM spec named a repo the VM token cannot read
(PR #645: a personal mount leaked into formal review VMs). It is not a flake. Compare
the scenario's result on other PRs' builds of the same trigger before calling it one.

Main-wide flake (2026-10-27): `workspace-preview.spec.ts:313` (run-log case, test id
`c10fc0756db5`) fails its first attempt in ~8 of 29 builds across unrelated PRs with
"the viewer must receive G5_NOISY_SHUTDOWN" (~105 s vs ~51 s passing); the retry passes but
the receipt rejects it. Quantify any suspected flake by querying `G5_TEST_END` lines for the
test id with `"retry":0` across builds since a date, before touching the diff.

Also: the dev Postgres container (`g5-dev-postgres`) can vanish mid-session (docker state
reset); "pool timed out waiting for connection" in integration tests means restart it with
`docker compose -f infra/docker/docker-compose.dev.yml up -d postgres`, not a code bug.

2026-10-27 (PR #853, e2e build 608a36e0, no frontend/API changes in the diff): shard 2
receipt rejected `auto-commit-review.spec.ts › accepting a review edit…` (first attempt
409 CONFLICT on `/review/publish`, passed on retry) and `source-control-selection.spec.ts
:306 › live navigation retains the snapshot…` (line 326 timing, passed on retry); the
value-loop receipt then reported mandatory selectors `not_run`. Same flaky-receipt class;
retrigger with a real commit. A queued g5-ci-e2e can also EXPIRE (f0276a1e sat ~1 h in
the queue with no step run); that too needs a real commit to retrigger.
Also seen 2026-10-27 (PR #853, e2e ae9ba904): `workspace-preview.spec.ts:324 › streams the
preview's output on the terminal path…` first attempt "the viewer must receive
G5_NOISY_SHUTDOWN" after 60 s, passed on retry; receipt rejected. The pool was congested
(16+ queued builds) and queued builds EXPIRED twice that evening.
