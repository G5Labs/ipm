# Iteration 65: developer-mode versus normal-mode refresh across resource kinds

Batch 14 lane. Product `571e04a1` unchanged. Not sealed or committed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter65/moderefresh.rs), [cases and controls](../../../Verus_verification/consolidated/iter65/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json).

Fixture: two directory modules with the same eight resource kinds. md65 is loaded with `-dev` and mn65 without. Every source is rewritten with a new value before each step: CLS, INC, MAC, GBL data, FileCopy file, WebApplication Description, LOC message and DFI title.

| Kind | load again (dev / normal) | `m reload` (dev / normal) | `m compile` (dev / normal) | `sync m` (dev / normal) |
|---|---|---|---|---|
| CLS | yes / yes | no / no | yes / no | yes / refused |
| INC | yes / yes | yes / no | yes / no | yes / refused |
| MAC | yes / yes | no / no | yes / no | yes / refused |
| GBL | yes / yes | yes / no | yes / no | no (no sync support) / refused |
| FileCopy | yes / yes | no / no | no / no | no / refused |
| WebApplication | yes / yes | no / no | no / no | no / refused |
| LOC | yes / yes | no / no | yes / no | yes / refused |
| DFI | yes / yes | yes / no | yes / no | yes / refused |

"yes" means the namespace shows the edited value after the step. Plain `sync` names nothing it skipped. `sync -verbose` prints "3 resource(s) skipped (no sync support)" and suggests `reload`.

**Predictions.** The owner predictions held, with one miss. Developer-mode reload plus compile was predicted to refresh every kind, but FileCopy and WebApplication apply only in Activate, which the `compile` chain does not include (lifecycle.chain). Normal mode refreshing only on `load` is the owned load.reload.root rule (P:F-SYNC-3), and sync refusing normal mode is owned by devtools.sync. Explore1's two fixture modules both declared `FileCopy Name="static/"`, so the second load was refused as a duplicate resource. The fixture was fixed before explore2. Both logs are kept.

**New discrepancy: none.** Sync's silence about unsupported resources without `-verbose` looks deliberate (`PrintUnsupportedNote`), and the whole-namespace summary records it as a warning. It is recorded as a fact.

**Evidence.** `moderefresh.rs` 5 verified, 0 errors. Five controls were each rejected: `nondev-reload-uses-root`, `compile-runs-activate`, `sync-supports-globals`, `sync-in-normal-mode` and `reload-compiles`. The [native receipt](native/20261116T184118648509Z/observations.json) has 126 gated fields. 114 replay in Verus, and flips of `c.D.gbl`, `b2.D.filecopy` and `b1.N.inc` were each rejected. Command: `python3 .g5/Verus_verification/consolidated/iter65/run.py` (14 s).

**Reach candidates** against the iter63 map: newly reached `General/Sync/Output.cls::PrintUnsupportedNote`, `::PrintTail`, `ResourceProcessor/Abstract.cls::SupportsSync` and `ResourceProcessor/AbstractCompilable.cls::SupportsSync`. Strengthened: `%Reload`, `ExecutePhases`, `Pipeline.Run` and the Global, DeepSeeItem, LocalizedMessages, FileCopy, WebApplication and Document phase methods.

**Limits.** UnitTest resources are excluded (C:D48), as are preload (C:D47), the flat layout (C:D36) and `-nodev` transitions. Each fixture module declares one resource per kind. No ObjectScript body is verified directly.

## Proposed owner text

- `features/developer-tools.g5.md#devtools.sync`, added bullet: "Sync applies class, include, routine, `.LOC` and `.DFI` files. Global data, FileCopy and WebApplication resources have no sync support. Sync leaves their edits unapplied and names them only with `-verbose`, which suggests `reload`." Metadata: "Iteration65: [moderefresh.rs](../../Verus_verification/consolidated/iter65/moderefresh.rs) `sync_skips_unsupported` ([handoff](../../uplift/consolidated/iter65/HANDOFF.md))."
- `concepts/lifecycle.g5.md#lifecycle.chain`, added bullet: "Each kind becomes visible at its own phase. Include text, global data and `.DFI` items load at Reload, classes, routines and `.LOC` messages at Compile, and FileCopy and WebApplication at Activate. A `compile` module action therefore leaves FileCopy targets and web applications stale." Metadata: "Model: [moderefresh.rs](../../Verus_verification/consolidated/iter65/moderefresh.rs) `compile_chain_skips_activate`."
- No register entry.
