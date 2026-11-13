# Issue #238 — plain PR follow-up

## Human request

The original [issue #238](https://github.com/G5Labs/G5-Lean/issues/238) requires the owner’s explicit editor Save and acceptance of review changes, including new files, to commit and push only affected files through one reusable file-list operation; no empty commits; minimal production-quality implementation.

In the [plain PR follow-up](https://github.com/G5Labs/G5-Lean/pull/257), the user asked to read `docs/experiments/issue-238-plain-vs-g5-scope.md`, narrowly port fixes and tests needed to make the plain PR safer to merge, avoid behaviors inferred only from broad ontology, update the PR’s ontology to match code, and fully verify the original feature on its own dev stack.

In the later PR CI follow-up, the user asked to fix the reported failures, wait for the running checks, and continue fixing and pushing until the PR checks are green.

## Agent interpretation

Preserve the plain shared selected-file Git helper. Repair concrete Save trigger gaps, review partial-result truthfulness and selected-version safety. Do not import G5’s durable coordinator, receipt journal, retention policy or arbitrary aggregate limits without a demonstrated need for this PR.
