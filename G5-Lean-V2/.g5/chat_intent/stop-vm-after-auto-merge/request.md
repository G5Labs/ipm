# Optional VM stop after merge

The owner requested a **Stop VM after merge** option: inject the one-minute shutdown instruction into Ship with agent's prompt only when selected, and also stop the VM when Merge when ready is used without an agent and the option is selected. They explicitly requested UI mocks with alternatives before implementation (2026-11-07).

This supersedes the earlier unconditional Ship with agent prompt instruction. At that stage the delay remained one minute; `g5d workspace stop` stops the current workspace, and `/bin/bash -lc 'sleep 60 && g5d workspace stop'` is its explicit shell form.

The agent proposes default off, one choice per pull request and unchanged immediate Ship to parent behavior for the mock. These are assumptions for review, not owner decisions.

The owner selected **B — Dedicated VM setting**, with the helper text “Stop this workspace after merge. Also applies to Ship with agent.” on one line. This removes the delay from that helper text; it does not change the requested one-minute delay. On narrow screens the sentence may wrap naturally rather than overflow.

The owner then approved the selected mock (“looks good to me”) and requested their build-to-ship workflow to make the PR ready: implement the accepted design, verify it, publish the PR, and address CI, blocking reviewer findings and merge conflicts. The [implementation plan](../../pr_ontology_diff_design/stop-vm-after-auto-merge/ontology-plan.md) retains the mock's explicit assumptions.

## Immediate stop revision (2026-11-09)

The owner rejected the scheduling complexity and explicitly removed the delay: “ignore the 60s delay condition, i.e, on api side, stop the vm right after call func `merge_pull_request_as`”. This supersedes the one-minute behavior above. Keep the approved optional setting and both automatic shipping paths; implement the API follow-up using the normal Stop operation and one `stop_vm_after_merge` column. The agent interprets this as API-owned shutdown for both paths, so the selected prompt describes that behavior without issuing a second stop command.

## Ship to parent revision (2026-11-09)

The owner explicitly extended the selected setting to successful merges through **Ship to parent**, superseding the earlier assumption that direct shipping would leave the VM running. They requested the exact helper “Stop this workspace after merge. Also applies to 'Ship with agent' and 'Ship to parent'.” and an updated mock. Keep immediate normal Stop and the same per-PR choice for all three shipping paths.

## Shared setting layout (2026-11-09)

The owner requested a mock-only revision placing **Stop VM after merge** above **Ship to parent** and **Ship with agent**, removing the long helper, and making clear that the setting applies to every merge option. After reviewing the shared-border mock, they approved the short helper “Applies to all merge options below.” and explicitly authorized implementation. The lightly shaded setting row sits first in the group, followed by both buttons and **Merge when ready**. This replaces the prior below-buttons placement and helper; the setting's behavior and scope remain the same.

## Personal default (2026-11-10)

The owner requested a user-level **Stop VM after merge** default in `?view=settings`, for all their workspaces, with a mock first. They corrected its category to **Execution policy**, as a peer setting **before Idle timeout**, then approved that mock and explicitly asked for implementation and the build-to-ship workflow. The helper is “Default for all your workspaces. You can change it in Prepare to Ship.” The reviewed mock demonstrates the personal default and an independent workspace override.

The implementation interprets this as an account-wide, server-stored default, initially off, used by open PRs without an explicit choice; the source owner's explicit PR choice wins. Existing saved PR values remain explicit choices because the old field cannot distinguish an initial false from an intentional false. Merge fixes the effective choice, and transfer clears the previous owner's choice without allowing an old merged PR to stop the new owner's VM. These compatibility and concurrency rules are agent decisions under the approved request.
