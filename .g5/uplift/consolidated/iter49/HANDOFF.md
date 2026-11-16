# Iteration 49: the `import` command

Batch 9 lane, product `571e04a1` unchanged. Not sealed or committed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter49/importcmd.rs), [cases and controls](../../../Verus_verification/consolidated/iter49/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json).

## Gap

`Main.cls::Import` (93 lines) had no binding and no product test. The only owner text is one bullet under [devtools.authoring](../../../ontology/features/developer-tools.g5.md#devtools.authoring): "`import` loads classes from files and re-exports them to source control if needed."

## Predictions and observations

Help-text predictions were written before the body was read; source predictions followed (both in the ledger).

| Case | Observed | Predicted |
|---|---|---|
| i1 `import x/export.xml` (two classes) | both compiled (1, 2) | same |
| i2 `import d/` (.cls and .xml at root and in `sub/`) | all four compiled | same; the open question was ImportDir's default wildcard, which includes `.cls` |
| i3 `import n/ -norecurse` | root items compiled, `sub/` items absent | same |
| i4 `import c/ -nocompile` | defined, not compiled | same |
| i5 `import e/` (one class with an undefined property type) | fails "Error in import: ... #5373"; the good class compiled; the bad class defined but uncompiled | same |
| i6 missing path | fails "Error in import: ... #5012" | same |
| i7 with recording source-control class | UserAction(1,6) once over both items; OnAfterSave for the item reported in source control; AddToSourceControl for the other; "2 item(s) loaded." | same |

No defect. The import is not atomic (i5 keeps the good class), and no statement promises otherwise; this is recorded as a fact.

## Evidence

- Formal: `importcmd.rs` 6 verified, 0 errors. Controls rejected: `norecurse-ignored`, `nocompile-ignored`, `bad-item-rolls-back-good`, `sc-all-added`, `compile-error-ignored`.
- Native: [receipt](native/20261116T180209444815Z/observations.json), 67 fields, all gated; network none; source bytes matched the pin.
- Replay: 65 fields in Verus ([receipt](replay/receipt.json)); flips of `i3.defined.I49N.Sub`, `i5.ok` and `i7.add.new` rejected.
- Attempts kept: explore1 (reader crashed on an empty `$lb()` after i6) and run 1, whose replay did not compile because the model function `add` clashed with `vstd::prelude::add` (renamed `added`; run 2 reused the unchanged native receipt with `--no-native`).
- Command: `python3 .g5/Verus_verification/consolidated/iter49/run.py`.

## Reach

Newly reached against the iteration48 map: `Main.cls::Import` (1 member). Its callees are SDK providers (`$system.OBJ.ImportDir/Load`, `%Studio.SourceControl.Interface`), not IPM members.

## Limits

Perforce timestamp handling (`^Sources(0,"FileTimeStamp")`), failing source-control calls, `-q`/`-v` output and Unicode or unusual file names are not assessed. No ObjectScript body is verified directly.

## Proposed owner text

New statement in `.g5/ontology/features/developer-tools.g5.md`, after `devtools.authoring`:

```
<a id="devtools.authoring.import"></a>
[D · devtools.authoring.import]

`import <file|dir>` loads class files into the current namespace and compiles them. A directory is searched recursively unless `-norecurse` is given, and `-nocompile` loads without compiling.

- A class that fails to compile fails the command with "Error in import", while the other classes in the same import stay loaded and compiled. A missing path fails before anything loads.
- When the namespace has a source-control class, IPM first tells it which items will load, then loads them, then saves each loaded item through it: an item it already tracks is re-exported, and a new one is added.
```

Metadata: `agent`, `user-facing-behavior`, Expands [Authoring commands](#devtools.authoring). Sources: `Main.cls::Import`; command help. Model: [importcmd.rs](../../Verus_verification/consolidated/iter49/importcmd.rs) `norecurse_skips_subdirectories`, `nocompile_defines_only`, `one_bad_item_fails_command_but_keeps_good`, `source_control_routing`, `missing_path_fails` ([handoff](../../uplift/consolidated/iter49/HANDOFF.md)). No register entry. Bindings use the anchor `devtools.authoring.import`; drop the "(proposed)" suffix when the statement is added.
