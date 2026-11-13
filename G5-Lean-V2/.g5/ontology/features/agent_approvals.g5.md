# Formal agent approvals

## Explicit admission

<a id="g5.agent-approval.admission"></a>
[I · g5.agent-approval.admission]

Request agent review explicitly starts a formal review, under the shared review-request authorization policy. Fork pushed source commits into a temporary workspace owned by the requester; its parent is the configured merge target. Reject an invalid or inaccessible target. Attaching requirements, publishing changes and moving inputs never start or rerun a review.

<a id="g5.agent-approval.admission-replay"></a>
[D · g5.agent-approval.admission-replay]

A client operation identity binds the requester, source workspace and normalized admission inputs to its retained request/run receipt. Replaying it returns the same receipt after settlement or failure as well as while active; unequal inputs conflict. Concurrent admission and reused active runs preserve these bindings. A deliberate new review or frozen execution retry uses a new operation identity. This prevents a lost response from launching another paid attempt without adding a separate batch ledger.

<a id="g5.agent-approval.inventory"></a>
[I · g5.agent-approval.inventory]

Home workspace and running-compute inventories include live formal review resources for readers of their source workspace, labeled Agent review and linked to the source Approvals page. This summary grants no live workspace, task or compute mutation access. Ordinary retention and cleanup remove resources from these inventories; retained reports remain available through approval history.

## Frozen assignment

<a id="g5.agent-approval.assignment"></a>
[I · g5.agent-approval.assignment]

Retain the actual fork manifest's original source identities, repository names and commits, the configured parent target's captured HEAD vector, and the source workspace's saved-base vector from its immutable original fork manifest. Missing or incomplete saved-base evidence fails capture rather than substituting a parent tip or computed merge base. At assignment capture, the shared Git comparison used by Source Control verifies a unique merge base from every captured pushed source/parent pair. Freeze those merge bases and an RFC 3339 UTC assignedAt in the same transaction as the immutable assignment. Review merge base → captured source; workspace base remains required complete provenance. Missing or ambiguous objects fail operationally before invocation. Reports and frontmatter include repository names and IDs, both vectors, definition/version, simple-agent prompt, harness/model/effort and versioned contracts. Use repository names in readable report references. Never retarget an execution. An explicit retry preserves this assignment, timestamps, runtime contract, prompts and skill; reviewing current inputs captures another assignment. Schema 4 reports copy complete source, target, workspaceBase and mergeBase vectors matched by repository ID and assignedAt; mergeBase entries have repository ID/name/SHA only. The confined write_report accepts only approved/rejected and a nonblank Markdown body, writes canonical metadata and its own UTC submittedAt, and harvest retains the exact bytes. Transient tool output is not durable submission; artifact retention time is distinct. Schema 5 adds a required agent-authored `summary` of at most 250 characters to the closed frontmatter, written through the confined write_report beside the verdict and body; it meaningfully states the review and verdict and is never derived from or clipped from the body. Schema 6 keeps schema 5's closed frontmatter and validation apart from `schemaVersion` and orders the body for readers: it starts directly with Governing prompt, then Verdict; Required criteria and supporting evidence; Blocking findings; Advisory findings; Waivers and non-applicable dimensions; Limitations; and last Review scope and provenance, which keeps scope, exact cuts, domains and posture together. No section is filled with invented findings or evidence, and stored reports are never rewritten. Older frozen schemas retain their original behavior. The confined comparison exposes frozen merge bases and explicit source, mergeBase, parent and workspaceBase reads; no moving refs or model-written frontmatter.

## Confined execution

<a id="g5.agent-approval.human-read-only"></a>
[I · g5.agent-approval.human-read-only]

Humans, including owners and administrators, cannot change the temporary review workspace through any surface. Readers of the source workspace's approval history may inspect the review's retained report, assignment and reviewed commits, and its transcript while ordinary task retention keeps it; target-side content requires target read access.

<a id="g5.agent-approval.human-read-only.agent-inspection"></a>
[I · g5.agent-approval.human-read-only.agent-inspection]

A workspace agent inspects the same retained evidence as its owner on any workspace its owner may currently read under [agent read scope](../concepts/security.g5.md#g5.security.agent-read-scope), including one the owner merely reads; requesting and cancelling approvals stay under the narrower [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) and its [opt-in](../concepts/security.g5.md#g5.security.agent-other-workspaces). The same read scope governs the [approval state](reviews.g5.md#g5.governance.approval.status), the retained report, assignment and reviewed commits, and the transcript, which keeps requiring target read access for target-side content. Inspection is read-only, and the reviewer's text is data, not instructions.

<a id="g5.agent-approval.execution"></a>
[I · g5.agent-approval.execution]

Preserve tenant and provider-account authority; platform operations are limited to the review's recorded resources. Run the assignment-versioned sole skill, prompts and closed frontmatter contract with the system/user boundary intact. During execution allow reviewed-content reads and only the assigned output write; no delegation, Git mutation, inherited tools, settings or project instructions may widen authority.

<a id="g5.agent-approval.launch-policy"></a>
[D · g5.agent-approval.launch-policy]

For [confined execution](#g5.agent-approval.execution), task admission supplies the harness-fixed permission mode before shared launch validation. Ordinary conversation modes are neither prerequisites nor overrides; provider-account authority still follows the shared resolver.

<a id="g5.agent-approval.codex-launch"></a>
[D · g5.agent-approval.codex-launch]

Codex [confined execution](#g5.agent-approval.execution) uses the shared [App Server transport](chat.g5.md#g5.chat.supervisor) in the review's fresh private home and launch directory, with strict configuration and the pin's runtime opt-outs. It disables project instructions, skills, hooks, plugins, memory, web, shell, image and delegation tools, applies a root-denying read-only permission profile and exposes only the two pre-approved formal MCP tools. It starts one ephemeral thread and never resumes one. Before the turn is sent, the thread response must confirm the frozen model, never-approval, read-only sandbox, an ephemeral thread (with no rollout path where reported) and no loaded instruction sources; otherwise the attempt fails without delivery. These controls replace exec-only flags and are verified with the pinned native binary against hostile inherited configuration and unauthorized operations. Codex formal attempts freeze this transport under a distinct module identity that accepts every assignment schema retained when it was introduced; a newer schema adds a newer identity, so an older runtime never receives it. Claude keeps its per-schema formal modules.

<a id="g5.agent-approval.contract-rollout"></a>
[D · g5.agent-approval.contract-rollout]

A new assignment schema uses a distinct negotiated runtime module and catalog identity, preserving historical modules for frozen retries. When the prior API cannot read the new durable schema, deploy coordinators without old/new overlap before new assignments are written. This transition uses Recreate for every API replica count and forward-only recovery, accepting rollout downtime to prevent an older reader from failing a valid review.

## Durable evidence and limits

<a id="g5.agent-approval.evidence"></a>
[I · g5.agent-approval.evidence]

After the writer finishes, read a fresh confined regular UTF-8 file, validate the exact assignment and its closed versioned contract, and retain its exact original bytes, SHA-256 digest and parsed approved/rejected candidate in Postgres with execution provenance. A schema 5 or 6 decision's summary is its exact frontmatter `summary`; earlier schemas expose no summary on compact surfaces. Missing, stale, symlinked, malformed or mismatched output fails the review. Output is at most 262,144 bytes including delimiters and body; raw YAML frontmatter is independently at most 32,768 bytes before YAML parsing. The nonblank body uses the remaining file budget. Derive guidance, parser and schema constraints from one versioned limit manifest; published migrations remain immutable.

<a id="g5.agent-approval.evidence-refs"></a>
[D · g5.agent-approval.evidence-refs]

At capture, retain private evidence refs at each repository's assigned source, workspace-base and parent commits. They are review evidence, not temporary resources: cleanup keeps them, Git transports never expose them, and they share the evidence's lifetime. Inspection shows base content only to readers of the target. Source deletion retains objects under ordinary access rules; it grants no new inspection or frozen-retry authority.

## Effectiveness and retirement

<a id="g5.agent-approval.effectiveness"></a>
[I · g5.agent-approval.effectiveness]

Persist candidate evidence, then seal the temporary workspace through an enforced mutation boundary before making the verdict effective. Task success alone grants no approval. Mark deletion intent and stop/reap/delete only review-created temporary resources; evidence refs remain with the evidence. Never merge temporary resources back. Deletion scheduling or cleanup failure never delays or invalidates a sealed verdict; cleanup remains visible, discoverable and retryable. Evidence survives resource deletion under the ordinary approval-history audience and retention policy.

## Requests and attempts

<a id="g5.agent-approval.request-state"></a>
[I · g5.agent-approval.request-state]

A newer review request stays pending until an effective verdict settles it or it is cancelled; a failed review never closes it, so an unaddressed request remains visible. A newer request or its failure does not clear an earlier effective verdict.

<a id="g5.agent-approval.attempts"></a>
[D · g5.agent-approval.attempts]

Each execution is an attempt under its request; a failed attempt leaves the request pending, a retry or review of current inputs starts a new attempt, and starting a second active attempt is refused.

<a id="g5.agent-approval.write-authority"></a>
[D · g5.agent-approval.write-authority]

Only platform contexts and the review's own runtime context may change the private review workspace and its child resources; the database enforces this per context, and every system-scoped route proves its workspace binding before writing.

## Failure, time and recovery

<a id="g5.agent-approval.recovery"></a>
[I · g5.agent-approval.recovery]

Any pre-effectiveness operational failure is failed, with stage and cause, never a synthetic rejection. Preserve candidate evidence where available and stop compute on timeout or cancellation. Bound capture/fork to 5 minutes from first claim, readiness including capacity to 15, execution from input acceptance to 45, harvest/persistence and sealing to 2 each, and each cleanup attempt to 5. Waiting for coordinator capacity does not spend an unclaimed capture's budget; one slow owner operation must not stall the reconciliation loop for other reviews. Keep safe retries within the same stage budget and operation identity; reconcile ambiguous effects and crashes without automatic agent reruns or duplicate decisions. Cancellation cannot rewrite an effective verdict or abandon cleanup.

<a id="g5.agent-approval.recovery.readiness-consent"></a>
[D · g5.agent-approval.recovery.readiness-consent]

Readiness replays the review's one [Start consent](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start) and never revives or replaces another. If a Stop, expiry, failed boot, deletion or another Start has ended it, the attempt fails with a cause naming that end. If the readiness deadline passes while the workspace still waits, the cause names the wait, such as a full organization VM quota or no free host slot. If that wait cannot be read within 10 seconds, the cause falls back to `stage deadline exceeded` rather than delaying the failure. Either way Retry stays available.

## Ownership of effects

<a id="g5.agent-approval.coordination"></a>
[D · g5.agent-approval.coordination]

The approval service retains only assignment, evidence, stage/deadline and resource/cleanup coordination missing from existing owners. Compose checked fork, task and VM/lifecycle operations, adapting their policy at those owners rather than duplicating their state or creating another scheduler. The formal operation is the scoped exception to ordinary owner inspection, arbitrary guest authority and ordinary task-independent workspace lifetime.

## Metadata

Implementation and shared verification evidence: [issue #640 plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md). Required pull-request checks establish Linux VM coverage. The [October 19 owner decisions](../../chat_intent/issue-640/review-decisions.md) amend the original issue.

- Implementation: `approval_review_service.rs` owns admission, retry, reports and hidden refs; `approval_review_service/workflow.rs` coordinates the existing owners through its typed store; `approval_review_service/inspection.rs` reads retained commits and bounded transcripts under source/target authority; `WorkspaceForkService::{fork_for_approval, approval_source_heads, retire_approval_fork, pin_approval_base, pin_approval_evidence}`, `WorkspaceVmService::{resume_agent_review, stop_agent_review}` and `WorkspaceService::{seal_agent_review, delete_agent_review}` retain their effects. `g5d-agent/src/formal_review.rs` owns confined tools; migrations 094/095 own retained evidence and database constraints.
- Owner follow-up implementation and focused evidence are recorded in the same plan. The local candidate adds separate attempts/pending requests, independent reconciliation, exact-context command policies and guards, retained refs/inspection, actual uncertain-commit regressions, acceptance cases and a required staged-native-client CI step. Sixth/final reviewer findings are addressed or adjudicated; affected tests and final workspace checks pass after repair. Cloud run 8 passed staged native confinement and browser suites but failed the Rust XFS and Rust E2E lanes; detailed selectors are now diagnosed in the plan after access renewal. The owner confirmed object retention under ordinary source-history authorization; source deletion grants no new inspection/retry authority.

### g5.agent-approval.admission

- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).

- PR448 integration assessment: explicit owner/target admission and the existing formal lifecycle remain implemented. The 30-case formal-review suite passes locally. Legacy requests retain their immutable key/phase under workspace-wide admission; real public concurrent-attempt, frozen-assignment/replay and precommit-fence/postcommit-receipt checks pass after the compatibility repair. [Current evidence](../../../docs/review-flow.md#october-19-latest-main-integration) distinguishes local checks from pending new-head Cloud qualification.

- QA child-workspace follow-up: formal admission resolves the same explicit override or clone parent as outgoing approvals and merge admission, under the existing source lock and target-access guard. The exact reported 422 reproduces before repair; all 32 formal-review API cases pass afterward, including the inherited-target lifecycle and deleted-override refusal. The existing native-provider VM scenario covers both target forms; Cloud execution remains pending. [Repair and evidence](../../../docs/review-flow.md#october-20-qa-child-workspace-approval-repair).

### g5.agent-approval.admission-replay

- human-provided guarantee: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704).
- agent mechanism: [retained run aliases and admission fencing](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.agent-approval.inventory

- human-provided
- From [issue 704 follow-up](https://github.com/G5Labs/G5-Lean/issues/704#issuecomment-6350999389).


### g5.agent-approval.assignment

- Schema 6 report order: the owner's [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) and [schema 6 decision](../../chat_intent/issue-1095/report-order-contract.md); contract `g5.formal-agent-approval/v5`, Claude formal module 1.0.13 (schema 6), Codex App Server module 1.0.14 (schemas 2–6; 1.0.12 keeps 2–5) and catalog `2026.11.05-approval-report-order`. An older review runtime is refused with the existing retry notice.
- Schema 5 summary amendment: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- Follow-up: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704#issuecomment-6350999389) specifies workspace-base comparison and readable repository identity.

- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).

### g5.agent-approval.human-read-only

- human-provided
- [Recorded owner decisions](../../chat_intent/issue-640/review-decisions.md), DEC-1 to DEC-4; amends issue #640's "neither inspect nor modify". Inspection is retained data, never a live view of the temporary workspace.
- Implementation/evidence: retained `inspection.rs` routes and `FormalReviewInspection`, per-context SQL refusal, bounded pagination/target redaction, and human/VM Git refusal are checked in the [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary). Ordinary source authorization still governs source deletion.

### g5.agent-approval.human-read-only.agent-inspection

- human-provided, security, test-coverage
- Expands: [human read-only inspection](#g5.agent-approval.human-read-only).
- The owner's [issue #1019](https://github.com/G5Labs/G5-Lean/issues/1019) asked for agents to read why a review rejected, with agent-written content labelled as data. Implemented as the `review-report`, `review-transcript` and `review-commits` g5d commands over workspace-agent routes that call the same `ApprovalReviewService` reads as the owner.
- The owner's [issue #1099](https://github.com/G5Labs/G5-Lean/issues/1099) widened the admission: #1019's narrower own-and-owned relation moves to the owner's [read scope](../concepts/security.g5.md#g5.security.agent-read-scope) ([plan](../../pr_ontology_diff_design/issue-1099-approval-activity-feed/ontology-plan.md)), so the agent can drill into any event its [activity feed](reviews.g5.md#g5.governance.approval.activity-feed) surfaces. The shared `admit_approval_read` admits the target through `agent_read_target`; request and cancel keep their own-or-owned write admission. Evidence `an_agent_reads_review_evidence_on_any_readable_workspace` ([tests](../../../backend/g5-api/tests/formal_agent_approval_inspection.rs)).
- Not needing the opt-in follows [the setting](../concepts/security.g5.md#g5.security.agent-other-workspaces), which exempts reads.

### g5.agent-approval.execution

- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).
- Integration: main's issue #621 owner email remains an ordinary workspace feature; formal execution retains only its frozen-object read/report tool surface ([integration rationale](../../pr_ontology_diff_design/issue-640/ontology-plan.md#main-integration--2026-10-19)).

### g5.agent-approval.launch-policy

- agent
- Derived from [g5.agent-approval.execution](#g5.agent-approval.execution): implementation compatibility finding.
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation compatibility and evidence](../../pr_ontology_diff_design/issue-640/ontology-plan.md).

### g5.agent-approval.codex-launch

- agent
- Amendment: [#910 request](../../chat_intent/issue-910/request.md) and [formal-review migration design](../../pr_ontology_diff_design/issue-910/ontology-plan.md#formal-review-on-app-server); replaces the restricted `codex exec` launcher retained by [#809](../../pr_ontology_diff_design/issue-809/ontology-plan.md).
- Native evidence (2026-10-28, codex-cli 0.164.0): both real-binary formal dispatcher tests passed against hostile workspace and owner-home configuration, for schemas 2/3/4, Stop and a missing report; see the [validation record](../../pr_ontology_diff_design/issue-910/ontology-plan.md#real-binary-formal-validation-2026-10-28).
- Planning evidence (2026-10-28, CLI 0.164.0, no upstream turn): `thread/start` with `ephemeral: true` and the formal permission profile returned `thread.ephemeral=true`, `thread.path=null`, `sandbox.type=readOnly`, `instructionSources=[]` (with `project_doc_max_bytes=0`; a planted ancestor `AGENTS.md` otherwise loads). `activePermissionProfile` is reported but outside the stable schema, so runtime checks cannot rely on it.

### g5.agent-approval.contract-rollout

- Schema 5 ([issue #974](https://github.com/G5Labs/G5-Lean/issues/974)): contract `g5.formal-agent-approval/v4`, Claude formal module 1.0.11 (schema 5 only), Codex App Server formal module 1.0.12 (schemas 2–5; 1.0.10 keeps 2–4) and catalog `2026.10.30-approval-summary`. A schema-5 review on an older runtime catalog is refused at admission with a retry instruction.
- agent
- Required by the versioned assignment change in [issue 704](https://github.com/G5Labs/G5-Lean/issues/704) and confirmed by PR 705's reader-compatibility finding; see [the current design](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.agent-approval.evidence

- Summary storage amendment: the owner's [issue #974](https://github.com/G5Labs/G5-Lean/issues/974).
- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).

### g5.agent-approval.evidence-refs

- human-verified
- [Recorded owner decisions](../../chat_intent/issue-640/review-decisions.md), DEC-3/DEC-10: the owner approved retention of reviewed state and the proposed evidence-ref mechanism.
- Implementation/evidence: `WorkspaceForkService::pin_approval_evidence`, resolution-scoped identity and GitRepo create-only refs; cleanup, aggressive pruning, frozen replay and transport refusal are checked in the [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary).

### g5.agent-approval.effectiveness

- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).
- Temporary-resource scope clarified by [owner decisions](../../chat_intent/issue-640/review-decisions.md), DEC-3/DEC-10; evidence refs retain the evidence lifetime. The owner's [source-deletion answer](../../chat_intent/issue-640/review-decisions.md#source-deletion-clarification) confirms ordinary access rules.

### g5.agent-approval.request-state

- human-provided
- [Recorded owner decisions](../../chat_intent/issue-640/review-decisions.md), DEC-5/DEC-9.
- Implementation/evidence: attempt admission and typed store settlement/terminalization, gate/history projections and parent cancellation; failed-pending/earlier-verdict and later-settlement cases pass locally. See the [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary) for remaining Cloud and human-decision limits.

### g5.agent-approval.attempts

- agent
- Derived from [request state](#g5.agent-approval.request-state) and [owner decision DEC-6](../../chat_intent/issue-640/review-decisions.md).
- Implementation/evidence: common request-key-fenced admission returns separate request/attempt ids; both overlap directions, identical replay and new current/frozen attempts pass locally in the [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary).

### g5.agent-approval.write-authority

- agent
- Derived from [human read-only inspection](#g5.agent-approval.human-read-only) and [owner decision DEC-7](../../chat_intent/issue-640/review-decisions.md). FIX-3 names the workspace and its child rows. Attempt coordination records preserve ordinary scoped admission and cleanup-retry controls plus immutable-history guards; they are not private workspace resources. The final review clarified this scope without broadening authority.
- Implementation/evidence: migration 095 command-specific restrictive context policies, incarnation-bound route scopes and binding/final-schema guards. The role matrix and restricted migrations pass locally; full VM ingestion awaits Cloud as recorded in the [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary).

- PR448 integration: migration 100 extends the same four command-specific private-resource policies to the later revision journal, preserving existing grants and infrastructure authority. The final-schema inventory and the actual platform/runtime/read/ordinary/other-review/human visibility and insertion matrix pass locally; [current evidence](../../../docs/review-flow.md#october-19-latest-main-integration) records the Cloud-discovered gap and its repair.

### g5.agent-approval.recovery

- human-provided
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).
- First-claim capture budget and independent progress: [owner review FIX-2](https://github.com/G5Labs/G5-Lean/pull/642#issuecomment-6261547790).
- Implementation/evidence: production-loop liveness/capacity, stage/seal deadlines, query bounds through fork/VM reopeners, real uncertain-commit recovery and no-rerun cases pass locally. The [same plan](../../pr_ontology_diff_design/issue-640/ontology-plan.md#review-summary) distinguishes provider fixtures, native processes and pending Cloud proof.

### g5.agent-approval.recovery.readiness-consent

- agent
- test-coverage
- Expands: [recovery](#g5.agent-approval.recovery).
- Governed by [`g5.lifecycle.start`](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start). Source: [G5-Lean#1048](https://github.com/G5Labs/G5-Lean/issues/1048) and the owner's follow-up request in the issue's workspace chat (2026-11-05) to name the capacity wait.
- Before this change, a replay surfaced the placement refusal `Conflict: start intent is unavailable or expired`, and a deadline during a capacity wait read only `stage deadline exceeded`. `WorkspaceVmService::{resume_agent_review, agent_review_deadline_cause}` now report the end or the wait. Evidence: `a_failed_readiness_names_its_cause_and_leaves_retry_available` in [the formal approval tests](../../../backend/g5-api/tests/api_formal_agent_approvals.rs).

### g5.agent-approval.coordination

- human-verified
- [Issue #640](https://github.com/G5Labs/G5-Lean/issues/640), [implementation request](../../chat_intent/issue-640/request.md).
- [Owner decision DEC-10](../../chat_intent/issue-640/review-decisions.md) confirms the integration ownership commitments.
