# Abstraction leakiness: audit and refactor plan

## Human request

On 2026-10-01 the user attached `abstraction-leakiness-review-2026-10-01.md` (a review of five platform abstractions at `05dddc47`) and asked:

> please create a clean git worktree under folder `~/Workspace` and work on audit the doc attached

On 2026-10-02, after reading the audit, the user asked:

> please make a plan to refactor to reduce leakiness, please focus on low hanging fruit (i.e, i do not expect you make a plan to fix everything but only fix those critical leaks ) say, our budget is 5 PRs, each with less than 5k of code changes

Later the same day, on order and rollout:

> please do not complicate rollout, as there is no active user in qa, we can have manual intervention, as for order, let's do it in sequential

On 2026-10-03, asked whether PR 4 should build on the overlapping job-allocation draft G5Labs/G5-Lean#201:

> please completely ignore what #201 is doing, we are going to close it

## Agent interpretation

- The plan is a refactor of existing code toward ownership the ontology already states. It adds no product behavior.
- "Critical" is read as a leak that can cause wrong behavior or wrong authority today, or one that lets new leaks appear unnoticed. "Low hanging" is read as small, mostly mechanical and checkable with existing tests.
- The budget is a ceiling: at most five PRs, each under 5,000 changed lines (additions plus deletions). Leaks that do not fit are left out and listed, not half-fixed.
- Evidence comes from an audit of that review (kept outside the repository) and was re-checked on `main` at `8a3ec325` before planning.
- The five PRs run one at a time in the plan's order. Rollout is a merge, the QA redeploy and a manual check; no staged rollout, canary or compatibility window is planned.
- The choice of a source guard with declared table writers, and the choice of which five PRs, are agent decisions for the user to review.
