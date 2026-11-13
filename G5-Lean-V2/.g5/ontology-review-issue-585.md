# Issue 585 implementation review

PR #590 builds the `g5-api` integration tests as one Cargo target instead of 144. Compared with `origin/main` at `5fb27ecb4`. No ontology block governs test build layout; the decision is recorded in `backend/g5-api/tests/main.rs` and here.

## Self-review before external review

| Check | Evidence | Result |
|---|---|---|
| Every test still compiles and is enumerated | nextest lists 1501 tests; the sources carry 1501 test attributes (`#[tokio::test]` 1370, `#[test]` 130, nested `steering.rs` 10) | Equal. |
| Whole suite passes under nextest | local Postgres, `--test-threads 6`: 1501 passed, 0 failed, 12.5 min | Passed. |
| CI selects g5-api tests by binary name | `cloudbuild-ci-rust.yaml`, `cloudbuild-ci-e2e.yaml`, `run-rust-xfs-orchestrator.sh`, `local_validation.py`, `justfile` use `--test "*"` or `package()`/`kind()` filters | No change needed (one exception found by Codex, below). |
| Tests that re-exec themselves by exact name | `--exact` sites in `api_workspace_pull_requests.rs`, `remote_advisory_pinger.rs`, `migration_role_provisioning_concurrency.rs` | Repaired with `helpers::exact_test_name(module_path!(), name)`; the provider scenarios also require `1 passed` because an empty filter exits 0. Codex found a fourth site (below). |
| Cost on a workspace VM | fresh QA VM: per-file binaries 532/537 MiB; merged binary 733 MiB; compile+link 97 s; relink after a `g5-api-core` touch 4m13s / 4m04s versus 3m00s / 2m54s for two per-file binaries | Disk 75 GiB → 0.7 GiB; the single-file edit loop is about a minute slower while the image keeps `CARGO_INCREMENTAL=0`. |

## Codex round 1 (gpt-6-astra, read-only) — REQUEST_CHANGES

| Finding | Evidence | Disposition |
|---|---|---|
| P1 `api_git_push_policy.rs` re-execs `late_observation_effects_child` by bare name; the child runs zero tests and exits 0, so six fault scenarios vanish while both tests pass | Codex ran the prebuilt binary with the bare filter | Accept. Same repair as the other three sites plus the `1 passed` requirement. |
| P1 value-loop lane in `cloudbuild-ci-e2e.yaml` derives `--test <file stem>` from the catalog source; F8 and M10 name targets that no longer exist | yaml line 2865 | Accept. `--test integration` with the selector scoped to the module: `test(/^<stem>::.*<selector>/)`. `value_loop_receipt.py producers` and its 17 unit tests pass against the edited pipeline. |
| P1 `cargo fmt --check` fails in 12 files after `mod helpers;` became `use crate::helpers;` (import order) | `rust-fmt` is a required step | Accept. `cargo fmt -p g5-api`. |
| P2 `api_client_ip.rs` and `api_ws_tracing.rs` each unwrap `set_global_default`; in one process (`cargo test`, used by `run-integration.sh`, `test-parallel.sh`, `just test-cross`) the second panics | both files inspected | Accept. One process-wide subscriber in `helpers::tracing_capture()` (the library's capture layer on a `Registry`); both tests use it. Verified by running both in a single `cargo test` process. |
| P2 `seed_codex_explainer.rs` sets `G5_E2E_AUTH` without the `e2e_auth` serial lock that `api_orgs.rs` uses | file inspected | Accept. `#[serial_test::serial(e2e_auth)]` on the seed test. |

Checked by Codex and not an issue: macro scope, inner attributes, `#[path]` modules, executable `crate::` paths, `exact_test_name`, CI and coverage lane selection, feature selection, binary-keyed ports or temp dirs.

First CI run on the round-1 head failed in `build-ci-image`: the builder warms dependencies with dummy sources, and the explicit `[[test]] path = "tests/main.rs"` must exist when Cargo loads the manifest. The dummy warm-up test is now written to that path (precedent: the g5d `isolation_backends` dummy in the same step). All five round-1 findings and this were repaired in f0c246c32; the 137 tests in the touched modules pass under nextest, and the two tracing tests pass together in one plain `cargo test` process.

## Codex round 2 — REQUEST_CHANGES (two P2, no P1)

| Finding | Evidence | Disposition |
|---|---|---|
| P2 `Dockerfile.g5d`, `Dockerfile.g5-api`, `Dockerfile.test-runner` stub stages lay out `backend/g5-api/src` but no `tests/main.rs`; with a pre-PR warm base reused in a workspace VM (`justfile` warm-base reuse), Cargo cannot load the g5-api manifest, so even `cargo build -p g5d` fails | Dockerfile stub lines inspected | Accept. Each stub stage now also writes an empty `backend/g5-api/tests/main.rs`. |
| P2 `api_workspace_reviews.rs::init_tracing` still installs a global fmt subscriber with `try_init`; if a review test runs first in one process, `helpers::tracing_capture` panics | file inspected | Accept. `capture_subscriber` returns the concrete `Layered<CaptureLayer, Registry>` so the shared subscriber can stack the WARN formatting layer the review tests wanted; `init_tracing` now calls `helpers::tracing_capture()`. Verified with a review test, `api_client_ip` and `api_ws_tracing` in one `cargo test` process, review test first. |

Checked by Codex and not an issue: all 13 `api-integration` catalog rows match exactly one test under the module-scoped filter; one integration target in Cargo metadata; the four child filters resolve; cleanup is package-based; other lanes use wildcards or Cargo metadata.

Repaired in af347bd39; g5-api-core capture unit tests (15), the three tracing modules under nextest (65) and a review test with `api_client_ip` and `api_ws_tracing` in one `cargo test` process all pass.

## Codex round 3 — REQUEST_CHANGES (two P2, one P3; all single-process or documentation)

| Finding | Evidence | Disposition |
|---|---|---|
| P2 the shared capture buffer records every event for the life of the process; under one-process `cargo test` it would hold the whole suite's DEBUG/TRACE output | `CaptureLayer::on_event` appends unconditionally | Accept. `test_capture::capture_layer()` exposes the layer; the shared subscriber installs it behind `filter_fn(target == "g5_audit")`, which is all any test asserts on. |
| P2 `api_worker_hosts_cloud.rs` sets `G5_WORKER_SCRATCH_DISK_MIB` in two tests while `api_worker_hosts.rs` rereads it in two tests; under threaded one-process `cargo test` the reread can see the other module's override | sites inspected | Accept. `#[serial_test::serial(worker_scratch_disk)]` on the two writers and the two readers. Note: every plain `cargo test` caller in the repo (`run-integration.sh`, `test-parallel.sh`, `just test-cross`) passes `--test-threads=1`, so this is defence in depth; the remaining env guards (`G5_STALE_HOST_DETECTOR_ENABLED`, rotation flags, `RUST_LOG`) restore on drop and are safe sequentially. |
| P3 three file doc comments still name `cargo test -p g5-api --test <stem>` | `api_config_concurrency.rs`, `api_config_secret_hints.rs`, `api_provider_accounts.rs` | Accept. Rewritten as the nextest module filter. |

Checked by Codex and not an issue: the concrete `Layered` type with the other two `capture_subscriber` callers (`audit_log`, `telemetry`); WARN formatting does not filter INFO audit capture; `with_test_writer` under nextest; the three Docker stub `RUN` chains parse (`sh -n`) and later `COPY`/`touch` ordering keeps the file; one integration target in Cargo metadata; all 13 value-loop rows resolve; self-exec names qualified.

Repaired in b4e8c5ab0; 126 tests across the touched modules pass under nextest, one-process `cargo test` still passes.

## Codex round 4 — REQUEST_CHANGES (one P1)

| Finding | Evidence | Disposition |
|---|---|---|
| P1 with per-layer filters a span is enabled only if some layer wants it; the audit filter rejects non-audit spans and the WARN formatting layer rejects INFO, so the server's INFO `ws_connection` span was disabled and `api_ws_tracing`'s "no span leaks across await" assertions passed vacuously, under nextest too | `g5-api-core/src/ws/mod.rs:434`, `api_ws_tracing.rs:44` | Accept. The capture filter is now `is_span() || target == "g5_audit"` (the layer records events, not spans, so enabling spans costs nothing), and the test first asserts that an INFO span in the server's target is enabled. |

Checked by Codex and not an issue: `api_client_ip` reads only `captured_g5_audit_events`; `capture_layer` visibility; the scratch-disk lock covers the full override and reread; 144 files declared, one target; all 13 value-loop rows resolve.

Repaired in bda5636c2.

## Codex round 5 — APPROVE

No findings. Checked: every span stays enabled and visible through `Span::current()` while WARN formatting leaves INFO audit capture intact; the enabled-span assertion closes the vacuous leak check; `maintenance_integration` and `tracing_targets_moved_modules` use independent scoped subscribers; 144 files declared, 1501 tests in source and in the binary; all 13 value-loop rows resolve uniquely; `cargo fmt` and `git diff --check` clean.

Five rounds; each round's findings were a new class (self-exec names, CI selectors and image stubs, process-wide subscriber, unbounded capture and env locks, filter-disabled spans). Final local evidence is the full-suite rerun recorded in the PR description.
