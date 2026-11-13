# Preview selection follow-up

Human direction in chat, 2026-10-25, for issue #750 / PR #754:

- After changing the left checklist and clicking Show Changes, clear the pending-selection warning when that selection is applied.
- Remove the fixed comparison-direction subtitle from the center pane; the comparison depends on the selected commits.
- Use the same left selector when the VM is stopped. Keep Unpushed visible but greyed out, with contextual help explaining why it is unavailable.
- Remove the Clear control from the Source Control menu; retain the All changes checkbox for selecting and clearing changes across repositories.
- All changes also selects and clears the File types choices, treating them like the other changes in the checklist.

The supplied example is the `g5labs/e2e-test` workspace in preview task `7d686cf4-2606-4b5e-8170-9799f8e90b71`, opened through a captured pushed-scope link. Its current empty comparison exposed a type-only apply being blocked by the absence of commit choices. The exception for applying types to that existing snapshot is an implementation decision recorded in the existing ontology plan.

The subsequent request adds two preview fixtures forked from Base, each with pushed commits, local commits and unpushed working-tree changes. One has no PR; the other has a pushed PR. These supplement the existing empty example so the owner can exercise real commit lists and Save changes.
