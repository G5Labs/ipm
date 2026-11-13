# PR checks independent review two — repair recheck

Read-only full-scope recheck of the repaired PR-checks delta above
`fa1685cca72ebc548ab4821db95fd60ab3fe72ba`, including the untracked checks owner,
the saved amendment and consolidated findings. This follows the independent discovery
in `pr-checks-two.md`; it is an incremental recheck in the same reviewer context.
No production, test or ontology edits, delegation or shared DB/build writes occurred.
The unrelated parity documents and newly concurrent parity changes to core user stories,
security and workspaces ontology are excluded as directed by main.

Production snapshot inspected:

- `workspace_pull_request_checks.rs` SHA256 `d3883a9d320f5eda04dfe178d52b3214f95610b0c24f378f8c292402c226f67f`.
- `g5d-cli/src/pr.rs` SHA256 `b8db22abf5ffe90f4f4cfd2080aea6c997d7f967568d1465634cf5ea7753ac95`.

## New finding before further repair

### R2-R1 — P2: unfinished suites hide already observed failures in the current attempt

**Location:** `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:417–428`.

The rerun repair rewrites every completed check to pending whenever its suite is not
completed. An ordinary suite can still be in progress while one current job has
finished with failure and another current job is running. This is not evidence that
the completed job belongs to an older attempt. The new condition converts that real
failure to `state=PENDING,bucket=pending`, suppresses its diagnostics, and prevents
`--watch --fail-fast` from stopping on the observed failure. A nonwatch read similarly
returns pending instead of reporting the known failure. It may make the agent wait for
a long unrelated job before diagnosing the already failed job.

**Governing:** `g5.governance.pr.checks`, `.details`, `.revision`; saved interface's
failure-bucket and fail-fast contract; `g5.data.outcome-evidence`. The repair amendment
says an active suite invalidates **older** completed attempts. It does not make all
completed jobs in an active suite old or superseded.

**Runtime evidence:** isolated `active_failure` fixture has a stable PR head, an
`in_progress` suite, a current `completed/failure` job with supplied compiler diagnostic
text, and another `in_progress` job in that same suite. The actual extracted provider
owner returns three pending rows (suite plus both jobs), `availability=ready`, no
failure bucket, no diagnostic fields and no annotation request. The CLI's unchanged
`check_outcome` cannot set `failed`, so fail-fast cannot trigger. The new supplied rerun
test uses queued suites or fully completed suites; its other failed job belongs to a
separate completed suite, so it does not discriminate this valid active-suite case.

**Proposed fix:** invalidate only outcomes actually identified as superseded; do not
infer supersession solely from suite incompleteness. Keep the suite's pending evidence
separate from the real status of current completed jobs. Preserve the old-success
rerequest protection and same-head snapshot revalidation while exposing current failures
and diagnostics promptly. Clarify the repair note if necessary; do not weaken the
existing failure/diagnostic/fail-fast commitments.

**Discriminating regression:** a first/current attempt has one failed completed run and
one running run in the same in-progress suite. Public/VM checks must expose the failed
run and requested details; nonwatch exits 1, ordinary watch may keep waiting, and
fail-fast exits 1 immediately. Pair it with the queued-rerequest/old-success fixture and
the same-head attempt transition fixture to preserve those distinct protections.

## Original/consolidated findings rechecked

| Finding / commitment | Actual repaired path and check | Result |
|---|---|---|
| R2-1, queued rerun false success | Bounded suite inventory, run-to-suite identity validation, second suite/run/status snapshot and final revision vector read | Original queued-suite/old-success counterexample now yields only pending buckets. Same-head change between snapshots yields pending with empty checks. **Original failure repaired; valid active-suite case regresses as R2-R1.** |
| Test-merge omission; `.revision` | Open/mergeable PR supplies current merge SHA; head and test-merge snapshots label `headRefOid`, `commitOid`, `revision`; unknown mergeability is pending, base/merge/head/state revalidated | Source inspected; isolated stable head-pass/test-merge-fail fixture yields distinct pass/fail rows with correct commit identities. Added tests cover unknown and changed revision vectors. No remaining defect found in those cases. |
| R2-2, lost partial diagnostics; `.details` | `diagnostics` catches annotation failure and retains cleaned/bounded output; reader returns unavailable with the partial rows | Isolated annotation 503 retains summary/text and token/config redaction, marks details partial/annotations unavailable. Original counterexample repaired. |
| R2-3, rate-limit continuation; architecture | `ReadFailure.rate_limited`; annotation failure exits observation; outer reader skips remaining repositories; CLI sees unavailable and stops | Isolated annotation 429 stops on that request, retains text and returns unavailable/rateLimited. No final head call follows. Added API test checks multi-repository stop; added compiled CLI test checks one poll and retained summary. Original counterexample repaired by inspected composition; this reviewer did not rerun the full compiled suite. |
| Redaction corrupting protocol | `clean` applied only to provider text; derived state/bucket/availability and identities remain outside redaction | Isolated secrets `pass`, `ready`, `secret-value` redact matching check text while ready/pass protocol survives. Supplied real-route test adds matching secrets. Repaired. |
| Lost Ctrl-C across request/wait | One pinned cancellation future consumed in both selects | Source inspected; supplied two-phase process test exercises read and wait. Main reported 18 compiled tests passing after fixing the test-only `assert_cmd` API mismatch. That report is main's evidence, not an independent execution by this reviewer. |
| Stored provider identity | Stored `repo_id` compared with selected attached UUID before credentials/provider calls | Source inspected; added mismatch test discriminates same cached name/different repository ID. No remaining defect found. |
| Enumeration completeness | Both suite/run lists and statuses use bounded pages, body limit, total-count/incomplete handling; suite enumeration guards hidden suite cap | Source inspected; incomplete/empty paths preserved. Successful real-provider pagination is still unexecuted here. |
| Authority / credential / read-only commitments | Same public/current-incarnation adapters, scoped selector, current repository credential owner and read permissions; GET-only check adapter and rollback before provider I/O | Original review evidence retained; no repair changes admission, write authority, token projection, publication or approval behavior. Stored identity repair strengthens the reused selector. No new defect found. |
| CLI/JSON/docs/minimality | Parser/field projection and exit logic, actual commit labels in readable/JSON output, existing process shell, shared root guidance | Entire delta reinspected with repair scope. No added durable state, queue, monitor or arbitrary-URL fetch. No architecture excess found. Fail-fast correctness depends on repairing R2-R1. |

## Verification and limits

The isolated runtime probe compiled the unchanged helper/reader section from
`fn valid_sha` onward, substituting only enclosing query/error/header definitions.
It uses real reqwest against a local fake GitHub protocol and writes only scratch.
It does not substitute the normalization/diagnostics behavior being checked, and does
not prove real API/DB admission. It links already built dependency artifacts read-only.

Receipts: `/private/tmp/g5-pr-checks-two-recheck-qcca7apt/` contains `probe.rs`,
`probe`, `build.log`, `protocol.py` and `results.json`. Seven scenarios executed:
queued rerequest, active current failure, annotation 503, annotation 429, protocol-safe
redaction, failing test-merge, and same-head snapshot change. Six show the intended
repair; active current failure reproduces R2-R1. Scoped `git diff --check` also passed.

Main owns the full test/Clippy run and reported all 12 API read tests and all 18 compiled
CLI tests passing while this recheck was in progress. Production remained frozen; main
notified this reviewer of a test-only cancellation fixture correction and a planned
Clippy-only equivalent enum-filter simplification after this report. No author/test
pass claim substitutes for the counterexample above. Live GitHub, deployed VM execution,
provider pagination and real synthetic-merge protocol remain unexecuted by this reviewer.

The four new metadata footers should reflect the remaining failure-visibility defect
until repaired and rechecked. The current ontology meaning already covers the defect;
no new policy or weakening is justified.

## Focused repair disposition — R2-R1 closed

Rechecked the subsequent frozen checks-owner snapshot, SHA256
`f22a485c5d581ec348d4dfcf856885169e120e8c0667dd8bf2970db3e7cef437`.
The terminal-to-pending override now applies only to a queued suite; an in-progress
suite retains each current job's actual conclusion alongside the separate pending
suite row. The status-expression change is equivalent validation with unknown fallback.
No additional production change or defect was found in this focused recheck.

Reran all seven isolated provider scenarios against the actual repaired helper/reader
section. The current active-suite fixture now returns `ready`, one failed run, two
pending rows and available, redacted summary/text plus the requested line annotation.
The original queued-rerequest fixture remains pending; same-head change still drops
superseded rows. Partial annotation failure, rate-limit stop, protocol-safe redaction
and test-merge failure cases retain their intended results. This closes R2-R1 without
reintroducing the original queued-success defect.

Fed that exact repaired active-suite observation to the current compiled `g5d` through
an isolated localhost API. Both nonwatch and `--watch --fail-fast --interval 1` made
one API read, exited 1 and retained the failure diagnostic summary in selected JSON.
Fail-fast returned in 0.038 seconds, without waiting for the other pending job.
This establishes the authoritative provider projection and its actual CLI consumer;
API/DB admission remains main's separate test lane.

New receipts under the same scratch directory: `probe-focused.rs`, `probe-focused`,
`build-focused.log`, `protocol-focused.py`, `results-focused.json`, `g5d-focused`,
`cli-focused.py`, `cli-focused-results.json`. Initial evidence and counterexample
artifacts are preserved unchanged. The successful-annotation response in the focused
active-suite fixture makes its details assertion discriminate ordinary failure visibility
from the separately tested annotation-error early return.

Inspected the added public+VM active-suite regression and compiled nonwatch failure-plus-
pending case; both assert the intended output. Main subsequently reported all 13 API
read tests passing. Main's planned move of unchanged `attached_repository` before the
test module is item ordering only and does not affect this assessment. Full final
Clippy remains main's responsibility. No remaining actionable concern from this
reviewer's findings; previously stated live-provider/VM and broader evidence limits
remain in force.
