---
name: issue-816-pr-822
description: "Issue 816 non-stacked workspace landing shipped as PR #822; design path, scope decisions, CI traps and the ship-loop instruction."
metadata:
  node_type: memory
  type: project
  originSessionId: 72aafc6e-59a5-4bb6-a03f-f450f162b2dd
  modified: 2026-10-25T07:27:45.389Z
---

Issue G5Labs/G5-Lean#816 (workspace landing for non-stacked PRs, with or without GitHub) is PR #822 on branch `issue-816-improve-workspace-landing-for-non-stacked-prs-at`, opened 2026-10-25. Stacked PRs are #818.

Design: `.g5/pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md`; ontology blocks `g5.workbench.landing.{summary,task,implementation,status}` (ui.g5.md) and `g5.git.pr-description` (git.g5.md). New read endpoint `GET …/pull-requests/:id/description` lives in `workspace_pull_request_description.rs` (child module of the provider lifecycle service, reusing `build_provider_pr_payload`).

Agent decisions the owner has not confirmed: landing presents the open PR else the latest (a closed PR shows Closed, not the empty state); RV10's landing-PR-submit retention was dropped (Prepare to Ship owns submission); approvals before a PR read "N required · not requested".

Owner instruction (2026-10-25): rebase on latest origin/main, fix in-scope CodeRabbit comments and CI errors, push in one batch per CI round, repeat until green; refresh any running preview after each push (none was running).

CI traps hit: `frontend-test` runs `src/test/button-testid-coverage.test.ts` (every raw `<button>` needs `data-testid`); new `g5-api-core/src/services/**` files must be listed in HOMES of `backend/g5-api/tests/service_context_layering.rs`; landing reads of issue links must be gated on `useExternalIssuesFeature` (WorkspacePageV2 asserts no request when off). See [[bot-review-scope-discipline]], [[g5-ci-cloudbuild-triage]].

Later the same day a Codex session in this checkout implemented the owner-approved Activity cleanup (commits dde6c876, b2db5049) and twice reintroduced `getByRole(..., { name, exact: true })`, which breaks CI's frontend-build: `tsconfig.build.json` skips test files, so check with `npx tsc -p tsconfig.json`. While another agent has uncommitted edits, stage only your own lines and publish with a plain fast-forward `git push`; `g5d pr update` saves pending changes and would sweep its WIP in.
