# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S16-01 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S16-01 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S16-01 | direct | [src/cls/IPM/Storage/QualifiedModuleInfo.cls::%OnNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Storage/QualifiedModuleInfo.cls#L11) |
| S16-01 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::GetPackageService](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L52) |
| S16-01 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::%OnNew](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L6) |
| S16-01 | direct | [src/cls/IPM/Repo/Filesystem/PackageService.cls::GetModuleDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/PackageService.cls#L48) |
| S16-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S16-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S16-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S16-01 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S16-01 | reader | [src/cls/IPM/General/ModuleSummary.cls::AddField](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L340) |
| S16-01 | reader | [src/cls/IPM/General/ModuleSummary.cls::Show](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L9) |
| S16-01 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
| S16-02 | direct | [src/cls/IPM/General/AbstractHistory.cls::SetSource](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L86) |
| S16-02 | direct | [src/cls/IPM/General/HistoryTemp.cls::PersistToTwin](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/HistoryTemp.cls#L30) |
| S16-02 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::OnConfigure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L58) |
| S16-02 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::RootSet](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L42) |
| S16-02 | direct | [src/cls/IPM/Repo/Definition.cls::Configure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Definition.cls#L138) |
| S16-02 | reader | [src/cls/IPM/General/History.cls::SourceToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L457) |
| S16-02 | reader | [src/cls/IPM/General/History.cls::DisplayOneRecord](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L319) |
| S16-02 | reader | [src/cls/IPM/General/History.cls::ShowColumns](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L334) |
| S16-02 | reader | [src/cls/IPM/Main.cls::History](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4127) |
| S16-03 | direct | [src/cls/IPM/Main.cls::Load](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2415) |
| S16-03 | direct | [src/cls/IPM/Main.cls::Uninstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2701) |
| S16-03 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S16-03 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S16-03 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
| S16-03 | reader | [src/cls/IPM/General/History.cls::SourceToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L457) |
