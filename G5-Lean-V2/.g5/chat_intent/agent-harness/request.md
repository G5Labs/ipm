# Workspace agent instruction delivery

## Human instructions

On 2026-10-06 the user requested an isolated worktree at `~/Workspace/g5-lean-agent-harness`, a dedicated `asserts/AGENTS.md` file, and Packer packaging of that file instead of the repository-root `AGENTS.md`, while retaining `.agents/skills/`.

The user then requested that both providers read the packaged `/var/g5d-data/runtime/assets/harness/AGENTS.md`, with Codex receiving `$HOME/.codex/AGENTS.md` and Claude receiving `$HOME/.claude/CLAUDE.md`.

## Implementation interpretation

The worker runtime is mounted inside the VM at `/opt/g5`, so both installers continue reading `/opt/g5/assets/harness/AGENTS.md`. Docker and local staging use the same dedicated source as Packer. Refresh Claude's instructions and provided skills in its native home, preserve unrelated owner state, and retire the previous G5-owned directory after installation succeeds. Enable Claude's user setting source so it actually reads the native user instructions and skills. The title-generation subprocess retains its separate project-only settings.

The user populated `asserts/AGENTS.md` during implementation. Preserve those bytes and the existing missing/empty bundle checks.

## Local validation

- Regression tests first failed against the previous Claude destination and against local staging's root-file source; they pass after the change. A new user-settings memory-override regression also failed before the compatibility fix.
- `cargo test -p g5d-agent-turn --lib -- --test-threads=1`: 78 passed, including concurrent refresh, owner-state preservation, skill retirement, partial-install retry and identical instruction bytes for both providers. An initial parallel run hit the existing Codex auth lock-release timing assertion; the serial run passed.
- Targeted `dispatch_modules` tests passed for Claude harness delivery outside the checkout, Codex execution and native-memory/Git preservation.
- `python3 scripts/test_stage_local_worker_runtime.py`: both Packer/local staging cases passed, including changed, empty and missing instructions and incomplete skills.
- `scripts/tests/agent-harness-native-test.py` passed through the built g5d and pinned Claude 2.1.285 against a local scripted API: initial install, refresh, native skill discovery, terminal launch and refusal of incompatible user memory settings. This observes prompt loading, not autonomous model adherence.
- Strict `g5d-agent-turn` Clippy, Rust formatting, changed shell syntax and diff checks passed. Full Docker/Packer images, Cloud CI and deployment were not run.

## Claude CLI review follow-up

The user requested an independent Claude CLI review and PR creation after approval. Claude reviewed commit `f442e01bd8ab843447a2954c3d28b6dd2ce63fdd` and requested changes: dispatcher fixtures still installed root development instructions and checked PR guidance absent from the shipped source; the README also described the old source.

The findings were independently confirmed. Both unit and dispatcher fixtures now assemble bundles from `asserts/AGENTS.md` and `.agents/skills`, the test image includes that source, and provider-boundary assertions compare installed bytes with the shipped file. The README now distinguishes development instructions from the workspace harness. The user-authored instruction bytes remain unchanged. Both affected ontology metadata entries explicitly record that agents receive no `g5d pr` guidance from this dedicated file; this is a remaining discovery gap, not evidence that the PR CLI was removed or that agent publication choice was tested.

## Source location follow-up

After merging PR #327, the user requested moving the dedicated instruction source from `asserts/AGENTS.md` to `.agents/AGENTS.md` and opening a small follow-up PR. Preserve the instruction bytes, packaged runtime path and both native-home destinations. Update Docker, Packer, local staging, source freshness hashing and fixtures to use `.agents/AGENTS.md`; the test image already copies the complete `.agents/` directory. The repository-root `AGENTS.md` remains separate and is read in place by the provider.

Relocation validation: the existing Packer/local staging tests failed after moving the source and updating their fixtures, then passed after correcting the producer paths. All eight harness unit tests passed. A direct check of the production freshness function confirmed that `.agents/AGENTS.md` changes its hash while the root `AGENTS.md` does not. Byte comparisons confirmed the moved payload and repository-root instructions are unchanged. Rust formatting, shell/Python syntax and diff checks passed; full worker-image builds and deployment were not run.

## PR #329 CI follow-up

Rust build `85adb630-e5e4-4f47-8b37-06c2e6ce7f92` failed on `agent_turn_blocked_replies_leave_cancellation_and_deadlines_running`: the journal recorded both replies as fully written before the cancellation was observed. The fixture emitted cancellation on stdout and immediately drained stdin, which provides no ordering between the two pipes. This was a fixture race rather than evidence that the source relocation broke instruction delivery. The production owner remains `g5d-agent-turn::runtime`: `frame_written` records completed writes while a request is pending, and cancellation removes the pending request when its provider frame is consumed.

The cancellation fixture now keeps stdin blocked until the test observes the following assistant message, which proves the preceding cancellations were processed. It also positively asserts that the second request was invalidated by provider cancellation. The sibling queued-End fixture waits until the test observes the empty background-task set before draining input. Runtime behavior and deadlines are unchanged.

Validation: both affected dispatcher tests and the separate native reply-deadline test passed. Twelve additional runs of the affected tests passed with four test processes running concurrently. Rust formatting and diff checks passed. The replacement Cloud CI result remains to be observed.
