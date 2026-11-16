# iter17 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S17-01 | [build.test-output.run-identity](../../../ontology/features/build-and-publish.g5.md#build.test-output.run-identity) | [report_identity.rs](../../../Verus_verification/consolidated/iter17/report_identity.rs) `complete, consume, device_index, file_index, snapshot, export_file, report, nested_state, symbolic_order_neighbors, explicit_forwarding_preserves_reader` | src/cls/IPM/ResourceProcessor/Test.cls::OnPhase; src/cls/IPM/ResourceProcessor/Test.cls::ReportTestResults; src/cls/IPM/Test/Manager.cls::RunTest; src/cls/IPM/Test/Manager.cls::SaveResult; src/cls/IPM/Test/Abstract.cls::ToFile → src/cls/IPM/Test/JsonOutput.cls::WriteToFileStream; src/cls/IPM/Test/JsonOutput.cls::OutputToDevice; src/cls/IPM/Test/Manager.cls::GetAllTestsStatus; src/cls/IPM/Main.cls::Shell |
| S17-02 | [build.test-output.counted-rows](../../../ontology/features/build-and-publish.g5.md#build.test-output.counted-rows) | [report_identity.rs](../../../Verus_verification/consolidated/iter17/report_identity.rs) `passed, error, decode_entries, decode, summary, entry_log, classified, device_failures, raw_neighbors` | src/cls/IPM/Test/Abstract.cls::BuildResultTree; src/cls/IPM/Test/Abstract.cls::GetSummary → src/cls/IPM/Test/Manager.cls::GetAllTestsStatus; src/cls/IPM/Test/JsonOutput.cls::OutputToDevice |
