# Backend architecture

The current relocation preserves the 26-member Rust workspace, including its Rust E2E test member. The API binary composes the API implementation crates, persistence, routing and background maintenance. The g5d binary composes worker, CLI, host and isolation crates. Existing shared crates and their dependency directions remain in place. Agents in every workspace receive the packaged harness from `.agents/AGENTS.md` and `.agents/skills/`. The same instructions are installed as `$HOME/.codex/AGENTS.md` for Codex and `$HOME/.claude/CLAUDE.md` for Claude Code.

Folder names describe ownership; they do not imply additional deployed services.

## Replacement backend design

The current proposal is [lean-backend-design-v9.md](lean-backend-design-v9.md) (draft 9, revised 2026-09-18, candidate for acceptance). It incorporates the architecture meeting and the subsequent owner/read-access clarification, as traced in the [v9 decision record](v9-meeting-decisions.md). Exactly one owner writes or uses live compute. Non-owners browse repository content from durable Git and fork to contribute; they cannot use the source VM filesystem, Terminal or preview. An unattached workspace is read-only even for its owner until explicit Start/Resume.

V9 focuses on backend refactoring and simplification. Ordinary checked forks and shared agent conversations remain; dedicated reviewer requests, source-versioned returned results, report APIs/CLI/UI and result-driven child cleanup are outside the MVP. A workspace still spans multiple member repositories with one enforced branch name created only by collision-checked fork. V8 search removal, agent-controlled commits, lifecycle-only flush and native-provider phasing remain.

The [v9 review response](lean-backend-design-v9-review-response.md) evaluates all 125 findings and records the contract and plan corrections; it distinguishes design changes from future implementation validation.

The [data-model review response](lean-backend-design-v9-datamodel-review-response.md) records subsequent simplifications: one start-authority record, ordinary stop intents, a separate input inbox, normalized task dependencies and explicit configuration ownership. It explains why receipt refs, native reply delivery state and Postgres provider artifacts remain. The latest reservation simplification uses pending/aborting workspace_forks as durable name claims, with serialized admission and verified settlement instead of a separate reservation table.

Draft 9 is self-contained. Its normative companion contracts are [workspace-sharing-v9.md](workspace-sharing-v9.md), [workspace-vm-api-v9.md](workspace-vm-api-v9.md), [worker-host-api-v9.md](worker-host-api-v9.md) and [agent-api-surface-v9.md](agent-api-surface-v9.md).

It supersedes [draft 8](lean-backend-design-v8.md), merged through [PR #78](https://github.com/G5Labs/G5-Lean/pull/78). V8, [draft 7](lean-backend-design-v7.md), [draft 6](lean-backend-design-v6.md), their plans, trackers and companion contracts remain unchanged. The [v8 security review](lean-backend-design-v8-security-review.md) remains historical evidence; this documentation does not claim its findings are implemented or closed. Earlier reliability corrections remain unless a later version explicitly supersedes their mechanism.

The supporting code survey is in [research/](research/README.md). Until the design is implemented, the crate layout above remains the implemented architecture. The v9 documentation does not establish implementation, deployment or observed behavior.

## Implementation

The plan is [lean-implementation-plan-v9.md](lean-implementation-plan-v9.md), with capability owners, ordered dependencies and self-contained implementation prompts. It preserves existing PR identifiers and merged evidence, revises affected contracts, withdraws P4.14 and removes all result-workflow dependencies while preserving the separately scheduled governance replacement. The execution-plan checkpoint covers coherent PR grouping, API-boundary workstreams, possible development mocks and early layered testing without claiming those open delivery choices were settled in the meeting.

Progress is recorded in [lean-implementation-status-v9.md](lean-implementation-status-v9.md). Previously merged rows remain unchanged; no new implementation row is marked complete by publishing this design. The [v8 plan](lean-implementation-plan-v8.md) and [v8 status](lean-implementation-status-v8.md) remain available for history.

The latest scope/follow-up disposition is [v9 final scope and review reconciliation](v9-final-scope-and-review-reconciliation.md). It records the user’s MVP scope decision and addresses or explicitly defers the reviewing agent’s follow-up findings; runtime acceptance still requires the plan’s named tests.
