# Iteration 46: `show <module>` summary

Batch 8, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter46/summary.rs), [cases and controls](../../../Verus_verification/consolidated/iter46/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [inspect](../../../ontology/features/inspection.g5.md#inspect) (show bullet).

## Gap

Iteration2 bound `GetDirectDependents`, `GetLines`, `GetInstalledStatus` and `ParseRequirements`. The members that decide which blocks and entries appear (`GetSystemRequirements`, `AddList`, `AddTable`, `AddBlock`, `GetSection`, `GetShortName`, `SplitLines`, `GetRequirements`, `Join`) were unbound.

## Predictions and observations

Owner predictions were recorded before the bodies were read, and source predictions after. Fixtures: s46a (rich manifest), s46b (minimal, required by s46a), s46d (only `Health="false"`) and s46h (no requirements).

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| a: Version, Interoperability="disabled", Health="false" | "IRIS >=2020.1", "Interoperability disabled", no Health line | requirements shown | Health omitted |
| d: only Health="false" (stored) | no System requirements section | shown | omitted |
| a: `<Parameter Mode>` and `<Default Greeting>` | Defaults lists Greeting only | unspecified | Mode omitted |
| a: Keywords | not shown (stored) | details shown | not shown |
| a: multi-line description | one folded line "First line second line" | full text | folded |
| a: Scope="test" resource | hidden | unspecified | hidden |
| a: after-install message | heading plus two trimmed lines | unspecified | same |
| a: dependency s46b >=1.0.0 | "s46b >=1.0.0  installed 1.0.0" | shown | same |
| a: FileCopy Target `${mgrdir}s46data/` | printed unexpanded | unspecified | stored attribute text |
| b, h: minimal modules | only non-empty blocks: b has Code and "Required by directly (1)"; h has Code | unspecified | empty blocks omitted |

Every source prediction matched. Exploratory run 1 is in `attempts/explore1.log`. Its reader key `a.requiredby.row` matches the module's own title line, so that key is gated but not replayed.

## Evidence

- Formal: `summary.rs` 5 verified, 0 errors. Four controls were each rejected: `health-false-shown`, `parameters-listed`, `empty-block-printed` and `scoped-shown`. In harness run 1, `scoped-shown` survived because no obligation covered hidden scoped resources. That run is kept in `attempts/proofs-run1-scoped-control-survived`, and the `scoped_hidden` obligation was added.
- Native: [receipt](native/20261116T175856608516Z/observations.json), 113 fields, all gated, network none, source bytes matched the pin.
- Replay: 52 fields in Verus ([receipt](replay/receipt.json)). Flips of `d.sysreq.heading`, `a.defaults.mode` and `a.sysreq.health` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter46/run.py` (12 s).

## Reach candidates

Newly reached (unbound at the iteration44 map): `ModuleSummary.cls::{GetSystemRequirements, AddList, AddTable, AddBlock, GetSection, GetShortName, SplitLines}`. `GetLines` and `Show` gain this role. `GetRequirements` and `Join` were only exercised on the absent-file path and are not claimed.

## Limits

Presence and selection only. Column padding, web-app detail (owned by `inspect.webapp.detail`), mappings, python and system-settings sections, the unsatisfied/not-installed dependency states and `requirements.txt` rendering are not assessed. No ObjectScript body is verified directly.

## Proposed owner text

- **`inspect`, `show` bullet, added sub-bullets:**
  - "The summary prints only non-empty blocks: description and fields, after-install message and `<Default>` values, the resources by kind, dependencies with their install status, direct dependents, and system requirements. Scoped and unit-test resources, `<Defaults><Parameter>` values and keywords are not shown, and a FileCopy target is shown as written, before expansion."
  - "As built, a `Health="false"` system requirement is not shown, although it stops installation on IRIS for Health; a module whose only requirement it is shows no requirements at all (defect C:D-next)."
- **`inspect` metadata:** "Iteration46 summary: [summary.rs](../../Verus_verification/consolidated/iter46/summary.rs) `health_false_hidden`, `parameter_not_listed`, `empty_blocks_omitted`, `scoped_hidden` ([handoff](../../uplift/consolidated/iter46/HANDOFF.md))." plus a Findings line to the new entry.
- **C register entry, Low · Decision: Open: "`show` omits a `Health="false"` system requirement".**
  - Found by: source review of `General/ModuleSummary.cls::GetSystemRequirements` (Health listed only when true) against `Storage/SystemRequirements.cls::CheckHealth` (Health=0 refuses Health instances), and the iteration46 model `health_false_hidden` with control `health-false-shown`.
  - Reproduced by: E2E reproduced. Receipt `iter46/native/20261116T175856608516Z`: `show s46d` (stored `Health="false"`) prints no System requirements section (case d), and `show s46a` lists IRIS and Interoperability but no Health entry (case a).
  - Passing neighbour: s46h without requirements also shows none, and the other set requirements are listed.
  - Repair, unchecked: add an entry such as "not supported on IRIS for Health" when Health is explicitly false.
