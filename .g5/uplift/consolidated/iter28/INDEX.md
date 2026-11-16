# iter28 scoped evidence index

These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.

| Case | Canonical owner | Contract | Source decisions → consuming readers |
|---|---|---|---|
| S28-01 | [devtools.scripting.file-sequence](../../../ontology/features/developer-tools.g5.md#devtools.scripting.file-sequence) | [script.rs](../../../Verus_verification/consolidated/iter28/script.rs) `left, right, trim, line_item, lines, dot, extension, decode_json, prepare, fixture, planned, next_line_end` | src/cls/IPM/Main.cls::ProcessRunFromFile → src/cls/IPM/CLI.cls::ParseJsonToCommand |
| S28-02 | [devtools.scripting.file-sequence](../../../ontology/features/developer-tools.g5.md#devtools.scripting.file-sequence) | [script.rs](../../../Verus_verification/consolidated/iter28/script.rs) `params_ok, values, translate, translate_all, prepare, render, validate_all, render_all` | src/cls/IPM/CLI.cls::ParseJsonToCommand → src/cls/IPM/Main.cls::ProcessRunFromFile |
| S28-03 | [devtools.scripting.file-sequence](../../../ontology/features/developer-tools.g5.md#devtools.scripting.file-sequence) | [script.rs](../../../Verus_verification/consolidated/iter28/script.rs) `initial_with_default, initial, set_selected, shell, execute, run, named, setting_value, raw_value, ok, last_listed, first_listed, same_view, shell_same_view, execute_same_view, arbitrary_default, settings_default_frame, raw_reader, output_neighbors` | src/cls/IPM/Main.cls::ProcessRunFromFile; src/cls/IPM/Main.cls::Config; src/cls/IPM/Repo/UniversalSettings.cls::UpdateOne; src/cls/IPM/Repo/UniversalSettings.cls::SetValue → src/cls/IPM/Repo/UniversalSettings.cls::GetValue |
