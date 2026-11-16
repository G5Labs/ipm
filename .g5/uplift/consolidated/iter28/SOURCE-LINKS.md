# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S28-01 | direct | [src/cls/IPM/Main.cls::ProcessRunFromFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1972) |
| S28-01 | reader | [src/cls/IPM/CLI.cls::ParseJsonToCommand](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/CLI.cls#L708) |
| S28-02 | direct | [src/cls/IPM/CLI.cls::ParseJsonToCommand](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/CLI.cls#L708) |
| S28-02 | reader | [src/cls/IPM/Main.cls::ProcessRunFromFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1972) |
| S28-03 | direct | [src/cls/IPM/Main.cls::ProcessRunFromFile](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L1972) |
| S28-03 | direct | [src/cls/IPM/Main.cls::Config](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2063) |
| S28-03 | direct | [src/cls/IPM/Repo/UniversalSettings.cls::UpdateOne](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/UniversalSettings.cls#L112) |
| S28-03 | direct | [src/cls/IPM/Repo/UniversalSettings.cls::SetValue](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/UniversalSettings.cls#L166) |
| S28-03 | reader | [src/cls/IPM/Repo/UniversalSettings.cls::GetValue](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Repo/UniversalSettings.cls#L150) |
