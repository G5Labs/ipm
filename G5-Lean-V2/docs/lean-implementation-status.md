# G5-Lean implementation status

Tracker for [lean-implementation-plan-v7.md](lean-implementation-plan-v7.md), one row per PR of the plan.

- **Branch** is filled by the PR that lands the row, in the same change, with the branch name and the PR link. On `main`, a non-empty Branch means the PR is merged and an empty one means it is not; whether someone is working on a PR is not recorded here.
- **Depends on** is a copy of the Depends on column of plan §5.1, or of the draft 6 plan for rows merged before plan v7. A PR is ready to start when every PR it lists has a non-empty Branch. A PR that changes the dependencies in the plan updates this column in the same change.
- **Notes** hold one line on what landed and the `LEAN-TODO` markers left; the PR description holds the full record.

Rows of merged PRs are never edited by later PRs.

| PR | Title | Depends on | Branch | Notes |
|---|---|---|---|---|
| P0.1 | Baseline tag, before-metrics, status tracker | — | `lean/P0.1-baseline`, [#1](https://github.com/G5Labs/G5-Lean/pull/1) | Tag `lean-before` = 6bf9979; before-metrics recorded in the PR. |
| P0.2 | Squash the migrations into `001_lean_baseline.sql` | P0.1 | `lean/P0.2-squash-migrations`, [#3](https://github.com/G5Labs/G5-Lean/pull/3) | 289 files became one baseline, since frozen (see database-migrations.md). |
| P1.1 | Git refs authoritative for reads | P0.2 | `lean/P1.1-git-refs-authoritative`, [#24](https://github.com/G5Labs/G5-Lean/pull/24) | Reads resolve heads from `refs/heads/<branch>`; head columns are caches. |
| P1.2 | Receive-pack policy: workspace branches, protected refs | P0.2 | `lean/P1.2-receive-pack-policy`, [#18](https://github.com/G5Labs/G5-Lean/pull/18) | Policy table for protected refs and workspace branches; `branch.updated` hint. |
| P1.3 | Projection convergence loop and `projection_state` | P1.1 | `lean/P1.3-projection-convergence`, [#31](https://github.com/G5Labs/G5-Lean/pull/31) | Convergence from ref OIDs; advance notifier and consumer deleted. |
| P1.4 | Delete the head-operation registry and staging refs | P1.1, P1.2, P1.3 | `lean/P1.4-delete-head-operation-registry`, [#35](https://github.com/G5Labs/G5-Lean/pull/35) | Registry, staging refs and finalizer deleted; server-side landings by compare-and-swap. |
| P1.5 | Remove remote tracking; add protected-branch mirroring and Publish | P1.4 | | |
| P1.6 | Frontend: remove remote-tracking UI, add Publish | P1.5 | | |
| P2.1 | Interim Save API (server-side commit) and committed-only reads | P1.4 | | |
| P2.2 | Frontend explicit Save; remove CRDT from editors | P2.1 | | |
| P2.3 | Worker native writes and native push at task end | P1.2 | `lean/P2.3-worker-native-push`, [#29](https://github.com/G5Labs/G5-Lean/pull/29) | The worker runs the provider directly, commits with `git add -A` semantics and pushes natively; CRDT-bound CLI commands deleted. |
| P2.4 | Delete the backend CRDT layer | P2.2 | | |
| P2.5 | Agent profiles rewritten for native git | P2.3 | `lean/P2.5-native-git-profiles`, [#33](https://github.com/G5Labs/G5-Lean/pull/33) | Profiles describe native git; hooks reduced to SessionStart and Stop. |
| P3.1 | Claude CLI pin bump; fake `claude` and `codex` binaries | P0.1 | `lean/P3.1-provider-cli-fakes`, [#12](https://github.com/G5Labs/G5-Lean/pull/12) | Pin 2.1.270; fake `claude`, `codex` and `g5-tools`; provider contract doc. |
| P3.2 | Rust provider runner; delete both Node sidecars | P3.1, P2.3 | `lean/P3.2-provider-runner`, [#34](https://github.com/G5Labs/G5-Lean/pull/34) | One Rust runner per turn; both Node sidecars deleted. |
| P3.3 | `task_questions`, `ask_user`, `request_permission`, answer polling | P3.2 | `lean/P3.3-task-questions`, [#46](https://github.com/G5Labs/G5-Lean/pull/46) | One HITL table and two MCP tools by answer polling; LEAN-TODO(P4.1), LEAN-TODO(P4.6), LEAN-TODO(P4.7). |
| P3.4 | `chat_session_files` harvest and restore | P3.2 | `lean/P3.4-chat-session-files`, [#47](https://github.com/G5Labs/G5-Lean/pull/47) | Provider session files in Postgres only; LEAN-TODO(P4.5), LEAN-TODO(P5.1). |
| P3.5 | Remove `naive_react` and the `fake` provider value | P3.2 | `lean/P3.5-remove-naive-react-and-fake-provider`, [#44](https://github.com/G5Labs/G5-Lean/pull/44) | Provider enum is `claude_code` or `codex`. |
| P3.6 | Session-resident Claude CLI process behind `agent.turn` | P4.11 | | |
| P3.7 | Codex App Server adapter with interactive approvals | P4.11 | | |
| P4.1 | Migration: `workspace_vms`, `workspace_flushes`, execution identity, `task_events`, inbox, hosts | P0.2 | | |
| P4.2a | `g5d dispatch`: listener, private protocol, files, ports, status | P3.2, P3.3, P3.4 | | |
| P4.2b | `g5d dispatch`: module runtime, journal, dispositions, built-in modules | P4.2a | | |
| P4.3a | Shutdown flush, receipts, restore, stopped reads | P4.1, P4.2a | | |
| P4.3b | Live flush quiescence and fork | P4.2b, P4.5 | | |
| P4.4a | `g5d host`: the `ctl` protocol on both ends, host credential, event journal, drain | P4.1 | | |
| P4.4b | `g5d host`: `VmSpec`, boot and stop, lease, stream relay, retained roots | P4.2a, P4.4a | | |
| P4.5 | API: lifecycle service, five actions, stop sequence, fence, Workspace Runtime facade | P4.1, P4.3a, P4.4b, P4.13 | | |
| P4.6 | Dispatch loop, two-phase invocation, event contract, reconciler, question deadlines | P4.5, P4.11 | | |
| P4.7 | Chat turns on the dispatcher, the inbox; delete sticky runtimes | P4.6, P4.12 | | |
| P4.8 | Agent tasks and jobs on the dispatcher; delete job/task branches and job merge | P4.6 | | |
| P4.9a | Delete per-task execution on the host and in the worker | P4.7, P4.8 | | |
| P4.9b | Delete the workers table, per-task routes, loops and UI | P4.9a | | |
| P4.10 | Frontend: workspace VM status, live reads, Changes, Commit/Push, Restart | P4.5 | | |
| P4.11 | The `agent.turn` module and the per-turn CLI adapters | P4.2b | | |
| P4.12 | Host proxies: model traffic per session, git and platform credential proxy, grant updates | P4.4b | | |
| P4.13 | Host registry, host states, placement, quotas, drain and the operator API | P4.1, P4.4a | | |
| P5.1 | Preview on the workspace VM (`workspace.preview`) | P4.5 | | |
| P5.2 | Browser terminal | P4.2a, P4.4b | | |
| P6.1 | `pull_requests`, `approval_rules`, `approval_decisions`, `g5d pr` | P1.5, P4.1 | | |
| P6.2 | The merge operation | P6.1 | | |
| P6.3 | Governance frontend | P6.1 | | |
| P6.4 | Delete governance legacy | P6.2, P6.3 | | |
| P7.1 | Lifecycle cleanup: one status enum, delete the kernel | P4.9b, P6.4 | | |
| P7.2 | Schema cleanup migrations and after-metrics | P7.1 | | |
| P8.1 | Remove internal issue tracking | P3.2 | | |
| P8.2 | Remove IDE mockup, hidden settings UI, takeover remnants | P0.1 | `lean/P8.2-remove-ide-mockup`, [#11](https://github.com/G5Labs/G5-Lean/pull/11) | Three dead frontend surfaces removed. |
| P8.3 | GitHub-only platform login | P0.1 | `lean/P8.3-github-only-login`, [#21](https://github.com/G5Labs/G5-Lean/pull/21) | Google, GitLab and Microsoft platform login removed (Entra returns for self-hosted in P9.2). |
| P8.4 | Infra cleanup: dormant MIG, customer images, disabled workflows | P4.9a, P4.9b | | |
| P8.5 | Crate consolidation | P7.2 | | |
| P8.6 | CI review | P8.5 | | |
| P8.7 | Versioned rollout: protocol gates, feature flags, drain-and-recreate runbook | P4.5, P4.13 | | |
| P8.8 | Pre-launch audits: security, fault tolerance, scalability | P2.4, P4.3b, P4.9b, P4.10, P5.1, P5.2 | | |
| P9.1 | Self-hosted bundle on the workspace-VM model | P4.9a, P8.4 | | |
| P9.2 | Microsoft Entra platform login for self-hosted installations | P9.1 | | |
| P9.3 | Backup barrier and restore of Postgres and the git store; upgrade drain | P4.3a, P9.1 | | |
| P9.4 | Azure canary on the workspace-VM host | P9.1, P9.2, P9.3 | | |
| CI2 | Track the 50 embedded agent-profile files | — | `lean/CI2-track-embedded-profile-files`, [#22](https://github.com/G5Labs/G5-Lean/pull/22) | Fresh checkouts compile. |
| CI3 | Add the docker-ci Phase 1 lane for tools/fake-provider-cli | — | `lean/CI3-fake-provider-ci-lane`, [#32](https://github.com/G5Labs/G5-Lean/pull/32) | Phase 0b passes on `main`. |
