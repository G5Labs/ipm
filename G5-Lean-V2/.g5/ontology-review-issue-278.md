# Issue 278 review: approval administration and decisions without workspace ownership

**Mode:** self-review by the implementing session, using three independent read-only discovery reviews in parallel fresh subagents plus one independent security audit, followed by main-agent consolidation and one repair. The change is substantial: it relaxes authority on two approval routes and changes a shared approval contract. Raw reports are preserved unchanged: [r1](ontology-review-issue-278.r1.md), [r2](ontology-review-issue-278.r2.md), [r3](ontology-review-issue-278.r3.md), [security](ontology-review-issue-278.security.md).

**Inputs:** [review inputs](ontology-review-issue-278.inputs.md) (issue #278, human decisions H1–H4 made while PR #280 was open, reconstructed ontology-change section), `git diff bce87811..050af5f1` on `fix/approval-requirement-org-admin`.

**Handoff gap:** no design document was saved before implementation. The `Ontology changes and governance` section in the inputs is a later reconstruction from the request, the human decisions and the actual ontology diff, not an original handoff. Reviewer r3 confirmed the recorded ontology diff matches the actual range.

## Initial consolidated findings (before repair)

All reviewers agreed the issue #278 fix (org-admin attach/retire) and the H1 fix (addressed non-owner settles their own request) are correct and that their tests discriminate against the pre-PR guards. Checks before repair: `api_workspace_approvals` 15/15, `route_auth_static` 7/7, `api_org_admin_authority` 18/18 (r1, r2 on throwaway Postgres); probe tests in r1 and r2.

| # | Finding | Sources | Kind | Governing rule |
|---|---|---|---|---|
| C1 | After an addressed non-owner settles a request, the chain's successor pending request stores the decider as `requested_by` (`approval_config_service.rs` successor insert binds the actor). The owner then gets 404 cancelling it (not the requester) and the approver 403 (cancel is owner-only); re-requesting reuses the row. Unreachable before H1 because only the owner could settle. Confirmed by r2 probe A. | r2 F1, security SEC-1 | Defect (high, runtime-confirmed) | Requester-only cancellation; `g5.architecture.policy` (equivalent effects for reused operations) |
| C2 | The new requestless sentence in `g5.governance.approval` claims every decision without a pending request is owner-only. Review-verdict sync (`review_approval_sync.rs`) records such decisions for a listed non-owner reviewer through the service, existing tested behavior. Footers in `git_governance` and principles repeat the over-broad claim. H2 covered only requestless submissions to the decisions endpoint. | r2 F2/F3, r3 F1 | Ontology defect + human question | `g5.governance.approval`; H2 |
| C3 | Attach/retire resolve `org:admin` with the workspace id, so a legacy workspace-scoped `org:admin` override plus an `org:admin` token from another org lets a plain member attach (200, probe P2). Contradicts "administrator of the workspace's organization"; ownership blocked this before the PR. The service has no authority check of its own. | r1 F1, r3 F6 | Defect (low, conditional, runtime-confirmed) | `g5.governance.approval.requirements`; `g5.security.owner.retirement` |
| C4 | The Approvals UI (and `callerCanDecide`/home actionable list) offers Approve/Reject/Withdraw to a listed non-owner with no pending request; the requestless submission then returns 403. Pre-existing, but H2 makes it the explicit contract. | r1 F2, r2 F4, r3 F4 | Defect (UI, low–medium, runtime-confirmed) | `g5.governance.approval` requestless exception; offer only permitted actions |
| C5 | No permanent test that an addressed human without current read access is refused (probe-verified 403, no write), nor for non-owner reject and Withdraw through the request. | r1 F3/F4, r2 F5, r3 F2/F3 | Missing evidence | `g5.governance.approval` ("current read access"), `g5.security.read`; H1 |
| C6 | `/decisions` is scope-exempt at the route and the DB check ignores token claims, so a narrow API key of an addressed human can record a human approval. Pre-existing for owners; the PR widens it to addressed non-owners with a `workspaces:read` floor. | security SEC-2, r3 F10 | Hardening (low, pre-existing, widened) | `g5.governance.approval` (human authority) |
| C7 | Ontology form: requestless exception mixed into `g5.governance.approval` (human/agent provenance mixed, tags and lineage split across bullets); `approval.requirements` tagged human-provided but adds "sharing"; its footer narrates mechanism and claims Playwright coverage without a run; status footers restate the approval rules instead of linking (also CodeRabbit). | r1 F5/F6, r2 F6/F7/F8, r3 F8, CodeRabbit | Nit (ontology) | Authoring format |
| C8 | E2E: owner-id assertion passes vacuously when the field is missing; `finally` does not retire the Base requirement on mid-test failure; the flow leaves a permanent extra org admin in the shared test org. | r1 F9, r2 F9, r3 F7, security SEC-6 | Nit (test hygiene) | — |
| C9 | Dev OAuth stub: a repeated `email`/`login` query parameter crashes the stub (introduced); email comparison is case-sensitive; changing only email keeps the default login; consent page reflects `scope`/`redirect_uri` unescaped and the callback redirects anywhere (pre-existing, dev/CI only). | security SEC-4/SEC-5, r1 F10, r2 F9, r3 F9 | Defect (dev-only, low) + pre-existing | H3 |
| C10 | A pending ownership transfer no longer blocks attach/retire or request-addressed decisions; liveness is still locked. | r1 F7, security | Design question (accept: durable records, not live-workspace mutation) | `g5.security.admission` |
| C11 | Route doc and `/decisions` exemption rationale describe only the request branch. | r2 F10, r3 F5 | Nit | — |
| C12 | Any org admin can retire a Base requirement, merge and re-attach without notification; attributable in append-only history and audit log. | security SEC-3 | Info (intended by #278) | `g5.governance.approval.requirements` |
| C13 | Member/other-tenant denials in the new API test stop at middleware or lookup, not the changed handler; only `sec_h02_every_admin_route_rejects_other_org_authority` exercises the handler's own admin check. | r1 F8 | Missing evidence note | — |

Uncovered boundaries: the Playwright flow, the OAuth stub consent page and the review-sync path were not executed by any reviewer.

## Human decision during consolidation

C2 went to the human, who chose to say nothing about requestless (unsolicited) decisions, deferring them to future approver work, and to state only the positive rule that a reviewer with a valid pending request may approve or reject. The human also chose to require `workspaces:read` on the decisions route token (C6).

## Dispositions, repairs and rechecks

| # | Disposition | Repair and recheck |
|---|---|---|
| C1 | **Fixed.** | The successor insert copies `requested_by` from the settled row. `addressed_nonowner_decides_while_requester_keeps_the_chain`: a non-owner rejects, then withdraws through the successor; every successor stays the owner's, the reviewer's cancel gets 403 and the owner's gets 200. It fails against the pre-repair service. |
| C2 | **Fixed (amended per the human).** | The requestless sentence was removed. New block `g5.governance.approval.request` holds the positive rule. The `g5.governance.approval` footer records requestless and unsolicited decisions as deferred, and notes that the endpoint keeps its legacy owner guard. Footers that restated rules now link to the rule. Review-round verdict sync is unchanged. |
| C3 | **Fixed.** | `ApprovalConfigService::{attach, retire}` call `PermissionService::require_org_admin_on_conn` (org level, no workspace grants). `workspace_scoped_org_admin_grant_cannot_administer_requirements` (the P2 shape) gets 403 on attach and retire with no new event; it fails against the pre-repair service. This also settles C13: the handler-level admin check now has its own discriminating test. |
| C4 | **Fixed in the tab; remainder recorded.** | Approve, Reject and Withdraw open only with the caller's `requestId` or workspace ownership. The new `WorkspaceApprovalsPage.test.tsx` case fails without the gate. The status band tint and home inbox still read `callerCanDecide`, which review-round sync shares; this is recorded as a gap in `g5.governance.approval.request`. |
| C5 | **Fixed.** | Added `addressed_human_needs_current_read_access` (invite-only: 403 and no write, then 200 after invitation), plus non-owner reject and withdraw in the chain test. |
| C6 | **Fixed (human chose).** | `/decisions` is now an audited route requiring `Scope::WorkspacesRead`, with the static contract updated. A `jobs:read`-only token gets 403 in the chain test. API-key approval eligibility in general is left to P6. The cancel route's equivalent pattern is pre-existing and unchanged. |
| C7 | **Fixed.** | "Sharing" was removed from the human-provided block text (the tests still check it). Footers carry implementation and evidence without narrating mechanism. Tags and lineage are single bullets. The browser flow is marked as awaiting its first CI run. The new blocks are linked from the formal-decision and merge stories and from the git/governance grants list. |
| C8 | **Fixed (partly).** | The owner assertion now requires a present owner. `finally` retires the requirement idempotently. The permanent extra org admin stays: CI provisions a fresh database each run. |
| C9 | **Fixed (introduced items); pre-existing items partly fixed.** | Repeated parameters use their first value. The email comparison ignores case. Changing only the email derives the login from it. Consent-page values are HTML-escaped, and the callback redirects only to the code's bound `redirect_uri` (400 for an unknown code). Rechecked on a private port. The authorize endpoint still accepts any `redirect_uri` (pre-existing, dev/CI only). |
| C10 | **Accepted.** | Approval configuration and decisions are durable records, not live-workspace mutations, so admitting them during a transfer is consistent with `g5.security.admission`; liveness is still locked. |
| C11 | **Fixed.** | The route docs and the static contract describe both branches. |
| C12 | **Accepted (intended).** | Notification of requirement changes is not requested; history and audit remain. |

Rechecks after repair, run against the test Postgres this session uses:
- `api_workspace_approvals` 18/18, `route_auth_static` 7/7, `api_org_approvals` 8/8, `api_org_admin_authority` 18/18, `api_workspace_reviews` 16/16, `api_agent_approval_evaluation` 4/4, `approval_request_retention` 4/4, `api_home` 13/13 (serial), `approval_config` unit tests 7/7.
- Clippy is clean on `g5-api-core` and `g5-api-routes`.
- vitest: `WorkspaceApprovalsPage` 40/40 and `WorkspaceStatusBand` 79/79. eslint, prettier and tsc are clean.

Not executed: the Playwright `org-approvals.spec.ts` flow, which awaits CI.

## Commitment ledger after repair

- `g5.governance.approval.requirements` → `workspace_approvals.rs::{attach,retire}_requirement` → `ApprovalConfigService::{attach,retire}` with an org-level admin check → non-owner admin with an `org:admin`-only token (200), member and other tenant (403/404), workspace-scoped grant (403), and owner-only content, sharing and VM probes (403) → **implemented; API-tested; manually verified; browser flow pending CI.**
- `g5.governance.approval.request` → `record_decision` request branch (`workspaces:read` route and token) → `record_human_decision` exact-request lock, with the successor keeping its requester → non-owner approve, reject and withdraw (200); another actor's request (409); no read access (403, no write); narrow token (403); requester cancel (200) → **implemented; API- and UI-tested; manually verified. Gap: band and home tint.**
- `g5.governance.approval` → unchanged rule; requestless and unsolicited decisions deferred → **no new commitment; legacy owner guard on requestless submissions is retained.**
- `g5.governance.approval.version` → the request branch records action-time `reviewed_git_heads` → `explicit_human_request_admission_and_settlement` → **pass.**
- `g5.security.owner`, `g5.security.read`, `g5.security.read.sharing` → the requirement administrator and the addressed reviewer gain no owner-only content, sharing or VM authority; read access is enforced → **preserved; tested.**
- `g5.security.admission` → transfer and liveness behavior (C10) → **accepted by inspection.**
- `g5.architecture.policy` (equivalent effects across callers) → the request chain's requester is now preserved for every decider → **fixed (C1).**

## Ontology changes and governance (final)

Changed blocks in the final PR:
- **New:** `g5.governance.approval.requirements` (#278) and `g5.governance.approval.request` (H1 and the consolidation decision).
- **`g5.governance.approval`:** the body is unchanged from main. Its footer was reassessed.
- **Footers reassessed:** `g5.story.record-a-formal-review-decision` and `g5.story.stale-approval`; the git_governance approval footer; two system-design-principle footers.
- **Story governing-contract links:** formal decision and merge.
- **Grants list:** the git_governance list of distinct grants.

An interim requestless owner-only sentence was added during implementation and removed at consolidation by the human's decision. It does not appear in the final diff.
