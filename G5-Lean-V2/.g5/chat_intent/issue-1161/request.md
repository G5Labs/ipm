# Remove the per-turn output limit — 2026-11-10

## Human instruction

For issue [#1161](https://github.com/G5Labs/G5-Lean/issues/1161) the owner decided to remove the 64 MiB per-turn output limit (`max_output_bytes`) for agent turns completely, not to tune it. The limit ended healthy long turns: QA task `1864a997-7508-4920-a183-0aa425b2645b` ran 5 h 37 min with 67 background sub-agents and ended `limit_exceeded` / `max_output_bytes` after recording about 3 MB of events, because output the adapter ignores (background sub-agent frames, streaming partials) was charged at full size.

The owner's scope, in their words and order:

1. Turn supervisor: delete the recording budget, the unrecorded-bytes counter, the settlement reserve and the main-loop limit check, so every event is always written. The session-title call and the ontology card get fixed caps instead of sizing themselves from the remaining budget.
2. The budget was the only bound on the in-memory control-reply queue; give that queue its own small fixed bound that ends the turn as a provider error.
3. Dispatcher: no output cap for `agent.turn`. The catalog `max_output_bytes` becomes optional, is removed from every `agent.turn` entry and stays required for all other modules. The per-event 1 MiB check stays, reported under its own code.
4. The catalog, its hash and the API's pinned catalog hash and version change together.
5. Wire compatibility during rollout: the new worker accepts the field and ignores it; the API keeps sending 64 MiB and drops only its upper-bound check; stored task specs carrying `maxOutputBytes` still load; frozen policy snapshots round-trip unchanged.
6. The ontology keeps only the per-line and per-event bounds, and the steering block no longer says a flushed receipt survives the output recording limit.

Out of scope: the 8 MiB stderr cap (`max_log_bytes`), the journal's own 64 MiB disk budget and the frontend message (old turns still carry `limit_exceeded`). No replacement time or spend limit is added.

The owner accepts what this leaves unguarded: a runaway turn now ends only by Stop or a VM stop, and the VM journal can grow while the API is not acknowledging events.

## Agent's choices within that scope

These are the implementing agent's choices, not further owner decisions:

- The control-reply queue holds at most 1 MiB of native control frames (handshake, hook answers and rejections) waiting for the provider's input. The reply that does not fit is dropped, a `provider_control_backlog` notice is recorded and the attempt ends `failed` / `provider_error`. The owner's own input (steering, native replies, End) is not counted.
- The title call's stdout is capped at 1 MiB; the ontology card keeps its existing 768 KiB bound.
- The catalog refuses an `agent.turn` entry that declares `max_output_bytes`, so a stale entry cannot restore the limit. Module versions and manifest hashes are unchanged; only the catalog version and hash move.
- One stdout line over 1 MiB ends a module run as `limit_exceeded` with `limit: max_event_bytes`.

## Found while implementing, not decided

The budget also bounded two things indirectly, which only matter for a runaway turn and are now left to Stop:

- What one turn adds to the API's stored timeline: every event the API acknowledges is kept, and nothing caps a turn's total.
- The supervisor's memory for the length of a turn: it keeps the text of the final messages it recorded and the ids of its native requests and tool uses, under the module's memory limit only.
