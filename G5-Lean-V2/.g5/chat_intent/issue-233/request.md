# Issue 233 implementation request

The user asked to read [issue 233](https://github.com/G5Labs/G5-Lean/issues/233)
carefully, work from the latest `origin/main` in a separate branch and worktree, and
implement it. The issue asks for one production lifecycle, effective execution limits,
bounded IO, explicit deadlines and production-path regressions. It calls for separate
reviewable PRs for behavior fixes and mechanical consolidation, with issue 229's urgent
cancellation fix proceeding independently and issue 168's termination work reused.

The user also required staged local validation, independent Claude CLI review only
after local checks pass, at most six review and address rounds, rebase before each
Cloud CI run, two required Cloud checks on the final head and bounded CI watching.
