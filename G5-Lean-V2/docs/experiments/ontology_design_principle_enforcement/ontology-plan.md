# Ontology design principle enforcement

## Ontology changes and governance

**Ontology changes:** none in this initial experiment brief. Comparison base:
`412cccae8dcb78453745341bb544d1011c35b581` on
`ziniuw/ontology_design_principle_enforcement`. The existing ontology already requires
good abstractions, complete module operations, boundary enforcement and evidence-based
review. This document consolidates the research direction and plans the PR; it does
not yet select a replacement architecture or claim that the code satisfies those rules.
There are no ontology block or metadata diff hunks to report.

The [user's PR scope](../../../.g5/chat_intent/ziniuw/ontology_design_principle_enforcement/experiment-scope.md)
supplies the four workstreams below. Keep this file as the evolving PR plan. When the
investigation justifies a consequential design change, update the owning ontology
blocks and metadata, and include their actual Markdown diff here before implementing
the dependent refactor. Detailed findings and execution receipts can be linked artifacts.

**Relevant and governing ontology:** these are starting commitments, not an exhaustive
inventory for every eventual refactor. Implementation support remains unassessed by
this brief. Each selected slice must follow its additional feature and protocol owners.

| Governing commitment | Obligation and owner/operation | Allowed case and counterexample | Planning conclusion |
|---|---|---|---|
| [g5.ontology.design-principles; g5.ontology.formal-verification](../../../.g5/ontology/features/ontology_and_harness.g5.md#design-principles-invariants-and-policy) | Ontology records meaningful design and identifies suitable forms of verification. | A scoped model supports one contract; its success cannot establish whole-system correctness. | Keep specification, abstract checks and implementation evidence separate. |
| [g5.architecture.capabilities](../../../.g5/ontology/concepts/software_architecture.md#four-capability-owners) | Workspace, Agents, Worker & Host, and Git/governance own capabilities across processes. | Internal restructuring can preserve ownership; a crate split alone does not demonstrate it. | Start from these owners; justify any changed responsibility. |
| [g5.architecture.commands](../../../.g5/ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner) | State owners expose invariant-preserving operations or declared transaction phases. | A new caller invokes the owner; it must not assemble private policy, locks and writes. | Trace all relevant entry paths during the audit. |
| [g5.design.transaction-composition](../../../.g5/ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects); [g5.architecture.dispatch_phases](../../../.g5/ontology/software_architecture/agents.g5.md#dispatch-transaction-phases) | Owners retain validation and writes inside a coordinator's atomic operation. | Caller-owned graph transactions are valid; committing each child separately can violate the contract. | Encapsulation must preserve composition. |
| [g5.architecture.dependencies; g5.architecture.api_layers](../../../.g5/ontology/software_architecture/boundaries.g5.md#direct-dependencies-toward-shared-capabilities) | Shared capabilities remain independent of outer adapters; services cannot import routes. | A route calls a domain operation; a domain owner cannot depend on HTTP presentation. | Check dependencies and actual authority, including raw storage access. |
| [g5.design.abstraction; g5.design.information-hiding](../../../.g5/ontology/concepts/system_design_principles.g5.md#create-abstractions-around-stable-meaning); [g5.design.minimal-mechanism](../../../.g5/ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Interfaces hide meaningful variation and justify their reasoning/operating cost. | A provider can change behind its adapter; a forwarding layer with no protected decision needs justification. | Judge usefulness through concrete changes and substitution. |
| [g5.security.owner](../../../.g5/ontology/concepts/security.g5.md#one-eligible-owner-uses-live-compute); [g5.security.read](../../../.g5/ontology/concepts/security.g5.md#read-permission-is-explicit); [g5.security.admission](../../../.g5/ontology/concepts/security.g5.md#admit-actions-against-current-authority) | Live admission and durable reading retain distinct grants and current-state checks. | An administrator may have durable read access without permission to mutate another owner's VM. | Shared helpers must preserve legitimate authority differences. |
| [g5.architecture.recovery](../../../.g5/ontology/concepts/system_design_principles.g5.md#make-failures-and-retries-explicit); [g5.design.commit-notify](../../../.g5/ontology/concepts/system_design_principles.g5.md#publish-notifications-after-authoritative-commit) | Durable operations own commit, replay identity, recovery and notification order. | A lost reply reuses the same accepted action; a retry must not create a second protected effect. | Include failure and recovery challenges, not only successful calls. |
| [g5.quality.review.cases; g5.quality.review.guards](../../../.g5/ontology/concepts/coding_principles.g5.md#review-both-permitted-and-forbidden-behavior) | Review checks forbidden behavior, valid variants and checker calibration. | A guard rejects the intended bypass and accepts a harmless refactor. | A reject-everything implementation or brittle checker is insufficient. |
| [g5.quality.migrations](../../../.g5/ontology/concepts/coding_principles.g5.md#preserve-deployed-data-through-forward-migrations); [g5.quality.evidence.claims](../../../.g5/ontology/concepts/coding_principles.g5.md#distinguish-inspection-from-demonstrated-behavior) | Persistent evolution preserves deployed data; claims name their evidence. | Add forward migrations; do not alter historical SQL/checksums or call inspection a runtime pass. | Preserve compatibility and report unknowns explicitly. |
| [g5.workflow.ontology.skill-sources](../../../.g5/ontology/concepts/coding_principles.g5.md#maintain-one-source-for-workflow-skills); [g5.harness.implementation](../../../.g5/ontology/features/ontology_and_harness.g5.md#pin-the-design-during-regular-implementation) | `.agents/skills` owns workflow skills; implementation currently uses ordinary coding with the design pinned. | Improve existing planning/review guidance and code checks; a new mandatory coding harness requires an explicit contract amendment. | Avoid parallel skill copies or silently adding a workflow phase. |

## Purpose and intended PR outcome

Test whether explicit design commitments, useful abstractions and enforced module
boundaries make G5 Lean easier and safer for agents to change. The PR has four parts:

1. **Investigate good design deeply.** Evaluate the current codebase through several
   design principles, locate concrete problems, and establish why they matter.
2. **Encode the useful knowledge.** Determine which general guidance belongs in
   skills and which G5-specific abstractions, boundaries and obligations belong in ontology.
3. **Refactor and enforce.** Deliver substantive modularity improvements in G5 Lean,
   with skills and executable checks that help subsequent agents preserve them.
4. **Compare incremental development.** Measure agent efficiency and delivered code
   quality on the current codebase versus the refactored, selectively verified codebase.

The immediate deliverable is this consolidated plan. The audit, production refactors,
skill changes and experiments are planned work, not completed results. The final PR
must contain actual improvements and evidence; documentation alone does not fulfill it.
Feature removal, repository relocation and an unrelated backend rewrite are outside
this study. Retain existing product behavior unless a specific change is justified
and recorded in the governing design.

## Consolidated foundation

The three supplied documents contribute complementary ideas:

| Source | Contribution to this PR |
|---|---|
| [Formal verification and good design][formal] | Modeling exposes hidden assumptions about state, identity, preconditions, effects, recovery and composition. An abstraction should support independent reasoning by its callers. |
| [Enforcing good design][enforcing] | Connect normal agent work, focused skills, project contracts and executable enforcement. Evaluate actual subsequent changes and account for the cost of preparation and maintenance. |
| [Design principles in ontology][encoding] | Separate general principles, project commitments and evidence. Put each rule at its semantic owner and follow dependencies to select affected review scope. |

Use these conclusions throughout the work:

- **A boundary is a reasoning boundary.** Ask whether a caller remains correct with
  any implementation satisfying the public contract. Unspecified result order,
  latency or representation must not become a hidden prerequisite.
- **A small API must still be sufficient.** Callers should request complete domain
  operations. A generic executor, public pool or read trait inheriting mutation methods
  may expose much more authority than its name suggests.
- **Specify useful outcomes and effects.** Identify authoritative and derived state,
  preconditions and who maintains them until commit, required results, required secondary
  effects, permitted changes and state that must remain unchanged. The latter is the
  operation's *frame*. An allowed audit write is not a required audit event.
- **Composition has obligations on both sides.** Owners deliver promises, callers
  establish prerequisites, and the combined operation preserves system invariants.
  Include delegated effects, transaction boundaries and remote acceptance. An application
  responsibility cannot become an environmental assumption merely to simplify a proof.
- **Identity and lifetime are domain decisions.** A retry key must identify the business
  action across recovery. Captured permission or a capability does not necessarily remain
  valid after revocation, owner transfer or incarnation change.
- **DRY follows meaning.** Consolidate equivalent policy; preserve authorized variants.
  Similar predicates may legitimately govern different actors or lifecycle states.
- **Instructions and enforcement complement each other.** Skills guide choices; types,
  visibility, dependency rules, credentials and behavioral checks constrain actual paths.
  Changes that weaken a boundary or its checks need the same scrutiny as code changes.
- **Verification is scoped.** Separate design specified, abstract property checked,
  and implementation enforced. Test allowed behavior to avoid vacuous safety. Formal
  tractability alone is not evidence of simplicity or maintainability.

The bank examples in the sources are prior evidence for these questions. They do not
establish defects or improvements in this checkout. Older G5 examples refer to a
different architecture snapshot; their paths, policies and hypotheses must be mapped
to current owners and independently verified. The sources themselves describe some
G5 findings as source-traced hypotheses without service-backed reproduction.

## Workstream 1: audit the current design

Begin with a codebase-wide map of semantic owners, public interfaces, entry paths,
state/effect writers and dependency direction. Cover backend, frontend, workers and
their shared contracts; use risk and expected change frequency to select deeper probes.
Read source and tests during this investigation. Keep subsequent ontology-only design
exercises distinct, and do not describe discoveries informed by this audit as independent
source-free findings.

Use the twelve dimensions from the sources as questions, not twelve new policy pages:

| Dimension | Concrete G5 investigation |
|---|---|
| Requirements fitness | Do retained user flows, refusals and failure outcomes have clear owners and working paths? |
| Information hiding and modularity | Can a sibling caller bypass an owner using raw writers, connections or overly broad interfaces? |
| Simplicity and useful abstraction | Does each layer hide a decision or variation, or add forwarding and coordination without benefit? |
| Separation and dependency direction | Do presentation, domain policy, provider protocol and process custody remain separate? Where do cycles or reversed dependencies appear? |
| Domain coherence | Are task/attempt, owner/incarnation, delivery/outcome and durable/live meanings preserved across types and transitions? |
| Knowledge duplication | Which policies or mutable facts have independent definitions? Which apparent duplicates are legitimate variants or projections? |
| Ease and safety of change | Which plausible changes require callers to know private mechanisms or edit unrelated owners? |
| Data and distributed correctness | Do commit, replay identity, notifications and delegated effects form complete operations across races and partial failure? |
| Resilience and recovery | Who discovers unfinished work, makes progress and proves cleanup after restart or uncertainty? |
| Evolutionary safeguards | Which violations do current guards actually reject, and which harmless transformations do they obstruct? |
| Least privilege and complete mediation | Do all relevant entry paths enforce current grants, tenancy and credential scope? |
| Precise models and protocol assurance | Which ambiguous or high-risk protocols benefit from a small explicit model, and how would it correspond to code? |

Start deeper investigation around the current [module map](../../../.g5/ontology/concepts/software_architecture.md#module-map):
Workspace admission and lifecycle; Agents allocation, dispatch and completion; Worker
& Host custody; Git capture, receipts and retention; and shared platform authority.
Include frontend callers and projections where they duplicate policy or coordinate
recovery. These are candidate investigation areas, not confirmed defect locations.

For each material finding, retain **governing rule → actual path and owner → concrete
consequence → smallest useful repair → discriminating check**. Record the commit,
source locations, affected producers/consumers/sibling paths, reproduction and evidence
limits. Distinguish observation, hypothesis, confirmed cause, existing debt, intentional
amendment and optional improvement. Freeze initial findings before repairs.

Probe from the caller's position: a new entry path, a permitted alternate implementation,
an omitted secondary effect, a lost response and retry, or a small policy change. Pair
each forbidden case with a valid case and material exceptions. Prioritize by user impact,
boundary reach and likely maintenance cost; a file-size or dependency count alone is
not a finding. Record coverage gaps instead of claiming an exhaustive audit.

## Workstream 2: encode general and project-specific design

Keep three levels connected without making them duplicate authorities:

| Level | Home and content |
|---|---|
| Reusable guidance | Existing authoring/planning/review skills: design questions, substitution and composition challenges, review selection and evidence limits. |
| G5 commitments | Existing `.g5/ontology` owners: domain meanings, invariants, authority, chosen module responsibilities, exclusions, complete operations, allowed dependencies and reasons. |
| Enforcement and evidence | Production interfaces, tests, checks, selected models and experiment receipts linked to the owning commitments. |

Use [system design principles](../../../.g5/ontology/concepts/system_design_principles.g5.md)
for project-wide choices and [module pages](../../../.g5/ontology/software_architecture/boundaries.g5.md)
for concrete ownership. Keep domain identity in concepts, grants in security, user outcomes
in features, and atomicity/replay in the owning protocol. General principles become
project rules only when their obligation and applicability are clear. Reuse sufficient
contracts; do not expand every page or add a block per metric.

For each selected boundary, clarify what it owns and excludes; what callers can request;
what stays private; its governing invariants and authority; preconditions, useful outcomes
and effects; failure/replay/lifetime semantics; permitted collaborators; and the changes
that should trigger review. These are contract questions, not a required field schema.
Preserve one independently changeable commitment per I/D block, stable IDs, visible
conditions and exceptions, and paired metadata in the file's final `## Metadata` section.

Relationships should explain how a rule constrains an operation, supplies policy,
defines a legitimate variant or governs a consumer's assumptions. Search beyond links
when finding writers or callers: missing ontology links cannot exempt a source change
from review. Models and run logs stay outside the governing prose.

Deliver the selected ontology diff and concrete refactor direction in this plan before
dependent implementation. A genuinely unresolved design choice stays explicit; an
unknown source fact belongs in the investigation. Valid intent must not be weakened
to make the current implementation appear conformant.

## Workstream 3: refactor, enforce and selectively verify

Choose coherent slices from confirmed findings, ranked after the audit. For each slice,
make the owner/interface contract sufficient, move invariant-preserving operations to
that owner, migrate all material callers, remove bypasses and duplicate policy, and
exercise the production boundary. Include backend and frontend changes where the same
root cause crosses them. Avoid choosing crate count, service count or a full rewrite
as the target before establishing the responsibilities.

Select enforcement mechanisms for the actual obligation:

| Mechanism | Evidence to require |
|---|---|
| Private adapters, narrow public APIs and dependency restrictions | A forbidden caller cannot access private mechanisms; a valid new caller can complete its task. Prefer compiler/resolved boundaries to exact-text checks. |
| Domain types and restricted capabilities | Invalid construction or misuse is refused; mutable authority is checked at the required lifetime/commit boundary. |
| Complete commands and declared transaction phases | Checks and required writes remain atomic where required; external effects follow their own recovery protocol. Preserve caller-owned multi-owner transactions. |
| Database constraints and scoped credentials | The invariant survives relevant alternate writers under real runtime privileges; migration/bootstrap authority is evaluated separately. |
| Boundary tests and structural guards | Intended violations fail for the intended reason, while valid exceptions and harmless internal refactors pass. |
| Selected formal models or proofs | The exact property, assumptions, bounds and trusted components are stated; useful actions remain possible; a meaningful weakened guard yields a counterexample. |

For a formal candidate, choose a small consequential protocol after the audit, such as
replay/commit ordering, ownership fencing or receipt completion. Map its state and
transitions to the governing ontology and implementation. Preserve model commands,
results, counterexamples and correspondence limits. A bounded model check supports the
model; an implementation proof supports its encoded program and assumptions. Ordinary
tests alone must not be reported as formal verification. If no candidate is justified
or a check remains inconclusive, report that gap rather than calling the code verified.

Improve the existing skills with a short applicable procedure: identify owners and
governing contracts, invoke complete operations, investigate missing interface capability,
follow affected consumers, and challenge valid and forbidden behavior. Keep general
guidance in one maintained source. Add a separate skill only for a distinct demonstrated
need; produce client copies through the repository's existing mechanism. Shipped agent
profiles under `assets/agents` are product assets, not this checkout's development rules.

Check boundary-defeating changes to visibility, generic hooks, credentials, expected
outcomes and guard configuration. Restricting one API while leaving an equivalent raw
writer elsewhere does not close the finding. Recheck what the repaired invariant makes
newly possible, including races, retries and partial recovery.

Use [local validation](../../local-validation.md): run `just validation-recommend main`
for suggestions, select relevant preflight and focused runtime checks, prepare once
before several Docker slices, inspect receipts and clean up with `just ci-docker-down`.
Existing migration SQL and checksums remain immutable; additions require a new numbered
SQL file and SHA384 entry. Both `g5-ci-rust` and `g5-ci-e2e` must pass on the current PR
head before merge, including repair of inherited failures or keeping the PR open.

## Workstream 4: compare incremental agent work

The primary question is whether the **combined intervention** improves subsequent work:

| Condition | Starting point |
|---|---|
| Current | Frozen pre-refactor G5 Lean code, ontology, skills and existing checks. The initial candidate is `412cccae8dcb78453745341bb544d1011c35b581`; record the exact runnable snapshot before dispatch. |
| Improved | Frozen refactored code with matching ontology, improved skills, boundary checks and whatever selective verification actually completed. |

Both conditions receive identical incremental requirements, authoritative product policy,
acceptance outcomes and evaluation support. This comparison measures the package. It
does not isolate ontology formatting, modularity, skills or formal verification individually.
If resources permit, retain intermediate snapshots and add focused ablations: refactor
with existing versus improved skills; added model-derived checks with versus without
those checks; or ontology versus an ordinary document carrying the same knowledge and
procedure. Do not remove essential correctness mechanisms just to manufacture an arm.

### Freeze the protocol before running the comparison

1. Define a varied task suite: a new caller of an existing operation, a policy change
   with an exception, a provider/adapter change, a recovery change, a frontend consumer
   change, and a simple internal change that may not benefit from the extra process.
   Select executable tasks after the audit; keep some follow-up tasks genuinely withheld
   from refactor authors so the result tests transfer as well as known regressions.
2. Freeze requirements, governing obligations, valid/adverse acceptance cases, severity
   rubric, budgets, stopping rules and repair policy before dispatch. Keep evaluator-only
   fixtures and solutions outside worker context. Allow different sound implementations;
   do not grade exact filenames, wording or a preferred abstraction.
3. Pin each arm's commit, dependencies, toolchain, model/settings, tools and runtime
   configuration. Use fresh sessions and isolated worktrees/resources. Counterbalance
   task/condition order and record cache warmth and infrastructure failures. Do not leak
   prior solutions, findings or feedback between workers.
4. Run repeated attempts per task/condition under equal time/token budgets, including
   each condition's planning overhead. A proposed pilot is six tasks with three attempts
   per primary condition (36 runs); finalize the affordable run count and budgets before
   execution, and label a reduced pilot's uncertainty explicitly.
5. Freeze first submissions before evaluation or repair. Independently review them
   with the same criteria and support; retain initial findings before explanations or
   feedback. If repairs are allowed, use the same repair budget and report initial,
   repaired and subsequent-feature outcomes separately. A self-review is not independent.
6. Preserve commands, diffs, timestamps, available token/cost records, check receipts and
   review evidence. Record failure, timeout and unknown cases; do not discard inconvenient
   runs or substitute missing usage data with estimated precision.

Baseline behavior and new-task correctness need separate accounting. Classify pre-existing
defects, newly introduced regressions and defects repaired by the task. Shared tests should
assert equivalent behavior across both architectures; document necessary harness adaptation
without changing expected outcomes. A baseline infrastructure failure is not automatically
an agent-quality failure, but its diagnosis and delay must remain visible.

### Outcomes to measure

| Outcome | Measurement and interpretation |
|---|---|
| Task success and correctness | Fraction satisfying the frozen obligations; critical violations, regressions and unresolved failures shown separately. |
| Agent efficiency | Wall time, active work versus setup/test waits, input/output tokens and monetary cost when available, tool calls, repair rounds and human interventions. |
| Design quality | Validated bypasses, duplicated policy, leaked private knowledge, unnecessary abstractions, preserved exceptions and change locality. Counts support concrete findings; they are not quality by themselves. |
| Boundary strength | Intended forbidden cases rejected and valid controls accepted; report denominator, reason and enforcement layer. |
| Maintenance effort | Responsibilities/files touched, repeated policy edits, review/repair work and performance on a withheld follow-up. A smaller patch is useful only when it is correct and complete. |
| Verification cost and benefit | Model/translation/checking effort, defects or missed obligations exposed, correspondence work and ongoing maintenance cost. |
| Total investment | Audit, refactor, skill/ontology maintenance and evaluation setup reported separately from per-task savings. |

Use paired per-task results and distributions, not only one aggregate average. Publish
the obligation denominator and supported, violated, unknown and justified non-applicable
cases. Count shared causes carefully and keep critical violations visible. Report effect
sizes and uncertainty appropriate to the number of independent tasks; repeats on one task
are not additional independent task types. Call a small pilot exploratory.

Predeclare quality and efficiency decision thresholds in the executable protocol after
the pilot scope is known. A faster result with new critical violations is not success;
better correctness with higher cost is a tradeoff to report. Include break-even analysis
when estimating whether later task savings repay the initial refactor and maintenance.
Null or negative results are valid experiment outcomes.

## Sequence, deliverables and completion

| Stage | Deliverable and exit condition |
|---|---|
| Establish baseline | Pin runnable baseline and source provenance; inventory owners, existing checks and known validation failures. |
| Audit | Save concrete findings with paths and adverse/valid evidence; declare examined scope and gaps; rank refactor slices. |
| Encode and design | Update this plan and only necessary ontology owners/metadata; expose the actual diff, boundaries, preserved exceptions and open decisions. |
| Implement and enforce | Land coherent refactors and maintained skill changes; demonstrate selected bypass closures, valid behavior and calibrated checks; retain formal results and their limits. |
| Evaluate | Freeze improved snapshot and comparison protocol; run matched incremental tasks and preserve initial, repaired and follow-up evidence. |
| Conclude | Report improvements, regressions, efficiency/cost tradeoffs and remaining gaps; obtain current-head CI evidence before merge. |

Completion means evidence of real modularity improvements across the declared scope,
concrete enforcement that subsequent agents can use, and a reproducible comparison.
Claims about the whole codebase require correspondingly broad coverage. Remaining modules
or interactions must be named as unassessed or deferred; a successful narrow slice cannot
establish that all of G5 Lean is modular or verified.

The material choices still to resolve are which confirmed issues deserve the first
slices, whether existing interfaces need semantic amendments, which protocol merits
formal work, and the executable task suite/run budget. None has been resolved by a
code audit in this initial document. The study must also account for over-abstraction,
false-positive guards, lost authority exceptions, behavior drift, model/code mismatch,
benchmark leakage and the cost of extra instructions.

Planning self-review followed a second route through the
[invariant index](../../../.g5/ontology/concepts/system_invariants.g5.md) into
[current authority](../../../.g5/ontology/concepts/security.g5.md#admit-actions-against-current-authority),
[atomic allocation](../../../.g5/ontology/software_architecture/agents.g5.md#compose-allocation-through-the-owning-services),
[durability failure exceptions](../../../.g5/ontology/software_architecture/durability.g5.md#a-durability-failure-preserves-recoverable-work)
and the [callback-lock exception](../../../.g5/ontology/software_architecture/runtime.g5.md#release-callback-locks-before-exclusive-flush).
These constrain future refactors: complete operations may join caller transactions;
authority varies by operation and lifetime; cleanup/flush cannot be reduced to a blanket
lock or success rule. This is a review of the research plan, not independent implementation
review or executed verification.

## Source provenance

Consolidated on 2026-09-29 from local working copies in
`/Users/ziniuw/Desktop/projects/G5/g5_docs`, whose Git HEAD was
`6931f234c47c4941960f596393d9e2d01735101e`. The first file was untracked and the other
two modified; that commit does **not** identify the exact supplied text. The links below
open those local sources; the summary above stands on its own in this repository.
SHA256 fingerprints identify the versions read:

| File under `g5_docs/docs/lane_1_ontology_design/formal_verification/` | SHA256 |
|---|---|
| [formal_verification_and_good_design.md][formal] | `ab1ce3bc352c7573b7d7243eb34ce8718881ee30dc7e60b32c3edff1e4f2f3fd` |
| [enforcing_good_design.md][enforcing] | `cf71269d6ae080f37bfcd62064699d4d46836962664a893206ab855b2f3da883` |
| [design_principles_in_ontology.md][encoding] | `68bc5d7ff0a41effc0190dbefbe3c95f628a6b40a864df043d9351eadacf4e5e` |

[formal]: /Users/ziniuw/Desktop/projects/G5/g5_docs/docs/lane_1_ontology_design/formal_verification/formal_verification_and_good_design.md
[enforcing]: /Users/ziniuw/Desktop/projects/G5/g5_docs/docs/lane_1_ontology_design/formal_verification/enforcing_good_design.md
[encoding]: /Users/ziniuw/Desktop/projects/G5/g5_docs/docs/lane_1_ontology_design/formal_verification/design_principles_in_ontology.md
