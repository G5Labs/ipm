# Approvals says when the workspace merges when ready (2026-11-06)

## Human instruction

After issue [#1095](https://github.com/G5Labs/G5-Lean/issues/1095) was implemented, the owner asked for the Approvals
banner to mention when "merge when ready" is on ("This workspace will merge automatically when all approval requirements
are met", concise so it takes little room), or instead to put it at the top of the Approvals page so that it is visible
not just to approvers but to anybody visiting the workspace, with mocks for approval before implementation.

From mocks rendered on the real Approvals page (A: a line in the personal banner; B: a notice strip at the top;
C: one muted line under "Approvals Required to merge into …"; D: a pill beside that heading with the sentence as a
tooltip), the owner chose **C**.

## Interpretation

The line reuses Prepare to Ship's wording ("once checks pass and approvals are met"), because a merge when ready also
waits for checks. It is read-only: Prepare to Ship keeps the switch, and nothing about merge admission changes.
