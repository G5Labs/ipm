# Iteration 69 predictions: `generate` resource grouping

Written 2026-11-18T01:30Z, before opening `Storage/ModuleTemplate.cls`, `Main.cls::GenerateModuleXML`, the iteration53 native fixtures and results, the census gap texts for `devtools.generate.resources` / `devtools.authoring.generate`, and iteration68 `HANDOFF.md` §4.

## Exposure disclosure

These predictions are not an independent discovery test. The iteration68 census builder already read the deciding code and summarized it in its handoff and in the census gap texts, and the master that wrote this assignment saw those summaries. The assignment text itself leaks hints: it says the census grades "#8 emitted group name absent", that iteration68 "reported that B3 misstates the group name and that test-folder leftovers may be dropped", and it lists the method names `AddPackages`, `GroupClasses`, `SharedPackage`. I read the assignment before writing this file. I have not opened the passages or the code.

Inputs used here: the owning blocks `devtools.authoring.generate` and `devtools.generate.resources` (`.g5/ontology/features/developer-tools.g5.md`), the iteration53 spec `generator.rs::shared_resource` / `grouping_neighbors`, and the iteration6 contract `workflows.rs::{application_item,test_item,template_status}`.

## What each source determines

- Ontology: "Classes are grouped by directory and their shared top-level package." `devtools.authoring.generate`: "source resources grouped by package". It does not name the emitted resource or say what happens to a class that shares its group with nobody.
- iteration53 model: `shared_resource(t1, t2) == (t1 == t2)` over an abstract top-level identifier. It says whether two classes are in one resource. It has no directory, no resource name, no singleton rule and no multiplicity.
- iteration6 model: `application_item` excludes items below the tests directory and items that are not loaded or have an unsupported extension. `test_item` needs an existing tests directory.

## Reader

The final reader is the `<Resource Name="...">` and `<UnitTest .../>` list in the `module.xml` written by `ModuleTemplate.SaveFile`. Beyond it, `load <dir>` of the generated module turns each `Resource` into a module resource (a `.PKG` resource owns every class in that package).

## Cases

Source folder `src`, tests folder `tests` unless stated. "Onto" = prediction from the ontology alone; "Model" = prediction from iteration53 alone.

| # | Tree (file -> class) | Onto | Model | Difference the reader sees |
|---|---|---|---|---|
| N1 | `src/A/B/C.cls` (A.B.C), `src/A/B/D.cls` (A.B.D) | One package resource. The text says "shared top-level package", so `A.PKG`. | One resource (`shared_resource(A,A)`), name undetermined. | `A.PKG` vs `A.B.PKG` vs two `.CLS`. |
| N2 | `src/A/X.cls` (A.X), `src/B/Y.cls` (B.Y), i.e. different top-level packages | Two resources, each its own package: `A.PKG`, `B.PKG`. | Two resources (`!shared_resource(A,B)`), names undetermined. | Package vs individual class entries for a group of one. |
| N3 | `src/A/B/C.cls` alone | Ambiguous. "Grouped by package" suggests `A.PKG`. My own guess is that a group of one is written as `A.B.C.CLS`. | Says nothing (needs two classes). | `A.PKG` vs `A.B.C.CLS`. |
| N4 | `src/A/B/C.cls` (A.B.C), `src/A/D/E.cls` (A.D.E): shared top level, different second level, different directories | "By directory" splits them. Two groups of one, so two entries (see N3). If directory meant the top-level source subdirectory, then one `A.PKG`. | Same top level, so one resource. The model and the ontology disagree here unless "directory" is coarse. | One `A.PKG` vs two entries. |
| N4b | A.B.C and A.D.E in the same directory | One `A.PKG`. | One resource. | `A.PKG` vs `A.B.PKG`/`A.D.PKG`. |
| N5 | `src/x/A.B.C.cls` and `src/y/A.B.D.cls` (flat file names, same package, two directories) | Two groups (directory differs). Each a group of one, so `A.B.C.CLS`, `A.B.D.CLS`, or a duplicated `A.PKG`. | One resource (same top level). | Two entries, or a duplicate `A.PKG`, vs one. |
| N6 | `src/Foo.cls` (class `Foo`, one segment, directly in the source root) | Individual `Foo.CLS`. A one-segment name has no package to share. | Undetermined. | `Foo.CLS` present vs missing. |
| N7 | `src/tests/T/U.cls` (T.U) with tests folder `src/tests`, plus `src/A/B/C.cls`, `src/A/B/D.cls` | `T.U` is not an application resource. One `UnitTest Name="T" Directory="tests"-relative Phase="test"`. App classes give `A.PKG`. | iteration6: `T.U` is a test item, not an application item. Grouping says nothing. | UnitTest present, and no `T.PKG`/`T.U.CLS` resource. |
| N8 | `src/A/B/C.cls`, `src/A/B/D.cls`, `src/r/R1.mac` (routine R1), `src/r/I1.inc`, `src/notes.txt` | Package resource for A, `R1.MAC` and `I1.INC` as individual resources, `notes.txt` skipped. | Grouping is class-only. iteration6: `.txt` is not an application item. | Routine entries present, `.txt` absent. |
| N9 | Unloadable class file (syntax error) next to N1's classes | Skipped, manifest written without it (working assumption). | iteration6 `application_item` false when not loaded. | No entry for the bad file and no failure. |
| N10 | Missing source folder | Fails before resources are added. | `template_status(false) == 0`. | Error status and no resources. |

## Ordering and duplicates

Onto: silent. Expected: each class appears in exactly one resource, no resource name twice, and entries in some deterministic (probably name) order. iteration53 is silent on all three.

## What would change the ontology

- If a group emits `A.B.PKG` (the longest shared prefix) rather than `A.PKG`, the "top-level" wording is wrong.
- If a class left over after grouping does not appear in any resource, that contradicts "source resources grouped by package" (classes silently missing from the manifest) and is a defect candidate.
- If a single class gets `A.PKG`, then loading that module claims every other class under `A` in the namespace.

## Results (written after reading the code and running the fixtures)

| # | Prediction (onto / model) | Observed (native receipt `native/20261118T014707781494Z`) | Held? |
|---|---|---|---|
| N1 | `A.PKG` / one resource | `Ia1.B.PKG` (Directory cls) | Ontology wording wrong (the name is the deepest shared package). The model got the count right but had no name. |
| N2 | `A.PKG`, `E.PKG` / two | `Ia4.PKG`, `Ia5.PKG` | Held. |
| N3 | my guess `A.B.C.CLS` / undetermined | `Ia3.B.PKG` | Missed: a group of one is still a package resource. |
| N4 | two entries (I read "directory" as the file's folder) / one | `Ia2.PKG`, one entry | Missed. Directory is the folder above the package folders (`cls`), so both classes share it. The iteration53 model was right. |
| N4b | `A.PKG` / one | (same as N4) | Held. |
| N5 | two groups / one | Package layout (`x/Id1/B/C.cls`, `y/Id1/B/E.cls`): two groups are formed, then module.xml keeps only `Id1.B.PKG` with Directory y, and `Id1.B.C` never loads (C:D72). The flat-name variant loses the directory instead (C:D73). | Neither source predicted the loss. |
| N6 | `Foo.CLS` / undetermined | Written as `Ir2a.cls` with no Directory. `load` fails when a `cls` folder exists (C:D73) and succeeds without one (compiled as `User.Ir2e`). | Name held. The load outcome was not predicted. |
| N7 | UnitTest, no resource | `<UnitTest Name="/src/tests/" Package="Ie1T" Phase="test"/>`, no resource | Held. |
| N8 | `R1.MAC`, `I1.INC`, `.txt` skipped | `Ir69.MAC` (mac), `Ii69.INC` (inc), notes.txt absent | Held. Not predicted: classes keep the listing's lowercase `.cls` while routines are uppercase. |
| N9 | unloadable skipped | Headerless file skipped. A class header with a broken body is listed (`Ig2.PKG`). | Partly. "Cannot load" means "cannot be listed". |
| N10 | fails, no resources | "Directory .../nosrc/ doesn't exist", no module.xml | Held. |
| Order | name order | Sorted by `GetSortKey`: PKG/CLS by name, then MAC, INC, other extensions (including lowercase `cls`), then `/` paths | Partly. Order is not modeled. |
| Duplicates | none | No duplicate names, but only because the writer drops one (C:D72) | Held for a different reason. |

Tests-folder leftovers (routines, package-less classes) are written nowhere. iteration68 had already reported this and the assignment repeated it, so it is not an independent result.
