# Twenty downlift lifecycle and session instructions

Research retrospective, 12 October 2026. All times below are America/New_York (EDT).

The governing objective of the Twenty downlift was a minimal sufficient Rust backend: preserve the selected core intent with fewer moving states, tractable proof obligations, clear model boundaries, good abstraction and encapsulation, and room to add deferred features without rearchitecting the core. The work proceeded through design, implementation, parallel implementation reviews, reconciled repairs, and a further design revision for scale and operation. Human instructions changed both the scope and the verification work: they rejected webhooks, required the named 18-outcome core, chose atomic administrator replacement, authorized implementation, requested read-only reviews, then separately authorized repairs and another TLC phase.

This study starts with the new-core design request on 11 October at 22:17. It treats the recovered Twenty ontology and source dispositions as supplied inputs; the earlier uplift investigation is out of scope. The records include an executed implementation and subsequent code reviews, not only a proposed implementation. They do not establish full Twenty parity, a whole-program proof or production deployment qualification.

The chronology records **19 human request turns across four chats, plus one policy reply inside a turn**. Eighteen requests had completed responses at the reading cutoff; the final ontology/Verus handoff request was still in progress at 19:02. Exact prompts are preserved below with whitespace and HTML-space encoding normalized. Tool outputs, injected skill instructions, subagent hand-backs, compaction summaries and automated monitor messages are not counted as human turns. Overlapping turns are ordered by their starting time.

## Your minimal-design instructions and how they governed the work

Minimal design was an explicit instruction to both agents, repeated and refined across the session. The first request stated it twice: first as the objective for the new core, then as the criterion for reviewing and incrementally improving its ontology through TLA+/TLC. The later requests required concrete evaluation, sufficient core functionality, broader design criticism and concise ontology. The table paraphrases those instructions; the numbered turns below preserve the exact prompts.

| Your requirement | Where you instructed it | What the agents did with it |
|---|---|---|
| Create a new design and ontology that satisfy the old core intent; use the recovered intent, code gaps and model evidence as inputs. | Codex turn 1, items 2–3. You explicitly distinguished this from refactoring the old codebase. | Created the separate Rust target, mapped source intent to retained/narrower/deferred/omitted outcomes, and kept the source implementation as evidence. This made scope reductions visible. |
| Minimize moving states and make TLC obligations easier, with clean model boundaries. | Codex turn 1, items 2–3; Claude turn 2 requested quantitative state-complexity evaluation. | Selected one durable authority, challenged redundant protocols, compared alternatives under matched bounds, and separated owner concerns with explicit composition premises. |
| Preserve good modularity, abstraction and encapsulation while minimizing. | Codex turn 1, items 2–3; Claude turn 2 scored these as separate dimensions. | Defined four semantic owners, complete domain operations, one-way dependencies, private representations and a pure decision kernel. Later implementation review tested whether public consumers could actually bypass those boundaries. |
| Start with the Rust backend core, allow reasonable dependencies, and support missing features later without rearchitecting the core or losing those structural qualities. | Codex turn 1, item 2. Frontend and immediate feature completeness were excluded. | Used a modular monolith and PostgreSQL, defined module authority/work/content/drain boundaries, and added Views as a first extension. The strength of the future-extension claim remains limited, as discussed below. |
| Reduce the obligations of later Verus verification. | Codex turn 1, item 3; Claude turn 2 requested an estimate of hypothetical proof complexity. | Removed a redundant claim protocol, bounded authorization recursion, separated pure decisions from I/O, and planned proof of production predicates. Actual proof-effort reduction was not established quantitatively. |
| Review and incrementally improve the design and ontology, confirm core intent/function, then create the implementation plan from that ontology. | Codex turn 1, items 3–4; Claude turn 2; Codex turn 5 asked it to examine Claude’s amendments. | Repeatedly amended the same governing artifacts, reconciled independent findings and carried unresolved premises into model refreshes, proof feasibility work and runtime checks. |
| Reach 18/18 core comprehensiveness, without adding webhooks. | Claude turn 4. | Filled missing outcomes largely through existing owners and operations; removed the agent-added webhook product. This set a functional floor for the smaller design. |
| Explain how the quality measures work, how a better design was found, what TLC contributed, and what remains outside those dimensions. | Claude turns 6 and 9; turns 10–11 saved and expanded the evaluation. | Documented the necessity questions, model comparisons and judgment-based rubric; expanded review to operational, lifecycle, coverage and structural dimensions. Those additional dimensions were evaluation criteria, not blanket authorization for new features. |
| Keep ontology blocks to 1–2 sentences, with children for the more formal specification. | Claude turn 19. | Split long commitments into concise parents and formal children. This concerned readable specification structure; a higher block count after splitting does not imply a more complex architecture. |

### How the agents searched for a smaller sufficient design

The strongest procedure was to ask **what obligation each mechanism discharges, whether another mechanism already discharges it, and what breaks if it is removed**. Claude’s [review method](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/design_review_method.md:23) records six questions covering state necessity, hot-path cost, failure progress, proof difficulty, dependency enforcement and missing capabilities that existing owners could provide.

- **Consolidate authority and transactions.** PostgreSQL holds durable truth; complete local commands commit policy-dependent effects, receipts and required consequences together. The design avoids independent queue acceptance, permission truth and deduplication stores. Workspace gates preserve tenant independence while deliberately limiting concurrent writers within a workspace.
- **Remove redundant coordination.** Local claims, leases, renewal and expiry duplicated the unsettled recheck plus atomic effect/settlement transaction. The matched comparison fell from 147 to 59 states; an unfenced-claim variant also remained safe. Removing settlement protection produced duplicate effects, distinguishing necessary protection from redundant machinery.
- **Move synchronization cost to the operation that needs it.** Ordinary writes use one workspace gate and reads use one consistent snapshot. Account-wide changes lock the account and its complete, sorted workspace set. The selected ordinary-command comparison fell from 47,176 to 28,350 states, while the rare account-change comparison increased from 186 to 279. This was an explicit tradeoff, not a reduction in every model.
- **Simplify proof shape and share semantics.** A decreasing bounded rank makes authorization recursion terminate; the follow-up extended it across row-policy and inheritance edges. The pure kernel isolates decisions from SQL and network behavior. This also narrows supported graphs, which the source map must acknowledge. Runtime loading and faithful application of those decisions remain separate obligations.
- **Add useful outcomes through existing abstractions.** Import uses batch upsert, export uses snapshot queries, search uses permitted query semantics, and Views use query validation and owner operations. The feed uses mandatory immutable change facts and workspace ordering. These choices avoided a separate lifecycle for every capability while still adding the state each outcome requires.
- **Test the claimed boundaries in code.** Dependency diagrams and private raw pools did not prevent every public bypass. Independent consumer probes exposed arbitrary owner selection and forgeable authority; repairs enforced owner-specific composition and transaction-bound authority. This was necessary to deliver the requested encapsulation.

The numerical comparisons and their broken controls are recorded in the [quality review](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md:82). Structural improvements came primarily from design reasoning and consumer checks; TLC supplied bounded evidence about state, interleavings and progress.

### Where the minimal-design brief remains only partly demonstrated

**Lower proof burden is a design argument, not a measured final result.** Claude’s historical estimate actually grew from 6 to 7 proof units and from 5,200–7,400 to 5,400–7,600 specification/proof lines as the planned guarantees expanded. It argued that recursion and trusted boundaries became easier. The later 99 verified units establish selected kernel results, not a comparison proving less total Verus work than another implementation. These are different measures of the original requirement.

**Extending without rearchitecture is only partly exercised.** Views and the extension-consumer checks provide concrete evidence for some boundaries. They do not establish that every deferred workflow, mail, billing, app or large-content lifecycle fits without revising the core. The saved design explicitly declines a guarantee for arbitrary future features. That is a qualification of the original ambition; the retrospective should not report the whole extensibility requirement as proved.

**Sufficiency depends on the selected operating envelope.** Whole-workspace loading initially avoided a second query implementation, but its small capacity bounds later motivated working sets, indexes and counters. Those additions brought new consistency obligations and three new models. They illustrate why minimality must be assessed against required behavior and operating conditions. Lower line counts, fewer ontology blocks, fewer total model configurations or a smaller feature denominator cannot alone establish a better design.

## Process and consequential changes

| Phase | Procedure and result | What challenged the previous result |
|---|---|---|
| Select the target | Freeze the source baseline; map retained and deferred intent; choose Access, Data, Work and Host with PostgreSQL as the durable authority; produce target ontology and an implementation plan. | Source-preservation and model reviews corrected policy, identity, transaction and assertion gaps before the initial design handoff. |
| Challenge the design | Claude inspected state necessity, gate cost, proof shape, architecture and feature coverage; ran alternative and broken-control models; amended the same artifacts. | Claims and leases were redundant for local work. Passing safety checks alone had not shown that. Human feedback removed added webhook scope and completed the selected CRM checklist. |
| Reconcile before implementation | Codex challenged the amended design and evidence across Access, Data and Work, then recorded the last-administrator decision. | New gaps concerned composed operations: every account-growth route, abandoned admissions, mixed authorization recursion and feed disclosure. Existing results did not cover all amendments. |
| Implement complete operations | Prove a feasible production kernel, qualify storage premises, refresh affected models, implement vertical slices, and test real HTTP/SQL/worker failures. | Runtime tests found replay disclosure, restored future-key admission, navigation cleanup and CSV output defects. Bounded Rust query evaluation simplified semantics while imposing a small capacity ceiling. |
| Review the implementation | Claude examined broader design dimensions; a fresh Codex chat reproduced contract violations and tested ordinary extension consumers. | Good protocols and passing suites coexisted with public owner/authority bypasses, omitted checks, unreachable recovery and operational weaknesses. |
| Reconcile and repair | Preserve both reports; adjudicate disagreements; separate C01–C19 current-scope work, D01–D06 policy choices and E01–E08 extensions; implement five closure slices. | Review recommendations were not all defects or human decisions. New tests and a late capability check refined the repairs. |
| Revisit the operating envelope | Claude reviewed the closure, fixed A1–A13 and selected scale, migrations, record-history erasure and key rotation for further work. | The first minimal query representation did not meet the agent-selected larger scale. New mechanisms required new models, regression tests and measurements. |

The common loop was **instruction → governing contracts and evidence → design or counterexample → amendment → implementation where authorized → discriminating checks → scoped handoff**. It was iterative: later reviews changed earlier conclusions, and the final scale work changed a mechanism whose simpler predecessor had already been implemented and tested.

## Human instructions and agent procedures

### 1 Codex selects and checks the new core

**Codex design and implementation; Oct 11, 22:17 to Oct 11, 23:00.** [Conversation source](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d).

**Your instruction:**

> great job!
>
> 1. please commit everything.
> 2.  based on all your TLA/TLC model check, code_gaps, our derived intent for this codebase, we need to come up with a new design, a minimal design with fewer moving states, easier proof obligations for TLC, clean model boundary, good abstraction, encapsulations. This is not a refactor attempt, this is a description for a new ontology, new codebase that satisfy the old intents of this codebase. To start, we will only build the backend core of this new application, so no frontend and does not need to be feature complete by building the missing features on top of our new core backend should not evolve re-architecture and can easily preserve our good modularity, abstraction, encapsulation. The new backend will be in rust, you can describe any reasonable external dependency as needed. Put the description for the new core in twenty/docs
> 3. Based on the new core's description, create a new ontology in /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean. Review and run the TLA/TLC again on the new design and ontology and make any incremental updoate to make sure the new design is minimal, with best model boundary, good abstraction, encapsulations. It should have less obligation for a Verus formal verification after the implementation. Also confirm that this core will satisfy the core backend intent and function of our current twenty.
> 4. Create an implementation plan to implement this following the new ontology.
>
> DO this job carefully.

**Procedure and outcome:**

Codex committed the source baseline, read the recovered contracts and known gaps as design inputs, and created a separate target rather than modifying the old backend. It assigned source-intent mapping and Access/Data model work to scoped agents, while coordinating the four-owner design and shared transaction/external-effect boundaries. It preserved initial inputs, mapped all 151 source families and 463 source blocks to retained, narrower, deferred or omitted outcomes, and compared selected mechanisms.

Design and model reviews corrected missing account/issuer protection, one-use identity proof, replay expiry, permit reconstruction, inherited sharing, catalog defaults and content ownership. They also challenged the assertions: projection needed exact fields and values, and cleanup identity and age needed independently discriminating controls. Codex amended the same ontology/design and reran affected checks.

**Result:** 63 target blocks, 13 models and 320 configurations with matching expected outcomes; a vertical-slice implementation plan and selected production Verus obligations. This was a design-only milestone. Twenty commits were `c5060489` and `63c7d443`; the target design commit was `19bb194`. See the [design](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty/docs/rust_core_design.md), [initial design review](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/design-review.md) and [core model review](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/core-model-review.md).

### 2 Claude scores and improves the initial design

**Claude Code; Oct 12, 08:43 to Oct 12, 09:16.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:4).

**Your instruction:**

> we just completed the uplift from source code to ontology, please read twenty/docs/rust_core_design.md to understand our minimal design for a new clean codebase at /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean
> you should review the ontology and the design to make sure: the design is of very high quality, use concrete quatitative score on TLA/TLC state complexity, Hypothetical Verus formal proof complexity (if the code were to be generated), modularity, abstraction, encapsulation, core feature comprehensiveness and overall feature coverage. You can iterate and incrementally improve the ontology and /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_plan.md

**Procedure and outcome:**

Claude read the design, ontology, source dispositions and saved formal evidence before creating QR01–QR10 and baseline scores. It used seven dimensions: TLC complexity, estimated Verus difficulty, modularity, abstraction, encapsulation, core comprehensiveness and overall coverage. It compared local-work mechanisms, account/workspace gate protocols and an external-effect consumer under explicit finite bounds.

The lease-removal hypothesis came from inspecting the existing atomic settlement rule. TLC supported it at 59 states versus 147 for claims; a guard-removal control showed that settlement, not the claim token, carried effect safety. Gate comparisons used two accounts/two workspaces (28,350 versus 47,176 states). Writing progress properties exposed a crash-loop gap. Structural reasoning produced dependency enforcement, module-operation descriptions, a command-outcome vocabulary, bounded authorization recursion and production-path disclosure checks.

Claude also added webhooks without an explicit human request, describing them as a scope choice at handoff. It rewrote the ontology, design and implementation plan, retired obsolete lease evidence and kept the new model ledger reproducible.

**Result at this intermediate point:** 15 distinct models/355 configurations and 76 blocks. Scores were the repair author’s self-assessment; Verus effort was estimated, not measured. The webhook addition was subsequently reversed. [Quality review](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md).

### 3 Claude resolves the report location

**Claude Code; Oct 12, 09:27 to Oct 12, 09:28.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:710).

**Your instruction:**

> there is no twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md

**Procedure and outcome:**

Claude checked the file and supplied its absolute path in the sibling twenty-clean repository. It explained that the relative path and untracked status could obscure it. This was a navigation clarification; no new design or verification milestone followed.

### 4 Claude completes the named core checklist and removes webhooks

**Claude Code; Oct 12, 09:29 to Oct 12, 09:39.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:737).

**Your instruction:**

> 1. no, this file is in the disk locally.
> 2. we must get to 18/18 Core feature comprehensiveness
> 3. no need to add webhooks
> 4. incrementally update ontology and plan now

**Procedure and outcome:**

Claude removed the webhook feature, plan stage and source-map additions, retaining the declined model/results outside active totals. It added QR11 and filled the named core checklist using cross-object search, CSV/validate-only import, streamed export, a pull change feed, and Views/favorites as the first extension.

The ChangeFeed model checked three writers, commit-ordered numbering and prefix pruning. Its 632-state selected run and broken alternatives exposed the PostgreSQL commit-visibility premise that needed a real runtime test. The external-effect boundary remained infrastructure with a controlled-provider qualification plan.

**Result:** 18/18 named planned outcomes, 79 blocks and 15 models/351 configurations. The reported source-family coverage was later corrected by Codex; 18/18 did not establish whole-source-family preservation. [Quality review human decisions and repairs](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md:60).

### 5 Codex challenges Claude’s amended design

**Codex design and implementation; Oct 12, 09:52 to Oct 12, 10:22.** [Conversation source](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d).

**Your instruction:**

> we had another claude code session review the ontology and implementation plan and made the incremental update, and have a few review report like twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md. Please exam them and incremental update them if you are certain, do you have any other questions before we start the implementation?

**Procedure and outcome:**

Codex inspected Claude’s reports, current contracts, source-map claims and saved model evidence. Three scoped inspections covered Access/concurrency, Data/Work/evidence and source preservation. The follow-up recorded QR12–QR21: account growth outside the revocation set; recursion crossing inheritance and row-policy edges; admissions abandoned before execution; mandatory feed capture, hidden-page progress and deletion authority; search/query trust; incomplete scope mapping; and replay outcome semantics.

**Human decision within this turn:** Codex asked whether disabling a last administrator should require atomic replacement or leave operator-only recovery. Your answer was **“Require replacement administrator (Recommended)”**. Codex incorporated replacement into every affected workspace’s resulting administrator set.

**Result:** 86 concise blocks and corrected dispositions of 11 core, 38 foundation, 101 deferred and one omitted family. Static validation checked 15 models/351 saved configurations. No TLC rerun occurred in this turn: under the then-active review rule, the generic review request did not authorize another modeling phase. The plan carried targeted refresh obligations into implementation. [Independent follow-up](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/quality-review.md:130).

### 6 Claude explains the scoring and the contribution of TLC

**Claude Code; Oct 12, 09:57 to Oct 12, 09:59.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:904).

**Your instruction:**

> can you explain more on concretely how you measure these design quality dimensions, how did you get the better design and how did the TLA model help you in this process? Hypothetically without TLA, would you be able to de an equally good job?
> put your findings concisely in twenty-clean/docs

**Procedure and outcome:**

Claude wrote [design_review_method.md](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/design_review_method.md). It described each score’s inputs and five 0–2 criteria, separated measurements from judgment, and acknowledged that it chose the rubric during the review and scored its own repairs.

It attributed substantive modeling contributions to four of eleven findings: lease redundancy, gate protocol, crash progress and feed ordering. Most structural and coverage improvements came from design reasoning. Its “without TLC” conclusion was a counterfactual judgment: similar overall architecture, but weaker concurrency evidence and likely one missed progress issue. The reported 361 model lines/50 runs/40 seconds covered the review packet, including subsequently declined webhook work, not the whole downlift’s cost.

### 7 Codex implements and verifies the selected core

**Codex design and implementation; Oct 12, 10:28 to Oct 12, 13:15.** [Conversation source](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d).

**Your instruction:**

> Ok please follow the ontology and plan to implement the whole thing. Verify with comprehensive e2e test, and these TLA/TLC-based testings.

**Procedure and outcome:**

Codex continued the same plan, with work divided across model refresh, production kernel/owner implementation, and independent runtime infrastructure/application testing. It first pinned tools, verified real production functions, and qualified PostgreSQL locks, snapshots and rollback. The targeted formal refresh added 124 configurations, bringing the intermediate inventory to 20 models/475 configurations.

Implementation followed complete operations through HTTP, authorization, protected persistence and worker execution. A consequential amendment replaced the proposed SQL query compiler with complete bounded loading and typed Rust evaluation. This simplified semantic duplication but introduced the later-reviewed 256-record/object and 4,096-record/workspace ceiling.

Real tests found stale upload/restore receipt disclosure, a future-skewed command surviving database restore, nested Work-result disclosure, navigation cleanup bounds and CSV formula output. Codex repaired these, added discriminating regressions, amended the governing design and reconciled proof claims. Test-oracle mistakes and a test-ordering failure were retained separately from application defects. Later packets covered recovery, navigation, export and concrete CRM constraints.

**Result:** all 18 selected outcomes were reported implemented; 116/116 application/fixture cases, 45 owner SQL tests and two worker-pool tests passed; 99 Verus units had zero errors; 26 models/659 configurations matched expected outcomes. This completion claim was later qualified by independent implementation reviews. [Initial implementation report](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/implementation.md), [runtime diagnoses](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/runtime-env-e2e-followup-diagnosis.md).

### 8 A separate Codex chat publishes an implementation checkpoint

**Codex publishing; Oct 12, 10:37 to Oct 12, 10:40.** [Conversation source](codex://threads/01a1308b-8360-7c61-ab10-3e17c6141f19).

**Your instruction:**

> please commit everything in /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean and push to [https://github.com/G5Labs/twenty-crm-clean](https://github.com/G5Labs/twenty-crm-clean) main branch

**Procedure and outcome:**

This was repository publication, not a design or verification turn. Codex found the destination repository empty, added the remote, and published the current history. Other agents were still changing the shared checkout, so it captured follow-up files in additional commits. The resulting checkpoint was `5a1f064`, following `f3ee47b` and `0f138f3`. Its “clean working tree” statement described that moment, not completion of the still-running implementation.

### 9 Claude evaluates missing dimensions and the lifecycle

**Claude Code; Oct 12, 10:48 to Oct 12, 10:51.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:934).

**Your instruction:**

> 1. Are there any missing angles to these dimensions that we examined and improved? should adding them significantly improve our design quality?
> 2. Review against the design of twenty and twenty-clean, what are their score? how big of a value are we getting in this simplified design?
> 3. what do you think our overall mechanism of uplift existing codebase to ontology -> find their gaps and propose new minimal designs via TLA/TLC -> generate new ontology that preserves their core intent and feature -> design review and improvement via TLA/TLC -> code generate and formal verification. The harness on the uplift-downlift lifecycle of this repo.
> 4. "TLA earned its cost where state and interleavings change. It added nothing to the structural and coverage questions, which were most of this review." How often does state and interleavings change for your different designs, and how it helps with you reasoning? how can we improve on the part of "structural and coverage questions" not limit to TLA/TLC?

**Procedure and outcome:**

Claude compared the source product, initial target and revised target using its rubric; identified performance/scale, privacy/erasure, threat modeling, operational evolution and API usability as omitted angles; and assessed the lifecycle’s strengths and weaknesses. It suggested mechanism-removal tests, a frozen rubric, early proof experiments, structural checks and better model/runtime correspondence.

This turn returned analysis without changing files. Its source-versus-target scores mixed radically different scopes and remained subjective. The claim that seven of eleven findings did not need modeling described this review, not a general numerical division of solver value. The uplift-specific discussion is outside this study’s scope.

### 10 Claude saves the evaluation

**Claude Code; Oct 12, 11:40 to Oct 12, 11:41.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:1009).

**Your instruction:**

> ok let's put all your findings in a file in /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs

**Procedure and outcome:**

Claude saved the previous analysis as [design_evaluation.md](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/design_evaluation.md) and linked it from the README. It retained the missing-angle proposals and score caveats. This request authorized saving the findings, not implementing those proposals.

### 11 Claude expands the design review dimensions

**Claude Code; Oct 12, 12:50 to Oct 12, 12:51.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:1027).

**Your instruction:**

> did you include your analysis on the missing angles in the file? List all the new dimensions you would want to have for a comprehensive design review

**Procedure and outcome:**

Claude appended 16 dimensions to the original seven: intent fidelity, verification coverage, failure-mode completeness, performance/scalability, availability/recovery, security, privacy/data lifecycle, observability/operability, resource cost, evolvability, migration/compatibility, delivery feasibility, dependency risk, API usability, specification quality and review independence.

Each dimension gained concrete measures and a stated consequence for this design. Claude highlighted performance, privacy and evolvability as likely to change mechanisms and therefore worth examining before further proof investment. These became the basis of its later 23-dimension implementation review; they were not all adopted as mandatory product requirements.

### 12 Codex publishes the completed initial implementation

**Codex design and implementation; Oct 12, 13:36 to Oct 12, 13:41.** [Conversation source](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d).

**Your instruction:**

> let's commit and push everything in /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean, make sure to have a good gitignore

**Procedure and outcome:**

Codex inspected pending files and ignore rules, checked for local runtime artifacts and secrets, and expanded `.gitignore` for environments, databases, caches and build outputs. It deliberately preserved verification logs byte-for-byte because their hashes were evidence inputs.

**Result:** commit `2da7071` pushed to `origin/main`. This became the baseline for both implementation reviews.

### 13 Claude reviews the implemented system across the broader dimensions

**Claude Code; Oct 12, 13:38 to Oct 12, 13:52.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:1049).

**Your instruction:**

> now that the implementation in /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean is completed, judge the implemented design, where gaps are? and if you have incorprated the missing dimensions into the design, what would you change about this implementation? Give me a concrete doc explaining these at a high-level

**Procedure and outcome:**

Claude surveyed the implementation and delegated three read-only audits: concurrency/durability; data/policy/API; and assurance/tests/operations. It rechecked consequential claims and wrote [implementation_review.md](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_review.md) against baseline `2da7071`. It did not rerun tests, Verus or TLC.

The review recognized the transaction, snapshot, local-work and external-admission protocols, but identified the whole-workspace scale ceiling, worker/pool starvation, unpruned state, thin observability, schema evolution and unresolved administration/privacy behavior. It rated 23 dimensions and proposed changes ordered by their effect on mechanisms.

Some statements were too broad or incorrect. The later Codex reconciliation corrected its claims about unreviewed query drift, network I/O under a gate, trajectory-name coverage, proof scope and blanket encapsulation/disclosure strength. The original remains historical evidence, not the final adjudication of every finding.

### 14 A fresh Codex chat reviews implementation without fixing it

**Codex review and closure; Oct 12, 13:43 to Oct 12, 14:06.** [Conversation source](codex://threads/01a13135-ee45-7060-bcf1-1d7888dc0598).

**Your instruction:**

> We have generated codebase following the ontology twenty-clean/.g5/ontology and implementation plan twenty-clean/docs/implementation_plan.md, please do a review against the ontology for the implementation and just report all your findings, gaps in a file, do not fix anything. (There is another parallel claude code session doing the review as well). Both session are read-only on the code, so you shouldn't need to worry about it.

**Procedure and outcome:**

Codex read the governing ontology, selected design and plan, inspected production modules across all nine crates, and checked saved proof/E2E fingerprints. It then executed 16 targeted HTTP/PostgreSQL scenarios and two external-consumer compile probes against isolated fixtures.

**Result:** 15 findings plus four additional contract/coverage gaps. Examples included an unusable administrator counted by its flag; ordinary consumers obtaining any owner’s repository; mutable, reusable authority envelopes; key-secret replay after assignability changed; multi-principal personal navigation; OIDC freshness lost during a gate wait; post-dispatch failure mislabeled as known-aborted; and an issued cursor too large for its reader. The report distinguished public Rust capability defects from reproduced HTTP behavior.

Only the report was written. Production code, ontology and existing verification artifacts were unchanged; the full suite, Verus and TLC were not rerun. [Codex implementation review](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_review_codex_2026-10-12.md).

### 15 Codex reconciles the two implementation reviews

**Codex review and closure; Oct 12, 14:15 to Oct 12, 14:39.** [Conversation source](codex://threads/01a13135-ee45-7060-bcf1-1d7888dc0598).

**Your instruction:**

> please read twenty-clean/docs/implementation_review.md and incorporate both findings to get a complete understanding of what gaps to close and how to close them to follow the ontology

**Procedure and outcome:**

Codex compared both reports against actual governing choices and re-inspected disputed claims before proposing repairs. It preserved the original findings and separated confirmed defects, scope questions, operational qualification and optional extensions.

This mattered because some Claude recommendations would change existing intent: replacement upsert was selected behavior; bounded Rust evaluation was already an explicit amendment; work budgets counted committed admissions, not failed scans; unknown commit was already part of the outcome contract. Conversely, Codex’s reproduced disclosure and capability defects qualified Claude’s favorable encapsulation assessment.

**Result:** C01–C19 closure items, six decision areas D01–D06, eight optional expansion areas E01–E08, and five ordered repair slices in the existing plan. No production or ontology change occurred yet. [Reconciliation](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_review.md:210).

### 16 Codex closes the reconciled implementation gaps

**Codex review and closure; Oct 12, 14:39 to Oct 12, 16:28.** [Conversation source](codex://threads/01a13135-ee45-7060-bcf1-1d7888dc0598).

**Your instruction:**

> ok please close all these gaps

**Procedure and outcome:**

Codex continued the existing downlift plan and amended the owning contracts for selected policy refinements. It closed authority/disclosure first, then complete data commands, work/recovery, retention/resource bounds and deployment checks. Fixed-owner repositories and immutable transaction-bound authority closed the ordinary extension bypasses. Other repairs covered effective administrator authority, complete replay/preview checks, protected OIDC freshness, personal principal composition, usable cursors, work ownership, fault isolation, pruning and explicit administration/relationship policies.

Regression and boundary tests preserved useful allowed behavior. A concurrent build initially replaced the worker binary with one lacking test hooks; Codex diagnosed this as a test-environment failure, then serialized checks and froze the binaries. A late check found lifecycle helpers still accepting caller-provided premises; those were restricted to composition-held maintenance authority before final verification.

**Result:** C01–C19 closed within the selected scope; all 22 gates passed, including 135 HTTP cases, 46 owner SQL/boundary cases, two worker tests, 20 compile probes, 99/0 Verus and five optimized capacity measurements. No new TLC phase was requested or executed. Larger-scale storage, migrations, erasure and key rotation remained explicit extensions at this milestone. [Closure report](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_gap_closure.md), [first closure evidence](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/review-closure/README.md).

### 17 Codex publishes the first closure

**Codex review and closure; Oct 12, 17:04 to Oct 12, 17:07.** [Conversation source](codex://threads/01a13135-ee45-7060-bcf1-1d7888dc0598).

**Your instruction:**

> commit everything and push

**Procedure and outcome:**

Codex checked that the 22-gate results still matched the Rust sources, committed the pending target changes and pushed `52a2490` to `origin/codex/rust-core-design`. Its source archive included the amended design from the sibling repository. Claude used this commit as the next review baseline.

### 18 Claude reviews the closure and revises the scale design

**Claude Code; Oct 12, 17:06 to Oct 12, 18:21.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:1231).

**Your instruction:**

> a codex session has finished to close the gaps according to you review and /Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_gap_closure.md
>
> please review the current code base, close any gaps, fix any design issues you have spotted. update the ontology and rerun TLA/TLC models if needed.

**Procedure and outcome:**

Claude took `52a2490` as the baseline, ran the existing 22-gate checks, and delegated a read-only audit of the closure changes. It recorded A1–A13, including anonymous ingress/login exhaustion, incomplete replacement-restore checks, delegated role changes, stale recovery fences, unreadable work rows, deactivated values, probes, timeouts and maintenance cost.

The agent interpreted the broader request to fix design issues as authority to address four previously deferred areas: E01 scale, E02 migrations, E03 record-history erasure and E06 key rotation. It modeled demand-loaded working sets, scan pagination and incremental uniqueness, repaired model syntax and an initially non-discriminating progress control, then implemented the selected mechanisms.

Implementation added complete-on-demand fact loading, filter-free ordered SQL enumeration with per-row policy evaluation, key-anchored encrypted cursors, search/unique indexes, gated quotas, versioned/checksummed migrations, key IDs with retained-key decoding, stateless one-use login and destroyed-record erasure. Tests exposed a PostgreSQL counter-decrement error, missing per-record share bounds and excessive statements per query page. Reducing the page to at most two statements improved the measured 100,000-record filtered traversal from 64 seconds to 8.8 seconds.

**Recorded order:** new scale models preceded the main replacement, but production edits began before all owning ontology/design amendments were written. The final ontology/design reconciliation occurred later in the same turn. This did not consistently follow the repository’s requirement to amend consequential contracts before dependent code.

**Result:** all 23 gates passed: 142 HTTP cases, 49 owner/boundary/scale SQL cases, two worker tests and unchanged 99/0 kernel proofs. The scale packet executed 28 configurations across three new models, making the retained inventory 29 models/687 configurations and 107 ontology blocks. A release benchmark wrote 100,000 records through HTTP; the one-million-record limit is a configured bound, not measured capacity. The changes were left uncommitted. [Second closure evidence](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/scale-closure/README.md), [scale models](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/TLA_verification/rust-core/scale/README.md).

### 19 Claude prepares ontology refinement and the next Verus handoff

**Claude Code; Oct 12, 18:58; ongoing at initial cutoff.** [Conversation source](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl:3297).

**Your instruction:**

> great!
>
> 1. please make sure the ontology is incrementally updated to reflect what we did for TLA/TLC. Keep the ontology block concise with 1-2 sentence, you should have children blocks to describe the more formal specification.
> 2. please summary what you have done for the review and the fix in this session in a file and
> 3. In that file, include instructions for next session to do a Verus verification of the codebase based on the instructions in /Users/ziniuw/Desktop/projects/bank-of-ziniu/update_new_workflow.md + a concise prompt (don't put into the file) so that I can kick off the next session

**Procedure and outcome:**

Claude inspected the current proof setup and changed ontology blocks, split longer commitments into concise parents and formal children, and reported successful static validation of 113 blocks at 19:02. It then began preparing the session summary and Verus handoff against the named workflow. At this cutoff the final handoff was still being written. This was an ontology/document refinement, not a new executed Verus campaign or a fresh full TLC run; the preceding milestone’s 107-block count remains historical.

## Verification milestones

These results belong to different revisions and scopes. Expected broken controls and reachability witnesses are not production defects, and counts across layers must not be added into a verification percentage.

| Milestone | Models and checks | Runtime and proof evidence | Interpretation |
|---|---|---|---|
| Initial Codex design | 13 models, 320 configurations; 63 blocks | No production implementation at that milestone | Selected design and its explicit premises, with source dispositions and implementation handoff. |
| Claude design review after webhook removal | 15 models, 351 configurations; 79 blocks | No Rust or Verus execution in that design review | Includes mechanism comparisons and a core-outcome checklist; subjective scores and incomplete amended-model coverage remained. |
| Codex pre-implementation follow-up | Same 351 saved configurations; 86 blocks | Static evidence validation only | Ten new corrections required targeted refreshes; saved passes were not relabeled as new checks. |
| Initial implementation | 26 models, 659 configurations; 87 blocks | 116 HTTP/fixture cases, 45 owner SQL tests, two pool tests, Verus 99/0 | Executed selected core, later challenged by independent implementation reviews. |
| Codex C01–C19 closure | Existing 26/659 retained; no fresh TLC phase | 22 gates, 135 HTTP cases, 46 owner SQL/boundary cases, two pool tests, 20 compile probes, Verus 99/0 | Current-scope repair plus explicit extensions and operational limits. |
| Claude second closure | Three new scale models, 28 fresh configurations; total inventory 29/687 and 107 blocks | 23 gates, 142 HTTP cases, 49 owner/boundary/scale SQL cases, two pool tests, Verus 99/0; 100,000-record release benchmark | Revised scale and lifecycle mechanisms with their own finite checks and runtime evidence. Earlier result sets retain their original scope. |

The **final new TLC campaign** was the [scale packet](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/TLA_verification/rust-core/scale/README.md):

- **WorkingSet:** decisions wait for complete demanded facts from one protected snapshot. Treating an unloaded fact as absent can disclose a hidden record; mixing snapshots breaks the decision contract.
- **ScanPagination:** a cursor advances from the last examined row, including hidden rows. Anchoring only on returned rows can prevent progress. Large-key rehydration must detect a moved or deleted anchor and require restart.
- **IncrementalUnique:** index-holder and in-batch checks agree with the bounded full-result reference. Missing index entries or skipped batch pairs break the equivalence.

Its 28 cases comprised six selected safety passes, five conditional progress passes, seven broken controls and ten witnesses. The 687 total is an inventory of accumulated saved configurations, including earlier alternatives, **not evidence that all 687 were freshly rerun after the final changes**. The general gate runner validates saved formal inputs; packet runners execute TLC.

At the 18:59 reading, all 42 source hashes in the final gate receipt, all 77 inputs in its HTTP receipt, and all seven kernel sources in the proof receipt matched the local files. This retrospective checked those bindings; it did not rerun the campaigns. [Final gate receipt](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/scale-closure/gates/results.json), [HTTP receipt](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/scale-closure/gates/e2e.json).

## What the process establishes and what it does not

**Minimality came from asking what each mechanism was needed for.** Removing local claims and comparing the remaining transaction rule was more informative than another unmodified safety run. The 147-to-59 and 47,176-to-28,350 state comparisons apply to their own matched finite bounds; they do not measure whole-system complexity or prove a globally minimal architecture.

**The useful core and whole-product coverage stayed different questions.** The corrected source map records 11 complete retained families, 38 narrower foundations, 101 deferred families and one omitted mechanism, across 151 families/463 blocks. The 18-outcome checklist describes the chosen core. Later operational additions do not silently turn those deferred products into delivered features. [Source map](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/source-intent-map.md).

**The operating envelope changed the meaning of sufficient simplicity.** Complete bounded loading removed a second query-semantics implementation and made early proofs/tests tractable. The later agent-selected scale goal justified demand loading, indexes, counters and migrations. That is a tradeoff: fewer coordination protocols remained valuable, but the larger scale needed additional state with explicit consistency obligations. The one-million-record quota is not a demonstrated million-record workload. Arbitrary field ordering still sorts object data, and the reported 100,000-record benchmark is local evidence, not a production SLO.

**Review reconciliation was necessary.** Claude supplied broad operational and lifecycle criticism; Codex supplied concrete contract counterexamples and public-consumer probes. Some broad praise and criticism from the original Claude report did not survive reinspection. Preserving both reports with an explicit reconciliation was more reliable than treating their union as an automatically correct repair list.

**Proof scope stayed limited despite repeated 99/0 results.** The production kernel has real bounded-policy and transition proofs, but loading, recursive row-policy assembly, SQL/index completeness, serialization, cryptography and orchestration retain independent obligations. The scale rewrite preserved the kernel while changing those adapters; unchanged kernel results do not prove the new adapters. The next requested Verus handoff should start from this distinction.

**Workflow execution was not uniformly ontology-first.** The history includes an agent-added webhook later rejected by the human, subjective scores chosen during the review, and the final Claude turn’s contract/document reconciliation after dependent code edits had begun. These are procedural observations, separate from the final passing tests. The current harness’s newer rules must not be retroactively described as the exact rules followed by every earlier turn.

The completed local work still leaves full source-product parity, Twenty data/API migration, whole-program proof, hosted CI execution, actual provider/TLS qualification, physical/cross-version recovery and mixed-version rollout outside the demonstrated result. Record-history erasure does not cover account data, backups or all in-window receipts. These are scoped limits rather than failures hidden by the final count.

## Evidence and conversation provenance

| Record | Role in this study |
|---|---|
| [Uplift codebase into ontology (2)](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d) and [local transcript](/Users/ziniuw/.codex/sessions/2026/10/11/rollout-2026-10-11T19-23-41-01a12d47-7ab3-79e0-a9aa-5981625f1b4d.jsonl) | New-core design, pre-implementation reconciliation, human administrator decision, implementation and publication. Earlier uplift turns excluded. |
| [Rust core design review and ontology](/Users/ziniuw/.claude/projects/-Users-ziniuw-Desktop-projects-G5-benchmark-twenty/75079e62-8765-4c6f-ad19-91098335f42d.jsonl) | Design scoring, human scope correction, broader evaluation, implementation review, second closure and the later handoff request. |
| [Review implementation against G5](codex://threads/01a13135-ee45-7060-bcf1-1d7888dc0598) and [local transcript](/Users/ziniuw/.codex/sessions/2026/10/12/rollout-2026-10-12T13-43-00-01a13135-ee45-7060-bcf1-1d7888dc0598.jsonl) | Independent read-only review, cross-review reconciliation, C01–C19 repair and publication. |
| [Commit and push all changes](codex://threads/01a1308b-8360-7c61-ab10-3e17c6141f19) | Auxiliary publication during implementation; no independent design or test result. |
| [Downlift plan](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/downlift-plan.md), [implementation plan](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/docs/implementation_plan.md), [saved design](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty/docs/rust_core_design.md) | Continuing governing documents, with successive amendments rather than a replacement plan per review. |
| [PairSoft lifecycle study](pairsoft_lifecycle_study.md) | Earlier lifecycle context; its numerical results are not added to Twenty’s evidence. |

Repository milestones were `19bb194` (target design), `5a1f064` (publication checkpoint), `2da7071` (initial implementation), and `52a2490` (Codex closure). At the 18:59 reading, HEAD remained `52a2490` and Claude’s second closure was uncommitted. Current working-tree documents contain historical sections and later addenda; their milestone scope, rather than the newest file timestamp, determines which claims apply.

## Proposed ontology organization and harness lessons — 13 October 2026

Historical proposal: the [applied disposition](#applied-after-the-humans-guardrail-constraint-2026-10-13) below narrows this edit set and supersedes its harness closure recommendations.

This is a concrete proposal following the human's acceptance of the six concept improvements discussed in this session, and the request for reviewer-oriented areas, ontology edits and harness lessons. It continues this study; it does not apply the proposed reorganization to Twenty Clean or change its implementation. The earlier chronology keeps its original cutoff. The proposals also use the later [Verus retrofit evidence](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/verus-retrofit/README.md), [decision inventory](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/evidence/verus-retrofit/decision-inventory.md) and [boundary contract](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/verification/boundary/CONTRACT.md), including their subsequent track A amendments; those documents do not all describe the same verification milestone.

### Organize authoritative meaning, then supply review paths

Each statement has one semantic home. A reviewer follows links across homes when a concern overlaps: security reviewers need authority invariants, policy, cryptographic lifecycle and disclosure contracts; a performance reviewer also needs the correctness conditions of indexes and working sets. A directory category neither creates a new runtime owner nor requires a separate review process.

Create a dedicated node when several consequential shared commitments form a useful review unit. Keep a small area as a section or linked reading path; leave an irrelevant dimension absent. Do not force one file per evaluation dimension, invent requirements to populate files, or duplicate rules for different personas. Readability and coherent ownership determine a split, not a fixed block count.

Recommended layout under `twenty-clean/.g5/ontology/concepts/`:

| Node | Owns | Principal review question |
|---|---|---|
| `system_invariants.g5.md` — retain and extend | Properties every applicable owner must preserve: confinement, complete authority, atomicity, freshness, identity, uncertainty, complete facts, derived-state consistency and outstanding obligations. | Can any permitted transition, failure or competing writer violate the guarantee? |
| `system_design_principles.g5.md` — retain and narrow | Necessary state and protocols, owner separation, gates, dependency direction, extension boundaries, derivation and equivalent optimization. | Does the selected architecture meet the required outcomes with sufficient simplicity and usable extension boundaries? |
| `coding_principles.g5.md` — new | Pure production decisions, exact loader contracts, actual kernel inputs, verified write plans and faithful execution. | Does production code establish the proved function's preconditions and apply its result, through every supported path? |
| `security_policies.g5.md` — new | Shared access-policy composition, explicit principal modes, transaction-bound authority and narrowly assigned privileged capabilities. | Who can authorize each operation, through what scope and evidence, including indirect disclosure and delegated access? |
| `operation_deployment.g5.md` — new | Recovery admission, fences, schema/version compatibility, key retention, readiness, shutdown, diagnostics and deployment clock premises. | Can an operator restore, upgrade and diagnose the application without violating its contracts? |
| `performance_requirements.g5.md` — new | Supported capacity and resource bounds, overload/refusal behavior and the distinction between admission limits and qualified workload performance. | What workload is supported, what bounds it, and what happens when a bound is exceeded? |
| `verification_contracts.g5.md` — new | Scope of assurance claims, correspondence to intent and production, explicit premises, and evidence binding. | What precisely is established, by which evidence, on what assumptions and revision? |
| `software_architecture.md` — retain | Owner/module map and navigation to module responsibilities. | Where is each rule enforced and which dependencies cross the boundary? |
| `core_user_stories.g5.md` — retain | Navigation to feature stories and their acceptance/refusal/recovery contracts. | Does the smaller system still provide the intended useful outcomes? |

The additional area justified now is **verification contracts**: the differences between model properties, production specifications, adapters, SQL statements and deployment premises have repeatedly changed the assurance conclusions. **Privacy/data lifecycle**, **API compatibility/evolution**, **dependency risk** and **observability** are useful review paths now, but need no additional files yet. Link privacy to Data erasure/history, Work receipt retention, Host keys and recovery; compatibility to source dispositions, Host upgrades and command outcomes; dependency risk to the existing qualification and trusted-boundary evidence; observability to the operations node. Split any of these later if its shared commitments become substantial. No latency SLO, RPO/RTO, deployment cost target, account-erasure promise or legacy API compatibility should be invented from existing tests or configured limits.

### Concrete moves and splits

Preserve existing stable IDs even when their historical prefixes no longer match the destination filename. Move each block's metadata with it; retain its lineage and scoped evidence. Replace the old contract with a reading link, not another block carrying the same rule. The filenames below are proposed destinations, not files already created.

| Current block or family | Proposed change |
|---|---|
| Existing six `clean.invariant.*` blocks | Keep in `system_invariants.g5.md`; add the three shared invariants below and a frame expansion of atomicity. |
| `clean.design.owners`, `clean.design.gates` and its children, `clean.design.dependencies`, `clean.design.extension`, `clean.design.derived`, `clean.design.outcomes` | Keep in `system_design_principles.g5.md`. Add minimality and optimization; link coding enforcement and resource bounds. |
| `clean.design.kernel` and `.formal.disclosure`, `.formal.boundary`, `.formal.writes` | Move the complete family to `coding_principles.g5.md`, preserving `Expands` relationships. Extract shared invariant claims from the children and add the precise loader/input clauses below. The design and architecture pages link this family. |
| `clean.access.policy` and `.formal.defaults`; `clean.access.modes`; `clean.access.scope` | Move shared policy to `security_policies.g5.md`. Split the bundled scope block: retain its ID for immutable ordinary authority, add a formal child for privileged capability holders, and move its personal-navigation rule into Views. Credential administration, invitation, sharing and private-record feature outcomes stay in Access and link these policies. |
| `clean.host.upgrade` with `.formal.migrations` and `.formal.keys`; `clean.host.portability` with `.formal.fence`; `clean.host.operations` with `.formal.ingress` and `.formal.rates` | Move to `operation_deployment.g5.md`. Split the operations parent as below: resource limits go to performance, diagnostics get a child, and recovery refers to its existing owner. Keep readiness/shutdown under the original operations ID. Host's feature story links these operational contracts. |
| `clean.design.bounds`; `clean.data.capacity`; `clean.data.capacity.formal.bounds` | Move to `performance_requirements.g5.md`. Separate the capacity promise from the loading algorithm. Extract the shared counter-equality rule into the derived-state invariant; retain quota admission and the 50,000-record scan bound in the capacity child. |
| `clean.data.capacity.formal.working-set` | Move to `coding_principles.g5.md`, retaining its ID but changing its `Expands` target to `clean.design.kernel`. Keep the 32-round algorithm and snapshot correspondence here; reference the complete-facts invariant. |
| `clean.product.assurance` | Move from the root to `verification_contracts.g5.md`, with the scoped formal children below. Root retains a prominent assurance link. |

Feature-specific limits remain with their behavioral contracts: credential eviction and quota fairness, per-record share bounds, search breadth refusal, query-page continuation and feed retention are not copied into a giant capacity table. The performance node links those owners. Likewise, confidentiality rules governing a replay or a deleted-record feed remain with that operation and link the shared security policy.

### Proposed invariant and design text

Each paragraph below is the proposed body of one block. New IDs are suggestions; implementation should check for concurrent additions before allocating them. Parent commitments remain short; formal details, assumptions and evidence have their own homes.

**Add `clean.invariant.complete-facts` — Decide on sufficient protected facts.**

> A decision uses every fact on which its answer depends from its protected transaction or snapshot; an unloaded fact is distinct from an absent fact. If those facts cannot be obtained within supported bounds, refuse explicitly rather than decide from incomplete input.

This permits short-circuiting when the known facts already determine the answer; it does not require loading unrelated facts. Promote the common meaning from `clean.design.kernel.formal.disclosure` and `clean.data.capacity.formal.working-set`; keep their disclosure and loading mechanisms as specializations.

**Add `clean.invariant.derived-state` — Keep materializations faithful.**

> At every committed boundary, maintained indexes, reference sets and counters equal their owners' declared derivations from authoritative records. Every mutation, migration, repair and cleanup path preserves that correspondence atomically.

The concrete unique-key, live-reference, search-token and counter derivations are already stated in [boundary contract §4](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/verification/boundary/CONTRACT.md:66). Extract them into these formal owners and have that boundary document reference them:

| Proposed formal child and home | Governing definition |
|---|---|
| `clean.data.records.formal.unique-index`, Data, expands `clean.data.records` | The unique index contains exactly each stored record's complete, non-null unique-group values under its current object definition, including retained trash. |
| `clean.data.records.formal.reference-index`, Data, expands `clean.data.records` | The reference index contains exactly the source–target pairs represented by reference values of untrashed records. |
| `clean.data.search.formal.index`, Data, expands `clean.data.search` | The search index contains exactly the declared normalized and truncated tokens of each stored record's active searchable text fields, associated with that record and its object. Candidate admission and permitted-field matching remain governed by the existing search contract. |
| `clean.invariant.derived-state.formal.counts`, invariants, expands `clean.invariant.derived-state` | The Data record counter equals its stored record-row count, and the Access share counter equals its stored share-row count, within each workspace. Their owner updates them in the same protected transaction as the counted rows. |

Use the current token derivation and encoding specification as the search child's formal reference, preserving its exact semantics and trusted normalization premises. The existing feature-owned feed-order invariant remains authoritative; it is not copied into this family.

**Add `clean.invariant.obligations` — Preserve outstanding domain obligations.**

> Retirement, pruning, exhausted execution admissions and transport settlement cannot silently discharge an unresolved domain obligation. Its owner records completion, explicit transfer or authorized abandonment; abandonment never claims successful business completion.

Consolidate the shared rule from `clean.host.retirement`, `clean.host.extensions`, `clean.work.claim.formal.attempts`, `clean.work.progress`, `clean.work.ownership` and maintenance. Each retains its distinct trigger, authority, recovery or disposition semantics. Conditional eventual progress stays in `clean.work.progress`; it is not a state invariant.

**Add `clean.invariant.atomic.formal.frame` under `clean.invariant.atomic` — Confine committed effects.**

> A committed local command changes exactly its decided write set and required durable consequences, leaving unrelated state unchanged. A known refusal or rollback publishes none of those local changes; an unknown commit is reported under the uncertainty contract.

This makes the frame obligation explicit without extending SQL atomicity to irreversible external effects or asserting that an ambiguous commit rolled back.

**Add `clean.design.minimality` — Justify independent mechanisms.**

> Retain an independent authority, durable protocol or coordination mechanism only when it discharges a required obligation that existing mechanisms cannot discharge. Compare simplifications under the same required outcomes, operating bounds and failure assumptions.

Lineage: the original human design instruction, lease-removal comparisons and gate tradeoffs. This does not require TLC for every future design decision. Its evidence can be a necessity argument, a counterexample or an appropriate executed check.

**Add `clean.design.optimization` — Preserve the reference contract.**

> An optimized mechanism preserves its reference contract within the declared scope, including result completeness, ordering and refusal behavior. Candidate reduction, incremental indexes and pagination state the correspondence conditions on which that equivalence depends.

Lineage: WorkingSet, IncrementalUnique and ScanPagination, followed by the production `workingset`, `unique`, `scan` and `sortkey` specifications. An intentional change to scope or a capacity bound remains an amendment, not an equivalence claim.

**Refine `clean.design.derived`.**

> Derive inherited access and work readiness from authoritative facts; retain durable identities for independently observable outcomes and recovery. Retain materialized indexes or counters when the operating envelope requires them, under the derived-state invariant.

This preserves the original preference for derivation while acknowledging the scale work's justified materializations.

### Proposed coding, security and operational text

**Narrow `clean.design.kernel.formal.boundary` after moving it.**

> Adapters load facts, call the production kernel function governing the operation and apply its returned decision. They do not substitute a second authorization or transition algorithm; remaining trusted behavior follows the verification-premise contract.

Its current paragraph also owns loader exactness, gate discipline, materialization consistency and commit-state correspondence. Extract those independent commitments: exactness into the following child, derived-state consistency into its invariant, synchronization into the existing freshness/gate contracts, and proof assumptions into verification contracts.

**Add `clean.design.kernel.formal.loaders` — Load the contract's facts.**

> A loader returns exactly the rows and ordering its decision contract requires from the protected view; candidate loaders may return a declared complete superset for subsequent evaluation. No truncation or omitted row may masquerade as absence or a complete result.

**Add `clean.design.kernel.formal.inputs` — Establish real proof inputs.**

> Production calls pass the actual decision facts to the verified function. A fixed input representing an already-established condition names the type guarantee or preceding check that establishes that same condition.

These children expand `clean.design.kernel`. The existing disclosure child retains the requirement that returned rows and fields pass the production evaluator and references complete facts, the loader contract and feature-owned query semantics. The working-set child keeps its finite retry bound and equality with the decision over the whole snapshot.

**Refine `clean.design.kernel.formal.writes` — Keep the proof's scope visible.**

> Data's planned records, derived rows, content, counters and change facts change only through kernel-built plans and one store executor; other write paths refuse, and dependent deletes are explicit rather than foreign-key cascades. Under the plan's stated preconditions and faithful execution of its statements, each step preserves storage constraints and the final state contains exactly the decided effects.

Reference the frame and derived-state invariants. The actual plan preconditions, SQL mapping, constraint model and remaining runtime premises stay explicit in the linked formal specification and boundary contract; this wording does not promise that operational database failures cannot occur.

**Split `clean.access.scope` in the security node.**

Keep its ID for:

> Access alone constructs immutable ordinary authority bound to its originating protected transaction or snapshot. Policy-validation facts convey no operational authority.

Add `clean.access.scope.formal.capabilities`, expanding it:

> Trusted Host holds scoped maintenance authority; Data receives only its narrow record-share retirement capability. Neither capability provides a generic privileged data path.

Move the distinct existing navigation rule to `features/views.g5.md` as `clean.views.principal`:

> Personal navigation requires one explicit human membership shared by every required human principal; incompatible human selections refuse.

Keep `clean.access.policy`, its defaults and `clean.access.modes` otherwise semantically unchanged. The security node links the invariant authority rule, credentials and sharing, replay reauthorization, historical disclosure, sealed cursors/login state/receipts, key rotation and ingress proxy trust. Their operation-specific guarantees keep their current owners.

**Narrow `clean.host.operations` in the operations node.**

> Host exposes readiness for serving ordinary requests, process liveness independent of the database, and graceful shutdown of API and worker processes. Admission follows the declared resource envelope and recovery barrier.

Add `clean.host.operations.formal.diagnostics`, expanding it:

> Correlate operational failures and command outcomes for diagnosis while redacting credentials and sensitive payloads. Diagnostics distinguish refusal, known rollback and uncertain commitment.

Keep the existing ingress and rate children. Move the parent’s repeated recovery clauses to references to `clean.host.portability` and `.formal.fence`, and its capacity/timeout premises to the performance node. Preserve all selected values and configuration exceptions in their owning contracts or a linked formal resource profile.

**Add `clean.ops.clock` in the operations node — State the deployment time premise.**

> Deployment supplies a trusted clock within the supported pairwise fleet-skew bound; credential admission, command windows and retention rely on that bound. Recovery and pruning preserve their safety margins before old identities become eligible for removal.

The current deployment contract selects a 30-second pairwise skew margin. Preserve that value in this block's formal expansion and make the existing receipt/credential retention rules reference it. This is a recorded external premise, not a property established by kernel verification.

**Add `clean.performance.resources` in the performance node — Bound shared resource use.**

> Host bounds ingress, database occupancy, lock and statement duration, and workspace/account fan-out so admitted work preserves the declared operating envelope. Resource exhaustion produces explicit bounded failure under the command-outcome contract.

Move the already-selected shared resource premises from `clean.host.operations` and the deployment envelope into its formal profile: 128 workspaces and 16 account attachments by default, their declared operator-adjustable ceilings, pool reserve and per-process connection bounds, and protected lock/statement timeouts. The feature-specific caps remain linked, not repeated. Preserve cancellation and unknown-commit distinctions; a timeout alone never proves rollback.

**Add `clean.performance.qualification` — Separate capacity from workload performance.**

> Configured admission limits define supported input bounds; workload performance is claimed only for the measured deployment, concurrency and operation mix. A larger capacity limit or successful local benchmark alone establishes no deployment latency or throughput guarantee.

The existing one-million-record admission bound stays in `clean.data.capacity`; the 100,000-record measurements remain evidence. The proposal sets no new SLO and does not silently turn a configured quota into measured capacity.

### Verification contracts and metadata reconciliation

Move `clean.product.assurance` unchanged, retaining its separation of bounded design checks, production proofs, runtime tests and provider qualification. Add three formal children in the same new node:

| Proposed child | Proposed body |
|---|---|
| `clean.product.assurance.formal.specification` | A proof specification identifies its governing intent and the behavior it establishes; a weaker postcondition cannot be reported as the whole contract. Required refusal, frame and progress conditions remain explicit in their owners. |
| `clean.product.assurance.formal.premises` | Each verification claim names its scope and material trusted premises, including the production path that establishes each premise. A proved kernel function alone does not prove its loaders, encodings, executor or external environment. |
| `clean.product.assurance.formal.binding` | Evidence used to claim current verification is bound to the relevant source, specification, schema, configuration and external proof dependencies. When those inputs change, reassess the affected claim before presenting the earlier result as current. |

Counts, commands, logs and complete premise inventories stay in evidence and the boundary contract. These ontology children govern the meaning of the claims, not a mandatory report format or a requirement to rerun every unrelated test.

Reconcile the existing metadata as part of the moves. In particular, `clean.design.kernel.formal.disclosure` still says working-set miss recording, filter connectives, grouping and insertion sort remain adapter code, whereas the later decision inventory records their kernel moves. The boundary metadata distinguishes proofs on a saved pgspec snapshot from a current receipt; preserve that distinction. Do not copy a parent's broad `implemented`/`100%` label or one proof result to every extracted child without checking the narrower scope. Pure moves preserve historical evidence; new or changed guarantees get only the evidence that actually establishes them.

### Root navigation, implementation scope and checks for the proposal

Replace the root's single shared-meaning line with a small reviewer navigation table linking the nine nodes above. Each new node opens with its scope, runtime enforcers and a short list of related contract owners. Include privacy and compatibility reading paths in this navigation without copying their rules. Retain the product intentions and feature stories in the root and feature pages.

When applying this proposal, continue the target's existing downlift plan with this edit set; record the human organization instruction separately from the proposed block wording. Preserve IDs, move their footers, repair links relative to their new files, update `Expands` by meaning, and retarget ID-to-file/evidence mappings such as `ontology-assessment.json` and source dispositions where affected. Historical evidence snapshots stay historical. Update the saved design's ontology navigation and reconcile any newly explicit governing meaning. No production implementation, new TLC campaign, automatic evidence promotion or harness mutation is included in this proposal.

Proportionate checks are unique IDs, resolving links and expansions, one authoritative home per meaning, no loss of exceptions or quantified bounds, and consistent claim scope. Use the existing static ontology/source-map validators when applying the reorganization. Any newly exposed implementation discrepancy remains a named gap for its owner; do not claim it fixed through moving text.

### What the harness should learn

The current harness already instructs agents to preserve consequential decisions, use one home per meaning, evaluate broad dimensions for downlift, refine ontology after model findings and verify adapter premises. The demonstrated gap is in **extracting and placing the lasting shared contract**: a lesson can remain in a review paragraph, a feature-specific clause or a proof premise even though it should govern several owners.

| Observed lesson | Harness consequence |
|---|---|
| Lease removal and the later scale rewrite both improved the design under different requirements. | Capture why a mechanism is necessary or removable, with the governing operating envelope. Neither deletion nor lower state counts alone establish minimality. |
| WorkingSet, uniqueness and pagination checks exposed assumptions that several consumers rely on. | Promote a shared guarantee or premise to its semantic owner; consumers retain only their distinct conditions and mechanisms. |
| Kernel proofs coexisted with unused helpers, fixed inputs and unproved adapter behavior. | Record caller preconditions and faithful application as contracts; inspect actual production callers and competing writers when claiming correspondence. |
| Review found correct code with weak specifications, and successful proofs with incomplete product coverage. | Reconcile intent, specification and implementation separately. A stronger proof cannot repair omitted intent without a contract amendment. |
| Subsequent kernel moves left older metadata describing the previous adapter boundary. | Updating evidence includes retiring stale descriptions and limiting old results to their original inputs and scope. |
| Important operational choices were bundled into Host or remained in a runbook. | Capture material clock, recovery, version, capacity and trust assumptions in reviewer-discoverable ontology; leave routine commands in operational documentation. |
| Different reviewers discovered different classes of gaps. | Provide cross-linked review areas and concrete ownership without requiring every reviewer, dimension or scorecard on every task. |

Three concise harness edits would address these failures; duplicating the same checklist in every skill would add little:

1. **[Authoring structure](/Users/ziniuw/Desktop/research/G5-Lean-V2/.agents/skills/ontology-authoring/references/structure.md):** add the placement and sparse-area rule:

   > Organize shared commitments into useful review areas such as invariants, design, coding, security, operations, performance and verification. Create a dedicated node when the area has several consequential commitments; otherwise use an existing section or linked reading path, and omit empty areas. Each meaning has one authoritative home; persona navigation references that home.

   The authoring skill's existing sentence directing every “design principle” to `system_design_principles.g5.md` should then be narrowed to architectural design principles. Implementation, security, operational and assurance commitments use their semantic nodes when those exist. This avoids a conflict between the new organization and the current catch-all instruction.

2. **[Planning](/Users/ziniuw/Desktop/research/G5-Lean-V2/.agents/skills/ontology-planning/SKILL.md), adjacent to contract amendment:** add a result-to-contract extraction rule that also applies without formal tools:

   > When investigation changes a relied-on guarantee, ownership rule, operating limit or assumption, record the lasting commitment in its semantic owner before dependent work. Consolidate shared meaning and reference it from affected consumers; keep the particular counterexample, measurement or proof result as linked evidence.

   This extends the existing model-refinement behavior to structural reasoning, runtime discoveries and proof-boundary findings. The sixteen evaluation dimensions remain questions, not a prescribed ontology schema.

3. **[Downlift verification](/Users/ziniuw/Desktop/research/G5-Lean-V2/.agents/skills/ontology-downlift/SKILL.md), extending its existing adapter-premise paragraph:** add the closure rule:

   > Reconcile newly exposed shared invariants and proof premises into ontology, including how production callers and every relevant writer establish them. Refresh affected metadata when that boundary changes; keep conditional, historical and unverified claims explicit.

These are proposed edits only. They preserve the current workflow's proportionate verification, optional incremental TLC, stable IDs and assumption of capable agents. The system-specific resulting contracts belong in Twenty's ontology; the general duty to discover, consolidate and maintain those contracts belongs in the harness.

### Applied after the human's guardrail constraint, 2026-10-13

This disposition supersedes the broader proposal above. The human required checking each rule against current code and enduring intent, retaining only consequential meaning that code alone does not adequately explain, and explicitly rejected adding verification closure to the harness.

The [target ontology](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/ontology/root.g5.md) adds seven compact commitments: distinguish missing facts from absence; maintain decision-index correspondence; preserve unresolved domain obligations; justify independent mechanisms; preserve contracts when optimizing; establish actual kernel inputs; and preserve the external clock premise. Seventeen existing blocks move with stable IDs into coding, security, operations and performance nodes. Existing design and invariant nodes remain; feature-specific limits retain their owners. No new verification node, frame block, detailed index formulas, resource profile, SLO or mandatory review checklist was added.

The [existing downlift plan](/Users/ziniuw/Desktop/projects/G5-benchmark/twenty-clean/.g5/downlift/rust-core/downlift-plan.md#checked-ontology-guardrails-2026-10-13) records the rationale, inspected enforcement, exceptions and executed checks: 24 runtime tests passed, five structural categories passed and all 13 negative controls were detected. Static ontology/source-map validation passes. These checks do not establish the deployment clock or turn conditional proof results into whole-system conformance. No production code changed and no new TLC/Verus campaign ran.

Only authoring guidance changed in the harness: [guardrail selection](../../../.agents/skills/ontology-authoring/SKILL.md) and [sparse review-area organization](../../../.agents/skills/ontology-authoring/references/structure.md). The same guidance is applied to Twenty Clean's local authoring skill and synchronized to the G5-Lean harness mirrors. Planning, downlift and review receive no new phase or verification-closure requirement. The earlier planning and downlift closure recommendations above were not applied.
