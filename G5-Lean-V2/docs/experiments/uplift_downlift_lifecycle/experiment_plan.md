# Evaluating uplift: intent recovery, defect discovery and simpler designs

14–16 October 2026 · Protocol and execution record. The first PairSoft E1 baseline pair, four-run lifecycle harness pilot and multi-domain coverage-feedback continuation comparison are complete. See section 10 and the completed execution results below. Their results do not establish reliable semantic completeness or consistent overall superiority. The user has authorized the operational discovery and reconciliation experiments specified at the end; fixture preparation and unscored calibration are underway.

The study has three separate experiments: **initial intent recovery**, **ontology/code reconciliation**, and **hard-to-find code defects**. The original full-workflow baseline remains a separate end-to-end comparison, including simplification at the end. The component experiments use fresh runs and controlled starting artifacts; they are not successive stages whose outputs silently differ between methods.

## 1. What advantage are we actually claiming?

The defensible hypothesis is that G5 makes a capable agent's investigation more reliable and its results easier to reuse: important commitments remain explicit, discrepancies have evidence, omissions are challenged independently, and selected behavioral claims receive executable checks. We have not established that an agent without G5 cannot discover the same intent, bugs or architecture.

Three contributions need separate measurement:

1. **The investigation process:** reconcile sources, distinguish intent from implementation, inspect alternate paths, challenge breadth and preserve uncertainty.
2. **The ontology:** a compact, persistent contract with semantic owners, stable references, conditions, exceptions and provenance. This is structured prose, not automatically an executable specification or proof.
3. **TLA+/TLC:** construct explicit transition models and mechanically check selected properties. Specification construction and executing the checker are separate sources of value.

A good requirements document can perform much of the ontology's role. A capable agent can reason about races, propose simpler ownership and use tests without TLA+. Other model checkers can provide mechanical exploration. The question is therefore **which contribution improves which outcome, at what cost, and how consistently**, rather than whether these activities are exclusive to G5.

### Technical mechanisms and their limits

| Mechanism | Concrete benefit to test | What the alternative can already do | What this mechanism cannot establish |
|---|---|---|---|
| Separate supported intent, observed behavior and provisional policy | Avoid turning a bug into a requirement or a preferred policy into historical intent | Ordinary source analysis and requirements engineering can make the same distinction | Recover a human decision absent from all available evidence |
| One semantic owner per commitment, with conditions and references | Reduce contradictory copies; make cross-feature obligations discoverable; preserve meaning through compression and handoff | A well-maintained requirements/ADR system can do this without G5 syntax | Consistency or completeness merely because IDs and links exist |
| Independent capability inventory and reconstruction challenge | Find important behavior omitted from both the initial contract and model set | A strong baseline can inspect endpoints, jobs, data and documents independently | Prove that no unknown requirement remains |
| Explicit state, identity, authority and interruption boundaries | Expose missing premises while specifying behavior: which submission, which snapshot, whose lease, which commit | Careful code review, pseudocode and stateful tests can expose the same issues | Establish that the chosen abstraction faithfully represents production |
| Executed model checking | Search modeled orderings systematically; retain counterexamples; repeat checks after a change | An alternative checker or exhaustive executable state model can provide analogous evidence | Verify omitted actions, unmodeled requirements or all production schedules |
| Safety plus legitimate completion and progress checks | Reject a repair that only succeeds by preventing useful work; expose stalls under explicit environmental premises | Tests can exercise successful flows and selected failure schedules | Infer eventual completion from one successful trace |
| Reuse the contract while comparing simpler mechanisms | Make lost behavior and added assumptions visible; assess fewer independent states/owners against the same obligations | A strong designer can do this from ordinary requirements | Generate the globally simplest architecture or prove minimality from TLC state counts |

The sharpest difference from unaided agent reasoning is **mechanically repeatable exploration of the specified state graph**, when the selected finite model can be fully explored. An agent's plausible narrative does not supply that evidence. This difference belongs to executable verification, not specifically to G5 ontology. TLA+ describes state transitions and behavioral properties; TLC checks safety and temporal properties of configured models. Model fidelity and requirement adequacy remain separate obligations. See [Lamport's high-level explanation](https://lamport.azurewebsites.net/tla/high-level-view.html), the [TLA+ tools reference](https://lamport.org/tla/tools.html) and [TLC's model/property configuration](https://dl.tlapl.us/tlatoolbox/doc/model/overview-page.html).

Without a persistent contract, an agent may lose exceptions or change meanings across a long investigation; without mechanical exploration, it may miss combinations of events. These are **reliability hypotheses**, not demonstrated impossibility results. Even with both, it can confidently miss a requirement shared by neither its contract nor its model. Measurement must include those failures.

## 2. What the existing studies actually show

The process definition comes from the [uplift skill](../../../.agents/skills/ontology-uplift/SKILL.md), its [source guide](../../../.agents/skills/ontology-uplift/references/intent-and-design.md), [execution/coverage guide](../../../.agents/skills/ontology-uplift/references/execution-plan.md), [authoring skill](../../../.agents/skills/ontology-authoring/SKILL.md) and [model-checking guide](../../../.agents/skills/ontology-planning/references/model-checking.md). These are references for this experiment design; this document does not start an uplift or downlift campaign.

The [PairSoft/Twenty uplift retrospective](../ontology_design_principle_enforcement/pairsoft/uplift_pairsoft_twenty.md) and [PairSoft lifecycle study](pairsoft_lifecycle_study.md) supply concrete examples, but not controlled comparisons:

| Example | Concrete contribution | Limit and experimental implication |
|---|---|---|
| **PairSoft: delayed approval approves edited content** | “Approve the requisition” became approval of a particular submission and reviewed content. The model explored submit A → approve A → edit to B → deliver old completion. A service replay then observed edited content being approved. | The replay supplied approval events, so it was a partial distributed reproduction. A code reviewer could also find the missing correlation. Measure whether each arm discovers and validates the issue, not whether it can explain the known answer. |
| **Twenty: restore races with trash cleanup** | The model separated selection of eligible IDs from destruction. Restoration between those operations invalidated the earlier decision; a proposed guard also needed to permit genuine cleanup. | The initial result was a source-bound model concern, not a runtime reproduction. Compare detection, model/code correspondence and independent execution separately. |
| **PairSoft receipt tolerance; Twenty quota exceptions** | Original decisions corrected overly strict extracted rules. Preserving legitimate exceptions improved fidelity and prevented false bug reports. | This was mainly source reconciliation. A checker cannot decide the intended business policy. Include policy-exception cases where stricter behavior is wrong. |
| **PairSoft: ordered outgoing effects** | A proposed per-purchase FIFO replaced per-job dependency relationships while preserving required reversal-before-replacement ordering in checked cases. | This was later design work, not original-intent recovery. It introduced head-of-line waiting. Compare retained functions, concurrency and operating limits before calling it simpler. |
| **PairSoft: lost mail during restore; Twenty: later-discovered capabilities** | Independent runtime/breadth investigation found omissions despite earlier specification and verification work. | Include obligations absent from the author's models and meaningful recovery/progress cases. Passing the author's own assertions cannot be the evaluation oracle. |

PairSoft's initial uplift inherited ontology. Its later replacement narrowed scope and changed policy. Twenty began without ontology, but later audits withdrew complete-backend coverage claims. PairSoft's recorded model corrections demonstrate that executable models can also be wrong. Neither study isolates ontology, modeling, checker feedback, extra effort or human intervention.

The [earlier design comparisons](../ontology_design_principle_enforcement/design_comparison_study.md) and [PairSoft experiment takeaways](../ontology_design_principle_enforcement/pairsoft-harness-experiment-takeaways.md) additionally document competitive plain-agent designs, run-to-run variation and contaminated controls. We should use these cases to form hypotheses, not treat their reported findings or counts as results of the new experiment.

## 3. Separate questions and outcomes

| Experiment | Question and hypothesis | Primary evidence | Evidence against the claim |
|---|---|---|---|
| **E1 — Recover a reusable contract** | Does G5 recovery produce a more faithful, comprehensive, concise and minimal account than code summarization or ordinary recovery from all sources? | Supported intent, preserved exceptions, unnecessary content, repetition, artifact size and fresh-reader reconstruction. | The all-source baseline captures the same meaning with less prose and equal or better reader accuracy. |
| **E2 — Refine ontology against code** | Starting from the same ontology, does TLA-assisted reconciliation improve precision and correctly decide whether ontology, code or both are wrong? | Correct ontology amendments, correct divergence classifications, preserved valid rules and remaining ambiguity. | Direct review makes equally good corrections, or formal work introduces unsupported restrictions or rewrites valid intent to match buggy code. |
| **E3 — Find hard-to-find bugs** | Given the same fixed ontology, does TLC-assisted investigation find more independently validated code defects than direct review? | Unique validated defects, especially temporal/concurrency/recovery cases, detection consistency, false positives and cost. | Direct review finds the same defects more reliably or cheaply; extra formal findings are model errors or unsupported concerns. |
| **W — Full workflow** | Does the integrated uplift improve the original three outcomes over the user's entire-workflow baseline? | Intent quality, validated defects and credible simplification opportunities at retained scope. | The plain workflow matches or exceeds the package, or apparent simplification merely removes required behavior. |

E1 changes source access deliberately in one baseline. E2 and E3 hold the starting ontology, code and supporting sources constant. E2 permits ontology correction; E3 evaluates code against a frozen contract. Their practical comparisons include modeling and checker execution together. An optional shared-model experiment isolates the additional contribution of executing TLC; discovery timing alone cannot establish that causal contribution.

Conciseness and minimalism are primary E1 criteria, alongside fidelity and coverage. More blocks, more rules or more formal detail are not inherently better. Keep the outcomes separate rather than combining them into a single winner score.

## 4. Dataset and isolation

Use **G5-Lean, PairSoftPlatform and Twenty**, each stripped of inherited ontology and agent harness instructions. E1 and W start from those stripped repositories; E2 and E3 deliberately receive one frozen starting ontology, identical within each comparison. Proposed primary scope is backend behavior, including server-side semantics supporting UI features. UI/UX assessment is a separate extension. Confirm scope and source availability before launch; supply the same confirmation within each matched comparison, with E1's code-only source restriction explicitly declared.

| Repository | Proposed starting point | Material qualification |
|---|---|---|
| G5-Lean | Select a product revision before benchmark preparation; record its full commit ID and included components. | This planning checkout is G5-Lean-V2, at `a95ae38d046c74dfaf7c862fffc5a960fe7212a0` when read. That is provenance for this plan, not an automatically selected dataset revision. |
| PairSoftPlatform | Consider historical baseline `b9613c09`, with PairSoftCore 2.0.10 available equally to all arms. Resolve and pin the full commit IDs/package hashes. | That imported snapshot reportedly has limited useful history. Do not give one arm later ontology or treat later branch policies as applicable without evidence. |
| Twenty | Consider application revision `2f9bd3aa6bcffe2095beeff19296d53a43819188`. | Preserve genuinely available upstream documentation/history for the revision. Exclude later uplift/downlift results and distinguish later documentation from contemporaneous policy. |

### Prepare one evidence universe per repository

- Preserve application code, tests, schemas, migrations, original product/design records and available historical decisions. Give matched arms the same dependencies, runtime fixtures and source access, except for E1's intentional code-only versus all-source contrast.
- Remove inherited `.g5` ontology, G5 skill/instruction packages, agent memory, experiment outputs, prior G5-generated model/proof artifacts and derived bug/design summaries from author access. Check ordinary docs, generated indexes, caches and task histories for copies. Preserve independent original design/verification records equally across arms when they belong to the agreed source universe. This plan and the retrospectives are evaluator-only.
- Sanitize reachable Git history, refs, commit messages and linked artifacts too. Deleting working-tree files while leaving them recoverable with `git show` is insufficient. Retain useful uncontaminated history and an evaluator-held original-to-sanitized revision map. Audit the sanitized history; do not solve leakage by withholding all history from only one arm.
- Separate instructions for the agent from product functionality. Removing a harness package must not accidentally remove application behavior under evaluation. If G5 runtime functionality embeds those assets, document the boundary/redaction and verify the benchmark application still operates; narrow scope explicitly if that cannot be achieved.
- Run authors in isolated environments without access to parent checkouts, prior chats, global G5 skills or other arms' files. Start fresh author contexts; this planning conversation cannot serve as an experimental author context. Record the exact G5 skill files/hashes injected only into the appropriate treatments.
- Pin source snapshots and a source cutoff. For all-source arms in the primary comparisons, provide searchable archived external records with their original context, corrections and provenance. Authors receive the corpus, not the evaluator's extracted answers or capability inventory. E1's code-only arm cannot retrieve these records or Git history.
- Preserve the user's “any external artifact they can find” baseline in a **secondary open-discovery comparison**, using the same access permissions across arms and logging every retrieval. Do not pool it with the fixed-corpus primary comparison; different discoveries are part of its treatment outcome.

These repositories already informed the workflow. They are development cases even after sanitization, not an untouched generalization set. Public-source familiarity may also survive context isolation. Report that limitation and reserve additional repositories for later external validation; do not claim three repositories establish universal superiority.

## 5. Three component experiments and the full-workflow baseline

These experiments replace the earlier five-arm comparison as the primary decomposition. Within each experiment, match the application revision, scope, agent model/settings, resources and ordinary application/test tools. E1 deliberately varies source access. Production repairs and replacement implementations are outside these experiments.

### E1 — Initial retrieval: concise, minimal and reusable intent

**Starting point:** stripped repository, no starting ontology. Compare three arms:

| Arm | Inputs and instruction | What the contrast measures |
|---|---|---|
| **E1-Code — Code-only baseline** | Current application code, comments, tests, schemas and configuration. Ask the agent to summarize the code and find the important intent behind it. No product documents, historical discussions, Git history or external sources. | The ordinary code-summarization baseline. |
| **E1-Sources — All-source baseline** | The same code plus all supplied original documents, available Git history and external records. Ask the agent to read those sources and recover the important intent behind the code. No G5 workflow or required format. | E1-Sources versus E1-Code measures the value of additional sources. |
| **E1-G5 — G5 intent recovery** | Exactly the E1-Sources evidence universe, using G5 source reconciliation, capability discovery, ontology authoring, coverage challenge and compression. | E1-G5 versus E1-Sources measures the recovery process and artifact together, with source access held constant. |

All three receive the same concise-output objective. None builds TLA models or executes a model checker in E1. E1-G5 is a recovery-only ablation, not a claim to have completed the full uplift skill. Incidental bugs can be recorded, but bug hunting and redesign are not this experiment's objective.

**E1-Code prompt:**

> Use only the supplied current code and its comments, tests, schemas and configuration. Read and summarize this material, and identify the important intent behind the code: what the system is for, its capabilities, rules, important exceptions and consequential design choices. Produce a concise, minimal account useful to another engineer. Preserve important meaning, avoid repetition and incidental implementation narration, and distinguish supported intent from inference or uncertainty. Reconcile conflicting evidence, identify important coverage gaps and report supplied material you could not inspect.

**E1-Sources prompt:**

> Use the supplied current code and its comments, tests, schemas and configuration, plus all supplied documentation, available Git history and other original sources. Read and summarize this material, and identify the important intent behind the code: what the system is for, its capabilities, rules, important exceptions and consequential design choices. Produce a concise, minimal account useful to another engineer. Preserve important meaning, avoid repetition and incidental implementation narration, and distinguish supported intent from inference or uncertainty. Reconcile conflicting evidence, identify important coverage gaps and report supplied material you could not inspect.

E1-G5 receives the same objective plus the frozen recovery-only G5 instructions. Baselines may use headings, references or a requirements ledger naturally; do not force them into G5 syntax. Score fidelity, breadth, conciseness and minimalism jointly. E1-G5 beating E1-Code alone cannot establish a workflow advantage because its source access is richer. E1-G5 versus E1-Sources still does not isolate document syntax from investigation discipline.

### E2 — Check and refine the same ontology against code

**Starting point:** the exact same initial ontology `O0`, application revision and supporting source corpus. Both arms can amend the ontology, report code defects and decide for themselves which side is correct when they diverge. No requirement to ask a human to resolve each discrepancy, and no presumption that either the supplied ontology or current code is authoritative in every conflict.

| Arm | Instruction | Output |
|---|---|---|
| **E2-Review — Direct reconciliation baseline** | Check the ontology against code using ordinary investigation and tests; judge disagreements from the available evidence. No TLA/model-checking campaign. | Revised ontology and a concise divergence ledger explaining ontology corrections, code issues and remaining uncertainty. |
| **E2-TLA — TLA-assisted reconciliation** | Perform the same task using code-bound TLA models and TLC checks to challenge behavior and refine ontology; investigate whether counterexamples reflect code, ontology or model errors. | The same deliverables, with supporting models/check results kept outside the readable contract. |

**Baseline prompt:**

> Check this ontology against the code. Fix what is wrong, incomplete or imprecise in the ontology, and report issues in the code. When they diverge, use your own judgment and the available sources to determine whether the ontology, the code or both are wrong; explain the evidence for your decision. Preserve correct intent and legitimate exceptions, keep the ontology concise and minimal, and distinguish unsupported choices from recovered intent. If evidence cannot settle a choice, state your best-supported provisional judgment and uncertainty. Do not change production code.

The treatment receives that same task plus the formal modeling/checking instructions. Evaluate the **change from `O0`**: resolved material ambiguities, correct amendments, correct assignment of faults, unjustified changes to previously correct rules, remaining errors and added prose. Bugs are a secondary output; E2's main question is whether the resulting contract is more precise and correct. Include cases where the ontology is wrong, code is wrong, both are wrong, an exception is legitimate, and neither needs changing.

This comparison tests TLA-assisted reconciliation as practiced, including TLC feedback. Retain the pre-execution model/ontology snapshots to examine specification-construction contributions. Do not attribute all improvement to writing TLA alone; section 6 provides an optional narrower checker comparison.

### E3 — Find hard-to-find code bugs from the same ontology

**Starting point:** identical fixed ontology `O0`, code and supporting sources in fresh runs. Reuse the same shared input packet as E2 where appropriate, but do not feed each method its own E2-refined ontology. Neither arm gets previously discovered findings, model files or counterexamples.

| Arm | Instruction | Primary output |
|---|---|---|
| **E3-Review — Ontology-guided code review baseline** | Review code against the supplied ontology and report bugs using ordinary investigation and runtime tests. No TLA/model-checking campaign. | Evidence-backed defect report. |
| **E3-TLC — TLC-assisted bug finding** | Review against the same ontology, build faithful as-built models, execute TLC and investigate/reproduce the resulting concerns. | The same defect report, with model and runtime evidence. |

**Baseline prompt:**

> Review the code against this ontology and report the bugs you find. Investigate ordinary behavior as well as interactions, concurrency, retries, failures and recovery. For each finding, identify the violated obligation, reachable trigger, affected code and evidence; reproduce it where feasible. Treat the supplied ontology as the fixed review contract. If a requirement appears mistaken or uncertain, flag that separately rather than changing it or counting the disagreement automatically as a code bug. Do not change production code.

Give E3-TLC the same bug-finding objective and scope, with explicit model/checker instructions. Both arms may create diagnostic tests and use real local dependencies under equal resources. This is not a comparison against a baseline denied testing or told to ignore difficult paths.

Measure **additional independently validated defects and detection reliability**, especially hard-to-find cases, rather than model counts or whether a report sounds more precise. Predeclare difficult-case categories: multi-actor orderings, delayed/stale authority, crash/commit boundaries, duplicate effects and progress across recovery. Also report the per-defect detection rate across repeats. Do not define a bug as “hard” merely because TLC found it. Include ordinary defects and legitimate-policy negative controls to measure breadth and false positives.

E3-TLC versus E3-Review measures the practical value of modeling plus mechanical exploration for bug finding. It does not by itself isolate checker execution from the reasoning used to construct the model. A useful later alternative directs the same budget toward production-code property-based testing and fault injection; it is not required for the first E3 comparison.

### W — Preserve the original full-workflow comparison

Run **W-Plain**, the user's whole-workflow baseline, against **W-G5**, the full frozen uplift workflow. Both start without ontology, have the same code, Git history and external-source access, recover/refine intent, question coverage and record code bugs throughout. After recovery output is frozen, both receive the same simplification task.

**W-Plain prompt:**

> Use the codebase, available Git history and other sources to distill the important intent behind this software. Check the code to confirm or refine that intent, question its coverage and investigate what is missing. Document code bugs throughout. Keep the resulting account concise and minimal while preserving important rules and exceptions. Distinguish intended behavior, observed behavior and uncertainty. Do not repair production code. Finish the intent/bug investigation before proceeding to the separate simplification task.

W-Plain chooses its own organization and tools; if it independently uses ontology-like prose or formal tools, retain and report that crossover. W-G5 receives the uplift skill and explicit authorization for the later formal design comparison. The primary controlled version uses a shared frozen source corpus; a separate open-discovery version preserves access to any external artifacts the agent can find. Do not pool these versions.

**Common final task:** offer up to three concrete simplification opportunities, prioritizing preserved behavior, and separately identify useful partial-scope alternatives. State removed mechanisms, retained/lost obligations, changed policy, new premises, capacity/concurrency limits and migration/recovery consequences. No preferred technology or architecture is supplied. This preserves the original simplification evaluation without adding redesign to E1–E3.

## 6. Shared inputs, execution and attribution

### Freeze a fair starting ontology

For E2/E3, select a natural initial recovery snapshot before formal refinement, using a predeclared selection rule rather than choosing the output most favorable to G5. It can come from an independently prepared draft or a frozen E1 result, but strip discovered-bug annotations, model artifacts and post-review corrections. Keep its run/method identity and evaluator diagnoses private; preserve source provenance needed to interpret its claims. Give both arms byte-identical contract content and code/source versions, including necessary governing links and uncertainty. Preparing this shared input is evaluator setup, not an arm's hidden advantage.

A single G5-authored starting contract could favor its own method. Where feasible, repeat on a non-G5 recovery snapshot as a separate input stratum. Match methods within each `O0`; never compare treatment on one author's ontology with baseline on another's. Assess initial defects/ambiguities privately before the run and do not expose their locations. If a draft contains a report of a code issue, remove that report from all author packets or mark the issue as disclosed and exclude it from new-discovery credit.

### Common execution boundaries

1. **Freeze inputs and evaluation.** Save experiment/arm IDs, dataset and `O0` hashes where applicable, source scope, prompts, model/settings, tool versions, budgets and hidden reference material before scored runs. Source-confirmation requirements apply at setup, not as repeated evaluation hints.
2. **Run independently.** Use fresh contexts and the assigned method. No arm sees another's output. No evaluator coaching or discretionary clarification during investigation; agents judge from the supplied evidence and record provisional conclusions where necessary. Shared infrastructure fixes are logged and applied consistently.
3. **Save the experiment's output.** E1 produces an initial intent account; E2 produces a revised contract and divergence ledger; E3 produces a bug report against frozen `O0`; W freezes recovery and bug findings before design. Partial completion and unread sources remain visible.
4. **Evaluate independently.** Apply matched questions, source checks, reproduction standards and validation budgets. Save author claims separately from evaluator findings. A timeout is a result, not permission to claim a workflow completion checkpoint.
5. **Keep design separate.** Only W receives the final simplification task. A later design-only study with a common adjudicated requirements packet can separate recovery quality from design reasoning. No replacement implementation is part of the present proposal.

### Optional isolation of checker feedback

In E2-TLA and E3-TLC, retain timestamped finding ledgers and model/ontology snapshots before the first checker execution. Record the first observation of an issue, not just the final report date. Attribute contributing evidence to source reconciliation, code inspection, specification construction, TLC feedback, runtime reproduction or coverage review; allow several contributors. These records explain the investigation but are not a causal experiment on their own.

For a smaller mechanism experiment, fork the same **pre-execution** model, claims and code bindings into fresh continuations. One may execute TLC; the other gets the same budget for manual model/code inspection and runtime tests but cannot execute a checker or equivalent exhaustive state explorer. A shared parser-only check can remove syntax repair as a confound. Select concerns before seeing outcomes and include passing/exception cases. Compare additional validated findings after the shared snapshot. This isolates checker feedback conditional on an existing model; report model-construction cost separately.

For each consequential formal result, retain source/model/configuration identities, actual guards and interruption boundaries, checked property, bounds, assumptions, outcome and trace/log. Inspect competing writers and model composition. Distinguish an as-built violation, repaired candidate, deliberately broken control and legitimate completion witness. A removed guard is a sensitivity test, not a newly discovered production defect. A reachable success is not a liveness proof; eventual-progress claims need appropriate temporal checks and justified fairness/recovery premises.

## 7. Independent reference and scoring

### Build a reference without making old ontology the answer key

Before scored runs, evaluators derive a stratified reference from original decisions, product records and actual behavior paths. Keep capability discovery independent of author outputs. Cover ordinary workflows, narrow exceptions, permissions, integration, asynchronous work, failure/recovery and cross-feature interactions; include simple local rules so a concurrency-focused method cannot win by ignoring them.

Each reference obligation has a source locator, applicable revision/scope, actor/outcome, material conditions/exceptions, evidence status and consequence weight. Separate supported intended behavior, observed behavior with uncertain intent, explicit deferrals and unresolved policy. Do not require the historical implementation mechanism when alternatives are permitted. Where intent is unknowable, correct uncertainty is the expected answer.

Use the old studies only as evaluator leads, then revalidate relevant claims against the selected source universe and application revision. They are neither complete ground truth nor author inputs. For natural bugs, create a private independently validated reference set where possible. Keep seeded faults in a separate diagnostic track.

Two reviewers independently assess material claims with method labels removed where practical, then adjudicate disagreement using source evidence; a domain owner may resolve real policy ambiguity after author outputs are frozen. That adjudication is evaluation, not live assistance to agents deciding which side is correct. At least one reviewer should be independent of workflow authorship. LLM judgments may assist but are not the sole oracle. Document residual unblinding from artifact style. Show both reviewer agreement and unresolved reference questions.

An author may discover a valid obligation or defect absent from the frozen reference. Validate it independently, version the expanded reference and rescore **all arms** against it. Report frozen-reference results and expanded-reference results separately to avoid rewarding only agreement with the evaluator's initial view.

### Intent recovery and reusable contract: E1, also W

| Measure | Operational test |
|---|---|
| **Fidelity / claim precision** | Check each material assertion against the applicable source. Count unsupported strengthening, incorrect attribution and lost exceptions. A new recommendation earns no credit as recovered intent; a labeled uncertainty is not a false claim. Split bundled prose into semantic claims for scoring. |
| **Coverage / comprehensiveness** | Match the output against independently established obligations and capability families. A mention earns no obligation credit; the prose must preserve its meaning. Report both overall and per-domain coverage, including failure/recovery and cross-owner rules. |
| **Conciseness at retained meaning** | Plot supported obligation coverage and reconstruction accuracy against total maintained prose words. Count the main artifact, companion documents and metadata, deduplicating identical files but not repeated prose. Also show the reader-facing contract alone. A shorter incomplete account is not a compression win. |
| **Semantic minimalism** | Identify duplicate commitments, incidental code narration, generic advice and unnecessary implementation restrictions. Ask whether a statement or abstraction can be removed/merged without losing important intent, rationale, exceptions or reader accuracy. Fewer headings or shorter sentences alone earn no credit. |
| **Consistency and uncertainty** | Count materially contradictory rules, inconsistent provisional policies and claims of certainty unsupported by sources. Check whether intent, code conformance and evidence completeness remain distinguishable. |
| **Reconstruction usefulness** | Give a fresh reader only the recovered contract, including its governing linked prose but no application code, models or raw source records. Answer hidden behavior questions and derive acceptance scenarios; compare with independently prepared expectations. Allow “not specified/uncertain.” Score correctness, justified uncertainty and time to locate the answer. |

For coverage, score an obligation `1` when all material meaning is preserved, `0.5` for a predefined partial case and `0` when absent or contradicted. Weighted coverage is `sum(weight × score) / sum(weight)` over the adjudicated reference; report unweighted coverage too. Freeze consequence weights before author runs and show critical omissions separately. Evaluate claim precision separately so verbose speculative contracts cannot gain recall without a penalty.

Report **nonduplicated supported obligations per 1,000 prose words**, removable/redundant prose, and fresh-reader time alongside precision and recall. Density cannot substitute for coverage or readability: compare compactness at comparable retained meaning, and identify tradeoffs when one account is shorter but omits more. Evidence prose and metadata count toward maintenance size; show raw logs and executable model/test artifacts separately rather than hiding repeated explanation in appendices. Give all E1 arms the same conciseness/minimalism instruction.

For E1-Code, report both the common all-source reference score and the subset recoverable from its allowed code evidence. An obligation documented only in an unavailable human decision is an information disadvantage, not proof that the agent failed to reason. E1-Sources versus E1-Code quantifies that source-access effect; E1-G5 versus E1-Sources is the matched-source method comparison.

Hidden reconstruction questions should discriminate real alternatives: can an old approval authorize edited content; may an administrator use a documented exception; can retention cleanup destroy a record restored after selection; what happens to required effects during pause/restore? Include ordinary feature details unrelated to formal-method strengths. Known retrospective examples belong in pilot/calibration material; scored challenges also need undisclosed paths and interactions.

Use repeated fresh readers or blinded human tasks, matched in skill and budget across artifacts. Test whether an incorrect but plausible replacement could obey every written rule while omitting required behavior. No full downlift is necessary for this first evaluation. A later implementation-from-contract study would measure a different, stronger outcome.

Report coverage **of the adjudicated reference**, not a percentage of all original human intent. Source files read, model links and exercised endpoints are supporting breadth evidence, not substitutes for semantic coverage.

### Ontology refinement and correct reconciliation: E2

Compare each revised contract with the shared `O0`, using independently adjudicated source/code evidence rather than the author's own formal assertions. Score:

- **Corrected meaning and useful precision:** missing identities, conditions, exceptions, authority, timing or failure obligations now expressed accurately; material ambiguity resolved without inventing a requirement.
- **Correct disagreement judgments:** ontology error, code defect, both, legitimate exception, model/binding error or unresolved policy. Track erroneous ontology corrections and false code accusations separately.
- **Preservation:** valid original commitments and legitimate outcomes retained; no weakening to fit buggy code and no unjustified strengthening that blocks supported behavior.
- **Residual error and minimalism:** material gaps still present, newly introduced contradictions, unnecessary prescriptions and prose growth per supported clarification. More formal-looking detail is not itself improvement.

Use a per-discrepancy record: `O0 claim → observed disagreement → agent judgment and evidence → ontology amendment or code issue → independent verdict`. Score genuinely unknown intent as unknown; an evidence-labeled provisional decision is not recovered history. Include unchanged correct rules as negative controls so unnecessary editing cannot improve the score. Report contract quality before and after, not only the number of edits. Apply E1's fresh-reader questions and conciseness measures to assess whether added precision remains usable.

E2's code findings use the evidence standards below, but do not substitute defect yield for ontology refinement quality. E2 output never becomes an arm-specific input to the main E3 comparison.

### Existing-code defects: E3, also W and E2's secondary findings

A finding needs a precise violated obligation, affected code/revision, reachable trigger, impact and evidence status. Deduplicate by root cause and necessary repair, retaining affected obligations as separate impact annotations. Do not count repeated traces, configurations or symptoms as separate bugs.

| Evidence class | Treatment |
|---|---|
| **Reproduced production-path defect** | Primary runtime-confirmed yield. Execute the relevant real code with independent assertions; retain environment, substitutions and missing boundaries. Distinguish component, integration and full E2E scope. |
| **Independently confirmed static defect** | Report separately; not every real defect needs or permits a runtime reproduction. Require a supported contract and convincing reachable code path. |
| **Source-bound model concern** | Useful candidate, but no runtime-confirmed credit solely from a counterexample. Inspect abstraction/code correspondence before upgrading it. |
| **Conditional-policy gap** | Depends on an unresolved or newly chosen policy. Report separately from defects against established intent. |
| **Rejected / unresolved** | Explain model errors, invalid test doubles, stale sources, accepted behavior and unavailable evidence. Inconclusive does not mean false or confirmed. |

Report unique validated defects, severity/impact distribution, false positives, unresolved fraction, time/cost to first validated finding, and yield per resource budget. Report recall only against the known validated natural-defect set; unknown total defects prevent a true whole-repository recall claim. “New discovery” means absent from the evaluator's initial set, not globally unprecedented.

For E3, report the paired difference in validated yield, the overlap and unique findings of each method, and each defect's detection frequency across repetitions. Break these out by the predeclared difficult-case categories and ordinary defects. A defect found in only one treatment run is evidence of a discovery, not yet of reliable superiority. Do not count a code/ontology disagreement as a defect until its governing requirement and reachable violation are supported. Contract disputes remain separate because E3 cannot amend `O0` to change its own target.

Provide every arm equivalent local reproduction capability and author time. An independent validator then receives the same per-finding validation ceiling across arms. Count that extra validation labor separately; methods generating many weak candidates must not receive unlimited invisible follow-up work. Keep author-validated yield and evaluator-validated yield distinct.

Use seeded faults only to diagnose sensitivity: stale identities, split commits, omitted alternate writers, duplicate delivery, missing aggregate checks and stalled recovery. Include unmodified negative controls and legitimate exceptions. Evaluators verify mutants are reachable and non-equivalent. Keep natural defects, seeded detections and model broken-control outcomes in separate tables.

### Simplification opportunities: W

Score a proposal only when it identifies a concrete existing mechanism, its purpose, a replacement/removal and the obligations that justify the change. Deduplicate equivalent proposals; a renamed module or fewer lines is not itself simplification.

For each opportunity, retain a compact comparison:

`existing mechanism → proposed change → retained/lost obligations → removed independent state/owners/recovery → new premises/costs → discriminating evidence`.

Evaluate two explicit tracks:

- **Full retained scope:** all required functions, legitimate acceptance, safety, recovery and applicable operating requirements remain. Conditional proposals stay conditional until essential premises are established.
- **Partial scope:** name every dropped/changed obligation and explain the benefit. Report retained coverage and simplification together; do not rank this as a full-parity architectural win.

Compare independently maintained facts, durable lifecycles, competing writers, commit boundaries, protocols and recovery obligations. Include throughput, latency, contention, external contracts, migration/coexistence and operational cost where affected. Do not collapse these into an arbitrary sum: show tradeoffs and which proposals improve complexity without worsening the agreed constraints.

An evaluator challenges each proposal with retained happy paths, forbidden paths, failures, recovery and alternative writers. Require evidence for claimed reuse, atomicity or provider guarantees. Formal arms may compare candidates under matched obligations/bounds and check removal variants, but a smaller TLC state space is not a design score. A nonformal proposal can earn equal credit with equally convincing evidence.

Grade outcomes as **credible supported opportunity**, **conditional opportunity**, or **rejected/lost required behavior**. Report validated independent opportunities and their magnitude, not just idea counts. With no implementation, this establishes design opportunities rather than achieved code, performance or maintenance savings. Implement selected candidates in a later equal-budget study if those stronger claims are needed.

## 8. Resources, repetitions and interpretation

Start with a calibration pilot on one bounded, representative slice per repository. Choose slices before seeing arm outcomes, mixing ordinary rules with temporal/recovery behavior rather than selecting only historical TLC successes. E1 has three arms, E2 two and E3 two: **three repositories × seven arms × two repetitions = 42 component author runs**. E2/E3 share a frozen input ontology within each slice, not their respective preceding outputs. Pilot outputs do not count as main-study replications.

Proposed pilot ceilings are **40,000 generated tokens including reasoning and two hours per component run**; stop at the first ceiling. E1–E3 have no simplification phase. These are calibration settings, not claims that complete-repository investigation fits them. Use identical CPU/RAM/concurrency allocations within comparisons; tool runtime counts toward elapsed time. If reasoning-token telemetry is unavailable, choose an observable common budget before launching any arm. Record input, cached input, output/reasoning tokens, actual billed cost, tool compute, elapsed time and human effort separately. Record shared-ontology preparation and evaluator costs separately too.

Use pilot feasibility and workload measurements to select one main budget per repository and experiment, identical across its matched arms, then freeze it before scored runs. Do not choose budgets to maximize G5's measured lead. Include intermediate artifact snapshots at common resource checkpoints to show quality versus cost; do not give reviewers' feedback to authors. A method that times out or cannot meet its completion requirements is reported as incomplete at that budget. Recovery-only E1 and focused E2/E3 have their own scoped objectives, not full-uplift completion claims.

The proposed main component study is **three repositories × seven arms × three independent repetitions = 63 author runs**, using the agreed backend scope. A separate W comparison with the same repetition count adds **18 full-workflow runs**, each followed by the common simplification task under a separate equal design budget. A provisional design-budget calibration is 15,000 generated tokens/45 minutes on pilot-sized scopes; whole-repository W budgets must be established before those runs. Optional checker-isolation and alternative-`O0` studies are additional experiments with separately declared counts, not hidden extra attempts for one arm.

Three repetitions are a starting estimate of variability, not a power calculation. If pilot variability makes that inadequate, revise the sample plan before the main study; do not add repetitions only for unfavorable arms. A larger scope/compute budget must not be silently substituted for a failing run. The experiments can be scheduled separately; results from one do not authorize or silently launch another.

Fix the agent model, reasoning setting, tool access and common scaffolding within each comparison. Record immutable version identifiers when available. Randomize run order in repository/repetition blocks to reduce service/host timing effects. Use the same author concurrency policy across arms and count all delegated work if future runs permit it; the initial proposal uses one author per run. A different model is a later replication, not an arm-specific advantage.

Report paired differences and run-level distributions per repository, plus an equally weighted repository summary. Thousands of claims or TLC states are not thousands of independent experimental subjects. Three repositories give limited grounds for population inference; uncertainty across agent runs does not measure generalization to new repositories. Report effect sizes and variability, and avoid declaring small differences a universal winner.

Interpretation rules:

- If E1-Sources exceeds E1-Code, attribute that contrast to access to additional evidence. It does not establish an ontology advantage.
- If E1-G5 matches E1-Sources while adding prose or maintenance cost, the recovery process/contract has not demonstrated an advantage on these criteria.
- If E2-TLA produces better supported amendments with no added bug yield, report a contract-refinement benefit rather than a bug-finding benefit.
- If E3-TLC adds validated difficult defects, report their code paths, evidence, detection frequency and cost. Attribute the comparison to the formal investigation package unless the optional checker-isolation experiment supports a narrower conclusion.
- If W-G5 improves end-to-end outcomes, use E1–E3 to investigate the contributions; do not add their effect sizes as though the benefits were independent.
- If simpler proposals require narrower policies or weaker operating guarantees, attribute the reduction to scope changes.
- If all arms miss the same reference obligations, investigate shared source, model or evaluation blind spots instead of treating agreement as correctness.

## 9. Proposed next step and deliverables

For subsequent experiments, prepare sanitized source manifests, backend scopes, source-access partitions, shared E2/E3 starting ontologies where applicable, independent reference rubrics and experiment-specific prompts. Confirm dataset revisions, runtime feasibility and resource ceilings before launching. Section 10 records the separately authorized first E1 pair.

Keep the protocol and subsequent amendments in this file. At execution, retain a small dataset manifest, frozen prompts/configurations, per-run outputs and telemetry, and an evaluator-owned adjudication/results table. Identify experiment/arm, repository/revision, source universe, `O0` hash where applicable, repetition and cost. Report E1 intent/fidelity/minimalism, E2 correct refinements and divergence judgments, E3 validated hard-to-find bugs, and W's full/partial-scope simplifications in separate result tables. Link supporting traces and reproductions instead of multiplying summaries.

The first report should answer three concrete questions: **which important commitments were recovered or missed; which real defects were found with what evidence; and which mechanisms could be removed while retaining exactly which behavior**. For each difference, distinguish observed historical precedent, experimental result and untested explanation. That is the evidence needed before claiming a technical advantage for the integrated workflow.

## 10. First PairSoft E1 pair — 14 October 2026

**Authorization and scope:** run E1-Code and E1-Sources in parallel on `/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform`, both using **GPT-6 Astra / extra high** (`gpt-6-astra`, `xhigh`). One run per baseline; no E1-G5 arm, formal checking or redesign. Backend intent is in scope, with frontend code available as evidence of server-side semantics. This is an exploratory first pair, not the complete multi-repository study or an independently scored superiority result.

**Source pin:** `0e0c3981653bf0935cdf3e4fcdc7101d104de7ff`, the current committed checkout on `feature/TBD-change-order-approval-workflow`. Only local `.DS_Store` changes were present and were excluded. The original repository and branch remain untouched.

| Arm | Prepared input | Runtime identity |
|---|---|---|
| E1-Code | [Worktree](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e1-code); 9,163 files; no `docs/` or other product-document bundles; one sanitized snapshot commit `fbdf8d25ba825b9249e227077d5a131f989c3b03`. | `01a13c32-d798-7341-99e0-ecba514563f2` |
| E1-Sources | [Worktree](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e1-sources); 10,442 files, including 1,224 under `docs/`; 396 sanitized history commits; head `cd2dfc9a1bf12c7b6875a5783155d78682db1336`. | `01a13c32-d798-7772-a67e-b0f79f90f989` |

The worktrees have separate Git stores. Common files have identical SHA-256 hashes. Both exclude ontology, agent instruction/skill packages and hooks, generated `docs/knowledge` intent summaries, agent-process documents and related automation. E1-Code additionally excludes other product documents such as the top-level change-order design bundle and README/changelog files; code/test fixtures and expected test snapshots remain. E1-Sources retains original functional designs, product decisions, architecture and operational records. Sanitized history rewrites affected hashes/messages and has no upstream remote; it does not expose the original ontology through historical objects.

The fixed local source corpus is the only source universe for this pair. Live external retrieval and external PairSoftCore source are unavailable; dependency references remain. Exact removal lists, file hashes, original-to-filtered history mapping and configuration are saved in the [control directory](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control), with the current [run manifest](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control/manifest.json).

**Execution:** Codex CLI 0.159.2, two independent fresh sessions launched in the same second. Both use the exact corresponding E1 prompt above plus common execution scope, output paths and limits; the [E1-Code prompt](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control/e1-code-prompt.txt) and [E1-Sources prompt](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control/e1-sources-prompt.txt) are frozen with hashes. Ambient user/project instruction files, global skills, plugins, memories, hooks, previous command-approval rules, web search and delegation are disabled. Initial session records confirm `gpt-6-astra`/`xhigh` with no injected skills catalog or memory. A sandbox preflight verified access to the assigned worktree and denial of the original checkout, sibling worktree and global skills.

**Budget and outputs:** two hours or 40,000 generated output tokens per run, including reasoning as reported in Codex telemetry, with a watchdog checking cumulative usage between updates. Each author writes `experiment_output/intent.md` and a short `experiment_output/coverage.md`; incidental findings are optional. The control directory retains event logs, timing, usage and stop/completion status. This pair uses the whole agreed backend scope rather than a bounded calibration slice; budget exhaustion must be reported as incomplete, not full recovery.

**Completed outputs:** both runs exited successfully without reaching either budget limit. The table counts whitespace-delimited words in the authored Markdown; those counts include its markup and links and are not semantic-density or coverage scores.

| Arm | Intent account | Coverage note | Combined words | Author duration | Uncached input tokens | Cached input tokens | Generated tokens |
|---|---|---|---:|---:|---:|---:|---:|
| E1-Code | [1,621 words](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-code/experiment_output/intent.md) | [503 words](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-code/experiment_output/coverage.md) | 2,124 | 9m 2s | 233,003 | 4,870,528 | 22,776 |
| E1-Sources | [1,381 words](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-sources/experiment_output/intent.md) | [480 words](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs/e1-sources/experiment_output/coverage.md) | 1,861 | 12m 32s | 307,839 | 5,404,032 | 27,956 |

Durations come from each session's `task_complete.duration_ms` (541.659s and 751.842s), not the common watcher teardown time. Cached input is a subset of total input; generated totals include the reported reasoning subsets (5,129 and 5,700 respectively), which must not be added again. Billed cost is unavailable. Raw event logs and the [artifact audit](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control/artifact-audit.json) preserve the measurements.

Both authors report **selective static inspection**, with no build, test or live integration execution. E1-Sources reports reading selected original requirements, ADRs, design/status records and historical diffs; it also lists many unread documents and unavailable external records. E1-Code reports selective inspection of production paths and test expectations with no documentary/history input. Final file hashes confirm all supplied input files remained unchanged, and local links in the accounts and coverage notes resolve. Author text was preserved; this validation did not correct or independently confirm its substantive claims. The source run encountered sandbox warnings about Git's system cache/global configuration, but its local history commands succeeded; those warnings remain in the logs.

**Evaluation boundary:** no independent adjudicated reference was frozen before this pair. The source-backed account is shorter in this run and took more author time/generated tokens; whether it preserves more important intent remains unscored. Fidelity, omissions, semantic minimalism and reconstructed behavior require the planned independent assessment. Any later scoring must disclose the retrospective reference preparation, and one run per method cannot establish reliable superiority. These two baselines compare source access; they do not yet compare either baseline with E1-G5. Status: **completed**, with saved outputs and evidence.

### Historical pre-TLA comparison — 14 October 2026

The [three-artifact comparison](pairsoft_e1_intent_comparison.md) retrieves the initial ontology at `0e0c3981`, the same original code pin as this E1 pair. Its ontology tree is identical to the parent of the October 3 strengthening commit `c4abdb3b`; the pre-TLA version has 688 blocks across 43 pages. A verbatim exported snapshot, file hashes and size measurements are retained beside the experiment outputs.

The report compares concrete retained commitments, omissions, stale statements, source authority and reading burden. Both fresh runs recover current change-order approval and over-receive behavior missing or stale in the old ontology; the old ontology preserves some consequential requirements absent from both short accounts. Neither fresh account explicitly retains the Received-PO reopening exception. These are retrospective, selected static probes, not independently adjudicated whole-product coverage scores.

The historical author inherited old ontology/generated knowledge and had a broader preservation task, unlike the controlled E1 packets. Treat it as a historical artifact comparison, not an E1-G5 run or a gold reference. Retrieval succeeded; no replacement uplift, fresh E1-G5 treatment or formal campaign was launched.

## 11. G5 current versus amended harness — 14 October 2026

**Authorization:** run the current G5 harness and the proposed amendment in parallel under E1, with code and docs in both; emphasize the docs folder as important SOT. Reuse the two existing experiment worktrees and discard their uncommitted work. GPT-6 Astra / extra-high and the prior two-hour/40,000-generated-token ceiling continue to apply. This is one new run per variant.

Before resetting either worktree, the previous plain-baseline outputs were copied byte-for-byte to the [baseline archive](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/baseline-outputs). Earlier report links now point there. Their source links resolve through the unchanged pinned input files; the first-run control logs, hashes and Git objects remain preserved. The original PairSoft checkout was not reset.

| Arm | Reused worktree | Harness |
|---|---|---|
| E1-G5-Current | `e1-code` | Frozen `.agents/skills` before this amendment |
| E1-G5-Amended | `e1-sources` | The same package plus the three changes below |

The directory names are historical: **both now contain code and docs** at sanitized `cd2dfc9a1bf12c7b6875a5783155d78682db1336`, corresponding to original `0e0c3981`. All 10,442 supplied files, including 1,224 docs files, have identical hashes; both expose the same 396 reachable sanitized commits. Old ontology, generated knowledge and earlier experiment answers remain excluded. Each gets its own frozen `_harness/skills` package. Ambient project/global instructions, memories, plugins and delegation remain disabled. Sandbox preflight confirmed read-only inputs/harness, writable output directories, and denied access to the original checkout, sibling, evaluator records and global skills.

**Common source policy:** docs are important SOT for intended behavior. Explicit human decisions and accepted requirements govern within their scope; distinguish proposals, supersession and implementation reports. Code is evidence of observed behavior and does not automatically override documentary intent. The source universe and backend scope are confirmed; no further source-confirmation pause is required. Original source material remains evidence, not executable workflow instructions.

**Common E1 adaptation:** use uplift and authoring with relevant references; retain source reconciliation, code tracing, independent capability discovery, reconstruction challenge, uncertainty and compression. Skip formal modeling/checker obligations only. No TLA/TLC, product builds/tests, application changes, downlift, dedicated bug hunt or redesign. Canonical ontology goes in `.g5/ontology`; `experiment_output` holds investigation/coverage records and a short `intent.md` entry point. The primary contract measure includes all ontology pages, not just that entry point.

**Amendment:** three maintained source files changed, with no PairSoft-specific answer hints:

1. Uplift source guide: preserve inherited narratives/history in supporting evidence; retain surviving obligations in the live contract; a migration or stable ID does not reverify an implementation claim.
2. Uplift final pass: test what important meaning deletion would lose, then challenge what important behavior the compressed contract could still omit; do not optimize for a word limit.
3. Authoring: reconcile live summaries/drift lists when an owning rule or assessment changes; replace duplicate assessments with owner links and keep historical snapshots as evidence.

The maintained `.agents/skills` were updated and client/harness copies regenerated. Both changed skills passed the skill validator and the copy-consistency check. The exact frozen packages, [amendment patch](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/control/harness.patch), byte-identical author prompts, commands, preflight results and telemetry are in the [run control directory](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/control); see the [manifest](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/control/manifest.json).

**Interpretation:** the only assigned treatment difference in this pair is the amendment under a shared docs-authority prompt. That clarification differs from the earlier plain baseline, so comparisons with the earlier E1-Sources run combine prompting and workflow differences. One run per variant cannot establish reliable superiority. Both start without inherited ontology/knowledge, so the migration-specific instruction is only weakly exercised. The known PairSoft probes and historical report are evaluator-side only; authors do not receive them. No independent adjudicated reference has been established.

### Execution and saved results

Both processes started at **21:34:29 UTC on 14 October 2026**. Both have stopped; neither claims complete backend recovery. The current author returned normally with a partial contract. The amended author saved its contract and coverage report, then the watchdog terminated it at the generated-token threshold before a final response and refreshed check report. It was not resumed or given additional budget.

| Measure | E1-G5-Current | E1-G5-Amended |
|---|---|---|
| Process outcome | Normal completion, exit 0 | Budget stop, exit −15 |
| Recovery milestone | Partial backend contract | Partial backend contract; final reporting interrupted |
| Elapsed time | 19m 47s | 19m 34s |
| Generated tokens, including reasoning | 38,486 | 42,955 |
| Reasoning tokens, already included above | 7,246 | 7,813 |
| Ontology pages / I–D–Q blocks | 21 / 85 | 20 / 89 |
| Ontology body words | 5,374 | 5,697 |
| Ontology words including metadata | 7,410 | 7,502 |
| Supporting Markdown words, excluding final chat transcript | 5,253 | 4,056 |
| Saved outputs | [Contract](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/.g5/ontology/root.g5.md), [coverage](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/current/experiment_output/coverage.md) | [Contract](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/.g5/ontology/root.g5.md), [coverage](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs/amended/experiment_output/coverage.md) |

The watchdog observes token-usage updates rather than imposing an exact decoder limit. The amended run overshot the common 40,000-token threshold by **2,955** before the stop was observed; report actual consumption, not a fictitious exact match. Elapsed time uses the current run's completed-turn duration and the amended process's observed exit. Cached input counts and complete telemetry remain in the manifest; billed monetary cost was not measured.

Word counts use whitespace tokens across **all** ontology Markdown pages. Body counts include headings/navigation and exclude per-file metadata footers; they are not identical to the historical comparison's marked-block-only measure. Supporting evidence has a separate count and is not silently excluded from reading effort. Neither length nor block count measures semantic coverage.

The evaluator's [artifact audit](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/control/artifact-audit.json) verifies unchanged input/harness hashes, unique IDs, body/metadata pairing, Q uncertainty labels, local link targets and ontology heading anchors. Both saved contracts pass these structural checks. The current final response is archived verbatim with links relative to its original workspace. The amended author's earlier `artifact-checks.md` still says FAIL and 84 IDs because it preceded the final coverage file and additions; the final artifact has 89 IDs. Preserve that stale author record and the separate evaluator result rather than rewriting experiment evidence. These checks do not establish semantic consistency or implementation conformance.

**Observed result:** the amendment did not produce a shorter contract or uniformly better retention on the selected probes. It retains vendor receipt-number uniqueness omitted by the current run; the current run more explicitly retains the dropdown-membership replication-lag exception and the original restriction on recall after review begins. Both recover the Received-PO reopening exception and distinguish several documentary intentions from implementation gaps. Both leave substantial original requirements, entity-specific rules and enforcement paths untraced. See [comparison section 7](pairsoft_e1_intent_comparison.md#7-fresh-current-versus-amended-g5-recovery).

This pair leaves the amendment's effectiveness **unproven**. The user subsequently declined adopting it: the three maintained skill files were restored from the frozen current-harness package and client/harness copies regenerated. Frozen experimental packages and outputs remain unchanged. The follow-up [comparison with the two plain baselines](pairsoft_e1_intent_comparison.md#8-current-g5-versus-the-two-plain-baselines) grades the same 13 previously recorded probes and reports actual effort/length. It supplies no TLA/TLC, validated bug-yield, downlift/reuse or simplification result. No additional experiment was launched.

## 12. First PairSoft E2 pair — 15 October 2026

**Authorization:** reuse the same two worktrees, start from the newly generated current G5 ontology, and run the full uplift skill's TLA/TLC reconciliation against E2's direct-review baseline. The rejected harness amendment stays reverted. This is one whole-backend pilot per method, using GPT-6 Astra / extra-high and the same two-hour/40,000-generated-token ceiling. E2 evaluates supported changes to the shared contract; code defects are secondary. It does not launch E3 or downlift.

The E1 G5 outputs, frozen harness packages and investigation files were archived byte-for-byte before clearing the two worktrees. Earlier comparison links now use the [E1 G5 archive](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/g5-harness-pair-20261014/outputs); unchanged input symlinks preserve their source links. Both worktrees were reset to sanitized `cd2dfc9a1bf12c7b6875a5783155d78682db1336` (original `0e0c3981`). The original PairSoft repository was not reset.

| Arm | Reused worktree | Assigned method |
|---|---|---|
| E2-Review | `e1-code` | Ordinary ontology/code/source reconciliation and optional real-code tests; no uplift workflow or formal/state-model exploration |
| E2-TLA | `e1-sources` | Unchanged current uplift and authoring skills, code-bound TLA models, executed TLC, reconciliation and reconstruction coverage |

**Shared O0:** the exact 21-page, 85-block current-harness E1 ontology, including all metadata and Q blocks. The file-map SHA-256 is `353c4a6cb118280e33ff3f0107ee643647291266d2fc066e32ce32e1f27d239f`; [file hashes](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/O0-files.json) and an [immutable evaluator copy](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/O0/ontology/root.g5.md) are preserved. Each author gets the same editable `.g5/ontology` plus immutable `_input/ontology`.

To preserve the user's chosen starting ontology exactly, its existing discrepancy annotations are **retained as disclosed inputs**, following section 5's disclosed-issue alternative. They cannot earn new-discovery credit. All starting claims and metadata are captured in the evaluator's [disclosure inventory](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/O0-disclosed-claims.json). E1 investigation reports, evaluator comparisons and other arms' outputs are withheld. Neutral coverage/Q-index scaffolds preserve the ontology's existing links without supplying the old investigation. Authors may confirm, refute or sharpen disclosed findings, but must distinguish that from discovery.

**Matched evidence and task:** both have the same 10,442 supplied source files, including 1,224 docs files, and 396 sanitized history commits. The common prompt gives docs the same important SOT status, makes both ontology and implementation open to evidence-based correction, preserves legitimate exceptions and authorizes provisional judgments without routine human pauses. Backend semantics, including those exposed in frontend code, remain in scope; UI/UX and live external retrieval remain out of scope. No evaluator feedback is supplied during execution.

**Runtime and execution:** local .NET SDK 8.0.403 was prepared for both arms; cached NuGet packages are read-only and network restoration is disabled. Java and TLC 2.19 are available; the formal arm has a wrapper that records pre-execution ontology/model/config snapshots, command, raw log, elapsed time and exit status. Runtime tests are permitted in both arms using real code and explicit environmental limits. Production inputs stay read-only; ontology, models and investigation/tests/build outputs stay under the designated writable directories. Each arm is instructed to use one compute job at a time, two workers and about 2 GiB heap; TLC enforces two workers, 2 GiB heap and a five-minute per-call timeout. These limits do not establish identical actual compute consumption.

The [control directory](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control) retains the [manifest](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/manifest.json), common and method-specific prompts, command settings, source/tool fingerprints, sandbox checks and telemetry. The watchdog records actual token consumption and may overshoot before a usage update. Artifact checkpoints at observed 10k/20k/30k/40k usage thresholds record their actual capture usage/time. Setup cost and runtime preparation are outside author budgets; final model/test runtime is inside them.

**Evaluation:** compare each revision with O0 using `starting claim → evidence → judgment → amendment/code issue → evaluator verdict`. Distinguish supported precision, corrected extraction, preserved valid intent, legitimate exceptions, unresolved choices and unsupported restrictions. Attribute findings to source/code inspection, model construction, TLC feedback and runtime reproduction where supported by timestamps and pre-check snapshots. A TLC trace is not a reproduced production defect; a broken control is not a newly discovered bug. Already disclosed findings and old test reports do not earn new-discovery/runtime credit. No whole-system accuracy percentage or isolated causal contribution from TLC will be inferred from one pair.

**Status:** both authors launched in parallel at 2026-10-15T14:13:11.960913+00:00 and have stopped. Review completed normally; formal stopped at the token ceiling. Sandbox, .NET build and TLC runtime preflights passed before author execution. Local sockets were permitted in both arms; direct external connections and proxied HTTPS were confirmed blocked. Actual outcomes and the evidence-limited evaluation follow.

### Execution outcome and artifact audit

**Both authors have stopped. E2-Review delivered a finished report with partial backend coverage; E2-TLA exhausted the token ceiling before completing its reporting and coverage pass.** The saved formal work includes useful contract amendments, executed models and two component-level defect reproductions, but it is not a completed full uplift. Neither author was resumed or given extra budget.

| Measure | E2-Review | E2-TLA |
|---|---:|---:|
| Process outcome | Normal completion, exit 0 | Token-budget stop, exit −15 |
| Actual author duration | 25m 15s | 32m 15s |
| Generated tokens, including reasoning | 36,813 | 43,359 |
| Reasoning subset, already included | 8,781 | 12,626 |
| Uncached / cached input tokens | 209,839 / 5,434,112 | 247,342 / 8,977,408 |
| Ontology pages / stable blocks | 21 / 85 | 21 / 85 |
| Changed block bodies or kinds / metadata-only changes | 15 / 3 | 10 / 2 |
| Ontology body words, from O0's 5,374 | 5,603 (+4.3%) | 5,545 (+3.2%) |
| Ontology words including metadata, from O0's 7,410 | 8,017 (+8.2%) | 7,960 (+7.4%) |
| Supporting Markdown words | 4,964 | 5,904 |
| Complete final review / actual coverage report | Yes / yes, explicitly partial | No / no, coverage remains the initial scaffold |
| New defects reproduced in production components | 0 | 2 |
| Additional supported static defects, without reproduction | 2 | 0 |
| Shared conditional stale-approval race | 1 | 1 |

The formal run consumed **17.8% more generated tokens and 27.7% more time** than direct review. It overshot the common 40,000-token threshold by 3,359 before the next usage report reached the watchdog. These are matched ceilings, not equal actual resource consumption. Review timing uses its completed-turn duration; formal timing uses observed process exit. Billed cost and evaluator labor were not measured. Supporting prose includes the formal author's duplicated `divergence-entries.md` draft; raw logs, model/configuration source, immutable input copies and final chat transcripts are excluded from that prose count. Length and edit counts are not quality scores.

Saved author outputs:

- **Review:** [ontology](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/review/.g5/ontology/root.g5.md), [divergence ledger](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/review/experiment_output/divergences.md), [coverage](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/review/experiment_output/coverage.md), [final review](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/review/experiment_output/final-review.md).
- **TLA:** [ontology](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tla/.g5/ontology/root.g5.md), [divergence ledger](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tla/experiment_output/divergences.md), [pre-check claims](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tla/experiment_output/precheck-claims.md), [models](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tla/.g5/TLA_verification), [component execution log](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tla/experiment_output/runtime/receiving/domain-run-2.log).

The evaluator's [artifact audit](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/artifact-audit.json) confirms all 10,442 supplied source files and immutable input/tool/harness files remained unchanged in both arms. Both preserve all 85 IDs and valid body/metadata pairing. Review's contract links resolve. Formal has **six broken link occurrences to two unwritten reports**, `formal-results.md` and `final-review.md`; its coverage scaffold was not replaced. These completion failures remain in the raw output. The evaluator has not repaired them on the author's behalf. Exact ontology diffs, telemetry and [supporting-prose counts](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/supporting-prose-audit.json) are retained in control.

### E2 primary result: what became more accurate or precise?

This is a **retrospective single-evaluator assessment**, checking selected amendments against original decisions and code. It is not blind, has no independent human adjudication panel or frozen comprehensive reference, and supplies no whole-system accuracy percentage. The [18-case adjudication record](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/adjudication.json) preserves O0, amendment, verdict and attribution; cases are semantic comparisons, not independent experimental replications.

| Contract question | Direct review | Full uplift/TLA | Evaluator judgment |
|---|---|---|---|
| Can an old approval authorize a newly edited/resubmitted request? | Adds new-context requirement and identifies missing correspondence. | Adds explicit current-submission requirement and executes a stale-result trace. | Both improve O0. The runtime race remains conditional on broker/dispatcher correspondence. |
| What differs between catalog editing, punchout return and checkout cleanup? | Qualifies punchout immutability, session binding, selective replacement and asynchronous cleanup. | Recovers the same distinctions; expressly preserves unrelated additions and checks cleanup. | Both correct consequential ambiguity. External session-store and changed-item concurrency guarantees remain unknown. |
| Can requester authority permit autoapproval without a manager? | Restores the exception, explicitly provisional. | Restores it and checks the initial branch; metadata records uncertain historical acceptance. | Both preserve implemented behavior missing from O0; inconsistent original wording prevents claiming recovered human acceptance. |
| Does the no-op company-delete validator establish a live bypass? | Finds no local production caller; narrows the claim. | Same. | Both improve evidence precision without inventing an exploitable defect. |
| Is login batch partiality a settled policy for every failure? | Changes `ps.renamebatch` from Q to resolved D using earlier per-row/chunk decisions. | Retains Q after reconciling later D33/D37. | **Formal arm is better on this case. Review over-resolves the contract.** |
| Must receipt validation aggregate repeated input rows for one PO line? | Leaves O0's general cumulative rule. | Makes the aggregation explicit and reproduces the missing sum. | Supported formal-arm precision; does not impose an unnecessary ban on all duplicate rows. |
| Are fractional receipt quantities permitted? | Leaves O0's omission; identifies input precision as unverified. | Recovers accepted R11's whole-number requirement and reproduces its missing enforcement. | Supported formal-arm recovery of an omitted rule. |
| How do decision redelivery and new policy insertion work? | Retains existing general rules. | Adds exact-versus-conflicting decision identity, and Requisition-first versus Change-Order-last insertion. | Supported formal-arm precision/recovery; no new production defects in these paths. |
| Which UDF lag behaviors were actually accepted, and whose defaults apply? | Separates accepted absent-usage tolerance from provisional empty-membership skipping; preserves source-requester defaults and exposes buyer substitution. | Leaves these O0 passages unchanged. | Review improves source authority and identity precision; merged-requester header choice remains provisional. |
| Does a preexisting draft receipt remain submittable after terminal PO status? | Requires submit-time recheck and traces its omission. | Leaves this lifecycle concern unreported. | Supported review-arm precision and static defect. |
| Are exporter filters absent, and what do architecture/authentication exceptions allow? | Traces exporter filters; narrows usage-table ownership and permitted reads; separates evaluation data from credentials. | Leaves these O0 questions/wording unchanged. | Review provides additional supported corrections outside the formal campaign. |
| Was the deferred OOO date change accepted? | Records current From-date filtering without treating code as acceptance. | Leaves O0 unchanged. | Review preserves a more accurate unresolved source/code conflict. |

The login-batch difference is particularly instructive. The [identity design](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e1-code/docs/designs/123482-user-identity-continuity-across-email-change.md:56) contains both D33's explicit intermediate repair commit and D37's later acceptance of whole-batch rollback on a final database concurrency conflict. Earlier D11/D15/D3 do not settle that conflict. Review's new rule that one failed pair must never erase completed pairs is too broad, and its Q→D conversion falsely closes an uncertainty. Formal correctly distinguishes row refusal from mixed-batch persistence. **That advantage came from source reconciliation, not TLC.**

The sampled unchanged controls preserve requisition nominal-rate fallback, scoped administrative quorum bypass, historical UDF values and release-gate distinctions. The already disclosed recall and idle-sync gaps earn no discovery credit. No completed fresh-reader reconstruction test or comprehensive before/after fidelity score was produced. On the observed evidence, direct review delivered broader finished reconciliation, while formal added precise temporal/aggregate contracts and avoided one material false resolution. Neither dominates every criterion; the contracts' size differs by less than 1%.

### Secondary code findings and attribution

| Finding, deduplicated by cause | Found by | Strongest observed evidence | Limits |
|---|---|---|---|
| Same-PO-line receipt rows each pass although their sum exceeds the cap | TLA | Unchanged `ReceivingStorage.ValidateReceivedQuantities` accepts 2+2 against ordered 2; correctly refuses a single 3. Prior-receipt variant also reproduced. | EF InMemory, test context/PO views/core markers; no SQL isolation or HTTP submit qualification. |
| Fractional receipt survives submission despite accepted integer-only intent | TLA | Unchanged `Receiving.Submit` processes 0.5, prunes an incomplete zero row, and rejects a second submit. | Real aggregate/state/event code with external marker/identity/audit substitutes; no full authenticated handler/DB pipeline. |
| Manual PO conversion uses buyer's unmapped header UDF default instead of source requester's | Review | D59 plus manual/automatic factory data flow; distinct requester A and converter B expose the mismatch. | Supported static defect; no execution. Multiple-requester header precedence is a separate unresolved choice. |
| Draft receipt submit omits Closed/Cancelled/Invoiced PO recheck | Review | Create checks terminal exclusions; submit and downstream status update check pending Change Order but not terminal status. | Supported static defect; no SQL/HTTP reproduction or concurrent-close test. |
| Old approval result can encounter a renewed Awaiting request | Both | Code drops approval-version correspondence; formal gives Decide(g0)→Edit(g1)→Deliver(g0). | Shared conditional race candidate; actual inbox scheduling and submission correlation still require validation. |

Thus this pair found **two different supported defects per method**, with the formal arm taking its two to **component runtime reproduction** and review leaving its two static. The confirmed defect sets do not overlap; the conditional race does. Neither reproduced a full HTTP/SQL/E2E failure. Review also ran 16 passing examples against unchanged policy/constants; those are useful positive checks, not 16 defects or a reproduction of its submit finding.

Both formal receipt hypotheses were recorded **before the first TLC execution**, at about 14:17 and 14:18 UTC. TLC then supplied bounded counterexamples, checks of candidate rules and sensitivity controls; runtime execution supplied the component evidence. The trace ledger therefore does **not** establish a bug first discovered by TLC in this pair. It demonstrates a stronger executable evidence trail, not that an agent without TLC cannot find these bugs. No seeded/broken control or model-construction error is counted as a product defect.

### What TLC actually established

The evaluator's [receipt audit](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop/tlc-audit.json) records **7 models, 44 distinct configurations and 52 model invocations**, plus one help invocation. All wrapper calls together took about **46.7 seconds**; this excludes model construction, source investigation and report writing. Model invocations include repeated checks and failed model construction, not 52 independent verified requirements.

| Concern | Final passing configuration: distinct states | What the other configurations add |
|---|---:|---|
| Receipt cumulative bound | 1,214 | As-built duplicate-row violation, broken-serialization sensitivity, legitimate completion/refusal witnesses |
| Receipt input/pruning | 1,152 | Fractional-input violation, incomplete-row sensitivity, completion/pruning witnesses |
| Requisition lifecycle | 19 | Disclosed recall violation and conditional stale-generation trace; valid recall/completion retained |
| Change Order application | 24,579 | Nine identity-mismatch controls, lock/duplicate controls, approval/rejection/quarantine/toggle-off witnesses |
| Cart cleanup | 22 | Unrelated-addition preservation, flush-all sensitivity and cleanup witnesses |
| Managerial initial decision | 60 | Requester-authority/no-manager branch sensitivity and self/route witnesses |
| Policy insertion order | 9 | Separate prepend/append sensitivity and completion witness |

These are finite abstract-model results with bounded actors/rows/events, fair-progress assumptions and explicit external persistence premises. They do not verify entire features or production transaction behavior. Reachable completion witnesses are distinguished from the temporal properties checked in normal configurations. Simple managerial and insertion-order models add mechanical repeatability, but do not establish an advantage over direct tests for those local rules.

Two model mistakes caused three failed executions: an unparenthesized monitor assignment in Receipt and an incompletely framed fairness action in Managerial. Corrected configurations were executed; the initial Receipt pass predates its correction and is not the final assurance result. Most exit-12 results are deliberate control/witness violations, not newly found bugs.

One final configuration was changed **without a recorded re-execution**: `req_built_generation.cfg` switched `GuardRecall` from true to false. The earlier valid stale-generation trace remains evidence for its frozen configuration, and that trace uses authorized editing rather than recall, but the author's statement that the final as-built configuration was checked is unsupported. Preserve this as a verification gap, along with the missing reports and coverage pass, rather than silently finishing the campaign outside budget.

**Interpretation:** this E2 pilot shows useful formal modeling and reproduction work, but does not establish that the full uplift workflow produces a better overall refined ontology at this budget. Direct review finished more of the reconciliation/reporting task; formal supplied deeper executable evidence on selected concerns, missed several review-arm corrections, and stopped incomplete despite higher actual consumption. A claim of reliable superiority, or an isolated causal effect of TLC, needs the planned repeated and independently adjudicated comparisons. No harness changes, production repairs, E3 run or downlift were performed.

### User-authorized continuation without an overall budget — 15 October 2026

After reviewing the first result, the user explicitly removed the budget stop and requested the same TLA/TLC session continue until the full assigned work is finished, followed by an updated comparison in this directory. Session `01a13fe8-ed4d-70c0-8177-d5d3a5bdf51d` resumes in the same `e1-sources` worktree with GPT-6 Astra / extra-high. There is **no overall token or elapsed-time cutoff**. The original source isolation, unchanged harness, read-only production inputs and per-job resource controls remain in force. The baseline is not rerun.

The original outputs and evaluator records were frozen before resumption in the [budget-stop archive](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/budget-stop), and historical result links now point there. The [continuation control directory](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/continuation-1) records the continuation prompt, exact command, telemetry and final outcome. The new prompt supplies the user's budget removal and repeats the original completion criteria; it supplies no evaluator findings or baseline answers.

**Final status: stopped at the user's request, 15 October 2026, 15:08 EDT, because the experiment was taking too long.** The author process and its process group are stopped; no follow-up author work or further experiment is running. This is an incomplete uplift, not a budget stop or normal workflow completion. Cumulative formal usage was **335,590 generated tokens and 4h 8m 46s active author time**, versus the baseline's 36,813 tokens and 25m 15s: **9.12× tokens and 9.85× time**. The final contract has 21 pages, 111 blocks and 7,545 body words; the baseline has 21 pages, 85 blocks and 5,603 body words. No source or immutable inputs changed.

The [stopped-run comparison and incremental harness adjustments](pairsoft_e2_refinement_comparison.md) retain the original matched-ceiling result, distinguish the unequal-effort continuation, and document eight evaluator-reviewed component/helper defect reproductions, selected static findings, the baseline finding still missed, structural/reporting gaps and limits on attributing gains to TLC. The [user-stop archive](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/outputs/user-stop) preserves the full author outputs, stop record and evaluator audits; all 2,907 fingerprinted artifacts match their archived copies. The earlier interim snapshot remains historical evidence. The user subsequently authorized three incremental harness adjustments while preserving comprehensive TLA/TLC: explicit code-binding evidence before implementation claims, integration at concern boundaries with lightweight structural checks, and earlier reconstruction challenges. These were applied to the maintained guidance and synchronized copies on 15 October 2026; no additional author run evaluated their effectiveness, and frozen experimental inputs remain unchanged.

## 13. Proposed uplift reliability improvements — 15 October 2026

**Status: implementation authorized on 15 October 2026; validation recorded below.**
The three adjustments recorded above are the baseline, not new work credited to this
amendment. The requested outcome is an autonomous, phased, one-time uplift
that produces a high-level reconstruction contract, broad inspected and formally checked
coverage, concrete code gaps and supported repair/simplification plans. More time and
tokens are acceptable; concise governing meaning remains important. There is no proposed
production-workflow token cap, model-count target or whole-system completeness percentage.

The existing skill already has five phases, checkpoints, independent coverage discovery,
bounded worker packets and central integration. Adding those instructions again would
not be an improvement. The changes below make a few existing decisions observable or
provide a missing deterministic tool. Their effect on autonomous quality must be tested.

### Evidence and proposed changes

“Demonstrated” below describes an observed failure or narrow mechanism, not proof that
the proposed amendment improves an agent's overall work.

| Change | Concrete change to execution | Justification and limit |
|---|---|---|
| Preserve workflow isolation | Keep new execution rules in uplift's entrypoint/reference and an uplift-local checker. Retain explicit uplift/continuation triggers and the existing separation from downlift, ordinary fixes, planning and review. Shared model guidance may describe checks, but must not activate uplift phases. | User requirement; current narrow routing probes are favorable. No new routing defect was found. Validate rather than add global routing prose. |
| Make checkpoint continuation concrete | At phase completion or handoff, record the exit criteria actually met and evidence links, unresolved criteria, and next ready tasks in the existing plan. Resume from that state and inspect changes affecting evidence; reopen affected work only. | Stale coverage/reporting and repeated human prompts justify the concern. This specific continuation mechanism is an untested hypothesis. The stopped E2 run does not prove it would falsely declare completion. |
| Supply the missing structural check | Adapt the evaluator's body/metadata, stable-ID, Q uncertainty and local-link checks into a read-only helper. Invoke it at the already-added concern-integration checkpoint and finalization. | Strong, narrow evidence: four ordering mismatches and three missing Q uncertainty labels. The checker cannot establish intent adequacy or implementation correctness. |
| Make gap and repair outputs assessable | For each material gap, require a supported trigger/path, violated commitment, actual effect, evidence class, affected correction boundary, reuse-based repair candidate and remaining verification. Incomplete candidates remain explicitly partial. | Complete-path mistakes and G5's incomplete exit-record repair demonstrate the problem. The more explicit repair deliverable is also a user requirement; its quality benefit is unmeasured. |
| Select runtime evidence by unresolved claim | Reuse relevant existing tests. Where a consequential boundary remains uncertain, use a discriminating local reproduction within confirmed authority; otherwise record the exact missing case and conditional claim. Set any diagnostic-test authority once in the existing scope confirmation, not separately for each concern. | The lease pilot's new test duplicated an existing detector; PairSoft component executions supplied stronger evidence on eight cases. Neither supports universal new tests or treating a helper execution as E2E proof. |
| Use parallelism and an independent contract challenge deliberately | Explicitly permit useful source/domain workers within uplift, subject to session/tool permissions. Keep bounded packets, separate drafts and coordinator integration. For the final reconstruction challenge, use a fresh reader when available, with independently gathered expectations kept separate until its ontology-only reading is complete. | User-requested capability; omissions justify challenging the contract. No controlled parallel-versus-serial or fresh-reader quality result exists. Do not mandate a fixed agent count or reviewer at every checkpoint. |
| Synthesize supported simplification opportunities | In the existing gap/plan artifact, group shared causes and compare credible reuse/removal candidates against retained obligations, new assumptions and recovery/migration consequences. Provide concrete bounded plans, without changing governing intent or implementing a replacement. | User-requested output and limited historical support for individual mechanism reductions. E1/E2 did not test simplification, and no evidence establishes global architectural minimality. |

Evidence: [E2 results and the three applied changes](pairsoft_e2_refinement_comparison.md),
[E1 amendment comparison](pairsoft_e1_intent_comparison.md#7-fresh-current-versus-amended-g5-recovery),
[earlier harness decisions](../ontology_design_principle_enforcement/pairsoft-harness-experiment-takeaways.md#narrow-harness-results),
[PairSoft lifecycle and simplification evidence](pairsoft_lifecycle_study.md),
[G5 quantitative coverage](/Users/ziniuw/Desktop/projects/G5-Lean/.g5/uplift/ben-ontology/coverage.md),
and [G5's recorded lease pilot](/Users/ziniuw/Desktop/projects/G5-Lean/.g5/uplift/ben-ontology/uplift-plan.md).
The E1 wording amendment did not establish improved outcomes and was reverted. Earlier
progress-journal and check-cache prototypes were also rejected; this proposal adds
neither a runner, a cache, a separate journal nor a new approval process.

### Phase execution and evidence checkpoints

Retain the five phases. Their dependencies operate per domain so workers can proceed
without a whole-repository barrier. The coordinator owns shared rules, cross-domain
obligations and the overall checkpoint. After the existing source/scope confirmation,
checkpoints are evidence decisions and progress updates, not requests for permission.
Continue feasible work while a criterion remains open; unavailable evidence or an
explicit user stop produces an accurate partial milestone. A completed model batch is
not completion of the uplift.

| Stage | Evidence required to close the checkpoint |
|---|---|
| Intake | Recorded sources, authority order, included/excluded domains and source revisions; preserve existing confirmation. Record diagnostic-test authority if relevant and already granted or resolved in that initial confirmation. |
| 1. Discover | The independently discovered domain/capability inventory and source families have candidate commitments or explained open/excluded dispositions. Unread behavior is not classified as an implementation detail. |
| 2. Form domain contracts | Important candidates have semantic owners, authority, conditions and legitimate exceptions. The early reconstruction challenge compares initial contracts with independently discovered operations, including distinct lifecycle operations. Newly exposed omissions become work before deepening that domain's campaign. |
| 3. Check and reconcile | Every accepted result names the actual property, code boundary, current model/configuration evidence and limitations. Required safety/progress checks, meaningful completion and relevant broken controls are distinguished. Integrate ontology, Q metadata, findings and coverage, then pass the structural check. Failure of a model, a code defect and unresolved policy have separate dispositions. |
| 4. Establish breadth | Reconcile source/operation discovery with contract-derived expectations, including cross-domain handoffs. All important in-scope behavioral obligations have meaningful executed TLA/TLC checks; structural items have reasons. Missing required checks, important unread areas or unexplained reconstruction mismatches reopen relevant earlier work. Runtime conformance remains a separate evidence dimension. |
| 5. Finalize | Read the governing contract without silently filling omissions from code. Check preservation, contradictions, duplicate owners and provisional-policy consistency. Every material gap has an adequate correction proposal or explicit remaining design work; simplification opportunities disclose preserved/lost scope. Final coverage and summaries agree with the integrated artifacts and report exact assurance limits. |

Use existing task and coverage rows rather than another ledger. Add a next action or
boundary only when needed to explain an open criterion. On context continuation, recover
confirmed scope, current checkpoint, relevant revisions, unresolved questions, worker
assignments and next ready tasks. Inspect changed premises before reusing evidence;
unchanged completed campaigns need not be repeated. This is resumption of the active
uplift, not automatic activation during later feature work or a summary request.

The detailed source inventory, inspection ranges, model bindings, traces and reproduction
logs remain supporting evidence. Ontology bodies retain outcomes, ownership, conditions,
exceptions and consequential decisions. Many functions/properties may share one commitment;
one model link does not cover all of that commitment's clauses. Preserve the current
full formal scope; do not turn the G5 session's suggestion of selective modeling into a
reduction of this user's uplift requirements.

### Concrete gap and simplification deliverables

Extend the existing `code_gaps.md` guidance, without a new report:

- For a material gap: supported obligation and authority; affected revision and reachable
  trigger; actual versus required effect and impact; relevant producers, alternate writers,
  handlers and consumers; evidence class and untraced boundaries. Deduplicate by cause.
- For its repair: the owner/protocol to change or reuse, all affected writers/consumers,
  legitimate behavior to preserve, failure/retry/recovery effects, material data/migration
  implications and a discriminating verification plan. Group related gaps where one repair
  addresses them. A pending reconciliation flag that cannot recover a lost exit payload,
  for example, is a partial repair and must retain that missing obligation.
- For a simplification: current mechanism and why it exists; candidate removal/reuse;
  independent state, authority or recovery paths removed; obligations retained or lost;
  new premises and operational tradeoffs; feasibility and model evidence plus outstanding
  checks. Compare credible alternatives under the same required outcomes. If a necessary
  reuse guarantee is unsupported, investigate it or keep the candidate conditional.

These are reviewable bounded repair/architecture plans, not generic “add locking” advice,
achieved runtime repairs or a silently adopted replacement design. An unsupported global
redesign stays a separate question. If no simplification survives the evidence, say so;
do not meet a quota by reducing required behavior. Required future validation is work in
the proposal, not evidence already obtained.

### File-level implementation plan

1. **`ontology-uplift/SKILL.md`:** keep the existing trigger and five-phase structure;
   point to checkpoint resumption and concrete gap/repair outputs. Clarify bounded
   repair/simplification proposals versus implementation/replacement design. Preserve
   runtime authority boundaries and comprehensive TLA/TLC requirements.
2. **`ontology-uplift/references/execution-plan.md`:** make the small continuation,
   delegation, evidence-selection and repair-output changes above. Reuse its existing
   artifact and integration sections; avoid restating shared modeling mechanics.
3. **`ontology-uplift/scripts/check_ontology.py` and focused fixtures:** extract/adapt the
   structural logic from the evaluator's `contract(root)` function in
   [audit_outputs.py](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/e2-pair-20261015/control/audit_outputs.py).
   Inspect the authoritative format before implementation; reject malformed/unrecognized
   block syntax instead of silently skipping it. Test missing, extra, duplicate or reordered
   metadata, Q uncertainty, valid format variants and links. Do not reuse its experiment
   manifest side effects. This script is callable from uplift, not a global execution gate.
4. **Packaging:** synchronize canonical skills through `scripts/sync-ontology-skills.py`
   and check maintained copies and skill structure. Make no new costly requirements in
   global AGENTS, ontology-authoring, incremental planning/review or downlift. The three
   already-applied changes remain intact. Inspect each diff against this scope.

### Validation before claiming improved autonomy

First implement/test the narrow mechanisms; keep efficacy claims provisional until
author runs evaluate them. Do not rerun or repair frozen experimental outputs in place.

| Experiment | Setup and observable acceptance |
|---|---|
| Routing regression | Fresh-context ordinary fix without ontology; generic full-workflow request with existing models; approved-design implementation; summary of completed uplift; explicit uplift continuation; explicit downlift. Inspect actual skill reads/tool calls and artifacts. Negative cases must not activate uplift or unrequested TLC. Continuation reuses scope confirmation; downlift remains its own workflow. These are routing probes, not benchmarks. |
| Structural checker | Run on copies of the frozen E2 artifact and valid/malformed fixtures. Detect the recorded four ordering defects and three Q omissions without silently skipping blocks or reporting valid forms as errors. Report only structural conclusions. |
| Interrupted continuation and integration | Prepare two domain tasks and one shared obligation; interrupt at a real checkpoint, change one relevant source boundary and resume in fresh context. Require retention of applicable evidence, reopening of affected claims, central resolution of worker overlaps, progress on unfinished obligations and truthful milestone reporting without coaching. Include an interruption before integration so worker output is not mistaken for canonical completion. |
| Unattended bounded full uplift | Compare the frozen current harness **including the three new adjustments** against this amendment on the same preselected, bounded slice with original sources, production code and hidden reference obligations. Both complete the full scoped phases with the same tools, test authority, concurrency availability and quality standard. Give sufficient resources to complete; record actual effort and any external stop honestly. No evaluator quality prompts or answer leakage. Repeat if the initial pair supports an adoption claim. |

Judge the full run on missed consequential obligations, unsupported restrictions, false
code/conformance claims, justified Q decisions, artifact coherence, complete correction
scope, fresh-reader reconstruction answers and human interventions. Measure time/tokens
and duplicated work, but do not optimize them at the expense of the required ontology.
Separately classify defects by static/component/integration/E2E evidence. Count a new
model-derived test's incremental detection over existing tests, not merely its existence.
An artificial required approval should count as avoidable intervention; an actual
unresolved human policy decision is a different outcome.

A paired full-run test measures the package, not each clause's causal contribution.
If it helps, use targeted ablations to distinguish the continuation protocol, early
coverage challenge and fresh-reader/parallel work. The earlier proposed intent+code /
plus TLA / plus TLC comparison addresses formalization's marginal value separately;
Apalache or trace-validation infrastructure is deferred until a specific unresolved
claim justifies it. Neither is needed to implement or evaluate this reliability proposal.

### Implementation and validation — 15 October 2026

Implemented the accepted amendment in the uplift entrypoint and execution guide:
evidence-backed checkpoint continuation, integrated gap/repair/simplification proposals,
selection of runtime evidence, and useful parallel investigations/fresh-reader challenges.
The original five phases and comprehensive formal completion criteria remain. The new
requirements are confined to uplift; the three earlier adjustments and concurrent
authoring/model-guide changes are preserved. Maintained client/harness copies were
regenerated with `scripts/sync-ontology-skills.py` and match the canonical skill files.

Added the dependency-free, read-only `ontology-uplift/scripts/check_ontology.py` with
22 passing filesystem/CLI tests. It reports mechanical structure and local-link errors,
not semantic correctness. Independent review exposed and verified fixes for two scanner
defects: ordinary unmatched/nested brackets hiding later broken links, and valid
single-letter reference/list links being misclassified as ontology markers. Trailing
marker whitespace also has a regression test. There are no unresolved findings from
that bounded review. Unsupported Markdown extensions are disclosed in the CLI help.

Executed validation:

- `PYTHONDONTWRITEBYTECODE=1 python3 .agents/skills/ontology-uplift/scripts/test_check_ontology.py`:
  22 tests pass, including malformed files, recorded defect types, common valid Markdown,
  reference/anchor checks, unchanged input bytes and CLI exit codes.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p test_sync_ontology_skills.py -v`:
  all three existing copy/synchronization tests pass.
- `python3 scripts/sync-ontology-skills.py --check`, skill-creator's `quick_validate.py`
  (with PyYAML supplied through a temporary `uv run --with pyyaml` environment), edited
  skill-link checks and `git diff --check` pass.
- The checker finds exactly the four metadata-order and three Q uncertainty defects in
  an ontology-only copy of frozen E2 output: 21 files, 111 blocks, all copied input hashes
  unchanged. [Regression evidence](/var/folders/6m/hcldwlsj4k72ml4b0s9d5fqr0000gn/T/g5-uplift-structure-regression-tddjotg4/structure-result.json).
  The copy omits 247 linked source/evidence targets; these are fixture-scope link failures,
  not newly discovered errors in the historical artifact. Links have separate fixture tests.
- A read-only run over this checkout's existing ontology (34 files, 823 blocks) reports
  three metadata-order issues, six missing anchors and seven missing file references.
  These pre-existing findings are recorded in the [corpus result](/var/folders/6m/hcldwlsj4k72ml4b0s9d5fqr0000gn/T/g5-uplift-structure-regression-tddjotg4/current-corpus-result.json);
  the ontology was not changed as part of harness tuning.

All six fresh-context smoke probes completed in disposable synthetic repositories:

| Probe | Observed result |
|---|---|
| Ordinary bounded fix | Source-only change; two existing tests pass; no ontology workflow activated. |
| Generic full workflow with existing TLA files | Planning/authoring used; nine tests pass; no uplift, model-guide read or TLC. |
| Approved-design implementation | Existing design followed; six tests pass; no uplift or TLC. |
| Completed-uplift summary | Read-only summary; no workflow restart. |
| Explicit downlift preparation | Downlift used, one plan created, later stages explicitly pending; no uplift or TLC. |
| Explicit uplift continuation | Changed source premise reopened, unaffected structural result retained, unsupported worker overclaim rejected, and six ontology/evidence/documentation files reconciled. Existing tests reproduce the seeded regression; structural and diff checks pass. Application code, tests and historical evidence remain unchanged. Required formal work and phases 4–5 remain explicitly open. |

[Probe report](/private/tmp/g5-uplift-routing-smoke-vie2vbg9/REPORT.md) and
[per-case commands/results](/private/tmp/g5-uplift-routing-smoke-vie2vbg9/summary.json)
retain the setup, observations and artifact changes. Successful launches used ephemeral
Codex CLI 0.159.2 with ordinary `workspace-write`/`never` and the existing configured
model/effort. Initial launches failed before model execution because the CLI could not
load an app-owned MCP transport. Supported `--ignore-user-config` resolved that
compatibility issue while preserving the model/effort and disabling ambient integrations;
the startup errors remain recorded. The negative probes preceded a final repair-output
detail amendment that did not change routing; their exact harness snapshots are recorded.

These are one synthetic execution per route, not an efficacy benchmark or proof of
isolation in every environment. The continuation starts from a seeded persistent handoff;
it is not a live interruption or live parallel-worker experiment. No unattended full-uplift
comparison or causal efficacy experiment has been run. Structural and routing checks
cannot establish improved whole-contract coverage or autonomous quality; those claims
still require the comparative experiment above.

### Recommended next experiment — bounded G5 lifecycle uplift

**Authorized on 15 October 2026; execution records below.** Start with a paired real-code uplift of the boundary from
VM Stop through terminal recording, host-event delivery/acknowledgment and workspace
deletion/reconnect. This tests two interacting owners and recovery across their boundary
without repeating the whole repository. The historical `RUNTIME-EVENT-DELETE` and
`RUNTIME-EXIT-FACT` findings motivate this selection; they are source/model findings,
not previously executed application reproductions. Their repair notes also distinguish
retaining an unfinished obligation from recovering a lost exit payload. This is a known
regression slice, not unseen evidence of generalization.

Before author runs, pin a historical production revision where those findings can be
independently revalidated, the original human source/decision records and the complete
relevant dependency closure. Include all alternate terminal producers and event consumers;
do not handpick only failing functions. Keep unrelated subsystems out of the assigned
contract while making relevant boundary evidence available. Reject the fixture if the
source-supported obligations or executable setup cannot be established. Do not treat
today's mutable checkout or its current issue list as the historical baseline.

Compare two frozen harness packages:

- **A:** the pre-amendment uplift harness, including the three earlier changes (complete
  code paths, continuous integration of findings/coverage, early reconstruction challenge).
- **B:** A plus the accepted reliability amendment and its checker. Apply unrelated later
  routing/shared-skill changes identically to both packages, and inspect/hash the complete
  package diff. If A cannot be recovered faithfully, report a qualification run rather
  than claim a causal A/B result.

Use two independently initialized matched pairs: **four author runs**, with identical
source snapshots, prompts, model/effort, tools, available concurrency, diagnostic-test
authority and full scoped phase requirements. Author contexts start empty. Confirm the
scope and sources in the initial prompt and authorize useful parallel work and isolated
diagnostics once. Both arms retain all required scoped TLA/TLC work. Let both finish;
record effort rather than impose a small competitive token cutoff. Any operational stop
remains an incomplete run. Balance launch order, retain every attempt, and do not tune
the harness or give quality feedback between repetitions.

Include one actual persistent handoff in each run: after the existing phase-2 checkpoint
is reached, snapshot the artifacts, terminate the coordinator context and continue in a
fresh context with the same neutral instruction to finish the agreed uplift. If a run
skips that checkpoint, record that behavior rather than coach it into producing one.
Recover from saved artifacts, not copied hidden reasoning or evaluator advice. Retain
the same source revision during this test so source drift is not another treatment.
This tests real continuation; it does not establish correct recovery of live unfinished
workers. A separate small live-worker interruption/source-change probe can test that
mechanism if needed, without presenting it as ontology-quality evidence.

Give authors original intent/history, production code and existing tests. Exclude prior
generated ontology, models, issue lists, repair plans, comparison reports and evaluator
records from every readable path, including Git history and search tools. Verify this
boundary: ambient-integration disabling alone is insufficient. Raw human decisions must
remain accessible even when generated material is removed. Evaluator-only reference
obligations and scenarios are frozen before seeing outputs; authors receive the lifecycle
scope, not the known bug list or scenario answers.

| Evaluation | Concrete observation |
|---|---|
| Contract fidelity and breadth | Blindly score the predeclared source-backed obligations, conditions and exceptions; count critical omissions and invented restrictions separately. Also admit independently validated novel obligations and apply the same added rubric to both frozen outputs. |
| Correct findings | Check complete paths for the known regression candidates and legitimate passing cases. Separate static, model, component and integration evidence. A model failure or a speculative missing handler is not an executed product defect. |
| Repair completeness | Check producers, acknowledgment/pruning consumers, failures, retry/restart, retained data and discriminating tests. A pending bit alone cannot earn credit for recovery of a vanished exit payload. A partial proposal must identify its exact remaining design obligation. |
| Simplification quality | Check actual ownership/state/recovery removed or reused and required behavior preserved. Credit a justified conclusion that no simplification is supported; do not reward fewer components by itself. |
| Reconstruction | A fresh reader receives ontology bodies and governing links, without implementation/diagnostic reports, and derives behavior for held-out lifecycle scenarios. Compare against independently established expected outcomes and legitimate unresolved policies. |
| Continuation and integration | Compare before/after checkpoints: applicable work retained, pending work completed, ontology/findings/coverage consistent, no premature completion or unnecessary reapproval. Count corrective human prompts; the scheduled neutral resume is not one. |
| Mechanical evidence | Run the same read-only structural evaluator on both outputs; replay saved formal configurations and available runtime checks. Record commands, revisions, failures and input hashes. A pass has only its stated boundary. |

Structural results and executable scenarios can be deterministic. Semantic coverage,
repair adequacy and simplification need source-grounded blind adjudication; do not turn
the reference set into a claim of exhaustive truth or invent an automatic quality score.
Record tokens, elapsed time and duplicated work as secondary outcomes, not the main target.

Require the amended arm to satisfy the predeclared critical contract cases, preserve valid
behavior, avoid material false defect/conformance claims and report unfinished work honestly
without corrective coaching. Compare actual omissions, repair gaps, artifact inconsistency
and interventions with A, showing each repetition separately. Better formatting or more
models alone is not an improvement. If both arms succeed, the slice demonstrates adequacy
but not uplift from the amendment; if outcomes are mixed, the result is inconclusive.
Four runs remain a pilot, not a statistical generalization claim.

After the pilot, use an independently selected untouched domain to test transfer before
making broad quality claims. A full fresh G5-Lean uplift is then useful as an acceptance
test for repository-scale coverage and orchestration; running only the new harness there
does not identify what the amendment improved. The existing deterministic checker/routing
tests remain regressions, with affected routing cases rerun against the final frozen
packages when their instructions differ from the already-tested snapshots.

### Bounded lifecycle experiment execution — 15 October 2026

The user authorized the four-run experiment and removal of the two prior PairSoft
experiment worktrees. Both `e1-code` and `e1-sources` were removed after verifying
417,478 evidence/input files, preserving one frozen source snapshot and repairing all
122 dependent archive links. Original PairSoft checkout, both bare repositories and
saved experiment outputs remain. Net path-accounted allocation decreased 1.98 GiB;
the immediate filesystem free-space increase was about 354 MiB (APFS sharing and
concurrent activity make these different measurements). [Cleanup report](/Users/ziniuw/Desktop/customers/PairSoft/intent-recovery-e1-20261014/control/cleanup-20261015/cleanup-report.md).

The experiment lives outside the active checkout at
`/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015`.
[Launch manifest](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/launch.json)
pins production `312b09609d76d539de502e5eb68f787fa37c96ef`, 2,288 identical original
input files, the existing configured `gpt-6-astra`/`xhigh`, identical prompts and two
concurrent author slots configured for up to two native subagents per coordinator. Pair 1
dispatches A/B and pair 2 B/A concurrently; exact process start order is recorded,
not guaranteed by the concurrent dispatcher. No small token/time cutoff or quality
coaching is supplied. Authors receive the backend/frontend/build/test source closure,
Ben's original statement document, six original design documents and four recorded
human follow-ups, not previous generated answers or original Git history.

A was recovered from exact recorded pre-edit files; current unrelated routing/format
changes are normalized into both packages. B matches committed `490cb941`. The
[package manifest and diff](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/harness/README.md)
show only the two uplift guidance files and B's checker/tests differ. Frozen B's current
format-compatible checker has 25 passing tests. The [hidden reference](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/reference/reference.md)
was frozen before author execution: 11 critical and 6 other obligations, passing controls,
repair criteria and 12 separately held reconstruction questions. Independent source
review also identified the descendant-deletion obligation; this is not only a two-bug
recognition test. An exact-query PostgreSQL probe confirms the acknowledgment component
behavior, without claiming an integrated Delete/host reproduction.

All four sandbox preflights passed: original source and assigned harness readable but
unwritable; evaluator, sibling, original checkout/history, global skills/config denied;
TLC execution succeeds; external networking is denied. A real native child verified
the same restrictions and loopback availability. One exact-source existing host journal
test passed inside the author profile. A shared immutable 86-test host binary is supplied
with build provenance, not evaluator results. Original application databases are not used.

**Continuation instrumentation clarification:** all four initial prompts request a transport
marker only after the existing phase-2 checkpoint is saved, then end that turn. The runner
snapshots ontology, plans, models and evidence and starts a fresh CLI coordinator using
only the persistent artifacts and a neutral continuation prompt. This is a real staged
persistent handoff, not an abrupt kill, source-drift or live-worker recovery test. Source
and harness hashes are checked before launch, before continuation and after completion.
This narrower observation will not be reported as proof of abrupt interruption recovery.
Because the common prompt requests the phase-2 handoff, it also does not test whether
the amended guidance independently causes an author to create that checkpoint.

An early first-pair attempt was stopped before evaluation because an independent
dependency audit identified missing frontend Stop/Delete callers and confirmation UI,
plus one compile-time Rust include of `.github/workflows/self-hosted-install.yml`.
The original 1,479-file fixture was expanded to 2,288 files at the same revision. Both
initial author attempts and their unique artifacts are retained in
[excluded-attempts/fixture-closure-01](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/excluded-attempts/fixture-closure-01/fixture-correction.json).
All four fixtures were recreated; the same prompts, harnesses and frozen rubric are used.
No author received corrective feedback. Repeated filesystem/TLC/network preflights and
source/harness equality checks pass. This setup correction is reported separately from
author or harness performance, including its extra unscored effort.

The corrected four author runs completed in two matched pairs. The final results
section below records independent evaluation; the earlier first-pair notes remain
historical observations, not the final comparison.

The final frozen package materially tightened routing after the earlier smoke snapshot,
so all five affected negative routes were repeated in fresh fixtures. Ordinary fix
(2 tests), generic full workflow (planning/authoring, 7 tests), approved implementation
(saved plan/testing guidance, 6 tests), completed-uplift summary (read only), and explicit
downlift preparation (one preparation plan, subsequent stages pending) all took their
expected routes. None loaded uplift, deep review or the model-checking guide. The frozen
package and all fixture copies remained byte-identical. These are observed bounded
routing results, not a general guarantee against contamination. [Final-package routing
report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/routing-final/REPORT.md)
retains exact instruction diffs, commands, outputs and separate effort; these five probes
are not included in the four-author quality comparison.

After freezing the rubric, evaluator-only diagnostics also reproduced lost terminal
records on the unchanged production Stop and natural-exit status paths. Actual filesystem
permission failure prevents journal replacement; both paths return physical Stop, remove
the active entry, and retain no durable Exited payload after storage recovery and fresh
journal/runtime construction. Writable controls preserve it. All four targeted tests
match those outcomes. Existing launcher/reap seams supply the physical-exit precondition;
this is a production-component reproduction, not a real VMM kill, OS crash or integrated
API reproduction. [Receipt, patch and limits](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/reference/runtime-fault/receipt.json)
remain private from authors. The original rubric, source fixtures and public test binary
were not changed, and authors received no feedback from these checks.

A subsequent evaluator-only pair exercises the acknowledgment candidate through real
owner HTTP Stop/Delete, host-JWT WebSocket admission, production control handling and
migrated restricted-role PostgreSQL. After deletion removes events 1–2, late event 3,
its duplicate and reconnect replay each receive ack 2, leaving only unacknowledged
event 3. The no-deletion control receives ack 3 in all three cases and preserves the
acknowledged prefix. Both targeted tests match their declared outcomes. Host/guest
responses and the pending host journal are scripted; physical VM termination and the
host filesystem journal are not exercised. Finite repeated/reconnect observations plus
the inspected loop support the progress defect, not an infinite-duration runtime test.
[API/DB receipt and limits](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/reference/event-replay/receipt.json).
Both private reproduction receipts are supplied identically to semantic reviewers as
post-rubric-freeze evaluator evidence, never credited as author work. Authors, production
files and the original frozen rubric remain unchanged.

**Observed launcher limitation:** both first-pair coordinators encountered a native
subagent error when loading inherited context because ephemeral sessions have no saved
rollout. The preflight established the fresh-context child path, not inherited-context
forking. The active comparison keeps its identical frozen settings and receives no
workaround coaching; delegation failures are infrastructure observations, not ontology
defects. This pilot cannot establish reliable inherited-context delegation. A separate
dummy capability probe reproduces the ephemeral failure and confirms that omitting
`--ephemeral` restores both inherited and fresh native children in two saved-session
checks, including exact author flags. Actual child completion messages return unique
nonces; all outside-read and immutable-write denials still hold. Saved transcripts remain
outside the model-readable roots. [Launcher diagnosis](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/harness/native-context-diagnosis/REPORT.md).
This verifies a later setup correction without changing the four current author runs.

**Checker instrumentation finding:** an author warning led to an isolated 14-fixture
probe, which confirms that the frozen checker rejects explicit provenance such as
`- agent; uncertain — source explanation`. The frozen format requires uncertainty
metadata for Q but does not require a tag-only line. Four clear prefix forms fail solely
because `tag_set` accepts only an entire bullet made of tags; two standalone forms pass,
and eight ambiguous prose/link/example controls correctly fail. This is an unsupported
formatting restriction, not missing semantic uncertainty. Raw comparative checker results
remain frozen and require this distinction in adjudication. A minimal later correction
can recognize exact initial tag lists before a clear annotation delimiter while rejecting
incidental prose. No frozen package or author was changed. [Probe, requirements and
receipts](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/checker-format-probe/report.md).

A narrow follow-up parser correction was first verified outside the repository: exact initial
tag lists, with balanced optional backticks, may end at a colon or spaced em/en dash
before provenance prose. All 27 checker tests and the independent 14-fixture probe pass;
the four false positives clear while incidental, negated and ambiguous wording remains
rejected. [Candidate patch and verification receipt](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/checker-prefix-correction/receipt.json).
It is now applied to the maintained checker and generated skill mirrors; all 27 tests,
skill validation and mirror consistency checks pass. Pre-existing generated-copy drift
in authoring/uplift was synchronized from the unchanged canonical instructions. The
frozen author packages and primary evaluation checker remain unchanged. This correction
has deterministic parser evidence, not four-author workflow efficacy evidence.
[Application receipt](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/checker-prefix-correction/applied.json).

The first completed author output exposed the same restriction for the explicit list
`agent; uncertain.` with a terminal period. Its Q body states the unresolved question
and provisional choice; the frozen checker and first correction both falsely rejected
that tag. A second narrow correction accepts terminal punctuation while still rejecting
incidental prose, related words and quoted code examples. All 28 tests, the independent
14-fixture probe, skill validation and mirror checks pass. The original candidate and
raw experimental warning remain preserved; a separately labeled corrected-parser check
of the unchanged 12-page output has zero issues. [Second correction and evidence](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/checker-prefix-correction/applied-v2.json).

A package-manifest check also found an unmanifested Python bytecode cache created in the
controller's frozen-checker directory during earlier evaluation preparation. No harness
source changed and no author package contained that cache. The bytecode and timestamps
were preserved outside the package, only the generated cache was removed, and exact
14/16-file package verification passes. Current evaluator imports disable bytecode
writes. [Instrumentation receipt](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/frozen-cache-incident/receipt.json).

**First matched pair, pending replication:** both authors finished their scoped work
and fresh-context continuation without corrective coaching. Independent replay matches
all 378 baseline and 293 amended current formal configurations, including expected
counterexamples, named violations and passing distinct-state counts. These are
reproduction results, not 671 implementation-conformance passes. Runtime audits preserve
the distinction between exact-source components, extracted functions, mocked peers,
blocked integration tests and unexecuted production microVM behavior.

Both ontology-only readers reconstruct ten held-out scenarios adequately and two only
partially: positive settlement from a complete authenticated inventory after commanded
Stop, and the restart ownership-settlement gate before serving/reclaiming. Both authors
identify the three predeclared code divergences; their repair proposals remain partial.
The amended specimen gives more concrete bounded reuse comparisons and correctly models
Stop before live-child deletion refusal; the baseline has stronger evidence for
post-replacement journal ambiguity and startup settlement. The latter evidence does not
repair its missing governing-body startup contract. The controller applies the same
body-level criterion to both, preserving the raw blind grades and documenting that
normalization in [adjudication notes](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/controller-notes.md).
This mixed first pair does not establish superiority of either harness.

Additional source-validated findings and legitimate passing controls are captured in a
[common supplementary rubric](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/supplementary-comparison-rubric.md),
recorded before the second pair finished. They will be compared across every frozen
output without expanding the original 17-obligation/12-scenario denominators. These
first-pair-derived observations are exploratory, not held-out transfer evidence.

A supplementary ordinary-fix probe addresses a limitation of the five controlled
routing tests: the installed global skill catalog is now enabled. A separate fresh,
tool-free visibility probe reports all 20 installed `g5-` names exactly; the fix then
runs in another fresh context with the original prompt. It changes only `labels.py`,
reproduces the failing existing test and passes both tests afterward, without loading
an ontology workflow or global G5 wrapper. Frozen-B copies and global skill files are
unchanged. This is one additional observation, not a universal contamination guarantee;
plugins and other ambient integrations remain disabled. [Mixed-catalog routing report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/routing-mixed-catalog/REPORT.md).

A separate attempt to verify controller-started private PostgreSQL as a remedy for the
author `initdb` restriction stopped before database startup. An outer fallback-port
guard passed owned-listener controls, but combining it with the unchanged author
sandbox failed with `sandbox_apply: Operation not permitted`. No database was contacted
or started and no API test executed. The proposed setup remedy remains unverified;
active author settings and evidence were unchanged. [Capability probe and limits](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/harness/controller-postgres-capability/REPORT.md).

### Four-run results and evidence preservation

All four author runs completed the scoped phases and their scheduled fresh-context
continuation without corrective quality prompts. All four ontology-only readers
finished with unchanged inputs and exact support quotations verified. Each reconstructed
ten of twelve held-out scenarios adequately and two partially: commanded-Stop versus
unexpected-loss disposition, and startup process-ownership settlement before
reclaim/listening with refusal on damaged/unproven ownership. The second pair expresses
positive inventory-driven release more clearly but still misses the disposition
contrast. This is a governing-body contract gap, not a demand for implementation
mechanics in high-level ontology. A passing model in investigative notes does not
supply the missing reader-visible contract.

Independent replay reproduced all 1,225 current configurations: 315 passes and 910
expected counterexamples, including witnesses and deliberately broken controls.
Outcome kinds, named violations, exit codes and passing distinct-state counts match;
no timeout/resource retry. The [formal replay report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/formal-replay/summary.md)
retains historical exclusions and the common normalization correction. These are
reproduction results, not 1,225 implementation-conformance passes.

All four [runtime audits](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/runtime-audit/REPORT.md)
are complete. Selected component replays corroborate their bounded claims; source
copies and audited outputs are unchanged. Real owners, extracted functions, controlled
peers, Git operations, SQLite arithmetic and blocked PostgreSQL/physical-process tests
remain distinct. The silent unsupported-reflink early return and unavailable native-child
raw tool streams are disclosed uniformly. No independent source corroboration is
credited as execution performed by the author.

The common corrected parser reports zero issues in every output. Frozen raw counts
are 1/0/0/2 for A1/B1/B2/A2; all three are the demonstrated explicit-uncertainty syntax
false positives. They cannot support a semantic or formatting-quality win for B.
A final [integrity audit](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/final-integrity/report.md)
passes all 164 checks against source, harness, prompt, reference and program hashes,
accounting for documented historical fixture/evaluation corrections.

All four blind semantic reviews are complete. Every run identifies the three main
source gaps; all three repair proposals remain incomplete in every run. Both amended
runs give more concrete bounded reuse/tradeoff comparisons; both baseline runs carry
stronger post-replacement journal evidence. Neither advantage establishes consistent
semantic superiority, complete repair or a verified simpler architecture.

The common adjudication corrects two reviewer inconsistencies without changing raw
scores: C08 credits recovered obligation/investigation separately from incomplete
exact-receipt repair in every run, and C10 applies the same missing governing startup-gate
criterion to every run. Model-specific caveats remain explicit. A further source audit
qualifies the second amended run's blanket restore-policy defect claim because original
human intent expressly allows stopped clean central rebase. General arbitrary-head
adoption authority remains unresolved. Supplementary findings are compared uniformly
outside the original 17-obligation/12-scenario denominators.
[Completed results and evidence](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/REPORT.md).

After every evaluation and replay finished, 31 audited local cache/config directories
and one duplicate binary were removed. Selected executable identities and small build
records are preserved. Verification matches all 48,265 retained evidence entries, 9,258
source/harness hashes and 202 preserved files. Observed available disk space increased
13.674 GiB; the earlier PairSoft cleanup separately removed the two requested worktrees.
[Timestamped cache cleanup receipt](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/runtime-audit/cache-removal-20261016T015226Z/REPORT.md).

The next proposed quality experiment is a small 12-fixture semantic reconciliation
comparison, with sufficient compressed contracts, missing qualifiers and overstrong
policies as controls. It tests an operational runner for the already-required fresh
reader: separate source/body permissions, frozen questions, explicit mismatch disposition
and affected-scenario rerun. This remedy remains a hypothesis; do not add another vague
instruction or launch a full re-uplift as if this pilot had proved the amendment.


### Interpretation clarification — 16 October 2026

The lifecycle A/B pilot measures the later reliability amendment on top of a workflow
that already couples independent coverage discovery, ontology and TLA in both arms.
It does not test whether that core coupling adds value. The cited G5 session's 47→57
adequate groups on a fixed 63-group inventory, concrete source gaps and corrected
conformance assessment remain evidence of useful coverage-driven refinement.

The narrow pilot preselected the lifecycle containing two historical gaps and its common
prompt supplied five-phase completion, meaningful coverage and a scheduled checkpoint.
It is useful regression evidence but insufficient to test broad discovery or autonomous
initiation of the coupled loop. Its mixed result supports only the narrower statement:
no consistent additional semantic improvement from the tested reliability amendment was
established on this slice. Repeated omissions still demonstrate incomplete propagation
from investigated/modelled evidence to governing bodies.

The 12-fixture reader test remains a diagnostic, not the primary joint-mechanism test.
The revised next proposal is a bounded multi-domain continuation from identical incomplete
ontology and passing models: full coupled workflow versus the same tools/objective with
the explicit independent coverage ledger and feedback requirement removed. The common
prompt must not reintroduce the intervention. Freeze independent obligations, assess
before/after semantic status and conformance corrections, and retain concern-level links
from discovery through governing/model changes to refreshed evidence and coverage. Let
the baseline investigate normally and record any spontaneous equivalent loop. No new
experiment or harness modification is authorized or performed by this clarification.
The [report's interpretation section](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/evaluation/REPORT.md#interpretation-clarification--16-october-2026)
contains the source comparison, design limits and proposed measures. Existing raw results
and prior dated verification receipts remain historical evidence.


### Coverage-feedback continuation experiment — authorized 16 October 2026

The user authorized the revised comparison and reuse of the existing experiment checkouts.
This is a new experiment with separate controller records under
`/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/coverage-loop-20261016/`.
The earlier lifecycle evidence and scores remain frozen. Before reuse, archive prior
output and Git metadata with exact manifests; retain original source files in place.
No new worktree or full source checkout is needed. Prior Git history and generated
findings must be inaccessible to all new author contexts.

**Question:** does the explicit independent coverage inventory and its feedback/closure
process improve a continuation when both arms already have ontology, formal tools,
ordinary source/model reconciliation and a strong general quality objective? This is
an ablation of that coupled process, not a factorial estimate of a coverage-by-TLA
interaction and not an untouched-domain generalization test.

Use the actual pre-refinement ontology from the G5 session, with consistent removal
of later/diagnostic answer leakage, plus a documented subset of existing finite models.
Keep governing meaning and legitimate questions unchanged. Both arms receive identical
seed and frozen original production/human/design sources. Existing passing models must
be described at their actual scope; replay selected seed inputs and verify source bindings
before launch rather than passing historical receipts off as current conformance.

The assigned product areas are identity/organization administration, chat/agent execution,
collaborative workspace review, and the main workbench user experience. These broad labels
avoid naming the omitted behavior families in the author prompt. Include consequential
handoff dependencies; unrelated host/fleet and product capabilities are context. Authors
receive these broad areas, not the evaluator's missing-obligation list or later findings.
The evaluator freezes source-backed before/final criteria, valid controls, policy
exceptions and reconstruction questions before any author is launched. Previously
studied Stop/Delete terminal-event gaps are not primary efficacy targets.

Compare Full (the current maintained workflow) with Ablated (same package except the
explicit independent capability denominator, coverage measurement/ledger, separate
source-versus-ontology reconstruction challenge, and coverage-triggered phase return and
closure). Ordinary completeness, source tracing, model faithfulness, correction of
findings, prose quality, runtime-evidence honesty and bounded repair/simplification remain
required in both. The baseline may spontaneously use the removed process; record that
rather than forbidding capable reasoning. The shared prompt must not restore the ablated
instructions. Inspect every linked guide for accidental reintroduction.

Run two independently initialized matched pairs, four authors total, reusing sample01–04
with balanced Ablated/Full/Full/Ablated assignment. Use the same GPT-6 Astra/xhigh settings,
two available native children per author, two simultaneous authors, equal tool/source
access, and no competitive cutoff or corrective quality coaching. Unlike the earlier
pilot, do not force a phase checkpoint or fresh-context handoff. Use the independently
verified saved-session launcher so inherited-context native children can function, while
framework transcripts remain outside author-readable paths. Record setup failures and
all attempts; do not tune between repetitions based on outcomes.

Take passive, controller-only content-addressed observations of changing Markdown/model/
configuration artifacts. These are file-version observations, not atomic snapshots or
proof that a discovery caused a change. Retain final artifacts and raw process records.
The identical prompt requests saved work and a truthful final account, without prescribing
a coverage ledger. Preserve the factual runtime limits, including the prior sandbox
PostgreSQL limitation; compiled tests or model passes do not receive runtime-conformance
credit. Native application inputs and assigned harnesses remain read-only.

Evaluate the common starting contract and every final output against the same independent
reference, keeping intent adequacy, source-bound code findings, model/property fidelity,
conformance correction, false claims and repair completeness distinct. Fresh body-only
readers test retained meaning; source-aware reviews inspect source/binding/evidence and
exact before/final changes. Require actual support rather than a heading or model link.
Compare agreement among ontology, findings, checks and stated coverage; record spontaneous
baseline feedback processes and corrective human prompts. Do not score an arm higher
merely for emitting the treatment's prescribed file. Novel supported findings are compared
uniformly outside the original denominator. No blanket coverage percentage, model-count
reward or aggregate winner is predeclared. Selected formal/runtime replays must preserve
the difference between models, components and product execution.

Status: execution and evaluation completed on 16 October 2026. All four authors completed
without corrective coaching under the frozen settings. All five independent readers,
four source-aware reviews, common adjudication and formal/runtime replays completed.
Four existing author roots were reused with no new worktree or full source checkout.
The common seed has 20 governing pages, 408 blocks and seven inherited models; all nine
selected seed configurations replayed successfully. The independent reference fixed 24
obligations and 16 reader questions before author launch. All four source/harness/seed and
sandbox checks passed. This experiment does not modify the maintained harness. Concurrent
planning-guide edits observed after freeze are outside the tested packages and remain
recorded separately; neither arm receives them.

The controller [launch freeze](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/coverage-loop-20261016/launch-freeze.json)
pins prompts, reference, packages, sources and runner. Prior outputs/history are retained
in the exact-manifest reuse archive. A separate ontology-only baseline reader runs while
authors work; final evaluation waits for terminal processes and stable saved artifacts.
No results or semantic advantage are claimed at launch.

The [new experiment report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/coverage-loop-20261016/evaluation/REPORT.md)
tracks the independent evaluation, common semantic adjudication, replay and effort
accounting. Raw author results and evaluator judgments remain preserved separately.


Completed comparison: under the preferred high-level composed reading, the common seed
has 14 adequate fixed obligations; paired Ablated/Full finals have 14/16 and 14/15.
A disclosed post-reader interpretation audit credits already-composed durable-child
meaning equally to every specimen and Larch's composed round/selected-edit rule. Under
the preserved stricter round interpretation, the second pair ties. Raw reviews and prior
adjudications remain unchanged. Independent reader recovery is mixed: 7/9 and 9/8 adequate
answers for Ablated/Full, versus 7 in the starting ontology. All 80 answers were assessed
faithful to their available bodies; that does not make omitted meaning complete or
validate the source authority of every written rule. All 951 quoted passages matched.

The Full runs show concrete coverage/review feedback restoring governing meaning and
correcting model or implementation claims. Ablated runs also spontaneously iterate and
recover substantial additional meaning, including the settled spending-admission rule.
Shared omissions and policy/model conflicts remain; completed checkpoints do not certify
complete ontology. Full took 58.45 versus 37.57 minutes in pair 1 and 53.96 versus 47.52
minutes in pair 2. These are not effort-matched or general causal estimates.

Every saved formal configuration is accounted for: 1,037 fresh replays plus 57 exact
baseline reuse paths, 1,094 paths total. All expected outcomes, properties and exits match
after three predeclared controller parser corrections; no resource or unresolved replay
failure remains. Real selected-Save component races and narrow separate-process-group
cancellation were independently corroborated. These are bounded component/model results,
not database/browser/Linux end-to-end verification. Production repairs and complete
architecture simplification were not implemented or established.

The supported action is to retain the coupled process while testing concrete semantic
closure rather than adding broad diligence instructions. The proposed 12-fixture isolated
reconstruction/disposition experiment remains a hypothesis and is not launched. Keep its
activation limited to explicit initial uplift; this continuation does not measure routine
workflow contamination. All detailed results, interpretation sensitivities, source findings,
process sequences and effort are linked from the experiment report above.

After all evaluations finished, five manifest-excluded build-cache trees were removed
with 140 retained-file mappings verified before and after. The deletion-phase available
space reading increased 7.79 GB (7.26 GiB), subject to whole-volume/APFS accounting;
preservation copies remain retained. All 9,484 pinned author outputs, four immutable input
sets, prior archives/overlays/artifacts and frozen harness checks passed afterward.
[Cleanup and final integrity](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/coverage-loop-20261016/evaluation/runtime-audit/cleanup-plan/removal/REPORT.md).

### Proposed operational discovery and reconciliation tests — 16 October 2026

Status: the user subsequently authorized execution on 16 October 2026. All 72 scored
trials completed on 17 October, with final semantic, structural and runtime audits.
Neither addition passes the predeclared advancement screen; no conditional transfer or
candidate installation followed. The maintained harness is unchanged by this experiment.
The frozen design below tested two changes separately before
a combined continuation. The earlier 24 obligations are diagnostic examples, not an unseen efficacy
benchmark. Repairing those known omissions would not establish independent discovery.

The hypotheses are (A) withholding inherited ontology from one discovery pass reduces
anchoring and improves recovered meaning, and (B) making an independently checked
reconciliation result a checkpoint dependency reduces missed integration and incorrect
closure. Existing guidance already requests both independence and reconciliation. Test
operational differences, not stronger wording or an intentionally weakened baseline.

#### Common controls and preparation

- Freeze the current maintained harness for both arms, including changes made since the
  previous experiment. Pin the exact treatment delta, runner, fixtures, prompts, expected
  outcomes, semantic rubric and stopping rules before scored launches. Earlier concurrent
  planning-guide changes must not become an arm difference.
- Use fresh sessions without this conversation, known G5 omissions, other outputs or
  evaluator references. Use the same model/reasoning, tools, source access apart from the
  intentional discovery restriction, reviewer opportunities and per-case resource limits.
  Balance launch order. Allow spontaneous equivalent reasoning in the control.
- Build small self-contained sources and concise authority records, with several ordinary
  capabilities and incidental implementation choices. Do not name the seeded omission in
  the task, fixture directory or diagnostics. Include documented intent, implementation-
  derived decisions and genuine unresolved policy; code alone is not a policy oracle.
- Validate references before execution using explicit source/authority bindings and concrete
  permitted/forbidden scenarios. Independently challenge alleged omissions against composed
  ontology owners. Replay supplied finite models, positive controls and deliberate broken
  controls before freezing them. Do not define success as matching exact prose or IDs.
- Use unscored calibration fixtures only to check access isolation, launch/replay behavior
  and cost. Set identical per-arm limits from calibration before scoring; a truncated run
  is incomplete. Preserve all attempts and measured effort. Do not tune on scored outcomes.
- Reuse an existing experiment worktree with isolated small directories and access roots;
  preserve prior evidence and keep it inaccessible to new authors. No new full checkout or
  whole-application build is needed. Paths alone do not enforce context separation: restrict
  actual file/tool access and verify that excluded seed/history material is inaccessible.

#### Experiment A: source-only discovery versus ontology-visible discovery

Create three small source packages with different discovery surfaces: public user/API
operations, background retention/recovery, and alternate entry paths crossing owners.
Each has multiple legitimate capabilities. For each package prepare two starting contracts:
one adequately covers the selected meaning, the other omits a consequential capability or
condition. Source and authority records are identical between those two variants. This
produces six cases; the complete variants test preservation and over-extraction.

| Arm | Discovery | Subsequent integration |
| --- | --- | --- |
| A0: current guidance | Fresh investigator receives source records plus inherited ontology and model inventory; current independence instructions remain. | Common coordinator receives all sources, seed and discovered inventory and performs initial contract/model-obligation integration. |
| A1: isolated discovery | Equally fresh investigator receives the same source records and broad scope, with seed ontology, candidate ledger and models inaccessible until its inventory is saved. | Identical integration stage and available information after the inventory is saved. |

Both arms have the same investigator/coordinator opportunities. The A1 inventory is
retained before reconciliation; it can subsequently be corrected, with changes recorded.
Evaluate phase-1 discovery and phase-2 integration separately. A useful inventory item is
not yet a recovered governing obligation. Integration must identify the owning body or
specific justified Q and the consequential obligation to check formally. This experiment
does not claim a TLC efficacy result and does not launch a full model campaign.

Run two independent repetitions of every case: **6 cases × 2 arms × 2 repetitions = 24
short trials**. Primary comparisons are source-backed discovery, unsupported inferred
requirements, adequate retained body meaning and sensitivity to complete versus incomplete
seed. Semantic inventory coverage should depend on sources, not on which seed was supplied.
Report downstream integration failures even if discovery improved.

#### Experiment B: ordinary review versus enforced semantic reconciliation

Use **12 fixtures**: four failure/control families, each with a complete, missing and
conflicting variant. Supply small source/authority packets, ontology, finite TLA models
and saved investigation evidence. The families are:

1. Consequential source behavior absent from the candidate/coverage inventory, or wrongly
   excluded despite confirmed scope. The complete variant already accounts for it.
2. An important investigated/modelled condition absent from the governing body, or a body
   inconsistent with that condition. The complete variant already expresses it.
3. A legitimate policy exception and an overstrong formal property. Variants distinguish
   an omitted exception, a conflicting property and an already faithful contract/model.
4. Compact meaning distributed across valid shared owners. Variants distinguish sufficient
   composition, a genuinely missing clause and a contradictory owner. Extra duplication or
   restriction must not earn recovery credit.

Include at least one precise unresolved-policy case whose correct disposition is a Q
with conditional findings, not an invented policy. Include both a genuine source defect
and a model/binding error so withdrawing every finding cannot pass. Freeze the exact
expected distinctions before authors run.

Both arms receive the same initial independent comparison of the identical seed, made
without evaluator answers. Both receive two review/repair opportunities under the same
limits. Reviews use separate source-derived expectations and body-only reconstruction,
then inspect property assumptions and results. A body reader cannot borrow missing meaning
from implementation or TLA. Reviewers can be wrong: authors may refute an alleged omission
using composed governing meaning and source authority. Neither arm receives the hidden gold.

| Arm | What differs |
| --- | --- |
| B0: current guidance plus matched review | Agent receives independent findings and may repair/recheck using the current workflow; completion remains its normal checkpoint judgment. |
| B1: enforced reconciliation | Same review opportunities, but checkpoint closure depends on an independently checked disposition of every material mismatch and completion of affected required checks. The runner retains an open checkpoint when these conditions are unmet. |

The treatment reuses the existing coverage record. Required comparison is source-backed
obligation → actual governing meaning → checked property/assumptions/result, including the
reverse question of whether modelling introduced meaning missing from the body. A reference
or field filled with text is not semantic evidence. Dispositions may be a verified correction,
an accepted refutation/design freedom, a source-supported scope exclusion, or a precise Q
with remaining conditional claims. Unfinished investigation cannot be relabelled uncertainty.
An independent semantic reviewer checks dispositions; the runner enforces sequencing and
open/closed state. This is not a deterministic natural-language correctness checker.

After each revision, run affected small TLC checks and reconstruct affected scenarios from
the resulting bodies. Check preservation of valid completion as well as rejection of the
bad trace. Both arms get the same number of review/repair opportunities; the treatment must
not win solely by receiving additional reviews. Two rounds are an experimental resource
limit, not a proposed permanent workflow limit. Remaining mismatches yield an incomplete
result; do not force a clean report or launch unbounded recovery.

Run two independent repetitions: **12 fixtures × 2 arms × 2 repetitions = 48 short trials**.
Judge actual governing meaning, correct property/exception alignment, supported defect
classification, executed affected checks, and truthfulness of completion separately. A gate
that only reports incomplete more honestly has demonstrated a reporting benefit, not improved
semantic recovery. Keep the reference evaluator separate from the operational reviewers.

#### Decision rules, isolation and transfer

The initial screen is 72 short trials, plus unscored calibration and routing checks, not
72 repository uplifts. Publish paired results and effort for every case and repetition;
preserve raw disagreements and adjudicate symmetrically with evidence. Do not use a single
overall quality score or count blocks, links and configurations as recovered meaning.

Predeclare the following screening decisions:

- Advance a mechanism only if its intended semantic outcome improves in both repetitions
  and across at least two distinct source packages (A) or fixture families (B). Recovery
  gains confined to the inventory or to more honest incomplete labels have narrower claims.
- Require preservation of all clean controls, no invented governing restrictions or false
  code-gap claims in those controls, and no closure despite a known unresolved material
  mismatch. Do not trade these failures against extra recovered obligations.
- If the strong baseline already succeeds, conclude no demonstrated incremental benefit
  on these cases. If both fail, investigate the mechanism rather than adding instructions.
  Small repeated fixture results remain a screening signal, not statistical generalization.
- Check ordinary bounded fixes, approved implementation, incremental design, ordinary review,
  completed-uplift summary and downlift preparation in fresh sessions with the candidate
  package and a realistic skill catalog. None should activate uplift or its new workers.
  Include an explicit-uplift positive control. A routing failure blocks promotion.

Only after a mechanism passes, propose a separate transfer stage on two untouched domain
slices, including one different codebase where available. Select and freeze those sources
and evaluation criteria before inspecting transfer outputs. If both mechanisms advance,
compare current / A only / B only / A+B with two repetitions per slice (16 bounded
continuations); if only one advances, compare it with current (8 continuations). Keep
reviewer opportunities matched and report effort. This distinguishes separate effects and
combination effects, and tests whether artificial-fixture gains survive real source breadth.
It is not a full G5 re-uplift or proof of architecture simplification quality.

No mechanism is promoted merely because its wrapper ran correctly. Promotion requires
recovery/preservation evidence and routing checks; broader reliability claims require the
transfer stage. The later instruction to run the experiments authorizes this protocol;
the transfer stage remains conditional on its stated screening results.

Execution root: `/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/`.
Small phase directories reuse the existing `runs/sample-01` checkout. The current maintained
harness was copied once for equal use in all arms; no maintained skill is changed. The
runner's first live probe confirms assigned reads/output writes and denial of hidden
references, sibling runs and session history. Formal replay runs in the controller because
the model sandbox denies TLC's local RMI listener; logs are supplied identically to both
arms. Both B arms receive two repair opportunities plus a matched read-only final handoff
after final replay/review, so completion judgments see the actual final check results.
Calibration, reference review and launch freezing precede scored execution.


Operational execution checkpoint: the scored freeze pins 773 files at SHA256
`5d53d27d58c27884b64afba29a4b09860317e35923f6a5096e7eb40e40941ac7`.
[Protocol](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/protocol.md),
[freeze](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/freeze.json),
and [launch receipt](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/launch.json).
Fourteen unscored calibration phases completed; the longest took 506.002 seconds.
The common scored ceiling is twice that duration rounded up to five minutes: 1200 seconds.
An early draft controller failed while overlaying read-only replay inputs; that attempt
was preserved and calibration continued from the completed first round using the already
audited replacement-copy fix. Calibration is execution/cost evidence, not an efficacy score.
All 12 negative routing cases avoided uplift activation, and both explicit-uplift controls
prepared its intended dependencies. The CLI disabling flag still advertised collaboration
tools, so only observed absence of child activation is claimed. Unexpected children fail
the terminal barrier; each scored role explicitly prohibits delegation.
Experiment A finished on 17 October 2026 at approximately 03:00 UTC. Its additive
[terminal semantic audit](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/review/results-a/report.md)
retains all raw grades: 72/72 adequate reference rows per arm for discovery, governing
meaning and named future formal obligations; all 12 paired comparisons tie. All 24
bounded completion claims are accurate, and complete controls preserve valid meaning.
Both arms recover the deliberately omitted move-refusal preservation, selected-batch
failure isolation and cross-path hold semantics. A therefore does not pass the
predeclared incremental-gain screen. This is a strong-baseline ceiling on small source
packets, not evidence against the common coverage/ontology/formal-obligation workflow
or a general claim that source-only discovery cannot help. A newly reproduced returned-
text newline-fidelity defect is retained as an exploratory supported code gap, not a
retrospective reference-row or treatment gain. No A transfer or harness promotion is
justified by these results alone.

Experiment B completed all 48 trials. Both arms recover the targeted missing/conflicting
meaning and preserve sufficient shared-owner composition. Each arm retains the original
96 governing-row denominator: raw grades are 90 adequate and six partial, with all six
partial rows belonging to the same print-room P01 rejection-frame authority ambiguity.
Admission/reservation meaning is adequate; code cannot silently supply a stronger policy.
The additive audit qualifies these rows symmetrically without overwriting raw grades.
Classification and verification judgments are 24/24 adequate per arm. No family has a
replicated adequate gain on any fixed primary measure, so B also fails the advancement
screen. This comparison retains strong source/body/formal review in both arms and does
not measure the causal value of that common process against its absence.

The gate prototype has a confirmed scope error in all six enforced print-room trials.
Independent comparisons accept bounded reconciliation with the documented source defect
left for later implementation, but the mechanical gate rejects every historically
material issue still labelled open. Ordinary closes 24/24 checkpoints; enforced closes
18/24 and falsely keeps these six reconciliations open. One author reports partial solely
because of this disagreement. Reconciliation disposition and implementation repair
status must be distinct; the existing protocol already permits that distinction. This
is evidence for correcting the prototype's state contract, not adding generic agent
instructions. The frozen controller was not repaired during scoring.

The formal-alignment metric cannot measure exhaustive verification breadth: narrow and
broader honest models can both be adequate. Supplementary evidence therefore remains
separate and substantive. Conditional queue-eligibility checks, sensor effect checks and
notice duplicate/status checks recur in enforced specimens across both repetitions.
Exact-refund assertions distinguish under-refund states allowed by the prior upper-bound
properties; one run supplies an actual missing-refund mutation. Other expansions do not
repeat, and one stale-response control also appears in the ordinary arm. Pledge models
remain byte-identical and honestly narrow across all 12 finals. These are promising
additional verification observations, not retrospective primary scores or proof that the
mechanical gate caused them. A focused future test should freeze behavior-level mutations
and valid completion witnesses to measure this breadth directly.

All 492 scored model phases completed once, with no retries, citation repairs or phase
failures. All 108 replay invocations ran 473 configurations with no timeout/unexecuted
case. Three intermediate runtime errors in one expanded model were retained and repaired
in the next round; affected configurations then produced their intended control/witness
results. Counterexamples and historical model errors were not relabelled as safety
passes. All 72 final ontologies passed the frozen structural checker: 277 Markdown files,
656 blocks and zero issues. These are structural counts, not semantic coverage.

Elapsed scored execution was 7h 32m 57.811s. Scored native usage was 187,346,779 tokens,
including repeated/cached context: 183,077,925 input (160,233,984 cached subset) and
4,268,854 output. Enforced B used 13.68% more native tokens and 5.75% more summed phase
time than ordinary. Preparation, coordination, calibration, routing and post-terminal
audits are excluded from those totals. No new checkout was created; evidence is retained.

The read-only final handoff updates only its completion record. Later actual replay
evidence may supersede earlier pending-execution coverage/findings text; this study does
not establish unattended synchronization of every final artifact, downstream phase gating,
whole-repository completeness, production conformance or architecture-simplification
quality. All 12 negative routing probes avoided uplift, and both explicit positives
selected its dependencies, within the supplied local catalog only.

The [final findings report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/operational-20261016/REPORT.md)
links immutable raw comparisons, symmetric semantic adjudication, model/error history,
runtime/accounting receipts, structural results and exact effort. No new mandatory
semantic-recovery instruction is validated by this screen. The maintained workflow and
all unrelated working changes are preserved; no commit, push or PR was made.

### Targeted Claude follow-up — 17 October 2026

The human authorized the proposed checkpoint tightening and targeted reruns, with most
implementation, TLA/TLC and analysis delegated to logged-in Claude Code Opus 5.5 at xhigh.
Execution uses `control/targeted-claude-20261017` and small phase directories under the
existing `runs/sample-01` checkout. The completed operational experiment remains immutable.
A live capability probe confirmed `claude-opus-5-5`; exact argv, native events and exit
records are retained. This is a new within-Claude comparison, not a cross-model comparison
against the previous Codex scores.

The deterministic part corrects the prototype's reconciliation/repair state conflation.
It must close the six historically accepted bounded reconciliations while preserving their
open implementation defects, and continue to block unexplained or unverified material
findings, lost historical IDs and missing required checks. Tests must exercise actual
downstream dispatch and consistency of final records, not just an eligibility label.
The maintained uplift guide already permits completion with known code gaps; the defect
was in the experimental controller, so another generic instruction is not the remedy.

The paired part measures verification breadth directly. Preparation targets three small
new finite transition models, with governing ontology, honest narrow initial checks,
held-out behavior mutations and legitimate completion/exception witnesses. Both arms get
the same comprehensive verification objective, inputs, tools, repair opportunities and
model settings. The candidate makes each claimed obligation's distinguishing check and
completion evidence explicit in the existing coverage record. A model/property count or
an honest narrow-model alignment score cannot establish increased detection coverage.
The intended screen is two repetitions per arm per family (12 author trials); final
fixture definitions, score rules, resource limits and prompts will be frozen before launch.
Hidden mutations are never used as author repair feedback. Correct-model failures, parser
errors, runtime errors and forbidden restrictions do not count as mutation detection.

Scored authors receive only serialized packets and have no model tools; an external
controller materializes validated outputs and runs TLC. Fresh sessions, safe mode, empty
MCP configuration and exact native tool/model checks isolate the instruction comparison.
This deliberately does not test repository discovery, skill routing or full uplift
autonomy. Final synchronization is writable after public replay and must agree with actual
evidence. Claude preparation workers have tools in separate owned directories; that
preparation is not represented as the scored no-tool isolation boundary.

Execution checkpoints: (1) Claude implements gate, benchmark and runner in parallel;
(2) coordinator and independent Claude review raw fixtures, oracle and isolation evidence;
(3) freeze inputs and predeclared scoring; (4) run matched author/repair/synchronization
phases and independent replay; (5) Claude analyzes fixed scores, coordinator verifies
findings and records the scoped result here. Failed preparations remain visible. No new
mandatory workflow instruction is promoted solely from exploratory additions or this
small packet screen.


<!-- targeted-claude-results:start -->
The follow-up is complete, including independent Claude review and coordinator adjudication.
Most construction, TLA/TLC, verification and analysis used Claude Code Opus 5.5 at xhigh.
No cross-model causal comparison is made with the prior Codex campaign.

The deterministic checkpoint correction reconstructs six historical print-room records
(one shared fixture source defect) and reproduces 38/38 TLC outcomes. Six supported new
review dispositions allow reconciliation to finish while repairs stay open. Five dispatch
from existing complete reports; one needs a labeled synthetic, evidence-bound synchronization
record. The historical author did not rerun. The revised prototype passes 28 tests, 16 legacy
tests, 3,000 sampled legacy comparisons, nine rule-removal mutation checks, 16 negative
controls and three positive dispatch controls. The old frozen controller is unchanged.

A three-line clarification is applied to the maintained uplift execution guide: reconciliation
disposition and implementation repair status are separate. Unsupported classifications and
missing required checks still block completion. Entry points and activation conditions are
unchanged; mirrors, skill validation and whitespace checks pass.

The reviewed semantic release remains frozen at
`1e3d8ca961a5d737f1ced91f1be19a9de3fd83a8b25cda715dbe1c5fc3d92a6f`
(155 files). All 12 trials mechanically finalized over 36 phase invocations in 83m51.6s;
hidden scoring took 5m42.8s. Both arms detect 36/36 mutation instances and preserve/cover
12/12 outcome instances. All six raw pairs tie at 6/6. Independent raw-log reaggregation
matches exactly. The extra 82-word distinguishing-behavior/check instruction has no
demonstrated advantage; this ceiling result does not establish no effect or full coverage.
Both arms already use ontology, coverage and TLC jointly.

A secondary review validates 28 additional clause probes. One initialization defect survives
every check in vault-ordinary-r1 and vault-linked-r2: starting the wrong-PIN count at one
permits lockout after a single wrong PIN, despite claimed coverage of the consecutive-count
and two-failure threshold. The other two vault suites detect it. The reference suite also
misses it. This is one verification gap repeated in two outputs, one per arm, not a new
production defect or a posthoc treatment advantage. The proposed mandatory named Init check
is not adopted: the failure is demonstrated, but that general remedy was not tested.

One author phase, triage-linked-r2/r1, received an internal Claude Code retry prompt and
used two native turns. No tool executed or controller rerun occurred. Raw scores and usage
remain unchanged; the affected pair is qualified. The other five pairs also tie at the
ceiling. A separate experimental runner now rejects extra user events, multiple distinct
assistant messages and non-one native turn counts; seven focused tests and 35 runner tests
pass. Two live turn-cap probes did not reproduce the retry, so prevention is unverified.

Review also reproduced a TLC classifier defect: source-location names in genuine conjunct
violations were treated as errors. The separate corrected benchmark module passes four
regressions and preserves all 3,210 original log classifications. Both experimental fixes
need a new protocol freeze before future scored use; frozen evidence is untouched.

The coordinator stopped and resumed the unscored review after detecting unbounded secondary
mutations. Interrupted matrix logs are retained; finite replacements and excluded probes
are explicit. The worker deleted two preliminary smoke-test outputs before the full matrix
and later reproduced them; the preliminary review trail is therefore incomplete. All
original author records, the 4,683-file review input packet and 773 old frozen files remain
unchanged. No new worktree, commit, push or PR was created.

The [completed findings report](/Users/ziniuw/Desktop/research/g5-uplift-lifecycle-experiment-20261015/control/targeted-claude-20261017/REPORT.md)
links raw scores, independent findings, counterexamples, corrected components, accounting
and limitations. No new mandatory semantic-recovery instruction is promoted. Whole-repository
discovery, production conformance, architecture-simplification quality and general skill
routing are not measured by this packet experiment.
<!-- targeted-claude-results:end -->

### Publication CI diagnosis — 18 October 2026

Both PR #571 checks at `d6594995a32364baece4545a51a669f9a7140aa0` failed in
`build-ci-image`, before their dependent Rust or E2E suites ran. Rust build
`5f40c59b-9c72-4df6-a5da-cffb1e99c512` and E2E build
`0571c6f0-2a25-4ec2-bb78-828d3f60d4d5` both report:
`network mode "cloudbuild" not supported by buildkit`.
The documented Cloud Logging view denied access; `gcloud builds log` successfully
returned both complete logs. The PR has no runtime, Dockerfile or CI configuration
changes relative to its merge base.

Upstream commit `774b54187ffbe121442762cd26dd4cd1a64bf175` already removes the
unsupported network option from both CI image builds. The authorized resolution is
to incorporate current `main`, including that fix, through rebase while retaining
the branch's docs and harness changes. No separate CI policy change is needed.
Checks on the rebased commit must be reported separately from these historical failures.

The rebase onto `49d0187db4bf23e5cc83924c4262bb8d9ccf096a` completed without
conflicts. Range-diff and byte-for-byte patch comparison preserve all four original
branch commits; `.g5` remains identical to current `main`. Both CI image steps contain
the upstream fix. Post-rebase validation passes 28 ontology-checker tests, 22 Cloud Build
configuration tests, skill-copy consistency and whitespace checks. The current ontology
scan finds 20 files and 449 blocks with zero structural issues. Full CI execution remains
a separate check on the published rebased head.
