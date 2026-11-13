# G5-Lean backend re-architecture — open questions (round 2)

Companion to [lean-backend-design-v2.md](lean-backend-design-v2.md). Round 1
([lean-backend-open-questions-v1.md](lean-backend-open-questions-v1.md)) is fully
answered and folded into draft 2. These are the few decisions that surfaced
while folding the answers in and re-checking the checkout. Each carries the
working assumption draft 2 uses (`[B#]` in the design). Please answer in
place.

### B1. Deployment-gate removals (Important, §3.2, §11.1, §12)
**Question.** `docs/deployment-gate-audit.md` proposes removing internal issue
tracking (live-disabled), the IDE plugin mockup, the hidden notification-
delivery settings UI, takeover remnants, and Google/GitLab/Microsoft platform
login (GitHub only). Are these approved for G5-Lean?
**Working assumption.** Approved. The design treats `g5d issue`, the issue
MCP tools, and the issue and hotlist tables as removed, and platform login as
GitHub-only.
**Answer.** _(Yes)_

### B2. Claude CLI pin (Confirm, §8.3)
**Question.** The design's Claude invocation was verified against `claude
2.1.269` installed on this machine; the repo pins `2.1.261`. Phase 3 should
bump the pin to a version that has `--permission-prompt-tool`,
`--permission-prompts host`, `--session-id`, `--input-format stream-json`, and
`--setting-sources`, and record the verified flag set. Agree?
**Working assumption.** Yes; pin to the newest tested release at Phase 3 start.
**Answer.** _(Yes)_

### B3. Fake provider for tests (Confirm, §8.6)
**Question.** Replace the `fake` provider value and the SDK-emulating
`fake_*.mjs` fixtures with fake `claude` and `codex` binaries that replay
recorded `stream-json` / `--json` frames, so the runner is exercised
unchanged in tests.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### B4. Node sidecars (Confirm, §8.3)
**Question.** The design moves both providers into one Rust runner and deletes
both Node sidecars (the Codex sidecar's event mapping is ported). The
alternative is to keep the small Codex Node sidecar as is and add a matching
Claude one. Prefer the Rust runner?
**Working assumption.** Rust runner; Node remains on the runtime mount only if
the pinned Codex CLI needs it.
**Answer.** _(Yes)_

### B5. Permission mode for non-chat tasks (Confirm, §8.3)
**Question.** Non-chat agent tasks (uplift, downlift, sync, explainer, review,
custom) keep running Claude with `bypassPermissions` and Codex with
`approval_policy=never` / `sandbox_mode=workspace-write`, as today, so they
never block on a human. Chat turns use the server-frozen mode and the
`request_permission` tool.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### B6. Periodic checkpoint interval (Confirm, §5.4)
**Question.** Default 10 minutes while the VM runs and the tree changed, plus
on every stop. Per-org override.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### B7. Cancel grace period (Confirm, §7.2)
**Question.** Cancel sends SIGINT to the provider process and SIGKILL after
15 s; the partial assistant output is kept and marked `stopped`.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### B8. Chat history rendering without a VM (Confirm, §8.1)
**Question.** The UI renders history from `chat_messages` and `task_events`
(tool calls, output) as today; `chat_session_files` is used only for
`--resume` and is not shown. Is that sufficient, or should the provider
transcript itself be viewable?
**Working assumption.** Sufficient.
**Answer.** _(I am not sure if i fully understand your assumption, but my expectation is that user should be able to view chat history on browser without workspace VM, this is because timeline are stored in postgres, so client/browser should be able to read them vis g5 api. Bascially this will be the same as today, i.e, user able to view chat history without any worker)_

### B9. Multi-task notice (Confirm, §6.3)
**Question.** With no isolation in the VM, the UI shows a notice when more
than one task or chat turn is running in a workspace, and the agent profiles
state that other agents may be editing the same folder. Anything stronger
wanted (for example refusing to start a second batch job while one runs)?
**Working assumption.** Notice only.
**Answer.** _(Yes)_

### B10. Workspace archive versus delete (Confirm, §5.1)
**Question.** Archiving a workspace stops and discards its VM, keeps the branch,
checkpoint ref, chats, and session files; deleting removes all of them.
**Working assumption.** Yes.
**Answer.** _(There is no archive action, we only have create[i.e, git fork and then addtach vm], stop(git commit, push and stop vm), resume [attach vm and restore files from branch] and delete (same act as today))_
