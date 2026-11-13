# Merge with override (#873, #550)

## Human instructions

The owner opened this workspace on [issue #873](https://github.com/G5Labs/G5-Lean/issues/873).

On 2026-10-27, after the first design, the owner wrote:

> also read https://github.com/G5Labs/G5-Lean/issues/550. link the issue to this
> workspace and update the design to include it

Later the same day, the owner answered the workspace question set
`admin-merge-override` (docs repository,
`docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`). Their answers:

- **scope:** only where the admin can already merge.
- **self-override:** allowed, flagged as a self-override.
- **opt-out:** not in the first version.
- **confirmation:** a confirmation checkbox, not typing the workspace name.
- **provider-identity:** every merge refuses a GitHub-blocked repository,
  enforced.
- **github-override-cli:** browser-only, like the G5 override.

They then wrote:

> Answered questions. During implementation, be sure to keep the auth-path for who
> is allowed to override tight so a future permission change for who is allowed to
> do this is a small and localized change.

## Interpretation

- **Implementation is authorized** within the
  [design](../../pr_ontology_diff_design/issue-873/ontology-plan.md).
- **One authority rule.** A single policy function decides who may waive each group
  of blockers (G5 requirements, GitHub rules). The merge route, the override preview
  and the UI's offer all consult it, and none of them re-derives role checks.
  Changing who may override then means editing that one function and its tests.
