# Human instructions — QA review prototype

The user requests an implemented, clickable review mock on a separate G5-Lean branch, building on the actual QA workbench rather than UXv2. Continue the existing review-flow design.

- Consolidate the Activity landing into task overview followed by timeline; avoid four additional selectors. Keep task/issue description distinct from PR implementation/decisions.
- Put changes in Source Control, Project Docs and the richer Ontology tools, with summary links. One shared conversation covers Business, System and Integration Engineering.
- Move repeated Approvals into the left rail if appropriate. Request/Submit Review share a position there; use configured approval types and configured reviewers. Multiple approvals are independent, and owners do not approve themselves. No approval/request before PR submission.
- Approve only pushed revisions. Current unpushed/dirty work remains owner-only. Workspace identity is a repository SHA vector; show changed repositories without deciding whether old approvals remain valid.
- Offer pushed, current, and changes since each reviewer's last submitted review; no arbitrary diff-anything picker.
- Inspect owner rahul-test and non-owner preview-test at qa.g5labs.ai. Do not special-case Base by name. Started/stopped state and ownership matter independently.
- Latest clarification: repurpose the current Activity page as landing, and consider a hamburger or another clearer landing icon.

Agent interpretation: use a dashboard icon labeled Workspace, a stable Approvals rail destination, and an isolated native frontend entry with sample data. No instruction authorizes changing live QA records or deploying this preview.

- Follow-up: do not use a house icon; Home already uses one at the top left.

## October 12 follow-up

Restore QA's icons and icon-only rail; replace only Activity's icon with Workspace. Add All files (no diff), owner/running-only Uncommitted changes and All unpushed changes, and durable Pushed vs workspace base / Pushed vs parent choices in Docs, Source Control and Files. Non-owners see pushed content only. Accommodate one requested type, multiple requested types, and unsolicited reviews with clear actions. The task issue must be clickable; long task/implementation descriptions need full reading space. Landing changes use the workspace base, with a notice if the parent has advanced.

Agent interpretation pending answers: saved workspace base is the fork/rebase revision vector; parent is its current pushed vector. Voluntary formal reviews choose eligible configured types and use Approve / Request changes, retaining request provenance separately.

Human answers: unrequested reviewers may leave general feedback only. Base means the saved revisions the workspace branched from; parent means the parent's latest pushed revisions. These supersede the provisional voluntary formal-review interpretation above.


The human requested push timestamps beside each pushed commit and retention of QA’s full Source Control commit history, preferably through a Commit History popup replacing the revision dropdown. Remove every added “Return to workspace” link, including Source Control, Project Docs, Core Ontology and Files.

The human corrected the per-repository history model: G5 is multi-repository and a workspace revision is one commit vector. Use a table of complete workspace vectors for history, and the same UI concept for current PR state. This supersedes the prior per-repository selector interpretation.

The human approved the mock, requested GitHub issue #447 and draft PR #448 with the mocks committed first, and authorized production implementation on the same branch and worktree while keeping the localhost mock available for demos. The issue is assigned to the human’s authenticated GitHub account.

The human requested a completeness audit of this PR’s E2E/value-loop test list and explicitly requires coverage for both workspace owners and non-owners. Treat assigned reviewers and unrequested visitors as distinct non-owner roles, with permitted actions and server refusals covered across running/stopped workspaces.

The human asked whether diff inspection, commenting and timeline integration were proved, asked for other UX acceptance gaps, then instructed: “ok implement all the necessary acceptance tests.” The continuing design records executable RV1–RV10 cases and separates authored coverage from executed evidence.

The human subsequently rejected leaving the product at the initial production slice and explicitly instructed completion of all remaining production implementation, including persisted review flows, canonical discussion/timeline integration, multi-repository revision tracking, consistent comparisons/history and recovery/concurrency. Outstanding development does not require another UX approval. Questions may be raised when genuinely needed.


## October 13 implementation follow-up

Human instructions: carefully read issue #447, PR #448 and latest origin/main, then implement in PR #448's existing local branch/worktree. Move the Approvals icon to the rightmost point of the top ribbon. Add approval choices for at least one reviewer versus all reviewers and Allow self approval. Defaults: at least one approval with no denial, self approval disallowed. Update issue and PR as needed. Invoke Claude by CLI for review only after self-verification; disclose verification results, independently assess findings, use no more than six review/address rounds and raise remaining significant disagreements to the human. Follow the supplied focused-to-workspace validation sequence, rebase latest main before each cloud CI, batch fixes and use a bounded watcher.

Agent interpretation: settings belong to immutable organization approval definition versions; old versions keep their all-reviewer policy while new versions default to any reviewer. The owner still needs configured membership, read access, a submitted PR and an addressed request; both the requested and current policy must allow self approval. A configured agent remains independently required. Meeting quorum never settles another person's request.

Default interpretation clarified during review: a blank definition editor and omitted create/publish API fields use any/self-off. Editing preserves the visible saved selections and sends them explicitly; the human did not request resetting stored policy on every edit.

## October 13 navigation clarification

Human reports missing ribbon actions and a failed workspace fork on localhost:3002. Approvals should be the rightmost icon among the action icons, before workspace controls, rather than at the far right of the full ribbon. Diagnose and repair the real local instance and keep the same PR/design.


## October 15 workspace-wide approval correction

Human feedback on the local production approvals page: approvals belong to a workspace, not individual engineering phases; list required approvals once. Request review accepts an optional information note for the reviewer. Each reviewer has one status: Not requested, Requested at a time, Approved or Rejected at a time, and optionally Re-requested. The target requires an approval type such as Peer review (any), rather than a separate requirement for each configured person; show the approval name at the top and requested reviewers slightly nested beneath it with their own status. The human explicitly chooses to update the production flow and refresh localhost:3002.

Agent interpretation: supersede the previous phase axis for request identity, evaluation, personal checkpoints and UI while preserving immutable historical evidence, addressed PR-bound human authority, any/all quorum, denial veto and the existing pushed-vector validity rules. Every attached target requirement is workspace-wide, including definitions with legacy empty/BE/SE phase bindings. Configured people remain the eligible selection pool; names appear as current reviewer rows after an actual request, rather than as target requirements. No new post-push invalidation policy is introduced.

The human clarifies the preferred usage model: approvals never gate phase movement; only target-configured required approvals gate workspace merge. A definition may be associated with an engineering phase as the earliest sensible request recommendation. The request picker puts the current phase first, then remaining phases in BE/SE/IE order: BE→BE/SE/IE, SE→SE/BE/IE, IE→IE/BE/SE. Required types appear once with “Recommended for Business Engineering” (or the associated phase), never repeated by phase. Continue implementation under this clarification.

Compatibility interpretation: retain existing zero/multiple phase associations without duplicating types. Display each associated phase as a recommendation and sort a multiply-associated type by its first association in the current phase order; unassociated types follow associated types. New configuration can retain the existing association choices; none makes a requirement unassociated, never inert.


### October 15 single phase configuration clarification

The human requires a single dropdown for the earliest phase at which an approval is relevant, defaulting to Integration Engineering. It belongs in the regular approval configuration window, outside Advanced. This supersedes the earlier agent interpretation permitting multiple associations in new configuration.

Agent interpretation: keep existing stored definitions readable; editing selects the earliest legacy association in engineering order, or IE when none, and publishing saves one selected phase through the existing versioned API. No bulk history rewrite or backend policy change is requested.

## October 16 combined approver policy

The human expands a definition to an optional agent (harness, model, effort and prompt) plus zero or more humans, with at least one configured approver in total. Any-of is the default: at least one configured approver says yes and none says no. All requires every configured human and agent to say yes. Human approvals may override an agent rejection by default: in Any-of, a human yes overrides the agent no, but any human no still blocks. The human explicitly clarifies that override applies only to Any-of; All still requires every approver's yes. Self approval defaults to false. Put the quorum, human override and self-approval options under a click-to-expand Advanced section.

The human requires rebasing onto the latest origin/main before making this change. After local verification, the human authorizes a fresh Claude review allowance of up to six rounds for this approval expansion.

Agent interpretation: reuse the existing definition versions, actor roster, addressed requests, agent execution configuration and shared merge/display evaluator. Keep the previously requested earliest-phase dropdown in the regular form. Apply combined quorum semantics to the latest policy, including existing Any-of definitions; preserve historical versions and decisions without rewriting them or adding a legacy evaluation mode. Existing self-eligibility and reviewed-cut checks remain authoritative.


October 20 continuation: after exact-head CI passed, main advanced with organization-member deactivation and migration099. Continue the authorized latest-main rebase, preserve active-recipient admission and retained history, resequence only unmerged PR SQL to100–104, and require new-head CI before merge. The human explicitly requests task-related branches, worktree and test-resource cleanup after merge; preserve unrelated root-checkout work.

## October 20 rebase, CI repair and merge

### Human instructions

“rebase with most recent origin/main, push to rtemote, watch ci, fix any errors and coderabbit comments that are in scope and you are confident about, push in a batch at end of ci, wait till green ci.”

“check if we are against most recent origin/main, rebase if not, and push. repeat till green ci, merge as soon as you have green ci”

The human authorizes gh/gcloud outside the sandbox and refreshed the local Mac's gcloud login when requested.

### Agent interpretation and selected security-gate decision

Continue the same PR/worktree and saved design, preserve compatible main intent during rebases, inspect actual failures, validate bounded repairs and publish batches after the active run ends. Tool access does not authorize bypassing login boundaries or repository protection. The merge instruction supersedes the earlier no-merge limit for PR #448; it does not authorize changing live QA settings.

The human answers the certificate-advisory design question: **“Keep gate; wait for upstream correction.”**

Newly published RustSec certificate advisories require two patch dependency upgrades. The residual audit finding concerns the immutable webpki-roots 0.26.11 forwarding package even after its actual roots resolve to patched 1.0.10. The human declines the proposed guarded compatibility compensation in the [same saved design](../../../docs/review-flow.md#october-20-certificate-advisory-follow-up). Preserve the existing full advisory gate, keep the prepared TLS patches, and wait for an authoritative upstream correction or compatible release. Main's subsequent advisory-wide ignore is not authority to override this explicit instruction. On the next integration, retain the full gate without that ignore; resume publication and the authorized green-CI merge loop only when the patched graph passes it without a local exception. No ignore, guard or corresponding ontology amendment has been applied in this worktree.


### Renewed latest-main publication

The human subsequently instructs: “rebase against most recent origin/main, push, correct any ci errors and coderabbit comments that are in scope and you are confident about, push all changes in one batch at the end of ci, repeat till green ci”.

Resume rebasing and publication now, preserving the earlier explicit full-advisory-gate choice. This renews action on the integration rather than authorizing the declined security exception; the wrapper finding remains an upstream blocker if unchanged. Preserve main's Guided panel intent and published migration 098; resequence only this unmerged PR's additions.


### Adopt main's certificate-advisory fix

The human explicitly says **“adopt main's fix”** after the explanation of the full advisory gate, forwarding package finding and main's advisory ignore. This supersedes the earlier keep-gate/wait choice. Use main's exact TLS patch lockfile and advisory configuration, without the declined custom guard, and resume the authorized rebase, publication and CI loop. The advisory-wide scope of main's ignore is recorded in the same design; all other checks remain enforced.
