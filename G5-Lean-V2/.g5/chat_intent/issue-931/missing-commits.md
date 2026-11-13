# Missing commits during checklist recovery

Human instruction, 2026-10-29: “you should not substitute a new commit but removing commits that no longer exist seems reasonable.” The owner objects to keeping a missing commit checked in a row at the bottom where it may be overlooked.

The [recovery commitment](../../ontology/features/ui.g5.md#g5.workbench.change-selection.recovery) records the design interpretation: remove confirmed missing selections and name them above the checklist. This refines scenario 3 in the [existing mock/design](../../pr_ontology_diff_design/issue-841/ontology-plan.md#stale-checklist-recovery--931-additional-scope); application implementation still awaits the previously requested approval.
