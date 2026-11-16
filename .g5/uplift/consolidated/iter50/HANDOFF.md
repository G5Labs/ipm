# Iteration 50: namespace default modifiers

Batch 9 lane, product `571e04a1` unchanged. Not sealed; the main thread integrates it. [Model](../../../Verus_verification/consolidated/iter50/defmods.rs), [cases and controls](../../../Verus_verification/consolidated/iter50/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [settings.default-modifiers](../../../ontology/concepts/settings.g5.md#settings.default-modifiers). The existing G:D18 (history filters) and G:D19/P:F-REPO-5 (the `^%IPM.DefaultModifiers` global is shared by every namespace) are reused and were not re-tested.

## Predictions and observations

Owner predictions were written before any body was read, and source predictions after. Both are in the ledger.

| Case | Observed | Owner | Source |
|---|---|---|---|
| g0 `-get` with nothing stored | "No default modifiers currently set" | same | same |
| e1 no modifier; e2 `-set -get` | refused, exactly-one error | refused | refused |
| s0 `-set` with no `-D` | success, nothing stored | unspecified | same |
| s1/s2 `-set -DMode=stored`, `-set -DOther.Sub=x` | both listed, nested key as `-DOther.Sub=x` | append | append |
| s3 `-set -DMode=second` | Mode now second, stored gone | unspecified | overwrite |
| l1 load m50 (`<Default Name="Mode" Value="manifest"/>`) | Invoke sees second | unspecified | stored beats manifest |
| l2 load with `-DMode=cmd` | cmd; stored value unchanged | cmd | cmd |
| v1 stored `-DVerbose=1`, load | verbose output | verbose | verbose |
| v2 same with `-quiet` | not verbose | command line wins | same |
| f1 `-set -verbose` | refused "Unknown modifier", nothing stored | "stores flags": accepted | refused (added after explore3) |
| d1 `-delete`, load | manifest | default | manifest |

Explore1 used `<Defaults><Parameter Name="Mode">`. The Invoke argument `${Mode}` was then recorded literally in every case, so neither `-D` nor stored defaults reached it. The fixture switched to `<Default Name Value>`, which iteration36 showed is expanded. That `<Parameter>` behaviour belongs to the module-parameter owner and is not claimed here. All four exploratory logs are kept.

## Evidence

- Formal: `defmods.rs` 6 verified, 0 errors. The generic lemmas are `set_appends_and_overwrites`, `command_line_wins`, `stored_beats_manifest`, `delete_restores_manifest` and `exactly_one_and_known`. Five controls were each rejected: `stored-overrides-command`, `manifest-beats-stored`, `set-replaces-all`, `flags-accepted` and `delete-keeps`.
- Native: [receipt](native/20261116T180355420390Z/observations.json), 66 fields, all gated, network none, image `sha256:7da4480b…`.
- Replay: 50 fields in Verus ([receipt](replay/receipt.json)). Flips of `l2.mode`, `l1.mode` and `f1.ok` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter50/run.py` (14 s).

## Findings

No new defect. The owner says the command "stores flags", but only `-D<name>=<value>` data can be stored, and a modifier such as `-verbose` is refused. The data equivalent `-DVerbose=1` works, and a command-line `-quiet` still overrides it. Stored defaults also override a module's own `<Default>`, which the owner does not state.

## Reach candidates

Newly reached: `Main.cls::ProcessDefaultModifiers` (previously only citation links) and `Main.cls::AccumulateDefaultModifierList`. Strengthened: `Get/Set/DeleteNamespaceDefaultModifiers`, `Main.cls::ShellInternal` and `Storage/Module.cls::ExecutePhases`. `WarnOnDeprecatedModifiersOrParameters` was not exercised, because no deprecated modifier was used, and is not claimed.

## Limits

One namespace only (sharing is owned by P:F-REPO-5). Keys are small ids. Interactions with history filters (G:D18) and transaction flags are not re-run. No ObjectScript body is verified directly.

## Proposed owner text

In `.g5/ontology/concepts/settings.g5.md#settings.default-modifiers`, replace "stores flags that IPM adds" with "stores `-D` data values that IPM adds", and add these bullets:

- "Only `-D<name>=<value>` data can be stored. A modifier such as `-verbose` is refused; store `-DVerbose=1` instead. A modifier given on the command line, such as `-quiet`, still overrides it."
- "`-set` of a name already stored replaces its value. `-set` without any `-D` value succeeds and stores nothing."
- "A stored value overrides a module's own `<Default>` for that name. `-delete` brings the module's default back."
- "`-get` lists every stored value as `-D<name>=<value>`, with nested names joined by dots."

Metadata line: "Model: [defmods.rs](../../Verus_verification/consolidated/iter50/defmods.rs) `default_modifiers`, `effective`, `command_line_wins`, `stored_beats_manifest`, `set_appends_and_overwrites` ([handoff](../../uplift/consolidated/iter50/HANDOFF.md))." No register entry. Optional note for the module-parameter owner: a `<Defaults><Parameter>` name is not expanded as `${name}` in an Invoke argument, while a `<Default>` name is (explore1). This was observed only, not traced.
