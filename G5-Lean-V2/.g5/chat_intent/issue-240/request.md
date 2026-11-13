# Issue 240 implementation request

## Human instructions

The user asked to start work on [GitHub issue 240](https://github.com/G5Labs/G5-Lean/issues/240) in the `G5-Lean-github-app` worktree. The issue says the G5 branch [`g5/feat/ben_force_pull-deaddc18`](https://github.com/G5Labs/G5/tree/g5/feat/ben_force_pull-deaddc18) "has a better UI for remote branch management" and asks to pull it into Lean.

That branch has two separable changes: a backend change that adopts an externally rebased remote inbound and offers an ambiguous-direction chooser, and a Workspace Details Repositories redesign. Asked which to port, the user chose **UI redesign only**.

The source branch's PR body records the user's answers to the redesign's open questions: repository detail is collapsed by default, ahead/behind stays binary, and repositories are stacked cards. Its final ontology (`center-panel#D77`) keeps mutations out of the card head and shows explicit loading/unavailable states.

## Agent interpretation

Port the presentation only: an always-visible full-text status pill, in-flow disclosure of the existing tracking detail, explicit unknown state, each action rendered once, and design tokens. Lean's current tracking states, actions, preconditions, hooks and API stay unchanged. The rebased-forward and `ambiguous_rewrite` backend lanes and their chooser are out of scope.
