# Agent permission to modify other workspaces

## Human instructions

On 2026-10-23 the owner asked, while PR #712 (G5-Lean#710) was open:

> Let's add a knob that determines whether an agent in a workspace has permission
> to modify other workspaces (essentially a way to opt in to some of the more
> dangerous behavior). Obviously, the knob shouldn't be settable by the agent
> itself. Forking a workspace with the knob set should inherit the setting.
> Transferring ownership should clear it. If unset, then fork a child is blocked
> (makes a new workspace). However, g5d workspace merge of self should not be
> blocked by the knob.

On 2026-10-26, after personal workspaces (G5-Lean#624) reached main, the owner
added:

> personal workspaces have landed.  A personal workspace should default to
> agents-can-modify-other-workspaces.

Later on 2026-10-26, before the branch shipped, the owner widened that:

> change: default enable the knob for all workspaces. (this branch has never
> been shipped anywhere, so you can modify db migrations that were added here)

## Interpretation

These are agent choices, recorded in the
[design](../../pr_ontology_diff_design/agent-other-workspaces/ontology-plan.md):

- The knob is a workspace setting, on by default for every workspace, existing
  ones included (the column default fills them); only the workspace's current
  owner changes it, from a person session, like any other workspace
  configuration. Transfer still turns it off, as first instructed.
- "Modify other workspaces" means every agent operation whose target is not the
  agent's own workspace: forking a child and every child operation (start,
  stop, delete, chats and their controls, tasks, approvals, previews, publishing
  a child's pull request). Reads are not modifications and stay available.
- "Merge of self" is `g5d workspace merge --pull-request PR`: merging a pull
  request into the agent's own workspace. Other operations on the agent's own
  workspace (stop itself, approvals, its pull requests, issue state, its
  preview) are unaffected too.
- Stop-all (`g5d workspace stop-all`) stays available without the knob: it is
  the owner's runaway brake (`g5.story.stop-workspaces-by-user`), not an opt-in
  capability.
- A fork copies the knob only when the new workspace has the source's owner;
  a fork owned by someone else starts with the default, on (otherwise every
  member's fork of a Base they do not own would start off, defeating the
  default). A formal review workspace starts with it off: its agent is the
  confined approver and never coordinates other workspaces.
