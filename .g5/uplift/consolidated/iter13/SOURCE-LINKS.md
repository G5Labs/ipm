# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S13-01 | direct | [src/cls/IPM/Main.cls::EnableIPM](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L3553) |
| S13-01 | direct | [src/cls/IPM/Utils/Build.cls::MapPackageEquivalently](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Build.cls#L537) |
| S13-01 | direct | [src/cls/IPM/Utils/Build.cls::MapRoutineEquivalently](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Build.cls#L570) |
| S13-01 | direct | [src/cls/IPM/Utils/Module.cls::AddPackageMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1614) |
| S13-01 | direct | [src/cls/IPM/Utils/Module.cls::AddRoutineMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1673) |
| S13-01 | direct | [src/cls/IPM/Utils/Module.cls::PerformBatchActivation](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1888) |
| S13-01 | reader | [src/cls/IPM/Main.cls::Shell](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L857) |
