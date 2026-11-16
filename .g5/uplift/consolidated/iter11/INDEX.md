# iter11 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S11-01 | [update.steps.shared-execution](../../../ontology/features/update.g5.md#update.steps.shared-execution) | [shared_steps.rs](../../../Verus_verification/consolidated/iter11/shared_steps.rs) `lock_key, begin, enter, finish, same_attempt, serial, shared_key_neighbors` | src/cls/IPM/Storage/Module.cls::HandleAllUpdateSteps; src/cls/IPM/Storage/Module.cls::HandleUpdateStepsFromList; src/cls/IPM/Storage/Module.cls::GetAddUpdateStep; src/cls/IPM/Storage/Module.cls::ExecuteUpdateStep → src/cls/IPM/Lifecycle/Base.cls::%ApplyUpdateSteps; src/cls/IPM/Main.cls::ModuleAction; src/cls/IPM/Storage/UpdateStep/AnyMember.cls::GetUpdateStep |
| S11-02 | [uninstall.class-delete-error](../../../ontology/features/uninstall.g5.md#uninstall.class-delete-error) | [deletion.rs](../../../Verus_verification/consolidated/iter11/deletion.rs) `delete_class, uninstall, refuse_on_error, deletion_neighbors, locked_retains_class` | src/cls/IPM/Lifecycle/Base.cls::%Clean → src/cls/IPM/StudioDocument/Module.cls::Delete; src/cls/IPM/Storage/Module.cls::Uninstall |
