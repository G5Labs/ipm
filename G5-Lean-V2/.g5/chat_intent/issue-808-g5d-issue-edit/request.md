# `g5d issue edit`: update and assign existing issues

Session of 2026-10-29 (Claude Code, for the issue's assignee). Design:
[ontology-plan](../../pr_ontology_diff_design/issue-808-g5d-issue-edit/ontology-plan.md).

## Human instruction

The request is [G5-Lean#808](https://github.com/G5Labs/G5-Lean/issues/808): let `g5d issue`
update an existing issue's title, description and state, assign and unassign users, and manage
listeners, with clear outcomes for invalid users, missing access, unsupported operations and
provider failures, and never report an unapplied change as successful.

The assignee's comment of 2026-10-29 on the issue:

> don't add listeners/watchers/subscribers - that is out of scope for the moment.

Answers in this session (2026-10-29), to the agent's questions:

- Which issues may `g5d issue edit` change? **Any readable issue**: as `g5d issue comment`, the
  primary issue by default, or any issue `view` can read, under the owner's account and the
  existing write permissions.
- Should `g5d issue close`/`reopen` widen to the same set? **Yes, match edit**: one selection
  rule for every agent write.

## Agent interpretation

- State changes already exist as `close`/`reopen`; "appropriate state changes" is met by widening
  them, not by a state flag on `edit`.
- Assignment follows `gh issue edit` (`--add-assignee`, `--remove-assignee`) rather than separate
  commands, as the CLI mirrors gh.
- Comment editing stays a recorded limitation, as the issue says.

## Later instruction (2026-10-29): external issues edited in the platform

> An additional requirement: We should be able to edit/update external issues in the platform
> just like we can internal issues. eg a user shouldn't be forced to the remote website to do
> operations that are supported by the issue adapter. give me a proposal for your suggested changes

The agent proposed the issue page's write controls for GitHub issues (edit, assign, close/reopen,
comment, edit own comment) through org-level routes and the existing guards, a field-level stale
check for edits, the untrusted-domain review step, labels left on GitHub, "completed" as the only
close reason, delivery on this branch. Asked what `issue_comment_publications` adds, the owner
asked:

> What real value is this table adding today? I thought we already had a hidden marker so comment
> posting can avoid duplicates. What if we deleted it, then we wouldn't have a problem with issues
> w/o a workspace?

After the agent's assessment (key replay without a GitHub read, cross-session pending receipts in
Activity, a weak history, PR comments also using it) and recommendation (issue comments become
stateless like issue creation; PR comments keep the table for now):

> Your recommend looks good. file an issue for the 'pr comment' usage of the table - we'll do that
> separately. Then proceed with the implementation.

Filed as [G5-Lean#950](https://github.com/G5Labs/G5-Lean/issues/950). The owner's approval covers
the proposal's recommended defaults: field-level stale check, the untrusted review step kept,
labels on GitHub, close as completed only, delivery on this branch.

On 2026-10-30, asked to explain the branch's migration (which deleted the issue-comment rows and
refused new ones), the owner said:

> remove the migration - we'll drop this table shortly, so no need to clean it up now
