# Preview run log blocks the shared dispatcher — redesign request

## Human-provided finding (2026-10-20)

The owner reported "a serious problem with streaming logs" in the previous
design (the preview run log that streams stdout/stderr as task events) and
gave this diagnosis from live QA:

> Found the cause: a noisy preview build is blocking the shared chat
> dispatcher. The backend processes running tasks' event backlogs sequentially
> before starting queued chats. Each event requires a database commit and an
> acknowledgement RPC, with no per-task processing limit.
> (`backend/g5-api-core/src/services/agents/agent_tasks.rs`, dispatch ordering
> at `run_once`, event processing in `reconcile_attempt`.)
>
> - One preview build produced over 8,600 events.
> - Processing 2,558 of those events occupied the loop from 1:05:54–1:10:18 PM PDT.
> - Several queued chats started immediately afterward.
> - An "ok i see" chat message waited from 1:06:50 to 1:10:29, then received a
>   response (verified in the browser).
>
> This loop serves all organizations, so one noisy task can delay everyone.
> Multiple affected workspaces were confirmed; today's sampled messages were all
> in g5labs. The example recovered, but the underlying bug remains. The fix
> needs bounded event processing per task and dispatch scheduling that cannot
> be held up by a large backlog. No application or deployment changes were made.

## Human instruction

"can you reuse the same terminal mechanisms and propose a new design?"

## Agent interpretation

- "Terminal mechanisms" means the owner-only workspace shell path:
  workspace-scoped WebSocket, the host's `Pty` mux stream, a guest session
  attached by identity with a bounded output buffer, byte-offset reattachment
  and explicit reset ([g5.terminal.reattach](../../ontology/features/terminal.g5.md#g5.terminal.reattach),
  [g5.runtime.terminal.attachment](../../ontology/features/terminal.g5.md#g5.runtime.terminal.attachment)).
- The request is a design proposal, not implementation. The plan lives at
  [ontology-plan.md](../../pr_ontology_diff_design/preview-status-observation/ontology-plan.md);
  ontology amendments listed there are applied with the implementation once
  the owner accepts the design.
- Two defects are in scope: application output travelling through the
  execution journal, and a reconciliation loop with no per-attempt bound whose
  dispatch step waits behind it. Both must be addressed; fixing only the transport
  would leave any chatty attempt (an agent turn streams one event per delta)
  able to delay dispatch for every organization.
