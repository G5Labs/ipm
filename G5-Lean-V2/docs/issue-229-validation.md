# Issue 229 validation and review evidence

Base: origin/main `412cccae`.

## Regression discrimination

The new `agent_execution_tree_is_reaped_before_settlement` test passed through
`g5d dispatch` on Linux. Restoring origin/main's runner and agent supervisor while
keeping the test made it fail in 0.66 seconds: the recorded provider was still
alive after cancellation settlement (`kill(pid, 0)` returned 0). The disposable
Linux container removed the leaked fixture processes. The implementation was
restored before subsequent validation.

The current matrix covers owner cancellation, owner pause, interruption, outer
wall/output limits, internal provider timeout and supervisor SIGKILL. Detached
tools call setsid and providers ignore SIGINT after recording it. Assertions
cover exact PID disappearance at settlement, capacity refusal/reuse, no later
workspace writes or journal events, and continued sibling writes.

## Validation

- Linux focused regression: 1 test, seven scenarios, passed (37.62 seconds).
- Local preflight: passed, including 46 architecture assertions and inventory,
  migrations, fixture and harness guards.
- Affected crates: 30 agent-turn and 273 CLI tests passed on the first run.
  One unchanged doctor test failed because the crate was built without libkrun
  but its Linux assertion unconditionally expected nested-virtualization text.
  The exact selector passed with `--features libkrun` (1 test). No doctor change.
- Remaining affected tests: 114 dispatcher unit, 300 foundation unit, 49 crate
  architecture, 30 dispatcher process, 36 module process, and 19 PTY process
  tests passed. One pre-existing host-materialized-VM test was ignored.
  Total unique passing tests across these affected suites: 852.
- Final targeted rerun: 14 termination tests passed (including two new receipt
  failure tests), the seven-scenario dispatcher regression passed, and 49
  architecture plus 19 PTY tests passed after sharing the proc-stat parser.
  Including the two added tests, 854 unique affected tests passed.
- `cargo check --locked --workspace --all-targets`, strict
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, formatting
  and staged diff checks passed. Clippy's conditional simplification was fixed
  before review; the affected receipt tests and final workspace checks were rerun.
- Independent Claude review: two complete full-scope reports; one bounded call timed out. No significant findings remain after the consolidated test improvements and independent validation of the timing hypothesis.

## Reuse and platform boundary

The PTY `TerminalReapWaiter` owns the portable-pty leader and checks the original
Linux session. Its common `process_termination` helper remains the provider's
SIGINT/grace/SIGKILL path. A new session is outside the PTY waiter's inventory;
the private child-subreaper supplies adoption and ECHILD proof without requiring
cgroup delegation. It extends the existing process-termination module in the foundation crate and starts before Tokio,
so one waiter owns its direct/adopted children. Direct-child enumeration reuses
the PTY proc-stat parser and avoids optional kernel /proc children support. It does not change PTY policy.

No live Claude/Codex service, API pause end-to-end flow or booted guest was exercised
by the synthetic process regression. macOS has no new descendant-reap guarantee.
Cloud CI remains required on the final PR head.

## Self-review

The owner must survive a dropped runner future: disable child-handle kill-on-drop
and let private socket EOF trigger cleanup. Unexpected /proc/reap failures retain
adoption and execution capacity instead of claiming exit. Internal timeout stops
waiting for inherited tool pipes and reaches bounded harvest/owner cleanup.

Independent review was requested only after the local gates passed.

## Independent review, round 1

Three fresh Claude CLI reviews received the same full scope, passing verification
results, six-round limit, and reminder that gh/gcloud are available outside the
sandbox. Two completed; one reached the 20-minute bound without returning findings.
Raw completed reports remain in `.dev/issue-229/claude-round1-{2,3}.txt`.
Both completed reviewers found the ownership, receipt/capacity boundary, sibling
isolation and PTY reuse sound. One reviewer also reported a clean compile of the four affected crates on macOS.

Initial findings and independent dispositions, recorded before the repair batch:

- Deeper escaped descendants were not recorded by the regression: accepted; add a
  double-forked descendant in its own session and assert its PID is gone.
- Missing proof gives no terminal client outcome: intentional. A terminal outcome
  would authorize release without evidence. Keep admission and the running execution,
  log uncertainty, and leave independent VM teardown as the recovery boundary.
- `Instant::now() - GRACE` might panic before two seconds of guest uptime: rejected
  after a focused Linux Rust probe successfully subtracted beyond the boot epoch.
  Linux Instant supports negative timespec values; the proposed boot-epoch scenario
  does not reach its representational bound. No speculative timing change.
- Startup SIGTERM may arrive before the supervisor handler: hard cleanup remains
  authoritative; no provider can have launched before this handler is installed.
- Self-review: move marker files from an external temp HOME into the checkout, and
  assert that cancellation acknowledgement does not itself release capacity.

The consolidated repair is limited to regression strength; no production behavior
or governing commitment changes are required by these findings.

The consolidated regression now records the supervisor, provider, separate-session
tool and double-forked grandchild; marker files live beneath the real checkout.
Cancellation acknowledgement is followed by an explicit capacity-refusal assertion.
All seven scenarios passed again (37.69 seconds), then workspace check, strict
Clippy, formatting and diff checks passed on the final candidate. The repair batch
changed tests only. The initial review concluded with no significant findings.

Required current-head Cloud CI results will be recorded on the PR.

## CodeRabbit follow-up and focused review, round 2

Accepted both in-scope CodeRabbit findings: gate the termination listener for
non-Unix targets, and remove the timeout matrix's dependence on delayed sibling
startup. Signal registration remains synchronous before setup. The sibling now
starts first with a long deadline; only the primary pins the short-deadline
catalog version. Catalog lookup without a version selects the original entry.

The seven-scenario Linux regression passed again (46.72 seconds), all 30
agent-turn tests passed, and workspace check, strict Clippy, formatting and diff
checks passed before requesting focused Claude review. Claude inspected the
changes and confirmed that the interruption future cannot be polled after
completion, but reached its tool-turn bound before a final verdict. A bounded
final-verdict request also timed out. No actionable follow-up finding was
returned; this is incomplete review evidence, not a completed approval. The
implementor independently verified catalog selection and retained the passing
production-boundary regression. The two earlier full-scope reviews remain the
completed independent review evidence.

First-head Cloud Rust CI passed. Both required checks must still pass on the
updated PR head; their authoritative results remain on the PR.
