# G5-Lean implementation plan (v7)

Status: plan for executing [lean-backend-design-v7.md](lean-backend-design-v7.md), written 2026-09-16. It succeeds [lean-implementation-plan-v1.md](lean-implementation-plan-v1.md) (the draft 6 plan, kept for history) and keeps its PR identifiers: a PR that draft 7 does not change keeps its id and its prompt with only the design references updated; a PR that draft 7 changes keeps its id and gets a revised prompt; new work gets new ids. Progress is tracked in [lean-implementation-status.md](lean-implementation-status.md): the PR that lands a row fills its Branch column, so on `main` a non-empty Branch means merged and an empty one means not merged.

The design is authoritative; when a prompt and the design disagree, the design wins and the prompt is corrected. The three companion notes ([workspace-vm-api.md](workspace-vm-api.md), [worker-host-api.md](worker-host-api.md), [agent-api-surface.md](agent-api-surface.md)) are normative for field-level detail where the design adopts them.

---

## 1. How to use this plan

**For the person running the program.** For the MVP, follow the three-person schedule in §5.3; afterwards land PRs wave by wave (§5.4). A PR may start only when everything it depends on has merged to `main` on https://github.com/G5Labs/G5-Lean; nothing is started against a stand-in. Each PR names the capability that owns it (§4); the owner of that capability reviews it, and the program owner merges on convergence. Before starting a PR, confirm in the status file on `origin/main` that its dependencies are merged (§2, item 1).

**For an agent starting a PR.** Copy the PR's prompt from §6 into a fresh session in this repository. The prompt says what to read first, what to build or delete, what not to touch, how to prove it works, and how to finish. The conventions in §2 apply to every PR and are part of the prompt by reference.

**Sizing.** S under 1K changed lines, M 1K to 4K, L 4K to 10K, XL over 10K. Deletion-heavy PRs are XL by line count but mechanical; the reviewer's job there is checking that nothing retained still references the deleted code, which the compiler and tests do for us.

---

## 2. Conventions for every PR

1. **Start from the latest `main`, and only when your dependencies are in.** Your PR's dependencies must already be merged. Read the status file from `origin/main`, not from a local checkout: `git -C /Users/zliu/Workspace/G5-Lean fetch origin main && git -C /Users/zliu/Workspace/G5-Lean show origin/main:docs/lean-implementation-status.md`. Your row's Depends on cell lists the dependencies, and each is merged when its own row has a non-empty Branch cell; an empty Branch means not merged, whatever GitHub shows in flight. To read one row, pipe the file through `grep -E '^\| P4\.2a \|'` with that row's id in place of `P4.2a`. If a dependency is not merged, stop and report instead of starting; never start against a stand-in. Then fetch and branch from `origin/main`, never from a local `main` that may be stale: `git -C /Users/zliu/Workspace/G5-Lean fetch origin main && git -C /Users/zliu/Workspace/G5-Lean worktree add ../G5-Lean-wt-<pr-id> -b lean/<pr-id>-<slug> origin/main`, where `<pr-id>` is the exact id from §5.1, suffix included (for example `P4.2a`). Work in that worktree, never in the primary checkout. Run `git branch --unset-upstream` after creating the branch so a bare `git push` can never target `main`. Do not touch other worktrees. Before your first build, run the validation ladder once on `origin/main`; a failure that predates your branch is pre-existing, and you record it in the PR description instead of fixing it.
2. **Keep your branch current.** Before you open the PR, and after every round of review changes, run `git fetch origin && git rebase origin/main` (rebase, never merge `main` into your branch). Resolve conflicts yourself; the usual hotspots are migration numbering, your row in `docs/lean-implementation-status.md`, and `Cargo.lock`. If the rebase brought in changes to a crate you touched, re-run the validation ladder for that crate before pushing. Push with `git push --force-with-lease` after a rebase. Never rebase or amend after the PR is approved unless the reviewer asks.
3. **Read first.** Every prompt lists design sections, companion notes and research notes. Read them before opening code. Line numbers drift; symbol names are stable.
4. **Scope discipline.** Do exactly what the PR says. If the PR cannot be completed without a change listed under "Do not", stop and report it in the PR description under "Blocked"; do not widen the PR. Unrelated cleanups go into a note in the status file, not into the diff.
5. **Buildable at every merge.** The workspace must compile and the retained tests must pass at the end of the PR. If a later PR will delete something this PR would otherwise have to fix, leave a `// LEAN-TODO(P<x.y>)` marker instead of fixing it, and say so in the PR description. Grep for markers with your PR id before you finish.
6. **Schema edits are forward migrations.** `001_lean_baseline.sql` and every applied migration are immutable, checksum-guarded files; never edit one, never repair the SQLx ledger. Add `NNN_<description>.sql` with the next unused number (coordinate numbering at rebase), append its SHA384 to `backend/g5-api/src/db/migrations/checksums.sha384`, and follow [database-migrations.md](database-migrations.md): validate a fresh install and an upgrade from the previous schema on a copy of real rows, and re-run to prove the no-op path. Every prompt of the draft 6 plan that said "edit the baseline" now means "add a migration". Regenerate the SQLx offline cache when queries change (`just db-prepare`, with `just dev-up`).
7. **Tests.** Add the acceptance tests named in the prompt. Retarget or delete tests only for behavior the PR removes; never delete a test to make a retained behavior pass. Classify every removed test in the PR description (deleted protocol / covered elsewhere / rehomed). Fake providers: tests point `G5_CLAUDE_PATH` / `G5_CODEX_PATH` at the fake binaries.
8. **Validation ladder** (run what applies; the prompt says which are required): `SQLX_OFFLINE=true cargo check --workspace --all-targets --offline`; `just fmt-check` and `just lint` plus `cargo clippy --workspace --all-targets -- -D warnings` (Cloud Build lints test targets); `SQLX_OFFLINE=true cargo test -p <crate> --lib` for touched crates; `just test-integration` (needs `just dev-up`) when routes or services change; `npm --prefix frontend run build && npm --prefix frontend test` when the frontend changes; `just docker-ci` for PRs that touch worker, host, isolation, dispatcher or CI configuration; `python3 scripts/check-layout.py` and `just commands-check` when files move or recipes change; the migration checks of item 6 when a migration is added.
9. **Commits and PR description.** Conventional subject line `<area>: <what>`. End every commit message with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. The PR description uses the template in §2.1 and ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. Push the branch and open the pull request with `gh pr create --repo G5Labs/G5-Lean --base main --head lean/<pr-id>-<slug> --title "P<id> <title>" --body-file <file>`. The title starts with the exact PR id from §5.1, suffix included, followed by a space (for example `P4.2a g5d dispatch: listener, private protocol, files, ports, status`); the status row and the PR are matched by it, so never abbreviate the id, never drop the suffix, never reuse an id. Record the branch and the PR URL in your status row (item 10). Do not merge your own PR. Cloud Build runs the PR lanes; until the lanes gate merges, the validation ladder you ran locally is the evidence the reviewer relies on, so list exactly what you ran.
10. **Status file.** In the same PR, fill your row in `docs/lean-implementation-status.md`: the Branch column with the branch name and the PR link, the Notes column with one line on what landed and the `LEAN-TODO` markers left. Never mark a row as in progress: a row stays empty until its PR merges, because other agents read a non-empty Branch on `origin/main` as "merged". Never edit another PR's Branch or Notes. The Depends on column is a copy of §5.1: leave it as it is; only a PR that changes the dependencies in §5.1 edits it, in every row it changes. The PR description (§2.1) holds the full record and stays true after every review round.
11. **No deployment.** Nothing in this plan deploys anything. Cloud Build stays the CI system; changes to the CI lanes are P8.6 and P8.7 only, except where a prompt names a lane it must adjust.
12. **Ownership.** Every PR carries the capability that owns it (§4). Ask that owner's questions in the PR, not another owner's. A change that crosses into another capability's code is a deviation to record, not a reason to widen the PR.
13. **Agents do not create GitHub issues, extra PRs or any public artifact.** Findings go into the PR description and the status file; the program owner files issues after the user approves.

### 2.1 PR description template

```
## P<id> <title>

Owner: <Workspace | Agents | Worker & Host | Git and governance | Platform>
Design: lean-backend-design-v7.md §<sections>
Depends on: P<..> (merged)

### What changed
- ...

### Deleted
- ...

### Tests
- added: ...
- retargeted: ...
- removed (behavior removed by this PR, classified): ...

### Migrations
- <NNN_name.sql>: fresh install and upgrade validated | none

### Validation run
- cargo check / lint / fmt: pass
- <crate> tests: pass (N tests)
- integration / docker-ci: pass | not run (why)

### LEAN-TODO markers left
- ...

### Blocked / deviations
- none
```

---

## 3. Where `main` is, and what draft 7 changes

### 3.1 Landed from the draft 6 plan

| PR | Merged as | Note |
|---|---|---|
| P0.1, P0.2 | #1, #3 | baseline tag; one baseline migration (now frozen) |
| P1.1, P1.2, P1.3, P1.4 | #24, #18, #31, #35 | refs authoritative for reads; receive-pack policy; projection convergence; head-operation registry deleted |
| P2.3, P2.5 | #29, #33 | worker native writes and push; native-git profiles |
| P3.1, P3.2, P3.3, P3.4, P3.5 | #12, #34, #46, #47, #44 | CLI pin and fakes; Rust provider runner; `task_questions`; `chat_session_files`; `naive_react` gone |
| P8.2, P8.3 | #11, #21 | IDE mockup, hidden settings, takeover remnants; GitHub-only login |
| out of plan | #5, #7, #8, #10, CI2 #22, CI3 #32 | dev-stack and CI fixes, recorded in their PRs |

### 3.2 CI/CD in the QA GCP project (not plan PRs)

Landed by the CI/CD owner: #2 QA infrastructure, #4 CI write isolation, #6 QA data-service labels, #9 firmware boot probe, #13 and #36 to #39 worker image publishing and artifact IAM, #15 CI repository-specific inputs, #41 rustls advisory, #42 worker image repin, #43 greenfield database bootstrap, #45 QA bootstrap key and RLS contracts, #50 migration checksum guard and deployment pauses (with `docs/database-migrations.md` and `docs/qa-database-reset.md`). Open at this writing: #51 (QA worker image defaults), #53 (seeder authority scope), #66 (E2E workflow repairs). Design notes landed as #48 (worker host API), #72 (agent API surface) and the workspace VM API note.

### 3.3 Draft 7's effect on the remaining draft 6 PRs

| PR | Change |
|---|---|
| P2.1, P2.2, P2.4 | unchanged in substance; P2.1 stays an interim server-side Save so the CRDT deletion can proceed in parallel with the VM runtime; P4.5 replaces the write path |
| P4.1 | a forward migration instead of a baseline edit; adds `workspace_flushes`, `epoch`, inbox and host columns; no longer depends on P2.4 |
| P4.2 | the dispatcher exposes the Workspace Runtime API and a module runtime; no `proc.*`, `git.*`, `agent.run_turn`; dispositions table; `agent.turn` moves to P4.11; split into P4.2a (listener, private protocol, files, ports, status) and P4.2b (module runtime, journal, dispositions, built-in modules) |
| P4.3 | "Checkpoint task and restore" becomes "Flush, receipts and restore": no cron; immutable per-flush refs; receipt verified by the API; quiescence rule; split into P4.3a (shutdown flush, receipts, restore, stopped reads) and P4.3b (live flush and fork) |
| P4.4 | the worker-host-api protocol: `ctl` stream, versioning, `VmSpec`, journaled `vm.event`, renewable host credential, lease bound; proxies move to P4.12; split into P4.4a (the `ctl` protocol on both ends, no VM boot) and P4.4b (`VmSpec`, boot and stop, lease, relay) |
| P4.5 | five actions (restart added), teardown reasons and deadlines, the stop sequence with the flush, Save ordering, the Workspace Runtime facade; placement moves to P4.13 |
| P4.6 | invocation through the facade; G1 to G4 of the draft 6 review and the cancellation retention rule; runner wait and `suspended`; inbox reservations |
| P4.7 | inbox columns and the queued strip; base workspaces without a VM |
| P4.8, P4.9, P4.10, P5.1, P5.2 | as before, on the new surfaces; one preview per workspace; restart button; `source: flush`; P4.9 is split into P4.9a (host and worker) and P4.9b (API and frontend) |
| P6.2 | G5: one ref transaction with a receipt ref |
| P7.2 | "Rewrite the baseline" becomes "Schema cleanup migrations and after-metrics" |
| P8.4 | the renewable host credential moves to P4.4a; the rest unchanged |
| P1.5, P1.6, P6.1, P6.3, P6.4, P7.1, P8.1, P8.5, P8.6 | unchanged apart from design references and the migration rule |

### 3.4 Outstanding markers on `main`

`LEAN-TODO(P4.8)` 55, `LEAN-TODO(P1.5)` 51, `LEAN-TODO(P6.4)` 16, `LEAN-TODO(P2.4)` 13, `LEAN-TODO(P8.6)` 5, `LEAN-TODO(P4.6)` 3, `LEAN-TODO(P6.1)` 2, `LEAN-TODO(P5.1)`, `LEAN-TODO(P4.7)`, `LEAN-TODO(P4.5)`, `LEAN-TODO(P4.1)` one each. Each owning PR resolves its markers; `grep -rn LEAN-TODO` is the ledger.

---

## 4. Capability tracks, owners and milestones

Ownership follows the capability, not the process a component runs in (design §4.1). The owners in the table are the reviewers of record for their track; the Git and governance track is assigned by the team. Ownership names who reviews, not who executes: for the MVP the executor of each PR is the person the schedule in §5.3 puts on it, and carrying another track's PR is expected.

| Track | Owner | PRs |
|---|---|---|
| Workspace | @zcl-g5 | P2.1, P2.2, P2.4, P4.1, P4.2a, P4.2b, P4.3a, P4.3b, P4.5, P4.10, P5.1, P5.2, P7.2 |
| Agents | @rahul-g5 | P4.6, P4.7, P4.8, P4.11, P7.1, P8.1, P3.6, P3.7 |
| Worker & Host | @kaihuiz-g5 | P4.4a, P4.4b, P4.9a, P4.9b, P4.12, P4.13, P8.4, P8.7 |
| Git and governance | to be assigned | P1.5, P1.6, P6.1, P6.2, P6.3, P6.4, the receive-pack half of P4.3a |
| Platform (program owner, CI/CD owner) | | P8.5, P8.6, P8.8, P9.1 to P9.4 |

**MVP milestone.** Design §13: the nine proposals on the QA environment. In PR terms: P2.1, P2.2, P2.4, P4.1, P4.2a, P4.2b, P4.3a, P4.3b, P4.4a, P4.4b, P4.5, P4.6, P4.7, P4.8, P4.9a, P4.9b, P4.10, P4.11, P4.12, P4.13, P5.1, P5.2 merged. P8.1 is idle-time work, not an MVP gate. Exit criteria are in §7.

**After the MVP, in priority order.** Governance and publish (P1.5, P1.6, P6.1 to P6.4); the pre-launch audits (P8.8); lifecycle and schema cleanup (P7.1, P7.2); rollout gates (P8.7) and infra cleanup (P8.4); the self-hosted phase (P9.1 to P9.4); the resident-process provider track (P3.6, P3.7); crate consolidation and the CI review (P8.5, P8.6).

---

## 5. PR map, dependencies and waves

### 5.1 PR table

| PR | Title | Owner | Size | Depends on | State |
|---|---|---|---|---|---|
| P1.5 | Remove remote tracking; add protected-branch mirroring and Publish | Git | XL | P1.4 | not started |
| P1.6 | Frontend: remove remote-tracking UI, add Publish | Git | M | P1.5 | not started |
| P2.1 | Interim Save API (server-side commit) and committed-only reads | Workspace | M | P1.4 | not started |
| P2.2 | Frontend explicit Save; remove CRDT from editors | Workspace | XL | P2.1 | not started |
| P2.4 | Delete the backend CRDT layer | Workspace | XL | P2.2 | not started |
| P3.6 | Session-resident Claude CLI process behind `agent.turn` | Agents | L | P4.11 | post-MVP |
| P3.7 | Codex App Server adapter with interactive approvals | Agents | L | P4.11 | post-MVP |
| P4.1 | Migration: `workspace_vms`, `workspace_flushes`, execution identity, `task_events`, inbox, hosts | Workspace | S | P0.2 | not started |
| P4.2a | `g5d dispatch`: listener, private protocol, files, ports, status | Workspace | L | P3.2, P3.3, P3.4 | not started |
| P4.2b | `g5d dispatch`: module runtime, journal, dispositions, built-in modules | Workspace | L | P4.2a | not started |
| P4.3a | Shutdown flush, receipts, restore, stopped reads | Workspace (+Git) | L | P4.1, P4.2a | not started |
| P4.3b | Live flush quiescence and fork | Workspace | M | P4.2b, P4.5 | not started |
| P4.4a | `g5d host`: the `ctl` protocol on both ends, host credential, event journal, drain | Worker & Host | L | P4.1 | not started |
| P4.4b | `g5d host`: `VmSpec`, boot and stop, lease, stream relay, retained roots | Worker & Host | L | P4.2a, P4.4a | not started |
| P4.5 | API: lifecycle service, five actions, stop sequence, fence, Workspace Runtime facade | Workspace | XL | P4.1, P4.3a, P4.4b, P4.13 | not started |
| P4.6 | Dispatch loop, two-phase invocation, event contract, reconciler, question deadlines | Agents | L | P4.5, P4.11 | not started |
| P4.7 | Chat turns on the dispatcher, the inbox; delete sticky runtimes | Agents | XL | P4.6, P4.12 | not started |
| P4.8 | Agent tasks and jobs on the dispatcher; delete job/task branches and job merge | Agents | XL | P4.6 | not started |
| P4.9a | Delete per-task execution on the host and in the worker | Worker & Host | L | P4.7, P4.8 | not started |
| P4.9b | Delete the workers table, per-task routes, loops and UI | Worker & Host | L | P4.9a | not started |
| P4.10 | Frontend: workspace VM status, live reads, Changes, Commit/Push, Restart | Workspace | L | P4.5 | not started |
| P4.11 | The `agent.turn` module and the per-turn CLI adapters | Agents | L | P4.2b | not started |
| P4.12 | Host proxies: model traffic per session, git and platform credential proxy, grant updates | Worker & Host | L | P4.4b | not started |
| P4.13 | Host registry, host states, placement, quotas, drain and the operator API | Worker & Host | M | P4.1, P4.4a | not started |
| P5.1 | Preview on the workspace VM (`workspace.preview`) | Workspace | L | P4.5 | not started |
| P5.2 | Browser terminal | Workspace | M | P4.2a, P4.4b | not started |
| P6.1 | `pull_requests`, `approval_rules`, `approval_decisions`, `g5d pr` | Git | L | P1.5, P4.1 | not started |
| P6.2 | The merge operation | Git | L | P6.1 | not started |
| P6.3 | Governance frontend | Git | L | P6.1 | not started |
| P6.4 | Delete governance legacy | Git | XL | P6.2, P6.3 | not started |
| P7.1 | Lifecycle cleanup: one status enum, delete the kernel | Agents | L | P4.9b, P6.4 | not started |
| P7.2 | Schema cleanup migrations and after-metrics | Workspace | M | P7.1 | not started |
| P8.1 | Remove internal issue tracking | Agents | XL | P3.2 | idle-time work |
| P8.4 | Infra cleanup: dormant MIG, customer images, disabled workflows | Worker & Host | M | P4.9a, P4.9b | not started |
| P8.5 | Crate consolidation | Platform | XL | P7.2 | not started |
| P8.6 | CI review | Platform | S | P8.5 | not started |
| P8.7 | Versioned rollout: protocol gates, feature flags, drain-and-recreate runbook | Worker & Host | M | P4.5, P4.13 | not started |
| P8.8 | Pre-launch audits: security, fault tolerance, scalability | Platform | S | P2.4, P4.3b, P4.9b, P4.10, P5.1, P5.2 | not started |
| P9.1 | Self-hosted bundle on the workspace-VM model | Platform | L | P4.9a, P8.4 | later phase |
| P9.2 | Microsoft Entra platform login for self-hosted installations | Platform | M | P9.1 | later phase |
| P9.3 | Backup barrier and restore of Postgres and the git store; upgrade drain | Platform | M | P4.3a, P9.1 | later phase |
| P9.4 | Azure canary on the workspace-VM host | Platform | M | P9.1, P9.2, P9.3 | later phase |

Merged PRs (P0.1, P0.2, P1.1 to P1.4, P2.3, P2.5, P3.1 to P3.5, P8.2, P8.3) keep their rows in the tracker.

### 5.2 Dependency graph

```
(merged) P1.4 ─► P2.1 ─► P2.2 ─► P2.4
(merged) P0.2 ─► P4.1 ─┬─► P4.4a ─┬─► P4.13 ─────────────────────────┐
                       │          │                                  │
(merged) P3.2/P3.3/P3.4 ─► P4.2a ─┼─► P4.4b ─┬─► P4.12 ──────────────┼─────────────────► P4.7 ─┐
                                  │          └─► P5.2                │                          │
                                  ├─► P4.3a ────────────────────────┴─► P4.5 ─┬─► P4.6 ─────────┼─► P4.8 ─┼─► P4.9a ─► P4.9b ─► P8.4 ─► P9.1
                                  └─► P4.2b ─┬─► P4.11 ───────────────────────┘   ├─► P4.10      │
                                             └─► P4.3b (with P4.5)                ├─► P5.1       │
                                                                                  └─► P8.7 (with P4.13)
(merged) P1.4 ─► P1.5 ─┬─► P1.6
                       └─► P6.1 (with P4.1) ─┬─► P6.2 ─┐
                                             └─► P6.3 ─┴─► P6.4 ─► P7.1 (with P4.9b) ─► P7.2 ─► P8.5 ─► P8.6
P4.11 ─► P3.6, P3.7 (post-MVP);  (merged) P3.2 ─► P8.1 (idle-time work);  MVP ─► P8.8
```

Every edge is a real dependency: the dependent PR builds on merged code and is never started against a stand-in. Two edges of the draft 6 plan were removed because they were not real: P4.5 does not need P4.12 (the dispatcher's flush pushes with the token the dispatcher holds; the proxies serve provider processes, which first run in QA with P4.7) and P4.5 does not need P4.2b (cancelling module executions in the stop sequence arrives with P4.6, the first PR that creates executions). Drain orchestration, which needs the stop sequence, lives in P8.7 rather than in P4.13.

### 5.3 MVP schedule for three people

The MVP is executed by three people with coding agents. Ownership (§4) says who reviews; this schedule says who executes. Each row starts when the dependencies of its PRs are merged; someone who finishes early moves down their own column, or takes the highest-ranked available PR from another column. Rank is the length of the longest dependency path from the PR to P4.9b, so the critical chain always goes first. Nothing is started against a stand-in: no stub method, no placeholder type, no interface-first PR. Test harnesses that drive a real component from its tests are not stand-ins.

| zcl | kaihui | rahul |
|---|---|---|
| P4.2a | P4.4a | P4.1, then P2.1, then P4.13 |
| P4.3a | P4.4b | P4.2b |
| P4.5 | P4.12, then P5.2 | P4.11 |
| P4.10 | P2.2 | P4.6 |
| P4.8 | P5.1 | P4.7 |
| P2.4 | P4.9a | P4.3b, then P4.9b |

The critical chain runs down the first column and then over to the third: P4.2a → P4.3a → P4.5 → P4.6 → P4.7 → P4.9a → P4.9b. The MVP closes when P2.4, P4.3b, P4.9b, P4.10, P5.1 and P5.2 are merged; every other MVP PR is a dependency of one of them (§7). P8.1 is idle-time work for whoever is waiting on a dependency.

### 5.4 Waves after the MVP

| Wave | PRs | Owners | Notes |
|---|---|---|---|
| 7 | P1.5, P1.6, P6.1, P8.4, P8.7, P8.8 | Git ×3, Host ×2, Platform | P1.6 and P6.1 follow P1.5 |
| 8 | P6.2, P6.3, P3.6 | Git ×2, Agents | |
| 9 | P6.4, P3.7 | Git, Agents | |
| 10 | P7.1, then P7.2 | Agents, Workspace | sequential |
| 11 | P9.1, then P9.2 and P9.3 | Platform | the self-hosted phase |
| 12 | P9.4 | Platform | |
| 13 | P8.5, then P8.6 | Platform | sequential |

---

## 6. PR prompts

Each prompt is self-contained apart from §2 of this document, which it references. Paths are relative to the repository root. Prompts for merged PRs are in the draft 6 plan.

### P1.5 — Remove remote tracking; add protected-branch mirroring and Publish

```
You are implementing P1.5 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.6, §3.2 (remote tracking
row), and docs/research/research-git-workspace.md §C.3 step 4, §C.4, §G. Depends on
P1.4 (merged). Owner: Git and governance.

Goal: the only provider interactions are (a) inbound mirroring of protected branches
and (b) explicit server-side publish of a workspace branch.

Do:
1. Inbound: backend/g5-api-core/src/maintenance/protected_branch_mirror.rs: every
   5 minutes (configurable) and on the provider webhook if one exists (grep for
   webhook handling; `repo_secrets.webhook_secret_enc` suggests one), fetch
   `refs/heads/<protected>` from the provider into the mirror with the connecting
   user's stored token (`ScmTokenManager`), under a per-repo lock that P6.2 will
   share. Publish the branch hint on change.
2. Outbound: `POST /api/v1/workspaces/:id/publish` (scope workspaces:write): push every
   member repo's workspace branch to the provider with the acting user's token and
   `--force-with-lease`, same branch name. Reuse `push_branch_ff_or_lease` from
   workspace_pull_request_provider_lifecycle.rs (move it to a small
   `provider_push.rs`; the lifecycle file is deleted in P6.4). Reduce Make Remote to
   "bind repo to provider URL, then publish".
3. Delete: `branch_integration_service.rs`, `branch_sync_runs_service.rs`,
   `remote_tracking_merge.rs`, maintenance `branch_integration_worker.rs`,
   `branch_integration_scheduler`, `base_auto_pull_scheduler`, `remote_advisory_pinger`,
   `retained_publication_sweep`'s remote-tracking duties; routes `/push_to_remote`,
   `/push_mode`, `/base_auto_pull`, `/sync`, `/branches/:b/sync_runs*`, `/merge_runs*`,
   `/remote_tracking*`, `/approved_rebase_publish`, `/auto_pull`; CLI `g5d remote`;
   every `LEAN-TODO(P1.5)` site (51 on main; the landing's initial-tracking,
   convergence and merge-completion consumers, the baseline-ref and integration-queue
   follow-ups, fork and make-remote metadata).
4. Migration `NNN_remove_remote_tracking.sql`: drop `branch_integration_queue`,
   `branch_integration_remote_pair_leases`, `branch_sync_runs`; drop
   `workspaces.{push_mode, base_auto_pull_enabled, base_auto_pull_actor_user_id}` and
   `workspace_branch_repos.{remote_branch, baseline_commit, sync_paused_reason,
   last_sync_at, remote_tip_advisory_sha, remote_tip_advisory_observed_at,
   auto_pull_enabled, tracking_mode}`; the `job_merge_runs.source_kind='remote_tracking'`
   path; `refs/g5-tracking/*` pruned by a one-time maintenance pass.
5. Tests: retire remote_tracking_*.rs and job_merge_* remote-tracking tests; add
   protected_branch_mirror.rs (fetch updates the mirror, workspace branches are never
   fetched) and api_workspace_publish.rs (publish pushes the branch with lease; base
   workspace publish refused).

Do not: touch PR creation or merge (P6), the frontend (P1.6).

Acceptance: `grep -rn "tracking_mode\|push_mode\|branch_integration\|auto_pull\|LEAN-TODO(P1.5)" backend`
is empty; mirror and publish tests pass; docker-ci passes with the remote-tracking
Playwright flows skipped (`LEAN-TODO(P1.6)`).
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P1.6 — Frontend: remove remote-tracking UI, add Publish

```
You are implementing P1.6 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.6, and
docs/research/research-frontend-preview-infra.md §1.3 row 16 and §5.1. Depends on
P1.5. Owner: Git and governance.

Goal: the workspace UI offers "Publish branch" and nothing about tracking modes,
auto-pull, sync runs or merge runs.

Do:
1. Delete frontend/src/components/workspace/{RemoteTrackingActions, BranchTrackingIcon,
   BranchTrackingPopover, BranchAutoPullControl, CoordinatedRepoSyncModal}.tsx, the
   merge-routing parts of MakeRemoteDialog.tsx (keep "connect repo to provider"), and
   hooks/useRemoteTracking.ts; remove push-mode and auto-pull controls from settings;
   remove the WS handlers for `workspace.remote_tip_advisory_updated` and the dead
   `workspace.authoritative_cut_advanced` handler.
2. Add a Publish button (ReposBar) calling `POST /workspaces/:id/publish` with toasts;
   disabled on base workspaces.
3. Delete Playwright flows remote-tracking.spec.ts, remote-tracking-fork.spec.ts,
   coordinated-repo-sync.spec.ts, workspace-sync.spec.ts; add a publish flow to
   workspace.spec.ts.
4. Update frontend/src/lib/types.ts and api-schemas.ts.

Do not: touch editors, drafts, chat, PR/approval components.

Acceptance: `npm --prefix frontend run build`, `npm --prefix frontend test` and
`just test-playwright` pass; grep for `remote_tracking|pushMode|autoPull` in
frontend/src is empty.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P2.1 — Interim Save API and committed-only reads

```
You are implementing P2.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and §3.3, design v7 §9.1, §9.2, and
docs/research/research-crdt-docs.md §B.4, §D. Depends on P1.4 (merged). Owner: Workspace.

Goal: the file, doc and ontology write routes no longer target the CRDT overlay.
Until the dispatcher exists (P4.5), Save is a server-side commit to the workspace
branch and reads are committed-only. This is interim: P4.5 replaces the write path
with `fs.write` through the Workspace Runtime facade; keep the route shape so the
frontend of P2.2 does not change again.

Do:
1. `PUT /api/v1/workspaces/:id/files/*path` (routes/files.rs): body
   `{content, expected_hash}`; read the blob at the branch head, compare sha256 of its
   content with `expected_hash` (409 with current content and hash on mismatch), build
   a commit with the single blob replaced using the tree helpers in
   backend/g5-workspace-ops/src/commit.rs, apply with the P1.4 `branch_commit` landing,
   author = the user, message `Save <path> from G5`. Same for create and delete. `GET`
   returns `{content, hash, source: "committed"}`. Remove `draft_dirty`.
2. Route `PUT /workspaces/:id/docs/:docId` and `PUT /workspaces/:id/ontology/:nodeId/content`
   through the same helper.
3. `RepoReadView` (workspace_reader.rs): drop the overlay; reads come from the
   committed view only. Leave the CRDT code compiled but unused (P2.4 deletes it);
   mark with LEAN-TODO(P2.4).
4. Tests: api_files.rs (new): save with matching hash commits and moves the ref; stale
   hash returns 409 with current content; create and delete; docs and ontology PUTs
   land commits; reads report `source: committed`.

Do not: touch the frontend (P2.2) or delete CRDT modules (P2.4).

Acceptance: new tests pass; api_drafts.rs tests that assert overlay writes through
these routes are retired (the overlay is no longer a write target); docker-ci passes
with CRDT Playwright flows temporarily skipped (LEAN-TODO(P2.2)).
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P2.2 — Frontend explicit Save; remove CRDT from editors

```
You are implementing P2.2 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §9.2, §9.3, and
docs/research/research-crdt-docs.md §F and research-frontend-preview-infra.md §1.3
rows 4 to 8. Depends on P2.1. Owner: Workspace.

Goal: no Yjs in the frontend. Editors hold local state and save explicitly.

Do:
1. CodeEditor.tsx: Monaco with a local model; Save (Ctrl+S) calls the P2.1 PUT with
   `expected_hash`; on 409 show a reload-or-overwrite dialog; optional autosave-on-blur
   behind a user preference; dirty indicator per tab; the `source` marker from the read
   response shown in the tab (live | flush | committed).
2. SpecEditor.tsx (block editor), StructuredQuestionEditor, EditableInline,
   questionAnswerWrite.ts, htmlDocAnswerBridge.tsx, ExplainerOutputView.tsx,
   JobOutputView.tsx: replace the `Y.Text` binding with a local document model and the
   same Save path; JobQuestionDocView.tsx already renders without Yjs and is the template.
3. Comments: anchor by `(path, line, line hash)`; adjust useComments.ts and the comment
   components; if `comment_index` lacks the hash, add it with a migration.
4. Delete frontend/src/lib/crdt.ts, hooks/useDraft.tsx, components/draft/*,
   useAutoCommitBeforeAction.ts, CRDT tests and mocks (test/crdt*.test.tsx,
   test/__mocks__/y-websocket.ts, vitest.config.crdt.ts, e2e/crdt-*.test.ts), the
   `draft.*` handlers in lib/ws.ts, and the deps yjs, y-websocket, y-monaco. Remove the
   WorkspacePage `crdt.yDoc/provider` plumbing and the 2.5 s file-tree polling (refetch
   on `workspace.branch_updated`).
5. Playwright: delete crdt-*.spec.ts, drafts.spec.ts, manual-commit-crdt-rollover.spec.ts,
   ui-auto-commit-before-actions.spec.ts, workspace-overlay-collaboration.spec.ts,
   draft-session-scope-isolation.spec.ts, typing-latency.spec.ts; add
   explicit-save.spec.ts (save, conflict dialog, spec editor save, comment anchor
   survives a save).

Do not: touch chat, jobs or PR components; change backend code except the comment
anchor column.

Acceptance: `grep -rn "yjs\|y-websocket\|y-monaco\|Y\.Doc" frontend/src` is empty;
frontend build and tests pass; the Playwright explicit-save flow passes.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P2.4 — Delete the backend CRDT layer

```
You are implementing P2.4 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §3.2 (CRDT row), §9, and
docs/research/research-crdt-docs.md §G. Depends on P2.2 (P2.3 merged). Owner: Workspace.

Goal: nothing in the repository references Yjs, drafts or the task-commit protocol.

Do:
1. Delete crates/modules: backend/g5-crdt (remove from Cargo.toml members),
   backend/g5-api-core/src/crdt/*, backend/g5d-foundation/src/crdt/* (including the
   relocated task_rebase_gate and connected_checkout), backend/g5-api-core/src/services/draft_service.rs,
   backend/g5-api-routes/src/routes/drafts.rs, backend/g5-workspace-ops/src/draft.rs
   (callers assume clean), backend/g5-api-data/src/models/draft.rs, the
   `/api/v1/yjs/:workspaceId/:branchId` route and `crdt_ws_adapter`, the internal routes
   `/tasks/:id/crdt/edit-file-replace` and `/tasks/:id/drafts/commit-all`, `draft.*`
   WsEvents and `Channel::Draft`, `CrdtConfig` env vars, `yrs` from workspace deps; the
   host supervisor's CRDT peer and fs→CRDT bridge (`LEAN-TODO(P2.4)` in
   backend/g5d-host/src/supervisor.rs); every other `LEAN-TODO(P2.4)` site (the landing's
   draft rollovers, dirty bookkeeping, task overlay watermark).
2. Migration `NNN_remove_crdt.sql`: drop `draft_sessions`, `draft_updates`,
   `draft_participants`, `task_commit_requests`, `task_commit_frontier_cuts`,
   `task_commit_frontier_receipts`; drop `task_crdt_head_commits` from the assignment DTO; set the sticky-runtime `teardown_repository_scopes` to empty (P4.7 deletes sticky entirely).
3. Remove `crdt` from the instance-info feature list and every mention in Helm, compose
   and env examples (`G5_CRDT_*`, `G5_TASK_CRDT_PERSISTENCE_ENABLED`,
   `G5_CRDT_PERSISTENCE_BARRIER_EXACT_V2_ENABLED`) and the single-replica CRDT comment in
   infra/helm/g5/values.yaml.
4. Delete exclusive tests (api_drafts.rs, workspace_overlay_drafts.rs,
   draft_session_scope_contract.rs, e2e crdt_collab.rs, crdt_draft_ops.rs,
   persistent_task_crdt_restart.rs, Playwright leftovers) and retarget any test that only
   used drafts as a way to create a commit to the P2.1 Save API or plain git; classify
   every removed test.

Do not: touch task lifecycle, sticky runtimes beyond that field, or governance.

Acceptance: `grep -rn "yrs\|crdt\|Crdt\|draft_session\|DocumentHandle" backend frontend/src --include='*.rs' --include='*.ts' --include='*.tsx'`
returns nothing except historical comments; the workspace compiles; docker-ci passes.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P3.6 — Session-resident Claude CLI process behind `agent.turn` (post-MVP)

```
You are implementing P3.6 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §8.1, §8.3, §8.7, §17.2,
docs/lean-provider-cli-contract.md §2.1 (the process waits for the next stdin message
and exits at EOF), and docs/agent-api-surface.md §8.1 and §10 (what the SDK's
`streamInput`, `interrupt`, `setModel` and `setPermissionMode` do; all of them are
messages on the CLI's own stdio protocol). Depends on P4.11. Owner: Agents.

Goal: a second Claude adapter for the `agent.turn` module that keeps the CLI process
resident for a chat session (stdin open) so a queued message can steer or follow a
running turn without a cold start, an interrupt can be sent in band, and a model or
permission change can apply without a new process; behind the same module input,
event and outcome schemas; selected per session by a feature flag with the per-turn
adapter as the rollback lane. The Claude Agent SDK is not used; its published
`sdk.d.ts` may be used to generate or check the Rust event types.

Do:
1. Probes first, recorded as fixtures with `tools/fake-provider-cli/record.sh` on the
   pinned CLI: a second user message written while a turn runs (folded into the turn or
   queued until the next one, and how `--replay-user-messages` echoes it); the in-band
   interrupt control message (does the turn end, does the process survive, which frames
   follow); `session_state_changed` under `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1` for
   turn-end settlement; memory of one process across many turns. Record each outcome in
   docs/lean-provider-cli-contract.md before writing adapter code; a capability the probe
   does not confirm is not implemented.
2. The resident adapter: one process per chat session from its first turn until the
   session idles (a per-session idle timeout shorter than the VM's), the VM stops, or the
   user closes the chat; turn boundaries from the settlement rule of design §8.3 (result
   frame plus `session_state_changed idle`, and for the last turn EOF and exit status);
   `input.accepted` journaled per message on the replay echo; steering as a stdin write
   when the probe confirmed folding, otherwise queued delivery to the next turn on the
   same process; interrupt in band when confirmed, else SIGINT; the execution identity
   stays per turn (`exec_id` per attempt), so the journal, dispositions and outcomes are
   unchanged. A process is never left resident while a question is open past its runner
   wait (suspension reaps it, design §8.4) or while a background job it announced still
   runs.
3. Selection per session by a `configs` flag; the per-turn adapter stays the default
   until the gates pass in QA. Extend `fake-claude` to run several turns on one process.
4. Gates (design §8.7): exhaustiveness fixtures over the recorded frames, unknown
   discriminant and contradictory-evidence tests, the golden turns including steering,
   in-band interrupt and a session that outlives an idle stop, the durability test, the
   redaction tests, and a memory budget over 200 turns; a short amendment to the design
   recording the confirmed capabilities.

Do not: change the module schemas; remove the per-turn adapter (a later PR after
rollout); introduce the Claude Agent SDK or a Node adapter; change Workspace or
Worker & Host code.

Acceptance: both adapters pass the same golden turns; the flag switches a session
between them; a queued message reaches a resident process without a cold start;
docker-ci passes.
Validation: cargo check, lint, fmt, `cargo test -p <module crate>`, `just docker-ci`.
```

### P3.7 — Codex App Server adapter with interactive approvals (post-MVP)

```
You are implementing P3.7 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §8.3, §17.2, and
docs/agent-api-surface.md §11. Depends on P4.11. Owner: Agents.

Goal: Codex turns that can answer their own tool-permission approvals through
`task_questions(kind=approval)`, using the Codex App Server JSON-RPC protocol bounded
to one turn, with process exit still the turn boundary.

Do:
1. Prototype first: a verified approval round trip against the pinned binary
   (`codex app-server`, `turn/start`, the approval request item, the answer, `turn/completed`),
   recorded as fixtures with `tools/fake-provider-cli/record.sh`; extend `fake-codex` to
   replay them.
2. The adapter: one App Server process per turn; approvals become `task_questions`
   rows and are answered from the committed row; questions keep using `ask_user`;
   settlement from `turn/completed`, EOF and exit status.
3. Selection per session by feature flag; `codex exec` stays the default until the
   golden turns pass in QA. Remove the "Codex runs non-prompting policies only" rule
   from the design and the runner when the flag becomes the default (a follow-up note).

Do not: change the module schemas; touch the Claude adapter.

Acceptance: a fake-codex turn blocks on an approval until the UI answers, then
continues; docker-ci passes.
Validation: cargo check, lint, fmt, module tests, `just docker-ci`.
```

### P4.1 — Migration: `workspace_vms`, `workspace_flushes`, execution identity, `task_events`, inbox, hosts

```
You are implementing P4.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 (item 6), design v7 §5.1, §6.2 (host
record), §7.1, §8.1, §12, and docs/database-migrations.md. Depends on P0.2 (merged).
Owner: Workspace; reviewed with Agents and Worker & Host.

Goal: one forward migration with the tables and columns the dispatcher, the flush,
the host protocol, the fence and the event contract need. Schema only, plus models.

Do (one file `002_workspace_vm_runtime.sql`, or the next unused number, plus
backend/g5-api-data/src/models):
1. `workspace_vms(workspace_id PK, host_id NULL, vm_id text NULL, epoch bigint default 0,
   status stopped|starting|running|stopping|failed, image_ref, image_digest,
   module_catalog_version, lease_expires_at, started_at, last_activity_at,
   stop_reason NULL, stop_deadline_at NULL, last_flush_id NULL, unflushed_loss_at NULL,
   last_error)` with org RLS.
2. `workspace_flushes(flush_id uuid PK, workspace_id, vm_id, epoch, reason, materialization,
   repos jsonb, concurrent_writers jsonb, started_at, complete_at NULL, error NULL)` with
   org RLS and an index on `(workspace_id, complete_at desc)`.
3. `workspace_members(workspace_id, user_id, role write|read)` if the existing members
   table lacks the role; `workspace_settings` columns for idle timeout, runner wait,
   answer expiry and retain hours if absent.
4. `tasks`: add `attempt int default 1`, `vm_id text NULL`, `epoch bigint NULL`,
   `start_confirmed bool default false`, `wall_clock_limit_secs int NULL`; keep existing
   status values for now. `task_attempts(task_id, attempt, vm_id, epoch, started_at,
   ended_at, outcome jsonb, reason, acked_event_seq bigint default 0, PRIMARY KEY
   (task_id, attempt))`.
5. `task_events`: rename `timeline_entries` to `task_events` (a view named
   `timeline_entries` is acceptable for one release), add `attempt int`, `vm_id text`,
   `seq bigint`, `UNIQUE (task_id, attempt, vm_id, seq)` (nullable for rows written by the
   pre-dispatcher path; P7.2 tightens).
6. `task_questions`: `exec_id` becomes structured text `<task_id>:<attempt>:<vm_id>`;
   add `runner_wait_deadline_at`, `suspended_at`, `answer_version int default 0`, status value `consumed`.
7. `chat_sessions.session_manifest_generation int default 0`; `chat_messages.revision int
   default 1`, `delivery text NULL` (queued|reserved|delivered|delivery_unknown|withdrawn),
   `author_id` if absent; `chat_session_files.harvested_epoch bigint NULL`.
8. `worker_hosts`: add `status` value `unreachable`, `slots_total`, `slot_cpus`,
   `slot_mem_mib`, `scratch_disk_mib`, `runtime_identity jsonb`, `capabilities text[]`,
   `credential_expires_at`, `drain_requested_at`, `last_error` where absent; leave the
   per-task columns for P4.9b to drop.
9. `configs(scope, scope_id, key, value jsonb, PRIMARY KEY (scope, scope_id, key))` for
   feature flags and settings.
10. Append the SHA384 to the checksum catalog; regenerate the SQLx cache; adjust models
    and the few queries that name `timeline_entries`.

Do not: change behavior; no service logic; no baseline edit.

Acceptance: fresh install and upgrade from the current schema both pass; the no-op
re-run passes; `just test-integration` passes; `just docker-ci` bootstrap phase passes.
Validation: migration checks, `just db-prepare`, cargo check, `just test-integration`.
```

### P4.2a — `g5d dispatch`: listener, private protocol, files, ports, status

```
You are implementing P4.2a of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.6 (the runtime API and the
private protocol), §4.2, docs/workspace-vm-api.md §1, §2 (Files, Ports, Workspace
status, Runtime inspection), §4, and docs/research/research-exec-isolation.md §E.6
(guest listener + mux + token pattern), §H. Depends on P3.2, P3.3, P3.4 (merged).
Owner: Workspace.

Goal: a new `g5d dispatch` subcommand that runs inside a workspace VM, accepts the
host's authenticated connection, serves the file, port, status and inspection surfaces
of the Workspace Runtime API and the private lifecycle protocol, and can be run locally
against a directory. The module runtime is P4.2b, the flush is P4.3a, terminals are
P5.2, the preview is P5.1; none of them is stubbed here, and the method set this PR
serves is exactly the set below.

Do:
1. New crate backend/g5d-dispatch (add to Cargo.toml members and to the `g5d` binary as
   `Command::Dispatch`). Config from env: `G5D_WORKSPACE_ID`, `G5D_VM_ID`, `G5D_EPOCH`,
   `G5D_WORKSPACE_TOKEN`, `G5D_API_URL`, `G5D_DISPATCH_PORT`, repo layout from
   `/work/.g5/layout.json`, `HOME=/home/agent`, state dir `/var/lib/g5d`.
2. Listener: accept one authenticated connection from the host on the guest port
   (32-byte token frame first, as backend/g5d-host/src/preview_attach_wire.rs does), then
   multiplex with `g5_internal_api::stream_mux`: channel `rpc` (JSON-RPC 2.0,
   newline-delimited). A new connection replaces the old.
3. Methods: `fs.list/read/stat/write(path, content, expected_hash)/delete/rename/watch`
   with the `file.changed` watcher; `ports.list` with `port.opened/closed` (reuse
   backend/g5d-worker/src/port_scan.rs); `workspace.status.get`;
   `runtime.inspect.summary`; the private protocol `runtime.ready` (sent to the peer on
   boot with `vm_id`, `epoch`, `dispatcher_contract_version`), `lease.renew` (a request
   to the peer at half the TTL; the returned token is installed only when its `vm_id`
   claim matches), `health`, `admission.close/open` (close returns when in-flight `fs`
   mutations finish and refuses new ones until open). A method outside this set answers
   `method_not_found`; nothing is stubbed.
4. Platform excludes and identity: write `/var/lib/g5d/excludes` (`.g5/cache/` and the
   other runtime subdirectories) and set `core.excludesFile` in a global gitconfig for the
   processes this VM runs (reuse task_git_config.rs); a git credential helper that serves
   the dispatcher's own token for the internal git service (used by P4.3a's flush;
   provider processes never receive it, P4.11).
5. Tests (crate unit plus an integration test that runs `g5d dispatch` against a temp
   directory with a test client on the mux): auth token rejected; every method above; a
   write with a stale hash returns 409; `file.changed` after a native write;
   `admission.close` waits for an in-flight write and refuses a new one; `lease.renew`
   installs a token only when the claim matches; `runtime.ready` carries the identity.

Do not: implement modules, the journal or dispositions (P4.2b), the flush (P4.3a),
terminals (P5.2), the preview (P5.1); touch the host or the API.

Acceptance: the crate's tests pass; `g5d dispatch --help` works from the built binary;
the served method set equals the list above.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`.
```

### P4.2b — `g5d dispatch`: module runtime, journal, dispositions, built-in modules

```
You are implementing P4.2b of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.6, §6.7, §7.5 (journaled
events, replay, durable cancellation), docs/workspace-vm-api.md §2 (Modules, Module
diagnostics, Diagnostic bundle), §3, §4. Depends on P4.2a. Owner: Workspace.

Goal: the declared-module runtime of the dispatcher: the catalog, invocation with
dispositions, the event journal with replay and acknowledgement, the retained
disposition table, module diagnostics, and the built-in `workspace.changes`,
`workspace.bootstrap` and `workspace.diagnostics` modules. `agent.turn` (P4.11) and
`workspace.preview` (P5.1) register their catalog entries in their own PRs.

Do:
1. Catalog: `/opt/g5/modules/catalog.json` in the runtime image, hash-pinned, with the
   declaration fields of design §6.7; `modules.list/describe`; resolution, input
   validation, capability intersection with the `grant_id`; a private process runner
   with the limits (time, CPU, memory, output).
2. `modules.invoke/status/cancel` with dispositions `started | already_running |
   already_finished(outcome) | cancelled` idempotent on `exec_id`; the admission barrier
   of P4.2a extended to module invocations.
3. Event journal: per `exec_id`, an append-only file under
   `/var/lib/g5d/journal/<exec_id>` with a per-execution `seq`; every event journaled
   before it is streamed on `rpc`; `events.replay(exec_id, from_seq)` streams from the
   journal then live; `events.ack` prunes; `outcomes.pending`; a disk budget with
   oldest-acknowledged-first pruning; the outcome is the last event. A separate small
   dispositions table under `/var/lib/g5d/dispositions` holds `cancelled` and
   `already_finished` (with the outcome digest) per `exec_id` for the life of the
   incarnation, never pruned with the journal; `modules.cancel` records a cancellation
   disposition even for an unknown `exec_id`, and a later `invoke` of that identity
   returns `cancelled` without spawning.
4. Built-in modules: `workspace.changes` (`get`, `commit(message, paths?, push)`, `push`,
   `update` refused when dirty, `switch_back`, thin wrappers over the git binary honoring
   the excludes file and the P4.2a credential helper), `workspace.bootstrap` (`run` =
   `g5d bootstrap`), `workspace.diagnostics` (`capture(profile)`, `probe(name)` with the
   four fixed probes); `modules.logs/events/history` and `diagnostics.capture/probe`.
5. Tests: invoke idempotency by `exec_id` returns the right disposition; cancel before
   invoke then invoke returns `cancelled` even after the journal was pruned; journal
   replay after a client disconnect mid-run yields a complete ordered list; ack prunes;
   a module over its limit is stopped with a definite outcome; catalog integrity probe;
   `workspace.changes get` reports the association, dirty set and ahead/behind; `update`
   refused when dirty.

Do not: implement `agent.turn` (P4.11), the flush (P4.3a), the preview (P5.1); touch
the host or the API.

Acceptance: the crate's tests pass; every module operation above is real.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`.
```

### P4.3a — Shutdown flush, receipts, restore, stopped reads

```
You are implementing P4.3a of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.3 (flush-ref rows), §5.4
(all), §5.5, §9.1, §9.4, docs/workspace-vm-api.md §5, and design v7 §0 items 2 and 3
(why receipts and quiescence). Depends on P4.1, P4.2a. Owner: Workspace; the
receive-pack rule with Git and governance.

Goal: the dispatcher's `workspace.flush` for reason `shutdown`, the receipt in the API,
restore on attach, `source: flush` reads, and the receive-pack rule for flush refs.
There is no periodic checkpoint. Live flushes (`fork`, `explicit`) and their
quiescence are P4.3b; a `workspace.flush` with one of those reasons is refused with
`unsupported_reason` until then.

Do:
1. Dispatcher `workspace.flush({reason: shutdown})`: for each member repo run exactly the
   recipe in design §5.4 (ls-remote observation, seeded temporary index with
   `--really-refresh` tolerating exit 1, `add -A`, `write-tree`, `commit-tree -p HEAD`
   skipped when the tree equals HEAD's, push to `refs/g5/flush/<workspace id>/<flush id>`
   with the P4.2a credential helper), then report the vector to the API
   (`POST /api/internal/workspaces/:id/flushes/:flush_id/complete`). Flushes are
   serialized; the P4.2a admission barrier is closed by the caller (the stop sequence,
   P4.5) before a shutdown flush and the flush verifies it is closed.
2. API: `workspace_flushes` rows (P4.1): `start` inserts the row; `complete` verifies with
   `git for-each-ref` that every reported flush ref exists at the reported `snapshot_oid`,
   then sets `complete_at` and `workspace_vms.last_flush_id` in one transaction; a
   mismatch fails the flush. Pruning (maintenance): keep the two newest complete receipts
   per workspace, delete older flush refs with the server identity, delete refs of
   incomplete flushes 24 h after `started_at`.
3. Restore on boot (dispatcher, from the `restore` block of `/work/.g5/layout.json`,
   which P4.4b's host writes from `VmSpec.restore`): clone or reflink each member repo,
   fetch the branch and the flush ref, `git checkout -B <branch> <local_head_oid>` with
   upstream `origin/<branch>`, materialize the snapshot tree into the working tree
   without moving HEAD (including untracked files present in the snapshot and absent
   from HEAD), remove untracked, non-ignored paths absent from the snapshot, leave
   changes unstaged; no automatic merge when origin moved. A warm retained root is a
   cache of ignored files and objects only: run the same steps on it; never use `HEAD`,
   `git status` or a path list as proof that the tree equals the receipt.
4. Receive-pack (routes/git.rs): allow `create` of `refs/g5/flush/<workspace id>/<flush id>`
   for the workspace incarnation whose claims name that workspace (worker/task token now;
   the workspace token from P4.5), refuse update and delete for every network identity;
   the server identity may delete.
5. API reads: `source: flush` in files, docs and ontology reads when the VM is not
   running and a complete receipt exists (resolve the snapshot tree); the Changes panel
   data while stopped from the receipt's dirty set; `flush.completed` feeds the P1.3
   convergence loop for the snapshot ref.
6. Tests (dispatcher integration with a local bare "origin", API tests): a two-repo flush
   whose second push fails, and separately whose completion fails verification, leaves
   the previous receipt as `last_flush_id` and a cold restore uses it; retained refs
   still resolve after two later flushes; a tracked-but-ignored file with a dirty edit is
   captured and restored, including a same-second same-size edit; after a flush
   `origin/<branch>` is unchanged and a `--force-with-lease` push that would discard an
   unseen commit is still rejected; committed-but-unpushed on a clean tree restores with
   the branch ahead of origin; local commits plus dirty edits restore both; untracked,
   deleted and ignored files handled as specified; a warm root whose file contents
   changed after the receipt with the same HEAD, dirty paths and sizes attaches with
   exactly the cold content; provider session files never appear in a flush; a flush with
   reason `fork` or `explicit` is refused; API test for `source: flush` and for pruning.

Do not: implement live flushes or fork (P4.3b), the stop sequence (P4.5) or host
commands (P4.4b).

Acceptance: all listed tests pass; the recipe in code matches design §5.4 in effect;
no periodic task exists.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`, `just test-integration`.
```

### P4.3b — Live flush quiescence and fork

```
You are implementing P4.3b of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.4 (Quiescence, Failure
policy), §5.8 (Forking a workspace), §6.7, and design v7 §0 item 3. Depends on P4.2b,
P4.5. Owner: Workspace.

Goal: `workspace.flush` for reasons `fork` and `explicit` on a running VM with the
quiescence rule, the "Flush now" action, and forking a running workspace.

Do:
1. Dispatcher: for `fork` and `explicit`, close module admission and file-mutation
   admission (the P4.2a barrier), wait for admitted mutations to finish, wait up to
   `quiesce_secs` (default 30) for bounded-run executions, refuse with `WORKSPACE_BUSY
   {active_exec_ids}` unless `interrupt: true` (then interrupt them through
   `modules.cancel` as a stop would), hold the barrier through the capture of every
   member repository, record open terminals and a running preview as
   `concurrent_writers`, run the P4.3a recipe, and reopen both admissions on every
   completion, refusal and failure path. Remove the `unsupported_reason` refusal.
2. API: `POST /workspaces/:id/flush` (reason `explicit`; a safety action that does not
   stop the VM); forking a running workspace in `create`: flush the source with reason
   `fork` (the caller passes `interrupt` when the user chose "stop them and fork"), copy
   the receipt's flush refs under the fork's workspace id, create the fork's branch at
   each repo's `local_head_oid`, and set the copy as the fork's `last_flush_id`. A failed
   fork flush aborts the fork and leaves the source running.
3. Frontend: "Flush now" in the status strip; the fork dialog shows active executions,
   terminals and the preview and offers "stop them and fork".
4. Tests: a fork flush with an active execution is refused, then succeeds with
   `interrupt: true`; a Save admitted just before a fork flush and released while the
   capture runs is waited for and the cut is consistent (also delete and rename, and two
   repositories); both admissions reopen after a refused and after a failed live flush;
   a fork of a running workspace restores the dirty tree in the fork; a fork of a stopped
   workspace uses its last receipt; a failed fork flush leaves the source running.

Do not: change the shutdown flush or the receipt shape (P4.3a).

Acceptance: listed tests pass; docker-ci passes.
Validation: cargo check, lint, fmt, `cargo test -p g5d-dispatch`, `just test-integration`, `just docker-ci`.
```

### P4.4a — `g5d host`: the `ctl` protocol on both ends, host credential, event journal, drain

```
You are implementing P4.4a of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.1 (host states), §6.2 (all),
§6.10 (drain), docs/worker-host-api.md §2, §3 (all; normative), §8, §12. Depends on
P4.1. Owner: Worker & Host.

Goal: the versioned `ctl` stream between a host and the control plane, on both ends,
with everything that does not need a VM: hello, accept and reject with version
negotiation, heartbeat, the journaled `vm.event` channel with acknowledgement, the
renewable host credential, `host.drain` and `host.drained`, command deadlines and
idempotency keys with retained results, and the control-plane client. Booting VMs is
P4.4b; nothing here pretends to.

Do:
1. Protocol crate types in backend/g5-internal-api: `host.hello/accept/reject` with
   `protocol_version` and `runtime_identity`, `host.heartbeat`, `vm.event` with per-VM
   `seq` and `vm.event.ack`, `lease.renew`, `host.credential.renew`, `host.drain`,
   `host.drained`; the command envelope (deadline, idempotency key) and the result
   variants of design §6.2 for `vm.start`, `vm.stop`, `vm.status`, `vm.grants.update`
   (the types only; their handlers arrive with P4.4b and P4.12).
2. Host side (backend/g5d-host): the `ctl` stream on the existing tunnel; version
   negotiation with self-drain on `host.reject` (refuse `vm.start`, keep running VMs and
   lease timers, retry with backoff); a command received after its deadline refused as
   `expired`; results retained for `result_retention_secs`; `host.drain` (refuse
   `vm.start`, report `host.drained` when no VM remains); the `vm.event` journal
   (monotonic `seq` per VM, replay of unacknowledged events on reconnect before anything
   else); heartbeat every `heartbeat_secs` with the fields of design §6.2; the renewable
   host credential replacing the fixed 24 h host JWT (`host.credential.renew` from 2 h
   before expiry; the old credential valid until its expiry).
3. Control-plane side: `HostCommandClient` (commands with deadlines and idempotency
   keys, timeouts that are not negative results, retry with the same key); ingestion of
   `vm.event` on the unique key `(vm_id, seq)` with `vm.event.ack`; hello and heartbeat
   answered with the current-incarnation set read from `workspace_vms`; credential
   renewal refused while the host is `draining`, `terminated`, or its runtime identity
   changed; `lease.renew` handled by extending `lease_expires_at` from the handling time
   (the dispatcher and host halves of renewal arrive with P4.4b).
4. Tests: version reject leads to self-drain that keeps VMs and timers; a `vm.event`
   survives a tunnel drop and is replayed before other traffic; a command after its
   deadline is refused; a retried command gets the retained result; credential renewal
   refused while draining and the old credential still valid; `host.drain` reports
   `drained`.

Do not: boot or stop VMs, validate a `VmSpec`, run a lease timer (P4.4b); implement
the proxies (P4.12), placement or the host registry columns (P4.13).

Acceptance: a host connects, negotiates, heartbeats, journals and replays events, and
renews its credential against the control plane; the old host JWT path is gone.
Validation: cargo check, lint, fmt, `cargo test -p g5d-host -p g5-internal-api`,
`just test-integration`.
```

### P4.4b — `g5d host`: `VmSpec`, boot and stop, lease, stream relay, retained roots

```
You are implementing P4.4b of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.2 (commands), §6.3, §6.4,
§6.10 (limits), docs/worker-host-api.md §3.2, §4, §5, §7, §7.1, §10, design v7 §0 item
5 (the lease bound), and docs/research/research-exec-isolation.md §A, §D, §E. Depends on
P4.2a, P4.4a. Owner: Worker & Host.

Goal: the host boots and stops workspace VMs from a `VmSpec`, connects to each VM's
dispatcher, relays streams, enforces the per-VM lease with the bound of design §6.4,
reports its inventory on reconnect and keeps retained roots. Per-task assignment stays
in place until P4.7/P4.8 move work off it; this PR adds the new path beside it.

Do:
1. `VmSpec` of design §6.3 with `spec_version`, validated in the documented order with
   the named refusals; `credential_grants` and `secrets_env` resolved through the
   control plane at boot with values kept in host memory (`boot_failed(grant_unresolved)`
   on failure; `secrets_env` goes into the dispatcher's environment; the grants are
   served by P4.12's proxies).
2. Boot with libkrun (backend/g5d-isolation) with entry `/opt/g5/bin/g5d dispatch`,
   mounts for `/work`, runtime bin, provider assets, scratch disk, the layout JSON in
   `/work/.g5/` including the `restore` block from `VmSpec.restore`, the env of design
   §6.3, and the guest `/etc/hosts` seeded from the configured `G5D_GUEST_HOST_ALIASES`;
   connect to the dispatcher listener with the per-VM token (reuse the preview-attach
   wiring) and relay `rpc`, `pty` and `http` streams (one `http` stream per TCP
   connection; `pty` and `http` acceptors in the guest arrive with P5.2 and P5.1, whose
   tests exercise those tags); `vm.status`; `vm.stop graceful` as SIGTERM to the guest
   init with SIGKILL at `grace_secs`, fenced by ownership only (any `vm_id` this host
   runs is stopped, current or not; `not_found` for one it does not run; never `stale`);
   `workspace_busy` when a previous incarnation of the same workspace has not exited;
   the limits of design §6.2 with definite refusals; `G5D_HOST_MAX_MICROVMS` shared by old
   per-task VMs and new workspace VMs during the transition.
3. Lease: a host-local timer per VM armed from `lease_ttl_secs` at receipt of `vm.start`
   (the control plane grants `lease_ttl_secs + start_deadline_secs`) and re-armed only by
   a `lease.renew` answer, measured from the monotonic time the request left the host;
   renewals carry a sequence number and obsolete replies are ignored; the dispatcher's
   `lease.renew` on `rpc` forwarded on `ctl` and the answer passed back as an opaque
   blob; expiry kills the VM and emits `lease_expired_killed`; a `stale` answer kills it
   and emits `stale_killed`.
4. Reconnect: `host.hello` carries the inventory; on `host.accept` kill every VM not in
   `current_incarnations`, then replay the unacknowledged `vm.event` journal.
5. Retained roots: keep the workspace root and scratch disk for `retain_hours` after
   `vm.stop`, evict oldest first under disk pressure, report `retained_roots` on hello and
   heartbeat.
6. Tests: backend/g5d/tests/host_up_smoke.rs extended; a test control plane driving
   start, stop, status and inventory over the tunnel; a workspace VM boots from a
   `VmSpec`, the dispatcher reports `ready`, an `fs.read` over the relayed `rpc` channel
   works, `vm.stop` stops it; a reconnect with an unknown `vm_id` kills that VM; a delayed
   renewal reply followed by a partition expires the host timer before the server grant;
   a lost `vm.start` result is resolved by the inventory; `vm.stop` for a superseded
   `vm_id` the host still runs stops it and frees the slot while `vm.start` and stream
   opens for it are refused; `workspace_busy`; the docker-ci isolation lane boots a
   workspace VM to `ready`.

Do not: implement the proxies or `vm.grants.update` (P4.12), placement or the host
registry (P4.13), the API's `workspace_vms` logic (P4.5), or delete per-task paths
(P4.9a).

Acceptance: the listed tests pass; a workspace VM boots to `ready`, serves `fs.read`
through the relay, is stopped on command, and is killed when superseded.
Validation: cargo check, lint, fmt, `cargo test -p g5d-host -p g5d`, `just docker-ci`.
```

### P4.5 — API: lifecycle service, five actions, stop sequence, fence, Workspace Runtime facade

```
You are implementing P4.5 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.2, §5.5, §5.8 (all), §6.1,
§6.4, §6.6 (the facade), §9.1, §9.2, docs/workspace-vm-api.md §1, §2, §4,
docs/agent-api-surface.md §4.1, and design v7 §0 item 9 (Save ordering). Depends on P4.1, P4.3a, P4.4b, P4.13. Owner: Workspace. Module executions and the Agents stop hooks do not exist yet; P4.6 adds their step to the stop sequence and the module surfaces to the facade.

Goal: the API owns `workspace_vms`, the five workspace actions, the teardown reasons and
the stop sequence with the flush, the idle reconciler, the incarnation fence at every
boundary, and the Workspace Runtime facade that every other platform system uses.

Do:
1. `WorkspaceVmService`: `resume` (CAS on the row: `starting`, new `vm_id`, `epoch + 1`,
   the host from P4.13's placement, the image resolved from the org default or `.g5/workspace.yaml`, a lease of `lease_ttl_secs + start_deadline_secs`; build the `VmSpec` with `restore` from `last_flush_id`, `credential_grants` and `secrets_env` (the host resolves them, P4.4b; the proxies that serve the grants are P4.12); send `vm.start`; `queued_for_capacity` when placement returns `no_capacity`),
   `ready` (`vm.event ready` → `running`, record `module_catalog_version`), `stop(reason,
   deadline)` (the stop sequence of design §5.8: decide → close admission (the facade refuses new work; `admission.close` on the dispatcher drains file mutations) → quiesce: close the terminals and the preview where P5.2 and P5.1 have landed → `workspace.flush shutdown`, retry with backoff, abort for `user` and `idle` on failure, proceed at the
   deadline for `drain|rollout|credential|capacity` recording `unflushed_loss_at` → harvest the session files → `vm.stop graceful`, waiting for the host's `exited` event or `stopped(exit)` result while the row stays `stopping` → release: `stopped`, `vm_id` cleared, `last_flush_id`), `restart` (stop with reason `restart`, then resume), `delete` (stop or force, delete branch, flush refs and receipts, rows), `create` (fork the source branch head, or, for a stopped source workspace, its last receipt with the copied flush refs; rows, carry, resume; forking a running workspace is P4.3b). Base workspaces never get a VM. Idle
   reconciler: stop with reason `idle` when `last_activity_at + idle_timeout` passed and
   nothing counts as activity (running executions, an open terminal, preview traffic,
   file RPC, an open question within its runner wait).
2. Teardown nudge: for deadline-bound reasons, a notification to the members and a
   `notice` appended to every running chat session before the stop sequence starts;
   the deadline default 30 minutes after the nudge.
3. Fence: reject a token whose `vm_id` is not the row's current one at receive-pack and
   upload-pack (git.rs), session-file routes (tag rows with `vm_id` and `epoch`; resolve
   `LEAN-TODO(P4.5)`), task event and outcome routes, question routes, flush routes and
   facade replies; accept `starting|running|stopping`. Reconciler: lease expiry or a host
   `exited|oom|lease_expired_killed|stale_killed` event in any status → `failed`, `vm_id`
   and `host_id` cleared, active executions settled (interim mapping onto today's
   failure path until P4.6); a `stopping` row whose VM a host still lists gets the
   idempotent `vm.stop` re-sent (a lifecycle crash after the stop command) and is
   released on the host's exit.
4. Workspace Runtime facade: `workspace.runtime.ensure(request)` and `get` returning the handle of docs/agent-api-surface.md §4.1, `workspace.runtime.events`, and the files, ports, status and inspection surfaces of P4.2a, each bound to `workspace_id`, `expected_vm_id`, actor or grant, deadline and idempotency key; the only code that opens `rpc`, `pty` or `http` streams through P4.4a's client (the module and event surfaces are added by P4.6, the terminal by P5.2, the preview by P5.1).
5. Reads and writes: `source: live` via `fs.read`/`fs.list` when running (P4.3a handles flush and committed); Save → authorize → refuse `WORKSPACE_STOPPING` → ensure running
   (resume, surface "resuming" or `queued_for_capacity`) → `workspace.changes get` on
   that incarnation → `fs.write` with `expected_vm_id`; delete the P2.1 server-side
   commit path; docs and ontology PUTs follow. `WORKSPACE_BRANCH_MISMATCH` on a mismatch.
6. Routes: `POST /workspaces/:id/{resume,stop,restart,delete}`, `GET /workspaces/:id/vm`; chat send and task create call `ensure` when needed (the full dispatch loop is P4.6; here inline). `GET /workspaces/:id/changes` and `POST /workspaces/:id/flush` arrive with P4.6 and P4.3b.
7. Tests: api_workspace_vms.rs: five actions; a base workspace has no VM; idle stop; the
   stop sequence refuses new Saves, completes an in-flight Save whose content is in the
   receipt, aborts on a failed flush for a user stop with admission reopened, proceeds at
   the deadline for a drain and records the loss; a reconciler tick right after admission
   closes does not mark loss; stale `vm_id` refused at git, session files, events,
   questions, flush routes and facade replies; reconnect inventory kills a stale VM; Save
   with no VM reaches resume before any module query, then a matching branch and a
   mismatch; live read and write through a running dispatcher (docker-ci with a workspace VM); a restart picks up a changed `base_image`; release follows the host's exit
   and a restart right after a stop never sees `workspace_busy`; a lifecycle restart
   after `vm.stop` re-sends it and then releases.

Do not: implement the dispatch loop, two-phase invocation, the six-state model or the module surfaces of the facade (P4.6); live flushes and fork (P4.3b); placement (P4.13); the proxies (P4.12).

Acceptance: the listed tests pass; a workspace can be created, used for Save and live
reads, stopped with a receipt, and resumed with its receipt restored.
Validation: cargo check, lint, fmt, `just test-integration`, `just docker-ci`.
```

### P4.6 — Dispatch loop, two-phase invocation, event contract, reconciler, question deadlines

```
You are implementing P4.6 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §7.1 to §7.5 (all), §8.1
(inbox), §8.4, docs/agent-api-surface.md §3, §4.2, §4.3, §8, §12, the draft 6 review
G1 to G4 (docs/lean-backend-design-v6-review.md) and design v7 §0 item 7 (durable
cancellation). Depends on P4.5, P4.11. Owner: Agents.

Goal: the six-state task model and the dispatch and event contract, with `TaskService::apply(event)` as the only writer of task state, invoking modules through the Workspace Runtime facade, whose module and event surfaces this PR adds. Existing producers (chat send, job creation) create `queued` tasks; consumers are switched in P4.7 and P4.8.

Do:
0. Facade: add `modules.describe/invoke/status/cancel`, `modules.events.replay/ack` and `modules.outcomes.pending` to the Workspace Runtime facade (P4.5), bound to `expected_vm_id`; add to the stop sequence the quiesce step that interrupts module executions through `modules.cancel` and the Agents stop coordination (`agents.workspace_stops.prepare` before the flush, `commit` after the harvest); add `GET /workspaces/:id/changes` over `workspace.changes get` and the association check before Save.
1. New `backend/g5-api-core/src/services/task_lifecycle.rs` implementing the six states
   and transitions of design §7.2, including outcomes `interrupted`, `suspended`,
   `timed_out`, `retry_on_vm_loss` re-queue as a fresh attempt, question expiry, wall-clock
   limit, and successor-turn creation only for `interrupted|suspended|vm_lost` with an
   answered unconsumed question (or for drain/rollout interruptions with none), under the
   same lock as cancel (unique `successor_task_id`). The old kernel stays for tasks not
   yet migrated (P4.7/P4.8 migrate them; P7.1 deletes it).
2. Dispatch loop (maintenance, 2 s + triggers): select dependency-ready `queued` tasks
   regardless of VM state; `workspace.runtime.ensure(reason=agent_turn)` for stopped or
   failed workspaces; for running VMs with a free slot reserve the inbox input, record
   `attempt`, `vm_id`, `epoch`, `status=running`, `start_confirmed=false` in one update and
   call `modules.invoke(exec_id, "agent.turn" | "workspace.bootstrap", …)` with
   `expected_vm_id`; on a disposition set `start_confirmed=true` (apply
   `already_finished`; treat `cancelled` as terminal); re-send unconfirmed starts every
   iteration and on reconnect only while their incarnation is current; never re-send for
   a terminal task; `modules.cancel` retried until acknowledged.
3. Loss: when an incarnation is no longer current, settle every active execution bound
   to it, confirmed or not, as `vm_lost` with the retry policy.
4. Event ingestion: per-execution `seq`, unique `(task_id, attempt, vm_id, seq)`
   insert-ignore, `task_attempts.acked_event_seq` acknowledged to the dispatcher; the
   outcome persisted with the chat assistant message update in one transaction before
   the ack; on (re)connect send watermarks and outstanding intents, replay, reconcile
   (confirmed-running unknown → `vm_lost`; processes for terminal tasks → cancel).
5. Reconciler (30 s): incarnation not current → `vm_lost` (draining is live); questions
   past `expires_at` → `expired` (task `timeout` if still waiting); successor turns;
   wall clock; re-offer queued and unconfirmed. Replace the lease_reaper hooks added in
   P3.3 (`LEAN-TODO(P4.6)`).
6. Questions: `runner_wait_deadline_at` on creation (workspace setting, default the idle
   timeout); an internal `POST /api/internal/questions/:id/suspend` (with the `exec_id`)
   that records `suspended_at` through a conditional update succeeding only while the row
   is `open` and unanswered, and answers `already_answered` or `cancelled` otherwise; the
   long-poll returns when the row leaves `open` or its process is reaped; the reconciler
   applies the same update and `modules.cancel` for a question past the deadline whose
   supervisor did not suspend it (a dead module), with one `suspended` outcome; the
   `consumed` route is idempotent by `answer_version`; an answer to a question whose
   execution ended with an eligible outcome calls
   `workspace.runtime.ensure(reason=agent_question_resume)` and creates the successor.
7. Inbox: reservation and delivery states on `chat_messages` (queued → reserved on
   dispatch → delivered on the journaled `input.accepted` event, never on the `started`
   disposition; an execution that ends without `input.accepted` releases the reservation
   to `queued`; `delivery_unknown` on loss of the incarnation before an acknowledged
   `input.accepted`, released back to `queued` on reconciliation unless the replayed
   journal or the outcome shows acceptance); withdraw and edit of queued rows.
8. Tests: api_task_lifecycle.rs: every transition; new task and new chat turn with all
   VMs stopped resume and run; crash between record and invoke re-sends the same
   `exec_id` and runs once (fake facade); incarnation loss before delivery and after
   spawn with the reply lost both settle, with and without retry; cancel → disposition →
   ack and prune → delayed invoke returns `cancelled` (with the P4.2b dispatcher); answer
   commit → cancel → reconciliation creates no successor, including with a concurrent
   creation; interrupt-and-requeue on the same incarnation gives the new attempt its own
   sequence space and old-attempt acks do not move it; channel loss before spawn, after
   spawn and at exit each yield one run and one outcome; API restart mid-tool-call yields
   a complete ordered timeline including a late lower-seq event; a runner wait that ends
   suspends the turn and the answer wakes exactly one successor; stranded pending
   assistant message repaired; a message typed during a turn is queued and delivered next;
   after `started`, a failed session restore, provider spawn and stdin write each leave
   the message not `delivered` and back in `queued`; loss after `input.accepted` and
   before its acknowledgement neither loses nor duplicates the input; a fixture that
   ignores the suspension tool error and finishes normally is still suspended at the
   deadline with one outcome and an answerable question, and the answer wakes one
   successor; an answer racing the suspension and a cancel racing the suspension yield
   one outcome each.

Do not: delete sticky runtimes, worker pins or job branches (P4.7, P4.8); delete the old
kernel (P7.1).

Acceptance: listed tests pass; `TaskService::apply` is the only writer for tasks that
carry a `vm_id`.
Validation: cargo check, lint, fmt, `cargo test -p g5-api-core --lib`, `just test-integration`.
```

### P4.7 — Chat turns on the dispatcher, the inbox; delete sticky runtimes

```
You are implementing P4.7 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.2 (base workspaces), §8.1,
§8.2, §8.6, §8.8, docs/agent-api-surface.md §8, and docs/research/research-chat.md
(all, especially B, D, G). Depends on P4.6, P4.12 (real providers in QA reach the model through the host proxies). Owner: Agents.

Goal: a chat turn is a `chat_turn` task run by the `agent.turn` module; messages typed
during a turn queue; no sticky runtime, worker pin, route classification, forced fork or
resident process exists.

Do:
1. Chat send (routes/chat.rs, chat_service.rs): insert the user message as `queued` and,
   if no turn runs, a `chat_turn` task with `spec` (provider, model, permission mode,
   provider_session_id, the reservation); otherwise the row waits; the P4.6 loop
   dispatches. Streaming: module events → `chat.chunk`, `chat.message_updated`,
   `chat.done`, `chat.error` with today's names; timeline into `task_events`. Any member
   with write may send (`author_id`); base workspaces refuse chat with a "fork first"
   answer. Fork stays a row copy; workspace carry copies rows. Successor turns for
   interrupted and suspended turns carry the answer or the continue note.
2. Delete: `chat_sticky_runtimes` and every sticky column, the `sticky_runtime.rs`
   service, both runtime keepers, parked runtimes in the supervisor, `chat_worker_loss.rs`,
   `ChatWorkerRoute` classification, `required_worker_id`, `branch_mode`,
   `attach_preview_task_id`, `chat_sessions.status`, `session_type='task'` and
   continue-in-merged-workspace, chat context links (`chat_context_service.rs`, the two
   tables, 4 routes, `g5d chat context`, ChatContextLinks.tsx), `agent_approval_handoffs`
   (`LEAN-TODO(P4.7)`), heartbeat sticky riders in g5-internal-api DTOs, and the
   `authz.*sticky*` SQL functions and trigger, all through a migration.
3. Frontend: ChatPanel loses worker pins, sticky badges, context links, "continue in
   merged workspace", worker conflict logic (lib/chatWorkerConflict.ts,
   WorkspacePageChatWorkerPin); a queued strip with edit and withdraw; an author label per
   message; a persistent question card that renders with or without a VM; a notification
   on an open question.
4. Tests: api_chat.rs retargeted to the task-backed turn; e2e chat scenarios (chat_agent.rs,
   chat_wait_input.rs, timeline_streaming.rs) on the dispatcher with fake providers;
   Playwright chat.spec.ts and chat-timeline-recovery.spec.ts plus a queued-message flow;
   delete sticky and context-link tests; the durability test of design §8.7 (VM killed
   after acceptance and before harvest ack: neither lost nor duplicated).

Do not: touch non-chat job kinds (P4.8) or the host's per-task code (P4.9a).

Acceptance: two chat sessions run concurrently in one workspace VM with fake providers;
a message to a stopped workspace resumes it; a message during a turn is queued and
delivered next; stop mid-turn yields `interrupted` and the answer to an open question
starts one successor; `grep -rn "sticky\|required_worker_id\|context_link" backend
frontend/src` is empty.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P4.8 — Agent tasks and jobs on the dispatcher; delete job/task branches and job merge

```
You are implementing P4.8 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §7.1, §7.3, §7.4, §6.7
(`workspace.bootstrap`), and docs/research/research-task-lifecycle.md §A.3, §A.4, §E;
research-governance.md §C.3. Depends on P4.6. Owner: Agents.

Goal: every non-chat job kind (uplift, downlift, sync, explainer, agent_review, custom)
runs as tasks in the workspace VM on the workspace branch; there are no job or task
branches and no job merge.

Do:
1. Job kinds become profiles: a `jobs.spec` carrying the profile name and parameters;
   child tasks are `agent` (`agent.turn` with the profile's prompt, non-prompting mode)
   or `command` (`workspace.bootstrap run` for uplift's deterministic DAG, which submits
   child tasks through the API with its execution identity). Remove `job/<id>` and
   `task/<id>` branch creation (job_service.rs, task_service.rs, the P1.1 task-branch
   initializer), the `branch_name`/`base_commit` task columns' git meaning (keep
   informational `base_sha`), and make agent tasks push to the workspace branch natively
   (the target is now always the workspace branch); resolve every `LEAN-TODO(P4.8)` (55
   on main, including `workspace_branches` authority-scope columns and the consumer
   input/receipt tables keyed by advance id).
2. Delete `job_merge_service.rs`, `job_merge_runs`, `job_merge_artifacts` (migration),
   the `/jobs/:id/merge*` routes, `g5d job merge`, the `merge` job and task types, the
   job-merge Playwright and API tests, and the job-merge frontend (job-merge-workflow/*,
   useJobs merge hooks).
3. Job status derived from children; cancel cascades; no pause (delete pause/resume
   routes and UI); no rollback.
4. Map today's uplift/downlift/sync/explainer/approval_review flows onto the new tasks
   with minimal prompt changes; the ontology, docs and explainer projections keep working
   from pushed commits (they are outside the MVP acceptance but must not break).
5. Tests: e2e full_pipeline.rs, job_lifecycle.rs, task_dependencies.rs,
   sync_job_one_task.rs, explainer_creation.rs, bootstrap_ontology.rs on the dispatcher
   with fake providers; api_jobs.rs retargeted; Playwright jobs.spec.ts,
   workspace-generate.spec.ts, job-progress.spec.ts.

Do not: touch approvals or PRs (P6); delete the host per-task code (P4.9a).

Acceptance: an uplift job with two child tasks runs concurrently in one VM and both
commits land on the workspace branch; `grep -rn "job/\|task/\|job_merge\|LEAN-TODO(P4.8)" backend`
finds no branch naming or merge-run code.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P4.9a — Delete per-task execution on the host and in the worker

```
You are implementing P4.9a of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §3.2, §6.2 (removed list), and
docs/research/research-exec-isolation.md §C, §D, §F. Depends on P4.7, P4.8.
Owner: Worker & Host.

Goal: the host and the worker crates contain only the workspace-VM path.

Do:
1. Host: delete from backend/g5d-host the per-task assignment loop (poll/claim/ack/nack/
   observation), `Materializer` per task, `WorkspaceMaterializer`, rootfs-per-task COW
   (keep template extraction and one COW per workspace VM), the handoff file, deferred
   and superseded bookkeeping, capacity backoff, saturated probe, the `preview_vms`
   registry (P5.1 replaced it), golden_builder.rs, golden_consumer.rs,
   golden_maintenance.rs, the discard fence, per-task attestation, the in-guest LLM proxy
   start path (P4.12 moved it to the host); the transition sharing of
   `G5D_HOST_MAX_MICROVMS` becomes plain slots. backend/g5d-foundation: delete golden/,
   workspace/materialize.rs; keep repo_cache.rs only if P4.3a's restore uses it.
2. Worker: delete backend/g5d-worker/src/{poll.rs, inbox.rs, task_agent.rs per-task
   lifecycle parts, task_env.rs per-task credential injection, build_telemetry.rs,
   view_task_builder.rs task-branch parts}, `g5d worker`, `g5d task poll|claim|retry`,
   `g5d golden-build`, `g5d preview-attach-listener`; fold the surviving runner into the
   module crate of P4.11 if it still lives in g5d-worker.
3. Infra: remove golden knobs and per-task env from the host's startup_script.rs and the
   self-hosted host script's per-task paths (leave the bundle runnable with
   `LEAN-TODO(P9.1)` where the host container command changes).
4. Tests: retarget or delete the host and worker tests that drove the per-task path;
   classify every removed test.

Do not: touch the API, the frontend, Helm or compose (P4.9b); approvals/PRs; the
Terraform MIG (P8.4).

Acceptance: `grep -rn "materializ\|golden\|poll_for_task\|claim_task" backend/g5d-host
backend/g5d-worker backend/g5d-foundation` is empty; docker-ci passes with the worker
lanes replaced by dispatcher lanes.
Validation: cargo check, lint, fmt, `cargo test -p g5d-host -p g5d-worker -p g5d`, `just docker-ci`.
```

### P4.9b — Delete the workers table, per-task routes, loops and UI

```
You are implementing P4.9b of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §3.2, §7.5, §12, and
docs/research/research-task-lifecycle.md §A.3, §A.4. Depends on P4.9a.
Owner: Worker & Host.

Goal: nothing in the API, the frontend or the deployment files serves per-task
workers any more.

Do:
1. API: a migration dropping `workers`, `worker_tokens`, `worker_session_generation`,
   `host_discard_fence_*`, `task_execution_attempts`, `builder_task_specs`,
   `session_build_telemetry`, `workspace_prebuild_state`, `runtime_attribution_events`,
   `cloud_orphan_sightings` if unused, and the per-task host columns; delete the
   `/api/v1/orgs/:id/workers*` and `/workspaces/:id/workers*` routes, the worker
   registration, heartbeat, poll, claim and ack internal routes, `WorkerTokenScope`,
   and the shared DTOs they used; maintenance loops `golden_builder_triggers`,
   `lease_reaper` (P4.6's reconciler replaced it), `build_telemetry_aggregation`,
   `attribution_retention`.
2. Frontend: delete WorkersDropdown, ConnectedWorkerPill, WorkspaceCapacityIndicator,
   capacity/previewLinks, worker pinning, `useWorkers.ts`, settings "concurrent tasks /
   cloud workers" (keep idle timeout as the VM idle setting).
3. Infra: remove the worker Deployment/HPA and per-task env from Helm and compose; keep
   `G5D_HOST_MAX_MICROVMS`.
4. Tests: retarget or delete the API and frontend tests that drove per-task workers;
   classify every removed test.

Do not: touch approvals/PRs or the Terraform MIG (P8.4).

Acceptance: `grep -rn "worker_session_generation\|claim_task\|poll_for_task" backend
frontend/src` is empty; docker-ci passes.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, frontend
build and tests, `just docker-ci`.
```

### P4.10 — Frontend: workspace VM status, live reads, Changes, Commit/Push, Restart

```
You are implementing P4.10 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §5.7, §5.8, §6.6 (concurrency),
§9.1, §9.2, and docs/research/research-frontend-preview-infra.md §1.1, §1.3. Depends on
P4.5. Owner: Workspace.

Goal: the workspace shell shows and controls the VM, reads live files, and exposes
Changes, Commit, Push, Update from origin, Restart, Flush now and the association check.

Do:
1. Status strip: VM state (stopped/starting/running/stopping/failed, `queued_for_capacity`
   with position) with Resume, Stop and Restart; "resuming workspace" when a Save wakes
   it; `WORKSPACE_STOPPING` handling; the last flush time and a "Flush now" action; the
   loss notice when `unflushed_loss_at` is set; the teardown nudge banner with its deadline.
2. Reads: use the `source` marker (live/flush/committed) in the file tree, editors, docs,
   ontology; refetch on `file.changed`; base workspaces render read-only with "Fork and…"
   on chat, preview and terminal.
3. Changes panel from `GET /workspaces/:id/changes` (the `workspace.changes get`
   projection), diff from the read routes, Commit (message dialog, optional push) and
   Push buttons, "Update from origin" with ahead/behind display; association mismatch
   banner with "Switch back"; the receipt's dirty set while stopped.
4. Multi-execution notice when more than one execution is running.
5. Remove `useFileTree` polling and draft-era commit UI leftovers.
6. Playwright: workspace-vm.spec.ts (resume/stop/resume with restored edits using a fake
   provider that writes a file; restart), changes-and-commit.spec.ts,
   association-mismatch.spec.ts (a terminal `git checkout` then Save; needs P5.2 or a
   fake-provider checkout), flush-now.spec.ts.

Do not: touch preview/terminal panels (P5), PR/approvals (P6), the chat panel (P4.7).

Acceptance: build and tests pass; Playwright flows pass against the docker-ci stack with
a workspace VM.
Validation: frontend build and tests, Playwright lane of docker-ci.
```

### P4.11 — The `agent.turn` module and the per-turn CLI adapters

```
You are implementing P4.11 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.7, §8.2 to §8.5, §8.7,
docs/agent-api-surface.md §2, §3, §5, §9 (schemas), §12, and
docs/lean-provider-cli-contract.md. Depends on P4.2b. Owner: Agents.

Goal: the P3.2 runner becomes the entrypoint of the `agent.turn` module: one CLI process
per turn for both providers, launched by the dispatcher's module runtime with the module
input, emitting the provider-neutral events and exactly one outcome, restoring and
harvesting session files inside the module, and pointing every provider process at the
host proxies with no credential in its environment.

Do:
1. New crate backend/g5d-agent-turn (or a module of g5d-dispatch's catalog) with the
   module manifest of design §8.3 (`bounded_run`, operation `run`, requested
   capabilities), input `AgentTurnInputV1`, events `AgentTurnEventV1` and outcome
   `AgentTurnOutcomeV1` as Rust types generated from one schema source shared with the
   API (docs/agent-api-surface.md §5, §9); the input carries a `prompt_ref` resolved
   through the proxied API, never inline credentials, argv or host paths.
2. Move backend/g5d-worker/src/runner/{mod,claude,codex,events,process,session_files}.rs
   under the module; keep the argv builders exactly per design §8.3 and the contract;
   set `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` to `http://g5-llm:<port>/s/<session_id>`
   (one-shot tasks use the task id), `G5D_API_URL` to the proxy's `/api` path,
   `G5D_EXEC_ID` and `G5D_SESSION_ID` as plain values, placeholder provider keys, no
   workspace token; the MCP server (`g5d mcp`) and `g5d` commands use the same proxied URL.
   Write the frozen prompt to stdin, close it, and journal
   `input.accepted {reservation_id, input_id, revision}` at that boundary; a failure in
   session restore, spawn or the stdin write ends the execution with a failed outcome and
   no `input.accepted`, so Agents releases the reservation.
3. Event mapping: every Claude `stream-json` frame type and every Codex `--json` item
   in the contract classified as C2 or a C3 candidate; the outcome settled only from the
   result frame, EOF and exit status together (Codex: `turn.completed` plus EOF and exit
   zero); unknown discriminants and contradictory evidence fail the turn with a named code;
   the interim silence timers kept as safety nets under `limits.wall_clock_ms` only.
4. Questions and permissions: `ask_user` and `request_permission` long-poll through the
   proxied API; the module supervisor (the Rust side) tracks `question.opened` without
   `question.consumed` and at `runner_wait_deadline_at` calls the suspend route (a
   conditional update that succeeds only while the row is open and unanswered),
   interrupts the provider with the cancel signal sequence, harvests, and emits exactly
   one `suspended` outcome with the blocker; a tool error is never what ends a turn; MCP
   and Codex tool timeouts set above the runner wait.
5. Session files (P3.4) restored before a resumed turn and harvested after every turn
   with `vm_id` and `epoch`; the history fallback from delivered rows.
6. Tests: module tests with the fake binaries (both providers: new session, resume,
   restore fallback, question answered during the wait, question suspended, permission
   allow and deny, cancel, nonzero exit, frame mapping exhaustiveness against the recorded
   fixtures); a receipt test proving no credential variable is set in the provider
   process (`G5_FAKE_RECEIPT`); a scenario in which the provider ignores the suspension
   tool error and finishes normally is still suspended, reaped, and its question left
   answerable; a spawn failure produces no `input.accepted`; the e2e `fake_provider_chat_turn` scenarios retargeted
   to `modules.invoke` through P4.2b's dispatcher.

Do not: touch the API's task lifecycle (P4.6); the host proxies (P4.12); delete the
worker's per-task adapters (P4.9a does; here they may delegate to the module code).

Acceptance: a fake-claude and a fake-codex turn run as `agent.turn` invocations through
`g5d dispatch` with events journaled and one outcome; docker-ci passes.
Validation: cargo check, lint, fmt, `cargo test -p g5d-agent-turn -p g5d-dispatch`,
`just docker-ci`.
```

### P4.12 — Host proxies: model traffic per session, git and platform credential proxy, grant updates

```
You are implementing P4.12 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.5, §5.3 (identities), §11.1,
docs/worker-host-api.md §6 (all) and §13. Depends on P4.4b. Owner: Worker & Host.

Goal: one host-side listener per VM (`g5-llm`) that terminates the provider CLIs' HTTP,
attaches the provider credential per session, proxies git over HTTPS and the platform
API with injected credentials, refuses non-current incarnations, meters usage per
session, and swaps grants live. No credential enters the VM.

Do:
1. Listener: a per-VM port on host loopback, reached from the guest through the passt door, seeded as `g5-llm` in the guest's `/etc/hosts` at boot; guarded by the per-VM proxy secret minted at boot and passed to the dispatcher only; closes with the VM. The `vm.grants.update` command handler (types from P4.4a): full replacement, atomic swap, `applied | stale | grant_unresolved`.
2. Model traffic: strip `/s/<session_id>`, select the `llm_provider` grant for the
   session (session-scoped grant first, workspace-wide grant otherwise), attach the
   credential, re-originate over HTTPS to the provider or gateway; refusals
   `403 stale_incarnation`, `400 missing_session`, `403 no_grant`, `413 body_too_large`,
   `403 wrong_vm`; usage per session recorded to `usage_records` through the control
   plane; `GET /_g5/status` inside the VM and the `sessions` view on `vm.status`.
3. Git: `/git/<host>/…` paths re-originated to the real host with the `git_https` credential; the internal git service's grant from the workspace token (`source: workspace_token`); the gitconfig rewrite rules for the processes the dispatcher launches get the port. The dispatcher's own flush push may keep the direct token path of P4.2a; both keep the token out of provider processes.
4. Platform API: `/api/…` re-originated to `G5D_API_URL` with the workspace token
   injected (`g5_api` grant); the fence and the execution identity in the request stay
   the API's checks.
5. Grants: the values P4.4b resolved at boot, replaced on `vm.grants.update` (in-flight requests finish with the old credential); re-resolve once on an upstream 401. The control plane pushes an
   update when a session of another user starts in the workspace (a session-scoped
   `llm_provider` grant from that user's provider account) and on rotations.
6. Remove the in-guest LLM proxy start path and the credential hand-in
   (`ANTHROPIC_UNIX_SOCKET`, `LEAN-TODO(P8.6)` in agent_runner.rs) for workspace VMs.
7. Tests: proxy unit tests for every refusal; a fake upstream proving credential
   injection and re-origination; no credential in the guest environment (the P3.2 carrier
   allowlist and a receipt test); a grant update taking effect on the next request; two
   sessions of two users using two grants; the git push from a workspace VM to the
   internal service succeeding with no token in the VM; docker-ci isolation lane.

Do not: implement `gh` and the per-VM CA (later, §17.2); touch the API's grant
persistence beyond the resolve and update routes.

Acceptance: a fake-provider turn in a workspace VM reaches the fake upstream through the
proxy with the credential attached on the host side; a stale incarnation is refused.
Validation: cargo check, lint, fmt, `cargo test -p g5d-host`, `just docker-ci`.
```

### P4.13 — Host registry, host states, placement, quotas, drain and the operator API

```
You are implementing P4.13 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.1 (host states), §6.2
(operator API), §6.10, docs/worker-host-api.md §2, §7, §8. Depends on P4.1, P4.4a. Owner: Worker & Host.

Goal: the control-plane side of the fleet: the host record of design §12, host states
including `unreachable`, occupancy derived from `workspace_vms`, placement, org quotas,
drain orchestration and the operator routes.

Do:
1. `WorkerHostService`: registration (GCE bring-up kept; self-hosted and macOS by
   registration token), the state machine `provisioning → ready → draining → terminated`
   with `unreachable` after three missed heartbeats and `terminated` after credential
   expiry; heartbeat ingestion (occupancy, boots in progress, load, memory and disk free,
   retained roots); placement thresholds on disk and memory.
2. Placement function `place(workspace, org)`: a host in the org's region with a free
   slot holding the workspace's retained root, else any host in the region with a free
   slot, else `no_capacity`; `max_running_vms` quota enforced before `vm.start`;
   `slot_unavailable` and `workspace_busy` re-placement rules. Occupancy is
   `count(workspace_vms where host_id = ? and status in (starting, running, stopping))`.
3. Drain: `POST …/:host/drain` records `drain_requested_at`, sends `host.drain` through P4.4a's client so the host admits no new VM, moves the host to `draining`, and returns a drain id. The per-VM stop sequences with reason `drain`, their retries until the deadline, `host.drained` handling and rotation are P8.7, which needs the stop sequence of P4.5.
4. Operator routes: `POST /orgs/:org/worker-hosts`, `GET /orgs/:org/worker-hosts` (with
   derived occupancy and heartbeat summary), `POST …/:host/drain`, `DELETE …/:host`
   (drain then terminate or forget; `force=true` records the loss per VM); retire
   `POST …/:host/runtime-labels/refresh` and the per-host image columns.
5. Tests: api_worker_hosts.rs: state transitions including `unreachable` and reconnect;
   placement preferring the retained root, then the region, then `no_capacity`; quota;
   occupancy derived; the drain route sends `host.drain` and marks the host draining; operator routes.

Do not: implement the stop sequence itself (P4.5), the drain orchestration (P8.7) or the host daemon (P4.4a, P4.4b).

Acceptance: listed tests pass; `worker_hosts` matches design §12.
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P5.1 — Preview on the workspace VM (`workspace.preview`)

```
You are implementing P5.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.7, §6.8, and
docs/research/research-frontend-preview-infra.md §2. Depends on P4.5. Owner: Workspace.

Goal: the preview is a desired-state service module in the workspace VM, one per
workspace, reached through the existing relay keyed by workspace and port.

Do:
1. Module `workspace.preview` (`start(profile)`, `stop`, `status`): runs the profile
   command (`.g5/preview_profiles/<name>.yaml` or `.g5/workspace.yaml preview:`) as a
   supervised service with the module limits, records the port, replaces a running
   preview on a second `start`; `ports.list` and `port.opened/closed` events.
2. API: `/api/v1/workspaces/:id/preview/:port/*` relay: keep the HTTP and WebSocket relay
   code of routes/preview.rs, replace `resolve_preview_target` with (workspace → current
   host and `vm_id` → `http` stream through the facade); workspace-scoped ticket cookie and
   mint route; subdomain mode keyed by workspace id; `POST /workspaces/:id/preview/start|stop`,
   `GET .../ports`; preview traffic counts as activity.
3. Delete the `preview` job type, `job_preview_sessions` (migration), boot generations,
   `preview_reaper`, the chat-attach listener, token and host drive (`g5d preview-serve`,
   `preview_attach_wire.rs`, agent_drive preview parts, `LEAN-TODO(P5.1)` in claude_code.rs),
   `attach_supported`, and their tests (api_preview_attach.rs, preview_chat_attach.rs).
4. Frontend: PreviewControl starts and stops by profile without commit/TTL plumbing;
   PreviewBrowserView unchanged apart from the URL; a Ports panel listing open ports with
   "open"; base workspaces offer "Fork and preview".
5. Tests: module preview and ports tests; api_preview.rs (relay reaches a port opened by
   a fake provider process, a second start replaces the first, ticket scope is one
   workspace, stop kills the process); Playwright preview.spec.ts.

Do not: touch terminal (P5.2).

Acceptance: a dev server started by a fake provider turn is reachable through the relay
in docker-ci; `grep -rn "job_preview_sessions\|preview_attach" backend` is empty.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P5.2 — Browser terminal

```
You are implementing P5.2 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.9, §6.6 (terminal row), and
docs/research/research-exec-isolation.md §H. Depends on P4.2a, P4.4b. Owner: Workspace.

Goal: a browser terminal into the workspace VM, the user's escape hatch.

Do:
1. Dispatcher: `terminal.open(cols, rows)`, `terminal.write`, `terminal.resize`,
   `terminal.close` over a dedicated `pty` stream using backend/g5d-foundation/src/terminal
   (TerminalManager); shell as uid 1000 in `/work` with the module environment (proxied
   egress, task secrets), bounded by `max_pty_streams_per_vm`.
2. API: `WS /api/v1/workspaces/:id/terminal` (workspace write access, wakes the VM through
   the facade) bridging the browser WebSocket to the `pty` stream through the host tunnel;
   terminal activity counts as VM activity; open terminals are reported to the flush as
   concurrent writers and closed by the stop sequence.
3. Frontend: xterm.js panel in the bottom drawer; resize handling; survives editor
   navigation within the page; base workspaces offer "Fork and open a terminal".
4. Tests: dispatcher pty test; API bridge test with a fake dispatcher; Playwright
   terminal.spec.ts (echo a command, resize).

Do not: record terminal sessions or add auditing.

Acceptance: a command typed in the browser runs in the VM and its output appears;
docker-ci passes.
Validation: cargo check, lint, fmt, frontend build and tests, `just docker-ci`.
```

### P6.1 — `pull_requests`, `approval_rules`, `approval_decisions`, `g5d pr`

```
You are implementing P6.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §10.1 to §10.3, §11.1, and
docs/research/research-governance.md §B, §D. Depends on P1.5, P4.1. Owner: Git and governance.

Goal: the minimal governance data model and routes; agents open PRs and record reviews
through `g5d pr`; provider PRs are mirrored at publish; a PR is created from what is on
the branch.

Do:
1. Migration: `pull_requests` (design §10.2 shape including the merge-operation columns),
   `approval_rules`, `approval_decisions` (design §10.3); drop the old
   `workspace_pull_requests`, `_events`, `_provider_repos` tables (their code is deleted
   in P6.4; here replace the routes' data source).
2. Routes: `POST /workspaces/:id/pull-requests` (reads the source OID from git, refuses
   with the `workspace.changes get` projection when the tree is dirty or ahead so the UI
   can offer "Commit and push all", records `published_sha`, publishes via P1.5's
   provider push, opens a provider PR when the repo has a provider), `GET`,
   `POST .../close`, `POST .../decisions` (records the `source_sha` the approver saw;
   the only writer of decisions), org routes for `approval_rules` CRUD (org admin),
   provider branch-protection read at repo connect with a warning flag.
3. `head_sha` as a projection of the source branch under P1.3's convergence (`pr_head`).
4. CLI: `g5d pr create|status|review --approve|--request-changes --sha` through the
   proxied API; agent review tasks are launched with the OID to review (job kind
   `agent_review` from P4.8) and pass it to `g5d pr review`; resolve `LEAN-TODO(P6.1)` in
   the profiles (`/pr` uses `g5d pr create`).
5. Tests: api_pull_requests.rs (create publishes and mirrors; a dirty tree is refused
   with the projection; decisions bind to the reviewed sha; a new push makes them stale),
   api_approval_rules.rs, CLI tests for `g5d pr`.

Do not: implement merge (P6.2); touch the frontend (P6.3); delete the old approval
services (P6.4).

Acceptance: tests pass; an agent can open a PR from a fake provider turn via
`g5d pr create` after pushing.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`.
```

### P6.2 — The merge operation

```
You are implementing P6.2 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §10.6 (all), §5.3 (merge receipt
ref row), §5.6 (per-repo lock), and the draft 6 review G5
(docs/lean-backend-design-v6-review.md). Depends on P6.1. Owner: Git and governance.

Goal: `POST /pull-requests/:id/merge` as the only writer of protected refs, implemented
as the persisted, resumable operation of design §10.6 with a receipt ref for local
merges.

Do:
1. Implement the six steps exactly: pin (or resume an unresolved operation by
   `merge_op_id`/`merge_state`), authorize against decisions at the pinned source OID,
   mergeability on the bare mirror, the provider path (republish if `published_sha`
   differs; provider merge API with the expected SHA on GitHub and GitLab; durable
   `provider_merged`; import under the per-repo lock shared with inbound mirroring; retry
   import without re-pinning; reconcile ambiguous replies by reading the provider PR and
   attribute only if the merged head equals the pinned source), the local-only path
   (write the merge commit, persist `merge_result_sha`, then one `git update-ref --stdin`
   transaction that compare-and-swaps the target and creates
   `refs/g5/merges/<merge_op_id>`; recovery reads the receipt ref), failure only before
   application; receipt refs pruned when the PR closes or 30 days after `applied`.
2. Receive-pack: the API server identity is the only writer of protected refs and of
   `refs/g5/merges/*`; no network path.
3. A retry loop for unresolved operations (maintenance, backoff).
4. Tests: api_merge_operation.rs: refused without decisions at head; an internal push
   after publication makes the merge republish and pass the expected SHA; a source push
   racing the merge is refused by the provider precondition (fake provider server);
   crash before and after the provider call, lost success reply, inbound target update
   during merge, provider success then import failure, local ref transaction success
   then crash before the metadata commit, and a local merge applied then the target
   fast-forwarded by its owner before recovery (recovery reports `applied` from the
   receipt); a rewritten target with no receipt is `failed`; a remotely merged PR at a
   different head is not attributed.

Do not: touch the frontend (P6.3).

Acceptance: listed tests pass against a fake GitHub/GitLab HTTP server.
Validation: cargo check, lint, fmt, `just test-integration`.
```

### P6.3 — Governance frontend

```
You are implementing P6.3 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §10, and
docs/research/research-frontend-preview-infra.md §1.3 rows 13 to 15. Depends on P6.1.
Owner: Git and governance.

Goal: one PR panel and one approvals surface on the new tables, with explicit
instructions on what an approval means.

Do:
1. PullRequestPanel/List/CreatePullRequestDialog on `pull_requests`; the create dialog
   shows the `workspace.changes` projection and offers "Commit and push all" when the
   tree is dirty or ahead; show the pinned source SHA, decisions at that SHA, provider PR
   link, merge state and error; a Merge button calling the P6.2 route (visible when the
   rule is satisfied, otherwise explains what is missing).
2. Approvals: replace WorkspaceApprovalsPageContent and OrgApprovalsPageContent with an
   org-level `approval_rules` editor and a per-PR decision UI (approve / request changes
   with note) that states what approving means; the review flow follows the UX team's
   definition.
3. Delete PrepareToShipModal, ReviewChangesModal, RebaseChangeGraphPreview, the
   change-graph views (SourceControlChangeGraphView*), workspace merge modals,
   `useWorkspaceApprovals`, `useOrgApprovals`, approvalStages, and the `stages` status
   band panels.
4. Playwright: pull-requests.spec.ts, approvals.spec.ts; delete org-approvals.spec.ts, the
   old workspace-pull-requests.spec.ts, source-control-change-graph.spec.ts.

Do not: touch backend code beyond DTO alignment.

Acceptance: build and tests pass; Playwright flows pass in docker-ci.
Validation: frontend build and tests, Playwright lane.
```

### P6.4 — Delete governance legacy

```
You are implementing P6.4 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §10.4, §10.5, and
docs/research/research-governance.md §G. Depends on P6.2, P6.3. Owner: Git and governance.

Goal: nothing remains of the approval ledger, coordinated publication, PR provider
lifecycle, mutation policy or fine-grained scopes.

Do:
1. Delete services approval_config_service.rs, approval_definition_service.rs,
   approval_review_service.rs, coordinated_repo_publication.rs,
   workspace_pull_request_provider_lifecycle.rs (after moving any helper P6.2 still uses),
   workspace_mutation_policy.rs and every `ensure_workspace_mutable` call, routes
   org_approvals.rs, workspace_approvals.rs, the merge/rebase routes left in
   workspaces.rs, `source_control_change_graph_service.rs` and its route; every
   `LEAN-TODO(P6.4)` site.
2. Migration: drop approval_definitions, approval_configs, approval_requests,
   approval_statuses, approval_review_runs, merge_evaluations, `jobs.approval_context`,
   `workspaces.{mutation_policy*, stages, merge_target_workspace_id}`, `user_scope_grants`,
   `workspace_permission_overrides`, `org_default_scopes`, `oauth_scopes`, the validator
   functions and governance triggers; collapse permission_service.rs to the four org
   roles plus workspace visibility and members (delete routes/permissions.rs except
   `/auth/me/scopes`).
3. Delete tests: api_agent_approval_*.rs, api_workspace_approvals.rs, api_org_approvals.rs,
   approval_*_static.rs, approval_request_retention.rs, route_auth_static.rs entries for
   removed routes, e2e approval_review_live.rs, merge_source_publish.rs leftovers.
4. Maintenance: delete `approval_request_retention`.

Do not: touch login providers.

Acceptance: `grep -rn "approval_definition\|approval_request\|mutation_policy\|scope_grant\|LEAN-TODO(P6.4)" backend frontend/src`
is empty; docker-ci passes.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, `just docker-ci`.
```

### P7.1 — Lifecycle cleanup: one status enum, delete the kernel

```
You are implementing P7.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §7.1, §7.2, §12, and
docs/research/research-task-lifecycle.md §B, §H. Depends on P4.9b, P6.4. Owner: Agents.

Goal: one task status vocabulary (`queued|running|waiting|succeeded|failed|cancelled`)
and one outcome vocabulary in the database, Rust and the frontend; the old execution
kernel is gone.

Do:
1. Delete backend/g5-api-core/src/services/execution_lifecycle/*, the
   `LifecycleCommand`/`JobRollupKind` families, lock plans, `apply_lifecycle` call sites
   (all now go through P4.6's `TaskService::apply`), task status values
   `pending|claimed|paused|blocked|rolled_back|recovering|recovery_blocked` and `merged`,
   `task_question_windows`, `human_escalations`, `human_escalation_questions`,
   `task_completion_constraints`, `task_checkpoints`, review-checkpoint columns on jobs,
   `pruned`, `is_checkpoint`, `max_retries` semantics, the `scheduler` remnants, `TaskType`
   enum drift, through a migration.
2. Frontend: `TaskDTO.status` typed to the six values and the outcome to the six
   outcomes; job status derived; remove pause/resume/rollback controls if any remain.
3. Tests: api_tasks.rs and api_jobs.rs retargeted to the six states; delete
   task_lifecycle_contract parts for removed states; e2e task_lifecycle_contract.rs updated.

Do not: rewrite schema beyond this PR's drops (P7.2).

Acceptance: `grep -rn "recovery_blocked\|rolled_back\|LifecycleCommand" backend frontend/src`
is empty; the three status vocabularies are identical.
Validation: cargo check, lint, fmt, migration checks, `just test-integration`, frontend tests.
```

### P7.2 — Schema cleanup migrations and after-metrics

```
You are implementing P7.2 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 (item 6), design v7 §12, and
docs/database-migrations.md. Depends on P7.1. Owner: Workspace.

Goal: the live schema contains the design's table set (about 36 tables), reached by
forward migrations, clean of every table, column, function, trigger and policy whose
last consumer is gone; the baseline file is untouched.

Do:
1. Inventory the live schema against design §12 and the tables the code still queries
   (grep `sqlx::query` and the models); write one migration (or a few, in order) that
   drops what nothing uses, tightens what earlier PRs left loose (`task_events.attempt/
   vm_id/seq` NOT NULL, `tasks.vm_id` semantics, drop `head_commit` columns replaced by
   `head_sha_cache`, provider enums, `chat_messages.delivery` NOT NULL for user rows), and
   moves org/user/workspace configuration into `configs` if the code was migrated
   (otherwise keep the three and note it).
2. Validate fresh install and upgrade on a copy of the QA database's rows; regenerate
   the SQLx offline cache; `just test-integration` and `just docker-ci`.
3. Record the "after" metrics in your status row's Notes and in the PR description, with the same commands P0.1 used (PR #1 holds the before-metrics) and the live table count.

Do not: change behavior; edit any applied migration.

Acceptance: table count matches the design within a few tables and every difference is
explained in the PR; upgrade from the previous schema passes; docker-ci passes.
Validation: migration checks, `just db-prepare`, cargo check, `just test-integration`, `just docker-ci`.
```

### P8.1 — Remove internal issue tracking

```
You are implementing P8.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §3.2 (issue tracking row),
§11.3, and docs/deployment-gate-audit.md §1 (issue tracking row). Depends on P3.2
(merged). Owner: Agents.

Goal: the issue tracker module is gone; platform bug reporting, comments and
notifications stay.

Do: delete backend/g5-api-routes/src/routes/issues.rs, the internal issue endpoints in
routes/internal.rs, backend/g5-api-core/src/services/{issue_service, issue_attachment_store}.rs,
maintenance `issue_inference` and `issue_attachment_scanner`,
backend/g5d-runtime-core/src/issue_tracking.rs, backend/g5d-tooling/src/tools/{issue_ops,
issue_context, search_issues, add_issue_comment, update_issue}.rs and their MCP
registration, `g5d issue`, frontend IssuesPage and components/issues/*, hooks/useIssues.ts,
the issue tab/URL/WS wiring, `issue.*` WS events, `G5_ISSUE_TRACKING_ENABLED`, and a
migration dropping the thirteen issue and hotlist tables; the `g5-report-issue` skill.
Keep `platform_issue_outbox` and the platform bug report button.

Do not: touch anything else.

Acceptance: `grep -rni "issue_tracking\|hotlist\|IssuesPage\|file_issue" backend frontend/src assets`
is empty except platform-issue code; docker-ci passes.
Validation: cargo check, lint, fmt, migration checks, frontend build and tests, `just docker-ci`.
```

### P8.4 — Infra cleanup

```
You are implementing P8.4 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.10, §16.1, and
docs/research/research-frontend-preview-infra.md §3, §5.3. Depends on P4.9a, P4.9b. Owner: Worker & Host.

Goal: infrastructure matches the workspace-VM fleet and the deployment targets.

Do: remove the Terraform `worker_host` and `tungsten_worker_host` MIG modules and daily
scheduler (target size is zero everywhere), the customer image Dockerfiles and their
Cloud Build steps, the disabled GitHub workflows (ci.yml, e2e.yml, release.yml,
base-image.yml), Helm worker Deployment/HPA templates and the Istio template, per-task
env from startup_script.rs and compose; the `pr1884-authority-cutover` deploy step and
its scripts and contract tests (`LEAN-TODO(P8.6)` in cloudbuild-deploy.yaml, done here
because it blocks staging promotes); rebuild or retire the pre-Lean Local Setup
binaries under infra/local-setup that still embed sidecar-era strings; keep the
self-hosted bundle and the Azure canary lane runnable (P9.x rewrites them); update
runbooks and README. The renewable host credential landed in P4.4a.

Do not: change CI lanes beyond the named step (P8.6).

Acceptance: `terraform validate` on each environment, `helm template` on each overlay,
the self-hosted bundle contract tests and the Azure canary's local contract tests pass.
Validation: `just helm-lint`, `just infra-plan env=dev` (plan only), `python3 scripts/run-ci-script-tests.py`.
```

### P8.5 — Crate consolidation

```
You are implementing P8.5 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §4.1, and
docs/relocation-validation.md. Depends on P7.2. Owner: Platform.

Goal: fewer crates with clear ownership by capability, no behavior change.

Do: propose the target layout in the PR description first (for example: `g5-api` with
one module per bounded context: workspace, agents, hosts, git; `g5d` = cli + dispatch +
agent-turn + host + isolation + tooling + foundation; shared: `g5-scm`, `g5-storage`,
`g5-auth`, `g5-authz`, `g5-semantic`, `g5-internal-api`), then move modules, fix paths,
keep public APIs used by tests, update Cargo.toml, Dockerfiles, Justfile,
scripts/check-layout.py and docs/architecture.md.

Do not: change behavior or delete code.

Acceptance: workspace builds; all tests pass; `just commands-check` and
`python3 scripts/check-layout.py` pass; docker-ci passes.
Validation: full ladder and `just docker-ci`.
```

### P8.6 — CI review

```
You are implementing P8.6 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2 and design v7 §16.1. Depends on P8.5.
Owner: Platform.

Goal: Cloud Build reflects the lean repository; nothing else changes.

Do: prune infra/ci/cloudbuild-*.yaml of retired lanes (worker per-task lanes, golden
canaries, customer images, CRDT lanes), name the dispatcher and workspace-VM lanes, add
`fake-provider-cli` and the new crates to every explicit crate list (the CI3 follow-up),
make sure `just docker-ci` phases map to the new lanes, resolve the remaining
`LEAN-TODO(P8.6)` markers (live-microVM e2e lanes rewritten on native git or pruned;
the in-guest proxy socket path), update docs/just-commands.md, and run the CI contract
tests (scripts/test_ci_docker_contract.py, scripts/test_cloudbuild_waitfor_order.py).

Do not: change application code.

Acceptance: contract tests pass; a full `just docker-ci` passes locally; both PR lanes
run green on a docs-only PR.
Validation: `python3 scripts/run-ci-script-tests.py`, `just docker-ci`.
```

### P8.7 — Versioned rollout: protocol gates, feature flags, drain-and-recreate runbook

```
You are implementing P8.7 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §6.2 (versioning), §6.10, §5.8
(teardown reasons), and docs/worker-host-api.md §3.0. Depends on P4.5, P4.13.
Owner: Worker & Host, with the CI/CD owner.

Goal: a rollout can be executed and rehearsed: the control plane deploys first and
accepts the current and previous host protocol version, hosts are replaced by drain,
workspaces are moved to a new image by a rollout drain with a nudge, and feature flags
gate behavior that must switch independently of a deploy.

Do:
1. A compatibility test in the Rust lane that decodes the previous protocol version's
   messages and specs against the current control plane (fixtures recorded per version).
2. `configs` feature flags read by the lifecycle service, the dispatcher (through
   `VmSpec.config`) and the adapters, with a documented list.
3. Drain and rollout orchestration in the operator API: the drain that P4.13 records now runs the stop sequence with reason `drain` for every VM on the host, retries failed flushes until the deadline, records the loss on force, sends `vm.stop`, and terminates on `host.drained`; rotation = provision the replacement, drain, terminate; `POST /orgs/:org/rollouts` naming the new image or dispatcher version: for each running workspace, the nudge, the deadline, the stop sequence with reason `rollout`, and an immediate resume when the workspace had running work; progress on `drains` and `rollouts` rows (migration).
4. A runbook `docs/rollout.md`: order of operations, the QA rehearsal (control-plane
   deploy with an old host connected; a host drain; a workspace rollout with a running
   fake-provider turn continuing after the resume), and rollback.
5. Tests: the compatibility test; api_rollouts.rs (nudge delivered, deadline interrupt,
   flush, resume with the new image, a successor turn with the continue note).

Do not: change the host protocol itself.

Acceptance: the QA rehearsal in the runbook passes end to end once.
Validation: cargo check, lint, fmt, `just test-integration`, the QA rehearsal.
```

### P8.8 — Pre-launch audits

```
You are implementing P8.8 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §4.3, §6.4, §6.5, §7.5, §12.
Depends on the MVP milestone: P2.4, P4.3b, P4.9b, P4.10, P5.1 and P5.2 merged. Owner: Platform, with every capability owner.

Goal: written audits of the APIs and the data model before launch: security (the
fence, the proxies, the module capability grants, RLS, the token surfaces), fault
tolerance (every crash point in the stop sequence, the flush, the dispatch contract and
the merge operation, each with its recovery and its test), and scalability (the single
API replica, the tunnel registry, the event bus, the git store). The table list is
written down after the APIs.

Do: produce `docs/audits/{security,fault-tolerance,scalability}.md` with findings ranked
by severity, each with its evidence (a test, a probe, a trace) and its owner; open no
issues yourself (plan §2.13): the program owner files them after the user approves.

Acceptance: the three documents exist and every finding names an owner.
Validation: none beyond the probes the documents record.
```

### P9.1 — Self-hosted bundle on the workspace-VM model (later phase)

```
You are implementing P9.1 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §16.2, §16.3, §6.2, §6.3,
§6.5, infra/self-hosted/README.md, infra/self-hosted/scripts/g5-workerhost.sh and
scripts/self-hosted-azure/*. Depends on P4.9a, P8.4. Owner: Platform.

Goal: a self-hosted control plane on a pre-provisioned VM and one or more pre-provisioned
worker-host VMs inside the customer's network run the workspace-VM model with no
auxiliary service.

Do:
1. Compose: the in-box worker service becomes `g5d-host` (the host daemon with
   `/dev/kvm`, the `ctl` tunnel to `g5-api`, slots from `G5D_HOST_MAX_MICROVMS`, retained
   roots and scratch under `/var/lib/g5/hosts`); keep `postgres`, `redis`, `db-migrate`,
   `g5-api`, `g5-api-indexer`, `edge`; `compose.kvm.yml` unchanged in purpose.
2. Worker-host installer: `install.sh --worker-host` and `g5-workerhost.sh` run the host
   container as `g5d host` with the registration token; keep `G5D_GUEST_HOST_ALIASES` and
   `G5D_GIT_HTTP_ALLOW_HOSTS` as host configuration so a host inside the VPC reaches the
   control plane at its private address and seeds the guest's `/etc/hosts`; document the
   private-DNS alternative.
3. Decide and document process isolation: either drop `G5D_ISOLATION_MODE=process` for
   the workspace-VM model (KVM required, as the installation guide's nested-virtualization
   sizing already assumes) or implement a dispatcher-as-host-process mode behind the
   existing acknowledgement flag; record the decision in the design's §16.
4. Rewrite the installation guide (`docs/self-hosted-installation.md`, replacing the
   stale PDF's content): deployment models, the responsibility split, host requirements
   per workspace VM, install, operate, register worker hosts, secrets (provider keys on
   the host, task secrets as env), flush and loss semantics, upgrade, backup (P9.3).
5. Contract tests: `scripts/test-self-hosted-bundle.sh` extended for the host service;
   the dry-run installer paths.

Do not: change the license and registry-token flow; touch the Azure workflow (P9.4).

Acceptance: `just up` on a KVM host brings the stack and one host to `ready`; a
workspace is created, chats with the fake provider, stops with a receipt and resumes;
the bundle contract tests pass.
Validation: bundle contract tests, a manual KVM-host run recorded in the PR.
```

### P9.2 — Microsoft Entra platform login for self-hosted installations (later phase)

```
You are implementing P9.2 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §10.4, §16.3, the P8.3 pull
request (#21) for what was removed, and the reference clone
/Users/zliu/Workspace/G5-original-reference/generation/g5-api-core/src/auth/oauth.rs,
generation/g5-api-foundation/src/config.rs, generation/self-hosted/env.template,
generation/scripts/self-hosted-azure/entra-login-smoke.js. Depends on P9.1.
Owner: Platform.

Goal: self-hosted installations can sign users in with Microsoft Entra ID; the SaaS
keeps GitHub only.

Do:
1. Re-add the Microsoft OAuth adapter (OIDC userinfo with the id-token email fallback,
   tenant, `G5_MICROSOFT_*` settings including the default org role map) behind
   `G5_OAUTH_ENABLED_PROVIDERS`, off unless configured; the login catalog entry and
   frontend icon; callback `/api/v1/auth/callback/microsoft`.
2. Self-hosted env template, `apply-install-env.py` keys, README and the guide.
3. Tests: api_auth.rs Entra cases restored from the reference clone; the installer
   contract; `entra-login-smoke.js` restored beside the GitHub smoke for the canary.

Do not: re-add Google or GitLab platform login; change SCM connections.

Acceptance: an Entra login completes in docker-ci against a fake OIDC server; the SaaS
configuration advertises `["github"]` only.
Validation: cargo check, lint, `just test-integration`, `just docker-ci` login lane.
```

### P9.3 — Backup barrier and restore of Postgres and the git store; upgrade drain (later phase)

```
You are implementing P9.3 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §16.3, §16.4, §5.4 (retention),
§5.8, design v7 §16.4 (why the barrier), and
infra/self-hosted/scripts/g5-selfhost.sh (`backup_cmd`, `restore_cmd`, the upgrade
flow). Depends on P4.3a, P9.1. Owner: Platform.

Goal: `just backup` produces a consistent pair of Postgres dump and git store; `just
restore` restores both; `just upgrade` drains workspaces before migrating.

Do:
1. API: `POST /api/internal/maintenance/barrier` (start, end, status) that makes the
   git service refuse ref writes (receive-pack 503 with retry-after; agents retry),
   refuses receipt completion, merges, forks, publishes and workspace deletions with
   `MAINTENANCE_BARRIER`, and lets reads, chats and module executions continue; a barrier
   auto-expires after `max_barrier_secs`.
2. `just backup`: start the barrier, `pg_dump -Fc`, `git bundle --all` per bare repo (or
   a filesystem snapshot when the store is on a snapshot-capable volume), copy the
   configuration, end the barrier, write `backup_manifest.json` with both digests and the
   barrier id; verify the dump and every bundle.
3. `just restore <backup>`: stop the app services, restore the dump and the bundles into
   an empty store, refuse a mismatched pair, run projection convergence on start, and
   verify that every complete receipt and every workspace branch resolves.
4. `just upgrade`: drain every workspace through the stop sequence (an operator route
   with a deadline) before the pre-upgrade backup and migrations; control-plane-first
   order for the host container.
5. Tests: a workspace deleted and another rewritten during a backup are wholly in or
   wholly out of the restored installation; every restored receipt resolves; the Azure
   canary's backup and restore step (P9.4) uses this path.

Do not: change the SaaS backup (Cloud SQL and Filestore snapshots are the SaaS path and
are documented, not scripted, here).

Acceptance: the consistency test passes; `just backup` and `just restore` round-trip a
workspace with a receipt.
Validation: bundle contract tests, `just test-integration` for the barrier.
```

### P9.4 — Azure canary on the workspace-VM host (later phase)

```
You are implementing P9.4 of docs/lean-implementation-plan-v7.md in
/Users/zliu/Workspace/G5-Lean. Read plan §2, design v7 §16.2, §16.3,
.github/workflows/self-hosted-azure-install.yml and scripts/self-hosted-azure/*.
Depends on P9.1, P9.2, P9.3. Owner: Platform.

Goal: the manual Azure canary installs the control plane and one nested-virtualization
worker host on a VNet with private DNS, and proves the workspace-VM model, Entra login,
VPC-internal routing, backup and restore, down and up, and upgrade.

Do: update the workflow and scripts to `g5d host` (registration, slots, the private
address through `G5D_GUEST_HOST_ALIASES` or private DNS), add a workspace smoke (create,
chat with the fake provider, stop with a receipt, resume, verify the file), the Entra
smoke beside the GitHub smoke, the P9.3 backup and restore step, and the contract test
`scripts/test-self-hosted-azure-microvm-canary.sh`.

Do not: make the workflow run on push.

Acceptance: a manual dispatch runs green once; the summary JSON records the release
image and the smoke results.
Validation: the workflow's local contract tests; one dispatched run.
```

---

## 7. Milestone and phase exit criteria

| Milestone | Done when |
|---|---|
| CRDT gone (P2.x) | no `yjs`/`yrs` anywhere; explicit Save with the conflict dialog; agents commit and push natively |
| Runtime (P4.2a, P4.2b, P4.3a, P4.4a, P4.4b, P4.11, P4.12, P4.13) | a workspace VM boots from a `VmSpec`, restores from a receipt, runs a fake-provider `agent.turn` invocation through the module runtime with events journaled, flushes on request with a verified receipt, and is fenced and proxied on the host |
| **MVP** (P4.5 to P4.8, P4.10, P5.x, P4.3b, P4.9a, P4.9b, P2.4) | one VM per workspace: two concurrent chats with queued messages, wake on message, stop with a receipt and resume with the restored edits and unpushed commits, restart with a new image, host kill recovery to the last receipt, a stale VM fenced everywhere, a complete timeline across an API restart, a suspended question answered after the VM stopped, preview and terminal on the VM, no per-task execution code; the QA environment runs it |
| Governance | a PR opened by an agent cannot merge without decisions at the pinned SHA; the merge operation recovers from every listed crash point; no remote-tracking code |
| Cleanup | one status enum; the kernel deleted; the live schema is the design's table set by forward migrations |
| Rollout | the QA rehearsal of P8.7 passes |
| Self-hosted | a pre-provisioned control plane and worker host run the workspace-VM model with Entra login, private routing, and a consistent backup and restore; the Azure canary is green |
| Platform | crates consolidated; CI lanes reflect the lean repository and gate merges |
