# Members may use the normal shipping controls (#1104)

## Human instructions

The owner (Ben) opened this workspace on
[issue #1104](https://github.com/G5Labs/G5-Lean/issues/1104): an organization
member who owns the source workspace cannot use **Ship to parent**, **Merge when
ready** or **Ship with agent**, and the blocked-ship message misleadingly claims
"Ship with agent resolves it."

On 2026-11-05 the owner settled the design with these decisions:

1. **New merge authority for the source owner.** The current owner of a pull
   request's source workspace, as an active organization member, may merge it
   into its parent whenever the parent has at least one attached approval
   requirement. This applies to every parent, not just Base. Reasoning: whoever
   configures approval requirements on a workspace is in effect granting
   permission for which child workspaces may merge into it. The merge still
   waits on the parent's approvals and every other merge gate (checks,
   external-checks policy, provider rules, conflicts, pinned versions,
   transfer). It is credited to the person. It grants no merge-with-override, no
   GitHub override and no other write to the parent. A parent with no
   requirements delegates nothing: only its existing merge authorities (its
   owner, or administrators for Base) can merge into it.

2. **"Has the approvals" is not part of who may merge.** Approvals stay the
   merge-admission gate for everyone, as today.

3. **Merge when ready and Ship with agent do NOT check merge authority when
   clicked.** G5 checks the authority of the person who turned it on only when
   the merge actually happens, in the background merger. If that person lacks
   authority then (no requirement on the parent, lost ownership or membership,
   pending transfer), G5 turns merge when ready off and says why. Ship with
   agent still needs source-workspace ownership at click, because its agent runs
   only its owner's chats, and a refused request leaves neither a decision nor a
   chat. Agents still never turn merge when ready on. Ship to parent (an
   immediate merge) checks authority up front as usual.

4. **Fix the misleading UI message.** A permission denial must not claim that
   "Ship with agent resolves it"; it should say why, e.g. that the parent has no
   approval requirement.

## Open points the owner asked to keep explicit (assumptions, not decided)

- Only organization administrators attach requirements today
  (`g5.workspace.approval-administration`). So an administrator attaching a
  requirement to someone else's workspace would let that workspace's children
  merge into it without the owner agreeing. Attachment authority is unchanged;
  this is flagged.
- Provisionally, any attached requirement counts, including agent-only
  definitions and definitions that allow self approval. Either kind can let a
  member land work with nobody else involved.
- Merging into another person's live workspace: the existing target-merge path
  already resolves the target's committed/published cut and refuses on target
  head drift; its dirty-workspace-head gate is presently a no-op stub
  (`WorkspaceMergeService::reject_dirty_workspace_head`). Source-owner merge
  reuses that identical body, so it is exactly as safe as a target owner's own
  merge — no better and no worse. Touches `g5.workspace.mutation-authority` and
  `g5.workspace.merge.admin-override.scope-question`.

## Interpretation

- **Implementation is authorized** within the
  [design](../../pr_ontology_diff_design/issue-1104/ontology-plan.md).
- The authority "may person P merge this pull request into target T" gains one
  branch — P currently owns the source and T has ≥1 attached requirement — kept
  in one place so each surface (immediate merge, auto-merge enable, background
  merger) asks the same question.
