# Identity quantitative refinement

Baseline: `312b09609d76d539de502e5eb68f787fa37c96ef`. Adopted additions are [keyed creation](../../../ontology/features/chat.g5.md#g5.chat.creation.idempotency) and the unresolved [admission-continuity question](../../../ontology/features/users.g5.md#g5.user.admission-continuity-question). The coordinator integrated the transfer amendment below in the shared workspace owner. No production repairs or runtime tests were performed. Fresh bounded evidence is in the [identity campaign](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/identity/README.md); the earlier formal suite was not restored.

## Applied ownership amendment

The coordinator appended the membership-disposition commitment to `g5.workspace.transfer.formal`:

> Removing membership or demoting a member to viewer must refuse while they own a nondeleted workspace. Complete explicit transfers or deletions first; membership changes never choose a replacement owner.

The recovered authority is [workspace-sharing v9](../../../../docs/workspace-sharing-v9.md), lines70–80 (especially76), and [backend v9](../../../../docs/lean-backend-design-v9.md), line288. Preserve the existing Ben ownership/transfer statements. The canonical metadata links these sources and this report: `OwnerMembership` traces the service preflight, generated eligible-owner foreign key, transfer completion and competing membership mutations. Foreign-key/key-lock semantics are an implementation premise, not tested PostgreSQL evidence. Physical VM stopping remains the existing transfer owner's concern. The integrated wording avoids repeating that existing authority in the new membership clause.

## Incremental findings and minimal repairs

### CI-KEYED-FORK — ordinary history fork ignores accepted request key

[The route](../../../../backend/g5-api-routes/src/routes/chat.rs)412–430 accepts `request_key` with a retry-returning-first-fork promise. The persona branch2097–2187 uses the shared key claim; the non-persona branch2188–2197 calls `fork_session` directly without forwarding the key. Equal ordinary history-fork requests can therefore create distinct sessions. This extends the original IDN-11 source inspection rather than relabeling a missing intent as a tested implementation.

`key-ordinary-fork-asbuilt` reaches two committed sessions for the same submitted identity and fails `UniqueCreation`; `key-ordinary-fork-intended` passes with a claim and a **session-only** result. The general uniqueness-lock negative control is separate from this actual bypass. Minimal repair proposal: extend/reuse the existing request-identity owner for keyed ordinary history-fork creation, preserving its no-input/no-task semantics, or explicitly refuse an unsupported key. Do not simply call a send path that always enqueues input, and do not introduce a parallel dedupe ledger. This is source-bound, not a runtime reproduction.

### CI-KEY-DELETION-CONDITIONAL — key fence follows deleted child by cascade

[Migration032](../../../../backend/g5-api/src/db/migrations/032_workspace_messages.sql)1–51 gives `workspace_message_requests` session/input foreign keys `ON DELETE CASCADE`, while [claim_key](../../../../backend/g5-api-core/src/services/agents/workspace_messages.rs)363–371 describes a retained-key Gone response after result deletion. Physical child deletion therefore removes the claim and would allow a subsequent admissible request to recreate it. No standalone production chat-session/input deletion caller was found in the bounded search; current workspace deletion is soft and separately denies admission. This is a conditional schema limit, not an established reachable API defect.

`key-delete-schema-conditional` reaches commit→reply→physical delete→new claim→second commit and fails `UniqueCreation`. `key-delete-intended` preserves the fence and passes; `key-delete-refusal` reaches an explicit Gone-equivalent refusal. Minimal conditional repair: retain a scoped digest/result tombstone while a request is retryable, then explicitly expire/refuse retired identities. Account/tenant erasure and key-retention duration need their own policy; no indefinite retention mandate is inferred.

### Existing CI-G1 remains separate

OAuth auto-accept reads pending invitations without locking before grant ([auth_service](../../../../backend/g5-api-core/src/services/identity/auth_service.rs)1069–1118); revocation can complete between read and grant ([org_service](../../../../backend/g5-api-core/src/services/identity/org_service.rs)1267–1296). `invitation-race-asbuilt` reaches capture→revocation→membership grant and fails `NoRevokedGrant`; `invitation-current-row-intended` passes with shared current-row exclusion. This is an existing tracked gap with `g5.organization.invitation-authority` as owner. Minimal repair remains to serialize/current-check grant against revocation, not weaken the invitation contract or treat the allowlist Q as permission for stale grants.

### No new membership/transfer defect

`OwnerMembership` separates mutable membership preflight, row/key locks, the transfer target read on a separate connection, deferred foreign-key validation and commit. A concurrent new-workspace insertion is an alternate writer. Safety passes under the inspected generated-key foreign-key premise; disabling that serialization exposes the stale-read failure. The corrected witness specifically observes no ownership at preflight, then a competing creation commits, then the membership constraint refuses. Another witness reaches initial owner-removal refusal→stopped explicit transfer→retried removal commit. Explicit deletion frees the same removed owner in the deletion witness. These results support the amendment but do not execute PostgreSQL or prove physical compute shutdown.

## Original quantitative row dispositions

These are dispositions of the original [20-row identity inventory](../quantitative/identity.json), not replacements for its pinned historical audit. Rows group unequal-sized trajectories, so counts below are not semantic or path-coverage percentages.

| Original row | Intent before → after | Enforcement disposition and reason |
| --- | --- | --- |
| **IDN-01** | partial → **question** | Partial remains: proof has existing owners; allowlist/member continuity now has an explicit provisional Q. Original issue 1517/lifecycle policy authority remains unrecovered; conditional models do not settle it. |
| **IDN-08** | partial → **adequate** | Previously traced bounded service/schema path remains traced, with PostgreSQL serialization and broader runtime lifecycle as named premises; abstract checking does not widen that scope. |
| **IDN-11** | partial → **adequate** | Enforcement **traced → partial** for the now-expanded route scope: shared keyed start/persona-fork path was traced, but ordinary-fork bypass and the conditional deletion fence prevent a complete enforcement claim. The new D states the commitment despite those gaps. |
| **IDN-09** | partial → partial | SCM connection independence remains a secondary candidate. Existing credential-class/provider authority may already imply it; no new reconstruction counterexample justifies another D. This is retained audit uncertainty, not a settled missing requirement. |
| **IDN-07** | question → question | Existing-user invitation acceptance/grant authority still has its existing Q; admission-continuity modeling does not resolve it. |
| **IDN-20** | question → question | Accounting versus spend-enforcement scope retains its existing Q; no spend mandate or threshold was adopted. |

IDN-02–06, IDN-10, IDN-12–19 retain adequate intent status and their original enforcement limitations. In particular, IDN-16's general history-fork intent remains adequate while keyed retry enforcement is owned by IDN-11; IDN-13's launch-confirmation implementation gap remains separate. No historical model association is revived by these fresh models.

With that integration, this **same selected 20-row inventory** has 16 adequate, one partial and three question intent dispositions; enforcement is three traced and 17 partial after the IDN-11 scope correction. This is a disposition count only. The independently sampled denominator is unchanged, high-risk unsampled paths remain unsampled, and source ranges do not imply full-file or every-symbol coverage.

## Executed evidence and review limits

The final local run and saved-receipt verification matched **39/39 expected outcomes**: 11 safety, 10 controls, 14 completed-goal/recovery witnesses and 4 divergences. The coordinator refreshed the same stable formal inputs under the final shared runner, and a fresh `--verify` confirmed all 39 saved receipts; [manifest](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/identity/manifest.json), [bindings](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/identity/bindings.json) and receipts are authoritative for exact hashes. The initial standard-module parser error was a tool failure and was never counted as a result; an isolated retry and the complete rerun succeeded. An explicit false proof was not inferred from it.

Trace review tightened witness predicates before the final run: rollback must interrupt staged creation and reach that caller's committed result; competing replay records the actual waiting/replaying caller; lost-response recovery requires loss→configuration edit→replay; stale preflight refusal must be the membership constraint's refusal, not an unrelated creator failure; and deletion must free the member subsequently removed. The successful traces were read after those corrections. No fairness/eventual-success guarantee is claimed. Full action/source/property correspondence, finite bounds and unmodeled consumers are in the campaign README.

The provisional allowlist option, strict-allowlist alternative and gate-retirement alternative remain explicitly conditional. The strict case's `RecommendedContinuity` violation describes a policy difference, not a production defect. CI-G1 is existing; CI-KEYED-FORK is new source-bound evidence; CI-KEY-DELETION-CONDITIONAL is deliberately conditional. These categories must stay distinct in shared coverage and gap records.

Read-only independent platform review inspected both model bodies, configurations/bindings, all witness/divergence action traces, Home capture/materialization 158–434, HomePage 936–1028, badges 180–220, explainer service 8383–8592 and native files 574–679. No material false witness was found. The coordinator corrected the native-check line reference and documented permitted stuttering plus unmodeled per-workspace grouping. This cross-review makes no claim to audit all platform behavior.

The runtime-domain agent independently reviewed all three identity model bodies, all 39 configuration values/predicates, manifest/README/bindings, canonical additions and direct source samples for the claim/digest, ordinary fork, generated owner foreign key, transfer/member mutations, fork writer and invitation admission/grant/revocation. No concrete blocker remained within the documented scope. The review specifically checked witness ordering/actor identity, competing-owner exclusion and conditional deletion/policy classifications; current-row grant repair, expiry and unmodeled writers retain their stated limits.
