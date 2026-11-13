# Merge when ready

## Human instructions

On 2026-10-24 the owner asked, while PR #712 (G5-Lean#710) was open:

> Let's explore supporting "merge myself to parent" from the agent. It's currently
> unsupported. But we want to support automation on that final "fix ci, rebase loop"
> so that once the merge is green the agent can finish it for you. Obviously you'd
> need full approvals for the merge to succeed. Is it reasonable to just include some
> agent guidance on only doing so with explicit clearance from the user?

The agent answered that guidance alone is not a sufficient control (a target with no
attached requirements admits a merge without any human; a child's "user" is its
parent agent; data the agent reads can look like clearance) and proposed a per-PR
"merge when ready" knob that only a person can turn on. The owner replied:

> I like the merge when ready knob on the PR. It looks like github has a similar
> mechanism. Can you check the 'gh' tool to see what syntax it uses so we can match it?

The agent reported `gh pr merge [<number> | <url> | <branch>] --auto | --disable-auto`,
`--match-head-commit SHA` and the `autoMergeRequest` JSON field, proposing that a
VM credential may never turn it on, may turn it off, and that `--admin` and the
merge-method flags are omitted. The owner replied:

> let's ignore --match-head-commit for the moment. Otherwise, looks good - design and build it

## Interpretation

Agent choices, recorded in the
[design](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md):

- As with GitHub's auto-merge, G5 itself performs the merge once the pull request is
  ready; the agent's part is keeping CI green and the branch rebased. The agent never
  gains authority to merge into its parent.
- "Green" means a passing server-side observation of external checks, required for
  this merge even when the organization does not require it for manual merges, plus
  the merge's ordinary admission (approvals, mergeability).
- Turning it on is a decision of a person who may merge the pull request; later
  pushes keep it on, so the fix-and-rebase loop does not need a new decision.

## Follow-up: knowing it merged, and Merge with agent

On 2026-10-24 the owner asked:

> how does the user know that the merge occurred? how does the agent know that it occurred?

The agent found that neither is told (a merge publishes no pull-request update,
notification or agent notice) and proposed: publish the update to both
workspaces; notify the person who turned it on; a platform notice to the source
workspace's active agent chats; guidance for the agent's wait loop; and not to
wake an idle agent. The owner replied:

> go ahead. the "merge when ready" button - consider whether that can be a "Merge
> with Agent" button that enables auto-merge and then launches a chat to ensure
> that the CI is passing and the branch is rebase-mergable (in a loop).

Interpretation (agent): Merge with agent becomes the primary button; turning on
merge when ready without an agent stays available as a secondary choice, because
a person may want no compute spent or may not own the source workspace. The
agent's loop ends when the pull request merges or merge when ready is turned off.

## Follow-up: the Merge with agent prompt

On 2026-10-25 the owner reported that the prompt "doesn't work well": it paired
rebasing with `g5d pr update`, which refuses rewritten history. They supplied a
replacement that publishes a rebase with
`git push --force-with-lease=refs/heads/<branch>:$OLD origin HEAD:refs/heads/<branch>`,
keeps `g5d pr update` for appended fixes, and leads with "G5 does the merging; keep
it mergeable and green, then wait". Adopted verbatim except that the agent fetches
before recording `OLD`, so the lease names the current published head.

## Follow-up: two ship controls

On 2026-10-26 the owner asked:

> Let's adjust the buttons for 'Ship to parent'. Just two: 'Ship to parent', which if
> it encounters any blockers (CI failures, merge conflicts, etc) offers 'Ship with
> agent' as the resolution. This is the default path - keeps the "everything green"
> path uncomplicated. The other button is enable/disable auto-merge

Interpretation (agent): a blocker is anything that keeps Ship to parent from merging
now with an open pull request: conflicts or a stale branch, failing or pending checks
the organization requires, missing approvals, or a refusal returned by the merge itself.
"Merge with agent" is renamed "Ship with agent" and offered only then; "Start an agent"
is dropped. Before a pull request exists, or while its admission is still being read,
Ship to parent is unavailable with its reason and no agent is offered.

## Follow-up: three ship controls

On 2026-10-26 the owner asked:

> Let's go with 3 buttons: always have a "Ship with Agent" button also

Interpretation (agent): Ship to parent stays visible and is disabled with its blocker
named; Ship with agent is shown beside it whenever a pull request is open, secondary
while nothing blocks and primary when something does; the merge when ready switch is
the third. This also covers failing checks the organization treats as advisory: Ship to
parent still merges, and Ship with agent is there to fix them first.
