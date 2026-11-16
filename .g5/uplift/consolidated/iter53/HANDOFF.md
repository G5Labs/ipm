# Iteration 53: `generate` manifest authoring (convergence test: a different underserved workflow)

Product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter53/generator.rs), [cases, controls and session](../../../Verus_verification/consolidated/iter53/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owners: [devtools.authoring](../../../ontology/features/developer-tools.g5.md#devtools.authoring) and [devtools.generate.resources](../../../ontology/features/developer-tools.g5.md#devtools.generate.resources). Not sealed; the main thread integrates it.

## How the command was driven

`generate -t <dir>` is non-interactive. The interactive form reads its answers through `%Library.Prompt` from the terminal, so the harness session pipes the answer lines straight after the `zpm "generate ..."` line. The order is name, version, description, keywords, then with `-author` person, organization, license, copyright and notes. After those come the source folder, the export when `-export` is given, the tests folder, the web-app list (USER lists `/csp/user`) with CORS `N` and JWT `N`, and the dependencies. The exact lines are in `cases.json` under `session`.

## Predictions and observations

| Case | Observed | Owner/help predicted | Source predicted |
|---|---|---|---|
| t1 `generate -t g1` | template: Name template, 1.0.0, License MIT, CopyrightDate current year, dependency `module` | template written | same |
| t2 `generate g2 -author`, license `Apache-2.0`, copyright `2024` | `<License>2024</License>`, `<CopyrightDate>Apache-2.0</CopyrightDate>` | stored as answered | swapped (Main passes license and copyright in the opposite order to `SetAuthorProps`) |
| t2 classes `G53.A.X`, `G53.B.Y`, `Other.Z`, tests `G53T.T` | `G53.PKG`, `Other.PKG`, `UnitTest /tests/ Package=G53T Phase=test`; no `G53.A.PKG` or `G53T.PKG` | owned behaviour | same |
| t2 dependency answer `g53dep:1.x` | ModuleReference `g53dep` `1.x` | listed | same |
| e1 `-export 00000,G53E,G53E.Skip` | exported `G53E.Keep.cls`, routines `G53E.Rout`, `G53E.Skip.R` and `ZZG53E`; not `G53E.Skip.Gone.cls`. All exported routines become resources | ignored items not exported | routines ignore the ignore list and match the mask anywhere |

In explore2 the routine fixtures were not in the namespace, because `LoadDir` loaded only the classes in this setup. The reader now loads each `.mac` explicitly, and explore4 supplied the expected values. All attempts are under `attempts/`.

## Evidence

- Formal: `generator.rs` 3 verified, 0 errors. Four controls were each rejected: `author-args-in-order`, `routines-honour-ignore`, `routines-prefix-mask` and `no-package-sharing`.
- Native: [receipt](native/20261116T180748958256Z/observations.json), 30 fields, all gated, network none, source pin matched. Replay: 11 fields in Verus ([receipt](replay/receipt.json)). Flips of `t2.licenseIsYear`, `e1.skiprout` and `t2.copyrightIsLicense` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter53/run.py` (11 s).

## Discrepancies

This workflow produced two new discrepancies, both reproduced end to end and both Low:

1. `generate -author` swaps the License and CopyrightDate answers in the written manifest.
2. `generate -export` applies the ignore list only to classes. Routines are matched when the mask appears anywhere in the name, so ignored and unrelated routines are exported and listed as resources.

## Reach candidates

Ten `Storage/ModuleTemplate.cls` members are newly reached against the iteration51 map: SetAuthorProps, NewTemplate, ExportResources, ExportOneResource, Expfilename, SharedPackage, AddDependant, RelativeFolder, GenerateTemplate and SetTemplateProps. `Main.cls::GenerateModuleXML`, ReadResources, ReadItems, GroupClasses and AddPackages gain this role. `ReadGlobals` and `GetGlobalsList` have no CLI caller and stay unreachable from `generate`. Web apps were reached only with an empty selection.

## Limits

DFI export, flag bytes other than `00000`, system, generated and mapped filtering, web-app selection and the tests-inside-source case are not exercised here. String predicates and `%Library.Prompt` are providers. No ObjectScript body is verified directly.

## Proposed owner text

- **`devtools.authoring`, `generate` bullet, added sub-bullets:**
  - "`generate -t <dir>` writes a fixed example manifest without asking questions."
  - "Interactive `generate` writes the answers it is given, plus source resources grouped by package, test classes as UnitTest entries and the dependencies entered as `name:version`. As built, with `-author` the License and Copyright Date answers are stored in each other's fields (defect C:D-next-1)."
  - "`-export <flags>,<mask>,<ignore>` first exports matching namespace items into the source folder. As built, the ignore list applies only to classes, and routines match when the mask appears anywhere in their name, so ignored or unrelated routines are exported and become resources (defect C:D-next-2)."
- **Metadata (`devtools.authoring`):** "Iteration53 `generate`: [generator.rs](../../Verus_verification/consolidated/iter53/generator.rs) `author_swapped`, `export_neighbors`, `grouping_neighbors` ([handoff](../../uplift/consolidated/iter53/HANDOFF.md))." Add a Findings line for each entry.
- **C register, Low · Decision: Open: "`generate -author` stores the license and copyright answers in each other's fields".**
  - Found by: source review of `Main.cls::GenerateModuleXML` against `Storage/ModuleTemplate.cls::SetAuthorProps`, and the iteration53 model `author_swapped` with control `author-args-in-order`.
  - Reproduced by: E2E reproduced, receipt `iter53/native/20261116T180748958256Z`. Answers `Apache-2.0` (license) and `2024` (copyright) produce `<License>2024</License>` and `<CopyrightDate>Apache-2.0</CopyrightDate>` (case t2). Passing neighbour: the template's own License MIT and CopyrightDate year are in the right fields (t1).
  - Repair, unchecked: pass the answers in `SetAuthorProps` parameter order.
- **C register, Low · Decision: Open: "`generate -export` ignores the ignore list for routines and matches routine masks anywhere in the name".**
  - Found by: source review of `Storage/ModuleTemplate.cls::ExportOneResource`, and the iteration53 model `export_neighbors` with controls `routines-honour-ignore` and `routines-prefix-mask`.
  - Reproduced by: E2E reproduced, same receipt. With `-export 00000,G53E,G53E.Skip`, class `G53E.Skip.Gone` is not exported, but routine `G53E.Skip.R` and the unrelated `ZZG53E` are exported and listed as resources (e1, t3). Passing neighbours: `G53E.Keep` and `G53E.Rout` are exported.
  - Repair, unchecked: apply the same prefix mask and ignore test to routines as to classes.
