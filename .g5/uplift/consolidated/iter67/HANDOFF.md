# Iteration 67: composite source control as seen by editors

Final convergence test (a different underserved workflow), product `571e04a1` unchanged. Not sealed; the main thread integrates it. [Model](../../../Verus_verification/consolidated/iter67/composite.rs), [cases and controls](../../../Verus_verification/consolidated/iter67/cases.json), [bindings](bindings.json), [ledger](working-ledger.json).

## Gap

Iteration43 bound the IPM lifecycle hooks of the composite and iteration55 bound `init -noprompt`. The editor-facing callbacks of `ExtensionBase/Composite.cls`, the menu and namespace routing in `CompositeMethodOverrides.cls`, and `Utils.CorrectInternalNames` had no binding and no owner.

## Setup

These are server-side calls on a `%IPM.ExtensionBase.Composite` instance in USER, with primary `IPMCont.P67` and other `IPMCont.O67` (which defines menu `O67Menu`). Both record every call. The module item is `M67.Main` from module m67, and the non-module item is `N67.X`. No IDE was used, and mapped source control (`MappedSourceControl`) was not enabled.

## Predictions and observations

| Case | Observed | Comment-based | Source |
|---|---|---|---|
| a1-a3 OnBeforeSave, OnAfterSave, OnBeforeLoad, OnAfterCompile, OnBeforeDelete, ItemIconState on `M67.Main.cls`, `N67.X.cls`, `/M67/Main.cls` | primary only (6 calls), other none; names arrive as `M67.Main.CLS` / `N67.X.CLS` | "corresponding" extension | primary only, corrected names |
| g1 GetStatus: primary editable, other not | Editable 0; primary called twice, other once | | same |
| g2 / g3 | Editable 1 / 0 | | same |
| r1 IsReadOnly: primary not read-only, other read-only | **0**, other never called | | 0 (stops at the primary) |
| r2 / r3 | 1 / 0 | | same |
| u1 UserAction type 1 / u2 menu `O67Menu,Do` / u3 unknown menu | primary / other / nobody (OK) | | same |
| d1 unknown method `Custom67` | dispatched to primary (67) | primary | same |
| s1 SetExtensionClasses(P67, [O67, No.Such67]) | returns OK, raw list stored, composite `%New` then **fails** | | same |
| e1 SetExtensionClasses("", [O67]) after P67 | OK, primary still P67 | | same |

## Evidence

- Formal: `composite.rs` 4 verified. Five controls were each rejected: `readonly-all-extensions`, `status-primary-only`, `subs-filtered`, `menu-to-primary` and `empty-primary-clears`.
- Native: [receipt](native/20261116T184753560177Z/observations.json), 55 gated fields. 33 fields replay in Verus, and flips of `r1.ro`, `s1.new` and `g1.editable` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter67/run.py` (12 s). The single exploratory run is kept under `attempts/`.

## New discrepancies

There are two, both Low and both E2E reproduced. Neither was in any register.

1. `IsReadOnly` ignores other extensions, although `GetStatus` lets them make an item uneditable (r1 against g1).
2. A nonexistent other-extension name is accepted with OK and then makes every composite creation fail (s1). This is Low rather than Medium because the trigger used the API directly, not an ordinary command.

Fact, not a defect: an empty primary name leaves the previous primary in place (e1).

## Reach candidates

Newly reached against the iteration64 map:
- `Composite`: OnBeforeSave, OnAfterSave, OnBeforeLoad, OnAfterCompile, OnBeforeDelete, ItemIconState, GetStatus, IsReadOnly, UserAction and %DispatchMethod.
- `CompositeMethodOverrides`: %OnNew, GetPrimaryExtension, GetMenuExtension, MapMenus and GetSubExtensionClasses.
- `Utils.CorrectInternalNames`.

That is 16. Strengthened: SetSubExtensionClasses, SetPrimaryExtensionClass and SetExtensionClasses.

## Limits

Mapped source control (`FindNamespaceForInternalName`, `FindNamespaceForDatabase`), Login/Logout, the compile-batch callbacks (OnBeforeAllClassCompile and the like), OnMenuItem and AfterUserAction are not exercised. Real IDE behaviour (which of GetStatus or IsReadOnly a given editor uses) is not observed. No ObjectScript body is verified directly.

## Proposed owner text

New child statement in `.g5/ontology/concepts/lifecycle.g5.md`, after `lifecycle.source-control`:

```
<a id="lifecycle.source-control.editor"></a>
[D · lifecycle.source-control.editor]

Editor callbacks on IPM's composite source-control class, such as save, load, compile, delete and icon state, go only to the primary extension, with item names normalized (a path such as `/A/B.cls` becomes `A.B.CLS`). A menu action goes to the extension that defines the menu, and other actions go to the primary.

- An item is editable only if every configured extension allows editing. As built, the read-only check asks only the primary, so another extension that marks an item read-only is ignored there (defect C:D-next-1).
- As built, configuring a nonexistent extension class returns success, and every later creation of the composite then fails (defect C:D-next-2). An empty primary name leaves the previous primary in place.
```

Metadata: `agent`, `user-facing-behavior`, Expands [Source-control extension](#lifecycle.source-control). Sources: `ExtensionBase/Composite.cls` (GetStatus comment "An item is editable only if all extensions allow editing"), `ExtensionBase/CompositeMethodOverrides.cls`, `ExtensionBase/Utils.cls::CorrectInternalNames`. Model: [composite.rs](../../Verus_verification/consolidated/iter67/composite.rs) `other_extension_inconsistency`, `neighbors`, `misconfigured_other_breaks_creation`, `empty_primary_ignored` ([handoff](../../uplift/consolidated/iter67/HANDOFF.md)). Add a Findings line for each entry.

Proposed C register entries:

- **Low · Decision: Open — "The composite's read-only check ignores other extensions".** Found by: source review of `ExtensionBase/Composite.cls::IsReadOnly` (it starts from the primary's false and stops at the first false), with iteration67 model `other_extension_inconsistency` and control `readonly-all-extensions`. Reproduced by: E2E reproduced, receipt `iter67/native/20261116T184753560177Z`. With the primary not read-only and the other extension read-only, `IsReadOnly("M67.Main.cls")` returns 0 and the other extension is never called (r1), while `GetStatus` for the same configuration returns Editable 0 (g1). Passing neighbour: a read-only primary gives 1 (r2). Repair, unchecked: OR the other extensions' answers after a false primary answer.
- **Low · Decision: Open — "Configuring a nonexistent composite extension succeeds and then breaks source control in the namespace".** Found by: source review of `CompositeMethodOverrides.cls::SetSubExtensionClasses` (validated list discarded, raw list stored) and `%OnNew`, with model `misconfigured_other_breaks_creation` and control `subs-filtered`. Reproduced by: E2E reproduced, same receipt. `SetExtensionClasses("IPMCont.P67",$lb("IPMCont.O67","No.Such67"))` returns OK and stores `No.Such67`, after which `%New` of the composite fails (s1). Passing neighbour: the same call without the bad name creates the composite (setup.new). Repair, unchecked: store the validated list, or refuse unknown class names.
