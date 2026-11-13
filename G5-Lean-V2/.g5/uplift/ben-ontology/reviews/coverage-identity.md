# Ontology reconstruction coverage: identity, approvals and conversations

Baseline: `/Users/ziniuw/Desktop/projects/G5-Lean`, HEAD `2d3bb3c586e0423d04bb2565a946f39602ba3f2b` (unchanged). Read-only bounded source audit on 2026-10-15. No application, ontology, skill or model edits; no runtime tests or TLC run. Only this report was written. Priority means importance to a replacement contract, not a claim of an exploitable defect in current code.

## Method and inspected scope

Read the current uplift and authoring skills, uplift `execution-plan.md` and `intent-and-design.md`. Built an initial capability inventory from public route/service names, durable records, tests, original design material and focused Git history; then challenged what a replacement could omit while obeying the current ontology. Existing model counts and previous candidate lists were not used as a coverage denominator.

Canonical contract bodies read before selected metadata:

- [.g5/ontology/root.g5.md:1-77](../../../ontology/root.g5.md#L1); `features/users.g5.md:1-47`; `organizations.g5.md:1-65`; `chat.g5.md:1-277`; `workspaces.g5.md:1-137`; `ui.g5.md:1-46`; `concepts/security.g5.md:1-16`, `data_model.g5.md:1-36`, `system_invariants.g5.md:1-16`, `system_design_principles.g5.md:1-16`.
- Selected metadata only: users `:73-91` (login/session/API-key boundaries); chat `:440-474` (accounts/auth provenance); workspaces `:265-281` (fork/transfer/approvals). The approval metadata explicitly says direct decisions and merge admission have distinct owners, but those owners are not present in the current canonical body.

Direct code reads (ranges are scoped reads, not whole-file coverage):

- [backend/g5-api-core/src/services/identity/permission_service.rs:1-300,350-472,1000-1130](../../../../backend/g5-api-core/src/services/identity/permission_service.rs#L1); `org_service.rs:115-185,806-976`; `auth_service.rs:1380-1435,1481-1708`.
- [backend/g5-api-routes/src/routes/auth.rs:715-840,1100-1225,1588-1628](../../../../backend/g5-api-routes/src/routes/auth.rs#L715); `config.rs:1-200,230-293`; `settings.rs:278-354`; `chat.rs:2201-2297`; `internal.rs:868-943`.
- [backend/g5-api-core/src/services/workspace/approval_definition_service.rs:1-46,199-330,649-765](../../../../backend/g5-api-core/src/services/workspace/approval_definition_service.rs#L1) (output for the latter validation range was partly truncated; no whole-range coverage claimed); `approval_config_service.rs:865-1010,1300-1435,2370-2608`.
- [backend/g5-api-core/src/services/agents/provider_account_service.rs:1-150,170-245,305-468,550-700](../../../../backend/g5-api-core/src/services/agents/provider_account_service.rs#L1); `config_service.rs:685-720,838-885,1260-1340`; `chat_service.rs:1-70,1331-1410,1480-1527`; [backend/g5d-agent/src/runtime.rs:599-632,2337-2408](../../../../backend/g5d-agent/src/runtime.rs#L599).
- [backend/g5-api-core/src/services/identity/spend_service.rs:1-20,184-349,613-690,722-770](../../../../backend/g5-api-core/src/services/identity/spend_service.rs#L1).
- Existing test source: [backend/g5-api/tests/api_auth.rs:1488-1625](../../../../backend/g5-api/tests/api_auth.rs#L1488) plus an incidental read of `3542-3731`; `api_org_admin_authority.rs:1518-1640`; route unit-test `auth.rs:1595-1628`. Test names/search hits elsewhere were discovery only. No tests were executed.
- Schema: [backend/g5-api/src/db/migrations/066_approval_phases.sql:1-56](../../../../backend/g5-api/src/db/migrations/066_approval_phases.sql#L1).
- Capacity follow-up: [backend/g5-api-core/src/services/agents/agent_dispatch.rs:75-115,1000-1050,1290-1338](../../../../backend/g5-api-core/src/services/agents/agent_dispatch.rs#L75); [backend/g5-api-data/src/models/workspace.rs:65-88](../../../../backend/g5-api-data/src/models/workspace.rs#L65); [backend/g5d-dispatch/modules/catalog.json:1-120](../../../../backend/g5d-dispatch/modules/catalog.json#L1). These are admission/catalog reads, not a concurrency test.

Original/corroborating records read: [.g5/chat_intent/issue-231/request.md](../../../chat_intent/issue-231/request.md) (all), [.g5/ontology-plan-issue-231.md:1-59](../../../ontology-plan-issue-231.md#L1), [.g5/chat_intent/ux-v2-workbench/request.md:1-130](../../../chat_intent/ux-v2-workbench/request.md#L1), its plan `:1-95,275-325`, Base PR admin plan `:1-70`, [docs/reference/designs/API_AND_STORAGE.md:1165-1334,3045-3087,3320-3365,4066-4076](../../../../docs/reference/designs/API_AND_STORAGE.md#L1165), [docs/dead-code-proposal.md:47-59,265-280](../../../../docs/dead-code-proposal.md#L47). [docs/ben_new_ontology.md](../../../../docs/ben_new_ontology.md) keyword passages were compared with the canonical bodies; its attempted broad output was truncated and does not establish a complete source read. Focused history: original commit messages/diffs/statistics for `4c9aa418`, `46eaf0cb`, `28a692f1`, `1786f58f`, `ba8a577d`; pre-rebuild ontology at `8571e389^`, selected bodies/ranges detailed below. Historical ontology and saved agent plans corroborate lost design, not automatically historical human intent. Live external issue/PR discussion and original meeting transcripts were not fetched.

## Obligation inventory

| Independently discovered obligation | Current canonical owner | Disposition and inspected counterpart |
|---|---|---|
| Roles, viewer write ceiling, tenant isolation | `g5.organization.membership/viewers/members/admins/tenancy`, `g5.security.tenant` | Present. Scope union can exceed the ceiling; this is the existing CI-G3 implementation/policy reconciliation, not a new missing-role finding. PermissionService direct reads. |
| Current membership/permission authority at mutation time | `g5.user.api-key-authority`, `g5.user.session-authority`, tenant/security blocks | Partial: explicit for API keys and org switching, not the general revocation-sensitive user operation contract. PermissionService `:412-469`; F7. |
| OAuth attempt proof, verified identity linking, refresh expiry/logout | `g5.user.login-proof/session-authority` | Present in ontology. Ordinary callback implementation was only inventoried, not re-audited fully. |
| Invitation expiry/revocation and existing-user timing | `g5.organization.invites/invitation-authority/existing-user-invitation-question` | Present or explicitly uncertain. No new omission claimed. |
| Organization onboarding and owner eligibility | `g5.team.onboarding`, `g5.workspace.identity/transfer.formal` | Present at outcome level. Active-owner removal/demotion guard preserves existing ownership intent. Historical fourth `owner` role needs reconciliation with supplied three-role ontology, not automatic restoration. |
| Workspace visibility and invitation audience | current generic access references | Partial; assigned to workspace/Git audit. Forwarded `scoped.rs:643-645` and old storage doc evidence. |
| G5 CLI secretless login and credential-class separation | nearest `g5.user.authentication/api-key-authority` | Missing/partial; no G5 CLI authorization owner. Auth routes/service and regression source; F2. |
| Approval actors, policy administration, phase/quorum semantics | `g5.workspace.stages/merge/approval-request-settlement/approval-cut-eligibility` | Partial; freshness/settlement precision survives but policy and human authority do not; F1. |
| Durable conversations, queued input, delivery, uncertainty, output, fork | `g5.chat.queue/delivery/uncertain-delivery/persistence/frozen-attempt/session-fork` | Present at stated scope. Full task/dispatch paths not independently re-read. |
| Exact current attachment delivery and safe recovery | nearest `g5.chat.frozen-attempt/module-input/delivery/recovery` | Partial; F3. Store-only attachment records are insufficient. |
| Attachment upload bounds and browser-active content handling | nearest persistence/tenant rules | Missing details observed, but weaker source pedigree in this pass. Browser route serves only inert raster inline, otherwise download; numeric caps not promoted to top findings. |
| Native question recovery, frozen answer, exact transport evidence | `g5.chat.native-question-recovery` | Present; no new finding. Other native-request protocol clauses not broadly inspected. |
| Provider account selection, encryption and bounded hints | `g5.chat.accounts/hints/credentials` | Present for selection/secrecy; account identity, assignment and deletion continuity partial; F6. |
| Scoped settings inheritance and exact confirmed launch | `g5.chat.inheritance/launch/frozen-attempt` | Partial: admitted snapshot exists, user-confirmation-to-admission agreement missing; F5. |
| Workspace budget admission and period-wide usage reporting | nearest `g5.chat.usage` | Unresolved retention/scope: prior explicit contract and continuing accounting/settings have no current disposition; enforcement was already missing. F4. |
| Default execution concurrency, VM/API agreement and operation headroom | `g5.chat.dispatch-eligibility` | Partial; shared capacity does not retain the explicit default or safe rollout/operational headroom contract; F8. |
| Notification private inbox and post-commit delivery | `g5.user.notifications` | Present in body. Preference implementation gap already documented; no fresh delivery implementation audit here. |
| General granular scope-management composition and retirement | `g5.organization.admins/viewers` | Uncertain: current additive implementation, old contradictory doc comments, planned retirement and stricter supplied role ceiling need explicit reconciliation; see U1. |

## Prioritized missing or incomplete commitments

<a id="f1"></a>

### F1 — High: approval evidence is specified, but the approval policy is not

**Nearest contract:** [.g5/ontology/features/workspaces.g5.md:63-76,129-137](../../../ontology/features/workspaces.g5.md#L63). These state that phases/merges *may* be gated and define individual request settlement and eligible evidence. They do not specify who controls requirements, which actors must approve, which target owns requirements, or which phases actually gate merging. A replacement can implement fresh, idempotent approval records yet permit workspace owners to remove inconvenient requirements, count one of several required reviewers, or require BE/SE sign-off to advance.

**Current evidence:** `approval_config_service.rs:880-941` makes attach/retire organization administration independently of workspace ownership; `:2370-2455` evaluates the configured target's active IE requirements; `:2487-2577` requires every listed human and the configured agent slot, with a rejection prevailing and otherwise pending; `approval_definition_service.rs:1-7,255-260` preserves append-only definition history. `approval_config_service.rs:1305-1425` treats the human actor/request as a distinct authorization identity. The existing eligibility block leaves its undefined “eligible definition version” policy without the invalidating-version boundary at `approval_config_service.rs:2437-2446`.

**Source corroboration:** [.g5/chat_intent/ux-v2-workbench/request.md:22-25](../../../chat_intent/ux-v2-workbench/request.md#L22) records the human phase migration; `:106-110` labels IE-only/advisory policy as agent interpretation. [.g5/pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md:18-22,69-73,280-325](../../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md#L18) preserves that design and its owner. Pre-rebuild `8571e389^:.g5/ontology/features/governance.g5.md:434-452` independently states human approval, addressed-reviewer access, org-admin requirement administration, IE merge requirements and advisory BE/SE. Commit `46eaf0cb` explicitly reports the human-directed addressed-reviewer wording and the security repair rejecting workspace-scoped `org:admin` grants for requirement administration. Migration `066_approval_phases.sql:1-22` corroborates the accepted phase transition.

**Minimal proposals, not edits:** Give `features/workspaces.g5.md` an approval-policy owner: “Organization administrators attach and retire requirements for merges into a workspace. A merge to its configured target needs eligible approval from every required human and any configured agent reviewer for every active IE requirement; BE/SE sign-offs do not block phase changes.” Add independent human authority: “A role-eligible addressed reviewer with current workspace read access may approve or reject their pending request; ownership, rule administration, native tool permission and agent review do not substitute for that human decision.” Preserve definition history and define which new version invalidates earlier decisions instead of leaving “eligible” circular.

**Confidence/limits:** High that current coverage is incomplete; high for org-admin/phase policy, medium for recovering all definition-version/quorum policy from historical design. Original external discussion behind older quorum design was not retrieved. Human-vs-viewer exception remains governed by the existing viewer Q; do not silently broaden it. This is not an assertion that all current approval paths enforce these rules.

<a id="f2"></a>

### F2 — High: G5 CLI authorization and delegated-key non-escalation are absent

**Nearest contract:** users `:13-16,31-34,44-47` and chat `:183-206`. Chat's device authorization is native **Codex** login, not `g5d auth login`. API-key scope checks do not say an API key can never exchange itself for a broader, longer-lived personal session.

**Current evidence:** `routes/auth.rs:1100-1222` exposes unauthenticated CLI start/poll and a human-session-only approve/deny route. `auth_service.rs:1481-1700` checks pending/unexpired grant, target membership and optional workspace access, consumes an approved request once, then issues the person's session. `api_auth.rs:1488-1579` explicitly covers read-only, cross-org and full-scope API keys remaining unable to approve/deny, without issuing refresh tokens. This is existing test source, not a run.

**Original corroboration:** merged repair `4c9aa418` (2026-10-12) explains how dropping an API key's identity at these endpoints previously minted the creator's full-role JWT/refresh token or WS ticket. Pre-rebuild security `:194-204` had `g5.security.login.service-identities` and `.delegated-keys`; neither survives in the current user/security contract. Do not label the commit's author historical human intent; it is explicit merged agent/security design with a concrete regression.

**Minimal proposal:** users/security owner: “A short-lived, one-use G5 CLI authorization exchanges credentials only after the person's own authenticated session approves the matching request for an organization/workspace they may access. API keys and host/VM identities cannot approve or deny that request or mint personal session credentials.” Coordinate the WS-ticket clause with the root audit.

**Confidence/limits:** High. Current routes implement the distinction; no new exploit or runtime result claimed. Exact race/retry behavior of device approval was not qualified.

<a id="f3"></a>

### F3 — High: storing/freezing attachments does not require delivering the intended content

**Nearest contract:** chat `:83-106,138-141,213-216,266-269`. These retain records and frozen descriptors, but never require identity/integrity checks, unsupported-content refusal, native encoding, or distinguish replayed history references from fresh byte delivery. A replacement could preserve attachment records, hand the provider only prompt text, and declare delivery without violating the literal text.

**Original source:** [.g5/chat_intent/issue-231/request.md:1-3](../../../chat_intent/issue-231/request.md#L1); [.g5/ontology-plan-issue-231.md:8-20,35-48](../../../ontology-plan-issue-231.md#L8) explicitly require text/image/file/reference handling, authorized integrity-checked retrieval, supported native encoding and explicit refusal. Its current implementation design deliberately refuses unsupported non-image files before acceptance. The original integrated implementation is `28a692f1` (2026-09-29), not a speculative old stub.

**Current evidence:** `routes/internal.rs:887-916` binds retrieval to the live workspace incarnation and same workspace session. `g5d-agent/src/runtime.rs:2337-2407` checks attachment/session/input/metadata/hash and actual image format before materialization; `:607-622` yields failed/undelivered rather than silent text-only success. Historical contract `8571e389^:features/conversations.g5.md:113-119` states that recovery may describe prior attachments but must not redeliver their bytes as a new user action.

**Minimal proposal:** expand the chat input owner: “Deliver the reserved typed content with verified identity and integrity through the provider's supported encoding; missing, corrupt, unauthorized or unsupported content fails explicitly before delivery is claimed. Recovery may reference prior attachments as history, but only the current reserved input delivers their bytes as a new action.”

**Confidence/limits:** High for missing content obligations. Supported file formats and numeric bounds remain implementation/configuration detail unless the product contract requires them. Separate browser-safe attachment serving (`routes/chat.rs:2246-2295`) is a further observed security commitment worth owning, but original source pedigree was not resolved in this pass.

<a id="f4"></a>

### F4 — Material unresolved retention: prior workspace cost-control intent has no current disposition

**Nearest contract:** `g5.chat.usage` at chat `:243-246` only requires token usage visible through the chat API. No current canonical block requires workspace budgets, prevents new billable work after exhaustion, defines period windows, or includes failed/cancelled/jobless work in totals. A replacement can collect chat tokens yet omit all spending controls.

**Evidence:** `API_AND_STORAGE.md:1165-1233,4072` specifies workspace token/dollar thresholds and stops new work on exhaustion. The pre-rebuild settings contract `8571e389^:features/settings_and_usage.g5.md:7-15,33-78` adapts that outcome to unified tasks, including fresh attempts/jobless chats, period windows, unknown totals and admission-threshold rather than invoice-ceiling semantics. Its metadata `:115-149` clearly labels clarifications agent-originated and already records missing enforcement. Current `SpendService:202-275,294-343,744-770` retains totals, windows and limit calculation; `routes/settings.rs:278-354` retains settings. A repository search finds no production caller of `check_spend_limit` beyond its definition. [docs/dead-code-proposal.md:50-56,269-278](../../../../docs/dead-code-proposal.md#L50) explicitly says obsolete scheduler removal does not establish complete budget enforcement and distinguishes optional reporting retirement from accounting/control. The newer [docs/lean-backend-design-v9.md:1116](../../../../docs/lean-backend-design-v9.md#L1116) still assigns concurrency/spend ownership to typed workspace settings, but labels that full configuration target post-MVP; it does not by itself reauthorize every old admission rule. The [historical coverage ledger](../coverage.md#historical-investigation-and-model-coverage) and [chat/identity investigation](../investigations/chat-identity.md#L121) correctly decline to infer a production hard cap from comments.

**Minimal proposal, conditional on retention:** give settings/usage a semantic owner and first state whether budgets are retained current scope, deferred scope, or retired. If retained, the prior compact outcome is: “The owner may set workspace token/dollar thresholds for a configured period. Exhaustion prevents new billable attempts, including ordinary chats; recorded usage includes all workspace work regardless of job membership or outcome. These are admission thresholds, not a promise to cancel admitted work or cap the final invoice.” Retain the documented unknown-total policy if reconfirmed rather than replacing missing accounting with zero. If deferred or retired, record that decision so readers do not mistake absent implementation or omitted ontology for a product decision.

**Confidence/limits:** High that a consequential prior contract has no current disposition; uncertain that the entire old contract remains active current human intent. This is **a retention/policy question plus a previously known implementation gap**, not an asserted currently working spend guarantee. Restoring words would not implement budget enforcement. Recent `1786f58f` improves period accounting but does not supply an admission caller. Original external human budget decision unavailable, and code persistence alone cannot resolve retention.

<a id="f5"></a>

### F5 — Medium: a confirmed launch can silently change before admission under the current words

**Nearest contract:** chat `:13-26,103-106,163-166` covers choices, precedence, overrides and admitted attempt freezing. It does not require the frozen result to agree with what the person reviewed before pressing launch. Freezing a changed model, endpoint or account at admission still satisfies those blocks.

**Evidence:** `ConfigService::resolve_launch` `config_service.rs:687-720` routes confirmed selections through `confirm_agent_launch`; `:848-862` rejects a changed resolution and requests fresh review. The resolver supports non-persisting preview at `routes/config.rs:168-200`. Pre-rebuild `settings_and_usage.g5.md:19-31` explicitly owns common preview and confirmed-resolution agreement, with agent provenance in `:103-113`.

**Minimal proposal:** chat/settings owner: “Preview and launch use the same scoped resolver; when the user supplies a confirmed resolution, reject a launch if current resolved configuration differs, rather than silently running changed choices.” This is conditional on a submitted confirmation, not an invented universal approval gate.

**Confidence/limits:** High for source behavior and omission; historical attribution is agent design. All legacy callers were not traced.

<a id="f6"></a>

### F6 — Medium: named account scope is named, but continuity and assignment rules are missing

**Nearest contract:** chat `:168-176` defines scopes, invalid-selection refusal and bounded hints, without management/visibility authority, immutable identity or deletion consequences. A replacement can change an account's kind/scope in place or delete an assigned account, leaving many owners' inherited configuration broken, while still failing invalid selections correctly.

**Evidence:** `provider_account_service.rs:180-208,308-402,430-459,620-697` separates visible account context, immutable kind/scope, authorized update, assigned-account delete refusal and launch resolution. `config_service.rs:1264-1334` prevents a broader settings scope from pointing at a more private account and checks provider family. Pre-rebuild `provider_accounts.g5.md:25-61` explicitly preserves account scope, identity stability and assigned deletion refusal. The current service header `:4-16` names these as its responsibilities, not incidental storage mechanics.

**Minimal proposals:** chat/account owner: “Account kind and ownership scope remain stable for existing assignments; an assigned account cannot be removed until its assignments are cleared. Assignments must name a visible compatible account whose scope is valid for the settings being shared.” Separate account-management authority from launch-use authority. Do not simply copy today's `workspaces:write` mutation grant as the intended long-term rule because the supplied owner/viewer constraints may be stricter.

**Confidence/limits:** High for continuity omission, medium for fully recovered authority policy. Original provider-account human request was not located; use agent provenance for reconstructed policy. Credential encryption/hints themselves are already covered and should not be duplicated.

<a id="f7"></a>

### F7 — Medium: current authority is not owned generally outside API keys and org switching

**Nearest contract:** org role blocks, `g5.security.tenant.rls`, `g5.user.session-authority` and `api-key-authority`. They establish tenancy/roles and require current authority in selected paths, but do not preserve the explicit general outcome that removal/revocation takes effect on the next operation or that a mutation cannot use authority revoked before its admission commits.

**Evidence:** `permission_service.rs:412-469` resolves and holds current role/grants in the consuming transaction. `api_org_admin_authority.rs:1519-1637` tests admission waiting for role/default/grant/expiry revocation and holding admitted authority through commit. Pre-rebuild security `:40-44` explicitly makes each request read current membership; `:62-77` makes org Runtime Config target-org administration and current admission explicit. The focused merged Runtime Config repair is `ba8a577d`; current `routes/config.rs:251-269` uses the shared target-org admin boundary.

**Minimal proposal:** security owner: “A user operation is admitted against current organization membership and applicable permissions, not only previously issued token claims. Revocation completed before admission is respected; mutations preserve the authority on which their transaction relies.” Keep already-issued access-token/logout semantics distinct, and do not promise cancellation of every in-flight request.

**Confidence/limits:** Medium-high for lost governing precision. Much is a specialization of existing authorization intent rather than a wholly absent capability. Exact synchronization mechanism is implementation freedom. The viewer-scope conflict below remains separate.

<a id="f8"></a>

### F8 — Medium: shared dispatch capacity loses the explicit default and operational headroom

**Nearest contract:** `g5.chat.dispatch-eligibility`, [.g5/ontology/features/chat.g5.md:272-277](../../../ontology/features/chat.g5.md#L272), only requires agent/command shared capacity. A replacement can default to two turns or fill all VM slots with turns so Changes and diagnostics cannot run, yet satisfy those words. Preview exemption is already owned at `g5.preview.service`; do not duplicate it.

**Original authority:** [.g5/chat_intent/workspace-execution-slots-16/request.md:5-21](../../../chat_intent/workspace-execution-slots-16/request.md#L5) records the explicit human choice of default 16 and backfilling every row on 2. Its `:33-37` separates agent interpretation: align the VM catalog, preserve four task-less operation slots, keep older four-slot VMs at two agent/command turns until replacement, and promise concurrency rather than reserved CPU/RAM/disk. The saved plan [.g5/pr_ontology_diff_design/workspace-execution-slots-16/ontology-plan.md:19-49](../../../pr_ontology_diff_design/workspace-execution-slots-16/ontology-plan.md#L19) preserves former `g5.agents.admission.default-slots` (human value) and `.vm-slots` (accepted agent design). Original commits `62eeda87` and `77ebec25` retain the design and explicit backfill choice. Both source files were read in full for this follow-up.

**Current counterpart:** [backend/g5-api-data/src/models/workspace.rs:70-74](../../../../backend/g5-api-data/src/models/workspace.rs#L70) defaults to 16; `agent_dispatch.rs:84-100,1023-1028,1315-1319` caps both admission paths on old catalog VMs. [backend/g5d-dispatch/modules/catalog.json:2-3,12-18,62-68,106-112](../../../../backend/g5d-dispatch/modules/catalog.json#L2) permits 20 total bounded executions, 16 agent turns, and two each Changes/diagnostics. These remain present after the ontology rebuild.

**Minimal proposal:** chat admission owner: “A workspace defaults to 16 concurrent agent/command turns unless its owner chooses another supported limit. API admission and VM limits agree and preserve capacity for Changes and diagnostics while the default turn capacity is occupied. Existing VMs remain within their catalog's safe capacity until replaced. Slots bound concurrency; turns share VM resources.” The exact 16 belongs because the human chose it; exact 20 and old-cap 2 can remain the concrete compatibility design while those catalogs remain live. Do not elevate the completed backfill/multi-PR rollout procedure into a perpetual product behavior.

**Confidence/limits:** High that explicit retained intent is omitted, with stronger original human provenance than the budget candidate. Shared-slot counting is already covered; this is the missing policy/default and safe operational relationship. No admission or VM saturation test was run.

## Unresolved policy and rejected apparent gaps

**U1: granular grants are a policy/lifecycle hole, not permission to reinstate all historical code.** Current `PermissionService:15-24,354-409` unions role/org defaults/live user grants/workspace overrides. Old `API_AND_STORAGE.md:1290-1313` contradicts itself (“REPLACES” vs union), and its role list is older. Pre-rebuild security `:15-20` planned retirement of per-user grants and overrides with governance replacement. Current supplied `g5.organization.viewers` forbids write grants; tests still deliberately grant viewer `org:admin`. The existing CI-G3 discrepancy is therefore not a missing role ceiling. The missing part is an explicit decision about which configurable scope capabilities remain, under the role ceilings, and when retirement occurs. Do not add union as permanent D without reconciliation.

**Not new missing intent:** invitation revoke race (existing `invitation-authority`); browser-global preferences and absent notification controls (existing user intent with code gaps); active-workspace-owner removal/demotion guard (already follows member eligibility/ownership, although the user experience is less explicit); saved transcript/fork ordering, native challenge privacy, encrypted stored credentials, no-fallback invalid selection, native-question frozen-answer recovery (already present).

**Do not infer human intent from observed safeguards:** 5 MiB/8-file/10 MiB attachment caps, blank-secret update semantics, exact device-code alphabet, account name lengths, status enum mechanics, SQL lock implementation and API status codes need not be copied into ontology. Preserve their consequential outcome if corroborated, not every constant.

## Material remaining inspection limits

This is a bounded domain pass, not whole-system completeness. Detailed ordinary OAuth linking, all invitation writers, notification fanout, provider quotas and metering idempotency, full job/task DAG failure/continuation/cancellation/retention, approval review artifact production, native session bundle recovery and all browser consumers remain uninspected or partly inspected. At-most-one-task/session serialization was forwarded to the runtime agent as a candidate, not asserted from search hits. Workspace audience/visibility belongs to the workspace/Git report; WebSocket tickets and event delivery belong to the coordinator. No global coverage percentage is justified.

## Authorized refinement disposition — October 15, 2026

The initial read-only audit above is retained as historical evidence. The user subsequently authorized supported ontology additions and TLA+/TLC refinement, while continuing to exclude skill/production edits and application tests. At baseline `832651d0`, users/chat now own CLI authorization, typed attachment delivery, confirmed launch agreement, account continuity and execution default/headroom; approval/security proposals are integrated by the coordinator. Budget and granular-grant lifecycle remain questions.

[Fresh identity refinement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) executes 73 configurations across eight concern models, all matching their declared outcomes. It records positive safety, broken controls, success/recovery witnesses, one conditional local liveness check, and one implementation divergence separately. This is not 73 passing safety proofs or whole-domain completeness.

F1/F2/F3/F6/F7/F8 receive scoped source/model support and their named limits. **F5 uncovered CI-LAUNCH-CONFIRMATION:** the local resolver compares confirmation correctly, but the shared messenger queues only preferences and delayed dispatch resolves those again. A confirmed inherited model/account can silently change before first-attempt admission. The contract now preserves/revalidates the supplied confirmation through queueing; intended and as-built models remain separate. Current-owner credentials still apply, and unconfirmed launches continue using current defaults. [Incremental gap and proposed runtime reproduction](../refinement/identity-gaps.md) are source/model evidence only; no production fix or runtime reproduction occurred.

The original whole-domain inspection limits remain. Passing finite checks do not cover omitted consumers, cryptographic/DB/provider premises, all reviewer-list changes, global scheduler fairness or cross-domain composition.
