# Settings

IPM has instance settings managed with `config`, and stored flags added to every command with `default-modifiers`.

## Definitions

- **Setting**: a named IPM option with a default value.
- **Default modifier**: a stored command-line flag that IPM adds to every command in a namespace.

## Statements

<a id="settings"></a>
[I · settings]

`config list`, `get`, `set` and `delete` manage IPM settings. `delete` restores a setting's default, and upgrading IPM keeps existing values.

| Key | Default | Meaning |
|---|---|---|
| `analytics` | `1` | Send usage data ([analytics](#settings.analytics)). |
| `DefaultLogEntryLimit` | `20` | How many records `history find` shows by default. |
| `HistoryRetain` | `0` | Days to keep history. `0` keeps it forever ([retention](../features/history.g5.md#history.retention)). |
| `ModuleRoot` | `<data directory>/ipm/` | Where archives unpack. It must be writable. |
| `PipCaller`, `UseStandalonePip` | | Which pip IPM runs. |
| `PublishTimeout` | | How long publishing may take. |
| `SemVerPostRelease` | `0` | How `x.y.z-w` is ordered ([post-release](../concepts/versions.g5.md#version.post-release)). |
| `TerminalPrompt` | | What the shell prompt shows. |
| `TestReportFormat` | | Default format of test summaries. |

<a id="settings.writes"></a>
[D · settings.writes]

A setting write with overwrite disabled keeps a nonempty current value. An absent or empty current value still accepts the write, and later writes preserve the first stored default even when the current value is cleared.

- This describes the low-level writer. It does not remove the known `config delete` defect: most keys restore that first value rather than a factory default. TestReportFormat and ModuleRoot have explicit reset rules.

<a id="settings.report-format"></a>
[D · settings.report-format]

The TestReportFormat setter accepts empty, JSON, YAML and TOON without regard to case and stores the supplied spelling. An invalid value returns an error before writing, preserving both the current value and its first default.

- Empty selects legacy output. This setter validates before applying the overwrite rule; an invalid value is still an error when overwrite is disabled.
- ModuleRoot's path normalization, directory creation and write-access checks are different rules. No validation guarantee for another key follows from this format setter.

<a id="settings.default-modifiers"></a>
[I · settings.default-modifiers]

`default-modifiers` stores flags that IPM adds to every command in the current namespace.

- `-set` adds flags to the stored ones and removes none.
- `-get` shows them, and `-delete` removes them all.
- Exactly one of these three is required.
- A flag given on the command line wins over a stored one.

<a id="settings.install-flags"></a>
[I · settings.install-flags]

Besides the [transaction flags](transactions-and-dev-mode.g5.md#transaction.opt-out), `-DNoMapping=1` turns off the automatic mappings for a module's resources. These flags apply to `install`, `load`, `reinstall` and `update`, and are expected to change in IPM 1.0.0.

<a id="settings.analytics"></a>
[I · settings.analytics]

`config set analytics 0` stops IPM from sending usage data.

<a id="settings.print"></a>
[D · settings.print]

Printing a configurable setting shows its key and current effective value. Printing an unknown key shows a not-found diagnostic and returns before requesting any value.

Key membership is exact, as in the configurable list. A diagnostic is not an effective setting value.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). `DefaultLogEntryLimit`, `HistoryRetain` and default modifiers reach the formal scope through [history](../features/history.g5.md) and [transaction flags](transactions-and-dev-mode.g5.md#transaction.flag-sources).

### settings

- human-provided
- user-facing-behavior
- test-coverage
- Children: [writes](#settings.writes), [report format](#settings.report-format), [default modifiers](#settings.default-modifiers), [install flags](#settings.install-flags), [analytics](#settings.analytics).

Sources: `wiki/02` L93-141; `Main.cls:210-220`; CHANGELOG 0.9.0 (`#224`); `preload/cls/IPM/Installer.cls:110-124`. Inventory: CAT-26; A2 1.2 row 15 (`Main:Config`, `%IPM.Repo.UniversalSettings`), OP-HIST-22. Tests: `Unit/CLI.cls` `TestReportFormatConfiguration`, `ModuleRoot.cls`. Observed: settings are stored in `^IPM.settings` in `%SYS`, so they apply to the whole instance. Confirmed product defect B2 D5: `delete` restores the first value ever set, not the built-in default, so `config delete HistoryRetain` can leave retention on (case F0; `20261113T184628Z-repro-config-delete-retain`; also seen after `20261113T182514Z-suite-test-CLI`).

Findings: [P:F-REPO-4 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-repo-4). The linked entry qualifies the caller, reader and providers.

### settings.writes

- agent
- Expands: [Settings](#settings).

Sources: `Repo/UniversalSettings.cls::{SetValue,GetValue,GetDefaultValue,ResetToDefault}`. Verus `public_repairs.rs::{set_value,setting_neighbors}`. Native nonempty refusal, empty acceptance and first-default persistence are distinct observations. Cross-process concurrency and failed storage writes remain open.

### settings.report-format

- agent
- Expands: [Settings](#settings).

Sources: `Repo/UniversalSettings.cls::{SetTestReportFormat,GetTestReportFormat,UpdateOne}`; `%IPM.Test.Abstract.VALIDFORMATS`. Verus `public_repairs.rs::{lower,format_valid,set_format,setting_neighbors}`. Native `JsOn`, invalid refusal and spelling retention are checked. This is the persistent setter, not the later renderer's supported-format contract.

### settings.default-modifiers

- human-provided
- user-facing-behavior
- Expands: [Settings](#settings).

Sources: `Main.cls:617-637`; `wiki/02` L143-166; maintainer comment `Main.cls:1067-1070`. Inventory: CAT-27; A2 1.2 row 28 (`Main:ProcessDefaultModifiers`). Confirmed product defect D19: the flags are stored in `^%IPM.DefaultModifiers`, which maps to IRISSYS from every namespace, so defaults set in USER are visible in USER-VERIFY (stage C case X1, `20261113T204152Z-stagec`). Confirmed as D18, decision pending under Q-18: they are also read by `history` as filters ([default-modifier filters](../features/history.g5.md#history.read.filter.default-modifiers), stage C case H2, `20261113T204152Z-stagec`).

Findings: [P:F-REPO-5 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-repo-5). The linked entry qualifies the caller, reader and providers.

### settings.install-flags

- human-provided
- user-facing-behavior
- Expands: [Settings](#settings).

Sources: `wiki/00` L139-180; `wiki/02` L447, L595, L907, L1367. Inventory: CAT-28. The wiki also lists `-DNoLock=1` without saying what it does. Observed: it turns compile locking on (`Lifecycle/Base.cls:1194-1198`). `UpdateSnapshots`, `IgnoreInstalled` and `PermitDowngrade` appear only in code, the `default-modifiers` example and `update`'s refusal of `PermitDowngrade`, so they are not stated here as intent.

### settings.analytics

- human-provided
- user-facing-behavior
- Expands: [Settings](#settings).

Sources: `Main.cls:211-215`; `wiki/02` L111, L124-138. Inventory: CAT-40; OP-LOAD-18 (`CollectAnalytics`). Observed: installs from a remote registry report their outcome to the registry and to Google Analytics when enabled (`Repo/Remote/PackageService.cls:201-272`). The data sent is not documented.

### settings.print

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [settings](#settings).

Sources and readers: `Repo/UniversalSettings.cls::PrintOne` → `GetValue`; shell `Main.cls::Config`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `config_get_called`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

Findings: [P:F-REPO-8 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-repo-8). The linked entry qualifies the caller, reader and providers.
