# Default workspace execution slots of 16

## Human instructions

On 2026-10-08 the user asked why a new chat message in QA workspace `test-env-just-docker-ci` stayed queued. The investigation found both of the workspace's bounded execution slots (default 2) held by two running agent turns. The user asked where the limit came from, then requested:

> let's make it default to 16

While that work was starting, the user added:

> please also verify the statement below
>
> ```
> The VM has its own fixed limit of 4 running jobs (catalog.json:3).
> ```
>
> if that's true, should we also increase it?

When review found that migration 071 cannot tell an explicit 2, set through the settings API, from the old default, the user was asked which rows to raise and chose:

> Raise every row on 2 (Recommended)

## Investigation evidence

Observations from the read-only QA investigation, not additional requirements:

- Session `559b2ddc` was created at 16:52:44 UTC with its first message; no task for it reached the VM. Its workspace VM was running two agent tasks: a held turn that Start re-ran (task `20df8b63` attempt 2, re-running `just ci-docker`) and a long CPU/RAM-watch turn in another chat (task `3d4c5d70`, dispatched 14 seconds before the new message, when one slot was still free).
- `agent_dispatch.rs::reserve_queued_agent_dispatch` reserves a queued turn only while the workspace's running agent and command tasks are fewer than `workspace_settings.bounded_execution_slots`, which defaults to 2 (migration 003). The skip is not logged.
- The statement about the VM is true: `backend/g5d-dispatch/modules/catalog.json` caps bounded executions per VM at 4 in total (shared by agent turns, bootstrap commands, Changes operations and diagnostics; preview is exempt) and `agent.turn` at 4. The API retries a turn the VM refuses as an unconfirmed invoke that keeps its workspace slot; Changes operations give up after about two seconds of refusals.

## Agent interpretation and decisions

- "Default to 16" means a workspace whose owner never chose a slot count runs up to 16 agent and command turns at once. New settings rows take 16, and existing rows still on 2 are raised to 16: forks copy their source's value, so a column default alone would reach no existing workspace or later fork of one. No UI sets the column; an explicit 2 set through the settings API is raised too, as the user chose. Rows holding any other value keep it.
- Yes, the VM limit must rise with it, or the VM refuses turns 5–16. The catalog admits 16 agent turns and 20 bounded executions in total, keeping the 4 slots that task-less Changes (2) and diagnostics (2) operations can use. The catalog version, the agent turn manifest hash and the pinned catalog hash move together, so the change ships as a worker image rollout: hosts on the previous image refuse new VM starts until replaced.
- Independent review found that the backfill also raised admission on VMs still running a four-slot catalog, where four turns would leave Changes no slot. Until such a VM is replaced, admission on it stays at two turns; the cap keys on the VM's recorded catalog version.
- Delivery is two PRs, because migrations run in a Helm pre-upgrade hook while the previous API still serves, and a failed deploy rolls the API back while the migrations stay (review round 2): first the catalog and the two-turn ceiling, which change nothing for default workspaces; then the default and the backfill, merged only after the first PR's API runs in QA, so every API that can read raised settings has the ceiling.
- Slots bound concurrency only. Sixteen heavy turns share one VM's CPU, memory and disk; no resource reservation is added.
- Showing why a turn waits for a slot (the designed `queued_for_slot`) is a separate follow-up.
