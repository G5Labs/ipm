# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S14-01 | direct | [src/cls/IPM/Utils/File.cls::IsProtectedDirectory](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/File.cls#L24) |
| S14-01 | reader | [src/cls/IPM/ResourceProcessor/FileCopy.cls::DoCopy](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L160) |
| S14-01 | reader | [src/cls/IPM/ResourceProcessor/FileCopy.cls::OnPhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L216) |
| S14-02 | direct | [src/cls/IPM/Utils/File.cls::CopyDir](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/File.cls#L149) |
| S14-02 | direct | [src/cls/IPM/ResourceProcessor/FileCopy.cls::DoCopy](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L160) |
| S14-02 | reader | [src/cls/IPM/ResourceProcessor/FileCopy.cls::OnBeforePhase](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L38) |
| S14-03 | direct | [src/cls/IPM/Utils/File.cls::NormalizeForComparison](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/File.cls#L81) |
| S14-03 | direct | [src/cls/IPM/Utils/File.cls::DatabaseDirectories](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Utils/File.cls#L60) |
| S14-03 | reader | [src/cls/IPM/ResourceProcessor/FileCopy.cls::NormalizeNames](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/ResourceProcessor/FileCopy.cls#L116) |
