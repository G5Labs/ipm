# iter9 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S9-01 | [uninstall.package-ownership](../../../ontology/features/uninstall.g5.md#uninstall.package-ownership) | [ownership.rs](../../../Verus_verification/consolidated/iter9/ownership.rs) `protects, claimed, children, module_children, clean, package_neighbors` | src/cls/IPM/Storage/ResourceReference.cls::GetChildren; src/cls/IPM/Storage/ResourceReference.cls::ResolveChildren; src/cls/IPM/Lifecycle/Base.cls::%Clean → src/cls/IPM/Storage/Module.cls::Uninstall; src/cls/IPM/StudioDocument/Module.cls::Delete |
| S9-02 | [update.steps.records](../../../ontology/features/update.g5.md#update.steps.records) | [ownership.rs](../../../Verus_verification/consolidated/iter9/ownership.rs) `get_add, seed, delete_module, both_moved, cascade_neighbors` | src/cls/IPM/Storage/Module.cls::GetAddUpdateStep; src/cls/IPM/Storage/Module.cls::SeedUpdateStep; src/cls/IPM/Storage/UpdateStep/AnyMember.cls::GetUpdateStep; src/cls/IPM/Storage/UpdateStep/PrimaryOnly.cls::GetUpdateStep → src/cls/IPM/Lifecycle/Module.cls::%Activate; src/cls/IPM/StudioDocument/Module.cls::Delete |
