# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S27-01 | direct | [src/cls/IPM/CLI.cls::SetData](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/CLI.cls#L809) |
| S27-01 | direct | [src/cls/IPM/CLI.cls::%ParseCommandInput](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/CLI.cls#L408) |
| S27-01 | reader | [src/cls/IPM/Main.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2701) |
| S27-01 | reader | [src/cls/IPM/Storage/Module.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L727) |
| S27-01 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Unconfigure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L545) |
| S27-02 | direct | [src/cls/IPM/Lifecycle/Module.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Module.cls#L8) |
| S27-02 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Unconfigure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L545) |
| S27-02 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L305) |
| S27-03 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L305) |
| S27-03 | direct | [src/cls/IPM/Storage/Module.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L727) |
| S27-03 | direct | [src/cls/IPM/StudioDocument/Module.cls::Delete](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L205) |
