# iter13 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S13-01 | [namespace.enable.partial-batch](../../../ontology/concepts/namespaces.g5.md#namespace.enable.partial-batch) | [batch.rs](../../../Verus_verification/consolidated/iter13/batch.rs) `visit, run, publish, selected_neighbors` | src/cls/IPM/Main.cls::EnableIPM; src/cls/IPM/Utils/Build.cls::MapPackageEquivalently; src/cls/IPM/Utils/Build.cls::MapRoutineEquivalently; src/cls/IPM/Utils/Module.cls::AddPackageMapping; src/cls/IPM/Utils/Module.cls::AddRoutineMapping; src/cls/IPM/Utils/Module.cls::PerformBatchActivation → src/cls/IPM/Main.cls::Shell |
