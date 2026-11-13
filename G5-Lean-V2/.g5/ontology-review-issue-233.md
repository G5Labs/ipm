# Issue 233 implementation review

## Scope and review mode

This is the implementing agent's self-review of the behavior branch. Independent
review follows the final local validation. The pinned design is
[ontology-plan-issue-233.md](ontology-plan-issue-233.md); this review starts
from its `Ontology changes and governance` inventory and the saved
[request](chat_intent/issue-233/request.md). The separate mechanical
consolidation branch will extend this same review record.

## Initial self-review findings

1. **Unsupported Codex policy turn cap.** `AgentTurnInputV1::validate` rejected
   a Codex request with `max_turns`, but accepted a policy snapshot declaring
   the cap when the request omitted it. That allowed an unenforced bound. The
   finding was discovered during self-review and fixed before this record was
   written: validation now rejects either source. The focused unit selector
   `rejects_unenforceable_turn_limits_before_delivery` passed after the fix.

## Independent discovery, round 1

Three fresh, full-scope, read-only reviews were saved before sharing at
`/private/tmp/g5-issue-233-review-{a,b,c}.md`. Their findings are consolidated
here before repair; the raw reports remain preserved. The requested Claude
CLI review was invoked after local validation and saved at
`/private/tmp/g5-issue-233-claude-review.txt`.

1. **Policy-only Claude turn cap is omitted (A, B, C).** Validation permits
   request `None` with policy `Some(2)`, while launch only reads the request.
   The public dispatcher contract can therefore accept an unenforced policy
   bound. Repair the effective turn cap and exercise actual argv through the
   dispatcher. B's suggestion that a request looser than policy must be
   clamped is not adopted: the current input contract rejects
   `LIMIT_EXCEEDS_POLICY`, which is a valid refusal of an unauthorized request.
2. **Timeout bypasses bounded harvest (A, C).** The supervisor's outer timer
   drops `run` at the exact attempt deadline and the dispatcher stops the
   supervisor at the same deadline, so neither can provide harvest grace.
   Preserve the success deadline and grant bounded cleanup/harvest time.
3. **A cap-crossing read drops an earlier complete frame (A, B).** The reader
   rejects the entire read before scanning the allowed prefix. Process that
   prefix through framing before reporting the limit; assert replay of it.
4. **Title stdout is unbounded (B).** The optional one-shot Claude title call
   uses `read_to_end` despite the turn's output bound. Bound this child before
   allocation, and test the active title path.
5. **Limit classification is lost at task settlement (B).** The supervisor
   reports byte/frame caps as generic failure and TaskService maps inner
   `limit_exceeded` to `provider_error`. Preserve a distinct attempt reason.
6. **A timed-out partial initial write can be marked withdrawn (B).** No
   accepted event proves complete handoff, but some bytes may have crossed.
   Carry the ambiguity to task settlement and mark the reserved input
   `delivery_unknown`; retain the proven pre-write release path.
7. **The duplicate outer signal listener can preempt inner harvest (Claude).**
   Like finding 2, the outer `select!` dropped `run` before its cleanup and
   harvest. The same bounded-grace repair addresses timeout and interruption.
8. **Non UTF-8 stderr fails the turn (Claude).** The new reader used strict
   UTF-8 decoding for diagnostic stderr. Decode stderr lossily while keeping
   strict stdout protocol parsing.
9. **The API Codex guard lacks its own unit test (Claude).** The accepted
   provider is resolved before the direct guard in `agent_dispatch.rs`; the
   active dispatcher no-spawn test and turn-input validation test exercise the
   essential enforcement. A separate unit that copies this one-line guard
   would not exercise launch resolution. This is recorded as a low-impact
   coverage gap rather than another implementation defect.

The reviews did not run full Docker CI. Their positive check evidence is
static inspection plus `git diff --check` and one focused validation unit; the
supplied local validation is recorded below. A/B/C all disclosed that their
contexts could not fetch the private GitHub issue, and used the saved human
request and pinned plan. The implementing session did read issue 233 through
`gh` before the implementation and reread it during self-review.

## Round 1 repair and recheck

- Policy-only Claude caps now feed the CLI and max-turn result mapping. The
  active dispatcher selector passed with the policy bound and no requested
  cap. A request above policy remains refused by the existing input contract.
- The dispatcher passes its absolute attempt deadline to the supervisor.
  The supervisor's provider loop retains bounded cleanup and harvest after
  expiry; the dispatcher grants a narrow settlement grace. The blocked-write
  and terminal-before-EOF selector passed and observed the harvesting phase
  after timeout. Native auth and stalled harvest remain unexecuted.
- The reader processes allowed bytes before emitting a cap; the unit test
  passes with a complete frame and excess bytes in one read. The active
  dispatcher flood test passed with the earlier assistant unit in replay.
- The optional title child reads into a bounded buffer using the remaining
  turn stdout allowance. The active dispatcher title-flood selector passed
  before its 8-second bound, with a successful main turn and no title event.
- Supervisor `limit_exceeded` survives TaskService settlement as the durable
  attempt reason through migration 043. The API process test passed and
  queried the persisted reason. The same test passed both ambiguous-write
  `delivery_unknown` and proven pre-write release, including task hold state.
- Lossy stderr decoding passed its focused unit test. Four reader tests and
  the full 41-test agent-turn library passed. The dispatcher library passed
  114 tests. In the API core library, 1003 tests passed inside the sandbox;
  all 11 socket-denied cases passed in focused groups outside it. The focused
  API process test passed. Architecture, migration and inventory preflight
  guards passed.

## Commitment checks

| Commitment | Actual path and counterexample/check | Result or remaining gap |
|---|---|
| `g5.agents.execution-limits` | API dispatch rejects Codex turn requests; turn input validation rejects unsupported request or policy caps; Claude launch supplies `--max-turns`; supervisor counts request-bounded raw output; dispatcher intersects catalog, policy and request wall clocks. Synthetic dispatcher tests exercise Codex rejection, Claude turn flag and tighter output. | Focused checks pass. Live provider and VM qualification remains unexecuted. |
| `g5.agents.output-bounds` | Supervisor reader counts bytes before frame allocation and JSON parse; active dispatcher tests send newline-free output and an oversized frame. | Reader unit and active dispatcher regressions pass. |
| `g5.agents.execution-deadlines` | Dispatcher clock begins before provider-reference refresh and initial module write; supervisor clock begins before auth, restore and provider write. Active dispatcher sends a blocked input and a terminal frame before EOF. | Both time out in focused checks. Native auth, harvest stall and Linux cleanup proof need Cloud CI or further evidence. |
| `g5.agents.outcome` | Supervisor needs terminal result, stream closure and exit code. Early exit and terminal-before-EOF tests exercise distinct failures. | Focused checks pass; live-provider path unexecuted. |
| `g5.architecture.agent_execution_tree`, `g5.design.cleanup-proof` | Dispatcher retains issue 229's execution owner and sibling custody. | Existing guards pass; macOS cannot prove Linux descendant reaping. |
| `g5.architecture.agent_lifecycle` | Active dispatcher invokes `g5d-agent-turn`; older `g5d-worker::runner::Runner::run_turn` has no production callers. | Partial until the separate mechanical consolidation branch removes the competing test-only lifecycle. |
| `g5.conversation.delivery`, `g5.conversation.delivery.unknown` | `input.accepted` follows the complete initial write; failed or timed-out blocked write emits no acceptance. | Active dispatcher regression passes. |
| `g5.provider.device-login.deadline`, `g5.conversation.bundle.harvest` | Supervisor attempt deadline wraps setup and harvest while their own narrower ceilings remain. | Source inspection supports the bound; stalled auth/harvest focused runs remain unexecuted. |

## Verification record

The smallest reader regression passed (2 tests). Focused active dispatcher
selectors for output/frame limits, blocked input/terminal-before-EOF, Codex
limit rejection, malformed frame/early exit/Claude turn cap and existing
journal/replay passed (five selectors, 5 tests). `g5d-dispatch --lib` passed
114 tests outside the sandbox; `g5-api-core --lib agent_dispatch` passed 13.
`g5d-agent-turn --lib` passed 38 of 39 outside the sandbox, with one fixture
using an obsolete Codex max-turn cap; the corrected failing selector passed.
The architecture/inventory `validation-preflight` guards passed. Final
`cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all -- --check` and `git diff --check` all passed on the local
candidate before independent review.

The existing Codex interactivity process selector failed on macOS in its
unchanged later profile-launch branch because the scrubbed child could not
resolve `libkrun.1.dylib`. Its test branch already exists in `origin/main`.
The new isolated Codex max-turn selector passed, so this harness limitation is
not used as evidence for the new rejection path.

## Round 2 independent re-review and repair

All three ontology reviewers closed their original findings on the inspected
paths. The Claude CLI reviewer also closed its two defects and found no new
significant issue. Independent reviewers A/B/C identified three related
settlement counterexamples in the repaired delivery path:

1. A dispatcher timeout after a journaled `input.accepted` can have no final
   module result. Inferring unknown delivery from that missing result alone
   holds a task whose input is already durably delivered. TaskService now
   promotes only a still-reserved input to `delivery_unknown` in its settlement
   transaction and bases the hold on that actual update.
2. The initial dispatcher-to-supervisor invocation write can be interrupted
   after some bytes cross. The dispatcher now records write commencement and
   carries an ambiguity fact on unsuccessful handoff, including cancellation
   and control-channel closure. The API applies it only when a reserved input
   remains and device authentication does not prove pre-delivery failure.
3. Owner cancellation during an ambiguous provider write was overwritten by
   a nonterminal unknown-delivery hold. TaskService now preserves terminal
   cancellation and its dependent/job settlement, records the unknown fact in
   the input audit, then withdraws the cancelled input. The durable API test
   includes this race and accepted-input dispatcher timeout.

These are accepted findings, with focused regression and final workspace
validation pending. They do not change the approved ontology commitments.

## Round 3 focused re-review and final local evidence

A, B and C independently rechecked the round 2 delivery and cancellation
counterexamples and found them closed on the inspected paths. B independently
ran the partial-write dispatcher unit; A ran `git diff --check`. Their raw
read-only reports remain at `/private/tmp/g5-issue-233-review-{a,b,c}.md`.
The required Claude CLI round 3 was invoked only after the final local gates;
its result is recorded separately once returned.

After round 2 repairs, the focused persisted API selector passed all five
cases (ambiguous write, proven pre-write failure, limit classification,
accepted input followed by dispatcher timeout, and owner cancellation during
ambiguity). The dispatcher partial-invocation-write unit and active
blocked-input/terminal-before-EOF process selector passed. `g5d-dispatch`
passed 115/115 library tests and `g5-api-core` passed 1014/1014 library
tests outside the sandbox. `just validation-preflight` passed every guard,
including architecture and migrations. The final candidate passed
`cargo check --workspace`, strict `cargo clippy --workspace --all-targets --
-D warnings`, `cargo fmt --all -- --check`, and `git diff --check`.

Live provider and VM harvest persistence and a complete native-auth and
stalled-harvest matrix remain Cloud CI qualifications. The second PR will
remove the unused test-only Runner lifecycle; this behavior PR leaves
that disclosed architecture gap until the stacked consolidation lands.

## PR review and first Cloud CI follow-up

CodeRabbit identified two in-scope edge cases on PR #259. Migration 043 now
adds its replacement reason constraint `NOT VALID`, and migration 044 validates
historical rows in a separate transaction, following migrations 034/035. The
provider-reference refresh deadline now uses checked `Instant` addition and
retains its 120-second cap. The affected API process and dispatcher library
tests and all validation-preflight guards passed after these changes.

The first Cloud Rust lane failed two selectors. The agent containment test
observed a structured supervisor `timed_out` result delivered successfully
before the dispatcher's outer cleanup deadline. Its old assertion required
only the outer `timed_out` status, so the fixture now permits either bounded
settlement path while still requiring descendant reaping and slot release.
The unrelated `g5-scm::scm_transport_tests::authenticated_native_lease_push_and_compensation_succeed`
fixture failed a TLS handshake under the parallel Cloud run; its exact selector
passed when rerun in isolation outside the local socket sandbox. No SCM source
changes are in this branch. The stacked PR's first Cloud Rust lane passed that
SCM fixture and failed only the same old agent containment assertion. The
corrected Linux containment selector passed in a focused Docker phase 5 run
(1/1, 262 skipped), exercising all seven settlement modes and their descendant
reaping and slot-release assertions. PR #259's first Cloud E2E lane passed.
Final local validation and the final focused reviewer pass are recorded below.

## Final follow-up verification and disposition

On the post-CodeRabbit candidate, the API process selector, dispatcher library
(115/115), and all validation-preflight guards passed. The corrected Linux
containment selector passed 1/1 in the focused Docker Phase 5 run. Workspace
check, strict Clippy across all targets, formatting and diff checks passed.
Claude CLI's read-only round 6 review found no significant in-scope defect or
disagreement in the migration split, checked deadline arithmetic or timeout
fixture. This closes the six-round review limit without an unresolved finding.

## Mechanical consolidation branch review

Caller trace across the workspace found `Runner::run_turn` and process launcher types referenced only by legacy module tests. The active `agent.turn` supervisor imports provider adapters, event types and session-file helpers. The first consolidation candidate removed the test-only process lifecycle and retained those imported parts. Round 4 independent review found that `TurnSpec` and the Claude/Codex argv builders had only test callers; the active supervisor constructs argv directly, so retaining those builders would preserve misleading tests of an inactive policy path. The finding is accepted: the unused builders and their tests are removed while the production frame mappers, checked Claude stdin construction, event types and session-file helpers stay. Reviewer A also noted that the branch must rebase onto the behavior branch after its origin/main rebase. Validation of this repair is recorded below.

## Mechanical consolidation disposition and evidence

Round 4 independent reviewers A and C found no significant lifecycle or
consumer defect. Reviewer B found that the first consolidation draft retained
`TurnSpec` and Claude/Codex argv builders with only test callers, leaving a
misleading inactive policy path; C also caught the saved plan's false claim
that the active supervisor used `TurnSpec`. The source search confirmed these
findings. The draft's unused builders, policy parser, stub stdin, constants and
tests were removed and the plan corrected. Reviewer A's upstream integration
condition requires the stacked branch to rebase onto the current behavior
branch before publishing.

After repair, focused Claude stdin and Codex mapper tests passed. The worker
library passed 107/107, the active supervisor library 41/41, and the active
`g5d` dispatcher malformed-frame/early-exit process selector passed. Every
`just validation-preflight` guard passed. The final candidate passed
`cargo check --workspace`, strict `cargo clippy --workspace --all-targets --
-D warnings`, `cargo fmt --all -- --check`, and `git diff --check`.

Reviewer B's focused round 5 read-only re-review found the inactive-builder
finding closed and no new significant defect. The required Claude CLI review
first exhausted its turn cap without a verdict; the focused retry after the
repair found no significant in-scope defect. Raw reports are in
`/private/tmp/g5-issue-233-review-a-pr2.md`, `g5-issue-233-pr2-review-b.md`, `g5-issue-233-review-c-pr2.md` and `g5-issue-233-claude-pr2-r5-review.txt`.
The final Claude CLI round 6 on the behavior PR found no significant finding
or disagreement. Live provider and VM harvest persistence still require Cloud
CI qualification; the focused Linux descendant-reap selector passed on the
behavior PR.

## Stacked rebase and PR feedback

The consolidation branch was rebased onto behavior commit `65c5ada3` after
reconciling both branches' review evidence. The final CodeRabbit comment on
this PR identified stale "pending" wording for validation already recorded
above; the wording is corrected. After the rebase, the worker library passed
107/107, the active supervisor library passed 41/41, and the active dispatcher
malformed-frame/early-exit selector passed 1/1. The process selector needed a
temporary local loader alias for this Mac's installed libkrun version; no
repository or product change was needed.
