# Review: warm Rust toolchain and browser surface in the g5dev guest

Reviewed commit `294f8fd3` on `kaihui/g5dev-warm-toolchain` (PR #386) against main
`795d1ab6`, by an independent reviewer (fresh context, no implementation history) with
the author's self-review folded in afterwards. Design:
[ontology-plan.md](ontology-plan.md). Governing block:
`g5.scope.deployment.images.g5dev-toolchain`.

## Checks executed

- `scripts/test_g5dev_rust_launcher.py` (12 cases before repair, 16 after), on macOS as a
  non-root user; `env -i`, stdlib only.
- `scripts/test_verify_base_digest_identity.py` (17), `scripts/test_cloudbuild_waitfor_order.py`
  (17), `scripts/test-image-fetch-retry.sh`, `scripts/verify-base-digest-identity.sh --static-scan infra/docker`.
- `scripts/test_g5dev_browser_gate.py` skips off Linux; its parsed slice of the Dockerfile
  (`find /opt/ms-playwright` to `g5-dev-testdb-start`) is untouched.
- The new gate `RUN` and the launcher install loop joined as the Dockerfile parser joins
  them and passed `bash -n`.
- Not executed: the image build itself (amd64, deploy pipeline only) and any live VM.

## Initial findings (before repair)

| # | Severity | Finding | Location |
|---|---|---|---|
| 1 | Medium, gate gap | The smoke crate had no dependency, so the uid-1000 gate never read the baked registry, took `.package-cache`, or touched cargo's root-owned `.global-cache` tracker; the block's "proves the toolchain" did not cover the path QA reported unreadable (#4). | `Dockerfile.g5d-g5dev` gate RUN; chmod RUN |
| 2 | Low, confirmed | A caller `SCCACHE_DIR` that does not exist yet failed the `-d` test and silently disabled sccache, where sccache itself would have created it. | `g5-dev-rust-launcher.sh` |
| 3 | Low, confirmed | The degrade branch was only covered by the missing-binary case on CI, because the unwritable-directory test skips as root and `gcr.io/cloud-builders/gcloud` runs as root. | `scripts/test_g5dev_rust_launcher.py` |
| 4 | Low, design | A caller setting `RUSTC_WRAPPER=sccache` itself received neither `SCCACHE_DIR` nor `CARGO_INCREMENTAL=0`, so sccache ran against a cold `~/.cache/sccache` and refused incremental builds. | `g5-dev-rust-launcher.sh` |

No confirmed defect that breaks the build or the runtime.

## Repairs and rechecks

1. The smoke crate now declares `cfg-if = "1"` (baked by the chef cook) and builds with
   `--offline`, so the gate reads the index cache and a `.crate` file and takes the
   package-cache lock as uid 1000; `.global-cache` is pre-created world-writable beside
   `.package-cache`. Rechecked by `bash -n` of the joined RUN and the contract test's
   gate assertions (`cfg-if`, `cargo build --quiet --offline`).
2. The launcher `mkdir -p`s the resolved cache directory before the writability test.
   Rechecked: `test_caller_sccache_dir_is_created_when_missing`.
3. Root-proof degrade case added: a regular file as `SCCACHE_DIR` fails `-d`.
   Rechecked: `test_cache_path_that_is_not_a_directory_degrades_to_plain_compilation`.
4. When the wrapper resolves to `sccache` (caller or default), the launcher fills the
   unset `SCCACHE_DIR` and `CARGO_INCREMENTAL=0`; another explicit wrapper is left alone.
   Rechecked: `test_explicit_sccache_wrapper_gets_its_companions`,
   `test_other_explicit_wrapper_is_left_alone`.

All 16 launcher cases pass after repair.

## Commitments

| Commitment | Actual path | Counterexample / check | Result |
|---|---|---|---|
| Every Rust entry point on the standard path is a launcher that fills unset toolchain keys | `/usr/local/bin/{cargo,…}` symlinks to `g5-dev-rust-launcher`, `$0` dispatch, exec of `/usr/local/cargo/bin/<tool>` | Cleared env, each of the seven names; direct invocation; missing proxy | Pass (inspection + contract test); recursion impossible because the proxy is resolved from the image constant, not the caller's `CARGO_HOME` |
| Caller values win | `${VAR:-default}` and `${RUSTC_WRAPPER+set}` | Caller `RUSTUP_HOME`/`CARGO_HOME`/`CARGO_INCREMENTAL`; `RUSTC_WRAPPER=""`; another wrapper | Pass (contract test) |
| Unwritable cache degrades instead of failing | `-d && -w` after `mkdir -p`; missing `sccache` binary | Regular file as cache; missing binary; unwritable dir (non-root only) | Pass (contract test) |
| Browsers at Playwright's default lookup path | `/home/agent/.cache/ms-playwright -> /opt/ms-playwright`; Playwright 1.58.2 `registry/index.js` uses `os.homedir()/.cache/ms-playwright` when the env var is unset; host platform `ubuntu26.04-x64` maps to `chrome-linux64/chrome`, the install-only refusal does not apply at launch | Inspection of `frontend/node_modules/playwright-core`; dispatcher `ensure_directory` accepts an existing dir, `chown_tree` uses `lchown`, OCI extraction recreates symlinks verbatim | Pass by inspection; launch on the 26.04 glibc remains a live-VM check |
| Gate proves the toolchain under the runtime's environment and identity | `runuser -u ubuntu -- env -i PATH=… HOME=/home/agent bash -c '…'` | Would fail without the launchers (bare proxies) or without the home link | Unexecuted: runs first in the deploy build; `bash -n` and text assertions only |
| Image tools keep winning over the bundle (`g5.runtime.bundle.tools`) | Launchers live in `/usr/local/bin`, which precedes `/opt/g5/bin` | `cargo nextest` resolves `/opt/g5/bin/cargo-nextest` through the launcher's cargo | Pass by inspection |

## Codex (gpt-6-astra) review rounds

Read-only `codex exec` rounds on the PR checkout; each round's findings were repaired,
re-tested and committed locally before the next.

| Round | Head | Verdict | Findings | Repair |
|---|---|---|---|---|
| 1 | f8c047ff | APPROVE (2 × P2, both confirmed) | (a) The gate accepted a compile whose cache writes failed: sccache tolerates write errors silently, so a request count alone passes a broken cache. (b) `${VAR:-default}` overwrote an explicitly empty `CARGO_HOME`/`RUSTUP_HOME`, contradicting caller precedence (cargo falls back to `$HOME/.cargo` on empty). | (a) The gate now checks `Cache write errors 0` and the `Local disk` location after the first build, drops the target, rebuilds and requires `Cache hits ≥ 1`. (b) Unset-only defaults (`${VAR-default}`) everywhere; an empty `SCCACHE_DIR` degrades. Tests: `test_explicitly_empty_homes_are_the_callers_choice`, `test_explicitly_empty_sccache_dir_degrades_to_plain_compilation`, gate assertions on the hit/write-error checks and their order. 18 cases pass. The commitment row "Caller values win" now includes explicit empties. |
| 2 | 329c4ba1 | APPROVE, no findings | Shell parsing, the four Dockerfile contract tests and local Rust dispatch probes passed in the reviewer's sandbox; the launcher fixture tests could not run there (read-only sandbox) and pass locally. | None; converged. |

## Phase 3 follow-up (branch `kaihui/guest-warm-tests`): Codex rounds

| Round | Head | Verdict | Findings | Repair |
|---|---|---|---|---|
| 1 | 5fa7fada | APPROVE (3 × P2, confirmed) | (a) One combined cook warmed a dependency feature mix (for example `serde_json/raw_value`) that no runner invocation uses, so the shared-crate unit group would recompile; check-mode artifacts were not warmed at all. (b) The test-DB autostart probed `G5_PORT_POSTGRES` but the helper reads `G5_TESTDB_PG_PORT`, so a shifted port started the helper on 5432 anyway. (c) `eval "$(g5-dev-testdb-start)"` masked a helper failure as an empty eval and ran the DB-dependent suite regardless. | (a) One cook per package set the runner invokes (compile phase, four unit groups, two integration shapes) plus `--check` cooks for both `just check` shapes; every command validated locally against the real recipe. (b) The helper receives the probed Postgres and Redis ports. (c) The helper's stdout is captured through a checked assignment; a failure records a FAIL result and skips the g5-api integration suite with a visible reason. |
| 2 | 45c87e32 | REQUEST_CHANGES (1 × P1, 1 × P2, confirmed) | (a) The prime exported `CARGO_BUILD_JOBS` and `CARGO_TARGET_DIR`; sccache folds every `CARGO_*` variable rustc inherits into its key, and the guest's cleared environment has neither, so warmed objects (including the pre-existing preview cooks') would miss at runtime. (b) With a shifted Redis port the autostart exported only `G5_REDIS_URL`, while the harness reads `REDIS_URL`. | (a) Jobs and target dir go through `cargo chef cook -j … --target-dir …` (a `cook` shell function); no `CARGO_*` export remains in the prime. Joined RUN passes `bash -n`; cook flags confirmed in cargo-chef 0.1.77 help. (b) After the helper's exports, the runner maps `TEST_DATABASE_URL` and `REDIS_URL` from the helper's values unless the caller set them. |
| 3 | 1ec91759 | REQUEST_CHANGES (1 × P1, 1 × P2) | (a) A stage-scoped build ARG (`CARGO_CHEF_VERSION`) is an environment variable during the prime's RUN and would enter the sccache keys. (b) When Postgres was already listening the whole mapping block was skipped, so a shifted Redis port never reached `REDIS_URL`. | (a) The prime unsets it and fails the build if any `CARGO_*` other than `CARGO_HOME`/`CARGO_INCREMENTAL` is present; more decisively, the final-stage gate now ships the prime's cargo config, builds the smoke crate under it and requires a cache hit on the FIRST build, which proves prime-to-guest key equality (rustflags, profile, `CARGO_*` environment) rather than arguing about it. (b) The harness names are mapped after the autostart block from the configured ports, whether or not this run started the services. |
| 4 | ab4f20c6 | REQUEST_CHANGES (1 × P1, 1 × P2, confirmed) | (a) The new first-build hit gate would have failed the deploy: cargo discovers `.cargo/config.toml` from the working directory, not `--manifest-path`, so `-Awarnings` was missing, and the smoke crate's default dev profile (`debug = 2`) differs from the workspace's (`debug = 1`), so `cfg-if`'s key could never match. (b) With an explicit `DATABASE_URL` and a silent local port, the autostart overwrote the caller's target with the helper's database. | (a) The gate `cd`s into the smoke crate, builds without `--manifest-path`, and appends the workspace manifest's `[profile.dev]` section (shipped from the planner beside the cargo config) before building; the contract test pins the cd, the absence of `--manifest-path`, and the config and profile preceding the build. (b) The autostart runs only when neither `TEST_DATABASE_URL` nor `DATABASE_URL` is set; explicit URLs are never overwritten. |
| 5 | 5d175d1f | APPROVE, no findings | Seven shell scenarios, four Dockerfile contract checks, syntax checks and package coverage passed in the reviewer's sandbox. | None; converged. The deploy build, guest hit rates and cache growth are measured on the first main merge. |
| after merge | b0007247 | QA deploy 14052d2e FAILED at `build-g5d-g5dev` | The fatal first-build hit assertion fired: nine compile requests, zero hits, while the prime had written 1,387 Rust objects (863 MiB of a 10 GiB cache, so no eviction). Reproduced locally with the real recipe: the smoke crate's `cfg-if = "1"` resolved offline to cfg-if 1.0.5, not the lock's 1.0.4, because `cargo install cargo-chef` earlier in the image leaves newer crates in the same registry cache and the offline resolver takes the newest present; the prime never cooked that version. sccache's key does not depend on cwd, `-L`, `--out-dir` or uid (verified: registry crates compile with cwd = their package dir). | Hotfix PR #395 turns the assertion into a logged warning with full statistics so deploys resume. Follow-up (`kaihui/g5dev-gate-pin`): the planner ships `Cargo.lock` and the gate pins the smoke dependency to its exact locked version; verified locally (first build: 1 hit, 0 misses). The assertion returns to fatal once a deploy log shows the hit. Lesson recorded: an unmeasured cache-hit assertion must not be fatal in the deploy image build; reproduce prime-to-guest with `cargo chef cook` locally first. |

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / blockers |
|---|---|---|---|
| `g5.scope.deployment.images.g5dev-toolchain` | The gate has not executed in a built image | Watch the `build-g5d-g5dev` step of the first `g5-deploy-qa` run after merge; probe a fresh Base VM through an agent turn | Image is amd64 and deploy-only; no local build |
| same | Chromium built for 24.04 running on the 26.04 glibc | Part of the same live-VM probe (`npx playwright test` on one spec) | Documented as working for PNC (#2242), not re-executed here |
| same | Deploy cost | The launcher `COPY` lands before the chef stages, so the next deploy re-runs the dependency cook once | Expected, not a defect |
