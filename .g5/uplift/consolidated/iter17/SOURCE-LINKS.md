# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S17-01 | direct | [src/cls/IPM/ResourceProcessor/Test.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Test.cls#L93) |
| S17-01 | direct | [src/cls/IPM/ResourceProcessor/Test.cls::ReportTestResults](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Test.cls#L240) |
| S17-01 | direct | [src/cls/IPM/Test/Manager.cls::RunTest](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Manager.cls#L7) |
| S17-01 | direct | [src/cls/IPM/Test/Manager.cls::SaveResult](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Manager.cls#L16) |
| S17-01 | direct | [src/cls/IPM/Test/Abstract.cls::ToFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Abstract.cls#L9) |
| S17-01 | reader | [src/cls/IPM/Test/JsonOutput.cls::WriteToFileStream](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/JsonOutput.cls#L4) |
| S17-01 | reader | [src/cls/IPM/Test/JsonOutput.cls::OutputToDevice](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/JsonOutput.cls#L11) |
| S17-01 | reader | [src/cls/IPM/Test/Manager.cls::GetAllTestsStatus](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Manager.cls#L58) |
| S17-01 | reader | [src/cls/IPM/Main.cls::Shell](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L857) |
| S17-02 | direct | [src/cls/IPM/Test/Abstract.cls::BuildResultTree](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Abstract.cls#L103) |
| S17-02 | direct | [src/cls/IPM/Test/Abstract.cls::GetSummary](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Abstract.cls#L199) |
| S17-02 | reader | [src/cls/IPM/Test/Manager.cls::GetAllTestsStatus](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/Manager.cls#L58) |
| S17-02 | reader | [src/cls/IPM/Test/JsonOutput.cls::OutputToDevice](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Test/JsonOutput.cls#L11) |
