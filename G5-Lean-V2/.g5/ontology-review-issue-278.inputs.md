# Review inputs: PR G5Labs/G5-Lean#280 (issue #278)

Repository: /Users/bvandiver/G5/lean/G5-Lean, branch `fix/approval-requirement-org-admin`.
Base: `bce87811` (merge-base with origin/main). Review the committed range `bce87811..HEAD`.

## Request

1. Issue #278 (saved verbatim in `[issue #278](https://github.com/G5Labs/G5-Lean/issues/278) beside this file): org admins
   cannot attach/retire approval requirements on Base unless they own it.
2. Human decisions made while the PR was open (human-provided):
   - H1: Fix in this PR the case where a non-owner human, addressed by a pending
     human approval request, gets 403 when submitting the decision
     (`POST /api/v1/workspaces/:id/approvals/decisions` with a `requestId`).
   - H2: Decisions submitted WITHOUT a `requestId` (requestless direct decisions)
     keep the existing owner-only behavior for now.
   - H3: Fix the local dev OAuth stub in this PR so the manual consent page's
     Email/Username fields choose the logged-in identity (needed to test a second
     dev user).
   - H4 (review feedback accepted): the API regression test should exercise
     attach/retire with an `org:admin`-only bearer and include representative
     content and VM denials; add a browser (Playwright) flow for a non-owner org
     admin attaching and retiring a Base requirement.

## Ontology changes and governance (RECONSTRUCTED)

No design document was saved before implementation (legacy-handoff gap). This section
is reconstructed after the fact from the request, the human decisions above and the
actual ontology diff (`ontology.diff`); it is not an original handoff.

Changed block IDs:
- `g5.governance.approval.requirements` (new, `.g5/ontology/features/governance.g5.md`)
- `g5.governance.approval` (body: requestless-decision exception per H2; metadata)
- Metadata only: `g5.story.record-a-formal-review-decision`,
  the other story/footers in governance.g5.md mentioning approver ownership,
  `.g5/ontology/software_architecture/git_governance.g5.md` (approval footer),
  `.g5/ontology/concepts/system_design_principles.g5.md` (two footers).

Relevant/governing blocks to read (current text, not just IDs):
- `g5.governance.approval`, `g5.governance.approval.version`,
  `g5.governance.approval.requirements` in `.g5/ontology/features/governance.g5.md`
- `g5.security.owner`, `g5.security.owner.eligibility`, `g5.security.read`,
  `g5.security.read.sharing`, `g5.security.read.durable` in
  `.g5/ontology/concepts/security.g5.md`
- System design principles touching route/service authorization in
  `.g5/ontology/concepts/system_design_principles.g5.md`
- Workspace mutation policy spec referenced from
  `backend/g5-api-core/src/services/workspace_mutation_policy.rs`
- Ontology authoring/metadata format: `.agents/skills/ontology-authoring/references/format.md`
- Design review guide: `.agents/skills/ontology-authoring/references/design_review.md`

Files changed: see `git diff --stat bce87811 HEAD` (route handlers in
`backend/g5-api-routes/src/routes/workspace_approvals.rs`, Rust integration tests,
Playwright spec + e2e oauth identities, dev OAuth stub `infra/docker/oauth-stub/server.js`,
ontology files).

Consequential cases to consider (non-exhaustive; extend from the diff):
- Org admin (non-owner) attach/retire on Base; member denied; other tenant denied;
  no owner-only content/sharing/VM authority granted.
- Addressed non-owner human settles (approve/reject/deferred undo) their own pending
  request; cannot settle another actor's request; requestless decision stays owner-only.
- Read-only / freezing / pending-transfer workspace states and how the changed guards
  interact with them.
- Any other caller of the changed helpers/guards.
