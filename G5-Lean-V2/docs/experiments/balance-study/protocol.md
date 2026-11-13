# Ontology, native implementation and review: bounded balance study

## Purpose and checkpoint

Started after the evaluator adjudication and minimal clarifications were pushed in
`1738018cad0cb411a31f4c52539e2cf9f2443bd9`. GPT-6 Sol, reasoning `xhigh`, performs all
experimental work; GPT-6 Astra evaluates evidence and proposes changes. No production
feature implementation is merged from these trials.

The practical questions are separate: does ontology improve a consequential design;
does a fresh native coder preserve that design; and does ontology-informed review find
real violations without blocking legitimate behavior? The unit of evidence is a
specific supported outcome, omission or false claim, not an overall `/10` grade.

## Stage D: new designs

Two new requests, three fresh contexts each. Each worker saves one concise high-level
design, consequential assumptions, acceptance outcomes and a short reading/decision
log. Source inspection is allowed in the two native conditions. No code, tests,
external research, prior experiment reports or other solutions are allowed.

| Condition | Available information and behavior |
|---|---|
| N | Application source and ordinary repository documents; native planning. Ontology and repository skills removed. |
| O | Same source/documents plus current ontology, including metadata; native planning. No required ontology editing or design-review ceremony. |
| H | Current ontology and ontology skills; use the maintained source-free planning workflow, including ontology edits and self-review. Application source/documents are absent. |

This is a **practical bundle comparison**, not an equal-information representation
experiment. O versus N tests the useful retained information and its retrieval together.
H versus O tests the workflow package, including source-free planning and required
artifacts. Metadata's implementation hints are disclosed treatment, not independent
discovery. Source bytes are identical where available. Sanitized copies have no Git
history, uplift archive, previous experiment artifacts, inherited repository AGENTS or
CLAUDE files. Task instructions forbid reading outside the assigned copy; this is an
instruction boundary, not an operating-system security boundary. Ambient model/tools
and globally advertised skills remain a limitation.

### Request D1: recipient-accepted ownership offers

Add workspace ownership offers. The current owner can offer a non-base workspace to
an eligible teammate. The recipient can accept or decline; the owner can withdraw.
Pending offers expire after 24 hours and are recoverable after browser/API restarts.
Making an offer must not transfer ownership or interrupt current work. Acceptance
uses the existing safe ownership-transfer lifecycle; the recipient must explicitly
start/resume afterward. Keep the existing administrator immediate-transfer action.
Provide the workspace/inbox experience and audit/notices needed by both people, with
truthful pending, in-progress and final outcomes. Handle retries and concurrent actions
without duplicate transfers. Choose and explain reasonable remaining product policy.

### Request D2: retry a failed workflow branch as a new run

Add an owner action on a terminal failed workflow job to retry one failed task and its
transitively blocked descendants as a fresh linked run. Preserve the original job,
task/input history and successful sibling work. Successful prerequisites outside the
retry set may satisfy the new run through an immutable recorded success; do not execute
them again. The owner can revise the selected task's instruction before submission.
Show original/new-run lineage and honest progress in the existing workflow/chat UI.
Submission retry after a lost response must return the same run. Respect current
workspace/provider authority and the platform's task/session/dependency rules. Choose
and explain reasonable remaining product policy; do not add automatic VM start.

### Frozen design obligations

Judge meaning and supported alternatives, not particular tables, service names or enum
spellings. `Explicit`, `partial`, `missing`, `contradictory`, and `requires source check`
are distinct. An assumption correctly reserved for coding is not an implemented defect.
Do not demand a detailed algorithm or every endpoint in a high-level plan.

| ID | D1: transfer offers |
|---|---|
| T1 | Pending offer preserves owner/compute; accept/decline/withdraw/expiry have clear exclusive effects. |
| T2 | Current owner and recipient eligibility are checked at their relevant admission boundaries; stale expected-owner and administrator intervention are safe. |
| T3 | Acceptance preserves stop, custody release and credential purge before ownership change; failure retains old owner; no automatic recipient Start. |
| T4 | Existing nonterminal legacy execution refusal and administrator immediate transfer remain valid; the new caller's authority differs from existing transfer authorization. |
| T5 | Durable replay identity and concurrent accept/withdraw/expiry do not create duplicate work or contradict a completed result. |
| T6 | API restart and interrupted transfer reconcile through one lifecycle owner; reused transfer behavior is either established or a named consequential assumption. |
| T7 | Recipient, owner and administrators see authorized durable state, useful notices/audit and truthful recovery; private owner credentials/work are not exposed. |
| T8 | Responsibilities and acceptance cases are concrete enough for a fresh coder; meaningful ambiguities/amendments are explicit without unnecessary infrastructure. |

| ID | D2: workflow branch retry |
|---|---|
| J1 | Defines retry closure and immutable external success; unrelated siblings and original histories remain unchanged. |
| J2 | A fresh logical job/tasks/sessions/input graph has explicit lineage; it does not reset failed task/attempt state or revive old delivery evidence. |
| J3 | Task/dependency owner performs atomic graph allocation and validation; dispatch follows commit; external-success representation respects normalized dependency authority. |
| J4 | Replayed submission binds accepted closure/instruction/specification and returns the same graph; changed accepted body conflicts. |
| J5 | Current workspace owner, provider admission, session serialization and held-work rules govern execution; no implicit VM Start. |
| J6 | Dependency settlement, cancellation and retry-run progress work for changed instructions and blocked descendants without re-running successful ancestors. |
| J7 | UI exposes original/new lineage, effective inputs, admission/failure state and recovery; shared checkout effects are not described as rollback or isolated execution. |
| J8 | Governing amendments, reuse assumptions and human choices are explicit; architecture remains minimal and implementable. |

These obligations come from the request plus current governing lifecycle, Agents,
security and recovery contracts. They are withheld from workers. They are diagnostic
coverage rows, not independent statistical observations. Check uncertain judgments
against the exact contract and allow multiple valid designs. Save initial plans before
adjudication. Record plan length, elapsed time, available token/tool telemetry, actual
reading, ontology changes, false claims and useful discoveries separately.

## Adaptive next stages, specified before dispatch

After D, choose the most informative bounded implementation task. Freeze a corrected
high-level design and evaluator cases before giving it to fresh native coders. Compare
ordinary plan handoff with a minimal ontology-aware handoff; preserve the first delivered
code before review. The same plan must be supplied in both arms so plan quality cannot
explain a coding difference. Keep source, tools and dependencies matched.

Then review an identical frozen artifact using native review with/without relevant
ontology. Include confirmed violations and allowed exceptions/authorized changes so
false blockers are measurable. Preserve reviewers' initial findings before explanations
or repairs. Distinguish ontology-exclusive information from source-recoverable facts.

One paired success motivates replication; it does not establish reliability. Repeat a
case when it resolves an actionable mechanism, and use an equal-information control
where a conclusion would otherwise confuse retained knowledge with representation.
Stop expanding the matrix once further trials would not change the practical decision;
report unresolved reliability and human-supervision limits explicitly.

## Preservation and decisions

Exact packets, hashes, manifests and raw public histories live in the ignored
`docs/experiments/runs/2026-09-28-balance-study/`; concise plans/findings and the synthesis
are tracked here. The preparation script captures source/context hashes and excludes
prior solutions. Never treat hidden reasoning as available trace evidence.

Change maintained ontology only to restore or clarify a demonstrated consequential
contract. Change harness only when a small trial identifies a useful decision-time
intervention and an allowed-case control does not regress. A failed implementation
despite an adequate plan is not automatically a reason to add more planning text.
Every later protocol amendment must identify what was already observed.
