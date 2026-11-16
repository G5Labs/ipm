# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S12-01 | direct | [src/cls/IPM/Main.cls::EnableIPM](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L3553) |
| S12-01 | direct | [src/cls/IPM/Main.cls::GetMapInfo](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4106) |
| S12-01 | direct | [src/cls/IPM/Main.cls::AddIPMMappedNamespaces](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1628) |
| S12-01 | direct | [src/cls/IPM/Utils/Build.cls::MapPackageEquivalently](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Build.cls#L537) |
| S12-01 | direct | [src/cls/IPM/Utils/Build.cls::MapRoutineEquivalently](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Build.cls#L570) |
| S12-01 | direct | [src/cls/IPM/Utils/Build.cls::MapGlobalEquivalently](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Build.cls#L604) |
| S12-01 | reader | [src/cls/IPM/Utils/Module.cls::AddPackageMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1614) |
| S12-01 | reader | [src/cls/IPM/Utils/Module.cls::AddRoutineMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1673) |
| S12-01 | reader | [src/cls/IPM/Utils/Module.cls::AddGlobalMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1736) |
| S12-01 | reader | [src/cls/IPM/Utils/Module.cls::PerformBatchActivation](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1888) |
| S12-01 | reader | [src/cls/IPM/Main.cls::IsIPMEnabled](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4333) |
| S12-01U | direct | [src/cls/IPM/Main.cls::UnmapIPM](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L3927) |
| S12-01U | direct | [src/cls/IPM/Utils/Module.cls::RemoveRoutineMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1707) |
| S12-01U | direct | [src/cls/IPM/Utils/Module.cls::RemovePackageMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1646) |
| S12-01U | direct | [src/cls/IPM/Utils/Module.cls::RemoveGlobalMapping](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1783) |
| S12-01U | reader | [src/cls/IPM/Main.cls::GetMapInfo](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4106) |
| S12-01U | reader | [src/cls/IPM/Main.cls::Shell](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L857) |
| S12-01U | reader | [src/cls/IPM/Utils/Module.cls::PerformBatchActivation](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1888) |
| S12-02 | direct | [src/cls/IPM/Storage/SystemRequirements.cls::CheckIPMVersion](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/SystemRequirements.cls#L85) |
| S12-02 | reader | [src/cls/IPM/Main.cls::Load](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2415) |
| S12-02 | reader | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S12-03 | direct | [src/cls/IPM/Utils/Module.cls::ConstructInvertedDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1428) |
| S12-03 | direct | [src/cls/IPM/Utils/Module.cls::GetFlatDependencyListFromInvertedDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1457) |
| S12-03 | direct | [src/cls/IPM/General/LockFile.cls::CreateLockFileForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/LockFile.cls#L29) |
| S12-03 | direct | [src/cls/IPM/General/LockFile.cls::AddRepositoryToLockFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/LockFile.cls#L93) |
| S12-03 | direct | [src/cls/IPM/General/LockFile.cls::GetRepo](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/LockFile.cls#L108) |
