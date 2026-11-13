# Shared identity and approval proposals

Baseline `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`. Root integrates these into shared canonical files. Original Ben blocks remain unchanged; each new commitment supplements its current owner. Evidence scope and fresh receipts are in [identity models](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md). Model results must be finalized before describing these as checked.

## Security bodies

<a id="g5.security.credential-classes"></a>
[D · g5.security.credential-classes]

A person's session, delegated API key and host/VM identity are distinct authorities. Service identities do not become human users, and an API key cannot approve or deny G5 CLI login or mint personal session credentials or browser tickets that escape the key's scope, organization or revocation boundary.

<a id="g5.security.current-authority"></a>
[D · g5.security.current-authority]

Admit user operations against current organization membership and applicable permissions, not only previously issued token claims. Respect revocation completed before admission and preserve the authority relied on by a transactional mutation until it commits; this does not cancel every already admitted effect or existing connection.

## Workspace bodies

Place under approvals, before existing request/cut detail. The protected policy snapshot belongs in the policy block, rather than copied into all consumers.

<a id="g5.workspace.approval-administration"></a>
[D · g5.workspace.approval-administration]

Organization administrators attach and retire requirements for merges into a workspace. Workspace ownership or a workspace-scoped grant does not confer that organization-level authority.

<a id="g5.workspace.approval-policy"></a>
[D · g5.workspace.approval-policy]

A merge must use the source workspace's configured target and satisfy every active Integration Engineering requirement on that target: every listed human and any configured agent reviewer must have an [eligible approval](#g5.workspace.approval-cut-eligibility). Admission evaluates one protected view of the requirement set, definition versions and decisions, including concurrent attachments, publications and withdrawals. Business and Software Engineering sign-offs are advisory and do not block phase changes.

<a id="g5.workspace.approval-human-authority"></a>
[D · g5.workspace.approval-human-authority]

Subject to the [viewer collaboration question](../../../ontology/features/organizations.g5.md#g5.organization.viewer-collaboration-question), an addressed human reviewer with current workspace read access and an eligible organization role may approve or reject their pending request. Ownership, requirement administration, agent review and native tool permission do not substitute for that person's decision.

<a id="g5.workspace.approval-definitions"></a>
[D · g5.workspace.approval-definitions]

Publishing a definition appends a new version without replacing prior definitions or decisions. The latest version defines the required reviewers and phases; a decision predating the latest version marked as invalidating earlier decisions is ineligible, while a non-invalidating publication does not by itself erase earlier eligible decisions.

## Shared metadata to integrate in body order

### g5.security.credential-classes

- agent
- Source: [coverage C03/F2](../reviews/coverage-identity.md#f2), merged repair `4c9aa418`, former `g5.security.login.service-identities`/`.delegated-keys`. The original defect exchanged an API-key identity for the creator's broader credentials.
- Evidence: `CliAuthorization` checks the human-session route boundary and one-use grant CAS with two pollers and issue failure. Root's browser-ticket model owns ticket purpose/workspace binding. Hashing, entropy, signed credential parsing and transport secrecy are structural premises, not cryptographic proof.

### g5.security.current-authority

- agent
- Source: [coverage C23/F7](../reviews/coverage-identity.md#f7), retained security contract and merged Runtime Config correction `ba8a577d`.
- Evidence: `CurrentAuthority` maps `PermissionService::require_user_scopes_on_conn` and migration 042's current membership/workspace/default/grant locks to separate admission/effect/revocation actions. Expiring grants are evaluated after lock waits. This establishes only inspected transactional consumers; root's fresh WebSocket org-subscription finding is a separate as-built gap. No token logout/scope-ceiling amendment is implied.

### g5.workspace.approval-administration

- agent
- Source: [coverage C02/F1](../reviews/coverage-identity.md#f1), merged correction `46eaf0cb`, retained governance requirement policy.
- Evidence: `ApprovalPolicy` distinguishes admin policy edits from owner review decisions; current `require_requirement_admin` uses organization-scoped locked authority. General role ceilings and unresolved viewer policy still govern.

### g5.workspace.approval-policy

- agent
- Source: [UX-v2 request](../../../chat_intent/ux-v2-workbench/request.md), [retained approval design](../../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md). The IE-only/advisory rule is explicitly agent interpretation; quorum/version rules are recovered design, not newly attributed human text.
- Evidence: `ApprovalPolicy` checks configured-target admission, required human+agent quorum and the shared requirement-set/definition lock protocol against attach/retire, publish, decide/Undo and merge. Existing reviewed-cut/artifact eligibility is referenced, not re-proved. Fixed source-head input and complete actor eligibility are premises; full Git merge effects are outside this model.

### g5.workspace.approval-human-authority

- agent
- Source: [coverage C02/F1](../reviews/coverage-identity.md#f1), request-addressed decision semantics and explicit human correction reported in `46eaf0cb`.
- Evidence: `ApprovalPolicy` distinguishes human actor slots and rejects substitution; request id/version/phase settlement remains owned by existing `approval-request-settlement`. Viewer eligibility remains unresolved; this addition does not grant viewers new writes.

### g5.workspace.approval-definitions

- agent
- Source: [coverage C02/F1](../reviews/coverage-identity.md#f1), former governance/version contract and retained definition service.
- Evidence: `ApprovalPolicy` includes append-only publication, invalidating/non-invalidating versions and fresh approval after invalidation. Direct source shows definition locks serialize publishers; the old comment claiming unreviewed concurrent publication always conflicts is not promoted into ontology because serialized later publication can append normally.

## Unresolved candidates: Q proposals only

A Q is a scoped unresolved product decision, not permission for production behavior to diverge from an existing commitment. The selected provisional options below preserve established ceilings and expose remaining evidence gaps.

<a id="g5.chat.spend-scope-question"></a>
[Q · g5.chat.spend-scope-question]

Are the prior workspace token/dollar admission thresholds and period-wide accounting retained current scope, deferred, or retired? Provisionally retain period-wide usage accounting and treat threshold enforcement as deferred pending scope confirmation: current settings still expose thresholds, but dispatch has no discovered enforcement caller. Retaining enforcement requires covering ordinary chats and failed/cancelled/jobless attempts; retirement requires an explicit replacement or removal of the settings' promise. The product decision and a traced admission owner would settle this question.

Metadata: `agent, uncertain`; source prior `API_AND_STORAGE.md:1165-1233,4072`, former settings contract, v9 design `:1116` (full target marked post-MVP), [F4](../reviews/coverage-identity.md#f4). No new model assumes a working spend cap. Provisional deferral is a recommendation about missing scope, not a historical human decision.

<a id="g5.organization.grant-lifecycle-question"></a>
[Q · g5.organization.grant-lifecycle-question]

Which organization defaults, individual scope grants and workspace overrides remain supported, and which are retired with the newer role model? Provisionally preserve configurable grants only within the existing organization role ceilings, so a viewer cannot gain writes; do not treat the additive implementation as authority to widen a role. Alternatives are complete retirement or a newly explicit narrower grant model. Reconcile the prior retirement design and current consumers before removing valid capabilities or extending grants.

Metadata: `agent, uncertain`; source [F7/U1](../reviews/coverage-identity.md#unresolved-policy-and-rejected-apparent-gaps), prior security retirement design, current PermissionService and Ben's viewer ceiling. Existing CI-G3 remains an implementation/policy gap; no model treats permanent unrestricted union as governing intent.
