# Can agents reliably derive the review scope from an ontology design?

Proposed 2026-09-29. The user subsequently requested **three features, two repetitions
each, given the ontology diff and governance**. That six-worker execution follows the
[amended protocol](review-scope-20260929/PROTOCOL.md) and is now complete:
[results and effort](review-scope-20260929/README.md). Core concerns appeared in both runs
for each feature; governing exceptions and conditional checks were incomplete, including
shared omissions after retrieval. It tests review-inventory preparation
only: no application implementation, defect search, repair or runtime tests. The original
two-condition proposal below remains unrun; the execution does not establish a benefit
over reconstructing governance without a handoff.

## What we know

The retry study's eight categories are a reasonable summary of graph/input correctness,
history, replay, atomic refusal, authority, launch eligibility, owning operations and UI.
Their underlying obligations follow from the request, plan and applicable ontology.
Eight is not a canonical count: different groupings can cover the same obligations, and
one defect can affect several groups. Generic design and coding obligations can also
require separate treatment when relevant to the diff.

The old comparison left repeatable recovery from a full handoff unmeasured. Its reviewers had a prose plan and searchable
ontology, without an approved feature ontology diff or explicit governing-node handoff.
The evaluator's exact 13-node mapping was added during discovery, before repairs, not
in the initial design. The observed retrieval misses do not measure performance with a
complete planning-skill handoff.

## Hypotheses

- A complete `Ontology changes and governance` section reduces omitted governing
  obligations and variation between independently prepared review inventories.
- Citing the right node is insufficient: agents can still miss a material exception,
  misstate its effect, or include unrelated debt. Evaluate meaning and scope, not ID counts.

## Small first experiment: six fresh workers

Select one retained feature with an actual pre-implementation ontology patch and design;
host draining is a candidate, subject to checking the retained initial artifacts.
Freeze the request, original design, base ontology, proposed ontology and diff. Exclude
later amendments, implementation, worker explanations, findings and evaluator answers.
Do not invent a retry ontology patch and present it as the original experimental input.

Prepare the required section from those frozen design artifacts only, without using
known implementation bugs. Keep a record of every assertion's source. This is a prepared
handoff experiment; it does not yet test whether a planner reliably creates that section.

| Condition | Inputs |
|---|---|
| Reconstruct | Request, original design body, actual ontology diff and searchable ontology; no consolidated first section. |
| Reuse handoff | Exactly the same inputs plus the new first section: changed IDs/diff and applicable governing IDs/obligations. |

Use three fresh independent GPT-6 Sol workers per condition, identical reasoning setting,
five-minute allowance and output request, with no inherited history or peer reports.
For this isolated inventory comparison, the control explicitly omits the section-first
requirement; other guidance is identical. Preserve prompt/input hashes and balanced run
order. GPT-6 Astra supervises and evaluates. Six workers permit a maximum of 30 summed
worker minutes, plus separately recorded packet preparation and evaluation.

Ask each worker to prepare a concise inventory:

`obligation → source ID/diff → why this change is affected → allowed/forbidden case →
owner or boundary to inspect → uncertainty`

Do not ask for eight categories or supply the old I1–I8/G1–G8 tables. Workers may organize
the inventory naturally. No numerical coverage target, defect quota or code inspection.

## Evaluation and interpretation

Before dispatch, Astra freezes a reference inventory from the original request and
ontology, with applicability reasons and materiality, without implementation findings.
It must include relevant architecture, compatibility, invariants and coding principles
as well as feature outcomes. Another fresh Astra context checks that reference for scope
overreach and missing obligations. Ambiguous items remain explicit, not forced negatives.

Compare outputs by atomic meaning, accepting different headings and groupings. Score
without condition labels where possible and disclose when the output reveals its arm.
Record valid additions beyond the reference and adjudicate them uniformly across all
outputs; preserve the original reference and report additions separately.

| Measure | Meaning |
|---|---|
| Material obligation coverage | Which applicable obligations each worker retains, partly captures or misses; cite exact evidence. |
| Repeatability | Which obligations all three recover, only some recover, or none recover. Agreement alone does not establish correctness. |
| Semantic accuracy | Whether the stated obligation/case preserves conditions, exceptions and legitimate behavior; a matching ID alone gets no full credit. |
| Scope precision | Unsupported restrictions, unrelated inherited debt and optional cleanup presented as requirements. |
| Handoff use versus discovery | Separate obligations already explicit in the supplied section from valid additional governing relationships discovered by the worker. |
| Effort | Worker time/tokens and preparation/evaluator effort, including the cost of creating the section. |

A useful pilot signal is fewer material omissions without new material false restrictions
or unrelated repair obligations. Report individual results and disagreement; three runs
per condition cannot establish a reliability rate. If the signal warrants replication,
repeat on two other changes with genuine initial ontology diffs (12 additional short runs),
chosen before examining those worker outputs. Do not replace failed runs selectively.

If both arms repeatedly retain the obligations, retrieval may not be the limiting step.
If the section helps, next test planner-produced sections rather than a curated handoff.
If IDs are retained but cases remain wrong, target interpretation in a separate experiment.
None of these outcomes establishes defect-detection or repair reliability.
