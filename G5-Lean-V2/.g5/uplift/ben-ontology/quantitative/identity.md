# Identity, authority and chat quantitative coverage

Baseline: `072a12d3e99032ee3a7356837542c5edeb825984`. The preceding source inspection began at `0b50e8d8e7bdb79dd98b8a28c08c4da23db4242c`; the intervening commit only repaired documentation/reference history. No production, canonical ontology, harness, model, receipt, plan or shared coverage file was edited for this report.

This is a bounded **20-trajectory inventory**, independently selected from authentication routes, membership mutations, queue/dispatch consumers, persisted-state boundaries and failure branches. It is not an exhaustive denominator or a global coverage percentage. Intent dispositions: **{'partial': 4, 'adequate': 14, 'question': 2}**. Enforcement dispositions: **{'partial': 16, 'traced': 4}**. `traced` means the named bounded transition/caller boundary was followed, not that all consumers or concurrency behavior were tested. The JSON is the machine-readable range ledger.

Three amendment candidates and one policy question are listed below. Two amendments have strong retained design evidence; the SCM independence candidate is qualified. None has been applied. Existing confirmation, account, attachment, capacity and steering commitments are retained without duplicate proposals.

## Important behavior inventory

| ID | Behavior | Intent | Enforcement | Existing owners |
|---|---|---|---|---|
| IDN-01 | Browser OAuth proof, verified-identity linking, and admission/re-entry through the instance email allowlist. | partial | partial | `g5.user.authentication`, `g5.user.login-proof`, `g5.organization.invitation-authority` |
| IDN-02 | Renew a session without extending its original absolute lifetime, switch to a currently authorized organization, revoke on logout, and distinguish ambient-cookie refresh from explicit-token exchange. | adequate | partial | `g5.user.session-authority`, `g5.security.login.refresh-exchange`, `g5.security.current-authority` |
| IDN-03 | Keep account/profile operations person-session-only and consume an approved CLI device request at most once, including competing polls and issuance failure after consumption. | adequate | traced | `g5.security.credential-classes`, `g5.user.cli-authorization`, `g5.user.identity`, `g5.user.settings` |
| IDN-04 | Create an administrator-managed delegated API key, disclose it once, authenticate by stored hash, and reject expired/revoked credentials. | adequate | partial | `g5.user.api-key-authority`, `g5.security.credential-classes` |
| IDN-05 | Use verified tenant scope and current membership/permission rows for transaction-scoped admission, including expiry after lock waits and competing membership/grant writers. | adequate | partial | `g5.security.tenant.rls`, `g5.security.current-authority`, `g5.organization.membership`, `g5.organization.grant-lifecycle-question` |
| IDN-06 | Create a team with owner membership, dedicated docs repository and Base-workspace metadata in one transaction, then materialize repository content. | adequate | partial | `g5.team.onboarding`, `g5.organization.membership`, `g5.organization.tenancy` |
| IDN-07 | Issue, accept or revoke organization invitations against pending state, verified identity and expiry; distinguish login-triggered acceptance from immediate existing-user membership. | question | partial | `g5.organization.invites`, `g5.organization.invitation-authority`, `g5.organization.existing-user-invitation-question` |
| IDN-08 | Refuse membership removal or demotion to viewer while that person owns active workspaces; require explicit transfer or deletion before changing eligibility. | partial | traced | `g5.organization.admins`, `g5.organization.members`, `g5.workspace.transfer`, `g5.workspace.transfer.formal` |
| IDN-09 | Bind repository-provider connection consent to the person/provider, validate replacement credentials, encrypt stored tokens, and disconnect that provider independently of platform identity. | partial | partial | `g5.security.credential-classes`, `g5.security.encryption`, `g5.architecture.scm_provider_client`, `g5.security.tenant.scm-import` |
| IDN-10 | Resolve org/workspace/user launch settings and scoped provider accounts; preserve account identity and prevent deletion of an assigned account. | adequate | partial | `g5.chat.configuration`, `g5.chat.inheritance`, `g5.chat.accounts`, `g5.chat.account-continuity`, `g5.user.settings` |
| IDN-11 | Create session/task/initial input atomically; claim a caller request key before mutable persona resolution and return the same original result on equal retries. | partial | traced | `g5.chat.creation`, `g5.chat.frozen-attempt` |
| IDN-12 | Keep pending input editable/withdrawable only before reservation, enforce expected revision, and require an explicit owner decision before initial-input unknown-delivery redelivery. | adequate | partial | `g5.chat.authority`, `g5.chat.queue`, `g5.chat.uncertain-delivery`, `g5.chat.controls` |
| IDN-13 | Admit eligible queued turns with frozen launch choices, completed dependencies and shared agent/command capacity, preserving supplied launch confirmation until dispatch. | adequate | partial | `g5.chat.dispatch`, `g5.chat.frozen-attempt`, `g5.chat.confirmed-launch`, `g5.chat.dispatch-eligibility`, `g5.chat.execution-defaults`, `g5.chat.execution-headroom` |
| IDN-14 | Expose a Codex native authentication challenge only under current owner/runtime authority and keep readiness, private credential state and visible challenge status distinct. | adequate | partial | `g5.chat.authentication`, `g5.chat.codex-auth`, `g5.chat.device-challenge`, `g5.chat.failed-login`, `g5.chat.credential-home`, `g5.chat.credential-replacement` |
| IDN-15 | Apply task journal events in sequence with duplicate watermarks, reject stale attempt identity, and atomically project accepted user input plus assistant placeholder. | adequate | partial | `g5.chat.delivery`, `g5.chat.accepted-input`, `g5.chat.journal`, `g5.chat.replay`, `g5.chat.completed-output`, `g5.chat.settlement` |
| IDN-16 | Fork saved conversation history in order with new identities, destination-bound retained attachments and stopped incomplete assistant output, without inheriting queued or native continuation work. | adequate | partial | `g5.chat.fork`, `g5.chat.session-fork`, `g5.chat.persistence`, `g5.chat.authority` |
| IDN-17 | Materialize only current typed attachments after identity/metadata/hash/image validation, pass supported bytes through Claude/Codex native encodings, and fail before delivery on invalid input. | adequate | partial | `g5.chat.attachment-delivery`, `g5.chat.frozen-attempt`, `g5.chat.delivery` |
| IDN-18 | Reserve eligible running-Claude follow-up text without overtaking prior input, bind delivery generation and apply bounded-send disposition while retaining the ordinary queue fallback. | adequate | partial | `g5.conversation.delivery.steering`, `g5.conversation.delivery.steering-recovery`, `g5.conversation.providers.claude-followups`, `g5.chat.queue` |
| IDN-19 | Bind module invocation to catalog, execution identity and capabilities; deduplicate cancelled/settled identities, journal before effects and retain an exact outcome before releasing capacity. | adequate | traced | `g5.workspace.module-execution`, `g5.chat.module`, `g5.chat.runtime`, `g5.chat.incarnation` |
| IDN-20 | Reconcile recorded usage per tenant and query current provider-account quota observations while leaving workspace spend-enforcement scope explicit. | question | partial | `g5.chat.usage`, `g5.chat.spend-scope-question`, `g5.security.tenant.rls` |

## Proposed missing commitments

### IDN-P01: amendment

Owner: `g5.workspace.transfer.formal (with organization membership cross-reference)`. Priority: high. Confidence: high: explicit v9 policy plus service and schema counterpart.

Eligible-target checking during transfer does not govern removal or demotion of an existing owner. A replacement could implement transfer correctly yet silently reassign or cascade-delete on member removal.

Proposed text only: Removing membership or changing a member to viewer must refuse while they own any non-deleted workspace. Complete explicit transfers or deletions first; never choose a replacement owner or preserve ex-member compute authority.

Evidence: `docs/workspace-sharing-v9.md` 70–80; `docs/lean-backend-design-v9.md` 288–288; `backend/g5-api-core/src/services/identity/org_service.rs` 127–159, 865–887, 945–968; `backend/g5-api/src/db/migrations/003_p4_1_runtime_schema.sql` 110–142.

Limits: No production defect asserted. Schema concurrency was inspected, not executed.

### IDN-P02: new D specializing creation

Owner: `g5.chat.creation`. Priority: high. Confidence: high for retained design/current behavior; source design is agent-authored retained decision.

Atomic creation and frozen attempts do not prevent duplicate sessions after a lost response or mutable persona changes between equal retries.

Proposed text only: A chat start or chat-history fork submitted with a request key has one identity for the submitting person and organization. Equal retries return its original session, input and task; reuse with different target, content, attachments or launch choices is refused, and deleted results are not recreated. Resolve mutable persona instructions only for the original accepted request.

Evidence: `.g5/pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md` 65–81, 219–236; `.g5/ontology-plan-issue-231.md` 25–45; `backend/g5-api-core/src/services/agents/workspace_messages.rs` 139–460.

Limits: Proposed wording is a draft, not added ontology. This applies to request-keyed chat start/history-fork submission, not every workspace-fork API.

### IDN-P03: amendment candidate

Owner: `g5.security.credential-classes or a user SCM-account D referring to it`. Priority: medium. Confidence: medium: historical API design and current service agree; no newly recovered human request.

Credential classes and provider ownership separate authority and protocol, but leave connection lifecycle and its independence implicit.

Proposed text only: Repository-provider connections belong to the person and are independent of platform sign-in. Connecting or replacing a validated provider credential changes that provider connection; disconnect removes G5’s stored connection without deleting platform identity or organization memberships.

Evidence: `docs/reference/designs/API_AND_STORAGE.md` 2628–2645, 2737–2795; `backend/g5-api-core/src/services/identity/auth_service.rs` 2051–2167, 2188–2310.

Limits: Do not recover the older document’s OAuth/App transport, scopes or upstream revoke promise. Do not duplicate the existing SCM import/current-authority clauses. May be judged an already implied design boundary rather than a necessary new D after human contract review.

### IDN-P04: Q candidate; code-only admission policy requires authority recovery

Owner: `g5.user.login-proof (separate admission Q)`. Priority: medium. Confidence: high observed behavior; limited recovered intent authority.

Verified login identity and invitation acceptance do not say whether an instance allowlist may later lock out an admitted member.

Proposed text only: Should the configured email allowlist govern first admission while a current invitation or existing organization membership permits login outside it? Provisionally preserve those exceptions so an invited member can return after the invitation is consumed; distinguish lookup failure from an actual denial. Alternatives are an explicit allowlist override of membership or retirement of this gate.

Evidence: `backend/g5-api-core/src/services/identity/auth_service.rs` 852–954.

Limits: The implementation comment cites issue #1517 and prior auth_session_lifecycle B7/D5, but those original records were not recovered. No settled D or human intent is inferred solely from code.

## Per-trajectory evidence limits

### IDN-01

Inspected source: `backend/g5-api-core/src/services/identity/auth_service.rs` 746–1003, 1100–1175.

Browser-state consumption and verified-email ambiguity were read; provider implementations and complete callback route were not traced in this sample. Allowlist bypass for pending invitations and existing memberships is consequential observed policy, absent from these bodies. The comment cites issue #1517 and an old owner that were not recovered as original authority.

No exact formal property is claimed for this trajectory.

### IDN-02

Inspected source: `backend/g5-api-core/src/services/identity/auth_service.rs` 1216–1384, 1711–1745; `backend/g5-api-routes/src/routes/auth.rs` 1028–1127.

Current service/refresh route read; cookie creation in every login path and browser behavior not exercised. No runtime authentication test was executed.

No exact formal property is claimed for this trajectory.

### IDN-03

Inspected source: `backend/g5-api-routes/src/routes/auth.rs` 1212–1265; `backend/g5-api-routes/src/routes/users.rs` 55–134; `backend/g5-api-core/src/services/identity/auth_service.rs` 1480–1695; `backend/g5-api-core/src/services/identity/user_service.rs` 283–315, 386–455.

Traced scope is session extraction, membership/workspace check and consuming CAS through token issuance; fresh request creation and every account route are outside this row. Historical model does not establish current provider behavior or guarantee successful issuance after consumption. Public profile projection is distinct from own-account access.

Historical formal association: `CliAuthorization.OnlyHumanExchange` (`cli-safe`), `CliAuthorization.SingleExchange` (`cli-safe`), `CliAuthorization.ValidAdmission` (`cli-safe`).

### IDN-04

Inspected source: `backend/g5-api-core/src/services/identity/api_key_service.rs` 108–192, 218–326.

Key lifecycle service inspected; every admission consumer and immediate revocation behavior across in-flight requests were not enumerated. Current-authority enforcement is separately sampled in IDN-05.

No exact formal property is claimed for this trajectory.

### IDN-05

Inspected source: `backend/g5-authz/src/scoped_db.rs` 137–176, 401–450; `backend/g5-api-core/src/services/identity/permission_service.rs` 412–469; `backend/g5-api/src/db/migrations/042_scope_workspace_liveness_lock.sql` 35–115; `backend/g5-api-core/src/services/identity/org_service.rs` 820–980.

Adequate denotes the present current-authority contract, not resolution of the separately explicit grant-lifecycle Q. Inspected SQL locks membership, matching defaults/grants/overrides and workspace liveness; not every endpoint consumer or every alternate writer was retraced. Historical model coverage remains bounded to its binding set.

Historical formal association: `CurrentAuthority.AuthorityAtCommit` (`authority-role`), `CurrentAuthority.CurrentAtAdmission` (`authority-expiring`).

### IDN-06

Inspected source: `backend/g5-api-core/src/services/identity/org_service.rs` 236–475; `backend/g5-api-routes/src/routes/orgs.rs` 571–625.

Atomic metadata creation is covered. The post-commit Git materialization/failure/retry chain is not complete in these ranges. Self-hosted single-organization admission appears in the route; deployment/license policy is coordinator scope.

No exact formal property is claimed for this trajectory.

### IDN-07

Inspected source: `backend/g5-api-core/src/services/identity/org_service.rs` 640–735, 1037–1158, 1267–1303; `backend/g5-api-core/src/services/identity/auth_service.rs` 1100–1175; `backend/g5-api-routes/src/routes/orgs.rs` 728–790.

Existing-user immediate membership remains the already-owned Q; no new duplicate proposal. Current service acceptance/revocation and callback continuation inspected; full callback invitation selection and all external notification effects not traced.

No exact formal property is claimed for this trajectory.

### IDN-08

Inspected source: `backend/g5-api-core/src/services/identity/org_service.rs` 127–159, 820–980; `backend/g5-api-routes/src/routes/orgs.rs` 792–860; `backend/g5-api/src/db/migrations/003_p4_1_runtime_schema.sql` 110–142.

Service checks, route transaction and generated eligible-membership/active-owner foreign key were inspected. No concurrency runtime test executed. Transfer intent requires an eligible target but does not specify disposition when membership of the current owner is removed or demoted.

No exact formal property is claimed for this trajectory.

### IDN-09

Inspected source: `backend/g5-api-core/src/services/identity/auth_service.rs` 2051–2167, 2188–2310.

The provider-owner block separates protocol authority from login identity but does not state the user-visible independence of repository connection lifecycle. Original API design is historical and includes superseded OAuth/App details. Only connection independence is proposed; neither those transport choices nor remote-token revocation is recovered as current intent. SCM API route wrappers and token refresh/import consumers were not completely traced.

No exact formal property is claimed for this trajectory.

### IDN-10

Inspected source: `backend/g5-api-core/src/services/agents/config_service.rs` 581–659; `backend/g5-api-core/src/services/agents/provider_account_service.rs` 383–432, 484–532, 668–705.

Preview configuration reads and account update/delete/assignment checks inspected; every persisted configuration writer and credential-encryption operation not freshly traced. Historical account model isolates writer lock/identity constraints, not the entire resolver or provider authentication.

Historical formal association: `AccountAssignments.AssignmentSurvives` (`account-org`), `AccountAssignments.AssignmentCompatible` (`account-workspace`), `AccountAssignments.IdentityStable` (`account-user`).

### IDN-11

Inspected source: `backend/g5-api-core/src/services/agents/workspace_messages.rs` 139–460; `backend/g5-api-routes/src/routes/chat.rs` 1222–1246, 1280–1334.

Current key claim binds org/actor, workspace and request digest; unequal reuse conflicts and deleted original results are not recreated. The creation body promises one transaction; frozen-attempt begins at reservation. Neither promises caller request identity across new-session retries. Current NewSession and ForkSession creation branches are inspected; complete fork internals, all attachment upload stages and persona Git access remain outside the traced slice.

No exact formal property is claimed for this trajectory.

### IDN-12

Inspected source: `backend/g5-api-core/src/services/agents/agent_dispatch.rs` 318–465, 495–609.

Current service transitions and owner/session locks inspected; every route, attempt-loss producer and continuation variant not followed. Steering has an explicit separate accepted duplicate-risk exception, so that behavior is not a contradiction.

No exact formal property is claimed for this trajectory.

### IDN-13

Inspected source: `backend/g5-api-core/src/services/agents/agent_dispatch.rs` 857–898, 1250–1335, 1410–1455, 1500–1535; `backend/g5-api-core/src/services/agents/workspace_messages.rs` 259–312; `backend/g5-api-core/src/services/agents/config_service.rs` 581–659.

Existing CI-LAUNCH-CONFIRMATION is an implementation/source-bound divergence, not missing ontology: enqueue persists launch.preferences while confirmation is absent from that handoff. Local resolver model cannot establish queued dispatch conformance. Queued model preserves submitted confirmation as specification/ghost evidence, not claimed production storage. Current source inspected without TLC/runtime rerun; whole catalog/default configuration were not freshly read in this sample. Fair scheduling and all command producers remain outside the bounded property association.

Historical formal association: `LaunchAgreement.ConfirmationHonored` (`launch-confirmed`), `LaunchAgreement.ResolvedChoiceRetained`, `QueuedLaunchAgreement.ExecutionHonorsConfirmation` (`queued-launch-asbuilt`), `ExecutionCapacity.DefaultSixteen`, `ExecutionCapacity.ApiCapacity`, `ExecutionCapacity.VmCapacity`, `ExecutionCapacity.OperationHeadroom`.

### IDN-14

Inspected source: `backend/g5-api-core/src/services/agents/codex_auth.rs` 32–244.

API orchestration and state admission inspected; provider process implementation, terminal/browser rendering and all challenge-expiry/replacement races were not traced.

No exact formal property is claimed for this trajectory.

### IDN-15

Inspected source: `backend/g5-api-core/src/services/agents/task_lifecycle.rs` 1876–1965, 2647–2735; `backend/g5-api-core/src/services/agents/chat_service.rs` 114–198.

Current attempt lock, sequence/duplicate branches and input-acceptance projection were inspected. Complete output-unit, provider-event validation, terminal settlement, reconnect consumers and replay drain were not traced. No whole transcript exactly-once claim.

No exact formal property is claimed for this trajectory.

### IDN-16

Inspected source: `backend/g5-api-core/src/services/agents/chat_service.rs` 2320–2405, 2523–2675.

Selected fork/session helpers and copy SQL were read; the middle of the fork flow, all selection filters, callers and search/read routes were not fully inspected. No model property is associated merely from the feature name.

No exact formal property is claimed for this trajectory.

### IDN-17

Inspected source: `backend/g5d-agent/src/runtime.rs` 650–695, 2507–2578, 2665–2689, 2720–2743.

Materializer, prelaunch failure and both encoding call sites read; internal content endpoint authorization and complete adapter acceptance paths not freshly inspected. Historical finite model covers identity/integrity/complete-set predicates, not cryptographic correctness, image decoder behavior or real provider acceptance.

Historical formal association: `AttachmentDelivery.VerifiedContent` (`attachments-safe`), `AttachmentDelivery.CompleteCurrentContent` (`attachments-safe`), `AttachmentDelivery.FailureUndelivered` (`attachments-safe`).

### IDN-18

Inspected source: `backend/g5-api-core/src/services/agents/agent_steering.rs` 67–247; `backend/g5-api-core/src/services/agents/agent_dispatch.rs` 357–434.

Current backend admission/reservation/disposition slice inspected. Native stdin serialization/closing, event attribution and output-budget reserve are not verified by this slice. The newer ontology explicitly accepts ambiguous-send duplicate risk; no missing-intent proposal or historical AttachmentDelivery proof is attributed to steering.

No exact formal property is claimed for this trajectory.

### IDN-19

Inspected source: `backend/g5d-dispatch/src/modules/mod.rs` 515–605, 610–681, 813–860, 954–1007.

Traced boundary is dispatcher invoke admission/cancel/settlement/capacity release, not subprocess drain or output-overflow implementation. VmCapacity is only a bounded counter association. It does not prove catalog validation, durable journal semantics, cancellation fencing or retained-outcome correctness.

Historical formal association: `ExecutionCapacity.VmCapacity`.

### IDN-20

Inspected source: `backend/g5-api-core/src/services/identity/usage_aggregation.rs` 146–280; `backend/g5-api-core/src/services/identity/provider_quota_service.rs` 54–112, 165–202.

Existing spend-scope Q already owns retained accounting versus deferred/retired threshold enforcement; no duplicate mandate proposed. Aggregation excludes some job statuses and quota uses latest captured observations; original product authority for their full semantics and all producers/admission consumers not recovered. Inspected maintenance/scoped update and account query do not establish complete billable accounting or provider quota freshness.

No exact formal property is claimed for this trajectory.

## Historical formal evidence

The sampled identity receipt records baseline `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`. Its domain source binding set includes 21 files; **8 now have different file hashes**. This is a freshness diagnostic, not a TLC rerun or claim that each changed line affects every property. All associations above are historical bounded property links. Current source traceability is separate.

- `backend/g5-api-core/src/services/identity/auth_service.rs`
- `backend/g5-api-routes/src/routes/auth.rs`
- `backend/g5d-agent/src/runtime.rs`
- `backend/g5-api-routes/src/routes/internal.rs`
- `backend/g5-api-core/src/services/agents/agent_dispatch.rs`
- `backend/g5d-dispatch/modules/catalog.json`
- `backend/g5d-dispatch/src/modules/mod.rs`
- `backend/g5-api-core/src/services/agents/chat_service.rs`

The local LaunchAgreement resolver and the queued confirmation model cover different boundaries. ExecutionCapacity covers counters only, not all dispatcher semantics. AccountAssignments covers its specific account/assignment lock contract. AttachmentDelivery does not model native steering. Existing source-bound CI-LAUNCH-CONFIRMATION remains enforcement evidence under an adequate canonical commitment.

## Exact current-pass read ledger

Ranges below were directly read, including pre-compaction inspection in this same pass. Search hits were used for discovery and are not counted as whole-file inspection. Source overlap with a named symbol is a reviewed association only; it does not establish that every branch of that symbol was audited.

- `backend/g5-api-core/src/services/identity/auth_service.rs` 746–1003, 1100–1175, 1216–1384, 1480–1695, 1711–1745, 2051–2167, 2188–2310
- `backend/g5-api-routes/src/routes/auth.rs` 1028–1127, 1212–1265
- `backend/g5-api-routes/src/routes/users.rs` 55–134
- `backend/g5-api-core/src/services/identity/user_service.rs` 283–315, 386–455
- `backend/g5-api-core/src/services/identity/api_key_service.rs` 108–192, 218–326
- `backend/g5-api-core/src/services/identity/permission_service.rs` 412–469
- `backend/g5-authz/src/scoped_db.rs` 137–176, 401–450
- `backend/g5-api-core/src/services/identity/org_service.rs` 127–170, 236–475, 640–735, 820–980, 1037–1158, 1267–1303
- `backend/g5-api-routes/src/routes/orgs.rs` 571–625, 728–790, 792–860
- `backend/g5-api/src/db/migrations/003_p4_1_runtime_schema.sql` 110–142
- `backend/g5-api/src/db/migrations/042_scope_workspace_liveness_lock.sql` 35–115
- `backend/g5-api-core/src/services/agents/workspace_messages.rs` 139–460
- `backend/g5-api-core/src/services/agents/agent_dispatch.rs` 318–465, 495–609, 857–898, 1250–1335, 1410–1455, 1500–1535
- `backend/g5-api-core/src/services/agents/config_service.rs` 581–659
- `backend/g5-api-core/src/services/agents/provider_account_service.rs` 383–432, 484–532, 668–705
- `backend/g5-api-core/src/services/agents/codex_auth.rs` 32–244
- `backend/g5-api-core/src/services/agents/chat_service.rs` 114–198, 2320–2405, 2523–2675
- `backend/g5-api-core/src/services/agents/task_lifecycle.rs` 1876–1965, 2647–2735
- `backend/g5-api-core/src/services/agents/agent_steering.rs` 67–247
- `backend/g5-api-routes/src/routes/chat.rs` 1222–1246, 1280–1334
- `backend/g5d-dispatch/src/modules/mod.rs` 515–605, 610–681, 813–860, 954–1007
- `backend/g5d-agent/src/runtime.rs` 650–695, 2507–2578, 2665–2689, 2720–2743
- `backend/g5-api-core/src/services/identity/usage_aggregation.rs` 146–280
- `backend/g5-api-core/src/services/identity/provider_quota_service.rs` 54–112, 165–202
- `.g5/ontology/features/users.g5.md` 1–61
- `.g5/ontology/features/organizations.g5.md` 1–84
- `.g5/ontology/features/chat.g5.md` 1–337
- `.g5/ontology/concepts/security.g5.md` 1–52
- `.g5/ontology/features/workspaces.g5.md` 38–52, 145–155
- `.g5/ontology/software_architecture/workspace_vm.g5.md` 204–235
- `.g5/ontology/software_architecture/git.g5.md` 260–270, 311–321
- `docs/workspace-sharing-v9.md` 70–80
- `docs/lean-backend-design-v9.md` 288–288
- `docs/reference/designs/API_AND_STORAGE.md` 2628–2645, 2737–2795
- `.g5/pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md` 65–81, 219–236
- `.g5/ontology-plan-issue-231.md` 25–45

## Consequential areas not completed in this sample

- Notification producers, preferences, acknowledgement consumers and external delivery retry chain (existing owner exists; not sampled here).
- SCM token refresh, provider revocation and every SCM discovery/import consumer.
- All permission-grant writers and every API-key/role admission consumer; existing grant-lifecycle Q unresolved.
- Full session GC/deletion, transcript search/read projection, all attachment upload and authorization consumers.
- Native question restore/answer lifecycle, provider credential home materialization and transfer across stopped compute.
- Complete native steering stdin/worker closing, output budgeting and segment attribution (current ontology owners exist).
- All background task/service lifecycle and scheduler fairness/recovery variants.
- Usage billing versus display lineage and all quota polling producers; existing spend scope remains Q.
- Organization/user erasure and durable tenant offboarding; no new deletion guarantee inferred here.
- Approval/review and live workspace/browser-ticket authority are owned by other audit domains.

## Rejected apparent gaps

- Queued launch confirmation, provider-account continuity, typed attachment transport and 16-slot/headroom intent already have canonical owners; do not propose duplicates.
- The deliberate ambiguous-steering duplicate risk is explicitly stated; it is not the initial-input unknown-delivery gap.
- Viewer grant and existing-user invitation contradictions already have Q owners.
- Invocation counts/model counts do not establish enforcement or behavior coverage.
- Legacy SCM OAuth transport, upstream revoke, spend thresholds and a permanent additive grant union are not silently promoted.
