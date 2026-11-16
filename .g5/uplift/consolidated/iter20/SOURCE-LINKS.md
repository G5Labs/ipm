# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S20-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S20-01 | direct | [src/cls/IPM/ResourceProcessor/Default/Document.cls::OnLoad](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L332) |
| S20-01 | direct | [src/cls/IPM/Storage/ResourceReference.cls::UpdateChildrenFromString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L242) |
| S20-01 | reader | [src/cls/IPM/Storage/ResourceReference.cls::NonNullResourceNameExists](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L323) |
| S20-02 | direct | [src/cls/IPM/Storage/ResourceReference.cls::NonNullResourceNameExists](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L323) |
| S20-02 | direct | [src/cls/IPM/ExtensionBase/Utils.cls::FindHomeModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ExtensionBase/Utils.cls#L38) |
| S20-02 | reader | [src/cls/IPM/ExtensionBase/Utils.cls::GetHomeModuleName](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ExtensionBase/Utils.cls#L32) |
| S20-02 | reader | [src/cls/IPM/Utils/Module.cls::OrphanedResourcesExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L2144) |
| S20-03 | direct | [src/cls/IPM/Utils/Module.cls::OrphanedResourcesExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L2144) |
| S20-03 | direct | [src/cls/IPM/Utils/Module.cls::OrphanedResourcesFetch](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L2174) |
| S20-03 | direct | [src/cls/IPM/Utils/Module.cls::OrphanedResourcesClose](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L2193) |
| S20-03 | reader | [src/cls/IPM/Main.cls::ListOrphans](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L3268) |
