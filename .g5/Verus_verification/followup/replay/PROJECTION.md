# Replay binding: model fields, native observations and the L1 clauses that read them

This is the binding used by `evaluate_replay2.py` and `l1sem.py`. Each L1 clause in `crates/ipm-load-history/src/contract.rs` is evaluated on the same fields the model uses. The table gives the native producer of each field and what it can and cannot show.

```
 contract.rs Expect ──(explore.rs expect_json)──▶ cases.json l1_permitted
        ▲                                                 │ l1sem.decode_expect (round trip checked, family A)
        │ same clause code                                ▼
 contract.rs mexp_ok / rec_*_ok ◀── transcribed ── l1sem.mexp_ok / rec_*_ok ◀── project_mod / project_twin ◀── raw.log OBS lines
                                                         │
                                         family B: applied to each case's l2_predicted, the failed
                                         clauses equal l1_vs_l2.violated_clauses in all 59 cases
```

## Module record (`Mod`, `model.rs:52-64`)

| Model field | Native field | Driver producer | Product producer | Observability |
|---|---|---|---|---|
| `present` | `modules.S.manifest_present` | `Replay.cls` `ModSlot`: a `%IPM_Storage.ModuleItem` row with that name | `%IPM.Storage.Module` save | exact |
| `ver` | `modules.S.version` | `VersionString` of that row; `+snapshot` removed for a snapshot dependency | manifest version | exact while present. Not meaningful while absent (INV-ABSENT) |
| `dev` | `modules.S.developer_mode` | `DeveloperMode` | `Utils/Module.cls` developer-mode save | exact while present. Not meaningful while absent (INV-ABSENT) |
| `res` | `modules.S.resources_loaded` | `%Dictionary.CompiledClass` exists for `<Pkg>.Main` | Compile phase | projection: each fixture has one class, so "the class exists" stands for "resources loaded". A module with several resources is not represented (R07) |
| `doc` | `modules.S.zpm_document_version` | `<Version>` inside the stored `%IPM.StudioDocument.ModuleStream`; `<none>` → 0 | manifest import | exact for the version element |
| `wr` | not observable | none | every `%Save` of the module | proxy: `modules.S.x.last_updated`. `%IPM.Storage.Module:%OnBeforeSave` stamps `LastUpdated` with `$horolog` at one-second resolution (`src/cls/IPM/Storage/Module.cls:1879-1882`), and the driver waits 1.1 s after setup (`Replay.cls`, `hang 1.1`). A changed value implies at least one write. An equal value implies no write that survived. Only defined while the record is present before and after |

INV-ABSENT. In the model, `ver`, `dev`, `res` and `wr` change only through `m_manifest`, `m_dev`, `m_res` and `m_touch` (`model.rs:444-450`). `m_manifest` sets `present`, and the other three are applied only after it (`model.rs:502-609`). A rollback restores the whole record. So a record absent before and after the command has its initial `ver`, `dev`, `res` and `wr`. `m_doc` changes only `doc`, which is observed directly. `mod_eq` therefore compares `present`, `res` and `doc` always, and `ver`, `dev` and the `wr` proxy only while the record is present on both sides.

Slots. The model always has three module slots. A case realizes `top` and as many dependency slots as it has dependencies. In the model, an unrealized slot is never written, so its clauses hold trivially. The evaluator evaluates realized slots only. A realized slot without an expectation is a binding error and makes the clause unknown.

Alternatives. `alts_ok` (`contract.rs:311-313`) considers at most three alternatives. No case has more than two. The evaluator reports each alternative per slot.

## History row (`Twin`, `model.rs:91-110`)

| Model field | Native field | Driver normalization | Observability |
|---|---|---|---|
| `ex` | `history.S.row` | a `%IPM_General.History` row for that package after the case's baseline ID | exact |
| `ok` | `history.S.success` | `OK` / `ERR` / `ZERO` (in progress) | exact for the three classes |
| `com` | `history.S.committed` | 1/0 | exact |
| `tend`, `fin`, `pkg` | `time_end_set`, `finalized`, `package_set` | 1/0 | exact |
| `ph` | `phases_started` | count of `Phases` entries | exact count; phase names and order are kept only as `x.last_phase_name` (R07) |
| `phst` | `last_phase_status` | `OPEN` / `OK` / `ERR`, or `<none>` with no phases | status of the last phase only (R07) |

## Clause identities

| Clause (explore.rs `violated_clauses`) | contract.rs | l1sem.py |
|---|---|---|
| `L1-STATUS` | `st_ok` 346-348 | `st_ok` |
| `L1-NAMESPACE` | `alts_ok`/`alt_ok`/`mexp_ok`/`installed_ok`/`dev_ok` 289-313 | same names |
| `H-RECORD-EXISTS[S]` | `rec_exists_ok` 324-326 | `rec_exists_ok` |
| `H-RECORD-FIELDS[S]` | `rec_fields_ok` 329-333 | `rec_fields_ok` (also reports the failing sub-field) |
| `H-COMMITTED[S]` | `rec_com_ok` 336-338 with `kept` 318-321 | `rec_com_ok`, `kept` (top: any slot; dependency: its own slot) |

The refusal kind read from the error text is reported as `aux_refusal_kind`. It is not an L1 clause: `expect_ok` (`contract.rs:358-360`) does not read `Expect.verdict`.

## Three-valued results

A clause is PASS, FAIL or UNKNOWN. UNKNOWN means an observation the clause needs is missing or malformed. A case is VIOLATES if any clause fails, INDETERMINATE if none fails and one is unknown, and PERMITTED only when every clause passes. Missing evidence never counts as agreement.
