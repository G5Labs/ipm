# Pinned source decisions and consuming readers

These are scoped source bindings, not direct implementation proofs.

| Case | Role | Exact pinned member |
|---|---|---|
| S26-01 | direct | [src/cls/IPM/Main.cls::Sync](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2375) |
| S26-01 | direct | [src/cls/IPM/General/Sync/Summary.cls::Begin](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L23) |
| S26-01 | direct | [src/cls/IPM/General/Sync/Summary.cls::End](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L29) |
| S26-01 | reader | [src/cls/IPM/General/Sync/Summary.cls::AddSkipped](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L59) |
| S26-01 | reader | [src/cls/IPM/General/Sync/Summary.cls::Report](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L103) |
| S26-02 | direct | [src/cls/IPM/General/Sync/Summary.cls::AddSkipped](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L59) |
| S26-02 | reader | [src/cls/IPM/General/Sync/Summary.cls::Report](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L103) |
| S26-03 | direct | [src/cls/IPM/General/Sync/Summary.cls::Report](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L103) |
| S26-03 | reader | [src/cls/IPM/General/Sync/Summary.cls::Report](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L103) |
| S26-04 | direct | [src/cls/IPM/General/Sync/Pipeline.cls::SyncRunTests](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Pipeline.cls#L584) |
| S26-04 | direct | [src/cls/IPM/Main.cls::Sync](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/Main.cls#L2375) |
| S26-04 | reader | [src/cls/IPM/General/Sync/Summary.cls::AddSkipped](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L59) |
| S26-04 | reader | [src/cls/IPM/General/Sync/Summary.cls::Report](https://github.com/intersystems/ipm/blob/571e04a19aba9f9b4bc0ea23fbe09182038f4e55/src/cls/IPM/General/Sync/Summary.cls#L103) |
