# Queue (master-maintained)

## Census enumeration

| Chunk | Pages | Blocks / units | Status |
| --- | --- | --- | --- |
| A | features/developer-tools, manifest, resources, packaging-deployed-code, web-applications | 58 / 240 | iteration 68 |
| B | features/load, history; concepts/transactions-and-dev-mode | 56 / 229 | pending |
| C | architecture/lifecycle, package-sources; concepts/data-policy, testing-policy, versions, download-security, repositories | 68 / 220 | pending |
| D | features/artifact-download, build-and-publish, dependencies, inspection, install, package-commands, uninstall, update; operations/* | 83 / 286 | pending |

## Leads (to reconcile against the census before assignment)

1. `generate` manifest grouping (`Main.cls::GenerateModuleXML`, `Storage/ModuleTemplate.cls::{GroupClasses,AddPackages,ReadResources,ProcessResources}`): the emitted group (package) name and individual-resource leftovers. Iteration53 `generator.rs::grouping_neighbors` checks only a Boolean shared-membership outcome. Owners: `devtools.authoring.generate`, `devtools.generate.resources`.
2. Residual unlinked members from the earlier run ([residual-final.json](../consolidated/common30/residual-final.json), [convergence record](../consolidated/CONVERGENCE-20261116.md)).
