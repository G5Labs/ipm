# Full-stack preview survives a lockfile newer than the guest image

## Ontology changes and governance

**Comparison base:** `main` at `fa422dfd` (working tree, 2026-10-14).

**Ontology changes:** added `g5.runtime.boot.image.rootfs` in `software_architecture/runtime.g5.md`; added `g5.preview.service.exit-diagnostics` in `features/preview_terminal.g5.md`; updated the evidence of `g5.scope.deployment.images.g5dev-toolchain` in `concepts/deployment.g5.md`. Intent: [chat intent](../../chat_intent/bake-permission/request.md).

## Decision

**Owner of the defect.** The request names the Dockerfile as the preferred place to fix uid-1000 writability, but the Dockerfile already does exactly that (`chmod -R a+rwX` over the registry, git and sccache trees, a world-writable package-cache lock, a uid-1000 write probe in the gate) and the live guest still shows `0755 root` directories next to `0666` files from the same layer. BuildKit records that chmod-only layer as directory entries carrying the new mode (probe image built in the guest's dockerd). The host's layer assembly (`extract.rs::apply_files`) applies a directory's mode only when it creates the directory, so a later layer's directory metadata is dropped while its files are not. The guarantee that was missing is **assembly fidelity**, owned by the runtime's image boot contract, not by the image recipe. Fixing it there repairs every image with the same shape (the sccache directory and `/usr/local/cargo`'s sticky bit included) and makes the Dockerfile's existing guarantee true in the guest; a Dockerfile-only workaround (recreating the trees in the chmod layer, or a per-workspace `CARGO_HOME` copy) would hide the assembly bug and either double the baked registry or leave the sccache directory unwritable.

**Assembly change.** `apply_files` sets the layer directory's permissions after populating it for existing directories too, with the same non-root setid strip. Ordering stays: children first, then the mode, so a read-only mode never blocks the layer's own writes. Ownership is still never preserved. Already-assembled templates are wrong; `TEMPLATE_FORMAT_VERSION` advances to 3 so every host rebuilds its template from the cached layers on the next start (no layer re-download, one assembly per host). The Dockerfile's gate keeps proving uid-1000 writes inside the image; a new sync test keeps the chmod line covering every cache tree the final stage copies from the prime stage, so a new cache path cannot ship root-only.

**Preview diagnostics.** `preview-serve` inherited the app's stdout and stderr, so the build error reached only the dispatcher's in-memory bounded log and the guest console. The guest now pipes the app's stderr through a reader that forwards every byte to its own stderr (so the dispatcher's log-byte cap and redaction are unchanged) and keeps the last 4 KiB. When the app exits with a failure, after cleanup, the guest emits the module protocol's `outcome` frame carrying the status, the tail, a truncation flag and the guest's own error message. The dispatcher stores the frame as the attempt outcome's `result`; the API persists the outcome verbatim on `task_attempts.outcome` and the `invocation.outcome` task event, so nothing on the API side changes. The frame passes the dispatcher's secret masking like every module event. A clean app exit and owner Close keep their current outcomes; the reader is detached and never awaited to EOF, because setsid daemons the profile starts (dockerd) keep the pipe open by design.

**Capacity and limits.** The tail is bounded at 4 KiB, well under the 1 MiB event bound; the forwarded stderr counts against the module's existing `max_log_bytes` exactly as the inherited stream did.

## Assumptions and acceptance

- The worker host image and `g5d` deploy together from this repository, so the extractor fix and any later image rebuild land in the same pipeline; until the host build deploys, QA previews keep failing on lock drift.
- Preview readiness, Close, lifecycle stop and cleanup behavior are unchanged; only the app-failure path gains a frame.
- The rendered activity view does not yet show the tail; this plan persists it (the request's deliverable) and leaves display to a follow-up.

## Verification plan

- `extract.rs` unit tests: a later layer's directory mode (including the sticky bit) reaches an existing directory; setid strip still applies for non-root.
- `end_to_end_tests.rs`: a mocked registry image whose second layer only changes a directory's mode materializes with that mode on both the direct and the template path; the v2 template is rebuilt.
- `preview_serve.rs`: tail bound and frame payload; `dispatch_modules.rs`: a preview app that writes stderr and exits non-zero yields a `failed` outcome whose `result.stderr_tail` holds the text through the real runner.
- `scripts/test_g5dev_cache_writability.py` in the Rust lane's image-build-script-contracts step.
- Reproduction: `cargo fetch` in the implementing guest with the image's `CARGO_HOME` fails with `Permission denied` before the fix; the fix cannot be executed on this VM's own rootfs (assembled by the deployed host), so the after-state is shown by the assembly tests and the cargo build under a tree with the corrected modes.

## Review outcome (independent pass, 2026-10-14)

- Confirmed the root cause independently: the tar extraction already records a directory-only entry's mode (so the layer cache needs no invalidation); only `apply_files` dropped it. Fixed defect from the review: the `### g5.runtime.bundle` metadata heading the ontology edit had displaced was restored.
- Closed gaps: `template_assembled_before_directory_mode_fidelity_is_rebuilt` seeds the v2 key deployed hosts hold; `api_task_lifecycle.rs::failed_preview_app_outcome_keeps_its_stderr_tail` proves the API persists the guest's result on the attempt; the block no longer promises owner-visible rendering (deferred) and states that a failed cleanup after a clean exit carries only its message (the request's deliverable is the app's non-zero exit); comments and metadata say that the full-stack profile's dockerd logs to a file rather than holding the pipe, and that the frame depends on nothing holding the module's stdout past the dispatcher's drain grace.
- Noted, not changed: under a non-root daemon a later layer can now re-emit an existing directory read-only, so a yet later layer's writes into it fail as they already did for directories created read-only (root hosts unaffected); recorded in the block's metadata.

## Codex review (PR #547, round 1)

- **P1, fixed.** A layer that adds `tmp/file` without a `tmp/` header makes the extraction synthesize the parent at a default mode; replaying that mode would have changed a lower `/tmp` from 1777 to 0755. The layer cache now records the directories each tar named (`.explicit-dirs`, written like `.complete` and required for a cache hit, so pre-existing cached layers are fetched again once per host); assembly replays a mode onto an existing directory only for a named directory. The block now says so; tests cover the unit, extraction and end-to-end paths.
- **P2, first declined, then fixed in round 7.** Customer stdout had always shared the module frame channel. CodeRabbit's merge-risk note argued the same sharing loses the outcome when a detached child holds the app's stdout past the dispatcher's drain grace; on Linux the execution owner reaps such a child with the module tree before the drain (the dispatcher test's detached sleeper is gone by then), so that loss needs an unreaped holder, but isolation removes the dependency by construction. The app's and the cleanup's stdout now pass through the guest as `output` frames (journaled exactly as raw lines were) and their stderr through the forwarder, so no customer process touches the dispatcher's channel; readiness is unchanged because the guest always emitted it itself.

## Codex review (PR #547, round 2)

- **P2, out of scope.** An image whose layer carries a root entry named `.explicit-dirs` would lose it to the cache sidecar. This is the same property `.complete` has had since the cache's inception (the cache owns two reserved names at the layer root). It is an unsurveyed reserved-name limitation: no image inventory was checked, the repository's own images do not create such an entry, and moving cache metadata outside the extracted tree is a cache layout change beyond this request. Recorded in the block's metadata.

## Codex review (PR #547, round 3)

- **P2, not applicable.** A layer entry for `.` or `./` never reaches `explicit_dirs`: `validate_relative_path` rejects an entry without a normal component before extraction records anything, as it did before this change. Root-mode handling is pre-existing and outside this request.

## Codex review (PR #547, round 4)

- **P2, out of scope.** A daemon older than this change reading a layer this one cached would copy `.explicit-dirs` into the guest root as a stray file (its `apply_files` skips only `.complete`). That needs a binary rollback over a persisted layer cache, and the effect is one small extra file at `/`; the newer daemon re-extracts layers the older one cached, so the forward path is unaffected. Versioning the cache namespace would force every host to refetch every layer again for a rollback-only cosmetic effect; not done.

## Codex and CodeRabbit review (PR #547, round 5)

- **Codex P2, fixed.** The extraction kept every directory header a layer repeated, so a small layer with many repeated headers would have grown the daemon's record and the `.explicit-dirs` sidecar without bound. `extract_gzipped_tar` now returns a set (each named directory once, in path order), the sidecar is encoded from it, and `extract_reports_only_the_directories_the_tar_recorded` repeats a header.
- **CodeRabbit minor, fixed.** On a drain timeout `run_cleanup` dropped its forwarders' handles, which only detaches the tasks, and `run_preview` aborted the app's forwarders after emitting the outcome; a process left holding a pipe could forward a line after the terminal frame. Both now drain under the grace and then abort and join whatever is still running, before the outcome is emitted (`drain_forwarders`, `stop_forwarders`; `stop_forwarders_ends_readers_whose_pipes_are_still_held`). Forwarders still run through cleanup, so cleanup-time output of an app descendant keeps landing as before.
- **CI, unrelated to this request.** The Rust lane failed once on `g5d-agent::port_scan::native_ipv4_and_ipv6_listeners_emit_open_and_close` ("close 127.0.0.1:0"): the scanner reads the namespace-wide table keyed on the port number alone, so a listener another concurrent test process bound on the released number masked the close. The test now keeps scanning for the close under a bound instead of reading one scan.

## Ontology diff

```diff
diff --git a/.g5/ontology/concepts/deployment.g5.md b/.g5/ontology/concepts/deployment.g5.md
index 0424a159..95b975a6 100644
--- a/.g5/ontology/concepts/deployment.g5.md
+++ b/.g5/ontology/concepts/deployment.g5.md
@@ -162,7 +162,7 @@ Development or validation work does not itself authorize deployment.
 
 - human-verified
 - [Warm toolchain request](../../chat_intent/g5dev-warm-toolchain/warm-toolchain.md) and the [design](../../pr_ontology_diff_design/g5dev-warm-toolchain/ontology-plan.md); evidence: QA agent tool results 2026-09-29 to 2026-10-08 (25 `rustup could not choose` results in ten workspaces, `cargo-fmt`/`cargo-clippy` "not installed" on the bundle toolchain, cold whole-workspace compiles over the ten-minute turn limit), `g5d-dispatch/src/modules/runner.rs:258` `env_clear` fed by `git.rs::process_environment`, and PR #335 leaving the image's own proxies first on the path.
-- Implemented in the recipe, unexecuted in a built image · `infra/docker/g5-dev-rust-launcher.sh` installed as the seven Rust entry points in `/usr/local/bin` and `/home/agent/.cache/ms-playwright` linked to the baked browsers (`infra/docker/Dockerfile.g5d-g5dev`); the uid-1000 `env -i` gate checks versions, fmt, clippy, a smoke crate that pulls one baked dependency offline (warm registry read, package-cache lock, `.global-cache` writable) and compiles with recorded sccache requests, and the browser lookup. Launcher contract test `scripts/test_g5dev_rust_launcher.py` (16 cases) runs in the Rust lane's `image-build-script-contracts`; independent review and repairs in the [review](../../pr_ontology_diff_design/g5dev-warm-toolchain/review.md). Phase 3 (`kaihui/guest-warm-tests`): the prime stage also cooks the test runner's crates with dev-dependencies and g5d under test-helpers, `just check` excludes g5d's libkrun default, and `scripts/test-parallel.sh` starts the baked test DB when the port is silent. Gaps: the image builds only in the deploy pipeline, so the gate first runs on the next main merge and the wider prime's warmth is measured only there; browser launch on Ubuntu 26.04 under Playwright 1.58's host-platform check and the single QA report of an unreadable baked registry file are verified only on a live VM.
+- Implemented · `infra/docker/g5-dev-rust-launcher.sh` installed as the seven Rust entry points in `/usr/local/bin` and `/home/agent/.cache/ms-playwright` linked to the baked browsers (`infra/docker/Dockerfile.g5d-g5dev`); the uid-1000 `env -i` gate checks versions, fmt, clippy, a smoke crate that pulls one baked dependency offline (warm registry read, package-cache lock, `.global-cache` writable) and compiles with recorded sccache requests, and the browser lookup. Launcher contract test `scripts/test_g5dev_rust_launcher.py` (16 cases) runs in the Rust lane's `image-build-script-contracts`; independent review and repairs in the [review](../../pr_ontology_diff_design/g5dev-warm-toolchain/review.md). Phase 3 (`kaihui/guest-warm-tests`): the prime stage also cooks the test runner's crates with dev-dependencies and g5d under test-helpers, `just check` excludes g5d's libkrun default, and `scripts/test-parallel.sh` starts the baked test DB when the port is silent. Gaps: the image builds only in the deploy pipeline, so the gate first runs on the next main merge and the wider prime's warmth is measured only there; browser launch on Ubuntu 26.04 under Playwright 1.58's host-platform check is verified only on a live VM. The QA reports of an unwritable baked registry (and the silently unwritable sccache directory the launcher degrades around) were the host's rootfs assembly dropping the image's chmod-only layer, fixed under [faithful rootfs assembly](../software_architecture/runtime.g5.md#assemble-the-rootfs-faithfully-to-every-layer) on 2026-10-14; `scripts/test_g5dev_cache_writability.py` keeps the image's uid-1000 writability lines in step with the cache trees the final stage ships.
 
 ### g5.quality.delivery
 
diff --git a/.g5/ontology/features/preview_terminal.g5.md b/.g5/ontology/features/preview_terminal.g5.md
index 55879375..82748a9e 100644
--- a/.g5/ontology/features/preview_terminal.g5.md
+++ b/.g5/ontology/features/preview_terminal.g5.md
@@ -81,6 +81,14 @@ Closing a preview is a bounded graceful stop owned by the preview task: the gues
 
 Governing: [one preview at a time](#close-the-running-preview-before-starting-another), [owner Stop reliably ends an attempt](agents.g5.md#owner-stop-reliably-ends-any-agent-attempt) for the bounded interruption shape.
 
+### Record why the application exited
+
+[D · g5.preview.service.exit-diagnostics]
+
+When the preview application exits on its own with a failure (non-zero status or a signal), the preview task's persisted outcome records that status together with a bounded tail of the application's stderr (the last 4 KiB) and the guest's own error, so why a preview failed survives the VM and can be shown to the owner. A failed cleanup after a clean application exit carries only its message. The application's stdout lines are still journaled as output and its stderr still reaches the dispatcher's bounded log, but neither the application nor its cleanup ever writes to the guest's protocol channel: a line they print cannot be read as a frame, and a process they leave behind cannot hold the channel open past the guest's exit, so the outcome always arrives. The tail passes the same secret masking as every module event. Reason: a QA full-stack preview failed in 14 s with only the cleanup step's stdout line persisted; the build error was visible only in the dispatcher's in-memory log.
+
+Governing: [Close runs the profile's cleanup](#close-runs-the-profiles-cleanup) for when the application's own exit still runs cleanup; [bounded output](agents.g5.md#bound-recorded-output-and-provider-lines) for the log-byte cap the forwarded stderr still counts against.
+
 ### Preview transport preserves owner authority
 
 [D · g5.preview.transport]
@@ -170,6 +178,12 @@ Governing concepts: Live access follows [current-owner authority](../concepts/se
 - From the [preview close chat intent](../../chat_intent/preview-close-cleanup/request.md) and its [plan](../../pr_ontology_diff_design/preview-close-cleanup/ontology-plan.md).
 - ~85% · Guest: `g5d/src/cli/preview_serve.rs::run_preview` runs the app in its own process group. Owner Close (SIGTERM) stops it on a 10 s bound and self-exit stops residual descendants before `cleanup_command`; cleanup uses `customer_command` (same shell, cwd, env scrub, PATH, PORT), is bounded to 60 s and cannot leave background descendants behind. Workspace lifecycle stop (SIGINT) immediately stops the app group and skips cleanup (`close_*`, `an_app_that_exits_on_its_own_still_runs_cleanup`, `self_exit_stops_background_app_children_before_cleanup`, `successful_cleanup_does_not_leave_background_children`, `lifecycle_stop_kills_the_app_group_without_running_cleanup`). Dispatcher: `g5d-dispatch/src/modules/runner.rs::module_stop_mode` selects graceful Close only for durable `owner_cancel`; other preview cancellation sends bounded SIGINT, while `modules::cancel` forgets the service port. Real dispatcher paths prove owner Close runs cleanup and reports `close = clean`, while `admission.close` kills the app without cleanup or close classification (`backend/g5d/tests/dispatch_modules.rs::workspace_preview_owner_close_is_graceful_at_dispatch_boundary`, `workspace_preview_ready_http_ws_and_stop_are_fenced_at_dispatch_boundary`). API: `WorkspacePreviewView.closing`/`close` and `workspace_preview_target` refusing streams while `cancel_requested` (`api_task_lifecycle.rs::closing_preview_is_visible_until_the_guest_reports_how_it_closed`). UI: `PreviewControl.tsx` Close preview, closing state and the failed-close notice (`PreviewControl.test.tsx`). Profiles carry `cleanup_command` through the API parser, DTOs, create route, YAML renderer and guest config; `.g5/preview_profiles/full-stack.yaml` cleans up with `just dev-down` and starts dockerd under `setsid` so the daemon outlives the app-group stop (`preview_serve.rs::a_daemon_started_under_setsid_outlives_the_app_stop_for_cleanup`). Gaps: the guest's scanned-port admission can still reach a non-TSI app for the ≤10 s owner-close app stop; the Playwright flow (`workspace-preview.spec.ts`, fixture `scripts/seed-test-workspace.sh` writes `e2e-preview-cleanup.txt`) and the G5 `dev-down` cleanup were not executed in the implementing VM.
 
+### g5.preview.service.exit-diagnostics
+
+- human-verified
+- [Preview registry permission request](../../chat_intent/bake-permission/request.md) and its [design](../../pr_ontology_diff_design/bake-permission/ontology-plan.md), 2026-10-14 (deliverable 2).
+- Implemented · `preview_serve.rs::run_preview` pipes the app's stderr through a forwarding reader that keeps the last `STDERR_TAIL_BYTES` (4 KiB) and, after cleanup, emits the module `outcome` frame with `app_exit_code`/`app_signal`, `stderr_tail`, `stderr_tail_truncated` and `message` when the app failed; the dispatcher stores it as the attempt outcome's `result` and the API persists it verbatim (`task_lifecycle.rs::apply_outcome`). Unit tests cover the tail bound and the frame (`preview_serve.rs::app_failure_outcome_carries_the_bounded_stderr_tail`, `stderr_tail_keeps_only_the_last_bytes`); the dispatcher boundary test `dispatch_modules.rs::workspace_preview_app_failure_records_its_stderr_tail_in_the_outcome` proves the persisted result through the real module runner. `api_task_lifecycle.rs::failed_preview_app_outcome_keeps_its_stderr_tail` proves the API stores the result on the attempt outcome. `forward_stdout` re-emits app and cleanup stdout as `output` frames (journaled as `module.output` exactly as raw lines were) and `run_cleanup` pipes cleanup output the same way, so no customer process touches the dispatcher's channel; `dispatch_modules.rs::workspace_preview_app_output_cannot_preempt_or_hold_the_outcome_channel` proves a frame-shaped app line arrives as output and a detached child holding the app's stdout does not delay or lose the outcome (Codex round-1 P2 and CodeRabbit's merge-risk note on PR #547). The forwarders are drained under a 250 ms grace and then aborted and joined before the outcome frame (`drain_forwarders`, `stop_forwarders`, `stop_forwarders_ends_readers_whose_pipes_are_still_held`), so a process left holding a pipe cannot add an output frame after it. Gaps: the activity view does not yet render the tail (deferred); not executed against QA.
+
 ### g5.preview.transport
 
 - human-verified
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 5a895029..2a5122a5 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -214,6 +214,14 @@ Install the placement-minted incarnation token at boot, while provider/repositor
 
 Use image pins only from committed restored code-member HEADs, falling back to the organization default only when no pin exists and rejecting malformed, mutable or conflicting pins.
 
+## Assemble the rootfs faithfully to every layer
+
+[D · g5.runtime.boot.image.rootfs]
+
+The rootfs a VM boots carries every layer's recorded content and metadata in layer order, including a later layer that only changes the mode of a directory an earlier layer created, so an image's declared ownership guarantees (which paths a non-root guest may write) hold in the guest exactly as they hold in the image. Only metadata the layer recorded counts: a parent directory the layer implies through a nested entry without naming it carries no mode and leaves the lower layer's directory as it was. Assembly still never preserves ownership and strips setid bits under a non-root daemon. When assembly semantics change, the template format version advances so hosts rebuild already-assembled templates from their cached layers instead of serving the old result. Reason: the development image makes its baked cargo registry and compile cache writable for uid 1000 in a layer after the one that ships them; assembly created those directories from the earlier layer and ignored the later mode, so every full-stack preview whose lockfile needed a crate the image had not baked failed with `Permission denied` (QA, 2026-10-14).
+
+Governing: [image selection](#select-images-from-committed-member-intent) chooses the image; the [development image's toolchain](../concepts/deployment.g5.md#make-the-development-images-toolchain-work-in-the-cleared-guest-environment) relies on this for its uid-1000 writability.
+
 ## The platform supplies its complete runtime
 
 [D · g5.runtime.bundle]
@@ -572,6 +580,12 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 - [C083–C084](../../uplift/g5-lean/candidates/contracts.md), source binding [X09](../../uplift/g5-lean/investigations/runtime-security.md#x09--provenance-is-implemented-its-normative-scope-must-remain-precise), microVM boundary [RF01](../../uplift/g5-lean/refinement/reference-coverage.md#rf01--a-configured-microvm-boundary-must-fail-visibly-never-silently-degrade).
 - ~90% · image policy and VM boot spec; conflicting-pin/image-pull behavior unexecuted.
 
+### g5.runtime.boot.image.rootfs
+
+- agent
+- [Preview registry permission request](../../chat_intent/bake-permission/request.md) and its [design](../../pr_ontology_diff_design/bake-permission/ontology-plan.md), 2026-10-14; evidence: QA preview task `d73f1da9` stderr, the live guest's `0755 root` registry beside the same layer's `0666` lock files, a BuildKit probe showing the chmod-only layer as directory entries with the new mode, and `cargo fetch` reproducing the failure in the implementing VM.
+- Implemented · `backend/g5d-foundation/src/oci/extract.rs::apply_files` applies a layer directory's mode after populating it whether or not the directory already existed (`apply_layer_applies_a_later_layers_directory_mode_to_an_existing_directory`, sticky bit included; the non-root setid strip still applies); `template.rs::TEMPLATE_FORMAT_VERSION` 3 rebuilds templates assembled before the change (`end_to_end_tests.rs::later_layer_directory_mode_reaches_direct_and_template_rootfs`, `template_assembled_before_directory_mode_fidelity_is_rebuilt` seeds the v2 key deployed hosts hold). The layer cache records which directories each layer named, each once however often the tar repeated its header (`EXPLICIT_DIRS_MARKER` sidecar written by `cache.rs::write_reserved_file`, required by `is_complete`, so layers cached before it are fetched again once per host); a synthesized parent keeps the lower mode (`extract_reports_only_the_directories_the_tar_recorded`, `apply_layer_keeps_a_lower_layers_mode_for_a_synthesized_parent_directory`, and the `tmp/marker` case of the end-to-end test; Codex P1 on PR #547). As with `.complete` since the cache's inception, a cache-internal name at the layer root replaces an image entry of the same name: an unsurveyed reserved-name limitation (no image inventory was checked for a root-level `.explicit-dirs`; the repository's own images do not create one), accepted as out of scope (Codex P2, round 2). Gaps: not executed on a QA host; the deployed worker image must carry this host build before the preview works there. Under a non-root daemon a later layer can now re-emit an existing directory read-only, so a yet later layer's writes into it fail as they already did for directories created read-only; root hosts are unaffected.
+
 ### g5.runtime.bundle
 
 - agent
```
