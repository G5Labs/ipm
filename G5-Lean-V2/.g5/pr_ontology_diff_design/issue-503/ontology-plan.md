# Issue 503: trim provider lines over 1 MiB instead of failing the turn

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


Work on [issue 503](https://github.com/G5Labs/G5-Lean/issues/503), read with
`g5d issue show` on 2026-10-13. Authorized checkout branch: `agent-journal-limit`
(initial HEAD `20e393c0`).

## Ontology changes

Amended in [task execution](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/agents.g5.md):

- [`g5.agents.output-bounds`](../../ontology/features/chat.g5.md#g5.agents.output-bounds)
  no longer counts raw provider stdout or fails a turn on a long line. It now owns a
  fixed line ceiling, the bound on every recorded event, the discard-with-notice rule
  for a line past the ceiling, and an output limit that counts recorded bytes plus
  every provider line the turn records nothing for. The heading changed; both inbound links were retargeted.
- [`g5.agents.execution-limits`](../../ontology/features/chat.g5.md#g5.agents.output-bounds)
  last sentence: the catalog's serialized module-output cap and the turn's output limit
  now measure the same recorded bytes, so the catalog cap is an outer backstop rather
  than an independent raw-output bound.

The actual ontology diff is retained beside this design in Git. `g5.agents.execution-deadlines`
is unchanged: without a requested wall clock a runaway turn still ends through the
output bound, which is why discarded bytes keep counting.

## Governing contracts

| Commitment | Obligation here | Conclusion |
|---|---|---|
| `g5.agents.output-bounds` (amended; block text from the owner's review) | Every event bounded and masked before the cut, a long line discarded and noted, limit on recorded bytes plus lines that record nothing, room kept to settle, stderr log cap kept, limited settlement retains recorded output. | New governing behavior. |
| `g5.agents.execution-limits` (amended) | Every accepted limit governs the actual execution; request and policy `max_output_bytes` keep governing, now on recorded bytes. | The supervisor enforces the limit on what it emits. |
| [`g5.agents.execution-deadlines`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/agents.g5.md#distinguish-attempt-and-provider-deadlines) | A hung or runaway turn without a requested limit ends only by Stop, the output bound or a VM stop. | A newline-free flood must still reach the limit: discarded bytes count. |
| [`g5.agents.outcome.message-status`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/agents.g5.md#project-assistant-status-from-settled-outcome) | Preserve assistant text through settlement. | The recorded final text is bounded well above any model reply; the digest names the full text. |
| Dispatcher journal (`g5d-dispatch::journal::MAX_EVENT_BYTES`, `runner::MAX_LINE_BYTES`, both 1 MiB; no ontology owner) | A module line over 1 MiB is a module output limit. | Every event the supervisor emits stays far below it. |
| API ingestion (`task_lifecycle.rs`: 64 KiB timeline content, 64 KiB native request payload and display) | Oversized request payloads are consumable projection errors; timeline content is cut at 64 KiB. | Bounding request payloads at the supervisor makes a large permission request answerable instead of a projection error. |

## Design

Owner: the `agent.turn` supervisor (`backend/g5d-agent/src/runtime.rs`), which already
owns bounded output and event emission. No new owner, state or schema.

1. **Line ceiling.** The stdout reader assembles one line under `MAX_PROVIDER_LINE_BYTES`
   = 16 MiB, replacing the 1 MiB frame cap. The pinned providers cap one tool result near
   1 MiB before JSON escaping, so real lines reach about 1.5 MB (issue evidence: 1.04 to
   1.44 MB; a 1.07 MB Claude image read). Sixteen times that leaves a wide margin. The
   reader hands lines over a one-message channel, so a turn holds at most the line each
   reader assembles, one queued and one being projected. A line that crosses the ceiling is discarded to its
   newline: the reader reports the overflow once and each discarded chunk as it is
   dropped, the supervisor records a `provider_line_skipped` warning notice and reads on.
   Stderr uses the same reader; its 8 MiB log cap trips before the ceiling can.
2. **Every recorded event is bounded.** Tool input and output already carry a 32 KiB
   inline copy beside their digest. The same treatment extends to the remaining unbounded
   fields: `message.final` and `thought.final` content and `message.delta` text at 256 KiB
   as serialized (control characters escape sixfold)
   with `content_truncated` / `text_truncated` beside the existing `content_ref`;
   `provider_request.opened` payload and display through the structure-preserving JSON
   bound at exactly the API's own 64 KiB per-field limit, with `payload_truncated` and
   `payload_ref`; provider error notice messages at 32 KiB. Each value is masked with
   the module secrets before it is cut, as tool events already are, so a secret
   straddling a cut cannot leave a prefix the dispatcher's exact-match redaction misses.
   Request payload and display stay records: a projection too wide to keep its
   structure becomes a one-key record holding the cut rendering. The request the supervisor
   keeps in memory stays complete, so the envelope it writes back to Claude (allow with
   the original input, or the positional question answers) is unchanged. With these, the
   largest event is under 160 KiB and the journal's 1 MiB event bound cannot be reached.
3. **Output limit on recorded bytes.** `max_output_bytes` (request, policy, API default
   64 MiB) counts the event-line bytes the supervisor emits plus, at full size, every
   provider line it records nothing for: a line past the ceiling, or a frame the adapter
   ignores (Claude `stream_event` tool-input deltas or pings). A line that yields a
   recorded event counts only as recorded. Raw provider stdout is no longer capped as a
   stream, yet a flood of any shape still reaches the limit. Provider events use the
   limit less a settlement reserve (256 KiB, or an eighth of a small limit). Each event
   is serialized and measured against what remains before it is written; one that does
   not fit is withheld and the attempt ends, so neither one line with many large blocks
   nor a single event can overshoot. The turn keeps the budget and its unrecorded bytes
   in its state, so `record` checks recorded plus unrecorded output against one constant
   budget without recomputing anything per line. The reserve keeps room for terminal progress,
   harvest notices and the outcome, because the default limit equals the dispatcher's
   own 64 MiB cap on module output, which would otherwise stop the supervisor before
   it settles. A final message joins the turn's state only once its event is recorded,
   so the outcome never names unrecorded text. The
   supervisor checks the count on each main-loop pass and, past the limit, ends the
   attempt exactly as a limit did before: pending requests invalidated, provider group
   stopped, `limit_exceeded` / `max_output_bytes`, recorded events retained. The optional
   title child keeps a raw cap of the remaining recorded budget, at most one line ceiling.
4. **Unchanged.** Stderr's `max_log_bytes`; the dispatcher's catalog caps; the API's
   64 KiB timeline cut and 64 KiB request bound; the frontend, which already renders
   truncation flags and the `… [truncated]` marker inside cut text.

Rejected: raising the frame cap alone (a 17 MiB line would still fail a turn for a
provider that was not at fault); keeping the raw-byte limit (a long Codex turn with
sixty 1 MiB command outputs would fail at the 64 MiB default next); failing on an
over-ceiling line (contradicts the issue and the trimmed-not-failed rule).

## Implementation amendment

The first cut bounded request payloads at the 32 KiB tool bound. Inspection of the
reply path showed that the owner's UI can send values from the displayed request back
as `updated_input` when accepting a suggestion. A 32 KiB bound would have cut requests
between 32 and 64 KiB that the API accepts and answers today. The bound now equals the
API's limit, so every request answerable today is journaled uncut, apart from the
secret masking the dispatcher applies anyway; only a request the API would refuse as a
projection error is cut, and becomes answerable.
A plain Allow sends no `updated_input`, so the supervisor writes its complete copy.
The helper's last-resort string rendering is now measured after JSON escaping, so the
bound is exact. This refines the existing commitment without changing it.

Self-review before the first CI result found that the first cut counted only skipped
lines besides recorded bytes. An endless stream of valid frames the adapter ignores
would then never reach the limit, though the raw count used to bound it and
`g5.agents.execution-deadlines` relies on the output bound to end a runaway turn. Every
line that yields no recorded event now counts at full size; the runaway test covers it.

## Review amendment

Codex review of `a215ada2` and `8b7b5a2b` found gaps in the commitment as implemented,
all repaired under it:

- The new text and request bounds cut values before masking secrets; values are now
  masked first. A unit test puts a secret across each cut.
- The 16-message output channel could queue about 256 MiB of 16 MiB lines; it now holds
  one message.
- The limit was checked only between lines, against bytes already written, and the
  default limit left no room before the dispatcher's own 64 MiB cap on module output.
  Each event is now measured before writing against the limit less a fixed settlement
  reserve, and recording stops within a line. The runaway test's single line of forty
  30 KB blocks recorded 1.2 MB without this.
- A withheld final message still updated the turn's final text; it now joins the state
  only once recorded.
- A very wide request projection could become a bare string; it now stays a record.
- Text was bounded by raw bytes, and escaping control characters grows it sixfold; it
  is now bounded as serialized.

## Owner decision

Later Codex rounds (`4ddc58b8` to `d5d60bb6`) led to guards for cases the pinned Codex
0.159 and Claude Code 2.1.285 do not produce: charging each pending request's held input
and closing event to the budget, withholding events over the journal bound that only a
megabyte-long identifier could cause and bounding the outcome for them, keeping digests
instead of reply text across thousands of near-ceiling replies, and bounding the
forwarded copy of an oversized stderr line. The owner's review of 2026-10-14 removed
them: request inputs and replies are model output of a few hundred KB at most, ids are
short, and the dispatcher's own 1 MiB line and 8 MiB log caps remain the backstop for
impossible cases. Retained memory across many replies is pre-existing and outside issue
503. The same review set the ontology block's text and asked that further bot findings
without a provider version and the frame that produces them be answered, not
implemented.

## Assumptions

- Provider line sizes follow the issue's evidence from the pinned Codex 0.159 and
  Claude 2.1.285 CLIs; a future provider that legitimately emits lines over 16 MiB
  would see them skipped with a notice, not a failed turn.
- Tests use the dispatcher's fake-provider seam; live provider and VM behavior stays
  unexecuted here, as for issue 233.

## Observable consequences

- A provider line between 1 MiB and 16 MiB is parsed and recorded through the
  existing bounds; the turn no longer fails with `max_frame_bytes`, which no longer exists.
- A line over 16 MiB leaves a `provider_line_skipped` warning where it was; the turn
  continues and its bytes count against the output limit.
- A chat reply longer than 256 KiB as serialized is now cut, with the truncation marker,
  in the conversation. The API builds the assistant message from the full
  `message.final` content (`refresh_assistant_message_content`), not from the 64 KiB
  timeline cut, so before this change a reply was uncut up to the journal's 1 MiB.
  The digest still names the full reply.
- A native request whose payload exceeds the API's 64 KiB limit is now journaled cut
  and can be answered; before, it was a projection error.
- `max_output_bytes` measures what the journal receives. The supervisor's own
  duplication counts, so a test or policy that sized the limit to raw stdout may need
  more headroom; the API default of 64 MiB is unaffected in practice. One existing
  dispatcher test with ten 60 KB permission requests needed its limit raised for this
  reason.

## Evidence

Executed on 2026-10-14 in this Linux VM on the cut-back tree, with fake providers
through the real dispatcher and supervisor binaries (libkrun on the system loader path):

| Check | Result |
|---|---|
| `cargo test -p g5d --test dispatch_modules -- --test-threads=2` | 78 passed, 1 failed, 1 ignored. The failure is `agent_turn_claude_background_report_turn_stays_in_the_attempt`, whose 1.5 s setup deadline expires before provider launch under parallel load; it passes alone. |
| The three provider-line tests against the previous `runtime.rs` (2026-10-13) | All three fail, at the intended assertions. |
| `cargo test -p g5d-agent --lib` | 208 tests; `codex_auth` and `port_scan`, in unchanged files, flake under parallel load and pass alone. |
| `cargo clippy -p g5d-agent -p g5d --all-targets -- -D warnings`, `rustfmt --check` | Clean. |

New dispatcher tests: `agent_turn_records_a_trimmed_tool_result_from_a_codex_line_over_1_mib`,
`agent_turn_records_claude_lines_over_1_mib_and_skips_lines_over_the_ceiling`,
`agent_turn_output_limit_counts_recorded_bytes_and_ends_a_runaway_turn` (replaces the
frame-limit test) and `agent_turn_answers_a_permission_request_too_large_to_store_with_its_full_input`.

Remaining gaps: live Claude and Codex behavior and the VM path are unexecuted. API
ingestion of cut events relies on the unchanged schema-free projection and the shared
64 KiB request bound rather than a new API test.
