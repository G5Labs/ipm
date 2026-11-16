# Iteration 41: `config` settings and their readers

Batch 6 lane, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter41/settings.rs), [cases and controls](../../../Verus_verification/consolidated/iter41/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [settings](../../../ontology/concepts/settings.g5.md#settings). Not committed or sealed.

## Gap

`UniversalSettings` list and analytics readers (`GetAll`, `PrintList`, `GetAnalyticsAvailable`) were unbound. The set and delete writers had citations, but no model derived the delete and status outcomes behind P:F-REPO-4 and P:F-REPO-8. ModuleRoot (the iteration29 scope) and archive paths were left alone.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| c0 fresh install | HistoryRetain, PublishTimeout, TerminalPrompt and TestReportFormat have no value and no default node; analytics 1, DefaultLogEntryLimit 20 and SemVerPostRelease 0 are seeded | table defaults | seeded subset |
| c1/c1g `config set HistoryRetain 5`, `get` | stored 5, default node 5, `HistoryRetain:5` printed | 5 | 5 |
| c2 `config delete HistoryRetain` | "reset to default" printed, value stays 5 | 0 | 5 (first write is the default) |
| c3/c3d PublishTimeout 99, then delete | stays 99 | default | 99 |
| c4 `config set TestReportFormat xml` | ERROR printed, value unchanged, shell status OK | error | OK status |
| c4n `json` | stored json | json | json |
| c5/c6 `config set/delete Bogus` | "not found", no node, shell status OK | refusal | OK status |
| c7 `analytics 0` | stored 0, consumer disabled | disabled | disabled |
| c8 `analytics yes` | stored "yes", consumer test counts it as disabled | unspecified | disabled |
| c9 `config list` | all 12 configurable keys; `default_registry` not listed | every key | same |

Explore1 had two reader bugs: c1g used an undefined array, and the error marker text was wrong. Explore2 crashed because `%IPM` classes are not visible from `%SYS`. Both are kept under `attempts/`. Explore3 fixed the expected values.

## Evidence

- Formal: `settings.rs` 6 verified, 0 errors. Five controls were each rejected: `reset-uses-factory-empty`, `unknown-key-errors`, `format-not-validated`, `first-write-not-default` and `analytics-text-enabled`.
- Native: [receipt](native/20261116T174828407055Z/observations.json), 132 fields, all gated.
- Replay: 125 fields ([receipt](replay/receipt.json)). Flips of `c2.HistoryRetain.value`, `c5.ok` and `c8.consumerdisabled` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter41/run.py` (13 s).

## Findings

No new defect. P:F-REPO-4 (delete restores the first stored value) and P:F-REPO-8 (unknown key or bad value returns OK) are re-qualified on this pin and Linux build. Values live in `%SYS` `^IPM.settings`. The analytics consumer treats any non-numeric text as disabled.

## Reach candidates

Newly reached against the iteration39 map: `UniversalSettings.GetAll`, `PrintList` and `GetAnalyticsAvailable`. Strengthened with this role: `UpdateOne`, `SetValue`, `ResetToDefault`, `GetDefaultValue`, `GetValue`, `PrintOne` and `Main.Config`. `GetHistoryRetain` is not claimed, because the readers used the raw node and `config get`.

## Limits

ModuleRoot and the actual remote analytics send are excluded (no network). Value formatting in the list output is not modelled. Seeded slots are a native premise of this image. No ObjectScript body is verified directly.

## Proposed owner text

- `settings` table, HistoryRetain row: keep the default `0` and add a bullet under `settings`: "As built, a fresh install does not store HistoryRetain, PublishTimeout, TerminalPrompt or TestReportFormat, so the first `config set` value becomes the one `config delete` restores (P:F-REPO-4)."
- New bullet under `settings`: "As built, `config` prints a message for an unknown key or a refused value but still returns success to the caller (P:F-REPO-8)."
- `settings.analytics`, added sentence: "The value is stored as given; any value that is not a positive number, such as `yes`, also stops usage data."
- Metadata for `settings` and `settings.analytics`: "Model: [settings.rs](../../Verus_verification/consolidated/iter41/settings.rs) `set`, `delete`, `delete_restores_first_write`, `unknown_key_reports_ok`, `invalid_format_refused_with_ok`, `analytics_disabled`."
- No new register entry. Add the iteration41 receipt as a fresh reproduction to P:F-REPO-4 and P:F-REPO-8.
