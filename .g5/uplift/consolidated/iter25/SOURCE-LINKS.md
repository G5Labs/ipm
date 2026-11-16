# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S25-01 | direct | [src/cls/IPM/Main.cls::Reinstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2673) |
| S25-01 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S25-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S25-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleFromDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L293) |
| S25-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S25-01 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Reload](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L615) |
| S25-01 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Reload](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L615) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Abstract.cls::%OnNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Abstract.cls#L19) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Abstract.cls::CopyAttributes](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Abstract.cls#L37) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Abstract.cls::ApplyDefaults](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Abstract.cls#L72) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Default/Document.cls::%OnNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L452) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Default/Document.cls::DirectorySet](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L52) |
| S25-02 | direct | [src/cls/IPM/ResourceProcessor/Default/Document.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L59) |
| S25-02 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Reload](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L615) |
| S25-02 | reader | [src/cls/IPM/ResourceProcessor/Default/Document.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L59) |
| S25-03 | direct | [src/cls/IPM/Storage/ResourceReference.cls::NonNullResourceNameOpen](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L344) |
| S25-03 | direct | [src/cls/IPM/Storage/ResourceReference.cls::ProcessorGet](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L64) |
| S25-03 | direct | [src/cls/IPM/Storage/ResourceReference.cls::ResolvedProcessorClass](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L80) |
| S25-03 | direct | [src/cls/IPM/ResourceProcessor/Default/Document.cls::%OnNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L452) |
| S25-03 | reader | [src/cls/IPM/ResourceProcessor/Default/Document.cls::GetSourceControlInfo](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L566) |
| S25-03 | reader | [src/cls/IPM/ResourceProcessor/Default/Document.cls::OnItemRelativePath](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L523) |
| S25-03 | reader | [src/cls/IPM/ResourceProcessor/Default/Document.cls::GetSyncDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L540) |
