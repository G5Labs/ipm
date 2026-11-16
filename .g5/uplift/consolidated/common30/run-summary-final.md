| Iteration | Workflow | Proof units | Controls | Native fields | Replayed | Defects recorded in ledger |
|---|---|---|---|---|---|---|
| 30 | module-action/RunOnePhase phase selection -> ExecutePhases -> executed phases, compiled code, custom-hook effe | 12 | 7 | 276 | 243 | C:D34, C:D35 |
| 31 | installed modules constrain dependency/version choice (deps.installed): BuildDependencyGraph choice and LoadMo | 4 | 6 | 107 | 106 | none |
| 32 | sync [-delete] on developer-mode modules: file change/delete detection -> reverse index routing -> namespace i | 15 | 7 | 206 | 191 | C:D36, C:D37 |
| 33 | list-dependents <module> [version] [-r repos]: dependency graphs -> dependents tree -> printed report | 13 | 7 | 524 | 471 | C:D38 |
| 34 | filesystem repository version index and match query (SemanticVersion.Index/Find via Repo/Filesystem/Cache Orde | 12 | 6 | 53 | 36 | C:D39 |
| 35 | list-installed [flags] -> installed module rows (namespace) -> flat list / tree / sort -> printed report | 13 | 6 | 441 | 368 | C:D42 |
| 36 | module <InstallerClass>: load/uninstall -> Activate/Configure and Unconfigure phases -> installer callbacks wi | 6 | 7 | 81 | 81 | C:D41 |
| 37 | Copy resource processor: resource copied from a Source namespace into the installing namespace during Reload,  | 5 | 5 | 136 | 128 | C:D40 |
| 38 | history find / details [-phases] rendering: stored %IPM_General.History rows -> VersionToString, TimeToString, | 5 | 5 | 280 | 141 | C:D43 |
| 39 | editing an installed module manifest as the <name>.ZPM document (document API used by IDEs): save/compile -> s | 6 | 5 | 84 | 75 | C:D44, C:D45 |
| 40 | UnitTest resource processor: test-class load before the test phase and removal after it, by developer mode and | 4 | 5 | 48 | 42 | C:D48 |
| 41 | config set/get/delete/list over UniversalSettings keys and their readers (getters used by consumers) | 6 | 5 | 132 | 125 | none |
| 42 | preload code: Initialize phase loads/compiles the module preload directory -> preload classes available; -only | 4 | 4 | 70 | 20 | C:D46, C:D47 |
| 43 | namespace source-control extension -> IPM lifecycle/document hooks (%OnBeforePhase/%OnAfterPhase/%OnModifyReso | 8 | 5 | 278 | 260 | C:D51 |
| 44 | arrange <module>: ExportDocumentForObject rewrites the module manifest (stored .ZPM document and module.xml on | 5 | 4 | 168 | 43 | C:D49, C:D50 |
| 45 | sync file discovery: scan directory collection/dedup, directory walk and hashing, path normalization and path- | 5 | 4 | 49 | 6 | none |
| 46 | show <module>: stored manifest -> ModuleSummary sections (fields, resources by section, dependencies, system r | 5 | 4 | 113 | 52 | C:D52 |
| 47 | test result report formats: test -only -f json|yaml|toon (device summary) and -output-file .json/.yaml/.toon/. | 7 | 4 | 71 | 33 | C:D53 |
| 48 | CPF resource: merge a CPF file into the instance configuration at a declared phase (Before/After) or custom ph | 3 | 4 | 17 | 6 | none |
| 49 | import <file|dir> [-norecurse] [-nocompile]: files -> classes loaded/compiled in namespace -> command status/o | 6 | 5 | 67 | 65 | none |
| 50 | default-modifiers -set/-get/-delete -> stored namespace defaults -> merged into later commands (data -D and fl | 6 | 5 | 66 | 50 | none |
| 51 | `information` (info) lists installed top-level modules: installed modules minus every module named as a depend | 2 | 3 | 21 | 11 | none |
| 52 | module record persistence on load of a changed manifest: save hooks (%OnAddToSaveSet, %OnBeforeSave, %OnOpen,  | 3 | 4 | 35 | 17 | none |
| 53 | generate -t <dir> (template) and generate <dir> -export <spec> (export namespace items) -> written module.xml  | 3 | 4 | 30 | 11 | C:D54, C:D55 |
| 54 | help [<command>] rendering from the Commands XDATA and the version command | 1 | 5 | 125 | 35 | none |
| 55 | zn/namespace listing and switching, exec/cos, init non-interactive decisions | 3 | 5 | 214 | 22 | C:D56, C:D57 |
| 56 | localization resources: .LOC message document (Default.LocalizedMessages, Directory localize) loaded into the  | 4 | 4 | 32 | 27 | C:D58, C:D59 |
| 57 | deprecated CSPApplication resource processor: load (non-dev, dev), reload, uninstall of a module declaring a C | 5 | 4 | 174 | 18 | C:D60, C:D61 |
| 58 | DeepSeeItem (.DFI) resources: load (non-dev, -dev), reload with changed/removed item, uninstall -> DFI documen | 3 | 4 | 48 | 12 | C:D62, C:D63 |
| 59 | automatic resource mappings: Reload calls each resource processor OnConfigureMappings (unless NoMapping) -> na | 5 | 5 | 433 | 280 | C:D65, C:D66 |
| 60 | legacy localized messages: <Resource ProcessorClass="LegacyLocalizedMessages" includeFiles=... merge=...> mess | 6 | 4 | 106 | 12 | C:D59, C:D64 |
| 61 | WebApplication resource processor: declared attributes (incl. expressions) -> web application properties on in | 1 | 5 | 171 | 43 | none |
| 62 | Interoperability resources (.LUT lookup tables, .HL7 custom schemas) in an interoperability-enabled namespace: | 4 | 4 | 92 | 11 | none |
| 63 | uninstall cleanup across ordinary resource kinds (CLS, PKG, INC, MAC/INT, GBL name-only and Global attribute,  | 5 | 5 | 122 | 37 | C:D40, C:D59, C:D63, C:D64, C:D65, C:D66, C:D67, C:D68 |
| 64 | failed ordinary load (normal mode in a transaction, and developer mode) with side-effecting resources: FileCop | 4 | 5 | 81 | 72 | none |
| 65 | developer-mode versus normal-mode refresh across resource kinds (CLS/PKG, INC, MAC, GBL, FileCopy, WebApplicat | 5 | 5 | 126 | 114 | C:D36, C:D47, C:D48 |
| 66 | <module> document: %Document phase -> dependency recursion -> generated API documentation artefacts, status, h | 2 | 5 | 63 | 42 | C:D69 |
| 67 | editor-facing callbacks of IPM composite source control (%IPM.ExtensionBase.Composite): which configured exten | 4 | 5 | 55 | 33 | C:D70, C:D71 |

New defects this run by severity: High 3 (C:D39, C:D40, C:D60); Low 23 (C:D37, C:D42, C:D43, C:D45, C:D47, C:D48, C:D50, C:D51, C:D52, C:D53, C:D54, C:D55, C:D56, C:D57, C:D59, C:D61, C:D62, C:D63, C:D64, C:D68, C:D69, C:D70, C:D71); Medium 12 (C:D34, C:D35, C:D36, C:D38, C:D41, C:D44, C:D46, C:D49, C:D58, C:D65, C:D66, C:D67)
