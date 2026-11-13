# Ontology coverage audit: workspace, Git, documents

Baseline: `/Users/ziniuw/Desktop/projects/G5-Lean`, HEAD `2d3bb3c586e0423d04bb2565a946f39602ba3f2b`; clean checkout at inspection. Read-only audit; only this report was written. No application/ontology/skill changes, tests, models, commits, provider calls or runtime probes. Applicable local uplift and authoring skills and their execution/source/format/structure references were read. `.g5/local/state.json` plus local G5 skill were not both present.

The question was whether a replacement can obey the **current ontology bodies** yet omit important evidenced behavior. A source implementation detail, a known conformance defect, or an unexecuted test is not counted as missing intent. Earlier source statements were compared with later corrections; in particular, review-copy merge prohibitions and old durable-flush browsing policies were not restored over the newly supplied ontology.

## Scope and directly inspected evidence

Canonical bodies read first: `features/workspaces.g5.md`, `documents.g5.md`, `repositories.g5.md`, `issues.g5.md`, `organizations.g5.md`, `users.g5.md`, `ui.g5.md`, `ontology_and_harness.g5.md`, `software_architecture/git.g5.md`, `workspace_vm.g5.md`, all concept files and root. Other bodies were searched for material review/read/authority/policy/source clauses, including chat and host owners. Affected metadata read: workspace identity/fork/ancestor/base/mutation/transfer/stages/merge/review/delete (lines 140–233), document drafts/comments/history/questions/selected-save (79–115), and Git/body lineage by targeted search. Metadata did not supply any missing governing rule proposed below.

Actual source inspection (ranges are the inspected part, not whole-file credit):

- [backend/g5-api-core/src/db/scoped.rs](../../../../backend/g5-api-core/src/db/scoped.rs): 630–733, workspace audience predicate.
- [backend/g5-api-routes/src/routes/workspaces.rs](../../../../backend/g5-api-routes/src/routes/workspaces.rs): 2110–2225, 5368–5485; audience/metadata administration and member checks; delete/download and Git handlers discovered by public function inventory.
- [backend/g5-api-core/src/services/workspace/workspace_service.rs](../../../../backend/g5-api-core/src/services/workspace/workspace_service.rs): 1980–2041, 2140–2245, 2249–2535, 2580–2648, 2990–3165, 6372–6577; phases, visibility, mutation-policy transition, deletion, invitation membership. Remainder inventoried by public operations, not treated as inspected.
- [backend/g5-api-core/src/services/identity/workspace_mutation_policy.rs](../../../../backend/g5-api-core/src/services/identity/workspace_mutation_policy.rs): 1–265; content-policy classes, freezing and live-owner guards.
- [backend/g5-api-routes/src/routes/workspace_reviews.rs](../../../../backend/g5-api-routes/src/routes/workspace_reviews.rs): 960–1180, 1396–1744, 1912–1948, 2021–2090, 2222–2265, 2345–2495; direct reader proposals, accepted changes, failed checkpoints, independent edits, reject/resolve/withdraw. Router and remaining public paths inventoried, including review start/publish/discard/artifact download.
- [frontend/src/hooks/useComposerStore.ts](../../../../frontend/src/hooks/useComposerStore.ts): 1–245; `components/comments/ComposerOwner.tsx`: 1–38; `lib/commentAnchors.ts`: 1–85, 165–374.
- [backend/g5-api-core/src/services/git/workspace_pull_request_create.rs](../../../../backend/g5-api-core/src/services/git/workspace_pull_request_create.rs): 381–578; `workspace_pull_request_provider_lifecycle.rs`: 382–467, 1045–1118; create/update and no-silent-replacement conditions.
- [backend/g5-api-core/src/services/git/external_issues.rs](../../../../backend/g5-api-core/src/services/git/external_issues.rs): 726–830, 1466–1540; federated partial results, current domain identity, exclusive primary linking.
- [backend/g5-api-core/src/services/git/git_policy.rs](../../../../backend/g5-api-core/src/services/git/git_policy.rs): 1–44; `routes/source_control_change_graph.rs`: 1–113 (route ownership only).
- [backend/g5-api-routes/src/routes/grounding.rs](../../../../backend/g5-api-routes/src/routes/grounding.rs): 352–664; `services/workspace/intent_log_service.rs`: 266–395; same-cut grounding/history and existing legacy review-action path.
- [backend/g5-api-core/src/services/workspace/question_set_service.rs](../../../../backend/g5-api-core/src/services/workspace/question_set_service.rs): 1–145, 743–828, 1181–1258; typed actor provenance and file/revision command entry. Complete decision engine not re-audited.
- Approval service: 880–1010, 2378–2635. Detailed approval omission transferred to the identity reviewer to avoid duplicate ownership.

Original/corroborating records read: [docs/workspace-sharing-v9.md](../../../../docs/workspace-sharing-v9.md) 1–80; selected v9 sections found by targeted terms; [docs/ben_new_ontology.md](../../../../docs/ben_new_ontology.md) relevant supplied statements; [.g5/chat_intent/files-source-editor/request.md](../../../chat_intent/files-source-editor/request.md) 1–46; [.g5/chat_intent/comments-design/request.md](../../../chat_intent/comments-design/request.md) 1–152; [.g5/chat_intent/workspace-button-gates/request.md](../../../chat_intent/workspace-button-gates/request.md) 1–58; [.g5/chat_intent/issue-238-plain/request.md](../../../chat_intent/issue-238-plain/request.md) (all); [.g5/pr_ontology_diff_design/issue-238-plain/ontology-plan.md](../../../pr_ontology_diff_design/issue-238-plain/ontology-plan.md) 1–150; [.g5/chat_intent/issue-387-g5d-pr-gaps/request.md](../../../chat_intent/issue-387-g5d-pr-gaps/request.md) 1–33 plus plan 62–99, 218–270 and material later correction; [.g5/ontology-plan-comments.md](../../../ontology-plan-comments.md) 85–110 and 204–248; [.g5/ontology-plan-workspace-button-gates.md](../../../ontology-plan-workspace-button-gates.md) amended gates and review summary. Focused Git history: `d6fd655c`, `8452aa9a`, `93d7c39e`, relevant path logs; baseline migration 164 at `5e309160`.

## Obligation inventory

| Capability / obligation | Current owner IDs | Disposition | Actual owner / evidence |
|---|---|---|---|
| Workspace identity, one branch vector, parent relation, owner writes | `g5.workspace.identity`, `.branch`, `.parent`, `.mutation-authority` | Present | WorkspaceService, WorkspaceForkService, mutation policy |
| Fork complete captured vector and ownership-safe retry | `g5.workspace.fork-state.formal` | Present; no new omission established | Fork owner function inventory; canonical contract already detailed |
| Tenant-local workspace audience and who changes it | `g5.security.tenant`, `g5.organization.membership`, `g5.workspace.ancestor-read`, `g5.security.owner` | **Partial: audience policy missing** | `db/scoped.rs::workspace_access_for_role`; member/visibility handlers |
| Owner transfer stops compute and revalidates recipient | `g5.workspace.transfer`, `.transfer.formal` | Present | Sharing v9 and canonical rules; lifecycle mechanism not re-audited |
| Parent deletion cascade and review merge | `g5.workspace.delete`, `.review` | Present intent; known code disagreement, not omission | delete refuses live children; review metadata already records merge discrepancy |
| Explicit non-Base read-only content policy | Base read-only only; no general content-policy owner | **Missing capability; detailed transition policy partly uncertain** | Mutation policy/transition owner; F7 |
| Phases and approval-cut eligibility | `g5.workspace.stages`, `.approval-request-settlement`, `.approval-cut-eligibility` | Partial; detailed approval findings delegated | ApprovalConfigService (target IE, admin configuration, unanimity) |
| Review suggestions produced from a copy | `g5.workspace.review` | Present at broad outcome level | review routes/services |
| Reader file proposals without fork/VM | No equivalent; `.review` requires review workspace | **Missing** | `publish_file_suggestions`; F2 |
| Suggestion action ownership and accepted-file checkpoint | `.review`, `g5.documents.comments.identity`, `.selected-save` | **Partial** | `accept_suggestion_as`, reject/resolve/withdraw; F3 |
| File identity, same-cut refs, projection recovery | `g5.documents.identity`, `.captured-references`, `.projection-recovery` | Present | Grounding captured read set; intent log typed views |
| Discussion anchor identity/authority | `g5.documents.comments.identity`, `.authority` | Present; **placement and draft survival partial** | commentAnchors, ComposerStore; F4/F5 |
| Question decisions vs agent proposals and advisory votes | `g5.documents.question-decisions`, `.question-votes` | Present in inspected scope | QuestionSetService actors/read/write entry; no additional omission established |
| Internal/external save split, branch creation, divergence, exact-head publication | `g5.git.save-publication`, `.branch-creation`, `.divergence`, `.publication-head`, `.pr-save` | Present | Git owner contracts; PR save/source admission |
| PR create/update/terminal provider identity | `g5.git.pr-update`, `.pr-identity`, `.pr-cli` | **Partial** | Shared create/update and provider lifecycle; F6 |
| Issue domains, identity, exclusivity, partial search and comment recovery | `g5.issue.domains`, `.identity`, `.primary`, `.search-completeness`, `.comment-recovery` | Present in inspected scope | ExternalIssues search/resolve/link |
| Source-control graph and legacy review action selections | `g5.documents.intent-history`, `g5.git.query-api`, `g5.ontology.lineage` | Discovery only / uncertain granularity | Routes read, graph owner not deeply read; no finding asserted |

## Proposed missing or partial intent

<a id="f1"></a>

### F1 — Workspace read audience and audience administration are unspecified (high confidence, high priority)

**Missing rule.** Organization-visible and invite-only workspaces have different read audiences. Current org admins can read either; other eligible members need organization visibility, current ownership or an invitation. The current owner manages workspace visibility/invitations; former ownership and creation do not retain private access. Preserve the newly supplied ancestor-read rule as its own governing exception rather than deleting it.

**Why existing blocks are insufficient.** Tenant isolation says nothing about isolation between two workspaces in one tenant. Owner-only VM mutation says who writes but not which same-tenant user may read a private workspace or change its audience. A replacement that exposes every workspace to every org member could satisfy those bodies while discarding invite-only collaboration. `g5.organization.admins` grants generic administration, but does not specify this particular read privilege or the owner-only audience mutation boundary.

**Direct source.** [backend/g5-api-core/src/db/scoped.rs:642–720](../../../../backend/g5-api-core/src/db/scoped.rs#L642) explicitly distinguishes org admins, `org`, `invite_only`, current owner and invited member; lines 698–700 reject creator-as-authority. `routes/workspaces.rs:2141–2172`, `5375–5418`, `5421–5452` require current owner for visibility/invite writes and validate org membership of invitees.

**Original record.** [docs/workspace-sharing-v9.md:9](../../../../docs/workspace-sharing-v9.md#L9) defines this exact policy and removal of creator/former-owner privileges; lines 32/38 describe revocation and lines 72–76 explain transfer. This is a retained v9 design rule, not an inferred new security hardening. Current code corroborates it. The supplied ancestor-read addition is newer and must remain visible beside it.

**Minimal wording / home.** Add `g5.workspace.audience`: “Workspaces may be visible to their organization or limited to invited readers. Organization administrators and the current owner may read either; other members need the selected audience grant, subject to the ancestor access rule. Only the current owner changes workspace visibility or invitations; creator and former-owner status alone grant no continuing access.” Put revocation generation mechanics beneath existing security owners only if further precision is needed.

**Limits.** This does not assert every stream/cache consumer enforces revocation; the root reviewer owns those paths. Exact interaction of ancestor access with invite-only users should be reconciled when wording is adopted, rather than weakening either rule.

<a id="f2"></a>

### F2 — Readers can propose file changes without a review workspace or VM (high confidence, high priority)

**Missing rule.** A reader may edit an existing text file into suggestions for the source owner, without forking, attaching compute or changing the source branch. Multiple pushes are additive; the proposer withdraws an earlier suggestion rather than overwriting it. An explicitly acknowledged stale base can still be proposed. Base/frozen sources and the viewer role ceiling require their stated policy exceptions.

**Why existing blocks are insufficient.** `g5.workspace.review` describes a review workspace whose changes become suggestions. It cannot reconstruct the separately requested no-fork flow. `g5.documents.comments.authority` permits replies, not new proposed changes. Owner-only mutation remains compatible with this platform-record contribution path.

**Direct source.** `routes/workspace_reviews.rs:960–1177`: reader-only entry, current authorized durable file read, stale-base confirmation (`1056–1061`), additive annotation publication (`1064–1114`), no source file write. Policy admission is `1153–1177`.

**Original record.** [.g5/chat_intent/files-source-editor/request.md:19–22](../../../chat_intent/files-source-editor/request.md#L19) directly quotes the human: non-owner changes become owner suggestions; “no fork”; “regardless of workspace vm.” Accepted scope at `38–45` limits v1 to existing text files and makes pushes additive. Commit `d6fd655c` repeats the implemented outcome and is corroboration, not human authorship proof.

**Minimal wording / home.** Add a document/review feature block: “Eligible readers may propose changes to an existing text file from their authorized source view without forking or running compute. Publishing records additive suggestions and does not modify the source branch; the proposer may withdraw their own proposal. A changed base is disclosed and requires explicit confirmation to submit.” Reference F3 for decisions and the existing role/read-only owners for eligibility. Keep a note that the viewer collaboration Q also affects this reader capability, not only replies/votes.

**Limits.** The current reader implementation can select retained flush state; the current ontology's committed-branch read rule remains governing. This finding does **not** revive flush browsing or expand v1 to create/delete/rename.

<a id="f3"></a>

### F3 — Review acceptance lacks its action and publication contract (high confidence, high priority)

**Missing rule.** Source-owner decisions apply to each suggestion (and independently selected edit), reviewer withdrawal applies only to their own open proposal, and discussion itself grants neither decision authority nor formal approval. Accepting a proposed file change, including a new file, invokes the shared selected-file checkpoint. Once bytes are applied, a checkpoint failure leaves that acceptance settled and reports pending publication; it must not be presented as unapplied/rejectable. Ambiguous or changed source cannot be silently overwritten.

**Why existing blocks are insufficient.** `.review` promises visible suggestions but not accepting/rejecting them individually. `g5.documents.comments.identity` only names a separate review owner. `g5.documents.selected-save` is scoped to explicit file Save/ordinary editing/question answers, not review acceptance. One can implement “Accept” as a dirty write, or relabel it rejected after push failure, without violating the literal current bodies.

**Direct source.** `routes/workspace_reviews.rs:1396–1456` owns accept/admission and distinguishes artifact acceptance without file mutation; `1519–1667` uses live owner state and refuses changed/ambiguous targets; `1671–1744` writes, checkpoints selected path, then records accepted even if publication fails. `2021–2088` preserves independent edits and sibling decisions. `2222–2248`, `2366–2397`, `2433–2457` establish owner reject/resolve versus reviewer withdrawal.

**Original records.** [.g5/chat_intent/issue-238-plain/request.md:5](../../../chat_intent/issue-238-plain/request.md#L5) records issue #238's explicit Save **and review acceptance**, including new files. [.g5/pr_ontology_diff_design/issue-238-plain/ontology-plan.md:46–74](../../../pr_ontology_diff_design/issue-238-plain/ontology-plan.md#L46) records the distinct checkpoint/accepted-pending contracts and their provenance; `145–147` clarifies the focused implementation. [.g5/ontology-plan-comments.md:208–240](../../../ontology-plan-comments.md#L208) establishes per-suggestion decision/withdrawal and reply-versus-decision distinctions. Commit `93d7c39e` corroborates independent-edit behavior.

**Minimal wording / home.** Two concise review children under `.review` (or a separate review feature owner): (1) “The source owner accepts or rejects individual proposed changes and resolves comments; a reviewer may withdraw their own open proposal. Discussion does not change decision authority.” (2) “Acceptance applies only the selected proposal against matching, unambiguous source, then uses selected-file Save for affected files, including new files. If application succeeds but publication fails, retain the accepted decision and expose pending publication for retry.” Reference the existing Save and VM/version rules rather than duplicate their mechanics.

**Limits.** Do not add the old prohibition on merging review copies: the current human-supplied ontology explicitly allows it and records the implementation mismatch. No new claim about crash recovery between file write and database acceptance was executed.

<a id="f4"></a>

### F4 — Discussion drafts and failed decisions can still disappear under the written contract (high confidence, medium priority)

**Missing rule.** Replies, new-thread drafts, rejection reasons and edited replacements survive transient refreshes, rendering/tab/regrouping changes and removal/settlement of their displayed target. A failed send preserves the words and error; authority loss leaves text visible read-only with an explicit discard; another principal/scope starts clean. This need not promise persistence across browser restart.

**Why existing blocks are insufficient.** `g5.documents.files-view.drafts` owns file-content drafts. It does not include discussion or decision composers. `comments.authority` governs successful writes, and `ui.authentication-continuity` protects identity but not retention of unsent user input. Losing a typed rejection reason on a source commit could obey all three.

**Direct source.** [frontend/src/hooks/useComposerStore.ts:24–32](../../../../frontend/src/hooks/useComposerStore.ts#L24), `70–190`, `202–220`; `components/comments/ComposerOwner.tsx:11–24`. Shared store owns text, in-flight send and errors independently of rendered composers and resets by principal/org/workspace scope.

**Original record.** [.g5/chat_intent/comments-design/request.md:124–126](../../../chat_intent/comments-design/request.md#L124) records the human's explicit page-level owner decision covering replies, rejection reasons, replacements and new-thread drafts so mode switches, commits, deletion and regrouping lose nothing. Lines `135–148` explicitly add read-only retention/Dismiss on authority loss and visible history errors. These are direct accepted choices after repeated demonstrated failures, not generic UX advice.

**Minimal wording / home.** Add `g5.documents.comments.drafts`: “Preserve a person's unsent discussion text, edited proposals, pending sends and errors across view changes and target updates/removal. Refusal or loss of authority leaves the words visible for recovery or explicit dismissal; changing the person or scope starts a separate draft context.” State page lifetime rather than prescribing a React store.

**Limits.** Relevant store structure was read; all consumer renderers were not re-executed. This is intent omission, not a claim that current UI loses drafts.

<a id="f5"></a>

### F5 — Retaining anchor evidence does not require truthful placement (high confidence, medium priority)

**Missing rule.** Reattach a comment only with evidence that the displayed text is the original target. Ambiguity or loss keeps the thread available with its original anchor, rather than selecting an arbitrary duplicate or dropping the thread.

**Why existing blocks are insufficient.** `g5.documents.comments.identity` requires retaining evidence but permits an implementation to retain it while displaying the thread on the nearest repeated heading. Generic captured references concerns document references, not line placement. Existing metadata explicitly avoids mandating an exact algorithm; that does not require omitting the outcome constraint.

**Direct source.** [frontend/src/lib/commentAnchors.ts:191–208](../../../../frontend/src/lib/commentAnchors.ts#L191), `298–352`: exact-content match, contextual candidate proof and explicit ambiguous/orphan dispositions; `355–365` forbids nearest-duplicate choice.

**Original record.** [.g5/chat_intent/comments-design/request.md:27–33](../../../chat_intent/comments-design/request.md#L27) identifies the demonstrated misplacement failure; the human requested design/implementation at `13–19`, `61–63`. [.g5/ontology-plan-comments.md:89–94](../../../ontology-plan-comments.md#L89) records the surviving intended outcome and original-anchor visibility. Exact two-line context and 20-line proximity are implementations/design detail unnecessary to the minimal recovered rule.

**Minimal wording / home.** Extend `g5.documents.comments.identity`: “Place a thread in changed text only when its retained evidence identifies the target unambiguously; otherwise keep it readable with its original anchor and an unresolved-placement indication.”

**Limits.** Do not infer a required diff algorithm or freeze current numeric heuristics.

<a id="f6"></a>

### F6 — PR update and replacement semantics were compressed away (high confidence, medium priority)

**Missing rule.** Update continues an existing open G5 PR and never creates a new G5 PR; create reuses the matching open PR. A previously recorded provider PR that has closed/merged is not silently replaced. Explicit replacement authorizes a new provider PR. **Exception retained:** update may finish first publication for a repository with no provider PR yet.

**Why existing blocks are insufficient.** `g5.git.pr-update` says only to follow save-before-publication; `.pr-identity` fixes source/target/revisions but not operation identity. “CLI follows gh” does not define G5's update command. A replacement could recreate a fresh PR after an update to a merged one and still save/publish exact versions.

**Direct source.** `workspace_pull_request_create.rs:395–449`, `479–493`, `528–533` reject invalid replacement/update and no-open-PR cases, including closure during Save. `workspace_pull_request_provider_lifecycle.rs:1072–1114` refuses silent terminal provider replacement and reports the explicit command.

**Original record.** [.g5/chat_intent/issue-387-g5d-pr-gaps/request.md:14–15](../../../chat_intent/issue-387-g5d-pr-gaps/request.md#L14) records the harmful prior behavior; `27` directly authorizes implementing phases 1–4 with shared UI/CLI behavior. Plan `218–222` makes the semantics explicit, while **later correction `270`** restores absent-provider first-publication recovery. The earlier draft “never creates any provider PR” must not be recovered.

**Minimal wording / home.** Expand `g5.git.pr-update`: “Create reuses the matching open G5 PR; Update requires an existing open G5 PR and cannot create another. Neither silently replaces a closed or merged provider PR; replacement is explicit. Update may complete initial provider publication for a member that has no provider PR.” Keep existing publication and outcome blocks as owners of save order and partial-result reporting.

**Limits.** Current code inspected; live GitHub writes not tested. Shared UI/CLI API parity is already governed by `g5.ui.api-parity` and is not a new finding.

<a id="f7"></a>

### F7 — Explicit non-Base read-only content policy has no owner (capability high confidence; precise drain policy medium, lower priority)

**Missing capability.** An owner can make a workspace's application content read-only independently of VM running/stopped state, and later restore editability. Discussion/content mutations are blocked; cancellation, administration and already-authorized Git synchronization have separate rules. Draining into read-only does not itself discard existing work.

**Why existing blocks are insufficient.** `g5.workspace.base` only describes Base. Owner-only mutation is an upper bound on actors, not a user-controlled restriction on the owner's workspace. UI unavailable-actions explains presentation but cannot reconstruct a content-policy operation. An implementation with no Make read-only action meets current bodies.

**Direct source.** `workspace_mutation_policy.rs:1–109`, `196–229` distinguishes content policy from Git and defines freezing admission; `workspace_service.rs:2249–2535`, `2580–2648` offers quiescent/drain transitions, cancellation/reversion and timeout behavior. Current schema has independent mutation-policy and transition checks at [backend/g5-api/src/db/migrations/001_lean_baseline.sql:5499–5500](../../../../backend/g5-api/src/db/migrations/001_lean_baseline.sql#L5499).

**Corroboration and uncertainty.** The human-selected button policies in [.g5/chat_intent/workspace-button-gates/request.md:32–43](../../../chat_intent/workspace-button-gates/request.md#L32) preserve Make read-only/editable as an actual capability, distinct from VM state. The merged plan's later review summary at [.g5/ontology-plan-workspace-button-gates.md:117](../../../ontology-plan-workspace-button-gates.md#L117) explicitly retains sync on read-only and preview/terminal according to runtime authority. Historical migration 164 at commit `5e309160` explains draining, reversible freeze and manual recovery after process loss, but its referenced original `read-only-workspace-transition-design.md` is unavailable in current tree and was not recovered. Therefore the precise exception table and drain timeout semantics remain agent-originated observed policy, not a confidently recovered human specification.

**Minimal wording / home.** Add `g5.workspace.content-policy`: “The owner may make a non-Base workspace's application content read-only and later restore editing, independently of whether compute is attached. Read-only blocks new content and discussion writes while leaving authorized teardown, administration and Git synchronization under their own rules.” A separate short D/Q should reconcile the drain/retained-work exceptions if those details are to govern; do not turn all current enum cases into ontology on code alone.

**Limits.** Native terminal writes are not filesystem-enforced read-only. Do not promise physical immutability or conflate this feature with Base, stopped VM, Git protection or review-copy exclusions.

## Rejected candidates and known mismatches (not omissions)

- **Deleting parents must cascade / review copies may merge:** already explicit in `g5.workspace.delete` and `.review`, and existing metadata records the contrary implementation. Adding code's refusal would weaken current intent.
- **Reader browsing should use retained dirty flush snapshots:** current source code and older v9 design do this, but supplied `g5.workspace.durable-read` requires committed branch state and `g5.git.saved-read` preserves the distinction. Existing code gap, not missing permission.
- **Exact workspace branch name vs generated uniquifier:** supplied branch rule is explicit. Do not revive v9's old exact-name/collision choice as omission.
- **Git object visibility requires a fresh cache artifact / force-with-lease machinery:** concrete mechanisms should not be added merely because code/docs describe them. Audience policy F1 plus existing repository authorization/incarnation/namespace contracts may govern protected outcomes; the full object closure implementation was not re-audited.
- **Selected Save safety generally:** `.selected-save`, runtime mutation/version and acknowledgment owners already cover the relevant Save obligations. F3 is specifically a missing *review-acceptance trigger and decision effect*, not repetition of those mechanisms. Existing `WORKSPACE-SAVE-BLOB` remains a code mismatch.
- **External issue identity/exclusive primary/search completeness:** explicit existing blocks cover inspected route/service behavior; unique indexes, partial-result arrays and case normalization do not each need ontology.
- **Question actor provenance and revision/idempotency:** existing `.question-decisions` and `.question-votes` cover the inspected outcome. No additional finding based on format/limits alone.
- **Separate review-thread owner:** already in `.comments.identity` and invariant-preserving owner operations. Do not add a second abstraction requirement merely because route code is long.
- **PR current provider authority, protected branches, exact source cut and partial results:** already governed. F6 concerns a specific missing create/update/replacement rule, not general retry safety.
- **No direct Cloud Build log reader:** source decision exists, but root owns operations/CI; not duplicated here.
- **Approval configuration/admin/unanimity/target IE gating:** concrete omission evidence sent to identity reviewer. It should be integrated once, not counted again here.

## Important unexplored or incomplete areas

This bounded pass does not justify full subsystem completeness. Remaining behavior-bearing areas include complete review round supersession/carry-forward/artifact lifecycle, review Start/Update/Discard with live dirty files, all repository onboarding/removal paths and credential changes during import, lower-level Git receive/pack/object-closure consumers, remote integration recovery/GC/watermark handling, Source Control graph projection/rebase comparison semantics, legacy Project Doc review-action command semantics, complete document upload/downlift/indexing and linked-file scanners, and all comment/review UI renderer consumers. Function inventories and search hits in those areas were discovery only. Source-graph detail and legacy review-action behavior should be investigated before treating them as mere mechanisms.

No absence of new tests or model runs is reported as missing intent. Existing evidence descriptions were read only for context; they do not establish execution on this checkout. No global coverage percentage is meaningful from this candidate set.
