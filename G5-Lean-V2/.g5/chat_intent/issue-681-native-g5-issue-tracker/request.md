# Native G5 issue tracker

Session of 2026-10-22 (Claude Code). The request is the owner's
[issue #681](https://github.com/G5Labs/G5-Lean/issues/681). Design:
[ontology-plan](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md).

## Human instruction

The agent asked four questions (set `native-issue-tracker` in the docs repository,
`docs/8e8564d2-0481-43b7-b2f6-f339bd3d0feb/questions/native-issue-tracker.g5q.json`), each
with a recommended option. The owner replied:

> I approve of your recommendations for 1-4.

## Agent interpretation

1. `reverse-decision-14` → reverse decision 14 for the native tracker only. Retired local-issue
   features stay retired, and their dropped data is not restored.
2. `availability` → native and external domains coexist, with one default creation domain.
3. `migration` → native issues and links keep working after GitHub or Jira is connected. The
   first release includes an administrator JSON export; the move tool is later work.
4. `native-page-actions` → the issue page can comment on, close/reopen, and edit the title,
   description and assignees of native issues. The agent keeps its existing commands.

The answers are recorded in the set as agent-proposed answers carrying this quote; the owner
can confirm them there.

## Later instruction (2026-10-22, during implementation)

> g5d issue comment currently doesn't let you comment on issues that aren't directly attached to the workspace.  We should support commenting on any issue.  It's also required for completeness of vision for a g5 native issue tracker

Interpretation: `g5d issue comment` takes an optional issue argument resolved like
`g5d issue view` (a linked issue's number first, then the creation domain, or a full
reference, with `-R`), defaulting to the primary issue as before. It applies to GitHub and
G5 issues alike; GitHub comments keep their publication guards and records. People's
comments outside Activity are the G5 issue page's (`g5.issue.native.page`); GitHub issues are
commented on in GitHub.

## Later instruction (2026-10-23, search)

> details page issue search doesn't look like it works at all - complains that external issue integration doesn't exist.  This box should use the same code to avoid having multiple issue search mechanisms
>
> I think we need some basic structured search support for native issues.  At minimal we should inject the assignee and state information so keyword search of an assignee will turn up their bugs.

Interpretation: the "details page issue search" is Workspace Details' link form, which only
resolved exact references, so any typed words fell through to the GitHub resolver ("external
issue integration is not configured" without the GitHub App). It becomes a search over the
organization's issue search, the one Home, the Issues tile and Import task already use, with
each result linkable; a pasted reference finds that issue through the same search. Native
search gains words that match an assignee's name or email and the issue's state, plus the
`assignee:`, `author:` and `no:assignee` qualifiers. Design:
[search and the link form](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#structured-native-search-and-one-search-for-linking).
