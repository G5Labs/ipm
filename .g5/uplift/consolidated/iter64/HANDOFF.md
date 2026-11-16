# Iteration 64: what a failed load leaves behind

Batch 14, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter64/failkeep.rs), [cases and controls](../../../Verus_verification/consolidated/iter64/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owners: [load.outcome](../../../ontology/features/load.g5.md#load.outcome), [load.outcome.dev](../../../ontology/features/load.g5.md#load.outcome.dev) and [transaction.rollback-scope](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).

Each fixture module declares a class package, a `.LOC` file, two FileCopy entries, a WebApplication (`/ipmcont64<case>`), declared global and package `<Mapping>`s to database DB64, and a CPF package-mapping merge. Case ec adds a class that fails to compile, case la an After-Activate Invoke that throws, and case dv is ec loaded with `-dev`.

| Kind | ok (success) | ec (normal, Compile fails) | la (normal, after Activate fails) | dv (`-dev`, Compile fails) |
|---|---|---|---|---|
| Module record | kept | gone | gone | kept |
| Valid class | compiled | gone | gone | compiled |
| FileCopy file and folder | copied | not copied | **left** | not copied |
| Web application | created | not created | removed by rollback | not created |
| Declared mappings (global, package) | added | **rolled back** | rolled back | kept |
| CPF merge | applied | **left** | **left** | left |
| `.LOC` messages and document | loaded | gone | gone | kept |
| `.LOC` export file | written | not written | **left** | not written |
| `^Sources` | not set (non-dev) | not set | not set | kept |
| History | success, committed | failure, not committed | failure, not committed | failure, committed |

Every outcome agrees with the owners. Normal mode rolls back module data, and the web application rollback matches [module.webapp.failure](../../../ontology/concepts/modules.g5.md#module.webapp.failure). The CPF merge, FileCopy targets and the localization export file survive as `transaction.rollback-scope` describes, with compensation still the open question C:Q12. Developer mode keeps database effects after its early commit. The source prediction that declared mappings would survive a normal rollback was refuted: they are undone with the transaction. **No new discrepancy.**

Evidence: `failkeep.rs` 4 verified. Five controls were each rejected: `mapping-external`, `cpf-rolled-back`, `file-copied-early`, `dev-rolls-back` and `webapp-external`. The [native receipt](native/20261116T184141777914Z/observations.json) has 81 gated fields. 72 replay in Verus, and flips of `ec.cpf`, `la.filea` and `dv.mapg` were each rejected. Explore1 and explore2 were refused because FileCopy and CPF resource names must be unique across installed modules; the fixture was fixed and both logs are kept. Command: `python3 .g5/Verus_verification/consolidated/iter64/run.py` (22 s).

Reach: no new member is expected, since all deciding members were already linked. Roles are added to `LoadNewModule`, `%Reload`, `AddGlobalMapping`/`AddPackageMapping`, the CPF and LocalizationExport `OnBeforePhase`, `FileCopy.OnAfterPhase`/`DoCopy` and `WebApplication.OnPhase`.

Limits: two failure points, one database and no NoTransaction or NoJournal runs (G:D4, G:D11 and G:D12 own those). The rollback of Config and Security changes, `iris merge` and the compiler are native-qualified premises.

## Proposed owner text

- `transaction.rollback-scope`, added bullet: "A normal-mode rollback also undoes declared `<Mapping>` entries and a web application created during the load, while a CPF merge, FileCopy targets and the localization export file written in Activate remain (iteration64)."
- `transaction.rollback-scope` metadata: "Iteration64: [failkeep.rs](../../Verus_verification/consolidated/iter64/failkeep.rs) `normal_compile_failure_keeps_only_cpf`, `late_failure_keeps_external_effects`, `developer_failure_keeps_database_effects` ([handoff](../../uplift/consolidated/iter64/HANDOFF.md))."
- No register entry.
