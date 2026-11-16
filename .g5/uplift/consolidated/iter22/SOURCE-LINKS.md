# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S22-01 | direct | [src/cls/IPM/Main.cls::Reinstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2673) |
| S22-01 | direct | [src/cls/IPM/General/SemanticVersion.cls::ToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L16) |
| S22-01 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S22-01 | direct | [src/cls/IPM/Repo/Utils.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Utils.cls#L8) |
| S22-01 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S22-01 | reader | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S22-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S22-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S22-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleFromDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L293) |
| S22-02 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S22-02 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S22-02 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
| S22-03 | direct | [src/cls/IPM/Main.cls::Repository](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2095) |
| S22-03 | direct | [src/cls/IPM/Repo/Definition.cls::Configure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Definition.cls#L138) |
| S22-03 | direct | [src/cls/IPM/Repo/Filesystem/Definition.cls::OnConfigure](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Filesystem/Definition.cls#L58) |
| S22-03 | reader | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S22-03 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S22-03 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
| S22-04 | direct | [src/cls/IPM/Main.cls::Shell](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L857) |
| S22-04 | direct | [src/cls/IPM/Main.cls::Reinstall](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2673) |
| S22-04 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S22-04 | direct | [src/cls/IPM/General/AbstractHistory.cls::Init](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L51) |
| S22-04 | direct | [src/cls/IPM/General/AbstractHistory.cls::InstallInit](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L65) |
| S22-04 | direct | [src/cls/IPM/General/AbstractHistory.cls::Finalize](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L127) |
| S22-04 | direct | [src/cls/IPM/General/AbstractHistory.cls::SetSource](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L86) |
| S22-04 | direct | [src/cls/IPM/General/HistoryTemp.cls::Init](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/HistoryTemp.cls#L9) |
| S22-04 | direct | [src/cls/IPM/General/HistoryTemp.cls::PersistToTwin](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/HistoryTemp.cls#L30) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::SourceToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L457) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::DisplayOneRecord](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L319) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::ShowColumns](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L334) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::SuccessToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L474) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::GetHistory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L132) |
| S22-04 | reader | [src/cls/IPM/General/History.cls::DisplayQueryResult](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L492) |
| S22-04 | reader | [src/cls/IPM/Main.cls::History](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4127) |
