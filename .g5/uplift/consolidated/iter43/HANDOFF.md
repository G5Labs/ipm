# Iteration 43: source-control extension hooks

Batch 7, product `571e04a1` unchanged. Executed by a delegated lane; not sealed. [Model](../../../Verus_verification/consolidated/iter43/schooks.rs), [cases and controls](../../../Verus_verification/consolidated/iter43/cases.json), [bindings](bindings.json), [ledger](working-ledger.json).

## Gap

`ExtensionBase/*` (61 members) had no links and no ontology owner. IPM calls `%IPM.ExtensionBase.SourceControl.Interface` dispatchers before and after every phase, on resource and manifest changes, and on module-root changes. Intent was recovered from the class and method comments and the `init` help.

## Predictions and observations

| Case | Configuration and command | Observed | Intent predicted | Source predicted |
|---|---|---|---|---|
| n1/n2 | none; load -dev, uninstall | no calls | none | none |
| d1/d2 | test class set directly as namespace source control | no calls | calls | none (`%GetClass` requires the Composite) |
| s1/s2 | Composite with a plain primary, test class secondary | no calls | calls via Composite | none (primary only) |
| p1 | Composite with test class primary (what `init` sets up); load -dev | Before/After for Initialize, Reload, Validate, Compile, Activate and nested Configure; 4 OnModifyResources | per phase | same |
| p2 | `m43 compile -only` | Before/After Compile | same | same |
| p3 | save `m43.ZPM` | 1 OnModifyResources | 1 | 1 |
| p4 | load from another directory | 16 calls, no OnChangeModuleRoot; stored root changed | root hook | round 1: root hook (refuted); round 2: none |
| p4b | `m43 reload -only -DRootDirectory=<original>` | Before/After Reload, 2 OnModifyResources, no root hook; root changed back | root hook | none |
| p5 | Before Compile returns an error | command fails, no After Compile | phase fails | same |
| p6 | uninstall | Before Clean, Before/After Unconfigure, After Clean | Clean hooks | same |

Explore1-2 failed on session quoting and loaded no reader. Explore3 hit a fixture version downgrade. Explore4 refuted the root-change prediction, and p4b was added before explore5. All runs are kept under `attempts/`.

## Evidence

- Formal: `schooks.rs` 8 verified, 0 errors. Five controls were each rejected: `direct-class-active`, `secondary-consulted`, `before-error-ignored`, `nested-after-parent` and `root-hook-fires`.
- Native: [receipt](native/20261116T175445272137Z/observations.json), 278 fields, all gated. Replay: 260 fields in Verus. Flips of `p5.ok`, `d1.total` and `p6.B.Unconfigure` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter43/run.py` (15 s).

## Reach candidates

13 newly reached against the iteration41 map:
- `Interface`: `%GetClass`, `%OnBeforePhase`, `%OnAfterPhase`, `%OnModifyResources`, `%OnChangeModuleRoot`.
- `CompositeMethodOverrides`: `GetPrimaryExtensionClass`, `SetExtensionClasses`, `SetPrimaryExtensionClass`, `SetSubExtensionClasses`.
- `ResourceReference.OnChange`.

Strengthened: `ExecutePhases`, `%Reload`, `UpdatePersistentFromStream` and `ModuleAction`. The base default hook bodies (overridden by the fixture) and `Main.Init` are not claimed.

## Limits

Interactive `init` prompts, Studio/Atelier menus, Composite delegation of ordinary Studio callbacks, `%OnMakeModuleEditable` and the build hooks were not exercised. The modify-call counts are native premises. Why the stored root is already updated is source-attributed (`Storage/Module.cls:477` sets Root from RootDirectory before the phases; `Utils/Module.cls:1302` sets it during load), but the p4b path was not fully traced. No ObjectScript body is verified directly.

## Proposed owner text

New statements for `.g5/ontology/concepts/lifecycle.g5.md`, after `lifecycle.hooks`:

```
<a id="lifecycle.source-control"></a>
[D · lifecycle.source-control]

A namespace's source-control extension can follow module lifecycles. IPM calls it before and after every phase it runs, including nested phases such as Configure inside Activate, when a module's resources or manifest change, and when a module's root changes.

- These calls reach only the primary extension of IPM's composite source-control class, which `init` configures by wrapping the namespace's existing extension. An extension set directly as the namespace source control, or listed only as a secondary extension, receives no calls.
- If the before-phase call returns an error, that phase fails and the command fails; the after-phase call for it is not made.
- As built, the root-change call is not made when a module is loaded from another directory or reloaded with an explicit root, although the stored root changes (defect C:D-next).
```

Metadata: agent; user-facing-behavior; Expands [Resource hooks](#lifecycle.hooks). Sources: `ExtensionBase/SourceControl/Interface.cls` comments and `%GetClass`, `ExtensionBase/CompositeMethodOverrides.cls::SetExtensionClasses`, `Main.cls` init (lines 1876-1890), `Storage/Module.cls::ExecutePhases`. Model: [schooks.rs](../../Verus_verification/consolidated/iter43/schooks.rs) `active`, `observe`, `before_error_stops_phase`, `nested_hooks_inside_parent`, `root_hook_not_reached` ([handoff](../../uplift/consolidated/iter43/HANDOFF.md)). Findings line to the new entry.

Proposed C register entry (Low · Decision: Open):

```
### C:D-next: A source-control extension is never told that a module's root changed

Found by: iteration43 native exploration (explore4) refuting the source prediction, then source review of Lifecycle/Base.cls::%Reload against Storage/Module.cls::ExecutePhases (Root assigned before the comparison), iteration43 model root_hook_not_reached with control root-hook-fires.

Reproduced by: E2E reproduced. Receipt iter43/native/20261116T175445272137Z. With the IPM-aware test extension as the composite's primary, loading m43 from /tmp/iter43/m43copy after /tmp/iter43/m43 changes the stored root but records no OnChangeModuleRoot call (p4); reloading with -DRootDirectory back to the original directory also changes the root without the call (p4b). Passing neighbours: before/after-phase and resource-change calls in the same runs (p1, p4, p4b).

The Interface comment says OnChangeModuleRoot is "invoked when an existing module is reloaded from a different directory than its known module root", but %Reload compares against a root that was already updated, so extensions that track module locations never learn of the move. A repair would compare against the stored root before ExecutePhases or LoadNewModule overwrites it. Not checked.
```

Severity is Low because ordinary loads succeed. It matters only to source-control extensions that rely on the notification.
