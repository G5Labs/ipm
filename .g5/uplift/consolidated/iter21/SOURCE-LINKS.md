# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S21-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::OnConfigure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L58) |
| S21-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::BuildCache](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L117) |
| S21-01 | reader | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleManifest](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L25) |
| S21-01 | reader | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L48) |
| S21-02 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S21-02 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleFromDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L293) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::GetModuleObjectFromPath](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L334) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::GetModuleObjectFromStream](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L352) |
| S21-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S21-02 | reader | [src/cls/IPM/ResourceProcessor/Default/Document.cls::OnLoad](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/Default/Document.cls#L332) |
| S21-02 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S21-03 | direct | [src/cls/IPM/Repo/Filesystem/Cache.cls::OrderedMatchesExecute](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Cache.cls#L48) |
| S21-03 | direct | [src/cls/IPM/Repo/Filesystem/Cache.cls::OrderedMatchesFetch](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Cache.cls#L65) |
| S21-03 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::ListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L12) |
| S21-03 | direct | [src/cls/IPM/Repo/Utils.cls::moduleSqlToList](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Utils.cls#L70) |
| S21-03 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S21-03 | reader | [src/cls/IPM/Main.cls::Search](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2037) |
