# 08 — Resources, packaging and author code

Module root: RES-0

Scope: resource scopes, test inclusion in packages, deployed resources and their mappings (`makedeployed`),
FileCopy semantics (except removal on uninstall, owned by 05), Python wheels and pip, Invoke hooks and custom
phases, and the `package`, `module-action`, `exec`, `run-from-file` and `generate` commands. Baseline: ipm 571e04a1.
Evidence was first by inspection of source and tests at that revision. Native checks N1-N22 (intents/07-09-seed-challenge.md) were then run on IRIS 2026.1 build 234U in container `ipm-uplift-iris-nsp` (N1 in a throwaway fresh container); `O:` tags name their observations (logs `validation/logs/8*-nsp-*.log`), `T:` tags name IPM test classes run here (log 892-nsp-ipm-tests).

## Glossary
| Term | Defined by |
|---|---|
| scope | RES-D1 |
| package file | RES-D2 |
| deployed resource | RES-D3 |
| hook | RES-D4 |
| custom phase | RES-D5 |
| file copy | RES-D6 |
| wheel | RES-D7 |
| developer mode | RES-D8 |
| command file | RES-D9 |
| module action | RES-D10 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| RES-0 | SYS-0 | R | Module authors list the parts of a module. They can also add code for IPM to run. This part says how IPM treats those parts and when it runs that code. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L1; ipm/src/cls/IPM/Storage/InvokeReference.cls#L1 | Seed "Resource scope"; index "Extension contracts". |
| RES-D1 | RES-0 | D | A resource may have a **scope**. It says the resource is only for the test phase or the verify phase. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L19 | `Scope` attribute. `<UnitTest Phase="test">` acts the same way. |
| RES-D2 | RES-0 | D | A **package file** is the .tgz file that the package command writes. | I | ipm/src/cls/IPM/Lifecycle/Module.cls#L173 | |
| RES-D3 | RES-0 | D | A **deployed resource** is a resource marked Deploy. It ships as built code with no source. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L39 | `Deploy="true"`. |
| RES-D4 | RES-0 | D | A **hook** is a class method that a manifest asks IPM to call during a phase. | I | ipm/src/cls/IPM/Storage/InvokeReference.cls#L1 | `<Invoke Class= Method= Phase= When= CustomPhase=>`. |
| RES-D5 | RES-0 | D | A **custom phase** is a phase name that a hook makes up. IPM has no built-in step for it. | I | ipm/src/cls/IPM/Storage/InvokeReference.cls#L14 | Also `CustomPhaseMixin` processors. |
| RES-D6 | RES-0 | D | A **file copy** is a resource that copies a file or folder to a target path. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L7 | `<FileCopy Name= Target=>`. |
| RES-D7 | RES-0 | D | A **wheel** is a Python package file that is ready to install. | I | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L8 | `<PythonWheel Name=>`, default folder `wheels`. |
| RES-D8 | RES-0 | D | **Developer mode** is a way to load a module so its source stays easy to edit and debug. | I | ipm/src/cls/IPM/Main.cls#L306 | `-dev`. |
| RES-D9 | RES-0 | D | A **command file** is a file of IPM shell commands. | I | ipm/src/cls/IPM/Main.cls#L344 | For `run-from-file`. |
| RES-D10 | RES-0 | D | A **module action** is a command that names a module and then one or more phases. | I | ipm/src/cls/IPM/Main.cls#L32 | `module-action` is the default command: `<module> <phase>...`. |
| RES-B1 | RES-0 | B | A resource with no scope is loaded and compiled on every load. | I; P:res_present | ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L75; ipm/src/cls/IPM/Lifecycle/Base.cls#L1052 | Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B2 | RES-0 | B | A resource with a scope is left out of an ordinary load. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L64; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L71; T:Test.PM.Integration.Scopes; P:res_present | ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L194; ipm/src/cls/IPM/Storage/ResourceReference.cls#L258 | Seed confirmed. A class and a whole package are both covered. T:Test.PM.Integration.Scopes has 1 skipped assertion: the registry step of Base.Setup (no `registry` service here); the scope checks themselves ran (see RES-B7). Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B3 | RES-B2 | B | It is also left out of a load in developer mode. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L96; T:Test.PM.Integration.Scopes | ipm/src/cls/IPM/Lifecycle/Base.cls#L625 | Asserted for both plain and `-dev` loads via `AssertScopedClassLoadsOnTest`. `tSkipScoped` at L625 is set but never read. T:Test.PM.Integration.Scopes has 1 skipped assertion: the registry step of Base.Setup (no `registry` service here); the scope checks themselves ran (see RES-B7). |
| RES-B4 | RES-B2 | B | The test phase loads and compiles each resource scoped to test. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L100; T:Test.PM.Integration.Scopes; P:res_present | ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L75; ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L184 | Holds after a normal and after a `-dev` load. Compile skips it (L195) so the test phase owns it. T:Test.PM.Integration.Scopes has 1 skipped assertion: the registry step of Base.Setup (no `registry` service here); the scope checks themselves ran (see RES-B7). Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B5 | RES-B4 | B | Before that, the test phase installs the dependencies scoped to test. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L129; T:Test.PM.Integration.Scopes | ipm/src/cls/IPM/Lifecycle/Base.cls#L1261 | T:Test.PM.Integration.Scopes has 1 skipped assertion: the registry step of Base.Setup (no `registry` service here); the scope checks themselves ran (see RES-B7). |
| RES-B6 | RES-B2 | B | The verify phase installs the dependencies that are scoped to verify. It does so first. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L163; T:Test.PM.Integration.Scopes | ipm/src/cls/IPM/Lifecycle/Base.cls#L1461 | The test asserts only the command status after verify, not the class. T:Test.PM.Integration.Scopes has 1 skipped assertion: the registry step of Base.Setup (no `registry` service here); the scope checks themselves ran (see RES-B7). |
| RES-B7 | RES-B2 | B | A dependency with a scope joins the dependency graph only when its phase will run. | I; X:tests/integration_tests/Test/PM/Integration/Scopes.cls#L38; T:Test.PM.Integration.Scopes | ipm/src/cls/IPM/Storage/Module.cls#L838 | Scopes run here: 6 methods pass with 65 assertions; 1 assertion is skipped (counted in INDEX). The skip is `Base.Setup`'s registry step (TEST_REGISTRY unset; the `registry` service is absent here), after which Setup returns OK and TestEverything still runs every BuildDependencyGraph check on modules read from XData, which need no registry (O:nc-nsp-ipm-test-run-1-assert-status). So the skipped assertion is not one of the graph checks this statement rests on. |
| RES-B8 | RES-B2 | B | IPM accepts only test or verify as a scope. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L20 | `VALUELIST = ",test,verify"`. |
| RES-B9 | RES-0 | B | In one namespace, a resource name can belong to only one module. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L461 | Except when the other copy is marked for update. |
| RES-B10 | RES-B9 | B | A resource listed twice in one module is refused. | I | ipm/src/cls/IPM/Storage/ResourceReference.cls#L472 | |
| RES-B11 | RES-0 | B | A resource marked Generated is never loaded or exported by IPM. | I; P:res_present | ipm/src/cls/IPM/Storage/ResourceReference.cls#L25; ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L106 | It is still compiled. Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B12 | RES-0 | B | The package command builds a package file from the parts of the module. | I; O:nc-res-N12b-package-dev | ipm/src/cls/IPM/Lifecycle/Module.cls#L134 | Runs Initialize to Activate first, then Package. |
| RES-B13 | RES-B12 | B | By default, a package file leaves out resources for test and verify. | I; X:tests/integration_tests/Test/PM/Integration/IncludeTests.cls#L29; T:Test.PM.Integration.IncludeTests; P:res_export | ipm/src/cls/IPM/Lifecycle/Base.cls#L1306 | Seed confirmed. Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B14 | RES-B13 | B | The -include-tests flag puts them in. | I; X:tests/integration_tests/Test/PM/Integration/IncludeTests.cls#L54; T:Test.PM.Integration.IncludeTests; P:res_export | ipm/src/cls/IPM/Lifecycle/Base.cls#L1308 | Adds both Test and Verify to the phase list. Also on `publish`. Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B15 | RES-B13 | B | IncludeTests set to 1 in the manifest also puts them in. | I; X:tests/integration_tests/Test/PM/Integration/IncludeTests.cls#L104; T:Test.PM.Integration.IncludeTests; P:res_export | ipm/src/cls/IPM/Lifecycle/Base.cls#L1308; ipm/CHANGELOG.md#L109 | Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B16 | RES-B15 | B | The flag can only add tests. It cannot drop tests that the manifest asks for. | I; P:res_export | ipm/src/cls/IPM/Main.cls#L143; ipm/src/cls/IPM/Lifecycle/Base.cls#L1308 | The flag only sets 1; `-DIncludeTests=0` would override. Proved in models/verus/scopes.rs for any number of resources and every command (premises: none; preloaded resources and custom phases out of the model). |
| RES-B17 | RES-B12 | B | With -path P, the package file is written as P.tgz. | I; X:tests/integration_tests/Test/PM/Integration/DeployedMappings.cls#L18; O:nc-res-N12b-package-dev; T:Test.PM.Integration.DeployedMappings | ipm/src/cls/IPM/Lifecycle/Module.cls#L146; ipm/src/cls/IPM/Lifecycle/Module.cls#L173 | Contents are staged in a random folder under P that is not removed. The test then loads P.tgz (L20, L72). Native N12: `-path /tmp/nc-pkg/dm` wrote /tmp/nc-pkg/dm.tgz. |
| RES-B18 | RES-B12 | B | A package file leaves out dependencies. Use -export-deps 1 to put them in. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L18; ipm/src/cls/IPM/Lifecycle/Base.cls#L1306 | |
| RES-B19 | RES-B12 | B | A package file always keeps the readme, changelog and license. It also keeps requirements.txt from the module root. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1384 | Case-insensitive names. |
| RES-B20 | RES-B12 | B | When packaging ends, IPM tells the operator where the package file is. | F:F-RES-4 | ipm/src/cls/IPM/Lifecycle/Module.cls#L183 | Native N12 confirms F-RES-4: no path line without `-v`, "Module package generated:" with `-v` (O:nc-res-N12c-package-verbose). |
| RES-B21 | RES-0 | B | Loading from source compiles a deployed resource like any other resource. | I | ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L216; ipm/src/cls/IPM/Lifecycle/Base.cls#L1134 | No deployed project file in the source folder, so `LoadFromDeployedProject` returns 0. |
| RES-B22 | RES-0 | B | A package file holds deployed resources as built code without source. | I; X:tests/integration_tests/Test/PM/Integration/DeployedPackageLoad.cls#L34; T:Test.PM.Integration.DeployedPackageLoad; O:nc-gap-P8-deployed-export-control | ipm/src/cls/IPM/Lifecycle/Base.cls#L1359; ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L296 | A Studio project `.prj` per module. |
| RES-B23 | RES-B22 | B | Packaging also makes those classes deployed in this namespace. That is so even in developer mode. | I; O:nc-res-N12b-package-dev | ipm/src/cls/IPM/Lifecycle/Module.cls#L142; ipm/src/cls/IPM/Lifecycle/Base.cls#L1896; issue #815; PR #898 | `MakeClassDeployed` removes class source from the namespace. Message at Base.cls#L1837 says items WILL be deployed when packaging. Native N12: after `package` of a `-dev` module, DeployMapA.First is Deployed=1 and its method source can no longer be read. Intended: issue #815 (maintainers chose packaging 'in the current namespace regardless of whether it is in dev mode or not', accepting a reload from source), fixed by PR #898. Intent: DEC-RES-3 resolved (evidence), decisions/research/nsp.md. |
| RES-B24 | RES-B22 | B | Packaging deployed code adds a need for the IRIS version it was built on. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1881 | `SystemRequirements.VersionString` set to the current platform version; see 07 for how that need is checked. |
| RES-B25 | RES-B22 | B | If a deployed item cannot be added to the package, packaging fails. It never ships the source in its place. | F:F-RES-1; O!:nc-gap-P8-deployed-export-forced-error | ipm/src/cls/IPM/ResourceProcessor/Default/Document.cls#L256 | O! (refutes): with the deployed-item export forced to fail, `package` returned OK and the .tgz ships src/cls/N2Dpl/Marker.cls (shipsMarkerSourceFile=1, shipsDeployedProject=0). The probe value filesWithMethodSource also holds a leaked `grep: .../studio-project/Deployed.xml: No such file` message (the deployed project file was absent); only its first path, the marker source, is a match. |
| RES-B26 | RES-B22 | B | When an existing mapping covers a deployed resource, IPM adds its own mapping. The code then lands in the namespace's own store. | I; X:tests/integration_tests/Test/PM/Integration/DeployedMappings.cls#L75; T:Test.PM.Integration.DeployedMappings | ipm/src/cls/IPM/Lifecycle/Base.cls#L721; ipm/src/cls/IPM/ResourceProcessor/Default/Class.cls#L15 | Mappings are made before any load. |
| RES-B27 | RES-B26 | B | With -DNoMapping=1, the code goes where the existing mappings send it. | I; X:tests/integration_tests/Test/PM/Integration/DeployedMappings.cls#L86; T:Test.PM.Integration.DeployedMappings | ipm/src/cls/IPM/Lifecycle/Base.cls#L722 | |
| RES-B28 | RES-B22 | B | Deployed dependencies that ship with the module are deployed too. | I; X:tests/integration_tests/Test/PM/Integration/DeployedItems.cls#L123; T:Test.PM.Integration.DeployedItems | ipm/src/cls/IPM/Lifecycle/Base.cls#L1360 | |
| RES-B29 | RES-0 | B | makedeployed marks the deployed classes and routines of a module as deployed. | I; O:nc-res-N12a-makedeployed-dev | ipm/src/cls/IPM/Lifecycle/Base.cls#L1825 | Routines are deleted except in developer mode. |
| RES-B30 | RES-B29 | B | makedeployed leaves alone each resource that is not marked Deploy. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1863; ipm/src/cls/IPM/Lifecycle/Base.cls#L1914 | Path resources (leading `/`) and non-code types are skipped. |
| RES-B31 | RES-B29 | B | In developer mode, this command just reports what it would do. It changes nothing. | F:F-RES-3 | ipm/src/cls/IPM/Lifecycle/Base.cls#L1836 | Native N12 confirms F-RES-3. |
| RES-B32 | RES-B29 | B | With -recurse, it does the same for each module this one needs. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1834; ipm/src/cls/IPM/Main.cls#L193 | |
| RES-B33 | RES-B29 | B | A class parameter named by -DDeploy.Parameter decides for that class. It wins over the manifest. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1946 | E.g. HSDEPLOY; applies to classes that define the parameter. |
| RES-B34 | RES-0 | B | IPM calls a hook before or after its phase, as the manifest says. | I; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L9; T:Test.PM.Integration.Invoke | ipm/src/cls/IPM/Storage/InvokeReference.cls#L54; ipm/src/cls/IPM/Storage/Module.cls#L602; ipm/src/cls/IPM/Storage/Module.cls#L661 | Default `When="After"`. |
| RES-B35 | RES-B34 | B | A hook with no phase runs in Configure. Activate runs Configure. | I | ipm/src/cls/IPM/Storage/InvokeReference.cls#L12; ipm/src/cls/IPM/Lifecycle/Module.cls#L102 | Configure runs nested inside Activate. |
| RES-B36 | RES-B34 | B | If the hook method returns a status, IPM checks it. A bad status fails the phase. | I; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L23; T:Test.PM.Integration.Invoke | ipm/src/cls/IPM/Storage/InvokeReference.cls#L84 | Decided from the declared return type. |
| RES-B37 | RES-B34 | B | If the hook method has no status return type, IPM ignores what it returns. | I; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L9; T:Test.PM.Integration.Invoke | ipm/src/cls/IPM/Storage/InvokeReference.cls#L87 | |
| RES-B38 | RES-B34 | B | CheckStatus 0 turns the check off. CheckStatus 1 treats any return as a status. | I; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L9; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L23; T:Test.PM.Integration.Invoke | ipm/src/cls/IPM/Storage/InvokeReference.cls#L18; ipm/src/cls/IPM/Storage/InvokeReference.cls#L85 | `CheckStatus` is deprecated. |
| RES-B39 | RES-B34 | B | An error thrown by a hook always fails the phase. | I; O:nc-res-N15-hook-throw-and-badname | ipm/src/cls/IPM/Storage/InvokeReference.cls#L90 | Even with CheckStatus 0. Native N15: a hook that throws with CheckStatus 0 ran once and the load failed with its error; no module record left. |
| RES-B40 | RES-B34 | B | Hook arguments may use names like ${ipmDir}. IPM fills them in before the call. | I; X:tests/integration_tests/Test/PM/Integration/Invoke.cls#L46; T:Test.PM.Integration.Invoke | ipm/src/cls/IPM/Storage/InvokeReference.cls#L33 | Two passes; custom `<Default>` parameters too. |
| RES-B41 | RES-B34 | B | IPM does not check a hook's class or method when it reads the manifest. A bad name fails only when the hook runs. | I; O:nc-res-N15-hook-throw-and-badname | ipm/src/cls/IPM/Storage/InvokeReference.cls#L8; ipm/src/cls/IPM/Storage/InvokeReference.cls#L84 | No validation path found for `Class`/`Method`; inferred. Native N15: a manifest naming a missing method loads; the error comes only when that phase runs. |
| RES-B42 | RES-0 | B | A hook with a custom phase runs only when an operator asks for that phase by name. | I; X:tests/integration_tests/Test/PM/Integration/CustomPhase.cls#L14; X:tests/integration_tests/Test/PM/Integration/CustomPhase.cls#L21; X:tests/integration_tests/Test/PM/Integration/CustomPhase.cls#L28; O:nc-res-N15-hook-throw-and-badname; T:Test.PM.Integration.CustomPhase | ipm/src/cls/IPM/Storage/Module.cls#L413; ipm/src/cls/IPM/Storage/Module.cls#L612 | Not on load, not on uninstall. |
| RES-B43 | RES-B42 | B | A custom phase runs alone. IPM adds no earlier phases to it. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L139 | |
| RES-B44 | RES-B42 | B | IPM matches phase names without regard to case. | I; O:nc-res-N15-hook-throw-and-badname | ipm/src/cls/IPM/Storage/Module.cls#L424; ipm/src/cls/IPM/Storage/Module.cls#L552; ipm/src/cls/IPM/Main.cls#L2842 | Native N15: `ncphase` reaches the hook declared for `NcPhase`. |
| RES-B45 | RES-0 | B | A module action runs the named phases and each phase they need first. | I | ipm/src/cls/IPM/Main.cls#L2830; ipm/src/cls/IPM/Lifecycle/Base.cls#L165 | E.g. test runs Initialize, Reload, Validate, Compile, Activate, Test. |
| RES-B46 | RES-B45 | B | The -only flag runs just the named phases. | I | ipm/src/cls/IPM/Main.cls#L2835 | |
| RES-B47 | RES-B45 | B | A module action may name the module by its external name. | I; X:tests/integration_tests/Test/PM/Integration/ExternalNameModuleAction.cls#L20; T:Test.PM.Integration.ExternalNameModuleAction | ipm/src/cls/IPM/Storage/Module.cls#L457 | |
| RES-B48 | RES-B45 | B | A phase name that is not built in and not custom fails with an error. | I; O:nc-res-N13-register | ipm/src/cls/IPM/Lifecycle/Base.cls#L64 | `<METHOD DOES NOT EXIST>` via `%DispatchMethod`. |
| RES-B49 | RES-B45 | B | The register phase saves the package file in the module cache of this namespace. | N; F:F-RES-2 | ipm/src/cls/IPM/Main.cls#L53; ipm/src/cls/IPM/DataType/PhaseName.cls#L13 | Documented in help, absent in code. Native N13 confirms F-RES-2 for a lone `register`; the help example `clean register` returns OK, runs Clean through Package and no Register step. |
| RES-B50 | RES-B45 | B | A module name with no phases does nothing if the module exists. If not, IPM shows help. | I | ipm/src/cls/IPM/Main.cls#L2849 | TODO at L2851: list phases. |
| RES-B51 | RES-0 | B | A file copy runs at the start of Activate. | I; X:tests/integration_tests/Test/PM/Integration/FileCopy.cls#L34; T:Test.PM.Integration.FileCopy | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L51 | |
| RES-B52 | RES-B51 | B | With Defer set, it runs at the end of Activate. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L78 | |
| RES-B53 | RES-B51 | B | Copying a folder first clears the target folder. Overlay turns this off. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L189 | |
| RES-B54 | RES-B53 | B | IPM never empties a protected IRIS or IPM folder. It warns and copies into it. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L190 | |
| RES-B55 | RES-B51 | B | IPM refuses a folder copy whose target is inside the source, holds the source, or links to it. | I; X:tests/integration_tests/Test/PM/Integration/CopyDirOverlap.cls#L36; X:tests/integration_tests/Test/PM/Integration/CopyDirOverlap.cls#L43; X:tests/integration_tests/Test/PM/Integration/CopyDirOverlap.cls#L50; T:Test.PM.Integration.CopyDirOverlap | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L194 | Checked in `%IPM.Utils.File.CopyDir`; the source stays intact. |
| RES-B56 | RES-B51 | B | A source path starts at the module root unless it begins with a ${ } name. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L143 | |
| RES-B57 | RES-B51 | B | A missing source file fails the copy. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L179 | |
| RES-B58 | RES-B51 | B | A target that ends in a slash is a folder. The file keeps its own name there. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L133 | Otherwise the target is the full file name. |
| RES-B59 | RES-B51 | B | Packaging copies the source of a file copy into the package file. | I | ipm/src/cls/IPM/ResourceProcessor/FileCopy.cls#L202 | |
| RES-B60 | RES-0 | B | IPM installs each wheel with pip in the Initialize phase. | I; X:tests/integration_tests/Test/PM/Integration/ProcessPythonWheel.cls#L113; O:nc-res-N14-bypass-py-deps | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L16 | |
| RES-B61 | RES-B60 | B | A missing wheel file fails the load. For IPM itself, it only warns. | I | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L41; ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L56 | |
| RES-B62 | RES-B60 | B | Wheels go into one Python folder that the whole server shares. | I | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L49 | `<mgr>/python`. |
| RES-B63 | RES-B60 | B | Each action that runs Initialize installs the wheels again, such as compile or test. | I; O:nc-res-N14-bypass-py-deps | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L29; ipm/src/cls/IPM/Lifecycle/Base.cls#L174 | No command check, unlike requirements.txt (Base.cls#L759). Native N14: `lune-wheel compile -v` installs the wheel again. |
| RES-B64 | RES-B60 | B | Removing the module leaves its wheels in place. | I | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L21 | TODO in code: shared wheels. |
| RES-B65 | RES-B60 | B | Packaging copies each wheel file into the package file. | I; O!:nc-suite-tar-name-100-truncated; F:F-RES-7 | ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L68 | O! (refutes): a file whose name is 100 characters or longer is not extracted from the package file (F-RES-7). |
| RES-B66 | RES-0 | B | On install, load or update, IPM runs pip on the requirements.txt of the module. | I; O:nc-res-N14-bypass-py-deps | ipm/src/cls/IPM/Lifecycle/Base.cls#L689; ipm/src/cls/IPM/Lifecycle/Base.cls#L753 | Reload phase; `--only-binary=:all:` and the IRIS Python version. |
| RES-B67 | RES-B66 | B | The -bypass-py-deps flag skips requirements.txt. Wheels still install. | I; X:tests/integration_tests/Test/PM/Integration/ProcessPythonWheel.cls#L175; O:nc-res-N14-bypass-py-deps | ipm/src/cls/IPM/Lifecycle/Base.cls#L763; ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L29 | Help says "Skip installing python dependencies"; the test pins the narrower meaning. Native N14: "Skipping installation of Python dependencies because -bypass-py-deps is set." while the wheel installs. |
| RES-B68 | RES-B66 | B | requirements.txt runs after the wheels, so its versions win. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L689 | The online test for this is skipped (ProcessPythonWheel.cls#L143), so not X. |
| RES-B69 | RES-B66 | B | If pip fails, the load fails. | I; X:tests/integration_tests/Test/PM/Integration/ProcessPythonWheel.cls#L187; O:nc-res-N14-bypass-py-deps | ipm/src/cls/IPM/Lifecycle/Base.cls#L805 | Native N14: without the flag the offline requirements.txt fails and so does the load. |
| RES-B70 | RES-B66 | B | The -extra-pip-flags flag adds flags to each pip call. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L769; ipm/src/cls/IPM/ResourceProcessor/PythonWheel.cls#L51 | Per-wheel `ExtraPipFlags` too. |
| RES-B71 | RES-B66 | B | With -export-python-deps, packaging gets wheels for requirements.txt. It puts them in the package file. | I; O!:nc-suite-tar-name-100-truncated; F:F-RES-7 | ipm/src/cls/IPM/Lifecycle/Base.cls#L1350; ipm/src/cls/IPM/Lifecycle/Base.cls#L786 | Changelog #720, #1024. O! (refutes): a file whose name is 100 characters or longer is not extracted from the package file (F-RES-7). |
| RES-B72 | RES-0 | B | The exec command runs code that the operator types. | I | ipm/src/cls/IPM/Main.cls#L2433 | Alias `cos`. Arbitrary code; no checks. |
| RES-B73 | RES-B72 | B | A namespace change made inside exec ends when exec ends. | I; O:nc-res-N16-exec-namespace-v2 | ipm/src/cls/IPM/Main.cls#L2435; ipm/CHANGELOG.md#L142; PR #996 | PR #996 intends isolation (fixes issue #995); the help example (Main.cls#L515) and the wiki example are stale (F-RES-6). Native N16: inside the command `$namespace` is %SYS; after it the caller is still in USER. Intent: DEC-RES-1 resolved (evidence), decisions/research/nsp.md. |
| RES-B84 | RES-B72 | B | The help example for exec does what the help says it does. | F:F-RES-6; O!:nc-res-N16-exec-namespace-v2 | ipm/src/cls/IPM/Main.cls#L515; ipm/src/cls/IPM/Main.cls#L2435 | Added for F-RES-6 (review finding 14). The help example says it sets the namespace to MYAPP; the code ends the switch with the command (RES-B73). Isolation is intended (PR #996). The example is stale; the wiki example too. Intent: DEC-RES-1 resolved (evidence), decisions/research/nsp.md. Native N16: inside the command $namespace is %SYS, after it the caller is in USER. |
| RES-B74 | RES-0 | B | run-from-file runs the commands of a command file in order. | I; O:nc-res-N17-run-from-file-v2 | ipm/src/cls/IPM/Main.cls#L1972 | Each runs through `Shell`. |
| RES-B75 | RES-B74 | B | It stops at the first command that fails. | I; O:nc-res-N17-run-from-file-v2 | ipm/src/cls/IPM/Main.cls#L2030 | Not stated in help. Native N17: of 3 lines, line 1 ran, line 2 failed, line 3 did not run. |
| RES-B76 | RES-B74 | B | A command file must end in .txt or .json. | I; O:nc-res-N17-run-from-file-v2 | ipm/src/cls/IPM/Main.cls#L1986; ipm/src/cls/IPM/Main.cls#L2017 | Help example uses `.text`, which fails; see F-RES-5. |
| RES-B77 | RES-B74 | B | A .json command file holds one object. Its commands list holds the commands. | I; O:nc-res-N17-run-from-file-v2 | ipm/src/cls/IPM/Main.cls#L2003; ipm/src/cls/IPM/Main.cls#L2006 | Help text says a single array; help example and code use `{"commands": [...]}`. Native N17: a root array is refused ("Expected JSON object at root level"); a `commands` object runs. Help example, wiki example and code (since the 2021 import) agree; the 'single array' sentence is the outlier (F-RES-5). Intent: DEC-RES-2 resolved (evidence), decisions/research/nsp.md. |
| RES-B85 | RES-0 | B | A verify run does not load resources scoped to test. | I; P:res_witness_verify_skips_test_scoped | ipm/src/cls/IPM/Lifecycle/Base.cls#L177 | The verify phase list (Initialize..Activate, Package, Verify) has no Test phase, so IsInScope is false for test-scoped resources. Intent not documented. Premises: none (scopes.rs). |
| RES-B83 | RES-B74 | B | The help for run-from-file shows a command file that it can run. | F:F-RES-5; O!:nc-res-N17-run-from-file-v2 | ipm/src/cls/IPM/Main.cls#L352; ipm/src/cls/IPM/Main.cls#L365 | Added for F-RES-5 (review finding 14). The help example names a `.text` file and the help text says the file holds a single array; the code refuses both (RES-B76, RES-B77; format is the `commands` object, DEC-RES-2 resolved). Native N17: "Unsupported file extension: text" and "Expected JSON object at root level". |
| RES-B78 | RES-B74 | B | With -dry-run, IPM prints the commands and runs none. | I | ipm/src/cls/IPM/Main.cls#L2020 | |
| RES-B79 | RES-0 | B | generate writes a new module.xml from the answers the operator gives. | I | ipm/src/cls/IPM/Main.cls#L1420 | Interactive. HEAD commit #1324 reworked it. |
| RES-B80 | RES-B79 | B | If a module.xml is there, generate renames it first. | I | ipm/src/cls/IPM/Storage/ModuleTemplate.cls#L661 | `module_<timestamp>.xml`. |
| RES-B81 | RES-B79 | B | Classes in the tests folder become unit tests, not source to ship. | I; X:tests/integration_tests/Test/PM/Integration/Generate.cls#L51; X:tests/integration_tests/Test/PM/Integration/Generate.cls#L61; T:Test.PM.Integration.Generate | ipm/src/cls/IPM/Storage/ModuleTemplate.cls#L385 | |
| RES-B82 | RES-B79 | B | With -template, generate writes a sample module.xml and asks nothing. | I; X:tests/integration_tests/Test/PM/Integration/Generate.cls#L129; T:Test.PM.Integration.Generate | ipm/src/cls/IPM/Main.cls#L1431 | |

## States
| ID | State |
|---|---|
| S1 | The module part is a resource with no scope and no Deploy flag. |
| S2 | The module part is a resource scoped to test. |
| S3 | The module part is a deployed resource. |
| S4 | The module part is a file copy. |
| S5 | The module part is a wheel. |
| S6 | The module part is a hook on a built-in phase. |
| S7 | The module part is a hook on a custom phase. |

## Commands
| ID | Command |
|---|---|
| K1 | `load <dir>` (not developer mode) |
| K2 | `load -dev <dir>` |
| K3 | `<module> test` |
| K4 | `package <module>` |
| K5 | `package <module> -include-tests` |
| K6 | `makedeployed <module>` |
| K7 | `<module> <custom phase>` |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| RES-C1 | S1 | K1 | RES-B1 | |
| RES-C2 | S1 | K2 | RES-B1 | |
| RES-C3 | S1 | K3 | RES-B1, RES-B45 | |
| RES-C4 | S1 | K4 | RES-B12 | |
| RES-C5 | S1 | K5 | RES-B12 | |
| RES-C6 | S1 | K6 | RES-B30 | |
| RES-C7 | S1 | K7 | RES-B43 | |
| RES-C8 | S2 | K1 | RES-B2 | |
| RES-C9 | S2 | K2 | RES-B3 | |
| RES-C10 | S2 | K3 | RES-B4, RES-B5 | |
| RES-C11 | S2 | K4 | RES-B13 | |
| RES-C12 | S2 | K5 | RES-B14 | |
| RES-C13 | S2 | K6 | RES-B30 | |
| RES-C14 | S2 | K7 | RES-B43 | |
| RES-C15 | S3 | K1 | RES-B21, RES-B26 | |
| RES-C16 | S3 | K2 | RES-B21 | |
| RES-C17 | S3 | K3 | RES-B21, RES-B45 | |
| RES-C18 | S3 | K4 | RES-B22, RES-B23, RES-B24, RES-B25 |  |
| RES-C19 | S3 | K5 | RES-B22, RES-B14 | |
| RES-C20 | S3 | K6 | RES-B29, RES-B31 | |
| RES-C21 | S3 | K7 | RES-B43 | |
| RES-C22 | S4 | K1 | RES-B51, RES-B53, RES-B56 | |
| RES-C23 | S4 | K2 | RES-B51 | |
| RES-C24 | S4 | K3 | RES-B51, RES-B45 | |
| RES-C25 | S4 | K4 | RES-B59 | |
| RES-C26 | S4 | K5 | RES-B59 | |
| RES-C27 | S4 | K6 | RES-B30 | |
| RES-C28 | S4 | K7 | RES-B43 | |
| RES-C29 | S5 | K1 | RES-B60, RES-B66 | |
| RES-C30 | S5 | K2 | RES-B60 | |
| RES-C31 | S5 | K3 | RES-B63 | |
| RES-C32 | S5 | K4 | RES-B63, RES-B65 | |
| RES-C33 | S5 | K5 | RES-B65 | |
| RES-C34 | S5 | K6 | RES-B30 | |
| RES-C35 | S5 | K7 | RES-B43 | |
| RES-C36 | S6 | K1 | RES-B34, RES-B36 | |
| RES-C37 | S6 | K2 | RES-B34 | |
| RES-C38 | S6 | K3 | RES-B34, RES-B45 | |
| RES-C39 | S6 | K4 | RES-B34 | |
| RES-C40 | S6 | K5 | RES-B34 | |
| RES-C41 | S6 | K6 | RES-B34 | |
| RES-C42 | S6 | K7 | RES-B43 | |
| RES-C43 | S7 | K1 | RES-B42 | |
| RES-C44 | S7 | K2 | RES-B42 | |
| RES-C45 | S7 | K3 | RES-B42 | |
| RES-C46 | S7 | K4 | RES-B42 | |
| RES-C47 | S7 | K5 | RES-B42 | |
| RES-C48 | S7 | K6 | RES-B42 | |
| RES-C49 | S7 | K7 | RES-B42, RES-B43 | |
