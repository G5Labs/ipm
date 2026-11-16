# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S9-01 | direct | [src/cls/IPM/Storage/ResourceReference.cls::GetChildren](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L114) |
| S9-01 | direct | [src/cls/IPM/Storage/ResourceReference.cls::ResolveChildren](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ResourceReference.cls#L287) |
| S9-01 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L305) |
| S9-01 | reader | [src/cls/IPM/Storage/Module.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L727) |
| S9-01 | reader | [src/cls/IPM/StudioDocument/Module.cls::Delete](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L205) |
| S9-02 | direct | [src/cls/IPM/Storage/Module.cls::GetAddUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L259) |
| S9-02 | direct | [src/cls/IPM/Storage/Module.cls::SeedUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L203) |
| S9-02 | direct | [src/cls/IPM/Storage/UpdateStep/AnyMember.cls::GetUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/UpdateStep/AnyMember.cls#L18) |
| S9-02 | direct | [src/cls/IPM/Storage/UpdateStep/PrimaryOnly.cls::GetUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/UpdateStep/PrimaryOnly.cls#L24) |
| S9-02 | reader | [src/cls/IPM/Lifecycle/Module.cls::%Activate](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Module.cls#L95) |
| S9-02 | reader | [src/cls/IPM/StudioDocument/Module.cls::Delete](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L205) |
