# Full-stack preview fails when Cargo.lock drifts from the guest image

## Human request

On 2026-10-14 the human reported that the QA `full-stack` preview (workspace `admin-page`, preview task `d73f1da9…`) exited with code 1 after 14 s because the in-guest `just worker-host-build` could not download a crate the image had not baked: cargo failed with `Permission denied` opening `/usr/local/cargo/registry/cache/…/rustls-0.23.46.crate` as uid 1000, since the baked registry tree is root-owned `drwxr-xr-x`. The workspace had bumped rustls past the image's lock, which recurs whenever a workspace lock differs from the image's commit (agent dependency bumps, or main moving ahead of the deployed worker image); the same pattern affects the sccache directory. Only the cleanup step's stdout line was persisted for the failure.

Deliverables, as written:

1. Make in-guest cargo able to fetch crates the image did not bake, as uid 1000, without losing the warm cache. Preferred: make the baked registry (cache, index, src) writable by uid 1000 in `Dockerfile.g5d-g5dev` the way `/opt/ms-playwright` is, and check the sccache directory for the same problem. The alternative of a per-workspace `CARGO_HOME` is for the case where the image must stay untouched. Do not switch the preview to `--offline` or `--locked`-only; a lock newer than the image must work.
2. Persist a bounded stderr tail (for example the last 4 KiB) of the preview app into the preview task's outcome or task events when `g5d preview-serve` sees the app exit non-zero.

Verification: reproduce before the fix in a g5dev guest, then show that the download and build work; a contract test for the Dockerfile ownership guarantee and a test that a non-zero preview exit records the stderr tail; run the relevant CI lanes and report what could not be executed. Follow AGENTS.md: planning workflow first, update the scoped ontology blocks, implement, run ontology-review, open a PR against main.

## Agent interpretation

- The Dockerfile already carries the preferred fix: the final stage runs `chmod -R a+rwX` over the registry, git and sccache trees and proves uid-1000 writes in its gate. The image from commit `4eff3a2` includes it. In the live VM the directories are nevertheless `0755 root`, while files the same layer re-emitted (`.package-cache` `0666`) and `/opt/ms-playwright` (chmodded in the layer that created it) kept their modes. BuildKit records a chmod-only layer as directory entries with the new mode (verified with a probe image in the guest's dockerd). The host rootfs assembly (`g5d-foundation/src/oci/extract.rs::apply_files`) applies a directory's mode only when it creates the directory, so a later layer that only changes an existing directory's mode is dropped. That is the defect; the Dockerfile route cannot work until it is fixed, and once it is fixed the existing Dockerfile lines are sufficient. The sccache directory is the same case and is fixed by the same change.
- Reproduction in the implementing VM: with the image's `CARGO_HOME`, `cargo fetch` on this branch fails with `Permission denied` on `wasm-bindgen-shared-0.2.127.crate`, exactly as reported.
- "Without losing the warm cache": the baked registry stays the one cargo uses; the fix makes it writable rather than copying it.
- The stderr tail travels in the preview module's outcome frame, which the dispatcher stores as the attempt outcome's `result` and the API persists verbatim with the `invocation.outcome` task event; the guest keeps forwarding the same bytes to its stderr so the dispatcher's bounded log and the guest console are unchanged.
- Already-assembled rootfs templates carry the wrong modes; the template format version advances so hosts rebuild them from the cached layers.
