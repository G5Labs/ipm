# First repair pass: observed failures and limits

All three fresh Sol fixers received the identical 105-claim/19-group packet and original
source, plan, ontology and revised harness. All saved initial dispositions before any
application edit. P1 took about22–25 minutes per fixer, below the30-minute allowance.
All first patches, reports and commits are preserved. Original governing text was unchanged;
ontology edits were implementation metadata only.

| Outcome | B1 | B2 | B3 |
|---|---|---|---|
| Accepted launch survives changed defaults | Pass | **Fails: rejects still-valid accepted launch** | Pass |
| Future shutdown warning allows fresh retry | Pass | Pass | Pass |
| Job cap, saved replay, current receipt-read authority | Pass at tested boundaries | Pass | Pass |
| Complete retry allocation owner | Source-supported | **Incomplete: session composition stays in JobService** | Source-supported |
| Stopped-page recovery visibility | Source-supported | Source-supported | Source-supported |
| Synthetic selected-target progress | Pass | Pass | Pass |
| Actual ordinary sync producer retains retry scope | **Fails** | **Fails** | **Fails** |
| Four newly measured cases: jobless success, distinct later session task, zero spend threshold, policy admission | All pass | All pass | All pass |
| Explicit Stop marker blocks fresh allocation | Pass | **New regression** | **New regression** |
| Expired shutdown warning blocks allocation | Pass | Pass | **New regression** |
| Recorded usage on failed tasks blocks exhausted budget | **Fails** | Pass | **Fails** |

Passing rows are specific local evidence, not overall alignment or production readiness.
The public producer/progress extension failed on all three after successful ordinary creation and retry, with missing direction/scope in the progress response. The required migration catalog check passed B1/B3 and failed B2 because its new migration lacked a manifest entry.

Independent source evaluation also finds actual progress/producer gaps, inherited attachment
and policy-mapping concerns, ordinary producer ownership debt, and conditional receipt/version
compatibility concerns. These additions are separate from supplied findings and must be
applied consistently, without changing frozen obligation denominators.

## Why the repairs differ

**B2 accepted launch:** the fixer saved the resolved choice, then compared it against freshly
resolved defaults. Its own unit test treats rejecting drift as success. The independent
check requires executing the accepted still-authorized choice. The consolidation packet
explicitly warned that refusal on changed defaults was not by itself a correct repair.
This is an incorrect repair/verification criterion after the issue was supplied.

**B2/B3 shutdown admission:** they made the reported future exception pass by weakening the
stop predicate. Their fixtures covered future warning and stopped/stopping VM status, but
missed an explicit Stop identity while the VM still reports running. B3 also missed a past
warning deadline before janitor transition. Independent persisted-state/API cases expose the
regressions. This is an incomplete repair of neighboring allowed/refused states, not failure
to retrieve the supplied exception.

**B1/B3 spending:** both wired a spending guard. Its inherited aggregation omits failed-task
consumption; B3 additionally fixed jobless membership but kept the outcome filter. A zero
threshold passes while a real recorded exhausted budget does not. B1 explicitly left spending
verification incomplete; it did not falsely claim that entire outcome verified. B3 claimed a
repair with narrower running-task/threshold evidence. B2 used recorded workspace usage and
passes the failed-task case, with unknown usage and concurrency still limited.

**Progress:** the synthetic selected-target probe supplied task file-path metadata, so all
passed. The ordinary sync producer creates one agent covering multiple targets without that
metadata. Independent source tracing identified the missing path; a separately labelled
posthoc public-producer/progress probe tests it on every candidate. This also limits the
initial evaluator probe: passing an artificial data shape was insufficient to close K7.

The fresh B2 closure reviewer also caught a specific ownership omission after its blind review had found no issue: TaskService writes task/input but JobService still creates the session separately. The initial Astra K4 verdict was too narrow and is corrected in the final adjudication; original assessment is preserved.

## Evaluation controls

The original13 backend checks were run only on these new candidates: B1 passed13,
B2 passed11, B3 passed12. All passed the independent recovery component, TypeScript and their
core unit suites (983/985/984 tests respectively). These totals hide distinct obligations;
the per-case matrix governs conclusions.

The receipt-authority extension initially failed on B1 because the evaluator produced an
invalid stopped-VM row. Its fixture was corrected (including removing inherited organization
admin authority for the revoked-read case); the corrected test passed on every P1. Original
failure, corrected bytes and amendment are retained. No product failure is inferred from it.

The recorded-failed-usage and public-producer progress checks were added after P1 inspection,
not presented as pre-registered. They are applied to every new candidate and later checkpoint.
No hidden evaluation output enters Sol loop reviews or repairs. Provider/VM/browser execution,
all policy variants and exhaustive concurrent schedules remain unverified.
