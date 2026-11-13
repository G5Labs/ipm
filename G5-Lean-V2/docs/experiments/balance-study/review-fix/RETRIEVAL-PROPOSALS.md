# Two small planning-retrieval experiments

Originally proposed 2026-09-28. The pilot and subsequent 27-run planning comparison are
complete; **A was incrementally adopted with user approval on 2026-09-29**. B remains
experimental. See the [results and adoption record](README.md#follow-up-27-planning-runs-on-three-scenarios).
The original proposals below are retained; the separate semantic-application intervention
has not been adopted or tested.

## What to borrow from PairSoft

The PairSoft analysis kept outside this repository
describes index → interaction map → relevant articles, with governing ADRs and a compact
feature decision log. It does not establish a universal decision-to-code graph: the
integration map describes event producers/consumers, citation impact checks find candidate
staleness, and decision links are distributed. Neither resolving links nor the report's
qualitative ratings establish measured retrieval reliability.

Our ontology already has a root reading map, a system-invariant index and module
collaborators. Adding another comprehensive index would duplicate maintenance without
demonstrated benefit. The sharper weakness is navigation **at the point a rule is used**:
the generic admission block links to the lifecycle file, while the future-warning exception
is a separate block not directly linked from that rule. A reader can follow a plausible
route and still miss the qualifier.

Test a small procedural change and a small structural change separately. Keep canonical
meaning in the existing blocks; do not build a parallel knowledge base, code dependency
database or mandatory repository-wide compliance matrix.

## A. One retrieval challenge before finalizing a plan

Keep the current ontology unchanged. Add one explicit checkpoint after initial discovery
and before the existing design review:

> Revisit the plan's material assumptions from a different retrieval route. Which owning
> rule could make a proposed allowed/refused behavior wrong? Which linked exception,
> lifecycle state or interacting capability has not yet been read? Inspect the relevant
> shared-concept/invariant entry and canonical blocks before resolving the assumption.
> Record new governing references or remaining uncertainty in the existing plan; do not
> answer “checked” without the corresponding retrieval.

Use judgment about relevance, following one additional link when its relevance is
uncertain and could change the design. Do not repeatedly ask “have you checked everything?”
or exhaust every concept for every task. Stop when material assumptions have a governing
basis or are explicitly unresolved; completeness is not guaranteed.

This changes the timing and accountability of retrieval, not the interpretation method.
The current skill already requests governing commitments and allowed/counterexamples:
reuse that artifact rather than adding another ledger. The hypothesis is that a deliberate
second route recovers assumptions missed during the first story/feature traversal.

## B. Put exception links beside the rules they qualify

Keep the planning procedure unchanged. In an isolated ontology variant, add or tighten
only navigational relationships:

- A generic rule directly links the canonical exception/state distinction that can
  reverse its outcome. The exception links back to the rule it qualifies.
- Relevant module operation entries link directly to their governing block rather than
  only to a whole policy file. Preserve existing stable IDs and one authoritative rule.
- Use descriptive link labels that identify the distinction; avoid copying the rule's
  full body into an index or adding a large new taxonomy.

Illustration from the known case: beside `g5.security.admission`, link “explicit Stop
closes admission” to `g5.lifecycle.stop` and “future deadline warning keeps admission open”
to `g5.lifecycle.deadline`. The deadline block links back to admission. This puts both
outcomes on the natural reading route without merging independently changeable rules.
It is a calibration example, **not a held-out evaluation task**.

The hypothesis is that a local, meaningful connection costs less to read and maintain
than a broad map while making overlooked exceptions more visible. It can still fail if
the relationship itself is missing or the reader ignores it. Record construction and
maintenance effort; do not assume organization makes omission impossible.

## Proposed pilot

Use a 2×2 comparison to test the two proposals and their interaction:

| Arm | Planning procedure | Ontology navigation |
|---|---|---|
| Control | Current frozen skill | Current structure |
| A | One retrieval challenge | Current structure |
| B | Current frozen skill | Direct rule/exception links |
| A+B | One retrieval challenge | Direct rule/exception links |

Preselect **two untouched requests**, each involving a different cross-cutting interaction.
Start with three fresh GPT-6 Sol/xhigh planners per arm on the first request: **12 planning
runs**. If a variant shows a useful signal, repeat all four arms on the second request
for **24 runs total**; stop after the first request for an unpromising pilot rather than
automatically spending the full budget. GPT-6 Astra/xhigh prepares the fixed evaluation
and independently assesses outputs. A result on just one request is not an adoption result.
No implementation or repair runs are needed for this screening experiment. Each planner
gets the same 15-minute maximum for retrieval, proposed ontology diff, plan and ordinary
design review. Compare actual effort as well as the common cap. The first stage allows
three summed Sol hours; both stages allow six, not six elapsed hours. Setup, ontology
preparation and Astra evaluation are extra.

Freeze the current planning skill after concurrent edits settle, rather than compare
against the old experiment's different handoff. Use the same request and ontology meaning
in every arm. Planning remains source-free. Keep application code/tests, previous findings,
worker reports and the evaluator's required-block list out of worker inputs. The structural
variant may change links/labels only; an independent check must confirm no requirement,
exception or answer was added. Save the base corpus, variant diff and prompts.

To avoid tailoring B to the answers, prepare its links for the bounded ontology area before
selecting the held-out requests. Apply one stated navigation rule across that area, record
all edits, then have the evaluator select requests and freeze the obligation set before
dispatch. Balance run order across arms. Each planner saves its initial plan before the
final review so later improvement can be distinguished from initial retrieval.

Evaluate at three separate stages for each material governing commitment:

1. Was the **actual authoritative text** retrieved, including the relevant exception?
2. Was its obligation correctly retained in the final plan/ontology diff, or explicitly
   left unresolved? Count false restrictions and contradictions too.
3. Did the planner claim completeness despite a known omission?

Primary outcomes: material retrieval omissions and omissions surviving into the final
design. Secondary outcomes: irrelevant retrieval, uncertainty quality, final design
alignment, time, input/cache/output tokens and one-time navigation-maintenance effort.
Successful retrieval without correct incorporation remains a diagnosis miss; this pilot
does not claim to fix that mechanism or establish coding reliability.

Prefer the smallest variant that reduces material omissions across both requests without
introducing serious false restrictions or materially increasing effort. Pre-register a
practical screening margin, such as at most 20% greater mean planning time/output tokens,
before running; report tradeoffs rather than changing it after seeing results. If A alone
works as well as B or A+B, keep A. If B provides the gain, prefer the structural change and
avoid retaining an unnecessary checkpoint. A mixed or one-request-only result warrants
another held-out request, not a reliability claim or immediate broad ontology rewrite.
