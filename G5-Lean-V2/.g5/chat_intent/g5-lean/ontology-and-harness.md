# Human intent: ontology and G5 harness

Source: [the current task](codex://threads/01a0ed93-7205-7842-859a-0fad5815c2d6).
The user described ontology and harness as “the core of G5” in the initial question.
The user clarified that “ontology” means `.g5/ontology`, not the `docs` directories,
then requested one node whose I/D blocks remain roughly exactly as supplied below.
The following is the relevant user wording, with chat formatting normalized.

## User stories that ontology supports

1. Live doc for human understanding: A user can rely on the ontology to understand the current systems.
2. Surface to understand proposed changes: A user can rely on the ontology changes and relevant/governing contract to gain a deeper understanding of the requested implemented (new feature, bug fix, refactor, etc).
3. Consistency and deduplication: Ontology is a consistent, deduplicated representation of all non-trivial human intent/design and important agent decisions: not conflict, duplicated desciptions in ontology.
4. Minimalism: Ontology has minimal, concise description to clarify the essence of non-trivial human intent/design and important agent decisions. It does not restate what can be covered from the code.
5. Lineage: When a consistent conflict is detected, the ontology provides the lineage needed to understand where the conflict original originated from.
6. Design Principles, Invariants, Policy: The ontology automatically follows good design principles and abstractions, encode system invariants and policies. These will guide and enforce agent to generate better and more reliable code.
7. Formal verification: Ontology will serve as the foundation for formal verification, e.g. specifying the formal specs, understanding what's important, distinguish between what can be enforced by test, human verification, agent review, or formal verification.
8. Sharing and reference architecture: ontology defines important invariants, boundaries, data model, design/coding principles, that can be shared across to another projects within the same organization. At the same time, referenced ontology can be built for commonly-used softwares (e.g., CRM system) + codebase that customer can start tailoring and integrating to their requirements instead of starting from scratch
9. Agent consumption: agent should be able to efficiently use ontology to do design, implementation, and review of new requests. The format and instructions must be agent friendly.

## Ontology and G5 workflows

1. Basic chat workflow for simple stuff: this will be same as any regular chat behavior with no harness.
2. G5 workflows for incremental development: for non-trivial incremental development (e.g. fix a complex bug, implement new feature, refactor), G5 workflow will first do the design and plan in the ontology space, then do regular implementation according to the high-level design, and review/verify the implementation against the design.
3. Uplift: ingesting the pre-existing codebase to create ontology for it. The key is to mine the important human intent/design and agent decisions from external resources, git histories and inline code comments and verify them against the code.
4. Downlift: a new ontology is created first from scratch and a functional codebase will be built from it. The ontology building will follow good design principles (module boundary, abstractions, encapsulation etc) and clearly define the user stories, features, and governing contracts so that code can be reliably generated.

## G5 harness principles

1. The design and plan is conducted in the ontology space, to propose the ontology changes in diff and identify all relevant/governing contract in the ontology, including adjacent feature, invariants, good design principle, coding principles etc.
2. Human must be able to read the design/plan and understand what is the request and how it would be implemented at a high-level.
3. The implementation phase will pin the design/plan in its context and ask agent to build the request according to this design/plan, but it does not have additional harness or strict enforcement on it yet.
4. The post-implementation review phase will review the code changes against the design (can update the original design and/or ontology if needed) to ensure they are correctly implemented.
5. Harness

## Top-level module boundaries

Source: a later Claude Code session, with chat formatting normalized.

Top-level modules are functioning similar to separate repos, ontology may enforce some changes to be bounded in some top-level modules, this is one way we enforce the edit boundary. Each top-level module must have its own ontology node in software_architecture.

## Authoring interpretation

- `features/ontology_and_harness.g5.md` owns the user outcomes and supporting workflows under the repository's feature structure.
- Grammar is corrected without changing the requested meaning. Story 8 has two blocks under one heading because organizational sharing and a customer reference architecture can change independently.
- Harness item 5 contains no description, so it adds no inferred commitment.
- Top-level module boundaries have two blocks under one heading because the edit-boundary role and the per-module node requirement can change independently. `software_architecture` means `.g5/ontology/software_architecture/`.
- Existing overlapping commitments retain their stable IDs at the new owner; detailed review rules remain at their existing owners and are linked.

## System invariants and ontology scope

Human clarification during the issue #238 ontology/harness comparison, with spelling normalized:

- System invariants must live in `.g5/ontology/concepts/system_invariants.g5.md`.
  Assess whether its referenced decisions are actually must-enforced system properties,
  using the concrete PR to determine what is useful to preserve.
- Human origin is already represented by metadata. Do not add explicit classifications
  for lasting decisions versus PR-local choices; file placement and content should make
  scope clear. Omit implementation choices reliably recoverable from code unless they
  are human-verified, very important for future features, or hard to recover from code alone.
- Preserve previous intent, reason about how relevant system invariants affect the
  feature and how to enforce them, use relevant contracts in `.g5/ontology/concepts`,
  and produce a consistent design without overinterpreting existing design decisions.

## Invariant enforcement and simplification

The human confirmed the five centralized properties as real invariants and requested
a fresh implementation assessment: why does metadata describe partial support, and
are additional invariants missing, especially ones that simplify state machines or code?
This confirmation concerns the required properties, not their implementation correctness.

## Three additional invariants

The human keeps “Base workspaces have no live compute or working member branch” as a
design decision, and requests human-readable definitions of the other three proposed
invariants in the canonical invariant file, with concrete implementation gaps:
shared workspace branch names, old VM work not affecting a replacement, and one
authoritative owner per fact.

## Principle consolidation and whole-ontology review

The human requested removing redundant `design.evolution`, combining coding and
system-design principles into one concise node, and removing generic advice that
agents already know. They requested concrete sections on module boundaries and good
abstractions: what to aim for and how to avoid leaking implementation details. They
also authorized reasonable ontology improvements and requested a whole-ontology scan
against the uplift skill, documents and code, with concrete recommendations for other
nodes. This authorizes ontology refinement, not application changes or deployment.

## Authorized incremental refinement

After reviewing the whole-ontology recommendations, the human requested: “ok please
make all incremental update to the ontology that make sense to you.” This authorizes
source-supported amendments, reference deduplication, scope clarification and metadata
correction. Recovered agent decisions remain agent-originated; the request does not
verify application conformance or authorize implementation changes.

## Minimal design and bounded review work

The human supplied a colleague's anecdotal report: three independent reviews surfaced
useful findings, but one PR took roughly 16 hours instead of an expected 3–4, involved
unexpected database migrations and an RLS issue, and suffered CI/merge restarts. The
colleague explicitly did not attribute this conclusively to the harness, ontology or
models and had no unbiased control.

The human requested another ontology/harness synchronization to remove opportunities
for misinterpretation, over-engineering and complicated design, and report remaining
risks. This continues the earlier instruction to favor the cleanest design with minimal
state and compact code while retaining complete implementation, integration, risk
mitigation and testing.

Agent interpretation: preserve the useful independent-review workflow, strengthen the
existing mechanism-justification and scoped-recovery rules, distinguish defects from
optional redesign, and make follow-up verification incremental. These are guidance
clarifications, not proof that this colleague's migrations were unnecessary or that
the harness caused the reported elapsed time. No application or CI changes are authorized
by this audit.

## Concise planning and one review

The human clarified: focus on high-level minimal design in ontology, reading code when
needed; use one review. Keep ontology/harness changes very concise. Merge the harness
changes first and validate with real features afterward, rather than more experiments now.
This supersedes the preceding agent interpretation retaining three reviews.

## Confidence-guided review fixes

The human requested fixing everything the reviewer can address confidently while keeping
minimal design and compact implementation. Put remaining gaps in a concise review-file
table with governing ontology, gap, proposal and uncertainty or alternatives; surface it
to the user.

## PairSoft verification workflow adoption

Source: [the PairSoft experiment conversation](codex://threads/01a10d4f-434c-7491-852f-8a9350a3e1fb),
5 October 2026. Relevant human instructions, faithfully summarized:

- Keep formal expansions in the parent's ontology file; child IDs and metadata make
  the relationship explicit. Add concise independent status/evidence tags, retaining
  implementation and test percentages with their basis.
- Add post-finalization TLA+/TLC checks to initial uplift. Suggest trajectory tests;
  create and run them only with user approval.
- Create `ontology-downlift`: recover requirements and external references, build
  initial ontology, use TLA+/TLC for every original core feature to refine correctness
  and minimal design, preserve modularity/abstraction/encapsulation, and finish with a
  separate file of remaining questions and the most production-ready options.
  TLC-driven tests are required for downlift implementation.
- Incremental features/fixes use model checking only for complex changes with design
  uncertainty; require new/extended TLC-driven tests when model checking was conducted
  in the PR. Otherwise follow the regular workflow and testing.
- Update the harness in G5-Lean-V2 and PairSoftPlatform_simplified. Defer the proposed
  incremental proved-core enforcement and cost/quality experiments to later sessions.

Agent interpretation: retain existing project model/proof gates; share modeling/test
mechanics in one skill reference. This is a harness update, not a new PairSoft model,
proof run, runtime test campaign or whole-ontology reassessment.

## Minimal boundaries and opt-in incremental model checking

Source: [the boundary discussion](codex://threads/01a10db1-ca30-7050-a681-2753b029e553),
6 October 2026. Human instructions, faithfully summarized:

- Apply the minimal ontology/harness refinement: keep project boundaries and proof
  policy in ontology and retain the existing design/review workflow.
- Planning runs TLA+/TLC only when the user explicitly requests it. If the plan did
  not use it, review adds no model-specific work; if it did, review ensures the
  TLC-driven tests are generated or extended and executed.
- Keep the harness concise and avoid an implementation skill; simple changes should
  not incur additional design or verification procedures.

Agent interpretation: this replaces the earlier complexity/project-gate trigger in
shared incremental planning. Initial uplift/downlift retain their separate policies.
The only added evidence reminder concerns code/specification/binding changes and
premises supplied by other modules. No new boundary artifact, implementation skill,
automatic proof campaign or filesystem enforcement is introduced.

## PR 246 minimal workflow and assumption checks

Source: [the PR 246 replay discussion](codex://threads/01a11705-0702-71a3-b3cd-6123a62c19db),
7 October 2026. Human instructions, faithfully summarized:

- Favor sharper obligations and minimal workflow, especially removing procedures
  without demonstrated benefit. Propose concrete removals and additions to the skills.
- Explain why the TLC arm did not check the assumption that could make the simpler
  no-TLC design safe through real retries, restoration and settlement. Generalize the
  useful question in the harness so future agents ask it more reliably.
- Apply the proposed harness changes now; run new experiments later.

Agent interpretation: install the [reviewed proposal](../../../docs/experiments/ontology_design_principle_enforcement/design_comparison_study.md#harness-conclusions),
including explicit-request separate review, one evolving design and optional presentation
formats. This supersedes automatic separate review and the prescribed remaining-gap
table, while retaining ordinary implementation verification and required trajectory
tests. Sharpen modeling around the guarantee that could eliminate a mechanism: where
uncertain, check how it is enforced instead of assuming it. External guarantees stay
conditional with named implementation checks. These are proposed improvements to agent
behavior, not measured gains; no new author experiment is authorized for this turn.

## Common design before optional model refinement

Source: [the design replay discussion](codex://threads/01a11705-0702-71a3-b3cd-6123a62c19db),
7 October 2026. Human instructions, faithfully summarized:

- With or without TLA/TLC, use the same skill to generate the design and ontology
  edits and perform the regular design review.
- Put all TLA/TLC instructions at the end of the planning skill. Only when explicitly
  requested, apply them to the already generated design to check it and consider
  simpler alternatives.
- Investigate whether simplification removed implementation arguments in the #339
  experiment. Additional implementation detail may or may not be consequential.

Agent findings: the authors began independent turns from the same inputs, not a shared
reviewed plan. TLC ran before that author's first saved design. The no-TLC author added
the Git-lock/host-cache findings during its final source consistency pass; no deletion
of those findings from the TLC plan is evidenced. Identical skills did not establish
identical intermediate designs.

Agent interpretation: make the common self-review explicit, then preserve a recoverable
input revision for optional refinement. Carry forward obligations and acceptance checks
outside the model and review only affected changes. Keep routine mechanics out of the
plan; retain details needed to establish consequential guarantees. Do not add a separate
reviewer, approval gate or active plan. Assertion sensitivity belongs in the optional
model guide. These instruction changes are not evidence of improved agent behavior;
no additional author experiment was run for this amendment.

## Final review after optional model refinement

Source: [the design replay discussion](codex://threads/01a11705-0702-71a3-b3cd-6123a62c19db),
7 October 2026. Human clarification superseding the prior ordering: require the common
design review after TLA/TLC rather than before it. Discard the TLC worktree's changes
and rerun only that design experiment with the new harness.

Agent interpretation: save the initial design and ontology changes, apply requested
model refinement, then self-review the whole final result including unmodeled
implementation obligations. No mandatory prior review or separate reviewer is added.
Keep the original run archived and start a fresh author chat at the same replay base,
with the same product-only prompt and `gpt-6-astra / xhigh`; supply no evaluator findings.
Improved reliability is a hypothesis to assess, not an established effect of ordering.

## PairSoft E/F boundary and evidence refinements

8 October 2026. Human approval of the preceding three proposals: “ok make these three
refinement.” Preserve the PairSoft implementation and prepare `experiment_G_G5` from
`main` with the newest harness; the human will run Experiment G manually.

Agent interpretation: check relied-on facts through the actual commit boundary;
include unchanged lifecycle consumers and independently varied correction bounds;
route through installed local skills and deduplicate shared evidence inventories.
Select affected model checks while preserving required gates; do not restore automatic
cache guidance. Instruction/tool consistency checks do not establish improved agent behavior.

Executed follow-up: the [TLC-only rerun](../../../docs/experiments/ontology_design_principle_enforcement/design_comparison_study.md#stop-and-restart-findings)
followed the saved-draft/model/final-review order. Its final review added concrete
implementation bindings and its assertions caught the prior false-loss probe, but
the long Git-lock and cached-host-failure gaps remained. This is one scoped result
under multiple instruction changes, not a causal reliability claim.

## Explicit model-specific review

8 October 2026. In this conversation the user requested a compact ontology-focused
review skill, with TLA/TLC rechecks only when explicitly requested for that review.
Earlier design-only modeling does not authorize model-specific review. The user then
rejected bounded execution and preserved diagnostic results as counterintuitive;
those runner changes and their shared instruction were removed. The
[mini-experiment summary](../../../docs/experiments/ontology_design_principle_enforcement/pairsoft-harness-experiment-takeaways.md#narrow-harness-results)
records adopted and rejected changes and their evidence limits.
The user further clarified: “Complete the full workflow should not trigger TLA/TLC.”
This excludes generic full-workflow wording from incremental design and review opt-in.

## Shared design evaluation and lightweight downlift implementation

12 October 2026. Human instructions in the Twenty downlift lifecycle discussion,
faithfully summarized:

- Design review and simplification should share principles with ontology-planning,
  while initial downlift and incremental development have different scope and cost.
- The sixteen proposed evaluation dimensions are reasonable. Initial downlift can
  take the time needed; incremental work needs appropriate weighting and attention
  to time and cost instead of the full evaluation on every feature.
- Keep downlift implementation instructions simple: tell the agent what to read and
  let it implement within the chosen design. Correctable implementation deviations
  can be resolved during implementation and review; a different architecture is not
  authorized. Review checks ontology/code conformance and formal verification.
- Generate real E2E system tests from the ontology and TLC-driven cases, and require
  them to pass at the end of implementation. Consider the same approach for incremental work.

The supplied dimensions are intent fidelity, verification coverage, failure-mode
completeness, performance/scalability, availability/recovery, security, privacy/data
lifecycle, observability/operability, resource cost, evolvability, migration/compatibility,
delivery feasibility, dependency/supply-chain risk, API usability, specification quality
and review independence.

Agent interpretation: keep the evaluation reference under planning, require full scoped
consideration for initial downlift, and select affected dimensions by impact and
uncertainty for incremental work. Required outcomes cannot be offset by scores; scores
and additional scorers are optional. Share ontology-derived E2E guidance, with TLC-derived
tests required where the workflow already uses modeling. Preserve explicit-request
separate review and incremental/model-review TLA rules; this clarification does not
authorize new modeling for every feature. No new implementation skill, approval step
or per-step enforcement is introduced. Local instruction checks do not demonstrate
improved agent behavior or lower execution cost.

Follow-up clarification, 12 October: minimal design is the key goal of incremental
work, and TLA+/TLC is enabled only by explicit human request. The harness should assume
top-capability agents and omit unnecessary directions or forced behavior without
demonstrated value. Agent interpretation: shorten the shared skills and references,
retain consequential commitments and demonstrated failure-boundary guidance, and leave
routine methods to the agent. The initial-downlift verification policy is unchanged.

## Minimal bookkeeping and explicit deep ontology review

12 October 2026. After discussing the historical coding-quality report, the human
approved the proposed simplification: “ok please do all of these.” The accepted scope:

- Keep consequential intent, exceptions and rationale useful to agents; traceability's
  effect on agent quality or effort remains unmeasured. Principles must not invent
  stronger product guarantees or recovery machinery beyond accepted requirements.
- Use one evolving design, actual ontology diffs and existing sources. Remove mandatory
  routine reports, copied handoffs, per-block status/percentage fields and parallel
  evidence inventories; preserve valuable evidence and unresolved gaps.
- Put normal design/ontology conformance and comprehensive affected E2E coverage in a
  concise completion paragraph for repository and packaged workspace agents.
- Reserve the short review skill for explicit deep/careful ontology review or an
  ontology/code conformance audit. Ordinary code review and “implement and review” use
  native methods. TLA+/TLC review remains separately opt-in.

Concrete wording is agent-authored under this authorization. Update maintained sources
and generated copies; no application changes or new experiments are requested.

## Twenty completion and model-to-ontology refinement

12 October 2026. The human asked to read the
[completed Twenty lifecycle study](../../../docs/experiments/uplift_downlift_lifecycle/twenty_downlift_lifecycle_study.md),
identify incremental harness improvements, and explicitly explain how TLA/TLC modeling
incrementally makes ontology descriptions more precise, including child blocks.
The earlier minimal-harness and explicit-request incremental modeling constraints remain.

Agent interpretation: clarify minimality within the required operating envelope, probe
public capability boundaries as ordinary consumers, adjudicate combined review findings,
and distinguish applicable proof/model evidence from unchanged kernels or saved counts.
Expand the existing model guide with the finding-to-contract refinement loop and the
WorkingSet example; refine existing children or add useful specification children under
concise parents. Reconcile consequential contracts before dependent implementation,
without turning model assumptions or finite bounds into new product requirements.

The changes use existing skills, references and ontology owners. They add no review
stage, mandatory scorecard, new modeling authorization or routine report. The study
supplies observations; local instruction checks do not establish better agent behavior.
