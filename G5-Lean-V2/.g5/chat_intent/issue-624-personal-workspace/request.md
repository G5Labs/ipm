# Personal workspace: per-user chat and persistent artifacts from Home

Session of 2026-10-18 (Claude Code). Design:
[ontology-plan](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md).

## Human instruction

> Work on G5Labs/G5-Lean#624: Personal workspace: per-user chat and persistent artifacts
> from Home

The issue, opened 2026-10-17 by @bvandiver-g5, is the durable source of the design:
[G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624). Its goal, quoted:

> Give each organization member a personal place to chat with an agent and keep artifacts
> that is not tied to a specific piece of work. Users can:
> - chat from Home without choosing or creating a workspace;
> - keep what the agent writes across chats, Stops and Starts;
> - run that chat against the organization's existing codebases;
> - show selected personal files to another person without exposing the rest.

It names one open question, `g5.security.delegated-read.exposure`, that blocks delegated
reads only, with a working recommendation of "(a) with audit".

## Agent interpretation

- The issue says its design was "recorded locally but not yet committed"; no copy reached
  this branch or any remote branch. This plan reconstructs it from the issue text.
- The issue's amended block IDs (`g5.workbench.deferred`, `g5.lifecycle.start.compound`,
  `g5.fork.names`, …) come from the ontology replaced in `5c66d337` ("Rebuild ontology
  from Ben's verbatim human-provided statements"). The plan maps each to the current block
  that carries the same commitment, or adds a block where none does.
- The issue's text is treated as the owner's intent for the commitments it states. Where it
  contradicts a current human-provided block (owner removal, every workspace includes the
  docs repo, every non-Base workspace has a parent, owner delete), the plan records an
  explicit exception rather than silently editing the human wording.
- "Work on" is read as: settle the design against the current ontology and code, then
  implement in reviewable slices.

## Later decisions

2026-10-18, after reading the design:

> Can we defer delegated read?  With access to the git repos, I think most tasks are
> already accomplishable.

Interpretation: delegated reads of platform records (message text, timeline, attention)
are out of scope for this work; the open exposure question is moot until they are revived.

Then, on the four questions (delegated-read exposure, handle source, rejoin, reference
derivation) and the stop-reason recommendation:

> I accept all your recommended answers to the questions - keep delegated reads deferred.

Interpretation: the handle is the frozen, normalized email local part; a rejoining member
gets a fresh personal workspace; reference checkouts are derived from Base at each Start;
retirement uses a distinct `owner_removed` stop reason; delegated reads stay deferred.
These settle the design's open decisions. They do not by themselves start implementation.

2026-10-19, after the Home panel shipped as a conversation list and composer:

> Can we have a side panel on Home where you can chat just like it's the workspace?

Interpretation: Home gets a side panel holding the personal workspace's full Chat,
the same `ChatPanel` the workbench mounts (sessions, streaming replies, tool
activity, queued inputs, approvals, Stop, Start and send), driven by the same chat
data hooks and the same live gate. The main column keeps the personal workspace's
status and My files.

2026-10-19, after the side panel shipped:

> The personal workspace file display (and copy-out mechanism) clutters the home page.
> We either need to tuck this into the chat panel on the side, or into the workspace
> view itself.

Interpretation: files and Copy to workspace leave Home for the personal workbench,
which already has the committed-file tree and both copy entry points. The side
panel's header takes the status and the admin-visibility notice, so on wide screens
Home shows no main-column personal panel; the compact panel remains below that width,
where the side panel is hidden.

2026-10-19, replacing copy-out with a mount. The owner, in sequence:

> Maybe we have copy-out backwards. What if we allowed the personal repo to be attached
> to workspaces by owner? Then copy out isn't a special operation at all.

> Let's punt on sharing for the moment, and assume that users will use issues to push
> ideas out of the personal workspace (an alternative way to capture intent). […] What
> if we just implicitly mount a checkout of the owner's personal repo when a workspace
> is started. It's not a repo that is "officially" associated with the workspace. UI
> doesn't render it.

> I am less worried about notes escaping from your personal repo - I think that's often
> the goal: users can customize how G5 works for them by putting notes into the personal
> repo and then pulling them easily into the workspaces they work on. We do need a write
> strategy in non-personal workspaces - can we commit to the personal repo? Does the
> "flush" operation ignore personal repo checkouts?

> I like write strategy 1. Update the design, build it, and rip out the existing "copy
> out" mechanism that it replaces.

Write strategy 1, as the agent proposed it: the owner's other workspace VMs push
straight to the personal branch, update-only and fast-forward only, with Git's old-head
check final; the push counts as a move of the personal workspace's branch; a running
personal VM fetches and fast-forwards when its branch moves; a stopped personal workspace
holding flushed, unpublished work refuses the push, as copy-out's stopped path did. The
mount is left out of flush.

Interpretation: sharing selected files with another person leaves the purpose for now;
tracker issues are the interim route, with no new mechanism. Copy-out, its route, dialog
and entry points are removed rather than kept beside the mount.
