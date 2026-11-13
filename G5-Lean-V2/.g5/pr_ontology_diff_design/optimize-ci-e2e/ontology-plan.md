# Shorten g5-ci-e2e without reducing coverage (issue #773)

Source: the owner's [instruction to follow the CI E2E investigation](../../chat_intent/optimize-ci-e2e/request.md)
(2026-10-23). The investigation and the open questions are the workspace's Project Docs
`g5-ci-e2e-investigation.md` and `open-questions.md`; no question has an owner decision.

A typical green g5-ci-e2e build now takes about 50 minutes. Browser shard 2 runs all
four workspace-review suites and finishes about 24 minutes after shard 1. Behind the
browser tail, the Rust E2E → value-loop chain runs the formal-review scenario twice.
This change rebalances the shards from measured durations, gives that scenario one
execution owner, and adds the timing evidence the investigation asks for before fixture reuse is considered.

## Ontology changes

All in [operations](../../ontology/features/operations.g5.md#g5.quality.delivery.ci.e2e-runtime), new section "CI E2E runtime":

| Block | Change | Why |
|---|---|---|
| `g5.quality.delivery.ci.e2e-runtime` (new I) | Shorten the check without reducing coverage: the same tests, assertions and receipts, and per-test fixture state. 35–40 min is a direction, not a gate. | Issue #773 and the investigation; there was no governing CI E2E intent beyond the image key. |
| `g5.quality.delivery.ci.browser-shards` (new D) | Whole-file placement on 2 shards × 2 workers from committed measured durations, replaying Playwright's dispatch. The exact test-ID partition is verified before and after execution. | Recommendation 1. |
| `g5.quality.delivery.ci.single-execution-owner` (new D) | The broad Rust lane skips a test only by exact name, and only when a Rust receipt selector owns it; a check enforces this. M6 and RV7. | The duplicated formal-review finding. |
| `g5.quality.delivery.ci.timing-evidence` (new D) | Per-file first-attempt and retry time vs prediction; per-test phase timings; marker wait time. Evidence only. | Recommendations 2 and 4; the validation section. |
| `g5.quality.delivery.ci.provider-failure-bound` (new D) | An assistant-reply wait ends on a confirmed failed turn (error status plus terminal task), not at the success timeout. | Recommendation 4. |
| `g5.quality.delivery.ci.release-overlap` (new D) | The release compile keeps waiting for frontend tests: the canaried overlap gained nothing under CPU contention. | The startup-overlap finding and its canary. |

`g5.quality.delivery.ci.image-key` is unchanged. Its metadata notes the proposed layer
split as separate work.

## Governing ontology

- [g5.quality.delivery.ci.image-key](../../ontology/features/operations.g5.md#g5.quality.delivery.ci.image-key):
  untouched. Gates still run against the candidate checkout.
- `docs/ci-e2e-scope.md` (required coverage) and `docs/review-flow.md` (RV1–RV10 receipt, the real
  invitation flow per fixture) are supporting design. Neither changes: every required test and
  receipt selector still runs, and each review test still invites, signs in and builds its own
  workspaces.

## Measured basis

These are median per-file durations from Playwright's per-shard reports over eight green
builds on 2026-10-23: `058cbb2f`, `87b25522`, `7bc8ff84`, `aa67b7dc`, `e8e49916`, `5d4c3a6e`,
`8c748b82` and `4ef1a989`. The broad lane holds 88.5 aggregate minutes. Four files hold
51.6 of them: journey 18.9, content 15.4, recovery 8.8 and usability 8.5.

- **Playwright's own order.** In Playwright 1.58, `--test-list` is only a filter. Files run in
  directory-sorted discovery order, and each goes to the first free worker. All four review files
  sort last.
- **The model matches today's builds.** Replaying today's placement predicts the slower shard
  within 0.2 min of the observed time, for example 35.6 predicted vs 35.8 observed in `7bc8ff84`.
- **The planned partition.** Journey and content go on one shard with little before them;
  recovery, usability and the next-heaviest files go on the other. Predicted slower shard: 22.7
  min on the medians and 22.4–25.4 min on each build's own durations, instead of 34.7–37.4.
- **Four explicit worker lists** would predict 22.1 min. Rejected: about half a minute is not
  worth a per-worker process, report and receipt input.

With the browser tail around 23–25 min, the Rust chain matters next. In `058cbb2f`, value-loop
ended at minute 39.9: broad `rust-e2e` took 11.9 min and value-loop 14.7 min. The duplicated
formal-review scenario costs 4.5–7.5 min of that chain.

## Design

### Shard planner (`scripts/plan_playwright_shards.py`)

**Inputs.** The two original `--shard=k/2 --list` inventories, unchanged: their union is the
inventory to partition, and their concatenated order is Playwright's discovery order. A new
`--durations infra/ci/playwright-file-durations.json` supplies per-file weights in ms.

**Search.** For the heaviest files (at most 12), every assignment to the two shards is tried.
The rest are then placed greedily, heaviest first, wherever the predicted shard makespan grows
least. The prediction replays the shard's files in discovery order onto `--workers` (2) workers,
each file to the earliest-free worker. The lowest predicted slower shard wins; ties are broken
deterministically.

**Output.** `plan.json` gains the predictions and the list of unmeasured files. The
partition proof is unchanged: duplicate files or IDs across the originals are refused, and
`verify` compares planned file and ID sets against discovery and, later, against both executed
reports.

**Refreshing durations.** `plan_playwright_shards.py durations --report …` rebuilds the table:
the per-file median of the first-attempt duration across the given reports. Unmeasured files
weigh the measured mean, so a new heavy file is misplaced until its measurement is committed. The
plan output and the shard summaries (predicted vs observed) make that drift visible. Whether
placement must adapt automatically is open question Q11.

### One execution owner for receipt-owned Rust tests

`run-rust-e2e-lanes.sh` keeps the `formal_agent_approvals` module and skips only
`explicit_formal_reviews_keep_verdict_and_evidence_after_isolated_resources_disappear`, beside the
existing M6 skip. Both skips move into one receipt-owned list.

`test_rust_e2e_lanes.py` checks every name in that list against `tests/e2e/value_loop_matrix.tsv`:
a selector with that exact name must exist and be routed to a Rust receipt profile. A skip
without an owner fails the script tests, which run in the value-loop step and in
`run-ci-script-tests.py`.

`finalize --fill-missing` already fails `VALUE_LOOP_MATRIX` when RV7 is not run. The broad
group then runs zero tests, which passes; future tests in the module still run there.

### Timing evidence

- **One annotation per test.** `frontend/e2e/timings.ts` keeps a single `g5-timings` annotation
  on the current test result and updates it in place (`{phase: {ms, count}}`). Playwright reports
  its final state per attempt; this was checked locally with Playwright 1.58.2. Recording outside
  a test is a no-op.
- **What is recorded.** The review fixture times its setup phases, the body (`provide`) and
  cleanup. `write`, `commit` (live-checkout lookup), `shell` (open terminal; run command) and
  `push` time themselves. `waitForWorkspaceVmStatus` records `vm.capacity-wait`, the time it saw
  `queued_for_capacity`, and names that time and the last status in its timeout error.
- **Per-shard summary.** `report-shard-durations.js` prints per-file first-attempt and retry time,
  the planner's prediction, and per-file phase totals. It writes the same data as JSON for the
  exported artifacts. `ci-progress-reporter.mjs` emits the annotation in `G5_TEST_END`.
- **Marker waits.** `ci-wait-markers.sh` prints how long it waited, so the provider-agent lane's
  receipt waiting is separated from its own execution.

### RV9 terminal reuse

Investigation: "retain an authenticated terminal session through the loop and reuse stable
preparation where valid."

`shell` skips its navigation when the page is already on this workspace's terminal view and the
terminal reads exactly "Connected". Otherwise it navigates, and reopens an ended session, exactly
as before. It still types `stty -echo`, the command and the completion token each call.

Each of RV9's 26 revisions still writes, commits through the real terminal, makes its own
coordinated push and reads the complete head vector. The pagination, unchanged-member and visitor
checks are untouched.

### Provider failure bound

`expectAssistantReply(page, turn, text, {timeout})` replaces the bare
`toContainText(successText)` waits in `agent-value-outcome`, `question-store` and
`native-requests`. It polls the last assistant message. On the success text it returns. On an
error status block it reads the turn's task (`/tasks/{taskId}`; the session's task route resolves
only live tasks) and fails at once if it is failed or cancelled, quoting the block's text. It keeps
waiting while the task is active. A page-only spec, `assistant-reply-wait.spec.ts`, covers the
decision logic. Otherwise the
timeout and its final assertion are as before.

### Release overlap (canaried, rejected)

The first PR build ran with `frontend-test` dropped from the `rust-release-artifacts` `waitFor`.
The release compile started about 2.3 minutes earlier, but CPU contention with the frontend tests
stretched it from about 2 to 5 minutes. It finished no earlier, and workers started later (see
`g5.quality.delivery.ci.release-overlap`). The edge is restored. The lane-isolation contract keeps
the added check that `frontend-test` still gates the browser fixtures and `assert-ci-status`.

### Risks the canaries must show

- **VM slots.** The four review files now run at the same time, two per shard. The broad host
  has 8 microVM slots for the running seed plus 4 workers. Most review cases hold one running VM
  at a time, but some briefly hold two; content resumes the parent next to its running child, for
  example. A short capacity queue is possible, and `vm.capacity-wait` reports it. Observed in
  `c3dcf693`: one `project-doc-creation` attempt waited 93 s, timed out and passed on retry.
  Raising host capacity is outside this change. In `73d21f25` three journey fixtures waited
  32–93 s. That build's only failure was the known transient `materialize workspace repository
  vector` boot 503, seen on other PRs since 2026-10-19, which a first-attempt-only receipt cannot
  absorb.
- **Measured durations.** These were measured with the review files on one shard. If running all
  four at once slows them, the prediction is optimistic; the shard summaries show it.

## Not in this change

- **Review-fixture reuse** (shared actors, or a prepared starting point per worker): the
  investigation gates it on the phase measurements added here. Q12 and Q13 stay open.
- **RV9 through an authenticated fixture command** (no terminal UI): it waits for the phase
  measurements. Q14 stays open.
- **CI image layer split and registry caches:** "retain the image-layering work independently";
  it amends `ci.image-key` and spans both pipelines. Q3 and Q16 stay open.
- **More workers, an isolated review lane, `fullyParallel`:** "only then evaluate". Q7 and Q10
  stay open.
- **Retry policy and capacity-wait behavior:** unchanged, now reported. Q20 and Q21 stay open.

## Verification

Local (no KVM here, so no workspace VMs):

- Planner and receipt-ownership unit tests: `scripts/tests/test_plan_playwright_shards.py` and
  `scripts/test_rust_e2e_lanes.py`.
- The Cloud Build contract tests (`scripts/test_cloudbuild_waitfor_order.py`, `run-ci-script-tests.py`).
- The planner on the real Playwright discovery of this checkout.
- A local Playwright run of the timing annotation, the reporter and the summary script.
- Frontend lint and typecheck of the changed E2E code.

On the PR (the canaries):

- Both shards' file/ID verification and every receipt.
- Shard durations against the prediction.
- RV7 executed once, in the receipt lane.
- The total build time, compared with the 50.5–52.6 min baseline above.

The review suites, RV9 and the provider lane run only there.

## Implementation record (2026-10-23)

Implemented as designed on `optimize-ci-e2e`. The release overlap was canaried and reverted
(see "Release overlap"). Another amendment: the planner's replay is approximate for a
file with its own worker-scoped options, which Playwright dispatches after the others (see
`g5.quality.delivery.ci.browser-shards`).

**Passed locally:**

- Planner, summary and refresh tests (`scripts.tests.test_plan_playwright_shards`, 11).
- Receipt, shard-result, lane-selection and mirror tests (the value-loop step's unit list, 50 in
  total).
- Cloud Build ordering contracts (`test_cloudbuild_waitfor_order`, 30).
- `g5-e2e` `ci_e2e_lane_isolation` (4) and `value_loop_matrix` (1). Mutations confirm that
  restoring the release edge, or dropping `frontend-test` from `assert-ci-status`, fails them.
- The planner on real `--shard` discovery: 61 files, predicted 22.5/22.5 min.
- A Playwright 1.58.2 run of the real timing recorder and CI reporter.
- A typecheck of the changed E2E files.

**Script regression suite:** `run-ci-script-tests.py` passes except six suites that need a g5d
binary, a Docker/KVM guest or extra arguments. They are unrelated to this change, and the three
Python ones fail identically on the base commit.

**Fresh-context review:** two defects in new code were fixed with regression cases. A summary
included the setup project, which the planner's table check would then reject, and a timing copied
from a `beforeAll` hook could be double-counted. The worker-hash ordering limit is documented.

**First canary**, g5-ci-e2e `c3dcf693`: green in 40.8 min (baseline 50.5–52.6). Shards ran
24.9/25.1 min against 22.9 predicted. Broad `rust-e2e` ended at minute 16.8 instead of about 27, and
value-loop at 31.0. The release overlap was rejected on this build's evidence.
