# Owner request

2026-10-30: "Work on G5Labs/G5-Lean#966: Chat: decouple live streaming from 5-second
history batches and fix repeated-Stop deadlocks. Read the issue with `g5d issue show`
before you start. after reading the issue to get the context, please follow the process
for design and then proceed on implementation. once you verified your changes, please
file a pr and then follow instructions below to make the pr ready to ship."

The owner's ship-readiness instructions: keep every GitHub check green, address P0/P1
(Codex) and Critical/Major (CodeRabbit) review comments with inline replies, keep the
branch free of merge conflicts, and validate each reviewer finding against the actual
execution path before fixing in-scope regressions; report other findings separately.

## Human decisions carried by the issue

- The routine history flush interval is **five seconds**. The issue states this is the
  owner's explicit preference and that it supersedes an earlier 500 ms proposal.
- Initial batch capacity limits of 256 events / 512 KiB may remain, subject to
  measurement; they are capacity safeguards, not a replacement subsecond timer.
- Reuse the existing guest journal as the durable recovery source; do not introduce
  another durable queue or rely on browser memory for recovery.
- Retain original event identities; reducing transaction granularity does not require
  redesigning the durable event-row format or dropping output.

## Interpretation

Implement the live relay, batched history, tail recovery, Stop lock removal and Stop
idempotency with the regression coverage the issue lists, in one PR. Where a criterion
needs infrastructure the repository lacks (for example a 10,000-delta browser run), use
the production boundaries that supply the guarantee and report what remains unexecuted.
