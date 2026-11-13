# How PairSoft and Twenty CRM were uplifted into ontology

Research retrospective · 14 October 2026. This document explains the **uplift** behind slide 3: recover intent, write ontology, check it against code with TLA+/TLC, refine its meaning and coverage, and finalize a readable reconstruction contract. It describes the recorded campaigns, including their unfinished work. It does not rerun verification or cover the subsequent Rust redesign, downlift or Verus campaigns.

The objective was to preserve enough important behavior that another implementation could reconstruct the product: outcomes, permissions, identities, data rules, workflows, exceptions and failure handling. Existing code establishes what happens; documentary authority and explicit decisions establish what should happen. A discrepancy can require correcting code, correcting the ontology, correcting the model, or asking a policy question.

PairSoft and Twenty reached this objective differently. **PairSoft began with existing ontology and draft knowledge; Twenty began without ontology.** Neither followed a perfectly linear five-step process. Human challenges and independent reviews repeatedly reopened earlier conclusions. Slide 3 presents the procedure distilled from that experience, rather than asserting that the original work already followed every current skill requirement.

## The five steps and their concrete outputs

| Slide 3 step | Work performed | Output used by the next step |
|---|---|---|
| **1. Recover intent** | Establish scope and source versions; read original requirements, decisions, documentation and relevant history; inventory capabilities independently of the paths already inspected. | Candidate commitments with sources, exceptions, conflicts and unknowns. |
| **2. Ontology** | Give each commitment one semantic owner; separate intended behavior from current implementation; organize concise feature rules, shared invariants and architecture. | A navigable contract with stable IDs and traceable evidence. |
| **3. TLA+/TLC model checking** | Represent selected code paths as states and transitions; express intended properties separately; explore permitted orderings, failures and recovery. | Counterexamples, scoped passes, useful completion witnesses, code bindings and explicit assumptions. |
| **4. Refine and reconstruct** | Investigate each discrepancy; strengthen or correct rules; independently search for missing capabilities and combinations of behavior. | Revised ontology, classified code gaps, policy questions and an updated coverage map. |
| **5. Finalize** | Remove duplication, preserve essential precision, reconcile questions, validate structure and report the actual evidence boundary. | Readable ontology plus a scoped handoff; unfinished investigation remains visible. |

## PairSoft: consolidate knowledge, then reconcile and strengthen it

### 1. Recover intent from existing material

The first recorded task, on **22 September**, consolidated the old `.g5` ontology and `docs/knowledge`. It sought to preserve human commitments and useful decisions that would be lost by reading code alone. Human feedback identified missing workflow/data concepts and rejected separate “knowledge” and “decisions” buckets that obscured ownership. The material was reorganized around workflows, data, invariants, security and architecture.

That initial activity preserved and reorganized knowledge; it did **not** establish that every inherited statement was current or correct. The knowledge articles described themselves as AI drafts awaiting review. Later work therefore traced their claims back to functional designs under `docs/analysis`, human-confirmed decisions under `docs/designs`, architecture decisions, feature tests and runbooks. Later authorized decisions could supersede earlier designs; generated summaries remained discovery aids. The verification source inventory recorded **279 feature-rule rows, including 63 contradictions and 48 gaps**. These were source-reconciliation tasks, not automatically software defects. [Lifecycle chronology](../../uplift_downlift_lifecycle/pairsoft_lifecycle_study.md), [source method](verification-targets.md#scope-and-sources).

Version separation mattered. The code-checking campaign used **PairSoftPlatform `b9613c09` and PairSoftCore 2.0.10**. That imported main snapshot offered no useful release history. Later ontology reconciliation also inspected a feature branch with newer over-receive and change-order decisions. A result against the baseline could not automatically describe that branch.

### 2. Write ontology while separating intent from implementation

Existing workflow rules supplied a starting vocabulary, but statements such as “an approval approves the requisition” omitted the identities and timing needed to distinguish a valid decision from an old one. Some ontology statements described implementation as if it were already guaranteed—for example, that receipt processing and purchase-order updates used one transaction.

The review mapped verification targets to their existing semantic owners, preserving stable IDs rather than creating a second set of competing rules. It also classified **112 working assumptions**:

- Recoverable intent belonged in its existing governing rule, with its original exceptions.
- Unresolved business choices remained explicit questions or clearly provisional assumptions.
- Environmental facts, such as database isolation, needed separate evidence.
- Model bounds and observation variables belonged in verification artifacts, not product requirements.

A passing model did not authorize a new policy. For example, a universal ban on self-approval could conflict with existing administrative exceptions; “all notifications eventually arrive” could contradict a deliberately best-effort workflow. [Assumption reconciliation](ontology-coverage-review.md#2-what-to-do-with-the-112-assumptions).

### 3. Model the actual behavior and check the intended rule

The October 1–3 campaign began with risky document-chain mechanisms, then expanded after the human challenged its breadth. Models covered approvals, purchasing, receipts, fields, numbering, replication, access, integration, deployment and other concerns. Each action was tied to inspected guards, updates and persistence boundaries. As-built models preserved missing checks; proposed repairs were separate configurations.

**Worked example: an old approval approves edited content.** The requirement was that a result apply to the submission and content actually reviewed. Model state represented the current requisition content, its review state and pending approval messages. Actions represented submission, approval processing, editing and result delivery. TLC could therefore explore an approval result being delayed while editing occurred.

One recorded failing configuration found this sequence: submit content A; approve A; delay completion; edit the in-review requisition to B; deliver the earlier completion; B becomes approved. TLC reconstructed an eight-state trace after generating **591 states, of which 303 were distinct**. These are counts from that failing run, not an exhaustive count of the whole system. The code binding showed that the receiver checked lifecycle status without establishing the necessary content/submission relationship.

A local replay then exercised the actual Requisitions service: prices changed from 10.50/11.50 to 9,999/8,888, yet the old result published approval of the edited content. The harness supplied Approvals events, so this was a **partial distributed reproduction**, not a complete live Approvals round trip. The refined contract required fresh submission identity, a matching reviewed snapshot and an explicit outcome for review-relevant edits. [F-06](findings.md), [approval contract](ontology-coverage-review.md#31-approval-authority-belongs-to-a-submission); exact model evidence is identified below.

The recorded campaign contained **749 TLC runs**, including as-built checks, broken controls, repaired alternatives and reachability probes. “Expected result” did not mean “property passed”: many configurations deliberately demonstrated a defect or reached a useful state. Local runtime experiments also corrected **11 models**, showing that code correspondence required investigation rather than trust in the first abstraction. [Verification handoff](verification-handoff.md).

### 4. Refine rules and challenge reconstruction coverage

Findings changed both the precision and correctness of the ontology:

| Example | What was missing or wrong | Resulting rule or disposition |
|---|---|---|
| **Receipt limits** | Checking each new receipt row separately missed several rows for the same order line; display-line renumbering could disconnect earlier receipts. | Before acceptance, combine earlier receipts and all new rows for each stable order-line identity, then check the current allowance. Local reproduction accepted quantity 3 against an order of 2. |
| **Over-receive policy** | Old prose imposed a strict ordered-quantity cap and excluded every fully received order. A later authorized design allowed extra quantity under a configured tolerance. | Preserve the policy-aware allowance and eligible fully received orders. Tightening policy does not invalidate already processed receipts. This correction depended on documentary reconciliation, not TLC choosing a policy. |
| **Receipt atomicity** | Prose asserted one transaction, while the checked baseline could save the receipt/outgoing event and then fail to save the order update. | Keep atomic completion as intended behavior and record the baseline implementation gap. Inspect the newer branch separately before carrying the defect forward. |
| **Tenant versus company** | Retained commentary confused an unfiltered company picker inside one tenant with a cross-tenant leak. | Distinguish tenant isolation from company-selection policy; do not preserve an incorrect security claim. |

These were not all solver discoveries. Source comparison, specification construction, model checking and runtime reproduction contributed different evidence. [F-11/F-17](findings.md), [ontology corrections](ontology-coverage-review.md#concrete-contradictions-to-fix-first).

Coverage was then measured against **65 core features**, not just the concerns already modeled: **39 covered, 16 partial, four planned and six excluded**. The registry also inventoried command/event handlers, triggers, aggregates and endpoints. This exposed the distinction between “we have a model for approvals” and “we have checked every important approval obligation.” [Coverage registry](coverage-registry.md).

### 5. Finalize the readable contract and report remaining work

The substantive ontology amendment was saved as **`c4abdb3b`**, followed by structure refinement **`81bad0e2`**. The latter organized shared concepts, feature outcomes and semantic module owners, retaining all **718 original IDs** and producing **772 records across 43 pages**. Thirty formal contracts from the preceding amendment retained their meaning. Duplicate summaries became references, and important conditions stayed with their owners.

Structural checks established ID preservation, navigation, metadata pairing and document integrity. They did not prove semantic completeness or runtime conformance. Self-approval, quorum, approved-line edits, mapping-lag omissions and notification recovery still had scoped questions. The verification handoff recorded **110 findings: 77 reproduced, 29 partly reproduced, two latent/not reproduced and two code-only**. Those findings were not all caused by TLC, and publishing stronger ontology did not repair the original application. [Applied changes and limits](ontology-coverage-review.md).

## Twenty CRM: reconstruct intent without prior ontology

Twenty is an open-source CRM with customizable records, permissions, workflows, communication integrations and extension mechanisms. Its uplift on **11 October** began with **no existing `.g5` ontology**. The original application was pinned to **`2f9bd3aa6bcffe2095beeff19296d53a43819188`**; the final uplift evidence discussed here is preserved in local commit **`c5060489`**. References T1–T8 below identify the historical files; the current checkout has since changed.

### 1. Establish scope and a documentary baseline

The user selected backend functionality, including server-side meaning of views/dashboards, while excluding UI/UX, Figma and Discord. Inputs included official product/developer documentation, local `twenty-docs`, server design records and targeted public issues, PRs, discussions and releases. The clone was shallow, so relevant upstream records supplemented local history. Unavailable roadmap/private records stayed missing evidence.

Before judging implementation, the agent saved a **documentary baseline and candidate ledger**. Candidate IDs recorded concise meanings, passages, conditions and exceptions. An independent domain inventory included identity, authorization, metadata, records, workflows, communications, AI, APIs, applications, files, billing and operations.

Authority was evaluated within each record: original comments and corrections mattered; merged code, bot summaries, confident prose and issue closure did not automatically constitute a human policy decision. Marketing wording such as instantaneous synchronization did not become a timing guarantee. Deliberate quota degradation and other documented exceptions had to survive extraction. [T1, T2](#twenty-source-records).

### 2. Trace candidates into code and write semantic contracts

The implementation evidence map traced candidates through entry points, permission resolution, database writes, workers and interruption boundaries. It recorded where code supported intent, where a stronger requirement was inferred, and where evidence was incomplete. Existing tests were inspected without automatically being counted as executed.

Concrete traces included recovery-code consumption versus token renewal; role/field/row permission composition; application schema synchronization; record restore versus retention cleanup; persisted workflow snapshots; and renewable Redis locks. These traces supplied feature rules and shared invariants with one owner per commitment. The initial ontology had **120 blocks**; later review expanded both its coverage and its necessary precision. This was not a translation of one source file into one ontology file. [T1, T3](#twenty-source-records).

### 3. Execute concern models, including checks of legitimate behavior

The initial campaign built five models and ran **26 configurations**:

| Concern | Behavior modeled | What the checks established or challenged |
|---|---|---|
| **Renewable locks** | Ownership, renewal/expiry, callbacks, reads/writes and release. | Owner-sensitive release could be correct while a stale callback still overwrote a successor's work after losing its lease. |
| **Trash cleanup** | Select an old deleted record, restore or delete it again, then permanently destroy it. | Selection did not preserve deletion authority indefinitely; an intervening restore or newer deletion could make the old cleanup decision invalid. |
| **Authentication recovery** | One-time consumption, expiry, security reset and token renewal under the inspected lock. | Scoped ordering properties passed; broken consumption/expiry/reread variants failed. This did not verify all authentication. |
| **Workflow execution** | Persisted run content, tenant identity, later execution and permitted run edits. | Selected execution used the stored flow; variants reading an inappropriate latest definition or wrong tenant failed. |
| **Quota admission** | Workspace/member checks, policy errors, usage charged after admission and infrastructure failures. | Legitimate overshoot and degraded admission refuted overly strict proposed requirements. A failed assertion could mean the specification was wrong. |

**Worked example: restore races with cleanup.** The code selected eligible IDs using a deletion cutoff, then destroyed those IDs in a separate operation without repeating the cutoff predicate. The model allowed restoration between those operations. TLC found a violation after reaching **eight distinct states**. A proposed eligibility guard passed its bounded check, and a completion probe confirmed that genuinely eligible trash could still be removed. The initial result was a source-bound model concern, not a runtime reproduction or application repair.

Of the 26 initial configurations, six were safety passes, nine broken controls, six completion witnesses, three as-built race counterexamples and two refutations of overly strong quota requirements. Except for a specifically stated timely-lease property, completion witnesses established possibility, not eventual completion under every fair schedule. [T3, T4](#twenty-source-records).

### 4. Reopen breadth, correct policy and refine the ontology

The user then requested independent review, broader reconstruction coverage, concise readable parents, runtime reproduction and reconciliation of policy questions. These were substantive extensions of the initial campaign:

1. **Review every existing block.** The 120-block ontology became **154 blocks**, including formal expansions and recovered policies/questions. Review sharpened membership lifetime, administrator continuity, delegated authority, named sharing, retention, deferred work and accounting. Source-supported exceptions corrected overly strong extraction.
2. **Inventory capabilities independently.** Documentation, registered entry points, jobs, action/tool catalogs and object behavior exposed functions absent from the original models. The next milestone had **137 capability families and 271 blocks**. Subsequent audits found more, including interpreter lifecycle, messaging operations, recording, workspace export and licensing. Earlier completeness claims were withdrawn.
3. **Investigate actual code separately.** New runtime experiments were authorized later. They exercised production methods with isolated PostgreSQL/Redis and explicit substitutes; copied algorithms and model traces did not count as production execution. Independent review rejected invalid test doubles. Coverage and reproduction failures remained recorded.
4. **Resolve policy with authority, not solver preference.** The user explicitly authorized reasonable assumptions when source evidence could not settle a choice. All **20 identified questions** received dispositions: three evidence-resolved, 16 adopted assumptions and one mixed answer. These did not retroactively become recovered historical intent. [T1, T5–T8](#twenty-source-records).

Two policy examples show the complete refinement loop:

| Question | Investigation and correction | What modeling contributed |
|---|---|---|
| **Which authority should a delegated AI tool retain?** | An initial blanket agent-role ceiling conflicted with SDK evidence for explicitly admitted member-runs and ordinary application execution. The adopted rule preserved the run's selected authority through delegation, while retaining those legitimate modes. | Source-reachable entry conditions and ordinary member/application success witnesses prevented a model from “proving security” by excluding supported work. Runtime confinement still required separate evidence. |
| **Must restoring an old record satisfy new business rules?** | Documentation supported recovery but did not fully settle changed-rule behavior. The adopted policy preserved historical values under current security/structural constraints, with diagnostics for invalid or unknown business-rule evaluation. Restoring with new input remained an ordinary validated write. | Comparing alternatives exposed that “new rules or no restore” could pass a safety check by blocking legitimate recovery. Broken controls and useful completion witnesses checked the selected policy. Diagnostics were a new adopted requirement, not a claim about the original code. |

The final policy campaign ran **eight modules / 104 configurations**, with results classified as policy/current safety, divergences, controls, useful outcomes and rejected alternatives. It was incremental evidence; its totals should not be added to earlier campaigns as a count of independent requirements or proven features. [T7, T8](#twenty-source-records).

### 5. Finalize readable ontology without concealing incomplete investigation

The final saved uplift snapshot contained **29 pages and 463 blocks**, preserving all 449 pre-policy IDs and the earlier 271 IDs. Concise parents expressed the main commitment; children carried necessary governing precision. The four root questions retained their identities and readable context, with explicit decisions beneath them. Source lineage and model limits stayed in metadata and evidence.

The final review nevertheless states that **complete backend intent coverage and all requested code reproductions remained unsatisfied**:

- The inventory contained **151 capability families**, 148 with some concern-model linkage. A link did not establish all clauses of that capability.
- **788 of 8,617 server files (9.14%)** had retained inspected ranges and matching hashes. This measured section-inspection evidence, not full-file understanding or functional coverage. Imports and search hits earned no reading credit.
- Original finding groups G01–G80 had **seven narrowly reproduced subclaims, 33 partial reproductions, two policy/static observations, 37 unavailable/inconclusive results and one unattempted broad gap**. They were not 80 demonstrated runtime bugs.
- Some reproduction actions had been stopped by automatic approval review, and others failed in the harness. Those limits did not justify unread source, establish product behavior or close the broader task.

Policy reconciliation and structural validation were completed within their scopes; they did not complete the earlier whole-backend audit. No production fixes, full live deployment verification or provider qualification were established by this uplift. [T5–T8](#twenty-source-records).

## How ontology, code and model checking worked together

The recurring loop was **source requirement → ontology obligation → code-bound model → executed check → investigation → revised ontology and gap record**. Breadth review supplied additional requirements from outside that loop, preventing the existing model set from defining what the product was allowed to contain.

TLC starts from configured initial states, applies enabled actions, records successor states and checks the stated properties. For a violation it can reconstruct a path of transitions. Broken variants test whether checks notice the missing protection; completion witnesses test whether useful behavior remains possible. Neither substitutes for a complete requirement or faithful model.

Code linking was an explicit abstraction task: inspect the real guard, update, message identity, transaction and failure boundary, then represent them in the model. It was **not a formal proof that the model and implementation were equivalent**. PairSoft's corrected models and Twenty's rejected test doubles demonstrate why correspondence needed challenge. Runtime reproduction could strengthen or narrow a finding; a hypothetical repaired configuration did not mean the application had been fixed.

The practical classification was:

| Observation | Correct response |
|---|---|
| Code violates supported intent | Preserve the intent; record the implementation gap and its evidence. |
| Prose is stale or omits an authorized exception | Correct the ontology from authoritative sources; update affected checks. |
| Model invents a guard or omits a relevant action | Repair the model/binding before interpreting the result. |
| Sources do not settle a business choice | Retain a question or explicitly authorized assumption; label conditional results. |
| A replacement could satisfy the ontology while omitting a major feature | Reopen source discovery and coverage, even if existing checks pass. |

## Lessons carried into the uplift procedure

1. **Recover intent independently of implementation.** PairSoft's draft knowledge needed original-source reconciliation; Twenty's documentary baseline prevented code from silently becoming policy.
2. **Keep exceptions while adding precision.** Receipt tolerances, quota degradation and member-run authority demonstrate why “stricter” does not necessarily mean “more correct.”
3. **Check identities and time boundaries.** Reviewed content, stable order lines, membership lifetime, lease ownership and retention eligibility cannot be replaced by a convenient status check.
4. **Challenge breadth independently of models.** Feature inventories and reconstruction review found omissions that a passing model could not report. New discoveries reopened completion claims.
5. **Separate evidence types.** Ontology coverage, inspected source, finite properties and runtime reproduction answer different questions. Their counts are not interchangeable percentages of correctness.
6. **Finalize meaning as well as formatting.** Short parents, stable IDs and semantic ownership help people review the contract; they must retain exceptions, unresolved decisions and evidence limits.

The [current uplift skill](../../../../.agents/skills/ontology-uplift/SKILL.md) formalizes this loop: recover/index intent; co-develop ontology and models; execute/reconcile; independently establish reconstruction coverage; finalize. Its stronger completion requirements are lessons from these campaigns, not evidence that both historical campaigns already satisfied them. Uplift ends with a reconstruction contract and known gaps. Choosing a smaller target, changing policy and generating a replacement remain separate design/downlift decisions.

## Evidence and retrieval

### PairSoft source records

The local [lifecycle study](../../uplift_downlift_lifecycle/pairsoft_lifecycle_study.md) links the September 22 consolidation chat and October 2 reconciliation chat, and records human corrections and chronology. Detailed source/obligation mappings are in [verification-targets.md](verification-targets.md), [coverage-registry.md](coverage-registry.md), [ontology-coverage-review.md](ontology-coverage-review.md), [findings.md](findings.md) and [verification-handoff.md](verification-handoff.md).

Original code checks used `b9613c09`; model/reproduction artifacts are preserved in PairSoft commit `2c8a288f028e2fbbd52597e66f6215aa9d06cb89`. The worked approval count comes from `.g5/TLA_verification/models/RA1/results.json`, entry `RA1_AsBuilt_HumanApprover`; its trace and bindings are in the same model directory. Ontology amendments `c4abdb3b` and `81bad0e2` describe a later branch and must not be treated as the checked main baseline.

### Twenty source records

Repository: `/Users/ziniuw/Desktop/projects/G5-benchmark/twenty`. Evidence snapshot: **`c5060489f3f466b48a8e6fc47ffcfe6cd359bc4f`**. The original application pin is `2f9bd3aa6bcffe2095beeff19296d53a43819188`. These paths are historical Git objects, not claims that the current working tree contains them.

| Reference | Historical repository path | What it establishes |
|---|---|---|
| T1 | `.g5/uplift/backend-2026-10-11/uplift-plan.md` | Scope, phases, human follow-ups, historical milestones and reopened completion claims. |
| T2 | `.g5/uplift/backend-2026-10-11/documentary-baseline.md` | Source authority, reviewed passages, candidate ledger and initial domain inventory. |
| T3 | `.g5/uplift/backend-2026-10-11/implementation-evidence.md` | Initial code traces, supported behavior, inferred obligations and inspection limits. |
| T4 | `.g5/TLA_verification/backend-2026-10-11/README.md` | Five initial models, all 26 configuration outcomes and their interpretation. |
| T5 | `.g5/uplift/backend-2026-10-11/uplift-review.md` | Final ontology, source/formal/runtime scope and remaining work. |
| T6 | `.g5/uplift/backend-2026-10-11/code-reproduction/README.md` | Corrected completeness assessment and reproduction classifications. |
| T7 | `.g5/uplift/backend-2026-10-11/clarifications.md` | All 20 question dispositions and their authority. |
| T8 | `.g5/uplift/backend-2026-10-11/policy-reconciliation/README.md` | Adopted policy, reviewed amendments, incremental checks and limits. |

Read a historical file without switching branches, for example:

```sh
git -C /Users/ziniuw/Desktop/projects/G5-benchmark/twenty show \
  c5060489f3f466b48a8e6fc47ffcfe6cd359bc4f:.g5/uplift/backend-2026-10-11/uplift-plan.md
```

The associated chats are [initial uplift](codex://threads/01a12c44-9742-7600-b495-3e68bcdfbb9b) and [continuation](codex://threads/01a12d47-7ab3-79e0-a9aa-5981625f1b4d). The continuation later entered downlift; that later work is outside this document. Companion PairSoft evidence is local and Git-ignored in this G5 checkout; external repository objects and chat links require access to the recorded repositories/account.
