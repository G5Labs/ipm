---
name: merge-when-ready
description: Merge when ready (gh pr merge --auto analogue) built 2026-10-24 on PR #712's branch; design path, owner decisions, and what is left.
metadata:
  type: project
---

Owner asked (2026-10-24) to let agents finish the "fix CI, rebase" loop; rejected guidance-only "agent merges itself into parent". Built a per-PR knob a person turns on and G5 merges as them. Owner decisions: match gh syntax (`g5d pr merge --auto/--disable-auto`, `autoMergeRequest`), ignore `--match-head-commit` for now. Design: `.g5/pr_ontology_diff_design/merge-when-ready/ontology-plan.md`; ontology `g5.git.pr-auto-merge(.readiness)`, `g5.story.merge-when-ready`.

Agent-chosen, not owner-confirmed: G5 (a 30 s background pass) merges rather than the agent; checks are always required for it; 2-minute settle window; 10-minute retry for a blocked attempt; agents may turn it off.

Follow-up 2026-10-24 (owner: "go ahead"): every merge is announced (PR-updated event to both workspaces, `pull_request` notification to the auto-merge author and source owner minus whoever acted, platform notice to source chats with queued/running/waiting agent tasks; idle agents are not woken). Owner asked for a "Merge with agent" button: turns it on and queues a loop chat in the source from a server prompt (`auto_merge::merge_with_agent_prompt`); needs target merge authority AND source ownership; with an agent an existing decision is kept. Platform-notice SQL must stay in `chat_service.rs` (`shared_shape_reader_guard` test).

**Why:** a later session may be asked to change these choices or add `--match-head-commit`.
**How to apply:** pass is `g5-api-routes/src/auto_merge.rs`; merger `PullRequestMerger::WhenReady`. Not done: a test that lands a provider-first merge under it; a control in the Base PR list (`PullRequestList.tsx`). Related: [[issue-710-workspace-agent-parity]], [[agent-route-incarnation-allowlist]].
