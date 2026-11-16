# 00 — IPM system root

Module root: SYS-0

Shared definitions. Every module root hangs from `SYS-0` and comes after these rows, so these terms are in scope for
every module (lexical scope; see the experiment README, "Interpretations").

## Glossary
| Term | Defined by |
|---|---|
| IRIS | SYS-D1 |
| namespace | SYS-D2 |
| module | SYS-D3 |
| manifest | SYS-D4 |
| version | SYS-D5 |
| installed module | SYS-D6 |
| registry | SYS-D7 |
| operator | SYS-D8 |
| module author | SYS-D9 |
| command | SYS-D10 |
| dependency | SYS-D11 |
| phase | SYS-D12 |
| transaction | SYS-D13 |
| resource | SYS-D14 |
| ZPM | SYS-D15 |
| crash | SYS-D16 |
| caller transaction | SYS-D17 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| SYS-0 | SYS-0 | R | IPM is a package manager. It adds, changes and removes software packages on a data server. | I | ipm/README.md#L1 | System root. Baseline: ipm 571e04a1 (tag v0.10.11-beta.28, manifest 0.10.11-SNAPSHOT). |
| SYS-D1 | SYS-0 | D | **IRIS** is the data server that runs IPM. | I | ipm/README.md | InterSystems IRIS; native checks here ran on IRIS Community 2026.1 build 234U (ARM64 container). |
| SYS-D2 | SYS-0 | D | A **namespace** is a named work area in IRIS. It has its own code and data. | I | ipm/README.md#L60 | Code and data may be mapped from shared databases. |
| SYS-D3 | SYS-0 | D | A **module** is a named bundle of code and files that IPM handles as one unit. | I | ipm/src/cls/IPM/Storage/Module.cls | Names are compared in lower case (`$$$lcase`). |
| SYS-D5 | SYS-0 | D | A **version** is a label like 1.2.3 that puts the releases of a module in order. | I | ipm/src/cls/IPM/General/SemanticVersion.cls | Semantic versioning, with prerelease and build parts. |
| SYS-D4 | SYS-0 | D | A **manifest** is the file that describes a module. It gives the name, version, parts and needs of the module. | I | ipm/module.xml | `module.xml`; a `<Module>` document (`.ZPM`). |
| SYS-D6 | SYS-0 | D | An **installed module** is a module with a stored record. The record lives in the namespace. | I | ipm/src/cls/IPM/Storage/Module.cls | `%IPM.Storage.Module` rows (`^IPM.Storage.ModuleD`). |
| SYS-D7 | SYS-0 | D | A **registry** is a place that offers modules. It can be a server or a folder. | I | ipm/src/cls/IPM/Repo/IPackageService.cls | Remote, ORAS, filesystem repositories. |
| SYS-D8 | SYS-0 | D | An **operator** is the person or script that runs IPM. | I | ipm/README.md | |
| SYS-D9 | SYS-0 | D | A **module author** writes a module. The author also writes its manifest. | I | ipm/README.md | |
| SYS-D10 | SYS-0 | D | A **command** is one request that an operator gives to IPM, such as load or install. | I | ipm/src/cls/IPM/Main.cls#L13 | `XData Commands` in Main.cls. |
| SYS-D11 | SYS-0 | D | A **dependency** is a module that a second module needs to work. | I | ipm/src/cls/IPM/Storage/Module.cls#L898 | |
| SYS-D12 | SYS-0 | D | A **phase** is one named step of work on a module, like Compile or Activate. | I | ipm/src/cls/IPM/Lifecycle/Base.cls | Lifecycle phases plus custom phases. |
| SYS-D13 | SYS-0 | D | A **transaction** groups data writes so they are all kept or all undone. | I | ipm/src/cls/IPM/Utils/Module.cls#L1195 | IRIS TSTART, TCOMMIT and TROLLBACK; see the IRIS environment contract in 02. |
| SYS-D14 | SYS-0 | D | A **resource** is one part listed in a manifest, such as a class, a package or a folder of files. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls | |
| SYS-D15 | SYS-0 | D | **ZPM** is the old name of IPM. The shell and some names still use it. | I | ipm/README.md | `zpm "..."` shell entry. |
| SYS-D16 | SYS-0 | D | A **crash** is when an IRIS process stops in the middle of a command. | O:env-halt-open-transaction | validation/iris/Uplift/Probe/Env.cls | HALT, a lost terminal, or `$system.Process.Terminate`. IRIS first closes the process's objects, then ends any open transaction: a background job rolls it back; an interactive terminal asks, and the operator may commit. |
| SYS-D17 | SYS-0 | D | A **caller transaction** is one that was open before the command began. | O:ipm-load-inside-caller-transaction-rolled-back | validation/iris/Uplift/Probe/Scenario.cls | `$TLEVEL > 0` at `%IPM.Main.Shell` entry, e.g. a script that calls IPM inside its own TSTART. |
