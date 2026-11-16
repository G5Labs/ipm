# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S15-01 | direct | [src/cls/IPM/Utils/EmbeddedPython.cls::SortOCITags](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/EmbeddedPython.cls#L59) |
| S15-01 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::GetAllTags](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L927) |
| S15-01 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::TagForReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L270) |
| S15-01 | reader | [src/cls/IPM/Repo/Oras/PackageService.cls::AggregatePlatformVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L299) |
| S15-02 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::AggregatePlatformVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L299) |
| S15-02 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::PreferCompatibleVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L331) |
| S15-02 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::AddModulesFromTags](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L371) |
| S15-02 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::ListModulesFromTagString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L353) |
| S15-02 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::ListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L489) |
| S15-02 | reader | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S15-02 | reader | [src/cls/IPM/Storage/ModuleInfo.cls::IsCompatibleWithPlatform](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleInfo.cls#L13) |
| S15-02 | reader | [src/cls/IPM/Utils/Module.cls::GetModuleListExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L905) |
| S15-02 | reader | [src/cls/IPM/Repo/Oras/PackageService.cls::GetModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L233) |
| S15-02 | reader | [src/cls/IPM/Repo/Utils.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Utils.cls#L8) |
| S15-03 | direct | [src/cls/IPM/Storage/Module.cls::BuildDependencyGraph](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/Module.cls#L898) |
| S15-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadDependencies](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1368) |
| S15-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S15-03 | reader | [src/cls/IPM/Main.cls::Load](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2415) |
| S15-03 | reader | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S15-04 | direct | [src/cls/IPM/General/SemanticVersion.cls::FromString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L33) |
| S15-04 | direct | [src/cls/IPM/Utils/EmbeddedPython.cls::CompareVersionTotalOrder](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/EmbeddedPython.cls#L7) |
| S15-04 | direct | [src/cls/IPM/Utils/EmbeddedPython.cls::SortVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/EmbeddedPython.cls#L43) |
| S15-04 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::AggregatePlatformVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L299) |
| S15-04 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S15-04 | reader | [src/cls/IPM/Utils/Module.cls::GetModuleListExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L905) |
| S15-04 | reader | [src/cls/IPM/Repo/Oras/PackageService.cls::TagForReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L270) |
| S15-04 | reader | [src/cls/IPM/Repo/Oras/PackageService.cls::GetModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L233) |
