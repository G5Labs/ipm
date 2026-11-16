# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S10-01 | direct | [src/cls/IPM/Storage/ResourceReference.cls::ResolveChildren](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L287) |
| S10-01 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L305) |
| S10-01 | direct | [src/cls/IPM/Storage/Module.cls::GetResolvedReferences](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L1349) |
| S10-01 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Export](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1283) |
| S10-01 | reader | [src/cls/IPM/StudioDocument/Module.cls::Delete](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L205) |
| S10-01 | reader | [src/cls/IPM/Storage/Module.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L727) |
| S10-02 | direct | [src/cls/IPM/ResourceProcessor/Test.cls::OnResolveChildren](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Test.cls#L300) |
| S10-02 | direct | [src/cls/IPM/Storage/Module.cls::HasScope](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L834) |
| S10-02 | direct | [src/cls/IPM/Storage/Module.cls::GetResolvedReferences](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L1349) |
| S10-02 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Export](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1283) |
| S10-02 | direct | [src/cls/IPM/Lifecycle/Base.cls::ExportSingleModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L2030) |
| S10-02 | reader | [src/cls/IPM/Lifecycle/Module.cls::%Package](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Module.cls#L134) |
| S10-02 | reader | [src/cls/IPM/General/Archive.cls::Create](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Archive.cls#L5) |
| S10-02 | reader | [src/cls/IPM/General/Archive.cls::Extract](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Archive.cls#L38) |
| S10-02 | reader | [src/cls/IPM/ResourceProcessor/Test.cls::OnExportItem](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Test.cls#L363) |
| S10-03 | direct | [src/cls/IPM/StudioDocument/Module.cls::UpdatePersistentFromStream](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L26) |
| S10-03 | direct | [src/cls/IPM/Storage/Module.cls::XMLNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L1642) |
| S10-03 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Export](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1283) |
| S10-03 | reader | [src/cls/IPM/Lifecycle/Base.cls::ExportSingleModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L2030) |
