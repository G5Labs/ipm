# Ontology

<a id="g5.workflow.ontology"></a>
<a id="g5.ontology"></a>
[I · g5.ontology]

The ontology is the one consistent, nonduplicated account of consequential human intent and design, including important decisions made by agents. It is the source of truth for that intent and design.

<a id="g5.ontology.priorities"></a>
[I · g5.ontology.priorities]

Ontology puts correctness first, human readability second and agent benefit third.

<a id="g5.ontology.minimalism"></a>
[I · g5.ontology.minimalism]

Explain the essential intent and decisions briefly.

- Omit details that code already explains or that a reasonable practitioner would infer from other statements.
- Keep conditions, exceptions and confirmed or notable carve-outs that change a rule’s meaning.
- Give a reason only when it is not obvious.

<a id="g5.ontology.lineage"></a>
[I · g5.ontology.lineage]

Keep each statement’s sources and attribution in its metadata so readers can trace conflicting statements; the statement itself does not cite issues, pull requests or bugs. When code and intent disagree, reconcile them without discarding valid intent.

<a id="g5.ontology.writing"></a>
[I · g5.ontology.writing]

Ontology writing is concise, consistent, deduplicated and clear, so people can read it easily.

<a id="g5.ontology.writing.length"></a>
[I · g5.ontology.writing.length]

A statement has at most three sentences outside its bullets, however many paragraphs they span, and each bullet has at most three sentences.

<a id="g5.ontology.writing.bullet-count"></a>
[I · g5.ontology.writing.bullet-count]

A statement may have no more than 10 bullets. Anything larger is worthy of a new concept, definition and collection of statements.

<a id="g5.ontology.writing.plain"></a>
[I · g5.ontology.writing.plain]

Write for a typical reader, at about an 8th-grade reading level. Prefer common words and short sentences.

<a id="g5.ontology.writing.id-levels"></a>
[I · g5.ontology.writing.id-levels]

Each level of a dotted ID has a statement that defines its concept. For example, `shop.cart` defines what a cart is before `shop.cart.limit` refines it. The root page’s overview defines the top-level name.

<a id="g5.ontology.writing.concepts"></a>
[I · g5.ontology.writing.concepts]

Define a shared concept or abstraction once when it makes many statements shorter or simpler.

<a id="g5.ontology.writing.file-size"></a>
[I · g5.ontology.writing.file-size]

No ontology file should exceed the platform's render size. Split a large page into several files that align with its abstractions or sub-components.

<a id="g5.ontology.design-principles"></a>
[I · g5.ontology.design-principles]

Use sound design and clear abstractions. Record system invariants and policies so agents can apply them when generating code.

<a id="g5.ontology.formal-verification"></a>
[I · g5.ontology.formal-verification]

Use the ontology to choose what matters for formal specifications and verification. Distinguish what tests, human checks, agent review and formal methods establish, and keep every claim tied to its evidence.

<a id="g5.ontology.sharing"></a>
[I · g5.ontology.sharing]

Make important invariants, boundaries, data models and design and coding principles reusable across projects in an organization. Keep shared meaning consistent, avoid duplication and make responsibilities explicit.

<a id="g5.ontology.material-change"></a>
[I · g5.ontology.material-change]

Add ontology only when work materially changes the design’s intent or introduces a significant concept.

<a id="g5.ontology.maintenance"></a>
[I · g5.ontology.maintenance]

During implementation, review fixes and later adjustments in a pull request, keep the owning statements current for important decisions and assumptions, in the same high-level, readable prose and with their essential qualifications.

<a id="g5.ontology.review"></a>
[I · g5.ontology.review]

Ontology review checks a design and its implementation against the ontology.

<a id="g5.workflow.ontology.review.conformance"></a>
<a id="g5.ontology.review.conformance"></a>
[I · g5.ontology.review.conformance]

Review the implementation against all relevant ontology rules, including unchanged rules. Follow the callers and consumers that could affect whether those rules hold.

<a id="g5.workflow.ontology.review.architecture"></a>
<a id="g5.ontology.review.architecture"></a>
[I · g5.ontology.review.architecture]

Review both the design and its implementation against the [system design principles](../concepts/system_design_principles.g5.md), including whether a simpler design meets the same requirements.

## Harness

<a id="g5.harness"></a>
[I · g5.harness]

The harness is the guidance, skills and checks G5 gives agents so their work follows the ontology.

<a id="g5.harness.human-readable-plan"></a>
[I · g5.harness.human-readable-plan]

A person reading the design and plan must be able to understand the request and the proposed implementation at a high level.

<a id="g5.harness.guidance"></a>
[D · g5.harness.guidance]

G5 provides only its planning skill for ordinary agents to discover. Ontology conventions, examples and instructions for explicitly requested deep PR review, initial uplift or downlift are ordinary documents. Ordinary implementation and review use native methods guided by the existing ontology and requirements. Formal workflows default to TLA+/TLC. A request or a project’s declared Verus foundation can select the Verus guides for uplift, downlift, new feature PRs or incremental re-verification.

<a id="g5.harness.verification-artifacts"></a>
[D · g5.harness.verification-artifacts]

TLA+ state machines, their evidence and a run’s findings live under `.g5/TLA_verification/`. The TLC foundation guide defines where agents save them, the formats G5 reads and how reviewers check them. The Verus foundation guide records proofs, model checks and their connection to the product in the project’s verification runbook.

- At the end of an initial uplift or downlift, the project explains how the next reviewer can validate a saved result, rerun an affected check and decide whether earlier evidence still applies. The handoff identifies what was checked and what was deferred.
- Every TLA+ verification or re-verification report records the human decisions the run relied on, their sources and any open questions about expected behavior. The agent can proceed on a stated assumption and identifies the conclusions that depend on it.
- When asked to prepare material for the viewer, the reviewer follows that guide to format the [findings files](documents/ontology.g5.md#g5.documents.findings.files), link the evidence and check that the owner’s decisions are preserved.
- A helper links each state machine from the metadata of the statements its manifest names. When that state machine has a manifest and a recorded run, as the viewer’s own test decides, the helper tags the statement `design-verified` in place of one weaker verification tag.
- The helper changes no other tag, link or statement prose. It does not judge outcomes, and it reports manifest IDs that match no statement.

<a id="g5.harness.guidance-delivery"></a>
[D · g5.harness.guidance-delivery]

Customer launches receive the same maintained guidance in their owner-scoped home. The installer refreshes only G5-managed guidance and retires only skills G5 recorded as provided; customer checkouts, owner settings and unrelated owner files are unchanged.

## Base re-verification

<a id="g5.reverification"></a>
[I · g5.reverification]

A Base re-verification run brings Base's formal verification, its state machines and their evidence, up to date in a workspace forked from Base. When that workspace merges into Base, Base counts as re-verified up to the commits the run forked from. Nothing else changes this, not even a pull request's own [Re-run Verification](ui.g5.md#g5.workbench.review-prompts), because pull requests merged around the same time are never checked together.

<a id="g5.reverification.run"></a>
[D · g5.reverification.run]

G5 keeps one record per run, which tells when Base was last re-verified and whether a run is in progress. A run is in progress until its workspace's first pull request into Base merges, which completes it. A failed fork, or deleting the workspace before that merge, abandons it.

- The record holds the run's kind, workspace, starter and start time, the Base commit it forked from in each code repository, and the completed run it continues from.
- The run's state is read from its fork, its workspace and that workspace's pull requests, never stored apart.
- An organization has at most one run in progress. Starting another is refused until that run completes or is abandoned.
- A run starts in one step with its fork, so both are admitted or neither is. Starting is refused while the [verification switch](ui.g5.md#g5.workbench.review-prompts.switch) is off, and to anyone who cannot fork Base.
- An incremental run needs a completed run to continue from, so an organization's first run is a full run.
- A run's record stays after its workspace is deleted.
- A run's workspace reaches Base only through a pull request, so a direct merge into Base is refused.
- A run's fork takes no other fork options, such as a merge target, a first chat or an issue.
- A fork still unfinished after 30 minutes has failed: its run reads as abandoned, and the next start, even a retry of that fork, aborts it so it can never finish.

<a id="g5.reverification.scope"></a>
[D · g5.reverification.scope]

A run's scope ends at the Base commits it forked from. An incremental run re-verifies what Base gained between the commits the previous completed run forked from and its own. A full run makes every model current at its fork commits, adds models for important behaviour that has none, and runs them all.

- Rebasing the run's workspace before it merges brings in later pull requests but does not widen the scope. Those, like everything merged after the fork, stay [not re-verified](#g5.reverification.status) until the next run.
- The run reports bugs and design simplifications as [findings](documents/ontology.g5.md#g5.documents.findings) and fixes no product code. Its owner decides them in the run's workspace before it merges.
- The run may repair models, and may keep checking past a bug with a repaired copy of a model. Only the as-built model's runs make a statement design verified.
- The run ends with every model parsing and running, which also catches models that a clean merge combined wrongly.

<a id="g5.reverification.status"></a>
[D · g5.reverification.status]

Base's re-verification status comes from Git and G5's records alone, with no AI calls. It counts what Base gained since the last completed run forked: the pull requests merged into Base, and the net lines added and removed in code repositories outside `.g5/`.

- Leaving out `.g5/` keeps a run's own models, ontology and findings out of the count. Commits made outside G5 count as lines but not as pull requests.
- A run's own pull request into Base, the one that completes it, counts as the run, not as a pull request.
- Before any run completes, the status says Base has never been re-verified and shows no counts.
- Lines in code that a model covers are not counted separately, because models name their code only in prose.

<a id="g5.reverification.workspace"></a>
[D · g5.reverification.workspace]

A run's workspace is a regular workspace owned by the person who started the run. Unlike [other forks](ui.g5.md#g5.workspace.ui.fork-landing) without an issue, starting a run lands on the new workspace's chat pre-typed with the run's prompt, under a bold cost notice about Base, and never sends it.

- The prompt names the run's kind, its Base commits and the label its findings use.
- It lists the run's work as required, optional and out of scope, following the [run's scope](#g5.reverification.scope).
- It asks the agent to write all of this into a Project Doc first, and to keep that document as its checkpoint and final report.
- Where it and the pull request re-verification guidance differ, the prompt wins: the agent pushes its work, extends earlier runs' findings, and reports product-code bugs instead of fixing them.

## Metadata

### g5.ontology

- human-provided
- Source: [Ben, line 311](../../../docs/ben_new_ontology.md#L311).
- Amendment: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), restated as the concept that defines the `g5.ontology` level.
- Former ID: `g5.workflow.ontology`.
- Children: [Priorities](#g5.ontology.priorities), [Minimalism](#g5.ontology.minimalism), [Lineage](#g5.ontology.lineage), [Writing](#g5.ontology.writing), [Design principles](#g5.ontology.design-principles), [Formal verification](#g5.ontology.formal-verification), [Sharing](#g5.ontology.sharing), [Material change](#g5.ontology.material-change), [Maintenance](#g5.ontology.maintenance), [Review](#g5.ontology.review).

### g5.ontology.priorities

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Harness rewrite intent](../../chat_intent/g5-lean/harness-rewrite-20261027.md).

### g5.ontology.minimalism

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 312](../../../docs/ben_new_ontology.md#L312).
- Amendment: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rules 3 (inferable details and carve-outs) and 4 (reasons only when non-obvious).

### g5.ontology.lineage

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 313](../../../docs/ben_new_ontology.md#L313).
- Amendment: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rule 4 (no provenance in statement prose).

### g5.ontology.writing

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119); the sentence comes from `g5.ontology.priorities`.
- Children: [Length](#g5.ontology.writing.length), [Bullet count](#g5.ontology.writing.bullet-count), [Plain words](#g5.ontology.writing.plain), [ID levels](#g5.ontology.writing.id-levels), [Shared concepts](#g5.ontology.writing.concepts), [File size](#g5.ontology.writing.file-size).
- Implementation: [examples](../../../.agents/guidance/ontology-examples.md#writing-rules), [format](../../../.agents/guidance/ontology-format.md), [checker](../../../.agents/guidance/scripts/check_ontology.py).

### g5.ontology.writing.length

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rule 1, with the [owner decisions](../../chat_intent/issue-1119/decisions.md) on the paragraph cap. Capping each bullet at three sentences is an agent reading of rule 1.
- The checker warns when a statement has more than three sentences outside its bullets, or a bullet has more than three.

### g5.ontology.writing.bullet-count

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: [owner decisions](../../chat_intent/issue-1119/decisions.md), 2026-11-06. Counting nested bullets toward the limit is an agent reading.
- The checker warns when a statement has more than ten bullets.

### g5.ontology.writing.plain

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rule 2.

### g5.ontology.writing.id-levels

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rule 5; the owner answered on the issue that it applies to the whole ontology, with the clean-up staged.
- The checker warns about each ID level below the top-level name that no block defines. Treating the root overview as the top-level definition is an agent interpretation.

### g5.ontology.writing.concepts

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119), rule 6.

### g5.ontology.writing.file-size

- human-provided
- Expands: [Ontology writing](#g5.ontology.writing).
- Source: owner request, 2026-11-07.
- Implementation: the Files view defers the rendered preview above 1 Mi UTF-16 code units in [FileView.tsx](../../../frontend/src/components/files/FileView.tsx), the [preview threshold](documents/editor.g5.md#g5.documents.editor.bounds); the [checker](../../../.agents/guidance/scripts/check_ontology.py) warns above the same 1,048,576 code units.

### g5.ontology.design-principles

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 314](../../../docs/ben_new_ontology.md#L314).

### g5.ontology.formal-verification

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 315](../../../docs/ben_new_ontology.md#L315).

### g5.ontology.sharing

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 316](../../../docs/ben_new_ontology.md#L316).

### g5.ontology.material-change

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Ben, line 320](../../../docs/ben_new_ontology.md#L320).

### g5.ontology.maintenance

- human-provided
- Expands: [Ontology](#g5.ontology).
- Source: [Harness rewrite intent](../../chat_intent/g5-lean/harness-rewrite-20261027.md).

### g5.ontology.review

- agent
- Expands: [Ontology](#g5.ontology).
- Defines the `g5.ontology.review` level for [ID levels](#g5.ontology.writing.id-levels).
- Children: [Conformance](#g5.ontology.review.conformance), [Architecture](#g5.ontology.review.architecture).

### g5.ontology.review.conformance

- human-provided
- Expands: [Ontology review](#g5.ontology.review).
- Source: [Ben, line 318](../../../docs/ben_new_ontology.md#L318).
- Former ID: `g5.workflow.ontology.review.conformance`.

### g5.ontology.review.architecture

- human-provided
- Expands: [Ontology review](#g5.ontology.review).
- Source: [Ben, line 319](../../../docs/ben_new_ontology.md#L319).
- Former ID: `g5.workflow.ontology.review.architecture`.

### g5.harness

- agent
- Defines the `g5.harness` level for [ID levels](#g5.ontology.writing.id-levels).
- Children: [Human readable plan](#g5.harness.human-readable-plan), [Guidance](#g5.harness.guidance), [Verification artifacts](#g5.harness.verification-artifacts), [Guidance delivery](#g5.harness.guidance-delivery).

### g5.harness.human-readable-plan

- human-provided
- Expands: [Harness](#g5.harness).
- Source: [Ben, line 317](../../../docs/ben_new_ontology.md#L317).

### g5.harness.guidance

- human-provided
- Expands: [Harness](#g5.harness).
- Source: [Harness rewrite intent](../../chat_intent/g5-lean/harness-rewrite-20261027.md).
- Amendment: [Review simplification request](../../chat_intent/g5-lean/review-simplification-20261030.md).
- Implementation: [guidance](../../../.agents/guidance/ontology.md), [planning](../../../.agents/skills/ontology-planning/SKILL.md), [deep review](../../../.agents/guidance/deep-review.md).

### g5.harness.verification-artifacts

- human-provided, agent
- Expands: [Harness](#g5.harness).
- Source: [Verification display decisions, 2026-10-31](../../chat_intent/ontology-formal-verification/verification-display.md), decisions 6, 11. Design: [verification display design](../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md#harness).
- The review guide, the move of the viewer's field rules into it, and its format checker: [findings decisions, 2026-11-01](../../chat_intent/ontology-formal-verification/findings-triage.md), decisions 9, 16 and 17.
- Implementation: [TLC foundation guide](../../../.agents/guidance/TLC/setup_foundation.md), [Verus foundation guide](../../../.agents/guidance/Verus/setup_foundation.md), [helper](../../../.agents/guidance/scripts/link_verification.py), [format checker](../../../.agents/guidance/scripts/check_findings.py).

### g5.harness.guidance-delivery

- human-verified
- Expands: [Harness](#g5.harness).
- Source: [Harness rewrite intent](../../chat_intent/g5-lean/harness-rewrite-20261027.md), requesting an incremental packaging update that preserves existing owner isolation.
- Implementation: [provider installer](../../../backend/g5d-agent/src/harness.rs).

### g5.reverification

- human-provided
- user-facing-behavior
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), the problem and owner decisions 1–3. Design: [Base re-verification plan](../../pr_ontology_diff_design/periodical-reverification/ontology-plan.md).
- Defines the `g5.reverification` level for [ID levels](#g5.ontology.writing.id-levels).
- Children: [Run](#g5.reverification.run), [Scope](#g5.reverification.scope), [Status](#g5.reverification.status), [Workspace](#g5.reverification.workspace).

### g5.reverification.run

- human-verified
- architecture-decision
- Expands: [Base re-verification](#g5.reverification).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decisions 4, 7 and 11. Reading the state instead of storing it was the agent's proposal, which the owner chose (question `run-state`, 2026-11-06). It keeps writes out of the merge, deletion and fork-abort transactions, so a missed write can never leave a run stuck in progress ([plan](../../pr_ontology_diff_design/periodical-reverification/ontology-plan.md#run-record)).
- Relies on the fork freezing each member's source commit and allocating the child at admission (`ForkRepoManifest` in `workspace_fork_service.rs`), and on workspaces being soft-deleted (`deleted_at`).
- Implemented: table `base_reverification_runs` (migration 155, tenant `SELECT`/`INSERT` only, with the formal-review restrictive policies every workspace-keyed table has); `services/workspace/base_reverification.rs` (`admit` under a per-organization advisory lock inside the fork's admission transaction, refusing while any other run can still finish; the shared derived-state query; `stale_run_forks`, which `WorkspaceForkService::abort_stale_run_forks` aborts before any start, a replay or retry included; `is_run_into_base`, which `workspace_service::merge_workspace` uses to refuse direct merges); `ForkWorkspaceRequest.reverification` (in the request digest, refusing other fork options); the clone route reads the switch and returns `reverificationRun`. The 30-minute bound is the agent's, from the review of 2026-11-06 that found a failed fork could block every later run. Tests: `g5-api/tests/api_base_reverification.rs` (switch, first run full, one in progress, concurrent starts, replay, a key naming one kind, other options refused, deletion abandons, a fresh pending fork blocks and a stale one is aborted, even by its own retry, merge completes, viewer refused, non-Base refused, direct merge refused).

### g5.reverification.scope

- human-provided
- Expands: [Base re-verification](#g5.reverification).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decisions 8, 12 and 14, and triage in the run's workspace (question `triage-location`, 2026-11-06). Issues created there link into that workspace, so those links stop working once it is deleted. Ending every run with all models parsing and running is the agent's addition; it covers what [model rebases](../software_architecture/git.g5.md#g5.git.conflicts-rebase.models) leave unchecked.
- Implemented as the run's prompt (`frontend/src/lib/reviewPrompts.ts`, `baseReverificationChatPrompt`); the scope is agent behaviour, not exercised by tests beyond the prompt's text (`reviewPrompts.test.ts`).

### g5.reverification.status

- human-provided
- user-facing-behavior
- Expands: [Base re-verification](#g5.reverification).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decision 5, and lines outside `.g5/` (question `lines-scope`, 2026-11-06). Counting runs' own pull requests as runs is the agent's reading.
- The foundation guide's viewer manifest format (`question`, `ontology_ids`, `runs`) lists no implementation files, so a count of lines in modelled code is not deterministic across foundations today. Only G5's developer helper `tools/tlc/scripts/model_evidence.py` declares them.
- Implemented in `base_reverification::read_status` and `complete_status`, behind `GET /orgs/:orgId/home/base-reverification` (`routes/home.rs`). Lines use the committed reader's Source Control summary (`source_control_summary_from`, and `_from_empty` for a repository with no recorded start), memoized per exact commit pair. The pull request boundary is the completed run's admission time, a few milliseconds after its fork read Base's heads. Lines are counted after the scoped transaction commits, one diff per commit pair at a time. Tests: `api_base_reverification.rs` (pull requests and lines since the fork, the completing pull request excluded and later ones counted, `.g5/` and the docs repository excluded, a repository added later counted from empty, never and current states).

### g5.reverification.workspace

- human-provided
- Expands: [Base re-verification](#g5.reverification).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decisions 6, 9 and 10, revised by the owner's answer to question `project-doc-writer` (2026-11-06): the prompt carries everything and asks the agent to create the Project Doc, with no other mechanism. Question `agent-opens-pr` was answered as irrelevant: the agent works as in any regular workspace.
- The prompt's exact wording is the product owner's to tune (decision 8).
- The landing reuses the fork landing's router state, so a reload before Send loses the draft. The prompt's precedence over the PR guidance answers the review of 2026-11-06, which found that guidance forbids pushing and reading other runs' findings and asks for repairs.
- Implemented: `baseReverificationSeed` in `lib/reviewPrompts.ts`, the `base-verification` seed header (`components/chat/ComposerSeedHeader.tsx`, `lib/workspaceLanding.ts`), applied by `WorkspacePage.tsx`'s `chatComposerDraftFor`. Tests: `reviewPrompts.test.ts`, `workspaceLanding.test.ts`, `WorkspacePageV2.test.tsx` (fork landing with a header), `HomeReverification.test.tsx`, `e2e/mock-preview/base-reverification.spec.ts` and `e2e/flows/base-reverification.spec.ts`.
