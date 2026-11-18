# Iteration73 predictions: `generate -export` -> `load` round trip

Written 2026-11-18 before reading `ModuleTemplate.cls::{ExportResources,ExportOneResource,Expfilename}` and before
any iteration73 native run. Sources read so far: the ontology blocks `devtools.authoring.generate` and
`devtools.generate.resources`, the `generate` command help in `Main.cls:689-705`, the iteration69 model
`grouping.rs` and handoff.

Disclosure: review-71 C5 already states that `Expfilename` (ModuleTemplate.cls:134-155) writes dotted routines and
include files flat and gives only `.cls` files nested folders, and that `Document.cls:34-38,145` looks for them in
nested folders. Iteration69's handoff cites iteration53 t3, where an exported routine was written without a
Directory. These predictions are therefore informed by that review text, not blind. What is not yet known to me is
the exact folder each type lands in (an extension folder such as `mac/`, or the source folder itself) and whether
the class is written under a `cls/` folder.

Case design: one module per item type, so that one failing resource cannot hide another. Each module exports one
class plus one non-class item by mask, then the items are deleted from the namespace, then `load` runs and the
reader checks that the module record exists and that each item exists again.

| Case | Namespace items exported | Prediction from the ontology | Prediction from the model (grouping.rs + reader) |
| --- | --- | --- | --- |
| x1 | class `Ixa.P.Q`, dotted routine `Ixa.R.Rtn.mac` | "Exported items become resources" and "Routines ... written one by one under their own names". The ontology does not say whether `load` can read them, so it predicts a loadable module. | If the routine file is flat (`<folder>/Ixa.R.Rtn.mac`), ReadItems gives it name `Ixa.R.Rtn` and path of at most one segment, so `directory` is empty (path shorter than the two package segments): entry `Ixa.R.Rtn.MAC` with no Directory. The reader then looks in `mac/Ixa/R/Rtn.mac` (when a `mac` folder exists) or `Ixa/R/Rtn.mac`, finds nothing, so `load` fails and creates no module (C:D73 extension). The class goes to `cls/Ixa/P/Q.cls` (or `Ixa/P/Q.cls`), gets `Ixa.P.PKG` with Directory `cls` (or none), and alone would load. |
| x2 | class `Ixb.P.Q`, undotted routine `IxbRtn.mac` | Same as x1: loadable. | The routine is one segment, so `pkg_len` is 0 and `directory` is the routine's folder. With `mac/IxbRtn.mac` the entry is `IxbRtn.MAC` Directory `mac`; the reader finds it. `load` succeeds and the routine exists after load. |
| x3 | class `Ixc.P.Q`, undotted include `IxcInc.inc` | Same: loadable. | As x2 with `inc`: `IxcInc.INC` Directory `inc`; `load` succeeds. |
| x4 | class `Ixd.P.Q`, dotted include `Ixd.I.Inc.inc` | Same: loadable. | As x1: flat file, no Directory, reader looks in `inc/Ixd/I/Inc.inc`; `load` fails. |
| x0 (neighbour) | class `Ixe.P.Q` only | Loadable. | `Ixe.P.PKG`; `load` succeeds and the class exists again. |

Expected contrast at the reader: x1 and x4 differ from x2 and x3 only in whether the routine or include name has
dots, and the model predicts `load` fails for the dotted ones and succeeds for the undotted ones. If export instead
nests dotted routines (`mac/Ixa/R/Rtn.mac`), both x1 and x4 load, and the review-71 C5 trigger is refuted.

Uncertain before reading code: whether `-export` writes into the answered source folder or into the module path;
whether the class lands under `cls/`; the extension folder names (`mac`, `inc`) and their case; whether the export
mask `Ixa*` matches the routine (C:D55 says routine masks match anywhere in the name).

## Added after reading the deciding code, before any native run

Read: `ModuleTemplate.cls::{ExportOneResource,Expfilename,ReadResources,ReadItems,GroupClasses,AddPackages,
ProcessResources,SaveFile}`, `Main.cls::GenerateModuleXML`, `Module.cls::CalculatedResourcesGet`,
`CodeGeneration.cls::BuildRelationshipSorter`, `ResourceReference.cls::GetSortKey`, `Document.cls::{%OnNew,OnPhase}`.

Export (refines the table above): `Expfilename` puts every item in `<src>/<lower-case extension>/`. Only `.cls`
names are split into package folders, so `Ixa.P.Q.cls` -> `src/cls/Ixa/P/Q.cls`, `Ixa.R.Rtn.MAC` ->
`src/mac/Ixa.R.Rtn.mac`, `Ixd.I.Inc.INC` -> `src/inc/Ixd.I.Inc.inc`, `IxbRtn.MAC` -> `src/mac/IxbRtn.mac`.
Generated `.INT` code is skipped with flag byte 1 = 0. Predicted module.xml: x1 has `Ixa.P.PKG` Directory `cls`
and `<Resource Name="Ixa.R.Rtn.MAC"/>` with no Directory, and its `load` fails. x2 has `IxbRtn.MAC` Directory `mac`
and loads. x3 has `IxcInc.INC` Directory `inc` and loads. x4 has `<Resource Name="Ixd.I.Inc.INC"/>` and fails.

Collapse winner (C3): the writer keeps the resource that `ProcessResources` creates last for a name. It creates
them in `$order` of the TemplateResources keys `pkg:/<directory>:<package>`, which is ascending character-code
order, so the directory whose key string sorts last wins. This is not the same as sorting directory names:

| Case | Same package in | Keys compared | Predicted winner (model) | Naive "later directory name" |
| --- | --- | --- | --- | --- |
| m3 (iter69) | `x`, `y` | `pkg:/x:Id1.B` < `pkg:/y:Id1.B` | `y` | `y` |
| m7 | `a`, `a/b` | `pkg:/a/b:Im7` < `pkg:/a:Im7` because `/` (47) < `:` (58) | `a`: `Im7.X` loads, `Im7.Y` (in `a/b`) is missing | `a/b` |
| m8 | `a`, `ab` | `pkg:/a:Im8` < `pkg:/ab:Im8` because `:` (58) < `b` (98) | `ab`: `Im8.Y` loads, `Im8.X` (in `a`) is missing | `ab` |

The ontology predicts only that one directory's resource is kept, not which.

Existing module.xml (C6): `SaveFile` renames an existing `module.xml` to `module_<UTC timestamp>.xml` before
writing. Predicted for m8, which starts with a module.xml holding a marker: after `generate`, one
`module_*.xml` file exists and holds the marker, and the new module.xml does not.
