# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S18-01 | direct | [src/cls/IPM/Main.cls::Install](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2554) |
| S18-01 | direct | [src/cls/IPM/Repo/Manager.cls::SearchRepositoriesForModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/Manager.cls#L55) |
| S18-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadQualifiedReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L23) |
| S18-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadModuleReference](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L40) |
| S18-01 | direct | [src/cls/IPM/Utils/Module.cls::LoadNewModule](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/Module.cls#L1152) |
| S18-01 | direct | [src/cls/IPM/General/SemanticVersion.cls::Follows](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/SemanticVersion.cls#L123) |
| S18-01 | reader | [src/cls/IPM/General/ModuleSummary.cls::GetLines](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/ModuleSummary.cls#L51) |
| S18-01 | reader | [src/cls/IPM/Main.cls::GetListModules](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1304) |
| S18-01 | reader | [src/cls/IPM/General/History.cls::SourceToString](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L457) |
| S18-02 | direct | [src/cls/IPM/General/AbstractHistory.cls::SetSource](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/AbstractHistory.cls#L86) |
| S18-02 | direct | [src/cls/IPM/General/HistoryTemp.cls::PersistToTwin](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/HistoryTemp.cls#L30) |
| S18-02 | reader | [src/cls/IPM/General/History.cls::DisplayOneRecord](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L319) |
| S18-02 | reader | [src/cls/IPM/General/History.cls::ShowColumns](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/History.cls#L334) |
| S18-02 | reader | [src/cls/IPM/Main.cls::History](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L4127) |
