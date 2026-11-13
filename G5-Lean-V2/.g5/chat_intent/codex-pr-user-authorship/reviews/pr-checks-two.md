# PR checks independent review two

## Scope and method

Independent, read-only full-scope discovery of the working-tree delta above
`fa1685cca72ebc548ab4821db95fd60ab3fe72ba`, including the new untracked
`backend/g5-api-core/src/services/workspace_pull_request_checks.rs`. Excluded the
unrelated `docs/ui-workspace-agent-parity-audit.md`. No author or peer review report
was read before saving these findings. No production, test or ontology file was edited.

Read AGENTS.md, the ontology-review skill and its design guide, the original CI-loop
request, the CURRENT first amendment and its exact ontology diff/governance table,
all new commitment bodies/metadata, and applicable read/security, authority, design
and quality commitments/metadata. Followed both route consumers, shared selector,
credential owner, PR provider selection, configuration secret resolution, CLI parser,
transport/exit shell, JSON projector and worker/user documentation. The new ontology
blocks match the amendment; their unimplemented footer assessments are still pending
this review's consolidation, rather than evidence of the final implementation.

Main owns the full test-suite run. This reviewer compiled the unchanged provider
observation/helper section (from `fn valid_sha` onward) in isolated scratch, substituting
only its enclosing query/error/header definitions, and exercised its real reqwest
requests against a localhost fake GitHub server. This is provider-adapter runtime
evidence, not API/DB admission evidence. Also exercised the existing compiled `g5d`
process against a separate localhost fake API. No shared database or cargo output was
modified. The first scratch attempt needed the matching Tokio artifact; the CLI needed
its installed libkrun search path. Successful receipts below supersede those setup
failures. No live GitHub or deployed-VM claim is made.

Scratch receipts: `/private/tmp/g5-pr-checks-two-probe-7sk36duq/` contains extracted
`probe.rs`, executable, `protocol.py`, `results.json`, `cli.py`, `cli-result.json` and
a copied CLI binary. The API/DB integration suite was inspected but not run by this reviewer.

## Initial findings

### R2-1 — P2: a re-requested check can report the previous success while its suite is queued

**Locations:** `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:288`
through 295, 327–340, and 374–383.

The reader only fetches `check-runs?filter=latest`, normalizes each run's own status,
and revalidates the PR head SHA. GitHub's check-rerequest protocol resets the associated
check suite to queued and clears its conclusion while leaving the existing check run
unchanged. Therefore a stable-head PR with a successful check that has just been
re-requested is still returned as `availability=ready`, `bucket=pass`; `g5d pr checks
--watch` exits 0 before the rerun completes. A head-only recheck also does not detect a
new run replacing a passing result during the remaining enumeration.

**Governing:** `g5.governance.pr.checks.revision`, current amendment's explicit
"A head change or rerun during watching invalidates the corresponding earlier result"
and rerun-identity acceptance; `g5.data.outcome-evidence`.

**Evidence:** GitHub documents the unchanged run/queued suite behavior in
[the rerequest endpoint](https://docs.github.com/en/rest/checks/runs#rerequest-a-check-run).
The scratch `rerun` fixture exposes that provider state and receives a ready/pass
observation; the request receipt proves no suite or rerun-state query occurs. This
uses the actual observation code. The compiled CLI's exit decision treats that single
pass as success by inspection. The supplied API tests cover a changed SHA but do not
cover this same-SHA rerun state.

**Proposed fix:** account for the selected runs' current suite/rerun state within the
provider owner and invalidate superseded attempts before terminal output. Keep this
bounded and read-only. Do not treat an unchanged SHA as proof that the run attempt is
unchanged.

**Discriminating regression:** serve a stable PR head, an old successful check run and
its suite queued with no conclusion after rerequest; the real public/VM operation and
CLI watch must remain pending. Then complete the rerun and verify terminal output
uses it. Add a same-head transition during enumeration, preserving legitimate stable
completed runs and identically named checks from distinct provider identities.

### R2-2 — P2: annotation retrieval failure discards diagnostics already returned by GitHub

**Locations:** `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:417`
through 429 and 344–350.

`diagnostics` fetches annotations with `await?` before assembling the available
`output.summary` and `output.text`. A transient 503, rate limit, interrupted body or
annotation response limit therefore discards the already retrieved compiler/test
failure text. The caller substitutes only an unavailable message. This prevents the
VM repair loop from using evidence that GitHub successfully supplied and misleadingly
makes all diagnostics look unavailable because one optional component failed.

**Governing:** `g5.governance.pr.checks.details` requires supplied GitHub failure
summaries/text with explicit missing or truncated diagnostics; `g5.data.outcome-evidence`
requires separate evidence for separate outcomes.

**Evidence:** scratch `annotations` case supplies summary `compiler error E0308;
expected bool` and text `src/main.rs:12 type mismatch`, with one annotation and a 503
on the annotation endpoint. Actual observation output has no summary/text in details,
only the 503 message. The title remains separately in `description`; it does not
preserve the actionable failure text. Existing tests cover successful annotation
retrieval and truncation but not this partial-error path.

**Proposed fix:** retain and redact/bound the already supplied title/summary/text,
separately flag annotation unavailability/truncation and include the recovery message.
Rate limits should additionally follow R2-3's stop behavior. A partial annotation error
must not erase other diagnostic evidence.

**Discriminating regression:** return meaningful output plus positive annotation count,
then make the annotation endpoint fail; assert the supplied summary/text survive,
annotation completeness is explicitly false/unavailable, secrets remain redacted and
the original failing check remains failed.

### R2-3 — P2: a diagnostic rate limit neither stops provider requests nor CLI polling

**Locations:** `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:174`
through 182, 344–350, 374–375 and 140–155;
`backend/g5d-cli/src/pr.rs:139` through 152 and 553–568.

The provider adapter collapses rate limits into an ordinary string error. When an
annotation returns HTTP 429 with `Retry-After: 60`, `observe` records the error only in
`check.details` and immediately calls GitHub again to re-read the head. The repository
remains `availability=ready`. With another pending check, the CLI only inspects the
repository availability/buckets, so `--watch --details` repeats at `--interval`, ignoring
the rate-limit error it just printed. Other per-repository rate-limit errors also leave
the outer loop free to call the remaining repositories immediately.

**Governing:** `g5.architecture.pr_checks` explicitly requires honoring provider rate
limits; the accepted interface says errors stop with actionable messages; current docs
say rate-limit errors stop polling. GitHub's
[rate-limit guidance](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api#handle-rate-limit-errors-appropriately)
requires stopping requests until the indicated wait/reset time.

**Evidence:** scratch `rate_limit` response includes the real adapter's ready repository,
failing diagnostic with Retry-After 60 and a pending status. Its request receipt includes
an immediate PR-head GET after the 429. Feeding exactly that observation to the compiled
CLI with `--watch --interval 1 --details --json name,details` yields two API requests
**1.004 seconds apart**; the fake ends the second poll deliberately. `cli-result.json`
records exit 1, two reads and the printed 60-second retry direction. This runtime result
discriminates the bug from ordinary failure-plus-pending watching behavior.

**Proposed fix:** preserve a typed/routable rate-limit outcome through provider reads,
stop further provider calls for that rate-limited read and make the CLI stop (as the
accepted design specifies) or honor an explicit provider delay if the design is amended.
Keep completed rows/available diagnostic text where possible, with explicit incomplete
or unavailable observations. Do not infer that a nested diagnostic error is harmless
for polling.

**Discriminating regression:** annotation 429/Retry-After on a failed check alongside a
pending check; assert no subsequent provider calls and no second CLI API poll. Cover
primary-limit 403 with remaining=0/reset as well as 429 and multi-repository fanout.
A normal failed check plus a pending check should still watch unless fail-fast is set.

## Commitment and boundary ledger

| Commitment | Actual path and counterexample/check | Assessment |
|---|---|---|
| `g5.governance.pr.checks` | Shared `read_checks_inner`, selected provider rows, checks/status pages; missing publication sentinel, missing credentials, empty observations, mixed repository outcomes; CLI outcome computed before JSON projection | Mechanism inspected; supplied API/CLI test cases cover principal allowed/refused shapes. R2-1 is a false-terminal counterexample. Successful multi-page enumeration and the documented GitHub 1000-suite hidden limit lack discriminating coverage. |
| `g5.governance.pr.checks.revision` | Provider head read before and after enumeration, each run's head validated, fixed expected SHA, watch re-requests operation | Changed-head/mismatch protection inspected and tests supplied. Same-head rerequest is violated by R2-1. No deployed-provider or synthetic-merge revision evidence. |
| `g5.governance.pr.checks.details` | Scoped annotation URL constructed from returned run ID, safe output links, configured/token redaction before detail truncation, bounded pages/body/text | Successful/absent/truncated paths inspected; external destinations never fetched. R2-2 loses available output on annotation failure. Runtime scratch confirms that defect. |
| `g5.governance.pr.read`, `.json` | Existing attached-repository selector extracted into one helper; both API routes share reader; CLI checks field inventory/projection | Attached/ambiguous repository and no-context field discovery preserved by inspection; existing/new tests inspected. G5 UUID selection stays within workspace PR listing. No arbitrary endpoint URL introduced. |
| `g5.security.read`, `.tenant`, `.tenant.rls`, `.admission` | Public `scoped_db_for_workspace`; VM `scoped_db_for_workspace_incarnation`/`require_live_workspace_incarnation`; scoped provider selection and credential membership/URL validation | Correct owners reused by inspection. Current-incarnation check happens each API poll; no new live-mutation/start requirement for human durable reads. Supplied test rejects outsider/stale epoch. Other revocation timings and full API/DB run not executed by this reviewer. |
| `g5.security.credentials.publication`, `.credentials.redaction` | Repository operation adds only Pull requests/Checks/statuses read capability; App binding coverage/permissions owner retained; effective configured secrets plus active token redacted | No token projection or bot fallback added; principal secret redaction test inspected. Narrow App access evidenced in fixture assertions, not live installation. Existing personal-token path unchanged but a checks-specific personal-token runtime case is absent. |
| `g5.governance.pr.write`, `.follow-push`, `.publish.readiness` | Checks branches before read formatting; read-only missing-provider discovery/listing, then DB rollback and GET-only provider adapter except credential minting | No save/push/publication/retry effects introduced by inspection; existing write owner untouched. New test observes zero PR creation and only GET/token-mint requests. Broader lifecycle state unchanged claim not separately executed here. |
| `g5.governance.approval`, `.approval.version`, `.merge` | Reader has no approval/merge calls; output labels observed checks | Preserved by dependency/source inspection; no merge-authority claim or write. Existing version-bound merge code is outside this delta. |
| `g5.data.git-authority`, `.platform-authority.vm-access`, `.outcome-evidence` | Live provider SHA/check observations come through authorized API, not G5 preflight or VM direct DB; no durable CI state | Ownership preserved except R2-1's superseded-run terminal claim and R2-2's lossy diagnostic outcome. |
| `g5.architecture.pr_checks` | One provider/read operation, thin public/internal adapters, bounded HTTP/enumeration and CLI polling | Scope is minimal; no new database columns/queue/monitor. R2-3 violates rate-limit behavior. The duplicate outer 120-second timeout is unnecessary but harmless, not a blocking finding. |
| `g5.design.reuse`, `.minimal-mechanism`, `.adapters`, `.authority`, `g5.architecture.policy` | Reuses selector, credential/config owner, user/VM admission, existing JSON parsing, documentation owner | No unjustified architecture found. Domain provider normalization stays in the API owner; CLI owns documented exit/poll presentation. Required repairs can stay inside those owners. |
| `g5.quality.review`, `.review.cases`, `.review.producer`, `.evidence`, `.evidence.fakes`, `.evidence.unknown` | Entire delta and affected consumers inspected; isolated provider-protocol and compiled CLI counterexamples run | Raw findings saved before sharing. Full API/DB and unit suite delegated to main by assignment, not claimed passed. Live GitHub/deployed VM remain unverified. |

## Remaining verification and proposed ontology follow-up

Main should update the four new metadata footers only after consolidation/repairs and
checks, preserving remaining live-provider/VM limits and recording these findings'
dispositions. The current body commitments already govern the defects; no weakening
or new policy block is needed. Preserve the original design handoff and append only
consequential implementation decisions. The saved amendment's open assumptions about
rerun identity and deployed permissions are still real evidence gaps until addressed.

Additional supplied-suite limits: no checks-specific personal credential test, successful
pagination test, HTTP-body limit test, cancellation execution, same-name distinct Apps,
or real synthetic-merge/head-provider case was found in the added tests. These are
verification gaps rather than separate proven defects. The three findings above have
concrete counterexamples and targeted repairs, independent of full-suite pass/fail.
