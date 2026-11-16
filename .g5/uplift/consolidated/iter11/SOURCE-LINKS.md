# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S11-01 | direct | [src/cls/IPM/Storage/Module.cls::HandleAllUpdateSteps](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L94) |
| S11-01 | direct | [src/cls/IPM/Storage/Module.cls::HandleUpdateStepsFromList](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L172) |
| S11-01 | direct | [src/cls/IPM/Storage/Module.cls::GetAddUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L259) |
| S11-01 | direct | [src/cls/IPM/Storage/Module.cls::ExecuteUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L223) |
| S11-01 | reader | [src/cls/IPM/Lifecycle/Base.cls::%ApplyUpdateSteps](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1923) |
| S11-01 | reader | [src/cls/IPM/Main.cls::ModuleAction](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2830) |
| S11-01 | reader | [src/cls/IPM/Storage/UpdateStep/AnyMember.cls::GetUpdateStep](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/UpdateStep/AnyMember.cls#L18) |
| S11-02 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Clean](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L305) |
| S11-02 | reader | [src/cls/IPM/StudioDocument/Module.cls::Delete](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/StudioDocument/Module.cls#L205) |
| S11-02 | reader | [src/cls/IPM/Storage/Module.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L727) |
