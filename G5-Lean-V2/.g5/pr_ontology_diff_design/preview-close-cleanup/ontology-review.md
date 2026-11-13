# Preview close and cleanup — implementation review

Self-review by the implementing agent (same context), 2026-10-05, against the
[plan](ontology-plan.md) and [chat intent](../../chat_intent/preview-close-cleanup/request.md).
Inventory: the plan's `Ontology changes and governance` (amended
`g5.preview.service.replace`, new `g5.preview.service.close`) plus the changed files
(`git status` on branch `preview-test`).

## Commitments and evidence

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| One preview at a time: a start beside an active preview is refused; the same key replays | `task_lifecycle.rs::start_workspace_preview` (EXISTS over queued/running/waiting → `G5Error::Conflict`); route returns the view only | `api_task_lifecycle.rs::second_preview_start_is_refused_while_one_is_active_and_non_owners_cannot_start_or_cancel` (409, first task still queued, replay 200) · `closing_preview_is_visible_until_the_guest_reports_how_it_closed` (409 while closing, 200 after settlement) | runtime: pass |
| Loss paths still cancel queued previews | `cancel_queued_workspace_previews`, `workspace_vm_release.rs` untouched | inspection; `pause_rejects_a_queued_preview_without_stranding_it_in_owner_stop` still passes | pass |
| Close is a bounded graceful stop owned by the preview task | dispatcher `runner.rs::stop_group(closing=true)`: SIGTERM, 90 s grace, SIGKILL; `preview_serve.rs::run_preview` stops the app group (SIGTERM, 10 s, SIGKILL) then runs cleanup (60 s) | `runner.rs` `a_closing_preview_gets_sigterm_and_its_exit_classifies_the_close`; `preview_serve.rs` `close_stops_the_app_group_then_runs_cleanup_with_the_launch_env`, `close_kills_an_app_that_ignores_sigterm_within_the_bound`, `close_reports_a_failed_or_overrunning_cleanup`; real dispatcher + preview-serve: `g5d/tests/dispatch_modules.rs::workspace_preview_ready_http_ws_and_stop_are_fenced_at_dispatch_boundary` asserts `detail.close == "clean"` and process exit | unit: pass; integration: pass |
| Cleanup uses the launch command's shell, cwd, env scrub, PATH and PORT | `preview_serve.rs::customer_command` shared by launch and cleanup | `close_stops_the_app_group_then_runs_cleanup_with_the_launch_env` asserts cleanup ran in the cwd with `$PORT` | pass |
| Cleanup also runs after a self-exit | `run_preview` runs `run_cleanup` on `AppEnd::Exited` | `an_app_that_exits_on_its_own_still_runs_cleanup` (failed app keeps its failure; clean exit + cleanup succeeds) | pass |
| Close admits no new stream | API: `workspace_preview_target` refuses when `lifecycle.cancel_requested` (503); guest: `modules::cancel` forgets the service port | `closing_preview_is_visible…` (mint → 503 while closing); `runner.rs::cancelling_a_preview_drops_its_port_from_the_http_gate` | pass |
| Closing is visible; a failed or killed close is reported | `WorkspacePreviewView.closing`/`close`; `PreviewControl.tsx` closing state, `workspace-preview-last-close-failed` notice | API test above (closing true, then `close == "failed"`); vitest: closing state disables buttons, failed-close notice, clean close silent, 409 message shown | pass |
| Profile `cleanup_command` is accepted end to end | API parser (`PreviewProfileConfig`, blank refused), DTOs, create route (blank refused), YAML renderer, guest `PreviewWorkspaceConfig::validate`, frontend types | `committed_preview_profiles_list_only_what_launch_accepts` (listed `cleanupCommand`); renderer round-trip tests; guest `load_preview_profile_reads_named_file`, `preview_config_rejects_blank_cleanup_command` | pass |
| G5's own profile cleans up with `just dev-down` | `.g5/preview_profiles/dev-up.yaml` | inspection only; needs a real G5 dev VM | not executed |
| Browser flow: start, refuse second start, close, cleanup write visible | `frontend/e2e/flows/workspace-preview.spec.ts` + fixture `scripts/seed-test-workspace.sh` (`cleanup_command` writes `e2e-preview-cleanup.txt`) | Playwright against a full stack with a workspace-VM host | not executed here (no nested virtualization in this VM) |

## Findings

1. **(fixed during review) The platform relay admitted streams to a closing preview.** `workspace_preview_target` checked status and readiness only; a browser could mint a session for a preview whose app was being torn down, and the guest's scanned-port admission would have carried it to a still-listening app. Fix: refuse with `PREVIEW_NOT_READY` while `cancel_requested` is set; covered by the closing API test. The ontology text of `g5.preview.service.close` was tightened to say what each side guarantees (relay refuses new streams; guest withdraws the declared port).
2. **(design note, no change) Scanned listeners.** The guest admits any scanned LISTEN socket, so during the ≤10 s app stop a non-TSI app is still reachable from inside the VM's own gate; the API refusal above is the user-facing boundary.
3. **(accepted risk) Workspace stop waits for the close grace.** `cancel_running("workspace_stop")` now SIGTERMs the preview and may wait up to 90 s before SIGKILL. The admission close already drains streams, and the dispatcher integration test exercises this exact path with a 5 s process-exit bound.
4. **(test fixture) Replacement test rewritten** as refusal; the old assertion that the first task ends `cancelled` is replaced by the assertion that it stays `queued`.

## CodeRabbit review (PR #323)

CodeRabbit's merge-risk note: the browser flow clicked `workspace-preview-close`, which the dialog's X button already used, so Playwright would have matched two elements. Fixed by giving the Close preview action its own id `workspace-preview-stop` and updating the four flow clicks; the dialog X keeps its id. Its docstring-coverage warning was addressed by documenting the new test helpers. No other findings were visible through the workspace PR tooling.

## Verification status

Executed (runtime evidence): `g5d-cli --lib preview` (26 tests), `g5d-dispatch --lib` close/preview tests (7), `g5-api-core --lib preview` (49), `g5-api` `api_task_lifecycle -- preview` (10) and `api_workspace_vms -- preview` (3) against a local Postgres 16, vitest `PreviewControl.test.tsx` (11), `tsc --noEmit`, eslint, prettier, `cargo clippy --tests` on the changed crates, `cargo fmt --check`.

Also executed after the relay fix: the real dispatcher integration test (`g5d --no-default-features --features test-helpers --test dispatch_modules -- workspace_preview`, 1 passed, close classified `clean`, process gone within 5 s) and the API preview suites again (10 + 3 passed).

Not executed: the Playwright preview flow and the G5 `dev-up`/`dev-down` profile in a real workspace VM.

## Independent review — 2026-10-05

Fresh-context review against PR #323's current head and provider base
`a97f692a`. The PR summary and governing plan were inputs; the implementing
agent's detailed review above was read only after the following initial
findings had been established from the code and diff. The current provider
checks were still pending except for the passing CodeRabbit checks. A synthetic
merge with the provider base completed without textual conflicts.

### Initial findings (before repair)

| Governing ontology | Gap | Consequence | Proposed repair / check |
|---|---|---|---|
| `g5.preview.service.close` | `run_preview` did not stop the app process group when its shell leader exited on its own. | A daemon/background child could survive while cleanup ran and remain in the shared VM after a successful preview outcome. | Stop the residual app group before cleanup on self-exit; add a discriminating regression test with a background child. |
| `g5.preview.service.close` | `run_cleanup` treated the cleanup shell leader's exit as completion without stopping descendants. | `cleanup_command` could background work, exit zero and be reported `close = clean` while its process tree remained alive. | Reap/kill the cleanup group after every leader outcome, not only timeout; cover a zero-exit command with a live background child. |
| `g5.preview.transport` / existing transfer coverage | The CodeRabbit selector repair changed the ownership-transfer flow's dialog-X click to the new Close action. | The test closes Preview before transfer and no longer proves transfer fences a live preview, broadening a test-only change beyond this PR's intended close flow. | Restore that one click to `workspace-preview-close`; retain `workspace-preview-stop` only where the flow intentionally closes the service. |

Available pre-repair evidence: `git diff --check` passed; the provider-base
merge tree was clean. Runtime rechecks follow after repair.

### Later finding during repair

- A focused `g5d-dispatch --lib close` rerun intermittently classified the
  supposedly clean SIGTERM fixture as `killed`. The test used a 200 ms sleep
  as a proxy for the shell having installed its trap, so it could signal too
  early under load. Repair: make the child print a readiness line after trap
  installation and wait for that line before calling `stop_group`.
- The bounded ontology back-check found that the dispatcher selected graceful
  close from `module == workspace.preview && cancellation`, regardless of the
  recorded cancellation reason. Consequently `admission.close` with
  `workspace_stop` ran `cleanup_command`, contradicting the accepted
  out-of-scope rule in `g5.preview.service.close`. Repair: read the already
  durable cancellation disposition and select graceful close only for
  `owner_cancel`.
- The first reason-scoping repair restored immediate dispatcher SIGKILL for
  workspace stop, but the new real-path regression showed that killing the
  `preview-serve` group alone orphaned the app because the app intentionally
  leads a separate process group. The existing design was amended before the
  dependent repair: lifecycle cancellation now sends bounded SIGINT;
  `preview-serve` immediately stops the app group and skips cleanup. Owner
  Close keeps SIGTERM and the cleanup grace.

### Repair results and rechecks

| Finding | Repair | Discriminating evidence | Result |
|---|---|---|---|
| App descendants survived a shell self-exit | Stop the residual app process group before self-exit cleanup. | `self_exit_stops_background_app_children_before_cleanup` backgrounds a delayed marker writer; cleanup waits past that delay and proves neither marker nor process survived. | fixed; `g5d-cli --lib preview` pass (29 tests) |
| Cleanup descendants survived a zero-exit shell | Stop the cleanup process group after every leader outcome, not only timeout. | `successful_cleanup_does_not_leave_background_children` uses a zero-exit cleanup with a delayed background marker. | fixed; pass |
| Transfer E2E accidentally invoked the new service Close | Restore the ownership-transfer setup click to the dialog X test id; retain the service-close test id only in Close flows. | Diff inspection distinguishes `workspace-preview-close` in transfer setup from `workspace-preview-stop` in the three intentional Close steps; eslint and prettier pass. | fixed |
| SIGTERM classification test was timing-dependent | Wait for a child readiness line emitted after trap installation. | The previously intermittent clean-close case passed five consecutive focused runs; the full preview/close filters pass. | fixed |
| Every preview cancellation ran owner cleanup | Select the signal from the durable cancellation reason: `owner_cancel` → SIGTERM/cleanup; lifecycle reason or unreadable reason → SIGINT/no cleanup. | `preview_stop_mode_is_scoped_to_the_durable_cancel_reason`; CLI lifecycle-stop test; real `workspace_preview_owner_close_is_graceful_at_dispatch_boundary` observes the cleanup marker and `close=clean`; real workspace-stop test observes app exit, no marker and null `detail.close`. | fixed; dispatcher integration pass (2 tests) |

The post-repair checks also include `g5d-dispatch --lib preview` (6 pass),
`g5d-dispatch --lib close` (5 pass), `g5-api-core --lib preview` (49 pass),
the serial API preview suites against local Postgres 16 (10 + 3 pass), the
PreviewControl vitest (11 pass), TypeScript typecheck, eslint, prettier,
`cargo fmt --check`, `git diff --check`, and clippy with warnings denied for
the changed CLI/dispatcher crates and the real `g5d` integration target. The
broader changed-crate clippy run had already passed before the final
reason-scoping repair; the final focused clippy covers every Rust file changed
by this review.

After the initial findings were saved, the provider's CodeRabbit comments were
read. Its final review independently identified the background-server variant
of the first finding; that hint was not used to establish the finding.

### Remaining gaps

No known in-scope code defect remains after repair. These evidence limits and
design questions remain:

| Governing ontology (or N/A) | Gap | Proposed action | Uncertainty / alternatives / blockers |
|---|---|---|---|
| `g5.preview.service.close`, `g5.preview.transport` | The published head's two broad Playwright shards passed, but `g5-ci-e2e` failed in `provider-agent-playwright-e2e`, the lane that explicitly reruns the M9/M12 live-preview transfer case repaired here. The local repaired flow and the committed G5 `dev-up` → `dev-down` profile were not executed in a real workspace VM. | Publish the review fixes, rerun `g5d pr checks --watch --head "$(git rev-parse HEAD)"`, and require the provider-agent lane plus receipt finalization to pass; inspect its exported Playwright artifact if it fails again. Exercise the committed profile in a workspace VM. | The check exposes step status but not the failing selector; Cloud Build log access is unavailable in this VM (`gcloud` is not installed). The failure may or may not be this transfer case, so it remains a merge blocker rather than an inferred code defect. |
| `g5.preview.service.close`, `g5.preview.service.readiness` | During cleanup after an application self-exits, the app group is gone but the historical ready fact and guest service-port admission remain until the wrapper settles. A different process deliberately binding the same port during cleanup could receive a new stream. | If immediate withdrawal on self-exit is required, design a guest `preview.stopping` event that removes admission before cleanup and decide whether the API view should expose that transient state. | The governing contract requires immediate withdrawal when owner Close is recorded, not on self-exit; the ordinary port is already closed. Adding a new lifecycle event/state is consequential and was not safe to infer in review. |
| `g5.preview.service.close` | For up to the 10 s owner-close app grace, the guest's generic scanned-port admission can still see a non-TSI listener. | Keep the API relay's tested refusal as the user-facing boundary; separately redesign guest admission if task-exclusive isolation is required. | This is an existing accepted design note, not introduced by the repairs; changing the generic scanner is broader than this PR. |
| N/A | Claude, Cursor and the second CodeRabbit check remain pending on the published pre-repair head; no provider check covers the uncommitted review fixes. | Publish through the workspace PR flow when requested and rerun all checks on the new head. | CodeRabbit and Rust CI pass on the published head; local evidence is the only evidence for the review fixes until publication. |

## CI diagnosis — 2026-10-06

With Cloud Build log access, the failed `g5-ci-e2e` run on `d0f3450e` (build `398e9549`) resolves to one test: the preview spec's "M9/M12 transfer fences live chat, Preview, and terminal" case, run in the `provider-agent-playwright-e2e` lane. Its click on `workspace-terminal-open` was blocked by the still-open preview dialog (`modal-overlay` intercepts pointer events, three retries). Cause: the CodeRabbit test-id repair had replaced that test's dialog-X click (`workspace-preview-close`) with the new Close preview action, so the dialog stayed open and the preview was stopped before the transfer. The independent review's restoration of that click is the fix; the three intentional Close clicks keep `workspace-preview-stop`. Both broad shards passed, so no other preview case failed.

A second finding from reading the guest image's `g5-dev-dockerd-start`: it launches `dockerd` with `nohup … &`, which stays in the preview command's process group, so Close would kill the daemon before `just dev-down` could reach it and the G5 profile's cleanup would fail. Fix: the `dev-up` profile now starts it with `setsid g5-dev-dockerd-start`, so the daemon leads its own session and outlives the app stop; `preview_serve.rs::a_daemon_started_under_setsid_outlives_the_app_stop_for_cleanup` pins the pattern. The profile itself still needs a real G5 workspace VM to be exercised.

