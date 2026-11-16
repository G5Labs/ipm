# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S19-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::ScanDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L313) |
| S19-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::BuildCache](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L117) |
| S19-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::AddCacheItem](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L184) |
| S19-01 | direct | [src/cls/IPM/Repo/Filesystem/Cache.cls::HandleSaveError](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Cache.cls#L84) |
| S19-01 | reader | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleManifest](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L25) |
| S19-01 | reader | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L48) |
| S19-02 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L48) |
| S19-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S19-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S19-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S19-02 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S19-02 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
