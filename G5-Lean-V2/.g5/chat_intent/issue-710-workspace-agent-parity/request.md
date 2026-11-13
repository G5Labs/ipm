# Workspace agent parity: child workspaces and stop by user

Source: [G5-Lean#710](https://github.com/G5Labs/G5-Lean/issues/710), opened by the
owner on 2026-10-22 with a three-part design and audit in its comments. This workspace
was asked to "Work on G5Labs/G5-Lean#710". The authorized branch is
`issue-710-workspace-agent-parity-child-workspaces-personal`, started at `a6e8aa7e`.

## Human request

Verbatim, from the issue:

> Any operation ON A WORKSPACE that is available in the UI should be available to an agent in the workspace VM.

> Agents should have access to child workspaces: fork a child (with a chat prompt), probe state of a child, send new chats, merge a child into its workspace.

> [Personal workspaces] should have access to any workspace owned by the user (let's defer admins having a personal workspace that can operate on everything - I think we'll want an explicit knob here). The "stop workspace and children" mechanism - with the advent of personal workspace, I think we might need to update this to "stop workspaces by user" - which also sounds like a primitive that we'd use in other non-runaway situations.

The bracketed subject replaces "They", which referred to personal workspaces from
[PR #645](https://github.com/G5Labs/G5-Lean/pull/645).

## Human instructions recorded by earlier sessions

The issue records these as paraphrases, not quotes:

- The parity audit allows a small number of exceptions, such as logout,
  security-related actions and approval on behalf of a human.
- Review only new capabilities. An agent can already overwrite its workspace and
  branch, so doing the same damage through a child is not a new risk.
- Add no new permission system: no per-agent child labels or grants; names may be
  enough to organize children. Quota evasion through fresh workspaces applies equally
  to humans and agents.
- Limit platform chat creation and modification to children. Native local sub-agents
  cover same-workspace delegation.
- An agent may delete a child workspace but must never delete its own.
- The earlier containment rule, "stop workspace and children", re-enabled forks
  implicitly when the root started again, with no separate unfreeze. Stop by user
  replaces that mechanism.

## Decisions in this workspace

On 2026-10-22 the owner asked: "Should an agent be able to read the output of a chat
in another workspace? I think it needs to. Is this included in the design?" The
design then covered only direct children. Offered choices, the owner selected:

- **Read scope:** "Any workspace the owner can read" — whatever the owner could read
  in the UI, the agent can read, including other people's visible workspaces;
  mutations stay limited to self and children.
- **Content:** "Messages and timeline, including tool I/O" — the same as the UI shows
  the owner.

The owner then answered the issue's open decisions in the question set
`issue-710-owner-decisions` (each `decision: true`):

1. **Personal-agent deletion:** allow deleting any owned workspace, through the
   ordinary delete checks.
2. **Personal-agent fork sources:** Base plus workspaces the user owns.
3. **Credential under owner scope:** the existing VM credential with server scope
   checks and per-operation audit.
4. **Formal agent review requests:** allow; the confined approver, not the
   workspace agent, produces the verdict.
5. **Linked issue close/reopen:** allow agents to close and reopen. This differs
   from the recommendation and amends `g5.external-issues.transition`.
6. **Agent access to a child's preview** (answered 2026-10-26): "Add a task-bound,
   non-browser preview credential" ("The parent's VM credential mints a short-lived
   token bound to one child preview task under agent scope"). This differs from the
   recommendation (no access). The owner then asked: "I recently answered a question
   about access to workspace previews from other workspaces by agents. Implement it."

On 2026-10-26, after hearing conflicting accounts of whether a workspace agent could
change workspaces other than its children, the owner asked for the scope rule to be
checked in the code, then instructed:

> Update the target check: target workspace owner is the same as current workspace
> owner. (otherwise personal workspaces are blocked from managing the platform for the
> user - a core use case)

The owner also reported having forked Base from a personal workspace's agent in a
preview of this branch; the branch's fork route then forked only the caller, which a
personal workspace cannot be, so the agent could not confirm that report from the code.

On 2026-10-23 the owner asked whether the agent can edit individual files in a child
(the UI can create, modify and delete files on a running workspace VM) and whether
agent operations are tracked like UI actions for auditing. After the answer (no file
routes; a separate `workspace_agent.action` event emitted only on success, missing on
PR refresh), the owner asked:

> audit refused and failed attempts as well as successes and add the missing
> PR-refresh event.
> Add support for the file routes to the agent (appropriately audit logged)

On 2026-10-26 the owner asked to close two more gaps for agents:

> 1) Fork a workspace and associate a primary issue. This is accessible from the UI
> but there isn't a route from the command line. adding it to 'g5d workspace fork'
> seems reasonable. or 'g5d issue link' needs to be able to work on other workspaces.
> 2) There's no way for an agent to look up users on the platform - this is useful
> for assigning issues. Consider some 'g5d user' command that would support search

Asked whether results should carry each member's connected GitHub login (new to
other members; the UI shows a person only their own), the owner answered: "include
login, but do so in a provider agnostic way."

The owner then asked:

> 'g5d user' should have a json option similar to 'gh' commands which returns user
> details (for when we have more interesting details)

## Interpretation

These are agent design choices, recorded in the
[design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md):

- "Child" first meant a direct child in the same organization with the same current
  owner, rechecked on every request. Since the owner's 2026-10-26 instruction the
  operation target is any live workspace of the same organization and current owner
  (owner scope), for every calling workspace, not only personal ones; Base and review
  copies stay excluded as targets, Base stays a fork source (decision 2). Platform operations only; Git reads keep the
  [issue 423 decision](../issue-423-git-read-boundary/request.md).
- Stop by user generalizes the member-deactivation cleanup into a per-membership
  execution hold that clears itself once everything has settled. That keeps
  "starting again re-enables" without a fork freeze.
- The callers are the user, any of their workspace agents and an organization
  administrator. The organization owner is not exempt.
- Personal owner scope was deferred until PR #645 landed (it has since merged). The
  owner's decisions on deletion, fork sources and credential exposure are recorded
  under [Decisions in this workspace](#decisions-in-this-workspace).
