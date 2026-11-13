# Review record: CI image key and mirrored miss path

Governing block: [`g5.quality.delivery.ci.image-key`](ontology/concepts/deployment.g5.md#key-the-ci-image-on-its-toolchain-and-dependency-graph-only), refining [`g5.quality.delivery.ci.base-image-mirror`](ontology/concepts/deployment.g5.md#mirror-ci-base-images-by-content-key).

## Design basis

`build-ci-image` over the 55 most recent E2E builds: 43 hits (median 3.1 min, a pull) and 12 misses (22%; median 19.6 min, max 30.8), with the Rust lane missing on the same SHAs for the same durations, so each miss costs two worker-slots for 20–30 minutes. Of 834 main commits in 14 days, 31 touched key inputs: 15 `Cargo.lock`, 6 `.sqlx`, the rest manifests/build scripts/recipes. Checked by reading code: `Dockerfile.ci-builder` copies `.sqlx/` only to run its warm-up compile under `SQLX_OFFLINE=true`; every CI step runs against the checkout (`/workspace`), which carries the current `.sqlx`; no step uses `--offline`/`--frozen`; `cargo-fetch` runs at build time; `verify-rust-target-cache` only asserts the baked target directory exists. A query-cache change therefore behaves exactly like a source change, which the key already excludes, so `.sqlx` leaves the key.

The miss path previously built `g5/base` and `g5/base-rust` from scratch before cooking the warm tier; it now resolves them through the CI mirror (limited to those two tiers so the multi-gigabyte workspace-warmed tier is never pushed), identical in both pipeline files.

## Verification before the PR run

- `scripts/test-ensure-base-images-registry.sh` (now 6 cases; new: tier whitelist), `scripts/test-ensure-base-images-lock.sh`.
- `test_cloudbuild_waitfor_order.py` (new `test_ci_image_step_is_shared_and_keyed_on_toolchain_inputs`: executable lines identical across both pipelines, `.sqlx` absent from the key, toolchain inputs present, mirror whitelist on the miss path), `test_rust_e2e_lanes.py`, `test_ci_docker_contract.py`, `bash -n` on all 59 + 29 inline step scripts, `cargo test -p g5-e2e ci_e2e_lane_isolation`.
- Expected on this PR's first run: a key miss in both lanes (the hash formula changed), which exercises the mirrored miss path; `base` is a mirror hit, `base-rust` is built and published on this first miss, so the full miss-path saving shows on the next miss after that.

## Not done, with reasons

- Dropping `frontend-test` from `rust-release-artifacts`' waitFor (~2 min per build): the only vitest flake seen in this work (`WorkspacePageAlignment`) occurred in that loaded window, so the edge stays until the flake class is understood.
- Keying the image on toolchain inputs only (ignoring `Cargo.lock`/manifests) would remove most remaining misses, but the Rust lane's test/clippy compiles use the image's baked sccache, not the GCS cache, so drift would recompile on every build; it needs GCS sccache for all cargo invocations first.
- Letting one lane wait for the other's push on a miss halves pool usage but not wall time.
