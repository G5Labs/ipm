# iter27 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S27-01 | [uninstall.purge.as-built](../../../ontology/features/uninstall.g5.md#uninstall.purge.as-built) | [purge.rs](../../../Verus_verification/consolidated/iter27/purge.rs) `split_from, data_path, set_data, command_data, get, top_key, nested_key, path_neighbor, command_neighbor` | src/cls/IPM/CLI.cls::SetData; src/cls/IPM/CLI.cls::%ParseCommandInput → src/cls/IPM/Main.cls::Uninstall; src/cls/IPM/Storage/Module.cls::Uninstall; src/cls/IPM/Lifecycle/Base.cls::%Unconfigure |
| S27-02 | [uninstall.purge.as-built](../../../ontology/features/uninstall.g5.md#uninstall.purge.as-built) | [purge.rs](../../../Verus_verification/consolidated/iter27/purge.rs) `has_installer, unconfigure, observed_callback, uninstall, plain_neighbor, hook_neighbor, lifecycle_frame, lifecycle_neighbors, prior_called, prior_after` | src/cls/IPM/Lifecycle/Module.cls::%Clean; src/cls/IPM/Lifecycle/Base.cls::%Unconfigure → src/cls/IPM/Lifecycle/Base.cls::%Clean |
| S27-03 | [uninstall.purge.as-built](../../../ontology/features/uninstall.g5.md#uninstall.purge.as-built) | [purge.rs](../../../Verus_verification/consolidated/iter27/purge.rs) `delete_extent, delete_class, reload, seeded, removed, after, row_count, row_value, read, count, value, sdk_before, sdk_after, delete_neighbor, reload_neighbor, reader_neighbor, sdk_neighbor, lifecycle_neighbors` | src/cls/IPM/Lifecycle/Base.cls::%Clean; src/cls/IPM/Storage/Module.cls::Uninstall; src/cls/IPM/StudioDocument/Module.cls::Delete →  |
