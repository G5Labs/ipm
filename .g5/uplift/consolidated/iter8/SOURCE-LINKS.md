# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S8-01 | direct | [src/cls/IPM/Storage/ModuleTemplate.cls::SetJWTProps](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleTemplate.cls#L338) |
| S8-01 | direct | [src/cls/IPM/Storage/ModuleTemplate.cls::AddWebApps](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleTemplate.cls#L182) |
| S8-01 | direct | [src/cls/IPM/Storage/ModuleTemplate.cls::SetSourcePathForCSPApp](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleTemplate.cls#L240) |
| S8-01 | direct | [src/cls/IPM/Storage/ModuleTemplate.cls::ProcessResources](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleTemplate.cls#L530) |
| S8-01 | direct | [src/cls/IPM/Storage/ModuleTemplate.cls::SaveFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleTemplate.cls#L650) |
| S8-01 | reader | [src/cls/IPM/Main.cls::GenerateModuleXML](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1420) |
| S8-01 | reader | [src/cls/IPM/ResourceProcessor/WebApplication.cls::CreateOrUpdateWebApp](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/WebApplication.cls#L108) |
| S8-01 | reader | [src/cls/IPM/ResourceProcessor/FileCopy.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L216) |
| S8-02 | direct | [src/cls/IPM/ResourceProcessor/WebApplication.cls::OnExportItem](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/WebApplication.cls#L207) |
| S8-02 | reader | [src/cls/IPM/Lifecycle/Base.cls::ExportSingleModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L2030) |
| S8-02 | reader | [src/cls/IPM/Lifecycle/Module.cls::%Package](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Module.cls#L134) |
| S8-02 | reader | [src/cls/IPM/Lifecycle/Base.cls::%Publish](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1580) |
| S8-02 | reader | [src/cls/IPM/ResourceProcessor/WebApplication.cls::CreateOrUpdateWebApp](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/WebApplication.cls#L108) |
| S8-03 | direct | [src/cls/IPM/Repo/Utils.cls::moduleSqlToList](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Utils.cls#L70) |
| S8-03 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::ListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L12) |
| S8-03 | reader | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S8-03 | reader | [src/cls/IPM/Repo/Utils.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Utils.cls#L8) |
| S8-04 | direct | [src/cls/IPM/Storage/ModuleInfo.cls::IsCompatibleWithPlatform](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleInfo.cls#L13) |
| S8-04 | direct | [src/cls/IPM/Repo/Oras/PackageService.cls::AggregatePlatformVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L299) |
| S8-04 | reader | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S8-04 | reader | [src/cls/IPM/Utils/Module.cls::GetModuleListExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L905) |
| S8-04 | reader | [src/cls/IPM/Repo/Oras/PackageService.cls::TagForReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PackageService.cls#L270) |
| S8-04 | reader | [src/cls/IPM/Storage/ModuleInfo.cls::FormatPlatformVersions](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/ModuleInfo.cls#L22) |
| S8-05 | direct | [src/cls/IPM/ResourceProcessor/PythonWheel.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L16) |
| S8-05 | reader | [src/cls/IPM/ResourceProcessor/PythonWheel.cls::OnResolveChildren](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L102) |
| S8-05 | reader | [src/cls/IPM/Lifecycle/Base.cls::InstallOrDownloadPythonRequirements](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L753) |
| S8-05 | reader | [src/cls/IPM/Utils/Module.cls::PipInstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L2485) |
| S8-06 | direct | [src/cls/IPM/Lifecycle/Base.cls::%Publish](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Lifecycle/Base.cls#L1580) |
| S8-06 | reader | [src/cls/IPM/Repo/Oras/PublishService.cls::PublishModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Oras/PublishService.cls#L6) |
| S8-06 | reader | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
