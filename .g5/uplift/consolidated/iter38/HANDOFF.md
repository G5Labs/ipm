# Iteration 38: history report rendering

Batch 5 lane, product `571e04a1` unchanged. Not sealed or committed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter38/render.rs), [cases and controls](../../../Verus_verification/consolidated/iter38/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owners: [history.fields](../../../ontology/features/history.g5.md#history.fields), [history.read.details](../../../ontology/features/history.g5.md#history.read.details), [history.phases](../../../ontology/features/history.g5.md#history.phases).

## Gap

The renderers (`VersionToString`, `TimeToString`, `CommittedToString`, `ShowPhases`) carried only extension citations without a formal target. `SourceToString`, `SuccessToString` and `DisplayOneRecord` had no rendering binding. Iteration23 binds only the Success field selection.

## Predictions and observations

Owner predictions were written before any renderer body was read. The only prior exposure was the member list and the extension citation scopes.

| Record | Stored | Rendered (find / details) | Owner predicted |
|---|---|---|---|
| a `load m38a` 1.2.3 | ok, committed, 5 phases ended | `v1.2.3`, Success, Committed, `n/a`, duration; 5 phase lines Success with duration | same |
| b `load` of a folder with an invalid manifest | no package, version 0.0.0-0, failed, no phases | `<unset>`, error text, Uncommitted, `n/a`; no phase lines | same |
| e `load m38e`, After-Activate Invoke returns an error | failed; Activate phase has no end and an error status | 4 phases Success with duration; Activate shows start time only and error text | failed phase listed last |
| z `load m38z`, real version 0.0.0-0 | known package, version 0.0.0-0, success | `<unset>` in find and details | owner: `<unset>` means "not yet known" |
| i `install m38r` from a filesystem repository | source set | repository name shown, not `n/a` | same |
| u `uninstall m38a` | Clean phase | `n/a`, Clean phase line | same |
| `m38a compile` | no new record | — | not recorded |

Every case matched the source predictions in the ledger. Stored and rendered values differ where the renderer decides: placeholder text, `n/a` for non-install sources, decoded status, and duration only with an end time.

## Evidence

- Formal: `render.rs` 5 verified, 0 errors. The generic lemmas cover any stored row: the placeholder hides the version, the source shows only for install, phase lines keep stored order and status, and a known 0.0.0-0 renders the same as an unknown version. Five controls were each rejected: `placeholder-only-when-unknown`, `source-shown-for-all`, `uninstall-shows-source`, `phase-success-from-record` and `phase-duration-always`.
- Native: [receipt](native/20261116T173827930786Z/observations.json), 280 fields gated (stored `st.*` are model inputs). Color was set to none for parsing. Network none, source bytes matched the pin.
- Replay: 141 rendered fields in Verus ([receipt](replay/receipt.json)). Flips of `find.z.vunset`, `find.i.na` and `det.e.p5.success` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter38/run.py` (14 s). The exploratory log `attempts/explore1.log` is not counted.

## Reach candidates

Against the iter37 map, four members are newly reached: `General/History.cls::{VersionToString, TimeToString, CommittedToString, ShowPhases}`, whose earlier rows were citation-only. Five already-linked members gain this rendering role: `ShowColumns`, `SourceToString`, `SuccessToString`, `DisplayQueryResult` and `DisplayOneRecord`. `RecordsExecute/Fetch`, `GlobalRecords*` and `DisplayArray` were not on the exercised path (`history find` uses `GetHistory`) and are not claimed.

## Limits

Not exercised: unfinished records (no end time), an install with an empty source (`<no source set>`), an empty phase status, `-globally` and the class queries. Status decoding and DATEDIFF are provider premises. No ObjectScript body is verified directly.

## Discrepancies

- **Real version 0.0.0-0 shown as `<unset>` (z).** The owner defines `<unset>` as "until the version is known". The renderer compares the stored text only, so a module that really declares 0.0.0-0 looks unknown. Low, cosmetic; reproduced end to end.
- **Open failed phase (e).** The Activate phase has no end time but a stored error status. Details show its error text without a duration, which matches the owner's reading of a phase with no end time as unfinished, though here the action did end with an error. This is consistent with the existing G:D6 (open phase after a returned Invoke error) and adds no new defect.

## Proposed owner text

- **`history.fields`, Version bullet, added sentence:** "As built, a module whose declared version is `0.0.0-0` is also shown as `<unset>`, because listings compare the stored text only (defect C:D-next)."
- **`history.read.details`, added bullet:** "Phase lines keep the stored order and show each phase's start time, its duration only when it has an end time, and `Success` or the decoded error text. A phase left open after a returned error shows its start time and error text (see G:D6)."
- **`history.fields` metadata:** "Model: [render.rs](../../Verus_verification/consolidated/iter38/render.rs) `summary`, `placeholder_hides_version`, `source_only_for_install`, `known_placeholder_indistinguishable`." **`history.read.details` metadata:** "Model: [render.rs](../../Verus_verification/consolidated/iter38/render.rs) `detail_phases`, `phase_rendering_keeps_order_and_status`."
- **C register entry (Low · Decision: Open):** "C:D-next: History shows a module whose real version is 0.0.0-0 as `<unset>`."
  - Found by: source review of `General/History.cls::VersionToString` and iteration38 model `known_placeholder_indistinguishable` with control `placeholder-only-when-unknown`.
  - Reproduced by: E2E reproduced, receipt `iter38/native/20261116T173827930786Z`. `load m38z` (version 0.0.0-0) succeeds with a known package, yet `history find` and `history details` show `<unset>` (record z). Passing neighbours: m38a 1.2.3 shows `v1.2.3`, and the invalid-manifest record b shows `<unset>` as intended.
  - Repair, unchecked: render the placeholder only when the record has no package or the version was never set, for example with an explicit "version known" flag.
