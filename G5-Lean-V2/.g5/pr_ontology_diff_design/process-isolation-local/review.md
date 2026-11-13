# Review — preview up means the worker is attached (PR #505)

Independent Codex (gpt-6-astra, read-only) rounds on the follow-up to #487, each
repaired, tested and recorded here before the next round.

## Round 1 — 2026-10-13

Verdict: changes requested. Findings and repairs:

- **P1 · Starting the edge from `dev-up` could recreate the API without its configuration.** `docker compose up -d edge` reconciles `g5-api` too, and the secrets, overlays and derived admission flag live in the `dev-stack-up` child. Repair: the edge is published by `dev-stack-up` itself in a `G5_DEV_EDGE_ONLY=1` phase (same identity, overlays and secrets) with `--no-deps`; the regression script asserts the phase starts the edge alone, builds nothing and never recreates the other services.
- **P1 · The copy fallback nested the repository inside a partial destination.** `cp --reflink=always` creates the destination and may leave files before failing; `cp -a` into it would nest. Repair: `copy_bare_repo` removes the partial destination first; a test pre-creates a partial `.git` with an empty `HEAD` and asserts the copy replaces it without nesting; the strict-mode assertion no longer assumes a refused reflink leaves nothing.
- **P2 · The host's "started" log line does not prove registration.** Repair: `scripts/wait-for-worker-host.sh` polls the selected API until a ready, live host is listed, and `dev-up` runs it before publishing the edge; tested with a curl stub for a ready and a pending host.
- **P2 · A rerun kept the previous edge published while the API was replaced.** Repair: the deferred phase removes an existing edge (`rm -sf edge`) before `up`; asserted in the regression script, and a plain start is asserted not to.
- **P2 · Devcontainer forwarding ran before the edge existed.** Repair: the edge-only phase runs the forwarding again after the edge is healthy.
- **P2 · `worker-host-build` left a live macOS host spawning an unsigned binary.** Repair: the recipe signs the rebuilt binary with the HVF entitlement in microvm mode; asserted statically.
- **P2 · The frontend profile's port collided with a developer's Vite server.** Repair: port 15174; asserted.
- **P3 · The job-count test accepted any count.** Repair: the test stubs `nproc` to 7 and asserts `--jobs 7`, and asserts `G5_DEV_HOST_CARGO_JOBS=3` overrides it.

Executed after the repairs: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-worker-host-env-process-build.sh`, `g5d-foundation` `workspace::restore` tests, fmt and clippy.

## Round 2 — 2026-10-13

Verdict: changes requested, no P1. Findings and repairs:

- **P2 · A mode switch rebuilt the shared binary under a live microVM host.** `worker-host-build` replaced `target/<profile>/g5d` without `vm-exec` while the old host still spawned from it. Repair: the build recipe stops a tracked host whose recorded mode differs from the configured one (and, slug-less, the per-org instances sharing the path) before building, as `worker-host-start` already did on the mismatch branch.
- **P2 · A failed edge removal was masked.** Repair: `rm -sf edge` propagates failure (absence still succeeds); the regression script makes the removal fail and asserts no `up` follows.
- **P2 · The registration poll read one page.** The host list keeps every retired host, newest first, fifty per page. Repair: `wait-for-worker-host.sh` filters to `status=ready` and follows pages until a live host is found or the list ends; tested with a full first page of stale hosts and the live host on page two, and the negative case.
- **P3 · The fallback tests did not force the fallback.** Repair: the decision after a strict reflink attempt is a pure function (`fallback_after`), tested with injected refused, other-failure and success results for both opt-in values; the copy itself keeps its own test.

Executed after the repairs: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-worker-host-env-process-build.sh`, `g5d-foundation` `workspace::restore` tests, fmt and clippy.

## Round 3 — 2026-10-13

Verdict: changes requested, no P1. Findings and repairs:

- **P2 · The build guard missed live per-org hosts.** Repair: `worker-host-build` inspects the default and every per-org instance directory for a live host whose recorded mode differs, and stops every instance (`worker-host-stop`, slug-less) before the shared binary is replaced; asserted statically.
- **P2 · The registration poll could accept the host just stopped.** A stopped host reads as live until its heartbeat ages out. Repair: `dev-up` passes the id `worker-host-start` recorded (`infra/docker/demo-worker.env`) and the wait accepts only that host, ready and live; tested with the old host live and the new one pending, and with both live.
- **P2 · A mode-changing rerun stopped the worker before withdrawing the edge.** Repair: `dev-up` first runs `dev-stack-up`'s `G5_DEV_WITHDRAW_EDGE=1` phase, which removes a published edge under the same identity and does nothing else, before the build can stop a host; the step order and the phase's single command are asserted.

Executed after the repairs: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-worker-host-env-process-build.sh`, fmt.

## Round 4 — 2026-10-13

Verdict: changes requested, no P1; one P2, the third refinement of the build guard: `worker-host-build <org>` sourced the helper, which exported the per-org directory, and the slug-less stop inherited it, so the default host stayed live. Repair: the caller's default directory is kept before the helper runs, scanned, and handed to the stop (`env -u G5_DEV_HOST_DIR` otherwise). The guard is now exercised for real: a fixture with two live processes that `ps` reports as `g5d host`, a default and a per-org instance in microvm mode, asserts that a process-mode build of either instance records the slug-less stop with the default directory before the build, that a caller's default directory reaches the stop, and that a same-mode build stops nothing.

Executed after the repair: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-worker-host-env-process-build.sh`.

## Round 5 — 2026-10-13

Verdict: changes requested, no P1; two P2s. (1) The valid org slug `demo` produced a per-org credential file named like the default instance's (`demo-worker.env`), which `dev-up` reads for the host it started; a later default rerun would then wait for the wrong host. Repair: per-org files are `org-<slug>-worker.env`; asserted. (2) Exporting `CARGO_BUILD_JOBS` for the host build would enter the guest's sccache hashes (the image primes without it) and miss the warm cache. Repair: the count travels as `cargo build --jobs`; the test asserts the argument and that the variable stays unset.

Executed after the repairs: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-worker-host-env-process-build.sh`.

## Convergence

Five rounds. The two P1s of round 1 (API recreated without its configuration; copy fallback nesting) did not recur. Rounds 2 to 5 found P2s only, all in two families (the edge's publication window on reruns and mode switches, and the build guard over hosts sharing one binary), each closed with executed regressions. The loop stops here: no high blocker remains, and the later findings refine repaired shapes rather than open new classes.

## CodeRabbit on PR #505 — 2026-10-13

Folded in after round 5: the plain copy reports `PlainCopyFailed` rather than a reflink failure; `G5_DEV_HOST_CARGO_JOBS` is validated as a positive integer and otherwise ignored; the withdraw phase runs before secrets are loaded, so a secrets failure cannot leave the edge published; an edge that never becomes healthy in the edge-only phase is removed again; the step-order checks search the `dev-up` recipe body alone and require each step once; the regression script has one exit handler that ends the fake hosts and removes every fixture; the status line names the unverified flows and the measurement is labelled as the previous order's.
