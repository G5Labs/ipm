# Local validation

The laptop workflow catches source-only contract failures before starting Docker builds, and retains a verified stack for focused reruns. Both `g5-ci-rust` and `g5-ci-e2e` must still pass on the current PR head before merging. Local selection and receipts do not waive those gates.

## Developer loop

Run these commands from the worktree you want to validate:

```sh
just validation-recommend main
just validation-preflight
just ci-docker-prepare
just ci-docker-5 'package(=g5d) and test(my_test)'
just ci-docker-6 my_rust_e2e_test
just ci-docker-7 'e2e/flows/login.spec.ts'
just ci-docker-down
```

`validation-preflight` checks physical build inputs, command syntax, migration checksums and version uniqueness, fixture identities, and local harness contracts. It needs Python 3.11+, Cargo/Rust and Just. Preflight runs offline and does not build application images or require a live database. The package-boundary and type compatibility assertions live in the `g5d` integration test target `crate_architecture`, which Phase 5 runs with `package(=g5d) and binary(=crate_architecture)`.

`ci-docker-prepare` builds and seeds the stack and browser prerequisites, then stops before running Phase 6 tests. Its initial preparation can still take substantial time. Each worktree gets a stable project name derived from its absolute path, its own service containers and volumes, and its own API/browser image tags. Set `G5_CI_COMPOSE_PROJECT` consistently on every command if you need a specific project name. Docker preparation and execution serialize on a host lock because the Rust target and base-image caches remain shared; contention exits with retryable status 75 before preflight or Docker runs. Retry after the current run finishes; standalone preflight remains available meanwhile. This does not enable concurrent Playwright workers. The local Rust builder cache key follows the Dockerfile’s actual COPY inputs, ignore files, parent image identity and warmup options.

For Phase 5, an exact selector such as `package(=g5d) and binary(=crate_architecture)` also restricts Cargo compilation to that package and test binary. Narrowing and recommendations use Cargo metadata and the Phase 5 recipe inventory; crates without integration targets receive unit-test recommendations instead. Complex filters and selectors without a matching integration target keep the full integration package inventory, avoiding unsafe guesses about unions or negation. Rust and browser runners use Docker’s init process so interrupting a run terminates the shell and its compilation children.

While a native development worker is running, keep native `g5d` test builds out of its Cargo output directory. The worker launches each VM by executing its binary again (by default `target/g5-dev/g5d`); an integration test built there with `--no-default-features` replaces that executable with one lacking `vm-exec`, breaking subsequent workspace starts. Rebuilding can also invalidate the macOS signature. Prefer the Docker slices above, or give native tests a separate `--target-dir target/g5d-tests`. If the live binary was replaced, `just worker-host-start` rebuilds, signs, checks, and refreshes the worker; it may drain existing workspaces while restarting the host.

Focused Phase 5/6/7 reruns use `reuse_stack=auto` by default. Reuse requires a preparation receipt matching the current commit, tracked and untracked source bytes, executable permissions, local `.env`, effective validation options, container IDs and start times, image identities, SQLx migration state, and seeded fixture identity. Live service health and workspace existence are checked again. A mismatch causes fresh setup; explicit `reuse_stack=true` instead fails with preparation guidance. The seed script remains the owner of fixture identities: reuse loads its complete `.env.test` without guessing a primary repository or rewriting the file. A healthy stack without a receipt is insufficient. Editing source, changing images, restarting a container, missing fixtures, or incomplete preparation cannot silently qualify as a warm pass.

The safety check is deliberately conservative: even a test-only or documentation edit invalidates the candidate receipt. Docker layers and Cargo caches still survive and can accelerate the rebuild. Fixtures that tests mutate are refreshed before the applicable browser specs. An explicit existing spec path limits fixture preparation by its imports; grep, regex and other ambiguous selectors retain the broad fixture setup. Mutable test database contents are not a general checkpoint: receipts never skip previously passed test phases.

## Retention and failure output

Fresh failed runs retain the stack by default. `G5_CI_KEEP_STACK=always` also retains successful fresh runs; `G5_CI_KEEP_STACK=never` requests teardown on either outcome. Reruns against an already retained stack keep it unless `never` is set. `just ci-docker-down` removes this worktree's validation services, disposable volumes and generated service/API/browser image tags. Shared base images, the Rust runner, custom browser tags and BuildKit caches remain available. Cleanup uses Compose ownership labels and restores workers paused by the harness. Failed or interrupted setup retains diagnostic state, but cannot create a reusable preparation receipt.

Local builds record worktree/project ownership on their images. `just ci-docker-gc` previews labelled image tags left by deleted worktrees; `just ci-docker-gc true` removes those tags. Collection skips existing worktree directories, projects with retained containers, custom project names, and images without ownership labels. Run `ci-docker-down` before deleting a worktree to remove its volumes and retained diagnostic stack as well. Images created before ownership labels were added can still be removed by `ci-docker-down` from their original worktree. These commands remove tags without forcing deletion of images used by other containers; they do not globally prune Docker.

Build cache pruning is opt-in with `G5_CI_INTERPHASE_PRUNE=1`, after a fresh service build. Warm reruns do not prune Docker caches. Monitor Docker Desktop disk usage and use deliberate cache maintenance when needed.

Every invocation writes logs and JSON under `.dev/validation/runs/<timestamp>-<id>/`. Preflight records include command, candidate, duration, exit status and an exact rerun command. Runtime receipts include phase durations, available Rust test counts, failure lines/selectors, preparation identity and the focused rerun command. Missing test counts are not reported as zero. A focused selector with no reported passing tests fails validation; browser authentication setup does not count as executing the selected specs. Browser test artifacts are copied out before the ephemeral runner exits. The execution-ready timestamps separate prerequisite work from compilation and test execution. Failure summaries precede service log dumps. Interrupted receipts marked `running` are incomplete evidence and are never accepted as test success.

Browser fixtures receive `E2E_API_CONTAINER` explicitly. For manual host browser runs, source `scripts/resolve-e2e-url.sh` with the intended `G5_CI_COMPOSE_PROJECT`, or set `E2E_API_CONTAINER` yourself. The fixture fails with setup guidance rather than guessing a container in a different worktree.

## Validation evidence

The PTY process-group scan and its job-control child regression run only on
Linux. When changing terminal shutdown, run
`just ci-docker-5 'package(=g5d) and binary(=terminal_pty)'` and inspect the
receipt for a nonzero test count. The portable `/proc/<pid>/stat` parser unit
test also runs on developer machines. macOS terminal execution is a
development fallback: it can signal the shell's process group, but cannot
verify or terminate separate job-control groups in the session.

On the development laptop, the extracted architecture runner compiled its small dependency set and ran 42 assertions in approximately 4.5 seconds; the assertions themselves took 0.23 seconds. Warm preflight took 4.7 seconds with the initial harness regressions and 6.35 seconds with the expanded suite while an image build ran concurrently. Temporarily removing `g5d-dispatch` from the real local test inventory made the public `ci-docker-6` entry point fail in 1.08 seconds without invoking Docker; the source was restored immediately afterward. These measurements establish early static feedback, not application runtime correctness.

Regression tests exercise dirty/untracked source and config changes, missing and replaced containers, restarts, paused workers, changed images/schema/fixtures, missing receipts, competing runs, failed preparation, retention/teardown, selector quoting, fixture selection and failure-summary ordering. A separate real Docker/PostgreSQL probe verified unchanged state in 0.19 seconds and rejected a deleted workspace and a restarted worker. A no-op PostgreSQL update reproduced unstable ordering among branches named `main` in different repositories; the workspace fingerprint now normalizes that ordering while continuing to reject semantic fixture changes. That probe tests receipt mechanics, not application E2E behavior. For an application candidate, inspect `execution_ready_seconds`, `duration_seconds` and `test_counts` in its run receipts: the target is warm setup below 120 seconds, with compilation and actual test time reported separately. A first selected Rust run may still compile its test binary; compare a subsequent unchanged rerun to assess the warm compiler cache. Cloud CI optimization and broader Playwright parallelism are outside this change.

Agent execution-tree cleanup on Linux is covered by
`cargo test -p g5d --test dispatch_modules agent_execution_tree_is_reaped_before_settlement`.
This drives the real dispatcher and supervisor with synthetic providers and tools
that create separate sessions. It checks cancellation, pause/interrupt reasons,
outer limits, provider timeout and supervisor death, reap before settlement,
capacity reuse, stable stopped output and a still-running sibling. It is a Linux
process-boundary test, not live-provider or booted-VM evidence. macOS keeps the
existing process-group development fallback; it cannot establish descendant-tree
cleanup across separate groups or sessions.
