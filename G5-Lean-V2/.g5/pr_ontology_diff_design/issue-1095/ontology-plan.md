# Issue #1095 — approval reports and request UI

Request: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) (13 narrowly scoped items) with the owner's
[mocks and preservation audit](https://approval-ux-k7m4p9.hariharan-rahul.chatgpt.site/approval-ui-review/AUDIT.md).
Owner decision on item 1: [report order ships as schema 6](../../chat_intent/issue-1095/report-order-contract.md).

Every item is presentation or future-report guidance. Approval authority, quorum, request/decision lifecycles, stored
reports and notes, and merge admission keep their existing owners and rules. The frontend keeps no quorum logic
([status](../../ontology/features/reviews.g5.md#g5.governance.approval.status)).

## Owners and choices

| Item | Owner | Choice |
| --- | --- | --- |
| 1 Report body order | Formal contract ([assignment](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.assignment), [rollout](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.contract-rollout)) | Published contract assets are digest-pinned and immutable, so frozen retries keep their instructions. New assignments use schema 6 = schema 5's frontmatter fields and validation, under contract `g5.formal-agent-approval/v5` and new formal `agent.turn` modules in a new catalog. Schemas 2–5 and their modules are unchanged. |
| 2 Governing prompt collapsed | [Report view](../../ontology/features/reviews.g5.md#g5.governance.approval.report-view) | The rendered view splits the body at a top-level `Governing prompt` heading outside code fences; that section alone is a native disclosure, closed by default. Raw view and bytes are unchanged; reports without the heading render as before. |
| 3 Currentness icons | Shared glyph/tone in `approvalRequirementRoster.ts` | One circled `=`/`≠`/`?` family (blue/amber/gray) for both the aggregate and per-decision indicators; labels, popups and absence when no evidence are unchanged. |
| 4, 10 Personal banner | [Personal requests](../../ontology/features/reviews.g5.md#g5.governance.approval.personal-requests) | Derived from the existing per-requirement `callerCanDecide` on the one workspace approvals projection: the server's addressed-active-request rule, independent of requirement status. The landing summary and the Approvals page already read this query, so the banner shares its loading/error status. There is no new count, inbox or request state. The landing variant links to Approvals; the Approvals-page variant shows plain text only. |
| 5 Home caption | [Home attention](../../ontology/features/ui.g5.md#g5.ui.home-attention) | Owner's prepared patch: approval rows' secondary line is the truncated workspace name with a native `title`. |
| 6 Escalation badges | [Status evaluator](../../ontology/features/reviews.g5.md#g5.governance.approval.status) | See below. |
| 7 Read report | [Request status](../../ontology/features/reviews.g5.md#g5.governance.approval.request-status) | Roster report links are a compact high-contrast button with a document icon. Request again, Retry and Cancel stay text links. Availability is unchanged. |
| 8, 11 Request notes | [Request status](../../ontology/features/reviews.g5.md#g5.governance.approval.request-status), [request note](../../ontology/features/reviews.g5.md#g5.governance.approval.request-note) | Each roster row's own request shows `Request note from <requester>` as a per-request disclosure (`Request note · Requester unavailable` when unknown). It is open initially only when the request's recipient user ID equals the viewer's. The disclosure is local state keyed by request, so refreshes keep manual choices. Responses are labelled `Reviewer response:` and `Agent response:`. Timeline is unchanged. |
| 9 Request mode | [Request composer](../../ontology/features/reviews.g5.md#g5.governance.approval.request-composer) | Owner's prepared patch: visible `Request mode` label, `Select` placeholder, same three options. |
| 12, 13 Overview sections | [Landing](../../ontology/features/ui.g5.md#g5.workbench.landing) | The visible and accessible section labels become Primary Issue and Work Description. Primary Issue gains a local disclosure, closed initially only when Work Description shows a PR. The populated state comes from the same PR-selection hook the section renders, so loading, error and the empty placeholder never count. A manual choice wins for the workspace's mounted landing. |

## Item 6: escalation as a derived display

The evaluator already holds the facts: the counted agent verdict, counted human verdicts, `override_allowed`
(Any-of with human override) and `requestable` humans (eligible, active members). Its request needs already use
"override allowed and a requestable eligible human exists" as the human route (`can_override`). Item 6 reuses that
exact condition, so a configuration flag alone never promises an unusable path:

- **Needs Human Escalation**: status Rejected, no counted human rejection, the counted agent verdict is a rejection,
  override is allowed and a human route exists. A Waiting human request does not change this.
- **Overridden by Human**: the counted agent rejection is overridden by a counted eligible human approval and no human
  rejection counts (status Approved).
- Otherwise there is no escalation display, and badges read as today.

The evaluation returns this display and the counted agent decision's review id. The projection exposes it as
`agentEscalation`. The UI relabels the requirement badge (Needs Human Escalation, amber) and the agent row whose primary
decision is that counted decision. The status, the `M rejected` counts, merge admission, path text, Timeline, reports
and earlier settled responses keep their existing wording. Interpretation: "current agent badge" is the agent row's
latest response badge; an older "Earlier settled response" beneath a newer agent request keeps its recorded label.

## Assumptions and limits

- `callerCanDecide` and Home's actionable list apply the same addressed-request, request-PR and self-approval rules.
  Neither checks the organization-viewer refusal, so the banner inherits Home's existing eligibility.
- Escalation's human route uses `requestable` (active member, eligible), which does not distinguish organization viewers;
  this matches the evaluator's existing request needs.
- Schema 6 reviews need hosts whose runtime carries the new catalog; an older review VM is refused with the existing
  "runtime predates the current approval contract" retry notice.

## Verification

Evaluator unit tests for the escalation table (including Waiting human, no requestable human, override off, All-of,
human approval/rejection, later agent approval, withdrawal); API projection assertion; contract tests for schema 6
(identical frontmatter shape, v5 assets, module acceptance); frontend tests per item (report disclosure and Raw bytes,
icons, banners in one/many/zero/loading/error/satisfied cases, Home caption, badges, Read report, notes defaults and
refresh, composer, Overview labels and disclosure); full frontend suite and a browser pass.

## Evidence (2026-11-05, local)

- Evaluator: `requirement_status` unit tests including `agent_escalation_follows_counted_evidence_and_an_actual_human_route`.
- API (Postgres): `api_agent_approval_evaluation` (escalation across the combined-quorum matrix and the human-supersedes
  flow), `api_workspace_approvals` (29), formal approval suites under schema 6 (39 passed, 1 ignored).
- Contract: `check-agent-approval-contract.py`, `sync-agent-approval-limits.py --check`, `g5-contracts`,
  `g5d-dispatch`, `g5d --test dispatch_modules`, `agent_dispatch` unit tests.
- Frontend: full Vitest suite (one unrelated PDF viewer test flaked under load and passes alone); new tests per item.
- Browser (full-stack preview, crafted approvals/report responses): banner on Overview and Approvals, escalation and
  override badges, own-note default and keyboard toggles, collapsed prompt with exact Raw, Request mode / Select,
  Primary Issue defaults with and without a PR, light/dark at 1280 and 1024 px with no horizontal overflow.
- Not run here: native-CLI formal runs, Playwright e2e flows (two updated to open Primary Issue), Cloud CI. One
  untouched `g5d-agent` ontology_turn test fails locally on a Git fetch by SHA.
