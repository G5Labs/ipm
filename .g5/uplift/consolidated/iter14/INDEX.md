# iter14 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S14-01 | [module.processor.file-copy.protection](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.protection) | [protection.rs](../../../Verus_verification/consolidated/iter14/protection.rs) `prefix, ancestor_of_root, protected, marker_write, clean, whole_segment_neighbors, lifecycle_neighbors` | src/cls/IPM/Utils/File.cls::IsProtectedDirectory → src/cls/IPM/ResourceProcessor/FileCopy.cls::DoCopy; src/cls/IPM/ResourceProcessor/FileCopy.cls::OnPhase |
| S14-02 | [module.processor.file-copy.unix-overlay](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.unix-overlay) | [protection.rs](../../../Verus_verification/consolidated/iter14/protection.rs) `delete_first, unix_args, unix_copy, copy, request_is_distinct_from_clearing` | src/cls/IPM/Utils/File.cls::CopyDir; src/cls/IPM/ResourceProcessor/FileCopy.cls::DoCopy → src/cls/IPM/ResourceProcessor/FileCopy.cls::OnBeforePhase |
| S14-03 | [module.processor.file-copy.unknown-descendants](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.unknown-descendants) | [protection.rs](../../../Verus_verification/consolidated/iter14/protection.rs) `protected, whole_segment_neighbors, nested_loaded, nested_after` | src/cls/IPM/Utils/File.cls::NormalizeForComparison; src/cls/IPM/Utils/File.cls::DatabaseDirectories → src/cls/IPM/ResourceProcessor/FileCopy.cls::NormalizeNames |
