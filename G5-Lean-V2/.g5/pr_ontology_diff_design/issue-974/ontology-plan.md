# Approval experience: authoritative requirement status (#974)

Request: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), written by the
owner with an [interactive mock](https://am-8fa06277afacbf9a6e41a808.hariharan-rahul.chatgpt.site/)
and its [change report](https://am-8fa06277afacbf9a6e41a808.hariharan-rahul.chatgpt.site/report.html).
The issue body carries the full report and is the normative source; this plan records
only the owners, choices and assumptions needed to keep the design coherent.

## Governing blocks

New:

- [`g5.governance.approval.status`](../../ontology/features/reviews.g5.md#g5.governance.approval.status):
  one overall requirement status, one evaluator and one response.
- [`g5.governance.approval.source-commit-policy`](../../ontology/features/reviews.g5.md#g5.governance.approval.source-commit-policy):
  `on_source_commit_vector_change`, `IGNORE` selected.
- [`g5.governance.approval.request-pr`](../../ontology/features/reviews.g5.md#g5.governance.approval.request-pr):
  one `PR_REQUIRED` request rule, `false` selected.

Amended: approval [request](../../ontology/features/reviews.g5.md#g5.governance.approval.request),
[request status](../../ontology/features/reviews.g5.md#g5.governance.approval.request-status),
[version](../../ontology/features/reviews.g5.md#g5.governance.approval.version),
[selection](../../ontology/features/reviews.g5.md#g5.governance.approval.selection),
[composer](../../ontology/features/reviews.g5.md#g5.governance.approval.request-composer),
[overview](../../ontology/features/reviews.g5.md#g5.governance.approval.overview),
[timeline](../../ontology/features/reviews.g5.md#g5.governance.approval.timeline),
[evidence](../../ontology/features/reviews.g5.md#g5.governance.approval.evidence),
[human authority](../../ontology/features/workspaces.g5.md#g5.workspace.approval-human-authority),
[cut eligibility](../../ontology/features/workspaces.g5.md#g5.workspace.approval-cut-eligibility),
agent [assignment](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.assignment)
and [evidence](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.evidence),
and UI [Home workspaces](../../ontology/features/ui.g5.md#g5.ui.home-workspaces),
[landing summary](../../ontology/features/ui.g5.md#g5.workbench.landing.summary),
[Ship to parent](../../ontology/features/ui.g5.md#g5.workbench.ship-to-parent) and
[approval readers](../../ontology/features/ui.g5.md#g5.workbench.approval-readers).

## Owners and interactions

| Concern | Owner | Consumers |
|---|---|---|
| Counted evidence, overall status, currentness, path to approval, request needs | the requirement evaluator in `approval_config_service.rs` | the workspace approvals projection and merge admission |
| Source-commit policy | one constant read only by the evaluator's evidence selector | none directly |
| Request PR rule | one request-policy value read by human request admission, human decision admission and agent review admission | the approvals response (`requestPolicy`) for display only |
| Agent summary | the schema-5 report contract (`approval_artifact.rs`) and its confined writer | the formal store, which copies the exact value into the decision |
| Presentation | the frontend | Home cell, landing summary, Approvals page, request dialog, Prepare to Ship, Activity |

**Choice: extend the existing response.** `GET /workspaces/:id/approvals` already returns
the alphabetical required list with a server-computed `state` and the merge admission
that Prepare to Ship reads. It gains `currentness`, `approvalPath`, `requestNeeds` and
`requestPolicy`. Home, landing and Prepare to Ship stop tallying reviewer rows. A second
endpoint would duplicate the evaluator's inputs without adding a guarantee. The Home
cell keeps its existing per-workspace read; its batching is unchanged.

**Choice: backend request needs.** The rules for which reviewers Request all must
address depend on quorum, veto, override and evidence. They are therefore computed by
the evaluator, not by `approvalSelection.ts`. The client keeps mode and checkbox state,
and resolves the one random human from the server's candidate list before showing the
fixed selection (as the [selection](../../ontology/features/reviews.g5.md#g5.governance.approval.selection)
rule already requires).

**Choice: the merge reason is the ship copy.** When requirements are unsatisfied, the
admission `reason` becomes exactly `Approval(s) required before merging`. Requirement
names remain in the admission's requirement list. This keeps one blocker string for
the merge refusal and section ④.

## Evaluator rules

Counted evidence for a slot (agent, or one configured human):

- Take the latest decision at a definition version at or above the invalidation boundary.
- Human evidence must also be request-addressed: its request was bound to a PR (open
  or not), or explicitly made with no PR. PR lifecycle and `PR_REQUIRED` never retire
  settled evidence ([#1010](#follow-up-settled-decisions-survive-pr-closure-1010)).
  Legacy evidence with no request, or on a legacy request recording neither, stays
  historical.
- Under `IGNORE` the reviewed source vector does not filter evidence. Under
  `MAKE_PENDING` it must equal the current vector, which is today's rule.
- A latest `deferred` (withdrawn) decision counts as no verdict.

Overall status reuses the existing quorum (`requirement_state`): a human rejection
vetoes; Any-of needs one yes; a human yes overrides an agent no when the definition
allows it; All needs every eligible approver.

Currentness compares each counted approved/rejected decision's vector with the current
source vector. The aggregate is `matching`, or `changed` naming the reviewers whose
evidence differs, or `unknown` when a vector is unreadable, or `none` without evidence.
It never changes status.

Path to approval is emitted only for Rejected, or for Pending after a settled response:

- `humansRejected`: rejecting humans, and whether All also needs other humans.
- `agentRejected`: whether human override is allowed.
- `needed`: the remaining approvers for All, or the eligible pool for Any-of.

Request needs:

- An active request is a pending agent request, or a pending human request that is
  active as above.
- **Any-of:**
  - Re-request rejecting humans.
  - If the agent rejected without override, request the agent.
  - If the agent rejected with override, choose a random eligible human when no human
    was ever requested; otherwise request the agent.
  - With no rejection and no active request, request the agent if one is configured,
    else a random eligible human.
- **All:** every eligible approver whose counted decision is not approved.
- Slots with active requests are excluded throughout.
- `repeat` applies the same rule as if the requirement were unsatisfied, for explicit
  inclusion of satisfied requirements.
- Agents only uses `agentRequestable` (agent configured, with no active agent request).

## Assumptions

1. **Settled evidence is not PR-scoped.** Like agent evidence, a human decision counts
   until a later decision, withdrawal or definition-version invalidation replaces it,
   or, under `MAKE_PENDING`, until the source vector moves. This holds whether its
   request was PR-bound or made with no PR. Closing or merging a PR does
   not retire it; the PR binding is provenance. PR lifecycle scopes only pending
   requests: with `PR_REQUIRED = true`, new requests and decisions require an open PR,
   and no-PR requests cannot be decided. (Amended by #1010; #974 originally scoped
   PR-bound evidence to the open PR.)
2. **Agent requests are not bound to a PR.** They keep no PR binding; only the same
   admission gate applies to them.
3. **Override merges bypass no-PR requests.** A waived merge closes the source's
   pending no-PR human requests as Bypassed, like PR-bound ones.
4. **Legacy agent decisions have no summary.** Schema 1–4 decisions expose no
   `summary`, because their stored text is a body clip. Compact surfaces show no
   excerpt for them, and the report remains readable.
5. **Schema 5 needs new module identities.** It gets new Claude and Codex formal module
   identities under the existing [rollout](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.contract-rollout).
   Old schemas keep theirs for frozen retries.

## Verification

The issue's acceptance matrix is the test plan:

- Backend evaluator unit tests for every row of the status/path/needs rules, for both
  commit policies.
- Real HTTP/Postgres tests: status after a source move and a multi-repository partial
  move; human supersede; veto; withdrawal; version invalidation; the four
  `PR_REQUIRED` × kind combinations; a decision on a no-PR request after a PR opens;
  duplicate prevention; and the merge gate agreeing with the projection.
- Contract tests for the schema-5 summary: accepted, blank, more than 250 characters,
  forbidden on older schemas, and historical schemas still valid.
- Frontend unit/component tests for the Home cell, landing text, roster/timeline/popup
  behaviour, dialog modes and prefill, and the Prepare to Ship schedule with fake timers.
- A browser check of the changed surfaces across themes in light and dark.

## Implementation and evidence (2026-10-30)

Implemented on this branch. Rollout identities for the agent summary are recorded under
[contract rollout](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.contract-rollout).
The agent summary is one line of at most 250 characters, enforced by the confined writer
and validator. The PR rule and source-commit policy reach request admission, human
decisions, agent admission, the projection, pull-request merge admission and direct
merges through `AppConfig.approval_policy`. The CLI always selects the constants; tests
inject the alternatives.

Local checks:

- Evaluator unit tests and the reworked quorum test (`g5-api-core`); clippy `-D warnings`
  on the touched crates.
- Postgres integration suites for approvals, agent evaluation, review flow, reviews, Home,
  policy locks, formal agent approvals and their acceptance and inspection suites,
  Activity, pull requests, merge override, auto-merge, provider targets and org
  approvals. All pass after the old PR-gate and stale-approval expectations were updated
  to the new rules. The stale direct-merge test now runs under MAKE_PENDING.
- The full frontend suite: 355 files, 5,502 tests. Build typecheck clean; eslint 0 errors.
- A browser pass in mock mode with the mock's four-requirement workspace. All 12 themes
  were checked in light and dark at 1440 and 1024 px, including popup placement and
  dismissal. Contrast of status badges and icons was at least 9.3:1.

Not yet run: real native Claude/Codex formal reviews on the new modules, new-head Cloud CI
and a deployed QA check.

Deviations from the mock or report, kept deliberately:

1. **Narrow width.** Below 1024 px the product shows its existing chat-only mobile layout,
   so 1024 px stood in for the narrow case.
2. **All-of pending sentence.** It reads "Approval from X will meet this requirement. All
   configured humans have to approve…" rather than the mock's "… still has to approve".
   This follows the report's "will" rule.
3. **Pick for Any-of.** Pick uses checkboxes, as the mock does, and keeps "Choose human
   reviewer at random" required by [approval readers](../../ontology/features/ui.g5.md#g5.workbench.approval-readers).
4. **Restored batches.** A page-level open with an unfinished failed batch reopens at the
   retry step. This is existing safe-retry behaviour.
5. **Withdrawn headline.** Activity reads "withdrew response for" instead of the raw word
   "deferred".
6. **Prepare to Ship ①.** It drops the Ready/Not ready pill and reviewer avatars in favour
   of the counts, icons and message.
7. **Prepare to Ship ④.** It keeps the existing "Ship with agent resolves it" suffix after
   the exact approval sentence when a PR is open.
8. **Legacy approvals.** The legacy approvals section keeps its info-button timeline.

## Follow-up: settled decisions survive PR closure (#1010)

Request: [issue #1010](https://github.com/G5Labs/G5-Lean/issues/1010). On QA, PR #996
merged after a human approval overrode agent rejections. The requirements then read
Rejected, because the shared `HUMAN_EVIDENCE_IN_SCOPE` predicate counted PR-bound
human decisions only while their PR was open. The agent rejections had no such
filter and kept counting.

Governing amendment: [`g5.governance.approval.request-pr`](../../ontology/features/reviews.g5.md#g5.governance.approval.request-pr).
A settled human or agent decision keeps its effect when a PR opens, closes or merges,
and when `PR_REQUIRED` changes. PR lifecycle still governs request admission and
whether a pending request can be decided.

**Choice: change the one shared predicate.** The outgoing projection and merge
admission both read `HUMAN_EVIDENCE_IN_SCOPE`, so they stay in agreement. The predicate
now requires a request (`request_id`) together with a PR binding or a no-PR request.
It no longer reads PR status. Requiring `request_id` on the PR branch is a deliberate
tightening of "remove only the open-PR condition". Without it, a requestless PR-bound
row would count forever after closure. Current code cannot write such a row
(requestless decisions are refused), and Activity already labels any such row
historical. Quorum, override, later decisions, withdrawal, invalidation and the
source-commit policy are unchanged.

Out of scope, unchanged: pending-request activity (the open-PR or no-PR-while-allowed
rule), the Home attention list of the caller's pending requests, and the personal review
checkpoint, which stays tied to the open PR.

The Request approvals dialog also changes in two places, following the owner's mocks
(issue comment 2026-10-31). First, the title (i) help bolds the four named phrases.
Second, Request mode becomes the design-system `ThemedSelect` listbox. Its trigger is
compact and reads "Request mode ▾" until a mode is chosen, then shows that mode. Its
themed menu grows wider than the trigger (#1370). `ThemedSelect` gains two optional
props: a plain ▾ caret like Request ▾, and `aria-labelledby`, so the accessible name
stays "Request mode" plus the selected mode, as with the former native select. The
visible label above the control and the disabled "Choose a request mode" option give
way to the mock's in-trigger text. One deliberate deviation: the menu keeps the design
system's ✓ column for the selected mode, so its labels sit slightly further right than
in the mock.

Verification:

- `agent_rejection_path_and_needs_then_human_supersedes` runs for each combination of
  `PR_REQUIRED` and closed or merged. It checks the projection, the locked merge gate,
  both decisions in history, and the opposite flag value.
- `closed_pr_keeps_settled_decisions_and_legacy_rows_remain_history` replaces the old
  closed-PR expectation. Legacy rows stay uncounted, and the closed PR's undecided
  request refuses a decision.
- `request_pr_rule_applies_to_both_kinds` adds a no-PR approval under a flipped flag
  and after the PR closes.
- Composer component test: the four bullets' text, exactly four bold phrases, the
  ▾ trigger and its name, the three-option menu, Escape closing only the menu,
  keyboard choice, and reselection keeping the random human. `ThemedSelect` tests
  cover the caret, `labelledBy` naming both trigger and list, and native-select keys
  (arrows open on the selected option; arrows, Home and End skip disabled options). The e2e flows choose
  modes through the menu (`chooseRequestMode`).
