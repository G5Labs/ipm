# Shipping compiled code

Module authors can ship eligible resources without exposing their source. The deployment choice governs both packaging and the later load of the compiled project.

## Definitions

A deployed resource ships in compiled form. A Studio project can carry deployed resources in an archive.

## Choosing which source to omit

<a id="module.deployed"></a>
[I · module.deployed]

Deployment ships eligible code without its source. An explicit resource `Deploy` value takes precedence over the lifecycle default, unless a named deployment parameter exists on that class: then only parameter value `1` enables deployment, even when the resource says true.

- The normal Module lifecycle defaults to false; DeployedModule defaults to true. The class parameter override applies only to an existing class with that parameter.
- The default comes from the lifecycle class's `DEPLOYBYDEFAULT` parameter and is used only when the resource has no `Deploy` attribute. A custom lifecycle inherits its parent's default unless it sets its own.
- Built-in compiled-only deployment applies to classes and MAC/INT routines. Include files and other types keep their source form unless their own processor handles deployment. IPM picks the package built for the current IRIS version and platform and installs code into each resource's mapped database.
- Loading a deployed project has separate [eligibility and retry rules](#module.deployed.project-load).

<a id="build.makedeployed"></a>
[I · build.makedeployed]

`makedeployed <module>` turns eligible resources selected by the [deployment rules](#module.deployed) into compiled-only code. A class deployment parameter can override `Deploy="true"`. `-recurse` also considers the module's dependencies.

## Loading a compiled project

<a id="module.deployed.project-load"></a>
[D · module.deployed.project-load]

A deployed resource loads from the module's Studio project only when its type is deployable or its name is a path, the project file exists, and the resource is neither generated nor preloaded. Otherwise the ordinary source path remains in use.

- IPM sets the object's Deployed flag before attempting its save. If save fails, the database row can remain false while the object says true, and the installer is not called. After save succeeds, a corrupt-project install error can leave the stored flag true; neither flag proves installation.
- A process-local marker is set only after successful project installation. Other resources using that path then skip the installer. Failure leaves no success marker, so a later attempt may retry. Provider writes before failure are not assumed atomic. A fresh duplicate resource reference can cause the earlier save to fail; [reusing its stored identity](../concepts/data-policy.g5.md#module.resource.identity) is a different input.
- Packaging's project check has a different purpose: it removes missing project items and can return success after doing so. Check success alone does not establish that every requested item remains in the project.

## Reporting a failed project export

<a id="build.package.deployed-status"></a>
[D · build.package.deployed-status]

As built, the document exporter overwrites the deployed-item helper's returned error with success before deciding whether to export source. A successful source fallback can therefore hide that earlier error; a source-export error still fails the operation.

- The actual inherited exporter was exercised with a controlled failing helper override and a real successful source export. This confirms C:D19's local status suppression, not an ordinary default-helper failure in a complete package command.
- The later Studio project check is an independent reader. It removes missing items and may return success with fewer items, so it is not proof that a discarded item or helper error will be caught. Source files, final project contents and command status must be assessed separately.
- The proposed repair is to preserve the helper status. It has not been applied to the product. An error after a provider's partial write may leave that write; no compensation is inferred.

## Metadata

### module.deployed

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).

Children: [project loading](#module.deployed.project-load). Iteration 3 Verus `resources.rs::deploy` and `deployment_precedence` derive class-parameter override and lifecycle defaults; actual native calls distinguish parameter false, resource true, Module and DeployedModule.

Iteration 4 producer repair: iteration 3 passed the lifecycle default in as a boolean, so swapping the two lifecycles' defaults could not break its proof. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{deploy_by_default,deploy,deploy_neighbors}` now takes the lifecycle class as input; control `lifecycle-defaults-swapped` is rejected, as are `explicit-false-ignored` and `explicit-overrides-class-parameter`. Producer: `Lifecycle/Base.cls` `DEPLOYBYDEFAULT=0` and the resolution at `:1945-1963`; `Lifecycle/DeployedModule.cls` `DEPLOYBYDEFAULT=1`. Native P-Base/P-Module 0 and P-DeployedModule 1 read the compiled parameters.

Sources: `wiki/05`; CHANGELOG 0.10.0 (`#754`), 0.10.4 (`#815`), 0.10.10 (`#1158`), 0.10.11 (`#1242`, `#1279`, `#1125`). Inventory: CAT-34. Tests: `DeployedItems.cls`, `DeployedPackageLoad.cls`, `OrasDeployedPlatformFilter.cls`. `wiki/05` L22-67 is stale about plain loads and removed phases.

State machines: [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### build.makedeployed

- human-provided
- user-facing-behavior
- Expands: [Build](build-and-publish.g5.md#build).

Sources: `Main.cls:189-195`; `wiki/05`; CHANGELOG 0.10.4 (`#815`). Inventory: CAT-34; A2 1.2 row 13.

### module.deployed.project-load

- agent
- Expands: [Deployment](#module.deployed).

Sources: `ResourceProcessor/Default/Document.cls::{OnPhase,LoadFromDeployedProject}`; `Lifecycle/Base.cls::%Export`; native `%Studio.Project.Check` contract and item counts. Verus `resources.rs::{load_project,check_project,load_project_boundaries,project_check_prunes_missing}`. Native receipts distinguish reused-reference installer error (object=true, SQL=true, marker absent) from fresh-duplicate save refusal (object=true, SQL=false, installer not reached). Success-marker installation is source/formal only. No proof of installer atomicity.

### build.package.deployed-status

- agent
- Expands: [Packaging](build-and-publish.g5.md#build.package).

Sources: `ResourceProcessor/Default/Document.cls::{OnExportItem,OnExportDeployedItem}`; `Storage/Module.cls::AddItemToProject`; `Lifecycle/Base.cls::{ExportSingleModule,%Export}`; native `%Studio.Project.Check` documentation and observations. Verus `resources.rs::{export_item,final_export,check_project,overwritten_error_and_final_reader,project_check_prunes_missing}`. The final checker status remains an explicit provider result; only its missing-item pruning is modeled. [C:D19](../../Verus_verification/bugs_issues.md) and [repair proposal C:S13](../../Verus_verification/design_simplification.md) retain that limit.

Findings: [C:D19 — partial reproduced](../../Verus_verification/bugs_issues.md#c-d19). The linked entry qualifies the caller, reader and providers.

Findings: [C:S13 — scoped repair comparison](../../Verus_verification/design_simplification.md#c-s13).
