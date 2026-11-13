# Workspace CLI UX independent review two

Initial discovery recorded 2026-09-30 before receiving the author's implementation explanation or other reviewers' findings. Review was read-only except for this report and isolated `/private/tmp/g5-cli-review-two-1wpjweuy` investigative files. No delegation or production/test/ontology edits.

Scope: current uncommitted diff against `4dc163dab4b0afb58fc9384a43023a7caa475387`, the workspace CLI usability amendment and final implementation section of `ontology-plan.md`, sibling `request.md`, changed runtime ontology and its governing contracts. Historical PR features were inspected only where they consume the new environment or constrain instructions. The task supplied no known-defect hints.

## Initial actionable findings

### F1 — P1: The promised direct PR commands still cannot reach the API in supported Codex workspace-write sessions

Changed instruction: `assets/agents/codex-g5/AGENTS.md:18-23`; changed verification: `backend/g5d/tests/dispatch_modules.rs:1105-1112`. Affected runtime consumer: `backend/g5d-agent-turn/src/runtime.rs:943-954`, `policy.rs:92-103`, and shipped `assets/agents/codex-g5/config.toml:24-25`.

The profile now tells the agent to invoke `g5d pr` directly with supplied context, but the actual Codex launch supports `never/workspace-write`, sets that sandbox plus `approval_policy="never"`, and supplies no network-access setting. The fresh profile also has no network override. The pinned CLI is 0.153.3 (`scripts/codex-cli-version`): its [workspace permission constructor](https://raw.githubusercontent.com/openai/codex/rust-v0.153.3/codex-rs/protocol/src/models.rs) uses restricted network, and its [configuration adapter](https://raw.githubusercontent.com/openai/codex/rust-v0.153.3/codex-rs/core/src/config/permissions.rs) retains that default when `sandbox_workspace_write` is absent. `g5d-cli/src/pr.rs:95-115` makes the direct HTTP request from the shell process, so an available executable and token are insufficient in this supported mode. With approvals disabled, the agent cannot request a normal network escalation. `danger-full-access` is the valid exception; browser terminals are unaffected.

This is an existing provider-policy interaction that the new usability outcome and unconditional guidance leave unresolved, rather than a newly introduced network regression. It is within the requested agent CLI usability boundary. The added fake provider invokes help and bare `--json` (field discovery, deliberately no network) outside native Codex shell enforcement, so it cannot catch this failure.

Governing obligations: `g5.runtime.workspace-cli`, `.context`, `g5.governance.pr.read`, `.write`, `g5.quality.evidence.fakes` and `.claims`.

Proposed repair: reconcile direct API access with the actual supported Codex sandbox policy, preserving its filesystem restrictions and server-side authority. Do not solve it by placing credentials in command arguments or silently selecting full-access mode. Add a pinned native Codex shell/tool test in workspace-write mode that reaches a local authenticated PR read/create API fixture; field discovery alone is insufficient. If restricted mode is intentionally excluded, explicitly reconcile that exception with the new design and instructions instead of claiming universal availability.

Evidence classification: repository and pinned upstream source inspection, not a native-model execution. The actual 0.153.3 tool call was not executed in this review.

### F2 — P2: A restrictive startup umask makes the new shared terminal launcher inaccessible to the module identity

Changed path: `backend/g5d-runtime-core/src/runtime_environment.rs:26` and `backend/g5d-dispatch/src/git.rs:57-72`.

The helper creates the bin directory using ambient permissions, then explicitly makes only the launcher executable. When the dispatcher starts with umask `077`, `/var/lib/g5d/bin` becomes root-owned `0700`. The terminal runs as the separately provisioned module UID (`terminal/session.rs:386-399`), so neither its inherited PATH entry nor the `/usr/local/bin/g5d` symlink can traverse this directory. `server.rs:230-232` repairs only the state-directory mode, not the new child bin. Dispatcher startup nevertheless succeeds. Existing inaccessible bin permissions also remain untouched on restart.

A tiny scratch Rust executable linked against the current built runtime-core library called the real `install_g5d_launcher` with umask `077`; result: `bin=700, launcher=755`. This is a permission counterexample, not an end-to-end Linux UID-switch test. The ordinary tests all use the developer UID and therefore do not discriminate this boundary.

Governing obligation: `g5.runtime.workspace-cli` availability to owner terminals while the dispatcher retains its distinct identity.

Proposed repair: ensure the dispatcher-owned launcher directory is traversable/readable by the module identity after creation and on retained startup, while retaining dispatcher-only write access (for example explicit `0755` on that directory). Keep the provider-home parent private. Add a root/Linux test with a restrictive umask or pre-existing `0700` bin and a UID-1000 shell resolving and executing the installed launcher.

## Coverage with no additional finding

| Commitment | Actual path and check | Result / limit |
|---|---|---|
| `g5.runtime.workspace-cli` shared setup and image-tool preservation | Shared `install_g5d_launcher`, `prepend_path`, both agent/dispatch callers; launcher argument and library-isolation tests; PTY test asserts image `/usr/bin/git` | Passed on macOS for inherited PATH, quoting, idempotent replacement and loader scoping. F2 covers permission exception. Linux standard-bin installation/login-shell reset branch not executed. |
| `g5.runtime.workspace-cli.context`, `g5.security.credentials.platform-token` | Three-key `workspace_cli_environment`; runner and terminal consumers; PTY mock matches current workspace and bearer token | Passed direct terminal PR view/create without explicit workspace or login. No proxy, protocol or credential store added. |
| `g5.security.credentials.reserved-env` | `server.rs` PTY input admission uses `is_exportable_task_env_key`; helper reserves all `G5_` / `G5D_` prefixes; injected identity wins in module setup | Inspection plus PTY rejection of a token override. No new settings replacement path found. |
| `g5.security.credentials.publication`, control-secret exclusion | Terminal clears inherited environment and accepts explicit Git context plus the three workspace bindings; launcher contains no token | PTY test confirms absence of dispatch token, host JWT and loader variables. Provider publication credentials remain server-owned; none are added here. |
| `g5.terminal`, `g5.security.owner`, `.admission`, `g5.runtime.lease` | Existing PTY current-owner admission/cancellation remains in place; fixed boot token enters the same internal API client and current-authority routes | No new lifecycle or authority bypass found by inspection. Fixed inherited tokens are expressly allowed by the lease contract. No new transfer/stale-token database execution performed. |
| `g5.governance.pr.read`, `.write`, `.json` | Existing `pr.rs` transport and internal shared API operation remain unchanged; real terminal CLI against mock API exercises selected JSON and error outcome | PTY passes matching GET/POST, title/body transport, returned fields and nonzero partial-publication error with saved PR identity. F1 concerns native Codex transport reachability. No live publication or repeated-write/replay test. |
| Agent instruction delivery | Codex profile install copies primary AGENTS and required skill; Claude appends explicit guidance on each launch including resume path; `/pr` and recovery docs consistently use G5 | Both provider launch fixtures pass; no residual `gh pr create` recommendation in the changed workflow. Claude appended prompt references the mounted skill. Real model choice and real Claude execution unverified. |
| `g5.design.minimal-mechanism`, `.reuse`, `.adapters` | One shared script/context builder, direct existing CLI/API, no extra endpoint/policy owner | New mechanism is proportional; direct API keeps publication policy in its existing owner. Separate provider-home and dispatcher-state launcher locations preserve their distinct startup needs. |
| `g5.quality.review.cases`, `g5.quality.evidence*` | Public PTY and scripted provider boundaries inspected and executed | Test claims must remain scoped as below. Runtime ontology entries currently say Unassessed and should be updated by consolidation only with verified evidence and outstanding boundaries. |

Checked and rejected an additional possible concern: the pinned Codex 0.153.3 [shell environment defaults](https://raw.githubusercontent.com/openai/codex/rust-v0.153.3/codex-rs/protocol/src/config_types.rs) have `ignore_default_excludes=true`; default token-name filtering is therefore not a finding for this pin.

## Verification receipt and limits

Reused freshly built local artifacts; no full build, shared database or expensive suite run:

- `target/debug/deps/g5d_runtime_core-2476878b07595a91 runtime_environment::tests --nocapture`: 3 passed.
- `target/debug/deps/dispatch-71311356f0d96e73 terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors --exact --nocapture`: 1 passed. Initial sandbox attempt could not bind wiremock's loopback port; rerun with approved local sandbox escalation passed.
- `target/debug/deps/dispatch_modules-741bd8116479ea21 agent_turn_claude_native_question_and_permission_round_trips --exact --nocapture`: 1 passed.
- Same module artifact, `agent_turn_codex_rejects_interactivity_before_spawn_and_runs_with_stdin_eof`: 1 passed. These provider tests use shell stand-ins and isolated temporary dispatcher fixtures, not model calls.
- `git diff --check`: passed.
- Permission probe above: actual helper emitted `bin=700, launcher=755`; initial tiny link omitted the local OpenSSL search path, corrected on rerun. The probe changed only its private temporary directory.
- An initially selected older runtime-core test artifact listed no matching new test; it is not counted as evidence. The current artifact was then identified and the three real tests above ran.

Unexecuted: actual Linux guest standard-path installation and login-shell resets; root-to-module-UID launcher execution; real native Codex/Claude tool execution, model instruction choice, live GitHub, live owner transfer/incarnation revocation, and a deployed worker restart. Existing API tests and historical reports were not rerun or treated as new runtime proof. This report completes independent discovery, not implementation acceptance; F1 and F2 remain proposed repairs.

## Incremental repair review

Reviewed the current U1/U2/U3 repairs on 2026-09-30 after reading the consolidated decisions and execution receipts. Production, test and ontology files were left unchanged; no additional broad test run or delegation. Original findings above remain intact.

- **F1 / U2: honestly scoped, not implemented for restricted agents.** The current runtime contract, saved acceptance, Codex primary instructions and both shared PR skills now disclose the network block and owner-terminal recovery. `g5.agents.admission.permission-policy` explicitly freezes the chosen sandbox and denies implicit escalation; preserving it is justified. Consolidation expressly retains the failed original unrestricted acceptance instead of claiming network-disabled agent API access was fixed. The remaining limitation must stay in the user-facing completion report. No additional authority bypass or silent permission widening was introduced.
- **F2 / U3: runtime-owned bin repair is correct.** The helper now sets its own bin to `0755` on creation/reinstall, and the retained-`0700` test would fail without the repair. The root/Linux test runs a shell as UID/GID 1000 and requires actual launcher output, so it tests the relevant consumer. I read `/tmp/g5-cli-linux-recheck.log`: four real-source helper tests, including the UID transition, passed. This is receipt inspection, not a repeat run by this reviewer; it does not establish the guest boot or standard-bin installation branch.
- **U1: portable image-Git test is correct.** The test discovers available Git, exposes it in an isolated image-tool directory, and asserts that exact path remains selected after runtime PATH setup. I read the passing terminal rerun receipt and inspected that the authenticated read/create and error assertions remain. The distro path assumption is removed without removing tool-order coverage.

### F3 — P2: Do not normalize permissions on an existing image-owned `/usr/local/bin`

Introduced during U3 repair, `backend/g5d-dispatch/src/git.rs:70-72` now unconditionally chmods the entire existing `/usr/local/bin` to `0755`. This directory is image-owned and contains unrelated tools; unlike `/var/lib/g5d/bin`, it is not a new G5-owned private runtime directory. For an image deliberately configured with a group-writable/setgid standard bin (for example mode `2775` with the agent in its group), every workspace start now removes group write and setgid, so owner installs or updates of ordinary tools start failing. The operation is unrelated to installing the single reserved `g5d` entry and violates the stated preservation of image tools. The Linux helper test does not execute this branch.

Proposed repair: preserve the permissions of pre-existing image standard-bin directories. Apply a usable mode only when G5 creates a missing directory, while continuing to repair the G5-owned launcher bin itself on every install. Add a small check that a pre-existing custom mode remains unchanged and a newly created standard bin is traversable. Source inspection establishes this deterministic mode change; no guest execution is claimed.

Repair review conclusion: U1 and the launcher-directory part of U3 are supported by discriminating evidence, and U2 now reports its retained limitation honestly under the stronger permission contract. F3 is a new bounded regression in the repair and remains actionable. All previously stated native-model/live-GitHub/full-guest limits remain; root-to-UID helper execution is now supported by the inspected Linux receipt.
