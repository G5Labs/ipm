# Workspace-owned coverage refinements

Integrated into `features/workspaces.g5.md`; preserve every supplied Ben body. New agent D blocks below recover supported intent and state model-refined conditions. Bodies own outcomes, not a prescribed mechanism. See [model/evidence limits](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) and [gap proposals](workspace-gaps.md).

## Bodies

<a id="g5.workspace.audience"></a>
[D · g5.workspace.audience]

A workspace is organization-visible or invite-only. Current organization administrators and the current workspace owner may read either; other current members need the selected audience grant. Only the current owner changes visibility or reader invitations, and invitees must belong to the organization. Creator and former-owner status alone retain no private access or administration. The child's [ancestor-read authority](#g5.workspace.ancestor-read) remains an additional read path; it grants no parent write authority.

<a id="g5.workspace.content-policy"></a>
[D · g5.workspace.content-policy]

The owner may make a non-Base workspace's application content read-only and restore editing, independently of attached compute. Read-only blocks application content and workspace-owned discussion writes. Authorized teardown, administration, personal preferences, Git synchronization and pure projection of accepted Git content remain governed by their own rules. This is not filesystem immutability or permission to bypass Base and VM authority.

<a id="g5.workspace.content-policy.transition"></a>
[D · g5.workspace.content-policy.transition]

Changing to read-only preserves existing work. A drain closes admission of new user content work while already-admitted agent and background work may finish; complete the policy change only after rechecking quiescence with competing admission excluded. Timeout or explicit cancellation restores editing without discarding work. A lost drain worker leaves a visible recoverable transition; the owner can restore editing. Automatic process-restart recovery is not promised.

<a id="g5.workspace.review-decisions"></a>
[D · g5.workspace.review-decisions]

The source owner decides acceptance, rejection and resolution; the reviewer may withdraw their own open suggestions. Reader replies carry no decision. Each suggestion keeps its discussion and decision history across its displays, and a decided suggestion cannot be reopened by a competing or stale action. Independently selectable edits settle only the selected edits.

<a id="g5.workspace.review-acceptance"></a>
[D · g5.workspace.review-acceptance]

Accepting a file suggestion applies the selected change in the owner's ready workspace, refusing missing or ambiguous source context instead of guessing. It then uses [selected-file Save](../../../ontology/features/documents.g5.md#g5.documents.selected-save), including accepted new files. Applied changes remain accepted when publication fails and expose the remaining publication work. Interrupted acceptance must recover prior application before a later decision can describe those bytes as rejected or withdrawn; retries must not apply the change twice. Record-only artifact decisions do not require a file write.

## Metadata entries

### g5.workspace.audience

- agent
- Evidence: [workspace sharing v9](../../../../docs/workspace-sharing-v9.md), access clarification; `db/scoped.rs`642–720 and owner-only visibility/invitation routes; [coverage refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md), WorkspaceAudience. Human ancestor access is preserved as a distinct originating-child capability. The exact mapping to direct human browsing remains unsettled; the direct-user predicate alone is not evidence that every ancestor read path fails. Audience reads are modeled at their admission boundary, not as an end-to-end stream revocation proof.

### g5.workspace.content-policy

- agent
- Evidence: human [workspace-button-gates request](../../../chat_intent/workspace-button-gates/request.md), editable/read-only controls, and amended [button gate plan](../../../ontology-plan-workspace-button-gates.md); `workspace_mutation_policy`1–109. Detailed exceptions are recovered agent design with medium confidence, not a new interpretation of Base or native-shell write authority. [Coverage refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md), ContentPolicy, checks finite operation classes; full route classification remains outside this pass.

### g5.workspace.content-policy.transition

- agent
- Evidence: `workspace_service`2249–2825 (durable freeze, detached drain, locked quiescence check and reversible owner cancellation), [coverage refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md), ContentPolicy. Historical migration 164 at `5e309160` explicitly describes crash-safe manual recovery, not restart resumption. The original transition design referenced there is unavailable; precision is recovered agent design at medium confidence. Modeled admission holds the inspected workspace lock through task creation; not every admission producer has been traced.

### g5.workspace.review-decisions

- agent
- Evidence: human [comments request](../../../chat_intent/comments-design/request.md) and retained [comments plan](../../../ontology-plan-comments.md); `workspace_reviews` and `workspace_review_service` transaction-scoped decision serialization and open-status compare-and-set. [Coverage refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md), ReviewAcceptance. Two selected edits are abstracted; full review-round carry-forward and artifact delivery are structural/unmodeled limits.

### g5.workspace.review-acceptance

- agent
- Evidence: human [issue-238 request](../../../chat_intent/issue-238-plain/request.md), [accepted-file plan](../../../pr_ontology_diff_design/issue-238-plain/ontology-plan.md) and `workspace_reviews`1396–1818/1912–2090. [Coverage refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md), ReviewAcceptance, separates guest write, checkpoint and SQL commit. `WORKSPACE-REVIEW-SETTLEMENT` records an as-built interruption gap; intended recovery is not claimed implemented. Exact-source publication inherits `WORKSPACE-SAVE-BLOB` and native-writer TOCTOU limits from selected Save; this block adds the acceptance trigger and truthful decision effect, not another Save owner.
